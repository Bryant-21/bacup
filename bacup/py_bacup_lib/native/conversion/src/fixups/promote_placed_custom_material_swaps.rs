//! Promote CK-generated custom material swaps used by placed references.
//!
//! FO4's master-file validation rejects a placed `REFR.XMSP` that targets a
//! custom `MSWP`. FO76 contains such placements, including 479A26-479A29,
//! which target `CustomMaterialSwap003A5B63`. The material pairs are valid and
//! must remain; only the swap's custom status is incompatible with an FO4
//! master.

use rustc_hash::FxHashSet;

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

const CUSTOM_MATERIAL_SWAP_EDITOR_ID_PREFIX: &str = "CustomMaterialSwap";
const WHITESPRING_X01_DISPLAY_BASE: u32 = 0x0020_F450;
const WHITESPRING_MILITARY_WING_LAYER: u32 = 0x002A_39BA;
const X01_ENCLAVE_MATERIAL_SWAP: u32 = 0x0033_870D;
const WHITESPRING_X01_DISPLAY_REFS: &[u32] = &[
    0x002A_3D0D,
    0x002A_3D17,
    0x002A_3D5C,
    0x002A_3D5E,
    0x002A_3D60,
    0x002A_3D62,
    0x002A_3D72,
    0x002A_3D81,
    0x002A_3DA4,
    0x002A_3DA6,
    0x002A_3DA8,
    0x002A_3DAE,
    0x002A_3DB1,
    0x002A_3DEC,
];

fn own_material_swap_object_id(xmsp: &[u8], own_load_index: usize) -> Option<u32> {
    let bytes = xmsp.get(..4)?;
    let raw = u32::from_le_bytes(bytes.try_into().ok()?);
    let object_id = raw & 0x00FF_FFFF;
    (object_id != 0 && (raw >> 24) as usize == own_load_index).then_some(object_id)
}

fn promote_custom_material_swap(record: &mut Record, interner: &StringInterner) -> bool {
    if record.sig.as_str() != "MSWP" || !record.flags.contains(RecordFlags::RANDOM_ANIM_START) {
        return false;
    }

    record.flags.remove(RecordFlags::RANDOM_ANIM_START);
    let needs_stable_editor_id = record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .is_none_or(|eid| eid.starts_with(CUSTOM_MATERIAL_SWAP_EDITOR_ID_PREFIX));
    if needs_stable_editor_id {
        let editor_id = interner.intern(&format!(
            "B21_FO76_MSWP_{:06X}",
            record.form_key.local & 0x00FF_FFFF
        ));
        record.eid = Some(editor_id);
        if let Some(field) = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"EDID")
        {
            field.value = FieldValue::String(editor_id);
        } else {
            record.fields.insert(
                0,
                FieldEntry {
                    sig: SubrecordSig(*b"EDID"),
                    value: FieldValue::String(editor_id),
                },
            );
        }
    }
    true
}

fn form_key_field(record: &Record, sig: &[u8; 4]) -> Option<FormKey> {
    record.fields.iter().find_map(|field| {
        if field.sig.0 != *sig {
            return None;
        }
        match &field.value {
            FieldValue::FormKey(form_key) => Some(*form_key),
            _ => None,
        }
    })
}

fn repair_whitespring_x01_display_swap(record: &mut Record, own_plugin: Sym) -> bool {
    if record.form_key.plugin != own_plugin
        || !WHITESPRING_X01_DISPLAY_REFS.contains(&(record.form_key.local & 0x00FF_FFFF))
        || form_key_field(record, b"NAME")
            != Some(FormKey {
                local: WHITESPRING_X01_DISPLAY_BASE,
                plugin: own_plugin,
            })
        || form_key_field(record, b"XLYR")
            != Some(FormKey {
                local: WHITESPRING_MILITARY_WING_LAYER,
                plugin: own_plugin,
            })
        || record.fields.iter().any(|field| field.sig.0 == *b"XMSP")
    {
        return false;
    }

    let insert_at = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"DATA")
        .unwrap_or(record.fields.len());
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: SubrecordSig(*b"XMSP"),
            value: FieldValue::FormKey(FormKey {
                local: X01_ENCLAVE_MATERIAL_SWAP,
                plugin: own_plugin,
            }),
        },
    );
    true
}

/// Repair known missing placed swaps and promote custom swaps reached by
/// `REFR.XMSP` records.
///
/// The caller gates this to FO76→FO4 and runs it after every placed-child copy
/// path has completed.
pub fn promote_placed_custom_material_swaps(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let interner = mapper.interner;
    let target_schema = config
        .target_schema
        .as_deref()
        .ok_or_else(|| FixupError::SchemaError("target schema unavailable".into()))?;
    let own_name = session.target_slot().parsed.plugin_name.clone();
    let own_sym = interner.intern(&own_name);
    let own_load_index = session.target_masters().len();
    let refr_sig =
        SigCode::from_str("REFR").map_err(|error| FixupError::SchemaError(error.to_string()))?;
    let mswp_sig =
        SigCode::from_str("MSWP").map_err(|error| FixupError::SchemaError(error.to_string()))?;

    let refr_fks = session
        .form_keys_of_sig(refr_sig, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let mswp_fks = session
        .form_keys_of_sig(mswp_sig, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let enclave_swap_fk = FormKey {
        local: X01_ENCLAVE_MATERIAL_SWAP,
        plugin: own_sym,
    };
    if mswp_fks.contains(&enclave_swap_fk) {
        let mut repaired = Vec::new();
        for fk in refr_fks.iter().filter(|fk| {
            fk.plugin == own_sym && WHITESPRING_X01_DISPLAY_REFS.contains(&(fk.local & 0x00FF_FFFF))
        }) {
            let mut record = session
                .record_decoded(fk, target_schema, interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            if repair_whitespring_x01_display_swap(&mut record, own_sym) {
                repaired.push(record);
            }
        }
        // Content-only replace: a structural replace would lift these placed
        // refs out of their cell's child group and re-add them to a top-level
        // REFR group, which strands them outside any cell.
        let repaired_count = session
            .replace_records_contents(repaired, target_schema, interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        report.records_changed = repaired_count.try_into().unwrap_or(u32::MAX);
    }

    let mut referenced_swap_ids = FxHashSet::default();
    for fk in refr_fks {
        let Some(xmsp) = session
            .first_subrecord_bytes(&fk, "XMSP")
            .map_err(|error| FixupError::HandleError(error.to_string()))?
        else {
            continue;
        };
        if let Some(object_id) = own_material_swap_object_id(&xmsp, own_load_index) {
            referenced_swap_ids.insert(object_id);
        }
    }
    if referenced_swap_ids.is_empty() {
        return Ok(report);
    }

    let mut promoted = Vec::new();
    for fk in mswp_fks {
        if fk.plugin != own_sym || !referenced_swap_ids.contains(&(fk.local & 0x00FF_FFFF)) {
            continue;
        }
        let mut record = session
            .record_decoded(&fk, target_schema, interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if promote_custom_material_swap(&mut record, interner) {
            promoted.push(record);
        }
    }

    let promoted_count = promoted.len();
    session
        .replace_records(promoted, target_schema, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    report.records_changed = report
        .records_changed
        .saturating_add(promoted_count.try_into().unwrap_or(u32::MAX));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::FormKey;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_add_master_native,
        plugin_handle_close_native, plugin_handle_new_native, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    fn material_field(sig: &str, value: &str, interner: &StringInterner) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::String(interner.intern(value)),
        }
    }

    #[test]
    fn promotes_reported_custom_swap_without_changing_material_pairs() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("MSWP").unwrap(),
            FormKey {
                local: 0x003A_5B64,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        record.eid = Some(interner.intern("CustomMaterialSwap003A5B63"));
        record.flags = RecordFlags::RANDOM_ANIM_START | RecordFlags::IGNORED;
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig::from_str("EDID").unwrap(),
                value: FieldValue::String(interner.intern("CustomMaterialSwap003A5B63")),
            },
            material_field(
                "BNAM",
                "FO76\\Architecture\\Buildings\\brickred01.bgsm",
                &interner,
            ),
            material_field(
                "SNAM",
                "architecture\\buildings\\BricksFactory01.BGSM",
                &interner,
            ),
            material_field(
                "BNAM",
                "Architecture\\Buildings\\brickredcornerdecal01.bgsm",
                &interner,
            ),
            material_field(
                "SNAM",
                "architecture\\buildings\\BrickEdgeFactoryNoDecal01.BGSM",
                &interner,
            ),
        ]);
        let material_pairs = record.fields[1..].to_vec();

        assert!(promote_custom_material_swap(&mut record, &interner));
        assert!(!record.flags.contains(RecordFlags::RANDOM_ANIM_START));
        assert!(record.flags.contains(RecordFlags::IGNORED));
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("B21_FO76_MSWP_3A5B64")
        );
        assert_eq!(record.fields[1..], material_pairs);
        assert_eq!(
            record.fields[0].value,
            FieldValue::String(interner.intern("B21_FO76_MSWP_3A5B64"))
        );
        assert!(!promote_custom_material_swap(&mut record, &interner));
    }

    #[test]
    fn preserves_descriptive_editor_id_while_clearing_custom_status() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("MSWP").unwrap(),
            FormKey {
                local: 0x123456,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        record.eid = Some(interner.intern("BrickFactorySwap"));
        record.flags = RecordFlags::RANDOM_ANIM_START;

        assert!(promote_custom_material_swap(&mut record, &interner));
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("BrickFactorySwap")
        );
    }

    #[test]
    fn repairs_whitespring_x01_display_with_enclave_swap_before_data() {
        let interner = StringInterner::new();
        let own_plugin = interner.intern("SeventySix.esm");
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey {
                local: 0x002A_3D62,
                plugin: own_plugin,
            },
        );
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"NAME"),
                value: FieldValue::FormKey(FormKey {
                    local: WHITESPRING_X01_DISPLAY_BASE,
                    plugin: own_plugin,
                }),
            },
            FieldEntry {
                sig: SubrecordSig(*b"XLYR"),
                value: FieldValue::FormKey(FormKey {
                    local: WHITESPRING_MILITARY_WING_LAYER,
                    plugin: own_plugin,
                }),
            },
            FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(Default::default()),
            },
        ]);

        assert!(repair_whitespring_x01_display_swap(&mut record, own_plugin));
        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["NAME", "XLYR", "XMSP", "DATA"]
        );
        assert_eq!(
            form_key_field(&record, b"XMSP"),
            Some(FormKey {
                local: X01_ENCLAVE_MATERIAL_SWAP,
                plugin: own_plugin,
            })
        );
        assert!(!repair_whitespring_x01_display_swap(
            &mut record,
            own_plugin
        ));
    }

    #[test]
    fn selects_only_own_plugin_material_swap_targets() {
        assert_eq!(
            own_material_swap_object_id(&0x073A_5B64_u32.to_le_bytes(), 7),
            Some(0x003A_5B64)
        );
        assert_eq!(
            own_material_swap_object_id(&0x003A_5B64_u32.to_le_bytes(), 7),
            None
        );
        assert_eq!(own_material_swap_object_id(&[0, 0, 0], 7), None);
    }

    #[test]
    fn post_copy_pass_promotes_only_referenced_custom_swaps() {
        let interner = StringInterner::new();
        let plugin_name = "PromotePlacedCustomMaterialSwapsTest.esm";
        let handle = plugin_handle_new_native(plugin_name, Some("fo4")).unwrap();
        plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
        let own_plugin = interner.intern(plugin_name);
        let referenced_fk = FormKey {
            local: 0x003A_5B64,
            plugin: own_plugin,
        };
        let unreferenced_fk = FormKey {
            local: 0x003A_5B65,
            plugin: own_plugin,
        };
        let enclave_swap_fk = FormKey {
            local: X01_ENCLAVE_MATERIAL_SWAP,
            plugin: own_plugin,
        };
        let display_fk = FormKey {
            local: 0x002A_3D62,
            plugin: own_plugin,
        };

        let target_schema = {
            let mut session = crate::session::open_session(handle, None).unwrap();
            let schema = session.schema().unwrap();
            for (fk, eid) in [
                (referenced_fk, "CustomMaterialSwap003A5B63"),
                (unreferenced_fk, "CustomMaterialSwap003A5B65"),
            ] {
                let mut swap = Record::new(SigCode::from_str("MSWP").unwrap(), fk);
                let eid = interner.intern(eid);
                swap.eid = Some(eid);
                swap.flags = RecordFlags::RANDOM_ANIM_START;
                swap.fields.extend([
                    FieldEntry {
                        sig: SubrecordSig::from_str("EDID").unwrap(),
                        value: FieldValue::String(eid),
                    },
                    material_field(
                        "BNAM",
                        "FO76\\Architecture\\Buildings\\brickred01.bgsm",
                        &interner,
                    ),
                    material_field(
                        "SNAM",
                        "architecture\\buildings\\BricksFactory01.BGSM",
                        &interner,
                    ),
                ]);
                session
                    .add_record(swap, schema.as_ref(), &interner)
                    .unwrap();
            }

            let mut enclave_swap = Record::new(SigCode::from_str("MSWP").unwrap(), enclave_swap_fk);
            let enclave_swap_eid = interner.intern("mat_PA_X01_Enclave");
            enclave_swap.eid = Some(enclave_swap_eid);
            enclave_swap.fields.push(FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(enclave_swap_eid),
            });
            session
                .add_record(enclave_swap, schema.as_ref(), &interner)
                .unwrap();

            let mut placed = Record::new(
                SigCode::from_str("REFR").unwrap(),
                FormKey {
                    local: 0x0047_9A29,
                    plugin: own_plugin,
                },
            );
            placed.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("XMSP").unwrap(),
                value: FieldValue::FormKey(referenced_fk),
            });
            session
                .add_record(placed, schema.as_ref(), &interner)
                .unwrap();

            let mut display = Record::new(SigCode::from_str("REFR").unwrap(), display_fk);
            display.fields.extend([
                FieldEntry {
                    sig: SubrecordSig(*b"NAME"),
                    value: FieldValue::FormKey(FormKey {
                        local: WHITESPRING_X01_DISPLAY_BASE,
                        plugin: own_plugin,
                    }),
                },
                FieldEntry {
                    sig: SubrecordSig(*b"XLYR"),
                    value: FieldValue::FormKey(FormKey {
                        local: WHITESPRING_MILITARY_WING_LAYER,
                        plugin: own_plugin,
                    }),
                },
            ]);
            session
                .add_record(display, schema.as_ref(), &interner)
                .unwrap();
            schema
        };

        let mut mapper_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: plugin_name.into(),
                target_master_names: vec!["Fallout4.esm".into()],
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
        let config = FixupConfig {
            target_schema: Some(target_schema.clone()),
            ..Default::default()
        };
        let mut session = crate::session::open_session(handle, None).unwrap();

        let report =
            promote_placed_custom_material_swaps(&mut session, &mut mapper, &config).unwrap();
        assert_eq!(report.records_changed, 2);

        let referenced = session
            .record_decoded(&referenced_fk, target_schema.as_ref(), &interner)
            .unwrap();
        assert!(!referenced.flags.contains(RecordFlags::RANDOM_ANIM_START));
        assert_eq!(
            referenced.eid.and_then(|eid| interner.resolve(eid)),
            Some("B21_FO76_MSWP_3A5B64")
        );

        let unreferenced = session
            .record_decoded(&unreferenced_fk, target_schema.as_ref(), &interner)
            .unwrap();
        assert!(unreferenced.flags.contains(RecordFlags::RANDOM_ANIM_START));
        assert_eq!(
            unreferenced.eid.and_then(|eid| interner.resolve(eid)),
            Some("CustomMaterialSwap003A5B65")
        );

        let display = session
            .record_decoded(&display_fk, target_schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(form_key_field(&display, b"XMSP"), Some(enclave_swap_fk));
        assert_eq!(
            promote_placed_custom_material_swaps(&mut session, &mut mapper, &config)
                .unwrap()
                .records_changed,
            0
        );

        drop(session);
        assert!(plugin_handle_close_native(handle));
    }

    fn parsed_subrecord(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn parsed_record(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new(sig),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        })
    }

    fn parsed_group(label: [u8; 4], group_type: i32, children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label,
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    /// Group-type path from the plugin root down to `form_id`, or `None` when
    /// the record is absent. A placed child nested under its cell yields
    /// `[0, 2, 3, 6, 9]`; one stranded at plugin top level yields `[0]`.
    fn group_type_path(items: &[ParsedItem], form_id: u32) -> Option<Vec<i32>> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id == form_id => return Some(Vec::new()),
                ParsedItem::Group(group) => {
                    if let Some(mut path) = group_type_path(&group.children, form_id) {
                        path.insert(0, group.group_type);
                        return Some(path);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn repaired_display_ref_keeps_its_cell_parentage() {
        let interner = StringInterner::new();
        let plugin_name = "PromotePlacedSwapCellTopologyTest.esm";
        let handle = plugin_handle_new_native(plugin_name, Some("fo4")).unwrap();
        plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
        let own_plugin = interner.intern(plugin_name);
        // One master, so the plugin's own records carry load index 1.
        let cell_form_id = 0x0100_0900_u32;
        let display_form_id = 0x0100_0000 | 0x002A_3D62_u32;
        let display_fk = FormKey {
            local: 0x002A_3D62,
            plugin: own_plugin,
        };

        let target_schema = {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.root_items = vec![
                parsed_group(
                    *b"CELL",
                    0,
                    vec![parsed_group(
                        0u32.to_le_bytes(),
                        2,
                        vec![parsed_group(
                            0u32.to_le_bytes(),
                            3,
                            vec![
                                parsed_record("CELL", cell_form_id, Vec::new()),
                                parsed_group(
                                    cell_form_id.to_le_bytes(),
                                    6,
                                    vec![parsed_group(
                                        cell_form_id.to_le_bytes(),
                                        9,
                                        vec![parsed_record(
                                            "REFR",
                                            display_form_id,
                                            vec![
                                                parsed_subrecord(
                                                    "NAME",
                                                    (0x0100_0000 | WHITESPRING_X01_DISPLAY_BASE)
                                                        .to_le_bytes()
                                                        .to_vec(),
                                                ),
                                                parsed_subrecord(
                                                    "XLYR",
                                                    (0x0100_0000 | WHITESPRING_MILITARY_WING_LAYER)
                                                        .to_le_bytes()
                                                        .to_vec(),
                                                ),
                                                parsed_subrecord("DATA", vec![0u8; 24]),
                                            ],
                                        )],
                                    )],
                                ),
                            ],
                        )],
                    )],
                ),
                parsed_group(
                    *b"MSWP",
                    0,
                    vec![parsed_record(
                        "MSWP",
                        0x0100_0000 | X01_ENCLAVE_MATERIAL_SWAP,
                        vec![parsed_subrecord("EDID", b"mat_PA_X01_Enclave\0".to_vec())],
                    )],
                ),
            ];
            slot.invalidate_sections();
            drop(store);
            let session = crate::session::open_session(handle, None).unwrap();
            session.schema().unwrap()
        };

        assert_eq!(
            group_type_path(
                &plugin_handle_store_ref()
                    .lock()
                    .unwrap()
                    .get(&handle)
                    .unwrap()
                    .parsed
                    .root_items,
                display_form_id
            ),
            Some(vec![0, 2, 3, 6, 9]),
            "fixture must start with the display ref nested under its cell"
        );

        let mut mapper_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: plugin_name.into(),
                target_master_names: vec!["Fallout4.esm".into()],
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
        let config = FixupConfig {
            target_schema: Some(target_schema.clone()),
            ..Default::default()
        };
        let mut session = crate::session::open_session(handle, None).unwrap();
        let report =
            promote_placed_custom_material_swaps(&mut session, &mut mapper, &config).unwrap();
        assert_eq!(report.records_changed, 1);

        let display = session
            .record_decoded(&display_fk, target_schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(
            form_key_field(&display, b"XMSP"),
            Some(FormKey {
                local: X01_ENCLAVE_MATERIAL_SWAP,
                plugin: own_plugin,
            })
        );
        drop(session);

        assert_eq!(
            group_type_path(
                &plugin_handle_store_ref()
                    .lock()
                    .unwrap()
                    .get(&handle)
                    .unwrap()
                    .parsed
                    .root_items,
                display_form_id
            ),
            Some(vec![0, 2, 3, 6, 9]),
            "repairing XMSP must not strand the placed ref in a top-level REFR group"
        );

        assert!(plugin_handle_close_native(handle));
    }
}
