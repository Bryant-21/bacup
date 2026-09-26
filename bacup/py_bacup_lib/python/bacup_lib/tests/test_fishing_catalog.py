from __future__ import annotations

import json
import struct

import pytest

from bacup_lib.run import ConversionRun


def _record(signature, form_id, fields, *, flags=0, version=209):
    body = b"".join(tag.encode() + struct.pack("<H", len(data)) + data for tag, data in fields)
    return struct.pack("<4sIIIIHH", signature.encode(), len(body), flags, form_id, 0, version, 0) + body


def _source(path, *, fish=True, extra=()):
    records = [
        ("ALCH", 0x801, [("EDID", b"B21_CatchReward\0"), ("FULL", b"Catch\0")]),
        ("GLOB", 0x802, [("EDID", b"B21_CatchChance\0"), ("FNAM", b"f"), ("FLTV", struct.pack("<f", 50))]),
    ]
    if fish:
        records.extend([
            ("FISH", 0x800, [("EDID", b"B21_LocalLegend\0"), ("FISP", struct.pack("<f", 0.3)), ("FIRI", struct.pack("<I", 0x801))]),
            ("LVLI", 0x803, [
                ("EDID", b"B21_Fishing_Master\0"),
                ("LVLF", b"\x40"),
                ("LLCT", b"\x02"),
                ("LVLO", struct.pack("<I", 0x800)),
                ("CTDA", struct.pack("<B3sIH2sIIIII", 4, b"\0" * 3, 0x802, 77, b"\0" * 2, 0, 0, 0, 0, 0)),
                ("CTDA", struct.pack("<B3sfH2sIIIII", 0, b"\0" * 3, 1, 875, b"\0" * 2, 0x804, 0, 0, 0, 0)),
                ("LVLO", struct.pack("<I", 0x800)),
                ("CTDA", struct.pack("<B3sfH2sIIIII", 0, b"\0" * 3, 20, 77, b"\0" * 2, 0, 0, 0, 0, 0)),
            ]),
        ])
        records.append(("CNDF", 0x804, [
            ("EDID", b"B21_CatchCondition\0"),
            ("CTDA", struct.pack("<B3sfH2sIIIII", 0, b"\0" * 3, 50, 74, b"\0" * 2, 0x802, 0, 0, 0, 0)),
        ]))
        records.append(("AVIF", 0x805, [
            ("EDID", b"B21_Fishing_ActorValue\0"),
            ("ANAM", bytes.fromhex("0e480261")),
        ]))
    records.extend(extra)
    header = _record("TES4", 0, [("HEDR", struct.pack("<fII", 1.0, len(records), max(row[1] for row in records) + 1))], flags=1)
    groups = []
    for signature, form_id, fields in records:
        record = _record(signature, form_id, fields)
        groups.append(struct.pack("<4sI4sI8s", b"GRUP", 24 + len(record), signature.encode(), 0, b"\0" * 8) + record)
    path.write_bytes(header + b"".join(groups))
    return records


def _catalog_path(directory):
    return directory / "F4SE/Plugins/B21_TalesFromAppalachia/Fishing/Output.esm.json"


def test_catalog_preserves_rules_and_resolves_translated_reward(tmp_path):
    source = tmp_path / "Source.esm"
    original_records = _source(source)
    original_bytes = source.read_bytes()
    with ConversionRun.create_new("fo76", "fo4", str(source), "Output.esm") as run:
        run.run_phase("translate_v2", mod_path=str(tmp_path))
        report = run.run_phase("emit_fishing_catalog", mod_path=str(tmp_path))
        first = _catalog_path(tmp_path).read_bytes()
        run.run_phase("emit_fishing_catalog", mod_path=str(tmp_path))
        assert _catalog_path(tmp_path).read_bytes() == first
        run.save_target(str(tmp_path / "Output.esm"), run_nvnm_validator=False)
    catalog = json.loads(first)
    assert report["assets_written"] == 1
    assert report["records_added"] == report["records_changed"] == report["records_dropped"] == 0
    assert catalog["fish_count"] == 1
    assert catalog["mapping_available"] is True
    records = {record["source_form_id"]: record for record in catalog["records"]}
    assert set(records) == {"00000800", "00000802", "00000803", "00000804", "00000805"}
    exported = records["00000803"]
    assert exported["form_version"] == 209
    assert [(field["signature"], bytes.fromhex(field["hex"])) for field in exported["fields"]] == original_records[-3][2]
    assert catalog["target_forms"]["000800:Source.esm"] is None
    assert catalog["target_forms"]["000801:Source.esm"]["plugin"] == "Output.esm"
    assert catalog["target_forms"]["000801:Source.esm"]["editor_id"] == "B21_CatchReward"
    assert catalog["target_forms"]["000801:Source.esm"]["signature"] == "ALCH"
    assert source.read_bytes() == original_bytes


def test_source_only_export_marks_mapping_unavailable_and_clears_stale_fish(tmp_path):
    source = tmp_path / "Source.esm"
    for fish in (True, False):
        _source(source, fish=fish)
        with ConversionRun.create_new("fo76", "fo4", str(source), "Output.esm") as run:
            run.run_phase("emit_fishing_catalog", mod_path=str(tmp_path))
        catalog = json.loads(_catalog_path(tmp_path).read_bytes())
        assert catalog["fish_count"] == int(fish)
        assert catalog["mapping_available"] is False


def test_catalog_requires_source(tmp_path):
    with ConversionRun.create_new("fo76", "fo4", None, "Output.esm") as run:
        with pytest.raises(RuntimeError, match="requires a source plugin"):
            run.run_phase("emit_fishing_catalog", mod_path=str(tmp_path))


def test_catalog_preserves_weather_keywords_and_non_fishing_setting_names(tmp_path):
    source = tmp_path / "Source.esm"
    _source(source, extra=[
        ("GMST", 0x806, [("EDID", b"fJoystickDeadzone\0"), ("DATA", struct.pack("<f", 0.35))]),
        ("KYWD", 0x807, [("EDID", b"B21_RainKeyword\0")]),
        ("CNDF", 0x808, [
            ("EDID", b"B21_FishingWeatherRules\0"),
            ("CTDA", struct.pack("<B3sfH2sIIIII", 0, b"\0" * 3, 1, 737, b"\0" * 2, 0x807, 0, 0, 0, 0)),
        ]),
        ("WTHR", 0x809, [("EDID", b"B21_RainWeather\0"), ("KSIZ", struct.pack("<I", 1)), ("KWDA", struct.pack("<I", 0x807))]),
        ("WTHR", 0x80A, [("EDID", b"B21_ClearWeather\0")]),
        ("GMST", 0x80B, [("EDID", b"fJumpHeightMin\0"), ("DATA", struct.pack("<f", 100))]),
    ])
    with ConversionRun.create_new("fo76", "fo4", str(source), "Output.esm") as run:
        run.run_phase("emit_fishing_catalog", mod_path=str(tmp_path))
    catalog = json.loads(_catalog_path(tmp_path).read_bytes())
    records = {record["source_form_id"]: record for record in catalog["records"]}
    assert records["00000806"]["editor_id"] == "fJoystickDeadzone"
    weather = records["00000809"]
    assert next(field["hex"] for field in weather["fields"] if field["signature"] == "KWDA") == "07080000"
    assert "0000080A" not in records
    assert "0000080B" not in records
    assert "000809:Source.esm" in catalog["target_forms"]


@pytest.mark.parametrize("source_game,target_game", [("fo4", "fo4"), ("fo76", "starfield")])
def test_other_game_pairs_do_not_emit_catalog(tmp_path, source_game, target_game):
    source = tmp_path / "Source.esm"
    _source(source)
    with ConversionRun.create_new(source_game, target_game, str(source), "Output.esm") as run:
        report = run.run_phase("emit_fishing_catalog", mod_path=str(tmp_path))
    assert report["assets_written"] == 0
    assert not _catalog_path(tmp_path).exists()
