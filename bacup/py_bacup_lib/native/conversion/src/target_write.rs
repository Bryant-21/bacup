//! Schema-driven target-handle encoder: encodes a typed `Record` back to raw
//! `ParsedRecord` bytes and inserts it into a plugin handle.
//!
//! `add_record_native` is the primary entry point. It is the inverse of
//! `read_record` in `source_read.rs`.

use crate::errors::WriteError;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::sym::StringInterner;
use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
use bytes::Bytes;
use esp_authoring_core::nvnm::{
    NvnmDoorRef, NvnmEdgeLink, NvnmParent, NvnmPayload, NvnmTriangle, NvnmVertex, parse_nvnm,
    write_nvnm,
};
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::codec_accepts_payload_length;
use esp_authoring_core::plugin_runtime::{
    COMPRESSED_RECORD_FLAG, LocalizedStringsState, NativePluginSlot, ParsedGroup, ParsedItem,
    ParsedRecord, ParsedSubrecord,
};
use esp_authoring_core::plugin_runtime::{NaviRebuildStats, WriteEffect};
use esp_authoring_core::plugin_runtime::{
    insert_parsed_record_in_slot, insert_projected_navmesh_record_in_slot,
    insert_projected_navmeshes_batch_in_slot, insert_quest_child_record_in_slot,
    insert_topic_child_record_in_slot, plugin_handle_store_ref,
    rebuild_projected_navi_record_from_source_in_slot,
    rebuild_projected_navi_record_from_source_in_slot_with_nver,
    rebuild_projected_navi_record_in_slot, replace_parsed_record_contents_in_slot,
    replace_parsed_record_in_slot, replace_parsed_records_contents_in_slot_batch,
    replace_parsed_records_in_slot_batch,
};
use smallvec::SmallVec;
use smol_str::SmolStr;
use std::collections::{BTreeMap, HashMap, HashSet};

const NVNM_TRIANGLE_EDGE_EXTRA_INFO_FLAGS: [u16; 3] = [0x0001, 0x0002, 0x0004];
const NVNM_EDGE_POINT_SCALE: f32 = 1024.0;
const NVNM_WORLDSPACE_CELL_SIZE: f32 = 4096.0;
const NVNM_MAX_FO4_EDGE_ROWS: usize = 255;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Encode a decoded `Record` and insert it into the target plugin handle.
///
///
/// # Errors
/// Returns `WriteError::UnknownSignature` if the record sig is unknown.
/// Returns `WriteError::EncodeFailure` for subrecord encoding errors.
/// Returns `WriteError::InsertFailure` if the plugin handle doesn't exist.
pub fn add_record_native(
    handle_id: u64,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    add_record_in_slot(slot, record, schema, interner)?;
    slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    Ok(())
}

pub(crate) fn encode_form_key_for_handle(
    handle_id: u64,
    form_key: crate::ids::FormKey,
    interner: &StringInterner,
) -> Result<u32, WriteError> {
    let store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get(&handle_id)
        .ok_or(WriteError::InvalidHandle(handle_id))?;
    let plugin_name = interner.resolve(form_key.plugin).ok_or_else(|| {
        WriteError::EncodeFailure(format!(
            "unresolved plugin symbol for FormKey {:06X}",
            form_key.local
        ))
    })?;
    let master_index = slot
        .parsed
        .header
        .masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin_name))
        .map(|index| index as u32)
        .unwrap_or(slot.parsed.header.masters.len() as u32);
    if master_index > 0xFF {
        return Err(WriteError::EncodeFailure(format!(
            "master index {master_index} exceeds FormID capacity"
        )));
    }
    Ok((master_index << 24) | (form_key.local & 0x00FF_FFFF))
}

/// Encode a NAVM and insert it into the target CELL child group described by
/// its NVNM parent data. Returns `Ok(false)` when the target cell/world is not
/// present in the target handle.
pub fn add_projected_navmesh_native(
    handle_id: u64,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<bool, WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)? else {
        return Ok(false);
    };
    let inserted = insert_projected_navmesh_record_in_slot(slot, parsed_record)
        .map_err(WriteError::InsertFailure)?;
    if inserted {
        slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    }
    Ok(inserted)
}

/// Chunked batch form of `add_projected_navmesh_native`: one store lock per
/// chunk; encodes serially IN INPUT ORDER (encode may allocate localized-string
/// ids — order is load-bearing), then batch-inserts. Per-record outcomes match
/// the single-record fn (`Ok(true)`/`Ok(false)`/`Err`) — including a missing
/// handle, which legacy surfaced as one `InsertFailure` per record. The single
/// `apply_write_effect(RecordsAddedOrRemoved)` per chunk is equivalent to one
/// per record because the effect is a pure idempotent cache invalidation
/// (`PluginIndexSections::apply_effect` → `invalidate_all`).
pub fn add_projected_navmeshes_chunk_native(
    handle_id: u64,
    records: Vec<Record>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Vec<Result<bool, WriteError>> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let Some(slot) = store.get_mut(&handle_id) else {
        return records
            .into_iter()
            .map(|_| {
                Err(WriteError::InsertFailure(format!(
                    "no plugin handle: {handle_id}"
                )))
            })
            .collect();
    };
    let mut outcomes: Vec<Result<bool, WriteError>> = Vec::with_capacity(records.len());
    let mut encoded_positions: Vec<usize> = Vec::new();
    let mut to_insert: Vec<ParsedRecord> = Vec::new();
    for (index, record) in records.into_iter().enumerate() {
        match encode_record_for_slot(slot, record, schema, interner) {
            Ok(Some(parsed)) => {
                encoded_positions.push(index);
                to_insert.push(parsed);
                outcomes.push(Ok(false)); // overwritten by the batch result below
            }
            Ok(None) => outcomes.push(Ok(false)),
            Err(e) => outcomes.push(Err(e)),
        }
    }
    let batch = insert_projected_navmeshes_batch_in_slot(slot, to_insert);
    let mut any_inserted = false;
    for (position, outcome) in encoded_positions.into_iter().zip(batch) {
        match outcome {
            Ok(inserted) => {
                if inserted {
                    any_inserted = true;
                }
                outcomes[position] = Ok(inserted);
            }
            Err(e) => outcomes[position] = Err(WriteError::InsertFailure(e)),
        }
    }
    if any_inserted {
        slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    }
    outcomes
}

/// Translate every NVNM vertex, grid bound, and waypoint in `record` by `offset`
/// (worldspace XYZ). The parent cell index and all index-based topology
/// (triangles, cover, edge links) are preserved. No-op when offset is zero.
///
/// Used by the projected-navmesh emit so navmesh geometry stays co-spatial with
/// placed records, which receive the same worldspace offset during conversion.
/// Returns the number of NVNM fields shifted.
pub fn offset_record_nvnm_geometry(
    record: &mut Record,
    offset: [f32; 3],
) -> Result<u32, WriteError> {
    if offset == [0.0, 0.0, 0.0] {
        return Ok(0);
    }
    let [dx, dy, dz] = offset;
    let mut shifted = 0_u32;
    for entry in record.fields.iter_mut() {
        if entry.sig.0 != *b"NVNM" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let mut payload = parse_nvnm(bytes.as_slice())
            .map_err(|e| WriteError::EncodeFailure(format!("NVNM parse for offset: {e}")))?;
        for v in payload.vertices.iter_mut() {
            v.x += dx;
            v.y += dy;
            v.z += dz;
        }
        if payload.grid.divisor > 0 {
            payload.grid.bounds_min_x += dx;
            payload.grid.bounds_max_x += dx;
            payload.grid.bounds_min_y += dy;
            payload.grid.bounds_max_y += dy;
            payload.grid.bounds_min_z += dz;
            payload.grid.bounds_max_z += dz;
        }
        for w in payload.waypoints.iter_mut() {
            w.x += dx;
            w.y += dy;
            w.z += dz;
        }
        *bytes = SmallVec::from_vec(write_nvnm(&payload));
        shifted += 1;
    }
    Ok(shifted)
}

pub fn add_quest_child_record_native(
    handle_id: u64,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<bool, WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)? else {
        return Ok(false);
    };
    let Some(parent_quest_form_id) = quest_parent_form_id_from_record(&parsed_record) else {
        return Ok(false);
    };
    let inserted = insert_quest_child_record_in_slot(slot, parent_quest_form_id, parsed_record)
        .map_err(WriteError::InsertFailure)?;
    if inserted {
        slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    }
    Ok(inserted)
}

pub fn add_topic_child_record_native(
    handle_id: u64,
    record: Record,
    parent_dialogue_form_id: u32,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<bool, WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)? else {
        return Ok(false);
    };
    let inserted = insert_topic_child_record_in_slot(slot, parent_dialogue_form_id, parsed_record)
        .map_err(WriteError::InsertFailure)?;
    if inserted {
        slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    }
    Ok(inserted)
}

/// Outcome of a batched interior-cell insert.
pub struct InteriorInsertOutcome {
    pub cell_inserted: bool,
    pub children_inserted: u32,
    pub children_dropped: u32,
}

/// Encode an interior CELL and its placed children, then build + attach the
/// whole interior-cell subtree in one esp call. No per-record whole-tree walk:
/// this replaces the former per-cell cell-insert + per-child placed-insert
/// pairing that made bulk interior conversion quadratic. The caller must strip
/// any pre-existing stub for this cell first via
/// [`remove_interior_cell_stubs_native`].
pub fn add_interior_cell_with_children_native(
    handle_id: u64,
    cell: Record,
    persistent: Vec<Record>,
    temporary: Vec<Record>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<InteriorInsertOutcome, WriteError> {
    let mut children_dropped = 0u32;
    let (cell_parsed, persistent_parsed, temporary_parsed) = {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store
            .get_mut(&handle_id)
            .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
        let cell_parsed = match encode_record_for_slot(slot, cell, schema, interner)? {
            Some(parsed) => parsed,
            None => {
                return Ok(InteriorInsertOutcome {
                    cell_inserted: false,
                    children_inserted: 0,
                    children_dropped: 0,
                });
            }
        };
        let mut persistent_parsed = Vec::with_capacity(persistent.len());
        for child in persistent {
            match encode_record_for_slot(slot, child, schema, interner)? {
                Some(parsed) => persistent_parsed.push(parsed),
                None => children_dropped += 1,
            }
        }
        let mut temporary_parsed = Vec::with_capacity(temporary.len());
        for child in temporary {
            match encode_record_for_slot(slot, child, schema, interner)? {
                Some(parsed) => temporary_parsed.push(parsed),
                None => children_dropped += 1,
            }
        }
        (cell_parsed, persistent_parsed, temporary_parsed)
    };

    let children_inserted = (persistent_parsed.len() + temporary_parsed.len()) as u32;
    esp_authoring_core::plugin_runtime::insert_interior_cell_with_children(
        handle_id,
        cell_parsed,
        persistent_parsed,
        temporary_parsed,
    )
    .map_err(WriteError::InsertFailure)?;
    apply_records_changed_effect(handle_id)?;
    Ok(InteriorInsertOutcome {
        cell_inserted: true,
        children_inserted,
        children_dropped,
    })
}

/// Remove every target CELL record whose object id is in `object_ids` in one
/// pass (interior-cell phase PKIN-stub dedup). Returns the number removed.
pub fn remove_interior_cell_stubs_native(
    handle_id: u64,
    object_ids: &[u32],
) -> Result<usize, WriteError> {
    let removed =
        esp_authoring_core::plugin_runtime::remove_cell_records_by_object_id(handle_id, object_ids)
            .map_err(WriteError::InsertFailure)?;
    if removed > 0 {
        apply_records_changed_effect(handle_id)?;
    }
    Ok(removed)
}

/// Re-acquire the store lock to mark records-added-or-removed after an esp
/// entry point mutated the tree behind its own lock.
fn apply_records_changed_effect(handle_id: u64) -> Result<(), WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    Ok(())
}

/// Build a `{source INFO form_id -> source parent-DIAL form_id}` index from the
/// source plugin's group nesting.
///
/// INFO->DIAL parentage in FO4/FO76 is expressed ONLY by group nesting: each
/// DIAL's child INFOs live in a TES4 Topic-Child group (group_type 7) whose
/// 4-byte label is the parent DIAL's form_id. There is no DIAL subrecord that
/// lists child INFOs, so the parent must be recovered from this nesting.
pub fn build_source_info_to_dialogue_index(
    source_handle_id: u64,
) -> Result<HashMap<u32, u32>, WriteError> {
    const TOPIC_CHILD_GROUP: i32 = 7;
    let store = plugin_handle_store_ref().lock().unwrap();
    let slot = store.get(&source_handle_id).ok_or_else(|| {
        WriteError::InsertFailure(format!("no source plugin handle: {source_handle_id}"))
    })?;
    let mut index = HashMap::new();
    fn walk(items: &[ParsedItem], index: &mut HashMap<u32, u32>) {
        for item in items {
            let ParsedItem::Group(group) = item else {
                continue;
            };
            if group.group_type == TOPIC_CHILD_GROUP {
                let parent_dialogue_form_id = u32::from_le_bytes(group.label);
                for child in &group.children {
                    if let ParsedItem::Record(record) = child {
                        if record.signature.as_str() == "INFO" {
                            index.insert(record.form_id, parent_dialogue_form_id);
                        }
                    }
                }
            }
            walk(&group.children, index);
        }
    }
    walk(&slot.parsed.root_items, &mut index);
    Ok(index)
}

fn quest_parent_form_id_from_record(record: &ParsedRecord) -> Option<u32> {
    let parent_sig = match record.signature.as_str() {
        "DIAL" => "QNAM",
        "SCEN" => "PNAM",
        _ => return None,
    };
    record
        .subrecords
        .iter()
        .rev()
        .find(|subrecord| subrecord.signature.as_str() == parent_sig && subrecord.data.len() >= 4)
        .map(|subrecord| {
            u32::from_le_bytes([
                subrecord.data[0],
                subrecord.data[1],
                subrecord.data[2],
                subrecord.data[3],
            ])
        })
        .filter(|form_id| *form_id != 0)
}

/// Rebuild the target top-level NAVI record from already-emitted target NAVM
/// records. This intentionally synthesizes from the target graph instead of
/// copying source NAVI, which can reference dropped or remapped navmeshes.
pub fn rebuild_projected_navi_native(
    handle_id: u64,
    preferred_form_id: Option<u32>,
) -> Result<NaviRebuildStats, WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    let navmesh_stats =
        canonicalize_navmesh_portals_in_slot(slot).map_err(WriteError::InsertFailure)?;
    let mut stats = rebuild_projected_navi_record_in_slot(slot, preferred_form_id)
        .map_err(WriteError::InsertFailure)?;
    apply_navmesh_finalize_stats(&mut stats, navmesh_stats);
    Ok(stats)
}

pub fn rebuild_projected_navi_from_source_native(
    target_handle_id: u64,
    source_handle_id: u64,
    source_to_target_formids: &[(u32, u32)],
    preferred_form_id: Option<u32>,
) -> Result<NaviRebuildStats, WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    // Borrow source + target disjointly from the handle store instead of
    // cloning the entire source `root_items` (~5.5M records, several GB) just to
    // dodge the double borrow; the rebuild below only *reads* the source tree.
    let [source_slot, target_slot] = store.get_disjoint_mut([&source_handle_id, &target_handle_id]);
    let source_root_items = &source_slot
        .ok_or_else(|| {
            WriteError::InsertFailure(format!("no source plugin handle: {source_handle_id}"))
        })?
        .parsed
        .root_items;
    let target = target_slot.ok_or_else(|| {
        WriteError::InsertFailure(format!("no target plugin handle: {target_handle_id}"))
    })?;
    let navmesh_stats =
        canonicalize_navmesh_portals_in_slot(target).map_err(WriteError::InsertFailure)?;
    let mut stats = rebuild_projected_navi_record_from_source_in_slot(
        target,
        source_root_items,
        source_to_target_formids,
        preferred_form_id,
    )
    .map_err(WriteError::InsertFailure)?;
    apply_navmesh_finalize_stats(&mut stats, navmesh_stats);
    Ok(stats)
}

pub fn rebuild_projected_navi_from_source_with_nver_native(
    target_handle_id: u64,
    source_handle_id: u64,
    source_to_target_formids: &[(u32, u32)],
    preferred_form_id: Option<u32>,
    target_nver: u32,
) -> Result<NaviRebuildStats, WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let [source_slot, target_slot] = store.get_disjoint_mut([&source_handle_id, &target_handle_id]);
    let source_root_items = &source_slot
        .ok_or_else(|| {
            WriteError::InsertFailure(format!("no source plugin handle: {source_handle_id}"))
        })?
        .parsed
        .root_items;
    let target = target_slot.ok_or_else(|| {
        WriteError::InsertFailure(format!("no target plugin handle: {target_handle_id}"))
    })?;
    let navmesh_stats =
        canonicalize_navmesh_portals_in_slot(target).map_err(WriteError::InsertFailure)?;
    let mut stats = rebuild_projected_navi_record_from_source_in_slot_with_nver(
        target,
        source_root_items,
        source_to_target_formids,
        preferred_form_id,
        Some(target_nver),
    )
    .map_err(WriteError::InsertFailure)?;
    apply_navmesh_finalize_stats(&mut stats, navmesh_stats);
    Ok(stats)
}

pub fn diagnose_navmesh_links_in_slot_native(
    handle_id: u64,
) -> Result<NavmeshFinalizeStats, WriteError> {
    let store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    let mut snapshots = HashMap::new();
    collect_navmesh_portal_snapshots(&slot.parsed.root_items, &mut snapshots)
        .map_err(WriteError::InsertFailure)?;
    if snapshots.is_empty() {
        return Ok(NavmeshFinalizeStats::default());
    }
    let (_finalized, stats) =
        finalize_navmesh_links_with_stats(&snapshots).map_err(WriteError::InsertFailure)?;
    Ok(stats)
}

fn apply_navmesh_finalize_stats(stats: &mut NaviRebuildStats, navmesh_stats: NavmeshFinalizeStats) {
    stats.navmeshes_seen += navmesh_stats.navmeshes_seen;
    stats.navmeshes_touched += navmesh_stats.navmeshes_touched;
    stats.navmesh_bad_internal_links += navmesh_stats.bad_internal_links;
    stats.navmesh_linked_edge_vertex_mismatches += navmesh_stats.linked_edge_vertex_mismatches;
    stats.navmesh_opposite_normal_linked_pairs += navmesh_stats.opposite_normal_linked_pairs;
    stats.navmesh_missing_internal_links += navmesh_stats.missing_internal_links;
    stats.navmesh_same_direction_internal_edges += navmesh_stats.same_direction_internal_edges;
    stats.navmesh_ambiguous_local_edges += navmesh_stats.ambiguous_local_edges;
    stats.navmesh_external_links_added += navmesh_stats.external_links_added;
    stats.navmesh_missing_external_links += navmesh_stats.missing_external_links;
    stats.navmesh_ambiguous_external_edges += navmesh_stats.ambiguous_external_edges;
    stats.navmesh_external_link_caps_hit += navmesh_stats.external_link_caps_hit;
    stats.navmesh_winding_conflicts += navmesh_stats.winding_conflicts;
    stats.warnings += navmesh_stats.residual_warning_count();
}

fn canonicalize_navmesh_portals_in_slot(
    slot: &mut NativePluginSlot,
) -> Result<NavmeshFinalizeStats, String> {
    let mut snapshots = HashMap::new();
    collect_navmesh_portal_snapshots(&slot.parsed.root_items, &mut snapshots)?;
    if snapshots.is_empty() {
        return Ok(NavmeshFinalizeStats::default());
    }
    let (finalized, stats) = finalize_navmesh_links_with_stats(&snapshots)?;
    let mut touched = SmallVec::<[u32; 4]>::new();
    let own_index = (slot.parsed.header.masters.len() & 0xFF) as u32;
    let mut valid_door_refs = HashSet::new();
    collect_navmesh_door_ref_targets(&slot.parsed.root_items, &mut valid_door_refs);
    canonicalize_navmesh_portals_in_items(
        &mut slot.parsed.root_items,
        &finalized,
        own_index,
        &valid_door_refs,
        &mut touched,
    )?;
    if !touched.is_empty() {
        slot.apply_write_effect(&WriteEffect::RecordContents { form_ids: touched });
    }
    Ok(stats)
}

#[derive(Debug, Clone)]
struct NvnmPortalSnapshot {
    parent: NvnmPortalParent,
    exterior_cell: Option<(i16, i16)>,
    vertices: Vec<NvnmVertex>,
    triangles: Vec<NvnmTriangle>,
    edge_links: Vec<NvnmEdgeLink>,
}

/// Cell-aware parent identity used as the edge-graph key inside the finalizer.
/// The exterior variant drops grid_x/grid_y because cross-navmesh edge matching
/// only cares about which worldspace they live in; cell coords are tracked
/// separately on `NvnmPortalSnapshot::exterior_cell`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum NvnmPortalParent {
    Exterior { world: u32 },
    Interior { cell: u32 },
}

impl NvnmPortalParent {
    fn from_payload(parent: NvnmParent) -> Self {
        match parent {
            NvnmParent::Interior { cell } => NvnmPortalParent::Interior { cell },
            NvnmParent::Exterior { world, .. } => NvnmPortalParent::Exterior { world },
        }
    }
}

fn portal_snapshot_from_payload(payload: &NvnmPayload) -> NvnmPortalSnapshot {
    let exterior_cell = match payload.parent {
        NvnmParent::Exterior { grid_x, grid_y, .. } => Some((grid_x, grid_y)),
        NvnmParent::Interior { .. } => None,
    };
    NvnmPortalSnapshot {
        parent: NvnmPortalParent::from_payload(payload.parent),
        exterior_cell,
        vertices: payload.vertices.clone(),
        triangles: payload.triangles.clone(),
        edge_links: payload.edge_links.clone(),
    }
}

/// Conversion-local intermediate: triangle topology + portal/edge link rows
/// after winding finalization, BEFORE byte serialization. Triangle count is
/// preserved 1:1 with the source snapshot (no pruning happens here).
#[derive(Debug, Clone)]
struct NvnmFinalizedPayload {
    triangles: Vec<NvnmFinalizedTriangle>,
    edge_rows: Vec<[u8; 11]>,
}

#[derive(Debug, Clone, Copy)]
struct NvnmFinalizedTriangle {
    vertices: [u16; 3],
    links: [i16; 3],
    flags: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct NvnmEdgePoint {
    x: i64,
    y: i64,
    z: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct NvnmGlobalEdgeKey {
    parent: NvnmPortalParent,
    a: NvnmEdgePoint,
    b: NvnmEdgePoint,
}

#[derive(Debug, Clone, Copy)]
struct NvnmFinalEdgeRef {
    form_id: u32,
    triangle: usize,
    slot: usize,
    oriented: [NvnmEdgePoint; 2],
}

#[derive(Debug, Clone, Copy)]
struct NvnmLocalEdgeRef {
    triangle: usize,
    slot: usize,
    oriented: [u16; 2],
}

struct NvnmWindingSolution {
    flips_by_form: HashMap<u32, Vec<bool>>,
    conflicts: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NavmeshFinalizeStats {
    pub navmeshes_seen: u32,
    pub navmeshes_touched: u32,
    pub bad_internal_links: u32,
    pub linked_edge_vertex_mismatches: u32,
    pub opposite_normal_linked_pairs: u32,
    pub missing_internal_links: u32,
    pub same_direction_internal_edges: u32,
    pub ambiguous_local_edges: u32,
    pub external_links_added: u32,
    pub missing_external_links: u32,
    pub ambiguous_external_edges: u32,
    pub external_link_caps_hit: u32,
    pub winding_conflicts: u32,
}

impl NavmeshFinalizeStats {
    pub fn residual_warning_count(self) -> u32 {
        self.bad_internal_links
            + self.linked_edge_vertex_mismatches
            + self.opposite_normal_linked_pairs
            + self.missing_internal_links
            + self.same_direction_internal_edges
            + self.ambiguous_local_edges
            + self.missing_external_links
            + self.ambiguous_external_edges
            + self.external_link_caps_hit
            + self.winding_conflicts
    }
}

fn collect_navmesh_portal_snapshots(
    items: &[ParsedItem],
    out: &mut HashMap<u32, NvnmPortalSnapshot>,
) -> Result<(), String> {
    use rayon::prelude::*;
    // Serial walk gathers payload borrows in walk order; the NVNM parses run in
    // parallel (ordered indexed collect); the serial fold inserts in walk order
    // and surfaces the FIRST error in walk order — including leaving `out`
    // holding exactly the pre-error prefix — matching the legacy serial walk.
    let mut gathered: Vec<(u32, &[u8])> = Vec::new();
    gather_navmesh_nvnm_payloads(items, &mut gathered);
    let parsed: Vec<(u32, Result<NvnmPortalSnapshot, String>)> = gathered
        .par_iter()
        .map(|(form_id, data)| {
            (
                *form_id,
                parse_nvnm(data)
                    .map(|payload| portal_snapshot_from_payload(&payload))
                    .map_err(|e| format!("NVNM parse: {e}")),
            )
        })
        .collect();
    for (form_id, result) in parsed {
        out.insert(form_id, result?);
    }
    Ok(())
}

fn gather_navmesh_nvnm_payloads<'a>(items: &'a [ParsedItem], out: &mut Vec<(u32, &'a [u8])>) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "NAVM" => {
                if let Some(data) = nvnm_subrecord_data(record) {
                    if data.is_empty() {
                        continue;
                    }
                    out.push((record.form_id, data));
                }
            }
            ParsedItem::Group(group) => gather_navmesh_nvnm_payloads(&group.children, out),
            _ => {}
        }
    }
}

fn canonicalize_navmesh_portals_in_items(
    items: &mut [ParsedItem],
    finalized: &HashMap<u32, NvnmFinalizedPayload>,
    own_index: u32,
    valid_door_refs: &HashSet<u32>,
    touched: &mut SmallVec<[u32; 4]>,
) -> Result<(), String> {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "NAVM" => {
                let form_id = record.form_id;
                for subrecord in &mut record.subrecords {
                    if subrecord.signature.as_str() != "NVNM" || subrecord.data.is_empty() {
                        continue;
                    }
                    let Some(finalized_payload) = finalized.get(&form_id) else {
                        continue;
                    };
                    let rewritten = rewrite_finalized_nvnm_portal_payload_with_door_refs(
                        subrecord.data.as_ref(),
                        finalized_payload,
                        own_index,
                        valid_door_refs,
                    )?;
                    if let Some(data) = rewritten {
                        subrecord.data = Bytes::from(data);
                        touched.push(form_id);
                    }
                    break;
                }
            }
            ParsedItem::Group(group) => canonicalize_navmesh_portals_in_items(
                &mut group.children,
                finalized,
                own_index,
                valid_door_refs,
                touched,
            )?,
            _ => {}
        }
    }
    Ok(())
}

fn collect_navmesh_door_ref_targets(items: &[ParsedItem], out: &mut HashSet<u32>) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "REFR" => {
                out.insert(record.form_id);
            }
            ParsedItem::Group(group) => collect_navmesh_door_ref_targets(&group.children, out),
            _ => {}
        }
    }
}

fn nvnm_subrecord_data(record: &ParsedRecord) -> Option<&[u8]> {
    record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "NVNM")
        .map(|subrecord| subrecord.data.as_ref())
}

/// Standalone diagnose (test + ad-hoc use): solves winding and finalizes
/// internally, then delegates to `diagnose_navmesh_links_with`. The production
/// finalizer calls `_with` directly so winding is solved and payloads are
/// finalized exactly ONCE instead of twice.
fn diagnose_navmesh_links(
    snapshots: &HashMap<u32, NvnmPortalSnapshot>,
) -> Result<NavmeshFinalizeStats, String> {
    let mut form_ids: Vec<u32> = snapshots.keys().copied().collect();
    form_ids.sort_unstable();
    let winding_solution = solve_global_triangle_winding_flips(snapshots, &form_ids)?;
    let finalized = finalize_all_navmesh_links(snapshots, &winding_solution, &form_ids)?;
    diagnose_navmesh_links_with(snapshots, &finalized, &form_ids)
}

/// Diagnose pass over already-finalized payloads. Stats are commutative
/// counter sums; iterating `form_ids` (sorted) instead of HashMap order only
/// affects which error surfaces first on a malformed payload.
fn diagnose_navmesh_links_with(
    snapshots: &HashMap<u32, NvnmPortalSnapshot>,
    finalized: &HashMap<u32, NvnmFinalizedPayload>,
    form_ids: &[u32],
) -> Result<NavmeshFinalizeStats, String> {
    let mut stats = NavmeshFinalizeStats {
        navmeshes_seen: snapshots.len() as u32,
        ..NavmeshFinalizeStats::default()
    };

    for form_id in form_ids {
        let snapshot = snapshots
            .get(form_id)
            .ok_or_else(|| format!("missing NAVM snapshot {form_id:08X}"))?;
        diagnose_local_navmesh_links(snapshot, &mut stats)?;
    }

    diagnose_external_boundary_links(snapshots, finalized, form_ids, &mut stats)?;
    Ok(stats)
}

/// Per-navmesh finalize over the solved winding, parallel with an ordered
/// indexed collect (`finalize_local_navmesh_links` is pure per snapshot).
fn finalize_all_navmesh_links(
    snapshots: &HashMap<u32, NvnmPortalSnapshot>,
    winding_solution: &NvnmWindingSolution,
    form_ids: &[u32],
) -> Result<HashMap<u32, NvnmFinalizedPayload>, String> {
    use rayon::prelude::*;
    let finalized_vec: Vec<(u32, NvnmFinalizedPayload)> = form_ids
        .par_iter()
        .map(|form_id| -> Result<(u32, NvnmFinalizedPayload), String> {
            let snapshot = snapshots
                .get(form_id)
                .ok_or_else(|| format!("missing NAVM snapshot {form_id:08X}"))?;
            let flips = winding_solution
                .flips_by_form
                .get(form_id)
                .ok_or_else(|| format!("missing NAVM winding solution {form_id:08X}"))?;
            Ok((*form_id, finalize_local_navmesh_links(snapshot, flips)?))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(finalized_vec.into_iter().collect())
}

fn diagnose_local_navmesh_links(
    snapshot: &NvnmPortalSnapshot,
    stats: &mut NavmeshFinalizeStats,
) -> Result<(), String> {
    let mut edge_map = BTreeMap::<[u16; 2], SmallVec<[NvnmLocalEdgeRef; 3]>>::new();
    for (triangle_index, triangle) in snapshot.triangles.iter().enumerate() {
        for slot in 0..3 {
            let oriented = triangle_edge_vertices_target(triangle.vertices, slot);
            edge_map
                .entry(normalized_local_edge(oriented))
                .or_default()
                .push(NvnmLocalEdgeRef {
                    triangle: triangle_index,
                    slot,
                    oriented,
                });
            diagnose_internal_link(snapshot, triangle_index, slot, stats)?;
        }
    }

    for entries in edge_map.values() {
        match entries.len() {
            2 => {
                let left = entries[0];
                let right = entries[1];
                if opposite_local_edges(left.oriented, right.oriented) {
                    if !local_edge_is_linked(snapshot, left, right) {
                        stats.missing_internal_links += 2;
                    }
                } else {
                    stats.same_direction_internal_edges += 2;
                }
            }
            len if len > 2 => stats.ambiguous_local_edges += len as u32,
            _ => {}
        }
    }
    Ok(())
}

fn diagnose_internal_link(
    snapshot: &NvnmPortalSnapshot,
    triangle_index: usize,
    slot: usize,
    stats: &mut NavmeshFinalizeStats,
) -> Result<(), String> {
    let triangle = snapshot
        .triangles
        .get(triangle_index)
        .ok_or_else(|| format!("missing NAVM triangle {triangle_index}"))?;
    if triangle.flags & nvnm_edge_extra_info_flag(slot) != 0 || triangle.links[slot] < 0 {
        return Ok(());
    }

    let target_index = triangle.links[slot] as usize;
    let Some(target) = snapshot.triangles.get(target_index) else {
        stats.bad_internal_links += 1;
        stats.linked_edge_vertex_mismatches += 1;
        return Ok(());
    };

    let source_edge = triangle_edge_vertices_target(triangle.vertices, slot);
    let Some(target_edge) = matching_triangle_edge(target.vertices, source_edge) else {
        stats.bad_internal_links += 1;
        stats.linked_edge_vertex_mismatches += 1;
        return Ok(());
    };
    if !opposite_local_edges(source_edge, target_edge) {
        stats.opposite_normal_linked_pairs += 1;
    }
    Ok(())
}

fn matching_triangle_edge(vertices: [u16; 3], source_edge: [u16; 2]) -> Option<[u16; 2]> {
    (0..3)
        .map(|slot| triangle_edge_vertices_target(vertices, slot))
        .find(|edge| normalized_local_edge(*edge) == normalized_local_edge(source_edge))
}

fn local_edge_is_linked(
    snapshot: &NvnmPortalSnapshot,
    left: NvnmLocalEdgeRef,
    right: NvnmLocalEdgeRef,
) -> bool {
    let left_triangle = snapshot.triangles[left.triangle];
    let right_triangle = snapshot.triangles[right.triangle];
    left_triangle.flags & nvnm_edge_extra_info_flag(left.slot) == 0
        && right_triangle.flags & nvnm_edge_extra_info_flag(right.slot) == 0
        && left_triangle.links[left.slot] == right.triangle as i16
        && right_triangle.links[right.slot] == left.triangle as i16
}

fn diagnose_external_boundary_links(
    snapshots: &HashMap<u32, NvnmPortalSnapshot>,
    finalized: &HashMap<u32, NvnmFinalizedPayload>,
    form_ids: &[u32],
    stats: &mut NavmeshFinalizeStats,
) -> Result<(), String> {
    let mut boundary_edges = BTreeMap::<NvnmGlobalEdgeKey, SmallVec<[NvnmFinalEdgeRef; 3]>>::new();
    for form_id in form_ids {
        let snapshot = snapshots
            .get(form_id)
            .ok_or_else(|| format!("missing NAVM snapshot {form_id:08X}"))?;
        let payload = finalized
            .get(form_id)
            .ok_or_else(|| format!("missing finalized NAVM {form_id:08X}"))?;
        for (triangle_index, triangle) in payload.triangles.iter().enumerate() {
            for slot in 0..3 {
                if triangle.links[slot] >= 0 {
                    continue;
                }
                let oriented = global_triangle_edge(snapshot, triangle.vertices, slot)?;
                let key = normalized_global_edge_key(snapshot.parent, oriented);
                boundary_edges
                    .entry(key)
                    .or_default()
                    .push(NvnmFinalEdgeRef {
                        form_id: *form_id,
                        triangle: triangle_index,
                        slot,
                        oriented,
                    });
            }
        }
    }

    for entries in boundary_edges.values() {
        match entries.len() {
            2 => {
                let left = entries[0];
                let right = entries[1];
                if left.form_id != right.form_id
                    && opposite_global_edges(left.oriented, right.oriented)
                {
                    stats.missing_external_links += 2;
                }
            }
            len if len > 2 => stats.ambiguous_external_edges += len as u32,
            _ => {}
        }
    }
    Ok(())
}

fn finalize_navmesh_links_with_stats(
    snapshots: &HashMap<u32, NvnmPortalSnapshot>,
) -> Result<(HashMap<u32, NvnmFinalizedPayload>, NavmeshFinalizeStats), String> {
    let mut form_ids: Vec<u32> = snapshots.keys().copied().collect();
    form_ids.sort_unstable();
    // Solve winding ONCE and finalize each navmesh ONCE (parallel) instead of
    // solving and finalizing a second time inside diagnose_navmesh_links.
    let winding_solution = solve_global_triangle_winding_flips(snapshots, &form_ids)?;
    let mut finalized = finalize_all_navmesh_links(snapshots, &winding_solution, &form_ids)?;
    let mut stats = diagnose_navmesh_links_with(snapshots, &finalized, &form_ids)?;
    stats.winding_conflicts += winding_solution.conflicts;

    let before_external_links: usize = finalized
        .values()
        .map(|payload| payload.edge_rows.len())
        .sum();
    add_external_final_links(snapshots, &mut finalized, &mut stats)?;
    let after_external_links: usize = finalized
        .values()
        .map(|payload| payload.edge_rows.len())
        .sum();
    stats.external_links_added +=
        u32::try_from(after_external_links.saturating_sub(before_external_links))
            .map_err(|_| "NAVM external link count exceeds u32".to_string())?;

    stats.navmeshes_touched = finalized
        .iter()
        .filter(|(form_id, payload)| {
            snapshots
                .get(form_id)
                .map(|snapshot| finalized_payload_differs(snapshot, payload))
                .unwrap_or(false)
        })
        .count() as u32;

    Ok((finalized, stats))
}

fn finalize_local_navmesh_links(
    snapshot: &NvnmPortalSnapshot,
    flips: &[bool],
) -> Result<NvnmFinalizedPayload, String> {
    if flips.len() != snapshot.triangles.len() {
        return Err(format!(
            "NVNM winding solution triangle count {} does not match payload triangle count {}",
            flips.len(),
            snapshot.triangles.len()
        ));
    }
    let mut triangles: Vec<NvnmFinalizedTriangle> = snapshot
        .triangles
        .iter()
        .enumerate()
        .map(|(index, triangle)| {
            let mut vertices = triangle.vertices;
            if flips[index] {
                vertices.swap(1, 2);
            }
            NvnmFinalizedTriangle {
                vertices,
                links: [-1; 3],
                flags: triangle.flags & !0x0007,
            }
        })
        .collect();

    let mut edge_map = BTreeMap::<[u16; 2], SmallVec<[NvnmLocalEdgeRef; 2]>>::new();
    for (triangle_index, triangle) in triangles.iter().enumerate() {
        for slot in 0..3 {
            let oriented = triangle_edge_vertices_target(triangle.vertices, slot);
            edge_map
                .entry(normalized_local_edge(oriented))
                .or_default()
                .push(NvnmLocalEdgeRef {
                    triangle: triangle_index,
                    slot,
                    oriented,
                });
        }
    }

    for entries in edge_map.values() {
        if entries.len() != 2 {
            continue;
        }
        let left = entries[0];
        let right = entries[1];
        // Only link a shared edge whose two half-edges run in OPPOSITE 2D
        // directions — the manifold invariant CK enforces. Two upfacing
        // triangles that traverse their shared edge in the SAME direction
        // overlap in projection (FO76 zigzag/sliver fans), and linking them is
        // exactly what makes CK log "opposite normals but linked / edges should
        // be linked but are not / vertices do not match". Leave same-direction
        // edges as unlinked boundaries — the honest representation of degenerate
        // overlap; CK's own rule forbids linking them. A degenerate triangle
        // could contribute the same normalized edge twice — never self-link.
        if left.triangle == right.triangle {
            continue;
        }
        if !opposite_local_edges(left.oriented, right.oriented) {
            continue;
        }
        let left_index = checked_i16_target(right.triangle, "NVNM internal triangle link")?;
        let right_index = checked_i16_target(left.triangle, "NVNM internal triangle link")?;
        triangles[left.triangle].links[left.slot] = left_index;
        triangles[right.triangle].links[right.slot] = right_index;
    }

    Ok(NvnmFinalizedPayload {
        triangles,
        edge_rows: Vec::new(),
    })
}

fn finalized_payload_differs(
    snapshot: &NvnmPortalSnapshot,
    finalized: &NvnmFinalizedPayload,
) -> bool {
    if snapshot.triangles.len() != finalized.triangles.len()
        || snapshot.edge_links.len() != finalized.edge_rows.len()
    {
        return true;
    }
    if snapshot
        .triangles
        .iter()
        .zip(&finalized.triangles)
        .any(|(old, new)| {
            old.vertices != new.vertices || old.links != new.links || old.flags != new.flags
        })
    {
        return true;
    }
    snapshot
        .edge_links
        .iter()
        .zip(&finalized.edge_rows)
        .any(|(old, new)| old.row != *new)
}

fn add_external_final_links(
    snapshots: &HashMap<u32, NvnmPortalSnapshot>,
    finalized: &mut HashMap<u32, NvnmFinalizedPayload>,
    stats: &mut NavmeshFinalizeStats,
) -> Result<(), String> {
    let mut boundary_edges = BTreeMap::<NvnmGlobalEdgeKey, SmallVec<[NvnmFinalEdgeRef; 3]>>::new();
    let mut form_ids: Vec<u32> = snapshots.keys().copied().collect();
    form_ids.sort_unstable();

    for form_id in &form_ids {
        let snapshot = snapshots
            .get(form_id)
            .ok_or_else(|| format!("missing NAVM snapshot {form_id:08X}"))?;
        let payload = finalized
            .get(form_id)
            .ok_or_else(|| format!("missing finalized NAVM {form_id:08X}"))?;
        for (triangle_index, triangle) in payload.triangles.iter().enumerate() {
            for slot in 0..3 {
                if triangle.links[slot] >= 0 {
                    continue;
                }
                let oriented = global_triangle_edge(snapshot, triangle.vertices, slot)?;
                let key = normalized_global_edge_key(snapshot.parent, oriented);
                boundary_edges
                    .entry(key)
                    .or_default()
                    .push(NvnmFinalEdgeRef {
                        form_id: *form_id,
                        triangle: triangle_index,
                        slot,
                        oriented,
                    });
            }
        }
    }

    for entries in boundary_edges.values() {
        match entries.len() {
            2 => {
                let left = entries[0];
                let right = entries[1];
                if left.form_id == right.form_id
                    || !opposite_global_edges(left.oriented, right.oriented)
                {
                    continue;
                }
                add_external_final_link_pair_with_stats(finalized, left, right, stats)?;
            }
            len if len > 2 => {}
            _ => {}
        }
    }
    Ok(())
}

fn solve_global_triangle_winding_flips(
    snapshots: &HashMap<u32, NvnmPortalSnapshot>,
    form_ids: &[u32],
) -> Result<NvnmWindingSolution, String> {
    // Creation Kit's navmesh winding invariant is per-triangle, not adjacency
    // based: every triangle must have an upward projected normal (+Z). It does
    // NOT require two triangles sharing an edge to traverse that edge in
    // opposite 2D-projected directions, and it never flips an already-upfacing
    // triangle to satisfy such a constraint. On steep/sloped terrain,
    // 3D-adjacent faces project onto overlapping 2D regions, so their shared
    // edge reads as "same direction"; an edge-consistency 2-colouring would
    // flip one of them and turn a correct, upfacing source triangle downfacing
    // — exactly the "downfacing normal / opposite normals but linked / vertices
    // do not match" cascade CK logs on the FO76->FO4 output. Decide each
    // triangle's flip purely from its own projected normal. FO76 source
    // navmeshes are uniformly upfacing, so this yields zero flips for them and
    // matches CK. Linking across the (possibly same-direction) shared edge is
    // handled independently in `finalize_local_navmesh_links`.
    let mut by_form = HashMap::with_capacity(form_ids.len());
    let mut conflicts = 0_u32;
    for form_id in form_ids {
        let snapshot = snapshots
            .get(form_id)
            .ok_or_else(|| format!("missing NAVM snapshot {form_id:08X}"))?;
        let mut flips = vec![false; snapshot.triangles.len()];
        for (index, triangle) in snapshot.triangles.iter().enumerate() {
            let (Some(a), Some(b), Some(c)) = (
                snapshot.vertices.get(triangle.vertices[0] as usize),
                snapshot.vertices.get(triangle.vertices[1] as usize),
                snapshot.vertices.get(triangle.vertices[2] as usize),
            ) else {
                continue;
            };
            // cross_z == 0 (degenerate / vertical-projection triangle) is left
            // as-is: there is no upfacing direction to choose, and CK does not
            // flip it. Only a strictly downfacing source triangle is flipped.
            if projected_triangle_cross_z_target(*a, *b, *c) < 0.0 {
                flips[index] = true;
                conflicts = conflicts.saturating_add(1);
            }
        }
        by_form.insert(*form_id, flips);
    }
    Ok(NvnmWindingSolution {
        flips_by_form: by_form,
        conflicts,
    })
}

#[cfg(test)]
fn rewrite_finalized_nvnm_portal_payload(
    data: &[u8],
    finalized: &NvnmFinalizedPayload,
) -> Result<Option<Vec<u8>>, String> {
    rewrite_finalized_nvnm_portal_payload_inner(data, finalized, None)
}

fn rewrite_finalized_nvnm_portal_payload_with_door_refs(
    data: &[u8],
    finalized: &NvnmFinalizedPayload,
    own_index: u32,
    valid_door_refs: &HashSet<u32>,
) -> Result<Option<Vec<u8>>, String> {
    rewrite_finalized_nvnm_portal_payload_inner(data, finalized, Some((own_index, valid_door_refs)))
}

fn rewrite_finalized_nvnm_portal_payload_inner(
    data: &[u8],
    finalized: &NvnmFinalizedPayload,
    door_filter: Option<(u32, &HashSet<u32>)>,
) -> Result<Option<Vec<u8>>, String> {
    let source = parse_nvnm(data).map_err(|e| format!("NVNM parse: {e}"))?;
    if source.triangles.len() != finalized.triangles.len() {
        return Err(format!(
            "finalized NAVM triangle count {} does not match payload triangle count {}",
            finalized.triangles.len(),
            source.triangles.len()
        ));
    }

    let mut payload = source.clone();
    let mut changed = false;

    // Apply finalized triangles back into the structured payload. The 9-byte
    // cover_marker spans flag bytes [5..7]; we mirror the finalized flag word
    // into cover_marker[5..7] so write_nvnm emits the up-to-date flags
    // (cover_marker is what's serialized).
    for (old, new) in payload.triangles.iter_mut().zip(&finalized.triangles) {
        if old.vertices != new.vertices || old.links != new.links || old.flags != new.flags {
            changed = true;
        }
        old.vertices = new.vertices;
        old.links = new.links;
        old.flags = new.flags;
        old.cover_marker[5..7].copy_from_slice(&new.flags.to_le_bytes());
    }

    let prev_edge_links = payload.edge_links.len();
    if prev_edge_links != finalized.edge_rows.len()
        || payload
            .edge_links
            .iter()
            .zip(&finalized.edge_rows)
            .any(|(old, new)| old.row != *new)
    {
        changed = true;
    }
    payload.edge_links = finalized
        .edge_rows
        .iter()
        .map(|row| NvnmEdgeLink { row: *row })
        .collect();

    // Regenerate trailing sections (cover_array, cover_triangle_mappings,
    // waypoints, navmesh_grid, door_refs) from finalized topology. On
    // identity finalize this passes the source sections through unchanged;
    // otherwise it remaps triangle indices and drops orphans (door_refs
    // anchored to pruned triangles are dropped here).
    let regenerated = regenerate_nvnm_trailing(finalized, &source);
    payload.cover_array = regenerated.cover_array;
    payload.cover_triangle_mappings = regenerated.cover_triangle_mappings;
    payload.waypoints = regenerated.waypoints;
    payload.grid = regenerated.grid;
    if payload.door_refs.len() != regenerated.door_refs.len() {
        changed = true;
    }
    payload.door_refs = regenerated.door_refs;

    if let Some((own_index, valid_door_refs)) = door_filter {
        let original = payload.door_refs.len();
        payload
            .door_refs
            .retain(|d| should_keep_nvnm_door_ref(d.door_ref_form_id, own_index, valid_door_refs));
        if payload.door_refs.len() != original {
            changed = true;
        }
    }

    if !changed {
        return Ok(None);
    }

    Ok(Some(write_nvnm(&payload)))
}

fn should_keep_nvnm_door_ref(
    door_ref: u32,
    own_index: u32,
    valid_door_refs: &HashSet<u32>,
) -> bool {
    door_ref != 0 && ((door_ref >> 24) != own_index || valid_door_refs.contains(&door_ref))
}

/// The regenerated trailing sections of an NVNM payload: cover entries,
/// cover→triangle mappings, waypoints (island markers), the
/// edge-in-common-but-not-connected `navmesh_grid` that CK's Finalize step
/// rebuilds, and door_refs (anchor-triangle carryover only — form_id
/// filtering still happens in the rewrite path). Produced from finalized
/// topology + the source payload by `regenerate_nvnm_trailing`.
#[derive(Debug, Clone, PartialEq)]
struct NvnmRegeneratedTrailing {
    cover_array: Vec<esp_authoring_core::nvnm::NvnmCoverEntry>,
    cover_triangle_mappings: Vec<esp_authoring_core::nvnm::NvnmCoverTriangleMapping>,
    waypoints: Vec<esp_authoring_core::nvnm::NvnmWaypoint>,
    grid: esp_authoring_core::nvnm::NvnmGrid,
    door_refs: Vec<NvnmDoorRef>,
}

/// Build a `source_triangle_index -> Option<finalized_triangle_index>` map
/// keyed by the triangle's vertex set. Winding flips (1↔2 swap) keep the
/// vertex set unchanged, so finalized triangles are still findable.
///
/// Degenerate triangles (vertex_set contains a duplicate, e.g. [5,5,7]) are
/// dropped on BOTH sides: a finalized degenerate is never indexed, and a
/// source degenerate maps to `None`. CK rejects degenerate triangles as
/// invalid pathing surfaces, so safely skipping them prevents the consume-
/// cursor multimap from misassigning indices when two source degenerates
/// share the same vertex set but were swapped during finalize.
fn build_finalized_triangle_index_map(
    source_triangles: &[NvnmTriangle],
    finalized_triangles: &[NvnmFinalizedTriangle],
) -> Vec<Option<usize>> {
    fn vertex_set(v: [u16; 3]) -> [u16; 3] {
        let mut s = v;
        s.sort_unstable();
        s
    }
    fn is_degenerate(v: [u16; 3]) -> bool {
        v[0] == v[1] || v[1] == v[2] || v[0] == v[2]
    }
    let mut by_key: HashMap<[u16; 3], Vec<usize>> = HashMap::new();
    for (idx, t) in finalized_triangles.iter().enumerate() {
        if is_degenerate(t.vertices) {
            continue;
        }
        by_key.entry(vertex_set(t.vertices)).or_default().push(idx);
    }
    let mut consumed: HashMap<[u16; 3], usize> = HashMap::new();
    let mut out = Vec::with_capacity(source_triangles.len());
    for t in source_triangles {
        if is_degenerate(t.vertices) {
            out.push(None);
            continue;
        }
        let key = vertex_set(t.vertices);
        let cursor = consumed.entry(key).or_insert(0);
        let slot = by_key
            .get(&key)
            .and_then(|indices| indices.get(*cursor).copied());
        if slot.is_some() {
            *cursor += 1;
        }
        out.push(slot);
    }
    out
}

/// Rebuild the per-cell `triangle_indices` lists of an NVNM grid using FO4
/// CK's spatial-bucketing rule: every triangle whose XY-AABB overlaps a cell's
/// XY-AABB (inclusive on cell boundaries) is indexed into that cell's list.
///
/// The grid's `divisor`, `grid_size_*`, and `bounds_*` are preserved verbatim
/// from `source_grid`. Only `cells[*].triangle_indices` are regenerated.
///
/// Arithmetic is done in f32 so cell-boundary touches (e.g. `aabb.y_max` lands
/// exactly on `bounds_min_y + k * grid_size_y`) round identically to CK. The
/// inclusive rule uses `floor((aabb_max - bounds_min) / grid_size)` and
/// `..=cell_max`, so a triangle whose AABB max is exactly on a cell boundary
/// is indexed into both touching cells (the cell whose y_max is that boundary
/// AND the cell whose y_min is that boundary), matching the +36% "boundary
/// touch" entries observed in CK Finalize.
///
/// Within each cell, indices are sorted ascending (CK keeps them sorted —
/// verified 0/144 sort violations across the 13-NAVM fixture corpus).
///
/// Caller must guarantee `source_grid.divisor > 0`; with a zero divisor the
/// grid is conceptually "absent" and CK will rebuild it itself on Finalize.
fn rebuild_nvnm_grid_fo4(
    source_grid: &esp_authoring_core::nvnm::NvnmGrid,
    vertices: &[NvnmVertex],
    triangles: &[NvnmFinalizedTriangle],
) -> esp_authoring_core::nvnm::NvnmGrid {
    let div = source_grid.divisor as i32;
    let gsx = source_grid.grid_size_x;
    let gsy = source_grid.grid_size_y;
    let bmnx = source_grid.bounds_min_x;
    let bmny = source_grid.bounds_min_y;
    let max_cell = div - 1;
    // Spatial extent of the entire grid (max_cell + 1 cells of width gsx/gsy
    // starting at bmnx/bmny). Triangles whose AABB lies fully outside this
    // rectangle don't belong in any cell — without this early-skip the
    // negative `xmin` would clamp to cell 0 and `xmax` past the rightmost
    // cell would clamp to `max_cell`, indexing the triangle into cells whose
    // AABB it doesn't actually overlap.
    let grid_x_max = bmnx + (div as f32) * gsx;
    let grid_y_max = bmny + (div as f32) * gsy;

    let cell_total = (source_grid.divisor as usize).saturating_mul(source_grid.divisor as usize);
    let mut cells: Vec<esp_authoring_core::nvnm::NvnmGridCell> =
        vec![esp_authoring_core::nvnm::NvnmGridCell::default(); cell_total];

    for (tri_idx, tri) in triangles.iter().enumerate() {
        // Skip triangles that can't be indexed: degenerate, or any vertex
        // missing from the vertex array (corruption guard).
        let (Some(a), Some(b), Some(c)) = (
            vertices.get(tri.vertices[0] as usize),
            vertices.get(tri.vertices[1] as usize),
            vertices.get(tri.vertices[2] as usize),
        ) else {
            continue;
        };
        let Ok(idx_i16) = i16::try_from(tri_idx) else {
            continue; // grid cells use i16; out-of-range indices can't appear in a real NVNM
        };

        // f32-precision AABB. CK's spatial test uses f32 throughout — using
        // f64 here would mis-bucket triangles whose AABB max lands exactly
        // on a cell boundary (the divide ratio comes out a hair below k in
        // f64 but equals k in f32).
        let xmin = a.x.min(b.x).min(c.x);
        let xmax = a.x.max(b.x).max(c.x);
        let ymin = a.y.min(b.y).min(c.y);
        let ymax = a.y.max(b.y).max(c.y);

        // Skip triangles whose XY-AABB is entirely outside the grid's
        // spatial extent. Without this, the clamp(0, max_cell) below would
        // index out-of-bounds triangles into the boundary cells (cell 0 for
        // triangles to the left/below, cell max_cell for triangles to the
        // right/above), even though their AABB doesn't actually overlap
        // those cells.
        if xmax < bmnx || xmin > grid_x_max || ymax < bmny || ymin > grid_y_max {
            continue;
        }

        // `.floor() as i32` truncates toward zero, NOT toward -∞, so for
        // negative values (vertex left/below the grid origin) we need an
        // explicit `.floor()` before the cast.
        let cx_min = (((xmin - bmnx) / gsx).floor() as i32).clamp(0, max_cell);
        let cx_max = (((xmax - bmnx) / gsx).floor() as i32).clamp(0, max_cell);
        let cy_min = (((ymin - bmny) / gsy).floor() as i32).clamp(0, max_cell);
        let cy_max = (((ymax - bmny) / gsy).floor() as i32).clamp(0, max_cell);

        for cy in cy_min..=cy_max {
            for cx in cx_min..=cx_max {
                let ci = (cy as usize) * (source_grid.divisor as usize) + (cx as usize);
                cells[ci].triangle_indices.push(idx_i16);
            }
        }
    }

    for cell in cells.iter_mut() {
        cell.triangle_indices.sort_unstable();
    }

    esp_authoring_core::nvnm::NvnmGrid {
        divisor: source_grid.divisor,
        grid_size_x: gsx,
        grid_size_y: gsy,
        bounds_min_x: bmnx,
        bounds_min_y: bmny,
        bounds_min_z: source_grid.bounds_min_z,
        bounds_max_x: source_grid.bounds_max_x,
        bounds_max_y: source_grid.bounds_max_y,
        bounds_max_z: source_grid.bounds_max_z,
        cells,
    }
}

/// Regenerate NVNM trailing sections (cover_array, cover_triangle_mappings,
/// waypoints, navmesh_grid) from finalized topology + the source payload.
///
/// Triangle indices in waypoints, cover_triangle_mappings, and grid cells are
/// remapped through the source→finalized index map (vertex-set keyed).
/// Unmappable indices (triangle pruned) are dropped: waypoints anchored to a
/// dropped triangle vanish, mappings pointing at dropped triangles drop,
/// covers that lose all their mappings drop, and grid cells filter dropped
/// indices.
///
/// When finalize is identity (every source triangle maps to itself), the
/// source grid is passed through unchanged — CK's exact cell-intersection
/// rule is not byte-reproducible from the schema spec alone, so we trust the
/// source's grid for identity paths and only regenerate when the topology
/// actually changed. A separate NVNM structural validator catches genuine
/// connectivity errors.
fn regenerate_nvnm_trailing(
    finalized: &NvnmFinalizedPayload,
    source: &NvnmPayload,
) -> NvnmRegeneratedTrailing {
    let index_map = build_finalized_triangle_index_map(&source.triangles, &finalized.triangles);

    let identity_finalize = index_map
        .iter()
        .enumerate()
        .all(|(src, dst)| *dst == Some(src))
        && index_map.len() == finalized.triangles.len();

    // Cover array + mappings: drop mappings whose triangle was pruned, then
    // drop covers that lost all their mappings (renumber surviving covers).
    let cover_array;
    let cover_triangle_mappings;
    if identity_finalize {
        cover_array = source.cover_array.clone();
        cover_triangle_mappings = source.cover_triangle_mappings.clone();
    } else {
        let mut surviving_mappings: Vec<esp_authoring_core::nvnm::NvnmCoverTriangleMapping> =
            Vec::new();
        for m in &source.cover_triangle_mappings {
            if m.triangle < 0 {
                surviving_mappings.push(*m);
                continue;
            }
            let src_idx = m.triangle as usize;
            if let Some(Some(new_idx)) = index_map.get(src_idx) {
                let new_tri = i16::try_from(*new_idx).unwrap_or(-1);
                surviving_mappings.push(esp_authoring_core::nvnm::NvnmCoverTriangleMapping {
                    cover: m.cover,
                    triangle: new_tri,
                });
            }
        }
        let mut cover_keep: HashSet<u16> = HashSet::new();
        for m in &surviving_mappings {
            cover_keep.insert(m.cover);
        }
        let mut new_cover: Vec<esp_authoring_core::nvnm::NvnmCoverEntry> = Vec::new();
        let mut cover_remap: HashMap<u16, u16> = HashMap::new();
        for (idx, c) in source.cover_array.iter().enumerate() {
            let src_cover = idx as u16;
            if !cover_keep.contains(&src_cover) {
                continue;
            }
            let new_idx = new_cover.len() as u16;
            cover_remap.insert(src_cover, new_idx);
            new_cover.push(*c);
        }
        cover_array = new_cover;
        cover_triangle_mappings = surviving_mappings
            .into_iter()
            .filter_map(|m| {
                cover_remap.get(&m.cover).map(|&new_cover| {
                    esp_authoring_core::nvnm::NvnmCoverTriangleMapping {
                        cover: new_cover,
                        triangle: m.triangle,
                    }
                })
            })
            .collect();
    }

    // Waypoints: drop if anchor triangle pruned; rewrite triangle index
    // otherwise.
    let waypoints: Vec<esp_authoring_core::nvnm::NvnmWaypoint> = if identity_finalize {
        source.waypoints.clone()
    } else {
        source
            .waypoints
            .iter()
            .filter_map(|w| {
                if w.triangle < 0 {
                    return Some(*w);
                }
                let src_idx = w.triangle as usize;
                let new_idx = index_map.get(src_idx).copied().flatten()?;
                let new_tri = i16::try_from(new_idx).ok()?;
                Some(esp_authoring_core::nvnm::NvnmWaypoint {
                    triangle: new_tri,
                    ..*w
                })
            })
            .collect()
    };

    // navmesh_grid: rebuild cell triangle_indices from the finalized topology
    // using FO4 CK's "every triangle's XY-AABB → every cell whose AABB overlaps"
    // rule. CK's Finalize rebuilds the grid this way regardless of whether
    // finalize changed anything; passing the FO76 source grid through verbatim
    // leaves cells under-indexed and triggers PATHFINDING warnings even when
    // edge_links are byte-correct.
    //
    // Bounds/divisor/grid_size are preserved unchanged from source. Only the
    // per-cell triangle_indices list is regenerated.
    //
    // divisor == 0 means "no grid emitted" — pass through; CK builds its own
    // grid on Finalize for such plugins.
    let grid = if source.grid.divisor == 0 {
        source.grid.clone()
    } else {
        rebuild_nvnm_grid_fo4(&source.grid, &source.vertices, &finalized.triangles)
    };

    // door_refs: drop entries whose anchor triangle was pruned; remap
    // anchor-triangle index through the source->finalized map. Negative
    // triangle indices (no-anchor sentinel) and form_id filtering are
    // preserved unchanged here; the caller still applies own-plugin
    // door-ref filtering.
    let door_refs: Vec<NvnmDoorRef> = if identity_finalize {
        source.door_refs.clone()
    } else {
        source
            .door_refs
            .iter()
            .filter_map(|d| {
                if d.triangle_index < 0 {
                    return Some(*d);
                }
                let src_idx = d.triangle_index as usize;
                let new_idx = index_map.get(src_idx).copied().flatten()?;
                let new_tri = i16::try_from(new_idx).ok()?;
                Some(NvnmDoorRef {
                    triangle_index: new_tri,
                    ..*d
                })
            })
            .collect()
    };

    NvnmRegeneratedTrailing {
        cover_array,
        cover_triangle_mappings,
        waypoints,
        grid,
        door_refs,
    }
}

fn add_external_final_link_pair(
    finalized: &mut HashMap<u32, NvnmFinalizedPayload>,
    left: NvnmFinalEdgeRef,
    right: NvnmFinalEdgeRef,
) -> Result<(), String> {
    let left_payload = finalized
        .get(&left.form_id)
        .ok_or_else(|| format!("missing finalized NAVM {:08X}", left.form_id))?;
    let right_payload = finalized
        .get(&right.form_id)
        .ok_or_else(|| format!("missing finalized NAVM {:08X}", right.form_id))?;
    if left_payload.triangles[left.triangle].links[left.slot] >= 0
        || right_payload.triangles[right.triangle].links[right.slot] >= 0
        || left_payload.edge_rows.len() >= NVNM_MAX_FO4_EDGE_ROWS
        || right_payload.edge_rows.len() >= NVNM_MAX_FO4_EDGE_ROWS
    {
        return Ok(());
    }

    add_external_final_link(finalized, left, right)?;
    add_external_final_link(finalized, right, left)?;
    Ok(())
}

fn add_external_final_link_pair_with_stats(
    finalized: &mut HashMap<u32, NvnmFinalizedPayload>,
    left: NvnmFinalEdgeRef,
    right: NvnmFinalEdgeRef,
    stats: &mut NavmeshFinalizeStats,
) -> Result<(), String> {
    let left_payload = finalized
        .get(&left.form_id)
        .ok_or_else(|| format!("missing finalized NAVM {:08X}", left.form_id))?;
    let right_payload = finalized
        .get(&right.form_id)
        .ok_or_else(|| format!("missing finalized NAVM {:08X}", right.form_id))?;
    if left_payload.triangles[left.triangle].links[left.slot] >= 0
        || right_payload.triangles[right.triangle].links[right.slot] >= 0
    {
        return Ok(());
    }
    if left_payload.edge_rows.len() >= NVNM_MAX_FO4_EDGE_ROWS
        || right_payload.edge_rows.len() >= NVNM_MAX_FO4_EDGE_ROWS
    {
        stats.external_link_caps_hit += 1;
        return Ok(());
    }

    add_external_final_link(finalized, left, right)?;
    add_external_final_link(finalized, right, left)?;
    Ok(())
}

fn add_external_final_link(
    finalized: &mut HashMap<u32, NvnmFinalizedPayload>,
    from: NvnmFinalEdgeRef,
    to: NvnmFinalEdgeRef,
) -> Result<(), String> {
    let payload = finalized
        .get_mut(&from.form_id)
        .ok_or_else(|| format!("missing finalized NAVM {:08X}", from.form_id))?;
    if payload.triangles[from.triangle].links[from.slot] >= 0 {
        return Ok(());
    }
    if payload.edge_rows.len() >= NVNM_MAX_FO4_EDGE_ROWS {
        return Ok(());
    }
    let row_index = checked_i16_target(payload.edge_rows.len(), "NVNM edge link row")?;
    let target_triangle = checked_i16_target(to.triangle, "NVNM external triangle link")?;
    payload
        .edge_rows
        .push(portal_row_bytes(to.form_id, target_triangle, to.slot as u8));
    payload.triangles[from.triangle].links[from.slot] = row_index;
    payload.triangles[from.triangle].flags |= nvnm_edge_extra_info_flag(from.slot);
    Ok(())
}

fn portal_row_bytes(navmesh: u32, triangle: i16, edge: u8) -> [u8; 11] {
    let mut row = [0u8; 11];
    row[4..8].copy_from_slice(&navmesh.to_le_bytes());
    row[8..10].copy_from_slice(&triangle.to_le_bytes());
    row[10] = edge;
    row
}

fn global_triangle_edge(
    snapshot: &NvnmPortalSnapshot,
    vertices: [u16; 3],
    slot: usize,
) -> Result<[NvnmEdgePoint; 2], String> {
    let edge = triangle_edge_vertices_target(vertices, slot);
    let a = snapshot
        .vertices
        .get(edge[0] as usize)
        .ok_or_else(|| format!("NVNM triangle vertex {} exceeds vertex array", edge[0]))?;
    let b = snapshot
        .vertices
        .get(edge[1] as usize)
        .ok_or_else(|| format!("NVNM triangle vertex {} exceeds vertex array", edge[1]))?;
    Ok([
        quantized_edge_point(snapshot, *a),
        quantized_edge_point(snapshot, *b),
    ])
}

fn projected_triangle_cross_z_target(a: NvnmVertex, b: NvnmVertex, c: NvnmVertex) -> f64 {
    let ab_x = (b.x - a.x) as f64;
    let ab_y = (b.y - a.y) as f64;
    let ac_x = (c.x - a.x) as f64;
    let ac_y = (c.y - a.y) as f64;
    ab_x * ac_y - ab_y * ac_x
}

fn quantized_edge_point(snapshot: &NvnmPortalSnapshot, vertex: NvnmVertex) -> NvnmEdgePoint {
    let (x, y) = if let Some((cell_x, cell_y)) = snapshot.exterior_cell {
        if nvnm_vertex_uses_worldspace_xy((cell_x, cell_y), vertex) {
            (vertex.x, vertex.y)
        } else {
            (
                vertex.x + cell_x as f32 * NVNM_WORLDSPACE_CELL_SIZE,
                vertex.y + cell_y as f32 * NVNM_WORLDSPACE_CELL_SIZE,
            )
        }
    } else {
        (vertex.x, vertex.y)
    };
    NvnmEdgePoint {
        x: (x * NVNM_EDGE_POINT_SCALE).round() as i64,
        y: (y * NVNM_EDGE_POINT_SCALE).round() as i64,
        z: (vertex.z * NVNM_EDGE_POINT_SCALE).round() as i64,
    }
}

fn nvnm_vertex_uses_worldspace_xy(cell: (i16, i16), vertex: NvnmVertex) -> bool {
    let (cell_x, cell_y) = cell;
    let origin_x = cell_x as f32 * NVNM_WORLDSPACE_CELL_SIZE;
    let origin_y = cell_y as f32 * NVNM_WORLDSPACE_CELL_SIZE;
    (cell_x != 0 && (vertex.x - origin_x).abs() < vertex.x.abs())
        || (cell_y != 0 && (vertex.y - origin_y).abs() < vertex.y.abs())
}

fn normalized_global_edge_key(
    parent: NvnmPortalParent,
    oriented: [NvnmEdgePoint; 2],
) -> NvnmGlobalEdgeKey {
    if oriented[0] <= oriented[1] {
        NvnmGlobalEdgeKey {
            parent,
            a: oriented[0],
            b: oriented[1],
        }
    } else {
        NvnmGlobalEdgeKey {
            parent,
            a: oriented[1],
            b: oriented[0],
        }
    }
}

fn normalized_local_edge(oriented: [u16; 2]) -> [u16; 2] {
    if oriented[0] <= oriented[1] {
        oriented
    } else {
        [oriented[1], oriented[0]]
    }
}

fn opposite_global_edges(left: [NvnmEdgePoint; 2], right: [NvnmEdgePoint; 2]) -> bool {
    left[0] == right[1] && left[1] == right[0]
}

fn opposite_local_edges(left: [u16; 2], right: [u16; 2]) -> bool {
    left[0] == right[1] && left[1] == right[0]
}

fn checked_i16_target(value: usize, label: &str) -> Result<i16, String> {
    i16::try_from(value).map_err(|_| format!("{label} exceeds i16"))
}

fn nvnm_edge_extra_info_flag(slot: usize) -> u16 {
    NVNM_TRIANGLE_EDGE_EXTRA_INFO_FLAGS[slot]
}

fn triangle_edge_vertices_target(vertices: [u16; 3], slot: usize) -> [u16; 2] {
    match slot {
        0 => [vertices[0], vertices[1]],
        1 => [vertices[1], vertices[2]],
        2 => [vertices[2], vertices[0]],
        _ => unreachable!("triangle edge slot is always 0..3"),
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WorldspaceGroupRebuildStats {
    pub groups_rebuilt: usize,
    pub records_nested: usize,
    pub flat_records_removed: usize,
}

pub fn rebuild_worldspace_groups_from_source_native(
    target_handle_id: u64,
    source_handle_id: u64,
    source_to_target_formids: &[(u32, u32)],
) -> Result<WorldspaceGroupRebuildStats, WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    // Borrow source + target disjointly instead of cloning the whole source
    // `root_items` (~5.5M records); the topology rebuild below only *reads* source.
    let [source_slot, target_slot] = store.get_disjoint_mut([&source_handle_id, &target_handle_id]);
    let source_root_items = &source_slot
        .ok_or_else(|| {
            WriteError::InsertFailure(format!("no source plugin handle: {source_handle_id}"))
        })?
        .parsed
        .root_items;
    let target = target_slot.ok_or_else(|| {
        WriteError::InsertFailure(format!("no target plugin handle: {target_handle_id}"))
    })?;

    // Allocation can relocate a source record when its object id collides in the
    // target. Only the mapper preserves that identity across the topology copy.
    let mut records_by_form_id = HashMap::new();
    collect_target_records_by_form_id(&target.parsed.root_items, &mut records_by_form_id);
    let source_to_target_formids: HashMap<u32, u32> =
        source_to_target_formids.iter().copied().collect();

    let mut stats = WorldspaceGroupRebuildStats::default();
    let mut nested_form_ids = HashSet::new();
    let group_tail = Bytes::from(vec![0u8; target.parsed.header_size.saturating_sub(16)]);
    let mut rebuilt_groups = Vec::new();

    for item in source_root_items {
        let ParsedItem::Group(group) = item else {
            continue;
        };
        if !is_rebuilt_topology_group(group) {
            continue;
        }
        let rebuilt = rebuild_worldspace_group(
            group,
            &records_by_form_id,
            &source_to_target_formids,
            &mut nested_form_ids,
            &group_tail,
            &mut stats,
        );
        if !rebuilt.children.is_empty() {
            stats.groups_rebuilt += 1;
            rebuilt_groups.push((group.label, ParsedItem::Group(rebuilt)));
        }
    }

    if rebuilt_groups.is_empty() {
        return Ok(stats);
    }

    let mut root_items = Vec::with_capacity(target.parsed.root_items.len());
    let rebuilt_labels: HashSet<[u8; 4]> = rebuilt_groups.iter().map(|(label, _)| *label).collect();
    for item in target.parsed.root_items.drain(..) {
        if let ParsedItem::Group(group) = &item
            && group.group_type == 0
            && rebuilt_labels.contains(&group.label)
        {
            if let Some(index) = rebuilt_groups
                .iter()
                .position(|(label, _)| *label == group.label)
            {
                let (_, rebuilt) = rebuilt_groups.remove(index);
                root_items.push(rebuilt);
            }
            continue;
        }
        if let Some(filtered) =
            remove_nested_records_from_flat_groups(item, &nested_form_ids, &mut stats)
        {
            root_items.push(filtered);
        }
    }
    for (_, rebuilt) in rebuilt_groups {
        root_items.push(rebuilt);
    }

    target.parsed.root_items = root_items;
    target.clear_record_count_cache();
    target.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    Ok(stats)
}

fn collect_target_records_by_form_id<'a>(
    items: &'a [ParsedItem],
    records_by_form_id: &mut HashMap<u32, &'a ParsedRecord>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                if record.form_id != 0 {
                    records_by_form_id.entry(record.form_id).or_insert(record);
                }
            }
            ParsedItem::Group(group) => {
                collect_target_records_by_form_id(&group.children, records_by_form_id);
            }
        }
    }
}

fn rebuild_worldspace_group(
    source_group: &ParsedGroup,
    target_records_by_form_id: &HashMap<u32, &ParsedRecord>,
    source_to_target_formids: &HashMap<u32, u32>,
    nested_form_ids: &mut HashSet<u32>,
    group_tail: &Bytes,
    stats: &mut WorldspaceGroupRebuildStats,
) -> ParsedGroup {
    let mut children = Vec::new();
    for child in &source_group.children {
        match child {
            ParsedItem::Record(source_record) => {
                let Some(target_form_id) = source_to_target_formids.get(&source_record.form_id)
                else {
                    continue;
                };
                let Some(target_record) = target_records_by_form_id.get(target_form_id) else {
                    continue;
                };
                nested_form_ids.insert(target_record.form_id);
                stats.records_nested += 1;
                children.push(ParsedItem::Record((*target_record).clone()));
            }
            ParsedItem::Group(source_child_group) => {
                if matches!(source_child_group.group_type, 1 | 6 | 7 | 8 | 9 | 10)
                    && !source_to_target_formids
                        .contains_key(&u32::from_le_bytes(source_child_group.label))
                {
                    continue;
                }
                let rebuilt = rebuild_worldspace_group(
                    source_child_group,
                    target_records_by_form_id,
                    source_to_target_formids,
                    nested_form_ids,
                    group_tail,
                    stats,
                );
                if !rebuilt.children.is_empty() {
                    children.push(ParsedItem::Group(rebuilt));
                }
            }
        }
    }

    ParsedGroup {
        label: rewrite_worldspace_group_label(
            source_group.label,
            source_group.group_type,
            source_to_target_formids,
        ),
        group_type: source_group.group_type,
        tail: group_tail.clone(),
        children,
    }
}

fn rewrite_worldspace_group_label(
    label: [u8; 4],
    group_type: i32,
    source_to_target_formids: &HashMap<u32, u32>,
) -> [u8; 4] {
    if !matches!(group_type, 1 | 6 | 7 | 8 | 9 | 10) {
        return label;
    }

    let source_form_id = u32::from_le_bytes(label);
    if source_form_id == 0 {
        return label;
    }
    source_to_target_formids
        .get(&source_form_id)
        .copied()
        .unwrap_or(source_form_id)
        .to_le_bytes()
}

fn remove_nested_records_from_flat_groups(
    item: ParsedItem,
    nested_form_ids: &HashSet<u32>,
    stats: &mut WorldspaceGroupRebuildStats,
) -> Option<ParsedItem> {
    match item {
        ParsedItem::Record(record) => {
            if nested_form_ids.contains(&record.form_id) {
                stats.flat_records_removed += 1;
                None
            } else {
                Some(ParsedItem::Record(record))
            }
        }
        ParsedItem::Group(mut group) => {
            let mut children = Vec::with_capacity(group.children.len());
            for child in group.children {
                if let Some(filtered) =
                    remove_nested_records_from_flat_groups(child, nested_form_ids, stats)
                {
                    children.push(filtered);
                }
            }
            if children.is_empty() {
                None
            } else {
                group.children = children;
                Some(ParsedItem::Group(group))
            }
        }
    }
}

fn is_top_group(group: &ParsedGroup, label: &[u8; 4]) -> bool {
    group.group_type == 0 && group.label == *label
}

fn is_rebuilt_topology_group(group: &ParsedGroup) -> bool {
    is_top_group(group, b"WRLD") || is_top_group(group, b"CELL")
}

pub(crate) fn encode_record_for_slot(
    slot: &mut NativePluginSlot,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<Option<ParsedRecord>, WriteError> {
    let masters = slot.parsed.header.masters.clone();
    let target_game = slot.parsed.game.clone();
    let target_is_localized = (slot.parsed.header.flags & 0x0000_0080) != 0;
    let master_map = build_master_lookup(&masters);
    let own_index = (masters.len() & 0xFF) as u32;
    let mut ctx = EncodeContext {
        interner,
        master_map: &master_map,
        own_index,
        target_is_localized,
        slot: Some(slot),
        localized_strings: None,
    };
    encode_record_for_target(record, schema, &mut ctx, target_game.as_deref())
}

struct EncodeContext<'a> {
    interner: &'a StringInterner,
    master_map: &'a HashMap<String, u32>,
    own_index: u32,
    target_is_localized: bool,
    slot: Option<&'a mut NativePluginSlot>,
    localized_strings: Option<&'a mut LocalizedStringsState>,
}

fn encode_record_for_target(
    record: Record,
    schema: &AuthoringSchema,
    ctx: &mut EncodeContext<'_>,
    target_game: Option<&str>,
) -> Result<Option<ParsedRecord>, WriteError> {
    let mut record = match TargetRecordNormalizer::target_only_with_interner(schema, ctx.interner)
        .normalize(record)
    {
        TargetRecordNormalization::Keep(record) => record,
        TargetRecordNormalization::DropUnsupportedRecord => return Ok(None),
    };
    redirect_npc_tpta_self_refs_before_encode(&mut record, ctx);
    let sig_str = record.sig.as_str().to_string();
    let mut subrecords: Vec<ParsedSubrecord> = Vec::with_capacity(record.fields.len());

    let record_def = schema
        .record_def(&sig_str)
        .ok_or(WriteError::UnknownSignature(record.sig))?;

    let is_pack_record = sig_str == "PACK";
    let mut in_pack_package_data = false;
    let mut in_pack_procedure_tree = false;
    let is_term_record = sig_str == "TERM";
    let mut seen_term_xmrk = false;
    let is_scen_record = sig_str == "SCEN";
    let mut seen_scen_body_vnam = false;
    let is_perk_record = sig_str == "PERK";
    let mut in_perk_effect = false;
    for entry in &record.fields {
        let subrec_sig = entry.sig.as_str();
        if is_term_record
            && subrec_sig == "SNAM"
            && !seen_term_xmrk
            && is_null_term_looping_sound(record_def, entry, ctx)
        {
            continue;
        }
        let subrec_bytes =
            if sig_str == "SCEN" && subrec_sig == "ANAM" && is_empty_subrecord_marker(entry) {
                Vec::new()
            } else if is_pack_record && in_pack_package_data && subrec_sig == "CNAM" {
                encode_pack_package_data_cnam(entry, ctx)?
            } else if is_pack_record && in_pack_procedure_tree && subrec_sig == "PNAM" {
                encode_pack_procedure_tree_pnam(entry, ctx)?
            } else {
                let selected_subrecord_def = if is_term_record && subrec_sig == "SNAM" {
                    term_snam_def_for_position(record_def, seen_term_xmrk)
                } else if is_scen_record {
                    scen_tnam_def_for_entry(record_def, entry, seen_scen_body_vnam)
                } else if is_perk_record && in_perk_effect && subrec_sig == "DATA" {
                    perk_effect_data_def(record_def)
                } else {
                    None
                };
                if let Some(selected_subrecord_def) = selected_subrecord_def {
                    encode_field_with_selected_subrecord_def(
                        entry,
                        Some(record_def),
                        Some(selected_subrecord_def),
                        ctx,
                    )?
                } else {
                    encode_field(entry, Some(record_def), ctx)?
                }
            };
        subrecords.push(ParsedSubrecord {
            signature: SmolStr::new(subrec_sig),
            data: Bytes::from(subrec_bytes),
            semantic_type: None,
        });

        if is_term_record && subrec_sig == "XMRK" {
            seen_term_xmrk = true;
        }
        if is_scen_record && subrec_sig == "VNAM" {
            seen_scen_body_vnam = true;
        }
        if is_perk_record {
            match subrec_sig {
                "PRKE" => in_perk_effect = true,
                "PRKF" => in_perk_effect = false,
                _ => {}
            }
        }

        if is_pack_record {
            match subrec_sig {
                "PKCU" => {
                    in_pack_package_data = true;
                    in_pack_procedure_tree = false;
                }
                "XNAM" => {
                    in_pack_package_data = false;
                    in_pack_procedure_tree = true;
                }
                "UNAM" | "BNAM" | "POBA" | "POEA" | "POCA" if in_pack_procedure_tree => {
                    in_pack_procedure_tree = false;
                }
                _ => {}
            }
        }
    }
    apply_fo4_ck_payload_limits(&sig_str, &mut subrecords, target_game);

    // ── 4. Compute raw form_id for the target plugin ───────────────────────
    let raw_form_id = encode_formkey_raw_before_encode(record.form_key, ctx);

    let mut record_flags = record.flags.bits();
    if force_compressed_for_target(&sig_str, target_game) {
        // Stamp COMPRESSED so io::record_bytes_from_parsed gzips the payload
        // on emit (CK requires CELL and LAND to be compressed in FO4 plugins;
        // FO76 sources sometimes ship them uncompressed and we'd otherwise
        // emit invalid plugins).
        record_flags |= COMPRESSED_RECORD_FLAG;
    }

    // Record-header normalization: version_control is always reset to 0
    // (the source's ESM revision counter has no meaning in the target
    // plugin), and form_version is forced to the target's modern value
    // (131 for FO4 via `default_form_version_for_game`). Mutating
    // ParsedRecord here means every downstream emitter sees already-
    // normalized values without a separate byte-emit-time wrapper.
    let parsed_record = ParsedRecord {
        signature: SmolStr::new(&sig_str),
        form_id: raw_form_id,
        flags: record_flags,
        version_control: 0,
        form_version: default_form_version_for_game(target_game),
        version2: None,
        subrecords,
        raw_payload: None,
        parse_error: None,
    };

    Ok(Some(parsed_record))
}

fn redirect_npc_tpta_self_refs_before_encode(record: &mut Record, ctx: &EncodeContext<'_>) -> bool {
    if record.sig.as_str() != "NPC_" || record.form_key.local == 0 {
        return false;
    }
    let object_id = record.form_key.local & 0x00FF_FFFF;
    let self_raw = (ctx.own_index << 24) | object_id;
    let default_template_raw = npc_default_template_raw_before_encode(record, ctx)
        .filter(|raw| *raw != 0 && *raw != self_raw)
        .unwrap_or(0);

    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if entry.sig.as_str() != "TPTA" {
            continue;
        }
        changed |=
            redirect_npc_tpta_self_value(&mut entry.value, self_raw, default_template_raw, ctx);
    }
    changed
}

fn npc_default_template_raw_before_encode(record: &Record, ctx: &EncodeContext<'_>) -> Option<u32> {
    for entry in &record.fields {
        if entry.sig.as_str() != "TPLT" {
            continue;
        }
        return raw_formid_value_before_encode(&entry.value, ctx);
    }
    None
}

fn redirect_npc_tpta_self_value(
    value: &mut FieldValue,
    self_raw: u32,
    default_template_raw: u32,
    ctx: &EncodeContext<'_>,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) => {
            let mut changed = false;
            for offset in (0..bytes.len()).step_by(4) {
                let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
                    continue;
                };
                let raw = u32::from_le_bytes(chunk.try_into().unwrap());
                if raw != self_raw {
                    continue;
                }
                chunk.copy_from_slice(&default_template_raw.to_le_bytes());
                changed = true;
            }
            changed
        }
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            redirect_npc_tpta_self_value(item, self_raw, default_template_raw, ctx) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            redirect_npc_tpta_self_value(item, self_raw, default_template_raw, ctx) | changed
        }),
        _ => {
            let Some(raw) = raw_formid_value_before_encode(value, ctx) else {
                return false;
            };
            if raw != self_raw {
                return false;
            }
            *value = FieldValue::Uint(default_template_raw as u64);
            true
        }
    }
}

fn raw_formid_value_before_encode(value: &FieldValue, ctx: &EncodeContext<'_>) -> Option<u32> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[0..4].try_into().unwrap()))
        }
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => Some(*raw as u32),
        FieldValue::Int(raw) if *raw >= 0 && *raw <= u32::MAX as i64 => Some(*raw as u32),
        FieldValue::FormKey(fk) => Some(encode_formkey_raw_before_encode(*fk, ctx)),
        _ => None,
    }
}

fn encode_formkey_raw_before_encode(fk: crate::ids::FormKey, ctx: &EncodeContext<'_>) -> u32 {
    let object_id = fk.local & 0x00FF_FFFF;
    if object_id == 0 {
        return 0;
    }
    let master_index = ctx
        .interner
        .resolve(fk.plugin)
        .and_then(|name| ctx.master_map.get(&name.to_ascii_lowercase()).copied())
        .unwrap_or(ctx.own_index);
    (master_index << 24) | object_id
}

fn is_empty_subrecord_marker(entry: &FieldEntry) -> bool {
    matches!(entry.value, FieldValue::None)
        || matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.is_empty())
}

/// Strict canonical-name FO4 target check.
///
/// The canonical target-game identifier is the exact lowercase string
/// `"fo4"` produced by `Game::as_str()`. Any other casing (`"FO4"`,
/// `"Fallout4"`, etc.) is a caller bug — the conversion run params are
/// already a `Game` enum, so the canonical string is the only thing that
/// can reach this function in production. A case-insensitive comparison
/// here would silently no-op on any typo and hide the misconfig. In debug
/// builds the assertion below trips on non-canonical input; release
/// builds fall through to the safe no-op.
fn is_fo4_target(target_game: Option<&str>) -> bool {
    let Some(game) = target_game else {
        return false;
    };
    debug_assert!(
        game == game.to_ascii_lowercase().trim(),
        "target_game must be canonical-lowercase (got {game:?}); upstream caller bug"
    );
    game == "fo4"
}

fn force_compressed_for_target(record_sig: &str, target_game: Option<&str>) -> bool {
    if !is_fo4_target(target_game) {
        return false;
    }
    matches!(record_sig, "CELL" | "LAND")
}

fn apply_fo4_ck_payload_limits(
    record_sig: &str,
    subrecords: &mut Vec<ParsedSubrecord>,
    target_game: Option<&str>,
) {
    if !is_fo4_target(target_game) {
        return;
    }

    match record_sig {
        "SCEN" => truncate_scen_contextual_subrecords(subrecords),
        "FURN" => {
            subrecords.retain(|subrecord| {
                subrecord.signature.as_str() != "WBDT" || !subrecord.data.is_empty()
            });
            normalize_fo4_furniture_workbench_data_width(subrecords);
            normalize_fo4_projected_furniture_marker_parameters(subrecords);
        }
        "TERM" => {
            subrecords.retain(|subrecord| {
                subrecord.signature.as_str() != "WBDT" || !subrecord.data.is_empty()
            });
            truncate_subrecords(subrecords, "WBDT", 1);
        }
        "MOVT" => truncate_subrecords(subrecords, "SPED", 112),
        "ARMO" | "WEAP" => project_subrecord_rows(subrecords, "DAMA", 12, 8),
        _ => {}
    }
}

fn truncate_scen_contextual_subrecords(subrecords: &mut [ParsedSubrecord]) {
    for index in 0..subrecords.len() {
        let should_truncate = {
            let sig = subrecords[index].signature.as_str();
            let prev_sig = index
                .checked_sub(1)
                .map(|prev| subrecords[prev].signature.as_str());

            matches!(
                (prev_sig, sig),
                (_, "DATA") | (Some("WNAM"), "FNAM") | (Some("PNAM" | "SNAM"), "SCQS")
            )
        };

        let max_len = if subrecords[index].signature.as_str() == "DATA" {
            4
        } else {
            2
        };
        if should_truncate && subrecords[index].data.len() > max_len {
            subrecords[index].data = Bytes::copy_from_slice(&subrecords[index].data[..max_len]);
        }
    }
}

fn truncate_subrecords(subrecords: &mut [ParsedSubrecord], sig: &str, max_len: usize) {
    for subrecord in subrecords {
        if subrecord.signature.as_str() != sig || subrecord.data.len() <= max_len {
            continue;
        }
        subrecord.data = Bytes::copy_from_slice(&subrecord.data[..max_len]);
    }
}

fn normalize_fo4_furniture_workbench_data_width(subrecords: &mut [ParsedSubrecord]) {
    for subrecord in subrecords {
        if subrecord.signature.as_str() != "WBDT" {
            continue;
        }
        let preserve_legacy_tail = subrecord.data.first().copied().unwrap_or_default() == 0
            && subrecord.data.get(1).copied() == Some(u8::MAX);
        let max_len = if preserve_legacy_tail { 2 } else { 1 };
        if subrecord.data.len() > max_len {
            subrecord.data = Bytes::copy_from_slice(&subrecord.data[..max_len]);
        }
    }
}

fn normalize_fo4_projected_furniture_marker_parameters(subrecords: &mut [ParsedSubrecord]) {
    const MARKER_PARAMETERS_ROW_LEN: usize = 24;
    const FO4_MARKER_TAIL_LEN: usize = 3;

    let has_projected_workbench_data = subrecords
        .iter()
        .any(|subrecord| subrecord.signature.as_str() == "WBDT" && subrecord.data.len() == 1);
    if !has_projected_workbench_data {
        return;
    }

    for subrecord in subrecords {
        if subrecord.signature.as_str() != "SNAM"
            || subrecord.data.is_empty()
            || subrecord.data.len() % MARKER_PARAMETERS_ROW_LEN != 0
        {
            continue;
        }

        let mut data = subrecord.data.to_vec();
        for row in data.chunks_exact_mut(MARKER_PARAMETERS_ROW_LEN) {
            row[MARKER_PARAMETERS_ROW_LEN - FO4_MARKER_TAIL_LEN..].fill(u8::MAX);
        }
        subrecord.data = Bytes::from(data);
    }
}

fn project_subrecord_rows(
    subrecords: &mut [ParsedSubrecord],
    sig: &str,
    source_row_len: usize,
    target_row_len: usize,
) {
    if source_row_len <= target_row_len || target_row_len == 0 {
        return;
    }

    for subrecord in subrecords {
        if subrecord.signature.as_str() != sig
            || subrecord.data.is_empty()
            || subrecord.data.len() % source_row_len != 0
        {
            continue;
        }
        let mut projected =
            Vec::with_capacity(subrecord.data.len() / source_row_len * target_row_len);
        for row in subrecord.data.chunks_exact(source_row_len) {
            projected.extend_from_slice(&row[..target_row_len]);
        }
        subrecord.data = Bytes::from(projected);
    }
}

fn encode_pack_package_data_cnam(
    entry: &FieldEntry,
    ctx: &mut EncodeContext<'_>,
) -> Result<Vec<u8>, WriteError> {
    match &entry.value {
        FieldValue::Bytes(raw) => Ok(raw.to_vec()),
        FieldValue::None => Ok(Vec::new()),
        other => encode_value(None, other, None, entry.sig.as_str(), ctx),
    }
}

fn encode_pack_procedure_tree_pnam(
    entry: &FieldEntry,
    ctx: &mut EncodeContext<'_>,
) -> Result<Vec<u8>, WriteError> {
    if let Some(name) = pack_procedure_tree_name(&entry.value, ctx.interner) {
        let mut bytes = name.as_bytes().to_vec();
        bytes.push(0);
        return Ok(bytes);
    }
    encode_value(Some("zstring"), &entry.value, None, entry.sig.as_str(), ctx)
}

fn pack_procedure_tree_name(value: &FieldValue, interner: &StringInterner) -> Option<&'static str> {
    let bytes = match value {
        FieldValue::Bytes(raw) => trim_nul_suffix(raw.as_slice()).to_vec(),
        FieldValue::String(sym) => interner.resolve(*sym)?.as_bytes().to_vec(),
        _ => return None,
    };
    match bytes.as_slice() {
        b"Trav" | b"Travel" => Some("Travel"),
        b"Sand" | b"Sandbox" => Some("Sandbox"),
        b"Foll" | b"Follow" => Some("Follow"),
        b"Wait" => Some("Wait"),
        b"Patr" | b"Patrol" => Some("Patrol"),
        b"Sit" => Some("Sit"),
        b"UseW" | b"UseWeapon" => Some("UseWeapon"),
        b"Rang" | b"Range" => Some("Range"),
        b"Unlo" | b"UnlockDoors" => Some("UnlockDoors"),
        b"Acti" | b"Activate" => Some("Activate"),
        b"Find" => Some("Find"),
        b"Esco" | b"Escort" => Some("Escort"),
        b"Hold" | b"HoldPosition" => Some("HoldPosition"),
        b"Slee" | b"Sleep" => Some("Sleep"),
        b"Guar" | b"Guard" => Some("Guard"),
        b"Eat" => Some("Eat"),
        b"Say" | b"ForceGreet" => Some("ForceGreet"),
        b"Flee" => Some("Flee"),
        b"Head" | b"Headtrack" => Some("Headtrack"),
        b"Orbi" | b"Orbit" => Some("Orbit"),
        b"UseI" | b"UseIdleMarker" => Some("UseIdleMarker"),
        _ => None,
    }
}

fn trim_nul_suffix(mut bytes: &[u8]) -> &[u8] {
    while matches!(bytes.last(), Some(0)) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

/// Resolve the record-header `form_version` field for the target game.
///
/// Mirrors Python's `_build_authoring_record_dicts` policy
/// (fixups.py:7814): FO4 records use 131. Other games are left as `None`
/// so the parser uses its default (typically zero / not emitted), matching
/// Python's "else propagate from source" branch which also has no per-game
/// override.
fn default_form_version_for_game(game: Option<&str>) -> Option<u16> {
    match game {
        Some("fo4") => Some(131),
        _ => None,
    }
}

/// Delete any existing record with the same form_key from `handle_id`, then
/// insert the new `record`.
///
/// This is the canonical way to update an existing record in the pipeline:
/// delete-then-insert is simpler and safer than an in-place mutation through
/// the index.
///
///
/// # Errors
/// Returns `WriteError::InsertFailure` when the handle does not exist or the
/// raw form_id cannot be computed.
pub fn replace_record_native(
    handle_id: u64,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    replace_record_in_slot(slot, record, schema, interner)?;
    slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    Ok(())
}

pub fn replace_record_contents_native(
    handle_id: u64,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<bool, WriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    Ok(replace_record_contents_in_slot(slot, record, schema, interner)?.is_some())
}

pub(crate) fn replace_record_in_slot(
    slot: &mut NativePluginSlot,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), WriteError> {
    if let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)? {
        replace_parsed_record_in_slot(slot, parsed_record);
    }
    Ok(())
}

pub(crate) fn replace_record_contents_in_slot(
    slot: &mut NativePluginSlot,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<Option<u32>, WriteError> {
    let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)? else {
        return Ok(None);
    };
    let form_id = parsed_record.form_id;
    Ok(replace_parsed_record_contents_in_slot(slot, parsed_record).then_some(form_id))
}

pub(crate) fn replace_records_in_slot_batch(
    slot: &mut NativePluginSlot,
    records: Vec<Record>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), WriteError> {
    replace_records_in_slot_batch_with_encoder(slot, records, |slot, record| {
        encode_record_for_slot(slot, record, schema, interner)
    })
}

fn replace_records_in_slot_batch_with_encoder(
    slot: &mut NativePluginSlot,
    records: Vec<Record>,
    mut encode_record: impl FnMut(
        &mut NativePluginSlot,
        Record,
    ) -> Result<Option<ParsedRecord>, WriteError>,
) -> Result<(), WriteError> {
    let mut parsed_records = Vec::with_capacity(records.len());
    for record in records {
        match encode_record(slot, record) {
            Ok(Some(parsed_record)) => parsed_records.push(parsed_record),
            Ok(None) => {}
            Err(error) => {
                replace_parsed_records_in_slot_batch(slot, parsed_records);
                return Err(error);
            }
        }
    }
    replace_parsed_records_in_slot_batch(slot, parsed_records);
    Ok(())
}

pub(crate) fn replace_records_contents_in_slot(
    slot: &mut NativePluginSlot,
    records: Vec<Record>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<SmallVec<[u32; 4]>, WriteError> {
    let mut parsed_records = Vec::with_capacity(records.len());
    for record in records {
        if let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)? {
            parsed_records.push(parsed_record);
        }
    }

    // Single-pass batch replace: one traversal of the GRUP tree resolves every
    // record, instead of a per-record recursive scan (which is O(changed × n)
    // and blew up to ~27 min on the 3.36M-record FO76→FO4 output once a late
    // top-level group — QUST — was touched after the WRLD group ballooned).
    Ok(replace_parsed_records_contents_in_slot_batch(
        slot,
        parsed_records,
    ))
}

pub(crate) fn add_record_in_slot(
    slot: &mut NativePluginSlot,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), WriteError> {
    if let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)? {
        insert_parsed_record_in_slot(slot, parsed_record);
    }
    Ok(())
}

pub(crate) fn add_skyrim_navmesh_record_in_slot(
    slot: &mut NativePluginSlot,
    record: Record,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), WriteError> {
    let Some(parsed_record) = encode_record_for_slot(slot, record, schema, interner)? else {
        return Ok(());
    };
    let nvnm = parsed_record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "NVNM")
        .ok_or_else(|| {
            WriteError::EncodeFailure("Skyrim NAVM has no NVNM subrecord".to_string())
        })?;
    parse_nvnm(nvnm.data.as_ref())
        .map_err(|error| WriteError::EncodeFailure(format!("Skyrim NAVM NVNM: {error}")))?
        .parent;

    insert_parsed_record_in_slot(slot, parsed_record);
    Ok(())
}
pub fn replace_records_native(
    handle_id: u64,
    records: Vec<Record>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), WriteError> {
    if records.is_empty() {
        return Ok(());
    }

    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle_id}")))?;
    for record in records {
        replace_record_in_slot(slot, record, schema, interner)?;
    }
    slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    Ok(())
}

// ---------------------------------------------------------------------------
// Field encoding
// ---------------------------------------------------------------------------

/// Build a lookup from lower-cased master filename to its master index. Master
/// matching is ASCII case-insensitive (matches the parser/handle conventions
/// in `plugin_handle_add_master_native`).
fn build_master_lookup(masters: &[String]) -> HashMap<String, u32> {
    masters
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_ascii_lowercase(), i as u32))
        .collect()
}

/// Public wrapper around `encode_field` used by unit tests in other modules.
#[cfg(test)]
pub fn encode_field_pub(
    entry: &FieldEntry,
    record_def: Option<&crate::schema::RecordDef>,
    interner: &StringInterner,
) -> Result<Vec<u8>, WriteError> {
    // External callers don't know the target plugin's master list, so
    // FormKey leaves fall back to own-plugin encoding (own_index = 0).
    let empty: HashMap<String, u32> = HashMap::new();
    let mut ctx = EncodeContext {
        interner,
        master_map: &empty,
        own_index: 0,
        target_is_localized: false,
        slot: None,
        localized_strings: None,
    };
    encode_field(entry, record_def, &mut ctx)
}

fn encode_field(
    entry: &FieldEntry,
    record_def: Option<&crate::schema::RecordDef>,
    ctx: &mut EncodeContext<'_>,
) -> Result<Vec<u8>, WriteError> {
    let subrec_sig_str = entry.sig.as_str();
    let subrecord_def = record_def.and_then(|record_def| {
        if matches!(
            (record_def.id.as_str(), subrec_sig_str),
            ("TERM", "SNAM") | ("RACE", "BSMS")
        ) {
            duplicate_subrecord_def_for_entry(record_def, entry)
        } else {
            record_def.subrecord_def(subrec_sig_str)
        }
    });
    encode_field_with_selected_subrecord_def(entry, record_def, subrecord_def, ctx)
}

fn encode_field_with_selected_subrecord_def(
    entry: &FieldEntry,
    record_def: Option<&crate::schema::RecordDef>,
    subrecord_def: Option<&crate::schema::SubrecordDef>,
    ctx: &mut EncodeContext<'_>,
) -> Result<Vec<u8>, WriteError> {
    let subrec_sig_str = entry.sig.as_str();
    let record_sig = record_def.map(|rd| rd.id.as_str());
    let codec = subrecord_def.and_then(|sd| {
        if sd.kind == "raw" {
            None
        } else {
            sd.codec.as_deref()
        }
    });

    // If value is Bytes, emit as-is except for true fixed-size structs. Some
    // generated schemas model a fixed prefix plus a variable tail; truncating
    // those payloads corrupts their internal counts.
    if let FieldValue::Bytes(raw) = &entry.value {
        if is_localized_lstring_subrecord(subrecord_def, codec) {
            if let Some(string_id) = raw_lstring_id(raw) {
                if ctx.target_is_localized {
                    let target_id = localized_string_id_for_unresolved(
                        ctx,
                        record_sig,
                        subrec_sig_str,
                        string_id,
                    );
                    return Ok(target_id.to_le_bytes().to_vec());
                }
                return Ok(unresolved_lstring_placeholder(string_id).into_bytes());
            }
        }
        if raw_bytes_need_variable_tail_passthrough(record_def, subrec_sig_str) {
            return Ok(raw.to_vec());
        }
        // Heterogeneous-size unions (e.g. SNDR.BNAM: 6-byte `values` struct vs
        // 4-byte `base_descriptor` formid). The source payload is already a
        // legal variant width; the `subrecord_fixed_size` clamp below would pick
        // ONE variant's size (it iterates variants .rev(), returning the smaller
        // formid size for SNDR.BNAM) and truncate a valid 6-byte payload to 4.
        // If the raw length is within the variants' max width, pass it through
        // untouched; only clamp when raw exceeds the MAX variant size (the
        // genuine over-long case). Single-variant version unions (e.g.
        // EFSH.DNAM) skip this guard and keep the existing clamp below.
        if let Some(def) = subrecord_def {
            let sizes = union_variant_sizes(def);
            if sizes.len() > 1 && sizes.iter().any(|&s| s != sizes[0]) {
                let max = sizes.iter().copied().max().unwrap_or(0);
                if raw.len() <= max {
                    return Ok(raw.to_vec());
                }
                return Ok(raw[..max].to_vec());
            }
        }
        if let Some(expected_size) = subrecord_def.and_then(subrecord_fixed_size) {
            if raw.len() > expected_size {
                return Ok(raw[..expected_size].to_vec());
            }
            if raw.len() < expected_size {
                let mut padded = raw.to_vec();
                padded.resize(expected_size, 0);
                return Ok(padded);
            }
        }
        return Ok(raw.to_vec());
    }

    if matches!(entry.value, FieldValue::None) {
        if let Some(expected_size) = subrecord_def
            .and_then(subrecord_fixed_size)
            .or_else(|| codec.and_then(fixed_size_for_codec))
        {
            return Ok(vec![0; expected_size]);
        }
    }

    encode_value(codec, &entry.value, record_sig, subrec_sig_str, ctx)
}

fn scen_tnam_def_for_entry<'a>(
    record_def: &'a crate::schema::RecordDef,
    entry: &FieldEntry,
    seen_body_vnam: bool,
) -> Option<&'a crate::schema::SubrecordDef> {
    if record_def.id != "SCEN" || entry.sig.as_str() != "TNAM" {
        return None;
    }

    let is_template_scene = seen_body_vnam || matches!(entry.value, FieldValue::FormKey(_));
    record_def.subrecords.iter().find(|subrecord| {
        if subrecord.id != "TNAM" {
            return false;
        }
        if is_template_scene {
            subrecord.scope_id.is_none() && subrecord.codec.as_deref() == Some("formid")
        } else {
            subrecord.scope_id.as_deref() == Some("actions")
                && subrecord.codec.as_deref() == Some("float32")
        }
    })
}

fn perk_effect_data_def(
    record_def: &crate::schema::RecordDef,
) -> Option<&crate::schema::SubrecordDef> {
    record_def.subrecords.iter().find(|subrecord| {
        subrecord.id == "DATA" && subrecord.scope_id.as_deref() == Some("effects")
    })
}

fn term_snam_def_for_position(
    record_def: &crate::schema::RecordDef,
    after_marker_model: bool,
) -> Option<&crate::schema::SubrecordDef> {
    record_def.subrecords.iter().find(|subrecord| {
        if subrecord.id != "SNAM" {
            return false;
        }
        if after_marker_model {
            subrecord
                .codec
                .as_deref()
                .is_some_and(|codec| codec.starts_with("array_struct:"))
        } else {
            subrecord.codec.as_deref() == Some("formid")
        }
    })
}

/// Duplicate 4CC slots are disambiguated by payload shape, not occurrence.
/// Occurrence is unreliable when an earlier optional slot is absent.
fn duplicate_subrecord_def_for_entry<'a>(
    record_def: &'a crate::schema::RecordDef,
    entry: &FieldEntry,
) -> Option<&'a crate::schema::SubrecordDef> {
    let subrecord_id = entry.sig.as_str();
    let mut candidates = record_def
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.id == subrecord_id);
    let first = candidates.next()?;
    let FieldValue::Bytes(raw) = &entry.value else {
        return Some(first);
    };

    let mut accepted = std::iter::once(first)
        .chain(candidates)
        .filter(|subrecord| {
            subrecord
                .codec
                .as_deref()
                .and_then(|codec| codec_accepts_payload_length(codec, raw.len()))
                != Some(false)
        });
    let matching = accepted.next();
    if matching.is_some() && accepted.next().is_none() {
        matching
    } else {
        Some(first)
    }
}

fn is_null_term_looping_sound(
    record_def: &crate::schema::RecordDef,
    entry: &FieldEntry,
    ctx: &EncodeContext<'_>,
) -> bool {
    let Some(subrecord_def) = duplicate_subrecord_def_for_entry(record_def, entry) else {
        return false;
    };
    if subrecord_def.codec.as_deref() != Some("formid") {
        return false;
    }
    matches!(entry.value, FieldValue::None)
        || raw_formid_value_before_encode(&entry.value, ctx) == Some(0)
}

fn is_localized_lstring_subrecord(
    subrecord_def: Option<&crate::schema::SubrecordDef>,
    codec: Option<&str>,
) -> bool {
    codec == Some("lstring") || subrecord_def.is_some_and(|sd| sd.localized)
}

fn raw_lstring_id(raw: &[u8]) -> Option<u32> {
    if raw.len() != 4 {
        return None;
    }
    Some(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn fixed_size_for_codec(codec: &str) -> Option<usize> {
    match codec {
        "u8" | "i8" | "int8" | "uint8" => Some(1),
        "u16" | "i16" | "int16" | "uint16" => Some(2),
        "u32" | "i32" | "int32" | "uint32" | "f32" | "float" | "float32" | "formid" | "form_id" => {
            Some(4)
        }
        "u64" | "i64" | "int64" | "uint64" | "f64" | "double" => Some(8),
        "empty" => Some(0),
        "zstring" | "lstring" | "bytes" | "raw" | "string" | "lenstring16" | "omod_data"
        | "formid_array" => None,
        other => other
            .strip_prefix("struct:")
            .and_then(fixed_size_for_struct_codec),
    }
}

fn subrecord_fixed_size(subrecord_def: &crate::schema::SubrecordDef) -> Option<usize> {
    subrecord_def
        .codec
        .as_deref()
        .and_then(fixed_size_for_codec)
        .or_else(|| {
            subrecord_def
                .union_variants
                .iter()
                .rev()
                .filter_map(|variant| {
                    variant
                        .codec
                        .as_deref()
                        .or_else(|| (!variant.kind.is_empty()).then_some(variant.kind.as_str()))
                        .and_then(fixed_size_for_codec)
                })
                .next()
        })
}

/// Fixed byte size of each union variant that has a computable fixed codec
/// size. Variable-size variants (zstring/formid_array/etc.) are skipped, so the
/// returned vec may be shorter than `union_variants`. Returns an empty vec for
/// non-union subrecords. Used by `encode_field` to avoid truncating a raw union
/// payload that is already a legal variant width (see SNDR.BNAM).
fn union_variant_sizes(
    subrecord_def: &crate::schema::SubrecordDef,
) -> smallvec::SmallVec<[usize; 4]> {
    subrecord_def
        .union_variants
        .iter()
        .filter_map(|variant| {
            variant
                .codec
                .as_deref()
                .or_else(|| (!variant.kind.is_empty()).then_some(variant.kind.as_str()))
                .and_then(fixed_size_for_codec)
        })
        .collect()
}

fn raw_bytes_need_variable_tail_passthrough(
    record_def: Option<&crate::schema::RecordDef>,
    subrec_sig: &str,
) -> bool {
    if matches!(subrec_sig, "NVNM" | "VMAD") {
        return true;
    }

    let Some(subrecord_def) = record_def.and_then(|rd| rd.subrecord_def(subrec_sig)) else {
        return false;
    };
    let Some(codec) = subrecord_def.codec.as_deref() else {
        return false;
    };
    let Some(struct_field_count) = structured_codec_field_count(codec) else {
        return false;
    };

    subrecord_def.fields.len() != struct_field_count
}

fn structured_codec_field_count(codec: &str) -> Option<usize> {
    codec
        .strip_prefix("struct:")
        .or_else(|| codec.strip_prefix("array_struct:"))
        .map(|body| {
            body.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .count()
        })
}

fn fixed_size_for_struct_codec(codec: &str) -> Option<usize> {
    let mut total = 0usize;
    for part in codec
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        total += match part {
            "B" | "b" | "u8" | "i8" | "uint8" | "int8" => 1,
            "H" | "h" | "u16" | "i16" | "uint16" | "int16" => 2,
            "I" | "i" | "f" | "u32" | "i32" | "uint32" | "int32" | "float" | "float32"
            | "formid" | "form_id" => 4,
            "Q" | "q" | "d" | "u64" | "i64" | "uint64" | "int64" | "double" => 8,
            _ => return None,
        };
    }
    Some(total)
}

fn encode_value(
    codec: Option<&str>,
    value: &FieldValue,
    record_sig: Option<&str>,
    field_sig: &str,
    ctx: &mut EncodeContext<'_>,
) -> Result<Vec<u8>, WriteError> {
    match value {
        FieldValue::None if matches!(codec, Some("formid") | Some("formid_array")) => {
            Ok(0u32.to_le_bytes().to_vec())
        }

        FieldValue::None => Ok(Vec::new()),

        FieldValue::Bytes(raw) => Ok(raw.to_vec()),

        FieldValue::Bool(b) => Ok(vec![*b as u8]),

        FieldValue::Int(n) => {
            // Encode based on codec size hint, defaulting to int32.
            match codec {
                Some("int8") => Ok(vec![(*n as i8) as u8]),
                Some("int16") => Ok((*n as i16).to_le_bytes().to_vec()),
                Some("int64") => Ok(n.to_le_bytes().to_vec()),
                _ => Ok((*n as i32).to_le_bytes().to_vec()),
            }
        }

        FieldValue::Uint(n) => {
            if codec == Some("lstring") {
                let string_id = *n as u32;
                if ctx.target_is_localized {
                    let target_id =
                        localized_string_id_for_unresolved(ctx, record_sig, field_sig, string_id);
                    return Ok(target_id.to_le_bytes().to_vec());
                }
                return Ok(unresolved_lstring_placeholder(string_id).into_bytes());
            }
            match codec {
                Some("uint8") => Ok(vec![*n as u8]),
                Some("uint16") => Ok((*n as u16).to_le_bytes().to_vec()),
                Some("uint64") => Ok(n.to_le_bytes().to_vec()),
                // uint32 and default (lstring, formid, unknown)
                _ => Ok((*n as u32).to_le_bytes().to_vec()),
            }
        }

        FieldValue::Float(f) => Ok(f.to_le_bytes().to_vec()),

        FieldValue::String(sym) => {
            let s = ctx.interner.resolve(*sym).unwrap_or_default();
            if codec == Some("lstring") && ctx.target_is_localized {
                let string_id = localized_string_id_for_text(ctx, record_sig, field_sig, s);
                return Ok(string_id.to_le_bytes().to_vec());
            }
            let mut bytes = s.as_bytes().to_vec();
            bytes.push(0); // null terminator for zstring/lstring
            Ok(bytes)
        }

        FieldValue::FormKey(fk) => {
            // Resolve fk.plugin against the target plugin's master list. The
            // top byte of the on-disk FormID is the master index (0..N-1 for
            // imported masters, N for the own plugin).
            let object_id = fk.local & 0x00FF_FFFF;
            if object_id == 0 {
                return Ok(0u32.to_le_bytes().to_vec());
            }
            let plugin_name = ctx.interner.resolve(fk.plugin);
            let master_index = plugin_name
                .and_then(|name| ctx.master_map.get(&name.to_ascii_lowercase()).copied())
                .unwrap_or(ctx.own_index);
            // TEMP instrumentation: trace the master-byte the encode assigns
            // to the Gulper-family DLCCoast refs (0247C1 race, 0247C4
            // template) that collapse to 00 in the output, vs 04E28E which is
            // correct. Gated on MODBOX_TRACE_0247C1; remove once root-caused.
            if matches!(object_id, 0x0002_47C1 | 0x0002_47C4 | 0x0004_E28E)
                && std::env::var_os("MODBOX_TRACE_0247C1").is_some()
            {
                eprintln!(
                    "[trace_0247c1] ENCODE object_id={object_id:06X} fk.plugin={:?} \
                     master_index={master_index} own_index={}",
                    plugin_name, ctx.own_index
                );
            }
            let raw = (master_index << 24) | object_id;
            Ok(raw.to_le_bytes().to_vec())
        }

        FieldValue::List(items) => {
            // Encode each item and concatenate. This is a best-effort
            // passthrough; proper list encoding is game-specific and handled
            // by the full authoring pipeline.
            let mut out = Vec::new();
            for item in items {
                out.extend(encode_value(codec, item, record_sig, field_sig, ctx)?);
            }
            Ok(out)
        }

        FieldValue::Struct(pairs) => {
            // Encode struct fields in order as concatenated bytes.
            let mut out = Vec::new();
            for (_, val) in pairs {
                out.extend(encode_value(None, val, record_sig, field_sig, ctx)?);
            }
            Ok(out)
        }
    }
}

fn localized_string_id_for_unresolved(
    ctx: &mut EncodeContext<'_>,
    record_sig: Option<&str>,
    field_sig: &str,
    source_id: u32,
) -> u32 {
    let table_type = localized_table_type_for_signature(record_sig, field_sig);
    let placeholder = unresolved_lstring_placeholder_text(source_id);
    if let Some(slot) = ctx.slot.as_deref_mut() {
        if localized_id_exists_for_table(slot.strings_ref(), source_id, table_type) {
            return source_id;
        }
        if let Some(string_id) = slot.localized_string_id_for_text(&placeholder, table_type) {
            return string_id;
        }
        return slot.allocate_localized_string_for_text(&placeholder, table_type);
    }
    let Some(strings) = ctx.localized_strings.as_deref_mut() else {
        return source_id;
    };
    if localized_id_exists_for_table(strings, source_id, table_type) {
        return source_id;
    }
    if let Some(string_id) = find_localized_string_id(strings, &placeholder, table_type) {
        return string_id;
    }
    let string_id = next_available_localized_string_id(strings, 1);
    let language = default_language_for_strings(strings);
    strings
        .by_language
        .entry(language)
        .or_default()
        .insert(string_id, placeholder);
    if let Some(table_type) = table_type {
        strings
            .table_types
            .insert(string_id, table_type.to_string());
    }
    string_id
}

fn localized_id_exists_for_table(
    strings: &LocalizedStringsState,
    string_id: u32,
    table_type: Option<&str>,
) -> bool {
    let exists = strings
        .by_language
        .values()
        .any(|table| table.contains_key(&string_id));
    if !exists {
        return false;
    }
    match table_type {
        Some("strings") => strings
            .table_types
            .get(&string_id)
            .is_none_or(|actual_type| actual_type == "strings"),
        Some(expected_type) => strings
            .table_types
            .get(&string_id)
            .is_some_and(|actual_type| actual_type == expected_type),
        None => true,
    }
}

fn localized_string_id_for_text(
    ctx: &mut EncodeContext<'_>,
    record_sig: Option<&str>,
    field_sig: &str,
    text: &str,
) -> u32 {
    let table_type = localized_table_type_for_signature(record_sig, field_sig);
    if let Some(slot) = ctx.slot.as_deref_mut() {
        if let Some(string_id) = slot.localized_string_id_for_text(text, table_type) {
            return string_id;
        }
        return slot.allocate_localized_string_for_text(text, table_type);
    }
    let Some(strings) = ctx.localized_strings.as_deref_mut() else {
        return 0;
    };
    if let Some(string_id) = find_localized_string_id(strings, text, table_type) {
        return string_id;
    }

    let string_id = next_available_localized_string_id(strings, 1);
    let language = default_language_for_strings(strings);
    strings
        .by_language
        .entry(language)
        .or_default()
        .insert(string_id, text.to_string());
    if let Some(table_type) = table_type {
        strings
            .table_types
            .insert(string_id, table_type.to_string());
    }
    string_id
}

fn find_localized_string_id(
    strings: &LocalizedStringsState,
    text: &str,
    table_type: Option<&str>,
) -> Option<u32> {
    let mut languages = Vec::new();
    let default_language = strings.default_language.trim();
    if !default_language.is_empty() {
        languages.push(default_language.to_string());
    }
    if default_language != "en" {
        languages.push("en".to_string());
    }
    let mut rest: Vec<&String> = strings.by_language.keys().collect();
    rest.sort();
    for language in rest {
        if !languages.iter().any(|existing| existing == language) {
            languages.push(language.clone());
        }
    }

    for language in languages {
        let Some(table) = strings.by_language.get(language.as_str()) else {
            continue;
        };
        let mut ids: Vec<u32> = table
            .iter()
            .filter_map(|(string_id, value)| {
                if value != text {
                    return None;
                }
                if let Some(expected_type) = table_type {
                    if strings
                        .table_types
                        .get(string_id)
                        .is_some_and(|actual_type| actual_type != expected_type)
                    {
                        return None;
                    }
                }
                Some(*string_id)
            })
            .collect();
        ids.sort_unstable();
        if let Some(id) = ids.first() {
            return Some(*id);
        }
    }
    None
}

fn next_available_localized_string_id(
    strings: &LocalizedStringsState,
    preferred_start: u32,
) -> u32 {
    let mut used: std::collections::HashSet<u32> = strings.table_types.keys().copied().collect();
    for table in strings.by_language.values() {
        used.extend(table.keys().copied());
    }
    let mut candidate = preferred_start;
    while used.contains(&candidate) {
        candidate = candidate.saturating_add(1);
        if candidate == u32::MAX {
            return candidate;
        }
    }
    candidate
}

fn default_language_for_strings(strings: &mut LocalizedStringsState) -> String {
    let language = strings.default_language.trim();
    if !language.is_empty() {
        return language.to_string();
    }
    strings.default_language = "en".to_string();
    "en".to_string()
}

fn localized_table_type_for_signature(
    record_sig: Option<&str>,
    signature: &str,
) -> Option<&'static str> {
    // Mirrors esp::io::table_type_for_localized_signature — keep in lockstep.
    // LSCR.DESC is plain STRINGS in FO4 (not DLSTRINGS like BOOK/SPEL/PERK); RNAM
    // (INFO Prompt / FLOR Activate Text) is plain UI text, never voiced → STRINGS.
    // BOOK.CNAM and QUST.CNAM read from DLSTRINGS despite CNAM defaulting to
    // STRINGS.
    match (record_sig, signature) {
        (Some("TERM"), "ITXT" | "RNAM") | (Some("MESG"), "DESC" | "ITXT") => Some("strings"),
        (Some("LSCR"), "DESC") => Some("strings"),
        (Some("BOOK"), "CNAM") => Some("dlstrings"),
        (Some("QUST"), "CNAM") => Some("dlstrings"),
        (_, signature) => match signature {
            "DESC" | "ITXT" => Some("dlstrings"),
            "NAM1" => Some("ilstrings"),
            "RNAM" => Some("strings"),
            _ => Some("strings"),
        },
    }
}

fn unresolved_lstring_placeholder_text(string_id: u32) -> String {
    format!("LOC_{string_id:08X}")
}

fn unresolved_lstring_placeholder(string_id: u32) -> String {
    format!("{}\0", unresolved_lstring_placeholder_text(string_id))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use crate::test_fixtures;
    use esp_authoring_core::plugin_runtime::{plugin_handle_new_native, plugin_handle_store_ref};

    // Helper: load fo4 schema
    fn fo4_schema() -> std::sync::Arc<AuthoringSchema> {
        AuthoringSchema::for_game("fo4").expect("fo4 schema")
    }

    // Helper: empty master map / zero own_index for non-FK encode tests.
    fn no_masters() -> HashMap<String, u32> {
        HashMap::new()
    }

    fn fo4_master_map() -> HashMap<String, u32> {
        let mut master_map = HashMap::new();
        for (i, master) in [
            "Fallout4.esm",
            "DLCRobot.esm",
            "DLCworkshop01.esm",
            "DLCCoast.esm",
            "DLCworkshop02.esm",
            "DLCworkshop03.esm",
            "DLCNukaWorld.esm",
        ]
        .iter()
        .enumerate()
        {
            master_map.insert(master.to_ascii_lowercase(), i as u32);
        }
        master_map
    }

    fn tpta_bytes(slots: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for slot in slots {
            bytes.extend_from_slice(&slot.to_le_bytes());
        }
        bytes
    }

    fn tpta_slots_from_record(record: &ParsedRecord) -> Vec<u32> {
        let tpta = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "TPTA")
            .expect("TPTA subrecord");
        tpta.data
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    fn scen_test_field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn encode_scen_for_test(
        interner: &StringInterner,
        fields: Vec<FieldEntry>,
        own_index: u32,
    ) -> ParsedRecord {
        let schema = fo4_schema();
        let mut record = Record::new(
            SigCode::from_str("SCEN").unwrap(),
            FormKey::parse("000800@Test.esm", interner).unwrap(),
        );
        record.fields.extend(fields);
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner,
            master_map: &master_map,
            own_index,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };
        encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("SCEN should be encoded")
    }

    fn scen_tnam_payloads(record: &ParsedRecord) -> Vec<Vec<u8>> {
        record
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "TNAM")
            .map(|subrecord| subrecord.data.to_vec())
            .collect()
    }

    #[test]
    fn final_encode_redirects_npc_struct_tpta_self_slots_to_default_template() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let master_map = fo4_master_map();
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse("03D628@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPLT").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x0003D628_u32.to_le_bytes().to_vec())),
        });
        let self_fk = FormKey::parse("03D628@SeventySix.esm", &interner).unwrap();
        let tpta_names = [
            "traits",
            "stats",
            "factions",
            "spells",
            "ai_data",
            "ai_packages",
            "model_animation",
            "base_data",
            "inventory",
            "script",
            "def_package_list",
            "attack_data",
            "keywords",
        ];
        let mut fields = Vec::new();
        for (index, name) in tpta_names.iter().enumerate() {
            let value = match index {
                0 => FieldValue::FormKey(self_fk),
                1 => FieldValue::Uint(0x0703D628),
                2 => FieldValue::Uint(0x0003D628),
                _ => FieldValue::Uint(0),
            };
            fields.push((interner.intern(name), value));
        }
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPTA").unwrap(),
            value: FieldValue::Struct(fields),
        });
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 7,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("record kept");

        let slots = tpta_slots_from_record(&parsed);
        assert_eq!(slots[0], 0x0003D628);
        assert_eq!(slots[1], 0x0003D628);
        assert_eq!(slots[2], 0x0003D628);
        assert!(!slots.contains(&0x0703D628));
    }

    #[test]
    fn final_encode_redirects_npc_byte_tpta_self_slots_to_default_template() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let master_map = fo4_master_map();
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse("03D628@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPLT").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x0003D628_u32.to_le_bytes().to_vec())),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPTA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(tpta_bytes(&[
                0x0703D628, 0x0703D628, 0x0003D628, 0, 0, 0x0703D628, 0, 0, 0, 0, 0, 0, 0,
            ]))),
        });
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 7,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("record kept");

        let slots = tpta_slots_from_record(&parsed);
        assert_eq!(slots[0], 0x0003D628);
        assert_eq!(slots[1], 0x0003D628);
        assert_eq!(slots[2], 0x0003D628);
        assert_eq!(slots[5], 0x0003D628);
        assert!(!slots.contains(&0x0703D628));
    }

    // A clean FO76 LCEP is N×12 bytes (I,I,B,B,B,B). decode_subrecord ->
    // List<Struct>, then encode_field must reproduce N×12 bytes (FKs
    // remapped to 07, flags & row count preserved); this asserts byte-exact
    // round-trip length + content.
    #[test]
    fn lcep_round_trips_multi_row_preserving_flags_and_count() {
        use crate::source_read::decode_subrecord;
        let interner = StringInterner::new();
        // Real source 7F1E8D LCEP: 4 rows, 48 bytes (00-prefix source FKs).
        let src_hex = "862e8500852e850000000000\
                       842e85008cdb850001000000\
                       872e8500842e850001000000\
                       852e85008cdb850001000000";
        let data: Vec<u8> = (0..src_hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&src_hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(data.len(), 48);

        let decoded = decode_subrecord(
            "LCTN",
            "LCEP",
            "array_struct:I,I,B,B,B,B",
            &data,
            &[],
            "SeventySix.esm",
            None,
            &interner,
        )
        .unwrap();
        let FieldValue::List(rows) = &decoded else {
            panic!("LCEP must decode to List, got {decoded:?}");
        };
        assert_eq!(rows.len(), 4, "4 source rows");

        // Encode with SeventySix.esm as the OWN plugin (own_index 7) so the
        // source-local FKs serialize to 07-prefix.
        let mut master_map = HashMap::new();
        for (i, m) in [
            "Fallout4.esm",
            "DLCRobot.esm",
            "DLCworkshop01.esm",
            "DLCCoast.esm",
            "DLCworkshop02.esm",
            "DLCworkshop03.esm",
            "DLCNukaWorld.esm",
        ]
        .iter()
        .enumerate()
        {
            master_map.insert(m.to_ascii_lowercase(), i as u32);
        }
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");

        // FULL pipeline: decode -> NORMALIZE -> encode. Normalize must run
        // here because a bare encode of the decoded value does not exercise
        // the tail-field handling that production goes through.
        let mut record = Record::new(
            crate::ids::SigCode::from_str("LCTN").unwrap(),
            crate::ids::FormKey::parse("7F1E8D@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: crate::ids::SubrecordSig::from_str("LCEP").unwrap(),
            value: decoded,
        });
        let normalizer = crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
            &schema, &interner,
        );
        let normalized = match normalizer.normalize(record) {
            crate::target_normalize::TargetRecordNormalization::Keep(r) => r,
            _ => panic!("LCTN must be kept"),
        };
        let lcep_entry = normalized
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "LCEP")
            .expect("LCEP survives normalize");

        let out = encode_field_for_test(
            lcep_entry,
            schema.record_def("LCTN"),
            &interner,
            &master_map,
            7,
            false,
        )
        .unwrap();

        assert_eq!(
            out.len(),
            48,
            "LCEP must survive decode->normalize->encode at 4×12=48 bytes, got {} \
             (the #16b flag-byte-drop / row-collapse bug)",
            out.len()
        );
        // Row 0: ref 00852E86 -> 07852E86, par 00852E85 -> 07852E85, flags 00000000.
        assert_eq!(&out[0..4], &0x0785_2E86u32.to_le_bytes(), "row0 ref -> 07");
        assert_eq!(&out[4..8], &0x0785_2E85u32.to_le_bytes(), "row0 par -> 07");
        assert_eq!(&out[8..12], &[0, 0, 0, 0], "row0 flags+unknown preserved");
        // Row 1 flags were 01000000 — must survive at the row-1 tail (offset 20).
        assert_eq!(&out[20..24], &[1, 0, 0, 0], "row1 flags+unknown preserved");
        // Row 2 ref 00852E87 -> 07852E87 at offset 24 (proves rows didn't collapse).
        assert_eq!(
            &out[24..28],
            &0x0785_2E87u32.to_le_bytes(),
            "row2 ref -> 07"
        );
    }

    // A NON-LCEP array_struct (LCUN, I,I,I) that shares the same
    // target_normalize + generic-encode path must round-trip
    // BYTE-IDENTICAL (modulo the source-local 00 -> own-index FK remap),
    // proving the shared normalize/encode path is byte-stable for other
    // struct types.
    #[test]
    fn lcun_round_trips_byte_identical_other_array_struct() {
        use crate::source_read::decode_subrecord;
        let interner = StringInterner::new();
        // Two LCUN rows (I,I,I): NPC, ActorRef, Location. Source-local ids
        // (00-prefix); a Location of 0 (NULL) stays 0.
        let mut data = Vec::new();
        for (npc, actor, loc) in [
            (0x0018_928Eu32, 0x0018_9250u32, 0u32),
            (0x0018_5C12u32, 0u32, 0x0018_5C99u32),
        ] {
            data.extend_from_slice(&npc.to_le_bytes());
            data.extend_from_slice(&actor.to_le_bytes());
            data.extend_from_slice(&loc.to_le_bytes());
        }
        assert_eq!(data.len(), 24);

        let decoded = decode_subrecord(
            "LCTN",
            "LCUN",
            "array_struct:I,I,I",
            &data,
            &[],
            "SeventySix.esm",
            None,
            &interner,
        )
        .unwrap();
        let mut record = Record::new(
            crate::ids::SigCode::from_str("LCTN").unwrap(),
            crate::ids::FormKey::parse("123456@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: crate::ids::SubrecordSig::from_str("LCUN").unwrap(),
            value: decoded,
        });
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let normalizer = crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
            &schema, &interner,
        );
        let normalized = match normalizer.normalize(record) {
            crate::target_normalize::TargetRecordNormalization::Keep(r) => r,
            _ => panic!("LCTN must be kept"),
        };
        let lcun_entry = normalized
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "LCUN")
            .expect("LCUN survives normalize");
        // own_index 0 (SeventySix.esm not in master_map) so 00-prefix FKs stay
        // byte-identical — pure round-trip of the normalize/encode path.
        let out = encode_field_for_test(
            lcun_entry,
            schema.record_def("LCTN"),
            &interner,
            &no_masters(),
            0,
            false,
        )
        .unwrap();
        assert_eq!(
            out, data,
            "LCUN must round-trip byte-identical (no flag/row loss)"
        );
    }

    #[test]
    fn offset_record_nvnm_geometry_shifts_verts_grid_waypoints_not_parent() {
        use esp_authoring_core::nvnm::{NvnmGrid, NvnmGridCell, NvnmWaypoint};
        let payload = NvnmPayload {
            version: 15,
            flags: 0,
            parent: NvnmParent::Exterior {
                world: 0x0125_DA15,
                grid_x: -1,
                grid_y: -1,
            },
            vertices: vec![
                NvnmVertex {
                    x: -4096.0,
                    y: -4096.0,
                    z: 18000.0,
                },
                NvnmVertex {
                    x: -2000.0,
                    y: -3000.0,
                    z: 18100.0,
                },
                NvnmVertex {
                    x: -1000.0,
                    y: -1000.0,
                    z: 18050.0,
                },
            ],
            triangles: vec![NvnmTriangle {
                vertices: [0, 1, 2],
                links: [-1; 3],
                cover_marker: [0; 9],
                flags: 0,
            }],
            edge_links: vec![],
            door_refs: vec![],
            cover_array: vec![],
            cover_triangle_mappings: vec![],
            waypoints: vec![NvnmWaypoint {
                x: -1500.0,
                y: -1500.0,
                z: 18075.0,
                triangle: 0,
                flags: 0,
            }],
            grid: NvnmGrid {
                divisor: 2,
                grid_size_x: 2048.0,
                grid_size_y: 2048.0,
                bounds_min_x: -4096.0,
                bounds_min_y: -4096.0,
                bounds_min_z: 18000.0,
                bounds_max_x: -1000.0,
                bounds_max_y: -1000.0,
                bounds_max_z: 18100.0,
                cells: vec![NvnmGridCell::default(); 4],
            },
        };

        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("NAVM").unwrap(),
            FormKey::parse("000900@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NVNM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(write_nvnm(&payload))),
        });

        let shifted =
            offset_record_nvnm_geometry(&mut record, [2048.0, 2048.0, 0.0]).expect("offset NVNM");
        assert_eq!(shifted, 1);

        let FieldValue::Bytes(out) = &record.fields[0].value else {
            panic!("NVNM field missing");
        };
        let got = parse_nvnm(out.as_slice()).expect("parse shifted NVNM");

        // XY shifted, Z untouched (offset z = 0).
        assert_eq!(got.vertices[0].x, -2048.0);
        assert_eq!(got.vertices[0].y, -2048.0);
        assert_eq!(got.vertices[0].z, 18000.0);
        assert_eq!(got.vertices[2].x, 1048.0);
        // Grid bounds shifted in XY, Z bounds untouched.
        assert_eq!(got.grid.bounds_min_x, -2048.0);
        assert_eq!(got.grid.bounds_max_y, 1048.0);
        assert_eq!(got.grid.bounds_min_z, 18000.0);
        // Waypoint shifted in XY.
        assert_eq!(got.waypoints[0].x, 548.0);
        assert_eq!(got.waypoints[0].y, 548.0);
        // Parent cell index preserved.
        assert_eq!(
            got.parent,
            NvnmParent::Exterior {
                world: 0x0125_DA15,
                grid_x: -1,
                grid_y: -1
            }
        );
        // Topology untouched.
        assert_eq!(got.triangles[0].vertices, [0, 1, 2]);
    }

    fn encode_field_for_test(
        entry: &FieldEntry,
        record_def: Option<&crate::schema::RecordDef>,
        interner: &StringInterner,
        master_map: &HashMap<String, u32>,
        own_index: u32,
        target_is_localized: bool,
    ) -> Result<Vec<u8>, WriteError> {
        let mut strings = LocalizedStringsState {
            default_language: "en".to_string(),
            ..LocalizedStringsState::default()
        };
        let localized_strings = target_is_localized.then_some(&mut strings);
        let mut ctx = EncodeContext {
            interner,
            master_map,
            own_index,
            target_is_localized,
            slot: None,
            localized_strings,
        };
        encode_field(entry, record_def, &mut ctx)
    }

    fn localized_strings_for_test() -> LocalizedStringsState {
        LocalizedStringsState {
            default_language: "en".to_string(),
            ..LocalizedStringsState::default()
        }
    }

    fn encode_localized_field_with_strings(
        entry: &FieldEntry,
        record_def: Option<&crate::schema::RecordDef>,
        interner: &StringInterner,
        strings: &mut LocalizedStringsState,
    ) -> Vec<u8> {
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: true,
            slot: None,
            localized_strings: Some(strings),
        };
        encode_field(entry, record_def, &mut ctx).unwrap()
    }

    fn assert_localized_entry(
        strings: &LocalizedStringsState,
        string_id: u32,
        expected_text: &str,
        expected_table_type: &str,
    ) {
        assert_eq!(
            strings
                .by_language
                .get("en")
                .and_then(|table| table.get(&string_id))
                .map(String::as_str),
            Some(expected_text)
        );
        assert_eq!(
            strings.table_types.get(&string_id).map(String::as_str),
            Some(expected_table_type)
        );
    }

    #[test]
    fn encode_bool_true_and_false() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let edid_sig = crate::ids::SubrecordSig::from_str("EDID").unwrap();
        let mm = no_masters();

        let entry_t = FieldEntry {
            sig: edid_sig,
            value: FieldValue::Bool(true),
        };
        let bytes_t =
            encode_field_for_test(&entry_t, record_def, &interner, &mm, 0, false).unwrap();
        assert_eq!(bytes_t, vec![1u8]);

        let entry_f = FieldEntry {
            sig: edid_sig,
            value: FieldValue::Bool(false),
        };
        let bytes_f =
            encode_field_for_test(&entry_f, record_def, &interner, &mm, 0, false).unwrap();
        assert_eq!(bytes_f, vec![0u8]);
    }

    #[test]
    fn encode_perk_keeps_effect_conditions_scoped_and_entry_data_three_bytes() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let mut record = Record::new(
            SigCode::from_str("PERK").unwrap(),
            FormKey::parse("000800@Test.esm", &interner).unwrap(),
        );
        let field = |sig: &str, bytes: Vec<u8>| FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        };
        let mut top_condition = vec![0_u8; 32];
        top_condition[8..10].copy_from_slice(&72_u16.to_le_bytes());
        let mut effect_condition = vec![0_u8; 32];
        effect_condition[8..10].copy_from_slice(&494_u16.to_le_bytes());
        record.fields.extend([
            field("EDID", b"ScopedPerk\0".to_vec()),
            field("CTDA", top_condition),
            field("DATA", vec![0, 0, 1, 1, 0]),
            field("PRKE", vec![2, 0, 0]),
            field("DATA", vec![35, 3, 3]),
            field("PRKC", vec![1]),
            field("CTDA", effect_condition),
            field("EPFT", vec![1]),
            field("EPFD", 1.2_f32.to_le_bytes().to_vec()),
            FieldEntry {
                sig: SubrecordSig::from_str("PRKF").unwrap(),
                value: FieldValue::None,
            },
        ]);
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("PERK should encode");

        assert_eq!(
            parsed
                .subrecords
                .iter()
                .map(|subrecord| subrecord.signature.as_str())
                .collect::<Vec<_>>(),
            vec![
                "EDID", "CTDA", "DATA", "PRKE", "DATA", "PRKC", "CTDA", "EPFT", "EPFD", "PRKF",
            ]
        );
        let effect_data = parsed
            .subrecords
            .iter()
            .skip_while(|subrecord| subrecord.signature.as_str() != "PRKE")
            .find(|subrecord| subrecord.signature.as_str() == "DATA")
            .expect("entry-point DATA");
        assert_eq!(effect_data.data.as_ref(), &[35, 3, 3]);
    }

    #[test]
    fn encode_term_snam_uses_payload_length_to_select_schema() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("TERM");
        let looping_sound = vec![0x34, 0x12, 0, 0];
        let marker_parameters =
            hex::decode("0000803F00006CC20000803F0000000000000000FF010000").unwrap();
        let looping_sound_entry = FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(looping_sound.clone())),
        };
        let marker_parameters_entry = FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(marker_parameters.clone())),
        };

        let encoded_looping_sound = encode_field_for_test(
            &looping_sound_entry,
            record_def,
            &interner,
            &no_masters(),
            0,
            false,
        )
        .unwrap();
        let encoded_marker_parameters = encode_field_for_test(
            &marker_parameters_entry,
            record_def,
            &interner,
            &no_masters(),
            0,
            false,
        )
        .unwrap();

        assert_eq!(encoded_looping_sound, looping_sound);
        assert_eq!(encoded_marker_parameters, marker_parameters);
    }

    #[test]
    fn fo76_term_znam_survives_source_decode_and_target_write() {
        let marker_parameters =
            hex::decode("0000803F00006CC20000803F0000000000000000FF010000").unwrap();
        let raw_source = ParsedRecord {
            signature: SmolStr::new_static("TERM"),
            form_id: 0x0072_6E6C,
            flags: 0,
            version_control: 0,
            form_version: Some(208),
            version2: Some(1),
            subrecords: vec![
                ParsedSubrecord {
                    signature: SmolStr::new_static("EDID"),
                    data: Bytes::from_static(b"Storm_UpperAtrium_ClinicTerminal\0"),
                    semantic_type: None,
                },
                ParsedSubrecord {
                    signature: SmolStr::new_static("XMRK"),
                    data: Bytes::from_static(b"Markers\\MarkerDeskTerminal3rdP.nif\0"),
                    semantic_type: None,
                },
                ParsedSubrecord {
                    signature: SmolStr::new_static("ZNAM"),
                    data: Bytes::from(marker_parameters.clone()),
                    semantic_type: None,
                },
            ],
            raw_payload: None,
            parse_error: None,
        };
        let source_schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let interner = StringInterner::new();
        let source_form_key = FormKey::parse("726E6C@SeventySix.esm", &interner).unwrap();
        let record = crate::source_read::decode_record_from_parsed(
            &raw_source,
            &source_form_key,
            &source_schema,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
        )
        .unwrap();
        let master_map = fo4_master_map();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 7,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &fo4_schema(), &mut ctx, Some("fo4"))
            .unwrap()
            .expect("TERM should encode");
        let snam = parsed
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "SNAM")
            .collect::<Vec<_>>();
        assert_eq!(snam.len(), 1);
        assert_eq!(snam[0].data.as_ref(), marker_parameters.as_slice());
    }

    #[test]
    fn encode_race_bsms_uses_payload_length_to_select_scale_or_range_schema() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("RACE");
        let scale = vec![0x11; 36];
        let range = vec![0x22; 16];

        for expected in [&scale, &range] {
            let entry = FieldEntry {
                sig: SubrecordSig::from_str("BSMS").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(expected.clone())),
            };
            let encoded =
                encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false)
                    .unwrap();
            assert_eq!(&encoded, expected);
        }
    }

    #[test]
    fn encode_race_preserves_scale_and_range_rows_without_padding() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let mut record = Record::new(
            SigCode::from_str("RACE").unwrap(),
            FormKey::parse("7AC578@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig::from_str("EDID").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(b"FishermanRace\0")),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("NAM0").unwrap(),
                value: FieldValue::None,
            },
            FieldEntry {
                sig: SubrecordSig::from_str("BSMP").unwrap(),
                value: FieldValue::Uint(0),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("BSMB").unwrap(),
                value: FieldValue::String(interner.intern("MaleScale")),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("BSMS").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(vec![0x11; 36])),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("BMMP").unwrap(),
                value: FieldValue::Uint(0),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("BSMB").unwrap(),
                value: FieldValue::String(interner.intern("MaleRange")),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("BSMS").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(vec![0x22; 16])),
            },
        ]);
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("RACE should encode");
        let lengths = parsed
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "BSMS")
            .map(|subrecord| subrecord.data.len())
            .collect::<Vec<_>>();
        assert_eq!(lengths, vec![36, 16]);
    }

    #[test]
    fn encode_term_omits_null_looping_sound_and_keeps_marker_parameters() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let marker_parameters =
            hex::decode("0000803F00006CC20000803F0000000000000000FF010000").unwrap();
        let mut record = Record::new(
            SigCode::from_str("TERM").unwrap(),
            FormKey::parse("85D9AA@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig::from_str("SNAM").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(vec![0; 4])),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("FNAM").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(vec![0; 2])),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("XMRK").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(
                    b"Markers\\MarkerWallTerminal3rdP.nif\0".to_vec(),
                )),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("SNAM").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(marker_parameters.clone())),
            },
        ]);
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("TERM should be encoded");
        let snam = parsed
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "SNAM")
            .collect::<Vec<_>>();

        assert_eq!(snam.len(), 1);
        assert_eq!(snam[0].data.as_ref(), marker_parameters.as_slice());
    }

    #[test]
    fn encode_uint32_round_trip() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("EDID").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Uint(42),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();
        let back = u32::from_le_bytes(bytes.try_into().unwrap());
        assert_eq!(back, 42);
    }

    #[test]
    fn encode_formid_array_preserves_null_slots() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let own_plugin = interner.intern("MyMod.esp");
        let mut master_map = HashMap::new();
        master_map.insert("fallout4.esm".to_string(), 0);
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 1,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };
        let value = FieldValue::List(vec![
            FieldValue::FormKey(FormKey {
                local: 0x12_3456,
                plugin: fallout4,
            }),
            FieldValue::None,
            FieldValue::FormKey(FormKey {
                local: 0x00_0800,
                plugin: own_plugin,
            }),
        ]);

        let bytes = encode_value(Some("formid_array"), &value, None, "SNAM", &mut ctx).unwrap();

        assert_eq!(bytes.len(), 12);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x0012_3456
        );
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            0x0100_0800
        );
    }

    #[test]
    fn encode_unresolved_lstring_id_as_placeholder_for_unlocalized_target() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("FULL").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Uint(0x6102_9A71),
        };

        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();

        assert_eq!(bytes, b"LOC_61029A71\0");
    }

    #[test]
    fn encode_unresolved_lstring_id_allocates_placeholder_for_localized_target() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("FULL").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Uint(0x6102_9A71),
        };
        let mut strings = localized_strings_for_test();
        let bytes =
            encode_localized_field_with_strings(&entry, record_def, &interner, &mut strings);

        let string_id = u32::from_le_bytes(bytes.try_into().unwrap());
        assert_ne!(string_id, 0x6102_9A71);
        assert_localized_entry(&strings, string_id, "LOC_61029A71", "strings");
    }

    #[test]
    fn encode_existing_lstring_id_preserves_matching_target_table_id() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("FULL").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Uint(0x1234),
        };
        let mut strings = localized_strings_for_test();
        strings
            .by_language
            .entry("en".to_string())
            .or_default()
            .insert(0x1234, "Seeded Name".to_string());
        strings.table_types.insert(0x1234, "strings".to_string());
        let bytes =
            encode_localized_field_with_strings(&entry, record_def, &interner, &mut strings);

        assert_eq!(bytes, 0x1234_u32.to_le_bytes());
    }

    #[test]
    fn encode_raw_lstring_id_allocates_placeholder_for_localized_target() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("FULL").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(SmallVec::from_slice(&0x6100_B7F4_u32.to_le_bytes())),
        };
        let mut strings = localized_strings_for_test();
        let bytes =
            encode_localized_field_with_strings(&entry, record_def, &interner, &mut strings);

        let string_id = u32::from_le_bytes(bytes.try_into().unwrap());
        assert_ne!(string_id, 0x6100_B7F4);
        assert_localized_entry(&strings, string_id, "LOC_6100B7F4", "strings");
    }

    #[test]
    fn encode_raw_dlstring_id_allocates_dlstrings_placeholder() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("PERK");
        let sig = SubrecordSig::from_str("DESC").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(SmallVec::from_slice(&0x8100_1CF0_u32.to_le_bytes())),
        };
        let mut strings = localized_strings_for_test();
        let bytes =
            encode_localized_field_with_strings(&entry, record_def, &interner, &mut strings);

        let string_id = u32::from_le_bytes(bytes.try_into().unwrap());
        assert_ne!(string_id, 0x8100_1CF0);
        assert_localized_entry(&strings, string_id, "LOC_81001CF0", "dlstrings");
    }

    #[test]
    fn encode_terminal_and_message_lstrings_use_persistent_strings() {
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let cases = [
            ("TERM", "RNAM", "ilstrings"),
            ("TERM", "ITXT", "dlstrings"),
            ("MESG", "DESC", "dlstrings"),
            ("MESG", "ITXT", "dlstrings"),
        ];

        for (record_sig, field_sig, seeded_type) in cases {
            let record_def = schema.record_def(record_sig);
            let text = format!("{record_sig}.{field_sig} text");
            let sym = interner.intern(&text);
            let entry = FieldEntry {
                sig: SubrecordSig::from_str(field_sig).unwrap(),
                value: FieldValue::String(sym),
            };
            let mut strings = localized_strings_for_test();
            let seeded_id = 0x6100_EDB2;
            strings
                .by_language
                .entry("en".to_string())
                .or_default()
                .insert(seeded_id, text.clone());
            strings
                .table_types
                .insert(seeded_id, seeded_type.to_string());

            let bytes =
                encode_localized_field_with_strings(&entry, record_def, &interner, &mut strings);
            let string_id = u32::from_le_bytes(bytes.try_into().unwrap());

            assert_ne!(string_id, seeded_id);
            assert_localized_entry(&strings, string_id, &text, "strings");
        }
    }

    #[test]
    fn encode_info_rnam_and_lscr_desc_text_use_strings_table() {
        // INFO.RNAM (Prompt) and LSCR.DESC are plain STRINGS in FO4, not the
        // ILSTRINGS/DLSTRINGS that FO76 files them under. A freshly encoded text
        // value must allocate into the strings table so xEdit resolves it.
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let cases = [("INFO", "RNAM"), ("LSCR", "DESC")];

        for (record_sig, field_sig) in cases {
            let record_def = schema.record_def(record_sig);
            let text = format!("{record_sig}.{field_sig} text");
            let sym = interner.intern(&text);
            let entry = FieldEntry {
                sig: SubrecordSig::from_str(field_sig).unwrap(),
                value: FieldValue::String(sym),
            };
            let mut strings = localized_strings_for_test();
            let bytes =
                encode_localized_field_with_strings(&entry, record_def, &interner, &mut strings);
            let string_id = u32::from_le_bytes(bytes.try_into().unwrap());
            assert_localized_entry(&strings, string_id, &text, "strings");
        }
    }

    #[test]
    fn encode_book_cnam_text_uses_dlstrings_table() {
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("BOOK");
        let sym = interner.intern("Test description");
        let entry = FieldEntry {
            sig: SubrecordSig::from_str("CNAM").unwrap(),
            value: FieldValue::String(sym),
        };
        let mut strings = localized_strings_for_test();

        let bytes =
            encode_localized_field_with_strings(&entry, record_def, &interner, &mut strings);

        let string_id = u32::from_le_bytes(bytes.try_into().unwrap());
        assert_localized_entry(&strings, string_id, "Test description", "dlstrings");
    }

    #[test]
    fn encode_localized_lstring_text_reuses_seeded_string_id() {
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("FULL").unwrap();
        let sym = interner.intern("Resolved Name");
        let entry = FieldEntry {
            sig,
            value: FieldValue::String(sym),
        };
        let master_map = no_masters();
        let mut strings = LocalizedStringsState {
            default_language: "en".to_string(),
            ..LocalizedStringsState::default()
        };
        strings
            .by_language
            .entry("en".to_string())
            .or_default()
            .insert(0x1234, "Resolved Name".to_string());
        strings
            .by_language
            .entry("fr".to_string())
            .or_default()
            .insert(0x1234, "Nom resolu".to_string());
        strings.table_types.insert(0x1234, "strings".to_string());
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: true,
            slot: None,
            localized_strings: Some(&mut strings),
        };

        let bytes = encode_field(&entry, record_def, &mut ctx).unwrap();

        assert_eq!(bytes, 0x1234_u32.to_le_bytes());
    }

    #[test]
    fn encode_localized_lstring_text_allocates_when_not_seeded() {
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("FULL").unwrap();
        let sym = interner.intern("New Name");
        let entry = FieldEntry {
            sig,
            value: FieldValue::String(sym),
        };
        let master_map = no_masters();
        let mut strings = LocalizedStringsState {
            default_language: "en".to_string(),
            ..LocalizedStringsState::default()
        };
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: true,
            slot: None,
            localized_strings: Some(&mut strings),
        };

        let bytes = encode_field(&entry, record_def, &mut ctx).unwrap();

        let string_id = u32::from_le_bytes(bytes.try_into().unwrap());
        let strings = ctx.localized_strings.expect("strings still attached");
        assert_eq!(
            strings
                .by_language
                .get("en")
                .and_then(|table| table.get(&string_id))
                .map(String::as_str),
            Some("New Name")
        );
    }

    #[test]
    fn encode_int32_round_trip() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("EDID").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Int(-7),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();
        let back = i32::from_le_bytes(bytes.try_into().unwrap());
        assert_eq!(back, -7);
    }

    #[test]
    fn encode_float32_round_trip() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("EDID").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Float(1.5),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();
        let back = f32::from_le_bytes(bytes.try_into().unwrap());
        assert!((back - 1.5f32).abs() < 1e-6);
    }

    #[test]
    fn encode_string_null_terminated() {
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sym = interner.intern("TestWeap");
        let sig = SubrecordSig::from_str("EDID").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::String(sym),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();
        assert!(bytes.ends_with(&[0u8]), "zstring must be null-terminated");
        let s = std::str::from_utf8(&bytes[..bytes.len() - 1]).unwrap();
        assert_eq!(s, "TestWeap");
    }

    #[test]
    fn encode_bytes_passthrough() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let raw: smallvec::SmallVec<[u8; 32]> = smallvec::smallvec![0xDE, 0xAD, 0xBE, 0xEF];
        let sig = SubrecordSig::from_str("EDID").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(raw),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();
        assert_eq!(bytes, &[0xDE, 0xAD, 0xBE, 0xEF]);
    }

    #[test]
    fn encode_raw_struct_bytes_truncates_to_target_struct_size() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("RACE");
        let raw = SmallVec::<[u8; 32]>::from_vec((0..48).collect());
        let sig = SubrecordSig::from_str("ATKD").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(raw),
        };

        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();

        assert_eq!(bytes.len(), 44);
        assert_eq!(bytes, (0..44).collect::<Vec<u8>>());
    }

    #[test]
    fn encode_short_fixed_size_raw_bytes_pads_to_target_size() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("CELL");
        let sig = SubrecordSig::from_str("LTMP").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(SmallVec::new()),
        };

        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();

        assert_eq!(bytes, vec![0, 0, 0, 0]);
    }

    #[test]
    fn encode_fo4_efsh_dnam_uses_fv131_variant_size() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let raw = (0..1024)
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let mut record = Record::new(
            SigCode::from_str("EFSH").unwrap(),
            FormKey::parse("000801@Test.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw.clone())),
        });
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("EFSH should be encoded");
        let dnam = parsed
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "DNAM")
            .expect("EFSH.DNAM should be emitted");

        assert_eq!(parsed.form_version, Some(131));
        assert_eq!(dnam.data.len(), 157);
        assert_eq!(dnam.data.as_ref(), &raw[..157]);
    }

    /// SNDR.BNAM is a heterogeneous-size union (`values` struct = 6 bytes,
    /// `base_descriptor` formid = 4 bytes). A valid 6-byte `values` payload
    /// must NOT be truncated to the smaller formid variant size, and a valid
    /// 4-byte `base_descriptor` payload must NOT be padded up to the larger
    /// struct size.
    #[test]
    fn encode_sndr_bnam_heterogeneous_union_preserves_variant_widths() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("SNDR");
        let sig = SubrecordSig::from_str("BNAM").unwrap();

        // 6-byte `values` variant: b,b,B,B,H (trailing u16 Static Attenuation).
        let values_raw = SmallVec::<[u8; 32]>::from_vec(vec![0x00, 0x05, 0x80, 0x03, 0xB0, 0x04]);
        let values_entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(values_raw.clone()),
        };
        let values_bytes = encode_field_for_test(
            &values_entry,
            record_def,
            &interner,
            &no_masters(),
            0,
            false,
        )
        .unwrap();
        assert_eq!(values_bytes.len(), 6);
        assert_eq!(values_bytes, values_raw.to_vec());

        // 4-byte `base_descriptor` variant (formid).
        let formid_raw = SmallVec::<[u8; 32]>::from_vec(vec![0x11, 0x22, 0x33, 0x00]);
        let formid_entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(formid_raw.clone()),
        };
        let formid_bytes = encode_field_for_test(
            &formid_entry,
            record_def,
            &interner,
            &no_masters(),
            0,
            false,
        )
        .unwrap();
        assert_eq!(formid_bytes.len(), 4);
        assert_eq!(formid_bytes, formid_raw.to_vec());
    }

    #[test]
    fn encode_fixed_size_none_uses_zero_payload() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("CELL");
        let sig = SubrecordSig::from_str("LTMP").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::None,
        };

        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();

        assert_eq!(bytes, vec![0, 0, 0, 0]);
    }

    #[test]
    fn encode_nvnm_bytes_does_not_truncate_to_schema_prefix_size() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("ACTI");
        let raw = SmallVec::<[u8; 32]>::from_vec((0..116).collect());
        let sig = SubrecordSig::from_str("NVNM").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(raw),
        };

        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();

        assert_eq!(bytes.len(), 116);
        assert_eq!(bytes, (0..116).collect::<Vec<u8>>());
    }

    #[test]
    fn encode_vmad_bytes_does_not_truncate_to_schema_prefix_size() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("TERM");
        let mut raw = Vec::new();
        raw.extend_from_slice(&6_u16.to_le_bytes());
        raw.extend_from_slice(&2_u16.to_le_bytes());
        raw.extend_from_slice(&1_u16.to_le_bytes());
        raw.extend_from_slice(&32_u16.to_le_bytes());
        raw.extend_from_slice(b"WorkshopTerminalActorValueScript");
        raw.push(0);
        raw.extend_from_slice(&[0, 0, 0, 0]);
        let raw = SmallVec::<[u8; 32]>::from_vec(raw);
        let sig = SubrecordSig::from_str("VMAD").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::Bytes(raw.clone()),
        };

        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &no_masters(), 0, false).unwrap();

        assert_eq!(bytes.len(), raw.len());
        assert_eq!(bytes, raw.to_vec());
    }

    #[test]
    fn encode_variable_tail_struct_bytes_do_not_truncate_to_schema_prefix_size() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let cases = [
            ("NAVI", "NVMI", 93),
            ("NAVI", "NVPP", 8),
            ("CELL", "MHDT", 1028),
            ("WRLD", "MHDT", 152),
            ("WRLD", "RNAM", 40),
            ("CELL", "XCRI", 96),
            ("LAND", "VHGT", 16),
            ("FSTS", "DATA", 32),
            ("ARMO", "OBTS", 36),
            ("FURN", "OBTS", 67),
            ("NPC_", "OBTS", 42),
            ("WEAP", "OBTS", 112),
            ("LCTN", "LCEC", 8),
            ("REFR", "XLOC", 16),
            ("REFR", "XPLK", 8),
            ("REGN", "RDAT", 8),
            ("STAG", "TNAM", 36),
        ];

        for (record_sig, subrecord_sig, len) in cases {
            let record_def = schema
                .record_def(record_sig)
                .unwrap_or_else(|| panic!("missing {record_sig} record schema"));
            let raw_vec: Vec<u8> = (0..len).map(|i| (i & 0xFF) as u8).collect();
            let entry = FieldEntry {
                sig: SubrecordSig::from_str(subrecord_sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::<[u8; 32]>::from_vec(raw_vec.clone())),
            };

            let bytes =
                encode_field_for_test(&entry, Some(record_def), &interner, &no_masters(), 0, false)
                    .unwrap();

            assert_eq!(
                bytes.len(),
                raw_vec.len(),
                "{record_sig}.{subrecord_sig} must preserve variable-tail raw bytes"
            );
            assert_eq!(
                bytes, raw_vec,
                "{record_sig}.{subrecord_sig} payload must round-trip unchanged"
            );
        }
    }

    #[test]
    fn formkey_encodes_with_target_master_index() {
        // Target plugin imports two masters; FK points at the second one.
        // Encoded top byte must be 1 (the master's index in the list).
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("CNAM").unwrap();

        let masters = vec!["Fallout4.esm".to_string(), "DLCRobot.esm".to_string()];
        let master_map = build_master_lookup(&masters);
        let own_index = masters.len() as u32; // = 2

        let plugin_sym = interner.intern("DLCRobot.esm");
        let fk = FormKey {
            local: 0x00ABCDEF,
            plugin: plugin_sym,
        };
        let entry = FieldEntry {
            sig,
            value: FieldValue::FormKey(fk),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &master_map, own_index, false)
                .unwrap();
        let raw = u32::from_le_bytes(bytes.as_slice().try_into().unwrap());
        assert_eq!(raw >> 24, 0x01, "master byte must be DLCRobot's index (1)");
        assert_eq!(raw & 0x00FF_FFFF, 0x00ABCDEF, "object id must be preserved");
    }

    #[test]
    fn record_identity_distinguishes_master_override_from_local_record() {
        let schema = fo4_schema();
        let interner = StringInterner::new();
        let sig = SigCode::from_str("WEAP").unwrap();
        let master_override = Record::new(
            sig,
            FormKey::parse("000800@Fallout4.esm", &interner).unwrap(),
        );
        let local_record =
            Record::new(sig, FormKey::parse("000800@Output.esm", &interner).unwrap());
        let master_map = build_master_lookup(&["Fallout4.esm".to_string()]);
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 1,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let encoded_override =
            encode_record_for_target(master_override, &schema, &mut ctx, Some("fo4"))
                .unwrap()
                .unwrap();
        let encoded_local = encode_record_for_target(local_record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .unwrap();

        assert_eq!(encoded_override.form_id, 0x0000_0800);
        assert_eq!(encoded_local.form_id, 0x0100_0800);
        assert_ne!(encoded_override.form_id, encoded_local.form_id);
    }

    #[test]
    fn formkey_encodes_with_own_index_when_plugin_unknown() {
        // FK plugin sym is not present in the target's master list → encode
        // with own_index (the slot for the target plugin itself).
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("CNAM").unwrap();

        let masters = vec!["Fallout4.esm".to_string()];
        let master_map = build_master_lookup(&masters);
        let own_index = masters.len() as u32; // = 1

        let plugin_sym = interner.intern("SomeOtherMod.esp");
        let fk = FormKey {
            local: 0x00012345,
            plugin: plugin_sym,
        };
        let entry = FieldEntry {
            sig,
            value: FieldValue::FormKey(fk),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &master_map, own_index, false)
                .unwrap();
        let raw = u32::from_le_bytes(bytes.as_slice().try_into().unwrap());
        assert_eq!(
            raw >> 24,
            own_index,
            "unknown plugin falls back to own_index"
        );
        assert_eq!(raw & 0x00FF_FFFF, 0x00012345);
    }

    #[test]
    fn formkey_null_encodes_as_zero() {
        // A FormKey whose object-id bits are zero encodes as four zero bytes
        // regardless of which plugin it points at.
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("CNAM").unwrap();

        let masters = vec!["Fallout4.esm".to_string()];
        let master_map = build_master_lookup(&masters);
        let own_index = masters.len() as u32;

        // local = 0xFF000000 → object_id bits are zero. Plugin sym is
        // irrelevant; the FK is a null reference.
        let plugin_sym = interner.intern("Anywhere.esp");
        let fk = FormKey {
            local: 0xFF00_0000,
            plugin: plugin_sym,
        };
        let entry = FieldEntry {
            sig,
            value: FieldValue::FormKey(fk),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &master_map, own_index, false)
                .unwrap();
        assert_eq!(
            bytes,
            vec![0u8, 0, 0, 0],
            "null FK must encode as 4 zero bytes"
        );
    }

    #[test]
    fn formkey_case_insensitive_master_match() {
        // Master matching mirrors plugin_handle_add_master_native's case-insensitive
        // policy: "Fallout4.esm" in the list must match a FK whose plugin sym is
        // "FALLOUT4.ESM".
        let mut interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("WEAP");
        let sig = SubrecordSig::from_str("CNAM").unwrap();

        let masters = vec!["Fallout4.esm".to_string()];
        let master_map = build_master_lookup(&masters);
        let own_index = masters.len() as u32;

        let plugin_sym = interner.intern("FALLOUT4.ESM");
        let fk = FormKey {
            local: 0x00000800,
            plugin: plugin_sym,
        };
        let entry = FieldEntry {
            sig,
            value: FieldValue::FormKey(fk),
        };
        let bytes =
            encode_field_for_test(&entry, record_def, &interner, &master_map, own_index, false)
                .unwrap();
        let raw = u32::from_le_bytes(bytes.as_slice().try_into().unwrap());
        assert_eq!(
            raw >> 24,
            0x00,
            "uppercase plugin sym still matches case-insensitive"
        );
        assert_eq!(raw & 0x00FF_FFFF, 0x00000800);
    }

    #[test]
    fn add_record_round_trip() {
        let fixture = test_fixtures::fixture_plugin("fo4_minimal_weap.esm");
        if !fixture.exists() {
            return; // skip gracefully if fixture not built
        }

        // Run inside a Python-free mock context: use the internal native fns.
        // We need a Python runtime for plugin_handle_load_native. Since this
        // is a unit test without a Python interpreter, we verify the encode
        // path separately. The full round-trip is covered by the Python
        // integration test in test_native_record_io.py.
        //
        // What we CAN test: construct a Record and call add_record_native on
        // a fresh plugin handle using plugin_handle_new_native (which does not
        // need Python).
        let result = plugin_handle_new_native("Output.esm", Some("fo4"));
        let tgt_handle_id = match result {
            Ok(id) => id,
            Err(_) => return, // cannot create handle without Python runtime
        };

        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@fo4_minimal_weap.esm", &mut interner).unwrap();

        let sig = SigCode::from_str("WEAP").unwrap();
        let edid_sym = interner.intern("TestWeap");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();

        let record = Record {
            sig,
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: smallvec::SmallVec::new(),
        };

        let result = add_record_native(tgt_handle_id, record, &schema, &interner);
        assert!(
            result.is_ok(),
            "add_record_native failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn skyrim_interior_navmesh_writer_defers_cell_topology() {
        let handle_id = plugin_handle_new_native("SkyrimMerged.esp", Some("fo4")).unwrap();
        let schema = fo4_schema();
        let interner = StringInterner::new();
        let cell_form_id = 0x0001_3A7Eu32;
        let navm_form_id = 0x000E_537Du32;

        let cell = Record::new(
            SigCode::from_str("CELL").unwrap(),
            FormKey::parse("013A7E@SkyrimMerged.esp", &interner).unwrap(),
        );
        let mut navmesh = Record::new(
            SigCode::from_str("NAVM").unwrap(),
            FormKey::parse("0E537D@SkyrimMerged.esp", &interner).unwrap(),
        );
        let mut nvnm = Vec::new();
        nvnm.extend_from_slice(&15u32.to_le_bytes());
        nvnm.extend_from_slice(&0u32.to_le_bytes());
        nvnm.extend_from_slice(&0u32.to_le_bytes());
        nvnm.extend_from_slice(&cell_form_id.to_le_bytes());
        for _ in 0..8 {
            nvnm.extend_from_slice(&0u32.to_le_bytes());
        }
        navmesh.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NVNM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(nvnm)),
        });

        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle_id).unwrap();
        add_record_in_slot(slot, cell, &schema, &interner).unwrap();
        add_skyrim_navmesh_record_in_slot(slot, navmesh, &schema, &interner).unwrap();

        let cell_top = top_group_ref(&slot.parsed.root_items, b"CELL").expect("CELL top group");
        assert!(contains_record(&cell_top.children, "CELL", cell_form_id));

        let navm_top = top_group_ref(&slot.parsed.root_items, b"NAVM").expect("NAVM top group");
        assert!(contains_record(&navm_top.children, "NAVM", navm_form_id));
    }

    #[test]
    fn default_form_version_fo4_is_131() {
        assert_eq!(default_form_version_for_game(Some("fo4")), Some(131));
    }

    #[test]
    fn rebuild_worldspace_groups_from_source_nests_render_records() {
        let source_handle = plugin_handle_new_native("FalloutNV.esm", Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native("FalloutNV.esm", Some("fo4")).unwrap();

        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source = store.get_mut(&source_handle).unwrap();
            source.parsed.root_items = vec![top_group(
                *b"WRLD",
                vec![
                    ParsedItem::Record(parsed_record("WRLD", 0x000D_A726)),
                    ParsedItem::Group(ParsedGroup {
                        label: 0x000D_A726u32.to_le_bytes(),
                        group_type: 1,
                        tail: Bytes::new(),
                        children: vec![ParsedItem::Group(ParsedGroup {
                            label: 0u32.to_le_bytes(),
                            group_type: 4,
                            tail: Bytes::new(),
                            children: vec![ParsedItem::Group(ParsedGroup {
                                label: 0u32.to_le_bytes(),
                                group_type: 5,
                                tail: Bytes::new(),
                                children: vec![
                                    ParsedItem::Record(parsed_record("CELL", 0x000D_DF1C)),
                                    ParsedItem::Group(ParsedGroup {
                                        label: 0x000D_DF1Cu32.to_le_bytes(),
                                        group_type: 6,
                                        tail: Bytes::new(),
                                        children: vec![ParsedItem::Group(ParsedGroup {
                                            label: 0x000D_DF1Cu32.to_le_bytes(),
                                            group_type: 9,
                                            tail: Bytes::new(),
                                            children: vec![
                                                ParsedItem::Record(parsed_record(
                                                    "LAND",
                                                    0x000D_DF2B,
                                                )),
                                                ParsedItem::Record(parsed_record(
                                                    "REFR",
                                                    0x0011_1111,
                                                )),
                                            ],
                                        })],
                                    }),
                                ],
                            })],
                        })],
                    }),
                ],
            )];

            let target = store.get_mut(&target_handle).unwrap();
            target.parsed.header.masters = vec![
                "Fallout4.esm".into(),
                "DLCRobot.esm".into(),
                "DLCworkshop01.esm".into(),
                "DLCCoast.esm".into(),
                "DLCworkshop02.esm".into(),
                "DLCworkshop03.esm".into(),
                "DLCNukaWorld.esm".into(),
            ];
            target.parsed.header_size = 24;
            target.parsed.root_items = vec![
                top_group(
                    *b"WRLD",
                    vec![ParsedItem::Record(parsed_record("WRLD", 0x070D_A726))],
                ),
                top_group(
                    *b"CELL",
                    vec![ParsedItem::Record(parsed_record("CELL", 0x070D_DF1C))],
                ),
                top_group(
                    *b"LAND",
                    vec![ParsedItem::Record(parsed_record("LAND", 0x070D_DF2B))],
                ),
                top_group(
                    *b"REFR",
                    vec![ParsedItem::Record(parsed_record("REFR", 0x0711_1111))],
                ),
            ];
        }

        let source_to_target_formids = [
            (0x000D_A726, 0x070D_A726),
            (0x000D_DF1C, 0x070D_DF1C),
            (0x000D_DF2B, 0x070D_DF2B),
            (0x0011_1111, 0x0711_1111),
        ];
        let stats = rebuild_worldspace_groups_from_source_native(
            target_handle,
            source_handle,
            &source_to_target_formids,
        )
        .unwrap();

        assert_eq!(stats.groups_rebuilt, 1);
        assert_eq!(stats.records_nested, 4);
        assert_eq!(stats.flat_records_removed, 3);

        let store = plugin_handle_store_ref().lock().unwrap();
        let target = store.get(&target_handle).unwrap();
        assert!(
            !has_top_group(&target.parsed.root_items, b"LAND"),
            "LAND should move out of the flat top-level group"
        );
        let wrld = top_group_ref(&target.parsed.root_items, b"WRLD").expect("WRLD group");
        assert!(contains_record(&wrld.children, "LAND", 0x070D_DF2B));
        assert!(contains_record(&wrld.children, "REFR", 0x0711_1111));
        assert!(contains_group_label(&wrld.children, 1, 0x070D_A726));
        assert!(contains_group_label(&wrld.children, 6, 0x070D_DF1C));
        assert!(contains_group_label(&wrld.children, 9, 0x070D_DF1C));
    }

    #[test]
    fn rebuild_worldspace_groups_uses_source_target_map_for_colliding_object_ids() {
        let source_handle = plugin_handle_new_native("Skyrim.esm", Some("skyrimse")).unwrap();
        let target_handle = plugin_handle_new_native("Skyrim_Merged.esm", Some("fo4")).unwrap();

        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source = store.get_mut(&source_handle).unwrap();
            source.parsed.root_items = vec![top_group(
                *b"WRLD",
                vec![
                    ParsedItem::Record(parsed_record("WRLD", 0x0000_003C)),
                    ParsedItem::Group(ParsedGroup {
                        label: 0x0000_003Cu32.to_le_bytes(),
                        group_type: 1,
                        tail: Bytes::new(),
                        children: vec![ParsedItem::Record(parsed_record("REFR", 0x0000_0A14))],
                    }),
                ],
            )];

            let target = store.get_mut(&target_handle).unwrap();
            target.parsed.header.masters = vec![
                "Fallout4.esm".into(),
                "DLCRobot.esm".into(),
                "DLCworkshop01.esm".into(),
                "DLCCoast.esm".into(),
                "DLCworkshop02.esm".into(),
                "DLCworkshop03.esm".into(),
                "DLCNukaWorld.esm".into(),
            ];
            target.parsed.root_items = vec![
                top_group(
                    *b"WRLD",
                    vec![ParsedItem::Record(parsed_record("WRLD", 0x0700_0A14))],
                ),
                top_group(
                    *b"REFR",
                    vec![ParsedItem::Record(parsed_record("REFR", 0x0712_3456))],
                ),
            ];
        }

        let stats = rebuild_worldspace_groups_from_source_native(
            target_handle,
            source_handle,
            &[(0x0000_003C, 0x0700_0A14), (0x0000_0A14, 0x0712_3456)],
        )
        .unwrap();

        assert_eq!(stats.records_nested, 2);
        let store = plugin_handle_store_ref().lock().unwrap();
        let target = store.get(&target_handle).unwrap();
        let wrld = top_group_ref(&target.parsed.root_items, b"WRLD").expect("WRLD group");
        assert!(matches!(
            wrld.children.first(),
            Some(ParsedItem::Record(record))
                if record.signature.as_str() == "WRLD" && record.form_id == 0x0700_0A14
        ));
        assert!(contains_group_label(&wrld.children, 1, 0x0700_0A14));
        assert!(contains_record(&wrld.children, "REFR", 0x0712_3456));
        let world_children = wrld
            .children
            .iter()
            .find_map(|item| match item {
                ParsedItem::Group(group) if group.group_type == 1 => Some(group),
                _ => None,
            })
            .expect("World Children group");
        assert!(!contains_record(
            &world_children.children,
            "WRLD",
            0x0700_0A14
        ));
    }

    #[test]
    fn rebuild_source_topology_preserves_skyrim_interior_cell_children() {
        const SOURCE_CELL: u32 = 0x0001_33C6;
        const SOURCE_PERSISTENT_REFR: u32 = 0x0002_1001;
        const SOURCE_TEMPORARY_REFR: u32 = 0x0002_1002;
        const SOURCE_ACHR: u32 = 0x0002_1003;
        const SOURCE_NAVM: u32 = 0x000E_537D;
        const TARGET_CELL: u32 = 0x0701_33C6;
        const TARGET_PERSISTENT_REFR: u32 = 0x0702_1001;
        const TARGET_TEMPORARY_REFR: u32 = 0x0702_1002;
        const TARGET_NAVM: u32 = 0x070E_537D;

        let source_handle = plugin_handle_new_native("Skyrim.esm", Some("skyrimse")).unwrap();
        let target_handle = plugin_handle_new_native("Skyrim_Merged.esm", Some("fo4")).unwrap();

        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source = store.get_mut(&source_handle).unwrap();
            source.parsed.root_items = vec![top_group(
                *b"CELL",
                vec![ParsedItem::Group(ParsedGroup {
                    label: 0i32.to_le_bytes(),
                    group_type: 2,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Group(ParsedGroup {
                        label: 1i32.to_le_bytes(),
                        group_type: 3,
                        tail: Bytes::new(),
                        children: vec![
                            ParsedItem::Record(parsed_record("CELL", SOURCE_CELL)),
                            ParsedItem::Group(ParsedGroup {
                                label: SOURCE_CELL.to_le_bytes(),
                                group_type: 6,
                                tail: Bytes::new(),
                                children: vec![
                                    ParsedItem::Group(ParsedGroup {
                                        label: SOURCE_CELL.to_le_bytes(),
                                        group_type: 8,
                                        tail: Bytes::new(),
                                        children: vec![
                                            ParsedItem::Record(parsed_record(
                                                "REFR",
                                                SOURCE_PERSISTENT_REFR,
                                            )),
                                            ParsedItem::Record(parsed_record("ACHR", SOURCE_ACHR)),
                                        ],
                                    }),
                                    ParsedItem::Group(ParsedGroup {
                                        label: SOURCE_CELL.to_le_bytes(),
                                        group_type: 9,
                                        tail: Bytes::new(),
                                        children: vec![
                                            ParsedItem::Record(parsed_record(
                                                "REFR",
                                                SOURCE_TEMPORARY_REFR,
                                            )),
                                            ParsedItem::Record(parsed_record("NAVM", SOURCE_NAVM)),
                                        ],
                                    }),
                                ],
                            }),
                        ],
                    })],
                })],
            )];

            let target = store.get_mut(&target_handle).unwrap();
            target.parsed.header.masters = vec![
                "Fallout4.esm".into(),
                "DLCRobot.esm".into(),
                "DLCworkshop01.esm".into(),
                "DLCCoast.esm".into(),
                "DLCworkshop02.esm".into(),
                "DLCworkshop03.esm".into(),
                "DLCNukaWorld.esm".into(),
            ];
            target.parsed.header_size = 24;
            let mut converted_navm = parsed_record("NAVM", TARGET_NAVM);
            converted_navm
                .subrecords
                .push(parsed_subrecord("NVNM", &15u32.to_le_bytes()));
            target.parsed.root_items = vec![
                top_group(
                    *b"CELL",
                    vec![ParsedItem::Record(parsed_record("CELL", TARGET_CELL))],
                ),
                top_group(*b"NAVM", vec![ParsedItem::Record(converted_navm)]),
                top_group(
                    *b"REFR",
                    vec![
                        ParsedItem::Record(parsed_record("REFR", TARGET_PERSISTENT_REFR)),
                        ParsedItem::Record(parsed_record("REFR", TARGET_TEMPORARY_REFR)),
                    ],
                ),
            ];
        }

        let stats = rebuild_worldspace_groups_from_source_native(
            target_handle,
            source_handle,
            &[
                (SOURCE_CELL, TARGET_CELL),
                (SOURCE_PERSISTENT_REFR, TARGET_PERSISTENT_REFR),
                (SOURCE_TEMPORARY_REFR, TARGET_TEMPORARY_REFR),
                (SOURCE_NAVM, TARGET_NAVM),
            ],
        )
        .unwrap();

        assert_eq!(stats.groups_rebuilt, 1);
        assert_eq!(stats.records_nested, 4);
        assert_eq!(stats.flat_records_removed, 3);

        let store = plugin_handle_store_ref().lock().unwrap();
        let target = store.get(&target_handle).unwrap();
        let cells = top_group_ref(&target.parsed.root_items, b"CELL").expect("CELL group");
        assert!(contains_record(&cells.children, "CELL", TARGET_CELL));
        assert!(contains_record(
            &cells.children,
            "REFR",
            TARGET_PERSISTENT_REFR
        ));
        assert!(contains_record(
            &cells.children,
            "REFR",
            TARGET_TEMPORARY_REFR
        ));
        assert!(contains_record(&cells.children, "NAVM", TARGET_NAVM));
        assert!(contains_group_label(&cells.children, 6, TARGET_CELL));
        assert!(contains_group_label(&cells.children, 8, TARGET_CELL));
        assert!(contains_group_label(&cells.children, 9, TARGET_CELL));
        assert!(
            !contains_record(&cells.children, "ACHR", SOURCE_ACHR),
            "an MVP-excluded child without a target mapping must not be copied"
        );
        assert!(
            !has_top_group(&target.parsed.root_items, b"REFR"),
            "nested placed references must be removed from their flat group"
        );

        fn find_record<'a>(
            items: &'a [ParsedItem],
            signature: &str,
            form_id: u32,
        ) -> Option<&'a ParsedRecord> {
            items.iter().find_map(|item| match item {
                ParsedItem::Record(record)
                    if record.signature.as_str() == signature && record.form_id == form_id =>
                {
                    Some(record)
                }
                ParsedItem::Group(group) => find_record(&group.children, signature, form_id),
                _ => None,
            })
        }

        let navm = find_record(&cells.children, "NAVM", TARGET_NAVM).expect("nested NAVM");
        let nvnm = navm
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "NVNM")
            .expect("converted NVNM");
        assert_eq!(
            u32::from_le_bytes(nvnm.data[..4].try_into().unwrap()),
            15,
            "topology rebuild must reuse the already converted target NAVM"
        );
    }

    #[test]
    fn default_form_version_unknown_game_is_none() {
        assert_eq!(default_form_version_for_game(None), None);
        assert_eq!(default_form_version_for_game(Some("fo3")), None);
        assert_eq!(default_form_version_for_game(Some("fnv")), None);
        assert_eq!(default_form_version_for_game(Some("skyrimse")), None);
        assert_eq!(default_form_version_for_game(Some("starfield")), None);
    }

    #[test]
    fn add_record_native_sets_form_version_131_on_fo4_handle() {
        // Records inserted on a fo4 plugin handle must carry form_version=131
        // in their ParsedRecord, matching Python's
        // _build_authoring_record_dicts behaviour (fixups.py:7814).
        use esp_authoring_core::plugin_runtime::plugin_handle_store_ref;

        let tgt_handle_id = match plugin_handle_new_native("FormVer.esm", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return, // no Python runtime in unit tests
        };

        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@FormVer.esm", &mut interner).unwrap();
        let sig = SigCode::from_str("WEAP").unwrap();
        let edid_sym = interner.intern("FormVerWeap");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let record = Record {
            sig,
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: smallvec::SmallVec::new(),
        };

        add_record_native(tgt_handle_id, record, &schema, &interner).unwrap();

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store
            .get(&tgt_handle_id)
            .expect("fo4 handle present after add_record_native");
        // Find our inserted record. Walk the tree until we hit the WEAP record.
        fn find_first_record<'a>(
            items: &'a [esp_authoring_core::plugin_runtime::ParsedItem],
        ) -> Option<&'a esp_authoring_core::plugin_runtime::ParsedRecord> {
            for item in items {
                match item {
                    esp_authoring_core::plugin_runtime::ParsedItem::Record(r) => return Some(r),
                    esp_authoring_core::plugin_runtime::ParsedItem::Group(g) => {
                        if let Some(r) = find_first_record(&g.children) {
                            return Some(r);
                        }
                    }
                }
            }
            None
        }
        let record = find_first_record(&slot.parsed.root_items)
            .expect("inserted record should be reachable from root_items");
        assert_eq!(
            record.form_version,
            Some(131),
            "fo4 records must emit form_version=131; got {:?}",
            record.form_version
        );
    }

    #[test]
    fn add_record_native_drops_subrecords_not_in_target_schema() {
        // A FO76 STAT carries DEFL/ENLM/ENLT/ENLS/AUUV/NAM1/LODP. None of
        // those are in the FO4 STAT schema. Writing them produces an
        // "Invalid base object" in CK, so encode_record_for_target must
        // filter them out.
        use esp_authoring_core::plugin_runtime::plugin_handle_store_ref;

        let handle_id = match plugin_handle_new_native("DropSchema.esm", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return, // no Python runtime in unit tests
        };

        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("133863@DropSchema.esm", &mut interner).unwrap();
        let sig = SigCode::from_str("STAT").unwrap();
        let edid_sym = interner.intern("HollyShrub03");

        let mk_bytes = |sig_str: &str, payload: &[u8]| FieldEntry {
            sig: SubrecordSig::from_str(sig_str).unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(payload)),
        };

        let record = Record {
            sig,
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(edid_sym),
                },
                // Valid FO4 STAT subrecords — kept.
                mk_bytes("OBND", &[0; 12]),
                mk_bytes("MODL", b"Landscape\\Plants\\HollyShrub03.nif\0"),
                // FO76-only — must be dropped by the target schema gate.
                mk_bytes("DEFL", &[0xE1, 0xA4, 0x00, 0x01]),
                mk_bytes("ENLM", &[0x01, 0x00, 0x00, 0x80]),
                mk_bytes("ENLT", &[0x8D, 0x63, 0x47, 0x6E]),
                mk_bytes("ENLS", &[0x66, 0x66, 0x66, 0x3F]),
                mk_bytes("AUUV", &[0; 28]),
                mk_bytes("NAM1", &[0; 4]),
                mk_bytes("LODP", &[0; 4]),
            ],
            warnings: smallvec::SmallVec::new(),
        };

        add_record_native(handle_id, record, &schema, &interner).unwrap();

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle_id).expect("handle present");
        fn find_first_record<'a>(
            items: &'a [esp_authoring_core::plugin_runtime::ParsedItem],
        ) -> Option<&'a esp_authoring_core::plugin_runtime::ParsedRecord> {
            for item in items {
                match item {
                    esp_authoring_core::plugin_runtime::ParsedItem::Record(r) => return Some(r),
                    esp_authoring_core::plugin_runtime::ParsedItem::Group(g) => {
                        if let Some(r) = find_first_record(&g.children) {
                            return Some(r);
                        }
                    }
                }
            }
            None
        }
        let written = find_first_record(&slot.parsed.root_items).expect("STAT was written");
        let sigs: Vec<&str> = written
            .subrecords
            .iter()
            .map(|s| s.signature.as_str())
            .collect();

        for keep in ["EDID", "OBND", "MODL"] {
            assert!(sigs.contains(&keep), "expected {keep} kept, got {sigs:?}");
        }
        for drop in ["DEFL", "ENLM", "ENLT", "ENLS", "AUUV", "NAM1", "LODP"] {
            assert!(
                !sigs.contains(&drop),
                "{drop} is not in FO4 STAT schema; should have been filtered, got {sigs:?}"
            );
        }
    }

    #[test]
    fn encode_record_drops_record_not_in_target_schema() {
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@Unsupported.esm", &mut interner).unwrap();
        let sig = SigCode::from_str("ATXO").unwrap();
        let record = Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        };

        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };
        let result = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"));

        assert!(
            matches!(result, Ok(None)),
            "ATXO is not in the FO4 generated schema and must be dropped"
        );
    }

    #[test]
    fn fo4_target_emit_normalizes_form_version_and_version_control() {
        // Every record emitted into an FO4 target plugin must carry
        // form_version=131 and version_control=0, regardless of the values
        // present on the source side. Source `Record` does not expose
        // form_version / version_control fields (those live on the
        // ParsedRecord header), but the encoder must still ensure the
        // emitted ParsedRecord normalizes both unconditionally.
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("Phase8NormTest");
        let fk = FormKey::parse("000801@Test.esm", &mut interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("WEAP").unwrap(),
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };
        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("WEAP should be encoded");
        assert_eq!(
            parsed.form_version,
            Some(131),
            "FO4 target must force form_version=131"
        );
        assert_eq!(
            parsed.version_control, 0,
            "FO4 target must reset version_control to 0"
        );
    }

    #[test]
    fn encode_record_force_compresses_cell_and_land_on_fo4_target() {
        // FO4 requires CELL and LAND records to carry the COMPRESSED flag
        // (0x00040000) on emit. FO76 sources sometimes ship them
        // uncompressed; without this stamp the target plugin would be
        // invalid for CK.
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        for sig in &["CELL", "LAND"] {
            let fk = FormKey::parse("000801@Test.esm", &mut interner).unwrap();
            let record = Record {
                sig: SigCode::from_str(sig).unwrap(),
                form_key: fk,
                eid: None,
                flags: RecordFlags::empty(),
                fields: smallvec::smallvec![],
                warnings: smallvec::SmallVec::new(),
            };
            let master_map = no_masters();
            let mut ctx = EncodeContext {
                interner: &interner,
                master_map: &master_map,
                own_index: 0,
                target_is_localized: false,
                slot: None,
                localized_strings: None,
            };
            let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
                .unwrap()
                .unwrap_or_else(|| panic!("{sig} should be encoded"));
            assert_eq!(
                parsed.flags & 0x0004_0000,
                0x0004_0000,
                "{sig} flags should have COMPRESSED set on FO4 target"
            );
        }
    }

    #[test]
    fn encode_record_does_not_force_compress_non_cell_or_land_on_fo4() {
        // Only CELL and LAND get the auto-COMPRESSED stamp.
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000801@Test.esm", &mut interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("STAT").unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![],
            warnings: smallvec::SmallVec::new(),
        };
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };
        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("STAT should be encoded");
        assert_eq!(
            parsed.flags & 0x0004_0000,
            0,
            "STAT must not be auto-COMPRESSED"
        );
    }

    #[test]
    fn encode_record_projects_skyrim_workbench_data_to_fo4_crafting_width() {
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@Test.esm", &mut interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("FURN").unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("WBDT").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[8])),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("FURN should be encoded");
        let wbdt = parsed
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "WBDT")
            .expect("WBDT should be present");

        assert_eq!(wbdt.data.as_ref(), &[8]);
    }

    #[test]
    fn fo4_ck_drops_zero_length_workbench_data() {
        for record_sig in ["FURN", "TERM"] {
            let mut subrecords = vec![
                parsed_subrecord("EDID", b"Workbench\0"),
                parsed_subrecord("WBDT", &[]),
                parsed_subrecord("FULL", b"Workbench\0"),
            ];

            apply_fo4_ck_payload_limits(record_sig, &mut subrecords, Some("fo4"));

            assert_eq!(
                subrecords
                    .iter()
                    .map(|subrecord| subrecord.signature.as_str())
                    .collect::<Vec<_>>(),
                ["EDID", "FULL"],
                "{record_sig} should drop only empty WBDT"
            );
        }
    }

    #[test]
    fn fo4_ck_projects_crafting_workbench_data_but_preserves_legacy_furniture_tail() {
        let mut subrecords = vec![
            parsed_subrecord("WBDT", &[0x05, 0x01]),
            parsed_subrecord("WBDT", &[0x00, 0xFF, 0xBB]),
            parsed_subrecord("WBDT", &[0x00, 0x00]),
            parsed_subrecord("WBDT", &[0x07]),
        ];

        apply_fo4_ck_payload_limits("FURN", &mut subrecords, Some("fo4"));

        assert_eq!(subrecords[0].data.as_ref(), &[0x05]);
        assert_eq!(subrecords[1].data.as_ref(), &[0x00, 0xFF]);
        assert_eq!(subrecords[2].data.as_ref(), &[0x00]);
        assert_eq!(subrecords[3].data.as_ref(), &[0x07]);
    }

    #[test]
    fn fo4_ck_normalizes_crafting_furniture_marker_parameter_tails() {
        let mut marker_rows = vec![0x11; 48];
        marker_rows[21..24].copy_from_slice(&[0x01, 0x00, 0x00]);
        marker_rows[45..48].copy_from_slice(&[0x00, 0x00, 0x00]);
        let mut subrecords = vec![
            parsed_subrecord("WBDT", &[0x05, 0x01]),
            parsed_subrecord("SNAM", &marker_rows),
        ];

        apply_fo4_ck_payload_limits("FURN", &mut subrecords, Some("fo4"));

        assert_eq!(subrecords[0].data.as_ref(), &[0x05]);
        assert_eq!(&subrecords[1].data[21..24], &[0xFF, 0xFF, 0xFF]);
        assert_eq!(&subrecords[1].data[45..48], &[0xFF, 0xFF, 0xFF]);
        assert!(subrecords[1].data[..21].iter().all(|byte| *byte == 0x11));
    }

    #[test]
    fn fo4_ck_normalizes_instrument_furniture_marker_parameter_tails() {
        let mut marker_row = [0x33; 24];
        marker_row[21..24].copy_from_slice(&[0x01, 0x00, 0x00]);
        let mut subrecords = vec![
            parsed_subrecord("WBDT", &[0x00, 0x01]),
            parsed_subrecord("SNAM", &marker_row),
        ];

        apply_fo4_ck_payload_limits("FURN", &mut subrecords, Some("fo4"));

        assert_eq!(subrecords[0].data.as_ref(), &[0x00]);
        assert_eq!(&subrecords[1].data[21..24], &[0xFF, 0xFF, 0xFF]);
        assert!(subrecords[1].data[..21].iter().all(|byte| *byte == 0x33));
    }

    #[test]
    fn fo4_ck_preserves_legacy_furniture_marker_parameter_tails() {
        let marker_row = [0x22; 24];
        let mut subrecords = vec![
            parsed_subrecord("WBDT", &[0x00, 0xFF]),
            parsed_subrecord("SNAM", &marker_row),
        ];

        apply_fo4_ck_payload_limits("FURN", &mut subrecords, Some("fo4"));

        assert_eq!(subrecords[0].data.as_ref(), &[0x00, 0xFF]);
        assert_eq!(subrecords[1].data.as_ref(), &marker_row);
    }

    #[test]
    fn fo4_ck_projects_terminal_workbench_data_to_one_byte() {
        let mut subrecords = vec![parsed_subrecord("WBDT", &[0x00, 0xFF])];

        apply_fo4_ck_payload_limits("TERM", &mut subrecords, Some("fo4"));

        assert_eq!(subrecords[0].data.as_ref(), &[0x00]);
    }

    #[test]
    fn fo4_ck_workbench_data_rule_does_not_touch_unrelated_records() {
        let mut subrecords = vec![
            parsed_subrecord("WBDT", &[]),
            parsed_subrecord("WBDT", &[0x7f, 0xaa]),
        ];

        apply_fo4_ck_payload_limits("STAT", &mut subrecords, Some("fo4"));

        assert!(subrecords[0].data.is_empty());
        assert_eq!(subrecords[1].data.as_ref(), &[0x7f, 0xaa]);
    }

    #[test]
    fn encode_record_projects_fo4_ck_damage_type_rows() {
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@Test.esm", &mut interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("WEAP").unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("DAMA").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[
                    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
                ])),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("WEAP should be encoded");
        let dama = parsed
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "DAMA")
            .expect("DAMA should be present");

        assert_eq!(dama.data.as_ref(), &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn encode_record_truncates_fo4_ck_movement_speed_data() {
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@Test.esm", &mut interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("MOVT").unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("SPED").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec((0_u8..124).collect())),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("MOVT should be encoded");
        let sped = parsed
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "SPED")
            .expect("SPED should be present");

        assert_eq!(sped.data.len(), 112);
        assert_eq!(
            sped.data.as_ref(),
            (0_u8..112).collect::<Vec<_>>().as_slice()
        );
    }

    #[test]
    fn fo4_ck_scene_payload_limits_are_contextual() {
        let mut subrecords = vec![
            parsed_subrecord("FNAM", &[1, 0, 0, 0]),
            parsed_subrecord("WNAM", &[0x5e, 0x01, 0, 0]),
            parsed_subrecord("FNAM", &[2, 0, 0, 0]),
            parsed_subrecord("FNAM", &[3, 0, 0, 0]),
            parsed_subrecord("SCQS", &[0xff, 0xff, 0x64, 0]),
            parsed_subrecord("DATA", &(0_u8..84).collect::<Vec<_>>()),
            parsed_subrecord("PNAM", &[0x63, 0x11, 0x03, 0x07]),
            parsed_subrecord("SCQS", &[0xff, 0xff, 0, 0]),
            parsed_subrecord("SNAM", &[0, 0, 0x80, 0x3e]),
            parsed_subrecord("SCQS", &[0x2d, 0x01, 0, 0]),
            parsed_subrecord("VNAM", &[3, 0, 0, 0]),
            parsed_subrecord("SCQS", &[4, 0, 5, 0]),
        ];

        apply_fo4_ck_payload_limits("SCEN", &mut subrecords, Some("fo4"));

        assert_eq!(subrecords[0].data.len(), 4, "top-level FNAM stays uint32");
        assert_eq!(
            subrecords[2].data.as_ref(),
            &[2, 0],
            "phase FNAM after WNAM is uint16"
        );
        assert_eq!(
            subrecords[3].data.len(),
            4,
            "adjacent non-phase FNAM is not truncated"
        );
        assert_eq!(
            subrecords[4].data.len(),
            4,
            "phase SCQS stays its two-int16 payload"
        );
        assert_eq!(
            subrecords[5].data.as_ref(),
            &[0, 1, 2, 3],
            "SCEN.DATA is a topic FormID"
        );
        assert_eq!(
            subrecords[7].data.as_ref(),
            &[0xff, 0xff],
            "action SCQS after PNAM is int16"
        );
        assert_eq!(
            subrecords[9].data.as_ref(),
            &[0x2d, 0x01],
            "action SCQS after SNAM is int16"
        );
        assert_eq!(
            subrecords[11].data.len(),
            4,
            "terminal SCQS stays its two-int16 payload"
        );
    }

    #[test]
    fn encode_record_preserves_scen_empty_action_markers() {
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@Test.esm", &mut interner).unwrap();
        let bytes = |sig: &str, data: &[u8]| FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(data)),
        };
        let record = Record {
            sig: SigCode::from_str("SCEN").unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                bytes("EDID", b"Scene\0"),
                FieldEntry {
                    sig: SubrecordSig::from_str("FNAM").unwrap(),
                    value: FieldValue::Uint(0x5024),
                },
                bytes("ALID", &1_i32.to_le_bytes()),
                bytes("LNAM", &0_u32.to_le_bytes()),
                bytes("DNAM", &10_u32.to_le_bytes()),
                bytes("ANAM", &3_u16.to_le_bytes()),
                bytes("ALID", &1_i32.to_le_bytes()),
                bytes("INAM", &1_u32.to_le_bytes()),
                bytes("DTGT", &1_i32.to_le_bytes()),
                bytes("ANAM", &[]),
                bytes("ANAM", &0_u16.to_le_bytes()),
                bytes("ALID", &1_i32.to_le_bytes()),
                bytes("INAM", &2_u32.to_le_bytes()),
                bytes("DATA", &0x0756_D327_u32.to_le_bytes()),
                bytes("ANAM", &[]),
            ],
            warnings: smallvec::SmallVec::new(),
        };
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("SCEN should be encoded");
        let sigs = parsed
            .subrecords
            .iter()
            .map(|subrecord| subrecord.signature.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            sigs,
            vec![
                "EDID", "FNAM", "ALID", "LNAM", "DNAM", "ANAM", "ALID", "INAM", "DTGT", "ANAM",
                "ANAM", "ALID", "INAM", "DATA", "ANAM",
            ]
        );
        let anam_lengths = parsed
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "ANAM")
            .map(|subrecord| subrecord.data.len())
            .collect::<Vec<_>>();
        assert_eq!(anam_lengths, vec![2, 0, 2, 0]);
    }

    #[test]
    fn encode_scen_mixed_timer_and_template_tnam_selects_distinct_slots() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("SCEN").expect("SCEN schema");
        let timer = scen_test_field("TNAM", FieldValue::Float(1.25));
        let template_fk = FormKey::parse("55DE1F@Test.esm", &interner).unwrap();
        let template = scen_test_field("TNAM", FieldValue::FormKey(template_fk));

        assert_eq!(
            scen_tnam_def_for_entry(record_def, &timer, false)
                .and_then(|subrecord| subrecord.codec.as_deref()),
            Some("float32")
        );
        assert_eq!(
            scen_tnam_def_for_entry(record_def, &template, true)
                .and_then(|subrecord| subrecord.codec.as_deref()),
            Some("formid")
        );

        let parsed = encode_scen_for_test(
            &interner,
            vec![
                scen_test_field(
                    "EDID",
                    FieldValue::Bytes(SmallVec::from_slice(b"MixedScene\0")),
                ),
                scen_test_field("ANAM", FieldValue::Uint(1)),
                scen_test_field("INAM", FieldValue::Uint(0)),
                timer,
                scen_test_field("ANAM", FieldValue::None),
                scen_test_field("VNAM", FieldValue::Bytes(SmallVec::from_vec(vec![0; 16]))),
                template,
            ],
            7,
        );

        assert_eq!(
            scen_tnam_payloads(&parsed),
            vec![
                1.25_f32.to_le_bytes().to_vec(),
                0x0755_DE1F_u32.to_le_bytes().to_vec(),
            ]
        );
    }

    #[test]
    fn encode_scen_template_only_tnam_survives_missing_optional_body_fields() {
        let interner = StringInterner::new();
        let schema = fo4_schema();
        let record_def = schema.record_def("SCEN").expect("SCEN schema");
        let template_fk = FormKey::parse("55DE1F@Test.esm", &interner).unwrap();
        let template = scen_test_field("TNAM", FieldValue::FormKey(template_fk));

        assert_eq!(
            scen_tnam_def_for_entry(record_def, &template, false)
                .and_then(|subrecord| subrecord.codec.as_deref()),
            Some("formid")
        );

        let parsed = encode_scen_for_test(&interner, vec![template], 7);
        assert_eq!(
            scen_tnam_payloads(&parsed),
            vec![0x0755_DE1F_u32.to_le_bytes().to_vec()]
        );
    }

    #[test]
    fn encode_scen_timer_only_tnam_remains_byte_identical() {
        let interner = StringInterner::new();
        let timer_bytes = 2.75_f32.to_le_bytes();
        let timer = scen_test_field(
            "TNAM",
            FieldValue::Bytes(SmallVec::from_slice(&timer_bytes)),
        );
        let parsed = encode_scen_for_test(
            &interner,
            vec![
                scen_test_field("ANAM", FieldValue::Uint(1)),
                scen_test_field("INAM", FieldValue::Uint(0)),
                timer,
                scen_test_field("ANAM", FieldValue::None),
            ],
            7,
        );

        assert_eq!(scen_tnam_payloads(&parsed), vec![timer_bytes.to_vec()]);
    }

    #[test]
    fn add_scen_round_trip_persists_repaired_template_tnam_to_output_plugin() {
        let handle_id = plugin_handle_new_native("Output.esm", Some("fo4")).unwrap();
        let schema = fo4_schema();
        let interner = StringInterner::new();
        let template_fk = FormKey::parse("55DE1F@Output.esm", &interner).unwrap();
        let mut record = Record::new(
            SigCode::from_str("SCEN").unwrap(),
            FormKey::parse("000800@Output.esm", &interner).unwrap(),
        );
        record
            .fields
            .push(scen_test_field("TNAM", FieldValue::FormKey(template_fk)));

        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle_id).expect("output handle");
        slot.parsed.header.masters = vec![
            "Fallout4.esm".into(),
            "DLCRobot.esm".into(),
            "DLCworkshop01.esm".into(),
            "DLCCoast.esm".into(),
            "DLCworkshop02.esm".into(),
            "DLCworkshop03.esm".into(),
            "DLCNukaWorld.esm".into(),
        ];
        add_record_in_slot(slot, record, &schema, &interner).unwrap();

        fn find_scen(items: &[ParsedItem]) -> Option<&ParsedRecord> {
            items.iter().find_map(|item| match item {
                ParsedItem::Record(record) if record.signature.as_str() == "SCEN" => Some(record),
                ParsedItem::Group(group) => find_scen(&group.children),
                _ => None,
            })
        }

        let written = find_scen(&slot.parsed.root_items).expect("written SCEN");
        assert_eq!(
            scen_tnam_payloads(written),
            vec![0x0755_DE1F_u32.to_le_bytes().to_vec()]
        );
    }

    #[test]
    fn encode_record_preserves_pack_package_data_cnam_lengths() {
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@Test.esm", &mut interner).unwrap();
        let sig = SigCode::from_str("PACK").unwrap();
        let pkcu_sig = SubrecordSig::from_str("PKCU").unwrap();
        let anam_sig = SubrecordSig::from_str("ANAM").unwrap();
        let cnam_sig = SubrecordSig::from_str("CNAM").unwrap();
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        let record = Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: pkcu_sig,
                    value: FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().repeat(3))),
                },
                FieldEntry {
                    sig: anam_sig,
                    value: FieldValue::Bytes(SmallVec::from_slice(b"Float\0")),
                },
                FieldEntry {
                    sig: cnam_sig,
                    value: FieldValue::Bytes(SmallVec::from_vec(12.5_f32.to_le_bytes().to_vec())),
                },
                FieldEntry {
                    sig: anam_sig,
                    value: FieldValue::Bytes(SmallVec::from_slice(b"Bool\0")),
                },
                FieldEntry {
                    sig: cnam_sig,
                    value: FieldValue::Bytes(SmallVec::from_slice(&[1])),
                },
                FieldEntry {
                    sig: xnam_sig,
                    value: FieldValue::Bytes(SmallVec::from_slice(&[0])),
                },
            ],
            warnings: smallvec::SmallVec::new(),
        };
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("PACK should be encoded");
        let cnam_payloads: Vec<Vec<u8>> = parsed
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "CNAM")
            .map(|subrecord| subrecord.data.to_vec())
            .collect();

        assert_eq!(
            cnam_payloads,
            vec![12.5_f32.to_le_bytes().to_vec(), vec![1]]
        );
    }

    #[test]
    fn encode_record_expands_pack_procedure_tree_pnam_codes() {
        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@Test.esm", &mut interner).unwrap();
        let record = Record {
            sig: SigCode::from_str("PACK").unwrap(),
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("PKCU").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().repeat(3))),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("XNAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_slice(&[0])),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("ANAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_slice(b"Procedure\0")),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("PNAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_slice(b"Esco")),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("FNAM").unwrap(),
                    value: FieldValue::Uint(0),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("UNAM").unwrap(),
                    value: FieldValue::Uint(0),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("BNAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_slice(b"destination\0")),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("PNAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_slice(&1_u32.to_le_bytes())),
                },
            ],
            warnings: smallvec::SmallVec::new(),
        };
        let master_map = no_masters();
        let mut ctx = EncodeContext {
            interner: &interner,
            master_map: &master_map,
            own_index: 0,
            target_is_localized: false,
            slot: None,
            localized_strings: None,
        };

        let parsed = encode_record_for_target(record, &schema, &mut ctx, Some("fo4"))
            .unwrap()
            .expect("PACK should be encoded");
        let pnam_payloads: Vec<Vec<u8>> = parsed
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "PNAM")
            .map(|subrecord| subrecord.data.to_vec())
            .collect();

        assert_eq!(pnam_payloads, vec![b"Escort\0".to_vec(), vec![1, 0, 0, 0]]);
    }

    #[test]
    fn replace_record_native_round_trip() {
        // Create a fresh handle, add a record, replace it with a mutation,
        // then verify the replacement was accepted without error.
        let handle_id = match plugin_handle_new_native("ReplaceTest.esm", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return, // no Python runtime in unit tests
        };

        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000800@ReplaceTest.esm", &mut interner).unwrap();
        let sig = SigCode::from_str("WEAP").unwrap();

        // First insertion.
        let edid_v1 = interner.intern("WeapV1");
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let record_v1 = Record {
            sig,
            form_key: fk,
            eid: Some(edid_v1),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_v1),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        add_record_native(handle_id, record_v1, &schema, &interner).unwrap();

        // Replace with mutated EDID.
        let edid_v2 = interner.intern("WeapV2");
        let record_v2 = Record {
            sig,
            form_key: fk,
            eid: Some(edid_v2),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_v2),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        let result = replace_record_native(handle_id, record_v2, &schema, &interner);
        assert!(
            result.is_ok(),
            "replace_record_native failed: {:?}",
            result.err()
        );
    }

    #[test]
    fn replace_record_contents_native_preserves_quest_child_group_adjacency() {
        const QUEST_FORM_ID: u32 = 0x0010_0001;
        const DIALOGUE_FORM_ID: u32 = 0x0010_0002;

        let handle_id = match plugin_handle_new_native("QuestReplaceTest.esm", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return,
        };
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle_id).unwrap();
            let mut parsed_quest = parsed_record("QUST", QUEST_FORM_ID);
            parsed_quest
                .subrecords
                .push(parsed_subrecord("EDID", b"QuestReplaceTest\0"));
            slot.parsed.root_items = vec![ParsedItem::Group(ParsedGroup {
                label: *b"QUST",
                group_type: 0,
                tail: Bytes::new(),
                children: vec![
                    ParsedItem::Record(parsed_quest),
                    ParsedItem::Group(ParsedGroup {
                        label: QUEST_FORM_ID.to_le_bytes(),
                        group_type: 10,
                        tail: Bytes::new(),
                        children: vec![ParsedItem::Record(parsed_record("DIAL", DIALOGUE_FORM_ID))],
                    }),
                ],
            })];
        }

        let schema = fo4_schema();
        let interner = StringInterner::new();
        let mut quest = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey {
                local: QUEST_FORM_ID,
                plugin: interner.intern("QuestReplaceTest.esm"),
            },
        );
        let editor_id = interner.intern("QuestReplaceTest");
        quest.eid = Some(editor_id);
        quest.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(editor_id),
        });

        assert!(replace_record_contents_native(handle_id, quest, &schema, &interner).unwrap());

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle_id).unwrap();
        let ParsedItem::Group(quest_top_group) = &slot.parsed.root_items[0] else {
            panic!("expected QUST top group");
        };
        assert!(matches!(
            &quest_top_group.children[..],
            [
                ParsedItem::Record(record),
                ParsedItem::Group(child_group),
            ] if record.form_id == QUEST_FORM_ID
                && child_group.group_type == 10
                && child_group.label == QUEST_FORM_ID.to_le_bytes()
        ));
    }

    #[test]
    fn replace_records_native_replaces_multiple_without_duplicates() {
        let handle_id = match plugin_handle_new_native("BatchReplaceTest.esm", Some("fo4")) {
            Ok(id) => id,
            Err(_) => return,
        };

        let schema = fo4_schema();
        let mut interner = StringInterner::new();
        let sig = SigCode::from_str("WEAP").unwrap();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let fk_a = FormKey::parse("000800@BatchReplaceTest.esm", &mut interner).unwrap();
        let fk_b = FormKey::parse("000801@BatchReplaceTest.esm", &mut interner).unwrap();

        fn weap_record(
            sig: SigCode,
            edid_sig: SubrecordSig,
            fk: FormKey,
            edid: &str,
            interner: &StringInterner,
        ) -> Record {
            let edid_sym = interner.intern(edid);
            Record {
                sig,
                form_key: fk,
                eid: Some(edid_sym),
                flags: RecordFlags::empty(),
                fields: smallvec::smallvec![FieldEntry {
                    sig: edid_sig,
                    value: FieldValue::String(edid_sym),
                }],
                warnings: smallvec::SmallVec::new(),
            }
        }

        add_record_native(
            handle_id,
            weap_record(sig, edid_sig, fk_a, "WeapA1", &mut interner),
            &schema,
            &interner,
        )
        .unwrap();
        add_record_native(
            handle_id,
            weap_record(sig, edid_sig, fk_b, "WeapB1", &mut interner),
            &schema,
            &interner,
        )
        .unwrap();

        replace_records_native(
            handle_id,
            vec![
                weap_record(sig, edid_sig, fk_a, "WeapA2", &mut interner),
                weap_record(sig, edid_sig, fk_b, "WeapB2", &mut interner),
            ],
            &schema,
            &interner,
        )
        .unwrap();

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&handle_id).unwrap();
        let mut edids = Vec::new();
        collect_edids(&slot.parsed.root_items, &mut edids);
        edids.sort();

        assert_eq!(
            edids,
            vec!["WeapA2".to_string(), "WeapB2".to_string()],
            "batch replace should leave only the final replacement records"
        );
    }

    #[test]
    fn structural_batch_error_applies_same_prefix_as_sequential_replaces() {
        let sequential = plugin_handle_new_native("BatchErrorSequential.esm", Some("fo4")).unwrap();
        let batched = plugin_handle_new_native("BatchErrorBatched.esm", Some("fo4")).unwrap();
        let schema = fo4_schema();
        let mut interner = StringInterner::new();

        fn weap_record(
            plugin_name: &str,
            local: u32,
            edid: &str,
            interner: &mut StringInterner,
        ) -> Record {
            let edid_sym = interner.intern(edid);
            Record {
                sig: SigCode::from_str("WEAP").unwrap(),
                form_key: FormKey::parse(&format!("{local:06X}@{plugin_name}"), interner).unwrap(),
                eid: Some(edid_sym),
                flags: RecordFlags::empty(),
                fields: smallvec::smallvec![FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(edid_sym),
                }],
                warnings: SmallVec::new(),
            }
        }

        for (handle, plugin_name) in [
            (sequential, "BatchErrorSequential.esm"),
            (batched, "BatchErrorBatched.esm"),
        ] {
            for (local, edid) in [(0x800, "WeapA1"), (0x801, "WeapB1"), (0x802, "WeapC1")] {
                add_record_native(
                    handle,
                    weap_record(plugin_name, local, edid, &mut interner),
                    &schema,
                    &interner,
                )
                .unwrap();
            }
        }

        let replacements = |plugin_name: &str, interner: &mut StringInterner| {
            vec![
                weap_record(plugin_name, 0x800, "WeapA2", interner),
                weap_record(plugin_name, 0x801, "WeapB2", interner),
                weap_record(plugin_name, 0x802, "WeapC2", interner),
            ]
        };

        let mut store = plugin_handle_store_ref().lock().unwrap();
        let sequential_result = {
            let slot = store.get_mut(&sequential).unwrap();
            let mut result = Ok(());
            for (index, record) in replacements("BatchErrorSequential.esm", &mut interner)
                .into_iter()
                .enumerate()
            {
                if index == 1 {
                    result = Err(WriteError::EncodeFailure("injected failure".to_string()));
                    break;
                }
                if let Some(parsed_record) =
                    encode_record_for_slot(slot, record, &schema, &interner).unwrap()
                {
                    replace_parsed_record_in_slot(slot, parsed_record);
                }
            }
            result
        };
        let batched_result = {
            let slot = store.get_mut(&batched).unwrap();
            let mut index = 0;
            replace_records_in_slot_batch_with_encoder(
                slot,
                replacements("BatchErrorBatched.esm", &mut interner),
                |slot, record| {
                    let current = index;
                    index += 1;
                    if current == 1 {
                        return Err(WriteError::EncodeFailure("injected failure".to_string()));
                    }
                    encode_record_for_slot(slot, record, &schema, &interner)
                },
            )
        };

        assert_eq!(
            sequential_result.unwrap_err().to_string(),
            batched_result.unwrap_err().to_string()
        );
        let mut sequential_edids = Vec::new();
        collect_edids(
            &store.get(&sequential).unwrap().parsed.root_items,
            &mut sequential_edids,
        );
        let mut batched_edids = Vec::new();
        collect_edids(
            &store.get(&batched).unwrap().parsed.root_items,
            &mut batched_edids,
        );
        assert_eq!(sequential_edids, batched_edids);
        assert_eq!(
            batched_edids,
            vec![
                "WeapB1".to_string(),
                "WeapC1".to_string(),
                "WeapA2".to_string(),
            ]
        );
    }

    fn collect_edids(
        items: &[esp_authoring_core::plugin_runtime::ParsedItem],
        out: &mut Vec<String>,
    ) {
        for item in items {
            match item {
                esp_authoring_core::plugin_runtime::ParsedItem::Record(record) => {
                    if record.signature.as_str() != "WEAP" {
                        continue;
                    }
                    if let Some(edid) = record
                        .subrecords
                        .iter()
                        .find(|subrecord| subrecord.signature.as_str() == "EDID")
                    {
                        let text = String::from_utf8_lossy(edid.data.as_ref())
                            .trim_end_matches('\0')
                            .to_string();
                        out.push(text);
                    }
                }
                esp_authoring_core::plugin_runtime::ParsedItem::Group(group) => {
                    collect_edids(&group.children, out);
                }
            }
        }
    }

    fn parsed_record(signature: &str, form_id: u32) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new(signature),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: Vec::new(),
            raw_payload: None,
            parse_error: None,
        }
    }

    fn parsed_subrecord(signature: &str, data: &[u8]) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(signature),
            data: Bytes::copy_from_slice(data),
            semantic_type: None,
        }
    }

    fn portal_snapshot(data: &[u8]) -> NvnmPortalSnapshot {
        let parsed = parse_nvnm(data).expect("NVNM payload");
        portal_snapshot_from_payload(&parsed)
    }

    fn finalized_nvnm(
        form_id: u32,
        data: &[u8],
        snapshots: &HashMap<u32, NvnmPortalSnapshot>,
    ) -> NvnmPayload {
        let (finalized, _stats) =
            finalize_navmesh_links_with_stats(snapshots).expect("NAVM finalization");
        let payload = finalized.get(&form_id).expect("finalized NAVM");
        let rewritten = rewrite_finalized_nvnm_portal_payload(data, payload)
            .expect("rewrite finalized NVNM")
            .unwrap_or_else(|| data.to_vec());
        parse_nvnm(&rewritten).expect("rewritten NVNM")
    }

    fn payload_triangle_cross_z(payload: &NvnmPayload, triangle_index: usize) -> f64 {
        let triangle = payload.triangles[triangle_index];
        let [a, b, c] = triangle.vertices;
        projected_triangle_cross_z_target(
            payload.vertices[a as usize],
            payload.vertices[b as usize],
            payload.vertices[c as usize],
        )
    }

    fn nvnm_mesh(
        parent_world: u32,
        cell: (i16, i16),
        vertices: &[(f32, f32, f32)],
        triangles: &[([u16; 3], [i16; 3], u16)],
        edge_links: &[[u8; 11]],
    ) -> Vec<u8> {
        nvnm_mesh_with_door_refs(parent_world, cell, vertices, triangles, edge_links, &[])
    }

    fn nvnm_mesh_with_door_refs(
        parent_world: u32,
        cell: (i16, i16),
        vertices: &[(f32, f32, f32)],
        triangles: &[([u16; 3], [i16; 3], u16)],
        edge_links: &[[u8; 11]],
        door_refs: &[(i16, u32)],
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&parent_world.to_le_bytes());
        if parent_world == 0 {
            data.extend_from_slice(&0_u32.to_le_bytes());
        } else {
            data.extend_from_slice(&cell.1.to_le_bytes());
            data.extend_from_slice(&cell.0.to_le_bytes());
        }
        data.extend_from_slice(&(vertices.len() as u32).to_le_bytes());
        for &(x, y, z) in vertices {
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
            data.extend_from_slice(&z.to_le_bytes());
        }
        data.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
        for (vertices, links, flags) in triangles {
            for vertex in vertices {
                data.extend_from_slice(&vertex.to_le_bytes());
            }
            for link in *links {
                data.extend_from_slice(&link.to_le_bytes());
            }
            data.extend_from_slice(&0.0_f32.to_le_bytes());
            data.push(0);
            data.extend_from_slice(&flags.to_le_bytes());
            data.extend_from_slice(&0_u16.to_le_bytes());
        }
        data.extend_from_slice(&(edge_links.len() as u32).to_le_bytes());
        for row in edge_links {
            data.extend_from_slice(row);
        }
        data.extend_from_slice(&(door_refs.len() as u32).to_le_bytes());
        for (triangle, door_ref) in door_refs {
            data.extend_from_slice(&triangle.to_le_bytes());
            data.extend_from_slice(&0_u32.to_le_bytes());
            data.extend_from_slice(&door_ref.to_le_bytes());
        }
        // FO4 NVNM trailing region: cover_array (count=0), cover_triangle_mappings
        // (count=0), waypoints (count=0), navmesh_grid (divisor=0 stops here).
        for _ in 0..4 {
            data.extend_from_slice(&0_u32.to_le_bytes());
        }
        data
    }

    fn edge_link_target(row: &NvnmEdgeLink) -> (u32, i16, u8) {
        (
            u32::from_le_bytes(row.row[4..8].try_into().unwrap()),
            i16::from_le_bytes(row.row[8..10].try_into().unwrap()),
            row.row[10],
        )
    }

    fn door_link_refs(data: &[u8]) -> Vec<u32> {
        let parsed = parse_nvnm(data).expect("NVNM payload");
        parsed
            .door_refs
            .iter()
            .map(|d| d.door_ref_form_id)
            .collect()
    }

    #[test]
    fn navmesh_finalizer_rebuilds_missing_reciprocal_portals_from_geometry() {
        let a = nvnm_mesh(
            0,
            (0, 0),
            &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (0.0, 10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let b = nvnm_mesh(
            0,
            (0, 0),
            &[(10.0, 0.0, 0.0), (0.0, 0.0, 0.0), (10.0, -10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&a)), (0x200, portal_snapshot(&b))]);

        let a = finalized_nvnm(0x100, &a, &snapshots);
        let b = finalized_nvnm(0x200, &b, &snapshots);

        assert_eq!(a.edge_links.len(), 1);
        assert_eq!(b.edge_links.len(), 1);
        assert_eq!(edge_link_target(&a.edge_links[0]), (0x200, 0, 0));
        assert_eq!(edge_link_target(&b.edge_links[0]), (0x100, 0, 0));
        assert_eq!(a.triangles[0].links[0], 0);
        assert_eq!(b.triangles[0].links[0], 0);
        assert_ne!(a.triangles[0].flags & nvnm_edge_extra_info_flag(0), 0);
        assert_ne!(b.triangles[0].flags & nvnm_edge_extra_info_flag(0), 0);
    }

    #[test]
    fn navmesh_finalizer_does_not_touch_existing_external_links() {
        let a = nvnm_mesh(
            0,
            (0, 0),
            &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (0.0, 10.0, 0.0)],
            &[([0, 1, 2], [0, -1, -1], nvnm_edge_extra_info_flag(0))],
            &[portal_row_bytes(0x200, 0, 0)],
        );
        let b = nvnm_mesh(
            0,
            (0, 0),
            &[(10.0, 0.0, 0.0), (0.0, 0.0, 0.0), (10.0, -10.0, 0.0)],
            &[([0, 1, 2], [0, -1, -1], nvnm_edge_extra_info_flag(0))],
            &[portal_row_bytes(0x100, 0, 0)],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&a)), (0x200, portal_snapshot(&b))]);

        let (finalized, stats) =
            finalize_navmesh_links_with_stats(&snapshots).expect("NAVM finalization");

        assert_eq!(stats.navmeshes_touched, 0);
        assert!(
            rewrite_finalized_nvnm_portal_payload(&a, finalized.get(&0x100).unwrap())
                .expect("rewrite finalized NVNM")
                .is_none()
        );
        assert!(
            rewrite_finalized_nvnm_portal_payload(&b, finalized.get(&0x200).unwrap())
                .expect("rewrite finalized NVNM")
                .is_none()
        );
    }

    #[test]
    fn navmesh_finalizer_matches_exterior_cell_local_boundary_edges() {
        let a = nvnm_mesh(
            0x800,
            (0, 0),
            &[(2048.0, 0.0, 0.0), (2048.0, 10.0, 0.0), (2000.0, 0.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let b = nvnm_mesh(
            0x800,
            (1, 0),
            &[
                (-2048.0, 10.0, 0.0),
                (-2048.0, 0.0, 0.0),
                (-2000.0, 0.0, 0.0),
            ],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&a)), (0x200, portal_snapshot(&b))]);

        let a = finalized_nvnm(0x100, &a, &snapshots);
        let b = finalized_nvnm(0x200, &b, &snapshots);

        assert_eq!(edge_link_target(&a.edge_links[0]), (0x200, 0, 0));
        assert_eq!(edge_link_target(&b.edge_links[0]), (0x100, 0, 0));
    }

    #[test]
    fn navmesh_finalizer_matches_exterior_worldspace_boundary_edges() {
        let a = nvnm_mesh(
            0x800,
            (-6, -56),
            &[
                (-23040.0, -229376.0, 0.0),
                (-24000.0, -229376.0, 0.0),
                (-23040.0, -229000.0, 0.0),
            ],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let b = nvnm_mesh(
            0x800,
            (-6, -57),
            &[
                (-24000.0, -229376.0, 0.0),
                (-23040.0, -229376.0, 0.0),
                (-24000.0, -229752.0, 0.0),
            ],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&a)), (0x200, portal_snapshot(&b))]);

        let a = finalized_nvnm(0x100, &a, &snapshots);
        let b = finalized_nvnm(0x200, &b, &snapshots);

        assert_eq!(edge_link_target(&a.edge_links[0]), (0x200, 0, 2));
        assert_eq!(edge_link_target(&b.edge_links[0]), (0x100, 0, 2));
    }

    /// Pins the full (finalized, stats) output of
    /// `finalize_navmesh_links_with_stats` on a three-navmesh fixture covering
    /// cross-cell boundary linking AND local diagnose stats, so the
    /// single-solve refactor is provably output-identical.
    #[test]
    fn navmesh_finalizer_stats_and_payload_pinned() {
        // A and B: the cross-cell boundary pair from
        // navmesh_finalizer_matches_exterior_cell_local_boundary_edges.
        let a = nvnm_mesh(
            0x800,
            (0, 0),
            &[(2048.0, 0.0, 0.0), (2048.0, 10.0, 0.0), (2000.0, 0.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let b = nvnm_mesh(
            0x800,
            (1, 0),
            &[
                (-2048.0, 10.0, 0.0),
                (-2048.0, 0.0, 0.0),
                (-2000.0, 0.0, 0.0),
            ],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        // C: two opposite-wound triangles sharing an unlinked edge in the same
        // navmesh -> exercises the missing-internal-links diagnose path.
        let c = nvnm_mesh(
            0x800,
            (0, 1),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (10.0, 10.0, 0.0),
            ],
            &[([0, 1, 2], [-1, -1, -1], 0), ([2, 1, 3], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([
            (0x100, portal_snapshot(&a)),
            (0x200, portal_snapshot(&b)),
            (0x300, portal_snapshot(&c)),
        ]);

        let (finalized, stats) =
            finalize_navmesh_links_with_stats(&snapshots).expect("NAVM finalization");

        let mut digest = String::new();
        let mut form_ids: Vec<u32> = finalized.keys().copied().collect();
        form_ids.sort_unstable();
        for form_id in form_ids {
            let payload = &finalized[&form_id];
            digest.push_str(&format!(
                "{form_id:06X} tris={:?} edges={:02X?}\n",
                payload.triangles, payload.edge_rows
            ));
        }

        assert_eq!(
            stats,
            NavmeshFinalizeStats {
                navmeshes_seen: 3,
                navmeshes_touched: 3,
                missing_internal_links: 2,
                external_links_added: 2,
                missing_external_links: 2,
                ..NavmeshFinalizeStats::default()
            },
            "pinned stats changed"
        );
        let expected_digest = "\
000100 tris=[NvnmFinalizedTriangle { vertices: [0, 1, 2], links: [0, -1, -1], flags: 1 }] edges=[[00, 00, 00, 00, 00, 02, 00, 00, 00, 00, 00]]\n\
000200 tris=[NvnmFinalizedTriangle { vertices: [0, 1, 2], links: [0, -1, -1], flags: 1 }] edges=[[00, 00, 00, 00, 00, 01, 00, 00, 00, 00, 00]]\n\
000300 tris=[NvnmFinalizedTriangle { vertices: [0, 1, 2], links: [-1, 1, -1], flags: 0 }, NvnmFinalizedTriangle { vertices: [2, 1, 3], links: [0, -1, -1], flags: 0 }] edges=[]\n";
        assert_eq!(digest, expected_digest, "pinned finalized payload changed");
    }

    #[test]
    fn navmesh_finalizer_leaves_same_direction_overlap_edge_unlinked() {
        // Two already-upfacing triangles that share edge {0,1} traversed in the
        // SAME 2D-projected direction (both 0->1). In a real navmesh every
        // walkable face is ~horizontal, so a same-direction shared edge means
        // the two faces OVERLAP in projection (FO76 zigzag/sliver fans), not a
        // legitimate steep fold. CK's manifold validator rejects linking such a
        // pair — it logs "opposite normals but linked / edges should be linked
        // but are not / vertices do not match". So we keep BOTH upfacing (no
        // winding flip) but leave the overlap edge UNLINKED.
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (2.0, 10.0, 100.0),
            ],
            &[([0, 1, 2], [-1, -1, -1], 0), ([0, 1, 3], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&mesh))]);

        let parsed = finalized_nvnm(0x100, &mesh, &snapshots);

        // Both triangles keep their upward projected normal (no winding flip).
        assert!(
            payload_triangle_cross_z(&parsed, 0) > 0.0,
            "triangle 0 must stay upfacing"
        );
        assert!(
            payload_triangle_cross_z(&parsed, 1) > 0.0,
            "triangle 1 must stay upfacing"
        );
        // ...and the same-direction overlap edge is NOT linked.
        assert!(!parsed.triangles[0].links.contains(&1));
        assert!(!parsed.triangles[1].links.contains(&0));
        assert_eq!(parsed.triangles[0].flags & 0x0007, 0);
        assert_eq!(parsed.triangles[1].flags & 0x0007, 0);
        assert!(parsed.edge_links.is_empty());
    }

    #[test]
    fn navmesh_finalizer_preserves_upward_projected_normals_after_winding_solution() {
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (10.0, 10.0, 0.0),
            ],
            &[([0, 2, 1], [-1, -1, -1], 0), ([1, 3, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&mesh))]);

        let parsed = finalized_nvnm(0x100, &mesh, &snapshots);

        assert!(payload_triangle_cross_z(&parsed, 0) > 0.0);
        assert!(payload_triangle_cross_z(&parsed, 1) > 0.0);
        assert!(parsed.triangles[0].links.contains(&1));
        assert!(parsed.triangles[1].links.contains(&0));
    }

    #[test]
    fn navmesh_finalizer_flips_fo76_07390d37_triangle_234_shape() {
        let mesh = nvnm_mesh(
            0x0725DA15,
            (5, -21),
            &[
                (21504.0, -86016.0, 29961.046875),
                (21696.0, -85752.0, 29897.67578125),
                (21768.0, -86016.0, 29985.236328125),
                (21504.0, -85752.0, 29887.55859375),
            ],
            &[([1, 0, 3], [-1, -1, -1], 0), ([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x07390D37, portal_snapshot(&mesh))]);

        let parsed = finalized_nvnm(0x07390D37, &mesh, &snapshots);

        assert!(payload_triangle_cross_z(&parsed, 0) > 0.0);
        assert!(payload_triangle_cross_z(&parsed, 1) > 0.0);
        assert!(parsed.triangles[0].links.contains(&1));
        assert!(parsed.triangles[1].links.contains(&0));
    }

    // Two adjacent downfacing triangles sharing an edge in opposite
    // directions (no solver flips needed) must end up BOTH upfacing AND
    // linked, with adjacency preserved — a per-triangle-only flip that
    // doesn't rotate the shared-edge slot breaks adjacency.
    #[test]
    fn navmesh_finalizer_force_up_preserves_adjacency_for_downfacing_pair() {
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (10.0, 10.0, 0.0),
            ],
            &[([0, 2, 1], [-1, -1, -1], 0), ([1, 2, 3], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&mesh))]);

        let parsed = finalized_nvnm(0x100, &mesh, &snapshots);

        assert!(payload_triangle_cross_z(&parsed, 0) > 0.0);
        assert!(payload_triangle_cross_z(&parsed, 1) > 0.0);
        assert!(parsed.triangles[0].links.contains(&1));
        assert!(parsed.triangles[1].links.contains(&0));
    }

    #[test]
    fn navmesh_winding_solver_counts_conflicting_shared_edges() {
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (10.0, 10.0, 0.0),
                (0.0, 20.0, 0.0),
                (10.0, 20.0, 0.0),
                (100.0, 0.0, 0.0),
                (110.0, 0.0, 0.0),
                (100.0, 10.0, 0.0),
                (110.0, 10.0, 0.0),
            ],
            &[
                ([0, 1, 2], [-1, -1, -1], 0),
                ([1, 3, 2], [-1, -1, -1], 0),
                ([2, 3, 4], [-1, -1, -1], 0),
                ([3, 5, 4], [-1, -1, -1], 0),
                ([4, 5, 1], [-1, -1, -1], 0),
                ([5, 0, 1], [-1, -1, -1], 0),
                ([6, 7, 8], [-1, -1, -1], 0),
                ([6, 7, 9], [-1, -1, -1], 0),
                ([7, 6, 9], [-1, -1, -1], 0),
            ],
            &[],
        );
        let snapshots = HashMap::from([(0x900, portal_snapshot(&mesh))]);

        let (_finalized, stats) =
            finalize_navmesh_links_with_stats(&snapshots).expect("NAVM finalization");

        assert!(stats.winding_conflicts > 0);
        assert!(stats.ambiguous_local_edges > 0);
    }

    #[test]
    fn navmesh_finalizer_clears_stale_extra_info_flags() {
        let a = nvnm_mesh(
            0,
            (0, 0),
            &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (0.0, 10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], nvnm_edge_extra_info_flag(2))],
            &[],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&a))]);

        let parsed = finalized_nvnm(0x100, &a, &snapshots);

        assert_eq!(parsed.triangles[0].links, [-1, -1, -1]);
        assert_eq!(parsed.triangles[0].flags & 0x0007, 0);
        assert!(parsed.edge_links.is_empty());
    }

    #[test]
    fn navmesh_finalizer_drops_missing_own_plugin_door_portals() {
        let mesh = nvnm_mesh_with_door_refs(
            0,
            (0, 0),
            &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (0.0, 10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
            &[(0, 0x01000100), (0, 0x01000200), (0, 0x00000300)],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&mesh))]);
        let (finalized, _stats) =
            finalize_navmesh_links_with_stats(&snapshots).expect("NAVM finalization");
        let valid_door_refs = HashSet::from([0x01000100]);

        let rewritten = rewrite_finalized_nvnm_portal_payload_with_door_refs(
            &mesh,
            finalized.get(&0x100).expect("finalized NAVM"),
            1,
            &valid_door_refs,
        )
        .expect("rewrite finalized NVNM")
        .expect("door portal filter should rewrite");

        assert_eq!(door_link_refs(&rewritten), vec![0x01000100, 0x00000300]);
    }

    #[test]
    fn navmesh_diagnostics_detect_bad_internal_link_target() {
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (100.0, 100.0, 0.0),
                (110.0, 100.0, 0.0),
                (100.0, 110.0, 0.0),
            ],
            &[([0, 1, 2], [1, -1, -1], 0), ([3, 4, 5], [0, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x762D933, portal_snapshot(&mesh))]);

        let stats = diagnose_navmesh_links(&snapshots).expect("NAVM diagnostics");

        assert_eq!(stats.navmeshes_seen, 1);
        assert_eq!(stats.bad_internal_links, 2);
        assert_eq!(stats.linked_edge_vertex_mismatches, 2);
        assert_eq!(stats.missing_internal_links, 0);
    }

    #[test]
    fn navmesh_diagnostics_detect_missing_internal_shared_edge_link() {
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (10.0, 10.0, 0.0),
            ],
            &[([0, 1, 2], [-1, -1, -1], 0), ([1, 3, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x742F713, portal_snapshot(&mesh))]);

        let stats = diagnose_navmesh_links(&snapshots).expect("NAVM diagnostics");

        assert_eq!(stats.bad_internal_links, 0);
        assert_eq!(stats.missing_internal_links, 2);
        assert_eq!(stats.same_direction_internal_edges, 0);
    }

    #[test]
    fn navmesh_diagnostics_treat_extra_info_links_as_missing_internal_links() {
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (10.0, 10.0, 0.0),
            ],
            &[
                ([0, 1, 2], [-1, 1, -1], nvnm_edge_extra_info_flag(1)),
                ([1, 3, 2], [-1, -1, 0], nvnm_edge_extra_info_flag(2)),
            ],
            &[[0u8; 11], [0u8; 11]],
        );
        let snapshots = HashMap::from([(0x742F713, portal_snapshot(&mesh))]);

        let stats = diagnose_navmesh_links(&snapshots).expect("NAVM diagnostics");

        assert_eq!(stats.bad_internal_links, 0);
        assert_eq!(stats.missing_internal_links, 2);
        assert_eq!(stats.same_direction_internal_edges, 0);
    }

    #[test]
    fn navmesh_diagnostics_use_finalized_winding_for_external_edges() {
        let a = nvnm_mesh(
            0,
            (0, 0),
            &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (0.0, 10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let b = nvnm_mesh(
            0,
            (0, 0),
            &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (10.0, -10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x100, portal_snapshot(&a)), (0x200, portal_snapshot(&b))]);

        let stats = diagnose_navmesh_links(&snapshots).expect("NAVM diagnostics");

        assert_eq!(stats.missing_external_links, 2);
        assert_eq!(stats.ambiguous_external_edges, 0);
    }

    #[test]
    fn navmesh_finalizer_reports_cleared_bad_internal_links() {
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (100.0, 100.0, 0.0),
                (110.0, 100.0, 0.0),
                (100.0, 110.0, 0.0),
            ],
            &[([0, 1, 2], [1, -1, -1], 0), ([3, 4, 5], [0, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x762D933, portal_snapshot(&mesh))]);

        let (finalized, stats) =
            finalize_navmesh_links_with_stats(&snapshots).expect("NAVM finalization");
        let payload = finalized.get(&0x762D933).expect("finalized payload");

        assert_eq!(stats.bad_internal_links, 2);
        assert_eq!(stats.linked_edge_vertex_mismatches, 2);
        assert_eq!(stats.navmeshes_touched, 1);
        assert_eq!(payload.triangles[0].links, [-1, -1, -1]);
        assert_eq!(payload.triangles[1].links, [-1, -1, -1]);
    }

    #[test]
    fn navmesh_finalizer_reports_rebuilt_missing_internal_links() {
        let mesh = nvnm_mesh(
            0,
            (0, 0),
            &[
                (0.0, 0.0, 0.0),
                (10.0, 0.0, 0.0),
                (0.0, 10.0, 0.0),
                (10.0, 10.0, 0.0),
            ],
            &[([0, 1, 2], [-1, -1, -1], 0), ([1, 3, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([(0x742F713, portal_snapshot(&mesh))]);

        let (finalized, stats) =
            finalize_navmesh_links_with_stats(&snapshots).expect("NAVM finalization");
        let payload = finalized.get(&0x742F713).expect("finalized payload");

        assert_eq!(stats.missing_internal_links, 2);
        assert_eq!(stats.navmeshes_touched, 1);
        assert!(payload.triangles[0].links.iter().any(|link| *link == 1));
        assert!(payload.triangles[1].links.iter().any(|link| *link == 0));
    }

    #[test]
    fn navmesh_finalizer_reports_ambiguous_external_edges_once() {
        let a = nvnm_mesh(
            0,
            (0, 0),
            &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (0.0, 10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let b = nvnm_mesh(
            0,
            (0, 0),
            &[(10.0, 0.0, 0.0), (0.0, 0.0, 0.0), (10.0, -10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let c = nvnm_mesh(
            0,
            (0, 0),
            &[(0.0, 0.0, 0.0), (10.0, 0.0, 0.0), (5.0, 10.0, 0.0)],
            &[([0, 1, 2], [-1, -1, -1], 0)],
            &[],
        );
        let snapshots = HashMap::from([
            (0x100, portal_snapshot(&a)),
            (0x200, portal_snapshot(&b)),
            (0x300, portal_snapshot(&c)),
        ]);

        let diagnostic_stats = diagnose_navmesh_links(&snapshots).expect("NAVM diagnostics");
        let (_finalized, finalizer_stats) =
            finalize_navmesh_links_with_stats(&snapshots).expect("NAVM finalization");

        assert_eq!(diagnostic_stats.ambiguous_external_edges, 3);
        assert_eq!(
            finalizer_stats.ambiguous_external_edges,
            diagnostic_stats.ambiguous_external_edges
        );
    }

    #[test]
    fn navmesh_finalizer_caps_external_edge_rows_to_fo4_limit() {
        let mut finalized = HashMap::from([
            (
                0x100,
                NvnmFinalizedPayload {
                    triangles: vec![NvnmFinalizedTriangle {
                        vertices: [0, 1, 2],
                        links: [-1, -1, -1],
                        flags: 0,
                    }],
                    edge_rows: vec![[0u8; 11]; NVNM_MAX_FO4_EDGE_ROWS],
                },
            ),
            (
                0x200,
                NvnmFinalizedPayload {
                    triangles: vec![NvnmFinalizedTriangle {
                        vertices: [0, 1, 2],
                        links: [-1, -1, -1],
                        flags: 0,
                    }],
                    edge_rows: Vec::new(),
                },
            ),
        ]);

        add_external_final_link_pair(
            &mut finalized,
            NvnmFinalEdgeRef {
                form_id: 0x100,
                triangle: 0,
                slot: 0,
                oriented: [
                    NvnmEdgePoint { x: 0, y: 0, z: 0 },
                    NvnmEdgePoint { x: 1, y: 0, z: 0 },
                ],
            },
            NvnmFinalEdgeRef {
                form_id: 0x200,
                triangle: 0,
                slot: 0,
                oriented: [
                    NvnmEdgePoint { x: 1, y: 0, z: 0 },
                    NvnmEdgePoint { x: 0, y: 0, z: 0 },
                ],
            },
        )
        .expect("capped portal add");

        assert_eq!(
            finalized.get(&0x100).unwrap().edge_rows.len(),
            NVNM_MAX_FO4_EDGE_ROWS
        );
        assert_eq!(finalized.get(&0x200).unwrap().edge_rows.len(), 0);
        assert_eq!(finalized.get(&0x100).unwrap().triangles[0].links[0], -1);
        assert_eq!(finalized.get(&0x200).unwrap().triangles[0].links[0], -1);
    }

    fn finalized_from_source(source: &NvnmPayload) -> NvnmFinalizedPayload {
        let triangles = source
            .triangles
            .iter()
            .map(|t| NvnmFinalizedTriangle {
                vertices: t.vertices,
                links: t.links,
                flags: t.flags,
            })
            .collect();
        let edge_rows = source.edge_links.iter().map(|el| el.row).collect();
        NvnmFinalizedPayload {
            triangles,
            edge_rows,
        }
    }

    fn make_test_source_payload_with_triangles(
        triangle_vertex_sets: &[[u16; 3]],
        waypoints: Vec<esp_authoring_core::nvnm::NvnmWaypoint>,
        cover_array: Vec<esp_authoring_core::nvnm::NvnmCoverEntry>,
        cover_mappings: Vec<esp_authoring_core::nvnm::NvnmCoverTriangleMapping>,
    ) -> NvnmPayload {
        NvnmPayload {
            version: 15,
            flags: 0,
            parent: NvnmParent::Interior { cell: 0x100 },
            vertices: vec![],
            triangles: triangle_vertex_sets
                .iter()
                .map(|v| NvnmTriangle {
                    vertices: *v,
                    links: [-1; 3],
                    cover_marker: [0; 9],
                    flags: 0,
                })
                .collect(),
            edge_links: vec![],
            door_refs: vec![],
            cover_array,
            cover_triangle_mappings: cover_mappings,
            waypoints,
            grid: esp_authoring_core::nvnm::NvnmGrid::default(),
        }
    }

    #[test]
    fn regenerate_nvnm_trailing_waypoint_carryover_drops_pruned_anchor() {
        // Source has 3 triangles + 3 waypoints anchored to indices 0,1,2.
        // Finalized drops triangle 1, keeps 0 and 2 (now at finalized indices
        // 0 and 1). Surviving waypoints must point at the new indices.
        use esp_authoring_core::nvnm::NvnmWaypoint;
        let source = make_test_source_payload_with_triangles(
            &[[0, 1, 2], [3, 4, 5], [6, 7, 8]],
            vec![
                NvnmWaypoint {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    triangle: 0,
                    flags: 0xA,
                },
                NvnmWaypoint {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                    triangle: 1,
                    flags: 0xB,
                },
                NvnmWaypoint {
                    x: 2.0,
                    y: 0.0,
                    z: 0.0,
                    triangle: 2,
                    flags: 0xC,
                },
            ],
            vec![],
            vec![],
        );
        let finalized = NvnmFinalizedPayload {
            triangles: vec![
                NvnmFinalizedTriangle {
                    vertices: [0, 1, 2],
                    links: [-1; 3],
                    flags: 0,
                },
                NvnmFinalizedTriangle {
                    vertices: [6, 7, 8],
                    links: [-1; 3],
                    flags: 0,
                },
            ],
            edge_rows: vec![],
        };

        let regen = regenerate_nvnm_trailing(&finalized, &source);

        assert_eq!(regen.waypoints.len(), 2, "pruned waypoint not dropped");
        assert_eq!(regen.waypoints[0].triangle, 0, "anchor 0 stays at 0");
        assert_eq!(regen.waypoints[0].flags, 0xA);
        assert_eq!(regen.waypoints[1].triangle, 1, "anchor 2 remapped to 1");
        assert_eq!(regen.waypoints[1].flags, 0xC);
    }

    #[test]
    fn regenerate_nvnm_trailing_cover_carryover_drops_pruned_covers() {
        // Source has 3 triangles and 3 covers, each with one mapping. Finalize
        // drops triangle index 1: cover 1's mapping points to a pruned
        // triangle, so cover 1 loses all mappings and is itself dropped.
        // Surviving covers 0 and 2 are renumbered to 0 and 1; their mappings'
        // triangle indices are remapped through the source->finalized index
        // map (triangle 2 -> finalized 1).
        use esp_authoring_core::nvnm::{NvnmCoverEntry, NvnmCoverTriangleMapping};
        let cover_a = NvnmCoverEntry {
            vertex_1: 0,
            vertex_2: 1,
            data_byte_1: 0xAA,
            data_byte_2: 0,
            data_byte_3: 0,
            data_byte_4: 0,
        };
        let cover_b = NvnmCoverEntry {
            vertex_1: 2,
            vertex_2: 3,
            data_byte_1: 0xBB,
            data_byte_2: 0,
            data_byte_3: 0,
            data_byte_4: 0,
        };
        let cover_c = NvnmCoverEntry {
            vertex_1: 4,
            vertex_2: 5,
            data_byte_1: 0xCC,
            data_byte_2: 0,
            data_byte_3: 0,
            data_byte_4: 0,
        };
        let source = make_test_source_payload_with_triangles(
            &[[0, 1, 2], [3, 4, 5], [6, 7, 8]],
            vec![],
            vec![cover_a, cover_b, cover_c],
            vec![
                NvnmCoverTriangleMapping {
                    cover: 0,
                    triangle: 0,
                },
                NvnmCoverTriangleMapping {
                    cover: 1,
                    triangle: 1,
                },
                NvnmCoverTriangleMapping {
                    cover: 2,
                    triangle: 2,
                },
            ],
        );
        let finalized = NvnmFinalizedPayload {
            triangles: vec![
                NvnmFinalizedTriangle {
                    vertices: [0, 1, 2],
                    links: [-1; 3],
                    flags: 0,
                },
                NvnmFinalizedTriangle {
                    vertices: [6, 7, 8],
                    links: [-1; 3],
                    flags: 0,
                },
            ],
            edge_rows: vec![],
        };

        let regen = regenerate_nvnm_trailing(&finalized, &source);

        assert_eq!(regen.cover_array.len(), 2, "orphan cover not dropped");
        assert_eq!(regen.cover_array[0].data_byte_1, 0xAA);
        assert_eq!(regen.cover_array[1].data_byte_1, 0xCC);
        assert_eq!(regen.cover_triangle_mappings.len(), 2);
        assert_eq!(regen.cover_triangle_mappings[0].cover, 0);
        assert_eq!(regen.cover_triangle_mappings[0].triangle, 0);
        assert_eq!(
            regen.cover_triangle_mappings[1].cover, 1,
            "cover renumbered"
        );
        assert_eq!(
            regen.cover_triangle_mappings[1].triangle, 1,
            "triangle index remapped"
        );
    }

    #[test]
    fn regenerate_nvnm_trailing_cover_mapping_triangle_remapped_on_swap() {
        // The `cover_triangle_mappings.triangle` field references source
        // triangle indices and MUST be remapped through the source->finalized
        // index map alongside waypoints and door_refs. CK uses an opaque
        // internal order for the mappings list, so compare as a multiset;
        // this test verifies the remap is correct when finalize swaps
        // triangle positions.
        use esp_authoring_core::nvnm::{NvnmCoverEntry, NvnmCoverTriangleMapping};
        let make_cover = |b1: u8| NvnmCoverEntry {
            vertex_1: 0,
            vertex_2: 1,
            data_byte_1: b1,
            data_byte_2: 0,
            data_byte_3: 0,
            data_byte_4: 0,
        };
        // Three source triangles with distinct vertex-sets. Each cover
        // points to a different triangle. Finalize REORDERS the triangles:
        // src 0 -> fin 2, src 1 -> fin 0, src 2 -> fin 1 (a 3-cycle).
        let source = make_test_source_payload_with_triangles(
            &[[0, 1, 2], [3, 4, 5], [6, 7, 8]],
            vec![],
            vec![make_cover(0xAA), make_cover(0xBB), make_cover(0xCC)],
            vec![
                NvnmCoverTriangleMapping {
                    cover: 0,
                    triangle: 0,
                },
                NvnmCoverTriangleMapping {
                    cover: 1,
                    triangle: 1,
                },
                NvnmCoverTriangleMapping {
                    cover: 2,
                    triangle: 2,
                },
            ],
        );
        let finalized = NvnmFinalizedPayload {
            triangles: vec![
                NvnmFinalizedTriangle {
                    vertices: [3, 4, 5],
                    links: [-1; 3],
                    flags: 0,
                },
                NvnmFinalizedTriangle {
                    vertices: [6, 7, 8],
                    links: [-1; 3],
                    flags: 0,
                },
                NvnmFinalizedTriangle {
                    vertices: [0, 1, 2],
                    links: [-1; 3],
                    flags: 0,
                },
            ],
            edge_rows: vec![],
        };
        let regen = regenerate_nvnm_trailing(&finalized, &source);
        // The mapping set must be:
        //   cover 0 -> triangle 2 (src 0 -> fin 2)
        //   cover 1 -> triangle 0 (src 1 -> fin 0)
        //   cover 2 -> triangle 1 (src 2 -> fin 1)
        // Compare as a multiset because we don't guarantee any specific
        // emission order — see investigation note above.
        let regen_set: std::collections::HashSet<(u16, i16)> = regen
            .cover_triangle_mappings
            .iter()
            .map(|m| (m.cover, m.triangle))
            .collect();
        let expected: std::collections::HashSet<(u16, i16)> =
            [(0u16, 2i16), (1u16, 0i16), (2u16, 1i16)]
                .into_iter()
                .collect();
        assert_eq!(
            regen_set, expected,
            "cover_triangle_mappings must reference FINALIZED triangle indices (src->fin remap), \
             not source indices"
        );
    }

    #[test]
    fn regenerate_nvnm_trailing_door_ref_carryover_drops_pruned_anchor() {
        // Source has 3 triangles + 3 door_refs anchored to indices 0,1,2.
        // Finalize drops triangle 1: that door_ref's anchor is pruned and
        // must be dropped. The third door_ref's anchor remaps from triangle 2
        // to finalized index 1. form_id-based filtering is a separate concern
        // (caller layer), so all door_refs here use distinct nonzero form_ids
        // that survive the anchor-only carryover.
        let mut source = make_test_source_payload_with_triangles(
            &[[0, 1, 2], [3, 4, 5], [6, 7, 8]],
            vec![],
            vec![],
            vec![],
        );
        source.door_refs = vec![
            NvnmDoorRef {
                triangle_index: 0,
                padding: [0; 4],
                door_ref_form_id: 0x01000001,
            },
            NvnmDoorRef {
                triangle_index: 1,
                padding: [0; 4],
                door_ref_form_id: 0x01000002,
            },
            NvnmDoorRef {
                triangle_index: 2,
                padding: [0; 4],
                door_ref_form_id: 0x01000003,
            },
        ];
        let finalized = NvnmFinalizedPayload {
            triangles: vec![
                NvnmFinalizedTriangle {
                    vertices: [0, 1, 2],
                    links: [-1; 3],
                    flags: 0,
                },
                NvnmFinalizedTriangle {
                    vertices: [6, 7, 8],
                    links: [-1; 3],
                    flags: 0,
                },
            ],
            edge_rows: vec![],
        };

        let regen = regenerate_nvnm_trailing(&finalized, &source);

        assert_eq!(regen.door_refs.len(), 2, "orphan door_ref not dropped");
        assert_eq!(regen.door_refs[0].triangle_index, 0);
        assert_eq!(regen.door_refs[0].door_ref_form_id, 0x01000001);
        assert_eq!(
            regen.door_refs[1].triangle_index, 1,
            "anchor 2 remapped to 1"
        );
        assert_eq!(regen.door_refs[1].door_ref_form_id, 0x01000003);
    }

    #[test]
    fn build_finalized_triangle_index_map_drops_degenerate_triangles() {
        // Two source triangles share the SAME degenerate vertex set [5,5,7].
        // Finalize "swaps" them (kept in same order, but they're indistinguishable
        // because both are degenerate). Without the fix, the consume-cursor
        // approach would happily map src[0]->fin[0] and src[1]->fin[1] — but
        // degenerate triangles are CK-invalid pathing surfaces, so the *safe*
        // behaviour is to drop them: every source degenerate maps to None.
        let source = vec![
            NvnmTriangle {
                vertices: [5, 5, 7],
                links: [-1; 3],
                cover_marker: [0; 9],
                flags: 0,
            },
            NvnmTriangle {
                vertices: [5, 5, 7],
                links: [-1; 3],
                cover_marker: [0; 9],
                flags: 0,
            },
        ];
        let finalized = vec![
            NvnmFinalizedTriangle {
                vertices: [5, 5, 7],
                links: [-1; 3],
                flags: 0,
            },
            NvnmFinalizedTriangle {
                vertices: [5, 5, 7],
                links: [-1; 3],
                flags: 0,
            },
        ];
        let map = build_finalized_triangle_index_map(&source, &finalized);
        assert_eq!(map.len(), 2);
        assert_eq!(map[0], None, "degenerate source must map to None");
        assert_eq!(map[1], None, "degenerate source must map to None");
    }

    #[test]
    fn regenerate_nvnm_trailing_drops_carryover_for_degenerate_source_triangles() {
        // Source triangles: [normal, degenerate, normal]. Finalize keeps the
        // same vertices. Cover mapping pointing at the degenerate triangle
        // (index 1) must be dropped (its anchor is invalid); the cover
        // mapping pointing at the second normal triangle (index 2) must
        // survive and remap to finalized index 2. With the bug, two
        // degenerates sharing a vertex set could shuffle finalized indices
        // between cover/waypoint/door anchors at random.
        use esp_authoring_core::nvnm::{NvnmCoverEntry, NvnmCoverTriangleMapping};
        let cover_a = NvnmCoverEntry {
            vertex_1: 0,
            vertex_2: 1,
            data_byte_1: 0xAA,
            data_byte_2: 0,
            data_byte_3: 0,
            data_byte_4: 0,
        };
        let cover_b = NvnmCoverEntry {
            vertex_1: 2,
            vertex_2: 3,
            data_byte_1: 0xBB,
            data_byte_2: 0,
            data_byte_3: 0,
            data_byte_4: 0,
        };
        let source = make_test_source_payload_with_triangles(
            &[[0, 1, 2], [5, 5, 7], [3, 4, 5]],
            vec![],
            vec![cover_a, cover_b],
            vec![
                NvnmCoverTriangleMapping {
                    cover: 0,
                    triangle: 1,
                }, // anchored to degenerate
                NvnmCoverTriangleMapping {
                    cover: 1,
                    triangle: 2,
                },
            ],
        );
        let finalized = NvnmFinalizedPayload {
            triangles: vec![
                NvnmFinalizedTriangle {
                    vertices: [0, 1, 2],
                    links: [-1; 3],
                    flags: 0,
                },
                NvnmFinalizedTriangle {
                    vertices: [5, 5, 7],
                    links: [-1; 3],
                    flags: 0,
                },
                NvnmFinalizedTriangle {
                    vertices: [3, 4, 5],
                    links: [-1; 3],
                    flags: 0,
                },
            ],
            edge_rows: vec![],
        };

        let regen = regenerate_nvnm_trailing(&finalized, &source);

        // Cover anchored to the degenerate triangle is dropped (cover 0 loses
        // all mappings → dropped). Cover 1's mapping survives, remapped to
        // finalized index 2 (which was always 2 — identity for normal tris).
        assert_eq!(regen.cover_triangle_mappings.len(), 1);
        assert_eq!(
            regen.cover_triangle_mappings[0].cover, 0,
            "cover renumbered"
        );
        assert_eq!(regen.cover_triangle_mappings[0].triangle, 2);
        assert_eq!(regen.cover_array.len(), 1, "cover with no mappings dropped");
        assert_eq!(regen.cover_array[0].data_byte_1, 0xBB);
    }

    #[test]
    fn regenerate_nvnm_trailing_identity_finalize_preserves_non_grid_sections() {
        // Identity finalize (no triangle pruning, no winding flips) must
        // preserve waypoints / cover_array / cover_triangle_mappings byte-for-
        // byte. The navmesh_grid is INTENTIONALLY rebuilt (FO4 CK rebuilds it
        // at Finalize using a wider spatial-bucketing rule than FO76's
        // single-cell-per-triangle bucket), so we don't assert grid equality
        // here — see the `rebuild_nvnm_grid_fo4_matches_ck_*` fixture tests
        // for the rebuild contract.
        let source_bytes = include_bytes!(
            "../../../../../py_creation_lib/native/esp/src/nvnm/tests/fixtures/4ea534_fo4.nvnm.bin"
        );
        let source = parse_nvnm(source_bytes).expect("parse source NVNM");
        let finalized = finalized_from_source(&source);
        let regenerated = regenerate_nvnm_trailing(&finalized, &source);
        assert_eq!(
            regenerated.waypoints, source.waypoints,
            "waypoints mismatch"
        );
        assert_eq!(
            regenerated.cover_array, source.cover_array,
            "cover_array mismatch"
        );
        assert_eq!(
            regenerated.cover_triangle_mappings, source.cover_triangle_mappings,
            "cover_triangle_mappings mismatch"
        );
        // The grid divisor / bounds / grid_size are preserved even though
        // cells are regenerated.
        assert_eq!(regenerated.grid.divisor, source.grid.divisor);
        assert_eq!(regenerated.grid.grid_size_x, source.grid.grid_size_x);
        assert_eq!(regenerated.grid.grid_size_y, source.grid.grid_size_y);
        assert_eq!(regenerated.grid.bounds_min_x, source.grid.bounds_min_x);
        assert_eq!(regenerated.grid.bounds_min_y, source.grid.bounds_min_y);
        assert_eq!(regenerated.grid.bounds_max_x, source.grid.bounds_max_x);
        assert_eq!(regenerated.grid.bounds_max_y, source.grid.bounds_max_y);
        assert_eq!(
            regenerated.grid.cells.len(),
            source.grid.cells.len(),
            "cell count must match divisor² unchanged"
        );
    }

    /// Helper: assert the rebuild includes every triangle CK has in each
    /// cell. The rebuild may legitimately produce a superset of CK's grid
    /// entries (AABB-rule is wider than CK's exact spatial rule); the
    /// PATHFINDING warning is triggered by UNDER-inclusion, so coverage is
    /// the safety-critical invariant.
    fn assert_rebuild_covers_ck(src_bytes: &[u8], ck_bytes: &[u8], label: &str) {
        let source = parse_nvnm(src_bytes).expect("parse src NVNM");
        let ck = parse_nvnm(ck_bytes).expect("parse ck NVNM");
        let finalized: Vec<NvnmFinalizedTriangle> = source
            .triangles
            .iter()
            .map(|t| NvnmFinalizedTriangle {
                vertices: t.vertices,
                links: t.links,
                flags: t.flags,
            })
            .collect();
        let rebuilt = rebuild_nvnm_grid_fo4(&source.grid, &source.vertices, &finalized);
        for (ci, (r, c)) in rebuilt.cells.iter().zip(&ck.grid.cells).enumerate() {
            let r_set: std::collections::HashSet<i16> =
                r.triangle_indices.iter().copied().collect();
            let missing: Vec<i16> = c
                .triangle_indices
                .iter()
                .copied()
                .filter(|i| *i >= 0 && !r_set.contains(i))
                .collect();
            assert!(
                missing.is_empty(),
                "{label}: cell[{ci}] rebuild missing {} CK triangles: {:?}",
                missing.len(),
                missing
            );
        }
    }

    #[test]
    fn rebuild_nvnm_grid_fo4_covers_ck_4ea53d() {
        let src = include_bytes!("test_fixtures/nvnm_grid_ck/src_4EA53D.nvnm.bin");
        let ck = include_bytes!("test_fixtures/nvnm_grid_ck/ck_4EA53D.nvnm.bin");
        assert_rebuild_covers_ck(src, ck, "4EA53D");
    }

    #[test]
    fn rebuild_nvnm_grid_fo4_covers_ck_4ea542() {
        let src = include_bytes!("test_fixtures/nvnm_grid_ck/src_4EA542.nvnm.bin");
        let ck = include_bytes!("test_fixtures/nvnm_grid_ck/ck_4EA542.nvnm.bin");
        assert_rebuild_covers_ck(src, ck, "4EA542");
    }

    #[test]
    fn rebuild_nvnm_grid_fo4_covers_ck_4ea534() {
        let src = include_bytes!("test_fixtures/nvnm_grid_ck/src_4EA534.nvnm.bin");
        let ck = include_bytes!("test_fixtures/nvnm_grid_ck/ck_4EA534.nvnm.bin");
        assert_rebuild_covers_ck(src, ck, "4EA534");
    }

    #[test]
    fn rebuild_nvnm_grid_fo4_covers_ck_2b740a() {
        let src = include_bytes!("test_fixtures/nvnm_grid_ck/src_2B740A.nvnm.bin");
        let ck = include_bytes!("test_fixtures/nvnm_grid_ck/ck_2B740A.nvnm.bin");
        assert_rebuild_covers_ck(src, ck, "2B740A");
    }

    #[test]
    fn rebuild_nvnm_grid_fo4_covers_ck_4ea532() {
        let src = include_bytes!("test_fixtures/nvnm_grid_ck/src_4EA532.nvnm.bin");
        let ck = include_bytes!("test_fixtures/nvnm_grid_ck/ck_4EA532.nvnm.bin");
        assert_rebuild_covers_ck(src, ck, "4EA532");
    }

    #[test]
    fn rebuild_nvnm_grid_fo4_preserves_grid_params() {
        // Bounds, divisor, and grid_size_* MUST be preserved unchanged from
        // source — only per-cell triangle_indices content is regenerated.
        let src_bytes = include_bytes!("test_fixtures/nvnm_grid_ck/src_4EA53D.nvnm.bin");
        let source = parse_nvnm(src_bytes).expect("parse src");
        let finalized: Vec<NvnmFinalizedTriangle> = source
            .triangles
            .iter()
            .map(|t| NvnmFinalizedTriangle {
                vertices: t.vertices,
                links: t.links,
                flags: t.flags,
            })
            .collect();
        let rebuilt = rebuild_nvnm_grid_fo4(&source.grid, &source.vertices, &finalized);
        assert_eq!(rebuilt.divisor, source.grid.divisor);
        assert_eq!(rebuilt.grid_size_x, source.grid.grid_size_x);
        assert_eq!(rebuilt.grid_size_y, source.grid.grid_size_y);
        assert_eq!(rebuilt.bounds_min_x, source.grid.bounds_min_x);
        assert_eq!(rebuilt.bounds_min_y, source.grid.bounds_min_y);
        assert_eq!(rebuilt.bounds_min_z, source.grid.bounds_min_z);
        assert_eq!(rebuilt.bounds_max_x, source.grid.bounds_max_x);
        assert_eq!(rebuilt.bounds_max_y, source.grid.bounds_max_y);
        assert_eq!(rebuilt.bounds_max_z, source.grid.bounds_max_z);
        assert_eq!(rebuilt.cells.len(), source.grid.cells.len());
        // Each cell's indices should be sorted ascending.
        for (ci, cell) in rebuilt.cells.iter().enumerate() {
            let mut sorted = cell.triangle_indices.clone();
            sorted.sort_unstable();
            assert_eq!(
                cell.triangle_indices, sorted,
                "cell[{ci}] indices not sorted: {:?}",
                cell.triangle_indices
            );
        }
    }

    #[test]
    fn rebuild_nvnm_grid_fo4_boundary_triangle_indexed_into_both_cells() {
        // Synthetic case: divisor=2, bounds (0,0)-(20,20), grid_size=(10,10).
        // One triangle whose AABB max is EXACTLY on the cell boundary
        // (xmax = ymax = 10.0) — the inclusive floor rule must index it
        // into both cells touching that boundary.
        let grid = esp_authoring_core::nvnm::NvnmGrid {
            divisor: 2,
            grid_size_x: 10.0,
            grid_size_y: 10.0,
            bounds_min_x: 0.0,
            bounds_min_y: 0.0,
            bounds_min_z: 0.0,
            bounds_max_x: 20.0,
            bounds_max_y: 20.0,
            bounds_max_z: 0.0,
            cells: vec![esp_authoring_core::nvnm::NvnmGridCell::default(); 4],
        };
        let vertices = vec![
            NvnmVertex {
                x: 5.0,
                y: 5.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 10.0,
                y: 5.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 5.0,
                y: 10.0,
                z: 0.0,
            },
        ];
        let tri = NvnmFinalizedTriangle {
            vertices: [0, 1, 2],
            links: [-1; 3],
            flags: 0,
        };
        let rebuilt = rebuild_nvnm_grid_fo4(&grid, &vertices, &[tri]);
        // AABB = (5,5)-(10,10). cx_min = floor(5/10)=0, cx_max = floor(10/10)=1.
        // cy_min = 0, cy_max = 1. All 4 cells must include the triangle.
        for (ci, cell) in rebuilt.cells.iter().enumerate() {
            assert_eq!(
                cell.triangle_indices,
                vec![0i16],
                "cell[{ci}] must include the boundary-touching triangle"
            );
        }
    }

    #[test]
    fn rebuild_nvnm_grid_fo4_skips_triangles_outside_grid_bounds() {
        // A triangle whose AABB lies entirely OUTSIDE the grid rectangle
        // must not be indexed into the nearest boundary cell — naive
        // floor(negative)/clamp(0..max) arithmetic would coerce its cell
        // indices into valid range, so such triangles are skipped entirely.
        let grid = esp_authoring_core::nvnm::NvnmGrid {
            divisor: 2,
            grid_size_x: 10.0,
            grid_size_y: 10.0,
            bounds_min_x: 0.0,
            bounds_min_y: 0.0,
            bounds_min_z: 0.0,
            bounds_max_x: 20.0,
            bounds_max_y: 20.0,
            bounds_max_z: 0.0,
            cells: vec![esp_authoring_core::nvnm::NvnmGridCell::default(); 4],
        };
        // Vertices: triangle 0 is INSIDE cell (0,0). Triangle 1 is entirely
        // to the LEFT of the grid (xmin=-30, xmax=-20). Triangle 2 is
        // entirely BELOW the grid (ymin=-30, ymax=-20). Triangle 3 is
        // entirely above-right (xmin=30, ymin=30). Only triangle 0 should
        // appear in any cell.
        let vertices = vec![
            NvnmVertex {
                x: 2.0,
                y: 2.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 4.0,
                y: 2.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 2.0,
                y: 4.0,
                z: 0.0,
            },
            NvnmVertex {
                x: -30.0,
                y: 2.0,
                z: 0.0,
            },
            NvnmVertex {
                x: -20.0,
                y: 2.0,
                z: 0.0,
            },
            NvnmVertex {
                x: -30.0,
                y: 4.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 2.0,
                y: -30.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 4.0,
                y: -30.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 2.0,
                y: -20.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 30.0,
                y: 30.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 40.0,
                y: 30.0,
                z: 0.0,
            },
            NvnmVertex {
                x: 30.0,
                y: 40.0,
                z: 0.0,
            },
        ];
        let make_tri = |a: u16, b: u16, c: u16| NvnmFinalizedTriangle {
            vertices: [a, b, c],
            links: [-1; 3],
            flags: 0,
        };
        let tris = vec![
            make_tri(0, 1, 2),
            make_tri(3, 4, 5),
            make_tri(6, 7, 8),
            make_tri(9, 10, 11),
        ];
        let rebuilt = rebuild_nvnm_grid_fo4(&grid, &vertices, &tris);
        // Cell (0,0) gets triangle 0; all other cells must be empty.
        assert_eq!(
            rebuilt.cells[0].triangle_indices,
            vec![0i16],
            "cell[0] (bottom-left) should only contain the in-bounds triangle 0"
        );
        for ci in 1..rebuilt.cells.len() {
            assert!(
                rebuilt.cells[ci].triangle_indices.is_empty(),
                "cell[{ci}] should be empty (out-of-bounds triangles must NOT be added) — got {:?}",
                rebuilt.cells[ci].triangle_indices
            );
        }
    }

    fn top_group(label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label,
            group_type: 0,
            tail: Bytes::new(),
            children,
        })
    }

    #[test]
    fn build_source_info_to_dialogue_index_maps_infos_to_parent_dial() {
        const DIAL_A: u32 = 0x0010_0001;
        const DIAL_B: u32 = 0x0010_0002;
        const INFO_A1: u32 = 0x0010_1001;
        const INFO_A2: u32 = 0x0010_1002;
        const INFO_B1: u32 = 0x0010_2001;

        fn topic_child(dial_form_id: u32, infos: &[u32]) -> ParsedItem {
            ParsedItem::Group(ParsedGroup {
                label: dial_form_id.to_le_bytes(),
                group_type: 7,
                tail: Bytes::new(),
                children: infos
                    .iter()
                    .map(|&fid| ParsedItem::Record(parsed_record("INFO", fid)))
                    .collect(),
            })
        }

        let source_handle = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source = store.get_mut(&source_handle).unwrap();
            // DIAL top group: each DIAL record is followed by its Topic-Child
            // (type 7) group whose label is the DIAL form_id and whose children
            // are the INFOs. This is the only place INFO->DIAL parentage lives.
            source.parsed.root_items = vec![top_group(
                *b"DIAL",
                vec![
                    ParsedItem::Record(parsed_record("DIAL", DIAL_A)),
                    topic_child(DIAL_A, &[INFO_A1, INFO_A2]),
                    ParsedItem::Record(parsed_record("DIAL", DIAL_B)),
                    topic_child(DIAL_B, &[INFO_B1]),
                ],
            )];
        }

        let index = build_source_info_to_dialogue_index(source_handle).expect("index");
        assert_eq!(index.get(&INFO_A1), Some(&DIAL_A));
        assert_eq!(index.get(&INFO_A2), Some(&DIAL_A));
        assert_eq!(index.get(&INFO_B1), Some(&DIAL_B));
        assert_eq!(index.len(), 3);
    }

    fn top_group_ref<'a>(items: &'a [ParsedItem], label: &[u8; 4]) -> Option<&'a ParsedGroup> {
        items.iter().find_map(|item| match item {
            ParsedItem::Group(group) if is_top_group(group, label) => Some(group),
            _ => None,
        })
    }

    fn has_top_group(items: &[ParsedItem], label: &[u8; 4]) -> bool {
        top_group_ref(items, label).is_some()
    }

    fn contains_record(items: &[ParsedItem], signature: &str, form_id: u32) -> bool {
        items.iter().any(|item| match item {
            ParsedItem::Record(record) => {
                record.signature.as_str() == signature && record.form_id == form_id
            }
            ParsedItem::Group(group) => contains_record(&group.children, signature, form_id),
        })
    }

    fn contains_group_label(items: &[ParsedItem], group_type: i32, form_id: u32) -> bool {
        items.iter().any(|item| match item {
            ParsedItem::Record(_) => false,
            ParsedItem::Group(group) => {
                group.group_type == group_type && u32::from_le_bytes(group.label) == form_id
                    || contains_group_label(&group.children, group_type, form_id)
            }
        })
    }
}
