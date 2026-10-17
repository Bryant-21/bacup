import struct

from creation_lib.swf.shapes import EndShape, ShapeDef, StraightEdge, StyleChange
from creation_lib.swf.tags import DefineShapeTag, PlaceObject2Tag
from creation_lib.swf.types import RGBA, FillStyle, LineStyle2

from bacup_lib.legendary_perks_ui import swf_tags
from bacup_lib.status_hud_ui import location_icon_art
from bacup_lib.tests.test_legendary_perks_ui import swf

CREAM = RGBA(255, 255, 203, 255)
FAINT_GOLD = RGBA(245, 203, 91, 76)


def shape(shape_id, *lines):
    return DefineShapeTag(ShapeDef(
        shape_id=shape_id, bounds=(0, 0, 400, 400), fill_styles=[FillStyle(fill_type=0, color=CREAM)],
        line_styles=[LineStyle2(width=20, color=color) for color in lines],
        records=[StyleChange(move_x=0, move_y=0, fill1=1, line=1), StraightEdge(400, 0), StraightEdge(0, 400),
                 StyleChange(line=len(lines)), StraightEdge(-400, 0), StraightEdge(0, -400), EndShape()],
        shape_version=4, edge_bounds=(0, 0, 400, 400), shape_flags=1), 83).to_bytes()


def sprite(sprite_id, character):
    place = PlaceObject2Tag(depth=1, character_id=character).to_bytes()
    return struct.pack("<HH", sprite_id, 1) + struct.pack("<HI", 26 << 6 | 63, len(place)) + place + b"\x40\0\0\0"


def shapes(movie):
    return {struct.unpack_from("<H", payload)[0]: payload for code, payload in swf_tags(movie) if code == 83}


def test_location_icons_drop_translucent_strokes_and_compass_widgets_keep_theirs():
    names = [(20, "TownMarker"), (21, "CompassMarkerWidget"), (22, "DirectionMarkerWidget")]
    source = swf([(83, shape(10, FAINT_GOLD, CREAM)), (83, shape(11, FAINT_GOLD)),
                  (39, sprite(20, 10)), (39, sprite(21, 11)), (39, sprite(22, 11)),
                  (76, struct.pack("<H", len(names)) + b"".join(struct.pack("<H", cid) + name.encode() + b"\0"
                                                             for cid, name in names))])

    converted = shapes(location_icon_art(source, [name for _, name in names]))

    icon = DefineShapeTag.parse(converted[10], 83).shape
    assert [style.color for style in icon.line_styles] == [CREAM]
    assert [record.line for record in icon.records if isinstance(record, StyleChange)] == [0, 1]
    assert converted[11] == shapes(source)[11]
