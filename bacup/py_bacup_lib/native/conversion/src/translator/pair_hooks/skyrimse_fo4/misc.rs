use super::SkyrimSeFo4Hook;
use crate::record::Record;

impl SkyrimSeFo4Hook {
    pub(super) fn drop_incompatible_debr_modt(record: &mut Record) {
        if record.sig.0 == *b"DEBR" {
            record.fields.retain(|entry| entry.sig.0 != *b"MODT");
        }
    }
}
