from __future__ import annotations

import json
import types
from pathlib import Path

import pytest

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.workflows import unified
from bacup_lib.run import ConversionRun
from bacup_lib.tests.native_script_fixtures import (
    assert_pex,
    native_scripts,
    script_path,
    write_pex,
)


def test_native_scripts_use_catalog_membership_without_materializing(tmp_path):
    import sqlite3
    from bacup_lib.tests.test_target_assets import _write_catalog

    with native_scripts(tmp_path, [("WorkshopScript", "ACTI")]) as fixture:
        catalog = tmp_path / "catalog.sqlite3"
        _write_catalog(catalog)
        with sqlite3.connect(catalog) as db:
            db.execute("UPDATE directories SET path_key='scripts/client' WHERE id=1")
            db.execute(
                "UPDATE assets SET name_key='workshopscript.pex', kind='script' WHERE id=1"
            )
        (fixture.target / "Fallout4 - Test.ba2").write_bytes(b"ba2")
        fixture.run.close()
        fixture.run = ConversionRun.open_existing(
            "fo76",
            "fo4",
            None,
            str(tmp_path / "Target.esp"),
            config={
                "target_data_dir": str(fixture.target),
                "target_asset_catalog_path": str(catalog),
                "target_asset_cache_dir": str(tmp_path / "cache"),
            },
        )
        report = fixture.convert()
        assert report["scripts"] == [
            {
                "script_name": "WorkshopScript",
                "status": "target",
                "pex_path": None,
                "message": "",
            }
        ]
        assert not list((tmp_path / "cache").rglob("*.pex"))
        assert not fixture.psc("WorkshopScript").exists()
        assert [ref[0] for ref in fixture.references()] == ["WorkshopScript"]


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
        "timings": (
            (
                "convert_scripts_collect_native",
                runtime._script_reference_profile["timings"][0][1],
            ),
        ),
        "counters": {
            "native_rows": 2,
            "vmad_rows": 1,
            "ctda_rows": 1,
            "subrecords": 3,
            "raw_bytes": 91,
            "authoring_bytes": 0,
        },
    }
    assert any(
        "redirected 1 Default2StateActivator" in message for _level, message in logs
    )
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


def test_convert_scripts_phase_includes_all_fo76_source_scripts(tmp_path):
    with native_scripts(tmp_path) as fixture:
        for name in ("QuestInstance", "UnusedUtility"):
            write_pex(
                fixture.source / "scripts/client" / script_path(name, ".pex"), name
            )
        report = fixture.convert()
        assert [row["script_name"] for row in report["scripts"]] == [
            "QuestInstance",
            "UnusedUtility",
        ]
        assert all(row["status"] == "compiled" for row in report["scripts"])
        for name in ("QuestInstance", "UnusedUtility"):
            assert_pex(fixture.pex(name), name)
        assert report["changed_records"] == 0


def test_native_script_phase_persists_vmad_and_condition_repairs(tmp_path):
    from creation_lib.esp.model import Subrecord

    conditions = [
        Subrecord("CTDA", _ctda(629, 0x800)),
        Subrecord("CIS1", b"WorkingScript\0"),
        Subrecord("CTDA", _ctda(629, 0x801)),
        Subrecord("CIS1", b"MissingScript\0"),
    ]
    with native_scripts(
        tmp_path,
        [("WorkingScript", "ACTI"), ("MissingScript", "ACTI")],
        {0x800: conditions},
    ) as fixture:
        write_pex(fixture.source / "Scripts/WorkingScript.pex", "WorkingScript")
        report = fixture.convert()
        assert report["stripped_conditions"] == report["stripped_vmad"] == 1
        assert report["changed_records"] == 2
        assert report["vmad_changes"]
        assert report["vmad_changes"] == [
            "ACTI 00000801 Fixture000801: removed the whole VMAD; no bound script survived"
        ]
        assert_pex(fixture.pex("WorkingScript"), "WorkingScript")
        assert len(fixture.saved_subrecords(0x800, "VMAD")) == 1
        assert fixture.saved_subrecords(0x800, "CTDA") == [_ctda(629, 0x800)]
        assert fixture.saved_subrecords(0x800, "CIS1") == [b"WorkingScript\0"]
        assert fixture.saved_subrecords(0x801, "VMAD") == []


def test_convert_scripts_phase_runs_without_a_target_plugin_handle(
    monkeypatch, tmp_path
):
    with native_scripts(tmp_path) as fixture:
        write_pex(fixture.source / "scripts/client/SourceOnly.pex", "SourceOnly")
        del fixture.ctx._rust_conversion_run
        created = []
        create_new = ConversionRun.create_new

        def create(*args, **kwargs):
            run = create_new(*args, **kwargs)
            created.append(run)
            dispatch = run.run_phase

            def run_phase(name, **params):
                params["params"].update(
                    resources_root=str(fixture.resources),
                    bundled_corpus_root=str(fixture.headers),
                )
                return dispatch(name, **params)

            run.run_phase = run_phase
            return run

        monkeypatch.setattr(ConversionRun, "create_new", create)
        monkeypatch.setattr(fixture.runtime, "_native_run_config", lambda _ctx: {})
        fixture.runtime._run_convert_scripts_phase(
            fixture.ctx, types.SimpleNamespace(emit_log=lambda *_: None)
        )
        assert len(created) == 1
        assert created[0]._closed
        report = json.loads(
            (fixture.ctx.diagnostics_root / "script_port_report.json").read_text()
        )
        assert report["changed_records"] == report["stripped_vmad"] == 0
        assert [(row["script_name"], row["status"]) for row in report["scripts"]] == [
            ("SourceOnly", "compiled")
        ]
        assert_pex(fixture.pex("SourceOnly"), "SourceOnly")


def test_skyrim_script_recovery_compiles_archive_sources(monkeypatch, tmp_path):
    from bacup_lib import skyrim_papyrus

    archive = tmp_path / "Scripts.bsa"
    archive.write_bytes(b"archive fixture")
    member = "Scripts/Source/ArchiveObjectScript.psc"
    source = b"Scriptname ArchiveObjectScript\n"
    monkeypatch.setattr(
        "creation_lib.ba2.native_runtime.list_archive", lambda _path: [member]
    )
    monkeypatch.setattr(
        "creation_lib.ba2.native_runtime.extract_one",
        lambda _path, name: source if name == member else None,
    )
    result = skyrim_papyrus.discover_skyrim_psc_artifacts(
        [skyrim_papyrus.SkyrimScriptIntent("ArchiveObjectScript", "record-vmad")],
        source_data_dir=tmp_path,
        fo4_import_dirs=(),
        temp_root=tmp_path / "work",
    )
    assert result.supported, result.unsupported_reason
    assert [artifact.class_name for artifact in result.artifacts] == [
        "ArchiveObjectScript"
    ]
    assert result.receipts[0].origin == "archive-psc"
    assert result.receipts[0].member == member
    assert result.receipts[0].fo4_validation == "compiled"


def test_convert_scripts_suppresses_explicit_base_creature_refs_but_keeps_custom(
    tmp_path,
):
    protected = [
        "creatures:MirelurkQueenRaceScript",
        "creatures:radscorpionracescript",
        "creatures:SMBehemothRaceScript",
    ]
    custom = "creatures:CustomCreatureScript"
    with native_scripts(
        tmp_path, [(name, "RACE") for name in [*protected, custom]]
    ) as fixture:
        for name in [*protected, custom]:
            write_pex(
                fixture.source / "scripts/client" / script_path(name, ".pex"), name
            )
        for name in protected:
            stale = fixture.pex(name)
            stale.parent.mkdir(parents=True, exist_ok=True)
            stale.write_bytes(b"stale")
        report = fixture.convert()
        resolutions = {row["script_name"].lower(): row for row in report["scripts"]}
        for name in protected:
            assert resolutions[name.lower()]["status"] == "unsupported"
            assert not fixture.pex(name).exists()
        assert resolutions[custom.lower()]["status"] == "compiled"
        assert_pex(fixture.pex(custom), custom)
        assert [ref[0] for ref in fixture.references()] == [custom]
        assert report["stripped_vmad"] == report["changed_records"] == len(protected)
        assert fixture.ctx.summary.scripts_flagged == 0


def test_native_script_conversion_adapts_fo76_types(tmp_path):
    from creation_lib.pex.native_runtime import compile_psc

    with native_scripts(tmp_path) as fixture:
        imports = tmp_path / "source-headers"
        imports.mkdir()
        for name in ("Player", "Region", "QuestInstance"):
            (imports / f"{name}.psc").write_text(f"Scriptname {name}\n")
        for name in ("Actor", "Form", "Quest"):
            (fixture.headers / f"{name}.psc").write_text(f"Scriptname {name}\n")
        source = "Scriptname SourceTypes\nPlayer Property User Auto\nRegion[] Property Areas Auto\nQuestInstance Property Mission Auto\n"
        result = compile_psc(source, imports=[str(imports)], game="fo4")
        assert result.ok, result.diagnostics
        source_path = fixture.source / "Scripts/SourceTypes.pex"
        source_path.parent.mkdir()
        source_path.write_bytes(result.pex_bytes)
        report = fixture.convert()
        assert report["scripts"][0]["status"] == "compiled", report
        converted = fixture.psc("SourceTypes").read_text().lower()
        assert "actor property user auto" in converted
        assert "form[] property areas auto" in converted
        assert "quest property mission auto" in converted
        assert_pex(fixture.pex("SourceTypes"), "SourceTypes")


@pytest.mark.parametrize(
    "source_game,target_game",
    [("skyrimse", "fo4"), ("fo4", "fo4"), ("fo76", "starfield")],
)
def test_convert_scripts_wrapper_rejects_unsupported_pairs(
    tmp_path, source_game, target_game
):
    with native_scripts(tmp_path) as fixture:
        fixture.runtime._req.source_game = source_game
        fixture.runtime._req.target_game = target_game
        with pytest.raises(ValueError, match="Convert Scripts supports only fo76:fo4"):
            fixture.convert()
        assert fixture.dispatches == []
