use std::collections::BTreeSet;

use crate::fnv_legacy_scripting::vmad::{
    CompiledScriptEvidence, QuestStageFragment, synthesize_quest_vmad,
};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::quest_runtime::{
    AliasFillKind, ConditionIntent, LocalizedTextIntent, QuestRecordKey, QuestRuntimeAdmission,
    QuestRuntimeComponentPlan, QuestSourceGame, QuestStartFlag, StartDisposition,
};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use serde_json::Value;

use super::papyrus::{
    GeneratedPscManifestRow, SkyrimPscCompilerEvidence, SkyrimPscKind, SkyrimPscSourceArtifact,
    psc_manifest_row, validate_compiler_evidence,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkyrimQuestFragmentAction {
    SetObjectiveDisplayed { index: u16, displayed: bool },
    SetObjectiveCompleted { index: u16, completed: bool },
    SetStage(u16),
    CompleteQuest,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimQuestFragmentSource {
    pub fragment_id: String,
    pub stage_index: u16,
    pub stage_item_index: u16,
    pub actions: Vec<SkyrimQuestFragmentAction>,
}

#[derive(Debug, Clone)]
pub struct SkyrimQuestProjection {
    pub component_id: String,
    pub source_quest: QuestRecordKey,
    pub target_quest: QuestRecordKey,
    pub record: Record,
    pub localized_text: Vec<LocalizedTextIntent>,
    pub psc_artifact: SkyrimPscSourceArtifact,
    pub psc_manifest: GeneratedPscManifestRow,
    pub pending_vmad: SkyrimPendingQuestVmad,
}

#[derive(Debug, Clone)]
pub struct SkyrimPendingQuestVmad {
    pub owner: QuestRecordKey,
    pub script_class: String,
    pub fragment: QuestStageFragment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimQuestVmadAttachmentIntent {
    pub owner: QuestRecordKey,
    pub script_class: String,
    pub compiler_evidence_id: String,
    pub relative_pex_path: String,
    pub pex_blake3: String,
    pub payload: Value,
}

pub fn minimal_quest_psc_artifact(
    class_prefix: &str,
    target_local: u32,
    fragment: &SkyrimQuestFragmentSource,
) -> Result<(SkyrimPscSourceArtifact, GeneratedPscManifestRow), String> {
    let class_name = fragment_class_name(class_prefix, target_local)?;
    let artifact = SkyrimPscSourceArtifact {
        class_name: class_name.clone(),
        source: build_fragment_psc(
            &class_name,
            fragment.stage_index,
            fragment.stage_item_index,
            &fragment.actions,
        ),
        kind: SkyrimPscKind::QuestFragment,
        required: true,
    };
    let manifest = psc_manifest_row(&artifact)?;
    Ok((artifact, manifest))
}

pub fn project_minimal_quest(
    plan: &QuestRuntimeComponentPlan,
    editor_id: &str,
    priority: u8,
    title: LocalizedTextIntent,
    fragment: SkyrimQuestFragmentSource,
    class_prefix: &str,
    interner: &StringInterner,
) -> Result<SkyrimQuestProjection, String> {
    validate_minimal_plan(plan, editor_id, &title, &fragment)?;
    let target_quest = mapped_root(plan)?;
    let target_form_key = parse_form_key(&target_quest.form_key, interner)?;
    let (psc_artifact, psc_manifest) =
        minimal_quest_psc_artifact(class_prefix, target_form_key.local, &fragment)?;
    let class_name = psc_artifact.class_name.clone();
    validate_declared_runtime_intents(plan, &target_quest, &psc_manifest, &fragment)?;

    let mut record = Record::new(SigCode(*b"QUST"), target_form_key);
    record.eid = Some(interner.intern(editor_id));
    let mut localized_text = vec![title.clone()];
    push_field(
        &mut record,
        "FULL",
        FieldValue::String(interner.intern(&title.text)),
    )?;
    push_field(&mut record, "DNAM", general_data(plan, priority, interner))?;
    for condition in &plan.semantics.conditions {
        push_condition(&mut record, condition, plan, interner)?;
    }

    for stage in &plan.semantics.stages {
        push_field(
            &mut record,
            "INDX",
            FieldValue::Struct(vec![
                (
                    interner.intern("stage_index"),
                    FieldValue::Uint(stage.index.into()),
                ),
                (interner.intern("flags"), FieldValue::Uint(0)),
                (interner.intern("unknown"), FieldValue::Uint(0)),
            ]),
        )?;
        for log_entry in &stage.log_entries {
            require_localized_field(log_entry, "CNAM", "DLSTRINGS")?;
            push_field(&mut record, "QSDT", FieldValue::Uint(0))?;
            push_field(
                &mut record,
                "CNAM",
                FieldValue::String(interner.intern(&log_entry.text)),
            )?;
            localized_text.push(log_entry.clone());
        }
    }

    for objective in &plan.semantics.objectives {
        require_localized_field(&objective.display_text, "NNAM", "STRINGS")?;
        push_field(
            &mut record,
            "QOBJ",
            FieldValue::Uint(objective.index.into()),
        )?;
        push_field(&mut record, "FNAM", FieldValue::Uint(0))?;
        push_field(
            &mut record,
            "NNAM",
            FieldValue::String(interner.intern(&objective.display_text.text)),
        )?;
        localized_text.push(objective.display_text.clone());
        for alias in &objective.target_aliases {
            push_field(
                &mut record,
                "QSTA",
                FieldValue::Struct(vec![
                    (interner.intern("alias"), FieldValue::Int(i64::from(*alias))),
                    (interner.intern("flags"), FieldValue::Uint(0)),
                    (interner.intern("keyword"), FieldValue::Uint(0)),
                ]),
            )?;
        }
    }

    push_field(
        &mut record,
        "ANAM",
        FieldValue::Uint(plan.semantics.aliases.len() as u64),
    )?;
    for alias in &plan.semantics.aliases {
        let AliasFillKind::ForcedReference(source_reference) = &alias.fill else {
            return Err("minimal Skyrim QUST requires a forced-reference alias".to_string());
        };
        let target_reference = mapped_form_key(plan, source_reference)?;
        push_field(&mut record, "ALST", FieldValue::Uint(alias.id.into()))?;
        push_field(
            &mut record,
            "ALID",
            FieldValue::String(interner.intern(&alias.name)),
        )?;
        push_field(&mut record, "FNAM", FieldValue::Uint(0))?;
        push_field(
            &mut record,
            "ALFR",
            FieldValue::FormKey(parse_form_key(&target_reference, interner)?),
        )?;
        for condition in &alias.conditions {
            push_condition(&mut record, condition, plan, interner)?;
        }
        push_field(&mut record, "ALED", FieldValue::None)?;
    }

    Ok(SkyrimQuestProjection {
        component_id: plan.component_id.clone(),
        source_quest: plan.root_quest.clone(),
        target_quest: target_quest.clone(),
        record,
        localized_text,
        psc_artifact,
        psc_manifest,
        pending_vmad: SkyrimPendingQuestVmad {
            owner: target_quest,
            script_class: class_name.clone(),
            fragment: QuestStageFragment {
                stage_index: i32::from(fragment.stage_index),
                stage_item_index: fragment.stage_item_index,
                psc_function_name: fragment_entrypoint(
                    fragment.stage_index,
                    fragment.stage_item_index,
                ),
            },
        },
    })
}

pub fn build_vmad_attachment_intent(
    projection: &SkyrimQuestProjection,
    evidence: Option<&SkyrimPscCompilerEvidence>,
) -> Result<SkyrimQuestVmadAttachmentIntent, String> {
    let validated = validate_compiler_evidence(&projection.psc_manifest, evidence)?;
    if !validated.entrypoints.contains(
        &projection
            .pending_vmad
            .fragment
            .psc_function_name
            .to_ascii_lowercase(),
    ) {
        return Err(format!(
            "compiled Skyrim PEX {} does not contain fragment entrypoint {}",
            validated.relative_pex_path, projection.pending_vmad.fragment.psc_function_name
        ));
    }
    let compiled = CompiledScriptEvidence {
        class_name: validated.class_name.clone(),
        relative_pex_path: validated.relative_pex_path.clone(),
        compiled_success: true,
    };
    let payload = synthesize_quest_vmad(
        &projection.pending_vmad.script_class,
        std::slice::from_ref(&projection.pending_vmad.fragment),
        &[],
        &compiled,
    )
    .map_err(|error| error.to_string())?;
    Ok(SkyrimQuestVmadAttachmentIntent {
        owner: projection.pending_vmad.owner.clone(),
        script_class: projection.pending_vmad.script_class.clone(),
        compiler_evidence_id: validated.compile_run_id,
        relative_pex_path: validated.relative_pex_path,
        pex_blake3: validated.pex_blake3,
        payload,
    })
}

fn validate_minimal_plan(
    plan: &QuestRuntimeComponentPlan,
    editor_id: &str,
    title: &LocalizedTextIntent,
    fragment: &SkyrimQuestFragmentSource,
) -> Result<(), String> {
    if plan.provenance.game != QuestSourceGame::SkyrimSe {
        return Err("minimal Skyrim QUST projector received another source game".to_string());
    }
    if !matches!(plan.admission, QuestRuntimeAdmission::Supported) {
        return Err("minimal Skyrim QUST component is not admitted".to_string());
    }
    if !plan.validation_issues().is_empty() {
        return Err("minimal Skyrim QUST component contract is invalid".to_string());
    }
    if editor_id.is_empty()
        || editor_id.len() > 128
        || !editor_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("minimal Skyrim QUST requires a valid EditorID".to_string());
    }
    require_localized_field(title, "FULL", "STRINGS")?;
    if !matches!(plan.start_disposition, Some(StartDisposition::Autostart))
        || !plan
            .semantics
            .start_flags
            .contains(&QuestStartFlag::StartGameEnabled)
    {
        return Err("minimal Skyrim QUST requires a proven autostart disposition".to_string());
    }
    if plan
        .owned_records
        .iter()
        .any(|record| record.signature != "QUST")
    {
        return Err(
            "minimal Skyrim QUST cannot own dialogue, SCEN, PACK, or Story Manager records"
                .to_string(),
        );
    }
    if plan.semantics.stages.is_empty()
        || plan.semantics.objectives.is_empty()
        || plan.semantics.aliases.len() != 1
        || plan.scripts.len() != 1
        || plan.fragments.len() != 1
        || plan.psc.len() != 1
        || plan.vmad.len() != 1
    {
        return Err("minimal Skyrim QUST component has an unsupported required shape".to_string());
    }
    if plan
        .semantics
        .stages
        .iter()
        .all(|stage| stage.log_entries.is_empty())
    {
        return Err("minimal Skyrim QUST requires a localized stage log entry".to_string());
    }
    let alias_ids = plan
        .semantics
        .aliases
        .iter()
        .map(|alias| alias.id)
        .collect::<BTreeSet<_>>();
    if plan
        .semantics
        .objectives
        .iter()
        .flat_map(|objective| &objective.target_aliases)
        .any(|alias| !alias_ids.contains(alias))
    {
        return Err("minimal Skyrim QUST objective references an unknown alias".to_string());
    }
    if fragment.actions.is_empty() {
        return Err("minimal Skyrim QUST fragment has no actions".to_string());
    }
    let stage = plan
        .semantics
        .stages
        .iter()
        .find(|stage| stage.index == fragment.stage_index)
        .ok_or("minimal Skyrim QUST fragment stage is missing")?;
    if !stage.fragment_ids.contains(&fragment.fragment_id) {
        return Err("minimal Skyrim QUST fragment is not owned by its stage".to_string());
    }
    if usize::from(fragment.stage_item_index) >= stage.log_entries.len() {
        return Err("minimal Skyrim QUST fragment stage-item index is missing".to_string());
    }
    validate_fragment_actions(plan, fragment)?;
    for condition in plan.semantics.conditions.iter().chain(
        plan.semantics
            .aliases
            .iter()
            .flat_map(|alias| &alias.conditions),
    ) {
        validate_condition(condition)?;
    }
    Ok(())
}

fn validate_declared_runtime_intents(
    plan: &QuestRuntimeComponentPlan,
    target_quest: &QuestRecordKey,
    manifest: &GeneratedPscManifestRow,
    fragment: &SkyrimQuestFragmentSource,
) -> Result<(), String> {
    let class_name = &manifest.class_name;
    let psc = plan.psc.iter().next().expect("minimal shape checked");
    let expected_source = format!("Scripts/Source/User/{class_name}.psc");
    let expected_evidence = psc.compiler_evidence.as_ref();
    if !psc.compiler_evidence_required
        || !psc.class_name.eq_ignore_ascii_case(class_name)
        || !psc.source_artifact.eq_ignore_ascii_case(&expected_source)
        || expected_evidence.is_none_or(|evidence| {
            evidence.manifest_id != manifest.manifest_id
                || !evidence
                    .source_digest
                    .eq_ignore_ascii_case(&manifest.source_blake3)
                || !evidence.require_fresh_output
        })
    {
        return Err(
            "minimal Skyrim QUST PSC intent does not match deterministic output".to_string(),
        );
    }
    let script = plan.scripts.iter().next().expect("minimal shape checked");
    if script.owner != plan.root_quest
        || !script.target_class.eq_ignore_ascii_case(class_name)
        || !script.required
    {
        return Err("minimal Skyrim QUST script intent does not match its fragment".to_string());
    }
    let vmad = plan.vmad.iter().next().expect("minimal shape checked");
    let vmad_evidence = vmad.compiler_evidence.as_ref();
    if vmad.owner != *target_quest
        || !vmad.script_class.eq_ignore_ascii_case(class_name)
        || !vmad.compiler_evidence_required
        || vmad_evidence != expected_evidence
        || !vmad.properties.is_empty()
    {
        return Err(
            "minimal Skyrim QUST VMAD intent does not match its compiled fragment".to_string(),
        );
    }
    let declared_fragment = plan.fragments.iter().next().expect("minimal shape checked");
    if declared_fragment.fragment_id != fragment.fragment_id
        || declared_fragment.owner != plan.root_quest
        || !declared_fragment
            .target_class
            .eq_ignore_ascii_case(class_name)
        || declared_fragment.entrypoint
            != fragment_entrypoint(fragment.stage_index, fragment.stage_item_index)
    {
        return Err(
            "minimal Skyrim QUST fragment intent does not match its source stage".to_string(),
        );
    }
    Ok(())
}

fn validate_fragment_actions(
    plan: &QuestRuntimeComponentPlan,
    fragment: &SkyrimQuestFragmentSource,
) -> Result<(), String> {
    let objectives = plan
        .semantics
        .objectives
        .iter()
        .map(|objective| objective.index)
        .collect::<BTreeSet<_>>();
    let stages = plan
        .semantics
        .stages
        .iter()
        .map(|stage| stage.index)
        .collect::<BTreeSet<_>>();
    for action in &fragment.actions {
        match action {
            SkyrimQuestFragmentAction::SetObjectiveDisplayed { index, .. }
            | SkyrimQuestFragmentAction::SetObjectiveCompleted { index, .. }
                if !objectives.contains(index) =>
            {
                return Err(format!("fragment references missing objective {index}"));
            }
            SkyrimQuestFragmentAction::SetStage(index) if !stages.contains(index) => {
                return Err(format!("fragment references missing stage {index}"));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_condition(condition: &ConditionIntent) -> Result<(), String> {
    if !matches!(condition.function.as_str(), "GetIsID" | "GetStageDone")
        || !matches!(condition.operator.as_str(), "==" | "EqualTo")
        || condition.comparison_value != "1"
        || condition.run_on != "Subject"
        || condition.cis1.is_some()
        || condition.cis2.is_some()
    {
        return Err(format!(
            "unsupported required Skyrim quest condition {}",
            condition.function
        ));
    }
    let expected_parameters = if condition.function == "GetIsID" {
        1
    } else {
        2
    };
    if condition.parameters.len() != expected_parameters {
        return Err(format!(
            "Skyrim quest condition {} has an unsupported parameter shape",
            condition.function
        ));
    }
    if condition.function == "GetStageDone" && condition.parameters[1].parse::<u16>().is_err() {
        return Err("Skyrim GetStageDone has an invalid stage parameter".to_string());
    }
    Ok(())
}

fn push_condition(
    record: &mut Record,
    condition: &ConditionIntent,
    plan: &QuestRuntimeComponentPlan,
    interner: &StringInterner,
) -> Result<(), String> {
    validate_condition(condition)?;
    let target = mapped_form_key(plan, &condition.parameters[0])?;
    let function = if condition.function == "GetIsID" {
        72
    } else {
        59
    };
    let parameter_2 = condition
        .parameters
        .get(1)
        .map(|value| value.parse::<u16>())
        .transpose()
        .map_err(|_| "Skyrim quest condition stage is invalid")?
        .unwrap_or(0);
    push_field(
        record,
        "CTDA",
        FieldValue::Struct(vec![
            (interner.intern("type"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_1"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
            (interner.intern("comparison_value"), FieldValue::Float(1.0)),
            (interner.intern("function"), FieldValue::Uint(function)),
            (interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_7"), FieldValue::Uint(0)),
            (
                interner.intern("parameter_1"),
                FieldValue::FormKey(parse_form_key(&target, interner)?),
            ),
            (
                interner.intern("parameter_2"),
                FieldValue::Uint(parameter_2.into()),
            ),
            (interner.intern("run_on"), FieldValue::Uint(0)),
            (interner.intern("reference"), FieldValue::Uint(0)),
            (interner.intern("parameter_3"), FieldValue::Int(-1)),
        ]),
    )
}

fn general_data(
    plan: &QuestRuntimeComponentPlan,
    priority: u8,
    interner: &StringInterner,
) -> FieldValue {
    let mut flags = 1_u64;
    if plan
        .semantics
        .start_flags
        .contains(&QuestStartFlag::RunOnce)
    {
        flags |= 8;
    }
    FieldValue::Struct(vec![
        (interner.intern("flags"), FieldValue::Uint(flags)),
        (
            interner.intern("priority"),
            FieldValue::Uint(priority.into()),
        ),
        (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
        (interner.intern("delay_time"), FieldValue::Float(0.0)),
        (interner.intern("type"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_5"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_7"), FieldValue::Uint(0)),
    ])
}

fn mapped_root(plan: &QuestRuntimeComponentPlan) -> Result<QuestRecordKey, String> {
    let mappings = plan
        .mappings
        .iter()
        .filter(|mapping| mapping.source == plan.root_quest)
        .collect::<Vec<_>>();
    match mappings.as_slice() {
        [mapping] if mapping.target.signature == "QUST" => Ok(mapping.target.clone()),
        _ => Err("minimal Skyrim QUST requires exactly one target QUST mapping".to_string()),
    }
}

fn mapped_form_key(plan: &QuestRuntimeComponentPlan, source: &str) -> Result<String, String> {
    let matching = plan
        .mappings
        .iter()
        .filter(|mapping| mapping.source.form_key.eq_ignore_ascii_case(source))
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [mapping] => Ok(mapping.target.form_key.clone()),
        [] if plan
            .mappings
            .iter()
            .filter(|mapping| mapping.target.form_key.eq_ignore_ascii_case(source))
            .count()
            == 1 =>
        {
            Ok(source.to_string())
        }
        _ => Err(format!(
            "required Skyrim quest FormKey {source} is not mapped exactly once"
        )),
    }
}

fn parse_form_key(value: &str, interner: &StringInterner) -> Result<FormKey, String> {
    if value.contains('@') {
        return FormKey::parse(value, interner);
    }
    let (local, plugin) = value
        .split_once(':')
        .ok_or_else(|| format!("invalid quest FormKey {value}"))?;
    FormKey::parse(&format!("{local}@{plugin}"), interner)
}

fn require_localized_field(
    intent: &LocalizedTextIntent,
    field: &str,
    table: &str,
) -> Result<(), String> {
    if intent.field != field || intent.target_table != table || intent.text.trim().is_empty() {
        return Err(format!(
            "Skyrim quest localized {field} must target {table} and contain text"
        ));
    }
    Ok(())
}

fn fragment_class_name(prefix: &str, target_local: u32) -> Result<String, String> {
    if prefix.is_empty()
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("Skyrim quest fragment prefix is invalid".to_string());
    }
    let class_name = format!("{prefix}_SkyQF_{target_local:06X}");
    if class_name.len() > 38 {
        return Err(format!(
            "Skyrim quest fragment class {class_name} exceeds 38 characters"
        ));
    }
    Ok(class_name)
}

fn fragment_entrypoint(stage_index: u16, stage_item_index: u16) -> String {
    format!("Fragment_Stage_{stage_index:04}_Item_{stage_item_index:02}")
}

fn build_fragment_psc(
    class_name: &str,
    stage_index: u16,
    stage_item_index: u16,
    actions: &[SkyrimQuestFragmentAction],
) -> String {
    let mut source = format!("ScriptName {class_name} Extends Quest\n\n");
    source.push_str(&format!(
        "Function {}()\n",
        fragment_entrypoint(stage_index, stage_item_index)
    ));
    for action in actions {
        let line = match action {
            SkyrimQuestFragmentAction::SetObjectiveDisplayed { index, displayed } => {
                format!(
                    "    SetObjectiveDisplayed({index}, {})\n",
                    papyrus_bool(*displayed)
                )
            }
            SkyrimQuestFragmentAction::SetObjectiveCompleted { index, completed } => {
                format!(
                    "    SetObjectiveCompleted({index}, {})\n",
                    papyrus_bool(*completed)
                )
            }
            SkyrimQuestFragmentAction::SetStage(index) => format!("    SetStage({index})\n"),
            SkyrimQuestFragmentAction::CompleteQuest => "    CompleteQuest()\n".to_string(),
            SkyrimQuestFragmentAction::Stop => "    Stop()\n".to_string(),
        };
        source.push_str(&line);
    }
    source.push_str("EndFunction\n");
    source
}

fn papyrus_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn push_field(record: &mut Record, signature: &str, value: FieldValue) -> Result<(), String> {
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str(signature)?,
        value,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::quest_runtime::{
        CompilerEvidenceIntent, DependencyClosure, FragmentIntent, PscIntent, QuestAliasIntent,
        QuestObjectiveIntent, QuestRuntimeExpectedReceipt, QuestSemanticPlan, QuestStageIntent,
        ScriptIntent, SourceProvenance, SourceTargetFormMapping, VmadIntent,
    };

    use super::*;

    fn fixture_plan() -> QuestRuntimeComponentPlan {
        let source = QuestRecordKey::new("QUST", "000123@Skyrim.esm");
        let target = QuestRecordKey::new("QUST", "010123@Output.esm");
        let source_ref = QuestRecordKey::new("REFR", "000456@Skyrim.esm");
        let target_ref = QuestRecordKey::new("REFR", "010456@Output.esm");
        let class_name = "B21_SkyQF_010123";
        let artifact = SkyrimPscSourceArtifact {
            class_name: class_name.to_string(),
            source: build_fragment_psc(
                class_name,
                10,
                0,
                &[SkyrimQuestFragmentAction::SetObjectiveDisplayed {
                    index: 10,
                    displayed: true,
                }],
            ),
            kind: SkyrimPscKind::QuestFragment,
            required: true,
        };
        let manifest = psc_manifest_row(&artifact).unwrap();
        let compiler_evidence = CompilerEvidenceIntent {
            manifest_id: manifest.manifest_id,
            source_digest: manifest.source_blake3,
            require_fresh_output: true,
        };
        QuestRuntimeComponentPlan {
            component_id: "skyrim:000123".to_string(),
            root_quest: source.clone(),
            provenance: SourceProvenance {
                game: QuestSourceGame::SkyrimSe,
                source_plugin: "Skyrim.esm".to_string(),
                graft: None,
            },
            owned_records: vec![source.clone()],
            shared_records: BTreeSet::from([source_ref.clone()]),
            dependencies: DependencyClosure {
                direct: BTreeSet::from([source_ref.clone()]),
                recursive: BTreeSet::from([source_ref.clone()]),
            },
            source_topology: BTreeSet::new(),
            target_topology: BTreeSet::new(),
            mappings: vec![
                SourceTargetFormMapping {
                    source: source.clone(),
                    target: target.clone(),
                },
                SourceTargetFormMapping {
                    source: source_ref,
                    target: target_ref,
                },
            ],
            semantics: QuestSemanticPlan {
                stages: vec![QuestStageIntent {
                    index: 10,
                    log_entries: vec![LocalizedTextIntent {
                        field: "CNAM".to_string(),
                        source_text_id: Some(11),
                        text: "The objective has begun.".to_string(),
                        target_table: "DLSTRINGS".to_string(),
                    }],
                    fragment_ids: BTreeSet::from(["stage-10-item-0".to_string()]),
                }],
                objectives: vec![QuestObjectiveIntent {
                    index: 10,
                    display_text: LocalizedTextIntent {
                        field: "NNAM".to_string(),
                        source_text_id: Some(12),
                        text: "Reach the marker.".to_string(),
                        target_table: "STRINGS".to_string(),
                    },
                    target_aliases: BTreeSet::from([0]),
                }],
                aliases: vec![QuestAliasIntent {
                    id: 0,
                    name: "Marker".to_string(),
                    fill: AliasFillKind::ForcedReference("000456@Skyrim.esm".to_string()),
                    conditions: Vec::new(),
                }],
                conditions: vec![ConditionIntent {
                    function: "GetStageDone".to_string(),
                    operator: "==".to_string(),
                    comparison_value: "1".to_string(),
                    parameters: vec!["000123@Skyrim.esm".to_string(), "10".to_string()],
                    run_on: "Subject".to_string(),
                    cis1: None,
                    cis2: None,
                }],
                start_flags: BTreeSet::from([
                    QuestStartFlag::StartGameEnabled,
                    QuestStartFlag::RunOnce,
                ]),
            },
            scripts: BTreeSet::from([ScriptIntent {
                owner: source.clone(),
                source_name: "QF_Source_000123".to_string(),
                target_class: class_name.to_string(),
                required: true,
            }]),
            fragments: BTreeSet::from([FragmentIntent {
                owner: source.clone(),
                fragment_id: "stage-10-item-0".to_string(),
                entrypoint: "Fragment_Stage_0010_Item_00".to_string(),
                target_class: class_name.to_string(),
            }]),
            psc: BTreeSet::from([PscIntent {
                class_name: class_name.to_string(),
                source_artifact: format!("Scripts/Source/User/{class_name}.psc"),
                compiler_evidence_required: true,
                compiler_evidence: Some(compiler_evidence.clone()),
            }]),
            vmad: BTreeSet::from([VmadIntent {
                owner: target,
                script_class: class_name.to_string(),
                properties: BTreeSet::new(),
                compiler_evidence_required: true,
                compiler_evidence: Some(compiler_evidence),
            }]),
            assets: BTreeSet::new(),
            inbound_producers: BTreeSet::new(),
            start_disposition: Some(StartDisposition::Autostart),
            admission: QuestRuntimeAdmission::Supported,
            expected_receipt: QuestRuntimeExpectedReceipt::default(),
        }
    }

    fn project(
        plan: &QuestRuntimeComponentPlan,
        interner: &StringInterner,
    ) -> Result<SkyrimQuestProjection, String> {
        project_minimal_quest(
            plan,
            "B21_TestQuest",
            50,
            LocalizedTextIntent {
                field: "FULL".to_string(),
                source_text_id: Some(10),
                text: "A Minimal Quest".to_string(),
                target_table: "STRINGS".to_string(),
            },
            SkyrimQuestFragmentSource {
                fragment_id: "stage-10-item-0".to_string(),
                stage_index: 10,
                stage_item_index: 0,
                actions: vec![SkyrimQuestFragmentAction::SetObjectiveDisplayed {
                    index: 10,
                    displayed: true,
                }],
            },
            "B21",
            interner,
        )
    }

    fn compile_projection(
        root: &std::path::Path,
        projection: &SkyrimQuestProjection,
    ) -> SkyrimPscCompilerEvidence {
        let compiled = papyrus_core::compiler::compile_source(
            &projection.psc_artifact.source,
            &[],
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(
            compiled.ok,
            "compiler diagnostics: {:?}",
            compiled.diagnostics
        );
        let pex_bytes = compiled.pex_bytes.unwrap();
        let pex_artifact_path = root
            .join("data")
            .join("Scripts")
            .join(format!("{}.pex", projection.psc_manifest.class_name));
        std::fs::create_dir_all(pex_artifact_path.parent().unwrap()).unwrap();
        std::fs::write(&pex_artifact_path, &pex_bytes).unwrap();
        SkyrimPscCompilerEvidence {
            manifest_id: projection.psc_manifest.manifest_id.clone(),
            class_name: projection.psc_manifest.class_name.clone(),
            relative_source_path: projection.psc_manifest.relative_source_path.clone(),
            source_blake3: projection.psc_manifest.source_blake3.clone(),
            relative_pex_path: format!("data/Scripts/{}.pex", projection.psc_manifest.class_name),
            pex_artifact_path,
            pex_blake3: blake3::hash(&pex_bytes).to_hex().to_string(),
            target_game: "fo4".to_string(),
            compile_run_id: "compile-1".to_string(),
            compiled_success: true,
        }
    }

    #[test]
    fn projects_minimal_localized_quest_and_deterministic_psc() {
        let plan = fixture_plan();
        let interner = StringInterner::new();
        let projection = project(&plan, &interner).unwrap();
        assert_eq!(projection.record.sig.0, *b"QUST");
        assert_eq!(projection.localized_text.len(), 3);
        assert_eq!(projection.psc_artifact.class_name, "B21_SkyQF_010123");
        assert!(
            projection
                .psc_artifact
                .source
                .contains("SetObjectiveDisplayed(10, True)")
        );
        assert!(
            projection
                .record
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"ALFR")
        );
        assert!(
            projection
                .record
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"CTDA")
        );
    }

    #[test]
    fn unsupported_required_shape_rejects_the_whole_component() {
        let mut plan = fixture_plan();
        let source_scene = QuestRecordKey::new("SCEN", "000999@Skyrim.esm");
        plan.owned_records.push(source_scene.clone());
        plan.mappings.push(SourceTargetFormMapping {
            source: source_scene,
            target: QuestRecordKey::new("SCEN", "010999@Output.esm"),
        });
        let error = project(&plan, &StringInterner::new()).unwrap_err();
        assert!(error.contains("SCEN"), "{error}");
    }

    #[test]
    fn vmad_requires_matching_fresh_compiler_evidence() {
        let root = tempfile::tempdir().unwrap();
        let plan = fixture_plan();
        let interner = StringInterner::new();
        let projection = project(&plan, &interner).unwrap();
        assert!(build_vmad_attachment_intent(&projection, None).is_err());
        let mut evidence = compile_projection(root.path(), &projection);
        evidence.source_blake3 = blake3::hash(b"stale source").to_hex().to_string();
        assert!(
            build_vmad_attachment_intent(&projection, Some(&evidence))
                .unwrap_err()
                .contains("stale")
        );
        evidence.source_blake3 = projection.psc_manifest.source_blake3.clone();
        let intent = build_vmad_attachment_intent(&projection, Some(&evidence)).unwrap();
        assert_eq!(intent.owner, projection.target_quest);
        assert_eq!(intent.compiler_evidence_id, "compile-1");
        assert_eq!(intent.payload["semantic_type"], "QUST");
    }

    #[test]
    fn vmad_rejects_missing_and_hash_mismatched_pex_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let plan = fixture_plan();
        let interner = StringInterner::new();
        let projection = project(&plan, &interner).unwrap();
        let mut evidence = compile_projection(root.path(), &projection);

        std::fs::remove_file(&evidence.pex_artifact_path).unwrap();
        assert!(
            build_vmad_attachment_intent(&projection, Some(&evidence))
                .unwrap_err()
                .contains("read compiled Skyrim PEX")
        );

        let compiled = papyrus_core::compiler::compile_source(
            &projection.psc_artifact.source,
            &[],
            papyrus_core::profile::Game::Fo4,
            None,
        );
        let pex_bytes = compiled.pex_bytes.unwrap();
        std::fs::write(&evidence.pex_artifact_path, pex_bytes).unwrap();
        evidence.pex_blake3 = blake3::hash(b"not the artifact").to_hex().to_string();
        assert!(
            build_vmad_attachment_intent(&projection, Some(&evidence))
                .unwrap_err()
                .contains("does not match compiler evidence")
        );
    }
}
