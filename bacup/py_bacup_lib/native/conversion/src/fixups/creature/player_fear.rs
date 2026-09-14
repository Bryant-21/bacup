use std::collections::HashSet;
use std::path::Path;

use ck_native::anim_text_data::bucket_files::{
    ClipGenEntry, ClipTrigger, clip_generator_data_body,
};
use ck_native::anim_text_data::core::name_id;
use ck_native::anim_text_data::emit::SubgraphInput;
use ck_native::anim_text_data::extract::clip_generator_entries;

use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

pub const EXPLOSION_EID: &str = "crexplosionwendigocolossusfear";
const ANCHOR_MODEL: &str = r"B21\CreatureCombat\B21_FearExplosionAnchor.nif";
const ANCHOR_NIF: &[u8] = include_bytes!("player_fear/B21_FearExplosionAnchor.nif");

pub fn repair_explosion(
    record: &mut Record,
    interner: &StringInterner,
    mod_path: Option<&Path>,
) -> Result<bool, String> {
    if record.sig.0 != *b"EXPL"
        || !record
            .eid
            .and_then(|eid| interner.resolve(eid))
            .is_some_and(|eid| eid.eq_ignore_ascii_case(EXPLOSION_EID))
    {
        return Ok(false);
    }
    let model_sig = SubrecordSig::from_str("MODL").unwrap();
    let model = record
        .fields
        .iter()
        .position(|field| field.sig == model_sig);
    let changed = match model.map(|i| &record.fields[i].value) {
        Some(FieldValue::String(value)) => match interner.resolve(*value) {
            Some(value) if value.eq_ignore_ascii_case(ANCHOR_MODEL) => false,
            Some("") => true,
            _ => return Ok(false),
        },
        Some(FieldValue::None) | None => true,
        _ => return Ok(false),
    };
    // FO4 never initializes a meshless explosion, so its enchantment never runs.
    if let Some(mod_path) = mod_path {
        let path = mod_path
            .join("data/Meshes")
            .join(ANCHOR_MODEL.replace('\\', "/"));
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&path, ANCHOR_NIF).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    if changed {
        let entry = FieldEntry {
            sig: model_sig,
            value: FieldValue::String(interner.intern(ANCHOR_MODEL)),
        };
        if let Some(index) = model {
            record.fields[index] = entry;
        } else {
            let index = record
                .fields
                .iter()
                .position(|field| matches!(&field.sig.0, b"EITM" | b"DATA"))
                .unwrap_or(record.fields.len());
            record.fields.insert(index, entry);
        }
    }
    Ok(changed)
}

fn add_selection_time(entries: &mut [ClipGenEntry]) -> bool {
    let mut changed = false;
    for entry in entries {
        if !entry.clip_name.eq_ignore_ascii_case("AttackFear")
            || !entry.anim_name.eq_ignore_ascii_case("AttackFear")
            || entry
                .triggers
                .iter()
                .any(|trigger| trigger.name.eq_ignore_ascii_case("HitFrame"))
        {
            continue;
        }
        if let Some(donor) = entry
            .triggers
            .iter()
            .find(|trigger| trigger.name == "SpawnExplosionOnGround")
        {
            // Attack selection needs HitFrame metadata; emitting it in HKX would cause melee damage.
            let trigger = ClipTrigger {
                name: "HitFrame".into(),
                time: donor.time,
                flag: donor.flag,
            };
            entry.triggers.push(trigger);
            changed = true;
        }
    }
    changed
}

pub fn write_selection_timing(
    subgraphs: &[SubgraphInput],
    src_meshes: &Path,
    out_meshes: &Path,
) -> Result<(), String> {
    let mut seen = HashSet::new();
    for subgraph in subgraphs {
        let core = &subgraph.core_behavior;
        if !core
            .to_ascii_lowercase()
            .contains("wendigocolossuscorebehavior")
            || !seen.insert(name_id(core))
        {
            continue;
        }
        let path = out_meshes.join(format!(
            "AnimTextData/ClipGeneratorData/{}.txt",
            name_id(core)
        ));
        if !path.is_file() {
            continue;
        }
        let mut entries = clip_generator_entries(&src_meshes.join(core.replace('\\', "/")));
        if add_selection_time(&mut entries) {
            std::fs::write(&path, clip_generator_data_body(core, &entries))
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
