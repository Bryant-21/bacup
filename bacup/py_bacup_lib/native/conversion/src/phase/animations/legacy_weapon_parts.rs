use std::collections::HashMap;
use std::path::{Path, PathBuf};

use nif_core_native::model::NifFile;
use serde_json::{Value, json};

use crate::phase::animations::weapon_events::{
    SOURCE_SPIN_START_EVENT, SOURCE_SPIN_STOP_EVENT, WeaponAnimationFields, WeaponClipAction,
    WeaponPerspective, build_weapon_event_contract, resolve_weapon_clip_candidate,
    source_weapon_clip_candidates,
};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::translator::Game;

use super::graph::{PartGraphState, build_behavior};
use super::{attach_behavior, attach_empty_idle, attach_weapon_part_sequence};

pub struct ConvertLegacyWeaponPartsPhase;

impl Phase for ConvertLegacyWeaponPartsPhase {
    fn name(&self) -> &'static str {
        "convert_legacy_weapon_parts"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fnv || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        let manifest_path = ctx.mod_path.join("debug/fnv_weapon_animations.json");
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).map_err(|error| {
                PhaseError::BadParams(format!("weapon parts require source actor plans: {error}"))
            })?)
            .map_err(|error| PhaseError::BadParams(error.to_string()))?;
        let target = ctx.target_extracted_dir.ok_or_else(|| {
            PhaseError::BadParams("weapon parts require extracted FO4 FX templates".into())
        })?;
        let mut source_roots = vec![ctx.source_extracted_dir.to_path_buf()];
        source_roots.extend(ctx.run.config.additional_source_asset_roots.clone());
        let source_paths = source_inventory(&source_roots).map_err(PhaseError::Internal)?;
        let meshes = ctx.mod_path.join("data/meshes");
        let mut receipts = Vec::new();
        let mut report = PhaseReport::default();
        for weapon in manifest
            .get("weapons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|weapon| weapon.get("status").and_then(Value::as_str) == Some("staged"))
        {
            ctx.check_cancel()?;
            let fields: WeaponAnimationFields =
                serde_json::from_value(weapon["animation_fields"].clone())
                    .map_err(|error| PhaseError::BadParams(error.to_string()))?;
            for model in weapon
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let result = (|| -> Result<_, String> {
                    let perspective: WeaponPerspective =
                        serde_json::from_value(model["perspective"].clone())
                            .map_err(|error| error.to_string())?;
                    let output = model
                        .get("output_model")
                        .and_then(Value::as_str)
                        .ok_or("weapon model plan is missing output_model")?;
                    let copy_from = model
                        .get("copy_from_model")
                        .and_then(Value::as_str)
                        .unwrap_or(output);
                    let output_path = mesh_path(&meshes, output)?;
                    let copy_path = mesh_path(&meshes, copy_from)?;
                    let graph_hash = blake3::hash(output.to_ascii_lowercase().as_bytes()).to_hex();
                    let graph_id = format!("B21_FNVWeaponParts_{}", &graph_hash[..16]);
                    stage_model_parts(
                        &fields,
                        perspective,
                        &source_roots,
                        &source_paths,
                        target,
                        &meshes,
                        &copy_path,
                        &output_path,
                        &graph_id,
                    )
                })();
                match result {
                    Ok((receipt, outputs)) => {
                        for output in &outputs {
                            if let Some(sink) = &ctx.run.output_sink {
                                let relative = output
                                    .strip_prefix(ctx.mod_path.join("data"))
                                    .map_err(|error| PhaseError::Internal(error.to_string()))?
                                    .to_string_lossy()
                                    .replace('\\', "/");
                                if sink.ba2.as_ref().is_some_and(|ba2| ba2.contains(&relative)) {
                                    return Err(PhaseError::Internal(format!(
                                        "weapon part output was streamed before part injection: {relative}"
                                    )));
                                }
                                sink.add_existing_file(&relative, output)
                                    .map_err(PhaseError::Internal)?;
                            }
                        }
                        report.assets_written += outputs.len() as u32;
                        receipts.push(json!({"weapon":weapon["weapon"],"model":model,"status":"staged","parts":receipt}));
                    }
                    Err(error) => {
                        report.items_failed += 1;
                        report.warnings += 1;
                        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                            phase: self.name(),
                            level: LogLevel::Warn,
                            message: format!("legacy weapon parts [{}]: {error}", weapon["weapon"]),
                        });
                        receipts.push(json!({"weapon":weapon["weapon"],"model":model,"status":"failed","reason":error}));
                    }
                }
            }
        }
        write(
            &ctx.mod_path.join("debug/fnv_weapon_parts.json"),
            &serde_json::to_vec_pretty(
                &json!({"models":receipts,"items_failed":report.items_failed}),
            )
            .map_err(|error| PhaseError::Internal(error.to_string()))?,
        )
        .map_err(PhaseError::Internal)?;
        if report.items_failed != 0 {
            return Err(PhaseError::Internal(format!(
                "{} planned weapon models failed moving-part conversion; see {}",
                report.items_failed,
                ctx.mod_path.join("debug/fnv_weapon_parts.json").display()
            )));
        }
        Ok(report)
    }
}

pub(in crate::phase::animations) fn stage_model_parts(
    fields: &WeaponAnimationFields,
    perspective: WeaponPerspective,
    source_roots: &[PathBuf],
    inventory: &[String],
    target_root: &Path,
    meshes: &Path,
    copy_from: &Path,
    output: &Path,
    graph_id: &str,
) -> Result<(Value, Vec<PathBuf>), String> {
    let mut weapon = NifFile::load(copy_from.to_path_buf())
        .map_err(|error| format!("converted weapon model {}: {error}", copy_from.display()))?;
    let mut receipts = Vec::new();
    let mut states = vec![PartGraphState {
        sequence: "LegacyIdle".into(),
        triggers: vec![],
        exits: vec![],
        exit_sequence: "LegacyIdle".into(),
    }];
    let mut has_idle = false;
    let mut contextual = Vec::new();
    let mut has_source_internal_loop = false;
    let mut reload_per_round = false;
    let candidates = source_weapon_clip_candidates(fields, perspective)?
        .iter()
        .map(|candidate| resolve_weapon_clip_candidate(candidate, inventory))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let has_explicit_tail = candidates
        .iter()
        .any(|candidate| candidate.action == WeaponClipAction::ReloadEnd);
    for candidate in candidates {
        let action = candidate.action;
        if action == WeaponClipAction::AttackLoop && has_source_internal_loop {
            continue;
        }
        let (name, triggers, looping) = match action {
            WeaponClipAction::Aim => ("LegacyIdle", vec![], true),
            WeaponClipAction::Equip => ("LegacyEquip", vec!["weaponDraw"], false),
            WeaponClipAction::Unequip => ("LegacyUnequip", vec!["weaponSheathe"], false),
            WeaponClipAction::Attack => (
                "LegacyFire",
                if fields.automatic {
                    vec![]
                } else {
                    vec!["WeaponFire"]
                },
                false,
            ),
            WeaponClipAction::AttackLoop => ("LegacyAuto", vec!["attackStartAuto"], true),
            WeaponClipAction::Reload => (
                "LegacyReload",
                vec!["reloadStart", "ReloadStart"],
                fields.per_round_reload,
            ),
            WeaponClipAction::ReloadStart => ("LegacyReloadStart", vec![], false),
            WeaponClipAction::ReloadEnd => ("LegacyReloadTail", vec![], false),
            WeaponClipAction::HandGrip => continue,
            _ => {
                contextual.push(candidate.source_path);
                continue;
            }
        };
        let source_path = source_roots
            .iter()
            .map(|root| root.join(&candidate.source_path))
            .find(|path| path.is_file())
            .ok_or_else(|| format!("weapon KF missing: {}", candidate.source_path))?;
        let bytes = std::fs::read(source_path).map_err(|error| error.to_string())?;
        let evidence = crate::phase::animations::parse_fnv_creature_kf(
            &bytes,
            &candidate.source_path,
            None,
            None,
        )
        .map_err(|error| error.to_string())?;
        let contract = build_weapon_event_contract(fields, action, &evidence, &HashMap::new())?;
        let kf = NifFile::from_bytes(&bytes, None).map_err(|error| error.to_string())?;
        if let Some(fire) = contract
            .continuous_fire
            .as_ref()
            .filter(|fire| fire.source_internal_loop)
        {
            let start = fire
                .loop_start_seconds
                .ok_or("source internal fire loop has no start")?;
            let end = fire.loop_end_seconds;
            for (name, start, end, triggers, exits, destination, looping) in [
                (
                    "LegacySpinStart",
                    0.0,
                    Some(start),
                    vec![SOURCE_SPIN_START_EVENT],
                    vec!["end"],
                    "LegacyAuto",
                    false,
                ),
                (
                    "LegacyAuto",
                    start,
                    Some(end),
                    vec!["attackStartAuto"],
                    vec!["attackStateExit", "attackStop"],
                    "LegacyIdle",
                    true,
                ),
                (
                    "LegacySpinStop",
                    end,
                    None,
                    vec![SOURCE_SPIN_STOP_EVENT],
                    vec!["end"],
                    "LegacyIdle",
                    false,
                ),
            ] {
                let receipt = attach_weapon_part_sequence(
                    &mut weapon,
                    &kf,
                    0,
                    name,
                    start,
                    end,
                    candidate.playback_speed,
                    looping,
                )?;
                if !receipt.matched_tracks.is_empty() {
                    has_source_internal_loop = true;
                    states.push(PartGraphState {
                        sequence: name.into(),
                        triggers: triggers.into_iter().map(String::from).collect(),
                        exits: exits.into_iter().map(String::from).collect(),
                        exit_sequence: destination.into(),
                    });
                }
                receipts.push(json!({"source_kf":candidate.source_path,"sequence":receipt}));
            }
            continue;
        }
        let start = match action {
            WeaponClipAction::Attack => contract.fire_seconds.first().copied().unwrap_or(0.0),
            WeaponClipAction::Equip | WeaponClipAction::Unequip => contract
                .annotations
                .iter()
                .find(|annotation| {
                    matches!(annotation.text.as_str(), "weaponDraw" | "weaponSheathe")
                })
                .map_or(0.0, |annotation| annotation.time),
            _ => 0.0,
        };
        let per_round = contract
            .reload
            .as_ref()
            .is_some_and(|reload| reload.per_round);
        if action == WeaponClipAction::Reload {
            reload_per_round = per_round;
        }
        let looping = if action.is_reload() {
            per_round
        } else {
            looping
        };
        let end = contract
            .reload
            .as_ref()
            .and_then(|reload| reload.repeat_until_seconds);
        let receipt = attach_weapon_part_sequence(
            &mut weapon,
            &kf,
            0,
            name,
            start,
            end,
            candidate.playback_speed,
            looping,
        )?;
        if receipt.matched_tracks.is_empty() {
            receipts.push(json!({"source_kf":candidate.source_path,"sequence":receipt}));
            continue;
        }
        if name == "LegacyIdle" {
            has_idle = true;
        } else {
            states.push(PartGraphState {
                sequence: name.into(),
                triggers: triggers.into_iter().map(String::from).collect(),
                exits: if action == WeaponClipAction::AttackLoop {
                    vec!["attackStateExit".into(), "attackStop".into()]
                } else if per_round && action == WeaponClipAction::Reload {
                    vec![
                        "Reload_to_Idle".into(),
                        "reloadEnd".into(),
                        "ReloadEnd".into(),
                    ]
                } else {
                    vec!["end".into()]
                },
                exit_sequence: "LegacyIdle".into(),
            });
        }
        if per_round && action == WeaponClipAction::Reload && !has_explicit_tail {
            let boundary = contract
                .reload
                .as_ref()
                .and_then(|reload| reload.exit_tail_start_seconds)
                .ok_or("per-round reload has no exit tail")?;
            let tail = attach_weapon_part_sequence(
                &mut weapon,
                &kf,
                0,
                "LegacyReloadTail",
                boundary,
                None,
                candidate.playback_speed,
                false,
            )?;
            if !tail.matched_tracks.is_empty() {
                states.push(PartGraphState {
                    sequence: "LegacyReloadTail".into(),
                    triggers: vec![],
                    exits: vec!["end".into()],
                    exit_sequence: "LegacyIdle".into(),
                });
                receipts.push(json!({"source_kf":candidate.source_path,"sequence":tail}));
            }
        }
        receipts.push(json!({"source_kf":candidate.source_path,"sequence":receipt}));
    }
    let has_parts = receipts.iter().any(|receipt| {
        receipt["sequence"]["matched_tracks"]
            .as_array()
            .is_some_and(|tracks| !tracks.is_empty())
    });
    if !has_parts {
        write(
            output,
            &weapon.to_bytes().map_err(|error| error.to_string())?,
        )?;
        return Ok((
            json!({"sequences":receipts,"has_matching_parts":false}),
            vec![output.to_path_buf()],
        ));
    }
    if !has_idle {
        attach_empty_idle(&mut weapon)?;
    }
    if has_source_internal_loop {
        states[0].triggers = vec!["attackStateExit".into(), "attackStop".into()];
    }
    let has_auto = states.iter().any(|state| state.sequence == "LegacyAuto");
    let has_tail = states
        .iter()
        .any(|state| state.sequence == "LegacyReloadTail");
    let has_start = states
        .iter()
        .any(|state| state.sequence == "LegacyReloadStart");
    for state in &mut states {
        if fields.automatic && !has_auto && state.sequence == "LegacyFire" {
            state.triggers = vec!["WeaponFire".into()];
        }
        if reload_per_round && state.sequence == "LegacyReload" {
            if has_tail {
                state.exit_sequence = "LegacyReloadTail".into();
            }
            if has_start {
                state.triggers.clear();
            }
        }
        if state.sequence == "LegacyReloadStart" {
            state.triggers = vec!["reloadStart".into(), "ReloadStart".into()];
            state.exit_sequence = "LegacyReload".into();
        }
    }
    let project = format!("UniqueBehaviors/{graph_id}/{graph_id}.hkx");
    attach_behavior(&mut weapon, &project)?;
    let behavior = build_behavior(&states)?;
    let template = target_root.join("meshes/UniqueBehaviors/BroadsiderFX");
    let project_bytes = std::fs::read(template.join("BroadsiderFX.hkx"))
        .map_err(|error| format!("weapon FX project template: {error}"))?;
    let character_bytes = std::fs::read(template.join("Characters/Character.hkx"))
        .map_err(|error| format!("weapon FX character template: {error}"))?;
    for bytes in [&project_bytes, &character_bytes] {
        havok_native::hkx::HkxFile::read(bytes).map_err(|error| error.to_string())?;
    }
    let outputs = vec![
        output.to_path_buf(),
        meshes.join(&project),
        meshes.join(format!(
            "UniqueBehaviors/{graph_id}/Characters/Character.hkx"
        )),
        meshes.join(format!("UniqueBehaviors/{graph_id}/Behaviors/Behavior.hkx")),
    ];
    let nif_bytes = weapon.to_bytes().map_err(|error| error.to_string())?;
    NifFile::from_bytes(&nif_bytes, None)
        .map_err(|error| format!("weapon part NIF cannot reload: {error}"))?;
    for (path, bytes) in
        outputs
            .iter()
            .zip([&nif_bytes, &project_bytes, &character_bytes, &behavior])
    {
        write(path, bytes)?;
    }
    Ok((
        json!({"sequences":receipts,"has_matching_parts":true,"project":project,"contextual_part_reuse_requires_runtime_validation":contextual,"graph_sequences":states.iter().map(|state|&state.sequence).collect::<Vec<_>>()}),
        outputs,
    ))
}

fn source_inventory(roots: &[PathBuf]) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for root in roots {
        for folder in ["meshes/characters/_male", "meshes/characters/_1stperson"] {
            let directory = root.join(folder);
            if !directory.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
                let path = entry.map_err(|error| error.to_string())?.path();
                if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("kf"))
                {
                    files.push(format!(
                        "{folder}/{}",
                        path.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn mesh_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let normalized = relative.replace('\\', "/");
    if Path::new(&normalized).is_absolute() || normalized.split('/').any(|part| part == "..") {
        return Err(format!("invalid mesh-relative path: {relative}"));
    }
    let relative = normalized
        .strip_prefix("meshes/")
        .or_else(|| normalized.strip_prefix("Meshes/"))
        .unwrap_or(&normalized);
    Ok(root.join(relative))
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nif_core_native::model::NifValue;

    #[test]
    #[ignore = "requires extracted FNV weapons/KFs and vanilla FO4 FX templates"]
    fn real_models_emit_separate_pov_nifs_with_packed_part_graphs() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let source = repo.join("extracted/fnv");
        let target = repo.join("extracted/fo4");
        let inventory = source_inventory(&[source.clone()]).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let meshes = temp.path().join("meshes");
        std::fs::create_dir_all(&meshes).unwrap();
        for (index, model, family, attack, reload, per_round, reload_time) in [
            (0, "weapons/1handpistol/9mm.nif", 3, 32, 11, false, 1.6667),
            (
                1,
                "weapons/2handrifle/huntingrifle.nif",
                5,
                38,
                0,
                false,
                1.9667,
            ),
            (
                2,
                "weapons/1handpistol/357revolverShort.nif",
                3,
                50,
                20,
                true,
                0.6333,
            ),
        ] {
            let fields = WeaponAnimationFields {
                animation_type: family,
                grip_animation: 255,
                attack_animation: attack,
                reload_animation: reload,
                ammo_use: 1,
                automatic: false,
                per_round_reload: per_round,
                animation_multiplier: 1.0,
                attack_multiplier: 1.0,
                fire_rate: 1.0,
                attack_shots_sec: 1.0,
                reload_time,
            };
            let copy = meshes.join(format!("source{index}.nif"));
            let converted = nif_core_native::convert_file::convert_nif_file(
                &source.join("meshes").join(model),
                &copy,
                "fnv",
                "fo4",
                None,
                &nif_core_native::convert_file::ConvertFileOptions {
                    weapon_role: Some("gun".into()),
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(
                converted.supported && converted.errors.is_empty(),
                "{:?}",
                converted.errors
            );
            let before = std::fs::read(&copy).unwrap();
            let mut references = Vec::new();
            for (perspective, suffix) in [
                (WeaponPerspective::Third, "3P"),
                (WeaponPerspective::First, "1P"),
            ] {
                let output = meshes.join(format!("{index}/{suffix}.nif"));
                let (receipt, outputs) = stage_model_parts(
                    &fields,
                    perspective,
                    &[source.clone()],
                    &inventory,
                    &target,
                    &meshes,
                    &copy,
                    &output,
                    &format!("WeaponParts{index}{suffix}"),
                )
                .unwrap();
                assert_eq!(outputs.len(), 4);
                assert!(outputs.iter().all(|path| path.is_file()));
                assert_eq!(receipt["has_matching_parts"], true);
                let nif = NifFile::load(output).unwrap();
                let bged = nif
                    .blocks
                    .iter()
                    .find(|block| block.type_name == "BSBehaviorGraphExtraData")
                    .unwrap();
                let Some(NifValue::String(project)) = bged.get_field("Behaviour Graph File") else {
                    panic!()
                };
                references.push(project.clone());
                let behavior = std::fs::read(&outputs[3]).unwrap();
                let xml = havok_native::api::havok_hkx_to_xml(&behavior).unwrap();
                assert!(
                    xml.contains("LegacyFire")
                        && xml.contains("WeaponFire")
                        && xml.contains("LegacyReload")
                );
                if per_round {
                    assert!(xml.contains("LegacyReloadTail") && xml.contains("Reload_to_Idle"));
                }
            }
            assert_ne!(references[0], references[1]);
            assert_eq!(before, std::fs::read(copy).unwrap());
        }
    }
}
