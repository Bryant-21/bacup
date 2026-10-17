use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde_json::json;

use crate::fixups::FixupConfig;
use crate::fixups::face::legacy_weapon_profiles::{
    LegacyWeaponProfile, LegacyWeaponProfileRegistration, discover_legacy_weapon_donor_profiles,
    legacy_weapon_donor, register_legacy_weapon_profiles, weapon_perspective,
};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::Game;

pub(crate) struct ConvertSkyrimWeaponAnimationsPhase;

impl Phase for ConvertSkyrimWeaponAnimationsPhase {
    fn name(&self) -> &'static str {
        "convert_skyrim_weapon_animations"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::SkyrimSe || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        ctx.check_cancel()?;
        let target_root = ctx.target_extracted_dir.ok_or_else(|| {
            PhaseError::BadParams(
                "Skyrim weapon animations require extracted Fallout 4 contracts".into(),
            )
        })?;
        let config = FixupConfig {
            target_master_handle_ids: ctx.run.master_handle_ids.clone(),
            target_schema: Some(ctx.run.schema_target.clone()),
            source_schema: Some(ctx.run.schema_source.clone()),
            target_extracted_dir: Some(target_root.into()),
            mod_path: Some(ctx.mod_path.into()),
            ..Default::default()
        };
        let mut session =
            crate::session::open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let state = ctx.run.mapper_state.as_mut().ok_or_else(|| {
            PhaseError::Internal("Skyrim weapon animations require retained mapper state".into())
        })?;
        let mut mapper = FormKeyMapper::from_state(state, &ctx.run.interner);
        let output_plugin = mapper.output_plugin_sym();
        let source_schema = ctx.run.schema_source.clone();
        let keys = session
            .source_form_keys_of_sig(SigCode(*b"WEAP"), mapper.interner)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let mut registrations = Vec::new();
        let mut receipts = Vec::new();
        let mut assets = BTreeMap::<String, Vec<u8>>::new();
        let mut profiles = HashMap::<(u8, bool, String), (String, Vec<String>)>::new();
        let prefix = ctx
            .params
            .get("mod_prefix")
            .and_then(|value| value.as_str())
            .unwrap_or("B21_Skyrim");
        let donor = legacy_weapon_donor(5, 0, false, mapper.interner)
            .ok_or_else(|| PhaseError::Internal("missing FO4 rifle profile contract".into()))?;
        let mut donors = None;
        for source_key in keys {
            if ctx.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(PhaseError::Cancelled);
            }
            let Some(target_key) = mapper
                .lookup(source_key)
                .filter(|key| key.plugin == output_plugin)
            else {
                continue;
            };
            let source = session
                .source_record_decoded(&source_key, &source_schema, mapper.interner)
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            let Some(category @ (7 | 9)) = weapon_category(&source, mapper.interner) else {
                continue;
            };
            if donors.is_none() {
                donors = Some(
                    discover_legacy_weapon_donor_profiles(
                        &mut session,
                        &mut mapper,
                        &config,
                        donor,
                    )
                    .map_err(|e| PhaseError::Internal(e.to_string()))?,
                );
            }
            let donors = donors.as_ref().unwrap();
            if [0, 1].into_iter().any(|perspective| {
                !donors
                    .iter()
                    .any(|donor| weapon_perspective(donor) == Some(perspective))
            }) {
                return Err(PhaseError::Internal(
                    "Skyrim weapon donor lacks both actor perspectives".into(),
                ));
            }
            let mut registered_profiles = Vec::new();
            for donor in donors {
                let first = weapon_perspective(donor) == Some(1);
                let graph = mapper
                    .interner
                    .resolve(donor.behaviour_graph)
                    .ok_or_else(|| {
                        PhaseError::Internal("unresolved FO4 weapon donor graph".into())
                    })?
                    .replace('\\', "/");
                let key = (category, first, graph.clone());
                if !profiles.contains_key(&key) {
                    let donor_bytes = std::fs::read(target_root.join("meshes").join(&graph))
                        .map_err(|e| PhaseError::Internal(format!("{graph}: {e}")))?;
                    let (mut graph_path, graph_bytes, clips) = stage_profile(
                        category,
                        first,
                        &ctx.source_extracted_dir.join("meshes"),
                        &target_root.join("meshes"),
                        &donor_bytes,
                    )
                    .map_err(PhaseError::Internal)?;
                    let suffix = blake3::hash(graph.as_bytes()).to_hex()[..12].to_string();
                    graph_path = graph_path
                        .strip_suffix(".hkx")
                        .ok_or_else(|| {
                            PhaseError::Internal("Skyrim actor graph has no HKX extension".into())
                        })?
                        .to_string()
                        + "_"
                        + &suffix
                        + ".hkx";
                    insert_asset(&mut assets, &graph_path, graph_bytes)
                        .map_err(PhaseError::Internal)?;
                    let mut paths = Vec::new();
                    for clip in clips {
                        receipts.push(json!({"source":clip.contract.source_path,"output":clip.contract.output_path,"first_person":first,"category":category,"unmapped_source_bones":clip.unmapped_source_bones,"source_float_tracks":clip.source_float_tracks,"weapon_fire_time":clip.weapon_fire_time}));
                        paths.push(clip.contract.output_path.clone());
                        insert_asset(&mut assets, &clip.contract.output_path, clip.bytes)
                            .map_err(PhaseError::Internal)?;
                    }
                    profiles.insert(key.clone(), (graph_path, paths));
                }
                let (graph_path, clips) = &profiles[&key];
                registered_profiles.push(LegacyWeaponProfile {
                    donor: donor.clone(),
                    custom_graph_path: graph_path.replace('/', "\\"),
                    custom_animation_folder: format!(
                        "Actors/Character/{}/Animations/B21_SkyrimWeapons",
                        if first { "_1stPerson" } else { "" }
                    )
                    .replace("//", "/"),
                    emitted_clip_paths: clips.clone(),
                });
            }
            let plugin = mapper.interner.resolve(source_key.plugin).unwrap_or("");
            let identity = blake3::hash(plugin.as_bytes()).to_hex()[..8].to_string();
            registrations.push(LegacyWeaponProfileRegistration {
                source_weapon: source_key,
                target_weapon: target_key,
                editor_id: format!("{prefix}_SkyrimWeapon_{identity}_{:06X}", source_key.local),
                profiles: registered_profiles,
            });
        }
        let meshes = ctx.mod_path.join("data/meshes");
        for (path, bytes) in &assets {
            write(&meshes.join(path), bytes).map_err(PhaseError::Internal)?;
        }
        let registered = register_legacy_weapon_profiles(
            &mut session,
            &mut mapper,
            &config,
            &meshes,
            &registrations,
        )
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
        if !registered.warnings.is_empty() {
            return Err(PhaseError::Internal(
                registered
                    .warnings
                    .iter()
                    .filter_map(|warning| mapper.interner.resolve(*warning))
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        let manifest = json!({"profiles":registrations.iter().map(|item| json!({"source":item.source_weapon.format(mapper.interner),"target":item.target_weapon.format(mapper.interner)})).collect::<Vec<_>>(),"clips":receipts,"functionality_certified":false});
        write(
            &ctx.mod_path.join("debug/skyrim_weapon_animations.json"),
            &serde_json::to_vec_pretty(&manifest)
                .map_err(|e| PhaseError::Internal(e.to_string()))?,
        )
        .map_err(PhaseError::Internal)?;
        if let Some(sink) = &ctx.run.output_sink {
            for path in assets.keys() {
                sink.add_existing_file(&format!("meshes/{path}"), &meshes.join(path))
                    .map_err(PhaseError::Internal)?;
            }
        }
        Ok(PhaseReport {
            assets_written: assets.len() as u32,
            records_added: registered.records_added,
            records_changed: registered.records_changed,
            ..Default::default()
        })
    }
}

fn stage_profile(
    category: u8,
    first: bool,
    source: &Path,
    target: &Path,
    donor: &[u8],
) -> Result<(String, Vec<u8>, Vec<super::StagedSkyrimActorClip>), String> {
    if category == 7 {
        let result = super::bow_graph::stage_profile(source, target, donor, first)?;
        return Ok((result.graph_path, result.graph_bytes, result.clips));
    }
    let actor = if first {
        "Actors/Character/_1stPerson"
    } else {
        "Actors/Character"
    };
    let stage = |leaf| {
        let contract = super::actor_clip_contract(&format!("{actor}/Animations/DLC01/{leaf}.hkx"))
            .ok_or("unrecognized Skyrim crossbow clip")?;
        super::stage_actor_clip(
            contract.clone(),
            if matches!(
                contract.action,
                super::SkyrimWeaponAction::CrossbowRelease
                    | super::SkyrimWeaponAction::CrossbowReload
            ) {
                super::SkyrimActorConsumer::Fo4CrossbowGun
            } else {
                super::SkyrimActorConsumer::SourceAnnotations
            },
            &std::fs::read(source.join(&contract.source_path)).map_err(|e| e.to_string())?,
            &std::fs::read(source.join(&contract.source_skeleton_path))
                .map_err(|e| e.to_string())?,
            &std::fs::read(target.join(&contract.target_skeleton_path))
                .map_err(|e| e.to_string())?,
        )
    };
    let mut support = [
        "Crossbow_IdleHeld",
        "Crossbow_IdleDrawn",
        "SneakCrossbow_IdleDrawn",
        "Crossbow_Equip",
    ]
    .into_iter()
    .map(stage)
    .collect::<Result<Vec<_>, _>>()?;
    support.push(super::crossbow_graph::stage_source_unequip(
        source, target, first,
    )?);
    let result = super::crossbow_graph::stage_crossbow_actor_profile(
        donor,
        stage("Crossbow_Release")?,
        stage("Crossbow_Reload")?,
        support,
    )?;
    Ok((
        result.graph_path,
        result.graph_bytes,
        result.clips.into_iter().collect(),
    ))
}

fn weapon_category(record: &Record, interner: &StringInterner) -> Option<u8> {
    match &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DNAM")?
        .value
    {
        FieldValue::Bytes(bytes) if bytes.len() == 100 => bytes.first().copied(),
        FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
            if interner
                .resolve(*name)?
                .chars()
                .filter(|ch| ch.is_ascii_alphanumeric())
                .collect::<String>()
                .eq_ignore_ascii_case("animationtype")
            {
                match value {
                    FieldValue::Uint(value) => u8::try_from(*value).ok(),
                    FieldValue::Int(value) => u8::try_from(*value).ok(),
                    FieldValue::String(value) => {
                        match interner.resolve(*value)?.to_ascii_lowercase().as_str() {
                            "bow" => Some(7),
                            "crossbow" => Some(9),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn insert_asset(
    assets: &mut BTreeMap<String, Vec<u8>>,
    path: &str,
    bytes: Vec<u8>,
) -> Result<(), String> {
    if let Some(previous) = assets.get(path) {
        if previous != &bytes {
            return Err(format!("conflicting Skyrim weapon asset {path}"));
        }
    } else {
        assets.insert(path.into(), bytes);
    }
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().ok_or("missing Skyrim weapon asset parent")?)
        .map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::FieldEntry;

    #[test]
    fn source_weapon_category_accepts_source_schema_and_rejects_target_binary() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode(*b"WEAP"),
            FormKey {
                local: 1,
                plugin: interner.intern("Source.esm"),
            },
        );
        for category in [7, 9] {
            let mut bytes = vec![0; 100];
            bytes[0] = category;
            record.fields = vec![FieldEntry {
                sig: SubrecordSig(*b"DNAM"),
                value: FieldValue::Bytes(bytes.into()),
            }]
            .into();
            assert_eq!(weapon_category(&record, &interner), Some(category));
            record.fields[0].value = FieldValue::Struct(vec![(
                interner.intern("animation_type"),
                FieldValue::Uint(category.into()),
            )]);
            assert_eq!(weapon_category(&record, &interner), Some(category));
        }
        record.fields[0].value = FieldValue::Bytes(vec![7; 136].into());
        assert_eq!(weapon_category(&record, &interner), None);
    }

    #[test]
    fn shared_profiles_reject_conflicting_clip_bytes() {
        let mut assets = BTreeMap::new();
        insert_asset(
            &mut assets,
            "Actors/Character/Animations/Source.hkx",
            vec![1],
        )
        .unwrap();
        insert_asset(
            &mut assets,
            "Actors/Character/Animations/Source.hkx",
            vec![1],
        )
        .unwrap();
        assert!(
            insert_asset(
                &mut assets,
                "Actors/Character/Animations/Source.hkx",
                vec![2]
            )
            .is_err()
        );
        assert_eq!(assets.len(), 1);
    }
}
