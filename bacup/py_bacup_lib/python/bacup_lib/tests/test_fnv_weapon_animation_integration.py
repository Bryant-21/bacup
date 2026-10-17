from __future__ import annotations

from types import SimpleNamespace

import pytest

from bacup_lib import PhaseSelection, regen_pipeline
from bacup_lib.models import ConversionSummary, PluginPortOptions, PluginPortRequest
from bacup_lib.source_pairs import get_pair
from bacup_lib.workflows import unified


class Runner:
    def __init__(self):
        self.logs = []
        self.phases = []

    def emit_log(self, level, message):
        self.logs.append((level, message))

    def emit_phase_start(self, progress):
        self.phases.append(progress.phase_name)

    def emit_phase_complete(self, progress):
        pass

    def emit_item_progress(self, progress):
        pass

    def is_cancelled(self):
        return False


class NativeRun:
    def __init__(self, report):
        self.report = report
        self.calls = []

    def run_phase(self, phase, **kwargs):
        self.calls.append((phase, kwargs))
        return self.report


def request_and_context(tmp_path):
    request = PluginPortRequest(
        source_game="fnv",
        target_game="fo4",
        source_plugins=[tmp_path / "FalloutNV.esm"],
        output_root=tmp_path / "mods",
        output_mod_name="FNV_FO3",
        source_data_dir=tmp_path / "extracted_fnv",
        target_extracted_dir=tmp_path / "extracted_fo4",
        target_data_dir=tmp_path / "Fallout4" / "Data",
        options=PluginPortOptions(
            translate_records=True, convert_animations=True, convert_nifs=True
        ),
    )
    ctx = SimpleNamespace(
        source_game=request.source_game,
        target_game=request.target_game,
        source_data_dir=request.source_data_dir,
        target_extracted_dir=request.target_extracted_dir,
        target_data_dir=request.target_data_dir,
        mod_path=request.output_root / request.output_mod_name,
        output_plugin_name="FalloutNV.esm",
        mod_prefix="B21",
    )
    return request, ctx


@pytest.mark.parametrize(
    "options, expected",
    [({}, True), ({"records_only": True}, False), ({"skip_animations": True}, False)],
)
def test_fnv_regen_enables_weapon_animation_phase_by_default(options, expected):
    pair = get_pair("fnvfo3:fo4")
    values = dict(records_only=False, conversion_workers=None, records_limit=None)
    values.update(options)
    port_options = regen_pipeline._build_options(**values, pair_id=pair.pair_id)

    assert pair.source_game == "fnv"
    assert pair.target_game == "fo4"
    assert PhaseSelection().convert_animations
    assert not PluginPortOptions().convert_animations
    assert port_options.convert_animations is expected
    assert not port_options.generate_anim_text_data


@pytest.mark.parametrize("prefix", [None, "CUSTOM"])
def test_weapon_phase_dispatches_extracted_roots_and_accounts_assets(tmp_path, prefix):
    request, ctx = request_and_context(tmp_path)
    ctx.mod_prefix = prefix
    native = NativeRun({"records_added": 4, "assets_written": 9, "items_failed": 2})
    summary = ConversionSummary(animations_converted=3, animations_failed=1)

    unified._run_fnv_weapon_animation_phase(request, ctx, native, summary)

    assert native.calls == [
        (
            "convert_legacy_weapon_animations",
            {
                "mod_path": str(ctx.mod_path),
                "source_extracted_dir": str(request.source_data_dir),
                "target_extracted_dir": str(request.target_extracted_dir),
                "params": {"mod_prefix": prefix or "B21_FNV"},
            },
        )
    ]
    assert ctx._fnv_weapon_animation_profiles is True
    assert summary.animations_converted == 12
    assert summary.animations_failed == 3


@pytest.mark.parametrize(
    "field, value, profile",
    [
        ("source_game", "fo76", None),
        ("target_game", "starfield", None),
        ("convert_animations", False, None),
        ("translate_records", False, None),
        ("mvp_melee_only", True, None),
        (None, None, "fnv_dog"),
    ],
)
def test_weapon_phase_respects_pair_and_scope_gates(tmp_path, field, value, profile):
    request, ctx = request_and_context(tmp_path)
    if field:
        owner = request if field.endswith("game") else request.options
        setattr(owner, field, value)
    native = NativeRun({"records_added": 4, "assets_written": 9})
    summary = ConversionSummary()

    unified._run_fnv_weapon_animation_phase(request, ctx, native, summary, profile)

    assert native.calls == []
    assert not getattr(ctx, "_fnv_weapon_animation_profiles", False)
    assert summary.animations_converted == 0


def test_failed_weapon_phase_does_not_mark_profiles_registered(tmp_path):
    request, ctx = request_and_context(tmp_path)
    summary = ConversionSummary()

    class FailingRun:
        def run_phase(self, *_args, **_kwargs):
            raise RuntimeError("weapon graph staging failed")

    with pytest.raises(RuntimeError, match="weapon graph staging failed"):
        unified._run_fnv_weapon_animation_phase(request, ctx, FailingRun(), summary)

    assert not getattr(ctx, "_fnv_weapon_animation_profiles", False)
    assert summary.animations_converted == 0


def test_weapon_phase_rejects_missing_nif_conversion_before_native_mutation(tmp_path):
    request, ctx = request_and_context(tmp_path)
    request.options.convert_nifs = False
    native = NativeRun({"records_added": 4, "assets_written": 9})
    summary = ConversionSummary()

    with pytest.raises(ValueError, match="requires NIF conversion"):
        unified._run_fnv_weapon_animation_phase(request, ctx, native, summary)

    assert native.calls == []
    assert not getattr(ctx, "_fnv_weapon_animation_profiles", False)
    assert summary.animations_converted == 0


@pytest.mark.parametrize("records_added, records_changed", [(0, 0), (4, 0), (0, 1)])
def test_unified_generates_weapon_metadata_after_assets_and_final_plugin(
    tmp_path, monkeypatch, records_added, records_changed
):
    request, ctx = request_and_context(tmp_path)
    request.target_data_dir.mkdir(parents=True)
    (request.target_data_dir.parent / "CreationKit.exe").write_bytes(b"CK")
    native_run = NativeRun({
        "records_added": records_added,
        "records_changed": records_changed,
        "assets_written": 9,
    })
    summary = ConversionSummary()
    calls = []
    runner = Runner()

    class Native:
        def sinks_create(self, _config):
            return 17

        def sinks_abort(self, _sink_id):
            calls.append("abort")

        def sinks_drop(self, _sink_id):
            calls.append("drop")

    class Driver:
        def __init__(self, *_args, **_kwargs):
            self.ctx = ctx
            self.record_runtime = SimpleNamespace(_aggregate_summary=summary, run_result=None)
            self.assets = []
            self.terrain_texture_jobs = []

        def run_record_track(self, _runner):
            calls.append("records")
            unified._run_fnv_weapon_animation_phase(request, ctx, native_run, summary)

        def emit_complete(self, _runner):
            calls.append("complete")

    class Mirror:
        def __init__(self, *_args, **_kwargs):
            pass

        def start(self):
            pass

        def finish(self, _status):
            pass

    def asset_track(*_args, **_kwargs):
        calls.append("assets")

    original_post_phase = unified._run_post_phase

    def post_phase(label, body, post_runner, **kwargs):
        if label == "Finalize Plugin Records":
            calls.append("finalize")
            (ctx.mod_path / ctx.output_plugin_name).write_bytes(b"TES4")
        elif label == "Generate AnimTextData":
            original_post_phase(label, body, post_runner, **kwargs)

    def native_animtext(native_ctx, native_runner, plugin_path, *, progress):
        assert native_ctx is ctx
        assert native_runner is runner
        assert plugin_path == ctx.mod_path / ctx.output_plugin_name
        assert plugin_path.is_file()
        assert progress.phase_name == "Generate AnimTextData"
        assert calls[-1] == "finalize"
        calls.append("native_animtext")

    def forbidden_ck(*_args, **_kwargs):
        pytest.fail("registered FNV weapon profiles must use native AnimTextData")

    monkeypatch.setattr(unified, "load_native_module", lambda: Native())
    monkeypatch.setattr(unified, "UnifiedDriver", Driver)
    monkeypatch.setattr(unified, "RunStateMirror", Mirror)
    monkeypatch.setattr(unified, "run_asset_track", asset_track)
    monkeypatch.setattr(unified, "_run_post_phase", post_phase)
    monkeypatch.setattr(unified, "_run_anim_text_data_native", native_animtext)
    monkeypatch.setattr(unified, "_run_anim_text_data_via_ck", forbidden_ck)
    monkeypatch.setattr(unified, "_record_timing", lambda *_args, **_kwargs: None)

    result = unified.run_unified(
        request, runner, enable_ba2=False, record_preflight_complete=True
    )

    expected = ["records", "assets", "finalize"]
    profiles_registered = bool(records_added or records_changed)
    if profiles_registered:
        expected.append("native_animtext")
    assert calls == expected + ["complete", "drop"]
    assert ("Generate AnimTextData" in runner.phases) is profiles_registered
    assert ctx._fnv_weapon_animation_profiles is profiles_registered
    assert not request.options.generate_anim_text_data
    assert not request.options.anim_text_data_native
    assert result.summary.animations_converted == 9


def test_explicit_animtext_request_preserves_ck_selection_without_weapon_profiles(
    tmp_path, monkeypatch
):
    request, ctx = request_and_context(tmp_path)
    request.options.generate_anim_text_data = True
    ctx.mod_path.mkdir(parents=True)
    (ctx.mod_path / ctx.output_plugin_name).write_bytes(b"TES4")
    request.target_data_dir.mkdir(parents=True)
    (request.target_data_dir.parent / "CreationKit.exe").write_bytes(b"CK")
    calls = []
    monkeypatch.setattr(
        unified, "_run_anim_text_data_via_ck", lambda *_args, **_kwargs: calls.append("CK")
    )
    monkeypatch.setattr(
        unified,
        "_run_anim_text_data_native",
        lambda *_args, **_kwargs: pytest.fail("unexpected native dispatch"),
    )

    unified._run_required_anim_text_data(request, ctx, Runner())

    assert calls == ["CK"]


@pytest.mark.parametrize("has_jobs", [False, True])
def test_creature_corpus_keeps_its_animtext_closure(tmp_path, monkeypatch, has_jobs):
    request, ctx = request_and_context(tmp_path)
    ctx.mvp_creature_corpus_policy = {"policy_id": "all_creatures_v1"}
    ctx._creature_corpus_has_jobs = has_jobs
    ctx._fnv_weapon_animation_profiles = True
    calls = []
    monkeypatch.setattr(
        unified,
        "_run_creature_corpus_anim_text_closure",
        lambda *_args, **_kwargs: calls.append("creatures"),
    )
    monkeypatch.setattr(
        unified,
        "_run_anim_text_data_generation",
        lambda *_args, **_kwargs: pytest.fail("creature closure was bypassed"),
    )

    unified._run_required_anim_text_data(request, ctx, Runner())

    assert calls == (["creatures"] if has_jobs else [])


@pytest.mark.parametrize("havok", [False, True])
def test_registered_weapon_parts_are_staged_with_havok_on_or_off(tmp_path, havok):
    request, ctx = request_and_context(tmp_path)
    ctx._fnv_weapon_animation_profiles = True
    ctx._source_profile = SimpleNamespace(engine="gamebryo")
    ctx._target_profile = SimpleNamespace(engine="creation1")
    builder = SimpleNamespace(
        driver=SimpleNamespace(ctx=ctx),
        toggles=SimpleNamespace(havok=havok, animations=True, drivers=False),
        runs=SimpleNamespace(
            nifs=SimpleNamespace(id=11),
            havok=SimpleNamespace(id=12) if havok else None,
            textures=None,
        ),
        runner=Runner(),
        _wave_plan=unified._wave_plan_for("fnv", "fo4"),
        _shim=lambda: ctx,
    )
    stages = unified.AssetWaveBuilder.build_wave_a4(builder)
    assert [stage.phase for stage in stages] == ["convert_legacy_weapon_parts"]
    parts = stages[0]

    assert parts.run_id == 11
    assert parts.source_extracted_dir == str(request.source_data_dir)
    assert parts.target_extracted_dir == str(request.target_extracted_dir)
    assert parts.after == ()
    plan = unified._sequential_wave_plan(stages)
    assert plan[0]["after"] == []
    ctx._fnv_weapon_animation_profiles = False
    assert unified.AssetWaveBuilder.build_wave_a4(builder) == []


def test_weapon_parts_report_counts_as_converted_animation_assets():
    summary = ConversionSummary()

    unified._merge_wave_report_into_summary(
        summary, "convert_legacy_weapon_parts", {"assets_written": 6, "items_failed": 1}
    )

    assert summary.animations_converted == 6
    assert summary.animations_failed == 1
