//! Shared admission policy for newly authored FO4 creature behavior families.

use super::{CapabilityGraphManifest, CreatureClipRole, TargetFormKey};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorAdmissionSeverity {
    Degradable,
    Fatal,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BehaviorAdmissionIssue {
    pub severity: BehaviorAdmissionSeverity,
    pub code: String,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatureFallbackDisposition {
    Omitted,
    TargetDefault,
    SupportedSubset,
    ReplacedByGeneratedRuntime,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CreatureDegradationReceipt {
    pub code: String,
    pub detail: String,
    pub policy_id: String,
    pub disposition: CreatureFallbackDisposition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affected_source_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_signature: Option<String>,
}

impl CreatureDegradationReceipt {
    pub fn canonicalize(&mut self) {
        self.affected_source_keys
            .sort_by_key(|source| source.to_ascii_lowercase());
        self.affected_source_keys
            .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        if let Some(signature) = &mut self.source_signature {
            *signature = signature.to_ascii_uppercase();
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.code.trim().is_empty()
            || self.detail.trim().is_empty()
            || self.policy_id.trim().is_empty()
        {
            return Err("degradation receipt code, detail, and policy id must be nonempty");
        }
        if self.policy_id.contains('/')
            || self.policy_id.contains('\\')
            || self.policy_id.contains('@')
        {
            return Err("degradation receipt policy id must be source-neutral");
        }
        if self
            .affected_source_keys
            .iter()
            .any(|source| source.trim().is_empty())
        {
            return Err("degradation receipt source keys must be nonempty");
        }
        if self.source_signature.as_ref().is_some_and(|signature| {
            signature.len() != 4
                || !signature
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        }) {
            return Err("degradation receipt source signature must be a four-character code");
        }
        Ok(())
    }
}

impl BehaviorAdmissionIssue {
    pub fn degradable(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: BehaviorAdmissionSeverity::Degradable,
            code: code.into(),
            detail: detail.into(),
        }
    }

    pub fn fatal(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: BehaviorAdmissionSeverity::Fatal,
            code: code.into(),
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BehaviorAdmissionReport {
    pub issues: Vec<BehaviorAdmissionIssue>,
}

impl BehaviorAdmissionReport {
    pub fn canonicalize(&mut self) {
        self.issues.sort();
        self.issues.dedup();
    }

    pub fn has_fatal(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| issue.severity == BehaviorAdmissionSeverity::Fatal)
    }

    pub fn degradable_issues(&self) -> impl Iterator<Item = &BehaviorAdmissionIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.severity == BehaviorAdmissionSeverity::Degradable)
    }
}

pub fn deterministic_standard_event(
    role: CreatureClipRole,
    family_fragment: &str,
    ordinal: usize,
) -> String {
    match role {
        CreatureClipRole::GroundForward => "moveStart".to_string(),
        CreatureClipRole::TurnLeft90 => "TurnLeft90".to_string(),
        CreatureClipRole::TurnRight90 => "TurnRight90".to_string(),
        CreatureClipRole::MeleeAttack => {
            format!("melee_{family_fragment}_{ordinal:02}")
        }
        CreatureClipRole::ProjectileAttack => {
            format!("projectile_{family_fragment}_{ordinal:02}")
        }
        CreatureClipRole::SwimForward => "SwimForward".to_string(),
        CreatureClipRole::FlyForward => "FlyForward".to_string(),
        CreatureClipRole::ContinuousAttackStart => "AttackStart".to_string(),
        CreatureClipRole::ContinuousAttackLoop => "AttackLoop".to_string(),
        CreatureClipRole::ContinuousAttackStop => "AttackStop".to_string(),
        CreatureClipRole::Idle
        | CreatureClipRole::SwimIdle
        | CreatureClipRole::FlyIdle
        | CreatureClipRole::StationaryIdle => "Idle".to_string(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorActionKind {
    Idle,
    MoveStart,
    MoveStop,
    Death,
    RagdollInstant,
    Melee,
    FireSingle,
    FireAuto,
    FireStop,
    Draw,
    Sheath,
    ForceEquip,
    SwimStart,
    SwimStop,
    FlyStart,
    FlyStop,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ActorActionRequirement {
    pub kind: ActorActionKind,
    pub parent_editor_id: String,
    pub parent_form_id: u32,
    pub behavior_path: String,
    pub animation_event: String,
}

impl ActorActionRequirement {
    pub fn parent_form_key(&self) -> TargetFormKey {
        TargetFormKey::new(self.parent_form_id, "Fallout4.esm")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CreatureActorActionRecordPlan {
    pub requirement: ActorActionRequirement,
    pub form_key: TargetFormKey,
    pub editor_id: String,
}

pub fn required_actor_action_records(
    graph: &CapabilityGraphManifest,
    root_behavior_path: &str,
) -> Vec<ActorActionRequirement> {
    let mut requirements = vec![
        ActorActionRequirement {
            kind: ActorActionKind::Idle,
            parent_editor_id: "ActionIdle".to_string(),
            parent_form_id: 0x013002,
            behavior_path: root_behavior_path.to_string(),
            animation_event: graph.idle_event.clone(),
        },
        ActorActionRequirement {
            kind: ActorActionKind::Death,
            parent_editor_id: "ActionDeath".to_string(),
            parent_form_id: 0x0489ED,
            behavior_path: root_behavior_path.to_string(),
            animation_event: "Ragdoll".to_string(),
        },
    ];
    extend_role_events(
        &mut requirements,
        graph,
        root_behavior_path,
        CreatureClipRole::GroundForward,
        ActorActionKind::MoveStart,
        "ActionMoveStart",
        0x0959F8,
    );
    if graph
        .roles
        .iter()
        .any(|role| role.role == CreatureClipRole::GroundForward)
    {
        requirements.push(ActorActionRequirement {
            kind: ActorActionKind::MoveStop,
            parent_editor_id: "ActionMoveStop".to_string(),
            parent_form_id: 0x0959F9,
            behavior_path: root_behavior_path.to_string(),
            animation_event: graph.idle_event.clone(),
        });
    }
    extend_role_events(
        &mut requirements,
        graph,
        root_behavior_path,
        CreatureClipRole::MeleeAttack,
        ActorActionKind::Melee,
        "ActionMelee",
        0x004A59,
    );
    extend_role_events(
        &mut requirements,
        graph,
        root_behavior_path,
        CreatureClipRole::ProjectileAttack,
        ActorActionKind::FireSingle,
        "ActionFireSingle",
        0x004A5A,
    );
    extend_role_events(
        &mut requirements,
        graph,
        root_behavior_path,
        CreatureClipRole::ContinuousAttackStart,
        ActorActionKind::FireAuto,
        "ActionFireAuto",
        0x004A5C,
    );
    extend_role_events(
        &mut requirements,
        graph,
        root_behavior_path,
        CreatureClipRole::ContinuousAttackStop,
        ActorActionKind::FireStop,
        "ActionRightRelease",
        0x013454,
    );
    extend_role_events(
        &mut requirements,
        graph,
        root_behavior_path,
        CreatureClipRole::SwimForward,
        ActorActionKind::SwimStart,
        "ActionSwimStart",
        0x14DB5D,
    );
    if graph
        .roles
        .iter()
        .any(|role| role.role == CreatureClipRole::SwimForward)
    {
        requirements.push(ActorActionRequirement {
            kind: ActorActionKind::SwimStop,
            parent_editor_id: "ActionSwimStop".to_string(),
            parent_form_id: 0x14DB5E,
            behavior_path: root_behavior_path.to_string(),
            animation_event: graph.idle_event.clone(),
        });
    }
    extend_role_events(
        &mut requirements,
        graph,
        root_behavior_path,
        CreatureClipRole::FlyForward,
        ActorActionKind::FlyStart,
        "ActionFlyStart",
        0x03B4E3,
    );
    if graph
        .roles
        .iter()
        .any(|role| role.role == CreatureClipRole::FlyForward)
    {
        requirements.push(ActorActionRequirement {
            kind: ActorActionKind::FlyStop,
            parent_editor_id: "ActionFlyStop".to_string(),
            parent_form_id: 0x03B4E4,
            behavior_path: root_behavior_path.to_string(),
            animation_event: graph.idle_event.clone(),
        });
    }
    for event in &graph.explicit_events {
        let (kind, parent_editor_id, parent_form_id) =
            match event.name.to_ascii_lowercase().as_str() {
                "weapequip" | "weaponequip" | "weapdraw" | "weapondraw" => {
                    (ActorActionKind::Draw, "ActionDraw", 0x0132AF)
                }
                "weapunequip" | "weaponunequip" | "weapsheath" | "weaponsheath"
                | "weaponsheathe" => (ActorActionKind::Sheath, "ActionSheath", 0x046BAF),
                "forceequip" | "g_weapforceequip" => {
                    (ActorActionKind::ForceEquip, "ActionForceEquip", 0x02ADF1)
                }
                "ragdollinstant" => (
                    ActorActionKind::RagdollInstant,
                    "ActionRagdollInstant",
                    0x09BB4E,
                ),
                _ => continue,
            };
        requirements.push(ActorActionRequirement {
            kind,
            parent_editor_id: parent_editor_id.to_string(),
            parent_form_id,
            behavior_path: root_behavior_path.to_string(),
            animation_event: event.name.clone(),
        });
    }
    requirements.sort();
    requirements.dedup();
    requirements
}

fn extend_role_events(
    requirements: &mut Vec<ActorActionRequirement>,
    graph: &CapabilityGraphManifest,
    root_behavior_path: &str,
    role: CreatureClipRole,
    kind: ActorActionKind,
    parent_editor_id: &str,
    parent_form_id: u32,
) {
    for declaration in graph.roles.iter().filter(|entry| entry.role == role) {
        for event in declaration
            .trigger_event
            .iter()
            .chain(&declaration.trigger_aliases)
            .filter(|event| !event.trim().is_empty())
        {
            requirements.push(ActorActionRequirement {
                kind,
                parent_editor_id: parent_editor_id.to_string(),
                parent_form_id,
                behavior_path: root_behavior_path.to_string(),
                animation_event: event.clone(),
            });
        }
    }
}

pub fn actor_action_admission_report(
    graph: &CapabilityGraphManifest,
    root_behavior_path: &str,
    emitted: &[CreatureActorActionRecordPlan],
) -> BehaviorAdmissionReport {
    let mut report = BehaviorAdmissionReport::default();
    for requirement in required_actor_action_records(graph, root_behavior_path) {
        if !emitted.iter().any(|record| {
            record.requirement == requirement
                && record.form_key.local != 0
                && !record.form_key.plugin.trim().is_empty()
                && !record.editor_id.trim().is_empty()
        }) {
            report.issues.push(BehaviorAdmissionIssue::fatal(
                "missing_actor_action_idle_record",
                format!(
                    "missing IDLE child of {} [{:06X}:Fallout4.esm] for {:?}",
                    requirement.parent_editor_id,
                    requirement.parent_form_id,
                    requirement.animation_event,
                ),
            ));
        }
    }
    report.canonicalize();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admission_report_only_blocks_on_fatal_issues() {
        let mut report = BehaviorAdmissionReport {
            issues: vec![
                BehaviorAdmissionIssue::degradable(
                    "ambiguous_optional_topology",
                    "the new FO4 template selected the canonical clip",
                ),
                BehaviorAdmissionIssue::fatal(
                    "missing_mandatory_attack_clip",
                    "no usable combat clip exists",
                ),
                BehaviorAdmissionIssue::degradable(
                    "ambiguous_optional_topology",
                    "the new FO4 template selected the canonical clip",
                ),
            ],
        };
        report.canonicalize();
        assert!(report.has_fatal());
        assert_eq!(report.degradable_issues().count(), 1);
    }

    #[test]
    fn degradation_receipts_are_canonical_and_source_neutral() {
        let mut receipt = CreatureDegradationReceipt {
            code: "source_semantic_defaulted".to_string(),
            detail: "FO4 uses the target default for the optional source field".to_string(),
            policy_id: "optional_target_default_v1".to_string(),
            disposition: CreatureFallbackDisposition::TargetDefault,
            affected_source_keys: vec![
                "000ABC:FalloutNV.esm".to_string(),
                "000abc:falloutnv.esm".to_string(),
                "000123:FalloutNV.esm".to_string(),
            ],
            source_signature: Some("npc_".to_string()),
        };
        receipt.canonicalize();
        assert_eq!(
            receipt.affected_source_keys,
            ["000123:FalloutNV.esm", "000ABC:FalloutNV.esm"]
        );
        assert_eq!(receipt.source_signature.as_deref(), Some("NPC_"));
        assert_eq!(receipt.validate(), Ok(()));

        receipt.policy_id = "fnv/source-specific".to_string();
        assert!(receipt.validate().is_err());
    }

    #[test]
    fn standard_attack_events_are_stable_and_distinct() {
        assert_eq!(
            deterministic_standard_event(CreatureClipRole::MeleeAttack, "wolf", 2),
            "melee_wolf_02"
        );
        assert_eq!(
            deterministic_standard_event(CreatureClipRole::ProjectileAttack, "wolf", 2),
            "projectile_wolf_02"
        );
    }

    #[test]
    fn absent_actor_action_idles_are_explicit_fatal_integration_gaps() {
        let graph = CapabilityGraphManifest {
            template: super::super::CreatureGraphTemplate::GroundMelee,
            roles: vec![super::super::CapabilityClipRole {
                role: CreatureClipRole::MeleeAttack,
                state_name: "MeleeAttack1".to_string(),
                clip_name: "attack".to_string(),
                generator: super::super::CapabilityRoleGenerator::Single,
                trigger_event: Some("melee_fixture_01".to_string()),
                trigger_aliases: Vec::new(),
                motion: super::super::ClipMotionPolicy::default(),
            }],
            candidate_attack_bindings: Vec::new(),
            idle_event: "Idle".to_string(),
            explicit_events: Vec::new(),
            overlays: Vec::new(),
        };
        let report = actor_action_admission_report(
            &graph,
            "Actors\\Fixture\\Behaviors\\FixtureRootBehavior.hkx",
            &[],
        );
        assert!(report.has_fatal());
        assert_eq!(report.issues.len(), 3);
    }

    #[test]
    fn actor_action_requirements_cover_graph_motion_combat_and_equipment_events() {
        let role = |role, event: &str| super::super::CapabilityClipRole {
            role,
            state_name: format!("{role:?}"),
            clip_name: format!("{role:?}_clip"),
            generator: super::super::CapabilityRoleGenerator::Single,
            trigger_event: Some(event.to_string()),
            trigger_aliases: Vec::new(),
            motion: super::super::ClipMotionPolicy::default(),
        };
        let mut ground = role(CreatureClipRole::GroundForward, "moveStart");
        ground.trigger_aliases.push("moveFast".to_string());
        let graph = CapabilityGraphManifest {
            template: super::super::CreatureGraphTemplate::GroundMelee,
            roles: vec![
                ground,
                role(CreatureClipRole::MeleeAttack, "meleePrimary"),
                role(CreatureClipRole::ProjectileAttack, "rangedPrimary"),
                role(CreatureClipRole::ContinuousAttackStart, "rangedStart"),
                role(CreatureClipRole::ContinuousAttackStop, "rangedStop"),
                role(CreatureClipRole::SwimForward, "swimStart"),
                role(CreatureClipRole::FlyForward, "flyStart"),
            ],
            candidate_attack_bindings: Vec::new(),
            idle_event: "Idle".to_string(),
            explicit_events: [
                "weaponDraw",
                "weaponSheathe",
                "forceEquip",
                "ragdollInstant",
            ]
            .map(|name| super::super::EventDecl {
                name: name.to_string(),
                usage: super::super::EventUsage::Generic,
                flags: 0,
            })
            .to_vec(),
            overlays: Vec::new(),
        };

        let requirements = required_actor_action_records(
            &graph,
            "Actors\\Fixture\\Behaviors\\FixtureRootBehavior.hkx",
        );
        let actual_kinds = requirements
            .iter()
            .map(|requirement| requirement.kind)
            .collect::<std::collections::BTreeSet<_>>();
        let expected_kinds = [
            ActorActionKind::Idle,
            ActorActionKind::MoveStart,
            ActorActionKind::MoveStop,
            ActorActionKind::Death,
            ActorActionKind::RagdollInstant,
            ActorActionKind::Melee,
            ActorActionKind::FireSingle,
            ActorActionKind::FireAuto,
            ActorActionKind::FireStop,
            ActorActionKind::Draw,
            ActorActionKind::Sheath,
            ActorActionKind::ForceEquip,
            ActorActionKind::SwimStart,
            ActorActionKind::SwimStop,
            ActorActionKind::FlyStart,
            ActorActionKind::FlyStop,
        ]
        .into_iter()
        .collect();
        assert_eq!(actual_kinds, expected_kinds);
        assert_eq!(requirements.len(), 17, "moveStart includes one graph alias");
        assert!(requirements.iter().all(|requirement| {
            requirement.behavior_path == "Actors\\Fixture\\Behaviors\\FixtureRootBehavior.hkx"
                && requirement.parent_form_key().plugin == "Fallout4.esm"
        }));
    }
}
