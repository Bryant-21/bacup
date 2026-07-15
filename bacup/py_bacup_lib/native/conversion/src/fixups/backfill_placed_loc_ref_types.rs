//! Fixup: backfill ref-side XLRT (Location Ref Type) from LCTN LCSR rows.
//!
//! FO76 bakes each location's special-ref list into LCSR but ships no XLRT on
//! the placed refs themselves. FO4's location system registers a ref's
//! reftypes from the REF's XLRT (vanilla boss actors always carry it), so
//! without it boss kills never decrement the location's tracking and clearable
//! locations can never flip to [CLEARED]. Runs AFTER
//! `synthesize_workshop_boundaries` so Boss rows stripped from workshop
//! locations are not re-applied to their defender spawns.

use rustc_hash::FxHashMap;

use crate::fixups::synthesize_workshop_boundaries::{decode_target_record_opt, struct_form_key};
use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const PLACED_SIGS: &[&str] = &[
    "ACHR", "REFR", "PGRE", "PHZD", "PMIS", "PARW", "PBAR", "PBEA", "PCON", "PFLA",
];

pub fn backfill_placed_loc_ref_types(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let target_schema = config
        .target_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing target schema".into()))?;
    let interner = mapper.interner;
    let own_plugin = interner.intern(&session.target_slot().parsed.plugin_name);

    let lctn_sig = SigCode::from_str("LCTN").map_err(FixupError::SchemaError)?;
    let lctn_fks = session
        .form_keys_of_sig(lctn_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;

    let mut wanted: FxHashMap<FormKey, Vec<FormKey>> = FxHashMap::default();
    for lctn_fk in lctn_fks {
        if lctn_fk.plugin != own_plugin {
            continue;
        }
        let Some(location) =
            decode_target_record_opt(session, &lctn_fk, target_schema.as_ref(), interner)?
        else {
            continue;
        };
        for (ref_fk, ref_type) in special_ref_type_pairs(&location, interner) {
            if ref_fk.plugin != own_plugin {
                continue;
            }
            let types = wanted.entry(ref_fk).or_default();
            if !types.contains(&ref_type) {
                types.push(ref_type);
            }
        }
    }

    let placed_sigs: Vec<SigCode> = PLACED_SIGS
        .iter()
        .map(|sig| SigCode::from_str(sig).map_err(FixupError::SchemaError))
        .collect::<Result<_, _>>()?;

    let mut targets: Vec<(FormKey, Vec<FormKey>)> = wanted.into_iter().collect();
    targets.sort_by_key(|(fk, _)| fk.local);
    for (ref_fk, mut types) in targets {
        let Some(mut record) =
            decode_target_record_opt(session, &ref_fk, target_schema.as_ref(), interner)?
        else {
            continue;
        };
        if !placed_sigs.contains(&record.sig) {
            continue;
        }
        types.sort_by_key(|fk| (interner.resolve(fk.plugin), fk.local));
        if merge_loc_ref_types(&mut record, &types) {
            if session
                .replace_record_contents(record, target_schema.as_ref(), interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?
            {
                report.records_changed = report.records_changed.saturating_add(1);
            }
        }
    }

    Ok(report)
}

fn special_ref_type_pairs(record: &Record, interner: &StringInterner) -> Vec<(FormKey, FormKey)> {
    let mut out = Vec::new();
    for entry in &record.fields {
        if entry.sig.as_str() != "LCSR" {
            continue;
        }
        let FieldValue::List(rows) = &entry.value else {
            continue;
        };
        for row in rows {
            let ref_fk = struct_form_key(row, "master_special_references_ref", interner);
            let type_fk = struct_form_key(row, "master_special_references_loc_ref_type", interner);
            if let (Some(ref_fk), Some(type_fk)) = (ref_fk, type_fk) {
                out.push((ref_fk, type_fk));
            }
        }
    }
    out
}

fn merge_loc_ref_types(record: &mut Record, types: &[FormKey]) -> bool {
    if types.is_empty() {
        return false;
    }
    if let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "XLRT")
    {
        let FieldValue::List(existing) = &mut entry.value else {
            return false;
        };
        let mut changed = false;
        for type_fk in types {
            let already = existing
                .iter()
                .any(|value| matches!(value, FieldValue::FormKey(fk) if fk == type_fk));
            if !already {
                existing.push(FieldValue::FormKey(*type_fk));
                changed = true;
            }
        }
        return changed;
    }

    let insert_at = xlrt_insert_index(record);
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: xlrt_sig(),
            value: FieldValue::List(types.iter().copied().map(FieldValue::FormKey).collect()),
        },
    );
    true
}

// XLRT sits between the base/level-modifier block and the X* link subrecords
// in the FO4 placed-ref layout.
fn xlrt_insert_index(record: &Record) -> usize {
    let mut index = 0;
    for (i, entry) in record.fields.iter().enumerate() {
        match entry.sig.as_str() {
            "EDID" | "VMAD" | "NAME" | "XLCM" => index = i + 1,
            _ => {}
        }
    }
    index
}

fn xlrt_sig() -> SubrecordSig {
    SubrecordSig::from_str("XLRT").expect("hard-coded subrecord signature must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SigCode;
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;
    use smallvec::{SmallVec, smallvec};

    fn fk(local: u32, plugin: crate::sym::Sym) -> FormKey {
        FormKey { local, plugin }
    }

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn placed_record(sig: &str, form_key: FormKey, fields: SmallVec<[FieldEntry; 8]>) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key,
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn merge_inserts_xlrt_after_base_block() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let fo4 = interner.intern("Fallout4.esm");
        let boss = fk(0x003956, fo4);
        let mut record = placed_record(
            "ACHR",
            fk(0x29C2DA, plugin),
            smallvec![
                field("NAME", FieldValue::FormKey(fk(0xF00017, plugin))),
                field("XLCM", FieldValue::Int(3)),
                field("XLKR", FieldValue::Struct(Vec::new())),
                field("DATA", FieldValue::Struct(Vec::new())),
            ],
        );

        assert!(merge_loc_ref_types(&mut record, &[boss]));
        assert_eq!(record.fields[2].sig.as_str(), "XLRT");
        let FieldValue::List(values) = &record.fields[2].value else {
            panic!("XLRT list");
        };
        assert_eq!(values.len(), 1);
        assert!(matches!(values[0], FieldValue::FormKey(t) if t == boss));

        // idempotent
        assert!(!merge_loc_ref_types(&mut record, &[boss]));
    }

    #[test]
    fn merge_appends_missing_types_to_existing_xlrt() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let fo4 = interner.intern("Fallout4.esm");
        let boss = fk(0x003956, fo4);
        let other = fk(0x1D50B8, plugin);
        let mut record = placed_record(
            "ACHR",
            fk(0x29C2DA, plugin),
            smallvec![
                field("NAME", FieldValue::FormKey(fk(0xF00017, plugin))),
                field("XLRT", FieldValue::List(vec![FieldValue::FormKey(other)]),),
                field("DATA", FieldValue::Struct(Vec::new())),
            ],
        );

        assert!(merge_loc_ref_types(&mut record, &[boss, other]));
        let FieldValue::List(values) = &record.fields[1].value else {
            panic!("XLRT list");
        };
        assert_eq!(values.len(), 2);
    }
}
