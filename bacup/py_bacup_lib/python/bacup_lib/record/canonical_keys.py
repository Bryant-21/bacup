"""Canonical authoring keys + utilities used by the live conversion pipeline.

The Rust translator owns translate-phase logic; these helpers are the
post-translate utilities and validator constants consumed by Python.
"""
from __future__ import annotations

from functools import lru_cache
from typing import Any

from bacup_lib.record.schema_surface import (
    SchemaRecordError,
    get_schema_surface,
)

TARGET_RECORD_TYPE_FIELD = "__target_record_type"

_CANONICAL_TOP_KEYS = frozenset(
    {
        "form_id",
        "flags",
        "version_control",
        "form_version",
        "version2",
        "eid",
        "fields",
        TARGET_RECORD_TYPE_FIELD,
    }
)

# Transform names registered for use in translation-map YAMLs. Consumed by
# the validator only; the Rust translator has its own transform registry.
REGISTERED_TRANSFORM_TYPES: frozenset[str] = frozenset(
    {
        "clamp_enum",
        "clamp_layer_index",
        "clamp_max",
        "convert_leveled_entries",
        "convert_leveled_item_entries",
        "enum_map",
        "filter_list",
        "fo76_scol_static",
        "flatten_curvetable",
        "merge",
        "remap_enum",
        "remap_formkey",
        "remap_formkey_with_overrides",
        "restructure",
        "scale",
        "scale_nested",
        "strip_subfields",
        "translate_conditions",
        "translate_effects",
        "trim_languages",
        "wrap_in_list",
        "rewrite_creature_anam",
        "rgdl_to_bodt_default",
    }
)

# Target-schema fields that translation transforms can emit even though they
# are not part of the target record's static schema (validator-aware).
TRANSIENT_OUTPUT_FIELDS: dict[tuple[str, str, str], frozenset[str]] = {
    ("fo76", "fo4", "WEAP"): frozenset({"RGW3"}),
}

# Pair-hook synthetic source fields the validator must accept on top of the
# source schema.
_SYNTHETIC_SOURCE_FIELDS: dict[tuple[str, str], dict[str, frozenset[str]]] = {
    ("fo76", "fo4"): {
        "RACE": frozenset({"BehaviorGraphDatas"}),
        "ALCH": frozenset({"Effects"}),
        "ENCH": frozenset({"Effects"}),
        "PERK": frozenset({"Effects"}),
        "SPEL": frozenset({"Effects"}),
    },
}


def synthetic_source_fields(
    source_game: str, target_game: str, record_type: str
) -> frozenset[str]:
    """Return synthetic source fields the validator should accept for this pair."""
    pair = _SYNTHETIC_SOURCE_FIELDS.get((source_game, target_game), {})
    return pair.get(record_type, frozenset())


@lru_cache(maxsize=None)
def _allowed_field_keys(
    target_game: str,
    effective_record_type: str,
    record_type: str,
    source_game: str | None,
) -> frozenset[str] | None:
    try:
        allowed = get_schema_surface(target_game).allowed_keys(effective_record_type)
    except SchemaRecordError:
        return None
    if source_game is not None:
        allowed = allowed | TRANSIENT_OUTPUT_FIELDS.get(
            (source_game, target_game, record_type),
            frozenset(),
        )
    return allowed


def find_unknown_fields(
    record: dict,
    record_type: str,
    target_game: str = "fo4",
    source_game: str | None = None,
) -> list[str]:
    """Return sorted labels not accepted by the target game's schema surface."""
    effective_record_type = str(record.get(TARGET_RECORD_TYPE_FIELD) or record_type)
    allowed = _allowed_field_keys(
        target_game, effective_record_type, record_type, source_game
    )
    if allowed is None:
        return []

    fields = record.get("fields")
    if isinstance(fields, list):
        labels = {
            key
            for entry in fields
            if isinstance(entry, dict)
            for key in entry
        }
        return sorted(label for label in labels if label not in allowed)

    return sorted(
        key for key in record
        if key not in _CANONICAL_TOP_KEYS and key not in allowed
    )


def _interleave_body_part_fields(fields: list[dict[str, Any]]) -> list[dict[str, Any]]:
    row_keys = (
        "PartName",
        "PartNode",
        "VATSTarget",
        "NodeData",
        "LimbReplacementModel",
        "GoreEffectsTargetBone",
        "NAM5",
        "HitReactionStart",
        "HitReactionEnd",
        "TwistVariablePrefix",
    )
    buckets: dict[str, list[Any]] = {key: [] for key in row_keys}
    other: list[dict[str, Any]] = []
    first_row_at: int | None = None
    for entry in fields:
        if not isinstance(entry, dict) or len(entry) != 1:
            other.append(entry)
            continue
        key, value = next(iter(entry.items()))
        if key not in buckets:
            other.append(entry)
            continue
        if first_row_at is None:
            first_row_at = len(other)
        buckets[key].append(value)

    row_count = max((len(values) for values in buckets.values()), default=0)
    if not row_count:
        return fields
    rows: list[dict[str, Any]] = []
    for index in range(row_count):
        for key in row_keys:
            values = buckets[key]
            if index < len(values):
                rows.append({key: values[index]})
    first_row_at = len(other) if first_row_at is None else first_row_at
    return other[:first_row_at] + rows + other[first_row_at:]
