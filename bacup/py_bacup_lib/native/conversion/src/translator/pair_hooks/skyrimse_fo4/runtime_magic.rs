use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const VMAD: SubrecordSig = SubrecordSig(*b"VMAD");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicComponent {
    ScrollConsumable,
    Spell,
    CloakEffect,
    AimedFireDamageEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicCast {
    Scroll,
    FireAndForget,
    Concentration,
}

impl MagicCast {
    fn target_value(self) -> u64 {
        match self {
            Self::Scroll | Self::FireAndForget => 1,
            Self::Concentration => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicDelivery {
    SelfTarget,
    Aimed,
}

impl MagicDelivery {
    fn target_value(self) -> u64 {
        match self {
            Self::SelfTarget => 0,
            Self::Aimed => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicArchetype {
    ValueModifier,
    Cloak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicVmadIntent {
    PreserveForFo4Port,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicEffectFlag {
    Hostile,
    Detrimental,
    NoArea,
    FxPersist,
    NoRecast,
    NoDeathDispel,
}

impl MagicEffectFlag {
    fn target_mask(self) -> u64 {
        match self {
            Self::Hostile => 1,
            Self::Detrimental => 4,
            Self::NoArea => 2_048,
            Self::FxPersist => 4_096,
            Self::NoRecast => 131_072,
            Self::NoDeathDispel => 268_435_456,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MagicEffectEdge {
    pub effect: FormKey,
    pub magnitude: f32,
    pub area: u32,
    pub duration: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicDonorRole {
    HealthActorValue,
    EnergyResistance,
    FireHitShader,
    AimedFlameProjectile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MagicDonorIdentity {
    pub role: MagicDonorRole,
    pub form_key: FormKey,
    pub signature: SigCode,
    pub editor_id: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MagicLoweringPlan {
    pub source_form: FormKey,
    pub source_signature: SigCode,
    pub target_signature: SigCode,
    pub component: MagicComponent,
    pub cast: MagicCast,
    pub delivery: MagicDelivery,
    pub archetype: Option<MagicArchetype>,
    pub effect: Option<MagicEffectEdge>,
    pub associated_item: Option<FormKey>,
    pub item_value: Option<u32>,
    pub item_weight: Option<f32>,
    pub effect_flags: Vec<MagicEffectFlag>,
    pub donor_roles: Vec<MagicDonorRole>,
    pub vmad_intent: MagicVmadIntent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicLinkedRole {
    BaseEffect,
    AssociatedSpell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MagicLinkedRecordRequirement {
    pub role: MagicLinkedRole,
    pub form_key: FormKey,
    pub signature: SigCode,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MagicComponentRequirements {
    pub plan: MagicLoweringPlan,
    pub linked_records: Vec<MagicLinkedRecordRequirement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicUnsupportedReason {
    Signature,
    EffectCardinality,
    MissingData,
    CastDelivery,
    Archetype,
    ActorValue,
    Resistance,
    Projectile,
    EffectFlags,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MagicSupport {
    Supported(MagicLoweringPlan),
    RequiresLinkedRecords(MagicComponentRequirements),
    Unsupported { reason: MagicUnsupportedReason },
}

impl MagicSupport {
    pub(crate) fn plan(&self) -> Option<&MagicLoweringPlan> {
        match self {
            Self::Supported(plan) => Some(plan),
            Self::RequiresLinkedRecords(requirements) => Some(&requirements.plan),
            Self::Unsupported { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MagicLoweringReceipt {
    pub source_form: FormKey,
    pub source_signature: SigCode,
    pub target_signature: SigCode,
    pub component: MagicComponent,
    pub donor_roles: Vec<MagicDonorRole>,
    pub preserved_vmad_fields: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MagicContractError {
    PlanDrift {
        form_key: FormKey,
    },
    DonorSignatureDrift {
        role: MagicDonorRole,
        expected: SigCode,
        actual: SigCode,
    },
    DonorEditorIdDrift {
        role: MagicDonorRole,
        expected: &'static str,
        actual: Option<String>,
    },
}

impl std::fmt::Display for MagicContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PlanDrift { form_key } => write!(
                f,
                "Skyrim magic lowering plan no longer matches {:06X}",
                form_key.local
            ),
            Self::DonorSignatureDrift {
                role,
                expected,
                actual,
            } => write!(
                f,
                "FO4 magic donor {role:?} signature drift: expected {}, found {}",
                expected.as_str(),
                actual.as_str()
            ),
            Self::DonorEditorIdDrift {
                role,
                expected,
                actual,
            } => write!(
                f,
                "FO4 magic donor {role:?} EditorID drift: expected {expected}, found {}",
                actual.as_deref().unwrap_or("<missing>")
            ),
        }
    }
}

impl std::error::Error for MagicContractError {}

pub(crate) fn classify_magic_component(record: &Record, interner: &StringInterner) -> MagicSupport {
    match record.sig.0 {
        sig if sig == *b"SCRL" => classify_scroll(record, interner),
        sig if sig == *b"SPEL" => classify_spell(record, interner),
        sig if sig == *b"MGEF" => classify_magic_effect(record, interner),
        _ => MagicSupport::Unsupported {
            reason: MagicUnsupportedReason::Signature,
        },
    }
}

pub(crate) fn strip_source_vmad(record: &mut Record) -> usize {
    let removed = record
        .fields
        .iter()
        .filter(|field| field.sig == VMAD)
        .count();
    record.fields.retain(|field| field.sig != VMAD);
    removed
}

pub(crate) fn normalize_standalone_vmad_for_fo4(record: &mut Record) -> usize {
    let mut normalized = 0;
    for field in &mut record.fields {
        if field.sig != VMAD {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut field.value else {
            continue;
        };
        if bytes.len() >= 4 && matches!(u16::from_le_bytes([bytes[2], bytes[3]]), 1 | 2) {
            bytes[..2].copy_from_slice(&6_u16.to_le_bytes());
            normalized += 1;
        }
    }
    normalized
}

pub(crate) fn lower_supported_magic_record(
    record: &mut Record,
    plan: &MagicLoweringPlan,
    interner: &StringInterner,
) -> Result<MagicLoweringReceipt, MagicContractError> {
    let current = classify_magic_component(record, interner);
    if current.plan() != Some(plan) {
        return Err(MagicContractError::PlanDrift {
            form_key: record.form_key,
        });
    }
    let preserved_vmad_fields = record
        .fields
        .iter()
        .filter(|field| field.sig == VMAD)
        .count();
    match plan.component {
        MagicComponent::ScrollConsumable => lower_scroll(record, plan, interner),
        MagicComponent::Spell => lower_spell(record, plan, interner),
        MagicComponent::CloakEffect => lower_cloak_effect(record, plan, interner),
        MagicComponent::AimedFireDamageEffect => lower_aimed_fire_effect(record, plan, interner),
    }
    Ok(MagicLoweringReceipt {
        source_form: plan.source_form,
        source_signature: plan.source_signature,
        target_signature: plan.target_signature,
        component: plan.component,
        donor_roles: plan.donor_roles.clone(),
        preserved_vmad_fields,
    })
}

pub(crate) fn lower_magic_record_for_fo4(
    record: &mut Record,
    interner: &StringInterner,
) -> Result<Option<MagicLoweringReceipt>, MagicContractError> {
    if let Some(plan) = classify_magic_component(record, interner).plan().cloned() {
        return lower_supported_magic_record(record, &plan, interner).map(Some);
    }
    match record.sig.0 {
        sig if sig == *b"SCRL" => lower_fallback_scroll(record, interner),
        sig if matches!(sig, value if value == *b"SPEL" || value == *b"SHOU") => {
            lower_fallback_spell(record, interner)
        }
        _ => {}
    }
    Ok(None)
}

fn lower_fallback_scroll(record: &mut Record, interner: &StringInterner) {
    let item = exact_named_field(record, *b"DATA");
    let item_value = item
        .and_then(|fields| scalar_u32(named(fields, "Value", interner)))
        .unwrap_or_default();
    let item_weight = item
        .and_then(|fields| scalar_f32(named(fields, "Weight", interner)))
        .unwrap_or_default();
    reset(
        record,
        SigCode(*b"ALCH"),
        &[
            *b"EDID", *b"VMAD", *b"OBND", *b"FULL", *b"DESC", *b"EFID", *b"EFIT",
        ],
    );
    push(record, *b"DATA", FieldValue::Float(item_weight));
    push(
        record,
        *b"ENIT",
        object(
            interner,
            [
                ("Value", FieldValue::Int(i64::from(item_value))),
                ("Flags", FieldValue::Uint(0x0001_0001)),
                ("Addiction", FieldValue::Uint(0)),
                ("AddictionChance", FieldValue::Float(0.0)),
                ("SoundConsume", FieldValue::Uint(0)),
            ],
        ),
    );
}

fn lower_fallback_spell(record: &mut Record, interner: &StringInterner) {
    reset(
        record,
        SigCode(*b"SPEL"),
        &[
            *b"EDID", *b"VMAD", *b"OBND", *b"FULL", *b"DESC", *b"EFID", *b"EFIT",
        ],
    );
    push_spell_data(
        record,
        MagicCast::FireAndForget,
        MagicDelivery::SelfTarget,
        interner,
    );
}

pub(crate) fn magic_donor_identity(
    role: MagicDonorRole,
    interner: &StringInterner,
) -> MagicDonorIdentity {
    let (local, signature, editor_id) = match role {
        MagicDonorRole::HealthActorValue => (0x0002D4, *b"AVIF", "Health"),
        MagicDonorRole::EnergyResistance => (0x0002EB, *b"AVIF", "EnergyResist"),
        MagicDonorRole::FireHitShader => (0x14237E, *b"EFSH", "FireHitFXS"),
        MagicDonorRole::AimedFlameProjectile => {
            (0x204172, *b"PROJ", "WorkshopTrapFlamethrowerProjectile")
        }
    };
    MagicDonorIdentity {
        role,
        form_key: FormKey {
            local,
            plugin: interner.intern("Fallout4.esm"),
        },
        signature: SigCode(signature),
        editor_id,
    }
}

pub(crate) fn validate_magic_donor(
    record: &Record,
    role: MagicDonorRole,
    interner: &StringInterner,
) -> Result<MagicDonorIdentity, MagicContractError> {
    let expected = magic_donor_identity(role, interner);
    if record.form_key != expected.form_key || record.sig != expected.signature {
        return Err(MagicContractError::DonorSignatureDrift {
            role,
            expected: expected.signature,
            actual: record.sig,
        });
    }
    let actual = record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .map(str::to_owned);
    let edid_matches = exact_fields(record, *b"EDID").is_some_and(|fields| {
        matches!(fields[0].value, FieldValue::String(value) if interner.resolve(value) == Some(expected.editor_id))
    });
    if actual.as_deref() != Some(expected.editor_id) || !edid_matches {
        return Err(MagicContractError::DonorEditorIdDrift {
            role,
            expected: expected.editor_id,
            actual,
        });
    }
    Ok(expected)
}

fn classify_scroll(record: &Record, interner: &StringInterner) -> MagicSupport {
    let Ok(effect) = effect_edge(record, interner) else {
        return unsupported(MagicUnsupportedReason::EffectCardinality);
    };
    let Some(data) = exact_named_field(record, *b"SPIT") else {
        return unsupported(MagicUnsupportedReason::MissingData);
    };
    if parse_cast(named(data, "CastType", interner), interner) != Some(MagicCast::Scroll)
        || parse_delivery(named(data, "Delivery", interner), interner)
            .unwrap_or(MagicDelivery::SelfTarget)
            != MagicDelivery::SelfTarget
    {
        return unsupported(MagicUnsupportedReason::CastDelivery);
    }
    let Some(item) = exact_named_field(record, *b"DATA") else {
        return unsupported(MagicUnsupportedReason::MissingData);
    };
    let Some(item_value) = scalar_u32(named(item, "Value", interner)) else {
        return unsupported(MagicUnsupportedReason::MissingData);
    };
    let Some(item_weight) = scalar_f32(named(item, "Weight", interner)) else {
        return unsupported(MagicUnsupportedReason::MissingData);
    };
    requirements(
        plan(
            record,
            *b"ALCH",
            MagicComponent::ScrollConsumable,
            MagicCast::Scroll,
            MagicDelivery::SelfTarget,
            None,
            Some(effect),
            None,
            Some(item_value),
            Some(item_weight),
            Vec::new(),
            Vec::new(),
        ),
        MagicLinkedRecordRequirement {
            role: MagicLinkedRole::BaseEffect,
            form_key: effect.effect,
            signature: SigCode(*b"MGEF"),
        },
    )
}

fn classify_spell(record: &Record, interner: &StringInterner) -> MagicSupport {
    let Ok(effect) = effect_edge(record, interner) else {
        return unsupported(MagicUnsupportedReason::EffectCardinality);
    };
    let Some(data) = exact_named_field(record, *b"SPIT") else {
        return unsupported(MagicUnsupportedReason::MissingData);
    };
    let Some(cast) = parse_cast(named(data, "CastType", interner), interner) else {
        return unsupported(MagicUnsupportedReason::CastDelivery);
    };
    let delivery = parse_delivery(
        named(data, "Delivery", interner).or_else(|| named(data, "TargetType", interner)),
        interner,
    )
    .unwrap_or(MagicDelivery::SelfTarget);
    if !matches!(
        (cast, delivery),
        (MagicCast::FireAndForget, MagicDelivery::SelfTarget)
            | (MagicCast::Concentration, MagicDelivery::Aimed)
    ) {
        return unsupported(MagicUnsupportedReason::CastDelivery);
    }
    requirements(
        plan(
            record,
            *b"SPEL",
            MagicComponent::Spell,
            cast,
            delivery,
            None,
            Some(effect),
            None,
            None,
            None,
            Vec::new(),
            Vec::new(),
        ),
        MagicLinkedRecordRequirement {
            role: MagicLinkedRole::BaseEffect,
            form_key: effect.effect,
            signature: SigCode(*b"MGEF"),
        },
    )
}

fn classify_magic_effect(record: &Record, interner: &StringInterner) -> MagicSupport {
    let Some(data) = exact_named_field(record, *b"DATA") else {
        return unsupported(MagicUnsupportedReason::MissingData);
    };
    let Some(archetype) = parse_archetype(
        named(data, "Archetype", interner).or_else(|| named(data, "Archtype", interner)),
        interner,
    ) else {
        return unsupported(MagicUnsupportedReason::Archetype);
    };
    match archetype {
        MagicArchetype::Cloak => classify_cloak_effect(record, data, interner),
        MagicArchetype::ValueModifier => classify_aimed_fire_effect(record, data, interner),
    }
}

fn classify_cloak_effect(
    record: &Record,
    data: &[(crate::sym::Sym, FieldValue)],
    interner: &StringInterner,
) -> MagicSupport {
    let cast = parse_cast(named(data, "CastingType", interner), interner)
        .unwrap_or(MagicCast::FireAndForget);
    let delivery = parse_delivery(named(data, "Delivery", interner), interner)
        .unwrap_or(MagicDelivery::SelfTarget);
    if (cast, delivery) != (MagicCast::FireAndForget, MagicDelivery::SelfTarget) {
        return unsupported(MagicUnsupportedReason::CastDelivery);
    }
    let Some(associated_item) =
        named(data, "AssocItem", interner).and_then(|value| form_from_value(value, interner))
    else {
        return unsupported(MagicUnsupportedReason::MissingData);
    };
    let flags = effect_flags(named(data, "Flags", interner), interner);
    if !flags.contains(&MagicEffectFlag::Detrimental) {
        return unsupported(MagicUnsupportedReason::EffectFlags);
    }
    let plan = plan(
        record,
        *b"MGEF",
        MagicComponent::CloakEffect,
        cast,
        delivery,
        Some(MagicArchetype::Cloak),
        None,
        Some(associated_item),
        None,
        None,
        flags,
        Vec::new(),
    );
    requirements(
        plan,
        MagicLinkedRecordRequirement {
            role: MagicLinkedRole::AssociatedSpell,
            form_key: associated_item,
            signature: SigCode(*b"SPEL"),
        },
    )
}

fn classify_aimed_fire_effect(
    record: &Record,
    data: &[(crate::sym::Sym, FieldValue)],
    interner: &StringInterner,
) -> MagicSupport {
    if parse_cast(named(data, "CastingType", interner), interner) != Some(MagicCast::Concentration)
        || parse_delivery(named(data, "Delivery", interner), interner) != Some(MagicDelivery::Aimed)
    {
        return unsupported(MagicUnsupportedReason::CastDelivery);
    }
    if !semantic_matches(named(data, "ActorValue", interner), interner, 24, "health") {
        return unsupported(MagicUnsupportedReason::ActorValue);
    }
    if !semantic_matches(
        named(data, "ResistValue", interner),
        interner,
        41,
        "resistfire",
    ) {
        return unsupported(MagicUnsupportedReason::Resistance);
    }
    if named(data, "Projectile", interner)
        .and_then(|value| form_from_value(value, interner))
        .is_none()
    {
        return unsupported(MagicUnsupportedReason::Projectile);
    }
    let flags = effect_flags(named(data, "Flags", interner), interner);
    if !flags.contains(&MagicEffectFlag::Hostile) || !flags.contains(&MagicEffectFlag::Detrimental)
    {
        return unsupported(MagicUnsupportedReason::EffectFlags);
    }
    MagicSupport::Supported(plan(
        record,
        *b"MGEF",
        MagicComponent::AimedFireDamageEffect,
        MagicCast::Concentration,
        MagicDelivery::Aimed,
        Some(MagicArchetype::ValueModifier),
        None,
        None,
        None,
        None,
        flags,
        vec![
            MagicDonorRole::HealthActorValue,
            MagicDonorRole::EnergyResistance,
            MagicDonorRole::FireHitShader,
            MagicDonorRole::AimedFlameProjectile,
        ],
    ))
}

#[allow(clippy::too_many_arguments)]
fn plan(
    record: &Record,
    target_signature: [u8; 4],
    component: MagicComponent,
    cast: MagicCast,
    delivery: MagicDelivery,
    archetype: Option<MagicArchetype>,
    effect: Option<MagicEffectEdge>,
    associated_item: Option<FormKey>,
    item_value: Option<u32>,
    item_weight: Option<f32>,
    effect_flags: Vec<MagicEffectFlag>,
    donor_roles: Vec<MagicDonorRole>,
) -> MagicLoweringPlan {
    MagicLoweringPlan {
        source_form: record.form_key,
        source_signature: record.sig,
        target_signature: SigCode(target_signature),
        component,
        cast,
        delivery,
        archetype,
        effect,
        associated_item,
        item_value,
        item_weight,
        effect_flags,
        donor_roles,
        vmad_intent: MagicVmadIntent::PreserveForFo4Port,
    }
}

fn requirements(
    plan: MagicLoweringPlan,
    requirement: MagicLinkedRecordRequirement,
) -> MagicSupport {
    MagicSupport::RequiresLinkedRecords(MagicComponentRequirements {
        plan,
        linked_records: vec![requirement],
    })
}

fn unsupported(reason: MagicUnsupportedReason) -> MagicSupport {
    MagicSupport::Unsupported { reason }
}

fn lower_scroll(record: &mut Record, plan: &MagicLoweringPlan, interner: &StringInterner) {
    reset(
        record,
        plan.target_signature,
        &[*b"EDID", *b"VMAD", *b"OBND", *b"FULL", *b"DESC"],
    );
    push(
        record,
        *b"DATA",
        FieldValue::Float(plan.item_weight.expect("classified scroll has weight")),
    );
    push(
        record,
        *b"ENIT",
        object(
            interner,
            [
                (
                    "Value",
                    FieldValue::Int(i64::from(
                        plan.item_value.expect("classified scroll has value"),
                    )),
                ),
                ("Flags", FieldValue::Uint(0x0001_0001)),
                ("Addiction", FieldValue::Uint(0)),
                ("AddictionChance", FieldValue::Float(0.0)),
                ("SoundConsume", FieldValue::Uint(0)),
            ],
        ),
    );
    push_effect(
        record,
        plan.effect.expect("classified scroll has effect"),
        interner,
    );
}

fn lower_spell(record: &mut Record, plan: &MagicLoweringPlan, interner: &StringInterner) {
    reset(
        record,
        plan.target_signature,
        &[*b"EDID", *b"VMAD", *b"OBND", *b"FULL", *b"DESC"],
    );
    push_spell_data(record, plan.cast, plan.delivery, interner);
    push_effect(
        record,
        plan.effect.expect("classified spell has effect"),
        interner,
    );
}

fn lower_cloak_effect(record: &mut Record, plan: &MagicLoweringPlan, interner: &StringInterner) {
    reset(
        record,
        plan.target_signature,
        &[*b"EDID", *b"VMAD", *b"FULL"],
    );
    push(record, *b"DATA", fo4_magic_effect_data(plan, interner));
}

fn lower_aimed_fire_effect(
    record: &mut Record,
    plan: &MagicLoweringPlan,
    interner: &StringInterner,
) {
    reset(
        record,
        plan.target_signature,
        &[*b"EDID", *b"VMAD", *b"FULL"],
    );
    push(record, *b"DATA", fo4_magic_effect_data(plan, interner));
}

fn reset(record: &mut Record, target_signature: SigCode, carried: &[[u8; 4]]) {
    let fields = record
        .fields
        .iter()
        .filter(|field| carried.contains(&field.sig.0))
        .cloned()
        .collect();
    record.sig = target_signature;
    record.fields = fields;
}

fn push_spell_data(
    record: &mut Record,
    cast: MagicCast,
    delivery: MagicDelivery,
    interner: &StringInterner,
) {
    push(
        record,
        *b"SPIT",
        object(
            interner,
            [
                ("BaseCost", FieldValue::Uint(0)),
                ("Flags", FieldValue::Uint(0)),
                ("Type", FieldValue::Uint(0)),
                ("ChargeTime", FieldValue::Float(0.0)),
                ("CastType", FieldValue::Uint(cast.target_value())),
                ("TargetType", FieldValue::Uint(delivery.target_value())),
                ("CastDuration", FieldValue::Float(0.0)),
                ("Range", FieldValue::Float(0.0)),
                ("CastingPerk", FieldValue::Uint(0)),
            ],
        ),
    );
}

fn push_effect(record: &mut Record, effect: MagicEffectEdge, interner: &StringInterner) {
    push(record, *b"EFID", FieldValue::FormKey(effect.effect));
    push(
        record,
        *b"EFIT",
        object(
            interner,
            [
                ("Magnitude", FieldValue::Float(effect.magnitude)),
                ("Area", FieldValue::Uint(u64::from(effect.area))),
                ("Duration", FieldValue::Uint(u64::from(effect.duration))),
            ],
        ),
    );
}

fn fo4_magic_effect_data(plan: &MagicLoweringPlan, interner: &StringInterner) -> FieldValue {
    let donor = |role| FieldValue::FormKey(magic_donor_identity(role, interner).form_key);
    let zero_byte = || FieldValue::Bytes(smallvec::smallvec![0]);
    let byte = |value| FieldValue::Bytes(smallvec::smallvec![value]);
    let zero_word = || FieldValue::Bytes(smallvec::smallvec![0, 0]);
    let aimed = plan.component == MagicComponent::AimedFireDamageEffect;
    let associated_item = plan
        .associated_item
        .map(FieldValue::FormKey)
        .unwrap_or(FieldValue::Uint(0));
    let flags = plan
        .effect_flags
        .iter()
        .fold(0_u64, |value, flag| value | flag.target_mask());

    object(
        interner,
        [
            ("Flags", FieldValue::Uint(flags)),
            ("BaseCost", FieldValue::Float(0.0)),
            ("AssocItem", associated_item),
            ("MagicSkillUnusedByte1", zero_byte()),
            ("MagicSkillUnusedByte2", zero_byte()),
            ("MagicSkillUnusedByte3", zero_byte()),
            ("MagicSkillUnusedByte4", zero_byte()),
            (
                "ResistValue",
                aimed
                    .then(|| donor(MagicDonorRole::EnergyResistance))
                    .unwrap_or(FieldValue::Uint(0)),
            ),
            ("CounterEffectCount", zero_word()),
            ("UnknownByte10", if aimed { byte(255) } else { zero_byte() }),
            ("UnknownByte11", if aimed { byte(255) } else { zero_byte() }),
            ("CastingLight", FieldValue::Uint(0)),
            ("TaperWeight", FieldValue::Float(0.0)),
            (
                "HitShader",
                aimed
                    .then(|| donor(MagicDonorRole::FireHitShader))
                    .unwrap_or(FieldValue::Uint(0)),
            ),
            ("EnchantShader", FieldValue::Uint(0)),
            ("MinimumSkillLevel", FieldValue::Uint(0)),
            ("SpellmakingArea", FieldValue::Uint(0)),
            ("SpellmakingCastingTime", FieldValue::Float(0.0)),
            ("TaperCurve", FieldValue::Float(0.0)),
            ("TaperDuration", FieldValue::Float(0.0)),
            ("SecondAVWeight", FieldValue::Float(0.0)),
            (
                "Archetype",
                FieldValue::Uint(if plan.archetype == Some(MagicArchetype::Cloak) {
                    35
                } else {
                    0
                }),
            ),
            (
                "ActorValue",
                aimed
                    .then(|| donor(MagicDonorRole::HealthActorValue))
                    .unwrap_or(FieldValue::Uint(0)),
            ),
            (
                "Projectile",
                aimed
                    .then(|| donor(MagicDonorRole::AimedFlameProjectile))
                    .unwrap_or(FieldValue::Uint(0)),
            ),
            ("Explosion", FieldValue::Uint(0)),
            ("CastingType", FieldValue::Uint(plan.cast.target_value())),
            ("Delivery", FieldValue::Uint(plan.delivery.target_value())),
            ("ActorValue1", FieldValue::Uint(0)),
            ("CastingArt", FieldValue::Uint(0)),
            ("HitEffectArt", FieldValue::Uint(0)),
            ("ImpactData", FieldValue::Uint(0)),
            ("SkillUsageMultiplier", FieldValue::Float(0.0)),
            ("DualCastingArt", FieldValue::Uint(0)),
            ("DualCastingScale", FieldValue::Float(1.0)),
            ("EnchantArt", FieldValue::Uint(0)),
            ("HitVisuals", FieldValue::Uint(0)),
            ("EnchantVisuals", FieldValue::Uint(0)),
            ("EquipAbility", FieldValue::Uint(0)),
            ("ImageSpaceModifier", FieldValue::Uint(0)),
            ("PerkToApply", FieldValue::Uint(0)),
            ("CastingSoundLevel", FieldValue::Uint(1)),
            ("ScriptEffectAIScore", FieldValue::Float(0.0)),
            ("ScriptEffectAIDelayTime", FieldValue::Float(0.0)),
        ],
    )
}

fn effect_edge(
    record: &Record,
    interner: &StringInterner,
) -> Result<MagicEffectEdge, MagicUnsupportedReason> {
    let efid = exact_fields(record, *b"EFID")
        .and_then(|fields| form_from_value(&fields[0].value, interner))
        .ok_or(MagicUnsupportedReason::EffectCardinality)?;
    let efit =
        exact_named_field(record, *b"EFIT").ok_or(MagicUnsupportedReason::EffectCardinality)?;
    Ok(MagicEffectEdge {
        effect: efid,
        magnitude: scalar_f32(named(efit, "Magnitude", interner)).unwrap_or(0.0),
        area: scalar_u32(named(efit, "Area", interner)).unwrap_or(0),
        duration: scalar_u32(named(efit, "Duration", interner)).unwrap_or(0),
    })
}

fn exact_fields(record: &Record, signature: [u8; 4]) -> Option<Vec<&FieldEntry>> {
    let fields: Vec<_> = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == signature)
        .collect();
    (fields.len() == 1).then_some(fields)
}

fn exact_named_field(
    record: &Record,
    signature: [u8; 4],
) -> Option<&[(crate::sym::Sym, FieldValue)]> {
    let fields = exact_fields(record, signature)?;
    let FieldValue::Struct(values) = &fields[0].value else {
        return None;
    };
    Some(values)
}

fn named<'a>(
    values: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    let expected = normalized(name);
    values.iter().find_map(|(key, value)| {
        interner
            .resolve(*key)
            .is_some_and(|actual| normalized(actual) == expected)
            .then_some(value)
    })
}

fn parse_cast(value: Option<&FieldValue>, interner: &StringInterner) -> Option<MagicCast> {
    match semantic(value, interner).as_deref() {
        Some("3") | Some("scroll") => Some(MagicCast::Scroll),
        Some("1") | Some("fireandforget") => Some(MagicCast::FireAndForget),
        Some("2") | Some("concentration") => Some(MagicCast::Concentration),
        _ => None,
    }
}

fn parse_delivery(value: Option<&FieldValue>, interner: &StringInterner) -> Option<MagicDelivery> {
    match semantic(value, interner).as_deref() {
        Some("0") | Some("self") => Some(MagicDelivery::SelfTarget),
        Some("2") | Some("aimed") => Some(MagicDelivery::Aimed),
        _ => None,
    }
}

fn parse_archetype(
    value: Option<&FieldValue>,
    interner: &StringInterner,
) -> Option<MagicArchetype> {
    match semantic(value, interner).as_deref() {
        None | Some("0") | Some("valuemodifier") => Some(MagicArchetype::ValueModifier),
        Some("35") | Some("cloak") => Some(MagicArchetype::Cloak),
        _ => None,
    }
}

fn semantic_matches(
    value: Option<&FieldValue>,
    interner: &StringInterner,
    numeric: i64,
    label: &str,
) -> bool {
    semantic(value, interner)
        .is_some_and(|value| value == numeric.to_string() || value == normalized(label))
}

fn semantic(value: Option<&FieldValue>, interner: &StringInterner) -> Option<String> {
    match value? {
        FieldValue::String(value) => interner.resolve(*value).map(normalized),
        FieldValue::Int(value) => Some(value.to_string()),
        FieldValue::Uint(value) => Some(value.to_string()),
        _ => None,
    }
}

fn effect_flags(value: Option<&FieldValue>, interner: &StringInterner) -> Vec<MagicEffectFlag> {
    let contains = |bit: u64, label: &str| match value {
        Some(FieldValue::Uint(bits)) => bits & bit != 0,
        Some(FieldValue::Int(bits)) if *bits >= 0 => (*bits as u64) & bit != 0,
        Some(FieldValue::List(values)) => values.iter().any(|value| {
            semantic(Some(value), interner).as_deref() == Some(normalized(label).as_str())
        }),
        _ => false,
    };
    [
        (MagicEffectFlag::Hostile, 1, "Hostile"),
        (MagicEffectFlag::Detrimental, 4, "Detrimental"),
        (MagicEffectFlag::NoArea, 2048, "NoArea"),
        (MagicEffectFlag::FxPersist, 4096, "FXPersist"),
        (MagicEffectFlag::NoRecast, 131072, "NoRecast"),
        (MagicEffectFlag::NoDeathDispel, 268435456, "NoDeathDispel"),
    ]
    .into_iter()
    .filter_map(|(flag, bit, label)| contains(bit, label).then_some(flag))
    .collect()
}

fn scalar_u32(value: Option<&FieldValue>) -> Option<u32> {
    match value? {
        FieldValue::Uint(value) => (*value).try_into().ok(),
        FieldValue::Int(value) => (*value).try_into().ok(),
        _ => None,
    }
}

fn scalar_f32(value: Option<&FieldValue>) -> Option<f32> {
    match value? {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Uint(value) => Some(*value as f32),
        FieldValue::Int(value) => Some(*value as f32),
        _ => None,
    }
}

fn form_from_value(value: &FieldValue, interner: &StringInterner) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form) => Some(*form),
        FieldValue::String(value) => interner
            .resolve(*value)
            .and_then(|value| FormKey::parse(value, interner).ok()),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| form_from_value(value, interner)),
        _ => None,
    }
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn push(record: &mut Record, signature: [u8; 4], value: FieldValue) {
    record.fields.push(FieldEntry {
        sig: SubrecordSig(signature),
        value,
    });
}

fn object<const N: usize>(
    interner: &StringInterner,
    values: [(&str, FieldValue); N],
) -> FieldValue {
    FieldValue::Struct(
        values
            .into_iter()
            .map(|(name, value)| (interner.intern(name), value))
            .collect(),
    )
}
