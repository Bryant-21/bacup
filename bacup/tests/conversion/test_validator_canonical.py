"""Tests for creation_lib/esp/validate.py against canonical-shape YAML."""

from __future__ import annotations

from pathlib import Path

import yaml

from creation_lib.esp.validate import validate_authoring


def _write_plugin_yaml(yaml_dir: Path, plugin: str, masters: list[str]) -> None:
    data = {"plugin": plugin, "header": {"masters": masters}}
    (yaml_dir / "plugin.yaml").write_text(
        yaml.dump(data, default_flow_style=False), encoding="utf-8"
    )


def _write_record(sig_dir: Path, fname: str, fields: list) -> Path:
    sig_dir.mkdir(parents=True, exist_ok=True)
    record = {
        "form_id": fname.split(" - ")[1].split("_")[0],
        "eid": fname.split(" - ")[0],
        "fields": fields,
    }
    path = sig_dir / f"{fname}.yaml"
    path.write_text(
        yaml.dump(record, default_flow_style=False, allow_unicode=True), encoding="utf-8"
    )
    return path


# ---------------------------------------------------------------------------
# Test 1: Canonical refs scanned correctly — clean validation
# ---------------------------------------------------------------------------

def test_canonical_refs_scanned_no_errors(tmp_path):
    """Canonical {reference: {plugin, object_id}} refs are found and validated."""
    yaml_dir = tmp_path / "yaml"
    yaml_dir.mkdir()
    _write_plugin_yaml(yaml_dir, "B21_Test.esp", ["Fallout4.esm"])

    weap_dir = yaml_dir / "WEAP"
    _write_record(
        weap_dir,
        "TestGun - 000800_B21_Test.esp",
        [
            {"PreviewTransform": {"reference": {"plugin": "Fallout4.esm", "object_id": "248AB9"}}},
            {"Keywords": [
                {"reference": {"plugin": "Fallout4.esm", "object_id": "017E69"}},
            ]},
        ],
    )

    errors, checked = validate_authoring(yaml_dir)

    assert checked >= 1
    undeclared = [e for e in errors if "not listed as a master" in e["reason"]]
    assert undeclared == []


# ---------------------------------------------------------------------------
# Test 2: Internal FK consistency — missing target detected
# ---------------------------------------------------------------------------

def test_internal_ref_missing_target(tmp_path):
    """An internal reference whose target YAML doesn't exist is reported."""
    yaml_dir = tmp_path / "yaml"
    yaml_dir.mkdir()
    _write_plugin_yaml(yaml_dir, "B21_Test.esp", [])

    weap_dir = yaml_dir / "WEAP"
    _write_record(
        weap_dir,
        "TestGun - 000800_B21_Test.esp",
        [
            {"SomeRef": {"reference": {"plugin": "B21_Test.esp", "object_id": "000801"}}},
        ],
    )

    errors, checked = validate_authoring(yaml_dir)

    assert checked >= 1
    internal_errors = [e for e in errors if "internal ref not found" in e["reason"]]
    assert len(internal_errors) >= 1
    assert any("000801:B21_Test.esp" in e["formkey"] for e in internal_errors)


# ---------------------------------------------------------------------------
# Test 3: Undeclared master is reported
# ---------------------------------------------------------------------------

def test_undeclared_master_reported(tmp_path):
    """A reference to a plugin not listed as a master is flagged."""
    yaml_dir = tmp_path / "yaml"
    yaml_dir.mkdir()
    _write_plugin_yaml(yaml_dir, "B21_Test.esp", [])  # no masters declared

    weap_dir = yaml_dir / "WEAP"
    _write_record(
        weap_dir,
        "TestGun - 000800_B21_Test.esp",
        [
            {"SomeRef": {"reference": {"plugin": "OtherMod.esp", "object_id": "001234"}}},
        ],
    )

    errors, checked = validate_authoring(yaml_dir)

    assert checked >= 1
    undeclared = [e for e in errors if "not listed as a master" in e["reason"]]
    assert len(undeclared) >= 1
    assert any("OtherMod.esp" in e["reason"] for e in undeclared)


# ---------------------------------------------------------------------------
# Test 4: Mixed string-FK + canonical both scanned
# ---------------------------------------------------------------------------

def test_mixed_string_and_canonical_refs(tmp_path):
    """Legacy string-FK shorthand lines are also caught alongside canonical refs."""
    yaml_dir = tmp_path / "yaml"
    yaml_dir.mkdir()
    _write_plugin_yaml(yaml_dir, "B21_Test.esp", ["Fallout4.esm"])

    weap_dir = yaml_dir / "WEAP"
    weap_dir.mkdir(parents=True, exist_ok=True)

    record_yaml = weap_dir / "TestGun - 000800_B21_Test.esp.yaml"
    record_yaml.write_text(
        "form_id: '000800'\n"
        "eid: TestGun\n"
        "fields:\n"
        "- SomeRef:\n"
        "    reference:\n"
        "      plugin: Fallout4.esm\n"
        "      object_id: '248AB9'\n"
        "- LegacyNote: '017E69:Fallout4.esm'\n",
        encoding="utf-8",
    )

    errors, checked = validate_authoring(yaml_dir)

    assert checked >= 2  # canonical 248AB9 + legacy 017E69
    undeclared = [e for e in errors if "not listed as a master" in e["reason"]]
    assert undeclared == []


# ---------------------------------------------------------------------------
# Test 5: Filename FK parsing accepted as internal definitions
# ---------------------------------------------------------------------------

def test_authoring_validate_parses_formkeys_from_names(tmp_path):
    """Records named ``EID - <id>_<plugin>.yaml`` register as internal defs."""
    yaml_dir = tmp_path / "yaml"
    yaml_dir.mkdir()
    _write_plugin_yaml(yaml_dir, "B21_Test.esp", ["Fallout4.esm"])
    record_dir = yaml_dir / "records" / "WEAP"
    record_dir.mkdir(parents=True)
    (record_dir / "10mm - 004822_Fallout4.esm.yaml").write_text(
        "form_id: '004822'\n",
        encoding="utf-8",
    )
    folder_dir = yaml_dir / "records" / "CELL" / "EID - 000800_B21_Test.esp"
    folder_dir.mkdir(parents=True)
    (folder_dir / "RecordData.yaml").write_text(
        "signature: CELL\nform_id: '000800:B21_Test.esp'\n",
        encoding="utf-8",
    )

    errors, _ = validate_authoring(yaml_dir)

    internal_misses = [e for e in errors if "internal ref not found" in e["reason"]]
    assert internal_misses == []


# ---------------------------------------------------------------------------
# Test 6: ESL FormID range constraint
# ---------------------------------------------------------------------------

def test_esl_formid_out_of_range(tmp_path):
    """ESL plugins must keep own FormIDs <= 0x000FFF."""
    yaml_dir = tmp_path / "yaml"
    yaml_dir.mkdir()
    _write_plugin_yaml(yaml_dir, "B21_Test.esl", [])

    record_dir = yaml_dir / "records" / "WEAP"
    record_dir.mkdir(parents=True)
    (record_dir / "OverLimit - 001234_B21_Test.esl.yaml").write_text(
        "signature: WEAP\nform_id: '001234:B21_Test.esl'\nfields: []\n",
        encoding="utf-8",
    )

    errors, _ = validate_authoring(yaml_dir)
    esl_errs = [e for e in errors if "ESL FormID limit" in e["reason"]]
    assert any("001234:B21_Test.esl" in e["formkey"] for e in esl_errs)


# ---------------------------------------------------------------------------
# Test 7: ESL master-type constraint
# ---------------------------------------------------------------------------

def test_esl_rejects_esp_master(tmp_path):
    """ESL plugins must not list .esp masters."""
    yaml_dir = tmp_path / "yaml"
    yaml_dir.mkdir()
    _write_plugin_yaml(yaml_dir, "B21_Test.esl", ["OtherMod.esp"])

    errors, _ = validate_authoring(yaml_dir)
    master_errs = [e for e in errors if "lists an .esp master" in e["reason"]]
    assert len(master_errs) == 1
    assert "OtherMod.esp" in master_errs[0]["reason"]
