"""Fixture generator for FO76 conversion tests.

Run manually with `uv run python bacup/py_bacup_lib/python/bacup_lib/tests/fixtures/fo76/_generate.py`
to (re)create the synthesized DDS textures used by the FO76->FO4 conversion
test suite. Output files are committed to the repo so the generator does NOT
need to be runnable as part of normal test execution.

Each generated DDS is an 8x8 uncompressed BGRA8 (DXGI_FORMAT_B8G8R8A8_UNORM)
file with hand-crafted header bytes. We pick uncompressed/no-mipmaps to keep
parsing trivial in tests (no DXT decompression dependency) and to make the
known channel values byte-exact.

Channel values per role (must stay in sync with textures/README.md):

  color_d.dds   : R=128 G= 64 B= 32 A=255   (FO76 albedo with bright RGB)
  normal_n.dds  : R=128 G=128 B=255 A=255   (FO76 tangent-space normal, +Z)
  rough_r.dds   : R=200 G= 50 B=255 A=  0   (R=rough, G=metal, B=AO, A=unused)
"""

from __future__ import annotations

import struct
from pathlib import Path

# DDS pixel format flags
DDPF_ALPHAPIXELS = 0x00000001
DDPF_RGB = 0x00000040

# Header flags
DDSD_CAPS = 0x00000001
DDSD_HEIGHT = 0x00000002
DDSD_WIDTH = 0x00000004
DDSD_PIXELFORMAT = 0x00001000
DDSD_PITCH = 0x00000008
DDSCAPS_TEXTURE = 0x00001000

WIDTH = 8
HEIGHT = 8


def _build_uncompressed_bgra(rgba: tuple[int, int, int, int]) -> bytes:
    """Build a complete uncompressed 8x8 BGRA DDS file."""
    r, g, b, a = rgba
    pitch = WIDTH * 4

    # Pixel data: 8x8 BGRA
    pixel = bytes((b, g, r, a))
    pixels = pixel * (WIDTH * HEIGHT)

    # DDS_PIXELFORMAT (32 bytes)
    # struct DDS_PIXELFORMAT {
    #   DWORD dwSize;             // 32
    #   DWORD dwFlags;            // RGB | ALPHAPIXELS
    #   DWORD dwFourCC;           // 0
    #   DWORD dwRGBBitCount;      // 32
    #   DWORD dwRBitMask;         // 0x00FF0000
    #   DWORD dwGBitMask;         // 0x0000FF00
    #   DWORD dwBBitMask;         // 0x000000FF
    #   DWORD dwABitMask;         // 0xFF000000
    # }
    pixel_format = struct.pack(
        "<8I",
        32,
        DDPF_RGB | DDPF_ALPHAPIXELS,
        0,
        32,
        0x00FF0000,
        0x0000FF00,
        0x000000FF,
        0xFF000000,
    )

    # DDS_HEADER (124 bytes after the 4-byte magic)
    header = struct.pack(
        "<7I44x",
        124,                                      # dwSize
        DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH      # dwFlags
        | DDSD_PIXELFORMAT | DDSD_PITCH,
        HEIGHT,                                   # dwHeight
        WIDTH,                                    # dwWidth
        pitch,                                    # dwPitchOrLinearSize
        0,                                        # dwDepth
        0,                                        # dwMipMapCount
    )
    caps = struct.pack("<4I", DDSCAPS_TEXTURE, 0, 0, 0)
    reserved2 = b"\x00" * 4

    assert len(header) + len(pixel_format) + len(caps) + len(reserved2) == 124, (
        len(header) + len(pixel_format) + len(caps) + len(reserved2)
    )

    return b"DDS " + header + pixel_format + caps + reserved2 + pixels


FIXTURES = {
    "color_d.dds":  (128,  64,  32, 255),
    "normal_n.dds": (128, 128, 255, 255),
    "rough_r.dds":  (200,  50, 255,   0),
}


def main() -> None:
    out_dir = Path(__file__).parent / "textures"
    out_dir.mkdir(parents=True, exist_ok=True)
    for name, rgba in FIXTURES.items():
        data = _build_uncompressed_bgra(rgba)
        (out_dir / name).write_bytes(data)
        print(f"wrote {out_dir / name} ({len(data)} bytes, RGBA={rgba})")


if __name__ == "__main__":
    main()
