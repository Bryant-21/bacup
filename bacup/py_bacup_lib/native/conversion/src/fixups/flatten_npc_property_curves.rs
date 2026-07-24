use crate::fixups::curve_table::{CurveMeanCache, cached_curve_mean, source_key_for_target};
use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};
use rustc_hash::FxHashMap;

const FO76_PROPERTY_ROW_LEN: usize = 12;
const FO4_PROPERTY_ROW_LEN: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ResolvedProperty {
    target_actor_value_raw: u32,
    value: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SourceProperty {
    actor_value: FormKey,
    curve: Option<FormKey>,
}

pub struct FlattenNpcPropertyCurvesFixup;

impl Fixup for FlattenNpcPropertyCurvesFixup {
    fn name(&self) -> &'static str {
        "flatten_npc_property_curves"
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
        let npc_sig = SigCode::from_str("NPC_")
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
        let Some(source_slot) = session.source_slot_opt() else {
            return Ok(FixupReport::empty());
        };
        let source_masters = source_slot.parsed.header.masters.clone();
        let source_plugin_name = source_slot.parsed.plugin_name.clone();
        let source_plugin_sym = mapper.interner.intern(&source_plugin_name);
        let target_plugin_sym = mapper
            .interner
            .intern(&session.target_slot().parsed.plugin_name);
        let target_masters = session.target_masters().to_vec();
        let target_to_source: FxHashMap<FormKey, FormKey> = mapper
            .source_to_target_iter()
            .map(|(source, target)| (target, source))
            .collect();
        let mut curve_cache = CurveMeanCache::default();
        let mut changed_records = Vec::new();
        let mut report = FixupReport::empty();

        let fks = session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        for fk in fks {
            let Some(source_fk) =
                source_key_for_target(fk, &target_to_source, target_plugin_sym, source_plugin_sym)
            else {
                continue;
            };
            let source_record =
                match session.source_record_decoded(&source_fk, source_schema, mapper.interner) {
                    Ok(record) => record,
                    Err(_) => continue,
                };
            let (resolved, warnings) = resolve_source_properties(
                &source_record,
                &source_masters,
                &source_plugin_name,
                source_plugin_sym,
                mapper,
                &target_masters,
                session,
                source_schema,
                source_extracted_dir,
                &mut curve_cache,
            );
            let eid = source_record
                .eid
                .and_then(|sym| mapper.interner.resolve(sym))
                .unwrap_or("unknown");
            for warning in warnings {
                report.warnings.push(
                    mapper
                        .interner
                        .intern(&format!("flatten_npc_curve:{eid}:{warning}")),
                );
            }
            if resolved.is_empty() {
                continue;
            }
            let mut target_record =
                match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(record) => record,
                    Err(error) => {
                        report.warnings.push(
                            mapper
                                .interner
                                .intern(&format!("flatten_npc_curve:{eid}:target:{error}")),
                        );
                        continue;
                    }
                };
            if apply_resolved_properties(&mut target_record, &resolved) {
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
                "flatten_npc_property_curves replaced {replaced} of {expected} expected records"
            )));
        }
        Ok(report)
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_source_properties(
    source_record: &Record,
    source_masters: &[String],
    source_plugin_name: &str,
    source_plugin_sym: Sym,
    mapper: &FormKeyMapper,
    target_masters: &[String],
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    source_extracted_dir: &std::path::Path,
    curve_cache: &mut CurveMeanCache,
) -> (Vec<ResolvedProperty>, Vec<String>) {
    let Some(prps) = source_record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "PRPS")
    else {
        return (Vec::new(), Vec::new());
    };
    let mut resolved = Vec::new();
    let mut warnings = Vec::new();
    for source in source_property_rows(
        &prps.value,
        source_masters,
        source_plugin_name,
        mapper.interner,
    ) {
        let Some(curve_fk) = source.curve else {
            continue;
        };
        let Some(target_actor_value_raw) = resolve_source_fk_to_target_raw(
            source.actor_value,
            source_plugin_sym,
            mapper,
            target_masters,
        ) else {
            continue;
        };
        match cached_curve_mean(
            curve_fk,
            session,
            source_schema,
            source_extracted_dir,
            mapper.interner,
            curve_cache,
        ) {
            Ok(value) => resolved.push(ResolvedProperty {
                target_actor_value_raw,
                value: value as f32,
            }),
            Err(error) => warnings.push(format!("{:06X}:{error}", source.actor_value.local)),
        }
    }
    (resolved, warnings)
}

fn source_property_rows(
    value: &FieldValue,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<SourceProperty> {
    match value {
        FieldValue::Bytes(bytes) => bytes
            .chunks_exact(FO76_PROPERTY_ROW_LEN)
            .filter_map(|row| {
                let actor_value = source_raw_to_form_key(
                    u32::from_le_bytes(row[0..4].try_into().ok()?),
                    source_masters,
                    source_plugin_name,
                    interner,
                )?;
                let curve = source_raw_to_form_key(
                    u32::from_le_bytes(row[8..12].try_into().ok()?),
                    source_masters,
                    source_plugin_name,
                    interner,
                );
                Some(SourceProperty { actor_value, curve })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn apply_resolved_properties(record: &mut Record, resolved: &[ResolvedProperty]) -> bool {
    let Ok(prps_sig) = SubrecordSig::from_str("PRPS") else {
        return false;
    };
    let mut changed = false;
    for entry in &mut record.fields {
        if entry.sig != prps_sig {
            continue;
        }
        changed |= apply_resolved_value(&mut entry.value, resolved);
    }
    changed
}

fn apply_resolved_value(value: &mut FieldValue, resolved: &[ResolvedProperty]) -> bool {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() % FO4_PROPERTY_ROW_LEN == 0 => {
            let mut changed = false;
            for row in bytes.chunks_exact_mut(FO4_PROPERTY_ROW_LEN) {
                let actor_value_raw = u32::from_le_bytes(row[0..4].try_into().unwrap());
                let Some(replacement) = resolved
                    .iter()
                    .find(|resolved| resolved.target_actor_value_raw == actor_value_raw)
                else {
                    continue;
                };
                let replacement_bytes = replacement.value.to_le_bytes();
                if row[4..8] != replacement_bytes {
                    row[4..8].copy_from_slice(&replacement_bytes);
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

fn source_raw_to_form_key(
    raw: u32,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let load_index = (raw >> 24) as usize;
    let plugin = source_masters
        .get(load_index)
        .map(String::as_str)
        .unwrap_or(source_plugin_name);
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
    })
}

fn resolve_source_fk_to_target_raw(
    source_fk: FormKey,
    source_plugin_sym: Sym,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> Option<u32> {
    if let Some(target_fk) = mapper.lookup(source_fk) {
        return encode_target_form_id(target_fk, mapper.interner, target_masters);
    }
    if source_fk.plugin == source_plugin_sym
        && target_masters
            .first()
            .is_some_and(|master| master.eq_ignore_ascii_case("Fallout4.esm"))
    {
        return Some(source_fk.local & 0x00FF_FFFF);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::record::{FieldEntry, RecordFlags};
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };
    use smallvec::SmallVec;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn npc_with_raw_properties(rows: &[(u32, f32)]) -> Record {
        let interner = StringInterner::new();
        let mut bytes = SmallVec::<[u8; 32]>::new();
        for (actor_value, value) in rows {
            bytes.extend_from_slice(&actor_value.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        Record {
            sig: SigCode::from_str("NPC_").unwrap(),
            form_key: FormKey {
                local: 1,
                plugin: interner.intern("Out.esp"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("PRPS").unwrap(),
                value: FieldValue::Bytes(bytes),
            }],
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn curve_values_replace_matching_actor_properties_only() {
        let mut record = npc_with_raw_properties(&[(0x000002D4, 0.0), (0x000002D5, 500.0)]);
        let changed = apply_resolved_properties(
            &mut record,
            &[ResolvedProperty {
                target_actor_value_raw: 0x000002D4,
                value: 99_890.0,
            }],
        );
        assert!(changed);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw PRPS");
        };
        assert_eq!(
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            99_890.0
        );
        assert_eq!(f32::from_le_bytes(bytes[12..16].try_into().unwrap()), 500.0);
    }

    #[test]
    fn source_rows_keep_actor_value_literal_and_curve() {
        let interner = StringInterner::new();
        let mut bytes = SmallVec::<[u8; 32]>::new();
        bytes.extend_from_slice(&0x000002D4u32.to_le_bytes());
        bytes.extend_from_slice(&0.0f32.to_le_bytes());
        bytes.extend_from_slice(&0x007ABE3Eu32.to_le_bytes());
        let rows =
            source_property_rows(&FieldValue::Bytes(bytes), &[], "SeventySix.esm", &interner);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].actor_value.local, 0x0002D4);
        assert_eq!(rows[0].curve.unwrap().local, 0x7ABE3E);
    }

    #[test]
    fn scorchbeast_queen_health_uses_points_through_level_fifty() {
        let json = r#"{"curve":[{"x":1,"y":22061},{"x":12,"y":60165},{"x":23,"y":99004},{"x":34,"y":138723},{"x":45,"y":179495},{"x":56,"y":221108}]}"#;
        assert_eq!(
            crate::fixups::curve_table::mean_curve_value(json),
            Ok(99_890)
        );
    }

    #[test]
    fn same_name_template_is_flattened_without_mapper_entry() {
        let source_handle = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();

        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let health_curve = FormKey {
            local: 0x7ABE3E,
            plugin,
        };
        let queen_template = FormKey {
            local: 0x043C75,
            plugin,
        };

        let source_schema = {
            let mut session = crate::session::open_session(source_handle, None).unwrap();
            let schema = session.schema().unwrap();
            let mut source_properties = SmallVec::<[u8; 32]>::new();
            source_properties.extend_from_slice(&0x000002D4u32.to_le_bytes());
            source_properties.extend_from_slice(&0.0f32.to_le_bytes());
            source_properties.extend_from_slice(&0x007ABE3Eu32.to_le_bytes());
            session
                .add_record(
                    Record {
                        sig: SigCode::from_str("NPC_").unwrap(),
                        form_key: queen_template,
                        eid: Some(interner.intern("EncScorchbeastQueen01Template")),
                        flags: RecordFlags::empty(),
                        fields: smallvec::smallvec![FieldEntry {
                            sig: SubrecordSig::from_str("PRPS").unwrap(),
                            value: FieldValue::Bytes(source_properties),
                        }],
                        warnings: SmallVec::new(),
                    },
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
            session
                .add_record(
                    Record {
                        sig: SigCode::from_str("CURV").unwrap(),
                        form_key: health_curve,
                        eid: Some(interner.intern("CT_Creatures_Health_Universal_Tier55")),
                        flags: RecordFlags::empty(),
                        fields: smallvec::smallvec![FieldEntry {
                            sig: SubrecordSig::from_str("JASF").unwrap(),
                            value: FieldValue::String(
                                interner.intern("Creatures\\Health\\Health_Universal_Tier55.json",)
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

        let target_schema = {
            let mut session = crate::session::open_session(target_handle, None).unwrap();
            let schema = session.schema().unwrap();
            let mut target_properties = SmallVec::<[u8; 32]>::new();
            target_properties.extend_from_slice(&0x000002D4u32.to_le_bytes());
            target_properties.extend_from_slice(&0.0f32.to_le_bytes());
            session
                .add_record(
                    Record {
                        sig: SigCode::from_str("NPC_").unwrap(),
                        form_key: queen_template,
                        eid: Some(interner.intern("EncScorchbeastQueen01Template")),
                        flags: RecordFlags::empty(),
                        fields: smallvec::smallvec![FieldEntry {
                            sig: SubrecordSig::from_str("PRPS").unwrap(),
                            value: FieldValue::Bytes(target_properties),
                        }],
                        warnings: SmallVec::new(),
                    },
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
            schema
        };

        let temp_root = std::env::temp_dir().join(format!(
            "bacup_npc_curve_{}_{}",
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
            .join("creatures")
            .join("health")
            .join("health_universal_tier55.json");
        std::fs::create_dir_all(curve_path.parent().unwrap()).unwrap();
        std::fs::write(
            &curve_path,
            r#"{"curve":[{"x":1,"y":22061},{"x":12,"y":60165},{"x":23,"y":99004},{"x":34,"y":138723},{"x":45,"y":179495},{"x":56,"y":221108}]}"#,
        )
        .unwrap();

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
            FlattenNpcPropertyCurvesFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap()
        };
        assert_eq!(report.records_changed, 1);

        let mut session = crate::session::open_session(target_handle, None).unwrap();
        let target = session
            .record_decoded(&queen_template, target_schema.as_ref(), &interner)
            .unwrap();
        let FieldValue::Bytes(properties) = &target.fields[0].value else {
            panic!("expected raw PRPS");
        };
        assert_eq!(
            f32::from_le_bytes(properties[4..8].try_into().unwrap()),
            99_890.0
        );

        drop(session);
        let _ = std::fs::remove_dir_all(temp_root);
        assert!(plugin_handle_close_native(source_handle));
        assert!(plugin_handle_close_native(target_handle));
    }
}
