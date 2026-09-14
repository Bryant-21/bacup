from __future__ import annotations

from types import SimpleNamespace

from bacup_lib.models import AssetProvenance, AssetRef
from bacup_lib.weapon_mvp_assets import build_weapon_mvp_asset_plan
from bacup_lib.weapon_report import weapon_metadata_rows


def _asset(
    asset_type: str,
    path: str,
    *,
    owner: str = "000100@FalloutNV.esm",
    signature: str = "WEAP",
    resolved: bool = True,
) -> AssetRef:
    resolved_path = "/resolved/" + path.replace("\\", "/") if resolved else None
    return AssetRef(
        asset_type=asset_type,
        source_path=path,
        resolved_path=resolved_path,
        provenance=AssetProvenance(
            added_by_record_fk=owner,
            added_by_record_eid="Owner",
            added_by_field="Model",
            walk_depth=0,
            walker_pass="main",
            added_by_record_sig=signature,
        ),
    )


def _receipt(
    form_key: str,
    editor_id: str,
    world_model: str | None,
    *,
    role: str = "melee",
    status: str = "admitted",
    reason_code: str = "admitted",
    target_profile: str = "one_hand_blade",
    first_person_model: str | None = None,
    first_person_stat_form_key: str | None = None,
    policy: str = "mvp_melee_v1",
) -> dict[str, object]:
    return {
        "source_form_key": form_key,
        "source_signature": "WEAP",
        "editor_id": editor_id,
        "status": status,
        "reason_code": reason_code,
        "role": role,
        "policy": policy,
        "target_profile": target_profile,
        "world_model": world_model,
        "first_person_model": first_person_model,
        "first_person_stat_form_key": first_person_stat_form_key,
    }


def test_corpus_plan_admits_each_melee_and_rejects_ranged() -> None:
    assets = [
        _asset("nif", r"Weapons\Melee\Axe.NIF", owner="000100@FalloutNV.esm"),
        _asset("nif", r"Data\Meshes\Weapons\Melee\Spear.nif", owner="000101@FalloutNV.esm"),
        _asset("nif", r"Meshes\Weapons\Guns\Rifle.nif", owner="000102@FalloutNV.esm"),
    ]
    rows = [
        _receipt("000101@FalloutNV.esm", "Spear", "weapons/melee/spear.nif"),
        _receipt(
            "000102@FalloutNV.esm",
            "ThrowingSpear",
            "weapons/guns/rifle.nif",
            role="ranged",
        ),
        _receipt("000100@FalloutNV.esm", "Axe", "weapons/melee/axe.nif"),
    ]

    plan = build_weapon_mvp_asset_plan("FNV", "FO4", assets, rows)
    reversed_plan = build_weapon_mvp_asset_plan(
        "fnv", "fo4", reversed(assets), reversed(rows)
    )

    assert plan == reversed_plan
    assert (plan.candidate_count, plan.admitted_count, plan.rejected_count) == (3, 2, 1)
    assert {closure.editor_id for closure in plan.admitted} == {"Axe", "Spear"}
    assert plan.rejected[0].reason_code == "not_melee_role"
    assert plan.allows(assets[0])
    assert not plan.allows(assets[2])


def test_corpus_plan_preserves_shared_melee_owners_and_resolved_first_person() -> None:
    shared = _asset("nif", r"Meshes\Weapons\Melee\Shared.nif")
    first_person = _asset(
        "nif",
        r"Weapons\Melee\1stPersonShared.nif",
        owner="000200@FalloutNV.esm",
        signature="STAT",
    )
    rows = [
        _receipt("000100@FalloutNV.esm", "SharedA", "weapons/melee/shared.nif"),
        _receipt(
            "000101@FalloutNV.esm",
            "SharedB",
            "weapons/melee/shared.nif",
            first_person_model="weapons/melee/1stpersonshared.nif",
            first_person_stat_form_key="000200@FalloutNV.esm",
        ),
    ]

    plan = build_weapon_mvp_asset_plan("fnv", "fo4", [shared, first_person], rows)

    assert plan.admitted_count == 2
    shared_claims = [
        claim
        for claim in plan.admitted_claims
        if claim.asset == ("nif", "meshes/weapons/melee/shared.nif")
        and claim.model_kind == "world"
    ]
    assert {claim.source_form_key for claim in shared_claims} == {
        "000100@FalloutNV.esm",
        "000101@FalloutNV.esm",
    }
    assert any(
        claim.owner_form_key == "000200@FalloutNV.esm"
        and claim.model_kind == "first_person"
        for claim in plan.admitted_claims
    )


def test_corpus_plan_canonicalizes_owned_texture_and_material_dependencies() -> None:
    form_key = "000100@FalloutNV.esm"
    assets = [
        _asset("nif", r"Data\Meshes\Weapons\Melee\Axe.nif", owner=form_key),
        _asset("texture", r"Weapons\Melee\Axe_D.dds", owner=form_key),
        _asset("material", r"Data\Materials\Weapons\Melee\Axe.BGSM", owner=form_key),
    ]

    plan = build_weapon_mvp_asset_plan(
        "fnv",
        "fo4",
        assets,
        [_receipt(form_key, "Axe", r"WEAPONS\MELEE\AXE.NIF")],
    )

    assert plan.assets == frozenset(
        {
            ("nif", "meshes/weapons/melee/axe.nif"),
            ("texture", "textures/weapons/melee/axe_d.dds"),
            ("material", "materials/weapons/melee/axe.bgsm"),
        }
    )


def test_corpus_plan_fails_closed_for_unresolved_model_and_mixed_role() -> None:
    unresolved = _asset("nif", "weapons/melee/missing.nif", resolved=False)
    shared = _asset("nif", "weapons/shared.nif")
    rows = [
        _receipt("000100@FalloutNV.esm", "Missing", "weapons/melee/missing.nif"),
        _receipt("000101@FalloutNV.esm", "Melee", "weapons/shared.nif"),
        _receipt(
            "000102@FalloutNV.esm",
            "Throwing",
            "weapons/shared.nif",
            role="ranged",
            status="rejected",
            reason_code="not_melee_animation_type",
        ),
    ]

    plan = build_weapon_mvp_asset_plan("fnv", "fo4", [unresolved, shared], rows)

    assert plan.admitted_count == 0
    assert plan.conflict_count == 1
    assert {rejection.reason_code for rejection in plan.rejected} == {
        "unresolved_required_model",
        "not_melee_animation_type",
        "mixed_weapon_roles",
    }


def test_corpus_plan_accepts_bulk_policy_and_fails_closed_on_policy_collision() -> None:
    shared = _asset("nif", "weapons/shared.nif")
    rows = [
        _receipt(
            "000100@FalloutNV.esm",
            "BulkMelee",
            "weapons/shared.nif",
            policy="bulk_melee_v1",
        ),
        _receipt(
            "000101@FalloutNV.esm",
            "OtherPolicy",
            "weapons/shared.nif",
            status="rejected",
            reason_code="projection_unsupported",
            policy="another_policy",
        ),
    ]

    plan = build_weapon_mvp_asset_plan("fnv", "fo4", [shared], rows)

    assert plan.admitted_count == 0
    assert plan.conflicts[0].reason_code == "mixed_weapon_policies"
    assert {rejection.reason_code for rejection in plan.rejected} == {
        "projection_unsupported",
        "mixed_weapon_policies",
    }


def test_corpus_plan_allows_model_less_unarmed_but_not_legacy_role_metadata() -> None:
    unarmed = _receipt(
        "000100@FalloutNV.esm",
        "Unarmed",
        None,
        target_profile="unarmed",
    )
    legacy = {
        "source_form_key": "000101@FalloutNV.esm",
        "editor_id": "LegacyAxe",
        "weapon_role": "melee",
        "base_model": "weapons/melee/axe.nif",
    }

    plan = build_weapon_mvp_asset_plan("fnv", "fo4", [], [legacy, unarmed])

    assert plan.admitted_count == 1
    assert plan.admitted[0].editor_id == "Unarmed"
    assert plan.admitted[0].assets == frozenset()
    assert plan.rejected[0].editor_id == "LegacyAxe"


def test_exact_hatchet_and_battleaxe_receipts_enrich_only_their_own_closures() -> None:
    hatchet = _asset(
        "nif",
        r"Weapons\1HandMelee\Hatchet.NIF",
        owner="11A8E4@FalloutNV.esm",
    )
    hatchet_plan = build_weapon_mvp_asset_plan(
        "fnv",
        "fo4",
        [hatchet],
        [
            _receipt(
                "11A8E4@FalloutNV.esm",
                "WeapNVHatchet",
                r"weapons\1handmelee\Hatchet.NIF",
                first_person_model=r"weapons\1handmelee\Hatchet.NIF",
            )
        ],
    )
    assert ("texture", "textures/weapons/1handmelee/hatchet_d.dds") in hatchet_plan.assets

    world = _asset(
        "nif",
        r"Weapons\Steel\SteelBattleAxe.nif",
        owner="013984@Skyrim.esm",
    )
    first_person = _asset(
        "nif",
        r"Weapons\Steel\1stPersonSteelBattleAxe.nif",
        owner="020E27@Skyrim.esm",
        signature="STAT",
    )
    skyrim_plan = build_weapon_mvp_asset_plan(
        "skyrimse",
        "fo4",
        [world, first_person],
        [
            _receipt(
                "013984@Skyrim.esm",
                "SteelBattleaxe",
                "weapons/steel/steelbattleaxe.nif",
                first_person_model="weapons/steel/1stpersonsteelbattleaxe.nif",
                first_person_stat_form_key="020E27@Skyrim.esm",
                target_profile="two_hand_axe",
            )
        ],
    )
    assert ("texture", "textures/weapons/steel/steelbattleaxe.dds") in skyrim_plan.assets


def test_weapon_metadata_rows_enumerates_native_run_when_graph_is_empty(
    monkeypatch,
) -> None:
    calls = []
    native = SimpleNamespace(
        conversion_run_weapon_metadata=lambda run_id, form_keys: calls.append(
            (run_id, form_keys)
        )
        or [
            {"source_form_key": "000002@FalloutNV.esm", "editor_id": "B"},
            {"source_form_key": "000001@FalloutNV.esm", "editor_id": "A"},
        ]
    )
    monkeypatch.setattr("bacup_lib.native_runtime.load_native_module", lambda: native)
    orchestrator = SimpleNamespace(
        graph=SimpleNamespace(all_records=[]),
        _rust_conversion_run=SimpleNamespace(id=42),
        _record_type_signature=lambda record_type: record_type,
    )

    rows = weapon_metadata_rows(orchestrator)

    assert calls == [(42, [])]
    assert [row["editor_id"] for row in rows] == ["A", "B"]


def test_melee_only_asset_filter_admits_only_authoritative_weapon_closure() -> None:
    from bacup_lib.workflows.unified import (
        _is_selected_mvp_asset,
        _mvp_asset_closure_set,
    )

    weapon = _asset("nif", r"Weapons\Melee\Axe.nif")
    unrelated_world = _asset(
        "nif",
        r"Landscape\Trees\TreePineForest01.nif",
        owner="000200@FalloutNV.esm",
        signature="STAT",
    )
    plan = build_weapon_mvp_asset_plan(
        "fnv",
        "fo4",
        [weapon, unrelated_world],
        [_receipt("000100@FalloutNV.esm", "Axe", r"Weapons\Melee\Axe.nif")],
    )
    driver = SimpleNamespace(
        _req=SimpleNamespace(options=SimpleNamespace(mvp_melee_only=True)),
        ctx=SimpleNamespace(_weapon_mvp_asset_plan=plan),
    )
    closure = _mvp_asset_closure_set(driver, [weapon, unrelated_world])

    assert _is_selected_mvp_asset(driver, weapon, frozenset({"WEAP"}), closure)
    assert not _is_selected_mvp_asset(
        driver, unrelated_world, frozenset({"WEAP"}), closure
    )
