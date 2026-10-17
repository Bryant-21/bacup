use super::*;
use crate::source_rig::SourceIdleOverlay;

#[test]
fn partial_source_idles_pack_as_independent_priority_masked_groups() {
    let (mut manifest, mut graph) = fixture();
    manifest.animation_skeleton.bones.extend([
        BoneDecl { name: "Wheel".into(), parent_index: Some(0) },
        BoneDecl { name: "Shoulder".into(), parent_index: Some(0) },
        BoneDecl { name: "Finger".into(), parent_index: Some(2) },
    ]);
    let priorities = |wheel, shoulder| BTreeMap::from([
        ("Root".to_string(), 20), ("Wheel".to_string(), wheel),
        ("Shoulder".to_string(), shoulder), ("Finger".to_string(), shoulder),
    ]);
    let base_priorities = graph.roles.iter().map(|role| (
        role.state_name.clone(),
        if role.role == CreatureClipRole::GroundForward { priorities(22, 22) } else { priorities(20, 20) },
    )).collect::<BTreeMap<_, _>>();
    for event_name in ["IdleStop", "UpperFidget", "UpperFidgetComplete", "UpperLook", "UpperLookComplete", "LeftFinger", "LeftFingerComplete"] {
        let event = EventDecl { name: event_name.into(), usage: EventUsage::Generic, flags: 0 };
        graph.explicit_events.push(event.clone());
        manifest.root.events.push(event.clone());
        manifest.core.events.push(event);
    }
    for (name, group, transform_priorities) in [
        ("UpperFidget", 21, BTreeMap::from([("Wheel".into(), 22), ("Shoulder".into(), 60)])),
        ("UpperLook", 21, BTreeMap::from([("Shoulder".into(), 60)])),
        ("LeftFinger", 4, BTreeMap::from([("Finger".into(), 70)])),
    ] {
        let mut binding = manifest.clips[0].binding.clone();
        binding.declared_transform_tracks = 4;
        binding.transform_track_to_bone_indices = vec![0, 1, 2, 3];
        manifest.clips.push(ClipDecl { name: name.into(), path: format!("Animations\\{name}.hkx"), binding, looping: false });
        graph.roles.push(CapabilityClipRole {
            equipment_index: None, role: CreatureClipRole::ScriptedAction,
            state_name: name.into(), clip_name: name.into(), trigger_event: Some(name.into()),
            trigger_aliases: Vec::new(), motion: Default::default(),
            generator: CapabilityRoleGenerator::SourceIdle {
                source_group: group,
                has_loop_markers: false,
                overlay: Some(SourceIdleOverlay { source_group: group, transform_priorities, base_priorities: base_priorities.clone() }),
                duration_bits: 3.0f32.to_bits(), loop_start_bits: 0.0f32.to_bits(), loop_end_bits: 3.0f32.to_bits(),
                looping_min: 0, looping_max: 0, complete_event: format!("{name}Complete"),
            },
        });
    }
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let xml = &scaffold.artifact(&manifest.paths.core_behavior).unwrap().xml;
    let document = roxmltree::Document::parse(xml).unwrap();
    let object = |id: &str| document.descendants().find(|node| node.attribute("name") == Some(id)).unwrap();
    let base = object("#0190");
    let states = base.children().find(|node| node.attribute("name") == Some("states")).unwrap();
    assert_eq!(states.attribute("numelements"), Some("7"));
    for (group, count) in [(4, "2"), (21, "3")] {
        let machine_id = format!("#source_idle_group{group}");
        let machine = object(&machine_id);
        let states = machine.children().find(|node| node.attribute("name") == Some("states")).unwrap();
        assert_eq!(states.attribute("numelements"), Some(count));
        let layer_id = format!("#source_idle_group{group}_layer");
        let layer = object(&layer_id);
        assert_eq!(layer.children().find(|node| node.attribute("name") == Some("weight")).unwrap().text(), Some("1.0"));
        assert_eq!(layer.children().find(|node| node.attribute("name") == Some("useMotion")).unwrap().text(), Some("false"));
    }
    assert!(xml.contains("boneWeights:1"));
    assert_eq!(object("#source_idle_layers").children().find(|node| node.attribute("name") == Some("flags")).unwrap().text(), Some("0"));
    assert!(xml.contains("iSourceIdleBaseState"));
    assert!(xml.contains("iSourceIdleGroup21"));
    let temp = tempfile::tempdir().unwrap();
    let packed = pack_capability_scaffold(&manifest, &graph, temp.path()).unwrap();
    let bytes = std::fs::read(&packed.artifact(&manifest.paths.core_behavior).unwrap().hkx_path).unwrap();
    let roundtrip = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
    assert!(roundtrip.contains("SourceIdlePriorityWeights"));
    assert!(roundtrip.contains("boneWeights:1"));
    assert!(roundtrip.contains("UpperFidgetComplete"));
}
