use std::collections::BTreeSet;
use std::path::Path;

use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::run::ConversionRun;
use crate::source_rig::creature_weapon_runtime::{
    CreatureWeaponRuntimeCatalog, CreatureWeaponRuntimeKind, build_creature_weapon_runtime_catalog,
};
use crate::source_rig::{SourceRigExecutableRecipe, TargetFormKey};
use crate::sym::StringInterner;

pub(crate) fn publish_creature_weapon_catalog(
    run: &ConversionRun,
    recipes: &[SourceRigExecutableRecipe],
    mod_root: &Path,
) -> Result<(), String> {
    let catalog = build_creature_weapon_runtime_catalog(recipes)?;
    let directory = mod_root.join("F4SE/Plugins/B21_ConversionRuntime");
    let stem = Path::new(&run.config.output_plugin_name)
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("Creature weapon output plugin has no filename")?;
    let path = directory.join(format!("{stem}.creature-weapons.json"));
    if catalog.actors.iter().all(|actor| actor.attacks.is_empty()) {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    }
    let data = runtime_catalog(&catalog, &run.interner, |target| {
        run.read_target_record_if_available(FormKey {
            local: target.local,
            plugin: run.interner.intern(&target.plugin),
        })
        .map_err(|error| error.to_string())
    })?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&data).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn runtime_catalog(
    catalog: &CreatureWeaponRuntimeCatalog,
    interner: &StringInterner,
    mut lookup: impl FnMut(&TargetFormKey) -> Result<Option<Record>, String>,
) -> Result<serde_json::Value, String> {
    if catalog.version != 1 {
        return Err("Unsupported creature weapon catalog version".into());
    }
    let mut output = serde_json::to_value(catalog).map_err(|error| error.to_string())?;
    let mut seen = BTreeSet::new();
    for (actor, data) in catalog
        .actors
        .iter()
        .zip(output["actors"].as_array_mut().unwrap())
    {
        if !seen.insert(actor.npc.clone()) {
            return Err(format!(
                "Duplicate creature weapon actor {}",
                runtime_key(&actor.npc)
            ));
        }
        let npc = target_record(&actor.npc, SigCode(*b"NPC_"), &mut lookup)?;
        let race = target_record(&actor.race, SigCode(*b"RACE"), &mut lookup)?;
        if !npc.fields.iter().any(|field| {
            field.sig.as_str() == "RNAM" && target_link(&field.value, &actor.race, interner)
        }) {
            return Err(format!(
                "Creature weapon actor {} does not use race {}",
                runtime_key(&actor.npc),
                runtime_key(&actor.race)
            ));
        }
        let slots = race
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "QNAM")
            .map(|field| &field.value)
            .collect::<Vec<_>>();
        let mut events = BTreeSet::new();
        data["npc"] = runtime_key(&actor.npc).into();
        data["race"] = runtime_key(&actor.race).into();
        for (attack, data) in actor
            .attacks
            .iter()
            .zip(data["attacks"].as_array_mut().unwrap())
        {
            if attack.event.is_empty() || !events.insert(&attack.event) {
                return Err(format!(
                    "Creature weapon race {} has an empty or duplicate attack event",
                    runtime_key(&actor.race)
                ));
            }
            let slot = slots.get(usize::from(attack.equip_index)).ok_or_else(|| {
                format!(
                    "Creature attack {} equipment index is absent from its race",
                    attack.event
                )
            })?;
            let mut matching_slots = Vec::new();
            let mut matching_event = false;
            for field in &race.fields {
                match field.sig.as_str() {
                    "ATKD" => matching_event = false,
                    "ATKE" => {
                        matching_event = field_text(&field.value, interner).as_deref()
                            == Some(attack.event.as_str())
                    }
                    "ATKW" if matching_event => matching_slots.push(&field.value),
                    _ => {}
                }
            }
            if matching_slots.as_slice() != [*slot] {
                return Err(format!(
                    "Creature attack {} does not have exactly its indexed RACE ATKW binding",
                    attack.event
                ));
            }
            if (attack.kind == CreatureWeaponRuntimeKind::Natural) != attack.weapons.is_empty() {
                return Err(format!(
                    "Creature attack {} has inconsistent weapon ownership",
                    attack.event
                ));
            }
            if let Some(gate) = &attack.graph_gate
                && gate.name.is_empty()
            {
                return Err(format!(
                    "Creature attack {} has an empty graph gate",
                    attack.event
                ));
            }
            let mut weapons = BTreeSet::new();
            for (weapon, data) in attack
                .weapons
                .iter()
                .zip(data["weapons"].as_array_mut().unwrap())
            {
                if !weapons.insert(weapon) {
                    return Err(format!("Creature attack {} repeats a weapon", attack.event));
                }
                target_record(weapon, SigCode(*b"WEAP"), &mut lookup)?;
                *data = runtime_key(weapon).into();
            }
        }
    }
    Ok(output)
}

fn target_record(
    target: &TargetFormKey,
    signature: SigCode,
    lookup: &mut impl FnMut(&TargetFormKey) -> Result<Option<Record>, String>,
) -> Result<Record, String> {
    if target.local != 0
        && let Some(record) = lookup(target)?
        && record.sig == signature
    {
        return Ok(record);
    }
    Err(format!(
        "Creature weapon target {} must resolve to {}",
        runtime_key(target),
        signature.as_str()
    ))
}

fn target_link(value: &FieldValue, target: &TargetFormKey, interner: &StringInterner) -> bool {
    matches!(value, FieldValue::FormKey(key) if key.local == target.local
        && interner.resolve(key.plugin).is_some_and(|plugin| plugin.eq_ignore_ascii_case(&target.plugin)))
}

fn field_text(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(value) => interner.resolve(*value).map(str::to_owned),
        FieldValue::Bytes(bytes) => std::str::from_utf8(bytes)
            .ok()
            .map(|value| value.trim_end_matches('\0').to_owned()),
        _ => None,
    }
}

fn runtime_key(target: &TargetFormKey) -> String {
    format!("{:06X}:{}", target.local, target.plugin)
}
