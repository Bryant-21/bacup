use std::collections::{BTreeMap, BTreeSet};

use havok_native::hkx::descriptors::{DescriptorRegistry, MemberTemplate};
use havok_native::hkx::types::HkxType;
use serde::{Deserialize, Serialize};

use super::manifest::{CreatureManifest, RagdollDisposition, ValidationErrors, VariableType};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRecoveryGraph {
    pub behavior_path: String,
    pub root: u32,
    pub living_generator: u32,
    pub nodes: BTreeMap<u32, SourceRecoveryNode>,
    pub variables: Vec<SourceRecoveryVariable>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRecoveryVariable {
    pub name: String,
    pub variable_type: VariableType,
    pub initial_word: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SourceRecoveryNode {
    pub class_name: String,
    pub members: BTreeMap<String, SourceRecoveryValue>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SourceRecoveryValue {
    Bool(bool),
    Int(i64),
    Real(u32),
    Vector(Vec<u32>),
    String(String),
    Reference(Option<u32>),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
    Event(String),
    Variable(String),
    Clip(String),
}

pub(crate) fn allowed_class(class: &str) -> bool {
    matches!(
        class,
        "hkbStateMachine"
            | "hkbStateMachineStateInfo"
            | "hkbStateMachineTransitionInfoArray"
            | "hkbStateMachineEventPropertyArray"
            | "hkbBlendingTransitionEffect"
            | "hkbModifierGenerator"
            | "hkbModifierList"
            | "hkbVariableBindingSet"
            | "hkbManualSelectorGenerator"
            | "hkbPoseMatchingGenerator"
            | "hkbBlenderGeneratorChild"
            | "hkbClipGenerator"
            | "hkbClipTriggerArray"
            | "hkbBoneIndexArray"
            | "hkbBoneWeightArray"
            | "hkbGetUpModifier"
            | "hkbPoweredRagdollControlsModifier"
            | "hkbRigidBodyRagdollControlsModifier"
            | "hkbKeyframeBonesModifier"
            | "hkbEventDrivenModifier"
            | "hkbTimerModifier"
            | "BSIsActiveModifier"
            | "BSRagdollContactListenerModifier"
            | "hkbReferencePoseGenerator"
    )
}

impl SourceRecoveryValue {
    pub(crate) fn visit<'a>(&'a self, visit: &mut impl FnMut(&'a Self)) {
        visit(self);
        match self {
            Self::Array(values) => values.iter().for_each(|value| value.visit(visit)),
            Self::Object(values) => values.values().for_each(|value| value.visit(visit)),
            _ => {}
        }
    }
}

impl SourceRecoveryGraph {
    pub(crate) fn clip_names(&self) -> Vec<&str> {
        let mut clips = BTreeSet::new();
        self.visit(&mut |value| {
            if let SourceRecoveryValue::Clip(name) = value {
                clips.insert(name.as_str());
            }
        });
        clips.into_iter().collect()
    }

    pub(crate) fn event_names(&self) -> Vec<&str> {
        let mut events = BTreeSet::new();
        self.visit(&mut |value| {
            if let SourceRecoveryValue::Event(name) = value {
                events.insert(name.as_str());
            }
        });
        events.into_iter().collect()
    }

    pub(crate) fn visit<'a>(&'a self, visit: &mut impl FnMut(&'a SourceRecoveryValue)) {
        self.nodes
            .values()
            .for_each(|node| node.members.values().for_each(|value| value.visit(visit)));
    }

    pub(crate) fn validate(&self, manifest: &CreatureManifest, errors: &mut ValidationErrors) {
        if self.behavior_path.is_empty()
            || !self.nodes.contains_key(&self.root)
            || self.nodes.contains_key(&self.living_generator)
            || !matches!(manifest.ragdoll, RagdollDisposition::SourceOwned { .. })
        {
            errors.push("source_recovery_root", "source recovery requires a source-owned ragdoll and a distinct source living-generator replacement");
        }
        let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
        for node in self.nodes.values() {
            if !allowed_class(&node.class_name) {
                errors.push(
                    "source_recovery_class",
                    format!("unsupported recovery class {}", node.class_name),
                );
                continue;
            }
            let Ok(members) = registry.get_all_members(&node.class_name) else {
                errors.push(
                    "source_recovery_descriptor",
                    format!("missing target descriptor {}", node.class_name),
                );
                continue;
            };
            for (name, value) in &node.members {
                if let Some(member) = members.iter().find(|member| member.name == *name) {
                    validate_value(self, value, member, &mut registry, errors);
                } else {
                    errors.push(
                        "source_recovery_member",
                        format!("{}.{} has no target member", node.class_name, name),
                    );
                }
            }
        }
        if !self
            .nodes
            .get(&self.root)
            .is_some_and(|node| node.class_name == "hkbStateMachine")
        {
            errors.push(
                "source_recovery_root",
                "recovery root must retain its source state machine",
            );
        }
        let variables = self
            .variables
            .iter()
            .map(|variable| variable.name.as_str())
            .collect::<BTreeSet<_>>();
        if variables.len() != self.variables.len() {
            errors.push(
                "source_recovery_variable",
                "duplicate recovery variable names",
            );
        }
        for variable in &self.variables {
            if let Some(decl) = manifest
                .root
                .variables
                .iter()
                .find(|decl| decl.name == variable.name)
            {
                if decl.variable_type != variable.variable_type
                    || decl.initial_value.word_bits() as u32 != variable.initial_word
                {
                    errors.push(
                        "source_recovery_variable",
                        format!(
                            "source initial value/type for {} conflicts with root declaration",
                            variable.name
                        ),
                    );
                }
            }
        }
        self.visit(&mut |value| match value {
            SourceRecoveryValue::Reference(Some(index))
                if *index != self.living_generator && !self.nodes.contains_key(index) =>
            {
                errors.push(
                    "source_recovery_reference",
                    format!("unresolved source object {index}"),
                )
            }
            SourceRecoveryValue::Real(bits) if !f32::from_bits(*bits).is_finite() => {
                errors.push("source_recovery_number", "nonfinite source recovery scalar")
            }
            SourceRecoveryValue::Vector(bits)
                if bits.iter().any(|bits| !f32::from_bits(*bits).is_finite()) =>
            {
                errors.push("source_recovery_number", "nonfinite source recovery vector")
            }
            SourceRecoveryValue::Variable(name) if !variables.contains(name.as_str()) => errors
                .push(
                    "source_recovery_variable",
                    format!("missing recovery variable {name}"),
                ),
            _ => {}
        });
        let events = self.event_names();
        for event in ["GetUpStart", "GetUpEnd", "Ragdoll"] {
            if !events.contains(&event) {
                errors.push(
                    "source_recovery_lifecycle",
                    format!("source recovery has no {event} lifecycle event"),
                );
            }
        }
        self.validate_lifecycle(errors);
    }

    fn validate_lifecycle(&self, errors: &mut ValidationErrors) {
        use SourceRecoveryValue as V;
        let Some(root) = self.nodes.get(&self.root) else {
            return;
        };
        let Some(V::Array(states)) = root.members.get("states") else {
            errors.push("source_recovery_lifecycle", "source recovery has no states");
            return;
        };
        let states = states
            .iter()
            .filter_map(|value| match value {
                V::Reference(Some(index)) => self.nodes.get(index),
                _ => None,
            })
            .collect::<Vec<_>>();
        let living = states.iter().find(|state| {
            state.members.get("generator") == Some(&V::Reference(Some(self.living_generator)))
        });
        let Some(living) = living else {
            errors.push(
                "source_recovery_lifecycle",
                "source recovery has no living state",
            );
            return;
        };
        if root.members.get("startStateId") != living.members.get("stateId") {
            errors.push(
                "source_recovery_lifecycle",
                "source recovery must start in its source living state",
            );
        }
        let ragdoll_entry = match root.members.get("wildcardTransitions") {
            Some(V::Reference(Some(index))) => self.nodes.get(index),
            _ => None,
        }
        .and_then(|array| match array.members.get("transitions") {
            Some(V::Array(values)) => Some(values),
            _ => None,
        })
        .is_some_and(|transitions| transitions.iter().any(|value| {
            matches!(value, V::Object(fields)
                if fields.get("eventId") == Some(&V::Event("Ragdoll".into()))
                && fields.get("toStateId") != living.members.get("stateId")
                && states.iter().any(|state| state.members.get("stateId") == fields.get("toStateId")))
        }));
        if !ragdoll_entry {
            errors.push(
                "source_recovery_lifecycle",
                "source recovery requires an exact wildcard Ragdoll destination",
            );
        }
        let completion = states.iter().filter_map(|state| match state.members.get("transitions") {
            Some(V::Reference(Some(index))) => self.nodes.get(index), _ => None,
        }).filter_map(|array| match array.members.get("transitions") { Some(V::Array(values)) => Some(values), _ => None }).flatten().any(|value| {
            matches!(value, V::Object(fields) if fields.get("eventId") == Some(&V::Event("GetUpEnd".into())) && fields.get("toStateId") == living.members.get("stateId"))
        });
        if !completion {
            errors.push(
                "source_recovery_lifecycle",
                "GetUpEnd must return to the exact source living state",
            );
        }
    }
}

fn validate_value(
    graph: &SourceRecoveryGraph,
    value: &SourceRecoveryValue,
    member: &MemberTemplate,
    registry: &mut DescriptorRegistry,
    errors: &mut ValidationErrors,
) {
    use SourceRecoveryValue as Value;
    let valid = match (member.vtype, value) {
        (HkxType::Bool, Value::Bool(_)) => true,
        (HkxType::Real | HkxType::Half, Value::Real(_)) => true,
        (HkxType::CString | HkxType::StringPtr, Value::String(_) | Value::Clip(_)) => true,
        (HkxType::Pointer, Value::Reference(None)) => true,
        (HkxType::Pointer, Value::Reference(Some(index))) => {
            let class = if *index == graph.living_generator {
                Some("BSBehaviorGraphSwapGenerator")
            } else {
                graph.nodes.get(index).map(|node| node.class_name.as_str())
            };
            class.is_some_and(|class| pointer_class_matches(class, &member.ctype, registry))
        }
        (HkxType::Int8, Value::Int(v)) => i8::try_from(*v).is_ok(),
        (HkxType::Uint8, Value::Int(v)) => u8::try_from(*v).is_ok(),
        (HkxType::Int16, Value::Int(v)) => i16::try_from(*v).is_ok(),
        (HkxType::Uint16, Value::Int(v)) => u16::try_from(*v).is_ok(),
        (HkxType::Int32, Value::Int(v)) => i32::try_from(*v).is_ok(),
        (HkxType::Uint32, Value::Int(v)) => u32::try_from(*v).is_ok(),
        (HkxType::Int64 | HkxType::Uint64 | HkxType::Ulong, Value::Int(_)) => true,
        (HkxType::Int32, Value::Event(_) | Value::Variable(_)) => true,
        (HkxType::Enum | HkxType::Flags, Value::Int(_)) => {
            let mut storage = member.clone();
            storage.vtype = member.vsubtype;
            validate_value(graph, value, &storage, registry, errors);
            true
        }
        (HkxType::Vector4 | HkxType::Quaternion, Value::Vector(bits)) => bits.len() == 4,
        (HkxType::QsTransform | HkxType::Transform | HkxType::Matrix3, Value::Vector(bits)) => {
            bits.len() == 12
        }
        (HkxType::Matrix4, Value::Vector(bits)) => bits.len() == 16,
        (HkxType::Array | HkxType::SimpleArray, Value::Array(values)) => {
            let mut element = member.clone();
            element.vtype = member.vsubtype;
            for value in values {
                validate_value(graph, value, &element, registry, errors);
            }
            true
        }
        (HkxType::Struct, Value::Object(values)) => {
            match registry.get_all_members(&member.ctype) {
                Ok(members) => {
                    for (name, value) in values {
                        if let Some(field) = members.iter().find(|field| field.name == *name) {
                            validate_value(graph, value, field, registry, errors);
                        } else {
                            errors.push(
                                "source_recovery_member",
                                format!("{}.{} has no target member", member.ctype, name),
                            );
                        }
                    }
                }
                Err(error) => errors.push("source_recovery_descriptor", error.to_string()),
            }
            true
        }
        _ => false,
    };
    if !valid {
        errors.push(
            "source_recovery_member_type",
            format!(
                "{} requires {:?}, received {value:?}",
                member.name, member.vtype
            ),
        );
    }
}

fn pointer_class_matches(class: &str, target: &str, registry: &mut DescriptorRegistry) -> bool {
    if target.is_empty() || target == "void" {
        return true;
    }
    let mut class = Some(class.to_string());
    while let Some(current) = class {
        if current == target {
            return true;
        }
        class = registry
            .get(&current)
            .ok()
            .flatten()
            .and_then(|descriptor| descriptor.parent.clone());
    }
    false
}
