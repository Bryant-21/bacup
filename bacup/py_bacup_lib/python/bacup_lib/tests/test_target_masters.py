from __future__ import annotations


def test_resolve_official_target_master_paths_includes_fo4_dlc_in_order(tmp_path):
    from bacup_lib.target_masters import (
        resolve_official_target_master_paths,
    )

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    names = [
        "Fallout4.esm",
        "DLCRobot.esm",
        "DLCworkshop01.esm",
        "DLCCoast.esm",
        "DLCworkshop02.esm",
        "DLCworkshop03.esm",
        "DLCNukaWorld.esm",
    ]
    for name in names:
        (data_dir / name).write_bytes(b"")

    paths, missing = resolve_official_target_master_paths(
        "fo4",
        target_data_dir=data_dir,
    )

    assert [path.name for path in paths] == names
    assert missing == []


def test_resolve_official_target_master_paths_skips_missing_dlc(tmp_path):
    from bacup_lib.target_masters import (
        resolve_official_target_master_paths,
    )

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    (data_dir / "Fallout4.esm").write_bytes(b"")
    (data_dir / "DLCRobot.esm").write_bytes(b"")

    paths, missing = resolve_official_target_master_paths(
        "fo4",
        target_data_dir=data_dir,
    )

    assert [path.name for path in paths] == ["Fallout4.esm", "DLCRobot.esm"]
    assert "DLCNukaWorld.esm" in missing


def test_resolve_target_master_paths_accepts_game_root_with_data_dir(tmp_path):
    from bacup_lib.target_masters import resolve_target_master_paths

    game_root = tmp_path / "Fallout 4"
    data_dir = game_root / "Data"
    data_dir.mkdir(parents=True)
    base_master = data_dir / "Fallout4.esm"
    base_master.write_bytes(b"")

    assert resolve_target_master_paths("fo4", target_data_dir=game_root) == [base_master]


def test_resolve_target_master_paths_keeps_explicit_files_first(tmp_path):
    from bacup_lib.target_masters import resolve_target_master_paths

    explicit_master = tmp_path / "Explicit.esm"
    explicit_master.write_bytes(b"")
    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    base_master = data_dir / "Fallout4.esm"
    base_master.write_bytes(b"")

    assert resolve_target_master_paths(
        "fo4",
        target_master_paths=[explicit_master, tmp_path / "Missing.esm"],
        target_data_dir=data_dir,
    ) == [explicit_master, base_master]


def test_resolve_target_master_plugin_paths_official_then_explicit(tmp_path):
    from bacup_lib.target_masters import resolve_target_master_plugin_paths

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    for name in ["Fallout4.esm", "DLCRobot.esm"]:
        (data_dir / name).write_bytes(b"")
    explicit_master = tmp_path / "Explicit.esm"
    explicit_master.write_bytes(b"")

    paths, missing = resolve_target_master_plugin_paths(
        "fo4",
        target_master_paths=[explicit_master],
        target_data_dir=data_dir,
    )

    assert [path.name for path in paths] == [
        "Fallout4.esm",
        "DLCRobot.esm",
        "Explicit.esm",
    ]
    assert "Fallout4.esm" not in missing
    assert "DLCRobot.esm" not in missing


def test_open_target_master_handles_uses_official_order_then_explicit_extras(
    monkeypatch,
    tmp_path,
):
    from bacup_lib.target_masters import (
        close_plugin_handles,
        open_target_master_handles,
    )
    from creation_lib.esp.plugin import Plugin

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    names = ["Fallout4.esm", "DLCRobot.esm"]
    for name in names:
        (data_dir / name).write_bytes(b"")
    explicit_master = tmp_path / "Explicit.esm"
    explicit_master.write_bytes(b"")
    loaded: list[str] = []
    closed: list[str] = []

    class FakePlugin:
        def __init__(self, name: str):
            self.name = name

        def close(self):
            closed.append(self.name)

    def fake_load(path, *, game, lazy_index=False):
        loaded.append(path.name)
        return FakePlugin(path.name)

    monkeypatch.setattr(Plugin, "load", fake_load)

    handles = open_target_master_handles(
        "fo4",
        target_master_paths=[explicit_master],
        target_data_dir=data_dir,
    )

    assert loaded == ["Fallout4.esm", "DLCRobot.esm", "Explicit.esm"]
    close_plugin_handles(handles)
    assert closed == loaded
