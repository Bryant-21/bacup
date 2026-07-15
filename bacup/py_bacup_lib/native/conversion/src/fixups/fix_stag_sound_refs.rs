//! Fixup: strip non-SNDR Sound references from STAG records.
//!

//!
//! # What this does
//! FO76 uses LVLI (leveled sound lists) or other non-SNDR record types in some
//! STAG `Sound` fields.  FO4 expects SNDR (SoundDescriptor) or NULL.
//! References to wrong record types produce xEdit errors.
//!
//! For each STAG record in the target plugin, this fixup inspects every TNAM
//! subrecord.  TNAM is decoded as a `FieldValue::Struct` with two fields:
//! - `sound` — `FieldValue::FormKey` pointing at the referenced sound record.
//! - `action` — `FieldValue::String` (the animation action tag).
//!
//! When the `sound` FK resolves to a record whose signature is not `SNDR`, the
//! FK is replaced with a null FormKey (local = 0), preserving the `action`
//! field intact.  TNAM entries whose `sound` is already null, or whose FK does
//! not resolve at all (external master not loaded), are left unchanged.
//!
//! # Algorithm
//! 1. Build a `FxHashMap<(local, plugin_sym) → SigCode>` for every record in
//!    the target plugin (one `iter_form_keys_of_sig` call per signature).
//! 2. For each STAG FormKey in the target plugin, read the record and call
//!    `apply_to_record`.
//! 3. `apply_to_record` scans TNAM structs, strips non-SNDR `sound` FKs, and
//!    returns `true` when any mutation occurred.
//! 4. Mutated records are written back with `replace_record_native`.

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

/// Scan TNAM subrecords in a STAG record and null out the `sound` FK whenever
/// it references a non-SNDR record type.
///
/// Returns `(stripped_count, changed)`:
/// - `stripped_count` — number of `sound` FKs that were nulled.
/// - `changed` — whether any mutation occurred (i.e. `stripped_count > 0`).
///
/// TNAM entries are kept regardless — only the `sound` field is zeroed, not
/// the entire entry, preserving the `action` field.
///
/// # Parameters
/// - `sound_sym` — interned `Sym` for the string `"sound"` (struct field name).
/// - `fk_to_sig` — map from `(local, plugin_sym)` to `SigCode` for target records.
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
    fn apply_no_tnam_is_no_op() {
        let mut interner = StringInterner::new();
        let sound_sym = interner.intern("sound");
        let fk = make_fk(0x000800, "Out.esp", &mut interner);
        let mut record = make_stag(fk, vec![]);
        let map = make_sig_map(&[], &mut interner);

        let (stripped, changed) = apply_to_record(&mut record, sound_sym, &map);
        assert_eq!(stripped, 0);
        assert!(!changed);
    }

    #[test]
    fn apply_keeps_sndr_sound() {
        let mut interner = StringInterner::new();
        let sound_sym = interner.intern("sound");
        let sndr_fk = make_fk(0x001000, "Out.esp", &mut interner);
        let stag_fk = make_fk(0x000800, "Out.esp", &mut interner);
        let entry = tnam_entry(sndr_fk, "Attack", &mut interner);
        let mut record = make_stag(stag_fk, vec![entry]);

        let map = make_sig_map(&[(0x001000, "Out.esp", "SNDR")], &mut interner);

        let (stripped, changed) = apply_to_record(&mut record, sound_sym, &map);
        assert_eq!(stripped, 0, "SNDR should not be stripped");
        assert!(!changed);
    }

    #[test]
    fn apply_strips_lvli_sound() {
        let mut interner = StringInterner::new();
        let sound_sym = interner.intern("sound");
        let lvli_fk = make_fk(0x002000, "Out.esp", &mut interner);
        let stag_fk = make_fk(0x000800, "Out.esp", &mut interner);
        let entry = tnam_entry(lvli_fk, "Equip", &mut interner);
        let mut record = make_stag(stag_fk, vec![entry]);

        let map = make_sig_map(&[(0x002000, "Out.esp", "LVLI")], &mut interner);

        let (stripped, changed) = apply_to_record(&mut record, sound_sym, &map);
        assert_eq!(stripped, 1);
        assert!(changed);

        // Entry is still present, but sound field is now None.
        assert_eq!(record.fields.len(), 1);
        if let FieldValue::Struct(ref fields) = record.fields[0].value {
            let (sym, val) = &fields[0];
            assert_eq!(*sym, interner.intern("sound"));
            assert_eq!(*val, FieldValue::None, "sound must be nulled");
        } else {
            panic!("expected Struct");
        }
    }

    #[test]
    fn apply_leaves_null_sound_unchanged() {
        let mut interner = StringInterner::new();
        let sound_sym = interner.intern("sound");
        let stag_fk = make_fk(0x000800, "Out.esp", &mut interner);
        let entry = null_tnam_entry("Attack", &mut interner);
        let mut record = make_stag(stag_fk, vec![entry]);

        let map = make_sig_map(&[], &mut interner);

        let (stripped, changed) = apply_to_record(&mut record, sound_sym, &map);
        assert_eq!(stripped, 0, "null FK must not be stripped");
        assert!(!changed);
    }

    #[test]
    fn apply_leaves_unknown_fk_unchanged() {
        let mut interner = StringInterner::new();
        let sound_sym = interner.intern("sound");
        let external_fk = make_fk(0x0ABCDE, "Fallout4.esm", &mut interner);
        let stag_fk = make_fk(0x000800, "Out.esp", &mut interner);
        let entry = tnam_entry(external_fk, "Draw", &mut interner);
        let mut record = make_stag(stag_fk, vec![entry]);

        // Map is empty — FK not found.
        let map = make_sig_map(&[], &mut interner);

        let (stripped, changed) = apply_to_record(&mut record, sound_sym, &map);
        assert_eq!(stripped, 0, "unknown FK should not be stripped");
        assert!(!changed);
    }

    #[test]
    fn apply_strips_lvli_keeps_sndr_in_mixed_record() {
        let mut interner = StringInterner::new();
        let sound_sym = interner.intern("sound");

        let sndr_fk = make_fk(0x001000, "Out.esp", &mut interner);
        let lvli_fk = make_fk(0x002000, "Out.esp", &mut interner);
        let stag_fk = make_fk(0x000800, "Out.esp", &mut interner);

        let entry_sndr = tnam_entry(sndr_fk, "Attack", &mut interner);
        let entry_lvli = tnam_entry(lvli_fk, "Equip", &mut interner);
        let mut record = make_stag(stag_fk, vec![entry_sndr, entry_lvli]);

        let map = make_sig_map(
            &[(0x001000, "Out.esp", "SNDR"), (0x002000, "Out.esp", "LVLI")],
            &mut interner,
        );

        let (stripped, changed) = apply_to_record(&mut record, sound_sym, &map);
        assert_eq!(stripped, 1, "exactly one LVLI entry should be stripped");
        assert!(changed);
        assert_eq!(record.fields.len(), 2, "both TNAM entries must remain");

        // First TNAM (SNDR): sound FK intact.
        if let FieldValue::Struct(ref fields) = record.fields[0].value {
            assert!(
                matches!(fields[0].1, FieldValue::FormKey(_)),
                "SNDR sound must remain as FormKey"
            );
        }

        // Second TNAM (LVLI): sound nulled.
        if let FieldValue::Struct(ref fields) = record.fields[1].value {
            assert_eq!(fields[0].1, FieldValue::None, "LVLI sound must be None");
        }
    }

    #[test]
    fn apply_preserves_non_tnam_fields() {
        let mut interner = StringInterner::new();
        let sound_sym = interner.intern("sound");

        let stag_fk = make_fk(0x000800, "Out.esp", &mut interner);
        let edid_sym = interner.intern("TestSTAG");

        let edid_entry = FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(edid_sym),
        };
        let lvli_fk = make_fk(0x002000, "Out.esp", &mut interner);
        let tnam = tnam_entry(lvli_fk, "Equip", &mut interner);
        let mut record = make_stag(stag_fk, vec![edid_entry, tnam]);

        let map = make_sig_map(&[(0x002000, "Out.esp", "LVLI")], &mut interner);

        let (stripped, changed) = apply_to_record(&mut record, sound_sym, &map);
        assert_eq!(stripped, 1);
        assert!(changed);
        assert_eq!(record.fields.len(), 2, "EDID and TNAM both remain");
        assert_eq!(record.fields[0].sig.as_str(), "EDID", "EDID must be first");
    }
}
