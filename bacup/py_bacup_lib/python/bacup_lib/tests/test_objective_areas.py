import json
from pathlib import Path
import struct
import subprocess

import pytest


ROOT = Path(__file__).resolve().parents[5]
EXPORTER = ROOT / "bacup/py_bacup_lib/target/debug/examples/export_objective_areas.exe"


def record(signature, form, fields, version):
    body = b"".join(tag.encode() + struct.pack("<H", len(data)) + data for tag, data in fields)
    return struct.pack("<4sIIIIHH", signature.encode(), len(body), 0, form, 0, version, 0) + body


def plugin(path, rows, version):
    header = record("TES4", 0, [("HEDR", struct.pack("<fII", 1.0, len(rows), 0x1000))], version)
    groups = []
    for sig, form, fields in rows:
        encoded = record(sig, form, fields, version)
        groups.append(struct.pack("<4sI4sI8s", b"GRUP", 24 + len(encoded), sig.encode(), 0, b"\0" * 8) + encoded)
    path.write_bytes(header + b"".join(groups))


def export(tmp_path, source_targets, output_targets, *, source_version=209, global_present=True):
    if not EXPORTER.exists():
        pytest.skip("build the native export_objective_areas example first")
    source, output = tmp_path / "Source.esm", tmp_path / "Output.esm"
    fields = lambda targets: [("EDID", b"B21_AreaQuest\0"), ("QOBJ", struct.pack("<H", 10)), *targets]
    glob = lambda value: [("EDID", b"B21_Radius\0"), ("FNAM", b"f"), ("FLTV", struct.pack("<f", value))]
    plugin(source, [("QUST", 0x800, fields(source_targets)), ("GLOB", 0x801, glob(50000))], source_version)
    rows = [("QUST", 0x900, fields(output_targets))]
    if global_present:
        rows.append(("GLOB", 0x901, glob(0)))
    plugin(output, rows, 131)
    before = source.read_bytes(), output.read_bytes()
    result = subprocess.run([str(EXPORTER), str(source), str(output), str(tmp_path)], capture_output=True, text=True)
    assert result.returncode == 0, result.stdout + result.stderr
    path = tmp_path / "F4SE/Plugins/B21_TalesFromAppalachia/ObjectiveAreas/Output.esm.json"
    first = path.read_bytes()
    subprocess.run([str(EXPORTER), str(source), str(output), str(tmp_path)], check=True, capture_output=True)
    assert path.read_bytes() == first
    assert before == (source.read_bytes(), output.read_bytes())
    return json.loads(first)


def qsta(alias=0, flags=0, radius=1800, *, source=True):
    return "QSTA", struct.pack("<iHII", alias, flags, 0, radius) if source else struct.pack("<iII", alias, flags & 255, 0)


def condition(value, parameter=0):
    return "CTDA", struct.pack("<B3sfH2sIIIII", 0, b"\0" * 3, value, 74, b"\0" * 2, parameter, 0, 0, 0, 0)


def test_literal_alias_zero_and_global_mapping(tmp_path):
    data = export(tmp_path, [qsta(flags=0x200), qsta(1, 0x300, 0x801)], [qsta(source=False), qsta(1, source=False)])
    assert data["skipped"] == []
    literal, global_area = data["areas"]
    assert literal["quest"] == {"plugin": "Output.esm", "object_id": 0x900}
    assert literal["alias"] == 0 and literal["area_flags"] == 2
    assert literal["radius"] == {"kind": "literal", "value": 1800}
    assert global_area["radius"] == {"kind": "global", "form": {"plugin": "Output.esm", "object_id": 0x901}}


def test_same_alias_distinct_conditions_keep_ordinals(tmp_path):
    source = [qsta(radius=4000), condition(1), qsta(radius=2000), condition(2)]
    output = [qsta(source=False), condition(1), qsta(source=False), condition(2)]
    data = export(tmp_path, source, output)
    assert [(a["target"], a["radius"]["value"]) for a in data["areas"]] == [(0, 4000), (1, 2000)]


def test_reordered_same_alias_conditions_rejected(tmp_path):
    data = export(tmp_path, [qsta(radius=4000), condition(1), qsta(radius=2000), condition(2)],
        [qsta(source=False), condition(2), qsta(source=False), condition(1)])
    assert data["areas"] == []
    assert {s["reason"] for s in data["skipped"]} == {"ambiguous repeated-alias conditions changed"}


def test_missing_global_and_deleted_target_are_reported(tmp_path):
    data = export(tmp_path, [qsta(flags=0x100, radius=0x801)], [qsta(source=False)], global_present=False)
    assert data["areas"] == [] and data["skipped"][0]["reason"] == "radius global unmapped"


def test_zero_area_and_ordinary_target(tmp_path):
    data = export(tmp_path, [qsta(radius=0), qsta(1, 0x200, 0)], [qsta(source=False), qsta(1, source=False)])
    assert len(data["areas"]) == 1 and data["areas"][0]["alias"] == 1
    assert data["areas"][0]["radius"]["value"] == 0
    assert data["nonzero_source_declarations"] == 0


def test_v151_byte_flags_and_integer_radius(tmp_path):
    data = export(tmp_path, [("QSTA", struct.pack("<iBII", 0, 4, 0, 3000))],
        [qsta(flags=4, source=False)], source_version=151)
    assert data["areas"][0]["flags"] == 4
    assert data["areas"][0]["radius"]["value"] == 3000


def test_repeated_alias_condition_formkeys_are_remapped(tmp_path):
    data = export(tmp_path, [qsta(radius=4000), condition(1, 0x801), qsta(radius=2000), condition(2, 0x801)],
        [qsta(source=False), condition(1, 0x901), qsta(source=False), condition(2, 0x901)])
    assert data["skipped"] == []
    assert data["areas"][0]["conditions"][0]["parameters"][0] == {
        "kind": "form", "form": {"plugin": "Output.esm", "object_id": 0x901}}


def test_same_alias_parameter_only_reorder_rejected(tmp_path):
    data = export(tmp_path, [qsta(radius=4000), condition(1, 0x801), qsta(radius=2000), condition(1)],
        [qsta(source=False), condition(1), qsta(source=False), condition(1, 0x901)])
    assert data["areas"] == []


def test_deleted_target_reported(tmp_path):
    data = export(tmp_path, [qsta(), qsta(1)], [qsta(source=False)])
    assert data["areas"] == []
    assert all(row["reason"] == "objective target count changed" for row in data["skipped"])


def test_unique_area_not_rejected_for_other_alias_condition_lowering(tmp_path):
    data = export(tmp_path, [qsta(1), qsta(2, radius=0), condition(1), qsta(2, radius=0), condition(2)],
        [qsta(1, source=False), qsta(2, source=False), qsta(2, source=False)])
    assert data["skipped"] == [] and len(data["areas"]) == 1


def test_identical_repeated_alias_area_semantics_allow_condition_lowering(tmp_path):
    data = export(tmp_path, [qsta(), condition(1), qsta(), condition(2)],
        [qsta(source=False), qsta(source=False)])
    assert data["skipped"] == [] and len(data["areas"]) == 2
