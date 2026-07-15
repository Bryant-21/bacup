"""Toolkit executable variants for full and standalone creator tools."""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class AppVariant:
    id: str
    exe_name: str
    window_title: str
    workspace_ids: tuple[str, ...] | None
    default_workspace: str
    icon_path: str
    include_ai_panel: bool = False
    include_index_settings: bool = True
    extraction_only_settings: bool = False
    ini_name: str | None = None
    minimum_window_size: tuple[int, int] | None = None
    start_centered: bool = False
    auto_hide_single_window_tabs: bool = False

    @property
    def is_standalone(self) -> bool:
        return self.workspace_ids is not None


FIRST_STANDALONE_VARIANT_IDS = (
    "nif",
    "bsa_viewer",
    "cloth_maker",
    "weight_painter",
    "papyrus",
    "materials",
    "esp_editor",
    "world_viewer",
)

VARIANTS: dict[str, AppVariant] = {
    "full": AppVariant(
        id="full",
        exe_name="ModBox21",
        window_title="ModBox21 - Bethesda Modding Toolkit",
        workspace_ids=None,
        default_workspace="nif",
        icon_path="resource/icon.ico",
        include_ai_panel=True,
    ),
    "nif": AppVariant(
        id="nif",
        exe_name="ModBox21-NIF",
        window_title="ModBox21 - NIF Editor",
        workspace_ids=("nif",),
        default_workspace="nif",
        icon_path="resource/icons/modbox21-nif.ico",
        include_index_settings=False,
    ),
    "bsa_viewer": AppVariant(
        id="bsa_viewer",
        exe_name="ModBox21-BSAViewer",
        window_title="ModBox21 - BSA Viewer",
        workspace_ids=("bsa_viewer",),
        default_workspace="bsa_viewer",
        icon_path="resource/icons/modbox21-bsa-viewer.ico",
    ),
    "cloth_maker": AppVariant(
        id="cloth_maker",
        exe_name="ModBox21-Cloth",
        window_title="ModBox21 - Cloth",
        workspace_ids=("cloth_maker",),
        default_workspace="cloth_maker",
        icon_path="resource/icons/modbox21-cloth.ico",
    ),
    "weight_painter": AppVariant(
        id="weight_painter",
        exe_name="ModBox21-Weights",
        window_title="ModBox21 - Weights",
        workspace_ids=("weight_painter",),
        default_workspace="weight_painter",
        icon_path="resource/icons/modbox21-weights.ico",
    ),
    "papyrus": AppVariant(
        id="papyrus",
        exe_name="ModBox21-Papyrus",
        window_title="ModBox21 - Papyrus",
        workspace_ids=("papyrus",),
        default_workspace="papyrus",
        icon_path="resource/icons/modbox21-papyrus.ico",
    ),
    "materials": AppVariant(
        id="materials",
        exe_name="ModBox21-Materials",
        window_title="ModBox21 - Materials",
        workspace_ids=("materials",),
        default_workspace="materials",
        icon_path="resource/icons/modbox21-materials.ico",
    ),
    "esp_editor": AppVariant(
        id="esp_editor",
        exe_name="ModBox21-ESPEditor",
        window_title="ModBox21 - ESP Editor",
        workspace_ids=("esp_editor",),
        default_workspace="esp_editor",
        icon_path="resource/icons/modbox21-esp-editor.ico",
    ),
    "world_viewer": AppVariant(
        id="world_viewer",
        exe_name="ModBox21-WorldViewer",
        window_title="ModBox21 - World Viewer",
        workspace_ids=("world_viewer",),
        default_workspace="world_viewer",
        icon_path="resource/icon.ico",
    ),
}


def get_variant(variant_id: str) -> AppVariant:
    try:
        return VARIANTS[variant_id]
    except KeyError as exc:
        known = ", ".join(sorted(VARIANTS))
        raise ValueError(f"Unknown toolkit variant {variant_id!r}; expected one of: {known}") from exc


def get_release_asset_names(version: str) -> dict[str, str]:
    return {
        variant_id: f"{variant.exe_name}-{version}.zip"
        for variant_id, variant in VARIANTS.items()
    }


def variant_id_from_exe_name(exe_name: str) -> str | None:
    normalized = exe_name.lower()
    for variant_id, variant in VARIANTS.items():
        if variant.exe_name.lower() == normalized:
            return variant_id
    return None
