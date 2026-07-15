//! Fixup: recover FO76 leveled-list Chance-None / Max-Count into FO4 LVLD/LVLM.
//!
//! # Why
//! FO76 does not store a leveled list's Chance-None and Max-Count as the plain
//! FO4 bytes. Each value has three source variants:
//!
//! | FO4 field        | GLOB ref | float value | literal byte |
//! |------------------|----------|-------------|--------------|
//! | Chance None LVLD | `LVLG`   | `LVCV`      | `LVLD`       |
//! | Max Count  LVLM  | `LVMG`   | `LVMV`      | `LVLM`       |
//!
//! `LVLG`/`LVMG` point at a `GLOB` whose `FLTV` holds the value (e.g.
//! `Container_MaxCount_Medium_Tier` = 15, `LL_Container_ChanceNone_Medium_ECON`
//! = 35). FO4 understands only the literal `LVLD`/`LVLM` bytes. The translator
//! drops the FO76-only subrecords and seeds `LVLD`/`LVLM` to 0, so every
//! converted list ends up with Chance-None 0 and Max-Count 0 unless the value is
//! recovered from the source.
//!
//! `clean_leveled_item_entries` recovers Chance-None from `LVLG`, but only for
//! records it already selects for entry cleanup (missing `LVLD`/`LVLM` defaults
//! or invalid entries). Because the translator seeds those defaults, almost every
//! list is skipped — which is why converted Chance-None is uniformly 0 — and
//! Max-Count is never recovered at all.
//!
//! # What
//! Run for every LVLI/LVLN, read the FO76 source record, and write the recovered
//! Chance-None (`LVLD`) and Max-Count (`LVLM`) onto the output record, resolving
//! `GLOB → float → literal` in that order. Nothing is written when the source has
//! no value for a field, so a legitimately-zero list is left untouched.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};

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
        // No source plugin (per-record asset ports without a source slot) → the
        // globals cannot be read, so there is nothing to recover.
        session.source_slot_opt().is_some()
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
                    let max_count = resolve_leveled_value(
                        &source,
                        "LVMG",
                        "LVMV",
                        "LVLM",
                        MAX_COUNT_MAX,
                        &mut resolve_global,
                    );

                    let mut changed = false;
                    if let Some(value) = chance_none {
                        changed |= set_uint8_subrecord(
                            &mut record,
                            "LVLD",
                            value,
                            &["LVLM", "LVLF", "LVLG", "LLCT", "LVLO", "LVLE", "COED"],
                        );
                    }
                    if let Some(value) = max_count {
                        changed |= set_uint8_subrecord(
                            &mut record,
                            "LVLM",
                            value,
                            &["LVLF", "LVLG", "LLCT", "LVLO", "LVLE", "COED"],
                        );
                    }

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

#[cfg(test)]
mod tests {
    use super::*;
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
        let max =
            resolve_leveled_value(&source, "LVMG", "LVMV", "LVLM", MAX_COUNT_MAX, &mut resolve);
        assert_eq!(
            chance,
            Some(0),
            "LVCV=0.0 present → chance none 0 (no-op against default)"
        );
        assert_eq!(max, Some(15), "LVMG global FLTV 15 → Max Count 15");
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
