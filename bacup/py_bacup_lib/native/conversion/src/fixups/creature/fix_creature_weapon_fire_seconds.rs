//! Fixup: correct near-zero AnimationFireSeconds on creature ranged weapons.
//!

//!
//! # What this does
//! FO76 creature weapons don't carry AnimationFireSeconds.  The translation map
//! injects a near-zero default (1e-05) for the FO4 `FNAM.animation_fire_seconds`
//! field.  For ranged creature weapons (`animation_type == 9`, i.e. "Gun"), the
//! CK uses this value to determine attack animation length.  A near-zero value
//! causes the CK to fail animation resolution, producing a T-pose.
//!
//! Fix: when `FNAM[0]` (animation_fire_seconds) < 0.001 and
//! `DNAM[106]` (animation_attack_seconds) > 0 and `DNAM[54]` (animation_type)
//! == 9 ("Gun"), set `FNAM[0] = DNAM[animation_attack_seconds] * 0.5`.
//!
//! # DNAM layout (relevant fields)
//! | Offset | Size | Field                     |
//! |--------|------|---------------------------|
//! |     54 |    1 | animation_type (B; 9=Gun) |
//! |    106 |    4 | animation_attack_seconds (f32 LE) |
//!
//! Total minimum DNAM length: 110 bytes.
//!
//! # FNAM layout
//! | Offset | Size | Field                   |
//! |--------|------|-------------------------|
//! |      0 |    4 | animation_fire_seconds (f32 LE) |
//!
//! Minimum FNAM length: 4 bytes.

use crate::fixups::creature::{creature_internal_fixup_applies, likely_creature_weapon_editor_id};
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// DNAM / FNAM byte-level constants
// ---------------------------------------------------------------------------

/// Byte offset of `animation_type` within DNAM.
const DNAM_ANIM_TYPE_OFFSET: usize = 54;
/// Byte offset of `animation_attack_seconds` within DNAM (f32 LE).
const DNAM_ATTACK_SECONDS_OFFSET: usize = 106;
/// Minimum DNAM length to read animation_attack_seconds.
const DNAM_MIN_LEN: usize = 110;

/// `animation_type` byte value for "Gun" (ranged weapon).
const ANIM_TYPE_GUN: u8 = 9;

/// Near-zero threshold for animation_fire_seconds: values below this get fixed.
const NEAR_ZERO_THRESHOLD: f32 = 0.001;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FixCreatureWeaponFireSecondsFixup;

impl Fixup for FixCreatureWeaponFireSecondsFixup {
    fn name(&self) -> &'static str {
        "fix_creature_weapon_fire_seconds"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::CreatureGated
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        creature_internal_fixup_applies(ctx.config)
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        creature_internal_fixup_applies(config)
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let weap_sig =
            SigCode::from_str("WEAP").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();
        let weap_fks = session
            .form_keys_of_sig(weap_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in weap_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("fix_fire_secs_read:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            if config.is_whole_plugin {
                let eid_lower = resolve_eid_lower(&record, mapper.interner);
                if !likely_creature_weapon_editor_id(&eid_lower) {
                    continue;
                }
            }

            if apply_to_record(&mut record) {
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
// Record-level mutation
// ---------------------------------------------------------------------------

/// Fix near-zero `animation_fire_seconds` on Gun-type WEAP records.
///
/// Returns `true` if the FNAM was modified.
///
/// Algorithm:
/// 1. Read `DNAM` — skip if missing or too short.
/// 2. If `animation_type` != 9 (Gun) → skip.
/// 3. Read `animation_attack_seconds` from DNAM[106..110].
///    If ≤ 0.0 → skip (no usable attack timing).
/// 4. Read `FNAM[0..4]` as f32 (animation_fire_seconds).
///    If already ≥ 0.001 → skip (already reasonable).
/// 5. Set `FNAM[0..4]` = `attack_seconds * 0.5` (round to 6 decimal places).
pub fn apply_to_record(record: &mut Record) -> bool {
    let dnam_sig = match SubrecordSig::from_str("DNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let fnam_sig = match SubrecordSig::from_str("FNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };

    // ── Step 1: read DNAM ─────────────────────────────────────────────────
    let (anim_type, attack_secs) = {
        let mut found = None;
        for entry in &record.fields {
            if entry.sig != dnam_sig {
                continue;
            }
            if let FieldValue::Bytes(ref data) = entry.value {
                if data.len() >= DNAM_MIN_LEN {
                    let anim_type = data[DNAM_ANIM_TYPE_OFFSET];
                    let attack_secs = f32::from_le_bytes([
                        data[DNAM_ATTACK_SECONDS_OFFSET],
                        data[DNAM_ATTACK_SECONDS_OFFSET + 1],
                        data[DNAM_ATTACK_SECONDS_OFFSET + 2],
                        data[DNAM_ATTACK_SECONDS_OFFSET + 3],
                    ]);
                    found = Some((anim_type, attack_secs));
                }
            }
            break; // Only one DNAM.
        }
        match found {
            Some(v) => v,
            None => return false,
        }
    };

    // ── Step 2: must be a Gun ─────────────────────────────────────────────
    if anim_type != ANIM_TYPE_GUN {
        return false;
    }

    // ── Step 3: attack_secs must be positive ─────────────────────────────
    if attack_secs <= 0.0 {
        return false;
    }

    // ── Step 4 & 5: patch FNAM ──────────────────────────────────────────
    let new_fire_secs = (attack_secs * 0.5 * 1_000_000.0).round() / 1_000_000.0;

    let mut mutated = false;
    for entry in &mut record.fields {
        if entry.sig != fnam_sig {
            continue;
        }
        if let FieldValue::Bytes(ref mut data) = entry.value {
            if data.len() >= 4 {
                let current = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                if current < NEAR_ZERO_THRESHOLD {
                    let new_bytes = new_fire_secs.to_le_bytes();
                    data[0] = new_bytes[0];
                    data[1] = new_bytes[1];
                    data[2] = new_bytes[2];
                    data[3] = new_bytes[3];
                    mutated = true;
                }
            }
        }
        break; // Only one FNAM.
    }

    mutated
}

fn resolve_eid_lower(record: &Record, interner: &crate::sym::StringInterner) -> String {
    record
        .eid
        .and_then(|sym| interner.resolve(sym))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::{FixupConfig, FixupContext};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use std::sync::Arc;

    /// Build a minimal 132-byte DNAM payload with specific anim_type and
    /// animation_attack_seconds.
    fn make_dnam(anim_type: u8, attack_secs: f32) -> smallvec::SmallVec<[u8; 32]> {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        data.resize(132, 0u8);
        data[DNAM_ANIM_TYPE_OFFSET] = anim_type;
        let secs_bytes = attack_secs.to_le_bytes();
        data[DNAM_ATTACK_SECONDS_OFFSET..DNAM_ATTACK_SECONDS_OFFSET + 4]
            .copy_from_slice(&secs_bytes);
        data
    }

    /// Build a minimal FNAM payload with a given animation_fire_seconds.
    fn make_fnam(fire_secs: f32) -> smallvec::SmallVec<[u8; 32]> {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        data.resize(41, 0u8); // full FNAM size
        let bytes = fire_secs.to_le_bytes();
        data[0..4].copy_from_slice(&bytes);
        data
    }

    fn make_weap(
        local: u32,
        plugin: &str,
        anim_type: u8,
        attack_secs: f32,
        fire_secs: f32,
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("WEAP").unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        let dnam_sig = SubrecordSig::from_str("DNAM").unwrap();
        let fnam_sig = SubrecordSig::from_str("FNAM").unwrap();
        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        fields.push(FieldEntry {
            sig: dnam_sig,
            value: FieldValue::Bytes(make_dnam(anim_type, attack_secs)),
        });
        fields.push(FieldEntry {
            sig: fnam_sig,
            value: FieldValue::Bytes(make_fnam(fire_secs)),
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

    #[test]
    fn fix_fire_secs_gun_near_zero_gets_fixed() {
        let mut interner = StringInterner::new();
        // attack_secs = 1.0, fire_secs = 1e-5 (near zero)
        let mut record = make_weap(
            0x000100,
            "Output.esp",
            ANIM_TYPE_GUN,
            1.0,
            1e-5,
            &mut interner,
        );
        let changed = apply_to_record(&mut record);
        assert!(changed, "must fix near-zero fire seconds on Gun weapon");

        // New value should be 1.0 * 0.5 = 0.5
        let fnam_sig = SubrecordSig::from_str("FNAM").unwrap();
        for entry in &record.fields {
            if entry.sig == fnam_sig {
                if let FieldValue::Bytes(ref data) = entry.value {
                    let new_fire = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    assert!(
                        (new_fire - 0.5).abs() < 1e-4,
                        "expected ~0.5, got {new_fire}"
                    );
                }
                break;
            }
        }
    }

    #[test]
    fn fix_fire_secs_melee_not_touched() {
        let mut interner = StringInterner::new();
        // animation_type = 1 (melee), fire_secs near-zero
        let mut record = make_weap(0x000100, "Output.esp", 1, 1.0, 1e-5, &mut interner);
        let changed = apply_to_record(&mut record);
        assert!(!changed, "must not touch melee weapons");
    }

    #[test]
    fn fix_fire_secs_already_reasonable_not_touched() {
        let mut interner = StringInterner::new();
        // fire_secs = 0.5 (already fine)
        let mut record = make_weap(
            0x000100,
            "Output.esp",
            ANIM_TYPE_GUN,
            1.0,
            0.5,
            &mut interner,
        );
        let changed = apply_to_record(&mut record);
        assert!(!changed, "must not touch already-reasonable fire seconds");
    }

    #[test]
    fn fix_fire_secs_zero_attack_secs_not_touched() {
        let mut interner = StringInterner::new();
        // attack_secs = 0.0, fire_secs near-zero
        let mut record = make_weap(
            0x000100,
            "Output.esp",
            ANIM_TYPE_GUN,
            0.0,
            1e-5,
            &mut interner,
        );
        let changed = apply_to_record(&mut record);
        assert!(!changed, "must not fix when attack_secs is zero");
    }

    #[test]
    fn fix_fire_secs_no_dnam_is_no_op() {
        let mut interner = StringInterner::new();
        let sig = SigCode::from_str("WEAP").unwrap();
        let fk = FormKey {
            local: 0x000100,
            plugin: interner.intern("Output.esp"),
        };
        let fnam_sig = SubrecordSig::from_str("FNAM").unwrap();
        let mut record = Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: fnam_sig,
                value: FieldValue::Bytes(make_fnam(1e-5)),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        let changed = apply_to_record(&mut record);
        assert!(!changed, "no DNAM means no information → skip");
    }

    #[test]
    fn applies_to_false_for_weap_root() {
        let schema = Arc::new(AuthoringSchema::for_game("fo4").unwrap());
        let mut ctx_interner = StringInterner::new();
        let mut mapper_interner = StringInterner::new();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(!FixCreatureWeaponFireSecondsFixup.applies_to(&ctx));
        let _ = mapper;
    }
}
