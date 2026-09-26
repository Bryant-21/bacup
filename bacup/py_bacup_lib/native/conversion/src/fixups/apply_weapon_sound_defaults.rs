//! Fixup: fill FO4 melee sound defaults for WEAP records missing sound fields.
//!
//! The FO76→FO4 sweep zeroes DNAM sound FormIDs that pointed into packed data.
//! Zero `sound_attack`, `sound_equip_sound`, or `sound_unequip_sound` slots get
//! Fallout4.esm melee defaults, written with master byte 0 (Fallout4.esm is
//! master 0 of every FO4 plugin).

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// DNAM byte-level constants
// ---------------------------------------------------------------------------

/// Byte offset of `sound_attack` (first sound FormID) within DNAM data.
const DNAM_SOUND_ATTACK_OFFSET: usize = 77;
/// Byte offset of `sound_equip_sound` within DNAM data.
const DNAM_SOUND_EQUIP_OFFSET: usize = 97;
/// Byte offset of `sound_unequip_sound` within DNAM data.
const DNAM_SOUND_UNEQUIP_OFFSET: usize = 101;
/// Minimum DNAM byte length for all sound fields to be present.
const DNAM_MIN_LEN: usize = 105;

/// Raw FormID for `WPNSwingBaseballBat` (Fallout4.esm 094307).
/// Master byte 0x00 → Fallout4.esm (master index 0 of any FO4 plugin).
const DEFAULT_ATTACK_FORM_ID: u32 = 0x00_094307;
/// Raw FormID for `WPNGenericMeleeLargeEquipUp` (Fallout4.esm 2498AE).
const DEFAULT_EQUIP_FORM_ID: u32 = 0x00_2498AE;
/// Raw FormID for `WPNEquipDown` (Fallout4.esm 1526AC).
const DEFAULT_UNEQUIP_FORM_ID: u32 = 0x00_1526AC;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct ApplyWeaponSoundDefaultsFixup;

impl Fixup for ApplyWeaponSoundDefaultsFixup {
    fn name(&self) -> &'static str {
        "apply_weapon_sound_defaults"
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
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let weap_sig =
            SigCode::from_str("WEAP").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let mut report = FixupReport::empty();

        let fks = session
            .form_keys_of_sig(weap_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in fks {
            // In-place DNAM patch that skips the schema decode/encode round-trip;
            // only 12 bytes change. The kernel returns `false` for a short DNAM.
            match session.patch_subrecord_bytes(&fk, "DNAM", patch_dnam_bytes) {
                Ok(true) => report.records_changed += 1,
                Ok(false) => {}
                Err(msg) => {
                    // Missing record / missing DNAM are not fatal — record a
                    // warning and move on.
                    let w = mapper.interner.intern(&format!("weap_sound:{msg}"));
                    report.warnings.push(w);
                }
            }
        }

        Ok(report)
    }
}

/// Patch DNAM bytes in place. Returns `true` when at least one of the three
/// sound FormIDs was zero and has been replaced with the FO4 melee default.
/// Returns `false` for short DNAMs or when all sounds are already populated.
///
/// `pub(crate)`: the store2 sweep visitor calls this same kernel.
pub(crate) fn patch_dnam_bytes(data: &mut [u8]) -> bool {
    if data.len() < DNAM_MIN_LEN {
        return false;
    }
    let mut mutated = false;
    if inject_if_zero_slice(data, DNAM_SOUND_ATTACK_OFFSET, DEFAULT_ATTACK_FORM_ID) {
        mutated = true;
    }
    if inject_if_zero_slice(data, DNAM_SOUND_EQUIP_OFFSET, DEFAULT_EQUIP_FORM_ID) {
        mutated = true;
    }
    if inject_if_zero_slice(data, DNAM_SOUND_UNEQUIP_OFFSET, DEFAULT_UNEQUIP_FORM_ID) {
        mutated = true;
    }
    mutated
}

/// `inject_if_zero` variant operating on a raw byte slice (the in-place
/// helper signature) rather than the `SmallVec`-shaped one used by
/// `apply_to_record`.
fn inject_if_zero_slice(data: &mut [u8], offset: usize, form_id: u32) -> bool {
    let existing = u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    if existing != 0 {
        return false;
    }
    let bytes = form_id.to_le_bytes();
    data[offset] = bytes[0];
    data[offset + 1] = bytes[1];
    data[offset + 2] = bytes[2];
    data[offset + 3] = bytes[3];
    true
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Inject melee sound defaults into a WEAP `Record`'s DNAM bytes; returns whether
/// any sound field was filled. DNAM is still raw `FieldValue::Bytes` at this stage.
pub fn apply_to_record(record: &mut Record) -> bool {
    let dnam_sig = match SubrecordSig::from_str("DNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut mutated = false;

    for entry in record.fields.iter_mut() {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(ref mut data) = entry.value {
            if data.len() < DNAM_MIN_LEN {
                break;
            }
            if inject_if_zero(data, DNAM_SOUND_ATTACK_OFFSET, DEFAULT_ATTACK_FORM_ID) {
                mutated = true;
            }
            if inject_if_zero(data, DNAM_SOUND_EQUIP_OFFSET, DEFAULT_EQUIP_FORM_ID) {
                mutated = true;
            }
            if inject_if_zero(data, DNAM_SOUND_UNEQUIP_OFFSET, DEFAULT_UNEQUIP_FORM_ID) {
                mutated = true;
            }
        }
        // DNAM appears at most once per record — stop after first match.
        break;
    }

    mutated
}

/// Write `form_id` at `offset` inside `data` if the existing 4-byte LE value
/// there is zero. Returns `true` when a write occurred.
fn inject_if_zero(data: &mut smallvec::SmallVec<[u8; 32]>, offset: usize, form_id: u32) -> bool {
    let existing = u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]);
    if existing != 0 {
        return false;
    }
    let bytes = form_id.to_le_bytes();
    data[offset] = bytes[0];
    data[offset + 1] = bytes[1];
    data[offset + 2] = bytes[2];
    data[offset + 3] = bytes[3];
    true
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

    fn make_weap_record_with_dnam(dnam_bytes: Vec<u8>, interner: &StringInterner) -> Record {
        let sig = SigCode::from_str("WEAP").unwrap();
        let fk = FormKey::parse("000800@Test.esm", interner).unwrap();
        let dnam_sig = SubrecordSig::from_str("DNAM").unwrap();

        let mut sv: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        sv.extend_from_slice(&dnam_bytes);

        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: dnam_sig,
                value: FieldValue::Bytes(sv),
            }],
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn sounds(dnam: &[u8]) -> [u32; 3] {
        [
            DNAM_SOUND_ATTACK_OFFSET,
            DNAM_SOUND_EQUIP_OFFSET,
            DNAM_SOUND_UNEQUIP_OFFSET,
        ]
        .map(|offset| u32::from_le_bytes(dnam[offset..offset + 4].try_into().unwrap()))
    }

    const DEFAULTS: [u32; 3] = [
        DEFAULT_ATTACK_FORM_ID,
        DEFAULT_EQUIP_FORM_ID,
        DEFAULT_UNEQUIP_FORM_ID,
    ];

    #[test]
    fn patch_dnam_bytes_fills_only_zero_sound_slots() {
        for (name, preset, expected) in [
            ("all zero", [0u32, 0, 0], DEFAULTS),
            (
                "attack already set",
                [0x00_DEADBE, 0, 0],
                [0x00_DEADBE, DEFAULT_EQUIP_FORM_ID, DEFAULT_UNEQUIP_FORM_ID],
            ),
            (
                "all set",
                [0x00_AABB01, 0x00_AABB02, 0x00_AABB03],
                [0x00_AABB01, 0x00_AABB02, 0x00_AABB03],
            ),
        ] {
            let mut dnam = vec![0u8; 140];
            for (offset, value) in [
                DNAM_SOUND_ATTACK_OFFSET,
                DNAM_SOUND_EQUIP_OFFSET,
                DNAM_SOUND_UNEQUIP_OFFSET,
            ]
            .into_iter()
            .zip(preset)
            {
                dnam[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            assert_eq!(patch_dnam_bytes(&mut dnam), preset != expected, "{name}");
            assert_eq!(sounds(&dnam), expected, "{name}");
        }

        let mut short = vec![0u8; 50];
        assert!(
            !patch_dnam_bytes(&mut short),
            "DNAM shorter than DNAM_MIN_LEN"
        );
    }

    #[test]
    fn apply_to_record_patches_weapon_dnam_only() {
        let interner = StringInterner::new();
        let mut record = make_weap_record_with_dnam(vec![0u8; 140], &interner);
        assert!(apply_to_record(&mut record));
        let FieldValue::Bytes(ref data) = record.fields[0].value else {
            panic!("DNAM must be FieldValue::Bytes");
        };
        assert_eq!(sounds(data), DEFAULTS);

        let mut short = make_weap_record_with_dnam(vec![0u8; 50], &interner);
        assert!(!apply_to_record(&mut short), "short DNAM");

        let mut without_dnam = make_weap_record_with_dnam(Vec::new(), &interner);
        without_dnam.fields[0].sig = SubrecordSig::from_str("EDID").unwrap();
        assert!(!apply_to_record(&mut without_dnam), "record without DNAM");
    }
}
