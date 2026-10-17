use super::*;
use havok_native::hkx::{HkxFile, types::HkxValue};

fn lower_test_clips(
    value: &mut SourceRecoveryValue,
    clips: &std::collections::BTreeMap<String, String>,
) {
    match value {
        SourceRecoveryValue::Clip(name) => *name = clips[name].clone(),
        SourceRecoveryValue::Array(values) => values
            .iter_mut()
            .for_each(|value| lower_test_clips(value, clips)),
        SourceRecoveryValue::Object(values) => values
            .values_mut()
            .for_each(|value| lower_test_clips(value, clips)),
        _ => {}
    }
}

#[test]
#[ignore = "requires extracted Skyrim Hagraven behavior and skeleton"]
fn source_hagraven_recovery_preserves_pose_matching_and_lifecycle_after_pack() {
    let root =
        std::path::PathBuf::from(std::env::var("MODKIT_REPO_ROOT").expect("MODKIT_REPO_ROOT"));
    let actors = root.join("extracted/skyrimse/meshes/actors");
    let family_root = actors.join("hagraven");
    let path = family_root.join("behaviors/havgravenbehavior.hkx");
    let source = HkxFile::read(&fs::read(&path).unwrap()).unwrap();
    let mut recovery = crate::skyrimse_fo4_runtime::creature_motion::extract_source_recovery(
        &source,
        &actors,
        &family_root,
        &path,
    )
    .expect("source recovery state machine")
    .unwrap();
    assert_eq!(
        recovery
            .nodes
            .values()
            .filter(|node| node.class_name == "hkbPoseMatchingGenerator")
            .count(),
        2
    );
    let (mut manifest, mut graph) = passive_ground_capability();
    manifest = with_source_owned_ragdoll(manifest);
    let skeleton =
        HkxFile::read(&fs::read(family_root.join("character assets/skeleton.hkx")).unwrap())
            .unwrap();
    let source_skeleton = skeleton
        .objects()
        .iter()
        .filter(|object| object.class_name == "hkaSkeleton")
        .max_by_key(|object| {
            match &object
                .members
                .iter()
                .find(|member| member.name == "bones")
                .unwrap()
                .value
            {
                HkxValue::Array(values) => values.len(),
                _ => 0,
            }
        })
        .unwrap();
    let field = |object: &havok_native::hkx::model::HkxObject, name: &str| {
        object
            .members
            .iter()
            .find(|member| member.name == name)
            .unwrap()
            .value
            .clone()
    };
    let HkxValue::Array(bones) = field(source_skeleton, "bones") else {
        panic!("bones")
    };
    let HkxValue::Array(parents) = field(source_skeleton, "parentIndices") else {
        panic!("parents")
    };
    manifest.animation_skeleton.bones = bones
        .iter()
        .zip(&parents)
        .map(|(bone, parent)| {
            let members = bone.as_object_members().unwrap();
            let HkxValue::String { value: name, .. } = &members
                .iter()
                .find(|member| member.name == "name")
                .unwrap()
                .value
            else {
                panic!("bone name")
            };
            let HkxValue::I16(parent) = parent else {
                panic!("bone parent")
            };
            BoneDecl {
                name: name.clone(),
                parent_index: (*parent >= 0).then_some(*parent as usize),
            }
        })
        .collect();
    for clip in &mut manifest.clips {
        clip.binding.declared_transform_tracks = bones.len();
        clip.binding.transform_track_to_bone_indices = (0..bones.len()).collect();
    }
    let clip_names = recovery
        .clip_names()
        .into_iter()
        .enumerate()
        .map(|(index, path)| (path.to_string(), format!("recovery_{index}")))
        .collect::<std::collections::BTreeMap<_, _>>();
    for name in clip_names.values() {
        let mut clip = manifest.clips[0].clone();
        clip.name = name.clone();
        clip.path = format!("Actors\\B21_SkyrimWolf\\Animations\\{name}.hkx");
        clip.looping = false;
        manifest.clips.push(clip);
    }
    for node in recovery.nodes.values_mut() {
        for value in node.members.values_mut() {
            lower_test_clips(value, &clip_names);
        }
    }
    graph.explicit_events.push(generic_event("GetUpBegin"));
    let mut role = capability_role(
        CreatureClipRole::GetUp,
        "SourceRecovery",
        recovery.clip_names()[0],
        Some("GetUpBegin"),
    );
    role.generator = CapabilityRoleGenerator::SourceRecovery {
        graph: recovery.clone(),
    };
    graph.roles.push(role);
    let actions = required_actor_action_records(&graph, &manifest.paths.root_behavior);
    assert!(
        actions
            .iter()
            .any(|action| action.kind == ActorActionKind::GetUp
                && action.parent_form_id == 0x0d1fdd
                && action.animation_event == "GetUpBegin")
    );
    let directory = tempfile::tempdir().unwrap();
    let packed = pack_capability_scaffold(&manifest, &graph, directory.path()).unwrap();
    let root_asset = packed
        .artifacts
        .iter()
        .find(|asset| asset.runtime_path == manifest.paths.root_behavior)
        .unwrap();
    let target = HkxFile::read(&fs::read(&root_asset.hkx_path).unwrap()).unwrap();
    let target_events = target
        .objects()
        .iter()
        .find(|object| object.class_name == "hkbBehaviorGraphStringData")
        .unwrap();
    let HkxValue::Array(event_names) = field(target_events, "eventNames") else {
        panic!("events")
    };
    let events = event_names
        .iter()
        .map(|value| {
            if let HkxValue::String { value, .. } = value {
                value.clone()
            } else {
                panic!("event")
            }
        })
        .collect::<Vec<_>>();
    let pose = target
        .objects()
        .iter()
        .filter(|node| node.class_name == "hkbPoseMatchingGenerator")
        .collect::<Vec<_>>();
    assert_eq!(pose.len(), 2);
    for pose in pose {
        assert_eq!(
            field(pose, "worldFromModelRotation"),
            HkxValue::F32List(vec![0.0, 0.0, 0.0, 1.0])
        );
        assert_eq!(field(pose, "blendSpeed"), HkxValue::F32(1.0));
        let HkxValue::I32(event) = field(pose, "startPlayingEventId") else {
            panic!("pose event")
        };
        assert_eq!(events[event as usize], "GetUpStart");
    }
    let getup = target
        .objects()
        .iter()
        .find(|node| node.class_name == "hkbGetUpModifier")
        .unwrap();
    assert_eq!(
        field(getup, "groundNormal"),
        HkxValue::F32List(vec![0.0, 0.0, 1.0, 0.0])
    );
    assert_eq!(field(getup, "duration"), HkxValue::F32(1.0));
    assert_eq!(field(getup, "alignWithGroundDuration"), HkxValue::F32(0.25));
    let named = |name: &str| {
        target
            .objects()
            .iter()
            .find(|object| {
                object.members.iter().any(|member| {
                    member.name == "name"
                        && matches!(&member.value, HkxValue::String { value, .. } if value == name)
                })
            })
            .unwrap()
    };
    let timer = named("GetUpTimerMod");
    assert_eq!(field(timer, "enable"), HkxValue::Bool(true));
    assert_eq!(field(timer, "alarmTimeSeconds"), HkxValue::F32(0.5));
    let alarm = field(timer, "alarmEvent");
    let HkxValue::I32(alarm_event) = alarm
        .as_object_members()
        .unwrap()
        .iter()
        .find(|member| member.name == "id")
        .unwrap()
        .value
    else {
        panic!("alarm event")
    };
    assert_eq!(events[alarm_event as usize], "GetUpStart");
    let selector = named("PoseMatching MSG");
    let HkxValue::Pointer(Some(binding)) = field(selector, "variableBindingSet") else {
        panic!("recovery selector binding")
    };
    let HkxValue::Array(bindings) = field(&target.objects()[binding], "bindings") else {
        panic!("recovery selector bindings")
    };
    let selected_index = bindings.iter().find_map(|binding| {
        let members = binding.as_object_members()?;
        let path = members.iter().find(|member| member.name == "memberPath")?;
        if !matches!(&path.value, HkxValue::String { value, .. } if value == "selectedGeneratorIndex") {
            return None;
        }
        match members.iter().find(|member| member.name == "variableIndex")?.value {
            HkxValue::I32(index) => Some(index as usize),
            _ => None,
        }
    }).unwrap();
    let HkxValue::Array(variable_names) = field(target_events, "variableNames") else {
        panic!("variables")
    };
    assert!(
        matches!(&variable_names[selected_index], HkxValue::String { value, .. } if value == "iGetUpType")
    );
    let completion = named("GetUpFromRagdoll");
    let HkxValue::Pointer(Some(transitions)) = field(completion, "transitions") else {
        panic!("GetUp transitions")
    };
    let HkxValue::Array(transitions) = field(&target.objects()[transitions], "transitions") else {
        panic!("GetUp transition array")
    };
    assert!(transitions.iter().any(|transition| {
        let fields = transition.as_object_members().unwrap();
        let event = &fields
            .iter()
            .find(|field| field.name == "eventId")
            .unwrap()
            .value;
        let state = &fields
            .iter()
            .find(|field| field.name == "toStateId")
            .unwrap()
            .value;
        matches!(event, HkxValue::I32(index) if events[*index as usize] == "GetUpEnd")
            && *state == HkxValue::I32(24)
    }));
    let core_asset = packed
        .artifacts
        .iter()
        .find(|asset| asset.runtime_path == manifest.paths.core_behavior)
        .unwrap();
    let core = HkxFile::read(&fs::read(&core_asset.hkx_path).unwrap()).unwrap();
    assert!(
        !core
            .objects()
            .iter()
            .any(|node| node.class_name == "hkbGetUpModifier")
    );
    let mut invalid = recovery.clone();
    invalid
        .nodes
        .values_mut()
        .find(|node| node.class_name == "hkbGetUpModifier")
        .unwrap()
        .members
        .insert(
            "duration".into(),
            SourceRecoveryValue::String("unsafe replacement".into()),
        );
    let mut errors = ValidationErrors::default();
    invalid.validate(&manifest, &mut errors);
    assert!(
        errors
            .0
            .iter()
            .any(|error| error.code == "source_recovery_member_type")
    );
    let mut invalid = recovery;
    invalid
        .nodes
        .get_mut(&invalid.root)
        .unwrap()
        .members
        .insert(
            "wildcardTransitions".into(),
            SourceRecoveryValue::Reference(None),
        );
    let mut errors = ValidationErrors::default();
    invalid.validate(&manifest, &mut errors);
    assert!(
        errors
            .0
            .iter()
            .any(|error| error.code == "source_recovery_lifecycle")
    );
}
