//! Fixup: strip non-SNDR Sound references from STAG records.
//!
//! FO76 puts LVLI (leveled sound lists) and other non-SNDR records in some STAG
//! `Sound` fields; FO4 expects SNDR or NULL, and xEdit flags the rest. A TNAM
//! (`Struct` of `sound` FormKey + `action` string) whose `sound` resolves to a
//! non-SNDR record gets a null `sound`, keeping `action`. Null or unresolvable
//! sounds (external master not loaded) are left alone.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};
use esp_authoring_core::plugin_runtime::ensure_core_section;

// ---------------------------------------------------------------------------
// SNDR-compatible signature
// ---------------------------------------------------------------------------

/// The only 4-char record sig that FO4 accepts in a STAG Sound field.
const SNDR_SIG: &str = "SNDR";

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FixStagSoundRefsFixup;

impl Fixup for FixStagSoundRefsFixup {
    fn name(&self) -> &'static str {
        "fix_stag_sound_refs"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let stag_sig =
            SigCode::from_str("STAG").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let mut report = FixupReport::empty();

        // ── 1. Build FK→sig map for every record in the target plugin ─────
        let fk_to_sig = build_fk_sig_map(session, mapper.interner)?;

        if fk_to_sig.is_empty() {
            return Ok(report);
        }

        // Intern "sound" and "action" once for O(1) struct field lookup.
        let sound_sym = mapper.interner.intern("sound");

        // ── 2. Iterate STAG records ───────────────────────────────────────
        let stag_fks = session
            .form_keys_of_sig(stag_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in stag_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("stag_sound_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            let (stripped, changed) = apply_to_record(&mut record, sound_sym, &fk_to_sig);
            if changed {
                session
                    .replace_record(record, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                report.records_changed += 1;
                report.records_dropped += stripped;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// FK→sig map builder
// ---------------------------------------------------------------------------

/// Build a map from `(local, plugin_sym)` → `SigCode` for every record in
/// `handle_id`, using the index (no record decode required).
fn build_fk_sig_map(
    session: &mut PluginSession,
    interner: &StringInterner,
) -> Result<rustc_hash::FxHashMap<(u32, Sym), SigCode>, FixupError> {
    let sigs = {
        let core = ensure_core_section(session.target_slot_mut());
        core.by_signature_form_keys
            .keys()
            .filter_map(|sig| SigCode::from_str(sig.as_str()).ok())
            .collect::<Vec<_>>()
    };
    let mut map: rustc_hash::FxHashMap<(u32, Sym), SigCode> = rustc_hash::FxHashMap::default();

    for sig in sigs {
        let fks = session
            .form_keys_of_sig(sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in fks {
            map.insert((fk.local, fk.plugin), sig);
        }
    }

    Ok(map)
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Null each TNAM `sound` FK that references a non-SNDR record, keeping the
/// entry and its `action`. Returns `(stripped_count, changed)`.
pub fn apply_to_record(
    record: &mut Record,
    sound_sym: Sym,
    fk_to_sig: &rustc_hash::FxHashMap<(u32, Sym), SigCode>,
) -> (u32, bool) {
    let tnam_sig = match SubrecordSig::from_str("TNAM") {
        Ok(s) => s,
        Err(_) => return (0, false),
    };

    let mut stripped: u32 = 0;

    for entry in record.fields.iter_mut() {
        if entry.sig != tnam_sig {
            continue;
        }

        let FieldValue::Struct(ref mut fields) = entry.value else {
            continue;
        };

        // Find the "sound" field within this TNAM struct.
        for (field_sym, field_val) in fields.iter_mut() {
            if *field_sym != sound_sym {
                continue;
            }

            // Only act on a non-null FormKey.
            let FieldValue::FormKey(ref fk) = *field_val else {
                break;
            };
            if fk.local == 0 {
                break;
            }

            // Look up the referenced record's sig.
            let ref_sig = fk_to_sig.get(&(fk.local, fk.plugin));
            let should_strip = match ref_sig {
                Some(sig) => sig.as_str() != SNDR_SIG,
                // FK not found in target map — unknown/external FK. Leave
                // unchanged (only strip when we positively know the type is
                // wrong).
                None => false,
            };

            if should_strip {
                *field_val = FieldValue::None;
                stripped += 1;
            }

            // Only one "sound" field per TNAM struct.
            break;
        }
    }

    (stripped, stripped > 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_stag(fk: FormKey, tnam_entries: Vec<FieldEntry>) -> Record {
        let sig = SigCode::from_str("STAG").unwrap();
        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: tnam_entries.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn tnam_entry(sound_fk: FormKey, action: &str, interner: &StringInterner) -> FieldEntry {
        let sound_sym = interner.intern("sound");
        let action_sym = interner.intern("action");
        let action_val = interner.intern(action);
        FieldEntry {
            sig: SubrecordSig::from_str("TNAM").unwrap(),
            value: FieldValue::Struct(vec![
                (sound_sym, FieldValue::FormKey(sound_fk)),
                (action_sym, FieldValue::String(action_val)),
            ]),
        }
    }

    fn null_tnam_entry(action: &str, interner: &StringInterner) -> FieldEntry {
        let sound_sym = interner.intern("sound");
        let action_sym = interner.intern("action");
        let action_val = interner.intern(action);
        FieldEntry {
            sig: SubrecordSig::from_str("TNAM").unwrap(),
            value: FieldValue::Struct(vec![
                (
                    sound_sym,
                    FieldValue::FormKey(FormKey {
                        local: 0,
                        plugin: interner.intern("Fallout4.esm"),
                    }),
                ),
                (action_sym, FieldValue::String(action_val)),
            ]),
        }
    }

    fn make_fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn make_sig_map(
        entries: &[(u32, &str, &str)], // (local, plugin, sig)
        interner: &StringInterner,
    ) -> rustc_hash::FxHashMap<(u32, Sym), SigCode> {
        let mut map = rustc_hash::FxHashMap::default();
        for (local, plugin, sig) in entries {
            let plugin_sym = interner.intern(plugin);
            let sig_code = SigCode::from_str(sig).unwrap();
            map.insert((*local, plugin_sym), sig_code);
        }
        map
    }

    #[test]
    fn apply_nulls_only_tnam_sounds_that_resolve_to_leveled_lists() {
        let interner = StringInterner::new();
        let sound_sym = interner.intern("sound");
        let stag_fk = make_fk(0x000800, "Out.esp", &interner);
        let map = make_sig_map(
            &[(0x001000, "Out.esp", "SNDR"), (0x002000, "Out.esp", "LVLI")],
            &interner,
        );
        let sndr = || tnam_entry(make_fk(0x001000, "Out.esp", &interner), "Attack", &interner);
        let lvli = || tnam_entry(make_fk(0x002000, "Out.esp", &interner), "Equip", &interner);
        let unknown = tnam_entry(
            make_fk(0x0ABCDE, "Fallout4.esm", &interner),
            "Draw",
            &interner,
        );
        let edid = FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("TestSTAG")),
        };

        for (name, fields, stripped_at) in [
            ("no tnam", vec![], vec![]),
            ("sndr", vec![sndr()], vec![]),
            ("lvli", vec![lvli()], vec![0]),
            (
                "null sound",
                vec![null_tnam_entry("Attack", &interner)],
                vec![],
            ),
            ("unmapped sound", vec![unknown], vec![]),
            ("mixed", vec![sndr(), lvli()], vec![1]),
            ("non-tnam kept", vec![edid, lvli()], vec![1]),
        ] {
            let mut expected = make_stag(stag_fk, fields.clone());
            for &index in &stripped_at {
                let FieldValue::Struct(ref mut tnam) = expected.fields[index].value else {
                    panic!("{name}: expected Struct");
                };
                tnam[0] = (sound_sym, FieldValue::None);
            }
            let mut record = make_stag(stag_fk, fields);

            let (stripped, changed) = apply_to_record(&mut record, sound_sym, &map);
            assert_eq!(stripped as usize, stripped_at.len(), "{name}");
            assert_eq!(changed, !stripped_at.is_empty(), "{name}");
            assert_eq!(record.fields, expected.fields, "{name}");
        }
    }
}
