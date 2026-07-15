//! Fixup: point LIGH gobos at the vanilla FO4 `_d` gobo when one exists.
//!
//! # What this does
//! FO76 light (`LIGH`) records reference a projected-light mask (gobo) in their
//! `NAM0` subrecord, e.g. `data\Textures\Effects\Gobos\HemisphereSoft_e.DDS`.
//! FO76 names these with FO76 suffixes (`_e`, `_fire`, …) and ships them as
//! sRGB textures, whereas every vanilla FO4 gobo is the `_d` variant stored as a
//! linear `BC1_UNORM` mask.
//!
//! When the FO4 base game already ships the matching `_d` gobo (verified against
//! `config.target_extracted_dir`), this fixup rewrites `NAM0` to that base-game
//! path. The light then uses FO4's own correct, linear gobo and we ship nothing.
//! Gobos with no FO4 equivalent are left untouched — the texture phase converts
//! those to a linear mask instead (`materials_native::texture_convert`).
//!
//! # FixupReport mapping
//! `records_changed` = number of LIGH records whose `NAM0` gobo was repointed.

use std::path::Path;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;
use rustc_hash::FxHashSet;

pub struct RemapLightGoboToFo4BaseFixup;

impl Fixup for RemapLightGoboToFo4BaseFixup {
    fn name(&self) -> &'static str {
        "remap_light_gobo_to_fo4_base"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.target_extracted_dir.is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();

        let Some(base_dir) = config.target_extracted_dir.as_deref() else {
            return Ok(report);
        };
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let ligh_sig =
            SigCode::from_str("LIGH").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let nam0_sig =
            SubrecordSig::from_str("NAM0").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let base_has = |rel: &str| -> bool { base_dir.join(rel_to_os_path(rel)).is_file() };

        let ligh_fks = session
            .form_keys_of_sig(ligh_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut seen_form_keys = FxHashSet::default();
        let has_duplicate_form_keys = ligh_fks
            .iter()
            .any(|form_key| !seen_form_keys.insert(*form_key));

        if has_duplicate_form_keys {
            for fk in ligh_fks {
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(record) => record,
                    Err(error) => {
                        let warning = mapper
                            .interner
                            .intern(&format!("ligh_gobo_read_err:{error}"));
                        report.warnings.push(warning);
                        continue;
                    }
                };

                if remap_record_gobo(&mut record, nam0_sig, mapper.interner, &base_has) {
                    session
                        .replace_record(record, target_schema, mapper.interner)
                        .map_err(|error| FixupError::HandleError(error.to_string()))?;
                    report.records_changed += 1;
                }
            }
            return Ok(report);
        }

        let mut changed_records = Vec::new();

        for fk in ligh_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("ligh_gobo_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            if remap_record_gobo(&mut record, nam0_sig, mapper.interner, &base_has) {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        session
            .replace_records(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_changed = expected.try_into().unwrap_or(u32::MAX);

        Ok(report)
    }
}

/// Rewrite the `NAM0` gobo string in `record` when a vanilla FO4 `_d` gobo
/// exists. Returns `true` when the record was changed.
fn remap_record_gobo(
    record: &mut Record,
    nam0_sig: SubrecordSig,
    interner: &StringInterner,
    base_has: &impl Fn(&str) -> bool,
) -> bool {
    for entry in record.fields.iter_mut() {
        if entry.sig != nam0_sig {
            continue;
        }
        let FieldValue::String(sym) = entry.value else {
            continue;
        };
        let Some(current) = interner.resolve(sym) else {
            continue;
        };
        if let Some(remapped) = fo4_base_gobo_path(current, base_has) {
            entry.value = FieldValue::String(interner.intern(&remapped));
            return true;
        }
    }
    false
}

/// Map an OS-agnostic relative path (`/`-separated) onto the host separator.
fn rel_to_os_path(rel: &str) -> std::path::PathBuf {
    rel.split('/')
        .filter(|seg| !seg.is_empty())
        .collect::<std::path::PathBuf>()
}

/// Given a gobo path as stored in `NAM0` (optionally `data\`-prefixed), return
/// the rewritten path pointing at the FO4 base-game `_d` gobo, or `None` to
/// leave it unchanged. `base_has(rel)` reports whether a `/`-separated relative
/// texture path exists in the FO4 base.
fn fo4_base_gobo_path(nam0: &str, base_has: &impl Fn(&str) -> bool) -> Option<String> {
    let nam0 = nam0.trim();
    if nam0.is_empty() {
        return None;
    }

    let sep = nam0.rfind(['\\', '/']);
    let (dir_with_sep, base) = match sep {
        Some(idx) => (&nam0[..=idx], &nam0[idx + 1..]),
        None => ("", nam0),
    };

    let dot = base.rfind('.');
    let (stem, ext) = match dot {
        Some(idx) => (&base[..idx], &base[idx..]),
        None => (base, ""),
    };

    // Already the FO4 `_d` variant — nothing to repoint.
    if stem.to_ascii_lowercase().ends_with("_d") {
        return None;
    }

    // Relative directory under the data root, `/`-separated, no `data\` prefix.
    let rel_dir = {
        let normalized = dir_with_sep.replace('\\', "/");
        let trimmed = normalized
            .strip_prefix("data/")
            .or_else(|| normalized.strip_prefix("Data/"))
            .unwrap_or(&normalized);
        trimmed.to_string()
    };

    for candidate_stem in candidate_d_stems(stem) {
        let rel = format!("{rel_dir}{candidate_stem}{ext}");
        if base_has(&rel) {
            return Some(format!("{dir_with_sep}{candidate_stem}{ext}"));
        }
    }
    None
}

/// Candidate FO4 `_d` stems for a FO76 gobo stem, most-specific first:
/// replace the trailing `_token` with `_d`, else append `_d`.
fn candidate_d_stems(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(idx) = stem.rfind('_') {
        out.push(format!("{}_d", &stem[..idx]));
    }
    let appended = format!("{stem}_d");
    if !out.contains(&appended) {
        out.push(appended);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::FormKey;
    use crate::record::FieldEntry;
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_new_native,
        plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    #[test]
    fn structural_batch_preserves_sequential_partial_subset_order() {
        let interner = StringInterner::new();
        let handle = plugin_handle_new_native("P2BGoboBatch.esp", Some("fo4")).unwrap();
        let plugin = interner.intern("P2BGoboBatch.esp");
        let ligh_sig = SigCode::from_str("LIGH").unwrap();
        let nam0_sig = SubrecordSig::from_str("NAM0").unwrap();
        let records = ["Batch_e.DDS", "Batch_d.DDS", "Batch_fire.DDS"]
            .into_iter()
            .enumerate()
            .map(|(index, filename)| {
                let mut record = Record::new(
                    ligh_sig,
                    FormKey {
                        local: 0x800 + index as u32,
                        plugin,
                    },
                );
                record.fields.push(FieldEntry {
                    sig: nam0_sig,
                    value: FieldValue::String(
                        interner.intern(&format!("data\\Textures\\Effects\\Gobos\\{filename}")),
                    ),
                });
                record
            })
            .collect();

        let schema = {
            let mut session = open_session(handle, None).unwrap();
            let schema = session.schema().unwrap();
            session
                .add_records(records, schema.as_ref(), &interner)
                .unwrap();
            schema
        };

        let base_dir = tempfile::tempdir().unwrap();
        let gobo_dir = base_dir.path().join("Textures/Effects/Gobos");
        std::fs::create_dir_all(&gobo_dir).unwrap();
        std::fs::write(gobo_dir.join("Batch_d.DDS"), []).unwrap();

        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let config = FixupConfig {
            target_extracted_dir: Some(base_dir.path().to_path_buf()),
            target_schema: Some(schema),
            ..FixupConfig::default()
        };
        let mut session = open_session(handle, None).unwrap();
        let report = RemapLightGoboToFo4BaseFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();

        assert_eq!(report.records_changed, 2);
        assert_eq!(report.records_dropped, 0);
        assert_eq!(report.records_added, 0);
        assert!(report.warnings.is_empty());

        let form_keys = session.form_keys_of_sig(ligh_sig, &interner).unwrap();
        assert_eq!(
            form_keys.iter().map(|fk| fk.local).collect::<Vec<_>>(),
            vec![0x801, 0x800, 0x802]
        );
        for fk in form_keys {
            let record = session
                .record_decoded(&fk, config.target_schema.as_deref().unwrap(), &interner)
                .unwrap();
            let remapped = record
                .fields
                .iter()
                .find(|entry| entry.sig == nam0_sig)
                .and_then(|entry| match entry.value {
                    FieldValue::String(sym) => interner.resolve(sym),
                    _ => None,
                });
            assert_eq!(
                remapped,
                Some("data\\Textures\\Effects\\Gobos\\Batch_d.DDS")
            );
        }

        let second = RemapLightGoboToFo4BaseFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(second.records_changed, 0);
    }

    #[test]
    fn duplicate_form_keys_fallback_matches_original_sequential_loop() {
        fn raw_ligh(form_id: u32, editor_id: &'static [u8], path: &'static [u8]) -> ParsedItem {
            ParsedItem::Record(ParsedRecord {
                signature: SmolStr::new_static("LIGH"),
                form_id,
                flags: 0,
                version_control: 0,
                form_version: Some(131),
                version2: None,
                subrecords: vec![
                    ParsedSubrecord {
                        signature: SmolStr::new_static("EDID"),
                        data: Bytes::from_static(editor_id),
                        semantic_type: None,
                    },
                    ParsedSubrecord {
                        signature: SmolStr::new_static("NAM0"),
                        data: Bytes::from_static(path),
                        semantic_type: None,
                    },
                ],
                raw_payload: None,
                parse_error: None,
            })
        }

        fn setup_handle(plugin_name: &str) -> u64 {
            let handle = plugin_handle_new_native(plugin_name, Some("fo4")).unwrap();
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.root_items = vec![ParsedItem::Group(ParsedGroup {
                label: *b"LIGH",
                group_type: 0,
                tail: Bytes::new(),
                children: vec![
                    raw_ligh(
                        0x800,
                        b"DuplicateFirst\0",
                        b"data\\Textures\\Effects\\Gobos\\Duplicate_e.DDS\0",
                    ),
                    raw_ligh(
                        0x800,
                        b"DuplicateSecond\0",
                        b"data\\Textures\\Effects\\Gobos\\Duplicate_fire.DDS\0",
                    ),
                ],
            })];
            slot.invalidate_sections();
            handle
        }

        fn fingerprint(handle: u64) -> Vec<(u32, Vec<u8>, Vec<u8>)> {
            let store = plugin_handle_store_ref().lock().unwrap();
            let ParsedItem::Group(group) = &store.get(&handle).unwrap().parsed.root_items[0] else {
                panic!("LIGH group expected");
            };
            group
                .children
                .iter()
                .filter_map(|item| match item {
                    ParsedItem::Record(record) => Some((
                        record.form_id,
                        record
                            .subrecords
                            .iter()
                            .find(|subrecord| subrecord.signature.as_str() == "EDID")
                            .unwrap()
                            .data
                            .to_vec(),
                        record
                            .subrecords
                            .iter()
                            .find(|subrecord| subrecord.signature.as_str() == "NAM0")
                            .unwrap()
                            .data
                            .to_vec(),
                    )),
                    ParsedItem::Group(_) => None,
                })
                .collect()
        }

        let interner = StringInterner::new();
        let reference_handle = setup_handle("P2BGoboDuplicateReference.esp");
        let fallback_handle = setup_handle("P2BGoboDuplicateFallback.esp");
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let base_dir = tempfile::tempdir().unwrap();
        let gobo_dir = base_dir.path().join("Textures/Effects/Gobos");
        std::fs::create_dir_all(&gobo_dir).unwrap();
        std::fs::write(gobo_dir.join("Duplicate_d.DDS"), []).unwrap();
        let config = FixupConfig {
            target_extracted_dir: Some(base_dir.path().to_path_buf()),
            target_schema: Some(schema.clone()),
            ..FixupConfig::default()
        };

        let reference_changed = {
            let mut session = open_session(reference_handle, None).unwrap();
            let form_keys = session
                .form_keys_of_sig(SigCode::from_str("LIGH").unwrap(), &interner)
                .unwrap();
            let nam0_sig = SubrecordSig::from_str("NAM0").unwrap();
            let base_has = |relative_path: &str| {
                relative_path.eq_ignore_ascii_case("Textures/Effects/Gobos/Duplicate_d.DDS")
            };
            let mut changed = 0;
            for form_key in form_keys {
                let mut record = session
                    .record_decoded(&form_key, &schema, &interner)
                    .unwrap();
                if remap_record_gobo(&mut record, nam0_sig, &interner, &base_has) {
                    session.replace_record(record, &schema, &interner).unwrap();
                    changed += 1;
                }
            }
            changed
        };

        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let report = {
            let mut session = open_session(fallback_handle, None).unwrap();
            RemapLightGoboToFo4BaseFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap()
        };

        assert_eq!(reference_changed, 2);
        assert_eq!(report.records_changed, 2);
        let reference = fingerprint(reference_handle);
        let fallback = fingerprint(fallback_handle);
        assert_eq!(fallback, reference);
        assert_eq!(
            fallback
                .iter()
                .map(|(form_id, editor_id, _)| (*form_id, editor_id.as_slice()))
                .collect::<Vec<_>>(),
            vec![
                (0x800, b"DuplicateFirst\0".as_slice()),
                (0x800, b"DuplicateSecond\0".as_slice()),
            ]
        );
        assert_eq!(
            fallback
                .iter()
                .map(|(_, _, path)| path.as_slice())
                .collect::<Vec<_>>(),
            vec![
                b"data\\Textures\\Effects\\Gobos\\Duplicate_d.DDS\0".as_slice(),
                b"data\\Textures\\Effects\\Gobos\\Duplicate_d.DDS\0".as_slice(),
            ]
        );
    }

    #[test]
    fn remaps_underscore_e_gobo_to_base_d() {
        let base_has = |rel: &str| rel == "Textures/Effects/Gobos/HemisphereSoft_d.DDS";
        let out = fo4_base_gobo_path(
            "data\\Textures\\Effects\\Gobos\\HemisphereSoft_e.DDS",
            &base_has,
        );
        assert_eq!(
            out.as_deref(),
            Some("data\\Textures\\Effects\\Gobos\\HemisphereSoft_d.DDS")
        );
    }

    #[test]
    fn remaps_underscore_fire_gobo_to_base_d() {
        let base_has = |rel: &str| rel == "Textures/Effects/Gobos/HemisphereSoftOmni_d.DDS";
        let out = fo4_base_gobo_path(
            "data\\Textures\\Effects\\Gobos\\HemisphereSoftOmni_fire.DDS",
            &base_has,
        );
        assert_eq!(
            out.as_deref(),
            Some("data\\Textures\\Effects\\Gobos\\HemisphereSoftOmni_d.DDS")
        );
    }

    #[test]
    fn leaves_gobo_with_no_fo4_equivalent_unchanged() {
        // FO76-only gobo: no `_d` variant in the FO4 base.
        let base_has = |_rel: &str| false;
        let out = fo4_base_gobo_path(
            "data\\Textures\\Effects\\Gobos\\church_stainedglass_gobo01.dds",
            &base_has,
        );
        assert_eq!(out, None);
    }

    #[test]
    fn leaves_existing_d_gobo_unchanged() {
        // Anything already `_d` is FO4-style — never repoint it.
        let base_has = |_rel: &str| true;
        let out = fo4_base_gobo_path(
            "data\\Textures\\Effects\\Gobos\\WorklightGobo_d.dds",
            &base_has,
        );
        assert_eq!(out, None);
    }

    #[test]
    fn preserves_path_without_data_prefix() {
        let base_has = |rel: &str| rel == "Textures/Effects/Gobos/HemisphereSoft_d.DDS";
        let out = fo4_base_gobo_path("Textures\\Effects\\Gobos\\HemisphereSoft_e.DDS", &base_has);
        assert_eq!(
            out.as_deref(),
            Some("Textures\\Effects\\Gobos\\HemisphereSoft_d.DDS")
        );
    }

    #[test]
    fn empty_gobo_is_noop() {
        let base_has = |_rel: &str| true;
        assert_eq!(fo4_base_gobo_path("", &base_has), None);
    }
}
