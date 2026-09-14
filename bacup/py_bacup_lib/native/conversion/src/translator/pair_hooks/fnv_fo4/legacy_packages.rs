use smallvec::SmallVec;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fnv_pack::{
    LegacyPackClassificationStatus, LegacyPackInventory, LegacyPackLocationType,
    LegacyPackScriptEvent, LegacyPackSourceFamily, LegacyPackTargetType, LegacyPackType,
    LegacyPackUnionKind, LegacyPackUnionPayload, classify_legacy_pack,
};

use super::legacy_ammo::source_form_key_for_raw_form_id;

const SOURCE_SHAPE_SCHEMA: &str = "fnv-fo3-semantic-pack-families-v2";
const FO4_SHARED_GENERAL_FLAGS: u32 = 0x0026_06C5;
const FO4_SHARED_INTERRUPT_FLAGS: u16 = 0x00F7;
const FO4_TRAVEL_TEMPLATE_LOCAL: u32 = 0x002C_B0;
const FO4_PATROL_TEMPLATE_LOCAL: u32 = 0x002C_E0;
const FO4_FOLLOW_TEMPLATE_LOCAL: u32 = 0x023D_36;
const FO4_ESCORT_TEMPLATE_LOCAL: u32 = 0x1E43_AF;
const FO4_SANDBOX_TEMPLATE_LOCAL: u32 = 0x002C_B1;
const FO4_FORCE_GREET_TEMPLATE_LOCAL: u32 = 0x017B_AB;
const FO4_ACTIVATE_TEMPLATE_LOCAL: u32 = 0x0797_6C;
const FO4_USE_IDLE_MARKER_TEMPLATE_LOCAL: u32 = 0x004D_F6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GenericLegacyPackageFamily {
    Travel,
    Patrol,
    DialogueForceGreet,
    Follow,
    Escort,
    Sandbox,
    Activate,
    UseItem,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GenericLegacyPackagePolicy {
    Travel,
    Patrol,
    DialogueForceGreet,
    Follow,
    Escort,
    Sandbox,
    Activate,
    UseItem,
    FallbackTravel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GenericLegacyPackageShapeProof {
    pub schema: &'static str,
    pub family: GenericLegacyPackageFamily,
    pub policy: GenericLegacyPackagePolicy,
    pub template_local: u32,
    pub general_flags: u32,
    pub interrupt_flags: u16,
    pub schedule_month: i8,
    pub schedule_day_of_week: i8,
    pub schedule_date: i8,
    pub schedule_hour: i8,
    pub schedule_duration_hours: i32,
    pub location_type: u32,
    pub object_type: Option<u32>,
    pub radius: i32,
    pub repeatable: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GenericLegacyPackageSupport {
    Ready {
        proof: GenericLegacyPackageShapeProof,
    },
    Fallback {
        proof: GenericLegacyPackageShapeProof,
        warning_codes: Vec<String>,
    },
}

pub(crate) fn classify_generic_legacy_package(
    record: &Record,
    source: LegacyPackSourceFamily,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> GenericLegacyPackageSupport {
    let mut normalized = record.clone();
    if let Err(reason) = decode_legacy_package_unions(
        &mut normalized,
        source_plugin_name,
        source_master_names,
        interner,
    ) {
        return GenericLegacyPackageSupport::Fallback {
            proof: default_fallback_travel_proof(),
            warning_codes: vec![reason],
        };
    }
    let report = classify_legacy_pack(&normalized, source, interner);
    if report.status != LegacyPackClassificationStatus::Accepted {
        return GenericLegacyPackageSupport::Fallback {
            proof: default_fallback_travel_proof(),
            warning_codes: vec!["legacy_pack_classification_rejected".to_string()],
        };
    }
    let Some(inventory) = report.inventory.as_ref() else {
        return GenericLegacyPackageSupport::Fallback {
            proof: default_fallback_travel_proof(),
            warning_codes: vec!["legacy_pack_classification_rejected".to_string()],
        };
    };
    let mut reasons = common_blockers(inventory);
    let proof = match inventory.package_type {
        LegacyPackType::Travel => classify_travel(inventory, &mut reasons),
        LegacyPackType::Patrol => classify_patrol(inventory, &mut reasons),
        LegacyPackType::Dialogue => classify_dialogue_force_greet(
            inventory,
            &normalized,
            source_plugin_name,
            source_master_names,
            interner,
            &mut reasons,
        ),
        LegacyPackType::Follow => classify_follow(inventory, &normalized, &mut reasons),
        LegacyPackType::Escort => classify_escort(inventory, &normalized, &mut reasons),
        LegacyPackType::Sandbox => classify_sandbox(inventory, &mut reasons),
        LegacyPackType::UseItemAt => classify_activate_or_use_item(inventory, &mut reasons),
        _ => {
            reasons.push("legacy_pack_family_not_semantically_lowered".to_string());
            None
        }
    };
    reasons.sort();
    reasons.dedup();
    match (proof, reasons.is_empty()) {
        (Some(proof), true) => GenericLegacyPackageSupport::Ready { proof },
        _ => GenericLegacyPackageSupport::Fallback {
            proof: fallback_travel_proof(inventory),
            warning_codes: fallback_warning_codes(inventory, reasons),
        },
    }
}

pub(crate) fn lower_generic_legacy_package(
    record: &mut Record,
    source: LegacyPackSourceFamily,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<Option<GenericLegacyPackageShapeProof>, String> {
    // A fallback must still be based on a decodable legacy package.  The
    // classifier deliberately reports malformed unions as fallback candidates
    // for diagnostic purposes; accepting those would turn corrupt topology
    // into a seemingly valid target package.
    let mut normalized = record.clone();
    decode_legacy_package_unions(
        &mut normalized,
        source_plugin_name,
        source_master_names,
        interner,
    )?;
    let support = classify_generic_legacy_package(
        record,
        source,
        source_plugin_name,
        source_master_names,
        interner,
    );
    let (proof, fallback_warnings) = match support {
        GenericLegacyPackageSupport::Ready { proof } => (proof, Vec::new()),
        GenericLegacyPackageSupport::Fallback {
            proof,
            warning_codes,
        } => (proof, warning_codes),
    };
    let mut output = target_record(record);
    push_common_header(&mut output, &proof);
    match proof.policy {
        GenericLegacyPackagePolicy::FallbackTravel => {
            lower_fallback_travel(&mut output, interner);
            append_unique_warnings(&mut output, &fallback_warnings, interner);
        }
        policy => {
            let report = classify_legacy_pack(&normalized, source, interner);
            let inventory = report
                .inventory
                .as_ref()
                .ok_or_else(|| "legacy_pack_classification_changed_during_lowering".to_string())?;
            match policy {
                GenericLegacyPackagePolicy::Travel => {
                    lower_travel(&mut output, inventory, interner)?
                }
                GenericLegacyPackagePolicy::Patrol => {
                    lower_patrol(&mut output, inventory, interner)?
                }
                GenericLegacyPackagePolicy::DialogueForceGreet => lower_dialogue_force_greet(
                    &mut output,
                    inventory,
                    &normalized,
                    source_plugin_name,
                    source_master_names,
                    interner,
                )?,
                GenericLegacyPackagePolicy::Follow => {
                    lower_follow(&mut output, inventory, &normalized, interner)?
                }
                GenericLegacyPackagePolicy::Escort => {
                    lower_escort(&mut output, inventory, &normalized, interner)?
                }
                GenericLegacyPackagePolicy::Sandbox => {
                    lower_sandbox(&mut output, inventory, interner)?
                }
                GenericLegacyPackagePolicy::Activate => {
                    lower_activate(&mut output, inventory, interner)?
                }
                GenericLegacyPackagePolicy::UseItem => {
                    lower_use_item(&mut output, inventory, interner)?
                }
                GenericLegacyPackagePolicy::FallbackTravel => unreachable!(),
            }
        }
    }
    *record = output;
    Ok(Some(proof))
}

pub(crate) fn legacy_package_fallback_warnings(
    record: &Record,
    source: LegacyPackSourceFamily,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Vec<String> {
    match classify_generic_legacy_package(
        record,
        source,
        source_plugin_name,
        source_master_names,
        interner,
    ) {
        GenericLegacyPackageSupport::Fallback { warning_codes, .. } => warning_codes,
        GenericLegacyPackageSupport::Ready { .. } => Vec::new(),
    }
}

fn common_blockers(inventory: &LegacyPackInventory) -> Vec<String> {
    let mut reasons = Vec::new();
    if inventory.pkdt.general_flags & !FO4_SHARED_GENERAL_FLAGS != 0 {
        reasons.push("legacy_pack_general_flags_not_fo4_shared".to_string());
    }
    if inventory.pkdt.fallout_behavior_flags & !FO4_SHARED_INTERRUPT_FLAGS != 0 {
        reasons.push("legacy_pack_interrupt_flags_not_fo4_shared".to_string());
    }
    if inventory.pkdt.type_specific_flags != 0 {
        reasons.push("legacy_pack_type_specific_flags_unverified".to_string());
    }
    if inventory.pkdt.unused_bytes_nonzero {
        reasons.push("legacy_pack_unused_pkdt_bytes_nonzero".to_string());
    }
    if !valid_schedule(inventory) {
        reasons.push("legacy_pack_schedule_not_target_valid".to_string());
    }
    if !inventory.conditions.is_empty() {
        reasons.push("legacy_pack_conditions_require_semantic_lowering".to_string());
    }
    if !empty_event_semantics(inventory) {
        reasons.push("legacy_pack_event_behavior_requires_port".to_string());
    }
    reasons
}

fn append_unique_warnings(
    record: &mut Record,
    warning_codes: &[String],
    interner: &StringInterner,
) {
    for warning_code in warning_codes {
        let warning = interner.intern(warning_code);
        if !record.warnings.contains(&warning) {
            record.warnings.push(warning);
        }
    }
}

fn classify_travel(
    inventory: &LegacyPackInventory,
    reasons: &mut Vec<String>,
) -> Option<GenericLegacyPackageShapeProof> {
    let [location] = inventory.unions.as_slice() else {
        reasons.push("legacy_pack_travel_requires_one_location".to_string());
        return None;
    };
    if location.sig != "PLDT" || !inventory.type_specific_subrecords.is_empty() {
        reasons.push("legacy_pack_travel_shape_unverified".to_string());
    }
    if location.radius_or_distance < 0 || location.trailing_unknown_nonzero {
        reasons.push("legacy_pack_travel_location_not_target_valid".to_string());
    }
    let LegacyPackUnionKind::Location(location_type) = location.union_kind else {
        reasons.push("legacy_pack_travel_requires_location_union".to_string());
        return None;
    };
    if !location_payload_is_exact(location_type, &location.payload) {
        reasons.push("legacy_pack_travel_location_payload_unverified".to_string());
    }
    Some(GenericLegacyPackageShapeProof {
        schema: SOURCE_SHAPE_SCHEMA,
        family: GenericLegacyPackageFamily::Travel,
        policy: GenericLegacyPackagePolicy::Travel,
        template_local: FO4_TRAVEL_TEMPLATE_LOCAL,
        general_flags: inventory.pkdt.general_flags,
        interrupt_flags: inventory.pkdt.fallout_behavior_flags,
        schedule_month: inventory.schedule.month,
        schedule_day_of_week: inventory.schedule.day_of_week,
        schedule_date: inventory.schedule.date,
        schedule_hour: inventory.schedule.hour,
        schedule_duration_hours: inventory.schedule.duration_hours,
        location_type: location_type_code(location_type),
        object_type: match &location.payload {
            LegacyPackUnionPayload::ObjectType { value } => Some(*value),
            _ => None,
        },
        radius: location.radius_or_distance,
        repeatable: None,
    })
}

fn classify_patrol(
    inventory: &LegacyPackInventory,
    reasons: &mut Vec<String>,
) -> Option<GenericLegacyPackageShapeProof> {
    let [location] = inventory.unions.as_slice() else {
        reasons.push("legacy_pack_patrol_requires_one_path_start".to_string());
        return None;
    };
    let exact_path = location.sig == "PLDT"
        && location.union_kind
            == LegacyPackUnionKind::Location(LegacyPackLocationType::NearReference)
        && matches!(
            location.payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: Some(_)
            }
        )
        && location.radius_or_distance >= 0
        && !location.trailing_unknown_nonzero;
    if !exact_path {
        reasons.push("legacy_pack_patrol_path_start_unverified".to_string());
    }
    let exact_type_specific = inventory.type_specific_subrecords.len() == 1
        && inventory.type_specific_subrecords[0].sig == "PKPT"
        && inventory.type_specific_subrecords[0].count == 1
        && inventory.type_specific_subrecords[0].observed_sizes == [2];
    let Some(patrol) = inventory.patrol_data.as_ref() else {
        reasons.push("legacy_pack_patrol_data_missing".to_string());
        return None;
    };
    if !exact_type_specific || patrol.unused_byte_nonzero {
        reasons.push("legacy_pack_patrol_data_unverified".to_string());
    }
    Some(GenericLegacyPackageShapeProof {
        schema: SOURCE_SHAPE_SCHEMA,
        family: GenericLegacyPackageFamily::Patrol,
        policy: GenericLegacyPackagePolicy::Patrol,
        template_local: FO4_PATROL_TEMPLATE_LOCAL,
        general_flags: inventory.pkdt.general_flags,
        interrupt_flags: inventory.pkdt.fallout_behavior_flags,
        schedule_month: inventory.schedule.month,
        schedule_day_of_week: inventory.schedule.day_of_week,
        schedule_date: inventory.schedule.date,
        schedule_hour: inventory.schedule.hour,
        schedule_duration_hours: inventory.schedule.duration_hours,
        location_type: 0,
        object_type: None,
        radius: location.radius_or_distance,
        repeatable: Some(patrol.repeatable),
    })
}

fn classify_dialogue_force_greet(
    inventory: &LegacyPackInventory,
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
    reasons: &mut Vec<String>,
) -> Option<GenericLegacyPackageShapeProof> {
    let [target] = inventory.unions.as_slice() else {
        reasons.push("legacy_pack_force_greet_requires_one_target".to_string());
        return None;
    };
    if target.sig != "PTDT"
        || target.union_kind != LegacyPackUnionKind::Target(LegacyPackTargetType::SpecificReference)
        || !is_player_reference(&target.payload)
        || target.radius_or_distance < 0
        || target.trailing_unknown_nonzero
    {
        reasons.push("legacy_pack_force_greet_target_not_exact_player_reference".to_string());
    }
    if !has_exact_type_specific(inventory, "PKDD", 24) {
        reasons.push("legacy_pack_force_greet_requires_one_pkdd_24".to_string());
        return None;
    }
    let Some(pkdd) = unique_bytes(record, b"PKDD") else {
        reasons.push("legacy_pack_force_greet_pkdd_not_bytes".to_string());
        return None;
    };
    if pkdd.len() != 24
        || f32::from_le_bytes(pkdd[0..4].try_into().expect("PKDD length")) != 0.0
        || read_u32(pkdd, 8) != 0
        || pkdd[12..16] != [0; 4]
        || read_u32(pkdd, 16) != 1
        || pkdd[20..24] != [0; 4]
    {
        reasons.push("legacy_pack_force_greet_pkdd_semantics_unverified".to_string());
    }
    let raw_topic = read_u32(pkdd, 4);
    if raw_topic != 0
        && source_form_key_for_raw_form_id(
            raw_topic,
            source_plugin_name,
            source_master_names,
            interner,
        )
        .is_none()
    {
        reasons.push("legacy_pack_force_greet_topic_unresolved".to_string());
    }
    Some(family_proof(
        inventory,
        GenericLegacyPackageFamily::DialogueForceGreet,
        GenericLegacyPackagePolicy::DialogueForceGreet,
        FO4_FORCE_GREET_TEMPLATE_LOCAL,
        0,
        target.radius_or_distance,
    ))
}

fn classify_follow(
    inventory: &LegacyPackInventory,
    record: &Record,
    reasons: &mut Vec<String>,
) -> Option<GenericLegacyPackageShapeProof> {
    let [target] = inventory.unions.as_slice() else {
        reasons.push("legacy_pack_follow_requires_one_target".to_string());
        return None;
    };
    if target.sig != "PTDT"
        || target.union_kind != LegacyPackUnionKind::Target(LegacyPackTargetType::SpecificReference)
        || !resolved_reference(&target.payload)
        || target.radius_or_distance < 0
        || target.trailing_unknown_nonzero
    {
        reasons.push("legacy_pack_follow_target_shape_unverified".to_string());
    }
    if !has_exact_type_specific(inventory, "PKFD", 4) {
        reasons.push("legacy_pack_follow_requires_one_pkfd_4".to_string());
        return None;
    }
    let Some(start_radius) = unique_f32(record, b"PKFD") else {
        reasons.push("legacy_pack_follow_pkfd_not_finite".to_string());
        return None;
    };
    if start_radius < 0.0 || start_radius > target.radius_or_distance as f32 {
        reasons.push("legacy_pack_follow_radius_order_invalid".to_string());
    }
    Some(family_proof(
        inventory,
        GenericLegacyPackageFamily::Follow,
        GenericLegacyPackagePolicy::Follow,
        FO4_FOLLOW_TEMPLATE_LOCAL,
        0,
        target.radius_or_distance,
    ))
}

fn classify_escort(
    inventory: &LegacyPackInventory,
    record: &Record,
    reasons: &mut Vec<String>,
) -> Option<GenericLegacyPackageShapeProof> {
    let Some(location) = unique_union(inventory, "PLDT") else {
        reasons.push("legacy_pack_escort_requires_one_destination".to_string());
        return None;
    };
    let Some(target) = unique_union(inventory, "PTDT") else {
        reasons.push("legacy_pack_escort_requires_one_target".to_string());
        return None;
    };
    if inventory.unions.len() != 2
        || !matches!(location.union_kind, LegacyPackUnionKind::Location(_))
        || !location_payload_is_exact(
            match location.union_kind {
                LegacyPackUnionKind::Location(kind) => kind,
                _ => unreachable!(),
            },
            &location.payload,
        )
        || location.radius_or_distance < 0
        || location.trailing_unknown_nonzero
    {
        reasons.push("legacy_pack_escort_destination_shape_unverified".to_string());
    }
    if target.union_kind != LegacyPackUnionKind::Target(LegacyPackTargetType::SpecificReference)
        || !is_player_reference(&target.payload)
        || target.trailing_unknown_nonzero
    {
        reasons.push("legacy_pack_escort_target_not_exact_player_reference".to_string());
    }
    if !has_exact_type_specific(inventory, "PKE2", 4) {
        reasons.push("legacy_pack_escort_requires_one_pke2_4".to_string());
        return None;
    }
    let Some(escort_distance) = unique_u32(record, b"PKE2") else {
        reasons.push("legacy_pack_escort_distance_not_u32".to_string());
        return None;
    };
    if escort_distance == 0 || escort_distance > i32::MAX as u32 {
        reasons.push("legacy_pack_escort_distance_not_target_valid".to_string());
    }
    let location_type = match location.union_kind {
        LegacyPackUnionKind::Location(kind) => location_type_code(kind),
        _ => 0,
    };
    Some(family_proof(
        inventory,
        GenericLegacyPackageFamily::Escort,
        GenericLegacyPackagePolicy::Escort,
        FO4_ESCORT_TEMPLATE_LOCAL,
        location_type,
        location.radius_or_distance,
    ))
}

fn classify_sandbox(
    inventory: &LegacyPackInventory,
    reasons: &mut Vec<String>,
) -> Option<GenericLegacyPackageShapeProof> {
    let [location] = inventory.unions.as_slice() else {
        reasons.push("legacy_pack_sandbox_requires_one_location".to_string());
        return None;
    };
    let LegacyPackUnionKind::Location(location_type) = location.union_kind else {
        reasons.push("legacy_pack_sandbox_requires_location_union".to_string());
        return None;
    };
    if location.sig != "PLDT"
        || !location_payload_is_exact(location_type, &location.payload)
        || location.radius_or_distance < 0
        || location.trailing_unknown_nonzero
        || !inventory.type_specific_subrecords.is_empty()
    {
        reasons.push("legacy_pack_sandbox_shape_unverified".to_string());
    }
    Some(family_proof(
        inventory,
        GenericLegacyPackageFamily::Sandbox,
        GenericLegacyPackagePolicy::Sandbox,
        FO4_SANDBOX_TEMPLATE_LOCAL,
        location_type_code(location_type),
        location.radius_or_distance,
    ))
}

fn classify_activate_or_use_item(
    inventory: &LegacyPackInventory,
    reasons: &mut Vec<String>,
) -> Option<GenericLegacyPackageShapeProof> {
    let [target] = inventory.unions.as_slice() else {
        reasons.push("legacy_pack_activate_use_item_requires_one_target".to_string());
        return None;
    };
    if target.sig != "PTDT"
        || target.union_kind != LegacyPackUnionKind::Target(LegacyPackTargetType::SpecificReference)
        || !resolved_reference(&target.payload)
        || target.radius_or_distance <= 0
        || target.trailing_unknown_nonzero
    {
        reasons.push("legacy_pack_activate_use_item_target_shape_unverified".to_string());
    }
    let use_item = has_exact_type_specific(inventory, "PUID", 0);
    let exact_type_specific = if use_item {
        inventory.type_specific_subrecords.len() == 1
    } else {
        inventory.type_specific_subrecords.is_empty()
    };
    if !exact_type_specific {
        reasons.push("legacy_pack_activate_use_item_type_specific_shape_unverified".to_string());
    }
    if use_item && target.radius_or_distance != 1 {
        reasons.push("legacy_pack_use_item_requires_single_activation".to_string());
    }
    Some(family_proof(
        inventory,
        if use_item {
            GenericLegacyPackageFamily::UseItem
        } else {
            GenericLegacyPackageFamily::Activate
        },
        if use_item {
            GenericLegacyPackagePolicy::UseItem
        } else {
            GenericLegacyPackagePolicy::Activate
        },
        if use_item {
            FO4_USE_IDLE_MARKER_TEMPLATE_LOCAL
        } else {
            FO4_ACTIVATE_TEMPLATE_LOCAL
        },
        0,
        target.radius_or_distance,
    ))
}

fn family_proof(
    inventory: &LegacyPackInventory,
    family: GenericLegacyPackageFamily,
    policy: GenericLegacyPackagePolicy,
    template_local: u32,
    location_type: u32,
    radius: i32,
) -> GenericLegacyPackageShapeProof {
    GenericLegacyPackageShapeProof {
        schema: SOURCE_SHAPE_SCHEMA,
        family,
        policy,
        template_local,
        general_flags: inventory.pkdt.general_flags,
        interrupt_flags: inventory.pkdt.fallout_behavior_flags,
        schedule_month: inventory.schedule.month,
        schedule_day_of_week: inventory.schedule.day_of_week,
        schedule_date: inventory.schedule.date,
        schedule_hour: inventory.schedule.hour,
        schedule_duration_hours: inventory.schedule.duration_hours,
        location_type,
        object_type: None,
        radius,
        repeatable: None,
    }
}

fn semantic_family(package_type: LegacyPackType) -> GenericLegacyPackageFamily {
    match package_type {
        LegacyPackType::Travel => GenericLegacyPackageFamily::Travel,
        LegacyPackType::Patrol => GenericLegacyPackageFamily::Patrol,
        LegacyPackType::Dialogue => GenericLegacyPackageFamily::DialogueForceGreet,
        LegacyPackType::Follow => GenericLegacyPackageFamily::Follow,
        LegacyPackType::Escort => GenericLegacyPackageFamily::Escort,
        LegacyPackType::Sandbox => GenericLegacyPackageFamily::Sandbox,
        LegacyPackType::UseItemAt => GenericLegacyPackageFamily::Activate,
        _ => GenericLegacyPackageFamily::Unsupported,
    }
}

fn fallback_travel_proof(inventory: &LegacyPackInventory) -> GenericLegacyPackageShapeProof {
    GenericLegacyPackageShapeProof {
        schema: SOURCE_SHAPE_SCHEMA,
        family: semantic_family(inventory.package_type),
        policy: GenericLegacyPackagePolicy::FallbackTravel,
        template_local: 0,
        general_flags: inventory.pkdt.general_flags & FO4_SHARED_GENERAL_FLAGS,
        interrupt_flags: inventory.pkdt.fallout_behavior_flags & FO4_SHARED_INTERRUPT_FLAGS,
        schedule_month: normalized_schedule_value(inventory.schedule.month, -1, 11, -1),
        schedule_day_of_week: normalized_schedule_value(inventory.schedule.day_of_week, -1, 10, -1),
        schedule_date: normalized_schedule_value(inventory.schedule.date, 0, 31, 0),
        schedule_hour: normalized_schedule_value(inventory.schedule.hour, -1, 23, -1),
        schedule_duration_hours: inventory.schedule.duration_hours.max(0),
        location_type: 2,
        object_type: None,
        radius: 0,
        repeatable: None,
    }
}

fn fallback_warning_codes(
    inventory: &LegacyPackInventory,
    mut warnings: Vec<String>,
) -> Vec<String> {
    warnings.push(format!(
        "legacy_pack_type_{}_uses_current_location_travel_fallback",
        inventory.package_type_code
    ));
    if !inventory.unions.is_empty() {
        warnings.push("legacy_pack_location_or_target_input_omitted".to_string());
    }
    if inventory.unions.iter().any(|union| {
        matches!(
            union.payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: None,
            }
        )
    }) {
        warnings.push("legacy_pack_unresolved_location_or_target_omitted".to_string());
    }
    if !inventory.conditions.is_empty() {
        warnings.push("legacy_pack_conditions_omitted".to_string());
    }
    if !inventory.scripts.is_empty() {
        warnings.push("legacy_pack_event_scripts_omitted".to_string());
    }
    if !valid_schedule(inventory) {
        warnings.push("legacy_pack_invalid_schedule_normalized".to_string());
    }
    warnings.sort();
    warnings.dedup();
    warnings
}

fn normalized_schedule_value(value: i8, minimum: i8, maximum: i8, default: i8) -> i8 {
    (minimum..=maximum)
        .contains(&value)
        .then_some(value)
        .unwrap_or(default)
}

fn valid_schedule(inventory: &LegacyPackInventory) -> bool {
    let schedule = &inventory.schedule;
    (-1..=11).contains(&schedule.month)
        && (-1..=10).contains(&schedule.day_of_week)
        && (0..=31).contains(&schedule.date)
        && (-1..=23).contains(&schedule.hour)
        && schedule.duration_hours >= 0
}

fn empty_event_semantics(inventory: &LegacyPackInventory) -> bool {
    if inventory.scripts.is_empty() {
        return true;
    }
    inventory.scripts.len() == 3
        && inventory.scripts.iter().map(|script| script.event).eq([
            LegacyPackScriptEvent::OnBegin,
            LegacyPackScriptEvent::OnEnd,
            LegacyPackScriptEvent::OnChange,
        ])
        && inventory.scripts.iter().all(|script| {
            !script.requires_port
                && script.compiled_size_matches
                && script.reference_count_matches
                && script.variable_count_matches
        })
}

fn location_payload_is_exact(
    location_type: LegacyPackLocationType,
    payload: &LegacyPackUnionPayload,
) -> bool {
    match location_type {
        LegacyPackLocationType::NearReference
        | LegacyPackLocationType::InCell
        | LegacyPackLocationType::ObjectId => matches!(
            payload,
            LegacyPackUnionPayload::Reference {
                present: true,
                source_form_key: Some(_)
            }
        ),
        LegacyPackLocationType::ObjectType => {
            matches!(payload, LegacyPackUnionPayload::ObjectType { .. })
        }
        LegacyPackLocationType::NearCurrentLocation
        | LegacyPackLocationType::NearEditorLocation
        | LegacyPackLocationType::NearLinkedReference
        | LegacyPackLocationType::AtPackageLocation => {
            matches!(payload, LegacyPackUnionPayload::Implicit)
        }
    }
}

fn decode_legacy_package_unions(
    record: &mut Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<(), String> {
    for field in &mut record.fields {
        let (expected_len, payload_name, is_location) = match &field.sig.0 {
            b"PLDT" | b"PLD2" => (12, "location", true),
            b"PTDT" | b"PTD2" => (16, "target", false),
            _ => continue,
        };
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        if bytes.len() != expected_len {
            return Err(format!(
                "legacy_pack_{}_shape_unverified",
                field.sig.as_str().to_ascii_lowercase()
            ));
        }
        let type_code = read_u32(bytes, 0);
        let raw_payload = read_u32(bytes, 4);
        let reference_typed = if is_location {
            matches!(type_code, 0 | 1 | 4)
        } else {
            matches!(type_code, 0 | 1)
        };
        let implicit = if is_location {
            matches!(type_code, 2 | 3 | 6 | 7)
        } else {
            type_code == 3
        };
        if implicit && raw_payload != 0 {
            return Err(format!(
                "legacy_pack_{}_implicit_payload_nonzero",
                field.sig.as_str().to_ascii_lowercase()
            ));
        }
        let payload = if reference_typed && raw_payload != 0 {
            source_form_key_for_raw_form_id(
                raw_payload,
                source_plugin_name,
                source_master_names,
                interner,
            )
            .map(FieldValue::FormKey)
            .unwrap_or_else(|| FieldValue::Uint(u64::from(raw_payload)))
        } else {
            FieldValue::Uint(u64::from(raw_payload))
        };
        let mut fields = vec![
            (
                interner.intern("type"),
                FieldValue::Uint(u64::from(type_code)),
            ),
            (interner.intern(payload_name), payload),
        ];
        fields.push((
            interner.intern(if is_location {
                "radius"
            } else {
                "count_distance"
            }),
            FieldValue::Int(i64::from(read_i32(bytes, 8))),
        ));
        if !is_location {
            fields.push((
                interner.intern("unknown"),
                FieldValue::Uint(u64::from(read_u32(bytes, 12))),
            ));
        }
        field.value = FieldValue::Struct(fields);
    }
    Ok(())
}

fn target_record(source: &Record) -> Record {
    let mut output = Record::new(SigCode(*b"PACK"), source.form_key);
    output.eid = source.eid;
    output.flags = source.flags;
    output.warnings = source.warnings.clone();
    if let Some(editor_id) = source.eid {
        push_value(&mut output, b"EDID", FieldValue::String(editor_id));
    }
    output
}

fn push_common_header(output: &mut Record, proof: &GenericLegacyPackageShapeProof) {
    let mut data = [0_u8; 12];
    data[..4].copy_from_slice(&proof.general_flags.to_le_bytes());
    data[4] = 18;
    data[6] = 2;
    data[8..10].copy_from_slice(&proof.interrupt_flags.to_le_bytes());
    push_bytes(output, b"PKDT", &data);

    let mut data = [0_u8; 12];
    data[0] = proof.schedule_month as u8;
    data[1] = proof.schedule_day_of_week as u8;
    data[2] = proof.schedule_date as u8;
    data[3] = proof.schedule_hour as u8;
    data[4] = u8::MAX;
    data[8..12].copy_from_slice(&(proof.schedule_duration_hours as u32).to_le_bytes());
    push_bytes(output, b"PSDT", &data);
}

fn lower_fallback_travel(output: &mut Record, interner: &StringInterner) {
    push_pack_counter(output, 4, FO4_TRAVEL_TEMPLATE_LOCAL, 1, interner);
    push_string(output, b"ANAM", "Location", interner);
    push_struct(
        output,
        b"PLDT",
        [
            ("type", FieldValue::Int(2)),
            ("location_value", FieldValue::Uint(0)),
            ("radius", FieldValue::Int(0)),
            ("collection_index", FieldValue::Uint(0)),
        ],
        interner,
    );
    for _ in 0..3 {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(false));
    }
    for index in [1_u8, 3, 5, 7] {
        push_bytes(output, b"UNAM", &[index]);
    }
    push_bytes(output, b"XNAM", &[0x08]);
    push_empty_events(output);
}

fn default_fallback_travel_proof() -> GenericLegacyPackageShapeProof {
    GenericLegacyPackageShapeProof {
        schema: SOURCE_SHAPE_SCHEMA,
        family: GenericLegacyPackageFamily::Unsupported,
        policy: GenericLegacyPackagePolicy::FallbackTravel,
        template_local: 0,
        general_flags: 0,
        interrupt_flags: 0,
        schedule_month: -1,
        schedule_day_of_week: -1,
        schedule_date: 0,
        schedule_hour: -1,
        schedule_duration_hours: 0,
        location_type: 2,
        object_type: None,
        radius: 0,
        repeatable: None,
    }
}

fn lower_travel(
    output: &mut Record,
    inventory: &LegacyPackInventory,
    interner: &StringInterner,
) -> Result<(), String> {
    let location = &inventory.unions[0];
    push_pack_counter(output, 4, FO4_TRAVEL_TEMPLATE_LOCAL, 1, interner);
    push_string(output, b"ANAM", "Location", interner);
    let payload = target_location_payload(&location.payload, interner)?;
    push_struct(
        output,
        b"PLDT",
        [
            ("type", FieldValue::Int(i64::from(location.type_code))),
            ("location_value", payload),
            (
                "radius",
                FieldValue::Int(i64::from(location.radius_or_distance)),
            ),
            ("collection_index", FieldValue::Uint(0)),
        ],
        interner,
    );
    for _ in 0..3 {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(false));
    }
    for index in [1_u8, 3, 5, 7] {
        push_bytes(output, b"UNAM", &[index]);
    }
    push_bytes(output, b"XNAM", &[0x08]);
    push_empty_events(output);
    Ok(())
}

fn lower_patrol(
    output: &mut Record,
    inventory: &LegacyPackInventory,
    interner: &StringInterner,
) -> Result<(), String> {
    let location = &inventory.unions[0];
    let LegacyPackUnionPayload::Reference {
        source_form_key: Some(source_form_key),
        ..
    } = &location.payload
    else {
        return Err("legacy_pack_patrol_path_start_unverified".to_string());
    };
    let path_start = FormKey::parse(source_form_key, interner)
        .map_err(|_| "legacy_pack_patrol_path_start_identity_invalid".to_string())?;
    let repeatable = inventory
        .patrol_data
        .as_ref()
        .ok_or_else(|| "legacy_pack_patrol_data_missing".to_string())?
        .repeatable;
    push_pack_counter(output, 7, FO4_PATROL_TEMPLATE_LOCAL, 2, interner);
    push_string(output, b"ANAM", "SingleRef", interner);
    push_struct(
        output,
        b"PTDA",
        [
            ("target_data_type", FieldValue::Int(0)),
            ("target_data_target", FieldValue::FormKey(path_start)),
            ("target_data_count_distance", FieldValue::Int(0)),
        ],
        interner,
    );
    push_string(output, b"ANAM", "Float", interner);
    push_value(
        output,
        b"CNAM",
        FieldValue::Float(location.radius_or_distance as f32),
    );
    for value in [repeatable, false, false, false] {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(value));
    }
    push_string(output, b"ANAM", "Float", interner);
    push_value(output, b"CNAM", FieldValue::Float(0.0));
    for index in [0_u8, 1, 2, 4, 6, 8, 10] {
        push_bytes(output, b"UNAM", &[index]);
    }
    push_bytes(output, b"XNAM", &[0x0B]);
    push_empty_events(output);
    Ok(())
}

fn lower_dialogue_force_greet(
    output: &mut Record,
    inventory: &LegacyPackInventory,
    record: &Record,
    source_plugin_name: &str,
    source_master_names: &[String],
    interner: &StringInterner,
) -> Result<(), String> {
    let target = unique_union(inventory, "PTDT")
        .ok_or_else(|| "legacy_pack_force_greet_requires_one_target".to_string())?;
    let target = target_reference(&target.payload, interner, "force_greet_target")?;
    let pkdd = unique_bytes(record, b"PKDD")
        .ok_or_else(|| "legacy_pack_force_greet_pkdd_not_bytes".to_string())?;
    let raw_topic = read_u32(pkdd, 4);

    push_pack_counter(output, 24, FO4_FORCE_GREET_TEMPLATE_LOCAL, 11, interner);
    push_string(output, b"ANAM", "Topic", interner);
    if raw_topic == 0 {
        push_struct(
            output,
            b"PDTO",
            [
                ("type", FieldValue::Uint(1)),
                ("data", FieldValue::String(interner.intern("HELO"))),
            ],
            interner,
        );
    } else {
        let topic = source_form_key_for_raw_form_id(
            raw_topic,
            source_plugin_name,
            source_master_names,
            interner,
        )
        .ok_or_else(|| "legacy_pack_force_greet_topic_unresolved".to_string())?;
        push_struct(
            output,
            b"PDTO",
            [
                ("type", FieldValue::Uint(0)),
                ("data", FieldValue::FormKey(topic)),
            ],
            interner,
        );
    }
    push_string(output, b"ANAM", "Location", interner);
    push_target_location(output, 3, FieldValue::Uint(0), 0, interner);
    push_string(output, b"ANAM", "Location", interner);
    push_target_location(output, 3, FieldValue::Uint(0), 750, interner);
    push_string(output, b"ANAM", "Location", interner);
    push_target_location(
        output,
        0,
        FieldValue::FormKey(target),
        inventory.unions[0].radius_or_distance,
        interner,
    );
    for value in [false, true] {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(value));
    }
    push_string(output, b"ANAM", "SingleRef", interner);
    push_target_reference(output, target, 0, interner);
    push_string(output, b"ANAM", "Location", interner);
    push_target_location(output, 0, FieldValue::FormKey(target), 5000, interner);
    for value in [
        false, false, false, false, false, true, true, false, true, false, false, true, false,
    ] {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(value));
    }
    push_string(output, b"ANAM", "TargetSelector", interner);
    push_target_selector(output, 0, interner);
    push_string(output, b"ANAM", "Float", interner);
    push_value(output, b"CNAM", FieldValue::Float(0.0));
    push_string(output, b"ANAM", "Int", interner);
    push_value(output, b"CNAM", FieldValue::Int(0));
    push_string(output, b"ANAM", "TargetSelector", interner);
    push_target_selector(output, 0, interner);
    for index in [
        0_u8, 1, 2, 5, 28, 7, 4, 9, 31, 13, 15, 16, 17, 18, 19, 20, 26, 29, 36, 22, 24, 32, 33, 34,
    ] {
        push_bytes(output, b"UNAM", &[index]);
    }
    push_bytes(output, b"XNAM", &[0x25]);
    push_empty_events(output);
    Ok(())
}

fn lower_follow(
    output: &mut Record,
    inventory: &LegacyPackInventory,
    record: &Record,
    interner: &StringInterner,
) -> Result<(), String> {
    let target = unique_union(inventory, "PTDT")
        .ok_or_else(|| "legacy_pack_follow_requires_one_target".to_string())?;
    let target_reference = target_reference(&target.payload, interner, "follow_target")?;
    let start_radius = unique_f32(record, b"PKFD")
        .ok_or_else(|| "legacy_pack_follow_pkfd_not_finite".to_string())?;
    push_pack_counter(output, 5, FO4_FOLLOW_TEMPLATE_LOCAL, 0, interner);
    push_string(output, b"ANAM", "SingleRef", interner);
    push_target_reference(output, target_reference, 0, interner);
    for radius in [start_radius, target.radius_or_distance as f32] {
        push_string(output, b"ANAM", "Float", interner);
        push_value(output, b"CNAM", FieldValue::Float(radius));
    }
    for value in [true, false] {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(value));
    }
    for index in [0_u8, 1, 2, 4, 6] {
        push_bytes(output, b"UNAM", &[index]);
    }
    push_bytes(output, b"XNAM", &[0x07]);
    push_empty_events(output);
    Ok(())
}

fn lower_escort(
    output: &mut Record,
    inventory: &LegacyPackInventory,
    record: &Record,
    interner: &StringInterner,
) -> Result<(), String> {
    let location = unique_union(inventory, "PLDT")
        .ok_or_else(|| "legacy_pack_escort_requires_one_destination".to_string())?;
    let target = unique_union(inventory, "PTDT")
        .ok_or_else(|| "legacy_pack_escort_requires_one_target".to_string())?;
    let target_reference = target_reference(&target.payload, interner, "escort_target")?;
    let escort_distance = unique_u32(record, b"PKE2")
        .ok_or_else(|| "legacy_pack_escort_distance_not_u32".to_string())?;

    push_pack_counter(output, 10, FO4_ESCORT_TEMPLATE_LOCAL, 11, interner);
    push_string(output, b"ANAM", "ObjectList", interner);
    push_value(output, b"CNAM", FieldValue::Float(0.0));
    push_string(output, b"ANAM", "Int", interner);
    push_value(output, b"CNAM", FieldValue::Int(1));
    push_string(output, b"ANAM", "Location", interner);
    push_location_union(output, location, interner)?;
    for radius in [
        escort_distance as f32,
        0.0,
        target.radius_or_distance.max(0) as f32,
    ] {
        push_string(output, b"ANAM", "Float", interner);
        push_value(output, b"CNAM", FieldValue::Float(radius));
    }
    push_string(output, b"ANAM", "SingleRef", interner);
    push_target_reference(output, target_reference, 0, interner);
    for value in [true, false] {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(value));
    }
    push_string(output, b"ANAM", "Float", interner);
    push_value(output, b"CNAM", FieldValue::Float(0.0));
    for index in [17_u8, 18, 19, 20, 21, 22, 23, 25, 27, 29] {
        push_bytes(output, b"UNAM", &[index]);
    }
    push_bytes(output, b"XNAM", &[0x1E]);
    push_empty_events(output);
    Ok(())
}

fn lower_sandbox(
    output: &mut Record,
    inventory: &LegacyPackInventory,
    interner: &StringInterner,
) -> Result<(), String> {
    let location = unique_union(inventory, "PLDT")
        .ok_or_else(|| "legacy_pack_sandbox_requires_one_location".to_string())?;
    push_pack_counter(output, 15, FO4_SANDBOX_TEMPLATE_LOCAL, 7, interner);
    push_string(output, b"ANAM", "Location", interner);
    push_location_union(output, location, interner)?;
    for value in [false, true, true, true, true, true, true, false, false] {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(value));
    }
    push_string(output, b"ANAM", "Float", interner);
    push_value(output, b"CNAM", FieldValue::Float(50.0));
    for value in [false, false] {
        push_string(output, b"ANAM", "Bool", interner);
        push_value(output, b"CNAM", FieldValue::Bool(value));
    }
    push_string(output, b"ANAM", "TargetSelector", interner);
    push_target_selector(output, 0, interner);
    push_string(output, b"ANAM", "Float", interner);
    push_value(output, b"CNAM", FieldValue::Float(150.0));
    for index in [2_u8, 12, 5, 6, 7, 8, 9, 14, 10, 22, 18, 20, 16, 24, 26] {
        push_bytes(output, b"UNAM", &[index]);
    }
    push_bytes(output, b"XNAM", &[0x1B]);
    push_empty_events(output);
    Ok(())
}

fn lower_activate(
    output: &mut Record,
    inventory: &LegacyPackInventory,
    interner: &StringInterner,
) -> Result<(), String> {
    let target = unique_union(inventory, "PTDT")
        .ok_or_else(|| "legacy_pack_activate_requires_one_target".to_string())?;
    let target_reference = target_reference(&target.payload, interner, "activate_target")?;
    push_pack_counter(output, 2, FO4_ACTIVATE_TEMPLATE_LOCAL, 0, interner);
    push_string(output, b"ANAM", "SingleRef", interner);
    push_target_reference(output, target_reference, 0, interner);
    push_string(output, b"ANAM", "Int", interner);
    push_value(
        output,
        b"CNAM",
        FieldValue::Int(i64::from(target.radius_or_distance)),
    );
    for index in [0_u8, 1] {
        push_bytes(output, b"UNAM", &[index]);
    }
    push_bytes(output, b"XNAM", &[0x02]);
    push_empty_events(output);
    Ok(())
}

fn lower_use_item(
    output: &mut Record,
    inventory: &LegacyPackInventory,
    interner: &StringInterner,
) -> Result<(), String> {
    let target = unique_union(inventory, "PTDT")
        .ok_or_else(|| "legacy_pack_use_item_requires_one_target".to_string())?;
    if target.radius_or_distance != 1 {
        return Err("legacy_pack_use_item_requires_single_activation".to_string());
    }
    let target_reference = target_reference(&target.payload, interner, "use_item_target")?;
    push_pack_counter(output, 1, FO4_USE_IDLE_MARKER_TEMPLATE_LOCAL, 0, interner);
    push_string(output, b"ANAM", "SingleRef", interner);
    push_target_reference(output, target_reference, 0, interner);
    push_bytes(output, b"UNAM", &[0]);
    push_bytes(output, b"XNAM", &[0x01]);
    push_empty_events(output);
    Ok(())
}

fn target_location_payload(
    payload: &LegacyPackUnionPayload,
    interner: &StringInterner,
) -> Result<FieldValue, String> {
    match payload {
        LegacyPackUnionPayload::Reference {
            source_form_key: Some(source),
            ..
        } => FormKey::parse(source, interner)
            .map(FieldValue::FormKey)
            .map_err(|_| "legacy_pack_travel_location_identity_invalid".to_string()),
        LegacyPackUnionPayload::ObjectType { value } => Ok(FieldValue::Uint(u64::from(*value))),
        LegacyPackUnionPayload::Implicit => Ok(FieldValue::Uint(0)),
        LegacyPackUnionPayload::Reference { .. } => {
            Err("legacy_pack_travel_location_payload_unverified".to_string())
        }
    }
}

fn resolved_reference(payload: &LegacyPackUnionPayload) -> bool {
    matches!(
        payload,
        LegacyPackUnionPayload::Reference {
            present: true,
            source_form_key: Some(_),
        }
    )
}

fn is_player_reference(payload: &LegacyPackUnionPayload) -> bool {
    matches!(
        payload,
        LegacyPackUnionPayload::Reference {
            present: true,
            source_form_key: Some(source),
        } if source_local(source) == Some(0x14)
    )
}

fn source_local(source: &str) -> Option<u32> {
    u32::from_str_radix(source.split(['@', ':']).next()?, 16).ok()
}

fn unique_union<'a>(
    inventory: &'a LegacyPackInventory,
    signature: &str,
) -> Option<&'a crate::translator::pair_hooks::fnv_pack::LegacyPackUnionInventory> {
    let mut matches = inventory
        .unions
        .iter()
        .filter(|union| union.sig == signature);
    let value = matches.next()?;
    matches.next().is_none().then_some(value)
}

fn has_exact_type_specific(inventory: &LegacyPackInventory, signature: &str, size: usize) -> bool {
    inventory
        .type_specific_subrecords
        .iter()
        .any(|entry| entry.sig == signature && entry.count == 1 && entry.observed_sizes == [size])
}

fn unique_bytes<'a>(record: &'a Record, signature: &[u8; 4]) -> Option<&'a [u8]> {
    let mut matches = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *signature);
    let field = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    match &field.value {
        FieldValue::Bytes(bytes) => Some(bytes),
        _ => None,
    }
}

fn unique_u32(record: &Record, signature: &[u8; 4]) -> Option<u32> {
    let bytes = unique_bytes(record, signature)?;
    (bytes.len() == 4).then(|| read_u32(bytes, 0))
}

fn unique_f32(record: &Record, signature: &[u8; 4]) -> Option<f32> {
    let bytes = unique_bytes(record, signature)?;
    if bytes.len() != 4 {
        return None;
    }
    let value = f32::from_le_bytes(bytes.try_into().ok()?);
    value.is_finite().then_some(value)
}

fn target_reference(
    payload: &LegacyPackUnionPayload,
    interner: &StringInterner,
    family: &str,
) -> Result<FormKey, String> {
    let LegacyPackUnionPayload::Reference {
        present: true,
        source_form_key: Some(source),
    } = payload
    else {
        return Err(format!("legacy_pack_{family}_reference_unresolved"));
    };
    FormKey::parse(source, interner)
        .map_err(|_| format!("legacy_pack_{family}_reference_identity_invalid"))
}

fn push_location_union(
    output: &mut Record,
    location: &crate::translator::pair_hooks::fnv_pack::LegacyPackUnionInventory,
    interner: &StringInterner,
) -> Result<(), String> {
    let LegacyPackUnionKind::Location(location_type) = location.union_kind else {
        return Err("legacy_pack_location_union_required".to_string());
    };
    push_target_location(
        output,
        i64::from(location_type_code(location_type)),
        target_location_payload(&location.payload, interner)?,
        location.radius_or_distance,
        interner,
    );
    Ok(())
}

fn push_target_location(
    output: &mut Record,
    location_type: i64,
    payload: FieldValue,
    radius: i32,
    interner: &StringInterner,
) {
    push_struct(
        output,
        b"PLDT",
        [
            ("type", FieldValue::Int(location_type)),
            ("location_value", payload),
            ("radius", FieldValue::Int(i64::from(radius))),
            ("collection_index", FieldValue::Uint(0)),
        ],
        interner,
    );
}

fn push_target_reference(
    output: &mut Record,
    target: FormKey,
    distance: i32,
    interner: &StringInterner,
) {
    push_struct(
        output,
        b"PTDA",
        [
            ("target_data_type", FieldValue::Int(0)),
            ("target_data_target", FieldValue::FormKey(target)),
            (
                "target_data_count_distance",
                FieldValue::Int(i64::from(distance)),
            ),
        ],
        interner,
    );
}

fn push_target_selector(output: &mut Record, object_type: u32, interner: &StringInterner) {
    push_struct(
        output,
        b"PTDA",
        [
            ("target_data_type", FieldValue::Int(2)),
            (
                "target_data_target",
                FieldValue::Uint(u64::from(object_type)),
            ),
            ("target_data_count_distance", FieldValue::Int(0)),
        ],
        interner,
    );
}

fn push_pack_counter(
    output: &mut Record,
    count: u32,
    template_local: u32,
    version: u32,
    interner: &StringInterner,
) {
    push_struct(
        output,
        b"PKCU",
        [
            ("data_input_count", FieldValue::Uint(u64::from(count))),
            (
                "package_template",
                FieldValue::FormKey(FormKey {
                    local: template_local,
                    plugin: interner.intern("Fallout4.esm"),
                }),
            ),
            (
                "version_counter_autoincremented",
                FieldValue::Uint(u64::from(version)),
            ),
        ],
        interner,
    );
}

fn push_empty_events(output: &mut Record) {
    for marker in [b"POBA", b"POEA", b"POCA"] {
        push_value(output, marker, FieldValue::None);
        push_bytes(output, b"INAM", &[0; 4]);
        push_bytes(output, b"PDTO", &[0; 8]);
    }
}

fn push_struct<const N: usize>(
    output: &mut Record,
    signature: &[u8; 4],
    fields: [(&str, FieldValue); N],
    interner: &StringInterner,
) {
    push_value(
        output,
        signature,
        FieldValue::Struct(
            fields
                .into_iter()
                .map(|(name, value)| (interner.intern(name), value))
                .collect(),
        ),
    );
}

fn push_string(output: &mut Record, signature: &[u8; 4], value: &str, interner: &StringInterner) {
    push_value(
        output,
        signature,
        FieldValue::String(interner.intern(value)),
    );
}

fn push_bytes(output: &mut Record, signature: &[u8; 4], bytes: &[u8]) {
    push_value(
        output,
        signature,
        FieldValue::Bytes(SmallVec::from_slice(bytes)),
    );
}

fn push_value(output: &mut Record, signature: &[u8; 4], value: FieldValue) {
    output.fields.push(FieldEntry {
        sig: SubrecordSig(*signature),
        value,
    });
}

fn location_type_code(location_type: LegacyPackLocationType) -> u32 {
    match location_type {
        LegacyPackLocationType::NearReference => 0,
        LegacyPackLocationType::InCell => 1,
        LegacyPackLocationType::NearCurrentLocation => 2,
        LegacyPackLocationType::NearEditorLocation => 3,
        LegacyPackLocationType::ObjectId => 4,
        LegacyPackLocationType::ObjectType => 5,
        LegacyPackLocationType::NearLinkedReference => 6,
        LegacyPackLocationType::AtPackageLocation => 7,
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated PACK row"),
    )
}

fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated PACK row"),
    )
}

#[cfg(test)]
mod tests {
    use crate::schema::AuthoringSchema;

    use super::*;

    fn field(signature: &[u8; 4], bytes: &[u8]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*signature),
            value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
        }
    }

    fn fixture(
        interner: &StringInterner,
        local: u32,
        package_type: u8,
        location_type: u32,
        raw_location: u32,
    ) -> Record {
        let mut record = Record::new(
            SigCode(*b"PACK"),
            FormKey {
                local,
                plugin: interner.intern("Owner.esp"),
            },
        );
        record.eid = Some(interner.intern(&format!("Package{local:06X}")));
        let mut pkdt = [0_u8; 12];
        pkdt[..4].copy_from_slice(&0x0000_0204_u32.to_le_bytes());
        pkdt[4] = package_type;
        pkdt[6..8].copy_from_slice(&0x20_u16.to_le_bytes());
        let mut schedule = [0_u8; 8];
        schedule[..4].copy_from_slice(&[u8::MAX, u8::MAX, 0, u8::MAX]);
        let mut location = [0_u8; 12];
        location[..4].copy_from_slice(&location_type.to_le_bytes());
        location[4..8].copy_from_slice(&raw_location.to_le_bytes());
        location[8..12].copy_from_slice(&128_i32.to_le_bytes());
        record.fields.extend([
            field(b"PKDT", &pkdt),
            field(b"PSDT", &schedule),
            field(b"PLDT", &location),
        ]);
        for marker in [b"POBA", b"POEA", b"POCA"] {
            record.fields.push(field(marker, &[]));
            record.fields.push(field(b"SCHR", &[0; 20]));
        }
        record
    }

    #[test]
    fn content_proof_is_source_identity_independent_and_load_order_aware() {
        let interner = StringInterner::new();
        let masters = ["Master.esm".to_string()];
        let first = fixture(&interner, 0x800, 6, 0, 0x0000_1234);
        let second = fixture(&interner, 0x900, 6, 0, 0x0000_5678);
        let classify = |record| {
            classify_generic_legacy_package(
                record,
                LegacyPackSourceFamily::Fnv,
                "Owner.esp",
                &masters,
                &interner,
            )
        };
        let GenericLegacyPackageSupport::Ready { proof: first_proof } = classify(&first) else {
            panic!("first Travel shape must be Ready");
        };
        let GenericLegacyPackageSupport::Ready {
            proof: second_proof,
        } = classify(&second)
        else {
            panic!("second Travel shape must be Ready");
        };
        assert_eq!(first_proof, second_proof);
        assert_eq!(first_proof.schema, SOURCE_SHAPE_SCHEMA);
        assert_eq!(first_proof.general_flags, 0x0000_0204);
        assert_eq!(first_proof.interrupt_flags, 0x20);
        assert_eq!(first_proof.schedule_month, -1);
        assert_eq!(first_proof.schedule_day_of_week, -1);
        assert_eq!(first_proof.schedule_date, 0);
        assert_eq!(first_proof.schedule_hour, -1);
        assert_eq!(first_proof.schedule_duration_hours, 0);
    }

    #[test]
    fn travel_projection_encodes_verified_fo4_template_and_location() {
        let interner = StringInterner::new();
        let masters = ["Master.esm".to_string()];
        let mut record = fixture(&interner, 0x800, 6, 0, 0x0000_1234);
        let proof = lower_generic_legacy_package(
            &mut record,
            LegacyPackSourceFamily::Fnv,
            "Owner.esp",
            &masters,
            &interner,
        )
        .unwrap()
        .expect("Travel lowers");
        assert_eq!(proof.policy, GenericLegacyPackagePolicy::Travel);
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let encode = |signature| {
            let field = record
                .fields
                .iter()
                .find(|field| field.sig.0 == signature)
                .unwrap();
            crate::target_write::encode_field_pub(field, schema.record_def("PACK"), &interner)
                .unwrap()
        };
        let counter = encode(*b"PKCU");
        assert_eq!(read_u32(&counter, 0), 4);
        assert_eq!(read_u32(&counter, 4), FO4_TRAVEL_TEMPLATE_LOCAL);
        assert_eq!(read_u32(&counter, 8), 1);
        let location = encode(*b"PLDT");
        assert_eq!(location.len(), 16);
        assert_eq!(read_u32(&location, 0), 0);
        assert_eq!(read_u32(&location, 4), 0x1234);
        assert_eq!(read_i32(&location, 8), 128);
        assert_eq!(read_u32(&location, 12), 0);
    }

    #[test]
    fn patrol_projection_preserves_repeatable_and_exact_reference() {
        let interner = StringInterner::new();
        let masters = ["Master.esm".to_string()];
        let mut record = fixture(&interner, 0x801, 13, 0, 0x0000_4321);
        record.fields.insert(3, field(b"PKPT", &[1, 0]));
        let proof = lower_generic_legacy_package(
            &mut record,
            LegacyPackSourceFamily::Fo3,
            "Owner.esp",
            &masters,
            &interner,
        )
        .unwrap()
        .expect("Patrol lowers");
        assert_eq!(proof.policy, GenericLegacyPackagePolicy::Patrol);
        assert_eq!(proof.repeatable, Some(true));
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let target = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"PTDA")
            .unwrap();
        let target =
            crate::target_write::encode_field_pub(target, schema.record_def("PACK"), &interner)
                .unwrap();
        assert_eq!(target.len(), 12);
        assert_eq!(read_u32(&target, 4), 0x4321);
        let bools = record
            .fields
            .iter()
            .filter_map(|field| match field.value {
                FieldValue::Bool(value) => Some(value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(bools, [true, false, false, false]);
    }

    fn target(kind: u32, raw_target: u32, distance: i32) -> FieldEntry {
        let mut bytes = [0_u8; 16];
        bytes[..4].copy_from_slice(&kind.to_le_bytes());
        bytes[4..8].copy_from_slice(&raw_target.to_le_bytes());
        bytes[8..12].copy_from_slice(&distance.to_le_bytes());
        field(b"PTDT", &bytes)
    }

    fn add_before_events(record: &mut Record, fields: impl IntoIterator<Item = FieldEntry>) {
        let mut index = record
            .fields
            .iter()
            .position(|field| field.sig.0 == *b"POBA")
            .unwrap_or(record.fields.len());
        for field in fields {
            record.fields.insert(index, field);
            index += 1;
        }
    }

    fn without_location(mut record: Record) -> Record {
        record.fields.retain(|field| field.sig.0 != *b"PLDT");
        record
    }

    fn assert_fo4_pack_encodes(record: &Record, interner: &StringInterner) {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        for field in &record.fields {
            crate::target_write::encode_field_pub(field, schema.record_def("PACK"), interner)
                .unwrap_or_else(|error| panic!("{}: {error}", field.sig.as_str()));
        }
    }

    fn template_local(record: &Record, interner: &StringInterner) -> u32 {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let counter = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"PKCU")
            .unwrap();
        let bytes =
            crate::target_write::encode_field_pub(counter, schema.record_def("PACK"), interner)
                .unwrap();
        read_u32(&bytes, 4)
    }

    #[test]
    fn dialogue_follow_escort_sandbox_activate_and_use_item_have_exact_family_receipts() {
        let interner = StringInterner::new();
        let masters = ["Master.esm".to_string()];
        let mut dialogue = without_location(fixture(&interner, 0x810, 15, 3, 0));
        let mut pkdd = [0_u8; 24];
        pkdd[16..20].copy_from_slice(&1_u32.to_le_bytes());
        add_before_events(
            &mut dialogue,
            [target(0, 0x0000_0014, 128), field(b"PKDD", &pkdd)],
        );

        let mut follow = without_location(fixture(&interner, 0x811, 1, 3, 0));
        add_before_events(
            &mut follow,
            [
                target(0, 0x0000_1234, 128),
                field(b"PKFD", &32_f32.to_le_bytes()),
            ],
        );

        let mut escort = fixture(&interner, 0x812, 2, 3, 0);
        add_before_events(
            &mut escort,
            [
                target(0, 0x0000_0014, 128),
                field(b"PKE2", &300_u32.to_le_bytes()),
            ],
        );

        let sandbox = fixture(&interner, 0x813, 12, 3, 0);

        let mut activate = without_location(fixture(&interner, 0x814, 8, 3, 0));
        add_before_events(&mut activate, [target(0, 0x0000_1234, 2)]);

        let mut use_item = without_location(fixture(&interner, 0x815, 8, 3, 0));
        add_before_events(
            &mut use_item,
            [target(0, 0x0000_1234, 1), field(b"PUID", &[])],
        );

        for (record, family, policy, template) in [
            (
                dialogue,
                GenericLegacyPackageFamily::DialogueForceGreet,
                GenericLegacyPackagePolicy::DialogueForceGreet,
                FO4_FORCE_GREET_TEMPLATE_LOCAL,
            ),
            (
                follow,
                GenericLegacyPackageFamily::Follow,
                GenericLegacyPackagePolicy::Follow,
                FO4_FOLLOW_TEMPLATE_LOCAL,
            ),
            (
                escort,
                GenericLegacyPackageFamily::Escort,
                GenericLegacyPackagePolicy::Escort,
                FO4_ESCORT_TEMPLATE_LOCAL,
            ),
            (
                sandbox,
                GenericLegacyPackageFamily::Sandbox,
                GenericLegacyPackagePolicy::Sandbox,
                FO4_SANDBOX_TEMPLATE_LOCAL,
            ),
            (
                activate,
                GenericLegacyPackageFamily::Activate,
                GenericLegacyPackagePolicy::Activate,
                FO4_ACTIVATE_TEMPLATE_LOCAL,
            ),
            (
                use_item,
                GenericLegacyPackageFamily::UseItem,
                GenericLegacyPackagePolicy::UseItem,
                FO4_USE_IDLE_MARKER_TEMPLATE_LOCAL,
            ),
        ] {
            let GenericLegacyPackageSupport::Ready { proof } = classify_generic_legacy_package(
                &record,
                LegacyPackSourceFamily::Fnv,
                "Owner.esp",
                &masters,
                &interner,
            ) else {
                panic!("{family:?} must be Ready");
            };
            assert_eq!(proof.family, family);
            assert_eq!(proof.policy, policy);
            assert_eq!(proof.template_local, template);

            let mut lowered = record;
            let lowered_proof = lower_generic_legacy_package(
                &mut lowered,
                LegacyPackSourceFamily::Fnv,
                "Owner.esp",
                &masters,
                &interner,
            )
            .unwrap()
            .expect("family lowers");
            assert_eq!(lowered_proof.family, family);
            assert_eq!(template_local(&lowered, &interner), template);
            assert_fo4_pack_encodes(&lowered, &interner);
        }
    }

    #[test]
    fn structurally_valid_unsupported_packages_lower_to_a_receipted_fallback() {
        let interner = StringInterner::new();
        let classify = |record: &Record| {
            classify_generic_legacy_package(
                record,
                LegacyPackSourceFamily::Fnv,
                "Owner.esp",
                &["Master.esm".to_string()],
                &interner,
            )
        };
        let mut condition = fixture(&interner, 0x800, 6, 6, 0);
        condition.fields.insert(2, field(b"CTDA", &[0; 28]));
        let GenericLegacyPackageSupport::Fallback { warning_codes, .. } = classify(&condition)
        else {
            panic!("conditioned Travel must be blocked");
        };
        assert!(warning_codes.contains(&"legacy_pack_conditions_omitted".into()));

        let mut flags = fixture(&interner, 0x802, 6, 6, 0);
        let FieldValue::Bytes(pkdt) = &mut flags.fields[0].value else {
            unreachable!()
        };
        pkdt[0..4].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
        let GenericLegacyPackageSupport::Fallback { warning_codes, .. } = classify(&flags) else {
            panic!("unshared flags must be blocked");
        };
        assert!(warning_codes.contains(&"legacy_pack_general_flags_not_fo4_shared".into()));

        let mut unused = fixture(&interner, 0x803, 6, 6, 0);
        let FieldValue::Bytes(pkdt) = &mut unused.fields[0].value else {
            unreachable!()
        };
        pkdt[5] = 1;
        let GenericLegacyPackageSupport::Fallback { warning_codes, .. } = classify(&unused) else {
            panic!("nonzero unused PKDT bytes must be blocked");
        };
        assert!(warning_codes.contains(&"legacy_pack_unused_pkdt_bytes_nonzero".into()));

        let mut scripted = fixture(&interner, 0x803, 6, 6, 0);
        let begin = scripted
            .fields
            .iter()
            .position(|field| field.sig.0 == *b"POBA")
            .unwrap();
        let FieldValue::Bytes(header) = &mut scripted.fields[begin + 1].value else {
            unreachable!()
        };
        header[8..12].copy_from_slice(&1_u32.to_le_bytes());
        scripted.fields.insert(begin + 2, field(b"SCDA", &[0x01]));
        let GenericLegacyPackageSupport::Fallback { warning_codes, .. } = classify(&scripted)
        else {
            panic!("nonempty event script must be blocked");
        };
        assert!(warning_codes.contains(&"legacy_pack_event_behavior_requires_port".into()));

        let mut sandbox = fixture(&interner, 0x804, 12, 3, 0);
        let mut second = [0_u8; 12];
        second[..4].copy_from_slice(&7_u32.to_le_bytes());
        add_before_events(&mut sandbox, [field(b"PLD2", &second)]);
        let GenericLegacyPackageSupport::Fallback { warning_codes, .. } = classify(&sandbox) else {
            panic!("two-location sandbox must be blocked");
        };
        assert!(warning_codes.contains(&"legacy_pack_sandbox_requires_one_location".into()));

        let mut use_item = without_location(fixture(&interner, 0x805, 8, 3, 0));
        add_before_events(
            &mut use_item,
            [target(0, 0x0000_1234, 2), field(b"PUID", &[])],
        );
        let GenericLegacyPackageSupport::Fallback { warning_codes, .. } = classify(&use_item)
        else {
            panic!("multi-use item package must be blocked");
        };
        assert!(warning_codes.contains(&"legacy_pack_use_item_requires_single_activation".into()));

        let proof = lower_generic_legacy_package(
            &mut condition,
            LegacyPackSourceFamily::Fnv,
            "Owner.esp",
            &["Master.esm".to_string()],
            &interner,
        )
        .unwrap()
        .expect("structurally valid conditioned package uses the deterministic fallback");
        assert_eq!(proof.policy, GenericLegacyPackagePolicy::FallbackTravel);
        assert_eq!(
            template_local(&condition, &interner),
            FO4_TRAVEL_TEMPLATE_LOCAL
        );
        assert_fo4_pack_encodes(&condition, &interner);
        let warnings = condition
            .warnings
            .iter()
            .filter_map(|warning| interner.resolve(*warning))
            .collect::<Vec<_>>();
        assert!(warnings.contains(&"legacy_pack_conditions_omitted"));
        assert!(warnings.contains(&"legacy_pack_type_6_uses_current_location_travel_fallback"));
    }

    #[test]
    fn malformed_or_unresolved_source_union_fails_closed_without_record_mutation() {
        let interner = StringInterner::new();
        let mut record = fixture(&interner, 0x804, 6, 6, 0);
        record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "PLDT")
            .unwrap()
            .value = FieldValue::Bytes(vec![0; 4].into());

        let GenericLegacyPackageSupport::Fallback {
            proof,
            warning_codes,
        } = classify_generic_legacy_package(
            &record,
            LegacyPackSourceFamily::Fnv,
            "Owner.esp",
            &[],
            &interner,
        )
        else {
            panic!("malformed source union must be blocked")
        };
        assert_eq!(proof.policy, GenericLegacyPackagePolicy::FallbackTravel);
        assert_eq!(warning_codes, ["legacy_pack_pldt_shape_unverified"]);
        let original = record.fields.clone();
        assert!(
            lower_generic_legacy_package(
                &mut record,
                LegacyPackSourceFamily::Fnv,
                "Owner.esp",
                &[],
                &interner,
            )
            .is_err()
        );
        assert_eq!(record.fields, original);

        let mut unresolved = fixture(&interner, 0x805, 6, 0, 0x0100_1234);
        let GenericLegacyPackageSupport::Fallback { warning_codes, .. } =
            classify_generic_legacy_package(
                &unresolved,
                LegacyPackSourceFamily::Fnv,
                "Owner.esp",
                &[],
                &interner,
            )
        else {
            panic!("an omitted unresolved source location must use the fallback");
        };
        assert!(
            warning_codes.contains(&"legacy_pack_unresolved_location_or_target_omitted".into())
        );
        let proof = lower_generic_legacy_package(
            &mut unresolved,
            LegacyPackSourceFamily::Fnv,
            "Owner.esp",
            &[],
            &interner,
        )
        .unwrap()
        .expect("unresolved structural source input is omitted by the fallback");
        assert_eq!(proof.policy, GenericLegacyPackagePolicy::FallbackTravel);
        assert_fo4_pack_encodes(&unresolved, &interner);
        assert!(unresolved.warnings.iter().any(|warning| {
            interner.resolve(*warning) == Some("legacy_pack_unresolved_location_or_target_omitted")
        }));
    }
}
