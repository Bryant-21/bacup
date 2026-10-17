"""Write a shippable copy of a linker /PDBSTRIPPED PDB with no local paths.

A stripped PDB keeps public symbols (enough to name crash-log frames) but its DBI
stream still lists every object and library by absolute build path. This rewrites
those names to bare file names and re-lays the MSF file, which also drops the stale
free blocks where old path strings linger. The PDB GUID and age are untouched, so
the copy still matches its DLL.

Usage: uv run python bacup/scripts/clean_pdb.py <stripped.pdb> <out.pdb>
"""

from __future__ import annotations

import re
import struct
import sys
from pathlib import Path, PureWindowsPath

_MSF_MAGIC = b"Microsoft C/C++ MSF 7.00\r\n\x1aDS\x00\x00\x00"
_DBI_STREAM = 3
_DBI_HEADER_SIZE = 64
_MOD_INFO_FIXED_SIZE = 64
_NIL_STREAM = 0xFFFFFFFF
LOCAL_PATH = re.compile(rb"[A-Za-z]:[\\/][\x20-\x7e]")


def _basename(name: bytes) -> bytes:
    return PureWindowsPath(name.decode("latin-1")).name.encode("latin-1") if LOCAL_PATH.match(name) else name


def _align4(data: bytearray) -> None:
    data.extend(b"\0" * (-len(data) % 4))


def read_msf(data: bytes) -> tuple[int, list[bytes | None]]:
    if not data.startswith(_MSF_MAGIC):
        raise ValueError("not an MSF 7.00 PDB")
    block_size, _fpm, _blocks, directory_size, _unknown, block_map = struct.unpack_from("<6I", data, 32)
    count = -(-directory_size // block_size)
    directory_blocks = struct.unpack_from(f"<{count}I", data, block_map * block_size)
    directory = b"".join(data[b * block_size:(b + 1) * block_size] for b in directory_blocks)
    stream_count = struct.unpack_from("<I", directory)[0]
    sizes = struct.unpack_from(f"<{stream_count}I", directory, 4)
    pos = 4 + 4 * stream_count
    streams: list[bytes | None] = []
    for size in sizes:
        if size == _NIL_STREAM:
            streams.append(None)
            continue
        n = -(-size // block_size)
        blocks = struct.unpack_from(f"<{n}I", directory, pos)
        pos += 4 * n
        streams.append(b"".join(data[b * block_size:(b + 1) * block_size] for b in blocks)[:size])
    return block_size, streams


def write_msf(block_size: int, streams: list[bytes | None]) -> bytes:
    blocks: dict[int, bytes] = {}
    next_block = 3

    def allocate(payload: bytes) -> list[int]:
        nonlocal next_block
        used = []
        for start in range(0, len(payload), block_size):
            # Blocks 1 and 2 of every block_size interval belong to the free block maps.
            while next_block % block_size in (1, 2):
                next_block += 1
            blocks[next_block] = payload[start:start + block_size]
            used.append(next_block)
            next_block += 1
        return used

    directory = bytearray(struct.pack("<I", len(streams)))
    directory += b"".join(struct.pack("<I", _NIL_STREAM if s is None else len(s)) for s in streams)
    for stream in streams:
        if stream:
            directory += b"".join(struct.pack("<I", b) for b in allocate(stream))
    directory_blocks = allocate(bytes(directory))
    block_map = allocate(b"".join(struct.pack("<I", b) for b in directory_blocks))[0]

    block_count = next_block
    intervals = -(-block_count // block_size)
    block_count = max(block_count, (intervals - 1) * block_size + 3)
    free_map = bytearray(b"\xff" * (intervals * block_size))
    for used in [0, *blocks, *(i * block_size + k for i in range(intervals) for k in (1, 2))]:
        free_map[used // 8] &= ~(1 << (used % 8)) & 0xFF
    for i in range(intervals):
        blocks[i * block_size + 1] = bytes(free_map[i * block_size:(i + 1) * block_size])
        blocks[i * block_size + 2] = b"\xff" * block_size

    out = bytearray(block_count * block_size)
    header = _MSF_MAGIC + struct.pack("<6I", block_size, 1, block_count, len(directory), 0, block_map)
    out[:len(header)] = header
    for index, payload in blocks.items():
        out[index * block_size:index * block_size + len(payload)] = payload
    return bytes(out)


def _clean_mod_info(mod_info: bytes) -> bytes:
    out = bytearray()
    pos = 0
    while pos < len(mod_info):
        fixed = mod_info[pos:pos + _MOD_INFO_FIXED_SIZE]
        pos += _MOD_INFO_FIXED_SIZE
        module_end = mod_info.index(b"\0", pos)
        obj_end = mod_info.index(b"\0", module_end + 1)
        out += fixed
        out += _basename(mod_info[pos:module_end]) + b"\0"
        out += _basename(mod_info[module_end + 1:obj_end]) + b"\0"
        _align4(out)
        pos = obj_end + 1
        pos += -pos % 4
    return bytes(out)


def _clean_source_info(source_info: bytes) -> bytes:
    if not source_info:
        return source_info
    modules = struct.unpack_from("<H", source_info)[0]
    counts = struct.unpack_from(f"<{modules}H", source_info, 4 + 2 * modules)
    files = sum(counts)
    offsets_at = 4 + 4 * modules
    names_at = offsets_at + 4 * files
    offsets = struct.unpack_from(f"<{files}I", source_info, offsets_at)
    names = bytearray()
    new_offsets: dict[bytes, int] = {}
    rewritten = []
    for offset in offsets:
        start = names_at + offset
        name = _basename(source_info[start:source_info.index(b"\0", start)])
        if name not in new_offsets:
            new_offsets[name] = len(names)
            names += name + b"\0"
        rewritten.append(new_offsets[name])
    out = bytearray(source_info[:offsets_at])
    out += b"".join(struct.pack("<I", o) for o in rewritten)
    out += names
    _align4(out)
    return bytes(out)


def _clean_string_table(table: bytes) -> bytes:
    """Blank paths in place: offsets into the table stay valid, the names become bare."""
    if len(table) < 12:
        return table
    size = struct.unpack_from("<I", table, 8)[0]
    buffer = bytearray(table[12:12 + size])
    for match in re.finditer(rb"[^\0]+", bytes(buffer)):
        name = match.group()
        if LOCAL_PATH.match(name):
            bare = _basename(name)
            buffer[match.start():match.end()] = bare + b"\0" * (len(name) - len(bare))
    return table[:12] + bytes(buffer) + table[12 + size:]


def clean_dbi(dbi: bytes) -> bytes:
    sizes = list(struct.unpack_from("<6i", dbi, 24))  # mod info, contributions, map, source info, type servers, (MFC index)
    optional_size, ec_size = struct.unpack_from("<2i", dbi, 48)
    parts = []
    pos = _DBI_HEADER_SIZE
    for size in (sizes[0], sizes[1], sizes[2], sizes[3], sizes[4], ec_size, optional_size):
        parts.append(dbi[pos:pos + size])
        pos += size
    mod_info, contributions, section_map, source_info, type_servers, ec, optional = parts
    mod_info = _clean_mod_info(mod_info)
    source_info = _clean_source_info(source_info)
    ec = _clean_string_table(ec)
    header = bytearray(dbi[:_DBI_HEADER_SIZE])
    struct.pack_into("<i", header, 24, len(mod_info))
    struct.pack_into("<i", header, 36, len(source_info))
    struct.pack_into("<i", header, 52, len(ec))
    return bytes(header) + mod_info + contributions + section_map + source_info + type_servers + ec + optional


def clean_pdb(source: Path, target: Path) -> None:
    block_size, streams = read_msf(source.read_bytes())
    # Stream 0 is the previous directory; it names blocks of the old layout, and dbghelp
    # fails the whole PDB when those lie past the end of the re-laid file.
    streams[0] = b""
    streams[_DBI_STREAM] = clean_dbi(streams[_DBI_STREAM])
    for index, stream in enumerate(streams):
        if stream and LOCAL_PATH.search(stream):
            raise ValueError(f"{source}: stream {index} still holds a local path; is it a /PDBSTRIPPED PDB?")
    target.write_bytes(write_msf(block_size, streams))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    clean_pdb(Path(sys.argv[1]), Path(sys.argv[2]))
