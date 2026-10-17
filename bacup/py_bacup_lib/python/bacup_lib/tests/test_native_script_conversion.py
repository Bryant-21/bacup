from __future__ import annotations

import json
from pathlib import Path

import pytest

from bacup_lib.run import ConversionRun
from creation_lib.pex.native_runtime import compile_psc


def _pex(path: Path, source: str) -> None:
    result = compile_psc(source, game="fo4")
    assert result.ok, result.diagnostics
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(result.pex_bytes)


def _convert(tmp_path: Path, *, workers: int = 2) -> dict:
    for name in ("source", "resources", "headers", "target"):
        (tmp_path / name).mkdir(exist_ok=True)
    with ConversionRun.create_new("fo76", "fo4", None, "Probe.esp") as run:
        return run.run_phase(
            "convert_scripts",
            mod_path=str(tmp_path / "output"),
            target_extracted_dir=str(tmp_path / "target"),
            params={
                "source_data_dir": str(tmp_path / "source"),
                "resources_root": str(tmp_path / "resources"),
                "bundled_corpus_root": str(tmp_path / "headers"),
                "workers": workers,
            },
        )


def _report(tmp_path: Path) -> dict[str, dict]:
    rows = json.loads((tmp_path / "output/script_port_report.json").read_text())["scripts"]
    return {row["script_name"].lower(): row for row in rows}


def test_native_script_phase_client_priority_and_target_protection(tmp_path):
    _pex(tmp_path / "source/scripts/client/Converted.pex", "Scriptname Converted\nInt Function Value()\nReturn 11\nEndFunction\n")
    _pex(tmp_path / "source/scripts/Converted.pex", "Scriptname Converted\nInt Function Value()\nReturn 22\nEndFunction\n")
    _pex(tmp_path / "source/scripts/Protected.pex", "Scriptname Protected\n")
    _pex(tmp_path / "target/Scripts/Protected.pex", "Scriptname Protected\n")
    for relative in ("Scripts/Source/User/Protected.psc", "data/Scripts/Protected.pex"):
        path = tmp_path / "output" / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"stale")

    phase = _convert(tmp_path)

    assert phase["assets_written"] == 1
    report = _report(tmp_path)
    assert report["converted"]["status"] == "compiled"
    assert report["protected"]["status"] == "target"
    assert "Return 11" in (tmp_path / "output/Scripts/Source/User/Converted.psc").read_text()
    assert not (tmp_path / "output/data/Scripts/Protected.pex").exists()
    assert not (tmp_path / "output/Scripts/Source/User/Protected.psc").exists()


def test_native_script_phase_merges_and_verifies_functional_patch(tmp_path):
    _pex(tmp_path / "source/scripts/Patched.pex", "Scriptname Patched\nInt Function Value()\nReturn 1\nEndFunction\n")
    patch = tmp_path / "resources/script_patches/Patched.psc"
    patch.parent.mkdir(parents=True)
    patch.write_text("Int Function Value()\nReturn 99\nEndFunction\n")

    _convert(tmp_path)

    assert _report(tmp_path)["patched"]["status"] == "compiled"
    assert "Return 99" in (tmp_path / "output/Scripts/Source/User/Patched.psc").read_text()
    timings = json.loads((tmp_path / "output/script_port_timing.json").read_text())
    assert {event["name"] for event in timings["events"]} >= {
        "convert_scripts_resolution", "convert_scripts_decompile_port", "convert_scripts_compile",
    }


def test_native_script_phase_rejects_undelivered_patch(tmp_path):
    patch = tmp_path / "resources/script_patches/Missing.psc"
    patch.parent.mkdir(parents=True)
    patch.write_text("Function Required()\nEndFunction\n")

    with pytest.raises(RuntimeError, match="functional Papyrus patch delivery failed"):
        _convert(tmp_path)


def test_native_script_phase_removes_bad_source_outputs(tmp_path):
    path = tmp_path / "source/scripts/Corrupt.pex"
    path.parent.mkdir(parents=True)
    path.write_bytes(b"not a PEX")
    stale = tmp_path / "output/data/Scripts/Corrupt.pex"
    stale.parent.mkdir(parents=True)
    stale.write_bytes(b"old successful run")

    _convert(tmp_path, workers=1)

    assert _report(tmp_path)["corrupt"]["status"] == "decompile_failed"
    assert not stale.exists()


def test_native_script_phase_rejects_other_pairs_before_writing(tmp_path):
    with ConversionRun.create_new("skyrimse", "fo4", None, "Probe.esp") as run:
        with pytest.raises(ValueError, match="supports only fo76:fo4"):
            run.run_phase("convert_scripts", mod_path=str(tmp_path / "output"))
    assert not (tmp_path / "output").exists()


@pytest.mark.parametrize("compiler", ["exe", "exe-batch"])
def test_native_script_phase_aborts_on_unlaunchable_compiler(tmp_path, compiler):
    from bacup_lib.tests.native_script_fixtures import native_scripts, write_pex

    with native_scripts(tmp_path, refs=[("Referenced", "ACTI")]) as fixture:
        write_pex(fixture.source / "Scripts/Referenced.pex", "Referenced")
        executable = tmp_path / "Papyrus Compiler/PapyrusCompiler.exe"
        executable.parent.mkdir()
        executable.write_bytes(b"not an executable")
        fixture.runtime._req.options.papyrus_compiler = compiler
        before = fixture.saved_subrecords(0x800, "VMAD")

        with pytest.raises(RuntimeError, match="Papyrus toolchain unavailable"):
            fixture.convert()

        assert fixture.saved_subrecords(0x800, "VMAD") == before
        assert not (fixture.ctx.diagnostics_root / "script_port_report.json").exists()
