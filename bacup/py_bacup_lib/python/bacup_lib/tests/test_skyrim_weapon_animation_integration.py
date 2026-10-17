from types import SimpleNamespace

import pytest

from bacup_lib.models import ConversionSummary
from bacup_lib.workflows import unified


def fixture(tmp_path):
    request = SimpleNamespace(
        source_game="skyrimse", target_game="fo4",
        options=SimpleNamespace(convert_animations=True, translate_records=True,
                                convert_nifs=True, generate_anim_text_data=False),
    )
    context = SimpleNamespace(mod_path=tmp_path / "output", source_data_dir=tmp_path / "skyrim",
                              target_extracted_dir=tmp_path / "fallout", mod_prefix="B21")
    calls = []
    native = SimpleNamespace(run_phase=lambda *args, **kwargs: calls.append((args, kwargs)) or
                             {"records_added": 2, "assets_written": 12})
    return request, context, native, calls


def test_skyrim_weapon_phase_registers_profiles_and_accounts_assets(tmp_path):
    request, context, native, calls = fixture(tmp_path)
    summary = ConversionSummary()
    unified._run_skyrim_weapon_animation_phase(request, context, native, summary)
    assert calls == [(("convert_skyrim_weapon_animations",), {
        "mod_path": str(context.mod_path), "source_extracted_dir": str(context.source_data_dir),
        "target_extracted_dir": str(context.target_extracted_dir), "params": {"mod_prefix": "B21"},
    })]
    assert context._skyrim_weapon_animation_profiles
    assert summary.animations_converted == 12


@pytest.mark.parametrize("name,value", [
    ("source_game", "fnv"), ("target_game", "starfield"),
    ("convert_animations", False), ("translate_records", False), ("mvp_melee_only", True),
])
def test_skyrim_weapon_phase_respects_scope(tmp_path, name, value):
    request, context, native, calls = fixture(tmp_path)
    setattr(request if name.endswith("game") else request.options, name, value)
    unified._run_skyrim_weapon_animation_phase(request, context, native, ConversionSummary())
    assert not calls


def test_skyrim_weapon_phase_requires_assets_before_record_mutation(tmp_path):
    request, context, native, calls = fixture(tmp_path)
    request.options.convert_nifs = False
    with pytest.raises(ValueError, match="requires NIF"):
        unified._run_skyrim_weapon_animation_phase(request, context, native, ConversionSummary())
    assert not calls
    assert not getattr(context, "_skyrim_weapon_animation_profiles", False)


def test_creature_metadata_does_not_skip_registered_skyrim_weapon_graphs(tmp_path, monkeypatch):
    request, context, _, _ = fixture(tmp_path)
    context._skyrim_weapon_animation_profiles = True
    context.mvp_creature_corpus_policy = {"policy_id": "all_creatures_v1"}
    context._creature_corpus_has_jobs = True
    phases, generated = [], []
    monkeypatch.setattr(unified, "_run_post_phase", lambda name, action, *a, **k:
                        (phases.append(name), action(None)))
    monkeypatch.setattr(unified, "_run_creature_corpus_anim_text_closure", lambda *a, **k:
                        generated.append("creatures"))
    monkeypatch.setattr(unified, "_run_anim_text_data_generation", lambda *a, **k:
                        generated.append(k["force_native"]))
    runner = SimpleNamespace(emit_log=lambda *args: None)
    unified._run_required_anim_text_data(request, context, runner)
    assert phases == ["Generate Creature AnimTextData", "Generate AnimTextData"]
    assert generated == ["creatures", True]
