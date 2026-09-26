import json
from pathlib import Path

import pytest

from creation_lib.build.deploy_plan import plan_deploy
from creation_lib.build.deployer import deploy_mod
from creation_lib.esp import Plugin, export_json
from creation_lib.esp.authoring import deserialize
from creation_lib.esp.strings import parse_string_table
from ui.builder.mod_builder_app import ModBuilderApp


def write_menu(mod, title):
    source = mod / "yaml"
    records = source / "records/MESG"
    records.mkdir(parents=True, exist_ok=True)
    (source / "plugin.json").write_text(json.dumps({
        "format_version": 1, "plugin": "B21_Localized.esp", "game": "fo4", "header_size": 24,
        "header": {"flags": ["Localized"], "masters": [], "next_object_id": "000801"},
    }))
    (records / "B21_Menu.json").write_text(json.dumps({
        "form_id": "000800", "eid": "B21_Menu", "fields": [
            {"Name": {"TargetLanguage": "English", "Values": [{"Language": "English", "String": title},
                                                               {"Language": "German", "String": "Kopfgeld"}]}},
            {"Description": {"TargetLanguage": "English", "Values": [{"Language": "English", "String": "Choose a bounty"}]}},
            {"ButtonText": {"TargetLanguage": "English", "Values": [{"Language": "English", "String": "Start hunt"}]}},
        ],
    }))


def assert_message_tables(directory):
    for language in ("en", "de"):
        strings = parse_string_table(directory / f"B21_Localized_{language}.STRINGS")
        descriptions = parse_string_table(directory / f"B21_Localized_{language}.DLSTRINGS")
        assert "Start hunt" in strings.values()
        assert "Choose a bounty" in descriptions.values()
        if language == "de":
            assert "Kopfgeld" in strings.values()


def test_builder_build_regenerates_localized_tables(tmp_path, monkeypatch):
    mod = tmp_path / "mods/B21_Localized"
    write_menu(mod, "Old bounty title")
    app = ModBuilderApp.__new__(ModBuilderApp)
    app._selected_mod = lambda: mod.name
    app._warn_creation_only_masters = lambda: None
    app._get_mod_game = lambda: "fo4"
    app._deploy_patches = False
    app._resolve_game_data_path = lambda game: tmp_path / "Game/Data"
    app._run_fn = lambda fn, **kwargs: fn(lambda message: None)
    monkeypatch.setattr("ui.builder.mod_builder_app.MODS_DIR", str(mod.parent))
    app._on_build()
    tables = mod / "Strings/B21_Localized_en.STRINGS"
    assert b"Old bounty title" in tables.read_bytes()
    write_menu(mod, "New bounty title")
    app._on_build()
    assert b"New bounty title" in tables.read_bytes()
    assert b"Old bounty title" not in tables.read_bytes()
    assert_message_tables(tables.parent)


@pytest.mark.parametrize("esp_only", [False, True])
def test_deploy_regenerates_and_ships_tables_without_repacking(tmp_path, esp_only):
    mod = tmp_path / "mods/B21_Localized"
    target = tmp_path / "MO2/B21_Localized"
    write_menu(mod, "First bounty")
    kwargs = dict(game="fo4", game_data_dir=tmp_path / "Game/Data", deploy_data_dir=target,
                  project_root=tmp_path, skip_pack=True, skip_papyrus_compile=True, esp_only=esp_only)
    deploy_mod(mod.name, **kwargs)
    write_menu(mod, "Updated bounty")
    result = deploy_mod(mod.name, **kwargs)
    tables = target / "Strings/B21_Localized_en.STRINGS"
    assert b"Updated bounty" in tables.read_bytes()
    assert b"First bounty" not in tables.read_bytes()
    assert tables.read_bytes() == (mod / "Strings" / tables.name).read_bytes()
    assert_message_tables(tables.parent)
    assert result.strings_deployed > 0
    plan = plan_deploy(mod, target, game="fo4", skip_build=True, skip_pack=True,
                       skip_papyrus_compile=True, esp_only=esp_only)
    assert any(op["action"] == "copy" and Path(op["destination"]) == tables for op in plan["operations"])
    assert not any(op["action"] == "remove" and Path(op["destination"]) == tables for op in plan["operations"])
    assert not (tmp_path / "Game").exists()


def test_whole_plugin_build_keeps_string_table_stem(tmp_path):
    mod = tmp_path / "source"
    write_menu(mod, "Whole plugin bounty")
    plugin_path = mod / "B21_Localized.esp"
    deserialize(mod / "yaml", plugin_path, game="fo4")
    with Plugin.load(plugin_path, game="fo4") as plugin:
        whole = tmp_path / "whole.json"
        whole.write_text(export_json(plugin, mode="authoring"), encoding="utf-8")
    output = tmp_path / "output/B21_Localized.esp"
    deserialize(whole, output, game="fo4")
    tables = output.parent / "Strings/B21_Localized_en.STRINGS"
    assert b"Whole plugin bounty" in tables.read_bytes()
    assert_message_tables(tables.parent)
    assert all(p.name.startswith("B21_Localized_") for p in tables.parent.iterdir())


def test_deploy_leaves_tables_to_the_packed_archive(tmp_path, monkeypatch):
    mod = tmp_path / "mods/B21_Localized"
    target = tmp_path / "MO2/B21_Localized"
    write_menu(mod, "Packed bounty")
    (mod / "data/Meshes").mkdir(parents=True)
    stale = target / "Strings/B21_Localized_en.STRINGS"
    stale.parent.mkdir(parents=True)
    stale.write_bytes(b"stale")
    monkeypatch.setattr("creation_lib.build.deployer.pack_mod", lambda *args, **kwargs: None)
    result = deploy_mod(mod.name, game="fo4", game_data_dir=tmp_path / "Game/Data",
                        deploy_data_dir=target, project_root=tmp_path, skip_papyrus_compile=True)
    assert b"Packed bounty" in (mod / "Strings/B21_Localized_en.STRINGS").read_bytes()
    assert result.strings_deployed == 0
    assert list((target / "Strings").iterdir()) == []
    plan = plan_deploy(mod, target, game="fo4", skip_build=True, skip_papyrus_compile=True)
    assert not any(op["action"] == "copy" and Path(op["destination"]).parent == target / "Strings"
                   for op in plan["operations"])
