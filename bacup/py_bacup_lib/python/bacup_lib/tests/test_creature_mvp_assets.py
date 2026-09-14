from bacup_lib.creature_mvp_assets import (
    FNV_GECKO_MVP_ASSET_CLOSURE,
    SKYRIM_WOLF_MVP_ASSET_CLOSURE,
    creature_mvp_asset_closure,
)
from bacup_lib.models import AssetProvenance, AssetRef


def _asset(
    asset_type: str,
    path: str,
    *,
    form_key: str = "",
    signature: str = "",
) -> AssetRef:
    provenance = None
    if form_key:
        provenance = AssetProvenance(
            added_by_record_fk=form_key,
            added_by_record_eid="",
            added_by_field="MODL",
            walk_depth=0,
            walker_pass="native_asset_collect",
            added_by_record_sig=signature,
        )
    return AssetRef(asset_type, path, provenance=provenance)


def test_skyrim_wolf_closure_requires_exact_armor_addon_owner() -> None:
    exact = _asset(
        "nif",
        "Meshes/Actors/Canine/Character Assets Wolf/wolf.nif",
        form_key="04E885@Skyrim.esm",
        signature="ARMA",
    )
    assert (
        creature_mvp_asset_closure("skyrim_wolf", [exact])
        is SKYRIM_WOLF_MVP_ASSET_CLOSURE
    )
    wrong_owner = _asset(
        "nif",
        exact.source_path,
        form_key="04E885@WolfPack.esp",
        signature="ARMA",
    )
    assert creature_mvp_asset_closure("skyrim_wolf", [wrong_owner]) is None


def test_fnv_gecko_closure_requires_exact_creature_owner() -> None:
    exact = _asset(
        "nif",
        "Meshes/creatures/NVGecko/skeleton.nif",
        form_key="FalloutNV.esm:10CD73",
        signature="CREA",
    )
    assert (
        creature_mvp_asset_closure("fnv_gecko", [exact])
        is FNV_GECKO_MVP_ASSET_CLOSURE
    )
    wrong_record = _asset(
        "nif",
        exact.source_path,
        form_key="FalloutNV.esm:10CD74",
        signature="CREA",
    )
    assert creature_mvp_asset_closure("fnv_gecko", [wrong_record]) is None


def test_creature_closures_remap_only_body_and_visual_skeleton() -> None:
    wolf_body = _asset(
        "nif", "Meshes/Actors/Canine/Character Assets Wolf/wolf.nif"
    )
    gecko_skeleton = _asset("nif", "creatures/NVGecko/skeleton.nif")
    assert SKYRIM_WOLF_MVP_ASSET_CLOSURE.output_subpath_for(wolf_body) == (
        "Meshes/Actors/B21_SkyrimWolf/CharacterAssets/B21_SkyrimWolf.nif"
    )
    assert FNV_GECKO_MVP_ASSET_CLOSURE.output_subpath_for(gecko_skeleton) == (
        "Meshes/Actors/B21_FNVGecko/CharacterAssets/Skeleton.nif"
    )
    assert SKYRIM_WOLF_MVP_ASSET_CLOSURE.output_subpath_for(
        _asset("behavior", "Actors/Canine/Animations/attack1.hkx")
    ) is None


def test_unknown_or_inactive_profile_is_fail_closed() -> None:
    assert creature_mvp_asset_closure(None, []) is None
    assert creature_mvp_asset_closure("unknown", []) is None
    assert creature_mvp_asset_closure("skyrim_wolf", []) is None
