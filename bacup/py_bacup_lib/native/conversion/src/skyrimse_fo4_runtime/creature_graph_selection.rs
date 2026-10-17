use super::*;

pub(super) fn horse_locomotion(
    family: &SkyrimFamilyMotionSet,
    inventory: &SkyrimCreatureFamilyInventory,
    clips: &[&SkyrimCatalogClip],
    roles: &mut [CapabilityClipRole],
    events: &mut BTreeMap<String, EventDecl>,
) -> Result<(), SkyrimCreatureMotionError> {
    if family.family_id != "horse" {
        return Ok(());
    }
    let groups = inventory
        .behavior_groups
        .iter()
        .filter_map(|group| match group {
            SkyrimBehaviorGeneratorGroupEvidence::StateMachine(group)
                if group.machine.source.name.as_deref()
                    == Some("ForwardLocomotionAndSprintBehavior") =>
            {
                Some(group)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [group] = groups.as_slice() else {
        return adapter_unavailable(
            family,
            "horse locomotion",
            "missing unique source forward/sprint state machine".into(),
        );
    };
    let root = capability_state_machine_node(family, &group.behavior_path, &group.machine, clips)?;
    let role = roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::GroundForward)
        .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "horse locomotion",
            detail: "no ground forward role".into(),
        })?;
    role.generator = CapabilityRoleGenerator::Tree { root };
    role.clip_name = role.generator.clip_names(&role.clip_name)[0].to_string();
    let mut policies = role
        .generator
        .clip_names(&role.clip_name)
        .into_iter()
        .map(|name| {
            clips
                .iter()
                .find(|clip| clip.clip_id == name)
                .expect("lowered source clip exists")
                .root_motion
                .policy()
        });
    let motion = policies.next().expect("source locomotion tree has clips");
    if policies.any(|policy| policy.animation_driven != motion.animation_driven) {
        return adapter_unavailable(
            family,
            "horse locomotion",
            "source locomotion tree mixes incompatible root-motion policies".into(),
        );
    }
    role.motion = motion;
    role.trigger_event = Some("moveStart".into());
    role.trigger_aliases.clear();
    events.insert(
        "movestart".into(),
        EventDecl {
            name: "moveStart".into(),
            usage: EventUsage::Generic,
            flags: 0,
        },
    );
    collect_events(&role.generator, events);
    Ok(())
}

pub(super) fn collect_events(
    generator: &CapabilityRoleGenerator,
    events: &mut BTreeMap<String, EventDecl>,
) {
    let value = serde_json::to_value(generator).expect("generator serializes");
    fn visit(value: &serde_json::Value, events: &mut BTreeMap<String, EventDecl>) {
        match value {
            serde_json::Value::Object(fields) => {
                if let Some(name) = fields.get("event_name").and_then(serde_json::Value::as_str) {
                    events
                        .entry(name.to_ascii_lowercase())
                        .or_insert(EventDecl {
                            name: name.to_string(),
                            usage: EventUsage::Generic,
                            flags: 0,
                        });
                }
                for value in fields.values() {
                    visit(value, events);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, events);
                }
            }
            _ => {}
        }
    }
    visit(&value, events);
}

pub(super) fn horse_synchronization(
    family: &SkyrimFamilyMotionSet,
    inventory: &SkyrimCreatureFamilyInventory,
    clips: &[&SkyrimCatalogClip],
    roles: &mut [CapabilityClipRole],
    variables: &mut Vec<VariableDecl>,
) -> Result<(), SkyrimCreatureMotionError> {
    if family.family_id != "horse" {
        return Ok(());
    }
    let mut modifiers = Vec::new();
    for group in &inventory.behavior_groups {
        if let SkyrimBehaviorGeneratorGroupEvidence::StateMachine(group) = group {
            for state in &group.machine.states {
                if let Some(node) = &state.generator {
                    collect_modifiers(node, &group.behavior_path, &mut modifiers);
                }
            }
        }
    }
    let (path, sampler) = modifiers.iter().find(|(_, modifier)| matches!(modifier,
        SkyrimBehaviorModifierNodeEvidence::Unsupported { source, .. } if source.class_name == "BSSpeedSamplerModifier"))
        .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(), target: "horse synchronization", detail: "source speed sampler absent".into(),
        })?;
    let SkyrimBehaviorModifierNodeEvidence::Unsupported {
        source,
        variable_bindings,
        ..
    } = sampler
    else {
        unreachable!()
    };
    let source = capability_generator_source(family, path, source)?;
    let bindings = capability_generator_bindings(family, variable_bindings)?;
    let (path, damping) = modifiers.iter().find(|(_, modifier)| matches!(modifier,
        SkyrimBehaviorModifierNodeEvidence::Damping { source, .. } if source.name.as_deref() == Some("TurnDeltaDampingModifier")))
        .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(), target: "horse synchronization", detail: "source turn damping absent".into(),
        })?;
    let damping = capability_modifier_node(family, path, damping)?;
    for binding in &bindings {
        if !variables.iter().any(|v| v.name == binding.variable_name) {
            let evidence = inventory
                .behavior_variables
                .iter()
                .find(|v| {
                    v.variable_name.as_deref() == Some(&binding.variable_name)
                        && path_key(&v.behavior_path) == path_key(&source.behavior_path)
                })
                .ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
                    family_id: family.family_id.clone(),
                    target: "horse synchronization",
                    detail: format!("missing sampler variable {}", binding.variable_name),
                })?;
            variables.push(capability_variable_declaration(family, evidence)?);
        }
    }
    variables.push(VariableDecl {
        name: "SyncLocomotionState".into(),
        variable_type: VariableType::Int32,
        initial_value: VariableValue::Int32(0),
    });
    variables.push(VariableDecl {
        name: "B21MountRequiresDismount".into(),
        variable_type: VariableType::Bool,
        initial_value: VariableValue::Bool(false),
    });
    for (index, role) in roles.iter_mut().enumerate() {
        let source_rider_has_counterparts =
            role.generator
                .clip_names(&role.clip_name)
                .iter()
                .all(|name| {
                    let Some(clip) = clips.iter().find(|clip| clip.clip_id.as_str() == *name)
                    else {
                        return false;
                    };
                    let source_leaf = clip.clip_path.rsplit(['/', '\\']).next().unwrap();
                    inventory.mounted_rider_clip_paths.iter().any(|path| {
                        path.rsplit(['/', '\\'])
                            .next()
                            .is_some_and(|leaf| leaf.eq_ignore_ascii_case(source_leaf))
                    })
                });
        role.generator = CapabilityRoleGenerator::MountSynchronization {
            state_index: if source_rider_has_counterparts {
                index as i32 * 2
            } else {
                -1
            },
            generator: Box::new(role.generator.clone()),
            speed_sampler_source: source.clone(),
            speed_sampler_bindings: bindings.clone(),
            turn_damping: damping.clone(),
        };
    }
    Ok(())
}

fn collect_modifiers<'a>(
    node: &'a SkyrimBehaviorGeneratorNodeEvidence,
    path: &'a str,
    output: &mut Vec<(&'a str, &'a SkyrimBehaviorModifierNodeEvidence)>,
) {
    match node {
        SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
            modifier,
            generator,
            ..
        } => {
            fn add<'a>(
                modifier: &'a SkyrimBehaviorModifierNodeEvidence,
                path: &'a str,
                output: &mut Vec<(&'a str, &'a SkyrimBehaviorModifierNodeEvidence)>,
            ) {
                output.push((path, modifier));
                if let SkyrimBehaviorModifierNodeEvidence::ModifierList { modifiers, .. } = modifier
                {
                    for modifier in modifiers {
                        add(modifier, path, output);
                    }
                }
            }
            if let Some(modifier) = modifier {
                add(modifier, path, output);
            }
            if let Some(generator) = generator {
                collect_modifiers(generator, path, output);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) => {
            for state in &machine.states {
                if let Some(generator) = &state.generator {
                    collect_modifiers(generator, path, output);
                }
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::CyclicBlendTransition(node) => {
            if let Some(generator) = &node.generator {
                collect_modifiers(generator, path, output);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::Blender { children, .. } => {
            for child in children {
                collect_modifiers(&child.generator, path, output);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::ManualSelector { children, .. } => {
            for child in children {
                collect_modifiers(child, path, output);
            }
        }
        SkyrimBehaviorGeneratorNodeEvidence::Unsupported { child_members, .. } => {
            for child in child_members.iter().flat_map(|member| &member.children) {
                collect_modifiers(child, path, output);
            }
        }
        _ => {}
    }
}

pub(super) fn horse_interactions(
    family: &SkyrimFamilyMotionSet,
    clips: &[&SkyrimCatalogClip],
    roles: &mut Vec<CapabilityClipRole>,
    events: &mut BTreeMap<String, EventDecl>,
) -> Result<(), SkyrimCreatureMotionError> {
    if family.family_id != "horse" {
        return Ok(());
    }
    for (leaf, state_name, trigger, aliases) in [
        (
            "paired_mount.hkx",
            "HorseMountEnter",
            "sitStartFromStand",
            vec!["pa_HorseEnter", "pa_HorseEnterSwim"],
        ),
        (
            "paired_dismount.hkx",
            "HorseMountExit",
            "standStart",
            vec!["QuickExitToStand", "pa_HorseExit", "pa_HorseExitSwim"],
        ),
    ] {
        let matching = clips
            .iter()
            .filter(|clip| {
                clip.original_skeleton_name.as_deref() == Some("PairedRoot")
                    && clip
                        .clip_path
                        .rsplit(['/', '\\'])
                        .next()
                        .is_some_and(|name| name.eq_ignore_ascii_case(leaf))
            })
            .collect::<Vec<_>>();
        let [clip] = matching.as_slice() else {
            return adapter_unavailable(
                family,
                "horse interaction",
                format!("source paired clip {leaf} has {} bindings", matching.len()),
            );
        };
        for event in std::iter::once(trigger).chain(aliases.iter().copied()) {
            events.insert(
                event.to_ascii_lowercase(),
                EventDecl {
                    name: event.into(),
                    usage: EventUsage::Generic,
                    flags: 0,
                },
            );
        }
        roles.push(CapabilityClipRole {
            equipment_index: None,
            role: CreatureClipRole::ScriptedAction,
            state_name: state_name.into(),
            clip_name: clip.clip_id.clone(),
            generator: CapabilityRoleGenerator::MountInteraction,
            trigger_event: Some(trigger.into()),
            trigger_aliases: aliases.into_iter().map(str::to_string).collect(),
            motion: ClipMotionPolicy::default(),
        });
    }
    Ok(())
}

pub(super) fn horse_rider_generators(
    catalog: &SkyrimCreatureMotionCatalog,
    graph: &CapabilityGraphManifest,
) -> Result<Vec<(i32, CapabilityGeneratorNode)>, SkyrimCreatureMotionError> {
    let family = catalog
        .family("horse")
        .expect("horse graph requires its source family");
    let inventory = catalog
        .inventory("horse")
        .expect("horse graph requires its source inventory");
    let clips = catalog
        .clips
        .iter()
        .filter(|clip| clip.family_ids.iter().any(|family| family == "horse"))
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for role in &graph.roles {
        let CapabilityRoleGenerator::MountSynchronization {
            state_index,
            generator,
            ..
        } = &role.generator
        else {
            continue;
        };
        if *state_index < 0 {
            continue;
        }
        if let CapabilityRoleGenerator::Tree {
            root: CapabilityGeneratorNode::StateMachine { states, .. },
        } = generator.as_ref()
            && role.role == CreatureClipRole::GroundForward
        {
            for state in states {
                result.push((*state_index + state.state_id, (*state.generator).clone()));
            }
            continue;
        }
        let clip = clips
            .iter()
            .find(|clip| clip.clip_id == role.clip_name)
            .expect("graph clip is in the source catalog");
        let mut found = None;
        for group in &inventory.behavior_groups {
            let (path, nodes) = match group {
                SkyrimBehaviorGeneratorGroupEvidence::StateMachine(group) => (
                    &group.behavior_path,
                    group
                        .machine
                        .states
                        .iter()
                        .filter_map(|state| state.generator.as_ref())
                        .collect::<Vec<_>>(),
                ),
                SkyrimBehaviorGeneratorGroupEvidence::Blender(group) => (
                    &group.behavior_path,
                    group
                        .children
                        .iter()
                        .map(|child| &child.generator_tree)
                        .collect(),
                ),
                SkyrimBehaviorGeneratorGroupEvidence::ManualSelector(group) => (
                    &group.behavior_path,
                    group
                        .children
                        .iter()
                        .map(|child| &child.generator_tree)
                        .collect(),
                ),
            };
            if let Some(node) = nodes
                .into_iter()
                .find_map(|node| find_clip(node, &clip.clip_path))
            {
                found = Some(capability_generator_node(family, path, node, &clips)?);
                break;
            }
        }
        let node = found.ok_or_else(|| SkyrimCreatureMotionError::AdapterUnavailable {
            family_id: family.family_id.clone(),
            target: "horse rider",
            detail: format!("no source clip generator for {}", clip.clip_path),
        })?;
        result.push((*state_index, node));
    }
    result.sort_by_key(|(id, _)| *id);
    Ok(result)
}

fn find_clip<'a>(
    node: &'a SkyrimBehaviorGeneratorNodeEvidence,
    path: &str,
) -> Option<&'a SkyrimBehaviorGeneratorNodeEvidence> {
    match node {
        SkyrimBehaviorGeneratorNodeEvidence::Clip {
            resolved_clip_path: Some(source),
            ..
        } if path_key(source) == path_key(path) => Some(node),
        SkyrimBehaviorGeneratorNodeEvidence::Blender { children, .. } => children
            .iter()
            .find_map(|child| find_clip(&child.generator, path)),
        SkyrimBehaviorGeneratorNodeEvidence::ManualSelector { children, .. } => {
            children.iter().find_map(|child| find_clip(child, path))
        }
        SkyrimBehaviorGeneratorNodeEvidence::StateMachine(machine) => machine
            .states
            .iter()
            .filter_map(|state| state.generator.as_ref())
            .find_map(|child| find_clip(child, path)),
        SkyrimBehaviorGeneratorNodeEvidence::ModifierGenerator {
            generator: Some(child),
            ..
        } => find_clip(child, path),
        SkyrimBehaviorGeneratorNodeEvidence::CyclicBlendTransition(node) => node
            .generator
            .as_ref()
            .and_then(|child| find_clip(child, path)),
        SkyrimBehaviorGeneratorNodeEvidence::Unsupported { child_members, .. } => child_members
            .iter()
            .flat_map(|member| &member.children)
            .find_map(|child| find_clip(child, path)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_rig::{ClipDecl, CreatureManifest, pack_capability_scaffold};

    #[test]
    fn injury_entry_is_not_a_default_idle() {
        assert_eq!(role_from_text("GetUpFromRagdoll"), None);
        assert_eq!(role_from_text("ReanimateL"), None);
        assert_eq!(
            role_from_text("CLIP_Ground_BleedoutEnter"),
            Some(SkyrimCreatureMotionRole::Hurt)
        );
        assert_eq!(
            role_from_text("ST_Ground_Idle_Injure"),
            Some(SkyrimCreatureMotionRole::Hurt)
        );
        assert_eq!(
            candidate_attack_role(
                &SkyrimRaceAttackEvidence {
                    event: "attackStartTail".into(),
                    has_attack_spell: false,
                    attack_spell_delivery: None,
                },
                &[]
            ),
            CreatureClipRole::MeleeAttack
        );
        assert!(!transition_event_matches_node(
            "attackStartTailLeft",
            "CLIP_Ground_Combat_Attack_Tail_Right"
        ));
        assert!(transition_event_matches_node(
            "attackStartTailLeft",
            "CLIP_Ground_Combat_Attack_Tail_Left"
        ));
    }

    #[test]
    #[ignore = "requires extracted Skyrim assets and CLI source race audit fixture"]
    fn source_skyrim_all_race_attack_graph_selection() {
        let root = PathBuf::from(std::env::var("MODKIT_SKYRIM_MESH_ROOT").unwrap());
        let actors = root.join("actors").canonicalize().unwrap();
        let input: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("MODKIT_SKYRIM_GRAPH_INPUTS").unwrap()).unwrap(),
        )
        .unwrap();
        let groups = input.as_array().unwrap();
        assert_eq!(groups.len(), RACE_PROJECTS.len());
        let data = load_animation_data_catalog(&root.join("animationdata")).unwrap();
        let interner = crate::sym::StringInterner::new();
        let mut failures = Vec::new();
        let mut reports = Vec::new();
        let mut race_count = 0;
        let mut attack_count = 0;
        for group in groups {
            let family_id = group["family_id"].as_str().unwrap();
            let project = group["project_relative"].as_str().unwrap();
            let mut attacks = Vec::new();
            for race in group["races"].as_array().unwrap() {
                race_count += 1;
                let plugin = race["source_plugin"].as_str().unwrap();
                let local = race["source_local"].as_u64().unwrap() as u32;
                for attack in race["attacks"].as_array().unwrap() {
                    attack_count += 1;
                    attacks.push(SkyrimRaceAttackSourceEvidence {
                        source_race: FormKey {
                            local,
                            plugin: interner.intern(plugin),
                        },
                        source_plugin: plugin.into(),
                        attack: SkyrimRaceAttackEvidence {
                            event: attack["event"].as_str().unwrap().into(),
                            has_attack_spell: attack["has_attack_spell"].as_bool().unwrap(),
                            attack_spell_delivery: match attack["attack_spell_delivery"].as_str() {
                                None => None,
                                Some("Self") => Some(0),
                                Some("Touch") => Some(1),
                                Some("Aimed") => Some(2),
                                Some("TargetActor") => Some(3),
                                Some("TargetLocation") => Some(4),
                                value => panic!("unknown delivery {value:?}"),
                            },
                        },
                    });
                }
            }
            let stem = Path::new(project)
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_ascii_lowercase();
            let empty = ProjectAnimationData::default();
            let family = match load_family_evidence(
                &actors,
                data.get(&stem).unwrap_or(&empty),
                family_id,
                project,
                attacks,
            ) {
                Ok(family) => family,
                Err(error) => {
                    let detail = format!("{family_id}: {error}");
                    println!("{detail}");
                    failures.push(detail.clone());
                    reports.push(
                        serde_json::json!({"family": family_id, "source_closure_error": detail}),
                    );
                    continue;
                }
            };
            let recovery = serde_json::json!({
                "graphs": family.inventory.recovery_graphs.iter().filter_map(|result| result.as_ref().ok()).map(|graph| serde_json::json!({
                    "behavior_path": graph.behavior_path,
                    "node_count": graph.nodes.len(),
                    "clips": graph.clip_names(),
                })).collect::<Vec<_>>(),
                "errors": family.inventory.recovery_graphs.iter().filter_map(|result| result.as_ref().err()).collect::<Vec<_>>(),
            });
            let catalog = build_skyrim_creature_motion_catalog(&[family]).unwrap();
            assert!(
                !catalog.clips.is_empty(),
                "source project closure has no clips: {family_id}"
            );
            match catalog.capability_graph_bundle(family_id) {
                Ok((graph, _)) => {
                    if family_id == "atronach_flame" {
                        let role = graph.roles.iter().find(|role| role.trigger_event.as_deref() == Some("Spell_FireForget_LH")).expect("source Flame fire-and-forget entry");
                        let CapabilityRoleGenerator::SourceCasting { observations, .. } = &role.generator else { panic!("Flame source casting tree must survive selection"); };
                        assert_eq!(observations.len(),2,"source random hands require distinct branch observations");
                        let observed = observations.iter().map(|observation| catalog.clip(&observation.release_clip_name).unwrap().clip_path.to_ascii_lowercase()).collect::<BTreeSet<_>>();
                        assert!(observed.iter().any(|path|path.ends_with("mag_ff_rh_out.hkx")));
                        assert!(observed.iter().any(|path|path.ends_with("mag_ff_lh_out.hkx")));
                    }
                    if matches!(family_id,"atronach_flame" | "hagraven") {
                        let role=graph.roles.iter().find(|role|role.trigger_event.as_deref()==Some("Spell_Concentration_LH")).expect("source concentration entry");
                        assert!(source_clip_entry_events(&role.generator,&role.clip_name).iter().any(|event|event=="MLh_SpellFire_Event"),"concentration release is source state notification");
                    }
                    println!(
                        "source graph {family_id}: {} roles, {} executable candidate attacks, {} source-inert attack receipts",
                        graph.roles.len(),
                        graph.candidate_attack_bindings.len(),
                        graph.source_inert_attack_receipts.len()
                    );
                    reports.push(serde_json::json!({"family": family_id, "graph": graph, "recovery": recovery}));
                }
                Err(error) => {
                    let detail = format!("{family_id}: {error}");
                    println!("{detail}");
                    failures.push(detail);
                    reports.push(serde_json::json!({"family": family_id, "recovery": recovery, "clips": catalog.clips.iter().map(|clip| serde_json::json!({"path":clip.clip_path,"disposition":format!("{:?}",clip.disposition)})).collect::<Vec<_>>()}));
                }
            }
        }
        assert_eq!(race_count, 121);
        assert_eq!(attack_count, 998);
        std::fs::write(
            root.parent()
                .unwrap()
                .join("skyrim_source_graph_audit.json"),
            serde_json::to_vec_pretty(&reports).unwrap(),
        )
        .unwrap();
        assert!(
            failures.is_empty(),
            "{} source families failed: {}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    #[ignore = "requires the extracted Skyrim horse graph and prior source-rig manifest"]
    fn source_skyrim_horse_graph_selection_roundtrip() {
        let root = PathBuf::from(std::env::var("MODKIT_SKYRIM_MESH_ROOT").unwrap());
        let actors = root.join("actors").canonicalize().unwrap();
        let data = load_animation_data_catalog(&root.join("animationdata")).unwrap();
        let family = load_family_evidence(
            &actors,
            &data["horseproject"],
            "horse",
            "horse/horseproject.hkx",
            Vec::new(),
        )
        .unwrap();
        let catalog = build_skyrim_creature_motion_catalog(&[family]).unwrap();
        assert!(
            !catalog.clips.is_empty(),
            "source horse closure has no clips"
        );
        let (graph, variables) = catalog.capability_graph_bundle("horse").unwrap();
        let action_records =
            crate::source_rig::required_actor_action_records(&graph, "HorseRoot.hkx");
        for (event, parent) in [("SprintStart", 0x03B4A7), ("SprintStop", 0x013219)] {
            assert!(
                action_records.iter().any(
                    |record| record.animation_event == event && record.parent_form_id == parent
                )
            );
        }
        let source = serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(std::env::var("MODKIT_SKYRIM_HORSE_RECIPE").unwrap()).unwrap(),
        )
        .unwrap();
        let mut rig: CreatureManifest = serde_json::from_value(source["rig"].clone()).unwrap();
        let original = rig.clips[0].clone();
        rig.clips.clear();
        let mut seen = BTreeSet::new();
        for role in &graph.roles {
            for name in role.generator.clip_names(&role.clip_name) {
                if seen.insert(name.to_string()) {
                    let clip = catalog.clip(name).unwrap();
                    println!("horse {:?} {}", role.role, clip.clip_path);
                    rig.clips.push(ClipDecl {
                        name: name.to_string(),
                        path: original
                            .path
                            .rsplit_once('\\')
                            .map(|(folder, _)| format!("{folder}\\{name}.hkx"))
                            .unwrap(),
                        binding: original.binding.clone(),
                        looping: matches!(
                            role.role,
                            CreatureClipRole::Idle
                                | CreatureClipRole::GroundForward
                                | CreatureClipRole::SwimIdle
                                | CreatureClipRole::SwimForward
                                | CreatureClipRole::TurnLeft
                                | CreatureClipRole::TurnRight
                        ),
                    });
                    if clip.original_skeleton_name.as_deref() == Some("PairedRoot") {
                        let skeleton =
                            &catalog.inventory("horse").unwrap().animation_skeleton_paths[0];
                        let declaration = rig.clips.last_mut().unwrap();
                        let (_, converted) =
                            crate::phase::animations::skyrim_rider::stage_paired_horse_clip(
                                &std::fs::read(root.join(clip.clip_path.replace('\\', "/")))
                                    .unwrap(),
                                &std::fs::read(root.join(skeleton.replace('\\', "/"))).unwrap(),
                                &declaration.binding.skeleton_path,
                                &declaration.path,
                                &declaration.name,
                            )
                            .unwrap();
                        *declaration = converted;
                    }
                }
            }
        }
        rig.idle_clip = graph
            .roles
            .iter()
            .find(|role| role.role == CreatureClipRole::Idle)
            .unwrap()
            .clip_name
            .clone();
        for declarations in [&mut rig.root, &mut rig.core] {
            declarations.variables = variables.clone();
            for event in &graph.explicit_events {
                declarations
                    .events
                    .retain(|old| !old.name.eq_ignore_ascii_case(&event.name));
                declarations.events.push(event.clone());
            }
        }
        let stages = catalog.horse_rider_generators(&graph).unwrap();
        assert!(
            stages
                .iter()
                .any(|(_, node)| matches!(node, CapabilityGeneratorNode::StateMachine { .. }))
        );
        let rider = crate::phase::animations::skyrim_rider::stage_horse_rider(
            &root,
            &PathBuf::from(std::env::var("MODKIT_REPO_ROOT").unwrap()).join("extracted/fo4/meshes"),
            &catalog,
            &rig,
            &graph,
        )
        .unwrap();
        assert!(
            rider
                .assets
                .iter()
                .any(|(_, bytes)| HkxFile::read(bytes).is_ok_and(|file| {
                    file.objects()
                        .iter()
                        .any(|object| object.class_name == "hkbBehaviorGraph")
                }))
        );
        let output = std::env::temp_dir().join(format!("b21-horse-graph-{}", std::process::id()));
        let report = pack_capability_scaffold(&rig, &graph, &output).unwrap();
        for artifact in report.artifacts {
            let bytes = std::fs::read(&artifact.hkx_path).unwrap();
            let file = HkxFile::read(&bytes).unwrap();
            assert_eq!(file.save(), bytes);
            if artifact.runtime_path == rig.paths.core_behavior {
                let xml = std::fs::read_to_string(&artifact.xml_path).unwrap();
                let xml = roxmltree::Document::parse(&xml).unwrap();
                let sync_index = rig
                    .core
                    .variables
                    .iter()
                    .position(|variable| variable.name == "SyncLocomotionState")
                    .unwrap()
                    .to_string();
                for expression in xml.descendants().filter(|node| {
                    node.has_tag_name("hkobject")
                        && node.children().any(|child| {
                            child.attribute("name") == Some("assignmentVariableIndex")
                                && child.text() == Some(sync_index.as_str())
                        })
                }) {
                    let value = expression
                        .children()
                        .find(|child| child.attribute("name") == Some("expression"))
                        .and_then(|child| child.text())
                        .unwrap();
                    assert_ne!(
                        value, "-1",
                        "unsupported horse states must dismount without changing rider selector"
                    );
                }
                assert_eq!(
                    xml.descendants()
                        .filter(|node| node.attribute("name") == Some("name")
                            && node.text() == Some("MountInteractionCompatible"))
                        .count(),
                    2,
                    "both paired mount actions must clear the dismount flag"
                );
                assert!(
                    file.objects()
                        .iter()
                        .any(|object| object.class_name == "BSSpeedSamplerModifier")
                );
                let expressions = file
                    .objects()
                    .iter()
                    .filter(|object| object.class_name == "hkbExpressionDataArray")
                    .count();
                assert!(
                    expressions
                        >= graph
                            .roles
                            .iter()
                            .filter(|role| matches!(
                                role.generator,
                                CapabilityRoleGenerator::MountSynchronization { .. }
                            ))
                            .count()
                );
            }
        }
    }
}
