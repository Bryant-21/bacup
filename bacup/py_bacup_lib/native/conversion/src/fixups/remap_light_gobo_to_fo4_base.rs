//! Fixup: point LIGH gobos at the vanilla FO4 `_d` gobo when one exists.
//!
//! FO76 `LIGH.NAM0` gobos (e.g. `data\Textures\Effects\Gobos\HemisphereSoft_e.DDS`)
//! use FO76 suffixes (`_e`, `_fire`, ...) and ship as sRGB; every vanilla FO4 gobo
//! is a `_d` linear `BC1_UNORM` mask. When the base game ships the matching `_d`
//! gobo (checked against `config.target_membership_root`), `NAM0` is repointed to it
//! and nothing ships. Other gobos become linear masks in the texture phase
//! (`materials_native::texture_convert`).
//!
//! `records_changed` = LIGH records whose `NAM0` gobo was repointed.

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
        config.target_membership_root.is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();

        let Some(base_dir) = config.target_membership_root.as_deref() else {
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
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

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
            target_membership_root: Some(base_dir.path().to_path_buf()),
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
    fn fo4_base_gobo_path_repoints_only_to_existing_base_d_variants() {
        let base = [
            "Textures/Effects/Gobos/HemisphereSoft_d.DDS",
            "Textures/Effects/Gobos/HemisphereSoftOmni_d.DDS",
        ];
        let in_base: &dyn Fn(&str) -> bool = &|rel| base.contains(&rel);
        let anything: &dyn Fn(&str) -> bool = &|_| true;
        let nothing: &dyn Fn(&str) -> bool = &|_| false;
        for (name, path, base_has, expected) in [
            (
                "_e to base _d",
                "data\\Textures\\Effects\\Gobos\\HemisphereSoft_e.DDS",
                in_base,
                Some("data\\Textures\\Effects\\Gobos\\HemisphereSoft_d.DDS"),
            ),
            (
                "_fire to base _d",
                "data\\Textures\\Effects\\Gobos\\HemisphereSoftOmni_fire.DDS",
                in_base,
                Some("data\\Textures\\Effects\\Gobos\\HemisphereSoftOmni_d.DDS"),
            ),
            (
                "no data prefix",
                "Textures\\Effects\\Gobos\\HemisphereSoft_e.DDS",
                in_base,
                Some("Textures\\Effects\\Gobos\\HemisphereSoft_d.DDS"),
            ),
            (
                "fo76-only gobo",
                "data\\Textures\\Effects\\Gobos\\church_stainedglass_gobo01.dds",
                nothing,
                None,
            ),
            (
                "already _d",
                "data\\Textures\\Effects\\Gobos\\WorklightGobo_d.dds",
                anything,
                None,
            ),
            ("empty", "", anything, None),
        ] {
            assert_eq!(
                fo4_base_gobo_path(path, &base_has).as_deref(),
                expected,
                "{name}"
            );
        }
    }
}
