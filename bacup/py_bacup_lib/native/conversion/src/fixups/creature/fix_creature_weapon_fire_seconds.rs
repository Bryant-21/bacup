//! Fixup: correct near-zero AnimationFireSeconds on creature ranged weapons.
//!
//! FO76 creature weapons carry no AnimationFireSeconds, so the translation map
//! fills FO4 `FNAM.animation_fire_seconds` (f32 at offset 0) with 1e-05. On Gun
//! weapons (`animation_type == 9`) the CK uses it as the attack animation
//! length, and a near-zero value fails animation resolution and T-poses.

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

        let mut replacements = Vec::new();
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
                replacements.push(record);
            }
        }
        report.records_changed = session
            .replace_records_contents(replacements, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Set a Gun WEAP's near-zero `animation_fire_seconds` to half its positive
/// `animation_attack_seconds`, rounded to 6 decimals. Returns `true` if FNAM
/// changed.
pub fn apply_to_record(record: &mut Record) -> bool {
    let dnam_sig = match SubrecordSig::from_str("DNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let fnam_sig = match SubrecordSig::from_str("FNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };

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

    if anim_type != ANIM_TYPE_GUN {
        return false;
    }

    if attack_secs <= 0.0 {
        return false;
    }

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
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

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

    /// Only a gun with near-zero fire seconds and a nonzero attack duration is
    /// fixed, to half the attack seconds.
    #[test]
    fn fixes_near_zero_gun_fire_seconds_only() {
        let interner = StringInterner::new();
        let fnam_sig = SubrecordSig::from_str("FNAM").unwrap();
        for (name, anim_type, attack_secs, fire_secs, expected_fire) in [
            ("gun_near_zero", ANIM_TYPE_GUN, 1.0, 1e-5, Some(0.5)),
            ("melee", 1, 1.0, 1e-5, None),
            ("already_reasonable", ANIM_TYPE_GUN, 1.0, 0.5, None),
            ("zero_attack_secs", ANIM_TYPE_GUN, 0.0, 1e-5, None),
        ] {
            let mut record = make_weap(
                0x000100,
                "Output.esp",
                anim_type,
                attack_secs,
                fire_secs,
                &interner,
            );
            assert_eq!(
                apply_to_record(&mut record),
                expected_fire.is_some(),
                "{name}"
            );
            let Some(expected_fire) = expected_fire else {
                continue;
            };
            let entry = record.fields.iter().find(|e| e.sig == fnam_sig).unwrap();
            let FieldValue::Bytes(ref data) = entry.value else {
                panic!("{name}: FNAM must stay bytes");
            };
            let new_fire = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            assert!(
                (new_fire - expected_fire).abs() < 1e-4,
                "{name}: got {new_fire}"
            );
        }

        let mut no_dnam = make_weap(0x000100, "Output.esp", ANIM_TYPE_GUN, 1.0, 1e-5, &interner);
        no_dnam
            .fields
            .retain(|e| e.sig != SubrecordSig::from_str("DNAM").unwrap());
        assert!(!apply_to_record(&mut no_dnam), "no DNAM");
    }
}
