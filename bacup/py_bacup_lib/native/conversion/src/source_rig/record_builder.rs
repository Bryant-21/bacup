//! Typed bridge from the validated creature corpus to atomic record families.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{
    CapabilityGraphManifest, CreatureAttackRecordProjection, CreatureCorpusPlan,
    CreatureCorpusPlanError, CreatureGraphTemplate, CreatureManifest, CreaturePrimaryRecordMapping,
    CreatureReadiness, CreatureRecordError, CreatureRecordFamilyBatch, CreatureRecordFormKeys,
    CreatureRecordProjectionManifest, MotionAttackKind, PlannedCreature, RaceDataMapping,
    SourceCreatureIdentity, TargetFormKey, emit_creature_capability_record_projection,
};
use crate::formkey_mapper::{FormKeyMapper, MapperState};
use crate::sym::StringInterner;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CreatureRecordProjectionBuildInput {
    pub planned_source_identity: SourceCreatureIdentity,
    pub rig: CreatureManifest,
    pub graph: CapabilityGraphManifest,
    pub projection: CreatureRecordProjectionManifest,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureRecordKeyPlanRequest {
    pub planned_source_identity: SourceCreatureIdentity,
    pub primary_melee_attack_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureNpcRecordKeyPlan {
    pub source_identity: SourceCreatureIdentity,
    pub form_key: TargetFormKey,
    pub primary: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureMeleeRecordKeyPlan {
    pub attack_id: String,
    pub form_key: TargetFormKey,
    pub primary: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct CreatureRecordKeyPlan {
    pub planned_source_identity: SourceCreatureIdentity,
    pub base: CreatureRecordFormKeys,
    #[serde(default)]
    pub armor_addons: Vec<TargetFormKey>,
    pub npc_variants: Vec<CreatureNpcRecordKeyPlan>,
    pub melee_attacks: Vec<CreatureMeleeRecordKeyPlan>,
}

#[derive(Debug, Error)]
pub enum CreatureRecordBatchBuildError {
    #[error("invalid creature corpus plan: {0}")]
    InvalidPlan(#[from] CreatureCorpusPlanError),
    #[error("duplicate record projection input for planned source {source_key}")]
    DuplicateInput { source_key: String },
    #[error("missing record projection input for planned source {source_key}")]
    MissingInput { source_key: String },
    #[error("record projection input does not belong to planned corpus source {source_key}")]
    UnexpectedInput { source_key: String },
    #[error("record projection for {source_key} does not match corpus field {field}: {message}")]
    ProjectionMismatch {
        source_key: String,
        field: &'static str,
        message: String,
    },
    #[error("record projection failed for {source_key}: {source}")]
    Projection {
        source_key: String,
        #[source]
        source: CreatureRecordError,
    },
    #[error(
        "record projection collision {local:06X}@{plugin} between planned sources {first_source} and {second_source}"
    )]
    RecordCollision {
        first_source: String,
        second_source: String,
        local: u32,
        plugin: String,
    },
    #[error("record key plan request for {source_key} is missing or duplicated")]
    InvalidKeyPlanRequest { source_key: String },
    #[error("record key plan for {source_key} has invalid primary melee attack {attack_id:?}")]
    InvalidPrimaryMeleeAttack {
        source_key: String,
        attack_id: Option<String>,
    },
}

pub fn plan_creature_record_form_keys(
    plan: &CreatureCorpusPlan,
    mapper_state: &MapperState,
    requests: Vec<CreatureRecordKeyPlanRequest>,
    interner: &StringInterner,
) -> Result<Vec<CreatureRecordKeyPlan>, CreatureRecordBatchBuildError> {
    plan.validate()?;
    let mut requests = requests
        .into_iter()
        .map(|request| {
            (
                request
                    .planned_source_identity
                    .stable_key()
                    .to_ascii_lowercase(),
                request,
            )
        })
        .collect::<Vec<_>>();
    requests.sort_by_key(|(key, _)| key.clone());
    if requests.windows(2).any(|window| window[0].0 == window[1].0) {
        return Err(CreatureRecordBatchBuildError::InvalidKeyPlanRequest {
            source_key: requests
                .windows(2)
                .find(|window| window[0].0 == window[1].0)
                .map(|window| window[0].0.clone())
                .unwrap_or_default(),
        });
    }
    let mut requests = requests.into_iter().collect::<BTreeMap<_, _>>();
    let mut next_mapper_state = mapper_state.clone();
    let target_plugin = next_mapper_state.options.output_plugin_name.clone();
    let mut allocator = FormKeyMapper::from_state(&mut next_mapper_state, interner);
    let mut result = Vec::with_capacity(plan.planned.len());
    for planned in &plan.planned {
        let source_key = planned.source_key.clone();
        let request = requests
            .remove(&source_key.to_ascii_lowercase())
            .ok_or_else(|| CreatureRecordBatchBuildError::InvalidKeyPlanRequest {
                source_key: source_key.clone(),
            })?;
        if request.planned_source_identity != planned.source_identity {
            return Err(CreatureRecordBatchBuildError::InvalidKeyPlanRequest { source_key });
        }
        let allocate = |allocator: &mut FormKeyMapper<'_>| {
            TargetFormKey::new(allocator.allocate_generated().local, target_plugin.clone())
        };
        let race = allocate(&mut allocator);
        let mut variants = planned.record_variants.iter().collect::<Vec<_>>();
        variants.sort_by_key(|variant| {
            (
                !variant.primary,
                variant.source_identity.stable_key().to_ascii_lowercase(),
            )
        });
        let npc_variants = variants
            .into_iter()
            .map(|variant| CreatureNpcRecordKeyPlan {
                source_identity: variant.source_identity.clone(),
                form_key: allocate(&mut allocator),
                primary: variant.primary,
            })
            .collect::<Vec<_>>();
        let npc = npc_variants
            .iter()
            .find(|variant| variant.primary)
            .expect("validated corpus has one primary NPC")
            .form_key
            .clone();
        let skin = allocate(&mut allocator);
        let armor_addon = allocate(&mut allocator);
        let body_part_data = allocate(&mut allocator);

        let mut melee_ids = planned
            .motion_set
            .attacks
            .iter()
            .filter(|attack| {
                planned_attack_kind(planned, &attack.id) == Some(MotionAttackKind::MeleeUnarmed)
            })
            .map(|attack| attack.id.clone())
            .collect::<Vec<_>>();
        melee_ids.sort_by_key(|id| id.to_ascii_lowercase());
        let primary_melee = request.primary_melee_attack_id.as_ref();
        if (melee_ids.is_empty() && primary_melee.is_some())
            || (!melee_ids.is_empty()
                && !primary_melee.is_some_and(|primary| {
                    melee_ids.iter().any(|id| id.eq_ignore_ascii_case(primary))
                }))
        {
            return Err(CreatureRecordBatchBuildError::InvalidPrimaryMeleeAttack {
                source_key,
                attack_id: request.primary_melee_attack_id,
            });
        }
        melee_ids.sort_by_key(|id| {
            (
                !primary_melee.is_some_and(|primary| id.eq_ignore_ascii_case(primary)),
                id.to_ascii_lowercase(),
            )
        });
        let melee_attacks = melee_ids
            .into_iter()
            .map(|attack_id| CreatureMeleeRecordKeyPlan {
                primary: primary_melee
                    .is_some_and(|primary| attack_id.eq_ignore_ascii_case(primary)),
                attack_id,
                form_key: allocate(&mut allocator),
            })
            .collect::<Vec<_>>();
        let unarmed_weapon = melee_attacks
            .first()
            .map(|attack| attack.form_key.clone())
            .unwrap_or_else(|| TargetFormKey::new(0, target_plugin.clone()));
        result.push(CreatureRecordKeyPlan {
            planned_source_identity: planned.source_identity.clone(),
            base: CreatureRecordFormKeys {
                race,
                npc,
                skin,
                armor_addon: armor_addon.clone(),
                body_part_data,
                unarmed_weapon,
            },
            armor_addons: vec![armor_addon],
            npc_variants,
            melee_attacks,
        });
    }
    if let Some((_, request)) = requests.into_iter().next() {
        return Err(CreatureRecordBatchBuildError::UnexpectedInput {
            source_key: request.planned_source_identity.stable_key(),
        });
    }
    Ok(result)
}

pub fn build_creature_record_family_batches(
    plan: &CreatureCorpusPlan,
    inputs: Vec<CreatureRecordProjectionBuildInput>,
    interner: &StringInterner,
) -> Result<Vec<CreatureRecordFamilyBatch>, CreatureRecordBatchBuildError> {
    plan.validate()?;
    let planned_keys = plan
        .planned
        .iter()
        .map(|planned| planned.source_key.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut inputs_by_source = BTreeMap::new();
    for input in inputs {
        let source_key = input.planned_source_identity.stable_key();
        let normalized = source_key.to_ascii_lowercase();
        if !planned_keys.contains(&normalized) {
            return Err(CreatureRecordBatchBuildError::UnexpectedInput { source_key });
        }
        if inputs_by_source.insert(normalized, input).is_some() {
            return Err(CreatureRecordBatchBuildError::DuplicateInput { source_key });
        }
    }

    let mut families = BTreeMap::<String, CreatureRecordFamilyBatch>::new();
    let mut record_owners = BTreeMap::<(String, u32), String>::new();
    for planned in &plan.planned {
        let source_key = planned.source_key.clone();
        let input = inputs_by_source
            .remove(&source_key.to_ascii_lowercase())
            .ok_or_else(|| CreatureRecordBatchBuildError::MissingInput {
                source_key: source_key.clone(),
            })?;
        validate_build_input(planned, &input)?;
        let emitted = emit_creature_capability_record_projection(
            &input.rig,
            &input.graph,
            &input.projection,
            interner,
        )
        .map_err(|source| CreatureRecordBatchBuildError::Projection {
            source_key: source_key.clone(),
            source,
        })?;
        let primary = emitted
            .projected_identities
            .iter()
            .find(|identity| identity.primary)
            .expect("record projection validation guarantees one primary");
        for record in &emitted.closure.records {
            let plugin = interner
                .resolve(record.form_key.plugin)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if let Some(first_source) =
                record_owners.insert((plugin.clone(), record.form_key.local), source_key.clone())
            {
                return Err(CreatureRecordBatchBuildError::RecordCollision {
                    first_source,
                    second_source: source_key,
                    local: record.form_key.local,
                    plugin,
                });
            }
        }
        let family_id = planned.rig_family.id.clone();
        let family =
            families
                .entry(family_id.clone())
                .or_insert_with(|| CreatureRecordFamilyBatch {
                    family_id,
                    closures: Vec::new(),
                    primary_mappings: Vec::new(),
                });
        family.primary_mappings.push(CreaturePrimaryRecordMapping {
            source: emitted.source_primary_identity.clone(),
            target: primary.target_form_key.clone(),
        });
        family.closures.push(emitted);
    }
    debug_assert!(inputs_by_source.is_empty());

    let mut families = families.into_values().collect::<Vec<_>>();
    for family in &mut families {
        family.closures.sort_by_key(|closure| {
            closure
                .source_primary_identity
                .stable_key()
                .to_ascii_lowercase()
        });
        family.primary_mappings.sort_by_key(|mapping| {
            (
                mapping.source.stable_key().to_ascii_lowercase(),
                mapping.target.local,
            )
        });
    }
    Ok(families)
}

fn validate_build_input(
    planned: &PlannedCreature,
    input: &CreatureRecordProjectionBuildInput,
) -> Result<(), CreatureRecordBatchBuildError> {
    let source_key = planned.source_key.clone();
    let mismatch = |field, message| CreatureRecordBatchBuildError::ProjectionMismatch {
        source_key: source_key.clone(),
        field,
        message,
    };
    if planned.readiness != CreatureReadiness::Ready || !planned.capabilities.missing.is_empty() {
        return Err(mismatch(
            "readiness",
            "planned creature is not ready".to_string(),
        ));
    }
    if input.planned_source_identity != planned.source_identity {
        return Err(mismatch(
            "planned_source_identity",
            format!(
                "got {}, expected {}",
                input.planned_source_identity.stable_key(),
                planned.source_identity.stable_key()
            ),
        ));
    }
    if input.projection.source_primary_identity != planned.primary_record_identity {
        return Err(mismatch(
            "source_primary_identity",
            format!(
                "got {}, expected {}",
                input.projection.source_primary_identity.stable_key(),
                planned.primary_record_identity.stable_key()
            ),
        ));
    }
    if input.graph.template != planned.motion_set.graph_template {
        return Err(mismatch(
            "graph_template",
            format!(
                "got {:?}, expected {:?}",
                input.graph.template, planned.motion_set.graph_template
            ),
        ));
    }
    match &planned.rig_family.race_data {
        RaceDataMapping::Mapped { target } if target == &input.projection.race_data => {}
        RaceDataMapping::Mapped { .. } => {
            return Err(mismatch(
                "race_data",
                "projection RACE.DATA differs from the mapped family target".to_string(),
            ));
        }
        RaceDataMapping::Missing { .. } => {
            return Err(mismatch(
                "race_data",
                "planned family has no derived RACE.DATA target".to_string(),
            ));
        }
    }
    validate_variants(planned, &input.projection, &mismatch)?;
    validate_attacks(planned, &input.projection, &mismatch)?;
    Ok(())
}

fn validate_variants(
    planned: &PlannedCreature,
    projection: &CreatureRecordProjectionManifest,
    mismatch: &impl Fn(&'static str, String) -> CreatureRecordBatchBuildError,
) -> Result<(), CreatureRecordBatchBuildError> {
    if planned.record_variants.len() != projection.variants.len() {
        return Err(mismatch(
            "record_variants",
            format!(
                "planned {} variants but projection has {}",
                planned.record_variants.len(),
                projection.variants.len()
            ),
        ));
    }
    if planned
        .record_variants
        .iter()
        .any(|variant| variant.body_nif != projection.base.body_nif)
    {
        return Err(mismatch(
            "body_nif",
            "the projection has one ARMA body NIF but planned variants disagree".to_string(),
        ));
    }
    let projected = projection
        .variants
        .iter()
        .map(|variant| (variant.source_identity.stable_key(), variant))
        .collect::<BTreeMap<_, _>>();
    for variant in &planned.record_variants {
        let Some(actual) = projected.get(&variant.source_identity.stable_key()) else {
            return Err(mismatch(
                "record_variants",
                format!(
                    "missing projected NPC for {}",
                    variant.source_identity.stable_key()
                ),
            ));
        };
        if actual.display_name != variant.display_name
            || actual.primary != variant.primary
            || actual.level != variant.level
            || actual.health != variant.health
            || actual.action_points != variant.action_points
        {
            return Err(mismatch(
                "record_variants",
                format!(
                    "projected NPC fields differ for {}",
                    variant.source_identity.stable_key()
                ),
            ));
        }
    }
    Ok(())
}

fn validate_attacks(
    planned: &PlannedCreature,
    projection: &CreatureRecordProjectionManifest,
    mismatch: &impl Fn(&'static str, String) -> CreatureRecordBatchBuildError,
) -> Result<(), CreatureRecordBatchBuildError> {
    let projected = projection
        .attacks
        .iter()
        .map(|attack| (attack.id.to_ascii_lowercase(), attack))
        .collect::<BTreeMap<_, _>>();
    let declared = planned
        .motion_set
        .attacks
        .iter()
        .map(|attack| attack.id.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let used = planned
        .record_variants
        .iter()
        .flat_map(|variant| variant.attack_ids.iter().map(|id| id.to_ascii_lowercase()))
        .collect::<BTreeSet<_>>();
    if declared != used || projected.keys().cloned().collect::<BTreeSet<_>>() != declared {
        return Err(mismatch(
            "attacks",
            "motion, record-variant, and projected attack ids do not close exactly".to_string(),
        ));
    }
    for motion_attack in &planned.motion_set.attacks {
        let id = motion_attack.id.to_ascii_lowercase();
        let projected_attack = projected[&id];
        if projected_attack.event != motion_attack.event {
            return Err(mismatch(
                "attacks",
                format!(
                    "attack {:?} event differs from the motion set",
                    motion_attack.id
                ),
            ));
        }
        let Some(kind) = planned_attack_kind(planned, &id) else {
            return Err(mismatch(
                "attacks",
                format!("attack {:?} has no declared semantics", motion_attack.id),
            ));
        };
        let kind_matches = match (
            planned.motion_set.graph_template,
            kind,
            &projected_attack.projection,
        ) {
            (
                _,
                MotionAttackKind::MeleeUnarmed,
                CreatureAttackRecordProjection::MeleeUnarmed { .. }
                | CreatureAttackRecordProjection::MeleeEquipment { .. },
            ) => true,
            (
                CreatureGraphTemplate::StationaryTurret,
                MotionAttackKind::RangedProjectile | MotionAttackKind::SpellAbility,
                CreatureAttackRecordProjection::Stationary { .. },
            ) => true,
            (
                _,
                MotionAttackKind::RangedProjectile,
                CreatureAttackRecordProjection::RangedProjectile { .. }
                | CreatureAttackRecordProjection::RangedEquipment { .. },
            ) => true,
            (
                CreatureGraphTemplate::RobotContinuousAttack,
                MotionAttackKind::ContinuousRobot,
                CreatureAttackRecordProjection::RangedEquipment { .. },
            ) => true,
            (
                _,
                MotionAttackKind::SpellAbility,
                CreatureAttackRecordProjection::SpellAbility { .. },
            ) => true,
            (
                _,
                MotionAttackKind::ContinuousRobot,
                CreatureAttackRecordProjection::ContinuousRobot { .. },
            ) => true,
            _ => false,
        };
        if !kind_matches {
            return Err(mismatch(
                "attacks",
                format!(
                    "attack {:?} projection does not match {:?} semantics",
                    motion_attack.id, kind
                ),
            ));
        }
    }
    Ok(())
}

fn planned_attack_kind(planned: &PlannedCreature, attack_id: &str) -> Option<MotionAttackKind> {
    planned
        .motion_set
        .attack_kinds
        .get(&attack_id.to_ascii_lowercase())
        .copied()
        .or_else(|| {
            (planned.motion_set.graph_template == CreatureGraphTemplate::GroundMelee
                && planned.motion_set.attack_kinds.is_empty())
            .then_some(MotionAttackKind::MeleeUnarmed)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::source_rig::{
        CreatureCorpusCandidate, Fo4RaceDataTarget, Fo4RaceFlag, Fo4RaceFlag2, Fo4RaceSize,
        MotionAttack, MotionSet, RaceDataMapping, RecordVariant, RigFamily,
    };

    fn source(namespace: &str, local_form_id: u32) -> SourceCreatureIdentity {
        SourceCreatureIdentity {
            namespace: namespace.to_string(),
            plugin: format!("{namespace}.esm"),
            local_form_id,
        }
    }

    fn race_data() -> Fo4RaceDataTarget {
        Fo4RaceDataTarget {
            male_height: 1.0,
            female_height: 1.0,
            male_default_weight: [0.0; 3],
            female_default_weight: [0.0; 3],
            flags: vec![Fo4RaceFlag::Walks],
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

    fn plan(
        template: CreatureGraphTemplate,
        attack_kind: MotionAttackKind,
        candidates: Vec<(&str, u32)>,
    ) -> CreatureCorpusPlan {
        let family = RigFamily {
            id: "test-family".to_string(),
            root_node: "Root".to_string(),
            race_data: RaceDataMapping::Mapped {
                target: race_data(),
            },
        };
        let motion = MotionSet {
            id: "test-motion".to_string(),
            graph_template: template,
            idle_clip: "idle.hkx".to_string(),
            locomotion_clips: BTreeMap::from([
                ("walk_forward".to_string(), "walk.hkx".to_string()),
                ("turn_left_90".to_string(), "left.hkx".to_string()),
                ("turn_right_90".to_string(), "right.hkx".to_string()),
            ]),
            attacks: vec![MotionAttack {
                id: "attack".to_string(),
                event: if attack_kind == MotionAttackKind::MeleeUnarmed {
                    "meleeAttack".to_string()
                } else {
                    "attackEvent".to_string()
                },
                clip: "attack.hkx".to_string(),
            }],
            attack_kinds: BTreeMap::from([("attack".to_string(), attack_kind)]),
            required_overlays: Vec::new(),
            overlays: Vec::new(),
            required_rigs: Vec::new(),
            rigs: Vec::new(),
        };
        let candidates = candidates
            .into_iter()
            .map(|(namespace, local)| {
                let race = source(namespace, local);
                let npc = source(namespace, local + 1);
                CreatureCorpusCandidate {
                    source_identity: race,
                    primary_record_identity: npc.clone(),
                    output_slug: namespace.to_string(),
                    rig_family: family.id.clone(),
                    motion_set: motion.id.clone(),
                    record_variants: vec![RecordVariant {
                        source_identity: npc,
                        output_slug: "base".to_string(),
                        display_name: namespace.to_string(),
                        body_nif: format!("Actors\\{namespace}\\CharacterAssets\\body.nif"),
                        level: 1,
                        health: 25,
                        action_points: 50,
                        primary: true,
                        attack_ids: vec!["attack".to_string()],
                    }],
                    preflight_rejections: Vec::new(),
                }
            })
            .collect();
        CreatureCorpusPlan::build(vec![family], vec![motion], candidates).unwrap()
    }

    fn mapper_state() -> MapperState {
        MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "CreatureOutput.esp".to_string(),
                ..MapperOptions::default()
            },
        )
    }

    fn requests(
        plan: &CreatureCorpusPlan,
        primary_melee_attack_id: Option<&str>,
    ) -> Vec<CreatureRecordKeyPlanRequest> {
        plan.planned
            .iter()
            .map(|planned| CreatureRecordKeyPlanRequest {
                planned_source_identity: planned.source_identity.clone(),
                primary_melee_attack_id: primary_melee_attack_id.map(str::to_string),
            })
            .collect()
    }

    #[test]
    fn pure_key_planning_is_deterministic_and_does_not_mutate_mapper() {
        let plan = plan(
            CreatureGraphTemplate::GroundMelee,
            MotionAttackKind::MeleeUnarmed,
            vec![("skyrimse", 0x1000), ("fnv", 0x2000)],
        );
        let mapper = mapper_state();
        let state_before = (
            mapper.source_to_target.clone(),
            mapper.used_object_ids.clone(),
            mapper.reserved_generated_object_ids.clone(),
            mapper.next_object_id,
        );
        let interner = StringInterner::new();
        let forward = plan_creature_record_form_keys(
            &plan,
            &mapper,
            requests(&plan, Some("attack")),
            &interner,
        )
        .unwrap();
        let mut reverse_requests = requests(&plan, Some("attack"));
        reverse_requests.reverse();
        let reverse =
            plan_creature_record_form_keys(&plan, &mapper, reverse_requests, &interner).unwrap();

        assert_eq!(forward, reverse);
        assert_eq!(
            state_before,
            (
                mapper.source_to_target.clone(),
                mapper.used_object_ids.clone(),
                mapper.reserved_generated_object_ids.clone(),
                mapper.next_object_id,
            )
        );
        let locals = forward
            .iter()
            .flat_map(|keys| {
                [
                    keys.base.race.local,
                    keys.base.npc.local,
                    keys.base.skin.local,
                    keys.base.armor_addon.local,
                    keys.base.body_part_data.local,
                    keys.base.unarmed_weapon.local,
                ]
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(locals.len(), 12);
    }

    #[test]
    fn non_melee_key_plan_does_not_consume_a_phantom_unarmed_record() {
        let plan = plan(
            CreatureGraphTemplate::GroundRangedProjectile,
            MotionAttackKind::RangedProjectile,
            vec![("fnv", 0x2000)],
        );
        let mapper = mapper_state();
        let interner = StringInterner::new();
        let keys = plan_creature_record_form_keys(&plan, &mapper, requests(&plan, None), &interner)
            .unwrap()
            .pop()
            .unwrap();

        assert_eq!(keys.base.unarmed_weapon.local, 0);
        assert!(keys.melee_attacks.is_empty());
        assert_eq!(keys.base.race.local, 0x800);
        assert_eq!(keys.base.body_part_data.local, 0x804);
        assert_eq!(mapper.next_object_id, 0x800);
    }

    #[test]
    fn missing_and_duplicate_key_plan_requests_are_typed_and_atomic() {
        let plan = plan(
            CreatureGraphTemplate::GroundMelee,
            MotionAttackKind::MeleeUnarmed,
            vec![("fnv", 0x2000)],
        );
        let mapper = mapper_state();
        let interner = StringInterner::new();
        let next_before = mapper.next_object_id;

        assert!(matches!(
            plan_creature_record_form_keys(&plan, &mapper, Vec::new(), &interner),
            Err(CreatureRecordBatchBuildError::InvalidKeyPlanRequest { .. })
        ));
        let request = requests(&plan, Some("attack")).pop().unwrap();
        assert!(matches!(
            plan_creature_record_form_keys(
                &plan,
                &mapper,
                vec![request.clone(), request],
                &interner,
            ),
            Err(CreatureRecordBatchBuildError::InvalidKeyPlanRequest { .. })
        ));
        assert_eq!(mapper.next_object_id, next_before);
        assert!(mapper.used_object_ids.is_empty());
        assert!(mapper.source_to_target.is_empty());
    }
}
