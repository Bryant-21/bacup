//! Fixup: re-case MOVT movement-type names to match the behavior graphs'
//! `iState_*` variables.
//!
//! FO4 resolves an actor's movement type by name: the root behavior graph
//! declares one `iState_<Name>` variable per stance, and the engine looks up the
//! MOVT whose `MNAM` equals `<Name>`, case-sensitively. Two FO76 MNAMs differ
//! from their graph variable only in case:
//!
//! | MOVT MNAM          | graph variable            |
//! |--------------------|---------------------------|
//! | `MoleminerDefault` | `iState_MoleMinerDefault` |
//! | `frogDefault`      | `iState_FrogDefault`      |
//!
//! FO4 then never resolves the movement type and the requested speed falls back
//! to the engine default, so mole miners crawl whatever their SPED says
//! (retargeting SPED alone changed nothing in game). MNAMs are re-cased against
//! the `iState_*` names in the source behavior graphs, which survive conversion
//! verbatim.

use std::collections::HashMap;
use std::path::Path;

use crate::fixups::creature::creature_internal_fixup_applies;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

pub struct NormalizeMovementTypeNamesFixup;

impl Fixup for NormalizeMovementTypeNamesFixup {
    fn name(&self) -> &'static str {
        "normalize_movement_type_names"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::CreatureGated
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        creature_internal_fixup_applies(ctx.config) && ctx.source_extracted_dir.is_some()
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        creature_internal_fixup_applies(config) && config.source_extracted_dir.is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let Some(source_root) = config.source_extracted_dir.as_deref() else {
            return Ok(FixupReport::empty());
        };
        let movt_sig =
            SigCode::from_str("MOVT").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();
        let mut names: HashMap<String, String> = HashMap::new();
        collect_istate_names_in_tree(&source_root.join("Meshes").join("Actors"), &mut names);
        for extra in &config.additional_source_asset_roots {
            collect_istate_names_in_tree(&extra.join("Meshes").join("Actors"), &mut names);
        }
        if names.is_empty() {
            return Ok(report);
        }

        let movt_fks = session
            .form_keys_of_sig(movt_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in movt_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("normalize_mtname_read:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };
            if apply_to_record(&mut record, &names, mapper.interner) {
                session
                    .replace_record(record, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                report.records_changed += 1;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// iState_* name collection
// ---------------------------------------------------------------------------

/// Scan every behavior `.hkx` under `root` for ASCII `iState_<Name>`
/// occurrences and record `lower(<Name>) -> <Name>`. A lower-key claimed by
/// two DIFFERENT casings is poisoned (removed) — never guess between graphs.
fn collect_istate_names_in_tree(root: &Path, out: &mut HashMap<String, String>) {
    let mut poisoned: Vec<String> = Vec::new();
    walk_behavior_hkx(root, &mut |path| {
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        for name in extract_istate_names(&bytes) {
            let key = name.to_ascii_lowercase();
            match out.get(&key) {
                Some(existing) if existing != &name => poisoned.push(key),
                Some(_) => {}
                None => {
                    out.insert(key, name);
                }
            }
        }
    });
    for key in poisoned {
        out.remove(&key);
    }
}

fn walk_behavior_hkx(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_behavior_hkx(&path, f);
            continue;
        }
        let in_behaviors_dir = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("behaviors"));
        let is_hkx = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("hkx"));
        if in_behaviors_dir && is_hkx {
            f(&path);
        }
    }
}

/// Extract every `iState_<Name>` ASCII token from raw packfile bytes.
/// Variable names are stored as plain nul-terminated strings, so a byte scan
/// is exact without parsing the packfile.
fn extract_istate_names(bytes: &[u8]) -> Vec<String> {
    const PREFIX: &[u8] = b"iState_";
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(pos) = find_from(bytes, PREFIX, i) {
        let start = pos + PREFIX.len();
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        if end > start {
            if let Ok(name) = std::str::from_utf8(&bytes[start..end]) {
                out.push(name.to_string());
            }
        }
        i = end.max(pos + 1);
    }
    out
}

fn find_from(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Re-case the MNAM name to the graph's exact casing when it matches a known
/// `iState_*` name case-insensitively but not exactly. Returns `true` if the
/// record changed.
pub fn apply_to_record(
    record: &mut Record,
    names: &HashMap<String, String>,
    interner: &StringInterner,
) -> bool {
    let mnam_sig = match SubrecordSig::from_str("MNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    for entry in &mut record.fields {
        if entry.sig != mnam_sig {
            continue;
        }
        let FieldValue::String(sym) = entry.value else {
            break;
        };
        let Some(current) = interner.resolve(sym) else {
            break;
        };
        let Some(exact) = names.get(&current.to_ascii_lowercase()) else {
            break;
        };
        if exact != &current {
            entry.value = FieldValue::String(interner.intern(exact));
            return true;
        }
        break; // Only one MNAM.
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FormKey;
    use crate::record::{FieldEntry, RecordFlags};

    fn make_movt(interner: &StringInterner, mnam: &str) -> Record {
        Record {
            sig: SigCode::from_str("MOVT").unwrap(),
            form_key: FormKey {
                local: 0x0616FD,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("MNAM").unwrap(),
                value: FieldValue::String(interner.intern(mnam)),
            }],
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn mnam_of(record: &Record, interner: &StringInterner) -> String {
        let mnam_sig = SubrecordSig::from_str("MNAM").unwrap();
        for entry in &record.fields {
            if entry.sig == mnam_sig {
                if let FieldValue::String(sym) = entry.value {
                    return interner.resolve(sym).unwrap().to_string();
                }
            }
        }
        panic!("no MNAM");
    }

    #[test]
    fn recases_mismatched_default_name() {
        let interner = StringInterner::new();
        let mut names = HashMap::new();
        names.insert(
            "moleminerdefault".to_string(),
            "MoleMinerDefault".to_string(),
        );
        let mut record = make_movt(&interner, "MoleminerDefault");
        assert!(apply_to_record(&mut record, &names, &interner));
        assert_eq!(mnam_of(&record, &interner), "MoleMinerDefault");
    }

    #[test]
    fn exact_match_is_untouched() {
        let interner = StringInterner::new();
        let mut names = HashMap::new();
        names.insert("moleminergun".to_string(), "MoleMinerGun".to_string());
        let mut record = make_movt(&interner, "MoleMinerGun");
        assert!(!apply_to_record(&mut record, &names, &interner));
    }

    #[test]
    fn unknown_name_is_untouched() {
        let interner = StringInterner::new();
        let names = HashMap::new();
        let mut record = make_movt(&interner, "MoleminerDefault");
        assert!(!apply_to_record(&mut record, &names, &interner));
        assert_eq!(mnam_of(&record, &interner), "MoleminerDefault");
    }

    #[test]
    fn extracts_istate_tokens_from_raw_bytes() {
        let bytes = b"garbage\0iState_MoleMinerDefault\0more\0iState_FrogDefault\0iState_";
        let names = extract_istate_names(bytes);
        assert_eq!(
            names,
            vec!["MoleMinerDefault".to_string(), "FrogDefault".to_string()]
        );
    }

    #[test]
    fn conflicting_casings_are_poisoned() {
        let dir = tempfile::tempdir().unwrap();
        let behaviors = dir.path().join("X").join("behaviors");
        std::fs::create_dir_all(&behaviors).unwrap();
        std::fs::write(behaviors.join("a.hkx"), b"iState_FooDefault\0").unwrap();
        std::fs::write(behaviors.join("b.hkx"), b"iState_FooDEFAULT\0").unwrap();
        std::fs::write(behaviors.join("c.hkx"), b"iState_BarGun\0").unwrap();
        let mut names = HashMap::new();
        collect_istate_names_in_tree(dir.path(), &mut names);
        assert!(!names.contains_key("foodefault"), "conflict must poison");
        assert_eq!(names.get("bargun").map(String::as_str), Some("BarGun"));
    }

    #[test]
    fn live_census_finds_the_two_known_mismatches() {
        // Live-tree oracle: the FO76 source ships exactly the MoleMiner and
        // Frog Default mismatches. Skips when the source tree is absent.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .expect("conversion crate lives at repo/bacup/py_bacup_lib/native/conversion")
            .join("extracted/fo76/Meshes/Actors");
        if !root.is_dir() {
            eprintln!("fo76 extracted tree absent; skipping");
            return;
        }
        let mut names = HashMap::new();
        collect_istate_names_in_tree(&root, &mut names);
        assert_eq!(
            names.get("moleminerdefault").map(String::as_str),
            Some("MoleMinerDefault")
        );
        assert_eq!(
            names.get("frogdefault").map(String::as_str),
            Some("FrogDefault")
        );
    }
}
