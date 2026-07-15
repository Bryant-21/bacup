"""FO76 -> FO4 base-asset dedupe guard helpers."""
from __future__ import annotations

from bacup_lib.models import AssetRef

DEFAULT_FO76_FO4_RELOCATION_MESH_ROOTS = ("meshes/landscape",)
DEFAULT_FO76_FO4_NAMESPACE = "FO76"


def _normalize_mesh_root(value) -> str:
    return str(value or "").replace("\\", "/").strip().strip("/").lower()


def resolve_base_asset_relocation_mesh_roots(
    source_game: str,
    target_game: str,
    configured,
) -> tuple[str, ...]:
    roots = tuple(r for r in (_normalize_mesh_root(v) for v in (configured or ())) if r)
    if roots:
        return roots
    if source_game.lower() == "fo76" and target_game.lower() == "fo4":
        return DEFAULT_FO76_FO4_RELOCATION_MESH_ROOTS
    return ()


def resolve_base_asset_namespace(
    source_game: str,
    target_game: str,
    configured: str | None,
) -> str:
    namespace = str(configured or "").strip().strip("/\\")
    if namespace:
        return namespace
    if source_game.lower() == "fo76" and target_game.lower() == "fo4":
        return DEFAULT_FO76_FO4_NAMESPACE
    return ""


def asset_owner_signature(asset: AssetRef) -> str:
    prov = getattr(asset, "provenance", None)
    return str(getattr(prov, "added_by_record_sig", "") or "").strip().upper()
