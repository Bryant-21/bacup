from __future__ import annotations

import json
from pathlib import Path

import pytest

from ui.tools.meshes.nif_validation_report import (
    ReportFilters,
    ValidationReport,
    build_validation_fix_plan,
    filter_issue_files,
    resolve_issue_source,
    run_validation_fix_plan,
    select_fix_issue_files,
    sort_issue_files,
)


def _finding(
    severity: str,
    rule: str,
    check: str,
    message: str,
) -> dict:
    return {
        "severity": severity,
        "rule": rule,
        "check": check,
        "block_id": 4,
        "block_type": "BSTriShape",
        "field": "Vertex Data",
        "message": message,
    }


def test_parses_issue_only_report_and_summary_counts():
    report = ValidationReport.from_dict(
        {
            "summary": {
                "files_scanned": 10,
                "files_with_issues": 2,
                "load_failures": 0,
                "findings_by_severity": {"error": 1, "warning": 2, "info": 3},
            },
            "issues": [
                {
                    "path": r"data\Meshes\actors\mesh.nif",
                    "game": "fo4",
                    "findings": [
                        _finding(
                            "warning",
                            "duplicate-vertices",
                            "invalid-geometry",
                            "Duplicate vertices",
                        )
                    ],
                }
            ],
        }
    )

    assert report.totals.files_scanned == 10
    assert report.totals.files_with_issues == 2
    assert report.totals.errors == 1
    assert report.totals.warnings == 2
    assert report.totals.info == 3
    assert report.rules == ("duplicate-vertices",)
    assert report.checks == ("invalid-geometry",)
    assert report.issue_files[0].highest_severity == "warning"


def test_parses_full_cli_report_omits_clean_files_and_keeps_failures():
    report = ValidationReport.from_dict(
        {
            "summary": {
                "files": 3,
                "files_with_findings": 1,
                "findings": {"warning": 1},
                "failures": [{"path": "broken.nif", "error": "bad header"}],
            },
            "results": [
                {
                    "path": "clean.nif",
                    "success": True,
                    "game": "fo4",
                    "findings": [],
                    "warnings": [],
                },
                {
                    "path": "warning.nif",
                    "success": True,
                    "game": "fo4",
                    "findings": [],
                    "warnings": ["Non-fatal parser warning"],
                },
                {
                    "path": "broken.nif",
                    "success": False,
                    "error": "bad header",
                    "findings": [],
                    "warnings": [],
                },
            ],
        }
    )

    assert report.totals.files_scanned == 3
    assert report.totals.load_failures == 1
    assert [item.path for item in report.issue_files] == ["warning.nif", "broken.nif"]
    assert report.issue_files[0].findings[0].rule == "runtime-warning"
    assert report.issue_files[1].load_error == "bad header"


def test_filters_text_severity_rule_check_and_load_failures():
    report = ValidationReport.from_dict(
        {
            "summary": {},
            "issues": [
                {
                    "path": r"actors\alien.nif",
                    "game": "fo4",
                    "findings": [
                        _finding(
                            "warning",
                            "duplicate-vertices",
                            "invalid-geometry",
                            "Duplicate vertices",
                        ),
                        _finding(
                            "error",
                            "asset-path",
                            "texture-set-slots",
                            "Absolute texture path",
                        ),
                    ],
                },
                {
                    "path": r"props\crate.nif",
                    "game": "fo4",
                    "findings": [
                        _finding("info", "unused-block", "unused-blocks", "Unused node")
                    ],
                },
                {
                    "path": r"broken\mesh.nif",
                    "success": False,
                    "error": "Unsupported header",
                    "findings": [],
                },
            ],
        }
    )

    assert [
        item.path
        for item in filter_issue_files(
            report.issue_files, ReportFilters(text="absolute texture")
        )
    ] == [r"actors\alien.nif"]
    assert [
        item.path
        for item in filter_issue_files(
            report.issue_files,
            ReportFilters(
                severity="warning",
                rule="duplicate-vertices",
                check="invalid-geometry",
            ),
        )
    ] == [r"actors\alien.nif"]
    assert [
        item.path
        for item in filter_issue_files(
            report.issue_files, ReportFilters(load_failures="only")
        )
    ] == [r"broken\mesh.nif"]
    assert [
        item.path
        for item in filter_issue_files(
            report.issue_files, ReportFilters(severity="error", load_failures="only")
        )
    ] == [r"broken\mesh.nif"]


def test_sorts_issue_files_by_table_fields():
    report = ValidationReport.from_dict(
        {
            "summary": {},
            "issues": [
                {
                    "path": "z.nif",
                    "game": "fo4",
                    "findings": [
                        _finding("warning", "rule-b", "check-b", "one"),
                        _finding("info", "rule-a", "check-a", "two"),
                    ],
                },
                {
                    "path": "a.nif",
                    "game": "fo76",
                    "findings": [_finding("error", "rule-c", "check-c", "three")],
                },
            ],
        }
    )

    assert [item.path for item in sort_issue_files(report.issue_files, "path")] == [
        "a.nif",
        "z.nif",
    ]
    assert [
        item.path
        for item in sort_issue_files(report.issue_files, "severity", descending=True)
    ] == ["a.nif", "z.nif"]
    assert [
        item.path
        for item in sort_issue_files(report.issue_files, "findings", descending=True)
    ] == ["z.nif", "a.nif"]

    with pytest.raises(ValueError, match="Unknown report sort field"):
        sort_issue_files(report.issue_files, "missing")


def _issue_report(source_path, scan_path, issue_paths):
    return ValidationReport.from_dict(
        {
            "summary": {"scan_path": str(scan_path), "include_optional": False},
            "issues": [
                {
                    "path": str(path),
                    "game": "fo4",
                    "findings": [
                        _finding(
                            "warning",
                            "duplicate-vertices",
                            "invalid-geometry",
                            "Duplicate vertices",
                        )
                    ],
                }
                for path in issue_paths
            ],
        },
        source_path=source_path,
    )


def test_resolves_issue_only_relative_and_full_cli_absolute_paths(tmp_path):
    mod_root = tmp_path / "SeventySix"
    scan_root = mod_root / "data" / "Meshes"
    relative_source = scan_root / "actors" / "alien.nif"
    relative_source.parent.mkdir(parents=True)
    relative_source.write_bytes(b"nif")
    report = _issue_report(
        mod_root / "NIF_ISSUES_REPORT.json",
        scan_root,
        [Path("data") / "Meshes" / "actors" / "alien.nif"],
    )

    assert resolve_issue_source(report, report.issue_files[0]) == relative_source

    absolute_source = scan_root / "props" / "crate.nif"
    absolute_source.parent.mkdir(parents=True)
    absolute_source.write_bytes(b"nif")
    full_report = ValidationReport.from_dict(
        {
            "summary": {"path": str(scan_root)},
            "results": [
                {
                    "path": str(absolute_source),
                    "game": "fo4",
                    "success": True,
                    "findings": [
                        _finding("warning", "asset-path", "texture-set-slots", "Path")
                    ],
                }
            ],
        },
        source_path=tmp_path / "full.json",
    )

    assert resolve_issue_source(full_report, full_report.issue_files[0]) == absolute_source


def test_builds_in_place_and_mirrored_output_targets(tmp_path):
    mod_root = tmp_path / "SeventySix"
    scan_root = mod_root / "data" / "Meshes"
    source = scan_root / "actors" / "alien.nif"
    source.parent.mkdir(parents=True)
    source.write_bytes(b"nif")
    report = _issue_report(
        mod_root / "report.json",
        scan_root,
        [Path("data") / "Meshes" / "actors" / "alien.nif"],
    )

    in_place = build_validation_fix_plan(
        report,
        report.issue_files,
        in_place=True,
    )
    output_root = tmp_path / "fixed"
    mirrored = build_validation_fix_plan(
        report,
        report.issue_files,
        in_place=False,
        output_root=output_root,
    )

    assert in_place[0].source == source
    assert in_place[0].target == source
    assert mirrored[0].source == source
    assert mirrored[0].target == output_root / "data" / "Meshes" / "actors" / "alien.nif"


def test_full_cli_absolute_path_mirrors_relative_to_scan_root(tmp_path):
    scan_root = tmp_path / "scan" / "Meshes"
    source = scan_root / "actors" / "alien.nif"
    source.parent.mkdir(parents=True)
    source.write_bytes(b"nif")
    report = ValidationReport.from_dict(
        {
            "summary": {"path": str(scan_root)},
            "results": [
                {
                    "path": str(source),
                    "success": True,
                    "findings": [_finding("warning", "rule", "check", "message")],
                }
            ],
        },
        source_path=tmp_path / "full.json",
    )

    plan = build_validation_fix_plan(
        report,
        report.issue_files,
        in_place=False,
        output_root=tmp_path / "fixed",
    )

    assert plan[0].target == tmp_path / "fixed" / "actors" / "alien.nif"


def test_mirrored_fix_creates_parents_without_changing_source(tmp_path):
    mod_root = tmp_path / "SeventySix"
    scan_root = mod_root / "data" / "Meshes"
    source = scan_root / "actors" / "alien.nif"
    source.parent.mkdir(parents=True)
    source.write_bytes(b"original")
    report = _issue_report(
        mod_root / "report.json",
        scan_root,
        [Path("data") / "Meshes" / "actors" / "alien.nif"],
    )
    plan = build_validation_fix_plan(
        report,
        report.issue_files,
        in_place=False,
        output_root=tmp_path / "fixed",
    )

    def fixer(path, output_path, fix, include_optional):
        assert (fix, include_optional) == (True, False)
        Path(output_path).write_bytes(Path(path).read_bytes() + b" fixed")
        return {"changed": True, "changes": ["fixed"]}

    result = run_validation_fix_plan(plan, max_workers=1, fixer=fixer)

    assert result.changed == 1
    assert source.read_bytes() == b"original"
    assert plan[0].target.read_bytes() == b"original fixed"


def test_selects_selected_file_or_all_filtered_files():
    report = _issue_report(
        Path("report.json"),
        Path("Meshes"),
        [Path("a.nif"), Path("b.nif")],
    )
    selected = report.issue_files[1]

    assert select_fix_issue_files(
        selected,
        report.issue_files,
        "selected",
    ) == (selected,)
    assert select_fix_issue_files(
        selected,
        report.issue_files,
        "filtered",
    ) == report.issue_files
    assert select_fix_issue_files(
        selected,
        report.issue_files,
        "all",
    ) == report.issue_files


def test_fix_failures_do_not_stop_other_files(tmp_path):
    scan_root = tmp_path / "Meshes"
    paths = [scan_root / f"{name}.nif" for name in ("good-a", "bad", "good-b")]
    for path in paths:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"nif")
    report = _issue_report(tmp_path / "report.json", scan_root, paths)
    plan = build_validation_fix_plan(report, report.issue_files, in_place=True)
    calls = []

    def fixer(path, output_path, fix, include_optional):
        calls.append((path, output_path, fix, include_optional))
        if Path(path).stem == "bad":
            raise RuntimeError("cannot repair")
        return {"changed": True, "changes": ["fixed"]}

    result = run_validation_fix_plan(
        plan,
        include_optional=True,
        max_workers=2,
        fixer=fixer,
    )

    assert len(calls) == 3
    assert result.succeeded == 2
    assert result.changed == 2
    assert result.failed == 1
    assert [file.error for file in result.files if not file.success] == [
        "cannot repair"
    ]
    assert all(call[1] == call[0] for call in calls)
    assert all(call[2:] == (True, True) for call in calls)


def test_in_place_request_waits_for_explicit_confirmation():
    from ui.tools.meshes.nif_validation_tool import NifValidationReportTool

    report = _issue_report(
        Path("report.json"),
        Path("Meshes"),
        [Path("alien.nif")],
    )
    panel = NifValidationReportTool()
    panel.report = report
    panel.filtered_files = list(report.issue_files)
    panel._selected_issue = report.issue_files[0]
    panel.fix_mode = "in_place"
    started = []
    panel._start_fix = lambda files: started.append(files)

    panel._request_fix("selected")

    assert started == []
    assert panel._pending_in_place == (report.issue_files[0],)
    assert panel._open_in_place_confirmation is True


def test_loading_report_sets_safe_default_mirrored_output_root(tmp_path):
    from ui.tools.meshes.nif_validation_tool import NifValidationReportTool

    report_path = tmp_path / "report.json"
    report_path.write_text(
        json.dumps(
            {
                "summary": {},
                "issues": [
                    {
                        "path": "data/Meshes/alien.nif",
                        "findings": [
                            _finding("warning", "rule", "check", "message")
                        ],
                    }
                ],
            }
        ),
        encoding="utf-8",
    )
    tool = NifValidationReportTool()

    assert tool.load_report(report_path) is True
    assert tool.fix_mode == "output"
    assert tool.fix_output_folder == str(tmp_path / "nif_validation_fixed")


def test_batch_repair_skips_audit_only_dds_files():
    from ui.tools.meshes.nif_validation_tool import NifValidationReportTool

    report = _issue_report(
        Path("report.json"),
        Path("assets"),
        [Path("mesh.nif"), Path("texture.dds")],
    )
    tool = NifValidationReportTool()
    tool.report = report
    tool.filtered_files = list(report.issue_files)
    tool.fix_mode = "output"
    tool.fix_output_folder = "fixed"
    started = []
    tool._start_fix = lambda files: started.append(files)

    tool._request_fix("all")

    assert [[item.path for item in files] for files in started] == [["mesh.nif"]]
    assert tool._fix_skipped_audit_only == 1
