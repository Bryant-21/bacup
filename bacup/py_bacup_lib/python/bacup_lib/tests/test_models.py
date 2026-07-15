"""Tests for conversion data models."""
from __future__ import annotations

from pathlib import Path

import pytest


def test_asset_ref_creation():
    from bacup_lib.models import AssetRef

    ref = AssetRef(
        asset_type="nif",
        source_path="Meshes/Weapons/Gun.nif",
        resolved_path="/extracted/Meshes/Weapons/Gun.nif",
    )
    assert ref.asset_type == "nif"
    assert ref.source_path == "Meshes/Weapons/Gun.nif"
    assert ref.resolved_path == "/extracted/Meshes/Weapons/Gun.nif"


def test_asset_ref_is_cdb_ref_defaults_false():
    from bacup_lib.models import AssetRef

    a = AssetRef(asset_type="material", source_path="Materials/foo.mat")
    assert a.is_cdb_ref is False


def test_asset_ref_is_cdb_ref_explicit_true():
    from bacup_lib.models import AssetRef

    a = AssetRef(
        asset_type="material",
        source_path="Materials/foo.mat",
        is_cdb_ref=True,
    )
    assert a.is_cdb_ref is True


def test_asset_ref_unresolved():
    from bacup_lib.models import AssetRef

    ref = AssetRef(asset_type="texture", source_path="Textures/missing.dds")
    assert ref.resolved_path is None


def test_record_node_creation():
    from bacup_lib.models import AssetRef, RecordNode

    node = RecordNode(
        form_key="004822:Fallout4.esm",
        editor_id="10mm",
        record_type="WEAP",
        assets=[AssetRef("nif", "Meshes/Weapons/10mm.nif")],
        children=[],
    )
    assert node.form_key == "004822:Fallout4.esm"
    assert len(node.assets) == 1
    assert len(node.children) == 0


def test_dependency_graph_creation():
    from bacup_lib.models import AssetRef, DependencyGraph, RecordNode

    root = RecordNode(
        form_key="004822:Fallout4.esm",
        editor_id="10mm",
        record_type="WEAP",
        assets=[AssetRef("nif", "Meshes/10mm.nif", "/ext/Meshes/10mm.nif")],
        children=[],
    )
    graph = DependencyGraph(
        root=root,
        all_records=[root],
        all_assets=root.assets[:],
        errors=[],
    )
    assert graph.root is root
    assert len(graph.all_records) == 1
    assert len(graph.all_assets) == 1
    assert len(graph.errors) == 0


def test_extracted_refs():
    from bacup_lib.models import AssetRef, ExtractedRefs

    refs = ExtractedRefs(
        assets=[AssetRef("nif", "Meshes/gun.nif")],
        form_keys=["001234:Fallout4.esm", "005678:Fallout4.esm"],
    )
    assert len(refs.assets) == 1
    assert len(refs.form_keys) == 2


def test_phase_progress():
    from bacup_lib.models import PhaseProgress

    p = PhaseProgress(
        phase=3,
        phase_name="Convert NIFs",
        total_items=5,
        completed_items=2,
        current_item="Meshes/gun.nif",
        status="running",
    )
    assert p.phase == 3
    assert p.error is None
    assert p.elapsed_seconds is None


def test_conversion_summary():
    from bacup_lib.models import ConversionSummary

    s = ConversionSummary()
    assert s.records_translated == 0
    assert s.nifs_converted == 0
    assert s.textures_converted == 0


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


def test_conversion_context_defaults():
    from bacup_lib.models import ConversionContext, ConversionSummary

    ctx = ConversionContext(
        source_game="fnv",
        target_game="fo4",
        mod_path=Path("/tmp/m"),
        output_plugin_name="FalloutNV.esm",
        target_extracted_dir=None,
        target_data_dir=None,
        formkey_mapper=None,
        fixups=None,
        summary=ConversionSummary(),
        converted_plugin_registry=None,
    )
    assert ctx.source_game == "fnv"
    assert ctx.output_plugin_name == "FalloutNV.esm"
    assert ctx.conversion_workers is None


def test_conversion_context_accepts_explicit_worker_count():
    from bacup_lib.models import ConversionContext, ConversionSummary

    ctx = ConversionContext(
        source_game="fnv",
        target_game="fo4",
        mod_path=Path("/tmp/m"),
        output_plugin_name="FalloutNV.esm",
        target_extracted_dir=None,
        target_data_dir=None,
        formkey_mapper=None,
        fixups=None,
        summary=ConversionSummary(),
        conversion_workers=6,
    )

    assert ctx.conversion_workers == 6


def test_plugin_port_request_defaults():
    from bacup_lib.models import PluginPortOptions, PluginPortRequest

    req = PluginPortRequest(
        source_game="fnv",
        target_game="fo4",
        source_plugins=[Path("FalloutNV.esm")],
        output_root=Path("mods"),
        target_extracted_dir=None,
        target_data_dir=None,
        options=PluginPortOptions(),
    )
    assert req.options.translate_records is True
    assert req.options.convert_nifs is False
    assert req.options.build_esp is True
    assert req.options.validate_output is False
    assert req.options.validation_fail_on_error is True


def test_plugin_port_options_terrain_defaults():
    from bacup_lib.models import PluginPortOptions

    opts = PluginPortOptions()
    assert opts.conversion_workers is None
    assert opts.disable_nif_collision_memo is False
    assert opts.fnv_unmapped_function_policy == "halt"
    assert opts.terrain.btd_path == ""
    assert opts.terrain.resample_mode == "lanczos"
    assert opts.terrain.emit_textures is True
    assert opts.placed_record_position_offset == (0.0, 0.0, 0.0)
    assert opts.validate_output is False
    assert opts.validation_fail_on_error is True
    assert opts.validate_collision is False


def test_plugin_port_options_accept_explicit_worker_count():
    from bacup_lib.models import PluginPortOptions

    opts = PluginPortOptions(conversion_workers=4)

    assert opts.conversion_workers == 4


def test_plugin_port_options_accept_unmapped_policy_override():
    from bacup_lib.models import PluginPortOptions

    opts = PluginPortOptions(fnv_unmapped_function_policy="skip_record")

    assert opts.fnv_unmapped_function_policy == "skip_record"


def test_auto_conversion_worker_count_defaults_to_half_cpu(monkeypatch):
    from bacup_lib.models import auto_conversion_worker_count

    monkeypatch.setattr("bacup_lib.models.os.cpu_count", lambda: 8)

    assert auto_conversion_worker_count() == 4


def test_auto_conversion_worker_count_floors_at_one(monkeypatch):
    from bacup_lib.models import auto_conversion_worker_count

    monkeypatch.setattr("bacup_lib.models.os.cpu_count", lambda: None)

    assert auto_conversion_worker_count() == 1


def test_converted_plugin_registry_resolves():
    from bacup_lib.models import ConvertedPluginRegistry

    reg = ConvertedPluginRegistry()
    reg.resolutions["00112233:FalloutNV.esm"] = "00112233:FalloutNV.esm"
    reg.resolutions["00ABCDEF:FalloutNV.esm"] = None
    assert reg.resolutions.get("00112233:FalloutNV.esm") == "00112233:FalloutNV.esm"
    assert reg.resolutions.get("00ABCDEF:FalloutNV.esm") is None
    assert "missing" not in reg.resolutions


def test_plugin_port_options_papyrus_compiler_default_and_set():
    import dataclasses
    from bacup_lib.models import PluginPortOptions

    assert PluginPortOptions().papyrus_compiler == "native"
    opts = PluginPortOptions(papyrus_compiler="exe-batch")
    assert dataclasses.asdict(opts)["papyrus_compiler"] == "exe-batch"


def test_plugin_port_options_asdict_keeps_papyrus_compiler():
    import dataclasses
    from bacup_lib.models import PluginPortOptions

    options = PluginPortOptions(papyrus_compiler="exe-batch")
    assert options.papyrus_compiler == "exe-batch"
    assert dataclasses.asdict(options)["papyrus_compiler"] == "exe-batch"
