//! Fail-closed source-record adapter for the vanilla FNV Gecko.
//!
//! The exact FNV MVP phase schedules this adapter after its source-record and
//! asset gates pass. Other Gecko or creature records remain excluded.

use thiserror::Error;

use crate::record::{FieldValue, Record};
use crate::source_rig::{
    BoneDecl, Capsule, ClipBinding, ClipDecl, ClipMotionPolicy, CreatureManifest,
    CreatureRecordClosure, CreatureRecordEditorIds, CreatureRecordError, CreatureRecordFormKeys,
    CreatureRecordManifest, CreatureRecordProfile, EventDecl, EventUsage, GraphDeclarations,
    MvpGraphManifest, MvpMotionManifest, ScaffoldPaths, SkeletonDecl, VariableDecl, VariableType,
    VariableValue, emit_creature_record_closure,
};
use crate::sym::StringInterner;

const SOURCE_PLUGIN: &str = "FalloutNV.esm";
const SOURCE_LOCAL: u32 = 0x10_CD73;
const SOURCE_EDITOR_ID: &str = "NVCrGecko";
const SOURCE_NAME: &str = "Gecko";
const SOURCE_SKELETON: &str = "creatures\\NVGecko\\skeleton.nif";
const SOURCE_BODY_LIST: &[u8] = b"NVGecko.NIF\0\0";
const SOURCE_BODY: &str = "creatures\\NVGecko\\NVGecko.NIF";
const MELEE_EVENT: &str = "meleeGeckoAttackForwardPower";

fn gecko_runtime_clip_path(runtime_root: &str, name: &str) -> String {
    format!("{runtime_root}\\Animations\\{name}.hkx")
}

const SOURCE_FIELD_LAYOUT: [[u8; 4]; 27] = [
    *b"EDID", *b"OBND", *b"FULL", *b"MODL", *b"EAMT", *b"NIFZ", *b"NIFT", *b"ACBS", *b"SNAM",
    *b"SNAM", *b"SNAM", *b"INAM", *b"VTCK", *b"AIDT", *b"PKID", *b"PKID", *b"DATA", *b"RNAM",
    *b"ZNAM", *b"PNAM", *b"TNAM", *b"BNAM", *b"WNAM", *b"NAM4", *b"NAM5", *b"CSCR", *b"CNAM",
];

const SOURCE_BONES: [(&str, Option<usize>); 87] = [
    ("Bip01", None),
    ("Bip01 NonAccum", Some(0)),
    ("Bip01 Pelvis", Some(1)),
    ("Bip01 Spine1", Some(2)),
    ("Bip01 Spine2", Some(3)),
    ("Bip01 Spine3", Some(4)),
    ("Bip01 Spine4", Some(5)),
    ("Bip01 Neck1", Some(6)),
    ("Bip01 Neck2", Some(7)),
    ("Bip01 Head", Some(8)),
    ("Bip01 Jaw", Some(9)),
    ("Bip01 Tongue", Some(10)),
    ("Bip01 Tongue1", Some(11)),
    ("Bip01 Tongue2", Some(12)),
    ("Bip01 Tongue3", Some(13)),
    ("Bip01 Tongue4", Some(14)),
    ("Bip01 Tongue5", Some(15)),
    ("Bip01 Tongue6", Some(16)),
    ("Bip01 Tongue7", Some(17)),
    ("Bip01 Gullet", Some(10)),
    ("Bip01 R Fin11", Some(9)),
    ("Bip01 R Fin12", Some(20)),
    ("Bip01 L Fin01", Some(9)),
    ("Bip01 L Fin02", Some(22)),
    ("Bip01 R Fin01", Some(9)),
    ("Bip01 R Fin02", Some(24)),
    ("Bip01 L Fin11", Some(9)),
    ("Bip01 L Fin12", Some(26)),
    ("Bip01 R Fin21", Some(9)),
    ("Bip01 R Fin22", Some(28)),
    ("Bip01 L Fin21", Some(9)),
    ("Bip01 L Fin22", Some(30)),
    ("Bip01 L Eye", Some(9)),
    ("Bip01 R Eye", Some(9)),
    ("ProjectileNode_Fire", Some(9)),
    ("Bip01 L Clavicle", Some(6)),
    ("Bip01 L UpperArm", Some(35)),
    ("Bip01 L Forearm", Some(36)),
    ("Bip01 L Hand", Some(37)),
    ("Bip01 L Thumb1", Some(38)),
    ("Bip01 L Thumb2", Some(39)),
    ("Bip01 L Finger41", Some(38)),
    ("Bip01 L Finger42", Some(41)),
    ("Bip01 L Finger31", Some(38)),
    ("Bip01 L Finger32", Some(43)),
    ("Bip01 L Finger21", Some(38)),
    ("Bip01 L Finger22", Some(45)),
    ("Bip01 L Finger11", Some(38)),
    ("Bip01 L Finger12", Some(47)),
    ("Bip01 R Clavicle", Some(6)),
    ("Bip01 R UpperArm", Some(49)),
    ("Bip01 R Forearm", Some(50)),
    ("Bip01 R Hand", Some(51)),
    ("Bip01 R Thumb1", Some(52)),
    ("Bip01 R Thumb2", Some(53)),
    ("Bip01 R Finger41", Some(52)),
    ("Bip01 R Finger42", Some(55)),
    ("Bip01 R Finger31", Some(52)),
    ("Bip01 R Finger32", Some(57)),
    ("Bip01 R Finger21", Some(52)),
    ("Bip01 R Finger22", Some(59)),
    ("Bip01 R Finger11", Some(52)),
    ("Bip01 R Finger12", Some(61)),
    ("Weapon", Some(52)),
    ("Bip01 L Thigh", Some(2)),
    ("Bip01 L Calf", Some(64)),
    ("Bip01 L Foot", Some(65)),
    ("Bip01 L Toe11", Some(66)),
    ("Bip01 L Toe12", Some(67)),
    ("Bip01 L Toe21", Some(66)),
    ("Bip01 L Toe22", Some(69)),
    ("Bip01 L Toe31", Some(66)),
    ("Bip01 L Toe32", Some(71)),
    ("Bip01 Tail1", Some(2)),
    ("Bip01 Tail2", Some(73)),
    ("Bip01 Tail3", Some(74)),
    ("Bip01 Tail4", Some(75)),
    ("Bip01 Tail5", Some(76)),
    ("Bip01 R Thigh", Some(2)),
    ("Bip01 R Calf", Some(78)),
    ("Bip01 R Foot", Some(79)),
    ("Bip01 R Toe11", Some(80)),
    ("Bip01 R Toe12", Some(81)),
    ("Bip01 R Toe21", Some(80)),
    ("Bip01 R Toe22", Some(83)),
    ("Bip01 R Toe31", Some(80)),
    ("Bip01 R Toe32", Some(85)),
];

const ATTACK_TEXT_KEYS: [(u32, &str); 12] = [
    (0x0000_0000, "start"),
    (0x3D08_8900, "Enum: Left"),
    (0x3E08_8880, "Enum: Auxiliary2"),
    (0x3E2A_AAC0, "Enum: Auxiliary6"),
    (0x3F2A_AAB0, "Enum: Auxiliary5"),
    (0x3F44_4450, "Enum: Left"),
    (0x3F4C_CCD0, "Enum: Right"),
    (0x3F5D_DDE0, "Enum: Attack"),
    (0x3F6E_EEF0, "Hit"),
    (0x3FA2_2228, "Enum: Right"),
    (0x3FBB_BBC0, "Enum: Left"),
    (0x3FD9_9998, "end"),
];
const FORWARD_TEXT_KEYS: [(u32, &str); 7] = [
    (0x0000_0000, "start"),
    (0x3727_C5AC, "Enum: Auxiliary1"),
    (0x3D4C_CCCD, "Sound: NPCGeckoSwim"),
    (0x3D4C_D749, "Sound: NPCGeckoSwimSurface"),
    (0x3F77_7778, "Sound: NPCGeckoSwim"),
    (0x3F77_7820, "Sound: NPCGeckoSwimSurface"),
    (0x3FCC_CCCD, "end"),
];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LegacyGeckoTextKeyEvidence {
    pub time: f32,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LegacyGeckoClipEvidence {
    pub source_path: String,
    pub sequence_name: String,
    pub controlled_nodes: Vec<String>,
    pub cycle_loop: bool,
    pub frequency: f32,
    pub start_time: f32,
    pub stop_time: f32,
    pub accumulation_root: String,
    pub text_keys: Vec<LegacyGeckoTextKeyEvidence>,
    pub extracted_planar_reference_frames: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LegacyGeckoAssetEvidence {
    pub body_nif: String,
    pub skeleton_nif: String,
    pub skeleton_bones: Vec<BoneDecl>,
    pub clips: Vec<LegacyGeckoClipEvidence>,
}

#[derive(Clone, Debug)]
pub(crate) struct AdaptedGeckoCreature {
    pub rig: CreatureManifest,
    pub graph: MvpGraphManifest,
    pub motion: MvpMotionManifest,
    pub records: CreatureRecordManifest,
    pub profile: CreatureRecordProfile,
}

impl AdaptedGeckoCreature {
    pub(crate) fn emit_records(
        &self,
        interner: &StringInterner,
    ) -> Result<CreatureRecordClosure, CreatureRecordError> {
        emit_creature_record_closure(
            &self.rig,
            &self.graph,
            &self.records,
            &self.profile,
            interner,
        )
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(crate) enum GeckoAdapterError {
    #[error("canonical Gecko source record rejected: {0}")]
    SourceRecord(String),
    #[error("canonical Gecko asset closure rejected: {0}")]
    SourceAssets(String),
    #[error("invalid target runtime root: {0}")]
    RuntimeRoot(String),
}

pub(crate) fn canonical_gecko_asset_evidence() -> LegacyGeckoAssetEvidence {
    LegacyGeckoAssetEvidence {
        body_nif: SOURCE_BODY.to_string(),
        skeleton_nif: SOURCE_SKELETON.to_string(),
        skeleton_bones: source_bones(),
        clips: clip_specs()
            .into_iter()
            .map(|spec| LegacyGeckoClipEvidence {
                source_path: spec.path.to_string(),
                sequence_name: spec.sequence.to_string(),
                controlled_nodes: source_track_nodes(spec.include_root, spec.include_weapon),
                cycle_loop: spec.cycle_loop,
                frequency: 1.0,
                start_time: 0.0,
                stop_time: f32::from_bits(spec.stop_time_bits),
                accumulation_root: "Bip01".to_string(),
                text_keys: spec
                    .text_keys
                    .iter()
                    .map(|(time_bits, text)| LegacyGeckoTextKeyEvidence {
                        time: f32::from_bits(*time_bits),
                        text: (*text).to_string(),
                    })
                    .collect(),
                extracted_planar_reference_frames: usize::from(spec.animation_driven),
            })
            .collect(),
    }
}

pub(crate) fn adapt_canonical_gecko(
    source: &Record,
    assets: &LegacyGeckoAssetEvidence,
    target_plugin: &str,
    target_form_keys: CreatureRecordFormKeys,
    runtime_root: &str,
    interner: &StringInterner,
) -> Result<AdaptedGeckoCreature, GeckoAdapterError> {
    audit_source_record(source, interner)?;
    audit_source_assets(assets)?;
    let creature_name = validate_runtime_root(runtime_root)?;

    let skeleton_hkx = format!("{runtime_root}\\CharacterAssets\\Skeleton.hkx");
    let bones = source_bones();
    let specs = clip_specs();
    let clip = |spec: ClipSpec, looping: bool| {
        let track_map = source_track_map(&bones, spec.include_root, spec.include_weapon);
        ClipDecl {
            name: spec.target_role.to_string(),
            path: gecko_runtime_clip_path(runtime_root, spec.target_role),
            binding: ClipBinding {
                skeleton_path: skeleton_hkx.clone(),
                original_skeleton_name: "NVGecko".to_string(),
                declared_transform_tracks: track_map.len(),
                transform_track_to_bone_indices: track_map,
                declared_float_tracks: 0,
                float_track_to_float_slot_indices: Vec::new(),
            },
            looping,
        }
    };
    let clips = vec![
        clip(specs[0], true),
        clip(specs[1], true),
        // FO4 graph transitions own the turn lifetime even though these
        // legacy KF headers advertise CYCLE_LOOP.
        clip(specs[2], false),
        clip(specs[3], false),
        clip(specs[4], false),
    ];
    let declarations = GraphDeclarations {
        events: vec![
            generic_event("Idle"),
            generic_event("startWalk"),
            generic_event("TurnLeft90"),
            generic_event("TurnRight90"),
            EventDecl {
                name: MELEE_EVENT.to_string(),
                usage: EventUsage::MeleeAttack,
                flags: 0,
            },
            generic_event("HitFrame"),
        ],
        variables: vec![VariableDecl {
            name: "bGraphDriven".to_string(),
            variable_type: VariableType::Bool,
            initial_value: VariableValue::Bool(true),
        }],
        character_properties: Vec::new(),
    };
    let rig = CreatureManifest {
        creature_name: creature_name.clone(),
        visual_skeleton_nif: format!("{runtime_root}\\CharacterAssets\\Skeleton.nif"),
        animation_skeleton: SkeletonDecl {
            path: skeleton_hkx,
            runtime_name: "NVGecko".to_string(),
            bones,
            float_slots: Vec::new(),
        },
        controller: crate::source_rig::CreatureControllerDecl {
            collision_filter_info: 1,
            rigid_body_type: 255,
            model_up_ms: [0.0, 0.0, 1.0, 0.0],
            model_forward_ms: [1.0, 0.0, 0.0, 0.0],
            model_right_ms: [0.0, -1.0, 0.0, 0.0],
            model_scale: 1.0,
        },
        ragdoll: crate::source_rig::RagdollDisposition::deferred(),
        clips,
        idle_clip: specs[0].target_role.to_string(),
        capsule: Capsule {
            height: 100.0 / 70.0,
            radius: 30.0 / 70.0,
        },
        paths: ScaffoldPaths {
            project: format!("{runtime_root}\\{creature_name}Project.hkx"),
            character: format!("{runtime_root}\\Characters\\{creature_name}Character.hkx"),
            root_behavior: format!("{runtime_root}\\Behaviors\\{creature_name}RootBehavior.hkx"),
            core_behavior: format!("{runtime_root}\\Behaviors\\{creature_name}CoreBehavior.hkx"),
        },
        root: declarations.clone(),
        core: declarations,
    };
    let graph = MvpGraphManifest {
        idle_clip: specs[0].target_role.to_string(),
        walk_forward_clip: specs[1].target_role.to_string(),
        turn_left_90_clip: specs[2].target_role.to_string(),
        turn_right_90_clip: specs[3].target_role.to_string(),
        attack_1_clip: specs[4].target_role.to_string(),
        melee_event: MELEE_EVENT.to_string(),
    };
    let motion_policy = |index: usize, animation_driven: bool| ClipMotionPolicy {
        animation_driven,
        extracted_planar_reference_frames: assets.clips[index].extracted_planar_reference_frames,
    };
    let motion = MvpMotionManifest {
        idle: motion_policy(0, false),
        walk_forward: motion_policy(1, true),
        turn_left_90: motion_policy(2, false),
        turn_right_90: motion_policy(3, false),
        attack_1: motion_policy(4, true),
    };
    rig.validate_mvp_motion(&graph, &motion)
        .map_err(|error| GeckoAdapterError::SourceAssets(error.to_string()))?;

    let records = CreatureRecordManifest {
        target_plugin: target_plugin.to_string(),
        editor_id_prefix: "B21_".to_string(),
        form_keys: target_form_keys,
        editor_ids: CreatureRecordEditorIds {
            race: format!("{creature_name}Race"),
            npc: format!("{creature_name}NPC"),
            skin: format!("{creature_name}Skin"),
            armor_addon: format!("{creature_name}BodyAA"),
            body_part_data: format!("{creature_name}BodyPartData"),
            unarmed_weapon: format!("{creature_name}Unarmed"),
        },
        display_name: SOURCE_NAME.to_string(),
        body_nif: format!("{runtime_root}\\CharacterAssets\\{creature_name}.nif"),
    };
    let mut profile = CreatureRecordProfile::root_segment_32("Bip01");
    profile.unarmed_damage = 30;
    profile.unarmed_reach = 0.35;
    profile.unarmed_attack_seconds = f32::from_bits(0x3F_D99998);
    profile.npc_level = 7;
    profile.npc_health = 65;

    Ok(AdaptedGeckoCreature {
        rig,
        graph,
        motion,
        records,
        profile,
    })
}

fn audit_source_record(
    record: &Record,
    interner: &StringInterner,
) -> Result<(), GeckoAdapterError> {
    if record.sig.as_str() != "CREA"
        || record.form_key.local != SOURCE_LOCAL
        || interner.resolve(record.form_key.plugin) != Some(SOURCE_PLUGIN)
        || record.flags.bits() != 0
        || record.eid.and_then(|value| interner.resolve(value)) != Some(SOURCE_EDITOR_ID)
    {
        return Err(source_record_error("identity or record flags drifted"));
    }
    let layout = record
        .fields
        .iter()
        .map(|field| field.sig.0)
        .collect::<Vec<_>>();
    if layout != SOURCE_FIELD_LAYOUT {
        return Err(source_record_error(format!(
            "field layout drifted; scripts, templates, and unknown fields are not admitted: {layout:?}"
        )));
    }
    if string_value(field(record, "EDID", 0)?, interner) != Some(SOURCE_EDITOR_ID)
        || !source_bounds(field(record, "OBND", 0)?, interner)
        || string_value(field(record, "FULL", 0)?, interner) != Some(SOURCE_NAME)
        || !path_value(field(record, "MODL", 0)?, SOURCE_SKELETON, interner)
        || integer_value(field(record, "EAMT", 0)?) != Some(0)
        || bytes_value(field(record, "NIFZ", 0)?) != Some(SOURCE_BODY_LIST)
        || bytes_value(field(record, "NIFT", 0)?) != Some(&[0, 0, 0, 0])
        || !source_configuration(field(record, "ACBS", 0)?, interner)
        || !source_ai(field(record, "AIDT", 0)?, interner)
        || !source_stats(field(record, "DATA", 0)?, interner)
        || integer_value(field(record, "RNAM", 0)?) != Some(35)
        || float_bits(field(record, "TNAM", 0)?) != Some(200.0_f32.to_bits())
        || float_bits(field(record, "BNAM", 0)?) != Some(0x3F4C_CCCD)
        || float_bits(field(record, "WNAM", 0)?) != Some(5.0_f32.to_bits())
        || integer_value(field(record, "NAM4", 0)?) != Some(6)
        || integer_value(field(record, "NAM5", 0)?) != Some(1)
    {
        return Err(source_record_error(
            "critical scalar or model semantics drifted",
        ));
    }
    for (signature, occurrence, local) in [
        ("INAM", 0, 0x11_B9AF),
        ("VTCK", 0, 0x09_4EED),
        ("PKID", 0, 0x02_5482),
        ("PKID", 1, 0x07_4FE6),
        ("ZNAM", 0, 0x14_F405),
        ("PNAM", 0, 0x10_CD74),
        ("CSCR", 0, 0x14_5A43),
        ("CNAM", 0, 0x14_5A47),
    ] {
        if !source_form_key(
            field(record, signature, occurrence)?,
            record.form_key.plugin,
            local,
        ) {
            return Err(source_record_error(format!(
                "{signature}[{occurrence}] source dependency drifted"
            )));
        }
    }
    for (occurrence, expected) in [
        (0, &[0xB3, 0xB9, 0x11, 0x00, 0x00, 0x49, 0x46, 0x5A][..]),
        (1, &[0x9F, 0xFF, 0x05, 0x00, 0x00, 0x49, 0x46, 0x5A][..]),
        (2, &[0x13, 0x00, 0x00, 0x00, 0x00, 0x49, 0x46, 0x5A][..]),
    ] {
        if bytes_value(field(record, "SNAM", occurrence)?) != Some(expected) {
            return Err(source_record_error(format!(
                "SNAM[{occurrence}] faction row drifted"
            )));
        }
    }
    Ok(())
}

fn audit_source_assets(assets: &LegacyGeckoAssetEvidence) -> Result<(), GeckoAdapterError> {
    if normalize_path(&assets.body_nif) != normalize_path(SOURCE_BODY)
        || normalize_path(&assets.skeleton_nif) != normalize_path(SOURCE_SKELETON)
        || assets.skeleton_bones != source_bones()
    {
        return Err(source_assets_error(
            "body or 87-node Bip01 skeleton drifted",
        ));
    }
    let expected = clip_specs();
    if assets.clips.len() != expected.len() {
        return Err(source_assets_error(format!(
            "expected five canonical KF roles, got {}",
            assets.clips.len()
        )));
    }
    for (clip, spec) in assets.clips.iter().zip(expected) {
        let expected_nodes = source_track_nodes(spec.include_root, spec.include_weapon);
        if normalize_path(&clip.source_path) != normalize_path(spec.path)
            || clip.sequence_name != spec.sequence
            || clip.controlled_nodes != expected_nodes
            || clip.cycle_loop != spec.cycle_loop
            || clip.frequency.to_bits() != 1.0_f32.to_bits()
            || clip.start_time.to_bits() != 0
            || clip.stop_time.to_bits() != spec.stop_time_bits
            || clip.accumulation_root != "Bip01"
            || (clip.extracted_planar_reference_frames > 0) != spec.animation_driven
            || clip.text_keys.len() != spec.text_keys.len()
            || clip
                .text_keys
                .iter()
                .zip(spec.text_keys)
                .any(|(actual, (time_bits, text))| {
                    actual.time.to_bits() != *time_bits || actual.text != *text
                })
        {
            return Err(source_assets_error(format!(
                "KF contract drifted for {}",
                spec.path
            )));
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ClipSpec {
    path: &'static str,
    sequence: &'static str,
    target_role: &'static str,
    include_root: bool,
    include_weapon: bool,
    cycle_loop: bool,
    animation_driven: bool,
    stop_time_bits: u32,
    text_keys: &'static [(u32, &'static str)],
}

fn clip_specs() -> [ClipSpec; 5] {
    [
        ClipSpec {
            path: "creatures\\NVGecko\\mtidle.kf",
            sequence: "Idle",
            target_role: "Idle",
            include_root: false,
            include_weapon: false,
            cycle_loop: true,
            animation_driven: false,
            stop_time_bits: 0x4155_5556,
            text_keys: &[(0x0000_0000, "start"), (0x4155_5556, "end")],
        },
        ClipSpec {
            path: "creatures\\NVGecko\\swimmtforward.kf",
            sequence: "Forward",
            target_role: "WalkForward",
            include_root: true,
            include_weapon: false,
            cycle_loop: true,
            animation_driven: true,
            stop_time_bits: 0x3FCC_CCCD,
            text_keys: &FORWARD_TEXT_KEYS,
        },
        ClipSpec {
            path: "creatures\\NVGecko\\mtturnleft.kf",
            sequence: "TurnLeft",
            target_role: "TurnLeft90",
            include_root: false,
            include_weapon: false,
            cycle_loop: true,
            animation_driven: false,
            stop_time_bits: 0x3F4C_CCCD,
            text_keys: &[(0x0000_0000, "start"), (0x3F4C_CCCD, "end")],
        },
        ClipSpec {
            path: "creatures\\NVGecko\\mtturnright.kf",
            sequence: "TurnRight",
            target_role: "TurnRight90",
            include_root: true,
            include_weapon: false,
            cycle_loop: true,
            animation_driven: false,
            stop_time_bits: 0x3F4C_CCC8,
            text_keys: &[(0x0000_0000, "start"), (0x3F4C_CCC8, "end")],
        },
        ClipSpec {
            path: "creatures\\NVGecko\\h2hattackforwardpower.kf",
            sequence: "AttackForwardPower",
            target_role: "Attack1",
            include_root: true,
            include_weapon: true,
            cycle_loop: false,
            animation_driven: true,
            stop_time_bits: 0x3FD9_9998,
            text_keys: &ATTACK_TEXT_KEYS,
        },
    ]
}

fn source_bones() -> Vec<BoneDecl> {
    SOURCE_BONES
        .iter()
        .map(|(name, parent_index)| BoneDecl {
            name: (*name).to_string(),
            parent_index: *parent_index,
        })
        .collect()
}

fn source_track_map(bones: &[BoneDecl], include_root: bool, include_weapon: bool) -> Vec<usize> {
    let mut indices = Vec::with_capacity(bones.len());
    if include_root {
        indices.push(0);
    }
    for (index, bone) in bones.iter().enumerate().skip(2) {
        if bone.name == "ProjectileNode_Fire" || (!include_weapon && bone.name == "Weapon") {
            continue;
        }
        indices.push(index);
    }
    indices.push(1);
    indices
}

fn source_track_nodes(include_root: bool, include_weapon: bool) -> Vec<String> {
    let bones = source_bones();
    source_track_map(&bones, include_root, include_weapon)
        .into_iter()
        .map(|index| bones[index].name.clone())
        .collect()
}

fn validate_runtime_root(runtime_root: &str) -> Result<String, GeckoAdapterError> {
    let components = runtime_root.split('\\').collect::<Vec<_>>();
    let valid = components.len() == 2
        && components[0].eq_ignore_ascii_case("Actors")
        && components[1].starts_with("B21_")
        && components[1]
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !runtime_root.contains('/')
        && runtime_root == runtime_root.trim();
    if !valid {
        return Err(GeckoAdapterError::RuntimeRoot(runtime_root.to_string()));
    }
    Ok(components[1].to_string())
}

fn generic_event(name: &str) -> EventDecl {
    EventDecl {
        name: name.to_string(),
        usage: EventUsage::Generic,
        flags: 0,
    }
}

fn field<'a>(
    record: &'a Record,
    signature: &str,
    occurrence: usize,
) -> Result<&'a FieldValue, GeckoAdapterError> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .nth(occurrence)
        .map(|field| &field.value)
        .ok_or_else(|| source_record_error(format!("missing {signature}[{occurrence}]")))
}

fn string_value<'a>(value: &FieldValue, interner: &'a StringInterner) -> Option<&'a str> {
    match value {
        FieldValue::String(value) => interner.resolve(*value),
        _ => None,
    }
}

fn path_value(value: &FieldValue, expected: &str, interner: &StringInterner) -> bool {
    string_value(value, interner)
        .is_some_and(|value| normalize_path(value) == normalize_path(expected))
}

fn bytes_value(value: &FieldValue) -> Option<&[u8]> {
    match value {
        FieldValue::Bytes(value) => Some(value),
        _ => None,
    }
}

fn integer_value(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Int(value) => Some(*value),
        FieldValue::Uint(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn float_bits(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Float(value) if value.is_finite() => Some(value.to_bits()),
        _ => None,
    }
}

fn source_form_key(value: &FieldValue, plugin: crate::sym::Sym, local: u32) -> bool {
    matches!(value, FieldValue::FormKey(value) if value.plugin == plugin && value.local == local)
}

fn source_bounds(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Bytes(bytes) => {
            bytes.as_slice()
                == [-30_i16, -70, 0, 30, 70, 100]
                    .into_iter()
                    .flat_map(i16::to_le_bytes)
                    .collect::<Vec<_>>()
        }
        FieldValue::Struct(fields) if fields.len() == 6 => [
            ("object_bounds_x1", -30),
            ("object_bounds_y1", -70),
            ("object_bounds_z1", 0),
            ("object_bounds_x2", 30),
            ("object_bounds_y2", 70),
            ("object_bounds_z2", 100),
        ]
        .into_iter()
        .all(|(name, expected)| named_integer(fields, name, interner) == Some(expected)),
        _ => false,
    }
}

fn source_configuration(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Struct(fields) if fields.len() == 10 => {
            [
                ("flags", 20_972_104),
                ("fatigue", 80),
                ("barter_gold", 0),
                ("level", 7),
                ("calc_min", 0),
                ("calc_max", 0),
                ("speed_multiplier", 100),
                ("disposition_base", 35),
                ("template_flags", 0),
            ]
            .into_iter()
            .all(|(name, expected)| named_integer(fields, name, interner) == Some(expected))
                && named_float_bits(fields, "karma_alignment", interner) == Some(0)
        }
        FieldValue::Bytes(bytes) if bytes.len() == 24 => {
            read_u32(bytes, 0) == Some(20_972_104)
                && read_u16(bytes, 4) == Some(80)
                && read_u16(bytes, 6) == Some(0)
                && read_u16(bytes, 8) == Some(7)
                && read_u16(bytes, 10) == Some(0)
                && read_u16(bytes, 12) == Some(0)
                && read_u16(bytes, 14) == Some(100)
                && read_u32(bytes, 16) == Some(0)
                && read_i16(bytes, 20) == Some(35)
                && read_u16(bytes, 22) == Some(0)
        }
        _ => false,
    }
}

fn source_ai(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Struct(fields) if fields.len() == 14 => {
            [
                ("aggression", 2),
                ("confidence", 3),
                ("energy_level", 50),
                ("responsibility", 50),
                ("mood", 0),
                ("unknown_u8_5", 118),
                ("unknown_u8_6", 108),
                ("unknown_u8_7", 51),
                ("buys_sells_and_services", 0),
                ("maximum_training_level", 0),
                ("assistance", 2),
                ("aggro_radius_behavior", 1),
                ("aggro_radius", 1500),
            ]
            .into_iter()
            .all(|(name, expected)| named_integer(fields, name, interner) == Some(expected))
                && matches!(named_integer(fields, "teaches", interner), Some(-1 | 0))
        }
        FieldValue::Bytes(bytes) if bytes.len() == 20 => {
            bytes[0..8] == [2, 3, 50, 50, 0, 118, 108, 51]
                && read_u32(bytes, 8) == Some(0)
                && matches!(bytes[12], 0 | 255)
                && bytes[13..16] == [0, 2, 1]
                && read_i32(bytes, 16) == Some(1500)
        }
        _ => false,
    }
}

fn source_stats(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Struct(fields) if fields.len() == 15 => [
            ("type", 1),
            ("combat_skill", 40),
            ("magic_skill", 50),
            ("stealth_skill", 50),
            ("health", 65),
            ("unused_1", 0),
            ("unused_2", 0),
            ("damage", 30),
            ("attribute_strength", 6),
            ("attribute_perception", 5),
            ("attribute_endurance", 4),
            ("attribute_charisma", 3),
            ("attribute_intelligence", 6),
            ("attribute_agility", 7),
            ("attribute_luck", 3),
        ]
        .into_iter()
        .all(|(name, expected)| named_integer(fields, name, interner) == Some(expected)),
        FieldValue::Bytes(bytes) if bytes.len() == 17 => {
            bytes[0..4] == [1, 40, 50, 50]
                && read_i16(bytes, 4) == Some(65)
                && bytes[6..8] == [0, 0]
                && read_i16(bytes, 8) == Some(30)
                && bytes[10..17] == [6, 5, 4, 3, 6, 7, 3]
        }
        _ => false,
    }
}

fn named_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    let name = canonical_name(name);
    fields.iter().find_map(|(field_name, value)| {
        (canonical_name(interner.resolve(*field_name).unwrap_or_default()) == name).then_some(value)
    })
}

fn named_integer(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<i64> {
    integer_value(named_value(fields, name, interner)?)
}

fn named_float_bits(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    float_bits(named_value(fields, name, interner)?)
}

fn canonical_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_path(value: &str) -> String {
    value.replace('/', "\\").to_ascii_lowercase()
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    Some(i32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn source_record_error(message: impl Into<String>) -> GeckoAdapterError {
    GeckoAdapterError::SourceRecord(message.into())
}

fn source_assets_error(message: impl Into<String>) -> GeckoAdapterError {
    GeckoAdapterError::SourceAssets(message.into())
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};

    use nif_core_native::model::{NifBlock, NifFile, NifValue};
    use smallvec::SmallVec;

    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::phase::animations::{FnvClipMotion, FnvExtractedMotionPolicy, convert_kf_to_hkx};
    use crate::record::FieldEntry;
    use crate::schema::AuthoringSchema;
    use crate::source_rig::{TargetFormKey, validate_havok_xml};

    fn source_record(interner: &StringInterner) -> Record {
        let plugin = interner.intern(SOURCE_PLUGIN);
        let mut record = Record::new(
            SigCode::from_str("CREA").unwrap(),
            FormKey {
                local: SOURCE_LOCAL,
                plugin,
            },
        );
        record.eid = Some(interner.intern(SOURCE_EDITOR_ID));
        let source_ref =
            |signature, local| raw_field(signature, FieldValue::FormKey(FormKey { local, plugin }));
        record.fields = SmallVec::from_vec(vec![
            string_field("EDID", SOURCE_EDITOR_ID, interner),
            struct_field(
                "OBND",
                interner,
                vec![
                    ("object_bounds_x1", FieldValue::Int(-30)),
                    ("object_bounds_y1", FieldValue::Int(-70)),
                    ("object_bounds_z1", FieldValue::Int(0)),
                    ("object_bounds_x2", FieldValue::Int(30)),
                    ("object_bounds_y2", FieldValue::Int(70)),
                    ("object_bounds_z2", FieldValue::Int(100)),
                ],
            ),
            string_field("FULL", SOURCE_NAME, interner),
            string_field("MODL", SOURCE_SKELETON, interner),
            raw_field("EAMT", FieldValue::Uint(0)),
            bytes_field("NIFZ", SOURCE_BODY_LIST),
            bytes_field("NIFT", &[0, 0, 0, 0]),
            struct_field(
                "ACBS",
                interner,
                vec![
                    ("flags", FieldValue::Uint(20_972_104)),
                    ("fatigue", FieldValue::Uint(80)),
                    ("barter_gold", FieldValue::Uint(0)),
                    ("level", FieldValue::Uint(7)),
                    ("calc_min", FieldValue::Uint(0)),
                    ("calc_max", FieldValue::Uint(0)),
                    ("speed_multiplier", FieldValue::Uint(100)),
                    ("karma_alignment", FieldValue::Float(0.0)),
                    ("disposition_base", FieldValue::Int(35)),
                    ("template_flags", FieldValue::Uint(0)),
                ],
            ),
            bytes_field("SNAM", &[0xB3, 0xB9, 0x11, 0, 0, 0x49, 0x46, 0x5A]),
            bytes_field("SNAM", &[0x9F, 0xFF, 0x05, 0, 0, 0x49, 0x46, 0x5A]),
            bytes_field("SNAM", &[0x13, 0, 0, 0, 0, 0x49, 0x46, 0x5A]),
            source_ref("INAM", 0x11_B9AF),
            source_ref("VTCK", 0x09_4EED),
            struct_field(
                "AIDT",
                interner,
                vec![
                    ("aggression", FieldValue::Uint(2)),
                    ("confidence", FieldValue::Uint(3)),
                    ("energy_level", FieldValue::Uint(50)),
                    ("responsibility", FieldValue::Uint(50)),
                    ("mood", FieldValue::Uint(0)),
                    ("unknown_u8_5", FieldValue::Uint(118)),
                    ("unknown_u8_6", FieldValue::Uint(108)),
                    ("unknown_u8_7", FieldValue::Uint(51)),
                    ("buys_sells_and_services", FieldValue::Uint(0)),
                    ("teaches", FieldValue::Int(-1)),
                    ("maximum_training_level", FieldValue::Uint(0)),
                    ("assistance", FieldValue::Uint(2)),
                    ("aggro_radius_behavior", FieldValue::Uint(1)),
                    ("aggro_radius", FieldValue::Int(1500)),
                ],
            ),
            source_ref("PKID", 0x02_5482),
            source_ref("PKID", 0x07_4FE6),
            struct_field(
                "DATA",
                interner,
                vec![
                    ("type", FieldValue::Uint(1)),
                    ("combat_skill", FieldValue::Uint(40)),
                    ("magic_skill", FieldValue::Uint(50)),
                    ("stealth_skill", FieldValue::Uint(50)),
                    ("health", FieldValue::Int(65)),
                    ("unused_1", FieldValue::Uint(0)),
                    ("unused_2", FieldValue::Uint(0)),
                    ("damage", FieldValue::Int(30)),
                    ("attribute_strength", FieldValue::Uint(6)),
                    ("attribute_perception", FieldValue::Uint(5)),
                    ("attribute_endurance", FieldValue::Uint(4)),
                    ("attribute_charisma", FieldValue::Uint(3)),
                    ("attribute_intelligence", FieldValue::Uint(6)),
                    ("attribute_agility", FieldValue::Uint(7)),
                    ("attribute_luck", FieldValue::Uint(3)),
                ],
            ),
            raw_field("RNAM", FieldValue::Uint(35)),
            source_ref("ZNAM", 0x14_F405),
            source_ref("PNAM", 0x10_CD74),
            raw_field("TNAM", FieldValue::Float(200.0)),
            raw_field("BNAM", FieldValue::Float(f32::from_bits(0x3F4C_CCCD))),
            raw_field("WNAM", FieldValue::Float(5.0)),
            raw_field("NAM4", FieldValue::Uint(6)),
            raw_field("NAM5", FieldValue::Uint(1)),
            source_ref("CSCR", 0x14_5A43),
            source_ref("CNAM", 0x14_5A47),
        ]);
        record
    }

    fn source_assets() -> LegacyGeckoAssetEvidence {
        canonical_gecko_asset_evidence()
    }

    fn target_keys(plugin: &str) -> CreatureRecordFormKeys {
        let key = |offset: u32| TargetFormKey::new(0x900_u32 + offset, plugin);
        CreatureRecordFormKeys {
            race: key(0),
            npc: key(1),
            skin: key(2),
            armor_addon: key(3),
            body_part_data: key(4),
            unarmed_weapon: key(5),
        }
    }

    #[test]
    fn canonical_corpus_contract_emits_six_target_owned_records() {
        let interner = StringInterner::new();
        let target_plugin = "B21_FNVGeckoMVP.esp";
        let real_assets = load_real_asset_evidence_if_available();
        if default_gecko_mesh_root().is_some_and(|path| path.is_dir()) {
            assert!(
                real_assets.is_some(),
                "default Gecko asset corpus was not parsed"
            );
        }
        let (assets, serializer_clip_paths) = match real_assets {
            Some(real_assets) => (
                real_assets.evidence,
                Some(real_assets.serializer_clip_paths),
            ),
            None => (source_assets(), None),
        };
        let adapted = adapt_canonical_gecko(
            &source_record(&interner),
            &assets,
            target_plugin,
            target_keys(target_plugin),
            "Actors\\B21_FNVGecko",
            &interner,
        )
        .unwrap();
        let closure = adapted.emit_records(&interner).unwrap();
        let graph_clip_paths = adapted
            .rig
            .clips
            .iter()
            .map(|clip| clip.path.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            graph_clip_paths,
            [
                "Actors\\B21_FNVGecko\\Animations\\Idle.hkx",
                "Actors\\B21_FNVGecko\\Animations\\WalkForward.hkx",
                "Actors\\B21_FNVGecko\\Animations\\TurnLeft90.hkx",
                "Actors\\B21_FNVGecko\\Animations\\TurnRight90.hkx",
                "Actors\\B21_FNVGecko\\Animations\\Attack1.hkx",
            ]
        );
        if let Some(serializer_clip_paths) = serializer_clip_paths {
            assert_eq!(graph_clip_paths, serializer_clip_paths);
        }
        assert_eq!(
            adapted
                .rig
                .clips
                .iter()
                .map(|clip| clip.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Idle",
                "WalkForward",
                "TurnLeft90",
                "TurnRight90",
                "Attack1"
            ]
        );
        assert_eq!(adapted.graph.idle_clip, "Idle");
        assert_eq!(adapted.graph.walk_forward_clip, "WalkForward");
        assert_eq!(adapted.graph.turn_left_90_clip, "TurnLeft90");
        assert_eq!(adapted.graph.turn_right_90_clip, "TurnRight90");
        assert_eq!(adapted.graph.attack_1_clip, "Attack1");

        assert_eq!(closure.records.len(), 6);
        assert_eq!(
            closure
                .records
                .iter()
                .map(|record| record.sig.as_str())
                .collect::<Vec<_>>(),
            ["RACE", "NPC_", "ARMO", "ARMA", "BPTD", "WEAP"]
        );
        assert!(closure.records.iter().all(|record| {
            interner.resolve(record.form_key.plugin) == Some(target_plugin)
                && !record_contains_source_form_key(record, &interner)
        }));
        assert_eq!(adapted.profile.root_body_part.geometry_segment_index, 32);
        assert_eq!(adapted.profile.root_body_part.node, "Bip01");
        assert_eq!(adapted.graph.melee_event, MELEE_EVENT);
        assert!(adapted.graph.melee_event.starts_with("melee"));
        assert_eq!(adapted.motion.idle, ClipMotionPolicy::default());
        assert_eq!(adapted.motion.turn_left_90, ClipMotionPolicy::default());
        assert_eq!(adapted.motion.turn_right_90, ClipMotionPolicy::default());
        assert!(adapted.motion.walk_forward.animation_driven);
        assert!(
            adapted
                .motion
                .walk_forward
                .extracted_planar_reference_frames
                > 0
        );
        assert!(adapted.motion.attack_1.animation_driven);
        assert!(adapted.motion.attack_1.extracted_planar_reference_frames > 0);
        let hit = assets.clips[4]
            .text_keys
            .iter()
            .find(|event| event.text == "Hit")
            .unwrap();
        assert_eq!(hit.time.to_bits(), 0x3F6E_EEF0);
        assert!(
            adapted
                .rig
                .core
                .events
                .iter()
                .any(|event| event.name == "HitFrame")
        );
        assert!(
            adapted
                .records
                .body_nif
                .starts_with("Actors\\B21_FNVGecko\\")
        );
        assert!(
            adapted
                .rig
                .required_runtime_paths()
                .iter()
                .all(|path| path.starts_with("Actors\\B21_FNVGecko\\"))
        );
    }

    #[test]
    fn optional_real_falloutnv_gecko_corpus_emits_normalized_closure() {
        let Some(path) = fnv_corpus_path() else {
            return;
        };
        if !path.is_file() {
            return;
        }
        let handle = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            path.to_str().unwrap(),
            Some("fnv"),
            None,
            None,
            true,
        )
        .unwrap();
        let plugin_name = crate::source_read::plugin_name_for_handle(handle).unwrap();
        assert_eq!(plugin_name, SOURCE_PLUGIN);
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fnv").unwrap();
        let source = crate::source_read::read_record(
            handle,
            &format!("{SOURCE_LOCAL:06X}@{plugin_name}"),
            &schema,
            &interner,
        )
        .unwrap();
        let Some(assets) = load_real_asset_evidence_if_available() else {
            return;
        };
        let target_plugin = "B21_FNVGeckoMVP.esp";
        let adapted = adapt_canonical_gecko(
            &source,
            &assets.evidence,
            target_plugin,
            target_keys(target_plugin),
            "Actors\\B21_FNVGecko",
            &interner,
        )
        .unwrap();
        let closure = adapted.emit_records(&interner).unwrap();

        assert_eq!(closure.records.len(), 6);
        assert!(closure.records.iter().all(|record| {
            interner.resolve(record.form_key.plugin) == Some(target_plugin)
                && !record_contains_source_form_key(record, &interner)
        }));
        assert_eq!(adapted.profile.root_body_part.geometry_segment_index, 32);
        assert!(
            adapted
                .records
                .body_nif
                .starts_with("Actors\\B21_FNVGecko\\")
        );
    }

    #[test]
    fn source_record_scripts_unknown_fields_and_semantic_drift_fail_closed() {
        let interner = StringInterner::new();
        let target_plugin = "B21_FNVGeckoMVP.esp";
        let mut cases = Vec::new();

        let mut script = source_record(&interner);
        script.fields.push(raw_field(
            "SCRI",
            FieldValue::FormKey(FormKey {
                local: 1,
                plugin: interner.intern(SOURCE_PLUGIN),
            }),
        ));
        cases.push(script);

        let mut unknown = source_record(&interner);
        unknown.fields.push(raw_field("UNKN", FieldValue::Uint(0)));
        cases.push(unknown);

        let mut model = source_record(&interner);
        replace_field(
            &mut model,
            "MODL",
            0,
            FieldValue::String(interner.intern("creatures\\Molerat\\skeleton.nif")),
        );
        cases.push(model);

        let mut damage = source_record(&interner);
        let FieldValue::Struct(fields) = field_mut(&mut damage, "DATA", 0) else {
            panic!("DATA fixture")
        };
        let value = fields
            .iter_mut()
            .find(|(name, _)| interner.resolve(*name) == Some("damage"))
            .unwrap();
        value.1 = FieldValue::Int(31);
        cases.push(damage);

        for source in cases {
            assert!(matches!(
                adapt_canonical_gecko(
                    &source,
                    &source_assets(),
                    target_plugin,
                    target_keys(target_plugin),
                    "Actors\\B21_FNVGecko",
                    &interner,
                ),
                Err(GeckoAdapterError::SourceRecord(_))
            ));
        }
    }

    #[test]
    fn source_skeleton_and_kf_contract_drift_fail_closed() {
        let interner = StringInterner::new();
        let target_plugin = "B21_FNVGeckoMVP.esp";
        let mut assets = source_assets();
        assets.skeleton_bones[9].parent_index = Some(1);
        assert!(matches!(
            adapt_canonical_gecko(
                &source_record(&interner),
                &assets,
                target_plugin,
                target_keys(target_plugin),
                "Actors\\B21_FNVGecko",
                &interner,
            ),
            Err(GeckoAdapterError::SourceAssets(_))
        ));

        let mut assets = source_assets();
        assets.clips[4].text_keys[8].text = "NoHit".to_string();
        assert!(matches!(
            adapt_canonical_gecko(
                &source_record(&interner),
                &assets,
                target_plugin,
                target_keys(target_plugin),
                "Actors\\B21_FNVGecko",
                &interner,
            ),
            Err(GeckoAdapterError::SourceAssets(_))
        ));

        let mut assets = source_assets();
        assets.clips[1].extracted_planar_reference_frames = 0;
        assert!(matches!(
            adapt_canonical_gecko(
                &source_record(&interner),
                &assets,
                target_plugin,
                target_keys(target_plugin),
                "Actors\\B21_FNVGecko",
                &interner,
            ),
            Err(GeckoAdapterError::SourceAssets(_))
        ));

        let mut assets = source_assets();
        assets.clips[0].extracted_planar_reference_frames = 1;
        assert!(matches!(
            adapt_canonical_gecko(
                &source_record(&interner),
                &assets,
                target_plugin,
                target_keys(target_plugin),
                "Actors\\B21_FNVGecko",
                &interner,
            ),
            Err(GeckoAdapterError::SourceAssets(_))
        ));
    }

    struct ParsedRealGeckoAssets {
        evidence: LegacyGeckoAssetEvidence,
        serializer_clip_paths: Vec<String>,
    }

    fn load_real_asset_evidence_if_available() -> Option<ParsedRealGeckoAssets> {
        let mesh_root = gecko_mesh_root()?;
        if !mesh_root.is_dir() {
            return None;
        }
        for path in [
            SOURCE_BODY,
            SOURCE_SKELETON,
            "creatures\\NVGecko\\mtidle.kf",
            "creatures\\NVGecko\\swimmtforward.kf",
            "creatures\\NVGecko\\mtturnleft.kf",
            "creatures\\NVGecko\\mtturnright.kf",
            "creatures\\NVGecko\\h2hattackforwardpower.kf",
        ] {
            assert!(
                mesh_root.join(path.replace('\\', "/")).is_file(),
                "missing {path}"
            );
        }

        let skeleton_path = mesh_root.join(SOURCE_SKELETON.replace('\\', "/"));
        let skeleton_bones = parse_real_skeleton(&skeleton_path);
        let event_map = HashMap::from([("Hit".to_string(), "HitFrame".to_string())]);
        let output = tempfile::tempdir().unwrap();
        let mut clips = Vec::with_capacity(5);
        let mut serializer_clip_paths = Vec::with_capacity(5);
        for (index, spec) in clip_specs().iter().enumerate() {
            let source_path = mesh_root.join(spec.path.replace('\\', "/"));
            let mut evidence = parse_real_kf(&source_path, spec.path);
            let output_path = output.path().join(format!("{index}.hkx"));
            let runtime_clip_path =
                gecko_runtime_clip_path("Actors\\B21_FNVGecko", spec.target_role);
            let outcome = convert_kf_to_hkx(
                &source_path,
                &output_path,
                &event_map,
                spec.path,
                &runtime_clip_path,
                None,
                Some(&skeleton_path),
                Some("Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"),
                Some("NVGecko"),
                None,
                if spec.animation_driven {
                    FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame
                } else {
                    FnvExtractedMotionPolicy::RejectNonzero
                },
            )
            .unwrap();
            assert_eq!(outcome.output_clip_path, runtime_clip_path);
            serializer_clip_paths.push(outcome.output_clip_path.clone());
            assert!(
                outcome.warnings.is_empty(),
                "{}: {:?}",
                spec.path,
                outcome.warnings
            );
            evidence.extracted_planar_reference_frames = match outcome.motion {
                FnvClipMotion::InPlace => {
                    assert!(
                        !spec.animation_driven,
                        "{} unexpectedly became in-place",
                        spec.path
                    );
                    0
                }
                FnvClipMotion::ExtractedPlanar { sample_count, .. } => {
                    assert!(
                        spec.animation_driven,
                        "{} unexpectedly extracted motion",
                        spec.path
                    );
                    sample_count
                }
                FnvClipMotion::ExtractedPlanarYaw { .. } => {
                    panic!("{} unexpectedly extracted yaw", spec.path)
                }
            };
            if spec.sequence == "AttackForwardPower" {
                let packed = std::fs::read(&output_path).unwrap();
                let xml = havok_native::api::havok_hkx_to_xml(&packed).unwrap();
                assert!(xml.contains("HitFrame"));
            }
            clips.push(evidence);
        }

        Some(ParsedRealGeckoAssets {
            evidence: LegacyGeckoAssetEvidence {
                body_nif: SOURCE_BODY.to_string(),
                skeleton_nif: SOURCE_SKELETON.to_string(),
                skeleton_bones,
                clips,
            },
            serializer_clip_paths,
        })
    }

    fn parse_real_skeleton(path: &Path) -> Vec<BoneDecl> {
        let nif = NifFile::load(path).unwrap();
        let root = nif
            .blocks
            .iter()
            .find(|block| {
                block.type_name == "NiNode" && block_string(block, "Name") == Some("Bip01")
            })
            .map(|block| block.block_id)
            .expect("real Gecko skeleton Bip01 root");
        let mut bones = Vec::new();
        let mut visited = HashSet::new();
        visit_real_skeleton_node(&nif, root, None, &mut bones, &mut visited);
        let node_count = nif
            .blocks
            .iter()
            .filter(|block| block.type_name == "NiNode")
            .count();
        assert_eq!(bones.len(), node_count, "unreachable Gecko NiNode");
        bones
    }

    fn visit_real_skeleton_node(
        nif: &NifFile,
        block_id: usize,
        parent_index: Option<usize>,
        bones: &mut Vec<BoneDecl>,
        visited: &mut HashSet<usize>,
    ) {
        assert!(visited.insert(block_id), "Gecko skeleton node cycle");
        let block = &nif.blocks[block_id];
        assert_eq!(block.type_name, "NiNode");
        let index = bones.len();
        bones.push(BoneDecl {
            name: block_string(block, "Name").unwrap().to_string(),
            parent_index,
        });
        let Some(NifValue::Array(children)) = block.get_field("Children") else {
            return;
        };
        for child in children {
            let Some(child_id) = value_ref(child) else {
                continue;
            };
            if nif.blocks[child_id].type_name == "NiNode" {
                visit_real_skeleton_node(nif, child_id, Some(index), bones, visited);
            }
        }
    }

    fn parse_real_kf(path: &Path, source_path: &str) -> LegacyGeckoClipEvidence {
        let nif = NifFile::load(path).unwrap();
        let sequences = nif
            .blocks
            .iter()
            .filter(|block| block.type_name == "NiControllerSequence")
            .collect::<Vec<_>>();
        assert_eq!(
            sequences.len(),
            1,
            "{source_path} controller sequence count"
        );
        let sequence = sequences[0];
        let frequency = block_f32(sequence, "Frequency").unwrap();
        let start_time = block_f32(sequence, "Start Time").unwrap();
        let stop_time = block_f32(sequence, "Stop Time").unwrap();
        let cycle_loop = block_u32(sequence, "Cycle Type").unwrap() == 0;
        let controlled = block_array(sequence, "Controlled Blocks").unwrap();
        assert_eq!(
            block_u32(sequence, "Num Controlled Blocks").unwrap() as usize,
            controlled.len(),
            "{source_path} controlled-block count"
        );
        let controlled_nodes = controlled
            .iter()
            .filter_map(|entry| {
                let interpolator = struct_ref(entry, "Interpolator")?;
                let interpolator_type = nif.blocks.get(interpolator)?.type_name.as_str();
                assert!(
                    matches!(
                        interpolator_type,
                        "NiTransformInterpolator" | "NiBSplineCompTransformInterpolator"
                    ),
                    "{source_path} unsupported interpolator {interpolator_type}"
                );
                Some(struct_string(entry, "Node Name").unwrap().to_string())
            })
            .collect::<Vec<_>>();

        let text_key_block = block_ref(sequence, "Text Keys").unwrap();
        let text_key_block = &nif.blocks[text_key_block];
        assert_eq!(text_key_block.type_name, "NiTextKeyExtraData");
        let text_keys = block_array(text_key_block, "Text Keys").unwrap();
        assert_eq!(
            block_u32(text_key_block, "Num Text Keys").unwrap() as usize,
            text_keys.len(),
            "{source_path} text-key count"
        );
        let text_keys = text_keys
            .iter()
            .map(|entry| LegacyGeckoTextKeyEvidence {
                time: struct_f32(entry, "Time").unwrap(),
                text: struct_string(entry, "Value").unwrap().to_string(),
            })
            .collect();

        LegacyGeckoClipEvidence {
            source_path: source_path.to_string(),
            sequence_name: block_string(sequence, "Name").unwrap().to_string(),
            controlled_nodes,
            cycle_loop,
            frequency,
            start_time,
            stop_time,
            accumulation_root: block_string(sequence, "Accum Root Name")
                .unwrap()
                .to_string(),
            text_keys,
            extracted_planar_reference_frames: 0,
        }
    }

    fn block_string<'a>(block: &'a NifBlock, name: &str) -> Option<&'a str> {
        value_string(block.get_field(name)?)
    }

    fn block_f32(block: &NifBlock, name: &str) -> Option<f32> {
        value_f32(block.get_field(name)?)
    }

    fn block_u32(block: &NifBlock, name: &str) -> Option<u32> {
        value_u32(block.get_field(name)?)
    }

    fn block_ref(block: &NifBlock, name: &str) -> Option<usize> {
        value_ref(block.get_field(name)?)
    }

    fn block_array<'a>(block: &'a NifBlock, name: &str) -> Option<&'a [NifValue]> {
        match block.get_field(name)? {
            NifValue::Array(values) => Some(values),
            _ => None,
        }
    }

    fn struct_value<'a>(value: &'a NifValue, name: &str) -> Option<&'a NifValue> {
        let NifValue::Struct(fields) = value else {
            return None;
        };
        fields.get(name).or_else(|| {
            fields
                .iter()
                .find(|(field_name, _)| field_name.split(':').next() == Some(name))
                .map(|(_, value)| value)
        })
    }

    fn struct_string<'a>(value: &'a NifValue, name: &str) -> Option<&'a str> {
        value_string(struct_value(value, name)?)
    }

    fn struct_f32(value: &NifValue, name: &str) -> Option<f32> {
        value_f32(struct_value(value, name)?)
    }

    fn struct_ref(value: &NifValue, name: &str) -> Option<usize> {
        value_ref(struct_value(value, name)?)
    }

    fn value_string(value: &NifValue) -> Option<&str> {
        match value {
            NifValue::String(value) | NifValue::Char(value) => Some(value),
            _ => None,
        }
    }

    fn value_f32(value: &NifValue) -> Option<f32> {
        match value {
            NifValue::Float(value) => Some(*value as f32),
            NifValue::Int(value) => Some(*value as f32),
            NifValue::UInt(value) => Some(*value as f32),
            _ => None,
        }
    }

    fn value_u32(value: &NifValue) -> Option<u32> {
        match value {
            NifValue::UInt(value) => (*value).try_into().ok(),
            NifValue::Int(value) => (*value).try_into().ok(),
            _ => None,
        }
    }

    fn value_ref(value: &NifValue) -> Option<usize> {
        match value {
            NifValue::Ref(value) if *value >= 0 => Some(*value as usize),
            NifValue::Int(value) if *value >= 0 => Some(*value as usize),
            _ => None,
        }
    }

    fn gecko_mesh_root() -> Option<PathBuf> {
        std::env::var_os("FNV_GECKO_ASSET_ROOT")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("FONV_EXTRACTED_DIR")
                    .map(PathBuf::from)
                    .map(|path| path.join("Meshes"))
            })
            .or_else(default_gecko_mesh_root)
    }

    fn repository_root() -> Option<PathBuf> {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| path.join("AGENTS.md").is_file() && path.join("bacup").is_dir())
            .map(Path::to_path_buf)
    }

    fn default_gecko_mesh_root() -> Option<PathBuf> {
        repository_root().map(|root| root.join("extracted").join("fnv").join("Meshes"))
    }

    fn fnv_corpus_path() -> Option<PathBuf> {
        std::env::var_os("FNV_GECKO_CORPUS_PLUGIN")
            .map(PathBuf::from)
            .or_else(|| {
                ["FONV_DIR", "FNV_DIR"].into_iter().find_map(|name| {
                    std::env::var_os(name)
                        .map(PathBuf::from)
                        .map(|path| path.join("Data").join("FalloutNV.esm"))
                })
            })
    }

    fn record_contains_source_form_key(record: &Record, interner: &StringInterner) -> bool {
        fn contains(value: &FieldValue, interner: &StringInterner) -> bool {
            match value {
                FieldValue::FormKey(value) => {
                    value.local != 0
                        && interner.resolve(value.plugin).is_some_and(|plugin| {
                            matches!(plugin, "FalloutNV.esm" | "Fallout3.esm")
                        })
                }
                FieldValue::List(values) => values.iter().any(|value| contains(value, interner)),
                FieldValue::Struct(fields) => {
                    fields.iter().any(|(_, value)| contains(value, interner))
                }
                _ => false,
            }
        }
        record
            .fields
            .iter()
            .any(|field| contains(&field.value, interner))
    }

    fn raw_field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn string_field(signature: &str, value: &str, interner: &StringInterner) -> FieldEntry {
        raw_field(signature, FieldValue::String(interner.intern(value)))
    }

    fn bytes_field(signature: &str, value: &[u8]) -> FieldEntry {
        raw_field(signature, FieldValue::Bytes(SmallVec::from_slice(value)))
    }

    fn struct_field(
        signature: &str,
        interner: &StringInterner,
        fields: Vec<(&str, FieldValue)>,
    ) -> FieldEntry {
        raw_field(
            signature,
            FieldValue::Struct(
                fields
                    .into_iter()
                    .map(|(name, value)| (interner.intern(name), value))
                    .collect(),
            ),
        )
    }

    fn replace_field(record: &mut Record, signature: &str, occurrence: usize, value: FieldValue) {
        *field_mut(record, signature, occurrence) = value;
    }

    fn field_mut<'a>(
        record: &'a mut Record,
        signature: &str,
        occurrence: usize,
    ) -> &'a mut FieldValue {
        &mut record
            .fields
            .iter_mut()
            .filter(|field| field.sig.as_str() == signature)
            .nth(occurrence)
            .unwrap()
            .value
    }

    #[test]
    fn adapter_output_scaffold_xml_remains_structurally_valid() {
        let interner = StringInterner::new();
        let target_plugin = "B21_FNVGeckoMVP.esp";
        let adapted = adapt_canonical_gecko(
            &source_record(&interner),
            &source_assets(),
            target_plugin,
            target_keys(target_plugin),
            "Actors\\B21_FNVGecko",
            &interner,
        )
        .unwrap();
        let scaffold = crate::source_rig::emit_mvp_scaffold_with_motion(
            &adapted.rig,
            &adapted.graph,
            &adapted.motion,
        )
        .unwrap();
        assert_eq!(scaffold.artifacts.len(), 4);
        assert!(
            scaffold
                .artifacts
                .iter()
                .all(|artifact| validate_havok_xml(&artifact.xml).is_ok())
        );
        let core = scaffold
            .artifacts
            .iter()
            .find(|artifact| artifact.runtime_path == adapted.rig.paths.core_behavior)
            .unwrap();
        assert!(core.xml.contains("startAnimationDriven"));
        assert!(core.xml.contains("bAnimationDriven"));
        assert!(core.xml.contains("HitFrame"));
    }
}
