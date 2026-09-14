use std::collections::{BTreeMap, HashMap, HashSet};

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::source_read::{
    decode_record_from_parsed, iter_form_keys_of_sig, read_record_relayout_by_form_key,
    snapshot_records_by_form_keys,
};
use crate::sym::{StringInterner, Sym};
use crate::target_fo4_melee::{
    FO4_UNARMED_WEAPON_EQUIP_LOCAL, Fo4MeleeProfile, Fo4ResolvedMeleeSourcePayload,
    emit_fo4_resolved_melee_weapon, emit_fo4_resolved_melee_weapon_with_equip,
    emit_fo4_unarmed_weapon,
};
use crate::translator::{Decision, Game};
use esp_authoring_core::plugin_runtime::{ParsedRecord, effective_subrecords_for_record};

const SKYRIM_DNAM_WIDTH: usize = 100;
const FNV_DNAM_WIDTH: usize = 204;
const FNV_LEGACY_INTEGRATED_DNAM_WIDTH: usize = 196;
const FO3_DNAM_WIDTH: usize = 136;
const LEGACY_PROJECTILE_OFFSET: usize = 36;
const LEGACY_FLAGS_1_OFFSET: usize = 12;
const LEGACY_AUTOMATIC_FLAG: u8 = 0x02;
const LEGACY_EMBEDDED_WEAPON_FLAG: u8 = 0x20;
pub(crate) const BULK_MELEE_V1_POLICY: &str = "bulk_melee_v1";

const ONE_HAND_KEYWORDS: &[u32] = &[
    0x02_3465, 0x04_A0A4, 0x0C_F6B7, 0x09_3EB9, 0x10_C89B, 0x0F_4AEA,
];
const TWO_HAND_KEYWORDS: &[u32] = &[0x0E_B4A2, 0x04_A0A5, 0x10_C89B];

const ONE_HAND_PROFILE: Fo4MeleeProfile = Fo4MeleeProfile {
    block_impact: 0x0A_726B,
    block_material: 0x0A_726C,
    keywords: ONE_HAND_KEYWORDS,
    swing_sound: 0x03_C730,
    idle_sound: None,
    equip_sound: 0x24_98BB,
    unequip_sound: 0x15_26AC,
    impact: 0x01_3CAC,
    animation_type: 1,
    flags: 256,
    animation_attack_seconds: 1.117_955,
    action_point_cost: 30.0,
    stagger: 1,
    trailing_unknowns: [0, 0, 0, 0],
};

const TWO_HAND_PROFILE: Fo4MeleeProfile = Fo4MeleeProfile {
    block_impact: 0x0D_398C,
    block_material: 0x07_74C1,
    keywords: TWO_HAND_KEYWORDS,
    swing_sound: 0x09_4307,
    idle_sound: Some(0x24_989F),
    equip_sound: 0x0C_06CF,
    unequip_sound: 0x15_26AC,
    impact: 0x23_13F3,
    animation_type: 5,
    flags: 16 | 256,
    animation_attack_seconds: 1.764_111_9,
    action_point_cost: 20.0,
    stagger: 2,
    trailing_unknowns: [255, 255, 127, 127],
};

const CONTINUOUS_KEYWORDS: &[u32] = &[
    0x0D_20A6, 0x04_A0A4, 0x02_3465, 0x10_C89B, 0x11_FB88, 0x18_8A8F, 0x0C_F6B7, 0x0F_4AEA,
    0x21_3FDE, 0x22_5767,
];
const CONTINUOUS_PROFILE: Fo4MeleeProfile = Fo4MeleeProfile {
    block_impact: 0x0A_726B,
    block_material: 0x0D_525E,
    keywords: CONTINUOUS_KEYWORDS,
    swing_sound: 0x17_E5EE,
    idle_sound: Some(0x16_454D),
    equip_sound: 0x16_454C,
    unequip_sound: 0x15_26AC,
    impact: 0x17_E5EF,
    animation_type: 1,
    flags: 256 | 32_768,
    animation_attack_seconds: 1.102_227_6,
    action_point_cost: 40.0,
    stagger: 1,
    trailing_unknowns: [0, 0, 0, 0],
};

const POWER_FIST_KEYWORDS: &[u32] = &[
    0x02_405E, 0x05_240E, 0x12_3880, 0x10_C89B, 0x18_5D00, 0x1C_9E79, 0x0F_4AEA, 0x22_6453,
    0x23_0326,
];
const POWER_FIST_PROFILE: Fo4MeleeProfile = Fo4MeleeProfile {
    block_impact: 0x0A_726B,
    block_material: 0x07_74C1,
    keywords: POWER_FIST_KEYWORDS,
    swing_sound: 0x12_77AE,
    idle_sound: None,
    equip_sound: 0x1F_425D,
    unequip_sound: 0x15_26AC,
    impact: 0x21_DDD2,
    animation_type: 0,
    flags: 256,
    animation_attack_seconds: 1.117_951_2,
    action_point_cost: 35.0,
    stagger: 1,
    trailing_unknowns: [0, 0, 0, 0],
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceMeleeFamily {
    Skyrim,
    Fnv,
    Fo3,
}

impl SourceMeleeFamily {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Skyrim => "skyrim",
            Self::Fnv => "fnv",
            Self::Fo3 => "fo3",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fo4MeleeKind {
    UnarmedProxy,
    UnarmedGauntlet,
    OneHand,
    TwoHand,
    Continuous,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Fo4MeleeTargetProfile {
    UnarmedProxy,
    PowerFistGauntlet,
    MacheteOneHand,
    GrognakTwoHand,
    RipperContinuous,
}

impl Fo4MeleeTargetProfile {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::UnarmedProxy => "unarmed",
            Self::PowerFistGauntlet => "power_fist_unarmed",
            Self::MacheteOneHand => "machete_one_hand",
            Self::GrognakTwoHand => "grognak_two_hand",
            Self::RipperContinuous => "ripper_continuous",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MvpMeleeFirstPersonResolution {
    None,
    WnamStat,
    WorldModelFallback,
}

impl MvpMeleeFirstPersonResolution {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::WnamStat => "wnam_stat",
            Self::WorldModelFallback => "world_model_fallback",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MeleeLoweringReason {
    Enchantment,
    Script,
    LegacyWeaponMods,
    TemplateFlattened,
    ContinuousTwoHandGrip,
}

impl MeleeLoweringReason {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Enchantment => "enchantment",
            Self::Script => "script",
            Self::LegacyWeaponMods => "legacy_weapon_mods",
            Self::TemplateFlattened => "template_flattened",
            Self::ContinuousTwoHandGrip => "continuous_two_hand_grip",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MeleeRejectionReason {
    Deleted,
    MissingEditorId,
    DnamMissing,
    DnamMultiplicity,
    DnamWidth(usize),
    ProvenanceMismatch,
    RangedAnimation(u32),
    ThrowingAnimation(u32),
    UnknownAnimation(u32),
    AmmoContradiction,
    ProjectileContradiction,
    GunEquipmentContradiction,
    TemplateMultiplicity,
    TemplateCycle,
    UnresolvedTemplate(FormKey),
    FieldMultiplicity(&'static str),
    InvalidStats,
    MissingWorldModel,
    MissingFirstPersonModel,
    UnresolvedFirstPersonModel(FormKey),
}

impl MeleeRejectionReason {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Deleted => "deleted",
            Self::MissingEditorId => "missing_editor_id",
            Self::DnamMissing => "dnam_missing",
            Self::DnamMultiplicity => "dnam_multiplicity",
            Self::DnamWidth(_) => "dnam_width",
            Self::ProvenanceMismatch => "provenance_mismatch",
            Self::RangedAnimation(_) => "ranged_animation",
            Self::ThrowingAnimation(_) => "throwing_animation",
            Self::UnknownAnimation(_) => "unknown_animation",
            Self::AmmoContradiction => "ammo_contradiction",
            Self::ProjectileContradiction => "projectile_contradiction",
            Self::GunEquipmentContradiction => "gun_equipment_contradiction",
            Self::TemplateMultiplicity => "template_multiplicity",
            Self::TemplateCycle => "template_cycle",
            Self::UnresolvedTemplate(_) => "unresolved_template",
            Self::FieldMultiplicity(_) => "field_multiplicity",
            Self::InvalidStats => "invalid_stats",
            Self::MissingWorldModel => "missing_world_model",
            Self::MissingFirstPersonModel => "missing_first_person_model",
            Self::UnresolvedFirstPersonModel(_) => "unresolved_first_person_model",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MeleeProjection {
    pub source_form_key: FormKey,
    pub family: SourceMeleeFamily,
    pub kind: Fo4MeleeKind,
    pub first_person_resolution: MvpMeleeFirstPersonResolution,
    pub lowered: Vec<MeleeLoweringReason>,
    pub record: Record,
}

#[derive(Clone, Debug)]
pub(crate) struct MeleeRejection {
    pub source_form_key: FormKey,
    pub reason: MeleeRejectionReason,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MvpMeleeDisposition {
    Admitted,
    Rejected(MeleeRejectionReason),
}

impl MvpMeleeDisposition {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Admitted => "admitted",
            Self::Rejected(_) => "rejected",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MvpMeleeReceipt {
    pub source_form_key: FormKey,
    pub source_family: Option<SourceMeleeFamily>,
    pub disposition: MvpMeleeDisposition,
    pub policy: &'static str,
    pub target_profile: Option<Fo4MeleeTargetProfile>,
    pub world_model: Option<Sym>,
    pub first_person_model: Option<Sym>,
    pub first_person_resolution: MvpMeleeFirstPersonResolution,
    pub drops: Vec<MeleeLoweringReason>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MeleeCorpusPlan {
    pub candidates: usize,
    pub projections: Vec<MeleeProjection>,
    pub rejections: Vec<MeleeRejection>,
    pub receipts: Vec<MvpMeleeReceipt>,
}

#[derive(Clone, Debug)]
pub(crate) struct MvpMeleeSourceRecord {
    pub record: Record,
    pub raw_dnam: Vec<Vec<u8>>,
    pub template_refs: Vec<FormKey>,
    pub has_ammo: bool,
}

pub struct MvpMeleePhase;

impl Phase for MvpMeleePhase {
    fn name(&self) -> &'static str {
        "mvp_melee"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        if ctx.run.target != Game::Fo4
            || !matches!(
                ctx.run.source,
                Game::Skyrim | Game::SkyrimSe | Game::Fnv | Game::Fo3
            )
        {
            return Ok(PhaseReport::default());
        }
        ctx.run.clear_bulk_melee_v1_receipts();
        if ctx.run.mapper_state.is_none() {
            return Err(internal(
                "mvp_melee requires mapper_state and must run after record translation",
            ));
        }

        let (weapons, stat_models) = read_source_corpus(ctx)?;
        let mut plan = plan_bulk_melee_v1(weapons, &stat_models, &ctx.run.interner);
        let candidate_count = plan.receipts.len();
        let admitted_count = plan
            .receipts
            .iter()
            .filter(|receipt| receipt.disposition == MvpMeleeDisposition::Admitted)
            .count();
        let rejected_count = candidate_count - admitted_count;
        if !validate_bulk_melee_v1_plan(&plan, &ctx.run.interner) {
            return Err(internal(
                "bulk_melee_v1 produced an internally inconsistent receipt plan",
            ));
        }

        allocate_projection_form_keys(ctx, &mut plan.projections)?;
        for projection in &plan.projections {
            ctx.check_cancel()?;
            crate::target_write::replace_record_native(
                ctx.run.target_handle_id,
                projection.record.clone(),
                &ctx.run.schema_target,
                &ctx.run.interner,
            )
            .map_err(|error| {
                internal(format!(
                    "write projected melee {:06X}: {error}",
                    projection.source_form_key.local
                ))
            })?;
        }

        append_ledger(ctx, &plan);
        let message = format!(
            "mvp_melee: candidates={candidate_count} admitted={admitted_count} rejected={rejected_count} ranged_output=0"
        );
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "mvp_melee",
            level: LogLevel::Info,
            message,
        });
        ctx.run
            .commit_bulk_melee_v1_receipts(std::mem::take(&mut plan.receipts));

        Ok(PhaseReport {
            records_added: admitted_count.try_into().unwrap_or(u32::MAX),
            records_dropped: rejected_count.try_into().unwrap_or(u32::MAX),
            ..PhaseReport::default()
        })
    }
}

pub(crate) fn read_source_corpus(
    ctx: &PhaseCtx<'_>,
) -> Result<(Vec<MvpMeleeSourceRecord>, HashMap<FormKey, Sym>), PhaseError> {
    let weap_sig = SigCode::from_str("WEAP").map_err(internal)?;
    let weapon_keys = iter_form_keys_of_sig(ctx.run.source_handle_id, weap_sig, &ctx.run.interner)
        .map_err(|error| internal(format!("enumerate source WEAP records: {error}")))?;
    let batch =
        snapshot_records_by_form_keys(ctx.run.source_handle_id, &weapon_keys, &ctx.run.interner)
            .map_err(|error| internal(format!("snapshot source WEAP records: {error}")))?;

    let mut weapons = Vec::with_capacity(batch.records.len());
    for snapshot in &batch.records {
        let record = decode_record_from_parsed(
            &snapshot.raw_record,
            &snapshot.form_key,
            &ctx.run.schema_source,
            &batch.masters,
            &batch.plugin_name,
            batch.strings.as_ref(),
            batch.plugin_is_localized,
            &ctx.run.interner,
        )
        .map_err(|error| {
            internal(format!(
                "decode source WEAP {:06X}: {error}",
                snapshot.form_key.local
            ))
        })?;
        weapons.push(source_weapon(
            record,
            &snapshot.raw_record,
            &batch.masters,
            &batch.plugin_name,
            &ctx.run.interner,
        ));
    }

    let mut stat_keys = HashSet::new();
    for weapon in &weapons {
        for field in &weapon.record.fields {
            if field.sig.as_str() == "WNAM"
                && let Some(form_key) = form_key_value(&field.value)
                && form_key.local != 0
            {
                stat_keys.insert(form_key);
            }
        }
    }
    let mut stat_models = HashMap::new();
    for form_key in stat_keys {
        if let Ok(record) = read_record_relayout_by_form_key(
            ctx.run.source_handle_id,
            &form_key,
            &ctx.run.schema_source,
            &ctx.run.interner,
            None,
        ) && record.sig.as_str() == "STAT"
            && let Ok(Some(field)) = exact_field(&record, "MODL")
            && let FieldValue::String(model) = field.value
            && ctx
                .run
                .interner
                .resolve(model)
                .is_some_and(|path| !path.trim().is_empty())
        {
            stat_models.insert(form_key, model);
        }
    }
    Ok((weapons, stat_models))
}

fn source_weapon(
    record: Record,
    raw_record: &ParsedRecord,
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> MvpMeleeSourceRecord {
    let mut raw_dnam = Vec::new();
    let mut template_refs = Vec::new();
    let mut has_ammo = false;
    let subrecords = effective_subrecords_for_record(raw_record);
    for subrecord in subrecords.iter() {
        match subrecord.signature.as_str() {
            "DNAM" => raw_dnam.push(subrecord.data.to_vec()),
            "CNAM" if subrecord.data.len() == 4 => {
                let raw = u32::from_le_bytes(subrecord.data.as_ref().try_into().unwrap());
                if let Some(form_key) = raw_form_key(raw, masters, plugin_name, interner) {
                    template_refs.push(form_key);
                }
            }
            "CNAM" => template_refs.push(FormKey {
                local: 0,
                plugin: interner.intern("<invalid-template>"),
            }),
            "NAM0" | "AMMO" | "PROJ" => {
                has_ammo |= subrecord.data.iter().any(|byte| *byte != 0);
            }
            _ => {}
        }
    }
    if template_refs.is_empty()
        && let Ok(Some(field)) = exact_field(&record, "CNAM")
        && let Some(form_key) = form_key_value(&field.value)
        && form_key.local != 0
    {
        template_refs.push(form_key);
    }
    MvpMeleeSourceRecord {
        record,
        raw_dnam,
        template_refs,
        has_ammo,
    }
}

fn raw_form_key(
    raw: u32,
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let index = (raw >> 24) as usize;
    let plugin = if index < masters.len() {
        masters.get(index)?.as_str()
    } else if index == masters.len() || index == 0xFF {
        plugin_name
    } else {
        return None;
    };
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
    })
}

pub(crate) fn plan_bulk_melee_v1(
    weapons: Vec<MvpMeleeSourceRecord>,
    stat_models: &HashMap<FormKey, Sym>,
    interner: &StringInterner,
) -> MeleeCorpusPlan {
    let candidates = weapons.len();
    let by_form_key = weapons
        .iter()
        .enumerate()
        .map(|(index, weapon)| (weapon.record.form_key, index))
        .collect::<HashMap<_, _>>();
    let mut plan = MeleeCorpusPlan {
        candidates,
        ..MeleeCorpusPlan::default()
    };
    for weapon in &weapons {
        match project_weapon(weapon, &weapons, &by_form_key, stat_models, interner) {
            Ok(projection) => {
                plan.receipts.push(MvpMeleeReceipt {
                    source_form_key: projection.source_form_key,
                    source_family: Some(projection.family),
                    disposition: MvpMeleeDisposition::Admitted,
                    policy: BULK_MELEE_V1_POLICY,
                    target_profile: Some(target_profile_for_kind(projection.kind)),
                    world_model: projected_model(&projection.record, "MODL"),
                    first_person_model: projected_model(&projection.record, "MOD4"),
                    first_person_resolution: projection.first_person_resolution,
                    drops: projection.lowered.clone(),
                });
                plan.projections.push(projection);
            }
            Err(reason) => {
                plan.receipts.push(MvpMeleeReceipt {
                    source_form_key: weapon.record.form_key,
                    source_family: source_family_hint(weapon, &weapons, &by_form_key),
                    disposition: MvpMeleeDisposition::Rejected(reason.clone()),
                    policy: BULK_MELEE_V1_POLICY,
                    target_profile: None,
                    world_model: None,
                    first_person_model: None,
                    first_person_resolution: MvpMeleeFirstPersonResolution::None,
                    drops: Vec::new(),
                });
                plan.rejections.push(MeleeRejection {
                    source_form_key: weapon.record.form_key,
                    reason,
                });
            }
        }
    }
    plan
}

fn validate_bulk_melee_v1_plan(plan: &MeleeCorpusPlan, interner: &StringInterner) -> bool {
    if plan.candidates != plan.receipts.len()
        || plan.projections.len() + plan.rejections.len() != plan.candidates
    {
        return false;
    }
    let mut identities = HashSet::new();
    for receipt in &plan.receipts {
        if receipt.policy != BULK_MELEE_V1_POLICY || !identities.insert(receipt.source_form_key) {
            return false;
        }
        match &receipt.disposition {
            MvpMeleeDisposition::Admitted => {
                let Some(profile) = receipt.target_profile else {
                    return false;
                };
                if receipt.source_family.is_none()
                    || !plan.projections.iter().any(|projection| {
                        projection.source_form_key == receipt.source_form_key
                            && target_profile_for_kind(projection.kind) == profile
                            && emitted_animation_type(&projection.record, interner)
                                == Some(match profile {
                                    Fo4MeleeTargetProfile::UnarmedProxy
                                    | Fo4MeleeTargetProfile::PowerFistGauntlet => 0,
                                    Fo4MeleeTargetProfile::MacheteOneHand => 1,
                                    Fo4MeleeTargetProfile::GrognakTwoHand => 5,
                                    Fo4MeleeTargetProfile::RipperContinuous => 1,
                                })
                    })
                {
                    return false;
                }
                match receipt.first_person_resolution {
                    MvpMeleeFirstPersonResolution::None => {
                        if receipt.world_model.is_some() || receipt.first_person_model.is_some() {
                            return false;
                        }
                    }
                    MvpMeleeFirstPersonResolution::WnamStat => {
                        if receipt.first_person_model.is_none() {
                            return false;
                        }
                    }
                    MvpMeleeFirstPersonResolution::WorldModelFallback => {
                        if receipt.world_model.is_none()
                            || receipt.first_person_model != receipt.world_model
                        {
                            return false;
                        }
                    }
                }
                if profile != Fo4MeleeTargetProfile::UnarmedProxy
                    && receipt.first_person_resolution != MvpMeleeFirstPersonResolution::None
                    && (receipt.world_model.is_none() || receipt.first_person_model.is_none())
                {
                    return false;
                }
            }
            MvpMeleeDisposition::Rejected(reason) => {
                if receipt.target_profile.is_some()
                    || receipt.world_model.is_some()
                    || receipt.first_person_model.is_some()
                    || receipt.first_person_resolution != MvpMeleeFirstPersonResolution::None
                    || !receipt.drops.is_empty()
                    || !plan.rejections.iter().any(|rejection| {
                        rejection.source_form_key == receipt.source_form_key
                            && &rejection.reason == reason
                    })
                {
                    return false;
                }
            }
        }
    }
    true
}

fn emitted_animation_type(record: &Record, interner: &StringInterner) -> Option<u32> {
    let field = exact_field(record, "DNAM").ok()??;
    let FieldValue::Struct(fields) = &field.value else {
        return None;
    };
    named_u32(fields, "animation_type", interner)
}

fn source_family_hint(
    weapon: &MvpMeleeSourceRecord,
    weapons: &[MvpMeleeSourceRecord],
    by_form_key: &HashMap<FormKey, usize>,
) -> Option<SourceMeleeFamily> {
    let chain = inheritance_chain(weapon, weapons, by_form_key).ok()?;
    let dnam = inherited_dnam(&chain).ok()?;
    match dnam.len() {
        SKYRIM_DNAM_WIDTH => Some(SourceMeleeFamily::Skyrim),
        FNV_DNAM_WIDTH | FNV_LEGACY_INTEGRATED_DNAM_WIDTH => Some(SourceMeleeFamily::Fnv),
        FO3_DNAM_WIDTH => Some(SourceMeleeFamily::Fo3),
        _ => None,
    }
}

fn target_profile_for_kind(kind: Fo4MeleeKind) -> Fo4MeleeTargetProfile {
    match kind {
        Fo4MeleeKind::UnarmedProxy => Fo4MeleeTargetProfile::UnarmedProxy,
        Fo4MeleeKind::UnarmedGauntlet => Fo4MeleeTargetProfile::PowerFistGauntlet,
        Fo4MeleeKind::OneHand => Fo4MeleeTargetProfile::MacheteOneHand,
        Fo4MeleeKind::TwoHand => Fo4MeleeTargetProfile::GrognakTwoHand,
        Fo4MeleeKind::Continuous => Fo4MeleeTargetProfile::RipperContinuous,
    }
}

fn projected_model(record: &Record, signature: &str) -> Option<Sym> {
    record.fields.iter().find_map(|field| {
        (field.sig.as_str() == signature)
            .then_some(&field.value)
            .and_then(|value| match value {
                FieldValue::String(model) => Some(*model),
                _ => None,
            })
    })
}

fn project_weapon(
    weapon: &MvpMeleeSourceRecord,
    weapons: &[MvpMeleeSourceRecord],
    by_form_key: &HashMap<FormKey, usize>,
    stat_models: &HashMap<FormKey, Sym>,
    interner: &StringInterner,
) -> Result<MeleeProjection, MeleeRejectionReason> {
    if weapon.record.flags.contains(RecordFlags::DELETED) {
        return Err(MeleeRejectionReason::Deleted);
    }
    let chain = inheritance_chain(weapon, weapons, by_form_key)?;
    if has_gun_equipment_type(&chain, interner)? {
        return Err(MeleeRejectionReason::GunEquipmentContradiction);
    }
    if chain.iter().any(|source| source.has_ammo) {
        return Err(MeleeRejectionReason::AmmoContradiction);
    }
    let dnam = inherited_dnam(&chain)?;
    let family = classify_family(weapon.record.form_key, dnam, interner)?;
    let animation_type = source_animation_type(family, dnam);
    let source_kind = classify_melee_kind(family, animation_type)?;
    let source_two_hand = source_kind == Fo4MeleeKind::TwoHand;
    let mut kind = if matches!(source_kind, Fo4MeleeKind::OneHand | Fo4MeleeKind::TwoHand)
        && is_continuous_melee(family, dnam)
    {
        Fo4MeleeKind::Continuous
    } else {
        source_kind
    };

    if matches!(family, SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3)
        && dnam
            .get(LEGACY_PROJECTILE_OFFSET..LEGACY_PROJECTILE_OFFSET + 4)
            .is_some_and(|bytes| bytes.iter().any(|byte| *byte != 0))
    {
        return Err(MeleeRejectionReason::ProjectileContradiction);
    }

    let editor_id = exact_field(&weapon.record, "EDID")?
        .cloned()
        .ok_or(MeleeRejectionReason::MissingEditorId)?;
    let bounds = inherited_field(&chain, "OBND")?.cloned();
    let display_name = inherited_field(&chain, "FULL")?.cloned();
    let world_model = inherited_field(&chain, "MODL")?.cloned();
    let world_model_path = world_model
        .as_ref()
        .and_then(|field| source_model(field, interner));
    if world_model.is_some() && world_model_path.is_none() {
        return Err(MeleeRejectionReason::MissingWorldModel);
    }
    let first_person_ref = inherited_field(&chain, "WNAM")?
        .and_then(|field| form_key_value(&field.value))
        .filter(|form_key| form_key.local != 0);
    let (first_person_model, first_person_resolution) = match first_person_ref {
        Some(form_key) => (
            Some(
                stat_models
                    .get(&form_key)
                    .copied()
                    .filter(|model| {
                        interner
                            .resolve(*model)
                            .is_some_and(|path| !path.trim().is_empty())
                    })
                    .ok_or(MeleeRejectionReason::UnresolvedFirstPersonModel(form_key))?,
            ),
            MvpMeleeFirstPersonResolution::WnamStat,
        ),
        None => match world_model_path {
            Some(model) => (
                Some(model),
                MvpMeleeFirstPersonResolution::WorldModelFallback,
            ),
            None => (None, MvpMeleeFirstPersonResolution::None),
        },
    };
    if kind == Fo4MeleeKind::UnarmedProxy
        && (world_model_path.is_some() || first_person_model.is_some())
    {
        kind = Fo4MeleeKind::UnarmedGauntlet;
    }
    let integrated = is_integrated_melee(family, dnam);
    if kind != Fo4MeleeKind::UnarmedProxy && !integrated && world_model_path.is_none() {
        return Err(MeleeRejectionReason::MissingWorldModel);
    }
    if kind != Fo4MeleeKind::UnarmedProxy && !integrated && first_person_model.is_none() {
        return Err(MeleeRejectionReason::MissingFirstPersonModel);
    }

    let stats = source_stats(inherited_field(&chain, "DATA")?, family, interner)
        .ok_or(MeleeRejectionReason::InvalidStats)?;
    let payload = Fo4ResolvedMeleeSourcePayload {
        editor_id,
        bounds,
        display_name,
        world_model,
        first_person_model,
        value: stats.value,
        weight: stats.weight,
        damage: stats.damage,
    };
    let mut record = weapon.record.clone();
    match kind {
        Fo4MeleeKind::UnarmedProxy => emit_fo4_unarmed_weapon(&mut record, payload, interner),
        Fo4MeleeKind::UnarmedGauntlet => emit_fo4_resolved_melee_weapon_with_equip(
            &mut record,
            payload,
            POWER_FIST_PROFILE,
            FO4_UNARMED_WEAPON_EQUIP_LOCAL,
            interner,
        ),
        Fo4MeleeKind::OneHand => {
            emit_fo4_resolved_melee_weapon(&mut record, payload, ONE_HAND_PROFILE, interner)
        }
        Fo4MeleeKind::TwoHand => {
            emit_fo4_resolved_melee_weapon(&mut record, payload, TWO_HAND_PROFILE, interner)
        }
        Fo4MeleeKind::Continuous => {
            emit_fo4_resolved_melee_weapon(&mut record, payload, CONTINUOUS_PROFILE, interner)
        }
    }
    let mut lowered = lowering_reasons(&chain);
    if kind == Fo4MeleeKind::Continuous && source_two_hand {
        lowered.push(MeleeLoweringReason::ContinuousTwoHandGrip);
    }
    Ok(MeleeProjection {
        source_form_key: weapon.record.form_key,
        family,
        kind,
        first_person_resolution,
        lowered,
        record,
    })
}

fn has_gun_equipment_type(
    chain: &[&MvpMeleeSourceRecord],
    interner: &StringInterner,
) -> Result<bool, MeleeRejectionReason> {
    let Some(field) = inherited_field(chain, "ETYP")? else {
        return Ok(false);
    };
    Ok(match &field.value {
        FieldValue::String(value) => matches!(
            interner
                .resolve(*value)
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "bigguns" | "smallguns" | "energyweapons"
        ),
        FieldValue::Int(value) => (0..=2).contains(value),
        FieldValue::Uint(value) => *value <= 2,
        _ => false,
    })
}

fn source_model(field: &FieldEntry, interner: &StringInterner) -> Option<Sym> {
    let FieldValue::String(model) = &field.value else {
        return None;
    };
    interner
        .resolve(*model)
        .is_some_and(|path| !path.trim().is_empty())
        .then_some(*model)
}

fn inheritance_chain<'a>(
    weapon: &'a MvpMeleeSourceRecord,
    weapons: &'a [MvpMeleeSourceRecord],
    by_form_key: &HashMap<FormKey, usize>,
) -> Result<Vec<&'a MvpMeleeSourceRecord>, MeleeRejectionReason> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    let mut current = weapon;
    loop {
        if !seen.insert(current.record.form_key) {
            return Err(MeleeRejectionReason::TemplateCycle);
        }
        chain.push(current);
        let template = match current.template_refs.as_slice() {
            [] => break,
            [template] if template.local != 0 => *template,
            _ => return Err(MeleeRejectionReason::TemplateMultiplicity),
        };
        let Some(index) = by_form_key.get(&template).copied() else {
            return Err(MeleeRejectionReason::UnresolvedTemplate(template));
        };
        current = &weapons[index];
    }
    Ok(chain)
}

fn inherited_dnam<'a>(
    chain: &'a [&MvpMeleeSourceRecord],
) -> Result<&'a [u8], MeleeRejectionReason> {
    for source in chain {
        match source.raw_dnam.as_slice() {
            [] => {}
            [dnam] => return Ok(dnam),
            _ => return Err(MeleeRejectionReason::DnamMultiplicity),
        }
    }
    Err(MeleeRejectionReason::DnamMissing)
}

fn classify_family(
    source: FormKey,
    dnam: &[u8],
    interner: &StringInterner,
) -> Result<SourceMeleeFamily, MeleeRejectionReason> {
    let family = match dnam.len() {
        SKYRIM_DNAM_WIDTH => SourceMeleeFamily::Skyrim,
        FNV_DNAM_WIDTH | FNV_LEGACY_INTEGRATED_DNAM_WIDTH => SourceMeleeFamily::Fnv,
        FO3_DNAM_WIDTH => SourceMeleeFamily::Fo3,
        width => return Err(MeleeRejectionReason::DnamWidth(width)),
    };
    let owner = interner
        .resolve(source.plugin)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let owner_family = if owner.contains("skyrim") {
        Some(SourceMeleeFamily::Skyrim)
    } else if owner.contains("falloutnv") {
        Some(SourceMeleeFamily::Fnv)
    } else if owner.contains("fallout3") {
        Some(SourceMeleeFamily::Fo3)
    } else {
        None
    };
    let merged_fo3_graft =
        owner_family == Some(SourceMeleeFamily::Fnv) && family == SourceMeleeFamily::Fo3;
    if owner_family.is_some_and(|owner_family| owner_family != family) && !merged_fo3_graft {
        return Err(MeleeRejectionReason::ProvenanceMismatch);
    }
    Ok(family)
}

fn source_animation_type(family: SourceMeleeFamily, dnam: &[u8]) -> u32 {
    match family {
        SourceMeleeFamily::Skyrim => u32::from(dnam[0]),
        SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3 => {
            u32::from_le_bytes(dnam[0..4].try_into().unwrap())
        }
    }
}

fn is_continuous_melee(family: SourceMeleeFamily, dnam: &[u8]) -> bool {
    matches!(family, SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3)
        && dnam
            .get(LEGACY_FLAGS_1_OFFSET)
            .is_some_and(|flags| flags & LEGACY_AUTOMATIC_FLAG != 0)
}

fn is_integrated_melee(family: SourceMeleeFamily, dnam: &[u8]) -> bool {
    matches!(family, SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3)
        && dnam
            .get(LEGACY_FLAGS_1_OFFSET)
            .is_some_and(|flags| flags & LEGACY_EMBEDDED_WEAPON_FLAG != 0)
}

fn classify_melee_kind(
    family: SourceMeleeFamily,
    animation_type: u32,
) -> Result<Fo4MeleeKind, MeleeRejectionReason> {
    match family {
        SourceMeleeFamily::Skyrim => match animation_type {
            0 => Ok(Fo4MeleeKind::UnarmedProxy),
            1..=4 => Ok(Fo4MeleeKind::OneHand),
            5..=6 => Ok(Fo4MeleeKind::TwoHand),
            7..=9 => Err(MeleeRejectionReason::RangedAnimation(animation_type)),
            _ => Err(MeleeRejectionReason::UnknownAnimation(animation_type)),
        },
        SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3 => match animation_type {
            0 => Ok(Fo4MeleeKind::UnarmedProxy),
            1 => Ok(Fo4MeleeKind::OneHand),
            2 => Ok(Fo4MeleeKind::TwoHand),
            10..=13 => Err(MeleeRejectionReason::ThrowingAnimation(animation_type)),
            3..=9 => Err(MeleeRejectionReason::RangedAnimation(animation_type)),
            _ => Err(MeleeRejectionReason::UnknownAnimation(animation_type)),
        },
    }
}

fn inherited_field<'a>(
    chain: &'a [&MvpMeleeSourceRecord],
    signature: &'static str,
) -> Result<Option<&'a FieldEntry>, MeleeRejectionReason> {
    for source in chain {
        if let Some(field) = exact_field(&source.record, signature)? {
            return Ok(Some(field));
        }
    }
    Ok(None)
}

fn exact_field<'a>(
    record: &'a Record,
    signature: &'static str,
) -> Result<Option<&'a FieldEntry>, MeleeRejectionReason> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature);
    let first = fields.next();
    if fields.next().is_some() {
        return Err(MeleeRejectionReason::FieldMultiplicity(signature));
    }
    Ok(first)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SourceStats {
    value: u32,
    weight: f32,
    damage: u16,
}

fn source_stats(
    field: Option<&FieldEntry>,
    family: SourceMeleeFamily,
    interner: &StringInterner,
) -> Option<SourceStats> {
    let value = &field?.value;
    let stats = match (family, value) {
        (SourceMeleeFamily::Skyrim, FieldValue::Bytes(bytes)) if bytes.len() == 10 => SourceStats {
            value: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            weight: f32::from_le_bytes(bytes[4..8].try_into().ok()?),
            damage: u16::from_le_bytes(bytes[8..10].try_into().ok()?),
        },
        (SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3, FieldValue::Bytes(bytes))
            if bytes.len() == 15 =>
        {
            SourceStats {
                value: u32::try_from(i32::from_le_bytes(bytes[0..4].try_into().ok()?)).ok()?,
                weight: f32::from_le_bytes(bytes[8..12].try_into().ok()?),
                damage: u16::try_from(i16::from_le_bytes(bytes[12..14].try_into().ok()?)).ok()?,
            }
        }
        (SourceMeleeFamily::Skyrim, FieldValue::Struct(fields)) => SourceStats {
            value: named_u32(fields, "value", interner)?,
            weight: named_f32(fields, "weight", interner)?,
            damage: named_u16(fields, "damage", interner)?,
        },
        (SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3, FieldValue::Struct(fields)) => {
            SourceStats {
                value: named_u32(fields, "value", interner)?,
                weight: named_f32(fields, "weight", interner)?,
                damage: named_u16(fields, "base_damage", interner)?,
            }
        }
        _ => return None,
    };
    (stats.weight.is_finite() && stats.weight >= 0.0).then_some(stats)
}

fn lowering_reasons(chain: &[&MvpMeleeSourceRecord]) -> Vec<MeleeLoweringReason> {
    let mut reasons = BTreeMap::new();
    if chain.len() > 1 {
        reasons.insert(
            MeleeLoweringReason::TemplateFlattened,
            MeleeLoweringReason::TemplateFlattened,
        );
    }
    for source in chain {
        for field in &source.record.fields {
            let reason = match field.sig.as_str() {
                "EITM" | "EAMT" => Some(MeleeLoweringReason::Enchantment),
                "VMAD" | "SCRI" => Some(MeleeLoweringReason::Script),
                "WMI1" | "WMI2" | "WMI3" | "WNM1" | "WNM2" | "WNM3" | "WNM4" | "WNM5" | "WNM6"
                | "WNM7" | "MWD1" | "MWD2" | "MWD3" | "MWD4" | "MWD5" | "MWD6" | "MWD7" => {
                    Some(MeleeLoweringReason::LegacyWeaponMods)
                }
                _ => None,
            };
            if let Some(reason) = reason {
                reasons.insert(reason, reason);
            }
        }
    }
    reasons.into_values().collect()
}

fn allocate_projection_form_keys(
    ctx: &mut PhaseCtx<'_>,
    projections: &mut [MeleeProjection],
) -> Result<(), PhaseError> {
    let state = ctx
        .run
        .mapper_state
        .as_mut()
        .ok_or_else(|| internal("mvp_melee mapper_state disappeared"))?;
    let output_plugin = ctx.run.interner.intern(&state.options.output_plugin_name);
    let mut mapper = FormKeyMapper::from_state(state, &ctx.run.interner);
    let weap_sig = SigCode::from_str("WEAP").map_err(internal)?;
    for projection in projections {
        let source = projection.source_form_key;
        let mut target = mapper.allocate_or_resolve(source, None, weap_sig);
        if target.plugin != output_plugin {
            target = mapper.allocate_generated();
            mapper.add_mapping(source, target);
        }
        projection.record.form_key = target;
    }
    Ok(())
}

fn append_ledger(ctx: &mut PhaseCtx<'_>, plan: &MeleeCorpusPlan) {
    for receipt in &plan.receipts {
        for reason in &receipt.drops {
            ctx.run.decisions.push(Decision {
                kind: ctx
                    .run
                    .interner
                    .intern(&format!("mvp_melee_lowered_{}", reason.code())),
                message: format!("{:06X}:{}", receipt.source_form_key.local, reason.code()),
            });
        }
        if let MvpMeleeDisposition::Rejected(reason) = &receipt.disposition {
            ctx.run.decisions.push(Decision {
                kind: ctx
                    .run
                    .interner
                    .intern(&format!("mvp_melee_rejected_{}", reason.code())),
                message: format!(
                    "{:06X}:{}:{reason:?}",
                    receipt.source_form_key.local,
                    reason.code()
                ),
            });
        }
    }
}

fn form_key_value(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        _ => None,
    }
}

fn named_value<'a>(
    fields: &'a [(Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields.iter().find_map(|(field_name, value)| {
        interner
            .resolve(*field_name)
            .is_some_and(|field_name| field_name.eq_ignore_ascii_case(name))
            .then_some(value)
    })
}

fn named_u32(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<u32> {
    match named_value(fields, name, interner)? {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn named_u16(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<u16> {
    named_u32(fields, name, interner).and_then(|value| u16::try_from(value).ok())
}

fn named_f32(fields: &[(Sym, FieldValue)], name: &str, interner: &StringInterner) -> Option<f32> {
    match named_value(fields, name, interner)? {
        FieldValue::Float(value) => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: crate::ids::SubrecordSig::from_str(signature).expect("valid field signature"),
        value,
    }
}

fn internal(message: impl Into<String>) -> PhaseError {
    PhaseError::Internal(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AuthoringSchema;
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use smallvec::SmallVec;

    fn source_key(interner: &StringInterner, local: u32, plugin: &str) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn make_weapon(
        interner: &StringInterner,
        local: u32,
        plugin: &str,
        width: usize,
        animation_type: u32,
    ) -> MvpMeleeSourceRecord {
        let form_key = source_key(interner, local, plugin);
        let mut record = Record::new(SigCode::from_str("WEAP").unwrap(), form_key);
        let eid = interner.intern(&format!("TestWeapon{local:06X}"));
        record.eid = Some(eid);
        record.fields = SmallVec::from_vec(vec![
            field("EDID", FieldValue::String(eid)),
            field("OBND", FieldValue::Bytes(SmallVec::from_slice(&[0; 12]))),
            field("FULL", FieldValue::String(interner.intern("Test Weapon"))),
            field(
                "MODL",
                FieldValue::String(interner.intern("Weapons\\Test\\Weapon.nif")),
            ),
            field(
                "WNAM",
                FieldValue::FormKey(source_key(interner, local + 1, plugin)),
            ),
            field(
                "DATA",
                if width == SKYRIM_DNAM_WIDTH {
                    FieldValue::Bytes(SmallVec::from_slice(&[25, 0, 0, 0, 0, 0, 0, 64, 12, 0]))
                } else {
                    let mut bytes = [0_u8; 15];
                    bytes[0..4].copy_from_slice(&25_i32.to_le_bytes());
                    bytes[8..12].copy_from_slice(&2.0_f32.to_le_bytes());
                    bytes[12..14].copy_from_slice(&12_i16.to_le_bytes());
                    FieldValue::Bytes(SmallVec::from_slice(&bytes))
                },
            ),
        ]);
        let mut dnam = vec![0; width];
        if width == SKYRIM_DNAM_WIDTH {
            dnam[0] = u8::try_from(animation_type).unwrap();
        } else {
            dnam[0..4].copy_from_slice(&animation_type.to_le_bytes());
        }
        MvpMeleeSourceRecord {
            record,
            raw_dnam: vec![dnam],
            template_refs: Vec::new(),
            has_ammo: false,
        }
    }

    fn stat_models_for(weapons: &[MvpMeleeSourceRecord]) -> HashMap<FormKey, Sym> {
        weapons
            .iter()
            .filter_map(|weapon| {
                exact_field(&weapon.record, "WNAM")
                    .ok()
                    .flatten()
                    .and_then(|field| form_key_value(&field.value))
                    .zip(
                        exact_field(&weapon.record, "MODL")
                            .ok()
                            .flatten()
                            .and_then(|field| match &field.value {
                                FieldValue::String(model) => Some(*model),
                                _ => None,
                            }),
                    )
            })
            .collect()
    }

    fn target_animation(record: &Record, interner: &StringInterner) -> u32 {
        let dnam = exact_field(record, "DNAM").unwrap().unwrap();
        let FieldValue::Struct(fields) = &dnam.value else {
            panic!("target DNAM must be structured");
        };
        named_u32(fields, "animation_type", interner).unwrap()
    }

    fn target_form_keys(record: &Record) -> Vec<FormKey> {
        fn collect(value: &FieldValue, form_keys: &mut Vec<FormKey>) {
            match value {
                FieldValue::FormKey(form_key) if form_key.local != 0 => {
                    form_keys.push(*form_key);
                }
                FieldValue::List(values) => {
                    for value in values {
                        collect(value, form_keys);
                    }
                }
                FieldValue::Struct(fields) => {
                    for (_, value) in fields {
                        collect(value, form_keys);
                    }
                }
                _ => {}
            }
        }

        let mut form_keys = Vec::new();
        for field in &record.fields {
            collect(&field.value, &mut form_keys);
        }
        form_keys
    }

    fn normalized_struct_width(value: &FieldValue) -> usize {
        let FieldValue::Struct(fields) = value else {
            panic!("normalized field must remain structured");
        };
        fields
            .iter()
            .map(|(name, value)| match value {
                FieldValue::Bytes(bytes) => bytes.len(),
                FieldValue::FormKey(_) => 4,
                _ => panic!("normalized scalar {name:?} was not encoded"),
            })
            .sum()
    }

    #[test]
    fn all_source_melee_families_emit_only_fo4_melee_animation_types() {
        let interner = StringInterner::new();
        let mut continuous = make_weapon(&interner, 0x2030, "FalloutNV.esm", 204, 2);
        continuous.raw_dnam[0][LEGACY_FLAGS_1_OFFSET] |= LEGACY_AUTOMATIC_FLAG;
        let weapons = vec![
            make_weapon(&interner, 0x1000, "Skyrim.esm", 100, 0),
            make_weapon(&interner, 0x1010, "Skyrim.esm", 100, 4),
            make_weapon(&interner, 0x1020, "Skyrim.esm", 100, 6),
            make_weapon(&interner, 0x2000, "FalloutNV.esm", 204, 0),
            make_weapon(&interner, 0x2010, "FalloutNV.esm", 204, 1),
            make_weapon(&interner, 0x2020, "FalloutNV.esm", 204, 2),
            continuous,
            make_weapon(&interner, 0x3000, "Fallout3.esm", 136, 0),
            make_weapon(&interner, 0x3010, "Fallout3.esm", 136, 1),
            make_weapon(&interner, 0x3020, "Fallout3.esm", 136, 2),
        ];
        let stat_models = stat_models_for(&weapons);
        let plan = plan_bulk_melee_v1(weapons, &stat_models, &interner);
        assert_eq!(plan.candidates, 10);
        assert_eq!(plan.projections.len(), 10);
        assert!(plan.rejections.is_empty());
        assert_eq!(plan.receipts.len(), 10);
        assert!(plan.receipts.iter().all(|receipt| {
            receipt.disposition == MvpMeleeDisposition::Admitted
                && receipt.policy == BULK_MELEE_V1_POLICY
                && receipt.source_family.is_some()
                && receipt.target_profile.is_some()
                && receipt.world_model.is_some()
                && receipt.first_person_model.is_some()
                && receipt.first_person_resolution == MvpMeleeFirstPersonResolution::WnamStat
        }));
        let target_types = plan
            .projections
            .iter()
            .map(|projection| target_animation(&projection.record, &interner))
            .collect::<HashSet<_>>();
        assert_eq!(target_types, HashSet::from([0, 1, 5]));
        let profile_counts = plan.receipts.iter().fold(
            HashMap::<Fo4MeleeTargetProfile, usize>::new(),
            |mut counts, receipt| {
                *counts.entry(receipt.target_profile.unwrap()).or_default() += 1;
                counts
            },
        );
        assert_eq!(
            profile_counts,
            HashMap::from([
                (Fo4MeleeTargetProfile::PowerFistGauntlet, 3),
                (Fo4MeleeTargetProfile::MacheteOneHand, 3),
                (Fo4MeleeTargetProfile::GrognakTwoHand, 3),
                (Fo4MeleeTargetProfile::RipperContinuous, 1),
            ])
        );
        let continuous = plan
            .receipts
            .iter()
            .find(|receipt| receipt.source_form_key.local == 0x2030)
            .unwrap();
        assert_eq!(
            continuous.drops,
            vec![MeleeLoweringReason::ContinuousTwoHandGrip]
        );
        let fallout4 = interner.intern("Fallout4.esm");
        for projection in &plan.projections {
            let form_keys = target_form_keys(&projection.record);
            assert!(!form_keys.is_empty());
            assert!(form_keys.iter().all(|form_key| form_key.plugin == fallout4));
        }
    }

    #[test]
    fn all_target_profiles_normalize_to_complete_fo4_struct_widths() {
        let interner = StringInterner::new();
        let mut unarmed_proxy = make_weapon(&interner, 0x2100, "FalloutNV.esm", 204, 0);
        unarmed_proxy
            .record
            .fields
            .retain(|field| !matches!(field.sig.as_str(), "MODL" | "WNAM"));
        let unarmed_gauntlet = make_weapon(&interner, 0x2120, "FalloutNV.esm", 204, 0);
        let one_hand = make_weapon(&interner, 0x2140, "FalloutNV.esm", 204, 1);
        let two_hand = make_weapon(&interner, 0x2160, "FalloutNV.esm", 204, 2);
        let mut continuous = make_weapon(&interner, 0x2180, "FalloutNV.esm", 204, 1);
        continuous.raw_dnam[0][LEGACY_FLAGS_1_OFFSET] |= LEGACY_AUTOMATIC_FLAG;
        let weapons = vec![
            unarmed_proxy,
            unarmed_gauntlet,
            one_hand,
            two_hand,
            continuous,
        ];
        let stats = stat_models_for(&weapons);
        let plan = plan_bulk_melee_v1(weapons, &stats, &interner);
        assert_eq!(plan.projections.len(), 5);
        assert_eq!(
            plan.receipts
                .iter()
                .filter_map(|receipt| receipt.target_profile)
                .collect::<HashSet<_>>(),
            HashSet::from([
                Fo4MeleeTargetProfile::UnarmedProxy,
                Fo4MeleeTargetProfile::PowerFistGauntlet,
                Fo4MeleeTargetProfile::MacheteOneHand,
                Fo4MeleeTargetProfile::GrognakTwoHand,
                Fo4MeleeTargetProfile::RipperContinuous,
            ])
        );
        let source_schema = AuthoringSchema::for_game("fnv").unwrap();
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def("WEAP"),
            interner: Some(&interner),
        };
        for projection in plan.projections {
            let TargetRecordNormalization::Keep(record) = normalizer.normalize(projection.record)
            else {
                panic!("every target-native melee profile must remain target-supported");
            };
            assert_eq!(
                normalized_struct_width(&exact_field(&record, "DNAM").unwrap().unwrap().value),
                132
            );
            assert_eq!(
                normalized_struct_width(&exact_field(&record, "CRDT").unwrap().unwrap().value),
                12
            );
        }
    }

    #[test]
    fn template_inheritance_keeps_child_identity_and_flattens_mechanics() {
        let interner = StringInterner::new();
        let mut parent = make_weapon(&interner, 0x1100, "Skyrim.esm", 100, 3);
        parent.record.fields.push(field(
            "EITM",
            FieldValue::FormKey(source_key(&interner, 0x9000, "Skyrim.esm")),
        ));
        let mut child = make_weapon(&interner, 0x1200, "Skyrim.esm", 100, 3);
        child.raw_dnam.clear();
        child.record.fields.retain(|field| {
            !matches!(
                field.sig.as_str(),
                "OBND" | "FULL" | "MODL" | "WNAM" | "DATA"
            )
        });
        child.template_refs.push(parent.record.form_key);
        child.record.fields.push(field(
            "VMAD",
            FieldValue::Bytes(SmallVec::from_slice(&[1, 2, 3])),
        ));
        let child_key = child.record.form_key;
        let child_eid = child.record.eid;
        let stat_models = stat_models_for(std::slice::from_ref(&parent));
        let plan = plan_bulk_melee_v1(vec![parent, child], &stat_models, &interner);
        assert_eq!(plan.projections.len(), 2);
        let projected = plan
            .projections
            .iter()
            .find(|projection| projection.source_form_key == child_key)
            .unwrap();
        assert_eq!(projected.record.form_key, child_key);
        assert_eq!(projected.record.eid, child_eid);
        assert_eq!(
            projected.lowered,
            vec![
                MeleeLoweringReason::Enchantment,
                MeleeLoweringReason::Script,
                MeleeLoweringReason::TemplateFlattened,
            ]
        );
        assert!(
            projected
                .record
                .fields
                .iter()
                .all(|field| !matches!(field.sig.as_str(), "CNAM" | "VMAD" | "EITM"))
        );
        let receipt = plan
            .receipts
            .iter()
            .find(|receipt| receipt.source_form_key == child_key)
            .unwrap();
        assert_eq!(receipt.disposition, MvpMeleeDisposition::Admitted);
        assert_eq!(receipt.drops, projected.lowered);
    }

    #[test]
    fn model_less_unarmed_is_admitted_and_retains_stats() {
        let interner = StringInterner::new();
        let mut weapon = make_weapon(&interner, 0x1F4, "Fallout3.esm", 136, 0);
        weapon
            .record
            .fields
            .retain(|field| !matches!(field.sig.as_str(), "FULL" | "MODL" | "WNAM"));
        let plan = plan_bulk_melee_v1(vec![weapon], &HashMap::new(), &interner);
        assert_eq!(plan.projections.len(), 1);
        assert_eq!(plan.receipts.len(), 1);
        assert_eq!(plan.receipts[0].policy, BULK_MELEE_V1_POLICY);
        assert_eq!(
            plan.receipts[0].target_profile,
            Some(Fo4MeleeTargetProfile::UnarmedProxy)
        );
        assert_eq!(plan.receipts[0].world_model, None);
        assert_eq!(plan.receipts[0].first_person_model, None);
        assert_eq!(
            plan.receipts[0].first_person_resolution,
            MvpMeleeFirstPersonResolution::None
        );
        let output = &plan.projections[0].record;
        assert_eq!(target_animation(output, &interner), 0);
        assert!(
            output
                .fields
                .iter()
                .all(|field| !matches!(field.sig.as_str(), "MODL" | "MOD4"))
        );
        let dnam = exact_field(output, "DNAM").unwrap().unwrap();
        let FieldValue::Struct(fields) = &dnam.value else {
            panic!("target DNAM");
        };
        assert_eq!(named_u32(fields, "value", &interner), Some(25));
        assert_eq!(named_u32(fields, "damage_base", &interner), Some(12));
        assert_eq!(named_f32(fields, "weight", &interner), Some(2.0));
    }

    #[test]
    fn missing_wnam_uses_explicit_world_model_fallback() {
        let interner = StringInterner::new();
        let mut weapon = make_weapon(&interner, 0x2F4, "Skyrim.esm", 100, 4);
        weapon
            .record
            .fields
            .retain(|field| field.sig.as_str() != "WNAM");
        let plan = plan_bulk_melee_v1(vec![weapon], &HashMap::new(), &interner);
        assert_eq!(plan.projections.len(), 1);
        let receipt = &plan.receipts[0];
        assert_eq!(receipt.disposition, MvpMeleeDisposition::Admitted);
        assert_eq!(
            receipt.first_person_resolution,
            MvpMeleeFirstPersonResolution::WorldModelFallback
        );
        assert_eq!(receipt.first_person_model, receipt.world_model);
        assert_eq!(
            projected_model(&plan.projections[0].record, "MOD4"),
            receipt.world_model
        );
    }

    #[test]
    fn width_provenance_and_ranged_contradictions_fail_closed() {
        let interner = StringInterner::new();
        let mut weapons = vec![
            make_weapon(&interner, 0x1000, "Skyrim.esm", 100, 7),
            make_weapon(&interner, 0x1010, "Skyrim.esm", 99, 0),
            make_weapon(&interner, 0x2000, "FalloutNV.esm", 204, 10),
            make_weapon(&interner, 0x2010, "FalloutNV.esm", 204, 3),
            make_weapon(&interner, 0x2020, "FalloutNV.esm", 204, 1),
            make_weapon(&interner, 0x3000, "Fallout3.esm", 204, 1),
            make_weapon(&interner, 0x4000, "Skyrim.esm", 100, 10),
            make_weapon(&interner, 0x4010, "FalloutNV.esm", 204, 14),
        ];
        weapons[4].has_ammo = true;
        let stat_models = stat_models_for(&weapons);
        let plan = plan_bulk_melee_v1(weapons, &stat_models, &interner);
        assert!(plan.projections.is_empty());
        assert!(
            plan.rejections.iter().any(|rejection| matches!(
                rejection.reason,
                MeleeRejectionReason::RangedAnimation(7)
            ))
        );
        assert!(
            plan.rejections
                .iter()
                .any(|rejection| matches!(rejection.reason, MeleeRejectionReason::DnamWidth(99)))
        );
        assert!(plan.rejections.iter().any(|rejection| matches!(
            rejection.reason,
            MeleeRejectionReason::ThrowingAnimation(10)
        )));
        assert!(
            plan.rejections.iter().any(|rejection| matches!(
                rejection.reason,
                MeleeRejectionReason::AmmoContradiction
            ))
        );
        assert!(
            plan.rejections.iter().any(|rejection| matches!(
                rejection.reason,
                MeleeRejectionReason::ProvenanceMismatch
            ))
        );
        assert_eq!(
            plan.rejections
                .iter()
                .filter(|rejection| matches!(
                    rejection.reason,
                    MeleeRejectionReason::UnknownAnimation(_)
                ))
                .count(),
            2
        );
        assert_eq!(plan.receipts.len(), plan.candidates);
        assert!(plan.receipts.iter().all(|receipt| {
            matches!(receipt.disposition, MvpMeleeDisposition::Rejected(_))
                && receipt.policy == BULK_MELEE_V1_POLICY
                && receipt.target_profile.is_none()
                && receipt.world_model.is_none()
                && receipt.first_person_model.is_none()
                && receipt.drops.is_empty()
        }));
    }

    #[test]
    fn merged_fnvfo3_owner_preserves_fo3_layout_provenance() {
        let interner = StringInterner::new();
        let weapon = make_weapon(&interner, 0x5A00, "FalloutNV.esm", FO3_DNAM_WIDTH, 1);
        let stat_models = stat_models_for(std::slice::from_ref(&weapon));
        let plan = plan_bulk_melee_v1(vec![weapon], &stat_models, &interner);
        assert_eq!(plan.projections.len(), 1);
        assert_eq!(plan.receipts.len(), 1);
        assert_eq!(plan.receipts[0].source_family, Some(SourceMeleeFamily::Fo3));
        assert_eq!(plan.receipts[0].disposition, MvpMeleeDisposition::Admitted);
        assert_eq!(
            plan.receipts[0].target_profile,
            Some(Fo4MeleeTargetProfile::MacheteOneHand)
        );
    }

    #[test]
    fn legacy_integrated_melee_is_record_only_and_gun_equipment_rejects_first() {
        let interner = StringInterner::new();
        let mut integrated = make_weapon(
            &interner,
            0x03_BC6F,
            "FalloutNV.esm",
            FNV_LEGACY_INTEGRATED_DNAM_WIDTH,
            1,
        );
        integrated.raw_dnam[0][LEGACY_FLAGS_1_OFFSET] |= LEGACY_EMBEDDED_WEAPON_FLAG;
        integrated
            .record
            .fields
            .retain(|field| !matches!(field.sig.as_str(), "MODL" | "WNAM"));

        let mut gun = make_weapon(&interner, 0x0F_26D3, "FalloutNV.esm", 164, 0);
        gun.record.fields.push(field("ETYP", FieldValue::Int(0)));
        let plan = plan_bulk_melee_v1(vec![integrated, gun], &HashMap::new(), &interner);
        assert_eq!(plan.projections.len(), 1);
        assert_eq!(plan.projections[0].source_form_key.local, 0x03_BC6F);
        assert_eq!(plan.receipts[0].source_family, Some(SourceMeleeFamily::Fnv));
        assert_eq!(plan.receipts[0].world_model, None);
        assert_eq!(plan.receipts[0].first_person_model, None);
        assert_eq!(
            plan.receipts[1].disposition,
            MvpMeleeDisposition::Rejected(MeleeRejectionReason::GunEquipmentContradiction)
        );
    }

    #[test]
    fn projectile_and_template_drift_are_rejected() {
        let interner = StringInterner::new();
        let mut projectile = make_weapon(&interner, 0x2000, "FalloutNV.esm", 204, 1);
        projectile.raw_dnam[0][LEGACY_PROJECTILE_OFFSET] = 1;
        let mut cycle = make_weapon(&interner, 0x2100, "FalloutNV.esm", 204, 1);
        cycle.template_refs.push(cycle.record.form_key);
        let stats = stat_models_for(&[projectile.clone(), cycle.clone()]);
        let plan = plan_bulk_melee_v1(vec![projectile, cycle], &stats, &interner);
        assert!(plan.rejections.iter().any(|rejection| matches!(
            rejection.reason,
            MeleeRejectionReason::ProjectileContradiction
        )));
        assert!(
            plan.rejections
                .iter()
                .any(|rejection| matches!(rejection.reason, MeleeRejectionReason::TemplateCycle))
        );
    }

    struct RealCorpusPlan {
        plan: MeleeCorpusPlan,
        source_types: Vec<(FormKey, SourceMeleeFamily, u32)>,
        raw_source_types: Vec<RawCorpusType>,
        physical_raw_source_types: Vec<RawCorpusType>,
        deleted: HashSet<FormKey>,
    }

    #[derive(Clone, Copy)]
    struct RawCorpusType {
        form_key: FormKey,
        width: usize,
        animation_type: u32,
    }

    fn raw_corpus_type(raw_record: &ParsedRecord, form_key: FormKey) -> Option<RawCorpusType> {
        let subrecords = effective_subrecords_for_record(raw_record);
        let mut dnam = subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "DNAM");
        let data = dnam.next()?.data.as_ref();
        if dnam.next().is_some() {
            return None;
        }
        let animation_type = if data.len() == SKYRIM_DNAM_WIDTH {
            u32::from(*data.first()?)
        } else {
            u32::from_le_bytes(data.get(0..4)?.try_into().ok()?)
        };
        Some(RawCorpusType {
            form_key,
            width: data.len(),
            animation_type,
        })
    }

    fn supported_source_type(raw: RawCorpusType) -> Option<(FormKey, SourceMeleeFamily, u32)> {
        let family = match raw.width {
            SKYRIM_DNAM_WIDTH => SourceMeleeFamily::Skyrim,
            FNV_DNAM_WIDTH | FNV_LEGACY_INTEGRATED_DNAM_WIDTH => SourceMeleeFamily::Fnv,
            FO3_DNAM_WIDTH => SourceMeleeFamily::Fo3,
            _ => return None,
        };
        Some((raw.form_key, family, raw.animation_type))
    }

    fn load_real_corpus_paths(paths: &[&std::path::Path], game: &str) -> RealCorpusPlan {
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game(game).unwrap();
        let signature = SigCode::from_str("WEAP").unwrap();
        let mut handles = Vec::new();
        let mut winners = HashMap::new();
        let mut physical_raw_source_types = Vec::new();
        for path in paths {
            let handle = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
                path.to_str().unwrap(),
                Some(game),
                None,
                None,
                true,
            )
            .unwrap();
            let keys = iter_form_keys_of_sig(handle, signature, &interner).unwrap();
            let batch = snapshot_records_by_form_keys(handle, &keys, &interner).unwrap();
            for snapshot in &batch.records {
                let record = decode_record_from_parsed(
                    &snapshot.raw_record,
                    &snapshot.form_key,
                    &schema,
                    &batch.masters,
                    &batch.plugin_name,
                    batch.strings.as_ref(),
                    batch.plugin_is_localized,
                    &interner,
                )
                .unwrap();
                let raw_source_type = raw_corpus_type(&snapshot.raw_record, snapshot.form_key);
                let source_type = raw_source_type.and_then(supported_source_type);
                let weapon = source_weapon(
                    record,
                    &snapshot.raw_record,
                    &batch.masters,
                    &batch.plugin_name,
                    &interner,
                );
                if let Some(raw_source_type) = raw_source_type {
                    physical_raw_source_types.push(raw_source_type);
                }
                winners.insert(
                    weapon.record.form_key,
                    (weapon, source_type, raw_source_type),
                );
            }
            handles.push(handle);
        }
        let mut winners = winners.into_values().collect::<Vec<_>>();
        winners.sort_by_key(|(weapon, _, _)| {
            (
                interner
                    .resolve(weapon.record.form_key.plugin)
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
                weapon.record.form_key.local,
            )
        });
        let source_types = winners
            .iter()
            .filter_map(|(_, source_type, _)| *source_type)
            .collect::<Vec<_>>();
        let raw_source_types = winners
            .iter()
            .filter_map(|(_, _, raw_source_type)| *raw_source_type)
            .collect::<Vec<_>>();
        let weapons = winners
            .into_iter()
            .map(|(weapon, _, _)| weapon)
            .collect::<Vec<_>>();
        let deleted = weapons
            .iter()
            .filter(|weapon| weapon.record.flags.contains(RecordFlags::DELETED))
            .map(|weapon| weapon.record.form_key)
            .collect::<HashSet<_>>();
        let mut stat_models = HashMap::new();
        for weapon in &weapons {
            if let Ok(Some(wnam)) = exact_field(&weapon.record, "WNAM")
                && let Some(form_key) = form_key_value(&wnam.value)
            {
                for handle in handles.iter().rev() {
                    if let Ok(stat) = read_record_relayout_by_form_key(
                        *handle, &form_key, &schema, &interner, None,
                    ) && let Ok(Some(model)) = exact_field(&stat, "MODL")
                        && let FieldValue::String(model) = model.value
                    {
                        stat_models.insert(form_key, model);
                        break;
                    }
                }
            }
        }
        let winner_count = weapons.len();
        let plan = plan_bulk_melee_v1(weapons, &stat_models, &interner);
        assert_eq!(plan.candidates, winner_count);
        assert!(validate_bulk_melee_v1_plan(&plan, &interner));
        assert!(plan.projections.iter().all(|projection| matches!(
            target_animation(&projection.record, &interner),
            0 | 1 | 5
        )));
        for handle in handles {
            esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle);
        }
        RealCorpusPlan {
            plan,
            source_types,
            raw_source_types,
            physical_raw_source_types,
            deleted,
        }
    }

    fn load_real_corpus(path: &std::path::Path, game: &str) -> RealCorpusPlan {
        load_real_corpus_paths(&[path], game)
    }

    fn admitted_count(corpus: &RealCorpusPlan, family: SourceMeleeFamily) -> usize {
        corpus
            .plan
            .receipts
            .iter()
            .filter(|receipt| {
                receipt.source_family == Some(family)
                    && receipt.disposition == MvpMeleeDisposition::Admitted
            })
            .count()
    }

    fn raw_melee_candidate_count(
        source_types: &[RawCorpusType],
        family: SourceMeleeFamily,
    ) -> usize {
        source_types
            .iter()
            .filter(|source_type| match family {
                SourceMeleeFamily::Skyrim => {
                    source_type.width == SKYRIM_DNAM_WIDTH && source_type.animation_type <= 6
                }
                SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3 => {
                    source_type.width != SKYRIM_DNAM_WIDTH && source_type.animation_type <= 2
                }
            })
            .count()
    }

    fn assert_family_count_and_ranged_rejection(
        corpus: &RealCorpusPlan,
        family: SourceMeleeFamily,
        expected_melee_counts: &[usize],
        count_deleted: bool,
    ) {
        let melee = corpus
            .source_types
            .iter()
            .filter(|(form_key, source_family, animation_type)| {
                *source_family == family
                    && (count_deleted || !corpus.deleted.contains(form_key))
                    && match family {
                        SourceMeleeFamily::Skyrim => *animation_type <= 6,
                        SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3 => *animation_type <= 2,
                    }
            })
            .map(|(form_key, _, _)| *form_key)
            .collect::<HashSet<_>>();
        assert!(
            expected_melee_counts.contains(&melee.len()),
            "unexpected {family:?} melee count {}",
            melee.len()
        );
        for source_form_key in &melee {
            let receipt = corpus
                .plan
                .receipts
                .iter()
                .find(|receipt| receipt.source_form_key == *source_form_key)
                .unwrap();
            if corpus.deleted.contains(source_form_key) {
                assert_eq!(
                    receipt.disposition,
                    MvpMeleeDisposition::Rejected(MeleeRejectionReason::Deleted),
                    "{source_form_key:?}"
                );
            } else {
                assert_eq!(
                    receipt.disposition,
                    MvpMeleeDisposition::Admitted,
                    "{source_form_key:?}"
                );
            }
        }

        assert_ranged_rejection(corpus, family);
    }

    fn assert_ranged_rejection(corpus: &RealCorpusPlan, family: SourceMeleeFamily) {
        for (source_form_key, source_family, animation_type) in &corpus.source_types {
            let is_ranged_or_throwing = *source_family == family
                && !corpus.deleted.contains(source_form_key)
                && match family {
                    SourceMeleeFamily::Skyrim => (7..=9).contains(animation_type),
                    SourceMeleeFamily::Fnv | SourceMeleeFamily::Fo3 => {
                        (3..=13).contains(animation_type)
                    }
                };
            if !is_ranged_or_throwing {
                continue;
            }
            let receipt = corpus
                .plan
                .receipts
                .iter()
                .find(|receipt| receipt.source_form_key == *source_form_key)
                .unwrap();
            assert!(
                matches!(receipt.disposition, MvpMeleeDisposition::Rejected(_)),
                "ranged or throwing source must terminate as rejected: {source_form_key:?}"
            );
            assert!(
                corpus
                    .plan
                    .projections
                    .iter()
                    .all(|projection| projection.source_form_key != *source_form_key)
            );
        }
    }

    #[test]
    fn optional_real_skyrim_official_count_gate() {
        let Some(path) = std::env::var_os("SKYRIMSE_WEAPON_CORPUS_PLUGIN") else {
            return;
        };
        let path = std::path::Path::new(&path);
        if !path.is_file() {
            return;
        }
        let data_dir = path.parent().unwrap();
        let official_paths = [
            "Skyrim.esm",
            "Update.esm",
            "Dawnguard.esm",
            "HearthFires.esm",
            "Dragonborn.esm",
        ]
        .map(|name| data_dir.join(name));
        if official_paths.iter().any(|path| !path.is_file()) {
            return;
        }
        let official_refs = official_paths
            .iter()
            .map(std::path::PathBuf::as_path)
            .collect::<Vec<_>>();
        let corpus = load_real_corpus_paths(&official_refs, "skyrimse");
        assert_eq!(
            raw_melee_candidate_count(&corpus.physical_raw_source_types, SourceMeleeFamily::Skyrim,),
            2_815
        );
        assert_family_count_and_ranged_rejection(
            &corpus,
            SourceMeleeFamily::Skyrim,
            &[2_792],
            false,
        );
        assert_eq!(admitted_count(&corpus, SourceMeleeFamily::Skyrim), 2_792);
    }

    #[test]
    fn optional_real_fnv_official_count_gate() {
        let Some(path) = std::env::var_os("FNV_WEAPON_CORPUS_PLUGIN") else {
            return;
        };
        let path = std::path::Path::new(&path);
        if !path.is_file() {
            return;
        }
        let data_dir = path.parent().unwrap();
        let mut official_paths = [
            "FalloutNV.esm",
            "DeadMoney.esm",
            "HonestHearts.esm",
            "OldWorldBlues.esm",
            "LonesomeRoad.esm",
            "GunRunnersArsenal.esm",
            "CaravanPack.esm",
            "ClassicPack.esm",
            "MercenaryPack.esm",
        ]
        .map(|name| data_dir.join(name))
        .to_vec();
        let include_tribal = std::env::var_os("FNV_WEAPON_CORPUS_INCLUDE_TRIBAL").is_some();
        if include_tribal {
            official_paths.push(data_dir.join("TribalPack.esm"));
        }
        if official_paths.iter().any(|path| !path.is_file()) {
            return;
        }
        let official_refs = official_paths
            .iter()
            .map(std::path::PathBuf::as_path)
            .collect::<Vec<_>>();
        let corpus = load_real_corpus_paths(&official_refs, "fnv");
        assert_eq!(
            raw_melee_candidate_count(&corpus.physical_raw_source_types, SourceMeleeFamily::Fnv,),
            if include_tribal { 158 } else { 157 }
        );
        assert_eq!(
            raw_melee_candidate_count(&corpus.raw_source_types, SourceMeleeFamily::Fnv),
            if include_tribal { 156 } else { 155 }
        );
        assert_eq!(
            corpus
                .raw_source_types
                .iter()
                .filter(|source_type| {
                    source_type.width != SKYRIM_DNAM_WIDTH
                        && source_type.animation_type <= 2
                        && corpus.deleted.contains(&source_type.form_key)
                })
                .count(),
            0,
            "official FNV type 0..2 winning identities contain no deleted tombstones"
        );
        assert_eq!(
            corpus
                .source_types
                .iter()
                .filter(|(_, family, animation_type)| {
                    *family == SourceMeleeFamily::Fnv && *animation_type <= 2
                })
                .count(),
            if include_tribal { 150 } else { 149 }
        );
        assert_family_count_and_ranged_rejection(&corpus, SourceMeleeFamily::Fo3, &[1], true);
        assert_ranged_rejection(&corpus, SourceMeleeFamily::Fnv);
        assert_eq!(admitted_count(&corpus, SourceMeleeFamily::Fo3), 1);
        let mut expected_rejections = HashMap::new();
        for (local, reason) in [
            (0x00_01F6, MeleeRejectionReason::GunEquipmentContradiction),
            (0x0F_204A, MeleeRejectionReason::GunEquipmentContradiction),
            (0x0F_26CE, MeleeRejectionReason::GunEquipmentContradiction),
            (0x0F_26CF, MeleeRejectionReason::GunEquipmentContradiction),
            (0x0F_26D3, MeleeRejectionReason::GunEquipmentContradiction),
            (0x0F_26D4, MeleeRejectionReason::GunEquipmentContradiction),
            (0x0F_26D7, MeleeRejectionReason::GunEquipmentContradiction),
            (0x0F_26DA, MeleeRejectionReason::GunEquipmentContradiction),
            (0x10_84AD, MeleeRejectionReason::GunEquipmentContradiction),
            (0x16_6B95, MeleeRejectionReason::ProjectileContradiction),
            (0x15_BA03, MeleeRejectionReason::ProjectileContradiction),
            (0x00_0809, MeleeRejectionReason::ProjectileContradiction),
        ] {
            let source_form_key = corpus
                .raw_source_types
                .iter()
                .find(|source_type| source_type.form_key.local == local)
                .map(|source_type| source_type.form_key)
                .expect("official rejected raw type 0..2 candidate");
            expected_rejections.insert(source_form_key, reason);
        }
        assert_eq!(expected_rejections.len(), 12);
        for source_type in corpus.raw_source_types.iter().filter(|source_type| {
            source_type.width != SKYRIM_DNAM_WIDTH && source_type.animation_type <= 2
        }) {
            let source_form_key = source_type.form_key;
            let receipt = corpus
                .plan
                .receipts
                .iter()
                .find(|receipt| receipt.source_form_key == source_form_key)
                .expect("every raw type 0..2 candidate has a receipt");
            if let Some(reason) = expected_rejections.get(&source_form_key) {
                assert_eq!(
                    receipt.disposition,
                    MvpMeleeDisposition::Rejected(reason.clone()),
                    "{source_form_key:?}"
                );
                assert!(
                    corpus
                        .plan
                        .projections
                        .iter()
                        .all(|projection| { projection.source_form_key != source_form_key })
                );
            } else {
                assert_eq!(
                    receipt.disposition,
                    MvpMeleeDisposition::Admitted,
                    "{source_form_key:?}"
                );
            }
        }
        assert_eq!(
            admitted_count(&corpus, SourceMeleeFamily::Fnv),
            if include_tribal { 143 } else { 142 }
        );
        let admitted = corpus
            .plan
            .receipts
            .iter()
            .filter(|receipt| receipt.disposition == MvpMeleeDisposition::Admitted)
            .collect::<Vec<_>>();
        assert_eq!(admitted.len(), if include_tribal { 144 } else { 143 });
        for profile in [
            Fo4MeleeTargetProfile::UnarmedProxy,
            Fo4MeleeTargetProfile::PowerFistGauntlet,
            Fo4MeleeTargetProfile::MacheteOneHand,
            Fo4MeleeTargetProfile::GrognakTwoHand,
            Fo4MeleeTargetProfile::RipperContinuous,
        ] {
            assert!(
                admitted
                    .iter()
                    .any(|receipt| receipt.target_profile == Some(profile)),
                "missing admitted target profile {}",
                profile.code()
            );
        }
    }

    #[test]
    fn optional_real_fnvfo3_graft_layout_gate() {
        let Some(path) = std::env::var_os("FNVFO3_MERGED_WEAPON_CORPUS_PLUGIN") else {
            return;
        };
        let path = std::path::Path::new(&path);
        if !path.is_file() {
            return;
        }
        let corpus = load_real_corpus(path, "fnv");
        let fo3_count = corpus
            .source_types
            .iter()
            .filter(|(_, family, animation_type)| {
                *family == SourceMeleeFamily::Fo3 && *animation_type <= 2
            })
            .count();
        assert!(fo3_count > 0, "merged fixture has no 136-byte FO3 WEAP");
        for (source_form_key, _, _) in
            corpus
                .source_types
                .iter()
                .filter(|(_, family, animation_type)| {
                    *family == SourceMeleeFamily::Fo3 && *animation_type <= 2
                })
        {
            let receipt = corpus
                .plan
                .receipts
                .iter()
                .find(|receipt| receipt.source_form_key == *source_form_key)
                .expect("every raw 136-byte FO3 melee identity has a terminal receipt");
            assert_eq!(receipt.source_family, Some(SourceMeleeFamily::Fo3));
            assert_eq!(
                corpus
                    .plan
                    .projections
                    .iter()
                    .any(|projection| projection.source_form_key == *source_form_key),
                receipt.disposition == MvpMeleeDisposition::Admitted
            );
        }
        assert_ranged_rejection(&corpus, SourceMeleeFamily::Fo3);
    }
}
