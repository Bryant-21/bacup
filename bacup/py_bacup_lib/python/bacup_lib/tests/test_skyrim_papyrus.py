from __future__ import annotations

from hashlib import sha256
from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import skyrim_papyrus
from bacup_lib.skyrim_papyrus import SkyrimScriptIntent


def test_extract_minimal_quest_fragment_actions_accepts_only_supported_calls() -> None:
    source = """
ScriptName QF_Test Extends Quest
Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
    SetObjectiveCompleted(10, False)
    SetStage(20)
    CompleteQuest()
    Stop()
EndFunction
"""

    assert skyrim_papyrus.extract_minimal_quest_fragment_actions(
        source, "Fragment_Stage_0010_Item_00"
    ) == [
        {"kind": "set_objective_displayed", "index": 10, "displayed": True},
        {"kind": "set_objective_completed", "index": 10, "completed": False},
        {"kind": "set_stage", "index": 20},
        {"kind": "complete_quest"},
        {"kind": "stop"},
    ]


def test_extract_minimal_quest_fragment_actions_fails_closed() -> None:
    source = """
Function Fragment_Stage_0010_Item_00()
    Utility.Wait(1.0)
EndFunction
"""

    with pytest.raises(ValueError, match="unsupported statement"):
        skyrim_papyrus.extract_minimal_quest_fragment_actions(
            source, "Fragment_Stage_0010_Item_00"
        )


def _compile_ok(*_args, **_kwargs):
    return SimpleNamespace(ok=True, pex_bytes=b"compiled", diagnostics=[])


def _prepare_source_stubs(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(
        skyrim_papyrus, "_adapt_psc_source_for_fo4", lambda source: source
    )
    monkeypatch.setattr(
        skyrim_papyrus, "_validate_script_identity", lambda *_args: None
    )
    monkeypatch.setattr(skyrim_papyrus, "_compile_fo4_source", _compile_ok)


def test_loose_psc_precedes_loose_pex_and_emits_positive_receipt(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _prepare_source_stubs(monkeypatch)
    source = "ScriptName LooseWinner Extends Quest\n"
    psc = tmp_path / "Data" / "Scripts" / "Source" / "User" / "LooseWinner.psc"
    psc.parent.mkdir(parents=True)
    psc.write_text(source, encoding="utf-8")
    pex = tmp_path / "Data" / "Scripts" / "LooseWinner.pex"
    pex.parent.mkdir(parents=True, exist_ok=True)
    pex.write_bytes(b"ignored pex")

    result = skyrim_papyrus.discover_skyrim_psc_artifacts(
        [SkyrimScriptIntent("LooseWinner", "helper")],
        source_data_dir=tmp_path / "Data",
        fo4_import_dirs=(),
        temp_root=tmp_path / "tmp",
    )

    assert result.supported
    assert [artifact.class_name for artifact in result.artifacts] == ["LooseWinner"]
    assert result.receipts[0].origin == "loose-psc"
    assert result.receipts[0].input_sha256 == sha256(psc.read_bytes()).hexdigest()
    assert result.receipts[0].fo4_validation == "compiled"


def test_archive_psc_precedes_loose_pex(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _prepare_source_stubs(monkeypatch)
    data = tmp_path / "Data"
    archive = data / "Source Scripts.bsa"
    archive.parent.mkdir(parents=True)
    archive.write_bytes(b"archive")
    pex = data / "Scripts" / "ArchiveSource.pex"
    pex.parent.mkdir(parents=True)
    pex.write_bytes(b"ignored")
    member = "Source/Scripts/ArchiveSource.psc"
    payload = b"ScriptName ArchiveSource Extends Quest\n"
    monkeypatch.setattr(
        "creation_lib.ba2.native_runtime.list_archive", lambda _path: [member]
    )
    monkeypatch.setattr(
        "creation_lib.ba2.native_runtime.extract_one",
        lambda _archive, requested: payload if requested == member else None,
    )

    result = skyrim_papyrus.discover_skyrim_psc_artifacts(
        [SkyrimScriptIntent("ArchiveSource", "helper")],
        source_data_dir=data,
        fo4_import_dirs=(),
        temp_root=tmp_path / "tmp",
    )

    assert result.supported
    assert result.receipts[0].origin == "archive-psc"
    assert result.receipts[0].member == member


def test_archive_pex_is_decompiled_with_fo4_adapters(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(
        skyrim_papyrus, "_validate_script_identity", lambda *_args: None
    )
    monkeypatch.setattr(skyrim_papyrus, "_compile_fo4_source", _compile_ok)
    archive = tmp_path / "Scripts.bsa"
    archive.write_bytes(b"archive")
    member = "Scripts/ArchiveOnly.pex"
    payload = b"compiled skyrim script"
    monkeypatch.setattr(
        "creation_lib.ba2.native_runtime.list_archive", lambda _path: [member]
    )
    monkeypatch.setattr(
        "creation_lib.ba2.native_runtime.extract_one",
        lambda _archive, requested: payload if requested == member else None,
    )

    def decompile(path: Path) -> str:
        assert path.read_bytes() == payload
        return "ScriptName ArchiveOnly Extends Quest\n"

    monkeypatch.setattr(skyrim_papyrus, "_decompile_for_fo4", decompile)
    result = skyrim_papyrus.discover_skyrim_psc_artifacts(
        [SkyrimScriptIntent("ArchiveOnly", "helper")],
        source_data_dir=tmp_path,
        fo4_import_dirs=(),
        temp_root=tmp_path / "tmp",
    )

    assert result.supported
    receipt = result.receipts[0]
    assert receipt.origin == "archive-pex"
    assert receipt.input_sha256 == sha256(payload).hexdigest()
    assert receipt.adapters == (
        "skyrim-type-map-v1",
        "fo4-api-compat-v1",
        "skip-internal-functions-v1",
    )


def test_required_fo4_validation_failure_rejects_atomically(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    _prepare_source_stubs(monkeypatch)
    psc = tmp_path / "Scripts" / "Source" / "RejectMe.psc"
    psc.parent.mkdir(parents=True)
    psc.write_text("ScriptName RejectMe Extends Quest\n", encoding="utf-8")
    monkeypatch.setattr(
        skyrim_papyrus,
        "_compile_fo4_source",
        lambda *_args, **_kwargs: SimpleNamespace(
            ok=False,
            pex_bytes=None,
            diagnostics=[{"message": "unknown FO4 API"}],
        ),
    )

    result = skyrim_papyrus.discover_skyrim_psc_artifacts(
        [SkyrimScriptIntent("RejectMe", "helper")],
        source_data_dir=tmp_path,
        fo4_import_dirs=(),
        temp_root=tmp_path / "tmp",
    )

    assert not result.supported
    assert result.artifacts == ()
    assert "unknown FO4 API" in (result.unsupported_reason or "")


def test_optional_missing_script_does_not_reject_component(tmp_path: Path) -> None:
    result = skyrim_papyrus.discover_skyrim_psc_artifacts(
        [SkyrimScriptIntent("OptionalScript", "record-script", required=False)],
        source_data_dir=tmp_path,
        fo4_import_dirs=(),
        temp_root=tmp_path / "tmp",
    )

    assert result.supported
    assert result.artifacts == ()
    assert result.unsupported_reason is None


def test_skyrim_source_type_adapter_is_ast_safe() -> None:
    source = """ScriptName TypeProbe Extends Quest
ActorValueInfo Property ValueType Auto

Function Probe(ArmorAddon[] Items)
    Apparatus LocalItem
EndFunction
"""

    adapted = skyrim_papyrus._adapt_psc_source_for_fo4(source)

    assert "ActorValue Property ValueType" in adapted
    assert "Function Probe(Form[] Items)" in adapted
    assert "Form LocalItem" in adapted
    assert "ActorValueInfo" not in adapted
    assert "ArmorAddon" not in adapted
