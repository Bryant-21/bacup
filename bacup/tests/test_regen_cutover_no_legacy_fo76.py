from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SCAN_ROOTS = ("bacup", "py_creation_lib", "scripts", "tools", "tests")
DELETED_FILES = (
    Path("scripts") / ("regen_" + "fo76.py"),
    Path("scripts") / ("regen_" + "fo76_smoke.py"),
    Path("scripts") / ("regen_" + "appalachia.py"),
    Path("scripts") / ("regen_" + "appalachia_cell00.py"),
    Path("scripts") / ("regen_" + "fnv.py"),
    Path("py_creation_lib")
    / "python"
    / "creation_lib"
    / "conversion"
    / "workflows"
    / ("plugin_" + "port.py"),
)
FORBIDDEN = (
    "PluginPort" + "Orchestrator",
    "workflows." + "plugin_port",
    "run_" + "plugin_port",
    "regen_" + "fo76.py",
    "regen_" + "appalachia_cell00.py",
    "scripts/" + "regen_fnv.py",
)


def test_cutover_deleted_legacy_files_are_absent():
    assert [path.as_posix() for path in DELETED_FILES if (ROOT / path).exists()] == []


def test_cutover_does_not_reference_deleted_legacy_entrypoints():
    offenders: list[str] = []
    current = Path(__file__).resolve()
    for root_name in SCAN_ROOTS:
        root = ROOT / root_name
        if not root.exists():
            continue
        for path in root.rglob("*.py"):
            if path.resolve() == current:
                continue
            text = path.read_text(encoding="utf-8", errors="ignore")
            for needle in FORBIDDEN:
                if needle in text:
                    offenders.append(f"{path.relative_to(ROOT)}: {needle}")

    assert offenders == []
