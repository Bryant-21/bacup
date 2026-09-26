"""Tests for conversion data models."""
from __future__ import annotations

import pytest


def test_write_coverage_report_accepts_native_dict_decisions(tmp_path):
    from bacup_lib.models import (
        ConversionDecision,
        ConversionDecisionKind,
        write_coverage_report,
    )

    out_path = tmp_path / "conversion_report.md"
    write_coverage_report(
        out_path,
        decisions=[
            {"kind": "skip_records", "message": "sig NAVM in skip_records"},
            {
                "kind": "unmapped_drop",
                "record_type": "WEAP",
                "field": "DNAM.Unknown",
            },
            ConversionDecision(
                ConversionDecisionKind.UNMAPPED_DROP,
                "MISC",
                "DATA.Unknown",
                "schema_gap",
            ),
        ],
        translated_counts={},
        skipped_counts={},
        failed_nifs=[],
        failed_textures=[],
        failed_bgsms=[],
    )

    report = out_path.read_text(encoding="utf-8")
    assert "WEAP.DNAM.Unknown" in report
    assert "MISC.DATA.Unknown" in report


def test_write_coverage_report_includes_asset_summary(tmp_path):
    from bacup_lib.models import ConversionSummary, write_coverage_report

    summary = ConversionSummary(
        nifs_total=12,
        nifs_converted=10,
        nifs_failed=2,
        textures_total=8,
        textures_converted=7,
        textures_failed=1,
    )
    out_path = tmp_path / "conversion_report.md"

    write_coverage_report(
        out_path,
        decisions=[],
        translated_counts={},
        skipped_counts={},
        failed_nifs=["Meshes/missing.nif"],
        failed_textures=["terrain bundle missing"],
        failed_bgsms=[],
        asset_summary=summary,
    )

    report = out_path.read_text(encoding="utf-8")
    assert "## Asset outcomes" in report
    assert "NIF | 12 | 10 | 0 | 2" in report
    assert "Texture | 8 | 7 | 0 | 1" in report
    assert "Meshes/missing.nif" in report


def test_write_provenance_files_ancestor_summary(tmp_path):
    """ancestor_counts keys on depth-1 ancestor, not the immediate adder."""
    from bacup_lib.models import (
        AssetProvenance,
        AssetRef,
        DependencyGraph,
        RecordNode,
        RecordProvenance,
    )

    root = RecordNode("000001:Test.esm", "RootWeapon", "WEAP")

    depth1 = RecordNode(
        "000002:Test.esm",
        "DepthOneRecord",
        "RACE",
        provenance=RecordProvenance(
            added_by_record_fk="000001:Test.esm",
            added_by_record_eid="RootWeapon",
            added_by_field="Race",
            walk_depth=1,
            walker_pass="main",
        ),
    )

    depth2 = RecordNode(
        "000003:Test.esm",
        "DepthTwoRecord",
        "NPC_",
        provenance=RecordProvenance(
            added_by_record_fk="000002:Test.esm",
            added_by_record_eid="DepthOneRecord",
            added_by_field="DefaultOutfit",
            walk_depth=2,
            walker_pass="main",
        ),
    )

    asset_via_depth2 = AssetRef(
        asset_type="nif",
        source_path="Meshes/deep.nif",
        provenance=AssetProvenance(
            added_by_record_fk="000003:Test.esm",
            added_by_record_eid="DepthTwoRecord",
            added_by_field="Model",
            walk_depth=2,
            walker_pass="main",
        ),
    )

    asset_via_depth1 = AssetRef(
        asset_type="texture",
        source_path="Textures/shallow.dds",
        provenance=AssetProvenance(
            added_by_record_fk="000002:Test.esm",
            added_by_record_eid="DepthOneRecord",
            added_by_field="Texture",
            walk_depth=1,
            walker_pass="main",
        ),
    )

    graph = DependencyGraph(
        root=root,
        all_records=[root, depth1, depth2],
        all_assets=[asset_via_depth2, asset_via_depth1],
        errors=[],
    )

    counts = graph.write_provenance_files(str(tmp_path))

    # Both assets should roll up to the single depth-1 ancestor
    assert len(counts) == 1
    assert "DepthOneRecord (000002:Test.esm)" in counts
    assert counts["DepthOneRecord (000002:Test.esm)"] == 2


@pytest.mark.parametrize(("cpu_count", "expected"), [(8, 4), (None, 1)])
def test_auto_conversion_worker_count(monkeypatch, cpu_count, expected):
    from bacup_lib.models import auto_conversion_worker_count

    monkeypatch.setattr("bacup_lib.models.os.cpu_count", lambda: cpu_count)

    assert auto_conversion_worker_count() == expected
