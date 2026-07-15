//! Rewrite Race skeletal model paths for sub-creatures.
//!

//!
//! # What this does
//! Some FO76 sub-creatures inherit the parent creature's behavior graph and
//! share the parent's `CharacterAssets` skeleton path in the source RACE record,
//! but ship their own skeleton nested under
//! `Actors/<Parent>/<child>/skeleton.nif`.  Without this rewrite, the converted
//! race animates the sub-creature mesh on the parent's bones and the NPC
//! T-poses in-game.
//!
//! Detection signal:
//!   - `MaleSkeletalModel` ends in `CharacterAssets/skeleton.nif`
//!   - A `Path` field entry is one sub-directory deeper than `Actors/<Parent>/Animations/`
//!   - A `skeleton.nif` exists at `<source_extracted_dir>/meshes/Actors/<Parent>/<child>/skeleton.nif`
//!     (or one level up without `meshes/`)
//!
//! Canonical rewrites:
//!   - `MaleSkeletalModel`: `Actors\<Parent>\CharacterAssets\skeleton.nif`
//!     → `Actors\<Parent>\<child>\skeleton.nif`
//!   - `FemaleSkeletalModel`: same rewrite only when it points at the same
//!     parent `CharacterAssets/skeleton.nif`. Some FO76 creature races keep a
//!     target-game fallback in the female row (e.g. Sheepsquatch → Deathclaw,
//!     RadHog → Molerat); those must remain untouched.
//!
//! # Schema mapping
//! In the FO4 RACE schema, both Male and Female skeletal model paths share the
//! subrecord signature `ANAM` (zstring). They are positionally disambiguated by
//! a preceding `MNAM` (Male Marker, empty) and `FNAM` (Female Marker, empty)
//! subrecord. The first `ANAM` after `MNAM` is `MaleSkeletalModel`; the first
//! `ANAM` after `FNAM` is `FemaleSkeletalModel`. The `Path` field-list entries
//! use subrecord sig `SAPT` (zstring, repeatable).

use std::path::Path as FsPath;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Lower-cased, slash-normalised suffix that identifies a parent CharacterAssets
/// skeleton path.
const CHARACTERASSETS_MARKER: &str = "/characterassets/skeleton.nif";

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FixSubcreatureSkeletonPathsFixup;

impl Fixup for FixSubcreatureSkeletonPathsFixup {
    fn name(&self) -> &'static str {
        "fix_subcreature_skeleton_paths"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.nifs
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.source_extracted_dir.is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let extracted = match config.source_extracted_dir.as_deref() {
            Some(path) => path,
            None => return Ok(FixupReport::empty()),
        };
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let race_sig =
            SigCode::from_str("RACE").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let mut report = FixupReport::empty();

        let race_fks = session
            .form_keys_of_sig(race_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        if race_fks.is_empty() {
            return Ok(report);
        }

        for fk in race_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("fix_subcreature_skel_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            if rewrite_subcreature_paths(&mut record, extracted, mapper.interner) {
                let replaced = session
                    .replace_record_contents(record, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                if !replaced {
                    return Err(FixupError::HandleError(
                        "fix_subcreature_skeleton_paths expected existing RACE record".into(),
                    ));
                }
                report.records_changed += 1;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Core rewrite (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Inspect a RACE `Record` and rewrite the male/female skeletal model paths
/// when the sub-creature pattern is detected. Returns `true` when the record
/// was mutated.
///
/// Mutation is performed in place via the supplied interner.
pub fn rewrite_subcreature_paths(
    record: &mut Record,
    extracted: &FsPath,
    interner: &StringInterner,
) -> bool {
    let male_anam_idx = match find_anam_after_marker(record, "MNAM", interner) {
        Some(idx) => idx,
        None => return false,
    };
    let male_file = match string_value(&record.fields[male_anam_idx].value, interner) {
        Some(s) if !s.is_empty() => s,
        _ => return false,
    };

    let norm = male_file.replace('\\', "/").to_lowercase();
    if !norm.ends_with(CHARACTERASSETS_MARKER) {
        return false;
    }
    let head = &norm[..norm.len() - CHARACTERASSETS_MARKER.len()];
    let head_parts: Vec<&str> = head.split('/').collect();
    if head_parts.len() != 2 || head_parts[0] != "actors" {
        return false;
    }
    let parent = head_parts[1];
    if parent.is_empty() {
        return false;
    }

    let parent_anim_prefix = format!("actors/{parent}/animations/");
    let candidates = collect_candidate_subs(record, &parent_anim_prefix, interner);
    if candidates.is_empty() {
        return false;
    }

    let chosen = match pick_existing_subskeleton(&candidates, extracted, parent) {
        Some(c) => c,
        None => return false,
    };

    // Preserve the parent segment casing from the existing path.
    let backslash = male_file.replace('/', "\\");
    let parts: Vec<&str> = backslash.split('\\').collect();
    let parent_cased = if parts.len() >= 2 { parts[1] } else { parent };
    let new_file = format!("Actors\\{parent_cased}\\{}\\skeleton.nif", chosen.0);

    // Rewrite male skeletal model.
    let new_sym = interner.intern(&new_file);
    record.fields[male_anam_idx].value = FieldValue::String(new_sym);

    // Rewrite female skeletal model only when it is the same parent skeleton.
    // FO76 creature races sometimes store a target-game fallback skeleton in
    // the female row; rewriting that fallback to the custom FO76 skeleton
    // removes the exact FO4-compatible path the game can animate.
    if let Some(female_anam_idx) = find_anam_after_marker(record, "FNAM", interner) {
        if let Some(female_file) = string_value(&record.fields[female_anam_idx].value, interner) {
            let fnorm = female_file.replace('\\', "/").to_lowercase();
            if fnorm == norm {
                record.fields[female_anam_idx].value = FieldValue::String(new_sym);
            }
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Find the index of the first `ANAM` field that follows the named marker
/// (e.g. `MNAM` or `FNAM`). Returns `None` when the marker is absent or has
/// no following `ANAM`.
fn find_anam_after_marker(
    record: &Record,
    marker: &str,
    _interner: &StringInterner,
) -> Option<usize> {
    let marker_sig = SubrecordSig::from_str(marker).ok()?;
    let anam_sig = SubrecordSig::from_str("ANAM").ok()?;
    let mut after_marker = false;
    for (i, entry) in record.fields.iter().enumerate() {
        if entry.sig == marker_sig {
            after_marker = true;
            continue;
        }
        if after_marker && entry.sig == anam_sig {
            return Some(i);
        }
    }
    None
}

/// Read a `FieldValue::String` as an owned `String`. Returns `None` for
/// non-string values or when the interner can't resolve the symbol.
fn string_value(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_string()),
        _ => None,
    }
}

/// Collect candidate sub-creature names from the record's `SAPT` (Path) entries.
/// Returns `(original_case_tail, lower_case_tail)` tuples, deduplicated and in
/// first-occurrence order.
fn collect_candidate_subs(
    record: &Record,
    parent_anim_prefix: &str,
    interner: &StringInterner,
) -> Vec<(String, String)> {
    let sapt_sig = match SubrecordSig::from_str("SAPT") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mut out: Vec<(String, String)> = Vec::new();
    for entry in &record.fields {
        if entry.sig != sapt_sig {
            continue;
        }
        let path_val = match string_value(&entry.value, interner) {
            Some(s) => s,
            None => continue,
        };
        let pn_orig = path_val.replace('\\', "/");
        let pn_orig = pn_orig.trim_matches('/').to_string();
        let pn = pn_orig.to_lowercase();
        if !pn.starts_with(parent_anim_prefix) {
            continue;
        }
        let tail = &pn[parent_anim_prefix.len()..];
        if tail.is_empty() || tail.contains('/') {
            continue;
        }
        let tail_orig = &pn_orig[parent_anim_prefix.len()..];
        let entry_pair = (tail_orig.to_string(), tail.to_string());
        if !out.iter().any(|e| e == &entry_pair) {
            out.push(entry_pair);
        }
    }
    out
}

/// Probe each candidate against the source extracted tree. Returns the first
/// candidate whose nested `skeleton.nif` exists on disk.
fn pick_existing_subskeleton<'a>(
    candidates: &'a [(String, String)],
    extracted: &FsPath,
    parent: &str,
) -> Option<&'a (String, String)> {
    for sub in candidates {
        if should_preserve_parent_skeleton(parent, &sub.1) {
            continue;
        }
        let probes = [
            extracted
                .join("meshes")
                .join("actors")
                .join(parent)
                .join(&sub.1)
                .join("skeleton.nif"),
            extracted
                .join("actors")
                .join(parent)
                .join(&sub.1)
                .join("skeleton.nif"),
        ];
        if probes.iter().any(|p| p.is_file()) {
            return Some(sub);
        }
    }
    None
}

fn should_preserve_parent_skeleton(parent: &str, sub: &str) -> bool {
    let parent = parent.to_ascii_lowercase();
    let sub = sub.to_ascii_lowercase();
    sub.starts_with("ambush") || (parent == "megasloth" && sub == "ogua")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::FixupConfig;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    /// Build a RACE record with a male+female ANAM and optional SAPT Path
    /// entries. `sub_names` are the (cased) child directory names inserted via
    /// SAPT zstrings under `Actors\<parent>\Animations\<sub>`.
    fn make_race(
        local: u32,
        plugin: &str,
        parent: &str,
        male_path: &str,
        female_path: Option<&str>,
        sub_names: &[&str],
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("RACE").unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        let mnam_sig = SubrecordSig::from_str("MNAM").unwrap();
        let fnam_sig = SubrecordSig::from_str("FNAM").unwrap();
        let anam_sig = SubrecordSig::from_str("ANAM").unwrap();
        let sapt_sig = SubrecordSig::from_str("SAPT").unwrap();

        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

        // MNAM (empty marker) then male ANAM.
        fields.push(FieldEntry {
            sig: mnam_sig,
            value: FieldValue::None,
        });
        fields.push(FieldEntry {
            sig: anam_sig,
            value: FieldValue::String(interner.intern(male_path)),
        });

        // FNAM (empty marker) then female ANAM (if provided).
        if let Some(fp) = female_path {
            fields.push(FieldEntry {
                sig: fnam_sig,
                value: FieldValue::None,
            });
            fields.push(FieldEntry {
                sig: anam_sig,
                value: FieldValue::String(interner.intern(fp)),
            });
        }

        // Append SAPT path entries.
        let _ = parent;
        for sub in sub_names {
            let p = format!("Actors\\{parent}\\Animations\\{sub}");
            fields.push(FieldEntry {
                sig: sapt_sig,
                value: FieldValue::String(interner.intern(&p)),
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

    /// Create a temporary extracted-dir layout with `skeleton.nif` for the
    /// given (parent, sub) pairs under `meshes/actors/<parent>/<sub>/`.
    fn make_extracted_tree(parent: &str, subs: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for sub in subs {
            let nif_dir = dir
                .path()
                .join("meshes")
                .join("actors")
                .join(parent)
                .join(sub);
            std::fs::create_dir_all(&nif_dir).unwrap();
            std::fs::write(nif_dir.join("skeleton.nif"), b"").unwrap();
        }
        dir
    }

    #[test]
    fn does_not_apply_without_source_extracted_dir() {
        let config = FixupConfig::default();
        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "SubcreatureNoExtracted.esp",
            Some("fo4"),
        )
        .expect("test plugin handle");
        let session = crate::session::open_session(target_handle, None).expect("open session");
        assert!(!FixSubcreatureSkeletonPathsFixup.applies_to_session(&session, &config));
    }

    #[test]
    fn applies_when_source_extracted_dir_present() {
        let mut config = FixupConfig::default();
        let dir = tempfile::tempdir().unwrap();
        config.source_extracted_dir = Some(dir.path().to_path_buf());
        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "SubcreatureWithExtracted.esp",
            Some("fo4"),
        )
        .expect("test plugin handle");
        let session = crate::session::open_session(target_handle, None).expect("open session");
        assert!(FixSubcreatureSkeletonPathsFixup.applies_to_session(&session, &config));
    }

    #[test]
    fn rewrites_subcreature_skeleton_when_disk_exists() {
        let mut interner = StringInterner::new();
        let extracted = make_extracted_tree("MegaSloth", &["Variant"]);
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif",
            Some("Actors\\MegaSloth\\CharacterAssets\\skeleton.nif"),
            &["Variant"],
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(changed, "fixup should mutate the record");

        let male_idx = find_anam_after_marker(&record, "MNAM", &interner).unwrap();
        let male_str = string_value(&record.fields[male_idx].value, &interner).unwrap();
        assert_eq!(male_str, "Actors\\MegaSloth\\Variant\\skeleton.nif");

        let female_idx = find_anam_after_marker(&record, "FNAM", &interner).unwrap();
        let female_str = string_value(&record.fields[female_idx].value, &interner).unwrap();
        assert_eq!(female_str, "Actors\\MegaSloth\\Variant\\skeleton.nif");
    }

    #[test]
    fn no_op_when_disk_skeleton_missing() {
        let mut interner = StringInterner::new();
        let dir = tempfile::tempdir().unwrap(); // empty tree
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif",
            Some("Actors\\MegaSloth\\CharacterAssets\\skeleton.nif"),
            &["Variant"],
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, dir.path(), &mut interner);
        assert!(!changed);

        // Male still points to the parent skeleton.
        let male_idx = find_anam_after_marker(&record, "MNAM", &interner).unwrap();
        let male_str = string_value(&record.fields[male_idx].value, &interner).unwrap();
        assert_eq!(male_str, "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif");
    }

    #[test]
    fn no_op_when_male_not_characterassets() {
        let mut interner = StringInterner::new();
        let extracted = make_extracted_tree("MegaSloth", &["Variant"]);
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\Variant\\skeleton.nif", // already child-pointed
            Some("Actors\\MegaSloth\\CharacterAssets\\skeleton.nif"),
            &["Variant"],
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(!changed);
    }

    #[test]
    fn female_left_alone_when_not_characterassets() {
        let mut interner = StringInterner::new();
        let extracted = make_extracted_tree("MegaSloth", &["Variant"]);
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif",
            Some("Actors\\Molerat\\CharacterAssets\\skeleton.nif"),
            &["Variant"],
            &mut interner,
        );
        // Mark female with a different (non-marker) ending — but in the schema
        // female still ends with /characterassets/skeleton.nif. We instead point
        // it at a wholly-different path so the marker test fails:
        let female_idx = find_anam_after_marker(&record, "FNAM", &interner).unwrap();
        record.fields[female_idx].value =
            FieldValue::String(interner.intern("Actors\\Molerat\\Variant\\skeleton.nif"));

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(changed);

        // Male was rewritten.
        let male_idx = find_anam_after_marker(&record, "MNAM", &interner).unwrap();
        let male_str = string_value(&record.fields[male_idx].value, &interner).unwrap();
        assert_eq!(male_str, "Actors\\MegaSloth\\Variant\\skeleton.nif");

        // Female untouched (did not end in CharacterAssets marker).
        let female_idx = find_anam_after_marker(&record, "FNAM", &interner).unwrap();
        let female_str = string_value(&record.fields[female_idx].value, &interner).unwrap();
        assert_eq!(female_str, "Actors\\Molerat\\Variant\\skeleton.nif");
    }

    #[test]
    fn ambush_candidate_preserves_parent_characterassets() {
        let mut interner = StringInterner::new();
        let extracted = make_extracted_tree("Sheepsquatch", &["Ambush_Burrow"]);
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "Sheepsquatch",
            "Actors\\Sheepsquatch\\CharacterAssets\\skeleton.nif",
            Some("Actors\\Deathclaw\\CharacterAssets\\skeleton.nif"),
            &["Ambush_Burrow"],
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(!changed);

        let male_idx = find_anam_after_marker(&record, "MNAM", &interner).unwrap();
        let male_str = string_value(&record.fields[male_idx].value, &interner).unwrap();
        assert_eq!(
            male_str,
            "Actors\\Sheepsquatch\\CharacterAssets\\skeleton.nif"
        );

        let female_idx = find_anam_after_marker(&record, "FNAM", &interner).unwrap();
        let female_str = string_value(&record.fields[female_idx].value, &interner).unwrap();
        assert_eq!(
            female_str,
            "Actors\\Deathclaw\\CharacterAssets\\skeleton.nif"
        );
    }

    #[test]
    fn ogua_preserves_parent_megasloth_skeleton() {
        let mut interner = StringInterner::new();
        let extracted = make_extracted_tree("MegaSloth", &["Ogua"]);
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif",
            Some("Actors\\MegaSloth\\CharacterAssets\\skeleton.nif"),
            &["Ogua"],
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(!changed);

        let male_idx = find_anam_after_marker(&record, "MNAM", &interner).unwrap();
        let male_str = string_value(&record.fields[male_idx].value, &interner).unwrap();
        assert_eq!(male_str, "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif");

        let female_idx = find_anam_after_marker(&record, "FNAM", &interner).unwrap();
        let female_str = string_value(&record.fields[female_idx].value, &interner).unwrap();
        assert_eq!(
            female_str,
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif"
        );
    }

    #[test]
    fn picks_first_existing_candidate() {
        let mut interner = StringInterner::new();
        // Only the second candidate (Variant) has a skeleton on disk.
        let extracted = make_extracted_tree("MegaSloth", &["Variant"]);
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif",
            None,
            &["Ghost", "Variant"],
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(changed);

        let male_idx = find_anam_after_marker(&record, "MNAM", &interner).unwrap();
        let male_str = string_value(&record.fields[male_idx].value, &interner).unwrap();
        assert_eq!(male_str, "Actors\\MegaSloth\\Variant\\skeleton.nif");
    }

    #[test]
    fn no_op_when_no_path_candidates() {
        let mut interner = StringInterner::new();
        let extracted = make_extracted_tree("MegaSloth", &["Variant"]);
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif",
            None,
            &[], // no SAPT entries
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(!changed);
    }

    #[test]
    fn preserves_parent_casing_from_existing_path() {
        let mut interner = StringInterner::new();
        let extracted = make_extracted_tree("MegaSloth", &["Variant"]);
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth", // SAPT entries use this casing
            // Male path uses a different parent casing than the SAPT tree.
            "Actors\\MEGASLOTH\\CharacterAssets\\skeleton.nif",
            None,
            &["Variant"],
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(changed);

        let male_idx = find_anam_after_marker(&record, "MNAM", &interner).unwrap();
        let male_str = string_value(&record.fields[male_idx].value, &interner).unwrap();
        // Parent casing comes from the existing male path ("MEGASLOTH"), not SAPT.
        assert_eq!(male_str, "Actors\\MEGASLOTH\\Variant\\skeleton.nif");
    }

    #[test]
    fn probes_no_meshes_prefix_layout() {
        let mut interner = StringInterner::new();
        // Create an extracted tree where skeleton.nif lives at
        // <extracted>/actors/<parent>/<sub>/skeleton.nif (no meshes/ prefix).
        let dir = tempfile::tempdir().unwrap();
        let nif_dir = dir.path().join("actors").join("MegaSloth").join("Variant");
        std::fs::create_dir_all(&nif_dir).unwrap();
        std::fs::write(nif_dir.join("skeleton.nif"), b"").unwrap();

        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif",
            None,
            &["Variant"],
            &mut interner,
        );

        let changed = rewrite_subcreature_paths(&mut record, dir.path(), &mut interner);
        assert!(changed);

        let male_idx = find_anam_after_marker(&record, "MNAM", &interner).unwrap();
        let male_str = string_value(&record.fields[male_idx].value, &interner).unwrap();
        assert_eq!(male_str, "Actors\\MegaSloth\\Variant\\skeleton.nif");
    }

    #[test]
    fn rejects_nested_path_tail() {
        let mut interner = StringInterner::new();
        let extracted = make_extracted_tree("MegaSloth", &["Variant"]);
        // Build a record manually so SAPT includes a deeper path.
        let mut record = make_race(
            0x000800,
            "Output.esp",
            "MegaSloth",
            "Actors\\MegaSloth\\CharacterAssets\\skeleton.nif",
            None,
            &[],
            &mut interner,
        );
        let sapt_sig = SubrecordSig::from_str("SAPT").unwrap();
        // Path with nested directory → must be ignored.
        record.fields.push(FieldEntry {
            sig: sapt_sig,
            value: FieldValue::String(
                interner.intern("Actors\\MegaSloth\\Animations\\Variant\\Sub"),
            ),
        });

        let changed = rewrite_subcreature_paths(&mut record, extracted.path(), &mut interner);
        assert!(!changed, "nested tail must not match a candidate");
    }
}
