//! Fixup: recover FO76 leveled-list global-backed values into FO4 literals.
//!
//! FO76 stores a leveled list's Chance-None and Max-Count in three variants:
//!
//! | FO4 field        | GLOB ref | float value | literal byte |
//! |------------------|----------|-------------|--------------|
//! | Chance None LVLD | `LVLG`   | `LVCV`      | `LVLD`       |
//! | Max Count  LVLM  | `LVMG`   | `LVMV`      | `LVLM`       |
//!
//! `LVLG`/`LVMG` point at a `GLOB` whose `FLTV` holds the value (e.g.
//! `Container_MaxCount_Medium_Tier` = 15, `LL_Container_ChanceNone_Medium_ECON`
//! = 35). FO4 supports `LVLG`, so the translator keeps that live reference and the
//! materialized `LVLD` is only a fallback; the other variants are materialized into
//! `LVLD`/`LVLM`. A zero FO76 max count means no limit and is not written as `LVLM`.
//! Entry minimum levels held in an `LVOG` global (no FO4 equivalent) are written to
//! `LVLO.Level`; left at the placeholder zero, those entries are unavailable.
//!
//! `clean_leveled_item_entries` recovers Chance-None from `LVLG` only for records it
//! selects for entry cleanup, and the translator's seeded defaults make it skip
//! almost every list (converted Chance-None is uniformly 0). It never recovers
//! Max-Count. This pass runs for every LVLI/LVLN, reading the FO76 source record.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};
use crate::translator::pair_hook::{PairCtx, PairHook};
use crate::translator::pair_hooks::fo76_fo4::Fo76Fo4Hook;

/// Chance-None is a percentage; Max-Count is an unbounded `uint8` slot count.
const CHANCE_NONE_MAX: u8 = 100;
const MAX_COUNT_MAX: u8 = u8::MAX;

pub struct RecoverFo76LeveledListValuesFixup;

impl Fixup for RecoverFo76LeveledListValuesFixup {
    fn name(&self) -> &'static str {
        "recover_fo76_leveled_list_values"
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
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let source_schema = config
            .source_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing source schema in fixup config".into()))?;
        let interner = mapper.interner;
        let Some(source_plugin_name) = session
            .source_slot_opt()
            .map(|slot| slot.parsed.plugin_name.clone())
        else {
            return Ok(FixupReport::empty());
        };

        let mut report = FixupReport::empty();
        for sig_str in ["LVLI", "LVLN"] {
            let sig =
                SigCode::from_str(sig_str).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            let sig_report = session.map_apply_by_sig(
                sig,
                mapper,
                // Recovery depends on the FO76 source (unavailable in the parallel
                // decide phase), so every list is a candidate; the apply phase
                // returns NoOp when there is nothing to write.
                |view, _snapshot, fk| view.record_decoded(fk, target_schema, interner).ok(),
                |session, mapper, _fk, mut record| {
                    let source_fk = FormKey {
                        local: record.form_key.local,
                        plugin: mapper.interner.intern(&source_plugin_name),
                    };
                    let Ok(source) =
                        session.source_record_decoded(&source_fk, source_schema, mapper.interner)
                    else {
                        return Ok(EditOutcome::NoOp);
                    };

                    let mut resolve_global = |fk: &FormKey| {
                        session
                            .source_record_decoded(fk, source_schema, mapper.interner)
                            .ok()
                    };
                    let chance_none = resolve_leveled_value(
                        &source,
                        "LVLG",
                        "LVCV",
                        "LVLD",
                        CHANCE_NONE_MAX,
                        &mut resolve_global,
                    );
                    let max_count = resolve_max_count(&source, &mut resolve_global);
                    let mut entry_levels =
                        resolve_entry_levels(&source, mapper.interner, &mut resolve_global);

                    let mut changed = false;
                    if let Some(value) = chance_none {
                        changed |= set_uint8_subrecord(
                            &mut record,
                            "LVLD",
                            value,
                            &["LVLM", "LVLF", "LVLG", "LLCT", "LVLO", "LVLE", "COED"],
                        );
                    }
                    changed |= apply_max_count(&mut record, max_count);
                    changed |= apply_entry_levels(&mut record, &mut entry_levels, mapper);

                    if !changed {
                        return Ok(EditOutcome::NoOp);
                    }
                    session
                        .replace_record_contents(record, target_schema, mapper.interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    Ok(EditOutcome::Changed)
                },
            )?;
            report.records_changed += sig_report.records_changed;
            report.records_dropped += sig_report.records_dropped;
            report.records_added += sig_report.records_added;
            report.warnings.extend(sig_report.warnings);
        }
        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Value recovery (pure — `resolve_global` injected for unit-test access)
// ---------------------------------------------------------------------------

/// Recover one leveled-list value from a decoded FO76 source record, resolving
/// `GLOB → float → literal`. Returns `None` when the source carries no variant,
/// so the caller leaves the FO4 default untouched.
fn resolve_leveled_value(
    source: &Record,
    global_sig: &str,
    float_sig: &str,
    literal_sig: &str,
    clamp_max: u8,
    resolve_global: &mut impl FnMut(&FormKey) -> Option<Record>,
) -> Option<u8> {
    if let Some(global_fk) = subrecord_form_key(source, global_sig) {
        if global_fk.local != 0 {
            if let Some(global) = resolve_global(&global_fk) {
                if let Some(value) = glob_fltv(&global) {
                    return Some(clamp_u8(value, clamp_max));
                }
            }
        }
    }
    if let Some(value) = subrecord_float(source, float_sig) {
        return Some(clamp_u8(value, clamp_max));
    }
    subrecord_literal_u8(source, literal_sig)
}

fn resolve_max_count(
    source: &Record,
    resolve_global: &mut impl FnMut(&FormKey) -> Option<Record>,
) -> Option<u8> {
    resolve_leveled_value(
        source,
        "LVMG",
        "LVMV",
        "LVLM",
        MAX_COUNT_MAX,
        resolve_global,
    )
    .filter(|value| *value > 0)
}

fn apply_max_count(record: &mut Record, max_count: Option<u8>) -> bool {
    if let Some(value) = max_count {
        return set_uint8_subrecord(
            record,
            "LVLM",
            value,
            &["LVLF", "LVLG", "LLCT", "LVLO", "LVLE", "COED"],
        );
    }
    remove_subrecord(record, "LVLM")
}

#[derive(Clone, Copy, Debug)]
struct RecoveredEntryLevel {
    source_reference: FormKey,
    level: Option<u16>,
    used: bool,
}

fn resolve_entry_levels(
    source: &Record,
    interner: &crate::sym::StringInterner,
    resolve_global: &mut impl FnMut(&FormKey) -> Option<Record>,
) -> Vec<RecoveredEntryLevel> {
    let mut prepared = source.clone();
    let mut ctx = PairCtx::new(interner);
    if Fo76Fo4Hook.pre_translate(&mut ctx, &mut prepared).is_err() {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let mut index = 0;
    while index < prepared.fields.len() {
        if prepared.fields[index].sig.as_str() != "LVLO" {
            index += 1;
            continue;
        }

        let next_entry = prepared.fields[index + 1..]
            .iter()
            .position(|entry| entry.sig.as_str() == "LVLO")
            .map_or(prepared.fields.len(), |offset| index + 1 + offset);
        let source_reference = first_form_key(&prepared.fields[index].value);
        let level = prepared.fields[index + 1..next_entry]
            .iter()
            .find(|entry| entry.sig.as_str() == "LVOG")
            .and_then(|entry| first_form_key(&entry.value))
            .filter(|global_fk| global_fk.local != 0)
            .and_then(|global_fk| resolve_global(&global_fk))
            .and_then(|global| glob_fltv(&global))
            .map(clamp_u16);

        if let Some(source_reference) = source_reference {
            entries.push(RecoveredEntryLevel {
                source_reference,
                level,
                used: false,
            });
        }
        index = next_entry;
    }
    entries
}

fn apply_entry_levels(
    record: &mut Record,
    source_entries: &mut [RecoveredEntryLevel],
    mapper: &FormKeyMapper<'_>,
) -> bool {
    let interner = mapper.interner;
    let mut changed = false;

    for target_entry in record
        .fields
        .iter_mut()
        .filter(|entry| entry.sig.as_str() == "LVLO")
    {
        let Some(target_reference) = first_form_key(&target_entry.value) else {
            continue;
        };
        let matches_reference = |entry: &RecoveredEntryLevel| {
            mapper
                .lookup(entry.source_reference)
                .unwrap_or(entry.source_reference)
                == target_reference
        };
        let candidate = source_entries
            .iter()
            .position(|entry| !entry.used && matches_reference(entry));
        let Some(candidate) = candidate else {
            continue;
        };
        source_entries[candidate].used = true;
        if let Some(level) = source_entries[candidate].level {
            changed |= set_lvlo_level(&mut target_entry.value, level, interner);
        }
    }

    changed
}

fn set_lvlo_level(
    value: &mut FieldValue,
    level: u16,
    interner: &crate::sym::StringInterner,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            if bytes[0..2] == level.to_le_bytes() {
                return false;
            }
            bytes[0..2].copy_from_slice(&level.to_le_bytes());
            true
        }
        FieldValue::Struct(fields) => {
            if let Some((_, value)) = fields.iter_mut().find(|(name, _)| {
                interner
                    .resolve(*name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("level"))
            }) {
                return set_u16_value(value, level);
            }
            fields.insert(
                0,
                (
                    interner.intern("level"),
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&level.to_le_bytes())),
                ),
            );
            true
        }
        _ => false,
    }
}

fn set_u16_value(value: &mut FieldValue, replacement: u16) -> bool {
    match value {
        FieldValue::Uint(value) => {
            let replacement = u64::from(replacement);
            let changed = *value != replacement;
            *value = replacement;
            changed
        }
        FieldValue::Int(value) => {
            let replacement = i64::from(replacement);
            let changed = *value != replacement;
            *value = replacement;
            changed
        }
        FieldValue::Float(value) => {
            let replacement = f32::from(replacement);
            let changed = *value != replacement;
            *value = replacement;
            changed
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            if bytes[0..2] == replacement.to_le_bytes() {
                return false;
            }
            bytes[0..2].copy_from_slice(&replacement.to_le_bytes());
            true
        }
        _ => false,
    }
}

fn subrecord_form_key(record: &Record, sig: &str) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == sig)
        .and_then(|entry| first_form_key(&entry.value))
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, v)| first_form_key(v)),
        FieldValue::List(items) => items.iter().find_map(first_form_key),
        _ => None,
    }
}

fn subrecord_float(record: &Record, sig: &str) -> Option<f32> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == sig)
        .and_then(|entry| value_as_f32(&entry.value))
}

/// The `FLTV` value of a `GLOB` record. FO76 chance-none / max globals store a
/// float; other codecs are accepted defensively.
fn glob_fltv(record: &Record) -> Option<f32> {
    if record.sig.as_str() != "GLOB" {
        return None;
    }
    subrecord_float(record, "FLTV")
}

fn value_as_f32(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(f) => Some(*f),
        FieldValue::Uint(u) => Some(*u as f32),
        FieldValue::Int(i) => Some(*i as f32),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        _ => None,
    }
}

/// A literal `uint8` byte. An empty FO76 `LVLD`/`LVLM` (zero-length subrecord)
/// decodes to empty bytes and yields `None` — no value to recover.
fn subrecord_literal_u8(record: &Record, sig: &str) -> Option<u8> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == sig)
        .and_then(|entry| match &entry.value {
            FieldValue::Uint(u) => Some(*u as u8),
            FieldValue::Int(i) => Some(*i as u8),
            FieldValue::Bytes(bytes) => bytes.first().copied(),
            _ => None,
        })
}

fn clamp_u8(value: f32, clamp_max: u8) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(0.0, f32::from(clamp_max)) as u8
}

fn clamp_u16(value: f32) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(0.0, u16::MAX as f32) as u16
}

/// Set `sig_str` to `value`, updating in place or inserting before the first of
/// `before`. Returns whether the record changed.
fn set_uint8_subrecord(record: &mut Record, sig_str: &str, value: u8, before: &[&str]) -> bool {
    let Ok(sig) = SubrecordSig::from_str(sig_str) else {
        return false;
    };
    let new_value = FieldValue::Uint(u64::from(value));
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == sig) {
        if entry.value == new_value {
            return false;
        }
        entry.value = new_value;
        return true;
    }
    let index = record
        .fields
        .iter()
        .position(|entry| before.iter().any(|b| entry.sig.as_str() == *b))
        .unwrap_or(record.fields.len());
    record.fields.insert(
        index,
        FieldEntry {
            sig,
            value: new_value,
        },
    );
    true
}

fn remove_subrecord(record: &mut Record, sig_str: &str) -> bool {
    let Ok(sig) = SubrecordSig::from_str(sig_str) else {
        return false;
    };
    let before = record.fields.len();
    record.fields.retain(|entry| entry.sig != sig);
    record.fields.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::SigCode;
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn sub(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn lvli(fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("LVLI").unwrap(),
            form_key: FormKey {
                local: 0x39ED04,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: Some(interner.intern("LLC_Doctors_Bag")),
            flags: RecordFlags::empty(),
            fields: SmallVec::from_vec(fields),
            warnings: SmallVec::new(),
        }
    }

    fn glob(fltv: f32, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("GLOB").unwrap(),
            form_key: FormKey {
                local: 0x308434,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: SmallVec::from_vec(vec![sub("FLTV", FieldValue::Float(fltv))]),
            warnings: SmallVec::new(),
        }
    }

    fn never_resolves() -> impl FnMut(&FormKey) -> Option<Record> {
        |_: &FormKey| None
    }

    fn lvlo_without_level(reference: FormKey, count: u16, interner: &StringInterner) -> FieldEntry {
        sub(
            "LVLO",
            FieldValue::Struct(vec![
                (interner.intern("item"), FieldValue::FormKey(reference)),
                (
                    interner.intern("count"),
                    FieldValue::Bytes(SmallVec::from_slice(&count.to_le_bytes())),
                ),
            ]),
        )
    }

    fn lvlo_level(value: &FieldValue, interner: &StringInterner) -> Option<u16> {
        let FieldValue::Struct(fields) = value else {
            return None;
        };
        fields.iter().find_map(|(name, value)| {
            interner
                .resolve(*name)
                .is_some_and(|name| name == "level")
                .then(|| match value {
                    FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
                    }
                    _ => None,
                })
                .flatten()
        })
    }

    #[test]
    fn max_count_recovered_from_max_global() {
        let interner = StringInterner::new();
        let global_fk = FormKey {
            local: 0x308434,
            plugin: interner.intern("SeventySix.esm"),
        };
        // 0739ED04: empty LVLD, LVMG → Container_MaxCount_Medium_Tier (FLTV 15).
        let source = lvli(
            vec![
                sub("LVLD", FieldValue::Bytes(SmallVec::new())),
                sub("LVMV", FieldValue::Float(0.0)),
                sub("LVMG", FieldValue::FormKey(global_fk)),
                sub("LVCV", FieldValue::Float(0.0)),
            ],
            &interner,
        );
        let mut resolve = |fk: &FormKey| (fk.local == 0x308434).then(|| glob(15.0, &interner));

        let chance = resolve_leveled_value(
            &source,
            "LVLG",
            "LVCV",
            "LVLD",
            CHANCE_NONE_MAX,
            &mut resolve,
        );
        let max = resolve_max_count(&source, &mut resolve);
        assert_eq!(
            chance,
            Some(0),
            "LVCV=0.0 present → chance none 0 (no-op against default)"
        );
        assert_eq!(max, Some(15), "LVMG global FLTV 15 → Max Count 15");
    }

    #[test]
    fn zero_max_value_removes_target_max_count() {
        let interner = StringInterner::new();
        let source = lvli(vec![sub("LVMV", FieldValue::Float(0.0))], &interner);
        let mut none = never_resolves();
        assert_eq!(resolve_max_count(&source, &mut none), None);

        let mut target = lvli(vec![sub("LVLM", FieldValue::Uint(0))], &interner);
        assert!(apply_max_count(&mut target, None));
        assert!(
            target
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "LVLM")
        );
    }

    #[test]
    fn chance_none_recovered_from_global_then_float_then_literal() {
        let interner = StringInterner::new();
        let global_fk = FormKey {
            local: 0x361350,
            plugin: interner.intern("SeventySix.esm"),
        };
        let mut resolve = |_: &FormKey| Some(glob(35.0, &interner));

        // Global wins.
        let via_global = lvli(vec![sub("LVLG", FieldValue::FormKey(global_fk))], &interner);
        assert_eq!(
            resolve_leveled_value(
                &via_global,
                "LVLG",
                "LVCV",
                "LVLD",
                CHANCE_NONE_MAX,
                &mut resolve
            ),
            Some(35)
        );

        // Float value used when no global.
        let via_float = lvli(vec![sub("LVCV", FieldValue::Float(12.6))], &interner);
        let mut none = never_resolves();
        assert_eq!(
            resolve_leveled_value(
                &via_float,
                "LVLG",
                "LVCV",
                "LVLD",
                CHANCE_NONE_MAX,
                &mut none
            ),
            Some(13),
            "float rounds to nearest"
        );

        // Literal byte used when neither global nor float.
        let via_literal = lvli(vec![sub("LVLD", FieldValue::Uint(20))], &interner);
        assert_eq!(
            resolve_leveled_value(
                &via_literal,
                "LVLG",
                "LVCV",
                "LVLD",
                CHANCE_NONE_MAX,
                &mut none
            ),
            Some(20)
        );
    }

    #[test]
    fn chance_none_clamped_to_percentage() {
        let interner = StringInterner::new();
        let source = lvli(vec![sub("LVCV", FieldValue::Float(250.0))], &interner);
        let mut none = never_resolves();
        assert_eq!(
            resolve_leveled_value(&source, "LVLG", "LVCV", "LVLD", CHANCE_NONE_MAX, &mut none),
            Some(100),
            "chance none is a percentage"
        );
    }

    #[test]
    fn empty_literal_yields_no_value() {
        let interner = StringInterner::new();
        let source = lvli(
            vec![sub("LVLD", FieldValue::Bytes(SmallVec::new()))],
            &interner,
        );
        let mut none = never_resolves();
        assert_eq!(
            resolve_leveled_value(&source, "LVLG", "LVCV", "LVLD", CHANCE_NONE_MAX, &mut none),
            None
        );
    }

    #[test]
    fn entry_minimum_levels_are_recovered_from_lvog_globals() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let target_plugin = interner.intern("Converted.esp");
        let source_a = FormKey {
            local: 0x100001,
            plugin: source_plugin,
        };
        let source_b = FormKey {
            local: 0x100002,
            plugin: source_plugin,
        };
        let target_a = FormKey {
            local: 0x200001,
            plugin: target_plugin,
        };
        let target_b = FormKey {
            local: 0x200002,
            plugin: target_plugin,
        };
        let level_seven = FormKey {
            local: 0x300007,
            plugin: source_plugin,
        };
        let level_one = FormKey {
            local: 0x300001,
            plugin: source_plugin,
        };
        let source = lvli(
            vec![
                sub("LVLO", FieldValue::FormKey(source_a)),
                sub("LVIV", FieldValue::Float(1.0)),
                sub("LVLV", FieldValue::Float(0.0)),
                sub("LVLO", FieldValue::FormKey(source_a)),
                sub("LVIV", FieldValue::Float(1.0)),
                sub("LVLV", FieldValue::Float(0.0)),
                sub("LVOG", FieldValue::FormKey(level_seven)),
                sub("LVLO", FieldValue::FormKey(source_b)),
                sub("LVIV", FieldValue::Float(1.0)),
                sub("LVLV", FieldValue::Float(0.0)),
                sub("LVOG", FieldValue::FormKey(level_one)),
            ],
            &interner,
        );
        let mut resolve = |fk: &FormKey| match fk.local {
            0x300007 => Some(glob(7.0, &interner)),
            0x300001 => Some(glob(1.0, &interner)),
            _ => None,
        };
        let mut recovered = resolve_entry_levels(&source, &interner, &mut resolve);
        assert_eq!(
            recovered
                .iter()
                .map(|entry| entry.level)
                .collect::<Vec<_>>(),
            vec![None, Some(7), Some(1)]
        );

        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &interner);
        mapper.add_mapping(source_a, target_a);
        mapper.add_mapping(source_b, target_b);
        let mut target = lvli(
            vec![
                lvlo_without_level(target_a, 1, &interner),
                lvlo_without_level(target_a, 1, &interner),
                lvlo_without_level(target_b, 1, &interner),
            ],
            &interner,
        );

        assert!(apply_entry_levels(&mut target, &mut recovered, &mapper));
        let levels = target
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "LVLO")
            .map(|entry| lvlo_level(&entry.value, &interner))
            .collect::<Vec<_>>();
        assert_eq!(levels, vec![None, Some(7), Some(1)]);
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let encoded = crate::target_write::encode_field_pub(
            target
                .fields
                .iter()
                .filter(|entry| entry.sig.as_str() == "LVLO")
                .nth(1)
                .unwrap(),
            schema.record_def("LVLI"),
            &interner,
        )
        .unwrap();
        assert_eq!(&encoded[0..2], &7_u16.to_le_bytes());
        recovered.iter_mut().for_each(|entry| entry.used = false);
        assert!(
            !apply_entry_levels(&mut target, &mut recovered, &mapper),
            "second application is idempotent"
        );
    }

    #[test]
    fn entry_level_recovery_matches_reference_after_an_earlier_entry_was_dropped() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let source_a = FormKey {
            local: 0x100001,
            plugin: source_plugin,
        };
        let source_b = FormKey {
            local: 0x100002,
            plugin: source_plugin,
        };
        let level_five = FormKey {
            local: 0x300005,
            plugin: source_plugin,
        };
        let level_one = FormKey {
            local: 0x300001,
            plugin: source_plugin,
        };
        let source = lvli(
            vec![
                sub("LVLO", FieldValue::FormKey(source_a)),
                sub("LVOG", FieldValue::FormKey(level_five)),
                sub("LVLO", FieldValue::FormKey(source_b)),
                sub("LVOG", FieldValue::FormKey(level_one)),
            ],
            &interner,
        );
        let mut resolve = |fk: &FormKey| match fk.local {
            0x300005 => Some(glob(5.0, &interner)),
            0x300001 => Some(glob(1.0, &interner)),
            _ => None,
        };
        let mut recovered = resolve_entry_levels(&source, &interner, &mut resolve);
        let mapper = FormKeyMapper::new([], MapperOptions::default(), &interner);
        let mut target = lvli(vec![lvlo_without_level(source_b, 1, &interner)], &interner);

        assert!(apply_entry_levels(&mut target, &mut recovered, &mapper));
        assert_eq!(lvlo_level(&target.fields[0].value, &interner), Some(1));
    }

    #[test]
    fn set_uint8_updates_existing_and_inserts_missing() {
        let interner = StringInterner::new();

        // Update in place.
        let mut record = lvli(
            vec![
                sub("LVLD", FieldValue::Uint(0)),
                sub("LVLM", FieldValue::Uint(0)),
            ],
            &interner,
        );
        assert!(set_uint8_subrecord(
            &mut record,
            "LVLM",
            15,
            &["LVLF", "LLCT", "LVLO"]
        ));
        assert!(!set_uint8_subrecord(
            &mut record,
            "LVLM",
            15,
            &["LVLF", "LLCT", "LVLO"]
        ));
        let lvlm = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "LVLM")
            .unwrap();
        assert_eq!(lvlm.value, FieldValue::Uint(15));

        // Insert before the entry block.
        let mut record = lvli(
            vec![
                sub("LVLF", FieldValue::Uint(4)),
                sub("LLCT", FieldValue::Uint(5)),
            ],
            &interner,
        );
        assert!(set_uint8_subrecord(
            &mut record,
            "LVLD",
            35,
            &["LVLM", "LVLF", "LLCT", "LVLO"]
        ));
        assert_eq!(record.fields[0].sig.as_str(), "LVLD");
        assert_eq!(record.fields[0].value, FieldValue::Uint(35));
    }
}
