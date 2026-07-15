from __future__ import annotations

from pathlib import Path


def _repo_root() -> Path:
    for parent in Path(__file__).resolve().parents:
        if (parent / "pyproject.toml").exists() and (parent / "py_creation_lib").exists():
            return parent
    raise AssertionError("repo root not found")


ROOT = _repo_root()


def _production_files() -> list[Path]:
    roots = [
        ROOT / "py_creation_lib" / "python" / "creation_lib" / "conversion",
        ROOT / "py_creation_lib" / "native" / "conversion" / "src",
        ROOT / "scripts",
        ROOT / "cli",
        ROOT / "ui",
    ]
    files: list[Path] = []
    for root in roots:
        if not root.exists():
            continue
        for path in root.rglob("*"):
            if not path.is_file():
                continue
            if "tests" in path.parts:
                continue
            if path.suffix not in {".py", ".rs"}:
                continue
            files.append(path)
    return files


def test_no_production_asset_port_or_legacy_fixup_references() -> None:
    assert not (
        ROOT
        / "py_creation_lib"
        / "python"
        / "creation_lib"
        / "conversion"
        / "workflows"
        / "asset_port.py"
    ).exists()

    forbidden = [
        "asset_port",
        "ConversionOrchestrator",
        "conversion_run_apply_fixups",
        "use_translate_v2",
        "use_fixups_v2",
        "build_default_fixup_registry",
        "Segment::Legacy",
        "fixup_progress_callback",
        "run.apply_fixups(",
        'insert("fixups"',
        "use_nif_engine_v2",
        "use_textures_v2",
        "ConvertNifsPhase",
        "ConvertTexturesPhase",
        "MaterialsPhase",
        'inner.insert("convert_nifs"',
        'inner.insert("convert_textures"',
        'inner.insert("convert_materials"',
        'run_phase("convert_nifs",',
        'run_phase("convert_textures",',
        'run_phase("convert_materials",',
        'phase_name="convert_nifs"',
    ]
    hits: list[str] = []
    for path in _production_files():
        text = path.read_text(encoding="utf-8", errors="ignore")
        for token in forbidden:
            if token in text:
                hits.append(f"{path.relative_to(ROOT)}: {token}")
    assert hits == []
