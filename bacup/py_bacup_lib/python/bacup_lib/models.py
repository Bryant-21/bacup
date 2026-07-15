"""Data models for the conversion engine."""
from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from enum import StrEnum
from pathlib import Path
from typing import Any, Literal

from creation_lib.animation.models import (
    AnimationClip,
    AnimationEvent,
    AnimationKeyframe,
    BoneChannel,
    FloatChannel,
)


@dataclass
class AssetProvenance:
    """Why a given asset ended up in the dependency graph."""

    added_by_record_fk: str   # FormKey of the record that caused inclusion
    added_by_record_eid: str  # EditorID of that record (human-readable)
    added_by_field: str       # Field/dotpath that referenced this asset
    walk_depth: int           # Hops from the root FormKey (root = 0)
    walker_pass: str          # "main" | "reverse_race" | "reverse_skm" | "behavior_sound" | "nif_inline" | "nif_secondary" | "material_texture" | "behavior_bundle" | "character_assets" | "addon_node" | "animation_lookup"
    added_by_record_sig: str = ""


@dataclass
class RecordProvenance:
    """Why a given record ended up in the dependency graph."""

    added_by_record_fk: str   # FormKey of the record that caused inclusion
    added_by_record_eid: str  # EditorID of that record (human-readable)
    added_by_field: str       # Field/dotpath that triggered the edge
    walk_depth: int           # Hops from the root FormKey (root = 0)
    walker_pass: str          # Same enum as AssetProvenance.walker_pass


@dataclass
class AssetRef:
    """Reference to a game asset (mesh, texture, material, sound, etc.)."""

    asset_type: str  # "nif", "texture", "material", "sound", "behavior", "script"
    source_path: str  # Relative game path, e.g. "Meshes/Weapons/Gun.nif"
    resolved_path: str | None = None  # Absolute disk path, None if not found
    resolution_error: str | None = None  # Why resolution failed (shown in UI)
    is_cdb_ref: bool = False  # True when this material is sourced from a FO76 MaterialsDB.cdb entry
    provenance: AssetProvenance | None = None  # Why this asset is in the graph
    force_convert: bool = False
    force_reason: str = ""
    output_subpath: str | None = None


@dataclass
class ExtractedRefs:
    """Output of a record-type extractor — assets and sub-record FormKeys."""

    assets: list[AssetRef] = field(default_factory=list)
    form_keys: list[str] = field(default_factory=list)


@dataclass(init=False)
class RecordNode:
    """A game record and its direct dependencies."""

    form_key: str
    editor_id: str
    record_type: str  # "WEAP", "ARMO", "NPC_", etc.
    assets: list[AssetRef] = field(default_factory=list)
    children: list[RecordNode] = field(default_factory=list)
    provenance: RecordProvenance | None = None  # Why this record is in the graph

    def __init__(
        self,
        form_key: str,
        editor_id: str,
        record_type: str,
        assets: list[AssetRef] | None = None,
        children: list["RecordNode"] | None = None,
        provenance: RecordProvenance | None = None,
    ) -> None:
        self.form_key = form_key
        self.editor_id = editor_id
        self.record_type = record_type
        self.assets = list(assets or [])
        self.children = list(children or [])
        self.provenance = provenance


@dataclass
class DependencyGraph:
    """Complete dependency tree from a root record."""

    root: RecordNode
    all_records: list[RecordNode] = field(default_factory=list)
    all_assets: list[AssetRef] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)

    def write_provenance_files(self, mod_dir: str) -> dict[str, int]:
        """Write asset_provenance.jsonl and record_provenance.jsonl to mod_dir.

        Returns a summary dict mapping ancestor label -> asset count.
        The ancestor label is "<eid> (<fk>)" for the depth-1 record that
        transitively brought in the asset (direct child of root, not the
        immediate adder).
        """
        asset_path = os.path.join(mod_dir, "asset_provenance.jsonl")
        record_path = os.path.join(mod_dir, "record_provenance.jsonl")

        # Build fk -> "eid (fk)" label for the depth-1 ancestor of each record.
        # Depth-1 records (direct children of root) are their own top-level ancestor.
        # Deeper records trace added_by_record_fk up the chain until depth <= 1.
        fk_to_rec: dict[str, "RecordNode"] = {r.form_key: r for r in self.all_records}
        fk_to_ancestor_label: dict[str, str] = {}

        def _ancestor_label(fk: str, _depth: int = 0) -> str:
            if fk in fk_to_ancestor_label:
                return fk_to_ancestor_label[fk]
            rec = fk_to_rec.get(fk)
            if rec is None or rec.provenance is None or rec.provenance.walk_depth <= 1:
                label = f"{rec.editor_id} ({fk})" if rec else fk
                fk_to_ancestor_label[fk] = label
                return label
            # Recurse up one hop; guard against cycles with a depth limit
            if _depth > 50:
                label = f"{rec.editor_id} ({fk})"
                fk_to_ancestor_label[fk] = label
                return label
            label = _ancestor_label(rec.provenance.added_by_record_fk, _depth + 1)
            fk_to_ancestor_label[fk] = label
            return label

        ancestor_counts: dict[str, int] = {}

        with open(asset_path, "w", encoding="utf-8") as af:
            for asset in self.all_assets:
                prov = asset.provenance
                entry: dict = {
                    "asset_path": asset.source_path,
                    "asset_type": asset.asset_type,
                    "added_by_record_fk": prov.added_by_record_fk if prov else None,
                    "added_by_record_eid": prov.added_by_record_eid if prov else None,
                    "added_by_field": prov.added_by_field if prov else None,
                    "walk_depth": prov.walk_depth if prov else None,
                    "walker_pass": prov.walker_pass if prov else None,
                }
                af.write(json.dumps(entry) + "\n")

                if prov:
                    label = _ancestor_label(prov.added_by_record_fk)
                    ancestor_counts[label] = ancestor_counts.get(label, 0) + 1

        with open(record_path, "w", encoding="utf-8") as rf:
            for rec in self.all_records:
                prov = rec.provenance
                entry = {
                    "form_key": rec.form_key,
                    "editor_id": rec.editor_id,
                    "record_type": rec.record_type,
                    "added_by_record_fk": prov.added_by_record_fk if prov else None,
                    "added_by_record_eid": prov.added_by_record_eid if prov else None,
                    "added_by_field": prov.added_by_field if prov else None,
                    "walk_depth": prov.walk_depth if prov else None,
                    "walker_pass": prov.walker_pass if prov else None,
                }
                rf.write(json.dumps(entry) + "\n")

        return ancestor_counts


def merge_dependency_graphs(graphs: list[DependencyGraph]) -> DependencyGraph:
    """Union multiple dependency graphs into one for batch conversion.

    Dedups records by form_key and assets by (asset_type, source_path),
    preserving first-seen order. The first graph's root is reused as the merged
    graph's `.root` to preserve the `DependencyGraph` invariant.
    Errors from all input graphs are concatenated.

    Raises ValueError if `graphs` is empty.
    """
    if not graphs:
        raise ValueError("merge_dependency_graphs requires at least one graph")
    if len(graphs) == 1:
        return graphs[0]

    seen_records: set[str] = set()
    merged_records: list[RecordNode] = []
    seen_assets: set[tuple[str, str]] = set()
    merged_assets: list[AssetRef] = []
    merged_errors: list[str] = []

    for g in graphs:
        for rec in g.all_records:
            if rec.form_key in seen_records:
                continue
            seen_records.add(rec.form_key)
            merged_records.append(rec)
        for asset in g.all_assets:
            key = (asset.asset_type, asset.source_path)
            if key in seen_assets:
                continue
            seen_assets.add(key)
            merged_assets.append(asset)
        merged_errors.extend(g.errors)

    return DependencyGraph(
        root=graphs[0].root,
        all_records=merged_records,
        all_assets=merged_assets,
        errors=merged_errors,
    )


@dataclass
class PhaseProgress:
    """Progress update for a single pipeline phase."""

    phase: int  # 1-7
    phase_name: str
    total_items: int = 0
    completed_items: int = 0
    current_item: str = ""
    status: str = "pending"  # "pending", "running", "completed", "error", "cancelled"
    error: str | None = None
    elapsed_seconds: float | None = None


@dataclass
class ConversionSummary:
    """Final summary of a conversion run."""

    records_translated: int = 0
    records_warnings: int = 0
    records_vanilla_remapped: int = 0
    records_new_allocated: int = 0
    nifs_base_game_skipped: int = 0
    btos_base_game_skipped: int = 0
    textures_base_game_skipped: int = 0
    materials_base_game_skipped: int = 0
    havok_base_game_skipped: int = 0
    animations_base_game_skipped: int = 0
    audio_base_game_skipped: int = 0
    nifs_total: int = 0
    nifs_converted: int = 0
    nifs_failed: int = 0
    btos_total: int = 0
    btos_converted: int = 0
    btos_failed: int = 0
    combo_nifs_skipped: int = 0
    textures_total: int = 0
    textures_converted: int = 0
    textures_failed: int = 0
    materials_total: int = 0
    materials_converted: int = 0
    materials_failed: int = 0
    havok_total: int = 0
    havok_converted: int = 0
    havok_remapped: int = 0
    havok_failed: int = 0
    animations_total: int = 0
    animations_converted: int = 0
    animations_failed: int = 0
    audio_total: int = 0
    audio_copied: int = 0
    audio_failed: int = 0
    scripts_flagged: int = 0
    formkeys_swept: int = 0       # FormKeys caught by post-rewrite sweep
    formkeys_unresolved: int = 0  # FormKeys that couldn't be resolved by sweep
    validation_errors: int = 0
    validation_warnings: int = 0
    esp_built: bool = False
    mod_path: str = ""
    weapon_slot_records: dict[tuple[str, int], list[str]] = field(default_factory=dict)
    fnv_translation_gaps: list[FnvTranslationGap] = field(default_factory=list)
    lip_regeneration_needed: list[str] = field(default_factory=list)


@dataclass
class PhaseSelection:
    """Per-phase enable flags. Mirrors the legacy PluginPortOptions phase bools
    1:1 EXCEPT `convert_lod`, which is the user-facing name for the `.bto`
    object-LOD phase (legacy `convert_btos`). Defaults are the full-run defaults.
    `synthesize_drivers` defaults on but only runs when `convert_havok` is also on
    (the whole-plugin orchestrator couples the two); it operates on the HKX the
    havok phase produces, so it is a no-op without havok."""

    translate_records: bool = True
    convert_placed_records: bool = True
    convert_npc_faces: bool = True
    convert_terrain: bool = True
    convert_nifs: bool = True
    convert_lod: bool = True            # -> PluginPortOptions.convert_btos
    convert_textures: bool = True
    convert_materials: bool = True
    convert_havok: bool = True
    synthesize_drivers: bool = True     # havok-gated at runtime (see orchestrator)
    convert_animations: bool = True
    convert_scripts: bool = True
    copy_sounds: bool = True
    build_esp: bool = True
    regenerate_modt: bool = True        # Bucket B: post-asset MODT compute, always on (see family_map)
    generate_anim_text_data: bool = True
    lod_mode: str = "convert"  # {"convert","generate","hybrid","hybrid-atlas","none"}: native lodgen delivery switch

    @classmethod
    def defaults(cls) -> "PhaseSelection":
        return cls()


def auto_conversion_worker_count() -> int:
    """Return the default conversion worker count for auto mode."""

    cpu_count = os.cpu_count() or 2
    return max(cpu_count // 2, 1)


# ---------------------------------------------------------------------------
# Workflow context + plugin-port API
# ---------------------------------------------------------------------------


@dataclass
class ConversionContext:
    """Shared state passed to every pipeline phase."""

    source_game: str
    target_game: str
    mod_path: Path
    output_plugin_name: str
    target_extracted_dir: Path | None
    target_data_dir: Path | None
    formkey_mapper: Any
    fixups: Any
    summary: ConversionSummary
    target_asset_store: Any | None = None
    target_asset_catalog_path: Path | None = None
    target_asset_cache_dir: Path | None = None
    material_overrides: dict = field(default_factory=dict)
    converted_plugin_registry: "ConvertedPluginRegistry | None" = None
    source_plugin_handle: Any | None = None
    source_master_handles: list[Any] = field(default_factory=list)
    target_master_handles: list[Any] = field(default_factory=list)
    terrain_options: Any | None = None
    is_whole_plugin: bool = False
    emit_authoring_yaml: bool = True
    preserve_source_ids: bool = True
    overwrite_existing: bool = False
    source_data_dir: Path | None = None
    additional_source_asset_roots: tuple[Path, ...] = ()
    conversion_workers: int | None = None
    disable_nif_collision_memo: bool = False
    records_limit: int | None = None
    generated_object_id_floor: int = 0
    force_cpu_textures: bool = False
    pbr_carry: bool = False
    texture_landscape_mip_flooding: bool = False
    convert_precombined_nifs: bool = True
    base_asset_relocation_mesh_roots: tuple[str, ...] = ()
    base_asset_namespace: str = ""
    diagnostics_root: Path | None = None
    # AddonNode old→new index remap drained from the ESP fixup decisions; fed to
    # the NIF phase so `BSValueNode` blocks are repointed to reconciled indices.
    addon_index_map: dict[int, int] = field(default_factory=dict)


@dataclass
class TerrainOptions:
    """Terrain phase inputs."""

    btd_path: str = ""
    fo76_data_dir: str = ""
    source_extracted_dir: str = ""
    worldspace_editor_id: str = ""
    source_worldspace_editor_id: str = ""
    source_min_x: int | None = None
    source_min_y: int | None = None
    source_max_x: int | None = None
    source_max_y: int | None = None
    resample_mode: str = "lanczos"
    emit_btd4: bool = True
    emit_textures: bool = True
    export_heightmap: bool = False
    debug_flat_land: bool = False
    source_worldspace_authoring_dir: str = ""
    water_manifest_path: str = ""
    lod_mode: str = "convert"  # carried from PhaseSelection; interpreted by regen LOD hook


@dataclass(slots=True)
class WorldspaceCellBounds:
    """Cell-coordinate window inside a single worldspace, used by bounded plugin-port.

    Inclusive on both ends: a 3x3 slice around (0,0) is (min=-1, max=1).
    """

    worldspace_editor_id: str
    min_x: int
    min_y: int
    max_x: int
    max_y: int
    include_worldspace_persistent_cell: bool = True


@dataclass
class PluginPortOptions:
    """Per-phase enable flags for plugin-port runs."""

    translate_records: bool = True
    convert_placed_records: bool = True
    convert_npc_faces: bool = False
    convert_terrain: bool = True
    reuse_terrain_navmesh: bool = False
    # Upgrade mode: source the terrain graft from this live deployed ESM instead
    # of the run-local ``.regen_land_cache.esm``. ``None`` = legacy cache path.
    terrain_graft_esm: Path | None = None
    convert_nifs: bool = False
    convert_btos: bool = False
    # Synthesize FO4 DistantLOD (MNAM) on converted LOD-capable bases so native
    # lodgen produces object LOD (.bto). Default-enabled when lod_mode=="generate".
    synthesize_object_lod: bool = False
    convert_textures: bool = False
    convert_materials: bool = False
    convert_havok: bool = False
    synthesize_drivers: bool = False
    convert_animations: bool = False
    generate_anim_text_data: bool = False
    # Force the CK-free native AnimTextData generator even when CreationKit.exe is
    # present (default prefers CK for full-fidelity all-bucket output).
    anim_text_data_native: bool = False
    validate_collision: bool = False
    convert_scripts: bool = True
    copy_sounds: bool = False
    build_esp: bool = True
    use_native_asset_collector: bool = False
    validate_output: bool = False
    validation_fail_on_error: bool = True
    conversion_workers: int | None = None
    disable_nif_collision_memo: bool = False
    records_limit: int | None = None
    fnv_unmapped_function_policy: Literal["halt", "skip_record"] = "halt"
    terrain: TerrainOptions = field(default_factory=TerrainOptions)
    cell_bounds: WorldspaceCellBounds | None = None
    placed_record_position_offset: tuple[float, float, float] = (0.0, 0.0, 0.0)
    exclude_signatures: frozenset[str] = frozenset()
    include_signatures: frozenset[str] = frozenset()
    overwrite_existing: bool = False
    force_cpu_textures: bool = False
    pbr_carry: bool = False
    texture_landscape_mip_flooding: bool = False
    convert_precombined_nifs: bool = True
    base_asset_relocation_mesh_roots: tuple[str, ...] = ()
    base_asset_namespace: str = ""
    papyrus_compiler: Literal["exe", "exe-batch", "native"] = "native"
    include_interior: bool = True
    carry_interior_previs: bool = False


@dataclass
class PluginPortRequest:
    """Headless input for a plugin-port conversion run."""

    source_game: str
    target_game: str
    source_plugins: list[Path]
    output_root: Path
    output_mod_name: str | None = None
    target_extracted_dir: Path | None = None
    target_data_dir: Path | None = None
    source_data_dir: Path | None = None
    additional_source_asset_roots: tuple[Path, ...] = ()
    target_asset_catalog_path: Path | None = None
    target_asset_cache_dir: Path | None = None
    target_master_paths: list[Path] = field(default_factory=list)
    options: PluginPortOptions = field(default_factory=PluginPortOptions)
    emit_authoring_yaml: bool = True
    source_worldspace_authoring_dir: Path | None = None
    diagnostics_root: Path | None = None


@dataclass
class ConvertedPluginRegistry:
    """Per-run source FormKey to resolved target FormKey map."""

    resolutions: dict[str, str | None] = field(default_factory=dict)


# ---------------------------------------------------------------------------
# Conversion decisions / run results
# ---------------------------------------------------------------------------


class ConversionDecisionKind(StrEnum):
    UNMAPPED_DROP = "unmapped_drop"
    EXPLICIT_DROP = "explicit_drop"
    TARGET_INVALID = "target_invalid"
    SCHEMA_DEFAULT = "schema_default"
    SEMANTIC_DEFAULT = "semantic_default"
    FORMKEY_REMAP = "formkey_remap"
    FORMKEY_NULL = "formkey_null"
    FORMKEY_ALLOCATION = "formkey_allocation"
    VALUE_TRANSFORM = "value_transform"


@dataclass(frozen=True)
class ConversionDecision:
    kind: ConversionDecisionKind
    record_type: str
    field: str
    reason: str
    source_value: Any = None
    target_value: Any = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "kind": self.kind.value,
            "record_type": self.record_type,
            "field": self.field,
            "reason": self.reason,
            "source_value": self.source_value,
            "target_value": self.target_value,
        }


@dataclass(frozen=True)
class FnvTranslationGap:
    function_or_av: str
    kind: Literal["function", "actor_value"]
    affected_records: list[str] = field(default_factory=list)
    sample_record_eid: str = ""


@dataclass(frozen=True)
class CreatureCoverage:
    target_name: str
    archetype: str
    clips_total: int
    clips_converted: int
    role_coverage_pct: float
    status: str


@dataclass
class RunResult:
    decisions: list[ConversionDecision] = field(default_factory=list)
    translated_counts: dict[str, int] = field(default_factory=dict)
    skipped_counts: dict[str, int] = field(default_factory=dict)
    failed_nifs: list[str] = field(default_factory=list)
    failed_textures: list[str] = field(default_factory=list)
    failed_bgsms: list[str] = field(default_factory=list)
    failed_faces: list[str] = field(default_factory=list)
    degraded_faces: list[str] = field(default_factory=list)
    fnv_translation_gaps: list[FnvTranslationGap] = field(default_factory=list)
    lip_regeneration_needed: list[str] = field(default_factory=list)


def write_coverage_report(
    out_path: "str | Path",
    *,
    decisions: list[object],
    translated_counts: dict[str, int],
    skipped_counts: dict[str, int],
    failed_nifs: list[str],
    failed_textures: list[str],
    failed_bgsms: list[str],
    creature_coverages: "list[CreatureCoverage] | None" = None,
) -> None:
    lines = ["# Conversion Report", ""]
    lines.extend(_count_table("Translated", translated_counts))
    lines.extend([""])
    lines.extend(_count_table("Skipped (V2)", skipped_counts))
    if creature_coverages:
        lines.extend([""])
        append_creature_section(lines, creature_coverages)
    lines.extend(["", "## Unmapped fields", "", "| Type.Field |", "|---|"])
    for decision in decisions:
        kind = _decision_value(decision, "kind")
        record_type = _decision_value(decision, "record_type")
        field_name = _decision_value(decision, "field")
        if kind == ConversionDecisionKind.UNMAPPED_DROP and record_type and field_name:
            lines.append(f"{record_type}.{field_name}")
    lines.extend([""])
    lines.extend(_path_list("Failed NIFs", failed_nifs))
    lines.extend([""])
    lines.extend(_path_list("Failed textures", failed_textures))
    lines.extend([""])
    lines.extend(_path_list("Failed BGSMs", failed_bgsms))
    Path(out_path).write_text("\n".join(lines) + "\n", encoding="utf-8")


def _decision_value(decision: object, name: str) -> Any:
    if isinstance(decision, dict):
        return decision.get(name)
    return getattr(decision, name, None)


def append_creature_section(
    md_lines: list[str],
    coverages: list[CreatureCoverage],
) -> None:
    md_lines.extend(
        [
            "## Creature coverage",
            "",
            "| target_name | archetype | clips | converted | role coverage | status |",
            "|---|---|---|---|---|---|",
        ]
    )
    for coverage in sorted(coverages, key=lambda item: (item.archetype, item.target_name)):
        md_lines.append(
            f"{coverage.target_name} | "
            f"{coverage.archetype} | "
            f"{coverage.clips_total} | "
            f"{coverage.clips_converted} | "
            f"{_format_coverage_pct(coverage.role_coverage_pct)} | "
            f"{coverage.status}"
        )


def _count_table(title: str, counts: dict[str, int]) -> list[str]:
    lines = [f"## {title}", "", "| Type | Count |", "|---|---|"]
    for record_type, count in sorted(counts.items()):
        lines.append(f"{record_type} | {count}")
    return lines


def _path_list(title: str, paths: list[str]) -> list[str]:
    lines = [f"## {title}", ""]
    lines.extend(f"- {path}" for path in paths)
    return lines


def _format_coverage_pct(value: float) -> str:
    text = f"{value:.1f}".rstrip("0").rstrip(".")
    return f"{text}%"
