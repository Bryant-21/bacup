use std::path::Path;

use crate::ids::{FormKey, SigCode};
use crate::run::ConversionRun;
use crate::skyrimse_fo4_runtime::creature_casting::CreatureCastCatalog;
use crate::source_rig::TargetFormKey;

pub(crate) fn publish_creature_cast_catalog(
    run: &ConversionRun,
    staged_path: &Path,
    mod_path: &Path,
) -> Result<(), String> {
    let catalog: CreatureCastCatalog = serde_json::from_slice(
        &std::fs::read(staged_path)
            .map_err(|error| format!("{}: {error}", staged_path.display()))?,
    )
    .map_err(|error| error.to_string())?;
    let data = runtime_catalog(&catalog, |target, signature| {
        let key = FormKey {
            local: target.local,
            plugin: run.interner.intern(&target.plugin),
        };
        run.read_target_record_if_available(key)
            .map(|record| record.is_some_and(|record| record.sig == signature))
            .map_err(|error| error.to_string())
    })?;
    let directory = mod_path.join("F4SE/Plugins/B21_ConversionRuntime");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let stem = Path::new(&run.config.output_plugin_name)
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("Creature cast output plugin has no filename")?;
    std::fs::write(
        directory.join(format!("{stem}.creature-casts.json")),
        serde_json::to_vec_pretty(&data).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn runtime_catalog(
    catalog: &CreatureCastCatalog,
    mut exists: impl FnMut(&TargetFormKey, SigCode) -> Result<bool, String>,
) -> Result<serde_json::Value, String> {
    let mut data = serde_json::to_value(catalog).map_err(|error| error.to_string())?;
    let mut seen = std::collections::BTreeSet::new();
    for (actor, output) in catalog
        .actors
        .iter()
        .zip(data["actors"].as_array_mut().unwrap())
    {
        if !seen.insert((actor.npc.clone(), actor.race.clone())) {
            return Err(format!("Duplicate creature casting actor {:?}", actor.npc));
        }
        validate_target(&actor.npc, SigCode(*b"NPC_"), &mut exists)?;
        validate_target(&actor.race, SigCode(*b"RACE"), &mut exists)?;
        output["npc"] = runtime_key(&actor.npc).into();
        output["race"] = runtime_key(&actor.race).into();
        for (ability, output) in actor
            .abilities
            .iter()
            .zip(output["abilities"].as_array_mut().unwrap())
        {
            for (word, output) in ability
                .words
                .iter()
                .zip(output["words"].as_array_mut().unwrap())
            {
                validate_target(&word.spell, SigCode(*b"SPEL"), &mut exists)?;
                output["spell"] = runtime_key(&word.spell).into();
            }
        }
    }
    Ok(data)
}

fn validate_target(
    target: &TargetFormKey,
    signature: SigCode,
    exists: &mut impl FnMut(&TargetFormKey, SigCode) -> Result<bool, String>,
) -> Result<(), String> {
    if target.local == 0 || !exists(target, signature)? {
        return Err(format!(
            "Creature cast target {} must resolve to {}",
            runtime_key(target),
            signature.as_str()
        ));
    }
    Ok(())
}

fn runtime_key(target: &TargetFormKey) -> String {
    format!("{:06X}:{}", target.local, target.plugin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> CreatureCastCatalog {
        serde_json::from_value(serde_json::json!({
            "actors": [{
                "npc": "000800@Converted.esm", "race": "000802@Converted.esm", "source_race": "DragonRace",
                "abilities": [{
                    "source": "000123@Skyrim.esm", "shout": true,
                    "once_per_day": false, "require_known": false,
                    "words": [{"spell": "000801@Converted.esm", "source_spell": "000456@Skyrim.esm",
                        "ordinal": 0, "word_present": true, "recovery": 5.0,
                        "cast_type": 1, "delivery": 2, "duration": 1.0, "range": 100.0}]
                }], "animations": []
            }]
        }))
        .unwrap()
    }

    #[test]
    fn casting_publication_validates_target_types_and_runtime_formkeys() {
        let mut visited = Vec::new();
        let output = runtime_catalog(&catalog(), |key, signature| {
            visited.push((key.local, signature.0));
            Ok(true)
        })
        .unwrap();
        assert_eq!(
            visited,
            [(0x800, *b"NPC_"), (0x802, *b"RACE"), (0x801, *b"SPEL")]
        );
        assert_eq!(output["actors"][0]["npc"], "000800:Converted.esm");
        assert_eq!(output["actors"][0]["race"], "000802:Converted.esm");
        assert_eq!(
            output["actors"][0]["abilities"][0]["words"][0]["spell"],
            "000801:Converted.esm"
        );
        assert_eq!(
            output["actors"][0]["abilities"][0]["source"],
            "000123@Skyrim.esm"
        );
    }

    #[test]
    fn casting_publication_rejects_missing_spells_and_duplicate_actors() {
        assert!(runtime_catalog(&catalog(), |_, signature| Ok(signature.0 == *b"NPC_")).is_err());
        let mut repeated = catalog();
        repeated.actors.push(repeated.actors[0].clone());
        assert!(runtime_catalog(&repeated, |_, _| Ok(true)).is_err());
    }
}
