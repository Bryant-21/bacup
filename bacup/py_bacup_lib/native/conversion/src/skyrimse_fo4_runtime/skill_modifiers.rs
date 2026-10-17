use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const SOURCE_NAMESPACE: &str = "B21_SkyrimSkillRuntime.esm";
const MODIFIERS: [(u32, &str, u8); 4] = [
    (96, "OneHandedModifier", 35),
    (97, "TwoHandedModifier", 35),
    (103, "PickpocketModifier", 56),
    (104, "LockpickingModifier", 59),
];
// Skyrim's PerkSkillBoosts excludes daggers from OneHandedModifier.
const WEAPON_KEYWORDS: [(u32, &str); 6] = [
    (0x01E714, "WeapTypeMace"),
    (0x01E711, "WeapTypeSword"),
    (0x01E712, "WeapTypeWarAxe"),
    (0x06D932, "WeapTypeBattleaxe"),
    (0x06D931, "WeapTypeGreatsword"),
    (0x06D930, "WeapTypeWarhammer"),
];

pub(crate) fn skill_modifier_source_key(
    value: &FieldValue,
    interner: &StringInterner,
) -> Option<FormKey> {
    MODIFIERS
        .iter()
        .enumerate()
        .find_map(|(index, (number, name, _))| {
            let matches = match value {
                FieldValue::Uint(value) => *value == u64::from(*number),
                FieldValue::Int(value) => *value == i64::from(*number),
                FieldValue::String(value) => interner.resolve(*value).is_some_and(|value| {
                    value.eq_ignore_ascii_case(name)
                        || value.eq_ignore_ascii_case(name.strip_suffix("ifier").unwrap())
                        || value.parse::<u32>() == Ok(*number)
                }),
                _ => false,
            };
            matches.then(|| source_key(index, interner))
        })
}

pub(crate) fn skill_modifier_perk_source_key(interner: &StringInterner) -> FormKey {
    source_key(4, interner)
}

pub(crate) fn record_uses_skill_modifiers(record: &Record, interner: &StringInterner) -> bool {
    record.sig.0 == *b"MGEF"
        && record.fields.iter().any(|field| {
            field.sig.0 == *b"DATA"
                && match &field.value {
                    FieldValue::Struct(fields) => fields.iter().any(|(name, value)| {
                        interner.resolve(*name).is_some_and(|name| {
                            matches!(canonical(name).as_str(), "actorvalue" | "secondactorvalue")
                        }) && skill_modifier_source_key(value, interner).is_some()
                    }),
                    FieldValue::Bytes(bytes) if bytes.len() == 152 => {
                        [68, 88].into_iter().any(|offset| {
                            let number =
                                u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                            skill_modifier_source_key(&FieldValue::Uint(number.into()), interner)
                                .is_some()
                        })
                    }
                    _ => false,
                }
        })
}

pub(crate) fn allocate_skill_modifier_records(
    mapper: &mut FormKeyMapper<'_>,
    previous: Option<[FormKey; 11]>,
    interner: &StringInterner,
) -> ([FormKey; 11], Vec<Record>) {
    let targets = previous.unwrap_or_else(|| std::array::from_fn(|_| mapper.allocate_generated()));
    for (index, target) in targets.iter().copied().enumerate() {
        mapper.add_mapping(source_key(index, interner), target);
    }
    let records = if previous.is_some() {
        Vec::new()
    } else {
        skill_modifier_records(targets, interner)
    };
    (targets, records)
}

fn source_key(index: usize, interner: &StringInterner) -> FormKey {
    FormKey {
        local: if index < 5 {
            index as u32 + 1
        } else {
            WEAPON_KEYWORDS[index - 5].0
        },
        plugin: interner.intern(if index < 5 {
            SOURCE_NAMESPACE
        } else {
            "Skyrim.esm"
        }),
    }
}

pub(crate) fn ensure_skill_modifier_keywords(
    run: &mut crate::run::ConversionRun,
) -> Result<u32, String> {
    let Some(targets) = run.skyrim_skill_modifier_records else {
        return Ok(0);
    };
    ensure_keyword_records(
        run.target_handle_id,
        targets,
        &run.schema_target,
        &mut run.interner,
    )
}

fn ensure_keyword_records(
    handle: u64,
    targets: [FormKey; 11],
    schema: &crate::schema::AuthoringSchema,
    interner: &mut StringInterner,
) -> Result<u32, String> {
    let existing = crate::source_read::iter_form_keys_of_sig(handle, SigCode(*b"KYWD"), interner)
        .map_err(|error| error.to_string())?;
    let mut added = 0;
    for (index, (_, name)) in WEAPON_KEYWORDS.iter().enumerate() {
        let key = targets[index + 5];
        if existing.contains(&key) {
            continue;
        }
        let mut keyword = Record::new(SigCode(*b"KYWD"), key);
        keyword.eid = Some(interner.intern(&format!("B21_Skyrim{name}")));
        crate::target_write::add_record_native(handle, keyword, schema, interner)
            .map_err(|error| error.to_string())?;
        added += 1;
    }
    Ok(added)
}

fn skill_modifier_records(targets: [FormKey; 11], interner: &StringInterner) -> Vec<Record> {
    let mut records = Vec::with_capacity(5);
    for (index, (_, name, _)) in MODIFIERS.iter().enumerate() {
        let mut record = Record::new(SigCode(*b"AVIF"), targets[index]);
        record.eid = Some(interner.intern(&format!("B21_Skyrim{name}")));
        record.fields.extend([
            field(*b"NAM0", FieldValue::Float(0.0)),
            field(*b"AVFL", FieldValue::Uint(1024)),
            field(*b"NAM1", FieldValue::Uint(8)),
        ]);
        records.push(record);
    }
    let mut perk = Record::new(SigCode(*b"PERK"), targets[4]);
    // MGEF PerkToApply reference-counts this shared perk across active diseases;
    // reading their summed actor values preserves additive percentage stacking.
    perk.eid = Some(interner.intern("B21_SkyrimSkillModifierEffects"));
    perk.fields.push(field(*b"DATA", bytes(&[0, 0, 1, 0, 1])));
    for (index, (_, _, entry_point)) in MODIFIERS.iter().enumerate() {
        perk.fields
            .push(field(*b"PRKE", bytes(&[2, 0, index as u8])));
        perk.fields.push(field(
            *b"DATA",
            structure(
                interner,
                [
                    (
                        "EntryPointEntryPoint",
                        FieldValue::Uint((*entry_point).into()),
                    ),
                    ("EntryPointFunction", FieldValue::Uint(14)),
                    (
                        "EntryPointPerkConditionTabCount",
                        FieldValue::Uint(if index == 3 { 2 } else { 3 }),
                    ),
                ],
            ),
        ));
        if index < 2 {
            perk.fields.push(field(*b"PRKC", FieldValue::Uint(1)));
            for category in 0..3 {
                perk.fields.push(field(
                    *b"CTDA",
                    structure(
                        interner,
                        [
                            ("Type", FieldValue::Uint(if category < 2 { 1 } else { 0 })),
                            ("UnknownByte2", FieldValue::Uint(0)),
                            ("UnknownByte3", FieldValue::Uint(0)),
                            ("UnknownByte4", FieldValue::Uint(0)),
                            ("ComparisonValue", FieldValue::Float(1.0)),
                            ("Function", FieldValue::Uint(560)),
                            ("UnknownByte7", FieldValue::Uint(0)),
                            ("UnknownByte8", FieldValue::Uint(0)),
                            (
                                "Parameter1",
                                FieldValue::FormKey(targets[5 + index * 3 + category]),
                            ),
                            ("Parameter2", FieldValue::Uint(0)),
                            ("RunOn", FieldValue::Uint(0)),
                            ("Reference", FieldValue::Uint(0)),
                            ("Parameter3", FieldValue::Int(-1)),
                        ],
                    ),
                ));
            }
        }
        perk.fields.extend([
            field(*b"EPFT", FieldValue::Uint(2)),
            field(*b"EPFB", FieldValue::Uint(index as u64)),
            field(
                *b"EPFD",
                structure(
                    interner,
                    [
                        (
                            "ActorValueFloatActorValue",
                            FieldValue::FormKey(targets[index]),
                        ),
                        ("ActorValueFloatFloat", FieldValue::Float(0.01)),
                    ],
                ),
            ),
            field(*b"PRKF", FieldValue::None),
        ]);
    }
    records.push(perk);
    records
}

fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(sig),
        value,
    }
}

fn bytes(value: &[u8]) -> FieldValue {
    FieldValue::Bytes(value.iter().copied().collect())
}

fn structure<const N: usize>(
    interner: &StringInterner,
    fields: [(&str, FieldValue); N],
) -> FieldValue {
    FieldValue::Struct(
        fields
            .into_iter()
            .map(|(name, value)| (interner.intern(name), value))
            .collect(),
    )
}

fn canonical(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests;
