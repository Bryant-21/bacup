use crate::ids::FormKey;
use crate::record::FieldValue;
use crate::schema::AuthoringSchema;
use crate::sym::StringInterner;
use crate::translator::pair_hook::PairCtx;

pub(crate) fn lowered_magic_condition_actor_value(
    value: &FieldValue,
    interner: &StringInterner,
) -> Option<FormKey> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    let member = |name| {
        fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some(name))
            .map(|(_, value)| value)
    };
    if !matches!(member("function"), Some(FieldValue::Uint(14 | 277 | 640 | 696))) {
        return None;
    }
    let FieldValue::FormKey(target) = member("parameter_1")? else {
        return None;
    };
    Some(*target)
}

pub(crate) fn decode_source_magic_condition(
    record_signature: &str,
    subrecord_signature: &str,
    bytes: &[u8],
    schema: &AuthoringSchema,
    ctx: &PairCtx<'_>,
) -> Option<FieldValue> {
    if !matches!(record_signature, "SPEL" | "MGEF" | "PERK")
        || subrecord_signature != "CTDA"
        || bytes.len() != 32
        || !schema
            .record_def("MGEF")?
            .subrecord_def("DATA")?
            .fields
            .iter()
            .any(|field| field.id == "magic_skill")
    {
        return None;
    }
    let uint = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let reference = |raw| {
        if raw == 0 {
            Some(FieldValue::Uint(0))
        } else {
            ctx.resolve_source_form_id(raw).map(FieldValue::FormKey)
        }
    };
    let flags = bytes[0];
    let function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    // xEdit's TES5/FO4 condition tables retain these opcodes. GetValuePercent
    // (640) changes its operand from a Skyrim enum to an FO4 AVIF reference.
    let parameter_1 = match function {
        14 | 277 | 640 | 696 => super::skyrim_actor_value_for_fo4(&FieldValue::Uint(uint(12) as u64), ctx.interner)?,
        71 | 72 | 74 | 448 | 560 | 682 | 693 | 699 | 713 => reference(uint(12))?,
        719 if flags & (0x02 | 0x08) == 0 => reference(uint(12))?,
        49 | 77 | 80 | 244 | 289 | 300 | 323 | 698 | 700 | 715 | 719 | 724 => {
            FieldValue::Uint(uint(12) as u64)
        }
        _ => return None,
    };
    let comparison = if flags & 0x04 != 0 {
        reference(uint(4))?
    } else {
        FieldValue::Float(f32::from_le_bytes(bytes[4..8].try_into().unwrap()))
    };
    let run_on = uint(20);
    let reference = if run_on == 2 {
        reference(uint(24))?
    } else {
        FieldValue::Uint(uint(24) as u64)
    };
    Some(FieldValue::Struct(
        [
            ("type", FieldValue::Uint(flags as u64)),
            ("unknown_u8_1", FieldValue::Uint(bytes[1] as u64)),
            ("unknown_u8_2", FieldValue::Uint(bytes[2] as u64)),
            ("unknown_u8_3", FieldValue::Uint(bytes[3] as u64)),
            ("comparison_value", comparison),
            ("function", FieldValue::Uint(function as u64)),
            ("unknown_u8_6", FieldValue::Uint(bytes[10] as u64)),
            ("unknown_u8_7", FieldValue::Uint(bytes[11] as u64)),
            ("parameter_1", parameter_1),
            ("parameter_2", FieldValue::Uint(uint(16) as u64)),
            ("run_on", FieldValue::Uint(run_on as u64)),
            ("reference", reference),
            (
                "parameter_3",
                FieldValue::Int(i32::from_le_bytes(bytes[28..32].try_into().unwrap()) as i64),
            ),
        ]
        .into_iter()
        .map(|(name, value)| (ctx.interner.intern(name), value))
        .collect(),
    ))
}
