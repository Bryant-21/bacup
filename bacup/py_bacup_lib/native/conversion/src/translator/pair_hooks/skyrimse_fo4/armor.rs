use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const HUMAN_RACE_LOCAL_ID: u32 = 0x013746;

pub(super) fn normalize_skyrim_armor(record: &mut Record, interner: &StringInterner) {
    if !matches!(record.sig.as_str(), "ARMO" | "ARMA") {
        return;
    }

    replace_race_with_fo4_human(record, interner);
    if record.sig.as_str() == "ARMA" {
        for field in &mut record.fields {
            if field.sig.0 == *b"BODT" {
                field.sig = SubrecordSig(*b"BOD2");
            }
        }
    }
    normalize_biped_template(record, interner);

    if record.sig.as_str() == "ARMA" {
        record.fields.retain(|field| field.sig.0 != *b"MODL");
    }
}

fn replace_race_with_fo4_human(record: &mut Record, interner: &StringInterner) {
    let rnam = SubrecordSig(*b"RNAM");
    let insert_at = record
        .fields
        .iter()
        .position(|field| field.sig == rnam)
        .unwrap_or(record.fields.len());
    record.fields.retain(|field| field.sig != rnam);
    record.fields.insert(
        insert_at.min(record.fields.len()),
        FieldEntry {
            sig: rnam,
            value: FieldValue::FormKey(FormKey {
                local: HUMAN_RACE_LOCAL_ID,
                plugin: interner.intern("Fallout4.esm"),
            }),
        },
    );
}

fn normalize_biped_template(record: &mut Record, interner: &StringInterner) {
    let Some(template) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"BOD2")
    else {
        return;
    };
    if let FieldValue::Bytes(bytes) = &template.value
        && bytes.len() >= 4
    {
        template.value = FieldValue::Uint(u64::from(u32::from_le_bytes(
            bytes[0..4].try_into().unwrap(),
        )));
    }
    normalize_biped_value(&mut template.value, interner);
}

fn normalize_biped_value(value: &mut FieldValue, interner: &StringInterner) {
    match value {
        FieldValue::Uint(mask) => *mask = skyrim_mask_to_fo4(*mask),
        FieldValue::Int(mask) if *mask >= 0 => *mask = skyrim_mask_to_fo4(*mask as u64) as i64,
        FieldValue::List(items) => normalize_biped_tokens(items, interner),
        FieldValue::Struct(fields) => {
            for (key, nested) in fields {
                if interner.resolve(*key) == Some("FirstPersonFlags") {
                    normalize_biped_value(nested, interner);
                }
            }
        }
        _ => {}
    }
}

fn skyrim_mask_to_fo4(source_mask: u64) -> u64 {
    let mut target_mask = 0_u64;
    for source_slot in 30_u8..=61 {
        if source_mask & slot_bit(source_slot) == 0 {
            continue;
        }
        for target_slot in skyrim_slot_targets(source_slot) {
            target_mask |= slot_bit(*target_slot);
        }
    }
    target_mask
}

fn normalize_biped_tokens(items: &mut Vec<FieldValue>, interner: &StringInterner) {
    let mut target_slots = Vec::new();
    let mut passthrough = Vec::new();
    for item in items.drain(..) {
        let FieldValue::String(token) = item else {
            passthrough.push(item);
            continue;
        };
        let Some(source_slot) = interner.resolve(token).and_then(|token| {
            token
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse::<u8>()
                .ok()
        }) else {
            passthrough.push(FieldValue::String(token));
            continue;
        };
        for target_slot in skyrim_slot_targets(source_slot) {
            if !target_slots.contains(target_slot) {
                target_slots.push(*target_slot);
            }
        }
    }

    items.extend(
        target_slots
            .into_iter()
            .map(|slot| FieldValue::String(interner.intern(fo4_slot_token(slot)))),
    );
    items.extend(passthrough);
}

fn slot_bit(slot: u8) -> u64 {
    1_u64 << (slot - 30)
}

fn skyrim_slot_targets(slot: u8) -> &'static [u8] {
    match slot {
        30 => &[30],
        31 | 41 => &[31, 52],
        32 => &[33, 36],
        33 => &[34, 35],
        34 => &[37, 38],
        35 | 45 => &[50],
        36 => &[51],
        37 | 38 => &[39, 40],
        39 => &[59],
        40 | 48 | 60 | 61 => &[61],
        42 | 43 => &[46],
        44 | 55 => &[49],
        46 | 56 => &[41],
        47 => &[54],
        49 | 52 | 53 | 54 => &[44, 45],
        50 | 51 => &[53],
        57 | 58 | 59 => &[42, 43],
        _ => &[],
    }
}

fn fo4_slot_token(slot: u8) -> &'static str {
    match slot {
        30 => "30Head",
        31 => "31HairLong",
        33 => "33BODY",
        34 => "34LHand",
        35 => "35RHand",
        36 => "36UTorso",
        37 => "37ULArm",
        38 => "38URArm",
        39 => "39ULLeg",
        40 => "40URLLeg",
        41 => "41Torso",
        42 => "42LArm",
        43 => "43RArm",
        44 => "44LLeg",
        45 => "45RLeg",
        46 => "46Headband",
        49 => "49Mouth",
        50 => "50Neck",
        51 => "51Ring",
        52 => "52Scalp",
        53 => "53Decapitation",
        54 => "54Unnamed",
        59 => "59Shield",
        61 => "61FX",
        _ => unreachable!("Skyrim armor slot map emitted unsupported FO4 slot {slot}"),
    }
}
