from __future__ import annotations

import struct

from creation_lib.swf.native_runtime import list_symbols
from creation_lib.swf.shapes import CurvedEdge, StraightEdge, StyleChange, parse_shape_records
from creation_lib.swf.types import BitReader

from bacup_lib.legendary_perks_ui import swf_tags


def bonus_icons(fonts: bytes, glyphs: dict[str, str]) -> dict:
    font_id = next((character for character, name in list_symbols(fonts) if name == "$MAIN_Font"), None)
    font = next((payload for code, payload in swf_tags(fonts)
                 if code in (48, 75) and struct.unpack_from("<H", payload)[0] == font_id), None)
    if font is None:
        return {}
    flags = font[2]
    count_at = 5 + font[4]
    count, = struct.unpack_from("<H", font, count_at)
    start = count_at + 2
    fmt = "I" if flags & 8 else "H"
    offsets = struct.unpack_from("<" + fmt * (count + 1), font, start)
    codes = struct.unpack_from("<" + ("H" if flags & 4 else "B") * count, font, start + offsets[-1])
    result = {}
    for name, glyph in glyphs.items():
        if len(glyph) != 1 or ord(glyph) not in codes:
            continue
        index = codes.index(ord(glyph))
        reader = BitReader(font[start + offsets[index]:start + offsets[index + 1]])
        records = parse_shape_records(reader, reader.read_ubits(4), reader.read_ubits(4))
        x = y = 0
        commands, points = [], []
        for record in records:
            if isinstance(record, StyleChange) and record.has_move:
                x, y = record.move_x, record.move_y
                commands.append([0, x, y])
                points.append((x, y))
            elif isinstance(record, StraightEdge):
                x, y = x + record.dx, y + record.dy
                commands.append([1, x, y])
                points.append((x, y))
            elif isinstance(record, CurvedEdge):
                cx, cy = x + record.cx, y + record.cy
                x, y = cx + record.ax, cy + record.ay
                commands.append([2, cx, cy, x, y])
                points.extend(((cx, cy), (x, y)))
        if not points:
            continue
        left, top = min(x for x, _ in points), min(y for _, y in points)
        width, height = max(x for x, _ in points) - left, max(y for _, y in points) - top
        if not width or not height:
            continue
        for command in commands:
            for coordinate in range(1, len(command), 2):
                command[coordinate] = (command[coordinate] - left) / height
                command[coordinate + 1] = (command[coordinate + 1] - top) / height
        result[name] = {"width": width / height, "commands": commands}
    return result
