import json
import struct

import pytest

from bacup_lib.camp_snap import (
    FEATURE, build_document, flatten, key_function, parse_enam, parse_node_type, parse_template,
    parse_transform,
)
from bacup_lib.plugin_catalog import write_document
from bacup_lib.tests.catalog_schema_validation import validate_catalog

KEY = key_function("SeventySix.esm", [])


def enam36(node_id, stnd, x, y, z, rx, ry, rz):
    return struct.pack("<II6ff", node_id, stnd, x, y, z, rx, ry, rz, 99.0)


def enam24(node_id, stnd, x, y, z, rz):
    return struct.pack("<II4f", node_id, stnd, x, y, z, rz)


def onam(node_id):
    return struct.pack("<I", node_id)


def u32(value):
    return struct.pack("<I", value)


def edid(text):
    return text.encode() + b"\0"


def test_enam_36_bytes_reads_full_rotation_and_ignores_trailing_float():
    node = parse_enam(enam36(7, 0x3071C3, 1, 2, 3, 10, 20, 30))
    assert (node.id, node.type, node.pos, node.rot) == (7, 0x3071C3, (1, 2, 3), (10, 20, 30))


def test_enam_24_bytes_reads_z_rotation_only():
    node = parse_enam(enam24(3, 0x10, 4, 5, 6, 90))
    assert (node.id, node.pos, node.rot) == (3, (4, 5, 6), (0.0, 0.0, 90))


def test_tnam_sizes():
    assert parse_transform(struct.pack("<6f", 1, 2, 3, 4, 5, 6)) == ((1, 2, 3), (4, 5, 6))
    assert parse_transform(struct.pack("<4f", 1, 2, 3, 45)) == ((1, 2, 3), (0.0, 0.0, 45))
    assert parse_transform(b"\0" * 12) is None


def test_non_finite_enam_is_dropped():
    assert parse_enam(enam24(1, 2, float("nan"), 0, 0, 0)) is None


def template(*subrecords):
    return parse_template(list(subrecords))


def ids(nodes):
    return [node.id for node in nodes]


def test_child_inherits_parent_nodes_and_appends_its_own():
    templates = {
        1: template(("EDID", edid("Parent")), ("ENAM", enam24(0, 0x10, 0, 0, 0, 0)), ("ENAM", enam24(1, 0x10, 1, 0, 0, 0))),
        2: template(("PNAM", u32(1)), ("ENAM", enam24(5, 0x11, 9, 9, 9, 0))),
        3: template(("PNAM", u32(2)), ("ENAM", enam24(6, 0x11, 0, 0, 0, 0))),
    }
    assert ids(flatten(3, templates)) == [0, 1, 5, 6]


def test_onam_followed_by_tnam_moves_the_parent_node():
    templates = {
        1: template(("ENAM", enam36(9, 0x10, 0, 0, 0, 0, 0, 0))),
        2: template(("PNAM", u32(1)), ("ONAM", onam(9)), ("TNAM", struct.pack("<6f", 64, 64, 0, 0, 0, 90))),
    }
    [node] = flatten(2, templates)
    assert (node.id, node.type, node.pos, node.rot) == (9, 0x10, (64, 64, 0), (0, 0, 90))
    assert flatten(1, templates)[0].pos == (0, 0, 0)


def test_bare_onam_removes_the_parent_node():
    templates = {
        1: template(("ENAM", enam24(0, 0x10, 0, 0, 0, 0)), ("ENAM", enam24(1, 0x10, 0, 0, 0, 0)),
                    ("ENAM", enam24(2, 0x10, 0, 0, 0, 0))),
        2: template(("PNAM", u32(1)), ("ENAM", enam24(1008, 0x11, 0, 0, 0, 0)), ("ONAM", onam(0)), ("ONAM", onam(2)),
                    ("TNAM", struct.pack("<4f", 1, 1, 1, 0)), ("ONAM", onam(1)), ("INAM", u32(0))),
    }
    nodes = flatten(2, templates)
    assert ids(nodes) == [2, 1008]
    assert nodes[0].pos == (1, 1, 1)


def test_child_enam_with_parent_id_replaces_the_node():
    templates = {
        1: template(("ENAM", enam24(4, 0x10, 0, 0, 0, 0)), ("ENAM", enam24(5, 0x10, 0, 0, 0, 0))),
        2: template(("PNAM", u32(1)), ("ENAM", enam24(4, 0x22, 7, 7, 7, 0))),
    }
    nodes = flatten(2, templates)
    assert ids(nodes) == [4, 5]
    assert (nodes[0].type, nodes[0].pos) == (0x22, (7, 7, 7))


def test_parent_cycle_terminates():
    templates = {
        1: template(("PNAM", u32(2)), ("ENAM", enam24(1, 0x10, 0, 0, 0, 0))),
        2: template(("PNAM", u32(1)), ("ENAM", enam24(2, 0x10, 0, 0, 0, 0))),
        3: template(("PNAM", u32(3)), ("ENAM", enam24(3, 0x10, 0, 0, 0, 0))),
    }
    assert sorted(ids(flatten(1, templates))) == [1, 2]
    assert ids(flatten(3, templates)) == [3]


def test_missing_parent_keeps_own_nodes():
    templates = {1: template(("PNAM", u32(0x999)), ("ENAM", enam24(1, 0x10, 0, 0, 0, 0)))}
    assert ids(flatten(1, templates)) == [1]


def test_category_from_stpt_else_zero():
    assert template(("STPT", u32(3))).category == 3
    assert template(("ENAM", enam24(1, 0x10, 0, 0, 0, 0))).category == 0


def test_node_type_self_snap_is_fnam_bit_0_and_adjacency_stays_one_way():
    floor = parse_node_type([("EDID", edid("SnapNode_Floor01")), ("FNAM", u32(1))])
    wall = parse_node_type([("EDID", edid("SnapNode_Wall01L")), ("FNAM", u32(4)), ("NNAM", u32(0x20))])
    no_flags = parse_node_type([("EDID", edid("X"))])
    assert floor == {"editor_id": "SnapNode_Floor01", "self_snap": True, "adjacent": []}
    assert wall == {"editor_id": "SnapNode_Wall01L", "self_snap": False, "adjacent": [0x20]}
    assert no_flags["self_snap"] is False


def sample():
    node_types = {
        0x10: parse_node_type([("EDID", edid("SnapNode_Floor01")), ("FNAM", u32(1))]),
        0x11: parse_node_type([("EDID", edid("SnapNode_Wall01L")), ("NNAM", u32(0x12))]),
        0x12: parse_node_type([("EDID", edid("SnapNode_Wall01R"))]),
        0x13: parse_node_type([("EDID", edid("snapnode_floor01foundation")), ("FNAM", u32(1))]),
        0x14: parse_node_type([("EDID", edid("DefaultSnapNode_Origin"))]),
    }
    templates = {
        0x100: template(("EDID", edid("Floor_workshopSnap")), ("STPT", u32(2)),
                        ("ENAM", enam36(0, 0x10, 0, 0, 0, 0, 0, 0)), ("ENAM", enam24(1, 0x11, 0.1, 0, 0, 0))),
        0x101: template(("EDID", edid("Empty_workshopSnap"))),
    }
    forms = {0x500: 0x100, 0x501: 0x101, 0x502: 0x999}
    return node_types, templates, forms


def test_document_shape():
    document = build_document(*sample(), KEY)
    assert document["node_types"]["SeventySix.esm:000011"] == {
        "editor_id": "SnapNode_Wall01L", "self_snap": False, "adjacent": ["SeventySix.esm:000012"]}
    assert document["node_types"]["SeventySix.esm:000012"]["adjacent"] == []
    assert document["templates"]["SeventySix.esm:000100"] == {
        "editor_id": "Floor_workshopSnap", "category": 2,
        "nodes": [{"id": 0, "type": "SeventySix.esm:000010", "pos": [0.0, 0.0, 0.0], "rot": [0.0, 0.0, 0.0]},
                  {"id": 1, "type": "SeventySix.esm:000011", "pos": [0.1, 0.0, 0.0], "rot": [0.0, 0.0, 0.0]}]}
    assert document["forms"] == {"SeventySix.esm:000500": "SeventySix.esm:000100"}
    assert document["fo4_aliases"] == {"P-Floor": ["SeventySix.esm:000010", "SeventySix.esm:000013"],
                                       "P-WS-Origin": ["SeventySix.esm:000014"]}


def test_gnam_moves_the_templates_own_flattened_nodes_into_object_space():
    # workshopSnap_WrhsWall01: the master's nodes sit at the tile edge (y 128); GNAM (0,-128,0) puts them on the wall.
    node_types = {0x10: parse_node_type([("EDID", edid("SnapNode_Wall01Bottom"))])}
    templates = {
        1: template(("EDID", edid("Master")), ("ENAM", enam24(1003, 0x10, 0, 128, 0, 0))),
        2: template(("EDID", edid("Wall")), ("PNAM", u32(1)), ("ENAM", enam24(1015, 0x10, 0, 128, 125, 180)),
                    ("GNAM", struct.pack("<3f", 0, -128, 0))),
    }
    document = build_document(node_types, templates, {0x500: 2}, KEY)
    wall = document["templates"]["SeventySix.esm:000002"]["nodes"]
    assert [node["pos"] for node in wall] == [[0.0, 0.0, 0.0], [0.0, 0.0, 125.0]]
    assert wall[1]["rot"] == [0.0, 0.0, 180.0]
    assert document["templates"]["SeventySix.esm:000001"]["nodes"][0]["pos"] == [0.0, 128.0, 0.0]


def test_master_index_resolves_owner():
    key = key_function("Child.esp", ["SeventySix.esm"])
    assert key(0x00123456) == "SeventySix.esm:123456"
    assert key(0x01000ABC) == "Child.esp:000ABC"
    with pytest.raises(ValueError):
        key(0x02000001)


def test_written_document_validates_against_schema(tmp_path):
    path = write_document(FEATURE, "SeventySix.esm", build_document(*sample(), KEY), tmp_path)
    assert path == tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/CampSnap/SeventySix.esm.json"
    document = json.loads(path.read_text(encoding="utf-8"))
    assert list(document)[:2] == ["version", "settings"]
    validate_catalog(FEATURE, document)


def test_schema_accepts_remove_entries():
    validate_catalog(FEATURE, {"version": 1, "settings": {}, "node_types": {"SeventySix.esm:000010": {"remove": True}},
                               "templates": {"SeventySix.esm:000100": {"remove": True}},
                               "forms": {"SeventySix.esm:000500": {"remove": True}},
                               "fo4_aliases": {"P-Floor": {"remove": True}}})



def test_workflow_emits_only_for_fo76_to_fo4(monkeypatch, tmp_path):
    from types import SimpleNamespace
    from bacup_lib import camp_snap
    from bacup_lib.workflows import unified
    calls = []
    result = {"node_types": 1, "templates": 2, "forms": 3, "skipped_forms": 0}
    monkeypatch.setattr(camp_snap, "emit_camp_snap", lambda *args: calls.append(args) or result)
    ctx = SimpleNamespace(source_plugin_path=tmp_path / "SeventySix.esm", mod_path=tmp_path / "mod")
    assert unified._prepare_fo76_camp_snap(SimpleNamespace(source_game="fo76", target_game="fo4"), ctx) == 3
    assert calls == [(ctx.source_plugin_path, ctx.mod_path)]
    assert unified._prepare_fo76_camp_snap(SimpleNamespace(source_game="skyrimse", target_game="fo4"), ctx) == 0
    assert len(calls) == 1
