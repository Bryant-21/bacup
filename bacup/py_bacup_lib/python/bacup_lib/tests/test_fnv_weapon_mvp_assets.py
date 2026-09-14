from pathlib import Path

from bacup_lib.fnv_weapon_mvp_assets import fnv_weapon_mvp_asset_closure
from bacup_lib.models import AssetProvenance, AssetRef
from bacup_lib.source_pairs import FNV_MVP_EXCLUDE_SIGNATURES
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


def _hatchet_assets() -> list[AssetRef]:
    return [
        _asset(
            "nif",
            r"Weapons\1HandMelee\Hatchet.NIF",
            form_key="FalloutNV.esm:11A8E4",
            signature="WEAP",
        ),
        _asset("texture", "Weapons/1HandMelee/Hatchet_D.dds"),
        _asset("texture", "textures/weapons/1handmelee/hatchet_n.DDS"),
    ]


def test_exact_hatchet_asset_closure_bypasses_fnv_weap_fence() -> None:
    assets = _hatchet_assets()
    closure = fnv_weapon_mvp_asset_closure("FNV", "FO4", assets)

    assert closure is not None
    assert all(
        _is_world_only_mvp_asset(asset, frozenset({"WEAP"}), closure)
        for asset in assets
    )


def test_hatchet_closure_normalizes_case_slashes_and_data_prefix() -> None:
    assets = _hatchet_assets()
    closure = fnv_weapon_mvp_asset_closure("fnv", "fo4", assets)

    assert closure is not None
    assert _is_world_only_mvp_asset(
        _asset("nif", r"DATA\MESHES\WEAPONS\1HANDMELEE\HATCHET.NIF"),
        frozenset({"WEAP"}),
        closure,
    )
    assert _is_world_only_mvp_asset(
        _asset("texture", r"data\textures\weapons\1handmelee\HATCHET_N.dds"),
        frozenset({"WEAP"}),
        closure,
    )


def test_other_excluded_weapon_assets_remain_filtered() -> None:
    closure = fnv_weapon_mvp_asset_closure("fnv", "fo4", _hatchet_assets())
    other_weapon = _asset(
        "nif",
        "Meshes/Weapons/1HandMelee/Cleaver.NIF",
        form_key="0010E4A2@FalloutNV.esm",
        signature="WEAP",
    )

    assert closure is not None
    assert not _is_world_only_mvp_asset(other_weapon, frozenset({"WEAP"}), closure)
    assert not _is_world_only_mvp_asset(
        _asset("texture", "Textures/Weapons/1HandMelee/Cleaver_D.dds"),
        frozenset({"WEAP"}),
        closure,
    )
    assert not _is_world_only_mvp_asset(
        _asset(
            "nif",
            "Meshes/Weapons/1HandMelee/ThrowingHatchet.NIF",
            form_key="FalloutNV.esm:0F0001",
            signature="WEAP",
        ),
        frozenset({"WEAP"}),
        closure,
    )


def test_fnv_live_fence_rejects_stat_owned_weapon_but_keeps_other_stat() -> None:
    closure = fnv_weapon_mvp_asset_closure("fnv", "fo4", _hatchet_assets())
    throwing_hatchet = _asset(
        "nif",
        "Meshes/Weapons/1HandMelee/ThrowingHatchet.NIF",
        form_key="FalloutNV.esm:0F0001",
        signature="STAT",
    )
    non_weapon_stat = _asset(
        "nif",
        "Meshes/Clutter/Junk/MetalBox.nif",
        form_key="FalloutNV.esm:0F0002",
        signature="STAT",
    )

    assert "STAT" not in FNV_MVP_EXCLUDE_SIGNATURES
    assert closure is not None
    assert not _is_world_only_mvp_asset(
        throwing_hatchet,
        FNV_MVP_EXCLUDE_SIGNATURES,
        closure,
    )
    assert _is_world_only_mvp_asset(
        non_weapon_stat,
        FNV_MVP_EXCLUDE_SIGNATURES,
        closure,
    )


def test_hatchet_manifest_does_not_open_the_weapons_directory() -> None:
    closure = fnv_weapon_mvp_asset_closure("fnv", "fo4", _hatchet_assets())

    assert closure is not None
    assert not _is_world_only_mvp_asset(
        _asset("nif", "Meshes/Weapons/1HandMelee/Hatchet01.NIF"),
        frozenset({"WEAP"}),
        closure,
    )
    assert not _is_world_only_mvp_asset(
        _asset("texture", "Textures/Weapons/1HandMelee/Hatchet_G.dds"),
        frozenset({"WEAP"}),
        closure,
    )


def test_closure_requires_exact_owned_hatchet_mesh() -> None:
    wrong_owner = _asset(
        "nif",
        "Meshes/Weapons/1HandMelee/Hatchet.NIF",
        form_key="0011A8E5@FalloutNV.esm",
        signature="WEAP",
    )
    wrong_plugin = _asset(
        "nif",
        "Meshes/Weapons/1HandMelee/Hatchet.NIF",
        form_key="0011A8E4@Fallout3.esm",
        signature="WEAP",
    )

    assert fnv_weapon_mvp_asset_closure("fnv", "fo4", [wrong_owner]) is None
    assert fnv_weapon_mvp_asset_closure("fnv", "fo4", [wrong_plugin]) is None
    assert (
        fnv_weapon_mvp_asset_closure(
            "fnv",
            "fo4",
            [
                _asset(
                    "texture",
                    "Textures/Weapons/1HandMelee/Hatchet_D.dds",
                    form_key="0011A8E4@FalloutNV.esm",
                    signature="WEAP",
                )
            ],
        )
        is None
    )
    assert fnv_weapon_mvp_asset_closure("skyrimse", "fo4", _hatchet_assets()) is None


def test_exact_closure_materializes_only_missing_hatchet_textures(tmp_path) -> None:
    source_root = tmp_path / "source"
    texture_root = source_root / "Textures" / "Weapons" / "1HandMelee"
    texture_root.mkdir(parents=True)
    diffuse = texture_root / "Hatchet_D.dds"
    normal = texture_root / "Hatchet_N.dds"
    diffuse.write_bytes(b"diffuse")
    normal.write_bytes(b"normal")
    runtime = _UnifiedRecordRuntime(
        type(
            "Request",
            (),
            {
                "source_game": "fnv",
                "target_game": "fo4",
                "output_root": tmp_path / "output",
            },
        )()
    )
    ctx = type("Context", (), {"source_data_dir": source_root})()

    expanded = runtime._augment_fnv_weapon_mvp_asset_closure(
        [_hatchet_assets()[0]],
        tmp_path / "FalloutNV.esm",
        ctx,
    )

    by_path = {asset.source_path.casefold(): asset for asset in expanded}
    assert set(by_path) == {
        r"weapons\1handmelee\hatchet.nif",
        "textures/weapons/1handmelee/hatchet_d.dds",
        "textures/weapons/1handmelee/hatchet_n.dds",
    }
    assert Path(
        by_path["textures/weapons/1handmelee/hatchet_d.dds"].resolved_path
    ).samefile(diffuse)
    assert Path(
        by_path["textures/weapons/1handmelee/hatchet_n.dds"].resolved_path
    ).samefile(normal)
