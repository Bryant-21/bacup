//! Source-neutral FO4 creature record closure authoring.

use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use smallvec::SmallVec;
use thiserror::Error;

use super::CreatureActorActionRecordPlan;
use super::corpus::{Fo4RaceDataTarget, SourceCreatureIdentity};
use super::manifest::{
    CapabilityGraphManifest, CreatureClipRole, CreatureGraphTemplate, CreatureManifest, EventUsage,
    MvpGraphManifest,
};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::sym::StringInterner;
use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};

const FALLOUT4_MASTER: &str = "Fallout4.esm";
const BOTH_HANDS: u32 = 0x013F45;
const ANIMS_UNARMED: u32 = 0x02405E;
const WEAPON_TYPE_UNARMED: u32 = 0x05240E;
const BODY_SLOT_33: u32 = 1 << (33 - 30);
const ROOT_GEOMETRY_SEGMENT: u8 = 32;

pub const ATTACK_TYPE_RUNTIME_INERT_PROOF_SCHEMA: &str = "source-rig-attack-type-runtime-inert-v1";
pub const ATTACK_SHOUT_TO_SPELL_PROOF_SCHEMA: &str = "source-rig-attack-shout-to-spell-v1";
pub const ATTACK_STAMINA_TO_ACTION_POINTS_PROOF_SCHEMA: &str =
    "source-rig-attack-stamina-to-action-points-v1";
pub const ATTACK_STAGGER_OFFSET_ZERO_PROOF_SCHEMA: &str =
    "source-rig-attack-stagger-offset-zero-v1";

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TargetFormKey {
    pub local: u32,
    pub plugin: String,
}

impl TargetFormKey {
    pub fn new(local: u32, plugin: impl Into<String>) -> Self {
        Self {
            local,
            plugin: plugin.into(),
        }
    }

    fn intern(&self, interner: &StringInterner) -> FormKey {
        FormKey {
            local: self.local,
            plugin: interner.intern(&self.plugin),
        }
    }
}

impl Serialize for TargetFormKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:06X}@{}", self.local, self.plugin))
    }
}

impl<'de> Deserialize<'de> for TargetFormKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let (local, plugin) = value
            .split_once('@')
            .ok_or_else(|| de::Error::custom("target FormKey must be HEX@Plugin"))?;
        if plugin.trim().is_empty() {
            return Err(de::Error::custom("target FormKey plugin is empty"));
        }
        let local = u32::from_str_radix(local, 16)
            .map_err(|_| de::Error::custom("target FormKey local id is not hexadecimal"))?;
        if local > 0x00ff_ffff {
            return Err(de::Error::custom("target FormKey local id exceeds 24 bits"));
        }
        Ok(Self::new(local, plugin))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureRecordFormKeys {
    pub race: TargetFormKey,
    pub npc: TargetFormKey,
    pub skin: TargetFormKey,
    pub armor_addon: TargetFormKey,
    pub body_part_data: TargetFormKey,
    pub unarmed_weapon: TargetFormKey,
}

impl CreatureRecordFormKeys {
    fn entries(&self) -> [(&'static str, &TargetFormKey); 6] {
        [
            ("RACE", &self.race),
            ("NPC_", &self.npc),
            ("ARMO", &self.skin),
            ("ARMA", &self.armor_addon),
            ("BPTD", &self.body_part_data),
            ("WEAP", &self.unarmed_weapon),
        ]
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureRecordEditorIds {
    pub race: String,
    pub npc: String,
    pub skin: String,
    pub armor_addon: String,
    pub body_part_data: String,
    pub unarmed_weapon: String,
}

impl CreatureRecordEditorIds {
    fn entries(&self) -> [(&'static str, &str); 6] {
        [
            ("RACE", &self.race),
            ("NPC_", &self.npc),
            ("ARMO", &self.skin),
            ("ARMA", &self.armor_addon),
            ("BPTD", &self.body_part_data),
            ("WEAP", &self.unarmed_weapon),
        ]
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureRecordManifest {
    pub target_plugin: String,
    pub editor_id_prefix: String,
    pub form_keys: CreatureRecordFormKeys,
    pub editor_ids: CreatureRecordEditorIds,
    pub display_name: String,
    pub body_nif: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureBodyNifRecordPart {
    pub body_nif: String,
    pub armor_addon_form_key: TargetFormKey,
    pub armor_addon_editor_id: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RootBodyPartProfile {
    pub name: String,
    pub node: String,
    pub vats_target: String,
    pub geometry_segment_index: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureRecordProfile {
    pub root_body_part: RootBodyPartProfile,
    pub attack_damage_multiplier: f32,
    pub attack_chance: f32,
    pub attack_strike_angle: f32,
    pub unarmed_damage: u16,
    pub unarmed_reach: f32,
    pub unarmed_attack_seconds: f32,
    pub action_point_cost: f32,
    pub npc_level: u16,
    pub npc_health: u16,
    pub npc_action_points: u16,
}

impl CreatureRecordProfile {
    pub fn root_segment_32(root_node: impl Into<String>) -> Self {
        let root_node = root_node.into();
        Self {
            root_body_part: RootBodyPartProfile {
                name: "Root".to_string(),
                node: root_node.clone(),
                vats_target: root_node,
                geometry_segment_index: ROOT_GEOMETRY_SEGMENT,
            },
            attack_damage_multiplier: 1.0,
            attack_chance: 1.0,
            attack_strike_angle: 35.0,
            unarmed_damage: 10,
            unarmed_reach: 0.68,
            unarmed_attack_seconds: 0.8,
            action_point_cost: 20.0,
            npc_level: 1,
            npc_health: 25,
            npc_action_points: 50,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureNpcRecordVariant {
    pub source_identity: SourceCreatureIdentity,
    pub form_key: TargetFormKey,
    pub editor_id: String,
    pub display_name: String,
    pub primary: bool,
    pub level: u16,
    pub health: u16,
    pub action_points: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npc_inventory: Option<Vec<CreatureNpcInventoryEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npc_equipment: Option<Vec<CreatureTargetRecordReference>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npc_spells: Option<Vec<CreatureTargetRecordReference>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npc_death_item: Option<Option<CreatureTargetRecordReference>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureAttackRecordVariant {
    pub id: String,
    pub event: String,
    pub primary: bool,
    pub projection: CreatureAttackRecordProjection,
    pub damage_multiplier: f32,
    pub chance: f32,
    pub strike_angle: f32,
    pub action_point_cost: f32,
    #[serde(default)]
    pub target_data: CreatureAttackTargetData,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureAttackTargetData {
    pub attack_flags: u32,
    pub attack_angle: f32,
    pub stagger: f32,
    pub knockdown: f32,
    pub recovery_time: f32,
    pub action_points_multiplier: f32,
    pub stagger_offset: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_atkd: Option<CreatureAttackSourceDataReceipt>,
}

impl Default for CreatureAttackTargetData {
    fn default() -> Self {
        Self {
            attack_flags: 0,
            attack_angle: 0.0,
            stagger: 0.0,
            knockdown: 0.0,
            recovery_time: 0.0,
            action_points_multiplier: 1.0,
            stagger_offset: 0,
            source_atkd: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureSourceRecordReference {
    pub source_identity: SourceCreatureIdentity,
    pub signature: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureAttackSourceDataReceipt {
    pub schema_id: String,
    pub damage_multiplier_bits: u32,
    pub chance_bits: u32,
    pub attack_spell: Option<CreatureSourceRecordReference>,
    pub attack_flags: u32,
    pub attack_angle_bits: u32,
    pub strike_angle_bits: u32,
    pub stagger_bits: u32,
    pub attack_type: Option<CreatureSourceRecordReference>,
    pub knockdown_bits: u32,
    pub recovery_time_bits: u32,
    pub stamina_multiplier_bits: u32,
    pub event: String,
    pub ordinal: u32,
    pub attack_type_policy: CreatureAttackTypePolicy,
    pub attack_spell_policy: CreatureAttackSpellPolicy,
    pub stamina_multiplier_policy: CreatureAttackStaminaPolicy,
    pub stagger_offset_policy: CreatureAttackStaggerOffsetPolicy,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAttackTypePolicy {
    NullOmitted,
    RuntimeInertKeyword { proof: CreatureSemanticProofReceipt },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAttackSpellPolicy {
    NoSourceSpell,
    DirectSpell,
    LowerShoutToSpell { proof: CreatureSemanticProofReceipt },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAttackStaminaPolicy {
    PreserveAsActionPointsMultiplier { proof: CreatureSemanticProofReceipt },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureAttackStaggerOffsetPolicy {
    ExplicitZero { proof: CreatureSemanticProofReceipt },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureSemanticProofReceipt {
    pub schema: String,
    pub canonical_json: String,
    pub canonical_json_blake3: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CreatureTargetRecordReference {
    pub signature: String,
    pub form_key: TargetFormKey,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureNpcInventoryEntry {
    pub target_record: CreatureTargetRecordReference,
    pub count: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ownership: Option<CreatureNpcInventoryOwnership>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreatureNpcInventoryOwnership {
    FactionRank {
        faction: CreatureTargetRecordReference,
        required_rank: i32,
        condition: f32,
    },
    OwnerGlobal {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        owner: Option<CreatureTargetRecordReference>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        global: Option<CreatureTargetRecordReference>,
        condition: f32,
    },
}

impl CreatureTargetRecordReference {
    pub fn new(signature: impl Into<String>, form_key: TargetFormKey) -> Self {
        Self {
            signature: signature.into(),
            form_key,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreatureAttackRecordProjection {
    MeleeUnarmed {
        weapon_form_key: TargetFormKey,
        weapon_editor_id: String,
        damage: u16,
        reach: f32,
        attack_seconds: f32,
    },
    MeleeEquipment {
        equipment: Vec<CreatureTargetRecordReference>,
    },
    RangedProjectile {
        attack_spell: CreatureTargetRecordReference,
        projectile: CreatureTargetRecordReference,
        equipment: CreatureTargetRecordReference,
    },
    RangedEquipment {
        equipment: Vec<CreatureTargetRecordReference>,
        projectile: Option<CreatureTargetRecordReference>,
        ammunition: Option<CreatureTargetRecordReference>,
    },
    SpellAbility {
        spell: CreatureTargetRecordReference,
    },
    Stationary {
        attack_spell: CreatureTargetRecordReference,
        projectile: CreatureTargetRecordReference,
        equipment: CreatureTargetRecordReference,
    },
    ContinuousRobot {
        attack_spell: CreatureTargetRecordReference,
        equipment: CreatureTargetRecordReference,
    },
}

impl CreatureAttackRecordProjection {
    fn kind_name(&self) -> &'static str {
        match self {
            Self::MeleeUnarmed { .. } => "melee_unarmed",
            Self::MeleeEquipment { .. } => "melee_equipment",
            Self::RangedProjectile { .. } => "ranged_projectile",
            Self::RangedEquipment { .. } => "ranged_equipment",
            Self::SpellAbility { .. } => "spell_ability",
            Self::Stationary { .. } => "stationary",
            Self::ContinuousRobot { .. } => "continuous_robot",
        }
    }

    fn attack_spell(&self) -> Option<&CreatureTargetRecordReference> {
        match self {
            Self::RangedProjectile { attack_spell, .. }
            | Self::Stationary { attack_spell, .. }
            | Self::ContinuousRobot { attack_spell, .. } => Some(attack_spell),
            Self::SpellAbility { spell } => Some(spell),
            Self::MeleeUnarmed { .. }
            | Self::MeleeEquipment { .. }
            | Self::RangedEquipment { .. } => None,
        }
    }

    fn equipment(&self) -> Vec<&CreatureTargetRecordReference> {
        match self {
            Self::MeleeEquipment { equipment } | Self::RangedEquipment { equipment, .. } => {
                equipment.iter().collect()
            }
            Self::RangedProjectile { equipment, .. }
            | Self::Stationary { equipment, .. }
            | Self::ContinuousRobot { equipment, .. } => vec![equipment],
            Self::MeleeUnarmed { .. } | Self::SpellAbility { .. } => Vec::new(),
        }
    }

    fn target_dependencies(&self) -> Vec<&CreatureTargetRecordReference> {
        match self {
            Self::MeleeUnarmed { .. } => Vec::new(),
            Self::MeleeEquipment { equipment } => equipment.iter().collect(),
            Self::RangedEquipment {
                equipment,
                projectile,
                ammunition,
            } => equipment
                .iter()
                .chain(projectile.iter())
                .chain(ammunition.iter())
                .collect(),
            Self::RangedProjectile {
                attack_spell,
                projectile,
                equipment,
            }
            | Self::Stationary {
                attack_spell,
                projectile,
                equipment,
            } => vec![attack_spell, projectile, equipment],
            Self::SpellAbility { spell } => vec![spell],
            Self::ContinuousRobot {
                attack_spell,
                equipment,
            } => vec![attack_spell, equipment],
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreatureBodyPartProjection {
    RootOnly32 {
        name: String,
        node: String,
        vats_target: String,
    },
}

impl CreatureBodyPartProjection {
    pub fn root_only_32(root_node: impl Into<String>) -> Self {
        let root_node = root_node.into();
        Self::RootOnly32 {
            name: "Root".to_string(),
            node: root_node.clone(),
            vats_target: root_node,
        }
    }

    fn profile(&self) -> RootBodyPartProfile {
        match self {
            Self::RootOnly32 {
                name,
                node,
                vats_target,
            } => RootBodyPartProfile {
                name: name.clone(),
                node: node.clone(),
                vats_target: vats_target.clone(),
                geometry_segment_index: ROOT_GEOMETRY_SEGMENT,
            },
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureRecordProjectionManifest {
    pub base: CreatureRecordManifest,
    pub source_primary_identity: SourceCreatureIdentity,
    pub race_data: Fo4RaceDataTarget,
    pub body_parts: CreatureBodyPartProjection,
    #[serde(default)]
    pub body_nif_parts: Vec<CreatureBodyNifRecordPart>,
    pub variants: Vec<CreatureNpcRecordVariant>,
    #[serde(default)]
    pub npc_inventory: Vec<CreatureNpcInventoryEntry>,
    #[serde(default)]
    pub npc_equipment: Vec<CreatureTargetRecordReference>,
    #[serde(default)]
    pub npc_spells: Vec<CreatureTargetRecordReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npc_death_item: Option<CreatureTargetRecordReference>,
    pub attacks: Vec<CreatureAttackRecordVariant>,
}

impl CreatureRecordProjectionManifest {
    pub fn effective_body_nif_parts(&self) -> Vec<CreatureBodyNifRecordPart> {
        if self.body_nif_parts.is_empty() {
            vec![CreatureBodyNifRecordPart {
                body_nif: self.base.body_nif.clone(),
                armor_addon_form_key: self.base.form_keys.armor_addon.clone(),
                armor_addon_editor_id: self.base.editor_ids.armor_addon.clone(),
            }]
        } else {
            self.body_nif_parts.clone()
        }
    }

    pub(crate) fn materialize_legacy_body_nif_part(&mut self) {
        if self.body_nif_parts.is_empty() {
            self.body_nif_parts = self.effective_body_nif_parts();
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct ProjectedRecordIdentity {
    pub signature: String,
    pub source_identity: SourceCreatureIdentity,
    pub target_form_key: TargetFormKey,
    pub primary: bool,
}

#[derive(Clone, Debug)]
pub struct CreatureRecordProjectionClosure {
    pub source_primary_identity: SourceCreatureIdentity,
    pub projected_identities: Vec<ProjectedRecordIdentity>,
    pub required_target_records: Vec<CreatureTargetRecordReference>,
    pub closure: CreatureRecordClosure,
}

#[derive(Clone, Debug)]
pub struct CreatureRecordClosure {
    pub target_plugin: String,
    pub records: Vec<Record>,
}

impl CreatureRecordClosure {
    pub fn record(&self, signature: &str) -> Option<&Record> {
        self.records
            .iter()
            .find(|record| record.sig.as_str() == signature)
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CreatureRecordError {
    #[error("{code}: {message}")]
    InvalidManifest { code: &'static str, message: String },
    #[error("unsupported FO4 field {record}.{field}: {reason}")]
    UnsupportedField {
        record: &'static str,
        field: &'static str,
        reason: String,
    },
    #[error("FO4 schema unavailable: {0}")]
    Schema(String),
    #[error("FO4 schema rejected record type {0}")]
    UnsupportedRecord(String),
    #[error("FO4 normalization changed the {record} field set: before={before:?}, after={after:?}")]
    NormalizationLoss {
        record: String,
        before: Vec<String>,
        after: Vec<String>,
    },
}

pub fn emit_creature_record_closure(
    rig: &CreatureManifest,
    graph: &MvpGraphManifest,
    manifest: &CreatureRecordManifest,
    profile: &CreatureRecordProfile,
    interner: &StringInterner,
) -> Result<CreatureRecordClosure, CreatureRecordError> {
    validate_inputs(rig, graph, manifest, profile)?;

    let custom = InternedRecordKeys::new(&manifest.form_keys, interner);
    let fallout4 = interner.intern(FALLOUT4_MASTER);
    let mut records = vec![
        emit_race(
            rig,
            &graph.melee_event,
            manifest,
            profile,
            &custom,
            interner,
        ),
        emit_npc(manifest, profile, &custom, interner),
        emit_skin(manifest, &custom, interner),
        emit_armor_addon(manifest, &custom, interner),
        emit_body_part_data(rig, manifest, profile, &custom, interner),
        emit_unarmed_weapon(manifest, profile, &custom, fallout4, interner),
    ];

    let schema = AuthoringSchema::for_game("fo4").map_err(CreatureRecordError::Schema)?;
    normalize_records(&mut records, &schema, interner)?;

    let closure = CreatureRecordClosure {
        target_plugin: manifest.target_plugin.clone(),
        records,
    };
    validate_closure(&closure, rig, graph, manifest, profile, &schema, interner)?;
    Ok(closure)
}

pub fn emit_creature_record_projection(
    rig: &CreatureManifest,
    graph: &MvpGraphManifest,
    projection: &CreatureRecordProjectionManifest,
    interner: &StringInterner,
) -> Result<CreatureRecordProjectionClosure, CreatureRecordError> {
    rig.validate_mvp(graph)
        .map_err(|errors| invalid("graph_contract", errors.to_string()))?;
    emit_creature_record_projection_inner(
        rig,
        &ProjectionGraphContract::from_mvp(graph),
        projection,
        interner,
    )
}

pub fn emit_creature_capability_record_projection(
    rig: &CreatureManifest,
    graph: &CapabilityGraphManifest,
    projection: &CreatureRecordProjectionManifest,
    interner: &StringInterner,
) -> Result<CreatureRecordProjectionClosure, CreatureRecordError> {
    rig.validate_capability_graph(graph)
        .map_err(|errors| invalid("graph_contract", errors.to_string()))?;
    emit_creature_record_projection_inner(
        rig,
        &ProjectionGraphContract::from_capability(graph),
        projection,
        interner,
    )
}

pub fn append_creature_actor_action_records(
    closure: &mut CreatureRecordProjectionClosure,
    plans: &[CreatureActorActionRecordPlan],
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    if plans.is_empty() {
        return Ok(());
    }
    let mut plans = plans.to_vec();
    plans.sort_by_key(|plan| {
        (
            plan.requirement.parent_form_id,
            plan.requirement.kind,
            plan.requirement.animation_event.to_ascii_lowercase(),
            plan.editor_id.to_ascii_lowercase(),
            plan.form_key.local,
        )
    });
    let mut seen_requirements = BTreeSet::new();
    let mut seen_form_keys = closure
        .closure
        .records
        .iter()
        .filter_map(|record| {
            interner
                .resolve(record.form_key.plugin)
                .filter(|plugin| plugin.eq_ignore_ascii_case(&closure.closure.target_plugin))
                .map(|_| record.form_key.local)
        })
        .collect::<HashSet<_>>();
    let mut seen_editor_ids = closure
        .closure
        .records
        .iter()
        .filter_map(|record| record.eid.and_then(|eid| interner.resolve(eid)))
        .map(str::to_ascii_lowercase)
        .collect::<HashSet<_>>();
    for plan in &plans {
        if plan.requirement.behavior_path.trim().is_empty()
            || plan.requirement.animation_event.trim().is_empty()
            || plan.requirement.parent_form_id == 0
            || plan.requirement.parent_form_id > 0x00ff_ffff
        {
            return Err(invalid(
                "actor_action_requirement",
                format!("invalid Actor Action requirement {:?}", plan.requirement),
            ));
        }
        if !seen_requirements.insert(plan.requirement.clone()) {
            return Err(invalid(
                "duplicate_actor_action_requirement",
                format!("duplicate Actor Action requirement {:?}", plan.requirement),
            ));
        }
        validate_target_form_key(
            "IDLE",
            &plan.form_key,
            &closure.closure.target_plugin,
            &mut seen_form_keys,
        )?;
        validate_editor_id("IDLE", &plan.editor_id, "", &mut seen_editor_ids)?;
    }

    let mut previous_by_parent = std::collections::BTreeMap::<u32, FormKey>::new();
    let mut records = Vec::with_capacity(plans.len());
    for plan in &plans {
        let parent = plan.requirement.parent_form_key().intern(interner);
        let previous = previous_by_parent
            .get(&plan.requirement.parent_form_id)
            .copied()
            .map(FieldValue::FormKey)
            .unwrap_or(FieldValue::Uint(0));
        let form_key = plan.form_key.intern(interner);
        records.push(make_record(
            "IDLE",
            form_key,
            &plan.editor_id,
            vec![
                string_field("EDID", &plan.editor_id, interner),
                string_field("DNAM", &plan.requirement.behavior_path, interner),
                string_field("ENAM", &plan.requirement.animation_event, interner),
                struct_field(
                    "ANAM",
                    vec![
                        ("parent", FieldValue::FormKey(parent)),
                        ("previous", previous),
                    ],
                    interner,
                ),
                struct_field(
                    "DATA",
                    vec![
                        ("looping_seconds_both_255_forever_min", FieldValue::Uint(0)),
                        ("looping_seconds_both_255_forever_max", FieldValue::Uint(0)),
                        ("flags", FieldValue::Uint(0)),
                        ("animation_group_section", FieldValue::Uint(0)),
                        ("replay_delay", FieldValue::Uint(0)),
                    ],
                    interner,
                ),
            ],
            interner,
        ));
        previous_by_parent.insert(plan.requirement.parent_form_id, form_key);
    }
    let schema = AuthoringSchema::for_game("fo4").map_err(CreatureRecordError::Schema)?;
    normalize_records(&mut records, &schema, interner)?;
    closure.closure.records.extend(records);
    closure
        .required_target_records
        .extend(plans.iter().map(|plan| CreatureTargetRecordReference {
            signature: "AACT".to_string(),
            form_key: plan.requirement.parent_form_key(),
        }));
    closure.required_target_records.sort_by_key(|reference| {
        (
            reference.form_key.plugin.to_ascii_lowercase(),
            reference.form_key.local,
            reference.signature.clone(),
        )
    });
    closure.required_target_records.dedup();
    closure.closure.records.sort_by_key(|record| {
        (
            record.sig.as_str().to_string(),
            interner
                .resolve(record.form_key.plugin)
                .unwrap_or_default()
                .to_ascii_lowercase(),
            record.form_key.local,
        )
    });
    validate_record_set(
        &closure.closure,
        closure.closure.records.len(),
        &schema,
        interner,
        &closure.required_target_records,
    )
}

fn emit_creature_record_projection_inner(
    rig: &CreatureManifest,
    graph: &ProjectionGraphContract,
    projection: &CreatureRecordProjectionManifest,
    interner: &StringInterner,
) -> Result<CreatureRecordProjectionClosure, CreatureRecordError> {
    let (primary_variant, primary_attack, primary_profile) =
        validate_projection_inputs(rig, graph, projection)?;
    let has_melee_attack = projection.attacks.iter().any(|attack| {
        matches!(
            attack.projection,
            CreatureAttackRecordProjection::MeleeUnarmed { .. }
        )
    });
    validate_common_inputs(rig, &projection.base, &primary_profile, has_melee_attack)?;

    let custom = InternedRecordKeys::new(&projection.base.form_keys, interner);
    let fallout4 = interner.intern(FALLOUT4_MASTER);
    let mut race = emit_race(
        rig,
        primary_attack
            .map(|attack| attack.event.as_str())
            .unwrap_or(""),
        &projection.base,
        &primary_profile,
        &custom,
        interner,
    );
    apply_race_projection(&mut race, projection, interner);

    let mut variants = projection.variants.iter().collect::<Vec<_>>();
    variants.sort_by_key(|variant| {
        (
            !variant.primary,
            variant.source_identity.stable_key(),
            variant.editor_id.to_ascii_lowercase(),
        )
    });
    let mut attacks = projection.attacks.iter().collect::<Vec<_>>();
    attacks.sort_by_key(|attack| {
        (
            !attack.primary,
            attack.id.to_ascii_lowercase(),
            attack.projection.kind_name(),
        )
    });

    let emitted_attack_count = attacks
        .iter()
        .filter(|attack| {
            matches!(
                attack.projection,
                CreatureAttackRecordProjection::MeleeUnarmed { .. }
            )
        })
        .count();
    let body_nif_parts = projection.effective_body_nif_parts();
    let mut records =
        Vec::with_capacity(3 + variants.len() + body_nif_parts.len() + emitted_attack_count);
    records.push(race);
    for variant in &variants {
        let inventory = variant_inventory(projection, variant, &attacks);
        let spells = variant_npc_spells(projection, variant);
        let death_item = variant_npc_death_item(projection, variant);
        records.push(emit_npc_record_variant(
            variant,
            custom.race,
            custom.skin,
            &inventory,
            spells,
            death_item,
            interner,
        ));
    }
    records.push(emit_skin_parts(
        &projection.base,
        &custom,
        &body_nif_parts,
        interner,
    ));
    records.extend(
        body_nif_parts
            .iter()
            .map(|part| emit_armor_addon_part(custom.race, part, interner)),
    );
    records.push(emit_body_part_data(
        rig,
        &projection.base,
        &primary_profile,
        &custom,
        interner,
    ));
    for attack in &attacks {
        if matches!(
            attack.projection,
            CreatureAttackRecordProjection::MeleeUnarmed { .. }
        ) {
            records.push(emit_attack_record_variant(attack, fallout4, interner));
        }
    }

    let schema = AuthoringSchema::for_game("fo4").map_err(CreatureRecordError::Schema)?;
    normalize_records(&mut records, &schema, interner)?;
    let closure = CreatureRecordClosure {
        target_plugin: projection.base.target_plugin.clone(),
        records,
    };
    validate_projection_closure(
        &closure,
        rig,
        graph,
        projection,
        primary_variant,
        primary_attack,
        &schema,
        interner,
    )?;

    Ok(CreatureRecordProjectionClosure {
        source_primary_identity: projection.source_primary_identity.clone(),
        projected_identities: variants
            .into_iter()
            .map(|variant| ProjectedRecordIdentity {
                signature: "NPC_".to_string(),
                source_identity: variant.source_identity.clone(),
                target_form_key: variant.form_key.clone(),
                primary: variant.primary,
            })
            .collect(),
        required_target_records: projection_target_dependencies(projection),
        closure,
    })
}

fn normalize_records(
    records: &mut [Record],
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    let normalizer = TargetRecordNormalizer::target_only_with_interner(schema, interner);
    for record in records {
        let signature = record.sig.as_str().to_string();
        let before = record
            .fields
            .iter()
            .map(|field| field.sig.as_str().to_string())
            .collect::<Vec<_>>();
        let inventory_ownership_pattern = (signature == "NPC_").then(|| {
            record
                .fields
                .iter()
                .enumerate()
                .filter(|(_, field)| field.sig.as_str() == "CNTO")
                .map(|(index, _)| {
                    record
                        .fields
                        .get(index + 1)
                        .is_some_and(|field| field.sig.as_str() == "COED")
                })
                .collect::<Vec<_>>()
        });
        let mut normalized = match normalizer.normalize(record.clone()) {
            TargetRecordNormalization::Keep(record) => record,
            TargetRecordNormalization::DropUnsupportedRecord => {
                return Err(CreatureRecordError::UnsupportedRecord(signature));
            }
        };
        if let Some(pattern) = inventory_ownership_pattern {
            restore_npc_inventory_associations(&mut normalized, &pattern)?;
        }
        let after = normalized
            .fields
            .iter()
            .map(|field| field.sig.as_str().to_string())
            .collect::<Vec<_>>();
        if after.len() != before.len() {
            return Err(CreatureRecordError::NormalizationLoss {
                record: signature,
                before,
                after,
            });
        }
        *record = normalized;
    }
    Ok(())
}

fn restore_npc_inventory_associations(
    record: &mut Record,
    ownership_pattern: &[bool],
) -> Result<(), CreatureRecordError> {
    let first_inventory_index = record
        .fields
        .iter()
        .position(|field| matches!(field.sig.as_str(), "CNTO" | "COED"));
    let Some(first_inventory_index) = first_inventory_index else {
        return if ownership_pattern.is_empty() {
            Ok(())
        } else {
            Err(invalid(
                "npc_inventory_normalization",
                "target normalization removed every NPC_ inventory row",
            ))
        };
    };
    let insertion_index = record.fields[..first_inventory_index]
        .iter()
        .filter(|field| !matches!(field.sig.as_str(), "CNTO" | "COED"))
        .count();
    let mut inventory = Vec::new();
    let mut other = SmallVec::<[FieldEntry; 8]>::new();
    for field in record.fields.drain(..) {
        if matches!(field.sig.as_str(), "CNTO" | "COED") {
            inventory.push(field);
        } else {
            other.push(field);
        }
    }
    let mut cnto = inventory
        .iter()
        .filter(|field| field.sig.as_str() == "CNTO")
        .cloned();
    let mut coed = inventory
        .iter()
        .filter(|field| field.sig.as_str() == "COED")
        .cloned();
    let mut restored = Vec::with_capacity(inventory.len());
    for has_ownership in ownership_pattern {
        restored.push(cnto.next().ok_or_else(|| {
            invalid(
                "npc_inventory_normalization",
                "target normalization changed the NPC_ CNTO row count",
            )
        })?);
        if *has_ownership {
            restored.push(coed.next().ok_or_else(|| {
                invalid(
                    "npc_inventory_normalization",
                    "target normalization changed the NPC_ COED row count",
                )
            })?);
        }
    }
    if cnto.next().is_some() || coed.next().is_some() {
        return Err(invalid(
            "npc_inventory_normalization",
            "target normalization introduced an NPC_ CNTO or COED row",
        ));
    }
    for (offset, field) in restored.into_iter().enumerate() {
        other.insert(insertion_index + offset, field);
    }
    record.fields = other;
    Ok(())
}

#[derive(Clone, Debug)]
struct ProjectionGraphContract {
    template: CreatureGraphTemplate,
    declared_events: std::collections::BTreeMap<String, EventUsage>,
    event_roles: std::collections::BTreeMap<String, CreatureClipRole>,
    candidate_attack_roles: std::collections::BTreeMap<(String, String), CreatureClipRole>,
    candidate_bound_events: std::collections::BTreeSet<String>,
    required_primary_event: Option<String>,
}

impl ProjectionGraphContract {
    fn from_mvp(graph: &MvpGraphManifest) -> Self {
        Self {
            template: CreatureGraphTemplate::GroundMelee,
            declared_events: std::collections::BTreeMap::from([(
                graph.melee_event.to_ascii_lowercase(),
                EventUsage::MeleeAttack,
            )]),
            event_roles: std::collections::BTreeMap::from([(
                graph.melee_event.to_ascii_lowercase(),
                CreatureClipRole::MeleeAttack,
            )]),
            candidate_attack_roles: std::collections::BTreeMap::new(),
            candidate_bound_events: std::collections::BTreeSet::new(),
            required_primary_event: Some(graph.melee_event.clone()),
        }
    }

    fn from_capability(graph: &CapabilityGraphManifest) -> Self {
        let declared_events = graph
            .explicit_events
            .iter()
            .map(|event| (event.name.to_ascii_lowercase(), event.usage))
            .collect();
        let event_roles = graph
            .roles
            .iter()
            .flat_map(|role| {
                role.trigger_event
                    .iter()
                    .chain(&role.trigger_aliases)
                    .map(|event| (event.to_ascii_lowercase(), role.role))
            })
            .collect();
        let candidate_attack_roles = graph
            .candidate_attack_bindings
            .iter()
            .map(|binding| {
                (
                    (
                        binding.source_identity.stable_key(),
                        binding.event.to_ascii_lowercase(),
                    ),
                    binding.role,
                )
            })
            .collect();
        let candidate_bound_events = graph
            .candidate_attack_bindings
            .iter()
            .map(|binding| binding.event.to_ascii_lowercase())
            .collect();
        Self {
            template: graph.template,
            declared_events,
            event_roles,
            candidate_attack_roles,
            candidate_bound_events,
            required_primary_event: None,
        }
    }

    fn validates(
        &self,
        source_identity: &SourceCreatureIdentity,
        attack: &CreatureAttackRecordVariant,
    ) -> bool {
        let event = attack.event.to_ascii_lowercase();
        let declared_usage = self.declared_events.get(&event);
        let role = self.event_roles.get(&event);
        if self.candidate_bound_events.contains(&event) {
            let candidate_role = self
                .candidate_attack_roles
                .get(&(source_identity.stable_key(), event.clone()));
            let exact_binding = candidate_role.is_some_and(|candidate_role| {
                declared_usage == Some(&EventUsage::Generic)
                    && role.is_some_and(|role| {
                        matches!(
                            role,
                            CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
                        )
                    })
                    && attack_projection_role(&attack.projection) == Some(*candidate_role)
                    && projection_template_allows_role(self.template, *candidate_role)
            });
            if exact_binding {
                return true;
            }
            let generated_or_source_melee =
                attack
                    .target_data
                    .source_atkd
                    .as_ref()
                    .is_none_or(|source| {
                        source.attack_spell.is_none()
                            && matches!(
                                &source.attack_spell_policy,
                                CreatureAttackSpellPolicy::NoSourceSpell
                            )
                    });
            return matches!(
                attack.projection,
                CreatureAttackRecordProjection::MeleeUnarmed { .. }
            ) && generated_or_source_melee
                && matches!(
                    declared_usage,
                    Some(EventUsage::MeleeAttack | EventUsage::Generic)
                )
                && role.is_some_and(|role| {
                    matches!(
                        role,
                        CreatureClipRole::MeleeAttack | CreatureClipRole::ProjectileAttack
                    )
                })
                && projection_template_allows_role(self.template, CreatureClipRole::MeleeAttack);
        }
        match (&attack.projection, self.template) {
            (
                CreatureAttackRecordProjection::MeleeUnarmed { .. }
                | CreatureAttackRecordProjection::MeleeEquipment { .. },
                CreatureGraphTemplate::GroundMelee
                | CreatureGraphTemplate::GroundMeleeRanged
                | CreatureGraphTemplate::GroundSwim
                | CreatureGraphTemplate::GroundFly
                | CreatureGraphTemplate::Swim
                | CreatureGraphTemplate::Fly,
            ) => {
                matches!(
                    declared_usage,
                    Some(EventUsage::MeleeAttack | EventUsage::Generic)
                ) && role == Some(&CreatureClipRole::MeleeAttack)
            }
            (
                CreatureAttackRecordProjection::RangedProjectile { .. }
                | CreatureAttackRecordProjection::RangedEquipment { .. }
                | CreatureAttackRecordProjection::SpellAbility { .. },
                CreatureGraphTemplate::GroundRangedProjectile
                | CreatureGraphTemplate::GroundMeleeRanged
                | CreatureGraphTemplate::GroundSwim
                | CreatureGraphTemplate::GroundFly
                | CreatureGraphTemplate::Swim
                | CreatureGraphTemplate::Fly,
            ) => {
                declared_usage == Some(&EventUsage::Generic)
                    && role == Some(&CreatureClipRole::ProjectileAttack)
            }
            (
                CreatureAttackRecordProjection::RangedEquipment { .. },
                CreatureGraphTemplate::StationaryTurret,
            ) => {
                declared_usage == Some(&EventUsage::Generic)
                    && role == Some(&CreatureClipRole::ProjectileAttack)
            }
            (
                CreatureAttackRecordProjection::RangedEquipment { .. },
                CreatureGraphTemplate::RobotContinuousAttack,
            ) => {
                declared_usage == Some(&EventUsage::Generic)
                    && role == Some(&CreatureClipRole::ContinuousAttackStart)
            }
            (
                CreatureAttackRecordProjection::Stationary { .. },
                CreatureGraphTemplate::StationaryTurret,
            ) => {
                declared_usage == Some(&EventUsage::Generic)
                    && role == Some(&CreatureClipRole::ProjectileAttack)
            }
            (
                CreatureAttackRecordProjection::ContinuousRobot { .. },
                CreatureGraphTemplate::RobotContinuousAttack,
            ) => {
                declared_usage == Some(&EventUsage::Generic)
                    && role == Some(&CreatureClipRole::ContinuousAttackStart)
            }
            _ => false,
        }
    }
}

fn attack_projection_role(projection: &CreatureAttackRecordProjection) -> Option<CreatureClipRole> {
    match projection {
        CreatureAttackRecordProjection::MeleeUnarmed { .. }
        | CreatureAttackRecordProjection::MeleeEquipment { .. } => {
            Some(CreatureClipRole::MeleeAttack)
        }
        CreatureAttackRecordProjection::RangedProjectile { .. }
        | CreatureAttackRecordProjection::RangedEquipment { .. }
        | CreatureAttackRecordProjection::SpellAbility { .. }
        | CreatureAttackRecordProjection::Stationary { .. } => {
            Some(CreatureClipRole::ProjectileAttack)
        }
        CreatureAttackRecordProjection::ContinuousRobot { .. } => None,
    }
}

fn projection_template_allows_role(
    template: CreatureGraphTemplate,
    role: CreatureClipRole,
) -> bool {
    match role {
        CreatureClipRole::MeleeAttack => matches!(
            template,
            CreatureGraphTemplate::GroundMelee
                | CreatureGraphTemplate::GroundMeleeRanged
                | CreatureGraphTemplate::GroundSwim
                | CreatureGraphTemplate::GroundFly
                | CreatureGraphTemplate::Swim
                | CreatureGraphTemplate::Fly
        ),
        CreatureClipRole::ProjectileAttack => matches!(
            template,
            CreatureGraphTemplate::GroundRangedProjectile
                | CreatureGraphTemplate::GroundMeleeRanged
                | CreatureGraphTemplate::GroundSwim
                | CreatureGraphTemplate::GroundFly
                | CreatureGraphTemplate::Swim
                | CreatureGraphTemplate::Fly
                | CreatureGraphTemplate::StationaryTurret
        ),
        _ => false,
    }
}

fn validate_projection_inputs<'a>(
    _rig: &CreatureManifest,
    graph: &ProjectionGraphContract,
    projection: &'a CreatureRecordProjectionManifest,
) -> Result<
    (
        &'a CreatureNpcRecordVariant,
        Option<&'a CreatureAttackRecordVariant>,
        CreatureRecordProfile,
    ),
    CreatureRecordError,
> {
    projection
        .race_data
        .validate()
        .map_err(|error| CreatureRecordError::UnsupportedField {
            record: "RACE",
            field: "DATA",
            reason: error.to_string(),
        })?;
    if !projection.source_primary_identity.is_valid() {
        return Err(invalid(
            "source_primary_identity",
            "the source primary identity is incomplete",
        ));
    }

    let primary_variants = projection
        .variants
        .iter()
        .filter(|variant| variant.primary)
        .collect::<Vec<_>>();
    let [primary_variant] = primary_variants.as_slice() else {
        return Err(invalid(
            "primary_record_variant",
            format!(
                "expected exactly one primary NPC variant, got {}",
                primary_variants.len()
            ),
        ));
    };
    if primary_variant.source_identity != projection.source_primary_identity {
        return Err(invalid(
            "primary_record_variant",
            format!(
                "primary NPC source {} does not match corpus source {}",
                primary_variant.source_identity.stable_key(),
                projection.source_primary_identity.stable_key()
            ),
        ));
    }

    let primary_attacks = projection
        .attacks
        .iter()
        .filter(|attack| attack.primary)
        .collect::<Vec<_>>();
    let primary_attack = if graph.template == CreatureGraphTemplate::PassiveGround {
        if !projection.attacks.is_empty() {
            return Err(invalid(
                "passive_attack_mapping",
                "a PassiveGround projection must not declare attacks",
            ));
        }
        None
    } else {
        let [primary_attack] = primary_attacks.as_slice() else {
            return Err(invalid(
                "primary_attack_variant",
                format!(
                    "expected exactly one primary attack, got {}",
                    primary_attacks.len()
                ),
            ));
        };
        Some(*primary_attack)
    };
    if graph
        .required_primary_event
        .as_ref()
        .is_some_and(|event| primary_attack.is_none_or(|attack| attack.event != *event))
    {
        return Err(invalid(
            "primary_attack_variant",
            format!(
                "primary attack event {:?} does not match graph event {:?}",
                primary_attack.map(|attack| attack.event.as_str()),
                graph.required_primary_event
            ),
        ));
    }
    if primary_variant.form_key != projection.base.form_keys.npc
        || primary_variant.editor_id != projection.base.editor_ids.npc
    {
        return Err(invalid(
            "primary_target_mapping",
            "the primary NPC target must match the base NPC_ identity",
        ));
    }
    if let Some(CreatureAttackRecordVariant {
        projection:
            CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key,
                weapon_editor_id,
                ..
            },
        ..
    }) = primary_attack
        && (weapon_form_key != &projection.base.form_keys.unarmed_weapon
            || weapon_editor_id != &projection.base.editor_ids.unarmed_weapon)
    {
        return Err(invalid(
            "primary_target_mapping",
            "a primary melee attack must match the base unarmed WEAP identity",
        ));
    }
    let mut attack_ids = HashSet::new();
    let mut attack_events = HashSet::new();
    for attack in &projection.attacks {
        if attack.id.trim().is_empty()
            || attack.event.trim().is_empty()
            || !graph.validates(&projection.source_primary_identity, attack)
        {
            return Err(invalid(
                "attack_mapping",
                format!(
                    "attack {:?} ({}) does not match declared {:?} graph event {:?}",
                    attack.id,
                    attack.projection.kind_name(),
                    graph.template,
                    attack.event,
                ),
            ));
        }
        if !attack_ids.insert(attack.id.to_ascii_lowercase())
            || !attack_events.insert(attack.event.to_ascii_lowercase())
        {
            return Err(invalid(
                "duplicate_attack_mapping",
                format!("duplicate attack id/event for {:?}", attack.id),
            ));
        }
        for (field, value) in [
            ("damage_multiplier", attack.damage_multiplier),
            ("chance", attack.chance),
            ("strike_angle", attack.strike_angle),
            ("action_point_cost", attack.action_point_cost),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(invalid(
                    "attack_numeric_profile",
                    format!("{}.{} must be finite and non-negative", attack.id, field),
                ));
            }
        }
        for (field, value) in [
            ("attack_angle", attack.target_data.attack_angle),
            ("stagger", attack.target_data.stagger),
            ("knockdown", attack.target_data.knockdown),
            ("recovery_time", attack.target_data.recovery_time),
            (
                "action_points_multiplier",
                attack.target_data.action_points_multiplier,
            ),
        ] {
            if !value.is_finite() {
                return Err(invalid(
                    "attack_numeric_profile",
                    format!("{}.{} must be finite", attack.id, field),
                ));
            }
        }
        if let Some(source) = &attack.target_data.source_atkd {
            validate_attack_source_data(attack, source)?;
        }
        validate_attack_projection(attack, &projection.base)?;
    }
    if !projection.npc_inventory.is_empty() && !projection.npc_equipment.is_empty() {
        return Err(invalid(
            "ambiguous_npc_inventory",
            "family-wide exact NPC inventory and legacy equipment are both populated",
        ));
    }
    for entry in &projection.npc_inventory {
        validate_npc_inventory_entry(entry, &projection.base)?;
    }
    let mut spell_keys = HashSet::new();
    for spell in &projection.npc_spells {
        validate_target_record_reference_at(spell, "SPEL", &projection.base, "NPC_", "SPLO")?;
        if !spell_keys.insert((
            spell.form_key.plugin.to_ascii_lowercase(),
            spell.form_key.local,
        )) {
            return Err(invalid(
                "duplicate_npc_spell",
                format!(
                    "NPC spell list repeats {:06X}@{}",
                    spell.form_key.local, spell.form_key.plugin
                ),
            ));
        }
    }
    if let Some(death_item) = &projection.npc_death_item {
        validate_target_record_reference_at(death_item, "LVLI", &projection.base, "NPC_", "INAM")?;
    }
    let mut npc_equipment_keys = HashSet::new();
    for equipment in &projection.npc_equipment {
        validate_target_record_reference_at(
            equipment,
            "WEAP",
            &projection.base,
            "NPC_",
            "CNTO.item",
        )?;
        if !npc_equipment_keys.insert((
            equipment.form_key.plugin.to_ascii_lowercase(),
            equipment.form_key.local,
        )) {
            return Err(invalid(
                "duplicate_npc_equipment",
                format!(
                    "NPC equipment list repeats {:06X}@{}",
                    equipment.form_key.local, equipment.form_key.plugin
                ),
            ));
        }
    }
    for variant in &projection.variants {
        if let Some(inventory) = &variant.npc_inventory {
            if !projection.npc_inventory.is_empty() && inventory != &projection.npc_inventory {
                return Err(invalid(
                    "family_npc_inventory_mismatch",
                    format!(
                        "family-wide NPC inventory differs for {}",
                        variant.source_identity.stable_key()
                    ),
                ));
            }
            for entry in inventory {
                validate_npc_inventory_entry(entry, &projection.base)?;
            }
        }
        if !variant_npc_inventory(projection, variant).is_empty()
            && !variant_npc_equipment(projection, variant).is_empty()
        {
            return Err(invalid(
                "ambiguous_npc_inventory",
                format!(
                    "NPC {} has both exact inventory and legacy equipment",
                    variant.source_identity.stable_key()
                ),
            ));
        }
        if let Some(equipment) = &variant.npc_equipment {
            if !projection.npc_equipment.is_empty() && equipment != &projection.npc_equipment {
                return Err(invalid(
                    "family_npc_equipment_mismatch",
                    format!(
                        "family-wide NPC equipment differs for {}",
                        variant.source_identity.stable_key()
                    ),
                ));
            }
            let mut keys = HashSet::new();
            for reference in equipment {
                validate_target_record_reference_at(
                    reference,
                    "WEAP",
                    &projection.base,
                    "NPC_",
                    "CNTO.item",
                )?;
                if !keys.insert((
                    reference.form_key.plugin.to_ascii_lowercase(),
                    reference.form_key.local,
                )) {
                    return Err(invalid(
                        "duplicate_variant_npc_equipment",
                        format!(
                            "NPC {} equipment repeats {:06X}@{}",
                            variant.source_identity.stable_key(),
                            reference.form_key.local,
                            reference.form_key.plugin
                        ),
                    ));
                }
            }
        }
        if let Some(spells) = &variant.npc_spells {
            if !projection.npc_spells.is_empty() && spells != &projection.npc_spells {
                return Err(invalid(
                    "family_npc_spell_mismatch",
                    format!(
                        "family-wide NPC spell list differs for {}",
                        variant.source_identity.stable_key()
                    ),
                ));
            }
            let mut keys = HashSet::new();
            for reference in spells {
                validate_target_record_reference_at(
                    reference,
                    "SPEL",
                    &projection.base,
                    "NPC_",
                    "SPLO",
                )?;
                if !keys.insert((
                    reference.form_key.plugin.to_ascii_lowercase(),
                    reference.form_key.local,
                )) {
                    return Err(invalid(
                        "duplicate_variant_npc_spell",
                        format!(
                            "NPC {} spell list repeats {:06X}@{}",
                            variant.source_identity.stable_key(),
                            reference.form_key.local,
                            reference.form_key.plugin
                        ),
                    ));
                }
            }
        }
        if let Some(death_item) = &variant.npc_death_item {
            if projection.npc_death_item.is_some()
                && death_item.as_ref() != projection.npc_death_item.as_ref()
            {
                return Err(invalid(
                    "family_npc_death_item_mismatch",
                    format!(
                        "family-wide NPC death item differs for {}",
                        variant.source_identity.stable_key()
                    ),
                ));
            }
            if let Some(death_item) = death_item {
                validate_target_record_reference_at(
                    death_item,
                    "LVLI",
                    &projection.base,
                    "NPC_",
                    "INAM",
                )?;
            }
        }
    }
    let body_nif_parts = projection.effective_body_nif_parts();
    let first_body_part = body_nif_parts
        .first()
        .expect("legacy single-body fallback always supplies one part");
    if first_body_part.body_nif != projection.base.body_nif
        || first_body_part.armor_addon_form_key != projection.base.form_keys.armor_addon
        || first_body_part.armor_addon_editor_id != projection.base.editor_ids.armor_addon
    {
        return Err(invalid(
            "primary_body_nif_part",
            "the first body NIF part must exactly match the legacy base ARMA identity",
        ));
    }
    for part in &body_nif_parts {
        if part.body_nif.trim().is_empty() || !part.body_nif.to_ascii_lowercase().ends_with(".nif")
        {
            return Err(invalid(
                "body_nif_part",
                format!("body NIF part {:?} is not a target NIF path", part.body_nif),
            ));
        }
    }

    let helper_form_keys = [
        ("RACE", &projection.base.form_keys.race),
        ("ARMO", &projection.base.form_keys.skin),
        ("BPTD", &projection.base.form_keys.body_part_data),
    ];
    let helper_editor_ids = [
        ("RACE", projection.base.editor_ids.race.as_str()),
        ("ARMO", projection.base.editor_ids.skin.as_str()),
        ("BPTD", projection.base.editor_ids.body_part_data.as_str()),
    ];
    let mut form_keys = HashSet::new();
    let mut editor_ids = HashSet::new();
    for (signature, form_key) in helper_form_keys
        .into_iter()
        .chain(
            body_nif_parts
                .iter()
                .map(|part| ("ARMA", &part.armor_addon_form_key)),
        )
        .chain(
            projection
                .variants
                .iter()
                .map(|variant| ("NPC_", &variant.form_key)),
        )
        .chain(
            projection
                .attacks
                .iter()
                .filter_map(|attack| match &attack.projection {
                    CreatureAttackRecordProjection::MeleeUnarmed {
                        weapon_form_key, ..
                    } => Some(("WEAP", weapon_form_key)),
                    _ => None,
                }),
        )
    {
        validate_target_form_key(
            signature,
            form_key,
            &projection.base.target_plugin,
            &mut form_keys,
        )?;
    }
    for (signature, editor_id) in helper_editor_ids
        .into_iter()
        .chain(
            body_nif_parts
                .iter()
                .map(|part| ("ARMA", part.armor_addon_editor_id.as_str())),
        )
        .chain(
            projection
                .variants
                .iter()
                .map(|variant| ("NPC_", variant.editor_id.as_str())),
        )
        .chain(
            projection
                .attacks
                .iter()
                .filter_map(|attack| match &attack.projection {
                    CreatureAttackRecordProjection::MeleeUnarmed {
                        weapon_editor_id, ..
                    } => Some(("WEAP", weapon_editor_id.as_str())),
                    _ => None,
                }),
        )
    {
        validate_editor_id(
            signature,
            editor_id,
            &projection.base.editor_id_prefix,
            &mut editor_ids,
        )?;
    }

    let (unarmed_damage, unarmed_reach, unarmed_attack_seconds) =
        match primary_attack.map(|attack| &attack.projection) {
            Some(CreatureAttackRecordProjection::MeleeUnarmed {
                damage,
                reach,
                attack_seconds,
                ..
            }) => (*damage, *reach, *attack_seconds),
            _ => (0, 0.0, 0.0),
        };
    let primary_profile = CreatureRecordProfile {
        root_body_part: projection.body_parts.profile(),
        attack_damage_multiplier: primary_attack.map_or(0.0, |attack| attack.damage_multiplier),
        attack_chance: primary_attack.map_or(0.0, |attack| attack.chance),
        attack_strike_angle: primary_attack.map_or(0.0, |attack| attack.strike_angle),
        unarmed_damage,
        unarmed_reach,
        unarmed_attack_seconds,
        action_point_cost: primary_attack.map_or(0.0, |attack| attack.action_point_cost),
        npc_level: primary_variant.level,
        npc_health: primary_variant.health,
        npc_action_points: primary_variant.action_points,
    };
    Ok((primary_variant, primary_attack, primary_profile))
}

fn validate_attack_source_data(
    attack: &CreatureAttackRecordVariant,
    source: &CreatureAttackSourceDataReceipt,
) -> Result<(), CreatureRecordError> {
    if source.schema_id.trim().is_empty()
        || source.schema_id.contains('/')
        || source.schema_id.contains('\\')
        || source.schema_id.contains('@')
    {
        return Err(invalid(
            "attack_source_schema",
            format!(
                "{} has invalid ATKD schema id {:?}",
                attack.id, source.schema_id
            ),
        ));
    }
    if source.event != attack.event {
        return Err(invalid(
            "attack_source_event",
            format!(
                "{} source event {:?} differs from projected event {:?}",
                attack.id, source.event, attack.event
            ),
        ));
    }
    for (field, expected, actual) in [
        (
            "damage_multiplier",
            source.damage_multiplier_bits,
            attack.damage_multiplier.to_bits(),
        ),
        ("chance", source.chance_bits, attack.chance.to_bits()),
        (
            "attack_angle",
            source.attack_angle_bits,
            attack.target_data.attack_angle.to_bits(),
        ),
        (
            "strike_angle",
            source.strike_angle_bits,
            attack.strike_angle.to_bits(),
        ),
        (
            "stagger",
            source.stagger_bits,
            attack.target_data.stagger.to_bits(),
        ),
        (
            "knockdown",
            source.knockdown_bits,
            attack.target_data.knockdown.to_bits(),
        ),
        (
            "recovery_time",
            source.recovery_time_bits,
            attack.target_data.recovery_time.to_bits(),
        ),
    ] {
        if expected != actual {
            return Err(invalid(
                "attack_source_numeric",
                format!(
                    "{}.{} source bits do not match the target projection",
                    attack.id, field
                ),
            ));
        }
    }
    if source.attack_flags != attack.target_data.attack_flags {
        return Err(invalid(
            "attack_source_flags",
            format!(
                "{} source flags do not match the target projection",
                attack.id
            ),
        ));
    }
    if let Some(attack_type) = &source.attack_type {
        validate_source_record_reference(attack_type, &["KYWD"], attack, "attack_type")?;
    }
    if let Some(attack_spell) = &source.attack_spell {
        validate_source_record_reference(attack_spell, &["SPEL", "SHOU"], attack, "attack_spell")?;
    }
    let target_spell = match &attack.projection {
        CreatureAttackRecordProjection::RangedProjectile { attack_spell, .. }
        | CreatureAttackRecordProjection::Stationary { attack_spell, .. }
        | CreatureAttackRecordProjection::ContinuousRobot { attack_spell, .. } => {
            Some(attack_spell)
        }
        CreatureAttackRecordProjection::SpellAbility { spell } => Some(spell),
        CreatureAttackRecordProjection::MeleeUnarmed { .. }
        | CreatureAttackRecordProjection::MeleeEquipment { .. }
        | CreatureAttackRecordProjection::RangedEquipment { .. } => None,
    };
    let valid_attack_type_policy = match (&source.attack_type_policy, source.attack_type.as_ref()) {
        (CreatureAttackTypePolicy::NullOmitted, None) => true,
        (CreatureAttackTypePolicy::RuntimeInertKeyword { proof }, Some(_)) => {
            validate_semantic_proof(
                proof,
                attack,
                "attack_type",
                ATTACK_TYPE_RUNTIME_INERT_PROOF_SCHEMA,
                "runtime_inert_keyword",
            )?;
            true
        }
        _ => false,
    };
    if !valid_attack_type_policy {
        return Err(invalid(
            "attack_type_policy",
            format!("{} attack-type source and typed policy disagree", attack.id),
        ));
    }
    let valid_attack_spell_policy = match (
        &source.attack_spell_policy,
        source.attack_spell.as_ref(),
        target_spell,
    ) {
        (CreatureAttackSpellPolicy::NoSourceSpell, None, None) => true,
        (CreatureAttackSpellPolicy::DirectSpell, Some(source), Some(_)) => {
            source.signature == "SPEL"
        }
        (CreatureAttackSpellPolicy::LowerShoutToSpell { proof }, Some(source), Some(_)) => {
            validate_semantic_proof(
                proof,
                attack,
                "attack_spell",
                ATTACK_SHOUT_TO_SPELL_PROOF_SCHEMA,
                "lower_shout_to_spell",
            )?;
            source.signature == "SHOU"
        }
        _ => false,
    };
    if !valid_attack_spell_policy {
        return Err(invalid(
            "attack_spell_policy",
            format!(
                "{} attack-spell source, target, and typed policy disagree",
                attack.id
            ),
        ));
    }
    match &source.stamina_multiplier_policy {
        CreatureAttackStaminaPolicy::PreserveAsActionPointsMultiplier { proof }
            if source.stamina_multiplier_bits
                == attack.target_data.action_points_multiplier.to_bits() =>
        {
            validate_semantic_proof(
                proof,
                attack,
                "stamina_multiplier",
                ATTACK_STAMINA_TO_ACTION_POINTS_PROOF_SCHEMA,
                "preserve_as_action_points_multiplier",
            )?;
        }
        CreatureAttackStaminaPolicy::PreserveAsActionPointsMultiplier { .. } => {
            return Err(invalid(
                "attack_stamina_policy",
                format!(
                    "{} preserve policy requires identical stamina/AP multiplier bits",
                    attack.id
                ),
            ));
        }
    }
    match &source.stagger_offset_policy {
        CreatureAttackStaggerOffsetPolicy::ExplicitZero { proof }
            if attack.target_data.stagger_offset == 0 =>
        {
            validate_semantic_proof(
                proof,
                attack,
                "stagger_offset",
                ATTACK_STAGGER_OFFSET_ZERO_PROOF_SCHEMA,
                "explicit_zero",
            )?;
        }
        CreatureAttackStaggerOffsetPolicy::ExplicitZero { .. } => {
            return Err(invalid(
                "attack_stagger_offset_policy",
                format!(
                    "{} explicit-zero policy requires stagger_offset=0",
                    attack.id
                ),
            ));
        }
    }
    Ok(())
}

fn validate_semantic_proof(
    proof: &CreatureSemanticProofReceipt,
    attack: &CreatureAttackRecordVariant,
    field: &'static str,
    expected_schema: &'static str,
    expected_policy: &'static str,
) -> Result<(), CreatureRecordError> {
    if proof.schema != expected_schema
        || proof.canonical_json.trim().is_empty()
        || proof.canonical_json_blake3.len() != 64
        || !proof
            .canonical_json_blake3
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid(
            "attack_semantic_proof",
            format!(
                "{}.{} has an invalid semantic proof identity",
                attack.id, field
            ),
        ));
    }
    let document =
        serde_json::from_str::<serde_json::Value>(&proof.canonical_json).map_err(|error| {
            invalid(
                "attack_semantic_proof",
                format!(
                    "{}.{} proof is not canonical JSON: {error}",
                    attack.id, field
                ),
            )
        })?;
    let canonical = serde_json::to_string(&document)
        .expect("a parsed semantic-proof JSON value must be serializable");
    if canonical != proof.canonical_json
        || document.get("field").and_then(serde_json::Value::as_str) != Some(field)
        || document.get("policy").and_then(serde_json::Value::as_str) != Some(expected_policy)
    {
        return Err(invalid(
            "attack_semantic_proof",
            format!(
                "{}.{} proof does not match the recognized schema policy",
                attack.id, field
            ),
        ));
    }
    let valid_document = match expected_schema {
        ATTACK_TYPE_RUNTIME_INERT_PROOF_SCHEMA => {
            document.as_object().is_some_and(|object| object.len() == 4)
                && document
                    .get("source_signature")
                    .and_then(serde_json::Value::as_str)
                    == Some("KYWD")
                && valid_proof_evidence_hash(&document, "evidence_blake3")
        }
        ATTACK_SHOUT_TO_SPELL_PROOF_SCHEMA => {
            document.as_object().is_some_and(|object| object.len() == 5)
                && document
                    .get("source_signature")
                    .and_then(serde_json::Value::as_str)
                    == Some("SHOU")
                && document
                    .get("target_signature")
                    .and_then(serde_json::Value::as_str)
                    == Some("SPEL")
                && valid_proof_evidence_hash(&document, "lowering_receipt_blake3")
        }
        ATTACK_STAMINA_TO_ACTION_POINTS_PROOF_SCHEMA => {
            document.as_object().is_some_and(|object| object.len() == 4)
                && document.get("relation").and_then(serde_json::Value::as_str)
                    == Some("bit_identical")
                && valid_proof_evidence_hash(&document, "evidence_blake3")
        }
        ATTACK_STAGGER_OFFSET_ZERO_PROOF_SCHEMA => {
            document.as_object().is_some_and(|object| object.len() == 4)
                && document
                    .get("target_value")
                    .and_then(serde_json::Value::as_i64)
                    == Some(0)
                && valid_proof_evidence_hash(&document, "evidence_blake3")
        }
        _ => false,
    };
    if !valid_document {
        return Err(invalid(
            "attack_semantic_proof",
            format!("{}.{} proof payload is incomplete", attack.id, field),
        ));
    }
    let actual = blake3::hash(proof.canonical_json.as_bytes()).to_hex();
    if !proof
        .canonical_json_blake3
        .eq_ignore_ascii_case(actual.as_str())
    {
        return Err(invalid(
            "attack_semantic_proof",
            format!("{}.{} proof hash does not match its JSON", attack.id, field),
        ));
    }
    Ok(())
}

fn valid_proof_evidence_hash(document: &serde_json::Value, field: &str) -> bool {
    document
        .get(field)
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| {
            value.len() == 64
                && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                && value.bytes().all(|byte| !byte.is_ascii_uppercase())
        })
}

fn validate_source_record_reference(
    reference: &CreatureSourceRecordReference,
    expected_signatures: &[&str],
    attack: &CreatureAttackRecordVariant,
    field: &'static str,
) -> Result<(), CreatureRecordError> {
    if !reference.source_identity.is_valid()
        || !expected_signatures.contains(&reference.signature.as_str())
    {
        return Err(invalid(
            "attack_source_reference",
            format!(
                "{}.{} has invalid source {:?} signature {:?}",
                attack.id, field, reference.source_identity, reference.signature
            ),
        ));
    }
    Ok(())
}

fn validate_attack_projection(
    attack: &CreatureAttackRecordVariant,
    manifest: &CreatureRecordManifest,
) -> Result<(), CreatureRecordError> {
    match &attack.projection {
        CreatureAttackRecordProjection::MeleeUnarmed {
            reach,
            attack_seconds,
            ..
        } => {
            for (field, value) in [("reach", *reach), ("attack_seconds", *attack_seconds)] {
                if !value.is_finite() || value < 0.0 {
                    return Err(invalid(
                        "attack_numeric_profile",
                        format!("{}.{} must be finite and non-negative", attack.id, field),
                    ));
                }
            }
        }
        CreatureAttackRecordProjection::MeleeEquipment { equipment } => {
            if equipment.is_empty() {
                return Err(invalid(
                    "attack_equipment_empty",
                    format!("attack {:?} has no evidenced melee equipment", attack.id),
                ));
            }
            for reference in equipment {
                validate_target_record_reference(reference, "WEAP", manifest)?;
            }
            let references = equipment.iter().collect::<Vec<_>>();
            validate_distinct_attack_dependency_slice(attack, &references)?;
        }
        CreatureAttackRecordProjection::RangedProjectile {
            attack_spell,
            projectile,
            equipment,
        }
        | CreatureAttackRecordProjection::Stationary {
            attack_spell,
            projectile,
            equipment,
        } => {
            validate_target_record_reference(attack_spell, "SPEL", manifest)?;
            validate_target_record_reference(projectile, "PROJ", manifest)?;
            validate_target_record_reference(equipment, "WEAP", manifest)?;
            validate_distinct_attack_dependencies(attack, [attack_spell, projectile, equipment])?;
        }
        CreatureAttackRecordProjection::RangedEquipment {
            equipment,
            projectile,
            ammunition,
        } => {
            if equipment.is_empty() {
                return Err(invalid(
                    "attack_equipment_empty",
                    format!("attack {:?} has no evidenced ranged equipment", attack.id),
                ));
            }
            for reference in equipment {
                validate_target_record_reference(reference, "WEAP", manifest)?;
            }
            if let Some(projectile) = projectile {
                validate_target_record_reference(projectile, "PROJ", manifest)?;
            }
            if let Some(ammunition) = ammunition {
                validate_target_record_reference(ammunition, "AMMO", manifest)?;
            }
            let references = equipment
                .iter()
                .chain(projectile.iter())
                .chain(ammunition.iter())
                .collect::<Vec<_>>();
            validate_distinct_attack_dependency_slice(attack, &references)?;
        }
        CreatureAttackRecordProjection::SpellAbility { spell } => {
            validate_target_record_reference(spell, "SPEL", manifest)?;
        }
        CreatureAttackRecordProjection::ContinuousRobot {
            attack_spell,
            equipment,
        } => {
            validate_target_record_reference(attack_spell, "SPEL", manifest)?;
            validate_target_record_reference(equipment, "WEAP", manifest)?;
            validate_distinct_attack_dependencies(attack, [attack_spell, equipment])?;
        }
    }
    Ok(())
}

fn validate_target_record_reference(
    reference: &CreatureTargetRecordReference,
    expected_signature: &'static str,
    manifest: &CreatureRecordManifest,
) -> Result<(), CreatureRecordError> {
    validate_target_record_reference_at(
        reference,
        expected_signature,
        manifest,
        "RACE",
        "ATKD.attack_spell",
    )
}

fn validate_npc_inventory_entry(
    entry: &CreatureNpcInventoryEntry,
    manifest: &CreatureRecordManifest,
) -> Result<(), CreatureRecordError> {
    let signature = entry.target_record.signature.as_bytes();
    if signature.len() != 4
        || !signature
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_')
    {
        return Err(CreatureRecordError::UnsupportedField {
            record: "NPC_",
            field: "CNTO.item",
            reason: format!(
                "inventory dependency has invalid target signature {:?}",
                entry.target_record.signature
            ),
        });
    }
    if entry.target_record.form_key.local == 0 || entry.target_record.form_key.local > 0x00ff_ffff {
        return Err(invalid(
            "npc_inventory_target_form_key",
            format!(
                "{} inventory dependency has invalid local id {:08X}",
                entry.target_record.signature, entry.target_record.form_key.local
            ),
        ));
    }
    if !entry
        .target_record
        .form_key
        .plugin
        .eq_ignore_ascii_case(&manifest.target_plugin)
        && !entry
            .target_record
            .form_key
            .plugin
            .eq_ignore_ascii_case(FALLOUT4_MASTER)
    {
        return Err(CreatureRecordError::UnsupportedField {
            record: "NPC_",
            field: "CNTO.item",
            reason: format!(
                "{} inventory dependency {:06X}@{} is neither the target plugin nor Fallout4.esm",
                entry.target_record.signature,
                entry.target_record.form_key.local,
                entry.target_record.form_key.plugin
            ),
        });
    }
    if let Some(ownership) = &entry.ownership {
        let condition = match ownership {
            CreatureNpcInventoryOwnership::FactionRank { condition, .. }
            | CreatureNpcInventoryOwnership::OwnerGlobal { condition, .. } => *condition,
        };
        if !condition.is_finite() || condition < 0.0 {
            return Err(invalid(
                "npc_inventory_condition",
                "NPC inventory COED item condition must be finite and non-negative",
            ));
        }
        match ownership {
            CreatureNpcInventoryOwnership::FactionRank { faction, .. } => {
                validate_target_record_reference_at(
                    faction,
                    "FACT",
                    manifest,
                    "NPC_",
                    "COED.owner",
                )?;
            }
            CreatureNpcInventoryOwnership::OwnerGlobal { owner, global, .. } => {
                if let Some(owner) = owner {
                    validate_target_record_reference_at(
                        owner,
                        "NPC_",
                        manifest,
                        "NPC_",
                        "COED.owner",
                    )?;
                }
                if let Some(global) = global {
                    validate_target_record_reference_at(
                        global,
                        "GLOB",
                        manifest,
                        "NPC_",
                        "COED.global_variable_required_rank",
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn validate_target_record_reference_at(
    reference: &CreatureTargetRecordReference,
    expected_signature: &'static str,
    manifest: &CreatureRecordManifest,
    record: &'static str,
    field: &'static str,
) -> Result<(), CreatureRecordError> {
    if reference.signature != expected_signature {
        return Err(CreatureRecordError::UnsupportedField {
            record,
            field,
            reason: format!(
                "expected explicit {expected_signature} evidence, got {:?}",
                reference.signature
            ),
        });
    }
    if reference.form_key.local == 0 || reference.form_key.local > 0x00ff_ffff {
        return Err(invalid(
            "attack_target_form_key",
            format!(
                "{} dependency has invalid local id {:08X}",
                reference.signature, reference.form_key.local
            ),
        ));
    }
    if !reference
        .form_key
        .plugin
        .eq_ignore_ascii_case(&manifest.target_plugin)
        && !reference
            .form_key
            .plugin
            .eq_ignore_ascii_case(FALLOUT4_MASTER)
    {
        return Err(CreatureRecordError::UnsupportedField {
            record,
            field,
            reason: format!(
                "{} dependency {:06X}@{} is neither the target plugin nor Fallout4.esm",
                reference.signature, reference.form_key.local, reference.form_key.plugin
            ),
        });
    }
    Ok(())
}

fn validate_distinct_attack_dependencies<'a, const N: usize>(
    attack: &CreatureAttackRecordVariant,
    references: [&'a CreatureTargetRecordReference; N],
) -> Result<(), CreatureRecordError> {
    validate_distinct_attack_dependency_slice(attack, &references)
}

fn validate_distinct_attack_dependency_slice(
    attack: &CreatureAttackRecordVariant,
    references: &[&CreatureTargetRecordReference],
) -> Result<(), CreatureRecordError> {
    let unique = references
        .iter()
        .map(|reference| {
            (
                reference.form_key.local,
                reference.form_key.plugin.to_ascii_lowercase(),
            )
        })
        .collect::<BTreeSet<_>>();
    if unique.len() != references.len() {
        return Err(invalid(
            "attack_target_collision",
            format!(
                "attack {:?} aliases records with different SPEL/PROJ/WEAP semantics",
                attack.id
            ),
        ));
    }
    Ok(())
}

fn validate_inputs(
    rig: &CreatureManifest,
    graph: &MvpGraphManifest,
    manifest: &CreatureRecordManifest,
    profile: &CreatureRecordProfile,
) -> Result<(), CreatureRecordError> {
    rig.validate_mvp(graph)
        .map_err(|errors| invalid("graph_contract", errors.to_string()))?;
    if !graph.melee_event.starts_with("melee") {
        return Err(invalid(
            "melee_event_name",
            "the attack event must start with lowercase `melee`",
        ));
    }
    validate_common_inputs(rig, manifest, profile, true)
}

fn validate_common_inputs(
    rig: &CreatureManifest,
    manifest: &CreatureRecordManifest,
    profile: &CreatureRecordProfile,
    include_unarmed_weapon: bool,
) -> Result<(), CreatureRecordError> {
    if profile.root_body_part.geometry_segment_index != ROOT_GEOMETRY_SEGMENT {
        return Err(CreatureRecordError::UnsupportedField {
            record: "BPTD",
            field: "BPND.geometry_segment_index",
            reason: format!(
                "the source-rig MVP supports only explicit root segment {ROOT_GEOMETRY_SEGMENT}, got {}",
                profile.root_body_part.geometry_segment_index
            ),
        });
    }
    if !rig
        .animation_skeleton
        .bones
        .iter()
        .any(|bone| bone.name.eq_ignore_ascii_case(&profile.root_body_part.node))
    {
        return Err(invalid(
            "body_part_node",
            format!(
                "BPTD node {:?} is not present in the source-owned rig",
                profile.root_body_part.node
            ),
        ));
    }
    if manifest.target_plugin.eq_ignore_ascii_case(FALLOUT4_MASTER)
        || !has_plugin_extension(&manifest.target_plugin)
    {
        return Err(invalid(
            "target_plugin",
            format!("invalid target plugin {:?}", manifest.target_plugin),
        ));
    }

    let mut form_keys = HashSet::new();
    for (signature, form_key) in manifest
        .form_keys
        .entries()
        .into_iter()
        .filter(|(signature, _)| include_unarmed_weapon || *signature != "WEAP")
    {
        if form_key.local == 0 || form_key.local > 0x00FF_FFFF {
            return Err(invalid(
                "target_form_key",
                format!("{signature} has invalid local id {:08X}", form_key.local),
            ));
        }
        if !form_key
            .plugin
            .eq_ignore_ascii_case(&manifest.target_plugin)
        {
            return Err(invalid(
                "target_form_key",
                format!(
                    "{signature} belongs to {:?}, expected {:?}",
                    form_key.plugin, manifest.target_plugin
                ),
            ));
        }
        if !form_keys.insert(form_key.local) {
            return Err(invalid(
                "duplicate_form_key",
                format!("duplicate local id {:06X}", form_key.local),
            ));
        }
    }

    let mut editor_ids = HashSet::new();
    for (signature, editor_id) in manifest
        .editor_ids
        .entries()
        .into_iter()
        .filter(|(signature, _)| include_unarmed_weapon || *signature != "WEAP")
    {
        if !editor_id.starts_with(&manifest.editor_id_prefix)
            || !editor_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(invalid(
                "editor_id",
                format!("{signature} has invalid custom EditorID {editor_id:?}"),
            ));
        }
        if !editor_ids.insert(editor_id.to_ascii_lowercase()) {
            return Err(invalid(
                "duplicate_editor_id",
                format!("duplicate EditorID {editor_id:?}"),
            ));
        }
    }

    let custom_root = format!("Actors\\{}\\", rig.creature_name);
    for (label, path, extension) in [
        ("RACE skeleton", rig.visual_skeleton_nif.as_str(), ".nif"),
        ("RACE project", rig.paths.project.as_str(), ".hkx"),
        ("ARMA body", manifest.body_nif.as_str(), ".nif"),
    ] {
        let lower = path.to_ascii_lowercase();
        if !starts_with_ascii_case(path, &custom_root)
            || !lower.ends_with(extension)
            || has_authoring_extension(&lower)
        {
            return Err(invalid(
                "custom_runtime_path",
                format!("{label} path {path:?} must be a runtime asset under {custom_root:?}"),
            ));
        }
    }

    for (field, value) in [
        ("attack_damage_multiplier", profile.attack_damage_multiplier),
        ("attack_chance", profile.attack_chance),
        ("attack_strike_angle", profile.attack_strike_angle),
        ("unarmed_reach", profile.unarmed_reach),
        ("unarmed_attack_seconds", profile.unarmed_attack_seconds),
        ("action_point_cost", profile.action_point_cost),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(invalid(
                "numeric_profile",
                format!("{field} must be finite and non-negative"),
            ));
        }
    }

    Ok(())
}

fn validate_target_form_key(
    signature: &str,
    form_key: &TargetFormKey,
    target_plugin: &str,
    seen: &mut HashSet<u32>,
) -> Result<(), CreatureRecordError> {
    if form_key.local == 0 || form_key.local > 0x00ff_ffff {
        return Err(invalid(
            "target_form_key",
            format!("{signature} has invalid local id {:08X}", form_key.local),
        ));
    }
    if !form_key.plugin.eq_ignore_ascii_case(target_plugin) {
        return Err(invalid(
            "target_form_key",
            format!(
                "{signature} belongs to {:?}, expected {target_plugin:?}",
                form_key.plugin
            ),
        ));
    }
    if !seen.insert(form_key.local) {
        return Err(invalid(
            "duplicate_form_key",
            format!("duplicate local id {:06X}", form_key.local),
        ));
    }
    Ok(())
}

fn validate_editor_id(
    signature: &str,
    editor_id: &str,
    prefix: &str,
    seen: &mut HashSet<String>,
) -> Result<(), CreatureRecordError> {
    if !editor_id.starts_with(prefix)
        || !editor_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(invalid(
            "editor_id",
            format!("{signature} has invalid custom EditorID {editor_id:?}"),
        ));
    }
    if !seen.insert(editor_id.to_ascii_lowercase()) {
        return Err(invalid(
            "duplicate_editor_id",
            format!("duplicate EditorID {editor_id:?}"),
        ));
    }
    Ok(())
}

fn validate_closure(
    closure: &CreatureRecordClosure,
    rig: &CreatureManifest,
    graph: &MvpGraphManifest,
    manifest: &CreatureRecordManifest,
    profile: &CreatureRecordProfile,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    validate_record_set(closure, 6, schema, interner, &[])?;

    assert_form_link(
        closure.record("RACE"),
        "WNAM",
        &manifest.form_keys.skin,
        interner,
    )?;
    assert_form_link(
        closure.record("RACE"),
        "GNAM",
        &manifest.form_keys.body_part_data,
        interner,
    )?;
    assert_form_link(
        closure.record("RACE"),
        "UNWP",
        &manifest.form_keys.unarmed_weapon,
        interner,
    )?;
    assert_string_value(
        closure.record("RACE"),
        "ANAM",
        &rig.visual_skeleton_nif,
        interner,
    )?;
    assert_string_value(closure.record("RACE"), "MODL", &rig.paths.project, interner)?;
    validate_race_behavior_subgraph(closure.record("RACE"), rig, interner)?;
    assert_string_value(closure.record("RACE"), "ATKE", &graph.melee_event, interner)?;
    assert_form_link(
        closure.record("ARMO"),
        "RNAM",
        &manifest.form_keys.race,
        interner,
    )?;
    assert_form_link(
        closure.record("ARMO"),
        "MODL",
        &manifest.form_keys.armor_addon,
        interner,
    )?;
    assert_form_link(
        closure.record("ARMA"),
        "RNAM",
        &manifest.form_keys.race,
        interner,
    )?;
    assert_string_value(closure.record("ARMA"), "MOD2", &manifest.body_nif, interner)?;
    assert_form_link(
        closure.record("NPC_"),
        "RNAM",
        &manifest.form_keys.race,
        interner,
    )?;
    assert_form_link(
        closure.record("NPC_"),
        "WNAM",
        &manifest.form_keys.skin,
        interner,
    )?;
    assert_form_link(
        closure.record("NPC_"),
        "ATKR",
        &manifest.form_keys.race,
        interner,
    )?;

    validate_root_body_part_array(closure, &profile.root_body_part, interner)?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_projection_closure(
    closure: &CreatureRecordClosure,
    rig: &CreatureManifest,
    _graph: &ProjectionGraphContract,
    projection: &CreatureRecordProjectionManifest,
    primary_variant: &CreatureNpcRecordVariant,
    primary_attack: Option<&CreatureAttackRecordVariant>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    let body_nif_parts = projection.effective_body_nif_parts();
    let melee_count = projection
        .attacks
        .iter()
        .filter(|attack| {
            matches!(
                attack.projection,
                CreatureAttackRecordProjection::MeleeUnarmed { .. }
            )
        })
        .count();
    validate_record_set(
        closure,
        3 + projection.variants.len() + body_nif_parts.len() + melee_count,
        schema,
        interner,
        &projection_target_dependencies(projection),
    )?;
    assert_form_link(
        closure.record("RACE"),
        "WNAM",
        &projection.base.form_keys.skin,
        interner,
    )?;
    assert_form_link(
        closure.record("RACE"),
        "GNAM",
        &projection.base.form_keys.body_part_data,
        interner,
    )?;
    match first_melee_attack(&projection.attacks) {
        Some(CreatureAttackRecordProjection::MeleeUnarmed {
            weapon_form_key, ..
        }) => assert_form_link(closure.record("RACE"), "UNWP", weapon_form_key, interner)?,
        Some(_) => unreachable!("first_melee_attack returns only melee projections"),
        None => {
            if closure
                .record("RACE")
                .is_some_and(|record| field(record, "UNWP").is_some())
            {
                return Err(invalid(
                    "attack_weapon_closure",
                    "a non-melee projection must not emit RACE.UNWP",
                ));
            }
        }
    }
    assert_string_value(
        closure.record("RACE"),
        "ANAM",
        &rig.visual_skeleton_nif,
        interner,
    )?;
    assert_string_value(closure.record("RACE"), "MODL", &rig.paths.project, interner)?;
    validate_race_behavior_subgraph(closure.record("RACE"), rig, interner)?;
    if field(
        closure
            .record("RACE")
            .ok_or_else(|| invalid("record_link", "missing RACE"))?,
        "DATA",
    )
    .is_none()
    {
        return Err(invalid("race_data", "projected RACE is missing DATA"));
    }

    let expected_events = projection
        .attacks
        .iter()
        .map(|attack| attack.event.as_str())
        .collect::<BTreeSet<_>>();
    let actual_events = closure
        .record("RACE")
        .into_iter()
        .flat_map(|record| record.fields.iter())
        .filter(|entry| entry.sig.as_str() == "ATKE")
        .filter_map(|entry| match entry.value {
            FieldValue::String(value) => interner.resolve(value),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if actual_events != expected_events
        || primary_attack.is_some_and(|attack| !actual_events.contains(attack.event.as_str()))
    {
        return Err(invalid(
            "attack_event_closure",
            "RACE ATKE rows do not close over the projected attack events",
        ));
    }

    for variant in &projection.variants {
        let record = record_by_target(closure, "NPC_", &variant.form_key, interner);
        assert_form_link(record, "RNAM", &projection.base.form_keys.race, interner)?;
        assert_form_link(record, "WNAM", &projection.base.form_keys.skin, interner)?;
        assert_form_link(record, "ATKR", &projection.base.form_keys.race, interner)?;
        let mut attacks = projection.attacks.iter().collect::<Vec<_>>();
        attacks.sort_by_key(|attack| {
            (
                !attack.primary,
                attack.id.to_ascii_lowercase(),
                attack.projection.kind_name(),
            )
        });
        let expected_inventory = variant_inventory(projection, variant, &attacks)
            .into_iter()
            .map(|entry| {
                (
                    entry.target_record.form_key.intern(interner),
                    i64::from(entry.count),
                    entry.ownership.map(|ownership| match ownership {
                        CreatureNpcInventoryOwnership::FactionRank {
                            faction,
                            required_rank,
                            condition,
                        } => ProjectedNpcInventoryOwnership::FactionRank {
                            faction: faction.form_key.intern(interner),
                            required_rank,
                            condition_bits: condition.to_bits(),
                        },
                        CreatureNpcInventoryOwnership::OwnerGlobal {
                            owner,
                            global,
                            condition,
                        } => ProjectedNpcInventoryOwnership::OwnerGlobal {
                            owner: owner.map(|owner| owner.form_key.intern(interner)),
                            global: global.map(|global| global.form_key.intern(interner)),
                            condition_bits: condition.to_bits(),
                        },
                    }),
                )
            })
            .collect::<Vec<_>>();
        let actual_inventory = record
            .map(|record| projected_npc_inventory_rows(record, interner))
            .transpose()?
            .unwrap_or_default();
        if actual_inventory != expected_inventory {
            return Err(invalid(
                "npc_inventory_closure",
                format!(
                    "projected NPC inventory differs for {}: expected {expected_inventory:?}, got {actual_inventory:?}",
                    variant.source_identity.stable_key(),
                ),
            ));
        }
        let expected_spells = variant_npc_spells(projection, variant)
            .iter()
            .map(|spell| spell.form_key.intern(interner))
            .collect::<Vec<_>>();
        let actual_spells = record
            .into_iter()
            .flat_map(|record| record.fields.iter())
            .filter(|field| field.sig.as_str() == "SPLO")
            .filter_map(|field| match &field.value {
                FieldValue::FormKey(form_key) => Some(*form_key),
                _ => None,
            })
            .collect::<Vec<_>>();
        if actual_spells != expected_spells {
            return Err(invalid(
                "npc_spell_closure",
                format!(
                    "projected NPC spell list differs for {}",
                    variant.source_identity.stable_key()
                ),
            ));
        }
        let expected_death_item = variant_npc_death_item(projection, variant)
            .map(|death_item| death_item.form_key.intern(interner));
        let actual_death_items = record
            .into_iter()
            .flat_map(|record| record.fields.iter())
            .filter(|field| field.sig.as_str() == "INAM")
            .map(|field| match &field.value {
                FieldValue::FormKey(form_key) => Ok(*form_key),
                _ => Err(invalid(
                    "npc_death_item_closure",
                    "projected NPC_.INAM is not a FormKey",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if actual_death_items.as_slice() != expected_death_item.as_slice() {
            return Err(invalid(
                "npc_death_item_closure",
                format!(
                    "projected NPC death item differs for {}",
                    variant.source_identity.stable_key()
                ),
            ));
        }
    }
    for attack in &projection.attacks {
        if let CreatureAttackRecordProjection::MeleeUnarmed {
            weapon_form_key, ..
        } = &attack.projection
            && record_by_target(closure, "WEAP", weapon_form_key, interner).is_none()
        {
            return Err(invalid(
                "attack_weapon_closure",
                format!("missing unarmed WEAP for melee attack {:?}", attack.id),
            ));
        }
    }
    if record_by_target(closure, "NPC_", &primary_variant.form_key, interner).is_none() {
        return Err(invalid(
            "source_primary_identity",
            "primary source NPC mapping was lost",
        ));
    }

    assert_form_link(
        closure.record("ARMO"),
        "RNAM",
        &projection.base.form_keys.race,
        interner,
    )?;
    let expected_armatures = body_nif_parts
        .iter()
        .map(|part| part.armor_addon_form_key.intern(interner))
        .collect::<Vec<_>>();
    let actual_armatures = closure
        .record("ARMO")
        .into_iter()
        .flat_map(|record| record.fields.iter())
        .filter(|field| field.sig.as_str() == "MODL")
        .filter_map(|field| match &field.value {
            FieldValue::FormKey(form_key) => Some(*form_key),
            _ => None,
        })
        .collect::<Vec<_>>();
    if actual_armatures != expected_armatures {
        return Err(invalid(
            "armo_armature_closure",
            "ARMO armature array does not preserve the ordered body NIF parts",
        ));
    }
    for part in &body_nif_parts {
        let armor_addon = record_by_target(closure, "ARMA", &part.armor_addon_form_key, interner);
        assert_form_link(
            armor_addon,
            "RNAM",
            &projection.base.form_keys.race,
            interner,
        )?;
        assert_string_value(armor_addon, "MOD2", &part.body_nif, interner)?;
        assert_string_value(armor_addon, "MOD3", &part.body_nif, interner)?;
    }
    validate_root_body_part_array(closure, &projection.body_parts.profile(), interner)?;
    Ok(())
}

fn validate_race_behavior_subgraph(
    race: Option<&Record>,
    rig: &CreatureManifest,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    let race = race.ok_or_else(|| invalid("record_link", "missing RACE"))?;
    assert_string_value(Some(race), "SGNM", &rig.paths.core_behavior, interner)?;
    assert_string_value(
        Some(race),
        "SAPT",
        &runtime_animation_directory(&rig.paths.project),
        interner,
    )?;
    let sraf = field(race, "SRAF")
        .ok_or_else(|| invalid("race_subgraph", "generated RACE is missing SRAF"))?;
    if !matches!(sraf, FieldValue::Bytes(value) if value.as_slice() == [1, 0, 0, 0]) {
        return Err(invalid(
            "race_subgraph",
            "generated RACE SRAF must declare the third-person behavior subgraph",
        ));
    }
    Ok(())
}

fn validate_record_set(
    closure: &CreatureRecordClosure,
    expected_count: usize,
    schema: &AuthoringSchema,
    interner: &StringInterner,
    allowed_dependencies: &[CreatureTargetRecordReference],
) -> Result<(), CreatureRecordError> {
    if closure.records.len() != expected_count {
        return Err(invalid(
            "record_count",
            format!(
                "expected {expected_count} records, got {}",
                closure.records.len()
            ),
        ));
    }
    let mut record_keys = HashSet::new();
    for record in &closure.records {
        let plugin = interner
            .resolve(record.form_key.plugin)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !record_keys.insert((record.form_key.local, plugin)) {
            return Err(invalid(
                "duplicate_record_key",
                format!("duplicate emitted FormKey {:06X}", record.form_key.local),
            ));
        }
        let Some(record_def) = schema.record_def(record.sig.as_str()) else {
            return Err(CreatureRecordError::UnsupportedRecord(
                record.sig.as_str().to_string(),
            ));
        };
        for field in &record.fields {
            let Some(subrecord_def) = record_def.subrecord_def(field.sig.as_str()) else {
                return Err(invalid(
                    "schema_field",
                    format!(
                        "{}.{} is absent from the FO4 schema",
                        record.sig.as_str(),
                        field.sig.as_str()
                    ),
                ));
            };
            if let (FieldValue::Struct(fields), Some(expected)) = (
                &field.value,
                subrecord_def
                    .codec
                    .as_deref()
                    .and_then(fixed_struct_codec_size),
            ) {
                let actual = fields
                    .iter()
                    .map(|(_, value)| normalized_field_width(value))
                    .sum::<usize>();
                if actual != expected {
                    return Err(invalid(
                        "struct_size",
                        format!(
                            "{}.{} normalizes to {actual} bytes, expected {expected}",
                            record.sig.as_str(),
                            field.sig.as_str()
                        ),
                    ));
                }
            }
        }
    }

    let emitted = closure
        .records
        .iter()
        .map(|record| {
            (
                record.form_key.local,
                interner
                    .resolve(record.form_key.plugin)
                    .unwrap_or_default()
                    .to_ascii_lowercase(),
            )
        })
        .collect::<HashSet<_>>();
    let allowed_dependencies = allowed_dependencies
        .iter()
        .map(|reference| {
            (
                reference.form_key.local,
                reference.form_key.plugin.to_ascii_lowercase(),
            )
        })
        .collect::<HashSet<_>>();
    for record in &closure.records {
        for field in &record.fields {
            validate_form_keys(&field.value, &emitted, &allowed_dependencies, interner)?;
        }
    }
    Ok(())
}

fn validate_root_body_part_array(
    closure: &CreatureRecordClosure,
    root: &RootBodyPartProfile,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    let body_part = closure
        .record("BPTD")
        .ok_or_else(|| invalid("bptd_contract", "closure is missing BPTD"))?;
    for signature in ["BPTN", "BPNN", "BPNT", "BPND"] {
        let count = body_part
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == signature)
            .count();
        if count != 1 {
            return Err(invalid(
                "bptd_array_index_closure",
                format!("root-only BPTD requires one {signature} row, got {count}"),
            ));
        }
    }
    let data =
        field(body_part, "BPND").ok_or_else(|| invalid("bptd_contract", "BPTD is missing BPND"))?;
    let segment = struct_member(data, "geometry_segment_index", interner)
        .and_then(uint_value)
        .ok_or_else(|| invalid("bptd_contract", "BPND has no geometry segment"))?;
    if root.geometry_segment_index != ROOT_GEOMETRY_SEGMENT
        || segment != u64::from(ROOT_GEOMETRY_SEGMENT)
    {
        return Err(invalid(
            "bptd_contract",
            format!(
                "root-only BPTD row 0 must close over geometry segment {ROOT_GEOMETRY_SEGMENT}, got {segment}"
            ),
        ));
    }
    assert_string_value(Some(body_part), "BPNN", &root.node, interner)
}

fn record_by_target<'a>(
    closure: &'a CreatureRecordClosure,
    signature: &str,
    target: &TargetFormKey,
    interner: &StringInterner,
) -> Option<&'a Record> {
    closure.records.iter().find(|record| {
        record.sig.as_str() == signature
            && record.form_key.local == target.local
            && interner
                .resolve(record.form_key.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case(&target.plugin))
    })
}

fn emit_race(
    rig: &CreatureManifest,
    attack_event: &str,
    manifest: &CreatureRecordManifest,
    profile: &CreatureRecordProfile,
    keys: &InternedRecordKeys,
    interner: &StringInterner,
) -> Record {
    make_record(
        "RACE",
        keys.race,
        &manifest.editor_ids.race,
        vec![
            string_field("EDID", &manifest.editor_ids.race, interner),
            string_field("FULL", &manifest.display_name, interner),
            form_field("WNAM", keys.skin),
            uint_field("KSIZ", 0),
            list_field("KWDA", Vec::new()),
            empty_field("MNAM"),
            string_field("ANAM", &rig.visual_skeleton_nif, interner),
            empty_field("FNAM"),
            string_field("ANAM", &rig.visual_skeleton_nif, interner),
            struct_field(
                "ATKD",
                vec![
                    (
                        "damage_mult",
                        FieldValue::Float(profile.attack_damage_multiplier),
                    ),
                    ("attack_chance", FieldValue::Float(profile.attack_chance)),
                    ("attack_spell", FieldValue::Uint(0)),
                    ("attack_flags", FieldValue::Uint(0)),
                    ("attack_angle", FieldValue::Float(0.0)),
                    (
                        "strike_angle",
                        FieldValue::Float(profile.attack_strike_angle),
                    ),
                    ("stagger", FieldValue::Float(0.0)),
                    ("knockdown", FieldValue::Float(0.0)),
                    ("recovery_time", FieldValue::Float(0.0)),
                    ("action_points_mult", FieldValue::Float(1.0)),
                    ("stagger_offset", FieldValue::Int(0)),
                ],
                interner,
            ),
            string_field("ATKE", attack_event, interner),
            form_field("GNAM", keys.body_part_data),
            empty_field("NAM3"),
            empty_field("MNAM"),
            string_field("MODL", &rig.paths.project, interner),
            empty_field("FNAM"),
            string_field("MODL", &rig.paths.project, interner),
            string_field("SGNM", &rig.paths.core_behavior, interner),
            string_field(
                "SAPT",
                &runtime_animation_directory(&rig.paths.project),
                interner,
            ),
            bytes_field("SRAF", vec![1, 0, 0, 0]),
            form_field("UNWP", keys.unarmed_weapon),
        ],
        interner,
    )
}

fn runtime_animation_directory(project_path: &str) -> String {
    project_path
        .rsplit_once('\\')
        .map(|(project_root, _)| format!("{project_root}\\Animations"))
        .unwrap_or_else(|| "Animations".to_string())
}

fn first_melee_attack(
    attacks: &[CreatureAttackRecordVariant],
) -> Option<&CreatureAttackRecordProjection> {
    attacks
        .iter()
        .filter(|attack| {
            matches!(
                attack.projection,
                CreatureAttackRecordProjection::MeleeUnarmed { .. }
            )
        })
        .min_by_key(|attack| (!attack.primary, attack.id.to_ascii_lowercase()))
        .map(|attack| &attack.projection)
}

fn variant_inventory(
    projection: &CreatureRecordProjectionManifest,
    variant: &CreatureNpcRecordVariant,
    attacks: &[&CreatureAttackRecordVariant],
) -> Vec<CreatureNpcInventoryEntry> {
    let mut inventory = variant_npc_inventory(projection, variant).to_vec();
    if inventory.is_empty() {
        inventory.extend(
            variant_npc_equipment(projection, variant)
                .iter()
                .cloned()
                .map(|target_record| CreatureNpcInventoryEntry {
                    target_record,
                    count: 1,
                    ownership: None,
                }),
        );
    }
    let mut seen = inventory
        .iter()
        .map(|entry| {
            (
                entry.target_record.form_key.plugin.to_ascii_lowercase(),
                entry.target_record.form_key.local,
            )
        })
        .collect::<HashSet<_>>();
    for reference in attacks
        .iter()
        .flat_map(|attack| attack.projection.equipment())
    {
        if seen.insert((
            reference.form_key.plugin.to_ascii_lowercase(),
            reference.form_key.local,
        )) {
            inventory.push(CreatureNpcInventoryEntry {
                target_record: reference.clone(),
                count: 1,
                ownership: None,
            });
        }
    }
    inventory
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ProjectedNpcInventoryOwnership {
    FactionRank {
        faction: FormKey,
        required_rank: i32,
        condition_bits: u32,
    },
    OwnerGlobal {
        owner: Option<FormKey>,
        global: Option<FormKey>,
        condition_bits: u32,
    },
}

type ProjectedNpcInventoryRow = (FormKey, i64, Option<ProjectedNpcInventoryOwnership>);

fn projected_npc_inventory_rows(
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<ProjectedNpcInventoryRow>, CreatureRecordError> {
    for (index, field) in record.fields.iter().enumerate() {
        if field.sig.as_str() == "COED"
            && (index == 0 || record.fields[index - 1].sig.as_str() != "CNTO")
        {
            return Err(invalid(
                "npc_inventory_coed_association",
                "NPC_ COED must immediately follow its owning CNTO row",
            ));
        }
    }

    let mut rows = Vec::new();
    for (index, field) in record.fields.iter().enumerate() {
        if field.sig.as_str() != "CNTO" {
            continue;
        }
        let item = match struct_member(&field.value, "item", interner) {
            Some(FieldValue::FormKey(form_key)) => *form_key,
            _ => {
                return Err(invalid(
                    "npc_inventory_closure",
                    "projected NPC_ CNTO item is not a FormKey",
                ));
            }
        };
        let count = match struct_member(&field.value, "count", interner) {
            Some(FieldValue::Int(count)) => *count,
            Some(FieldValue::Uint(count)) => i64::try_from(*count).map_err(|_| {
                invalid(
                    "npc_inventory_closure",
                    "projected NPC_ CNTO count exceeds signed range",
                )
            })?,
            Some(FieldValue::Bytes(count)) if count.len() == 4 => {
                i64::from(i32::from_le_bytes([count[0], count[1], count[2], count[3]]))
            }
            _ => {
                return Err(invalid(
                    "npc_inventory_closure",
                    "projected NPC_ CNTO count is not an integer",
                ));
            }
        };
        let ownership = record
            .fields
            .get(index + 1)
            .filter(|field| field.sig.as_str() == "COED")
            .map(|field| projected_npc_inventory_ownership(&field.value, interner))
            .transpose()?;
        rows.push((item, count, ownership));
    }
    Ok(rows)
}

fn projected_npc_inventory_ownership(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<ProjectedNpcInventoryOwnership, CreatureRecordError> {
    let owner = optional_inventory_form_key(struct_member(value, "owner", interner), "COED.owner")?;
    let condition_bits = match struct_member(value, "item_condition", interner) {
        Some(FieldValue::Float(condition)) => condition.to_bits(),
        Some(FieldValue::Bytes(condition)) if condition.len() == 4 => {
            u32::from_le_bytes([condition[0], condition[1], condition[2], condition[3]])
        }
        _ => {
            return Err(invalid(
                "npc_inventory_closure",
                "projected NPC_ COED item condition is not a float",
            ));
        }
    };
    let second = struct_member(value, "global_variable_required_rank", interner);
    let is_faction_rank = matches!(second, Some(FieldValue::Int(_) | FieldValue::Uint(1..)))
        || matches!(second, Some(FieldValue::Bytes(value)) if value.len() == 4 && value.as_slice() != [0, 0, 0, 0]);
    if is_faction_rank {
        let faction = owner.ok_or_else(|| {
            invalid(
                "npc_inventory_closure",
                "projected faction-rank COED has no faction owner",
            )
        })?;
        let required_rank = inventory_required_rank(second)?;
        Ok(ProjectedNpcInventoryOwnership::FactionRank {
            faction,
            required_rank,
            condition_bits,
        })
    } else {
        Ok(ProjectedNpcInventoryOwnership::OwnerGlobal {
            owner,
            global: optional_inventory_form_key(second, "COED.global_variable_required_rank")?,
            condition_bits,
        })
    }
}

fn inventory_required_rank(value: Option<&FieldValue>) -> Result<i32, CreatureRecordError> {
    match value {
        Some(FieldValue::Int(value)) => i32::try_from(*value).map_err(|_| {
            invalid(
                "npc_inventory_closure",
                "projected COED required rank exceeds i32 range",
            )
        }),
        Some(FieldValue::Uint(value)) => {
            u32::try_from(*value)
                .map(|value| value as i32)
                .map_err(|_| {
                    invalid(
                        "npc_inventory_closure",
                        "projected COED required rank exceeds 32-bit range",
                    )
                })
        }
        Some(FieldValue::Bytes(value)) if value.len() == 4 => {
            Ok(i32::from_le_bytes([value[0], value[1], value[2], value[3]]))
        }
        _ => Err(invalid(
            "npc_inventory_closure",
            "projected faction-rank COED required rank is not a 32-bit integer",
        )),
    }
}

fn optional_inventory_form_key(
    value: Option<&FieldValue>,
    field: &'static str,
) -> Result<Option<FormKey>, CreatureRecordError> {
    match value {
        Some(FieldValue::FormKey(form_key)) if form_key.local != 0 => Ok(Some(*form_key)),
        Some(FieldValue::FormKey(_)) | Some(FieldValue::Uint(0)) => Ok(None),
        Some(FieldValue::Bytes(value)) if value.as_slice() == [0, 0, 0, 0] => Ok(None),
        _ => Err(invalid(
            "npc_inventory_closure",
            format!("projected NPC_ {field} is neither a FormKey nor null"),
        )),
    }
}

fn variant_npc_inventory<'a>(
    projection: &'a CreatureRecordProjectionManifest,
    variant: &'a CreatureNpcRecordVariant,
) -> &'a [CreatureNpcInventoryEntry] {
    variant
        .npc_inventory
        .as_deref()
        .unwrap_or(&projection.npc_inventory)
}

fn variant_npc_equipment<'a>(
    projection: &'a CreatureRecordProjectionManifest,
    variant: &'a CreatureNpcRecordVariant,
) -> &'a [CreatureTargetRecordReference] {
    variant
        .npc_equipment
        .as_deref()
        .unwrap_or(&projection.npc_equipment)
}

fn variant_npc_spells<'a>(
    projection: &'a CreatureRecordProjectionManifest,
    variant: &'a CreatureNpcRecordVariant,
) -> &'a [CreatureTargetRecordReference] {
    variant
        .npc_spells
        .as_deref()
        .unwrap_or(&projection.npc_spells)
}

fn variant_npc_death_item<'a>(
    projection: &'a CreatureRecordProjectionManifest,
    variant: &'a CreatureNpcRecordVariant,
) -> Option<&'a CreatureTargetRecordReference> {
    variant
        .npc_death_item
        .as_ref()
        .map(|death_item| death_item.as_ref())
        .unwrap_or(projection.npc_death_item.as_ref())
}

fn attack_target_dependencies_from_projection(
    projection: &CreatureRecordProjectionManifest,
) -> Vec<CreatureTargetRecordReference> {
    collect_attack_target_dependencies(projection.attacks.iter())
}

fn projection_target_dependencies(
    projection: &CreatureRecordProjectionManifest,
) -> Vec<CreatureTargetRecordReference> {
    let mut dependencies = attack_target_dependencies_from_projection(projection);
    for entry in projection.npc_inventory.iter().chain(
        projection
            .variants
            .iter()
            .flat_map(|variant| variant.npc_inventory.iter().flatten()),
    ) {
        dependencies.push(entry.target_record.clone());
        if let Some(ownership) = &entry.ownership {
            match ownership {
                CreatureNpcInventoryOwnership::FactionRank { faction, .. } => {
                    dependencies.push(faction.clone());
                }
                CreatureNpcInventoryOwnership::OwnerGlobal { owner, global, .. } => {
                    dependencies.extend(owner.iter().cloned());
                    dependencies.extend(global.iter().cloned());
                }
            }
        }
    }
    dependencies.extend(projection.npc_equipment.iter().cloned());
    dependencies.extend(projection.npc_spells.iter().cloned());
    dependencies.extend(projection.npc_death_item.iter().cloned());
    dependencies.extend(
        projection
            .variants
            .iter()
            .flat_map(|variant| variant.npc_equipment.iter().flatten().cloned()),
    );
    dependencies.extend(
        projection
            .variants
            .iter()
            .flat_map(|variant| variant.npc_spells.iter().flatten().cloned()),
    );
    dependencies.extend(
        projection
            .variants
            .iter()
            .flat_map(|variant| variant.npc_death_item.iter().flatten().cloned()),
    );
    dependencies.sort_by_key(|reference| {
        (
            reference.form_key.plugin.to_ascii_lowercase(),
            reference.form_key.local,
            reference.signature.clone(),
        )
    });
    dependencies.dedup_by(|left, right| left == right);
    dependencies
}

fn collect_attack_target_dependencies<'a>(
    attacks: impl IntoIterator<Item = &'a CreatureAttackRecordVariant>,
) -> Vec<CreatureTargetRecordReference> {
    let mut dependencies = attacks
        .into_iter()
        .flat_map(|attack| attack.projection.target_dependencies())
        .cloned()
        .collect::<Vec<_>>();
    dependencies.sort_by_key(|reference| {
        (
            reference.form_key.plugin.to_ascii_lowercase(),
            reference.form_key.local,
            reference.signature.clone(),
        )
    });
    dependencies.dedup_by(|left, right| left == right);
    dependencies
}

fn apply_race_projection(
    race: &mut Record,
    projection: &CreatureRecordProjectionManifest,
    interner: &StringInterner,
) {
    let unarmed_weapon =
        first_melee_attack(&projection.attacks).and_then(|projection| match projection {
            CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key, ..
            } => Some(weapon_form_key.intern(interner)),
            _ => None,
        });
    let mut fields = SmallVec::new();
    let mut data_inserted = false;
    let mut attacks_inserted = false;
    for mut entry in std::mem::take(&mut race.fields) {
        match entry.sig.as_str() {
            "DATA" | "ATKD" | "ATKE" => continue,
            "MNAM" if !data_inserted => {
                fields.push(race_data_field(&projection.race_data, interner));
                data_inserted = true;
            }
            "GNAM" if !attacks_inserted => {
                let mut attacks = projection.attacks.iter().collect::<Vec<_>>();
                attacks.sort_by_key(|attack| {
                    (
                        !attack.primary,
                        attack.id.to_ascii_lowercase(),
                        attack.projection.kind_name(),
                    )
                });
                for attack in attacks {
                    fields.extend(race_attack_fields(attack, interner));
                }
                attacks_inserted = true;
            }
            "UNWP" => match unarmed_weapon {
                Some(weapon) => entry.value = FieldValue::FormKey(weapon),
                None => continue,
            },
            _ => {}
        }
        fields.push(entry);
    }
    race.fields = fields;
}

fn race_attack_fields(
    attack: &CreatureAttackRecordVariant,
    interner: &StringInterner,
) -> [FieldEntry; 2] {
    let attack_spell = attack
        .projection
        .attack_spell()
        .map(|reference| FieldValue::FormKey(reference.form_key.intern(interner)))
        .unwrap_or(FieldValue::Uint(0));
    [
        struct_field(
            "ATKD",
            vec![
                ("damage_mult", FieldValue::Float(attack.damage_multiplier)),
                ("attack_chance", FieldValue::Float(attack.chance)),
                ("attack_spell", attack_spell),
                (
                    "attack_flags",
                    FieldValue::Uint(u64::from(attack.target_data.attack_flags)),
                ),
                (
                    "attack_angle",
                    FieldValue::Float(attack.target_data.attack_angle),
                ),
                ("strike_angle", FieldValue::Float(attack.strike_angle)),
                ("stagger", FieldValue::Float(attack.target_data.stagger)),
                ("knockdown", FieldValue::Float(attack.target_data.knockdown)),
                (
                    "recovery_time",
                    FieldValue::Float(attack.target_data.recovery_time),
                ),
                (
                    "action_points_mult",
                    FieldValue::Float(attack.target_data.action_points_multiplier),
                ),
                (
                    "stagger_offset",
                    FieldValue::Int(i64::from(attack.target_data.stagger_offset)),
                ),
            ],
            interner,
        ),
        string_field("ATKE", &attack.event, interner),
    ]
}

fn race_data_field(target: &Fo4RaceDataTarget, interner: &StringInterner) -> FieldEntry {
    let zero_bytes = |prefix: &str, count: usize| {
        (1..=count)
            .map(|index| (format!("{prefix}_byte_{index}"), FieldValue::Uint(0)))
            .collect::<Vec<_>>()
    };
    let mut fields = vec![
        (
            "male_height".to_string(),
            FieldValue::Float(target.male_height),
        ),
        (
            "female_height".to_string(),
            FieldValue::Float(target.female_height),
        ),
        (
            "male_default_weight_thin".to_string(),
            FieldValue::Float(target.male_default_weight[0]),
        ),
        (
            "male_default_weight_muscular".to_string(),
            FieldValue::Float(target.male_default_weight[1]),
        ),
        (
            "male_default_weight_fat".to_string(),
            FieldValue::Float(target.male_default_weight[2]),
        ),
        (
            "female_default_weight_thin".to_string(),
            FieldValue::Float(target.female_default_weight[0]),
        ),
        (
            "female_default_weight_muscular".to_string(),
            FieldValue::Float(target.female_default_weight[1]),
        ),
        (
            "female_default_weight_fat".to_string(),
            FieldValue::Float(target.female_default_weight[2]),
        ),
        (
            "flags".to_string(),
            FieldValue::Uint(u64::from(target.flags_bits())),
        ),
        (
            "acceleration_rate".to_string(),
            FieldValue::Float(target.acceleration_rate),
        ),
        (
            "deceleration_rate".to_string(),
            FieldValue::Float(target.deceleration_rate),
        ),
        ("size".to_string(), FieldValue::Uint(target.size.value())),
    ];
    for prefix in ["unknown_bytes1", "unknown_bytes2"] {
        for index in 1..=4 {
            fields.push((format!("{prefix}_byte_{index}"), FieldValue::Uint(255)));
        }
    }
    fields.extend([
        (
            "injured_health_pct".to_string(),
            FieldValue::Float(target.injured_health_percent),
        ),
        ("shield_biped_object".to_string(), FieldValue::Int(-1)),
        ("beard_biped_object".to_string(), FieldValue::Int(-1)),
        (
            "body_biped_object".to_string(),
            FieldValue::Int(i64::from(target.body_biped_object)),
        ),
        (
            "aim_angle_tolerance".to_string(),
            FieldValue::Float(target.aim_angle_tolerance),
        ),
        (
            "flight_radius".to_string(),
            FieldValue::Float(target.flight_radius),
        ),
        (
            "angular_acceleration_rate".to_string(),
            FieldValue::Float(target.angular_acceleration_rate),
        ),
        (
            "angular_tolerance".to_string(),
            FieldValue::Float(target.angular_tolerance),
        ),
        (
            "flags_2".to_string(),
            FieldValue::Uint(u64::from(target.flags_2_bits())),
        ),
        ("unknown_float1".to_string(), FieldValue::Float(0.0)),
        ("unknown_float2".to_string(), FieldValue::Float(0.0)),
    ]);
    for prefix in [
        "unknown_bytes3",
        "unknown_bytes4",
        "unknown_bytes5",
        "unknown_bytes6",
        "unknown_bytes7",
    ] {
        fields.extend(zero_bytes(prefix, 4));
    }
    fields.push(("unknown_float3".to_string(), FieldValue::Float(0.0)));
    fields.extend(zero_bytes("unknown_bytes8", 4));
    fields.extend([
        ("pipboy_biped_object".to_string(), FieldValue::Int(-1)),
        (
            "xp_value".to_string(),
            FieldValue::Int(i64::from(target.xp_value)),
        ),
        ("severable_debris_scale".to_string(), FieldValue::Float(1.0)),
        ("severable_debris_count".to_string(), FieldValue::Uint(0)),
        ("severable_decal_count".to_string(), FieldValue::Uint(0)),
        (
            "explodable_debris_scale".to_string(),
            FieldValue::Float(1.0),
        ),
        ("explodable_debris_count".to_string(), FieldValue::Uint(0)),
        ("explodable_decal_count".to_string(), FieldValue::Uint(0)),
        ("severable_explosion".to_string(), FieldValue::Uint(0)),
        ("severable_debris".to_string(), FieldValue::Uint(0)),
        ("severable_impact_dataset".to_string(), FieldValue::Uint(0)),
        ("explodable_explosion".to_string(), FieldValue::Uint(0)),
        ("explodable_debris".to_string(), FieldValue::Uint(0)),
        ("explodable_impact_dataset".to_string(), FieldValue::Uint(0)),
        ("oncripple_debris_scale".to_string(), FieldValue::Float(1.0)),
        ("oncripple_debris_count".to_string(), FieldValue::Uint(0)),
        ("oncripple_decal_count".to_string(), FieldValue::Uint(0)),
        ("oncripple_explosion".to_string(), FieldValue::Uint(0)),
        ("oncripple_debris".to_string(), FieldValue::Uint(0)),
        ("oncripple_impact_dataset".to_string(), FieldValue::Uint(0)),
        (
            "explodable_subsegment_explosion".to_string(),
            FieldValue::Uint(0),
        ),
        (
            "orientation_limits_pitch".to_string(),
            FieldValue::Float(target.orientation_limit_pitch),
        ),
        (
            "orientation_limits_roll".to_string(),
            FieldValue::Float(target.orientation_limit_roll),
        ),
    ]);
    field_entry(
        "DATA",
        FieldValue::Struct(
            fields
                .into_iter()
                .map(|(name, value)| (interner.intern(&name), value))
                .collect(),
        ),
    )
}

fn emit_npc_record_variant(
    variant: &CreatureNpcRecordVariant,
    race: FormKey,
    skin: FormKey,
    inventory: &[CreatureNpcInventoryEntry],
    spells: &[CreatureTargetRecordReference],
    death_item: Option<&CreatureTargetRecordReference>,
    interner: &StringInterner,
) -> Record {
    let mut fields = vec![
        string_field("EDID", &variant.editor_id, interner),
        struct_field(
            "ACBS",
            vec![
                ("flags", FieldValue::Uint(0)),
                ("xp_value_offset", FieldValue::Int(0)),
                ("level", FieldValue::Uint(u64::from(variant.level))),
                ("calc_min_level", FieldValue::Uint(u64::from(variant.level))),
                ("calc_max_level", FieldValue::Uint(u64::from(variant.level))),
                ("disposition_base", FieldValue::Int(35)),
                ("template_flags", FieldValue::Uint(0)),
                ("bleedout_override", FieldValue::Uint(0)),
                ("unknown_u8_8", FieldValue::Uint(0)),
                ("unknown_u8_9", FieldValue::Uint(0)),
            ],
            interner,
        ),
    ];
    if let Some(death_item) = death_item {
        fields.push(form_field("INAM", death_item.form_key.intern(interner)));
    }
    fields.push(form_field("RNAM", race));
    fields.extend(
        spells
            .iter()
            .map(|spell| form_field("SPLO", spell.form_key.intern(interner))),
    );
    fields.extend([form_field("WNAM", skin), form_field("ATKR", race)]);
    if !inventory.is_empty() {
        fields.push(uint_field("COCT", inventory.len() as u64));
        for entry in inventory {
            fields.push(struct_field(
                "CNTO",
                vec![
                    (
                        "item",
                        FieldValue::FormKey(entry.target_record.form_key.intern(interner)),
                    ),
                    ("count", FieldValue::Int(i64::from(entry.count))),
                ],
                interner,
            ));
            if let Some(ownership) = &entry.ownership {
                let (owner, global_or_rank, condition) = match ownership {
                    CreatureNpcInventoryOwnership::FactionRank {
                        faction,
                        required_rank,
                        condition,
                    } => (
                        FieldValue::FormKey(faction.form_key.intern(interner)),
                        FieldValue::Int(i64::from(*required_rank)),
                        *condition,
                    ),
                    CreatureNpcInventoryOwnership::OwnerGlobal {
                        owner,
                        global,
                        condition,
                    } => (
                        owner.as_ref().map_or(FieldValue::Uint(0), |owner| {
                            FieldValue::FormKey(owner.form_key.intern(interner))
                        }),
                        global.as_ref().map_or(FieldValue::Uint(0), |global| {
                            FieldValue::FormKey(global.form_key.intern(interner))
                        }),
                        *condition,
                    ),
                };
                fields.push(struct_field(
                    "COED",
                    vec![
                        ("owner", owner),
                        ("global_variable_required_rank", global_or_rank),
                        ("item_condition", FieldValue::Float(condition)),
                    ],
                    interner,
                ));
            }
        }
    }
    fields.extend([
        uint_field("KSIZ", 0),
        list_field("KWDA", Vec::new()),
        string_field("FULL", &variant.display_name, interner),
        empty_field("DATA"),
        struct_field(
            "DNAM",
            vec![
                (
                    "calculated_health",
                    FieldValue::Uint(u64::from(variant.health)),
                ),
                (
                    "calculated_action_points",
                    FieldValue::Uint(u64::from(variant.action_points)),
                ),
                ("far_away_model_distance", FieldValue::Uint(0)),
                ("geared_up_weapons", FieldValue::Uint(1)),
                ("unknown_u8_4", FieldValue::Uint(0)),
            ],
            interner,
        ),
    ]);
    make_record(
        "NPC_",
        variant.form_key.intern(interner),
        &variant.editor_id,
        fields,
        interner,
    )
}

fn emit_attack_record_variant(
    attack: &CreatureAttackRecordVariant,
    fallout4: crate::sym::Sym,
    interner: &StringInterner,
) -> Record {
    let CreatureAttackRecordProjection::MeleeUnarmed {
        weapon_form_key,
        weapon_editor_id,
        damage,
        reach,
        attack_seconds,
    } = &attack.projection
    else {
        unreachable!("only melee projections emit an unarmed WEAP")
    };
    let base = |local| FormKey {
        local,
        plugin: fallout4,
    };
    make_record(
        "WEAP",
        weapon_form_key.intern(interner),
        weapon_editor_id,
        vec![
            string_field("EDID", weapon_editor_id, interner),
            string_field("FULL", &attack.id, interner),
            form_field("ETYP", base(BOTH_HANDS)),
            uint_field("KSIZ", 2),
            list_field(
                "KWDA",
                vec![
                    FieldValue::FormKey(base(ANIMS_UNARMED)),
                    FieldValue::FormKey(base(WEAPON_TYPE_UNARMED)),
                ],
            ),
            unarmed_weapon_data(
                *reach,
                *damage,
                *attack_seconds,
                attack.action_point_cost,
                interner,
            ),
            struct_field(
                "CRDT",
                vec![
                    ("crit_damage_mult", FieldValue::Float(1.0)),
                    ("crit_charge_bonus", FieldValue::Float(0.0)),
                    ("crit_effect", FieldValue::Uint(0)),
                ],
                interner,
            ),
        ],
        interner,
    )
}

fn unarmed_weapon_data(
    reach: f32,
    damage: u16,
    attack_seconds: f32,
    action_point_cost: f32,
    interner: &StringInterner,
) -> FieldEntry {
    struct_field(
        "DNAM",
        vec![
            ("ammo", FieldValue::Uint(0)),
            ("speed", FieldValue::Float(1.0)),
            ("reload_speed", FieldValue::Float(1.0)),
            ("reach", FieldValue::Float(reach)),
            ("min_range", FieldValue::Float(0.0)),
            ("max_range", FieldValue::Float(0.0)),
            ("attack_delay", FieldValue::Float(0.0)),
            ("unused", FieldValue::Float(0.0)),
            ("damage_outofrange_mult", FieldValue::Float(0.0)),
            ("on_hit", FieldValue::Uint(0)),
            ("skill", FieldValue::Uint(0)),
            ("resist", FieldValue::Uint(0)),
            ("flags", FieldValue::Uint(0)),
            ("capacity", FieldValue::Uint(0)),
            ("animation_type", FieldValue::Uint(0)),
            ("damage_secondary", FieldValue::Float(0.0)),
            ("weight", FieldValue::Float(0.0)),
            ("value", FieldValue::Uint(0)),
            ("damage_base", FieldValue::Uint(u64::from(damage))),
            ("sound_level", FieldValue::Uint(0)),
            ("sound_attack", FieldValue::Uint(0)),
            ("sound_attack_2d", FieldValue::Uint(0)),
            ("sound_attack_loop", FieldValue::Uint(0)),
            ("sound_attack_fail", FieldValue::Uint(0)),
            ("sound_idle", FieldValue::Uint(0)),
            ("sound_equip_sound", FieldValue::Uint(0)),
            ("sound_unequip_sound", FieldValue::Uint(0)),
            ("sound_fast_equip_sound", FieldValue::Uint(0)),
            ("accuracy_bonus", FieldValue::Uint(0)),
            (
                "animation_attack_seconds",
                FieldValue::Float(attack_seconds),
            ),
            ("unknown_u8_30", FieldValue::Uint(0)),
            ("unknown_u8_31", FieldValue::Uint(0)),
            ("action_point_cost", FieldValue::Float(action_point_cost)),
            ("full_power_seconds", FieldValue::Float(0.0)),
            ("min_power_per_shot", FieldValue::Float(0.0)),
            ("stagger", FieldValue::Uint(0)),
            ("unknown_u8_36", FieldValue::Uint(0)),
            ("unknown_u8_37", FieldValue::Uint(0)),
            ("unknown_u8_38", FieldValue::Uint(0)),
            ("unknown_u8_39", FieldValue::Uint(0)),
        ],
        interner,
    )
}

fn emit_npc(
    manifest: &CreatureRecordManifest,
    profile: &CreatureRecordProfile,
    keys: &InternedRecordKeys,
    interner: &StringInterner,
) -> Record {
    make_record(
        "NPC_",
        keys.npc,
        &manifest.editor_ids.npc,
        vec![
            string_field("EDID", &manifest.editor_ids.npc, interner),
            struct_field(
                "ACBS",
                vec![
                    ("flags", FieldValue::Uint(0)),
                    ("xp_value_offset", FieldValue::Int(0)),
                    ("level", FieldValue::Uint(u64::from(profile.npc_level))),
                    (
                        "calc_min_level",
                        FieldValue::Uint(u64::from(profile.npc_level)),
                    ),
                    (
                        "calc_max_level",
                        FieldValue::Uint(u64::from(profile.npc_level)),
                    ),
                    ("disposition_base", FieldValue::Int(35)),
                    ("template_flags", FieldValue::Uint(0)),
                    ("bleedout_override", FieldValue::Uint(0)),
                    ("unknown_u8_8", FieldValue::Uint(0)),
                    ("unknown_u8_9", FieldValue::Uint(0)),
                ],
                interner,
            ),
            form_field("RNAM", keys.race),
            form_field("WNAM", keys.skin),
            form_field("ATKR", keys.race),
            uint_field("KSIZ", 0),
            list_field("KWDA", Vec::new()),
            string_field("FULL", &manifest.display_name, interner),
            empty_field("DATA"),
            struct_field(
                "DNAM",
                vec![
                    (
                        "calculated_health",
                        FieldValue::Uint(u64::from(profile.npc_health)),
                    ),
                    (
                        "calculated_action_points",
                        FieldValue::Uint(u64::from(profile.npc_action_points)),
                    ),
                    ("far_away_model_distance", FieldValue::Uint(0)),
                    ("geared_up_weapons", FieldValue::Uint(1)),
                    ("unknown_u8_4", FieldValue::Uint(0)),
                ],
                interner,
            ),
        ],
        interner,
    )
}

fn emit_skin(
    manifest: &CreatureRecordManifest,
    keys: &InternedRecordKeys,
    interner: &StringInterner,
) -> Record {
    emit_skin_parts(
        manifest,
        keys,
        &[CreatureBodyNifRecordPart {
            body_nif: manifest.body_nif.clone(),
            armor_addon_form_key: manifest.form_keys.armor_addon.clone(),
            armor_addon_editor_id: manifest.editor_ids.armor_addon.clone(),
        }],
        interner,
    )
}

fn emit_skin_parts(
    manifest: &CreatureRecordManifest,
    keys: &InternedRecordKeys,
    body_nif_parts: &[CreatureBodyNifRecordPart],
    interner: &StringInterner,
) -> Record {
    let mut fields = vec![
        string_field("EDID", &manifest.editor_ids.skin, interner),
        uint_field("BOD2", u64::from(BODY_SLOT_33)),
        form_field("RNAM", keys.race),
        uint_field("KSIZ", 0),
        list_field("KWDA", Vec::new()),
    ];
    for (index, part) in body_nif_parts.iter().enumerate() {
        fields.push(uint_field("INDX", index as u64));
        fields.push(form_field(
            "MODL",
            part.armor_addon_form_key.intern(interner),
        ));
    }
    fields.push(struct_field(
        "DATA",
        vec![
            ("value", FieldValue::Int(0)),
            ("weight", FieldValue::Float(0.0)),
            ("health", FieldValue::Uint(0)),
        ],
        interner,
    ));
    make_record(
        "ARMO",
        keys.skin,
        &manifest.editor_ids.skin,
        fields,
        interner,
    )
}

fn emit_armor_addon(
    manifest: &CreatureRecordManifest,
    keys: &InternedRecordKeys,
    interner: &StringInterner,
) -> Record {
    emit_armor_addon_part(
        keys.race,
        &CreatureBodyNifRecordPart {
            body_nif: manifest.body_nif.clone(),
            armor_addon_form_key: manifest.form_keys.armor_addon.clone(),
            armor_addon_editor_id: manifest.editor_ids.armor_addon.clone(),
        },
        interner,
    )
}

fn emit_armor_addon_part(
    race: FormKey,
    part: &CreatureBodyNifRecordPart,
    interner: &StringInterner,
) -> Record {
    make_record(
        "ARMA",
        part.armor_addon_form_key.intern(interner),
        &part.armor_addon_editor_id,
        vec![
            string_field("EDID", &part.armor_addon_editor_id, interner),
            uint_field("BOD2", u64::from(BODY_SLOT_33)),
            form_field("RNAM", race),
            struct_field(
                "DNAM",
                vec![
                    ("male_priority", FieldValue::Uint(0)),
                    ("female_priority", FieldValue::Uint(0)),
                    ("weight_slider_male", FieldValue::Uint(0)),
                    ("weight_slider_female", FieldValue::Uint(0)),
                    ("unknown_u8_4", FieldValue::Uint(0)),
                    ("unknown_u8_5", FieldValue::Uint(0)),
                    ("detection_sound_value", FieldValue::Uint(0)),
                    ("unknown_u8_7", FieldValue::Uint(0)),
                    ("weapon_adjust", FieldValue::Float(0.0)),
                ],
                interner,
            ),
            string_field("MOD2", &part.body_nif, interner),
            string_field("MOD3", &part.body_nif, interner),
        ],
        interner,
    )
}

fn emit_body_part_data(
    rig: &CreatureManifest,
    manifest: &CreatureRecordManifest,
    profile: &CreatureRecordProfile,
    keys: &InternedRecordKeys,
    interner: &StringInterner,
) -> Record {
    let body = &profile.root_body_part;
    make_record(
        "BPTD",
        keys.body_part_data,
        &manifest.editor_ids.body_part_data,
        vec![
            string_field("EDID", &manifest.editor_ids.body_part_data, interner),
            string_field("MODL", &rig.visual_skeleton_nif, interner),
            string_field("BPTN", &body.name, interner),
            string_field("BPNN", &body.node, interner),
            string_field("BPNT", &body.vats_target, interner),
            struct_field(
                "BPND",
                vec![
                    ("damage_mult", FieldValue::Float(1.0)),
                    ("explodable_debris", FieldValue::Uint(0)),
                    ("explodable_explosion", FieldValue::Uint(0)),
                    ("explodable_debris_scale", FieldValue::Float(1.0)),
                    ("severable_debris", FieldValue::Uint(0)),
                    ("severable_explosion", FieldValue::Uint(0)),
                    ("severable_debris_scale", FieldValue::Float(1.0)),
                    ("cut_min", FieldValue::Float(0.0)),
                    ("cut_max", FieldValue::Float(0.0)),
                    ("cut_radius", FieldValue::Float(0.0)),
                    ("gore_effects_local_rotate_x", FieldValue::Float(0.0)),
                    ("gore_effects_local_rotate_y", FieldValue::Float(0.0)),
                    ("cut_tesselation", FieldValue::Float(0.0)),
                    ("severable_impact_dataset", FieldValue::Uint(0)),
                    ("explodable_impact_dataset", FieldValue::Uint(0)),
                    ("explodable_limb_replacement_scale", FieldValue::Float(1.0)),
                    ("flags", FieldValue::Uint(0)),
                    ("part_type", FieldValue::Uint(18)),
                    ("health_percent", FieldValue::Uint(0)),
                    ("actor_value", FieldValue::Uint(0)),
                    ("to_hit_chance", FieldValue::Uint(100)),
                    ("explodable_explosion_chance", FieldValue::Uint(0)),
                    ("non_lethal_dismemberment_chance", FieldValue::Uint(0)),
                    ("severable_debris_count", FieldValue::Uint(0)),
                    ("explodable_debris_count", FieldValue::Uint(0)),
                    ("severable_decal_count", FieldValue::Uint(0)),
                    ("explodable_decal_count", FieldValue::Uint(0)),
                    (
                        "geometry_segment_index",
                        FieldValue::Uint(u64::from(body.geometry_segment_index)),
                    ),
                    ("on_cripple_art_object", FieldValue::Uint(0)),
                    ("on_cripple_debris", FieldValue::Uint(0)),
                    ("on_cripple_explosion", FieldValue::Uint(0)),
                    ("on_cripple_impact_dataset", FieldValue::Uint(0)),
                    ("on_cripple_debris_scale", FieldValue::Float(1.0)),
                    ("on_cripple_debris_count", FieldValue::Uint(0)),
                    ("on_cripple_decal_count", FieldValue::Uint(0)),
                ],
                interner,
            ),
            string_field("NAM1", "", interner),
            string_field("NAM4", &body.node, interner),
            bytes_field("NAM5", Vec::new()),
            string_field("ENAM", &body.node, interner),
            string_field("FNAM", &body.node, interner),
            none_field("BNAM"),
            none_field("INAM"),
            none_field("JNAM"),
            none_field("CNAM"),
            none_field("NAM2"),
            string_field("DNAM", "", interner),
        ],
        interner,
    )
}

fn emit_unarmed_weapon(
    manifest: &CreatureRecordManifest,
    profile: &CreatureRecordProfile,
    keys: &InternedRecordKeys,
    fallout4: crate::sym::Sym,
    interner: &StringInterner,
) -> Record {
    let base = |local| FormKey {
        local,
        plugin: fallout4,
    };
    make_record(
        "WEAP",
        keys.unarmed_weapon,
        &manifest.editor_ids.unarmed_weapon,
        vec![
            string_field("EDID", &manifest.editor_ids.unarmed_weapon, interner),
            string_field(
                "FULL",
                &format!("{} Unarmed", manifest.display_name),
                interner,
            ),
            form_field("ETYP", base(BOTH_HANDS)),
            uint_field("KSIZ", 2),
            list_field(
                "KWDA",
                vec![
                    FieldValue::FormKey(base(ANIMS_UNARMED)),
                    FieldValue::FormKey(base(WEAPON_TYPE_UNARMED)),
                ],
            ),
            struct_field(
                "DNAM",
                vec![
                    ("ammo", FieldValue::Uint(0)),
                    ("speed", FieldValue::Float(1.0)),
                    ("reload_speed", FieldValue::Float(1.0)),
                    ("reach", FieldValue::Float(profile.unarmed_reach)),
                    ("min_range", FieldValue::Float(0.0)),
                    ("max_range", FieldValue::Float(0.0)),
                    ("attack_delay", FieldValue::Float(0.0)),
                    ("unused", FieldValue::Float(0.0)),
                    ("damage_outofrange_mult", FieldValue::Float(0.0)),
                    ("on_hit", FieldValue::Uint(0)),
                    ("skill", FieldValue::Uint(0)),
                    ("resist", FieldValue::Uint(0)),
                    ("flags", FieldValue::Uint(0)),
                    ("capacity", FieldValue::Uint(0)),
                    ("animation_type", FieldValue::Uint(0)),
                    ("damage_secondary", FieldValue::Float(0.0)),
                    ("weight", FieldValue::Float(0.0)),
                    ("value", FieldValue::Uint(0)),
                    (
                        "damage_base",
                        FieldValue::Uint(u64::from(profile.unarmed_damage)),
                    ),
                    ("sound_level", FieldValue::Uint(0)),
                    ("sound_attack", FieldValue::Uint(0)),
                    ("sound_attack_2d", FieldValue::Uint(0)),
                    ("sound_attack_loop", FieldValue::Uint(0)),
                    ("sound_attack_fail", FieldValue::Uint(0)),
                    ("sound_idle", FieldValue::Uint(0)),
                    ("sound_equip_sound", FieldValue::Uint(0)),
                    ("sound_unequip_sound", FieldValue::Uint(0)),
                    ("sound_fast_equip_sound", FieldValue::Uint(0)),
                    ("accuracy_bonus", FieldValue::Uint(0)),
                    (
                        "animation_attack_seconds",
                        FieldValue::Float(profile.unarmed_attack_seconds),
                    ),
                    ("unknown_u8_30", FieldValue::Uint(0)),
                    ("unknown_u8_31", FieldValue::Uint(0)),
                    (
                        "action_point_cost",
                        FieldValue::Float(profile.action_point_cost),
                    ),
                    ("full_power_seconds", FieldValue::Float(0.0)),
                    ("min_power_per_shot", FieldValue::Float(0.0)),
                    ("stagger", FieldValue::Uint(0)),
                    ("unknown_u8_36", FieldValue::Uint(0)),
                    ("unknown_u8_37", FieldValue::Uint(0)),
                    ("unknown_u8_38", FieldValue::Uint(0)),
                    ("unknown_u8_39", FieldValue::Uint(0)),
                ],
                interner,
            ),
            struct_field(
                "CRDT",
                vec![
                    ("crit_damage_mult", FieldValue::Float(1.0)),
                    ("crit_charge_bonus", FieldValue::Float(0.0)),
                    ("crit_effect", FieldValue::Uint(0)),
                ],
                interner,
            ),
        ],
        interner,
    )
}

#[derive(Clone, Copy)]
struct InternedRecordKeys {
    race: FormKey,
    npc: FormKey,
    skin: FormKey,
    armor_addon: FormKey,
    body_part_data: FormKey,
    unarmed_weapon: FormKey,
}

impl InternedRecordKeys {
    fn new(keys: &CreatureRecordFormKeys, interner: &StringInterner) -> Self {
        Self {
            race: keys.race.intern(interner),
            npc: keys.npc.intern(interner),
            skin: keys.skin.intern(interner),
            armor_addon: keys.armor_addon.intern(interner),
            body_part_data: keys.body_part_data.intern(interner),
            unarmed_weapon: keys.unarmed_weapon.intern(interner),
        }
    }
}

fn make_record(
    signature: &str,
    form_key: FormKey,
    editor_id: &str,
    fields: Vec<FieldEntry>,
    interner: &StringInterner,
) -> Record {
    let mut record = Record::new(
        SigCode::from_str(signature).expect("static record signature"),
        form_key,
    );
    record.eid = Some(interner.intern(editor_id));
    record.fields = SmallVec::from_vec(fields);
    record
}

fn field_entry(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).expect("static subrecord signature"),
        value,
    }
}

fn string_field(signature: &str, value: &str, interner: &StringInterner) -> FieldEntry {
    field_entry(signature, FieldValue::String(interner.intern(value)))
}

fn form_field(signature: &str, value: FormKey) -> FieldEntry {
    field_entry(signature, FieldValue::FormKey(value))
}

fn uint_field(signature: &str, value: u64) -> FieldEntry {
    field_entry(signature, FieldValue::Uint(value))
}

fn list_field(signature: &str, value: Vec<FieldValue>) -> FieldEntry {
    field_entry(signature, FieldValue::List(value))
}

fn empty_field(signature: &str) -> FieldEntry {
    bytes_field(signature, Vec::new())
}

fn bytes_field(signature: &str, value: Vec<u8>) -> FieldEntry {
    field_entry(signature, FieldValue::Bytes(SmallVec::from_vec(value)))
}

fn none_field(signature: &str) -> FieldEntry {
    field_entry(signature, FieldValue::None)
}

fn struct_field(
    signature: &str,
    fields: Vec<(&str, FieldValue)>,
    interner: &StringInterner,
) -> FieldEntry {
    field_entry(
        signature,
        FieldValue::Struct(
            fields
                .into_iter()
                .map(|(name, value)| (interner.intern(name), value))
                .collect(),
        ),
    )
}

fn validate_form_keys(
    value: &FieldValue,
    emitted: &HashSet<(u32, String)>,
    allowed_dependencies: &HashSet<(u32, String)>,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    match value {
        FieldValue::FormKey(form_key) => {
            let plugin = interner
                .resolve(form_key.plugin)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !plugin.eq_ignore_ascii_case(FALLOUT4_MASTER)
                && !emitted.contains(&(form_key.local, plugin.clone()))
                && !allowed_dependencies.contains(&(form_key.local, plugin.clone()))
            {
                let code = if is_source_plugin(&plugin) {
                    "source_form_key"
                } else {
                    "unresolved_form_key"
                };
                return Err(invalid(
                    code,
                    format!(
                        "FormKey {:06X}@{plugin} is outside the closure",
                        form_key.local
                    ),
                ));
            }
        }
        FieldValue::List(items) => {
            for item in items {
                validate_form_keys(item, emitted, allowed_dependencies, interner)?;
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                validate_form_keys(value, emitted, allowed_dependencies, interner)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn assert_form_link(
    record: Option<&Record>,
    signature: &str,
    expected: &TargetFormKey,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    let actual = record
        .and_then(|record| field(record, signature))
        .and_then(|value| match value {
            FieldValue::FormKey(value) => Some(value),
            _ => None,
        });
    if actual.is_some_and(|actual| {
        actual.local == expected.local
            && interner
                .resolve(actual.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case(&expected.plugin))
    }) {
        return Ok(());
    }
    Err(invalid(
        "record_link",
        format!(
            "missing {signature} -> {:06X}@{}",
            expected.local, expected.plugin
        ),
    ))
}

fn assert_string_value(
    record: Option<&Record>,
    signature: &str,
    expected: &str,
    interner: &StringInterner,
) -> Result<(), CreatureRecordError> {
    let matches = record.is_some_and(|record| {
        record.fields.iter().any(|entry| {
            entry.sig.as_str() == signature
                && matches!(
                    entry.value,
                    FieldValue::String(value)
                        if interner.resolve(value).is_some_and(|value| value == expected)
                )
        })
    });
    if matches {
        Ok(())
    } else {
        Err(invalid(
            "record_string",
            format!("missing {signature} value {expected:?}"),
        ))
    }
}

fn field<'a>(record: &'a Record, signature: &str) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == signature)
        .map(|entry| &entry.value)
}

fn struct_member<'a>(
    value: &'a FieldValue,
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
}

fn uint_value(value: &FieldValue) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Bytes(bytes) => match bytes.len() {
            1 => Some(u64::from(bytes[0])),
            2 => Some(u64::from(u16::from_le_bytes([bytes[0], bytes[1]]))),
            4 => Some(u64::from(u32::from_le_bytes([
                bytes[0], bytes[1], bytes[2], bytes[3],
            ]))),
            _ => None,
        },
        _ => None,
    }
}

fn normalized_field_width(value: &FieldValue) -> usize {
    match value {
        FieldValue::None => 0,
        FieldValue::Bool(_) => 1,
        FieldValue::Int(_) | FieldValue::Uint(_) | FieldValue::Float(_) => 4,
        FieldValue::Bytes(bytes) => bytes.len(),
        FieldValue::FormKey(_) => 4,
        FieldValue::List(items) => items.iter().map(normalized_field_width).sum(),
        FieldValue::Struct(fields) => fields
            .iter()
            .map(|(_, value)| normalized_field_width(value))
            .sum(),
        FieldValue::String(_) => 0,
    }
}

fn fixed_struct_codec_size(codec: &str) -> Option<usize> {
    let fields = codec.strip_prefix("struct:")?;
    fields.split(',').try_fold(0usize, |size, field| {
        let width = match field.trim() {
            "B" | "b" | "u8" | "i8" | "uint8" | "int8" => 1,
            "H" | "h" | "u16" | "i16" | "uint16" | "int16" => 2,
            "I" | "i" | "f" | "u32" | "i32" | "uint32" | "int32" | "float" | "float32"
            | "formid" | "form_id" => 4,
            "Q" | "q" | "d" | "u64" | "i64" | "uint64" | "int64" | "double" => 8,
            _ => return None,
        };
        Some(size + width)
    })
}

fn invalid(code: &'static str, message: impl Into<String>) -> CreatureRecordError {
    CreatureRecordError::InvalidManifest {
        code,
        message: message.into(),
    }
}

#[cfg(test)]
mod attack_semantics_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn reference(signature: &str, local: u32) -> CreatureTargetRecordReference {
        CreatureTargetRecordReference {
            signature: signature.to_string(),
            form_key: TargetFormKey::new(local, "CreatureOutput.esp"),
        }
    }

    fn contract(
        template: CreatureGraphTemplate,
        event: &str,
        role: CreatureClipRole,
    ) -> ProjectionGraphContract {
        ProjectionGraphContract {
            template,
            declared_events: BTreeMap::from([(event.to_ascii_lowercase(), EventUsage::Generic)]),
            event_roles: BTreeMap::from([(event.to_ascii_lowercase(), role)]),
            candidate_attack_roles: BTreeMap::new(),
            candidate_bound_events: BTreeSet::new(),
            required_primary_event: None,
        }
    }

    fn source_identity() -> SourceCreatureIdentity {
        SourceCreatureIdentity {
            namespace: "fixture".to_string(),
            plugin: "Fixture.esm".to_string(),
            local_form_id: 0x800,
        }
    }

    fn attack(
        event: &str,
        projection: CreatureAttackRecordProjection,
    ) -> CreatureAttackRecordVariant {
        CreatureAttackRecordVariant {
            id: "attack".to_string(),
            event: event.to_string(),
            primary: true,
            projection,
            damage_multiplier: 1.0,
            chance: 1.0,
            strike_angle: 30.0,
            action_point_cost: 10.0,
            target_data: CreatureAttackTargetData::default(),
        }
    }

    #[test]
    fn stationary_and_continuous_attacks_require_exact_projection_variants() {
        let stationary = contract(
            CreatureGraphTemplate::StationaryTurret,
            "fireTurret",
            CreatureClipRole::ProjectileAttack,
        );
        let stationary_attack = attack(
            "fireTurret",
            CreatureAttackRecordProjection::Stationary {
                attack_spell: reference("SPEL", 0x900),
                projectile: reference("PROJ", 0x901),
                equipment: reference("WEAP", 0x902),
            },
        );
        let coerced_stationary = attack(
            "fireTurret",
            CreatureAttackRecordProjection::RangedProjectile {
                attack_spell: reference("SPEL", 0x900),
                projectile: reference("PROJ", 0x901),
                equipment: reference("WEAP", 0x902),
            },
        );
        assert!(stationary.validates(&source_identity(), &stationary_attack));
        assert!(!stationary.validates(&source_identity(), &coerced_stationary));

        let continuous = contract(
            CreatureGraphTemplate::RobotContinuousAttack,
            "startBeam",
            CreatureClipRole::ContinuousAttackStart,
        );
        let continuous_attack = attack(
            "startBeam",
            CreatureAttackRecordProjection::ContinuousRobot {
                attack_spell: reference("SPEL", 0x910),
                equipment: reference("WEAP", 0x911),
            },
        );
        let coerced_continuous = attack(
            "startBeam",
            CreatureAttackRecordProjection::SpellAbility {
                spell: reference("SPEL", 0x910),
            },
        );
        assert!(continuous.validates(&source_identity(), &continuous_attack));
        assert!(!continuous.validates(&source_identity(), &coerced_continuous));
    }

    #[test]
    fn candidate_bound_event_accepts_only_the_exact_source_attack_kind() {
        let event = "attackStart_Attack1";
        let melee_source = source_identity();
        let spell_source = SourceCreatureIdentity {
            namespace: "fixture".to_string(),
            plugin: "Fixture.esm".to_string(),
            local_form_id: 0x801,
        };
        let absent_source = SourceCreatureIdentity {
            namespace: "fixture".to_string(),
            plugin: "Fixture.esm".to_string(),
            local_form_id: 0x802,
        };
        let contract = ProjectionGraphContract {
            template: CreatureGraphTemplate::GroundMeleeRanged,
            declared_events: BTreeMap::from([(event.to_ascii_lowercase(), EventUsage::Generic)]),
            event_roles: BTreeMap::from([(
                event.to_ascii_lowercase(),
                CreatureClipRole::MeleeAttack,
            )]),
            candidate_attack_roles: BTreeMap::from([
                (
                    (melee_source.stable_key(), event.to_ascii_lowercase()),
                    CreatureClipRole::MeleeAttack,
                ),
                (
                    (spell_source.stable_key(), event.to_ascii_lowercase()),
                    CreatureClipRole::ProjectileAttack,
                ),
            ]),
            candidate_bound_events: BTreeSet::from([event.to_ascii_lowercase()]),
            required_primary_event: None,
        };
        let melee = attack(
            event,
            CreatureAttackRecordProjection::MeleeEquipment {
                equipment: vec![reference("WEAP", 0x940)],
            },
        );
        let spell = attack(
            event,
            CreatureAttackRecordProjection::SpellAbility {
                spell: reference("SPEL", 0x941),
            },
        );
        let generated_unarmed = attack(
            event,
            CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key: TargetFormKey::new(0x942, "CreatureOutput.esp"),
                weapon_editor_id: "GeneratedFallback".to_string(),
                damage: 10,
                reach: 1.0,
                attack_seconds: 0.5,
            },
        );
        let mut source_backed_unarmed = generated_unarmed.clone();
        source_backed_unarmed.target_data.source_atkd = Some(CreatureAttackSourceDataReceipt {
            schema_id: "fixture_v1".to_string(),
            damage_multiplier_bits: 1.0_f32.to_bits(),
            chance_bits: 1.0_f32.to_bits(),
            attack_spell: None,
            attack_flags: 0,
            attack_angle_bits: 0.0_f32.to_bits(),
            strike_angle_bits: 0.0_f32.to_bits(),
            stagger_bits: 0.0_f32.to_bits(),
            attack_type: None,
            knockdown_bits: 0.0_f32.to_bits(),
            recovery_time_bits: 0.0_f32.to_bits(),
            stamina_multiplier_bits: 1.0_f32.to_bits(),
            event: event.to_string(),
            ordinal: 0,
            attack_type_policy: CreatureAttackTypePolicy::NullOmitted,
            attack_spell_policy: CreatureAttackSpellPolicy::NoSourceSpell,
            stamina_multiplier_policy:
                CreatureAttackStaminaPolicy::PreserveAsActionPointsMultiplier {
                    proof: CreatureSemanticProofReceipt {
                        schema: "fixture".to_string(),
                        canonical_json: "{}".to_string(),
                        canonical_json_blake3: blake3::hash(b"{}").to_hex().to_string(),
                    },
                },
            stagger_offset_policy: CreatureAttackStaggerOffsetPolicy::ExplicitZero {
                proof: CreatureSemanticProofReceipt {
                    schema: "fixture".to_string(),
                    canonical_json: "{}".to_string(),
                    canonical_json_blake3: blake3::hash(b"{}").to_hex().to_string(),
                },
            },
        });

        assert!(contract.validates(&melee_source, &melee));
        assert!(!contract.validates(&melee_source, &spell));
        assert!(contract.validates(&spell_source, &spell));
        assert!(!contract.validates(&spell_source, &melee));
        assert!(!contract.validates(&absent_source, &melee));
        assert!(!contract.validates(&absent_source, &spell));
        assert!(contract.validates(&spell_source, &generated_unarmed));
        assert!(contract.validates(&absent_source, &generated_unarmed));
        assert!(contract.validates(&spell_source, &source_backed_unarmed));
        assert!(contract.validates(&absent_source, &source_backed_unarmed));
    }

    #[test]
    fn projection_dependencies_and_equipment_follow_declared_attack_kind() {
        let spell = CreatureAttackRecordProjection::SpellAbility {
            spell: reference("SPEL", 0x920),
        };
        assert_eq!(spell.target_dependencies().len(), 1);
        assert!(spell.equipment().is_empty());

        let stationary = CreatureAttackRecordProjection::Stationary {
            attack_spell: reference("SPEL", 0x930),
            projectile: reference("PROJ", 0x931),
            equipment: reference("WEAP", 0x932),
        };
        assert_eq!(stationary.target_dependencies().len(), 3);
        assert_eq!(stationary.equipment()[0].signature, "WEAP");

        let continuous = CreatureAttackRecordProjection::ContinuousRobot {
            attack_spell: reference("SPEL", 0x940),
            equipment: reference("WEAP", 0x941),
        };
        assert_eq!(continuous.target_dependencies().len(), 2);
        assert_eq!(continuous.equipment()[0].signature, "WEAP");
    }
}

fn starts_with_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn has_authoring_extension(lower: &str) -> bool {
    [".xml", ".hkt", ".hkp", ".hkc", ".hkb"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn has_plugin_extension(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [".esp", ".esm", ".esl"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn is_source_plugin(value: &str) -> bool {
    ["skyrim.esm", "falloutnv.esm", "fallout3.esm"]
        .iter()
        .any(|plugin| value.eq_ignore_ascii_case(plugin))
}
