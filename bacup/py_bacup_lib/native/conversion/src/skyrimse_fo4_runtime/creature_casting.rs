use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::source_rig::{CreatureClipRole, TargetFormKey};
use crate::sym::StringInterner;

use super::creature_mvp_adapter::SkyrimCreatureTargetDependencyProjection;
use super::creature_recipe::SkyrimCreatureFamilyJob;
mod actions;
mod spell_actions;
pub use actions::SourceCastAction;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CreatureCastCatalog {
    pub actors: Vec<CreatureCastActor>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureCastActor {
    pub npc: TargetFormKey,
    pub race: TargetFormKey,
    pub source_race: String,
    pub abilities: Vec<CreatureCastAbility>,
    pub animations: Vec<CreatureCastAnimation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureCastAbility {
    pub source: String,
    pub shout: bool,
    pub action: Option<SourceCastAction>,
    pub words: Vec<CreatureCastWord>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureCastWord {
    pub spell: TargetFormKey,
    pub source_spell: String,
    pub once_per_day: bool,
    pub power_uses: Vec<String>,
    pub ordinal: usize,
    pub word_present: bool,
    pub recovery: f32,
    pub prepare_setting: Option<SourceShoutTiming>,
    pub timeout_setting: Option<SourceShoutTiming>,
    pub cast_type: u32,
    pub delivery: u32,
    pub duration: f32,
    pub range: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceShoutTiming {
    pub setting: String,
    pub source: String,
    pub seconds: f32,
}

fn shout_timing_setting(records: &HashMap<FormKey, &Record>, name: &str, interner: &StringInterner) -> Result<SourceShoutTiming, String> {
    let matches = records.values().filter(|record| record.sig.0 == *b"GMST" &&
        matches!(field(record, b"EDID"), Some(FieldValue::String(value)) if interner.resolve(*value).is_some_and(|value| value == name)))
        .collect::<Vec<_>>();
    let [record] = matches.as_slice() else { return Err(format!("missing unique winning source GMST {name}")); };
    let seconds = field(record, b"DATA").and_then(|value| match value {
        FieldValue::Bytes(bytes) if bytes.len() == 4 => Some(f32::from_le_bytes(bytes[..].try_into().unwrap())),
        value => number(value),
    }).filter(|value| value.is_finite() && *value >= 0.0)
        .ok_or_else(|| format!("source GMST {name} has invalid duration"))?;
    Ok(SourceShoutTiming { setting: name.into(), source: record.form_key.format(interner), seconds })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureCastAnimation {
    pub event: String,
    pub prepare: Option<String>,
    pub ready: Option<String>,
    pub interrupt: Option<String>,
    pub allowed_spells: Vec<String>,
    pub event_driven: bool,
    pub stop_events: Vec<String>,
    pub observation_events: Vec<String>,
    pub finish_event: String,
    pub clip: String,
    pub source_clip: String,
    pub voice: bool,
    pub casting_source: u32,
    pub release: String,
    pub release_time: f32,
    pub clip_duration: f32,
    pub looping: bool,
    pub stop: Option<String>,
    pub stop_time: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SourceShoutWord {
    pub spell: FormKey,
    pub ordinal: usize,
    pub word_present: bool,
    pub recovery: f32,
}

pub(crate) fn shout_words(
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<SourceShoutWord>, String> {
    let mut words = Vec::new();
    let mut ordinal = 0;
    for field in record.fields.iter().filter(|field| field.sig.0 == *b"SNAM") {
        let values = match &field.value {
            FieldValue::List(values) => values.as_slice(),
            value => std::slice::from_ref(value),
        };
        for value in values {
            let spell = named(value, "spell", interner).and_then(form_key);
            let word_present = named(value, "word", interner)
                .and_then(form_key)
                .is_some_and(|key| key.local != 0);
            let recovery = named(value, "recovery_time", interner)
                .and_then(number)
                .unwrap_or(0.0);
            if !recovery.is_finite() || recovery < 0.0 {
                return Err(format!(
                    "{} word {ordinal} has invalid recovery",
                    record.form_key.format(interner)
                ));
            }
            if let Some(spell) = spell.filter(|key| key.local != 0) {
                words.push(SourceShoutWord {
                    spell,
                    ordinal,
                    word_present,
                    recovery,
                });
            }
            ordinal += 1;
        }
    }
    if words.is_empty()
        || ordinal > 3
        || (!words.iter().any(|word| word.word_present) && words[0].ordinal != 0)
    {
        return Err(format!(
            "{} has {} usable shout words",
            record.form_key.format(interner),
            words.len()
        ));
    }
    Ok(words)
}

pub(crate) fn is_active_spell(record: &Record, interner: &StringInterner) -> bool {
    if record.sig.0 != *b"SPEL" {
        return false;
    }
    let Some(data) = spell_data(record, interner) else {
        return false;
    };
    let data = data.as_ref();
    let spell_type = semantic(
        named(data, "type", interner),
        &[
            "spell",
            "disease",
            "power",
            "lesserpower",
            "ability",
            "poison",
            "enchantment",
            "potion",
            "ingredient",
            "leveledspell",
            "addiction",
            "voicepower",
        ],
        interner,
    )
    .or_else(|| named(data, "type", interner).is_none().then_some(0));
    let cast = semantic(
        named(data, "cast_type", interner),
        &["constanteffect", "fireandforget", "concentration"],
        interner,
    )
    .or_else(|| named(data, "cast_type", interner).is_none().then_some(0));
    let delivery = semantic(
        named(data, "target_type", interner).or_else(|| named(data, "delivery", interner)),
        &["self", "touch", "aimed", "targetactor", "targetlocation"],
        interner,
    )
    .or_else(|| (named(data, "target_type", interner).is_none() && named(data, "delivery", interner).is_none()).then_some(0));
    matches!(spell_type, Some(0 | 2 | 3 | 11)) && matches!(cast, Some(1 | 2)) && matches!(delivery, Some(0 | 2 | 3 | 4))
}

pub(crate) fn build_actor_profile(
    race: &Record,
    npc_spells: Option<&Record>,
    target_npc: TargetFormKey,
    target_race: TargetFormKey,
    family: &SkyrimCreatureFamilyJob,
    records: &HashMap<FormKey, &Record>,
    projections: &[SkyrimCreatureTargetDependencyProjection],
    interner: &StringInterner,
) -> Result<Option<CreatureCastActor>, String> {
    let mut roots = Vec::new();
    for owner in std::iter::once(race).chain(npc_spells) {
        for entry in owner.fields.iter().filter(|entry| entry.sig.0 == *b"SPLO") {
            if let Some(key) = form_key(&entry.value) {
                if !roots.contains(&key) {
                    roots.push(key);
                }
            }
        }
    }
    let mut abilities = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut cursor = 0;
    while cursor < roots.len() {
        let key = roots[cursor];
        cursor += 1;
        if !seen.insert(key) {
            continue;
        }
        let source = records
            .get(&key)
            .ok_or_else(|| format!("missing ability {}", key.format(interner)))?;
        if source.sig.0 == *b"LVSP" {
            for entry in source.fields.iter().filter(|field| field.sig.0 == *b"LVLO") {
                let values = match &entry.value {
                    FieldValue::List(values) => values.as_slice(),
                    value => std::slice::from_ref(value),
                };
                for value in values {
                    if let Some(target) = named(value, "reference", interner).and_then(form_key) {
                        roots.push(target);
                    } else {
                        return Err("leveled spell row lacks its source master context".to_string());
                    }
                }
            }
            continue;
        }
        let shout = source.sig.0 == *b"SHOU";
        let words = if shout {
            shout_words(source, interner)?
        } else if is_active_spell(source, interner) {
            vec![SourceShoutWord {
                spell: key,
                ordinal: 0,
                word_present: false,
                recovery: 0.0,
            }]
        } else {
            continue;
        };
        let shared_power_uses = if shout && source.flags.bits() & 0x80 != 0 {
            words.iter().filter_map(|word| records.get(&word.spell)).filter(|spell| is_daily_power(spell, interner))
                .map(|spell| spell.form_key.format(interner)).collect::<Vec<_>>()
        } else { Vec::new() };
        let mut mapped = Vec::new();
        for word in words {
            let spell = records
                .get(&word.spell)
                .ok_or_else(|| format!("missing shout spell {}", word.spell.format(interner)))?;
            if !is_active_spell(spell, interner) {
                return Err(format!(
                    "{} has non-active word {}",
                    key.format(interner),
                    word.spell.format(interner)
                ));
            }
            let target = projections
                .iter()
                .find(|projection| {
                    projection.source.form_key == word.spell
                        && projection.target.signature == "SPEL"
                })
                .ok_or_else(|| {
                    format!("unpublished active spell {}", word.spell.format(interner))
                })?;
            let data = spell_data(spell, interner).ok_or("active spell has no SPIT")?;
            let data = data.as_ref();
            let cast_type = semantic(
                named(data, "cast_type", interner),
                &["constanteffect", "fireandforget", "concentration"],
                interner,
            )
            .ok_or("invalid casting type")?;
            let delivery = semantic(
                named(data, "target_type", interner).or_else(|| named(data, "delivery", interner)),
                &["self", "touch", "aimed", "targetactor", "targetlocation"],
                interner,
            )
            .unwrap_or(0);
            let duration = named(data, "cast_duration", interner)
                .and_then(number)
                .unwrap_or(0.0);
            let range = named(data, "range", interner)
                .and_then(number)
                .unwrap_or(0.0);
            if !duration.is_finite() || duration < 0.0 || !range.is_finite() || range < 0.0 {
                return Err(format!(
                    "invalid active spell timing/range {}",
                    word.spell.format(interner)
                ));
            }
            mapped.push(CreatureCastWord {
                spell: target.target.form_key.clone(),
                source_spell: word.spell.format(interner),
                once_per_day: is_daily_power(spell, interner),
                power_uses: if is_daily_power(spell, interner) { vec![word.spell.format(interner)] }
                    else if named(data, "type", interner).is_some_and(|value| matches!(value, FieldValue::Uint(11) | FieldValue::Int(11))
                        || matches!(value, FieldValue::String(value) if interner.resolve(*value).is_some_and(|name| matches!(normalize(name).as_str(), "voice" | "voicepower")))) { shared_power_uses.clone() }
                    else { Vec::new() },
                ordinal: word.ordinal,
                word_present: word.word_present,
                recovery: word.recovery,
                prepare_setting: if shout { Some(shout_timing_setting(records, if word.ordinal == 0 { "fShoutTime1" } else { "fShoutTime2" }, interner)?) } else { None },
                timeout_setting: if shout { Some(shout_timing_setting(records, "fShoutTimeout", interner)?) } else { None },
                cast_type,
                delivery,
                duration,
                range,
            });
        }
        abilities.push(CreatureCastAbility {
            source: key.format(interner),
            shout,
            action: if shout { Some(actions::family_shout_action(&family.family_id, key, records, interner)?) } else { None },
            words: mapped,
        });
    }
    if abilities.is_empty() {
        return Ok(None);
    }
    let mut animations = casting_animations(family)?;
    let voice = animations.iter().filter(|animation| animation.voice).cloned().collect::<Vec<_>>();
    animations.retain(|animation| !animation.voice);
    for ability in abilities.iter().filter(|ability| ability.shout) {
        let action = ability.action.as_ref().ok_or("shout has no source action")?;
        for animation in voice.iter().filter(|animation| animation.event == action.event) {
            let mut animation = animation.clone();
            animation.prepare = action.prepare.clone();
            animation.ready = action.ready.clone();
            animation.interrupt = action.interrupt.clone();
            if animation.event_driven { animation.finish_event.clear(); }
            animation.allowed_spells = ability.words.iter().map(|word| word.source_spell.clone()).collect();
            animations.push(animation);
        }
    }
    spell_actions::bind(family, &abilities, records, interner, &mut animations)?;
    if animations.is_empty() {
        return Err(format!(
            "{} has active spells without a source casting cue",
            race.form_key.format(interner)
        ));
    }
    for ability in &abilities {
        if !animations.iter().any(|animation| {
            animation.voice == ability.shout
                && ability.action.as_ref().is_none_or(|action| animation.event == action.event)
                && (ability.shout || ability.words.iter().any(|word| animation.allowed_spells.contains(&word.source_spell)))
        }) {
            return Err(format!(
                "{} has no {} animation cue",
                ability.source,
                if ability.shout { "voice" } else { "spell" }
            ));
        }
    }
    Ok(Some(CreatureCastActor {
        npc: target_npc,
        race: target_race,
        source_race: race.form_key.format(interner),
        abilities,
        animations,
    }))
}

fn casting_animations(family: &SkyrimCreatureFamilyJob) -> Result<Vec<CreatureCastAnimation>, String> {
    let mut animations = Vec::new();
    let mut seen = BTreeSet::new();
    for role in &family.graph.roles {
        if !matches!(
            role.role,
            CreatureClipRole::ProjectileAttack
                | CreatureClipRole::ContinuousAttackStart
                | CreatureClipRole::ContinuousAttackLoop
        ) {
            continue;
        }
        let Some(event) = &role.trigger_event else {
            continue;
        };
        let Some(finish_event) = &family.graph.idle_event else {
            continue;
        };
        let prepare = match (&role.generator, event.as_str()) {
            (crate::source_rig::CapabilityRoleGenerator::Tree { .. }, "shoutStart")
                if family.family_id == "dragon" =>
            {
                Some("shoutRelease".to_string())
            }
            (crate::source_rig::CapabilityRoleGenerator::Tree { .. }, "shoutStartFireball")
                if family.family_id == "dragon" =>
            {
                Some("shoutRelease".to_string())
            }
            _ => None,
        };
        for clip_name in role.generator.clip_names(&role.clip_name) {
            let Some(clip) = family.clips.iter().find(|clip| clip.clip_id == clip_name) else {
                continue;
            };
            let mut cues = clip.annotations.iter().map(|annotation| (annotation.name.clone(), annotation.time)).collect::<Vec<_>>();
            for trigger in super::creature_motion::source_clip_triggers(&role.generator, &clip.clip_id) {
                let Some(name) = trigger.event.event.event_name else { continue; };
                let mut time = f32::from_bits(trigger.local_time_bits);
                if trigger.relative_to_end_of_clip { time += clip.source_duration.ok_or("source graph trigger has no animation duration")?; }
                cues.push((name, time));
            }
            cues.extend(super::creature_motion::source_clip_entry_events(&role.generator, &clip.clip_id).into_iter().map(|name| (name, 0.0)));
            cues.sort_by(|left, right| left.1.total_cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
            for (name, time) in &cues {
                let cue = normalize(name);
                if !matches!(
                    cue.as_str(),
                    "voicespellfireevent"
                        | "mlhspellfireevent"
                        | "mrhspellfireevent"
                        | "spellfireevent"
                        | "castrelease"
                ) {
                    continue;
                }
                if !time.is_finite()
                    || *time < 0.0
                    || !seen.insert((event.clone(), clip.clip_id.clone(), name.clone()))
                {
                    continue;
                }
                let stop = cues
                    .iter()
                    .filter(|item| item.1 >= *time)
                    .find(|item| {
                        matches!(
                            normalize(&item.0).as_str(),
                            "voicespellstopevent"
                                | "mlhspellendevent"
                                | "mrhspellendevent"
                                | "spellendevent"
                                | "caststop"
                        )
                    });
                animations.push(CreatureCastAnimation {
                    event: event.clone(),
                    prepare: prepare.clone(),
                    ready: None,
                    interrupt: None,
                    allowed_spells: Vec::new(),
                    event_driven: matches!(role.generator, crate::source_rig::CapabilityRoleGenerator::SourceCasting { .. }),
                    stop_events: match &role.generator { crate::source_rig::CapabilityRoleGenerator::SourceCasting { stop_events, .. } => stop_events.clone(), _ => Vec::new() },
                    observation_events: match &role.generator {
                        crate::source_rig::CapabilityRoleGenerator::SourceCasting { observations, .. } => observations.iter()
                            .filter(|observation| observation.release_clip_name == clip.clip_id)
                            .map(|observation| observation.target_event_name.clone()).collect(),
                        _ => Vec::new(),
                    },
                    finish_event: finish_event.clone(),
                    clip: clip.clip_id.clone(),
                    source_clip: clip.clip_path.clone(),
                    voice: cue.starts_with("voice"),
                    casting_source: if cue.starts_with("voice") { 2 } else if cue.starts_with("mlh") { 0 } else { 1 },
                    release: name.clone(),
                    release_time: *time,
                    clip_duration: clip.source_duration.filter(|duration| duration.is_finite() && *duration > *time)
                        .ok_or_else(|| format!("casting clip {} has no valid source duration after release", clip.clip_path))?,
                    looping: role.role == CreatureClipRole::ContinuousAttackLoop,
                    stop: stop
                        .map(|item| item.0.clone())
                        .or_else(|| prepare.as_ref().map(|_| "shoutStop".into())),
                    stop_time: stop.map(|item| item.1),
                });
            }
        }
    }
    Ok(animations)
}

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn is_daily_power(record: &Record, interner: &StringInterner) -> bool {
    spell_data(record, interner).is_some_and(|data| named(&data, "type", interner).is_some_and(|value| {
        matches!(value, FieldValue::Uint(2) | FieldValue::Int(2))
            || matches!(value, FieldValue::String(name) if interner.resolve(*name).is_some_and(|name| normalize(name) == "power"))
    }))
}

fn named<'a>(
    value: &'a FieldValue,
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    let FieldValue::Struct(pairs) = value else {
        return None;
    };
    pairs
        .iter()
        .find(|(key, _)| {
            interner
                .resolve(*key)
                .is_some_and(|value| normalize(&value) == normalize(name))
        })
        .map(|(_, value)| value)
}

fn field<'a>(record: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|field| field.sig.0 == *sig)
        .map(|field| &field.value)
}
fn spell_data<'a>(record: &'a Record, interner: &StringInterner) -> Option<std::borrow::Cow<'a, FieldValue>> {
    let data = field(record, b"SPIT")?;
    let FieldValue::Bytes(bytes) = data else { return Some(std::borrow::Cow::Borrowed(data)); };
    if bytes.len() != 36 { return None; }
    let fields = [("base_cost", 0, false), ("flags", 4, false), ("type", 8, false),
        ("charge_time", 12, true), ("cast_type", 16, false), ("target_type", 20, false),
        ("cast_duration", 24, true), ("range", 28, true)]
        .into_iter().map(|(name, offset, float)| {
            let bits = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            (interner.intern(name), if float { FieldValue::Float(f32::from_bits(bits)) } else { FieldValue::Uint(u64::from(bits)) })
        }).collect();
    Some(std::borrow::Cow::Owned(FieldValue::Struct(fields)))
}
fn form_key(value: &FieldValue) -> Option<FormKey> {
    if let FieldValue::FormKey(key) = value {
        Some(*key)
    } else {
        None
    }
}
fn number(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Uint(value) => Some(*value as f32),
        _ => None,
    }
}
fn semantic(value: Option<&FieldValue>, names: &[&str], interner: &StringInterner) -> Option<u32> {
    match value? {
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::String(value) => {
            let value = normalize(&interner.resolve(*value)?);
            if value == "voice" && names.get(11) == Some(&"voicepower") { return Some(11); }
            names
                .iter()
                .position(|name| *name == value)
                .map(|index| index as u32)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
