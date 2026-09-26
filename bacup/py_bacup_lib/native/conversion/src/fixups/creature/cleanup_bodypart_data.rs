//! Fixup: remap BPTD `ModelFileName` to the owning Race's `MaleSkeletalModel`.
//!
//! Translated BPTD records keep the FO76 skeleton `MODL`. The RACE's `GNAM`
//! names its BPTD, and its male skeleton path is the first `ANAM` zstring after
//! the first `MNAM` marker.

use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::Sym;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct CleanupBodypartDataFixup;

impl Fixup for CleanupBodypartDataFixup {
    fn name(&self) -> &'static str {
        "cleanup_bodypart_data"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, _ctx: &FixupContext) -> bool {
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
        let race_sig =
            SigCode::from_str("RACE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let bptd_sig =
            SigCode::from_str("BPTD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();

        // ── Phase 1: build BPTD FK → Male Skeletal Model path map ──────────
        let race_fks = session
            .form_keys_of_sig(race_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut bptd_to_skel: HashMap<FormKey, Sym> = HashMap::new();
        for fk in race_fks {
            let record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("cleanup_bptd_race_read:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            let Some(bptd_fk) = extract_bptd_formkey(&record) else {
                continue;
            };
            let Some(skel_sym) = extract_male_skeletal_model_sym(&record) else {
                continue;
            };
            // Only register non-empty paths.
            if mapper
                .interner
                .resolve(skel_sym)
                .map(str::is_empty)
                .unwrap_or(true)
            {
                continue;
            }
            bptd_to_skel.insert(bptd_fk, skel_sym);
        }

        if bptd_to_skel.is_empty() {
            return Ok(report);
        }

        // ── Phase 2: rewrite BPTD.MODL when it differs from the mapped path ─
        let bptd_fks = session
            .form_keys_of_sig(bptd_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in bptd_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("cleanup_bptd_read:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            let Some(&skel_sym) = bptd_to_skel.get(&fk) else {
                continue;
            };

            if apply_to_record(&mut record, skel_sym) {
                let replaced = session
                    .replace_record_contents(record, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                if !replaced {
                    return Err(FixupError::HandleError(
                        "cleanup_bodypart_data expected existing BPTD record".into(),
                    ));
                }
                report.records_changed += 1;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Rewrite the BPTD's first `MODL` zstring to `skel_sym` if it differs.
/// Returns `true` when a write occurred.
pub fn apply_to_record(record: &mut Record, skel_sym: Sym) -> bool {
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(s) => s,
        Err(_) => return false,
    };

    for entry in record.fields.iter_mut() {
        if entry.sig != modl_sig {
            continue;
        }
        let current_sym = match entry.value {
            FieldValue::String(s) => s,
            _ => return false,
        };
        if current_sym == skel_sym {
            return false;
        }
        entry.value = FieldValue::String(skel_sym);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Helpers (RACE field extraction)
// ---------------------------------------------------------------------------

/// Extract the GNAM (Body Part Data) FormKey from a RACE record.
pub fn extract_bptd_formkey(record: &Record) -> Option<FormKey> {
    let gnam_sig = SubrecordSig::from_str("GNAM").ok()?;
    for entry in &record.fields {
        if entry.sig != gnam_sig {
            continue;
        }
        if let FieldValue::FormKey(fk) = entry.value {
            return Some(fk);
        }
        return None;
    }
    None
}

/// Extract the Male Skeletal Model `Sym` from a RACE record — the first ANAM
/// zstring that appears after the first MNAM marker.
pub fn extract_male_skeletal_model_sym(record: &Record) -> Option<Sym> {
    let mnam_sig = SubrecordSig::from_str("MNAM").ok()?;
    let anam_sig = SubrecordSig::from_str("ANAM").ok()?;
    let mut seen_mnam = false;
    for entry in &record.fields {
        if !seen_mnam {
            if entry.sig == mnam_sig {
                seen_mnam = true;
            }
            continue;
        }
        if entry.sig == anam_sig {
            if let FieldValue::String(sym) = entry.value {
                return Some(sym);
            }
            return None;
        }
    }
    None
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

    fn make_fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn make_bptd(local: u32, plugin: &str, modl_path: &str, interner: &StringInterner) -> Record {
        let sig = SigCode::from_str("BPTD").unwrap();
        let fk = make_fk(local, plugin, interner);
        let modl_sig = SubrecordSig::from_str("MODL").unwrap();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("TestBPTD");
        let modl_sym = interner.intern(modl_path);

        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(edid_sym),
        });
        fields.push(FieldEntry {
            sig: modl_sig,
            value: FieldValue::String(modl_sym),
        });

        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn make_race(
        local: u32,
        plugin: &str,
        gnam_target: Option<FormKey>,
        male_skel_path: Option<&str>,
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("RACE").unwrap();
        let fk = make_fk(local, plugin, interner);
        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

        // Optional Male Skeletal Model block: MNAM marker followed by ANAM zstring.
        if let Some(path) = male_skel_path {
            let mnam_sig = SubrecordSig::from_str("MNAM").unwrap();
            let anam_sig = SubrecordSig::from_str("ANAM").unwrap();
            let path_sym = interner.intern(path);
            fields.push(FieldEntry {
                sig: mnam_sig,
                value: FieldValue::None,
            });
            fields.push(FieldEntry {
                sig: anam_sig,
                value: FieldValue::String(path_sym),
            });
        }

        if let Some(target) = gnam_target {
            let gnam_sig = SubrecordSig::from_str("GNAM").unwrap();
            fields.push(FieldEntry {
                sig: gnam_sig,
                value: FieldValue::FormKey(target),
            });
        }

        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    #[test]
    fn apply_rewrites_modl_only_when_it_differs() {
        const MEGASLOTH: &str = "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif";
        for (name, modl, expected_changed) in [
            (
                "differs",
                Some("actors/ogua/characterassets/skeleton.nif"),
                true,
            ),
            ("matches", Some(MEGASLOTH), false),
            ("no_modl", None, false),
        ] {
            let interner = StringInterner::new();
            let mut record = make_bptd(0x000100, "Output.esp", modl.unwrap_or(""), &interner);
            if modl.is_none() {
                record.fields.retain(|e| e.sig.as_str() != "MODL");
            }
            let changed = apply_to_record(&mut record, interner.intern(MEGASLOTH));
            assert_eq!(changed, expected_changed, "{name}");
            let modl_value =
                record
                    .fields
                    .iter()
                    .find(|e| e.sig.as_str() == "MODL")
                    .map(|e| match e.value {
                        FieldValue::String(s) => interner.resolve(s),
                        _ => panic!("expected MODL to be String"),
                    });
            assert_eq!(modl_value, modl.map(|_| Some(MEGASLOTH)), "{name}");
        }
    }

    /// An ANAM without a preceding MNAM is not the Male Skeletal Model.
    #[test]
    fn extracts_race_bptd_formkey_and_male_skeleton() {
        let interner = StringInterner::new();
        let target = make_fk(0x00ABCD, "Fallout4.esm", &interner);
        let record = make_race(0x000800, "Output.esp", Some(target), None, &interner);
        assert_eq!(extract_bptd_formkey(&record), Some(target));
        assert!(extract_male_skeletal_model_sym(&record).is_none());

        let record = make_race(
            0x000800,
            "Output.esp",
            None,
            Some("Actors\\MegaSloth\\CharacterAssets\\skeleton.nif"),
            &interner,
        );
        assert!(extract_bptd_formkey(&record).is_none());
        let sym = extract_male_skeletal_model_sym(&record).expect("must find Male ANAM");
        assert_eq!(
            interner.resolve(sym),
            Some("Actors\\MegaSloth\\CharacterAssets\\skeleton.nif")
        );

        let mut orphan = make_race(0x000800, "Output.esp", None, None, &interner);
        orphan.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ANAM").unwrap(),
            value: FieldValue::String(interner.intern("orphan.nif")),
        });
        assert!(extract_male_skeletal_model_sym(&orphan).is_none());
    }
}
