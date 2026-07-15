from __future__ import annotations

from pathlib import Path

import pytest

from creation_lib.esp.plugin import Plugin


OUTPUT_ROOT = Path("mods/B21_FNV2FO4")
DLC_PLUGINS = (
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
)


def _converted_plugin_path(plugin_name: str) -> Path:
    return OUTPUT_ROOT / Path(plugin_name).stem / plugin_name


def _require_converted_plugin(plugin_name: str) -> Path:
    plugin_path = _converted_plugin_path(plugin_name)
    if not plugin_path.is_file():
        pytest.skip(f"missing expected converted plugin output: {plugin_path}")
    return plugin_path


def test_converted_plugin_paths_use_per_plugin_folders() -> None:
    assert _converted_plugin_path("DeadMoney.esm") == OUTPUT_ROOT / "DeadMoney" / "DeadMoney.esm"


@pytest.mark.parametrize("plugin_name", DLC_PLUGINS)
def test_dlc_outputs_include_falloutnv_and_fallout4_masters(plugin_name: str) -> None:
    plugin_path = _require_converted_plugin(plugin_name)
    plugin = Plugin.load(plugin_path, game="fo4")
    try:
        masters = set(plugin.header.masters)
    finally:
        plugin.close()

    assert "FalloutNV.esm" in masters, f"{plugin_name} missing FalloutNV.esm master"
    assert "Fallout4.esm" in masters, f"{plugin_name} missing Fallout4.esm master"
