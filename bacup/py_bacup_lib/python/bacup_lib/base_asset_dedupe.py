"""Cross-game base-asset dedupe guard helpers."""
from __future__ import annotations

from bacup_lib.models import AssetRef
from bacup_lib.native_runtime import load_native_module

def resolve_base_asset_relocation_mesh_roots(
    source_game: str,
    target_game: str,
    configured,
) -> tuple[str, ...]:
    return tuple(load_native_module().conversion_setup_relocation_roots(
        source_game, target_game, [str(value or "") for value in (configured or ())]
    ))


def resolve_base_asset_namespace(
    source_game: str,
    target_game: str,
    configured: str | None,
) -> str:
    return str(load_native_module().conversion_setup_relocation_namespace(
        source_game, target_game, str(configured or "")
    ))


def asset_owner_signature(asset: AssetRef) -> str:
    prov = getattr(asset, "provenance", None)
    return str(getattr(prov, "added_by_record_sig", "") or "").strip().upper()
