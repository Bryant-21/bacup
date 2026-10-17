use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};

fn bytes(record: &Record, sig: &[u8; 4]) -> Option<Vec<u8>> {
    record.fields.iter().find_map(|entry| {
        if entry.sig.0 == *sig
            && let FieldValue::Bytes(data) = &entry.value
        {
            return Some(data.to_vec());
        }
        None
    })
}

fn set_bytes(record: &mut Record, sig: [u8; 4], data: Vec<u8>) {
    record.fields.retain(|field| field.sig.0 != sig);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(sig),
        value: FieldValue::Bytes(data.into()),
    });
}

pub(super) fn lower_skyrim_imagespace(record: &mut Record) {
    if record.sig.0 != *b"IMGS" || bytes(record, b"DNAM").is_some_and(|data| data.len() == 24) {
        return;
    }
    let legacy = bytes(record, b"ENAM").filter(|data| data.len() == 56);
    let (source, sunlight, sky) =
        if let Some(data) = bytes(record, b"HNAM").filter(|data| data.len() == 36) {
            (data, 24, 28)
        } else if let Some(data) = legacy.as_ref() {
            (data[..28].to_vec(), 20, 24)
        } else {
            return;
        };
    // Blur radius, receive-bloom threshold, white and adaptation strength do
    // not mean FO4 tonemap E, exposure bounds and middle gray. Use the target
    // DefaultImageSpaceF4 values for those four fields.
    let mut hdr: Vec<u8> = [10.0_f32, 0.02, 0.33, 0.1, 10.0, 0.5, 1.0, 1.0, 0.18]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    for (source_offset, target_offset) in [(0, 0), (8, 8), (12, 12), (sunlight, 24), (sky, 28)] {
        hdr[target_offset..target_offset + 4]
            .copy_from_slice(&source[source_offset..source_offset + 4]);
    }
    set_bytes(record, *b"HNAM", hdr);
    if let Some(legacy) = legacy {
        if bytes(record, b"CNAM").is_none() {
            set_bytes(record, *b"CNAM", legacy[28..40].to_vec());
        }
        if bytes(record, b"TNAM").is_none() {
            set_bytes(record, *b"TNAM", legacy[40..56].to_vec());
        }
        record.fields.retain(|field| field.sig.0 != *b"ENAM");
    }
    let mut dof = vec![0; 24];
    dof[12..16].copy_from_slice(&2.0_f32.to_le_bytes());
    if let Some(source) = bytes(record, b"DNAM").filter(|data| matches!(data.len(), 12 | 16)) {
        dof[..source.len()].copy_from_slice(&source);
    }
    set_bytes(record, *b"DNAM", dof);
}
