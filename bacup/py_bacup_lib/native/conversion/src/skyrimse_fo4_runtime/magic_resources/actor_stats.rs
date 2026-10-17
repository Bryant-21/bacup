use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SourceAttributeFormula {
    pub base: [f32; 3],
    pub weights: [u8; 3],
    pub points_per_level: u32,
    pub health_per_level: f32,
}

impl SourceAttributeFormula {
    pub(crate) fn evaluate_at_level(&self, level: u16) -> [f32; 3] {
        let levels = u32::from(level.saturating_sub(1));
        let mut result = self.base;
        result[0] += levels as f32 * self.health_per_level;
        let mut points = u64::from(levels) * u64::from(self.points_per_level);
        let total_weight = self
            .weights
            .iter()
            .map(|&value| u64::from(value))
            .sum::<u64>();
        let mut order = [0, 1, 2];
        order.sort_by_key(|&index| {
            (
                std::cmp::Reverse(self.weights[index]),
                std::cmp::Reverse(index),
            )
        });
        if total_weight == 0 {
            return result;
        }
        let quotient = points / total_weight;
        points %= total_weight;
        let mut remaining_weights = self.weights;
        for index in 0..3 {
            result[index] += (quotient * u64::from(self.weights[index])) as f32;
        }
        while points > 0 {
            for index in order {
                if remaining_weights[index] > 0 && points > 0 {
                    result[index] += 1.0;
                    remaining_weights[index] -= 1;
                    points -= 1;
                }
            }
        }
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SourceActorLevel {
    Fixed {
        level: u16,
    },
    PlayerMultiplier {
        multiplier: f32,
        minimum: u16,
        maximum: u16,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SourceActorStats {
    pub health: f32,
    pub magicka: f32,
    pub stamina: f32,
    pub health_rate: f32,
    pub magicka_rate: f32,
    pub stamina_rate: f32,
    pub level: SourceActorLevel,
    pub formula: SourceAttributeFormula,
    pub skill_formula: SourceSkillFormula,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SourceSkillFormula {
    pub base: [u16; 18],
    pub weights: [u8; 18],
    pub points_per_level: u32,
    pub maximum: u8,
}

pub(crate) fn project_actor_stats(
    original: &Record,
    forced_race: Option<FormKey>,
    sources: &HashMap<FormKey, Record>,
    interner: &StringInterner,
) -> Result<SourceActorStats, String> {
    let is_race = original.sig.0 == *b"RACE";
    if !is_race && original.sig.0 != *b"NPC_" {
        return Err("actor attributes require an NPC_ or RACE record".into());
    }
    let race = if is_race {
        original
    } else {
        let key = if let Some(race) = forced_race {
            race
        } else {
            let traits = inherited_template(original, 1, sources, interner)?;
            form_key(traits, *b"RNAM").ok_or("source NPC has no resolved race")?
        };
        sources.get(&key).ok_or("source NPC race is absent")?
    };
    let race_data = field(race, *b"DATA").ok_or("source race has no DATA")?;
    let mut formula = SourceAttributeFormula {
        base: [
            number(race_data, "StartingHealth", 36, NumberKind::Float, interner)?,
            number(
                race_data,
                "StartingMagicka",
                40,
                NumberKind::Float,
                interner,
            )?,
            number(
                race_data,
                "StartingStamina",
                44,
                NumberKind::Float,
                interner,
            )?,
        ],
        weights: [0; 3],
        points_per_level: 10,
        health_per_level: 5.0,
    };
    let mut skill_formula = SourceSkillFormula {
        base: [15; 18],
        weights: [0; 18],
        points_per_level: 10,
        maximum: 100,
    };
    for record in sources.values().filter(|record| record.sig.0 == *b"GMST") {
        let Some(eid) = record.eid.and_then(|eid| interner.resolve(eid)) else {
            continue;
        };
        if eid.eq_ignore_ascii_case("iAVDhmsLevelUp") {
            formula.points_per_level = setting(record, interner)?.max(0.0) as u32;
        } else if eid.eq_ignore_ascii_case("fNPCHealthLevelBonus") {
            formula.health_per_level = setting(record, interner)?;
        } else if eid.eq_ignore_ascii_case("iAVDSkillStart") {
            skill_formula
                .base
                .fill(setting(record, interner)? as u8 as u16);
        } else if eid.eq_ignore_ascii_case("iAVDSkillsLevelUp") {
            skill_formula.points_per_level = setting(record, interner)?.max(0.0) as u32;
        } else if eid.eq_ignore_ascii_case("iAVDAutoCalcSkillMax") {
            skill_formula.maximum = setting(record, interner)?.clamp(0.0, 255.0) as u8;
        }
    }
    add_race_skill_boosts(&mut skill_formula.base, race_data, interner)?;
    let (level, initial_level) = if is_race {
        (SourceActorLevel::Fixed { level: 1 }, 1)
    } else {
        let stats = inherited_template(original, 2, sources, interner)?;
        let config = field(stats, *b"ACBS").ok_or("source NPC has no ACBS")?;
        formula.base[0] += number(
            config,
            "HealthOffset",
            20,
            NumberKind::SignedShort,
            interner,
        )?;
        formula.base[1] += number(
            config,
            "MagickaOffset",
            4,
            NumberKind::SignedShort,
            interner,
        )?;
        formula.base[2] += number(
            config,
            "StaminaOffset",
            6,
            NumberKind::SignedShort,
            interner,
        )?;
        if flag(config, "Flags", 0, 0x10, "AutoCalcStats", interner)? {
            let class = form_key(stats, *b"CNAM")
                .and_then(|key| sources.get(&key))
                .ok_or("source NPC has no resolved class")?;
            let class_data = field(class, *b"DATA").ok_or("source class has no DATA")?;
            formula.weights = attribute_weights(class_data, interner)?;
            skill_formula.weights = byte_array(class_data, "SkillWeights", 6, interner)?;
        } else {
            formula.health_per_level = 0.0;
            let data = field(stats, *b"DNAM").ok_or("manual source NPC has no skills")?;
            let values: [u8; 18] = byte_array(data, "SkillValues", 0, interner)?;
            let offsets: [u8; 18] = byte_array(data, "SkillOffsets", 18, interner)?;
            for index in 0..18 {
                skill_formula.base[index] = u16::from(values[index]) + u16::from(offsets[index]);
            }
        }
        let encoded_level = number(config, "Level", 8, NumberKind::UnsignedShort, interner)? as u16;
        if flag(config, "Flags", 0, 0x80, "PCLevelMult", interner)? {
            let minimum = number(
                config,
                "CalcMinLevel",
                10,
                NumberKind::UnsignedShort,
                interner,
            )? as u16;
            let maximum = number(
                config,
                "CalcMaxLevel",
                12,
                NumberKind::UnsignedShort,
                interner,
            )? as u16;
            (
                SourceActorLevel::PlayerMultiplier {
                    multiplier: f32::from(encoded_level) / 1000.0,
                    minimum,
                    maximum,
                },
                minimum.max(1),
            )
        } else {
            (
                SourceActorLevel::Fixed {
                    level: encoded_level,
                },
                encoded_level,
            )
        }
    };
    let [health, magicka, stamina] = formula.evaluate_at_level(initial_level);
    Ok(SourceActorStats {
        health,
        magicka,
        stamina,
        level,
        formula,
        skill_formula,
        health_rate: number(race_data, "HealthRegen", 84, NumberKind::Float, interner)?,
        magicka_rate: number(race_data, "MagickaRegen", 88, NumberKind::Float, interner)?,
        stamina_rate: number(race_data, "StaminaRegen", 92, NumberKind::Float, interner)?,
    })
}

fn byte_array<const N: usize>(
    value: &FieldValue,
    name: &str,
    offset: usize,
    interner: &StringInterner,
) -> Result<[u8; N], String> {
    if let FieldValue::Bytes(bytes) = value {
        return bytes
            .get(offset..offset + N)
            .ok_or_else(|| format!("truncated source {name}"))
            .map(|bytes| bytes.try_into().unwrap());
    }
    let Some(FieldValue::List(values)) = member(value, name, interner) else {
        return Err(format!("source record has no {name}"));
    };
    if values.len() < N {
        return Err(format!("truncated source {name}"));
    }
    let mut result = [0; N];
    for (target, value) in result.iter_mut().zip(values) {
        *target = numeric(value)
            .filter(|value| (0.0..=255.0).contains(value))
            .ok_or_else(|| format!("invalid source {name}"))? as u8;
    }
    Ok(result)
}

fn add_race_skill_boosts(
    base: &mut [u16; 18],
    data: &FieldValue,
    interner: &StringInterner,
) -> Result<(), String> {
    let mut add = |skill: i32, boost: u8| {
        if (6..24).contains(&skill) {
            let value = &mut base[(skill - 6) as usize];
            *value = u16::from((*value as u8).wrapping_add(boost));
        }
    };
    if let FieldValue::Bytes(bytes) = data {
        let boosts = bytes
            .get(..14)
            .ok_or("truncated source race skill boosts")?;
        for pair in boosts.chunks_exact(2) {
            add(i32::from(pair[0] as i8), pair[1]);
        }
    } else if let Some(FieldValue::List(boosts)) = member(data, "SkillBoosts", interner) {
        const SKILLS: [&str; 18] = [
            "OneHanded",
            "TwoHanded",
            "Marksman",
            "Block",
            "Smithing",
            "HeavyArmor",
            "LightArmor",
            "Pickpocket",
            "Lockpicking",
            "Sneak",
            "Alchemy",
            "Speechcraft",
            "Alteration",
            "Conjuration",
            "Destruction",
            "Illusion",
            "Restoration",
            "Enchanting",
        ];
        for boost in boosts {
            let skill =
                member(boost, "Skill", interner).ok_or("source skill boost has no skill")?;
            let skill = numeric(skill)
                .map(|value| value as i32)
                .or_else(|| {
                    if let FieldValue::String(value) = skill {
                        interner
                            .resolve(*value)
                            .and_then(|value| {
                                SKILLS
                                    .iter()
                                    .position(|name| canonical(name) == canonical(value))
                            })
                            .map(|index| index as i32 + 6)
                    } else {
                        None
                    }
                })
                .unwrap_or(-1);
            let value = member(boost, "Boost", interner)
                .and_then(numeric)
                .ok_or("source skill boost has no value")?;
            add(skill, value as i16 as u8);
        }
    }
    Ok(())
}

fn inherited_template<'a>(
    mut record: &'a Record,
    mask: u32,
    sources: &'a HashMap<FormKey, Record>,
    interner: &StringInterner,
) -> Result<&'a Record, String> {
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(record.form_key) {
            return Err("source NPC template cycle".into());
        }
        let Some(config) = field(record, *b"ACBS") else {
            return Ok(record);
        };
        let name = if mask == 1 { "Traits" } else { "Stats" };
        if !flag(config, "TemplateFlags", 18, mask, name, interner)? {
            return Ok(record);
        }
        let key = form_key(record, *b"TPLT").ok_or("source NPC template flag has no template")?;
        record = sources.get(&key).ok_or("source NPC template is absent")?;
        if record.sig.0 != *b"NPC_" {
            return Err("source NPC template requires a resolved NPC, not a leveled list".into());
        }
    }
}

fn field(record: &Record, sig: [u8; 4]) -> Option<&FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == sig)
        .map(|entry| &entry.value)
}

fn form_key(record: &Record, sig: [u8; 4]) -> Option<FormKey> {
    match field(record, sig)? {
        FieldValue::FormKey(key) => Some(*key),
        _ => None,
    }
}

fn member<'a>(
    value: &'a FieldValue,
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    fields
        .iter()
        .find(|(key, _)| {
            interner
                .resolve(*key)
                .is_some_and(|key| canonical(key) == canonical(name))
        })
        .map(|(_, value)| value)
}

fn canonical(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

enum NumberKind {
    Float,
    SignedShort,
    UnsignedShort,
}

fn number(
    value: &FieldValue,
    name: &str,
    offset: usize,
    kind: NumberKind,
    interner: &StringInterner,
) -> Result<f32, String> {
    let value = if let FieldValue::Bytes(bytes) = value {
        let length = if matches!(kind, NumberKind::Float) {
            4
        } else {
            2
        };
        let bytes = bytes
            .get(offset..offset + length)
            .ok_or_else(|| format!("truncated source {name}"))?;
        match kind {
            NumberKind::Float => f32::from_le_bytes(bytes.try_into().unwrap()),
            NumberKind::SignedShort => f32::from(i16::from_le_bytes(bytes.try_into().unwrap())),
            NumberKind::UnsignedShort => f32::from(u16::from_le_bytes(bytes.try_into().unwrap())),
        }
    } else {
        numeric(member(value, name, interner).ok_or_else(|| format!("missing source {name}"))?)
            .ok_or_else(|| format!("invalid source {name}"))?
    };
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| format!("non-finite source {name}"))
}

fn numeric(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Uint(value) => Some(*value as f32),
        _ => None,
    }
}

fn flag(
    value: &FieldValue,
    name: &str,
    offset: usize,
    mask: u32,
    label: &str,
    interner: &StringInterner,
) -> Result<bool, String> {
    if let FieldValue::Bytes(bytes) = value {
        let length = if name == "Flags" { 4 } else { 2 };
        let bytes = bytes
            .get(offset..offset + length)
            .ok_or("truncated source NPC flags")?;
        let flags = if length == 4 {
            u32::from_le_bytes(bytes.try_into().unwrap())
        } else {
            u32::from(u16::from_le_bytes(bytes.try_into().unwrap()))
        };
        return Ok(flags & mask != 0);
    }
    let Some(value) = member(value, name, interner) else {
        return Ok(false);
    };
    Ok(match value {
        FieldValue::Uint(value) => *value & u64::from(mask) != 0,
        FieldValue::Int(value) => *value & i64::from(mask) != 0,
        FieldValue::List(values) => values.iter().any(|value| matches!(value, FieldValue::String(value) if interner.resolve(*value).is_some_and(|value| canonical(value) == canonical(label) || canonical(value) == format!("use{}", canonical(label))))),
        _ => return Err("invalid source NPC flags".into()),
    })
}

fn attribute_weights(value: &FieldValue, interner: &StringInterner) -> Result<[u8; 3], String> {
    if let FieldValue::Bytes(bytes) = value {
        return bytes
            .get(32..35)
            .ok_or_else(|| "truncated source class weights".into())
            .map(|bytes| bytes.try_into().unwrap());
    }
    match member(value, "AttributeWeights", interner) {
        Some(FieldValue::List(weights)) if weights.len() >= 3 => {
            let mut result = [0; 3];
            for (target, value) in result.iter_mut().zip(weights) {
                *target = numeric(value)
                    .filter(|value| (0.0..=255.0).contains(value))
                    .ok_or("invalid source class weight")? as u8;
            }
            Ok(result)
        }
        _ => Err("source class has no attribute weights".into()),
    }
}

fn setting(record: &Record, interner: &StringInterner) -> Result<f32, String> {
    let value = field(record, *b"DATA").ok_or("source GMST has no DATA")?;
    let value = numeric(value)
        .or_else(|| member(value, "Value", interner).and_then(numeric))
        .or_else(|| {
            if let FieldValue::Bytes(bytes) = value {
                bytes.get(..4).map(|bytes| {
                    if record
                        .eid
                        .and_then(|eid| interner.resolve(eid))
                        .is_some_and(|eid| eid.starts_with('i'))
                    {
                        i32::from_le_bytes(bytes.try_into().unwrap()) as f32
                    } else {
                        f32::from_le_bytes(bytes.try_into().unwrap())
                    }
                })
            } else {
                None
            }
        });
    value
        .filter(|value| value.is_finite())
        .ok_or_else(|| "invalid source attribute game setting".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    fn record(sig: [u8; 4], local: u32, interner: &StringInterner) -> Record {
        Record::new(
            SigCode(sig),
            FormKey {
                local,
                plugin: interner.intern("Source.esm"),
            },
        )
    }

    fn add(record: &mut Record, sig: [u8; 4], value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(sig),
            value,
        });
    }

    #[test]
    fn class_attribute_distribution_preserves_rounding_remainders() {
        let mut formula = SourceAttributeFormula {
            base: [100.0; 3],
            weights: [1, 2, 3],
            points_per_level: 10,
            health_per_level: 5.0,
        };
        assert_eq!(formula.evaluate_at_level(7), [140.0, 120.0, 130.0]);
        assert_eq!(formula.evaluate_at_level(3), [113.0, 107.0, 110.0]);
        formula.weights = [1, 4, 1];
        assert_eq!(formula.evaluate_at_level(6), [133.0, 133.0, 109.0]);
        formula.weights = [1, 1, 1];
        assert_eq!(formula.evaluate_at_level(2), [108.0, 103.0, 104.0]);
        formula.weights = [0, 0, 0];
        assert_eq!(formula.evaluate_at_level(2), [105.0, 100.0, 100.0]);
    }

    #[test]
    fn source_wolf_class_and_race_reproduce_cached_attributes() {
        let interner = StringInterner::new();
        let mut race = record(*b"RACE", 1, &interner);
        let mut data = vec![0; 164];
        for (offset, value) in [(36, 12.0_f32), (44, 200.0), (88, 10.0), (92, 5.0)] {
            data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        add(
            &mut race,
            *b"DATA",
            FieldValue::Bytes(data.as_slice().into()),
        );
        let mut class = record(*b"CLAS", 2, &interner);
        let mut data = [0; 36];
        data[32..35].copy_from_slice(&[3, 0, 3]);
        add(
            &mut class,
            *b"DATA",
            FieldValue::Bytes(data.as_slice().into()),
        );
        let mut npc = record(*b"NPC_", 3, &interner);
        let mut data = [0; 24];
        data[0] = 0x10;
        data[8..10].copy_from_slice(&2_u16.to_le_bytes());
        add(
            &mut npc,
            *b"ACBS",
            FieldValue::Bytes(data.as_slice().into()),
        );
        add(&mut npc, *b"RNAM", FieldValue::FormKey(race.form_key));
        add(&mut npc, *b"CNAM", FieldValue::FormKey(class.form_key));
        let sources = [race, class]
            .into_iter()
            .map(|record| (record.form_key, record))
            .collect();
        let stats = project_actor_stats(&npc, None, &sources, &interner).unwrap();
        assert_eq!(
            [stats.health, stats.magicka, stats.stamina],
            [22.0, 0.0, 205.0]
        );
        assert_eq!(
            [stats.health_rate, stats.magicka_rate, stats.stamina_rate],
            [0.0, 10.0, 5.0]
        );
        assert_eq!(stats.level, SourceActorLevel::Fixed { level: 2 });
    }

    #[test]
    fn source_dragon_raw_data_reproduces_cached_attributes() {
        let interner = StringInterner::new();
        let bytes = |text: &str| {
            FieldValue::Bytes(
                (0..text.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&text[offset..offset + 2], 16).unwrap())
                    .collect::<Vec<_>>()
                    .as_slice()
                    .into(),
            )
        };
        let mut race = record(*b"RACE", 0x012E82, &interner);
        add(&mut race, *b"DATA", bytes("FF00FF00FF00FF00FF00FF00FF0000000000803F0000803F0000803F0000803F808D15010000FA43000016430000C84200004843000080400AD7233C0AD7233C03000000FFFFFFFFFFFFFFFF3333B33EFFFFFFFF3333333F00002041000020410000C8410000804302000000000027420000C8430000803E0000A040000000007FEA7DC20000000000000000000048C2000000000000824200000000000096C300000000"));
        let mut class = record(*b"CLAS", 0x02F201, &interner);
        add(
            &mut class,
            *b"DATA",
            bytes("000000000000030003030000000000050000000000000000000000006400000004000200"),
        );
        let mut npc = record(*b"NPC_", 0x01CA03, &interner);
        let mut config = [0; 24];
        config[0] = 0x10;
        config[8..10].copy_from_slice(&10_u16.to_le_bytes());
        config[20..22].copy_from_slice(&300_i16.to_le_bytes());
        add(
            &mut npc,
            *b"ACBS",
            FieldValue::Bytes(config.as_slice().into()),
        );
        add(&mut npc, *b"RNAM", FieldValue::FormKey(race.form_key));
        add(&mut npc, *b"CNAM", FieldValue::FormKey(class.form_key));
        let sources = [race, class]
            .into_iter()
            .map(|record| (record.form_key, record))
            .collect();
        let stats = project_actor_stats(&npc, None, &sources, &interner).unwrap();
        assert_eq!(
            [stats.health, stats.magicka, stats.stamina],
            [905.0, 150.0, 130.0]
        );
        assert_eq!(
            [stats.health_rate, stats.magicka_rate, stats.stamina_rate],
            [0.7, 10.0, 10.0]
        );
        assert_eq!(stats.skill_formula.base, [15; 18]);
        assert_eq!(
            stats.skill_formula.weights,
            [3, 0, 3, 3, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn manual_npc_uses_dnam_skills_and_race_offsets_without_level_bonuses() {
        let interner = StringInterner::new();
        let mut race = record(*b"RACE", 1, &interner);
        let mut data = vec![0; 164];
        for offset in [36, 40, 44] {
            data[offset..offset + 4].copy_from_slice(&100.0_f32.to_le_bytes());
        }
        add(
            &mut race,
            *b"DATA",
            FieldValue::Bytes(data.as_slice().into()),
        );
        let mut npc = record(*b"NPC_", 2, &interner);
        let mut config = [0; 24];
        config[8..10].copy_from_slice(&40_u16.to_le_bytes());
        config[4..6].copy_from_slice(&(-20_i16).to_le_bytes());
        config[20..22].copy_from_slice(&30_i16.to_le_bytes());
        add(
            &mut npc,
            *b"ACBS",
            FieldValue::Bytes(config.as_slice().into()),
        );
        add(&mut npc, *b"RNAM", FieldValue::FormKey(race.form_key));
        let mut skills = [0; 52];
        skills[..18].fill(50);
        skills[14] = 80;
        skills[18 + 14] = 25;
        add(
            &mut npc,
            *b"DNAM",
            FieldValue::Bytes(skills.as_slice().into()),
        );
        let sources = [(race.form_key, race)].into_iter().collect();
        let stats = project_actor_stats(&npc, None, &sources, &interner).unwrap();
        assert_eq!(
            [stats.health, stats.magicka, stats.stamina],
            [130.0, 80.0, 100.0]
        );
        assert_eq!(stats.skill_formula.base[14], 105);
        assert_eq!(stats.skill_formula.base[13], 50);
        assert_eq!(stats.skill_formula.weights, [0; 18]);
        assert_eq!(stats.formula.evaluate_at_level(80), [130.0, 80.0, 100.0]);
    }
}
