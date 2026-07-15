from __future__ import annotations

from bacup_lib.record.extractors import (
    ASSET_CONTRIBUTIONS,
    signatures_for_asset_kind,
)


def _subrecords_by_record_signature(kind: str) -> dict[str, frozenset[str]]:
    mapping: dict[str, set[str]] = {}
    for contribution in ASSET_CONTRIBUTIONS:
        if contribution.kind != kind:
            continue
        for record_signature in contribution.record_signatures:
            mapping.setdefault(record_signature, set()).update(
                contribution.subrecord_signatures
            )
    return {
        record_signature: frozenset(subrecord_signatures)
        for record_signature, subrecord_signatures in mapping.items()
    }


def test_sound_signatures_include_audio_record_types() -> None:
    assert signatures_for_asset_kind("sound") == frozenset(
        {"SNDR", "SOUN", "MUSC", "MUST"}
    )


def test_sound_contributions_capture_filename_subrecords() -> None:
    assert _subrecords_by_record_signature("sound") == {
        "SNDR": frozenset({"ANAM", "FNAM"}),
        "SOUN": frozenset({"FNAM"}),
        "MUSC": frozenset({"ANAM", "FNAM"}),
        "MUST": frozenset({"ANAM", "FNAM"}),
    }
