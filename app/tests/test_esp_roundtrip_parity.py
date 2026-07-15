from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

import pytest

from app.env_config import build_game_context_from_env
from app.env_sync import parse_env_file
from creation_lib.esp import Plugin, export_json, export_yaml, import_json, import_yaml
from creation_lib.esp.schema import (
    build_corpus_manifest,
    build_layered_manifest,
    get_official_allowlist,
    iter_official_plugin_paths,
    record_coverage_rows,
    schema_coverage_summary,
)

ROUNDTRIP_CASES = [
    ("fo4", "Fallout4.esm"),
    ("fo3", "Fallout3.esm"),
    ("fnv", "FalloutNV.esm"),
    ("skyrimse", "Skyrim.esm"),
]

PARSE_ONLY_CASES = [
    ("fo76", "SeventySix.esm"),
    ("starfield", "Starfield.esm"),
]

LOSSLESS_TEXT_ROUNDTRIP_CASES = [
    ("json", export_json, import_json),
    ("yaml", export_yaml, import_yaml),
]

BREAKING_CLASSIFICATIONS = {"coverage_gap", "structural_loss_or_type_change", "other_payload_mismatch"}

ManifestBuilder = Callable[..., Any]
LosslessExporter = Callable[[Plugin], str]
LosslessImporter = Callable[[str], Plugin]


def resolve_game_data_dir(game: str) -> Path | None:
    import os

    env = parse_env_file()
    env.update(os.environ)
    return build_game_context_from_env(game, env).data_dir


@dataclass(frozen=True, slots=True)
class PluginDiffClassification:
    kind: str
    summary: str


def _selected_official_plugin_path(game: str, plugin_name: str) -> Path:
    assert plugin_name in get_official_allowlist(game)
    data_dir = resolve_game_data_dir(game)
    if data_dir is None:
        pytest.skip(f"data directory not configured for {game}")
    paths = iter_official_plugin_paths(game, data_dir, allowlist=[plugin_name])
    if not paths:
        pytest.skip(f"{plugin_name} not available for {game}")
    return paths[0]


def _build_selected_manifest(builder: ManifestBuilder, game: str, plugin_name: str):
    plugin_path = _selected_official_plugin_path(game, plugin_name)
    manifest = builder(game, plugin_path.parent, allowlist=[plugin_name])
    assert manifest.plugins == [plugin_name]
    return manifest


def _clone_plugin(plugin: Plugin) -> Plugin:
    return Plugin.from_bytes(
        plugin.to_bytes(),
        plugin_name=plugin.plugin_name,
        game=plugin.game,
        auto_load_strings=False,
    )


def _canonical_form_id(form_id: int) -> int:
    form_id &= 0xFFFFFFFF
    if form_id >> 24 == 0xFF:
        return form_id & 0x00FFFFFF
    return form_id


def _record_identity_signature_map(plugin: Plugin) -> dict[int, str]:
    return {
        _canonical_form_id(record.form_id): record.signature
        for record in plugin.records
        if _canonical_form_id(record.form_id) != 0
    }


def _record_payload_fingerprint(record) -> tuple[Any, ...]:
    return (
        record.flags,
        record.version_control,
        record.form_version,
        record.version2,
        tuple((subrecord.signature, bytes(subrecord.data)) for subrecord in record.subrecords),
    )


def _ordered_record_fingerprints(plugin: Plugin) -> list[tuple[Any, ...]]:
    return [
        (
            _canonical_form_id(record.form_id),
            record.signature,
            _record_payload_fingerprint(record),
        )
        for record in plugin.records
    ]


def _payload_fingerprints_by_form_id(plugin: Plugin) -> dict[int, tuple[Any, ...]]:
    return {
        _canonical_form_id(record.form_id): _record_payload_fingerprint(record)
        for record in plugin.records
        if _canonical_form_id(record.form_id) != 0
    }


def _format_structural_drift(original: Plugin, roundtrip: Plugin) -> str:
    original_map = _record_identity_signature_map(original)
    roundtrip_map = _record_identity_signature_map(roundtrip)

    missing = sorted(set(original_map) - set(roundtrip_map))
    extra = sorted(set(roundtrip_map) - set(original_map))
    type_changed = sorted(
        form_id for form_id in set(original_map) & set(roundtrip_map) if original_map[form_id] != roundtrip_map[form_id]
    )

    def _sample(entries: list[int], mapping: dict[int, str]) -> str:
        shown = [f"{form_id:08X}:{mapping[form_id]}" for form_id in entries[:5]]
        if len(entries) > 5:
            shown.append(f"+{len(entries) - 5} more")
        return ", ".join(shown) if shown else "none"

    return (
        "structural_loss_or_type_change: "
        f"missing={len(missing)} [{_sample(missing, original_map)}]; "
        f"extra={len(extra)} [{_sample(extra, roundtrip_map)}]; "
        f"type_changed={len(type_changed)} ["
        + ", ".join(
            f"{form_id:08X}:{original_map[form_id]}->{roundtrip_map[form_id]}" for form_id in type_changed[:5]
        )
        + ("]" if len(type_changed) <= 5 else f", +{len(type_changed) - 5} more]")
    )


def _classify_plugin_diff(original: Plugin, roundtrip: Plugin) -> PluginDiffClassification:
    original_bytes = original.to_bytes()
    roundtrip_bytes = roundtrip.to_bytes()
    if roundtrip_bytes == original_bytes:
        return PluginDiffClassification(
            "exact_byte_match",
            f"exact_byte_match: {len(original_bytes)} bytes identical",
        )

    original_identity_map = _record_identity_signature_map(original)
    roundtrip_identity_map = _record_identity_signature_map(roundtrip)
    if len(original.records) != len(roundtrip.records) or original_identity_map != roundtrip_identity_map:
        return PluginDiffClassification(
            "structural_loss_or_type_change",
            _format_structural_drift(original, roundtrip),
        )

    original_ordered = _ordered_record_fingerprints(original)
    roundtrip_ordered = _ordered_record_fingerprints(roundtrip)
    if Counter(original_ordered) == Counter(roundtrip_ordered) and original_ordered != roundtrip_ordered:
        return PluginDiffClassification(
            "ordering_only_drift",
            "ordering_only_drift: same record identities and record-byte multiset, different record order",
        )

    original_payloads = _payload_fingerprints_by_form_id(original)
    roundtrip_payloads = _payload_fingerprints_by_form_id(roundtrip)
    changed = [
        f"{form_id:08X}:{original_identity_map[form_id]}"
        for form_id in sorted(original_payloads)
        if original_payloads[form_id] != roundtrip_payloads.get(form_id)
    ]
    shown = ", ".join(changed[:5]) if changed else "none"
    if len(changed) > 5:
        shown += f", +{len(changed) - 5} more"
    return PluginDiffClassification(
        "other_payload_mismatch",
        f"other_payload_mismatch: payload changed for {len(changed)} records [{shown}]",
    )


def _classify_coverage_row(row: dict[str, Any]) -> str:
    if row["expected_subrecords"] == 0:
        return "observation_only"
    if row["missing_subrecords"] or row["unexpected_subrecords"]:
        return "coverage_gap"
    if row["order_overlap"] is not None and row["order_overlap"] < 1.0:
        return "ordering_drift_only"
    return "covered_no_gap"


def _classification_counts(rows: list[dict[str, Any]]) -> Counter[str]:
    return Counter(_classify_coverage_row(row) for row in rows)


def _is_breaking_classification(classification: str) -> bool:
    return classification in BREAKING_CLASSIFICATIONS


def _build_fixture_plugin() -> Plugin:
    plugin = Plugin.new("B21_ParityFixture.esp", game="fo4")

    alpha = plugin.new_record("MISC")
    alpha.editor_id = "B21_ParityAlpha"
    alpha.full_name = "Parity Alpha"
    alpha.add_subrecord("AID ", b"\x10\x20\x30\x40")
    plugin.add_record(alpha)

    beta = plugin.new_record("MISC")
    beta.editor_id = "B21_ParityBeta"
    beta.full_name = "Parity Beta"
    beta.add_subrecord("DESC", b"beta\x00")
    plugin.add_record(beta)

    return plugin


def _reorder_fixture_records(plugin: Plugin) -> Plugin:
    plugin.find_top_group("MISC").children.reverse()
    return plugin


def _drop_fixture_record(plugin: Plugin) -> Plugin:
    plugin.find_top_group("MISC").children.pop()
    return plugin


def _mutate_fixture_payload(plugin: Plugin) -> Plugin:
    plugin.find_top_group("MISC").children[0].get_subrecord("FULL").set_string("Parity Alpha Changed")
    return plugin


@pytest.mark.parametrize(
    ("mutator", "expected_kind"),
    [
        (lambda plugin: plugin, "exact_byte_match"),
        (_reorder_fixture_records, "ordering_only_drift"),
        (_drop_fixture_record, "structural_loss_or_type_change"),
        (_mutate_fixture_payload, "other_payload_mismatch"),
    ],
)
def test_plugin_diff_classification_distinguishes_exact_ordering_structural_and_payload_cases(
    mutator: Callable[[Plugin], Plugin],
    expected_kind: str,
) -> None:
    original = _build_fixture_plugin()
    roundtrip = _clone_plugin(original)

    classified = _classify_plugin_diff(original, mutator(roundtrip))

    assert classified.kind == expected_kind
    assert expected_kind in classified.summary


@pytest.mark.parametrize(
    ("row", "expected_classification", "is_breaking"),
    [
        (
            {
                "expected_subrecords": 4,
                "missing_subrecords": 0,
                "unexpected_subrecords": 0,
                "order_overlap": 1.0,
            },
            "covered_no_gap",
            False,
        ),
        (
            {
                "expected_subrecords": 4,
                "missing_subrecords": 0,
                "unexpected_subrecords": 0,
                "order_overlap": 0.5,
            },
            "ordering_drift_only",
            False,
        ),
        (
            {
                "expected_subrecords": 4,
                "missing_subrecords": 1,
                "unexpected_subrecords": 0,
                "order_overlap": 0.5,
            },
            "coverage_gap",
            True,
        ),
        (
            {
                "expected_subrecords": 0,
                "missing_subrecords": 0,
                "unexpected_subrecords": 7,
                "order_overlap": None,
            },
            "observation_only",
            False,
        ),
    ],
)
def test_coverage_classification_treats_ordering_drift_as_diagnostic_only(
    row: dict[str, Any],
    expected_classification: str,
    is_breaking: bool,
) -> None:
    classification = _classify_coverage_row(row)

    assert classification == expected_classification
    assert _is_breaking_classification(classification) is is_breaking


@pytest.mark.parametrize(("exporter", "importer"), [(export_json, import_json), (export_yaml, import_yaml)])
def test_lossless_text_roundtrip_preserves_space_padded_subrecord_signatures(
    exporter: LosslessExporter,
    importer: LosslessImporter,
) -> None:
    plugin = Plugin.new("B21_Spacing.esp", game="fo4")
    record = plugin.new_record("MISC")
    record.editor_id = "B21_SpacingRecord"
    record.add_subrecord("AID ", b"\x10\x20\x30\x40")
    plugin.add_record(record)

    roundtrip = importer(exporter(plugin, mode="lossless"))
    classified = _classify_plugin_diff(plugin, roundtrip)

    assert [subrecord.signature for subrecord in roundtrip.records[0].subrecords] == ["EDID", "AID "]
    assert classified.kind == "exact_byte_match", classified.summary


@pytest.mark.integration
@pytest.mark.parametrize(("format_name", "exporter", "importer"), LOSSLESS_TEXT_ROUNDTRIP_CASES)
@pytest.mark.parametrize(("game", "plugin_name"), ROUNDTRIP_CASES)
def test_lossless_official_plugin_roundtrip_prefers_exact_byte_match(
    game: str,
    plugin_name: str,
    format_name: str,
    exporter: LosslessExporter,
    importer: LosslessImporter,
) -> None:
    del format_name
    plugin_path = _selected_official_plugin_path(game, plugin_name)
    plugin = Plugin.load(plugin_path, game=game)

    roundtrip = importer(exporter(plugin, mode="lossless"))
    classified = _classify_plugin_diff(plugin, roundtrip)

    assert classified.kind == "exact_byte_match", classified.summary


@pytest.mark.integration
@pytest.mark.parametrize(("game", "plugin_name"), ROUNDTRIP_CASES)
def test_roundtrip_matrix_classifies_layered_manifest_rows(game: str, plugin_name: str) -> None:
    manifest = _build_selected_manifest(build_layered_manifest, game, plugin_name)

    rows = record_coverage_rows(manifest)
    summary = schema_coverage_summary(manifest)
    classifications = _classification_counts(rows)
    xedit_rows = [row for row in rows if row["expected_subrecords"] > 0]

    assert rows
    assert {"signature", "expected_subrecords", "missing_subrecords", "unexpected_subrecords", "order_overlap"} <= set(rows[0])
    assert summary["game"] == game
    assert summary["record_count"] == len(rows) > 0
    assert summary["records_with_xedit"] == len(xedit_rows) > 0
    assert sum(classifications.values()) == len(rows)
    assert classifications["covered_no_gap"] + classifications["ordering_drift_only"] > 0
    assert classifications["covered_no_gap"] + classifications["ordering_drift_only"] + classifications["coverage_gap"] == len(xedit_rows)


@pytest.mark.integration
@pytest.mark.parametrize(("game", "plugin_name"), PARSE_ONLY_CASES)
def test_parse_only_matrix_builds_observation_only_manifest(game: str, plugin_name: str) -> None:
    manifest = _build_selected_manifest(build_corpus_manifest, game, plugin_name)

    rows = record_coverage_rows(manifest)
    summary = schema_coverage_summary(manifest)
    classifications = _classification_counts(rows)

    assert summary["game"] == game
    assert summary["record_count"] == len(rows) > 0
    assert summary["records_with_xedit"] == 0
    assert classifications == Counter({"observation_only": len(rows)})
