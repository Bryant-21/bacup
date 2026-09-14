from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
from typing import Iterable

from bacup_lib.asset_paths import normalize_asset_source_path
from bacup_lib.models import AssetRef


_FORM_KEY_ID_PLUGIN_RE = re.compile(
    r"^\s*(?:0x)?(?P<form_id>[0-9a-fA-F]{1,8})\s*[@:]\s*(?P<plugin>[^@:]+?)\s*$"
)
_FORM_KEY_PLUGIN_ID_RE = re.compile(
    r"^\s*(?P<plugin>[^@:]+?)\s*[@:]\s*(?:0x)?(?P<form_id>[0-9a-fA-F]{1,8})\s*$"
)


def _asset_key(asset: AssetRef) -> tuple[str, str]:
    asset_type = str(getattr(asset, "asset_type", "") or "").casefold()
    source_path = normalize_asset_source_path(
        str(getattr(asset, "source_path", "") or "")
    ).casefold()
    expected_root = {"nif": "meshes", "texture": "textures"}.get(asset_type)
    if expected_root and source_path.split("/", 1)[0] != expected_root:
        source_path = f"{expected_root}/{source_path}"
    return asset_type, source_path


@dataclass(frozen=True)
class CreatureMvpAssetActivator:
    source_plugin: str
    source_form_id: int
    source_record_signature: str
    asset: tuple[str, str]

    def matches(self, candidate: AssetRef) -> bool:
        provenance = getattr(candidate, "provenance", None)
        signature = str(getattr(provenance, "added_by_record_sig", "") or "").upper()
        if signature != self.source_record_signature:
            return False
        form_key = str(getattr(provenance, "added_by_record_fk", "") or "")
        match = _FORM_KEY_ID_PLUGIN_RE.fullmatch(
            form_key
        ) or _FORM_KEY_PLUGIN_ID_RE.fullmatch(form_key)
        if match is None:
            return False
        return (
            int(match.group("form_id"), 16) == self.source_form_id
            and Path(match.group("plugin")).name.casefold()
            == self.source_plugin.casefold()
            and _asset_key(candidate) == self.asset
        )


@dataclass(frozen=True)
class CreatureMvpAssetClosure:
    profile: str
    label: str
    activator: CreatureMvpAssetActivator
    assets: frozenset[tuple[str, str]]
    output_subpaths: tuple[tuple[tuple[str, str], str], ...]

    def allows(self, asset: AssetRef) -> bool:
        return _asset_key(asset) in self.assets

    def activating_assets(self, assets: Iterable[AssetRef]) -> tuple[AssetRef, ...]:
        candidate = next(
            (asset for asset in assets if self.activator.matches(asset)),
            None,
        )
        return (candidate,) if candidate is not None else ()

    def is_activated_by(self, assets: Iterable[AssetRef]) -> bool:
        return bool(self.activating_assets(assets))

    def missing_entries(
        self, assets: Iterable[AssetRef]
    ) -> tuple[tuple[str, str], ...]:
        existing = {_asset_key(asset) for asset in assets}
        return tuple(sorted(self.assets - existing))

    def output_subpath_for(self, asset: AssetRef) -> str | None:
        return dict(self.output_subpaths).get(_asset_key(asset))


SKYRIM_WOLF_MVP_ASSET_CLOSURE = CreatureMvpAssetClosure(
    profile="skyrim_wolf",
    label="Skyrim Wolf",
    activator=CreatureMvpAssetActivator(
        source_plugin="Skyrim.esm",
        source_form_id=0x04E885,
        source_record_signature="ARMA",
        asset=(
            "nif",
            "meshes/actors/canine/character assets wolf/wolf.nif",
        ),
    ),
    assets=frozenset(
        {
            ("nif", "meshes/actors/canine/character assets wolf/wolf.nif"),
            ("nif", "meshes/actors/canine/character assets wolf/skeleton.nif"),
            ("behavior", "actors/canine/character assets wolf/skeleton.hkx"),
            ("behavior", "actors/canine/animations/mt_idle_wolf.hkx"),
            ("behavior", "actors/canine/animations/walkforward_wolf.hkx"),
            ("behavior", "actors/canine/animations/turncannedl90.hkx"),
            ("behavior", "actors/canine/animations/turncannedr90.hkx"),
            ("behavior", "actors/canine/animations/attack1.hkx"),
            ("texture", "textures/actors/wolf/wolf.dds"),
            ("texture", "textures/actors/wolf/wolf_n.dds"),
            ("texture", "textures/actors/wolf/wolf_sk.dds"),
        }
    ),
    output_subpaths=(
        (
            ("nif", "meshes/actors/canine/character assets wolf/wolf.nif"),
            "Meshes/Actors/B21_SkyrimWolf/CharacterAssets/B21_SkyrimWolf.nif",
        ),
        (
            ("nif", "meshes/actors/canine/character assets wolf/skeleton.nif"),
            "Meshes/Actors/B21_SkyrimWolf/CharacterAssets/Skeleton.nif",
        ),
    ),
)


FNV_GECKO_MVP_ASSET_CLOSURE = CreatureMvpAssetClosure(
    profile="fnv_gecko",
    label="FNV Gecko",
    activator=CreatureMvpAssetActivator(
        source_plugin="FalloutNV.esm",
        source_form_id=0x10CD73,
        source_record_signature="CREA",
        asset=("nif", "meshes/creatures/nvgecko/skeleton.nif"),
    ),
    assets=frozenset(
        {
            ("nif", "meshes/creatures/nvgecko/nvgecko.nif"),
            ("nif", "meshes/creatures/nvgecko/skeleton.nif"),
            ("kf_animation", "creatures/nvgecko/mtidle.kf"),
            ("kf_animation", "creatures/nvgecko/swimmtforward.kf"),
            ("kf_animation", "creatures/nvgecko/mtturnleft.kf"),
            ("kf_animation", "creatures/nvgecko/mtturnright.kf"),
            ("kf_animation", "creatures/nvgecko/h2hattackforwardpower.kf"),
            ("texture", "textures/creatures/gecko/gecko_d.dds"),
            ("texture", "textures/creatures/gecko/gecko_fire_d.dds"),
            ("texture", "textures/creatures/gecko/gecko_fire_n.dds"),
            ("texture", "textures/gore/meatcapgore01.dds"),
            ("texture", "textures/gore/meatcapgore01_n.dds"),
        }
    ),
    output_subpaths=(
        (
            ("nif", "meshes/creatures/nvgecko/nvgecko.nif"),
            "Meshes/Actors/B21_FNVGecko/CharacterAssets/B21_FNVGecko.nif",
        ),
        (
            ("nif", "meshes/creatures/nvgecko/skeleton.nif"),
            "Meshes/Actors/B21_FNVGecko/CharacterAssets/Skeleton.nif",
        ),
    ),
)


_CLOSURES_BY_PROFILE = {
    SKYRIM_WOLF_MVP_ASSET_CLOSURE.profile: SKYRIM_WOLF_MVP_ASSET_CLOSURE,
    FNV_GECKO_MVP_ASSET_CLOSURE.profile: FNV_GECKO_MVP_ASSET_CLOSURE,
}


def creature_mvp_asset_closure(
    profile: str | None,
    assets: Iterable[AssetRef],
) -> CreatureMvpAssetClosure | None:
    closure = _CLOSURES_BY_PROFILE.get(str(profile or "").casefold())
    if closure is None:
        return None
    return closure if closure.is_activated_by(assets) else None
