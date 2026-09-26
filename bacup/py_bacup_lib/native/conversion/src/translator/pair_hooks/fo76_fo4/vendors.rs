use super::*;

const MEDICAL_VENDING_MACHINE_FACTION: u32 = 0x175087;
const AMMO_VENDING_MACHINE_FACTION: u32 = 0x1750A5;
const FO4_VENDING_MACHINE_VENDOR_VALUES: [u8; 12] = [0, 0, 24, 0, 0xF4, 0x01, 0, 0, 1, 1, 1, 0];
const FO4_VENDOR_NEAR_SELF: [u8; 16] = [12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const FACT_VENDOR_FLAG: u64 = 0x4000;
const VENV_BUY_SELL_EVERYTHING_NOT_IN_LIST_OFFSET: usize = 9;

impl Fo76Fo4Hook {
    /// FO76 vendors without a `VEND` buy/sell list trade everything. FO4's
    /// `Actor::CalculateSellsBuysObject` treats the missing list as an empty whitelist
    /// unless this flag is set, so the barter menu hides the whole merchant chest.
    pub(super) fn sell_everything_without_vendor_buy_sell_list(record: &mut Record) {
        if record.sig.0 != *b"FACT"
            || record.fields.iter().any(|field| field.sig.0 == *b"VEND")
            || !record.fields.iter().any(|field| {
                field.sig.0 == *b"DATA" && faction_flags(&field.value) & FACT_VENDOR_FLAG != 0
            })
        {
            return;
        }

        for field in record
            .fields
            .iter_mut()
            .filter(|field| field.sig.0 == *b"VENV")
        {
            if let FieldValue::Bytes(bytes) = &mut field.value {
                if let Some(flag) = bytes.get_mut(VENV_BUY_SELL_EVERYTHING_NOT_IN_LIST_OFFSET) {
                    *flag = 1;
                }
            }
        }
    }

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

fn faction_flags(value: &FieldValue) -> u64 {
    match value {
        FieldValue::Uint(flags) => *flags,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u64::from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        _ => 0,
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
