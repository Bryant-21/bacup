"""Tests for post-M7 asset extractor metadata."""
from __future__ import annotations

from bacup_lib.record import extractors
from bacup_lib.record.extractors import (
    ASSET_CONTRIBUTIONS,
    AssetContribution,
    signatures_for_asset_kind,
)


def _contribution_for(kind: str, record_sig: str | None, subrecord_sig: str) -> AssetContribution:
    for contribution in ASSET_CONTRIBUTIONS:
        if contribution.kind != kind:
            continue
        if record_sig is not None and record_sig not in contribution.record_signatures:
            continue
        if subrecord_sig in contribution.subrecord_signatures:
            return contribution
    raise AssertionError(f"No contribution for {kind}:{record_sig}:{subrecord_sig}")


def test_key_subrecord_signatures_map_to_asset_kinds() -> None:
    assert _contribution_for("nif", None, "MODL").record_signatures == frozenset()
    assert _contribution_for("nif", None, "MOD2").record_signatures == frozenset()
    assert _contribution_for("nif", None, "MOD3").record_signatures == frozenset()
    assert _contribution_for("texture", "TXST", "TX00").record_signatures == frozenset({"TXST"})
    assert _contribution_for("texture", None, "ICON").record_signatures == frozenset()
    assert _contribution_for("material", "MSWP", "BNAM").record_signatures == frozenset({"MSWP"})
    assert _contribution_for("behavior", "IDLE", "ANAM").record_signatures == frozenset({"IDLE", "RACE"})
    assert _contribution_for("behavior", "RACE", "BNAM").record_signatures == frozenset({"IDLE", "RACE"})
    assert _contribution_for("sound", "SNDR", "FNAM").record_signatures == frozenset({"SNDR"})
    assert _contribution_for("sound", "MUSC", "ANAM").record_signatures == frozenset({"MUSC"})
    assert _contribution_for("sound", "MUST", "FNAM").record_signatures == frozenset({"MUST"})


def test_txst_texture_slots_are_canonical_range() -> None:
    contribution = _contribution_for("texture", "TXST", "TX00")
    assert contribution.subrecord_signatures == frozenset(f"TX{index:02d}" for index in range(8))


def test_signatures_for_asset_kind_reports_record_scoped_contributions() -> None:
    assert signatures_for_asset_kind("material") == frozenset({"MSWP"})
    assert signatures_for_asset_kind("behavior") == frozenset({"IDLE", "RACE"})
    assert signatures_for_asset_kind("sound") == frozenset({"SNDR", "SOUN", "MUSC", "MUST"})


def test_signatures_for_asset_kind_returns_empty_for_global_or_unknown_kinds() -> None:
    assert signatures_for_asset_kind("nif") == frozenset()
    assert signatures_for_asset_kind("texture") == frozenset()
    assert signatures_for_asset_kind("missing") == frozenset()
    assert signatures_for_asset_kind("SOUND") == frozenset({"SNDR", "SOUN", "MUSC", "MUST"})


def test_module_exports_metadata_api_only() -> None:
    assert set(extractors.__all__) == {
        "ASSET_CONTRIBUTIONS",
        "AssetContribution",
        "signatures_for_asset_kind",
    }
    assert not hasattr(extractors, "get" + "_extractor")
    assert not hasattr(extractors, "_" + "EXTRACTORS")
    assert all(isinstance(contribution, AssetContribution) for contribution in ASSET_CONTRIBUTIONS)
