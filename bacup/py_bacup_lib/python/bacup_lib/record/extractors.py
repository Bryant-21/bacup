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


__all__ = [
    "ASSET_CONTRIBUTIONS",
    "AssetContribution",
    "signatures_for_asset_kind",
]
