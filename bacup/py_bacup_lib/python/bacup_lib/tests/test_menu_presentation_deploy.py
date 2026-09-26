import json

import pytest

from bacup_lib import regen_pipeline
from bacup_lib.menu_presentation_deploy import CATALOG, startup_assets, write_always_loose


@pytest.mark.parametrize("mode", ["packed", "selective", "loose"])
@pytest.mark.parametrize("legacy_catalog", [False, True])
def test_startup_hosts_and_music_are_available_without_plugin_archives(tmp_path, mode, legacy_catalog):
    mod = tmp_path / "mods/SeventySix"
    target = tmp_path / "MO2/SeventySix"
    catalog = mod / CATALOG
    catalog.parent.mkdir(parents=True)
    (mod / "SeventySix.esm").write_bytes(b"plugin")
    menus = {"B21_TFAMainMenu_" + family: "Interface/B21_TFAMainMenu_" + family + ".swf"
             for family in ("OG", "AE")}
    music = "Music/B21/TalesFromAppalachia/Menu/fo76_main.xwm"
    loose_music = "F4SE/Plugins/B21_TalesFromAppalachia/menu-media/Music/fo76_main.xwm"
    catalog.write_text(json.dumps({"menus": menus, "media_pairs": [
        {"music": loose_music if legacy_catalog else music, "source_music": "Data/" + music}]}))
    expected = {*menus.values(), music}
    for relative in (*expected, "Interface/B21_TFALoadingMenu.swf", "Music/Special/unrelated.xwm"):
        path = mod / "data" / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"current " + relative.encode())
    wanted = {relative: (mod / "data" / relative).read_bytes() for relative in expected}
    if legacy_catalog:
        old_music = mod / loose_music
        old_music.parent.mkdir(parents=True)
        old_music.write_bytes((mod / "data" / music).read_bytes())
        (mod / "data" / music).unlink()
    old_host = target / menus["B21_TFAMainMenu_AE"]
    old_host.parent.mkdir(parents=True)
    old_host.write_bytes(b"stale")

    assert write_always_loose(mod) == len(expected)
    regen_pipeline._deploy_output_mods(
        "SeventySix", plugin_names=["SeventySix.esm"], project_root=tmp_path,
        game_data_dir=target, resource_dir=tmp_path / "resource",
        plugin_only=mode == "selective", deploy_loose=mode == "loose",
    )
    for relative in expected:
        assert (target / relative).read_bytes() == wanted[relative]
    if mode != "loose":
        assert not (target / "Interface/B21_TFALoadingMenu.swf").exists()
        assert not (target / "Music/Special/unrelated.xwm").exists()


def test_startup_manifest_cannot_deploy_unrelated_paths(tmp_path):
    catalog = tmp_path / CATALOG
    catalog.parent.mkdir(parents=True)
    catalog.write_text(json.dumps({"menus": {"B21_TFAMainMenu_AE": "../other.swf"},
                                  "media_pairs": [{"music": "Music/Special/../../other.xwm"}]}))
    assert startup_assets(tmp_path) == {}


def test_missing_startup_asset_does_not_silently_skip_deployment(tmp_path):
    catalog = tmp_path / CATALOG
    catalog.parent.mkdir(parents=True)
    catalog.write_text(json.dumps({"media_pairs": [{"music": "Music/B21/TalesFromAppalachia/Menu/missing.xwm"}]}))
    with pytest.raises(FileNotFoundError, match="Converted startup asset unavailable"):
        startup_assets(tmp_path)
