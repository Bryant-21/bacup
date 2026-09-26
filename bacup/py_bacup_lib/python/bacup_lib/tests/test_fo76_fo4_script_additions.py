from __future__ import annotations

import json
import tomllib
import types
from pathlib import Path

import pytest

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.workflows import unified


TRIGGER_SCRIPT = "B21:StoryEventOnTriggerEnter"
ACTIVATION_SCRIPT = "B21:StoryEventOnActivateStartScene"
REWARD_SCRIPT = "B21:QuestRewards"
MATERIALIZER_SCRIPT = "B21:LocalEncounterMaterializer"
TWO_STATE_ADAPTER_SCRIPT = "B21TwoStateActivator76"
MUSIC_INSTRUMENT_SCRIPT = "B21MusicInstrumentScript"
FURNITURE_BUFF_SCRIPT = "B21:FurnitureBuff"
HOLOTAPE_STAGE_SCRIPT = "B21:HolotapeStageOnPlay"
COLLECTOR_SCRIPT = "B21:WorkshopCollector"
MARCIA_NOTE_SCRIPT = "Fragments:Packages:PF_BS02_MQ02_Missing_Marci_0060E191_2"
MANIFEST_NAMES = tuple(
    sorted(unified._SCRIPT_ADDITION_MANIFEST[("fo76", "fo4")], key=str.lower)
)
# The subset the VMAD-reference fixtures below drive directly; additions the
# conversion injects from a consumer script instead are in MANIFEST_NAMES only.
SCRIPT_NAMES = tuple(
    sorted(
        (
            TRIGGER_SCRIPT,
            ACTIVATION_SCRIPT,
            REWARD_SCRIPT,
            "B21:CurrencyQuestRewards",
            "B21:ExpeditionMissionRewards",
            MATERIALIZER_SCRIPT,
            "B21:EnclaveEventSupport",
            HOLOTAPE_STAGE_SCRIPT,
            COLLECTOR_SCRIPT,
            TWO_STATE_ADAPTER_SCRIPT,
            MUSIC_INSTRUMENT_SCRIPT,
            FURNITURE_BUFF_SCRIPT,
            "B21_PlayerFear",
            "B21:PlanLearnOnRead",
            "B21_ActivateAliasWithRequiredItem",
            "B21_ShowMessageOnActivateAlias",
            "B21:KeypadNative",
            MARCIA_NOTE_SCRIPT,
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
        sorted(unified._script_key(script_name) for script_name in MANIFEST_NAMES)
    )
    for script_name in MANIFEST_NAMES:
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
        sorted(unified._script_key(script_name) for script_name in MANIFEST_NAMES)
    )


def test_required_script_additions_are_exact_and_pair_scoped():
    for script_name in MANIFEST_NAMES:
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


def test_every_pair_scoped_addition_source_is_registered():
    # The manifest is the only delivery path, so an unlisted .psc never reaches
    # the mod: callers compile against nothing and VMAD binds a missing class.
    pair_root = unified._SCRIPT_ADDITION_DIR / "fo76_fo4"
    on_disk = {
        unified._script_key(":".join(path.relative_to(pair_root).with_suffix("").parts))
        for path in pair_root.rglob("*.psc")
    }

    assert sorted(on_disk - set(unified._script_addition_sources("fo76", "fo4"))) == []


@pytest.mark.parametrize(
    ("source_game", "script_name"),
    (
        ("fo76", "B21:Unrelated"),
        ("fo76", "ObjectReference"),
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
        _ref("B21:CurrencyQuestRewards", 21, record_sig="QUST"),
        _ref("B21:CurrencyQuestRewards", 22, record_sig="QUST"),
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
        _ref("B21_ActivateAliasWithRequiredItem", 23, record_sig="QUST"),
        _ref("B21_ActivateAliasWithRequiredItem", 24, record_sig="QUST"),
        _ref("B21_ShowMessageOnActivateAlias", 25, record_sig="QUST"),
        _ref("B21_ShowMessageOnActivateAlias", 26, record_sig="QUST"),
        _ref("B21:KeypadNative", 27, record_sig="ACTI"),
        _ref("B21:KeypadNative", 28, record_sig="ACTI"),
        _ref("B21:EnclaveEventSupport", 29, record_sig="QUST"),
        _ref("B21:EnclaveEventSupport", 30, record_sig="QUST"),
        _ref(MARCIA_NOTE_SCRIPT, 0x60E191, record_sig="PACK"),
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


@pytest.mark.parametrize(
    ("consumer", "record_sig", "addition"),
    [
        ("Creatures:WendigoColossusRaceScript", "MGEF", "B21_PlayerFear"),
        ("ENz01_AboveScript", "QUST", "B21:EnclaveEventSupport"),
        ("W05_Wayward_SwapMarkerOnCriteria", "REFR", "B21_WaywardState"),
    ],
)
def test_consumer_script_reference_installs_its_shared_addition(
    monkeypatch, tmp_path, consumer, record_sig, addition
):
    runtime = _runtime(tmp_path, monkeypatch)
    monkeypatch.setattr(
        runtime, "_collect_script_references",
        lambda *_args: ([_ref(consumer, 1, record_sig=record_sig)], 1),
    )
    monkeypatch.setattr(
        runtime, "_reconcile_script_references",
        lambda *_args, **_kwargs: (0, 0, 0, 0, [], [], [], [], 0, 0, 0),
    )
    monkeypatch.setattr(
        "creation_lib.pex.native_runtime.compile_psc",
        lambda *_args, **_kwargs: types.SimpleNamespace(ok=True, pex_bytes=b"addition", diagnostics=[]),
    )
    ctx = _context(tmp_path)

    runtime._run_convert_scripts_phase(ctx, _runner())

    assert (Path(ctx.mod_path) / "data/Scripts" / _relative_path(addition, ".pex")).read_bytes() == b"addition"
    assert (Path(ctx.mod_path) / "Scripts/Source/User" / _relative_path(addition)).read_text() == (
        _addition_path(addition).read_text()
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


