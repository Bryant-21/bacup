from types import SimpleNamespace

import pytest

from bacup_ui.conversion.panels.regen_panel import RegenPanel


@pytest.mark.parametrize("pair,native,terrain,fo4_materials", [
    ("fo76:fo4", True, True, True),
    ("starfield:fo4", True, False, False),
    ("skyrimse:fo4", False, False, False),
])
def test_native_assets_use_the_existing_switch_only_for_supported_pairs(pair, native, terrain, fo4_materials):
    panel = SimpleNamespace(
        _pair=lambda: SimpleNamespace(pair_id=pair),
        _load_upgrade_manifest_cached=lambda: None,
        _effective_lod_mode=lambda: "none",
        deploy_format="standard", install_location="none", workers=0,
        lod_mode="none", re_use_land=False, native_assets=True,
        replace_fo4_materials=True,
    )
    options = RegenPanel.build_options(panel)
    assert options.native_assets is native
    assert options.emit_btd4 is terrain
    assert options.replace_fo4_materials is fo4_materials
    assert options.upgrade_fo4_materials is False
    assert options.deploy is False
    panel.native_assets = False
    options = RegenPanel.build_options(panel)
    assert not options.native_assets
    assert not options.emit_btd4
