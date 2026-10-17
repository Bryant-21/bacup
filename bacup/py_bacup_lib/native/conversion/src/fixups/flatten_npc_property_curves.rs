use crate::fixups::curve_table::{
    CurveMeanCache, CurvePointsCache, cached_curve_at_level, cached_curve_mean,
    source_key_for_target,
};
use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::source_read::form_key_to_read_str;
use crate::sym::{StringInterner, Sym};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

const FO76_PROPERTY_ROW_LEN: usize = 12;
const FO4_PROPERTY_ROW_LEN: usize = 8;
const ACBS_LEVEL_OFFSET: usize = 6;
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;
const ACBS_FLAG_PC_LEVEL_MULT: u32 = 0x0000_0080;
const ACBS_TEMPLATE_FLAG_STATS: u16 = 0x0002;
const CURVE_SOURCE_EDITOR_ID: &str = "B21_NpcCurveSource";
const AVIF_FLAG_DEFAULT_TO_0: u32 = 0x0000_0400;
const AVIF_TYPE_VARIABLE: u32 = 8;
/// NPC_ subrecords that follow PRPS, for placing a PRPS the record lacked.
const PRPS_SUCCESSOR_SIGS: &[&str] = &[
    "FTYP", "NTRM", "COCT", "CNTO", "AIDT", "PKID", "KSIZ", "KWDA", "APPR", "OBTE", "CNAM", "FULL",
    "SHRT", "DATA", "DNAM",
];

/// Mapper key the curve-source marker AVIF is allocated under.
///
/// Tales hooks `TESNPC::GetActorValue`, which only sees a TESNPC, often a leveled
/// template base whose PRPS was copied from its Stats template. Every converted NPC
/// therefore carries its own object id under this actor value, naming the FO76 record
/// whose curves `emit_npc_stats_catalog` exports. The id is exact in an f32 (24 bits).
pub(crate) fn curve_source_marker_key(interner: &StringInterner) -> FormKey {
    FormKey {
        local: 1,
        plugin: interner.intern(CURVE_SOURCE_EDITOR_ID),
    }
}

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
        let mut points_cache = CurvePointsCache::default();
        let mut changed_records = Vec::new();
        let mut report = FixupReport::empty();
        let converts_fo76_to_fo4 = source_slot.parsed.game.as_deref() == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4");
        let marker_raw = if converts_fo76_to_fo4 {
            let marker = ensure_curve_source_avif(session, mapper, target_schema, &mut report)?;
            encode_target_form_id(marker, mapper.interner, &target_masters)
        } else {
            None
        };

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
            let level = npc_concrete_level(&source_record, mapper.interner);
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
                level,
                &mut curve_cache,
                &mut points_cache,
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
            // Master overrides keep the master's identity; only the plugin's own NPCs name themselves.
            let marker = marker_raw
                .filter(|_| fk.plugin == target_plugin_sym && source_record.sig == npc_sig)
                .map(|raw| ResolvedProperty {
                    target_actor_value_raw: raw,
                    value: fk.local as f32,
                });
            if resolved.is_empty() && marker.is_none() {
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
            let mut changed = apply_resolved_properties(&mut target_record, &resolved);
            if let Some(marker) = marker {
                changed |= set_property(&mut target_record, marker);
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
    level: Option<u16>,
    curve_cache: &mut CurveMeanCache,
    points_cache: &mut CurvePointsCache,
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
        let value = match level {
            Some(level) => cached_curve_at_level(
                curve_fk,
                level,
                session,
                source_schema,
                source_extracted_dir,
                mapper.interner,
                points_cache,
            ),
            None => cached_curve_mean(
                curve_fk,
                session,
                source_schema,
                source_extracted_dir,
                mapper.interner,
                curve_cache,
            ),
        };
        match value {
            Ok(value) => resolved.push(ResolvedProperty {
                target_actor_value_raw,
                value: value as f32,
            }),
            Err(error) => warnings.push(format!("{:06X}:{error}", source.actor_value.local)),
        }
    }
    (resolved, warnings)
}

/// The level an actor's property curves should be read at, when it has one.
///
/// FO76 reads a creature's property curves at its spawn level. The curve mean
/// gives every tier of a leveled family the same value instead:
/// `EncScorched_Template_Tier01` (level 1) and `Tier14` (level 100) both came out
/// at 179 health against a curve that runs 36 → 842. A `PCLevelMult` level is a
/// multiplier rather than a level, and a Stats-templated actor's own level is a
/// placeholder its template overrides, so both keep the mean.
fn npc_concrete_level(record: &Record, interner: &StringInterner) -> Option<u16> {
    let acbs = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "ACBS")
        .map(|entry| &entry.value)?;
    let (level, pc_level_mult, stats_templated) = match acbs {
        FieldValue::Bytes(bytes) if bytes.len() >= ACBS_TEMPLATE_FLAGS_OFFSET + 2 => {
            let flags = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
            let template_flags = u16::from_le_bytes(
                bytes[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                    .try_into()
                    .ok()?,
            );
            (
                u16::from_le_bytes(
                    bytes[ACBS_LEVEL_OFFSET..ACBS_LEVEL_OFFSET + 2]
                        .try_into()
                        .ok()?,
                ),
                flags & ACBS_FLAG_PC_LEVEL_MULT != 0,
                template_flags & ACBS_TEMPLATE_FLAG_STATS != 0,
            )
        }
        FieldValue::Struct(members) => {
            let level = match acbs_member(members, "level", interner)? {
                FieldValue::Uint(level) => u16::try_from(*level).ok()?,
                FieldValue::Int(level) => u16::try_from(*level).ok()?,
                _ => return None,
            };
            (
                level,
                acbs_flag_set(
                    acbs_member(members, "flags", interner),
                    "pclevelmult",
                    ACBS_FLAG_PC_LEVEL_MULT.into(),
                    interner,
                ),
                acbs_flag_set(
                    acbs_member(members, "templateflags", interner),
                    "stats",
                    ACBS_TEMPLATE_FLAG_STATS.into(),
                    interner,
                ),
            )
        }
        _ => return None,
    };
    (level > 0 && !pc_level_mult && !stats_templated).then_some(level)
}

fn normalized(name: &str) -> String {
    name.replace('_', "").to_ascii_lowercase()
}

fn acbs_member<'a>(
    members: &'a [(Sym, FieldValue)],
    wanted: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    members.iter().find_map(|(name, value)| {
        interner
            .resolve(*name)
            .is_some_and(|name| normalized(name) == wanted)
            .then_some(value)
    })
}

/// Decoded flag fields arrive either as a bitfield or as a list of flag names.
fn acbs_flag_set(
    value: Option<&FieldValue>,
    name: &str,
    bit: u64,
    interner: &StringInterner,
) -> bool {
    match value {
        Some(FieldValue::Uint(bits)) => bits & bit != 0,
        Some(FieldValue::Int(bits)) => *bits >= 0 && (*bits as u64) & bit != 0,
        Some(FieldValue::List(items)) => items.iter().any(|item| {
            matches!(item, FieldValue::String(sym)
                if interner.resolve(*sym).is_some_and(|flag| normalized(flag) == name))
        }),
        _ => false,
    }
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

/// Writes `property` into the record's PRPS row for its actor value, adding the row
/// (and the PRPS itself) when missing.
fn set_property(record: &mut Record, property: ResolvedProperty) -> bool {
    let mut row = [0u8; FO4_PROPERTY_ROW_LEN];
    row[0..4].copy_from_slice(&property.target_actor_value_raw.to_le_bytes());
    row[4..8].copy_from_slice(&property.value.to_le_bytes());
    let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "PRPS")
    else {
        let at = record
            .fields
            .iter()
            .position(|entry| PRPS_SUCCESSOR_SIGS.contains(&entry.sig.as_str()))
            .unwrap_or(record.fields.len());
        record.fields.insert(
            at,
            FieldEntry {
                sig: SubrecordSig(*b"PRPS"),
                value: FieldValue::Bytes(SmallVec::from_slice(&row)),
            },
        );
        return true;
    };
    let FieldValue::Bytes(bytes) = &mut entry.value else {
        return false;
    };
    if bytes.len() % FO4_PROPERTY_ROW_LEN != 0 {
        return false;
    }
    match bytes
        .chunks_exact_mut(FO4_PROPERTY_ROW_LEN)
        .find(|existing| existing[0..4] == row[0..4])
    {
        Some(existing) if *existing == row => false,
        Some(existing) => {
            existing.copy_from_slice(&row);
            true
        }
        None => {
            bytes.extend_from_slice(&row);
            true
        }
    }
}

fn ensure_curve_source_avif(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    report: &mut FixupReport,
) -> Result<FormKey, FixupError> {
    let interner = mapper.interner;
    let marker =
        mapper.allocate_or_resolve(curve_source_marker_key(interner), None, SigCode(*b"AVIF"));
    let target_id = session.target_id();
    let exists = session
        .record_exists_in_handle(target_id, &form_key_to_read_str(&marker, interner))
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    if !exists {
        let editor_id = interner.intern(CURVE_SOURCE_EDITOR_ID);
        let mut avif = Record::new(SigCode(*b"AVIF"), marker);
        avif.eid = Some(editor_id);
        avif.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(editor_id),
            },
            FieldEntry {
                sig: SubrecordSig(*b"NAM0"),
                value: FieldValue::Float(0.0),
            },
            FieldEntry {
                sig: SubrecordSig(*b"AVFL"),
                value: FieldValue::Bytes(SmallVec::from_slice(
                    &AVIF_FLAG_DEFAULT_TO_0.to_le_bytes(),
                )),
            },
            FieldEntry {
                sig: SubrecordSig(*b"NAM1"),
                value: FieldValue::Bytes(SmallVec::from_slice(&AVIF_TYPE_VARIABLE.to_le_bytes())),
            },
        ]);
        session
            .add_record(avif, target_schema, interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        report.records_added += 1;
    }
    Ok(marker)
}

pub(crate) fn source_raw_to_form_key(
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
            &[
                ResolvedProperty {
                    target_actor_value_raw: 0x000002D4,
                    value: 99_890.0,
                },
                ResolvedProperty {
                    target_actor_value_raw: 0x000002D5,
                    value: 40_000.0,
                },
            ],
        );
        assert!(changed);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw PRPS");
        };
        // Health above i16::MAX is written through unchanged. The engine's auto-calc wraps it
        // to a negative int16, which the B21_TalesFromAppalachia F4SE plugin corrects at
        // runtime by recomputing the true total into the actor's float Health ActorValue.
        assert_eq!(
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            99_890.0
        );
        assert_eq!(
            f32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            40_000.0
        );

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
    fn scorchbeast_queen_health_is_the_curve_mean_unless_the_source_level_is_concrete() {
        let json = r#"{"curve":[{"x":1,"y":22061},{"x":12,"y":60165},{"x":23,"y":99004},{"x":34,"y":138723},{"x":45,"y":179495},{"x":56,"y":221108}]}"#;
        assert_eq!(
            crate::fixups::curve_table::mean_curve_value(json),
            Ok(99_890)
        );

        assert_eq!(flatten_queen_template_health(None), 99_890.0);

        assert_eq!(
            flatten_queen_template_health(Some(raw_acbs(34, 0, 0))),
            138_723.0
        );

        assert_eq!(
            flatten_queen_template_health(Some(raw_acbs(34, ACBS_FLAG_PC_LEVEL_MULT, 0))),
            99_890.0
        );
    }

    fn flatten_queen_template_health(source_acbs: Option<SmallVec<[u8; 32]>>) -> f32 {
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
                        fields: source_acbs
                            .map(|acbs| FieldEntry {
                                sig: SubrecordSig::from_str("ACBS").unwrap(),
                                value: FieldValue::Bytes(acbs),
                            })
                            .into_iter()
                            .chain([FieldEntry {
                                sig: SubrecordSig::from_str("PRPS").unwrap(),
                                value: FieldValue::Bytes(source_properties),
                            }])
                            .collect(),
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
        let health = f32::from_le_bytes(properties[4..8].try_into().unwrap());

        drop(session);
        let _ = std::fs::remove_dir_all(temp_root);
        assert!(plugin_handle_close_native(source_handle));
        assert!(plugin_handle_close_native(target_handle));
        health
    }

    fn property_rows(bytes: &[u8]) -> Vec<(u32, f32)> {
        bytes
            .chunks_exact(FO4_PROPERTY_ROW_LEN)
            .map(|row| {
                (
                    u32::from_le_bytes(row[0..4].try_into().unwrap()),
                    f32::from_le_bytes(row[4..8].try_into().unwrap()),
                )
            })
            .collect()
    }

    fn raw_fields(handle: u64, raw: u32) -> Vec<(String, Vec<u8>)> {
        let mut session = crate::session::open_session(handle, None).unwrap();
        let scan = session.handle_raw_scan(handle).unwrap();
        scan.with_record_subrecords(raw, |fields| {
            fields
                .iter()
                .map(|field| (field.signature.to_string(), field.data.to_vec()))
                .collect()
        })
        .unwrap_or_default()
    }

    fn field_of<'a>(fields: &'a [(String, Vec<u8>)], signature: &str) -> Option<&'a [u8]> {
        fields
            .iter()
            .find(|(found, _)| found == signature)
            .map(|(_, data)| data.as_slice())
    }

    #[test]
    fn own_converted_npcs_carry_one_curve_source_marker_naming_themselves() {
        let source_handle = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();

        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let curved = FormKey {
            local: 0x043C75,
            plugin,
        };
        let bare = FormKey {
            local: 0x05A1B2,
            plugin,
        };
        let overridden = FormKey {
            local: 0x05A1B3,
            plugin,
        };
        let master_npc = FormKey {
            local: 0x000007,
            plugin: interner.intern("Fallout4.esm"),
        };
        let field = |sig: &str, bytes: Vec<u8>| FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        };
        let record = |sig: &str, form_key, fields: Vec<FieldEntry>| Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key,
            eid: Some(interner.intern(&format!("Npc{:06X}", form_key.local))),
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        };
        let rows = |rows: &[(u32, f32)]| {
            rows.iter()
                .flat_map(|(actor_value, value)| {
                    [actor_value.to_le_bytes(), value.to_le_bytes()].concat()
                })
                .collect::<Vec<u8>>()
        };

        let source_schema = {
            let mut session = crate::session::open_session(source_handle, None).unwrap();
            let schema = session.schema().unwrap();
            let health_curve = [
                0x2D4u32.to_le_bytes(),
                0.0f32.to_le_bytes(),
                0x7ABE3Eu32.to_le_bytes(),
            ]
            .concat();
            for source in [
                record("NPC_", curved, vec![field("PRPS", health_curve)]),
                record("NPC_", bare, vec![field("ACBS", vec![0; 20])]),
                record("NPC_", overridden, vec![field("ACBS", vec![0; 20])]),
                Record {
                    sig: SigCode::from_str("CURV").unwrap(),
                    form_key: FormKey {
                        local: 0x7ABE3E,
                        plugin,
                    },
                    eid: Some(interner.intern("CT_Health")),
                    flags: RecordFlags::empty(),
                    fields: smallvec::smallvec![FieldEntry {
                        sig: SubrecordSig::from_str("JASF").unwrap(),
                        value: FieldValue::String(
                            interner.intern("Creatures\\Health\\Health.json")
                        ),
                    }],
                    warnings: SmallVec::new(),
                },
            ] {
                session
                    .add_record(source, schema.as_ref(), &interner)
                    .unwrap();
            }
            schema
        };
        let target_schema = {
            let mut session = crate::session::open_session(target_handle, None).unwrap();
            let schema = session.schema().unwrap();
            for target in [
                record("NPC_", curved, vec![field("PRPS", rows(&[(0x2D4, 0.0)]))]),
                record(
                    "NPC_",
                    bare,
                    vec![field("ACBS", vec![0; 20]), field("AIDT", vec![0; 20])],
                ),
                record(
                    "NPC_",
                    master_npc,
                    vec![field("PRPS", rows(&[(0x2D4, 5.0)]))],
                ),
            ] {
                session
                    .add_record(target, schema.as_ref(), &interner)
                    .unwrap();
            }
            schema
        };

        let temp_root = std::env::temp_dir().join(format!(
            "bacup_npc_marker_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let curve_path = temp_root.join("misc/curvetables/json/creatures/health/health.json");
        std::fs::create_dir_all(curve_path.parent().unwrap()).unwrap();
        std::fs::write(&curve_path, r#"{"curve":[{"x":1,"y":10},{"x":50,"y":30}]}"#).unwrap();

        let mut mapper_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        mapper_state.source_to_target.insert(overridden, master_npc);
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
        let config = FixupConfig {
            is_whole_plugin: true,
            source_extracted_dir: Some(temp_root.clone()),
            target_schema: Some(target_schema.clone()),
            source_schema: Some(source_schema.clone()),
            ..Default::default()
        };
        let mut run = || {
            let mut session =
                crate::session::open_session(target_handle, Some(source_handle)).unwrap();
            FlattenNpcPropertyCurvesFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap()
        };
        let first = run();
        let second = run();
        let marker = mapper
            .lookup(curve_source_marker_key(&interner))
            .expect("marker AVIF allocated");

        let marker_raw = 0x0100_0000 | marker.local;
        let avif = raw_fields(target_handle, marker_raw);
        let curved_fields = raw_fields(target_handle, 0x0100_0000 | curved.local);
        let bare_fields = raw_fields(target_handle, 0x0100_0000 | bare.local);
        let master_fields = raw_fields(target_handle, master_npc.local);
        let _ = std::fs::remove_dir_all(temp_root);
        assert!(plugin_handle_close_native(source_handle));
        assert!(plugin_handle_close_native(target_handle));

        assert_eq!(marker.plugin, plugin);
        assert_eq!(field_of(&avif, "EDID"), Some(&b"B21_NpcCurveSource\0"[..]));
        assert_eq!(field_of(&avif, "NAM0"), Some(&0.0f32.to_le_bytes()[..]));
        assert_eq!(field_of(&avif, "AVFL"), Some(&0x400u32.to_le_bytes()[..]));
        assert_eq!(field_of(&avif, "NAM1"), Some(&8u32.to_le_bytes()[..]));

        assert_eq!(
            property_rows(field_of(&curved_fields, "PRPS").unwrap()),
            vec![(0x2D4, 20.0), (marker_raw, 0x043C75 as f32)]
        );
        let bare_signatures: Vec<&str> = bare_fields
            .iter()
            .map(|(sig, _)| sig.as_str())
            .filter(|sig| *sig != "EDID")
            .collect();
        assert_eq!(bare_signatures, vec!["ACBS", "PRPS", "AIDT"]);
        assert_eq!(
            property_rows(field_of(&bare_fields, "PRPS").unwrap()),
            vec![(marker_raw, 0x05A1B2 as f32)]
        );
        assert_eq!(
            property_rows(field_of(&master_fields, "PRPS").unwrap()),
            vec![(0x2D4, 5.0)]
        );

        assert_eq!((first.records_added, first.records_changed), (1, 2));
        assert_eq!((second.records_added, second.records_changed), (0, 0));
    }

    fn raw_acbs(level: u16, flags: u32, template_flags: u16) -> SmallVec<[u8; 32]> {
        let mut bytes = SmallVec::from_slice(&[0u8; 20]);
        bytes[0..4].copy_from_slice(&flags.to_le_bytes());
        bytes[ACBS_LEVEL_OFFSET..ACBS_LEVEL_OFFSET + 2].copy_from_slice(&level.to_le_bytes());
        bytes[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
            .copy_from_slice(&template_flags.to_le_bytes());
        bytes
    }

    fn npc_with_acbs(value: FieldValue) -> Record {
        let mut record = npc_with_raw_properties(&[]);
        record.fields[0] = FieldEntry {
            sig: SubrecordSig::from_str("ACBS").unwrap(),
            value,
        };
        record
    }

    #[test]
    fn only_a_real_unscaled_untemplated_level_is_concrete() {
        let interner = StringInterner::new();
        let level = |acbs| npc_concrete_level(&npc_with_acbs(FieldValue::Bytes(acbs)), &interner);

        assert_eq!(level(raw_acbs(100, 0, 0)), Some(100));
        assert_eq!(level(raw_acbs(0, 0, 0)), None);
        assert_eq!(level(raw_acbs(100, ACBS_FLAG_PC_LEVEL_MULT, 0)), None);
        assert_eq!(level(raw_acbs(100, 0, ACBS_TEMPLATE_FLAG_STATS)), None);
        assert_eq!(
            level(raw_acbs(100, 0, !ACBS_TEMPLATE_FLAG_STATS)),
            Some(100)
        );

        let names = |names: &[&str]| {
            FieldValue::List(
                names
                    .iter()
                    .map(|name| FieldValue::String(interner.intern(name)))
                    .collect(),
            )
        };
        let level = |flags: &[&str], template_flags: &[&str]| {
            let configuration = FieldValue::Struct(vec![
                (interner.intern("Flags"), names(flags)),
                (interner.intern("Level"), FieldValue::Uint(100)),
                (interner.intern("TemplateFlags"), names(template_flags)),
            ]);
            npc_concrete_level(&npc_with_acbs(configuration), &interner)
        };

        assert_eq!(
            level(&["Respawn", "AutoCalcStats"], &["Traits", "Inventory"]),
            Some(100)
        );
        assert_eq!(level(&["AutoCalcStats", "PCLevelMult"], &[]), None);
        assert_eq!(level(&["AutoCalcStats"], &["Traits", "Stats"]), None);
    }
}
