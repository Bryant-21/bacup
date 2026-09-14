from __future__ import annotations

from typing import Iterable

from bacup_lib.models import AssetRef
from bacup_lib.weapon_mvp_assets import (
    FNV_HATCHET_MVP_ASSET_CLOSURE as _FNV_HATCHET_MVP_ASSET_CLOSURE,
    WeaponMvpAssetClosure,
    weapon_mvp_asset_closure,
)


FnvWeaponAssetClosure = WeaponMvpAssetClosure
FNV_HATCHET_MVP_ASSET_CLOSURE = _FNV_HATCHET_MVP_ASSET_CLOSURE


def fnv_weapon_mvp_asset_closure(
    source_game: str,
    target_game: str,
    assets: Iterable[AssetRef],
) -> WeaponMvpAssetClosure | None:
    if (source_game.casefold(), target_game.casefold()) != ("fnv", "fo4"):
        return None
    return weapon_mvp_asset_closure(source_game, target_game, assets)
