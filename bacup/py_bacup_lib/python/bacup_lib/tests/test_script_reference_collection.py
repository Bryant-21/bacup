from __future__ import annotations

import types
from pathlib import Path

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.workflows import unified
from creation_lib.esp.model import Record, Subrecord


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


def test_condition_script_refs_infer_script_from_get_vm_quest_variable_form():
    record = Record(
        signature="INFO",
        form_id=0x003C4727,
        subrecords=[
            Subrecord("CTDA", _ctda(629, 0x0701A634)),
            Subrecord("CIS2", b"::iRandomPart_var\x00"),
        ],
    )
    setattr(record, "editor_id", "")

    refs = unified._iter_condition_script_refs(
        record,
        form_key="3C4727:SeventySix.esm",
        scripts_by_form_id={0x0001A634: ["MTNZ05QuestScript"]},
    )

    assert len(refs) == 1
    assert refs[0].script_name == "MTNZ05QuestScript"
    assert refs[0].variable_name == "::iRandomPart_var"
    assert refs[0].kind == "condition"


def test_collect_script_references_finds_conditions_and_vmad(monkeypatch):
    runtime = _runtime()
    info_form_id = 0x003C4727
    quest_form_id = 0x0001A634
    summaries = {
        info_form_id: types.SimpleNamespace(
            form_id=info_form_id,
            signature="INFO",
            editor_id="",
        ),
        quest_form_id: types.SimpleNamespace(
            form_id=quest_form_id,
            signature="QUST",
            editor_id="RE_Scene_MTNZ05_Messenger",
        ),
    }
    subrecords = {
        info_form_id: [
            ("CTDA", _ctda(629, 0x0701A634), None),
            ("CIS2", b"::iRandomPart_var\x00", None),
        ],
        quest_form_id: [("VMAD", b"\x00", "INFO")],
    }
    authoring_records = {
        quest_form_id: """
        {
          "fields": [
            {
              "VirtualMachineAdapter": {
                "Scripts": [
                  {"ScriptName": "MTNZ05QuestScript", "Properties": []}
                ],
                "Script Fragments": {
                  "Script": {
                    "ScriptName": "fragments:quests:qf_mtnz05_messenger_0001a634"
                  }
                }
              }
            }
          ]
        }
        """
    }

    monkeypatch.setattr(
        unified,
        "load_native_module",
        lambda: types.SimpleNamespace(
            conversion_run_script_reference_records=lambda _run_id, _sigs: (
                "SeventySix.esm",
                [
                    (
                        form_id,
                        summaries[form_id].signature,
                        summaries[form_id].editor_id,
                        subrecords[form_id],
                        authoring_records.get(form_id),
                    )
                    for form_id in (info_form_id, quest_form_id)
                ],
            )
        ),
    )
    runner = types.SimpleNamespace(emit_log=lambda *_args: None)

    refs, candidates = runtime._collect_script_references(1, runner)

    ref_keys = {
        (ref.kind, ref.script_name, ref.variable_name, ref.form_id) for ref in refs
    }
    assert (
        "condition",
        "MTNZ05QuestScript",
        "::iRandomPart_var",
        info_form_id,
    ) in ref_keys
    assert ("vmad", "MTNZ05QuestScript", None, quest_form_id) in ref_keys
    assert (
        "vmad",
        "fragments:quests:qf_mtnz05_messenger_0001a634",
        None,
        quest_form_id,
    ) in ref_keys
    assert sorted(candidates) == [quest_form_id, info_form_id]


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
        runtime, "_strip_failed_script_refs", lambda *_args, **_kwargs: (0, 0, 0)
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
        "_strip_failed_script_refs",
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
        runtime, "_collect_script_references", lambda *_args: (refs, {})
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
        runtime, "_strip_failed_script_refs", lambda *_args, **_kwargs: (0, 0, 0)
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
        assert resolutions[key].status == "target"
        assert not (
            mod_path / "data" / "Scripts" / "creatures" / f"{script_name}.pex"
        ).exists()
    assert resolutions["creatures:customcreaturescript"].status == "compiled"


def test_fo76_to_fo4_script_type_aliases_region():
    assert unified._fo76_to_fo4_script_type("player") == "Actor"
    assert unified._fo76_to_fo4_script_type("region") == "Form"
    assert unified._fo76_to_fo4_script_type("Region[]") == "Form[]"
    assert unified._fo76_to_fo4_script_type("QuestInstance") == "Quest"


def test_script_strip_removes_inferred_get_vm_quest_variable_condition():
    record = Record(
        signature="INFO",
        form_id=0x003C4727,
        subrecords=[
            Subrecord("CITC", (1).to_bytes(4, "little")),
            Subrecord("CTDA", _ctda(629, 0x0701A634)),
            Subrecord("CIS2", b"::iRandomPart_var\x00"),
            Subrecord("NAM1", b"kept"),
        ],
    )

    payloads, stripped_conditions, stripped_vmad = (
        unified._subrecord_payloads_after_script_strip(
            record,
            failed_script_keys={"mtnz05questscript"},
            invalid_condition_keys=set(),
            strip_vmad=False,
            scripts_by_form_id={0x0001A634: ["MTNZ05QuestScript"]},
        )
    )

    assert stripped_conditions == 1
    assert stripped_vmad == 0
    assert [item["signature"] for item in payloads] == ["CITC", "NAM1"]
    assert payloads[0]["data"] == (0).to_bytes(4, "little")


def test_strip_failed_script_refs_uses_typed_subrecord_setter(monkeypatch):
    record = Record(
        signature="INFO",
        form_id=0x003C4727,
        subrecords=[
            Subrecord("CITC", (1).to_bytes(4, "little")),
            Subrecord("CTDA", _ctda(629, 0x0701A634)),
            Subrecord("CIS2", b"::iRandomPart_var\x00"),
            Subrecord("NAM1", b"kept"),
        ],
    )
    runtime = _runtime()
    runtime._script_condition_form_scripts = {0x0001A634: ["MTNZ05QuestScript"]}
    captured = {}

    def set_record_subrecords(run_id, form_id, subrecords):
        assert all(
            isinstance(item, tuple) and len(item) == 3 for item in subrecords
        )
        captured["args"] = (run_id, form_id, subrecords)
        return True

    monkeypatch.setattr(
        unified,
        "load_native_module",
        lambda: types.SimpleNamespace(
            conversion_run_set_record_subrecords=set_record_subrecords
        ),
    )

    stripped_conditions, stripped_vmad, changed_records = (
        runtime._strip_failed_script_refs(
            42,
            {record.form_id: record},
            failed_script_keys={"mtnz05questscript"},
            invalid_condition_keys=set(),
            vmad_strip_form_ids=set(),
        )
    )

    assert (stripped_conditions, stripped_vmad, changed_records) == (1, 0, 1)
    assert captured["args"][0:2] == (42, record.form_id)
    assert [item[0] for item in captured["args"][2]] == ["CITC", "NAM1"]
    assert captured["args"][2][0][1] == (0).to_bytes(4, "little")
    assert all(item[2] is None for item in captured["args"][2])


def test_script_strip_keeps_inferred_condition_when_any_script_has_variable():
    record = Record(
        signature="INFO",
        form_id=0x003C4727,
        subrecords=[
            Subrecord("CITC", (1).to_bytes(4, "little")),
            Subrecord("CTDA", _ctda(629, 0x0701A634)),
            Subrecord("CIS2", b"::iRandomPart_var\x00"),
            Subrecord("NAM1", b"kept"),
        ],
    )

    payloads, stripped_conditions, stripped_vmad = (
        unified._subrecord_payloads_after_script_strip(
            record,
            failed_script_keys=set(),
            invalid_condition_keys={
                ("unrelatedquestscript", "::irandompart_var"),
            },
            strip_vmad=False,
            scripts_by_form_id={
                0x0001A634: ["UnrelatedQuestScript", "MTNZ05QuestScript"]
            },
        )
    )

    assert stripped_conditions == 0
    assert stripped_vmad == 0
    assert [item["signature"] for item in payloads] == [
        "CITC",
        "CTDA",
        "CIS2",
        "NAM1",
    ]
