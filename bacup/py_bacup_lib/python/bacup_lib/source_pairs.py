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

    @property
    def output_plugin_name(self) -> str:
        return self.merge.output_name if self.merge is not None else self.source_plugins[0]


DEFAULT_PAIR_ID = "fo76:fo4"

SKYRIM_MVP_EXCLUDE_SIGNATURES = frozenset(
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
        "ACHR",
        "ACRE",
        "AMMO",
        "ARMA",
        "ARMO",
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
        "LVLN",
        "NPC_",
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

MVP_EXCLUDE_SIGNATURES_BY_PAIR = {
    "fnvfo3:fo4": FNV_MVP_EXCLUDE_SIGNATURES,
    "skyrimse:fo4": SKYRIM_MVP_EXCLUDE_SIGNATURES,
}


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
        output_mod_name="FNV_FO3_Merged",
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
            output_name="FNV_FO3_Merged.esm",
            grafted_data_env="FO3_DATA_DIR",
            grafted_dir_env="FO3_DIR",
            grafted_extracted_env=get_profile("fo3").env_var_name,
        ),
        engine=get_profile("fnv").engine,
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
        output_mod_name="Skyrim_Merged",
        source_extracted_env=get_profile("skyrimse").env_var_name,
        source_data_env="SKYRIMSE_DATA_DIR",
        source_dir_env="SKYRIMSE_DIR",
        merge=MergeStageSpec(
            grafted_game="skyrimse",
            grafted_plugins=(),
            output_name="Skyrim_Merged.esm",
            grafted_data_env="SKYRIMSE_DATA_DIR",
            grafted_dir_env="SKYRIMSE_DIR",
            grafted_extracted_env=get_profile("skyrimse").env_var_name,
        ),
        engine=get_profile("skyrimse").engine,
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
