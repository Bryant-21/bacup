use super::*;

const FO76_MODERN_DECAL_DATA_LEN: usize = 28;
const FO4_DECAL_DATA_LEN: usize = 36;
const FO76_ALT_NO_SUBTEXTURES: u8 = 0x80;
const FO4_SHARED_DECAL_FLAGS: u8 = 0x0F;
const FO4_NO_SUBTEXTURES: u8 = 0x08;

impl Fo76Fo4Hook {
    pub(super) fn convert_txst_decal_data_to_fo4_layout(record: &mut Record) {
        if record.sig.0 != *b"TXST" {
            return;
        }

        let Some(decal_data) = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"DODT")
        else {
            return;
        };
        let FieldValue::Bytes(source) = &mut decal_data.value else {
            return;
        };
        if source.len() != FO76_MODERN_DECAL_DATA_LEN {
            return;
        }

        let mut target = vec![0_u8; FO4_DECAL_DATA_LEN];
        target[..20].copy_from_slice(&source[..20]);
        target[20..24].copy_from_slice(&1.0_f32.to_le_bytes());
        target[24..28].copy_from_slice(&source[20..24]);
        target[28] = source[26];
        target[29] = source[24] & FO4_SHARED_DECAL_FLAGS;
        if source[24] & FO76_ALT_NO_SUBTEXTURES != 0 {
            target[29] |= FO4_NO_SUBTEXTURES;
        }
        target[30..32].copy_from_slice(&127_u16.to_le_bytes());
        target[32..35].fill(255);
        *source = SmallVec::from_vec(target);
    }
}
