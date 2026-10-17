"""Opaque SWF container editing. Untouched tag headers and payloads stay exact.

The Python shape codec is used only for authored/replaced artwork, never to
reserialize a loaded menu. FWS and CWS are supported; other containers fail
explicitly before a document is changed.
"""
from __future__ import annotations

from dataclasses import dataclass
import struct
import zlib


@dataclass(frozen=True)
class MovieTag:
    code: int
    payload: bytes
    encoded: bytes

    @property
    def character_id(self) -> int | None:
        if self.code in {2, 6, 7, 10, 11, 14, 20, 21, 22, 32, 33, 34, 35, 36, 37, 39, 46, 48, 60, 75, 83, 84, 87, 90, 91} and len(self.payload) >= 2:
            return struct.unpack_from("<H", self.payload)[0]
        return None


def encode_tag(code: int, payload: bytes) -> bytes:
    if not 0 <= code <= 1023:
        raise ValueError("Tag code must be between 0 and 1023")
    length = len(payload)
    return (struct.pack("<H", (code << 6) | min(length, 63))
            + (struct.pack("<I", length) if length >= 63 else b"") + payload)


def simple_alpha(payload: bytes | None) -> float:
    """Decode alpha-only CXFORMWITHALPHA; reject tint/addition transforms."""
    if payload is None:
        return 255
    from creation_lib.swf.types import BitReader
    reader = BitReader(payload)
    add, multiply, bits = reader.read_ubits(1), reader.read_ubits(1), reader.read_ubits(4)
    multipliers = [reader.read_sbits(bits) for _ in range(4)] if multiply else [256] * 4
    additions = [reader.read_sbits(bits) for _ in range(4)] if add else [0] * 4
    if multipliers[:3] != [256] * 3 or any(additions) or not 0 <= multipliers[3] <= 256:
        raise ValueError("RGB tint/additive color transforms require binary editing")
    return multipliers[3] * 255 / 256


def artwork_metadata_supported(movie: Movie) -> bool:
    """Keep metadata omitted by the authoring codec in an opaque session."""
    from creation_lib.swf.types import BitReader, RECT
    reader = BitReader(movie.header)
    rect = RECT.parse(reader)
    reader.align()
    if rect.xmin or rect.ymin or reader.read_ui16() % 256:
        return False
    for tag in movie.tags:
        if tag.code == 83:
            reader = BitReader(tag.payload)
            reader.read_ui16()
            RECT.parse(reader)
            reader.align()
            RECT.parse(reader)
            reader.align()
            if reader.read_ui8():
                return False
    return True


def split_tags(stream: bytes) -> list[MovieTag]:
    tags = []
    offset = 0
    while offset < len(stream):
        start = offset
        if offset + 2 > len(stream):
            raise ValueError("Truncated SWF tag header")
        header = struct.unpack_from("<H", stream, offset)[0]
        offset += 2
        code, length = header >> 6, header & 63
        if length == 63:
            if offset + 4 > len(stream):
                raise ValueError("Truncated long SWF tag header")
            length = struct.unpack_from("<I", stream, offset)[0]
            offset += 4
        if offset + length > len(stream):
            raise ValueError(f"Truncated SWF tag {code}")
        payload = stream[offset:offset + length]
        offset += length
        tags.append(MovieTag(code, payload, stream[start:offset]))
        if code == 0:
            if length or offset != len(stream):
                raise ValueError("Data after End tag or nonempty End tag")
            return tags
    raise ValueError("SWF tag stream has no End tag")


class Movie:
    def __init__(self, data: bytes):
        if len(data) < 9 or data[:3] not in (b"FWS", b"CWS"):
            raise ValueError("Only FWS and CWS SWFs are supported (ZWS/GFX require conversion)")
        self.original = bytes(data)
        self.signature, self.version = data[:3], data[3]
        try:
            body = zlib.decompress(data[8:]) if self.signature == b"CWS" else data[8:]
        except zlib.error as exc:
            raise ValueError(f"Invalid compressed SWF: {exc}") from exc
        if len(body) + 8 != struct.unpack_from("<I", data, 4)[0]:
            raise ValueError("SWF file length does not match its decompressed contents")
        rect_size = (5 + 4 * (body[0] >> 3) + 7) // 8
        offset = rect_size + 4
        if offset > len(body):
            raise ValueError("Truncated SWF movie header")
        self.header = body[:offset]
        self.tags = split_tags(body[offset:])

    def assemble(self, tags: list[MovieTag] | None = None) -> bytes:
        if tags is None or tags == self.tags:
            return self.original
        body = self.header + b"".join(tag.encoded for tag in tags)
        split_tags(body[len(self.header):])
        compressed = zlib.compress(body) if self.signature == b"CWS" else body
        return self.signature + bytes([self.version]) + struct.pack("<I", len(body) + 8) + compressed

    def replace_tag(self, index: int, payload: bytes) -> bytes:
        tag = self.tags[index]
        if tag.code in {0, 1}:
            raise ValueError("End and ShowFrame control tags cannot be replaced")
        if tag.character_id is not None and payload[:2] != tag.payload[:2]:
            raise ValueError("Replacement must preserve the character ID")
        if tag.code == 39:
            if len(payload) < 4:
                raise ValueError("Truncated DefineSprite")
            split_tags(payload[4:])
        tags = list(self.tags)
        tags[index] = MovieTag(tag.code, bytes(payload), encode_tag(tag.code, payload))
        return self.assemble(tags)

    def replace_shape(self, character_id: int, shape) -> bytes:
        from creation_lib.swf.tags import DefineShapeTag, write_tag
        for index, tag in enumerate(self.tags):
            if tag.code in {2, 22, 32, 83} and tag.character_id == character_id:
                shape.shape_id = character_id
                shape.shape_version = {2: 1, 22: 2, 32: 3, 83: 4}[tag.code]
                _, payload = write_tag(DefineShapeTag(shape=shape, tag_id=tag.code))
                return self.replace_tag(index, payload)
        raise ValueError(f"No editable vector shape with character ID {character_id}")
