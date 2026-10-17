use super::*;
use crate::source_rig::{CapabilityBlenderChild, CapabilityLocomotionDirection};

#[cfg(test)]
#[path = "creature_action_corpus_tests.rs"]
mod source_corpus_tests;

pub(super) fn configure_attack_request(
    family: &CreatureFamilyRecipeJob,
    selection: &SelectedClip,
    request: &mut crate::phase::animations::FnvKfStageRequest<'_>,
) -> Result<(), String> {
    use crate::phase::animations::fnv_creature_events::FnvCreatureAttackKind;
    request.attack_kind = match selection.role {
        CreatureClipRole::MeleeAttack => Some(FnvCreatureAttackKind::Melee),
        CreatureClipRole::ProjectileAttack
        | CreatureClipRole::ContinuousAttackStart
        | CreatureClipRole::ContinuousAttackLoop
        | CreatureClipRole::ContinuousAttackStop => Some(FnvCreatureAttackKind::Ranged),
        _ => None,
    };
    request.automatic_attack = selection
        .automatic_attack
        .as_ref()
        .map(|attack| attack.segment);
    request.equipment_index = selection.equipment_index;
    if let Some(segment) = &mut request.automatic_attack {
        segment.equip_index = selection.equipment_index.map(u32::from);
    }
    if request.automatic_attack.is_none()
        && let Some(source) = &selection.source_weapon
        && let Some(motion) = family.indexed_motion_evidence.as_ref()
    {
        let weapon = motion
            .weapon_attacks
            .iter()
            .find(|weapon| &weapon.source == source)
            .ok_or_else(|| format!("selected attack weapon {source} has no source timing"))?;
        if selection.role == CreatureClipRole::ProjectileAttack
            && matches!(weapon.animation.animation_type, 10 | 13)
        {
            if selection.equipment_index.is_none() {
                return Err(format!(
                    "selected thrown weapon {source} has no equipment index"
                ));
            }
            request.attack_kind = Some(FnvCreatureAttackKind::Thrown);
        }
        request.attack_playback_speed =
            Some(weapon.animation.animation_multiplier * weapon.animation.attack_multiplier);
    }
    Ok(())
}

fn leaf(candidate: &MotionCandidate) -> String {
    candidate
        .source_kf
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&candidate.source_kf)
        .to_ascii_lowercase()
}

fn ordinary_motion_rank(candidate: &MotionCandidate) -> (bool, bool, String) {
    let leaf = leaf(candidate);
    (leaf.contains("hurt"), !leaf.starts_with("mt"), leaf)
}

fn direction(candidate: &MotionCandidate) -> Option<u8> {
    let name = candidate.sequence_name.to_ascii_lowercase();
    if name.contains("forward") {
        Some(0)
    } else if name.contains("backward") {
        Some(2)
    } else if name.contains("left") {
        Some(3)
    } else if name.contains("right") {
        Some(1)
    } else {
        None
    }
}

fn source_speed(family: &CreatureFamilyRecipeJob, candidate: &MotionCandidate) -> Option<f32> {
    let RootMotionEvidence::Planar { distance, .. } = candidate.root_motion else {
        return None;
    };
    let sequence = family
        .indexed_motion_evidence
        .as_ref()?
        .kf_evidence
        .iter()
        .find_map(|evidence| {
            if evidence
                .source_kf
                .replace('\\', "/")
                .eq_ignore_ascii_case(&candidate.source_kf.replace('\\', "/"))
            {
                if let KfParseEvidence::Parsed(sequence) = &evidence.sequence {
                    if sequence.sequence_index == candidate.sequence_index {
                        return Some(sequence);
                    }
                }
            }
            None
        })?;
    let duration = (sequence.stop_time - sequence.start_time) / sequence.frequency;
    let speed = distance.abs() / duration;
    (duration > 0.0 && speed.is_finite() && speed > 0.0).then_some(speed as f32)
}

pub(super) fn add_locomotion_clips(
    family: &CreatureFamilyRecipeJob,
    clips: &mut Vec<SelectedClip>,
    used_events: &mut BTreeSet<String>,
    name: &str,
    state_name: &str,
) -> Result<(), String> {
    let mut candidates = motion_candidates(family, MotionRole::GroundLocomotion)?
        .iter()
        .filter(|candidate| candidate.looping && !leaf(candidate).contains("swim"))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|candidate| ordinary_motion_rank(candidate));
    let mut samples = BTreeMap::new();
    for candidate in candidates.iter().copied() {
        let Some(direction) = direction(candidate) else {
            continue;
        };
        let Some(speed) = source_speed(family, candidate) else {
            continue;
        };
        let fast = candidate
            .sequence_name
            .to_ascii_lowercase()
            .contains("fast")
            || leaf(candidate).contains("run");
        samples
            .entry((direction, fast))
            .or_insert((candidate.clone(), speed));
    }
    if !samples.keys().any(|(direction, _)| *direction == 0) {
        let candidate = candidates
            .iter()
            .find(|candidate| direction(candidate) == Some(0))
            .or_else(|| candidates.first())
            .ok_or_else(|| "ground locomotion has no looping source sequence".to_string())?;
        return add_selected_clip(
            clips,
            used_events,
            (**candidate).clone(),
            CreatureClipRole::GroundForward,
            name,
            state_name,
            true,
            false,
        );
    }
    let mut ordered = samples.into_iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.0
            .0
            .cmp(&right.0.0)
            .then_with(|| left.1.1.total_cmp(&right.1.1))
    });
    ordered.dedup_by(|right, left| left.0.0 == right.0.0 && (left.1.1 - right.1.1).abs() < 0.001);
    for (index, ((direction, fast), (candidate, speed))) in ordered.into_iter().enumerate() {
        let clip_name = if index == 0 {
            name.to_string()
        } else {
            format!(
                "{name}_direction{direction}_{}",
                if fast { "run" } else { "walk" }
            )
        };
        add_selected_clip(
            clips,
            used_events,
            candidate,
            CreatureClipRole::GroundForward,
            &clip_name,
            state_name,
            index == 0,
            false,
        )?;
        clips.last_mut().unwrap().locomotion_sample = Some((direction as f32 * 0.25, speed));
    }
    Ok(())
}

pub(super) fn add_lifecycle_clips(
    family: &CreatureFamilyRecipeJob,
    clips: &mut Vec<SelectedClip>,
    used_events: &mut BTreeSet<String>,
) -> Result<(), String> {
    for (motion_role, role, token, name, state_name) in [
        (
            MotionRole::Turn,
            CreatureClipRole::TurnLeft,
            "left",
            "turn_left",
            "TurnLeft",
        ),
        (
            MotionRole::Turn,
            CreatureClipRole::TurnRight,
            "right",
            "turn_right",
            "TurnRight",
        ),
        (MotionRole::Hurt, CreatureClipRole::Hurt, "", "hurt", "Hurt"),
        (
            MotionRole::Death,
            CreatureClipRole::Death,
            "",
            "death",
            "Death",
        ),
    ] {
        let Ok(candidates) = motion_candidates(family, motion_role) else {
            continue;
        };
        let selected = candidates
            .iter()
            .filter(|candidate| {
                candidate.looping == clip_role_expects_looping(role)
                    && candidate.sequence_name.to_ascii_lowercase().contains(token)
                    && !leaf(candidate).contains("swim")
            })
            .min_by_key(|candidate| ordinary_motion_rank(candidate));
        if let Some(candidate) = selected {
            add_selected_clip(
                clips,
                used_events,
                candidate.clone(),
                role,
                name,
                state_name,
                true,
                false,
            )?;
        }
    }
    Ok(())
}

pub(super) fn role_generator(
    selected: &SelectedGraph,
    clip: &SelectedClip,
) -> CapabilityRoleGenerator {
    if let Some(generator) = &clip.source_idle {
        return generator.clone();
    }
    if clip.locomotion_sample.is_none() {
        return CapabilityRoleGenerator::Single;
    }
    let mut directions = BTreeMap::<u32, Vec<CapabilityBlenderChild>>::new();
    for sample in &selected.clips {
        if let Some((direction, speed)) = sample.locomotion_sample {
            directions
                .entry(direction.to_bits())
                .or_default()
                .push(CapabilityBlenderChild {
                    clip_name: sample.name.clone(),
                    weight_bits: speed.to_bits(),
                    world_from_model_weight_bits: 1.0_f32.to_bits(),
                });
        }
    }
    CapabilityRoleGenerator::Locomotion {
        directions: directions
            .into_iter()
            .map(|(direction_bits, speeds)| CapabilityLocomotionDirection {
                direction_bits,
                speeds,
            })
            .collect(),
    }
}

pub(super) fn add_source_idle_clips(
    family: &CreatureFamilyRecipeJob,
    clips: &mut Vec<SelectedClip>,
    used_events: &mut BTreeSet<String>,
) -> Result<(), String> {
    let Ok(candidates) = motion_candidates(family, MotionRole::ScriptedIdle) else {
        return Ok(());
    };
    let mut bindings = BTreeMap::new();
    for candidate in candidates {
        for claim in &candidate.idle_claims {
            if let Some((previous, _)) =
                bindings.insert(claim.source_idle.clone(), (candidate, claim))
            {
                if motion_candidate_hash(previous) != motion_candidate_hash(candidate) {
                    return Err(format!(
                        "source IDLE {} binds more than one KF in its family",
                        claim.source_idle
                    ));
                }
            }
        }
    }
    for (source_idle, (candidate, claim)) in bindings {
        let event =
            super::super::creature_source_idles::source_idle_event(family.rig.game, &source_idle);
        if !used_events.insert(event.to_ascii_lowercase()) {
            return Err(format!(
                "source IDLE {} repeats graph event {event}",
                source_idle
            ));
        }
        let name = format!(
            "source_idle_{}",
            event.strip_prefix("B21_SourceIdle_").unwrap()
        );
        add_selected_clip(
            clips,
            used_events,
            candidate.clone(),
            if candidate.looping {
                CreatureClipRole::ScriptedLoop
            } else {
                CreatureClipRole::ScriptedAction
            },
            &name,
            &name,
            false,
            false,
        )?;
        let generator =
            source_idle_generator(family, candidate, claim, format!("{event}_Complete"))?;
        let selected = clips.last_mut().unwrap();
        selected.trigger_event = Some(event);
        selected.source_idle = Some(generator);
    }
    Ok(())
}

fn source_idle_generator(
    family: &CreatureFamilyRecipeJob,
    candidate: &MotionCandidate,
    claim: &super::super::creature_motion::IdleClaimEvidence,
    complete_event: String,
) -> Result<CapabilityRoleGenerator, String> {
    let sequence = family
        .indexed_motion_evidence
        .as_ref()
        .into_iter()
        .flat_map(|motion| &motion.kf_evidence)
        .find_map(|evidence| match &evidence.sequence {
            KfParseEvidence::Parsed(sequence)
                if evidence
                    .source_kf
                    .replace('\\', "/")
                    .eq_ignore_ascii_case(&candidate.source_kf.replace('\\', "/"))
                    && sequence.sequence_index == candidate.sequence_index =>
            {
                Some(sequence)
            }
            _ => None,
        })
        .ok_or_else(|| format!("source IDLE {} has no indexed KF timing", claim.source_idle))?;
    let seconds = |time| ((time - sequence.start_time) / sequence.frequency) as f32;
    let marker = |name: &str| {
        sequence
            .text_keys
            .iter()
            .find(|event| event.text.trim().eq_ignore_ascii_case(name))
            .map(|event| seconds(event.time))
    };
    let duration = seconds(sequence.stop_time);
    let (start, end, has_loop_markers) = match (marker("StartLoop"), marker("EndLoop")) {
        (Some(start), Some(end)) => (start, end, true),
        (None, None) => (0.0, duration, false),
        _ => {
            return Err(format!(
                "source IDLE {} has an incomplete StartLoop/EndLoop window",
                claim.source_idle
            ));
        }
    };
    if !(duration.is_finite()
        && start.is_finite()
        && end.is_finite()
        && 0.0 <= start
        && start < end
        && end <= duration)
    {
        return Err(format!(
            "source IDLE {} has invalid loop timing",
            claim.source_idle
        ));
    }
    Ok(CapabilityRoleGenerator::SourceIdle {
        source_group: claim.source_group,
        overlay: None,
        duration_bits: duration.to_bits(),
        loop_start_bits: start.to_bits(),
        loop_end_bits: end.to_bits(),
        has_loop_markers,
        looping_min: claim.looping_min,
        looping_max: claim.looping_max,
        complete_event,
    })
}

#[cfg(test)]
mod tests {
    use super::super::super::creature_catalog::{
        CreatureCatalogOptions, build_creature_corpus_plan,
    };
    use super::super::super::creature_dependencies::{
        CreatureDependencyOptions, LegacyPluginLoadOrder,
        build_creature_dependency_ledger_with_load_orders,
        required_fnv_fo3_creature_runtime_dependency_signatures,
    };
    use super::super::super::creature_motion::{
        CreatureMotionFamilyEvidence, build_creature_motion_catalog,
    };
    use super::super::super::creature_recipe::RequiredMotionRoleRecipe;
    use super::*;

    #[test]
    #[ignore = "requires B21_FNV_CREATURE_ACTION_CORPUS pointing to the workspace source/output corpus"]
    fn real_creature_actions_stage_and_pack_connected_graphs() {
        let workspace = PathBuf::from(
            std::env::var("B21_FNV_CREATURE_ACTION_CORPUS").expect("corpus workspace"),
        );
        let mod_root = workspace.join("mods/FNV_FO3");
        let diagnostics: serde_json::Value = serde_json::from_slice(
            &fs::read(mod_root.join("debug/creature_corpus/live_recipe_diagnostics.json")).unwrap(),
        )
        .unwrap();
        let mut evidence: Vec<CreatureMotionFamilyEvidence> =
            serde_json::from_value(diagnostics["motion_families"].clone()).unwrap();
        let plugin_path = PathBuf::from(
            std::env::var("B21_FNV_CREATURE_SOURCE_PLUGIN").expect("source FalloutNV.esm"),
        );
        let signatures = required_fnv_fo3_creature_runtime_dependency_signatures();
        let signature_bytes = signatures
            .iter()
            .map(|signature| signature.as_bytes().try_into().unwrap())
            .collect::<Vec<[u8; 4]>>();
        let source_handle =
            crate::run::OwnedPluginHandle::load_projection(&plugin_path, "fnv", &signature_bytes)
                .unwrap();
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("fnv").unwrap();
        let mut source_records = Vec::new();
        for signature in signatures {
            for key in esp_authoring_core::plugin_runtime::plugin_handle_record_form_keys_by_signature_no_py(source_handle.id(), signature).unwrap() {
                source_records.push(crate::source_read::read_record(source_handle.id(), &key, &schema, &interner).unwrap());
            }
        }
        let sources = source_records
            .iter()
            .map(|record| LegacyRecordSource {
                record,
                provenance: super::super::super::creature_catalog::CreatureProvenance {
                    game: LegacyCreatureGame::Fnv,
                    source_plugin: "FalloutNV.esm".to_string(),
                    precedence: 0,
                },
            })
            .collect::<Vec<_>>();
        super::super::super::creature_source_idles::attach_source_idle_claims(
            &sources,
            &mut evidence,
            &interner,
        )
        .unwrap();
        let source_catalog = build_creature_corpus_plan(
            &sources,
            CreatureCatalogOptions {
                mesh_root: None,
                expected_creature_winners: None,
            },
            &interner,
        )
        .unwrap();
        let dependency = build_creature_dependency_ledger_with_load_orders(
            &sources,
            CreatureDependencyOptions::default(),
            &[LegacyPluginLoadOrder {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
            }],
            &interner,
        )
        .unwrap();
        let reachable = super::super::super::creature_source_idles::reachable_shared_rig_idles(
            &source_catalog,
            &dependency,
        );
        super::super::super::creature_source_idles::restrict_shared_rig_idle_claims(
            &mut evidence,
            &reachable,
        );
        super::super::super::creature_weapon_attacks::load_weapon_attack_evidence(
            &source_catalog,
            &sources,
            &dependency,
            &mut evidence,
            &interner,
        )
        .unwrap();
        let catalog = build_creature_motion_catalog(&evidence).unwrap();
        let mut ledger: CreatureFamilyRecipeLedger =
            serde_json::from_value(diagnostics["recipe_ledger"].clone()).unwrap();
        let plan: serde_json::Value = serde_json::from_slice(
            &fs::read(mod_root.join("debug/creature_corpus/plan.json")).unwrap(),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let source_root = workspace.join("extracted/fnv");
        let mut rows = Vec::new();
        let mut failures = Vec::new();
        for family in &mut ledger.families {
            let motion = catalog
                .families
                .iter()
                .find(|motion| motion.rig == family.rig)
                .unwrap();
            family.motion_evidence = Some(motion.clone());
            family.indexed_motion_evidence = evidence
                .iter()
                .find(|entry| entry.rig == family.rig)
                .cloned();
            let has = |role| {
                motion
                    .role(role)
                    .is_some_and(|entry| !entry.candidates.is_empty())
            };
            if has(MotionRole::Swim) && has(MotionRole::GroundLocomotion) {
                family.graph_contract.as_mut().unwrap().capability =
                    CreatureGraphCapability::GroundSwim;
            }
            family.required_motion =
                super::super::super::creature_live_builder::required_capability_roles(
                    family.graph_contract.as_ref().unwrap().capability,
                    Some(motion),
                )
                .into_iter()
                .map(|role| RequiredMotionRoleRecipe {
                    role,
                    candidates: motion
                        .role(role)
                        .map(|entry| entry.candidates.clone())
                        .unwrap_or_default(),
                })
                .collect();
            let family_id = canonical_fnv_fo3_family_id(&family.rig);
            let result = (|| -> Result<serde_json::Value, String> {
                let selected = select_family_graph(family)?;
                let job = plan["jobs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|job| job["family_id"] == family_id)
                    .ok_or_else(|| format!("no published recipe for {family_id}"))?;
                let recipe: serde_json::Value = serde_json::from_slice(
                    &fs::read(
                        mod_root.join(job["executable_recipe"]["recipe_path"].as_str().unwrap()),
                    )
                    .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                let mut rig: CreatureManifest = serde_json::from_value(recipe["rig"].clone())
                    .map_err(|error| error.to_string())?;
                let skeleton = crate::phase::skeleton::convert_source_owned_skeleton_artifact(
                    &source_mesh_path(&source_root, &family.rig.skeleton_path),
                    &rig.animation_skeleton.path,
                    &rig.animation_skeleton.runtime_name,
                    &rig.animation_skeleton.float_slots,
                )
                .map_err(|error| error.to_string())?;
                let mut policies = BTreeMap::new();
                rig.clips.clear();
                for clip in &selected.clips {
                    let path = format!(
                        "Actors\\{}\\Animations\\{}.hkx",
                        rig.creature_name, clip.name
                    );
                    let source_bytes =
                        fs::read(source_mesh_path(&source_root, &clip.candidate.source_kf))
                            .map_err(|error| error.to_string())?;
                    let event_map = HashMap::new();
                    let mut request = stage_request_from_motion_candidate(
                        &clip.candidate,
                        &family.rig.skeleton_path,
                        &skeleton,
                        &path,
                        &event_map,
                        None,
                    )
                    .map_err(|error| error.to_string())?;
                    configure_attack_request(family, clip, &mut request)?;
                    let staged =
                        crate::phase::animations::stage_fnv_creature_kf(&source_bytes, request)
                            .map_err(|error| format!("{}: {error}", clip.candidate.source_kf))?;
                    policies.insert(
                        clip.name.clone(),
                        staged_clip_motion_policy(&staged.receipt.motion),
                    );
                    rig.clips.push(ClipDecl {
                        name: clip.name.clone(),
                        path,
                        binding: staged.receipt.binding,
                        looping: clip.target_looping,
                    });
                }
                for overlay in &selected.overlays {
                    let path = format!(
                        "Actors\\{}\\Animations\\{}.hkx",
                        rig.creature_name, overlay.clip_name
                    );
                    let source_bytes =
                        fs::read(source_mesh_path(&source_root, &overlay.candidate.source_kf))
                            .map_err(|error| error.to_string())?;
                    let event_map = HashMap::new();
                    let request = stage_request_from_motion_candidate(
                        &overlay.candidate,
                        &family.rig.skeleton_path,
                        &skeleton,
                        &path,
                        &event_map,
                        None,
                    )
                    .map_err(|error| error.to_string())?;
                    let staged =
                        crate::phase::animations::stage_fnv_creature_kf(&source_bytes, request)
                            .map_err(|error| error.to_string())?;
                    policies.insert(
                        overlay.clip_name.clone(),
                        staged_clip_motion_policy(&staged.receipt.motion),
                    );
                    rig.clips.push(ClipDecl {
                        name: overlay.clip_name.clone(),
                        path,
                        binding: staged.receipt.binding,
                        looping: overlay.candidate.looping,
                    });
                }
                let graph = build_capability_graph(&selected, Some(&policies))?;
                if family
                    .rig
                    .skeleton_path
                    .replace('\\', "/")
                    .eq_ignore_ascii_case("creatures/nvraven/skeleton.nif")
                {
                    let one_shots = graph
                        .roles
                        .iter()
                        .filter(|role| role.role == CreatureClipRole::ScriptedAction)
                        .collect::<Vec<_>>();
                    assert_eq!(
                        one_shots.len(),
                        2,
                        "both source Raven IDLE records must route to graph states"
                    );
                    for local in [0x1424D2, 0x140DBA] {
                        let event = super::super::super::creature_source_idles::source_idle_event(
                            LegacyCreatureGame::Fnv,
                            &StableFormKey {
                                local,
                                plugin: "FalloutNV.esm".to_string(),
                            },
                        );
                        assert!(
                            one_shots
                                .iter()
                                .any(|role| role.trigger_event.as_ref() == Some(&event))
                        );
                    }
                }
                for (motion_role, graph_role, direction) in [
                    (MotionRole::Turn, CreatureClipRole::TurnLeft, "left"),
                    (MotionRole::Turn, CreatureClipRole::TurnRight, "right"),
                    (MotionRole::Hurt, CreatureClipRole::Hurt, ""),
                    (MotionRole::Death, CreatureClipRole::Death, ""),
                ] {
                    let supported_source = motion.role(motion_role).is_some_and(|entry| {
                        entry.candidates.iter().any(|candidate| {
                            candidate.looping == clip_role_expects_looping(graph_role)
                                && candidate
                                    .sequence_name
                                    .to_ascii_lowercase()
                                    .contains(direction)
                                && !leaf(candidate).contains("swim")
                        })
                    });
                    if supported_source && !graph.roles.iter().any(|role| role.role == graph_role) {
                        return Err(format!("source-backed {graph_role:?} action was omitted"));
                    }
                }
                rig.idle_clip = selected
                    .clips
                    .iter()
                    .find(|clip| is_primary_idle(selected.template, clip.role))
                    .unwrap()
                    .name
                    .clone();
                rig.root = graph_declarations(&graph, &rig.ragdoll);
                rig.core = graph_declarations(
                    &graph,
                    &RagdollDisposition::NoRagdoll {
                        reason: NoRagdollReason::SourceHasNoRagdoll,
                    },
                );
                if graph.template == CreatureGraphTemplate::StationaryTurret {
                    add_stationary_aim_declarations(&mut rig.root);
                    add_stationary_aim_declarations(&mut rig.core);
                }
                let packed = crate::source_rig::pack_capability_scaffold(
                    &rig,
                    &graph,
                    temp.path().join(&family_id),
                )
                .map_err(|error| error.to_string())?;
                let actions = required_actor_action_records(&graph, &rig.paths.root_behavior);
                for role in &graph.roles {
                    if matches!(
                        role.role,
                        CreatureClipRole::TurnLeft
                            | CreatureClipRole::TurnRight
                            | CreatureClipRole::Hurt
                            | CreatureClipRole::Death
                    ) {
                        assert!(
                            actions.iter().any(|action| Some(&action.animation_event)
                                == role.trigger_event.as_ref())
                        );
                    }
                }
                Ok(
                    serde_json::json!({"family":family_id,"clips":selected.clips.iter().map(|clip| serde_json::json!({"name":clip.name,"role":clip.role,"source":clip.candidate.source_kf,"locomotion":clip.locomotion_sample})).collect::<Vec<_>>(),"states":graph.roles.len(),"packed_artifacts":packed.artifacts.len(),"actions":actions}),
                )
            })();
            match result {
                Ok(row) => rows.push(row),
                Err(error) => failures.push(format!("{family_id}: {error}")),
            }
        }
        fs::write(
            workspace.join("tmp/fnv_creature_action_audit.json"),
            serde_json::to_vec_pretty(&serde_json::json!({"families":rows,"failures":failures}))
                .unwrap(),
        )
        .unwrap();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        assert_eq!(rows.len(), 63);
    }
}
