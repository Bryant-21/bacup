//! Fixup: repair placed-reference local refs whose FO76/Starfield source-local
//! master byte still points at a target master after conversion to FO4.
//!
//! Interior placed refs can bypass `cell_slice::rewrite_placed_child_local_refs`.
//! Raw placed-only subrecords then keep the source high byte `0x00`, which FO4
//! interprets as `Fallout4.esm` after masters are added. If the finished output
//! owns the same object id with the expected type, rewrite the slot to the output
//! plugin. If the slot allows NULL and resolves nowhere or to the wrong type,
//! null it; for non-null repeat rows like XPLK, drop that row.

use bytes::Bytes;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::smallvec;

use esp_authoring_core::plugin_runtime::{ParsedRecord, WriteEffect};

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;

const XLKR_KEYWORD_OFFSET: usize = 0;
const XLKR_REF_OFFSET: usize = 4;
const XPLK_REF_OFFSET: usize = 0;
const XESP_REF_OFFSET: usize = 0;
const XAPR_REF_OFFSET: usize = 0;
const XLOC_KEY_OFFSET: usize = 4;
const XNDP_NAVM_OFFSET: usize = 0;

const PLACED_REF_OWNER_SIGS: &[&str] = &["REFR", "ACHR", "PGRE", "PHZD"];
const REPAIR_SUBRECORD_SIGS: &[&str] = &["XLKR", "XPLK", "XESP", "XAPR", "XLOC", "XNDP"];
const LINKED_REF_TARGET_SIGS: &[&str] = &[
    "PLYR", "ACHR", "REFR", "PGRE", "PHZD", "PMIS", "PARW", "PBAR", "PBEA", "PCON", "PFLA",
];
const LINKED_KEYWORD_OR_REF_SIGS: &[&str] = &[
    "KYWD", "PLYR", "ACHR", "REFR", "PGRE", "PHZD", "PMIS", "PARW", "PBAR", "PBEA", "PCON", "PFLA",
];
const SPLINE_TARGET_SIGS: &[&str] = &["REFR", "ACHR"];
const KEY_SIGS: &[&str] = &["KEYM"];
const NAVM_SIGS: &[&str] = &["NAVM"];
const REPAIR_TARGET_SIGS: &[&str] = &[
    "KYWD", "PLYR", "ACHR", "REFR", "PGRE", "PHZD", "PMIS", "PARW", "PBAR", "PBEA", "PCON", "PFLA",
    "KEYM", "NAVM",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotRepair {
    Keep,
    Rewrite(u32),
    Invalid,
}

struct RepairSets {
    own_load_index: u8,
    own_linked_keyword_or_ref_ids: FxHashSet<u32>,
    own_linked_ref_ids: FxHashSet<u32>,
    own_spline_target_ids: FxHashSet<u32>,
    own_key_ids: FxHashSet<u32>,
    own_navmesh_ids: FxHashSet<u32>,
}

fn read_formid(buf: &[u8], offset: usize) -> Option<u32> {
    buf.get(offset..offset.checked_add(4)?)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn sig_allowed(sig: SigCode, allowed_sigs: &[&str]) -> bool {
    allowed_sigs.contains(&sig.as_str())
}

fn repair_ref_slot(
    raw: u32,
    own_load_index: u8,
    own_object_ids: &FxHashSet<u32>,
    allowed_target_sigs: &[&str],
    master_sigs: &FxHashMap<u32, Option<SigCode>>,
) -> SlotRepair {
    let object_id = raw & 0x00FF_FFFF;
    if object_id == 0 {
        return SlotRepair::Keep;
    }

    let load_index = (raw >> 24) as usize;
    if load_index == own_load_index as usize {
        return if own_object_ids.contains(&object_id) {
            SlotRepair::Keep
        } else {
            SlotRepair::Invalid
        };
    }

    if load_index < own_load_index as usize {
        if master_sigs
            .get(&raw)
            .and_then(|sig| *sig)
            .is_some_and(|sig| sig_allowed(sig, allowed_target_sigs))
        {
            return SlotRepair::Keep;
        }
        if own_object_ids.contains(&object_id) {
            return SlotRepair::Rewrite(((own_load_index as u32) << 24) | object_id);
        }
        return SlotRepair::Invalid;
    }

    SlotRepair::Keep
}

fn repair_ref_slot_bytes(
    buf: &mut [u8],
    offset: usize,
    own_load_index: u8,
    own_object_ids: &FxHashSet<u32>,
    allowed_target_sigs: &[&str],
    master_sigs: &FxHashMap<u32, Option<SigCode>>,
    null_invalid: bool,
) -> bool {
    let Some(raw) = read_formid(buf, offset) else {
        return false;
    };
    match repair_ref_slot(
        raw,
        own_load_index,
        own_object_ids,
        allowed_target_sigs,
        master_sigs,
    ) {
        SlotRepair::Keep => false,
        SlotRepair::Rewrite(repaired) => {
            buf[offset..offset + 4].copy_from_slice(&repaired.to_le_bytes());
            true
        }
        SlotRepair::Invalid if null_invalid => {
            buf[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
            true
        }
        SlotRepair::Invalid => false,
    }
}

fn repair_xlkr_bytes(
    buf: &mut [u8],
    sets: &RepairSets,
    master_sigs: &FxHashMap<u32, Option<SigCode>>,
) -> bool {
    let mut changed = false;
    changed |= repair_ref_slot_bytes(
        buf,
        XLKR_KEYWORD_OFFSET,
        sets.own_load_index,
        &sets.own_linked_keyword_or_ref_ids,
        LINKED_KEYWORD_OR_REF_SIGS,
        master_sigs,
        true,
    );
    changed |= repair_ref_slot_bytes(
        buf,
        XLKR_REF_OFFSET,
        sets.own_load_index,
        &sets.own_linked_ref_ids,
        LINKED_REF_TARGET_SIGS,
        master_sigs,
        true,
    );
    changed
}

fn repair_placed_ref_record(
    record: &mut ParsedRecord,
    sets: &RepairSets,
    master_sigs: &FxHashMap<u32, Option<SigCode>>,
) -> u32 {
    let mut changed = 0u32;
    record.subrecords.retain_mut(|subrecord| {
        let sig = subrecord.signature.as_str();
        match sig {
            "XLKR" => {
                let mut data = subrecord.data.to_vec();
                if repair_xlkr_bytes(&mut data, sets, master_sigs) {
                    subrecord.data = Bytes::from(data);
                    changed += 1;
                }
                true
            }
            "XPLK" => {
                let Some(raw) = read_formid(subrecord.data.as_ref(), XPLK_REF_OFFSET) else {
                    return true;
                };
                match repair_ref_slot(
                    raw,
                    sets.own_load_index,
                    &sets.own_spline_target_ids,
                    SPLINE_TARGET_SIGS,
                    master_sigs,
                ) {
                    SlotRepair::Keep => true,
                    SlotRepair::Rewrite(repaired) => {
                        let mut data = subrecord.data.to_vec();
                        data[XPLK_REF_OFFSET..XPLK_REF_OFFSET + 4]
                            .copy_from_slice(&repaired.to_le_bytes());
                        subrecord.data = Bytes::from(data);
                        changed += 1;
                        true
                    }
                    SlotRepair::Invalid => {
                        changed += 1;
                        false
                    }
                }
            }
            "XESP" => {
                let mut data = subrecord.data.to_vec();
                if repair_ref_slot_bytes(
                    &mut data,
                    XESP_REF_OFFSET,
                    sets.own_load_index,
                    &sets.own_linked_ref_ids,
                    LINKED_REF_TARGET_SIGS,
                    master_sigs,
                    true,
                ) {
                    subrecord.data = Bytes::from(data);
                    changed += 1;
                }
                true
            }
            "XAPR" => {
                let mut data = subrecord.data.to_vec();
                if repair_ref_slot_bytes(
                    &mut data,
                    XAPR_REF_OFFSET,
                    sets.own_load_index,
                    &sets.own_linked_ref_ids,
                    LINKED_REF_TARGET_SIGS,
                    master_sigs,
                    true,
                ) {
                    subrecord.data = Bytes::from(data);
                    changed += 1;
                }
                true
            }
            "XLOC" => {
                let mut data = subrecord.data.to_vec();
                if repair_ref_slot_bytes(
                    &mut data,
                    XLOC_KEY_OFFSET,
                    sets.own_load_index,
                    &sets.own_key_ids,
                    KEY_SIGS,
                    master_sigs,
                    true,
                ) {
                    subrecord.data = Bytes::from(data);
                    changed += 1;
                }
                true
            }
            "XNDP" => {
                let Some(raw) = read_formid(subrecord.data.as_ref(), XNDP_NAVM_OFFSET) else {
                    changed += 1;
                    return false;
                };
                if raw & 0x00FF_FFFF == 0 || raw >> 24 > sets.own_load_index as u32 {
                    changed += 1;
                    return false;
                }
                let object_id = raw & 0x00FF_FFFF;
                let repair = if raw >> 24 < sets.own_load_index as u32
                    && sets.own_navmesh_ids.contains(&object_id)
                {
                    SlotRepair::Rewrite(((sets.own_load_index as u32) << 24) | object_id)
                } else {
                    repair_ref_slot(
                        raw,
                        sets.own_load_index,
                        &sets.own_navmesh_ids,
                        NAVM_SIGS,
                        master_sigs,
                    )
                };
                match repair {
                    SlotRepair::Keep => true,
                    SlotRepair::Rewrite(repaired) => {
                        let mut data = subrecord.data.to_vec();
                        data[XNDP_NAVM_OFFSET..XNDP_NAVM_OFFSET + 4]
                            .copy_from_slice(&repaired.to_le_bytes());
                        subrecord.data = Bytes::from(data);
                        changed += 1;
                        true
                    }
                    SlotRepair::Invalid => {
                        changed += 1;
                        false
                    }
                }
            }
            _ => true,
        }
    });
    if changed > 0 {
        record.raw_payload = None;
    }
    changed
}

fn collect_repair_raws(record: &ParsedRecord) -> Vec<u32> {
    let mut out = Vec::new();
    for subrecord in &record.subrecords {
        match subrecord.signature.as_str() {
            "XLKR" => {
                if let Some(raw) = read_formid(subrecord.data.as_ref(), XLKR_KEYWORD_OFFSET) {
                    out.push(raw);
                }
                if let Some(raw) = read_formid(subrecord.data.as_ref(), XLKR_REF_OFFSET) {
                    out.push(raw);
                }
            }
            "XPLK" => {
                if let Some(raw) = read_formid(subrecord.data.as_ref(), XPLK_REF_OFFSET) {
                    out.push(raw);
                }
            }
            "XESP" => {
                if let Some(raw) = read_formid(subrecord.data.as_ref(), XESP_REF_OFFSET) {
                    out.push(raw);
                }
            }
            "XAPR" => {
                if let Some(raw) = read_formid(subrecord.data.as_ref(), XAPR_REF_OFFSET) {
                    out.push(raw);
                }
            }
            "XLOC" => {
                if let Some(raw) = read_formid(subrecord.data.as_ref(), XLOC_KEY_OFFSET) {
                    out.push(raw);
                }
            }
            "XNDP" => {
                if let Some(raw) = read_formid(subrecord.data.as_ref(), XNDP_NAVM_OFFSET) {
                    out.push(raw);
                }
            }
            _ => {}
        }
    }
    out
}

fn resolve_master_sig(
    session: &mut PluginSession,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    raw: u32,
) -> Option<SigCode> {
    let object_id = raw & 0x00FF_FFFF;
    if object_id == 0 {
        return None;
    }
    let load_index = (raw >> 24) as usize;
    let master_name = target_masters.get(load_index)?;
    let handle_id = *target_master_handle_ids.get(load_index)?;
    let fk_str = format!("{master_name}:{object_id:06X}");
    let sig = session
        .record_signature_in_handle(handle_id, &fk_str)
        .ok()
        .flatten()?;
    SigCode::from_str(&sig).ok()
}

fn master_sigs_for_record(
    session: &mut PluginSession,
    raw_form_id: u32,
    own_load_index: u8,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    cache: &mut FxHashMap<u32, Option<SigCode>>,
) -> Result<FxHashMap<u32, Option<SigCode>>, FixupError> {
    let raws = {
        let record = session
            .record(raw_form_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        collect_repair_raws(record)
    };
    let mut out = FxHashMap::default();
    for raw in raws {
        if raw == 0 || (raw >> 24) as usize >= own_load_index as usize {
            continue;
        }
        let sig = if let Some(cached) = cache.get(&raw).copied() {
            cached
        } else {
            let resolved =
                resolve_master_sig(session, target_masters, target_master_handle_ids, raw);
            cache.insert(raw, resolved);
            resolved
        };
        out.insert(raw, sig);
    }
    Ok(out)
}

fn collect_repair_sets(
    session: &mut PluginSession,
    interner: &crate::sym::StringInterner,
    own_sym: crate::sym::Sym,
) -> Result<RepairSets, FixupError> {
    let own_load_index = session.target_masters().len() as u8;
    let mut sets = RepairSets {
        own_load_index,
        own_linked_keyword_or_ref_ids: FxHashSet::default(),
        own_linked_ref_ids: FxHashSet::default(),
        own_spline_target_ids: FxHashSet::default(),
        own_key_ids: FxHashSet::default(),
        own_navmesh_ids: FxHashSet::default(),
    };
    for sig in REPAIR_TARGET_SIGS {
        let linked_keyword_or_ref = LINKED_KEYWORD_OR_REF_SIGS.contains(sig);
        let linked_ref = LINKED_REF_TARGET_SIGS.contains(sig);
        let spline_target = SPLINE_TARGET_SIGS.contains(sig);
        let key = KEY_SIGS.contains(sig);
        let navmesh = NAVM_SIGS.contains(sig);
        let sig_code =
            SigCode::from_str(sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
        for fk in session
            .form_keys_of_sig(sig_code, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            if fk.plugin == own_sym {
                let object_id = fk.local & 0x00FF_FFFF;
                if linked_keyword_or_ref {
                    sets.own_linked_keyword_or_ref_ids.insert(object_id);
                }
                if linked_ref {
                    sets.own_linked_ref_ids.insert(object_id);
                }
                if spline_target {
                    sets.own_spline_target_ids.insert(object_id);
                }
                if key {
                    sets.own_key_ids.insert(object_id);
                }
                if navmesh {
                    sets.own_navmesh_ids.insert(object_id);
                }
            }
        }
    }
    Ok(sets)
}

#[cfg(test)]
fn collect_repair_sets_repeated_for_test(
    session: &mut PluginSession,
    interner: &crate::sym::StringInterner,
    own_sym: crate::sym::Sym,
) -> Result<RepairSets, FixupError> {
    fn ids(
        session: &mut PluginSession,
        interner: &crate::sym::StringInterner,
        own_sym: crate::sym::Sym,
        sigs: &[&str],
    ) -> Result<FxHashSet<u32>, FixupError> {
        let mut ids = FxHashSet::default();
        for sig in sigs {
            let sig_code =
                SigCode::from_str(sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            for fk in session
                .form_keys_of_sig(sig_code, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?
            {
                if fk.plugin == own_sym {
                    ids.insert(fk.local & 0x00FF_FFFF);
                }
            }
        }
        Ok(ids)
    }

    let own_load_index = session.target_masters().len() as u8;
    let mut own_linked_keyword_or_ref_ids =
        ids(session, interner, own_sym, LINKED_KEYWORD_OR_REF_SIGS)?;
    own_linked_keyword_or_ref_ids.extend(ids(session, interner, own_sym, LINKED_REF_TARGET_SIGS)?);
    Ok(RepairSets {
        own_load_index,
        own_linked_keyword_or_ref_ids,
        own_linked_ref_ids: ids(session, interner, own_sym, LINKED_REF_TARGET_SIGS)?,
        own_spline_target_ids: ids(session, interner, own_sym, SPLINE_TARGET_SIGS)?,
        own_key_ids: ids(session, interner, own_sym, KEY_SIGS)?,
        own_navmesh_ids: ids(session, interner, own_sym, NAVM_SIGS)?,
    })
}

/// Repair raw placed-reference slots in the finished output plugin. Gated to
/// FO76/Starfield to FO4 by `ConversionRun::repair_placed_child_refs`.
pub fn repair_placed_linked_refs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    repair_placed_linked_refs_with_set_builder(session, mapper, config, collect_repair_sets)
}

fn repair_placed_linked_refs_with_set_builder(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    set_builder: impl FnOnce(
        &mut PluginSession,
        &crate::sym::StringInterner,
        crate::sym::Sym,
    ) -> Result<RepairSets, FixupError>,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let interner = mapper.interner;

    let present = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if !PLACED_REF_OWNER_SIGS
        .iter()
        .any(|sig| present.iter().any(|s| s.as_str() == *sig))
    {
        return Ok(report);
    }

    let target_masters = session.target_masters().to_vec();
    if target_masters.len() > 0xFF {
        return Ok(report);
    }
    let own_load_index = target_masters.len() as u8;
    let own_sym = interner.intern(&session.target_slot().parsed.plugin_name);

    let sets = set_builder(session, interner, own_sym)?;

    let mut master_sig_cache: FxHashMap<u32, Option<SigCode>> = FxHashMap::default();
    for owner_sig in PLACED_REF_OWNER_SIGS {
        if !present.iter().any(|s| s.as_str() == *owner_sig) {
            continue;
        }
        let sig_code =
            SigCode::from_str(owner_sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
        for fk in session
            .form_keys_of_sig(sig_code, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            if fk.plugin != own_sym {
                continue;
            }
            if !session
                .record_has_any_subrecord(&fk, REPAIR_SUBRECORD_SIGS)
                .unwrap_or(false)
            {
                continue;
            }
            let raw_form_id = ((own_load_index as u32) << 24) | (fk.local & 0x00FF_FFFF);
            let master_sigs = master_sigs_for_record(
                session,
                raw_form_id,
                own_load_index,
                &target_masters,
                &config.target_master_handle_ids,
                &mut master_sig_cache,
            )?;
            let changed = {
                let record = session
                    .record_mut(raw_form_id)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                repair_placed_ref_record(record, &sets, &master_sigs)
            };
            if changed > 0 {
                session.record_effect(WriteEffect::RecordContents {
                    form_ids: smallvec![raw_form_id],
                });
                report.records_changed = report.records_changed.saturating_add(1);
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedSubrecord, plugin_handle_close_native, plugin_handle_new_native,
        plugin_handle_save_no_py, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::session::open_session;

    fn ids(values: &[u32]) -> FxHashSet<u32> {
        values.iter().copied().collect()
    }

    fn sets() -> RepairSets {
        RepairSets {
            own_load_index: 7,
            own_linked_keyword_or_ref_ids: ids(&[0x632375, 0x2C7635]),
            own_linked_ref_ids: ids(&[0x2C7635, 0x39E691, 0x3F17D5, 0x18DF32]),
            own_spline_target_ids: ids(&[0x39E691]),
            own_key_ids: ids(&[0x55ADA7, 0x3B5D2B]),
            own_navmesh_ids: ids(&[0x2F1305]),
        }
    }

    fn subrecord(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn xndp(navmesh: u32, triangle: i16) -> Vec<u8> {
        let mut data = navmesh.to_le_bytes().to_vec();
        data.extend_from_slice(&triangle.to_le_bytes());
        data.extend_from_slice(&[0, 0]);
        data
    }

    fn record(subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("REFR"),
            form_id: 0x073B73D3,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        }
    }

    fn parsed(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        let mut record = record(subrecords);
        record.signature = SmolStr::new(sig);
        record.form_id = form_id;
        record.raw_payload = None;
        record
    }

    fn linked_ref_fixture(background: usize, duplicates: bool) -> Vec<ParsedItem> {
        let signatures = REPAIR_TARGET_SIGS;
        let mut items = Vec::with_capacity(background + 202);
        for index in 0..background {
            let sig = signatures[index % signatures.len()];
            items.push(ParsedItem::Record(parsed(
                sig,
                0x0740_0000 + index as u32,
                Vec::new(),
            )));
        }
        for index in 0..100u32 {
            let keyword = 0x0001_0000 + index * 2;
            let target = keyword + 1;
            items.push(ParsedItem::Record(parsed(
                "KYWD",
                0x0700_0000 | keyword,
                Vec::new(),
            )));
            items.push(ParsedItem::Record(parsed(
                "REFR",
                0x0700_0000 | target,
                Vec::new(),
            )));
            let mut xlkr = keyword.to_le_bytes().to_vec();
            xlkr.extend_from_slice(&target.to_le_bytes());
            let mut owner = parsed("REFR", 0x0700_8000 + index, vec![subrecord("XLKR", xlkr)]);
            if index % 3 == 0 {
                owner.flags |= 1 << 18;
            }
            items.push(ParsedItem::Record(owner));
        }
        items.push(ParsedItem::Record(parsed("KYWD", 0x0000_0123, Vec::new())));
        if duplicates {
            items.push(ParsedItem::Record(parsed(
                "REFR",
                0x0700_8000,
                vec![subrecord("XESP", 0x0001_0001u32.to_le_bytes().to_vec())],
            )));
        }
        items
    }

    fn run_full_fixup(items: Vec<ParsedItem>, repeated: bool) -> (String, Vec<u8>, f64) {
        let handle = plugin_handle_new_native("Output.esp", Some("fo4")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.header.masters = (0..7).map(|i| format!("Master{i}.esm")).collect();
            slot.parsed.root_items = items;
            slot.invalidate_sections();
        }
        let interner = crate::sym::StringInterner::new();
        let mut mapper_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esp".into(),
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
        let started = std::time::Instant::now();
        let mut session = open_session(handle, None).unwrap();
        let report = if repeated {
            repair_placed_linked_refs_with_set_builder(
                &mut session,
                &mut mapper,
                &FixupConfig::default(),
                collect_repair_sets_repeated_for_test,
            )
        } else {
            repair_placed_linked_refs(&mut session, &mut mapper, &FixupConfig::default())
        }
        .unwrap();
        session.flush_pending_effects();
        drop(session);
        let elapsed = started.elapsed().as_secs_f64();
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("output.esp");
        plugin_handle_save_no_py(handle, output.to_str().unwrap()).unwrap();
        let bytes = std::fs::read(output).unwrap();
        assert!(plugin_handle_close_native(handle));
        (format!("{report:?}"), bytes, elapsed)
    }

    #[test]
    fn partitioned_repair_sets_preserve_full_fixup_output_and_report() {
        let repeated = run_full_fixup(linked_ref_fixture(1_000, true), true);
        let partitioned = run_full_fixup(linked_ref_fixture(1_000, true), false);
        assert_eq!(repeated.0, partitioned.0);
        assert_eq!(repeated.1, partitioned.1);
    }

    #[test]
    fn partitioned_repair_sets_match_repeated_memberships_for_every_signature() {
        let handle = plugin_handle_new_native("Output.esp", Some("fo4")).unwrap();
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.header.masters = (0..7).map(|i| format!("Master{i}.esm")).collect();
            slot.parsed.root_items = linked_ref_fixture(REPAIR_TARGET_SIGS.len(), false);
            slot.invalidate_sections();
        }
        let interner = crate::sym::StringInterner::new();
        let own_sym = interner.intern("Output.esp");
        let mut session = open_session(handle, None).unwrap();
        let repeated =
            collect_repair_sets_repeated_for_test(&mut session, &interner, own_sym).unwrap();
        let partitioned = collect_repair_sets(&mut session, &interner, own_sym).unwrap();
        assert_eq!(repeated.own_load_index, partitioned.own_load_index);
        assert_eq!(
            repeated.own_linked_keyword_or_ref_ids,
            partitioned.own_linked_keyword_or_ref_ids
        );
        assert_eq!(repeated.own_linked_ref_ids, partitioned.own_linked_ref_ids);
        assert_eq!(
            repeated.own_spline_target_ids,
            partitioned.own_spline_target_ids
        );
        assert_eq!(repeated.own_key_ids, partitioned.own_key_ids);
        assert_eq!(repeated.own_navmesh_ids, partitioned.own_navmesh_ids);
        drop(session);
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    #[ignore = "representative cold whole-fixup benchmark"]
    fn benchmark_partitioned_repair_sets_against_repeated_enumeration() {
        const SAMPLES: usize = 6;
        for sample in 0..SAMPLES {
            let mut pair = Vec::new();
            for repeated in if sample % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                let result = run_full_fixup(linked_ref_fixture(200_000, false), repeated);
                eprintln!(
                    "linked_ref_sets sample={} mode={} elapsed_ms={:.3} bytes={} report={}",
                    sample + 1,
                    if repeated { "repeated" } else { "partitioned" },
                    result.2 * 1000.0,
                    result.1.len(),
                    result.0,
                );
                pair.push(result);
            }
            assert_eq!(pair[0].0, pair[1].0);
            assert_eq!(pair[0].1, pair[1].1);
        }
    }

    fn empty_master_sigs() -> FxHashMap<u32, Option<SigCode>> {
        FxHashMap::default()
    }

    #[test]
    fn repairs_reported_direct_travel_ref_slot() {
        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&0x0763_2375_u32.to_le_bytes());
        xlkr.extend_from_slice(&0x002C_7635_u32.to_le_bytes());

        let changed = repair_xlkr_bytes(&mut xlkr, &sets(), &empty_master_sigs());

        assert!(changed);
        assert_eq!(
            u32::from_le_bytes(xlkr[0..4].try_into().unwrap()),
            0x0763_2375
        );
        assert_eq!(
            u32::from_le_bytes(xlkr[4..8].try_into().unwrap()),
            0x072C_7635
        );
    }

    #[test]
    fn repairs_keyword_and_ref_slots_when_both_are_master_prefixed() {
        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&0x0063_2375_u32.to_le_bytes());
        xlkr.extend_from_slice(&0x002C_7635_u32.to_le_bytes());

        let changed = repair_xlkr_bytes(&mut xlkr, &sets(), &empty_master_sigs());

        assert!(changed);
        assert_eq!(
            u32::from_le_bytes(xlkr[0..4].try_into().unwrap()),
            0x0763_2375
        );
        assert_eq!(
            u32::from_le_bytes(xlkr[4..8].try_into().unwrap()),
            0x072C_7635
        );
    }

    #[test]
    fn starfield_to_fo4_repairs_or_nulls_live_xlkr_targets() {
        let mut missing_actor = Vec::new();
        missing_actor.extend_from_slice(&0u32.to_le_bytes());
        missing_actor.extend_from_slice(&0x0024_08D3_u32.to_le_bytes());
        let mut surviving_ref = Vec::new();
        surviving_ref.extend_from_slice(&0u32.to_le_bytes());
        surviving_ref.extend_from_slice(&0x0018_DF32_u32.to_le_bytes());
        let mut placed = record(vec![
            subrecord("XLKR", missing_actor),
            subrecord("XLKR", surviving_ref),
        ]);
        let mut master_sigs = FxHashMap::default();
        master_sigs.insert(0x0024_08D3, Some(SigCode::from_str("SNDR").unwrap()));
        master_sigs.insert(0x0018_DF32, Some(SigCode::from_str("NAVM").unwrap()));

        let changed = repair_placed_ref_record(&mut placed, &sets(), &master_sigs);

        assert_eq!(changed, 2);
        assert_eq!(
            read_formid(placed.subrecords[0].data.as_ref(), XLKR_REF_OFFSET),
            Some(0)
        );
        assert_eq!(
            read_formid(placed.subrecords[1].data.as_ref(), XLKR_REF_OFFSET),
            Some(0x0718_DF32)
        );
    }

    #[test]
    fn starfield_to_fo4_rewrites_live_xndp_to_output_navmesh() {
        let mut placed = record(vec![subrecord("XNDP", xndp(0x002F_1305, 385))]);
        let mut master_sigs = FxHashMap::default();
        master_sigs.insert(0x002F_1305, Some(SigCode::from_str("NAVM").unwrap()));

        let changed = repair_placed_ref_record(&mut placed, &sets(), &master_sigs);

        assert_eq!(changed, 1);
        assert_eq!(
            read_formid(placed.subrecords[0].data.as_ref(), XNDP_NAVM_OFFSET),
            Some(0x072F_1305)
        );
        assert_eq!(
            i16::from_le_bytes(placed.subrecords[0].data[4..6].try_into().unwrap()),
            385
        );
    }

    #[test]
    fn starfield_to_fo4_drops_live_xndp_when_navmesh_was_not_emitted() {
        let mut placed = record(vec![subrecord("XNDP", xndp(0x0014_122D, 154))]);
        let mut master_sigs = FxHashMap::default();
        master_sigs.insert(0x0014_122D, Some(SigCode::from_str("REFR").unwrap()));

        let changed = repair_placed_ref_record(&mut placed, &sets(), &master_sigs);

        assert_eq!(changed, 1);
        assert!(placed.subrecords.is_empty());
    }

    #[test]
    fn keeps_xndp_with_valid_target_master_navmesh() {
        let mut placed = record(vec![subrecord("XNDP", xndp(0x0000_1234, 9))]);
        let mut master_sigs = FxHashMap::default();
        master_sigs.insert(0x0000_1234, Some(SigCode::from_str("NAVM").unwrap()));

        let changed = repair_placed_ref_record(&mut placed, &sets(), &master_sigs);

        assert_eq!(changed, 0);
        assert_eq!(placed.subrecords.len(), 1);
        assert_eq!(
            read_formid(placed.subrecords[0].data.as_ref(), XNDP_NAVM_OFFSET),
            Some(0x0000_1234)
        );
    }

    #[test]
    fn leaves_already_own_xlkr_untouched() {
        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&0x0763_2375_u32.to_le_bytes());
        xlkr.extend_from_slice(&0x072C_7635_u32.to_le_bytes());

        let changed = repair_xlkr_bytes(&mut xlkr, &sets(), &empty_master_sigs());

        assert!(!changed);
    }

    #[test]
    fn nulls_master_ref_without_own_shadow_or_valid_master_target() {
        let mut xlkr = Vec::new();
        xlkr.extend_from_slice(&0x0063_2375_u32.to_le_bytes());
        xlkr.extend_from_slice(&0x002C_7635_u32.to_le_bytes());
        let no_own = RepairSets {
            own_load_index: 7,
            own_linked_keyword_or_ref_ids: ids(&[]),
            own_linked_ref_ids: ids(&[]),
            own_spline_target_ids: ids(&[]),
            own_key_ids: ids(&[]),
            own_navmesh_ids: ids(&[]),
        };

        let changed = repair_xlkr_bytes(&mut xlkr, &no_own, &empty_master_sigs());

        assert!(changed);
        assert_eq!(u32::from_le_bytes(xlkr[0..4].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(xlkr[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn keeps_valid_master_target_without_own_shadow() {
        let raw = 0x0000_1234;
        let mut master_sigs = FxHashMap::default();
        master_sigs.insert(raw, Some(SigCode::from_str("REFR").unwrap()));

        assert_eq!(
            repair_ref_slot(raw, 7, &ids(&[]), LINKED_REF_TARGET_SIGS, &master_sigs),
            SlotRepair::Keep
        );
    }

    #[test]
    fn keeps_valid_master_target_when_output_object_id_collides() {
        let raw = 0x0039_E691;
        let mut master_sigs = FxHashMap::default();
        master_sigs.insert(raw, Some(SigCode::from_str("REFR").unwrap()));

        assert_eq!(
            repair_ref_slot(
                raw,
                7,
                &ids(&[0x39_E691]),
                LINKED_REF_TARGET_SIGS,
                &master_sigs,
            ),
            SlotRepair::Keep
        );
    }

    #[test]
    fn repairs_repeated_xplk_rows_and_drops_invalid_rows() {
        let mut valid = Vec::new();
        valid.extend_from_slice(&0x0039_E691_u32.to_le_bytes());
        valid.extend_from_slice(&0u32.to_le_bytes());
        let mut invalid = Vec::new();
        invalid.extend_from_slice(&0x0044_4444_u32.to_le_bytes());
        invalid.extend_from_slice(&0u32.to_le_bytes());
        let mut placed = record(vec![subrecord("XPLK", valid), subrecord("XPLK", invalid)]);

        let changed = repair_placed_ref_record(&mut placed, &sets(), &empty_master_sigs());

        assert_eq!(changed, 2);
        assert!(placed.raw_payload.is_none());
        assert_eq!(placed.subrecords.len(), 1);
        let data = &placed.subrecords[0].data;
        assert_eq!(
            u32::from_le_bytes(data[0..4].try_into().unwrap()),
            0x0739_E691
        );
    }

    #[test]
    fn nulls_unresolved_xloc_key_without_output_key() {
        let mut xloc = vec![0, 0, 0, 0];
        xloc.extend_from_slice(&0x0055_ADA7_u32.to_le_bytes());
        xloc.extend_from_slice(&[0, 0, 0, 0]);
        let no_key = RepairSets {
            own_load_index: 7,
            own_linked_keyword_or_ref_ids: ids(&[]),
            own_linked_ref_ids: ids(&[]),
            own_spline_target_ids: ids(&[]),
            own_key_ids: ids(&[]),
            own_navmesh_ids: ids(&[]),
        };
        let mut placed = record(vec![subrecord("XLOC", xloc)]);

        let changed = repair_placed_ref_record(&mut placed, &no_key, &empty_master_sigs());

        assert_eq!(changed, 1);
        let data = &placed.subrecords[0].data;
        assert_eq!(u32::from_le_bytes(data[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn starfield_to_fo4_nulls_missing_live_xloc_key_without_mutating_flags() {
        let mut xloc = vec![0, 0, 0, 0];
        xloc.extend_from_slice(&0x003B_5D2B_u32.to_le_bytes());
        xloc.extend_from_slice(&[1, 0, 0, 0, 8, 0, 0, 0]);
        let no_key = RepairSets {
            own_load_index: 7,
            own_linked_keyword_or_ref_ids: ids(&[]),
            own_linked_ref_ids: ids(&[]),
            own_spline_target_ids: ids(&[]),
            own_key_ids: ids(&[]),
            own_navmesh_ids: ids(&[]),
        };
        let mut placed = record(vec![subrecord("XLOC", xloc)]);

        let changed = repair_placed_ref_record(&mut placed, &no_key, &empty_master_sigs());

        assert_eq!(changed, 1);
        let data = &placed.subrecords[0].data;
        assert_eq!(read_formid(data.as_ref(), XLOC_KEY_OFFSET), Some(0));
        assert_eq!(data[8], 1);
        assert_eq!(&data[12..16], &[8, 0, 0, 0]);
    }

    #[test]
    fn repairs_xloc_key_when_output_key_exists() {
        let mut xloc = vec![0, 0, 0, 0];
        xloc.extend_from_slice(&0x0055_ADA7_u32.to_le_bytes());
        xloc.extend_from_slice(&[0, 0, 0, 0]);
        let mut placed = record(vec![subrecord("XLOC", xloc)]);

        let changed = repair_placed_ref_record(&mut placed, &sets(), &empty_master_sigs());

        assert_eq!(changed, 1);
        let data = &placed.subrecords[0].data;
        assert_eq!(
            u32::from_le_bytes(data[4..8].try_into().unwrap()),
            0x0755_ADA7
        );
    }

    #[test]
    fn nulls_xesp_when_master_ref_is_not_valid_and_no_output_ref_exists() {
        let mut xesp = Vec::new();
        xesp.extend_from_slice(&0x003F_17D4_u32.to_le_bytes());
        xesp.extend_from_slice(&[0, 0, 0, 0]);
        let mut placed = record(vec![subrecord("XESP", xesp)]);

        let changed = repair_placed_ref_record(&mut placed, &sets(), &empty_master_sigs());

        assert_eq!(changed, 1);
        let data = &placed.subrecords[0].data;
        assert_eq!(u32::from_le_bytes(data[0..4].try_into().unwrap()), 0);
    }

    #[test]
    fn repairs_xesp_when_output_ref_exists() {
        let mut xesp = Vec::new();
        xesp.extend_from_slice(&0x003F_17D5_u32.to_le_bytes());
        xesp.extend_from_slice(&[0, 0, 0, 0]);
        let mut placed = record(vec![subrecord("XESP", xesp)]);

        let changed = repair_placed_ref_record(&mut placed, &sets(), &empty_master_sigs());

        assert_eq!(changed, 1);
        let data = &placed.subrecords[0].data;
        assert_eq!(
            u32::from_le_bytes(data[0..4].try_into().unwrap()),
            0x073F_17D5
        );
    }

    #[test]
    fn repairs_xapr_when_output_ref_exists() {
        let mut xapr = Vec::new();
        xapr.extend_from_slice(&0x003F_17D5_u32.to_le_bytes());
        xapr.extend_from_slice(&[0, 0, 0, 0]);
        let mut placed = record(vec![subrecord("XAPR", xapr)]);

        let changed = repair_placed_ref_record(&mut placed, &sets(), &empty_master_sigs());

        assert_eq!(changed, 1);
        let data = &placed.subrecords[0].data;
        assert_eq!(
            u32::from_le_bytes(data[0..4].try_into().unwrap()),
            0x073F_17D5
        );
    }
}
