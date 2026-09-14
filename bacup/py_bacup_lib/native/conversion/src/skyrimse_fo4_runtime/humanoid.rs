use std::fmt;

use smallvec::SmallVec;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::materialized_npc_facegen::FacegenAlias;
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::sym::{StringInterner, Sym};

const PERSISTENT_CELL_CHILD_GROUP_TYPE: u32 = 8;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum SkyrimHumanoidFaceKind {
    Human,
    Child,
    Argonian,
    Khajiit,
    Dremora,
}

pub(crate) const SUPPORTED_SOURCE_HUMANOID_RACES: &[(&str, u32)] = &[
    ("BretonRace", 0x0001_3746),
    ("DarkElfRace", 0x0001_3746),
    ("HighElfRace", 0x0001_3746),
    ("ImperialRace", 0x0001_3746),
    ("NordRace", 0x0001_3746),
    ("OrcRace", 0x0001_3746),
    ("RedguardRace", 0x0001_3746),
    ("WoodElfRace", 0x0001_3746),
    ("ElderRace", 0x0001_3746),
    ("NordRaceAstrid", 0x0001_3746),
    ("DA13AfflictedRace", 0x0001_3746),
    ("BretonRaceVampire", 0x0001_3746),
    ("DarkElfRaceVampire", 0x0001_3746),
    ("ElderRaceVampire", 0x0001_3746),
    ("HighElfRaceVampire", 0x0001_3746),
    ("ImperialRaceVampire", 0x0001_3746),
    ("NordRaceVampire", 0x0001_3746),
    ("OrcRaceVampire", 0x0001_3746),
    ("RedguardRaceVampire", 0x0001_3746),
    ("WoodElfRaceVampire", 0x0001_3746),
    ("ArgonianRace", 0x0001_3746),
    ("ArgonianRaceVampire", 0x0001_3746),
    ("KhajiitRace", 0x0001_3746),
    ("KhajiitRaceVampire", 0x0001_3746),
    ("DremoraRace", 0x0001_3746),
    ("DLC2DremoraRace", 0x0001_3746),
    ("BretonRaceChild", 0x0011_D83F),
    ("ImperialRaceChild", 0x0011_D83F),
    ("NordRaceChild", 0x0011_D83F),
    ("RedguardRaceChild", 0x0011_D83F),
    ("BretonRaceChildVampire", 0x0011_D83F),
];

pub(crate) fn is_supported_source_humanoid_race(editor_id: &str) -> bool {
    SUPPORTED_SOURCE_HUMANOID_RACES
        .iter()
        .any(|(candidate, _)| candidate.eq_ignore_ascii_case(editor_id))
}

pub(crate) fn source_humanoid_face_kind(editor_id: &str) -> Option<SkyrimHumanoidFaceKind> {
    if !is_supported_source_humanoid_race(editor_id) {
        return None;
    }
    let editor_id = editor_id.to_ascii_lowercase();
    if editor_id.contains("argonian") {
        Some(SkyrimHumanoidFaceKind::Argonian)
    } else if editor_id.contains("khajiit") {
        Some(SkyrimHumanoidFaceKind::Khajiit)
    } else if editor_id.contains("dremora") {
        Some(SkyrimHumanoidFaceKind::Dremora)
    } else if editor_id.contains("child") {
        Some(SkyrimHumanoidFaceKind::Child)
    } else {
        Some(SkyrimHumanoidFaceKind::Human)
    }
}

const DROPPED_NPC_FIELDS: &[[u8; 4]] = &[
    *b"VMAD", *b"SNAM", *b"INAM", *b"TPLT", *b"TPTA", *b"SPCT", *b"SPLO", *b"PRKZ", *b"PRKR",
    *b"COCT", *b"CNTO", *b"KSIZ", *b"KWDA", *b"PKID", *b"CRIF", *b"SOFT", *b"DPLT", *b"GNAM",
    *b"CS2H", *b"CS2K", *b"CS2D", *b"CS2E", *b"CS2F", *b"CSCR", *b"PFRN", *b"ATKR",
];

const REQUIRED_DONOR_FACE_FIELDS: &[[u8; 4]] = &[
    *b"HCLF", *b"NAM5", *b"NAM6", *b"NAM4", *b"MWGT", *b"NAM8", *b"FTST", *b"QNAM", *b"MSDK",
    *b"MSDV", *b"MRSV", *b"FMIN",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TargetNativePlacement {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TargetNativeFacegenAssetCopy {
    pub source_archive: String,
    pub source_relative_path: String,
    pub target_relative_path: String,
    pub required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TargetNativeFacegenCopy {
    pub alias: FacegenAlias,
    pub geometry: TargetNativeFacegenAssetCopy,
    pub tint: Option<TargetNativeFacegenAssetCopy>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TargetNativeVoicePlan {
    pub form_key: FormKey,
    pub editor_id: String,
    pub flags: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TargetNativeHumanoidPlan {
    pub source_npc: FormKey,
    pub source_placed_actor: FormKey,
    pub source_cell: FormKey,
    pub source_race: FormKey,
    pub source_voice_type: FormKey,
    pub source_placement: TargetNativePlacement,
    pub source_required_acbs_flags: u32,
    pub source_level: u16,
    pub source_minimum_level: u16,
    pub source_full_name_lstring_id: Option<u32>,
    pub donor_npc: FormKey,
    pub donor_voice_type: FormKey,
    pub donor_race: FormKey,
    pub donor_class: FormKey,
    pub donor_package: FormKey,
    pub donor_outfit: FormKey,
    pub donor_acbs_flags: u32,
    pub donor_level: u16,
    pub target_npc: FormKey,
    pub target_placed_actor: FormKey,
    pub target_cell: FormKey,
    pub target_voice: TargetNativeVoicePlan,
    pub target_race: FormKey,
    pub target_class: FormKey,
    pub target_outfit: FormKey,
    pub target_package: FormKey,
    pub target_combat_style: FormKey,
    pub target_placement: TargetNativePlacement,
    pub target_acbs_flags: u32,
    pub target_level: u16,
    pub target_minimum_level: u16,
    pub target_disposition: i16,
    pub target_aidt: [u8; 24],
    pub npc_editor_id: String,
    pub placed_actor_editor_id: String,
    pub display_name: String,
    pub short_name: String,
    pub donor_editor_id: String,
    pub target_cell_editor_id: String,
    pub persistent_child_group_type: u32,
    pub target_coc_marker: FormKey,
    pub target_navmesh_evidence: FormKey,
    pub facegeom_source_archive: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HumanoidFormMapping {
    pub source: FormKey,
    pub target: FormKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HumanoidTopologyReceipt {
    pub source_placed_actor: FormKey,
    pub source_cell: FormKey,
    pub target_cell: FormKey,
    pub target_cell_editor_id: String,
    pub placed_actor: FormKey,
    pub persistent: bool,
    pub child_group_type: u32,
    pub target_coc_marker: FormKey,
    pub target_navmesh_evidence: FormKey,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HumanoidProjectionReceipt {
    pub mappings: Vec<HumanoidFormMapping>,
    pub topology: HumanoidTopologyReceipt,
    pub facegeom: TargetNativeFacegenAssetCopy,
}

#[derive(Clone, Debug)]
pub(crate) struct TargetNativeHumanoidProjection {
    pub npc: Record,
    pub voice_type: Record,
    pub placed_actor: Record,
    pub target_cell: FormKey,
    pub facegen_copy: TargetNativeFacegenCopy,
    pub receipt: HumanoidProjectionReceipt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NpcContractError {
    InvalidSource(&'static str),
    InvalidDonor(&'static str),
    InvalidTarget(&'static str),
    InvalidProjection(&'static str),
}

impl fmt::Display for NpcContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSource(reason) => write!(formatter, "invalid humanoid source: {reason}"),
            Self::InvalidDonor(reason) => {
                write!(formatter, "invalid target-native humanoid donor: {reason}")
            }
            Self::InvalidTarget(reason) => write!(formatter, "invalid humanoid target: {reason}"),
            Self::InvalidProjection(reason) => {
                write!(
                    formatter,
                    "invalid target-native humanoid projection: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for NpcContractError {}

pub(crate) fn project_target_native_humanoid(
    source_npc: &Record,
    source_achr: &Record,
    donor_npc: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<TargetNativeHumanoidProjection, NpcContractError> {
    validate_humanoid_plan(plan, interner)?;
    validate_source_npc(source_npc, plan, interner)?;
    validate_source_achr(source_achr, plan, interner)?;
    validate_donor_npc(donor_npc, plan, interner)?;

    let npc = project_npc(donor_npc, plan, interner);
    let voice_type = project_voice(plan, interner);
    let placed_actor = project_actor(plan, interner);
    let facegen_copy = facegen_copy(plan, interner);
    let receipt = projection_receipt(plan, facegen_copy.geometry.clone());
    let projection = TargetNativeHumanoidProjection {
        npc,
        voice_type,
        placed_actor,
        target_cell: plan.target_cell,
        facegen_copy,
        receipt,
    };
    validate_target_native_humanoid_projection(&projection, plan, interner)?;
    Ok(projection)
}

pub(crate) fn validate_target_native_humanoid_projection(
    projection: &TargetNativeHumanoidProjection,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), NpcContractError> {
    validate_humanoid_plan(plan, interner)?;
    validate_projected_npc(&projection.npc, plan, interner)?;
    validate_projected_voice(&projection.voice_type, plan, interner)?;
    validate_projected_actor(&projection.placed_actor, plan, interner)?;
    if projection.target_cell != plan.target_cell {
        return Err(NpcContractError::InvalidProjection(
            "projection points at a CELL other than the planned target",
        ));
    }
    let expected_facegen = facegen_copy(plan, interner);
    if projection.facegen_copy != expected_facegen
        || projection.facegen_copy.tint.is_some()
        || !projection.facegen_copy.geometry.required
    {
        return Err(NpcContractError::InvalidProjection(
            "FaceGeom receipt differs from the plan or requests unsupported FaceTint",
        ));
    }
    if projection.receipt != projection_receipt(plan, expected_facegen.geometry) {
        return Err(NpcContractError::InvalidProjection(
            "source mappings, topology, or asset receipt differs from the plan",
        ));
    }
    Ok(())
}

fn projection_receipt(
    plan: &TargetNativeHumanoidPlan,
    facegeom: TargetNativeFacegenAssetCopy,
) -> HumanoidProjectionReceipt {
    HumanoidProjectionReceipt {
        mappings: vec![
            HumanoidFormMapping {
                source: plan.source_npc,
                target: plan.target_npc,
            },
            HumanoidFormMapping {
                source: plan.source_placed_actor,
                target: plan.target_placed_actor,
            },
            HumanoidFormMapping {
                source: plan.source_voice_type,
                target: plan.target_voice.form_key,
            },
        ],
        topology: HumanoidTopologyReceipt {
            source_placed_actor: plan.source_placed_actor,
            source_cell: plan.source_cell,
            target_cell: plan.target_cell,
            target_cell_editor_id: plan.target_cell_editor_id.clone(),
            placed_actor: plan.target_placed_actor,
            persistent: true,
            child_group_type: plan.persistent_child_group_type,
            target_coc_marker: plan.target_coc_marker,
            target_navmesh_evidence: plan.target_navmesh_evidence,
        },
        facegeom,
    }
}

fn validate_humanoid_plan(
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), NpcContractError> {
    let output_plugin = plan.target_npc.plugin;
    if plan.target_placed_actor.plugin != output_plugin
        || plan.target_voice.form_key.plugin != output_plugin
    {
        return Err(NpcContractError::InvalidTarget(
            "target NPC, ACHR, and synthesized VTYP must share the output plugin",
        ));
    }
    if plan.target_npc == plan.target_placed_actor
        || plan.target_npc == plan.target_voice.form_key
        || plan.target_placed_actor == plan.target_voice.form_key
    {
        return Err(NpcContractError::InvalidTarget(
            "target NPC, ACHR, and VTYP must have distinct allocated FormKeys",
        ));
    }
    let keys = [
        plan.source_npc,
        plan.source_placed_actor,
        plan.source_cell,
        plan.source_race,
        plan.source_voice_type,
        plan.donor_npc,
        plan.donor_voice_type,
        plan.donor_race,
        plan.donor_class,
        plan.donor_package,
        plan.donor_outfit,
        plan.target_npc,
        plan.target_placed_actor,
        plan.target_cell,
        plan.target_voice.form_key,
        plan.target_race,
        plan.target_class,
        plan.target_outfit,
        plan.target_package,
        plan.target_combat_style,
        plan.target_coc_marker,
        plan.target_navmesh_evidence,
    ];
    if keys.iter().any(|key| {
        key.local == 0
            || key.local > 0x00FF_FFFF
            || interner.resolve(key.plugin).is_none_or(str::is_empty)
    }) {
        return Err(NpcContractError::InvalidTarget(
            "plan contains a null, out-of-range, or unresolved FormKey",
        ));
    }
    if plan.npc_editor_id.is_empty()
        || plan.placed_actor_editor_id.is_empty()
        || plan.display_name.is_empty()
        || plan.short_name.is_empty()
        || plan.donor_editor_id.is_empty()
        || plan.target_voice.editor_id.is_empty()
        || plan.target_cell_editor_id.is_empty()
        || plan.facegeom_source_archive.is_empty()
    {
        return Err(NpcContractError::InvalidTarget(
            "plan identity, topology, and FaceGeom provenance strings must be non-empty",
        ));
    }
    if plan.persistent_child_group_type != PERSISTENT_CELL_CHILD_GROUP_TYPE {
        return Err(NpcContractError::InvalidTarget(
            "FO4 persistent interior placement requires CELL child group type 8",
        ));
    }
    let source_plugins = [
        plan.source_npc.plugin,
        plan.source_placed_actor.plugin,
        plan.source_cell.plugin,
        plan.source_race.plugin,
        plan.source_voice_type.plugin,
    ];
    if [
        plan.target_cell,
        plan.target_race,
        plan.target_class,
        plan.target_outfit,
        plan.target_package,
        plan.target_combat_style,
        plan.target_coc_marker,
        plan.target_navmesh_evidence,
    ]
    .iter()
    .any(|key| source_plugins.contains(&key.plugin))
    {
        return Err(NpcContractError::InvalidTarget(
            "target-native records and placement evidence cannot point into a source plugin",
        ));
    }
    Ok(())
}

fn validate_source_npc(
    record: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), NpcContractError> {
    require_exact_record(
        record,
        "NPC_",
        plan.source_npc,
        &plan.npc_editor_id,
        interner,
    )
    .map_err(NpcContractError::InvalidSource)?;
    require_source_name(record, plan, interner).map_err(NpcContractError::InvalidSource)?;
    require_exact_form_key(record, b"RNAM", plan.source_race)
        .map_err(NpcContractError::InvalidSource)?;
    require_exact_form_key(record, b"VTCK", plan.source_voice_type)
        .map_err(NpcContractError::InvalidSource)?;
    let acbs = require_bytes(record, b"ACBS").map_err(NpcContractError::InvalidSource)?;
    let (level_offset, minimum_offset) = match acbs.len() {
        24 => (8, 10),
        20 => (6, 8),
        _ => {
            return Err(NpcContractError::InvalidSource(
                "source ACBS is neither the source nor relaid target width",
            ));
        }
    };
    if read_u32(acbs, 0).is_none_or(|flags| {
        flags & plan.source_required_acbs_flags != plan.source_required_acbs_flags
    }) || read_u16(acbs, level_offset) != Some(plan.source_level)
        || read_u16(acbs, minimum_offset) != Some(plan.source_minimum_level)
    {
        return Err(NpcContractError::InvalidSource(
            "source ACBS differs from the semantic capability plan",
        ));
    }
    Ok(())
}

fn validate_source_achr(
    record: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), NpcContractError> {
    require_exact_record(
        record,
        "ACHR",
        plan.source_placed_actor,
        &plan.placed_actor_editor_id,
        interner,
    )
    .map_err(NpcContractError::InvalidSource)?;
    if !record.flags.contains(RecordFlags::PERSISTENT) {
        return Err(NpcContractError::InvalidSource(
            "source placed actor is not persistent",
        ));
    }
    require_exact_form_key(record, b"NAME", plan.source_npc)
        .map_err(NpcContractError::InvalidSource)?;
    let transform =
        decode_transform(require_bytes(record, b"DATA").map_err(NpcContractError::InvalidSource)?)
            .ok_or(NpcContractError::InvalidSource(
                "source placed actor DATA is not a six-float transform",
            ))?;
    if !approx_transform(transform.0, plan.source_placement.position)
        || !approx_transform(transform.1, plan.source_placement.rotation)
    {
        return Err(NpcContractError::InvalidSource(
            "source placed actor transform differs from the plan evidence",
        ));
    }
    Ok(())
}

fn validate_donor_npc(
    record: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), NpcContractError> {
    require_exact_record(
        record,
        "NPC_",
        plan.donor_npc,
        &plan.donor_editor_id,
        interner,
    )
    .map_err(NpcContractError::InvalidDonor)?;
    if !record.warnings.is_empty() {
        return Err(NpcContractError::InvalidDonor(
            "donor was decoded with warnings",
        ));
    }
    for (signature, expected) in [
        (b"VTCK", plan.donor_voice_type),
        (b"RNAM", plan.donor_race),
        (b"CNAM", plan.donor_class),
        (b"PKID", plan.donor_package),
        (b"DOFT", plan.donor_outfit),
    ] {
        require_exact_form_key(record, signature, expected)
            .map_err(NpcContractError::InvalidDonor)?;
    }
    let acbs = require_bytes(record, b"ACBS").map_err(NpcContractError::InvalidDonor)?;
    if acbs.len() != 20
        || read_u32(acbs, 0) != Some(plan.donor_acbs_flags)
        || read_u16(acbs, 6) != Some(plan.donor_level)
    {
        return Err(NpcContractError::InvalidDonor(
            "donor ACBS differs from the audited donor receipt",
        ));
    }
    validate_face_fields(record).map_err(NpcContractError::InvalidDonor)
}

fn project_npc(
    donor: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Record {
    let mut npc = donor.clone();
    npc.form_key = plan.target_npc;
    set_editor_id(&mut npc, &plan.npc_editor_id, interner);
    npc.fields
        .retain(|entry| !DROPPED_NPC_FIELDS.contains(&entry.sig.0));
    replace_single(&mut npc, b"ACBS", FieldValue::Bytes(planned_acbs(plan)));
    replace_single(
        &mut npc,
        b"VTCK",
        FieldValue::FormKey(plan.target_voice.form_key),
    );
    replace_single(&mut npc, b"RNAM", FieldValue::FormKey(plan.target_race));
    replace_single(
        &mut npc,
        b"AIDT",
        FieldValue::Bytes(SmallVec::from_slice(&plan.target_aidt)),
    );
    replace_single(&mut npc, b"CNAM", FieldValue::FormKey(plan.target_class));
    replace_single(
        &mut npc,
        b"FULL",
        FieldValue::String(interner.intern(&plan.display_name)),
    );
    if count_fields(&npc, b"SHRT") == 1 {
        replace_single(
            &mut npc,
            b"SHRT",
            FieldValue::String(interner.intern(&plan.short_name)),
        );
    }
    replace_single(&mut npc, b"DOFT", FieldValue::FormKey(plan.target_outfit));
    insert_before(
        &mut npc,
        b"CNAM",
        field("PKID", FieldValue::FormKey(plan.target_package)),
    );
    insert_before(
        &mut npc,
        b"NAM5",
        field("ZNAM", FieldValue::FormKey(plan.target_combat_style)),
    );
    npc
}

fn project_voice(plan: &TargetNativeHumanoidPlan, interner: &StringInterner) -> Record {
    let mut voice = Record::new(sig("VTYP"), plan.target_voice.form_key);
    set_editor_id(&mut voice, &plan.target_voice.editor_id, interner);
    voice
        .fields
        .push(field("DNAM", FieldValue::Uint(plan.target_voice.flags)));
    voice
}

fn project_actor(plan: &TargetNativeHumanoidPlan, interner: &StringInterner) -> Record {
    let mut actor = Record::new(sig("ACHR"), plan.target_placed_actor);
    actor.flags = RecordFlags::PERSISTENT;
    set_editor_id(&mut actor, &plan.placed_actor_editor_id, interner);
    actor
        .fields
        .push(field("NAME", FieldValue::FormKey(plan.target_npc)));
    actor.fields.push(field(
        "DATA",
        FieldValue::Bytes(encode_transform(
            plan.target_placement.position,
            plan.target_placement.rotation,
        )),
    ));
    actor
}

fn facegen_copy(
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> TargetNativeFacegenCopy {
    let source_plugin = interner
        .resolve(plan.donor_npc.plugin)
        .expect("validated donor plugin is interned")
        .to_owned();
    let target_plugin = interner
        .resolve(plan.target_npc.plugin)
        .expect("validated target plugin is interned")
        .to_owned();
    TargetNativeFacegenCopy {
        alias: FacegenAlias {
            source_plugin: source_plugin.clone(),
            source_local: plan.donor_npc.local,
            target_plugin: target_plugin.clone(),
            target_local: plan.target_npc.local,
        },
        geometry: TargetNativeFacegenAssetCopy {
            source_archive: plan.facegeom_source_archive.clone(),
            source_relative_path: facegen_geometry_path(&source_plugin, plan.donor_npc.local),
            target_relative_path: facegen_geometry_path(&target_plugin, plan.target_npc.local),
            required: true,
        },
        tint: None,
    }
}

fn validate_projected_npc(
    npc: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), NpcContractError> {
    require_exact_record(npc, "NPC_", plan.target_npc, &plan.npc_editor_id, interner)
        .map_err(NpcContractError::InvalidProjection)?;
    require_string(npc, b"FULL", &plan.display_name, interner)
        .map_err(NpcContractError::InvalidProjection)?;
    for (signature, expected) in [
        (b"VTCK", plan.target_voice.form_key),
        (b"RNAM", plan.target_race),
        (b"CNAM", plan.target_class),
        (b"DOFT", plan.target_outfit),
        (b"PKID", plan.target_package),
        (b"ZNAM", plan.target_combat_style),
    ] {
        require_exact_form_key(npc, signature, expected)
            .map_err(NpcContractError::InvalidProjection)?;
    }
    if DROPPED_NPC_FIELDS
        .iter()
        .filter(|signature| **signature != *b"PKID")
        .any(|signature| count_fields(npc, signature) != 0)
        || count_fields(npc, b"PKID") != 1
    {
        return Err(NpcContractError::InvalidProjection(
            "unsupported source or donor behavior dependencies survived projection",
        ));
    }
    if require_bytes(npc, b"ACBS").map_err(NpcContractError::InvalidProjection)?
        != planned_acbs(plan).as_slice()
        || require_bytes(npc, b"AIDT").map_err(NpcContractError::InvalidProjection)?
            != plan.target_aidt
    {
        return Err(NpcContractError::InvalidProjection(
            "projected NPC stats or AI differ from the plan",
        ));
    }
    validate_face_fields(npc).map_err(NpcContractError::InvalidProjection)?;
    let source_plugins = [
        plan.source_npc.plugin,
        plan.source_race.plugin,
        plan.source_voice_type.plugin,
        plan.source_cell.plugin,
    ];
    if npc.fields.iter().any(|entry| {
        value_contains_source_dependency(&entry.value, plan.target_voice.form_key, &source_plugins)
    }) {
        return Err(NpcContractError::InvalidProjection(
            "projected NPC retains an unsupported source-plugin dependency",
        ));
    }
    Ok(())
}

fn validate_projected_voice(
    voice: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), NpcContractError> {
    require_exact_record(
        voice,
        "VTYP",
        plan.target_voice.form_key,
        &plan.target_voice.editor_id,
        interner,
    )
    .map_err(NpcContractError::InvalidProjection)?;
    if voice.flags != RecordFlags::empty()
        || voice.fields.len() != 2
        || field_value(voice, b"DNAM") != Some(&FieldValue::Uint(plan.target_voice.flags))
    {
        return Err(NpcContractError::InvalidProjection(
            "synthesized VTYP differs from the planned strategy",
        ));
    }
    Ok(())
}

fn validate_projected_actor(
    actor: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), NpcContractError> {
    require_exact_record(
        actor,
        "ACHR",
        plan.target_placed_actor,
        &plan.placed_actor_editor_id,
        interner,
    )
    .map_err(NpcContractError::InvalidProjection)?;
    if actor.flags != RecordFlags::PERSISTENT
        || actor.fields.len() != 3
        || field_value(actor, b"NAME") != Some(&FieldValue::FormKey(plan.target_npc))
        || decode_transform(
            require_bytes(actor, b"DATA").map_err(NpcContractError::InvalidProjection)?,
        ) != Some((
            plan.target_placement.position,
            plan.target_placement.rotation,
        ))
    {
        return Err(NpcContractError::InvalidProjection(
            "persistent placed actor differs from the plan",
        ));
    }
    Ok(())
}

fn validate_face_fields(record: &Record) -> Result<(), &'static str> {
    for signature in REQUIRED_DONOR_FACE_FIELDS {
        if count_fields(record, signature) != 1 {
            return Err("donor is missing a required FO4 head, tint, or morph field");
        }
    }
    if count_fields(record, b"PNAM") == 0
        || count_fields(record, b"TETI") != count_fields(record, b"TEND")
        || count_fields(record, b"FMRI") != count_fields(record, b"FMRS")
        || count_fields(record, b"FMRI") == 0
    {
        return Err("donor has an incomplete head-part, tint, or face-morph pair");
    }
    Ok(())
}

fn require_exact_record(
    record: &Record,
    signature: &str,
    form_key: FormKey,
    editor_id: &str,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    if record.sig.as_str() != signature || record.form_key != form_key {
        return Err("record signature or FormKey differs from the plan");
    }
    if record.eid.and_then(|eid| interner.resolve(eid)) != Some(editor_id)
        || count_fields(record, b"EDID") != 1
        || !matches!(
            field_value(record, b"EDID"),
            Some(FieldValue::String(value)) if interner.resolve(*value) == Some(editor_id)
        )
    {
        return Err("record EditorID differs from the plan");
    }
    Ok(())
}

fn require_exact_form_key(
    record: &Record,
    signature: &[u8; 4],
    expected: FormKey,
) -> Result<(), &'static str> {
    if count_fields(record, signature) != 1
        || field_value(record, signature) != Some(&FieldValue::FormKey(expected))
    {
        return Err("FormKey edge differs from the plan");
    }
    Ok(())
}

fn require_source_name(
    record: &Record,
    plan: &TargetNativeHumanoidPlan,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    match field_value(record, b"FULL") {
        Some(FieldValue::String(value)) if interner.resolve(*value) == Some(&plan.display_name) => {
            Ok(())
        }
        Some(FieldValue::Uint(value))
            if plan.source_full_name_lstring_id == u32::try_from(*value).ok() =>
        {
            Ok(())
        }
        Some(FieldValue::Bytes(value))
            if plan.source_full_name_lstring_id.is_some()
                && read_u32(value, 0) == plan.source_full_name_lstring_id =>
        {
            Ok(())
        }
        _ => Err("source display name differs from the plan"),
    }
}

fn require_string(
    record: &Record,
    signature: &[u8; 4],
    expected: &str,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    if count_fields(record, signature) != 1
        || !matches!(
            field_value(record, signature),
            Some(FieldValue::String(value)) if interner.resolve(*value) == Some(expected)
        )
    {
        return Err("string field differs from the plan");
    }
    Ok(())
}

fn require_bytes<'a>(record: &'a Record, signature: &[u8; 4]) -> Result<&'a [u8], &'static str> {
    if count_fields(record, signature) != 1 {
        return Err("required byte field is missing or repeated");
    }
    match field_value(record, signature) {
        Some(FieldValue::Bytes(bytes)) => Ok(bytes),
        _ => Err("required field is not byte data"),
    }
}

fn field_value<'a>(record: &'a Record, signature: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *signature)
        .map(|entry| &entry.value)
}

fn count_fields(record: &Record, signature: &[u8; 4]) -> usize {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *signature)
        .count()
}

fn set_editor_id(record: &mut Record, editor_id: &str, interner: &StringInterner) {
    let value = interner.intern(editor_id);
    record.eid = Some(value);
    if let Some(edid) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"EDID")
    {
        edid.value = FieldValue::String(value);
    } else {
        record
            .fields
            .insert(0, field("EDID", FieldValue::String(value)));
    }
}

fn replace_single(record: &mut Record, signature: &[u8; 4], value: FieldValue) {
    if let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *signature)
    {
        entry.value = value;
    } else {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*signature),
            value,
        });
    }
}

fn insert_before(record: &mut Record, anchor: &[u8; 4], entry: FieldEntry) {
    let position = record
        .fields
        .iter()
        .position(|existing| existing.sig.0 == *anchor)
        .unwrap_or(record.fields.len());
    record.fields.insert(position, entry);
}

fn planned_acbs(plan: &TargetNativeHumanoidPlan) -> SmallVec<[u8; 32]> {
    let mut bytes = vec![0u8; 20];
    bytes[0..4].copy_from_slice(&plan.target_acbs_flags.to_le_bytes());
    bytes[6..8].copy_from_slice(&plan.target_level.to_le_bytes());
    bytes[8..10].copy_from_slice(&plan.target_minimum_level.to_le_bytes());
    bytes[12..14].copy_from_slice(&plan.target_disposition.to_le_bytes());
    SmallVec::from_vec(bytes)
}

fn encode_transform(position: [f32; 3], rotation: [f32; 3]) -> SmallVec<[u8; 32]> {
    let mut bytes = SmallVec::new();
    for value in position.into_iter().chain(rotation) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode_transform(bytes: &[u8]) -> Option<([f32; 3], [f32; 3])> {
    if bytes.len() != 24 {
        return None;
    }
    let mut values = [0.0f32; 6];
    for (index, value) in values.iter_mut().enumerate() {
        *value = f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().ok()?);
    }
    Some((
        [values[0], values[1], values[2]],
        [values[3], values[4], values[5]],
    ))
}

fn approx_transform(actual: [f32; 3], expected: [f32; 3]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(actual, expected)| (actual - expected).abs() <= 0.000_1)
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

fn value_contains_source_dependency(
    value: &FieldValue,
    allowed_voice: FormKey,
    source_plugins: &[Sym],
) -> bool {
    match value {
        FieldValue::FormKey(key) => *key != allowed_voice && source_plugins.contains(&key.plugin),
        FieldValue::List(values) => values
            .iter()
            .any(|value| value_contains_source_dependency(value, allowed_voice, source_plugins)),
        FieldValue::Struct(values) => values.iter().any(|(_, value)| {
            value_contains_source_dependency(value, allowed_voice, source_plugins)
        }),
        _ => false,
    }
}

fn facegen_geometry_path(plugin: &str, local: u32) -> String {
    format!("Meshes/Actors/Character/FaceGenData/FaceGeom/{plugin}/{local:08x}.nif")
}

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).expect("static subrecord signature is valid"),
        value,
    }
}

fn sig(signature: &str) -> SigCode {
    SigCode::from_str(signature).expect("static record signature is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_all_nonstandard_humanoid_face_topologies() {
        for editor_id in ["ArgonianRace", "ArgonianRaceVampire"] {
            assert_eq!(
                source_humanoid_face_kind(editor_id),
                Some(SkyrimHumanoidFaceKind::Argonian)
            );
        }
        for editor_id in ["KhajiitRace", "KhajiitRaceVampire"] {
            assert_eq!(
                source_humanoid_face_kind(editor_id),
                Some(SkyrimHumanoidFaceKind::Khajiit)
            );
        }
        for editor_id in ["DremoraRace", "DLC2DremoraRace"] {
            assert_eq!(
                source_humanoid_face_kind(editor_id),
                Some(SkyrimHumanoidFaceKind::Dremora)
            );
        }
        assert_eq!(
            source_humanoid_face_kind("NordRace"),
            Some(SkyrimHumanoidFaceKind::Human)
        );
        assert_eq!(
            source_humanoid_face_kind("NordRaceChild"),
            Some(SkyrimHumanoidFaceKind::Child)
        );
        assert_eq!(source_humanoid_face_kind("FalmerRace"), None);
    }

    const SOURCE_PLUGIN: &str = "SourceActors.esm";
    const TARGET_PLUGIN: &str = "ConvertedActors.esm";
    const DONOR_PLUGIN: &str = "Fallout4.esm";

    fn fk(interner: &StringInterner, local: u32, plugin: &str) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn identified_record(
        interner: &StringInterner,
        signature: &str,
        form_key: FormKey,
        editor_id: &str,
    ) -> Record {
        let mut record = Record::new(sig(signature), form_key);
        set_editor_id(&mut record, editor_id, interner);
        record
    }

    fn fixture_plan(interner: &StringInterner) -> TargetNativeHumanoidPlan {
        TargetNativeHumanoidPlan {
            source_npc: fk(interner, 0x100, SOURCE_PLUGIN),
            source_placed_actor: fk(interner, 0x101, SOURCE_PLUGIN),
            source_cell: fk(interner, 0x102, SOURCE_PLUGIN),
            source_race: fk(interner, 0x103, SOURCE_PLUGIN),
            source_voice_type: fk(interner, 0x104, SOURCE_PLUGIN),
            source_placement: TargetNativePlacement {
                position: [1.0, 2.0, 3.0],
                rotation: [0.0, 0.0, 1.0],
            },
            source_required_acbs_flags: 0x30,
            source_level: 12,
            source_minimum_level: 3,
            source_full_name_lstring_id: None,
            donor_npc: fk(interner, 0x200, DONOR_PLUGIN),
            donor_voice_type: fk(interner, 0x201, DONOR_PLUGIN),
            donor_race: fk(interner, 0x202, DONOR_PLUGIN),
            donor_class: fk(interner, 0x203, DONOR_PLUGIN),
            donor_package: fk(interner, 0x204, DONOR_PLUGIN),
            donor_outfit: fk(interner, 0x205, DONOR_PLUGIN),
            donor_acbs_flags: 0x230,
            donor_level: 9,
            target_npc: fk(interner, 0x300, TARGET_PLUGIN),
            target_placed_actor: fk(interner, 0x301, TARGET_PLUGIN),
            target_cell: fk(interner, 0x400, DONOR_PLUGIN),
            target_voice: TargetNativeVoicePlan {
                form_key: fk(interner, 0x302, TARGET_PLUGIN),
                editor_id: "ConvertedVoice".to_owned(),
                flags: 1,
            },
            target_race: fk(interner, 0x402, DONOR_PLUGIN),
            target_class: fk(interner, 0x403, DONOR_PLUGIN),
            target_outfit: fk(interner, 0x404, DONOR_PLUGIN),
            target_package: fk(interner, 0x405, DONOR_PLUGIN),
            target_combat_style: fk(interner, 0x406, DONOR_PLUGIN),
            target_placement: TargetNativePlacement {
                position: [128.0, -256.0, 0.0],
                rotation: [0.0, 0.0, 0.0],
            },
            target_acbs_flags: 0x30,
            target_level: 12,
            target_minimum_level: 3,
            target_disposition: 35,
            target_aidt: [0; 24],
            npc_editor_id: "ConvertedActor".to_owned(),
            placed_actor_editor_id: "ConvertedActorRef".to_owned(),
            display_name: "Converted Actor".to_owned(),
            short_name: "Actor".to_owned(),
            donor_editor_id: "AuditedHumanDonor".to_owned(),
            target_cell_editor_id: "AuditedInterior".to_owned(),
            persistent_child_group_type: 8,
            target_coc_marker: fk(interner, 0x407, DONOR_PLUGIN),
            target_navmesh_evidence: fk(interner, 0x408, DONOR_PLUGIN),
            facegeom_source_archive: "Fallout4 - Meshes.ba2".to_owned(),
        }
    }

    fn source_npc(plan: &TargetNativeHumanoidPlan, interner: &StringInterner) -> Record {
        let mut record = identified_record(interner, "NPC_", plan.source_npc, &plan.npc_editor_id);
        let mut acbs = vec![0u8; 24];
        acbs[0..4].copy_from_slice(&plan.source_required_acbs_flags.to_le_bytes());
        acbs[8..10].copy_from_slice(&plan.source_level.to_le_bytes());
        acbs[10..12].copy_from_slice(&plan.source_minimum_level.to_le_bytes());
        record.fields.extend([
            field("ACBS", FieldValue::Bytes(SmallVec::from_vec(acbs))),
            field("VTCK", FieldValue::FormKey(plan.source_voice_type)),
            field("RNAM", FieldValue::FormKey(plan.source_race)),
            field(
                "FULL",
                FieldValue::String(interner.intern(&plan.display_name)),
            ),
        ]);
        record
    }

    fn source_actor(plan: &TargetNativeHumanoidPlan, interner: &StringInterner) -> Record {
        let mut record = identified_record(
            interner,
            "ACHR",
            plan.source_placed_actor,
            &plan.placed_actor_editor_id,
        );
        record.flags = RecordFlags::PERSISTENT;
        record.fields.extend([
            field("NAME", FieldValue::FormKey(plan.source_npc)),
            field(
                "DATA",
                FieldValue::Bytes(encode_transform(
                    plan.source_placement.position,
                    plan.source_placement.rotation,
                )),
            ),
        ]);
        record
    }

    fn donor_npc(plan: &TargetNativeHumanoidPlan, interner: &StringInterner) -> Record {
        let mut record = identified_record(interner, "NPC_", plan.donor_npc, &plan.donor_editor_id);
        record.flags = RecordFlags::COMPRESSED;
        let mut acbs = vec![0u8; 20];
        acbs[0..4].copy_from_slice(&plan.donor_acbs_flags.to_le_bytes());
        acbs[6..8].copy_from_slice(&plan.donor_level.to_le_bytes());
        record.fields.extend([
            field("ACBS", FieldValue::Bytes(SmallVec::from_vec(acbs))),
            field("VTCK", FieldValue::FormKey(plan.donor_voice_type)),
            field("RNAM", FieldValue::FormKey(plan.donor_race)),
            field("CNAM", FieldValue::FormKey(plan.donor_class)),
            field("PKID", FieldValue::FormKey(plan.donor_package)),
            field("DOFT", FieldValue::FormKey(plan.donor_outfit)),
            field("AIDT", FieldValue::Bytes(SmallVec::from_vec(vec![0; 24]))),
            field("FULL", FieldValue::String(interner.intern("Audited Donor"))),
            field("SHRT", FieldValue::String(interner.intern("Donor"))),
        ]);
        for local in 0..2 {
            record.fields.push(field(
                "PNAM",
                FieldValue::FormKey(fk(interner, 0x500 + local, DONOR_PLUGIN)),
            ));
        }
        record.fields.extend([
            field(
                "HCLF",
                FieldValue::FormKey(fk(interner, 0x510, DONOR_PLUGIN)),
            ),
            field("NAM5", FieldValue::Bytes(SmallVec::from_slice(&[255, 0]))),
            field("NAM6", FieldValue::Float(1.0)),
            field("NAM4", FieldValue::Float(1.0)),
            field("MWGT", FieldValue::Bytes(SmallVec::from_vec(vec![0; 12]))),
            field("NAM8", FieldValue::Uint(0)),
            field(
                "FTST",
                FieldValue::FormKey(fk(interner, 0x511, DONOR_PLUGIN)),
            ),
            field("QNAM", FieldValue::Bytes(SmallVec::from_vec(vec![0; 16]))),
            field("MSDK", FieldValue::Bytes(SmallVec::from_vec(vec![0; 32]))),
            field("MSDV", FieldValue::Bytes(SmallVec::from_vec(vec![0; 32]))),
            field("TETI", FieldValue::Bytes(SmallVec::from_vec(vec![0; 4]))),
            field("TEND", FieldValue::Bytes(SmallVec::from_vec(vec![0; 8]))),
            field("MRSV", FieldValue::Bytes(SmallVec::from_vec(vec![0; 20]))),
            field("FMRI", FieldValue::Uint(0)),
            field("FMRS", FieldValue::Bytes(SmallVec::from_vec(vec![0; 28]))),
            field("FMIN", FieldValue::Float(3.0)),
        ]);
        record
    }

    fn projection(
        plan: &TargetNativeHumanoidPlan,
        interner: &StringInterner,
    ) -> TargetNativeHumanoidProjection {
        project_target_native_humanoid(
            &source_npc(plan, interner),
            &source_actor(plan, interner),
            &donor_npc(plan, interner),
            plan,
            interner,
        )
        .unwrap()
    }

    #[test]
    fn semantic_plan_produces_records_topology_mappings_and_geometry_receipt() {
        let interner = StringInterner::new();
        let plan = fixture_plan(&interner);
        let projection = projection(&plan, &interner);

        validate_target_native_humanoid_projection(&projection, &plan, &interner).unwrap();
        assert_eq!(projection.receipt.mappings.len(), 3);
        assert_eq!(projection.receipt.topology.target_cell, plan.target_cell);
        assert_eq!(projection.receipt.topology.child_group_type, 8);
        assert!(projection.receipt.topology.persistent);
        assert!(projection.facegen_copy.geometry.required);
        assert!(projection.facegen_copy.tint.is_none());
        assert_eq!(
            projection.facegen_copy.geometry.source_relative_path,
            "Meshes/Actors/Character/FaceGenData/FaceGeom/Fallout4.esm/00000200.nif"
        );
        assert_eq!(
            projection.facegen_copy.geometry.target_relative_path,
            "Meshes/Actors/Character/FaceGenData/FaceGeom/ConvertedActors.esm/00000300.nif"
        );
    }

    #[test]
    fn near_miss_source_donor_and_topology_evidence_are_terminal() {
        let interner = StringInterner::new();
        let plan = fixture_plan(&interner);
        let mut source = source_actor(&plan, &interner);
        replace_single(
            &mut source,
            b"NAME",
            FieldValue::FormKey(fk(&interner, 0x999, SOURCE_PLUGIN)),
        );
        assert!(matches!(
            project_target_native_humanoid(
                &source_npc(&plan, &interner),
                &source,
                &donor_npc(&plan, &interner),
                &plan,
                &interner,
            ),
            Err(NpcContractError::InvalidSource(_))
        ));

        let mut donor = donor_npc(&plan, &interner);
        donor.form_key.local += 1;
        assert!(matches!(
            project_target_native_humanoid(
                &source_npc(&plan, &interner),
                &source_actor(&plan, &interner),
                &donor,
                &plan,
                &interner,
            ),
            Err(NpcContractError::InvalidDonor(_))
        ));

        let mut invalid_plan = plan.clone();
        invalid_plan.persistent_child_group_type = 9;
        assert!(matches!(
            project_target_native_humanoid(
                &source_npc(&invalid_plan, &interner),
                &source_actor(&invalid_plan, &interner),
                &donor_npc(&invalid_plan, &interner),
                &invalid_plan,
                &interner,
            ),
            Err(NpcContractError::InvalidTarget(_))
        ));

        let mut colliding_plan = plan.clone();
        colliding_plan.target_voice.form_key = colliding_plan.target_npc;
        assert!(matches!(
            project_target_native_humanoid(
                &source_npc(&colliding_plan, &interner),
                &source_actor(&colliding_plan, &interner),
                &donor_npc(&colliding_plan, &interner),
                &colliding_plan,
                &interner,
            ),
            Err(NpcContractError::InvalidTarget(_))
        ));
    }

    #[test]
    fn missing_face_data_and_fabricated_tint_are_terminal_after_reopen() {
        let interner = StringInterner::new();
        let plan = fixture_plan(&interner);
        let mut donor = donor_npc(&plan, &interner);
        donor.fields.retain(|entry| entry.sig.0 != *b"FMRS");
        assert!(matches!(
            project_target_native_humanoid(
                &source_npc(&plan, &interner),
                &source_actor(&plan, &interner),
                &donor,
                &plan,
                &interner,
            ),
            Err(NpcContractError::InvalidDonor(_))
        ));

        let mut reopened = projection(&plan, &interner);
        reopened.facegen_copy.tint = Some(reopened.facegen_copy.geometry.clone());
        assert!(matches!(
            validate_target_native_humanoid_projection(&reopened, &plan, &interner),
            Err(NpcContractError::InvalidProjection(_))
        ));
    }
}
