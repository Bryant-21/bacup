from __future__ import annotations

import json
from types import SimpleNamespace

import pytest


@pytest.mark.parametrize(
    "source,target,havok,plan,creature_policy,expected",
    [
        ("fo76", "fo4", True, True, None, True),
        ("fo76", "fo4", False, True, None, False),
        ("fo76", "fo4", True, False, None, False),
        ("skyrimse", "fo4", True, True, None, False),
        ("fo76", "starfield", True, True, None, False),
        ("fo76", "fo4", True, True, {"policy_id": "all_creatures_v1"}, False),
    ],
)
def test_rebuilding_fo76_graphs_requires_fresh_animtext(
    tmp_path, source, target, havok, plan, creature_policy, expected
):
    from bacup_lib.workflows.unified import _requires_fo76_behavior_animtext

    if plan:
        path = tmp_path / "debug/fo76_behaviors/plan.json"
        path.parent.mkdir(parents=True)
        path.write_text('{"routes": []}')
    ctx = SimpleNamespace(
        source_game=source, target_game=target, mod_path=tmp_path,
        mvp_creature_corpus_policy=creature_policy,
    )
    options = SimpleNamespace(convert_havok=havok, generate_anim_text_data=False)
    assert _requires_fo76_behavior_animtext(ctx, options) is expected
    assert not _requires_fo76_behavior_animtext(None, options)


def test_fo76_animtext_stays_native_even_with_creation_kit(tmp_path, monkeypatch):
    from bacup_lib.workflows import unified

    game = tmp_path / "game"
    (game / "Data").mkdir(parents=True)
    (game / "CreationKit.exe").write_bytes(b"present")
    mod = tmp_path / "mod"
    stale = mod / "data/Meshes/AnimTextData/stale.txt"
    stale.parent.mkdir(parents=True)
    stale.write_text("old graph bindings")
    plugin = mod / "SeventySix.esm"
    plugin.write_bytes(b"plugin")
    calls = []

    def native(ctx, runner, plugin_path, **kwargs):
        assert not stale.exists()
        calls.append(plugin_path)

    def ck(*args, **kwargs):
        pytest.fail("FO76 behavior graphs require the native generator")

    monkeypatch.setattr(unified, "_run_anim_text_data_native", native)
    monkeypatch.setattr(unified, "_run_anim_text_data_via_ck", ck)
    ctx = SimpleNamespace(
        source_game="fo76", target_game="fo4", mod_path=mod,
        output_plugin_name=plugin.name, target_data_dir=game / "Data",
    )
    unified._run_anim_text_data_generation(ctx, StubRunner())
    assert calls == [plugin]


class StubRunner:
    def __init__(self) -> None:
        self.logs: list[tuple[str, str]] = []
        self.item_progress: list[str] = []

    def emit_log(self, level: str, message: str) -> None:
        self.logs.append((level, message))

    def emit_item_progress(self, progress) -> None:
        self.item_progress.append(progress.current_item)


def test_creature_animtext_uses_combined_native_contract_and_marshals_receipt(
    tmp_path, monkeypatch
):
    from bacup_lib import native_runtime
    from bacup_lib.models import PhaseProgress
    from bacup_lib.workflows.unified import _run_anim_text_data_native

    mod_dir = tmp_path / "mod"
    meshes = mod_dir / "data" / "Meshes"
    meshes.mkdir(parents=True)
    plugin = mod_dir / "Target.esp"
    plugin.write_bytes(b"plugin")
    base_plugin = tmp_path / "Fallout4.esm"
    catalog = tmp_path / "target-assets.sqlite3"
    cache = tmp_path / "cache"
    overlay = tmp_path / "overlay"
    plan = tmp_path / "plan.json"
    execution = tmp_path / "execution.json"
    records = tmp_path / "records.json"
    calls = []
    receipt_payload = {
        "version": 1,
        "policy_id": "all_creatures_v1",
        "written": 2,
        "families": [
            {
                "family_id": "family-canis",
                "status": "passed",
                "race_form_keys": ["000800@Target.esp"],
                "graph_paths": ["Actors/Canis/Behaviors/Canis.hkx"],
                "emitted_files": [
                    "AnimTextData/AnimationFileData/Canis.txt",
                    "AnimTextData/AnimEventInfo/Canis.txt",
                ],
                "attack_events": [
                    {
                        "event": "AttackPrimary",
                        "race_form_key": "000800@Target.esp",
                        "atkd_index": 0,
                        "source": "emitted_race_atkd_atke",
                        "targets": [
                            {
                                "graph_path": "Actors/Canis/Behaviors/Canis.hkx",
                                "clip_name": "AttackPrimary",
                                "annotation": "AttackPrimary",
                            }
                        ],
                    }
                ],
            }
        ],
    }

    class FakeNative:
        def conversion_generate_anim_text_data(
            self, target, game, bases, source, output, **kwargs
        ):
            calls.append((target, game, bases, source, output, kwargs))
            kwargs["progress_callback"]("creature AnimText: published family-canis")
            return 2, json.dumps(receipt_payload)

    monkeypatch.setattr(native_runtime, "load_native_module", lambda: FakeNative())
    ctx = SimpleNamespace(
        target_game="fo4",
        target_data_dir=tmp_path / "Fallout 4" / "Data",
        target_extracted_dir=overlay,
        target_asset_store=object(),
        target_asset_catalog_path=catalog,
        target_asset_cache_dir=cache,
        anim_text_data_base_race_path=base_plugin,
        mod_prefix="B21",
        conversion_workers=6,
    )
    runner = StubRunner()
    progress = PhaseProgress(phase=0, phase_name="AnimText", status="running")

    receipt = _run_anim_text_data_native(
        ctx,
        runner,
        plugin,
        progress=progress,
        creature_family_ids=("family-canis",),
        creature_corpus_plan=plan,
        creature_execution_ledger=execution,
        creature_record_commit_ledger=records,
    )

    assert receipt.to_dict() == receipt_payload
    assert len(calls) == 1
    target, game, bases, source, output, kwargs = calls[0]
    assert (target, game, bases) == (
        str(plugin),
        "fo4",
        [str(base_plugin)],
    )
    assert source == output == str(meshes)
    assert kwargs["base_meshes_root"] is None
    assert kwargs["target_data_dir"] == str(ctx.target_data_dir)
    assert kwargs["target_catalog_path"] == str(catalog)
    assert kwargs["target_cache_dir"] == str(cache)
    assert kwargs["target_overlay_dir"] == str(overlay)
    assert kwargs["mod_prefix"] == "B21"
    assert kwargs["workers"] == 6
    assert json.loads(kwargs["creature_contract_json"]) == {
        "version": 1,
        "policy_id": "all_creatures_v1",
        "game": "fo4",
        "plugin_path": str(plugin),
        "source_meshes_root": str(meshes),
        "output_meshes_root": str(meshes),
        "base_meshes_root": None,
        "base_plugin_paths": [str(base_plugin)],
        "corpus_plan_path": str(plan),
        "execution_ledger_path": str(execution),
        "record_commit_ledger_path": str(records),
        "expected_family_ids": ["family-canis"],
        "event_source": "emitted_race_atkd_atke",
        "resolution_source": "emitted_family_graphs",
        "mod_prefix": "B21",
    }
    assert runner.item_progress == ["creature AnimText: published family-canis"]
    assert runner.logs[-1] == (
        "INFO",
        "animtext: wrote 2 AnimTextData bucket file(s)",
    )
