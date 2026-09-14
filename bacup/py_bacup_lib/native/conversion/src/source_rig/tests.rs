use std::fs;
use std::path::PathBuf;

use super::emit::emit_capability_scaffold;
use super::manifest::{
    CapabilityClipRole, CapabilityGraphManifest, CreatureClipRole, CreatureGraphTemplate,
    OverlayClipRole, RAGDOLL_ENTER_EVENTS, RAGDOLL_TRANSITION_EVENTS,
};
use super::pack::pack_capability_scaffold;
use super::race_data::*;
use super::*;
use crate::ids::FormKey;
use crate::record::FieldValue;

fn controller_decl() -> CreatureControllerDecl {
    CreatureControllerDecl {
        collision_filter_info: 1,
        rigid_body_type: 255,
        model_up_ms: [0.0, 0.0, 1.0, 0.0],
        model_forward_ms: [1.0, 0.0, 0.0, 0.0],
        model_right_ms: [0.0, -1.0, 0.0, 0.0],
        model_scale: 1.0,
    }
}

#[test]
fn articulated_collision_accepts_only_owned_or_explicitly_unsupported_ragdolls() {
    let unsupported = RagdollDisposition::NoRagdoll {
        reason: NoRagdollReason::UnsupportedSourceRagdoll,
    };
    assert!(unsupported.resolves_articulated_collision());
    for reason in [
        NoRagdollReason::SourceHasNoRagdoll,
        NoRagdollReason::Deferred,
        NoRagdollReason::NotApplicable,
    ] {
        assert!(
            !RagdollDisposition::NoRagdoll { reason }.resolves_articulated_collision(),
            "{reason:?} must not account for articulated source collision"
        );
    }
}

fn wolf_manifest() -> CreatureManifest {
    let core = GraphDeclarations {
        events: vec![
            EventDecl {
                name: "meleeBite".to_string(),
                usage: EventUsage::MeleeAttack,
                flags: 0,
            },
            EventDecl {
                name: "HitFrame".to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            },
        ],
        variables: vec![
            VariableDecl {
                name: "bGraphDriven".to_string(),
                variable_type: VariableType::Bool,
                initial_value: VariableValue::Bool(true),
            },
            VariableDecl {
                name: "Speed".to_string(),
                variable_type: VariableType::Real,
                initial_value: VariableValue::Real(0.0),
            },
        ],
        character_properties: vec![PropertyDecl {
            name: "HeadBoneIndex".to_string(),
            variable_type: VariableType::Int32,
            initial_value: VariableValue::Int32(3),
        }],
    };
    CreatureManifest {
        creature_name: "SourceWolf".to_string(),
        visual_skeleton_nif: "CharacterAssets\\Skeleton.nif".to_string(),
        animation_skeleton: SkeletonDecl {
            path: "CharacterAssets\\Skeleton.hkx".to_string(),
            runtime_name: "SourceWolfSkeleton".to_string(),
            bones: vec![
                BoneDecl {
                    name: "Root".to_string(),
                    parent_index: None,
                },
                BoneDecl {
                    name: "COM".to_string(),
                    parent_index: Some(0),
                },
                BoneDecl {
                    name: "Spine".to_string(),
                    parent_index: Some(1),
                },
                BoneDecl {
                    name: "Head".to_string(),
                    parent_index: Some(2),
                },
            ],
            float_slots: Vec::new(),
        },
        controller: controller_decl(),
        ragdoll: RagdollDisposition::NoRagdoll {
            reason: NoRagdollReason::SourceHasNoRagdoll,
        },
        clips: vec![ClipDecl {
            name: "Idle".to_string(),
            path: "Animations\\Idle.hkx".to_string(),
            binding: ClipBinding {
                skeleton_path: "CharacterAssets\\Skeleton.hkx".to_string(),
                original_skeleton_name: "SourceWolfSkeleton".to_string(),
                declared_transform_tracks: 4,
                transform_track_to_bone_indices: vec![0, 1, 2, 3],
                declared_float_tracks: 0,
                float_track_to_float_slot_indices: Vec::new(),
            },
            looping: true,
        }],
        idle_clip: "Idle".to_string(),
        capsule: Capsule {
            height: 1.2,
            radius: 0.3,
        },
        paths: ScaffoldPaths {
            project: "SourceWolfProject.hkx".to_string(),
            character: "Characters\\SourceWolfCharacter.hkx".to_string(),
            root_behavior: "Behaviors\\SourceWolfRootBehavior.hkx".to_string(),
            core_behavior: "Behaviors\\SourceWolfCoreBehavior.hkx".to_string(),
        },
        root: core.clone(),
        core,
    }
}

fn wolf_mvp() -> (CreatureManifest, MvpGraphManifest) {
    let skeleton_path = "Actors\\B21_SkyrimWolf\\CharacterAssets\\Skeleton.hkx";
    let clip = |name: &str, file_name: &str, looping: bool| ClipDecl {
        name: name.to_string(),
        path: format!("Actors\\B21_SkyrimWolf\\Animations\\{file_name}.hkx"),
        binding: ClipBinding {
            skeleton_path: skeleton_path.to_string(),
            original_skeleton_name: "B21_SkyrimWolfSkeleton".to_string(),
            declared_transform_tracks: 4,
            transform_track_to_bone_indices: vec![0, 1, 2, 3],
            declared_float_tracks: 0,
            float_track_to_float_slot_indices: Vec::new(),
        },
        looping,
    };
    let declarations = GraphDeclarations {
        events: vec![
            EventDecl {
                name: "Idle".to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            },
            EventDecl {
                name: "startWalk".to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            },
            EventDecl {
                name: "TurnLeft90".to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            },
            EventDecl {
                name: "TurnRight90".to_string(),
                usage: EventUsage::Generic,
                flags: 0,
            },
            EventDecl {
                name: "meleeWolfAttack1".to_string(),
                usage: EventUsage::MeleeAttack,
                flags: 0,
            },
        ],
        variables: vec![VariableDecl {
            name: "bGraphDriven".to_string(),
            variable_type: VariableType::Bool,
            initial_value: VariableValue::Bool(true),
        }],
        character_properties: Vec::new(),
    };
    let manifest = CreatureManifest {
        creature_name: "B21_SkyrimWolf".to_string(),
        visual_skeleton_nif: "Actors\\B21_SkyrimWolf\\CharacterAssets\\Skeleton.nif".to_string(),
        animation_skeleton: SkeletonDecl {
            path: skeleton_path.to_string(),
            runtime_name: "B21_SkyrimWolfSkeleton".to_string(),
            bones: vec![
                BoneDecl {
                    name: "NPC Root [Root]".to_string(),
                    parent_index: None,
                },
                BoneDecl {
                    name: "NPC COM [COM ]".to_string(),
                    parent_index: Some(0),
                },
                BoneDecl {
                    name: "NPC Spine [Spn0]".to_string(),
                    parent_index: Some(1),
                },
                BoneDecl {
                    name: "NPC Head [Head]".to_string(),
                    parent_index: Some(2),
                },
            ],
            float_slots: Vec::new(),
        },
        controller: controller_decl(),
        ragdoll: RagdollDisposition::NoRagdoll {
            reason: NoRagdollReason::SourceHasNoRagdoll,
        },
        clips: vec![
            clip("mt_idle_wolf", "mt_idle_wolf", true),
            clip("walkforward_wolf", "walkforward_wolf", true),
            clip("turncannedl90_wolf", "turncannedl90_wolf", false),
            clip("turncannedr90_wolf", "turncannedr90_wolf", false),
            clip("attack1", "attack1", false),
        ],
        idle_clip: "mt_idle_wolf".to_string(),
        capsule: Capsule {
            height: 1.7,
            radius: 0.4,
        },
        paths: ScaffoldPaths {
            project: "Actors\\B21_SkyrimWolf\\B21_SkyrimWolfProject.hkx".to_string(),
            character: "Actors\\B21_SkyrimWolf\\Characters\\B21_SkyrimWolfCharacter.hkx"
                .to_string(),
            root_behavior: "Actors\\B21_SkyrimWolf\\Behaviors\\B21_SkyrimWolfRootBehavior.hkx"
                .to_string(),
            core_behavior: "Actors\\B21_SkyrimWolf\\Behaviors\\B21_SkyrimWolfCoreBehavior.hkx"
                .to_string(),
        },
        root: declarations.clone(),
        core: declarations,
    };
    let graph = MvpGraphManifest {
        idle_clip: "mt_idle_wolf".to_string(),
        walk_forward_clip: "walkforward_wolf".to_string(),
        turn_left_90_clip: "turncannedl90_wolf".to_string(),
        turn_right_90_clip: "turncannedr90_wolf".to_string(),
        attack_1_clip: "attack1".to_string(),
        melee_event: "meleeWolfAttack1".to_string(),
    };
    (manifest, graph)
}

fn with_source_owned_ragdoll(mut manifest: CreatureManifest) -> CreatureManifest {
    manifest.ragdoll = RagdollDisposition::SourceOwned {
        runtime_path: "Actors\\B21_SkyrimWolf\\CharacterAssets\\Ragdoll.hkx".to_string(),
        receipt: SourceOwnedRagdollReceipt {
            byte_len: 64,
            blake3: "ab".repeat(32),
            powered_ragdoll: None,
        },
    };
    manifest.root.events.extend(
        RAGDOLL_TRANSITION_EVENTS
            .into_iter()
            .chain(RAGDOLL_ENTER_EVENTS)
            .map(generic_event),
    );
    manifest
}

fn gecko_mvp() -> (CreatureManifest, MvpGraphManifest) {
    let (mut manifest, _) = wolf_mvp();
    manifest.creature_name = "B21_Gecko".to_string();
    manifest.visual_skeleton_nif = "Actors\\B21_Gecko\\CharacterAssets\\Skeleton.nif".to_string();
    manifest.animation_skeleton.path =
        "Actors\\B21_Gecko\\CharacterAssets\\Skeleton.hkx".to_string();
    manifest.animation_skeleton.runtime_name = "NVGecko".to_string();
    manifest.animation_skeleton.bones = (0_usize..87)
        .map(|index| BoneDecl {
            name: match index {
                0 => "Bip01".to_string(),
                3 => "Bip01 Pelvis".to_string(),
                _ => format!("GeckoBone{index:02}"),
            },
            parent_index: index.checked_sub(1),
        })
        .collect();
    let gecko_clips = [
        ("mtidle", true),
        ("mtforward", true),
        ("turnleft90", false),
        ("turnright90", false),
        ("attackbite", false),
    ];
    manifest.clips = gecko_clips
        .into_iter()
        .map(|(name, looping)| ClipDecl {
            name: name.to_string(),
            path: format!("Actors\\B21_Gecko\\Animations\\{name}.hkx"),
            binding: ClipBinding {
                skeleton_path: manifest.animation_skeleton.path.clone(),
                original_skeleton_name: "NVGecko".to_string(),
                declared_transform_tracks: 84,
                transform_track_to_bone_indices: (3..87).collect(),
                declared_float_tracks: 0,
                float_track_to_float_slot_indices: Vec::new(),
            },
            looping,
        })
        .collect();
    manifest.idle_clip = "mtidle".to_string();
    manifest.paths = ScaffoldPaths {
        project: "Actors\\B21_Gecko\\B21_GeckoProject.hkx".to_string(),
        character: "Actors\\B21_Gecko\\Characters\\B21_GeckoCharacter.hkx".to_string(),
        root_behavior: "Actors\\B21_Gecko\\Behaviors\\B21_GeckoRootBehavior.hkx".to_string(),
        core_behavior: "Actors\\B21_Gecko\\Behaviors\\B21_GeckoCoreBehavior.hkx".to_string(),
    };
    manifest.core.events[4].name = "meleeGeckoBite".to_string();
    manifest.root = manifest.core.clone();
    let graph = MvpGraphManifest {
        idle_clip: "mtidle".to_string(),
        walk_forward_clip: "mtforward".to_string(),
        turn_left_90_clip: "turnleft90".to_string(),
        turn_right_90_clip: "turnright90".to_string(),
        attack_1_clip: "attackbite".to_string(),
        melee_event: "meleeGeckoBite".to_string(),
    };
    (manifest, graph)
}

fn gecko_motion() -> MvpMotionManifest {
    MvpMotionManifest {
        walk_forward: ClipMotionPolicy {
            animation_driven: true,
            extracted_planar_reference_frames: 3,
        },
        attack_1: ClipMotionPolicy {
            animation_driven: true,
            extracted_planar_reference_frames: 2,
        },
        ..MvpMotionManifest::default()
    }
}

fn ranged_capability() -> (CreatureManifest, CapabilityGraphManifest) {
    let (manifest, _) = wolf_mvp();
    let role =
        |role, state_name: &str, clip_name: &str, trigger_event: Option<&str>| CapabilityClipRole {
            role,
            state_name: state_name.to_string(),
            clip_name: clip_name.to_string(),
            generator: CapabilityRoleGenerator::Single,
            trigger_event: trigger_event.map(str::to_string),
            trigger_aliases: Vec::new(),
            motion: ClipMotionPolicy::default(),
        };
    let event = |name: &str| EventDecl {
        name: name.to_string(),
        usage: EventUsage::Generic,
        flags: 0,
    };
    let graph = CapabilityGraphManifest {
        template: CreatureGraphTemplate::GroundRangedProjectile,
        roles: vec![
            role(CreatureClipRole::Idle, "Rest", "mt_idle_wolf", None),
            role(
                CreatureClipRole::GroundForward,
                "Advance",
                "walkforward_wolf",
                Some("beginStride"),
            ),
            role(
                CreatureClipRole::TurnLeft90,
                "RotatePort",
                "turncannedl90_wolf",
                Some("rotatePort"),
            ),
            role(
                CreatureClipRole::ProjectileAttack,
                "SpitBolt",
                "turncannedr90_wolf",
                Some("spitBolt"),
            ),
            role(
                CreatureClipRole::ProjectileAttack,
                "SpitAcid",
                "attack1",
                Some("spitAcid"),
            ),
        ],
        candidate_attack_bindings: Vec::new(),
        idle_event: "Idle".to_string(),
        explicit_events: ["Idle", "beginStride", "rotatePort", "spitBolt", "spitAcid"]
            .map(event)
            .to_vec(),
        overlays: Vec::new(),
    };
    (manifest, graph)
}

fn passive_ground_capability() -> (CreatureManifest, CapabilityGraphManifest) {
    let (manifest, _) = wolf_mvp();
    let graph = CapabilityGraphManifest {
        template: CreatureGraphTemplate::PassiveGround,
        roles: vec![
            capability_role(CreatureClipRole::Idle, "Rest", "mt_idle_wolf", None),
            capability_role(
                CreatureClipRole::GroundForward,
                "Advance",
                "walkforward_wolf",
                Some("startWalk"),
            ),
        ],
        candidate_attack_bindings: Vec::new(),
        idle_event: "Idle".to_string(),
        explicit_events: ["Idle", "startWalk"].map(generic_event).to_vec(),
        overlays: Vec::new(),
    };
    (manifest, graph)
}

fn capability_role(
    role: CreatureClipRole,
    state_name: &str,
    clip_name: &str,
    trigger_event: Option<&str>,
) -> CapabilityClipRole {
    CapabilityClipRole {
        role,
        state_name: state_name.to_string(),
        clip_name: clip_name.to_string(),
        generator: CapabilityRoleGenerator::Single,
        trigger_event: trigger_event.map(str::to_string),
        trigger_aliases: Vec::new(),
        motion: ClipMotionPolicy::default(),
    }
}

fn generic_event(name: &str) -> EventDecl {
    EventDecl {
        name: name.to_string(),
        usage: EventUsage::Generic,
        flags: 0,
    }
}

fn swim_capability() -> (CreatureManifest, CapabilityGraphManifest) {
    let (manifest, _) = wolf_mvp();
    let graph = CapabilityGraphManifest {
        template: CreatureGraphTemplate::Swim,
        roles: vec![
            capability_role(CreatureClipRole::SwimIdle, "SwimIdle", "mt_idle_wolf", None),
            capability_role(
                CreatureClipRole::SwimForward,
                "SwimForward",
                "walkforward_wolf",
                Some("moveStart"),
            ),
        ],
        candidate_attack_bindings: Vec::new(),
        idle_event: "moveStop".to_string(),
        explicit_events: ["moveStop", "moveStart"].map(generic_event).to_vec(),
        overlays: Vec::new(),
    };
    (manifest, graph)
}

fn fly_capability() -> (CreatureManifest, CapabilityGraphManifest) {
    let (manifest, _) = wolf_mvp();
    let graph = CapabilityGraphManifest {
        template: CreatureGraphTemplate::Fly,
        roles: vec![
            capability_role(
                CreatureClipRole::FlyIdle,
                "FlightHover",
                "mt_idle_wolf",
                None,
            ),
            capability_role(
                CreatureClipRole::FlyForward,
                "FlightCruise",
                "walkforward_wolf",
                Some("moveStart"),
            ),
        ],
        candidate_attack_bindings: Vec::new(),
        idle_event: "moveStop".to_string(),
        explicit_events: ["moveStop", "moveStart"].map(generic_event).to_vec(),
        overlays: Vec::new(),
    };
    (manifest, graph)
}

fn multimodal_capability(
    template: CreatureGraphTemplate,
) -> (CreatureManifest, CapabilityGraphManifest) {
    let (mut manifest, _) = wolf_mvp();
    let cloned_clip = |source: &ClipDecl, name: &str, looping: bool| {
        let mut clip = source.clone();
        clip.name = name.to_string();
        clip.path = format!("Actors\\B21_SkyrimWolf\\Animations\\{name}.hkx");
        clip.looping = looping;
        clip
    };
    let secondary_looping = template != CreatureGraphTemplate::GroundMeleeRanged;
    manifest.clips.push(cloned_clip(
        &manifest.clips[0],
        "secondary_idle",
        secondary_looping,
    ));
    manifest.clips.push(cloned_clip(
        &manifest.clips[1],
        "secondary_forward",
        secondary_looping,
    ));

    let (secondary_idle_role, secondary_forward_role, secondary_idle_event, secondary_move_event) =
        match template {
            CreatureGraphTemplate::GroundSwim => (
                CreatureClipRole::SwimIdle,
                CreatureClipRole::SwimForward,
                "enterSwim",
                "swimForward",
            ),
            CreatureGraphTemplate::GroundFly => (
                CreatureClipRole::FlyIdle,
                CreatureClipRole::FlyForward,
                "enterFlight",
                "flyForward",
            ),
            _ => (
                CreatureClipRole::TurnLeft90,
                CreatureClipRole::TurnRight90,
                "turnLeft",
                "turnRight",
            ),
        };
    let graph = CapabilityGraphManifest {
        template,
        roles: vec![
            capability_role(CreatureClipRole::Idle, "GroundIdle", "mt_idle_wolf", None),
            capability_role(
                CreatureClipRole::GroundForward,
                "GroundForward",
                "walkforward_wolf",
                Some("groundForward"),
            ),
            capability_role(
                secondary_idle_role,
                "SecondaryIdle",
                "secondary_idle",
                Some(secondary_idle_event),
            ),
            capability_role(
                secondary_forward_role,
                "SecondaryForward",
                "secondary_forward",
                Some(secondary_move_event),
            ),
            capability_role(
                CreatureClipRole::MeleeAttack,
                "MeleeAttack",
                "attack1",
                Some("meleeMultiAttack"),
            ),
            capability_role(
                CreatureClipRole::ProjectileAttack,
                "ProjectileAttack",
                "turncannedr90_wolf",
                Some("fireProjectile"),
            ),
        ],
        candidate_attack_bindings: Vec::new(),
        idle_event: "Idle".to_string(),
        explicit_events: vec![
            generic_event("Idle"),
            generic_event("groundForward"),
            generic_event(secondary_idle_event),
            generic_event(secondary_move_event),
            EventDecl {
                name: "meleeMultiAttack".to_string(),
                usage: EventUsage::MeleeAttack,
                flags: 0,
            },
            generic_event("fireProjectile"),
        ],
        overlays: Vec::new(),
    };
    (manifest, graph)
}

fn stationary_turret_capability() -> (CreatureManifest, CapabilityGraphManifest) {
    let (mut manifest, _) = wolf_mvp();
    let aim_variables = vec![
        VariableDecl {
            name: "AimHeadingMaxCCW".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(90.0),
        },
        VariableDecl {
            name: "AimHeadingMaxCW".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(90.0),
        },
        VariableDecl {
            name: "fDirectAtHeadingSavedGain".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.0),
        },
        VariableDecl {
            name: "bAimActive".to_string(),
            variable_type: VariableType::Bool,
            initial_value: VariableValue::Bool(false),
        },
        VariableDecl {
            name: "AimHeadingCurrent".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.0),
        },
        VariableDecl {
            name: "AimPitchCurrent".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.0),
        },
        VariableDecl {
            name: "fAimOnGain".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.05),
        },
        VariableDecl {
            name: "camerafromx".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.0),
        },
        VariableDecl {
            name: "camerafromy".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.0),
        },
        VariableDecl {
            name: "camerafromz".to_string(),
            variable_type: VariableType::Real,
            initial_value: VariableValue::Real(0.0),
        },
    ];
    let aim_properties = vec![
        PropertyDecl {
            name: "DirectAtHeadingSourceBoneIndex".to_string(),
            variable_type: VariableType::Int32,
            initial_value: VariableValue::Int32(0),
        },
        PropertyDecl {
            name: "DirectAtHeadingBoneIndex".to_string(),
            variable_type: VariableType::Int32,
            initial_value: VariableValue::Int32(2),
        },
    ];
    manifest.core.variables.extend(aim_variables);
    manifest.core.character_properties.extend(aim_properties);
    manifest.root = manifest.core.clone();
    let graph = CapabilityGraphManifest {
        template: CreatureGraphTemplate::StationaryTurret,
        roles: vec![
            capability_role(
                CreatureClipRole::StationaryIdle,
                "TurretIdle",
                "mt_idle_wolf",
                None,
            ),
            capability_role(
                CreatureClipRole::ProjectileAttack,
                "TurretFire",
                "attack1",
                Some("attackStart"),
            ),
        ],
        candidate_attack_bindings: Vec::new(),
        idle_event: "g_defaultState".to_string(),
        explicit_events: ["g_defaultState", "attackStart", "weaponFire"]
            .map(generic_event)
            .to_vec(),
        overlays: Vec::new(),
    };
    (manifest, graph)
}

fn robot_continuous_attack_capability() -> (CreatureManifest, CapabilityGraphManifest) {
    let (manifest, _) = wolf_mvp();
    let graph = CapabilityGraphManifest {
        template: CreatureGraphTemplate::RobotContinuousAttack,
        roles: vec![
            capability_role(CreatureClipRole::Idle, "RobotIdle", "mt_idle_wolf", None),
            capability_role(
                CreatureClipRole::ContinuousAttackStart,
                "ContinuousStart",
                "turncannedl90_wolf",
                Some("attackStartAuto"),
            ),
            capability_role(
                CreatureClipRole::ContinuousAttackLoop,
                "ContinuousLoop",
                "walkforward_wolf",
                Some("weaponFire"),
            ),
            capability_role(
                CreatureClipRole::ContinuousAttackStop,
                "ContinuousStop",
                "attack1",
                Some("attackRelease"),
            ),
        ],
        candidate_attack_bindings: Vec::new(),
        idle_event: "g_defaultState".to_string(),
        explicit_events: [
            "g_defaultState",
            "attackStartAuto",
            "weaponFire",
            "attackRelease",
        ]
        .map(generic_event)
        .to_vec(),
        overlays: Vec::new(),
    };
    (manifest, graph)
}

fn overlay_capability() -> (CreatureManifest, CapabilityGraphManifest) {
    let (mut manifest, mut graph) = ranged_capability();
    let mut overlay_clip = manifest.clips[0].clone();
    overlay_clip.name = "GlowOverlay".to_string();
    overlay_clip.path = "Actors\\B21_SkyrimWolf\\Animations\\glow_overlay.hkx".to_string();
    manifest.clips.push(overlay_clip);
    graph.explicit_events.push(generic_event("overlayOn"));
    graph.explicit_events.push(generic_event("overlayOff"));
    graph.overlays.push(OverlayClipRole {
        name: "Glow".to_string(),
        clip_name: "GlowOverlay".to_string(),
        start_event: "overlayOn".to_string(),
        stop_event: "overlayOff".to_string(),
        motion: ClipMotionPolicy::default(),
    });
    (manifest, graph)
}

fn assert_capability_pack_roundtrip(manifest: &CreatureManifest, graph: &CapabilityGraphManifest) {
    let output = tempfile::tempdir().unwrap();
    let report = pack_capability_scaffold(manifest, graph, output.path()).unwrap();
    assert_eq!(report.artifacts.len(), 4);
    for artifact in &report.artifacts {
        let bytes = fs::read(&artifact.hkx_path).unwrap();
        let packed = havok_native::hkx::HkxFile::read(&bytes).unwrap();
        assert_eq!(packed.class_version(), 11);
        assert_eq!(packed.contents_version(), "hk_2014.1.0-r1");
        assert_eq!(packed.packfile().header.pointer_size, 8);
        let roundtrip = reread_packed_xml(&report, &artifact.runtime_path);
        assert!(validate_fo4_havok_xml_signatures(&roundtrip).is_ok());
    }
}

fn error_codes(result: Result<(), ValidationErrors>) -> Vec<&'static str> {
    result
        .expect_err("validation was expected to fail")
        .0
        .into_iter()
        .map(|error| error.code)
        .collect()
}

#[test]
fn bethesda_runtime_paths_may_contain_spaces_inside_components() {
    let mut manifest = wolf_manifest();
    manifest.visual_skeleton_nif = "Actors\\Wolf\\Character Assets\\Skeleton.nif".to_string();

    manifest.validate().unwrap();
}

#[test]
fn synthetic_wolf_emits_deterministic_idle_scaffold() {
    let manifest = wolf_manifest();
    let first = emit_idle_scaffold(&manifest).unwrap();
    let second = emit_idle_scaffold(&manifest).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.artifacts.len(), 4);
    assert_eq!(
        first.race_visual_skeleton_nif,
        "CharacterAssets\\Skeleton.nif"
    );

    let character = first.artifact(&manifest.paths.character).unwrap();
    assert!(
        character
            .xml
            .contains("<hkparam name=\"rigName\">CharacterAssets\\Skeleton.hkx</hkparam>")
    );
    assert!(!character.xml.contains("Skeleton.nif"));
    assert!(character.xml.contains(
        "<hkparam name=\"behaviorFilename\">Behaviors\\SourceWolfRootBehavior.hkx</hkparam>"
    ));

    let root = first.artifact(&manifest.paths.root_behavior).unwrap();
    assert!(root.xml.contains("class=\"BSBehaviorGraphSwapGenerator\""));
    assert!(root.xml.contains("<hkparam name=\"userData\">1</hkparam>"));

    let core = first.artifact(&manifest.paths.core_behavior).unwrap();
    assert!(core.xml.contains("class=\"hkbClipGenerator\""));
    assert!(
        core.xml
            .contains("<hkparam name=\"animationName\">Animations\\Idle.hkx</hkparam>")
    );
    assert!(
        first
            .artifacts
            .iter()
            .all(|artifact| validate_havok_xml(&artifact.xml).is_ok())
    );
    assert!(
        first
            .artifacts
            .iter()
            .all(|artifact| validate_fo4_havok_xml_signatures(&artifact.xml).is_ok())
    );
}

#[test]
fn synthetic_wolf_packs_four_readable_fo4_hkx_files() {
    let manifest = wolf_manifest();
    let output = tempfile::tempdir().unwrap();
    let report = pack_idle_scaffold(&manifest, output.path()).unwrap();

    assert_eq!(report.artifacts.len(), 4);
    assert!(
        report
            .runtime_manifest
            .cross_file_paths()
            .into_iter()
            .all(|path| path.to_ascii_lowercase().ends_with(".hkx"))
    );

    for artifact in &report.artifacts {
        assert!(artifact.xml_path.is_file());
        assert!(artifact.hkx_path.is_file());
        let bytes = fs::read(&artifact.hkx_path).unwrap();
        let packed = havok_native::hkx::HkxFile::read(&bytes).unwrap();
        assert_eq!(packed.class_version(), 11);
        assert_eq!(packed.contents_version(), "hk_2014.1.0-r1");
        assert_eq!(packed.packfile().header.pointer_size, 8);
        assert!(
            validate_fo4_havok_xml_signatures(&reread_packed_xml(&report, &artifact.runtime_path))
                .is_ok()
        );
    }

    let character = report.artifact(&manifest.paths.character).unwrap();
    let character_xml = fs::read_to_string(&character.xml_path).unwrap();
    assert!(character_xml.contains("<hkparam name=\"ragdollName\"/>"));
    assert!(!character_xml.to_ascii_lowercase().contains("ragdoll.hkx"));
}

#[test]
fn source_owned_ragdoll_is_character_linked_and_packs_powered_death_state() {
    let (manifest, graph) = wolf_mvp();
    let manifest = with_source_owned_ragdoll(manifest);
    let output = tempfile::tempdir().unwrap();
    let report = pack_mvp_scaffold(&manifest, &graph, output.path()).unwrap();

    let character = report.artifact(&manifest.paths.character).unwrap();
    let character_xml = fs::read_to_string(&character.xml_path).unwrap();
    assert!(
        character_xml
            .contains("<hkparam name=\"ragdollName\">CharacterAssets\\Ragdoll.hkx</hkparam>")
    );
    let root = report.artifact(&manifest.paths.root_behavior).unwrap();
    let root_xml = reread_packed_xml(&report, &root.runtime_path);
    for class_name in [
        "hkbStateMachine",
        "hkbPoweredRagdollControlsModifier",
        "hkbReferencePoseGenerator",
        "hkbModifierGenerator",
    ] {
        assert!(root_xml.contains(&format!("class=\"{class_name}\"")));
    }
    for event in RAGDOLL_TRANSITION_EVENTS {
        assert_eq!(
            root_xml.matches(&format!(">{event}</hkcstring>")).count(),
            1
        );
    }
    let authored_root = fs::read_to_string(&root.xml_path).unwrap();
    assert!(authored_root.contains(
        "name=\"#0203\" class=\"hkbStateMachineTransitionInfoArray\" signature=\"0x704a19af\"><hkparam name=\"transitions\" numelements=\"2\""
    ));
    let root_facts = validate_havok_xml(&authored_root).unwrap();
    let core = report.artifact(&manifest.paths.core_behavior).unwrap();
    let core_facts = validate_havok_xml(&fs::read_to_string(&core.xml_path).unwrap()).unwrap();
    let root_events = root_facts
        .event_names
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let core_events = core_facts
        .event_names
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(root_events.is_superset(&core_events));
    assert!(root_events.len() > core_events.len());
}

#[test]
fn source_owned_rgdl_binds_only_proven_feedback_and_pose_bones() {
    let (manifest, graph) = wolf_mvp();
    let mut manifest = with_source_owned_ragdoll(manifest);
    let RagdollDisposition::SourceOwned { receipt, .. } = &mut manifest.ragdoll else {
        unreachable!();
    };
    receipt.powered_ragdoll = Some(SourceOwnedPoweredRagdollConfig {
        source: SourcePoweredRagdollEvidence {
            dynamic_bone_count: 2,
            unknown_byte_2: 0,
            unknown_byte_3: 0,
            unknown_byte_4: 0,
            unknown_byte_5: 0,
            enabled_feedback: true,
            enabled_foot_ik: false,
            enabled_look_ik: false,
            enabled_grab_ik: false,
            enabled_pose_matching: true,
            unknown_byte_11: 0,
            unknown_trailing_u8: None,
            feedback: Some(SourcePoweredRagdollFeedbackEvidence {
                dynamic_keyframe_blend_amount_bits: 0.5_f32.to_bits(),
                hierarchy_gain_bits: 1.0_f32.to_bits(),
                position_gain_bits: 2.0_f32.to_bits(),
                velocity_gain_bits: 3.0_f32.to_bits(),
                acceleration_gain_bits: 4.0_f32.to_bits(),
                snap_gain_bits: 5.0_f32.to_bits(),
                velocity_damping_bits: 6.0_f32.to_bits(),
                snap_max_linear_velocity_bits: 7.0_f32.to_bits(),
                snap_max_angular_velocity_bits: 8.0_f32.to_bits(),
                snap_max_linear_distance_bits: 9.0_f32.to_bits(),
                snap_max_angular_distance_bits: 10.0_f32.to_bits(),
                position_max_velocity_linear_bits: 11.0_f32.to_bits(),
                position_max_velocity_angular_bits: 12.0_f32.to_bits(),
                position_max_velocity_projectile: 13,
                position_max_velocity_melee: 14,
                bones: vec![0, 3],
            }),
            pose_matching: Some(SourcePoweredRagdollPoseEvidence {
                matching_bones: [Some(0), Some(3), None],
                flags: 0,
                unknown: 0,
                motors_strength_bits: 3000.0_f32.to_bits(),
                pose_activation_delay_time_bits: 2.0_f32.to_bits(),
                match_error_allowance_bits: 0.28_f32.to_bits(),
                displacement_to_disable_bits: 1.0_f32.to_bits(),
            }),
            death_pose_animation: None,
        },
        target_control_policy: PoweredRagdollTargetControlPolicy::Fo4RuntimeDefaultsV1,
        proven_inert_source_fields: vec![
            PoweredRagdollSourceField::DataUnknownByte2,
            PoweredRagdollSourceField::DataUnknownByte3,
            PoweredRagdollSourceField::DataUnknownByte4,
            PoweredRagdollSourceField::DataUnknownByte5,
            PoweredRagdollSourceField::DataEnabledFootIk,
            PoweredRagdollSourceField::DataEnabledLookIk,
            PoweredRagdollSourceField::DataEnabledGrabIk,
            PoweredRagdollSourceField::DataUnknownByte11,
            PoweredRagdollSourceField::FeedbackDynamicKeyframeBlendAmount,
            PoweredRagdollSourceField::FeedbackHierarchyGain,
            PoweredRagdollSourceField::FeedbackPositionGain,
            PoweredRagdollSourceField::FeedbackVelocityGain,
            PoweredRagdollSourceField::FeedbackAccelerationGain,
            PoweredRagdollSourceField::FeedbackSnapGain,
            PoweredRagdollSourceField::FeedbackVelocityDamping,
            PoweredRagdollSourceField::FeedbackSnapMaxLinearVelocity,
            PoweredRagdollSourceField::FeedbackSnapMaxAngularVelocity,
            PoweredRagdollSourceField::FeedbackSnapMaxLinearDistance,
            PoweredRagdollSourceField::FeedbackSnapMaxAngularDistance,
            PoweredRagdollSourceField::FeedbackPositionMaxVelocityLinear,
            PoweredRagdollSourceField::FeedbackPositionMaxVelocityAngular,
            PoweredRagdollSourceField::FeedbackPositionMaxVelocityProjectile,
            PoweredRagdollSourceField::FeedbackPositionMaxVelocityMelee,
            PoweredRagdollSourceField::PoseFlags,
            PoweredRagdollSourceField::PoseUnknown,
            PoweredRagdollSourceField::PoseMotorsStrength,
            PoweredRagdollSourceField::PoseActivationDelayTime,
            PoweredRagdollSourceField::PoseMatchErrorAllowance,
            PoweredRagdollSourceField::PoseDisplacementToDisable,
        ]
        .into_iter()
        .map(powered_ragdoll_inert_receipt)
        .collect(),
    });

    let scaffold = emit_mvp_scaffold(&manifest, &graph).unwrap();
    let root = &scaffold
        .artifact(&manifest.paths.root_behavior)
        .unwrap()
        .xml;
    assert!(root.contains("class=\"hkbBoneIndexArray\" signature=\"0x3d26f425\""));
    assert!(root.contains("<hkparam name=\"boneIndices\" numelements=\"2\">0 3</hkparam>"));
    assert!(root.contains("<hkparam name=\"bones\">#0210</hkparam>"));
    assert!(root.contains("<hkparam name=\"poseMatchingBone0\">0</hkparam>"));
    assert!(root.contains("<hkparam name=\"poseMatchingBone1\">3</hkparam>"));
    assert!(root.contains("<hkparam name=\"poseMatchingBone2\">-1</hkparam>"));

    let mut forged = manifest.clone();
    let RagdollDisposition::SourceOwned { receipt, .. } = &mut forged.ragdoll else {
        unreachable!();
    };
    receipt
        .powered_ragdoll
        .as_mut()
        .unwrap()
        .proven_inert_source_fields[0]
        .canonical_json_blake3 = "00".repeat(32);
    assert!(error_codes(forged.validate()).contains(&"powered_ragdoll_inert_proof"));

    let RagdollDisposition::SourceOwned { receipt, .. } = &mut manifest.ragdoll else {
        unreachable!();
    };
    receipt
        .powered_ragdoll
        .as_mut()
        .unwrap()
        .proven_inert_source_fields
        .pop();
    assert!(manifest.validate().is_err());
}

fn powered_ragdoll_inert_receipt(
    field: PoweredRagdollSourceField,
) -> PoweredRagdollInertFieldReceipt {
    let canonical_json = serde_json::to_string(&serde_json::json!({
        "field": field,
        "disposition": "runtime_inert",
        "evidence_blake3": blake3::hash(format!("fixture-proof-{field:?}").as_bytes())
            .to_hex()
            .to_string(),
    }))
    .unwrap();
    PoweredRagdollInertFieldReceipt {
        field,
        proof_schema: POWERED_RAGDOLL_INERT_PROOF_SCHEMA.to_string(),
        canonical_json_blake3: blake3::hash(canonical_json.as_bytes()).to_hex().to_string(),
        canonical_json,
    }
}

#[test]
fn no_ragdoll_is_an_explicit_roundtrip_disposition() {
    let (mut manifest, graph) = wolf_mvp();
    manifest.controller = CreatureControllerDecl {
        collision_filter_info: 0x1234,
        rigid_body_type: 7,
        model_up_ms: [0.0, 1.0, 0.0, 0.0],
        model_forward_ms: [0.0, 0.0, 1.0, 0.0],
        model_right_ms: [1.0, 0.0, 0.0, 0.0],
        model_scale: 1.25,
    };
    assert!(matches!(
        manifest.ragdoll,
        RagdollDisposition::NoRagdoll {
            reason: NoRagdollReason::SourceHasNoRagdoll
        }
    ));
    let scaffold = emit_mvp_scaffold(&manifest, &graph).unwrap();
    let character = scaffold.artifact(&manifest.paths.character).unwrap();
    assert!(character.xml.contains("<hkparam name=\"ragdollName\"/>"));
    for exact in [
        "<hkparam name=\"collisionFilterInfo\">4660</hkparam>",
        "<hkparam name=\"type\">7</hkparam>",
        "<hkparam name=\"modelUpMS\">(0 1 0 0)</hkparam>",
        "<hkparam name=\"modelForwardMS\">(0 0 1 0)</hkparam>",
        "<hkparam name=\"modelRightMS\">(1 0 0 0)</hkparam>",
        "<hkparam name=\"scale\">1.25</hkparam>",
    ] {
        assert!(character.xml.contains(exact), "missing {exact}");
    }
    assert!(
        !scaffold
            .artifact(&manifest.paths.root_behavior)
            .unwrap()
            .xml
            .contains("hkbPoweredRagdollControlsModifier")
    );
}

#[test]
fn wolf_mvp_emits_exact_five_state_graph_without_annotation_duplication() {
    let (manifest, graph) = wolf_mvp();
    let first = emit_mvp_scaffold(&manifest, &graph).unwrap();
    let second = emit_mvp_scaffold(&manifest, &graph).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        manifest.paths.project,
        "Actors\\B21_SkyrimWolf\\B21_SkyrimWolfProject.hkx"
    );
    assert_eq!(
        manifest.animation_skeleton.path,
        "Actors\\B21_SkyrimWolf\\CharacterAssets\\Skeleton.hkx"
    );

    let project = first.artifact(&manifest.paths.project).unwrap();
    for runtime_path in manifest.clips.iter().map(|clip| clip.path.as_str()) {
        let internal_path = runtime_path
            .strip_prefix("Actors\\B21_SkyrimWolf\\")
            .unwrap();
        assert!(project.xml.contains(internal_path));
        assert!(!project.xml.contains(runtime_path));
    }

    let core = first.artifact(&manifest.paths.core_behavior).unwrap();
    for (state_name, file_name, mode) in [
        ("Idle", "Animations\\mt_idle_wolf.hkx", "MODE_LOOPING"),
        (
            "WalkForward",
            "Animations\\walkforward_wolf.hkx",
            "MODE_LOOPING",
        ),
        (
            "TurnLeft90",
            "Animations\\turncannedl90_wolf.hkx",
            "MODE_SINGLE_PLAY",
        ),
        (
            "TurnRight90",
            "Animations\\turncannedr90_wolf.hkx",
            "MODE_SINGLE_PLAY",
        ),
        ("Attack1", "Animations\\attack1.hkx", "MODE_SINGLE_PLAY"),
    ] {
        let clip_object = object_containing(
            &core.xml,
            &format!("<hkparam name=\"animationName\">{file_name}</hkparam>"),
        );
        assert!(clip_object.contains(&format!("<hkparam name=\"name\">{state_name}</hkparam>")));
        assert!(clip_object.contains(&format!("<hkparam name=\"mode\">{mode}</hkparam>")));
        assert!(clip_object.contains("<hkparam name=\"triggers\">null</hkparam>"));
    }
    for trigger in [
        "Idle",
        "startWalk",
        "TurnLeft90",
        "TurnRight90",
        "meleeWolfAttack1",
    ] {
        assert!(
            core.xml
                .contains(&format!("<hkcstring>{trigger}</hkcstring>"))
        );
    }
    assert_eq!(
        core.xml
            .matches("<hkparam name=\"flags\">16384</hkparam>")
            .count(),
        3
    );
    assert!(
        core.xml
            .contains("class=\"hkbStateMachine\" signature=\"0xa5896bcf\"")
    );
    assert!(!core.xml.contains("startAnimationDriven"));
    assert!(!core.xml.contains("weaponSwing"));
    assert!(!core.xml.contains("preHitFrame"));
    assert!(!core.xml.contains("HitFrame"));

    let root = first.artifact(&manifest.paths.root_behavior).unwrap();
    assert!(root.xml.contains("class=\"BSBehaviorGraphSwapGenerator\""));
    for trigger in [
        "Idle",
        "startWalk",
        "TurnLeft90",
        "TurnRight90",
        "meleeWolfAttack1",
    ] {
        assert!(
            root.xml
                .contains(&format!("<hkcstring>{trigger}</hkcstring>"))
        );
    }

    for artifact in &first.artifacts {
        assert!(artifact.runtime_path.ends_with(".hkx"));
        assert!(!artifact.xml.to_ascii_lowercase().contains(".hkt"));
        assert!(!artifact.xml.contains(">.xml<"));
    }
}

#[test]
fn wolf_mvp_packs_four_readable_fo4_hkx_files() {
    let (manifest, graph) = wolf_mvp();
    let output = tempfile::tempdir().unwrap();
    let report = pack_mvp_scaffold(&manifest, &graph, output.path()).unwrap();

    assert_eq!(report.artifacts.len(), 4);
    assert_eq!(report.runtime_manifest.animation_clips.len(), 5);
    assert!(
        report
            .runtime_manifest
            .cross_file_paths()
            .into_iter()
            .all(|path| path.to_ascii_lowercase().ends_with(".hkx"))
    );
    for artifact in &report.artifacts {
        let bytes = fs::read(&artifact.hkx_path).unwrap();
        let packed = havok_native::hkx::HkxFile::read(&bytes).unwrap();
        assert_eq!(packed.class_version(), 11);
        assert_eq!(packed.contents_version(), "hk_2014.1.0-r1");
        assert_eq!(packed.packfile().header.pointer_size, 8);
        assert!(
            validate_fo4_havok_xml_signatures(&reread_packed_xml(&report, &artifact.runtime_path))
                .is_ok()
        );
    }

    assert!(
        report
            .runtime_manifest
            .cross_file_paths()
            .into_iter()
            .all(|path| path.starts_with("Actors\\B21_SkyrimWolf\\"))
    );
    let project_xml = reread_packed_xml(&report, &manifest.paths.project);
    assert!(project_xml.contains("Characters\\B21_SkyrimWolfCharacter.hkx"));
    assert!(project_xml.contains("Behaviors\\B21_SkyrimWolfRootBehavior.hkx"));
    assert!(project_xml.contains("Animations\\mt_idle_wolf.hkx"));
    assert!(!project_xml.contains("Actors\\B21_SkyrimWolf\\"));

    let character_xml = reread_packed_xml(&report, &manifest.paths.character);
    assert!(character_xml.contains("CharacterAssets\\Skeleton.hkx"));
    assert!(character_xml.contains("Behaviors\\B21_SkyrimWolfRootBehavior.hkx"));
    assert!(!character_xml.contains("Actors\\B21_SkyrimWolf\\"));

    let core_xml = reread_packed_xml(&report, &manifest.paths.core_behavior);
    assert!(core_xml.contains("Animations\\mt_idle_wolf.hkx"));
    assert!(core_xml.contains("Animations\\attack1.hkx"));
    assert!(!core_xml.contains("startAnimationDriven"));
    assert!(!core_xml.contains("bAnimationDriven"));
    assert!(!core_xml.contains("Actors\\B21_SkyrimWolf\\"));
}

#[test]
fn same_mvp_api_accepts_source_neutral_gecko_roles() {
    let (manifest, graph) = gecko_mvp();
    let motion = gecko_motion();
    let scaffold = emit_mvp_scaffold_with_motion(&manifest, &graph, &motion).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    let root = scaffold.artifact(&manifest.paths.root_behavior).unwrap();

    assert!(core.xml.contains("Animations\\mtidle.hkx"));
    assert!(core.xml.contains("Animations\\attackbite.hkx"));
    assert!(core.xml.contains("meleeGeckoBite"));
    assert!(
        core.xml
            .contains("<hkcstring>startAnimationDriven</hkcstring>")
    );
    assert!(core.xml.contains("<hkcstring>bAnimationDriven</hkcstring>"));
    assert!(
        root.xml
            .contains("<hkcstring>startAnimationDriven</hkcstring>")
    );
    assert!(root.xml.contains("<hkcstring>bAnimationDriven</hkcstring>"));
    assert_eq!(
        core.xml.matches("class=\"hkbModifierGenerator\"").count(),
        2
    );
    assert_eq!(core.xml.matches("class=\"BSIsActiveModifier\"").count(), 2);
    let idle_state = object_containing(
        &core.xml,
        "<hkparam name=\"name\">Idle</hkparam><hkparam name=\"stateId\">0</hkparam>",
    );
    assert!(idle_state.contains("<hkparam name=\"enterNotifyEvents\">null</hkparam>"));
    assert!(idle_state.contains("<hkparam name=\"generator\">#0100</hkparam>"));
    let walk_state = object_containing(
        &core.xml,
        "<hkparam name=\"name\">WalkForward</hkparam><hkparam name=\"stateId\">1</hkparam>",
    );
    assert!(walk_state.contains("<hkparam name=\"enterNotifyEvents\">#0131</hkparam>"));
    assert!(walk_state.contains("<hkparam name=\"exitNotifyEvents\">null</hkparam>"));
    assert!(walk_state.contains("<hkparam name=\"generator\">#0161</hkparam>"));
    let attack_state = object_containing(
        &core.xml,
        "<hkparam name=\"name\">Attack1</hkparam><hkparam name=\"stateId\">4</hkparam>",
    );
    assert!(attack_state.contains("<hkparam name=\"enterNotifyEvents\">#0134</hkparam>"));
    assert!(attack_state.contains("<hkparam name=\"exitNotifyEvents\">null</hkparam>"));
    assert!(attack_state.contains("<hkparam name=\"generator\">#0164</hkparam>"));
    assert!(!core.xml.contains("stopAnimationDriven"));
    assert!(!core.xml.contains("endAnimationDriven"));
    assert!(!core.xml.contains("Skyrim"));
    assert_eq!(manifest.animation_skeleton.bones.len(), 87);

    let output = tempfile::tempdir().unwrap();
    let report = pack_mvp_scaffold_with_motion(&manifest, &graph, &motion, output.path()).unwrap();
    assert_eq!(report.artifacts.len(), 4);
    assert!(
        report
            .runtime_manifest
            .cross_file_paths()
            .into_iter()
            .all(|path| path.starts_with("Actors\\B21_Gecko\\"))
    );

    let project_xml = reread_packed_xml(&report, &manifest.paths.project);
    assert!(project_xml.contains("Characters\\B21_GeckoCharacter.hkx"));
    assert!(project_xml.contains("Animations\\mtidle.hkx"));
    assert!(!project_xml.contains("Actors\\B21_Gecko\\"));

    let character_xml = reread_packed_xml(&report, &manifest.paths.character);
    assert!(character_xml.contains("CharacterAssets\\Skeleton.hkx"));
    assert!(character_xml.contains("Behaviors\\B21_GeckoRootBehavior.hkx"));
    assert!(!character_xml.contains("Actors\\B21_Gecko\\"));

    let core_xml = reread_packed_xml(&report, &manifest.paths.core_behavior);
    assert!(core_xml.contains("Animations\\mtidle.hkx"));
    assert!(core_xml.contains("Animations\\attackbite.hkx"));
    assert!(core_xml.contains("startAnimationDriven"));
    assert!(core_xml.contains("bAnimationDriven"));
    assert_eq!(
        core_xml.matches("class=\"hkbModifierGenerator\"").count(),
        2
    );
    assert!(validate_fo4_havok_xml_signatures(&core_xml).is_ok());
    assert!(!core_xml.contains("Actors\\B21_Gecko\\"));
}

#[test]
fn gecko_sparse_84_track_bindings_validate_against_87_bones() {
    let (manifest, graph) = gecko_mvp();
    assert!(manifest.validate_mvp(&graph).is_ok());
    for clip in &manifest.clips {
        assert_eq!(clip.binding.declared_transform_tracks, 84);
        assert_eq!(clip.binding.transform_track_to_bone_indices.len(), 84);
        assert!(!clip.binding.transform_track_to_bone_indices.contains(&0));
        assert_eq!(clip.binding.transform_track_to_bone_indices[0], 3);
        assert_eq!(clip.binding.transform_track_to_bone_indices[83], 86);
    }
}

#[test]
fn legacy_bone_names_preserve_edge_whitespace_but_reject_blank_names() {
    let (mut manifest, graph) = wolf_mvp();
    manifest.animation_skeleton.bones[3].name = " NPC Head [Head]".to_string();
    assert!(manifest.validate_mvp(&graph).is_ok());

    manifest.animation_skeleton.bones[3].name = "   ".to_string();
    let codes = error_codes(manifest.validate_mvp(&graph));
    assert!(codes.contains(&"invalid_name"));
}

#[test]
fn gecko_sparse_binding_rejects_duplicate_and_out_of_range_indices() {
    let (mut manifest, graph) = gecko_mvp();
    manifest.clips[0].binding.transform_track_to_bone_indices[1] = 3;
    manifest.clips[0].binding.transform_track_to_bone_indices[83] = 87;
    let codes = error_codes(manifest.validate_mvp(&graph));
    assert!(codes.contains(&"duplicate_clip_bone"));
    assert!(codes.contains(&"clip_bone_index"));
}

#[test]
fn mvp_role_validation_fails_closed_on_modes_and_events() {
    let (mut manifest, graph) = wolf_mvp();
    manifest.clips[1].looping = false;
    manifest.core.events[4].usage = EventUsage::Generic;
    manifest.root = manifest.core.clone();
    let codes = error_codes(manifest.validate_mvp(&graph));
    assert!(codes.contains(&"mvp_clip_mode"));
    assert!(codes.contains(&"mvp_event_usage"));
}

#[test]
fn motion_policy_requires_extracted_planar_reference_frames_in_both_directions() {
    let (manifest, graph) = gecko_mvp();
    let mut motion = gecko_motion();
    motion.walk_forward.extracted_planar_reference_frames = 0;
    motion.turn_left_90.extracted_planar_reference_frames = 1;
    let codes = error_codes(manifest.validate_mvp_motion(&graph, &motion));
    assert!(codes.contains(&"animation_driven_without_extracted_motion"));
    assert!(codes.contains(&"extracted_motion_without_animation_driven"));
}

#[test]
fn capability_graph_supports_optional_turns_and_multiple_projectile_attacks() {
    let (manifest, graph) = ranged_capability();
    assert!(manifest.validate_capability_graph(&graph).is_ok());
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    let root = scaffold.artifact(&manifest.paths.root_behavior).unwrap();
    for state in ["Rest", "Advance", "RotatePort", "SpitBolt", "SpitAcid"] {
        assert!(
            core.xml
                .contains(&format!("<hkparam name=\"name\">{state}</hkparam>"))
        );
    }
    for event in ["Idle", "beginStride", "rotatePort", "spitBolt", "spitAcid"] {
        assert!(
            core.xml
                .contains(&format!("<hkcstring>{event}</hkcstring>"))
        );
        assert!(
            root.xml
                .contains(&format!("<hkcstring>{event}</hkcstring>"))
        );
    }
    assert!(
        !core
            .xml
            .contains("<hkparam name=\"name\">TurnRight90</hkparam>")
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"states\" numelements=\"5\">")
    );
    assert!(
        scaffold
            .artifacts
            .iter()
            .all(|artifact| validate_fo4_havok_xml_signatures(&artifact.xml).is_ok())
    );

    let output = tempfile::tempdir().unwrap();
    let report = pack_capability_scaffold(&manifest, &graph, output.path()).unwrap();
    assert_eq!(report.artifacts.len(), 4);
    for artifact in &report.artifacts {
        let roundtrip = reread_packed_xml(&report, &artifact.runtime_path);
        assert!(validate_fo4_havok_xml_signatures(&roundtrip).is_ok());
    }
}

#[test]
fn capability_trigger_aliases_emit_same_role_transitions_and_reject_cross_role_aliases() {
    let (manifest, mut graph) = ranged_capability();
    graph
        .explicit_events
        .extend(["attackEnterA", "attackEnterB"].map(generic_event).to_vec());
    graph
        .roles
        .iter_mut()
        .find(|role| role.state_name == "SpitBolt")
        .unwrap()
        .trigger_aliases = vec!["attackEnterA".to_string(), "attackEnterB".to_string()];

    manifest.validate_capability_graph(&graph).unwrap();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    let facts = validate_havok_xml(&core.xml).unwrap();
    let idle_transitions = object_containing(&core.xml, "name=\"#0110\"");
    for alias in ["attackEnterA", "attackEnterB"] {
        let event_id = facts
            .event_names
            .iter()
            .position(|event| event == alias)
            .unwrap();
        let transition = format!(
            "<hkparam name=\"eventId\">{event_id}</hkparam><hkparam name=\"toStateId\">3</hkparam>"
        );
        assert_eq!(idle_transitions.matches(&transition).count(), 1);
    }
    assert_capability_pack_roundtrip(&manifest, &graph);

    let mut cross_role = graph.clone();
    cross_role
        .roles
        .iter_mut()
        .find(|role| role.state_name == "SpitAcid")
        .unwrap()
        .trigger_aliases = vec!["attackEnterA".to_string()];
    assert!(
        error_codes(manifest.validate_capability_graph(&cross_role))
            .contains(&"duplicate_role_event")
    );

    let mut unsorted = graph;
    unsorted
        .roles
        .iter_mut()
        .find(|role| role.state_name == "SpitBolt")
        .unwrap()
        .trigger_aliases = vec!["attackEnterB".to_string(), "attackEnterA".to_string()];
    assert!(
        error_codes(manifest.validate_capability_graph(&unsorted))
            .contains(&"capability_trigger_alias_order")
    );
}

#[test]
fn passive_ground_emits_only_evidenced_locomotion_states() {
    let (manifest, graph) = passive_ground_capability();
    assert!(manifest.validate_capability_graph(&graph).is_ok());
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();

    assert!(core.xml.contains("PassiveGroundLocomotionSM"));
    assert!(core.xml.contains("<hkparam name=\"name\">Rest</hkparam>"));
    assert!(
        core.xml
            .contains("<hkparam name=\"name\">Advance</hkparam>")
    );
    assert!(
        !core
            .xml
            .contains("<hkparam name=\"name\">Attack1</hkparam>")
    );
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn passive_ground_rejects_any_attack_role() {
    let (manifest, mut graph) = passive_ground_capability();
    graph.roles.push(capability_role(
        CreatureClipRole::MeleeAttack,
        "Attack1",
        "attack1",
        Some("meleeWolfAttack1"),
    ));
    graph.explicit_events.push(EventDecl {
        name: "meleeWolfAttack1".to_string(),
        usage: EventUsage::MeleeAttack,
        flags: 0,
    });
    let codes = error_codes(manifest.validate_capability_graph(&graph));
    assert!(codes.contains(&"capability_role_not_allowed"));
}

#[test]
fn ground_melee_ranged_requires_and_emits_both_attack_capabilities() {
    let (manifest, graph) = multimodal_capability(CreatureGraphTemplate::GroundMeleeRanged);
    assert!(manifest.validate_capability_graph(&graph).is_ok());
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();

    assert!(core.xml.contains("CapabilityLocomotionCombatSM"));
    assert!(core.xml.contains("meleeMultiAttack"));
    assert!(core.xml.contains("fireProjectile"));
    assert_capability_pack_roundtrip(&manifest, &graph);

    let mut missing_projectile = graph;
    missing_projectile
        .roles
        .retain(|role| role.role != CreatureClipRole::ProjectileAttack);
    assert!(
        error_codes(manifest.validate_capability_graph(&missing_projectile))
            .contains(&"missing_capability_role")
    );
}

#[test]
fn candidate_bindings_share_one_attack_state_without_failing_role_cardinality() {
    let (manifest, mut graph) = multimodal_capability(CreatureGraphTemplate::GroundMeleeRanged);
    graph
        .roles
        .retain(|role| role.role != CreatureClipRole::ProjectileAttack);
    let shared_event = "attackStart_Attack1";
    graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::MeleeAttack)
        .unwrap()
        .trigger_event = Some(shared_event.to_string());
    graph
        .explicit_events
        .retain(|event| !matches!(event.name.as_str(), "meleeMultiAttack" | "fireProjectile"));
    graph.explicit_events.push(generic_event(shared_event));
    graph.candidate_attack_bindings = vec![
        CapabilityCandidateAttackBinding {
            source_identity: SourceCreatureIdentity {
                namespace: "skyrimse".to_string(),
                plugin: "Skyrim.esm".to_string(),
                local_form_id: 0x100,
            },
            event: shared_event.to_string(),
            role: CreatureClipRole::MeleeAttack,
        },
        CapabilityCandidateAttackBinding {
            source_identity: SourceCreatureIdentity {
                namespace: "skyrimse".to_string(),
                plugin: "Skyrim.esm".to_string(),
                local_form_id: 0x101,
            },
            event: shared_event.to_string(),
            role: CreatureClipRole::ProjectileAttack,
        },
    ];

    manifest.validate_capability_graph(&graph).unwrap();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    assert_eq!(core.xml.matches(shared_event).count(), 1);
    assert_eq!(
        core.xml
            .matches("<hkparam name=\"name\">MeleeAttack</hkparam>")
            .count(),
        2
    );
    assert!(!core.xml.contains("ProjectileAttack"));
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn manual_selector_retains_ordered_clips_and_exact_dynamic_index_binding() {
    let (mut manifest, mut graph) = ranged_capability();
    let mut alternate = manifest
        .clips
        .iter()
        .find(|clip| clip.name == "mt_idle_wolf")
        .unwrap()
        .clone();
    alternate.name = "idle_combat".to_string();
    alternate.path = "Actors\\B21_SkyrimWolf\\Animations\\idle_combat.hkx".to_string();
    manifest.clips.push(alternate);
    let selector_variable = VariableDecl {
        name: "iIdleSelector".to_string(),
        variable_type: VariableType::Int32,
        initial_value: VariableValue::Int32(0),
    };
    manifest.root.variables.push(selector_variable.clone());
    manifest.core.variables.push(selector_variable);
    graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::Idle)
        .unwrap()
        .generator = CapabilityRoleGenerator::ManualSelector {
        children: vec!["mt_idle_wolf".to_string(), "idle_combat".to_string()],
        selected_generator_index: 0,
        selected_index_can_change_after_activate: true,
        selected_index_variable: Some("iIdleSelector".to_string()),
    };

    manifest.validate_capability_graph(&graph).unwrap();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    assert!(
        core.xml
            .contains("class=\"hkbManualSelectorGenerator\" signature=\"0xeed8d5cd\"")
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"memberPath\">selectedGeneratorIndex</hkparam>")
    );
    assert!(core.xml.contains("Animations\\mt_idle_wolf.hkx"));
    assert!(core.xml.contains("Animations\\idle_combat.hkx"));
    assert_capability_pack_roundtrip(&manifest, &graph);

    let mut unbound = graph;
    if let CapabilityRoleGenerator::ManualSelector {
        selected_index_variable,
        ..
    } = &mut unbound
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::Idle)
        .unwrap()
        .generator
    {
        *selected_index_variable = None;
    }
    assert!(
        error_codes(manifest.validate_capability_graph(&unbound))
            .contains(&"manual_selector_dynamic_control")
    );
}

#[test]
fn locomotion_blender_retains_exact_children_weights_and_parameter_binding() {
    let (mut manifest, mut graph) = ranged_capability();
    let mut run = manifest
        .clips
        .iter()
        .find(|clip| clip.name == "walkforward_wolf")
        .unwrap()
        .clone();
    run.name = "runforward_wolf".to_string();
    run.path = "Actors\\B21_SkyrimWolf\\Animations\\runforward_wolf.hkx".to_string();
    manifest.clips.push(run);
    let blend_variable = VariableDecl {
        name: "fLocomotionBlend".to_string(),
        variable_type: VariableType::Real,
        initial_value: VariableValue::Real(0.25),
    };
    manifest.root.variables.push(blend_variable.clone());
    manifest.core.variables.push(blend_variable);
    graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::GroundForward)
        .unwrap()
        .generator = CapabilityRoleGenerator::Blender {
        children: vec![
            CapabilityBlenderChild {
                clip_name: "walkforward_wolf".to_string(),
                weight_bits: 0.0f32.to_bits(),
                world_from_model_weight_bits: 1.0f32.to_bits(),
            },
            CapabilityBlenderChild {
                clip_name: "runforward_wolf".to_string(),
                weight_bits: 1.0f32.to_bits(),
                world_from_model_weight_bits: 1.0f32.to_bits(),
            },
        ],
        reference_pose_weight_threshold_bits: 0.0f32.to_bits(),
        blend_parameter_bits: 0.25f32.to_bits(),
        min_cyclic_blend_parameter_bits: 0.0f32.to_bits(),
        max_cyclic_blend_parameter_bits: 1.0f32.to_bits(),
        index_of_sync_master_child: 0,
        flags: 0x11,
        subtract_last_child: false,
        blend_parameter_variable: Some("fLocomotionBlend".to_string()),
    };

    manifest.validate_capability_graph(&graph).unwrap();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    assert!(
        core.xml
            .contains("class=\"hkbBlenderGenerator\" signature=\"0xce45c088\"")
    );
    assert_eq!(
        core.xml
            .matches("class=\"hkbBlenderGeneratorChild\"")
            .count(),
        2
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"memberPath\">blendParameter</hkparam>")
    );
    assert!(core.xml.contains("Animations\\walkforward_wolf.hkx"));
    assert!(core.xml.contains("Animations\\runforward_wolf.hkx"));
    assert_capability_pack_roundtrip(&manifest, &graph);
}

fn recursive_source_object(
    object_index: u32,
    class_name: &str,
    name: &str,
) -> CapabilityGeneratorSourceObject {
    CapabilityGeneratorSourceObject {
        behavior_path: "Actors\\B21_SkyrimWolf\\Behaviors\\WolfBehavior.hkx".to_string(),
        object_index,
        class_name: class_name.to_string(),
        name: name.to_string(),
    }
}

fn recursive_clip(object_index: u32, clip_name: &str) -> CapabilityGeneratorNode {
    CapabilityGeneratorNode::Clip {
        source: recursive_source_object(object_index, "hkbClipGenerator", clip_name),
        clip_name: clip_name.to_string(),
    }
}

fn no_generator_event() -> CapabilityGeneratorEventRef {
    CapabilityGeneratorEventRef {
        source_id: -1,
        event_name: None,
    }
}

#[test]
fn recursive_locomotion_blender_preserves_nested_children_and_pack_roundtrip() {
    let (mut manifest, mut graph) = ranged_capability();
    let base_clip = manifest
        .clips
        .iter()
        .find(|clip| clip.name == "walkforward_wolf")
        .unwrap()
        .clone();
    let clip_names = [
        "walkforward_wolf",
        "walkleft_wolf",
        "walkright_wolf",
        "runforward_wolf",
        "runleft_wolf",
        "runright_wolf",
    ];
    for clip_name in clip_names.iter().skip(1) {
        let mut clip = base_clip.clone();
        clip.name = (*clip_name).to_string();
        clip.path = format!("Actors\\B21_SkyrimWolf\\Animations\\{clip_name}.hkx");
        manifest.clips.push(clip);
    }
    let variable_index = manifest.core.variables.len();
    let speed = VariableDecl {
        name: "SpeedSampled".to_string(),
        variable_type: VariableType::Real,
        initial_value: VariableValue::Real(0.0),
    };
    manifest.root.variables.push(speed.clone());
    manifest.core.variables.push(speed);
    let direction_blender = |object_index, names: &[&str]| CapabilityGeneratorNode::Blender {
        source: recursive_source_object(
            object_index,
            "hkbBlenderGenerator",
            &format!("DirectionBlend{object_index}"),
        ),
        bindings: Vec::new(),
        children: names
            .iter()
            .enumerate()
            .map(|(index, name)| CapabilityGeneratorBlenderChild {
                generator: recursive_clip(object_index + 1 + index as u32, name),
                bone_weights: None,
                weight_bits: (index as f32 * 0.5).to_bits(),
                world_from_model_weight_bits: 1.0f32.to_bits(),
            })
            .collect(),
        reference_pose_weight_threshold_bits: 0.0f32.to_bits(),
        blend_parameter_bits: 0.5f32.to_bits(),
        min_cyclic_blend_parameter_bits: 0.0f32.to_bits(),
        max_cyclic_blend_parameter_bits: 1.0f32.to_bits(),
        index_of_sync_master_child: -1,
        flags: 0x11,
        subtract_last_child: false,
    };
    let root = CapabilityGeneratorNode::Blender {
        source: recursive_source_object(100, "hkbBlenderGenerator", "ForwardWalkBlend"),
        bindings: vec![CapabilityGeneratorVariableBinding {
            member_path: "blendParameter".to_string(),
            variable_index: variable_index as i32,
            variable_name: "SpeedSampled".to_string(),
            bit_index: -1,
            binding_type: CapabilityVariableBindingType::Variable,
            source_variable_type: CapabilitySourceVariableType::Real,
            initial_word_value: Some(0.0f32.to_bits()),
        }],
        children: vec![
            CapabilityGeneratorBlenderChild {
                generator: direction_blender(
                    200,
                    &[
                        "walkleft_wolf",
                        "walkforward_wolf",
                        "walkright_wolf",
                        "runforward_wolf",
                    ],
                ),
                bone_weights: None,
                weight_bits: 0.0f32.to_bits(),
                world_from_model_weight_bits: 1.0f32.to_bits(),
            },
            CapabilityGeneratorBlenderChild {
                generator: direction_blender(
                    300,
                    &["runleft_wolf", "runforward_wolf", "runright_wolf"],
                ),
                bone_weights: None,
                weight_bits: 1.0f32.to_bits(),
                world_from_model_weight_bits: 1.0f32.to_bits(),
            },
        ],
        reference_pose_weight_threshold_bits: 0.0f32.to_bits(),
        blend_parameter_bits: 1.0f32.to_bits(),
        min_cyclic_blend_parameter_bits: 0.0f32.to_bits(),
        max_cyclic_blend_parameter_bits: 1.0f32.to_bits(),
        index_of_sync_master_child: -1,
        flags: 0x11,
        subtract_last_child: false,
    };
    graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::GroundForward)
        .unwrap()
        .generator = CapabilityRoleGenerator::Tree { root };

    manifest.validate_capability_graph(&graph).unwrap();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    assert_eq!(core.xml.matches("class=\"hkbBlenderGenerator\"").count(), 3);
    assert_eq!(
        core.xml
            .matches("class=\"hkbBlenderGeneratorChild\"")
            .count(),
        9
    );
    assert_eq!(
        core.xml.matches("Animations\\runforward_wolf.hkx").count(),
        2
    );
    for clip_name in clip_names {
        assert!(core.xml.contains(&format!("Animations\\{clip_name}.hkx")));
    }
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn recursive_modifier_generator_preserves_damping_and_expressions() {
    let (mut manifest, mut graph) = ranged_capability();
    let speed_index = manifest.core.variables.len();
    let speed = VariableDecl {
        name: "Speed".to_string(),
        variable_type: VariableType::Real,
        initial_value: VariableValue::Real(0.0),
    };
    manifest.root.variables.push(speed.clone());
    manifest.core.variables.push(speed);
    let damped_speed_index = manifest.core.variables.len();
    let damped_speed = VariableDecl {
        name: "DampedSpeed".to_string(),
        variable_type: VariableType::Real,
        initial_value: VariableValue::Real(1.0),
    };
    manifest.root.variables.push(damped_speed.clone());
    manifest.core.variables.push(damped_speed);

    let variable =
        |member_path: &str, variable_index: usize, variable_name: &str, initial_word_value: u32| {
            CapabilityGeneratorVariableBinding {
                member_path: member_path.to_string(),
                variable_index: variable_index as i32,
                variable_name: variable_name.to_string(),
                bit_index: -1,
                binding_type: CapabilityVariableBindingType::Variable,
                source_variable_type: CapabilitySourceVariableType::Real,
                initial_word_value: Some(initial_word_value),
            }
        };
    let root = CapabilityGeneratorNode::ModifierGenerator {
        source: recursive_source_object(600, "hkbModifierGenerator", "ForwardLocomotion_MG"),
        bindings: Vec::new(),
        user_data: 1,
        modifier: Box::new(CapabilityModifierNode::List {
            source: recursive_source_object(
                601,
                "hkbModifierList",
                "ForwardLocomotionModifierList",
            ),
            bindings: Vec::new(),
            user_data: 1,
            enable: false,
            modifiers: vec![
                CapabilityModifierNode::Damping {
                    source: recursive_source_object(
                        602,
                        "hkbDampingModifier",
                        "TurnDeltaDampingModifier",
                    ),
                    bindings: vec![
                        variable("rawValue", speed_index, "Speed", 0.0f32.to_bits()),
                        variable(
                            "dampedValue",
                            damped_speed_index,
                            "DampedSpeed",
                            1.0f32.to_bits(),
                        ),
                    ],
                    user_data: 1,
                    enable: true,
                    k_p_bits: (-0.1f32).to_bits(),
                    k_i_bits: 1,
                    k_d_bits: 0.0f32.to_bits(),
                    enable_scalar_damping: false,
                    enable_vector_damping: false,
                    raw_value_bits: 0.0f32.to_bits(),
                    damped_value_bits: 0.0f32.to_bits(),
                    raw_vector_bits: [0; 4],
                    damped_vector_bits: [0; 4],
                    vector_error_sum_bits: [0; 4],
                    vector_previous_error_bits: [0; 4],
                    error_sum_bits: 0.0f32.to_bits(),
                    previous_error_bits: 0.0f32.to_bits(),
                },
                CapabilityModifierNode::EvaluateExpression {
                    source: recursive_source_object(
                        603,
                        "hkbEvaluateExpressionModifier",
                        "ForwardLocomotion_EEM",
                    ),
                    bindings: Vec::new(),
                    user_data: 2,
                    enable: false,
                    expressions: CapabilityGeneratorExpressionArray {
                        source: CapabilityGeneratorAnonymousSourceObject {
                            behavior_path: "actors\\wolf\\wolfbehavior.hkx".to_string(),
                            object_index: 604,
                            class_name: "hkbExpressionDataArray".to_string(),
                        },
                        expressions: vec![CapabilityGeneratorExpression {
                            expression: "runStart if (Speed > 420)".to_string(),
                            referenced_variables: vec![
                                CapabilityGeneratorExpressionVariableReference {
                                    variable_index: speed_index as i32,
                                    variable_name: "Speed".to_string(),
                                    source_variable_type: CapabilitySourceVariableType::Real,
                                    initial_word_value: Some(0.0f32.to_bits()),
                                },
                            ],
                            assignment_variable_index: -1,
                            assignment_event_index: -1,
                            event_mode: 2,
                        }],
                    },
                },
            ],
        }),
        generator: Box::new(recursive_clip(605, "walkforward_wolf")),
    };
    graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::GroundForward)
        .unwrap()
        .generator = CapabilityRoleGenerator::Tree { root };

    manifest.validate_capability_graph(&graph).unwrap();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    for (class_name, signature) in [
        ("hkbModifierGenerator", "0xc499fc9e"),
        ("hkbModifierList", "0x0ded564c"),
        ("hkbDampingModifier", "0x68a51d05"),
        ("hkbEvaluateExpressionModifier", "0x4a3ac449"),
        ("hkbExpressionDataArray", "0x1ebfc6d7"),
    ] {
        assert!(
            core.xml
                .contains(&format!("class=\"{class_name}\" signature=\"{signature}\""))
        );
    }
    assert!(core.xml.contains("runStart if (Speed &gt; 420)"));
    assert!(
        core.xml
            .contains("<hkparam name=\"memberPath\">dampedValue</hkparam>")
    );
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn recursive_state_machine_preserves_start_binding_and_blending_transition_effect() {
    let (mut manifest, mut graph) = ranged_capability();
    let alternate_name = "walkforward_fast_wolf";
    let mut alternate = manifest
        .clips
        .iter()
        .find(|clip| clip.name == "walkforward_wolf")
        .unwrap()
        .clone();
    alternate.name = alternate_name.to_string();
    alternate.path = format!("Actors\\B21_SkyrimWolf\\Animations\\{alternate_name}.hkx");
    manifest.clips.push(alternate);

    let state_variable_index = manifest.core.variables.len();
    let state_variable = VariableDecl {
        name: "iMovementSpeed".to_string(),
        variable_type: VariableType::Int32,
        initial_value: VariableValue::Int32(0),
    };
    manifest.root.variables.push(state_variable.clone());
    manifest.core.variables.push(state_variable);
    let duration_variable_index = manifest.core.variables.len();
    let duration_variable = VariableDecl {
        name: "BlendDuration".to_string(),
        variable_type: VariableType::Real,
        initial_value: VariableValue::Real(0.2),
    };
    manifest.root.variables.push(duration_variable.clone());
    manifest.core.variables.push(duration_variable);

    let transition_effect = CapabilityGeneratorBlendingTransitionEffect {
        source: recursive_source_object(706, "hkbBlendingTransitionEffect", "DefaultBlend"),
        bindings: vec![CapabilityGeneratorVariableBinding {
            member_path: "duration".to_string(),
            variable_index: duration_variable_index as i32,
            variable_name: "BlendDuration".to_string(),
            bit_index: -1,
            binding_type: CapabilityVariableBindingType::Variable,
            source_variable_type: CapabilitySourceVariableType::Real,
            initial_word_value: Some(0.2f32.to_bits()),
        }],
        self_transition_mode: 0,
        event_mode: 0,
        duration_bits: 0.2f32.to_bits(),
        to_generator_start_fraction_bits: 0.0f32.to_bits(),
        flags: 1,
        end_mode: 0,
        blend_curve: 0,
        alignment_bone: 0,
    };
    let transition = CapabilityGeneratorTransition {
        trigger_interval: CapabilityGeneratorInterval {
            enter_event: no_generator_event(),
            exit_event: no_generator_event(),
            enter_time_bits: 0.0f32.to_bits(),
            exit_time_bits: 0.0f32.to_bits(),
        },
        initiate_interval: CapabilityGeneratorInterval {
            enter_event: no_generator_event(),
            exit_event: no_generator_event(),
            enter_time_bits: 0.0f32.to_bits(),
            exit_time_bits: 0.0f32.to_bits(),
        },
        transition_effect: Some(transition_effect),
        condition: None,
        event: no_generator_event(),
        to_state_id: 1,
        from_nested_state_id: 0,
        to_nested_state_id: 0,
        priority: 0,
        flags: 0,
    };
    let mut second_transition = transition.clone();
    second_transition.priority = 1;
    let transitions = CapabilityGeneratorTransitionArray {
        source: recursive_source_object(
            705,
            "hkbStateMachineTransitionInfoArray",
            "ForwardTransitions",
        ),
        has_eventless_transitions: Some(true),
        has_time_bounded_transitions: Some(false),
        transitions: vec![transition, second_transition],
    };
    let root = CapabilityGeneratorNode::StateMachine {
        source: recursive_source_object(700, "hkbStateMachine", "ForwardLocomotionBehavior"),
        bindings: vec![CapabilityGeneratorVariableBinding {
            member_path: "startStateId".to_string(),
            variable_index: state_variable_index as i32,
            variable_name: "iMovementSpeed".to_string(),
            bit_index: -1,
            binding_type: CapabilityVariableBindingType::Variable,
            source_variable_type: CapabilitySourceVariableType::Int32,
            initial_word_value: Some(0),
        }],
        event_to_send_when_state_or_transition_changes: CapabilityGeneratorEventProperty {
            event: no_generator_event(),
            payload: None,
        },
        start_state_id_selector: None,
        start_state_id: 0,
        return_to_previous_state_event: no_generator_event(),
        random_transition_event: no_generator_event(),
        transition_to_next_higher_state_event: no_generator_event(),
        transition_to_next_lower_state_event: no_generator_event(),
        sync_variable_index: -1,
        wrap_around_state_id: false,
        max_simultaneous_transitions: 1,
        start_state_mode: 0,
        self_transition_mode: 0,
        states: vec![
            CapabilityGeneratorState {
                source: recursive_source_object(701, "hkbStateMachineStateInfo", "Walk"),
                listeners: Vec::new(),
                enter_notify_events: Vec::new(),
                exit_notify_events: Vec::new(),
                transitions: Some(transitions),
                generator: Box::new(recursive_clip(702, "walkforward_wolf")),
                name: "Walk".to_string(),
                state_id: 0,
                probability_bits: 1.0f32.to_bits(),
                enable: true,
                has_eventless_transitions: true,
            },
            CapabilityGeneratorState {
                source: recursive_source_object(703, "hkbStateMachineStateInfo", "Run"),
                listeners: Vec::new(),
                enter_notify_events: Vec::new(),
                exit_notify_events: Vec::new(),
                transitions: None,
                generator: Box::new(recursive_clip(704, alternate_name)),
                name: "Run".to_string(),
                state_id: 1,
                probability_bits: 1.0f32.to_bits(),
                enable: true,
                has_eventless_transitions: false,
            },
        ],
        wildcard_transitions: None,
    };
    graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::GroundForward)
        .unwrap()
        .generator = CapabilityRoleGenerator::Tree { root };

    manifest.validate_capability_graph(&graph).unwrap();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    assert!(
        core.xml
            .contains("class=\"hkbBlendingTransitionEffect\" signature=\"0x14e54c5c\"")
    );
    assert_eq!(
        core.xml
            .matches("class=\"hkbBlendingTransitionEffect\"")
            .count(),
        1
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"memberPath\">startStateId</hkparam>")
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"memberPath\">duration</hkparam>")
    );
    assert!(core.xml.contains("<hkparam name=\"transition\">#"));
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn recursive_tree_rejects_unlowered_policy_objects_and_missing_variable_evidence() {
    let (mut manifest, mut graph) = ranged_capability();
    let alternate_name = "idle_combat";
    let mut alternate = manifest
        .clips
        .iter()
        .find(|clip| clip.name == "mt_idle_wolf")
        .unwrap()
        .clone();
    alternate.name = alternate_name.to_string();
    alternate.path = format!("Actors\\B21_SkyrimWolf\\Animations\\{alternate_name}.hkx");
    manifest.clips.push(alternate);
    let variable_index = manifest.core.variables.len();
    let selector_variable = VariableDecl {
        name: "IsInCombat".to_string(),
        variable_type: VariableType::Bool,
        initial_value: VariableValue::Bool(false),
    };
    manifest.root.variables.push(selector_variable.clone());
    manifest.core.variables.push(selector_variable);
    let state_machine = CapabilityGeneratorNode::StateMachine {
        source: recursive_source_object(500, "hkbStateMachine", "IdleStateMachine"),
        bindings: Vec::new(),
        event_to_send_when_state_or_transition_changes: CapabilityGeneratorEventProperty {
            event: no_generator_event(),
            payload: Some(recursive_source_object(590, "hkbEventPayload", "Payload")),
        },
        start_state_id_selector: None,
        start_state_id: 0,
        return_to_previous_state_event: no_generator_event(),
        random_transition_event: no_generator_event(),
        transition_to_next_higher_state_event: no_generator_event(),
        transition_to_next_lower_state_event: no_generator_event(),
        sync_variable_index: -1,
        wrap_around_state_id: false,
        max_simultaneous_transitions: 1,
        start_state_mode: 0,
        self_transition_mode: 0,
        states: vec![
            CapabilityGeneratorState {
                source: recursive_source_object(501, "hkbStateMachineStateInfo", "Idle"),
                listeners: Vec::new(),
                enter_notify_events: Vec::new(),
                exit_notify_events: Vec::new(),
                transitions: Some(CapabilityGeneratorTransitionArray {
                    source: recursive_source_object(
                        502,
                        "hkbStateMachineTransitionInfoArray",
                        "IdleTransitions",
                    ),
                    has_eventless_transitions: None,
                    has_time_bounded_transitions: None,
                    transitions: vec![CapabilityGeneratorTransition {
                        trigger_interval: CapabilityGeneratorInterval {
                            enter_event: no_generator_event(),
                            exit_event: no_generator_event(),
                            enter_time_bits: 0.0f32.to_bits(),
                            exit_time_bits: 0.0f32.to_bits(),
                        },
                        initiate_interval: CapabilityGeneratorInterval {
                            enter_event: no_generator_event(),
                            exit_event: no_generator_event(),
                            enter_time_bits: 0.0f32.to_bits(),
                            exit_time_bits: 0.0f32.to_bits(),
                        },
                        transition_effect: Some(CapabilityGeneratorBlendingTransitionEffect {
                            source: recursive_source_object(
                                591,
                                "hkbBlendingTransitionEffect",
                                "IdleBlend",
                            ),
                            bindings: Vec::new(),
                            self_transition_mode: 0,
                            event_mode: 0,
                            duration_bits: 0.2f32.to_bits(),
                            to_generator_start_fraction_bits: 0.0f32.to_bits(),
                            flags: 1,
                            end_mode: 0,
                            blend_curve: 0,
                            alignment_bone: 0,
                        }),
                        condition: Some(recursive_source_object(
                            592,
                            "hkbCondition",
                            "IdleCondition",
                        )),
                        event: no_generator_event(),
                        to_state_id: 1,
                        from_nested_state_id: 0,
                        to_nested_state_id: 0,
                        priority: 0,
                        flags: 0,
                    }],
                }),
                generator: Box::new(recursive_clip(503, "mt_idle_wolf")),
                name: "Idle".to_string(),
                state_id: 0,
                probability_bits: 1.0f32.to_bits(),
                enable: true,
                has_eventless_transitions: false,
            },
            CapabilityGeneratorState {
                source: recursive_source_object(504, "hkbStateMachineStateInfo", "CombatIdle"),
                listeners: Vec::new(),
                enter_notify_events: Vec::new(),
                exit_notify_events: Vec::new(),
                transitions: None,
                generator: Box::new(recursive_clip(505, alternate_name)),
                name: "CombatIdle".to_string(),
                state_id: 1,
                probability_bits: 1.0f32.to_bits(),
                enable: true,
                has_eventless_transitions: false,
            },
        ],
        wildcard_transitions: None,
    };
    let selector = CapabilityGeneratorNode::ManualSelector {
        source: recursive_source_object(600, "hkbManualSelectorGenerator", "IdleSelector"),
        bindings: vec![CapabilityGeneratorVariableBinding {
            member_path: "selectedGeneratorIndex".to_string(),
            variable_index: variable_index as i32,
            variable_name: "IsInCombat".to_string(),
            bit_index: -1,
            binding_type: CapabilityVariableBindingType::Variable,
            source_variable_type: CapabilitySourceVariableType::Bool,
            initial_word_value: None,
        }],
        children: vec![state_machine, recursive_clip(601, alternate_name)],
        selected_generator_index: 0,
        index_selector: None,
        selected_index_can_change_after_activate: true,
        generator_changed_transition_effect: Some(recursive_source_object(
            602,
            "hkbTransitionEffect",
            "SelectorTransition",
        )),
    };
    graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::Idle)
        .unwrap()
        .generator = CapabilityRoleGenerator::Tree { root: selector };

    let codes = error_codes(manifest.validate_capability_graph(&graph));
    assert!(codes.contains(&"capability_generator_event_payload_unlowered"));
    assert!(codes.contains(&"capability_generator_transition_object_unlowered"));
    assert!(codes.contains(&"manual_selector_unlowered_policy_object"));
    assert!(codes.contains(&"capability_generator_missing_initial_value"));
}

#[test]
fn ground_swim_unions_ground_swim_melee_and_projectile_capabilities() {
    let (manifest, graph) = multimodal_capability(CreatureGraphTemplate::GroundSwim);
    assert!(manifest.validate_capability_graph(&graph).is_ok());
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();

    assert!(core.xml.contains("GroundSwimLocomotionCombatSM"));
    for event in [
        "groundForward",
        "enterSwim",
        "swimForward",
        "meleeMultiAttack",
        "fireProjectile",
    ] {
        assert!(core.xml.contains(event), "missing event {event}");
    }
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn ground_fly_unions_ground_flight_and_attack_capabilities() {
    let (manifest, graph) = multimodal_capability(CreatureGraphTemplate::GroundFly);
    assert!(manifest.validate_capability_graph(&graph).is_ok());
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();

    assert!(core.xml.contains("GroundFlightLocomotionCombatSM"));
    for event in [
        "groundForward",
        "enterFlight",
        "flyForward",
        "meleeMultiAttack",
        "fireProjectile",
    ] {
        assert!(core.xml.contains(event), "missing event {event}");
    }
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn capability_graph_never_aliases_a_missing_required_role_to_idle() {
    let (manifest, mut graph) = ranged_capability();
    graph
        .roles
        .retain(|role| role.role != CreatureClipRole::GroundForward);
    let codes = error_codes(manifest.validate_capability_graph(&graph));
    assert!(codes.contains(&"missing_capability_role"));

    let (manifest, mut graph) = ranged_capability();
    let forward = graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::GroundForward)
        .unwrap();
    forward.clip_name = manifest.idle_clip.clone();
    let codes = error_codes(manifest.validate_capability_graph(&graph));
    assert!(codes.contains(&"duplicate_capability_clip"));
}

#[test]
fn swim_capability_emits_move_start_stop_loop_and_packs() {
    let (manifest, graph) = swim_capability();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    let root = scaffold.artifact(&manifest.paths.root_behavior).unwrap();

    assert!(core.xml.contains("SwimLocomotionCombatSM"));
    assert!(
        core.xml
            .contains("<hkparam name=\"name\">SwimIdle</hkparam>")
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"name\">SwimForward</hkparam>")
    );
    let idle_transitions = object_containing(&core.xml, "name=\"#0110\"");
    assert!(
        idle_transitions.contains("<hkparam name=\"eventId\">6</hkparam>"),
        "{idle_transitions}"
    );
    assert!(idle_transitions.contains("<hkparam name=\"toStateId\">1</hkparam>"));
    let forward_transitions = object_containing(&core.xml, "name=\"#0111\"");
    assert!(forward_transitions.contains("<hkparam name=\"eventId\">5</hkparam>"));
    assert!(forward_transitions.contains("<hkparam name=\"toStateId\">0</hkparam>"));
    for event in ["moveStart", "moveStop"] {
        assert!(
            core.xml
                .contains(&format!("<hkcstring>{event}</hkcstring>"))
        );
        assert!(
            root.xml
                .contains(&format!("<hkcstring>{event}</hkcstring>"))
        );
    }
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn fly_capability_emits_hover_cruise_loop_and_packs() {
    let (manifest, graph) = fly_capability();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();

    assert!(core.xml.contains("FlightLocomotionCombatSM"));
    assert!(
        core.xml
            .contains("<hkparam name=\"name\">FlightHover</hkparam>")
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"name\">FlightCruise</hkparam>")
    );
    let hover_transitions = object_containing(&core.xml, "name=\"#0110\"");
    assert!(hover_transitions.contains("<hkparam name=\"eventId\">6</hkparam>"));
    let cruise_transitions = object_containing(&core.xml, "name=\"#0111\"");
    assert!(cruise_transitions.contains("<hkparam name=\"eventId\">5</hkparam>"));
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn stationary_turret_emits_source_rig_direct_at_and_packs() {
    let (manifest, graph) = stationary_turret_capability();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    let root = scaffold.artifact(&manifest.paths.root_behavior).unwrap();

    assert!(core.xml.contains("StationaryTurretCombatSM"));
    assert!(
        core.xml
            .contains("class=\"BSDirectAtModifier\" signature=\"0xcda56038\"")
    );
    assert!(core.xml.contains("StationaryTurretDirectAtGenerator"));
    for binding in [
        "limitHeadingDegreesCCW",
        "limitHeadingDegreesCW",
        "active",
        "sourceBoneIndex",
        "startBoneIndex",
        "endBoneIndex",
    ] {
        assert!(
            core.xml
                .contains(&format!("<hkparam name=\"memberPath\">{binding}</hkparam>"))
        );
    }
    for declaration in [
        "AimHeadingMaxCCW",
        "AimHeadingMaxCW",
        "bAimActive",
        "DirectAtHeadingSourceBoneIndex",
        "DirectAtHeadingBoneIndex",
    ] {
        assert!(
            core.xml
                .contains(&format!("<hkcstring>{declaration}</hkcstring>"))
        );
        assert!(
            root.xml
                .contains(&format!("<hkcstring>{declaration}</hkcstring>"))
        );
    }
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn robot_continuous_attack_emits_start_loop_stop_lifecycle_and_packs() {
    let (manifest, graph) = robot_continuous_attack_capability();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();

    assert!(core.xml.contains("RobotContinuousAttackSM"));
    let start_clip = object_containing(
        &core.xml,
        "<hkparam name=\"name\">ContinuousStart</hkparam>",
    );
    let loop_clip = object_containing(&core.xml, "<hkparam name=\"name\">ContinuousLoop</hkparam>");
    let stop_clip = object_containing(&core.xml, "<hkparam name=\"name\">ContinuousStop</hkparam>");
    assert!(start_clip.contains("<hkparam name=\"mode\">MODE_SINGLE_PLAY</hkparam>"));
    assert!(loop_clip.contains("<hkparam name=\"mode\">MODE_LOOPING</hkparam>"));
    assert!(stop_clip.contains("<hkparam name=\"mode\">MODE_SINGLE_PLAY</hkparam>"));
    let idle_transitions = object_containing(&core.xml, "name=\"#0110\"");
    assert!(idle_transitions.contains("<hkparam name=\"eventId\">6</hkparam>"));
    let start_transitions = object_containing(&core.xml, "name=\"#0111\"");
    assert!(start_transitions.contains("<hkparam name=\"eventId\">7</hkparam>"));
    let loop_transitions = object_containing(&core.xml, "name=\"#0112\"");
    assert!(loop_transitions.contains("<hkparam name=\"eventId\">8</hkparam>"));
    let stop_transitions = object_containing(&core.xml, "name=\"#0113\"");
    assert!(stop_transitions.contains("<hkparam name=\"eventId\">-1</hkparam>"));
    assert!(stop_transitions.contains("<hkparam name=\"flags\">16384</hkparam>"));
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn overlay_capability_emits_event_driven_hkb_layers_and_packs() {
    let (manifest, graph) = overlay_capability();
    let scaffold = emit_capability_scaffold(&manifest, &graph).unwrap();
    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    let root = scaffold.artifact(&manifest.paths.root_behavior).unwrap();

    assert!(
        core.xml
            .contains("class=\"hkbLayerGenerator\" signature=\"0xb4e0c52f\"")
    );
    assert_eq!(core.xml.matches("class=\"hkbLayer\"").count(), 2);
    assert!(core.xml.contains("<hkparam name=\"onEventId\">9</hkparam>"));
    assert!(
        core.xml
            .contains("<hkparam name=\"offEventId\">10</hkparam>")
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"useMotion\">false</hkparam>")
    );
    assert!(
        core.xml
            .contains("<hkparam name=\"animationName\">Animations\\glow_overlay.hkx</hkparam>")
    );
    for event in ["overlayOn", "overlayOff"] {
        assert!(
            core.xml
                .contains(&format!("<hkcstring>{event}</hkcstring>"))
        );
        assert!(
            root.xml
                .contains(&format!("<hkcstring>{event}</hkcstring>"))
        );
    }
    assert_capability_pack_roundtrip(&manifest, &graph);
}

#[test]
fn turret_and_overlay_validation_fail_closed_on_unproven_inputs() {
    let (mut manifest, graph) = stationary_turret_capability();
    manifest
        .core
        .variables
        .retain(|variable| variable.name != "bAimActive");
    manifest.root = manifest.core.clone();
    let codes = error_codes(manifest.validate_capability_graph(&graph));
    assert!(codes.contains(&"missing_turret_aim_declaration"));

    let (mut manifest, mut graph) = overlay_capability();
    let overlay = graph.overlays.first_mut().unwrap();
    overlay.stop_event = overlay.start_event.clone();
    let overlay_clip = manifest
        .clips
        .iter_mut()
        .find(|clip| clip.name == "GlowOverlay")
        .unwrap();
    overlay_clip.looping = false;
    let codes = error_codes(manifest.validate_capability_graph(&graph));
    assert!(codes.contains(&"overlay_event_collision"));
    assert!(codes.contains(&"overlay_clip_mode"));
}

#[test]
fn emitted_class_signatures_are_gated_by_the_hk2014_registry() {
    let (manifest, graph) = wolf_mvp();
    let scaffold = emit_mvp_scaffold(&manifest, &graph).unwrap();
    for artifact in &scaffold.artifacts {
        assert!(
            validate_fo4_havok_xml_signatures(&artifact.xml).is_ok(),
            "{} did not match the hk2014 descriptor registry",
            artifact.source_xml_path
        );
    }

    let core = scaffold.artifact(&manifest.paths.core_behavior).unwrap();
    assert!(
        core.xml
            .contains("class=\"hkbStateMachine\" signature=\"0xa5896bcf\"")
    );
    let mismatched = core.xml.replacen(
        "class=\"hkbStateMachine\" signature=\"0xa5896bcf\"",
        "class=\"hkbStateMachine\" signature=\"0x1913d1c1\"",
        1,
    );
    let mismatch_codes: Vec<&str> = validate_fo4_havok_xml_signatures(&mismatched)
        .unwrap_err()
        .0
        .iter()
        .map(|error| error.code)
        .collect();
    assert!(mismatch_codes.contains(&"havok_signature_mismatch"));

    let unknown = core.xml.replacen(
        "class=\"hkbClipGenerator\" signature=\"0xd4cc9f6\"",
        "class=\"B21UnknownGenerator\" signature=\"0xd4cc9f6\"",
        1,
    );
    let unknown_codes: Vec<&str> = validate_fo4_havok_xml_signatures(&unknown)
        .unwrap_err()
        .0
        .iter()
        .map(|error| error.code)
        .collect();
    assert!(unknown_codes.contains(&"unknown_havok_class"));
}

fn object_containing<'a>(xml: &'a str, needle: &str) -> &'a str {
    let needle_offset = xml.find(needle).expect("expected XML fragment");
    let start = xml[..needle_offset]
        .rfind("    <hkobject name=")
        .expect("fragment belongs to a named top-level hkobject");
    let end = xml[needle_offset..]
        .find("\n    <hkobject name=")
        .map(|offset| needle_offset + offset)
        .or_else(|| {
            xml[needle_offset..]
                .find("\n  </hksection>")
                .map(|offset| needle_offset + offset)
        })
        .expect("named top-level hkobject closes before the data section");
    &xml[start..end]
}

fn reread_packed_xml(report: &SourceRigPackReport, runtime_path: &str) -> String {
    let artifact = report.artifact(runtime_path).unwrap();
    let bytes = fs::read(&artifact.hkx_path).unwrap();
    havok_native::api::havok_hkx_to_xml(&bytes).unwrap()
}

#[test]
fn real_defaults_use_signed_havok_word_encoding() {
    assert_eq!(VariableValue::Real(1.0).word_bits(), 1_065_353_216);
    assert_eq!(
        VariableValue::Real(-1.0).word_bits(),
        i64::from((-1.0_f32).to_bits() as i32)
    );
}

#[test]
fn root_must_be_the_union_of_child_declarations() {
    let mut manifest = wolf_manifest();
    manifest.root.events.remove(0);
    manifest.root.variables.remove(0);
    manifest.root.character_properties.clear();
    let codes = error_codes(manifest.validate());
    assert_eq!(
        codes
            .iter()
            .filter(|code| **code == "root_union_missing")
            .count(),
        3
    );
}

#[test]
fn melee_attack_events_require_the_engine_prefix() {
    let mut manifest = wolf_manifest();
    manifest.core.events[0].name = "Bite".to_string();
    manifest.root.events[0].name = "Bite".to_string();
    let codes = error_codes(manifest.validate());
    assert_eq!(
        codes
            .iter()
            .filter(|code| **code == "melee_event_name")
            .count(),
        2
    );
}

#[test]
fn clip_binding_must_target_the_source_rig_contract() {
    let mut manifest = wolf_manifest();
    manifest.clips[0].binding.skeleton_path = "CharacterAssets\\DonorSkeleton.hkx".to_string();
    manifest.clips[0].binding.declared_transform_tracks = 5;
    manifest.clips[0].binding.transform_track_to_bone_indices = vec![1, 1, 9];
    let codes = error_codes(manifest.validate());
    for expected in [
        "clip_rig_mismatch",
        "clip_track_count",
        "duplicate_clip_bone",
        "clip_bone_index",
    ] {
        assert!(
            codes.contains(&expected),
            "missing error {expected}: {codes:?}"
        );
    }
}

#[test]
fn runtime_file_tree_requires_visual_and_animatable_skeletons() {
    let manifest = wolf_manifest();
    let available: Vec<String> = manifest
        .required_runtime_paths()
        .into_iter()
        .filter(|path| !path.ends_with("Skeleton.nif"))
        .collect();
    let codes = error_codes(manifest.validate_file_tree(&available));
    assert_eq!(codes, vec!["missing_file"]);
}

#[test]
fn xml_validator_rejects_duplicate_ids_dangling_refs_and_bad_counts() {
    let scaffold = emit_idle_scaffold(&wolf_manifest()).unwrap();
    let root = scaffold
        .artifact("Behaviors\\SourceWolfRootBehavior.hkx")
        .unwrap();

    let duplicate = root.xml.replacen("name=\"#0093\"", "name=\"#0092\"", 1);
    let duplicate_codes: Vec<&str> = validate_havok_xml(&duplicate)
        .unwrap_err()
        .0
        .iter()
        .map(|error| error.code)
        .collect();
    assert!(duplicate_codes.contains(&"duplicate_hkobject_id"));

    let dangling = root.xml.replacen(
        "<hkparam name=\"variant\">#0094</hkparam>",
        "<hkparam name=\"variant\">#0999</hkparam>",
        1,
    );
    let dangling_codes: Vec<&str> = validate_havok_xml(&dangling)
        .unwrap_err()
        .0
        .iter()
        .map(|error| error.code)
        .collect();
    assert!(dangling_codes.contains(&"dangling_hkobject_reference"));

    let bad_count = root.xml.replacen(
        "<hkparam name=\"eventNames\" numelements=\"2\">",
        "<hkparam name=\"eventNames\" numelements=\"3\">",
        1,
    );
    let count_codes: Vec<&str> = validate_havok_xml(&bad_count)
        .unwrap_err()
        .0
        .iter()
        .map(|error| error.code)
        .collect();
    assert!(count_codes.contains(&"numelements_mismatch"));

    let misaligned = root.xml.replacen(
        "<hkparam name=\"eventInfos\" numelements=\"2\">",
        "<hkparam name=\"eventInfos\" numelements=\"1\">",
        1,
    );
    let alignment_codes: Vec<&str> = validate_havok_xml(&misaligned)
        .unwrap_err()
        .0
        .iter()
        .map(|error| error.code)
        .collect();
    assert!(alignment_codes.contains(&"declaration_array_alignment"));
}

#[test]
fn xml_validator_rejects_out_of_range_variable_bindings() {
    let scaffold = emit_idle_scaffold(&wolf_manifest()).unwrap();
    let core = scaffold
        .artifact("Behaviors\\SourceWolfCoreBehavior.hkx")
        .unwrap();
    let binding = "    <hkobject name=\"#0096\" class=\"hkbVariableBindingSet\" signature=\"0xe942f339\"><hkparam name=\"bindings\" numelements=\"1\"><hkobject><hkparam name=\"memberPath\">playbackSpeed</hkparam><hkparam name=\"variableIndex\">99</hkparam><hkparam name=\"bitIndex\">255</hkparam><hkparam name=\"bindingType\">BINDING_TYPE_VARIABLE</hkparam></hkobject></hkparam><hkparam name=\"indexOfBindingToEnable\">-1</hkparam></hkobject>\n";
    let invalid = core
        .xml
        .replacen("</hksection>", &format!("{binding}</hksection>"), 1);
    assert!(invalid.contains("#0096"));
    let codes: Vec<&str> = validate_havok_xml(&invalid)
        .unwrap_err()
        .0
        .iter()
        .map(|error| error.code)
        .collect();
    assert!(codes.contains(&"binding_index"));
}

#[test]
fn tutorial_seeker_and_sentry_xmls_pass_structural_validation_when_present() {
    let Some(repo_root) = repository_root() else {
        return;
    };
    let relative_paths = [
        "refs/newcreature/Samples/Sample Behavior - Seeker Mine/Export/SeekerMineProject.xml",
        "refs/newcreature/Samples/Sample Behavior - Seeker Mine/Export/Characters/SeekerMineCharacter.xml",
        "refs/newcreature/Samples/Sample Behavior - Seeker Mine/Export/Behaviors/SeekerMineRootBehavior.xml",
        "refs/newcreature/Samples/Sample Behavior - Seeker Mine/Export/Behaviors/SeekerMineCoreBehavior.xml",
        "refs/newcreature/Samples/Sample Behavior - Sentry Machinegun Turret/Export/SentryTurretProject.xml",
        "refs/newcreature/Samples/Sample Behavior - Sentry Machinegun Turret/Export/Characters/SentryTurretCharacter.xml",
        "refs/newcreature/Samples/Sample Behavior - Sentry Machinegun Turret/Export/Behaviors/SentryTurretRootBehavior.xml",
        "refs/newcreature/Samples/Sample Behavior - Sentry Machinegun Turret/Export/Behaviors/SentryTurretCoreBehavior.xml",
    ];

    for relative_path in relative_paths {
        let path = repo_root.join(relative_path);
        if !path.is_file() {
            continue;
        }
        let xml = fs::read_to_string(&path).unwrap();
        let result = validate_havok_xml(&xml);
        assert!(
            result.is_ok(),
            "tutorial sample {} failed validation: {}",
            path.display(),
            result.unwrap_err()
        );
    }
}

fn repository_root() -> Option<PathBuf> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for _ in 0..6 {
        path = path.parent()?.to_path_buf();
    }
    Some(path)
}

fn record_manifest(
    creature_name: &str,
    target_plugin: &str,
    first_local_id: u32,
) -> CreatureRecordManifest {
    let target = |offset| TargetFormKey::new(first_local_id + offset, target_plugin);
    CreatureRecordManifest {
        target_plugin: target_plugin.to_string(),
        editor_id_prefix: "B21_".to_string(),
        form_keys: CreatureRecordFormKeys {
            race: target(0),
            npc: target(1),
            skin: target(2),
            armor_addon: target(3),
            body_part_data: target(4),
            unarmed_weapon: target(5),
        },
        editor_ids: CreatureRecordEditorIds {
            race: format!("{creature_name}Race"),
            npc: format!("{creature_name}NPC"),
            skin: format!("{creature_name}Skin"),
            armor_addon: format!("{creature_name}BodyAA"),
            body_part_data: format!("{creature_name}BodyPartData"),
            unarmed_weapon: format!("{creature_name}Unarmed"),
        },
        display_name: creature_name.trim_start_matches("B21_").replace('_', " "),
        body_nif: format!("Actors\\{creature_name}\\CharacterAssets\\{creature_name}.nif"),
    }
}

fn record_field<'a>(record: &'a crate::record::Record, signature: &str) -> &'a FieldValue {
    &record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == signature)
        .unwrap_or_else(|| panic!("{} missing {signature}", record.sig.as_str()))
        .value
}

fn is_null_form_reference(value: &FieldValue) -> bool {
    match value {
        FieldValue::None | FieldValue::Uint(0) | FieldValue::Int(0) => true,
        FieldValue::FormKey(form_key) => form_key.local == 0,
        FieldValue::Bytes(bytes) => bytes.as_slice() == [0, 0, 0, 0],
        _ => false,
    }
}

fn record_string_values(
    record: &crate::record::Record,
    signature: &str,
    interner: &crate::sym::StringInterner,
) -> Vec<String> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == signature)
        .filter_map(|entry| match entry.value {
            FieldValue::String(value) => interner.resolve(value).map(ToOwned::to_owned),
            _ => None,
        })
        .collect()
}

fn npc_loadout(
    record: &crate::record::Record,
    interner: &crate::sym::StringInterner,
) -> (Vec<u32>, Vec<u32>) {
    let equipment = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "CNTO")
        .filter_map(|field| match &field.value {
            FieldValue::Struct(fields) => fields
                .iter()
                .find(|(name, _)| interner.resolve(*name) == Some("item"))
                .and_then(|(_, value)| match value {
                    FieldValue::FormKey(form_key) => Some(form_key.local),
                    _ => None,
                }),
            _ => None,
        })
        .collect();
    let spells = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "SPLO")
        .filter_map(|field| match field.value {
            FieldValue::FormKey(form_key) => Some(form_key.local),
            _ => None,
        })
        .collect();
    (equipment, spells)
}

fn struct_test_member<'a>(
    value: &'a FieldValue,
    name: &str,
    interner: &crate::sym::StringInterner,
) -> &'a FieldValue {
    let FieldValue::Struct(fields) = value else {
        panic!("expected struct")
    };
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing struct member {name}"))
}

fn normalized_i32(value: &FieldValue) -> i32 {
    match value {
        FieldValue::Int(value) => i32::try_from(*value).unwrap(),
        FieldValue::Uint(value) => u32::try_from(*value).unwrap() as i32,
        FieldValue::Bytes(value) if value.len() == 4 => {
            i32::from_le_bytes([value[0], value[1], value[2], value[3]])
        }
        other => panic!("expected normalized i32, got {other:?}"),
    }
}

fn normalized_u32(value: &FieldValue) -> u32 {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).unwrap(),
        FieldValue::Bytes(value) if value.len() == 4 => {
            u32::from_le_bytes([value[0], value[1], value[2], value[3]])
        }
        other => panic!("expected normalized u32, got {other:?}"),
    }
}

fn normalized_f32_bits(value: &FieldValue) -> u32 {
    match value {
        FieldValue::Float(value) => value.to_bits(),
        FieldValue::Bytes(value) if value.len() == 4 => {
            u32::from_le_bytes([value[0], value[1], value[2], value[3]])
        }
        other => panic!("expected normalized f32, got {other:?}"),
    }
}

fn struct_u8(value: &FieldValue, name: &str, interner: &crate::sym::StringInterner) -> u8 {
    let FieldValue::Struct(fields) = value else {
        panic!("expected struct")
    };
    let value = fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing struct field {name}"));
    match value {
        FieldValue::Uint(value) => *value as u8,
        FieldValue::Bytes(bytes) if bytes.len() == 1 => bytes[0],
        other => panic!("unexpected value for {name}: {other:?}"),
    }
}

#[test]
fn wolf_record_profile_emits_complete_custom_fo4_closure() {
    let (rig, graph) = wolf_mvp();
    let manifest = record_manifest(&rig.creature_name, "B21_CreatureMVP.esp", 0x800);
    let profile = CreatureRecordProfile::root_segment_32("NPC Root [Root]");
    let interner = crate::sym::StringInterner::new();

    let closure =
        emit_creature_record_closure(&rig, &graph, &manifest, &profile, &interner).unwrap();
    let signatures = closure
        .records
        .iter()
        .map(|record| record.sig.as_str())
        .collect::<Vec<_>>();
    assert_eq!(signatures, ["RACE", "NPC_", "ARMO", "ARMA", "BPTD", "WEAP"]);

    let race = closure.record("RACE").unwrap();
    assert_eq!(
        record_string_values(race, "ANAM", &interner),
        [
            rig.visual_skeleton_nif.clone(),
            rig.visual_skeleton_nif.clone()
        ]
    );
    assert_eq!(
        record_string_values(race, "MODL", &interner),
        [rig.paths.project.clone(), rig.paths.project.clone()]
    );
    assert_eq!(
        record_string_values(race, "ATKE", &interner),
        [graph.melee_event.clone()]
    );
    let subgraphs =
        crate::fixups::havok::anim_text_data_emit::subgraphs_from_race_record(race, &interner);
    assert_eq!(subgraphs.len(), 1);
    assert_eq!(subgraphs[0].core_behavior, rig.paths.core_behavior);
    assert_eq!(
        subgraphs[0].sapt_chain,
        ["Actors\\B21_SkyrimWolf\\Animations"]
    );
    assert!(matches!(
        record_field(race, "SRAF"),
        FieldValue::Bytes(value) if value.as_slice() == [1, 0, 0, 0]
    ));

    let body_part = closure.record("BPTD").unwrap();
    assert_eq!(
        struct_u8(
            record_field(body_part, "BPND"),
            "geometry_segment_index",
            &interner
        ),
        32
    );
    assert_eq!(
        record_string_values(body_part, "BPNN", &interner),
        ["NPC Root [Root]"]
    );

    for record in &closure.records {
        assert_eq!(
            interner.resolve(record.form_key.plugin),
            Some("B21_CreatureMVP.esp")
        );
    }
}

#[test]
fn same_record_api_emits_fnv_gecko_profile_without_source_formkeys() {
    let (rig, graph) = gecko_mvp();
    let manifest = record_manifest(&rig.creature_name, "B21_CreatureMVP.esp", 0x900);
    let profile = CreatureRecordProfile::root_segment_32("Bip01");
    let interner = crate::sym::StringInterner::new();

    let closure =
        emit_creature_record_closure(&rig, &graph, &manifest, &profile, &interner).unwrap();
    assert_eq!(closure.records.len(), 6);
    assert_eq!(
        record_string_values(closure.record("RACE").unwrap(), "ATKE", &interner),
        ["meleeGeckoBite"]
    );
    assert_eq!(
        record_string_values(closure.record("ARMA").unwrap(), "MOD2", &interner),
        ["Actors\\B21_Gecko\\CharacterAssets\\B21_Gecko.nif"]
    );
    for source_plugin in ["Skyrim.esm", "FalloutNV.esm", "Fallout3.esm"] {
        assert!(
            interner.get(source_plugin).is_none(),
            "source plugin leaked into record closure: {source_plugin}"
        );
    }
}

#[test]
fn record_emitter_fails_closed_on_non_mvp_body_segment() {
    let (rig, graph) = gecko_mvp();
    let manifest = record_manifest(&rig.creature_name, "B21_CreatureMVP.esp", 0x900);
    let mut profile = CreatureRecordProfile::root_segment_32("Bip01");
    profile.root_body_part.geometry_segment_index = 6;
    let interner = crate::sym::StringInterner::new();

    let error =
        emit_creature_record_closure(&rig, &graph, &manifest, &profile, &interner).unwrap_err();
    assert!(matches!(
        error,
        CreatureRecordError::UnsupportedField {
            record: "BPTD",
            field: "BPND.geometry_segment_index",
            ..
        }
    ));
}

#[test]
fn record_emitter_accepts_same_named_converted_output_plugin() {
    let (rig, graph) = gecko_mvp();
    let manifest = record_manifest(&rig.creature_name, "FalloutNV.esm", 0x900);
    let profile = CreatureRecordProfile::root_segment_32("Bip01");
    let interner = crate::sym::StringInterner::new();

    let closure =
        emit_creature_record_closure(&rig, &graph, &manifest, &profile, &interner).unwrap();
    assert!(
        closure
            .records
            .iter()
            .all(|record| { interner.resolve(record.form_key.plugin) == Some("FalloutNV.esm") })
    );
}

#[test]
fn record_emitter_rejects_authoring_or_out_of_tree_runtime_paths() {
    let (rig, graph) = wolf_mvp();
    let mut manifest = record_manifest(&rig.creature_name, "B21_CreatureMVP.esp", 0x800);
    manifest.body_nif = "Actors\\Molerat\\Molerat.xml".to_string();
    let profile = CreatureRecordProfile::root_segment_32("NPC Root [Root]");
    let interner = crate::sym::StringInterner::new();

    let error =
        emit_creature_record_closure(&rig, &graph, &manifest, &profile, &interner).unwrap_err();
    assert!(matches!(
        error,
        CreatureRecordError::InvalidManifest {
            code: "custom_runtime_path",
            ..
        }
    ));
}

fn source_identity(namespace: &str, plugin: &str, local_form_id: u32) -> SourceCreatureIdentity {
    SourceCreatureIdentity {
        namespace: namespace.to_string(),
        plugin: plugin.to_string(),
        local_form_id,
    }
}

fn semantic_proof(schema: &str, document: serde_json::Value) -> CreatureSemanticProofReceipt {
    let canonical_json = serde_json::to_string(&document).unwrap();
    CreatureSemanticProofReceipt {
        schema: schema.to_string(),
        canonical_json: canonical_json.clone(),
        canonical_json_blake3: blake3::hash(canonical_json.as_bytes()).to_hex().to_string(),
    }
}

fn grounded_race_data() -> Fo4RaceDataTarget {
    Fo4RaceDataTarget {
        male_height: 1.0,
        female_height: 1.0,
        male_default_weight: [0.0, 0.0, 0.0],
        female_default_weight: [0.0, 0.0, 0.0],
        flags: vec![Fo4RaceFlag::Walks, Fo4RaceFlag::AllowRagdollCollision],
        acceleration_rate: 1.0,
        deceleration_rate: 1.0,
        size: Fo4RaceSize::Medium,
        injured_health_percent: 0.2,
        body_biped_object: 3,
        aim_angle_tolerance: 30.0,
        flight_radius: 0.0,
        angular_acceleration_rate: 1.0,
        angular_tolerance: 30.0,
        flags_2: vec![Fo4RaceFlag2::UseQuadrupedController],
        xp_value: 0,
        orientation_limit_pitch: 0.0,
        orientation_limit_roll: 0.0,
    }
}

fn executable_recipe_fixture(
    namespace: &str,
    plugin: &str,
    source_game: &str,
) -> SourceRigExecutableRecipe {
    let (rig, mvp) = wolf_mvp();
    let graph = CapabilityGraphManifest::from_mvp(&mvp, &MvpMotionManifest::default());
    let base = record_manifest(&rig.creature_name, "B21_CreatureRecipe.esp", 0xD00);
    let source = source_identity(namespace, plugin, 0x12_3456);
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: source.clone(),
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Recipe Creature".to_string(),
            primary: true,
            level: 7,
            health: 120,
            action_points: 80,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: vec![CreatureAttackRecordVariant {
            id: "bite".to_string(),
            event: mvp.melee_event,
            primary: true,
            projection: CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key: base.form_keys.unarmed_weapon.clone(),
                weapon_editor_id: base.editor_ids.unarmed_weapon.clone(),
                damage: 18,
                reach: 0.75,
                attack_seconds: 0.9,
            },
            damage_multiplier: 1.1,
            chance: 0.8,
            strike_angle: 32.0,
            action_point_cost: 22.0,
            target_data: CreatureAttackTargetData::default(),
        }],
    };
    let key_plan = CreatureRecordKeyPlan {
        planned_source_identity: source.clone(),
        base: base.form_keys.clone(),
        armor_addons: vec![base.form_keys.armor_addon.clone()],
        npc_variants: vec![CreatureNpcRecordKeyPlan {
            source_identity: source.clone(),
            form_key: base.form_keys.npc.clone(),
            primary: true,
        }],
        melee_attacks: vec![CreatureMeleeRecordKeyPlan {
            attack_id: "bite".to_string(),
            form_key: base.form_keys.unarmed_weapon.clone(),
            primary: true,
        }],
    };
    let batch_intent = SourceRigRecordBatchIntent {
        family_id: "recipe-family".to_string(),
        primary_mapping: CreaturePrimaryRecordMapping {
            source,
            target: base.form_keys.npc.clone(),
        },
        required_target_records: Vec::new(),
    };
    let receipts = SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
        &rig,
        &graph,
        &projection,
        &key_plan,
        &batch_intent,
        None,
        source_game,
    );
    SourceRigExecutableRecipe::new(rig, graph, projection, key_plan, batch_intent, receipts)
        .unwrap()
}

#[test]
fn executable_recipe_roundtrip_is_canonical_and_rebuilds_deterministically() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let mut shuffled = recipe.clone();
    shuffled.rig.clips.reverse();
    shuffled.graph.roles.reverse();
    shuffled.graph.explicit_events.reverse();
    shuffled.projection.race_data.flags.reverse();
    shuffled.field_receipts.reverse();

    assert_eq!(
        recipe.canonical_json().unwrap(),
        shuffled.canonical_json().unwrap()
    );
    assert_eq!(
        recipe.stable_hash().unwrap(),
        shuffled.stable_hash().unwrap()
    );
    let roundtrip =
        SourceRigExecutableRecipe::from_json(&recipe.canonical_json().unwrap()).unwrap();
    assert_eq!(roundtrip, recipe);
    assert_eq!(
        roundtrip.rebuild_projection_input().unwrap(),
        recipe.rebuild_projection_input().unwrap()
    );

    let interner = crate::sym::StringInterner::new();
    let first = recipe.rebuild_record_family_batch(&interner).unwrap();
    let second = shuffled.rebuild_record_family_batch(&interner).unwrap();
    assert_eq!(first.family_id, second.family_id);
    assert_eq!(first.primary_mappings, second.primary_mappings);
    assert_eq!(
        first.closures[0].required_target_records,
        second.closures[0].required_target_records
    );
    assert_eq!(
        first.closures[0].closure.records.len(),
        second.closures[0].closure.records.len()
    );
    for (left, right) in first.closures[0]
        .closure
        .records
        .iter()
        .zip(&second.closures[0].closure.records)
    {
        assert_eq!(left.sig, right.sig);
        assert_eq!(left.form_key, right.form_key);
        assert_eq!(left.fields, right.fields);
    }
}

#[test]
fn actor_action_idles_are_recipe_bound_and_emitted_with_exact_parent_dependencies() {
    let base = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let requirements = required_actor_action_records(&base.graph, &base.rig.paths.root_behavior);
    let plans = requirements
        .iter()
        .enumerate()
        .map(|(index, requirement)| CreatureActorActionRecordPlan {
            requirement: requirement.clone(),
            form_key: TargetFormKey::new(
                0xD20 + u32::try_from(index).unwrap(),
                "B21_CreatureRecipe.esp",
            ),
            editor_id: format!("B21_RecipeAction{index:02}"),
        })
        .collect::<Vec<_>>();
    let batch_intent = base
        .batch_intent
        .clone()
        .with_actor_action_dependencies(&plans);
    let receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent_with_actor_actions(
            &base.rig,
            &base.graph,
            &base.projection,
            &base.key_plan,
            &plans,
            &batch_intent,
            None,
            "fallout_new_vegas",
        );
    let recipe = SourceRigExecutableRecipe::new_with_actor_actions(
        base.rig,
        base.graph,
        base.projection,
        base.key_plan,
        plans.clone(),
        batch_intent,
        receipts,
    )
    .unwrap();
    assert!(
        !actor_action_admission_report(
            &recipe.graph,
            &recipe.rig.paths.root_behavior,
            &recipe.actor_action_records,
        )
        .has_fatal()
    );

    let interner = crate::sym::StringInterner::new();
    let batch = recipe.rebuild_record_family_batch(&interner).unwrap();
    let closure = &batch.closures[0];
    let idles = closure
        .closure
        .records
        .iter()
        .filter(|record| record.sig.as_str() == "IDLE")
        .collect::<Vec<_>>();
    assert_eq!(idles.len(), requirements.len());
    assert!(idles.iter().all(|record| {
        ["EDID", "DNAM", "ENAM", "ANAM", "DATA"]
            .iter()
            .all(|signature| {
                record
                    .fields
                    .iter()
                    .any(|field| field.sig.as_str() == *signature)
            })
    }));
    let parent_key = interner.intern("parent");
    let previous_key = interner.intern("previous");
    let mut ordered_plans = plans.clone();
    ordered_plans.sort_by_key(|plan| {
        (
            plan.requirement.parent_form_id,
            plan.requirement.kind,
            plan.requirement.animation_event.to_ascii_lowercase(),
            plan.editor_id.to_ascii_lowercase(),
            plan.form_key.local,
        )
    });
    let mut previous_by_parent = std::collections::BTreeMap::new();
    for plan in &ordered_plans {
        let planned_form_key = FormKey {
            local: plan.form_key.local,
            plugin: interner.intern(&plan.form_key.plugin),
        };
        let record = idles
            .iter()
            .find(|record| record.form_key == planned_form_key)
            .unwrap();
        let FieldValue::Struct(animations) = &record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ANAM")
            .unwrap()
            .value
        else {
            panic!("generated IDLE ANAM must be a parsed struct");
        };
        assert_eq!(
            animations
                .iter()
                .find(|(key, _)| *key == parent_key)
                .map(|(_, value)| value),
            Some(&FieldValue::FormKey(FormKey {
                local: plan.requirement.parent_form_id,
                plugin: interner.intern("Fallout4.esm"),
            }))
        );
        let previous = animations
            .iter()
            .find(|(key, _)| *key == previous_key)
            .map(|(_, value)| value)
            .unwrap();
        if let Some(expected_previous) = previous_by_parent
            .get(&plan.requirement.parent_form_id)
            .copied()
        {
            assert_eq!(previous, &FieldValue::FormKey(expected_previous));
        } else {
            assert!(
                matches!(previous, FieldValue::Uint(0))
                    || matches!(previous, FieldValue::Bytes(bytes) if bytes.as_slice() == [0; 4])
            );
        }
        previous_by_parent.insert(plan.requirement.parent_form_id, record.form_key);
    }
    let expected_dependencies = requirements
        .iter()
        .map(|requirement| CreatureTargetRecordReference {
            signature: "AACT".to_string(),
            form_key: requirement.parent_form_key(),
        })
        .collect::<std::collections::BTreeSet<_>>();
    let actual_dependencies = closure
        .required_target_records
        .iter()
        .filter(|dependency| dependency.signature == "AACT")
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(actual_dependencies, expected_dependencies);
}

fn multipart_executable_recipe_fixture() -> SourceRigExecutableRecipe {
    let base = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let mut projection = base.projection.clone();
    let additional_body = format!(
        "{}\\AdditionalVariant.nif",
        projection.base.body_nif.rsplit_once('\\').unwrap().0
    );
    let additional_armor_addon = TargetFormKey::new(0xD10, "B21_CreatureRecipe.esp");
    projection.body_nif_parts.push(CreatureBodyNifRecordPart {
        body_nif: additional_body,
        armor_addon_form_key: additional_armor_addon.clone(),
        armor_addon_editor_id: "B21_SkyrimWolfAdditionalBodyAA".to_string(),
    });
    let mut key_plan = base.key_plan.clone();
    key_plan.armor_addons.push(additional_armor_addon);
    let field_receipts = SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
        &base.rig,
        &base.graph,
        &projection,
        &key_plan,
        &base.batch_intent,
        None,
        "fallout_new_vegas",
    );
    SourceRigExecutableRecipe::new(
        base.rig,
        base.graph,
        projection,
        key_plan,
        base.batch_intent,
        field_receipts,
    )
    .unwrap()
}

#[test]
fn multipart_body_recipe_preserves_ordered_armatures_and_closure() {
    let recipe = multipart_executable_recipe_fixture();
    let interner = crate::sym::StringInterner::new();
    let closure = recipe.rebuild_projection_closure(&interner).unwrap();

    assert_eq!(closure.closure.records.len(), 7);
    let armature_keys = closure
        .closure
        .record("ARMO")
        .unwrap()
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "MODL")
        .map(|field| match field.value {
            FieldValue::FormKey(form_key) => form_key.local,
            ref value => panic!("expected ARMO armature FormKey, got {value:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        armature_keys,
        recipe
            .projection
            .body_nif_parts
            .iter()
            .map(|part| part.armor_addon_form_key.local)
            .collect::<Vec<_>>()
    );
    let armor_addons = closure
        .closure
        .records
        .iter()
        .filter(|record| record.sig.as_str() == "ARMA")
        .collect::<Vec<_>>();
    assert_eq!(armor_addons.len(), 2);
    assert_eq!(
        armor_addons
            .iter()
            .map(|record| record_string_values(record, "MOD2", &interner)[0].clone())
            .collect::<Vec<_>>(),
        recipe
            .projection
            .body_nif_parts
            .iter()
            .map(|part| part.body_nif.clone())
            .collect::<Vec<_>>()
    );

    let canonical = recipe.canonical_json().unwrap();
    let roundtrip = SourceRigExecutableRecipe::from_json(&canonical).unwrap();
    assert_eq!(roundtrip, recipe);
    assert_eq!(
        roundtrip.stable_hash().unwrap(),
        recipe.stable_hash().unwrap()
    );

    let temp = tempfile::tempdir().unwrap();
    let artifacts = execution_artifacts(&recipe, &temp.path().join("inputs"));
    let nif_closure = execution_nif_closure(&recipe, &temp.path().join("nif-closure"));
    let recipe = bind_execution_recipe(recipe, &artifacts, &nif_closure);
    let recipe_path = temp.path().join("multipart-recipe.json");
    fs::write(&recipe_path, recipe.canonical_json().unwrap()).unwrap();
    let stage = temp.path().join("stage");
    prepare_source_rig_execution(&recipe_path, &artifacts, &nif_closure, &stage, &interner)
        .unwrap();

    let missing_closure =
        execution_nif_closure_with_material(&recipe, &temp.path().join("missing-nif-closure"));
    let missing_recipe = bind_execution_recipe(recipe, &artifacts, &missing_closure);
    let missing_recipe_path = temp.path().join("missing-nif-recipe.json");
    fs::write(
        &missing_recipe_path,
        missing_recipe.canonical_json().unwrap(),
    )
    .unwrap();
    let missing_stage = temp.path().join("missing-nif-stage");
    assert!(matches!(
        prepare_source_rig_execution(
            &missing_recipe_path,
            &artifacts,
            &missing_closure,
            &missing_stage,
            &interner,
        ),
        Err(SourceRigExecutionError::Artifact {
            code: "creature_closure_nif_mismatch",
            ..
        })
    ));
    assert!(!missing_stage.exists());
}

#[test]
fn recipe_v1_migrates_only_legacy_single_body_projection() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let mut legacy = serde_json::to_value(&recipe).unwrap();
    legacy["version"] = serde_json::json!(1);
    legacy["projection"]
        .as_object_mut()
        .unwrap()
        .remove("body_nif_parts");
    legacy["key_plan"]
        .as_object_mut()
        .unwrap()
        .remove("armor_addons");
    let migrated = SourceRigExecutableRecipe::from_json(&legacy.to_string()).unwrap();
    assert_eq!(migrated.version, SOURCE_RIG_RECIPE_VERSION);
    assert_eq!(migrated.projection.body_nif_parts.len(), 1);
    assert_eq!(migrated.key_plan.armor_addons.len(), 1);

    let mut ambiguous = serde_json::to_value(multipart_executable_recipe_fixture()).unwrap();
    ambiguous["version"] = serde_json::json!(1);
    assert!(matches!(
        SourceRigExecutableRecipe::from_json(&ambiguous.to_string()),
        Err(SourceRigRecipeError::Invalid {
            code: "legacy_multipart_recipe",
            ..
        })
    ));
}

#[test]
fn recipe_v7_with_named_expression_array_migrates_to_v8() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let mut legacy = serde_json::to_value(&recipe).unwrap();
    legacy["version"] = serde_json::json!(7);
    let migrated = SourceRigExecutableRecipe::from_json(&legacy.to_string()).unwrap();
    assert_eq!(migrated.version, SOURCE_RIG_RECIPE_VERSION);
    assert_eq!(migrated.graph, recipe.graph);
}

fn execution_xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn execution_fixture_skeleton(recipe: &SourceRigExecutableRecipe) -> Vec<u8> {
    let skeleton = &recipe.rig.animation_skeleton;
    let parents = skeleton
        .bones
        .iter()
        .map(|bone| {
            bone.parent_index
                .map_or_else(|| "-1".to_string(), |parent| parent.to_string())
        })
        .collect::<Vec<_>>()
        .join(" ");
    let bones = skeleton
        .bones
        .iter()
        .map(|bone| {
            format!(
                "<hkobject><hkparam name=\"name\">{}</hkparam><hkparam name=\"lockTranslation\">false</hkparam></hkobject>",
                execution_xml_escape(&bone.name)
            )
        })
        .collect::<String>();
    let pose = std::iter::repeat_n("(0 0 0 0)(0 0 0 1)(1 1 1 0)", skeleton.bones.len())
        .collect::<Vec<_>>()
        .join(" ");
    let float_slots = skeleton
        .float_slots
        .iter()
        .map(|slot| format!("<hkcstring>{}</hkcstring>", execution_xml_escape(slot)))
        .collect::<String>();
    let reference_floats = std::iter::repeat_n("0", skeleton.float_slots.len())
        .collect::<Vec<_>>()
        .join(" ");
    let xml = format!(
        r##"<?xml version="1.0" encoding="ASCII" standalone="no"?>
<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1">
<hksection name="__data__">
<hkobject name="#0001" class="hkRootLevelContainer" signature="0x2772c11e"><hkparam name="namedVariants" numelements="1"><hkobject><hkparam name="name">Merged Animation Container</hkparam><hkparam name="className">hkaAnimationContainer</hkparam><hkparam name="variant">#0002</hkparam></hkobject></hkparam></hkobject>
<hkobject name="#0002" class="hkaAnimationContainer" signature="0x8dc20333"><hkparam name="skeletons" numelements="1">#0003</hkparam><hkparam name="animations" numelements="0"></hkparam><hkparam name="bindings" numelements="0"></hkparam><hkparam name="attachments" numelements="0"></hkparam><hkparam name="skins" numelements="0"></hkparam></hkobject>
<hkobject name="#0003" class="hkaSkeleton" signature="0x366e8220"><hkparam name="name">{runtime_name}</hkparam><hkparam name="parentIndices" numelements="{bone_count}">{parents}</hkparam><hkparam name="bones" numelements="{bone_count}">{bones}</hkparam><hkparam name="referencePose" numelements="{bone_count}">{pose}</hkparam><hkparam name="referenceFloats" numelements="{float_count}">{reference_floats}</hkparam><hkparam name="floatSlots" numelements="{float_count}">{float_slots}</hkparam><hkparam name="localFrames" numelements="0"></hkparam><hkparam name="partitions" numelements="0"></hkparam></hkobject>
</hksection></hkpackfile>"##,
        runtime_name = execution_xml_escape(&skeleton.runtime_name),
        bone_count = skeleton.bones.len(),
        float_count = skeleton.float_slots.len(),
    );
    havok_native::hkx::tagxml::read_tagxml_string(&xml)
        .unwrap()
        .save()
}

fn execution_clip_motion(recipe: &SourceRigExecutableRecipe, clip_name: &str) -> ClipMotionPolicy {
    recipe
        .graph
        .roles
        .iter()
        .find(|role| role.clip_name == clip_name)
        .map(|role| role.motion)
        .or_else(|| {
            recipe
                .graph
                .overlays
                .iter()
                .find(|overlay| overlay.clip_name == clip_name)
                .map(|overlay| overlay.motion)
        })
        .unwrap()
}

fn execution_fixture_clip(recipe: &SourceRigExecutableRecipe, clip: &ClipDecl) -> Vec<u8> {
    let transform = "(0 0 0 0)(0 0 0 1)(1 1 1 0)";
    let transforms = std::iter::repeat_n(transform, clip.binding.declared_transform_tracks)
        .collect::<Vec<_>>()
        .join(" ");
    let transform_map = clip
        .binding
        .transform_track_to_bone_indices
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let floats = std::iter::repeat_n("0", clip.binding.declared_float_tracks)
        .collect::<Vec<_>>()
        .join(" ");
    let float_map = clip
        .binding
        .float_track_to_float_slot_indices
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let motion = execution_clip_motion(recipe, &clip.name);
    let (extracted_motion, reference_frame) = if motion.extracted_planar_reference_frames == 0 {
        ("#null".to_string(), String::new())
    } else {
        let samples = std::iter::repeat_n("(0 0 0 0)", motion.extracted_planar_reference_frames)
            .collect::<Vec<_>>()
            .join(" ");
        (
            "#0005".to_string(),
            format!(
                "<hkobject name=\"#0005\" class=\"hkaDefaultAnimatedReferenceFrame\" signature=\"0x60f8e0b8\"><hkparam name=\"up\">(0 0 1 0)</hkparam><hkparam name=\"forward\">(0 1 0 0)</hkparam><hkparam name=\"duration\">1</hkparam><hkparam name=\"referenceFrameSamples\" numelements=\"{}\">{samples}</hkparam></hkobject>",
                motion.extracted_planar_reference_frames
            ),
        )
    };
    let xml = format!(
        r##"<?xml version="1.0" encoding="ASCII" standalone="no"?>
<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1">
<hksection name="__data__">
<hkobject name="#0001" class="hkRootLevelContainer" signature="0x2772c11e"><hkparam name="namedVariants" numelements="1"><hkobject><hkparam name="name">Merged Animation Container</hkparam><hkparam name="className">hkaAnimationContainer</hkparam><hkparam name="variant">#0002</hkparam></hkobject></hkparam></hkobject>
<hkobject name="#0002" class="hkaAnimationContainer" signature="0x8dc20333"><hkparam name="skeletons" numelements="0"></hkparam><hkparam name="animations" numelements="1">#0003</hkparam><hkparam name="bindings" numelements="1">#0004</hkparam><hkparam name="attachments" numelements="0"></hkparam><hkparam name="skins" numelements="0"></hkparam></hkobject>
<hkobject name="#0003" class="hkaInterleavedUncompressedAnimation" signature="0xa5eff3f2"><hkparam name="type">HK_INTERLEAVED_ANIMATION</hkparam><hkparam name="duration">1</hkparam><hkparam name="numberOfTransformTracks">{transform_count}</hkparam><hkparam name="numberOfFloatTracks">{float_count}</hkparam><hkparam name="extractedMotion">{extracted_motion}</hkparam><hkparam name="annotationTracks" numelements="0"></hkparam><hkparam name="transforms" numelements="{transform_count}">{transforms}</hkparam><hkparam name="floats" numelements="{float_count}">{floats}</hkparam></hkobject>
<hkobject name="#0004" class="hkaAnimationBinding" signature="0x0faf9150"><hkparam name="originalSkeletonName">{original_name}</hkparam><hkparam name="animation">#0003</hkparam><hkparam name="transformTrackToBoneIndices" numelements="{transform_count}">{transform_map}</hkparam><hkparam name="floatTrackToFloatSlotIndices" numelements="{float_count}">{float_map}</hkparam><hkparam name="partitionIndices" numelements="0"></hkparam><hkparam name="blendHint">NORMAL</hkparam></hkobject>
{reference_frame}
</hksection></hkpackfile>"##,
        transform_count = clip.binding.declared_transform_tracks,
        float_count = clip.binding.declared_float_tracks,
        original_name = execution_xml_escape(&clip.binding.original_skeleton_name),
    );
    havok_native::hkx::tagxml::read_tagxml_string(&xml)
        .unwrap()
        .save()
}

fn execution_fixture_ragdoll(recipe: &SourceRigExecutableRecipe) -> Vec<u8> {
    use havok_native::convert::creature_ragdoll::*;
    let animation_bones = recipe
        .rig
        .animation_skeleton
        .bones
        .iter()
        .map(|bone| RigBoneIr {
            name: bone.name.clone(),
            parent: bone.parent_index,
            reference_pose: QsTransformIr::identity(),
            lock_translation: false,
        })
        .collect::<Vec<_>>();
    let ragdoll_bones = animation_bones[..2].to_vec();
    let hull = || {
        RagdollShapeIr::ConvexHull(ConvexHullIr {
            vertices: vec![
                [-0.5, -0.5, -0.5],
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
            ],
            planes: vec![
                [1.0, 0.0, 0.0, -0.5],
                [-1.0, 0.0, 0.0, -0.5],
                [0.0, 1.0, 0.0, -0.5],
                [0.0, -1.0, 0.0, -0.5],
                [0.0, 0.0, 1.0, -0.5],
                [0.0, 0.0, -1.0, -0.5],
            ],
            convex_radius: 0.05,
        })
    };
    let body = |index: usize| RigidBodyIr {
        name: format!("Body{index}"),
        ragdoll_bone: ragdoll_bones[index].name.clone(),
        shape: hull(),
        world_from_body: QsTransformIr::identity(),
        mass_properties: MassPropertiesIr {
            mass: 1.0,
            center_of_mass: [0.0; 3],
            inertia_diagonal: [0.2; 3],
        },
        friction: 0.5,
        restitution: 0.0,
        linear_damping: 0.1,
        angular_damping: 0.1,
        collision_filter_info: 0,
    };
    let ir = CreatureRagdollIr {
        name: "ExecutionFixtureSourceOwned".to_string(),
        animation_skeleton: RigSkeletonIr {
            name: "AnimationSkeleton".to_string(),
            bones: animation_bones,
        },
        ragdoll_skeleton: RigSkeletonIr {
            name: "RagdollSkeleton".to_string(),
            bones: ragdoll_bones.clone(),
        },
        bodies: vec![body(0), body(1)],
        constraints: vec![ConstraintIr {
            name: "BodyConstraint".to_string(),
            body_a: "Body0".to_string(),
            body_b: "Body1".to_string(),
            kind: ConstraintKindIr::LimitedHinge(LimitedHingeIr {
                frame_a: QsTransformIr::identity(),
                frame_b: QsTransformIr::identity(),
                min_angle: -0.5,
                max_angle: 0.5,
                max_friction_torque: 1.0,
            }),
        }],
        mappings: ragdoll_bones
            .iter()
            .map(|bone| BoneMappingIr {
                ragdoll_bone: bone.name.clone(),
                animation_bone: bone.name.clone(),
                ragdoll_from_animation: QsTransformIr::identity(),
            })
            .collect(),
    };
    reconstruct_fo4_creature_ragdoll_packfile(&ir).unwrap()
}

fn execution_nif_closure(
    recipe: &SourceRigExecutableRecipe,
    root: &std::path::Path,
) -> SourceRigCreatureClosureInput {
    use nif_core_native::creature_closure::*;
    use nif_core_native::model::NifFile;
    let source_root = root.join("source-data");
    let additional_body = format!(
        "{}\\AdditionalVariant.nif",
        recipe.projection.base.body_nif.rsplit_once('\\').unwrap().0
    );
    let runtime_paths = vec![
        (
            CreatureNifRole::Skeleton,
            recipe.rig.visual_skeleton_nif.clone(),
        ),
        (
            CreatureNifRole::Body,
            recipe.projection.base.body_nif.clone(),
        ),
        (CreatureNifRole::Body, additional_body),
    ];
    let mut inputs = Vec::new();
    for (index, (role, runtime_path)) in runtime_paths.into_iter().enumerate() {
        let source_relative = format!(
            "meshes/{}",
            runtime_path.replace('\\', "/").split_once('/').unwrap().1
        );
        let source_path = source_relative
            .split('/')
            .fold(source_root.clone(), |path, part| path.join(part));
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        let mut nif = NifFile::new("fnv");
        nif.rebuild_header();
        nif.save(Some(source_path)).unwrap();
        inputs.push(CreatureNifInput {
            role,
            source_data_relative_path: source_relative,
            source_owner: format!("fixture-{index}"),
            body_variant: (role == CreatureNifRole::Body).then(|| format!("variant-{index}")),
        });
    }
    let staging = root.join("closure-stage");
    let receipt = stage_creature_nif_closure(&CreatureClosureRequest {
        source_game: "fnv".to_string(),
        source_data_root: source_root,
        private_staging_root: staging.clone(),
        target_namespace: "actors".to_string(),
        texture_fallbacks: std::collections::BTreeMap::new(),
        inputs,
    })
    .unwrap();
    SourceRigCreatureClosureInput {
        receipt_json: receipt.canonical_json().unwrap(),
        staged_data_root: staging.join("data"),
    }
}

fn one_pixel_execution_dds() -> Vec<u8> {
    let mut bytes = vec![0_u8; 132];
    bytes[..4].copy_from_slice(b"DDS ");
    bytes[4..8].copy_from_slice(&124_u32.to_le_bytes());
    bytes[8..12].copy_from_slice(&0x1007_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());
    bytes[16..20].copy_from_slice(&1_u32.to_le_bytes());
    bytes[28..32].copy_from_slice(&1_u32.to_le_bytes());
    bytes[76..80].copy_from_slice(&32_u32.to_le_bytes());
    bytes[80..84].copy_from_slice(&0x41_u32.to_le_bytes());
    bytes[88..92].copy_from_slice(&32_u32.to_le_bytes());
    bytes[92..96].copy_from_slice(&0x00ff_0000_u32.to_le_bytes());
    bytes[96..100].copy_from_slice(&0x0000_ff00_u32.to_le_bytes());
    bytes[100..104].copy_from_slice(&0x0000_00ff_u32.to_le_bytes());
    bytes[104..108].copy_from_slice(&0xff00_0000_u32.to_le_bytes());
    bytes[108..112].copy_from_slice(&0x1000_u32.to_le_bytes());
    bytes
}

fn execution_nif_closure_with_material(
    recipe: &SourceRigExecutableRecipe,
    root: &std::path::Path,
) -> SourceRigCreatureClosureInput {
    use nif_core_native::creature_closure::*;
    use nif_core_native::model::{NifFile, NifValue};
    let source_root = root.join("source-data");
    let runtime_paths = [
        (
            CreatureNifRole::Skeleton,
            recipe.rig.visual_skeleton_nif.as_str(),
        ),
        (
            CreatureNifRole::Body,
            recipe.projection.base.body_nif.as_str(),
        ),
    ];
    let mut inputs = Vec::new();
    for (index, (role, runtime_path)) in runtime_paths.into_iter().enumerate() {
        let source_relative = format!(
            "meshes/{}",
            runtime_path.replace('\\', "/").split_once('/').unwrap().1
        );
        let source_path = source_relative
            .split('/')
            .fold(source_root.clone(), |path, part| path.join(part));
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        let mut nif = NifFile::new("skyrimse");
        if role == CreatureNifRole::Body {
            let target_body = runtime_path.replace('\\', "/");
            let relative_body = target_body.split_once('/').unwrap().1;
            let texture_relative = format!(
                "textures/{}",
                relative_body
                    .rsplit_once('/')
                    .unwrap()
                    .0
                    .to_ascii_lowercase()
            ) + "/execution_d.dds";
            let texture_path = texture_relative
                .split('/')
                .fold(source_root.clone(), |path, part| path.join(part));
            fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
            fs::write(texture_path, one_pixel_execution_dds()).unwrap();

            let texture_set = nif.add_block("BSShaderTextureSet", None);
            nif.blocks[texture_set]
                .fields
                .insert("Num Textures".to_string(), NifValue::UInt(9));
            nif.blocks[texture_set].fields.insert(
                "Textures".to_string(),
                NifValue::Array(
                    std::iter::once(NifValue::String(texture_relative.replace('/', "\\")))
                        .chain(std::iter::repeat_n(NifValue::String(String::new()), 8))
                        .collect(),
                ),
            );
            let shader = nif.add_block("BSLightingShaderProperty", None);
            nif.blocks[shader]
                .fields
                .insert("Name".to_string(), NifValue::String(String::new()));
            nif.blocks[shader]
                .fields
                .insert("Texture Set".to_string(), NifValue::Ref(texture_set as i32));
            nif.blocks[shader]
                .fields
                .insert("Shader Flags 1:SK".to_string(), NifValue::UInt(0));
            nif.blocks[shader]
                .fields
                .insert("Shader Flags 2:SK".to_string(), NifValue::UInt(0));
        }
        nif.rebuild_header();
        nif.save(Some(source_path)).unwrap();
        inputs.push(CreatureNifInput {
            role,
            source_data_relative_path: source_relative,
            source_owner: format!("fixture-{index}"),
            body_variant: (role == CreatureNifRole::Body).then(|| "base".to_string()),
        });
    }
    let staging = root.join("closure-stage");
    let receipt = stage_creature_nif_closure(&CreatureClosureRequest {
        source_game: "skyrimse".to_string(),
        source_data_root: source_root,
        private_staging_root: staging.clone(),
        target_namespace: "actors".to_string(),
        texture_fallbacks: std::collections::BTreeMap::new(),
        inputs,
    })
    .unwrap();
    SourceRigCreatureClosureInput {
        receipt_json: receipt.canonical_json().unwrap(),
        staged_data_root: staging.join("data"),
    }
}

fn execution_artifacts(
    recipe: &SourceRigExecutableRecipe,
    input_root: &std::path::Path,
) -> Vec<SourceRigArtifactInput> {
    fs::create_dir_all(input_root).unwrap();
    let skeleton_hkx = execution_fixture_skeleton(recipe);
    let ragdoll_hkx = execution_fixture_ragdoll(recipe);
    let mut required = vec![(
        SourceRigArtifactRole::AnimationSkeleton,
        SourceRigArtifactProvenance::ReconstructedSourceSkeleton,
        recipe.rig.animation_skeleton.path.clone(),
        skeleton_hkx.clone(),
    )];
    let clip_names = recipe
        .graph
        .roles
        .iter()
        .map(|role| role.clip_name.as_str())
        .chain(
            recipe
                .graph
                .overlays
                .iter()
                .map(|overlay| overlay.clip_name.as_str()),
        )
        .collect::<std::collections::BTreeSet<_>>();
    for clip_name in clip_names {
        let clip = recipe
            .rig
            .clips
            .iter()
            .find(|clip| clip.name == clip_name)
            .unwrap();
        required.push((
            SourceRigArtifactRole::AnimationClip {
                clip_name: clip.name.clone(),
            },
            SourceRigArtifactProvenance::ConvertedSourceClip,
            clip.path.clone(),
            execution_fixture_clip(recipe, clip),
        ));
    }
    if let RagdollDisposition::SourceOwned { runtime_path, .. } = &recipe.rig.ragdoll {
        required.push((
            SourceRigArtifactRole::Ragdoll,
            SourceRigArtifactProvenance::ReconstructedSourceRagdoll,
            runtime_path.clone(),
            ragdoll_hkx,
        ));
    }
    required
        .into_iter()
        .enumerate()
        .map(|(index, (role, provenance, runtime_path, bytes))| {
            let path = input_root.join(format!("artifact-{index}.bin"));
            fs::write(&path, &bytes).unwrap();
            SourceRigArtifactInput {
                path,
                receipt: SourceRigArtifactReceipt {
                    role,
                    provenance,
                    runtime_path,
                    byte_len: bytes.len() as u64,
                    blake3: blake3::hash(&bytes).to_hex().to_string(),
                },
            }
        })
        .collect()
}

fn bind_execution_recipe(
    mut recipe: SourceRigExecutableRecipe,
    artifacts: &[SourceRigArtifactInput],
    closure: &SourceRigCreatureClosureInput,
) -> SourceRigExecutableRecipe {
    let closure_receipt =
        nif_core_native::creature_closure::CreatureClosureReceipt::parse_and_validate(
            &closure.receipt_json,
        )
        .unwrap();
    let artifact_receipts = artifacts
        .iter()
        .map(|artifact| artifact.receipt.clone())
        .collect::<Vec<_>>();
    let artifact_requests = artifact_receipts
        .iter()
        .enumerate()
        .map(
            |(index, artifact)| SourceRigConvertedArtifactRequestReceipt {
                role: artifact.role.clone(),
                runtime_path: artifact.runtime_path.clone(),
                source_game: closure_receipt.source_game.clone(),
                source_evidence_blake3: blake3::hash(
                    format!("execution-evidence-{index}").as_bytes(),
                )
                .to_hex()
                .to_string(),
                conversion_request_blake3: blake3::hash(
                    format!("execution-request-{index}").as_bytes(),
                )
                .to_hex()
                .to_string(),
            },
        )
        .collect();
    recipe.bridge_receipt = Some(SourceRigRecipeBridgeReceipt {
        version: SOURCE_RIG_BRIDGE_VERSION,
        ledger_schema: "execution_fixture_ledger_v1".to_string(),
        ledger_blake3: blake3::hash(b"execution-fixture-ledger")
            .to_hex()
            .to_string(),
        ledger_canonical_json_blake3: blake3::hash(b"execution-fixture-canonical-ledger")
            .to_hex()
            .to_string(),
        source_identity: recipe.projection.source_primary_identity.clone(),
        family_id: recipe.batch_intent.family_id.clone(),
        creature_closure_request_blake3: closure_receipt.request_blake3.clone(),
        creature_closure_receipt_blake3: closure_receipt.receipt_hash.clone(),
        artifact_requests,
        artifact_receipts,
    });
    recipe.field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
            &recipe.rig,
            &recipe.graph,
            &recipe.projection,
            &recipe.key_plan,
            &recipe.batch_intent,
            recipe.bridge_receipt.as_ref(),
            &closure_receipt.source_game,
        );
    recipe.canonical_json().unwrap();
    recipe
}

fn bridged_recipe_input(
    source_games: &[&str],
    ledger_schema: &str,
    temp_root: &std::path::Path,
) -> (
    SourceRigRecipeBridgeInput,
    Vec<SourceRigArtifactInput>,
    SourceRigCreatureClosureInput,
) {
    use nif_core_native::creature_closure::{
        CreatureArtifactKind, CreatureClosureRequest, CreatureNifInput, CreatureNifRole,
        stage_creature_nif_closure,
    };
    use nif_core_native::model::NifFile;

    let source_game = source_games[0];
    let source_data = temp_root.join("source-data");
    let source_nif = source_data.join("meshes/B21_SkyrimWolf/Recipe/skeleton.nif");
    fs::create_dir_all(source_nif.parent().unwrap()).unwrap();
    let mut nif = NifFile::new(source_game);
    nif.rebuild_header();
    nif.save(Some(source_nif)).unwrap();
    let source_body = source_data.join("meshes/B21_SkyrimWolf/Recipe/body.nif");
    let mut body_nif = NifFile::new(source_game);
    body_nif.rebuild_header();
    body_nif.save(Some(source_body)).unwrap();
    let closure_stage = temp_root.join("closure-stage");
    let creature_closure = stage_creature_nif_closure(&CreatureClosureRequest {
        source_game: source_game.to_string(),
        source_data_root: source_data,
        private_staging_root: closure_stage.clone(),
        target_namespace: "Actors".to_string(),
        texture_fallbacks: std::collections::BTreeMap::new(),
        inputs: vec![
            CreatureNifInput {
                role: CreatureNifRole::Skeleton,
                source_data_relative_path: "meshes/B21_SkyrimWolf/Recipe/skeleton.nif".to_string(),
                source_owner: "synthetic-ledger".to_string(),
                body_variant: None,
            },
            CreatureNifInput {
                role: CreatureNifRole::Body,
                source_data_relative_path: "meshes/B21_SkyrimWolf/Recipe/body.nif".to_string(),
                source_owner: "synthetic-ledger".to_string(),
                body_variant: Some("default".to_string()),
            },
        ],
    })
    .unwrap();
    let closure_visual = creature_closure
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.kind == CreatureArtifactKind::Nif
                && artifact
                    .target_data_relative_path
                    .ends_with("/skeleton.nif")
        })
        .unwrap();
    let visual_runtime_path = closure_visual
        .target_data_relative_path
        .replace('/', "\\")
        .strip_prefix("meshes\\")
        .unwrap()
        .to_string();
    let closure_body = creature_closure
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.kind == CreatureArtifactKind::Nif
                && artifact.target_data_relative_path.ends_with("/body.nif")
        })
        .unwrap();
    let body_runtime_path = closure_body
        .target_data_relative_path
        .replace('/', "\\")
        .strip_prefix("meshes\\")
        .unwrap()
        .to_string();
    let mut base = executable_recipe_fixture("bridge", "Synthetic.esm", source_game);
    base.rig.visual_skeleton_nif = visual_runtime_path;
    base.projection.base.body_nif = body_runtime_path.clone();
    base.projection.body_nif_parts[0].body_nif = body_runtime_path;
    let bridge_skeleton_hkx = execution_fixture_skeleton(&base);
    let mut converted = vec![(
        SourceRigArtifactReceipt {
            role: SourceRigArtifactRole::AnimationSkeleton,
            provenance: SourceRigArtifactProvenance::ReconstructedSourceSkeleton,
            runtime_path: base.rig.animation_skeleton.path.clone(),
            byte_len: bridge_skeleton_hkx.len() as u64,
            blake3: blake3::hash(&bridge_skeleton_hkx).to_hex().to_string(),
        },
        bridge_skeleton_hkx,
    )];
    let clip_names = base
        .graph
        .roles
        .iter()
        .map(|role| role.clip_name.as_str())
        .chain(
            base.graph
                .overlays
                .iter()
                .map(|role| role.clip_name.as_str()),
        )
        .collect::<std::collections::BTreeSet<_>>();
    for clip_name in clip_names {
        let clip = base
            .rig
            .clips
            .iter()
            .find(|clip| clip.name == clip_name)
            .unwrap();
        let bridge_clip_hkx = execution_fixture_clip(&base, clip);
        converted.push((
            SourceRigArtifactReceipt {
                role: SourceRigArtifactRole::AnimationClip {
                    clip_name: clip.name.clone(),
                },
                provenance: SourceRigArtifactProvenance::ConvertedSourceClip,
                runtime_path: clip.path.clone(),
                byte_len: bridge_clip_hkx.len() as u64,
                blake3: blake3::hash(&bridge_clip_hkx).to_hex().to_string(),
            },
            bridge_clip_hkx,
        ));
    }

    let artifact_receipts = converted
        .iter()
        .map(|(receipt, _)| receipt.clone())
        .collect::<Vec<_>>();
    let artifact_requests = artifact_receipts
        .iter()
        .enumerate()
        .map(
            |(index, receipt)| SourceRigConvertedArtifactRequestReceipt {
                role: receipt.role.clone(),
                runtime_path: receipt.runtime_path.clone(),
                source_game: source_games[index % source_games.len()].to_string(),
                source_evidence_blake3: blake3::hash(format!("evidence-{index}").as_bytes())
                    .to_hex()
                    .to_string(),
                conversion_request_blake3: blake3::hash(format!("request-{index}").as_bytes())
                    .to_hex()
                    .to_string(),
            },
        )
        .collect::<Vec<_>>();
    let ledger_json = serde_json::to_string(&serde_json::json!({
        "schema": ledger_schema,
        "source_games": source_games,
        "family_id": base.batch_intent.family_id,
        "source_identity": base.projection.source_primary_identity,
    }))
    .unwrap();
    let selected_family = SourceRigSelectedFamilyEvidence {
        source_games: source_games
            .iter()
            .map(|game| (*game).to_string())
            .collect(),
        actual_root_bone: base.rig.animation_skeleton.bones[0].name.clone(),
        visual_skeleton_nif: base.rig.visual_skeleton_nif.clone(),
        visual_creature_closure_target_path: closure_visual.target_data_relative_path.clone(),
        animation_skeleton: base.rig.animation_skeleton.clone(),
        controller: base.rig.controller,
        graph: base.graph.clone(),
        clips: base.rig.clips.clone(),
        ragdoll: base.rig.ragdoll.clone(),
        race_data: base.projection.race_data.clone(),
    };
    let actor_action_records =
        required_actor_action_records(&base.graph, &base.rig.paths.root_behavior)
            .into_iter()
            .enumerate()
            .map(|(index, requirement)| CreatureActorActionRecordPlan {
                requirement,
                form_key: TargetFormKey::new(
                    0xDA0 + u32::try_from(index).unwrap(),
                    base.projection.base.target_plugin.clone(),
                ),
                editor_id: format!("B21_BridgeActorAction{index:02}"),
            })
            .collect::<Vec<_>>();
    let batch_intent = base
        .batch_intent
        .clone()
        .with_actor_action_dependencies(&actor_action_records);
    let mut input = SourceRigRecipeBridgeInput {
        pair_ledger: SourceRigPairLedgerEvidence {
            schema: ledger_schema.to_string(),
            canonical_json: ledger_json.clone(),
            ledger_blake3: blake3::hash(format!("strict-{ledger_schema}").as_bytes())
                .to_hex()
                .to_string(),
            canonical_json_blake3: blake3::hash(ledger_json.as_bytes()).to_hex().to_string(),
            source_identity: base.projection.source_primary_identity.clone(),
            family_id: base.batch_intent.family_id.clone(),
        },
        selected_family,
        creature_closure_request_blake3: creature_closure.request_blake3.clone(),
        creature_closure,
        artifact_requests,
        artifact_receipts,
        rig: base.rig,
        graph: base.graph,
        projection: base.projection,
        key_plan: base.key_plan,
        actor_action_records,
        batch_intent,
        field_receipts: Vec::new(),
    };
    input.field_receipts = input.required_field_receipts_from_policy("bridge_policy_v1");

    let inputs = converted
        .into_iter()
        .enumerate()
        .map(|(index, (receipt, bytes))| {
            let path = temp_root
                .join("converted-inputs")
                .join(format!("{index}.bin"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            SourceRigArtifactInput { path, receipt }
        })
        .collect();
    let closure_input = SourceRigCreatureClosureInput {
        receipt_json: input.creature_closure.canonical_json().unwrap(),
        staged_data_root: closure_stage.join("data"),
    };
    (input, inputs, closure_input)
}

pub(crate) fn evidence_bound_execution_fixture(
    temp_root: &std::path::Path,
) -> (
    SourceRigExecutableRecipe,
    Vec<SourceRigArtifactInput>,
    SourceRigCreatureClosureInput,
) {
    let (input, artifacts, closure) =
        bridged_recipe_input(&["fnv", "fo3"], "fnvfo3_creature_recipe_v1", temp_root);
    (
        build_source_rig_executable_recipe(input).unwrap(),
        artifacts,
        closure,
    )
}

#[test]
fn bridge_skyrim_and_fnvfo3_evidence_roundtrips_and_prepares_deterministically() {
    for (source_games, schema) in [
        (vec!["skyrimse"], "skyrimse_creature_recipe_v1"),
        (vec!["fnv", "fo3"], "fnvfo3_creature_recipe_v1"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (input, artifacts, closure) = bridged_recipe_input(&source_games, schema, temp.path());
        let mut shuffled = input.clone();
        shuffled.artifact_requests.reverse();
        shuffled.artifact_receipts.reverse();
        shuffled.selected_family.clips.reverse();
        shuffled.selected_family.graph.roles.reverse();
        shuffled.field_receipts = shuffled.required_field_receipts_from_policy("bridge_policy_v1");
        let first = build_source_rig_executable_recipe(input).unwrap();
        let second = build_source_rig_executable_recipe(shuffled).unwrap();
        assert_eq!(
            first.canonical_json().unwrap(),
            second.canonical_json().unwrap()
        );
        let loaded =
            SourceRigExecutableRecipe::from_json(&first.canonical_json().unwrap()).unwrap();
        assert_eq!(loaded, first);

        let recipe_path = temp.path().join("bridged-recipe.json");
        fs::write(&recipe_path, first.canonical_json().unwrap()).unwrap();
        let prepared = prepare_source_rig_execution(
            recipe_path,
            &artifacts,
            &closure,
            temp.path().join("private-execution"),
            &crate::sym::StringInterner::new(),
        )
        .unwrap();
        assert_eq!(
            prepared.receipt.recipe_blake3,
            first.stable_hash_blake3().unwrap()
        );
    }
}

#[test]
fn runtime_model_closure_is_recipe_bound_and_staged_with_execution() {
    let temp = tempfile::tempdir().unwrap();
    let (recipe, artifacts, closure) = evidence_bound_execution_fixture(temp.path());
    let target_bytes = nif_core_native::model::NifFile::new("fo4")
        .to_bytes()
        .unwrap();
    let target_hash = blake3::hash(&target_bytes).to_hex().to_string();
    let target_data_path = "meshes\\effects\\debris_fo4.nif".to_string();
    let source_data_path = "meshes\\effects\\debris.nif".to_string();
    let runtime_artifact = SourceRigRuntimeModelArtifactReceipt {
        kind: SourceRigRuntimeModelArtifactKind::Nif,
        source_paths: vec![source_data_path.clone()],
        target_data_path: target_data_path.clone(),
        byte_len: target_bytes.len() as u64,
        blake3: target_hash.clone(),
    };
    let runtime_closure = SourceRigRuntimeModelClosureReceipt {
        version: SOURCE_RIG_RUNTIME_MODEL_CLOSURE_VERSION,
        rows: vec![SourceRigRuntimeModelRowReceipt {
            key: SourceRigRuntimeModelRowKey {
                source_game: "fnv".to_string(),
                source_record: SourceCreatureIdentity {
                    namespace: "fnv".to_string(),
                    plugin: "FalloutNV.esm".to_string(),
                    local_form_id: 0x12_3456,
                },
                source_signature: "DEBR".to_string(),
                row_index: 0,
            },
            percentage: 100,
            has_collision: false,
            source_model_filename: "effects\\debris.nif".to_string(),
            source_data_path,
            source_byte_len: 6,
            source_blake3: blake3::hash(b"legacy").to_hex().to_string(),
            target_model_filename: "effects\\debris_fo4.nif".to_string(),
            target_data_path: target_data_path.clone(),
            target_byte_len: target_bytes.len() as u64,
            target_blake3: target_hash,
            artifacts: vec![runtime_artifact.clone()],
        }],
    };
    let recipe = recipe
        .with_runtime_model_closure(runtime_closure.clone())
        .unwrap();
    let recipe_path = temp.path().join("runtime-model-recipe.json");
    fs::write(&recipe_path, recipe.canonical_json().unwrap()).unwrap();
    let loaded =
        SourceRigExecutableRecipe::from_json(&fs::read_to_string(&recipe_path).unwrap()).unwrap();
    assert_eq!(loaded.runtime_model_closure, Some(runtime_closure.clone()));

    let runtime_source = temp.path().join("converted-runtime-model.nif");
    fs::write(&runtime_source, target_bytes).unwrap();
    let prepared = prepare_source_rig_execution_with_runtime_models(
        &recipe_path,
        &artifacts,
        &[SourceRigRuntimeModelArtifactInput {
            path: runtime_source,
            receipt: runtime_artifact,
        }],
        &closure,
        temp.path().join("runtime-model-execution"),
        &crate::sym::StringInterner::new(),
    )
    .unwrap();
    assert_eq!(
        prepared.receipt.runtime_model_closure_blake3,
        Some(runtime_closure.stable_hash_blake3().unwrap())
    );
    assert_eq!(prepared.receipt.runtime_model_artifacts.len(), 1);
    assert!(
        prepared
            .staging_root
            .join(target_data_path.replace('\\', "/"))
            .is_file()
    );
}

#[test]
fn bridge_rejects_mismatch_tamper_and_missing_evidence_before_recipe_output() {
    let temp = tempfile::tempdir().unwrap();
    let (input, _, _) =
        bridged_recipe_input(&["skyrimse"], "skyrimse_creature_recipe_v1", temp.path());

    let mut root_mismatch = input.clone();
    root_mismatch.selected_family.actual_root_bone =
        root_mismatch.rig.animation_skeleton.bones[1].name.clone();
    assert!(matches!(
        build_source_rig_executable_recipe(root_mismatch),
        Err(SourceRigBridgeError::Invalid {
            code: "family_root",
            ..
        })
    ));

    let mut ledger_tamper = input.clone();
    ledger_tamper.pair_ledger.canonical_json.push(' ');
    assert!(matches!(
        build_source_rig_executable_recipe(ledger_tamper),
        Err(SourceRigBridgeError::Invalid {
            code: "pair_ledger_hash",
            ..
        })
    ));

    let mut missing_artifact = input.clone();
    missing_artifact.artifact_receipts.pop();
    assert!(matches!(
        build_source_rig_executable_recipe(missing_artifact),
        Err(SourceRigBridgeError::Invalid {
            code: "converted_artifact_closure",
            ..
        })
    ));

    let mut closure_path_tamper = input.clone();
    closure_path_tamper
        .selected_family
        .visual_creature_closure_target_path
        .push_str(".tampered");
    closure_path_tamper.field_receipts =
        closure_path_tamper.required_field_receipts_from_policy("bridge_policy_v1");
    assert!(matches!(
        build_source_rig_executable_recipe(closure_path_tamper),
        Err(SourceRigBridgeError::Invalid {
            code: "creature_closure_path",
            ..
        })
    ));

    let mut missing_closure_request = input.clone();
    missing_closure_request
        .creature_closure_request_blake3
        .clear();
    missing_closure_request.field_receipts =
        missing_closure_request.required_field_receipts_from_policy("bridge_policy_v1");
    assert!(matches!(
        build_source_rig_executable_recipe(missing_closure_request),
        Err(SourceRigBridgeError::Invalid {
            code: "creature_closure_identity",
            ..
        })
    ));

    let mut missing_field = input;
    missing_field
        .field_receipts
        .retain(|receipt| receipt.field != "emitted.bptd.geometry_segment_index");
    assert!(matches!(
        build_source_rig_executable_recipe(missing_field),
        Err(SourceRigBridgeError::Recipe(_))
    ));
}

#[test]
fn bridge_rejects_arbitrary_closure_request_hash() {
    let temp = tempfile::tempdir().unwrap();
    let (mut input, _, _) =
        bridged_recipe_input(&["skyrimse"], "skyrimse_creature_recipe_v1", temp.path());
    input.creature_closure_request_blake3 = blake3::hash(b"arbitrary-but-well-formed-request")
        .to_hex()
        .to_string();
    input.field_receipts = input.required_field_receipts_from_policy("bridge_policy_v1");
    assert!(matches!(
        build_source_rig_executable_recipe(input),
        Err(SourceRigBridgeError::Invalid {
            code: "creature_closure_identity",
            ..
        })
    ));
}

#[test]
fn bridge_nonmelee_evidence_does_not_create_a_phantom_weap() {
    let temp = tempfile::tempdir().unwrap();
    let (mut input, _, _) =
        bridged_recipe_input(&["fnv", "fo3"], "fnvfo3_creature_recipe_v1", temp.path());
    input.graph.template = CreatureGraphTemplate::GroundRangedProjectile;
    let attack_role = input
        .graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::MeleeAttack)
        .unwrap();
    attack_role.role = CreatureClipRole::ProjectileAttack;
    let attack_event = attack_role.trigger_event.clone().unwrap();
    input
        .graph
        .explicit_events
        .iter_mut()
        .find(|event| event.name == attack_event)
        .unwrap()
        .usage = EventUsage::Generic;
    for declarations in [&mut input.rig.root, &mut input.rig.core] {
        declarations
            .events
            .iter_mut()
            .find(|event| event.name == attack_event)
            .unwrap()
            .usage = EventUsage::Generic;
    }
    let spell =
        CreatureTargetRecordReference::new("SPEL", TargetFormKey::new(0x2D0, "Fallout4.esm"));
    input.projection.attacks[0].projection = CreatureAttackRecordProjection::SpellAbility {
        spell: spell.clone(),
    };
    input.key_plan.melee_attacks.clear();
    input.key_plan.base.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureRecipe.esp");
    input.projection.base.form_keys.unarmed_weapon = input.key_plan.base.unarmed_weapon.clone();
    input.batch_intent.required_target_records = vec![spell];
    input.selected_family.graph = input.graph.clone();
    input.field_receipts = input.required_field_receipts_from_policy("bridge_policy_v1");

    let recipe = build_source_rig_executable_recipe(input).unwrap();
    let closure = recipe
        .rebuild_projection_closure(&crate::sym::StringInterner::new())
        .unwrap();
    assert!(
        !closure
            .closure
            .records
            .iter()
            .any(|record| record.sig.as_str() == "WEAP")
    );
}

#[test]
fn prepared_execution_rejects_missing_bridge_without_output() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    assert!(recipe.bridge_receipt.is_none());
    let temp = tempfile::tempdir().unwrap();
    let recipe_path = temp.path().join("unbridged-recipe.json");
    fs::write(&recipe_path, recipe.canonical_json().unwrap()).unwrap();
    let closure = execution_nif_closure(&recipe, &temp.path().join("nif-closure"));
    let stage = temp.path().join("missing-bridge-stage");

    assert!(matches!(
        prepare_source_rig_execution(
            &recipe_path,
            &[],
            &closure,
            &stage,
            &crate::sym::StringInterner::new(),
        ),
        Err(SourceRigExecutionError::MissingBridgeReceipt)
    ));
    assert!(!stage.exists());
}

#[test]
fn prepared_execution_rejects_wrong_valid_skeleton_and_clip_without_output() {
    let temp = tempfile::tempdir().unwrap();
    let base = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let closure = execution_nif_closure(&base, &temp.path().join("nif-closure"));
    let interner = crate::sym::StringInterner::new();

    let mut skeleton_artifacts = execution_artifacts(&base, &temp.path().join("wrong-skeleton"));
    let mut wrong_skeleton_recipe = base.clone();
    wrong_skeleton_recipe.rig.animation_skeleton.runtime_name =
        "DifferentButValidRuntimeSkeleton".to_string();
    let wrong_skeleton = execution_fixture_skeleton(&wrong_skeleton_recipe);
    let skeleton = skeleton_artifacts
        .iter_mut()
        .find(|artifact| artifact.receipt.role == SourceRigArtifactRole::AnimationSkeleton)
        .unwrap();
    fs::write(&skeleton.path, &wrong_skeleton).unwrap();
    skeleton.receipt.byte_len = wrong_skeleton.len() as u64;
    skeleton.receipt.blake3 = blake3::hash(&wrong_skeleton).to_hex().to_string();
    let skeleton_recipe = bind_execution_recipe(base.clone(), &skeleton_artifacts, &closure);
    let skeleton_recipe_path = temp.path().join("wrong-skeleton-recipe.json");
    fs::write(
        &skeleton_recipe_path,
        skeleton_recipe.canonical_json().unwrap(),
    )
    .unwrap();
    let skeleton_stage = temp.path().join("wrong-skeleton-stage");
    assert!(matches!(
        prepare_source_rig_execution(
            &skeleton_recipe_path,
            &skeleton_artifacts,
            &closure,
            &skeleton_stage,
            &interner,
        ),
        Err(SourceRigExecutionError::Artifact {
            code: "skeleton_semantics",
            ..
        })
    ));
    assert!(!skeleton_stage.exists());

    let mut clip_artifacts = execution_artifacts(&base, &temp.path().join("wrong-clip"));
    let clip_name = clip_artifacts
        .iter()
        .find_map(|artifact| match &artifact.receipt.role {
            SourceRigArtifactRole::AnimationClip { clip_name } => Some(clip_name.clone()),
            _ => None,
        })
        .unwrap();
    let mut wrong_clip_recipe = base.clone();
    let wrong_clip_decl = {
        let declaration = wrong_clip_recipe
            .rig
            .clips
            .iter_mut()
            .find(|clip| clip.name == clip_name)
            .unwrap();
        declaration.binding.original_skeleton_name = "DifferentValidSourceSkeleton".to_string();
        declaration.clone()
    };
    let wrong_clip = execution_fixture_clip(&wrong_clip_recipe, &wrong_clip_decl);
    let clip = clip_artifacts
        .iter_mut()
        .find(|artifact| {
            matches!(
                &artifact.receipt.role,
                SourceRigArtifactRole::AnimationClip { clip_name: name } if name == &clip_name
            )
        })
        .unwrap();
    fs::write(&clip.path, &wrong_clip).unwrap();
    clip.receipt.byte_len = wrong_clip.len() as u64;
    clip.receipt.blake3 = blake3::hash(&wrong_clip).to_hex().to_string();
    let clip_recipe = bind_execution_recipe(base, &clip_artifacts, &closure);
    let clip_recipe_path = temp.path().join("wrong-clip-recipe.json");
    fs::write(&clip_recipe_path, clip_recipe.canonical_json().unwrap()).unwrap();
    let clip_stage = temp.path().join("wrong-clip-stage");
    assert!(matches!(
        prepare_source_rig_execution(
            &clip_recipe_path,
            &clip_artifacts,
            &closure,
            &clip_stage,
            &interner,
        ),
        Err(SourceRigExecutionError::Artifact {
            code: "clip_semantics",
            ..
        })
    ));
    assert!(!clip_stage.exists());
}

#[test]
fn prepared_execution_is_private_complete_and_deterministic() {
    let base = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let temp = tempfile::tempdir().unwrap();
    let artifacts = execution_artifacts(&base, &temp.path().join("inputs"));
    let closure = execution_nif_closure(&base, &temp.path().join("nif-closure"));
    let recipe = bind_execution_recipe(base, &artifacts, &closure);
    let recipe_path = temp.path().join("recipe.json");
    fs::write(&recipe_path, recipe.canonical_json().unwrap()).unwrap();
    let interner = crate::sym::StringInterner::new();

    let first = prepare_source_rig_execution(
        &recipe_path,
        &artifacts,
        &closure,
        temp.path().join("private-a"),
        &interner,
    )
    .unwrap();
    let mut reversed = artifacts.clone();
    reversed.reverse();
    let second = prepare_source_rig_execution(
        &recipe_path,
        &reversed,
        &closure,
        temp.path().join("private-b"),
        &interner,
    )
    .unwrap();

    assert_eq!(first.receipt, second.receipt);
    assert_eq!(
        first.receipt.stable_hash_blake3().unwrap(),
        second.receipt.stable_hash_blake3().unwrap()
    );
    assert_eq!(first.receipt.scaffold_artifacts.len(), 4);
    assert_eq!(first.receipt.record_family.family_id, "recipe-family");
    assert_eq!(first.record_batch.family_id, "recipe-family");
    for artifact in &first.receipt.scaffold_artifacts {
        assert!(
            artifact
                .runtime_path
                .split('\\')
                .fold(first.staging_root.clone(), |path, part| path.join(part))
                .is_file()
        );
    }
}

#[test]
fn prepared_execution_rejects_missing_and_tampered_artifacts_without_output() {
    let base = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let temp = tempfile::tempdir().unwrap();
    let mut artifacts = execution_artifacts(&base, &temp.path().join("inputs"));
    let closure = execution_nif_closure(&base, &temp.path().join("nif-closure"));
    let recipe = bind_execution_recipe(base, &artifacts, &closure);
    let recipe_path = temp.path().join("recipe.json");
    fs::write(&recipe_path, recipe.canonical_json().unwrap()).unwrap();
    let missing = artifacts.pop().unwrap();
    let missing_stage = temp.path().join("missing-stage");
    let interner = crate::sym::StringInterner::new();

    assert!(matches!(
        prepare_source_rig_execution(
            &recipe_path,
            &artifacts,
            &closure,
            &missing_stage,
            &interner
        ),
        Err(SourceRigExecutionError::Artifact {
            code: "missing_artifact",
            ..
        })
    ));
    assert!(!missing_stage.exists());

    artifacts.push(missing);
    fs::write(&artifacts[0].path, b"tampered").unwrap();
    let tampered_stage = temp.path().join("tampered-stage");
    assert!(matches!(
        prepare_source_rig_execution(
            &recipe_path,
            &artifacts,
            &closure,
            &tampered_stage,
            &interner
        ),
        Err(SourceRigExecutionError::Artifact {
            code: "artifact_content_mismatch",
            ..
        })
    ));
    assert!(!tampered_stage.exists());

    let closure_receipt =
        nif_core_native::creature_closure::CreatureClosureReceipt::parse_and_validate(
            &closure.receipt_json,
        )
        .unwrap();
    let nif = closure_receipt
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.kind == nif_core_native::creature_closure::CreatureArtifactKind::Nif
        })
        .unwrap();
    let nif_path = nif
        .target_data_relative_path
        .split('/')
        .fold(closure.staged_data_root.clone(), |path, part| {
            path.join(part)
        });
    fs::write(nif_path, b"fake NIF").unwrap();
    let closure_stage = temp.path().join("closure-stage");
    let fresh_artifacts = execution_artifacts(&recipe, &temp.path().join("fresh-inputs"));
    assert!(matches!(
        prepare_source_rig_execution(
            &recipe_path,
            &fresh_artifacts,
            &closure,
            &closure_stage,
            &interner
        ),
        Err(SourceRigExecutionError::Artifact {
            code: "creature_closure_content_mismatch",
            ..
        })
    ));
    assert!(!closure_stage.exists());
}

#[test]
fn prepared_execution_rejects_missing_material_and_tampered_closure_without_output() {
    use nif_core_native::creature_closure::{CreatureArtifactKind, CreatureClosureReceipt};

    let base = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let temp = tempfile::tempdir().unwrap();
    let artifacts = execution_artifacts(&base, &temp.path().join("inputs"));
    let closure = execution_nif_closure_with_material(&base, &temp.path().join("nif-closure"));
    let recipe = bind_execution_recipe(base, &artifacts, &closure);
    let recipe_path = temp.path().join("recipe.json");
    fs::write(&recipe_path, recipe.canonical_json().unwrap()).unwrap();
    let receipt = CreatureClosureReceipt::parse_and_validate(&closure.receipt_json).unwrap();
    let material = receipt
        .artifacts
        .iter()
        .find(|artifact| {
            matches!(
                artifact.kind,
                CreatureArtifactKind::Bgsm | CreatureArtifactKind::Bgem
            )
        })
        .unwrap();
    let material_path = material
        .target_data_relative_path
        .split('/')
        .fold(closure.staged_data_root.clone(), |path, part| {
            path.join(part)
        });
    fs::remove_file(material_path).unwrap();
    let missing_material_stage = temp.path().join("missing-material-stage");
    let interner = crate::sym::StringInterner::new();
    assert!(
        prepare_source_rig_execution(
            &recipe_path,
            &artifacts,
            &closure,
            &missing_material_stage,
            &interner,
        )
        .is_err()
    );
    assert!(!missing_material_stage.exists());

    let mut tampered_closure =
        execution_nif_closure(&recipe, &temp.path().join("tampered-closure"));
    tampered_closure.receipt_json = tampered_closure
        .receipt_json
        .replace("\"target_game\":\"fo4\"", "\"target_game\":\"fnv\"");
    let tampered_receipt_stage = temp.path().join("tampered-receipt-stage");
    assert!(matches!(
        prepare_source_rig_execution(
            &recipe_path,
            &artifacts,
            &tampered_closure,
            &tampered_receipt_stage,
            &interner,
        ),
        Err(SourceRigExecutionError::Artifact {
            code: "invalid_creature_closure_receipt",
            ..
        })
    ));
    assert!(!tampered_receipt_stage.exists());
}

#[test]
fn source_owned_ragdoll_recipe_roundtrips_and_is_required_for_execution() {
    let mut recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    recipe.rig = with_source_owned_ragdoll(recipe.rig);
    let ragdoll = execution_fixture_ragdoll(&recipe);
    if let RagdollDisposition::SourceOwned { receipt, .. } = &mut recipe.rig.ragdoll {
        receipt.byte_len = ragdoll.len() as u64;
        receipt.blake3 = blake3::hash(&ragdoll).to_hex().to_string();
    }
    recipe.field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
            &recipe.rig,
            &recipe.graph,
            &recipe.projection,
            &recipe.key_plan,
            &recipe.batch_intent,
            recipe.bridge_receipt.as_ref(),
            "fallout_new_vegas",
        );
    let json = recipe.canonical_json().unwrap();
    let loaded = SourceRigExecutableRecipe::from_json(&json).unwrap();
    assert!(matches!(
        loaded.rig.ragdoll,
        RagdollDisposition::SourceOwned { .. }
    ));

    let temp = tempfile::tempdir().unwrap();
    let mut artifacts = execution_artifacts(&loaded, &temp.path().join("inputs"));
    let closure = execution_nif_closure(&loaded, &temp.path().join("nif-closure"));
    let loaded = bind_execution_recipe(loaded, &artifacts, &closure);
    let recipe_path = temp.path().join("recipe.json");
    fs::write(&recipe_path, loaded.canonical_json().unwrap()).unwrap();
    artifacts.retain(|artifact| artifact.receipt.role != SourceRigArtifactRole::Ragdoll);
    let stage = temp.path().join("private-stage");
    let interner = crate::sym::StringInterner::new();
    assert!(matches!(
        prepare_source_rig_execution(&recipe_path, &artifacts, &closure, &stage, &interner),
        Err(SourceRigExecutionError::Artifact {
            code: "missing_artifact",
            ..
        })
    ));
    assert!(!stage.exists());

    let complete = execution_artifacts(&loaded, &temp.path().join("complete-inputs"));
    let prepared = prepare_source_rig_execution(
        &recipe_path,
        &complete,
        &closure,
        temp.path().join("complete-stage"),
        &interner,
    )
    .unwrap();
    assert!(
        prepared
            .receipt
            .source_artifacts
            .iter()
            .any(|artifact| artifact.role == SourceRigArtifactRole::Ragdoll)
    );
}

#[test]
fn deferred_ragdoll_and_skeleton_mismatch_fail_without_staging() {
    let mut deferred = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    deferred.rig.ragdoll = RagdollDisposition::deferred();
    assert!(
        deferred
            .rig
            .validate()
            .unwrap_err()
            .iter()
            .any(|error| error.code == "ragdoll_deferred")
    );
    assert!(deferred.validate().is_err());
    let temp = tempfile::tempdir().unwrap();
    let deferred_path = temp.path().join("deferred.json");
    fs::write(&deferred_path, serde_json::to_string(&deferred).unwrap()).unwrap();
    let deferred_stage = temp.path().join("deferred-stage");
    let deferred_closure = execution_nif_closure(&deferred, &temp.path().join("deferred-closure"));
    let interner = crate::sym::StringInterner::new();
    assert!(matches!(
        prepare_source_rig_execution(
            &deferred_path,
            &[],
            &deferred_closure,
            &deferred_stage,
            &interner
        ),
        Err(SourceRigExecutionError::Recipe(_))
    ));
    assert!(!deferred_stage.exists());

    let mut recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    recipe.rig = with_source_owned_ragdoll(recipe.rig);
    let mut mismatched_recipe = recipe.clone();
    mismatched_recipe.rig.animation_skeleton.bones[3].name = "MismatchedHead".to_string();
    let ragdoll = execution_fixture_ragdoll(&mismatched_recipe);
    if let RagdollDisposition::SourceOwned { receipt, .. } = &mut recipe.rig.ragdoll {
        receipt.byte_len = ragdoll.len() as u64;
        receipt.blake3 = blake3::hash(&ragdoll).to_hex().to_string();
    }
    recipe.field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
            &recipe.rig,
            &recipe.graph,
            &recipe.projection,
            &recipe.key_plan,
            &recipe.batch_intent,
            recipe.bridge_receipt.as_ref(),
            "fallout_new_vegas",
        );
    let mut artifacts = execution_artifacts(&recipe, &temp.path().join("mismatch-inputs"));
    let ragdoll_artifact = artifacts
        .iter_mut()
        .find(|artifact| artifact.receipt.role == SourceRigArtifactRole::Ragdoll)
        .unwrap();
    fs::write(&ragdoll_artifact.path, &ragdoll).unwrap();
    ragdoll_artifact.receipt.byte_len = ragdoll.len() as u64;
    ragdoll_artifact.receipt.blake3 = blake3::hash(&ragdoll).to_hex().to_string();
    let mismatch_closure = execution_nif_closure(&recipe, &temp.path().join("mismatch-closure"));
    let recipe = bind_execution_recipe(recipe, &artifacts, &mismatch_closure);
    let mismatch_path = temp.path().join("mismatch.json");
    fs::write(&mismatch_path, recipe.canonical_json().unwrap()).unwrap();
    let mismatch_stage = temp.path().join("mismatch-stage");
    assert!(matches!(
        prepare_source_rig_execution(
            &mismatch_path,
            &artifacts,
            &mismatch_closure,
            &mismatch_stage,
            &interner
        ),
        Err(SourceRigExecutionError::Artifact {
            code: "ragdoll_skeleton_mismatch",
            ..
        })
    ));
    assert!(!mismatch_stage.exists());
}

#[test]
fn executable_recipe_rejects_missing_profile_or_race_data_receipt() {
    let mut recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    recipe
        .field_receipts
        .retain(|receipt| receipt.field != "projection.race_data.male_height");

    assert!(matches!(
        recipe.validate(),
        Err(SourceRigRecipeError::Invalid {
            code: "missing_field_receipt",
            ..
        })
    ));

    let mut fixed = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    fixed
        .field_receipts
        .retain(|receipt| receipt.field != "emitted.bptd.geometry_segment_index");
    assert!(matches!(
        fixed.validate(),
        Err(SourceRigRecipeError::Invalid {
            code: "missing_field_receipt",
            ..
        })
    ));
}

#[test]
fn executable_recipe_requires_actual_root_bone_and_fixed_segment_32() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    assert!(
        recipe
            .field_receipts
            .iter()
            .any(|receipt| receipt.field == "emitted.bptd.geometry_segment_index")
    );
    let interner = crate::sym::StringInterner::new();
    let closure = recipe.rebuild_projection_closure(&interner).unwrap();
    let bptd = closure.closure.record("BPTD").unwrap();
    assert_eq!(
        struct_u8(
            record_field(bptd, "BPND"),
            "geometry_segment_index",
            &interner,
        ),
        32
    );

    let mut invalid = recipe;
    let CreatureBodyPartProjection::RootOnly32 {
        node, vats_target, ..
    } = &mut invalid.projection.body_parts;
    *node = "Actors\\Creature\\skeleton.nif".to_string();
    *vats_target = node.clone();
    invalid.rig.animation_skeleton.bones.push(BoneDecl {
        name: node.clone(),
        parent_index: Some(0),
    });
    assert!(matches!(
        invalid.validate(),
        Err(SourceRigRecipeError::Invalid {
            code: "root_bone_name",
            ..
        })
    ));

    let mut child = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let child_name = child.rig.animation_skeleton.bones[1].name.clone();
    let CreatureBodyPartProjection::RootOnly32 {
        node, vats_target, ..
    } = &mut child.projection.body_parts;
    *node = child_name.clone();
    *vats_target = child_name;
    assert!(matches!(
        child.validate(),
        Err(SourceRigRecipeError::Invalid {
            code: "root_bone_name",
            ..
        })
    ));
}

#[test]
fn executable_nonmelee_recipe_has_no_generated_weap() {
    let mut recipe = executable_recipe_fixture("fo3", "Fallout3.esm", "fallout_3");
    recipe.graph.template = CreatureGraphTemplate::GroundRangedProjectile;
    let attack_role = recipe
        .graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::MeleeAttack)
        .unwrap();
    attack_role.role = CreatureClipRole::ProjectileAttack;
    let attack_event = attack_role.trigger_event.clone().unwrap();
    recipe
        .graph
        .explicit_events
        .iter_mut()
        .find(|event| event.name == attack_event)
        .unwrap()
        .usage = EventUsage::Generic;
    for declarations in [&mut recipe.rig.root, &mut recipe.rig.core] {
        declarations
            .events
            .iter_mut()
            .find(|event| event.name == attack_event)
            .unwrap()
            .usage = EventUsage::Generic;
    }
    let spell =
        CreatureTargetRecordReference::new("SPEL", TargetFormKey::new(0x2D0, "Fallout4.esm"));
    recipe.projection.attacks[0].projection = CreatureAttackRecordProjection::SpellAbility {
        spell: spell.clone(),
    };
    recipe.key_plan.melee_attacks.clear();
    recipe.key_plan.base.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureRecipe.esp");
    recipe.projection.base.form_keys.unarmed_weapon = recipe.key_plan.base.unarmed_weapon.clone();
    recipe.batch_intent.required_target_records = vec![spell];
    recipe.field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
            &recipe.rig,
            &recipe.graph,
            &recipe.projection,
            &recipe.key_plan,
            &recipe.batch_intent,
            recipe.bridge_receipt.as_ref(),
            "fallout_3",
        );

    recipe.validate().unwrap();
    let interner = crate::sym::StringInterner::new();
    let closure = recipe.rebuild_projection_closure(&interner).unwrap();
    assert!(
        !closure
            .closure
            .records
            .iter()
            .any(|record| record.sig.as_str() == "WEAP")
    );
}

#[test]
fn executable_recipe_retains_fnv_and_fo3_field_provenance() {
    let mut recipe = executable_recipe_fixture("fnvfo3", "FalloutNV.esm", "fallout_new_vegas");
    for (index, receipt) in recipe.field_receipts.iter_mut().enumerate() {
        if index % 2 == 1
            && let SourceRigFieldDecision::Source { source_game, .. } = &mut receipt.decision
        {
            *source_game = "fallout_3".to_string();
        }
    }
    let loaded = SourceRigExecutableRecipe::from_json(&recipe.canonical_json().unwrap()).unwrap();
    let games = loaded
        .field_receipts
        .iter()
        .filter_map(|receipt| match &receipt.decision {
            SourceRigFieldDecision::Source { source_game, .. } => Some(source_game.as_str()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        games,
        ["fallout_3", "fallout_new_vegas"].into_iter().collect()
    );
}

fn corpus_rig_family(id: &str) -> RigFamily {
    RigFamily {
        id: id.to_string(),
        root_node: "Root".to_string(),
        race_data: RaceDataMapping::Mapped {
            target: grounded_race_data(),
        },
    }
}

fn corpus_motion_set(id: &str) -> MotionSet {
    MotionSet {
        id: id.to_string(),
        graph_template: CreatureGraphTemplate::GroundMelee,
        idle_clip: "idle".to_string(),
        locomotion_clips: std::collections::BTreeMap::from([
            ("walk_forward".to_string(), "walk".to_string()),
            ("turn_left_90".to_string(), "left".to_string()),
            ("turn_right_90".to_string(), "right".to_string()),
        ]),
        attacks: vec![MotionAttack {
            id: "bite".to_string(),
            event: "meleeBite".to_string(),
            clip: "bite".to_string(),
        }],
        attack_kinds: std::collections::BTreeMap::new(),
        required_overlays: Vec::new(),
        overlays: Vec::new(),
        required_rigs: Vec::new(),
        rigs: Vec::new(),
    }
}

fn corpus_candidate(
    namespace: &str,
    plugin: &str,
    local_form_id: u32,
    output_slug: &str,
) -> CreatureCorpusCandidate {
    let race_identity = source_identity(namespace, plugin, local_form_id);
    let primary_record_identity = source_identity(namespace, plugin, local_form_id + 1);
    CreatureCorpusCandidate {
        source_identity: race_identity,
        primary_record_identity: primary_record_identity.clone(),
        output_slug: output_slug.to_string(),
        rig_family: "grounded-quadruped".to_string(),
        motion_set: "quadruped-basic".to_string(),
        record_variants: vec![RecordVariant {
            source_identity: primary_record_identity,
            output_slug: "base".to_string(),
            display_name: output_slug.replace('-', " "),
            body_nif: format!("Actors\\{output_slug}\\CharacterAssets\\body.nif"),
            level: 1,
            health: 25,
            action_points: 50,
            primary: true,
            attack_ids: vec!["bite".to_string()],
        }],
        preflight_rejections: Vec::new(),
    }
}

#[test]
fn creature_corpus_plan_is_deterministic_and_accounted() {
    let wolf = corpus_candidate("skyrimse", "Skyrim.esm", 0x01_1234, "wolf");
    let gecko = corpus_candidate("fnv", "FalloutNV.esm", 0x02_2345, "gecko");
    let families = vec![corpus_rig_family("grounded-quadruped")];
    let motions = vec![corpus_motion_set("quadruped-basic")];

    let forward = CreatureCorpusPlan::build(
        families.clone(),
        motions.clone(),
        vec![wolf.clone(), gecko.clone()],
    )
    .unwrap();
    let reverse = CreatureCorpusPlan::build(families, motions, vec![gecko, wolf]).unwrap();

    assert_eq!(
        forward.canonical_json().unwrap(),
        reverse.canonical_json().unwrap()
    );
    assert_eq!(forward.candidate_count, 2);
    assert_eq!(forward.planned.len(), 2);
    assert!(forward.rejected.is_empty());
    assert_ne!(
        forward.planned[0].source_identity,
        forward.planned[0].primary_record_identity
    );
    assert_eq!(
        forward.candidate_count,
        forward.planned.len() + forward.rejected.len()
    );
}

#[test]
fn creature_corpus_plan_rejects_every_colliding_candidate() {
    let first = corpus_candidate("fnv", "FalloutNV.esm", 0x02_2345, "gecko");
    let mut second = first.clone();
    second.record_variants[0].display_name = "Gecko Variant".to_string();

    let plan = CreatureCorpusPlan::build(
        vec![corpus_rig_family("grounded-quadruped")],
        vec![corpus_motion_set("quadruped-basic")],
        vec![first, second],
    )
    .unwrap();

    assert_eq!(plan.candidate_count, 2);
    assert!(plan.planned.is_empty());
    assert_eq!(plan.rejected.len(), 2);
    assert!(plan.rejected.iter().all(|entry| {
        entry.reasons.iter().any(|reason| {
            matches!(
                reason,
                CreatureRejectionReason::DuplicateSourceIdentity { .. }
                    | CreatureRejectionReason::DuplicateOutputSlug { .. }
            )
        })
    }));
}

#[test]
fn creature_corpus_plan_validation_rejects_tampered_accounting() {
    let mut plan = CreatureCorpusPlan::build(
        vec![corpus_rig_family("grounded-quadruped")],
        vec![corpus_motion_set("quadruped-basic")],
        vec![corpus_candidate("fnv", "FalloutNV.esm", 0x02_2345, "gecko")],
    )
    .unwrap();
    plan.candidate_count += 1;

    assert!(matches!(
        plan.validate(),
        Err(CreatureCorpusPlanError::Accounting { .. })
    ));
}

#[test]
fn creature_corpus_plan_ledgers_missing_family_attack_and_race_data() {
    let mut missing_family = corpus_candidate("fnv", "FalloutNV.esm", 0x02_2345, "gecko");
    missing_family.rig_family = "unmapped-family".to_string();
    missing_family.record_variants[0].attack_ids = vec!["unmapped-attack".to_string()];
    let missing_race_family = RigFamily {
        id: "grounded-quadruped".to_string(),
        root_node: "Root".to_string(),
        race_data: RaceDataMapping::Missing {
            fields: vec![Fo4RaceDataField::MovementFlags, Fo4RaceDataField::Heights],
        },
    };
    let race_candidate = corpus_candidate("skyrimse", "Skyrim.esm", 0x01_1234, "wolf");

    let plan = CreatureCorpusPlan::build(
        vec![missing_race_family],
        vec![corpus_motion_set("quadruped-basic")],
        vec![missing_family, race_candidate],
    )
    .unwrap();

    assert!(plan.planned.is_empty());
    assert_eq!(plan.rejected.len(), 2);
    assert!(plan.rejected.iter().any(|entry| {
        entry
            .reasons
            .iter()
            .any(|reason| matches!(reason, CreatureRejectionReason::MissingRigFamily { .. }))
            && entry.reasons.iter().any(|reason| {
                matches!(reason, CreatureRejectionReason::MissingAttackMapping { .. })
            })
    }));
    assert!(plan.rejected.iter().any(|entry| {
        entry
            .reasons
            .iter()
            .any(|reason| matches!(reason, CreatureRejectionReason::MissingRaceData { .. }))
    }));
}

#[test]
fn creature_corpus_plan_preserves_typed_upstream_rejection() {
    let mut candidate = corpus_candidate("fnv", "FalloutNV.esm", 0x02_2345, "gecko");
    candidate.preflight_rejections = vec![CreatureRejectionReason::UpstreamCatalogRejected {
        catalog: "fnv-creature-races".to_string(),
        reason_code: "missing-body-model".to_string(),
        detail: "catalog winner has no source body NIF".to_string(),
    }];

    let plan = CreatureCorpusPlan::build(
        vec![corpus_rig_family("grounded-quadruped")],
        vec![corpus_motion_set("quadruped-basic")],
        vec![candidate],
    )
    .unwrap();

    assert!(plan.planned.is_empty());
    assert!(matches!(
        plan.rejected[0].reasons.as_slice(),
        [CreatureRejectionReason::UpstreamCatalogRejected { reason_code, .. }]
            if reason_code == "missing-body-model"
    ));
}

#[test]
fn record_projection_preserves_primary_identity_variants_attacks_and_race_data() {
    let (mut rig, mvp) = wolf_mvp();
    let second_event = EventDecl {
        name: "meleeWolfAttack2".to_string(),
        usage: EventUsage::MeleeAttack,
        flags: 0,
    };
    rig.root.events.push(second_event.clone());
    rig.core.events.push(second_event.clone());
    let mut second_clip = rig
        .clips
        .iter()
        .find(|clip| clip.name == mvp.attack_1_clip)
        .unwrap()
        .clone();
    second_clip.name = "attack2".to_string();
    second_clip.path = "Actors\\B21_SkyrimWolf\\Animations\\attack2.hkx".to_string();
    rig.clips.push(second_clip);
    let mut graph = CapabilityGraphManifest::from_mvp(&mvp, &MvpMotionManifest::default());
    graph.explicit_events.push(second_event);
    graph.roles.push(CapabilityClipRole {
        role: CreatureClipRole::MeleeAttack,
        state_name: "Attack2".to_string(),
        clip_name: "attack2".to_string(),
        generator: CapabilityRoleGenerator::Single,
        trigger_event: Some("meleeWolfAttack2".to_string()),
        trigger_aliases: Vec::new(),
        motion: ClipMotionPolicy::default(),
    });
    let base = record_manifest(&rig.creature_name, "B21_CreatureMVP.esp", 0xA00);
    let primary_source = source_identity("skyrimse", "Skyrim.esm", 0x01_1234);
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![
            CreatureNpcRecordVariant {
                source_identity: primary_source.clone(),
                form_key: base.form_keys.npc.clone(),
                editor_id: base.editor_ids.npc.clone(),
                display_name: "Wolf".to_string(),
                primary: true,
                level: 5,
                health: 100,
                action_points: 75,
                npc_inventory: None,
                npc_equipment: None,
                npc_spells: None,
                npc_death_item: None,
            },
            CreatureNpcRecordVariant {
                source_identity: source_identity("skyrimse", "Skyrim.esm", 0x01_1235),
                form_key: TargetFormKey::new(0xA06, "B21_CreatureMVP.esp"),
                editor_id: "B21_SkyrimWolfAlphaNPC".to_string(),
                display_name: "Alpha Wolf".to_string(),
                primary: false,
                level: 12,
                health: 220,
                action_points: 90,
                npc_inventory: None,
                npc_equipment: None,
                npc_spells: None,
                npc_death_item: None,
            },
        ],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: vec![
            CreatureAttackRecordVariant {
                id: "Wolf Bite".to_string(),
                event: mvp.melee_event.clone(),
                primary: true,
                projection: CreatureAttackRecordProjection::MeleeUnarmed {
                    weapon_form_key: base.form_keys.unarmed_weapon.clone(),
                    weapon_editor_id: base.editor_ids.unarmed_weapon.clone(),
                    damage: 15,
                    reach: 0.7,
                    attack_seconds: 0.8,
                },
                damage_multiplier: 1.0,
                chance: 1.0,
                strike_angle: 35.0,
                action_point_cost: 20.0,
                target_data: CreatureAttackTargetData::default(),
            },
            CreatureAttackRecordVariant {
                id: "Wolf Lunge".to_string(),
                event: "meleeWolfAttack2".to_string(),
                primary: false,
                projection: CreatureAttackRecordProjection::MeleeUnarmed {
                    weapon_form_key: TargetFormKey::new(0xA07, "B21_CreatureMVP.esp"),
                    weapon_editor_id: "B21_SkyrimWolfLunge".to_string(),
                    damage: 25,
                    reach: 1.0,
                    attack_seconds: 1.1,
                },
                damage_multiplier: 1.5,
                chance: 0.5,
                strike_angle: 25.0,
                action_point_cost: 30.0,
                target_data: CreatureAttackTargetData::default(),
            },
        ],
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();

    assert_eq!(emitted.source_primary_identity, primary_source);
    assert_eq!(emitted.projected_identities.len(), 2);
    assert_eq!(emitted.closure.records.len(), 8);
    assert!(record_field(emitted.closure.record("RACE").unwrap(), "DATA") != &FieldValue::None);
    assert_eq!(
        record_string_values(emitted.closure.record("RACE").unwrap(), "ATKE", &interner),
        ["meleeWolfAttack1", "meleeWolfAttack2"]
    );
    assert_eq!(
        emitted
            .closure
            .records
            .iter()
            .filter(|record| record.sig.as_str() == "NPC_")
            .count(),
        2
    );
    assert_eq!(
        emitted
            .closure
            .records
            .iter()
            .filter(|record| record.sig.as_str() == "WEAP")
            .count(),
        2
    );
}

#[test]
fn passive_ground_projection_emits_no_attack_artifacts() {
    let (rig, graph) = passive_ground_capability();
    let mut base = record_manifest(&rig.creature_name, "B21_CreaturePassive.esp", 0xC40);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreaturePassive.esp");
    let primary_source = source_identity("skyrimse", "Skyrim.esm", 0x02_3456);
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: primary_source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Passive Creature".to_string(),
            primary: true,
            level: 1,
            health: 25,
            action_points: 20,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: Vec::new(),
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let signatures = emitted
        .closure
        .records
        .iter()
        .map(|record| record.sig.as_str())
        .collect::<Vec<_>>();
    assert_eq!(signatures, ["RACE", "NPC_", "ARMO", "ARMA", "BPTD"]);
    let race = emitted.closure.record("RACE").unwrap();
    assert!(
        race.fields
            .iter()
            .all(|field| !matches!(field.sig.as_str(), "ATKD" | "ATKE" | "UNWP"))
    );
    assert!(emitted.required_target_records.is_empty());
}

#[test]
fn passive_npc_spells_preserve_exact_order_without_attack_inference() {
    let (rig, graph) = passive_ground_capability();
    let mut base = record_manifest(&rig.creature_name, "B21_CreatureSpells.esp", 0xC40);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureSpells.esp");
    let primary_source = source_identity("fnv", "FalloutNV.esm", 0x02_3567);
    let spells = [0xC61, 0xC60].map(|local| CreatureTargetRecordReference {
        signature: "SPEL".to_string(),
        form_key: TargetFormKey::new(local, "B21_CreatureSpells.esp"),
    });
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: primary_source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Passive Spell Creature".to_string(),
            primary: true,
            level: 1,
            health: 25,
            action_points: 20,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: spells.to_vec(),
        npc_death_item: None,
        attacks: Vec::new(),
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let race = emitted.closure.record("RACE").unwrap();
    let npc = emitted.closure.record("NPC_").unwrap();
    let emitted_spells = npc
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "SPLO")
        .map(|field| match &field.value {
            FieldValue::FormKey(form_key) => form_key.local,
            other => panic!("unexpected NPC_.SPLO value {other:?}"),
        })
        .collect::<Vec<_>>();

    assert_eq!(emitted_spells, [0xC61, 0xC60]);
    assert!(
        race.fields
            .iter()
            .all(|field| !matches!(field.sig.as_str(), "ATKD" | "ATKE" | "UNWP"))
    );
    assert!(
        !emitted
            .closure
            .records
            .iter()
            .any(|record| matches!(record.sig.as_str(), "SPEL" | "WEAP" | "PROJ"))
    );
    assert_eq!(
        emitted.required_target_records,
        [spells[1].clone(), spells[0].clone()]
    );
}

#[test]
fn npc_variant_loadouts_preserve_differences_and_validate_common_sets() {
    let (rig, graph) = passive_ground_capability();
    let mut base = record_manifest(&rig.creature_name, "B21_CreatureLoadouts.esp", 0xD40);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureLoadouts.esp");
    let primary_source = source_identity("skyrimse", "Skyrim.esm", 0x02_7000);
    let reference = |signature: &str, local| CreatureTargetRecordReference {
        signature: signature.to_string(),
        form_key: TargetFormKey::new(local, "B21_CreatureLoadouts.esp"),
    };
    let primary_equipment = reference("WEAP", 0xD60);
    let secondary_equipment = reference("WEAP", 0xD61);
    let primary_spell = reference("SPEL", 0xD62);
    let secondary_spell = reference("SPEL", 0xD63);
    let primary_death_item = reference("LVLI", 0xD64);
    let common_death_item = reference("LVLI", 0xD65);
    let mut projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![
            CreatureNpcRecordVariant {
                source_identity: primary_source,
                form_key: base.form_keys.npc.clone(),
                editor_id: base.editor_ids.npc.clone(),
                display_name: "Primary Loadout".to_string(),
                primary: true,
                level: 1,
                health: 25,
                action_points: 20,
                npc_inventory: None,
                npc_equipment: Some(vec![primary_equipment.clone()]),
                npc_spells: Some(vec![primary_spell.clone()]),
                npc_death_item: Some(Some(primary_death_item.clone())),
            },
            CreatureNpcRecordVariant {
                source_identity: source_identity("skyrimse", "Skyrim.esm", 0x02_7001),
                form_key: TargetFormKey::new(0xD46, "B21_CreatureLoadouts.esp"),
                editor_id: "B21_SkyrimWolfSecondaryNPC".to_string(),
                display_name: "Secondary Loadout".to_string(),
                primary: false,
                level: 2,
                health: 30,
                action_points: 22,
                npc_inventory: None,
                npc_equipment: Some(vec![secondary_equipment.clone()]),
                npc_spells: Some(vec![secondary_spell.clone()]),
                npc_death_item: Some(None),
            },
        ],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: Vec::new(),
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let primary_npc = emitted
        .closure
        .records
        .iter()
        .find(|record| record.form_key.local == base.form_keys.npc.local)
        .unwrap();
    let secondary_npc = emitted
        .closure
        .records
        .iter()
        .find(|record| record.form_key.local == 0xD46)
        .unwrap();
    assert_eq!(
        npc_loadout(primary_npc, &interner),
        (
            vec![primary_equipment.form_key.local],
            vec![primary_spell.form_key.local]
        )
    );
    assert_eq!(
        npc_loadout(secondary_npc, &interner),
        (
            vec![secondary_equipment.form_key.local],
            vec![secondary_spell.form_key.local]
        )
    );
    assert_eq!(
        primary_npc
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "INAM")
            .map(|field| match field.value {
                FieldValue::FormKey(form_key) => form_key.local,
                ref other => panic!("unexpected NPC_.INAM value {other:?}"),
            })
            .collect::<Vec<_>>(),
        [primary_death_item.form_key.local]
    );
    assert!(
        secondary_npc
            .fields
            .iter()
            .all(|field| field.sig.as_str() != "INAM")
    );
    assert_eq!(
        emitted.required_target_records,
        [
            primary_equipment.clone(),
            secondary_equipment.clone(),
            primary_spell.clone(),
            secondary_spell.clone(),
            primary_death_item.clone(),
        ]
    );

    projection.npc_equipment = vec![primary_equipment.clone()];
    projection.npc_spells = vec![primary_spell.clone()];
    projection.npc_death_item = Some(common_death_item.clone());
    for variant in &mut projection.variants {
        variant.npc_equipment = None;
        variant.npc_spells = None;
        variant.npc_death_item = None;
    }
    let common =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    for npc in common
        .closure
        .records
        .iter()
        .filter(|record| record.sig.as_str() == "NPC_")
    {
        assert_eq!(
            npc_loadout(npc, &interner),
            (
                vec![primary_equipment.form_key.local],
                vec![primary_spell.form_key.local]
            )
        );
        assert!(npc.fields.iter().any(|field| {
            field.sig.as_str() == "INAM"
                && matches!(
                    field.value,
                    FieldValue::FormKey(form_key)
                        if form_key.local == common_death_item.form_key.local
                )
        }));
    }

    projection.variants[1].npc_equipment = Some(vec![secondary_equipment]);
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner),
        Err(CreatureRecordError::InvalidManifest {
            code: "family_npc_equipment_mismatch",
            ..
        })
    ));
    projection.variants[1].npc_equipment = None;
    projection.variants[1].npc_spells = Some(vec![secondary_spell]);
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner),
        Err(CreatureRecordError::InvalidManifest {
            code: "family_npc_spell_mismatch",
            ..
        })
    ));
    projection.variants[1].npc_spells = None;
    projection.variants[1].npc_death_item = Some(None);
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner),
        Err(CreatureRecordError::InvalidManifest {
            code: "family_npc_death_item_mismatch",
            ..
        })
    ));
}

#[test]
fn npc_inventory_preserves_mixed_counts_and_typed_coed_rows() {
    let (rig, graph) = passive_ground_capability();
    let mut base = record_manifest(&rig.creature_name, "B21_CreatureInventory.esp", 0xD70);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureInventory.esp");
    let reference = |signature: &str, local| CreatureTargetRecordReference {
        signature: signature.to_string(),
        form_key: TargetFormKey::new(local, "B21_CreatureInventory.esp"),
    };
    let weapon = reference("WEAP", 0xD90);
    let ammunition = reference("AMMO", 0xD91);
    let ingestible = reference("ALCH", 0xD92);
    let armor = reference("ARMO", 0xD93);
    let owner = reference("NPC_", 0xDA0);
    let global = reference("GLOB", 0xDA1);
    let faction = reference("FACT", 0xDA2);
    let inventory = vec![
        CreatureNpcInventoryEntry {
            target_record: weapon.clone(),
            count: 2,
            ownership: None,
        },
        CreatureNpcInventoryEntry {
            target_record: ammunition.clone(),
            count: 40,
            ownership: Some(CreatureNpcInventoryOwnership::OwnerGlobal {
                owner: Some(owner.clone()),
                global: Some(global.clone()),
                condition: 0.75,
            }),
        },
        CreatureNpcInventoryEntry {
            target_record: ingestible.clone(),
            count: -3,
            ownership: Some(CreatureNpcInventoryOwnership::FactionRank {
                faction: faction.clone(),
                required_rank: -2,
                condition: 0.5,
            }),
        },
        CreatureNpcInventoryEntry {
            target_record: armor.clone(),
            count: 1,
            ownership: Some(CreatureNpcInventoryOwnership::OwnerGlobal {
                owner: None,
                global: None,
                condition: 1.0,
            }),
        },
        CreatureNpcInventoryEntry {
            target_record: ingestible.clone(),
            count: 1,
            ownership: None,
        },
    ];
    let source = source_identity("skyrimse", "Skyrim.esm", 0x02_7100);
    let mut projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Mixed Inventory".to_string(),
            primary: true,
            level: 1,
            health: 25,
            action_points: 20,
            npc_inventory: Some(inventory.clone()),
            npc_equipment: Some(Vec::new()),
            npc_spells: Some(Vec::new()),
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: Vec::new(),
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let npc = emitted.closure.record("NPC_").unwrap();
    let inventory_fields = npc
        .fields
        .iter()
        .filter(|field| matches!(field.sig.as_str(), "CNTO" | "COED"))
        .collect::<Vec<_>>();
    assert_eq!(
        inventory_fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        [
            "CNTO", "CNTO", "COED", "CNTO", "COED", "CNTO", "COED", "CNTO"
        ]
    );
    let cnto = inventory_fields
        .iter()
        .filter(|field| field.sig.as_str() == "CNTO")
        .map(|field| {
            let item = match struct_test_member(&field.value, "item", &interner) {
                FieldValue::FormKey(form_key) => form_key.local,
                other => panic!("unexpected CNTO item {other:?}"),
            };
            let count = match struct_test_member(&field.value, "count", &interner) {
                FieldValue::Int(count) => *count,
                value => i64::from(normalized_i32(value)),
            };
            (item, count)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cnto,
        [
            (weapon.form_key.local, 2),
            (ammunition.form_key.local, 40),
            (ingestible.form_key.local, -3),
            (armor.form_key.local, 1),
            (ingestible.form_key.local, 1),
        ]
    );
    let coed = inventory_fields
        .iter()
        .filter(|field| field.sig.as_str() == "COED")
        .collect::<Vec<_>>();
    assert!(matches!(
        struct_test_member(&coed[0].value, "owner", &interner),
        FieldValue::FormKey(form_key) if form_key.local == owner.form_key.local
    ));
    assert!(matches!(
        struct_test_member(
            &coed[0].value,
            "global_variable_required_rank",
            &interner
        ),
        FieldValue::FormKey(form_key) if form_key.local == global.form_key.local
    ));
    assert_eq!(
        normalized_f32_bits(struct_test_member(
            &coed[0].value,
            "item_condition",
            &interner
        )),
        0.75_f32.to_bits()
    );
    assert!(matches!(
        struct_test_member(&coed[1].value, "owner", &interner),
        FieldValue::FormKey(form_key) if form_key.local == faction.form_key.local
    ));
    assert_eq!(
        normalized_i32(struct_test_member(
            &coed[1].value,
            "global_variable_required_rank",
            &interner
        )),
        -2
    );
    assert_eq!(
        normalized_u32(struct_test_member(&coed[2].value, "owner", &interner)),
        0
    );
    assert_eq!(
        normalized_u32(struct_test_member(
            &coed[2].value,
            "global_variable_required_rank",
            &interner
        )),
        0
    );
    assert_eq!(
        emitted.required_target_records,
        [
            weapon,
            ammunition,
            ingestible.clone(),
            armor,
            owner,
            global,
            faction
        ]
    );

    projection.npc_inventory = inventory.clone();
    projection.variants[0].npc_inventory = None;
    projection.variants.push(CreatureNpcRecordVariant {
        source_identity: source_identity("skyrimse", "Skyrim.esm", 0x02_7101),
        form_key: TargetFormKey::new(0xD76, "B21_CreatureInventory.esp"),
        editor_id: "B21_InventorySecondaryNPC".to_string(),
        display_name: "Common Inventory".to_string(),
        primary: false,
        level: 1,
        health: 25,
        action_points: 20,
        npc_inventory: None,
        npc_equipment: Some(Vec::new()),
        npc_spells: Some(Vec::new()),
        npc_death_item: None,
    });
    emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    projection.variants[1].npc_inventory = Some(vec![CreatureNpcInventoryEntry {
        target_record: ingestible,
        count: 99,
        ownership: None,
    }]);
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner),
        Err(CreatureRecordError::InvalidManifest {
            code: "family_npc_inventory_mismatch",
            ..
        })
    ));

    projection.npc_inventory.clear();
    projection.variants.truncate(1);
    projection.variants[0].npc_inventory = Some(inventory.clone());
    let CreatureNpcInventoryOwnership::FactionRank { faction, .. } =
        projection.variants[0].npc_inventory.as_mut().unwrap()[2]
            .ownership
            .as_mut()
            .unwrap()
    else {
        unreachable!()
    };
    faction.signature = "NPC_".to_string();
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner),
        Err(CreatureRecordError::UnsupportedField {
            field: "COED.owner",
            ..
        })
    ));
    projection.variants[0].npc_inventory = Some(inventory.clone());
    let CreatureNpcInventoryOwnership::OwnerGlobal { owner, .. } =
        projection.variants[0].npc_inventory.as_mut().unwrap()[1]
            .ownership
            .as_mut()
            .unwrap()
    else {
        unreachable!()
    };
    owner.as_mut().unwrap().signature = "FACT".to_string();
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner),
        Err(CreatureRecordError::UnsupportedField {
            field: "COED.owner",
            ..
        })
    ));
    projection.variants[0].npc_inventory = Some(inventory);
    let CreatureNpcInventoryOwnership::OwnerGlobal { condition, .. } =
        projection.variants[0].npc_inventory.as_mut().unwrap()[1]
            .ownership
            .as_mut()
            .unwrap()
    else {
        unreachable!()
    };
    *condition = f32::NAN;
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner),
        Err(CreatureRecordError::InvalidManifest {
            code: "npc_inventory_condition",
            ..
        })
    ));
}

#[test]
fn ranged_projection_emits_race_spell_and_npc_equipment_without_unarmed_weapon() {
    let (mut rig, mvp) = wolf_mvp();
    let fire_event = "fireProjectile";
    for declarations in [&mut rig.root, &mut rig.core] {
        let event = declarations
            .events
            .iter_mut()
            .find(|event| event.name == mvp.melee_event)
            .unwrap();
        event.name = fire_event.to_string();
        event.usage = EventUsage::Generic;
    }
    let mut graph = CapabilityGraphManifest::from_mvp(&mvp, &MvpMotionManifest::default());
    graph.template = CreatureGraphTemplate::GroundRangedProjectile;
    let attack_role = graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::MeleeAttack)
        .unwrap();
    attack_role.role = CreatureClipRole::ProjectileAttack;
    attack_role.trigger_event = Some(fire_event.to_string());
    let attack_event = graph
        .explicit_events
        .iter_mut()
        .find(|event| event.name == mvp.melee_event)
        .unwrap();
    attack_event.name = fire_event.to_string();
    attack_event.usage = EventUsage::Generic;

    let mut base = record_manifest(&rig.creature_name, "B21_CreatureRanged.esp", 0xC00);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureRanged.esp");
    let primary_source = source_identity("fnv", "FalloutNV.esm", 0x02_1234);
    let reference = |signature: &str, local| CreatureTargetRecordReference {
        signature: signature.to_string(),
        form_key: TargetFormKey::new(local, "B21_CreatureRanged.esp"),
    };
    let attack_spell = reference("SPEL", 0xC20);
    let projectile = reference("PROJ", 0xC21);
    let equipment = reference("WEAP", 0xC22);
    let source_attack_spell = CreatureSourceRecordReference {
        source_identity: source_identity("skyrimse", "Skyrim.esm", 0x12_3456),
        signature: "SHOU".to_string(),
    };
    let source_attack_type = CreatureSourceRecordReference {
        source_identity: source_identity("skyrimse", "Skyrim.esm", 0x12_3457),
        signature: "KYWD".to_string(),
    };
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: primary_source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Ranged Creature".to_string(),
            primary: true,
            level: 5,
            health: 100,
            action_points: 75,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: vec![CreatureAttackRecordVariant {
            id: "projectile".to_string(),
            event: fire_event.to_string(),
            primary: true,
            projection: CreatureAttackRecordProjection::RangedProjectile {
                attack_spell: attack_spell.clone(),
                projectile: projectile.clone(),
                equipment: equipment.clone(),
            },
            damage_multiplier: 1.0,
            chance: 1.0,
            strike_angle: 35.0,
            action_point_cost: 20.0,
            target_data: CreatureAttackTargetData {
                attack_flags: 0x12,
                attack_angle: 0.25,
                stagger: 0.4,
                knockdown: 0.5,
                recovery_time: 0.6,
                action_points_multiplier: 2.0,
                stagger_offset: 0,
                source_atkd: Some(CreatureAttackSourceDataReceipt {
                    schema_id: "skyrimse-race-atkd-v1".to_string(),
                    damage_multiplier_bits: 1.0_f32.to_bits(),
                    chance_bits: 1.0_f32.to_bits(),
                    attack_spell: Some(source_attack_spell),
                    attack_flags: 0x12,
                    attack_angle_bits: 0.25_f32.to_bits(),
                    strike_angle_bits: 35.0_f32.to_bits(),
                    stagger_bits: 0.4_f32.to_bits(),
                    attack_type: Some(source_attack_type),
                    knockdown_bits: 0.5_f32.to_bits(),
                    recovery_time_bits: 0.6_f32.to_bits(),
                    stamina_multiplier_bits: 2.0_f32.to_bits(),
                    event: fire_event.to_string(),
                    ordinal: 4,
                    attack_type_policy: CreatureAttackTypePolicy::RuntimeInertKeyword {
                        proof: semantic_proof(
                            ATTACK_TYPE_RUNTIME_INERT_PROOF_SCHEMA,
                            serde_json::json!({
                                "evidence_blake3": blake3::hash(b"runtime-inert-keyword-evidence").to_hex().to_string(),
                                "field": "attack_type",
                                "policy": "runtime_inert_keyword",
                                "source_signature": "KYWD",
                            }),
                        ),
                    },
                    attack_spell_policy: CreatureAttackSpellPolicy::LowerShoutToSpell {
                        proof: semantic_proof(
                            ATTACK_SHOUT_TO_SPELL_PROOF_SCHEMA,
                            serde_json::json!({
                                "field": "attack_spell",
                                "lowering_receipt_blake3": blake3::hash(b"shout-to-spell-lowering-receipt").to_hex().to_string(),
                                "policy": "lower_shout_to_spell",
                                "source_signature": "SHOU",
                                "target_signature": "SPEL",
                            }),
                        ),
                    },
                    stamina_multiplier_policy:
                        CreatureAttackStaminaPolicy::PreserveAsActionPointsMultiplier {
                            proof: semantic_proof(
                                ATTACK_STAMINA_TO_ACTION_POINTS_PROOF_SCHEMA,
                                serde_json::json!({
                                    "evidence_blake3": blake3::hash(b"stamina-ap-semantic-evidence").to_hex().to_string(),
                                    "field": "stamina_multiplier",
                                    "policy": "preserve_as_action_points_multiplier",
                                    "relation": "bit_identical",
                                }),
                            ),
                        },
                    stagger_offset_policy: CreatureAttackStaggerOffsetPolicy::ExplicitZero {
                        proof: semantic_proof(
                            ATTACK_STAGGER_OFFSET_ZERO_PROOF_SCHEMA,
                            serde_json::json!({
                                "evidence_blake3": blake3::hash(b"stagger-offset-zero-policy-evidence").to_hex().to_string(),
                                "field": "stagger_offset",
                                "policy": "explicit_zero",
                                "target_value": 0,
                            }),
                        ),
                    },
                }),
            },
        }],
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let race = emitted.closure.record("RACE").unwrap();
    let npc = emitted.closure.record("NPC_").unwrap();

    assert_eq!(emitted.closure.records.len(), 5);
    assert!(
        !emitted
            .closure
            .records
            .iter()
            .any(|record| record.sig.as_str() == "WEAP")
    );
    assert!(!race.fields.iter().any(|field| field.sig.as_str() == "UNWP"));
    assert!(race.fields.iter().any(|field| {
        field.sig.as_str() == "ATKD"
            && matches!(&field.value, FieldValue::Struct(fields) if fields.iter().any(|(name, value)| {
                interner.resolve(*name) == Some("attack_spell")
                    && matches!(value, FieldValue::FormKey(form_key) if form_key.local == attack_spell.form_key.local)
            }))
    }));
    let atkd_fields = race
        .fields
        .iter()
        .find_map(|field| match &field.value {
            FieldValue::Struct(fields) if field.sig.as_str() == "ATKD" => Some(fields),
            _ => None,
        })
        .unwrap();
    for (name, expected) in [
        (
            "attack_flags",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&0x12_u32.to_le_bytes())),
        ),
        (
            "attack_angle",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&0.25_f32.to_le_bytes())),
        ),
        (
            "stagger",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&0.4_f32.to_le_bytes())),
        ),
        (
            "knockdown",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&0.5_f32.to_le_bytes())),
        ),
        (
            "recovery_time",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&0.6_f32.to_le_bytes())),
        ),
        (
            "action_points_mult",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&2.0_f32.to_le_bytes())),
        ),
        (
            "stagger_offset",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&0_i32.to_le_bytes())),
        ),
    ] {
        assert_eq!(
            atkd_fields
                .iter()
                .find_map(
                    |(field_name, value)| (interner.resolve(*field_name) == Some(name))
                        .then_some(value)
                )
                .unwrap(),
            &expected,
            "ATKD.{name}"
        );
    }
    assert!(npc.fields.iter().any(|field| {
        field.sig.as_str() == "CNTO"
            && matches!(&field.value, FieldValue::Struct(fields) if fields.iter().any(|(name, value)| {
                interner.resolve(*name) == Some("item")
                    && matches!(value, FieldValue::FormKey(form_key) if form_key.local == equipment.form_key.local)
            }))
    }));
    assert_eq!(
        emitted.required_target_records,
        vec![attack_spell, projectile, equipment]
    );
    let field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source(&projection, "skyrimse");
    assert!(
        field_receipts
            .iter()
            .any(|receipt| receipt.field.contains(".source_atkd.attack_type_policy"))
    );

    let mut arbitrary_schema = projection.clone();
    let CreatureAttackTypePolicy::RuntimeInertKeyword { proof } = &mut arbitrary_schema.attacks[0]
        .target_data
        .source_atkd
        .as_mut()
        .unwrap()
        .attack_type_policy
    else {
        unreachable!()
    };
    proof.schema = "adapter-asserted-inert-v1".to_string();
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &arbitrary_schema, &interner),
        Err(CreatureRecordError::InvalidManifest {
            code: "attack_semantic_proof",
            ..
        })
    ));

    let mut arbitrary_payload = projection.clone();
    let CreatureAttackStaggerOffsetPolicy::ExplicitZero { proof } = &mut arbitrary_payload.attacks
        [0]
    .target_data
    .source_atkd
    .as_mut()
    .unwrap()
    .stagger_offset_policy;
    proof.canonical_json = serde_json::to_string(&serde_json::json!({
        "evidence_blake3": blake3::hash(b"stagger-offset-zero-policy-evidence").to_hex().to_string(),
        "field": "stagger_offset",
        "policy": "adapter_chosen_zero",
        "target_value": 0,
    }))
    .unwrap();
    proof.canonical_json_blake3 = blake3::hash(proof.canonical_json.as_bytes())
        .to_hex()
        .to_string();
    assert!(matches!(
        emit_creature_capability_record_projection(&rig, &graph, &arbitrary_payload, &interner),
        Err(CreatureRecordError::InvalidManifest {
            code: "attack_semantic_proof",
            ..
        })
    ));
}

#[test]
fn ranged_equipment_projection_emits_no_phantom_spell_or_weapon() {
    let (mut rig, mvp) = wolf_mvp();
    let fire_event = "fireEquipment";
    for declarations in [&mut rig.root, &mut rig.core] {
        let event = declarations
            .events
            .iter_mut()
            .find(|event| event.name == mvp.melee_event)
            .unwrap();
        event.name = fire_event.to_string();
        event.usage = EventUsage::Generic;
    }
    let mut graph = CapabilityGraphManifest::from_mvp(&mvp, &MvpMotionManifest::default());
    graph.template = CreatureGraphTemplate::GroundRangedProjectile;
    let attack_role = graph
        .roles
        .iter_mut()
        .find(|role| role.role == CreatureClipRole::MeleeAttack)
        .unwrap();
    attack_role.role = CreatureClipRole::ProjectileAttack;
    attack_role.trigger_event = Some(fire_event.to_string());
    let attack_event = graph
        .explicit_events
        .iter_mut()
        .find(|event| event.name == mvp.melee_event)
        .unwrap();
    attack_event.name = fire_event.to_string();
    attack_event.usage = EventUsage::Generic;

    let mut base = record_manifest(&rig.creature_name, "B21_CreatureEquipment.esp", 0xC60);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureEquipment.esp");
    let primary_source = source_identity("fnv", "FalloutNV.esm", 0x02_3456);
    let reference = |signature: &str, local| CreatureTargetRecordReference {
        signature: signature.to_string(),
        form_key: TargetFormKey::new(local, "B21_CreatureEquipment.esp"),
    };
    let equipment = reference("WEAP", 0xC80);
    let projectile = reference("PROJ", 0xC81);
    let ammunition = reference("AMMO", 0xC82);
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: primary_source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Equipment Creature".to_string(),
            primary: true,
            level: 5,
            health: 100,
            action_points: 75,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: vec![CreatureAttackRecordVariant {
            id: "equipment".to_string(),
            event: fire_event.to_string(),
            primary: true,
            projection: CreatureAttackRecordProjection::RangedEquipment {
                equipment: vec![equipment.clone()],
                projectile: Some(projectile.clone()),
                ammunition: Some(ammunition.clone()),
            },
            damage_multiplier: 1.0,
            chance: 1.0,
            strike_angle: 35.0,
            action_point_cost: 20.0,
            target_data: CreatureAttackTargetData::default(),
        }],
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let race = emitted.closure.record("RACE").unwrap();
    let npc = emitted.closure.record("NPC_").unwrap();

    assert_eq!(emitted.closure.records.len(), 5);
    assert!(
        !emitted
            .closure
            .records
            .iter()
            .any(|record| matches!(record.sig.as_str(), "WEAP" | "SPEL" | "PROJ" | "AMMO"))
    );
    assert!(!race.fields.iter().any(|field| field.sig.as_str() == "UNWP"));
    assert!(race.fields.iter().any(|field| {
        field.sig.as_str() == "ATKD"
            && matches!(&field.value, FieldValue::Struct(fields) if fields.iter().any(|(name, value)| {
                interner.resolve(*name) == Some("attack_spell")
                    && is_null_form_reference(value)
            }))
    }));
    assert!(npc.fields.iter().any(|field| {
        field.sig.as_str() == "CNTO"
            && matches!(&field.value, FieldValue::Struct(fields) if fields.iter().any(|(name, value)| {
                interner.resolve(*name) == Some("item")
                    && matches!(value, FieldValue::FormKey(form_key) if form_key.local == equipment.form_key.local)
            }))
    }));
    assert_eq!(
        emitted.required_target_records,
        vec![equipment, projectile, ammunition]
    );
}

#[test]
fn stationary_equipment_projection_requires_no_phantom_spell() {
    let (rig, graph) = stationary_turret_capability();
    let mut base = record_manifest(&rig.creature_name, "B21_CreatureTurret.esp", 0xCC0);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureTurret.esp");
    let primary_source = source_identity("fnv", "FalloutNV.esm", 0x02_5678);
    let equipment = CreatureTargetRecordReference {
        signature: "WEAP".to_string(),
        form_key: TargetFormKey::new(0xCE0, "B21_CreatureTurret.esp"),
    };
    let projectile = CreatureTargetRecordReference {
        signature: "PROJ".to_string(),
        form_key: TargetFormKey::new(0xCE1, "B21_CreatureTurret.esp"),
    };
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: primary_source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Equipment Turret".to_string(),
            primary: true,
            level: 5,
            health: 100,
            action_points: 75,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: vec![CreatureAttackRecordVariant {
            id: "turret_equipment".to_string(),
            event: "attackStart".to_string(),
            primary: true,
            projection: CreatureAttackRecordProjection::RangedEquipment {
                equipment: vec![equipment.clone()],
                projectile: Some(projectile.clone()),
                ammunition: None,
            },
            damage_multiplier: 1.0,
            chance: 1.0,
            strike_angle: 35.0,
            action_point_cost: 20.0,
            target_data: CreatureAttackTargetData::default(),
        }],
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let race = emitted.closure.record("RACE").unwrap();

    assert_eq!(emitted.closure.records.len(), 5);
    assert!(
        !emitted
            .closure
            .records
            .iter()
            .any(|record| matches!(record.sig.as_str(), "SPEL" | "WEAP" | "PROJ"))
    );
    assert!(race.fields.iter().any(|field| {
        field.sig.as_str() == "ATKD"
            && matches!(&field.value, FieldValue::Struct(fields) if fields.iter().any(|(name, value)| {
                interner.resolve(*name) == Some("attack_spell") && is_null_form_reference(value)
            }))
    }));
    assert_eq!(emitted.required_target_records, [equipment, projectile]);
}

#[test]
fn continuous_robot_equipment_does_not_infer_passive_npc_spell_as_attack() {
    let (rig, graph) = robot_continuous_attack_capability();
    let mut base = record_manifest(&rig.creature_name, "B21_CreatureRobot.esp", 0xCF0);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureRobot.esp");
    let primary_source = source_identity("fnv", "FalloutNV.esm", 0x02_6789);
    let passive_spell = CreatureTargetRecordReference {
        signature: "SPEL".to_string(),
        form_key: TargetFormKey::new(0xD20, "B21_CreatureRobot.esp"),
    };
    let equipment = CreatureTargetRecordReference {
        signature: "WEAP".to_string(),
        form_key: TargetFormKey::new(0xD21, "B21_CreatureRobot.esp"),
    };
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: primary_source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Continuous Robot".to_string(),
            primary: true,
            level: 5,
            health: 100,
            action_points: 75,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: vec![passive_spell.clone()],
        npc_death_item: None,
        attacks: vec![CreatureAttackRecordVariant {
            id: "continuous_equipment".to_string(),
            event: "attackStartAuto".to_string(),
            primary: true,
            projection: CreatureAttackRecordProjection::RangedEquipment {
                equipment: vec![equipment.clone()],
                projectile: None,
                ammunition: None,
            },
            damage_multiplier: 1.0,
            chance: 1.0,
            strike_angle: 35.0,
            action_point_cost: 20.0,
            target_data: CreatureAttackTargetData::default(),
        }],
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let race = emitted.closure.record("RACE").unwrap();
    let npc = emitted.closure.record("NPC_").unwrap();

    assert_eq!(emitted.closure.records.len(), 5);
    assert!(
        !emitted
            .closure
            .records
            .iter()
            .any(|record| matches!(record.sig.as_str(), "SPEL" | "WEAP" | "PROJ" | "AMMO"))
    );
    assert!(race.fields.iter().any(|field| {
        field.sig.as_str() == "ATKD"
            && matches!(&field.value, FieldValue::Struct(fields) if fields.iter().any(|(name, value)| {
                interner.resolve(*name) == Some("attack_spell") && is_null_form_reference(value)
            }))
    }));
    assert!(npc.fields.iter().any(|field| {
        field.sig.as_str() == "SPLO"
            && matches!(field.value, FieldValue::FormKey(form_key) if form_key.local == passive_spell.form_key.local)
    }));
    assert!(npc.fields.iter().any(|field| {
        field.sig.as_str() == "CNTO"
            && matches!(&field.value, FieldValue::Struct(fields) if fields.iter().any(|(name, value)| {
                interner.resolve(*name) == Some("item")
                    && matches!(value, FieldValue::FormKey(form_key) if form_key.local == equipment.form_key.local)
            }))
    }));
    assert_eq!(emitted.required_target_records, [passive_spell, equipment]);
}

#[test]
fn melee_equipment_projection_preserves_all_source_weapons_without_generated_unarmed_weapon() {
    let (rig, mvp) = wolf_mvp();
    let graph = CapabilityGraphManifest::from_mvp(&mvp, &MvpMotionManifest::default());
    let mut base = record_manifest(&rig.creature_name, "B21_CreatureMeleeEquip.esp", 0xC90);
    base.form_keys.unarmed_weapon = TargetFormKey::new(0, "B21_CreatureMeleeEquip.esp");
    let primary_source = source_identity("fnv", "FalloutNV.esm", 0x02_4567);
    let reference = |local| CreatureTargetRecordReference {
        signature: "WEAP".to_string(),
        form_key: TargetFormKey::new(local, "B21_CreatureMeleeEquip.esp"),
    };
    let attack_equipment = [reference(0xCB1), reference(0xCB0)];
    let inventory_equipment = reference(0xCB2);
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data: grounded_race_data(),
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: primary_source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Melee Equipment Creature".to_string(),
            primary: true,
            level: 5,
            health: 100,
            action_points: 75,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: vec![inventory_equipment.clone(), attack_equipment[1].clone()],
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: vec![CreatureAttackRecordVariant {
            id: "equipment_melee".to_string(),
            event: mvp.melee_event,
            primary: true,
            projection: CreatureAttackRecordProjection::MeleeEquipment {
                equipment: attack_equipment.to_vec(),
            },
            damage_multiplier: 1.0,
            chance: 1.0,
            strike_angle: 35.0,
            action_point_cost: 20.0,
            target_data: CreatureAttackTargetData::default(),
        }],
    };
    let interner = crate::sym::StringInterner::new();

    let emitted =
        emit_creature_capability_record_projection(&rig, &graph, &projection, &interner).unwrap();
    let race = emitted.closure.record("RACE").unwrap();
    let npc = emitted.closure.record("NPC_").unwrap();

    assert_eq!(emitted.closure.records.len(), 5);
    assert!(
        !emitted
            .closure
            .records
            .iter()
            .any(|record| record.sig.as_str() == "WEAP")
    );
    assert!(!race.fields.iter().any(|field| field.sig.as_str() == "UNWP"));
    assert!(race.fields.iter().any(|field| {
        field.sig.as_str() == "ATKD"
            && matches!(&field.value, FieldValue::Struct(fields) if fields.iter().any(|(name, value)| {
                interner.resolve(*name) == Some("attack_spell")
                    && is_null_form_reference(value)
            }))
    }));
    let equipped = npc
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "CNTO")
        .filter_map(|field| match &field.value {
            FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
                (interner.resolve(*name) == Some("item")).then_some(value)
            }),
            _ => None,
        })
        .map(|value| match value {
            FieldValue::FormKey(form_key) => form_key.local,
            other => panic!("unexpected NPC_.CNTO.item value {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(equipped, [0xCB2, 0xCB0, 0xCB1]);
    assert_eq!(
        emitted.required_target_records,
        [
            attack_equipment[1].clone(),
            attack_equipment[0].clone(),
            inventory_equipment,
        ]
    );
}

#[test]
fn record_projection_rejects_missing_derived_race_data_values() {
    let (rig, graph) = wolf_mvp();
    let base = record_manifest(&rig.creature_name, "B21_CreatureMVP.esp", 0xB00);
    let primary_source = source_identity("skyrimse", "Skyrim.esm", 0x01_1234);
    let mut race_data = grounded_race_data();
    race_data.male_height = 0.0;
    let projection = CreatureRecordProjectionManifest {
        base: base.clone(),
        source_primary_identity: primary_source.clone(),
        race_data,
        body_parts: CreatureBodyPartProjection::root_only_32("NPC Root [Root]"),
        body_nif_parts: Vec::new(),
        variants: vec![CreatureNpcRecordVariant {
            source_identity: primary_source,
            form_key: base.form_keys.npc.clone(),
            editor_id: base.editor_ids.npc.clone(),
            display_name: "Wolf".to_string(),
            primary: true,
            level: 5,
            health: 100,
            action_points: 75,
            npc_inventory: None,
            npc_equipment: None,
            npc_spells: None,
            npc_death_item: None,
        }],
        npc_inventory: Vec::new(),
        npc_equipment: Vec::new(),
        npc_spells: Vec::new(),
        npc_death_item: None,
        attacks: vec![CreatureAttackRecordVariant {
            id: "Wolf Bite".to_string(),
            event: graph.melee_event.clone(),
            primary: true,
            projection: CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key: base.form_keys.unarmed_weapon.clone(),
                weapon_editor_id: base.editor_ids.unarmed_weapon.clone(),
                damage: 15,
                reach: 0.7,
                attack_seconds: 0.8,
            },
            damage_multiplier: 1.0,
            chance: 1.0,
            strike_angle: 35.0,
            action_point_cost: 20.0,
            target_data: CreatureAttackTargetData::default(),
        }],
    };
    let interner = crate::sym::StringInterner::new();

    assert!(matches!(
        emit_creature_record_projection(&rig, &graph, &projection, &interner),
        Err(CreatureRecordError::UnsupportedField {
            record: "RACE",
            field: "DATA",
            ..
        })
    ));
}

fn race_data_policy() -> Fo4RaceDataScalePolicy {
    Fo4RaceDataScalePolicy {
        small_max_stature: 32.0,
        medium_max_stature: 96.0,
        large_max_stature: 192.0,
    }
}

fn angular_race_motion() -> AngularMotionEvidence {
    AngularMotionEvidence {
        max_yaw_speed_degrees_per_second: 120.0,
        seconds_to_full_yaw_speed: 0.5,
        tolerance_degrees: 15.0,
        aim_tolerance_degrees: 30.0,
        pitch_limit_degrees: 45.0,
        roll_limit_degrees: 20.0,
    }
}

fn family_race_evidence(
    stature: f32,
    movement_architecture: MovementArchitecture,
    controller_architecture: ControllerArchitecture,
) -> SourceFamilyRaceDataEvidence {
    let mobile = movement_architecture != MovementArchitecture::Stationary;
    SourceFamilyRaceDataEvidence {
        body_bounds: Some(MeasuredBounds {
            min: [-stature * 0.4, -stature * 0.3, 0.0],
            max: [stature * 0.4, stature * 0.3, stature],
        }),
        skeleton_bounds: Some(MeasuredBounds {
            min: [-stature * 0.35, -stature * 0.25, stature * 0.05],
            max: [stature * 0.35, stature * 0.25, stature * 0.95],
        }),
        up_axis: MeasurementAxis::Z,
        capsule: Some(CapsuleEvidence {
            radius: stature * 0.15,
            total_height: stature * 0.75,
            architecture: controller_architecture,
        }),
        geometry_scale_to_fo4: Some(1.0),
        male_actor_scale: Some(1.0),
        female_actor_scale: Some(1.0),
        default_weights: Some(DefaultWeightEvidence {
            male: [0.0, 1.0, 0.0],
            female: [0.0, 1.0, 0.0],
            ungendered: true,
        }),
        movement: Some(MovementSemantics {
            architecture: movement_architecture,
            linear: mobile.then_some(LinearMotionEvidence {
                max_speed: 120.0,
                seconds_to_full_speed: 0.5,
                seconds_to_stop: 0.25,
            }),
            angular: Some(angular_race_motion()),
            pushable: mobile,
            opens_doors: false,
            allow_ragdoll_collision: mobile,
            non_hostile: false,
            use_large_actor_pathing: stature > 192.0,
            use_subsegmented_damage: false,
        }),
        injured_health_percent: Some(0.2),
        body_biped_object: Some(0),
        xp_value: Some(10),
    }
}

fn mapped_race_data(derivation: Fo4RaceDataDerivation) -> Fo4RaceDataTarget {
    match derivation.mapping {
        RaceDataMapping::Mapped { target } => target,
        RaceDataMapping::Missing { fields } => panic!("unexpected missing fields: {fields:?}"),
    }
}

#[test]
fn race_data_adapter_builds_wolf_and_gecko_without_source_branching() {
    let wolf_evidence = family_race_evidence(
        80.0,
        MovementArchitecture::Grounded,
        ControllerArchitecture::Quadruped,
    );
    let gecko_evidence = family_race_evidence(
        28.0,
        MovementArchitecture::Grounded,
        ControllerArchitecture::Quadruped,
    );

    let first = derive_fo4_race_data(&wolf_evidence, &race_data_policy()).unwrap();
    let second = derive_fo4_race_data(&wolf_evidence, &race_data_policy()).unwrap();
    assert_eq!(first, second);
    let wolf = mapped_race_data(first);
    let gecko =
        mapped_race_data(derive_fo4_race_data(&gecko_evidence, &race_data_policy()).unwrap());

    assert_eq!(wolf.size, Fo4RaceSize::Medium);
    assert_eq!(gecko.size, Fo4RaceSize::Small);
    assert_eq!(wolf.acceleration_rate, 240.0);
    assert_eq!(wolf.deceleration_rate, 480.0);
    assert!(wolf.flags.contains(&Fo4RaceFlag::Walks));
    assert!(wolf.flags_2.contains(&Fo4RaceFlag2::UseQuadrupedController));
    assert!(wolf.flags_2.contains(&Fo4RaceFlag2::Ungendered));
}

#[test]
fn race_data_adapter_scales_extreme_dragon_and_flyer_capsules() {
    let mut dragon_evidence = family_race_evidence(
        300.0,
        MovementArchitecture::Flying,
        ControllerArchitecture::Standard,
    );
    dragon_evidence.geometry_scale_to_fo4 = Some(1.5);
    dragon_evidence
        .movement
        .as_mut()
        .unwrap()
        .use_large_actor_pathing = true;
    let flyer_evidence = family_race_evidence(
        120.0,
        MovementArchitecture::Flying,
        ControllerArchitecture::Standard,
    );

    let dragon_derivation = derive_fo4_race_data(&dragon_evidence, &race_data_policy()).unwrap();
    let dragon_audit = dragon_derivation.scale_audit.clone().unwrap();
    let dragon = mapped_race_data(dragon_derivation);
    let flyer =
        mapped_race_data(derive_fo4_race_data(&flyer_evidence, &race_data_policy()).unwrap());

    assert_eq!(dragon.size, Fo4RaceSize::ExtraLarge);
    assert_eq!(flyer.size, Fo4RaceSize::Large);
    assert_eq!(dragon.male_height, 1.5);
    assert_eq!(dragon.flight_radius, dragon_audit.scaled_capsule_radius);
    assert!(dragon.flags.contains(&Fo4RaceFlag::Flies));
    assert!(dragon.flags_2.contains(&Fo4RaceFlag2::UseLargeActorPathing));
    assert!(flyer.flight_radius > 0.0);
}

#[test]
fn race_data_adapter_preserves_grounded_swimming_and_flying_flags() {
    let swimmer = mapped_race_data(
        derive_fo4_race_data(
            &family_race_evidence(
                80.0,
                MovementArchitecture::GroundedSwimming,
                ControllerArchitecture::Quadruped,
            ),
            &race_data_policy(),
        )
        .unwrap(),
    );
    let flyer = mapped_race_data(
        derive_fo4_race_data(
            &family_race_evidence(
                180.0,
                MovementArchitecture::GroundedFlying,
                ControllerArchitecture::Standard,
            ),
            &race_data_policy(),
        )
        .unwrap(),
    );

    assert!(swimmer.flags.contains(&Fo4RaceFlag::Walks));
    assert!(swimmer.flags.contains(&Fo4RaceFlag::Swims));
    assert!(!swimmer.flags.contains(&Fo4RaceFlag::Flies));
    assert_eq!(swimmer.flight_radius, 0.0);
    assert!(flyer.flags.contains(&Fo4RaceFlag::Walks));
    assert!(flyer.flags.contains(&Fo4RaceFlag::Flies));
    assert!(!flyer.flags.contains(&Fo4RaceFlag::Swims));
    assert!(flyer.flight_radius > 0.0);
}

#[test]
fn race_data_adapter_handles_stationary_turret_and_small_critter() {
    let turret_evidence = family_race_evidence(
        140.0,
        MovementArchitecture::Stationary,
        ControllerArchitecture::Fixed,
    );
    let critter_evidence = family_race_evidence(
        12.0,
        MovementArchitecture::Grounded,
        ControllerArchitecture::Quadruped,
    );

    let turret =
        mapped_race_data(derive_fo4_race_data(&turret_evidence, &race_data_policy()).unwrap());
    let critter =
        mapped_race_data(derive_fo4_race_data(&critter_evidence, &race_data_policy()).unwrap());

    assert_eq!(turret.size, Fo4RaceSize::Large);
    assert_eq!(turret.acceleration_rate, 0.0);
    assert_eq!(turret.deceleration_rate, 0.0);
    assert!(turret.flags.contains(&Fo4RaceFlag::Immobile));
    assert_eq!(turret.flight_radius, 0.0);
    assert_eq!(critter.size, Fo4RaceSize::Small);
    assert!(critter.flags.contains(&Fo4RaceFlag::Walks));
}

#[test]
fn race_data_adapter_returns_exact_missing_evidence_without_defaults() {
    let mut evidence = family_race_evidence(
        80.0,
        MovementArchitecture::Grounded,
        ControllerArchitecture::Quadruped,
    );
    evidence.body_bounds = None;
    evidence.capsule = None;
    evidence.xp_value = None;
    evidence.movement.as_mut().unwrap().linear = None;
    evidence.movement.as_mut().unwrap().angular = None;

    let derivation = derive_fo4_race_data(&evidence, &race_data_policy()).unwrap();

    assert_eq!(
        derivation.missing_evidence,
        [
            RaceDataEvidenceField::BodyBounds,
            RaceDataEvidenceField::ControllerCapsule,
            RaceDataEvidenceField::LinearMotion,
            RaceDataEvidenceField::AngularMotion,
            RaceDataEvidenceField::XpValue,
        ]
    );
    assert_eq!(derivation.scale_audit, None);
    assert!(matches!(
        derivation.mapping,
        RaceDataMapping::Missing { ref fields }
            if fields.contains(&Fo4RaceDataField::ControllerCapsule)
                && fields.contains(&Fo4RaceDataField::FlightRadius)
                && fields.contains(&Fo4RaceDataField::LinearAcceleration)
                && fields.contains(&Fo4RaceDataField::AngularAcceleration)
                && fields.contains(&Fo4RaceDataField::AimAngleTolerance)
                && fields.contains(&Fo4RaceDataField::OrientationLimits)
                && fields.contains(&Fo4RaceDataField::Size)
                && fields.contains(&Fo4RaceDataField::XpValue)
    ));
}

#[test]
fn race_data_adapter_rejects_invalid_measurements_instead_of_guessing() {
    let mut evidence = family_race_evidence(
        80.0,
        MovementArchitecture::Grounded,
        ControllerArchitecture::Quadruped,
    );
    evidence.capsule.as_mut().unwrap().radius = -1.0;

    assert!(matches!(
        derive_fo4_race_data(&evidence, &race_data_policy()),
        Err(RaceDataDerivationError::InvalidEvidence {
            field: RaceDataEvidenceField::ControllerCapsule,
            ..
        })
    ));
}
