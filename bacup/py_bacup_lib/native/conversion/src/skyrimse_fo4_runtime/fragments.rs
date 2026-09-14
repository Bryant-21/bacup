use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use serde_json::{Value, json};

use crate::fnv_legacy_scripting::dialogue::InfoFragmentPhase;
use crate::fnv_legacy_scripting::vmad::{
    CompiledScriptEvidence, QuestAliasBinding, QuestAliasScriptBinding, QuestStageFragment,
    ScriptProperty, attach_vmad_payload_to_record, property_payload,
    synthesize_quest_vmad_with_properties, synthesize_scene_vmad,
    synthesize_topic_info_vmad_with_properties,
};
use crate::ids::SubrecordSig;
use crate::quest_runtime::QuestRecordKey;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

use super::papyrus::{
    GeneratedPscManifestRow, SkyrimPscCompilerEvidence, SkyrimPscKind, SkyrimPscSourceArtifact,
    psc_manifest_row,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkyrimFragmentClass {
    QuestStage,
    TopicInfo,
    Scene,
    Package,
    ReferenceAlias,
    LocationAlias,
}

impl SkyrimFragmentClass {
    pub fn owner_signature(self) -> &'static str {
        match self {
            Self::QuestStage | Self::ReferenceAlias | Self::LocationAlias => "QUST",
            Self::TopicInfo => "INFO",
            Self::Scene => "SCEN",
            Self::Package => "PACK",
        }
    }

    pub fn parent_class(self) -> &'static str {
        match self {
            Self::QuestStage => "Quest",
            Self::TopicInfo => "TopicInfo",
            Self::Scene => "Scene",
            Self::Package => "Package",
            Self::ReferenceAlias => "ReferenceAlias",
            Self::LocationAlias => "LocationAlias",
        }
    }

    fn psc_kind(self) -> SkyrimPscKind {
        match self {
            Self::QuestStage => SkyrimPscKind::QuestFragment,
            Self::TopicInfo => SkyrimPscKind::InfoFragment,
            Self::Scene | Self::Package => SkyrimPscKind::RecordScript,
            Self::ReferenceAlias | Self::LocationAlias => SkyrimPscKind::AliasScript,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimFragmentArtifactProvenance {
    pub relative_path: String,
    pub blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimFragmentPropertyIntent {
    pub name: String,
    pub type_name: String,
    pub target_value: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimFragmentSourceProvenance {
    pub class_name: String,
    pub parent_class: String,
    pub psc: SkyrimFragmentArtifactProvenance,
    pub pex: SkyrimFragmentArtifactProvenance,
    pub entrypoints: BTreeSet<String>,
    pub properties: BTreeSet<SkyrimFragmentPropertyIntent>,
    pub api_calls: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimFragmentPlanRequest {
    pub owner: QuestRecordKey,
    pub alias_id: Option<u32>,
    pub fragment_class: SkyrimFragmentClass,
    pub source: SkyrimFragmentSourceProvenance,
    pub target_psc: SkyrimPscSourceArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimFragmentBindingPlan {
    pub owner: QuestRecordKey,
    pub alias_id: Option<u32>,
    pub fragment_class: SkyrimFragmentClass,
    pub parent_class: String,
    pub source: SkyrimFragmentSourceProvenance,
    pub target_psc: SkyrimPscSourceArtifact,
    pub compiler_manifest: GeneratedPscManifestRow,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SkyrimFragmentVmadAttachmentKey {
    pub owner: QuestRecordKey,
    pub alias_id: Option<u32>,
    pub fragment_class: SkyrimFragmentClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimFragmentVmadAttachmentIntent {
    pub key: SkyrimFragmentVmadAttachmentKey,
    pub script_class: String,
    pub parent_class: String,
    pub entrypoints: BTreeSet<String>,
    pub properties: BTreeSet<SkyrimFragmentPropertyIntent>,
    pub compiler_manifest_id: String,
    pub compiler_run_id: String,
    pub relative_pex_path: String,
    pub pex_blake3: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimFragmentVmadRecordReceipt {
    pub owner: QuestRecordKey,
    pub attachment_keys: BTreeSet<SkyrimFragmentVmadAttachmentKey>,
    pub script_classes: BTreeSet<String>,
    pub entrypoints: BTreeSet<String>,
    pub property_names: BTreeSet<String>,
    pub vmad_size: usize,
    pub vmad_blake3: String,
}

pub fn materialize_fragment_vmad_attachment(
    intent: &SkyrimFragmentVmadAttachmentIntent,
    target: &mut Record,
    interner: &StringInterner,
    masters: &[String],
    output_plugin_name: &str,
) -> Result<SkyrimFragmentVmadRecordReceipt, String> {
    materialize_fragment_vmad_attachments(
        std::slice::from_ref(intent),
        target,
        interner,
        masters,
        output_plugin_name,
    )
}

pub fn materialize_fragment_vmad_attachments(
    intents: &[SkyrimFragmentVmadAttachmentIntent],
    target: &mut Record,
    interner: &StringInterner,
    masters: &[String],
    output_plugin_name: &str,
) -> Result<SkyrimFragmentVmadRecordReceipt, String> {
    validate_materialization_target(intents, target, interner)?;
    if target.fields.iter().any(|field| field.sig.0 == *b"VMAD") {
        return Err(format!(
            "Skyrim fragment target {} already has a VMAD attachment",
            target.form_key.format(interner)
        ));
    }

    let payload = synthesize_fragment_vmad_payload(intents)?;
    let mut authoring_record = json!({ "fields": [] });
    attach_vmad_payload_to_record(&mut authoring_record, payload)
        .map_err(|error| error.to_string())?;
    let attached_payload = authoring_record["fields"]
        .as_array()
        .and_then(|fields| fields.first())
        .and_then(|field| field.get("VirtualMachineAdapter"))
        .ok_or_else(|| "Skyrim fragment VMAD attachment helper emitted no payload".to_string())?;
    let bytes = build_vmad_bytes_from_payload(attached_payload, masters, output_plugin_name)
        .ok_or_else(|| "Skyrim fragment VMAD payload is not encodable for Fallout 4".to_string())?;
    if bytes.is_empty() {
        return Err("Skyrim fragment VMAD encoder emitted an empty payload".to_string());
    }
    target.fields.insert(
        0,
        FieldEntry {
            sig: SubrecordSig(*b"VMAD"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes)),
        },
    );

    fragment_vmad_receipt_from_record(intents, target, interner, masters, output_plugin_name)
}

pub fn fragment_vmad_receipt_from_record(
    intents: &[SkyrimFragmentVmadAttachmentIntent],
    target: &Record,
    interner: &StringInterner,
    masters: &[String],
    output_plugin_name: &str,
) -> Result<SkyrimFragmentVmadRecordReceipt, String> {
    validate_materialization_target(intents, target, interner)?;
    let vmad_fields = target
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"VMAD")
        .collect::<Vec<_>>();
    if vmad_fields.len() != 1 {
        return Err(format!(
            "Skyrim fragment receipt requires exactly one VMAD, found {}",
            vmad_fields.len()
        ));
    }
    let FieldValue::Bytes(actual) = &vmad_fields[0].value else {
        return Err("Skyrim fragment receipt requires a raw VMAD byte payload".to_string());
    };
    if actual.is_empty() {
        return Err("Skyrim fragment receipt found an empty VMAD payload".to_string());
    }

    let expected_payload = synthesize_fragment_vmad_payload(intents)?;
    let expected = build_vmad_bytes_from_payload(&expected_payload, masters, output_plugin_name)
        .ok_or_else(|| "Skyrim fragment expected VMAD payload is not encodable".to_string())?;
    if actual.as_slice() != expected {
        return Err(
            "Skyrim fragment record VMAD does not exactly match its attachment intents".to_string(),
        );
    }
    let signature = target.sig.as_str();
    let decoded = compact_vmad_payload_json(actual, masters, output_plugin_name, Some(signature))
        .ok_or_else(|| "Skyrim fragment record VMAD cannot be decoded".to_string())?;
    let roundtrip = build_vmad_bytes_from_payload(&decoded, masters, output_plugin_name)
        .ok_or_else(|| "Skyrim fragment record VMAD cannot be re-encoded".to_string())?;
    if roundtrip.as_slice() != actual.as_slice()
        || decoded.get("semantic_type").and_then(Value::as_str) != Some(signature)
    {
        return Err("Skyrim fragment record VMAD failed strict receipt verification".to_string());
    }

    let attachment_keys = intents.iter().map(|intent| intent.key.clone()).collect();
    let script_classes = intents
        .iter()
        .map(|intent| intent.script_class.clone())
        .collect();
    let entrypoints = intents
        .iter()
        .flat_map(|intent| intent.entrypoints.iter().cloned())
        .collect();
    let property_names = intents
        .iter()
        .flat_map(|intent| {
            intent
                .properties
                .iter()
                .map(|property| property.name.clone())
        })
        .collect();
    Ok(SkyrimFragmentVmadRecordReceipt {
        owner: intents[0].key.owner.clone(),
        attachment_keys,
        script_classes,
        entrypoints,
        property_names,
        vmad_size: actual.len(),
        vmad_blake3: blake3::hash(actual).to_hex().to_string(),
    })
}

pub fn plan_fragment_binding(
    request: SkyrimFragmentPlanRequest,
) -> Result<SkyrimFragmentBindingPlan, String> {
    validate_request(&request)?;
    let compiler_manifest = psc_manifest_row(&request.target_psc)?;
    Ok(SkyrimFragmentBindingPlan {
        owner: request.owner,
        alias_id: request.alias_id,
        fragment_class: request.fragment_class,
        parent_class: request.fragment_class.parent_class().to_string(),
        source: request.source,
        target_psc: request.target_psc,
        compiler_manifest,
    })
}

pub fn build_fragment_vmad_attachment_intents(
    plans: &[SkyrimFragmentBindingPlan],
    evidence: &[SkyrimPscCompilerEvidence],
) -> Result<Vec<SkyrimFragmentVmadAttachmentIntent>, String> {
    let mut evidence_by_manifest = BTreeMap::new();
    for row in evidence {
        if evidence_by_manifest
            .insert(row.manifest_id.to_ascii_lowercase(), row)
            .is_some()
        {
            return Err(format!(
                "duplicate Skyrim fragment compiler evidence for manifest {}",
                row.manifest_id
            ));
        }
    }

    let mut attached = BTreeSet::new();
    let mut intents = Vec::with_capacity(plans.len());
    for plan in plans {
        let key = SkyrimFragmentVmadAttachmentKey {
            owner: plan.owner.clone(),
            alias_id: plan.alias_id,
            fragment_class: plan.fragment_class,
        };
        if !attached.insert(key.clone()) {
            return Err(format!(
                "duplicate Skyrim {:?} VMAD attachment for {} {}",
                plan.fragment_class, plan.owner.signature, plan.owner.form_key
            ));
        }
        let evidence = evidence_by_manifest
            .get(&plan.compiler_manifest.manifest_id.to_ascii_lowercase())
            .copied()
            .ok_or_else(|| {
                format!(
                    "required Skyrim fragment {} has no compiler evidence",
                    plan.target_psc.class_name
                )
            })?;
        let validated = validate_fragment_compiler_evidence(plan, evidence)?;
        intents.push(SkyrimFragmentVmadAttachmentIntent {
            key,
            script_class: plan.target_psc.class_name.clone(),
            parent_class: plan.parent_class.clone(),
            entrypoints: plan.source.entrypoints.clone(),
            properties: plan.source.properties.clone(),
            compiler_manifest_id: plan.compiler_manifest.manifest_id.clone(),
            compiler_run_id: evidence.compile_run_id.clone(),
            relative_pex_path: evidence.relative_pex_path.clone(),
            pex_blake3: validated,
        });
    }
    Ok(intents)
}

fn validate_materialization_target(
    intents: &[SkyrimFragmentVmadAttachmentIntent],
    target: &Record,
    interner: &StringInterner,
) -> Result<(), String> {
    let first = intents
        .first()
        .ok_or_else(|| "Skyrim fragment materialization has no attachment intents".to_string())?;
    if first.key.fragment_class == SkyrimFragmentClass::Package
        || first.key.owner.signature.eq_ignore_ascii_case("PACK")
    {
        return Err(
            "Skyrim PACK fragment lowering has no proven Fallout 4 target shape".to_string(),
        );
    }
    let record_form_key = target.form_key.format(interner);
    if !target
        .sig
        .as_str()
        .eq_ignore_ascii_case(&first.key.owner.signature)
        || !record_form_key.eq_ignore_ascii_case(&first.key.owner.form_key)
    {
        return Err(format!(
            "Skyrim fragment target record {} {} does not match intent owner {} {}",
            target.sig.as_str(),
            record_form_key,
            first.key.owner.signature,
            first.key.owner.form_key
        ));
    }

    let mut keys = BTreeSet::new();
    let mut alias_ids = BTreeSet::new();
    for intent in intents {
        validate_materialization_intent(intent)?;
        if intent.key.owner != first.key.owner {
            return Err("Skyrim fragment VMAD batch contains multiple target owners".to_string());
        }
        if !keys.insert(intent.key.clone()) {
            return Err(format!(
                "duplicate Skyrim {:?} VMAD attachment for alias {:?}",
                intent.key.fragment_class, intent.key.alias_id
            ));
        }
        if let Some(alias_id) = intent.key.alias_id
            && !alias_ids.insert(alias_id)
        {
            return Err(format!(
                "duplicate Skyrim QUST VMAD attachment for alias {alias_id}"
            ));
        }
    }
    Ok(())
}

fn validate_materialization_intent(
    intent: &SkyrimFragmentVmadAttachmentIntent,
) -> Result<(), String> {
    let fragment_class = intent.key.fragment_class;
    if fragment_class == SkyrimFragmentClass::Package {
        return Err(
            "Skyrim PACK fragment lowering has no proven Fallout 4 target shape".to_string(),
        );
    }
    if !intent
        .key
        .owner
        .signature
        .eq_ignore_ascii_case(fragment_class.owner_signature())
        || !intent
            .parent_class
            .eq_ignore_ascii_case(fragment_class.parent_class())
    {
        return Err(format!(
            "Skyrim {:?} materialization has a signature/class/parent mismatch",
            fragment_class
        ));
    }
    let is_alias = matches!(
        fragment_class,
        SkyrimFragmentClass::ReferenceAlias | SkyrimFragmentClass::LocationAlias
    );
    if is_alias != intent.key.alias_id.is_some() {
        return Err(format!(
            "Skyrim {:?} materialization has an invalid alias owner identity",
            fragment_class
        ));
    }
    validate_class_name(&intent.script_class, "attachment")?;
    validate_entrypoint_shape(fragment_class, &intent.entrypoints)?;
    validate_properties(&intent.properties)?;
    if intent.compiler_manifest_id.trim().is_empty()
        || intent.compiler_run_id.trim().is_empty()
        || !is_blake3(&intent.pex_blake3)
    {
        return Err(format!(
            "Skyrim fragment {} has incomplete compiler evidence",
            intent.script_class
        ));
    }
    let expected_pex = format!("data/Scripts/{}.pex", intent.script_class);
    if normalize_path(&intent.relative_pex_path) != normalize_path(&expected_pex) {
        return Err(format!(
            "Skyrim fragment {} compiler evidence identifies another PEX",
            intent.script_class
        ));
    }
    Ok(())
}

fn synthesize_fragment_vmad_payload(
    intents: &[SkyrimFragmentVmadAttachmentIntent],
) -> Result<Value, String> {
    let first = intents
        .first()
        .ok_or_else(|| "Skyrim fragment materialization has no attachment intents".to_string())?;
    match first.key.owner.signature.as_str() {
        "QUST" => synthesize_quest_fragment_vmad_payload(intents),
        "INFO" => {
            let intent = require_single_intent(intents, SkyrimFragmentClass::TopicInfo)?;
            let phases = intent
                .entrypoints
                .iter()
                .map(|entrypoint| match entrypoint.as_str() {
                    "Fragment_Begin" => Ok(InfoFragmentPhase::Begin),
                    "Fragment_End" => Ok(InfoFragmentPhase::End),
                    _ => Err(format!(
                        "unsupported Skyrim INFO fragment entrypoint {entrypoint}"
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            synthesize_topic_info_vmad_with_properties(
                &intent.script_class,
                &phases,
                &materialize_properties(&intent.properties)?,
                &compiled_evidence(intent),
            )
            .map_err(|error| error.to_string())
        }
        "SCEN" => {
            let intent = require_single_intent(intents, SkyrimFragmentClass::Scene)?;
            let mut payload =
                synthesize_scene_vmad(&intent.script_class, 0, &compiled_evidence(intent))
                    .map_err(|error| error.to_string())?;
            let properties = materialize_properties(&intent.properties)?
                .iter()
                .map(property_payload)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?;
            payload["Script Fragments"]["Script"]["Properties"] = Value::Array(properties);
            payload["Script Fragments"]["Phase Fragments"] = Value::Array(
                intent
                    .entrypoints
                    .iter()
                    .map(|entrypoint| scene_phase_payload(&intent.script_class, entrypoint))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            Ok(payload)
        }
        "PACK" => {
            Err("Skyrim PACK fragment lowering has no proven Fallout 4 target shape".to_string())
        }
        signature => Err(format!(
            "unsupported Skyrim fragment VMAD owner signature {signature}"
        )),
    }
}

fn require_single_intent(
    intents: &[SkyrimFragmentVmadAttachmentIntent],
    expected: SkyrimFragmentClass,
) -> Result<&SkyrimFragmentVmadAttachmentIntent, String> {
    if intents.len() != 1 || intents[0].key.fragment_class != expected {
        return Err(format!(
            "Skyrim {} VMAD requires exactly one {:?} attachment intent",
            expected.owner_signature(),
            expected
        ));
    }
    Ok(&intents[0])
}

fn synthesize_quest_fragment_vmad_payload(
    intents: &[SkyrimFragmentVmadAttachmentIntent],
) -> Result<Value, String> {
    if intents.iter().any(|intent| {
        !matches!(
            intent.key.fragment_class,
            SkyrimFragmentClass::QuestStage
                | SkyrimFragmentClass::ReferenceAlias
                | SkyrimFragmentClass::LocationAlias
        )
    }) {
        return Err("Skyrim QUST VMAD batch contains a non-QUST fragment class".to_string());
    }
    let stage_intents = intents
        .iter()
        .filter(|intent| intent.key.fragment_class == SkyrimFragmentClass::QuestStage)
        .collect::<Vec<_>>();
    if stage_intents.len() > 1 {
        return Err("Skyrim QUST VMAD has multiple quest-stage fragment owners".to_string());
    }
    let stage = stage_intents.first().copied();
    let fragments = stage
        .into_iter()
        .flat_map(|intent| intent.entrypoints.iter())
        .map(|entrypoint| parse_quest_stage_entrypoint(entrypoint))
        .collect::<Result<Vec<_>, _>>()?;
    let aliases = intents
        .iter()
        .filter(|intent| {
            matches!(
                intent.key.fragment_class,
                SkyrimFragmentClass::ReferenceAlias | SkyrimFragmentClass::LocationAlias
            )
        })
        .map(|intent| {
            let alias_id = i32::try_from(intent.key.alias_id.expect("validated alias identity"))
                .map_err(|_| format!("Skyrim QUST alias id is out of range for VMAD"))?;
            Ok(QuestAliasBinding {
                alias_id,
                target_quest_form_key: intent.key.owner.form_key.clone(),
                target_form_key: intent.key.owner.form_key.clone(),
                scripts: vec![QuestAliasScriptBinding {
                    script_class_name: intent.script_class.clone(),
                    properties: materialize_properties(&intent.properties)?,
                }],
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let evidence_intent = stage.unwrap_or(&intents[0]);
    let mut payload = synthesize_quest_vmad_with_properties(
        stage.map_or(&evidence_intent.script_class, |intent| &intent.script_class),
        &fragments,
        &stage
            .map(|intent| materialize_properties(&intent.properties))
            .transpose()?
            .unwrap_or_default(),
        &aliases,
        &compiled_evidence(evidence_intent),
    )
    .map_err(|error| error.to_string())?;
    if stage.is_none() {
        payload["Script Fragments"]["Script"]["ScriptName"] = json!("");
        payload["Script Fragments"]["Script"]["Properties"] = json!([]);
    }
    Ok(payload)
}

fn parse_quest_stage_entrypoint(entrypoint: &str) -> Result<QuestStageFragment, String> {
    let parts = entrypoint.split('_').collect::<Vec<_>>();
    let stage_index = parts
        .get(2)
        .and_then(|value| value.parse::<i32>().ok())
        .ok_or_else(|| format!("invalid Skyrim QUST stage entrypoint {entrypoint}"))?;
    let stage_item_index = parts
        .get(4)
        .and_then(|value| value.parse::<u16>().ok())
        .ok_or_else(|| format!("invalid Skyrim QUST stage entrypoint {entrypoint}"))?;
    Ok(QuestStageFragment {
        stage_index,
        stage_item_index,
        psc_function_name: entrypoint.to_string(),
    })
}

fn scene_phase_payload(script_class: &str, entrypoint: &str) -> Result<Value, String> {
    let parts = entrypoint.split('_').collect::<Vec<_>>();
    let phase_index = parts
        .get(2)
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or_else(|| format!("invalid Skyrim SCEN phase entrypoint {entrypoint}"))?;
    let phase_flag = match parts.get(3).copied() {
        Some("Begin") => 1,
        Some("End") => 2,
        _ => return Err(format!("invalid Skyrim SCEN phase entrypoint {entrypoint}")),
    };
    Ok(json!({
        "Phase Flag": phase_flag,
        "Phase Index": phase_index,
        "Unknown": 0,
        "Unknown1": 0,
        "Unknown2": 0,
        "ScriptName": script_class,
        "FragmentName": entrypoint,
    }))
}

fn compiled_evidence(intent: &SkyrimFragmentVmadAttachmentIntent) -> CompiledScriptEvidence {
    CompiledScriptEvidence {
        class_name: intent.script_class.clone(),
        relative_pex_path: intent.relative_pex_path.clone(),
        compiled_success: true,
    }
}

fn materialize_properties(
    properties: &BTreeSet<SkyrimFragmentPropertyIntent>,
) -> Result<Vec<ScriptProperty>, String> {
    properties
        .iter()
        .map(|property| {
            let normalized_type = property.type_name.to_ascii_lowercase();
            let value = match normalized_type.as_str() {
                "bool" => match property.target_value.to_ascii_lowercase().as_str() {
                    "true" | "1" => Value::Bool(true),
                    "false" | "0" => Value::Bool(false),
                    _ => {
                        return Err(format!(
                            "Skyrim fragment bool property {} has a noncanonical value",
                            property.name
                        ));
                    }
                },
                "int" => Value::Number(
                    property
                        .target_value
                        .parse::<i64>()
                        .map_err(|_| {
                            format!(
                                "Skyrim fragment int property {} has an invalid value",
                                property.name
                            )
                        })?
                        .into(),
                ),
                "float" => {
                    let value = property.target_value.parse::<f64>().map_err(|_| {
                        format!(
                            "Skyrim fragment float property {} has an invalid value",
                            property.name
                        )
                    })?;
                    Value::Number(serde_json::Number::from_f64(value).ok_or_else(|| {
                        format!(
                            "Skyrim fragment float property {} is not finite",
                            property.name
                        )
                    })?)
                }
                "string" => Value::String(property.target_value.clone()),
                "form" | "objectreference" | "actor" | "quest" | "scene" | "package" => {
                    Value::String(property.target_value.clone())
                }
                "referencealias" => {
                    let (alias_id, quest_form_key) = property
                        .target_value
                        .split_once('|')
                        .ok_or_else(|| {
                            format!(
                                "Skyrim fragment ReferenceAlias property {} requires alias_id|quest_form_key",
                                property.name
                            )
                        })?;
                    json!({
                        "alias_id": alias_id.parse::<i32>().map_err(|_| format!(
                            "Skyrim fragment ReferenceAlias property {} has an invalid alias id",
                            property.name
                        ))?,
                        "target_quest_form_key": quest_form_key,
                    })
                }
                "locationalias" => {
                    return Err(format!(
                        "Skyrim fragment LocationAlias property {} has no proven Fallout 4 VMAD lowering",
                        property.name
                    ));
                }
                _ => {
                    return Err(format!(
                        "unsupported Skyrim fragment property {}:{}",
                        property.name, property.type_name
                    ));
                }
            };
            let prop_type = if matches!(normalized_type.as_str(), "scene") {
                "form".to_string()
            } else {
                property.type_name.clone()
            };
            Ok(ScriptProperty {
                name: property.name.clone(),
                prop_type,
                value: Some(value),
            })
        })
        .collect()
}

fn validate_request(request: &SkyrimFragmentPlanRequest) -> Result<(), String> {
    let expected_signature = request.fragment_class.owner_signature();
    if request.owner.signature != expected_signature {
        return Err(format!(
            "Skyrim {:?} fragment requires {expected_signature}, received {}",
            request.fragment_class, request.owner.signature
        ));
    }
    let is_alias = matches!(
        request.fragment_class,
        SkyrimFragmentClass::ReferenceAlias | SkyrimFragmentClass::LocationAlias
    );
    if is_alias != request.alias_id.is_some() {
        return Err(format!(
            "Skyrim {:?} fragment has an invalid alias owner identity",
            request.fragment_class
        ));
    }
    if request.fragment_class == SkyrimFragmentClass::Package {
        return Err(
            "Skyrim PACK fragment lowering has no proven Fallout 4 target shape".to_string(),
        );
    }
    if !request
        .source
        .parent_class
        .eq_ignore_ascii_case(request.fragment_class.parent_class())
    {
        return Err(format!(
            "Skyrim {:?} fragment source parent {} does not match {}",
            request.fragment_class,
            request.source.parent_class,
            request.fragment_class.parent_class()
        ));
    }
    validate_class_name(&request.source.class_name, "source")?;
    validate_provenance(&request.source.psc, "psc")?;
    validate_provenance(&request.source.pex, "pex")?;
    validate_entrypoint_shape(request.fragment_class, &request.source.entrypoints)?;
    validate_properties(&request.source.properties)?;
    validate_api_calls(request.fragment_class, &request.source.api_calls)?;

    if request.target_psc.kind != request.fragment_class.psc_kind() {
        return Err(format!(
            "Skyrim {:?} fragment cannot use {:?} PSC artifact",
            request.fragment_class, request.target_psc.kind
        ));
    }
    validate_class_name(&request.target_psc.class_name, "target")?;
    validate_target_psc(request)?;
    Ok(())
}

fn validate_provenance(
    provenance: &SkyrimFragmentArtifactProvenance,
    extension: &str,
) -> Result<(), String> {
    let path = Path::new(&provenance.relative_path);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || path
            .extension()
            .is_none_or(|value| !value.to_string_lossy().eq_ignore_ascii_case(extension))
    {
        return Err(format!(
            "invalid Skyrim source {extension} provenance path {:?}",
            provenance.relative_path
        ));
    }
    if !is_blake3(&provenance.blake3) {
        return Err(format!(
            "Skyrim source {extension} provenance has no BLAKE3 evidence"
        ));
    }
    Ok(())
}

fn validate_class_name(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 38
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!("invalid Skyrim fragment {label} class {value:?}"));
    }
    Ok(())
}

fn validate_entrypoint_shape(
    fragment_class: SkyrimFragmentClass,
    entrypoints: &BTreeSet<String>,
) -> Result<(), String> {
    if entrypoints.is_empty() {
        return Err("Skyrim fragment has no entrypoint".to_string());
    }
    let valid = match fragment_class {
        SkyrimFragmentClass::QuestStage => entrypoints
            .iter()
            .all(|name| matches_numbered_entrypoint(name, &["Fragment", "Stage"], "Item", 5)),
        SkyrimFragmentClass::TopicInfo => {
            entrypoints.len() <= 2
                && entrypoints
                    .iter()
                    .all(|name| matches!(name.as_str(), "Fragment_Begin" | "Fragment_End"))
        }
        SkyrimFragmentClass::Scene => {
            entrypoints.len() == 1
                && entrypoints.iter().all(|name| {
                    matches_numbered_entrypoint(name, &["Fragment", "Phase"], "Begin", 4)
                        || matches_numbered_entrypoint(name, &["Fragment", "Phase"], "End", 4)
                })
        }
        SkyrimFragmentClass::ReferenceAlias => {
            entrypoints.len() == 1
                && entrypoints
                    .iter()
                    .all(|name| matches!(name.as_str(), "OnInit" | "OnActivate" | "OnDeath"))
        }
        SkyrimFragmentClass::LocationAlias => {
            entrypoints.len() == 1 && entrypoints.iter().all(|name| name == "OnInit")
        }
        SkyrimFragmentClass::Package => false,
    };
    if !valid {
        return Err(format!(
            "unsupported Skyrim {:?} fragment entrypoint shape: {}",
            fragment_class,
            entrypoints.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(())
}

fn matches_numbered_entrypoint(
    value: &str,
    prefix: &[&str],
    suffix: &str,
    part_count: usize,
) -> bool {
    let parts = value.split('_').collect::<Vec<_>>();
    parts.len() == part_count
        && parts.get(0..prefix.len()) == Some(prefix)
        && parts
            .get(prefix.len())
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && parts.get(prefix.len() + 1) == Some(&suffix)
        && (part_count == 4
            || parts.last().is_some_and(|part| {
                !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
            }))
}

fn validate_properties(properties: &BTreeSet<SkyrimFragmentPropertyIntent>) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for property in properties {
        if property.name.is_empty()
            || !property
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            || !names.insert(property.name.to_ascii_lowercase())
        {
            return Err(format!(
                "invalid or duplicate Skyrim fragment property {:?}",
                property.name
            ));
        }
        if !matches!(
            property.type_name.to_ascii_lowercase().as_str(),
            "bool"
                | "int"
                | "float"
                | "string"
                | "form"
                | "objectreference"
                | "actor"
                | "quest"
                | "referencealias"
                | "locationalias"
                | "scene"
                | "package"
        ) || property.target_value.trim().is_empty()
        {
            return Err(format!(
                "unsupported Skyrim fragment property {}:{}",
                property.name, property.type_name
            ));
        }
    }
    Ok(())
}

fn validate_api_calls(
    fragment_class: SkyrimFragmentClass,
    api_calls: &BTreeSet<String>,
) -> Result<(), String> {
    for call in api_calls {
        let supported = match fragment_class {
            SkyrimFragmentClass::QuestStage => matches!(
                call.as_str(),
                "SetObjectiveDisplayed"
                    | "SetObjectiveCompleted"
                    | "SetStage"
                    | "CompleteQuest"
                    | "Stop"
            ),
            SkyrimFragmentClass::TopicInfo
            | SkyrimFragmentClass::Scene
            | SkyrimFragmentClass::ReferenceAlias
            | SkyrimFragmentClass::LocationAlias => call == "GetOwningQuest",
            SkyrimFragmentClass::Package => false,
        };
        if !supported {
            return Err(format!(
                "unsupported Skyrim {:?} fragment API call {call}",
                fragment_class
            ));
        }
    }
    Ok(())
}

fn validate_target_psc(request: &SkyrimFragmentPlanRequest) -> Result<(), String> {
    let header = request
        .target_psc
        .source
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with(';'))
        .ok_or_else(|| "Skyrim target fragment PSC is empty".to_string())?;
    let words = header.split_ascii_whitespace().collect::<Vec<_>>();
    if words.len() < 4
        || !words[0].eq_ignore_ascii_case("ScriptName")
        || words[1] != request.target_psc.class_name
        || !words[2].eq_ignore_ascii_case("Extends")
        || !words[3].eq_ignore_ascii_case(request.fragment_class.parent_class())
    {
        return Err(format!(
            "Skyrim {:?} target PSC has a signature/class/parent mismatch",
            request.fragment_class
        ));
    }
    for entrypoint in &request.source.entrypoints {
        let function = format!("function {}(", entrypoint.to_ascii_lowercase());
        let event = format!("event {}(", entrypoint.to_ascii_lowercase());
        if !request
            .target_psc
            .source
            .lines()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .any(|line| line.starts_with(&function) || line.starts_with(&event))
        {
            return Err(format!(
                "Skyrim target fragment PSC is missing entrypoint {entrypoint}"
            ));
        }
    }
    Ok(())
}

fn validate_fragment_compiler_evidence(
    plan: &SkyrimFragmentBindingPlan,
    evidence: &SkyrimPscCompilerEvidence,
) -> Result<String, String> {
    let manifest = &plan.compiler_manifest;
    if !evidence.compiled_success {
        return Err(format!(
            "Skyrim fragment {} compiler evidence is not successful",
            manifest.class_name
        ));
    }
    if evidence.manifest_id != manifest.manifest_id
        || !evidence
            .class_name
            .eq_ignore_ascii_case(&manifest.class_name)
        || normalize_path(&evidence.relative_source_path)
            != normalize_path(&manifest.relative_source_path)
    {
        return Err(format!(
            "Skyrim fragment {} compiler evidence identifies another artifact",
            manifest.class_name
        ));
    }
    if !evidence
        .source_blake3
        .eq_ignore_ascii_case(&manifest.source_blake3)
    {
        return Err(format!(
            "Skyrim fragment {} compiler evidence is stale",
            manifest.class_name
        ));
    }
    if !evidence.target_game.eq_ignore_ascii_case("fo4")
        || evidence.compile_run_id.trim().is_empty()
        || !is_blake3(&evidence.pex_blake3)
    {
        return Err(format!(
            "Skyrim fragment {} compiler evidence is incomplete",
            manifest.class_name
        ));
    }
    let expected_pex = format!("data/Scripts/{}.pex", manifest.class_name);
    if normalize_path(&evidence.relative_pex_path) != normalize_path(&expected_pex)
        || !evidence.pex_artifact_path.is_absolute()
        || !path_has_suffix(&evidence.pex_artifact_path, &expected_pex)
    {
        return Err(format!(
            "Skyrim fragment {} compiler evidence identifies another PEX",
            manifest.class_name
        ));
    }
    let bytes = std::fs::read(&evidence.pex_artifact_path).map_err(|error| {
        format!(
            "read compiled Skyrim fragment PEX {}: {error}",
            evidence.pex_artifact_path.display()
        )
    })?;
    if bytes.is_empty() {
        return Err(format!(
            "compiled Skyrim fragment PEX {} is empty",
            evidence.pex_artifact_path.display()
        ));
    }
    let actual_hash = blake3::hash(&bytes).to_hex().to_string();
    if !actual_hash.eq_ignore_ascii_case(&evidence.pex_blake3) {
        return Err(format!(
            "compiled Skyrim fragment PEX {} does not match compiler evidence",
            evidence.pex_artifact_path.display()
        ));
    }
    let pex = papyrus_core::pex::parse_pex_bytes(&bytes).map_err(|error| {
        format!(
            "parse compiled Skyrim fragment PEX {}: {error}",
            evidence.pex_artifact_path.display()
        )
    })?;
    if pex.game_id != 2 {
        return Err(format!(
            "compiled Skyrim fragment PEX {} is not a Fallout 4 artifact",
            evidence.pex_artifact_path.display()
        ));
    }
    let objects = pex
        .objects
        .iter()
        .filter(|object| object.name.eq_ignore_ascii_case(&manifest.class_name))
        .collect::<Vec<_>>();
    if objects.len() != 1 || !objects[0].parent.eq_ignore_ascii_case(&plan.parent_class) {
        return Err(format!(
            "compiled Skyrim fragment PEX {} has a signature/class/parent mismatch",
            evidence.pex_artifact_path.display()
        ));
    }
    let object = objects[0];
    let functions = object
        .states
        .iter()
        .flat_map(|state| state.functions.iter())
        .map(|function| (function.name.to_ascii_lowercase(), function))
        .collect::<BTreeMap<_, _>>();
    for entrypoint in &plan.source.entrypoints {
        let function = functions
            .get(&entrypoint.to_ascii_lowercase())
            .ok_or_else(|| {
                format!(
                    "compiled Skyrim fragment PEX {} is missing entrypoint {entrypoint}",
                    evidence.pex_artifact_path.display()
                )
            })?;
        validate_entrypoint_parameters(plan.fragment_class, entrypoint, function)?;
    }
    let properties = object
        .properties
        .iter()
        .map(|property| (property.name.to_ascii_lowercase(), property))
        .collect::<BTreeMap<_, _>>();
    if properties.len() != plan.source.properties.len() {
        return Err(format!(
            "compiled Skyrim fragment PEX {} property evidence does not match the plan",
            evidence.pex_artifact_path.display()
        ));
    }
    for property in &plan.source.properties {
        let compiled = properties
            .get(&property.name.to_ascii_lowercase())
            .ok_or_else(|| {
                format!(
                    "compiled Skyrim fragment PEX {} is missing property {}",
                    evidence.pex_artifact_path.display(),
                    property.name
                )
            })?;
        if !compiled.ty.eq_ignore_ascii_case(&property.type_name) {
            return Err(format!(
                "compiled Skyrim fragment property {} has type {}, expected {}",
                property.name, compiled.ty, property.type_name
            ));
        }
    }
    Ok(actual_hash)
}

fn validate_entrypoint_parameters(
    fragment_class: SkyrimFragmentClass,
    entrypoint: &str,
    function: &papyrus_core::pex::PexFunctionPayload,
) -> Result<(), String> {
    let expected: &[&str] = match fragment_class {
        SkyrimFragmentClass::QuestStage
        | SkyrimFragmentClass::Scene
        | SkyrimFragmentClass::LocationAlias => &[],
        SkyrimFragmentClass::TopicInfo => &["ObjectReference"],
        SkyrimFragmentClass::ReferenceAlias if entrypoint == "OnActivate" => &["ObjectReference"],
        SkyrimFragmentClass::ReferenceAlias if entrypoint == "OnDeath" => &["Actor"],
        SkyrimFragmentClass::ReferenceAlias => &[],
        SkyrimFragmentClass::Package => &["Actor"],
    };
    if function.params.len() != expected.len()
        || function
            .params
            .iter()
            .zip(expected)
            .any(|(actual, expected)| !actual.ty.eq_ignore_ascii_case(expected))
    {
        return Err(format!(
            "compiled Skyrim {:?} fragment entrypoint {entrypoint} has unsupported parameters",
            fragment_class
        ));
    }
    Ok(())
}

fn normalize_path(value: &str) -> String {
    value.trim().replace('\\', "/").to_ascii_lowercase()
}

fn path_has_suffix(path: &Path, suffix: &str) -> bool {
    let actual = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let expected = Path::new(suffix)
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_ascii_lowercase()),
            _ => None,
        })
        .collect::<Vec<_>>();
    actual.ends_with(&expected)
}

fn is_blake3(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};

    fn hash(label: &str) -> String {
        blake3::hash(label.as_bytes()).to_hex().to_string()
    }

    fn source(fragment_class: SkyrimFragmentClass) -> SkyrimFragmentSourceProvenance {
        let entrypoints = match fragment_class {
            SkyrimFragmentClass::QuestStage => ["Fragment_Stage_0010_Item_00"],
            SkyrimFragmentClass::TopicInfo => ["Fragment_End"],
            SkyrimFragmentClass::Scene => ["Fragment_Phase_01_Begin"],
            SkyrimFragmentClass::ReferenceAlias | SkyrimFragmentClass::LocationAlias => ["OnInit"],
            SkyrimFragmentClass::Package => ["Fragment_End"],
        };
        SkyrimFragmentSourceProvenance {
            class_name: "SourceFragment".to_string(),
            parent_class: fragment_class.parent_class().to_string(),
            psc: SkyrimFragmentArtifactProvenance {
                relative_path: "Scripts/Source/SourceFragment.psc".to_string(),
                blake3: hash("source-psc"),
            },
            pex: SkyrimFragmentArtifactProvenance {
                relative_path: "Scripts/SourceFragment.pex".to_string(),
                blake3: hash("source-pex"),
            },
            entrypoints: entrypoints.into_iter().map(str::to_string).collect(),
            properties: BTreeSet::new(),
            api_calls: BTreeSet::new(),
        }
    }

    fn psc_source(fragment_class: SkyrimFragmentClass, class_name: &str) -> String {
        let function = match fragment_class {
            SkyrimFragmentClass::QuestStage => "Function Fragment_Stage_0010_Item_00()",
            SkyrimFragmentClass::TopicInfo => "Function Fragment_End(ObjectReference akSpeakerRef)",
            SkyrimFragmentClass::Scene => "Function Fragment_Phase_01_Begin()",
            SkyrimFragmentClass::ReferenceAlias | SkyrimFragmentClass::LocationAlias => {
                "Event OnInit()"
            }
            SkyrimFragmentClass::Package => "Function Fragment_End(Actor akActor)",
        };
        let end = if function.starts_with("Event ") {
            "EndEvent"
        } else {
            "EndFunction"
        };
        format!(
            "ScriptName {class_name} Extends {}\n{function}\n{end}\n",
            fragment_class.parent_class()
        )
    }

    fn request(fragment_class: SkyrimFragmentClass) -> SkyrimFragmentPlanRequest {
        let class_name = match fragment_class {
            SkyrimFragmentClass::QuestStage => "B21_QF_000001",
            SkyrimFragmentClass::TopicInfo => "B21_TIF_000001",
            SkyrimFragmentClass::Scene => "B21_SF_000001",
            SkyrimFragmentClass::Package => "B21_PF_000001",
            SkyrimFragmentClass::ReferenceAlias => "B21_RefAlias_000001",
            SkyrimFragmentClass::LocationAlias => "B21_LocAlias_000001",
        };
        SkyrimFragmentPlanRequest {
            owner: QuestRecordKey::new(fragment_class.owner_signature(), "000001@Output.esm"),
            alias_id: matches!(
                fragment_class,
                SkyrimFragmentClass::ReferenceAlias | SkyrimFragmentClass::LocationAlias
            )
            .then_some(1),
            fragment_class,
            source: source(fragment_class),
            target_psc: SkyrimPscSourceArtifact {
                class_name: class_name.to_string(),
                source: psc_source(fragment_class, class_name),
                kind: fragment_class.psc_kind(),
                required: true,
            },
        }
    }

    fn compile_evidence(
        root: &Path,
        plan: &SkyrimFragmentBindingPlan,
    ) -> SkyrimPscCompilerEvidence {
        let compiled = papyrus_core::compiler::compile_source(
            &plan.target_psc.source,
            &[],
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(compiled.ok, "diagnostics: {:?}", compiled.diagnostics);
        let bytes = compiled.pex_bytes.expect("real PEX bytes");
        assert!(!bytes.is_empty());
        let path = root
            .join("data")
            .join("Scripts")
            .join(format!("{}.pex", plan.target_psc.class_name));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        SkyrimPscCompilerEvidence {
            manifest_id: plan.compiler_manifest.manifest_id.clone(),
            class_name: plan.target_psc.class_name.clone(),
            relative_source_path: plan.compiler_manifest.relative_source_path.clone(),
            source_blake3: plan.compiler_manifest.source_blake3.clone(),
            relative_pex_path: format!("data/Scripts/{}.pex", plan.target_psc.class_name),
            pex_artifact_path: path,
            pex_blake3: blake3::hash(&bytes).to_hex().to_string(),
            target_game: "fo4".to_string(),
            compile_run_id: "compile-fragment-1".to_string(),
            compiled_success: true,
        }
    }

    fn attachment_intents(
        root: &Path,
        requests: Vec<SkyrimFragmentPlanRequest>,
    ) -> Vec<SkyrimFragmentVmadAttachmentIntent> {
        let plans = requests
            .into_iter()
            .map(|request| plan_fragment_binding(request).unwrap())
            .collect::<Vec<_>>();
        let evidence = plans
            .iter()
            .map(|plan| compile_evidence(root, plan))
            .collect::<Vec<_>>();
        build_fragment_vmad_attachment_intents(&plans, &evidence).unwrap()
    }

    fn target_record(signature: &str, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey::parse("000001@Output.esm", interner).unwrap(),
        )
    }

    fn with_string_property(mut request: SkyrimFragmentPlanRequest) -> SkyrimFragmentPlanRequest {
        request
            .source
            .properties
            .insert(SkyrimFragmentPropertyIntent {
                name: "Greeting".to_string(),
                type_name: "String".to_string(),
                target_value: "hello".to_string(),
                required: true,
            });
        request.target_psc.source =
            request
                .target_psc
                .source
                .replacen('\n', "\nString Property Greeting Auto\n", 1);
        request
    }

    #[test]
    fn plans_exact_supported_shapes_and_rejects_pack() {
        for fragment_class in [
            SkyrimFragmentClass::QuestStage,
            SkyrimFragmentClass::TopicInfo,
            SkyrimFragmentClass::Scene,
            SkyrimFragmentClass::ReferenceAlias,
            SkyrimFragmentClass::LocationAlias,
        ] {
            let plan = plan_fragment_binding(request(fragment_class)).unwrap();
            assert_eq!(plan.fragment_class, fragment_class);
            assert_eq!(plan.parent_class, fragment_class.parent_class());
        }
        assert!(
            plan_fragment_binding(request(SkyrimFragmentClass::Package))
                .unwrap_err()
                .contains("no proven")
        );
    }

    #[test]
    fn rejects_signature_class_properties_and_api_mismatches() {
        let mut wrong_signature = request(SkyrimFragmentClass::TopicInfo);
        wrong_signature.owner.signature = "QUST".to_string();
        assert!(
            plan_fragment_binding(wrong_signature)
                .unwrap_err()
                .contains("requires INFO")
        );

        let mut wrong_parent = request(SkyrimFragmentClass::Scene);
        wrong_parent.source.parent_class = "Quest".to_string();
        assert!(
            plan_fragment_binding(wrong_parent)
                .unwrap_err()
                .contains("source parent")
        );

        let mut unsupported = request(SkyrimFragmentClass::TopicInfo);
        unsupported
            .source
            .properties
            .insert(SkyrimFragmentPropertyIntent {
                name: "Unsupported".to_string(),
                type_name: "Actor[]".to_string(),
                target_value: "[]".to_string(),
                required: true,
            });
        assert!(
            plan_fragment_binding(unsupported)
                .unwrap_err()
                .contains("unsupported Skyrim fragment property")
        );

        let mut unsupported = request(SkyrimFragmentClass::Scene);
        unsupported
            .source
            .api_calls
            .insert("PlayAnimation".to_string());
        assert!(
            plan_fragment_binding(unsupported)
                .unwrap_err()
                .contains("unsupported Skyrim Scene fragment API call")
        );
    }

    #[test]
    fn attachment_requires_real_nonempty_fresh_pex_with_entrypoint() {
        let root = tempfile::tempdir().unwrap();
        let plan = plan_fragment_binding(request(SkyrimFragmentClass::QuestStage)).unwrap();
        assert!(
            build_fragment_vmad_attachment_intents(std::slice::from_ref(&plan), &[])
                .unwrap_err()
                .contains("no compiler evidence")
        );

        let mut evidence = compile_evidence(root.path(), &plan);
        let intents = build_fragment_vmad_attachment_intents(
            std::slice::from_ref(&plan),
            std::slice::from_ref(&evidence),
        )
        .unwrap();
        assert_eq!(intents[0].pex_blake3, evidence.pex_blake3);

        evidence.source_blake3 = hash("stale-target-psc");
        assert!(
            build_fragment_vmad_attachment_intents(&[plan], &[evidence])
                .unwrap_err()
                .contains("stale")
        );
    }

    #[test]
    fn attachment_rejects_missing_entrypoint_and_duplicate_target() {
        let root = tempfile::tempdir().unwrap();
        let plan = plan_fragment_binding(request(SkyrimFragmentClass::QuestStage)).unwrap();
        let mut missing_request = request(SkyrimFragmentClass::QuestStage);
        missing_request.source.entrypoints = ["Fragment_Stage_0020_Item_00".to_string()]
            .into_iter()
            .collect();
        missing_request.target_psc.source =
            missing_request.target_psc.source.replace("0010", "0020");
        let missing_plan = plan_fragment_binding(missing_request).unwrap();
        let evidence = compile_evidence(root.path(), &plan);
        let mut missing_evidence = evidence.clone();
        missing_evidence.manifest_id = missing_plan.compiler_manifest.manifest_id.clone();
        missing_evidence.source_blake3 = missing_plan.compiler_manifest.source_blake3.clone();
        assert!(
            build_fragment_vmad_attachment_intents(&[missing_plan], &[missing_evidence])
                .unwrap_err()
                .contains("missing entrypoint")
        );

        let evidence = compile_evidence(root.path(), &plan);
        assert!(
            build_fragment_vmad_attachment_intents(
                &[plan.clone(), plan],
                std::slice::from_ref(&evidence),
            )
            .unwrap_err()
            .contains("duplicate Skyrim QuestStage VMAD attachment")
        );
    }

    #[test]
    fn info_scene_and_alias_evidence_enforces_parent_and_parameters() {
        for fragment_class in [
            SkyrimFragmentClass::TopicInfo,
            SkyrimFragmentClass::Scene,
            SkyrimFragmentClass::ReferenceAlias,
            SkyrimFragmentClass::LocationAlias,
        ] {
            let root = tempfile::tempdir().unwrap();
            let plan = plan_fragment_binding(request(fragment_class)).unwrap();
            let evidence = compile_evidence(root.path(), &plan);
            let intent = build_fragment_vmad_attachment_intents(&[plan], &[evidence]).unwrap();
            assert_eq!(intent[0].parent_class, fragment_class.parent_class());
        }
    }

    #[test]
    fn materializes_info_vmad_and_verifies_exact_properties_from_record() {
        let root = tempfile::tempdir().unwrap();
        let intents = attachment_intents(
            root.path(),
            vec![with_string_property(request(
                SkyrimFragmentClass::TopicInfo,
            ))],
        );
        let interner = StringInterner::new();
        let mut record = target_record("INFO", &interner);
        let receipt = materialize_fragment_vmad_attachment(
            &intents[0],
            &mut record,
            &interner,
            &[],
            "Output.esm",
        )
        .unwrap();
        assert!(receipt.vmad_size > 6);
        assert_eq!(receipt.property_names, ["Greeting".to_string()].into());
        assert_eq!(receipt.entrypoints, ["Fragment_End".to_string()].into());

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw VMAD bytes")
        };
        let decoded = compact_vmad_payload_json(bytes, &[], "Output.esm", Some("INFO")).unwrap();
        assert_eq!(
            decoded["Script Fragments"]["Script"]["Properties"][0]["Value"],
            "hello"
        );
        assert!(
            materialize_fragment_vmad_attachment(
                &intents[0],
                &mut record,
                &interner,
                &[],
                "Output.esm",
            )
            .unwrap_err()
            .contains("already has a VMAD")
        );
    }

    #[test]
    fn materializes_scene_phase_vmad_without_action_fragment_substitution() {
        let root = tempfile::tempdir().unwrap();
        let intents = attachment_intents(root.path(), vec![request(SkyrimFragmentClass::Scene)]);
        let interner = StringInterner::new();
        let mut record = target_record("SCEN", &interner);
        materialize_fragment_vmad_attachment(
            &intents[0],
            &mut record,
            &interner,
            &[],
            "Output.esm",
        )
        .unwrap();
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw VMAD bytes")
        };
        let decoded = compact_vmad_payload_json(bytes, &[], "Output.esm", Some("SCEN")).unwrap();
        assert_eq!(
            decoded["Script Fragments"]["Fragments"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            decoded["Script Fragments"]["Phase Fragments"][0]["FragmentName"],
            "Fragment_Phase_01_Begin"
        );
        assert_eq!(
            decoded["Script Fragments"]["Phase Fragments"][0]["Phase Flag"],
            1
        );
    }

    #[test]
    fn materializes_distinct_reference_and_location_aliases_on_one_quest() {
        let root = tempfile::tempdir().unwrap();
        let reference = request(SkyrimFragmentClass::ReferenceAlias);
        let mut location = request(SkyrimFragmentClass::LocationAlias);
        location.alias_id = Some(2);
        let intents = attachment_intents(root.path(), vec![reference, location]);
        let interner = StringInterner::new();
        let mut record = target_record("QUST", &interner);
        let receipt = materialize_fragment_vmad_attachments(
            &intents,
            &mut record,
            &interner,
            &[],
            "Output.esm",
        )
        .unwrap();
        assert_eq!(receipt.attachment_keys.len(), 2);
        assert_eq!(receipt.script_classes.len(), 2);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw VMAD bytes")
        };
        let decoded = compact_vmad_payload_json(bytes, &[], "Output.esm", Some("QUST")).unwrap();
        let aliases = decoded["Script Fragments"]["Aliases"].as_array().unwrap();
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0]["Object"]["Alias"], 1);
        assert_eq!(aliases[1]["Object"]["Alias"], 2);
        assert_eq!(decoded["Script Fragments"]["Script"]["ScriptName"], "");
    }

    #[test]
    fn materializes_quest_stage_and_aliases_as_one_signature_correct_vmad() {
        let root = tempfile::tempdir().unwrap();
        let mut reference = request(SkyrimFragmentClass::ReferenceAlias);
        reference.alias_id = Some(7);
        let intents = attachment_intents(
            root.path(),
            vec![request(SkyrimFragmentClass::QuestStage), reference],
        );
        let interner = StringInterner::new();
        let mut record = target_record("QUST", &interner);
        materialize_fragment_vmad_attachments(&intents, &mut record, &interner, &[], "Output.esm")
            .unwrap();
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw VMAD bytes")
        };
        let decoded = compact_vmad_payload_json(bytes, &[], "Output.esm", Some("QUST")).unwrap();
        assert_eq!(
            decoded["Script Fragments"]["Fragments"][0]["FragmentName"],
            "Fragment_Stage_0010_Item_00"
        );
        assert_eq!(
            decoded["Script Fragments"]["Aliases"][0]["Object"]["Alias"],
            7
        );
    }

    #[test]
    fn materialization_rejects_pack_mismatch_missing_entrypoint_and_duplicate_alias() {
        let root = tempfile::tempdir().unwrap();
        let mut intent =
            attachment_intents(root.path(), vec![request(SkyrimFragmentClass::Scene)]).remove(0);
        let interner = StringInterner::new();

        let mut wrong_record = target_record("INFO", &interner);
        assert!(
            materialize_fragment_vmad_attachment(
                &intent,
                &mut wrong_record,
                &interner,
                &[],
                "Output.esm",
            )
            .unwrap_err()
            .contains("does not match intent owner")
        );

        intent.entrypoints.clear();
        let mut scene = target_record("SCEN", &interner);
        assert!(
            materialize_fragment_vmad_attachment(
                &intent,
                &mut scene,
                &interner,
                &[],
                "Output.esm",
            )
            .unwrap_err()
            .contains("no entrypoint")
        );

        let mut class_mismatch =
            attachment_intents(root.path(), vec![request(SkyrimFragmentClass::Scene)]).remove(0);
        class_mismatch.parent_class = "Quest".to_string();
        assert!(
            materialize_fragment_vmad_attachment(
                &class_mismatch,
                &mut scene,
                &interner,
                &[],
                "Output.esm",
            )
            .unwrap_err()
            .contains("signature/class/parent mismatch")
        );

        intent.key.fragment_class = SkyrimFragmentClass::Package;
        intent.key.owner = QuestRecordKey::new("PACK", "000001@Output.esm");
        let mut package = target_record("PACK", &interner);
        assert!(
            materialize_fragment_vmad_attachment(
                &intent,
                &mut package,
                &interner,
                &[],
                "Output.esm",
            )
            .unwrap_err()
            .contains("no proven")
        );

        let mut reference = request(SkyrimFragmentClass::ReferenceAlias);
        reference.alias_id = Some(3);
        let mut location = request(SkyrimFragmentClass::LocationAlias);
        location.alias_id = Some(3);
        let duplicates = attachment_intents(root.path(), vec![reference, location]);
        let mut quest = target_record("QUST", &interner);
        assert!(
            materialize_fragment_vmad_attachments(
                &duplicates,
                &mut quest,
                &interner,
                &[],
                "Output.esm",
            )
            .unwrap_err()
            .contains("alias 3")
        );
    }

    #[test]
    fn strict_receipt_rejects_tampered_record_and_stale_intent_evidence() {
        let root = tempfile::tempdir().unwrap();
        let mut intents =
            attachment_intents(root.path(), vec![request(SkyrimFragmentClass::TopicInfo)]);
        let interner = StringInterner::new();
        let mut record = target_record("INFO", &interner);
        materialize_fragment_vmad_attachment(
            &intents[0],
            &mut record,
            &interner,
            &[],
            "Output.esm",
        )
        .unwrap();
        let FieldValue::Bytes(bytes) = &mut record.fields[0].value else {
            panic!("expected raw VMAD bytes")
        };
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        assert!(
            fragment_vmad_receipt_from_record(&intents, &record, &interner, &[], "Output.esm",)
                .unwrap_err()
                .contains("does not exactly match")
        );

        intents[0].pex_blake3 = "stale".to_string();
        let clean = target_record("INFO", &interner);
        assert!(
            fragment_vmad_receipt_from_record(&intents, &clean, &interner, &[], "Output.esm",)
                .unwrap_err()
                .contains("incomplete compiler evidence")
        );
    }
}
