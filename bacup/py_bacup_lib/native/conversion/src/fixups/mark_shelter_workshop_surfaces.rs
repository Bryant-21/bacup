use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const WORKSHOP_STACKABLE_ITEM_LOCAL_ID: u32 = 0x00033D;

pub struct MarkShelterWorkshopSurfacesFixup;

impl Fixup for MarkShelterWorkshopSurfacesFixup {
    fn name(&self) -> &'static str {
        "mark_shelter_workshop_surfaces"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        source_game == Some("fo76") && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let mut form_keys = Vec::new();
        for signature in ["STAT", "ACTI"] {
            let sig = SigCode::from_str(signature)
                .map_err(|error| FixupError::SchemaError(error.to_string()))?;
            form_keys.extend(
                session
                    .form_keys_of_sig(sig, mapper.interner)
                    .map_err(|error| FixupError::HandleError(error.to_string()))?,
            );
        }
        let mut changed_records = Vec::new();
        let mut decode_errors = 0u32;
        let mut unsupported_properties = 0u32;

        for form_key in form_keys {
            let mut record =
                match session.record_decoded(&form_key, target_schema.as_ref(), mapper.interner) {
                    Ok(record) => record,
                    Err(_) => {
                        decode_errors = decode_errors.saturating_add(1);
                        continue;
                    }
                };
            let editor_id = record.eid.and_then(|eid| mapper.interner.resolve(eid));
            let model_path = record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"MODL")
                .and_then(|entry| match &entry.value {
                    FieldValue::String(model_path) => mapper.interner.resolve(*model_path),
                    _ => None,
                });
            if !is_shelter_workshop_surface(editor_id, model_path) {
                continue;
            }

            match ensure_workshop_stackable_property(&mut record, mapper.interner) {
                PropertyResult::Added => changed_records.push(record),
                PropertyResult::Present => {}
                PropertyResult::Unsupported => {
                    unsupported_properties = unsupported_properties.saturating_add(1);
                }
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "mark_shelter_workshop_surfaces replaced {replaced} of {expected} expected records"
            )));
        }

        let mut report = FixupReport::empty();
        report.records_changed = replaced as u32;
        if decode_errors > 0 || unsupported_properties > 0 {
            report.warnings.push(mapper.interner.intern(&format!(
                "mark_shelter_workshop_surfaces:decode_errors={decode_errors}:unsupported_properties={unsupported_properties}"
            )));
        }
        Ok(report)
    }
}

fn is_shelter_workshop_surface(editor_id: Option<&str>, model_path: Option<&str>) -> bool {
    if model_path.is_some_and(|path| is_shelter_model_path(path) || is_prefab_model_path(path)) {
        return true;
    }

    let Some(editor_id) = editor_id else {
        return false;
    };
    let editor_id = editor_id.to_ascii_lowercase();
    if !editor_id.starts_with("shelters_") {
        return false;
    }

    editor_id.contains("_mainfloor")
        || (editor_id.contains("_mainterrain") && !editor_id.contains("decal"))
        || editor_id.starts_with("shelters_missilesilo_concretefloor_")
        || (editor_id.starts_with("shelters_soundstage_concretefloor_")
            && !editor_id.contains("_snap_"))
        || editor_id == "shelters_nucleartestbunker_floor"
        || editor_id.starts_with("shelters_vault_basic_floor_")
        || editor_id.starts_with("shelters_vault_basic_floor02_")
        || editor_id.starts_with("shelters_vault_basic_flooroverhang")
        || editor_id.starts_with("shelters_vault_basic_floorpanel")
        || editor_id.starts_with("shelters_vault_basic_floorglasspanel")
}

fn is_shelter_model_path(model_path: &str) -> bool {
    let model_path = model_path.replace('\\', "/").to_ascii_lowercase();
    let model_path = model_path.strip_prefix("meshes/").unwrap_or(&model_path);
    model_path.starts_with("setdressing/shelters/")
}

fn is_prefab_model_path(model_path: &str) -> bool {
    model_path
        .replace('\\', "/")
        .to_ascii_lowercase()
        .split('/')
        .any(|component| component == "prefabs")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyResult {
    Added,
    Present,
    Unsupported,
}

fn ensure_workshop_stackable_property(
    record: &mut Record,
    interner: &StringInterner,
) -> PropertyResult {
    let fallout4 = interner.intern("Fallout4.esm");
    let actor_value_name = interner.intern("properties_actor_value");
    let property_value_name = interner.intern("properties_value");
    let workshop_stackable = FormKey {
        local: WORKSHOP_STACKABLE_ITEM_LOCAL_ID,
        plugin: fallout4,
    };
    let new_row = || {
        FieldValue::Struct(vec![
            (actor_value_name, FieldValue::FormKey(workshop_stackable)),
            (property_value_name, FieldValue::Float(1.0)),
        ])
    };

    let Some(properties) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"PRPS")
    else {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"PRPS"),
            value: FieldValue::List(vec![new_row()]),
        });
        return PropertyResult::Added;
    };
    let FieldValue::List(rows) = &mut properties.value else {
        return PropertyResult::Unsupported;
    };
    if rows.iter().any(|row| {
        let FieldValue::Struct(fields) = row else {
            return false;
        };
        fields.iter().any(|(name, value)| {
            *name == actor_value_name
                && matches!(
                    value,
                    FieldValue::FormKey(form_key)
                        if form_key.local == WORKSHOP_STACKABLE_ITEM_LOCAL_ID
                            && interner
                                .resolve(form_key.plugin)
                                .is_some_and(|plugin| plugin.eq_ignore_ascii_case("Fallout4.esm"))
                )
        })
    }) {
        return PropertyResult::Present;
    }

    rows.push(new_row());
    PropertyResult::Added
}

#[cfg(test)]
mod tests {
    use smallvec::smallvec;

    use super::*;
    use crate::record::RecordFlags;

    fn record(interner: &StringInterner, editor_id: &str, fields: Vec<FieldEntry>) -> Record {
        Record {
            sig: SigCode(*b"STAT"),
            form_key: FormKey {
                local: 0x853582,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: Some(interner.intern(editor_id)),
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec![],
        }
    }

    fn property_row(interner: &StringInterner, local: u32, value: f32) -> FieldValue {
        FieldValue::Struct(vec![
            (
                interner.intern("properties_actor_value"),
                FieldValue::FormKey(FormKey {
                    local,
                    plugin: interner.intern("Fallout4.esm"),
                }),
            ),
            (
                interner.intern("properties_value"),
                FieldValue::Float(value),
            ),
        ])
    }

    fn property_rows(record: &Record) -> &Vec<FieldValue> {
        let properties = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"PRPS")
            .expect("PRPS should exist");
        let FieldValue::List(rows) = &properties.value else {
            panic!("PRPS should be structured rows");
        };
        rows
    }

    #[test]
    fn selects_shelter_models_by_path() {
        for model_path in [
            "SetDressing\\shelters\\Shelters_RestrictedArea\\Shelters_RestrictedArea_Hallway1.nif",
            "SetDressing\\shelters\\Shelters_RestrictedArea\\Shelters_RestrictedArea_SideTunnel.nif",
            "Meshes/SetDressing/Shelters/Shelters_RootCellar/Shelters_RootCellar_MainFloor01.nif",
        ] {
            assert!(
                is_shelter_workshop_surface(Some("UnrelatedEditorId"), Some(model_path)),
                "{model_path} should be selected"
            );
        }

        for model_path in [
            "SetDressing\\GunthersWildWestShowProps\\GWWS_EntranceSign.nif",
            "Architecture\\Vault\\VaultWall01.nif",
            "SetDressing\\NotShelters\\Floor.nif",
        ] {
            assert!(
                !is_shelter_workshop_surface(Some("UnrelatedEditorId"), Some(model_path)),
                "{model_path} should not be selected"
            );
        }
    }

    #[test]
    fn selects_prefab_models_by_path() {
        for model_path in [
            "atx\\architecture\\prefabs\\atx_haunted_belltower\\ATX_Haunted_Belltower_BellwithButton.nif",
            "Meshes/Architecture/Workshop/Prefabs/Barn/BarnComplete01.nif",
        ] {
            assert!(
                is_shelter_workshop_surface(Some("UnrelatedEditorId"), Some(model_path)),
                "{model_path} should be selected"
            );
        }

        for model_path in [
            "Architecture\\Workshop\\PrefabParts\\BarnWall01.nif",
            "SetDressing\\PrefabsDisplay\\Sign01.nif",
        ] {
            assert!(
                !is_shelter_workshop_surface(Some("UnrelatedEditorId"), Some(model_path)),
                "{model_path} should not be selected"
            );
        }
    }

    #[test]
    fn retains_walkable_shelter_editor_id_fallback() {
        for editor_id in [
            "Shelters_WranglerCasino_MainFloor01",
            "Shelters_RestrictedArea_MainFloor",
            "Shelters_RootCellar_MainFloor01",
            "Shelters_Flatlands_MainTerrain01",
            "Shelters_HousingDevelopment_MainTerrain_Copy01",
            "Shelters_SummerCamp_MainTerrain",
            "Shelters_MissileSilo_ConcreteFloor_MaintenanceTunnel",
            "Shelters_SoundStage_ConcreteFloor_768",
            "Shelters_NuclearTestBunker_Floor",
            "Shelters_Vault_Basic_Floor_256W_256L",
            "Shelters_Vault_Basic_Floor02_128W_064L",
            "Shelters_Vault_Basic_FloorGlassPanel01_256W_128L",
        ] {
            assert!(
                is_shelter_workshop_surface(Some(editor_id), None),
                "{editor_id} should be selected"
            );
        }

        for editor_id in [
            "Shelters_Flatlands_FakeDistanceTerrain01",
            "Shelters_Flatlands_MainTerrain01Decal",
            "Shelters_HousingDevelopment_BillboardTree01",
            "Shelters_RestrictedArea_Door_Closed",
            "Shelters_SoundStage_ConcreteFloor_Snap_256",
            "Shelters_MissileSilo_DecalSignage_001",
            "Shelters_Vault_Basic_Wall_256W_256H",
            "WorkshopConcreteFloor",
        ] {
            assert!(
                !is_shelter_workshop_surface(Some(editor_id), None),
                "{editor_id} should not be selected"
            );
        }

        assert!(!is_shelter_workshop_surface(None, None));
    }

    #[test]
    fn adds_vanilla_workshop_stackable_property() {
        let interner = StringInterner::new();
        let mut record = record(&interner, "Shelters_WranglerCasino_MainFloor01", vec![]);

        assert_eq!(
            ensure_workshop_stackable_property(&mut record, &interner),
            PropertyResult::Added
        );
        assert_eq!(
            property_rows(&record),
            &vec![property_row(
                &interner,
                WORKSHOP_STACKABLE_ITEM_LOCAL_ID,
                1.0
            )]
        );
    }

    #[test]
    fn preserves_existing_properties_and_is_idempotent() {
        let interner = StringInterner::new();
        let existing = property_row(&interner, 0x123456, 0.25);
        let mut record = record(
            &interner,
            "Shelters_RestrictedArea_MainFloor",
            vec![FieldEntry {
                sig: SubrecordSig(*b"PRPS"),
                value: FieldValue::List(vec![existing.clone()]),
            }],
        );

        assert_eq!(
            ensure_workshop_stackable_property(&mut record, &interner),
            PropertyResult::Added
        );
        assert_eq!(property_rows(&record)[0], existing);
        assert_eq!(property_rows(&record).len(), 2);
        assert_eq!(
            ensure_workshop_stackable_property(&mut record, &interner),
            PropertyResult::Present
        );
        assert_eq!(property_rows(&record).len(), 2);
    }

    #[test]
    fn leaves_opaque_existing_properties_unchanged() {
        let interner = StringInterner::new();
        let raw = smallvec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut record = record(
            &interner,
            "Shelters_RestrictedArea_MainFloor",
            vec![FieldEntry {
                sig: SubrecordSig(*b"PRPS"),
                value: FieldValue::Bytes(raw.clone()),
            }],
        );

        assert_eq!(
            ensure_workshop_stackable_property(&mut record, &interner),
            PropertyResult::Unsupported
        );
        assert_eq!(record.fields[0].value, FieldValue::Bytes(raw));
    }
}
