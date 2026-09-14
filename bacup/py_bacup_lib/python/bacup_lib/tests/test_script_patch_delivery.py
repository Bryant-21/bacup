from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.models import ConversionSummary
from bacup_lib.tests.test_unified_driver import StubRunner, make_request
from bacup_lib.workflows import unified as unified_mod
from bacup_lib.workflows.unified import (
    _ScriptPatchEntry,
    _ScriptResolution,
    _UnifiedRecordRuntime,
    _assert_compiled_script_patch_members,
    _assert_functional_script_patches_delivered,
    _assert_script_patch_members_merged,
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
)


def test_namespaced_return_type_is_recognized_and_merged_once() -> None:
    skeleton = (
        "Scriptname TestScript Extends Quest\n\nFunction Existing()\nEndFunction\n"
    )
    patch = (
        "Quests:Storm:QuestScript Function QuestController()\n"
        "    return None\n"
        "EndFunction\n"
    )

    merged = _merge_script_method_patches(skeleton, patch)
    members = _iter_top_level_papyrus_members(merged.splitlines())

    assert [name for _kind, name, _start, _end in members].count("questcontroller") == 1
    _assert_script_patch_members_merged(patch, merged)


def test_merge_postcondition_rejects_missing_patch_member() -> None:
    patch = "Quests:Storm:QuestScript Function QuestController()\nEndFunction\n"
    merged = "Scriptname TestScript Extends Quest\n"

    with pytest.raises(
        ValueError, match="expected exactly one function questcontroller"
    ):
        _assert_script_patch_members_merged(patch, merged)


def test_compiled_pex_gate_accepts_then_rejects_namespaced_patch_member(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    pex_path = tmp_path / "TestScript.pex"
    pex_path.write_bytes(b"pex")
    patch = "Quests:Storm:QuestScript Function QuestController()\nEndFunction\n"
    parsed = SimpleNamespace(
        objects=[
            SimpleNamespace(
                variables=[],
                properties=[],
                states=[
                    SimpleNamespace(
                        name="",
                        functions=[SimpleNamespace(name="QuestController")],
                    )
                ],
            )
        ]
    )
    monkeypatch.setattr("creation_lib.pex.parse_pex", lambda _path: parsed)

    _assert_compiled_script_patch_members(pex_path, patch)
    parsed.objects[0].states[0].functions.clear()

    with pytest.raises(ValueError, match="did not retain.*questcontroller"):
        _assert_compiled_script_patch_members(pex_path, patch)


def test_functional_patch_failure_aborts_delivery() -> None:
    entry = _ScriptPatchEntry(
        script_name="Quests:Storm:QuestScript",
        path=Path("QuestScript.psc"),
        source="Function QuestController()\nEndFunction\n",
        member_count=1,
        directive_count=0,
    )
    resolutions = {
        "quests:storm:questscript": _ScriptResolution(
            "Quests:Storm:QuestScript", "compile_failed", message="bad source"
        )
    }

    with pytest.raises(RuntimeError, match="functional Papyrus patch delivery failed"):
        _assert_functional_script_patches_delivered(
            {"quests:storm:questscript": entry}, resolutions
        )


def test_directive_only_patch_is_asserted_and_fail_closed() -> None:
    skeleton = (
        "Scriptname TestScript Extends Quest\n"
        "Int Property ObsoleteStage Auto\n\n"
        "Function ObsoleteHandler()\n"
        "EndFunction\n"
    )
    patch = "; @drop-member ObsoleteHandler\n; @drop-property ObsoleteStage\n"
    merged = _merge_script_method_patches(skeleton, patch)
    assert "ObsoleteHandler" not in merged
    assert "ObsoleteStage" not in merged

    entry = _ScriptPatchEntry(
        script_name="TestScript",
        path=Path("TestScript.psc"),
        source=patch,
        member_count=0,
        directive_count=2,
    )
    with pytest.raises(RuntimeError, match="functional Papyrus patch delivery failed"):
        _assert_functional_script_patches_delivered(
            {"testscript": entry},
            {"testscript": _ScriptResolution("TestScript", "compile_failed")},
        )


def test_loose_target_script_does_not_shadow_source_but_extracted_target_does(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    request = make_request(tmp_path)
    request.source_data_dir = tmp_path / "source"
    request.target_data_dir = tmp_path / "Fallout4" / "Data"
    request.target_extracted_dir = tmp_path / "target-extracted"
    source_scripts = request.source_data_dir / "Scripts" / "Client"
    loose_target_scripts = request.target_data_dir / "Scripts"
    extracted_target_scripts = request.target_extracted_dir / "Scripts"
    for root in (source_scripts, loose_target_scripts, extracted_target_scripts):
        root.mkdir(parents=True)
    for name in ("LooseCollision", "AuthoritativeCollision"):
        (source_scripts / f"{name}.pex").write_bytes(b"source")
        (loose_target_scripts / f"{name}.pex").write_bytes(b"loose target")
    (extracted_target_scripts / "AuthoritativeCollision.pex").write_bytes(
        b"authoritative target"
    )

    runtime = _UnifiedRecordRuntime(request)
    ported: list[str] = []
    reported: dict[str, _ScriptResolution] = {}

    def decompile(script_names, **_kwargs):
        ported.extend(script_names)
        return [(name, None) for name in script_names]

    def compile_scripts(script_names, **kwargs):
        output_root = Path(kwargs["ctx"].mod_path) / "data" / "Scripts"
        output_root.mkdir(parents=True, exist_ok=True)
        results = []
        for name in script_names:
            pex_path = output_root / f"{name}.pex"
            pex_path.write_bytes(b"compiled")
            results.append((name, _ScriptResolution(name, "compiled", pex_path)))
        return results

    monkeypatch.setattr(unified_mod, "_script_patch_inventory", lambda: {})
    monkeypatch.setattr(unified_mod, "_record_timing", lambda *_args, **_kwargs: None)
    monkeypatch.setattr(runtime, "_decompile_source_scripts_for_fo4", decompile)
    monkeypatch.setattr(runtime, "_compile_decompiled_scripts_for_fo4", compile_scripts)
    monkeypatch.setattr(
        runtime,
        "_write_script_port_report",
        lambda _ctx, *, resolutions, **_kwargs: reported.update(resolutions),
    )
    ctx = SimpleNamespace(
        _rust_conversion_run=None,
        mod_path=tmp_path / "mod",
        target_asset_store=None,
        summary=ConversionSummary(),
    )

    runtime._run_convert_scripts_phase(ctx, StubRunner())

    assert ported == ["LooseCollision"]
    assert reported["loosecollision"].status == "compiled"
    assert reported["authoritativecollision"].status == "target"
