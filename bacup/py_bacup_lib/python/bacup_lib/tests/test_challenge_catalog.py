import json
from pathlib import Path
from unittest.mock import MagicMock

import pytest

from bacup_lib import challenge_catalog as cat
from bacup_lib.tests.catalog_schema_validation import validate_catalog


def test_catalog_dispatches_native_phase_and_returns_output(tmp_path, monkeypatch):
    source = tmp_path / "source.esm"
    source.write_bytes(b"source")
    converted = tmp_path / "converted"
    converted.mkdir()
    (converted / "Output.esm").write_bytes(b"converted")
    data = tmp_path / "Data"
    data.mkdir()
    catalog = {"schema_version": 1, "source": {"plugin": "SeventySix.esm", "sha256": "abc"},
               "categories": [{"key": "character", "label": "Character"}],
               "challenges": [{"id": "50DBFB", "name": "Test", "kind": "craft", "count": 1, "parent": "50DBFA",
                               "prerequisites": ["437FED"], "sub_challenges": [], "reward": {"xp": 1},
                               "conditions": [], "frequency": "lifetime", "category": "character"},
                              {"id": "50DBFA", "name": "Parent", "kind": "parent", "count": 1, "parent": None,
                               "prerequisites": [], "sub_challenges": ["50DBFB"], "reward": {},
                               "conditions": [], "frequency": "lifetime", "category": "character"}]}
    run = MagicMock()
    run.__enter__.return_value = run
    create = MagicMock(return_value=run)
    monkeypatch.setattr(cat.ConversionRun, "create_new", create)

    def emit(name, **kwargs):
        destination = Path(kwargs["params"]["catalog_path"])
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(catalog), encoding="utf-8")

    run.run_phase.side_effect = emit
    document = cat.emit_challenge_catalog(source, converted, data, "Output.esm", source_sha256="abc")
    assert [row["id"] for row in document["challenges"]] == ["Output.esm:50DBFB", "Output.esm:50DBFA"]
    first = document["challenges"][0]
    assert (first["parent"], first["prerequisites"]) == ("Output.esm:50DBFA", ["Output.esm:437FED"])
    assert document["settings"]["source"]["sha256"] == "abc"
    written = json.loads((converted / "F4SE/Plugins/B21_TalesFromAppalachia/Challenges/Output.esm.json").read_text(encoding="utf-8"))
    assert written == document
    validate_catalog("Challenges", written)
    create.assert_called_once_with("fo76", "fo4", None, "Output.esm")
    run.run_phase.assert_called_once_with(
        "emit_challenge_catalog", mod_path=str(converted), target_data_dir=str(data),
        params={"source_plugin": str(source), "converted_plugin": str(converted / "Output.esm"),
                "catalog_path": str(converted / cat.RAW_CATALOG), "report_path": str(converted / cat.REPORT),
                "source_sha256": "abc"},
    )
    run.__exit__.assert_called_once_with(None, None, None)


@pytest.mark.parametrize("missing", ["source", "converted", "data"])
def test_missing_inputs_fail_before_native_dispatch(tmp_path, monkeypatch, missing):
    source = tmp_path / "source.esm"
    if missing != "source":
        source.write_bytes(b"source")
    if missing != "converted":
        (tmp_path / "SeventySix.esm").write_bytes(b"converted")
    data = tmp_path / "Data"
    if missing != "data":
        data.mkdir()
    create = MagicMock()
    monkeypatch.setattr(cat.ConversionRun, "create_new", create)
    with pytest.raises(FileNotFoundError):
        cat.emit_challenge_catalog(source, tmp_path, data)
    create.assert_not_called()


def test_native_failure_closes_run_without_reading_stale_catalog(tmp_path, monkeypatch):
    source = tmp_path / "source.esm"
    source.write_bytes(b"source")
    (tmp_path / "SeventySix.esm").write_bytes(b"converted")
    run = MagicMock()
    run.__enter__.return_value = run
    run.run_phase.side_effect = RuntimeError("catalog failed")
    monkeypatch.setattr(cat.ConversionRun, "create_new", lambda *args: run)
    with pytest.raises(RuntimeError, match="catalog failed"):
        cat.emit_challenge_catalog(source, tmp_path, None)
    assert run.__exit__.call_args.args[0] is RuntimeError
