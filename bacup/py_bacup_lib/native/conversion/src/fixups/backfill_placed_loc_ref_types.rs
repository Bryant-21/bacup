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

use crate::fixups::synthesize_workshop_boundaries::{
    LCSR_ROW_STRIDE, decode_target_record_opt, read_raw_form_id, struct_form_key,
    target_form_key_from_raw,
};
use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

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
    let target_masters = session.target_masters().to_vec();

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
        for (ref_fk, ref_type) in
            special_ref_type_pairs(&location, &target_masters, own_plugin, interner)
        {
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
    let mut replacements = Vec::new();
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
            replacements.push(record);
        }
    }
    report.records_changed = session
        .replace_records_contents(replacements, target_schema.as_ref(), interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
        .try_into()
        .unwrap_or(u32::MAX);

    Ok(report)
}

/// The pipeline record model keeps LCTN special-ref arrays RAW (see
/// `rewrite_raw_lctn_formids`), so live records carry LCSR as `Bytes`;
/// unit-built records carry decoded `List` rows. Handle both — reading only the
/// `List` shape silently matched nothing in production, so every location was
/// skipped and no XLRT was ever backfilled.
fn special_ref_type_pairs(
    record: &Record,
    target_masters: &[String],
    own_plugin: Sym,
    interner: &StringInterner,
) -> Vec<(FormKey, FormKey)> {
    let mut out = Vec::new();
    for entry in &record.fields {
        if entry.sig.as_str() != "LCSR" {
            continue;
        }
        match &entry.value {
            FieldValue::List(rows) => {
                for row in rows {
                    let ref_fk = struct_form_key(row, "master_special_references_ref", interner);
                    let type_fk =
                        struct_form_key(row, "master_special_references_loc_ref_type", interner);
                    if let (Some(ref_fk), Some(type_fk)) = (ref_fk, type_fk) {
                        out.push((ref_fk, type_fk));
                    }
                }
            }
            // Row layout: [LocRefType u32][Ref u32][WorldCell u32][GridY i16][GridX i16].
            FieldValue::Bytes(bytes) if bytes.len() % LCSR_ROW_STRIDE == 0 => {
                for row in bytes.chunks_exact(LCSR_ROW_STRIDE) {
                    let type_fk = read_raw_form_id(row, 0).and_then(|raw| {
                        target_form_key_from_raw(raw, target_masters, own_plugin, interner)
                    });
                    let ref_fk = read_raw_form_id(row, 4).and_then(|raw| {
                        target_form_key_from_raw(raw, target_masters, own_plugin, interner)
                    });
                    if let (Some(ref_fk), Some(type_fk)) = (ref_fk, type_fk) {
                        out.push((ref_fk, type_fk));
                    }
                }
            }
            _ => {}
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
    fn batched_backfill_preserves_placed_groups_and_is_idempotent() {
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::schema::AuthoringSchema;
        use crate::session::open_session;
        use esp_authoring_core::plugin_runtime::{
            ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_new_native,
        };

        let handle = plugin_handle_new_native("Performance.esm", Some("fo4")).unwrap();
        let mut session = open_session(handle, None).unwrap();
        let raw_record = |sig: &str, form_id, subrecords| {
            ParsedItem::Record(ParsedRecord {
                signature: sig.into(),
                form_id,
                flags: 0,
                version_control: 0,
                form_version: Some(131),
                version2: Some(0),
                subrecords,
                raw_payload: None,
                parse_error: None,
            })
        };
        let subrecord = |sig: &str, data: Vec<u8>| ParsedSubrecord {
            signature: sig.into(),
            data: data.into(),
            semantic_type: None,
        };
        let mut rows = Vec::new();
        let mut placed = Vec::new();
        for form_id in 0x2000u32..0x2010 {
            rows.extend_from_slice(&0xF0001u32.to_le_bytes());
            rows.extend_from_slice(&form_id.to_le_bytes());
            rows.extend_from_slice(&[0; 8]);
            placed.push(raw_record(
                "REFR",
                form_id,
                vec![
                    subrecord("NAME", 0xF0002u32.to_le_bytes().to_vec()),
                    subrecord("DATA", vec![0; 24]),
                ],
            ));
        }
        session.target_slot_mut().parsed.root_items = vec![
            ParsedItem::Group(ParsedGroup {
                label: *b"LCTN",
                group_type: 0,
                tail: Vec::new().into(),
                children: vec![raw_record("LCTN", 0x800, vec![subrecord("LCSR", rows)])],
            }),
            ParsedItem::Group(ParsedGroup {
                label: *b"CELL",
                group_type: 0,
                tail: Vec::new().into(),
                children: vec![ParsedItem::Group(ParsedGroup {
                    label: 0x1000u32.to_le_bytes(),
                    group_type: 9,
                    tail: Vec::new().into(),
                    children: placed,
                })],
            }),
        ];
        let interner = StringInterner::new();
        let mut state = MapperState::new([], MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let config = FixupConfig {
            target_schema: Some(AuthoringSchema::for_game("fo4").unwrap()),
            ..FixupConfig::default()
        };
        assert_eq!(
            backfill_placed_loc_ref_types(&mut session, &mut mapper, &config)
                .unwrap()
                .records_changed,
            16
        );
        assert_eq!(
            backfill_placed_loc_ref_types(&mut session, &mut mapper, &config)
                .unwrap()
                .records_changed,
            0
        );
        let ParsedItem::Group(top) = &session.target_slot().parsed.root_items[1] else {
            panic!()
        };
        let ParsedItem::Group(cell) = &top.children[0] else {
            panic!()
        };
        assert_eq!((cell.label, cell.group_type), (0x1000u32.to_le_bytes(), 9));
        assert_eq!(cell.children.len(), 16);
        for (index, item) in cell.children.iter().enumerate() {
            let ParsedItem::Record(record) = item else {
                panic!()
            };
            assert_eq!(record.form_id, 0x2000 + index as u32);
            let xlrt = record
                .subrecords
                .iter()
                .find(|sub| sub.signature == "XLRT")
                .unwrap();
            assert_eq!(xlrt.data.as_ref(), &0xF0001u32.to_le_bytes());
            assert_eq!(record.subrecords.last().unwrap().signature, "DATA");
        }
    }

    /// Production LCTNs carry LCSR as a raw `Bytes` blob, not decoded `List`
    /// rows. Reading only the `List` shape made this pass a silent no-op: every
    /// location was skipped, no XLRT was backfilled, and location-scoped quest
    /// aliases could not fill (which makes FO4 refuse the quest start outright).
    #[test]
    fn special_ref_pairs_read_the_raw_bytes_lcsr_shape() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let masters = vec!["Fallout4.esm".to_string()];

        // One row: [LocRefType 0x01405EA2][Ref 0x01405EA3][WorldCell][GridY][GridX]
        let mut row: SmallVec<[u8; 32]> = SmallVec::new();
        row.extend_from_slice(&0x01405EA2_u32.to_le_bytes());
        row.extend_from_slice(&0x01405EA3_u32.to_le_bytes());
        row.extend_from_slice(&0u32.to_le_bytes());
        row.extend_from_slice(&0i16.to_le_bytes());
        row.extend_from_slice(&0i16.to_le_bytes());
        assert_eq!(row.len(), LCSR_ROW_STRIDE);

        let location = placed_record(
            "LCTN",
            fk(0x405E9F, plugin),
            smallvec![field("LCSR", FieldValue::Bytes(row))],
        );

        let pairs = special_ref_type_pairs(&location, &masters, plugin, &interner);

        assert_eq!(pairs.len(), 1, "raw Bytes LCSR must yield a pair");
        let (ref_fk, type_fk) = pairs[0];
        assert_eq!(ref_fk.local, 0x405EA3);
        assert_eq!(type_fk.local, 0x405EA2);
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
