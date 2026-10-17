//! Fixup: FO76 → FO4 OMOD `DATA` property values that the translator copies raw.
//!
//! - FO76 reads a float property from the curve table named in the row's last
//!   field (FO4's `Step`), in place of `Value 1`. Rapid's Speed row is
//!   `MulAdd 0` + `CT_Legendary_Weapon_Guns_RoF` (0.25), so a raw copy is +0%.
//!   The curve is evaluated into `Value 1` and `Step` zeroed.
//! - `MinPowerPerShot` (87) `Set`/`Add` values get the same unit change as the
//!   weapon's own DNAM field.
//! - A weapon Speed `MulAdd` also shortens charge time (see `mirror_speed_onto_charge_time`).
//!
//! Runs before `flatten_omod_includes`, so the `_PARENT_` rows it inlines are
//! already converted. Also reports the property ids the translator's filter dropped.

use std::collections::BTreeSet;

use rustc_hash::FxHashMap;

use crate::fixups::curve_table::{
    CurvePointsCache, cached_curve_y_at_level, source_key_for_target,
};
use crate::fixups::flatten_npc_property_curves::source_raw_to_form_key;
use crate::fixups::flatten_omod_includes::parse_data_layout;
use crate::fixups::synthesize_weap_data_blocks::FO76_TO_FO4_MIN_POWER_PER_SHOT_SCALE;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fo76_fo4::{omod_property_target, valid_object_mod_property};

/// FO4 has no item levels; like converted armor, mods get FO76's level-50 (max drop level) value.
const OMOD_CURVE_LEVEL: u16 = 50;
const PROPERTY_ROW_LEN: usize = 24;
const DATA_FORM_TYPE_OFFSET: usize = 10;
const VALUE_TYPE_FLOAT: u8 = 1;
const FUNCTION_SET: u8 = 0;
const FUNCTION_MUL_ADD: u8 = 1;
const FUNCTION_ADD: u8 = 2;
const PROPERTY_SPEED: u16 = 0;
const PROPERTY_FULL_POWER_SECONDS: u16 = 84;
const PROPERTY_MIN_POWER_PER_SHOT: u16 = 87;

#[derive(Clone, Copy, Debug, PartialEq)]
struct PropertyRow {
    value_type: u8,
    function: u8,
    property: u16,
    value1: u32,
    curve: u32,
}

impl PropertyRow {
    fn from_bytes(row: &[u8]) -> Self {
        Self {
            value_type: row[0],
            function: row[4],
            property: u16::from_le_bytes([row[8], row[9]]),
            value1: u32::from_le_bytes(row[12..16].try_into().unwrap()),
            curve: u32::from_le_bytes(row[20..24].try_into().unwrap()),
        }
    }
}

#[derive(Default)]
struct Counts {
    curves_flattened: u32,
    min_power_rescaled: u32,
    charge_time_mirrored: u32,
    dropped_properties: u32,
    dropped_property_ids: BTreeSet<u16>,
}

pub struct NormalizeFo76OmodPropertiesFixup;

impl Fixup for NormalizeFo76OmodPropertiesFixup {
    fn name(&self) -> &'static str {
        "normalize_fo76_omod_properties"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        let source_is_fo76 = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            .is_some_and(|game| game.eq_ignore_ascii_case("fo76"));
        let target_is_fo4 = session
            .target_slot()
            .parsed
            .game
            .as_deref()
            .is_some_and(|game| game.eq_ignore_ascii_case("fo4"));
        source_is_fo76
            && target_is_fo4
            && config.source_schema.is_some()
            && config.source_extracted_dir.is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let omod_sig = SigCode::from_str("OMOD")
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
        let target_to_source: FxHashMap<FormKey, FormKey> = mapper
            .source_to_target_iter()
            .map(|(source, target)| (target, source))
            .collect();
        let mut curve_cache = CurvePointsCache::default();
        let mut counts = Counts::default();
        let mut changed_records = Vec::new();
        let mut report = FixupReport::empty();

        let fks = session
            .form_keys_of_sig(omod_sig, mapper.interner)
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
            let Some(source_data) = data_value(&source_record) else {
                continue;
            };
            let source_rows = source_property_rows(source_data, mapper.interner);
            let property_target = omod_property_target(source_data, mapper.interner);
            for row in &source_rows {
                if !valid_object_mod_property(property_target, row.property) {
                    counts.dropped_properties += 1;
                    counts.dropped_property_ids.insert(row.property);
                }
            }
            if source_rows.is_empty() {
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
                        report.warnings.push(mapper.interner.intern(&format!(
                            "normalize_fo76_omod_properties:{eid}:target:{error}"
                        )));
                        continue;
                    }
                };
            let Some(FieldValue::Bytes(target_data)) = target_record
                .fields
                .iter_mut()
                .find(|entry| entry.sig.as_str() == "DATA")
                .map(|entry| &mut entry.value)
            else {
                continue;
            };
            let mut data = target_data.to_vec();
            let mut evaluate_curve = |curve_raw: u32| -> Result<f32, String> {
                let curve_fk = source_raw_to_form_key(
                    curve_raw,
                    &source_masters,
                    &source_plugin_name,
                    mapper.interner,
                )
                .ok_or_else(|| "null_curve".to_string())?;
                cached_curve_y_at_level(
                    curve_fk,
                    OMOD_CURVE_LEVEL,
                    session,
                    source_schema,
                    source_extracted_dir,
                    mapper.interner,
                    &mut curve_cache,
                )
                .map(|y| y as f32)
            };
            let mut curve_errors = Vec::new();
            let changed = normalize_data(
                &mut data,
                &source_rows,
                &mut evaluate_curve,
                &mut counts,
                &mut curve_errors,
            );
            for error in curve_errors {
                report.warnings.push(
                    mapper
                        .interner
                        .intern(&format!("normalize_fo76_omod_properties:{eid}:{error}")),
                );
            }
            if changed {
                *target_data = data.into();
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
                "normalize_fo76_omod_properties replaced {replaced} of {expected} expected records"
            )));
        }
        if counts.dropped_properties > 0 {
            report.warnings.push(mapper.interner.intern(&format!(
                "normalize_fo76_omod_properties:filter_dropped_properties:count={};ids={:?}",
                counts.dropped_properties, counts.dropped_property_ids
            )));
        }
        report.message = Some(mapper.interner.intern(&format!(
            "curves_flattened={} min_power_rescaled={} charge_time_mirrored={} filter_dropped={}",
            counts.curves_flattened,
            counts.min_power_rescaled,
            counts.charge_time_mirrored,
            counts.dropped_properties
        )));
        Ok(report)
    }
}

fn data_value(record: &Record) -> Option<&FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "DATA")
        .map(|entry| &entry.value)
}

/// Rewrites one target OMOD `DATA` in place against the source record's rows.
///
/// The translator only drops rows, never reorders them, so each target row is
/// paired with the next unconsumed source row of the same value type and property.
/// Value 1 is only changed while it still equals the source value, which keeps a
/// rerun from rescaling twice.
fn normalize_data(
    data: &mut Vec<u8>,
    source_rows: &[PropertyRow],
    evaluate_curve: &mut dyn FnMut(u32) -> Result<f32, String>,
    counts: &mut Counts,
    curve_errors: &mut Vec<String>,
) -> bool {
    let Some(layout) = parse_data_layout(data) else {
        return false;
    };
    let is_weapon = data.get(DATA_FORM_TYPE_OFFSET..DATA_FORM_TYPE_OFFSET + 4) == Some(b"WEAP");
    let mut changed = false;
    let mut cursor = 0;
    for index in 0..layout.property_count {
        let start = layout.properties_start + index * PROPERTY_ROW_LEN;
        let row = &mut data[start..start + PROPERTY_ROW_LEN];
        let target = PropertyRow::from_bytes(row);
        let Some(offset) = source_rows[cursor..].iter().position(|source| {
            source.value_type == target.value_type && source.property == target.property
        }) else {
            continue;
        };
        let source = source_rows[cursor + offset];
        cursor += offset + 1;
        if source.value_type != VALUE_TYPE_FLOAT || target.value1 != source.value1 {
            continue;
        }

        let mut value1 = f32::from_bits(source.value1);
        if source.curve != 0 {
            match evaluate_curve(source.curve) {
                Ok(value) => {
                    value1 = value;
                    row[20..24].copy_from_slice(&0u32.to_le_bytes());
                    counts.curves_flattened += 1;
                }
                Err(error) => curve_errors.push(error),
            }
        }
        if is_weapon
            && source.property == PROPERTY_MIN_POWER_PER_SHOT
            && matches!(source.function, FUNCTION_SET | FUNCTION_ADD)
        {
            value1 *= FO76_TO_FO4_MIN_POWER_PER_SHOT_SCALE;
            counts.min_power_rescaled += 1;
        }
        if value1.to_bits() != source.value1 || row[20..24] != source.curve.to_le_bytes() {
            row[12..16].copy_from_slice(&value1.to_bits().to_le_bytes());
            changed = true;
        }
    }
    if is_weapon {
        changed |= mirror_speed_onto_charge_time(data, counts);
    }
    changed
}

/// FO76 applies a Speed `MulAdd` to charge time too: on PTS, Rapid (+25% Speed)
/// shows "-25% Seconds to Charge" on the Compound Bow (2.0 s -> 1.5 s). FO4 does
/// not, so each one gets a matching `FullPowerSeconds` row. FO76's own charge
/// mods (fast-trigger receivers, sniper barrels) author `FullPowerSeconds`
/// themselves and are left alone; non-charging weapons have 0 to scale.
fn mirror_speed_onto_charge_time(data: &mut Vec<u8>, counts: &mut Counts) -> bool {
    let Some(layout) = parse_data_layout(data) else {
        return false;
    };
    let rows: Vec<PropertyRow> = data[layout.properties_start..layout.properties_end]
        .chunks_exact(PROPERTY_ROW_LEN)
        .map(PropertyRow::from_bytes)
        .collect();
    if rows
        .iter()
        .any(|row| row.property == PROPERTY_FULL_POWER_SECONDS)
    {
        return false;
    }
    let mirrored: Vec<u8> = rows
        .iter()
        .filter(|row| {
            row.property == PROPERTY_SPEED
                && row.value_type == VALUE_TYPE_FLOAT
                && row.function == FUNCTION_MUL_ADD
                && f32::from_bits(row.value1) != 0.0
        })
        .flat_map(|row| float_mul_add_row(PROPERTY_FULL_POWER_SECONDS, -f32::from_bits(row.value1)))
        .collect();
    if mirrored.is_empty() {
        return false;
    }
    let added = mirrored.len() / PROPERTY_ROW_LEN;
    data.splice(layout.properties_end..layout.properties_end, mirrored);
    data[4..8].copy_from_slice(&((layout.property_count + added) as u32).to_le_bytes());
    counts.charge_time_mirrored += added as u32;
    true
}

fn float_mul_add_row(property: u16, multiplier: f32) -> [u8; PROPERTY_ROW_LEN] {
    let mut row = [0u8; PROPERTY_ROW_LEN];
    row[0] = VALUE_TYPE_FLOAT;
    row[4] = FUNCTION_MUL_ADD;
    row[8..10].copy_from_slice(&property.to_le_bytes());
    row[12..16].copy_from_slice(&multiplier.to_bits().to_le_bytes());
    row
}

fn source_property_rows(value: &FieldValue, interner: &StringInterner) -> Vec<PropertyRow> {
    match value {
        FieldValue::Bytes(bytes) => parse_data_layout(bytes)
            .map(|layout| {
                bytes[layout.properties_start..layout.properties_end]
                    .chunks_exact(PROPERTY_ROW_LEN)
                    .map(PropertyRow::from_bytes)
                    .collect()
            })
            .unwrap_or_default(),
        FieldValue::Struct(fields) => match struct_member(fields, "properties", interner) {
            Some(FieldValue::List(rows)) => rows
                .iter()
                .filter_map(|row| struct_property_row(row, interner))
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn struct_property_row(value: &FieldValue, interner: &StringInterner) -> Option<PropertyRow> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    let member = |name| struct_member(fields, name, interner).map_or(0, raw_u32);
    Some(PropertyRow {
        value_type: member("valuetype") as u8,
        function: member("functiontype") as u8,
        property: member("property") as u16,
        value1: member("value1"),
        curve: member("step"),
    })
}

fn struct_member<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields.iter().find_map(|(key, value)| {
        interner
            .resolve(*key)
            .filter(|key| key.replace('_', "").eq_ignore_ascii_case(name))
            .map(|_| value)
    })
}

fn raw_u32(value: &FieldValue) -> u32 {
    match value {
        FieldValue::Uint(raw) => *raw as u32,
        FieldValue::Int(raw) => *raw as u32,
        FieldValue::Float(raw) => raw.to_bits(),
        FieldValue::FormKey(fk) => fk.local,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };
    use smallvec::SmallVec;
    use std::time::{SystemTime, UNIX_EPOCH};

    const RAPID_CURVE_RAW: u32 = 0x004E_7249;
    const MISSING_CURVE_RAW: u32 = 0x004E_7250;
    const RAPID_CURVE: &str = r#"{"curve":[{"x":1,"y":0.25},{"x":100,"y":0.25}]}"#;
    const PROPERTY_ATTACK_DAMAGE: u16 = 28;
    const PROPERTY_MAX_RANGE: u16 = 3;
    const FO76_ONLY_PROPERTY: u16 = 106;

    fn float_row(function: u8, property: u16, value1: f32, curve: u32) -> [u8; PROPERTY_ROW_LEN] {
        let mut row = [0u8; PROPERTY_ROW_LEN];
        row[0] = VALUE_TYPE_FLOAT;
        row[4] = function;
        row[8..10].copy_from_slice(&property.to_le_bytes());
        row[12..16].copy_from_slice(&value1.to_bits().to_le_bytes());
        row[20..24].copy_from_slice(&curve.to_le_bytes());
        row
    }

    fn omod_data(form_type: &[u8; 4], rows: &[[u8; PROPERTY_ROW_LEN]]) -> Vec<u8> {
        let mut data = vec![0u8; 20];
        data[4..8].copy_from_slice(&(rows.len() as u32).to_le_bytes());
        data[DATA_FORM_TYPE_OFFSET..DATA_FORM_TYPE_OFFSET + 4].copy_from_slice(form_type);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        for row in rows {
            data.extend_from_slice(row);
        }
        data
    }

    fn source_rows(data: &[u8]) -> Vec<PropertyRow> {
        let layout = parse_data_layout(data).unwrap();
        data[layout.properties_start..layout.properties_end]
            .chunks_exact(PROPERTY_ROW_LEN)
            .map(PropertyRow::from_bytes)
            .collect()
    }

    fn rows_of(data: &[u8]) -> Vec<(u8, u16, f32, u32)> {
        source_rows(data)
            .into_iter()
            .map(|row| {
                (
                    row.function,
                    row.property,
                    f32::from_bits(row.value1),
                    row.curve,
                )
            })
            .collect()
    }

    fn normalize(target: &mut Vec<u8>, source: &[u8], counts: &mut Counts) -> (bool, Vec<String>) {
        let mut errors = Vec::new();
        let mut curve = |raw: u32| match raw {
            RAPID_CURVE_RAW => Ok(0.25),
            _ => Err(format!("{raw:06X}:missing")),
        };
        let changed = normalize_data(
            target,
            &source_rows(source),
            &mut curve,
            counts,
            &mut errors,
        );
        (changed, errors)
    }

    #[test]
    fn curve_valued_rows_take_the_curve_value_and_lose_the_step() {
        let source = omod_data(
            b"ARMO",
            &[
                float_row(FUNCTION_MUL_ADD, 4, -0.1, RAPID_CURVE_RAW),
                float_row(FUNCTION_SET, PROPERTY_MAX_RANGE, 1.0, MISSING_CURVE_RAW),
                float_row(FUNCTION_ADD, PROPERTY_ATTACK_DAMAGE, 3.0, 0),
            ],
        );
        let mut target = source.clone();
        let mut counts = Counts::default();

        let (changed, errors) = normalize(&mut target, &source, &mut counts);

        assert!(changed);
        assert_eq!(
            rows_of(&target),
            vec![
                (FUNCTION_MUL_ADD, 4, 0.25, 0),
                (FUNCTION_SET, PROPERTY_MAX_RANGE, 1.0, MISSING_CURVE_RAW),
                (FUNCTION_ADD, PROPERTY_ATTACK_DAMAGE, 3.0, 0),
            ]
        );
        assert_eq!(errors, vec!["4E7250:missing".to_string()]);
        assert_eq!(counts.curves_flattened, 1);

        let flattened = target.clone();
        assert!(!normalize(&mut target, &source, &mut Counts::default()).0);
        assert_eq!(target, flattened);
    }

    #[test]
    fn rows_pair_with_their_source_row_after_the_filter_drops_one() {
        let source = omod_data(
            b"WEAP",
            &[
                float_row(FUNCTION_ADD, FO76_ONLY_PROPERTY, 0.1, RAPID_CURVE_RAW),
                float_row(
                    FUNCTION_MUL_ADD,
                    PROPERTY_ATTACK_DAMAGE,
                    0.0,
                    RAPID_CURVE_RAW,
                ),
            ],
        );
        let mut target = omod_data(
            b"WEAP",
            &[float_row(
                FUNCTION_MUL_ADD,
                PROPERTY_ATTACK_DAMAGE,
                0.0,
                RAPID_CURVE_RAW,
            )],
        );

        normalize(&mut target, &source, &mut Counts::default());

        assert_eq!(
            rows_of(&target),
            vec![(FUNCTION_MUL_ADD, PROPERTY_ATTACK_DAMAGE, 0.25, 0)]
        );
    }

    #[test]
    fn weapon_speed_bonus_also_shortens_charge_time_unless_the_mod_sets_it() {
        let rapid = omod_data(
            b"WEAP",
            &[float_row(
                FUNCTION_MUL_ADD,
                PROPERTY_SPEED,
                0.0,
                RAPID_CURVE_RAW,
            )],
        );
        let mut target = rapid.clone();
        let mut counts = Counts::default();

        assert!(normalize(&mut target, &rapid, &mut counts).0);
        assert_eq!(
            rows_of(&target),
            vec![
                (FUNCTION_MUL_ADD, PROPERTY_SPEED, 0.25, 0),
                (FUNCTION_MUL_ADD, PROPERTY_FULL_POWER_SECONDS, -0.25, 0),
            ]
        );
        assert_eq!(counts.charge_time_mirrored, 1);
        let mirrored = target.clone();
        assert!(!normalize(&mut target, &rapid, &mut Counts::default()).0);
        assert_eq!(target, mirrored);

        let sniper = omod_data(
            b"WEAP",
            &[
                float_row(FUNCTION_SET, PROPERTY_FULL_POWER_SECONDS, 1.5, 0),
                float_row(FUNCTION_MUL_ADD, PROPERTY_SPEED, -0.4, 0),
            ],
        );
        let mut target = sniper.clone();
        assert!(!normalize(&mut target, &sniper, &mut Counts::default()).0);
        assert_eq!(target, sniper);

        let no_speed_change = omod_data(
            b"WEAP",
            &[float_row(FUNCTION_MUL_ADD, PROPERTY_SPEED, 0.0, 0)],
        );
        let mut target = no_speed_change.clone();
        assert!(!normalize(&mut target, &no_speed_change, &mut Counts::default()).0);

        let armor_speed = omod_data(
            b"ARMO",
            &[float_row(FUNCTION_MUL_ADD, PROPERTY_SPEED, 0.3, 0)],
        );
        let mut target = armor_speed.clone();
        assert!(!normalize(&mut target, &armor_speed, &mut Counts::default()).0);
    }

    #[test]
    fn min_power_per_shot_set_and_add_get_the_weapon_dnam_unit_change() {
        let source = omod_data(
            b"WEAP",
            &[
                float_row(FUNCTION_SET, PROPERTY_MIN_POWER_PER_SHOT, 2.0, 0),
                float_row(FUNCTION_ADD, PROPERTY_MIN_POWER_PER_SHOT, 1.0, 0),
                float_row(FUNCTION_MUL_ADD, PROPERTY_MIN_POWER_PER_SHOT, 0.5, 0),
            ],
        );
        let mut target = source.clone();
        let mut counts = Counts::default();

        assert!(normalize(&mut target, &source, &mut counts).0);

        assert_eq!(
            rows_of(&target),
            vec![
                (
                    FUNCTION_SET,
                    PROPERTY_MIN_POWER_PER_SHOT,
                    2.0 * FO76_TO_FO4_MIN_POWER_PER_SHOT_SCALE,
                    0
                ),
                (
                    FUNCTION_ADD,
                    PROPERTY_MIN_POWER_PER_SHOT,
                    FO76_TO_FO4_MIN_POWER_PER_SHOT_SCALE,
                    0
                ),
                (FUNCTION_MUL_ADD, PROPERTY_MIN_POWER_PER_SHOT, 0.5, 0),
            ]
        );
        assert_eq!(counts.min_power_rescaled, 2);
        let rescaled = target.clone();
        assert!(!normalize(&mut target, &source, &mut Counts::default()).0);
        assert_eq!(target, rescaled, "a rerun must not rescale twice");
    }

    fn omod(interner: &StringInterner, form_key: FormKey, data: Vec<u8>) -> Record {
        Record {
            sig: SigCode::from_str("OMOD").unwrap(),
            form_key,
            eid: Some(interner.intern("mod_Legendary_Weapon2_Guns_RoF")),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(interner.intern("mod_Legendary_Weapon2_Guns_RoF")),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("DATA").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(data)),
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
    fn rapid_converts_to_a_quarter_more_speed_and_a_quarter_less_charge_time() {
        let source_handle = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();

        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let key = |local| FormKey { local, plugin };
        let rapid = key(0x52414F);
        let source_data = omod_data(
            b"WEAP",
            &[
                float_row(FUNCTION_MUL_ADD, PROPERTY_SPEED, 0.0, RAPID_CURVE_RAW),
                float_row(FUNCTION_ADD, FO76_ONLY_PROPERTY, 0.1, 0),
            ],
        );
        let target_data = omod_data(
            b"WEAP",
            &[float_row(
                FUNCTION_MUL_ADD,
                PROPERTY_SPEED,
                0.0,
                RAPID_CURVE_RAW,
            )],
        );

        let source_schema = {
            let mut session = crate::session::open_session(source_handle, None).unwrap();
            let schema = session.schema().unwrap();
            for record in [
                omod(&interner, rapid, source_data),
                curv(
                    &interner,
                    key(RAPID_CURVE_RAW),
                    r"LegendaryMods\Weapon_Guns_RoF.json",
                ),
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
            session
                .add_record(
                    omod(&interner, rapid, target_data),
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
            schema
        };

        let temp_root = std::env::temp_dir().join(format!(
            "bacup_omod_curve_{}_{}",
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
            .join("legendarymods")
            .join("weapon_guns_rof.json");
        std::fs::create_dir_all(curve_path.parent().unwrap()).unwrap();
        std::fs::write(&curve_path, RAPID_CURVE).unwrap();

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
            assert!(NormalizeFo76OmodPropertiesFixup.applies_to_session(&session, &config));
            NormalizeFo76OmodPropertiesFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap()
        };

        let mut session = crate::session::open_session(target_handle, None).unwrap();
        let target = session
            .record_decoded(&rapid, target_schema.as_ref(), &interner)
            .unwrap();
        let rows = match data_value(&target) {
            Some(FieldValue::Bytes(data)) => rows_of(data),
            other => panic!("expected raw OMOD.DATA, got {other:?}"),
        };
        let resolve = |sym| interner.resolve(sym).unwrap_or_default().to_owned();
        let warnings: Vec<String> = report.warnings.iter().map(|sym| resolve(*sym)).collect();
        let message = report.message.map(resolve);

        drop(session);
        let _ = std::fs::remove_dir_all(temp_root);
        assert!(plugin_handle_close_native(source_handle));
        assert!(plugin_handle_close_native(target_handle));

        assert_eq!(
            rows,
            vec![
                (FUNCTION_MUL_ADD, PROPERTY_SPEED, 0.25, 0),
                (FUNCTION_MUL_ADD, PROPERTY_FULL_POWER_SECONDS, -0.25, 0),
            ]
        );
        assert_eq!(report.records_changed, 1);
        assert_eq!(
            warnings,
            vec![
                "normalize_fo76_omod_properties:filter_dropped_properties:count=1;ids={106}"
                    .to_string()
            ]
        );
        assert_eq!(
            message.as_deref(),
            Some("curves_flattened=1 min_power_rescaled=0 charge_time_mirrored=1 filter_dropped=1")
        );
    }
}
