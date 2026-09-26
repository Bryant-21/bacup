"""exe-batch compile: selector dispatch and stale-output handling."""
from __future__ import annotations

import types
from pathlib import Path

import pytest

from creation_lib.pex.corpus import bundled_corpus_root

from bacup_lib.workflows import unified
from bacup_lib.models import PluginPortOptions, PluginPortRequest


def _fake_pex_corpus(target_data):
    """Anchor-named .pex so the type universe resolves.

    Contents are irrelevant here: these tests mock the compiler and assert only
    on the import roots it is handed. Header fidelity is covered by
    creation_lib.pex.tests.test_headers and the cache's own tests.
    """
    from pathlib import Path as _Path
    scripts = _Path(target_data) / "scripts"
    scripts.mkdir(parents=True, exist_ok=True)
    for anchor in ("ScriptObject", "Form", "ObjectReference"):
        (scripts / f"{anchor}.pex").write_bytes(b"not a real pex")
    return scripts


def _runtime_with_selector(selector: str):
    req = PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[],
        output_root=Path("out"),
        target_extracted_dir=None,
        target_data_dir=None,
        options=PluginPortOptions(papyrus_compiler=selector),
    )
    # Bypass __init__ side effects — we only exercise the compile dispatch, which
    # reads self._req, which is populated by the runtime constructor.
    return unified._UnifiedRecordRuntime(req)


@pytest.mark.parametrize("selector,method", [
    ("exe-batch", "_compile_decompiled_scripts_batch_for_fo4"),
    ("native", "_compile_decompiled_scripts_native_for_fo4"),
])
def test_selector_dispatches_to_its_compiler(monkeypatch, selector, method):
    runtime = _runtime_with_selector(selector)
    calls = []

    def fake_compile(self, script_names, *, ctx, runner, psc_paths=None, **kwargs):
        calls.append(kwargs)
        assert psc_paths is None
        return [
            (n, unified._ScriptResolution(n, "compiled", Path(f"{n}.pex")))
            for n in script_names
        ]

    monkeypatch.setattr(unified._UnifiedRecordRuntime, method, fake_compile, raising=True)

    out = runtime._compile_decompiled_scripts_for_fo4(
        ["B21:Alpha"], source_index={}, ctx=types.SimpleNamespace(mod_path="m"), runner=None, workers=4
    )
    assert calls == [{"workers": 4}] if selector == "native" else calls == [{}]
    assert out[0][1].status == "compiled"


def _batch_orch(tmp_path, mod_path, *, selector="exe-batch"):
    data_dir = tmp_path / "FO4" / "Data"
    (tmp_path / "FO4" / "Papyrus Compiler").mkdir(parents=True, exist_ok=True)
    (tmp_path / "FO4" / "Papyrus Compiler" / "PapyrusCompiler.exe").write_text("")
    (mod_path / "Scripts" / "Source" / "User" / "B21").mkdir(parents=True, exist_ok=True)
    req = PluginPortRequest(
        source_game="fo76", target_game="fo4", source_plugins=[],
        output_root=tmp_path, target_extracted_dir=None, target_data_dir=data_dir,
        options=PluginPortOptions(papyrus_compiler=selector),
    )
    return unified._UnifiedRecordRuntime(req)


def _native_orch(tmp_path, mod_path):
    data_dir = tmp_path / "FO4" / "Data"
    _fake_pex_corpus(data_dir)
    (mod_path / "Scripts" / "Source" / "User" / "B21").mkdir(parents=True, exist_ok=True)
    req = PluginPortRequest(
        source_game="fo76", target_game="fo4", source_plugins=[],
        output_root=tmp_path, target_extracted_dir=None, target_data_dir=data_dir,
        options=PluginPortOptions(papyrus_compiler="native"),
    )
    return unified._UnifiedRecordRuntime(req)


def test_native_compile_writes_pex(tmp_path, monkeypatch):
    mod_path = tmp_path / "mod"
    runtime = _native_orch(tmp_path, mod_path)
    user_root = mod_path / "Scripts" / "Source" / "User"
    _fake_pex_corpus(tmp_path / "FO4" / "Data")
    (user_root / "B21" / "Alpha.psc").write_text(
        "Scriptname B21:Alpha extends Quest\n",
        encoding="utf-8",
    )
    calls = []

    def fake_compile(source, *, imports, game, flags, source_path=None):
        calls.append({"source": source, "imports": imports, "game": game, "flags": flags})
        return types.SimpleNamespace(ok=True, pex_bytes=b"PEX", diagnostics=[])

    monkeypatch.setattr("creation_lib.pex.native_runtime.compile_psc", fake_compile)
    runner = types.SimpleNamespace(emit_log=lambda *a, **k: None)
    out = runtime._compile_decompiled_scripts_native_for_fo4(
        ["B21:Alpha"], ctx=types.SimpleNamespace(mod_path=str(mod_path)), runner=runner, workers=1)
    expected_pex = mod_path / "data" / "Scripts" / "B21" / "Alpha.pex"
    assert out[0][1].status == "compiled"
    assert out[0][1].pex_path == expected_pex
    assert expected_pex.read_bytes() == b"PEX"
    headers_root = tmp_path / "Scripts" / "GeneratedHeaders"
    assert calls == [{
        "source": "Scriptname B21:Alpha extends Quest\n",
        "imports": [str(user_root), str(headers_root), str(bundled_corpus_root("fo4"))],
        "game": "fo4",
        "flags": str(headers_root / "Institute_Papyrus_Flags.flg"),
    }]


def test_native_compile_reports_diagnostics_and_removes_stale_pex(tmp_path, monkeypatch):
    mod_path = tmp_path / "mod"
    runtime = _native_orch(tmp_path, mod_path)
    user_root = mod_path / "Scripts" / "Source" / "User"
    (user_root / "B21" / "Alpha.psc").write_text(
        "Scriptname B21:Alpha extends Quest\n",
        encoding="utf-8",
    )
    stale_pex = mod_path / "data" / "Scripts" / "B21" / "Alpha.pex"
    stale_pex.parent.mkdir(parents=True, exist_ok=True)
    stale_pex.write_bytes(b"stale")

    def fake_compile(source, *, imports, game, flags, source_path=None):
        return types.SimpleNamespace(
            ok=False,
            pex_bytes=None,
            diagnostics=[{"line": 7, "col": 3, "message": "cannot assign None to Int"}],
        )

    monkeypatch.setattr("creation_lib.pex.native_runtime.compile_psc", fake_compile)
    runner = types.SimpleNamespace(emit_log=lambda *a, **k: None)
    out = runtime._compile_decompiled_scripts_native_for_fo4(
        ["B21:Alpha"], ctx=types.SimpleNamespace(mod_path=str(mod_path)), runner=runner, workers=1)
    assert out[0][1].status == "compile_failed"
    assert out[0][1].message == "7:3: cannot assign None to Int"
    assert not stale_pex.is_file()


def test_native_compile_rejects_stale_pex_when_unlink_fails(
    tmp_path,
    monkeypatch,
):
    mod_path = tmp_path / "mod"
    runtime = _native_orch(tmp_path, mod_path)
    user_root = mod_path / "Scripts" / "Source" / "User"
    (user_root / "B21" / "Alpha.psc").write_text(
        "Scriptname B21:Alpha extends Quest\n",
        encoding="utf-8",
    )
    stale_pex = mod_path / "data" / "Scripts" / "B21" / "Alpha.pex"
    stale_pex.parent.mkdir(parents=True, exist_ok=True)
    stale_pex.write_bytes(b"stale")
    original_unlink = Path.unlink

    def fail_stale_unlink(path, *args, **kwargs):
        if path == stale_pex:
            raise PermissionError("locked")
        return original_unlink(path, *args, **kwargs)

    compile_calls = []
    monkeypatch.setattr(Path, "unlink", fail_stale_unlink)
    monkeypatch.setattr(
        "creation_lib.pex.native_runtime.compile_psc",
        lambda *_args, **_kwargs: compile_calls.append(True),
    )

    out = runtime._compile_decompiled_scripts_native_for_fo4(
        ["B21:Alpha"],
        ctx=types.SimpleNamespace(mod_path=str(mod_path)),
        runner=types.SimpleNamespace(emit_log=lambda *_args: None),
        workers=1,
    )

    assert out[0][1].status == "compile_failed"
    assert "could not remove stale output" in out[0][1].message
    assert compile_calls == []
    assert stale_pex.read_bytes() == b"stale"


def test_batch_reconstructs_compile_failed_when_no_pex(tmp_path, monkeypatch):
    import subprocess as sp
    import types
    mod_path = tmp_path / "mod"
    runtime = _batch_orch(tmp_path, mod_path)
    # Pre-create a stale .pex: the pre-delete must remove it so the (no-output)
    # batch run still reconstructs compile_failed instead of a masked "compiled".
    stale_pex = mod_path / "data" / "Scripts" / "B21" / "Alpha.pex"
    stale_pex.parent.mkdir(parents=True, exist_ok=True)
    stale_pex.write_bytes(b"\xde\xad\xbe\xef")
    monkeypatch.setattr(unified.subprocess, "run",
        lambda *a, **k: sp.CompletedProcess(a[0] if a else k.get("args"), 0, stdout="", stderr=""))
    runner = types.SimpleNamespace(emit_log=lambda *a, **k: None)
    out = runtime._compile_decompiled_scripts_batch_for_fo4(
        ["B21:Alpha"], ctx=types.SimpleNamespace(mod_path=str(mod_path)), runner=runner)
    assert out[0][1].status == "compile_failed"
    assert not stale_pex.is_file()


def test_batch_reconstructs_compiled_when_pex_present(tmp_path, monkeypatch):
    import subprocess as sp
    import types
    mod_path = tmp_path / "mod"
    runtime = _batch_orch(tmp_path, mod_path)
    expected_pex = mod_path / "data" / "Scripts" / "B21" / "Alpha.pex"

    def fake_run(*a, **k):
        expected_pex.parent.mkdir(parents=True, exist_ok=True)
        expected_pex.write_bytes(b"\xfa\x57\xc0\xde")
        return sp.CompletedProcess(a[0] if a else k.get("args"), 0, stdout="", stderr="")

    monkeypatch.setattr(unified.subprocess, "run", fake_run)
    runner = types.SimpleNamespace(emit_log=lambda *a, **k: None)
    out = runtime._compile_decompiled_scripts_batch_for_fo4(
        ["B21:Alpha"], ctx=types.SimpleNamespace(mod_path=str(mod_path)), runner=runner)
    assert out[0][1].status == "compiled"
    assert out[0][1].pex_path == expected_pex


def test_perscript_compile_failure_does_not_accept_stale_pex(tmp_path, monkeypatch):
    import subprocess as sp

    mod_path = tmp_path / "mod"
    runtime = _batch_orch(tmp_path, mod_path, selector="exe")
    stale_pex = mod_path / "data" / "Scripts" / "B21" / "Alpha.pex"
    stale_pex.parent.mkdir(parents=True, exist_ok=True)
    stale_pex.write_bytes(b"stale")
    monkeypatch.setattr(
        unified.subprocess,
        "run",
        lambda *args, **kwargs: sp.CompletedProcess(
            args[0] if args else kwargs.get("args"),
            1,
            stdout="compile failed",
            stderr="",
        ),
    )

    out = runtime._compile_decompiled_script_for_fo4(
        "B21:Alpha",
        tmp_path / "source.pex",
        types.SimpleNamespace(mod_path=str(mod_path)),
        types.SimpleNamespace(emit_log=lambda *_args: None),
        cleanup_on_failure=False,
    )

    assert out.status == "compile_failed"
    assert out.message == "compile failed"
    assert not stale_pex.exists()


def test_batch_compiler_unavailable_when_exe_missing(tmp_path):
    import types
    mod_path = tmp_path / "mod"
    (mod_path / "Scripts" / "Source" / "User").mkdir(parents=True, exist_ok=True)
    data_dir = tmp_path / "FO4" / "Data"  # note: NO "Papyrus Compiler" dir -> exe missing
    req = PluginPortRequest(
        source_game="fo76", target_game="fo4", source_plugins=[],
        output_root=tmp_path, target_extracted_dir=None, target_data_dir=data_dir,
        options=PluginPortOptions(papyrus_compiler="exe-batch"),
    )
    runtime = unified._UnifiedRecordRuntime(req)
    out = runtime._compile_decompiled_scripts_batch_for_fo4(
        ["B21:Alpha"], ctx=types.SimpleNamespace(mod_path=str(mod_path)),
        runner=types.SimpleNamespace(emit_log=lambda *a, **k: None))
    assert out[0][1].status == "compiler_unavailable"
