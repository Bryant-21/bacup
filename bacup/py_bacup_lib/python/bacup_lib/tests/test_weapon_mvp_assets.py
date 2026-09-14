from pathlib import Path

from bacup_lib.models import AssetProvenance, AssetRef
from bacup_lib.source_pairs import SKYRIM_MVP_EXCLUDE_SIGNATURES
from bacup_lib.weapon_mvp_assets import (
    SKYRIM_STEEL_BATTLEAXE_MVP_ASSET_CLOSURE,
    weapon_mvp_asset_closure,
)
from bacup_lib.workflows.unified import _UnifiedRecordRuntime, _is_world_only_mvp_asset


def _asset(
    asset_type: str,
    path: str,
    *,
    form_key: str = "",
    signature: str = "",
) -> AssetRef:
    provenance = None
    if form_key or signature:
        provenance = AssetProvenance(
            added_by_record_fk=form_key,
            added_by_record_eid="",
            added_by_field="MODL",
            walk_depth=0,
            walker_pass="native_asset_collect",
            added_by_record_sig=signature,
        )
    return AssetRef(asset_type, path, provenance=provenance)


def _skyrim_activators() -> list[AssetRef]:
    return [
        _asset(
            "nif",
            r"Weapons\Steel\SteelBattleAxe.nif",
            form_key="Skyrim.esm:013984",
            signature="WEAP",
        ),
        _asset(
            "nif",
            "Meshes/Weapons/Steel/1stPersonSteelBattleAxe.NIF",
            form_key="020E27@Skyrim.esm",
            signature="STAT",
        ),
    ]


_SKYRIM_TEXTURES = {
    "textures/blood/bloodedge01.dds",
    "textures/blood/bloodedge01add.dds",
    "textures/blood/bloodedge01_n.dds",
    "textures/cubemaps/eyecubemap.dds",
    "textures/cubemaps/shinydull_e.dds",
    "textures/weapons/steel/steelbattleaxe.dds",
    "textures/weapons/steel/steelbattleaxe_n.dds",
    "textures/weapons/steel/steelbattleaxe_m.dds",
}


def test_skyrim_live_graph_filter_admits_only_exact_battleaxe_closure() -> None:
    exact_assets = [
        *_skyrim_activators(),
        *[_asset("texture", path) for path in sorted(_SKYRIM_TEXTURES)],
    ]
    unrelated_assets = [
        _asset(
            "nif",
            "Meshes/Weapons/Steel/SteelWarAxe.nif",
            form_key="Skyrim.esm:01399B",
            signature="WEAP",
        ),
        _asset(
            "nif",
            "Meshes/Weapons/Steel/SteelBow.nif",
            form_key="Skyrim.esm:013986",
            signature="WEAP",
        ),
        _asset("texture", "Textures/Weapons/Steel/SteelWarAxe.dds"),
    ]
    graph_assets = [*exact_assets, *unrelated_assets]
    closure = weapon_mvp_asset_closure("SkyrimSE", "FO4", graph_assets)

    assert closure is SKYRIM_STEEL_BATTLEAXE_MVP_ASSET_CLOSURE
    allowed = [
        asset
        for asset in graph_assets
        if _is_world_only_mvp_asset(
            asset,
            SKYRIM_MVP_EXCLUDE_SIGNATURES,
            closure,
        )
    ]
    assert allowed == exact_assets


def test_skyrim_manifest_matches_both_nifs_and_their_live_texture_sets() -> None:
    assert SKYRIM_STEEL_BATTLEAXE_MVP_ASSET_CLOSURE.assets == frozenset(
        {
            ("nif", "meshes/weapons/steel/steelbattleaxe.nif"),
            ("nif", "meshes/weapons/steel/1stpersonsteelbattleaxe.nif"),
            *[("texture", path) for path in _SKYRIM_TEXTURES],
        }
    )


def test_skyrim_closure_requires_both_exact_record_owned_nifs() -> None:
    world, first_person = _skyrim_activators()
    wrong_stat = _asset(
        "nif",
        "Meshes/Weapons/Steel/1stPersonSteelBattleAxe.nif",
        form_key="Skyrim.esm:020E28",
        signature="STAT",
    )

    assert weapon_mvp_asset_closure("skyrimse", "fo4", [world]) is None
    assert weapon_mvp_asset_closure("skyrimse", "fo4", [first_person]) is None
    assert weapon_mvp_asset_closure("skyrimse", "fo4", [world, wrong_stat]) is None
    assert weapon_mvp_asset_closure("fnv", "fo4", [world, first_person]) is None


def test_closure_does_not_grant_unrelated_skyrim_weap_or_stat_assets() -> None:
    closure = weapon_mvp_asset_closure("skyrimse", "fo4", _skyrim_activators())
    unrelated = [
        _asset(
            "nif",
            "Meshes/Weapons/Steel/SteelBow.nif",
            form_key="Skyrim.esm:013986",
            signature="WEAP",
        ),
        _asset(
            "nif",
            "Meshes/Weapons/Steel/1stPersonSteelSword.nif",
            form_key="Skyrim.esm:020E29",
            signature="STAT",
        ),
    ]

    assert closure is not None
    assert "STAT" not in SKYRIM_MVP_EXCLUDE_SIGNATURES
    assert all(
        not _is_world_only_mvp_asset(
            asset,
            SKYRIM_MVP_EXCLUDE_SIGNATURES,
            closure,
        )
        for asset in unrelated
    )
    assert _is_world_only_mvp_asset(
        _asset(
            "nif",
            "Meshes/Clutter/Common/Crate.nif",
            form_key="Skyrim.esm:020E30",
            signature="STAT",
        ),
        SKYRIM_MVP_EXCLUDE_SIGNATURES,
        closure,
    )


def test_skyrim_closure_materializes_only_the_audited_textures(tmp_path) -> None:
    source_root = tmp_path / "source"
    for source_path in _SKYRIM_TEXTURES:
        path = source_root.joinpath(*source_path.split("/"))
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(source_path.encode())
    runtime = _UnifiedRecordRuntime(
        type(
            "Request",
            (),
            {
                "source_game": "skyrimse",
                "target_game": "fo4",
                "output_root": tmp_path / "output",
            },
        )()
    )
    ctx = type("Context", (), {"source_data_dir": source_root})()

    existing_unresolved_texture = _asset(
        "texture", "Textures/Weapons/Steel/SteelBattleAxe.dds"
    )
    expanded = runtime._augment_weapon_mvp_asset_closure(
        [*_skyrim_activators(), existing_unresolved_texture],
        tmp_path / "Skyrim.esm",
        ctx,
    )

    assert len(expanded) == 10
    textures = [asset for asset in expanded if asset.asset_type == "texture"]
    assert {
        asset.source_path.replace("\\", "/").casefold() for asset in textures
    } == _SKYRIM_TEXTURES
    assert all(
        asset.resolved_path is not None and Path(asset.resolved_path).is_file()
        for asset in textures
    )
