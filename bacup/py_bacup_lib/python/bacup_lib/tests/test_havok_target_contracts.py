from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.workflows.asset_phases import phase_postprocess_havok_native
from bacup_lib.workflows.unified import AssetWaveBuilder, AssetWaveToggles


WEAPON = "meshes/actors/character/behaviors/weaponbehavior.hkx"
CONTRACTS = [
    WEAPON,
    "meshes/actors/character/behaviors/workbenchfurniturebehavior.hkx",
    "meshes/actors/robobrain/characters/character.hkx",
    "meshes/actors/createabot/characterassets/skeleton.hkx",
]
ANIMATION = "meshes/actors/character/animations/idle.hkx"


class IndexedAssets:
    def __init__(self, root: Path):
        self.cache_data_root = root
        self.requests = []

    def list_assets(self, *, prefix, suffix):
        assert (prefix, suffix) == ("meshes/", ".hkx")
        return [*CONTRACTS, ANIMATION]

    def materialize_many(self, paths):
        self.requests.extend(paths)
        outputs = []
        for relative in paths:
            path = self.cache_data_root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"contract")
            outputs.append(path)
        return outputs


def _orchestrator(tmp_path, extracted):
    mod = tmp_path / "mod"
    plan = mod / "debug/fo76_behaviors/plan.json"
    plan.parent.mkdir(parents=True)
    plan.write_text('{"routes": [], "projects": {}}', encoding="utf-8")
    calls = []

    def run_phase(name, **kwargs):
        calls.append((name, kwargs))
        return {"assets_written": 0, "warnings": 0}

    return SimpleNamespace(
        source_game="fo76", target_game="fo4", mod_path=mod,
        source_data_dir=tmp_path / "source", target_extracted_dir=extracted,
        target_data_dir=tmp_path / "Fallout4/Data",
        target_asset_store=IndexedAssets(tmp_path / "cache/Data"),
        _rust_conversion_run=SimpleNamespace(id=1, run_phase=run_phase),
        _summary=SimpleNamespace(havok_converted=0, havok_failed=0),
        _source_profile=SimpleNamespace(engine="creation1"),
        _target_profile=SimpleNamespace(engine="creation1"),
        calls=calls,
    )


@pytest.mark.parametrize("partial_extraction", [False, True])
def test_postprocess_materializes_indexed_contracts(tmp_path, partial_extraction):
    extracted = tmp_path / "partial-extraction" if partial_extraction else None
    if extracted is not None:
        extracted.mkdir()
    orchestrator = _orchestrator(tmp_path, extracted)
    runner = SimpleNamespace(emit_log=lambda *_args: None)

    phase_postprocess_havok_native(orchestrator, runner, SimpleNamespace())

    store = orchestrator.target_asset_store
    assert orchestrator.calls[0][1]["target_extracted_dir"] == str(store.cache_data_root)
    assert set(store.requests) == set(CONTRACTS)
    assert not (store.cache_data_root / ANIMATION).exists()
    assert orchestrator.target_extracted_dir == extracted


def test_havok_wave_supplies_materialized_contract_root(tmp_path, monkeypatch):
    orchestrator = _orchestrator(tmp_path, None)
    driver = SimpleNamespace(ctx=orchestrator)
    runs = SimpleNamespace(havok=orchestrator._rust_conversion_run, nifs=None, textures=None)
    builder = AssetWaveBuilder(
        driver, AssetWaveToggles(drivers=False, animations=False), runs,
        SimpleNamespace(emit_log=lambda *_args: None),
    )
    monkeypatch.setattr(builder, "_shim", lambda: orchestrator)
    monkeypatch.setattr(
        "bacup_lib.workflows.asset_phases._params_for_convert_havok", lambda _shim: {}
    )

    stages = builder.build_wave_a4()

    postprocess = next(stage for stage in stages if stage.phase == "postprocess_havok_assets")
    assert postprocess.target_extracted_dir == str(orchestrator.target_asset_store.cache_data_root)
    assert set(orchestrator.target_asset_store.requests) == set(CONTRACTS)


def test_missing_indexed_contract_fails_before_native_dispatch(tmp_path):
    orchestrator = _orchestrator(tmp_path, None)
    orchestrator.target_asset_store.materialize_many = lambda paths: []

    with pytest.raises(RuntimeError, match="FO4.*contract"):
        phase_postprocess_havok_native(
            orchestrator, SimpleNamespace(emit_log=lambda *_args: None), SimpleNamespace()
        )
    assert orchestrator.calls == []
