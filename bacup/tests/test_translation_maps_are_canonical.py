from __future__ import annotations

from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[2]
MAP_DIR = (
    ROOT
    / "bacup"
    / "py_bacup_lib"
    / "native"
    / "conversion"
    / "src"
    / "embedded"
    / "translation_maps"
)
LEGACY_MAP_DIR = (
    ROOT
    / "py_creation_lib"
    / "python"
    / "creation_lib"
    / "conversion"
    / "record"
    / "translation_maps"
)

OLD_SHAPE_TERMS = (
    "MutagenObjectType",
    "Spriggit YAML keys",
    "FormKey",
    "EditorID",
    "Weapons",
    "Armors",
    "ArmorAddons",
    "Npcs",
    "Races",
    "Ammunitions",
)

REQUIRED_MAP_FILENAMES = {
    "fnv_to_fo4.yaml",
    "fo3_to_fo4.yaml",
    "fo4_to_skyrimse.yaml",
    "fo76_to_skyrimse.yaml",
    "skyrimse_to_fo4.yaml",
    "starfield_to_fo4.yaml",
}


def _active_translation_maps() -> list[Path]:
    return sorted(
        path
        for path in MAP_DIR.glob("*_to_*.yaml")
        if not path.name.startswith(("ammo_", "events_", "skeleton_"))
    )


def test_legacy_python_translation_maps_removed() -> None:
    assert not LEGACY_MAP_DIR.exists()


def test_active_translation_maps_are_canonical() -> None:
    failures: list[str] = []

    for map_file in _active_translation_maps():
        text = map_file.read_text(encoding="utf-8")
        for term in OLD_SHAPE_TERMS:
            if term in text:
                failures.append(f"{map_file.name}: active map contains old-shape term {term!r}")

        data = yaml.safe_load(text) or {}
        if not isinstance(data, dict):
            failures.append(f"{map_file.name}: expected top-level YAML mapping")
            continue

        for key, value in data.items():
            if key in {"skip_records", "material_overrides"} or str(key).startswith("_"):
                continue
            valid_signature = (
                isinstance(key, str)
                and len(key) == 4
                and key.upper() == key
                and any(char.isalpha() or char == "_" for char in key)
            )
            if not valid_signature:
                failures.append(f"{map_file.name}: invalid record key {key!r}")
                continue
            if not isinstance(value, dict):
                failures.append(f"{map_file.name}: {key} block must be a mapping")

    assert not failures, "Translation map canonical validation failed:\n" + "\n".join(failures)


def test_required_translation_maps_exist() -> None:
    missing = sorted(
        filename
        for filename in REQUIRED_MAP_FILENAMES
        if not (MAP_DIR / filename).is_file()
    )
    assert not missing, "Required translation maps missing from active loader path: " + ", ".join(missing)


def test_fo76_fact_preserves_legacy_and_relayouts_new_vendor_values() -> None:
    translation_map = yaml.safe_load(
        (MAP_DIR / "fo76_to_fo4.yaml").read_text(encoding="utf-8")
    )
    fact_rules = translation_map["FACT"]

    assert "VENV" not in fact_rules.get("drop", [])
    assert fact_rules["fields"]["VENP"] == "VENV"
    assert fact_rules["transforms"]["VENV"] == {"type": "venp_to_venv"}
