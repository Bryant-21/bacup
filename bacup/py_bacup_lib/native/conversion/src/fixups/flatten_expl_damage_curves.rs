use crate::fixups::curve_table::{CurveMeanCache, cached_curve_mean, source_key_for_target};
use crate::fixups::flatten_npc_property_curves::source_raw_to_form_key;
use crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use rustc_hash::FxHashMap;

/// FO76 takes an explosion's damage from `DATA.damage_curve_table` whenever one is
/// set, and the authored `damage` beside it is a leftover: `ExplosionFatMan` carries
/// 1.0. FO4 has no curve slot, so relayout drops it and the explosion lands for 0
/// or 1 while its projectile still registers hits. The curve mean is written into
/// `damage`, the same flattening weapon damage gets.
pub struct FlattenExplDamageCurvesFixup;

impl Fixup for FlattenExplDamageCurvesFixup {
    fn name(&self) -> &'static str {
        "flatten_expl_damage_curves"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.source_schema.is_some() && config.source_extracted_dir.is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let expl_sig = SigCode::from_str("EXPL")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let source_schema = config
            .source_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing source schema in fixup config".into()))?;
        let source_extracted_dir = config
            .source_extracted_dir
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing source extracted dir".into()))?;
        let (Some(curve_offset), Some(damage_offset)) = (
            expl_data_offset(source_schema, "damage_curve_table", None),
            expl_data_offset(target_schema, "damage", Some(FO4_TARGET_FORM_VERSION)),
        ) else {
            return Ok(FixupReport::empty());
        };
        let Some(source_slot) = session.source_slot_opt() else {
            return Ok(FixupReport::empty());
        };
        let source_masters = source_slot.parsed.header.masters.clone();
        let source_plugin_name = source_slot.parsed.plugin_name.clone();
        let source_plugin_sym = mapper.interner.intern(&source_plugin_name);
        let target_plugin_sym = mapper
            .interner
            .intern(&session.target_slot().parsed.plugin_name);
        let target_to_source: FxHashMap<FormKey, FormKey> = mapper
            .source_to_target_iter()
            .map(|(source, target)| (target, source))
            .collect();
        let mut curve_cache = CurveMeanCache::default();
        let mut changed_records = Vec::new();
        let mut report = FixupReport::empty();

        let fks = session
            .form_keys_of_sig(expl_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        for fk in fks {
            let Some(source_fk) =
                source_key_for_target(fk, &target_to_source, target_plugin_sym, source_plugin_sym)
            else {
                continue;
            };
            let Ok(source_record) =
                session.source_record_decoded(&source_fk, source_schema, mapper.interner)
            else {
                continue;
            };
            let Some(curve_fk) = data_u32(&source_record, curve_offset).and_then(|raw| {
                source_raw_to_form_key(raw, &source_masters, &source_plugin_name, mapper.interner)
            }) else {
                continue;
            };
            let eid = source_record
                .eid
                .and_then(|sym| mapper.interner.resolve(sym))
                .unwrap_or("unknown");
            let damage = match cached_curve_mean(
                curve_fk,
                session,
                source_schema,
                source_extracted_dir,
                mapper.interner,
                &mut curve_cache,
            ) {
                Ok(damage) => damage as f32,
                Err(error) => {
                    report.warnings.push(
                        mapper
                            .interner
                            .intern(&format!("flatten_expl_curve:{eid}:{error}")),
                    );
                    continue;
                }
            };
            let mut target_record =
                match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(record) => record,
                    Err(error) => {
                        report.warnings.push(
                            mapper
                                .interner
                                .intern(&format!("flatten_expl_curve:{eid}:target:{error}")),
                        );
                        continue;
                    }
                };
            if write_data_f32(&mut target_record, damage_offset, damage) {
                changed_records.push(target_record);
                report.records_changed += 1;
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "flatten_expl_damage_curves replaced {replaced} of {expected} expected records"
            )));
        }
        Ok(report)
    }
}

fn expl_data_offset(
    schema: &AuthoringSchema,
    field_id: &str,
    form_version: Option<u16>,
) -> Option<usize> {
    schema
        .struct_field_layout_versioned("EXPL", "DATA", form_version)
        .into_iter()
        .find(|field| field.field_id == field_id && field.width == 4)
        .map(|field| field.offset)
}

fn data_u32(record: &Record, offset: usize) -> Option<u32> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "DATA")
        .and_then(|entry| match &entry.value {
            FieldValue::Bytes(bytes) => bytes.get(offset..offset + 4),
            _ => None,
        })
        .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
}

fn write_data_f32(record: &mut Record, offset: usize, value: f32) -> bool {
    let Some(word) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "DATA")
        .and_then(|entry| match &mut entry.value {
            FieldValue::Bytes(bytes) => bytes.get_mut(offset..offset + 4),
            _ => None,
        })
    else {
        return false;
    };
    let value = value.to_le_bytes();
    if *word == value {
        return false;
    }
    word.copy_from_slice(&value);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };
    use smallvec::SmallVec;
    use std::time::{SystemTime, UNIX_EPOCH};

    const DAMAGE_UNIVERSAL_TIER57: &str = r#"{"curve":[{"x":1,"y":177},{"x":5,"y":198},{"x":10,"y":223},{"x":15,"y":252},{"x":20,"y":284},{"x":25,"y":321},{"x":30,"y":362},{"x":35,"y":409},{"x":40,"y":462},{"x":45,"y":522},{"x":50,"y":589}]}"#;
    const M79_CURVE_RAW: u32 = 0x0080_F229;
    const M79_FORCE: f32 = 500.0;

    struct Flattened {
        records_changed: u32,
        force: f32,
        damage: f32,
    }

    fn f32_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn expl(interner: &StringInterner, form_key: FormKey, data: SmallVec<[u8; 32]>) -> Record {
        Record {
            sig: SigCode::from_str("EXPL").unwrap(),
            form_key,
            eid: Some(interner.intern("ExplosionM79")),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("DATA").unwrap(),
                value: FieldValue::Bytes(data),
            }],
            warnings: SmallVec::new(),
        }
    }

    fn flatten_m79(
        curve_path_sig: &str,
        curve_raw: u32,
        source_damage: f32,
        target_damage: f32,
    ) -> Flattened {
        let source_handle = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();

        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let explosion = FormKey {
            local: 0x80F275,
            plugin,
        };
        let curve = FormKey {
            local: 0x80F229,
            plugin,
        };

        let source_schema = {
            let mut session = crate::session::open_session(source_handle, None).unwrap();
            let schema = session.schema().unwrap();
            let curve_offset = expl_data_offset(&schema, "damage_curve_table", None).unwrap();
            let force_offset = expl_data_offset(&schema, "force", None).unwrap();
            let damage_offset = expl_data_offset(&schema, "damage", None).unwrap();
            assert_eq!((curve_offset, force_offset, damage_offset), (24, 28, 32));
            let mut data = SmallVec::from_slice(&[0u8; 96]);
            data[curve_offset..curve_offset + 4].copy_from_slice(&curve_raw.to_le_bytes());
            data[force_offset..force_offset + 4].copy_from_slice(&M79_FORCE.to_le_bytes());
            data[damage_offset..damage_offset + 4].copy_from_slice(&source_damage.to_le_bytes());
            session
                .add_record(expl(&interner, explosion, data), schema.as_ref(), &interner)
                .unwrap();
            session
                .add_record(
                    Record {
                        sig: SigCode::from_str("CURV").unwrap(),
                        form_key: curve,
                        eid: Some(interner.intern("CT_Player_Damage_Universal_Tier57")),
                        flags: RecordFlags::empty(),
                        fields: smallvec::smallvec![FieldEntry {
                            sig: SubrecordSig::from_str(curve_path_sig).unwrap(),
                            value: FieldValue::String(
                                interner.intern("Player\\Damage\\Damage_Universal_Tier57.json")
                            ),
                        }],
                        warnings: SmallVec::new(),
                    },
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
            schema
        };

        let (target_schema, target_force_offset, target_damage_offset) = {
            let mut session = crate::session::open_session(target_handle, None).unwrap();
            let schema = session.schema().unwrap();
            let target_force_offset =
                expl_data_offset(&schema, "force", Some(FO4_TARGET_FORM_VERSION)).unwrap();
            let target_damage_offset =
                expl_data_offset(&schema, "damage", Some(FO4_TARGET_FORM_VERSION)).unwrap();
            assert_eq!((target_force_offset, target_damage_offset), (24, 28));
            let mut data = SmallVec::from_slice(&[0u8; 84]);
            data[target_force_offset..target_force_offset + 4]
                .copy_from_slice(&M79_FORCE.to_le_bytes());
            data[target_damage_offset..target_damage_offset + 4]
                .copy_from_slice(&target_damage.to_le_bytes());
            session
                .add_record(expl(&interner, explosion, data), schema.as_ref(), &interner)
                .unwrap();
            (schema, target_force_offset, target_damage_offset)
        };

        let temp_root = std::env::temp_dir().join(format!(
            "bacup_expl_curve_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let curve_path = temp_root
            .join("misc")
            .join("curvetables")
            .join("json")
            .join("player")
            .join("damage")
            .join("damage_universal_tier57.json");
        std::fs::create_dir_all(curve_path.parent().unwrap()).unwrap();
        std::fs::write(&curve_path, DAMAGE_UNIVERSAL_TIER57).unwrap();

        let mut mapper_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
        let config = FixupConfig {
            is_whole_plugin: true,
            source_extracted_dir: Some(temp_root.clone()),
            target_schema: Some(target_schema.clone()),
            source_schema: Some(source_schema.clone()),
            ..Default::default()
        };
        let report = {
            let mut session =
                crate::session::open_session(target_handle, Some(source_handle)).unwrap();
            FlattenExplDamageCurvesFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap()
        };

        let mut session = crate::session::open_session(target_handle, None).unwrap();
        let target = session
            .record_decoded(&explosion, target_schema.as_ref(), &interner)
            .unwrap();
        let FieldValue::Bytes(data) = &target.fields[0].value else {
            panic!("expected raw EXPL.DATA");
        };
        let flattened = Flattened {
            records_changed: report.records_changed,
            force: f32_at(data, target_force_offset),
            damage: f32_at(data, target_damage_offset),
        };

        drop(session);
        let _ = std::fs::remove_dir_all(temp_root);
        assert!(plugin_handle_close_native(source_handle));
        assert!(plugin_handle_close_native(target_handle));
        flattened
    }

    #[test]
    fn explosion_damage_comes_from_the_curve_mean_unless_already_flattened_or_curveless() {
        for (name, curve_tag, curve_raw, source_damage, target_damage, changed, damage) in [
            ("unset damage", "JASF", M79_CURVE_RAW, 0.0, 0.0, 1, 345.0),
            (
                "authored placeholder",
                "JASF",
                M79_CURVE_RAW,
                1.0,
                1.0,
                1,
                345.0,
            ),
            ("older CRVE tag", "CRVE", M79_CURVE_RAW, 0.0, 0.0, 1, 345.0),
            ("no curve", "JASF", 0, 150.0, 150.0, 0, 150.0),
            (
                "already flattened",
                "JASF",
                M79_CURVE_RAW,
                0.0,
                345.0,
                0,
                345.0,
            ),
        ] {
            let flattened = flatten_m79(curve_tag, curve_raw, source_damage, target_damage);
            assert_eq!(flattened.records_changed, changed, "{name}");
            assert_eq!(flattened.damage, damage, "{name}");
            assert_eq!(flattened.force, M79_FORCE, "{name}");
        }
    }
}
