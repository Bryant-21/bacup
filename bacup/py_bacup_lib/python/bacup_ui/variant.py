"""B.A.C.U.P. application metadata."""

from __future__ import annotations

from ui.toolkit.variants import AppVariant

BACUP_VARIANT = AppVariant(
    id="appalachia",
    exe_name="BACUP",
    window_title="B.A.C.U.P. Bethesda Asset Converter Universal Platform",
    workspace_ids=("appalachia",),
    default_workspace="appalachia",
    icon_path="resource/icons/modbox21-converter.ico",
    include_index_settings=True,
    extraction_only_settings=True,
    ini_name="bacup",
    minimum_window_size=(1280, 760),
    start_centered=True,
    auto_hide_single_window_tabs=True,
)
