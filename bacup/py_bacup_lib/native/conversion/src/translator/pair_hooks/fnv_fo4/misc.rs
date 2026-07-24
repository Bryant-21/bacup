use crate::record::{FieldValue, Record};
pub(super) fn clear_legacy_bptd_ragdoll_payloads(record: &mut Record) {
    if record.sig.0 == *b"BPTD" {
        for entry in &mut record.fields {
            if entry.sig.0 == *b"NAM5" {
                // FO4 treats the first legacy model-info dwords as row counts;
                // zero-length NAM5 slots are the only safe lossless fallback.
                entry.value = FieldValue::Bytes(smallvec::SmallVec::new());
            }
        }
    }
}
