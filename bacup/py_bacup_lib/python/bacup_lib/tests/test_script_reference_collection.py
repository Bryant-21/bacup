from __future__ import annotations

import json
import types
from pathlib import Path

import pytest

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.workflows import unified


@pytest.fixture(autouse=True)
def _isolate_script_additions(monkeypatch, tmp_path):
    monkeypatch.setattr(unified, "_SCRIPT_ADDITION_DIR", tmp_path / "script_additions")
    # The convert-scripts phase asserts that every durable patch was delivered.
    # This module tests reference *collection* against synthetic records, so
    # pointing it at the live corpus makes it fail whenever anyone adds a patch.
    monkeypatch.setattr(unified, "_SCRIPT_PATCH_DIR", tmp_path / "script_patches")


def test_target_pex_index_uses_catalog_membership_without_materializing(tmp_path):
    class Store:
        def list_assets(self, *, prefix, suffix):
            assert (prefix, suffix) == ("scripts/", ".pex")
            return ["scripts/client/workshopscript.pex"]

    index = unified._build_target_pex_index(
        target_data_dir=tmp_path / "Data",
        target_extracted_dir=None,
        target_asset_store=Store(),
    )

    assert index["workshopscript"] is None


def _runtime(source_data_dir: Path | None = None) -> unified._UnifiedRecordRuntime:
    req = PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[],
        output_root=Path("out"),
        source_data_dir=source_data_dir,
        target_extracted_dir=None,
        target_data_dir=None,
        options=PluginPortOptions(),
    )
    return unified._UnifiedRecordRuntime(req)


def _ctda(function_id: int, param1: int = 0) -> bytes:
    data = bytearray(28)
    data[8:12] = int(function_id).to_bytes(4, "little", signed=False)
    data[12:16] = int(param1).to_bytes(4, "little", signed=False)
    return bytes(data)


def test_collect_script_references_marshals_native_typed_dtos(monkeypatch):
    runtime = _runtime()
    calls: list[int] = []
    logs: list[tuple[str, str]] = []
    native = types.SimpleNamespace(
        conversion_run_inspect_script_references=lambda run_id: (
            calls.append(run_id)
            or (
                "SeventySix.esm",
                [
                    (
                        "MTNZ05QuestScript",
                        "::iRandomPart_var",
                        "3C4727:SeventySix.esm",
                        0x003C4727,
                        "INFO",
                        "",
                        "condition",
                        True,
                    ),
                    (
                        "B21TwoStateActivator76",
                        None,
                        "37879E:SeventySix.esm",
                        0x0037879E,
                        "MSTT",
                        "CoolingTowerFX",
                        "vmad",
                        False,
                    ),
                ],
                (2, 1, 1, 3, 91, 0),
                2,
                1,
                1,
                ["failed to apply FO76 script alias for 00000001"],
            )
        )
    )
    monkeypatch.setattr(unified, "load_native_module", lambda: native)
    runner = types.SimpleNamespace(
        emit_log=lambda level, message: logs.append((level, message))
    )

    refs, candidate_count = runtime._collect_script_references(7, runner)

    assert calls == [7]
    assert candidate_count == 2
    assert refs == [
        unified._ScriptReference(
            "MTNZ05QuestScript",
            "::iRandomPart_var",
            "3C4727:SeventySix.esm",
            0x003C4727,
            "INFO",
            "",
            "condition",
            True,
        ),
        unified._ScriptReference(
            "B21TwoStateActivator76",
            None,
            "37879E:SeventySix.esm",
            0x0037879E,
            "MSTT",
            "CoolingTowerFX",
            "vmad",
            False,
        ),
    ]
    assert runtime._script_reference_profile == {
        "timings": (("convert_scripts_collect_native", runtime._script_reference_profile["timings"][0][1]),),
        "counters": {
            "native_rows": 2,
            "vmad_rows": 1,
            "ctda_rows": 1,
            "subrecords": 3,
            "raw_bytes": 91,
            "authoring_bytes": 0,
        },
    }
    assert any("redirected 1 Default2StateActivator" in message for _level, message in logs)
    assert ("WARN", "[Scripts] failed to apply FO76 script alias for 00000001") in logs


def test_collect_script_references_empty_native_result(monkeypatch):
    runtime = _runtime()
    monkeypatch.setattr(
        unified,
        "load_native_module",
        lambda: types.SimpleNamespace(
            conversion_run_inspect_script_references=lambda _run_id: (
                "Output.esp",
                [],
                (0, 0, 0, 0, 0, 0),
                0,
                0,
                0,
                [],
            )
        ),
    )

    refs, candidate_count = runtime._collect_script_references(
        1, types.SimpleNamespace(emit_log=lambda *_args: None)
    )

    assert refs == []
    assert candidate_count == 0


def test_reconcile_script_references_forwards_resolution_evidence(monkeypatch):
    expected = (1, 0, 2, 1, ["vmad"], ["reward"], ["condition"], [], 2, 1, 1)
    calls = []
    monkeypatch.setattr(
        unified,
        "load_native_module",
        lambda: types.SimpleNamespace(
            conversion_run_reconcile_script_references=lambda *args: (
                calls.append(args) or expected
            )
        ),
    )

    result = _runtime()._reconcile_script_references(
        9,
        [("broken", "Broken", "compile_failed", None)],
    )

    assert result == expected
    assert calls == [(9, [("broken", "Broken", "compile_failed", None)])]


def test_convert_scripts_phase_includes_all_fo76_source_scripts(monkeypatch, tmp_path):
    source_root = tmp_path / "scripts" / "client"
    source_root.mkdir(parents=True)
    (source_root / "QuestInstance.pex").write_bytes(b"pex")
    (source_root / "UnusedUtility.pex").write_bytes(b"pex")
    runtime = _runtime(source_data_dir=tmp_path)

    monkeypatch.setattr(
        unified.native_runtime,
        "plugin_handle_get",
        lambda _handle, _name, _default=None: "SeventySix.esm",
    )
    monkeypatch.setattr(
        unified.native_runtime,
        "plugin_handle_record_form_ids_with_subrecords",
        lambda _handle, _sigs: [],
    )

    decompiled: list[str] = []
    compiled: list[str] = []

    def decompile_batch(script_names, **_kwargs):
        decompiled.extend(script_names)
        return [(name, None) for name in script_names]

    def compile_batch(script_names, **_kwargs):
        compiled.extend(script_names)
        return [
            (name, unified._ScriptResolution(name, "compiled", Path(f"{name}.pex")))
            for name in script_names
        ]

    monkeypatch.setattr(runtime, "_decompile_source_scripts_for_fo4", decompile_batch)
    monkeypatch.setattr(runtime, "_compile_decompiled_scripts_for_fo4", compile_batch)
    monkeypatch.setattr(
        runtime, "_reconcile_script_references", lambda *_args, **_kwargs: (0, 0, 0, 0, [], [], [], [], 0, 0, 0)
    )
    reports = []
    monkeypatch.setattr(
        runtime,
        "_write_script_port_report",
        lambda _ctx, **kwargs: reports.append(kwargs),
    )
    ctx = types.SimpleNamespace(
        mod_path=str(tmp_path / "mod"),
        summary=types.SimpleNamespace(scripts_flagged=0),
        timings=[],
    )
    runner = types.SimpleNamespace(emit_log=lambda *_args: None)

    runtime._run_convert_scripts_phase(ctx, runner)

    assert sorted(decompiled) == ["QuestInstance", "UnusedUtility"]
    assert sorted(compiled) == ["QuestInstance", "UnusedUtility"]
    assert len(reports[0]["resolutions"]) == 2


def test_convert_scripts_phase_runs_without_a_target_plugin_handle(monkeypatch, tmp_path):
    source_root = tmp_path / "scripts" / "client"
    source_root.mkdir(parents=True)
    (source_root / "SourceOnly.pex").write_bytes(b"pex")
    runtime = _runtime(source_data_dir=tmp_path)
    decompiled: list[str] = []
    compiled: list[str] = []

    def decompile_batch(script_names, **_kwargs):
        decompiled.extend(script_names)
        return [(name, None) for name in script_names]

    def compile_batch(script_names, **_kwargs):
        compiled.extend(script_names)
        return [
            (name, unified._ScriptResolution(name, "compiled", Path(f"{name}.pex")))
            for name in script_names
        ]

    monkeypatch.setattr(runtime, "_decompile_source_scripts_for_fo4", decompile_batch)
    monkeypatch.setattr(runtime, "_compile_decompiled_scripts_for_fo4", compile_batch)
    monkeypatch.setattr(
        runtime,
        "_reconcile_script_references",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(
            AssertionError("source-only script conversion must not modify plugin records")
        ),
    )
    monkeypatch.setattr(runtime, "_write_script_port_report", lambda *_args, **_kwargs: None)
    ctx = types.SimpleNamespace(
        mod_path=str(tmp_path / "mod"),
        summary=types.SimpleNamespace(scripts_flagged=0),
    )
    runner = types.SimpleNamespace(emit_log=lambda *_args: None)

    runtime._run_convert_scripts_phase(ctx, runner)

    assert decompiled == ["SourceOnly"]
    assert compiled == ["SourceOnly"]


def test_skyrim_script_phase_recovers_archive_script_sources(monkeypatch, tmp_path):
    from bacup_lib import skyrim_papyrus

    request = PluginPortRequest(
        source_game="skyrimse",
        target_game="fo4",
        source_plugins=[],
        output_root=tmp_path / "out",
        source_data_dir=tmp_path / "Data",
        target_extracted_dir=None,
        target_data_dir=None,
        options=PluginPortOptions(),
    )
    runtime = unified._UnifiedRecordRuntime(request)
    script_name = "ArchiveObjectScript"
    reference = unified._ScriptReference(
        script_name=script_name,
        variable_name=None,
        form_key="000800:Skyrim.esm",
        form_id=0x00000800,
        record_sig="ACTI",
        editor_id="ArchiveActivator",
        kind="vmad",
    )
    monkeypatch.setattr(
        runtime,
        "_collect_script_references",
        lambda *_args, **_kwargs: ([reference], 1),
    )
    headers_root = tmp_path / "headers"
    headers_root.mkdir()
    monkeypatch.setattr(runtime, "_papyrus_type_universe", lambda *_args: headers_root)

    def recover(intents, **kwargs):
        assert [intent.class_name for intent in intents] == [script_name]
        assert kwargs["source_data_dir"] == tmp_path / "Data"
        return skyrim_papyrus.SkyrimPapyrusProductionResult(
            supported=True,
            artifacts=(
                skyrim_papyrus.SkyrimPscSourceArtifact(
                    class_name=script_name,
                    source=f"Scriptname {script_name} Extends ObjectReference\n",
                    kind="record-vmad",
                    required=False,
                ),
            ),
            receipts=(),
            unsupported_reason=None,
        )

    monkeypatch.setattr(skyrim_papyrus, "discover_skyrim_psc_artifacts", recover)
    def decompile_batch(script_names, **_kwargs):
        assert script_names == []
        return []

    monkeypatch.setattr(runtime, "_decompile_source_scripts_for_fo4", decompile_batch)
    compiled: list[str] = []

    def compile_batch(script_names, **_kwargs):
        compiled.extend(script_names)
        return [
            (name, unified._ScriptResolution(name, "compiled", Path(f"{name}.pex")))
            for name in script_names
        ]

    monkeypatch.setattr(runtime, "_compile_decompiled_scripts_for_fo4", compile_batch)
    monkeypatch.setattr(
        runtime, "_reconcile_script_references", lambda *_args, **_kwargs: (0, 0, 0, 0, [], [], [], [], 0, 0, 0)
    )
    monkeypatch.setattr(runtime, "_write_script_port_report", lambda *_args, **_kwargs: None)
    ctx = types.SimpleNamespace(
        _rust_conversion_run=types.SimpleNamespace(id=1),
        mod_path=str(tmp_path / "mod"),
        summary=types.SimpleNamespace(scripts_flagged=0),
    )
    runner = types.SimpleNamespace(emit_log=lambda *_args: None)

    runtime._run_convert_scripts_phase(ctx, runner)

    assert compiled == [script_name]
    assert (
        tmp_path / "mod" / "Scripts" / "Source" / "User" / f"{script_name}.psc"
    ).is_file()


def test_fo76_to_fo4_base_creature_script_suppression_is_exact_and_target_aware():
    protected = [
        "Creatures:MirelurkQueenRaceScript",
        "CREATURES:RADSCORPIONRACESCRIPT",
        "creatures\\SMBehemothRaceScript",
    ]
    for script_name in protected:
        assert unified._skip_fo76_to_fo4_source_script(
            script_name,
            source_game="FO76",
            target_game="FO4",
        )

    assert not unified._skip_fo76_to_fo4_source_script(
        "Creatures:CustomRaceScript",
        source_game="fo76",
        target_game="fo4",
    )
    assert not unified._skip_fo76_to_fo4_source_script(
        "Creatures:MirelurkQueenRaceScript",
        source_game="fo4",
        target_game="fo4",
    )
    assert not unified._skip_fo76_to_fo4_source_script(
        "Creatures:MirelurkQueenRaceScript",
        source_game="fo76",
        target_game="starfield",
    )


def test_convert_scripts_suppresses_explicit_base_creature_refs_but_keeps_custom(
    monkeypatch, tmp_path
):
    source_root = tmp_path / "scripts" / "client" / "creatures"
    source_root.mkdir(parents=True)
    protected_names = [
        "MirelurkQueenRaceScript",
        "radscorpionracescript",
        "SMBehemothRaceScript",
    ]
    for script_name in [*protected_names, "CustomCreatureScript"]:
        (source_root / f"{script_name}.pex").write_bytes(b"pex")

    runtime = _runtime(source_data_dir=tmp_path)
    refs = [
        unified._ScriptReference(
            script_name=f"creatures:{script_name}",
            variable_name=None,
            form_key="000001:SeventySix.esm",
            form_id=1,
            record_sig="RACE",
            editor_id="TestRace",
            kind="vmad",
        )
        for script_name in [*protected_names, "CustomCreatureScript"]
    ]
    monkeypatch.setattr(
        runtime, "_collect_script_references", lambda *_args: (refs, len({ref.form_id for ref in refs}))
    )

    decompiled: list[str] = []
    compiled: list[str] = []

    def decompile_batch(script_names, **_kwargs):
        decompiled.extend(script_names)
        return [(name, None) for name in script_names]

    def compile_batch(script_names, **_kwargs):
        compiled.extend(script_names)
        return [
            (name, unified._ScriptResolution(name, "compiled", Path(f"{name}.pex")))
            for name in script_names
        ]

    monkeypatch.setattr(runtime, "_decompile_source_scripts_for_fo4", decompile_batch)
    monkeypatch.setattr(runtime, "_compile_decompiled_scripts_for_fo4", compile_batch)
    monkeypatch.setattr(
        runtime, "_reconcile_script_references", lambda *_args, **_kwargs: (0, 0, 0, 0, [], [], [], [], 0, 0, 0)
    )
    reports = []
    monkeypatch.setattr(
        runtime,
        "_write_script_port_report",
        lambda _ctx, **kwargs: reports.append(kwargs),
    )

    mod_path = tmp_path / "mod"
    for script_name in protected_names:
        stale = mod_path / "data" / "Scripts" / "creatures" / f"{script_name}.pex"
        stale.parent.mkdir(parents=True, exist_ok=True)
        stale.write_bytes(b"stale")
    ctx = types.SimpleNamespace(
        _rust_conversion_run=types.SimpleNamespace(id=1),
        mod_path=str(mod_path),
        summary=types.SimpleNamespace(scripts_flagged=0),
        timings=[],
    )
    runner = types.SimpleNamespace(emit_log=lambda *_args: None)

    runtime._run_convert_scripts_phase(ctx, runner)

    assert decompiled == ["creatures:CustomCreatureScript"]
    assert compiled == ["creatures:CustomCreatureScript"]
    resolutions = reports[0]["resolutions"]
    for script_name in protected_names:
        key = f"creatures:{script_name}".lower()
        assert resolutions[key].status == "unsupported"
        assert not (
            mod_path / "data" / "Scripts" / "creatures" / f"{script_name}.pex"
        ).exists()
    assert resolutions["creatures:customcreaturescript"].status == "compiled"


def test_fo76_to_fo4_script_type_aliases_region():
    assert unified._fo76_to_fo4_script_type("player") == "Actor"
    assert unified._fo76_to_fo4_script_type("region") == "Form"
    assert unified._fo76_to_fo4_script_type("Region[]") == "Form[]"
    assert unified._fo76_to_fo4_script_type("QuestInstance") == "Quest"


def test_script_port_report_names_every_vmad_change(tmp_path):
    unified._UnifiedRecordRuntime._write_script_port_report(
        types.SimpleNamespace(mod_path=tmp_path, diagnostics_root=tmp_path),
        resolutions={},
        invalid_condition_notes=[],
        stripped_conditions=0,
        stripped_vmad=1,
        changed_records=1,
        vmad_notes=["PERK 005727BB COMP_Rescue_CaptiveActivate: removed the whole VMAD"],
    )

    report = json.loads((tmp_path / "script_port_report.json").read_text())

    assert report["vmad_changes"] == [
        "PERK 005727BB COMP_Rescue_CaptiveActivate: removed the whole VMAD"
    ]
