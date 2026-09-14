from __future__ import annotations

import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.native_runtime import _fnv_result_from_raw
from bacup_lib.source_pairs import (
    FNV_MVP_EXCLUDE_SIGNATURES,
    FNV_QUEST_SLICE_ENABLED_SIGNATURES,
    FNV_QUEST_SLICE_EXCLUDE_SIGNATURES,
    FNV_QUEST_SLICE_RECORD_FORM_IDS,
    fnv_quest_slice_record_form_ids,
    is_fnv_quest_slice_exclusion_set,
    quest_slice_exclude_signatures,
)
from bacup_lib.workflows.unified import _ScriptResolution, _UnifiedRecordRuntime


class _Runner:
    def __init__(self) -> None:
        self.logs: list[tuple[str, str]] = []

    def emit_log(self, level: str, message: str) -> None:
        self.logs.append((level, message))


def _runtime(tmp_path: Path, *, convert_scripts: bool = True) -> _UnifiedRecordRuntime:
    runtime = object.__new__(_UnifiedRecordRuntime)
    runtime._req = SimpleNamespace(
        source_game="fnv",
        target_game="fo4",
        target_data_dir=None,
        options=SimpleNamespace(
            exclude_signatures=FNV_QUEST_SLICE_EXCLUDE_SIGNATURES,
            fnv_quest_slice=True,
            convert_scripts=convert_scripts,
            conversion_workers=1,
            fnv_unmapped_function_policy="halt",
        ),
    )
    return runtime


def _context(tmp_path: Path) -> SimpleNamespace:
    return SimpleNamespace(
        mod_path=tmp_path,
        diagnostics_root=tmp_path / "diagnostics",
        summary=SimpleNamespace(
            records_translated=0,
            records_warnings=0,
            lip_regeneration_needed=[],
        ),
    )


def _legacy_result(**overrides: object) -> dict[str, object]:
    result: dict[str, object] = {
        "translated_scripts": 1,
        "translated_quests": 1,
        "translated_infos": 1,
        "translated_scenes": 0,
        "records_written": 3,
        "records_failed": 0,
        "psc_files_written": 1,
        "psc_files_skipped": 0,
        "generated_psc_classes": ["B21_VTechatticup"],
        "skipped_records": [],
        "lip_regeneration_needed": [],
        "vmad_intents": [],
        "vmad_attached_in_rust": True,
    }
    result.update(overrides)
    return result


def test_quest_slice_keeps_unsupported_pack_family_excluded() -> None:
    assert (
        quest_slice_exclude_signatures("fnvfo3:fo4")
        == FNV_QUEST_SLICE_EXCLUDE_SIGNATURES
    )
    assert FNV_QUEST_SLICE_EXCLUDE_SIGNATURES < FNV_MVP_EXCLUDE_SIGNATURES
    assert "PACK" in FNV_QUEST_SLICE_EXCLUDE_SIGNATURES
    assert not (FNV_QUEST_SLICE_ENABLED_SIGNATURES & FNV_QUEST_SLICE_EXCLUDE_SIGNATURES)
    assert is_fnv_quest_slice_exclusion_set(FNV_QUEST_SLICE_EXCLUDE_SIGNATURES)
    assert not is_fnv_quest_slice_exclusion_set(FNV_MVP_EXCLUDE_SIGNATURES)


def test_quest_slice_dependency_closure_is_exact_and_json_ready() -> None:
    assert "ACRE" not in FNV_QUEST_SLICE_RECORD_FORM_IDS
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["ACHR"] == (
        0x12319B,
        0x12319C,
        0x134B9C,
    )
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["ACTI"] == (0x133F41,)
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["QUST"] == (0x06136D, 0x11F935)
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["REFR"] == (0x133F42,)
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["DIAL"] == (0x13015B, 0x134B9A, 0x138A74)
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["INFO"] == (
        0x130161,
        0x134B9B,
        0x15734B,
        0x15734C,
        0x15734D,
    )
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["SPEL"] == (0x172091,)
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["MGEF"] == (0x0CB05D,)
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["PACK"] == (
        0x1231B6,
        0x1231B7,
        0x13289E,
        0x133F3E,
    )
    copy = fnv_quest_slice_record_form_ids()
    assert "ACRE" not in copy
    assert copy["ACHR"] == [0x12319B, 0x12319C, 0x134B9C]
    copy["QUST"].append(0xFFFFFF)
    assert FNV_QUEST_SLICE_RECORD_FORM_IDS["QUST"] == (0x06136D, 0x11F935)


def test_world_only_mvp_skips_legacy_psc_work(tmp_path: Path) -> None:
    runtime = _runtime(tmp_path, convert_scripts=False)
    runtime._req.options.exclude_signatures = FNV_MVP_EXCLUDE_SIGNATURES
    runtime._req.options.fnv_quest_slice = False
    assert runtime._is_fnv_world_only_mvp()
    assert not runtime._is_fnv_quest_slice()


def test_world_only_mvp_fence_overrides_ui_script_default(tmp_path: Path) -> None:
    runtime = _runtime(tmp_path, convert_scripts=True)
    runtime._req.options.exclude_signatures = FNV_MVP_EXCLUDE_SIGNATURES
    runtime._req.options.fnv_quest_slice = False

    assert runtime._is_fnv_world_only_mvp()


def test_explicit_quest_slice_gate_survives_unrelated_extra_excludes(
    tmp_path: Path,
) -> None:
    runtime = _runtime(tmp_path)
    runtime._req.options.exclude_signatures = FNV_QUEST_SLICE_EXCLUDE_SIGNATURES | {
        "TREE"
    }
    assert runtime._is_fnv_quest_slice()


def test_explicit_quest_slice_gate_defaults_false(tmp_path: Path) -> None:
    runtime = _runtime(tmp_path)
    runtime._req.options.fnv_quest_slice = False
    assert not runtime._is_fnv_quest_slice()


@pytest.mark.parametrize(
    "field", ["records_failed", "skipped_records", "psc_files_skipped"]
)
def test_strict_slice_rejects_incomplete_accounting_before_compile(
    tmp_path: Path, field: str
) -> None:
    runtime = _runtime(tmp_path)
    ctx = _context(tmp_path)
    runner = _Runner()
    value: object = (
        [("INFO", "00130161", "missing")] if field == "skipped_records" else 1
    )
    with pytest.raises(RuntimeError, match="incomplete accounting"):
        runtime._apply_fnv_legacy_result_dict(
            _legacy_result(**{field: value}), ctx, runner
        )


def test_strict_full_slice_compiles_exact_required_psc_manifest(
    tmp_path: Path, monkeypatch
) -> None:
    runtime = _runtime(tmp_path)
    ctx = _context(tmp_path)
    runner = _Runner()
    manifest = [
        ("FNV_FO3_FnvSliceCompat", "fnv-slice-compatibility"),
        ("FNV_FO3_S_11FC64", "scpt"),
        ("FNV_FO3_S_123191", "scpt"),
        ("FNV_FO3_S_134491", "scpt"),
        ("FNV_FO3_S_166305", "scpt"),
        ("QF_FNV_FO3_06136D", "quest-fragment"),
        ("QF_FNV_FO3_11F935", "quest-fragment"),
        ("TIF__130161", "topic-info-fragment"),
        ("TIF__134B9B", "topic-info-fragment"),
    ]
    manifest_names = [class_name for class_name, _kind in manifest]
    assert len({class_name.casefold() for class_name in manifest_names}) == 9
    assert max(map(len, manifest_names)) <= 38
    generated = []
    for class_name, kind in manifest:
        relative_source_path = f"Scripts/Source/User/{class_name}.psc"
        source_path = tmp_path / relative_source_path
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_text(
            f"ScriptName {class_name} extends Quest\n", encoding="utf-8"
        )
        generated.append(
            {
                "class_name": class_name,
                "relative_source_path": relative_source_path,
                "kind": kind,
                "required": True,
            }
        )

    compile_calls: list[list[str]] = []

    def compile_manifest(names, **_kwargs):
        compile_calls.append(list(names))
        return [
            (
                name,
                _ScriptResolution(
                    name,
                    "compiled",
                    tmp_path / "data" / "Scripts" / f"{name}.pex",
                ),
            )
            for name in names
        ]

    monkeypatch.setattr(
        runtime, "_compile_decompiled_scripts_native_for_fo4", compile_manifest
    )
    runtime._apply_fnv_legacy_result_dict(
        _legacy_result(
            translated_scripts=4,
            translated_quests=2,
            translated_infos=5,
            records_written=18,
            psc_files_written=9,
            psc_files_skipped=0,
            generated_psc_classes=generated,
        ),
        ctx,
        runner,
    )

    assert compile_calls == [manifest_names]
    assert {
        class_name for class_name, _kind in manifest if class_name.startswith("TIF__")
    } == {
        "TIF__130161",
        "TIF__134B9B",
    }
    assert any(
        "translated 4 SCPT, 2 QUST, 5 INFO, 0 SCEN" in message and "psc=9/9" in message
        for _level, message in runner.logs
    )


@pytest.mark.parametrize(
    ("class_name", "source_stem", "script_name", "reason"),
    [
        ("B21_" + "A" * 35, "B21_" + "A" * 35, "B21_" + "A" * 35, "exceeds"),
        ("B21_Valid", "B21_" + "B" * 35, "B21_Valid", "filename stem exceeds"),
        ("B21_Valid", "B21_Valid", "B21_Different", "does not match manifest"),
    ],
)
def test_strict_slice_rejects_invalid_psc_manifest_before_compile(
    tmp_path: Path,
    monkeypatch,
    class_name: str,
    source_stem: str,
    script_name: str,
    reason: str,
) -> None:
    runtime = _runtime(tmp_path)
    ctx = _context(tmp_path)
    runner = _Runner()
    source_path = tmp_path / "Scripts" / "Source" / "User" / f"{source_stem}.psc"
    source_path.parent.mkdir(parents=True)
    source_path.write_text(
        f"ScriptName {script_name} extends Quest\n", encoding="utf-8"
    )
    calls: list[list[str]] = []

    def compile_manifest(names, **_kwargs):
        calls.append(list(names))
        return []

    monkeypatch.setattr(
        runtime, "_compile_decompiled_scripts_for_fo4", compile_manifest
    )

    with pytest.raises(RuntimeError, match=reason):
        runtime._compile_fnv_generated_psc_manifest(
            manifest=[
                {
                    "class_name": class_name,
                    "relative_source_path": f"{source_stem}.psc",
                    "kind": "quest",
                    "required": True,
                }
            ],
            psc_files_written=1,
            ctx=ctx,
            runner=runner,
        )

    assert calls == []


def test_fnv_manifest_rejects_case_insensitive_duplicate_class_names(
    tmp_path: Path,
) -> None:
    runtime = _runtime(tmp_path)
    with pytest.raises(ValueError, match="duplicate generated PSC class name"):
        runtime._fnv_generated_psc_manifest(
            {
                "generated_psc_classes": [
                    {
                        "class_name": "B21_Same",
                        "relative_source_path": "B21_Same.psc",
                    },
                    {
                        "class_name": "b21_same",
                        "relative_source_path": "b21_same.psc",
                    },
                ]
            }
        )


def test_fnv_compiler_uses_only_current_run_manifest(
    tmp_path: Path, monkeypatch
) -> None:
    runtime = _runtime(tmp_path)
    ctx = _context(tmp_path)
    runner = _Runner()
    psc_path = tmp_path / "Scripts" / "Source" / "User" / "B21_VTechatticup.psc"
    psc_path.parent.mkdir(parents=True)
    psc_path.write_text("ScriptName B21_VTechatticup extends Quest\n", encoding="utf-8")
    calls: list[list[str]] = []

    def compile_manifest(names, **_kwargs):
        calls.append(names)
        return [
            (
                name,
                _ScriptResolution(
                    name, "compiled", tmp_path / "data" / "Scripts" / f"{name}.pex"
                ),
            )
            for name in names
        ]

    monkeypatch.setattr(
        runtime, "_compile_decompiled_scripts_native_for_fo4", compile_manifest
    )
    runtime._apply_fnv_legacy_result_dict(_legacy_result(), ctx, runner)

    assert calls == [["B21_VTechatticup"]]
    report = json.loads(
        (ctx.diagnostics_root / "fnv_script_compile_manifest.json").read_text(
            encoding="utf-8"
        )
    )
    assert report["generated_psc_classes"] == [
        {
            "class_name": "B21_VTechatticup",
            "relative_source_path": "B21_VTechatticup.psc",
            "kind": "unknown",
            "required": True,
        }
    ]
    assert report["results"] == [
        {
            "class_name": "B21_VTechatticup",
            "relative_source_path": "B21_VTechatticup.psc",
            "kind": "unknown",
            "required": True,
            "status": "compiled",
            "pex_path": str(tmp_path / "data" / "Scripts" / "B21_VTechatticup.pex"),
            "message": "",
        }
    ]


def test_fnv_manifest_honors_exe_batch_compiler(tmp_path: Path, monkeypatch) -> None:
    runtime = _runtime(tmp_path)
    runtime._req.options.papyrus_compiler = "exe-batch"
    ctx = _context(tmp_path)
    runner = _Runner()
    psc_root = tmp_path / "Scripts" / "Source" / "User"
    psc_root.mkdir(parents=True)
    selected = psc_root / "B21_VTechatticup.psc"
    selected.write_text("ScriptName B21_VTechatticup extends Quest\n", encoding="utf-8")
    (psc_root / "StaleSibling.psc").write_text(
        "ScriptName StaleSibling extends Quest\n", encoding="utf-8"
    )
    calls: list[tuple[list[str], dict[str, Path]]] = []

    def compile_batch(names, **kwargs):
        calls.append((list(names), dict(kwargs["psc_paths"])))
        return [
            (
                name,
                _ScriptResolution(
                    name, "compiled", tmp_path / "data" / "Scripts" / f"{name}.pex"
                ),
            )
            for name in names
        ]

    monkeypatch.setattr(
        runtime, "_compile_decompiled_scripts_batch_for_fo4", compile_batch
    )
    monkeypatch.setattr(
        runtime,
        "_compile_decompiled_scripts_native_for_fo4",
        lambda *_args, **_kwargs: pytest.fail("exe-batch must not route to native"),
    )

    runtime._compile_fnv_generated_psc_manifest(
        manifest=[
            {
                "class_name": "B21_VTechatticup",
                "relative_source_path": "B21_VTechatticup.psc",
                "kind": "quest",
                "required": True,
            }
        ],
        psc_files_written=1,
        ctx=ctx,
        runner=runner,
    )

    assert calls == [
        (
            ["B21_VTechatticup"],
            {"B21_VTechatticup": selected},
        )
    ]


def test_exe_batch_stages_only_current_fnv_manifest(
    tmp_path: Path, monkeypatch
) -> None:
    runtime = _runtime(tmp_path)
    runtime._req.options.papyrus_compiler = "exe-batch"
    target_data = tmp_path / "Fallout4" / "Data"
    compiler = target_data.parent / "Papyrus Compiler" / "PapyrusCompiler.exe"
    compiler.parent.mkdir(parents=True)
    compiler.write_bytes(b"fixture")
    runtime._req.target_data_dir = target_data
    ctx = _context(tmp_path)
    psc_root = tmp_path / "Scripts" / "Source" / "User"
    psc_root.mkdir(parents=True)
    selected = psc_root / "B21_VTechatticup.psc"
    selected.write_text("ScriptName B21_VTechatticup extends Quest\n", encoding="utf-8")
    (psc_root / "StaleSibling.psc").write_text(
        "ScriptName StaleSibling extends Quest\n", encoding="utf-8"
    )
    staged_roots: list[Path] = []

    monkeypatch.setattr(
        "creation_lib.core.game_profiles.get_profile",
        lambda _game: SimpleNamespace(
            papyrus_compiler_dir="Papyrus Compiler",
            papyrus_flags=None,
        ),
    )

    def fake_run(command, **kwargs):
        staged_root = Path(command[1])
        staged_roots.append(staged_root)
        assert Path(kwargs["cwd"]) == staged_root
        assert [path.name for path in staged_root.rglob("*.psc")] == [
            "B21_VTechatticup.psc"
        ]
        output_root = Path(
            next(
                arg.removeprefix("-output=")
                for arg in command
                if arg.startswith("-output=")
            )
        )
        output_root.mkdir(parents=True, exist_ok=True)
        (output_root / "B21_VTechatticup.pex").write_bytes(b"compiled")
        return SimpleNamespace(stdout="", returncode=0)

    monkeypatch.setattr("bacup_lib.workflows.unified.subprocess.run", fake_run)

    results = runtime._compile_decompiled_scripts_batch_for_fo4(
        ["B21_VTechatticup"],
        ctx=ctx,
        runner=_Runner(),
        psc_paths={"B21_VTechatticup": selected},
    )

    assert results[0][1].status == "compiled"
    assert staged_roots and not staged_roots[0].exists()


def test_voice_manifest_uses_hardened_multi_root_contract(
    tmp_path: Path, monkeypatch
) -> None:
    runtime = _runtime(tmp_path)
    primary = tmp_path / "fnv" / "Data"
    fo3 = tmp_path / "fo3" / "Data"
    ctx = _context(tmp_path)
    ctx.source_data_dir = primary
    ctx.additional_source_asset_roots = (fo3,)
    ctx.output_plugin_name = "Converted.esm"
    runner = _Runner()
    psc_path = tmp_path / "Scripts" / "Source" / "User" / "B21_VTechatticup.psc"
    psc_path.parent.mkdir(parents=True)
    psc_path.write_text("ScriptName B21_VTechatticup extends Quest\n", encoding="utf-8")
    monkeypatch.setattr(
        runtime,
        "_compile_decompiled_scripts_native_for_fo4",
        lambda names, **_kwargs: [
            (
                name,
                _ScriptResolution(
                    name, "compiled", tmp_path / "data" / "Scripts" / f"{name}.pex"
                ),
            )
            for name in names
        ],
    )
    captured: dict[str, object] = {}

    def process_voice_manifest(manifest_path, **kwargs):
        captured["manifest_path"] = manifest_path
        captured.update(kwargs)
        return []

    monkeypatch.setattr(
        "bacup_lib.fnv_voice.process_voice_manifest", process_voice_manifest
    )
    monkeypatch.setattr(
        "bacup_lib.fnv_voice.write_voice_processing_report", lambda *_: None
    )
    runtime._apply_fnv_legacy_result_dict(
        _legacy_result(
            voice_manifest={
                "version": 1,
                "entries": [
                    {
                        "origin_plugin": "FalloutNV.esm",
                        "source_root": "fnv-base",
                    }
                ],
            }
        ),
        ctx,
        runner,
    )

    assert captured["source_roots"] == {
        "fnv-base": (primary, fo3),
        "fo3-base": (primary, fo3),
    }
    assert captured["output_mod_dir"] == tmp_path
    assert captured["target_plugin"] == "Converted.esm"
    assert captured["strict"] is True
    assert captured["workers"] == 1
    assert "source_data_dir" not in captured
    assert "output_data_dir" not in captured


def test_quest_slice_rejects_psc_without_current_run_manifest(tmp_path: Path) -> None:
    runtime = _runtime(tmp_path)
    with pytest.raises(RuntimeError, match="generated_psc_classes manifest"):
        runtime._apply_fnv_legacy_result_dict(
            _legacy_result(generated_psc_classes=[]),
            _context(tmp_path),
            _Runner(),
        )


def test_fnv_legacy_duplicate_phase_is_guarded(tmp_path: Path) -> None:
    runtime = _runtime(tmp_path)
    ctx = _context(tmp_path)
    ctx._fnv_legacy_completed = True
    assert (
        runtime._run_optional_fnv_legacy_phase(ctx, Path("FNV.esm"), _Runner()) is False
    )


def test_native_fnv_result_accepts_legacy_and_structured_psc_manifests() -> None:
    legacy = (1, 2, 3, 4, 5, 6, 7, 8, 9, [], [], [], True)
    legacy_result = _fnv_result_from_raw(legacy)
    assert legacy_result["generated_psc_classes"] == []
    assert legacy_result["quest_runtime_components"] == []

    structured = (
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        [],
        [],
        [],
        True,
        [
            {
                "class_name": "B21_VTechatticup",
                "relative_source_path": "B21_VTechatticup.psc",
                "kind": "quest",
                "required": True,
            }
        ],
    )
    structured_result = _fnv_result_from_raw(structured)
    assert structured_result["generated_psc_classes"][0]["kind"] == "quest"
    assert structured_result["quest_runtime_components"] == []

    current = structured + ('{"version":1,"entries":[]}',)
    assert _fnv_result_from_raw(current)["voice_manifest"] == {
        "version": 1,
        "entries": [],
    }

    component = {
        "component_id": "fnv:falloutnv.esm:11f935",
        "source_root": {"signature": "QUST", "form_key": "11F935@FalloutNV.esm"},
        "target_root": {"signature": "QUST", "form_key": "11F935@FalloutNV.esm"},
        "provenance": {
            "game": "fo3",
            "source_plugin": "Fallout3.esm",
            "graft": {
                "source_plugin": "Fallout3.esm",
                "source_form_key": "012F935@Fallout3.esm",
                "graft_form_key": "11F935@FalloutNV.esm",
            },
        },
        "start_disposition": {"kind": "autostart"},
        "inbound_producers": [],
        "start_routes": [
            {
                "route_id": "fnvfo3:start:fo3:fallout3.esm:012f935:autostart",
                "quest": {"signature": "QUST", "form_key": "11F935@FalloutNV.esm"},
                "producer_evidence_id": "autostart:11F935@FalloutNV.esm",
                "node_chain": [],
            }
        ],
    }
    receipt_result = _fnv_result_from_raw(current + (json.dumps([component]),))
    assert receipt_result["quest_runtime_components"] == [component]
