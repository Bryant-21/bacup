from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib import regen_pipeline
from bacup_lib.regen_pipeline import RegenPaths


def _paths(tmp_path: Path) -> RegenPaths:
    return RegenPaths(
        source_extracted_dir=tmp_path / "fo76_extracted",
        source_data_dir=tmp_path / "fo76" / "Data",
        target_extracted_dir=tmp_path / "fo4_extracted",
        target_data_dir=tmp_path / "Fallout4" / "Data",
        target_ck_ini_path=tmp_path / "Fallout4" / "CreationKitCustom.ini",
        target_custom_ini_path=tmp_path / "Fallout4Custom.ini",
        target_game_ini_path=tmp_path / "Fallout4.ini",
        output_root=tmp_path / "BACUP" / "mods" / "SeventySix",
        resource_dir=tmp_path / "resource",
    )


def _runner(logs: list[str]) -> SimpleNamespace:
    return SimpleNamespace(
        emit_log=lambda _level, message: logs.append(message),
        emit_status=lambda *_a, **_k: None,
        is_cancelled=lambda: False,
    )


def test_clean_scripts_output_removes_stale_pex_and_psc(tmp_path):
    paths = _paths(tmp_path)
    pex = paths.output_root / "data" / "Scripts" / "Creatures"
    psc = paths.output_root / "Scripts" / "Source" / "User" / "Creatures"
    pex.mkdir(parents=True)
    psc.mkdir(parents=True)
    (pex / "ScorchedSuiciderScript.pex").write_bytes(b"stale")
    (psc / "ScorchedSuiciderScript.psc").write_text("stale", encoding="utf-8")
    kept = paths.output_root / "data" / "meshes" / "keep.nif"
    kept.parent.mkdir(parents=True)
    kept.write_bytes(b"keep")

    logs: list[str] = []
    regen_pipeline._clean_scripts_output(paths, _runner(logs))

    assert not (paths.output_root / "data" / "Scripts").exists()
    assert not (paths.output_root / "Scripts" / "Source" / "User").exists()
    assert kept.read_bytes() == b"keep"
    assert any("cleared stale script output" in message for message in logs)


def test_clean_scripts_output_is_a_noop_without_prior_scripts(tmp_path):
    paths = _paths(tmp_path)
    paths.output_root.mkdir(parents=True)

    logs: list[str] = []
    regen_pipeline._clean_scripts_output(paths, _runner(logs))

    assert logs == []


def test_clean_scripts_output_refuses_protected_output_root(tmp_path):
    paths = _paths(tmp_path)
    object.__setattr__(paths, "output_root", paths.target_data_dir)

    with pytest.raises(ValueError, match="protected conversion path"):
        regen_pipeline._clean_scripts_output(paths, _runner([]))
