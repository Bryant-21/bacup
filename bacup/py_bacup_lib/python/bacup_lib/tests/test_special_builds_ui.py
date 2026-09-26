import struct
import zlib

from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.special_builds_ui import bind_button_bars


def movie(tags):
    body = b"\x08\x00\x00\x1e\x01\x00"
    body += b"".join(struct.pack("<HI", code << 6 | 63, len(data)) + data for code, data in tags)
    body += b"\0\0"
    return b"CWS\x25" + struct.pack("<I", len(body) + 8) + zlib.compress(body)


def test_class_only_components_receive_timelines_without_changing_placement():
    placement = b"\x24\x08\x04\x00Shared.AS3.BSButtonHintBar\0matrix-and-name"
    child = struct.pack("<HI", 70 << 6 | 63, len(placement)) + placement
    source = movie([(2, b"\x07\x00art"), (39, b"\x08\x00\x01\x00" + child + b"\0\0"),
        (37, b"\x06\x00<P>$SPECIAL_LOADOUTS_SLOTPURCHASE</P>\0"),
        (76, b"\x01\x00\x00\x00SpecialBuilds.SpecialBuildsMenu\0")])
    tags = swf_tags(bind_button_bars(source))
    assert tags[0] == (2, b"\x07\x00art")
    sprite = next(data for code, data in tags if code == 39)
    size, = struct.unpack_from("<I", sprite, 6)
    placed = sprite[10:10 + size]
    assert placed == b"\x26\x00\x04\x00\x09\x00matrix-and-name"
    assert next(data for code, data in tags if code == 37) == b"\x06\x00<P>$B21_TFA_AddLoadout</P>\0"
    symbols = next(data for code, data in tags if code == 76)
    assert symbols.startswith(b"\x03\x00\x00\x00SpecialBuilds.SpecialBuildsMenu\0")
    assert b"\x09\x00Shared.AS3.BSButtonHintBar\0" in symbols
    assert b"\x0a\x00PerkLibraryImporter\0" in symbols
