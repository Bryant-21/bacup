use super::*;
use crate::source_rig::CreaturePropertySequence;

#[test]
fn property_sequence_graph_pairs_preserve_source_idle_crops_and_completion() {
    let (mut manifest, mut graph) = fixture();
    for name in ["IdleStop", "ScreenFidget", "ScreenFidgetComplete"] {
        let event = EventDecl {
            name: name.into(),
            usage: EventUsage::Generic,
            flags: 0,
        };
        graph.explicit_events.push(event.clone());
        manifest.core.events.push(event.clone());
        manifest.root.events.push(event);
    }
    manifest.clips.push(ClipDecl {
        name: "ScreenFidget".into(),
        path: "Animations\\ScreenFidget.hkx".into(),
        binding: manifest.clips[0].binding.clone(),
        looping: false,
    });
    graph.roles.push(CapabilityClipRole {
        role: CreatureClipRole::ScriptedAction,
        state_name: "ScreenFidget".into(),
        clip_name: "ScreenFidget".into(),
        generator: CapabilityRoleGenerator::SourceIdle {
            source_group: 20,
            duration_bits: 4.0_f32.to_bits(),
            loop_start_bits: 1.0_f32.to_bits(),
            loop_end_bits: 3.0_f32.to_bits(),
            has_loop_markers: true,
            looping_min: 255,
            looping_max: 255,
            complete_event: "ScreenFidgetComplete".into(),
            overlay: None,
        },
        equipment_index: None,
        trigger_event: Some("ScreenFidget".into()),
        trigger_aliases: vec![],
        motion: Default::default(),
    });
    for (index, start, end, looping) in [
        (0, 0.0_f32, 3.0_f32, false),
        (1, 1.0, 1.0, true),
        (2, 3.0, 0.0, false),
    ] {
        manifest.property_sequences.push(CreaturePropertySequence {
            clip_name: "ScreenFidget".into(),
            crop_start_bits: start.to_bits(),
            crop_end_bits: end.to_bits(),
            looping,
            sequence_names: vec![
                format!("B21CP_screen_{index}"),
                format!("B21CP_screen_alt_{index}"),
            ],
            source_float_slots: vec!["Screen01:0|UV_U".into()],
        });
    }
    let temp = tempfile::tempdir().unwrap();
    let packed = pack_capability_scaffold(&manifest, &graph, temp.path()).unwrap();
    let bytes = std::fs::read(
        &packed
            .artifact(&manifest.paths.core_behavior)
            .unwrap()
            .hkx_path,
    )
    .unwrap();
    let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let objects = doc
        .descendants()
        .filter(|node| node.has_tag_name("hkobject"))
        .collect::<Vec<_>>();
    let parameter = |object: roxmltree::Node<'_, '_>, name| {
        object
            .children()
            .find(|node| node.attribute("name") == Some(name))
            .and_then(|node| node.text())
            .unwrap_or("")
            .to_string()
    };
    let sequences = objects
        .iter()
        .filter(|object| object.attribute("class") == Some("BGSGamebryoSequenceGenerator"))
        .collect::<Vec<_>>();
    assert_eq!(sequences.len(), 6);
    for sequence in sequences {
        assert!(parameter(*sequence, "name").starts_with("B21PropertySequence_"));
        assert!(parameter(*sequence, "pSequence").starts_with("B21CP_screen"));
        assert!(matches!(
            parameter(*sequence, "eBlendModeFunction").as_str(),
            "1" | "BMF_PERCENT"
        ));
    }
    let clocks = objects
        .iter()
        .filter(|object| {
            object.attribute("class") == Some("hkbClipGenerator")
                && parameter(**object, "name").starts_with("B21PropertyClock_")
        })
        .collect::<Vec<_>>();
    assert_eq!(clocks.len(), 3);
    let pairs = objects
        .iter()
        .filter(|object| {
            object.attribute("class") == Some("hkbBlenderGenerator")
                && parameter(**object, "name").starts_with("B21PropertyPair_")
        })
        .collect::<Vec<_>>();
    assert_eq!(pairs.len(), 3);
    for pair in pairs {
        assert_eq!(parameter(*pair, "flags"), "1");
        assert_eq!(parameter(*pair, "indexOfSyncMasterChild"), "0");
    }
    let crops = clocks
        .iter()
        .map(|clock| {
            (
                parameter(**clock, "cropStartAmountLocalTime")
                    .parse::<f32>()
                    .unwrap()
                    .to_bits(),
                parameter(**clock, "cropEndAmountLocalTime")
                    .parse::<f32>()
                    .unwrap()
                    .to_bits(),
            )
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        crops,
        [
            (0.0_f32.to_bits(), 3.0_f32.to_bits()),
            (1.0_f32.to_bits(), 1.0_f32.to_bits()),
            (3.0_f32.to_bits(), 0.0_f32.to_bits())
        ]
        .into()
    );
    let outro = clocks
        .iter()
        .find(|clock| {
            parameter(***clock, "cropStartAmountLocalTime")
                .parse::<f32>()
                .unwrap()
                == 3.0
        })
        .unwrap();
    assert_ne!(parameter(**outro, "triggers"), "null");
}

#[test]
fn property_sequence_manifest_rejects_ambiguous_or_nonfinite_bindings() {
    let (mut manifest, graph) = fixture();
    let binding = CreaturePropertySequence {
        clip_name: "Idle".into(),
        crop_start_bits: 0,
        crop_end_bits: 0,
        looping: true,
        sequence_names: vec!["B21CP_idle".into()],
        source_float_slots: vec!["uv".into()],
    };
    manifest.property_sequences.push(binding.clone());
    manifest.validate().unwrap();
    manifest.property_sequences.push(binding);
    assert!(manifest.validate().is_err());
    manifest.property_sequences.pop();
    manifest.property_sequences[0].crop_start_bits = 1.0_f32.to_bits();
    assert!(emit_capability_scaffold(&manifest, &graph).is_err());
    manifest.property_sequences[0].crop_start_bits = f32::NAN.to_bits();
    assert!(manifest.validate().is_err());
}
