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
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const CUSTOM_MATERIAL_SWAP_EDITOR_ID_PREFIX: &str = "CustomMaterialSwap";

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

/// Promote own-plugin custom MSWPs reached by placed `REFR.XMSP` records.
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

    let mut referenced_swap_ids = FxHashSet::default();
    for fk in session
        .form_keys_of_sig(refr_sig, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?
    {
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
    for fk in session
        .form_keys_of_sig(mswp_sig, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?
    {
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
    report.records_changed = promoted_count.try_into().unwrap_or(u32::MAX);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::FormKey;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

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
        assert_eq!(report.records_changed, 1);

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
        assert_eq!(
            promote_placed_custom_material_swaps(&mut session, &mut mapper, &config)
                .unwrap()
                .records_changed,
            0
        );

        drop(session);
        assert!(plugin_handle_close_native(handle));
    }
}
