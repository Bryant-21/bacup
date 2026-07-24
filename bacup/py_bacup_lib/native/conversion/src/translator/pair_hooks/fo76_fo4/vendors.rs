use super::*;

const MEDICAL_VENDING_MACHINE_FACTION: u32 = 0x175087;
const AMMO_VENDING_MACHINE_FACTION: u32 = 0x1750A5;
const FO4_VENDING_MACHINE_VENDOR_VALUES: [u8; 12] = [0, 0, 24, 0, 0xF4, 0x01, 0, 0, 1, 1, 1, 0];
const FO4_VENDOR_NEAR_SELF: [u8; 16] = [12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

impl Fo76Fo4Hook {
    pub(super) fn normalize_vending_machine_vendor_faction(record: &mut Record) {
        if record.sig.0 != *b"FACT"
            || !matches!(
                record.form_key.local,
                MEDICAL_VENDING_MACHINE_FACTION | AMMO_VENDING_MACHINE_FACTION
            )
        {
            return;
        }

        upsert_vendor_field(record, *b"VENV", &FO4_VENDING_MACHINE_VENDOR_VALUES);
        upsert_vendor_field(record, *b"PLVD", &FO4_VENDOR_NEAR_SELF);
    }
}

fn upsert_vendor_field(record: &mut Record, sig: [u8; 4], value: &[u8]) {
    if let Some(field) = record.fields.iter_mut().find(|field| field.sig.0 == sig) {
        field.value = FieldValue::Bytes(SmallVec::from_slice(value));
        return;
    }

    record.fields.push(FieldEntry {
        sig: SubrecordSig(sig),
        value: FieldValue::Bytes(SmallVec::from_slice(value)),
    });
}
