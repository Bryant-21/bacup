use crate::record::{FieldValue, Record};

pub(crate) fn lower_legacy_light(record: &mut Record) -> bool {
    if record.sig.0 != *b"LIGH" {
        return false;
    }
    let mut lowered = false;
    for field in &mut record.fields {
        if field.sig.0 != *b"DATA" {
            continue;
        }
        let FieldValue::Bytes(source) = &field.value else {
            continue;
        };
        if source.len() != 32 {
            continue;
        }
        let mut target = vec![0_u8; 64];
        target[..24].copy_from_slice(&source[..24]);
        let flags = u32::from_le_bytes(source[12..16].try_into().unwrap());
        if flags & 0x200 != 0 {
            target[12..16].copy_from_slice(&((flags & !0x200) | 0x4000).to_le_bytes());
        }
        // FO4 TESObjectLIGH::InitializeData defaults. A padded zero exponent
        // bypasses Load's legacy fallback and removes radial attenuation.
        for (offset, value) in [(24, 0.001_f32), (44, 1.0), (48, 2.0), (52, 10.0)] {
            target[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        target[56..64].copy_from_slice(&source[24..32]);
        field.value = FieldValue::Bytes(target.into());
        lowered = true;
    }
    lowered
}
