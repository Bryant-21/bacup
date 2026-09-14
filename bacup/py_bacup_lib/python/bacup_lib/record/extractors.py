"""Native asset collection metadata for conversion records."""
from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class AssetContribution:
    record_signatures: frozenset[str]
    subrecord_signatures: frozenset[str]
    kind: str


ASSET_CONTRIBUTIONS: tuple[AssetContribution, ...] = (
    AssetContribution(
        record_signatures=frozenset(),
        subrecord_signatures=frozenset({"MODL", "MOD2", "MOD3", "MOD4", "MOD5"}),
        kind="nif",
    ),
    AssetContribution(
        record_signatures=frozenset({"TXST"}),
        subrecord_signatures=frozenset({f"TX{index:02d}" for index in range(8)}),
        kind="texture",
    ),
    AssetContribution(
        record_signatures=frozenset(),
        subrecord_signatures=frozenset({"ICON", "MICO"}),
        kind="texture",
    ),
    # Cloud layers: FO3/FNV name the first four, later games number all 32.
    AssetContribution(
        record_signatures=frozenset({"WTHR"}),
        subrecord_signatures=(
            frozenset({"DNAM", "CNAM", "ANAM", "BNAM"})
            # Layer index counts up from '0' through ':' to 'O', not in hex.
            | frozenset(chr(ord("0") + index) + "0TX" for index in range(32))
        ),
        kind="texture",
    ),
    AssetContribution(
        record_signatures=frozenset({"MSWP"}),
        subrecord_signatures=frozenset({"BNAM", "MNAM"}),
        kind="material",
    ),
    AssetContribution(
        record_signatures=frozenset({"IDLE", "RACE"}),
        subrecord_signatures=frozenset({"ANAM", "BNAM"}),
        kind="behavior",
    ),
    AssetContribution(
        record_signatures=frozenset({"IDLE"}),
        subrecord_signatures=frozenset({"MODL"}),
        kind="kf_animation",
    ),
    AssetContribution(
        record_signatures=frozenset({"CREA"}),
        subrecord_signatures=frozenset({"MODL", "MODT"}),
        kind="nif",
    ),
    AssetContribution(
        record_signatures=frozenset({"CREA"}),
        subrecord_signatures=frozenset({"MODL"}),
        kind="creature_dir_scan",
    ),
    AssetContribution(
        record_signatures=frozenset({"SNDR"}),
        subrecord_signatures=frozenset({"ANAM", "FNAM"}),
        kind="sound",
    ),
    AssetContribution(
        record_signatures=frozenset({"SOUN"}),
        subrecord_signatures=frozenset({"FNAM"}),
        kind="sound",
    ),
    AssetContribution(
        record_signatures=frozenset({"MUSC"}),
        subrecord_signatures=frozenset({"ANAM", "FNAM"}),
        kind="sound",
    ),
    AssetContribution(
        record_signatures=frozenset({"MUST"}),
        subrecord_signatures=frozenset({"ANAM", "FNAM"}),
        kind="sound",
    ),
)

ASSET_SOURCE_PREFIXES: dict[str, tuple[str, ...]] = {
    "kf_animation": ("Meshes",),
}

FNV_KNOWN_DANGLING_CREATURE_KF_PATHS = frozenset(
    {
        "creatures/nvsecuritron/idleanims/specialidle_nvopening_securitron.kf",
        "creatures/nvsecuritron/idleanims/specialidle_nvopening_securitronidle.kf",
        "creatures/nvsecuritron/idleanims/specialidle_screentransition2.kf",
        "creatures/roach/idleanims/specialidle_wings.kf",
        "creatures/yaoguai/idleanims/mtspecialide_cleaningself.kf",
    }
)


def signatures_for_asset_kind(kind: str) -> frozenset[str]:
    sigs: set[str] = set()
    normalized_kind = str(kind).lower()
    for contribution in ASSET_CONTRIBUTIONS:
        if contribution.kind != normalized_kind:
            continue
        if not contribution.record_signatures:
            return frozenset()
        sigs.update(contribution.record_signatures)
    return frozenset(sigs)


def source_prefixes_for_asset_kind(kind: str) -> tuple[str, ...]:
    return ASSET_SOURCE_PREFIXES.get(str(kind).casefold(), ())


def is_known_dangling_asset_reference(
    source_game: str,
    kind: str,
    source_path: str,
) -> bool:
    if str(source_game).casefold() not in {"fnv", "falloutnv"}:
        return False
    if str(kind).casefold() != "kf_animation":
        return False
    normalized = str(source_path).replace("\\", "/").strip().lstrip("/").casefold()
    if normalized.startswith("meshes/"):
        normalized = normalized[7:]
    return normalized in FNV_KNOWN_DANGLING_CREATURE_KF_PATHS


__all__ = [
    "ASSET_CONTRIBUTIONS",
    "ASSET_SOURCE_PREFIXES",
    "AssetContribution",
    "FNV_KNOWN_DANGLING_CREATURE_KF_PATHS",
    "is_known_dangling_asset_reference",
    "signatures_for_asset_kind",
    "source_prefixes_for_asset_kind",
]
