use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
const PROJ_SOURCE_DATA_FO3_SIZE: usize = 68;
const PROJ_SOURCE_DATA_FNV_SIZE: usize = 84;
const PROJ_TARGET_DNAM_SIZE: usize = 93;
const PROJ_SHARED_FLAGS_MASK: u16 = 0x03ef;

fn read_u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn copy_four_bytes(source: &[u8], source_offset: usize, target: &mut [u8], target_offset: usize) {
    target[target_offset..target_offset + 4]
        .copy_from_slice(&source[source_offset..source_offset + 4]);
}

fn proj_type_for_fo4(source_type: u16) -> u16 {
    match source_type {
        // Missile, Lobber, Beam, and Flame have the same semantic values.
        1 | 2 | 4 | 8 => source_type,
        // FNV's Continuous Beam is represented by FO4's Beam type.
        16 => 4,
        // Unknown values, including zero, safely fall back to Missile.
        _ => 1,
    }
}

fn build_fo4_proj_dnam(source: Option<&[u8]>) -> Vec<u8> {
    let mut target = vec![0_u8; PROJ_TARGET_DNAM_SIZE];
    // A deterministic target-shape default for missing/malformed source DATA.
    target[2..4].copy_from_slice(&1_u16.to_le_bytes());

    let Some(source) = source.filter(|bytes| {
        matches!(
            bytes.len(),
            PROJ_SOURCE_DATA_FO3_SIZE | PROJ_SOURCE_DATA_FNV_SIZE
        )
    }) else {
        return target;
    };

    target[0..2].copy_from_slice(&(read_u16_at(source, 0) & PROJ_SHARED_FLAGS_MASK).to_le_bytes());
    target[2..4].copy_from_slice(&proj_type_for_fo4(read_u16_at(source, 2)).to_le_bytes());

    // Rebuild field-by-field. Source tracer chance (offset 24) and the FNV-only
    // rotations/bouncy tail (68..84) do not share target meanings and are
    // intentionally dropped. All FO4-only fields retain their safe zero defaults.
    for (source_offset, target_offset) in [
        (4, 4),   // gravity
        (8, 8),   // speed
        (12, 12), // range
        (16, 16), // light
        (20, 20), // muzzle flash light
        (28, 24), // explosion alternate-trigger proximity
        (32, 28), // explosion alternate-trigger timer
        (36, 32), // explosion
        (44, 40), // muzzle flash duration
        (48, 44), // fade duration
        (52, 48), // impact force
        (64, 60), // default weapon
    ] {
        copy_four_bytes(source, source_offset, &mut target, target_offset);
    }

    // FNV/FO3 sound slots reference legacy SOUN records, but FO4 DNAM requires
    // SNDR at target offsets 36, 52, and 56. The raw-ID rewrite remaps by object
    // id without a SOUN→SNDR semantic guarantee, and PROJ is not covered by the
    // later struct target-type validator, so these unsafe refs remain zero.

    // PairCtx exposes only the interner, so embedded raw FormIDs cannot be
    // remapped here. The always-on schema-aware
    // RewriteRawObjectTemplateFormIdsFixup later remaps PROJ.DNAM through the
    // final mapper context after these refs have landed at FO4 offsets.
    target
}

pub(super) fn relayout_proj_data(record: &mut Record) {
    let source_data = record.fields.iter().find_map(|entry| {
        if entry.sig.0 != *b"DATA" {
            return None;
        }
        match &entry.value {
            FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
            _ => None,
        }
    });
    let target_dnam = build_fo4_proj_dnam(source_data);
    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(entry.sig.0, sig if sig == *b"DATA" || sig == *b"DNAM" || sig == *b"NAM2"))
        .unwrap_or(record.fields.len());

    // Vanilla FO4's required contract is one empty DATA followed by one
    // exactly-93-byte DNAM. Source/raw DNAM and source-layout NAM2 model info
    // must never be copied into that target contract.
    record.fields.retain(|entry| {
        !matches!(entry.sig.0, sig if sig == *b"DATA" || sig == *b"DNAM" || sig == *b"NAM2")
    });
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(smallvec::SmallVec::new()),
        },
    );
    record.fields.insert(
        insert_at + 1,
        FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(target_dnam)),
        },
    );
}
