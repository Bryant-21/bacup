//! Versioned, source-neutral precommit recipes for creature record projection.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;

use super::{
    CapabilityGraphManifest, CreatureActorActionRecordPlan, CreatureAttackRecordProjection,
    CreatureManifest, CreatureMeleeRecordKeyPlan, CreatureNpcInventoryOwnership,
    CreatureNpcRecordKeyPlan, CreaturePrimaryRecordMapping, CreatureRecordFamilyBatch,
    CreatureRecordKeyPlan, CreatureRecordProjectionBuildInput, CreatureRecordProjectionClosure,
    CreatureRecordProjectionManifest, CreatureTargetRecordReference, SourceRigRecipeBridgeReceipt,
    SourceRigRuntimeModelClosureReceipt, actor_action_admission_report,
    append_creature_actor_action_records, emit_creature_capability_record_projection,
    required_actor_action_records,
};
use crate::sym::StringInterner;

pub const SOURCE_RIG_RECIPE_VERSION: u32 = 9;

/// Intent required to construct a batch. This is deliberately not a commit receipt.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigRecordBatchIntent {
    pub family_id: String,
    pub primary_mapping: CreaturePrimaryRecordMapping,
    pub required_target_records: Vec<CreatureTargetRecordReference>,
}

impl SourceRigRecordBatchIntent {
    pub fn with_actor_action_dependencies(
        mut self,
        actor_action_records: &[CreatureActorActionRecordPlan],
    ) -> Self {
        self.required_target_records
            .extend(actor_action_dependencies(actor_action_records));
        sort_dependencies(&mut self.required_target_records);
        self
    }
}

/// How one persisted projection value was chosen.
///
/// Source evidence retains the source game but never stores a donor path or FormKey.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceRigFieldDecision {
    Source {
        source_game: String,
        source_field: String,
    },
    Policy {
        policy_id: String,
    },
    Derived {
        source_games: Vec<String>,
        source_fields: Vec<String>,
        policy_id: String,
    },
}

/// A decision receipt bound to the serialized value by BLAKE3.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRigFieldReceipt {
    pub field: String,
    pub value_blake3: String,
    pub decision: SourceRigFieldDecision,
}

/// Executable precommit recipe. It rebuilds projection and batch inputs but cannot publish them.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceRigExecutableRecipe {
    pub version: u32,
    pub rig: CreatureManifest,
    pub graph: CapabilityGraphManifest,
    pub projection: CreatureRecordProjectionManifest,
    pub key_plan: CreatureRecordKeyPlan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actor_action_records: Vec<CreatureActorActionRecordPlan>,
    pub batch_intent: SourceRigRecordBatchIntent,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_receipt: Option<SourceRigRecipeBridgeReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_model_closure: Option<SourceRigRuntimeModelClosureReceipt>,
    pub field_receipts: Vec<SourceRigFieldReceipt>,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SourceRigRecipeError {
    #[error("unsupported source-rig recipe version {actual}; expected {expected}")]
    UnsupportedVersion { actual: u32, expected: u32 },
    #[error("invalid source-rig recipe ({code}): {message}")]
    Invalid { code: &'static str, message: String },
    #[error("source-rig recipe serialization failed: {0}")]
    Serialization(String),
    #[error("source-rig recipe projection failed: {0}")]
    Projection(String),
}

impl SourceRigExecutableRecipe {
    pub fn new(
        rig: CreatureManifest,
        graph: CapabilityGraphManifest,
        projection: CreatureRecordProjectionManifest,
        key_plan: CreatureRecordKeyPlan,
        batch_intent: SourceRigRecordBatchIntent,
        field_receipts: Vec<SourceRigFieldReceipt>,
    ) -> Result<Self, SourceRigRecipeError> {
        Self::new_inner(
            rig,
            graph,
            projection,
            key_plan,
            Vec::new(),
            batch_intent,
            None,
            field_receipts,
        )
    }

    pub fn new_bridged(
        rig: CreatureManifest,
        graph: CapabilityGraphManifest,
        projection: CreatureRecordProjectionManifest,
        key_plan: CreatureRecordKeyPlan,
        batch_intent: SourceRigRecordBatchIntent,
        bridge_receipt: SourceRigRecipeBridgeReceipt,
        field_receipts: Vec<SourceRigFieldReceipt>,
    ) -> Result<Self, SourceRigRecipeError> {
        Self::new_inner(
            rig,
            graph,
            projection,
            key_plan,
            Vec::new(),
            batch_intent,
            Some(bridge_receipt),
            field_receipts,
        )
    }

    pub fn new_with_actor_actions(
        rig: CreatureManifest,
        graph: CapabilityGraphManifest,
        projection: CreatureRecordProjectionManifest,
        key_plan: CreatureRecordKeyPlan,
        actor_action_records: Vec<CreatureActorActionRecordPlan>,
        batch_intent: SourceRigRecordBatchIntent,
        field_receipts: Vec<SourceRigFieldReceipt>,
    ) -> Result<Self, SourceRigRecipeError> {
        Self::new_inner(
            rig,
            graph,
            projection,
            key_plan,
            actor_action_records,
            batch_intent,
            None,
            field_receipts,
        )
    }

    pub fn new_bridged_with_actor_actions(
        rig: CreatureManifest,
        graph: CapabilityGraphManifest,
        projection: CreatureRecordProjectionManifest,
        key_plan: CreatureRecordKeyPlan,
        actor_action_records: Vec<CreatureActorActionRecordPlan>,
        batch_intent: SourceRigRecordBatchIntent,
        bridge_receipt: SourceRigRecipeBridgeReceipt,
        field_receipts: Vec<SourceRigFieldReceipt>,
    ) -> Result<Self, SourceRigRecipeError> {
        Self::new_inner(
            rig,
            graph,
            projection,
            key_plan,
            actor_action_records,
            batch_intent,
            Some(bridge_receipt),
            field_receipts,
        )
    }

    fn new_inner(
        rig: CreatureManifest,
        graph: CapabilityGraphManifest,
        projection: CreatureRecordProjectionManifest,
        key_plan: CreatureRecordKeyPlan,
        actor_action_records: Vec<CreatureActorActionRecordPlan>,
        batch_intent: SourceRigRecordBatchIntent,
        bridge_receipt: Option<SourceRigRecipeBridgeReceipt>,
        field_receipts: Vec<SourceRigFieldReceipt>,
    ) -> Result<Self, SourceRigRecipeError> {
        let mut recipe = Self {
            version: SOURCE_RIG_RECIPE_VERSION,
            rig,
            graph,
            projection,
            key_plan,
            actor_action_records,
            batch_intent,
            bridge_receipt,
            runtime_model_closure: None,
            field_receipts,
        };
        recipe.normalize();
        recipe.validate()?;
        Ok(recipe)
    }

    pub fn from_json(json: &str) -> Result<Self, SourceRigRecipeError> {
        let mut recipe: Self = serde_json::from_str(json)
            .map_err(|error| SourceRigRecipeError::Serialization(error.to_string()))?;
        if recipe.version == 1 {
            if !recipe.projection.body_nif_parts.is_empty()
                || !recipe.key_plan.armor_addons.is_empty()
            {
                return Err(invalid(
                    "legacy_multipart_recipe",
                    "version 1 recipes cannot declare multipart body NIF identities",
                ));
            }
            recipe.version = SOURCE_RIG_RECIPE_VERSION;
        } else if (recipe.version == 4 && recipe.runtime_model_closure.is_none())
            || matches!(recipe.version, 5 | 6 | 7 | 8)
        {
            recipe.version = SOURCE_RIG_RECIPE_VERSION;
        }
        recipe.normalize();
        recipe.validate()?;
        Ok(recipe)
    }

    pub fn canonical_json(&self) -> Result<String, SourceRigRecipeError> {
        let mut recipe = self.clone();
        recipe.normalize();
        recipe.validate()?;
        serde_json::to_string(&recipe)
            .map_err(|error| SourceRigRecipeError::Serialization(error.to_string()))
    }

    pub fn stable_hash(&self) -> Result<String, SourceRigRecipeError> {
        Ok(blake3::hash(self.canonical_json()?.as_bytes())
            .to_hex()
            .to_string())
    }

    pub fn stable_hash_blake3(&self) -> Result<String, SourceRigRecipeError> {
        self.stable_hash()
    }

    pub fn with_runtime_model_closure(
        mut self,
        receipt: SourceRigRuntimeModelClosureReceipt,
    ) -> Result<Self, SourceRigRecipeError> {
        receipt.validate_structure().map_err(|error| {
            invalid(
                "runtime_model_closure",
                format!("invalid runtime-model closure receipt: {error}"),
            )
        })?;
        self.runtime_model_closure = Some(receipt);
        self.validate()?;
        Ok(self)
    }

    pub fn rebuild_key_plan(&self) -> Result<CreatureRecordKeyPlan, SourceRigRecipeError> {
        let mut recipe = self.clone();
        recipe.normalize();
        recipe.validate()?;
        Ok(recipe.key_plan)
    }

    pub fn rebuild_projection_input(
        &self,
    ) -> Result<CreatureRecordProjectionBuildInput, SourceRigRecipeError> {
        let mut recipe = self.clone();
        recipe.normalize();
        recipe.validate()?;
        Ok(CreatureRecordProjectionBuildInput {
            planned_source_identity: recipe.key_plan.planned_source_identity,
            rig: recipe.rig,
            graph: recipe.graph,
            projection: recipe.projection,
        })
    }

    pub fn rebuild_projection_closure(
        &self,
        interner: &StringInterner,
    ) -> Result<CreatureRecordProjectionClosure, SourceRigRecipeError> {
        let input = self.rebuild_projection_input()?;
        let mut closure = emit_creature_capability_record_projection(
            &input.rig,
            &input.graph,
            &input.projection,
            interner,
        )
        .map_err(|error| SourceRigRecipeError::Projection(error.to_string()))?;
        append_creature_actor_action_records(&mut closure, &self.actor_action_records, interner)
            .map_err(|error| SourceRigRecipeError::Projection(error.to_string()))?;
        Ok(closure)
    }

    pub fn rebuild_record_family_batch(
        &self,
        interner: &StringInterner,
    ) -> Result<CreatureRecordFamilyBatch, SourceRigRecipeError> {
        let mut recipe = self.clone();
        recipe.normalize();
        recipe.validate()?;
        let mut closure = emit_creature_capability_record_projection(
            &recipe.rig,
            &recipe.graph,
            &recipe.projection,
            interner,
        )
        .map_err(|error| SourceRigRecipeError::Projection(error.to_string()))?;
        append_creature_actor_action_records(&mut closure, &recipe.actor_action_records, interner)
            .map_err(|error| SourceRigRecipeError::Projection(error.to_string()))?;
        Ok(CreatureRecordFamilyBatch {
            family_id: recipe.batch_intent.family_id,
            closures: vec![closure],
            primary_mappings: vec![recipe.batch_intent.primary_mapping],
        })
    }

    pub fn required_field_receipts_from_policy(
        projection: &CreatureRecordProjectionManifest,
        policy_id: impl Into<String>,
    ) -> Vec<SourceRigFieldReceipt> {
        let policy_id = policy_id.into();
        projection_field_values(projection)
            .into_iter()
            .map(|(field, value)| SourceRigFieldReceipt {
                field,
                value_blake3: value_hash(&value),
                decision: SourceRigFieldDecision::Policy {
                    policy_id: policy_id.clone(),
                },
            })
            .collect()
    }

    pub fn required_field_receipts_from_source(
        projection: &CreatureRecordProjectionManifest,
        source_game: impl Into<String>,
    ) -> Vec<SourceRigFieldReceipt> {
        let source_game = source_game.into();
        projection_field_values(projection)
            .into_iter()
            .map(|(field, value)| {
                let source_field = field.clone();
                SourceRigFieldReceipt {
                    field,
                    value_blake3: value_hash(&value),
                    decision: SourceRigFieldDecision::Source {
                        source_game: source_game.clone(),
                        source_field,
                    },
                }
            })
            .collect()
    }

    pub fn required_field_receipts_from_policy_for_intent(
        rig: &CreatureManifest,
        graph: &CapabilityGraphManifest,
        projection: &CreatureRecordProjectionManifest,
        key_plan: &CreatureRecordKeyPlan,
        batch_intent: &SourceRigRecordBatchIntent,
        bridge_receipt: Option<&SourceRigRecipeBridgeReceipt>,
        policy_id: impl Into<String>,
    ) -> Vec<SourceRigFieldReceipt> {
        let policy_id = policy_id.into();
        executable_intent_field_values(
            rig,
            graph,
            projection,
            key_plan,
            &[],
            batch_intent,
            bridge_receipt,
        )
        .into_iter()
        .map(|(field, value)| SourceRigFieldReceipt {
            field,
            value_blake3: value_hash(&value),
            decision: SourceRigFieldDecision::Policy {
                policy_id: policy_id.clone(),
            },
        })
        .collect()
    }

    pub fn required_field_receipts_from_policy_for_intent_with_actor_actions(
        rig: &CreatureManifest,
        graph: &CapabilityGraphManifest,
        projection: &CreatureRecordProjectionManifest,
        key_plan: &CreatureRecordKeyPlan,
        actor_action_records: &[CreatureActorActionRecordPlan],
        batch_intent: &SourceRigRecordBatchIntent,
        bridge_receipt: Option<&SourceRigRecipeBridgeReceipt>,
        policy_id: impl Into<String>,
    ) -> Vec<SourceRigFieldReceipt> {
        let policy_id = policy_id.into();
        executable_intent_field_values(
            rig,
            graph,
            projection,
            key_plan,
            actor_action_records,
            batch_intent,
            bridge_receipt,
        )
        .into_iter()
        .map(|(field, value)| SourceRigFieldReceipt {
            field,
            value_blake3: value_hash(&value),
            decision: SourceRigFieldDecision::Policy {
                policy_id: policy_id.clone(),
            },
        })
        .collect()
    }

    pub fn required_field_receipts_from_source_for_intent(
        rig: &CreatureManifest,
        graph: &CapabilityGraphManifest,
        projection: &CreatureRecordProjectionManifest,
        key_plan: &CreatureRecordKeyPlan,
        batch_intent: &SourceRigRecordBatchIntent,
        bridge_receipt: Option<&SourceRigRecipeBridgeReceipt>,
        source_game: impl Into<String>,
    ) -> Vec<SourceRigFieldReceipt> {
        let source_game = source_game.into();
        executable_intent_field_values(
            rig,
            graph,
            projection,
            key_plan,
            &[],
            batch_intent,
            bridge_receipt,
        )
        .into_iter()
        .map(|(field, value)| {
            let source_field = field.clone();
            SourceRigFieldReceipt {
                field,
                value_blake3: value_hash(&value),
                decision: SourceRigFieldDecision::Source {
                    source_game: source_game.clone(),
                    source_field,
                },
            }
        })
        .collect()
    }

    pub fn required_field_receipts_from_source_for_intent_with_actor_actions(
        rig: &CreatureManifest,
        graph: &CapabilityGraphManifest,
        projection: &CreatureRecordProjectionManifest,
        key_plan: &CreatureRecordKeyPlan,
        actor_action_records: &[CreatureActorActionRecordPlan],
        batch_intent: &SourceRigRecordBatchIntent,
        bridge_receipt: Option<&SourceRigRecipeBridgeReceipt>,
        source_game: impl Into<String>,
    ) -> Vec<SourceRigFieldReceipt> {
        let source_game = source_game.into();
        executable_intent_field_values(
            rig,
            graph,
            projection,
            key_plan,
            actor_action_records,
            batch_intent,
            bridge_receipt,
        )
        .into_iter()
        .map(|(field, value)| {
            let source_field = field.clone();
            SourceRigFieldReceipt {
                field,
                value_blake3: value_hash(&value),
                decision: SourceRigFieldDecision::Source {
                    source_game: source_game.clone(),
                    source_field,
                },
            }
        })
        .collect()
    }

    pub fn validate(&self) -> Result<(), SourceRigRecipeError> {
        if self.version != SOURCE_RIG_RECIPE_VERSION {
            return Err(SourceRigRecipeError::UnsupportedVersion {
                actual: self.version,
                expected: SOURCE_RIG_RECIPE_VERSION,
            });
        }
        validate_root_node(&self.rig, &self.projection)?;
        validate_key_plan(&self.key_plan, &self.projection)?;
        validate_actor_action_records(
            &self.actor_action_records,
            &self.graph,
            &self.rig.paths.root_behavior,
            &self.projection.base.target_plugin,
        )?;
        validate_batch_intent(
            &self.batch_intent,
            &self.projection,
            &self.actor_action_records,
        )?;
        if let Some(receipt) = &self.runtime_model_closure {
            receipt.validate_structure().map_err(|error| {
                invalid(
                    "runtime_model_closure",
                    format!("invalid runtime-model closure receipt: {error}"),
                )
            })?;
        }
        validate_receipts(
            &self.field_receipts,
            &self.rig,
            &self.graph,
            &self.projection,
            &self.key_plan,
            &self.actor_action_records,
            &self.batch_intent,
            self.bridge_receipt.as_ref(),
        )?;
        let interner = StringInterner::new();
        let mut closure = emit_creature_capability_record_projection(
            &self.rig,
            &self.graph,
            &self.projection,
            &interner,
        )
        .map_err(|error| SourceRigRecipeError::Projection(error.to_string()))?;
        append_creature_actor_action_records(&mut closure, &self.actor_action_records, &interner)
            .map_err(|error| SourceRigRecipeError::Projection(error.to_string()))?;
        validate_emitted_closure(
            &closure,
            &self.projection,
            &self.actor_action_records,
            &interner,
        )
    }

    fn normalize(&mut self) {
        self.projection.materialize_legacy_body_nif_part();
        if self.key_plan.armor_addons.is_empty() && self.projection.body_nif_parts.len() == 1 {
            self.key_plan.armor_addons = vec![
                self.projection.body_nif_parts[0]
                    .armor_addon_form_key
                    .clone(),
            ];
        }
        self.rig.clips.sort_by_key(|clip| {
            (
                clip.name.to_ascii_lowercase(),
                clip.path.to_ascii_lowercase(),
            )
        });
        sort_graph_declarations(&mut self.rig.root);
        sort_graph_declarations(&mut self.rig.core);
        self.graph.roles.sort_by_key(|role| {
            (
                role.role,
                role.state_name.to_ascii_lowercase(),
                role.clip_name.to_ascii_lowercase(),
            )
        });
        self.graph
            .explicit_events
            .sort_by_key(|event| event.name.to_ascii_lowercase());
        self.graph
            .overlays
            .sort_by_key(|overlay| overlay.name.to_ascii_lowercase());
        self.projection.race_data.flags.sort();
        self.projection.race_data.flags.dedup();
        self.projection.race_data.flags_2.sort();
        self.projection.race_data.flags_2.dedup();
        self.projection.variants.sort_by_key(|variant| {
            (
                !variant.primary,
                variant.source_identity.stable_key(),
                variant.editor_id.to_ascii_lowercase(),
            )
        });
        self.projection.attacks.sort_by_key(|attack| {
            (
                !attack.primary,
                attack.id.to_ascii_lowercase(),
                attack.event.to_ascii_lowercase(),
            )
        });
        sort_npc_key_plan(&mut self.key_plan.npc_variants);
        sort_melee_key_plan(&mut self.key_plan.melee_attacks);
        self.actor_action_records.sort_by_key(|plan| {
            (
                plan.requirement.clone(),
                plan.editor_id.to_ascii_lowercase(),
                plan.form_key.clone(),
            )
        });
        sort_dependencies(&mut self.batch_intent.required_target_records);
        if let Some(bridge) = &mut self.bridge_receipt {
            bridge.artifact_requests.sort_by_key(|request| {
                (
                    request.runtime_path.to_ascii_lowercase(),
                    request.role.clone(),
                )
            });
            bridge.artifact_receipts.sort_by_key(|artifact| {
                (
                    artifact.runtime_path.to_ascii_lowercase(),
                    artifact.role.clone(),
                )
            });
        }
        for receipt in &mut self.field_receipts {
            if let SourceRigFieldDecision::Derived {
                source_games,
                source_fields,
                ..
            } = &mut receipt.decision
            {
                source_games.sort_by_key(|game| game.to_ascii_lowercase());
                source_games.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
                source_fields.sort_by_key(|field| field.to_ascii_lowercase());
                source_fields.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
            }
        }
        self.field_receipts
            .sort_by_key(|receipt| receipt.field.to_ascii_lowercase());
    }
}

fn sort_graph_declarations(declarations: &mut super::GraphDeclarations) {
    declarations
        .events
        .sort_by_key(|event| event.name.to_ascii_lowercase());
    declarations
        .variables
        .sort_by_key(|variable| variable.name.to_ascii_lowercase());
    declarations
        .character_properties
        .sort_by_key(|property| property.name.to_ascii_lowercase());
}

fn sort_npc_key_plan(plans: &mut [CreatureNpcRecordKeyPlan]) {
    plans.sort_by_key(|plan| (!plan.primary, plan.source_identity.stable_key()));
}

fn sort_melee_key_plan(plans: &mut [CreatureMeleeRecordKeyPlan]) {
    plans.sort_by_key(|plan| (!plan.primary, plan.attack_id.to_ascii_lowercase()));
}

fn sort_dependencies(dependencies: &mut Vec<CreatureTargetRecordReference>) {
    dependencies.sort_by_key(|dependency| {
        (
            dependency.form_key.plugin.to_ascii_lowercase(),
            dependency.form_key.local,
            dependency.signature.clone(),
        )
    });
    dependencies.dedup();
}

fn validate_root_node(
    rig: &CreatureManifest,
    projection: &CreatureRecordProjectionManifest,
) -> Result<(), SourceRigRecipeError> {
    let super::CreatureBodyPartProjection::RootOnly32 {
        node, vats_target, ..
    } = &projection.body_parts;
    if node.trim().is_empty()
        || node.contains('/')
        || node.contains('\\')
        || node.to_ascii_lowercase().ends_with(".nif")
        || node.to_ascii_lowercase().ends_with(".hkx")
    {
        return Err(invalid(
            "root_bone_name",
            format!("BPTD root node {node:?} must be a bone name, not a path"),
        ));
    }
    let roots = rig
        .animation_skeleton
        .bones
        .iter()
        .filter(|bone| bone.parent_index.is_none())
        .collect::<Vec<_>>();
    if !roots.iter().any(|root| root.name == *node) {
        return Err(invalid(
            "root_bone_name",
            format!("BPTD node {node:?} is not an animation skeleton root"),
        ));
    }
    if vats_target != node {
        return Err(invalid(
            "root_bone_name",
            "BPTD node and VATS target must exactly match",
        ));
    }
    Ok(())
}

fn validate_key_plan(
    key_plan: &CreatureRecordKeyPlan,
    projection: &CreatureRecordProjectionManifest,
) -> Result<(), SourceRigRecipeError> {
    if key_plan.planned_source_identity != projection.source_primary_identity {
        return Err(invalid(
            "key_plan_source",
            "key plan source does not match projection source",
        ));
    }
    if key_plan.base != projection.base.form_keys {
        return Err(invalid(
            "key_plan_base",
            "key plan base keys do not match projection keys",
        ));
    }
    let expected_armor_addons = projection
        .effective_body_nif_parts()
        .into_iter()
        .map(|part| part.armor_addon_form_key)
        .collect::<Vec<_>>();
    if key_plan.armor_addons != expected_armor_addons {
        return Err(invalid(
            "key_plan_armor_addons",
            "key plan ARMA identities do not exactly match the ordered body NIF parts",
        ));
    }
    let expected_npcs = projection
        .variants
        .iter()
        .map(|variant| {
            (
                variant.source_identity.stable_key(),
                variant.form_key.clone(),
                variant.primary,
            )
        })
        .collect::<BTreeSet<_>>();
    let actual_npcs = key_plan
        .npc_variants
        .iter()
        .map(|variant| {
            (
                variant.source_identity.stable_key(),
                variant.form_key.clone(),
                variant.primary,
            )
        })
        .collect::<BTreeSet<_>>();
    if expected_npcs != actual_npcs || actual_npcs.len() != key_plan.npc_variants.len() {
        return Err(invalid(
            "key_plan_npcs",
            "key plan NPC variants do not exactly match projection variants",
        ));
    }
    let expected_melee = projection
        .attacks
        .iter()
        .filter_map(|attack| match &attack.projection {
            CreatureAttackRecordProjection::MeleeUnarmed {
                weapon_form_key, ..
            } => Some((
                attack.id.to_ascii_lowercase(),
                weapon_form_key.clone(),
                attack.primary,
            )),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let actual_melee = key_plan
        .melee_attacks
        .iter()
        .map(|attack| {
            (
                attack.attack_id.to_ascii_lowercase(),
                attack.form_key.clone(),
                attack.primary,
            )
        })
        .collect::<BTreeSet<_>>();
    if expected_melee != actual_melee || actual_melee.len() != key_plan.melee_attacks.len() {
        return Err(invalid(
            "key_plan_melee",
            "key plan melee attacks do not exactly match projection melee attacks",
        ));
    }
    Ok(())
}

fn validate_batch_intent(
    intent: &SourceRigRecordBatchIntent,
    projection: &CreatureRecordProjectionManifest,
    actor_action_records: &[CreatureActorActionRecordPlan],
) -> Result<(), SourceRigRecipeError> {
    if intent.family_id.trim().is_empty() {
        return Err(invalid("batch_family", "batch family id is empty"));
    }
    if intent.primary_mapping.source != projection.source_primary_identity {
        return Err(invalid(
            "batch_primary_mapping",
            "batch source mapping does not match projection source",
        ));
    }
    let primary = projection
        .variants
        .iter()
        .find(|variant| variant.primary)
        .ok_or_else(|| invalid("batch_primary_mapping", "projection has no primary NPC"))?;
    if intent.primary_mapping.target != primary.form_key {
        return Err(invalid(
            "batch_primary_mapping",
            "batch target mapping does not match the primary projected NPC",
        ));
    }
    let mut expected = projection_dependencies(projection);
    expected.extend(actor_action_dependencies(actor_action_records));
    let mut actual = intent.required_target_records.clone();
    sort_dependencies(&mut expected);
    sort_dependencies(&mut actual);
    if expected != actual {
        return Err(invalid(
            "batch_dependencies",
            "batch target dependencies do not exactly match record projections and Actor Action plans",
        ));
    }
    Ok(())
}

fn validate_actor_action_records(
    records: &[CreatureActorActionRecordPlan],
    graph: &CapabilityGraphManifest,
    root_behavior_path: &str,
    target_plugin: &str,
) -> Result<(), SourceRigRecipeError> {
    if records.is_empty() {
        return Ok(());
    }
    let report = actor_action_admission_report(graph, root_behavior_path, records);
    if report.has_fatal() {
        return Err(invalid(
            "actor_action_admission",
            report
                .issues
                .into_iter()
                .map(|issue| issue.detail)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    let expected = required_actor_action_records(graph, root_behavior_path)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let actual = records
        .iter()
        .map(|record| record.requirement.clone())
        .collect::<BTreeSet<_>>();
    if actual != expected || actual.len() != records.len() {
        return Err(invalid(
            "actor_action_admission",
            "generated IDLE plans do not exactly match the graph Actor Action requirements",
        ));
    }
    let mut form_keys = BTreeSet::new();
    let mut editor_ids = BTreeSet::new();
    for record in records {
        if record.form_key.local == 0
            || record.form_key.local > 0x00ff_ffff
            || !record.form_key.plugin.eq_ignore_ascii_case(target_plugin)
            || record.editor_id.trim().is_empty()
            || !record
                .editor_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return Err(invalid(
                "actor_action_record_identity",
                format!("invalid generated Actor Action IDLE plan {record:?}"),
            ));
        }
        if !form_keys.insert((
            record.form_key.plugin.to_ascii_lowercase(),
            record.form_key.local,
        )) || !editor_ids.insert(record.editor_id.to_ascii_lowercase())
        {
            return Err(invalid(
                "actor_action_record_identity",
                "generated Actor Action IDLE plans contain duplicate record identities",
            ));
        }
    }
    Ok(())
}

fn actor_action_dependencies(
    actor_action_records: &[CreatureActorActionRecordPlan],
) -> Vec<CreatureTargetRecordReference> {
    let mut dependencies = actor_action_records
        .iter()
        .map(|record| CreatureTargetRecordReference {
            signature: "AACT".to_string(),
            form_key: record.requirement.parent_form_key(),
        })
        .collect::<Vec<_>>();
    sort_dependencies(&mut dependencies);
    dependencies
}

pub(crate) fn projection_dependencies(
    projection: &CreatureRecordProjectionManifest,
) -> Vec<CreatureTargetRecordReference> {
    let mut dependencies = Vec::new();
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
    for attack in &projection.attacks {
        match &attack.projection {
            CreatureAttackRecordProjection::MeleeUnarmed { .. } => {}
            CreatureAttackRecordProjection::MeleeEquipment { equipment } => {
                dependencies.extend(equipment.iter().cloned());
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
            } => dependencies.extend([attack_spell.clone(), projectile.clone(), equipment.clone()]),
            CreatureAttackRecordProjection::RangedEquipment {
                equipment,
                projectile,
                ammunition,
            } => {
                dependencies.extend(equipment.iter().cloned());
                dependencies.extend(projectile.iter().cloned());
                dependencies.extend(ammunition.iter().cloned());
            }
            CreatureAttackRecordProjection::SpellAbility { spell } => {
                dependencies.push(spell.clone());
            }
            CreatureAttackRecordProjection::ContinuousRobot {
                attack_spell,
                equipment,
            } => dependencies.extend([attack_spell.clone(), equipment.clone()]),
        }
    }
    sort_dependencies(&mut dependencies);
    dependencies
}

fn validate_receipts(
    receipts: &[SourceRigFieldReceipt],
    rig: &CreatureManifest,
    graph: &CapabilityGraphManifest,
    projection: &CreatureRecordProjectionManifest,
    key_plan: &CreatureRecordKeyPlan,
    actor_action_records: &[CreatureActorActionRecordPlan],
    batch_intent: &SourceRigRecordBatchIntent,
    bridge_receipt: Option<&SourceRigRecipeBridgeReceipt>,
) -> Result<(), SourceRigRecipeError> {
    let required = executable_intent_field_values(
        rig,
        graph,
        projection,
        key_plan,
        actor_action_records,
        batch_intent,
        bridge_receipt,
    );
    let mut actual = BTreeMap::new();
    for receipt in receipts {
        validate_decision(&receipt.decision)?;
        if actual.insert(receipt.field.clone(), receipt).is_some() {
            return Err(invalid(
                "duplicate_field_receipt",
                format!("duplicate receipt for {:?}", receipt.field),
            ));
        }
    }
    for (field, value) in &required {
        let receipt = actual.remove(field).ok_or_else(|| {
            invalid(
                "missing_field_receipt",
                format!("missing receipt for {field}"),
            )
        })?;
        let expected = value_hash(value);
        if !receipt.value_blake3.eq_ignore_ascii_case(&expected) {
            return Err(invalid(
                "field_receipt_value",
                format!("receipt for {field} does not match the persisted value"),
            ));
        }
    }
    if let Some(field) = actual.keys().next() {
        return Err(invalid(
            "unexpected_field_receipt",
            format!("receipt for non-profile field {field}"),
        ));
    }
    Ok(())
}

fn validate_decision(decision: &SourceRigFieldDecision) -> Result<(), SourceRigRecipeError> {
    let validate_token = |label: &'static str, value: &str| {
        if value.trim().is_empty()
            || value.contains('/')
            || value.contains('\\')
            || value.contains('@')
        {
            Err(invalid(
                "receipt_provenance",
                format!("{label} {value:?} must be a source-neutral label, not a path or FormKey"),
            ))
        } else {
            Ok(())
        }
    };
    match decision {
        SourceRigFieldDecision::Source {
            source_game,
            source_field,
        } => {
            validate_token("source_game", source_game)?;
            validate_token("source_field", source_field)
        }
        SourceRigFieldDecision::Policy { policy_id } => validate_token("policy_id", policy_id),
        SourceRigFieldDecision::Derived {
            source_games,
            source_fields,
            policy_id,
        } => {
            if source_games.is_empty() || source_fields.is_empty() {
                return Err(invalid(
                    "receipt_provenance",
                    "derived receipt must name source games and source fields",
                ));
            }
            for game in source_games {
                validate_token("source_game", game)?;
            }
            for field in source_fields {
                validate_token("source_field", field)?;
            }
            validate_token("policy_id", policy_id)
        }
    }
}

fn validate_emitted_closure(
    closure: &CreatureRecordProjectionClosure,
    projection: &CreatureRecordProjectionManifest,
    actor_action_records: &[CreatureActorActionRecordPlan],
    interner: &StringInterner,
) -> Result<(), SourceRigRecipeError> {
    let has_melee = projection.attacks.iter().any(|attack| {
        matches!(
            attack.projection,
            CreatureAttackRecordProjection::MeleeUnarmed { .. }
        )
    });
    let generated_weapons = closure
        .closure
        .records
        .iter()
        .filter(|record| record.sig.as_str() == "WEAP")
        .count();
    if (!has_melee && generated_weapons != 0) || (has_melee && generated_weapons == 0) {
        return Err(invalid(
            "capability_record_closure",
            format!("melee={has_melee} emitted_weap={generated_weapons}"),
        ));
    }
    let mut expected = projection_dependencies(projection);
    expected.extend(actor_action_dependencies(actor_action_records));
    let mut actual = closure.required_target_records.clone();
    sort_dependencies(&mut expected);
    sort_dependencies(&mut actual);
    if expected != actual {
        return Err(invalid(
            "capability_record_dependencies",
            "emitted target dependencies differ from recipe intent",
        ));
    }
    let emitted_actor_actions = closure
        .closure
        .records
        .iter()
        .filter(|record| record.sig.as_str() == "IDLE")
        .count();
    if emitted_actor_actions != actor_action_records.len() {
        return Err(invalid(
            "actor_action_record_closure",
            format!(
                "planned {} generated Actor Action IDLE records but emitted {emitted_actor_actions}",
                actor_action_records.len()
            ),
        ));
    }
    for record in &closure.closure.records {
        let plugin = interner.resolve(record.form_key.plugin).unwrap_or_default();
        if !plugin.eq_ignore_ascii_case(&projection.base.target_plugin) {
            return Err(invalid(
                "target_record_key",
                format!("emitted record belongs to unexpected plugin {plugin:?}"),
            ));
        }
    }
    Ok(())
}

fn executable_intent_field_values(
    rig: &CreatureManifest,
    graph: &CapabilityGraphManifest,
    projection: &CreatureRecordProjectionManifest,
    key_plan: &CreatureRecordKeyPlan,
    actor_action_records: &[CreatureActorActionRecordPlan],
    batch_intent: &SourceRigRecordBatchIntent,
    bridge_receipt: Option<&SourceRigRecipeBridgeReceipt>,
) -> BTreeMap<String, Value> {
    let mut canonical = SourceRigExecutableRecipe {
        version: SOURCE_RIG_RECIPE_VERSION,
        rig: rig.clone(),
        graph: graph.clone(),
        projection: projection.clone(),
        key_plan: key_plan.clone(),
        actor_action_records: actor_action_records.to_vec(),
        batch_intent: batch_intent.clone(),
        bridge_receipt: bridge_receipt.cloned(),
        runtime_model_closure: None,
        field_receipts: Vec::new(),
    };
    canonical.normalize();
    let mut values = BTreeMap::new();
    for (prefix, value) in [
        (
            "rig",
            serde_json::to_value(&canonical.rig).expect("validated rig must be JSON serializable"),
        ),
        (
            "graph",
            serde_json::to_value(&canonical.graph)
                .expect("validated graph must be JSON serializable"),
        ),
        (
            "projection",
            serde_json::to_value(&canonical.projection)
                .expect("validated projection must be JSON serializable"),
        ),
        (
            "key_plan",
            serde_json::to_value(&canonical.key_plan)
                .expect("validated key plan must be JSON serializable"),
        ),
        (
            "batch_intent",
            serde_json::to_value(&canonical.batch_intent)
                .expect("validated batch intent must be JSON serializable"),
        ),
    ] {
        flatten_json_leaves(prefix, &value, &mut values);
    }
    if !canonical.actor_action_records.is_empty() {
        let value = serde_json::to_value(&canonical.actor_action_records)
            .expect("validated Actor Action records must be JSON serializable");
        flatten_json_leaves("actor_action_records", &value, &mut values);
    }
    if let Some(bridge_receipt) = &canonical.bridge_receipt {
        let value = serde_json::to_value(bridge_receipt)
            .expect("validated bridge receipt must be JSON serializable");
        flatten_json_leaves("bridge_receipt", &value, &mut values);
    }
    values.insert("emitted.bptd.geometry_segment_index".to_string(), json!(32));
    values
}

fn flatten_json_leaves(prefix: &str, value: &Value, output: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(entries) if !entries.is_empty() => {
            for (key, value) in entries {
                flatten_json_leaves(&format!("{prefix}.{key}"), value, output);
            }
        }
        Value::Array(entries) if !entries.is_empty() => {
            for (index, value) in entries.iter().enumerate() {
                flatten_json_leaves(&format!("{prefix}[{index}]"), value, output);
            }
        }
        _ => {
            output.insert(prefix.to_string(), value.clone());
        }
    }
}

fn projection_field_values(
    projection: &CreatureRecordProjectionManifest,
) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    let root = match &projection.body_parts {
        super::CreatureBodyPartProjection::RootOnly32 {
            name,
            node,
            vats_target,
        } => (name, node, vats_target),
    };
    values.insert("profile.root_body_part.name".to_string(), json!(root.0));
    values.insert("profile.root_body_part.node".to_string(), json!(root.1));
    values.insert(
        "profile.root_body_part.vats_target".to_string(),
        json!(root.2),
    );
    values.insert(
        "profile.root_body_part.geometry_segment_index".to_string(),
        json!(32),
    );
    for variant in &projection.variants {
        let prefix = format!("profile.npc[{}]", variant.source_identity.stable_key());
        values.insert(format!("{prefix}.level"), json!(variant.level));
        values.insert(format!("{prefix}.health"), json!(variant.health));
        values.insert(
            format!("{prefix}.action_points"),
            json!(variant.action_points),
        );
    }
    for attack in &projection.attacks {
        let prefix = format!("profile.attack[{}]", attack.id.to_ascii_lowercase());
        values.insert(
            format!("{prefix}.damage_multiplier"),
            json!(attack.damage_multiplier),
        );
        values.insert(format!("{prefix}.chance"), json!(attack.chance));
        values.insert(format!("{prefix}.strike_angle"), json!(attack.strike_angle));
        values.insert(
            format!("{prefix}.attack_flags"),
            json!(attack.target_data.attack_flags),
        );
        values.insert(
            format!("{prefix}.attack_angle"),
            json!(attack.target_data.attack_angle),
        );
        values.insert(
            format!("{prefix}.stagger"),
            json!(attack.target_data.stagger),
        );
        values.insert(
            format!("{prefix}.knockdown"),
            json!(attack.target_data.knockdown),
        );
        values.insert(
            format!("{prefix}.recovery_time"),
            json!(attack.target_data.recovery_time),
        );
        values.insert(
            format!("{prefix}.action_points_multiplier"),
            json!(attack.target_data.action_points_multiplier),
        );
        values.insert(
            format!("{prefix}.stagger_offset"),
            json!(attack.target_data.stagger_offset),
        );
        values.insert(
            format!("{prefix}.action_point_cost"),
            json!(attack.action_point_cost),
        );
        if let Some(source_atkd) = &attack.target_data.source_atkd {
            let value = serde_json::to_value(source_atkd)
                .expect("validated attack source receipt must be JSON serializable");
            flatten_json_leaves(&format!("{prefix}.source_atkd"), &value, &mut values);
        }
        if let CreatureAttackRecordProjection::MeleeUnarmed {
            damage,
            reach,
            attack_seconds,
            ..
        } = &attack.projection
        {
            values.insert(format!("{prefix}.unarmed_damage"), json!(damage));
            values.insert(format!("{prefix}.unarmed_reach"), json!(reach));
            values.insert(
                format!("{prefix}.unarmed_attack_seconds"),
                json!(attack_seconds),
            );
        }
    }
    let race = &projection.race_data;
    let mut flags = race.flags.clone();
    flags.sort();
    flags.dedup();
    let mut flags_2 = race.flags_2.clone();
    flags_2.sort();
    flags_2.dedup();
    values.extend([
        ("race_data.male_height".to_string(), json!(race.male_height)),
        (
            "race_data.female_height".to_string(),
            json!(race.female_height),
        ),
        (
            "race_data.male_default_weight".to_string(),
            json!(race.male_default_weight),
        ),
        (
            "race_data.female_default_weight".to_string(),
            json!(race.female_default_weight),
        ),
        ("race_data.flags".to_string(), json!(flags)),
        (
            "race_data.acceleration_rate".to_string(),
            json!(race.acceleration_rate),
        ),
        (
            "race_data.deceleration_rate".to_string(),
            json!(race.deceleration_rate),
        ),
        ("race_data.size".to_string(), json!(race.size)),
        (
            "race_data.injured_health_percent".to_string(),
            json!(race.injured_health_percent),
        ),
        (
            "race_data.body_biped_object".to_string(),
            json!(race.body_biped_object),
        ),
        (
            "race_data.aim_angle_tolerance".to_string(),
            json!(race.aim_angle_tolerance),
        ),
        (
            "race_data.flight_radius".to_string(),
            json!(race.flight_radius),
        ),
        (
            "race_data.angular_acceleration_rate".to_string(),
            json!(race.angular_acceleration_rate),
        ),
        (
            "race_data.angular_tolerance".to_string(),
            json!(race.angular_tolerance),
        ),
        ("race_data.flags_2".to_string(), json!(flags_2)),
        ("race_data.xp_value".to_string(), json!(race.xp_value)),
        (
            "race_data.orientation_limit_pitch".to_string(),
            json!(race.orientation_limit_pitch),
        ),
        (
            "race_data.orientation_limit_roll".to_string(),
            json!(race.orientation_limit_roll),
        ),
    ]);
    values
}

fn value_hash(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).expect("serde_json::Value serialization cannot fail");
    blake3::hash(&bytes).to_hex().to_string()
}

fn invalid(code: &'static str, message: impl Into<String>) -> SourceRigRecipeError {
    SourceRigRecipeError::Invalid {
        code,
        message: message.into(),
    }
}
