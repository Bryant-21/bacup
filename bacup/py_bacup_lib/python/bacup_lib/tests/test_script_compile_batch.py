"""exe-batch compile: selector dispatch + (env-gated) real-exe parity."""
from __future__ import annotations

import types
from pathlib import Path

from bacup_lib.workflows import unified
from bacup_lib.models import PluginPortOptions, PluginPortRequest


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


def test_selector_exe_batch_dispatches_to_batch(monkeypatch):
    runtime = _runtime_with_selector("exe-batch")
    calls = {"batch": 0, "perscript": 0}

    def fake_batch(self, script_names, *, ctx, runner):
        calls["batch"] += 1
        return [
            (n, unified._ScriptResolution(n, "compiled", Path(f"{n}.pex")))
            for n in script_names
        ]

    monkeypatch.setattr(
        unified._UnifiedRecordRuntime,
        "_compile_decompiled_scripts_batch_for_fo4",
        fake_batch,
        raising=True,
    )

    out = runtime._compile_decompiled_scripts_for_fo4(
        ["B21:Alpha"], source_index={}, ctx=types.SimpleNamespace(mod_path="m"), runner=None, workers=4
    )
    assert calls["batch"] == 1
    assert out[0][1].status == "compiled"


def test_selector_native_dispatches_to_native(monkeypatch):
    runtime = _runtime_with_selector("native")
    calls = {"native": 0}

    def fake_native(self, script_names, *, ctx, runner, workers):
        calls["native"] += 1
        assert workers == 4
        return [
            (n, unified._ScriptResolution(n, "compiled", Path(f"{n}.pex")))
            for n in script_names
        ]

    monkeypatch.setattr(
        unified._UnifiedRecordRuntime,
        "_compile_decompiled_scripts_native_for_fo4",
        fake_native,
        raising=True,
    )

    out = runtime._compile_decompiled_scripts_for_fo4(
        ["B21:Alpha"], source_index={}, ctx=types.SimpleNamespace(mod_path="m"), runner=None, workers=4
    )
    assert calls["native"] == 1
    assert out[0][1].status == "compiled"


import os
import subprocess
import pytest

from creation_lib.pex import decompile_pex


def _fo4_paths():
    fo4_dir = os.environ.get("FO4_DIR", "").strip().strip('"')
    if not fo4_dir:
        return None
    base = Path(fo4_dir)
    compiler = base / "Papyrus Compiler" / "PapyrusCompiler.exe"
    scripts_base = base / "Data" / "Scripts" / "Source" / "Base"
    if not compiler.is_file() or not scripts_base.is_dir():
        return None
    return compiler, scripts_base


@pytest.mark.skipif(_fo4_paths() is None, reason="FO4 PapyrusCompiler.exe / Base scripts not available")
def test_batch_output_matches_perscript_semantically(tmp_path):
    compiler, scripts_base = _fo4_paths()
    psc_root = tmp_path / "Scripts" / "Source" / "User" / "B21"
    psc_root.mkdir(parents=True)
    # Two trivial namespaced scripts (namespace B21 -> compiles to B21\Name.pex).
    (psc_root / "Alpha.psc").write_text(
        "Scriptname B21:Alpha extends Quest\nInt Function Two()\n    Return 2\nEndFunction\n",
        encoding="utf-8",
    )
    (psc_root / "Beta.psc").write_text(
        "Scriptname B21:Beta extends Quest\nString Function Hi()\n    Return \"hi\"\nEndFunction\n",
        encoding="utf-8",
    )

    user_root = tmp_path / "Scripts" / "Source" / "User"
    import_arg = f"{user_root};{scripts_base}"

    def compile_perscript(out_dir: Path):
        out_dir.mkdir(parents=True, exist_ok=True)
        for rel in ("B21/Alpha", "B21/Beta"):
            subprocess.run(
                [str(compiler), rel, f"-import={import_arg}", f"-output={out_dir}", "-quiet"],
                cwd=str(user_root), check=False, capture_output=True, text=True,
            )

    def compile_batch(out_dir: Path):
        out_dir.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [str(compiler), str(user_root), "-all", f"-import={import_arg}", f"-output={out_dir}", "-quiet"],
            cwd=str(user_root), check=False, capture_output=True, text=True,
        )

    per_dir = tmp_path / "out_perscript"
    batch_dir = tmp_path / "out_batch"
    compile_perscript(per_dir)
    compile_batch(batch_dir)

    for rel in ("B21/Alpha.pex", "B21/Beta.pex"):
        per_pex = per_dir / rel
        batch_pex = batch_dir / rel
        assert per_pex.is_file(), f"per-script missing {rel}"
        assert batch_pex.is_file(), f"batch missing {rel} (-all path mapping differs?)"
        # Semantic equality: decompiled source is timestamp-free, unlike raw .pex headers.
        assert decompile_pex(batch_pex) == decompile_pex(per_pex), f"{rel} semantics diverged"


def _batch_orch(tmp_path, mod_path):
    data_dir = tmp_path / "FO4" / "Data"
    (tmp_path / "FO4" / "Papyrus Compiler").mkdir(parents=True, exist_ok=True)
    (tmp_path / "FO4" / "Papyrus Compiler" / "PapyrusCompiler.exe").write_text("")
    (mod_path / "Scripts" / "Source" / "User" / "B21").mkdir(parents=True, exist_ok=True)
    req = PluginPortRequest(
        source_game="fo76", target_game="fo4", source_plugins=[],
        output_root=tmp_path, target_extracted_dir=None, target_data_dir=data_dir,
        options=PluginPortOptions(papyrus_compiler="exe-batch"),
    )
    return unified._UnifiedRecordRuntime(req)


def _native_orch(tmp_path, mod_path):
    data_dir = tmp_path / "FO4" / "Data"
    (data_dir / "Scripts" / "Source" / "Base").mkdir(parents=True, exist_ok=True)
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
    base_root = tmp_path / "FO4" / "Data" / "Scripts" / "Source" / "Base"
    flags_path = base_root / "Institute_Papyrus_Flags.flg"
    flags_path.write_text("flag data", encoding="utf-8")
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
    assert calls == [{
        "source": "Scriptname B21:Alpha extends Quest\n",
        "imports": [str(user_root), str(base_root)],
        "game": "fo4",
        "flags": str(flags_path),
    }]


def test_native_compile_uses_target_data_sources(tmp_path, monkeypatch):
    mod_path = tmp_path / "mod"
    user_root = mod_path / "Scripts" / "Source" / "User"
    (user_root / "B21").mkdir(parents=True, exist_ok=True)
    (user_root / "B21" / "Alpha.psc").write_text(
        "Scriptname B21:Alpha extends Quest\n",
        encoding="utf-8",
    )
    target_data = tmp_path / "Fallout4" / "Data"
    game_base = target_data / "Scripts" / "Source" / "Base"
    game_base.mkdir(parents=True)
    flags_path = game_base / "Institute_Papyrus_Flags.flg"
    flags_path.write_text("flag data", encoding="utf-8")
    req = PluginPortRequest(
        source_game="fo76", target_game="fo4", source_plugins=[],
        output_root=tmp_path, target_extracted_dir=None,
        target_data_dir=target_data, options=PluginPortOptions(papyrus_compiler="native"),
    )
    runtime = unified._UnifiedRecordRuntime(req)
    calls = []

    def fake_compile(source, *, imports, game, flags, source_path=None):
        calls.append({"imports": imports, "flags": flags})
        return types.SimpleNamespace(ok=True, pex_bytes=b"PEX", diagnostics=[])

    monkeypatch.setattr("creation_lib.pex.native_runtime.compile_psc", fake_compile)
    runner = types.SimpleNamespace(emit_log=lambda *a, **k: None)
    out = runtime._compile_decompiled_scripts_native_for_fo4(
        ["B21:Alpha"], ctx=types.SimpleNamespace(mod_path=str(mod_path)), runner=runner, workers=1)
    assert out[0][1].status == "compiled"
    assert calls == [
        {"imports": [str(user_root), str(game_base)], "flags": str(flags_path)}
    ]


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
