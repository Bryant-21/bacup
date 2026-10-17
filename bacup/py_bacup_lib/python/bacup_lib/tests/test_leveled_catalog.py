from __future__ import annotations

import json
import struct

from bacup_lib.run import ConversionRun
from bacup_lib.tests.catalog_schema_validation import validate_catalog


def _record(signature, form_id, fields, *, flags=0, version=209):
    body = b"".join(tag.encode() + struct.pack("<H", len(data)) + data for tag, data in fields)
    return struct.pack("<4sIIIIHH", signature.encode(), len(body), flags, form_id, 0, version, 0) + body


def _ctda(flags, comparison, function, param1=0, run_on=0, reference=0):
    return struct.pack("<B3sIH2sIIIIi", flags, b"\0" * 3, comparison, function, b"\0" * 2, param1, 0, run_on, reference, -1)


def _f(value):
    return struct.pack("<f", value)


def _u(value):
    return struct.pack("<I", value)


def _source(path):
    records = [
        ("KYWD", 0x810, [("EDID", b"B21_LocRegionGolf\0")]),
        ("GLOB", 0x811, [("EDID", b"B21_FestiveChance\0"), ("FNAM", b"f"), ("FLTV", _f(100))]),
        ("ALCH", 0x820, [("EDID", b"B21_GolfOutfit\0"), ("FULL", b"Golf\0")]),
        ("ALCH", 0x821, [("EDID", b"B21_GeneralOutfit\0"), ("FULL", b"General\0")]),
        ("KYWD", 0x850, [("EDID", b"B21_ESSMainRobot\0")]),
        ("AVIF", 0x851, [("EDID", b"B21_ESSChanceMainRobot\0"), ("NAM3", _u(0x850))]),
        ("FLST", 0x852, [("EDID", b"EMSMainKeywordMap\0"), ("LNAM", _u(0x851))]),
        ("FLST", 0x853, [("EDID", b"EMSActorValueLists\0"), ("LNAM", _u(0x852))]),
        ("DFOB", 0x854, [("EDID", b"EMSActorValueLists_DO\0"), ("DATA", _u(0x853))]),
        ("LVLI", 0x830, [
            ("EDID", b"B21_Outfit_FeralGhoul\0"),
            ("LVLF", struct.pack("<H", 0x40)),
            ("LLCT", b"\x02"),
            ("LVLO", _u(0x820)), ("LVLV", _f(1)), ("LVIV", _f(1)), ("LVOV", _f(0)),
            ("CTDA", _ctda(0, struct.unpack("<I", _f(1))[0], 579, 0x810)),
            ("LVLO", _u(0x821)), ("LVLV", _f(1)), ("LVIV", _f(1)), ("LVOV", _f(0)),
        ]),
        ("LVLI", 0x831, [
            ("EDID", b"B21_Plain\0"),
            ("LVLF", struct.pack("<H", 0x01)),
            ("LVLO", _u(0x820)), ("LVLV", _f(1)), ("LVIV", _f(1)), ("LVOV", _f(0)),
        ]),
        ("LVLI", 0x832, [
            ("EDID", b"B21_Festive\0"),
            ("LVLF", struct.pack("<H", 0x00)),
            ("LVLO", _u(0x821)), ("LVLV", _f(1)), ("LVIV", _f(1)), ("LVOV", _f(0)), ("LVOC", _u(0x811)),
        ]),
    ]
    header = _record("TES4", 0, [("HEDR", struct.pack("<fII", 1.0, len(records), 0x900))], flags=1)
    groups = []
    for signature, form_id, fields in records:
        record = _record(signature, form_id, fields)
        groups.append(struct.pack("<4sI4sI8s", b"GRUP", 24 + len(record), signature.encode(), 0, b"\0" * 8) + record)
    path.write_bytes(header + b"".join(groups))


def _catalog(tmp_path):
    return json.loads((tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/LeveledLists/Output.esm.json").read_text(encoding="utf-8"))


def test_catalog_keeps_conditions_flags_and_globals(tmp_path):
    source = tmp_path / "Source.esm"
    _source(source)
    with ConversionRun.create_new("fo76", "fo4", str(source), "Output.esm") as run:
        run.run_phase("translate_v2", mod_path=str(tmp_path))
        report = run.run_phase("emit_leveled_catalog", mod_path=str(tmp_path))
    catalog = _catalog(tmp_path)
    assert report["assets_written"] == 1
    assert catalog["version"] == 1
    validate_catalog("LeveledLists", catalog)
    assert catalog["settings"]["source_plugin"] == "Source.esm"
    lists = {row["editor_id"]: row for row in catalog["lists"]}
    assert set(lists) == {"B21_Outfit_FeralGhoul", "B21_Festive"}
    outfit = lists["B21_Outfit_FeralGhoul"]
    assert outfit["source"] == "Source.esm:000830"
    assert outfit["target"].startswith("Output.esm:")
    assert outfit["scope"] == "always"
    assert outfit["flags"] == 0x40
    condition = outfit["entries"][0]["conditions"][0]
    assert condition["function"] == 579
    assert condition["params"][0]["source"] == "Source.esm:000810"
    assert condition["params"][0]["form"].startswith("Output.esm:")
    assert outfit["entries"][1]["conditions"] == []
    festive = lists["B21_Festive"]["entries"][0]["chance_none"]
    assert festive["global"].startswith("Output.esm:")
    assert "unresolved" not in festive
    assert catalog["diagnostics"]["lists"] == 2


def test_catalog_is_deterministic(tmp_path):
    source = tmp_path / "Source.esm"
    _source(source)
    with ConversionRun.create_new("fo76", "fo4", str(source), "Output.esm") as run:
        run.run_phase("translate_v2", mod_path=str(tmp_path))
        first = run.run_phase("emit_leveled_catalog", mod_path=str(tmp_path))
        bytes_before = (tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/LeveledLists/Output.esm.json").read_bytes()
        run.run_phase("emit_leveled_catalog", mod_path=str(tmp_path))
    assert first["assets_written"] == 1
    assert (tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/LeveledLists/Output.esm.json").read_bytes() == bytes_before


def test_catalog_exports_ess_axes(tmp_path):
    source = tmp_path / "Source.esm"
    _source(source)
    with ConversionRun.create_new("fo76", "fo4", str(source), "Output.esm") as run:
        run.run_phase("translate_v2", mod_path=str(tmp_path))
        run.run_phase("emit_leveled_catalog", mod_path=str(tmp_path))
    ess = _catalog(tmp_path)["ess"]
    assert ess["reroll_seconds"] == 1200
    assert ess["recheck_seconds"] == 60
    assert [row["keyword"].startswith("Output.esm:") for row in ess["axes"]["main"]] == [True]
    assert ess["placed_locations"] == {}
