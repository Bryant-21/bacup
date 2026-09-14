from __future__ import annotations

import json
from pathlib import Path
from time import monotonic, sleep

from ui.tools.meshes.nif_validation_runner import (
    ValidationCheck,
    collect_validation_files,
    default_validation_check_ids,
    load_validation_checks,
    run_validation_scan,
    write_validation_payload,
)


def _checks():
    return (
        ValidationCheck("geometry", "Geometry", "Meshes", ("nif",), False),
        ValidationCheck("animation", "Animation", "Meshes", ("nif", "kf"), False),
        ValidationCheck("uvs", "UVs", "Meshes", ("nif",), True),
        ValidationCheck("dds", "Textures", "Textures", ("dds",), False),
    )


def test_loads_native_check_catalog_and_defaults_to_nonoptional():
    checks = load_validation_checks(
        lambda: {
            "checks": [
                {
                    "id": "geometry",
                    "title": "Geometry",
                    "group": "Meshes",
                    "extensions": ["nif"],
                    "optional": False,
                },
                {
                    "id": "uvs",
                    "title": "UVs",
                    "group": "Meshes",
                    "extensions": [".nif"],
                    "optional": True,
                },
            ]
        }
    )

    assert checks[1].extensions == ("nif",)
    assert default_validation_check_ids(checks) == {"geometry"}


def test_collects_selected_file_types_recursively_with_path_filter(tmp_path):
    wanted = tmp_path / "actors" / "alien.nif"
    skipped_path = tmp_path / "props" / "crate.nif"
    skipped_type = tmp_path / "actors" / "alien.dds"
    for path in (wanted, skipped_path, skipped_type):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"data")

    files = collect_validation_files(
        tmp_path,
        _checks(),
        {"geometry"},
        recursive=True,
        path_contains="actors",
    )

    assert files == (wanted.resolve(),)


def test_scan_routes_file_types_filters_checks_and_builds_viewable_report(tmp_path):
    nif_path = tmp_path / "mesh.nif"
    kf_path = tmp_path / "anim.kf"
    dds_path = tmp_path / "texture.dds"
    for path in (nif_path, kf_path, dds_path):
        path.write_bytes(b"data")
    nif_calls = []
    dds_calls = []
    progress = []

    def validate_nif(path, output_path, fix, include_optional):
        nif_calls.append((Path(path), output_path, fix, include_optional))
        return {
            "game": "fo4",
            "changed": False,
            "changes": [],
            "warnings": [],
            "findings": [
                {
                    "severity": "warning",
                    "rule": "duplicate-vertices",
                    "check": "geometry",
                    "message": "duplicate",
                },
                {
                    "severity": "info",
                    "rule": "clamped",
                    "check": "uvs",
                    "message": "optional",
                },
            ],
        }

    def validate_dds(path, *, include_optional):
        dds_calls.append((Path(path), include_optional))
        return {
            "game": "dds",
            "changed": False,
            "changes": [],
            "warnings": [],
            "findings": [
                {
                    "severity": "error",
                    "rule": "bad-dds",
                    "check": "dds",
                    "message": "bad",
                }
            ],
        }

    result = run_validation_scan(
        tmp_path,
        _checks(),
        {"geometry", "animation", "dds"},
        max_workers=3,
        nif_validator=validate_nif,
        dds_validator=validate_dds,
        on_progress=lambda current, total, item: progress.append(
            (current, total, item["path"])
        ),
    )

    assert {path for path, *_ in nif_calls} == {nif_path, kf_path}
    assert dds_calls == [(dds_path, False)]
    assert all(call[1:] == (None, False, False) for call in nif_calls)
    assert len(progress) == 3
    assert result.report.totals.files_scanned == 3
    assert result.report.totals.files_with_issues == 3
    assert result.report.totals.errors == 1
    assert result.report.totals.warnings == 2
    assert result.report.checks == ("dds", "geometry")
    assert result.payload["summary"]["checks"] == ["animation", "dds", "geometry"]


def test_scan_can_stop_after_first_load_error(tmp_path):
    for name in ("a.nif", "b.nif", "c.nif"):
        (tmp_path / name).write_bytes(b"data")
    calls = []

    def validate(path, output_path, fix, include_optional):
        calls.append(Path(path).name)
        raise RuntimeError("broken")

    result = run_validation_scan(
        tmp_path,
        _checks(),
        {"geometry"},
        skip_errors=False,
        max_workers=1,
        nif_validator=validate,
    )

    assert calls == ["a.nif"]
    assert result.processed == 1
    assert result.payload["summary"]["stopped_on_error"] is True
    assert result.report.totals.load_failures == 1


def test_writes_complete_scan_payload(tmp_path):
    payload = {"summary": {"files": 0}, "results": []}
    destination = write_validation_payload(payload, tmp_path / "reports" / "nifs.json")

    assert json.loads(destination.read_text(encoding="utf-8")) == payload


def test_standalone_tool_runs_scan_and_adopts_results(tmp_path, monkeypatch):
    from ui.tools.meshes.nif_validation_tool import NifValidationReportTool

    nif_path = tmp_path / "mesh.nif"
    nif_path.write_bytes(b"nif")
    scan_result = run_validation_scan(
        nif_path,
        _checks(),
        {"geometry"},
        nif_validator=lambda *args: {
            "game": "fo4",
            "changed": False,
            "changes": [],
            "warnings": [],
            "findings": [
                {
                    "severity": "warning",
                    "rule": "duplicate-vertices",
                    "check": "geometry",
                    "message": "duplicate",
                }
            ],
        },
    )
    tool = NifValidationReportTool()
    tool.input_path = str(nif_path)
    tool.check_catalog = _checks()
    tool.enabled_check_ids = {"geometry"}
    monkeypatch.setattr(tool, "_execute_scan", lambda *args: scan_result)

    tool._start_scan()
    deadline = monotonic() + 2.0
    while tool._scan_future is not None and not tool._scan_future.done():
        assert monotonic() < deadline
        sleep(0.01)
    tool._poll_scan()

    assert tool.report is scan_result.report
    assert tool.report_payload is scan_result.payload
    assert tool.filtered_files[0].path == str(nif_path)
    assert "Finished" in tool.scan_status
    assert tool.fix_output_folder == str(tmp_path / "nif_validation_fixed")
    tool.cleanup()


def test_standalone_tool_defaults_to_recommended_checks_and_persists_user_choice(
    monkeypatch,
):
    from ui.tools.meshes import nif_validation_tool

    monkeypatch.setattr(nif_validation_tool, "load_validation_checks", _checks)
    tool = nif_validation_tool.NifValidationReportTool()
    tool.initialize()

    assert tool.enabled_check_ids == {"geometry", "animation", "dds"}

    restored = nif_validation_tool.NifValidationReportTool()
    restored.apply_settings({"enabled_check_ids": ["uvs"]})
    restored.initialize()

    assert restored.enabled_check_ids == {"uvs"}
