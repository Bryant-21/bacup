from pathlib import Path

from bacup_lib import regen_pipeline
from bacup_lib.legacy_tales_catalogs import LEGACY_TALES_CATALOGS, remove_legacy_tales_catalogs

TALES = "F4SE/Plugins/B21_TalesFromAppalachia"
OUT_OF_SCOPE = (
    "item_icons.json", "respawn.json", "photo-mode.json", "menu-presentation.json",
    "EquipmentBonusIcons.json", "power_armor_icons.json", "perk_vaultboys.json", "photo-gallery.json",
)
LIVE = (
    "Diseases/SeventySix.esm.json", "LeveledLists/SeventySix.esm.json",
    "NpcStats/SeventySix.esm.json", "ObjectiveAreas/B21_TalesFromAppalachia.esm.json",
    "ObjectiveAreas/SeventySix.esm.json", "CampWallpapers/SeventySix.esm.json",
)


def _touch(root: Path, relative: str) -> Path:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("{}")
    return path


def test_removes_only_legacy_files_and_keeps_dirs(tmp_path):
    legacy = [_touch(tmp_path, relative) for relative in LEGACY_TALES_CATALOGS]
    keep = [_touch(tmp_path, f"{TALES}/{name}") for name in (*OUT_OF_SCOPE, *LIVE)]

    removed = remove_legacy_tales_catalogs(tmp_path)

    assert sorted(removed) == sorted(legacy)
    assert not any(path.exists() for path in legacy)
    assert all(path.is_file() for path in keep)
    assert (tmp_path / TALES / "LeveledLists").is_dir()


def test_missing_root_and_directories_named_like_legacy_files_are_safe(tmp_path):
    assert remove_legacy_tales_catalogs(tmp_path / "absent") == []
    (tmp_path / TALES / "pets.json").mkdir(parents=True)

    assert remove_legacy_tales_catalogs(tmp_path) == []
    assert (tmp_path / TALES / "pets.json").is_dir()


def test_deploy_output_mods_cleans_output_and_deploy_target(monkeypatch, tmp_path):
    output_mod = tmp_path / "mods" / "SeventySix"
    stale_output = _touch(output_mod, f"{TALES}/Diseases.json")
    live_output = _touch(output_mod, f"{TALES}/Diseases/SeventySix.esm.json")
    game_data = tmp_path / "MO2" / "mods" / "fo76"
    stale_target = _touch(game_data, f"{TALES}/PerkCards.json")
    live_target = _touch(game_data, f"{TALES}/item_icons.json")
    seen = {}

    def fake_deploy_mod(mod_name, **kwargs):
        seen["output_clean_at_deploy"] = not stale_output.exists()

    monkeypatch.setattr("creation_lib.build.deployer.deploy_mod", fake_deploy_mod)

    regen_pipeline._deploy_output_mods(
        "SeventySix",
        plugin_names=["SeventySix.esm"],
        project_root=tmp_path,
        game_data_dir=game_data,
        resource_dir=tmp_path / "resource",
    )

    assert seen["output_clean_at_deploy"]
    assert live_output.is_file()
    assert not stale_target.exists()
    assert live_target.is_file()


def test_plugin_only_deploy_also_cleans_target(monkeypatch, tmp_path):
    (tmp_path / "mods" / "SeventySix").mkdir(parents=True)
    game_data = tmp_path / "Fallout4" / "Data"
    stale_target = _touch(game_data, f"{TALES}/emotes.json")
    monkeypatch.setattr("creation_lib.build.deployer.deploy_mod", lambda *a, **k: None)
    monkeypatch.setattr("creation_lib.build.deployer.deploy_xse_files", lambda *a, **k: (0, []))

    regen_pipeline._deploy_output_mods(
        "SeventySix",
        plugin_names=["SeventySix.esm"],
        project_root=tmp_path,
        game_data_dir=game_data,
        resource_dir=tmp_path / "resource",
        plugin_only=True,
    )

    assert not stale_target.exists()
