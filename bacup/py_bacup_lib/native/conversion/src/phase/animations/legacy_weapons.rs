use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::fixups::FixupConfig;
use crate::fixups::face::legacy_weapon_profiles::{
    LegacyWeaponProfile, LegacyWeaponProfileRegistration, discover_legacy_weapon_donor_profiles,
    legacy_weapon_donor, register_legacy_weapon_profiles, weapon_perspective,
};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::Game;

use super::weapon_events::{
    SOURCE_SPIN_START_EVENT, SOURCE_SPIN_STOP_EVENT, WeaponAnimationFields, WeaponAttackTiming,
    WeaponClipAction, WeaponPerspective, build_weapon_event_contract,
    decode_weapon_animation_fields, resolve_weapon_clip_candidate, source_weapon_clip_candidates,
};
use super::weapon_retarget::{humanoid_bone_map, retarget_weapon_clip, target_reference_pose};

pub struct ConvertLegacyWeaponAnimationsPhase;

impl Phase for ConvertLegacyWeaponAnimationsPhase {
    fn name(&self) -> &'static str {
        "convert_legacy_weapon_animations"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fnv || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        ctx.check_cancel()?;
        let target_root = ctx.target_extracted_dir.ok_or_else(|| {
            PhaseError::BadParams(
                "legacy weapon animations require extracted Fallout 4 contracts".into(),
            )
        })?;
        let mut source_roots = vec![ctx.source_extracted_dir.to_path_buf()];
        source_roots.extend(ctx.run.config.additional_source_asset_roots.clone());
        let source_paths = source_weapon_paths(&source_roots).map_err(PhaseError::Internal)?;
        let prefix = ctx
            .params
            .get("mod_prefix")
            .and_then(|value| value.as_str())
            .unwrap_or("B21_FNV");
        let config = FixupConfig {
            target_master_handle_ids: ctx.run.master_handle_ids.clone(),
            target_schema: Some(ctx.run.schema_target.clone()),
            source_schema: Some(ctx.run.schema_source.clone()),
            target_extracted_dir: Some(target_root.to_path_buf()),
            mod_path: Some(ctx.mod_path.to_path_buf()),
            ..Default::default()
        };
        let mut session =
            crate::session::open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
                .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let state = ctx.run.mapper_state.as_mut().ok_or_else(|| {
            PhaseError::Internal(
                "legacy weapon animation phase requires retained mapper state".into(),
            )
        })?;
        let mut mapper = FormKeyMapper::from_state(state, &ctx.run.interner);
        let output_plugin = mapper.output_plugin_sym();
        let source_schema = ctx.run.schema_source.clone();
        let target_schema = ctx.run.schema_target.clone();
        let keys = session
            .source_form_keys_of_sig(SigCode(*b"WEAP"), mapper.interner)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let sounds = sound_editor_id_map(
            &mut session,
            &mut mapper,
            &source_schema,
            &target_schema,
            &config,
        )?;
        let mut report = PhaseReport::default();
        let mut receipts = Vec::new();
        let mut registrations = Vec::new();
        let mut staged = HashMap::<String, StagedActorSet>::new();
        let mut graph_wrappers = HashMap::<(String, String), String>::new();
        let mut graph_outputs = HashSet::new();
        let mut attack_timings = HashMap::new();
        let mut automatic_timings = HashMap::new();
        let mut ammo_units = HashMap::new();
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
                .map_err(|error| PhaseError::Internal(error.to_string()))?;
            let eid = editor_id(&source, mapper.interner)
                .unwrap_or_else(|| format!("Weapon{:06X}", source_key.local));
            let source_script = source.fields.iter().find_map(|field| {
                if let (b"SCRI", FieldValue::FormKey(key)) = (&field.sig.0, &field.value) {
                    Some(json!({"form_key":key.format(mapper.interner),"local_id":key.local}))
                } else {
                    None
                }
            });
            let fields = match decode_weapon_animation_fields(&source, mapper.interner) {
                Ok(fields) if (3..=9).contains(&fields.animation_type) => fields,
                Ok(fields) => {
                    receipts.push(json!({"weapon":eid,"source_local":source_key.local,
                        "status":"unsupported_source_weapon_family","animation_fields":fields,
                        "reason":"source melee, unarmed and thrown weapon behavior has not been ported"}));
                    report.warnings += 1;
                    continue;
                }
                Err(error) => {
                    receipts.push(json!({"weapon":eid,"status":"failed","reason":error}));
                    report.items_failed += 1;
                    continue;
                }
            };
            if mapper.interner.resolve(source_key.plugin) == Some("FalloutNV.esm")
                && ((source_key.local == 0x130041 && eid.eq_ignore_ascii_case("WeapNVDetonator"))
                    || (source_key.local == 0x14C068
                        && eid.eq_ignore_ascii_case("WeapNVDetonatorLoyal")))
            {
                receipts.push(json!({"weapon":eid,"source_local":source_key.local,
                    "status":"unsupported_scripted_weapon",
                    "source_script":source_script,
                    "reason":"source OnFire remote detonation and quest script behavior require a runtime adapter"}));
                report.warnings += 1;
                continue;
            }
            let mut target = session
                .record_decoded(&target_key, &target_schema, mapper.interner)
                .map_err(|error| PhaseError::Internal(error.to_string()))?;
            let source_world = model_path(&source, b"MODL", mapper.interner);
            let source_first = source
                .fields
                .iter()
                .find_map(|field| match (&field.sig.0, &field.value) {
                    (b"WNAM", FieldValue::FormKey(key)) => Some(*key),
                    _ => None,
                })
                .and_then(|key| {
                    session
                        .source_record_decoded(&key, &source_schema, mapper.interner)
                        .ok()
                })
                .and_then(|record| model_path(&record, b"MODL", mapper.interner))
                .or_else(|| source_world.clone());
            let Some(copy_world) =
                model_path(&target, b"MODL", mapper.interner).filter(|path| !path.is_empty())
            else {
                continue;
            };
            if source_world.as_deref().is_none_or(str::is_empty) {
                continue;
            }
            let copy_first = model_path(&target, b"MOD4", mapper.interner)
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| copy_world.clone());
            let model_root = format!(
                "Weapons/{}/{}_{:06X}",
                safe_name(prefix),
                safe_name(&eid),
                source_key.local
            );
            let world_model = format!("{model_root}/World.nif");
            let first_model = format!("{model_root}/FirstPerson.nif");
            let models = json!([
                {"perspective":"Third","source_model":source_world,"output_model":world_model,"copy_from_model":copy_world},
                {"perspective":"First","source_model":source_first,"output_model":first_model,"copy_from_model":copy_first},
            ]);
            if source_key.local == 0x0E6346
                && mapper.interner.resolve(source_key.plugin) == Some("FalloutNV.esm")
                && eid.eq_ignore_ascii_case("WeapNVBinoculars")
            {
                receipts.push(json!({"weapon":eid,"source_local":source_key.local,
                    "status":"unsupported_zoom_tool",
                    "reason":"binocular scope behavior requires a non-firing tool profile"}));
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Warn,
                    message: format!(
                        "legacy weapon animations [{eid}]: binocular zoom is not implemented"
                    ),
                });
                report.warnings += 1;
                continue;
            }
            let result = (|| -> Result<_, String> {
                let donor_key = legacy_weapon_donor(
                    fields.animation_type,
                    fields.attack_animation,
                    fields.per_round_reload,
                    mapper.interner,
                )
                .ok_or("source weapon has no matching FO4 graph topology")?;
                let donors = discover_legacy_weapon_donor_profiles(
                    &mut session,
                    &mut mapper,
                    &config,
                    donor_key,
                )
                .map_err(|error| error.to_string())?;
                if !donors
                    .iter()
                    .any(|donor| weapon_perspective(donor) == Some(0))
                    || !donors
                        .iter()
                        .any(|donor| weapon_perspective(donor) == Some(1))
                {
                    return Err("weapon animation donor lacks one of the two perspectives".into());
                }
                let fingerprint =
                    blake3::hash(&serde_json::to_vec(&fields).map_err(|error| error.to_string())?)
                        .to_hex()
                        .to_string();
                let set_id = format!("{}_FNV_{}", safe_name(prefix), &fingerprint[..12]);
                let mut profiles = Vec::new();
                let mut attack_timing = None;
                let mut automatic_timing = None;
                for perspective in [WeaponPerspective::Third, WeaponPerspective::First] {
                    let first = perspective == WeaponPerspective::First;
                    let key = format!("{set_id}_{}", if first { "1P" } else { "3P" });
                    if !staged.contains_key(&key) {
                        let actor = stage_actor_set(
                            &fields,
                            perspective,
                            &key,
                            &source_roots,
                            &source_paths,
                            target_root,
                            &ctx.mod_path.join("data/meshes"),
                            &sounds,
                        )?;
                        report.assets_written += actor.clips.len() as u32;
                        staged.insert(key.clone(), actor);
                    }
                    let actor = &staged[&key];
                    if !first {
                        attack_timing = actor.attack_timing.clone();
                        automatic_timing = actor.automatic_timing.clone();
                    }
                    for donor in donors
                        .iter()
                        .filter(|donor| weapon_perspective(donor) == Some(first as u16))
                    {
                        let donor_graph = mapper
                            .interner
                            .resolve(donor.behaviour_graph)
                            .ok_or("unresolved donor graph")?;
                        let cache_key = (key.clone(), donor_graph.to_string());
                        if !graph_wrappers.contains_key(&cache_key) {
                            let normalized = donor_graph.replace('\\', "/");
                            let bytes = std::fs::read(target_root.join("meshes").join(&normalized))
                                .map_err(|error| error.to_string())?;
                            let events = actor
                                .attachment_events
                                .iter()
                                .map(String::as_str)
                                .collect::<Vec<_>>();
                            let graphs = super::weapon_actor_graph::stage_source_attachment_graphs(
                                donor_graph,
                                &bytes,
                                &target_root.join("meshes"),
                                &key,
                                &events,
                                &actor.clips,
                            )?;
                            for (path, bytes) in graphs.assets {
                                write_asset(&ctx.mod_path.join("data/meshes").join(&path), &bytes)?;
                                if graph_outputs.insert(path) {
                                    report.assets_written += 1;
                                }
                            }
                            graph_wrappers.insert(cache_key.clone(), graphs.wrapper_path);
                        }
                        let custom_graph = &graph_wrappers[&cache_key];
                        profiles.push(LegacyWeaponProfile {
                            donor: donor.clone(),
                            custom_graph_path: custom_graph.replace('/', "\\"),
                            custom_animation_folder: actor.folder.replace('/', "\\"),
                            emitted_clip_paths: actor.clips.clone(),
                        });
                    }
                }
                Ok((profiles, set_id, attack_timing, automatic_timing))
            })();
            match result {
                Ok((profiles, set_id, attack_timing, automatic_timing)) => {
                    ammo_units.insert(target_key, fields.ammo_use);
                    if let Some(timing) = attack_timing {
                        attack_timings.insert(target_key, timing);
                    }
                    if let Some(timing) = automatic_timing {
                        automatic_timings.insert(target_key, timing);
                    }
                    set_model_path(&mut target, b"MODL", &world_model, mapper.interner);
                    set_model_path(&mut target, b"MOD4", &first_model, mapper.interner);
                    session
                        .replace_record_contents(target, &target_schema, mapper.interner)
                        .map_err(|error| PhaseError::Internal(error.to_string()))?;
                    registrations.push(LegacyWeaponProfileRegistration {
                        source_weapon: source_key,
                        target_weapon: target_key,
                        editor_id: format!(
                            "{}_{}_{:06X}",
                            safe_name(prefix),
                            safe_name(&eid),
                            source_key.local
                        ),
                        profiles,
                    });
                    receipts.push(
                        json!({"weapon":eid,"source_local":source_key.local,"actor_set":set_id,
                        "status":"staged","scope":"actor_animations","functionality_certified":false,
                        "ammo_consumption":{"source_units_per_shot":fields.ammo_use,
                            "runtime_adapter":(fields.ammo_use > 1).then_some("LegacyWeaponAmmo")},
                        "source_script":source_script,"animation_fields":fields,"models":models}),
                    );
                }
                Err(error) => {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: self.name(),
                        level: LogLevel::Warn,
                        message: format!("legacy weapon animations [{eid}]: {error}"),
                    });
                    receipts.push(json!({"weapon":eid,"status":"failed","reason":error}));
                    report.items_failed += 1;
                    report.warnings += 1;
                }
            }
        }
        let registered = register_legacy_weapon_profiles(
            &mut session,
            &mut mapper,
            &config,
            &ctx.mod_path.join("data/meshes"),
            &registrations,
        )
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
        report.records_added += registered.records_added;
        report.records_changed += registered.records_changed;
        report.warnings += registered.warnings.len() as u32;
        report.items_failed += registered.warnings.len() as u32;
        let profile_warnings = registered
            .warnings
            .iter()
            .filter_map(|warning| mapper.interner.resolve(*warning))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if registered.warnings.is_empty() {
            for registration in &registrations {
                let mut weapon = session
                    .record_decoded(&registration.target_weapon, &target_schema, mapper.interner)
                    .map_err(|error| PhaseError::Internal(error.to_string()))?;
                normalize_baked_weapon_stats(
                    &mut weapon,
                    mapper.interner,
                    attack_timings.get(&registration.target_weapon),
                    automatic_timings.get(&registration.target_weapon),
                )
                .map_err(PhaseError::Internal)?;
                session
                    .replace_record_contents(weapon, &target_schema, mapper.interner)
                    .map_err(|error| PhaseError::Internal(error.to_string()))?;
            }
        }
        let ammo_catalog = legacy_ammo_catalog(
            registrations
                .iter()
                .map(|item| (item.target_weapon, ammo_units[&item.target_weapon])),
            mapper.interner,
        );
        let output_plugin_name = mapper
            .interner
            .resolve(output_plugin)
            .ok_or_else(|| PhaseError::Internal("unresolved legacy weapon output plugin".into()))?
            .to_owned();
        if output_plugin_name.is_empty()
            || output_plugin_name.contains(['/', '\\', ':'])
            || matches!(output_plugin_name.as_str(), "." | "..")
        {
            return Err(PhaseError::Internal(
                "legacy weapon output plugin must be a file name".into(),
            ));
        }
        drop(session);
        drop(mapper);
        let manifest = ctx.mod_path.join("debug/fnv_weapon_animations.json");
        write_asset(
            &manifest,
            &serde_json::to_vec_pretty(&json!({"weapons":receipts,
                "records_changed":report.records_changed,"items_failed":report.items_failed,
                "profile_warnings":profile_warnings,
                "actor_sets":staged.values().map(|set| &set.receipt).collect::<Vec<_>>()
            }))
            .map_err(|error| PhaseError::Internal(error.to_string()))?,
        )
        .map_err(PhaseError::Internal)?;
        if report.items_failed != 0 {
            return Err(PhaseError::Internal(format!(
                "{} legacy weapon animation profiles failed; inspect {}",
                report.items_failed,
                manifest.display(),
            )));
        }
        write_asset(
            &ctx.mod_path
                .join("F4SE/Plugins/B21_ConversionRuntime/LegacyWeaponAmmo")
                .join(format!("{output_plugin_name}.json")),
            &serde_json::to_vec_pretty(&ammo_catalog)
                .map_err(|error| PhaseError::Internal(error.to_string()))?,
        )
        .map_err(PhaseError::Internal)?;
        report.assets_written += 1;
        if let Some(sink) = &ctx.run.output_sink {
            let mut seen = HashSet::new();
            for graph in graph_outputs {
                let relative = format!("meshes/{}", graph.replace('\\', "/"));
                if seen.insert(relative.clone()) {
                    sink.add_existing_file(&relative, &ctx.mod_path.join("data").join(&relative))
                        .map_err(PhaseError::Internal)?;
                }
            }
            for registration in registrations {
                for profile in registration.profiles {
                    for mesh in
                        std::iter::once(profile.custom_graph_path).chain(profile.emitted_clip_paths)
                    {
                        let relative = format!("meshes/{}", mesh.replace('\\', "/"));
                        if seen.insert(relative.clone()) {
                            sink.add_existing_file(
                                &relative,
                                &ctx.mod_path.join("data").join(&relative),
                            )
                            .map_err(PhaseError::Internal)?;
                        }
                    }
                }
            }
        }
        Ok(report)
    }
}

fn legacy_ammo_catalog(
    profiles: impl IntoIterator<Item = (crate::ids::FormKey, u8)>,
    interner: &StringInterner,
) -> serde_json::Value {
    json!({"version":1,"source_game":"fnv","weapons":profiles.into_iter()
        .filter(|(_, units)| *units > 1)
        .map(|(weapon, units)| json!({"weapon":format!("{:06X}:{}", weapon.local,
            interner.resolve(weapon.plugin).expect("weapon plugin must be interned in the conversion run")),
            "units_per_shot":units}))
        .collect::<Vec<_>>()})
}

pub(super) struct StagedActorSet {
    pub(super) folder: String,
    pub(super) clips: Vec<String>,
    pub(super) receipt: serde_json::Value,
    attachment_events: Vec<String>,
    attack_timing: Option<WeaponAttackTiming>,
    automatic_timing: Option<AutomaticWeaponTiming>,
}

#[derive(Clone)]
struct AutomaticWeaponTiming {
    animation_attack_seconds: f64,
    charge_seconds: Option<f64>,
}

pub(super) fn stage_actor_set(
    fields: &WeaponAnimationFields,
    perspective: WeaponPerspective,
    set_id: &str,
    source_roots: &[PathBuf],
    source_paths: &[String],
    target_root: &Path,
    meshes_root: &Path,
    sounds: &HashMap<String, String>,
) -> Result<StagedActorSet, String> {
    let first = perspective == WeaponPerspective::First;
    let actor_root = if first {
        "Actors/Character/_1stPerson"
    } else {
        "Actors/Character"
    };
    let source_rig_path = format!(
        "meshes/characters/{}/skeleton.nif",
        if first { "_1stperson" } else { "_male" }
    );
    let source_rig = nif_core_native::model::NifFile::from_bytes(
        &std::fs::read(resolve_source(source_roots, &source_rig_path)?)
            .map_err(|error| error.to_string())?,
        None,
    )
    .map_err(|error| error.to_string())?;
    let source_pose = crate::phase::skeleton::nif_reference_pose(&source_rig)?;
    let skeleton_path = format!("{actor_root}/CharacterAssets/Skeleton.hkx");
    let bytes = std::fs::read(target_root.join("meshes").join(&skeleton_path))
        .map_err(|error| error.to_string())?;
    let xml = havok_native::api::havok_hkx_to_xml(&bytes).map_err(|error| error.to_string())?;
    let target_record = havok_native::animation::parsers::parse_skeleton_xml(&xml)
        .map_err(|error| error.to_string())?;
    let target_pose = target_reference_pose(&target_record)?;
    let folder = format!("{actor_root}/Animations/{set_id}");
    let map = humanoid_bone_map()?;
    let mut outputs = Vec::new();
    let mut receipts = Vec::new();
    let mut attachment_events = Vec::new();
    let mut attack_timing = None;
    let mut automatic_timing = None;
    let mut source_loop_aliases = HashSet::new();
    let candidates = source_weapon_clip_candidates(fields, perspective)?
        .iter()
        .map(|candidate| resolve_weapon_clip_candidate(candidate, source_paths))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let grip = candidates
        .iter()
        .find(|candidate| candidate.action == WeaponClipAction::HandGrip)
        .map(|candidate| {
            let bytes = std::fs::read(resolve_source(source_roots, &candidate.source_path)?)
                .map_err(|error| error.to_string())?;
            super::parse_kf_bytes(&bytes, &candidate.source_path).map_err(|error| error.to_string())
        })
        .transpose()?;
    for candidate in candidates {
        if candidate.action == WeaponClipAction::HandGrip {
            continue;
        }
        let bytes = std::fs::read(resolve_source(source_roots, &candidate.source_path)?)
            .map_err(|error| error.to_string())?;
        let mut parsed = super::parse_kf_bytes(&bytes, &candidate.source_path)
            .map_err(|error| error.to_string())?;
        if candidate.action.is_attack()
            || matches!(
                candidate.action,
                WeaponClipAction::Aim
                    | WeaponClipAction::AimIronSights
                    | WeaponClipAction::SneakAim
                    | WeaponClipAction::SneakAimIronSights
            )
        {
            if let Some(grip) = &grip {
                apply_finger_grip(&mut parsed, grip);
            }
        }
        let evidence = super::parse_fnv_creature_kf(&bytes, &candidate.source_path, None, None)
            .map_err(|error| error.to_string())?;
        let contract = build_weapon_event_contract(fields, candidate.action, &evidence, sounds)?;
        if matches!(
            candidate.action,
            WeaponClipAction::Attack | WeaponClipAction::AttackLoop
        ) && attack_timing.is_none()
            && automatic_timing.is_none()
        {
            attack_timing = contract.attack_timing.clone();
            automatic_timing = contract.continuous_fire.as_ref().map(|continuous| {
                AutomaticWeaponTiming {
                    // FO4 GetShotsPerSecond divides fDispRoFShotsPerFile (20) by this value.
                    animation_attack_seconds: 20.0 * continuous.shot_interval_seconds,
                    charge_seconds: continuous
                        .source_internal_loop
                        .then(|| continuous.loop_start_seconds.unwrap() / contract.playback_speed),
                }
            });
        }
        for annotation in &contract.annotations {
            if matches!(annotation.text.as_str(), "weaponDraw" | "weaponSheathe")
                && !attachment_events.contains(&annotation.text)
            {
                attachment_events.push(annotation.text.clone());
            }
        }
        parsed.events = contract
            .annotations
            .iter()
            .map(|event| super::AnimationEvent {
                time: event.time,
                source_time: event.time,
                text: event.text.clone(),
            })
            .collect();
        let (clip, omitted) = retarget_weapon_clip(
            &parsed,
            &source_pose,
            &target_pose,
            &map,
            30.0,
            candidate.playback_speed,
        )?;
        let internal_loop = contract
            .continuous_fire
            .as_ref()
            .filter(|fire| fire.source_internal_loop);
        let mut aliases = if internal_loop.is_some() {
            internal_loop_aliases(candidate.action, first)
        } else {
            action_aliases(candidate.action, first)
        };
        if contract
            .reload
            .as_ref()
            .is_some_and(|reload| reload.per_round)
            && candidate.action == WeaponClipAction::Reload
        {
            aliases.push("WPNReloadLoop");
            aliases.push("WPNReloadEnd");
        }
        if fields.automatic
            && candidate.action == WeaponClipAction::Attack
            && internal_loop.is_none()
        {
            aliases.extend(action_aliases(WeaponClipAction::AttackLoop, first));
        }
        aliases.retain(|alias| !source_loop_aliases.contains(alias));
        for alias in &aliases {
            let mut alias_clip = clip.clone();
            if let Some(fire) = internal_loop {
                let start = fire
                    .loop_start_seconds
                    .ok_or("source internal fire loop has no start")?
                    / candidate.playback_speed;
                let end = fire.loop_end_seconds / candidate.playback_speed;
                let marker = if alias.starts_with("WPNChargeUp") {
                    alias_clip = slice_clip(&clip, 0.0, start, false)?;
                    Some(SOURCE_SPIN_START_EVENT)
                } else if alias.starts_with("WPNChargeDown") {
                    alias_clip = slice_clip(&clip, end, clip.duration, true)?;
                    Some(SOURCE_SPIN_STOP_EVENT)
                } else {
                    alias_clip = slice_clip(&clip, start, end, false)?;
                    alias_clip = repeat_fire_clip(
                        &alias_clip,
                        fire.loop_repeat_count,
                        fire.shot_interval_seconds,
                    )?;
                    None
                };
                if let Some(marker) = marker {
                    alias_clip.events.retain(|event| event.text != "weaponFire");
                    alias_clip.events.push(super::AnimationEvent {
                        time: 0.00001,
                        source_time: 0.0,
                        text: marker.into(),
                    });
                    if !attachment_events.iter().any(|event| event == marker) {
                        attachment_events.push(marker.into());
                    }
                }
                source_loop_aliases.insert(*alias);
            } else if let Some(fire) = contract.continuous_fire.as_ref() {
                if matches!(
                    candidate.action,
                    WeaponClipAction::AttackLoop
                        | WeaponClipAction::AttackLoopIronSights
                        | WeaponClipAction::SneakAttackLoop
                        | WeaponClipAction::SneakAttackLoopIronSights
                ) {
                    alias_clip = repeat_fire_clip(
                        &clip,
                        fire.loop_repeat_count,
                        fire.shot_interval_seconds,
                    )?;
                }
            }
            if let Some(reload) = &contract.reload {
                if reload.per_round {
                    if *alias == "WPNReloadLoop" {
                        let end = reload
                            .repeat_until_seconds
                            .ok_or("per-round reload has no repeat boundary")?
                            / candidate.playback_speed;
                        alias_clip = slice_clip(&clip, 0.0, end, false)?;
                        alias_clip.events.retain(|event| event.text != "reloadEnd");
                    } else if *alias == "WPNReloadEnd" {
                        let start = reload
                            .exit_tail_start_seconds
                            .ok_or("per-round reload has no exit tail")?
                            / candidate.playback_speed;
                        alias_clip = slice_clip(&clip, start, clip.duration, true)?;
                    }
                }
            }
            let path = format!("{folder}/{alias}.hkx");
            let declaration = super::bind_fnv_kf_clip(
                &target_pose.bone_names,
                &alias_clip,
                &skeleton_path,
                &path,
                &target_record.name,
                &[],
            )
            .map_err(|error| error.to_string())?;
            let (xml, _) = super::clip_to_havok_xml_with_float_tracks(
                &alias_clip,
                &declaration,
                &[],
                &target_record.name,
                30.0,
                super::FnvExtractedMotionPolicy::RejectNonzero,
            )
            .map_err(|error| error.to_string())?;
            let packed =
                havok_native::api::havok_xml_to_hkx(&xml).map_err(|error| error.to_string())?;
            super::validate_staged_fo4_animation(&packed).map_err(|error| error.to_string())?;
            write_asset(&meshes_root.join(&path), &packed)?;
            outputs.push(path);
        }
        receipts.push(
            json!({"source":candidate.source_path,"action":candidate.action,
            "aliases":aliases,"omitted_actor_channels":omitted,"event_contract":contract}),
        );
    }
    if outputs.is_empty() {
        return Err("weapon actor set emitted no playable clips".into());
    }
    outputs.sort();
    outputs.dedup();
    Ok(StagedActorSet {
        folder,
        clips: outputs,
        attachment_events,
        attack_timing,
        automatic_timing,
        receipt: json!({"id":set_id,"perspective":perspective,"source_grip":fields.grip_animation,
            "grip_overlay_applied":grip.is_some(),"clips":receipts}),
    })
}

fn action_aliases(action: WeaponClipAction, first: bool) -> Vec<&'static str> {
    match action {
        WeaponClipAction::Aim => vec![
            "WPNIdleReady",
            "WPNIdleReadyA",
            "WPNIdleReadyB",
            "WPNIdleReadyC",
            "WPNIdleReadyD",
        ],
        WeaponClipAction::AimIronSights => vec!["WPNIdleSighted"],
        WeaponClipAction::Equip => vec!["WPNEquip", "WPNEquipFast"],
        WeaponClipAction::Unequip => vec!["WPNUnequip"],
        WeaponClipAction::Attack => vec![
            "WPNFireSingleReady",
            "WPNFireSingleReadyA",
            "WPNFireSingleReadyB",
        ],
        WeaponClipAction::AttackIronSights => vec!["WPNFireSingleSighted"],
        WeaponClipAction::AttackLoop if first => vec![
            "WPNFireAutoReady",
            "WPNFireAutoReadyBack",
            "WPNFireAutoReadyForward",
        ],
        WeaponClipAction::AttackLoop => vec!["WPNFireAutoReady"],
        WeaponClipAction::AttackLoopIronSights => vec!["WPNFireAutoSighted"],
        WeaponClipAction::Reload => vec!["WPNReload"],
        WeaponClipAction::ReloadStart => vec!["WPNReloadStart"],
        WeaponClipAction::ReloadEnd => vec!["WPNReloadEnd"],
        WeaponClipAction::SneakAim => vec!["SneakWPNIdleReady"],
        WeaponClipAction::SneakAimIronSights => vec!["SneakWPNIdleSighted"],
        WeaponClipAction::SneakEquip => vec!["SneakWPNEquip"],
        WeaponClipAction::SneakUnequip => vec!["SneakWPNUnequip"],
        WeaponClipAction::SneakAttack => vec!["SneakWPNFireSingleReady"],
        WeaponClipAction::SneakAttackIronSights => vec!["SneakWPNFireSingleSighted"],
        WeaponClipAction::SneakAttackLoop => vec!["SneakWPNFireAutoReady"],
        WeaponClipAction::SneakAttackLoopIronSights => vec!["SneakWPNFireAutoSighted"],
        WeaponClipAction::SneakReload => vec!["SneakWPNReload"],
        _ => Vec::new(),
    }
}

fn internal_loop_aliases(action: WeaponClipAction, first: bool) -> Vec<&'static str> {
    match action {
        WeaponClipAction::Attack => {
            let mut aliases = vec!["WPNChargeUp", "WPNChargeDown"];
            aliases.extend(action_aliases(WeaponClipAction::AttackLoop, first));
            aliases
        }
        WeaponClipAction::AttackIronSights => vec![
            "WPNChargeUpSighted",
            "WPNChargeDownSighted",
            "WPNFireAutoSighted",
        ],
        WeaponClipAction::SneakAttack => vec!["SneakWPNFireAutoReady"],
        WeaponClipAction::SneakAttackIronSights => vec!["SneakWPNFireAutoSighted"],
        _ => Vec::new(),
    }
}

pub(super) fn repeat_fire_clip(
    clip: &super::AnimationClip,
    repeats: usize,
    interval: f64,
) -> Result<super::AnimationClip, String> {
    if repeats == 0 || !interval.is_finite() || interval <= 0.0 {
        return Err("invalid automatic clip cadence".into());
    }
    let mut result = clip.clone();
    result.duration *= repeats as f64;
    for channel in &mut result.channels {
        for keys in [
            &mut channel.rotations,
            &mut channel.translations,
            &mut channel.scales,
        ] {
            let original = keys.clone();
            keys.clear();
            for cycle in 0..repeats {
                for key in &original {
                    if cycle + 1 < repeats && key.time >= clip.duration - 1e-6 {
                        continue;
                    }
                    let mut key = key.clone();
                    key.time += cycle as f64 * clip.duration;
                    keys.push(key);
                }
            }
        }
    }
    for channel in &mut result.float_channels {
        let original = channel.keyframes.clone();
        channel.keyframes.clear();
        for cycle in 0..repeats {
            channel.keyframes.extend(original.iter().filter(|key| {
                cycle + 1 == repeats || key.time < clip.duration - 1e-6
            }).map(|key| {
                let mut key = key.clone();
                key.time += cycle as f64 * clip.duration;
                key
            }));
        }
    }
    result.events.clear();
    for cycle in 0..repeats {
        result.events.extend(
            clip.events
                .iter()
                .filter(|event| event.text != "weaponFire")
                .map(|event| super::AnimationEvent {
                    time: event.time + cycle as f64 * clip.duration,
                    source_time: event.source_time,
                    text: event.text.clone(),
                }),
        );
    }
    let count = (result.duration / interval).round() as usize;
    for index in 0..count {
        result.events.push(super::AnimationEvent {
            time: (index as f64 * interval).max(0.00001),
            source_time: index as f64 * interval,
            text: "weaponFire".into(),
        });
    }
    result
        .events
        .sort_by(|left, right| left.time.total_cmp(&right.time));
    Ok(result)
}

fn apply_finger_grip(clip: &mut super::AnimationClip, grip: &super::AnimationClip) {
    for channel in &grip.channels {
        let name = channel.bone_name.to_ascii_lowercase();
        if !name.contains("finger") && !name.contains("thumb") {
            continue;
        }
        if let Some(existing) = clip
            .channels
            .iter_mut()
            .find(|existing| existing.bone_name.eq_ignore_ascii_case(&channel.bone_name))
        {
            *existing = channel.clone();
        } else {
            clip.channels.push(channel.clone());
        }
    }
}

pub(super) fn slice_clip(
    clip: &super::AnimationClip,
    start: f64,
    end: f64,
    drop_start_events: bool,
) -> Result<super::AnimationClip, String> {
    if !start.is_finite()
        || !end.is_finite()
        || start < 0.0
        || end <= start
        || end > clip.duration + 1.0e-5
    {
        return Err("invalid source weapon clip slice".into());
    }
    let mut result = clip.clone();
    result.duration = end - start;
    for channel in &mut result.channels {
        for keys in [
            &mut channel.rotations,
            &mut channel.translations,
            &mut channel.scales,
        ] {
            let mut times = vec![start, end];
            times.extend(
                keys.iter()
                    .map(|key| key.time)
                    .filter(|time| *time > start && *time < end),
            );
            times.sort_by(f64::total_cmp);
            let original = keys.clone();
            *keys = times
                .into_iter()
                .map(|time| super::AnimationKeyframe {
                    time: time - start,
                    value: super::interp_keyframes(&original, time),
                    interpolation: super::Interpolation::Linear,
                })
                .collect();
        }
    }
    for channel in &mut result.float_channels {
        let original = channel.keyframes.clone();
        let mut times = vec![start, end];
        times.extend(original.iter().map(|key| key.time).filter(|time| *time > start && *time < end));
        times.sort_by(f64::total_cmp);
        channel.keyframes = times.into_iter().map(|time| super::AnimationKeyframe {
            time: time - start,
            value: super::interp_keyframes(&original, time),
            interpolation: super::Interpolation::Linear,
        }).collect();
    }
    result.events.retain(|event| {
        event.time >= start
            && event.time <= end
            && (!drop_start_events || event.time > start + 1.0e-5)
    });
    for event in &mut result.events {
        event.time -= start;
    }
    Ok(result)
}

fn write_asset(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn resolve_source(roots: &[PathBuf], relative: &str) -> Result<PathBuf, String> {
    roots
        .iter()
        .map(|root| root.join(relative.replace('\\', "/")))
        .find(|path| path.is_file())
        .ok_or_else(|| format!("source asset is missing: {relative}"))
}

pub(super) fn source_weapon_paths(roots: &[PathBuf]) -> Result<Vec<String>, String> {
    let mut paths = HashSet::new();
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
                    paths.insert(format!(
                        "{folder}/{}",
                        path.file_name().unwrap().to_string_lossy()
                    ));
                }
            }
        }
    }
    Ok(paths.into_iter().collect())
}

fn editor_id(record: &Record, interner: &StringInterner) -> Option<String> {
    record
        .fields
        .iter()
        .find_map(|field| match (&field.sig.0, &field.value) {
            (b"EDID", FieldValue::String(symbol)) => interner.resolve(*symbol).map(str::to_string),
            _ => None,
        })
}

fn model_path(record: &Record, signature: &[u8; 4], interner: &StringInterner) -> Option<String> {
    record
        .fields
        .iter()
        .find_map(|field| match (&field.sig.0, &field.value) {
            (sig, FieldValue::String(symbol)) if sig == signature => {
                interner.resolve(*symbol).map(str::to_string)
            }
            _ => None,
        })
}

fn set_model_path(record: &mut Record, signature: &[u8; 4], path: &str, interner: &StringInterner) {
    let value = FieldValue::String(interner.intern(&path.replace('/', "\\")));
    if let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *signature)
    {
        field.value = value;
    } else {
        record.fields.push(crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig(*signature),
            value,
        });
    }
}

fn normalize_baked_weapon_stats(
    record: &mut Record,
    interner: &StringInterner,
    attack_timing: Option<&WeaponAttackTiming>,
    automatic_timing: Option<&AutomaticWeaponTiming>,
) -> Result<(), String> {
    let attack_seconds = attack_timing
        .map(|timing| timing.animation_attack_seconds)
        .or_else(|| automatic_timing.map(|timing| timing.animation_attack_seconds));
    let charge_seconds = automatic_timing.and_then(|timing| timing.charge_seconds);
    let field = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"DNAM")
        .ok_or("generated weapon profile is missing target DNAM")?;
    // Source multipliers are baked into these clips. A base speed of one lets
    // bound and unbound FO4 clip routes agree while retaining runtime modifiers.
    match &mut field.value {
        FieldValue::Bytes(bytes) if bytes.len() >= 12 => {
            bytes[4..8].copy_from_slice(&1.0f32.to_le_bytes());
            if let Some(seconds) = attack_seconds {
                if bytes.len() < 110 {
                    return Err("generated weapon DNAM is truncated".into());
                }
                let mut flags = u32::from_le_bytes(bytes[48..52].try_into().unwrap())
                    & !(0x0040_0000 | 0x0000_0200);
                if charge_seconds.is_some() {
                    flags |= 0x0000_0200;
                }
                bytes[48..52].copy_from_slice(&flags.to_le_bytes());
                if let Some(timing) = attack_timing {
                    bytes[24..28]
                        .copy_from_slice(&(timing.attack_delay_seconds as f32).to_le_bytes());
                }
                bytes[106..110].copy_from_slice(&(seconds as f32).to_le_bytes());
            }
        }
        FieldValue::Struct(fields) => {
            let speed = fields
                .iter_mut()
                .find(|(name, _)| interner.resolve(*name) == Some("speed"))
                .ok_or("generated weapon DNAM has no speed field")?;
            speed.1 = FieldValue::Float(1.0);
            if let Some(seconds) = attack_seconds {
                for (name, seconds) in std::iter::once(("animation_attack_seconds", seconds)).chain(
                    attack_timing.map(|timing| ("attack_delay", timing.attack_delay_seconds)),
                ) {
                    let entry = fields
                        .iter_mut()
                        .find(|(key, _)| interner.resolve(*key) == Some(name))
                        .ok_or_else(|| format!("generated weapon DNAM has no {name}"))?;
                    entry.1 = FieldValue::Float(seconds as f32);
                }
                let entry = fields
                    .iter_mut()
                    .find(|(key, _)| interner.resolve(*key) == Some("flags"))
                    .ok_or("generated weapon DNAM has no flags")?;
                let FieldValue::Uint(flags) = &mut entry.1 else {
                    return Err("generated weapon DNAM flags are not numeric".into());
                };
                // FNV's shot clip already includes the bolt cycle. FO4's extra
                // BoltAction state would play a second bolt animation.
                *flags &= !(0x0040_0000 | 0x0000_0200);
                if charge_seconds.is_some() {
                    *flags |= 0x0000_0200;
                }
            }
        }
        _ => return Err("generated weapon profile has malformed target DNAM".into()),
    }
    if let Some(seconds) = charge_seconds {
        let field = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"FNAM")
            .ok_or("generated charging weapon has no FNAM")?;
        match &mut field.value {
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                bytes[..4].copy_from_slice(&(seconds as f32).to_le_bytes());
            }
            FieldValue::Struct(fields) => {
                let entry = fields
                    .iter_mut()
                    .find(|(key, _)| interner.resolve(*key) == Some("animation_fire_seconds"))
                    .ok_or("generated charging weapon FNAM has no fire duration")?;
                entry.1 = FieldValue::Float(seconds as f32);
            }
            _ => return Err("generated charging weapon has malformed FNAM".into()),
        }
    }
    Ok(())
}

fn sound_editor_id_map(
    session: &mut crate::session::PluginSession,
    mapper: &mut FormKeyMapper,
    source_schema: &crate::schema::AuthoringSchema,
    target_schema: &crate::schema::AuthoringSchema,
    config: &FixupConfig,
) -> Result<HashMap<String, String>, PhaseError> {
    let mut map = HashMap::new();
    let keys = session
        .source_form_keys_of_sig(SigCode(*b"SOUN"), mapper.interner)
        .map_err(|error| PhaseError::Internal(error.to_string()))?;
    for source_key in keys {
        let Some(target_key) = mapper.lookup(source_key) else {
            continue;
        };
        let source = session
            .source_record_decoded(&source_key, source_schema, mapper.interner)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let Some(mut target) = session.record_decoded(&target_key, target_schema, mapper.interner).ok()
            .or_else(|| crate::fixups::face::generate_additive_races::load_target_race_template_from_masters(
                session, target_schema, mapper, config, target_key,
            ))
        else {
            continue;
        };
        if target.sig.as_str() == "SOUN" {
            let descriptor =
                target
                    .fields
                    .iter()
                    .find_map(|field| match (&field.sig.0, &field.value) {
                        (b"SDSC", FieldValue::FormKey(key)) => Some(*key),
                        _ => None,
                    });
            if let Some(descriptor) = descriptor {
                if let Some(record) =
                    session.record_decoded(&descriptor, target_schema, mapper.interner).ok()
                        .or_else(|| crate::fixups::face::generate_additive_races::load_target_race_template_from_masters(
                            session, target_schema, mapper, config, descriptor,
                        ))
                {
                    target = record;
                }
            }
        }
        if target.sig.as_str() == "SNDR" {
            if let (Some(source), Some(target)) = (
                editor_id(&source, mapper.interner),
                editor_id(&target, mapper.interner),
            ) {
                map.insert(source.to_ascii_lowercase(), target);
            }
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FormKey;
    #[test]
    fn multi_unit_ammo_catalog_uses_registered_target_ownership_and_source_units() {
        let interner = StringInterner::new();
        let profiles = [(0x800, 0), (0x801, 1), (0x802, 3), (0x803, 255)].map(|(local, units)| {
            (
                FormKey {
                    plugin: interner.intern("FNV_FO3.esm"),
                    local,
                },
                units,
            )
        });
        assert_eq!(legacy_ammo_catalog(profiles, &interner), json!({
            "version":1,"source_game":"fnv","weapons":[
                {"weapon":"000802:FNV_FO3.esm","units_per_shot":3},
                {"weapon":"000803:FNV_FO3.esm","units_per_shot":255}
            ]
        }));
    }

    #[test]
    fn automatic_loop_wrap_preserves_motion_and_record_shot_cadence() {
        let clip = super::super::AnimationClip {
            duration: 2.0 / 3.0,
            channels: vec![super::super::BoneChannel {
                bone_name: "Barrel".into(),
                translations: [0.0, 1.0 / 3.0, 2.0 / 3.0]
                    .into_iter()
                    .map(|time| super::super::AnimationKeyframe {
                        time,
                        value: vec![time, 0.0, 0.0],
                        interpolation: super::super::Interpolation::Linear,
                    })
                    .collect(),
                rotations: Vec::new(),
                scales: Vec::new(),
            }],
            events: vec![super::super::AnimationEvent {
                time: 0.1,
                source_time: 0.1,
                text: "weaponFire".into(),
            }],
            ..Default::default()
        };
        let repeated = repeat_fire_clip(&clip, 3, 0.05).unwrap();
        assert_eq!(repeated.duration, 2.0);
        assert_eq!(repeated.events.len(), 40);
        for (index, event) in repeated.events.iter().enumerate() {
            assert!((event.time - (index as f64 * 0.05).max(0.00001)).abs() < 1e-6);
        }
        let keys = &repeated.channels[0].translations;
        assert_eq!(keys.len(), 7);
        assert!(keys.windows(2).all(|pair| pair[0].time < pair[1].time));
        assert_eq!(keys.last().unwrap().time, 2.0);
    }

    #[test]
    fn per_round_reload_tail_does_not_repeat_ammo_transfer() {
        let clip = super::super::AnimationClip {
            duration: 1.5,
            events: [
                (0.0, "reloadStart"),
                (0.6, "reloadComplete"),
                (1.5, "reloadEnd"),
            ]
            .into_iter()
            .map(|(time, text)| super::super::AnimationEvent {
                time,
                source_time: time,
                text: text.into(),
            })
            .collect(),
            ..Default::default()
        };
        let loop_clip = slice_clip(&clip, 0.0, 0.6, false).unwrap();
        let tail = slice_clip(&clip, 0.6, clip.duration, true).unwrap();
        assert_eq!(
            loop_clip
                .events
                .iter()
                .filter(|event| event.text == "reloadComplete")
                .count(),
            1
        );
        assert_eq!(tail.events.len(), 1);
        assert_eq!(tail.events[0].text, "reloadEnd");
        assert!((tail.events[0].time - 0.9).abs() < 1.0e-5);
    }

    #[test]
    fn actor_master_aliases_do_not_duplicate_fire_events_on_slaves_or_additives() {
        for perspective in [false, true] {
            for action in [WeaponClipAction::Attack, WeaponClipAction::AttackLoop] {
                let aliases = action_aliases(action, perspective);
                assert!(!aliases.is_empty());
                assert!(
                    aliases
                        .iter()
                        .all(|name| !name.contains("Slave") && !name.contains("Add"))
                );
            }
        }
    }

    #[test]
    fn baked_clip_speed_keeps_other_target_weapon_stats() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode(*b"WEAP"),
            crate::ids::FormKey {
                plugin: interner.intern("Output.esm"),
                local: 0x800,
            },
        );
        let mut bytes = vec![0xA5; 272];
        bytes[4..8].copy_from_slice(&1.25f32.to_le_bytes());
        let before = bytes.clone();
        record.fields.push(crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(bytes.into()),
        });
        normalize_baked_weapon_stats(&mut record, &interner, None, None).unwrap();
        let FieldValue::Bytes(after) = &record.fields[0].value else {
            panic!()
        };
        assert_eq!(&after[..4], &before[..4]);
        assert_eq!(&after[8..], &before[8..]);
        assert_eq!(&after[4..8], &1.0f32.to_le_bytes());
    }

    #[test]
    fn source_combined_bolt_cycle_sets_cooldown_without_fo4_second_bolt() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode(*b"WEAP"),
            crate::ids::FormKey {
                plugin: interner.intern("Output.esm"),
                local: 0x800,
            },
        );
        let mut bytes = vec![0; 132];
        bytes[48..52].copy_from_slice(&0x0040_0100u32.to_le_bytes());
        bytes[67..69].copy_from_slice(&37u16.to_le_bytes());
        record.fields.push(crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(bytes.into()),
        });
        let timing = WeaponAttackTiming {
            attack_delay_seconds: 1.055_555_6,
            animation_attack_seconds: 1.194_444_4,
            refire_authority: "source_a_marker".into(),
            combined_source_bolt_cycle: true,
        };
        normalize_baked_weapon_stats(&mut record, &interner, Some(&timing), None).unwrap();
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!()
        };
        assert_eq!(u32::from_le_bytes(bytes[48..52].try_into().unwrap()), 0x100);
        assert_eq!(u16::from_le_bytes(bytes[67..69].try_into().unwrap()), 37);
        assert!(
            (f32::from_le_bytes(bytes[24..28].try_into().unwrap()) - 1.055_555_6).abs() < 1.0e-6
        );
        assert!(
            (f32::from_le_bytes(bytes[106..110].try_into().unwrap()) - 1.194_444_4).abs() < 1.0e-6
        );
    }

    #[test]
    fn automatic_rate_and_charge_duration_follow_source_timing() {
        let interner = StringInterner::new();
        for structured in [false, true] {
            let mut record = Record::new(
                SigCode(*b"WEAP"),
                crate::ids::FormKey {
                    plugin: interner.intern("Output.esm"),
                    local: 0x800,
                },
            );
            let (dnam, fnam) = if structured {
                (
                    FieldValue::Struct(vec![
                        (interner.intern("speed"), FieldValue::Float(3.0)),
                        (interner.intern("flags"), FieldValue::Uint(0x8000)),
                        (
                            interner.intern("animation_attack_seconds"),
                            FieldValue::Float(0.0),
                        ),
                    ]),
                    FieldValue::Struct(vec![(
                        interner.intern("animation_fire_seconds"),
                        FieldValue::Float(1.0),
                    )]),
                )
            } else {
                let mut bytes = vec![0; 132];
                bytes[4..8].copy_from_slice(&3.0f32.to_le_bytes());
                bytes[48..52].copy_from_slice(&0x8000u32.to_le_bytes());
                (
                    FieldValue::Bytes(bytes.into()),
                    FieldValue::Bytes(vec![0; 41].into()),
                )
            };
            for (sig, value) in [(b"DNAM", dnam), (b"FNAM", fnam)] {
                record.fields.push(crate::record::FieldEntry {
                    sig: crate::ids::SubrecordSig(*sig),
                    value,
                });
            }
            let timing = AutomaticWeaponTiming {
                animation_attack_seconds: 20.0 / 20.0,
                charge_seconds: Some(0.2),
            };
            normalize_baked_weapon_stats(&mut record, &interner, None, Some(&timing)).unwrap();
            match &record.fields[0].value {
                FieldValue::Bytes(bytes) => {
                    assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1.0);
                    assert_eq!(f32::from_le_bytes(bytes[106..110].try_into().unwrap()), 1.0);
                    assert_eq!(
                        u32::from_le_bytes(bytes[48..52].try_into().unwrap()),
                        0x8200
                    );
                }
                FieldValue::Struct(fields) => {
                    for (name, expected) in [
                        ("speed", FieldValue::Float(1.0)),
                        ("animation_attack_seconds", FieldValue::Float(1.0)),
                        ("flags", FieldValue::Uint(0x8200)),
                    ] {
                        assert_eq!(
                            &fields
                                .iter()
                                .find(|(key, _)| interner.resolve(*key) == Some(name))
                                .unwrap()
                                .1,
                            &expected
                        );
                    }
                }
                _ => panic!(),
            }
            match &record.fields[1].value {
                FieldValue::Bytes(bytes) => {
                    assert_eq!(f32::from_le_bytes(bytes[..4].try_into().unwrap()), 0.2)
                }
                FieldValue::Struct(fields) => assert_eq!(fields[0].1, FieldValue::Float(0.2)),
                _ => panic!(),
            }
        }
    }

    #[test]
    #[ignore = "requires locally extracted FNV and Fallout 4 assets"]
    fn real_9mm_actor_sets_stage_with_source_events_for_both_perspectives() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let source_roots = vec![root.join("extracted/fnv")];
        let paths = source_weapon_paths(&source_roots).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let fields = WeaponAnimationFields {
            animation_type: 3,
            grip_animation: 255,
            attack_animation: 32,
            reload_animation: 11,
            ammo_use: 1,
            automatic: false,
            per_round_reload: false,
            animation_multiplier: 1.0,
            attack_multiplier: 1.25,
            fire_rate: 1.0,
            attack_shots_sec: 3.125,
            reload_time: 1.6667,
        };
        for perspective in [WeaponPerspective::Third, WeaponPerspective::First] {
            let staged = stage_actor_set(
                &fields,
                perspective,
                "B21_Test9mm",
                &source_roots,
                &paths,
                &root.join("extracted/fo4"),
                temporary.path(),
                &HashMap::new(),
            )
            .unwrap();
            assert!(
                staged
                    .clips
                    .iter()
                    .any(|path| path.ends_with("WPNFireSingleReady.hkx"))
            );
            assert!(
                staged
                    .clips
                    .iter()
                    .any(|path| path.ends_with("WPNReload.hkx"))
            );
            for clip in &staged.clips {
                super::super::validate_staged_fo4_animation(
                    &std::fs::read(temporary.path().join(clip)).unwrap(),
                )
                .unwrap();
            }
        }
    }

    #[test]
    #[ignore = "requires locally extracted FNV and Fallout 4 assets"]
    fn real_loop_only_assault_carbine_normalizes_source_cadence() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let source_roots = vec![root.join("extracted/fnv")];
        let paths = source_weapon_paths(&source_roots).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let fields = WeaponAnimationFields {
            animation_type: 6,
            grip_animation: 255,
            attack_animation: 74,
            reload_animation: 5,
            ammo_use: 1,
            automatic: true,
            per_round_reload: false,
            animation_multiplier: 1.0,
            attack_multiplier: 1.0,
            fire_rate: 12.0,
            attack_shots_sec: 12.0,
            reload_time: 2.0,
        };
        for perspective in [WeaponPerspective::Third, WeaponPerspective::First] {
            let staged = stage_actor_set(
                &fields,
                perspective,
                "TestAssaultCarbine",
                &source_roots,
                &paths,
                &root.join("extracted/fo4"),
                temporary.path(),
                &HashMap::new(),
            )
            .unwrap();
            let timing = staged.automatic_timing.as_ref().unwrap();
            assert!(timing.charge_seconds.is_none());
            let interner = StringInterner::new();
            let mut record = Record::new(
                SigCode(*b"WEAP"),
                FormKey {
                    plugin: interner.intern("Test.esm"),
                    local: 0x800,
                },
            );
            let mut dnam = vec![0; 130];
            dnam[4..8].copy_from_slice(&3.0f32.to_le_bytes());
            dnam[48..52].copy_from_slice(&0x8200u32.to_le_bytes());
            dnam[106..110].copy_from_slice(&2.2f32.to_le_bytes());
            record.fields.push(crate::record::FieldEntry {
                sig: crate::ids::SubrecordSig(*b"DNAM"),
                value: FieldValue::Bytes(dnam.into()),
            });
            normalize_baked_weapon_stats(&mut record, &interner, None, Some(timing)).unwrap();
            let FieldValue::Bytes(dnam) = &record.fields[0].value else {
                panic!();
            };
            let speed = f32::from_le_bytes(dnam[4..8].try_into().unwrap());
            let seconds = f32::from_le_bytes(dnam[106..110].try_into().unwrap());
            assert_eq!(speed, 1.0);
            assert!((20.0 / (seconds / speed) - fields.fire_rate as f32).abs() < 1e-5);
            assert_eq!(u32::from_le_bytes(dnam[48..52].try_into().unwrap()), 0x8000);
        }
    }

    #[test]
    #[ignore = "requires locally extracted FNV and Fallout 4 assets"]
    fn real_minigun_stages_source_startup_loop_and_shutdown_without_tail_shots() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let source_roots = vec![root.join("extracted/fnv")];
        let paths = source_weapon_paths(&source_roots).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        let fields = WeaponAnimationFields {
            animation_type: 8,
            grip_animation: 255,
            attack_animation: 80,
            reload_animation: 4,
            ammo_use: 1,
            automatic: true,
            per_round_reload: false,
            animation_multiplier: 1.0,
            attack_multiplier: 1.0,
            fire_rate: 20.0,
            attack_shots_sec: 20.0,
            reload_time: 4.0,
        };
        for perspective in [WeaponPerspective::Third, WeaponPerspective::First] {
            let staged = stage_actor_set(
                &fields,
                perspective,
                "TestMinigun",
                &source_roots,
                &paths,
                &root.join("extracted/fo4"),
                temporary.path(),
                &HashMap::new(),
            )
            .unwrap();
            assert!(
                (staged
                    .automatic_timing
                    .as_ref()
                    .unwrap()
                    .charge_seconds
                    .unwrap()
                    - 0.2)
                    .abs()
                    < 1e-5
            );
            assert!(
                staged
                    .attachment_events
                    .contains(&SOURCE_SPIN_START_EVENT.into())
            );
            assert!(
                staged
                    .attachment_events
                    .contains(&SOURCE_SPIN_STOP_EVENT.into())
            );
            for (alias, expected_duration, marker, fire_count) in [
                ("WPNChargeUp", 0.2, Some(SOURCE_SPIN_START_EVENT), 0),
                ("WPNFireAutoReady", 2.0, None, 40),
                ("WPNChargeDown", 0.8666667, Some(SOURCE_SPIN_STOP_EVENT), 0),
            ] {
                let path = staged
                    .clips
                    .iter()
                    .find(|path| path.ends_with(&format!("/{alias}.hkx")))
                    .unwrap();
                let bytes = std::fs::read(temporary.path().join(path)).unwrap();
                super::super::validate_staged_fo4_animation(&bytes).unwrap();
                let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
                let doc = roxmltree::Document::parse(&xml).unwrap();
                let animation = doc
                    .descendants()
                    .find(|node| {
                        node.attribute("class") == Some("hkaInterleavedUncompressedAnimation")
                    })
                    .unwrap();
                let duration: f64 = animation
                    .children()
                    .find(|node| node.attribute("name") == Some("duration"))
                    .unwrap()
                    .text()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(
                    (duration - expected_duration).abs() < 1e-4,
                    "{alias}: {duration}"
                );
                let annotations: Vec<_> = animation
                    .descendants()
                    .filter(|node| node.attribute("name") == Some("annotations"))
                    .flat_map(|node| node.children().filter(roxmltree::Node::is_element))
                    .collect();
                let annotation_text = |node: &roxmltree::Node<'_, '_>| {
                    node.children()
                        .find(|child| child.attribute("name") == Some("text"))
                        .and_then(|child| child.text())
                        .unwrap_or("")
                        .to_string()
                };
                assert_eq!(
                    annotations
                        .iter()
                        .filter(|node| annotation_text(node) == "weaponFire")
                        .count(),
                    fire_count,
                    "{alias}"
                );
                if let Some(marker) = marker {
                    assert!(
                        annotations
                            .iter()
                            .any(|node| annotation_text(node) == marker),
                        "{alias}"
                    );
                }
            }
        }
    }
}
