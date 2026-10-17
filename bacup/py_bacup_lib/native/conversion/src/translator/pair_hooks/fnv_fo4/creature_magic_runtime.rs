use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use serde::Serialize;

use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::run::ConversionRun;
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::translator::Game;

#[derive(Serialize)]
pub(crate) struct FnvRadiationOutput {
    version: u32,
    explosions: Vec<RadiationExplosion>,
    creature_races: BTreeSet<String>,
    settings: RadiationSettings,
    #[serde(skip)]
    plugin: String,
}

#[derive(Serialize)]
struct RadiationExplosion {
    explosion: String,
    level: f32,
    dissipation: f32,
    radius_bs: f32,
}

#[derive(Serialize)]
struct RadiationSettings {
    accumulation_rate: f32,
    decay_rate: f32,
    reevaluation_rate: f32,
    player_max_resistance: f32,
    avoid_threshold: f32,
}

impl FnvRadiationOutput {
    pub(crate) fn write(&self, mod_path: &Path) -> Result<(), String> {
        let directory = mod_path.join("F4SE/Plugins/B21_ConversionRuntime/FnvRadiation");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(directory.join(format!("{}.json", self.plugin)), bytes)
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn remove_stale_native_outputs(
    run: &ConversionRun,
    mod_path: &Path,
    radiation_present: bool,
    imagespace_present: bool,
) -> Result<(), String> {
    if run.source != Game::Fnv || run.target != Game::Fo4 || run.source_handle().is_none() {
        return Ok(());
    }
    let plugin = &run.config.output_plugin_name;
    if plugin.is_empty() || plugin.contains(['/', '\\', ':']) || matches!(plugin.as_str(), "." | "..") {
        return Err("FNV runtime catalog output plugin must be a file name".into());
    }
    for (directory, present) in [
        ("FnvRadiation", radiation_present),
        ("FnvImageSpace", imagespace_present),
    ] {
        if !present {
            let path = mod_path
                .join("F4SE/Plugins/B21_ConversionRuntime")
                .join(directory)
                .join(format!("{plugin}.json"));
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    Ok(())
}

pub(crate) fn prepare_fnv_radiation_output(
    run: &ConversionRun,
) -> Result<Option<FnvRadiationOutput>, String> {
    if run.source != Game::Fnv || run.target != Game::Fo4 || run.source_handle().is_none() {
        return Ok(None);
    }
    let source_explosions =
        iter_form_keys_of_sig(run.source_handle_id, SigCode(*b"EXPL"), &run.interner)
            .map_err(|error| error.to_string())?;
    let mut admitted = Vec::new();
    for key in source_explosions {
        if !run.is_admitted_creature_dependency(key, SigCode(*b"EXPL")) {
            continue;
        }
        let source = read_source(run, key)?;
        if radiation_scalars(&source)?.is_some() {
            admitted.push(source);
        }
    }
    if admitted.is_empty() {
        return Ok(None);
    }
    let mappings = &run
        .mapper_state
        .as_ref()
        .ok_or("FNV radiation mapper is unavailable")?
        .source_to_target;
    let settings = source_settings(run)?;
    let mut output = FnvRadiationOutput {
        version: 1,
        explosions: Vec::new(),
        creature_races: BTreeSet::new(),
        settings: RadiationSettings {
            accumulation_rate: setting(&settings, "fRadiationAccumulationRate", 1.0)?,
            decay_rate: setting(&settings, "fRadiationDecayRate", 0.01)?,
            reevaluation_rate: setting(&settings, "fRadiationReevaluationRate", 2.0)?,
            player_max_resistance: setting(&settings, "fPlayerMaxResistance", 85.0)?,
            avoid_threshold: setting(&settings, "fRadiationAvoidThreshold", 50.0)?,
        },
        plugin: run.config.output_plugin_name.clone(),
    };
    let units_per_foot = setting(&settings, "fBSUnitsPerFoot", 22.0)?;
    for source in admitted {
        let target = *mappings.get(&source.form_key).ok_or_else(|| {
            format!(
                "unmapped FNV radiation explosion {}",
                source.form_key.format(&run.interner)
            )
        })?;
        require_emitted(run, target, *b"EXPL")?;
        let (level, dissipation, radius, radius_in_bs_units) = radiation_scalars(&source)?.unwrap();
        output.explosions.push(RadiationExplosion {
            explosion: catalog_key(target, &run.interner),
            level,
            dissipation,
            radius_bs: if radius_in_bs_units {
                radius
            } else {
                radius * units_per_foot
            },
        });
    }
    output
        .explosions
        .sort_by(|left, right| left.explosion.cmp(&right.explosion));
    let source_creatures =
        iter_form_keys_of_sig(run.source_handle_id, SigCode(*b"CREA"), &run.interner)
            .map_err(|error| error.to_string())?;
    let mut target_creatures = HashSet::new();
    for source in &source_creatures {
        if let Some(target) = mappings.get(source) {
            target_creatures.insert(*target);
        }
    }
    for reservation in run.creature_ancillary_npc_reservations() {
        if source_creatures.contains(&reservation.source) {
            target_creatures.insert(reservation.target);
        }
    }
    for target in target_creatures {
        let Some(npc) = run
            .read_target_record_if_available(target)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        if npc.sig.0 != *b"NPC_" {
            return Err(format!(
                "FNV creature {} is not an emitted NPC",
                target.format(&run.interner)
            ));
        }
        let Some(FieldValue::FormKey(race)) = value(&npc, b"RNAM") else {
            return Err(format!(
                "FNV creature {} has no emitted race",
                target.format(&run.interner)
            ));
        };
        require_emitted(run, *race, *b"RACE")?;
        output
            .creature_races
            .insert(catalog_key(*race, &run.interner));
    }
    Ok(Some(output))
}

fn require_emitted(run: &ConversionRun, key: FormKey, sig: [u8; 4]) -> Result<(), String> {
    if !run
        .interner
        .resolve(key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(&run.config.output_plugin_name))
        || run
            .read_target_record_if_available(key)
            .map_err(|error| error.to_string())?
            .is_none_or(|record| record.sig.0 != sig)
    {
        return Err(format!(
            "FNV radiation dependency {} was not emitted",
            key.format(&run.interner)
        ));
    }
    Ok(())
}

fn read_source(run: &ConversionRun, key: FormKey) -> Result<Record, String> {
    read_record_relayout_by_form_key(
        run.source_handle_id,
        &key,
        &run.schema_source,
        &run.interner,
        None,
    )
    .map_err(|error| error.to_string())
}

fn radiation_scalars(record: &Record) -> Result<Option<(f32, f32, f32, bool)>, String> {
    let Some(FieldValue::Bytes(bytes)) = value(record, b"DATA") else {
        return Err("source radiation explosion has no raw DATA".into());
    };
    if bytes.len() != 52 {
        return Err(format!(
            "source radiation explosion DATA has {} bytes",
            bytes.len()
        ));
    }
    let [level, dissipation, radius] = [36, 40, 44]
        .map(|offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()));
    if [level, dissipation, radius]
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err("source radiation explosion has invalid radiation scalars".into());
    }
    if level == 0.0 {
        return Ok(None);
    }
    let flags = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    Ok(Some((level, dissipation, radius, flags & 1 != 0)))
}

pub(super) fn source_settings(run: &ConversionRun) -> Result<Vec<(String, FieldValue)>, String> {
    let mut settings = Vec::new();
    for key in iter_form_keys_of_sig(run.source_handle_id, SigCode(*b"GMST"), &run.interner)
        .map_err(|error| error.to_string())?
    {
        let record = read_source(run, key)?;
        if let (Some(name), Some(data)) = (
            record.eid.and_then(|eid| run.interner.resolve(eid)),
            value(&record, b"DATA"),
        ) {
            settings.push((name.to_owned(), data.clone()));
        }
    }
    Ok(settings)
}

pub(super) fn setting(settings: &[(String, FieldValue)], name: &str, default: f32) -> Result<f32, String> {
    let Some((_, value)) = settings
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
    else {
        return Ok(default);
    };
    let scalar = match value {
        FieldValue::Float(value) => *value,
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            f32::from_le_bytes(bytes[..].try_into().unwrap())
        }
        _ => return Err(format!("source setting {name} is not a float")),
    };
    if !scalar.is_finite() || scalar < 0.0 {
        return Err(format!("source setting {name} has invalid value {scalar}"));
    }
    Ok(scalar)
}

fn value<'a>(record: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|field| field.sig.0 == *sig)
        .map(|field| &field.value)
}

fn catalog_key(key: FormKey, interner: &StringInterner) -> String {
    format!(
        "{:06X}:{}",
        key.local,
        interner.resolve(key.plugin).unwrap()
    )
}

#[cfg(test)]
mod tests;
