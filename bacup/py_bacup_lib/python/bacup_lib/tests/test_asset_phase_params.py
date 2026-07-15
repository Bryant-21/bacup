from __future__ import annotations

from types import SimpleNamespace

from bacup_lib.models import AssetRef, DependencyGraph, RecordNode
from bacup_lib.workflows.asset_phases import (
    _params_for_convert_btos,
    _params_for_convert_havok,
    _params_for_convert_nifs,
)
from bacup_lib.workflows.unified import _wave_plan_for


def _graph(*assets: AssetRef) -> DependencyGraph:
    root = RecordNode(
        form_key="SeventySix.esm:000800",
        editor_id="B21_Test",
        record_type="STAT",
        assets=list(assets),
    )
    return DependencyGraph(root=root, all_records=[root], all_assets=list(assets), errors=[])


def _orchestrator(*assets: AssetRef):
    return SimpleNamespace(
        source_game="fo76",
        target_game="fo4",
        overwrite_existing=False,
        conversion_workers=None,
        disable_nif_collision_memo=False,
        emit_first_person=False,
        morph_weight_cap=0.5,
        base_asset_namespace="",
        _source_profile=None,
        _target_profile=None,
        _addon_index_map={},
        graph=_graph(*assets),
        _expand_animation_dirs=lambda _root: [],
        _load_target_behavior_paths=lambda: set(),
    )


def test_params_for_convert_nifs_forwards_addon_index_map(tmp_path):
    asset = AssetRef("nif", "Meshes/effects/x.nif", str(tmp_path / "x.nif"))
    orch = _orchestrator(asset)
    orch._addon_index_map = {78: 760001}

    params = _params_for_convert_nifs(orch, [asset])

    assert params["addon_index_map"] == {"78": 760001}
    assert params["nif_paths"] == [
        {"source_path": "Meshes/effects/x.nif", "resolved_path": str(tmp_path / "x.nif")}
    ]


def test_params_for_convert_btos_can_disable_collision_memo(tmp_path):
    asset = AssetRef(
        "bto",
        "Meshes/Terrain/Appalachia/Appalachia.0.0.0.bto",
        str(tmp_path / "x.bto"),
    )
    orch = _orchestrator(asset)
    orch.disable_nif_collision_memo = True

    params = _params_for_convert_btos(orch, [asset])

    assert params["disable_collision_memo"] is True
    assert params["bto_paths"] == [
        {
            "source_path": "Meshes/Terrain/Appalachia/Appalachia.0.0.0.bto",
            "resolved_path": str(tmp_path / "x.bto"),
        }
    ]


def test_params_for_convert_havok_includes_nif_assets(tmp_path):
    hkx = AssetRef(
        "behavior",
        "Meshes/Actors/Mirelurk/characterassets/seaweed.hkx",
        str(tmp_path / "seaweed.hkx"),
    )
    nif = AssetRef(
        "nif",
        "Meshes/Actors/Mirelurk/characterassets/seaweed.nif",
        str(tmp_path / "seaweed.nif"),
    )
    orch = _orchestrator(hkx, nif)
    orch.additional_source_asset_roots = (
        tmp_path / "extracted" / "fo3",
        tmp_path / "extracted" / "fo3-override",
    )

    params = _params_for_convert_havok(orch)

    assert params["hkx_assets"] == [
        {
            "source_path": hkx.source_path,
            "resolved_path": hkx.resolved_path,
            "asset_type": "behavior",
        }
    ]
    assert params["nif_assets"] == [
        {"source_path": nif.source_path, "resolved_path": nif.resolved_path}
    ]
    assert params["additional_source_asset_roots"] == [
        str(tmp_path / "extracted" / "fo3"),
        str(tmp_path / "extracted" / "fo3-override"),
    ]


def test_fnv_world_static_plan_intentionally_leaves_actor_a4_disabled():
    assert _wave_plan_for("fnv").wave_a4 is False
