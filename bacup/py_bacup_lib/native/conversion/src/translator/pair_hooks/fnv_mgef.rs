//! Semantic FNV/FO3 `MGEF.DATA` conversion for the FO4 target layout.
//!
//! This normalizer belongs in the serial mapper pass. Legacy struct codecs arrive as raw
//! bytes, and their embedded FormIDs cannot be moved safely without the run's mapper state.

use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};

pub const LEGACY_MGEF_DATA_LEN: usize = 72;
pub const FO4_MGEF_DATA_LEN: usize = 152;

const NORMAL_CASTING_SOUND_LEVEL: u32 = 1;
const SOURCE_SELF: u32 = 1 << 4;
const SOURCE_TOUCH: u32 = 1 << 5;
const SOURCE_TARGET: u32 = 1 << 6;

const SOURCE_FLAG_MAP: &[(u32, u32)] = &[
    (1 << 0, 1 << 0),   // Hostile
    (1 << 1, 1 << 1),   // Recover
    (1 << 2, 1 << 2),   // Detrimental
    (1 << 7, 1 << 9),   // No Duration
    (1 << 8, 1 << 10),  // No Magnitude
    (1 << 9, 1 << 11),  // No Area
    (1 << 10, 1 << 12), // FX Persist
    (1 << 12, 1 << 14), // Gory Visuals
    (1 << 24, 1 << 26), // Painless
    (1 << 27, 1 << 27), // No Hit Effect
    (1 << 28, 1 << 28), // No Death Dispel
];

const ACTOR_VALUE_TARGETS: &[ActorValueTarget] = &[
    ActorValueTarget::HardcodedRaw(0xF400_02BC), // Aggression
    ActorValueTarget::HardcodedRaw(0xF400_02BD), // Confidence
    ActorValueTarget::HardcodedRaw(0xF400_02BE), // Energy
    ActorValueTarget::ExplicitNull,              // Responsibility
    ActorValueTarget::ExplicitNull,              // Mood
    ActorValueTarget::EditorId("Strength"),
    ActorValueTarget::EditorId("Perception"),
    ActorValueTarget::EditorId("Endurance"),
    ActorValueTarget::EditorId("Charisma"),
    ActorValueTarget::EditorId("Intelligence"),
    ActorValueTarget::EditorId("Agility"),
    ActorValueTarget::EditorId("Luck"),
    ActorValueTarget::EditorId("ActionPoints"),
    ActorValueTarget::EditorId("CarryWeight"),
    ActorValueTarget::HardcodedRaw(0xF400_02DD), // Critical Chance
    ActorValueTarget::HardcodedRaw(0xF400_02D7), // Heal Rate
    ActorValueTarget::HardcodedRaw(0xF400_02D4), // Health
    ActorValueTarget::HardcodedRaw(0xF400_02DE), // Melee Damage
    ActorValueTarget::HardcodedRaw(0xF400_02E3), // Damage Resistance
    ActorValueTarget::HardcodedRaw(0xF400_02E4), // Poison Resistance
    ActorValueTarget::HardcodedRaw(0xF400_02EA), // Rad Resistance
    ActorValueTarget::HardcodedRaw(0xF400_02DA), // Speed Multiplier
    ActorValueTarget::HardcodedRaw(0xF400_034F), // Fatigue
    ActorValueTarget::HardcodedRaw(0xF400_032C), // Karma
    ActorValueTarget::HardcodedRaw(0xF400_02C9), // XP
    ActorValueTarget::HardcodedRaw(0xF400_036C), // Perception Condition
    ActorValueTarget::HardcodedRaw(0xF400_036D), // Endurance Condition
    ActorValueTarget::HardcodedRaw(0xF400_036E), // Left Attack Condition
    ActorValueTarget::HardcodedRaw(0xF400_036F), // Right Attack Condition
    ActorValueTarget::HardcodedRaw(0xF400_0370), // Left Mobility Condition
    ActorValueTarget::HardcodedRaw(0xF400_0371), // Right Mobility Condition
    ActorValueTarget::HardcodedRaw(0xF400_0372), // Brain Condition
    ActorValueTarget::ExplicitNull,              // Barter
    ActorValueTarget::ExplicitNull,              // Big Guns
    ActorValueTarget::ExplicitNull,              // Energy Weapons
    ActorValueTarget::ExplicitNull,              // Explosives
    ActorValueTarget::HardcodedRaw(0xF400_037E), // Lockpick
    ActorValueTarget::ExplicitNull,              // Medicine
    ActorValueTarget::ExplicitNull,              // Melee Weapons
    ActorValueTarget::HardcodedRaw(0xF400_037A), // Repair
    ActorValueTarget::ExplicitNull,              // Science
    ActorValueTarget::ExplicitNull,              // Guns / Small Guns
    ActorValueTarget::HardcodedRaw(0xF400_037F), // Sneak
    ActorValueTarget::HardcodedRaw(0xF400_0381), // Speech
    ActorValueTarget::HardcodedRaw(0xF400_0367), // Survival; FO3 overrides this to NULL
    ActorValueTarget::ExplicitNull,              // Unarmed
    ActorValueTarget::ExplicitNull,              // Inventory Weight
    ActorValueTarget::HardcodedRaw(0xF400_02F2), // Paralysis
    ActorValueTarget::HardcodedRaw(0xF400_02F3), // Invisibility
    ActorValueTarget::ExplicitNull,              // Chameleon
    ActorValueTarget::HardcodedRaw(0xF400_02F4), // Night Eye
    ActorValueTarget::ExplicitNull,              // Turbo / Detect Life Range
    ActorValueTarget::HardcodedRaw(0xF400_02E5), // Fire Resistance
    ActorValueTarget::HardcodedRaw(0xF400_02F6), // Water Breathing
    ActorValueTarget::HardcodedRaw(0xF400_02E1), // Rad Level
    ActorValueTarget::HardcodedRaw(0xF400_02ED), // Bloody Mess
    ActorValueTarget::HardcodedRaw(0xF400_02DF), // Unarmed Damage
    ActorValueTarget::HardcodedRaw(0xF400_02C1), // Assistance
    ActorValueTarget::HardcodedRaw(0xF400_02E6), // Electric Resistance
    ActorValueTarget::HardcodedRaw(0xF400_02E7), // Frost Resistance
    ActorValueTarget::HardcodedRaw(0xF400_02EB), // Energy Resistance
    ActorValueTarget::ExplicitNull,              // EMP Resistance
    ActorValueTarget::HardcodedRaw(0xF400_0301), // Variable01
    ActorValueTarget::HardcodedRaw(0xF400_0302), // Variable02
    ActorValueTarget::HardcodedRaw(0xF400_0303), // Variable03
    ActorValueTarget::HardcodedRaw(0xF400_0304), // Variable04
    ActorValueTarget::HardcodedRaw(0xF400_0305), // Variable05
    ActorValueTarget::HardcodedRaw(0xF400_0306), // Variable06
    ActorValueTarget::HardcodedRaw(0xF400_0307), // Variable07
    ActorValueTarget::HardcodedRaw(0xF400_0308), // Variable08
    ActorValueTarget::HardcodedRaw(0xF400_0309), // Variable09
    ActorValueTarget::HardcodedRaw(0xF400_030A), // Variable10
    ActorValueTarget::HardcodedRaw(0xF400_02F8), // Ignore Crippled Limbs; FO3 NULL
    ActorValueTarget::HardcodedRaw(0xF400_0868), // Dehydration
    ActorValueTarget::HardcodedRaw(0xF400_0855), // Hunger
    ActorValueTarget::HardcodedRaw(0xF400_0828), // Sleep Deprivation
    ActorValueTarget::ExplicitNull,              // Damage Threshold
];

const TARGET_ONLY_REFERENCE_FIELDS: &[&str] = &[
    "projectile",
    "explosion",
    "casting_art",
    "hit_effect_art",
    "impact_data",
    "dual_casting_art",
    "enchant_art",
    "hit_visuals",
    "enchant_visuals",
    "equip_ability",
    "image_space_modifier",
    "perk_to_apply",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyMgefFamily {
    Fnv,
    Fo3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MgefReferenceOutcome {
    SourceNull,
    MappedRaw {
        source_raw: u32,
        target_raw: u32,
    },
    DeferredNull {
        source_raw: u32,
    },
    ResolvedTarget {
        source_value: i32,
        target: FormKey,
    },
    PreservedHardcoded {
        source_value: i32,
        target_raw: u32,
    },
    ExplicitNull {
        source_value: i32,
    },
    UnsupportedValue {
        source_value: i32,
    },
    MissingTarget {
        source_value: i32,
        editor_id: &'static str,
    },
    TargetOnlyNull,
    DroppedIncompatible {
        source_raw: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MgefReferenceDecision {
    pub field: &'static str,
    pub outcome: MgefReferenceOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MgefEnumDecision {
    pub field: &'static str,
    pub source: u32,
    pub target: u32,
    pub used_default: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MgefNormalizeReport {
    pub converted_rows: usize,
    pub preserved_target_rows: usize,
    pub unsupported_rows: usize,
    pub references: Vec<MgefReferenceDecision>,
    pub enums: Vec<MgefEnumDecision>,
}

pub fn normalize_legacy_mgef_data(
    record: &mut Record,
    family: LegacyMgefFamily,
    mapper: &mut FormKeyMapper<'_>,
) -> MgefNormalizeReport {
    let mut report = MgefNormalizeReport::default();
    if record.sig.0 != *b"MGEF" {
        return report;
    }

    for index in 0..record.fields.len() {
        if record.fields[index].sig.0 != *b"DATA" {
            continue;
        }

        let source = match &record.fields[index].value {
            FieldValue::Bytes(bytes) if bytes.len() == FO4_MGEF_DATA_LEN => {
                report.preserved_target_rows += 1;
                continue;
            }
            FieldValue::Bytes(bytes) if bytes.len() == LEGACY_MGEF_DATA_LEN => bytes.to_vec(),
            _ => {
                report.unsupported_rows += 1;
                continue;
            }
        };

        record.fields[index].value = convert_legacy_data(&source, family, mapper, &mut report);
        report.converted_rows += 1;
    }

    report
}

fn convert_legacy_data(
    source: &[u8],
    family: LegacyMgefFamily,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    let source_flags = read_u32(source, 0);
    let target_flags = translate_flags(source_flags);
    let delivery = translate_delivery(source_flags);
    let source_archetype = read_u32(source, 64);
    let target_archetype = translate_archetype(source_archetype);

    report.enums.extend([
        MgefEnumDecision {
            field: "flags",
            source: source_flags,
            target: target_flags,
            used_default: false,
        },
        MgefEnumDecision {
            field: "delivery",
            source: source_flags & (SOURCE_SELF | SOURCE_TOUCH | SOURCE_TARGET),
            target: delivery,
            used_default: source_flags & (SOURCE_SELF | SOURCE_TOUCH | SOURCE_TARGET) == 0,
        },
        MgefEnumDecision {
            field: "archetype",
            source: source_archetype,
            target: target_archetype,
            used_default: !legacy_archetype_has_target(source_archetype),
        },
    ]);

    let assoc_item = assoc_item_reference(read_u32(source, 8), source_archetype, mapper, report);
    let resist_value =
        actor_value_reference("resist_value", read_i32(source, 16), family, mapper, report);
    let casting_light = mapped_raw_reference("casting_light", read_u32(source, 24), mapper, report);
    let hit_shader = mapped_raw_reference("hit_shader", read_u32(source, 32), mapper, report);
    let enchant_shader =
        mapped_raw_reference("enchant_shader", read_u32(source, 36), mapper, report);
    let actor_value =
        actor_value_reference("actor_value", read_i32(source, 68), family, mapper, report);
    let actor_value_1 = actor_value.clone();
    let actor_value_1_outcome = report
        .references
        .last()
        .map(|decision| decision.outcome.clone())
        .unwrap_or(MgefReferenceOutcome::SourceNull);
    report.references.push(MgefReferenceDecision {
        field: "actor_value_1",
        outcome: actor_value_1_outcome,
    });

    for (field, offset) in [
        ("effect_sound", 40),
        ("bolt_sound", 44),
        ("hit_sound", 48),
        ("area_sound", 52),
    ] {
        let source_raw = read_u32(source, offset);
        report.references.push(MgefReferenceDecision {
            field,
            outcome: if source_raw == 0 {
                MgefReferenceOutcome::SourceNull
            } else {
                // Legacy SOUN slots do not identify an equivalent FO4 SNDD sound type,
                // and this pair has no proven SOUN→SNDR record mapping.
                MgefReferenceOutcome::DroppedIncompatible { source_raw }
            },
        });
    }
    for &field in TARGET_ONLY_REFERENCE_FIELDS {
        report.references.push(MgefReferenceDecision {
            field,
            outcome: MgefReferenceOutcome::TargetOnlyNull,
        });
    }

    let interner = mapper.interner;
    let mut fields = Vec::with_capacity(43);
    push_field(&mut fields, interner, "flags", u32_value(target_flags));
    push_field(
        &mut fields,
        interner,
        "base_cost",
        FieldValue::Float(finite_or_zero(read_f32(source, 4))),
    );
    push_field(&mut fields, interner, "assoc_item", assoc_item);
    for name in [
        "magic_skill_unused_byte_1",
        "magic_skill_unused_byte_2",
        "magic_skill_unused_byte_3",
        "magic_skill_unused_byte_4",
    ] {
        push_field(&mut fields, interner, name, byte_value(0));
    }
    push_field(&mut fields, interner, "resist_value", resist_value);
    push_field(
        &mut fields,
        interner,
        "counter_effect_count",
        u16_value(read_u16(source, 20)),
    );
    push_field(&mut fields, interner, "unknown_u8_9", byte_value(0));
    push_field(&mut fields, interner, "unknown_u8_10", byte_value(0));
    push_field(&mut fields, interner, "casting_light", casting_light);
    push_field(
        &mut fields,
        interner,
        "taper_weight",
        FieldValue::Float(0.0),
    );
    push_field(&mut fields, interner, "hit_shader", hit_shader);
    push_field(&mut fields, interner, "enchant_shader", enchant_shader);
    push_field(&mut fields, interner, "minimum_skill_level", u32_value(0));
    push_field(&mut fields, interner, "spellmaking_area", u32_value(0));
    push_field(
        &mut fields,
        interner,
        "spellmaking_casting_time",
        FieldValue::Float(0.0),
    );
    push_field(&mut fields, interner, "taper_curve", FieldValue::Float(0.0));
    push_field(
        &mut fields,
        interner,
        "taper_duration",
        FieldValue::Float(0.0),
    );
    push_field(
        &mut fields,
        interner,
        "second_av_weight",
        FieldValue::Float(0.0),
    );
    push_field(
        &mut fields,
        interner,
        "archetype",
        u32_value(target_archetype),
    );
    push_field(&mut fields, interner, "actor_value", actor_value);
    push_field(&mut fields, interner, "projectile", null_reference());
    push_field(&mut fields, interner, "explosion", null_reference());
    push_field(&mut fields, interner, "casting_type", u32_value(0));
    push_field(&mut fields, interner, "delivery", u32_value(delivery));
    push_field(&mut fields, interner, "actor_value_1", actor_value_1);
    push_field(&mut fields, interner, "casting_art", null_reference());
    push_field(&mut fields, interner, "hit_effect_art", null_reference());
    push_field(&mut fields, interner, "impact_data", null_reference());
    push_field(
        &mut fields,
        interner,
        "skill_usage_multiplier",
        FieldValue::Float(0.0),
    );
    push_field(&mut fields, interner, "dual_casting_art", null_reference());
    push_field(
        &mut fields,
        interner,
        "dual_casting_scale",
        FieldValue::Float(1.0),
    );
    push_field(&mut fields, interner, "enchant_art", null_reference());
    push_field(&mut fields, interner, "hit_visuals", null_reference());
    push_field(&mut fields, interner, "enchant_visuals", null_reference());
    push_field(&mut fields, interner, "equip_ability", null_reference());
    push_field(
        &mut fields,
        interner,
        "image_space_modifier",
        null_reference(),
    );
    push_field(&mut fields, interner, "perk_to_apply", null_reference());
    push_field(
        &mut fields,
        interner,
        "casting_sound_level",
        u32_value(NORMAL_CASTING_SOUND_LEVEL),
    );
    push_field(
        &mut fields,
        interner,
        "script_effect_ai_score",
        FieldValue::Float(0.0),
    );
    push_field(
        &mut fields,
        interner,
        "script_effect_ai_delay_time",
        FieldValue::Float(0.0),
    );

    FieldValue::Struct(fields)
}

fn mapped_raw_reference(
    field: &'static str,
    source_raw: u32,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    if source_raw == 0 {
        report.references.push(MgefReferenceDecision {
            field,
            outcome: MgefReferenceOutcome::SourceNull,
        });
        return null_reference();
    }

    let mut bytes = source_raw.to_le_bytes();
    if mapper.rewrite_raw_formid_at(&mut bytes, 0).is_none() {
        report.references.push(MgefReferenceDecision {
            field,
            outcome: MgefReferenceOutcome::DeferredNull { source_raw },
        });
        return null_reference();
    }
    report.references.push(MgefReferenceDecision {
        field,
        outcome: MgefReferenceOutcome::MappedRaw {
            source_raw,
            target_raw: u32::from_le_bytes(bytes),
        },
    });
    FieldValue::Bytes(SmallVec::from_slice(&bytes))
}

fn assoc_item_reference(
    source_raw: u32,
    source_archetype: u32,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    if source_raw == 0 || matches!(source_archetype, 18 | 19) {
        return mapped_raw_reference("assoc_item", source_raw, mapper, report);
    }
    report.references.push(MgefReferenceDecision {
        field: "assoc_item",
        outcome: MgefReferenceOutcome::DroppedIncompatible { source_raw },
    });
    null_reference()
}

fn actor_value_reference(
    field: &'static str,
    source_value: i32,
    family: LegacyMgefFamily,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    let (value, outcome) = match actor_value_target(family, source_value) {
        ActorValueTarget::SourceNull => (null_reference(), MgefReferenceOutcome::SourceNull),
        ActorValueTarget::ExplicitNull => (
            null_reference(),
            MgefReferenceOutcome::ExplicitNull { source_value },
        ),
        ActorValueTarget::Unsupported => (
            null_reference(),
            MgefReferenceOutcome::UnsupportedValue { source_value },
        ),
        ActorValueTarget::EditorId(editor_id) => {
            match mapper.find_vanilla_fk(editor_id, SigCode(*b"AVIF")) {
                Some(target) => (
                    FieldValue::FormKey(target),
                    MgefReferenceOutcome::ResolvedTarget {
                        source_value,
                        target,
                    },
                ),
                None => (
                    null_reference(),
                    MgefReferenceOutcome::MissingTarget {
                        source_value,
                        editor_id,
                    },
                ),
            }
        }
        ActorValueTarget::HardcodedRaw(target_raw) => (
            FieldValue::Bytes(SmallVec::from_slice(&target_raw.to_le_bytes())),
            MgefReferenceOutcome::PreservedHardcoded {
                source_value,
                target_raw,
            },
        ),
    };
    report
        .references
        .push(MgefReferenceDecision { field, outcome });
    value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActorValueTarget {
    SourceNull,
    EditorId(&'static str),
    HardcodedRaw(u32),
    ExplicitNull,
    Unsupported,
}

fn actor_value_target(family: LegacyMgefFamily, source_value: i32) -> ActorValueTarget {
    if source_value == -1 {
        return ActorValueTarget::SourceNull;
    }
    let Ok(index) = usize::try_from(source_value) else {
        return ActorValueTarget::Unsupported;
    };
    if family == LegacyMgefFamily::Fo3 && (index == 44 || index >= 72) {
        return if index <= 72 {
            ActorValueTarget::ExplicitNull
        } else {
            ActorValueTarget::Unsupported
        };
    }
    ACTOR_VALUE_TARGETS
        .get(index)
        .copied()
        .unwrap_or(ActorValueTarget::Unsupported)
}

fn translate_flags(source: u32) -> u32 {
    SOURCE_FLAG_MAP.iter().fold(0, |target, (from, to)| {
        if source & from != 0 {
            target | to
        } else {
            target
        }
    })
}

fn translate_delivery(source_flags: u32) -> u32 {
    if source_flags & SOURCE_TARGET != 0 {
        3
    } else if source_flags & SOURCE_TOUCH != 0 {
        1
    } else {
        0
    }
}

fn translate_archetype(source: u32) -> u32 {
    match source {
        0..=3 => source,
        11 => 11,
        12 => 49,
        13 => 12,
        16 => 15,
        17 => 16,
        18 => 17,
        19 => 18,
        24 => 21,
        30 => 27,
        31 => 28,
        32 => 29,
        33 => 30,
        _ => 0,
    }
}

fn legacy_archetype_has_target(source: u32) -> bool {
    matches!(
        source,
        0..=3 | 11 | 12 | 13 | 16 | 17 | 18 | 19 | 24 | 30 | 31 | 32 | 33
    )
}

fn push_field(
    fields: &mut Vec<(crate::sym::Sym, FieldValue)>,
    interner: &crate::sym::StringInterner,
    name: &str,
    value: FieldValue,
) {
    fields.push((interner.intern(name), value));
}

fn byte_value(value: u8) -> FieldValue {
    FieldValue::Bytes(SmallVec::from_slice(&[value]))
}

fn u16_value(value: u16) -> FieldValue {
    FieldValue::Bytes(SmallVec::from_slice(&value.to_le_bytes()))
}

fn u32_value(value: u32) -> FieldValue {
    FieldValue::Uint(u64::from(value))
}

fn null_reference() -> FieldValue {
    u32_value(0)
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::SubrecordSig;
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;

    fn form_key(interner: &StringInterner, plugin: &str, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn mapper<'a>(interner: &'a StringInterner, avifs: &[(&str, u32)]) -> FormKeyMapper<'a> {
        let fallout4 = interner.intern("Fallout4.esm");
        let eid_index = avifs.iter().map(|(editor_id, local)| {
            (
                interner.intern(&editor_id.to_ascii_lowercase()),
                FormKey {
                    local: *local,
                    plugin: fallout4,
                },
                SigCode(*b"AVIF"),
            )
        });
        FormKeyMapper::new(
            eid_index,
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: "FalloutNV.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            interner,
        )
    }

    fn record_with_data(interner: &StringInterner, bytes: Vec<u8>) -> Record {
        let mut record = Record::new(
            SigCode(*b"MGEF"),
            form_key(interner, "FalloutNV.esm", 0x800),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        });
        record
    }

    fn legacy_data() -> Vec<u8> {
        let mut bytes = vec![0; LEGACY_MGEF_DATA_LEN];
        set_i32(&mut bytes, 16, -1);
        set_i32(&mut bytes, 68, -1);
        bytes
    }

    fn encoded_data(record: &Record) -> Vec<u8> {
        let data = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DATA")
            .expect("DATA");
        let mut bytes = Vec::new();
        encode_value(&data.value, &mut bytes);
        bytes
    }

    fn encode_value(value: &FieldValue, output: &mut Vec<u8>) {
        match value {
            FieldValue::Uint(value) => output.extend_from_slice(&(*value as u32).to_le_bytes()),
            FieldValue::Int(value) => output.extend_from_slice(&(*value as i32).to_le_bytes()),
            FieldValue::Float(value) => output.extend_from_slice(&value.to_le_bytes()),
            FieldValue::Bytes(bytes) => output.extend_from_slice(bytes),
            FieldValue::FormKey(form_key) => {
                output.extend_from_slice(&form_key.local.to_le_bytes())
            }
            FieldValue::Struct(fields) => {
                for (_, value) in fields {
                    encode_value(value, output);
                }
            }
            other => panic!("unexpected fixture value: {other:?}"),
        }
    }

    fn decision<'a>(report: &'a MgefNormalizeReport, field: &str) -> &'a MgefReferenceOutcome {
        &report
            .references
            .iter()
            .find(|decision| decision.field == field)
            .unwrap_or_else(|| panic!("missing decision for {field}"))
            .outcome
    }

    #[test]
    fn fnv_golden_fixture_rebuilds_the_fo4_contract() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, &[]);
        mapper.add_mapping(
            form_key(&interner, "FalloutNV.esm", 0x1234),
            form_key(&interner, "Fallout4.esm", 0x5678),
        );
        mapper.add_mapping(
            form_key(&interner, "FalloutNV.esm", 0x3456),
            form_key(&interner, "Converted.esm", 0x6789),
        );

        let source_flags =
            (1 << 0) | (1 << 1) | SOURCE_SELF | (1 << 7) | (1 << 12) | (1 << 24) | (1 << 27);
        let mut source = legacy_data();
        set_u32(&mut source, 0, source_flags);
        set_f32(&mut source, 4, 12.5);
        set_u32(&mut source, 8, 0x1234);
        set_i32(&mut source, 16, 18);
        set_u16(&mut source, 20, 3);
        set_u32(&mut source, 24, 0x2345);
        set_u32(&mut source, 36, 0x3456);
        set_u32(&mut source, 48, 0x4444);
        set_u32(&mut source, 64, 18);
        set_i32(&mut source, 68, 16);
        let mut record = record_with_data(&interner, source);

        let report = normalize_legacy_mgef_data(&mut record, LegacyMgefFamily::Fnv, &mut mapper);
        let actual = encoded_data(&record);
        let mut expected = vec![0; FO4_MGEF_DATA_LEN];
        set_u32(
            &mut expected,
            0,
            (1 << 0) | (1 << 1) | (1 << 9) | (1 << 14) | (1 << 26) | (1 << 27),
        );
        set_f32(&mut expected, 4, 12.5);
        set_u32(&mut expected, 8, 0x5678);
        set_u32(&mut expected, 16, 0xF400_02E3);
        set_u16(&mut expected, 20, 3);
        set_u32(&mut expected, 36, 0x0100_6789);
        set_u32(&mut expected, 64, 17);
        set_u32(&mut expected, 68, 0xF400_02D4);
        set_u32(&mut expected, 88, 0xF400_02D4);
        set_f32(&mut expected, 112, 1.0);
        set_u32(&mut expected, 140, NORMAL_CASTING_SOUND_LEVEL);

        assert_eq!(actual, expected);
        assert_eq!(actual.len(), FO4_MGEF_DATA_LEN);
        assert_eq!(report.converted_rows, 1);
        assert!(matches!(
            decision(&report, "assoc_item"),
            MgefReferenceOutcome::MappedRaw {
                source_raw: 0x1234,
                target_raw: 0x5678
            }
        ));
        assert_eq!(
            decision(&report, "casting_light"),
            &MgefReferenceOutcome::DeferredNull { source_raw: 0x2345 }
        );
        assert_eq!(
            decision(&report, "hit_shader"),
            &MgefReferenceOutcome::SourceNull
        );
        assert_eq!(
            decision(&report, "hit_sound"),
            &MgefReferenceOutcome::DroppedIncompatible { source_raw: 0x4444 }
        );
    }

    #[test]
    fn fo3_golden_fixture_uses_fo3_actor_value_meanings() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, &[]);
        let mut source = legacy_data();
        set_u32(&mut source, 0, SOURCE_TARGET);
        set_i32(&mut source, 16, 16);
        set_u32(&mut source, 64, 12);
        set_i32(&mut source, 68, 44);
        let mut record = record_with_data(&interner, source);

        let report = normalize_legacy_mgef_data(&mut record, LegacyMgefFamily::Fo3, &mut mapper);
        let actual = encoded_data(&record);

        assert_eq!(actual.len(), FO4_MGEF_DATA_LEN);
        assert_eq!(read_u32(&actual, 16), 0xF400_02D4);
        assert_eq!(read_u32(&actual, 64), 49);
        assert_eq!(read_u32(&actual, 68), 0);
        assert_eq!(read_u32(&actual, 84), 3);
        assert_eq!(read_u32(&actual, 88), 0);
        assert_eq!(
            decision(&report, "actor_value"),
            &MgefReferenceOutcome::ExplicitNull { source_value: 44 }
        );
    }

    #[test]
    fn fnv_and_fo3_actor_value_44_diverge() {
        let interner = StringInterner::new();
        let mut source = legacy_data();
        set_i32(&mut source, 68, 44);

        let mut fnv_mapper = mapper(&interner, &[]);
        let mut fnv = record_with_data(&interner, source.clone());
        normalize_legacy_mgef_data(&mut fnv, LegacyMgefFamily::Fnv, &mut fnv_mapper);

        let mut fo3_mapper = mapper(&interner, &[]);
        let mut fo3 = record_with_data(&interner, source);
        normalize_legacy_mgef_data(&mut fo3, LegacyMgefFamily::Fo3, &mut fo3_mapper);

        assert_eq!(read_u32(&encoded_data(&fnv), 68), 0xF400_0367);
        assert_eq!(read_u32(&encoded_data(&fo3), 68), 0);
    }

    #[test]
    fn missing_target_avif_is_reported_and_never_fabricated() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, &[]);
        let mut source = legacy_data();
        set_i32(&mut source, 68, 5);
        let mut record = record_with_data(&interner, source);

        let report = normalize_legacy_mgef_data(&mut record, LegacyMgefFamily::Fnv, &mut mapper);

        assert_eq!(read_u32(&encoded_data(&record), 68), 0);
        assert_eq!(
            decision(&report, "actor_value"),
            &MgefReferenceOutcome::MissingTarget {
                source_value: 5,
                editor_id: "Strength"
            }
        );
    }

    #[test]
    fn ordinary_and_hardcoded_actor_values_keep_their_distinct_encodings() {
        let interner = StringInterner::new();
        let mut ordinary_mapper = mapper(&interner, &[("Strength", 0x2C2)]);
        let mut ordinary_source = legacy_data();
        set_i32(&mut ordinary_source, 68, 5);
        let mut ordinary = record_with_data(&interner, ordinary_source);
        let ordinary_report =
            normalize_legacy_mgef_data(&mut ordinary, LegacyMgefFamily::Fnv, &mut ordinary_mapper);
        assert_eq!(read_u32(&encoded_data(&ordinary), 68), 0x0000_02C2);
        assert!(matches!(
            decision(&ordinary_report, "actor_value"),
            MgefReferenceOutcome::ResolvedTarget {
                source_value: 5,
                target
            } if target.local == 0x2C2
        ));

        let mut hardcoded_mapper = mapper(&interner, &[]);
        let mut hardcoded_source = legacy_data();
        set_i32(&mut hardcoded_source, 68, 16);
        let mut hardcoded = record_with_data(&interner, hardcoded_source);
        let hardcoded_report = normalize_legacy_mgef_data(
            &mut hardcoded,
            LegacyMgefFamily::Fnv,
            &mut hardcoded_mapper,
        );
        assert_eq!(read_u32(&encoded_data(&hardcoded), 68), 0xF400_02D4);
        assert_eq!(
            decision(&hardcoded_report, "actor_value"),
            &MgefReferenceOutcome::PreservedHardcoded {
                source_value: 16,
                target_raw: 0xF400_02D4
            }
        );
    }

    #[test]
    fn unresolved_and_incompatible_struct_references_never_leak_source_raws() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, &[]);
        mapper.add_mapping(
            form_key(&interner, "FalloutNV.esm", 0x3333),
            form_key(&interner, "Converted.esm", 0x4444),
        );
        let mut source = legacy_data();
        set_u32(&mut source, 8, 0x1111);
        set_u32(&mut source, 24, 0x2222);
        set_u32(&mut source, 36, 0x3333);
        set_u32(&mut source, 64, 18);
        let mut record = record_with_data(&interner, source);
        let report = normalize_legacy_mgef_data(&mut record, LegacyMgefFamily::Fnv, &mut mapper);
        let data = encoded_data(&record);

        assert_eq!(read_u32(&data, 8), 0);
        assert_eq!(read_u32(&data, 24), 0);
        assert_eq!(read_u32(&data, 32), 0);
        assert_eq!(read_u32(&data, 36), 0x0100_4444);
        assert_eq!(
            decision(&report, "assoc_item"),
            &MgefReferenceOutcome::DeferredNull { source_raw: 0x1111 }
        );
        assert_eq!(
            decision(&report, "casting_light"),
            &MgefReferenceOutcome::DeferredNull { source_raw: 0x2222 }
        );
        assert_eq!(
            decision(&report, "hit_shader"),
            &MgefReferenceOutcome::SourceNull
        );
        assert!(matches!(
            decision(&report, "enchant_shader"),
            MgefReferenceOutcome::MappedRaw {
                source_raw: 0x3333,
                target_raw: 0x0100_4444
            }
        ));

        let mut incompatible_source = legacy_data();
        set_u32(&mut incompatible_source, 8, 0x5555);
        set_u32(&mut incompatible_source, 64, 1);
        let mut incompatible = record_with_data(&interner, incompatible_source);
        let incompatible_report =
            normalize_legacy_mgef_data(&mut incompatible, LegacyMgefFamily::Fnv, &mut mapper);
        assert_eq!(read_u32(&encoded_data(&incompatible), 8), 0);
        assert_eq!(
            decision(&incompatible_report, "assoc_item"),
            &MgefReferenceOutcome::DroppedIncompatible { source_raw: 0x5555 }
        );
    }

    #[test]
    fn target_sized_and_nonlegacy_rows_are_preserved() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner, &[]);
        for (length, expected_preserved, expected_unsupported) in
            [(FO4_MGEF_DATA_LEN, 1, 0), (160, 0, 1), (71, 0, 1)]
        {
            let original = vec![0xA5; length];
            let mut record = record_with_data(&interner, original.clone());
            let report =
                normalize_legacy_mgef_data(&mut record, LegacyMgefFamily::Fnv, &mut mapper);
            let FieldValue::Bytes(after) = &record.fields[0].value else {
                panic!("unchanged row should remain raw");
            };
            assert_eq!(after.as_slice(), original);
            assert_eq!(report.preserved_target_rows, expected_preserved);
            assert_eq!(report.unsupported_rows, expected_unsupported);
            assert_eq!(report.converted_rows, 0);
        }
    }

    #[test]
    fn enum_maps_stay_inside_fo4_domains() {
        let allowed_flags = SOURCE_FLAG_MAP
            .iter()
            .fold(0, |allowed, (_, target)| allowed | target);
        for bit in 0..32 {
            assert_eq!(translate_flags(1 << bit) & !allowed_flags, 0);
        }
        for flags in 0..=u8::MAX {
            assert!(matches!(translate_delivery(u32::from(flags)), 0 | 1 | 3));
        }

        let allowed_archetypes = [0, 1, 2, 3, 11, 12, 15, 16, 17, 18, 21, 27, 28, 29, 30, 49];
        for source in 0..=64 {
            assert!(allowed_archetypes.contains(&translate_archetype(source)));
        }
    }

    #[test]
    fn actor_value_table_matches_the_xedit_proto_contract_exhaustively() {
        assert_eq!(ACTOR_VALUE_TARGETS.len(), 77);
        let ordinary = [
            (5, "Strength"),
            (6, "Perception"),
            (7, "Endurance"),
            (8, "Charisma"),
            (9, "Intelligence"),
            (10, "Agility"),
            (11, "Luck"),
            (12, "ActionPoints"),
            (13, "CarryWeight"),
        ];
        let hardcoded = [
            (0, 0xF400_02BC),
            (1, 0xF400_02BD),
            (2, 0xF400_02BE),
            (14, 0xF400_02DD),
            (15, 0xF400_02D7),
            (16, 0xF400_02D4),
            (17, 0xF400_02DE),
            (18, 0xF400_02E3),
            (19, 0xF400_02E4),
            (20, 0xF400_02EA),
            (21, 0xF400_02DA),
            (22, 0xF400_034F),
            (23, 0xF400_032C),
            (24, 0xF400_02C9),
            (25, 0xF400_036C),
            (26, 0xF400_036D),
            (27, 0xF400_036E),
            (28, 0xF400_036F),
            (29, 0xF400_0370),
            (30, 0xF400_0371),
            (31, 0xF400_0372),
            (36, 0xF400_037E),
            (39, 0xF400_037A),
            (42, 0xF400_037F),
            (43, 0xF400_0381),
            (44, 0xF400_0367),
            (47, 0xF400_02F2),
            (48, 0xF400_02F3),
            (50, 0xF400_02F4),
            (52, 0xF400_02E5),
            (53, 0xF400_02F6),
            (54, 0xF400_02E1),
            (55, 0xF400_02ED),
            (56, 0xF400_02DF),
            (57, 0xF400_02C1),
            (58, 0xF400_02E6),
            (59, 0xF400_02E7),
            (60, 0xF400_02EB),
            (62, 0xF400_0301),
            (63, 0xF400_0302),
            (64, 0xF400_0303),
            (65, 0xF400_0304),
            (66, 0xF400_0305),
            (67, 0xF400_0306),
            (68, 0xF400_0307),
            (69, 0xF400_0308),
            (70, 0xF400_0309),
            (71, 0xF400_030A),
            (72, 0xF400_02F8),
            (73, 0xF400_0868),
            (74, 0xF400_0855),
            (75, 0xF400_0828),
        ];
        let explicit_null = [3, 4, 32, 33, 34, 35, 37, 38, 40, 41, 45, 46, 49, 51, 61, 76];
        let mut covered = [false; 77];
        for (source, editor_id) in ordinary {
            covered[source] = true;
            assert_eq!(
                actor_value_target(LegacyMgefFamily::Fnv, source as i32),
                ActorValueTarget::EditorId(editor_id)
            );
        }
        for (source, target_raw) in hardcoded {
            covered[source] = true;
            assert_eq!(
                actor_value_target(LegacyMgefFamily::Fnv, source as i32),
                ActorValueTarget::HardcodedRaw(target_raw)
            );
        }
        for source in explicit_null {
            covered[source] = true;
            assert_eq!(
                actor_value_target(LegacyMgefFamily::Fnv, source as i32),
                ActorValueTarget::ExplicitNull
            );
        }
        assert!(covered.into_iter().all(|entry| entry));
        assert_eq!(
            actor_value_target(LegacyMgefFamily::Fo3, 44),
            ActorValueTarget::ExplicitNull
        );
        assert_eq!(
            actor_value_target(LegacyMgefFamily::Fo3, 72),
            ActorValueTarget::ExplicitNull
        );
        assert_eq!(
            actor_value_target(LegacyMgefFamily::Fo3, 73),
            ActorValueTarget::Unsupported
        );
        assert_eq!(
            actor_value_target(LegacyMgefFamily::Fnv, -1),
            ActorValueTarget::SourceNull
        );
        assert_eq!(
            actor_value_target(LegacyMgefFamily::Fnv, -2),
            ActorValueTarget::Unsupported
        );
        assert_eq!(
            actor_value_target(LegacyMgefFamily::Fnv, 77),
            ActorValueTarget::Unsupported
        );
    }

    fn set_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn set_i32(bytes: &mut [u8], offset: usize, value: i32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn set_f32(bytes: &mut [u8], offset: usize, value: f32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
