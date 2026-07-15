from __future__ import annotations

from bacup_lib import native_maps
from bacup_lib.native_maps import native_translation_maps_dir


def test_native_translation_maps_dir_resolves_embedded_native_maps() -> None:
    maps_dir = native_translation_maps_dir()

    assert maps_dir.name == "translation_maps"
    assert "native" in maps_dir.parts
    assert (maps_dir / "fo76_to_fo4.yaml").is_file()
    assert (maps_dir / "events_fo76_to_fo4.yaml").is_file()
    assert (maps_dir / "skeleton_fnv_to_fo4_creatures.yaml").is_file()


def test_native_translation_maps_dir_supports_packaged_resource_override(tmp_path, monkeypatch) -> None:
    maps_dir = tmp_path / "creation_lib" / "resources" / "conversion" / "translation_maps"
    maps_dir.mkdir(parents=True)
    (maps_dir / "fo76_to_fo4.yaml").write_text("material_overrides: {}\n", encoding="utf-8")

    monkeypatch.setenv("CREATION_LIB_TRANSLATION_MAPS_DIR", str(maps_dir))
    native_translation_maps_dir.cache_clear()
    try:
        assert native_translation_maps_dir() == maps_dir
    finally:
        native_translation_maps_dir.cache_clear()


def test_candidate_translation_map_dirs_include_pyinstaller_resource_layout(tmp_path) -> None:
    bundle_root = tmp_path / "_MEI123"
    module_file = bundle_root / "bacup_lib" / "native_maps.py"
    expected = bundle_root / "bacup_lib" / "resources" / "conversion" / "translation_maps"

    candidates = native_maps._candidate_translation_map_dirs(
        module_file=module_file,
        meipass=bundle_root,
    )

    assert candidates[0] == expected
