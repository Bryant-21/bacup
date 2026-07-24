from __future__ import annotations

import pytest


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


def test_resolve_required_target_master_path_finds_xdi_or_fails(tmp_path):
    from bacup_lib.target_masters import resolve_required_target_master_path

    data_dir = tmp_path / "Fallout 4" / "Data"
    data_dir.mkdir(parents=True)
    with pytest.raises(FileNotFoundError, match="required target master XDI.esm"):
        resolve_required_target_master_path("XDI.esm", target_data_dir=data_dir)

    xdi_master = data_dir / "XDI.esm"
    xdi_master.write_bytes(b"TES4")

    assert (
        resolve_required_target_master_path(
            "XDI.esm", target_data_dir=tmp_path / "Fallout 4"
        )
        == xdi_master
    )


def test_resolve_required_target_master_path_finds_xdi_in_renamed_mo2_mod(
    tmp_path,
):
    from bacup_lib.target_masters import resolve_required_target_master_path

    mods_dir = tmp_path / "ModOrganizer" / "mods"
    xdi_master = mods_dir / "My Renamed Dialogue Mod" / "XDI.esm"
    xdi_master.parent.mkdir(parents=True)
    xdi_master.write_bytes(b"TES4")

    assert (
        resolve_required_target_master_path(
            "XDI.esm",
            target_master_paths=[mods_dir],
        )
        == xdi_master
    )


def test_fo76_record_conversion_adds_xdi_to_master_inputs(tmp_path):
    from bacup_lib.models import PluginPortOptions, PluginPortRequest
    from bacup_lib.workflows.unified import _conversion_target_master_inputs

    data_dir = tmp_path / "Data"
    data_dir.mkdir()
    xdi_master = data_dir / "XDI.esm"
    xdi_master.write_bytes(b"TES4")
    request = PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[],
        output_root=tmp_path / "output",
        target_data_dir=data_dir,
        options=PluginPortOptions(translate_records=True),
    )

    assert _conversion_target_master_inputs(request) == [xdi_master]


def test_fo76_record_conversion_adds_xdi_from_mo2_sibling_mod(tmp_path):
    from bacup_lib.models import PluginPortOptions, PluginPortRequest
    from bacup_lib.workflows.unified import _conversion_target_master_inputs

    mods_dir = tmp_path / "ModOrganizer" / "mods"
    install_dir = mods_dir / "fo76"
    install_dir.mkdir(parents=True)
    xdi_master = mods_dir / "Any Folder Name" / "XDI.esm"
    xdi_master.parent.mkdir()
    xdi_master.write_bytes(b"TES4")
    request = PluginPortRequest(
        source_game="fo76",
        target_game="fo4",
        source_plugins=[],
        output_root=tmp_path / "output",
        target_data_dir=tmp_path / "Fallout 4" / "Data",
        target_master_paths=[install_dir.parent],
        options=PluginPortOptions(translate_records=True),
    )

    assert _conversion_target_master_inputs(request) == [
        install_dir.parent,
        xdi_master,
    ]


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
