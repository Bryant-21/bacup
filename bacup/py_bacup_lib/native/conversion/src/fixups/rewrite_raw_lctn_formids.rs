//! Fixup: rewrite raw LCTN formids that the generic struct codec preserves as bytes.
//!
//! LCTN subrecords such as LCPR/LCSR/LCEC use struct codecs that are currently
//! passed through as raw bytes by the conversion reader. Without a targeted pass,
//! source-local references like `0025DA15` are written into an FO4 plugin with
//! masters, where they resolve as `Fallout4.esm:25DA15` instead of the converted
//! local record `SeventySix.esm:25DA15`.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;
use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, ParsedSubrecord, WriteEffect, effective_subrecords_for_record,
};

const WORLD_CHILD_GROUP: i32 = 1;
const CELL_CHILD_GROUP: i32 = 6;
const CELL_PERSISTENT_GROUP: i32 = 8;
const CELL_TEMPORARY_GROUP: i32 = 9;
const CELL_VISIBLE_DISTANT_GROUP: i32 = 10;
const RECORD_FLAG_PERSISTENT: u32 = 0x0000_0400;

pub struct RewriteRawLctnFormIdsFixup;

impl Fixup for RewriteRawLctnFormIdsFixup {
    fn name(&self) -> &'static str {
        "rewrite_raw_lctn_formids"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        rewrite_lctn_raw_formids(session, mapper, config.defer_placed_child_ref_class)
    }
}

/// Post-copy repair for raw LCTN formids. At this point exterior placed children
/// have been copied into the output plugin, so LCPR/LCSR rows can be rewritten
/// or pruned against the complete target instead of being deferred.
pub fn repair_lctn_raw_formids(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    _config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    rewrite_lctn_raw_formids(session, mapper, false)
}

fn rewrite_lctn_raw_formids(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    defer_placed_child_special_refs: bool,
) -> Result<FixupReport, FixupError> {
    let target_schema = session
        .schema()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let lctn_sig = SigCode::from_str("LCTN").map_err(|e| FixupError::Other(e.to_string()))?;
    let lctn_fks = session
        .form_keys_of_sig(lctn_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if lctn_fks.is_empty() {
        return Ok(FixupReport::empty());
    }

    let mut encoded_targets = encoded_targets_by_source_object_id(mapper, session.target_masters());
    // LCTN LCPR/LCSR rows reference placed REFR/ACHR records, which reach the
    // target through the cell-slice copy path — that path bypasses the
    // FormKeyMapper, so those refs never land in `source_to_target`. Their
    // object ids are preserved on copy, so the only valid target encoding is the
    // own-plugin one. Register every surviving own record here; otherwise the row
    // pruning below treats each special/persistent ref as an unresolved
    // source-local and drops the entire array, wiping the location's Special Ref
    // data (CK "Special Ref X is not in the Special Ref data").
    augment_encoded_targets_with_own_records(session, &mut encoded_targets, mapper.interner)?;
    if encoded_targets.is_empty() {
        return Ok(FixupReport::empty());
    }
    let persistence_index = (!defer_placed_child_special_refs).then(|| {
        LctnPersistenceIndex::from_items(
            &session.target_slot().parsed.root_items,
            session.target_masters().len() as u32,
        )
    });

    let mut report = FixupReport::empty();
    for fk in lctn_fks {
        let mut record = match session.record_decoded(&fk, target_schema.as_ref(), mapper.interner)
        {
            Ok(record) => record,
            Err(e) => {
                let w = mapper
                    .interner
                    .intern(&format!("rewrite_raw_lctn_formids_read_err:{e}"));
                report.warnings.push(w);
                continue;
            }
        };

        if rewrite_lctn_record(
            &mut record,
            &encoded_targets,
            defer_placed_child_special_refs,
            persistence_index.as_ref(),
        ) {
            session
                .replace_record_contents(record, target_schema.as_ref(), mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            report.records_changed += 1;
        }
    }
    if !defer_placed_child_special_refs {
        report.records_changed += strip_nonpersistent_nonactor_placed_ref_xlcn(session) as u32;
    }
    report.records_changed += sync_cell_locations_from_lctn_world_cells(session) as u32;
    if !defer_placed_child_special_refs {
        let reconciled = reconcile_lctn_persistence_locations(session);
        report.records_changed = report
            .records_changed
            .saturating_add(reconciled.records_changed);
        eprintln!(
            "[lctn_persistence] rows_rehomed={} rows_dropped={} refs_changed={}",
            reconciled.rows_rehomed, reconciled.rows_dropped, reconciled.refs_changed
        );
    }

    Ok(report)
}

fn encoded_targets_by_source_object_id(
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> FxHashMap<u32, u32> {
    let mut out = FxHashMap::default();
    for (source, target) in mapper.source_to_target_iter() {
        if let Some(encoded) = encode_target_form_id(target, mapper.interner, target_masters) {
            out.insert(source.local, encoded);
        }
    }
    out
}

/// Register every own-plugin record present in the target as an identity mapping
/// (`object_id -> (own_index << 24) | object_id`). Records copied outside the
/// FormKeyMapper (placed refs via the cell-slice path) keep their source object
/// ids, so this is the only encoding that resolves them. Mapper-derived entries
/// win on collision (`or_insert`).
fn augment_encoded_targets_with_own_records(
    session: &mut PluginSession,
    encoded_targets: &mut FxHashMap<u32, u32>,
    interner: &StringInterner,
) -> Result<(), FixupError> {
    let target_masters = session.target_masters().to_vec();
    let own_index = target_masters.len() as u32;
    let sigs = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for sig in sigs {
        let fks = session
            .form_keys_of_sig(sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let encoded = fks
            .into_iter()
            .filter_map(|fk| encode_target_form_id(fk, interner, &target_masters));
        augment_with_own_encoded_form_ids(encoded_targets, encoded, own_index);
    }
    Ok(())
}

fn augment_with_own_encoded_form_ids(
    encoded_targets: &mut FxHashMap<u32, u32>,
    own_encoded_form_ids: impl IntoIterator<Item = u32>,
    own_index: u32,
) {
    for encoded in own_encoded_form_ids {
        if encoded >> 24 == own_index {
            encoded_targets
                .entry(encoded & 0x00FF_FFFF)
                .or_insert(encoded);
        }
    }
}

fn encode_target_form_id(
    target: FormKey,
    interner: &StringInterner,
    target_masters: &[String],
) -> Option<u32> {
    if target.local == 0 {
        return Some(0);
    }
    let plugin_name = interner.resolve(target.plugin)?;
    let load_index = target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin_name))
        .unwrap_or(target_masters.len());
    if load_index > u8::MAX as usize || target.local > 0x00FF_FFFF {
        return None;
    }
    Some(((load_index as u32) << 24) | target.local)
}

fn rewrite_lctn_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    defer_placed_child_special_refs: bool,
    persistence_index: Option<&LctnPersistenceIndex>,
) -> bool {
    if record.sig.0 != *b"LCTN" {
        return false;
    }

    let mut changed = false;
    let location_object_id = record.form_key.local & 0x00FF_FFFF;
    let mut new_fields = smallvec::SmallVec::new();
    for mut entry in record.fields.drain(..) {
        if defer_placed_child_special_refs && is_placed_child_special_ref_array(entry.sig) {
            new_fields.push(entry);
            continue;
        }
        let Some(layout) = LctnRawLayout::for_sig(entry.sig) else {
            new_fields.push(entry);
            continue;
        };
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            new_fields.push(entry);
            continue;
        };
        let had_bytes = !bytes.is_empty();
        changed |= rewrite_lctn_bytes(
            bytes,
            layout,
            encoded_targets,
            persistence_index,
            location_object_id,
        );
        if had_bytes && bytes.is_empty() {
            changed = true;
            continue;
        }
        new_fields.push(entry);
    }
    record.fields = new_fields;
    changed
}

fn is_placed_child_special_ref_array(sig: SubrecordSig) -> bool {
    matches!(&sig.0, b"ACPR" | b"LCPR" | b"ACSR" | b"LCSR")
}

fn sync_cell_locations_from_lctn_world_cells(session: &mut PluginSession) -> usize {
    let mut locations = FxHashMap::default();
    let mut conflicts = rustc_hash::FxHashSet::default();
    collect_lctn_world_cell_locations(
        &session.target_slot().parsed.root_items,
        &mut locations,
        &mut conflicts,
    );
    for key in conflicts {
        locations.remove(&key);
    }
    if locations.is_empty() {
        return 0;
    }

    let mut changed_form_ids = Vec::new();
    let changed = tag_cell_locations_in_items(
        &mut session.target_slot_mut().parsed.root_items,
        None,
        &locations,
        &mut changed_form_ids,
    );
    if changed > 0 {
        session.target_slot_mut().clear_record_count_cache();
        session.record_effect(
            esp_authoring_core::plugin_runtime::WriteEffect::RecordContents {
                form_ids: changed_form_ids.into_iter().collect(),
            },
        );
    }
    changed
}

fn collect_lctn_world_cell_locations(
    items: &[ParsedItem],
    locations: &mut FxHashMap<(u32, i16, i16), u32>,
    conflicts: &mut rustc_hash::FxHashSet<(u32, i16, i16)>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "LCTN" => {
                collect_lctn_record_world_cell_locations(record, locations, conflicts);
            }
            ParsedItem::Group(group) => {
                collect_lctn_world_cell_locations(&group.children, locations, conflicts);
            }
            _ => {}
        }
    }
}

fn collect_lctn_record_world_cell_locations(
    record: &ParsedRecord,
    locations: &mut FxHashMap<(u32, i16, i16), u32>,
    conflicts: &mut rustc_hash::FxHashSet<(u32, i16, i16)>,
) {
    for subrecord in &record.subrecords {
        if !matches!(subrecord.signature.as_str(), "ACEC" | "LCEC") {
            continue;
        }
        let data = subrecord.data.as_ref();
        if data.len() < 8 || (data.len() - 4) % 4 != 0 {
            continue;
        }
        let world = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        if world == 0 {
            continue;
        }
        for cell in data[4..].chunks_exact(4) {
            let y = i16::from_le_bytes([cell[0], cell[1]]);
            let x = i16::from_le_bytes([cell[2], cell[3]]);
            let key = (world, x, y);
            if let Some(existing) = locations.get(&key).copied() {
                if existing != record.form_id {
                    conflicts.insert(key);
                }
            } else {
                locations.insert(key, record.form_id);
            }
        }
    }
}

fn tag_cell_locations_in_items(
    items: &mut [ParsedItem],
    current_world: Option<u32>,
    locations: &FxHashMap<(u32, i16, i16), u32>,
    changed_form_ids: &mut Vec<u32>,
) -> usize {
    let mut changed = 0;
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "CELL" => {
                let Some(world) = current_world else {
                    continue;
                };
                if let Some((x, y)) = cell_grid_i16(record) {
                    if let Some(location) = locations.get(&(world, x, y)).copied() {
                        if set_missing_cell_location(record, location) {
                            changed += 1;
                            changed_form_ids.push(record.form_id);
                        }
                    }
                }
            }
            ParsedItem::Group(group) => {
                let child_world = if group.group_type == WORLD_CHILD_GROUP {
                    Some(u32::from_le_bytes(group.label))
                } else {
                    current_world
                };
                changed += tag_cell_locations_in_items(
                    &mut group.children,
                    child_world,
                    locations,
                    changed_form_ids,
                );
            }
            _ => {}
        }
    }
    changed
}

fn cell_grid_i16(record: &ParsedRecord) -> Option<(i16, i16)> {
    let xclc = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "XCLC")?;
    let data = xclc.data.as_ref();
    if data.len() < 8 {
        return None;
    }
    let x = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let y = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if x < i16::MIN as i32 || x > i16::MAX as i32 || y < i16::MIN as i32 || y > i16::MAX as i32 {
        return None;
    }
    Some((x as i16, y as i16))
}

fn set_missing_cell_location(record: &mut ParsedRecord, location: u32) -> bool {
    if record
        .subrecords
        .iter()
        .any(|subrecord| subrecord.signature.as_str() == "XLCN")
    {
        return false;
    }
    let subrecord = ParsedSubrecord {
        signature: "XLCN".into(),
        data: Bytes::copy_from_slice(&location.to_le_bytes()),
        semantic_type: None,
    };
    let insert_at = record
        .subrecords
        .iter()
        .rposition(|subrecord| matches!(subrecord.signature.as_str(), "XCLW" | "LTMP" | "XCLC"))
        .map(|index| index + 1)
        .unwrap_or(record.subrecords.len());
    record.subrecords.insert(insert_at, subrecord);
    record.raw_payload = None;
    true
}

#[derive(Default)]
struct LctnPersistenceIndex {
    own_index: u32,
    worlds: FxHashSet<u32>,
    cells: FxHashSet<u32>,
    placed_refs: FxHashMap<u32, PlacedRefPersistence>,
}

struct PlacedRefPersistence {
    is_actor: bool,
    persistent: bool,
    persist_location: Option<u32>,
    world: Option<u32>,
    cell: Option<u32>,
    position_grid: Option<(i32, i32)>,
}

impl LctnPersistenceIndex {
    fn from_items(items: &[ParsedItem], own_index: u32) -> Self {
        let mut index = Self {
            own_index,
            ..Self::default()
        };
        index.collect_items(items, None, None);
        index
    }

    fn row_is_valid(
        &self,
        row: &[u8],
        location_object_id: u32,
        layout: LctnPersistenceRowLayout,
    ) -> bool {
        let Some(ref_raw) = read_u32_at(row, layout.ref_offset) else {
            return false;
        };
        if ref_raw == 0 {
            return false;
        }
        if ref_raw >> 24 != self.own_index {
            return true;
        }

        let ref_object_id = ref_raw & 0x00FF_FFFF;
        let Some(placed_ref) = self.placed_refs.get(&ref_object_id) else {
            return false;
        };
        if layout.require_non_actor_persistent && !placed_ref.persistent && !placed_ref.is_actor {
            return false;
        }
        // Special-reference rows are the location relationship; FO76 map markers
        // often do not also carry XLCN.
        if layout.require_ref_location && placed_ref.persist_location != Some(location_object_id) {
            return false;
        }

        let Some(world_cell_raw) = read_u32_at(row, layout.world_cell_offset) else {
            return false;
        };
        if world_cell_raw == 0 {
            return false;
        }
        if world_cell_raw >> 24 != self.own_index {
            return true;
        }

        let world_cell_object_id = world_cell_raw & 0x00FF_FFFF;
        if self.worlds.contains(&world_cell_object_id) {
            let Some(grid_y) = read_i16_at(row, layout.grid_y_offset) else {
                return false;
            };
            let Some(grid_x) = read_i16_at(row, layout.grid_x_offset) else {
                return false;
            };
            return placed_ref.world == Some(world_cell_object_id)
                && placed_ref.position_grid == Some((grid_x as i32, grid_y as i32));
        }
        if self.cells.contains(&world_cell_object_id) {
            return placed_ref.cell == Some(world_cell_object_id);
        }
        false
    }

    fn collect_items(
        &mut self,
        items: &[ParsedItem],
        current_world: Option<u32>,
        current_cell: Option<u32>,
    ) {
        for item in items {
            match item {
                ParsedItem::Record(record) => {
                    self.collect_record(record, current_world, current_cell)
                }
                ParsedItem::Group(group) => {
                    let object_id = u32::from_le_bytes(group.label) & 0x00FF_FFFF;
                    match group.group_type {
                        WORLD_CHILD_GROUP => {
                            self.collect_items(&group.children, Some(object_id), current_cell);
                        }
                        CELL_CHILD_GROUP => {
                            self.collect_items(&group.children, current_world, Some(object_id));
                        }
                        CELL_PERSISTENT_GROUP
                        | CELL_TEMPORARY_GROUP
                        | CELL_VISIBLE_DISTANT_GROUP => {
                            self.collect_items(&group.children, current_world, current_cell);
                        }
                        _ => self.collect_items(&group.children, current_world, current_cell),
                    }
                }
            }
        }
    }

    fn collect_record(
        &mut self,
        record: &ParsedRecord,
        current_world: Option<u32>,
        current_cell: Option<u32>,
    ) {
        let object_id = record.form_id & 0x00FF_FFFF;
        match record.signature.as_str() {
            "WRLD" => {
                self.worlds.insert(object_id);
            }
            "CELL" => {
                self.cells.insert(object_id);
            }
            sig if is_placed_record_sig(sig) => {
                let subrecords = effective_subrecords_for_record(record);
                self.placed_refs.insert(
                    object_id,
                    PlacedRefPersistence {
                        is_actor: record.signature.as_str() == "ACHR",
                        persistent: record.flags & RECORD_FLAG_PERSISTENT != 0,
                        persist_location: parsed_subrecords_first_form_id(
                            subrecords.as_ref(),
                            "XLCN",
                        ),
                        world: current_world,
                        cell: current_cell,
                        position_grid: parsed_ref_position_grid(subrecords.as_ref()),
                    },
                );
            }
            _ => {}
        }
    }
}

fn strip_nonpersistent_nonactor_placed_ref_xlcn(session: &mut PluginSession) -> usize {
    let mut changed_form_ids = SmallVec::<[u32; 4]>::new();
    let changed = strip_nonpersistent_nonactor_placed_ref_xlcn_from_items(
        &mut session.target_slot_mut().parsed.root_items,
        &mut changed_form_ids,
    );
    if changed > 0 {
        session.record_effect(WriteEffect::RecordContents {
            form_ids: changed_form_ids,
        });
    }
    changed
}

fn strip_nonpersistent_nonactor_placed_ref_xlcn_from_items(
    items: &mut [ParsedItem],
    changed_form_ids: &mut SmallVec<[u32; 4]>,
) -> usize {
    let mut changed = 0;
    for item in items {
        match item {
            ParsedItem::Record(record)
                if is_placed_record_sig(record.signature.as_str())
                    && record.signature.as_str() != "ACHR"
                    && record.flags & RECORD_FLAG_PERSISTENT == 0 =>
            {
                if record.subrecords.is_empty() {
                    record.subrecords = effective_subrecords_for_record(record).into_owned();
                }
                let before = record.subrecords.len();
                record
                    .subrecords
                    .retain(|subrecord| subrecord.signature.as_str() != "XLCN");
                if record.subrecords.len() != before {
                    record.raw_payload = None;
                    changed_form_ids.push(record.form_id);
                    changed += 1;
                }
            }
            ParsedItem::Group(group) => {
                changed += strip_nonpersistent_nonactor_placed_ref_xlcn_from_items(
                    &mut group.children,
                    changed_form_ids,
                );
            }
            _ => {}
        }
    }
    changed
}

fn is_placed_record_sig(sig: &str) -> bool {
    matches!(
        sig,
        "REFR"
            | "ACHR"
            | "PGRE"
            | "PHZD"
            | "PMIS"
            | "PARW"
            | "PBAR"
            | "PBEA"
            | "PCON"
            | "PFLA"
            | "PGRD"
    )
}

fn parsed_subrecords_first_form_id(subrecords: &[ParsedSubrecord], sig: &str) -> Option<u32> {
    subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == sig)
        .and_then(|subrecord| read_u32_at(subrecord.data.as_ref(), 0))
        .map(|raw| raw & 0x00FF_FFFF)
        .filter(|raw| *raw != 0)
}

fn parsed_ref_position_grid(subrecords: &[ParsedSubrecord]) -> Option<(i32, i32)> {
    let data = subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "DATA")?
        .data
        .as_ref();
    let x = read_f32_at(data, 0)?;
    let y = read_f32_at(data, 4)?;
    Some(((x / 4096.0).floor() as i32, (y / 4096.0).floor() as i32))
}

#[derive(Default)]
struct LctnPersistenceLocationIndex {
    own_index: u32,
    worlds: FxHashSet<u32>,
    locations: FxHashSet<u32>,
    location_parents: FxHashMap<u32, u32>,
    exterior_cells: FxHashMap<(u32, i16, i16), Option<u32>>,
    interior_cells: FxHashMap<u32, Option<u32>>,
}

impl LctnPersistenceLocationIndex {
    fn from_items(items: &[ParsedItem], own_index: u32) -> Self {
        let mut index = Self {
            own_index,
            ..Self::default()
        };
        index.collect_items(items, None);
        index
    }

    fn collect_items(&mut self, items: &[ParsedItem], current_world: Option<u32>) {
        for item in items {
            match item {
                ParsedItem::Record(record) => self.collect_record(record, current_world),
                ParsedItem::Group(group) => {
                    let child_world = if group.group_type == WORLD_CHILD_GROUP {
                        Some(u32::from_le_bytes(group.label) & 0x00FF_FFFF)
                    } else {
                        current_world
                    };
                    self.collect_items(&group.children, child_world);
                }
            }
        }
    }

    fn collect_record(&mut self, record: &ParsedRecord, current_world: Option<u32>) {
        let object_id = record.form_id & 0x00FF_FFFF;
        let subrecords = effective_subrecords_for_record(record);
        match record.signature.as_str() {
            "WRLD" => {
                self.worlds.insert(object_id);
            }
            "LCTN" => {
                self.locations.insert(object_id);
                if let Some(parent) = parsed_subrecords_raw_form_id(subrecords.as_ref(), "PNAM") {
                    self.location_parents.insert(object_id, parent);
                }
            }
            "CELL" => {
                let location = parsed_subrecords_raw_form_id(subrecords.as_ref(), "XLCN");
                self.interior_cells.insert(object_id, location);
                if let (Some(world), Some((x, y))) =
                    (current_world, parsed_cell_grid_i16(subrecords.as_ref()))
                {
                    self.exterior_cells.insert((world, x, y), location);
                }
            }
            _ => {}
        }
    }

    fn row_cell_location(&self, world_cell_raw: u32, grid_x: i16, grid_y: i16) -> RowCellLocation {
        if world_cell_raw >> 24 != self.own_index {
            return RowCellLocation::External;
        }
        let object_id = world_cell_raw & 0x00FF_FFFF;
        if self.worlds.contains(&object_id) {
            return match self.exterior_cells.get(&(object_id, grid_x, grid_y)) {
                Some(Some(location)) => RowCellLocation::Location(*location),
                Some(None) => RowCellLocation::NoLocation,
                None => RowCellLocation::Missing,
            };
        }
        match self.interior_cells.get(&object_id) {
            Some(Some(location)) => RowCellLocation::Location(*location),
            Some(None) => RowCellLocation::NoLocation,
            None => RowCellLocation::Missing,
        }
    }

    fn location_contains(&self, ancestor_raw: u32, descendant_raw: u32) -> bool {
        if ancestor_raw == descendant_raw {
            return true;
        }
        if ancestor_raw >> 24 != self.own_index || descendant_raw >> 24 != self.own_index {
            return false;
        }

        let ancestor = ancestor_raw & 0x00FF_FFFF;
        let mut current = descendant_raw & 0x00FF_FFFF;
        let mut seen = FxHashSet::default();
        while seen.insert(current) {
            let Some(parent_raw) = self.location_parents.get(&current).copied() else {
                return false;
            };
            if parent_raw >> 24 != self.own_index {
                return false;
            }
            current = parent_raw & 0x00FF_FFFF;
            if current == ancestor {
                return true;
            }
        }
        false
    }
}

enum RowCellLocation {
    External,
    Missing,
    NoLocation,
    Location(u32),
}

#[derive(Clone)]
struct RehomedPersistenceRow {
    sig: smol_str::SmolStr,
    bytes: [u8; 12],
}

enum RefLocationAction {
    Set(u32),
    Remove(u32),
}

#[derive(Default)]
struct LctnPersistenceReconcileReport {
    records_changed: u32,
    rows_rehomed: u32,
    rows_dropped: u32,
    refs_changed: u32,
}

fn reconcile_lctn_persistence_locations(
    session: &mut PluginSession,
) -> LctnPersistenceReconcileReport {
    let own_index = session.target_masters().len() as u32;
    let index = LctnPersistenceLocationIndex::from_items(
        &session.target_slot().parsed.root_items,
        own_index,
    );
    let mut moves: FxHashMap<u32, Vec<RehomedPersistenceRow>> = FxHashMap::default();
    let mut ref_actions: FxHashMap<u32, RefLocationAction> = FxHashMap::default();
    let mut changed_form_ids = SmallVec::<[u32; 4]>::new();
    let mut report = LctnPersistenceReconcileReport::default();

    collect_lctn_persistence_moves(
        &mut session.target_slot_mut().parsed.root_items,
        &index,
        &mut moves,
        &mut ref_actions,
        &mut changed_form_ids,
        &mut report,
    );
    apply_lctn_persistence_moves(
        &mut session.target_slot_mut().parsed.root_items,
        &mut moves,
        &mut changed_form_ids,
    );
    report.refs_changed = apply_ref_location_actions(
        &mut session.target_slot_mut().parsed.root_items,
        &ref_actions,
        &mut changed_form_ids,
    );

    changed_form_ids.sort_unstable();
    changed_form_ids.dedup();
    report.records_changed = changed_form_ids.len() as u32;
    if !changed_form_ids.is_empty() {
        session.record_effect(WriteEffect::RecordContents {
            form_ids: changed_form_ids,
        });
    }
    report
}

fn collect_lctn_persistence_moves(
    items: &mut [ParsedItem],
    index: &LctnPersistenceLocationIndex,
    moves: &mut FxHashMap<u32, Vec<RehomedPersistenceRow>>,
    ref_actions: &mut FxHashMap<u32, RefLocationAction>,
    changed_form_ids: &mut SmallVec<[u32; 4]>,
    report: &mut LctnPersistenceReconcileReport,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "LCTN" => {
                let source_location = record.form_id;
                if record.subrecords.is_empty() {
                    record.subrecords = effective_subrecords_for_record(record).into_owned();
                }
                let mut record_changed = false;
                for subrecord in &mut record.subrecords {
                    if !matches!(subrecord.signature.as_str(), "LCPR" | "ACPR") {
                        continue;
                    }
                    let mut kept = Vec::with_capacity(subrecord.data.len());
                    for row in subrecord.data.chunks_exact(12) {
                        let ref_raw = u32::from_le_bytes(row[0..4].try_into().unwrap());
                        let world_cell_raw = u32::from_le_bytes(row[4..8].try_into().unwrap());
                        let grid_y = i16::from_le_bytes(row[8..10].try_into().unwrap());
                        let grid_x = i16::from_le_bytes(row[10..12].try_into().unwrap());
                        if ref_raw >> 24 != index.own_index {
                            kept.extend_from_slice(row);
                            continue;
                        }

                        match index.row_cell_location(world_cell_raw, grid_x, grid_y) {
                            RowCellLocation::External => kept.extend_from_slice(row),
                            RowCellLocation::Location(actual_location)
                                if index.location_contains(source_location, actual_location) =>
                            {
                                kept.extend_from_slice(row);
                            }
                            RowCellLocation::Location(actual_location)
                                if actual_location >> 24 == index.own_index
                                    && index
                                        .locations
                                        .contains(&(actual_location & 0x00FF_FFFF)) =>
                            {
                                moves.entry(actual_location).or_default().push(
                                    RehomedPersistenceRow {
                                        sig: subrecord.signature.clone(),
                                        bytes: row.try_into().unwrap(),
                                    },
                                );
                                ref_actions
                                    .insert(ref_raw, RefLocationAction::Set(actual_location));
                                report.rows_rehomed = report.rows_rehomed.saturating_add(1);
                                record_changed = true;
                            }
                            RowCellLocation::Missing
                            | RowCellLocation::NoLocation
                            | RowCellLocation::Location(_) => {
                                ref_actions
                                    .entry(ref_raw)
                                    .or_insert(RefLocationAction::Remove(source_location));
                                report.rows_dropped = report.rows_dropped.saturating_add(1);
                                record_changed = true;
                            }
                        }
                    }
                    if subrecord.data.len() % 12 != 0 {
                        kept.extend_from_slice(&subrecord.data[subrecord.data.len() / 12 * 12..]);
                    }
                    if kept.len() != subrecord.data.len() {
                        subrecord.data = Bytes::from(kept);
                    }
                }
                let before = record.subrecords.len();
                record.subrecords.retain(|subrecord| {
                    !matches!(subrecord.signature.as_str(), "LCPR" | "ACPR")
                        || !subrecord.data.is_empty()
                });
                record_changed |= record.subrecords.len() != before;
                if record_changed {
                    record.raw_payload = None;
                    changed_form_ids.push(record.form_id);
                }
            }
            ParsedItem::Group(group) => collect_lctn_persistence_moves(
                &mut group.children,
                index,
                moves,
                ref_actions,
                changed_form_ids,
                report,
            ),
            _ => {}
        }
    }
}

fn apply_lctn_persistence_moves(
    items: &mut [ParsedItem],
    moves: &mut FxHashMap<u32, Vec<RehomedPersistenceRow>>,
    changed_form_ids: &mut SmallVec<[u32; 4]>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "LCTN" => {
                let Some(rows) = moves.remove(&record.form_id) else {
                    continue;
                };
                if record.subrecords.is_empty() {
                    record.subrecords = effective_subrecords_for_record(record).into_owned();
                }
                let mut changed = false;
                for row in rows {
                    let ref_raw = u32::from_le_bytes(row.bytes[0..4].try_into().unwrap());
                    let existing = record
                        .subrecords
                        .iter()
                        .filter(|subrecord| subrecord.signature == row.sig)
                        .flat_map(|subrecord| subrecord.data.chunks_exact(12))
                        .any(|candidate| {
                            u32::from_le_bytes(candidate[0..4].try_into().unwrap()) == ref_raw
                        });
                    if existing {
                        continue;
                    }
                    if let Some(subrecord) = record
                        .subrecords
                        .iter_mut()
                        .find(|subrecord| subrecord.signature == row.sig)
                    {
                        let mut bytes = subrecord.data.to_vec();
                        bytes.extend_from_slice(&row.bytes);
                        subrecord.data = Bytes::from(bytes);
                    } else {
                        let insert_at = record
                            .subrecords
                            .iter()
                            .position(|subrecord| {
                                matches!(
                                    subrecord.signature.as_str(),
                                    "LCSR" | "ACSR" | "LCEC" | "ACEC"
                                )
                            })
                            .unwrap_or(record.subrecords.len());
                        record.subrecords.insert(
                            insert_at,
                            ParsedSubrecord {
                                signature: row.sig,
                                data: Bytes::copy_from_slice(&row.bytes),
                                semantic_type: None,
                            },
                        );
                    }
                    changed = true;
                }
                if changed {
                    record.raw_payload = None;
                    changed_form_ids.push(record.form_id);
                }
            }
            ParsedItem::Group(group) => {
                apply_lctn_persistence_moves(&mut group.children, moves, changed_form_ids)
            }
            _ => {}
        }
    }
}

fn apply_ref_location_actions(
    items: &mut [ParsedItem],
    actions: &FxHashMap<u32, RefLocationAction>,
    changed_form_ids: &mut SmallVec<[u32; 4]>,
) -> u32 {
    let mut changed_count = 0;
    for item in items {
        match item {
            ParsedItem::Record(record) if is_placed_record_sig(record.signature.as_str()) => {
                let Some(action) = actions.get(&record.form_id) else {
                    continue;
                };
                if record.subrecords.is_empty() {
                    record.subrecords = effective_subrecords_for_record(record).into_owned();
                }
                let changed = match action {
                    RefLocationAction::Set(location) => {
                        let mut changed = false;
                        for subrecord in record
                            .subrecords
                            .iter_mut()
                            .filter(|subrecord| subrecord.signature.as_str() == "XLCN")
                        {
                            if subrecord.data.as_ref() != location.to_le_bytes() {
                                subrecord.data = Bytes::copy_from_slice(&location.to_le_bytes());
                                changed = true;
                            }
                        }
                        changed
                    }
                    RefLocationAction::Remove(expected) => {
                        let before = record.subrecords.len();
                        record.subrecords.retain(|subrecord| {
                            if subrecord.signature.as_str() != "XLCN" || subrecord.data.len() < 4 {
                                return true;
                            }
                            u32::from_le_bytes(subrecord.data[..4].try_into().unwrap()) != *expected
                        });
                        record.subrecords.len() != before
                    }
                };
                if changed {
                    record.raw_payload = None;
                    changed_form_ids.push(record.form_id);
                    changed_count += 1;
                }
            }
            ParsedItem::Group(group) => {
                changed_count +=
                    apply_ref_location_actions(&mut group.children, actions, changed_form_ids);
            }
            _ => {}
        }
    }
    changed_count
}

fn parsed_subrecords_raw_form_id(subrecords: &[ParsedSubrecord], sig: &str) -> Option<u32> {
    subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == sig)
        .and_then(|subrecord| read_u32_at(subrecord.data.as_ref(), 0))
        .filter(|raw| *raw != 0)
}

fn parsed_cell_grid_i16(subrecords: &[ParsedSubrecord]) -> Option<(i16, i16)> {
    let xclc = subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "XCLC")?;
    let x = i32::from_le_bytes(xclc.data.get(0..4)?.try_into().ok()?);
    let y = i32::from_le_bytes(xclc.data.get(4..8)?.try_into().ok()?);
    Some((i16::try_from(x).ok()?, i16::try_from(y).ok()?))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let chunk = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(chunk.try_into().ok()?))
}

fn read_i16_at(bytes: &[u8], offset: usize) -> Option<i16> {
    let chunk = bytes.get(offset..offset.checked_add(2)?)?;
    Some(i16::from_le_bytes(chunk.try_into().ok()?))
}

fn read_f32_at(bytes: &[u8], offset: usize) -> Option<f32> {
    let chunk = bytes.get(offset..offset.checked_add(4)?)?;
    Some(f32::from_le_bytes(chunk.try_into().ok()?))
}

#[derive(Clone, Copy)]
enum LctnRawLayout {
    Rows {
        row_size: usize,
        formid_offsets: &'static [usize],
        persistence_ref: Option<LctnPersistenceRowLayout>,
    },
    FirstFormId,
}

#[derive(Clone, Copy)]
struct LctnPersistenceRowLayout {
    ref_offset: usize,
    world_cell_offset: usize,
    grid_y_offset: usize,
    grid_x_offset: usize,
    require_ref_location: bool,
    /// LCPR/ACPR requires persistence for non-actors, but vanilla FO4 uses it
    /// for temporary ACHRs with XLCN. LCSR/ACSR also allows nonpersistent refs.
    require_non_actor_persistent: bool,
}

impl LctnRawLayout {
    fn for_sig(sig: SubrecordSig) -> Option<Self> {
        match &sig.0 {
            b"ACPR" | b"LCPR" => Some(Self::Rows {
                row_size: 12,
                formid_offsets: &[0, 4],
                persistence_ref: Some(LctnPersistenceRowLayout {
                    ref_offset: 0,
                    world_cell_offset: 4,
                    grid_y_offset: 8,
                    grid_x_offset: 10,
                    require_ref_location: true,
                    require_non_actor_persistent: true,
                }),
            }),
            b"ACSR" | b"LCSR" => Some(Self::Rows {
                row_size: 16,
                formid_offsets: &[0, 4, 8],
                persistence_ref: Some(LctnPersistenceRowLayout {
                    ref_offset: 4,
                    world_cell_offset: 8,
                    grid_y_offset: 12,
                    grid_x_offset: 14,
                    require_ref_location: false,
                    require_non_actor_persistent: false,
                }),
            }),
            b"ACEC" | b"LCEC" => Some(Self::FirstFormId),
            _ => None,
        }
    }
}

fn rewrite_lctn_bytes(
    bytes: &mut SmallVec<[u8; 32]>,
    layout: LctnRawLayout,
    encoded_targets: &FxHashMap<u32, u32>,
    persistence_index: Option<&LctnPersistenceIndex>,
    location_object_id: u32,
) -> bool {
    match layout {
        LctnRawLayout::Rows {
            row_size,
            formid_offsets,
            persistence_ref,
        } => rewrite_row_formids(
            bytes,
            row_size,
            formid_offsets,
            persistence_ref.and_then(|layout| {
                persistence_index.map(|index| (layout, index, location_object_id))
            }),
            encoded_targets,
        ),
        LctnRawLayout::FirstFormId => {
            match rewrite_or_validate_formid_at(bytes, 0, encoded_targets) {
                RawFormIdStatus::Resolved(changed) => changed,
                RawFormIdStatus::UnresolvedSourceLocal => {
                    bytes.clear();
                    true
                }
            }
        }
    }
}

fn rewrite_row_formids(
    bytes: &mut SmallVec<[u8; 32]>,
    row_size: usize,
    formid_offsets: &[usize],
    persistence_ref: Option<(LctnPersistenceRowLayout, &LctnPersistenceIndex, u32)>,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    if row_size == 0 || bytes.len() % row_size != 0 {
        return false;
    }

    let mut changed = false;
    let mut kept = Vec::with_capacity(bytes.len());
    for row_start in (0..bytes.len()).step_by(row_size) {
        let mut row = bytes[row_start..row_start + row_size].to_vec();
        let mut drop_row = false;
        for offset in formid_offsets {
            match rewrite_or_validate_formid_at(&mut row, *offset, encoded_targets) {
                RawFormIdStatus::Resolved(row_changed) => changed |= row_changed,
                RawFormIdStatus::UnresolvedSourceLocal => {
                    changed = true;
                    drop_row = true;
                    break;
                }
            }
        }
        if let Some((layout, index, location_object_id)) = persistence_ref {
            if !index.row_is_valid(&row, location_object_id, layout) {
                changed = true;
                drop_row = true;
            }
        }
        if !drop_row {
            kept.extend_from_slice(&row);
        }
    }
    if changed {
        bytes.clear();
        bytes.extend_from_slice(&kept);
    }
    changed
}

#[derive(Clone, Copy)]
enum RawFormIdStatus {
    Resolved(bool),
    UnresolvedSourceLocal,
}

#[cfg(test)]
fn rewrite_formid_at(
    bytes: &mut [u8],
    offset: usize,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    matches!(
        rewrite_or_validate_formid_at(bytes, offset, encoded_targets),
        RawFormIdStatus::Resolved(true)
    )
}

fn rewrite_or_validate_formid_at(
    bytes: &mut [u8],
    offset: usize,
    encoded_targets: &FxHashMap<u32, u32>,
) -> RawFormIdStatus {
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return RawFormIdStatus::Resolved(false);
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    if raw == 0 || raw >> 24 != 0 {
        return RawFormIdStatus::Resolved(false);
    }
    let Some(encoded) = encoded_targets.get(&raw) else {
        return RawFormIdStatus::UnresolvedSourceLocal;
    };
    if *encoded == raw {
        return RawFormIdStatus::Resolved(false);
    }
    chunk.copy_from_slice(&encoded.to_le_bytes());
    RawFormIdStatus::Resolved(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FormKey;
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::ParsedGroup;
    use smallvec::SmallVec;

    fn target_map() -> FxHashMap<u32, u32> {
        let mut map = FxHashMap::default();
        map.insert(0x25DA15, 0x0725DA15);
        map.insert(0x3D4B0D, 0x073D4B0D);
        map.insert(0x001234, 0x07001234);
        map
    }

    fn target_map_without_world() -> FxHashMap<u32, u32> {
        let mut map = FxHashMap::default();
        map.insert(0x3D4B0D, 0x073D4B0D);
        map.insert(0x001234, 0x07001234);
        map
    }

    fn lctn_record_with_raw(sig: &str, raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("LCTN").unwrap(),
            FormKey::parse("00414F@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn lcsr_row(loc_ref_type: u32, reference: u32, world_cell: u32) -> Vec<u8> {
        lcsr_row_at(loc_ref_type, reference, world_cell, -18, -37)
    }

    fn lcsr_row_at(loc_ref_type: u32, reference: u32, world_cell: u32, y: i16, x: i16) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&loc_ref_type.to_le_bytes());
        raw.extend_from_slice(&reference.to_le_bytes());
        raw.extend_from_slice(&world_cell.to_le_bytes());
        raw.extend_from_slice(&y.to_le_bytes());
        raw.extend_from_slice(&x.to_le_bytes());
        raw
    }

    fn lcpr_row(reference: u32, world_cell: u32, y: i16, x: i16) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&reference.to_le_bytes());
        raw.extend_from_slice(&world_cell.to_le_bytes());
        raw.extend_from_slice(&y.to_le_bytes());
        raw.extend_from_slice(&x.to_le_bytes());
        raw
    }

    fn parsed_subrecord(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: sig.into(),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn parsed_record(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        parsed_record_with_flags(sig, form_id, 0, subrecords)
    }

    fn parsed_record_with_flags(
        sig: &str,
        form_id: u32,
        flags: u32,
        subrecords: Vec<ParsedSubrecord>,
    ) -> ParsedRecord {
        ParsedRecord {
            signature: sig.into(),
            form_id,
            flags,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn placed_record(
        sig: &str,
        form_id: u32,
        flags: u32,
        location: Option<u32>,
        x: f32,
        y: f32,
    ) -> ParsedRecord {
        let mut subrecords = Vec::new();
        if let Some(location) = location {
            subrecords.push(parsed_subrecord("XLCN", location.to_le_bytes().to_vec()));
        }
        let mut data = Vec::new();
        data.extend_from_slice(&x.to_le_bytes());
        data.extend_from_slice(&y.to_le_bytes());
        data.extend_from_slice(&[0; 16]);
        subrecords.push(parsed_subrecord("DATA", data));
        parsed_record_with_flags(sig, form_id, flags, subrecords)
    }

    fn lctn_cell_record(form_id: u32, world: u32, x: i16, y: i16) -> ParsedRecord {
        let mut raw = Vec::new();
        raw.extend_from_slice(&world.to_le_bytes());
        raw.extend_from_slice(&y.to_le_bytes());
        raw.extend_from_slice(&x.to_le_bytes());
        parsed_record("LCTN", form_id, vec![parsed_subrecord("LCEC", raw)])
    }

    fn parsed_cell_record(form_id: u32, x: i32, y: i32, location: Option<u32>) -> ParsedRecord {
        let mut xclc = Vec::new();
        xclc.extend_from_slice(&x.to_le_bytes());
        xclc.extend_from_slice(&y.to_le_bytes());
        xclc.extend_from_slice(&[0, 0, 0, 0]);
        let mut subrecords = vec![
            parsed_subrecord("XCLC", xclc),
            parsed_subrecord("LTMP", 0_u32.to_le_bytes().to_vec()),
            parsed_subrecord("XCLW", 0x7F7F_FFFF_u32.to_le_bytes().to_vec()),
        ];
        if let Some(location) = location {
            subrecords.push(parsed_subrecord("XLCN", location.to_le_bytes().to_vec()));
        }
        parsed_record("CELL", form_id, subrecords)
    }

    fn world_children_group(world: u32, children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label: world.to_le_bytes(),
            group_type: WORLD_CHILD_GROUP,
            tail: Bytes::new(),
            children,
        })
    }

    fn group(group_type: i32, label: u32, children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label: label.to_le_bytes(),
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    fn cell_child_group(cell: u32, children: Vec<ParsedItem>) -> ParsedItem {
        group(CELL_CHILD_GROUP, cell, children)
    }

    fn cell_section_group(group_type: i32, children: Vec<ParsedItem>) -> ParsedItem {
        group(group_type, 0, children)
    }

    fn exterior_persistence_items(world: u32, cell: u32, placed: ParsedRecord) -> Vec<ParsedItem> {
        vec![
            ParsedItem::Record(parsed_record("WRLD", world, vec![])),
            world_children_group(
                world,
                vec![
                    ParsedItem::Record(parsed_cell_record(cell, -37, -18, None)),
                    cell_child_group(
                        cell,
                        vec![cell_section_group(
                            CELL_PERSISTENT_GROUP,
                            vec![ParsedItem::Record(placed)],
                        )],
                    ),
                ],
            ),
        ]
    }

    fn interior_persistence_items(cell: u32, placed: ParsedRecord) -> Vec<ParsedItem> {
        vec![
            ParsedItem::Record(parsed_record("CELL", cell, vec![])),
            cell_child_group(
                cell,
                vec![cell_section_group(
                    CELL_PERSISTENT_GROUP,
                    vec![ParsedItem::Record(placed)],
                )],
            ),
        ]
    }

    fn first_cell(items: &[ParsedItem]) -> Option<&ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.signature.as_str() == "CELL" => {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(record) = first_cell(&group.children) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn find_record(items: &[ParsedItem], form_id: u32) -> Option<&ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id == form_id => return Some(record),
                ParsedItem::Group(group) => {
                    if let Some(record) = find_record(&group.children, form_id) {
                        return Some(record);
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
                    if let Some(record) = find_record_mut(&mut group.children, form_id) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn reconcile_test_items(actual_parent: Option<u32>) -> Vec<ParsedItem> {
        const SOURCE_LOCATION: u32 = 0x077D6A95;
        const ACTUAL_LOCATION: u32 = 0x077D2A0E;
        const WORLD: u32 = 0x0725DA15;
        const CELL: u32 = 0x07265353;
        const REFERENCE: u32 = 0x077FF721;

        let source_location = parsed_record(
            "LCTN",
            SOURCE_LOCATION,
            vec![parsed_subrecord(
                "LCPR",
                lcpr_row(REFERENCE, WORLD, 17, -54),
            )],
        );
        let actual_location = parsed_record(
            "LCTN",
            ACTUAL_LOCATION,
            actual_parent
                .map(|parent| vec![parsed_subrecord("PNAM", parent.to_le_bytes().to_vec())])
                .unwrap_or_default(),
        );
        let placed = placed_record(
            "ACHR",
            REFERENCE,
            RECORD_FLAG_PERSISTENT,
            Some(SOURCE_LOCATION),
            -53.25 * 4096.0,
            17.25 * 4096.0,
        );

        vec![
            ParsedItem::Record(source_location),
            ParsedItem::Record(actual_location),
            ParsedItem::Record(parsed_record("WRLD", WORLD, vec![])),
            world_children_group(
                WORLD,
                vec![
                    ParsedItem::Record(parsed_cell_record(CELL, -54, 17, Some(ACTUAL_LOCATION))),
                    cell_child_group(
                        CELL,
                        vec![cell_section_group(
                            CELL_PERSISTENT_GROUP,
                            vec![ParsedItem::Record(placed)],
                        )],
                    ),
                ],
            ),
        ]
    }

    fn reconcile_test_tree(items: &mut [ParsedItem]) -> LctnPersistenceReconcileReport {
        let index = LctnPersistenceLocationIndex::from_items(items, 7);
        let mut moves = FxHashMap::default();
        let mut actions = FxHashMap::default();
        let mut changed = SmallVec::<[u32; 4]>::new();
        let mut report = LctnPersistenceReconcileReport::default();
        collect_lctn_persistence_moves(
            items,
            &index,
            &mut moves,
            &mut actions,
            &mut changed,
            &mut report,
        );
        apply_lctn_persistence_moves(items, &mut moves, &mut changed);
        report.refs_changed = apply_ref_location_actions(items, &actions, &mut changed);
        report
    }

    #[test]
    fn rehomes_persistence_row_when_cell_uses_sibling_location() {
        const SOURCE_LOCATION: u32 = 0x077D6A95;
        const ACTUAL_LOCATION: u32 = 0x077D2A0E;
        const REFERENCE: u32 = 0x077FF721;
        let mut items = reconcile_test_items(None);

        let report = reconcile_test_tree(&mut items);

        assert_eq!(report.rows_rehomed, 1);
        assert_eq!(report.rows_dropped, 0);
        assert_eq!(report.refs_changed, 1);
        let source = find_record(&items, SOURCE_LOCATION).expect("source location");
        assert!(
            source
                .subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "LCPR")
        );
        let actual = find_record(&items, ACTUAL_LOCATION).expect("actual location");
        let moved = actual
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "LCPR")
            .expect("moved LCPR row");
        assert_eq!(
            u32::from_le_bytes(moved.data[0..4].try_into().unwrap()),
            REFERENCE
        );
        let reference = find_record(&items, REFERENCE).expect("placed reference");
        assert_eq!(
            parsed_subrecords_raw_form_id(&reference.subrecords, "XLCN"),
            Some(ACTUAL_LOCATION)
        );
    }

    #[test]
    fn keeps_persistence_row_when_cell_location_is_descendant() {
        const SOURCE_LOCATION: u32 = 0x077D6A95;
        const ACTUAL_LOCATION: u32 = 0x077D2A0E;
        const REFERENCE: u32 = 0x077FF721;
        let mut items = reconcile_test_items(Some(SOURCE_LOCATION));

        let report = reconcile_test_tree(&mut items);

        assert_eq!(report.rows_rehomed, 0);
        assert_eq!(report.rows_dropped, 0);
        assert_eq!(report.refs_changed, 0);
        let source = find_record(&items, SOURCE_LOCATION).expect("source location");
        assert!(
            source
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature.as_str() == "LCPR")
        );
        let reference = find_record(&items, REFERENCE).expect("placed reference");
        assert_eq!(
            parsed_subrecords_raw_form_id(&reference.subrecords, "XLCN"),
            Some(SOURCE_LOCATION)
        );
        let actual = find_record(&items, ACTUAL_LOCATION).expect("actual location");
        assert!(
            actual
                .subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "LCPR")
        );
    }

    #[test]
    fn drops_persistence_row_when_cell_has_no_location() {
        const SOURCE_LOCATION: u32 = 0x077D6A95;
        const CELL: u32 = 0x07265353;
        const REFERENCE: u32 = 0x077FF721;
        let mut items = reconcile_test_items(None);
        let cell = find_record_mut(&mut items, CELL).expect("cell");
        cell.subrecords
            .retain(|subrecord| subrecord.signature.as_str() != "XLCN");

        let report = reconcile_test_tree(&mut items);

        assert_eq!(report.rows_rehomed, 0);
        assert_eq!(report.rows_dropped, 1);
        assert_eq!(report.refs_changed, 1);
        let source = find_record(&items, SOURCE_LOCATION).expect("source location");
        assert!(
            source
                .subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "LCPR")
        );
        let reference = find_record(&items, REFERENCE).expect("placed reference");
        assert_eq!(
            parsed_subrecords_raw_form_id(&reference.subrecords, "XLCN"),
            None
        );
    }

    #[test]
    fn rewrites_lctn_master_special_reference_formids() {
        let mut record = lctn_record_with_raw("LCSR", lcsr_row(0x003D4B0D, 0x00001234, 0x0025DA15));

        assert!(rewrite_lctn_record(&mut record, &target_map(), false, None));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x073D4B0D
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x07001234
        );
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            0x0725DA15
        );
    }

    #[test]
    fn rewrites_lctn_master_worldspace_cell_formid() {
        let mut raw = SmallVec::<[u8; 32]>::new();
        raw.extend_from_slice(&0x0025DA15_u32.to_le_bytes());
        raw.extend_from_slice(&(-18_i16).to_le_bytes());
        raw.extend_from_slice(&(-37_i16).to_le_bytes());

        assert!(rewrite_lctn_bytes(
            &mut raw,
            LctnRawLayout::FirstFormId,
            &target_map(),
            None,
            0
        ));
        assert_eq!(
            u32::from_le_bytes(raw[0..4].try_into().unwrap()),
            0x0725DA15
        );
    }

    #[test]
    fn prunes_lctn_special_reference_row_with_unmapped_worldspace() {
        let mut record = lctn_record_with_raw("LCSR", lcsr_row(0x003D4B0D, 0x00001234, 0x0025DA15));

        assert!(rewrite_lctn_record(
            &mut record,
            &target_map_without_world(),
            false,
            None
        ));
        assert!(
            record.fields.is_empty(),
            "row should be removed rather than leaving a raw source-local worldspace"
        );
    }

    #[test]
    fn prunes_only_unmapped_lctn_special_reference_rows() {
        let mut raw = lcsr_row(0x003D4B0D, 0x00001234, 0x0025DA15);
        raw.extend_from_slice(&lcsr_row(0x003D4B0D, 0x00001234, 0x00BEEF01));
        let mut record = lctn_record_with_raw("LCSR", raw);

        assert!(rewrite_lctn_record(&mut record, &target_map(), false, None));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(bytes.len(), 16, "only the mapped row should remain");
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x073D4B0D
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x07001234
        );
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            0x0725DA15
        );
    }

    #[test]
    fn prunes_lctn_worldspace_cells_with_unmapped_worldspace() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x0025DA15_u32.to_le_bytes());
        raw.extend_from_slice(&(-18_i16).to_le_bytes());
        raw.extend_from_slice(&(-37_i16).to_le_bytes());
        let mut record = lctn_record_with_raw("LCEC", raw);

        assert!(rewrite_lctn_record(
            &mut record,
            &target_map_without_world(),
            false,
            None
        ));
        assert!(
            record.fields.is_empty(),
            "LCEC should be removed rather than leaving a raw source-local worldspace"
        );
    }

    #[test]
    fn augment_registers_own_record_object_ids_outside_mapper() {
        let mut encoded_targets = FxHashMap::default();
        augment_with_own_encoded_form_ids(&mut encoded_targets, [0x07001234, 0x0725DA15], 7);
        assert_eq!(encoded_targets.get(&0x001234), Some(&0x07001234));
        assert_eq!(encoded_targets.get(&0x25DA15), Some(&0x0725DA15));
    }

    #[test]
    fn augment_does_not_override_existing_mapper_target() {
        let mut encoded_targets = FxHashMap::default();
        encoded_targets.insert(0x001234, 0x07ABCDEF);
        augment_with_own_encoded_form_ids(&mut encoded_targets, [0x07001234], 7);
        assert_eq!(encoded_targets.get(&0x001234), Some(&0x07ABCDEF));
    }

    #[test]
    fn augment_ignores_master_resident_records() {
        let mut encoded_targets = FxHashMap::default();
        // index 0 (e.g. a Fallout4.esm override) is not an own record.
        augment_with_own_encoded_form_ids(&mut encoded_targets, [0x0000_0ABC], 7);
        assert!(encoded_targets.get(&0x000ABC).is_none());
    }

    #[test]
    fn keeps_special_reference_row_when_ref_is_own_record_via_cell_slice() {
        // LocRefType + WorldCell are mapper-resident; the placed Ref reached the
        // target through the cell-slice copy and is absent from the mapper.
        let mut base_targets = FxHashMap::default();
        base_targets.insert(0x3D4B0D, 0x073D4B0D); // LocRefType (LCRT)
        base_targets.insert(0x25DA15, 0x0725DA15); // WorldCell (WRLD)

        // In the pre-copy fixup phase, special-ref rows are deferred because the
        // placed refs have not been copied into the output plugin yet.
        let mut deferred =
            lctn_record_with_raw("LCSR", lcsr_row(0x003D4B0D, 0x00001234, 0x0025DA15));
        assert!(!rewrite_lctn_record(
            &mut deferred,
            &base_targets,
            true,
            None
        ));
        let FieldValue::Bytes(bytes) = &deferred.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x00001234,
            "deferred special ref keeps its source-local placed ref"
        );

        // Post-copy, a missing own ref still prunes the row.
        let mut pruned = lctn_record_with_raw("LCSR", lcsr_row(0x003D4B0D, 0x00001234, 0x0025DA15));
        assert!(rewrite_lctn_record(&mut pruned, &base_targets, false, None));
        assert!(
            pruned.fields.is_empty(),
            "special-ref row dropped when its placed ref is missing from the map"
        );

        let mut fixed_targets = base_targets;
        augment_with_own_encoded_form_ids(&mut fixed_targets, [0x07001234], 7);
        let mut kept = lctn_record_with_raw("LCSR", lcsr_row(0x003D4B0D, 0x00001234, 0x0025DA15));
        assert!(rewrite_lctn_record(&mut kept, &fixed_targets, false, None));
        let FieldValue::Bytes(bytes) = &kept.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(bytes.len(), 16, "the special-ref row is retained");
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x073D4B0D
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x07001234
        );
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            0x0725DA15
        );
    }

    #[test]
    fn keeps_persistent_lctn_row_when_ref_location_and_grid_match() {
        let location = 0x0700414F;
        let world = 0x0725DA15;
        let cell = 0x07A035C5;
        let reference = 0x07001234;
        let placed = placed_record(
            "REFR",
            reference,
            RECORD_FLAG_PERSISTENT,
            Some(location),
            -37.0 * 4096.0,
            -18.0 * 4096.0,
        );
        let items = exterior_persistence_items(world, cell, placed);
        let index = LctnPersistenceIndex::from_items(&items, 7);
        let mut record = lctn_record_with_raw("LCPR", lcpr_row(0x00001234, 0x0025DA15, -18, -37));

        assert!(rewrite_lctn_record(
            &mut record,
            &target_map(),
            false,
            Some(&index)
        ));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(bytes.len(), 12);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            reference
        );
    }

    #[test]
    fn keeps_special_lctn_row_without_ref_xlcn_when_grid_matches() {
        let world = 0x0725DA15;
        let cell = 0x07A035C5;
        let reference = 0x070B1051;
        let map_marker_ref_type = 0x0702271F;
        let placed = placed_record(
            "REFR",
            reference,
            RECORD_FLAG_PERSISTENT,
            None,
            -25.5 * 4096.0,
            22.5 * 4096.0,
        );
        let items = exterior_persistence_items(world, cell, placed);
        let index = LctnPersistenceIndex::from_items(&items, 7);
        let mut targets = FxHashMap::default();
        targets.insert(0x02271F, map_marker_ref_type);
        targets.insert(0x0B1051, reference);
        targets.insert(0x25DA15, world);
        let mut record = lctn_record_with_raw(
            "LCSR",
            lcsr_row_at(0x0002271F, 0x000B1051, 0x0025DA15, 22, -26),
        );

        assert!(rewrite_lctn_record(
            &mut record,
            &targets,
            false,
            Some(&index)
        ));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(bytes.len(), 16);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            map_marker_ref_type
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            reference
        );
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), world);
    }

    #[test]
    fn keeps_lcpr_row_for_nonpersistent_actor() {
        let location = 0x0700414F;
        let world = 0x0725DA15;
        let cell = 0x07A035C5;
        let reference = 0x07001234;
        let placed = placed_record(
            "ACHR",
            reference,
            0,
            Some(location),
            -37.0 * 4096.0,
            -18.0 * 4096.0,
        );
        let items = exterior_persistence_items(world, cell, placed);
        let index = LctnPersistenceIndex::from_items(&items, 7);
        let mut record = lctn_record_with_raw("LCPR", lcpr_row(0x00001234, 0x0025DA15, -18, -37));

        assert!(rewrite_lctn_record(
            &mut record,
            &target_map(),
            false,
            Some(&index)
        ));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("temporary actor LCPR row must be retained");
        };
        assert_eq!(bytes.len(), 12);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            reference
        );
    }

    #[test]
    fn prunes_lcpr_row_for_nonpersistent_nonactor() {
        let location = 0x0700414F;
        let world = 0x0725DA15;
        let cell = 0x07A035C5;
        let placed = placed_record(
            "REFR",
            0x07001234,
            0,
            Some(location),
            -37.0 * 4096.0,
            -18.0 * 4096.0,
        );
        let items = exterior_persistence_items(world, cell, placed);
        let index = LctnPersistenceIndex::from_items(&items, 7);
        let mut record = lctn_record_with_raw("LCPR", lcpr_row(0x00001234, 0x0025DA15, -18, -37));

        assert!(rewrite_lctn_record(
            &mut record,
            &target_map(),
            false,
            Some(&index)
        ));

        assert!(record.fields.is_empty());
    }

    #[test]
    fn keeps_lcsr_static_row_when_ref_is_not_persistent() {
        // The Whitespring greeter case: a non-persistent ACHR special-ref in an
        // interior cell. LCSR (static) refs are non-persistent by design (vanilla
        // FO4 carries both), so the row must be KEPT — pruning it leaves the
        // quest's location alias unable to fill, aborting the dialogue quest.
        let location = 0x0700414F;
        let cell = 0x076240BB;
        let reference = 0x07646883;
        let ref_type = 0x07653007;
        let placed = placed_record("ACHR", reference, 0, Some(location), 0.0, 0.0);
        let items = interior_persistence_items(cell, placed);
        let index = LctnPersistenceIndex::from_items(&items, 7);
        let mut targets = target_map();
        targets.insert(0x6240BB, cell);
        targets.insert(0x653007, ref_type);
        targets.insert(0x646883, reference);
        let mut record = lctn_record_with_raw(
            "LCSR",
            lcsr_row_at(0x00653007, 0x00646883, 0x006240BB, i16::MAX, i16::MAX),
        );

        assert!(rewrite_lctn_record(
            &mut record,
            &targets,
            false,
            Some(&index)
        ));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("static-ref row must be retained for a non-persistent interior ref");
        };
        assert_eq!(bytes.len(), 16);
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            reference
        );
    }

    #[test]
    fn prunes_lctn_row_when_ref_points_at_other_location() {
        let world = 0x0725DA15;
        let cell = 0x07A035C5;
        let placed = placed_record(
            "REFR",
            0x07001234,
            RECORD_FLAG_PERSISTENT,
            Some(0x07004150),
            -37.0 * 4096.0,
            -18.0 * 4096.0,
        );
        let items = exterior_persistence_items(world, cell, placed);
        let index = LctnPersistenceIndex::from_items(&items, 7);
        let mut record = lctn_record_with_raw("LCPR", lcpr_row(0x00001234, 0x0025DA15, -18, -37));

        assert!(rewrite_lctn_record(
            &mut record,
            &target_map(),
            false,
            Some(&index)
        ));

        assert!(
            record.fields.is_empty(),
            "a row under location 00414F cannot name a ref whose XLCN is another location"
        );
    }

    #[test]
    fn keeps_interior_persistent_row_by_cell_parentage() {
        let location = 0x0700414F;
        let cell = 0x076240BB;
        let reference = 0x07001234;
        let placed = placed_record(
            "ACHR",
            reference,
            RECORD_FLAG_PERSISTENT,
            Some(location),
            0.0,
            0.0,
        );
        let items = interior_persistence_items(cell, placed);
        let index = LctnPersistenceIndex::from_items(&items, 7);
        let raw = lcpr_row(0x00001234, 0x076240BB, i16::MAX, i16::MAX);
        let mut record = lctn_record_with_raw("LCPR", raw);
        let mut targets = target_map();
        targets.insert(0x6240BB, 0x076240BB);

        assert!(rewrite_lctn_record(
            &mut record,
            &targets,
            false,
            Some(&index)
        ));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(bytes.len(), 12);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            reference
        );
    }

    #[test]
    fn preserves_xlcn_on_nonpersistent_actors() {
        let location = 0x0700414F;
        let mut items = vec![
            ParsedItem::Record(placed_record(
                "REFR",
                0x07001234,
                0,
                Some(location),
                0.0,
                0.0,
            )),
            ParsedItem::Record(placed_record(
                "ACHR",
                0x07001235,
                0,
                Some(location),
                0.0,
                0.0,
            )),
            ParsedItem::Record(placed_record(
                "REFR",
                0x07001236,
                RECORD_FLAG_PERSISTENT,
                Some(location),
                0.0,
                0.0,
            )),
        ];
        if let ParsedItem::Record(record) = &mut items[0] {
            record.raw_payload = Some(Bytes::from_static(b"stale"));
        }
        let mut changed_form_ids = SmallVec::<[u32; 4]>::new();

        assert_eq!(
            strip_nonpersistent_nonactor_placed_ref_xlcn_from_items(
                &mut items,
                &mut changed_form_ids
            ),
            1
        );
        assert_eq!(changed_form_ids.as_slice(), &[0x07001234]);
        let ParsedItem::Record(nonpersistent) = &items[0] else {
            panic!("expected record");
        };
        assert!(
            nonpersistent
                .subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "XLCN")
        );
        assert!(nonpersistent.raw_payload.is_none());
        let ParsedItem::Record(nonpersistent_actor) = &items[1] else {
            panic!("expected actor record");
        };
        assert!(
            nonpersistent_actor
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature.as_str() == "XLCN")
        );
        let ParsedItem::Record(persistent) = &items[2] else {
            panic!("expected record");
        };
        assert!(
            persistent
                .subrecords
                .iter()
                .any(|subrecord| subrecord.signature.as_str() == "XLCN")
        );
    }

    #[test]
    fn leaves_already_encoded_target_formids_unchanged() {
        let mut raw = 0x073D4B0D_u32.to_le_bytes().to_vec();
        assert!(!rewrite_formid_at(&mut raw, 0, &target_map()));
        assert_eq!(
            u32::from_le_bytes(raw[0..4].try_into().unwrap()),
            0x073D4B0D
        );
    }

    #[test]
    fn tags_missing_cell_location_from_lctn_worldspace_cells() {
        let world = 0x0725DA15;
        let location = 0x077FFFCC;
        let cell = 0x072628FE;
        let mut items = vec![
            ParsedItem::Record(lctn_cell_record(location, world, 13, 42)),
            world_children_group(
                world,
                vec![ParsedItem::Record(parsed_cell_record(cell, 13, 42, None))],
            ),
        ];
        let mut locations = FxHashMap::default();
        let mut conflicts = rustc_hash::FxHashSet::default();
        collect_lctn_world_cell_locations(&items, &mut locations, &mut conflicts);
        assert!(conflicts.is_empty());

        let mut changed_form_ids = Vec::new();
        assert_eq!(
            tag_cell_locations_in_items(&mut items, None, &locations, &mut changed_form_ids),
            1
        );
        assert_eq!(changed_form_ids, vec![cell]);
        let cell_record = first_cell(&items).expect("tagged cell");
        let xlcn = cell_record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "XLCN")
            .expect("XLCN");
        assert_eq!(
            u32::from_le_bytes(xlcn.data[0..4].try_into().unwrap()),
            location
        );
    }

    #[test]
    fn does_not_overwrite_existing_cell_location() {
        let world = 0x0725DA15;
        let location = 0x077FFFCC;
        let existing_location = 0x07012345;
        let cell = 0x072628FE;
        let mut items = vec![
            ParsedItem::Record(lctn_cell_record(location, world, 13, 42)),
            world_children_group(
                world,
                vec![ParsedItem::Record(parsed_cell_record(
                    cell,
                    13,
                    42,
                    Some(existing_location),
                ))],
            ),
        ];
        let mut locations = FxHashMap::default();
        let mut conflicts = rustc_hash::FxHashSet::default();
        collect_lctn_world_cell_locations(&items, &mut locations, &mut conflicts);

        let mut changed_form_ids = Vec::new();
        assert_eq!(
            tag_cell_locations_in_items(&mut items, None, &locations, &mut changed_form_ids),
            0
        );
        assert!(changed_form_ids.is_empty());
        let cell_record = first_cell(&items).expect("cell");
        let xlcn = cell_record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "XLCN")
            .expect("XLCN");
        assert_eq!(
            u32::from_le_bytes(xlcn.data[0..4].try_into().unwrap()),
            existing_location
        );
    }
}
