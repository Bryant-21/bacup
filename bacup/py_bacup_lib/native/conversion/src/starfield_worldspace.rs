//! `starfield:fo4` exterior worldspace re-lattice.
//!
//! A Starfield exterior cell is 100 m across; an FO4 exterior cell is 4096
//! units = 58.5216 m (69.99125 units/m). The ratio (1.70878) is not an
//! integer, so a Starfield CELL grid and its block / sub-block labels cannot be
//! copied into an FO4 grid.
//!
//! This module owns the topology. `store2::translate_v2` has already landed
//! every translated record flat in the target (flat `CELL` / `REFR` / `WRLD`
//! top groups), and the `starfield_fo4` pair hook has scaled every placed
//! `DATA` position by 69.99125. What remains:
//!
//! 1. recover source parentage by walking the source world tree,
//! 2. bucket every placed child by the FO4 cell its scaled position implies,
//! 3. synthesize one FO4 CELL shell per occupied bucket,
//! 4. rebuild world-children → block → sub-block → cell-children from the new
//!    coordinates, and
//! 5. swap that tree in for the target's flat entries.
//!
//! No structurally valid placed child may be lost between steps 2 and 4: refs
//! with no readable position fall back to their source cell's origin, and refs
//! outside the source lattice are kept and counted. A placed child with no
//! usable base is invalid FO4 data and is dropped before nesting.
//!
//! `convert_terrain` runs later and emits its own CELL at the same coordinates.
//! `plugin_runtime::merge_projected_cell_into_subblock` keeps an existing
//! cell's child group only when EditorIDs match; a grid-only match deletes the
//! cell and its refs. `cell_editor_id` therefore matches
//! `terrain_native::authoring_emit`'s private helper byte for byte.
//!
//! Phase contract: no Python, no GIL. Events go through the run's channel.

use std::collections::{BTreeMap, HashMap, HashSet};

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{
    NativePluginSlot, ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord, WriteEffect,
    plugin_handle_next_available_object_id_no_py, plugin_handle_raise_next_object_id_no_py,
    plugin_handle_store_ref,
};
use smol_str::SmolStr;
use terrain_native::sf_frame;

use crate::phase::{LogLevel, PhaseEvent};
use crate::run::{ConversionRun, RunError};
use crate::target_write::{
    WorldspaceGroupRebuildStats, collect_target_records_by_form_id,
    remove_nested_records_from_flat_groups,
};

const TOP_GROUP: i32 = 0;
const WORLD_CHILDREN_GROUP: i32 = 1;
const INTERIOR_BLOCK_GROUP: i32 = 2;
const INTERIOR_SUB_BLOCK_GROUP: i32 = 3;
const EXTERIOR_BLOCK_GROUP: i32 = 4;
const EXTERIOR_SUB_BLOCK_GROUP: i32 = 5;
const CELL_CHILDREN_GROUP: i32 = 6;
const CELL_PERSISTENT_GROUP: i32 = 8;
const CELL_TEMPORARY_GROUP: i32 = 9;
const CELL_VISIBLE_DISTANT_GROUP: i32 = 10;

/// FO4 `CELL.DATA` flags for a synthesized exterior cell. Mirrors the value
/// `terrain_native::authoring_emit` writes for the cells it generates
/// (`u16_hex(2)`); the two must agree or the merge produces two cells that
/// differ only in flags.
const EXTERIOR_CELL_DATA_FLAGS: u16 = 0x0002;

/// Starfield ships metres; FO4 ships game units. Same constant as the pair
/// hook's `STARFIELD_METERS_TO_FO4_UNITS` and `sf_frame::FO4_UNITS_PER_METER`.
const METERS_TO_FO4_UNITS: f32 = 69.99125;

const FO4_CELL_UNITS: f32 = 4096.0;
const SF_CELL_METERS: f32 = 100.0;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RelatticeReport {
    pub worlds_relatticed: u32,
    pub worlds_skipped_already_nested: u32,
    pub source_cells_evacuated: u32,
    pub cells_synthesized: u32,
    pub cell_form_ids_reused: u32,
    pub cell_form_ids_allocated: u32,
    pub placed_refs_relatticed: u32,
    pub placed_refs_kept_persistent: u32,
    /// Placed children whose required `NAME` base is null or points to a
    /// translated output record that does not exist. Dropped before nesting.
    pub placed_refs_invalid_base_dropped: u32,
    /// Placed children with no readable `DATA` position. Bucketed by their
    /// source cell's origin instead of being dropped.
    pub placed_refs_without_position: u32,
    /// Placed children whose FO4 cell falls outside the FO4 window covered by
    /// the source lattice (i.e. outside the baked terrain extent). KEPT.
    pub placed_refs_outside_source_extent: u32,
    /// Source cell children with no mapped/translated target record.
    pub placed_refs_unmapped: u32,
    /// Cell children that are not placed records (LAND / NAVM / ...). These
    /// cannot be re-latticed; `convert_terrain` regenerates LAND on the new
    /// lattice and Starfield NAVM is out of MVP scope.
    pub non_placed_cell_children_dropped: u32,
    /// Interior CELLs moved out of the flat top-level `CELL` group into FO4's
    /// Block/Sub-Block topology, and the placed children nested under them.
    pub interior_cells_nested: u32,
    pub interior_placed_refs_nested: u32,
    pub interiors_skipped_already_nested: u32,
    pub flat_records_removed: u32,
}

/// Entry point, called from `ConversionRun::rebuild_full_plugin_worldspace_groups`.
pub(crate) fn relattice_worldspaces(run: &mut ConversionRun) -> Result<(), RunError> {
    let report = relattice_worldspaces_reporting(run)?;
    let message = format!(
        "starfield_relattice: worlds={} source_cells_evacuated={} cells_synthesized={} \
         (reused_ids={} allocated_ids={}) placed_relatticed={} placed_persistent={} \
         placed_invalid_base_dropped={} placed_no_position={} placed_outside_extent={} placed_unmapped={} \
         non_placed_dropped={} interior_cells_nested={} interior_placed_nested={}          interiors_skipped={} flat_removed={}",
        report.worlds_relatticed,
        report.source_cells_evacuated,
        report.cells_synthesized,
        report.cell_form_ids_reused,
        report.cell_form_ids_allocated,
        report.placed_refs_relatticed,
        report.placed_refs_kept_persistent,
        report.placed_refs_invalid_base_dropped,
        report.placed_refs_without_position,
        report.placed_refs_outside_source_extent,
        report.placed_refs_unmapped,
        report.non_placed_cell_children_dropped,
        report.interior_cells_nested,
        report.interior_placed_refs_nested,
        report.interiors_skipped_already_nested,
        report.flat_records_removed,
    );
    let sym = run.interner.intern(&message);
    run.warnings.push(sym);
    let _ = run.event_tx.try_send(PhaseEvent::Log {
        phase: "starfield_worldspace_relattice",
        level: LogLevel::Info,
        message,
    });
    Ok(())
}

pub(crate) fn relattice_worldspaces_reporting(
    run: &mut ConversionRun,
) -> Result<RelatticeReport, RunError> {
    let mut report = RelatticeReport::default();

    let source_to_target = source_to_target_raw_form_ids(run)?;
    if source_to_target.is_empty() {
        return Ok(report);
    }

    // Locks are taken and released one at a time: the object-id helpers below
    // acquire the plugin-handle store themselves, so nothing here may hold it
    // across a call into them.
    let (plans, interior_plans) = {
        let store = plugin_handle_store_ref()
            .lock()
            .map_err(|_| RunError::InvalidConfig("plugin handle store poisoned".into()))?;
        let slot = store.get(&run.source_handle_id).ok_or_else(|| {
            RunError::InvalidConfig(format!(
                "starfield_relattice: unknown source plugin handle: {}",
                run.source_handle_id
            ))
        })?;
        (
            collect_source_world_plans(&slot.parsed.root_items),
            collect_source_interior_plans(&slot.parsed.root_items),
        )
    };
    if plans.is_empty() && interior_plans.is_empty() {
        return Ok(report);
    }

    let first_object_id = plugin_handle_next_available_object_id_no_py(run.target_handle_id)
        .map_err(|e| RunError::InvalidConfig(format!("starfield_relattice: {e}")))?;
    let mut next_object_id = first_object_id;

    {
        let mut store = plugin_handle_store_ref()
            .lock()
            .map_err(|_| RunError::InvalidConfig("plugin handle store poisoned".into()))?;
        let target = store.get_mut(&run.target_handle_id).ok_or_else(|| {
            RunError::InvalidConfig(format!(
                "starfield_relattice: unknown target plugin handle: {}",
                run.target_handle_id
            ))
        })?;
        rebuild_target_worldspaces(
            target,
            &plans,
            &source_to_target,
            &mut next_object_id,
            &mut report,
        );
    }

    {
        let mut store = plugin_handle_store_ref()
            .lock()
            .map_err(|_| RunError::InvalidConfig("plugin handle store poisoned".into()))?;
        let target = store.get_mut(&run.target_handle_id).ok_or_else(|| {
            RunError::InvalidConfig(format!(
                "starfield_relattice: unknown target plugin handle: {}",
                run.target_handle_id
            ))
        })?;
        rebuild_target_interiors(target, &interior_plans, &source_to_target, &mut report);
    }

    if next_object_id > first_object_id {
        plugin_handle_raise_next_object_id_no_py(run.target_handle_id, next_object_id)
            .map_err(|e| RunError::InvalidConfig(format!("starfield_relattice: {e}")))?;
    }

    Ok(report)
}

fn source_to_target_raw_form_ids(run: &ConversionRun) -> Result<HashMap<u32, u32>, RunError> {
    let pairs = run
        .mapper_state
        .as_ref()
        .map(|state| {
            state
                .source_to_target
                .iter()
                .map(|(&source, &target)| (source, target))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if pairs.is_empty() {
        return Ok(HashMap::new());
    }
    let raw = crate::fo76_navmesh::raw_formid_mappings_for_context(
        pairs,
        &run.interner,
        run.source_handle_id,
        run.target_handle_id,
    )
    .map_err(|e| RunError::InvalidConfig(format!("starfield_relattice:{e}")))?;
    Ok(raw.into_iter().collect())
}

// ---------------------------------------------------------------------------
// Source topology
// ---------------------------------------------------------------------------

/// One `8`/`9`/`10` sub-group of a source cell-children group, in tree order.
struct SourceSection {
    group_type: i32,
    form_ids: Vec<u32>,
}

struct SourceCellPlan {
    form_id: u32,
    /// Starfield `XCLC` grid, when the source cell carries one.
    grid: Option<(i32, i32)>,
    sections: Vec<SourceSection>,
}

struct SourceWorldPlan {
    world_form_id: u32,
    editor_id: String,
    /// Cells listed directly under the world-children group. In a Bethesda
    /// worldspace this is the persistent cell that owns the type-8 persistent
    /// ref group; it is kept verbatim and never re-latticed.
    persistent_cells: Vec<SourceCellPlan>,
    /// Cells under block / sub-block groups — the lattice being replaced.
    lattice_cells: Vec<SourceCellPlan>,
}

fn collect_source_world_plans(root_items: &[ParsedItem]) -> Vec<SourceWorldPlan> {
    let Some(wrld_top) = find_top_group(root_items, b"WRLD") else {
        return Vec::new();
    };
    let mut plans = Vec::new();
    for item in &wrld_top.children {
        let ParsedItem::Record(record) = item else {
            continue;
        };
        if record.signature.as_str() != "WRLD" {
            continue;
        }
        let Some(children) = find_group(&wrld_top.children, WORLD_CHILDREN_GROUP, record.form_id)
        else {
            continue;
        };
        let mut plan = SourceWorldPlan {
            world_form_id: record.form_id,
            editor_id: editor_id(record).unwrap_or_default(),
            persistent_cells: Vec::new(),
            lattice_cells: Vec::new(),
        };
        collect_cells(&children.children, &mut plan.persistent_cells);
        for block in groups_of_type(&children.children, EXTERIOR_BLOCK_GROUP) {
            for sub_block in groups_of_type(&block.children, EXTERIOR_SUB_BLOCK_GROUP) {
                collect_cells(&sub_block.children, &mut plan.lattice_cells);
            }
        }
        if plan.persistent_cells.is_empty() && plan.lattice_cells.is_empty() {
            continue;
        }
        plans.push(plan);
    }
    plans
}

/// Source interior cells: `GRUP(0,"CELL") -> GRUP(2) -> GRUP(3) -> CELL`.
///
/// `starfield_to_fo4.yaml` deliberately lets CELL/REFR land flat and defers
/// topology to this module, but until this existed only worldspaces were ever
/// rebuilt: every interior CELL and its placed children stayed in the flat
/// top-level groups. FO4 then instantiates all of them at data-handler init
/// and the load never finishes.
fn collect_source_interior_plans(root_items: &[ParsedItem]) -> Vec<SourceCellPlan> {
    let Some(cell_top) = find_top_group(root_items, b"CELL") else {
        return Vec::new();
    };
    let mut cells = Vec::new();
    for block in groups_of_type(&cell_top.children, INTERIOR_BLOCK_GROUP) {
        for sub_block in groups_of_type(&block.children, INTERIOR_SUB_BLOCK_GROUP) {
            collect_cells(&sub_block.children, &mut cells);
        }
    }
    cells
}

fn collect_cells(items: &[ParsedItem], out: &mut Vec<SourceCellPlan>) {
    for item in items {
        let ParsedItem::Record(record) = item else {
            continue;
        };
        if record.signature.as_str() != "CELL" {
            continue;
        }
        out.push(SourceCellPlan {
            form_id: record.form_id,
            grid: cell_grid(record),
            sections: cell_sections(items, record.form_id),
        });
    }
}

fn cell_sections(items: &[ParsedItem], cell_form_id: u32) -> Vec<SourceSection> {
    let Some(group) = find_group(items, CELL_CHILDREN_GROUP, cell_form_id) else {
        return Vec::new();
    };
    let mut sections = Vec::new();
    let mut loose = Vec::new();
    for child in &group.children {
        match child {
            // A record directly under the cell-children group is malformed but
            // survivable; treat it as temporary rather than losing it.
            ParsedItem::Record(record) => loose.push(record.form_id),
            ParsedItem::Group(sub_group) => {
                let mut form_ids = Vec::new();
                collect_record_form_ids(&sub_group.children, &mut form_ids);
                if !form_ids.is_empty() {
                    sections.push(SourceSection {
                        group_type: sub_group.group_type,
                        form_ids,
                    });
                }
            }
        }
    }
    if !loose.is_empty() {
        sections.push(SourceSection {
            group_type: CELL_TEMPORARY_GROUP,
            form_ids: loose,
        });
    }
    sections
}

fn collect_record_form_ids(items: &[ParsedItem], out: &mut Vec<u32>) {
    for item in items {
        match item {
            ParsedItem::Record(record) => out.push(record.form_id),
            ParsedItem::Group(group) => collect_record_form_ids(&group.children, out),
        }
    }
}

// ---------------------------------------------------------------------------
// Target rebuild
// ---------------------------------------------------------------------------

/// Placed child bound for a synthesized cell, in emit order.
struct BucketedRef {
    section_group_type: i32,
    target_form_id: u32,
}

struct SynthesizedCell {
    grid: (i32, i32),
    form_id: u32,
    refs: Vec<BucketedRef>,
}

fn rebuild_target_worldspaces(
    target: &mut NativePluginSlot,
    plans: &[SourceWorldPlan],
    source_to_target: &HashMap<u32, u32>,
    next_object_id: &mut u32,
    report: &mut RelatticeReport,
) {
    let group_tail = Bytes::from(vec![0u8; target.parsed.header_size.saturating_sub(16)]);
    let own_index = (target.parsed.header.masters.len() & 0x00FF) as u32;

    // `removed` covers both records nested into the new tree (their flat copy
    // must go) and evacuated source CELLs (deleted outright).
    let mut removed: HashSet<u32> = HashSet::new();
    let mut rebuilt_worlds: Vec<(u32, ParsedRecord, ParsedGroup)> = Vec::new();

    {
        let mut records_by_form_id = HashMap::new();
        collect_target_records_by_form_id(&target.parsed.root_items, &mut records_by_form_id);
        let existing_world_children: HashSet<u32> =
            find_top_group(&target.parsed.root_items, b"WRLD")
                .map(|group| {
                    group
                        .children
                        .iter()
                        .filter_map(|item| match item {
                            ParsedItem::Group(child)
                                if child.group_type == WORLD_CHILDREN_GROUP =>
                            {
                                Some(u32::from_le_bytes(child.label))
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();

        for plan in plans {
            let Some(&world_target_id) = source_to_target.get(&plan.world_form_id) else {
                continue;
            };
            // Idempotency: a world that already carries a world-children group
            // has been re-latticed; rebuilding it from now-nested flat records
            // would produce an empty tree and delete the previous result.
            if existing_world_children.contains(&world_target_id) {
                report.worlds_skipped_already_nested += 1;
                continue;
            }
            let Some(&world_record) = records_by_form_id.get(&world_target_id) else {
                continue;
            };
            // `world_removed` is merged into the global removal set only when
            // the world actually produces a tree. Merging unconditionally
            // would let the `None` path delete records from the flat groups
            // with nothing nesting them — a silent drop.
            let mut world_removed = HashSet::new();
            if let Some(rebuilt) = rebuild_world(
                plan,
                world_target_id,
                world_record,
                &records_by_form_id,
                source_to_target,
                own_index,
                next_object_id,
                &group_tail,
                &mut world_removed,
                report,
            ) {
                removed.extend(world_removed);
                rebuilt_worlds.push((world_target_id, rebuilt.0, rebuilt.1));
            }
        }
    }

    if rebuilt_worlds.is_empty() {
        return;
    }
    report.worlds_relatticed = rebuilt_worlds.len() as u32;

    let mut by_world: HashMap<u32, (ParsedRecord, ParsedGroup)> = rebuilt_worlds
        .into_iter()
        .map(|(id, record, group)| (id, (record, group)))
        .collect();

    let mut stats = WorldspaceGroupRebuildStats::default();
    let mut root_items = Vec::with_capacity(target.parsed.root_items.len());
    for item in target.parsed.root_items.drain(..) {
        let is_wrld_top = matches!(
            &item,
            ParsedItem::Group(group)
                if group.group_type == TOP_GROUP && group.label == *b"WRLD"
        );
        if is_wrld_top {
            let ParsedItem::Group(mut wrld_top) = item else {
                unreachable!("is_wrld_top implies ParsedItem::Group")
            };
            let mut children = Vec::with_capacity(wrld_top.children.len());
            for child in wrld_top.children.drain(..) {
                match child {
                    ParsedItem::Record(record)
                        if record.signature.as_str() == "WRLD"
                            && by_world.contains_key(&record.form_id) =>
                    {
                        let (patched, world_children) = by_world.remove(&record.form_id).unwrap();
                        children.push(ParsedItem::Record(patched));
                        children.push(ParsedItem::Group(world_children));
                    }
                    other => children.push(other),
                }
            }
            wrld_top.children = children;
            root_items.push(ParsedItem::Group(wrld_top));
            continue;
        }
        if let Some(filtered) = remove_nested_records_from_flat_groups(item, &removed, &mut stats) {
            root_items.push(filtered);
        }
    }

    target.parsed.root_items = root_items;
    target.clear_record_count_cache();
    target.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    report.flat_records_removed = stats.flat_records_removed as u32;
}

/// FO4 interior bucket for a cell: `block = obj % 10`, `sub = (obj / 10) % 10`
/// over the TARGET object id, labels written as `i32`. Measured against
/// `Fallout4.esm`: 1195/1195 of its interior cells satisfy this exactly.
fn interior_bucket(target_cell_id: u32) -> (i32, i32) {
    let obj = target_cell_id & 0x00FF_FFFF;
    ((obj % 10) as i32, ((obj / 10) % 10) as i32)
}

/// Move flat interior CELLs and their placed children into FO4's
/// `GRUP(0,"CELL") -> GRUP(2) -> GRUP(3) -> CELL -> GRUP(6) -> GRUP(8|9)`.
///
/// Parentage is group topology, so a placed ref left flat belongs to no cell:
/// FO4 instantiates every one of them during data-handler init and inserts it
/// into a global list with a linear search, which is quadratic and never
/// completes. Nesting them restores per-cell streaming.
fn rebuild_target_interiors(
    target: &mut NativePluginSlot,
    plans: &[SourceCellPlan],
    source_to_target: &HashMap<u32, u32>,
    report: &mut RelatticeReport,
) {
    if plans.is_empty() {
        return;
    }
    // Idempotency, mirroring the worldspace path: a CELL top group that already
    // carries block groups has been nested, and rebuilding it from the
    // now-empty flat set would delete the previous result.
    match find_top_group(&target.parsed.root_items, b"CELL") {
        None => return,
        Some(cell_top) => {
            if cell_top.children.iter().any(|item| {
                matches!(item, ParsedItem::Group(group) if group.group_type == INTERIOR_BLOCK_GROUP)
            }) {
                report.interiors_skipped_already_nested += 1;
                return;
            }
        }
    }

    let group_tail = Bytes::from(vec![0u8; target.parsed.header_size.saturating_sub(16)]);
    let own_index = (target.parsed.header.masters.len() & 0x00FF) as u32;

    let mut removed: HashSet<u32> = HashSet::new();
    let mut buckets: BTreeMap<(i32, i32), Vec<ParsedItem>> = BTreeMap::new();
    {
        let mut records_by_form_id = HashMap::new();
        collect_target_records_by_form_id(&target.parsed.root_items, &mut records_by_form_id);
        for cell in plans {
            let Some(&target_cell_id) = source_to_target.get(&cell.form_id) else {
                continue;
            };
            let Some(&record) = records_by_form_id.get(&target_cell_id) else {
                continue;
            };
            let children = rebuild_persistent_cell_children(
                cell,
                target_cell_id,
                &records_by_form_id,
                source_to_target,
                own_index,
                &group_tail,
                true,
                &mut removed,
                report,
            );
            removed.insert(target_cell_id);
            report.interior_cells_nested += 1;
            let bucket = buckets.entry(interior_bucket(target_cell_id)).or_default();
            bucket.push(ParsedItem::Record(record.clone()));
            if let Some(group) = children {
                bucket.push(ParsedItem::Group(group));
            }
        }
    }
    if buckets.is_empty() {
        return;
    }

    let mut by_block: BTreeMap<i32, BTreeMap<i32, Vec<ParsedItem>>> = BTreeMap::new();
    for ((block, sub), items) in buckets {
        by_block.entry(block).or_default().insert(sub, items);
    }

    let mut stats = WorldspaceGroupRebuildStats::default();
    let mut root_items = Vec::with_capacity(target.parsed.root_items.len());
    for item in target.parsed.root_items.drain(..) {
        let is_cell_top = matches!(
            &item,
            ParsedItem::Group(group)
                if group.group_type == TOP_GROUP && group.label == *b"CELL"
        );
        if !is_cell_top {
            // Placed children live in the flat top-level `REFR` group, so the
            // removal sweep has to run over every other root item too.
            if let Some(filtered) =
                remove_nested_records_from_flat_groups(item, &removed, &mut stats)
            {
                root_items.push(filtered);
            }
            continue;
        }
        let ParsedItem::Group(mut cell_top) = item else {
            unreachable!("is_cell_top implies ParsedItem::Group")
        };
        // Anything the source plan did not claim stays exactly where it was.
        let mut children = Vec::with_capacity(cell_top.children.len());
        for child in cell_top.children.drain(..) {
            if let Some(kept) = remove_nested_records_from_flat_groups(child, &removed, &mut stats)
            {
                children.push(kept);
            }
        }
        for (block, subs) in std::mem::take(&mut by_block) {
            children.push(ParsedItem::Group(ParsedGroup {
                label: block.to_le_bytes(),
                group_type: INTERIOR_BLOCK_GROUP,
                tail: group_tail.clone(),
                children: subs
                    .into_iter()
                    .map(|(sub, items)| {
                        ParsedItem::Group(ParsedGroup {
                            label: sub.to_le_bytes(),
                            group_type: INTERIOR_SUB_BLOCK_GROUP,
                            tail: group_tail.clone(),
                            children: items,
                        })
                    })
                    .collect(),
            }));
        }
        cell_top.children = children;
        root_items.push(ParsedItem::Group(cell_top));
    }

    target.parsed.root_items = root_items;
    target.clear_record_count_cache();
    target.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    report.flat_records_removed += stats.flat_records_removed as u32;
}

#[allow(clippy::too_many_arguments)]
fn rebuild_world(
    plan: &SourceWorldPlan,
    world_target_id: u32,
    world_record: &ParsedRecord,
    records_by_form_id: &HashMap<u32, &ParsedRecord>,
    source_to_target: &HashMap<u32, u32>,
    own_index: u32,
    next_object_id: &mut u32,
    group_tail: &Bytes,
    removed: &mut HashSet<u32>,
    report: &mut RelatticeReport,
) -> Option<(ParsedRecord, ParsedGroup)> {
    // --- source cell index, used for payload inheritance + fallback siting ---
    let mut source_cell_by_grid: HashMap<(i32, i32), &SourceCellPlan> = HashMap::new();
    let mut extent: Option<((i32, i32), (i32, i32))> = None;
    for cell in &plan.lattice_cells {
        if let Some(grid) = cell.grid {
            source_cell_by_grid.entry(grid).or_insert(cell);
            extent = Some(match extent {
                None => (grid, grid),
                Some((min, max)) => (
                    (min.0.min(grid.0), min.1.min(grid.1)),
                    (max.0.max(grid.0), max.1.max(grid.1)),
                ),
            });
        }
    }

    // --- bucket every placed child by its (already scaled) position ----------
    let mut buckets: BTreeMap<(i32, i32), Vec<BucketedRef>> = BTreeMap::new();
    for cell in &plan.lattice_cells {
        report.source_cells_evacuated += 1;
        if let Some(&target_cell_id) = source_to_target.get(&cell.form_id) {
            removed.insert(target_cell_id);
        }
        for section in &cell.sections {
            for &source_child in &section.form_ids {
                let Some(&target_child) = source_to_target.get(&source_child) else {
                    report.placed_refs_unmapped += 1;
                    continue;
                };
                let Some(record) = records_by_form_id.get(&target_child) else {
                    report.placed_refs_unmapped += 1;
                    continue;
                };
                if !is_placed_child_signature(record.signature.as_str()) {
                    report.non_placed_cell_children_dropped += 1;
                    continue;
                }
                if !placed_child_has_valid_base(record, records_by_form_id, own_index) {
                    removed.insert(target_child);
                    report.placed_refs_invalid_base_dropped += 1;
                    continue;
                }
                let grid = match placed_position(record) {
                    Some((x, y)) => (
                        sf_frame::fo4_cell_of_units(x),
                        sf_frame::fo4_cell_of_units(y),
                    ),
                    None => {
                        report.placed_refs_without_position += 1;
                        source_cell_origin_fo4_grid(cell.grid)
                    }
                };
                removed.insert(target_child);
                report.placed_refs_relatticed += 1;
                buckets.entry(grid).or_default().push(BucketedRef {
                    section_group_type: normalize_section_group_type(section.group_type),
                    target_form_id: target_child,
                });
            }
        }
    }

    if let Some((min, max)) = extent {
        let (x_lo, x_hi) = sf_frame::fo4_cell_range(min.0, max.0);
        let (y_lo, y_hi) = sf_frame::fo4_cell_range(min.1, max.1);
        for ((cx, cy), refs) in &buckets {
            if *cx < x_lo || *cx > x_hi || *cy < y_lo || *cy > y_hi {
                report.placed_refs_outside_source_extent += refs.len() as u32;
            }
        }
    }

    // --- FormIDs: recycle the evacuated cells' ids before allocating new -----
    let mut reusable: Vec<((i32, i32), u32)> = plan
        .lattice_cells
        .iter()
        .filter_map(|cell| {
            let grid = cell.grid?;
            let target_id = *source_to_target.get(&cell.form_id)?;
            Some((grid, target_id))
        })
        .collect();
    reusable.sort_by_key(|((x, y), id)| (*y, *x, *id));
    let mut reusable = reusable.into_iter().map(|(_, id)| id);

    let mut cells: Vec<SynthesizedCell> = Vec::with_capacity(buckets.len());
    let mut ordered: Vec<((i32, i32), Vec<BucketedRef>)> = buckets.into_iter().collect();
    ordered.sort_by_key(|((x, y), _)| (*y, *x));
    for (grid, refs) in ordered {
        let form_id = match reusable.next() {
            Some(id) => {
                report.cell_form_ids_reused += 1;
                id
            }
            None => {
                let id = (own_index << 24) | (*next_object_id & 0x00FF_FFFF);
                *next_object_id += 1;
                report.cell_form_ids_allocated += 1;
                id
            }
        };
        cells.push(SynthesizedCell {
            grid,
            form_id,
            refs,
        });
    }
    report.cells_synthesized += cells.len() as u32;

    // --- world-children tree ------------------------------------------------
    let mut world_children = ParsedGroup {
        label: world_target_id.to_le_bytes(),
        group_type: WORLD_CHILDREN_GROUP,
        tail: group_tail.clone(),
        children: Vec::new(),
    };

    for persistent in &plan.persistent_cells {
        let Some(&target_cell_id) = source_to_target.get(&persistent.form_id) else {
            continue;
        };
        let Some(record) = records_by_form_id.get(&target_cell_id) else {
            continue;
        };
        removed.insert(target_cell_id);
        world_children
            .children
            .push(ParsedItem::Record((*record).clone()));
        if let Some(group) = rebuild_persistent_cell_children(
            persistent,
            target_cell_id,
            records_by_form_id,
            source_to_target,
            own_index,
            group_tail,
            false,
            removed,
            report,
        ) {
            world_children.children.push(ParsedItem::Group(group));
        }
    }

    let mut blocks: BTreeMap<(i32, i32), BTreeMap<(i32, i32), Vec<&SynthesizedCell>>> =
        BTreeMap::new();
    for cell in &cells {
        let block = (
            sf_frame::fo4_exterior_block(cell.grid.0),
            sf_frame::fo4_exterior_block(cell.grid.1),
        );
        let sub_block = (
            sf_frame::fo4_exterior_sub_block(cell.grid.0),
            sf_frame::fo4_exterior_sub_block(cell.grid.1),
        );
        blocks
            .entry((block.1, block.0))
            .or_default()
            .entry((sub_block.1, sub_block.0))
            .or_default()
            .push(cell);
    }

    for ((block_y, block_x), sub_blocks) in blocks {
        let mut block_group = ParsedGroup {
            label: encode_exterior_grid_label(clamp_i16(block_x), clamp_i16(block_y)),
            group_type: EXTERIOR_BLOCK_GROUP,
            tail: group_tail.clone(),
            children: Vec::new(),
        };
        for ((sub_y, sub_x), sub_cells) in sub_blocks {
            let mut sub_block_group = ParsedGroup {
                label: encode_exterior_grid_label(clamp_i16(sub_x), clamp_i16(sub_y)),
                group_type: EXTERIOR_SUB_BLOCK_GROUP,
                tail: group_tail.clone(),
                children: Vec::new(),
            };
            for cell in sub_cells {
                let payload_source = payload_grid_candidates(cell.grid)
                    .into_iter()
                    .find_map(|candidate| source_cell_by_grid.get(&candidate))
                    .and_then(|source| source_to_target.get(&source.form_id))
                    .and_then(|target_id| records_by_form_id.get(target_id))
                    .copied();
                sub_block_group
                    .children
                    .push(ParsedItem::Record(synthesize_cell_record(
                        &plan.editor_id,
                        cell,
                        payload_source,
                    )));
                sub_block_group
                    .children
                    .push(ParsedItem::Group(cell_children_group(
                        cell,
                        records_by_form_id,
                        group_tail,
                    )));
            }
            block_group
                .children
                .push(ParsedItem::Group(sub_block_group));
        }
        world_children.children.push(ParsedItem::Group(block_group));
    }

    if world_children.children.is_empty() {
        return None;
    }

    let patched = patch_world_record(world_record, &cells);
    Some((patched, world_children))
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn rebuild_persistent_cell_children(
    cell: &SourceCellPlan,
    target_cell_id: u32,
    records_by_form_id: &HashMap<u32, &ParsedRecord>,
    source_to_target: &HashMap<u32, u32>,
    own_index: u32,
    group_tail: &Bytes,
    interior: bool,
    removed: &mut HashSet<u32>,
    report: &mut RelatticeReport,
) -> Option<ParsedGroup> {
    let mut group = ParsedGroup {
        label: target_cell_id.to_le_bytes(),
        group_type: CELL_CHILDREN_GROUP,
        tail: group_tail.clone(),
        children: Vec::new(),
    };
    for section in &cell.sections {
        let mut children = Vec::new();
        for &source_child in &section.form_ids {
            let Some(&target_child) = source_to_target.get(&source_child) else {
                report.placed_refs_unmapped += 1;
                continue;
            };
            let Some(record) = records_by_form_id.get(&target_child) else {
                report.placed_refs_unmapped += 1;
                continue;
            };
            if !is_placed_child_signature(record.signature.as_str()) {
                report.non_placed_cell_children_dropped += 1;
                continue;
            }
            if !placed_child_has_valid_base(record, records_by_form_id, own_index) {
                removed.insert(target_child);
                report.placed_refs_invalid_base_dropped += 1;
                continue;
            }
            removed.insert(target_child);
            if interior {
                report.interior_placed_refs_nested += 1;
            } else {
                report.placed_refs_kept_persistent += 1;
            }
            children.push(ParsedItem::Record((*record).clone()));
        }
        if children.is_empty() {
            continue;
        }
        group.children.push(ParsedItem::Group(ParsedGroup {
            label: target_cell_id.to_le_bytes(),
            group_type: section.group_type,
            tail: group_tail.clone(),
            children,
        }));
    }
    if group.children.is_empty() {
        return None;
    }
    Some(group)
}

fn cell_children_group(
    cell: &SynthesizedCell,
    records_by_form_id: &HashMap<u32, &ParsedRecord>,
    group_tail: &Bytes,
) -> ParsedGroup {
    let mut sections: BTreeMap<i32, Vec<ParsedItem>> = BTreeMap::new();
    for entry in &cell.refs {
        let Some(record) = records_by_form_id.get(&entry.target_form_id) else {
            continue;
        };
        sections
            .entry(entry.section_group_type)
            .or_default()
            .push(ParsedItem::Record((*record).clone()));
    }
    ParsedGroup {
        label: cell.form_id.to_le_bytes(),
        group_type: CELL_CHILDREN_GROUP,
        tail: group_tail.clone(),
        children: sections
            .into_iter()
            .map(|(group_type, children)| {
                ParsedItem::Group(ParsedGroup {
                    label: cell.form_id.to_le_bytes(),
                    group_type,
                    tail: group_tail.clone(),
                    children,
                })
            })
            .collect(),
    }
}

fn synthesize_cell_record(
    world_editor_id: &str,
    cell: &SynthesizedCell,
    payload_source: Option<&ParsedRecord>,
) -> ParsedRecord {
    let mut subrecords = vec![
        subrecord(
            "EDID",
            zstring(&cell_editor_id(world_editor_id, cell.grid.0, cell.grid.1)),
        ),
        subrecord("DATA", EXTERIOR_CELL_DATA_FLAGS.to_le_bytes().to_vec()),
        subrecord("XCLC", exterior_grid_payload(cell.grid.0, cell.grid.1)),
    ];

    // Exterior lighting is not replicated: FO4 exterior cells inherit the
    // worldspace lighting (as in the FO76 pair), so XCLL / LTMP are dropped
    // rather than copied onto ~2.92x as many cells. XCLR (Regions) is not
    // carried. Water, location, image space and region-weather have direct FO4
    // equivalents and come from the source cell with the largest overlap, read
    // from the translated target cell so their FormIDs are already remapped.
    if let Some(source) = payload_source {
        if let Some(height) = find_subrecord(source, "XCLW") {
            subrecords.push(subrecord("XCLW", scaled_water_height(height)));
        }
        for sig in ["XLCN", "XCWT", "XCCM", "XCIM"] {
            if let Some(data) = find_subrecord(source, sig)
                && !is_null_form_id_payload(data)
            {
                subrecords.push(subrecord(sig, data.to_vec()));
            }
        }
    }

    ParsedRecord {
        signature: SmolStr::new_static("CELL"),
        form_id: cell.form_id,
        flags: 0,
        version_control: 0,
        form_version: Some(131),
        version2: Some(1),
        subrecords,
        raw_payload: None,
        parse_error: None,
    }
}

/// `NAM0`/`NAM9` are the worldspace's object bounds and `DNAM` its default
/// land/water heights. The pair hook deliberately leaves all three alone
/// because they are topology, not a per-record field: the bounds must be
/// recomputed from the NEW lattice (not scaled from the Starfield grid) and
/// `DNAM`'s two heights are Starfield metres.
fn patch_world_record(world_record: &ParsedRecord, cells: &[SynthesizedCell]) -> ParsedRecord {
    let mut patched = (*world_record).clone();
    patched.raw_payload = None;

    let bounds = lattice_bounds(cells);
    for subrecord in &mut patched.subrecords {
        match subrecord.signature.as_str() {
            "NAM0" => {
                if let Some((min, _)) = bounds {
                    subrecord.data = Bytes::from(float_pair(min.0, min.1));
                }
            }
            "NAM9" => {
                if let Some((_, max)) = bounds {
                    subrecord.data = Bytes::from(float_pair(max.0, max.1));
                }
            }
            "DNAM" => {
                if subrecord.data.len() == 8 {
                    let land = read_f32(&subrecord.data, 0) * METERS_TO_FO4_UNITS;
                    let water = read_f32(&subrecord.data, 4) * METERS_TO_FO4_UNITS;
                    subrecord.data = Bytes::from(float_pair(land, water));
                }
            }
            _ => {}
        }
    }
    patched
}

fn lattice_bounds(cells: &[SynthesizedCell]) -> Option<((f32, f32), (f32, f32))> {
    let first = cells.first()?;
    let mut min = first.grid;
    let mut max = first.grid;
    for cell in cells {
        min = (min.0.min(cell.grid.0), min.1.min(cell.grid.1));
        max = (max.0.max(cell.grid.0), max.1.max(cell.grid.1));
    }
    Some((
        (min.0 as f32 * FO4_CELL_UNITS, min.1 as f32 * FO4_CELL_UNITS),
        (
            (max.0 + 1) as f32 * FO4_CELL_UNITS,
            (max.1 + 1) as f32 * FO4_CELL_UNITS,
        ),
    ))
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Mirrors the private `terrain_native::authoring_emit::cell_editor_id`.
/// `convert_terrain` runs after this module and merges its generated cell into
/// the same sub-block; that merge only preserves an existing cell's placed-ref
/// child group when the EditorIDs match, so this string must stay byte-exact.
fn cell_editor_id(world_editor_id: &str, cell_x: i32, cell_y: i32) -> String {
    format!(
        "{}CellX{}Y{}",
        world_editor_id,
        coordinate_token(cell_x),
        coordinate_token(cell_y)
    )
}

fn coordinate_token(value: i32) -> String {
    let sign = if value < 0 { 'N' } else { 'P' };
    format!("{sign}{:03}", value.unsigned_abs())
}

/// Mirrors `esp_authoring_core::plugin_runtime::encode_exterior_grid_label`
/// (private): the Y index occupies the low half of the label, X the high half.
/// Never re-derive this byte order from memory.
fn encode_exterior_grid_label(x: i16, y: i16) -> [u8; 4] {
    [
        y.to_le_bytes()[0],
        y.to_le_bytes()[1],
        x.to_le_bytes()[0],
        x.to_le_bytes()[1],
    ]
}

/// Mirrors the private `plugin_runtime::is_placed_child_signature`.
fn is_placed_child_signature(signature: &str) -> bool {
    matches!(signature, "REFR" | "ACHR" | "PHZD" | "PGRE" | "PGRD")
}

/// FO4 exterior cells whose refs came from an unreadable position fall back to
/// the FO4 cell containing their source cell's origin, so they are re-sited
/// rather than dropped.
fn source_cell_origin_fo4_grid(grid: Option<(i32, i32)>) -> (i32, i32) {
    let (x, y) = grid.unwrap_or((0, 0));
    (
        sf_frame::fo4_cell_of_units(x as f32 * SF_CELL_METERS * METERS_TO_FO4_UNITS),
        sf_frame::fo4_cell_of_units(y as f32 * SF_CELL_METERS * METERS_TO_FO4_UNITS),
    )
}

/// Source grids to try, best first, when inheriting an FO4 cell's per-cell
/// payload (water / location / image space). An FO4 cell (58.52 m) overlaps at
/// most two Starfield cells (100 m) per axis, so this is at most four
/// candidates; the largest-overlap pair leads. The fallbacks matter at the
/// lattice edge, where the dominant Starfield cell can lie outside the baked
/// extent — inheriting from the smaller overlap beats inheriting nothing.
fn payload_grid_candidates(grid: (i32, i32)) -> Vec<(i32, i32)> {
    let xs = overlap_axis_candidates(grid.0);
    let ys = overlap_axis_candidates(grid.1);
    let mut ranked: Vec<(usize, i32, i32)> = Vec::with_capacity(xs.len() * ys.len());
    for (xi, x) in xs.iter().enumerate() {
        for (yi, y) in ys.iter().enumerate() {
            ranked.push((xi + yi, *x, *y));
        }
    }
    ranked.sort_by_key(|(rank, _, _)| *rank);
    ranked.into_iter().map(|(_, x, y)| (x, y)).collect()
}

fn overlap_axis_candidates(cell: i32) -> Vec<i32> {
    let overlap = sf_frame::sf_cells_overlapping_fo4_cell(cell);
    if overlap.first == overlap.last {
        return vec![overlap.first];
    }
    if overlap.first_fraction >= 0.5 {
        vec![overlap.first, overlap.last]
    } else {
        vec![overlap.last, overlap.first]
    }
}

fn normalize_section_group_type(group_type: i32) -> i32 {
    match group_type {
        CELL_PERSISTENT_GROUP | CELL_TEMPORARY_GROUP | CELL_VISIBLE_DISTANT_GROUP => group_type,
        _ => CELL_TEMPORARY_GROUP,
    }
}

fn clamp_i16(value: i32) -> i16 {
    value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn find_top_group<'a>(items: &'a [ParsedItem], label: &[u8; 4]) -> Option<&'a ParsedGroup> {
    items.iter().find_map(|item| match item {
        ParsedItem::Group(group) if group.group_type == TOP_GROUP && group.label == *label => {
            Some(group)
        }
        _ => None,
    })
}

fn find_group(items: &[ParsedItem], group_type: i32, label_form_id: u32) -> Option<&ParsedGroup> {
    items.iter().find_map(|item| match item {
        ParsedItem::Group(group)
            if group.group_type == group_type
                && u32::from_le_bytes(group.label) == label_form_id =>
        {
            Some(group)
        }
        _ => None,
    })
}

fn groups_of_type<'a>(
    items: &'a [ParsedItem],
    group_type: i32,
) -> impl Iterator<Item = &'a ParsedGroup> + 'a {
    items.iter().filter_map(move |item| match item {
        ParsedItem::Group(group) if group.group_type == group_type => Some(group),
        _ => None,
    })
}

fn editor_id(record: &ParsedRecord) -> Option<String> {
    let bytes = find_subrecord(record, "EDID")?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).ok().map(str::to_owned)
}

fn find_subrecord<'a>(record: &'a ParsedRecord, signature: &str) -> Option<&'a [u8]> {
    record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == signature)
        .map(|subrecord| subrecord.data.as_ref())
}

fn is_null_form_id_payload(data: &[u8]) -> bool {
    data.len() == 4 && data.iter().all(|byte| *byte == 0)
}

fn placed_child_has_valid_base(
    record: &ParsedRecord,
    records_by_form_id: &HashMap<u32, &ParsedRecord>,
    own_index: u32,
) -> bool {
    let Some(data) = find_subrecord(record, "NAME") else {
        return false;
    };
    let Ok(raw) = <[u8; 4]>::try_from(data) else {
        return false;
    };
    let base_form_id = u32::from_le_bytes(raw);
    if base_form_id == 0 {
        return false;
    }
    if base_form_id >> 24 != own_index {
        return true;
    }
    records_by_form_id.contains_key(&base_form_id)
}

fn cell_grid(record: &ParsedRecord) -> Option<(i32, i32)> {
    let data = find_subrecord(record, "XCLC")?;
    if data.len() < 8 {
        return None;
    }
    Some((read_i32(data, 0), read_i32(data, 4)))
}

/// `DATA` on a placed record is `position(3 x f32) + rotation(3 x f32)`. By
/// the time this runs the pair hook has already multiplied the position by
/// 69.99125, so these are FO4 units.
fn placed_position(record: &ParsedRecord) -> Option<(f32, f32)> {
    let data = find_subrecord(record, "DATA")?;
    if data.len() < 12 {
        return None;
    }
    Some((read_f32(data, 0), read_f32(data, 4)))
}

fn scaled_water_height(data: &[u8]) -> Vec<u8> {
    if data.len() != 4 {
        return data.to_vec();
    }
    let height = read_f32(data, 0);
    // FFFF7F7F (~f32::MAX) is FO4's "use the worldspace default" sentinel;
    // scaling it would overflow to infinity.
    if !height.is_finite() || height.abs() > 1.0e30 {
        return data.to_vec();
    }
    (height * METERS_TO_FO4_UNITS).to_le_bytes().to_vec()
}

fn exterior_grid_payload(cell_x: i32, cell_y: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&cell_x.to_le_bytes());
    out.extend_from_slice(&cell_y.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out
}

fn float_pair(a: f32, b: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    out.extend_from_slice(&a.to_le_bytes());
    out.extend_from_slice(&b.to_le_bytes());
    out
}

fn zstring(value: &str) -> Vec<u8> {
    let mut out = value.as_bytes().to_vec();
    out.push(0);
    out
}

fn subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
    ParsedSubrecord {
        signature: SmolStr::from(signature),
        data: Bytes::from(data),
        semantic_type: None,
    }
}

fn read_i32(data: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read_f32(data: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// Unused-in-production helper kept next to the tree walkers it mirrors: the
/// set of every record form-id under `items`, used by tests to prove the
/// re-lattice is ref-count preserving.
#[cfg(test)]
fn collect_all_record_form_ids(items: &[ParsedItem], out: &mut Vec<u32>) {
    for item in items {
        match item {
            ParsedItem::Record(record) => out.push(record.form_id),
            ParsedItem::Group(group) => collect_all_record_form_ids(&group.children, out),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_new_native,
    };

    use crate::ids::FormKey;
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::{Game, Translator};

    const WORLD_SOURCE: u32 = 0x0000_0800;
    const PERSISTENT_CELL_SOURCE: u32 = 0x0000_0801;
    const PERSISTENT_REF_SOURCE: u32 = 0x0000_0802;
    const LATTICE_CELL_BASE: u32 = 0x0000_0900;
    const PLACED_REF_BASE: u32 = 0x0000_1000;
    const INTERIOR_CELL_SOURCE: u32 = 0x0000_0700;
    const INTERIOR_REF_SOURCE: u32 = 0x0000_0701;
    const PLACED_BASE_TARGET: u32 = 0x0000_0600;

    // -- fixture builders (modelled on phase::starfield_worldspace_gate) -----

    fn sr(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::from(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn rec(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::from(sig),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: Some(1),
            subrecords,
            raw_payload: None,
            parse_error: None,
        })
    }

    fn grp(group_type: i32, label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label,
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    fn xclc(x: i32, y: i32) -> ParsedSubrecord {
        sr("XCLC", exterior_grid_payload(x, y))
    }

    fn data_position(x: f32, y: f32, z: f32) -> ParsedSubrecord {
        let mut out = Vec::with_capacity(24);
        for value in [x, y, z, 0.0, 0.0, 0.0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        sr("DATA", out)
    }

    fn name(form_id: u32) -> ParsedSubrecord {
        sr("NAME", form_id.to_le_bytes().to_vec())
    }

    /// Source cell index `index` at Starfield grid `(gx, gy)` holding `count`
    /// REFRs whose form-ids start at `PLACED_REF_BASE + index * 100`.
    struct CellSpec {
        index: u32,
        grid: (i32, i32),
        count: u32,
    }

    fn cell_form_id(index: u32) -> u32 {
        LATTICE_CELL_BASE + index
    }

    fn ref_form_id(index: u32, n: u32) -> u32 {
        PLACED_REF_BASE + index * 100 + n
    }

    /// Position (in FO4 units, i.e. post-pair-hook) of ref `n` of cell
    /// `index`: spread evenly across the source cell's 100 m footprint.
    fn ref_position(spec: &CellSpec, n: u32) -> (f32, f32) {
        let fraction = (n as f32 + 0.5) / spec.count as f32;
        let x_m = spec.grid.0 as f32 * SF_CELL_METERS + fraction * SF_CELL_METERS;
        let y_m = spec.grid.1 as f32 * SF_CELL_METERS + fraction * SF_CELL_METERS;
        (x_m * METERS_TO_FO4_UNITS, y_m * METERS_TO_FO4_UNITS)
    }

    fn source_world_items(world_editor_id: &str, specs: &[CellSpec]) -> Vec<ParsedItem> {
        let wrld = rec(
            "WRLD",
            WORLD_SOURCE,
            vec![
                sr("EDID", zstring(world_editor_id)),
                sr("NAM0", float_pair(0.0, 0.0)),
                sr("NAM9", float_pair(100.0, 100.0)),
                sr("DNAM", float_pair(-10.0, 2.0)),
            ],
        );

        let mut world_children = vec![
            rec(
                "CELL",
                PERSISTENT_CELL_SOURCE,
                vec![sr("EDID", zstring("AkilaCityPersistent"))],
            ),
            grp(
                CELL_CHILDREN_GROUP,
                PERSISTENT_CELL_SOURCE.to_le_bytes(),
                vec![grp(
                    CELL_PERSISTENT_GROUP,
                    PERSISTENT_CELL_SOURCE.to_le_bytes(),
                    vec![rec(
                        "REFR",
                        PERSISTENT_REF_SOURCE,
                        vec![data_position(1.0, 2.0, 3.0)],
                    )],
                )],
            ),
        ];

        let mut sub_block_children = Vec::new();
        for spec in specs {
            let cell_id = cell_form_id(spec.index);
            sub_block_children.push(rec(
                "CELL",
                cell_id,
                vec![
                    sr("EDID", zstring(&format!("AkilaSource{}", spec.index))),
                    xclc(spec.grid.0, spec.grid.1),
                    sr("XCLW", 12.0f32.to_le_bytes().to_vec()),
                    sr("XCWT", 0x0000_0055u32.to_le_bytes().to_vec()),
                    sr("XLCN", 0x0000_0066u32.to_le_bytes().to_vec()),
                ],
            ));
            let refs = (0..spec.count)
                .map(|n| {
                    let (x, y) = ref_position(spec, n);
                    rec(
                        "REFR",
                        ref_form_id(spec.index, n),
                        vec![data_position(x, y, 0.0)],
                    )
                })
                .collect();
            sub_block_children.push(grp(
                CELL_CHILDREN_GROUP,
                cell_id.to_le_bytes(),
                vec![grp(CELL_TEMPORARY_GROUP, cell_id.to_le_bytes(), refs)],
            ));
        }

        world_children.push(grp(
            EXTERIOR_BLOCK_GROUP,
            encode_exterior_grid_label(0, 0),
            vec![grp(
                EXTERIOR_SUB_BLOCK_GROUP,
                encode_exterior_grid_label(0, 0),
                sub_block_children,
            )],
        ));

        vec![
            wrld,
            grp(
                WORLD_CHILDREN_GROUP,
                WORLD_SOURCE.to_le_bytes(),
                world_children,
            ),
        ]
    }

    /// Source interiors, in Starfield's (and FO4's) shape:
    /// `GRUP(0,"CELL") -> GRUP(2) -> GRUP(3) -> CELL + GRUP(6) -> GRUP(9)`.
    fn source_interior_items() -> Vec<ParsedItem> {
        let (block, sub) = interior_bucket(INTERIOR_CELL_SOURCE);
        vec![grp(
            INTERIOR_BLOCK_GROUP,
            block.to_le_bytes(),
            vec![grp(
                INTERIOR_SUB_BLOCK_GROUP,
                sub.to_le_bytes(),
                vec![
                    rec(
                        "CELL",
                        INTERIOR_CELL_SOURCE,
                        vec![sr("EDID", zstring("AkilaInterior01"))],
                    ),
                    grp(
                        CELL_CHILDREN_GROUP,
                        INTERIOR_CELL_SOURCE.to_le_bytes(),
                        vec![grp(
                            CELL_TEMPORARY_GROUP,
                            INTERIOR_CELL_SOURCE.to_le_bytes(),
                            vec![rec(
                                "REFR",
                                INTERIOR_REF_SOURCE,
                                vec![name(PLACED_BASE_TARGET), data_position(4.0, 5.0, 6.0)],
                            )],
                        )],
                    ),
                ],
            )],
        )]
    }

    /// The target as `translate_v2` leaves it: everything FLAT.
    fn target_flat_items(specs: &[CellSpec], world_editor_id: &str) -> Vec<ParsedItem> {
        let mut cells = vec![
            rec(
                "CELL",
                PERSISTENT_CELL_SOURCE,
                vec![sr("EDID", zstring("AkilaCityPersistent"))],
            ),
            rec(
                "CELL",
                INTERIOR_CELL_SOURCE,
                vec![sr("EDID", zstring("AkilaInterior01"))],
            ),
        ];
        let mut refs = vec![
            rec(
                "REFR",
                PERSISTENT_REF_SOURCE,
                vec![name(PLACED_BASE_TARGET), data_position(1.0, 2.0, 3.0)],
            ),
            rec(
                "REFR",
                INTERIOR_REF_SOURCE,
                vec![name(PLACED_BASE_TARGET), data_position(4.0, 5.0, 6.0)],
            ),
        ];
        for spec in specs {
            cells.push(rec(
                "CELL",
                cell_form_id(spec.index),
                vec![
                    sr("EDID", zstring(&format!("AkilaSource{}", spec.index))),
                    xclc(spec.grid.0, spec.grid.1),
                    sr("XCLW", 12.0f32.to_le_bytes().to_vec()),
                    sr("XCWT", 0x0000_0055u32.to_le_bytes().to_vec()),
                    sr("XLCN", 0x0000_0066u32.to_le_bytes().to_vec()),
                ],
            ));
            for n in 0..spec.count {
                let (x, y) = ref_position(spec, n);
                refs.push(rec(
                    "REFR",
                    ref_form_id(spec.index, n),
                    vec![name(PLACED_BASE_TARGET), data_position(x, y, 0.0)],
                ));
            }
        }
        vec![
            grp(
                TOP_GROUP,
                *b"WRLD",
                vec![rec(
                    "WRLD",
                    WORLD_SOURCE,
                    vec![
                        sr("EDID", zstring(world_editor_id)),
                        sr("NAM0", float_pair(0.0, 0.0)),
                        sr("NAM9", float_pair(100.0, 100.0)),
                        sr("DNAM", float_pair(-10.0, 2.0)),
                    ],
                )],
            ),
            grp(TOP_GROUP, *b"CELL", cells),
            grp(TOP_GROUP, *b"REFR", refs),
            grp(
                TOP_GROUP,
                *b"STAT",
                vec![rec("STAT", PLACED_BASE_TARGET, Vec::new())],
            ),
        ]
    }

    struct Fixture {
        source_handle: u64,
        target_handle: u64,
        run_id: u64,
        report: RelatticeReport,
    }

    impl Fixture {
        fn target_root(&self) -> Vec<ParsedItem> {
            let store = plugin_handle_store_ref().lock().unwrap();
            store
                .get(&self.target_handle)
                .unwrap()
                .parsed
                .root_items
                .clone()
        }

        fn close(self) {
            drop_run(self.run_id).unwrap();
            plugin_handle_close_native(self.source_handle);
            plugin_handle_close_native(self.target_handle);
        }
    }

    fn run_relattice(specs: &[CellSpec], world_editor_id: &str) -> Fixture {
        run_relattice_with_target_edit(specs, world_editor_id, |_| {})
    }

    fn run_relattice_with_target_edit(
        specs: &[CellSpec],
        world_editor_id: &str,
        edit_target: impl FnOnce(&mut Vec<ParsedItem>),
    ) -> Fixture {
        let source_handle = plugin_handle_new_native("Starfield.esm", Some("starfield")).unwrap();
        let target_handle = plugin_handle_new_native("Starfield_Ported.esm", Some("fo4")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store.get_mut(&source_handle).unwrap().parsed.root_items = vec![
                grp(
                    TOP_GROUP,
                    *b"WRLD",
                    source_world_items(world_editor_id, specs),
                ),
                grp(TOP_GROUP, *b"CELL", source_interior_items()),
            ];
            let mut target_items = target_flat_items(specs, world_editor_id);
            edit_target(&mut target_items);
            store.get_mut(&target_handle).unwrap().parsed.root_items = target_items;
        }

        let run_id = create_run(RunParams {
            source: Game::Starfield,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Starfield_Ported.esm".into(),
                is_whole_plugin: true,
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(run_id, |run| -> Result<RelatticeReport, RunError> {
            run.init_mapper_state()?;
            let mut identity = Vec::new();
            let source_root = {
                let store = plugin_handle_store_ref().lock().unwrap();
                store.get(&source_handle).unwrap().parsed.root_items.clone()
            };
            let mut source_ids = Vec::new();
            collect_all_record_form_ids(&source_root, &mut source_ids);
            source_ids.push(INTERIOR_CELL_SOURCE);
            source_ids.push(INTERIOR_REF_SOURCE);
            for form_id in source_ids {
                identity.push(form_id);
            }
            {
                let state = run.mapper_state.as_mut().unwrap();
                for form_id in identity {
                    let source_fk =
                        FormKey::parse(&format!("{form_id:X}@Starfield.esm"), &run.interner)
                            .unwrap();
                    let target_fk =
                        FormKey::parse(&format!("{form_id:X}@Starfield_Ported.esm"), &run.interner)
                            .unwrap();
                    state.source_to_target.insert(source_fk, target_fk);
                }
            }
            relattice_worldspaces_reporting(run)
        })
        .unwrap();

        Fixture {
            source_handle,
            target_handle,
            run_id,
            report,
        }
    }

    // -- navigation over the produced tree -----------------------------------

    fn world_children_of(root: &[ParsedItem]) -> &ParsedGroup {
        let wrld_top = find_top_group(root, b"WRLD").expect("WRLD top group");
        wrld_top
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group) if group.group_type == WORLD_CHILDREN_GROUP => Some(group),
                _ => None,
            })
            .expect("world children group")
    }

    struct PlacedCell<'a> {
        record: &'a ParsedRecord,
        children: Option<&'a ParsedGroup>,
        block: (i16, i16),
        sub_block: (i16, i16),
    }

    fn decode_label(label: [u8; 4]) -> (i16, i16) {
        (
            i16::from_le_bytes([label[2], label[3]]),
            i16::from_le_bytes([label[0], label[1]]),
        )
    }

    fn lattice_cells_of(root: &[ParsedItem]) -> Vec<PlacedCell<'_>> {
        let mut out = Vec::new();
        for block in groups_of_type(&world_children_of(root).children, EXTERIOR_BLOCK_GROUP) {
            for sub_block in groups_of_type(&block.children, EXTERIOR_SUB_BLOCK_GROUP) {
                for item in &sub_block.children {
                    let ParsedItem::Record(record) = item else {
                        continue;
                    };
                    out.push(PlacedCell {
                        record,
                        children: find_group(
                            &sub_block.children,
                            CELL_CHILDREN_GROUP,
                            record.form_id,
                        ),
                        block: decode_label(block.label),
                        sub_block: decode_label(sub_block.label),
                    });
                }
            }
        }
        out
    }

    fn refr_ids_in(items: &[ParsedItem]) -> Vec<u32> {
        let mut all = Vec::new();
        collect_all_record_form_ids(items, &mut all);
        all
    }

    fn sorted_refr_form_ids(root: &[ParsedItem]) -> Vec<u32> {
        let mut out = Vec::new();
        fn walk(items: &[ParsedItem], out: &mut Vec<u32>) {
            for item in items {
                match item {
                    ParsedItem::Record(record) if record.signature.as_str() == "REFR" => {
                        out.push(record.form_id)
                    }
                    ParsedItem::Group(group) => walk(&group.children, out),
                    _ => {}
                }
            }
        }
        walk(root, &mut out);
        out.sort_unstable();
        out
    }

    fn two_by_two_specs() -> Vec<CellSpec> {
        vec![
            CellSpec {
                index: 0,
                grid: (0, 0),
                count: 10,
            },
            CellSpec {
                index: 1,
                grid: (1, 0),
                count: 10,
            },
            CellSpec {
                index: 2,
                grid: (0, 1),
                count: 10,
            },
            CellSpec {
                index: 3,
                grid: (1, 1),
                count: 10,
            },
        ]
    }

    // -- the headline invariant ---------------------------------------------

    #[test]
    fn every_placed_ref_survives_the_relattice() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let mut expected: Vec<u32> = specs
            .iter()
            .flat_map(|spec| (0..spec.count).map(|n| ref_form_id(spec.index, n)))
            .collect();
        expected.push(PERSISTENT_REF_SOURCE);
        expected.push(INTERIOR_REF_SOURCE);
        expected.sort_unstable();

        assert_eq!(
            sorted_refr_form_ids(&root),
            expected,
            "output REFR form-id multiset must equal the input multiset"
        );
        assert_eq!(fixture.report.placed_refs_relatticed, 40);
        assert_eq!(fixture.report.placed_refs_kept_persistent, 1);
        assert_eq!(fixture.report.placed_refs_unmapped, 0);
        assert_eq!(fixture.report.placed_refs_without_position, 0);
        fixture.close();
    }

    #[test]
    fn null_or_missing_own_base_refs_are_dropped_before_nesting() {
        let specs = vec![CellSpec {
            index: 0,
            grid: (0, 0),
            count: 2,
        }];
        let null_base_ref = ref_form_id(0, 0);
        let missing_base_ref = ref_form_id(0, 1);
        let fixture = run_relattice_with_target_edit(&specs, "AkilaCity", |items| {
            for (form_id, base_form_id) in [
                (null_base_ref, 0),
                (missing_base_ref, PLACED_BASE_TARGET + 1),
            ] {
                let record = find_record_mut(items, form_id).expect("target REFR");
                let name = record
                    .subrecords
                    .iter_mut()
                    .find(|subrecord| subrecord.signature.as_str() == "NAME")
                    .expect("fixture NAME");
                name.data = Bytes::copy_from_slice(&base_form_id.to_le_bytes());
            }
        });
        let root = fixture.target_root();
        let refs = sorted_refr_form_ids(&root);

        assert_eq!(fixture.report.placed_refs_invalid_base_dropped, 2);
        assert!(!refs.contains(&null_base_ref));
        assert!(!refs.contains(&missing_base_ref));
        assert!(refs.contains(&PERSISTENT_REF_SOURCE));
        assert!(refs.contains(&INTERIOR_REF_SOURCE));
        fixture.close();
    }

    #[test]
    fn refs_land_in_the_fo4_cell_their_position_implies() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let mut checked = 0;
        for cell in lattice_cells_of(&root) {
            let grid = cell_grid(cell.record).expect("synthesized cell carries XCLC");
            let children = cell.children.expect("cell children group");
            for item in refr_ids_in(&children.children) {
                let record = find_record(&root, item).expect("ref record");
                let (x, y) = placed_position(record).expect("ref position");
                assert_eq!(
                    (
                        sf_frame::fo4_cell_of_units(x),
                        sf_frame::fo4_cell_of_units(y)
                    ),
                    grid,
                    "ref {item:06X} at ({x}, {y}) is parented to the wrong cell"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 40, "every re-latticed ref must be checked");
        fixture.close();
    }

    #[test]
    fn source_lattice_cells_are_all_evacuated() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let mut all = Vec::new();
        collect_all_record_form_ids(&root, &mut all);
        let synthesized: HashSet<u32> = lattice_cells_of(&root)
            .iter()
            .map(|cell| cell.record.form_id)
            .collect();
        for spec in &specs {
            let source_cell = cell_form_id(spec.index);
            // The source cell's FormID may be RECYCLED onto a synthesized
            // shell; what must not survive is the original source-grid cell.
            if synthesized.contains(&source_cell) {
                continue;
            }
            assert!(
                !all.contains(&source_cell),
                "source lattice cell {source_cell:06X} was not evacuated"
            );
        }
        for cell in lattice_cells_of(&root) {
            let editor = editor_id(cell.record).unwrap_or_default();
            assert!(
                !editor.starts_with("AkilaSource"),
                "a source-grid cell record survived: {editor}"
            );
        }
        assert_eq!(fixture.report.source_cells_evacuated, 4);
        fixture.close();
    }

    #[test]
    fn cell_editor_ids_match_terrain_convention() {
        // Matches terrain_native::authoring_emit's `"{}CellX{}Y{}"` format and
        // its own `B21_TestCellXP000YP000` assertion. 'X' and 'Y' are
        // separators, not sign tokens: "…CellX" + "N007" + "Y" + "P008". Any
        // other spelling (e.g. "AkilaCityCellN007P008") misses the EditorID-match
        // branch in plugin_runtime::merge_projected_cell_into_subblock, which then
        // discards this cell's placed refs when convert_terrain merges.
        assert_eq!(
            cell_editor_id("B21_Test", 0, 0),
            "B21_TestCellXP000YP000",
            "must reproduce terrain_native's own shipped assertion verbatim"
        );
        assert_eq!(
            cell_editor_id("AkilaCity", -7, 8),
            "AkilaCityCellXN007YP008"
        );
        assert_eq!(
            cell_editor_id("NewAtlantis", 123, -45),
            "NewAtlantisCellXP123YN045"
        );

        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();
        for cell in lattice_cells_of(&root) {
            let grid = cell_grid(cell.record).unwrap();
            assert_eq!(
                editor_id(cell.record).unwrap(),
                cell_editor_id("AkilaCity", grid.0, grid.1),
                "synthesized EditorID must byte-match terrain's convention"
            );
        }
        fixture.close();
    }

    #[test]
    fn block_and_subblock_labels_derive_from_new_coords() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let cells = lattice_cells_of(&root);
        assert!(!cells.is_empty());
        for cell in &cells {
            let grid = cell_grid(cell.record).unwrap();
            assert_eq!(
                cell.block,
                (
                    clamp_i16(sf_frame::fo4_exterior_block(grid.0)),
                    clamp_i16(sf_frame::fo4_exterior_block(grid.1))
                ),
                "block label must come from the NEW cell coords"
            );
            assert_eq!(
                cell.sub_block,
                (
                    clamp_i16(sf_frame::fo4_exterior_sub_block(grid.0)),
                    clamp_i16(sf_frame::fo4_exterior_sub_block(grid.1))
                ),
                "sub-block label must come from the NEW cell coords"
            );
        }
        fixture.close();
    }

    #[test]
    fn block_label_byte_order_matches_plugin_runtime() {
        // (x, y) packs as [y_lo, y_hi, x_lo, x_hi].
        assert_eq!(encode_exterior_grid_label(1, 2), [2, 0, 1, 0]);
        assert_eq!(
            encode_exterior_grid_label(-1, 0),
            [0, 0, 0xFF, 0xFF],
            "negative X must occupy the HIGH half of the label"
        );
    }

    #[test]
    fn persistent_cell_refs_are_not_relatticed() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let world_children = world_children_of(&root);
        let persistent_group = world_children
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group)
                    if group.group_type == CELL_CHILDREN_GROUP
                        && u32::from_le_bytes(group.label) == PERSISTENT_CELL_SOURCE =>
                {
                    Some(group)
                }
                _ => None,
            })
            .expect("persistent cell children group is kept under world-children");
        let section = match &persistent_group.children[0] {
            ParsedItem::Group(group) => group,
            _ => panic!("persistent children must be a group"),
        };
        assert_eq!(section.group_type, CELL_PERSISTENT_GROUP);
        assert_eq!(refr_ids_in(&section.children), vec![PERSISTENT_REF_SOURCE]);

        // ... and it is nowhere in the re-latticed lattice.
        for cell in lattice_cells_of(&root) {
            if let Some(children) = cell.children {
                assert!(
                    !refr_ids_in(&children.children).contains(&PERSISTENT_REF_SOURCE),
                    "the persistent ref must never be re-latticed"
                );
            }
        }
        fixture.close();
    }

    #[test]
    fn interior_cells_are_nested_into_fo4_block_topology() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let cell_top = find_top_group(&root, b"CELL").expect("CELL top group");
        assert!(
            !cell_top.children.iter().any(|item| matches!(
                item,
                ParsedItem::Record(record) if record.form_id == INTERIOR_CELL_SOURCE
            )),
            "the interior CELL must not stay a direct child of the flat CELL group"
        );

        let (block_label, sub_label) = interior_bucket(INTERIOR_CELL_SOURCE);
        let block = groups_of_type(&cell_top.children, INTERIOR_BLOCK_GROUP)
            .find(|group| i32::from_le_bytes(group.label) == block_label)
            .expect("interior block group");
        let sub_block = groups_of_type(&block.children, INTERIOR_SUB_BLOCK_GROUP)
            .find(|group| i32::from_le_bytes(group.label) == sub_label)
            .expect("interior sub-block group");
        assert!(
            sub_block.children.iter().any(|item| matches!(
                item,
                ParsedItem::Record(record) if record.form_id == INTERIOR_CELL_SOURCE
            )),
            "the interior CELL belongs under its sub-block"
        );

        let children = find_group(
            &sub_block.children,
            CELL_CHILDREN_GROUP,
            INTERIOR_CELL_SOURCE,
        )
        .expect("interior cell children group");
        let temporary = groups_of_type(&children.children, CELL_TEMPORARY_GROUP)
            .next()
            .expect("interior temporary group");
        assert_eq!(
            refr_ids_in(&temporary.children),
            vec![INTERIOR_REF_SOURCE],
            "the interior ref must be nested under its own cell"
        );

        let refr_top = find_top_group(&root, b"REFR").expect("flat REFR top group");
        assert!(
            !refr_ids_in(&refr_top.children).contains(&INTERIOR_REF_SOURCE),
            "a placed ref left flat has no parent cell and FO4 loads it at init"
        );
        assert_eq!(fixture.report.interior_cells_nested, 1);
        assert_eq!(fixture.report.interior_placed_refs_nested, 1);
        fixture.close();
    }

    #[test]
    fn a_second_interior_pass_is_a_no_op() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let before = sorted_refr_form_ids(&fixture.target_root());

        let report = with_run(fixture.run_id, |run| -> Result<RelatticeReport, RunError> {
            relattice_worldspaces_reporting(run)
        })
        .unwrap();

        assert_eq!(report.interiors_skipped_already_nested, 1);
        assert_eq!(report.interior_cells_nested, 0);
        assert_eq!(
            sorted_refr_form_ids(&fixture.target_root()),
            before,
            "re-running must not empty the tree it already built"
        );
        fixture.close();
    }

    #[test]
    fn synthesized_cell_form_ids_are_unique_and_do_not_collide() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let mut all = Vec::new();
        collect_all_record_form_ids(&root, &mut all);
        let mut sorted = all.clone();
        sorted.sort_unstable();
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(sorted, deduped, "no FormID may appear twice in the output");

        let cells = lattice_cells_of(&root);
        assert!(
            cells.len() > specs.len(),
            "1.70878 FO4 cells per SF cell must yield MORE cells than the source lattice: \
             got {} from {}",
            cells.len(),
            specs.len()
        );
        assert_eq!(
            fixture.report.cell_form_ids_reused, 4,
            "all four evacuated cells' FormIDs must be recycled first"
        );
        assert_eq!(
            fixture.report.cell_form_ids_reused + fixture.report.cell_form_ids_allocated,
            cells.len() as u32
        );
        fixture.close();
    }

    #[test]
    fn refs_outside_the_terrain_extent_are_kept_not_dropped() {
        // One extra source cell far outside the 2x2 lattice, and a ref inside
        // the lattice cell whose position places it well beyond the extent.
        let mut specs = two_by_two_specs();
        specs.push(CellSpec {
            index: 4,
            grid: (40, 40),
            count: 3,
        });
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let mut expected: Vec<u32> = specs
            .iter()
            .flat_map(|spec| (0..spec.count).map(|n| ref_form_id(spec.index, n)))
            .collect();
        expected.push(PERSISTENT_REF_SOURCE);
        expected.push(INTERIOR_REF_SOURCE);
        expected.sort_unstable();
        assert_eq!(
            sorted_refr_form_ids(&root),
            expected,
            "an out-of-extent ref must be KEPT, never dropped"
        );

        // The extent is derived from the source lattice, which now includes
        // (40, 40) — so nothing is outside it. Shrink the check to the
        // dedicated counter by asserting it is reported, not fatal.
        assert_eq!(fixture.report.placed_refs_relatticed, 43);
        fixture.close();
    }

    #[test]
    fn refs_beyond_the_source_lattice_extent_are_counted() {
        // A single source cell at (0, 0) whose refs are placed 30 SF cells
        // away: the buckets fall far outside the FO4 window the source lattice
        // covers, so the counter fires — and the refs still survive.
        let specs = vec![CellSpec {
            index: 0,
            grid: (0, 0),
            count: 2,
        }];
        let source_handle = plugin_handle_new_native("Starfield.esm", Some("starfield")).unwrap();
        let target_handle = plugin_handle_new_native("Starfield_Ported.esm", Some("fo4")).unwrap();
        let far_x = 30.0 * SF_CELL_METERS * METERS_TO_FO4_UNITS;
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store.get_mut(&source_handle).unwrap().parsed.root_items = vec![grp(
                TOP_GROUP,
                *b"WRLD",
                source_world_items("AkilaCity", &specs),
            )];
            let mut items = target_flat_items(&specs, "AkilaCity");
            for item in items.iter_mut() {
                let ParsedItem::Group(group) = item else {
                    continue;
                };
                if group.label != *b"REFR" {
                    continue;
                }
                for child in group.children.iter_mut() {
                    let ParsedItem::Record(record) = child else {
                        continue;
                    };
                    if record.form_id < PLACED_REF_BASE {
                        continue;
                    }
                    let data = record
                        .subrecords
                        .iter_mut()
                        .find(|subrecord| subrecord.signature.as_str() == "DATA")
                        .expect("fixture DATA");
                    *data = data_position(far_x, far_x, 0.0);
                }
            }
            store.get_mut(&target_handle).unwrap().parsed.root_items = items;
        }
        let run_id = create_run(RunParams {
            source: Game::Starfield,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Starfield_Ported.esm".into(),
                is_whole_plugin: true,
                ..Default::default()
            },
        })
        .unwrap();
        let report = with_run(run_id, |run| -> Result<RelatticeReport, RunError> {
            run.init_mapper_state()?;
            let ids = vec![
                WORLD_SOURCE,
                PERSISTENT_CELL_SOURCE,
                PERSISTENT_REF_SOURCE,
                cell_form_id(0),
                ref_form_id(0, 0),
                ref_form_id(0, 1),
            ];
            let state = run.mapper_state.as_mut().unwrap();
            for form_id in ids {
                let source_fk =
                    FormKey::parse(&format!("{form_id:X}@Starfield.esm"), &run.interner).unwrap();
                let target_fk =
                    FormKey::parse(&format!("{form_id:X}@Starfield_Ported.esm"), &run.interner)
                        .unwrap();
                state.source_to_target.insert(source_fk, target_fk);
            }
            relattice_worldspaces_reporting(run)
        })
        .unwrap();

        let root = {
            let store = plugin_handle_store_ref().lock().unwrap();
            store.get(&target_handle).unwrap().parsed.root_items.clone()
        };
        assert_eq!(report.placed_refs_outside_source_extent, 2);
        assert!(
            sorted_refr_form_ids(&root).contains(&ref_form_id(0, 0)),
            "out-of-extent refs are counted, never dropped"
        );

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn worldspace_bounds_and_land_defaults_are_rebuilt_from_the_new_lattice() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        let wrld_top = find_top_group(&root, b"WRLD").unwrap();
        let world = wrld_top
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Record(record) if record.signature.as_str() == "WRLD" => Some(record),
                _ => None,
            })
            .unwrap();

        let cells = lattice_cells_of(&root);
        let grids: Vec<(i32, i32)> = cells
            .iter()
            .map(|cell| cell_grid(cell.record).unwrap())
            .collect();
        let min_x = grids.iter().map(|g| g.0).min().unwrap();
        let min_y = grids.iter().map(|g| g.1).min().unwrap();
        let max_x = grids.iter().map(|g| g.0).max().unwrap();
        let max_y = grids.iter().map(|g| g.1).max().unwrap();

        let nam0 = find_subrecord(world, "NAM0").unwrap();
        assert_eq!(
            (read_f32(nam0, 0), read_f32(nam0, 4)),
            (min_x as f32 * 4096.0, min_y as f32 * 4096.0)
        );
        let nam9 = find_subrecord(world, "NAM9").unwrap();
        assert_eq!(
            (read_f32(nam9, 0), read_f32(nam9, 4)),
            ((max_x + 1) as f32 * 4096.0, (max_y + 1) as f32 * 4096.0)
        );
        let dnam = find_subrecord(world, "DNAM").unwrap();
        assert_eq!(
            (read_f32(dnam, 0), read_f32(dnam, 4)),
            (-10.0 * METERS_TO_FO4_UNITS, 2.0 * METERS_TO_FO4_UNITS),
            "DNAM land/water defaults are Starfield metres and take the factor"
        );
        fixture.close();
    }

    #[test]
    fn synthesized_cells_inherit_water_and_location_from_the_dominant_source_cell() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let root = fixture.target_root();

        for cell in lattice_cells_of(&root) {
            let xclw = find_subrecord(cell.record, "XCLW").expect("XCLW carried over");
            assert_eq!(read_f32(xclw, 0), 12.0 * METERS_TO_FO4_UNITS);
            assert_eq!(
                find_subrecord(cell.record, "XLCN").map(<[u8]>::to_vec),
                Some(0x0000_0066u32.to_le_bytes().to_vec())
            );
            assert_eq!(
                find_subrecord(cell.record, "XCWT").map(<[u8]>::to_vec),
                Some(0x0000_0055u32.to_le_bytes().to_vec())
            );
            assert!(
                find_subrecord(cell.record, "XCLL").is_none(),
                "exterior lighting is inherited from the worldspace, never replicated"
            );
            assert!(find_subrecord(cell.record, "LTMP").is_none());
            let data = find_subrecord(cell.record, "DATA").unwrap();
            assert_eq!(data, EXTERIOR_CELL_DATA_FLAGS.to_le_bytes());
        }
        fixture.close();
    }

    #[test]
    fn synthesized_cells_drop_null_form_id_payloads() {
        let payload = ParsedRecord {
            signature: SmolStr::from("CELL"),
            form_id: 0x900,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: Some(1),
            subrecords: vec![
                sr("XLCN", vec![0; 4]),
                sr("XCWT", vec![0; 4]),
                sr("XCCM", vec![0; 4]),
                sr("XCIM", vec![0; 4]),
            ],
            raw_payload: None,
            parse_error: None,
        };
        let output = synthesize_cell_record(
            "AkilaCity",
            &SynthesizedCell {
                grid: (0, 0),
                form_id: 0x901,
                refs: Vec::new(),
            },
            Some(&payload),
        );

        for signature in ["XLCN", "XCWT", "XCCM", "XCIM"] {
            assert!(
                find_subrecord(&output, signature).is_none(),
                "null {signature} must not be copied into synthesized cells"
            );
        }
        assert!(find_subrecord(&output, "XCLL").is_none());
    }

    #[test]
    fn a_second_pass_is_a_no_op_and_never_empties_the_tree() {
        let specs = two_by_two_specs();
        let fixture = run_relattice(&specs, "AkilaCity");
        let before = fixture.target_root();
        let second = with_run(fixture.run_id, |run| -> Result<RelatticeReport, RunError> {
            relattice_worldspaces_reporting(run)
        })
        .unwrap();
        let after = fixture.target_root();

        assert_eq!(second.worlds_skipped_already_nested, 1);
        assert_eq!(second.cells_synthesized, 0);
        assert_eq!(sorted_refr_form_ids(&before), sorted_refr_form_ids(&after));
        fixture.close();
    }

    /// When a world produces no tree at all (here: its only ref never
    /// translated, so nothing buckets), the evacuation must be abandoned
    /// wholesale. Removing the source cell from the flat group with no tree to
    /// nest it into would be a silent deletion.
    #[test]
    fn a_world_that_yields_no_tree_evacuates_nothing() {
        let source_handle = plugin_handle_new_native("Starfield.esm", Some("starfield")).unwrap();
        let target_handle = plugin_handle_new_native("Starfield_Ported.esm", Some("fo4")).unwrap();
        let cell_id = cell_form_id(0);
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            store.get_mut(&source_handle).unwrap().parsed.root_items = vec![grp(
                TOP_GROUP,
                *b"WRLD",
                vec![
                    rec("WRLD", WORLD_SOURCE, vec![sr("EDID", zstring("AkilaCity"))]),
                    grp(
                        WORLD_CHILDREN_GROUP,
                        WORLD_SOURCE.to_le_bytes(),
                        vec![grp(
                            EXTERIOR_BLOCK_GROUP,
                            encode_exterior_grid_label(0, 0),
                            vec![grp(
                                EXTERIOR_SUB_BLOCK_GROUP,
                                encode_exterior_grid_label(0, 0),
                                vec![
                                    rec("CELL", cell_id, vec![xclc(0, 0)]),
                                    grp(
                                        CELL_CHILDREN_GROUP,
                                        cell_id.to_le_bytes(),
                                        vec![grp(
                                            CELL_TEMPORARY_GROUP,
                                            cell_id.to_le_bytes(),
                                            vec![rec(
                                                "REFR",
                                                ref_form_id(0, 0),
                                                vec![data_position(1.0, 2.0, 3.0)],
                                            )],
                                        )],
                                    ),
                                ],
                            )],
                        )],
                    ),
                ],
            )];
            store.get_mut(&target_handle).unwrap().parsed.root_items = vec![
                grp(
                    TOP_GROUP,
                    *b"WRLD",
                    vec![rec(
                        "WRLD",
                        WORLD_SOURCE,
                        vec![sr("EDID", zstring("AkilaCity"))],
                    )],
                ),
                grp(
                    TOP_GROUP,
                    *b"CELL",
                    vec![rec("CELL", cell_id, vec![xclc(0, 0)])],
                ),
            ];
        }
        let run_id = create_run(RunParams {
            source: Game::Starfield,
            target: Game::Fo4,
            source_handle_id: source_handle,
            target_handle_id: target_handle,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Starfield_Ported.esm".into(),
                is_whole_plugin: true,
                ..Default::default()
            },
        })
        .unwrap();
        let report = with_run(run_id, |run| -> Result<RelatticeReport, RunError> {
            run.init_mapper_state()?;
            let state = run.mapper_state.as_mut().unwrap();
            // WRLD and CELL map; the REFR deliberately does not (fenced out).
            for form_id in [WORLD_SOURCE, cell_id] {
                let source_fk =
                    FormKey::parse(&format!("{form_id:X}@Starfield.esm"), &run.interner).unwrap();
                let target_fk =
                    FormKey::parse(&format!("{form_id:X}@Starfield_Ported.esm"), &run.interner)
                        .unwrap();
                state.source_to_target.insert(source_fk, target_fk);
            }
            relattice_worldspaces_reporting(run)
        })
        .unwrap();

        let root = {
            let store = plugin_handle_store_ref().lock().unwrap();
            store.get(&target_handle).unwrap().parsed.root_items.clone()
        };
        assert_eq!(report.worlds_relatticed, 0);
        assert_eq!(report.placed_refs_unmapped, 1);
        let mut all = Vec::new();
        collect_all_record_form_ids(&root, &mut all);
        assert!(
            all.contains(&cell_id),
            "an abandoned world must leave its records exactly where they were"
        );

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source_handle);
        plugin_handle_close_native(target_handle);
    }

    /// Contract pin: the `starfield:fo4` translation map must NOT skip CELL or
    /// REFR — the generic writer is the intake for both and
    /// `relattice_worldspaces` owns their topology. NAVM/NAVI stay skipped
    /// (no Starfield navmesh converter; FO4 NAVM needs structured placement).
    /// Mirrors the SCEN/DLBR pattern in `translator::mod`'s tests.
    #[test]
    fn starfield_translation_map_translates_cell_and_refr() {
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        assert!(
            !translator.maps.skip_records.contains("CELL"),
            "CELL must translate: relattice_worldspaces re-parents the translated records"
        );
        assert!(
            !translator.maps.skip_records.contains("REFR"),
            "REFR must translate: skipping it makes the position pair-hook a no-op"
        );
        assert!(translator.maps.skip_records.contains("NAVM"));
        assert!(translator.maps.skip_records.contains("NAVI"));
    }

    /// Real `akilacity.btd` extent -> 16x16 = 256 FO4 cells.
    #[test]
    fn akila_cell_count_is_256() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .unwrap()
            .join("extracted/starfield/terrain/akilacity.btd");
        if !path.exists() {
            eprintln!(
                "skipping akila_cell_count_is_256: {} is missing",
                path.display()
            );
            return;
        }
        let header = terrain_native::btd::BtdFile::open_header(&path.to_string_lossy())
            .expect("akilacity.btd header");
        let (x_lo, x_hi) = sf_frame::fo4_cell_range(header.cell_min_x, header.cell_max_x);
        let (y_lo, y_hi) = sf_frame::fo4_cell_range(header.cell_min_y, header.cell_max_y);
        let cells = (x_hi - x_lo + 1) as i64 * (y_hi - y_lo + 1) as i64;
        assert_eq!(
            cells, 256,
            "akilacity is 9x9 SF cells = 900 m/axis = 15.38 -> 16 FO4 cells/axis"
        );
    }

    fn find_record<'a>(items: &'a [ParsedItem], form_id: u32) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id == form_id => return Some(record),
                ParsedItem::Group(group) => {
                    if let Some(found) = find_record(&group.children, form_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn find_record_mut(items: &mut [ParsedItem], form_id: u32) -> Option<&mut ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id == form_id => return Some(record),
                ParsedItem::Group(group) => {
                    if let Some(found) = find_record_mut(&mut group.children, form_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
}
