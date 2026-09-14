from __future__ import annotations

from dataclasses import replace
from types import SimpleNamespace

import pytest

from bacup_lib.models import PluginPortOptions, PluginPortRequest
from bacup_lib.source_pairs import (
    FNV_MVP_EXCLUDE_SIGNATURES,
    FNV_QUEST_SLICE_EXCLUDE_SIGNATURES,
    MvpMeleePolicy,
    MvpRecordException,
    SKYRIM_MVP_EXCLUDE_SIGNATURES,
    get_pair,
    is_mvp_record_exception,
    mvp_creature_corpus_policy_payload,
    mvp_creature_profile_payload,
    mvp_melee_policy_payload,
    mvp_record_exception_payload,
    serialize_mvp_melee_policy,
)
from bacup_lib.workflows.unified import AssetRuns, AssetWaveToggles, _UnifiedRecordRuntime


@pytest.mark.parametrize(
    ("pair_id", "exclusions", "expected"),
    [
        (
            "skyrimse:fo4",
            SKYRIM_MVP_EXCLUDE_SIGNATURES,
            {
                "policy_id": "bulk_melee_v1",
                "pair_id": "skyrimse:fo4",
                "signature": "WEAP",
                "source_games": ["skyrimse"],
                "source_plugins": [
                    "Skyrim.esm",
                    "Update.esm",
                    "Dawnguard.esm",
                    "HearthFires.esm",
                    "Dragonborn.esm",
                ],
                "allowed_raw_animation_types": [0, 1, 2, 3, 4, 5, 6],
                "include_model_less_unarmed": True,
                "provenance_policy": "official_source_plugins",
            },
        ),
        (
            "fnvfo3:fo4",
            FNV_MVP_EXCLUDE_SIGNATURES,
            {
                "policy_id": "bulk_melee_v1",
                "pair_id": "fnvfo3:fo4",
                "signature": "WEAP",
                "source_games": ["fnv", "fo3"],
                "source_plugins": [
                    "FalloutNV.esm",
                    "DeadMoney.esm",
                    "HonestHearts.esm",
                    "OldWorldBlues.esm",
                    "LonesomeRoad.esm",
                    "GunRunnersArsenal.esm",
                    "CaravanPack.esm",
                    "ClassicPack.esm",
                    "MercenaryPack.esm",
                    "TribalPack.esm",
                    "Fallout3.esm",
                    "Anchorage.esm",
                    "ThePitt.esm",
                    "BrokenSteel.esm",
                    "PointLookout.esm",
                    "Zeta.esm",
                ],
                "allowed_raw_animation_types": [0, 1, 2],
                "include_model_less_unarmed": True,
                "provenance_policy": "official_source_and_graft",
            },
        ),
    ],
)
def test_complete_pair_mvp_fence_selects_bulk_melee_policy(
    pair_id: str,
    exclusions: frozenset[str],
    expected: dict[str, object],
) -> None:
    assert mvp_melee_policy_payload(pair_id, exclusions) == expected
    assert mvp_record_exception_payload(pair_id, exclusions) == []


@pytest.mark.parametrize(
    ("pair_id", "full_fence"),
    [
        ("skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES),
        ("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES),
    ],
)
def test_bulk_melee_policy_requires_the_complete_mvp_fence(
    pair_id: str,
    full_fence: frozenset[str],
) -> None:
    assert mvp_melee_policy_payload(pair_id, frozenset()) is None
    assert mvp_melee_policy_payload(pair_id, full_fence - {"WEAP"}) is None
    assert mvp_melee_policy_payload(pair_id, full_fence - {"RACE"}) is None


def test_bulk_melee_policies_exclude_ranged_animation_enums() -> None:
    skyrim = mvp_melee_policy_payload(
        "skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES
    )
    fnv = mvp_melee_policy_payload("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES)

    assert skyrim is not None
    assert fnv is not None
    assert set(skyrim["allowed_raw_animation_types"]) == set(range(7))
    assert {7, 8, 9}.isdisjoint(skyrim["allowed_raw_animation_types"])
    assert set(fnv["allowed_raw_animation_types"]) == {0, 1, 2}
    assert set(range(3, 14)).isdisjoint(fnv["allowed_raw_animation_types"])


def test_fnv_bulk_policy_carries_official_fonv_and_grafted_fo3_provenance() -> None:
    policy = mvp_melee_policy_payload("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES)

    assert policy is not None
    assert policy["source_games"] == ["fnv", "fo3"]
    assert {
        "FalloutNV.esm",
        "DeadMoney.esm",
        "TribalPack.esm",
        "Fallout3.esm",
        "Anchorage.esm",
        "Zeta.esm",
    } <= set(policy["source_plugins"])
    assert policy["provenance_policy"] == "official_source_and_graft"
    assert policy["include_model_less_unarmed"] is True


@pytest.mark.parametrize(
    "policy",
    [
        replace(
            get_pair("skyrimse:fo4").mvp_melee_policy,
            pair_id="fnvfo3:fo4",
        ),
        replace(
            get_pair("skyrimse:fo4").mvp_melee_policy,
            signature="STAT",
        ),
        replace(
            get_pair("skyrimse:fo4").mvp_melee_policy,
            allowed_raw_animation_types=(0, 1, 2, 3, 4, 5, 6, 7),
        ),
        replace(
            get_pair("skyrimse:fo4").mvp_melee_policy,
            allowed_raw_animation_types=(-1, 0, 1, 2, 3, 4, 5, 6),
        ),
        replace(
            get_pair("skyrimse:fo4").mvp_melee_policy,
            allowed_raw_animation_types=(0, 1, 2, 3, 4, 5, 256),
        ),
        replace(
            get_pair("skyrimse:fo4").mvp_melee_policy,
            source_plugins=("Skyrim.esm",),
        ),
        replace(
            get_pair("skyrimse:fo4").mvp_melee_policy,
            include_model_less_unarmed=False,
        ),
    ],
)
def test_malformed_or_wrong_pair_bulk_melee_policy_fails_closed(
    policy: MvpMeleePolicy,
) -> None:
    assert serialize_mvp_melee_policy("skyrimse:fo4", policy) is None


@pytest.mark.parametrize(
    ("pair_id", "exclusions", "profile"),
    [
        ("skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES, "skyrim_wolf"),
        ("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES, "fnv_gecko"),
    ],
)
def test_complete_pair_mvp_fence_selects_exact_creature_profile(
    pair_id: str,
    exclusions: frozenset[str],
    profile: str,
) -> None:
    assert mvp_creature_profile_payload(pair_id, exclusions) == profile
    assert mvp_creature_profile_payload(pair_id, exclusions - {"RACE"}) is None


@pytest.mark.parametrize(
    ("pair_id", "exclusions", "source_games"),
    [
        ("skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES, ["skyrimse"]),
        ("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES, ["fnv", "fo3"]),
    ],
)
def test_complete_pair_mvp_fence_selects_all_creatures_corpus_policy(
    pair_id: str,
    exclusions: frozenset[str],
    source_games: list[str],
) -> None:
    policy = mvp_creature_corpus_policy_payload(pair_id, exclusions)

    assert policy is not None
    assert policy["policy_id"] == "all_creatures_v1"
    assert policy["source_games"] == source_games
    assert policy["execution_mode"] == "strict"
    assert policy["debug_dir"] == "debug/creature_corpus"
    assert "prepared_jobs_path" not in policy
    assert mvp_creature_corpus_policy_payload(pair_id, exclusions - {"RACE"}) is None


def test_legacy_exact_rows_remain_regression_fixtures_only() -> None:
    assert [
        row.to_config() for row in get_pair("skyrimse:fo4").mvp_record_exceptions
    ] == [
        {
            "signature": "WEAP",
            "local_form_id": 0x013984,
            "source_plugin": "Skyrim.esm",
        },
        {
            "signature": "STAT",
            "local_form_id": 0x020E27,
            "source_plugin": "Skyrim.esm",
        },
    ]
    assert not is_mvp_record_exception(
        "skyrimse:fo4",
        SKYRIM_MVP_EXCLUDE_SIGNATURES,
        signature="WEAP",
        local_form_id=0x013984,
        source_plugin="Skyrim.esm",
    )


@pytest.mark.parametrize("local_form_id", [-1, 0x01000000, 0x01013984])
def test_exception_config_rejects_out_of_range_local_form_ids(
    local_form_id: int,
) -> None:
    with pytest.raises(ValueError, match="outside the 24-bit range"):
        MvpRecordException("WEAP", local_form_id, "Skyrim.esm").to_config()


@pytest.mark.parametrize("local_form_id", [-1, 0x01013984])
def test_exact_record_matching_rejects_out_of_range_local_form_ids(
    local_form_id: int,
) -> None:
    assert not is_mvp_record_exception(
        "skyrimse:fo4",
        SKYRIM_MVP_EXCLUDE_SIGNATURES,
        signature="WEAP",
        local_form_id=local_form_id,
        source_plugin="Skyrim.esm",
    )


@pytest.mark.parametrize(
    ("pair_id", "exclusions", "signature", "local_form_id", "source_plugin"),
    [
        ("skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES, "WEAP", 0x09F25F, "Skyrim.esm"),
        ("skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES, "WEAP", 0x013985, "Skyrim.esm"),
        ("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES, "WEAP", 0x14DE1D, "FalloutNV.esm"),
        ("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES, "WEAP", 0x11A8E4, "DeadMoney.esm"),
    ],
)
def test_other_weapons_and_wrong_plugin_ownership_remain_excluded(
    pair_id: str,
    exclusions: frozenset[str],
    signature: str,
    local_form_id: int,
    source_plugin: str,
) -> None:
    assert not is_mvp_record_exception(
        pair_id,
        exclusions,
        signature=signature,
        local_form_id=local_form_id,
        source_plugin=source_plugin,
    )


@pytest.mark.parametrize(
    ("pair_id", "exclusions"),
    [
        ("skyrimse:fo4", frozenset()),
        ("fnvfo3:fo4", frozenset()),
        ("skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES - {"WEAP"}),
        ("fnvfo3:fo4", FNV_MVP_EXCLUDE_SIGNATURES - {"WEAP"}),
    ],
)
def test_non_mvp_and_partial_fences_emit_no_record_exceptions(
    pair_id: str,
    exclusions: frozenset[str],
) -> None:
    assert mvp_record_exception_payload(pair_id, exclusions) == []


@pytest.mark.parametrize(
    ("source_game", "pair_exclusions", "expected_plugin"),
    [
        ("skyrimse", SKYRIM_MVP_EXCLUDE_SIGNATURES, "Skyrim.esm"),
        ("fnv", FNV_MVP_EXCLUDE_SIGNATURES, "FalloutNV.esm"),
    ],
)
def test_native_run_config_forwards_pair_scoped_mvp_exceptions(
    tmp_path,
    monkeypatch,
    source_game: str,
    pair_exclusions: frozenset[str],
    expected_plugin: str,
) -> None:
    request = PluginPortRequest(
        source_game=source_game,
        target_game="fo4",
        source_plugins=[tmp_path / expected_plugin],
        output_root=tmp_path,
        options=PluginPortOptions(exclude_signatures=pair_exclusions),
    )
    runtime = _UnifiedRecordRuntime(request)
    monkeypatch.setattr(runtime, "_legacy_music_track_manifest", lambda _ctx: [])
    ctx = SimpleNamespace(
        source_data_dir=tmp_path,
        target_extracted_dir=tmp_path,
        target_data_dir=tmp_path,
        output_plugin_name=expected_plugin,
        is_whole_plugin=True,
    )
    config = runtime._native_run_config(ctx)

    assert "WEAP" in config["skip_record_signatures"]
    assert config["mvp_record_exceptions"] == mvp_record_exception_payload(
        "skyrimse:fo4" if source_game == "skyrimse" else "fnvfo3:fo4",
        pair_exclusions,
    )
    assert ctx.mvp_creature_profile is None
    assert ctx.mvp_creature_corpus_policy["policy_id"] == "all_creatures_v1"
    assert ctx.mvp_melee_policy["policy_id"] == "bulk_melee_v1"


def test_asset_runs_inherit_complete_creature_mvp_gate(monkeypatch, tmp_path) -> None:
    captured = []

    def create_new(source, target, source_plugin, output_plugin, *, config):
        captured.append((source, target, source_plugin, output_plugin, config))
        return SimpleNamespace(id=1)

    monkeypatch.setattr(
        "bacup_lib.run.ConversionRun.create_new",
        create_new,
    )
    ctx = SimpleNamespace(
        source_game="skyrimse",
        target_game="fo4",
        output_plugin_name="Skyrim_Merged.esm",
        mvp_creature_profile="skyrim_wolf",
        mvp_record_exceptions=mvp_record_exception_payload(
            "skyrimse:fo4", SKYRIM_MVP_EXCLUDE_SIGNATURES
        ),
        mod_path=tmp_path,
    )
    AssetRuns(
        ctx,
        AssetWaveToggles(
            nifs=False,
            btos=False,
            textures=False,
            materials=False,
            havok=True,
            drivers=False,
            sounds=False,
            animations=False,
        ),
    )

    assert len(captured) == 1
    config = captured[0][4]
    assert config["skip_record_signatures"] == ["WEAP"]
    assert config["mvp_record_exceptions"] == ctx.mvp_record_exceptions


def test_fnv_quest_slice_does_not_inherit_weapon_mvp_exceptions(
    tmp_path,
    monkeypatch,
) -> None:
    request = PluginPortRequest(
        source_game="fnv",
        target_game="fo4",
        source_plugins=[tmp_path / "FalloutNV.esm"],
        output_root=tmp_path,
        options=PluginPortOptions(
            exclude_signatures=FNV_QUEST_SLICE_EXCLUDE_SIGNATURES,
            fnv_quest_slice=True,
        ),
    )
    runtime = _UnifiedRecordRuntime(request)
    monkeypatch.setattr(runtime, "_legacy_music_track_manifest", lambda _ctx: [])
    config = runtime._native_run_config(
        SimpleNamespace(
            source_data_dir=tmp_path,
            target_extracted_dir=tmp_path,
            target_data_dir=tmp_path,
            output_plugin_name="FalloutNV.esm",
            is_whole_plugin=True,
        )
    )

    assert "WEAP" in config["skip_record_signatures"]
    assert config["mvp_record_exceptions"] == []
