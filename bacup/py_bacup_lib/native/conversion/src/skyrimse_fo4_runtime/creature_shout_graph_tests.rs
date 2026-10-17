use super::*;
use crate::source_rig::{ClipDecl, CreatureManifest, pack_capability_scaffold};

#[test]
#[ignore = "requires extracted Skyrim Draugr/Centurion assets and source-rig recipes"]
fn source_draugr_centurion_shout_lifecycles_pack_and_reload() {
    let mesh_root = PathBuf::from(std::env::var("MODKIT_SKYRIM_MESH_ROOT").unwrap());
    let data = load_animation_data_catalog(&mesh_root.join("animationdata")).unwrap();
    for (family_id, project, recipe_env, start, end) in [
        (
            "draugr",
            "draugr/draugrproject.hkx",
            "MODKIT_SKYRIM_DRAUGR_RECIPE",
            0,
            1,
        ),
        (
            "dwarven_steam",
            "dwarvensteamcenturion/steamproject.hkx",
            "MODKIT_SKYRIM_STEAM_RECIPE",
            1,
            0,
        ),
    ] {
        let project_name = Path::new(project).file_stem().unwrap().to_str().unwrap();
        let evidence = load_family_evidence(
            &mesh_root.join("actors").canonicalize().unwrap(),
            &data[project_name],
            family_id,
            project,
            Vec::new(),
        )
        .unwrap();
        let catalog = build_skyrim_creature_motion_catalog(&[evidence]).unwrap();
        let (graph, variables) = catalog.capability_graph_bundle(family_id).unwrap();
        let roles = graph
            .roles
            .iter()
            .filter(|role| role.trigger_event.as_deref() == Some("shoutStart"))
            .collect::<Vec<_>>();
        let [role] = roles.as_slice() else {
            panic!("{family_id} must have exactly one source shout action");
        };
        assert!(role.trigger_aliases.is_empty());
        let CapabilityRoleGenerator::SourceCasting {
            root,
            stop_events,
            exit_events,
            ..
        } = &role.generator
        else {
            panic!("source shout lifecycle was flattened");
        };
        assert_eq!(stop_events, &["shoutStop"]);
        assert_eq!(
            exit_events
                .iter()
                .filter_map(|event| event.event.event_name.as_deref())
                .collect::<Vec<_>>(),
            ["InterruptCast"]
        );
        let CapabilityGeneratorNode::ModifierGenerator { generator, .. } = root else {
            panic!("source shout activity modifier was dropped");
        };
        let CapabilityGeneratorNode::StateMachine {
            start_state_id,
            states,
            ..
        } = generator.as_ref()
        else {
            panic!("source shout machine was dropped");
        };
        assert_eq!(*start_state_id, start);
        assert_eq!(states.len(), 2);
        let inhale = states.iter().find(|state| state.state_id == start).unwrap();
        let transition = &inhale.transitions.as_ref().unwrap().transitions[0];
        assert_eq!(transition.event.event_name.as_deref(), Some("shoutRelease"));
        assert_eq!(transition.to_state_id, end);
        let exhale = states.iter().find(|state| state.state_id == end).unwrap();
        assert!(
            exhale
                .generator
                .clip_names()
                .contains(&role.clip_name.as_str())
        );
        let CapabilityGeneratorNode::SourceClip {
            playback_speed_bits,
            ..
        } = exhale.generator.as_ref()
        else {
            panic!("source exhale clip playback was dropped");
        };
        assert_eq!(
            f32::from_bits(*playback_speed_bits),
            1.0,
            "must use corrected source HKX clip layout"
        );
        let clip = catalog.clip(&role.clip_name).unwrap();
        assert!(
            clip.annotations
                .iter()
                .any(|annotation| annotation.name == "Voice_SpellFire_Event")
        );
        assert!(
            source_clip_triggers(&role.generator, &role.clip_name)
                .iter()
                .any(
                    |trigger| trigger.event.event.event_name.as_deref() == Some("shoutStop")
                        && trigger.relative_to_end_of_clip
                )
        );
        for event in [
            "shoutStart",
            "shoutRelease",
            "shoutStop",
            "Voice_SpellFire_Event",
            "InterruptCast",
        ] {
            assert!(
                graph
                    .explicit_events
                    .iter()
                    .any(|declaration| declaration.name == event),
                "{family_id} missing {event}"
            );
        }
        let shout_clips = role.generator.clip_names(&role.clip_name);
        assert!(
            graph
                .roles
                .iter()
                .filter(|role| role.trigger_event.as_deref() != Some("shoutStart"))
                .all(|role| !role
                    .generator
                    .clip_names(&role.clip_name)
                    .iter()
                    .any(|clip| shout_clips.contains(clip)))
        );
        let source: serde_json::Value =
            serde_json::from_slice(&std::fs::read(std::env::var(recipe_env).unwrap()).unwrap())
                .unwrap();
        let mut rig: CreatureManifest = serde_json::from_value(source["rig"].clone()).unwrap();
        let original_skeleton = rig.animation_skeleton.clone();
        let original = rig.clips[0].clone();
        rig.clips.clear();
        let mut seen = BTreeSet::new();
        for role in &graph.roles {
            for name in role.generator.clip_names(&role.clip_name) {
                if seen.insert(name.to_string()) {
                    rig.clips.push(ClipDecl {
                        name: name.into(),
                        path: format!("{}\\{name}.hkx", original.path.rsplit_once('\\').unwrap().0),
                        binding: original.binding.clone(),
                        looping: matches!(
                            role.role,
                            CreatureClipRole::Idle
                                | CreatureClipRole::GroundForward
                                | CreatureClipRole::SwimIdle
                                | CreatureClipRole::SwimForward
                                | CreatureClipRole::FlyIdle
                                | CreatureClipRole::FlyForward
                                | CreatureClipRole::StationaryIdle
                                | CreatureClipRole::TurnLeft
                                | CreatureClipRole::TurnRight
                                | CreatureClipRole::ContinuousAttackLoop
                        ),
                    });
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
        assert_eq!(rig.animation_skeleton, original_skeleton);
        let output = tempfile::tempdir().unwrap();
        let report = pack_capability_scaffold(&rig, &graph, output.path()).unwrap();
        for artifact in report.artifacts {
            let bytes = std::fs::read(&artifact.hkx_path).unwrap();
            let file = HkxFile::read(&bytes).unwrap();
            assert_eq!(
                file.save(),
                bytes,
                "{family_id}: {}",
                artifact.hkx_path.display()
            );
        }
    }
}
