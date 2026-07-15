//! Fixup: synthesize a weapon-specific InstanceNamingRules (INNR) record.
//!

//!
//! # What this does
//! When a weapon's `InstanceNaming` field vanilla-remaps to the generic
//! `dn_CommonGun` (FK `2377CF:Fallout4.esm`) the weapon gets generic workbench
//! naming.  This fixup synthesizes a `dn_<WeaponName>` INNR that:
//!
//! 1. Copies the RuleSets from vanilla FO4 `dn_CommonGun` so generic naming
//!    still works.  When the target DB is not available, falls back to no
//!    vanilla RuleSets (matching Python's degraded-mode behaviour).
//! 2. Appends one extra RuleSet with the weapon's display name keyed on the
//!    weapon's animation keyword (`AnimsXxx`).
//! 3. Rewrites the weapon record's `INRD` (Instance Naming) to point at the
//!    new INNR.
//!
//! # Guards (matching Python)
//! - Root signature is not `WEAP` → no-op.
//! - Weapon's `INRD` is not `dn_CommonGun` (FK `2377CF:Fallout4.esm`) → no-op.
//! - Weapon display name (`FULL`) absent or empty → no-op.
//!
//! # Subrecord layout (FO4 INNR)
//! - `EDID` (zstring) — Editor ID `dn_<CleanWeaponName>`.
//! - `UNAM` (uint32) — Target type.  Default raw bytes `[0x01, 0x02, 0x08, 0x20]`
//!   when no template is loaded (matches Python's `vanilla_target` fallback).
//! - One RuleSet per vanilla rule plus one extra for the weapon name:
//!     - `VNAM` (uint32) — Count of Names in this set (we emit 1).
//!     - `WNAM` (zstring) — Display name text.
//!     - `KSIZ` (uint32) — Keyword count.
//!     - `KWDA` (formid_array) — Keyword form-ids.
//!     - `YNAM` (uint16) — Index (10000 for the weapon-name rule).
//!
//! TODO: load vanilla `dn_CommonGun` RuleSets from the target records DB once
//! the DB path is wired through `FixupContext`; until then no vanilla RuleSets
//! are copied.

use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::session::PluginSession;
use crate::sym::StringInterner;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// FO4 FormKey for `dn_CommonGun` — the generic weapon INNR record.
/// Local object-id = 0x2377CF, plugin = `Fallout4.esm`.
const DN_COMMON_GUN_LOCAL: u32 = 0x0023_77CF;
const FALLOUT4_ESM: &str = "Fallout4.esm";

/// Default UNAM Target payload when no vanilla template is loaded.
/// Mirrors Python's `vanilla_target = [0x1, 0x2, 0x8, 0x20]` fallback.
const DEFAULT_TARGET_BYTES: [u8; 4] = [0x01, 0x02, 0x08, 0x20];

/// Index value written into the weapon-name RuleSet's YNAM subrecord.
const WEAPON_NAME_YNAM_INDEX: u16 = 10000;

/// Synthetic plugin name used to construct a unique source FormKey for the
/// new INNR record.  Combined with the weapon's local id, this gives every
/// synthesized INNR a distinct source-side key for the mapper.
const SYNTHETIC_INNR_PLUGIN: &str = "INNR_WEAPON";

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct SynthesizeWeaponInnrFixup;

impl Fixup for SynthesizeWeaponInnrFixup {
    fn name(&self) -> &'static str {
        "synthesize_weapon_innr"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::GraphOnly
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        match ctx.config.root_sig {
            Some(sig) => SigCode::from_str("WEAP")
                .map(|weap| sig == weap)
                .unwrap_or(false),
            None => false,
        }
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        match config.root_sig {
            Some(sig) => SigCode::from_str("WEAP")
                .map(|weap| sig == weap)
                .unwrap_or(false),
            None => false,
        }
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let weap_sig =
            SigCode::from_str("WEAP").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let innr_sig =
            SigCode::from_str("INNR").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let inrd_sig =
            SubrecordSig::from_str("INRD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let full_sig =
            SubrecordSig::from_str("FULL").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let kwda_sig =
            SubrecordSig::from_str("KWDA").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let mut report = FixupReport::empty();
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let dn_common_gun_fk = FormKey {
            local: DN_COMMON_GUN_LOCAL,
            plugin: mapper.interner.intern(FALLOUT4_ESM),
        };

        let weap_fks = session
            .form_keys_of_sig(weap_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for weap_fk in &weap_fks {
            let mut weapon =
                match session.record_decoded(weap_fk, target_schema.as_ref(), mapper.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("synthesize_weapon_innr:read_err:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };

            // Guard: weapon's INRD must point at dn_CommonGun.
            if !field_matches_formkey(&weapon, inrd_sig, &dn_common_gun_fk) {
                continue;
            }

            // Guard: weapon must have a non-empty FULL.
            let weapon_display_name = match read_full_text(&weapon, full_sig, mapper.interner) {
                Some(name) if !name.is_empty() => name,
                _ => {
                    let w = mapper
                        .interner
                        .intern("synthesize_weapon_innr:no_display_name");
                    report.warnings.push(w);
                    continue;
                }
            };

            // EditorID for the new INNR: dn_<clean_weapon_eid>.
            let weapon_eid = weapon
                .eid
                .and_then(|sym| mapper.interner.resolve(sym))
                .map(|s| s.to_string())
                .unwrap_or_else(|| "Unknown".to_string());
            let innr_eid = format!("dn_{}", clean_editor_id(&weapon_eid));

            // Detect a weapon-specific animation keyword (EditorID starts with
            // "Anims").  Resolve each KWDA entry's FK to its target EID via
            // read_record on the target plugin.
            let anim_keyword_fk = find_anim_keyword(
                session,
                &weapon,
                kwda_sig,
                target_schema.as_ref(),
                mapper.interner,
            );

            // Allocate a new target FormKey for the synthesized INNR.  Use a
            // synthetic source key derived from the weapon to keep it unique
            // across multiple weapons in the same run.  Pass eid=None so the
            // mapper skips vanilla-remap (the synthesized name is unique).
            let synth_source_plugin = mapper.interner.intern(SYNTHETIC_INNR_PLUGIN);
            let synth_source_fk = FormKey {
                local: weapon.form_key.local,
                plugin: synth_source_plugin,
            };
            let innr_target_fk = mapper.allocate_or_resolve(synth_source_fk, None, innr_sig);

            // Build and insert the INNR record.
            let innr_record = build_innr_record(
                innr_sig,
                innr_target_fk,
                &innr_eid,
                &weapon_display_name,
                anim_keyword_fk,
                mapper.interner,
            );

            if let Err(e) = session.add_record(innr_record, target_schema.as_ref(), mapper.interner)
            {
                let w = mapper
                    .interner
                    .intern(&format!("synthesize_weapon_innr:add_err:{e}"));
                report.warnings.push(w);
                continue;
            }
            report.records_added += 1;

            // Rewrite the weapon's INRD to point at the new INNR.
            let rewrote = rewrite_inrd_field(&mut weapon, inrd_sig, innr_target_fk);
            if rewrote {
                if let Err(e) =
                    session.replace_record(weapon, target_schema.as_ref(), mapper.interner)
                {
                    let w = mapper
                        .interner
                        .intern(&format!("synthesize_weapon_innr:replace_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
                report.records_changed += 1;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level helpers
// ---------------------------------------------------------------------------

/// Return `true` when `record` has a subrecord with `sig` whose decoded value
/// is a `FormKey` equal to `target`.
pub(crate) fn field_matches_formkey(record: &Record, sig: SubrecordSig, target: &FormKey) -> bool {
    for entry in &record.fields {
        if entry.sig != sig {
            continue;
        }
        if let FieldValue::FormKey(ref fk) = entry.value {
            return fk == target;
        }
    }
    false
}

/// Read the FULL display name from a record.
///
/// FO4 `FULL` is codec `lstring`.  In unlocalized plugins it decodes to
/// `FieldValue::String`; in localized plugins it decodes to a 4-byte
/// `FieldValue::Uint` (a string-table id).  This helper returns `Some(text)`
/// only for the unlocalized case — when the value is a string-table id we
/// cannot resolve it without loading the .STRINGS files, so the fixup is
/// degraded to a no-op for that weapon.
pub(crate) fn read_full_text(
    record: &Record,
    full_sig: SubrecordSig,
    interner: &StringInterner,
) -> Option<String> {
    for entry in &record.fields {
        if entry.sig != full_sig {
            continue;
        }
        return match &entry.value {
            FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_string()),
            FieldValue::Bytes(bytes) => {
                // Decode as null-terminated zstring (FULL was non-localized
                // but came back as raw bytes for some codec reason).
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                std::str::from_utf8(&bytes[..end])
                    .ok()
                    .map(|s| s.to_string())
            }
            _ => None,
        };
    }
    None
}

/// Strip FO76 EditorID suffixes/prefixes and collapse non-alphanumerics so the
/// resulting string is safe to splice into a `dn_<...>` EditorID.
///
/// Mirrors the Python algorithm:
///   1. Replace `_NONPLAYABLE` and `zzz_` with empty.
///   2. Keep only ASCII alphanumerics and underscores.
///   3. Collapse runs of underscores to one; strip leading/trailing underscores.
pub(crate) fn clean_editor_id(raw: &str) -> String {
    let mut intermediate = raw.replace("_NONPLAYABLE", "");
    intermediate = intermediate.replace("zzz_", "");
    let filtered: String = intermediate
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let mut out = String::with_capacity(filtered.len());
    let mut last_underscore = false;
    for c in filtered.chars() {
        if c == '_' {
            if !last_underscore {
                out.push(c);
            }
            last_underscore = true;
        } else {
            out.push(c);
            last_underscore = false;
        }
    }
    out.trim_matches('_').to_string()
}

/// Scan a weapon's KWDA subrecord (raw 4-byte formids per entry) for a keyword
/// whose target EditorID starts with `"Anims"`.  Returns the first match.
///
/// FO4 KWDA codec is `formid_array`, which `read_record` currently passes
/// through as raw bytes.  Each 4-byte entry is `(master_index << 24) |
/// object_id` from the weapon's plugin master list.  To resolve the EID we
/// re-read the target keyword record via `read_record`.
fn find_anim_keyword(
    session: &mut PluginSession,
    weapon: &Record,
    kwda_sig: SubrecordSig,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> Option<FormKey> {
    // Collect each KWDA FK into a vec first, then resolve, to avoid borrow
    // conflict between weapon.fields (immutable) and interner (mutable).
    let mut kwda_fks: Vec<FormKey> = Vec::new();
    for entry in &weapon.fields {
        if entry.sig != kwda_sig {
            continue;
        }
        match &entry.value {
            FieldValue::FormKey(fk) => kwda_fks.push(*fk),
            FieldValue::List(items) => {
                for v in items {
                    if let FieldValue::FormKey(fk) = v {
                        kwda_fks.push(*fk);
                    }
                }
            }
            FieldValue::Bytes(_) => {
                // Raw passthrough: skip — the formid_array codec is not
                // structurally decoded by read_record yet.  Without the
                // weapon's master list at this layer, we can't synthesize
                // valid FKs from the raw bytes.
            }
            _ => {}
        }
    }

    for fk in kwda_fks {
        let kw_record = match session.record_decoded(&fk, target_schema, interner) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if let Some(sym) = kw_record.eid {
            if let Some(eid_str) = interner.resolve(sym) {
                if eid_str.starts_with("Anims") {
                    return Some(fk);
                }
            }
        }
    }
    None
}

/// Build a fresh INNR `Record` with the given form key, editor id, weapon
/// display name, and optional animation keyword.
///
/// Produces one RuleSet (VNAM=1, WNAM, KSIZ, KWDA, YNAM=10000) for the
/// weapon name.  Vanilla `dn_CommonGun` RuleSets are not yet copied (see
/// module-level TODO).
pub(crate) fn build_innr_record(
    innr_sig: SigCode,
    innr_fk: FormKey,
    innr_eid: &str,
    weapon_display_name: &str,
    anim_keyword_fk: Option<FormKey>,
    interner: &StringInterner,
) -> Record {
    let edid_sig = SubrecordSig::from_str("EDID").expect("EDID is a valid sig");
    let unam_sig = SubrecordSig::from_str("UNAM").expect("UNAM is a valid sig");
    let vnam_sig = SubrecordSig::from_str("VNAM").expect("VNAM is a valid sig");
    let wnam_sig = SubrecordSig::from_str("WNAM").expect("WNAM is a valid sig");
    let ksiz_sig = SubrecordSig::from_str("KSIZ").expect("KSIZ is a valid sig");
    let kwda_sig = SubrecordSig::from_str("KWDA").expect("KWDA is a valid sig");
    let ynam_sig = SubrecordSig::from_str("YNAM").expect("YNAM is a valid sig");

    let eid_sym = interner.intern(innr_eid);
    let wnam_sym = interner.intern(weapon_display_name);

    let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

    // EDID — zstring.
    fields.push(FieldEntry {
        sig: edid_sig,
        value: FieldValue::String(eid_sym),
    });

    // UNAM — Target. Emit raw 4-byte default; encoder passes bytes through
    // unchanged (matches Python's `vanilla_target` fallback).
    let unam_bytes: smallvec::SmallVec<[u8; 32]> = DEFAULT_TARGET_BYTES.iter().copied().collect();
    fields.push(FieldEntry {
        sig: unam_sig,
        value: FieldValue::Bytes(unam_bytes),
    });

    // VNAM — Names count for the weapon-name RuleSet.  Emit raw 4 bytes for
    // uint32 = 1 to avoid relying on a schema codec lookup at encode time
    // (the encoder's default for FieldValue::Uint is int32, which is fine
    // here, but explicit bytes are cheaper and guaranteed correct).
    let vnam_bytes: smallvec::SmallVec<[u8; 32]> = 1u32.to_le_bytes().into_iter().collect();
    fields.push(FieldEntry {
        sig: vnam_sig,
        value: FieldValue::Bytes(vnam_bytes),
    });

    // WNAM — Text. Emit as zstring (unlocalized).
    fields.push(FieldEntry {
        sig: wnam_sig,
        value: FieldValue::String(wnam_sym),
    });

    // KSIZ — Keyword count.
    let kw_count: u32 = if anim_keyword_fk.is_some() { 1 } else { 0 };
    let ksiz_bytes: smallvec::SmallVec<[u8; 32]> = kw_count.to_le_bytes().into_iter().collect();
    fields.push(FieldEntry {
        sig: ksiz_sig,
        value: FieldValue::Bytes(ksiz_bytes),
    });

    // KWDA — Keyword form-ids.  Always emit the subrecord even when empty
    // (matches the schema-required marker semantics).
    let mut kwda_bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();

    if let Some(kw_fk) = anim_keyword_fk {
        let kwda_value = FieldValue::List(vec![FieldValue::FormKey(kw_fk)]);
        fields.push(FieldEntry {
            sig: kwda_sig,
            value: kwda_value,
        });
    } else {
        fields.push(FieldEntry {
            sig: kwda_sig,
            value: FieldValue::Bytes(kwda_bytes.clone()),
        });
    }
    // `kwda_bytes` is otherwise unused in the keyword-present branch.
    let _ = kwda_bytes;

    // YNAM — Index (uint16). Emit raw 2 bytes.
    let ynam_bytes: smallvec::SmallVec<[u8; 32]> =
        WEAPON_NAME_YNAM_INDEX.to_le_bytes().into_iter().collect();
    fields.push(FieldEntry {
        sig: ynam_sig,
        value: FieldValue::Bytes(ynam_bytes),
    });

    Record {
        sig: innr_sig,
        form_key: innr_fk,
        eid: Some(eid_sym),
        flags: RecordFlags::empty(),
        fields,
        warnings: smallvec::SmallVec::new(),
    }
}

/// Rewrite the weapon's INRD subrecord to point at `new_fk`.
///
/// Returns `true` when an INRD subrecord was found and updated.
pub(crate) fn rewrite_inrd_field(
    weapon: &mut Record,
    inrd_sig: SubrecordSig,
    new_fk: FormKey,
) -> bool {
    for entry in weapon.fields.iter_mut() {
        if entry.sig != inrd_sig {
            continue;
        }
        entry.value = FieldValue::FormKey(new_fk);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::{FixupConfig, FixupContext};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
    use std::sync::Arc;

    fn fo4_schema() -> Arc<AuthoringSchema> {
        AuthoringSchema::for_game("fo4").expect("fo4 schema")
    }

    fn new_target_handle(name: &str) -> Option<u64> {
        plugin_handle_new_native(name, Some("fo4")).ok()
    }

    fn make_test_ctx<'a>(
        schema: &'a Arc<AuthoringSchema>,
        config: &'a FixupConfig,
    ) -> FixupContext<'a> {
        FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: schema,
            schema_source: schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config,
        }
    }

    /// smoke: registered fixup applies to WEAP root.
    #[test]
    fn applies_to_weap_root() {
        let schema = fo4_schema();
        let weap_sig = SigCode::from_str("WEAP").expect("WEAP sig");
        let config = FixupConfig {
            root_sig: Some(weap_sig),
            ..Default::default()
        };
        let ctx = make_test_ctx(&schema, &config);
        let fixup = SynthesizeWeaponInnrFixup;
        assert!(fixup.applies_to(&ctx), "should apply to WEAP root");
    }

    /// does not apply to NPC_ roots.
    #[test]
    fn does_not_apply_to_npc_root() {
        let schema = fo4_schema();
        let npc_sig = SigCode::from_str("NPC_").expect("NPC_ sig");
        let config = FixupConfig {
            root_sig: Some(npc_sig),
            ..Default::default()
        };
        let ctx = make_test_ctx(&schema, &config);
        let fixup = SynthesizeWeaponInnrFixup;
        assert!(!fixup.applies_to(&ctx));
    }

    /// does not apply when root_sig is None.
    #[test]
    fn does_not_apply_when_no_root_sig() {
        let schema = fo4_schema();
        let config = FixupConfig {
            root_sig: None,
            ..Default::default()
        };
        let ctx = make_test_ctx(&schema, &config);
        let fixup = SynthesizeWeaponInnrFixup;
        assert!(!fixup.applies_to(&ctx));
    }

    /// clean_editor_id strips FO76 prefixes/suffixes.
    #[test]
    fn clean_editor_id_strips_known_affixes() {
        assert_eq!(clean_editor_id("zzz_TestWeapon_NONPLAYABLE"), "TestWeapon");
        assert_eq!(clean_editor_id("PlainGun"), "PlainGun");
        assert_eq!(clean_editor_id("__double___"), "double");
        assert_eq!(clean_editor_id(""), "");
        // Non-alphanumeric / non-underscore chars are dropped.
        assert_eq!(clean_editor_id("My-Gun.42"), "MyGun42");
    }

    /// field_matches_formkey returns true only for the exact FK.
    #[test]
    fn field_matches_formkey_exact_match() {
        let mut interner = StringInterner::new();
        let inrd_sig = SubrecordSig::from_str("INRD").unwrap();
        let plugin_sym = interner.intern("Fallout4.esm");
        let target = FormKey {
            local: DN_COMMON_GUN_LOCAL,
            plugin: plugin_sym,
        };
        let other = FormKey {
            local: 0x1234,
            plugin: plugin_sym,
        };

        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let weap_plugin = interner.intern("Out.esp");
        let weap_fk = FormKey {
            local: 0x800,
            plugin: weap_plugin,
        };

        let mut record = Record::new(weap_sig, weap_fk);
        record.fields.push(FieldEntry {
            sig: inrd_sig,
            value: FieldValue::FormKey(target),
        });
        assert!(field_matches_formkey(&record, inrd_sig, &target));
        assert!(!field_matches_formkey(&record, inrd_sig, &other));
    }

    /// field_matches_formkey returns false when subrecord absent.
    #[test]
    fn field_matches_formkey_missing_subrecord() {
        let mut interner = StringInterner::new();
        let inrd_sig = SubrecordSig::from_str("INRD").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let plugin_sym = interner.intern("Out.esp");
        let weap_fk = FormKey {
            local: 0x800,
            plugin: plugin_sym,
        };
        let target = FormKey {
            local: DN_COMMON_GUN_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        };
        let record = Record::new(weap_sig, weap_fk);
        assert!(!field_matches_formkey(&record, inrd_sig, &target));
    }

    /// read_full_text returns the interned string for FieldValue::String.
    #[test]
    fn read_full_text_unlocalized_string() {
        let mut interner = StringInterner::new();
        let full_sig = SubrecordSig::from_str("FULL").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let name_sym = interner.intern("My Test Gun");
        let weap_fk = FormKey {
            local: 0x800,
            plugin: interner.intern("Out.esp"),
        };
        let mut record = Record::new(weap_sig, weap_fk);
        record.fields.push(FieldEntry {
            sig: full_sig,
            value: FieldValue::String(name_sym),
        });
        let text = read_full_text(&record, full_sig, &interner);
        assert_eq!(text.as_deref(), Some("My Test Gun"));
    }

    /// read_full_text returns None when FULL decodes as a localized id.
    #[test]
    fn read_full_text_localized_uint_returns_none() {
        let mut interner = StringInterner::new();
        let full_sig = SubrecordSig::from_str("FULL").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let weap_fk = FormKey {
            local: 0x800,
            plugin: interner.intern("Out.esp"),
        };
        let mut record = Record::new(weap_sig, weap_fk);
        record.fields.push(FieldEntry {
            sig: full_sig,
            value: FieldValue::Uint(42),
        });
        assert!(read_full_text(&record, full_sig, &interner).is_none());
    }

    /// read_full_text decodes raw zstring bytes when FULL is bytes.
    #[test]
    fn read_full_text_bytes_decoded_as_zstring() {
        let mut interner = StringInterner::new();
        let full_sig = SubrecordSig::from_str("FULL").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let weap_fk = FormKey {
            local: 0x800,
            plugin: interner.intern("Out.esp"),
        };
        let mut record = Record::new(weap_sig, weap_fk);
        let bytes: smallvec::SmallVec<[u8; 32]> = b"Pipe Gun\0".iter().copied().collect();
        record.fields.push(FieldEntry {
            sig: full_sig,
            value: FieldValue::Bytes(bytes),
        });
        let text = read_full_text(&record, full_sig, &interner);
        assert_eq!(text.as_deref(), Some("Pipe Gun"));
    }

    /// rewrite_inrd_field updates the INRD FormKey and reports true.
    #[test]
    fn rewrite_inrd_field_updates_subrecord() {
        let mut interner = StringInterner::new();
        let inrd_sig = SubrecordSig::from_str("INRD").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let plugin_sym = interner.intern("Out.esp");
        let weap_fk = FormKey {
            local: 0x800,
            plugin: plugin_sym,
        };
        let old_fk = FormKey {
            local: DN_COMMON_GUN_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        };
        let new_fk = FormKey {
            local: 0x900,
            plugin: plugin_sym,
        };

        let mut record = Record::new(weap_sig, weap_fk);
        record.fields.push(FieldEntry {
            sig: inrd_sig,
            value: FieldValue::FormKey(old_fk),
        });

        assert!(rewrite_inrd_field(&mut record, inrd_sig, new_fk));
        match &record.fields[0].value {
            FieldValue::FormKey(fk) => assert_eq!(*fk, new_fk),
            other => panic!("expected FormKey, got {other:?}"),
        }
    }

    /// rewrite_inrd_field returns false when INRD absent.
    #[test]
    fn rewrite_inrd_field_no_subrecord_returns_false() {
        let mut interner = StringInterner::new();
        let inrd_sig = SubrecordSig::from_str("INRD").unwrap();
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let weap_fk = FormKey {
            local: 0x800,
            plugin: interner.intern("Out.esp"),
        };
        let new_fk = FormKey {
            local: 0x900,
            plugin: interner.intern("Out.esp"),
        };
        let mut record = Record::new(weap_sig, weap_fk);
        assert!(!rewrite_inrd_field(&mut record, inrd_sig, new_fk));
    }

    /// build_innr_record without an animation keyword produces
    /// EDID/UNAM/VNAM/WNAM/KSIZ(=0)/KWDA(empty)/YNAM(=10000) in order.
    #[test]
    fn build_innr_record_without_anim_keyword() {
        let mut interner = StringInterner::new();
        let innr_sig = SigCode::from_str("INNR").unwrap();
        let innr_fk = FormKey {
            local: 0x800,
            plugin: interner.intern("Out.esp"),
        };
        let record = build_innr_record(
            innr_sig,
            innr_fk,
            "dn_TestGun",
            "Test Gun",
            None,
            &mut interner,
        );
        assert_eq!(record.sig, innr_sig);
        assert_eq!(record.form_key, innr_fk);
        assert_eq!(
            record.eid.and_then(|s| interner.resolve(s)),
            Some("dn_TestGun")
        );
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(
            sigs,
            vec!["EDID", "UNAM", "VNAM", "WNAM", "KSIZ", "KWDA", "YNAM"]
        );

        // UNAM payload is the 4-byte default.
        if let FieldValue::Bytes(bytes) = &record.fields[1].value {
            assert_eq!(bytes.as_slice(), &DEFAULT_TARGET_BYTES);
        } else {
            panic!("UNAM should be Bytes");
        }

        // VNAM = 1.
        if let FieldValue::Bytes(bytes) = &record.fields[2].value {
            assert_eq!(bytes.as_slice(), &1u32.to_le_bytes());
        } else {
            panic!("VNAM should be Bytes");
        }

        // WNAM = interned weapon name string.
        if let FieldValue::String(sym) = record.fields[3].value {
            assert_eq!(interner.resolve(sym), Some("Test Gun"));
        } else {
            panic!("WNAM should be String");
        }

        // KSIZ = 0.
        if let FieldValue::Bytes(bytes) = &record.fields[4].value {
            assert_eq!(bytes.as_slice(), &0u32.to_le_bytes());
        } else {
            panic!("KSIZ should be Bytes");
        }

        // KWDA = empty bytes when no keyword.
        if let FieldValue::Bytes(bytes) = &record.fields[5].value {
            assert!(bytes.is_empty());
        } else {
            panic!("KWDA without keyword should be empty Bytes");
        }

        // YNAM = 10000 as little-endian uint16.
        if let FieldValue::Bytes(bytes) = &record.fields[6].value {
            assert_eq!(bytes.as_slice(), &WEAPON_NAME_YNAM_INDEX.to_le_bytes());
        } else {
            panic!("YNAM should be Bytes");
        }
    }

    /// build_innr_record with an animation keyword emits KSIZ=1
    /// and a KWDA list containing the keyword FormKey.
    #[test]
    fn build_innr_record_with_anim_keyword() {
        let mut interner = StringInterner::new();
        let innr_sig = SigCode::from_str("INNR").unwrap();
        let innr_fk = FormKey {
            local: 0x800,
            plugin: interner.intern("Out.esp"),
        };
        let anim_kw = FormKey {
            local: 0xABCDEF,
            plugin: interner.intern("Out.esp"),
        };
        let record = build_innr_record(
            innr_sig,
            innr_fk,
            "dn_TestGun",
            "Test Gun",
            Some(anim_kw),
            &mut interner,
        );

        // KSIZ should be 1.
        let ksiz = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "KSIZ")
            .expect("KSIZ present");
        if let FieldValue::Bytes(bytes) = &ksiz.value {
            assert_eq!(bytes.as_slice(), &1u32.to_le_bytes());
        } else {
            panic!("KSIZ should be Bytes");
        }

        // KWDA should be a List with one FormKey entry.
        let kwda = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "KWDA")
            .expect("KWDA present");
        if let FieldValue::List(items) = &kwda.value {
            assert_eq!(items.len(), 1);
            if let FieldValue::FormKey(fk) = items[0] {
                assert_eq!(fk, anim_kw);
            } else {
                panic!("KWDA item should be a FormKey");
            }
        } else {
            panic!("KWDA with keyword should be a List");
        }
    }

    /// fixup is a graceful no-op when the target plugin is empty.
    #[test]
    fn run_with_empty_plugin_is_no_op() {
        let handle = match new_target_handle("SynthesizeWeaponInnrEmpty.esp") {
            Some(handle) => handle,
            None => return,
        };
        let weap_sig = SigCode::from_str("WEAP").unwrap();
        let config = FixupConfig {
            root_sig: Some(weap_sig),
            ..Default::default()
        };
        let mut mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let mut session = open_session(handle, None).expect("open session");

        let fixup = SynthesizeWeaponInnrFixup;
        let report = fixup
            .run_with_session(&mut session, &mut mapper, &config)
            .expect("run succeeds");
        assert!(report.is_no_op());
    }
}
