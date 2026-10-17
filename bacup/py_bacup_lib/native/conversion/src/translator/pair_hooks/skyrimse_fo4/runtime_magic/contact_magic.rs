use super::*;
use crate::schema::AuthoringSchema;
use crate::skyrimse_fo4_runtime::{magic_resources, skill_modifiers};
use crate::translator::pair_hook::PairCtx;

pub(crate) fn decode_source_contact_data(
    record_signature: &str,
    subrecord_signature: &str,
    bytes: &[u8],
    schema: &AuthoringSchema,
    ctx: &PairCtx<'_>,
) -> Option<FieldValue> {
    if (record_signature, subrecord_signature) == ("MGEF", "SNDD") {
        let definition = schema.record_def("MGEF")?.subrecord_def("DATA")?;
        if !definition
            .fields
            .iter()
            .any(|field| field.id == "magic_skill")
            || bytes.len() % 8 != 0
        {
            return None;
        }
        let rows = bytes
            .chunks_exact(8)
            .map(|row| {
                let raw = u32::from_le_bytes(row[4..8].try_into().ok()?);
                let sound = if raw == 0 {
                    FieldValue::Uint(0)
                } else {
                    FieldValue::FormKey(ctx.resolve_source_form_id(raw)?)
                };
                Some(object(
                    ctx.interner,
                    [
                        (
                            "sounds_type",
                            FieldValue::Uint(u32::from_le_bytes(row[..4].try_into().ok()?) as u64),
                        ),
                        ("sounds_sound", sound),
                    ],
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        return Some(FieldValue::List(rows));
    }
    let source_field = match (record_signature, subrecord_signature) {
        ("SPEL", "SPIT") => "delivery",
        ("MGEF", "DATA") => "magic_skill",
        _ => return None,
    };
    let definition = schema
        .record_def(record_signature)?
        .subrecord_def(subrecord_signature)?;
    if !definition
        .fields
        .iter()
        .any(|field| field.id == source_field)
    {
        return None;
    }
    let layout = schema.struct_field_layout_versioned(record_signature, subrecord_signature, None);
    if layout
        .iter()
        .map(|field| field.offset + field.width)
        .max()?
        != bytes.len()
    {
        return None;
    }
    let mut fields = Vec::with_capacity(layout.len());
    for field in layout {
        let definition = definition
            .fields
            .iter()
            .find(|item| item.id == field.field_id)?;
        let data = bytes.get(field.offset..field.offset + field.width)?;
        let value = match definition.kind.as_str() {
            "formid" => {
                let raw = u32::from_le_bytes(data.try_into().ok()?);
                if raw == 0 {
                    FieldValue::Uint(0)
                } else {
                    FieldValue::FormKey(ctx.resolve_source_form_id(raw)?)
                }
            }
            "float32" => FieldValue::Float(f32::from_le_bytes(data.try_into().ok()?)),
            "int32" => FieldValue::Int(i32::from_le_bytes(data.try_into().ok()?) as i64),
            "uint32" => FieldValue::Uint(u32::from_le_bytes(data.try_into().ok()?) as u64),
            "uint16" => FieldValue::Uint(u16::from_le_bytes(data.try_into().ok()?) as u64),
            "uint8" => FieldValue::Uint(data[0] as u64),
            _ => return None,
        };
        fields.push((ctx.interner.intern(field.field_id), value));
    }
    Some(FieldValue::Struct(fields))
}

pub(super) fn classify(record: &Record, interner: &StringInterner) -> Option<MagicSupport> {
    let signature = match record.sig.0 {
        value if value == *b"SPEL" => *b"SPIT",
        value if value == *b"MGEF" => *b"DATA",
        _ => return None,
    };
    let data = exact_named_field(record, signature)?;
    let delivery =
        named(data, "Delivery", interner).or_else(|| named(data, "TargetType", interner));
    let delivery = if delivery.is_none() {
        Some(MagicDelivery::SelfTarget)
    } else {
        parse_delivery(delivery, interner)
    };
    let Some(delivery) = delivery else {
        return Some(unsupported(MagicUnsupportedReason::CastDelivery));
    };
    Some(match signature {
        value if value == *b"SPIT" => classify_spell(record, data, delivery, interner),
        _ => classify_effect(record, data, delivery, interner),
    })
}

fn classify_spell(
    record: &Record,
    data: &[(crate::sym::Sym, FieldValue)],
    delivery: MagicDelivery,
    interner: &StringInterner,
) -> MagicSupport {
    let Some(cast) = parse_cast(named(data, "CastType", interner), interner).or_else(|| {
        named(data, "CastType", interner)
            .is_none()
            .then_some(MagicCast::ConstantEffect)
    }) else {
        return unsupported(MagicUnsupportedReason::CastDelivery);
    };
    let mut linked_records = Vec::new();
    let mut awaiting_parameters = false;
    let mut has_parameters = false;
    for field in &record.fields {
        match &field.sig.0 {
            b"EFID" => {
                if awaiting_parameters {
                    return unsupported(MagicUnsupportedReason::EffectCardinality);
                }
                let Some(form_key) = form_from_value(&field.value, interner) else {
                    return unsupported(MagicUnsupportedReason::EffectCardinality);
                };
                linked_records.push(MagicLinkedRecordRequirement {
                    role: MagicLinkedRole::BaseEffect,
                    form_key,
                    signature: SigCode(*b"MGEF"),
                });
                awaiting_parameters = true;
                has_parameters = false;
            }
            b"EFIT" => {
                if !awaiting_parameters
                    || !matches!(&field.value, FieldValue::Struct(_) | FieldValue::Bytes(_))
                    || matches!(&field.value, FieldValue::Bytes(bytes) if bytes.len() != 12)
                {
                    return unsupported(MagicUnsupportedReason::EffectCardinality);
                }
                awaiting_parameters = false;
                has_parameters = true;
            }
            b"CTDA" | b"CIS1" | b"CIS2" if !has_parameters => {
                return unsupported(MagicUnsupportedReason::EffectCardinality);
            }
            _ => {}
        }
    }
    if awaiting_parameters || linked_records.is_empty() {
        return unsupported(MagicUnsupportedReason::EffectCardinality);
    }
    let mut plan = contact_plan(record, MagicComponent::NativeSpell, cast, delivery);
    let mut target = Record::new(record.sig, record.form_key);
    push_spell_data(&mut target, cast, delivery, interner);
    let FieldValue::Struct(target_data) = &mut target.fields[0].value else {
        unreachable!()
    };
    for (name, value) in data {
        let name = interner.resolve(*name).map(normalized).unwrap_or_default();
        let target_name = match name.as_str() {
            "delivery" => "targettype",
            "halfcostperk" => "castingperk",
            _ => &name,
        };
        let value = match target_name {
            "casttype" => FieldValue::Uint(cast.target_value()),
            "targettype" => FieldValue::Uint(delivery.target_value()),
            "type" => match spell_type(value, interner) {
                Some(value) => FieldValue::Uint(value),
                None => return unsupported(MagicUnsupportedReason::CastDelivery),
            },
            "flags" => match flag_mask(value, true, interner) {
                Some(value) => FieldValue::Uint(value),
                None => return unsupported(MagicUnsupportedReason::EffectFlags),
            },
            _ => value.clone(),
        };
        replace_named(target_data, target_name, value, interner);
    }
    let mut fields = replace_data(record, *b"SPIT", target.fields[0].value.clone());
    for field in &mut fields {
        if field.sig.0 == *b"EFIT"
            && let FieldValue::Struct(values) = &field.value
        {
            field.value = object(
                interner,
                [
                    (
                        "Magnitude",
                        FieldValue::Float(
                            scalar_f32(named(values, "Magnitude", interner)).unwrap_or(0.0),
                        ),
                    ),
                    (
                        "Area",
                        FieldValue::Uint(u64::from(
                            scalar_u32(named(values, "Area", interner)).unwrap_or(0),
                        )),
                    ),
                    (
                        "Duration",
                        FieldValue::Uint(u64::from(
                            scalar_u32(named(values, "Duration", interner)).unwrap_or(0),
                        )),
                    ),
                ],
            );
        }
    }
    plan.target_fields = Some(fields);
    MagicSupport::RequiresLinkedRecords(MagicComponentRequirements {
        plan,
        linked_records,
    })
}

fn classify_effect(
    record: &Record,
    data: &[(crate::sym::Sym, FieldValue)],
    delivery: MagicDelivery,
    interner: &StringInterner,
) -> MagicSupport {
    let Some(cast) = parse_cast(named(data, "CastingType", interner), interner).or_else(|| {
        named(data, "CastingType", interner)
            .is_none()
            .then_some(MagicCast::ConstantEffect)
    }) else {
        return unsupported(MagicUnsupportedReason::CastDelivery);
    };
    let archetype = semantic(
        named(data, "Archtype", interner).or_else(|| named(data, "Archetype", interner)),
        interner,
    );
    let archetype = match archetype.as_deref() {
        None | Some("0" | "valuemodifier") => 0,
        Some("1" | "script") => 1,
        Some("4" | "absorb") => 4,
        Some("5" | "dualvaluemodifier") => 5,
        Some("7" | "demoralize") => 7,
        Some("9" | "disarm") => 9,
        Some("10" | "commandsummoned") => 10,
        Some("11" | "invisibility") => 11,
        Some("12" | "light") => 12,
        Some("18" | "summoncreature") => 18,
        Some("21" | "paralysis") => 21,
        Some("22" | "reanimate") => 22,
        Some("32" | "accumulatemagnitude") => 32,
        Some("33" | "stagger") => 33,
        Some("34" | "peakvaluemodifier") => 34,
        Some("35" | "cloak") => 35,
        Some("40" | "spawnhazard") => 40,
        _ => return unsupported(MagicUnsupportedReason::Archetype),
    };
    let mut plan = contact_plan(record, MagicComponent::NativeEffect, cast, delivery);
    let FieldValue::Struct(mut target) = fo4_magic_effect_data(&plan, interner) else {
        unreachable!()
    };
    let mut applies_skill_perk = false;
    for (name, value) in data {
        let name = interner.resolve(*name).map(normalized).unwrap_or_default();
        let target_name = match name.as_str() {
            "archtype" => "archetype",
            "magicskill" => "associatedskill",
            "secondactorvalue" => "actorvalue1",
            "unknownu86" | "unknownbyte7" => "unknownbyte10",
            "unknownu87" | "unknownbyte8" => "unknownbyte11",
            _ => &name,
        };
        let value = match target_name {
            "actorvalue" | "actorvalue1" | "resistvalue" | "associatedskill" => {
                applies_skill_perk |=
                    skill_modifiers::skill_modifier_source_key(value, interner).is_some();
                let Some(value) = actor_value(value, interner) else {
                    return unsupported(MagicUnsupportedReason::ActorValue);
                };
                value
            }
            "archetype" => FieldValue::Uint(archetype),
            "castingtype" => FieldValue::Uint(cast.target_value()),
            "delivery" => FieldValue::Uint(delivery.target_value()),
            "castingsoundlevel" => {
                let value = match semantic(Some(value), interner).as_deref() {
                    Some("0" | "loud") => 0,
                    Some("1" | "normal") => 1,
                    Some("2" | "silent") => 2,
                    Some("3" | "veryloud") => 3,
                    Some("4" | "quiet") => 4,
                    _ => return unsupported(MagicUnsupportedReason::SoundLevel),
                };
                FieldValue::Uint(value)
            }
            "flags" => match flag_mask(value, false, interner) {
                Some(value) => FieldValue::Uint(value),
                None => return unsupported(MagicUnsupportedReason::EffectFlags),
            },
            _ => value.clone(),
        };
        replace_named(&mut target, target_name, value, interner);
    }
    replace_named(
        &mut target,
        "archetype",
        FieldValue::Uint(archetype),
        interner,
    );
    if applies_skill_perk {
        if named(data, "PerkToApply", interner)
            .and_then(|value| form_from_value(value, interner))
            .is_some()
        {
            return unsupported(MagicUnsupportedReason::Archetype);
        }
        replace_named(
            &mut target,
            "PerkToApply",
            FieldValue::FormKey(skill_modifiers::skill_modifier_perk_source_key(interner)),
            interner,
        );
    }
    plan.target_fields = Some(replace_data(record, *b"DATA", FieldValue::Struct(target)));
    if archetype == 35 {
        let Some(form_key) =
            named(data, "AssocItem", interner).and_then(|value| form_from_value(value, interner))
        else {
            return unsupported(MagicUnsupportedReason::MissingData);
        };
        return requirements(
            plan,
            MagicLinkedRecordRequirement {
                role: MagicLinkedRole::AssociatedSpell,
                form_key,
                signature: SigCode(*b"SPEL"),
            },
        );
    }
    MagicSupport::Supported(plan)
}

fn contact_plan(
    record: &Record,
    component: MagicComponent,
    cast: MagicCast,
    delivery: MagicDelivery,
) -> MagicLoweringPlan {
    plan(
        record,
        record.sig.0,
        component,
        cast,
        delivery,
        None,
        None,
        None,
        None,
        None,
        Vec::new(),
        Vec::new(),
    )
}

fn replace_data(record: &Record, signature: [u8; 4], value: FieldValue) -> Vec<FieldEntry> {
    record
        .fields
        .iter()
        .map(|field| {
            if field.sig.0 == signature {
                FieldEntry {
                    sig: field.sig,
                    value: value.clone(),
                }
            } else {
                field.clone()
            }
        })
        .collect()
}

fn replace_named(
    fields: &mut [(crate::sym::Sym, FieldValue)],
    name: &str,
    value: FieldValue,
    interner: &StringInterner,
) {
    if let Some((_, target)) = fields.iter_mut().find(|(key, _)| {
        interner
            .resolve(*key)
            .is_some_and(|actual| normalized(actual) == normalized(name))
    }) {
        *target = value;
    }
}

pub(crate) fn actor_value(value: &FieldValue, interner: &StringInterner) -> Option<FieldValue> {
    if let FieldValue::FormKey(_) = value {
        return Some(value.clone());
    }
    if let Some(form) = magic_resources::magic_resource_source_key(value, interner)
        .or_else(|| skill_modifiers::skill_modifier_source_key(value, interner))
    {
        return Some(FieldValue::FormKey(form));
    }
    let value = semantic(Some(value), interner)?;
    let local = match value.as_str() {
        "-1" | "4294967295" | "none" => return Some(FieldValue::Uint(0)),
        "1" | "confidence" => 0x0002BD,
        "14" | "lockpicking" => 0x00037E,
        "15" | "sneak" => 0x00037F,
        "18" | "alteration" => 0x000382,
        "19" | "conjuration" => 0x000383,
        "20" | "destruction" => 0x000384,
        "21" | "illusion" => 0x000385,
        "22" | "restoration" => 0x000386,
        "24" | "health" => 0x0002D4,
        "26" | "stamina" => 0x0002D5,
        "27" | "healrate" => 0x0002D7,
        "29" | "staminarate" => 0x0002D8,
        "30" | "speedmult" => 0x0002DA,
        "32" | "carryweight" => 0x0002DC,
        "35" | "unarmeddamage" => 0x0002DF,
        "39" | "damageresist" => 0x0002E3,
        "40" | "poisonresist" | "resistpoison" => 0x0002E4,
        "41" | "resistfire" => 0x0002E5,
        "42" | "resistshock" => 0x0002E6,
        "43" | "resistfrost" => 0x0002E7,
        "44" | "resistmagic" => 0x0002E8,
        "53" | "paralysis" => 0x0002F2,
        "54" | "invisibility" => 0x0002F3,
        "57" | "waterbreathing" => 0x0002F6,
        "58" | "waterwalking" => 0x0002F7,
        "63" | "wardpower" => 0x0002FC,
        "74" | "variable07" => 0x000307,
        "84" | "blindness" => 0x000311,
        "87" | "bowstaggerbonus" => 0x000314,
        "134" | "combathealthregenmultiply" => 0x000343,
        "155" | "healratemult" => 0x000358,
        "157" | "staminaratemult" => 0x000359,
        _ => return None,
    };
    Some(FieldValue::FormKey(FormKey {
        local,
        plugin: interner.intern("Fallout4.esm"),
    }))
}

fn spell_type(value: &FieldValue, interner: &StringInterner) -> Option<u64> {
    let value = semantic(Some(value), interner)?;
    match value.as_str() {
        "spell" => Some(0),
        "disease" => Some(1),
        "power" => Some(2),
        "lesserpower" => Some(3),
        "ability" => Some(4),
        "poison" => Some(5),
        "addiction" => Some(10),
        "voice" => Some(11),
        _ => value
            .parse::<u64>()
            .ok()
            .filter(|value| matches!(value, 0..=5 | 10 | 11)),
    }
}

fn flag_mask(value: &FieldValue, spell: bool, interner: &StringInterner) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) => u32::try_from(*value).ok().map(u64::from),
        FieldValue::List(values) => values.iter().try_fold(0, |mask, value| {
            let name = semantic(Some(value), interner)?;
            let flag = if spell {
                match name.as_str() {
                    "manualcostcalc" => 1,
                    "unknown16" => 65536,
                    "pcstartspell" => 131072,
                    "unknown18" | "instantcast" => 262144,
                    "areaeffectignoreslos" => 524288,
                    "ignoreresistance" => 1048576,
                    "disallowabsorbreflect" | "noabsorbreflect" => 2097152,
                    "unknown22" => 4194304,
                    "nodualcastmodification" => 8388608,
                    _ => return None,
                }
            } else {
                match name.as_str() {
                    "hostile" => 1,
                    "recover" => 2,
                    "detrimental" => 4,
                    "snaptonavmesh" => 8,
                    "nohitevent" => 16,
                    "dispelwithkeywords" => 256,
                    "noduration" => 512,
                    "nomagnitude" => 1024,
                    "noarea" => 2048,
                    "fxpersist" => 4096,
                    "goryvisuals" => 16384,
                    "hideinui" => 32768,
                    "norecast" => 131072,
                    "poweraffectsmagnitude" => 2097152,
                    "poweraffectsduration" => 4194304,
                    "painless" => 67108864,
                    "nohiteffect" => 134217728,
                    "nodeathdispel" => 268435456,
                    "unknown31" => 1073741824,
                    _ => return None,
                }
            };
            Some(mask | flag)
        }),
        _ => None,
    }
}
