from __future__ import annotations

import struct
import zlib

from bacup_lib.quest_area_ui import tag
from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.quest_area_ui import symbol_ids
from creation_lib.swf.tags import PlaceObject2Tag


def sprite_tags(payload: bytes):
    offset = 4
    while offset < len(payload):
        header, = struct.unpack_from("<H", payload, offset)
        offset += 2
        code, size = header >> 6, header & 63
        if size == 63:
            size, = struct.unpack_from("<I", payload, offset)
            offset += 4
        yield code, payload[offset:offset + size]
        offset += size


def placement(code: int, payload: bytes):
    if code == 26:
        return PlaceObject2Tag.parse(payload)
    if code == 70:
        flags = payload[1]
        end = payload.index(b"\0", 4) + 1 if flags & 8 or (flags & 16 and payload[0] & 2) else 4
        return PlaceObject2Tag.parse(payload[:1] + payload[2:4] + payload[end:])
    return None


def states(entries):
    active, frames = {}, []
    for code, payload in entries:
        item = placement(code, payload)
        if item is not None:
            previous = active.get(item.depth)
            if previous and item.move:
                for field in ("character_id", "matrix", "name"):
                    if getattr(item, field) is None:
                        setattr(item, field, getattr(previous, field))
            active[item.depth] = item
        elif code == 28:
            active.pop(struct.unpack_from("<H", payload)[0], None)
        elif code == 1:
            frames.append({item.name: item for item in active.values() if item.name})
    return frames


def select_children(payload: bytes, names: set[str], *, keep_unnamed: bool = False) -> bytes:
    entries = list(sprite_tags(payload))
    found = {p.name for c, b in entries if (p := placement(c, b)) and p.name in names}
    if found != names:
        raise ValueError(f"Missing source HUD placements: {sorted(names - found)}")
    kept, depths = [], set()
    for code, body in entries:
        item = placement(code, body)
        if item is not None:
            if item.name is not None or not item.move:
                if item.name in names or (keep_unnamed and item.name is None):
                    depths.add(item.depth)
                elif item.depth in depths:
                    kept.append(tag(28, struct.pack("<H", item.depth)))
                    depths.remove(item.depth)
            if item.depth not in depths:
                continue
        if code == 28:
            depth = struct.unpack_from("<H", body)[0]
            if depth not in depths:
                continue
            depths.remove(depth)
        kept.append(tag(code, body))
    return payload[:4] + b"".join(kept)


def resolve_class_placements(payload: bytes, symbols: dict[str, int]) -> bytes:
    output = []
    for code, body in sprite_tags(payload):
        if code == 70 and body[1] & 8:
            end = body.index(b"\0", 4)
            name = body[4:end].decode()
            if name not in symbols:
                raise ValueError(f"Missing status HUD imported symbol: {name}")
            if body[0] & 2:
                raise ValueError(f"Unexpected named and numbered HUD placement: {name}")
            body = bytes((body[0] | 2, body[1] & ~8)) + body[2:4] + struct.pack("<H", symbols[name]) + body[end + 1:]
            if body[1] == 0:
                code, body = 26, body[:1] + body[2:]
        output.append(tag(code, body))
    return payload[:4] + b"".join(output)


def transform(item):
    matrix = item.matrix
    if matrix is None:
        raise ValueError(f"Missing source transform: {item.name}")
    return [matrix.scale_x, matrix.rotate_skew_0, matrix.rotate_skew_1,
            matrix.scale_y, matrix.translate_x / 20, matrix.translate_y / 20]


def replace_tags(movie: bytes, entries) -> bytes:
    body = zlib.decompress(movie[8:]) if movie[:3] == b"CWS" else movie[8:]
    size = (5 + 4 * (body[0] >> 3) + 7) // 8 + 4
    body = body[:size] + b"".join(tag(*entry) for entry in entries)
    return b"FWS" + movie[3:4] + struct.pack("<I", len(body) + 8) + body


def inject_art(source: bytes, destination: bytes, pairs) -> bytes:
    from creation_lib.swf.native_runtime import inject_symbols_renamed

    entries = swf_tags(source)
    texts = {struct.unpack_from("<H", payload)[0]: payload for code, payload in entries if code == 37}
    # The native marker splicer rejects text; placeholders let it remap the entire
    # character graph before we restore the source's external-font text records.
    placeholders = [(39, payload[:2] + struct.pack("<H", 1) + tag(1, b"") + tag(0, b""))
                    if code == 37 else (code, payload) for code, payload in entries]
    injected = inject_symbols_renamed(replace_tags(source, placeholders), destination, pairs)
    offset = symbol_ids(swf_tags(injected))[pairs[0][1]] - symbol_ids(entries)[pairs[0][0]]
    output = []
    for code, payload in swf_tags(injected):
        cid = struct.unpack_from("<H", payload)[0] if code == 39 else -1
        if cid - offset in texts:
            code, payload = 37, struct.pack("<H", cid) + texts[cid - offset][2:]
            at = 2 + (5 + 4 * (payload[2] >> 3) + 7) // 8
            if payload[at] & 1:
                raise ValueError("Imported HUD text requires an embedded font")
        output.append((code, payload))
    return replace_tags(injected, output)
