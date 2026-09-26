use crate::fixups::curve_table::{CurvePointsCache, cached_curve_at_level, source_key_for_target};
use crate::fixups::synthesize_weap_data_blocks::{
    resolve_source_fk_to_target_raw, source_damage_type_rows,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use rustc_hash::FxHashMap;

const FO4_DAMAGE_TYPE_ROW_LEN: usize = 8;
/// FO4 has no item levels; converted armor gets FO76's level-50 (max drop level) resistances.
const ARMOR_CURVE_LEVEL: u16 = 50;

/// FO76 armor authors every resistance as 0 plus a curve table in the third DAMA
/// column; FO4 rows have no curve column, so relayout leaves armor with no
/// energy/rad/poison/fire/cold resistance. The level-50 curve value is written into
/// the matching FO4 row.
///
/// Note: FO76 also curves `dtPhysical` here, and it is flattened like the rest (user
/// decision 2026-09-24, for FO76 parity). FNAM's armor rating is converted separately,
/// so if FO4 sums FNAM with a DAMA `dtPhysical` row the physical resistance shown and
/// applied in game is roughly doubled; check the item card / actor value in game.
pub struct FlattenArmoDamageCurvesFixup;

impl Fixup for FlattenArmoDamageCurvesFixup {
    fn name(&self) -> &'static str {
        "flatten_armo_damage_curves"
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
        let armo_sig = SigCode::from_str("ARMO")
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
        let mut curve_cache = CurvePointsCache::default();
        let mut changed_records = Vec::new();
        let mut report = FixupReport::empty();

        let fks = session
            .form_keys_of_sig(armo_sig, mapper.interner)
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
            let Some(source_dama) = source_record
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "DAMA")
            else {
                continue;
            };
            let curve_rows: Vec<(u32, FormKey)> = source_damage_type_rows(
                &source_dama.value,
                &source_masters,
                &source_plugin_name,
                mapper.interner,
            )
            .into_iter()
            .filter(|row| row.amount == 0)
            .filter_map(|row| {
                let curve = row.curve?;
                let target_type = resolve_source_fk_to_target_raw(
                    row.source_type,
                    source_plugin_sym,
                    mapper,
                    &target_masters,
                )?;
                Some((target_type, curve))
            })
            .collect();
            if curve_rows.is_empty() {
                continue;
            }
            let eid = source_record
                .eid
                .and_then(|sym| mapper.interner.resolve(sym))
                .unwrap_or("unknown")
                .to_owned();
            let mut target_record =
                match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(record) => record,
                    Err(error) => {
                        report.warnings.push(
                            mapper
                                .interner
                                .intern(&format!("flatten_armo_curve:{eid}:target:{error}")),
                        );
                        continue;
                    }
                };
            let mut changed = false;
            for (target_type, curve_fk) in curve_rows {
                let resistance = match cached_curve_at_level(
                    curve_fk,
                    ARMOR_CURVE_LEVEL,
                    session,
                    source_schema,
                    source_extracted_dir,
                    mapper.interner,
                    &mut curve_cache,
                ) {
                    Ok(resistance) => resistance,
                    Err(error) => {
                        report.warnings.push(
                            mapper
                                .interner
                                .intern(&format!("flatten_armo_curve:{eid}:{error}")),
                        );
                        continue;
                    }
                };
                changed |= write_unset_resistance(&mut target_record, target_type, resistance);
            }
            if changed {
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
                "flatten_armo_damage_curves replaced {replaced} of {expected} expected records"
            )));
        }
        Ok(report)
    }
}

fn write_unset_resistance(record: &mut Record, damage_type: u32, value: u32) -> bool {
    let Some(FieldValue::Bytes(bytes)) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "DAMA")
        .map(|entry| &mut entry.value)
    else {
        return false;
    };
    if bytes.len() % FO4_DAMAGE_TYPE_ROW_LEN != 0 {
        return false;
    }
    let Some(row) = bytes
        .chunks_exact_mut(FO4_DAMAGE_TYPE_ROW_LEN)
        .find(|row| row[0..4] == damage_type.to_le_bytes())
    else {
        return false;
    };
    if row[4..8] != [0; 4] || value == 0 {
        return false;
    }
    row[4..8].copy_from_slice(&value.to_le_bytes());
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

    const RESIST_CURVE: &str =
        r#"{"curve":[{"x":1,"y":10},{"x":25,"y":40},{"x":50,"y":70},{"x":100,"y":500}]}"#;
    const RESIST_CURVE_RAW: u32 = 0x0084_6BFC;
    const MISSING_CURVE_RAW: u32 = 0x0084_6C02;
    const ENERGY: u32 = 0x0006_0A81;
    const FIRE: u32 = 0x0006_0A85;
    const RADS: u32 = 0x0006_0A87;
    const COLD: u32 = 0x0006_0A84;

    fn armo(interner: &StringInterner, form_key: FormKey, dama: Vec<u8>) -> Record {
        Record {
            sig: SigCode::from_str("ARMO").unwrap(),
            form_key,
            eid: Some(interner.intern("Armor_PowerArmor_T65_Torso")),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(interner.intern("Armor_PowerArmor_T65_Torso")),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("DAMA").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(dama)),
                },
            ],
            warnings: SmallVec::new(),
        }
    }

    fn curv(interner: &StringInterner, form_key: FormKey, path: &str) -> Record {
        Record {
            sig: SigCode::from_str("CURV").unwrap(),
            form_key,
            eid: Some(interner.intern(&format!("CT_{:06X}", form_key.local))),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("JASF").unwrap(),
                value: FieldValue::String(interner.intern(path)),
            }],
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn armor_resistances_come_from_the_level_50_curve_value_when_authored_as_zero() {
        let source_handle = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();

        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let key = |local| FormKey { local, plugin };
        let torso = key(0x5485BF);

        let source_rows: [(u32, u32, u32); 4] = [
            (ENERGY, 0, RESIST_CURVE_RAW),
            (FIRE, 25, RESIST_CURVE_RAW),
            (RADS, 0, MISSING_CURVE_RAW),
            (COLD, 0, 0),
        ];
        let source_schema = {
            let mut session = crate::session::open_session(source_handle, None).unwrap();
            let schema = session.schema().unwrap();
            let dama = source_rows
                .iter()
                .flat_map(|(ty, value, curve)| {
                    [ty.to_le_bytes(), value.to_le_bytes(), curve.to_le_bytes()].concat()
                })
                .collect();
            for record in [
                armo(&interner, torso, dama),
                curv(&interner, key(RESIST_CURVE_RAW), r"Armor\Resist.json"),
                curv(&interner, key(MISSING_CURVE_RAW), r"Armor\Missing.json"),
            ] {
                session
                    .add_record(record, schema.as_ref(), &interner)
                    .unwrap();
            }
            schema
        };

        let target_schema = {
            let mut session = crate::session::open_session(target_handle, None).unwrap();
            let schema = session.schema().unwrap();
            let dama = source_rows
                .iter()
                .flat_map(|(ty, value, _)| [ty.to_le_bytes(), value.to_le_bytes()].concat())
                .collect();
            session
                .add_record(armo(&interner, torso, dama), schema.as_ref(), &interner)
                .unwrap();
            schema
        };

        let temp_root = std::env::temp_dir().join(format!(
            "bacup_armo_curve_{}_{}",
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
            .join("armor")
            .join("resist.json");
        std::fs::create_dir_all(curve_path.parent().unwrap()).unwrap();
        std::fs::write(&curve_path, RESIST_CURVE).unwrap();

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
        let run = |mapper: &mut FormKeyMapper| {
            let mut session =
                crate::session::open_session(target_handle, Some(source_handle)).unwrap();
            FlattenArmoDamageCurvesFixup
                .run_with_session(&mut session, mapper, &config)
                .unwrap()
        };
        let report = run(&mut mapper);
        let rerun = run(&mut mapper);

        let mut session = crate::session::open_session(target_handle, None).unwrap();
        let target = session
            .record_decoded(&torso, target_schema.as_ref(), &interner)
            .unwrap();
        let Some(FieldValue::Bytes(dama)) = target
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DAMA")
            .map(|f| &f.value)
        else {
            panic!("expected raw ARMO.DAMA, got {:?}", target.fields);
        };
        let rows: Vec<(u32, u32)> = dama
            .chunks_exact(FO4_DAMAGE_TYPE_ROW_LEN)
            .map(|row| {
                (
                    u32::from_le_bytes(row[0..4].try_into().unwrap()),
                    u32::from_le_bytes(row[4..8].try_into().unwrap()),
                )
            })
            .collect();
        let warnings: Vec<String> = report
            .warnings
            .iter()
            .filter_map(|sym| interner.resolve(*sym).map(str::to_owned))
            .collect();

        drop(session);
        let _ = std::fs::remove_dir_all(temp_root);
        assert!(plugin_handle_close_native(source_handle));
        assert!(plugin_handle_close_native(target_handle));

        assert_eq!(rows, vec![(ENERGY, 70), (FIRE, 25), (RADS, 0), (COLD, 0)]);
        assert_eq!(report.records_changed, 1);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with("flatten_armo_curve:Armor_PowerArmor_T65_Torso:")
                && warnings[0].contains("missing.json"),
            "{warnings:?}"
        );
        assert_eq!(rerun.records_changed, 0);
    }
}
