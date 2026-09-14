use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

const FO4_CITIZEN_CLASS_LOCAL: u32 = 0x0001_326B;
const FO4_MALE_EVEN_TONED_VOICE_LOCAL: u32 = 0x0001_3AD2;
const FO4_FEMALE_EVEN_TONED_VOICE_LOCAL: u32 = 0x0001_3ADD;
const FO4_MALE_CHILD_VOICE_LOCAL: u32 = 0x0001_3AE8;
const FO4_FEMALE_CHILD_VOICE_LOCAL: u32 = 0x0001_3AE9;
const FO4_HUMAN_CHILD_RACE_LOCAL: u32 = 0x0011_D83F;

const UNSUPPORTED_SOURCE_NPC_FIELDS: &[[u8; 4]] = &[
    *b"VMAD", *b"SNAM", *b"INAM", *b"TPLT", *b"TPTA", *b"SPCT", *b"SPLO", *b"PRKZ", *b"PRKR",
    *b"COCT", *b"CNTO", *b"KSIZ", *b"KWDA", *b"PKID", *b"CRIF", *b"SOFT", *b"DPLT", *b"GNAM",
    *b"CS2H", *b"CS2K", *b"CS2D", *b"CS2E", *b"CS2F", *b"CSCR", *b"PFRN", *b"ATKR",
];

pub(super) fn normalize_skyrim_npc(record: &mut Record, interner: &StringInterner) {
    if record.sig.as_str() != "NPC_" {
        return;
    }

    record
        .fields
        .retain(|field| !UNSUPPORTED_SOURCE_NPC_FIELDS.contains(&field.sig.0));

    let fallout4 = interner.intern("Fallout4.esm");
    replace_existing_form_key(record, b"CNAM", FO4_CITIZEN_CLASS_LOCAL, fallout4);

    let is_child = record.fields.iter().any(|field| {
        field.sig.0 == *b"RNAM"
            && matches!(field.value, FieldValue::FormKey(form_key) if form_key.local == FO4_HUMAN_CHILD_RACE_LOCAL && form_key.plugin == fallout4)
    });
    let is_female = npc_is_female(record, interner);
    let voice_local = match (is_child, is_female) {
        (true, true) => FO4_FEMALE_CHILD_VOICE_LOCAL,
        (true, false) => FO4_MALE_CHILD_VOICE_LOCAL,
        (false, true) => FO4_FEMALE_EVEN_TONED_VOICE_LOCAL,
        (false, false) => FO4_MALE_EVEN_TONED_VOICE_LOCAL,
    };
    replace_existing_form_key(record, b"VTCK", voice_local, fallout4);
}

fn replace_existing_form_key(
    record: &mut Record,
    signature: &[u8; 4],
    local: u32,
    plugin: crate::sym::Sym,
) {
    for field in &mut record.fields {
        if field.sig.0 == *signature {
            field.value = FieldValue::FormKey(FormKey { local, plugin });
        }
    }
}

fn npc_is_female(record: &Record, interner: &StringInterner) -> bool {
    let Some(value) = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"ACBS")
        .map(|field| &field.value)
    else {
        return false;
    };
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()) & 1 != 0
        }
        FieldValue::Struct(fields) => fields.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("Flags") && flags_include_female(value, interner)
        }),
        _ => false,
    }
}

fn flags_include_female(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Uint(flags) => *flags & 1 != 0,
        FieldValue::Int(flags) => *flags & 1 != 0,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()) & 1 != 0
        }
        FieldValue::List(values) => values.iter().any(|value| {
            matches!(value, FieldValue::String(name) if interner.resolve(*name).is_some_and(|name| name.eq_ignore_ascii_case("Female")))
        }),
        _ => false,
    }
}
