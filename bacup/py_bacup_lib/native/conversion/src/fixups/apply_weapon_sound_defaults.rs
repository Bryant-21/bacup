//! Fixup: fill FO4 melee sound defaults for WEAP records missing sound fields.
//!

//!
//! # What this does
//! After the FO76→FO4 translation sweep removes packed-data FormKeys from
//! sound fields, weapons may be left with sound fields zeroed out.  This
//! fixup finds every WEAP record in the target plugin whose DNAM `sound_attack`,
//! `sound_equip_sound`, or `sound_unequip_sound` field is zero, and writes in
//! sensible FO4 melee defaults from Fallout4.esm.
//!
//! # DNAM struct layout (FO4, 40 fields, codec `I,f,f,...`)
//! The struct begins at byte 0 of the DNAM subrecord data:
//!
//! | Offset | Size | Field              | Notes                      |
//! |--------|------|--------------------|----------------------------|
//! |      0 |    4 | ammo (formid)      |                            |
//! |      4 |   36 | speed … damage_outofrange_mult (floats) |     |
//! |     40 |   12 | on_hit, skill, resist (uint32/formid) |       |
//! |     52 |    4 | flags              |                            |
//! |     56 |    2 | capacity           |                            |
//! |     58 |    1 | animation_type     |                            |
//! |     59 |    8 | damage_secondary, weight (floats) |            |
//! |     67 |    4 | value              |                            |
//! |     71 |    2 | damage_base        |                            |
//! |     73 |    4 | sound_level        |                            |
//! |     77 |    4 | **sound_attack** (formid) |                    |
//! |     81 |    4 | sound_attack_2d   |                            |
//! |     85 |    4 | sound_attack_loop |                            |
//! |     89 |    4 | sound_attack_fail |                            |
//! |     93 |    4 | sound_idle        |                            |
//! |     97 |    4 | **sound_equip_sound** (formid) |               |
//! |    101 |    4 | **sound_unequip_sound** (formid) |             |
//! |    105 |   35 | remaining fields  |                            |
//!
//! Minimum DNAM size for sound fields to be present: 105 bytes.
//!
//! # Default FormIDs (Fallout4.esm)
//! These match `_FO4_MELEE_SOUND_DEFAULTS` in the Python fixup:
//!
//! | Field            | FormID (hex) | EDID                           |
//! |------------------|-------------|--------------------------------|
//! | AttackSound      | 0x094307    | WPNSwingBaseballBat            |
//! | EquipSound       | 0x2498AE    | WPNGenericMeleeLargeEquipUp    |
//! | UnequipSound     | 0x1526AC    | WPNEquipDown                   |
//!
//! The raw FormID written into DNAM must use master-byte 0 for Fallout4.esm
//! (since every FO4 plugin has it as master 0).

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
            // In-place byte patch on DNAM. Bypasses the schema decode + encode
            // round-trip that `read_record` + `replace_record_native` would do,
            // since the only thing we need to touch is 12 bytes inside DNAM.
            // Records without a DNAM subrecord (or with a DNAM shorter than
            // DNAM_MIN_LEN) are silently skipped via the closure's `false`
            // return; "not found" handle/record errors surface as warnings.
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

/// Inject melee sound defaults into a WEAP `Record`'s DNAM bytes.
///
/// Returns `true` when the record was mutated (at least one sound field was
/// zero and has been filled in), `false` when no change was needed.
///
/// The mutation operates directly on `FieldValue::Bytes` because DNAM is a
/// large struct that the schema emits as raw bytes at this pipeline stage.
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

    fn zeroed_dnam(len: usize) -> Vec<u8> {
        vec![0u8; len]
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_no_op_when_sounds_present() {
        let mut interner = StringInterner::new();
        let mut dnam = zeroed_dnam(140);

        // Pre-fill all three sound fields with non-zero values.
        let existing_attack: u32 = 0x00_AABB01;
        let existing_equip: u32 = 0x00_AABB02;
        let existing_unequip: u32 = 0x00_AABB03;
        dnam[DNAM_SOUND_ATTACK_OFFSET..DNAM_SOUND_ATTACK_OFFSET + 4]
            .copy_from_slice(&existing_attack.to_le_bytes());
        dnam[DNAM_SOUND_EQUIP_OFFSET..DNAM_SOUND_EQUIP_OFFSET + 4]
            .copy_from_slice(&existing_equip.to_le_bytes());
        dnam[DNAM_SOUND_UNEQUIP_OFFSET..DNAM_SOUND_UNEQUIP_OFFSET + 4]
            .copy_from_slice(&existing_unequip.to_le_bytes());

        let mut record = make_weap_record_with_dnam(dnam, &mut interner);
        let changed = apply_to_record(&mut record);

        assert!(!changed, "should not mutate when sounds already set");

        // Verify originals unchanged.
        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            let a = u32::from_le_bytes(
                data[DNAM_SOUND_ATTACK_OFFSET..DNAM_SOUND_ATTACK_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(a, existing_attack);
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_injects_defaults_when_sounds_zero() {
        let mut interner = StringInterner::new();
        let dnam = zeroed_dnam(140);
        let mut record = make_weap_record_with_dnam(dnam, &mut interner);

        let changed = apply_to_record(&mut record);
        assert!(changed, "should mutate when sounds are zero");

        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            let attack = u32::from_le_bytes(
                data[DNAM_SOUND_ATTACK_OFFSET..DNAM_SOUND_ATTACK_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let equip = u32::from_le_bytes(
                data[DNAM_SOUND_EQUIP_OFFSET..DNAM_SOUND_EQUIP_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            let unequip = u32::from_le_bytes(
                data[DNAM_SOUND_UNEQUIP_OFFSET..DNAM_SOUND_UNEQUIP_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(attack, DEFAULT_ATTACK_FORM_ID);
            assert_eq!(equip, DEFAULT_EQUIP_FORM_ID);
            assert_eq!(unequip, DEFAULT_UNEQUIP_FORM_ID);
        } else {
            panic!("DNAM must be FieldValue::Bytes");
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_partial_injection() {
        let mut interner = StringInterner::new();
        let mut dnam = zeroed_dnam(140);

        // Pre-fill only attack sound.
        let existing_attack: u32 = 0x00_DEADBE;
        dnam[DNAM_SOUND_ATTACK_OFFSET..DNAM_SOUND_ATTACK_OFFSET + 4]
            .copy_from_slice(&existing_attack.to_le_bytes());

        let mut record = make_weap_record_with_dnam(dnam, &mut interner);
        let changed = apply_to_record(&mut record);
        assert!(changed, "should mutate when equip/unequip sounds are zero");

        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            // Attack should remain untouched.
            let attack = u32::from_le_bytes(
                data[DNAM_SOUND_ATTACK_OFFSET..DNAM_SOUND_ATTACK_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(attack, existing_attack);

            // Equip and unequip should have defaults.
            let equip = u32::from_le_bytes(
                data[DNAM_SOUND_EQUIP_OFFSET..DNAM_SOUND_EQUIP_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(equip, DEFAULT_EQUIP_FORM_ID);
            let unequip = u32::from_le_bytes(
                data[DNAM_SOUND_UNEQUIP_OFFSET..DNAM_SOUND_UNEQUIP_OFFSET + 4]
                    .try_into()
                    .unwrap(),
            );
            assert_eq!(unequip, DEFAULT_UNEQUIP_FORM_ID);
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_short_dnam_is_no_op() {
        let mut interner = StringInterner::new();
        let short_dnam = zeroed_dnam(50); // < DNAM_MIN_LEN
        let mut record = make_weap_record_with_dnam(short_dnam, &mut interner);
        let changed = apply_to_record(&mut record);
        assert!(!changed, "short DNAM must not be mutated");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn apply_to_record_no_dnam_is_no_op() {
        let mut interner = StringInterner::new();
        let sig = SigCode::from_str("WEAP").unwrap();
        let fk = FormKey::parse("000800@Test.esm", &mut interner).unwrap();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("TestWeap");
        let mut record = Record {
            sig,
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: edid_sig,
                value: FieldValue::String(edid_sym),
            }],
            warnings: smallvec::SmallVec::new(),
        };
        let changed = apply_to_record(&mut record);
        assert!(!changed, "record without DNAM must not be mutated");
    }

    // -----------------------------------------------------------------------
    // patch_dnam_bytes — direct byte-slice variant used by the in-place
    // helper. Mirrors the apply_to_record tests but operates on a plain
    // &mut [u8] so we exercise the same path the registry-driven fixup
    // takes for records living in the parsed plugin tree.
    // -----------------------------------------------------------------------

    #[test]
    fn patch_dnam_bytes_short_is_no_op() {
        let mut short = vec![0u8; 50]; // < DNAM_MIN_LEN
        assert!(!patch_dnam_bytes(&mut short));
    }

    #[test]
    fn patch_dnam_bytes_injects_defaults_when_zero() {
        let mut dnam = vec![0u8; 140];
        assert!(patch_dnam_bytes(&mut dnam));

        let attack = u32::from_le_bytes(
            dnam[DNAM_SOUND_ATTACK_OFFSET..DNAM_SOUND_ATTACK_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        let equip = u32::from_le_bytes(
            dnam[DNAM_SOUND_EQUIP_OFFSET..DNAM_SOUND_EQUIP_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        let unequip = u32::from_le_bytes(
            dnam[DNAM_SOUND_UNEQUIP_OFFSET..DNAM_SOUND_UNEQUIP_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        assert_eq!(attack, DEFAULT_ATTACK_FORM_ID);
        assert_eq!(equip, DEFAULT_EQUIP_FORM_ID);
        assert_eq!(unequip, DEFAULT_UNEQUIP_FORM_ID);
    }

    #[test]
    fn patch_dnam_bytes_no_op_when_sounds_present() {
        let mut dnam = vec![0u8; 140];
        // Pre-fill all sound slots so the patch returns false.
        for offset in [
            DNAM_SOUND_ATTACK_OFFSET,
            DNAM_SOUND_EQUIP_OFFSET,
            DNAM_SOUND_UNEQUIP_OFFSET,
        ] {
            dnam[offset..offset + 4].copy_from_slice(&0x00AABB01u32.to_le_bytes());
        }
        assert!(!patch_dnam_bytes(&mut dnam));
    }
}
