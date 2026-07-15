//! Fixup: inject sidecar-specified ExtraData fields into translated WEAP records.

//!
//! # What this does
//! The `weapon_extra_fks.yaml` sidecar may specify a `weap_extra_data` dict
//! for specific weapon EditorIDs (e.g. `meltdown`).  This fixup loads that
//! sidecar, then for each WEAP record in the target plugin whose EditorID
//! appears in the sidecar, injects the specified extra fields.
//!
//! Currently only `ProjectileOverride` is supported, which maps to the
//! `override_projectile` field (offset 29) in the FO4 FNAM subrecord.
//! Other `weap_extra_data` keys are logged as warnings and skipped.
//!
//! # FK remapping
//! The sidecar may specify values as FO76 FormKeys (e.g.
//! `7A316D:SeventySix.esm`).  The fixup uses the `FormKeyMapper`'s
//! `source_to_target` mapping to remap them.
//!
//! # FO4 FNAM struct layout (codec `struct:f,f,f,f,f,B,B,B,B,f,B,I,I,I`)
//! | Offset | Size | Field                       |
//! |--------|------|-----------------------------|
//! |      0 |    4 | animation_fire_seconds (f)  |
//! |      4 |    4 | rumble_left_motor_strength  |
//! |      8 |    4 | rumble_right_motor_strength |
//! |     12 |    4 | rumble_duration             |
//! |     16 |    4 | animation_reload_seconds    |
//! |     20 |    1 | bolt_anim_seconds_byte_1    |
//! |     21 |    1 | bolt_anim_seconds_byte_2    |
//! |     22 |    1 | bolt_anim_seconds_byte_3    |
//! |     23 |    1 | bolt_anim_seconds_byte_4    |
//! |     24 |    4 | sighted_transition_seconds  |
//! |     28 |    1 | projectiles (uint8)         |
//! |     29 |    4 | **override_projectile** (formid) |
//! |     33 |    4 | pattern (uint32)            |
//! |     37 |    4 | rumble_period_ms (uint32)   |
//!
//! Minimum FNAM size for override_projectile to be present: 33 bytes.
//!
//! # Data file
//! `src/embedded/weapon_extra_fks.yaml`, compiled into the native library and
//! cached lazily via `OnceLock`. Unparseable data produces an empty sidecar.

use std::sync::OnceLock;

use rustc_hash::FxHashMap;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

// ---------------------------------------------------------------------------
// FNAM byte-level constants
// ---------------------------------------------------------------------------

/// Byte offset of `override_projectile` (FormID) within FO4 FNAM data.
const FNAM_OVERRIDE_PROJ_OFFSET: usize = 29;
/// Minimum FNAM byte length for override_projectile to be present.
const FNAM_MIN_LEN: usize = 33;

// ---------------------------------------------------------------------------
// Sidecar data structures
// ---------------------------------------------------------------------------

/// Per-EditorID sidecar entry loaded from `weapon_extra_fks.yaml`.
/// Only `weap_extra_data` is used by this fixup.
#[derive(Debug, Clone, Default)]
struct WeapSidecarEntry {
    /// Field name → raw value string (may be a FormKey or a literal).
    weap_extra_data: FxHashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Sidecar loader (OnceLock-cached)
// ---------------------------------------------------------------------------

static WEAP_EXTRA_FKS: OnceLock<FxHashMap<String, WeapSidecarEntry>> = OnceLock::new();

fn weap_extra_fks() -> &'static FxHashMap<String, WeapSidecarEntry> {
    WEAP_EXTRA_FKS.get_or_init(load_weapon_extra_fks)
}

fn load_weapon_extra_fks() -> FxHashMap<String, WeapSidecarEntry> {
    let text = include_str!("../embedded/weapon_extra_fks.yaml");

    let raw: serde_json::Value = match serde_saphyr::from_str(text) {
        Ok(v) => v,
        Err(_) => return FxHashMap::default(),
    };

    let obj = match raw.as_object() {
        Some(o) => o,
        None => return FxHashMap::default(),
    };

    let mut result = FxHashMap::default();
    for (editor_id, entry_val) in obj {
        let mut sidecar = WeapSidecarEntry::default();

        if let Some(entry_obj) = entry_val.as_object() {
            if let Some(extra_obj) = entry_obj.get("weap_extra_data").and_then(|v| v.as_object()) {
                for (field, val) in extra_obj {
                    if let Some(s) = val.as_str() {
                        sidecar.weap_extra_data.insert(field.clone(), s.to_string());
                    }
                }
            }
        }

        result.insert(editor_id.clone(), sidecar);
    }

    result
}

// ---------------------------------------------------------------------------
// FK string helpers
// ---------------------------------------------------------------------------

/// Return `true` if `s` looks like a FormKey string (e.g. `7A316D:SeventySix.esm`).
fn looks_like_form_key(s: &str) -> bool {
    // Pattern: 6 hex digits, colon, plugin name ending in .es[mpl]
    if let Some((hex, plugin)) = s.split_once(':') {
        let hex = hex.trim();
        if hex.len() != 6 {
            return false;
        }
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
        let plugin_lower = plugin.to_ascii_lowercase();
        return plugin_lower.ends_with(".esm")
            || plugin_lower.ends_with(".esp")
            || plugin_lower.ends_with(".esl");
    }
    false
}

/// Parse a FormKey string `XXXXXX:Plugin.esm` into a `FormKey`.
fn parse_form_key_str(s: &str, interner: &StringInterner) -> Option<FormKey> {
    let (hex, plugin) = s.split_once(':')?;
    let local = u32::from_str_radix(hex.trim(), 16).ok()?;
    let plugin_sym = interner.intern(plugin.trim());
    Some(FormKey {
        local,
        plugin: plugin_sym,
    })
}

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct InjectWeapExtraDataFixup;

impl Fixup for InjectWeapExtraDataFixup {
    fn name(&self) -> &'static str {
        "inject_weap_extra_data"
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
        let sidecar = weap_extra_fks();
        if sidecar.is_empty() {
            return Ok(FixupReport::empty());
        }

        let weap_sig =
            SigCode::from_str("WEAP").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let target_masters = session.target_masters().to_vec();
        let mut report = FixupReport::empty();

        let fks = session
            .form_keys_of_sig(weap_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("weap_extra_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            // Look up the record's EditorID in the sidecar.
            let eid_sym = match record.eid {
                Some(sym) => sym,
                None => continue,
            };
            let eid_str = match mapper.interner.resolve(eid_sym) {
                Some(s) => s.to_string(),
                None => continue,
            };

            let entry = match sidecar.get(eid_str.as_str()) {
                Some(e) => e,
                None => continue,
            };

            if entry.weap_extra_data.is_empty() {
                continue;
            }

            // Inject extra data fields into the record.
            let changed = apply_extra_data_to_record(
                &mut record,
                &entry.weap_extra_data,
                mapper,
                &target_masters,
                &mut report,
            );

            if changed {
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
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Inject `weap_extra_data` fields into a WEAP record's FNAM bytes.
///
/// Currently supports only `ProjectileOverride` → FNAM offset 29.
/// Other field names are emitted as warnings.
///
/// Returns `true` when the record was mutated.
pub fn apply_extra_data_to_record(
    record: &mut Record,
    weap_extra_data: &FxHashMap<String, String>,
    mapper: &mut FormKeyMapper,
    target_masters: &[String],
    report: &mut FixupReport,
) -> bool {
    let fnam_sig = match SubrecordSig::from_str("FNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut mutated = false;

    for (field, raw_val) in weap_extra_data {
        match field.as_str() {
            "ProjectileOverride" => {
                // Resolve the FK: if raw_val looks like a FormKey, try to remap
                // it through the mapper (source → target).  Fall back to parsing
                // raw_val directly as the target FK if no mapping exists.
                let target_raw_id: Option<u32> = if looks_like_form_key(raw_val) {
                    resolve_proj_override_raw_id(raw_val, mapper, target_masters)
                } else {
                    None
                };

                let raw_id = match target_raw_id {
                    Some(id) => id,
                    None => {
                        // TODO: FK remapping through mapper requires
                        // the mapper to be pre-loaded with source→target
                        // mappings at fixup time.  Currently the mapper is
                        // empty at fixup time; the remapping is a no-op.
                        // When the conversion run passes pre-built mappings
                        // to fixups, revisit this path.
                        let w = mapper.interner.intern(&format!(
                            "inject_weap_extra_data: cannot remap FK {raw_val} for ProjectileOverride"
                        ));
                        report.warnings.push(w);
                        continue;
                    }
                };

                // Patch FNAM override_projectile (offset 29).
                for entry in record.fields.iter_mut() {
                    if entry.sig != fnam_sig {
                        continue;
                    }
                    if let FieldValue::Bytes(ref mut data) = entry.value {
                        if data.len() >= FNAM_MIN_LEN {
                            let bytes = raw_id.to_le_bytes();
                            data[FNAM_OVERRIDE_PROJ_OFFSET] = bytes[0];
                            data[FNAM_OVERRIDE_PROJ_OFFSET + 1] = bytes[1];
                            data[FNAM_OVERRIDE_PROJ_OFFSET + 2] = bytes[2];
                            data[FNAM_OVERRIDE_PROJ_OFFSET + 3] = bytes[3];
                            mutated = true;
                        }
                    }
                    break;
                }
            }
            other => {
                // Unknown field — warn and skip.
                let w = mapper.interner.intern(&format!(
                    "inject_weap_extra_data: unsupported weap_extra_data field '{other}'"
                ));
                report.warnings.push(w);
            }
        }
    }

    mutated
}

/// Resolve a source-game FormKey string through the mapper, returning the raw
/// 32-bit FormID to write into FNAM.
///
/// Interns the plugin name in `mapper.interner` so that the `FormKey` produced
/// is comparable to keys stored in the mapper's `source_to_target` table
/// (which also interned their plugin names via the same interner).
///
/// Returns `None` when the mapper has no pre-existing entry for the source FK
/// (i.e. does not allocate a new mapping — this function is read-only).

fn resolve_proj_override_raw_id(
    source_fk_str: &str,
    mapper: &mut FormKeyMapper,
    target_masters: &[String],
) -> Option<u32> {
    // Parse the source FK using the mapper's interner so the Sym values
    // match the ones stored in source_to_target.
    let source_fk = parse_form_key_str(source_fk_str, mapper.interner)?;

    // Read-only lookup — do not allocate a new mapping.
    let target_fk = mapper.lookup(source_fk)?;

    encode_target_form_id(target_fk, mapper.interner, target_masters)
}

fn encode_target_form_id(
    fk: FormKey,
    interner: &StringInterner,
    target_masters: &[String],
) -> Option<u32> {
    let plugin = interner.resolve(fk.plugin)?;
    let load_index = target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin))
        .unwrap_or(target_masters.len());
    if load_index > 0xFF {
        return None;
    }
    Some(((load_index as u32) << 24) | (fk.local & 0x00FF_FFFF))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn make_fnam_bytes(len: usize) -> smallvec::SmallVec<[u8; 32]> {
        let mut sv: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        sv.resize(len, 0u8);
        sv
    }

    fn make_weap_record_with_fnam(
        eid: &str,
        fnam_bytes: Vec<u8>,
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("WEAP").unwrap();
        let fk = FormKey::parse("000800@Output.esp", interner).unwrap();
        let fnam_sig = SubrecordSig::from_str("FNAM").unwrap();
        let edid_sym = interner.intern(eid);

        let mut sv: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        sv.extend_from_slice(&fnam_bytes);

        Record {
            sig,
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: fnam_sig,
                value: FieldValue::Bytes(sv),
            }],
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn make_mapper(interner: &StringInterner) -> FormKeyMapper<'_> {
        FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esp".to_string(),
                ..Default::default()
            },
            interner,
        )
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_extra_data_unknown_field_warns_no_mutation() {
        let mut interner = StringInterner::new();
        let mut mapper_interner = StringInterner::new();
        let mut mapper = make_mapper(&mut mapper_interner);

        let fnam_bytes = vec![0u8; 50];
        let mut record = make_weap_record_with_fnam("meltdown", fnam_bytes, &mut interner);

        let mut extra: FxHashMap<String, String> = FxHashMap::default();
        extra.insert("UnknownField".to_string(), "somevalue".to_string());

        let mut report = FixupReport::empty();
        let target_masters = vec!["Fallout4.esm".to_string()];
        let changed = apply_extra_data_to_record(
            &mut record,
            &extra,
            &mut mapper,
            &target_masters,
            &mut report,
        );

        assert!(!changed, "unknown field must not mutate the record");
        assert_eq!(report.warnings.len(), 1, "should emit one warning");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_extra_data_short_fnam_is_no_op() {
        let mut interner = StringInterner::new();
        let mut mapper_interner = StringInterner::new();
        let mut mapper = make_mapper(&mut mapper_interner);

        let fnam_bytes = vec![0u8; 20]; // < FNAM_MIN_LEN
        let mut record = make_weap_record_with_fnam("meltdown", fnam_bytes, &mut interner);

        // Pre-register a source→target mapping so lookup succeeds.
        let src_fk = FormKey {
            local: 0x7A316D,
            plugin: mapper.interner.intern("SeventySix.esm"),
        };
        let tgt_fk = FormKey {
            local: 0x001234,
            plugin: mapper.interner.intern("Output.esp"),
        };
        mapper.add_mapping(src_fk, tgt_fk);

        let mut extra: FxHashMap<String, String> = FxHashMap::default();
        extra.insert(
            "ProjectileOverride".to_string(),
            "7A316D:SeventySix.esm".to_string(),
        );

        let mut report = FixupReport::empty();
        let target_masters = vec!["Fallout4.esm".to_string()];
        let changed = apply_extra_data_to_record(
            &mut record,
            &extra,
            &mut mapper,
            &target_masters,
            &mut report,
        );

        assert!(!changed, "short FNAM must not be mutated");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_extra_data_project_override_patches_fnam() {
        let mut interner = StringInterner::new();
        let mut mapper_interner = StringInterner::new();
        let mut mapper = make_mapper(&mut mapper_interner);

        // Register a source→target mapping.
        let src_fk = FormKey {
            local: 0x7A316D,
            plugin: mapper.interner.intern("SeventySix.esm"),
        };
        let tgt_fk = FormKey {
            local: 0x001234,
            plugin: mapper.interner.intern("Output.esp"),
        };
        mapper.add_mapping(src_fk, tgt_fk);

        let fnam_bytes = vec![0u8; 50]; // >= FNAM_MIN_LEN
        let mut record = make_weap_record_with_fnam("meltdown", fnam_bytes, &mut interner);

        let mut extra: FxHashMap<String, String> = FxHashMap::default();
        extra.insert(
            "ProjectileOverride".to_string(),
            "7A316D:SeventySix.esm".to_string(),
        );

        let mut report = FixupReport::empty();
        let target_masters = vec!["Fallout4.esm".to_string()];
        let changed = apply_extra_data_to_record(
            &mut record,
            &extra,
            &mut mapper,
            &target_masters,
            &mut report,
        );

        assert!(
            changed,
            "ProjectileOverride with mapped FK must mutate FNAM"
        );

        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            let raw = u32::from_le_bytes([
                data[FNAM_OVERRIDE_PROJ_OFFSET],
                data[FNAM_OVERRIDE_PROJ_OFFSET + 1],
                data[FNAM_OVERRIDE_PROJ_OFFSET + 2],
                data[FNAM_OVERRIDE_PROJ_OFFSET + 3],
            ]);
            assert_eq!(raw, 0x01_001234);
        } else {
            panic!("FNAM must be FieldValue::Bytes");
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_extra_data_non_fk_value_warns() {
        let mut interner = StringInterner::new();
        let mut mapper_interner = StringInterner::new();
        let mut mapper = make_mapper(&mut mapper_interner);

        let fnam_bytes = vec![0u8; 50];
        let mut record = make_weap_record_with_fnam("meltdown", fnam_bytes, &mut interner);

        let mut extra: FxHashMap<String, String> = FxHashMap::default();
        extra.insert(
            "ProjectileOverride".to_string(),
            "not_a_formkey".to_string(),
        );

        let mut report = FixupReport::empty();
        let target_masters = vec!["Fallout4.esm".to_string()];
        let changed = apply_extra_data_to_record(
            &mut record,
            &extra,
            &mut mapper,
            &target_masters,
            &mut report,
        );

        assert!(!changed, "non-FK value must not mutate");
        assert!(!report.warnings.is_empty(), "should have a warning");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_extra_data_empty_map_is_no_op() {
        let mut interner = StringInterner::new();
        let mut mapper_interner = StringInterner::new();
        let mut mapper = make_mapper(&mut mapper_interner);

        let fnam_bytes = vec![0u8; 50];
        let mut record = make_weap_record_with_fnam("meltdown", fnam_bytes, &mut interner);

        let extra: FxHashMap<String, String> = FxHashMap::default();
        let mut report = FixupReport::empty();
        let target_masters = vec!["Fallout4.esm".to_string()];
        let changed = apply_extra_data_to_record(
            &mut record,
            &extra,
            &mut mapper,
            &target_masters,
            &mut report,
        );

        assert!(!changed);
        assert_eq!(report.warnings.len(), 0);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn looks_like_form_key_correct_format() {
        assert!(looks_like_form_key("7A316D:SeventySix.esm"));
        assert!(looks_like_form_key("000800:Fallout4.esm"));
        assert!(looks_like_form_key("ABCDEF:MyMod.esp"));
        assert!(!looks_like_form_key("not_a_formkey"));
        assert!(!looks_like_form_key("7A316D"));
        assert!(!looks_like_form_key(""));
        assert!(!looks_like_form_key("123:MyMod.esp")); // hex too short
        assert!(!looks_like_form_key("7A316D:MyMod.txt")); // wrong extension
    }
}
