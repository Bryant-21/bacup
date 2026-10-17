use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record, RecordFlags};
use crate::run::{ConversionRun, SkyrimCreatureReservedRecordKind};
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::source_rig::TargetFormKey;
use crate::sym::StringInterner;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProjectedRaceStagger {
    pub(crate) race: TargetFormKey,
    pub(crate) attacks: Vec<ProjectedAttackStagger>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct ProjectedAttackStagger {
    pub(crate) event: String,
    pub(crate) source_event: String,
    pub(crate) stagger: f32,
}

#[derive(Default, Serialize)]
pub(crate) struct BowStaggerOutput {
    weapons: Vec<SourceWeaponStagger>,
    races: Vec<SourceRaceStagger>,
    armor: Vec<SourceArmorRating>,
    settings: BTreeMap<String, f32>,
}

impl BowStaggerOutput {
    pub(crate) fn is_empty(&self) -> bool {
        self.weapons.is_empty() && self.races.is_empty() && self.armor.is_empty()
    }
}

#[derive(PartialEq, Serialize)]
struct SourceWeaponStagger {
    weapon: String,
    source_weapon: String,
    animation_type: u8,
    stagger: f32,
}

#[derive(PartialEq, Serialize)]
struct SourceRaceStagger {
    race: String,
    source_race: String,
    mass: f32,
    attacks: Vec<ProjectedAttackStagger>,
}

#[derive(PartialEq, Serialize)]
struct SourceArmorRating {
    armor: String,
    source_armor: String,
    armor_rating: f32,
}

// Skyrim's executable supplies these defaults when no winning GMST overrides them.
const SOURCE_SETTINGS: [(&str, f32); 13] = [
    ("fStaggerPlayerMassMult", 2.0),
    ("fStaggerMassMult", 1.0),
    ("fStaggerMassBase", 0.0),
    ("fStaggerMassOffsetBase", 3.0),
    ("fStaggerMassOffsetMult", 0.15),
    ("fStaggerBlockAttackShieldMult", -0.005),
    ("fStaggerBlockAttackShieldBase", 1.0),
    ("fStaggerBlockMult", 0.005),
    ("fStaggerBlockBase", 0.5),
    ("fStagger1WeapAR", 10.0),
    ("fStaggerDaggerAR", 5.0),
    ("fStagger2WeapAR", 15.0),
    ("fStaggerBowAR", 5.0),
];

pub(crate) fn prepare_bow_stagger_output(run: &ConversionRun) -> Result<BowStaggerOutput, String> {
    let mappings = &run
        .mapper_state
        .as_ref()
        .ok_or("Bow stagger mapper is unavailable")?
        .source_to_target;
    let output_plugin = run.interner.intern(&run.config.output_plugin_name);
    let read_source = |key: FormKey| {
        read_record_relayout_by_form_key(
            run.source_handle_id,
            &key,
            &run.schema_source,
            &run.interner,
            None,
        )
        .map_err(|error| error.to_string())
    };
    let mut weapons = BTreeMap::new();
    let mut armor = BTreeMap::new();
    for sig in [*b"WEAP", *b"ARMO"] {
        for source_key in iter_form_keys_of_sig(run.source_handle_id, SigCode(sig), &run.interner)
            .map_err(|error| error.to_string())?
        {
            let Some(target_key) = mappings
                .get(&source_key)
                .filter(|key| key.plugin == output_plugin)
            else {
                continue;
            };
            let Some(target) = run
                .read_target_record_if_available(*target_key)
                .map_err(|error| error.to_string())?
            else {
                continue;
            };
            if target.flags.contains(RecordFlags::DELETED) {
                continue;
            }
            if target.sig.0 != sig {
                return Err(format!(
                    "Bow stagger source {} mapped to the wrong target record type",
                    source_key.format(&run.interner)
                ));
            }
            let source = read_source(source_key)?;
            if source.flags.contains(RecordFlags::DELETED) {
                continue;
            }
            let target_name = catalog_key(*target_key, &run.interner);
            if sig == *b"WEAP" {
                let source = resolve_weapon_template(source, read_source)?;
                let (animation_type, stagger) = weapon_stagger(&source, &run.interner)?;
                let row = SourceWeaponStagger {
                    weapon: target_name.clone(),
                    source_weapon: catalog_key(source_key, &run.interner),
                    animation_type,
                    stagger,
                };
                if weapons.insert(target_name.clone(), row).is_some() {
                    return Err(format!(
                        "Bow stagger weapon {target_name} has duplicate source mappings"
                    ));
                }
            } else {
                let row = SourceArmorRating {
                    armor: target_name.clone(),
                    source_armor: catalog_key(source_key, &run.interner),
                    armor_rating: armor_rating(&source, &run.interner)?,
                };
                if armor.insert(target_name.clone(), row).is_some() {
                    return Err(format!(
                        "Bow stagger armor {target_name} has duplicate source mappings"
                    ));
                }
            }
        }
    }
    let mut races = BTreeMap::new();
    for projected in &run.skyrim_bow_stagger_races {
        let target_key = FormKey {
            local: projected.race.local,
            plugin: run.interner.intern(&projected.race.plugin),
        };
        if target_key.plugin != output_plugin {
            return Err("Bow stagger race belongs to a target donor plugin".into());
        }
        let source_key = run
            .skyrim_creature_record_reservations()
            .iter()
            .find(|row| {
                row.kind == SkyrimCreatureReservedRecordKind::Race && row.target == target_key
            })
            .map(|row| row.source)
            .ok_or("Bow stagger race has no source-owned creature reservation")?;
        let target = run
            .read_target_record_if_available(target_key)
            .map_err(|error| error.to_string())?
            .filter(|record| {
                record.sig.0 == *b"RACE" && !record.flags.contains(RecordFlags::DELETED)
            })
            .ok_or("Bow stagger race has no live emitted RACE")?;
        let events: HashSet<_> = target
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"ATKE")
            .filter_map(|entry| match entry.value {
                FieldValue::String(value) => run.interner.resolve(value),
                _ => None,
            })
            .collect();
        for attack in &projected.attacks {
            if !attack.stagger.is_finite() || !events.contains(attack.event.as_str()) {
                return Err(format!(
                    "Bow stagger projected attack {} is not preserved in its target RACE",
                    attack.event
                ));
            }
        }
        let source = read_source(source_key)?;
        let name = catalog_key(target_key, &run.interner);
        let row = SourceRaceStagger {
            race: name.clone(),
            source_race: catalog_key(source_key, &run.interner),
            mass: race_mass(&source, &run.interner)?,
            attacks: projected.attacks.clone(),
        };
        if let Some(previous) = races.insert(name.clone(), row) {
            if races.get(&name) != Some(&previous) {
                return Err(format!(
                    "Bow stagger race {name} has conflicting projected attacks"
                ));
            }
        }
    }
    let mut settings: BTreeMap<_, _> = SOURCE_SETTINGS
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    let mut overrides = HashSet::new();
    for key in iter_form_keys_of_sig(run.source_handle_id, SigCode(*b"GMST"), &run.interner)
        .map_err(|error| error.to_string())?
    {
        let source = read_source(key)?;
        if source.flags.contains(RecordFlags::DELETED) {
            continue;
        }
        let Some(name) = source.eid.and_then(|name| run.interner.resolve(name)) else {
            continue;
        };
        let Some((canonical, _)) = SOURCE_SETTINGS
            .iter()
            .find(|(setting, _)| setting.eq_ignore_ascii_case(name))
        else {
            continue;
        };
        if !overrides.insert(*canonical) {
            return Err(format!(
                "Bow stagger setting {canonical} has conflicting winning records"
            ));
        }
        settings.insert(
            (*canonical).into(),
            scalar_float(field(&source, b"DATA"), canonical)?,
        );
    }
    Ok(BowStaggerOutput {
        weapons: weapons.into_values().collect(),
        races: races.into_values().collect(),
        armor: armor.into_values().collect(),
        settings,
    })
}

fn resolve_weapon_template(
    mut source: Record,
    mut read: impl FnMut(FormKey) -> Result<Record, String>,
) -> Result<Record, String> {
    let mut visited = HashSet::new();
    loop {
        if source.sig.0 != *b"WEAP"
            || source.flags.contains(RecordFlags::DELETED)
            || !visited.insert(source.form_key)
        {
            return Err("Bow stagger weapon template is cyclic or not a live WEAP".into());
        }
        match field(&source, b"CNAM") {
            Some(FieldValue::FormKey(key)) if key.local != 0 => source = read(*key)?,
            None
            | Some(
                FieldValue::None
                | FieldValue::Uint(0)
                | FieldValue::Int(0)
                | FieldValue::FormKey(_),
            ) => return Ok(source),
            _ => return Err("Bow stagger weapon template is not a resolved source FormKey".into()),
        }
    }
}

fn weapon_stagger(source: &Record, interner: &StringInterner) -> Result<(u8, f32), String> {
    let (animation_type, stagger) = match field(source, b"DNAM") {
        Some(FieldValue::Bytes(bytes)) if bytes.len() == 100 => (
            u64::from(bytes[0]),
            f32::from_le_bytes(bytes[96..100].try_into().unwrap()),
        ),
        Some(FieldValue::Struct(fields)) => {
            let animation_type = match named(fields, "animationtype", interner) {
                Some(FieldValue::Uint(value)) => *value,
                Some(FieldValue::Int(value)) => {
                    u64::try_from(*value).map_err(|_| "Invalid source weapon animation type")?
                }
                _ => return Err("Source WEAP DNAM has no animation type".into()),
            };
            (
                animation_type,
                scalar_float(named(fields, "stagger", interner), "WEAP stagger")?,
            )
        }
        _ => return Err("Source WEAP DNAM has no supported stagger layout".into()),
    };
    if animation_type > 9 || !stagger.is_finite() {
        return Err("Source WEAP has an invalid animation type or stagger".into());
    }
    Ok((animation_type as u8, stagger))
}

fn race_mass(source: &Record, interner: &StringInterner) -> Result<f32, String> {
    let mass = match field(source, b"DATA") {
        Some(FieldValue::Bytes(bytes)) if bytes.len() >= 128 => {
            f32::from_le_bytes(bytes[52..56].try_into().unwrap())
        }
        Some(FieldValue::Struct(fields)) => {
            scalar_float(named(fields, "basemass", interner), "RACE base mass")?
        }
        _ => return Err("Source RACE DATA has no supported base mass layout".into()),
    };
    if source.sig.0 != *b"RACE" || source.flags.contains(RecordFlags::DELETED) || !mass.is_finite()
    {
        return Err("Source RACE base mass is invalid".into());
    }
    Ok(mass)
}

fn armor_rating(source: &Record, interner: &StringInterner) -> Result<f32, String> {
    let mut value = field(source, b"DNAM");
    if let Some(FieldValue::Struct(fields)) = value {
        value = named(fields, "armorrating", interner);
    }
    let raw = match value {
        Some(FieldValue::Bytes(bytes)) if bytes.len() == 4 => {
            u32::from_le_bytes(bytes[..].try_into().unwrap())
        }
        Some(FieldValue::Int(value)) => {
            i32::try_from(*value).map_err(|_| "Source armor rating exceeds int32")? as u32
        }
        Some(FieldValue::Uint(value)) => {
            u32::try_from(*value).map_err(|_| "Source armor rating exceeds uint32")?
        }
        None => 0,
        _ => return Err("Source ARMO DNAM is not an armor rating".into()),
    };
    // The source loader copies DNAM; CalcStagger consumes its low uint16 at 0.01 scale.
    Ok((raw as u16) as f32 * 0.01)
}

fn scalar_float(value: Option<&FieldValue>, label: &str) -> Result<f32, String> {
    let number = match value {
        Some(FieldValue::Float(value)) => *value,
        Some(FieldValue::Bytes(bytes)) if bytes.len() == 4 => {
            f32::from_le_bytes(bytes[..].try_into().unwrap())
        }
        _ => return Err(format!("{label} is not a source float")),
    };
    number
        .is_finite()
        .then_some(number)
        .ok_or_else(|| format!("{label} is not finite"))
}

fn field<'a>(source: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    source
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *sig)
        .map(|entry| &entry.value)
}

fn named<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields.iter().find_map(|(key, value)| {
        interner
            .resolve(*key)
            .is_some_and(|key| {
                key.chars()
                    .filter(|ch| ch.is_ascii_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .eq(name.chars())
            })
            .then_some(value)
    })
}

fn catalog_key(key: FormKey, interner: &StringInterner) -> String {
    format!(
        "{:06X}:{}",
        key.local,
        interner.resolve(key.plugin).unwrap()
    )
}

#[cfg(test)]
#[path = "bow_stagger_tests.rs"]
mod tests;
