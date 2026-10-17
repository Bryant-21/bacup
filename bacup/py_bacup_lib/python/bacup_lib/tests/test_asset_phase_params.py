from __future__ import annotations

from types import SimpleNamespace

import pytest

from bacup_lib.models import (
    AssetProvenance,
    AssetRef,
    DependencyGraph,
    RecordNode,
)
from bacup_lib.workflows.asset_phases import (
    _params_for_convert_animations,
    _params_for_convert_havok,
    _params_for_convert_nifs,
    _params_for_convert_skeleton,
    phase_postprocess_havok_native,
)


def _graph(*assets: AssetRef) -> DependencyGraph:
    root = RecordNode(
        form_key="SeventySix.esm:000800",
        editor_id="B21_Test",
        record_type="STAT",
        assets=list(assets),
    )
    return DependencyGraph(root=root, all_records=[root], all_assets=list(assets), errors=[])


def _orchestrator(*assets: AssetRef):
    return SimpleNamespace(
        source_game="fo76",
        target_game="fo4",
        overwrite_existing=False,
        conversion_workers=None,
        disable_nif_collision_memo=False,
        emit_first_person=False,
        morph_weight_cap=0.5,
        base_asset_namespace="",
        _source_profile=None,
        _target_profile=None,
        _addon_index_map={},
        graph=_graph(*assets),
        _expand_animation_dirs=lambda _root: [],
        _load_target_behavior_paths=lambda: set(),
    )


def test_starfield_material_params_include_nifs_for_native_parent_discovery(tmp_path):
    from bacup_lib.workflows.asset_phases import _params_for_convert_materials

    nif = AssetRef("nif", "Meshes/Weapons/Shotty.nif", str(tmp_path / "shotty.nif"))
    material = AssetRef("material", "Materials/Weapons/Shotty.mat", str(tmp_path / "shotty.mat"))
    orchestrator = _orchestrator(nif, material)
    orchestrator.source_game = "starfield"
    params = _params_for_convert_materials(orchestrator)
    assert params["nif_paths"] == [{"source_path": nif.source_path, "resolved_path": nif.resolved_path}]
    assert params["materials"][0]["source_path"] == material.source_path


def _admitted_weapon_receipt(
    *,
    source_form_key: str,
    world_model: str,
    first_person_model: str,
    anim_type: str,
    first_person_stat_form_key: str = "",
) -> dict:
    receipt = {
        "status": "admitted",
        "role": "melee",
        "policy": "mvp_melee_v1",
        "source_form_key": source_form_key,
        "world_model": world_model,
        "first_person_model": first_person_model,
        "weapon_role": "melee",
        "anim_type": anim_type,
    }
    if first_person_stat_form_key:
        receipt["first_person_stat_form_key"] = first_person_stat_form_key
    return receipt


def _weapon_asset(
    path: str,
    resolved_path: str,
    *,
    form_key: str,
    signature: str = "WEAP",
) -> AssetRef:
    return AssetRef(
        "nif",
        path,
        resolved_path,
        provenance=AssetProvenance(
            added_by_record_fk=form_key,
            added_by_record_eid="",
            added_by_field="MODL",
            walk_depth=0,
            walker_pass="native_asset_collect",
            added_by_record_sig=signature,
        ),
    )


def test_params_for_convert_nifs_forwards_asset_fields(tmp_path):
    plain = AssetRef("nif", "Meshes/effects/x.nif", str(tmp_path / "x.nif"))
    wired = AssetRef(
        "nif",
        "Meshes/Workshop/Generator.nif",
        str(tmp_path / "Generator.nif"),
        workshop_wire_point=(3.5, 15.0, 77.0),
    )
    orch = _orchestrator(plain, wired)
    orch._addon_index_map = {78: 760001}

    params = _params_for_convert_nifs(orch, [plain, wired])

    assert params["addon_index_map"] == {"78": 760001}
    assert params["nif_paths"] == [
        {"source_path": "Meshes/effects/x.nif", "resolved_path": str(tmp_path / "x.nif")},
        {
            "source_path": "Meshes/Workshop/Generator.nif",
            "resolved_path": str(tmp_path / "Generator.nif"),
            "workshop_wire_point": [3.5, 15.0, 77.0],
        },
    ]


@pytest.mark.parametrize("store_misses", [False, True])
@pytest.mark.parametrize("source_game", ["fnv", "starfield", "skyrimse"])
def test_params_for_convert_nifs_resolves_target_skeleton_without_skin_options(
    tmp_path, store_misses, source_game
):
    """A default legacy regen requests no skin options, and that is exactly the
    run whose translated binds need reposing onto the FO4 rest pose. A store
    miss must not report an on-disk overlay asset as absent -- that silently
    disabled the skin rebind for an entire regen."""

    class MissingStore:
        def materialize(self, _path):
            return None

    skeleton = tmp_path / "meshes/actors/character/characterassets/skeleton.nif"
    skeleton.parent.mkdir(parents=True)
    skeleton.write_bytes(b"")

    asset = AssetRef("nif", "Armor/VaultSuit/M/Outfit.NIF", str(tmp_path / "Outfit.NIF"))
    orch = _orchestrator(asset)
    orch.source_game = source_game
    orch.target_extracted_dir = str(tmp_path)
    if store_misses:
        orch.target_asset_store = MissingStore()

    params = _params_for_convert_nifs(orch, [asset])

    assert params["target_skeleton"] == str(skeleton)


def test_starfield_reused_armor_mesh_gets_distinct_first_person_pose_and_destination(tmp_path):
    skeleton = tmp_path / "meshes/actors/character/_1stperson/characterassets/skeleton.nif"
    skeleton.parent.mkdir(parents=True)
    skeleton.write_bytes(b"")
    third = _weapon_asset("Meshes/Clothes/Gloves.nif", str(tmp_path / "Gloves.nif"), form_key="000800@Starfield.esm", signature="ARMA")
    third.provenance.added_by_field = "BipedModels.MOD2"
    first = _weapon_asset("Meshes/Clothes/Gloves.nif", str(tmp_path / "Gloves.nif"), form_key="000800@Starfield.esm", signature="ARMA")
    first.provenance.added_by_field = "1stPersons.MOD4"
    orch = _orchestrator(third)
    orch.source_game = "starfield"
    orch.target_extracted_dir = str(tmp_path)
    orch.base_asset_namespace = "Starfield"
    orch._forced_asset_output_subpath = lambda _asset: "Meshes/Starfield/Clothes/Gloves.nif"
    orch.graph.root.assets = [third, first]

    entries = _params_for_convert_nifs(orch)["nif_paths"]

    assert len(entries) == 2
    assert entries[0]["source_path"] == entries[1]["source_path"]
    assert entries[1]["target_skeleton"] == str(skeleton)
    assert entries[1]["output_subpath"] == "Meshes/Starfield/FirstPerson/Clothes/Gloves.nif"
    assert "target_skeleton" not in entries[0]


def test_starfield_first_person_armor_requires_its_target_skeleton(tmp_path):
    first = _weapon_asset("Meshes/Clothes/Gloves.nif", str(tmp_path / "Gloves.nif"), form_key="000800@Starfield.esm", signature="ARMA")
    first.provenance.added_by_field = "MOD5"
    orch = _orchestrator(first)
    orch.source_game = "starfield"
    orch.target_extracted_dir = str(tmp_path)
    with pytest.raises(ValueError, match="first-person skeleton"):
        _params_for_convert_nifs(orch)


@pytest.mark.parametrize(
    ("source_game", "phase"),
    [("fnv", "convert_gamebryo_nifs"), ("skyrimse", "convert_nifs_v2")],
)
def test_asset_wave_preserves_resolved_target_skeleton(tmp_path, source_game, phase):
    from bacup_lib.workflows.unified import AssetWaveBuilder, _wave_plan_for

    skeleton = tmp_path / "meshes/actors/character/characterassets/skeleton.nif"
    skeleton.parent.mkdir(parents=True)
    skeleton.write_bytes(b"")
    asset = AssetRef("nif", "Armor/LeatherArmor/M/OutfitM.NIF", str(tmp_path / "OutfitM.NIF"))
    shim = _orchestrator(asset)
    shim.source_game = source_game
    shim.target_extracted_dir = str(tmp_path)
    shim.mod_path = tmp_path / "FNV_FO3"
    shim._summary = SimpleNamespace(nifs_total=0)
    shim._target_has_asset = lambda _asset: False
    driver = SimpleNamespace(
        ctx=SimpleNamespace(source_game=source_game, target_game="fo4"),
        _req=SimpleNamespace(options=SimpleNamespace(exclude_signatures=())),
    )
    builder = SimpleNamespace(
        driver=driver,
        _wave_plan=_wave_plan_for(source_game, "fo4"),
        runner=SimpleNamespace(emit_log=lambda *_args: None),
        runs=SimpleNamespace(nifs=SimpleNamespace(id="test-nifs")),
    )

    stage = AssetWaveBuilder._build_nif_stage(builder, shim, ())

    assert stage.phase == phase
    assert stage.params["target_skeleton"] == str(skeleton)


@pytest.mark.parametrize("base_signature", ["ARMA", "STAT"])
def test_skyrim_shared_armor_model_keeps_base_and_first_person_outputs(tmp_path, base_signature):
    skeleton = tmp_path / "meshes/actors/character/_1stperson/characterassets/skeleton.nif"
    skeleton.parent.mkdir(parents=True)
    skeleton.write_bytes(b"")
    asset = _weapon_asset(
        "Meshes/Armor/Dragonbone/DragonboneGauntlets_1.nif",
        str(tmp_path / "DragonboneGauntlets_1.nif"),
        form_key="000800@Skyrim.esm",
        signature=base_signature,
    )
    asset.provenance.added_by_field = "MOD2" if base_signature == "ARMA" else "MODL"
    asset.owner_claims = [
        {"source_record_signature": base_signature, "source_subrecord_sig": asset.provenance.added_by_field},
        {"source_record_signature": "ARMA", "source_subrecord_sig": "MOD4"},
    ]
    orch = _orchestrator(asset)
    orch.source_game = "skyrimse"
    orch.target_extracted_dir = str(tmp_path)

    entries = _params_for_convert_nifs(orch)["nif_paths"]

    assert len(entries) == 2
    assert "output_subpath" not in entries[0]
    assert "target_skeleton" not in entries[0]
    assert entries[1]["output_subpath"] == "Meshes/FirstPerson/Armor/Dragonbone/DragonboneGauntlets_1.nif"
    assert entries[1]["target_skeleton"] == str(skeleton)
    assert entries[1]["source_path"] == entries[0]["source_path"]


def test_skyrim_first_person_only_armor_uses_its_own_skeleton_and_destination(tmp_path):
    skeleton = tmp_path / "meshes/actors/character/_1stperson/characterassets/skeleton.nif"
    skeleton.parent.mkdir(parents=True)
    skeleton.write_bytes(b"")
    asset = _weapon_asset("Meshes/Armor/Iron/1stPerson.nif", "source.nif", form_key="000800@Skyrim.esm", signature="ARMA")
    asset.provenance.added_by_field = "1stPersons.MOD5"
    orch = _orchestrator(asset)
    orch.source_game = "skyrimse"
    orch.target_extracted_dir = str(tmp_path)
    orch.base_asset_namespace = "Skyrim"
    orch._forced_asset_output_subpath = lambda _asset: "Meshes/Skyrim/Armor/Iron/1stPerson.nif"

    entries = _params_for_convert_nifs(orch)["nif_paths"]

    assert len(entries) == 1
    assert entries[0]["output_subpath"] == "Meshes/Skyrim/FirstPerson/Armor/Iron/1stPerson.nif"
    assert entries[0]["target_skeleton"] == str(skeleton)


def test_params_for_convert_nifs_marks_exact_skyrim_battleaxe_models_as_melee(
    tmp_path,
):
    world = _weapon_asset(
        r"Weapons\Steel\SteelBattleAxe.nif",
        str(tmp_path / "SteelBattleAxe.nif"),
        form_key="013984@Skyrim.esm",
    )
    first_person = _weapon_asset(
        r"Meshes\Weapons\Steel\1stPersonSteelBattleAxe.nif",
        str(tmp_path / "1stPersonSteelBattleAxe.nif"),
        form_key="020E27@Skyrim.esm",
        signature="STAT",
    )
    orch = _orchestrator(world, first_person)
    orch.source_game = "skyrimse"
    orch._weapon_metadata_index = {
        "013984@Skyrim.esm": _admitted_weapon_receipt(
            source_form_key="013984@Skyrim.esm",
            world_model=r"Weapons\Steel\SteelBattleAxe.nif",
            first_person_model=r"Weapons\Steel\1stPersonSteelBattleAxe.nif",
            anim_type="TwoHandAxe",
            first_person_stat_form_key="020E27@Skyrim.esm",
        )
    }

    params = _params_for_convert_nifs(orch, [world, first_person])

    assert [entry["weapon_role"] for entry in params["nif_paths"]] == [
        "melee",
        "melee",
    ]


@pytest.mark.parametrize(
    ("source_game", "animation_type"),
    [("skyrimse", "0"), ("skyrimse", "6"), ("fnv", "1"), ("fo3", "2")],
)
def test_params_for_convert_nifs_forwards_melee_families(
    tmp_path, source_game, animation_type
):
    world_path = f"Weapons/Melee{animation_type}/World.nif"
    first_person_path = f"Meshes/Weapons/Melee{animation_type}/FirstPerson.nif"
    world = AssetRef("nif", world_path, str(tmp_path / "World.nif"))
    first_person = AssetRef(
        "nif", first_person_path, str(tmp_path / "FirstPerson.nif")
    )
    orch = _orchestrator(world, first_person)
    orch.source_game = source_game
    if source_game != "skyrimse":
        orch._source_profile = SimpleNamespace(id=source_game, asset_prefix=source_game)
    orch._weapon_metadata_index = {
        f"{animation_type}@Source.esm": _admitted_weapon_receipt(
            source_form_key=f"0001{animation_type}@Source.esm",
            world_model=world_path,
            first_person_model=first_person_path,
            anim_type=animation_type,
        )
    }

    params = _params_for_convert_nifs(orch, [world, first_person])

    assert [entry["weapon_role"] for entry in params["nif_paths"]] == [
        "melee",
        "melee",
    ]


def test_params_for_convert_nifs_fails_closed_for_shared_model_role_conflict(
    tmp_path,
):
    shared = AssetRef(
        "nif",
        r"Meshes\Weapons\Shared\Shared.nif",
        str(tmp_path / "Shared.nif"),
    )
    orch = _orchestrator(shared)
    orch.source_game = "skyrimse"
    orch._weapon_metadata_index = {
        "melee": _admitted_weapon_receipt(
            source_form_key="000111@Skyrim.esm",
            world_model=r"Weapons\Shared\Shared.nif",
            first_person_model=r"Weapons\Shared\Shared.nif",
            anim_type="1",
        ),
        "ranged": {
            "status": "rejected",
            "role": "gun",
            "policy": "mvp_melee_v1",
            "source_form_key": "000222@Skyrim.esm",
            "world_model": r"Meshes\Weapons\Shared\Shared.nif",
            "first_person_model": "",
            "anim_type": "7",
        },
    }

    entry = _params_for_convert_nifs(orch, [shared])["nif_paths"][0]

    assert "weapon_role" not in entry
    assert orch._weapon_mvp_asset_plan.conflict_count == 1
    assert orch._weapon_mvp_asset_plan.conflicts[0].roles == ("gun", "melee")


def test_params_for_convert_nifs_preserves_general_fnv_weapon_roles(tmp_path):
    pistol = AssetRef(
        "nif",
        r"Weapons\10mmPistol\10mmPistol.nif",
        str(tmp_path / "10mmPistol.nif"),
    )
    bat = AssetRef(
        "nif",
        r"Weapons\1HandMelee\BaseballBat.nif",
        str(tmp_path / "BaseballBat.nif"),
    )
    fnv = _orchestrator(pistol, bat)
    fnv.source_game = "fnv"
    fnv._source_profile = SimpleNamespace(id="fnv", asset_prefix="fnv")
    fnv._weapon_metadata_index = {
        "0000434F@FalloutNV.esm": {
            "base_model": pistol.source_path,
            "weapon_role": "gun",
            "anim_type": "3",
        },
        "0000421F@FalloutNV.esm": {
            "base_model": bat.source_path,
            "weapon_role": "melee",
            "anim_type": "1",
        },
    }

    params = _params_for_convert_nifs(fnv, [pistol, bat])

    assert [entry["weapon_role"] for entry in params["nif_paths"]] == ["gun", "melee"]


def test_params_for_convert_nifs_adds_static_cloth_variant_from_record_decision(tmp_path):
    source_path = (
        "Meshes/ATX/backpack_flair/Flair_FilmReel/ATX_FilmReel_Flair.nif"
    )
    output_subpath = (
        "Meshes/BACUP_Static/ATX/backpack_flair/Flair_FilmReel/"
        "ATX_FilmReel_Flair.nif"
    )
    asset = AssetRef("nif", source_path, str(tmp_path / "film_reel.nif"))
    orch = _orchestrator(asset)
    orch._conversion_decisions = [
        {
            "kind": "fo76_misc_static_model_variant",
            "message": (
                '{"source_path":"' + source_path + '",'
                '"output_subpath":"' + output_subpath + '"}'
            ),
        }
    ]

    params = _params_for_convert_nifs(orch, [asset])

    assert params["nif_paths"] == [
        {
            "source_path": source_path,
            "resolved_path": str(tmp_path / "film_reel.nif"),
        },
        {
            "source_path": source_path,
            "resolved_path": str(tmp_path / "film_reel.nif"),
            "output_subpath": output_subpath,
            "strip_cloth": True,
        },
    ]


def test_params_for_convert_havok_includes_nif_assets(tmp_path):
    hkx = AssetRef(
        "behavior",
        "Meshes/Actors/Mirelurk/characterassets/seaweed.hkx",
        str(tmp_path / "seaweed.hkx"),
    )
    nif = AssetRef(
        "nif",
        "Meshes/Actors/Mirelurk/characterassets/seaweed.nif",
        str(tmp_path / "seaweed.nif"),
    )
    orch = _orchestrator(hkx, nif)
    orch.additional_source_asset_roots = (
        tmp_path / "extracted" / "fo3",
        tmp_path / "extracted" / "fo3-override",
    )

    params = _params_for_convert_havok(orch)

    assert params["hkx_assets"] == [
        {
            "source_path": hkx.source_path,
            "resolved_path": hkx.resolved_path,
            "asset_type": "behavior",
        }
    ]
    assert params["nif_assets"] == [
        {"source_path": nif.source_path, "resolved_path": nif.resolved_path}
    ]
    assert params["additional_source_asset_roots"] == [
        str(tmp_path / "extracted" / "fo3"),
        str(tmp_path / "extracted" / "fo3-override"),
    ]


def test_fnv_kf_assets_use_shared_animation_event_map(tmp_path):
    human = AssetRef(
        "behavior",
        "Meshes/Characters/_Male/IdleAnims/dlcpittweldinghighidle.kf",
        str(tmp_path / "dlcpittweldinghighidle.kf"),
    )
    gecko_dir = tmp_path / "meshes" / "creatures" / "nvgecko"
    gecko_dir.mkdir(parents=True)
    source_skeleton = gecko_dir / "skeleton.nif"
    source_skeleton.write_bytes(b"fixture")
    gecko_paths = [
        "mtidle.kf",
        "swimmtforward.kf",
        "mtturnleft.kf",
        "mtturnright.kf",
        "h2hattackforwardpower.kf",
    ]
    gecko_assets = [
        AssetRef(
            "kf_animation",
            f"creatures/NVGecko/{path}",
            str(gecko_dir / path),
        )
        for path in gecko_paths
    ]
    unrelated_gecko = AssetRef(
        "kf_animation",
        "Meshes/creatures/NVGecko/IdleAnims/MT_SpecialIdle_EyeLickLeft.kf",
        str(tmp_path / "MT_SpecialIdle_EyeLickLeft.kf"),
    )
    behavior = AssetRef(
        "behavior",
        "Meshes/Actors/Example/Behaviors/Example.hkx",
        str(tmp_path / "Example.hkx"),
    )
    unrelated_skeleton = AssetRef(
        "nif",
        "Meshes/Characters/_Male/skeleton.nif",
        str(tmp_path / "humanoid-skeleton.nif"),
    )
    gecko_skeleton = AssetRef(
        "nif",
        "creatures/NVGecko/skeleton.nif",
        str(source_skeleton),
    )
    orch = _orchestrator(
        human,
        *gecko_assets,
        unrelated_gecko,
        behavior,
        unrelated_skeleton,
        gecko_skeleton,
    )
    orch.source_game = "fnv"

    params = _params_for_convert_animations(orch)

    expected_clips = {
        "mtidle.kf": ("Idle.hkx", "reject_nonzero"),
        "swimmtforward.kf": (
            "WalkForward.hkx",
            "extract_planar_reference_frame",
        ),
        "mtturnleft.kf": ("TurnLeft90.hkx", "reject_nonzero"),
        "mtturnright.kf": ("TurnRight90.hkx", "reject_nonzero"),
        "h2hattackforwardpower.kf": (
            "Attack1.hkx",
            "extract_planar_reference_frame",
        ),
    }
    assert params["animations"] == [
        {
            "source_path": asset.source_path,
            "resolved_path": asset.resolved_path,
            "asset_type": "kf_animation",
            "source_skeleton_path": str(source_skeleton),
            "runtime_skeleton_path": (
                "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"
            ),
            "original_skeleton_name": "NVGecko",
            "output_clip_path": (
                "Actors\\B21_FNVGecko\\Animations\\"
                + expected_clips[asset.source_path.rsplit("/", 1)[-1]][0]
            ),
            "extracted_motion_policy": expected_clips[
                asset.source_path.rsplit("/", 1)[-1]
            ][1],
        }
        for asset in gecko_assets
    ]
    assert params["event_map"] == {
        "start": "",
        "end": "",
        "hit": "HitFrame",
        "Hit": "HitFrame",
        "Equip": "weaponDraw",
        "Unequip": "weaponSheathe",
    }
    assert "Sound: NPCGeckoSwim" not in params["event_map"]

    skeleton_params = _params_for_convert_skeleton(orch)
    assert skeleton_params == {
        "skeleton_nif": gecko_skeleton.source_path,
        "resolved_path": gecko_skeleton.resolved_path,
        "source_game": "fnv",
        "target_game": "fo4",
        "preserve_source_rig": True,
        "output_skeleton_path": (
            "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"
        ),
        "skeleton_name": "NVGecko",
    }
    assert {
        animation["runtime_skeleton_path"] for animation in params["animations"]
    } == {skeleton_params["output_skeleton_path"]}
    assert {
        animation["original_skeleton_name"] for animation in params["animations"]
    } == {skeleton_params["skeleton_name"]}

    unrelated_only = _orchestrator(unrelated_skeleton)
    unrelated_only.source_game = "fnv"
    assert _params_for_convert_skeleton(unrelated_only) is None


def test_postprocess_havok_forwards_distinct_source_and_target_roots(tmp_path):
    calls = []

    class FakeRustRun:
        def run_phase(self, phase, **kwargs):
            calls.append((phase, kwargs))
            return {"assets_written": 2, "warnings": 0, "elapsed_ms": 1}

    source_root = tmp_path / "fo76_extracted"
    target_root = tmp_path / "fo4_extracted"
    orchestrator = SimpleNamespace(
        _rust_conversion_run=FakeRustRun(),
        mod_path=tmp_path / "mod",
        source_data_dir=source_root,
        target_extracted_dir=target_root,
        _summary=SimpleNamespace(havok_converted=0, havok_failed=0),
    )
    runner = SimpleNamespace(emit_log=lambda *_args: None)

    phase_postprocess_havok_native(orchestrator, runner, SimpleNamespace())

    assert calls == [
        (
            "postprocess_havok_assets",
            {
                "mod_path": str(orchestrator.mod_path),
                "source_extracted_dir": str(source_root),
                "target_extracted_dir": str(target_root),
                "params": {"source_archive_dirs": []},
            },
        )
    ]
    assert orchestrator._summary.havok_converted == 2


def test_native_overwrite_paths_pass_the_target_collision_filter():
    from bacup_lib.workflows.asset_phases import ConversionFixups

    fixups = ConversionFixups(
        SimpleNamespace(
            use_base_game_assets=True,
            target_asset_index=SimpleNamespace(has_asset=lambda _asset: True),
            target_extracted_dir=None,
            _rust_conversion_run=SimpleNamespace(
                native_overwrite_paths=lambda: [
                    "materials/rocks/rock01.bgsm",
                    "textures/rocks/rock01_d.dds",
                    "meshes/actors/character/hair01.nif",
                ]
            ),
        )
    )

    assert not fixups._target_has_asset(AssetRef("material", "Rocks\\Rock01.BGSM"))
    assert not fixups._target_has_asset(AssetRef("texture", "Textures/Rocks/Rock01_d.dds"))
    assert fixups._target_has_asset(AssetRef("material", "Rocks/Other.bgsm"))
    assert fixups._target_has_asset(AssetRef("texture", "Rocks/Rock01_n.dds"))
    assert fixups._target_has_asset(AssetRef("nif", "Rocks/Rock01.bgsm"))
    assert not fixups._target_has_asset(AssetRef("nif", "Actors\Character\Hair01.nif"))
    assert fixups._target_has_asset(AssetRef("nif", "Actors/Character/Hair02.nif"))


def test_without_a_native_run_every_collision_still_filters():
    from bacup_lib.workflows.asset_phases import ConversionFixups

    fixups = ConversionFixups(
        SimpleNamespace(
            use_base_game_assets=True,
            target_asset_index=SimpleNamespace(has_asset=lambda _asset: True),
            target_extracted_dir=None,
            _rust_conversion_run=SimpleNamespace(),
        )
    )

    assert fixups._target_has_asset(AssetRef("material", "Rocks/Rock01.bgsm"))
