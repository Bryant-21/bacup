"""Supported source/target game pairs for the conversion driver."""

from __future__ import annotations

from dataclasses import dataclass

from creation_lib.core.game_profiles import get_profile


@dataclass(frozen=True)
class MergeStageSpec:
    grafted_game: str
    grafted_plugins: tuple[str, ...]
    output_name: str
    grafted_data_env: str
    grafted_dir_env: str
    grafted_extracted_env: str


@dataclass(frozen=True)
class MvpRecordException:
    signature: str
    local_form_id: int
    source_plugin: str

    def to_config(self) -> dict[str, str | int]:
        if not 0 <= self.local_form_id <= 0x00FF_FFFF:
            raise ValueError(
                f"MVP record local FormID is outside the 24-bit range: "
                f"{self.local_form_id}"
            )
        return {
            "signature": self.signature.upper(),
            "local_form_id": self.local_form_id,
            "source_plugin": self.source_plugin,
        }


@dataclass(frozen=True)
class MvpMeleePolicy:
    policy_id: str
    pair_id: str
    signature: str
    source_games: tuple[str, ...]
    source_plugins: tuple[str, ...]
    allowed_raw_animation_types: tuple[int, ...]
    include_model_less_unarmed: bool
    provenance_policy: str


@dataclass(frozen=True)
class MvpCreatureCorpusPolicy:
    policy_id: str
    pair_id: str
    source_games: tuple[str, ...]
    source_plugins: tuple[str, ...]
    execution_mode: str
    debug_dir: str


@dataclass(frozen=True)
class SourcePair:
    pair_id: str
    source_game: str
    target_game: str
    source_plugins: tuple[str, ...]
    output_mod_name: str
    source_extracted_env: str
    source_data_env: str
    source_dir_env: str
    merge: MergeStageSpec | None
    engine: str
    optional_source_plugins: tuple[str, ...] = ()
    mvp_record_exceptions: tuple[MvpRecordException, ...] = ()
    mvp_melee_policy: MvpMeleePolicy | None = None
    mvp_creature_corpus_policy: MvpCreatureCorpusPolicy | None = None
    mvp_creature_profile: str | None = None

    @property
    def output_plugin_name(self) -> str:
        return self.merge.output_name if self.merge is not None else self.source_plugins[0]


DEFAULT_PAIR_ID = "fo76:fo4"

SKYRIM_MVP_EXCLUDE_SIGNATURES = frozenset(
    {
        "ACHR",
        "AMMO",
        "BPTD",
        "CLFM",
        "CSTY",
        "DIAL",
        "DLBR",
        "DLVW",
        "ENCH",
        "EYES",
        "HDPT",
        "INFO",
        "LVLN",
        "LSCR",
        "MOVT",
        "NPC_",
        "PACK",
        "PERK",
        "PROJ",
        "QUST",
        "RACE",
        "SCEN",
        "SMBN",
        "SMEN",
        "SMQN",
        "SPEL",
        "WEAP",
    }
)

FNV_MVP_EXCLUDE_SIGNATURES = frozenset(
    {
        "ACRE",
        "AMMO",
        "BPTD",
        "CREA",
        "CSTY",
        "DIAL",
        "ENCH",
        "EYES",
        "HAIR",
        "HDPT",
        "INFO",
        "LVLC",
        "PACK",
        "PERK",
        "PROJ",
        "QUST",
        "RACE",
        "SCEN",
        "SPEL",
        "WEAP",
    }
)

# Normal FNV/FO3 conversion now admits all non-quest record families through
# their pair lowerers or target-native replacements.  The remaining gate is
# intentionally limited to the quest runtime families whose corpus-wide
# producer, script, and topology contracts are not yet complete.  Keep the
# older world-only set above for explicit MVP/fixture modes only.
FNV_REQUIRED_EXCLUDE_SIGNATURES = frozenset({"DIAL", "INFO", "QUST", "SCEN"})

# This is intentionally a small, explicit expansion of the world-only FNV
# MVP.  It enables the record families required by the quest vertical slice
# while retaining PACK and the unrelated combat/animation families behind the
# world-only gate.  It is not a declaration that the complete FNV corpus is
# safe to convert.
FNV_QUEST_SLICE_ENABLED_SIGNATURES = frozenset(
    {
        "CREA",
        "DIAL",
        "ENCH",
        "INFO",
        "PERK",
        "QUST",
        "SCEN",
        "SPEL",
    }
)

FNV_QUEST_SLICE_EXCLUDE_SIGNATURES = frozenset(
    FNV_MVP_EXCLUDE_SIGNATURES - FNV_QUEST_SLICE_ENABLED_SIGNATURES
)

# The opt-in signature set above is only the command-line mode selector.  The
# native run keeps the full world-only exclusion fence and admits only these
# audited records through the quest-slice bypass.
FNV_QUEST_SLICE_RECORD_FORM_IDS: dict[str, tuple[int, ...]] = {
    "ACHR": (0x12319B, 0x12319C, 0x134B9C),
    "ACTI": (0x133F41,),
    "DIAL": (0x13015B, 0x134B9A, 0x138A74),
    "INFO": (0x130161, 0x134B9B, 0x15734B, 0x15734C, 0x15734D),
    "MGEF": (0x0CB05D,),
    "PACK": (0x1231B6, 0x1231B7, 0x13289E, 0x133F3E),
    "PERK": (0x058FDF,),
    "QUST": (0x06136D, 0x11F935),
    "REFR": (0x133F42,),
    "SCPT": (0x11FC64, 0x123191, 0x134491, 0x166305),
    "SPEL": (0x172091,),
}


def fnv_quest_slice_record_form_ids() -> dict[str, list[int]]:
    """Return a mutable JSON-ready copy of the audited quest dependency closure."""
    return {
        signature: list(form_ids)
        for signature, form_ids in FNV_QUEST_SLICE_RECORD_FORM_IDS.items()
    }

FO4_MVP_EXCLUDE_SIGNATURES = frozenset(
    {
        "ACHR",
        "AMMO",
        "ARMA",
        "ARMO",
        "BPTD",
        "CLFM",
        "CSTY",
        "DIAL",
        "DLBR",
        "DLVW",
        "ENCH",
        "EYES",
        "FURN",
        "HDPT",
        "INFO",
        "INNR",
        "LSCR",
        "LVLN",
        "MOVT",
        "NPC_",
        "OMOD",
        "PACK",
        "PERK",
        "PROJ",
        "QUST",
        "RACE",
        "SCEN",
        "SMBN",
        "SMEN",
        "SMQN",
        "SPEL",
        "TERM",
        "WEAP",
    }
)

STARFIELD_MVP_EXCLUDE_SIGNATURES = frozenset(
    {
        # actor / quest / combat fence (Skyrim MVP model)
        "ACHR", "AMMO", "ARMA", "ARMO", "AVIF", "BPTD", "CLFM", "CSTY",
        "DIAL", "DLBR", "DLVW", "ENCH", "HDPT", "INFO", "LVLN",
        "LSCR", "MOVT", "NPC_", "PACK", "PERK", "PROJ", "QUST", "RACE",
        "SCEN", "SMBN", "SMEN", "SMQN", "SPEL", "WEAP",
        # Item-modification and crafting layer. These exist only to modify or
        # craft the WEAP/ARMO/NPC_ above, so the fence orphans them: sampled
        # OMOD target TESObjectWEAP/TESNPC/TESObjectARMOR instance data (plus
        # Starfield-only Spaceship), and most COBJ arrive with no CreatedObject
        # at all. Their payloads also stay in Starfield's native property-blob
        # form, which FO4 reads as its own layout — BGSAttachParentArray::Load
        # walks the blob and access-violates during startup form load.
        "COBJ", "OMOD",
        # Starfield-novel planet/space/procgen (no FO4 analogue)
        "BIOM", "PNDT", "STDT", "PCBN", "PCCN", "PCMT", "GBFM", "GBFT",
        "SFBK", "SFPC", "SFPT", "SFTR", "AOPF", "AOPS", "AORU", "WBAR",
        "WKMF", "LMSW", "IRES", "SUNP", "ATMO", "CLDF", "FOGV",
        # WWise-only audio
        "WWED", "AMBS",
    }
)

MVP_EXCLUDE_SIGNATURES_BY_PAIR = {
    "fnvfo3:fo4": FNV_MVP_EXCLUDE_SIGNATURES,
    "skyrimse:fo4": SKYRIM_MVP_EXCLUDE_SIGNATURES,
    "fo4:starfield": FO4_MVP_EXCLUDE_SIGNATURES,
    "starfield:fo4": STARFIELD_MVP_EXCLUDE_SIGNATURES,
}

# FNV/FO3 normal conversion retains only its quest-runtime gate. Skyrim
# gameplay is admitted by native capability planning; its world-only fence is
# available only through explicit MVP mode. fo4:starfield and starfield:fo4
# always convert world-only (exteriors and interiors; no quests, NPCs, or
# actors); starfield:fo4 also fences Starfield-novel planet/space/procgen
# records and WWise-only audio.
MVP_REQUIRED_PAIRS = frozenset(
    {"fo4:starfield", "starfield:fo4"}
)


def required_exclude_signatures(pair_id: str) -> frozenset[str]:
    """Signatures a pair must exclude for its conversion to complete."""
    if pair_id == "fnvfo3:fo4":
        return FNV_REQUIRED_EXCLUDE_SIGNATURES
    if pair_id not in MVP_REQUIRED_PAIRS:
        return frozenset()
    return MVP_EXCLUDE_SIGNATURES_BY_PAIR.get(pair_id, frozenset())


def quest_slice_exclude_signatures(pair_id: str) -> frozenset[str]:
    """Return the explicit safe remainder for a supported quest slice.

    The caller must opt in to this mode.  Keeping ``PACK`` in the returned
    exclusions prevents the slice from turning on every legacy AI package.
    """
    if pair_id == "fnvfo3:fo4":
        return FNV_QUEST_SLICE_EXCLUDE_SIGNATURES
    raise ValueError(f"{pair_id!r} has no supported quest-slice contract")


def is_fnv_quest_slice_exclusion_set(signatures: frozenset[str]) -> bool:
    """Whether an option set explicitly selects the FNV quest-slice gate."""
    normalized = frozenset(signature.upper() for signature in signatures)
    return (
        FNV_QUEST_SLICE_EXCLUDE_SIGNATURES <= normalized
        and not (FNV_QUEST_SLICE_ENABLED_SIGNATURES & normalized)
    )


def mvp_record_exception_payload(
    pair_id: str,
    exclude_signatures: frozenset[str],
) -> list[dict[str, str | int]]:
    """Return legacy exact fixtures only when no bulk melee policy supersedes them."""
    expected = MVP_EXCLUDE_SIGNATURES_BY_PAIR.get(pair_id, frozenset())
    normalized_excludes = frozenset(
        str(signature).strip().upper() for signature in exclude_signatures
    )
    if not expected or not expected <= normalized_excludes:
        return []
    pair = SOURCE_PAIRS[pair_id]
    if pair.mvp_melee_policy is not None:
        return []
    return [exception.to_config() for exception in pair.mvp_record_exceptions]


_MVP_MELEE_ANIMATION_TYPES_BY_PAIR = {
    "skyrimse:fo4": tuple(range(7)),
    "fnvfo3:fo4": (0, 1, 2),
}


def serialize_mvp_melee_policy(
    pair_id: str,
    policy: MvpMeleePolicy,
) -> dict[str, object] | None:
    """Validate a pair-owned bulk melee policy and return its native-ready shape."""
    pair = SOURCE_PAIRS.get(pair_id)
    expected_animation_types = _MVP_MELEE_ANIMATION_TYPES_BY_PAIR.get(pair_id)
    if pair is None or expected_animation_types is None:
        return None
    if (
        not isinstance(policy.policy_id, str)
        or not isinstance(policy.pair_id, str)
        or not isinstance(policy.signature, str)
        or not isinstance(policy.source_games, tuple)
        or any(not isinstance(game, str) for game in policy.source_games)
        or not isinstance(policy.source_plugins, tuple)
        or any(not isinstance(plugin, str) for plugin in policy.source_plugins)
        or not isinstance(policy.allowed_raw_animation_types, tuple)
        or any(
            isinstance(value, bool) or not isinstance(value, int)
            for value in policy.allowed_raw_animation_types
        )
        or not isinstance(policy.include_model_less_unarmed, bool)
        or not isinstance(policy.provenance_policy, str)
    ):
        return None
    source_games = [pair.source_game]
    if pair.merge is not None and pair.merge.grafted_game not in source_games:
        source_games.append(pair.merge.grafted_game)
    source_plugins = list(pair.source_plugins) + list(pair.optional_source_plugins)
    if pair.merge is not None:
        source_plugins.extend(pair.merge.grafted_plugins)
    expected_provenance = (
        "official_source_and_graft"
        if pair.merge is not None and pair.merge.grafted_plugins
        else "official_source_plugins"
    )
    normalized_plugins = [plugin.casefold() for plugin in policy.source_plugins]
    if (
        policy.policy_id != "bulk_melee_v1"
        or policy.pair_id != pair_id
        or policy.signature != "WEAP"
        or policy.source_games != tuple(source_games)
        or policy.source_plugins != tuple(source_plugins)
        or len(normalized_plugins) != len(set(normalized_plugins))
        or any(not plugin.strip() for plugin in policy.source_plugins)
        or policy.allowed_raw_animation_types != expected_animation_types
        or any(
            not 0 <= value <= 0xFF
            for value in policy.allowed_raw_animation_types
        )
        or policy.include_model_less_unarmed is not True
        or policy.provenance_policy != expected_provenance
    ):
        return None
    return {
        "policy_id": policy.policy_id,
        "pair_id": policy.pair_id,
        "signature": policy.signature,
        "source_games": list(policy.source_games),
        "source_plugins": list(policy.source_plugins),
        "allowed_raw_animation_types": list(policy.allowed_raw_animation_types),
        "include_model_less_unarmed": policy.include_model_less_unarmed,
        "provenance_policy": policy.provenance_policy,
    }


def mvp_melee_policy_payload(
    pair_id: str,
    exclude_signatures: frozenset[str],
) -> dict[str, object] | None:
    """Return the validated bulk melee policy only for a complete MVP fence."""
    expected = MVP_EXCLUDE_SIGNATURES_BY_PAIR.get(pair_id, frozenset())
    normalized_excludes = frozenset(
        str(signature).strip().upper() for signature in exclude_signatures
    )
    if not expected or not expected <= normalized_excludes:
        return None
    pair = SOURCE_PAIRS.get(pair_id)
    if pair is None or pair.mvp_melee_policy is None:
        return None
    return serialize_mvp_melee_policy(pair_id, pair.mvp_melee_policy)


def mvp_creature_profile_payload(
    pair_id: str,
    exclude_signatures: frozenset[str],
) -> str | None:
    """Return the pair's exact creature slice only for its complete MVP fence."""
    expected = MVP_EXCLUDE_SIGNATURES_BY_PAIR.get(pair_id, frozenset())
    normalized_excludes = frozenset(
        str(signature).strip().upper() for signature in exclude_signatures
    )
    if not expected or not expected <= normalized_excludes:
        return None
    return SOURCE_PAIRS[pair_id].mvp_creature_profile


def mvp_creature_corpus_policy_payload(
    pair_id: str,
    exclude_signatures: frozenset[str],
) -> dict[str, object] | None:
    expected = MVP_EXCLUDE_SIGNATURES_BY_PAIR.get(pair_id, frozenset())
    normalized_excludes = frozenset(
        str(signature).strip().upper() for signature in exclude_signatures
    )
    if not expected or not expected <= normalized_excludes:
        return None
    pair = SOURCE_PAIRS.get(pair_id)
    if pair is None or pair.mvp_creature_corpus_policy is None:
        return None
    policy = pair.mvp_creature_corpus_policy
    source_games = [pair.source_game]
    if pair.merge is not None and pair.merge.grafted_game not in source_games:
        source_games.append(pair.merge.grafted_game)
    source_plugins = list(pair.source_plugins) + list(pair.optional_source_plugins)
    if pair.merge is not None:
        source_plugins.extend(pair.merge.grafted_plugins)
    if (
        policy.policy_id != "all_creatures_v1"
        or policy.pair_id != pair_id
        or policy.source_games != tuple(source_games)
        or policy.source_plugins != tuple(source_plugins)
        or policy.execution_mode != "strict"
        or policy.debug_dir != "debug/creature_corpus"
    ):
        return None
    return {
        "policy_id": policy.policy_id,
        "pair_id": policy.pair_id,
        "source_games": list(policy.source_games),
        "source_plugins": list(policy.source_plugins),
        "execution_mode": policy.execution_mode,
        "debug_dir": policy.debug_dir,
    }


def is_mvp_record_exception(
    pair_id: str,
    exclude_signatures: frozenset[str],
    *,
    signature: str,
    local_form_id: int,
    source_plugin: str,
) -> bool:
    normalized_signature = signature.strip().upper()
    if not 0 <= local_form_id <= 0x00FF_FFFF:
        return False
    normalized_plugin = source_plugin.strip().casefold()
    return any(
        row["signature"] == normalized_signature
        and row["local_form_id"] == local_form_id
        and str(row["source_plugin"]).casefold() == normalized_plugin
        for row in mvp_record_exception_payload(pair_id, exclude_signatures)
    )


SOURCE_PAIRS: dict[str, SourcePair] = {
    "fo76:fo4": SourcePair(
        pair_id="fo76:fo4",
        source_game="fo76",
        target_game="fo4",
        source_plugins=("SeventySix.esm",),
        output_mod_name="SeventySix",
        source_extracted_env=get_profile("fo76").env_var_name,
        source_data_env="FO76_DATA_DIR",
        source_dir_env="FO76_DIR",
        merge=None,
        engine=get_profile("fo76").engine,
    ),
    "fnvfo3:fo4": SourcePair(
        pair_id="fnvfo3:fo4",
        source_game="fnv",
        target_game="fo4",
        source_plugins=(
            "FalloutNV.esm",
            "DeadMoney.esm",
            "HonestHearts.esm",
            "OldWorldBlues.esm",
            "LonesomeRoad.esm",
            "GunRunnersArsenal.esm",
        ),
        optional_source_plugins=(
            "CaravanPack.esm",
            "ClassicPack.esm",
            "MercenaryPack.esm",
            "TribalPack.esm",
        ),
        output_mod_name="FNV_FO3",
        source_extracted_env=get_profile("fnv").env_var_name,
        source_data_env="FONV_DATA_DIR",
        source_dir_env="FONV_DIR",
        merge=MergeStageSpec(
            grafted_game="fo3",
            grafted_plugins=(
                "Fallout3.esm",
                "Anchorage.esm",
                "ThePitt.esm",
                "BrokenSteel.esm",
                "PointLookout.esm",
                "Zeta.esm",
            ),
            output_name="FalloutNV.esm",
            grafted_data_env="FO3_DATA_DIR",
            grafted_dir_env="FO3_DIR",
            grafted_extracted_env=get_profile("fo3").env_var_name,
        ),
        engine=get_profile("fnv").engine,
        mvp_record_exceptions=(
            MvpRecordException("WEAP", 0x11A8E4, "FalloutNV.esm"),
            MvpRecordException("STAT", 0x11A8E3, "FalloutNV.esm"),
        ),
        mvp_melee_policy=MvpMeleePolicy(
            policy_id="bulk_melee_v1",
            pair_id="fnvfo3:fo4",
            signature="WEAP",
            source_games=("fnv", "fo3"),
            source_plugins=(
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
            ),
            allowed_raw_animation_types=(0, 1, 2),
            include_model_less_unarmed=True,
            provenance_policy="official_source_and_graft",
        ),
        mvp_creature_corpus_policy=MvpCreatureCorpusPolicy(
            policy_id="all_creatures_v1",
            pair_id="fnvfo3:fo4",
            source_games=("fnv", "fo3"),
            source_plugins=(
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
            ),
            execution_mode="strict",
            debug_dir="debug/creature_corpus",
        ),
        mvp_creature_profile="fnv_gecko",
    ),
    "skyrimse:fo4": SourcePair(
        pair_id="skyrimse:fo4",
        source_game="skyrimse",
        target_game="fo4",
        source_plugins=(
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
        ),
        output_mod_name="Skyrim",
        source_extracted_env=get_profile("skyrimse").env_var_name,
        source_data_env="SKYRIMSE_DATA_DIR",
        source_dir_env="SKYRIMSE_DIR",
        merge=MergeStageSpec(
            grafted_game="skyrimse",
            grafted_plugins=(),
            output_name="Skyrim.esm",
            grafted_data_env="SKYRIMSE_DATA_DIR",
            grafted_dir_env="SKYRIMSE_DIR",
            grafted_extracted_env=get_profile("skyrimse").env_var_name,
        ),
        engine=get_profile("skyrimse").engine,
        mvp_record_exceptions=(
            MvpRecordException("WEAP", 0x013984, "Skyrim.esm"),
            MvpRecordException("STAT", 0x020E27, "Skyrim.esm"),
        ),
        mvp_melee_policy=MvpMeleePolicy(
            policy_id="bulk_melee_v1",
            pair_id="skyrimse:fo4",
            signature="WEAP",
            source_games=("skyrimse",),
            source_plugins=(
                "Skyrim.esm",
                "Update.esm",
                "Dawnguard.esm",
                "HearthFires.esm",
                "Dragonborn.esm",
            ),
            allowed_raw_animation_types=(0, 1, 2, 3, 4, 5, 6),
            include_model_less_unarmed=True,
            provenance_policy="official_source_plugins",
        ),
        mvp_creature_corpus_policy=MvpCreatureCorpusPolicy(
            policy_id="all_creatures_v1",
            pair_id="skyrimse:fo4",
            source_games=("skyrimse",),
            source_plugins=(
                "Skyrim.esm",
                "Update.esm",
                "Dawnguard.esm",
                "HearthFires.esm",
                "Dragonborn.esm",
            ),
            execution_mode="strict",
            debug_dir="debug/creature_corpus",
        ),
        mvp_creature_profile="skyrim_wolf",
    ),
    "fo4:starfield": SourcePair(
        pair_id="fo4:starfield",
        source_game="fo4",
        target_game="starfield",
        source_plugins=("Fallout4.esm",),
        output_mod_name="Fallout4_SF",
        source_extracted_env=get_profile("fo4").env_var_name,
        source_data_env="FO4_DATA_DIR",
        source_dir_env="FO4_DIR",
        merge=MergeStageSpec(
            grafted_game="fo4",
            grafted_plugins=(),
            output_name="Fallout4_SF.esm",
            grafted_data_env="FO4_DATA_DIR",
            grafted_dir_env="FO4_DIR",
            grafted_extracted_env=get_profile("fo4").env_var_name,
        ),
        engine=get_profile("fo4").engine,
    ),
    "starfield:fo4": SourcePair(
        pair_id="starfield:fo4",
        source_game="starfield",
        target_game="fo4",
        source_plugins=("Starfield.esm",),
        output_mod_name="Starfield",
        source_extracted_env=get_profile("starfield").env_var_name,
        source_data_env="STARFIELD_DATA_DIR",
        source_dir_env="STARFIELD_DIR",
        merge=MergeStageSpec(
            grafted_game="starfield",
            grafted_plugins=(),
            output_name="Starfield.esm",
            grafted_data_env="STARFIELD_DATA_DIR",
            grafted_dir_env="STARFIELD_DIR",
            grafted_extracted_env=get_profile("starfield").env_var_name,
        ),
        engine=get_profile("starfield").engine,
    ),
}


def get_pair(pair_id: str) -> SourcePair:
    try:
        return SOURCE_PAIRS[pair_id]
    except KeyError:
        available = ", ".join(sorted(SOURCE_PAIRS))
        raise KeyError(
            f"Unknown source pair {pair_id!r}; available pairs: {available}"
        ) from None
