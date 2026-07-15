from ui.toolkit.settings import ToolkitSettings


def test_set_game_root_dir_persists(tmp_path):
    s = ToolkitSettings(shared_path=tmp_path / "shared.json", variant_path=tmp_path / "v.json")
    s.set_game_root_dir("fo76", "C:/Games/Fallout76")
    assert s.get_game_paths("fo76")["root_dir"] == "C:/Games/Fallout76"
    reloaded = ToolkitSettings(
        shared_path=tmp_path / "shared.json", variant_path=tmp_path / "v.json"
    )
    assert reloaded.get_game_paths("fo76")["root_dir"] == "C:/Games/Fallout76"
