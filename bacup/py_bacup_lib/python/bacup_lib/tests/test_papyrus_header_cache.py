"""The type universe is materialized from the target game's own `.pex`."""
from __future__ import annotations

from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import papyrus_header_cache as cache


def _corpus(root: Path, names=("ScriptObject", "Form", "ObjectReference")) -> Path:
    scripts = root / "scripts"
    scripts.mkdir(parents=True, exist_ok=True)
    for name in names:
        (scripts / f"{name}.pex").write_bytes(b"pex")
    return scripts


def test_corpus_needs_every_anchor_type_and_first_viable_root_wins(tmp_path):
    """An empty Scripts dir passes is_dir() and still compiles nothing."""
    assert cache.find_pex_corpus(_corpus(tmp_path / "empty", names=())) is None
    assert cache.find_pex_corpus(_corpus(tmp_path / "partial", names=("ScriptObject", "Form"))) is None
    good = _corpus(tmp_path / "ext")
    assert cache.find_pex_corpus(tmp_path / "missing", good) == good


@pytest.mark.parametrize("corpus_location", ["loose", "extracted"])
@pytest.mark.parametrize("cached_headers", [False, True])
def test_partial_corpus_compiles_calls_to_missing_api_types(
    tmp_path, corpus_location, cached_headers
):
    from bacup_lib.workflows.unified import _UnifiedRecordRuntime
    from creation_lib.pex.native_runtime import compile_psc

    target_root = tmp_path / "target"
    scripts = target_root / "scripts"
    scripts.mkdir(parents=True)
    for name, parent in (
        ("ScriptObject", ""), ("Form", "ScriptObject"), ("ObjectReference", "Form")
    ):
        source = f"Scriptname {name}" + (f" Extends {parent}" if parent else "") + "\n"
        source_path = scripts / f"{name}.psc"
        source_path.write_text(source, encoding="utf-8")
        result = compile_psc(source, imports=[str(scripts)], game="fo4")
        assert result.ok, result.diagnostics
        (scripts / f"{name}.pex").write_bytes(result.pex_bytes)

    mod_path = tmp_path / "mod"
    source_root = mod_path / "Scripts/Source/User"
    source_root.mkdir(parents=True)
    script_name = "B21_PartialCorpusCaller"
    source = (
        f"Scriptname {script_name} Extends ObjectReference\n"
        "Float Function ReadValue(GlobalVariable setting)\n"
        "    Return setting.GetValue()\n"
        "EndFunction\n"
        "Int Function PlaySound(Sound effect)\n"
        "    Return effect.Play(Self)\n"
        "EndFunction\n"
        "Bool Function StartStory(Keyword story)\n"
        "    Return story.SendStoryEventAndWait(None, Self)\n"
        "EndFunction\n"
    )
    (source_root / f"{script_name}.psc").write_text(source, encoding="utf-8")
    request = SimpleNamespace(
        target_game="fo4",
        output_root=mod_path,
        target_data_dir=target_root if corpus_location == "loose" else None,
        target_extracted_dir=target_root if corpus_location == "extracted" else None,
    )
    ctx = SimpleNamespace(mod_path=mod_path, target_asset_store=None)
    runner = SimpleNamespace(emit_log=lambda *_args: None)
    if cached_headers:
        _UnifiedRecordRuntime(request)._papyrus_type_universe(ctx, runner)

    runtime = _UnifiedRecordRuntime(request)
    results = runtime._compile_decompiled_scripts_native_for_fo4(
        [script_name], ctx=ctx, runner=runner, workers=1
    )
    resolution = results[0][1]
    assert resolution.status == "compiled", resolution.message
    assert resolution.pex_path.is_file()


def test_bundled_imports_preserve_local_api_declarations(tmp_path, monkeypatch):
    from bacup_lib.workflows.unified import _UnifiedRecordRuntime
    from creation_lib.pex.native_runtime import compile_psc

    local_headers = tmp_path / "headers"
    local_headers.mkdir()
    (local_headers / "GlobalVariable.psc").write_text(
        "Scriptname GlobalVariable Extends Form\n"
        "Int Function GetValue() Native\n",
        encoding="utf-8",
    )
    runtime = _UnifiedRecordRuntime(
        SimpleNamespace(target_game="fo4", output_root=tmp_path)
    )
    monkeypatch.setattr(runtime, "_papyrus_type_universe", lambda *_args: local_headers)
    imports = runtime._papyrus_import_roots(
        SimpleNamespace(), SimpleNamespace(emit_log=lambda *_args: None)
    )
    result = compile_psc(
        "Scriptname B21_LocalApiCaller\n"
        "Int Function ReadValue(GlobalVariable setting)\n"
        "    Return setting.GetValue()\n"
        "EndFunction\n",
        imports=[str(path) for path in imports],
        game="fo4",
    )
    assert result.ok, result.diagnostics


def test_materialize_pex_corpus_unpacks_from_the_archives(tmp_path):
    """The corpus root is derived from where the store actually put the files."""
    from bacup_lib import papyrus_header_cache

    cache = tmp_path / "cache" / "Scripts"
    cache.mkdir(parents=True)
    assets = ["scripts/ScriptObject.pex", "scripts/Form.pex", "scripts/ObjectReference.pex"]

    class _Store:
        def list_assets(self, *, prefix="", suffix=""):
            return list(assets)

        def materialize(self, path):
            return cache / Path(path).name

        def materialize_many(self, paths, include_dependencies=False):
            return [cache / Path(p).name for p in paths]

    assert papyrus_header_cache.materialize_pex_corpus(_Store()) == cache


def test_materialize_pex_corpus_rejects_a_partial_archive(tmp_path):
    """Missing an anchor means the type graph cannot resolve; say so up front."""
    from bacup_lib import papyrus_header_cache

    class _Store:
        def list_assets(self, *, prefix="", suffix=""):
            return ["scripts/Form.pex"]

        def materialize(self, path):  # pragma: no cover - must not be reached
            raise AssertionError("should not materialize a partial corpus")

    assert papyrus_header_cache.materialize_pex_corpus(_Store()) is None
