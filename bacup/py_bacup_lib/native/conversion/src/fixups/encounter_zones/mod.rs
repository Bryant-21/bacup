//! FO76→FO4 encounter-zone synthesis (post-copy).
//!
//! FO76 has no `ECZN`; the role lives on `LCTN` (band on `DATA`, parent on
//! `PNAM`, cell footprint on `LCEC`, keywords on `KWDA`). This synthesizes one
//! FO4 `ECZN` per qualifying Location, stamps `CELL.XEZN` on its footprint
//! cells, and rewrites workshop `LCTN` keywords to FO4's settlement contract.
//!
//! Not a registry `Fixup`: exterior CELLs only exist after the post-fixup
//! cell-slice copy. `ConversionRun::synthesize_encounter_zones` calls this after
//! persistent-cell synthesis, with the source handle still open for LCTN reads.

mod build;
mod cell_index;
pub mod model;

use std::collections::{HashMap, HashSet};

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

use build::{build_eczn_record, rebuild_keyword_fields};
use cell_index::{PLACED_SIGNATURES, build_target_index};
use model::{
    KW_WORKSHOP_PUBLIC, KW_WORKSHOP_SHELTER, WorkshopClass, clamp_level, classify,
    decode_lcec_footprint, decode_lctn_info, eczn_editor_id, eczn_flags, resolve_band,
};

const FALLOUT4_ESM: &str = "Fallout4.esm";
const SEVENTYSIX_ESM: &str = "SeventySix.esm";
const SYNTH_ECZN_PLUGIN: &str = "__synth_eczn__";

struct ZonePlan {
    source_lctn: FormKey,
    class: WorkshopClass,
    own_band: Option<(u8, u8)>,
    footprint: Vec<(FormKey, i32, i32)>,
    editor_id: String,
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        FieldValue::List(items) => items.iter().find_map(first_form_key),
        _ => None,
    }
}

fn output_to_source_object_ids(
    mapper: &FormKeyMapper,
    output_plugin: crate::sym::Sym,
) -> HashMap<u32, u32> {
    mapper
        .source_to_target_iter()
        .filter_map(|(source, target)| {
            (target.plugin == output_plugin)
                .then_some((target.local & 0x00FF_FFFF, source.local & 0x00FF_FFFF))
        })
        .collect()
}

fn source_object_id_for_target(
    target_local: u32,
    output_to_source: &HashMap<u32, u32>,
    config: &FixupConfig,
) -> Option<u32> {
    output_to_source
        .get(&(target_local & 0x00FF_FFFF))
        .copied()
        .or_else(|| {
            config
                .preserve_source_ids
                .then_some(target_local & 0x00FF_FFFF)
        })
}

fn source_location_assignments_for_target_cells(
    session: &mut PluginSession,
    schema_source: &std::sync::Arc<crate::schema::AuthoringSchema>,
    interner: &crate::sym::StringInterner,
    target: &cell_index::TargetIndex,
    mapper: &FormKeyMapper,
    config: &FixupConfig,
    output_to_source: &HashMap<u32, u32>,
    actor_cells: &HashSet<u32>,
) -> Result<(HashSet<u32>, HashMap<u32, (u32, u32)>), FixupError> {
    use rayon::prelude::*;

    let mut source_lctn_locals = HashSet::new();
    let mut cell_source_lctn_assignment: HashMap<u32, (u32, u32)> = HashMap::new();

    let lctn_sig = SigCode::from_str("LCTN").map_err(FixupError::SchemaError)?;
    let source_lctn_fks = session
        .source_form_keys_of_sig(lctn_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let (source_masters, source_plugin_name) = match session.source_slot_opt() {
        Some(slot) => (
            slot.parsed.header.masters.clone(),
            slot.parsed.plugin_name.clone(),
        ),
        None => (Vec::new(), String::new()),
    };

    let decoded_lctns = {
        let source_id = session
            .source_id()
            .ok_or_else(|| FixupError::HandleError("missing source handle".into()))?;
        let source_scan = session
            .handle_raw_scan(source_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let raw_form_ids = source_scan.raw_form_ids_of_sig(lctn_sig);
        debug_assert_eq!(source_lctn_fks.len(), raw_form_ids.len());
        source_lctn_fks
            .par_iter()
            .zip(raw_form_ids.par_iter())
            .filter_map(|(fk, raw_form_id)| {
                let record = source_scan
                    .record_decoded(*raw_form_id, fk, schema_source.as_ref(), interner)?
                    .ok()?;
                let info = decode_lctn_info(&record, interner);
                let source_lctn = info.form_key.local & 0x00FF_FFFF;
                let parent = info.parent.map(|parent| parent.local & 0x00FF_FFFF);
                let footprint =
                    decode_lcec_footprint(&record, interner, &source_masters, &source_plugin_name);
                Some((source_lctn, parent, footprint))
            })
            .collect::<Vec<_>>()
    };
    let mut parents: HashMap<u32, Option<u32>> = HashMap::new();
    let mut footprints: Vec<(u32, Vec<(FormKey, i32, i32)>)> = Vec::new();
    for (source_lctn, parent, footprint) in decoded_lctns {
        parents.insert(source_lctn, parent);
        if !footprint.is_empty() {
            footprints.push((source_lctn, footprint));
        }
    }

    let depth_of = |start: u32| -> u32 {
        let (mut depth, mut current, mut seen) = (
            0u32,
            parents.get(&start).and_then(|parent| *parent),
            vec![start],
        );
        while let Some(parent) = current {
            if seen.contains(&parent) {
                break;
            }
            seen.push(parent);
            depth += 1;
            current = parents.get(&parent).and_then(|next| *next);
        }
        depth
    };

    for (source_lctn, footprint) in footprints {
        let depth = depth_of(source_lctn);
        for (world_fk, grid_x, grid_y) in footprint {
            let world_objid = mapper
                .lookup(world_fk)
                .map(|fk| fk.local)
                .unwrap_or(world_fk.local)
                & 0x00FF_FFFF;
            let Some(cell_objid) = target.grid.get(&(world_objid, grid_x, grid_y)).copied() else {
                continue;
            };
            if !actor_cells.contains(&cell_objid) {
                continue;
            }
            let entry = cell_source_lctn_assignment
                .entry(cell_objid)
                .or_insert((u32::MIN, source_lctn));
            if depth >= entry.0 {
                *entry = (depth, source_lctn);
            }
        }
    }

    for (cell_objid, lctn_objid) in &target.cell_locations {
        if !actor_cells.contains(cell_objid) {
            continue;
        }
        let Some(source_lctn) = source_object_id_for_target(*lctn_objid, output_to_source, config)
        else {
            continue;
        };
        cell_source_lctn_assignment.insert(*cell_objid, (u32::MAX, source_lctn));
    }

    // Fallback: a target cell that lost its XLCN and has no LCEC footprint hit
    // can still resolve through the SOURCE cell's own XLCN (ids preserved).
    if !source_plugin_name.is_empty() {
        let source_plugin = interner.intern(&source_plugin_name);
        for cell_objid in actor_cells {
            if cell_source_lctn_assignment.contains_key(cell_objid) {
                continue;
            }
            let fk = FormKey {
                local: *cell_objid,
                plugin: source_plugin,
            };
            let Ok(record) = session.source_record_decoded(&fk, schema_source.as_ref(), interner)
            else {
                continue;
            };
            if record.sig.as_str() != "CELL" {
                continue;
            }
            let Some(xlcn) = cell_xlcn_local(&record) else {
                continue;
            };
            cell_source_lctn_assignment.insert(*cell_objid, (u32::MAX - 1, xlcn));
        }
    }

    source_lctn_locals.extend(
        cell_source_lctn_assignment
            .values()
            .map(|(_, source_lctn)| *source_lctn),
    );
    Ok((source_lctn_locals, cell_source_lctn_assignment))
}

/// The FO76 `EditorLocationHasKeyword` location of each placed actor (design 2.7):
/// its own XEZN location, else its cell's location claim. Output ref object id ->
/// source LCTN object id.
fn merge_placed_actor_locations(
    actors: &[cell_index::PlacedActorRef],
    xezn: &HashMap<u32, u32>,
    cell_locations: &HashMap<u32, (u32, u32)>,
) -> Vec<(u32, u32)> {
    actors
        .iter()
        .filter_map(|actor| {
            xezn.get(&actor.ref_objid)
                .copied()
                .or_else(|| cell_locations.get(&actor.cell_objid).map(|(_, location)| *location))
                .map(|location| (actor.ref_objid, location))
        })
        .collect()
}

/// Source XEZN of each placed actor when it names an LCTN, as FO76 placed refs do.
fn source_actor_xezn_locations(
    session: &mut PluginSession,
    target: &cell_index::TargetIndex,
    output_to_source: &HashMap<u32, u32>,
) -> Result<HashMap<u32, u32>, FixupError> {
    let source_id = session
        .source_id()
        .ok_or_else(|| FixupError::HandleError("missing source handle".into()))?;
    let own_index = session
        .source_slot_opt()
        .map_or(0, |slot| slot.parsed.header.masters.len() as u32);
    let scan = session
        .handle_raw_scan(source_id)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let lctn_sig = SigCode::from_str("LCTN").map_err(FixupError::SchemaError)?;
    let locations: HashSet<u32> = scan.raw_form_ids_of_sig(lctn_sig).into_iter().collect();
    let mut out = HashMap::new();
    for actor in &target.placed_actors {
        let Some(source) = output_to_source.get(&actor.ref_objid) else { continue };
        let zone = scan
            .with_record(own_index << 24 | source, |record| {
                esp_authoring_core::plugin_runtime::effective_subrecords_for_record(record)
                    .iter()
                    .find(|field| field.signature == "XEZN")
                    .and_then(|field| field.data.get(0..4))
                    .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
            })
            .flatten();
        if let Some(zone) = zone
            && locations.contains(&zone)
        {
            out.insert(actor.ref_objid, zone & 0x00FF_FFFF);
        }
    }
    Ok(out)
}

pub fn placed_actor_fo76_locations(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<Vec<(u32, u32)>, FixupError> {
    let source_schema = config
        .source_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing source schema".into()))?;
    session.refresh_target_read_indexes();
    let target = build_target_index(&session.target_slot().parsed.root_items, true);
    if target.placed_actors.is_empty() {
        return Ok(Vec::new());
    }
    // The retired ESS pass reserved these at this point; vendor dialogue allocates next.
    mapper.reserve_object_ids(target.all_object_ids.iter().copied());
    let output_plugin = mapper.output_plugin_sym();
    let output_to_source = output_to_source_object_ids(mapper, output_plugin);
    let actor_cells: HashSet<u32> = target.placed_actors.iter().map(|actor| actor.cell_objid).collect();
    let (_, cell_locations) = source_location_assignments_for_target_cells(
        session,
        &source_schema,
        mapper.interner,
        &target,
        mapper,
        config,
        &output_to_source,
        &actor_cells,
    )?;
    let xezn = source_actor_xezn_locations(session, &target, &output_to_source)?;
    Ok(merge_placed_actor_locations(&target.placed_actors, &xezn, &cell_locations))
}

fn cell_xlcn_local(record: &Record) -> Option<u32> {
    let field = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "XLCN")?;
    match &field.value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF;
            (raw != 0).then_some(raw)
        }
        value => first_form_key(value)
            .map(|fk| fk.local & 0x00FF_FFFF)
            .filter(|local| *local != 0),
    }
}

fn eczn_location_object_id(record: &Record, interner: &StringInterner) -> Option<u32> {
    let data = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "DATA")?;
    if let FieldValue::Bytes(bytes) = &data.value {
        if bytes.len() >= 8 {
            let raw = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            let object_id = raw & 0x00FF_FFFF;
            return (object_id != 0).then_some(object_id);
        }
    }
    let FieldValue::Struct(fields) = &data.value else {
        return first_form_key(&data.value)
            .map(|fk| fk.local & 0x00FF_FFFF)
            .filter(|object_id| *object_id != 0);
    };
    for (name, value) in fields {
        if interner
            .resolve(*name)
            .is_some_and(|name| name.eq_ignore_ascii_case("location"))
        {
            if let Some(fk) = first_form_key(value) {
                let object_id = fk.local & 0x00FF_FFFF;
                if object_id != 0 {
                    return Some(object_id);
                }
            }
        }
    }
    fields
        .iter()
        .filter_map(|(_, value)| first_form_key(value))
        .map(|fk| fk.local & 0x00FF_FFFF)
        .find(|object_id| *object_id != 0)
}

fn points_to_known_eczn(
    fk: FormKey,
    output_plugin: crate::sym::Sym,
    output_eczn_object_ids: &HashSet<u32>,
    master_eczn_object_ids: &HashMap<crate::sym::Sym, HashSet<u32>>,
) -> bool {
    let object_id = fk.local & 0x00FF_FFFF;
    if fk.plugin == output_plugin {
        return output_eczn_object_ids.contains(&object_id);
    }
    master_eczn_object_ids
        .get(&fk.plugin)
        .is_some_and(|ids| ids.contains(&object_id))
}

/// Final target-only cleanup for placed-record `XEZN`.
///
/// Early encounter-zone synthesis can repoint copied placed refs, but later
/// placed-child repair/copy paths can leave or restore FO76-style `XEZN` fields
/// that still name `LCTN`. At this point the output tree is complete, so build
/// the authoritative map from existing target `ECZN.DATA.Location` and normalize
/// every placed record once.
pub fn finalize_placed_xezn_targets(
    session: &mut PluginSession,
    config: &FixupConfig,
    interner: &StringInterner,
) -> Result<FixupReport, FixupError> {
    finalize_placed_xezn_targets_with_refr_candidates(session, config, interner, None)
}

pub(crate) fn finalize_placed_xezn_targets_for_refr_candidates(
    session: &mut PluginSession,
    config: &FixupConfig,
    interner: &StringInterner,
    refr_candidates: &[FormKey],
) -> Result<FixupReport, FixupError> {
    finalize_placed_xezn_targets_with_refr_candidates(
        session,
        config,
        interner,
        Some(refr_candidates),
    )
}

fn finalize_placed_xezn_targets_with_refr_candidates(
    session: &mut PluginSession,
    config: &FixupConfig,
    interner: &StringInterner,
    refr_candidates: Option<&[FormKey]>,
) -> Result<FixupReport, FixupError> {
    use rayon::prelude::*;

    let mut report = FixupReport::empty();
    let target_schema = config
        .target_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing target schema".into()))?;
    let eczn_sig = SigCode::from_str("ECZN").map_err(FixupError::SchemaError)?;
    let xezn_sig = SubrecordSig::from_str("XEZN").map_err(FixupError::SchemaError)?;
    let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
    let output_plugin = interner.intern(&output_plugin_name);

    let target_masters = session.target_masters().to_vec();
    let eczn_fks = session
        .form_keys_of_sig(eczn_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut lctn_to_eczn: HashMap<u32, FormKey> = HashMap::new();
    let mut output_eczn_object_ids: HashSet<u32> = HashSet::new();
    for eczn_fk in eczn_fks {
        output_eczn_object_ids.insert(eczn_fk.local & 0x00FF_FFFF);
        let Ok(record) = session.record_decoded(&eczn_fk, target_schema.as_ref(), interner) else {
            continue;
        };
        let Some(location_objid) = eczn_location_object_id(&record, interner) else {
            continue;
        };
        lctn_to_eczn.entry(location_objid).or_insert(eczn_fk);
    }

    let mut master_eczn_object_ids: HashMap<crate::sym::Sym, HashSet<u32>> = HashMap::new();
    for (master_name, handle_id) in target_masters
        .iter()
        .zip(config.target_master_handle_ids.iter().copied())
    {
        let fks = session
            .form_keys_of_sig_in_handle(handle_id, eczn_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if !fks.is_empty() {
            master_eczn_object_ids.insert(
                interner.intern(master_name),
                fks.into_iter().map(|fk| fk.local & 0x00FF_FFFF).collect(),
            );
        }
    }

    let mut placed_fks = Vec::new();
    for placed_sig in PLACED_SIGNATURES {
        if placed_sig == "REFR" {
            if let Some(refr_candidates) = refr_candidates {
                placed_fks.extend_from_slice(refr_candidates);
                continue;
            }
        }
        let sig = SigCode::from_str(placed_sig).map_err(FixupError::SchemaError)?;
        placed_fks.extend(
            session
                .form_keys_of_sig(sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?,
        );
    }
    let placed_changes: Vec<Record> = {
        let view = session
            .target_read_view()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        placed_fks
            .par_iter()
            .filter_map(|placed_fk| {
                if !view.record_has_any_subrecord(placed_fk, &["XEZN"], interner) {
                    return None;
                }
                let mut record = view
                    .record_decoded(placed_fk, target_schema.as_ref(), interner)
                    .ok()?;
                let Some(idx) = record.fields.iter().position(|field| field.sig == xezn_sig) else {
                    return None;
                };
                let current = match &record.fields[idx].value {
                    FieldValue::FormKey(fk) => *fk,
                    _ => return None,
                };
                let current_objid = current.local & 0x00FF_FFFF;
                if current_objid == 0 {
                    return None;
                }

                let changed = if let Some(eczn_fk) = lctn_to_eczn.get(&current_objid).copied() {
                    if current == eczn_fk {
                        false
                    } else {
                        record.fields[idx].value = FieldValue::FormKey(eczn_fk);
                        true
                    }
                } else if points_to_known_eczn(
                    current,
                    output_plugin,
                    &output_eczn_object_ids,
                    &master_eczn_object_ids,
                ) {
                    false
                } else {
                    record.fields.remove(idx);
                    true
                };
                changed.then_some(record)
            })
            .collect()
    };

    report.records_changed = session
        .replace_records_contents(placed_changes, target_schema.as_ref(), interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    Ok(report)
}

/// `identity_resolve` enables the slice (bounded cell-slice) path: the host run
/// never ran `translate_all`, so the mapper is empty. With `preserve_source_ids`
/// the slice keeps object-ids 1:1, so we resolve a source `LCTN` to its target
/// by identity (against actual target presence) and reserve every existing
/// target id so a synthesized ECZN never steals a preserved LCTN id. It is a
/// no-op on the whole-plugin path, where `lookup` always hits after
/// `translate_all`.
pub fn synthesize_encounter_zones(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    identity_resolve: bool,
) -> Result<FixupReport, FixupError> {
    use rayon::prelude::*;

    let mut report = FixupReport::empty();
    let log_timing = |name: &str, started: std::time::Instant| {
        eprintln!(
            "[eczn_timing] {name} elapsed_ms={}",
            started.elapsed().as_millis()
        );
    };
    let phase_started = std::time::Instant::now();
    let output_plugin = mapper.output_plugin_sym();
    let lctn_sig = SigCode::from_str("LCTN").map_err(FixupError::SchemaError)?;
    let eczn_sig = SigCode::from_str("ECZN").map_err(FixupError::SchemaError)?;
    let xezn_sig = SubrecordSig::from_str("XEZN").map_err(FixupError::SchemaError)?;

    let target_schema = config
        .target_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing target schema".into()))?;
    let source_schema = config
        .source_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing source schema".into()))?;
    let started = std::time::Instant::now();
    let source_lctn_fks = session
        .source_form_keys_of_sig(lctn_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    log_timing("source_form_keys_of_sig", started);

    // Source plugin master order + name, needed to resolve the raw `world`
    // form_id inside a rawified LCEC footprint (see model::decode_lcec_footprint).
    let (source_masters, source_plugin_name) = match session.source_slot_opt() {
        Some(slot) => (
            slot.parsed.header.masters.clone(),
            slot.parsed.plugin_name.clone(),
        ),
        None => (Vec::new(), String::new()),
    };

    let started = std::time::Instant::now();
    let target = build_target_index(&session.target_slot().parsed.root_items, true);
    mapper.reserve_object_ids(target.all_object_ids.iter().copied());
    log_timing("build_target_index", started);
    let target_index = &target.grid;

    // Target-LCTN object-ids referenced by an interior cell's XLCN. Lets an
    // interior-only Location (band present, empty exterior footprint) qualify
    // for an ECZN so its interior cells can be stamped.
    let interior_lctn_targets: rustc_hash::FxHashSet<u32> = target
        .interior_cells
        .iter()
        .filter_map(|(_, xlcn)| *xlcn)
        .collect();

    // Target object-ids named by some placed record's XEZN. Lets a zero-band,
    // footprint-less, interior-cell-less Location still synthesize an ECZN when a
    // placed ref points its encounter zone at it — so the repoint step has a
    // target instead of stripping. Keyed by TARGET object-id (post-copy refs are
    // already remapped); compared against each LCTN's resolved target id.
    let placed_xezn_targets = &target.placed_xezn_targets;

    let started = std::time::Instant::now();
    let decoded_plans = {
        let source_id = session
            .source_id()
            .ok_or_else(|| FixupError::HandleError("missing source handle".into()))?;
        let scan_started = std::time::Instant::now();
        let source_scan = session
            .handle_raw_scan(source_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        log_timing("source_raw_scan_prime", scan_started);
        let raw_form_ids = source_scan.raw_form_ids_of_sig(lctn_sig);
        debug_assert_eq!(source_lctn_fks.len(), raw_form_ids.len());
        let decode = |(fk, raw_form_id): (&FormKey, &u32)| {
            let rec = match source_scan.record_decoded(
                *raw_form_id,
                fk,
                source_schema.as_ref(),
                mapper.interner,
            ) {
                Some(Ok(record)) => record,
                _ => return None,
            };
            let info = decode_lctn_info(&rec, mapper.interner);
            let editor_id = rec
                .eid
                .and_then(|sym| mapper.interner.resolve(sym))
                .map(str::to_string)
                .unwrap_or_default();
            let class = classify(info.location_type, &info.keyword_locals, &editor_id);
            let footprint =
                decode_lcec_footprint(&rec, mapper.interner, &source_masters, &source_plugin_name);
            Some((
                info.form_key.local,
                info.own_band,
                info.parent.map(|parent| parent.local),
                ZonePlan {
                    source_lctn: info.form_key,
                    class,
                    own_band: info.own_band,
                    footprint,
                    editor_id,
                },
            ))
        };
        if source_lctn_fks.len() < 64 {
            source_lctn_fks
                .iter()
                .zip(&raw_form_ids)
                .filter_map(decode)
                .collect::<Vec<_>>()
        } else {
            source_lctn_fks
                .par_iter()
                .zip(raw_form_ids.par_iter())
                .filter_map(decode)
                .collect::<Vec<_>>()
        }
    };
    log_timing("decode_source_lctns", started);
    let mut bands: HashMap<u32, Option<(u8, u8)>> = HashMap::new();
    let mut parents: HashMap<u32, Option<u32>> = HashMap::new();
    let mut plans: Vec<ZonePlan> = Vec::with_capacity(decoded_plans.len());
    for (local, own_band, parent, plan) in decoded_plans {
        bands.insert(local, own_band);
        parents.insert(local, parent);
        plans.push(plan);
    }

    let depth_of = |start: u32| -> u32 {
        let (mut d, mut cur, mut seen) = (0u32, parents.get(&start).and_then(|p| *p), vec![start]);
        while let Some(p) = cur {
            if seen.contains(&p) {
                break;
            }
            seen.push(p);
            d += 1;
            cur = parents.get(&p).and_then(|x| *x);
        }
        d
    };

    let mut cell_assignment: HashMap<u32, (u32, FormKey)> = HashMap::new();
    let mut workshop_lctns: Vec<(FormKey, WorkshopClass)> = Vec::new();
    // target-LCTN object-id -> synthesized ECZN FormKey. The synthesized ECZN's
    // DATA.location and the translated interior cell's XLCN both carry the target
    // LCTN, so keying by its object-id lets the interior pass match cells to
    // ECZNs without inverse-mapping back to the source.
    let mut target_lctn_to_eczn: HashMap<u32, FormKey> = HashMap::new();

    let started = std::time::Instant::now();
    for plan in &plans {
        let band = if plan.own_band.is_some() {
            plan.own_band
        } else {
            resolve_band(plan.source_lctn.local, &bands, &parents)
        };

        // Require a real source->target LCTN mapping. Without it we would emit
        // an ECZN whose DATA.location dangles at the source plugin and a keyword
        // rewrite that silently misses. On the slice path the mapper is empty
        // (no translate_all), so fall back to the preserved-id identity FK when
        // the target actually holds that LCTN; otherwise skip.
        let target_lctn = match mapper.lookup(plan.source_lctn) {
            Some(fk) => fk,
            None if identity_resolve
                && config.preserve_source_ids
                && target.lctn_object_ids.contains(&plan.source_lctn.local) =>
            {
                FormKey {
                    local: plan.source_lctn.local,
                    plugin: output_plugin,
                }
            }
            None => continue,
        };

        let target_objid = target_lctn.local & 0x00FF_FFFF;
        let has_interior_cells = interior_lctn_targets.contains(&target_objid);
        let referenced_by_placed_ref = placed_xezn_targets.contains(&target_objid);
        // Synthesize an ECZN for EVERY Location referenced as an encounter zone,
        // not just workshops: workshop class, OR a band (own/inherited), OR an
        // exterior footprint, OR interior cells pointing back at it (XLCN), OR a
        // placed ref's XEZN pointing at it. Zero-band zones get a default (0,0)
        // band below.
        let create = match plan.class {
            WorkshopClass::Settlement | WorkshopClass::Shelter => true,
            WorkshopClass::NonWorkshop => {
                band.is_some()
                    || !plan.footprint.is_empty()
                    || has_interior_cells
                    || referenced_by_placed_ref
            }
        };
        if !create {
            continue;
        }

        let synth_source = FormKey {
            local: plan.source_lctn.local,
            plugin: mapper.interner.intern(SYNTH_ECZN_PLUGIN),
        };
        let eczn_fk = mapper.allocate_or_resolve(synth_source, None, eczn_sig);

        let (min, max) = band.unwrap_or((0, 0));
        let eid = if plan.editor_id.is_empty() {
            format!("Synth{:06X}EncounterZone", plan.source_lctn.local)
        } else {
            eczn_editor_id(&plan.editor_id)
        };
        let eczn = build_eczn_record(
            eczn_fk,
            &eid,
            target_lctn,
            clamp_level(min),
            eczn_flags(plan.class),
            clamp_level(max),
            mapper.interner,
        );
        if session
            .add_record(eczn, target_schema.as_ref(), mapper.interner)
            .is_err()
        {
            continue;
        }
        report.records_added += 1;
        target_lctn_to_eczn.insert(target_lctn.local & 0x00FF_FFFF, eczn_fk);

        let depth = depth_of(plan.source_lctn.local);
        for (world_fk, gx, gy) in &plan.footprint {
            let world_objid = mapper
                .lookup(*world_fk)
                .map(|f| f.local)
                .unwrap_or(world_fk.local)
                & 0x00FF_FFFF;
            if let Some(cell_objid) = target_index.get(&(world_objid, *gx, *gy)).copied() {
                let e = cell_assignment
                    .entry(cell_objid)
                    .or_insert((u32::MIN, eczn_fk));
                if depth >= e.0 {
                    *e = (depth, eczn_fk);
                }
            }
        }
        if matches!(
            plan.class,
            WorkshopClass::Settlement | WorkshopClass::Shelter
        ) {
            workshop_lctns.push((target_lctn, plan.class));
        }
    }
    log_timing("plan_loop_add_eczns", started);

    // Repoint placed-ref XEZN at the synthesized ECZN. FO76 XEZN names an LCTN,
    // already remapped to the target LCTN post-copy. An XEZN whose LCTN has no
    // ECZN (only a master-resident LCTN) is stripped, since FO4 requires
    // XEZN→ECZN. Placed records sit under cell groups, so edits go through one
    // batched contents replace that keeps tree position (`replace_record` would
    // move them to a top-level group; per-record replace is O(changed × tree)).
    // `build_target_index` recorded which refs carry a non-null XEZN, so only
    // those are decoded.
    let started = std::time::Instant::now();
    let mut placed_changes: Vec<crate::record::Record> = Vec::new();
    for &ref_objid in &target.placed_xezn_ref_objids {
        let pfk = FormKey {
            local: ref_objid,
            plugin: output_plugin,
        };
        let mut rec = match session.record_decoded(&pfk, target_schema.as_ref(), mapper.interner) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let Some(idx) = rec.fields.iter().position(|f| f.sig == xezn_sig) else {
            continue;
        };
        // Only an output-plugin XEZN can name a converted LCTN with a synthesized
        // ECZN. A null XEZN is harmless (leave it). A master-resident XEZN (FO76
        // LCTN in a master, or any non-output plugin) has no FO4 ECZN equivalent,
        // so it falls through to the strip branch.
        let eczn_for_target = match &rec.fields[idx].value {
            FieldValue::FormKey(fk) if fk.local & 0x00FF_FFFF == 0 => continue,
            FieldValue::FormKey(fk) if fk.plugin == output_plugin => {
                target_lctn_to_eczn.get(&(fk.local & 0x00FF_FFFF)).copied()
            }
            FieldValue::FormKey(_) => None,
            // Non-formkey XEZN — leave it.
            _ => continue,
        };
        let changed = match eczn_for_target {
            Some(eczn_fk) => {
                if rec.fields[idx].value == FieldValue::FormKey(eczn_fk) {
                    false
                } else {
                    rec.fields[idx].value = FieldValue::FormKey(eczn_fk);
                    true
                }
            }
            // No synthesized ECZN for this target (master-resident LCTN, or a
            // converted LCTN that didn't qualify); strip the wrong-type XEZN so
            // FO4 doesn't dereference it.
            None => {
                rec.fields.remove(idx);
                true
            }
        };
        if changed {
            placed_changes.push(rec);
        }
    }
    report.records_changed += session
        .replace_records_contents(placed_changes, target_schema.as_ref(), mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    log_timing("xezn_repoint", started);

    // Interior pull-model: each interior cell names its own Location via XLCN.
    // Stamp XEZN when that target LCTN has a synthesized ECZN. Interior cells are
    // not in the grid footprint, so they never collide with the exterior
    // depth-priority assignment; insert them with max priority.
    for (cell_objid, xlcn) in &target.interior_cells {
        let Some(lctn_objid) = xlcn else { continue };
        if let Some(eczn_fk) = target_lctn_to_eczn.get(lctn_objid) {
            cell_assignment.insert(*cell_objid, (u32::MAX, *eczn_fk));
        }
    }

    // Stamp XEZN in place with ONE batched tree traversal. A structural
    // `replace_record` would (a) re-scan the whole tree + rebuild the index per
    // cell (O(cells × tree)) and (b) RELOCATE each cell to a top-level CELL group —
    // catastrophic for exterior footprint cells, which must stay nested under WRLD.
    // Content-replace mutates the cell where it sits, preserving position.
    let started = std::time::Instant::now();
    let mut cell_changes: Vec<crate::record::Record> = Vec::new();
    for (cell_objid, (_, eczn_fk)) in &cell_assignment {
        let cell_fk = FormKey {
            local: *cell_objid,
            plugin: eczn_fk.plugin,
        };
        let mut cell =
            match session.record_decoded(&cell_fk, target_schema.as_ref(), mapper.interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
        if let Some(f) = cell.fields.iter_mut().find(|f| f.sig == xezn_sig) {
            f.value = FieldValue::FormKey(*eczn_fk);
        } else {
            cell.fields.push(FieldEntry {
                sig: xezn_sig,
                value: FieldValue::FormKey(*eczn_fk),
            });
        }
        cell_changes.push(cell);
    }
    report.records_changed += session
        .replace_records_contents(cell_changes, target_schema.as_ref(), mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    log_timing("cell_xezn_stamp", started);

    let started = std::time::Instant::now();
    let fallout4 = mapper.interner.intern(FALLOUT4_ESM);
    let seventysix = mapper.interner.intern(SEVENTYSIX_ESM);
    let mut drop_fks: Vec<FormKey> = [
        mapper.lookup(FormKey {
            local: KW_WORKSHOP_PUBLIC,
            plugin: seventysix,
        }),
        mapper.lookup(FormKey {
            local: KW_WORKSHOP_SHELTER,
            plugin: seventysix,
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    if identity_resolve {
        // The slice mapper is empty; preserve_source_ids keeps FO76-only keyword
        // records (no FO4 equivalent) at their source id under the output plugin,
        // so that is the form the translated KWDA carries.
        for local in [KW_WORKSHOP_PUBLIC, KW_WORKSHOP_SHELTER] {
            drop_fks.push(FormKey {
                local,
                plugin: output_plugin,
            });
        }
    }
    let mut workshop_changes: Vec<crate::record::Record> = Vec::new();
    for (target_lctn, class) in workshop_lctns {
        let mut rec =
            match session.record_decoded(&target_lctn, target_schema.as_ref(), mapper.interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
        rebuild_keyword_fields(&mut rec, class, &drop_fks, fallout4, mapper.interner);
        workshop_changes.push(rec);
    }
    report.records_changed += session
        .replace_records_contents(workshop_changes, target_schema.as_ref(), mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))? as u32;
    log_timing("workshop_keyword_rewrite", started);
    log_timing("synthesize_encounter_zones_total", phase_started);

    Ok(report)
}

#[cfg(test)]
mod interior_tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, ResolutionMode};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, ParsedSubrecord, ensure_interior_cell_and_child_group,
        insert_placed_child_into_cell_group, plugin_handle_close_native, plugin_handle_new_native,
        plugin_handle_save_no_py, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    const OUTPUT_PLUGIN: &str = "Converted.esm";

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    /// An interior CELL (DATA interior bit set) carrying EDID/DATA/XCLW and an
    /// optional XLCN→`xlcn` (raw form_id incl. master byte).
    fn interior_cell_record(form_id: u32, eid: &str, xlcn: Option<u32>) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        let mut subrecords = vec![
            sub("EDID", edid),
            sub("DATA", vec![0x01u8, 0x00]),
            sub("XCLW", 3.0_f32.to_le_bytes().to_vec()),
        ];
        if let Some(loc) = xlcn {
            subrecords.push(sub("XLCN", loc.to_le_bytes().to_vec()));
        }
        ParsedRecord {
            signature: SmolStr::new("CELL"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    /// FO76 LCTN with EDID, a `struct:I,B,B,B,B` DATA carrying `(min,
    /// location_type, max)`, and an optional KWDA keyword array. The conversion
    /// decoder rawifies `struct:` codecs, so class qualification in these tests
    /// rides on the KWDA workshop keyword (decoded as a formid_array), mirroring
    /// the exterior path's `classify` reliance. Placed directly into the source
    /// slot's root.
    fn source_lctn_record(
        form_id: u32,
        eid: &str,
        min: u8,
        lt: u8,
        max: u8,
        keywords: &[u32],
    ) -> ParsedRecord {
        let mut edid = eid.as_bytes().to_vec();
        edid.push(0);
        let data = vec![0, 0, 0, 0, 0, min, lt, max];
        let mut subrecords = vec![sub("EDID", edid), sub("DATA", data)];
        if !keywords.is_empty() {
            let mut kwda = Vec::with_capacity(keywords.len() * 4);
            for kw in keywords {
                kwda.extend_from_slice(&kw.to_le_bytes());
            }
            subrecords.push(sub("KSIZ", (keywords.len() as u32).to_le_bytes().to_vec()));
            subrecords.push(sub("KWDA", kwda));
        }
        ParsedRecord {
            signature: SmolStr::new("LCTN"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(155),
            version2: Some(0),
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn put_source_lctn(source: u64, record: ParsedRecord) {
        put_record_in_group(source, *b"LCTN", record);
    }

    fn put_record_in_group(handle: u64, label: [u8; 4], record: ParsedRecord) {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle).unwrap();
        if let Some(ParsedItem::Group(group)) = slot
            .parsed
            .root_items
            .iter_mut()
            .find(|item| matches!(item, ParsedItem::Group(g) if g.label == label))
        {
            group.children.push(ParsedItem::Record(record));
        } else {
            slot.parsed.root_items.push(ParsedItem::Group(
                esp_authoring_core::plugin_runtime::ParsedGroup {
                    label,
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Record(record)],
                },
            ));
        }
        slot.invalidate_sections();
    }

    fn find_cell<'a>(items: &'a [ParsedItem], object_id: u32) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(r)
                    if r.signature.as_str() == "CELL" && r.form_id & 0x00FF_FFFF == object_id =>
                {
                    return Some(r);
                }
                ParsedItem::Group(g) => {
                    if let Some(found) = find_cell(&g.children, object_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn xezn_target(record: &ParsedRecord) -> Option<u32> {
        record
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == "XEZN")
            .map(|s| u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]) & 0x00FF_FFFF)
    }

    /// A placed REFR carrying NAME + an XEZN raw form_id (the target LCTN it
    /// references as its encounter zone). Inserted into an interior cell's
    /// Temporary (group_type 9) child group.
    fn placed_refr(form_id: u32, xezn_target_form_id: u32) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("REFR"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                sub("NAME", 0x01000800u32.to_le_bytes().to_vec()),
                sub("XEZN", xezn_target_form_id.to_le_bytes().to_vec()),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn find_refr<'a>(items: &'a [ParsedItem], object_id: u32) -> Option<&'a ParsedRecord> {
        find_record_by_sig_id(items, "REFR", object_id)
    }

    fn find_record_by_sig_id<'a>(
        items: &'a [ParsedItem],
        sig: &str,
        object_id: u32,
    ) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(r)
                    if r.signature.as_str() == sig && r.form_id & 0x00FF_FFFF == object_id =>
                {
                    return Some(r);
                }
                ParsedItem::Group(g) => {
                    if let Some(found) = find_record_by_sig_id(&g.children, sig, object_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn subrecord_formid(record: &ParsedRecord, sig: &str) -> Option<u32> {
        record
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == sig)
            .map(|s| u32::from_le_bytes([s.data[0], s.data[1], s.data[2], s.data[3]]) & 0x00FF_FFFF)
    }

    fn invalidate_handle(handle: u64) {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        store.get_mut(&handle).unwrap().invalidate_sections();
    }

    fn config() -> FixupConfig {
        FixupConfig {
            preserve_source_ids: true,
            is_whole_plugin: true,
            target_schema: Some(AuthoringSchema::for_game("fo4").unwrap()),
            source_schema: Some(AuthoringSchema::for_game("fo76").unwrap()),
            ..FixupConfig::default()
        }
    }

    fn mapper_options() -> MapperOptions {
        MapperOptions {
            output_plugin_name: OUTPUT_PLUGIN.into(),
            resolution_mode: ResolutionMode::DeferAndFixup,
            ..MapperOptions::default()
        }
    }

    /// Interior cell whose Location LCTN has a synthesized ECZN ends up with
    /// XEZN = that ECZN; one whose LCTN cannot be resolved gets none.
    #[test]
    fn interior_cell_gets_xezn_only_when_lctn_has_eczn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        // Source workshop LCTN (no exterior footprint — interior-only). Workshop
        // class qualifies unconditionally via the KWDA keyword.
        let src_lctn_local = 0x0989F5u32;
        put_source_lctn(
            source,
            source_lctn_record(
                src_lctn_local,
                "LocVaultX",
                20,
                0,
                99,
                &[crate::fixups::encounter_zones::model::KW_WORKSHOP],
            ),
        );
        // Target LCTN id (preserved identity) + interior cell pointing at it.
        let tgt_lctn_local = src_lctn_local;
        let cell_local = 0x00275EDEu32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "VaultXCell", Some(tgt_lctn_local)),
        )
        .unwrap();
        // The target must also hold the LCTN id so identity-resolve qualifies it.
        put_target_lctn(target, tgt_lctn_local);
        // A second interior cell names a LCTN with no source record (e.g. a
        // master-resident Location): nothing can be decoded, so it gets no XEZN.
        let unresolved_cell_local = 0x00275EE0u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(unresolved_cell_local, "PlainYCell", Some(0x00ABCDEF)),
        )
        .unwrap();

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: tgt_lctn_local,
                plugin: out,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(report.records_added, 1, "one ECZN synthesized");

        // The synthesized ECZN object-id is what the cell's XEZN must point at.
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let eczn = find_record_by_sig(&slot.parsed.root_items, "ECZN").expect("ECZN added");
        let eczn_objid = eczn.form_id & 0x00FF_FFFF;
        let cell = find_cell(&slot.parsed.root_items, cell_local).expect("interior cell");
        assert_eq!(
            xezn_target(cell),
            Some(eczn_objid),
            "interior cell XEZN points at synthesized ECZN"
        );
        let unresolved = find_cell(&slot.parsed.root_items, unresolved_cell_local)
            .expect("unresolved interior cell");
        assert_eq!(
            xezn_target(unresolved),
            None,
            "no XEZN for unresolvable LCTN"
        );
    }

    /// A NON-workshop Location with a band (no keyword, no footprint)
    /// synthesizes an ECZN.
    #[test]
    fn non_workshop_banded_lctn_synthesizes_eczn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        // Banded, non-workshop LCTN: min=10, type=0 (not workshop), max=40, no kw.
        let src_lctn_local = 0x0989F7u32;
        put_source_lctn(
            source,
            source_lctn_record(src_lctn_local, "LocBandedA", 10, 0, 40, &[]),
        );
        let tgt_lctn_local = src_lctn_local;
        put_target_lctn(target, tgt_lctn_local);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: tgt_lctn_local,
                plugin: out,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(
            report.records_added, 1,
            "banded non-workshop LCTN synthesizes one ECZN"
        );
    }

    #[test]
    fn synthesized_eczn_skips_existing_target_object_id() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let src_lctn_local = 0x0989F9u32;
        put_source_lctn(
            source,
            source_lctn_record(src_lctn_local, "LocCollisionA", 10, 0, 40, &[]),
        );
        put_target_lctn(target, src_lctn_local);

        let collision_local = 0x00A0033Fu32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(collision_local, "ExistingGeneratedBandCell", None),
        )
        .unwrap();

        let mut state = crate::formkey_mapper::MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: OUTPUT_PLUGIN.into(),
                generated_object_id_floor: collision_local,
                resolution_mode: ResolutionMode::DeferAndFixup,
                ..MapperOptions::default()
            },
        );
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: src_lctn_local,
                plugin: out,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(report.records_added, 1, "one ECZN synthesized");

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let eczn = find_record_by_sig(&slot.parsed.root_items, "ECZN").expect("ECZN added");
        assert_eq!(
            eczn.form_id & 0x00FF_FFFF,
            collision_local + 1,
            "ECZN allocation skips target records that were not allocated by the mapper"
        );
        assert!(
            find_cell(&slot.parsed.root_items, collision_local).is_some(),
            "preexisting target CELL remains at the collision id"
        );
    }

    /// A zero-band Location referenced ONLY by a placed ref's XEZN still
    /// synthesizes a (default-band) ECZN, and that ref's XEZN is REPOINTED to the
    /// ECZN — not stripped.
    #[test]
    fn placed_ref_xezn_repointed_to_synthesized_eczn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        // Zero-band, non-workshop, footprint-less, interior-cell-less LCTN. Only a
        // placed ref's XEZN references it.
        let src_lctn_local = 0x0989F8u32;
        put_source_lctn(
            source,
            source_lctn_record(src_lctn_local, "LocZeroBandA", 0, 0, 0, &[]),
        );
        let tgt_lctn_local = src_lctn_local;
        put_target_lctn(target, tgt_lctn_local);

        // An interior cell to host the placed REFR (the cell's own XLCN is
        // unrelated — set to None so the cell itself doesn't pull an ECZN).
        let cell_local = 0x00275EF0u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "HostCell", None),
        )
        .unwrap();
        let refr_local = 0x00300010u32;
        // XEZN raw form_id carries the output-plugin master byte; masked it is the
        // target LCTN object-id.
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(refr_local, tgt_lctn_local),
        )
        .unwrap();
        // XEZN → a LCTN id with no source LCTN record → no ECZN → safety-net strip.
        let stripped_refr_local = 0x00300012u32;
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(stripped_refr_local, 0x00ABCDEF),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: tgt_lctn_local,
                plugin: out,
            },
        );

        let report = {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            let r =
                synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
            r
        };
        assert_eq!(
            report.records_added, 1,
            "zero-band LCTN referenced by a ref's XEZN synthesizes a default-band ECZN"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let eczn = find_record_by_sig(&slot.parsed.root_items, "ECZN").expect("ECZN added");
        let eczn_objid = eczn.form_id & 0x00FF_FFFF;
        let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(refr),
            Some(eczn_objid),
            "placed ref XEZN repointed to the synthesized ECZN (not stripped)"
        );
        let stripped =
            find_refr(&slot.parsed.root_items, stripped_refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(stripped),
            None,
            "placed ref XEZN with no ECZN target is stripped"
        );
    }

    #[test]
    fn finalizer_repoints_placed_xezn_to_existing_eczn_or_strips_it() {
        let interner = StringInterner::new();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let lctn_local = 0x0955B5u32;
        let eczn_local = 0xA15300u32;
        put_target_lctn(target, lctn_local);
        add_target_eczn(target, &interner, eczn_local, lctn_local);

        let cell_local = 0x00275EF4u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "HostCell3", None),
        )
        .unwrap();
        let refr_local = 0x00300014u32;
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(refr_local, lctn_local),
        )
        .unwrap();
        let eczn_less_lctn = 0x0955B6u32;
        put_target_lctn(target, eczn_less_lctn);
        let stripped_refr_local = 0x00300018u32;
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(stripped_refr_local, eczn_less_lctn),
        )
        .unwrap();
        invalidate_handle(target);

        let report = {
            let mut session = open_session(target, None).unwrap();
            let report = finalize_placed_xezn_targets(&mut session, &config(), &interner).unwrap();
            session.flush_pending_effects();
            report
        };
        assert_eq!(
            report.records_changed, 2,
            "stale placed XEZN repaired and wrong-type XEZN stripped"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(refr),
            Some(eczn_local),
            "placed REFR XEZN points at the existing ECZN tied to its LCTN"
        );
        assert_eq!(
            subrecord_formid(refr, "XEZN"),
            Some(eczn_local),
            "decoded and raw XEZN agree"
        );
        let stripped =
            find_refr(&slot.parsed.root_items, stripped_refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(stripped),
            None,
            "placed REFR XEZN without an ECZN target is stripped"
        );
    }

    #[test]
    fn shared_refr_candidates_match_legacy_xezn_scan_exactly() {
        let interner = StringInterner::new();
        let build = || {
            let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();
            let lctn_local = 0x0955B5u32;
            put_target_lctn(target, lctn_local);
            add_target_eczn(target, &interner, 0xA15300, lctn_local);
            let cell_local = 0x00275EF4u32;
            ensure_interior_cell_and_child_group(
                target,
                interior_cell_record(cell_local, "ParityCell", None),
            )
            .unwrap();
            insert_placed_child_into_cell_group(
                target,
                cell_local,
                9,
                placed_refr(0x00300014, lctn_local),
            )
            .unwrap();
            let mut malformed = placed_refr(0x00300015, lctn_local);
            malformed
                .subrecords
                .iter_mut()
                .find(|subrecord| subrecord.signature.as_str() == "XEZN")
                .unwrap()
                .data = Bytes::from_static(&[1, 2]);
            malformed.parse_error = Some("malformed XEZN fixture".to_string());
            insert_placed_child_into_cell_group(target, cell_local, 9, malformed).unwrap();
            invalidate_handle(target);
            target
        };
        let legacy = build();
        let shared = build();
        let run = |handle, use_shared: bool| {
            let report = {
                let mut session = open_session(handle, None).unwrap();
                let report = if use_shared {
                    let discovery = crate::fixups::repair_placed_teleport_doors::discover_placed_refr_candidates(
                        &mut session,
                        &interner,
                    )
                    .unwrap();
                    finalize_placed_xezn_targets_for_refr_candidates(
                        &mut session,
                        &config(),
                        &interner,
                        &discovery.xezn_candidates,
                    )
                    .unwrap()
                } else {
                    finalize_placed_xezn_targets(&mut session, &config(), &interner).unwrap()
                };
                session.flush_pending_effects();
                report
            };
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join(OUTPUT_PLUGIN);
            plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
            let bytes = std::fs::read(path).unwrap();
            assert!(plugin_handle_close_native(handle));
            (report, bytes)
        };

        let (legacy_report, legacy_bytes) = run(legacy, false);
        let (shared_report, shared_bytes) = run(shared, true);
        assert_eq!(legacy_report.records_changed, shared_report.records_changed);
        assert_eq!(legacy_report.records_dropped, shared_report.records_dropped);
        assert_eq!(legacy_report.warnings, shared_report.warnings);
        assert_eq!(legacy_report.diagnostics, shared_report.diagnostics);
        assert_eq!(legacy_bytes, shared_bytes);
    }

    #[test]
    fn finalizer_repoints_after_late_repair_restores_lctn() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native(OUTPUT_PLUGIN, Some("fo4")).unwrap();

        let out = interner.intern(OUTPUT_PLUGIN);
        let sevsix = interner.intern("SeventySix.esm");
        let src_lctn_local = 0x098A10u32;
        put_source_lctn(
            source,
            source_lctn_record(src_lctn_local, "LocLateRepairA", 10, 0, 20, &[]),
        );
        put_target_lctn(target, src_lctn_local);
        let cell_local = 0x00275EF6u32;
        ensure_interior_cell_and_child_group(
            target,
            interior_cell_record(cell_local, "LateRepairHost", None),
        )
        .unwrap();
        let refr_local = 0x00300016u32;
        insert_placed_child_into_cell_group(
            target,
            cell_local,
            9,
            placed_refr(refr_local, src_lctn_local),
        )
        .unwrap();
        invalidate_handle(target);

        let mut state = crate::formkey_mapper::MapperState::new([], mapper_options());
        state.source_to_target.insert(
            FormKey {
                local: src_lctn_local,
                plugin: sevsix,
            },
            FormKey {
                local: src_lctn_local,
                plugin: out,
            },
        );

        {
            let mut mapper =
                crate::formkey_mapper::FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(target, Some(source)).unwrap();
            synthesize_encounter_zones(&mut session, &mut mapper, &config(), false).unwrap();
            session.flush_pending_effects();
        }

        let eczn_objid = {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&target).unwrap();
            let eczn = find_record_by_sig(&slot.parsed.root_items, "ECZN").expect("ECZN added");
            let eczn_objid = eczn.form_id & 0x00FF_FFFF;
            let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
            assert_eq!(
                xezn_target(refr),
                Some(eczn_objid),
                "synthesis initially repoints the placed XEZN"
            );
            eczn_objid
        };

        set_placed_xezn(target, refr_local, src_lctn_local);

        {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&target).unwrap();
            let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
            assert_eq!(
                xezn_target(refr),
                Some(src_lctn_local),
                "simulated late repair restored the stale LCTN"
            );
        }

        let report = {
            let mut session = open_session(target, None).unwrap();
            let report = finalize_placed_xezn_targets(&mut session, &config(), &interner).unwrap();
            session.flush_pending_effects();
            report
        };
        assert_eq!(
            report.records_changed, 1,
            "finalizer repairs late stale XEZN"
        );

        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&target).unwrap();
        let refr = find_refr(&slot.parsed.root_items, refr_local).expect("placed REFR");
        assert_eq!(
            xezn_target(refr),
            Some(eczn_objid),
            "late finalizer restores XEZN to the synthesized ECZN"
        );
    }

    fn put_target_lctn(target: u64, local: u32) {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&target).unwrap();
        let mut edid = b"TgtLctn".to_vec();
        edid.push(0);
        let record = ParsedRecord {
            signature: SmolStr::new("LCTN"),
            form_id: local,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("EDID", edid)],
            raw_payload: None,
            parse_error: None,
        };
        if let Some(ParsedItem::Group(group)) = slot
            .parsed
            .root_items
            .iter_mut()
            .find(|item| matches!(item, ParsedItem::Group(g) if &g.label == b"LCTN"))
        {
            group.children.push(ParsedItem::Record(record));
        } else {
            slot.parsed.root_items.push(ParsedItem::Group(
                esp_authoring_core::plugin_runtime::ParsedGroup {
                    label: *b"LCTN",
                    group_type: 0,
                    tail: Bytes::new(),
                    children: vec![ParsedItem::Record(record)],
                },
            ));
        }
        slot.invalidate_sections();
    }

    fn add_target_eczn(target: u64, interner: &StringInterner, eczn_local: u32, lctn_local: u32) {
        let out = interner.intern(OUTPUT_PLUGIN);
        let cfg = config();
        let schema = cfg.target_schema.as_ref().unwrap();
        let mut session = open_session(target, None).unwrap();
        session
            .add_record(
                build_eczn_record(
                    FormKey {
                        local: eczn_local,
                        plugin: out,
                    },
                    "ExistingLocationEncounterZone",
                    FormKey {
                        local: lctn_local,
                        plugin: out,
                    },
                    10,
                    0,
                    20,
                    interner,
                ),
                schema.as_ref(),
                interner,
            )
            .unwrap();
        session.flush_pending_effects();
    }

    fn set_placed_xezn(target: u64, object_id: u32, raw_form_id: u32) {
        fn walk(items: &mut [ParsedItem], object_id: u32, raw_form_id: u32) -> bool {
            for item in items {
                match item {
                    ParsedItem::Record(record)
                        if record.form_id & 0x00FF_FFFF == object_id
                            && record.signature.as_str() == "REFR" =>
                    {
                        let raw = raw_form_id.to_le_bytes().to_vec();
                        if let Some(subrecord) = record
                            .subrecords
                            .iter_mut()
                            .find(|subrecord| subrecord.signature.as_str() == "XEZN")
                        {
                            subrecord.data = Bytes::from(raw);
                        } else {
                            record.subrecords.push(sub("XEZN", raw));
                        }
                        return true;
                    }
                    ParsedItem::Group(group) => {
                        if walk(&mut group.children, object_id, raw_form_id) {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
            false
        }

        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&target).unwrap();
        assert!(walk(&mut slot.parsed.root_items, object_id, raw_form_id));
        slot.invalidate_sections();
    }

    fn find_record_by_sig<'a>(items: &'a [ParsedItem], sig: &str) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(r) if r.signature.as_str() == sig => return Some(r),
                ParsedItem::Group(g) => {
                    if let Some(found) = find_record_by_sig(&g.children, sig) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod fo76_location_tests {
    use super::*;

    #[test]
    fn own_encounter_zone_wins_over_the_cell_claim() {
        let actors = [
            cell_index::PlacedActorRef { ref_objid: 1, cell_objid: 10 },
            cell_index::PlacedActorRef { ref_objid: 2, cell_objid: 10 },
            cell_index::PlacedActorRef { ref_objid: 3, cell_objid: 11 },
        ];
        let xezn = HashMap::from([(1u32, 0x500u32)]);
        let cells = HashMap::from([(10u32, (0u32, 0x600u32))]);
        assert_eq!(merge_placed_actor_locations(&actors, &xezn, &cells), vec![(1, 0x500), (2, 0x600)]);
    }

    #[test]
    fn location_collection_keeps_the_retired_ess_pass_id_reservation() {
        let source = include_str!("mod.rs");
        let start = source.find("pub fn placed_actor_fo76_locations").unwrap();
        let end = start + source[start..].find("\n}\n").unwrap();
        assert!(source[start..end].contains("mapper.reserve_object_ids(target.all_object_ids"));
    }
}
