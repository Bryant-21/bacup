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
    "skeleton_skyrimse_to_fo4.yaml",
    "skyrimse_to_fo4.yaml",
    "starfield_to_fo4.yaml",
}


def _active_translation_maps() -> list[Path]:
    return sorted(
        path
        for path in MAP_DIR.glob("*_to_*.yaml")
        if not path.name.startswith(("ammo_", "events_", "skeleton_"))
    )


def test_active_translation_maps_are_canonical() -> None:
    failures: list[str] = [
        f"{filename}: required map missing from active loader path"
        for filename in sorted(REQUIRED_MAP_FILENAMES)
        if not (MAP_DIR / filename).is_file()
    ]

    for map_file in _active_translation_maps():
        text = map_file.read_text(encoding="utf-8")
        uncommented = "\n".join(line.split("#", 1)[0] for line in text.splitlines())
        for term in OLD_SHAPE_TERMS:
            if term in uncommented:
                failures.append(f"{map_file.name}: active map contains old-shape term {term!r}")

        data = yaml.safe_load(text) or {}
        if not isinstance(data, dict):
            failures.append(f"{map_file.name}: expected top-level YAML mapping")
            continue

        for key, value in data.items():
            if key in {"skip_records", "material_overrides", "record_routes"} or str(key).startswith("_"):
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
