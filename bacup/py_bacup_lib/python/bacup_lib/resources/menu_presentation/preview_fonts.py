import struct

from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.quest_area_ui import symbol_ids
from bacup_lib.status_hud_source import replace_tags


def embed_game_fonts(movie: bytes, library: bytes) -> bytes:
    tags, fonts = swf_tags(movie), swf_tags(library)
    names = symbol_ids(fonts)
    mapping = {name: 60000 + index for index, name in enumerate(names)}
    imported = [(code, struct.pack('<H', mapping[name]) + payload[2:])
                for name in mapping for code, payload in fonts
                if code in (48, 75, 73, 88) and struct.unpack_from('<H', payload)[0] == names[name]]
    output = []
    for code, payload in tags:
        if code == 37:
            at = 2 + (5 + 4 * (payload[2] >> 3) + 7) // 8
            if payload[at + 1] & 128:
                end = payload.index(b'\0', at + 2)
                name = payload[at + 2:end].decode()
                if name not in mapping:
                    raise ValueError(f'Unmapped preview font: {name}')
                payload = (payload[:at] + bytes((payload[at] | 1, (payload[at + 1] & ~128) | 1)) +
                           struct.pack('<H', mapping[name]) + payload[end + 1:])
        output.append((code, payload))
        if code == 69:
            output.extend(imported)
    return replace_tags(movie, output)
