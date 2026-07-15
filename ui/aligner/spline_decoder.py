"""Decode hkaSplineCompressedAnimation to extract frame 0 bone transforms.

For frame 0 of a clamped B-spline, the curve passes through the first
control point. This means we can skip the full De Boor evaluation and
just read the first control point of each dynamic track.

Based on PredatorCZ/HavokLib spline decompressor (GPLv3).
"""
from __future__ import annotations

import logging
import math
import struct
from io import BytesIO

import numpy as np

_log = logging.getLogger("aligner.spline")

# Position/Scale quantization types
QT_8BIT = 0
QT_16BIT = 1

# Rotation quantization types (add 2 to the 4-bit field value)
ROTQT_32BIT = 2
ROTQT_40BIT = 3
ROTQT_48BIT = 4
ROTQT_THREECOMP16 = 5
ROTQT_UNCOMPRESSED = 6

# Rotation quantization sizes in bytes
_ROT_SIZES = {
    ROTQT_32BIT: 4,
    ROTQT_40BIT: 5,
    ROTQT_48BIT: 6,
    ROTQT_THREECOMP16: 8,
    ROTQT_UNCOMPRESSED: 16,
}

# Sub-track flags
_STATIC_X = 1
_STATIC_Y = 2
_STATIC_Z = 4
_STATIC_W = 8
_SPLINE_X = 16
_SPLINE_Y = 32
_SPLINE_Z = 64
_SPLINE_W = 128


def _align(pos: int, alignment: int) -> int:
    """Round position up to alignment boundary."""
    return (pos + alignment - 1) & ~(alignment - 1)


def _read_u8(data: bytes, pos: int) -> tuple[int, int]:
    return data[pos], pos + 1


def _read_u16(data: bytes, pos: int) -> tuple[int, int]:
    return struct.unpack_from("<H", data, pos)[0], pos + 2


def _read_f32(data: bytes, pos: int) -> tuple[float, int]:
    return struct.unpack_from("<f", data, pos)[0], pos + 4


def _count_set_bits(flags: int, mask: int) -> int:
    """Count number of set bits in flags masked by mask."""
    return bin(flags & mask).count("1")


def _dequantize_8(raw: int, min_val: float, max_val: float) -> float:
    return min_val + (max_val - min_val) * (raw / 255.0)


def _dequantize_16(raw: int, min_val: float, max_val: float) -> float:
    return min_val + (max_val - min_val) * (raw / 65535.0)


def _smallest_three_result(a: float, b: float, c: float, d: float, which: int) -> list[float]:
    """Assign decoded components to quaternion XYZW based on which component was dropped.

    a, b, c are the three decoded components; d is the computed dropped component.
    which: 0=X dropped, 1=Y dropped, 2=Z dropped, 3=W dropped.
    """
    if which == 0:
        return [d, a, b, c]
    elif which == 1:
        return [a, d, b, c]
    elif which == 2:
        return [a, b, d, c]
    else:
        return [a, b, c, d]


def _decode_quat_48bit(data: bytes, pos: int) -> tuple[list[float], int]:
    """Decode 48-bit (6 byte) quaternion — three int16 values.

    Layout per HavokLib: three int16 (X, Y, Z), each with 15 data bits + 1 MSB metadata.
    - comp = int16 & 0x7FFF (lower 15 bits)
    - which = (Y_MSB << 1) | X_MSB
    - sign = Z_MSB
    """
    ix, iy, iz = struct.unpack_from("<3H", data, pos)

    comp_a = ix & 0x7FFF
    comp_b = iy & 0x7FFF
    comp_c = iz & 0x7FFF
    which = ((iy >> 14) & 2) | ((ix >> 15) & 1)
    sign = (iz >> 15) & 1

    half = 0x3FFF  # 16383
    frac = 0.000043161  # sqrt(2) / 32767
    a = (comp_a - half) * frac
    b = (comp_b - half) * frac
    c = (comp_c - half) * frac

    sum_sq = a * a + b * b + c * c
    d = math.sqrt(max(0.0, 1.0 - sum_sq))
    if sign:
        d = -d

    return _smallest_three_result(a, b, c, d, which), pos + 6


def _decode_quat_40bit(data: bytes, pos: int) -> tuple[list[float], int]:
    """Decode 40-bit (5 byte) quaternion — contiguous bitfield, 12-bit components.

    Layout per HavokLib: 3 × 12-bit components + 2-bit which + 1-bit sign.
    """
    raw = int.from_bytes(data[pos:pos + 5], "little")

    mask = 0xFFF  # 12-bit
    comp_a = raw & mask
    comp_b = (raw >> 12) & mask
    comp_c = (raw >> 24) & mask
    which = (raw >> 36) & 0x3
    sign = (raw >> 38) != 0

    half = mask >> 1  # 2047
    frac = 0.000345436  # sqrt(2) / 4095
    a = (comp_a - half) * frac
    b = (comp_b - half) * frac
    c = (comp_c - half) * frac

    sum_sq = a * a + b * b + c * c
    d = math.sqrt(max(0.0, 1.0 - sum_sq))
    if sign:
        d = -d

    return _smallest_three_result(a, b, c, d, which), pos + 5


def _decode_quat_32bit(data: bytes, pos: int) -> tuple[list[float], int]:
    """Decode 32-bit quaternion — contiguous bitfield, 10-bit components.

    Layout per HavokLib: 3 × 10-bit components + 2-bit which + 1-bit sign.
    Note: which bits overlap with top 2 bits of comp_c (by design).
    """
    val, = struct.unpack_from("<I", data, pos)

    mask = 0x3FF  # 10-bit
    comp_a = val & mask
    comp_b = (val >> 10) & mask
    comp_c = (val >> 20) & mask
    which = (val >> 28) & 0x3
    sign = (val >> 30) != 0

    half = mask >> 1  # 511
    frac = 0.001381067  # sqrt(2) / 1023
    a = (comp_a - half) * frac
    b = (comp_b - half) * frac
    c = (comp_c - half) * frac

    sum_sq = a * a + b * b + c * c
    d = math.sqrt(max(0.0, 1.0 - sum_sq))
    if sign:
        d = -d

    return _smallest_three_result(a, b, c, d, which), pos + 4


def _decode_quat_uncompressed(data: bytes, pos: int) -> tuple[list[float], int]:
    """Read uncompressed float4 quaternion (XYZW)."""
    x, y, z, w = struct.unpack_from("<4f", data, pos)
    return [x, y, z, w], pos + 16


def _decode_quat_threecomp16(data: bytes, pos: int) -> tuple[list[float], int]:
    """Decode 16-bit three-component quaternion (8 bytes = 4 × uint16)."""
    a, b, c, d = struct.unpack_from("<4H", data, pos)
    frac = 1.0 / 32767.0
    x = (a - 32767) * frac
    y = (b - 32767) * frac
    z = (c - 32767) * frac
    sum_sq = x * x + y * y + z * z
    w = math.sqrt(max(0.0, 1.0 - sum_sq))
    if d & 0x8000:
        w = -w
    return [x, y, z, w], pos + 8


def _decode_rotation(data: bytes, pos: int, rot_quant: int) -> tuple[list[float], int]:
    """Decode a single quaternion based on quantization type."""
    if rot_quant == ROTQT_32BIT:
        return _decode_quat_32bit(data, pos)
    elif rot_quant == ROTQT_40BIT:
        return _decode_quat_40bit(data, pos)
    elif rot_quant == ROTQT_48BIT:
        return _decode_quat_48bit(data, pos)
    elif rot_quant == ROTQT_THREECOMP16:
        return _decode_quat_threecomp16(data, pos)
    elif rot_quant == ROTQT_UNCOMPRESSED:
        return _decode_quat_uncompressed(data, pos)
    else:
        raise ValueError(f"Unknown rotation quantization type: {rot_quant}")


def decode_frame0(
    raw_data: bytes,
    num_transform_tracks: int,
    num_float_tracks: int,
    block_offset: int = 0,
) -> list[dict]:
    """Decode frame 0 transforms from spline compressed animation data.

    Args:
        raw_data: The raw `data` byte array from the animation.
        num_transform_tracks: Number of bone tracks (typically 94).
        num_float_tracks: Number of float tracks (typically 0).
        block_offset: Offset to block 0 in data (typically 0).

    Returns:
        List of dicts per bone: {"t": [x,y,z], "q": [x,y,z,w], "s": [x,y,z]}
        None values mean identity (use reference pose).
    """
    data = raw_data
    pos = block_offset

    # --- Read transform masks (4 bytes each) ---
    masks = []
    for _ in range(num_transform_tracks):
        quant_types, pos_flags, rot_flags, scale_flags = struct.unpack_from("4B", data, pos)
        pos += 4

        pos_quant = quant_types & 0x3
        rot_quant = ((quant_types >> 2) & 0xF) + 2
        scale_quant = (quant_types >> 6) & 0x3

        masks.append({
            "pos_quant": pos_quant,
            "rot_quant": rot_quant,
            "scale_quant": scale_quant,
            "pos_flags": pos_flags,
            "rot_flags": rot_flags,
            "scale_flags": scale_flags,
        })

    # Skip float track masks (1 byte each)
    pos += num_float_tracks

    # Align to 4 bytes
    pos = _align(pos, 4)

    # --- Parse per-track data ---
    results = []

    for track_idx in range(num_transform_tracks):
        mask = masks[track_idx]
        t = None  # translation [x,y,z]
        q = None  # rotation [x,y,z,w]
        s = None  # scale [x,y,z]

        # --- Position ---
        pos_flags = mask["pos_flags"]
        pos_quant = mask["pos_quant"]
        has_pos_static = pos_flags & 0x0F
        has_pos_spline = pos_flags & 0xF0

        if has_pos_static or has_pos_spline:
            t = [0.0, 0.0, 0.0]

            if has_pos_spline:
                # Dynamic/spline position
                # Read spline header
                num_items, pos = _read_u16(data, pos)
                degree, pos = _read_u8(data, pos)
                num_knots = num_items + degree + 2
                pos += num_knots  # skip knots
                pos = _align(pos, 4)

                # Read min/max per dynamic component, static values for static components
                num_dynamic = _count_set_bits(pos_flags, 0xF0)
                num_static = _count_set_bits(pos_flags, 0x0F)

                # Mins and maxs for dynamic components
                mins = []
                maxs = []
                for comp in range(3):
                    if pos_flags & (_SPLINE_X << comp):
                        min_v, pos = _read_f32(data, pos)
                        max_v, pos = _read_f32(data, pos)
                        mins.append(min_v)
                        maxs.append(max_v)
                    elif pos_flags & (_STATIC_X << comp):
                        val, pos = _read_f32(data, pos)
                        t[comp] = val
                        mins.append(0)
                        maxs.append(0)
                    else:
                        mins.append(0)
                        maxs.append(0)

                # Read first control point for dynamic components
                # Control points are interleaved: for each point, one value per dynamic component
                dyn_idx = 0
                for comp in range(3):
                    if pos_flags & (_SPLINE_X << comp):
                        if pos_quant == QT_8BIT:
                            raw, pos = _read_u8(data, pos)
                            t[comp] = _dequantize_8(raw, mins[comp], maxs[comp])
                        elif pos_quant == QT_16BIT:
                            raw, pos = _read_u16(data, pos)
                            t[comp] = _dequantize_16(raw, mins[comp], maxs[comp])
                        dyn_idx += 1

                # Skip remaining control points
                num_cps = num_items + 1
                remaining_cps = num_cps - 1
                if remaining_cps > 0:
                    if pos_quant == QT_8BIT:
                        pos += remaining_cps * num_dynamic
                    elif pos_quant == QT_16BIT:
                        pos += remaining_cps * num_dynamic * 2

                pos = _align(pos, 4)

            else:
                # Static-only position (no spline header)
                for comp in range(3):
                    if pos_flags & (_STATIC_X << comp):
                        t[comp], pos = _read_f32(data, pos)

        # --- Rotation ---
        rot_flags = mask["rot_flags"]
        rot_quant = mask["rot_quant"]
        has_rot_static = rot_flags & 0x0F
        has_rot_spline = rot_flags & 0xF0

        if has_rot_static or has_rot_spline:
            if has_rot_spline:
                # Dynamic rotation spline
                num_items, pos = _read_u16(data, pos)
                degree, pos = _read_u8(data, pos)
                num_knots = num_items + degree + 2
                pos += num_knots

                # Alignment depends on rotation quantization
                if rot_quant in (ROTQT_48BIT, ROTQT_THREECOMP16):
                    pos = _align(pos, 2)
                else:
                    pos = _align(pos, 4)

                # Read first control point quaternion
                q, pos = _decode_rotation(data, pos, rot_quant)

                # Skip remaining control points
                num_cps = num_items + 1
                remaining_cps = num_cps - 1
                rot_size = _ROT_SIZES.get(rot_quant, 4)
                pos += remaining_cps * rot_size

                pos = _align(pos, 4)

            else:
                # Static rotation
                if rot_quant in (ROTQT_48BIT, ROTQT_THREECOMP16):
                    pos = _align(pos, 2)
                else:
                    pos = _align(pos, 4)
                q, pos = _decode_rotation(data, pos, rot_quant)
                pos = _align(pos, 4)

        # --- Scale ---
        scale_flags = mask["scale_flags"]
        scale_quant = mask["scale_quant"]
        has_scale_static = scale_flags & 0x0F
        has_scale_spline = scale_flags & 0xF0

        if has_scale_static or has_scale_spline:
            s = [1.0, 1.0, 1.0]

            if has_scale_spline:
                num_items, pos = _read_u16(data, pos)
                degree, pos = _read_u8(data, pos)
                num_knots = num_items + degree + 2
                pos += num_knots
                pos = _align(pos, 4)

                num_dynamic = _count_set_bits(scale_flags, 0xF0)

                mins = []
                maxs = []
                for comp in range(3):
                    if scale_flags & (_SPLINE_X << comp):
                        min_v, pos = _read_f32(data, pos)
                        max_v, pos = _read_f32(data, pos)
                        mins.append(min_v)
                        maxs.append(max_v)
                    elif scale_flags & (_STATIC_X << comp):
                        val, pos = _read_f32(data, pos)
                        s[comp] = val
                        mins.append(0)
                        maxs.append(0)
                    else:
                        mins.append(0)
                        maxs.append(0)

                dyn_idx = 0
                for comp in range(3):
                    if scale_flags & (_SPLINE_X << comp):
                        if scale_quant == QT_8BIT:
                            raw, pos = _read_u8(data, pos)
                            s[comp] = _dequantize_8(raw, mins[comp], maxs[comp])
                        elif scale_quant == QT_16BIT:
                            raw, pos = _read_u16(data, pos)
                            s[comp] = _dequantize_16(raw, mins[comp], maxs[comp])
                        dyn_idx += 1

                num_cps = num_items + 1
                remaining_cps = num_cps - 1
                if remaining_cps > 0:
                    if scale_quant == QT_8BIT:
                        pos += remaining_cps * num_dynamic
                    elif scale_quant == QT_16BIT:
                        pos += remaining_cps * num_dynamic * 2

                pos = _align(pos, 4)

            else:
                for comp in range(3):
                    if scale_flags & (_STATIC_X << comp):
                        s[comp], pos = _read_f32(data, pos)

        results.append({"t": t, "q": q, "s": s})

    return results
