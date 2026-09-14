from __future__ import annotations

import json
import tomllib
import types
from pathlib import Path

import pytest

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows import unified
from creation_lib.pex.native_runtime import compile_psc


TRIGGER_SCRIPT = "B21:StoryEventOnTriggerEnter"
ACTIVATION_SCRIPT = "B21:StoryEventOnActivateStartScene"
REWARD_SCRIPT = "B21:QuestRewards"
MATERIALIZER_SCRIPT = "B21:LocalEncounterMaterializer"
TWO_STATE_ADAPTER_SCRIPT = "B21TwoStateActivator76"
MUSIC_INSTRUMENT_SCRIPT = "B21MusicInstrumentScript"
FURNITURE_BUFF_SCRIPT = "B21:FurnitureBuff"
HOLOTAPE_STAGE_SCRIPT = "B21:HolotapeStageOnPlay"
COLLECTOR_SCRIPT = "B21:WorkshopCollector"
SCRIPT_NAMES = tuple(
    sorted(
        (
            TRIGGER_SCRIPT,
            ACTIVATION_SCRIPT,
            REWARD_SCRIPT,
            "B21:ExpeditionMissionRewards",
            MATERIALIZER_SCRIPT,
            HOLOTAPE_STAGE_SCRIPT,
            COLLECTOR_SCRIPT,
            TWO_STATE_ADAPTER_SCRIPT,
            MUSIC_INSTRUMENT_SCRIPT,
            FURNITURE_BUFF_SCRIPT,
            "B21_PlayerFear",
            "B21:PlanLearnOnRead",
        ),
        key=str.lower,
    )
)


def _relative_path(script_name: str, suffix: str = ".psc") -> Path:
    return Path(*script_name.split(":")).with_suffix(suffix)


def _addition_path(script_name: str) -> Path:
    return unified._SCRIPT_ADDITION_DIR / "fo76_fo4" / _relative_path(script_name)


def _runtime(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    *,
    source_game: str = "fo76",
) -> unified._UnifiedRecordRuntime:
    # Addition-only fixtures intentionally select no durable source-script patches.
    monkeypatch.setattr(unified, "_script_patch_inventory", lambda: {})
    target_data_dir = tmp_path / "Fallout4" / "Data"
    # Anchor-named .pex so the Papyrus type universe resolves. FO4 ships no
    # .psc, so the compile path builds its types from the game's own .pex.
    scripts_dir = target_data_dir / "scripts"
    scripts_dir.mkdir(parents=True)
    for anchor in ("ScriptObject", "Form", "ObjectReference"):
        (scripts_dir / f"{anchor}.pex").write_bytes(b"not a real pex")
    request = PluginPortRequest(
        source_game=source_game,
        target_game="fo4",
        source_plugins=[],
        output_root=tmp_path / "out",
        source_data_dir=tmp_path / "source",
        target_extracted_dir=None,
        target_data_dir=target_data_dir,
        options=PluginPortOptions(papyrus_compiler="native"),
    )
    return unified._UnifiedRecordRuntime(request)


def _ref(
    script_name: str,
    form_id: int,
    *,
    record_sig: str = "REFR",
) -> unified._ScriptReference:
    return unified._ScriptReference(
        script_name=script_name,
        variable_name=None,
        form_key=f"{form_id:06X}:SeventySix.esm",
        form_id=form_id,
        record_sig=record_sig,
        editor_id=f"Test{record_sig}{form_id:06X}",
        kind="vmad",
    )


def _context(tmp_path: Path):
    return types.SimpleNamespace(
        _rust_conversion_run=types.SimpleNamespace(id=1),
        mod_path=tmp_path / "mod",
        diagnostics_root=tmp_path / "diagnostics",
        summary=types.SimpleNamespace(scripts_flagged=0),
    )


def _runner():
    logs: list[tuple[str, str]] = []
    return types.SimpleNamespace(
        logs=logs,
        emit_log=lambda level, message: logs.append((level, message)),
    )


def _record_with_vmad(data: bytes):
    return types.SimpleNamespace(
        subrecords=[types.SimpleNamespace(signature="VMAD", data=data)]
    )


def test_script_addition_discovery_is_pair_scoped_and_namespace_correct():
    additions = unified._script_addition_sources("FO76", "FO4")

    assert tuple(sorted(additions)) == tuple(
        sorted(unified._script_key(script_name) for script_name in SCRIPT_NAMES)
    )
    for script_name in SCRIPT_NAMES:
        key = unified._script_key(script_name)
        assert additions[key] == (script_name, _addition_path(script_name))
        assert additions[key][1].relative_to(
            unified._SCRIPT_ADDITION_DIR / "fo76_fo4"
        ) == _relative_path(script_name)
    assert unified._script_addition_sources("skyrimse", "fo4") == {}
    assert unified._script_addition_sources("fnvfo3", "fo4") == {}


def test_script_additions_are_included_in_wheel_package_data():
    pyproject_path = Path(__file__).resolve().parents[3] / "pyproject.toml"
    pyproject = tomllib.loads(pyproject_path.read_text(encoding="utf-8"))
    wheel_includes = {
        item["path"]
        for item in pyproject["tool"]["maturin"]["include"]
        if item.get("format") == "wheel"
    }

    assert "python/bacup_lib/script_additions/**/*.psc" in wheel_includes
    assert tuple(sorted(unified._script_addition_sources("fo76", "fo4"))) == tuple(
        sorted(unified._script_key(script_name) for script_name in SCRIPT_NAMES)
    )


def test_required_script_additions_are_exact_and_pair_scoped():
    for script_name in SCRIPT_NAMES:
        assert unified._requires_script_addition(
            script_name,
            source_game="FO76",
            target_game="FO4",
        )

    assert not unified._requires_script_addition(
        "B21:Unrelated",
        source_game="fo76",
        target_game="fo4",
    )
    assert not unified._requires_script_addition(
        TRIGGER_SCRIPT,
        source_game="skyrimse",
        target_game="fo4",
    )


def test_two_state_adapter_uses_fo76_jump_events_for_load_reconciliation():
    source = _addition_path(TWO_STATE_ADAPTER_SCRIPT).read_text(encoding="utf-8")

    assert source.startswith(
        "Scriptname B21TwoStateActivator76 extends Default2StateActivator"
    )
    assert 'String Property SetClosedAnim = "JumpState01" Auto' in source
    assert 'String Property SetOpenAnim = "JumpState02" Auto' in source
    assert "PlayAnimation(SetOpenAnim)" in source
    assert "PlayAnimation(SetClosedAnim)" in source
    assert "PlayAnimationAndWait(SetOpenAnim" not in source
    assert "PlayAnimationAndWait(SetClosedAnim" not in source


def test_music_instrument_adapter_tracks_furniture_use_and_sound_roles():
    source, lines = _stripped_lines(MUSIC_INSTRUMENT_SCRIPT)

    assert lines[:5] == [
        "Scriptname B21MusicInstrumentScript extends ObjectReference",
        "Sound Property Intro Auto Const",
        "Sound Property Rhythm Auto Const",
        "Sound Property Lead Auto Const",
        "Sound Property Outro Auto Const",
    ]
    assert "Event OnActivate(ObjectReference akActionRef)" in lines
    assert "Event Actor.OnSit(Actor akSender, ObjectReference akFurniture)" in lines
    assert "Event Actor.OnGetUp(Actor akSender, ObjectReference akFurniture)" in lines
    assert 'RegisterForRemoteEvent(actionActor, "OnSit")' in lines
    assert 'RegisterForRemoteEvent(actionActor, "OnGetUp")' in lines
    assert "Intro.PlayAndWait(Self)" in lines
    assert "Sound loopSound = Rhythm" in lines
    assert "loopSound = Lead" in lines
    assert "nextInstance = loopSound.Play(Self)" in lines
    assert "LoopSoundInstance = nextInstance" in lines
    assert "Sound.StopInstance(currentLoopSoundInstance)" in lines
    assert "Outro.Play(Self)" in lines
    assert "PlaybackGeneration != thisPlayback" in source


def test_music_instrument_repeats_phrases_and_rejects_previous_use_timers():
    source, lines = _stripped_lines(MUSIC_INSTRUMENT_SCRIPT)

    timer = source.split("Event OnTimer(Int aiTimerID)", 1)[1].split("EndEvent", 1)[0]
    assert timer.index("aiTimerID != PlaybackGeneration") < timer.index("PlayRhythm(aiTimerID)")
    assert "StartTimer(2.0, aiGeneration)" in lines
    assert "CancelTimer(PlaybackGeneration)" in lines
    assert "PlaybackGeneration != aiGeneration" in source
    assert "Sound.StopInstance(nextInstance)" in lines
    assert "Sound.StopInstance(previousInstance)" in lines
    assert "Event OnUnload()\n    FinishPlayback(False)" in source


def test_holotape_stage_adapter_is_play_only_and_stage_guarded():
    source, lines = _stripped_lines(HOLOTAPE_STAGE_SCRIPT)

    assert lines[:4] == [
        f"Scriptname {HOLOTAPE_STAGE_SCRIPT} Extends ObjectReference",
        "Quest Property TargetQuest Auto Const",
        "Int Property PrereqStage Auto Const",
        "Int Property StageToSet Auto Const",
    ]
    assert "Event OnHolotapePlay(ObjectReference akTerminalRef)" in lines
    assert "If TargetQuest == None || !TargetQuest.IsRunning()" in lines
    assert (
        "If !TargetQuest.IsStageDone(PrereqStage) || TargetQuest.IsStageDone(StageToSet)"
        in lines
    )
    assert lines.count("TargetQuest.SetStage(StageToSet)") == 1
    assert "OnContainerChanged" not in source
    assert "OnItemAdded" not in source


@pytest.mark.parametrize(
    ("source_game", "script_name"),
    (
        ("fo76", "B21:Unrelated"),
        ("fo76", "ObjectReference"),
        ("fo76", "Actor"),
        ("fo76", "Quest"),
        ("fo76", "Terminal"),
        ("skyrimse", TRIGGER_SCRIPT),
    ),
)
def test_nonmanifest_script_additions_use_normal_target_resolution(
    source_game: str,
    script_name: str,
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
):
    additions_root = tmp_path / "script_additions"
    artifact_path = additions_root / f"{source_game}_fo4" / _relative_path(script_name)
    artifact_path.parent.mkdir(parents=True)
    artifact_path.write_text(
        f"Scriptname {script_name} extends ObjectReference\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(unified, "_SCRIPT_ADDITION_DIR", additions_root)

    runtime = _runtime(tmp_path, monkeypatch, source_game=source_game)
    target_pex = (
        runtime._req.target_data_dir / "Scripts" / _relative_path(script_name, ".pex")
    )
    target_pex.parent.mkdir(parents=True, exist_ok=True)
    target_pex.write_bytes(b"target")
    monkeypatch.setattr(
        runtime,
        "_collect_script_references",
        lambda *_args: ([_ref(script_name, 1)], 1),
    )
    monkeypatch.setattr(
        runtime,
        "_reconcile_script_references",
        lambda *_args, **_kwargs: (0, 0, 0, 0, [], [], [], [], 0, 0, 0),
    )
    ctx = _context(tmp_path)

    runtime._run_convert_scripts_phase(ctx, _runner())

    assert unified._script_addition_sources(source_game, "fo4") == {}
    report = json.loads(
        (Path(ctx.diagnostics_root) / "script_port_report.json").read_text(
            encoding="utf-8"
        )
    )
    assert report["scripts"][0]["status"] == "target"
    assert Path(report["scripts"][0]["pex_path"]) == target_pex
    assert not (
        Path(ctx.mod_path) / "Scripts" / "Source" / "User" / _relative_path(script_name)
    ).exists()


def test_script_addition_install_uses_namespace_path_and_validates_declaration(
    tmp_path: Path,
):
    source_path = tmp_path / "source" / "B21" / "TargetOnly.psc"
    source_path.parent.mkdir(parents=True)
    source_path.write_text(
        "Scriptname B21:TargetOnly extends ObjectReference\n",
        encoding="utf-8",
    )

    installed = unified._install_script_addition(
        tmp_path / "mod",
        "B21:TargetOnly",
        source_path,
    )

    assert installed == (
        tmp_path / "mod" / "Scripts" / "Source" / "User" / "B21" / "TargetOnly.psc"
    )
    assert installed.read_text(encoding="utf-8") == source_path.read_text(
        encoding="utf-8"
    )

    source_path.write_text("Scriptname B21:Wrong\n", encoding="utf-8")
    with pytest.raises(ValueError, match="declares B21:Wrong, expected B21:TargetOnly"):
        unified._install_script_addition(
            tmp_path / "mod",
            "B21:TargetOnly",
            source_path,
        )


def test_duplicate_references_install_compile_and_report_each_addition_once(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
):
    runtime = _runtime(tmp_path, monkeypatch)
    refs = [
        _ref(TRIGGER_SCRIPT, 1),
        _ref(TRIGGER_SCRIPT, 2),
        _ref(ACTIVATION_SCRIPT, 3),
        _ref(ACTIVATION_SCRIPT, 4),
        _ref(REWARD_SCRIPT, 5, record_sig="QUST"),
        _ref(REWARD_SCRIPT, 6, record_sig="QUST"),
        _ref("B21:ExpeditionMissionRewards", 9, record_sig="QUST"),
        _ref("B21:ExpeditionMissionRewards", 10, record_sig="QUST"),
        _ref(MATERIALIZER_SCRIPT, 7, record_sig="QUST"),
        _ref(MATERIALIZER_SCRIPT, 8, record_sig="QUST"),
        _ref(HOLOTAPE_STAGE_SCRIPT, 11, record_sig="NOTE"),
        _ref(TWO_STATE_ADAPTER_SCRIPT, 11, record_sig="MSTT"),
        _ref(TWO_STATE_ADAPTER_SCRIPT, 12, record_sig="ACTI"),
        _ref(MUSIC_INSTRUMENT_SCRIPT, 13, record_sig="FURN"),
        _ref(MUSIC_INSTRUMENT_SCRIPT, 14, record_sig="FURN"),
        _ref(FURNITURE_BUFF_SCRIPT, 17, record_sig="FURN"),
        _ref(FURNITURE_BUFF_SCRIPT, 18, record_sig="FURN"),
        _ref(COLLECTOR_SCRIPT, 15, record_sig="CONT"),
        _ref(COLLECTOR_SCRIPT, 16, record_sig="CONT"),
        _ref("B21_PlayerFear", 19, record_sig="MGEF"),
        _ref("B21:PlanLearnOnRead", 20, record_sig="BOOK"),
    ]
    monkeypatch.setattr(
        runtime,
        "_collect_script_references",
        lambda *_args: (refs, len({ref.form_id for ref in refs})),
    )
    monkeypatch.setattr(
        runtime,
        "_reconcile_script_references",
        lambda *_args, **_kwargs: (0, 0, 0, 0, [], [], [], [], 0, 0, 0),
    )

    compile_calls: list[dict[str, object]] = []

    def fake_compile_psc(source, *, imports, game, flags, source_path=None):
        compile_calls.append(
            {
                "source": source,
                "imports": imports,
                "game": game,
                "source_path": source_path,
            }
        )
        return types.SimpleNamespace(ok=True, pex_bytes=b"fresh pex", diagnostics=[])

    monkeypatch.setattr(
        "creation_lib.pex.native_runtime.compile_psc",
        fake_compile_psc,
    )
    ctx = _context(tmp_path)
    runner = _runner()

    runtime._run_convert_scripts_phase(ctx, runner)

    assert len(compile_calls) == len(SCRIPT_NAMES)
    assert {
        Path(call["source_path"]).relative_to(
            Path(ctx.mod_path) / "Scripts" / "Source" / "User"
        )
        for call in compile_calls
    } == {_relative_path(script_name) for script_name in SCRIPT_NAMES}
    for script_name in SCRIPT_NAMES:
        generated_psc = (
            Path(ctx.mod_path)
            / "Scripts"
            / "Source"
            / "User"
            / _relative_path(script_name)
        )
        generated_pex = (
            Path(ctx.mod_path)
            / "data"
            / "Scripts"
            / _relative_path(script_name, ".pex")
        )
        assert generated_psc.read_text(encoding="utf-8") == _addition_path(
            script_name
        ).read_text(encoding="utf-8")
        assert generated_pex.read_bytes() == b"fresh pex"
        assert (
            sum(
                f"installed target-only full script {script_name}" in message
                for _level, message in runner.logs
            )
            == 1
        )

    report = json.loads(
        (Path(ctx.diagnostics_root) / "script_port_report.json").read_text(
            encoding="utf-8"
        )
    )
    assert [item["script_name"] for item in report["scripts"]] == list(SCRIPT_NAMES)
    assert all(item["status"] == "compiled" for item in report["scripts"])


def test_colossus_script_reference_includes_fear_native_interface(monkeypatch, tmp_path):
    runtime = _runtime(tmp_path, monkeypatch)
    monkeypatch.setattr(
        runtime, "_collect_script_references",
        lambda *_args: ([_ref("Creatures:WendigoColossusRaceScript", 1, record_sig="MGEF")], 1),
    )
    monkeypatch.setattr(
        runtime, "_reconcile_script_references",
        lambda *_args, **_kwargs: (0, 0, 0, 0, [], [], [], [], 0, 0, 0),
    )
    monkeypatch.setattr(
        "creation_lib.pex.native_runtime.compile_psc",
        lambda *_args, **_kwargs: types.SimpleNamespace(ok=True, pex_bytes=b"fear interface", diagnostics=[]),
    )
    ctx = _context(tmp_path)

    runtime._run_convert_scripts_phase(ctx, _runner())

    assert (Path(ctx.mod_path) / "data/Scripts/B21_PlayerFear.pex").read_bytes() == b"fear interface"
    assert (Path(ctx.mod_path) / "Scripts/Source/User/B21_PlayerFear.psc").read_text() == (
        _addition_path("B21_PlayerFear").read_text()
    )


def test_unreferenced_addition_is_not_installed(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
):
    runtime = _runtime(tmp_path, monkeypatch)
    monkeypatch.setattr(
        runtime,
        "_collect_script_references",
        lambda *_args: ([_ref(TRIGGER_SCRIPT, 1)], 1),
    )
    monkeypatch.setattr(
        runtime,
        "_reconcile_script_references",
        lambda *_args, **_kwargs: (0, 0, 0, 0, [], [], [], [], 0, 0, 0),
    )
    monkeypatch.setattr(
        "creation_lib.pex.native_runtime.compile_psc",
        lambda *_args, **_kwargs: types.SimpleNamespace(
            ok=True,
            pex_bytes=b"pex",
            diagnostics=[],
        ),
    )
    ctx = _context(tmp_path)

    runtime._run_convert_scripts_phase(ctx, _runner())

    assert (
        Path(ctx.mod_path)
        / "Scripts"
        / "Source"
        / "User"
        / _relative_path(TRIGGER_SCRIPT)
    ).is_file()
    assert not (
        Path(ctx.mod_path)
        / "Scripts"
        / "Source"
        / "User"
        / _relative_path(ACTIVATION_SCRIPT)
    ).exists()
    assert not (
        Path(ctx.mod_path)
        / "Scripts"
        / "Source"
        / "User"
        / _relative_path(REWARD_SCRIPT)
    ).exists()


def test_missing_required_addition_is_reported_to_native_reconciliation(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
):
    missing_script = TRIGGER_SCRIPT
    missing_form_id = 0x123
    runtime = _runtime(tmp_path, monkeypatch)
    source_pex = (
        runtime._req.source_data_dir
        / "scripts"
        / "client"
        / _relative_path(missing_script, ".pex")
    )
    target_pex = (
        runtime._req.target_data_dir
        / "Scripts"
        / _relative_path(missing_script, ".pex")
    )
    source_pex.parent.mkdir(parents=True)
    target_pex.parent.mkdir(parents=True)
    source_pex.write_bytes(b"source fallback must not be used")
    target_pex.write_bytes(b"target fallback must not be used")
    monkeypatch.setattr(
        unified,
        "_SCRIPT_ADDITION_DIR",
        tmp_path / "empty_additions",
    )
    monkeypatch.setattr(
        runtime,
        "_collect_script_references",
        lambda *_args: (
            [_ref(missing_script, missing_form_id)],
            1,
        ),
    )
    reconcile_calls = []

    def capture_reconcile(run_id, evidence):
        reconcile_calls.append((run_id, evidence))
        return 0, 1, 1, 0, [], [], [], [], 0, 0, 0

    monkeypatch.setattr(runtime, "_reconcile_script_references", capture_reconcile)
    ctx = _context(tmp_path)
    stale_psc = (
        Path(ctx.mod_path)
        / "Scripts"
        / "Source"
        / "User"
        / _relative_path(missing_script)
    )
    stale_pex = (
        Path(ctx.mod_path) / "data" / "Scripts" / _relative_path(missing_script, ".pex")
    )
    stale_psc.parent.mkdir(parents=True)
    stale_pex.parent.mkdir(parents=True)
    stale_psc.write_text("stale", encoding="utf-8")
    stale_pex.write_bytes(b"stale")
    runner = _runner()

    runtime._run_convert_scripts_phase(ctx, runner)

    assert not stale_psc.exists()
    assert not stale_pex.exists()
    assert reconcile_calls[0][0] == 1
    assert reconcile_calls[0][1] == [
        (unified._script_key(missing_script), missing_script, "addition_missing", None)
    ]
    report = json.loads(
        (Path(ctx.diagnostics_root) / "script_port_report.json").read_text(
            encoding="utf-8"
        )
    )
    assert report["scripts"][0]["status"] == "addition_missing"
    assert (
        "required pair-scoped source artifact not found"
        in report["scripts"][0]["message"]
    )
    assert any("addition_missing" in message for _level, message in runner.logs)


def test_addition_compile_failure_is_reported_to_native_reconciliation(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
):
    form_id = 0x456
    runtime = _runtime(tmp_path, monkeypatch)
    monkeypatch.setattr(
        runtime,
        "_collect_script_references",
        lambda *_args: (
            [_ref(TRIGGER_SCRIPT, form_id)],
            1,
        ),
    )
    monkeypatch.setattr(
        "creation_lib.pex.native_runtime.compile_psc",
        lambda *_args, **_kwargs: types.SimpleNamespace(
            ok=False,
            pex_bytes=None,
            diagnostics=[{"line": 1, "col": 1, "message": "compile failed"}],
        ),
    )
    reconcile_calls = []

    def capture_reconcile(run_id, evidence):
        reconcile_calls.append((run_id, evidence))
        return 0, 1, 1, 0, [], [], [], [], 0, 0, 0

    monkeypatch.setattr(runtime, "_reconcile_script_references", capture_reconcile)
    ctx = _context(tmp_path)
    runner = _runner()

    runtime._run_convert_scripts_phase(ctx, runner)

    generated_psc = (
        Path(ctx.mod_path)
        / "Scripts"
        / "Source"
        / "User"
        / _relative_path(TRIGGER_SCRIPT)
    )
    assert not generated_psc.exists()
    assert reconcile_calls[0][0] == 1
    assert reconcile_calls[0][1][0][:3] == (
        unified._script_key(TRIGGER_SCRIPT),
        TRIGGER_SCRIPT,
        "compile_failed",
    )
    report = json.loads(
        (Path(ctx.diagnostics_root) / "script_port_report.json").read_text(
            encoding="utf-8"
        )
    )
    assert report["scripts"][0]["status"] == "compile_failed"
    assert "compile failed" in report["scripts"][0]["message"]
    assert any("compile_failed" in message for _level, message in runner.logs)


def test_invalid_addition_artifact_is_reported_to_native_reconciliation(
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
):
    script_name = TRIGGER_SCRIPT
    form_id = 0x789
    additions_root = tmp_path / "script_additions"
    source_path = additions_root / "fo76_fo4" / _relative_path(script_name)
    source_path.parent.mkdir(parents=True)
    source_path.write_text("Scriptname B21:Wrong\n", encoding="utf-8")
    monkeypatch.setattr(unified, "_SCRIPT_ADDITION_DIR", additions_root)
    runtime = _runtime(tmp_path, monkeypatch)
    monkeypatch.setattr(
        runtime,
        "_collect_script_references",
        lambda *_args: (
            [_ref(script_name, form_id)],
            1,
        ),
    )
    reconcile_calls = []

    def capture_reconcile(run_id, evidence):
        reconcile_calls.append((run_id, evidence))
        return 0, 1, 1, 0, [], [], [], [], 0, 0, 0

    monkeypatch.setattr(runtime, "_reconcile_script_references", capture_reconcile)
    ctx = _context(tmp_path)

    runtime._run_convert_scripts_phase(ctx, _runner())

    assert reconcile_calls[0][0] == 1
    assert reconcile_calls[0][1] == [
        (
            unified._script_key(script_name),
            script_name,
            "addition_install_failed",
            None,
        )
    ]
    report = json.loads(
        (Path(ctx.diagnostics_root) / "script_port_report.json").read_text(
            encoding="utf-8"
        )
    )
    assert report["scripts"][0]["status"] == "addition_install_failed"
    assert (
        f"declares B21:Wrong, expected {script_name}" in report["scripts"][0]["message"]
    )


def _stripped_lines(script_name: str) -> tuple[str, list[str]]:
    source = _addition_path(script_name).read_text(encoding="utf-8")
    return source, [line.strip() for line in source.splitlines() if line.strip()]


def _assert_suppressing_states(lines: list[str], event: str) -> None:
    for state_name in ("Busy", "Done"):
        state_index = lines.index(f"State {state_name}")
        assert lines[state_index : state_index + 4] == [
            f"State {state_name}",
            event,
            "EndEvent",
            "EndState",
        ]


def test_trigger_adapter_source_contract():
    source, lines = _stripped_lines(TRIGGER_SCRIPT)
    event = "Event OnTriggerEnter(ObjectReference akActionRef)"

    assert lines[:4] == [
        f"Scriptname {TRIGGER_SCRIPT} extends ObjectReference",
        "Quest Property TargetQuest Auto Const",
        "Keyword Property StoryEventKeyword Auto Const",
        "Int Property StageToSet Auto Const",
    ]
    assert lines.count(event) == 3
    _assert_suppressing_states(lines, event)
    assert "If akActionRef != playerRef" in lines
    assert "If TargetQuest.IsCompleted()" in lines
    busy_index = lines.index('GoToState("Busy")')
    event_index = lines.index(
        "StoryEventKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    running_index = lines.index("If TargetQuest.IsRunning()", event_index)
    stage_index = lines.index("TargetQuest.SetStage(StageToSet)")
    done_index = lines.index('GoToState("Done")', stage_index)
    rearm_index = lines.index('GoToState("Armed")', done_index)

    assert busy_index < event_index < running_index < stage_index
    assert lines[event_index - 1] == "If !TargetQuest.IsRunning()"
    assert lines[stage_index - 1] == "If TargetQuest.IsRunning()"
    assert stage_index < done_index < rearm_index
    assert lines.count("TargetQuest.SetStage(StageToSet)") == 1
    assert "TargetQuest.Start()" not in source
    assert "StartGameEnabled" not in source
    assert "W05_" not in source
    assert "405E" not in source


def test_activation_scene_adapter_source_contract():
    source, lines = _stripped_lines(ACTIVATION_SCRIPT)
    event = "Event OnActivate(ObjectReference akActionRef)"

    assert lines[:4] == [
        f"Scriptname {ACTIVATION_SCRIPT} extends ObjectReference",
        "Quest Property TargetQuest Auto Const",
        "Keyword Property StoryEventKeyword Auto Const",
        "Scene Property SceneToStart Auto Const",
    ]
    assert lines.count(event) == 3
    _assert_suppressing_states(lines, event)
    assert "If akActionRef != playerRef" in lines
    assert "If TargetQuest.IsCompleted() || SceneToStart.IsPlaying()" in lines
    busy_index = lines.index('GoToState("Busy")')
    event_index = lines.index(
        "StoryEventKeyword.SendStoryEventAndWait(None, playerRef, playerRef)"
    )
    running_index = lines.index("If TargetQuest.IsRunning()", event_index)
    not_playing_index = lines.index("If !SceneToStart.IsPlaying()", running_index)
    scene_start_index = lines.index("SceneToStart.Start()")
    playing_index = lines.index("If SceneToStart.IsPlaying()", scene_start_index)
    done_index = lines.index('GoToState("Done")', playing_index)
    rearm_index = lines.index('GoToState("Armed")', done_index)

    assert busy_index < event_index < running_index < not_playing_index
    assert lines[event_index - 1] == "If !TargetQuest.IsRunning()"
    assert not_playing_index < scene_start_index < playing_index
    assert lines[scene_start_index - 1] == "If !SceneToStart.IsPlaying()"
    assert lines[done_index - 1] == "If SceneToStart.IsPlaying()"
    assert playing_index < done_index < rearm_index
    assert lines.count("SceneToStart.Start()") == 1
    assert "TargetQuest.Start()" not in source
    assert "SetStage" not in source
    assert "W05_" not in source
    assert "405E" not in source


def test_quest_reward_source_contract():
    source, lines = _stripped_lines(REWARD_SCRIPT)

    assert lines[0] == f"Scriptname {REWARD_SCRIPT} Extends Quest"
    assert [line for line in lines if " Property " in line] == [
        "Int[] Property XPStages Auto Const",
        "GlobalVariable[] Property RewardXP Auto",
        "Int[] Property CapsStages Auto Const",
        "GlobalVariable[] Property RewardCaps Auto",
        "MiscObject Property CapsItem Auto",
        "Int[] Property ItemStages Auto Const",
        "Form[] Property RewardItems Auto",
        "Int[] Property RewardCounts Auto",
    ]
    assert "Event OnStageSet(Int auiStageID, Int auiItemID)" in source
    assert "Event OnQuestInit()" in source
    assert source.count("GrantedStages = None") == 1
    assert "GrantRewardsForStage(auiStageID)" in source
    assert "Int[] GrantedStages" in source
    assert "HasGrantedStage(auiStageID)" in source
    assert "RecordGrantedStage(auiStageID)" in source
    assert "XPStages[index] == auiStageID" in source
    assert "CapsStages[index] == auiStageID" in source
    assert "ItemStages[index] == auiStageID" in source
    grant_record_index = lines.index("RecordGrantedStage(auiStageID)")
    xp_reward_index = lines.index("Game.RewardPlayerXP(xpAmount.GetValueInt())")
    caps_add_index = lines.index(
        "playerRef.AddItem(CapsItem, capsAmount.GetValueInt(), True)"
    )
    item_add_index = lines.index("playerRef.AddItem(rewardItem, rewardCount, True)")
    assert grant_record_index < xp_reward_index < caps_add_index < item_add_index
    assert "GrantedStages.Add(auiStageID)" in source
    assert "Event OnQuestShutdown()" not in source
    assert "OnPlayerLoadGame" not in source
    assert source.count("Game.RewardPlayerXP(xpAmount.GetValueInt())") == 1
    assert ".Complete(" not in source
    assert ".Stop(" not in source
    assert "Debug.Notification" not in source
    source_casefold = source.casefold()
    for forbidden in ("learnrecipe", "teachrecipe", "constructibleobject"):
        assert forbidden not in source_casefold


def test_quest_reward_run_guard_resets_only_when_a_new_run_initializes():
    source, lines = _stripped_lines(REWARD_SCRIPT)
    init_index = lines.index("Event OnQuestInit()")
    reset_index = lines.index("GrantedStages = None")
    init_end_index = lines.index("EndEvent", init_index)
    grant_index = lines.index("Function GrantRewardsForStage(Int auiStageID)")
    guard_index = lines.index(
        "If !HasRewardForStage(auiStageID) || HasGrantedStage(auiStageID)",
        grant_index,
    )
    record_index = lines.index("RecordGrantedStage(auiStageID)", guard_index)

    assert init_index < reset_index < init_end_index < grant_index
    assert guard_index < record_index
    assert source.count("GrantedStages = None") == 1
    assert "OnPlayerLoadGame" not in source

    granted_stages: set[int] = set()

    def grant_for_stage(stage: int) -> bool:
        if stage in granted_stages:
            return False
        granted_stages.add(stage)
        return True

    def initialize_new_run() -> None:
        granted_stages.clear()

    assert grant_for_stage(9000)
    assert not grant_for_stage(9000)
    initialize_new_run()
    assert grant_for_stage(9000)


@pytest.mark.parametrize("script_name", SCRIPT_NAMES)
def test_full_script_addition_compiles_with_fo4_imports(script_name: str):
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")

    result = compile_psc(
        _addition_path(script_name).read_text(encoding="utf-8"),
        imports=[str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=str(_relative_path(script_name)),
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
