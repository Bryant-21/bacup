use thiserror::Error;

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::source_rig::{
    BoneDecl, Capsule, ClipBinding, ClipDecl, CreatureManifest, CreatureRecordEditorIds,
    CreatureRecordFormKeys, CreatureRecordManifest, CreatureRecordProfile, EventDecl, EventUsage,
    GraphDeclarations, MvpGraphManifest, MvpMotionManifest, ScaffoldPaths, SkeletonDecl,
    VariableDecl, VariableType, VariableValue,
};
use crate::sym::StringInterner;

const SKYRIM_MASTER: &str = "Skyrim.esm";
const WOLF_NPC_LOCAL: u32 = 0x0753CE;
const WOLF_RACE_LOCAL: u32 = 0x01320A;
const WOLF_SKIN_LOCAL: u32 = 0x04E886;
const WOLF_ARMOR_ADDON_LOCAL: u32 = 0x04E885;
const WOLF_BODY_PART_DATA_LOCAL: u32 = 0x04FBF5;
const UNARMED_WEAPON_LOCAL: u32 = 0x0001F4;
const RIGHT_HAND_LOCAL: u32 = 0x013F42;
const LEFT_HAND_LOCAL: u32 = 0x013F43;
const BOTH_HANDS_LOCAL: u32 = 0x013F45;

const SOURCE_SKELETON: &str = "Actors\\Canine\\Character Assets Wolf\\skeleton.nif";
const SOURCE_PROJECT: &str = "Actors\\Canine\\WolfProject.hkx";
const SOURCE_BODY: &str = "Actors\\Canine\\Character Assets Wolf\\wolf.nif";
const SOURCE_BPTD_SKELETON: &str = "Actors\\Canine\\Character Assets Dog\\skeleton.nif";
const TARGET_RUNTIME_ROOT: &str = "Actors\\B21_SkyrimWolf";

const NPC_FIELDS: &[&str] = &[
    "EDID", "OBND", "ACBS", "SNAM", "INAM", "VTCK", "RNAM", "WNAM", "ATKR", "AIDT", "CNAM", "FULL",
    "DATA", "DNAM", "ZNAM", "NAM5", "NAM6", "NAM7", "NAM8", "CSCR", "DPLT", "QNAM",
];
const RACE_FIELDS: &[&str] = &[
    "EDID", "FULL", "DESC", "WNAM", "BOD2", "KSIZ", "KWDA", "DATA", "MNAM", "ANAM", "MODT", "FNAM",
    "MTNM", "VTCK", "PNAM", "UNAM", "ATKD", "ATKE", "NAM1", "INDX", "MODL", "GNAM", "NAM3", "NAM4",
    "NAM5", "ONAM", "LNAM", "NAME", "VNAM", "QNAM", "UNES",
];
const SKIN_FIELDS: &[&str] = &[
    "EDID", "OBND", "BOD2", "RNAM", "DESC", "MODL", "DATA", "DNAM",
];
const ARMOR_ADDON_FIELDS: &[&str] = &[
    "EDID", "BODT", "RNAM", "DNAM", "MOD2", "MO2T", "MODL", "SNDD",
];
const BODY_PART_FIELDS: &[&str] = &[
    "EDID", "MODL", "MODT", "BPTN", "BPNN", "BPNT", "BPNI", "BPND", "NAM1", "NAM4", "NAM5",
];
const UNARMED_FIELDS: &[&str] = &[
    "EDID", "OBND", "ETYP", "DESC", "INAM", "TNAM", "NAM9", "NAM8", "DATA", "DNAM", "CRDT", "VNAM",
];
const EQUIP_FIELDS: &[&str] = &["EDID", "PNAM", "DATA"];

pub struct SkyrimWolfSourceRecords<'a> {
    pub npc: &'a Record,
    pub race: &'a Record,
    pub skin: &'a Record,
    pub armor_addon: &'a Record,
    pub body_part_data: &'a Record,
    pub unarmed_weapon: &'a Record,
    pub right_hand: &'a Record,
    pub left_hand: &'a Record,
    pub both_hands: &'a Record,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkyrimWolfRecordTarget {
    pub target_plugin: String,
    pub form_keys: CreatureRecordFormKeys,
    pub runtime_root: String,
    pub resolved_source_name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkyrimWolfRecordProjection {
    pub manifest: CreatureRecordManifest,
    pub profile: CreatureRecordProfile,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SkyrimWolfRecordError {
    #[error("invalid Skyrim wolf {record}: {reason}")]
    InvalidSource {
        record: &'static str,
        reason: String,
    },
    #[error("invalid Skyrim wolf target: {0}")]
    InvalidTarget(String),
}

pub(crate) fn canonical_skyrim_wolf_runtime_contract()
-> (CreatureManifest, MvpGraphManifest, MvpMotionManifest) {
    const BONES: [(&str, Option<usize>); 50] = [
        ("NPC Root [Root]", None),
        ("Canine_COM", Some(0)),
        ("Canine_Pelvis", Some(1)),
        ("Canine_Spine1", Some(1)),
        ("Canine_Spine2", Some(3)),
        ("Canine_Spine3", Some(4)),
        ("Canine_Ribcage", Some(5)),
        ("Canine_Neck1", Some(6)),
        ("Canine_Neck2", Some(7)),
        ("Canine_Head", Some(8)),
        ("Canine_LFrontLegShoulderblade", Some(6)),
        ("Canine_RFrontLegShoulderblade", Some(6)),
        ("Canine_LFrontLeg1", Some(10)),
        ("Canine_RFrontLeg1", Some(11)),
        ("Canine_LBackLeg1", Some(2)),
        ("Canine_RBackLeg1", Some(2)),
        ("Canine_LFrontLeg2", Some(12)),
        ("Canine_RFrontLeg2", Some(13)),
        ("Canine_LBackLeg2", Some(14)),
        ("Canine_RBackLeg2", Some(15)),
        ("Canine_LFrontLegPalm", Some(16)),
        ("Canine_RFrontLegPalm", Some(17)),
        ("Canine_LBackLegPalm", Some(18)),
        ("Canine_RBackLegPalm", Some(19)),
        ("Canine_Tail1", Some(2)),
        ("Canine_Tail2", Some(24)),
        ("Canine_Tail3", Some(25)),
        ("Canine_LFrontLegToe", Some(20)),
        ("Canine_RFrontLegToe", Some(21)),
        ("Canine_LBackLegToe", Some(22)),
        ("Canine_RBackLegToe", Some(23)),
        ("Canine_JawBone", Some(9)),
        ("Canine_LEar_Wolf", Some(9)),
        ("Canine_REar_Wolf", Some(9)),
        ("Canine_LEar01", Some(9)),
        ("Canine_LEar02", Some(34)),
        ("Canine_REar01", Some(9)),
        ("Canine_REar02", Some(36)),
        ("Canine_Dog_LEyelid", Some(9)),
        ("Canine_Dog_REyelid", Some(9)),
        ("Canine_LEye", Some(9)),
        ("Canine_REye", Some(9)),
        ("Canine_FrontLip", Some(9)),
        ("Canine_Tongue01", Some(31)),
        ("Canine_Tongue02", Some(43)),
        ("Canine_Tongue03", Some(44)),
        ("Canine_LUpperLip", Some(9)),
        ("Canine_RUpperLip", Some(9)),
        ("Canine_Dog_LBrow", Some(9)),
        ("Canine_Dog_RBrow", Some(9)),
    ];
    let runtime_root = "Actors\\B21_SkyrimWolf";
    let skeleton_path = format!("{runtime_root}\\CharacterAssets\\Skeleton.hkx");
    let clip = |name: &str, looping: bool| ClipDecl {
        name: name.to_string(),
        path: format!("{runtime_root}\\Animations\\{name}.hkx"),
        binding: ClipBinding {
            skeleton_path: skeleton_path.clone(),
            original_skeleton_name: "B21_SkyrimWolfSkeleton".to_string(),
            declared_transform_tracks: BONES.len(),
            transform_track_to_bone_indices: (0..BONES.len()).collect(),
            declared_float_tracks: 0,
            float_track_to_float_slot_indices: Vec::new(),
        },
        looping,
    };
    let declarations = GraphDeclarations {
        events: [
            ("Idle", EventUsage::Generic),
            ("startWalk", EventUsage::Generic),
            ("TurnLeft90", EventUsage::Generic),
            ("TurnRight90", EventUsage::Generic),
            ("meleeWolfAttack1", EventUsage::MeleeAttack),
            ("HitFrame", EventUsage::Generic),
        ]
        .into_iter()
        .map(|(name, usage)| EventDecl {
            name: name.to_string(),
            usage,
            flags: 0,
        })
        .collect(),
        variables: vec![VariableDecl {
            name: "bGraphDriven".to_string(),
            variable_type: VariableType::Bool,
            initial_value: VariableValue::Bool(true),
        }],
        character_properties: Vec::new(),
    };
    let rig = CreatureManifest {
        creature_name: "B21_SkyrimWolf".to_string(),
        visual_skeleton_nif: format!("{runtime_root}\\CharacterAssets\\Skeleton.nif"),
        animation_skeleton: SkeletonDecl {
            path: skeleton_path.clone(),
            runtime_name: "B21_SkyrimWolfSkeleton".to_string(),
            bones: BONES
                .into_iter()
                .map(|(name, parent_index)| BoneDecl {
                    name: name.to_string(),
                    parent_index,
                })
                .collect(),
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
        clips: vec![
            clip("Idle", true),
            clip("WalkForward", true),
            clip("TurnLeft90", false),
            clip("TurnRight90", false),
            clip("Attack1", false),
        ],
        idle_clip: "Idle".to_string(),
        capsule: Capsule {
            height: 1.7,
            radius: 0.4,
        },
        paths: ScaffoldPaths {
            project: format!("{runtime_root}\\B21_SkyrimWolfProject.hkx"),
            character: format!("{runtime_root}\\Characters\\B21_SkyrimWolfCharacter.hkx"),
            root_behavior: format!("{runtime_root}\\Behaviors\\B21_SkyrimWolfRootBehavior.hkx"),
            core_behavior: format!("{runtime_root}\\Behaviors\\B21_SkyrimWolfCoreBehavior.hkx"),
        },
        root: declarations.clone(),
        core: declarations,
    };
    let graph = MvpGraphManifest {
        idle_clip: "Idle".to_string(),
        walk_forward_clip: "WalkForward".to_string(),
        turn_left_90_clip: "TurnLeft90".to_string(),
        turn_right_90_clip: "TurnRight90".to_string(),
        attack_1_clip: "Attack1".to_string(),
        melee_event: "meleeWolfAttack1".to_string(),
    };
    (rig, graph, MvpMotionManifest::default())
}

pub fn adapt_skyrim_wolf_records(
    source: SkyrimWolfSourceRecords<'_>,
    target: SkyrimWolfRecordTarget,
    interner: &StringInterner,
) -> Result<SkyrimWolfRecordProjection, SkyrimWolfRecordError> {
    validate_target(&target)?;
    let npc_level = validate_npc(source.npc, interner)?;
    let attack = validate_race(source.race, interner)?;
    validate_skin(source.skin, interner)?;
    validate_armor_addon(source.armor_addon, interner)?;
    validate_body_part_data(source.body_part_data, interner)?;
    let unarmed_reach = validate_unarmed(source.unarmed_weapon, interner)?;
    validate_equip_closure(
        source.right_hand,
        source.left_hand,
        source.both_hands,
        interner,
    )?;

    let mut profile = CreatureRecordProfile::root_segment_32("NPC Root [Root]");
    profile.attack_damage_multiplier = attack.damage_multiplier;
    profile.attack_chance = attack.chance;
    profile.attack_strike_angle = attack.strike_angle;
    profile.unarmed_reach = unarmed_reach;
    profile.npc_level = npc_level;

    Ok(SkyrimWolfRecordProjection {
        manifest: CreatureRecordManifest {
            target_plugin: target.target_plugin,
            editor_id_prefix: "B21_".to_string(),
            form_keys: target.form_keys,
            editor_ids: CreatureRecordEditorIds {
                race: "B21_SkyrimWolfRace".to_string(),
                npc: "B21_SkyrimWolfNPC".to_string(),
                skin: "B21_SkyrimWolfSkin".to_string(),
                armor_addon: "B21_SkyrimWolfBodyAA".to_string(),
                body_part_data: "B21_SkyrimWolfBodyPartData".to_string(),
                unarmed_weapon: "B21_SkyrimWolfUnarmed".to_string(),
            },
            display_name: target.resolved_source_name,
            body_nif: format!(
                "{}\\CharacterAssets\\B21_SkyrimWolf.nif",
                target.runtime_root
            ),
        },
        profile,
    })
}

#[derive(Clone, Copy)]
struct AttackProfile {
    damage_multiplier: f32,
    chance: f32,
    strike_angle: f32,
}

fn validate_npc(record: &Record, interner: &StringInterner) -> Result<u16, SkyrimWolfRecordError> {
    require_record(record, "NPC_", WOLF_NPC_LOCAL, "EncWolfDead", interner)?;
    if record.flags.bits() != 0x000C_0000 {
        return source_error("NPC_", "record flags drifted from 000C0000");
    }
    require_supported_fields(record, NPC_FIELDS, "NPC_")?;
    reject_fields(record, &["VMAD", "TPLT", "EITM", "SPLO"], "NPC_")?;
    require_form_key(record, "RNAM", WOLF_RACE_LOCAL, "NPC_")?;
    require_form_key(record, "WNAM", WOLF_SKIN_LOCAL, "NPC_")?;
    require_form_key(record, "ATKR", WOLF_RACE_LOCAL, "NPC_")?;
    require_count(record, "FULL", 1, "NPC_")?;

    let acbs = require_bytes(record, "ACBS", "NPC_")?;
    if acbs.len() != 24 {
        return source_error("NPC_", format!("ACBS width is {}, expected 24", acbs.len()));
    }
    let level = read_u16(acbs, 8).ok_or_else(|| invalid_source("NPC_", "ACBS has no level"))?;
    if level != 2 {
        return source_error("NPC_", format!("level is {level}, expected 2"));
    }
    Ok(level)
}

fn validate_race(
    record: &Record,
    interner: &StringInterner,
) -> Result<AttackProfile, SkyrimWolfRecordError> {
    require_record(record, "RACE", WOLF_RACE_LOCAL, "WolfRace", interner)?;
    if record.flags.bits() != 0 {
        return source_error("RACE", "record flags are nonzero");
    }
    require_supported_fields(record, RACE_FIELDS, "RACE")?;
    reject_fields(record, &["VMAD"], "RACE")?;
    require_form_key(record, "WNAM", WOLF_SKIN_LOCAL, "RACE")?;
    require_form_key(record, "GNAM", WOLF_BODY_PART_DATA_LOCAL, "RACE")?;
    require_form_key(record, "QNAM", RIGHT_HAND_LOCAL, "RACE")?;
    require_form_key(record, "UNES", RIGHT_HAND_LOCAL, "RACE")?;
    require_matching_strings(record, "ANAM", SOURCE_SKELETON, 2, "RACE", interner)?;
    require_matching_strings(record, "MODL", SOURCE_PROJECT, 2, "RACE", interner)?;

    let attack_index = record
        .fields
        .iter()
        .position(|field| {
            field.sig.as_str() == "ATKE"
                && string_value(&field.value, interner) == Some("attackStart_Attack1")
        })
        .ok_or_else(|| invalid_source("RACE", "missing attackStart_Attack1"))?;
    if record.fields.iter().skip(attack_index + 1).any(|field| {
        field.sig.as_str() == "ATKE"
            && string_value(&field.value, interner) == Some("attackStart_Attack1")
    }) {
        return source_error("RACE", "attackStart_Attack1 is repeated");
    }
    let Some(attack_data) = attack_index
        .checked_sub(1)
        .and_then(|index| record.fields.get(index))
        .filter(|field| field.sig.as_str() == "ATKD")
        .and_then(|field| bytes_value(&field.value))
    else {
        return source_error("RACE", "attackStart_Attack1 has no adjacent ATKD bytes");
    };
    if attack_data.len() != 44 {
        return source_error(
            "RACE",
            format!("Attack1 ATKD width is {}, expected 44", attack_data.len()),
        );
    }
    if read_u32(attack_data, 8) != Some(0) || read_u32(attack_data, 12) != Some(0) {
        return source_error(
            "RACE",
            "Attack1 unexpectedly carries a spell or attack flags",
        );
    }
    let damage_multiplier = read_f32(attack_data, 0)
        .ok_or_else(|| invalid_source("RACE", "Attack1 has no damage multiplier"))?;
    let chance = read_f32(attack_data, 4)
        .ok_or_else(|| invalid_source("RACE", "Attack1 has no attack chance"))?;
    let strike_angle = read_f32(attack_data, 20)
        .ok_or_else(|| invalid_source("RACE", "Attack1 has no strike angle"))?;
    if !damage_multiplier.is_finite()
        || damage_multiplier <= 0.0
        || !chance.is_finite()
        || !(0.0..=1.0).contains(&chance)
        || !strike_angle.is_finite()
        || !(0.0..=180.0).contains(&strike_angle)
    {
        return source_error("RACE", "Attack1 carries unsafe numeric values");
    }
    Ok(AttackProfile {
        damage_multiplier,
        chance,
        strike_angle,
    })
}

fn validate_skin(record: &Record, interner: &StringInterner) -> Result<(), SkyrimWolfRecordError> {
    require_record(record, "ARMO", WOLF_SKIN_LOCAL, "SkinWolf", interner)?;
    if record.flags.bits() != 0x0000_0004 {
        return source_error("ARMO", "record flags drifted from 00000004");
    }
    require_supported_fields(record, SKIN_FIELDS, "ARMO")?;
    reject_fields(record, &["VMAD", "EITM", "EAMT", "TNAM"], "ARMO")?;
    require_form_key(record, "RNAM", WOLF_RACE_LOCAL, "ARMO")?;
    require_form_key(record, "MODL", WOLF_ARMOR_ADDON_LOCAL, "ARMO")
}

fn validate_armor_addon(
    record: &Record,
    interner: &StringInterner,
) -> Result<(), SkyrimWolfRecordError> {
    require_record(
        record,
        "ARMA",
        WOLF_ARMOR_ADDON_LOCAL,
        "NakedWolfAA",
        interner,
    )?;
    if record.flags.bits() != 0 {
        return source_error("ARMA", "record flags are nonzero");
    }
    require_supported_fields(record, ARMOR_ADDON_FIELDS, "ARMA")?;
    reject_fields(record, &["VMAD"], "ARMA")?;
    require_form_key(record, "RNAM", WOLF_RACE_LOCAL, "ARMA")?;
    require_matching_strings(record, "MOD2", SOURCE_BODY, 1, "ARMA", interner)
}

fn validate_body_part_data(
    record: &Record,
    interner: &StringInterner,
) -> Result<(), SkyrimWolfRecordError> {
    require_record(
        record,
        "BPTD",
        WOLF_BODY_PART_DATA_LOCAL,
        "DogBodyPartData",
        interner,
    )?;
    if record.flags.bits() != 0 {
        return source_error("BPTD", "record flags are nonzero");
    }
    require_supported_fields(record, BODY_PART_FIELDS, "BPTD")?;
    require_matching_strings(record, "MODL", SOURCE_BPTD_SKELETON, 1, "BPTD", interner)?;
    require_matching_strings(record, "BPNN", "Canine_Head", 1, "BPTD", interner)?;
    require_matching_strings(record, "BPNN", "Canine_Pelvis", 1, "BPTD", interner)?;
    require_count(record, "BPND", 2, "BPTD")
}

fn validate_unarmed(
    record: &Record,
    interner: &StringInterner,
) -> Result<f32, SkyrimWolfRecordError> {
    require_record(record, "WEAP", UNARMED_WEAPON_LOCAL, "Unarmed", interner)?;
    if record.flags.bits() != 0 {
        return source_error("WEAP", "record flags are nonzero");
    }
    require_supported_fields(record, UNARMED_FIELDS, "WEAP")?;
    reject_fields(record, &["VMAD", "EITM", "EAMT", "CNAM"], "WEAP")?;
    require_form_key(record, "ETYP", BOTH_HANDS_LOCAL, "WEAP")?;
    let game_data = require_bytes(record, "DATA", "WEAP")?;
    if game_data.len() != 10 {
        return source_error(
            "WEAP",
            format!("DATA width is {}, expected 10", game_data.len()),
        );
    }
    let weapon_data = require_bytes(record, "DNAM", "WEAP")?;
    if weapon_data.len() != 100 {
        return source_error(
            "WEAP",
            format!("DNAM width is {}, expected 100", weapon_data.len()),
        );
    }
    let speed =
        read_f32(weapon_data, 4).ok_or_else(|| invalid_source("WEAP", "DNAM has no speed"))?;
    let reach =
        read_f32(weapon_data, 8).ok_or_else(|| invalid_source("WEAP", "DNAM has no reach"))?;
    if speed != 1.0 || reach != 1.0 {
        return source_error(
            "WEAP",
            format!("Unarmed speed/reach drifted from 1.0/1.0 to {speed}/{reach}"),
        );
    }
    Ok(reach)
}

fn validate_equip_closure(
    right_hand: &Record,
    left_hand: &Record,
    both_hands: &Record,
    interner: &StringInterner,
) -> Result<(), SkyrimWolfRecordError> {
    for (record, local, editor_id, use_all_parents) in [
        (right_hand, RIGHT_HAND_LOCAL, "RightHand", false),
        (left_hand, LEFT_HAND_LOCAL, "LeftHand", false),
        (both_hands, BOTH_HANDS_LOCAL, "BothHands", true),
    ] {
        require_record(record, "EQUP", local, editor_id, interner)?;
        if record.flags.bits() != 0 {
            return source_error("EQUP", format!("{editor_id} record flags are nonzero"));
        }
        require_supported_fields(record, EQUIP_FIELDS, "EQUP")?;
        let actual = require_scalar_bool(record, "DATA", "EQUP")?;
        if actual != use_all_parents {
            return source_error(
                "EQUP",
                format!("{editor_id} UseAllParents is {actual}, expected {use_all_parents}"),
            );
        }
    }
    let parents = exact_field(both_hands, "PNAM")
        .and_then(|value| match value {
            FieldValue::List(values) => Some(values),
            _ => None,
        })
        .ok_or_else(|| invalid_source("EQUP", "BothHands has no parent array"))?;
    let expected = [LEFT_HAND_LOCAL, RIGHT_HAND_LOCAL];
    if parents.len() != expected.len()
        || parents.iter().zip(expected).any(|(value, local)| {
            !matches!(value, FieldValue::FormKey(form_key) if source_form_key(*form_key, local, interner))
        })
    {
        return source_error("EQUP", "BothHands parents are not LeftHand then RightHand");
    }
    Ok(())
}

fn validate_target(target: &SkyrimWolfRecordTarget) -> Result<(), SkyrimWolfRecordError> {
    if !target
        .runtime_root
        .eq_ignore_ascii_case(TARGET_RUNTIME_ROOT)
    {
        return Err(SkyrimWolfRecordError::InvalidTarget(format!(
            "runtime root must be {TARGET_RUNTIME_ROOT:?}, got {:?}",
            target.runtime_root
        )));
    }
    if target.target_plugin.eq_ignore_ascii_case(SKYRIM_MASTER)
        || target.target_plugin.eq_ignore_ascii_case("Fallout4.esm")
        || !has_plugin_extension(&target.target_plugin)
    {
        return Err(SkyrimWolfRecordError::InvalidTarget(format!(
            "target plugin {:?} is not a custom plugin",
            target.target_plugin
        )));
    }
    if target.resolved_source_name.is_empty()
        || target.resolved_source_name != target.resolved_source_name.trim()
        || target.resolved_source_name.chars().any(char::is_control)
    {
        return Err(SkyrimWolfRecordError::InvalidTarget(
            "display name must be non-empty, trimmed, and contain no controls".to_string(),
        ));
    }
    Ok(())
}

fn require_record(
    record: &Record,
    signature: &'static str,
    local: u32,
    editor_id: &str,
    interner: &StringInterner,
) -> Result<(), SkyrimWolfRecordError> {
    if record.sig.as_str() != signature
        || record.form_key.local != local
        || !interner
            .resolve(record.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(SKYRIM_MASTER))
        || record.eid.and_then(|eid| interner.resolve(eid)) != Some(editor_id)
        || count_fields(record, "EDID") != 1
        || !matches!(
            exact_field(record, "EDID"),
            Some(FieldValue::String(value)) if interner.resolve(*value) == Some(editor_id)
        )
    {
        return source_error(
            signature,
            format!("expected {editor_id} {local:06X}@{SKYRIM_MASTER}"),
        );
    }
    Ok(())
}

fn require_supported_fields(
    record: &Record,
    supported: &[&str],
    label: &'static str,
) -> Result<(), SkyrimWolfRecordError> {
    if let Some(field) = record
        .fields
        .iter()
        .find(|field| !supported.contains(&field.sig.as_str()))
    {
        return source_error(label, format!("unsupported field {}", field.sig.as_str()));
    }
    Ok(())
}

fn reject_fields(
    record: &Record,
    rejected: &[&str],
    label: &'static str,
) -> Result<(), SkyrimWolfRecordError> {
    if let Some(field) = record
        .fields
        .iter()
        .find(|field| rejected.contains(&field.sig.as_str()))
    {
        return source_error(label, format!("rejected field {}", field.sig.as_str()));
    }
    Ok(())
}

fn require_form_key(
    record: &Record,
    signature: &str,
    local: u32,
    label: &'static str,
) -> Result<(), SkyrimWolfRecordError> {
    let Some(FieldValue::FormKey(form_key)) = exact_field(record, signature) else {
        return source_error(label, format!("missing or repeated {signature}"));
    };
    if form_key.local != local || form_key.plugin != record.form_key.plugin {
        return source_error(
            label,
            format!("{signature} is not {local:06X}@{SKYRIM_MASTER}"),
        );
    }
    Ok(())
}

fn require_matching_strings(
    record: &Record,
    signature: &str,
    expected: &str,
    count: usize,
    label: &'static str,
    interner: &StringInterner,
) -> Result<(), SkyrimWolfRecordError> {
    let actual = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .filter(|field| {
            string_value(&field.value, interner).is_some_and(|value| path_eq(value, expected))
        })
        .count();
    if actual != count {
        return source_error(
            label,
            format!("{signature} matched {expected:?} {actual} times, expected {count}"),
        );
    }
    Ok(())
}

fn require_count(
    record: &Record,
    signature: &str,
    count: usize,
    label: &'static str,
) -> Result<(), SkyrimWolfRecordError> {
    let actual = count_fields(record, signature);
    if actual != count {
        return source_error(
            label,
            format!("{signature} count is {actual}, expected {count}"),
        );
    }
    Ok(())
}

fn require_bytes<'a>(
    record: &'a Record,
    signature: &str,
    label: &'static str,
) -> Result<&'a [u8], SkyrimWolfRecordError> {
    exact_field(record, signature)
        .and_then(bytes_value)
        .ok_or_else(|| invalid_source(label, format!("missing byte field {signature}")))
}

fn require_scalar_bool(
    record: &Record,
    signature: &str,
    label: &'static str,
) -> Result<bool, SkyrimWolfRecordError> {
    match exact_field(record, signature) {
        Some(FieldValue::Bool(value)) => Ok(*value),
        Some(FieldValue::Uint(0)) | Some(FieldValue::Int(0)) => Ok(false),
        Some(FieldValue::Uint(1)) | Some(FieldValue::Int(1)) => Ok(true),
        _ => source_error(label, format!("{signature} is not a scalar boolean")),
    }
}

fn exact_field<'a>(record: &'a Record, signature: &str) -> Option<&'a FieldValue> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature);
    let value = &fields.next()?.value;
    fields.next().is_none().then_some(value)
}

fn count_fields(record: &Record, signature: &str) -> usize {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .count()
}

fn string_value<'a>(value: &FieldValue, interner: &'a StringInterner) -> Option<&'a str> {
    match value {
        FieldValue::String(value) => interner.resolve(*value),
        _ => None,
    }
}

fn bytes_value(value: &FieldValue) -> Option<&[u8]> {
    match value {
        FieldValue::Bytes(bytes) => Some(bytes),
        _ => None,
    }
}

fn source_form_key(form_key: FormKey, local: u32, interner: &StringInterner) -> bool {
    form_key.local == local
        && interner
            .resolve(form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(SKYRIM_MASTER))
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_f32(bytes: &[u8], offset: usize) -> Option<f32> {
    Some(f32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn path_eq(left: &str, right: &str) -> bool {
    left.replace('/', "\\").eq_ignore_ascii_case(right)
}

fn has_plugin_extension(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [".esp", ".esm", ".esl"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn invalid_source(record: &'static str, reason: impl Into<String>) -> SkyrimWolfRecordError {
    SkyrimWolfRecordError::InvalidSource {
        record,
        reason: reason.into(),
    }
}

fn source_error<T>(
    record: &'static str,
    reason: impl Into<String>,
) -> Result<T, SkyrimWolfRecordError> {
    Err(invalid_source(record, reason))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use smallvec::SmallVec;

    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::source_rig::{
        BoneDecl, Capsule, ClipBinding, ClipDecl, CreatureManifest, EventDecl, EventUsage,
        GraphDeclarations, MvpGraphManifest, ScaffoldPaths, SkeletonDecl, TargetFormKey,
        emit_creature_record_closure,
    };

    fn fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(SKYRIM_MASTER),
        }
    }

    fn field(signature: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        }
    }

    fn identified_record(
        interner: &StringInterner,
        signature: &str,
        local: u32,
        editor_id: &str,
    ) -> Record {
        let mut record = Record::new(SigCode::from_str(signature).unwrap(), fk(interner, local));
        let editor_id = interner.intern(editor_id);
        record.eid = Some(editor_id);
        record
            .fields
            .push(field("EDID", FieldValue::String(editor_id)));
        record
    }

    fn form_field(signature: &str, interner: &StringInterner, local: u32) -> FieldEntry {
        field(signature, FieldValue::FormKey(fk(interner, local)))
    }

    fn string_field(signature: &str, interner: &StringInterner, value: &str) -> FieldEntry {
        field(signature, FieldValue::String(interner.intern(value)))
    }

    fn source_records(interner: &StringInterner) -> Vec<Record> {
        let mut npc = identified_record(interner, "NPC_", WOLF_NPC_LOCAL, "EncWolfDead");
        npc.flags = RecordFlags::from_bits_retain(0x000C_0000);
        let mut acbs = vec![0_u8; 24];
        acbs[8..10].copy_from_slice(&2_u16.to_le_bytes());
        npc.fields.extend([
            field("ACBS", FieldValue::Bytes(SmallVec::from_vec(acbs))),
            form_field("RNAM", interner, WOLF_RACE_LOCAL),
            form_field("WNAM", interner, WOLF_SKIN_LOCAL),
            form_field("ATKR", interner, WOLF_RACE_LOCAL),
            string_field("FULL", interner, "Wolf"),
        ]);

        let mut race = identified_record(interner, "RACE", WOLF_RACE_LOCAL, "WolfRace");
        let mut attack = vec![0_u8; 44];
        attack[0..4].copy_from_slice(&1.0_f32.to_le_bytes());
        attack[4..8].copy_from_slice(&0.5_f32.to_le_bytes());
        attack[20..24].copy_from_slice(&35.0_f32.to_le_bytes());
        race.fields.extend([
            form_field("WNAM", interner, WOLF_SKIN_LOCAL),
            string_field("ANAM", interner, SOURCE_SKELETON),
            string_field("ANAM", interner, SOURCE_SKELETON),
            field("ATKD", FieldValue::Bytes(SmallVec::from_vec(attack))),
            string_field("ATKE", interner, "attackStart_Attack1"),
            form_field("GNAM", interner, WOLF_BODY_PART_DATA_LOCAL),
            string_field("MODL", interner, SOURCE_PROJECT),
            string_field("MODL", interner, SOURCE_PROJECT),
            form_field("QNAM", interner, RIGHT_HAND_LOCAL),
            form_field("UNES", interner, RIGHT_HAND_LOCAL),
        ]);

        let mut skin = identified_record(interner, "ARMO", WOLF_SKIN_LOCAL, "SkinWolf");
        skin.flags = RecordFlags::from_bits_retain(4);
        skin.fields.extend([
            form_field("RNAM", interner, WOLF_RACE_LOCAL),
            form_field("MODL", interner, WOLF_ARMOR_ADDON_LOCAL),
        ]);

        let mut armor_addon =
            identified_record(interner, "ARMA", WOLF_ARMOR_ADDON_LOCAL, "NakedWolfAA");
        armor_addon.fields.extend([
            form_field("RNAM", interner, WOLF_RACE_LOCAL),
            string_field("MOD2", interner, SOURCE_BODY),
        ]);

        let mut body_part_data = identified_record(
            interner,
            "BPTD",
            WOLF_BODY_PART_DATA_LOCAL,
            "DogBodyPartData",
        );
        body_part_data.fields.extend([
            string_field("MODL", interner, SOURCE_BPTD_SKELETON),
            string_field("BPNN", interner, "Canine_Head"),
            field("BPND", FieldValue::Bytes(SmallVec::from_slice(&[0; 88]))),
            string_field("BPNN", interner, "Canine_Pelvis"),
            field("BPND", FieldValue::Bytes(SmallVec::from_slice(&[0; 88]))),
        ]);

        let mut unarmed = identified_record(interner, "WEAP", UNARMED_WEAPON_LOCAL, "Unarmed");
        let mut dnam = vec![0_u8; 100];
        dnam[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
        dnam[8..12].copy_from_slice(&1.0_f32.to_le_bytes());
        unarmed.fields.extend([
            form_field("ETYP", interner, BOTH_HANDS_LOCAL),
            field("DATA", FieldValue::Bytes(SmallVec::from_slice(&[0; 10]))),
            field("DNAM", FieldValue::Bytes(SmallVec::from_vec(dnam))),
        ]);

        let mut right = identified_record(interner, "EQUP", RIGHT_HAND_LOCAL, "RightHand");
        right.fields.push(field("DATA", FieldValue::Uint(0)));
        let mut left = identified_record(interner, "EQUP", LEFT_HAND_LOCAL, "LeftHand");
        left.fields.push(field("DATA", FieldValue::Uint(0)));
        let mut both = identified_record(interner, "EQUP", BOTH_HANDS_LOCAL, "BothHands");
        both.fields.extend([
            field(
                "PNAM",
                FieldValue::List(vec![
                    FieldValue::FormKey(fk(interner, LEFT_HAND_LOCAL)),
                    FieldValue::FormKey(fk(interner, RIGHT_HAND_LOCAL)),
                ]),
            ),
            field("DATA", FieldValue::Uint(1)),
        ]);

        vec![
            npc,
            race,
            skin,
            armor_addon,
            body_part_data,
            unarmed,
            right,
            left,
            both,
        ]
    }

    fn target() -> SkyrimWolfRecordTarget {
        let plugin = "B21_CreatureMVP.esp";
        let target_key = |local| TargetFormKey::new(local, plugin);
        SkyrimWolfRecordTarget {
            target_plugin: plugin.to_string(),
            form_keys: CreatureRecordFormKeys {
                race: target_key(0x800),
                npc: target_key(0x801),
                skin: target_key(0x802),
                armor_addon: target_key(0x803),
                body_part_data: target_key(0x804),
                unarmed_weapon: target_key(0x805),
            },
            runtime_root: TARGET_RUNTIME_ROOT.to_string(),
            resolved_source_name: "Wolf".to_string(),
        }
    }

    fn adapt<'a>(
        records: &'a [Record],
        interner: &StringInterner,
    ) -> Result<SkyrimWolfRecordProjection, SkyrimWolfRecordError> {
        adapt_skyrim_wolf_records(
            SkyrimWolfSourceRecords {
                npc: &records[0],
                race: &records[1],
                skin: &records[2],
                armor_addon: &records[3],
                body_part_data: &records[4],
                unarmed_weapon: &records[5],
                right_hand: &records[6],
                left_hand: &records[7],
                both_hands: &records[8],
            },
            target(),
            interner,
        )
    }

    fn rig() -> (CreatureManifest, MvpGraphManifest) {
        let skeleton = "Actors\\B21_SkyrimWolf\\CharacterAssets\\Skeleton.hkx";
        let clip = |name: &str, looping: bool| ClipDecl {
            name: name.to_string(),
            path: format!("Actors\\B21_SkyrimWolf\\Animations\\{name}.hkx"),
            binding: ClipBinding {
                skeleton_path: skeleton.to_string(),
                original_skeleton_name: "B21_SkyrimWolfSkeleton".to_string(),
                declared_transform_tracks: 1,
                transform_track_to_bone_indices: vec![0],
                declared_float_tracks: 0,
                float_track_to_float_slot_indices: Vec::new(),
            },
            looping,
        };
        let events = [
            ("Idle", EventUsage::Generic),
            ("startWalk", EventUsage::Generic),
            ("TurnLeft90", EventUsage::Generic),
            ("TurnRight90", EventUsage::Generic),
            ("meleeWolfAttack1", EventUsage::MeleeAttack),
        ]
        .into_iter()
        .map(|(name, usage)| EventDecl {
            name: name.to_string(),
            usage,
            flags: 0,
        })
        .collect::<Vec<_>>();
        (
            CreatureManifest {
                creature_name: "B21_SkyrimWolf".to_string(),
                visual_skeleton_nif: "Actors\\B21_SkyrimWolf\\CharacterAssets\\Skeleton.nif"
                    .to_string(),
                animation_skeleton: SkeletonDecl {
                    path: skeleton.to_string(),
                    runtime_name: "B21_SkyrimWolfSkeleton".to_string(),
                    bones: vec![BoneDecl {
                        name: "NPC Root [Root]".to_string(),
                        parent_index: None,
                    }],
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
                clips: vec![
                    clip("mt_idle_wolf", true),
                    clip("walkforward_wolf", true),
                    clip("turncannedl90_wolf", false),
                    clip("turncannedr90_wolf", false),
                    clip("attack1", false),
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
                    root_behavior:
                        "Actors\\B21_SkyrimWolf\\Behaviors\\B21_SkyrimWolfRootBehavior.hkx"
                            .to_string(),
                    core_behavior:
                        "Actors\\B21_SkyrimWolf\\Behaviors\\B21_SkyrimWolfCoreBehavior.hkx"
                            .to_string(),
                },
                root: GraphDeclarations {
                    events: events.clone(),
                    ..GraphDeclarations::default()
                },
                core: GraphDeclarations {
                    events,
                    ..GraphDeclarations::default()
                },
            },
            MvpGraphManifest {
                idle_clip: "mt_idle_wolf".to_string(),
                walk_forward_clip: "walkforward_wolf".to_string(),
                turn_left_90_clip: "turncannedl90_wolf".to_string(),
                turn_right_90_clip: "turncannedr90_wolf".to_string(),
                attack_1_clip: "attack1".to_string(),
                melee_event: "meleeWolfAttack1".to_string(),
            },
        )
    }

    #[test]
    fn exact_wolf_source_adapter_emits_six_target_owned_records() {
        let interner = StringInterner::new();
        let records = source_records(&interner);
        let projection = adapt(&records, &interner).unwrap();
        let (rig, graph) = rig();
        let closure = emit_creature_record_closure(
            &rig,
            &graph,
            &projection.manifest,
            &projection.profile,
            &interner,
        )
        .unwrap();

        assert_eq!(closure.records.len(), 6);
        assert_eq!(projection.profile.npc_level, 2);
        assert_eq!(projection.profile.attack_damage_multiplier, 1.0);
        assert_eq!(projection.profile.attack_chance, 0.5);
        assert_eq!(projection.profile.attack_strike_angle, 35.0);
        assert_eq!(projection.profile.unarmed_reach, 1.0);
        assert_eq!(
            projection.manifest.body_nif,
            "Actors\\B21_SkyrimWolf\\CharacterAssets\\B21_SkyrimWolf.nif"
        );
        for record in &closure.records {
            assert_eq!(
                interner.resolve(record.form_key.plugin),
                Some("B21_CreatureMVP.esp")
            );
            for field in &record.fields {
                assert_no_source_form_keys(&field.value, &interner);
            }
        }
        assert!(record_has_string(
            closure.record("RACE").unwrap(),
            "ATKE",
            "meleeWolfAttack1",
            &interner
        ));
        assert!(record_has_string(
            closure.record("RACE").unwrap(),
            "ANAM",
            "Actors\\B21_SkyrimWolf\\CharacterAssets\\Skeleton.nif",
            &interner
        ));
        assert!(record_has_string(
            closure.record("RACE").unwrap(),
            "MODL",
            "Actors\\B21_SkyrimWolf\\B21_SkyrimWolfProject.hkx",
            &interner
        ));
        assert!(record_has_string(
            closure.record("ARMA").unwrap(),
            "MOD2",
            "Actors\\B21_SkyrimWolf\\CharacterAssets\\B21_SkyrimWolf.nif",
            &interner
        ));
        assert_eq!(
            record_struct_u8(
                closure.record("BPTD").unwrap(),
                "BPND",
                "geometry_segment_index",
                &interner
            ),
            Some(32)
        );
    }

    #[test]
    fn canonical_runtime_contract_preserves_the_fifty_bone_source_rig() {
        let (rig, graph, motion) = canonical_skyrim_wolf_runtime_contract();
        let errors = rig.validate_mvp_motion(&graph, &motion).unwrap_err();
        assert_eq!(errors.0.len(), 1);
        assert_eq!(errors.0[0].code, "ragdoll_deferred");
        assert_eq!(rig.animation_skeleton.bones.len(), 50);
        assert_eq!(rig.animation_skeleton.bones[0].name, "NPC Root [Root]");
        assert_eq!(rig.animation_skeleton.bones[1].name, "Canine_COM");
        assert_eq!(rig.animation_skeleton.bones[49].name, "Canine_Dog_RBrow");
        assert!(rig.clips.iter().all(|clip| {
            clip.binding.declared_transform_tracks == 50
                && clip.binding.transform_track_to_bone_indices == (0..50).collect::<Vec<_>>()
        }));
        assert!(rig.clips[0].looping);
        assert!(rig.clips[1].looping);
        assert!(rig.clips[2..].iter().all(|clip| !clip.looping));
    }

    #[test]
    fn adapter_rejects_script_template_enchantment_and_unknown_drift() {
        let interner = StringInterner::new();

        for signature in ["VMAD", "TPLT"] {
            let mut records = source_records(&interner);
            records[0]
                .fields
                .push(field(signature, FieldValue::Bytes(SmallVec::new())));
            assert!(adapt(&records, &interner).is_err());
        }

        let mut records = source_records(&interner);
        records[5]
            .fields
            .push(form_field("EITM", &interner, 0x000800));
        assert!(adapt(&records, &interner).is_err());

        let mut records = source_records(&interner);
        records[1]
            .fields
            .push(field("ZZZZ", FieldValue::Bytes(SmallVec::new())));
        assert!(adapt(&records, &interner).is_err());
    }

    #[test]
    fn optional_real_skyrim_wolf_corpus_emits_normalized_closure() {
        let Some(path) = skyrim_corpus_path() else {
            return;
        };
        if !path.is_file() {
            return;
        }
        let handle = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            path.to_str().unwrap(),
            Some("skyrimse"),
            None,
            None,
            true,
        )
        .unwrap();
        let plugin_name = crate::source_read::plugin_name_for_handle(handle).unwrap();
        assert_eq!(plugin_name, SKYRIM_MASTER);
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("skyrimse").unwrap();
        let records = [
            WOLF_NPC_LOCAL,
            WOLF_RACE_LOCAL,
            WOLF_SKIN_LOCAL,
            WOLF_ARMOR_ADDON_LOCAL,
            WOLF_BODY_PART_DATA_LOCAL,
            UNARMED_WEAPON_LOCAL,
            RIGHT_HAND_LOCAL,
            LEFT_HAND_LOCAL,
            BOTH_HANDS_LOCAL,
        ]
        .into_iter()
        .map(|local| {
            crate::source_read::read_record(
                handle,
                &format!("{local:06X}@{plugin_name}"),
                &schema,
                &interner,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();

        let projection = adapt(&records, &interner).unwrap();
        let (rig, graph) = rig();
        let closure = emit_creature_record_closure(
            &rig,
            &graph,
            &projection.manifest,
            &projection.profile,
            &interner,
        )
        .unwrap();
        assert_eq!(closure.records.len(), 6);
        for record in &closure.records {
            assert_eq!(
                interner.resolve(record.form_key.plugin),
                Some("B21_CreatureMVP.esp")
            );
            for field in &record.fields {
                assert_no_source_form_keys(&field.value, &interner);
            }
        }
    }

    fn skyrim_corpus_path() -> Option<PathBuf> {
        std::env::var_os("SKYRIMSE_WOLF_CORPUS_PLUGIN")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("SKYRIMSE_DIR")
                    .map(PathBuf::from)
                    .map(|path| path.join("Data").join("Skyrim.esm"))
            })
    }

    fn assert_no_source_form_keys(value: &FieldValue, interner: &StringInterner) {
        match value {
            FieldValue::FormKey(form_key) => assert_ne!(
                interner.resolve(form_key.plugin),
                Some(SKYRIM_MASTER),
                "source FormKey leaked into target closure"
            ),
            FieldValue::List(values) => {
                for value in values {
                    assert_no_source_form_keys(value, interner);
                }
            }
            FieldValue::Struct(fields) => {
                for (_, value) in fields {
                    assert_no_source_form_keys(value, interner);
                }
            }
            _ => {}
        }
    }

    fn record_has_string(
        record: &Record,
        signature: &str,
        expected: &str,
        interner: &StringInterner,
    ) -> bool {
        record.fields.iter().any(|field| {
            field.sig.as_str() == signature
                && string_value(&field.value, interner) == Some(expected)
        })
    }

    fn record_struct_u8(
        record: &Record,
        signature: &str,
        member: &str,
        interner: &StringInterner,
    ) -> Option<u8> {
        let FieldValue::Struct(fields) = exact_field(record, signature)? else {
            return None;
        };
        fields.iter().find_map(|(key, value)| {
            (interner.resolve(*key) == Some(member)).then(|| match value {
                FieldValue::Uint(value) => u8::try_from(*value).ok(),
                FieldValue::Bytes(bytes) if bytes.len() == 1 => Some(bytes[0]),
                _ => None,
            })?
        })
    }
}
