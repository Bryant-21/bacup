use encoding_rs::WINDOWS_1252;
use indexmap::IndexMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const SCRIPT_CONDITION_FUNCTION_IDS: [u32; 3] = [629, 659, 660];
const QUEST_REWARD_SCRIPT_NAME: &str = "B21:QuestRewards";
const QUEST_REWARD_CAPS_MISC_FORM_ID: u32 = 0x0000_000F;
const VMAD_SCRIPT_ALIASES: [(&str, &str); 1] =
    [("Default2StateActivator", "B21TwoStateActivator76")];

pub type SubrecordPayload = (String, Vec<u8>, Option<String>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptRecord {
    pub form_id: u32,
    pub signature: String,
    pub editor_id: String,
    pub subrecords: Vec<SubrecordPayload>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptReference {
    pub script_name: String,
    pub variable_name: Option<String>,
    pub form_key: String,
    pub form_id: u32,
    pub record_sig: String,
    pub editor_id: String,
    pub kind: &'static str,
    pub condition_inferred: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptReferenceCounters {
    pub native_rows: usize,
    pub vmad_rows: usize,
    pub ctda_rows: usize,
    pub subrecords: usize,
    pub raw_bytes: usize,
}

#[derive(Debug)]
pub struct ScriptReferenceState {
    candidate_records: IndexMap<u32, ScriptRecord>,
    scripts_by_form_id: HashMap<u32, Vec<String>>,
    vmad_script_keys_by_form_id: HashMap<u32, HashSet<String>>,
    references: Vec<ScriptReference>,
    report_quest_reward_coverage: bool,
}

impl ScriptReferenceState {
    pub fn candidate_count(&self) -> usize {
        self.candidate_records.len()
    }
}

#[derive(Debug)]
pub struct ScriptReferenceInspection {
    pub references: Vec<ScriptReference>,
    pub state: ScriptReferenceState,
    pub counters: ScriptReferenceCounters,
    pub alias_records_changed: usize,
    pub alias_bindings_changed: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScriptReconcileResult {
    pub stripped_conditions: usize,
    pub stripped_vmad: usize,
    pub changed_records: usize,
    pub vmad_notes: Vec<String>,
    pub unreached_reward_notes: Vec<String>,
    pub invalid_condition_notes: Vec<String>,
    pub invalid_condition_count: usize,
    pub parse_failures: Vec<ScriptParseFailure>,
    pub pex_member_requests: usize,
    pub pex_member_cache_hits: usize,
    pub pex_member_distinct_paths: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptResolutionEvidence {
    pub script_key: String,
    pub script_name: String,
    pub status: String,
    pub pex_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptParseFailure {
    pub script_key: String,
    pub script_name: String,
    pub pex_path: String,
    pub message: String,
}

#[derive(Clone, Debug)]
struct ScannedRecord {
    record: ScriptRecord,
    form_key: String,
    vmad_references: Vec<ScriptReference>,
    conditions: Vec<ConditionScriptData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConditionScriptData {
    script_name: Option<String>,
    variable_name: Option<String>,
    cursor: usize,
    target_form_id: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VmadScriptEntry {
    name: String,
    raw: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Vmad {
    version: i16,
    object_format: i16,
    scripts: Vec<VmadScriptEntry>,
    fragments: Vec<u8>,
}

impl Vmad {
    fn with_scripts(&self, scripts: &[VmadScriptEntry]) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            6 + scripts.iter().map(|entry| entry.raw.len()).sum::<usize>() + self.fragments.len(),
        );
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.object_format.to_le_bytes());
        out.extend_from_slice(&(scripts.len() as u16).to_le_bytes());
        for entry in scripts {
            out.extend_from_slice(&entry.raw);
        }
        out.extend_from_slice(&self.fragments);
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VmadFormatError(String);

impl std::fmt::Display for VmadFormatError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Default)]
struct ConditionResolutionPlan {
    failed_script_keys: HashSet<String>,
    invalid_condition_keys: HashSet<(String, String)>,
    invalid_condition_notes: Vec<String>,
    parse_failures: Vec<ScriptParseFailure>,
    pex_member_requests: usize,
    pex_member_cache_hits: usize,
    pex_member_distinct_paths: usize,
}

fn condition_resolution_plan(
    state: &ScriptReferenceState,
    resolution_evidence: Vec<ScriptResolutionEvidence>,
) -> ConditionResolutionPlan {
    let mut evidence_by_key = resolution_evidence
        .into_iter()
        .map(|mut evidence| {
            evidence.script_key = script_key(&evidence.script_key);
            (evidence.script_key.clone(), evidence)
        })
        .collect::<IndexMap<_, _>>();
    let mut plan = ConditionResolutionPlan {
        failed_script_keys: evidence_by_key
            .iter()
            .filter_map(|(key, evidence)| {
                (!resolution_ok(&evidence.status) && !infrastructure_failure(&evidence.status))
                    .then_some(key.clone())
            })
            .collect(),
        ..ConditionResolutionPlan::default()
    };
    let mut inferred_checks: IndexMap<
        (u32, String),
        Vec<(ScriptReference, bool, Option<(String, String)>)>,
    > = IndexMap::new();
    let mut member_names_by_path: HashMap<String, HashSet<String>> = HashMap::new();

    for reference in &state.references {
        let Some(variable_name) = reference.variable_name.as_deref() else {
            continue;
        };
        if reference.kind != "condition" {
            continue;
        }
        let key = script_key(&reference.script_name);
        let Some(evidence) = evidence_by_key.get(&key) else {
            if reference.condition_inferred {
                inferred_checks
                    .entry((reference.form_id, variable_key(Some(variable_name))))
                    .or_default()
                    .push((reference.clone(), false, None));
            }
            continue;
        };
        if evidence.status == "target" {
            if reference.condition_inferred {
                inferred_checks
                    .entry((reference.form_id, variable_key(Some(variable_name))))
                    .or_default()
                    .push((reference.clone(), true, None));
            }
            continue;
        }
        let evidence_status = evidence.status.clone();
        let Some(pex_path) = evidence
            .pex_path
            .clone()
            .filter(|_| resolution_ok(&evidence_status))
        else {
            if reference.condition_inferred {
                inferred_checks
                    .entry((reference.form_id, variable_key(Some(variable_name))))
                    .or_default()
                    .push((reference.clone(), false, None));
            }
            continue;
        };

        let cache_key = pex_member_cache_key(Path::new(&pex_path));
        plan.pex_member_requests += 1;
        plan.pex_member_cache_hits += usize::from(member_names_by_path.contains_key(&cache_key));
        let names = if let Some(names) = member_names_by_path.get(&cache_key) {
            Ok(names)
        } else {
            match pex_member_names(Path::new(&pex_path)) {
                Ok(names) => {
                    member_names_by_path.insert(cache_key.clone(), names);
                    Ok(member_names_by_path.get(&cache_key).unwrap())
                }
                Err(message) => Err(message),
            }
        };
        let names = match names {
            Ok(names) => names,
            Err(message) => {
                plan.failed_script_keys.insert(key.clone());
                if let Some(evidence) = evidence_by_key.get_mut(&key) {
                    evidence.status = "parse_failed".to_string();
                }
                plan.parse_failures.push(ScriptParseFailure {
                    script_key: key.clone(),
                    script_name: reference.script_name.clone(),
                    pex_path,
                    message,
                });
                if reference.condition_inferred {
                    inferred_checks
                        .entry((reference.form_id, variable_key(Some(variable_name))))
                        .or_default()
                        .push((reference.clone(), false, None));
                }
                continue;
            }
        };

        if names.contains(&variable_key(Some(variable_name))) {
            if reference.condition_inferred {
                inferred_checks
                    .entry((reference.form_id, variable_key(Some(variable_name))))
                    .or_default()
                    .push((reference.clone(), true, None));
            }
            continue;
        }
        let condition_key = (key, variable_key(Some(variable_name)));
        if reference.condition_inferred {
            inferred_checks
                .entry((reference.form_id, condition_key.1.clone()))
                .or_default()
                .push((reference.clone(), false, Some(condition_key)));
        } else {
            plan.invalid_condition_keys.insert(condition_key);
            plan.invalid_condition_notes.push(format!(
                "{} {}: {}.{}",
                reference.record_sig,
                if reference.editor_id.is_empty() {
                    &reference.form_key
                } else {
                    &reference.editor_id
                },
                reference.script_name,
                variable_name
            ));
        }
    }

    for checks in inferred_checks.values() {
        if checks.iter().any(|(_, has_variable, _)| *has_variable) {
            continue;
        }
        let mut note_reference = None;
        for (reference, _, condition_key) in checks {
            note_reference.get_or_insert(reference);
            if let Some(condition_key) = condition_key {
                plan.invalid_condition_keys.insert(condition_key.clone());
            }
        }
        if let Some(reference) = note_reference {
            plan.invalid_condition_notes.push(format!(
                "{} {}: {} not found on any script for condition form",
                reference.record_sig,
                if reference.editor_id.is_empty() {
                    &reference.form_key
                } else {
                    &reference.editor_id
                },
                reference.variable_name.as_deref().unwrap_or_default()
            ));
        }
    }
    plan.pex_member_distinct_paths = member_names_by_path.len();
    plan
}

fn resolution_ok(status: &str) -> bool {
    matches!(status, "target" | "compiled")
}

fn infrastructure_failure(status: &str) -> bool {
    status == "compiler_unavailable"
}

fn pex_member_cache_key(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::new())
            .join(path)
    };
    let normalized = absolute
        .components()
        .fold(PathBuf::new(), |mut normalized, component| {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                _ => normalized.push(component.as_os_str()),
            }
            normalized
        });
    let key = normalized.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        key.replace('/', "\\").to_lowercase()
    }
    #[cfg(not(windows))]
    {
        key
    }
}

fn pex_member_names(path: &Path) -> Result<HashSet<String>, String> {
    let pex = papyrus_core::pex::parse_pex_file(path.to_string_lossy().as_ref())?;
    let mut names = HashSet::new();
    for object in pex.objects {
        for variable in object.variables {
            names.insert(variable.name.to_lowercase());
        }
        for property in object.properties {
            names.insert(property.name.to_lowercase());
            if !property.auto_var.is_empty() {
                names.insert(property.auto_var.to_lowercase());
            }
        }
    }
    Ok(names)
}

pub fn inspect_script_references<ReadRecord, WriteRecord>(
    form_ids: Vec<u32>,
    plugin_name: &str,
    apply_fo76_aliases: bool,
    mut read_record: ReadRecord,
    mut write_record: WriteRecord,
) -> Result<ScriptReferenceInspection, String>
where
    ReadRecord: FnMut(u32) -> Result<Option<ScriptRecord>, String>,
    WriteRecord: FnMut(u32, Vec<SubrecordPayload>) -> Result<bool, String>,
{
    let mut scanned_records = Vec::new();
    let mut scripts_by_form_id: HashMap<u32, Vec<String>> = HashMap::new();
    let mut vmad_refs_by_form_id: HashMap<u32, Vec<ScriptReference>> = HashMap::new();
    let mut counters = ScriptReferenceCounters::default();
    let mut alias_records_changed = 0usize;
    let mut alias_bindings_changed = 0usize;
    let mut warnings = Vec::new();

    for form_id in form_ids {
        let Some(mut record) = read_record(form_id)? else {
            continue;
        };
        counters.native_rows += 1;
        let has_vmad = record.subrecords.iter().any(|(sig, _, _)| sig == "VMAD");
        let has_ctda = record.subrecords.iter().any(|(sig, _, _)| sig == "CTDA");
        counters.vmad_rows += usize::from(has_vmad);
        counters.ctda_rows += usize::from(has_ctda);
        counters.subrecords += record.subrecords.len();
        counters.raw_bytes += record
            .subrecords
            .iter()
            .map(|(_, data, _)| data.len())
            .sum::<usize>();

        if apply_fo76_aliases {
            let (rewritten, replacements) = rewrite_vmad_script_aliases(&record);
            if replacements > 0 {
                if write_record(record.form_id, rewritten.subrecords.clone())? {
                    record = rewritten;
                    alias_records_changed += 1;
                    alias_bindings_changed += replacements;
                } else {
                    warnings.push(format!(
                        "failed to apply FO76 script alias for {:08X}",
                        record.form_id
                    ));
                }
            }
        }

        let form_key = script_ref_form_key(record.form_id, plugin_name);
        let vmad_references = vmad_script_references(&record, &form_key);
        if !vmad_references.is_empty() {
            let script_names = vmad_references
                .iter()
                .map(|reference| reference.script_name.clone())
                .collect::<Vec<_>>();
            for key in form_id_lookup_keys(record.form_id) {
                scripts_by_form_id.insert(key, script_names.clone());
            }
            vmad_refs_by_form_id.insert(record.form_id, vmad_references.clone());
        }
        let conditions = condition_script_data(&record.subrecords);
        scanned_records.push(ScannedRecord {
            record,
            form_key,
            vmad_references,
            conditions,
        });
    }

    let mut references = Vec::new();
    let mut seen_references = HashSet::new();
    let mut candidate_records = IndexMap::new();
    let mut vmad_script_keys_by_form_id: HashMap<u32, HashSet<String>> = HashMap::new();
    for scanned in scanned_records {
        let mut record_references = condition_script_references(
            &scanned.record,
            &scanned.form_key,
            &scanned.conditions,
            &scripts_by_form_id,
        );
        record_references.extend(
            vmad_refs_by_form_id
                .get(&scanned.record.form_id)
                .cloned()
                .unwrap_or_else(|| scanned.vmad_references.clone()),
        );
        if record_references.is_empty() {
            continue;
        }
        candidate_records.insert(scanned.record.form_id, scanned.record);
        for reference in record_references {
            let key = (
                script_key(&reference.script_name),
                variable_key(reference.variable_name.as_deref()),
                reference.form_id,
                reference.kind,
            );
            if !seen_references.insert(key) {
                continue;
            }
            if reference.kind == "vmad" {
                vmad_script_keys_by_form_id
                    .entry(reference.form_id)
                    .or_default()
                    .insert(script_key(&reference.script_name));
            }
            references.push(reference);
        }
    }

    let state_references = references.clone();
    Ok(ScriptReferenceInspection {
        references,
        state: ScriptReferenceState {
            candidate_records,
            scripts_by_form_id,
            vmad_script_keys_by_form_id,
            references: state_references,
            report_quest_reward_coverage: apply_fo76_aliases,
        },
        counters,
        alias_records_changed,
        alias_bindings_changed,
        warnings,
    })
}

pub fn reconcile_script_references<WriteRecord>(
    state: &ScriptReferenceState,
    resolution_evidence: Vec<ScriptResolutionEvidence>,
    mut write_record: WriteRecord,
) -> Result<ScriptReconcileResult, String>
where
    WriteRecord: FnMut(u32, Vec<SubrecordPayload>) -> Result<bool, String>,
{
    let condition_plan = condition_resolution_plan(state, resolution_evidence);
    let failed_script_keys = condition_plan.failed_script_keys;
    let invalid_condition_count = condition_plan.invalid_condition_keys.len();
    let invalid_condition_keys = condition_plan.invalid_condition_keys;
    let attach_quest_rewards = state.report_quest_reward_coverage
        && !failed_script_keys.contains(&script_key(QUEST_REWARD_SCRIPT_NAME));
    let vmad_strip_form_ids = state
        .vmad_script_keys_by_form_id
        .iter()
        .filter_map(|(form_id, script_keys)| {
            (!script_keys.is_empty() && script_keys.is_subset(&failed_script_keys))
                .then_some(*form_id)
        })
        .collect::<HashSet<_>>();

    let mut result = ScriptReconcileResult {
        invalid_condition_count,
        invalid_condition_notes: condition_plan.invalid_condition_notes,
        parse_failures: condition_plan.parse_failures,
        pex_member_requests: condition_plan.pex_member_requests,
        pex_member_cache_hits: condition_plan.pex_member_cache_hits,
        pex_member_distinct_paths: condition_plan.pex_member_distinct_paths,
        ..ScriptReconcileResult::default()
    };
    for (form_id, record) in &state.candidate_records {
        let (subrecords, condition_count, vmad_count, notes) = subrecords_after_script_reconcile(
            record,
            &failed_script_keys,
            &invalid_condition_keys,
            vmad_strip_form_ids.contains(form_id),
            &state.scripts_by_form_id,
            attach_quest_rewards,
        )?;
        result.vmad_notes.extend(notes);
        if subrecord_bytes_equal(&subrecords, &record.subrecords) {
            continue;
        }
        if !write_record(*form_id, subrecords)? {
            continue;
        }
        result.changed_records += 1;
        result.stripped_conditions += condition_count;
        result.stripped_vmad += vmad_count;
    }
    if state.report_quest_reward_coverage {
        result.unreached_reward_notes = unreached_quest_reward_attachments(state);
    }
    Ok(result)
}

fn subrecord_bytes_equal(left: &[SubrecordPayload], right: &[SubrecordPayload]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((left_sig, left_data, _), (right_sig, right_data, _))| {
                left_sig == right_sig && left_data == right_data
            })
}

fn script_key(script_name: &str) -> String {
    script_name
        .replace('\\', ":")
        .replace('/', ":")
        .trim()
        .to_lowercase()
}

fn variable_key(variable_name: Option<&str>) -> String {
    variable_name.unwrap_or_default().trim().to_lowercase()
}

fn form_id_lookup_keys(form_id: u32) -> Vec<u32> {
    let object_id = form_id & 0x00FF_FFFF;
    if form_id == object_id {
        vec![form_id]
    } else {
        vec![form_id, object_id]
    }
}

fn script_ref_form_key(form_id: u32, plugin_name: &str) -> String {
    if plugin_name.is_empty() {
        format!("{form_id:08X}")
    } else {
        format!("{:06X}:{plugin_name}", form_id & 0x00FF_FFFF)
    }
}

fn record_label(record: &ScriptRecord) -> String {
    let label = format!("{} {:08X}", record.signature, record.form_id);
    if record.editor_id.is_empty() {
        label
    } else {
        format!("{label} {}", record.editor_id)
    }
}

fn read_zstring(data: &[u8]) -> String {
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    let (decoded, _, _) = WINDOWS_1252.decode(&data[..end]);
    decoded.trim().to_string()
}

fn ctda_function_id(data: &[u8]) -> Option<u32> {
    let bytes = data.get(8..12)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn ctda_param1_form_id(data: &[u8]) -> Option<u32> {
    let bytes = data.get(12..16)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn condition_script_data(subrecords: &[SubrecordPayload]) -> Vec<ConditionScriptData> {
    subrecords
        .iter()
        .enumerate()
        .filter_map(|(index, (sig, _, _))| {
            (sig == "CTDA").then(|| condition_script_data_at(subrecords, index))?
        })
        .collect()
}

fn condition_script_data_at(
    subrecords: &[SubrecordPayload],
    index: usize,
) -> Option<ConditionScriptData> {
    let (signature, data, _) = subrecords.get(index)?;
    if signature != "CTDA"
        || !ctda_function_id(data).is_some_and(|id| SCRIPT_CONDITION_FUNCTION_IDS.contains(&id))
    {
        return None;
    }
    let mut script_name = None;
    let mut variable_name = None;
    let mut cursor = index + 1;
    while let Some((signature, data, _)) = subrecords.get(cursor) {
        match signature.as_str() {
            "CIS1" => script_name = Some(read_zstring(data)),
            "CIS2" => variable_name = Some(read_zstring(data)),
            _ => break,
        }
        cursor += 1;
    }
    Some(ConditionScriptData {
        script_name: script_name.filter(|name| !name.is_empty()),
        variable_name: variable_name.filter(|name| !name.is_empty()),
        cursor,
        target_form_id: ctda_param1_form_id(data),
    })
}

fn resolved_condition_scripts(
    condition: &ConditionScriptData,
    scripts_by_form_id: &HashMap<u32, Vec<String>>,
) -> Vec<String> {
    if let Some(script_name) = condition.script_name.as_ref() {
        return vec![script_name.clone()];
    }
    for key in condition
        .target_form_id
        .into_iter()
        .flat_map(form_id_lookup_keys)
    {
        if let Some(scripts) = scripts_by_form_id.get(&key)
            && !scripts.is_empty()
        {
            return scripts.clone();
        }
    }
    Vec::new()
}

fn condition_script_references(
    record: &ScriptRecord,
    form_key: &str,
    conditions: &[ConditionScriptData],
    scripts_by_form_id: &HashMap<u32, Vec<String>>,
) -> Vec<ScriptReference> {
    let mut references = Vec::new();
    for condition in conditions {
        for script_name in resolved_condition_scripts(condition, scripts_by_form_id) {
            references.push(ScriptReference {
                script_name,
                variable_name: condition.variable_name.clone(),
                form_key: form_key.to_string(),
                form_id: record.form_id,
                record_sig: record.signature.clone(),
                editor_id: record.editor_id.clone(),
                kind: "condition",
                condition_inferred: condition.script_name.is_none(),
            });
        }
    }
    references
}

fn vmad_script_references(record: &ScriptRecord, form_key: &str) -> Vec<ScriptReference> {
    let mut references = Vec::new();
    let mut seen = HashSet::new();
    for (signature, data, _) in &record.subrecords {
        if signature != "VMAD" {
            continue;
        }
        for script_name in vmad_script_names(data, &record.signature) {
            let script_name = script_name.trim().to_string();
            if script_name.is_empty() || !seen.insert(script_key(&script_name)) {
                continue;
            }
            references.push(ScriptReference {
                script_name,
                variable_name: None,
                form_key: form_key.to_string(),
                form_id: record.form_id,
                record_sig: record.signature.clone(),
                editor_id: record.editor_id.clone(),
                kind: "vmad",
                condition_inferred: false,
            });
        }
    }
    references
}

fn rewrite_vmad_script_aliases(record: &ScriptRecord) -> (ScriptRecord, usize) {
    let mut rewritten = record.clone();
    let mut replacements = 0usize;
    for (signature, data, _) in &mut rewritten.subrecords {
        if signature != "VMAD" {
            continue;
        }
        for (source_name, target_name) in VMAD_SCRIPT_ALIASES {
            let source = length_prefixed_ascii(source_name);
            let target = length_prefixed_ascii(target_name);
            assert_eq!(source.len(), target.len());
            replacements += replace_equal_length(data, &source, &target);
        }
    }
    (rewritten, replacements)
}

fn length_prefixed_ascii(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() + 2);
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
    out
}

fn replace_equal_length(data: &mut [u8], source: &[u8], target: &[u8]) -> usize {
    let mut count = 0usize;
    let mut offset = 0usize;
    while offset + source.len() <= data.len() {
        if &data[offset..offset + source.len()] == source {
            data[offset..offset + target.len()].copy_from_slice(target);
            count += 1;
            offset += source.len();
        } else {
            offset += 1;
        }
    }
    count
}

fn parse_vmad(data: &[u8]) -> Result<Vmad, VmadFormatError> {
    if data.len() < 6 {
        return Err(VmadFormatError("VMAD shorter than its header".to_string()));
    }
    let version = i16::from_le_bytes(data[0..2].try_into().unwrap());
    let object_format = i16::from_le_bytes(data[2..4].try_into().unwrap());
    if !matches!(object_format, 1 | 2) {
        return Err(VmadFormatError(format!(
            "unsupported VMAD object format {object_format}"
        )));
    }
    let script_count = u16::from_le_bytes(data[4..6].try_into().unwrap()) as usize;
    let mut cursor = VmadCursor::new(data, 6);
    let mut scripts = Vec::with_capacity(script_count);
    for _ in 0..script_count {
        let start = cursor.offset;
        let name = cursor.read_string()?;
        cursor.skip_script_body(object_format as u16)?;
        scripts.push(VmadScriptEntry {
            name,
            raw: data[start..cursor.offset].to_vec(),
        });
    }
    Ok(Vmad {
        version,
        object_format,
        scripts,
        fragments: data[cursor.offset..].to_vec(),
    })
}

fn vmad_script_names(data: &[u8], record_signature: &str) -> Vec<String> {
    let Some(vmad) = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
        data, &[], "", Some(record_signature),
    ) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    collect_script_names(&vmad, &mut names);
    names
}

fn collect_script_names(value: &Value, names: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if key == "ScriptName"
                    && let Some(name) = value.as_str().map(str::trim)
                    && !name.is_empty()
                {
                    names.push(name.to_string());
                    continue;
                }
                collect_script_names(value, names);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_script_names(value, names);
            }
        }
        _ => {}
    }
}

struct VmadCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> VmadCursor<'a> {
    fn new(data: &'a [u8], offset: usize) -> Self {
        Self { data, offset }
    }

    fn advance(&mut self, amount: usize, message: &'static str) -> Result<(), VmadFormatError> {
        let end = self
            .offset
            .checked_add(amount)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| VmadFormatError(message.to_string()))?;
        self.offset = end;
        Ok(())
    }

    fn read_u8(&mut self, message: &'static str) -> Result<u8, VmadFormatError> {
        let value = *self
            .data
            .get(self.offset)
            .ok_or_else(|| VmadFormatError(message.to_string()))?;
        self.offset += 1;
        Ok(value)
    }

    fn read_u16(&mut self, message: &'static str) -> Result<u16, VmadFormatError> {
        let bytes = self
            .data
            .get(self.offset..self.offset.saturating_add(2))
            .ok_or_else(|| VmadFormatError(message.to_string()))?;
        self.offset += 2;
        Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn read_i32(&mut self, message: &'static str) -> Result<i32, VmadFormatError> {
        let bytes = self
            .data
            .get(self.offset..self.offset.saturating_add(4))
            .ok_or_else(|| VmadFormatError(message.to_string()))?;
        self.offset += 4;
        Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn read_string(&mut self) -> Result<String, VmadFormatError> {
        let length = self.read_u16("truncated string length")? as usize;
        let bytes = self
            .data
            .get(self.offset..self.offset.saturating_add(length))
            .ok_or_else(|| VmadFormatError("truncated string".to_string()))?;
        self.offset += length;
        let (decoded, _, _) = WINDOWS_1252.decode(bytes);
        Ok(decoded.into_owned())
    }

    fn skip_script_body(&mut self, object_format: u16) -> Result<(), VmadFormatError> {
        self.advance(1, "truncated script flags")?;
        let property_count = self.read_u16("truncated property count")? as usize;
        for _ in 0..property_count {
            self.read_string()?;
            self.skip_typed_value(object_format)?;
        }
        Ok(())
    }

    fn skip_typed_value(&mut self, object_format: u16) -> Result<(), VmadFormatError> {
        let value_type = self.read_u8("truncated property header")?;
        self.advance(1, "truncated property header")?;
        self.skip_value(value_type, object_format)
    }

    fn skip_value(&mut self, value_type: u8, object_format: u16) -> Result<(), VmadFormatError> {
        match value_type {
            0 | 6 => Ok(()),
            1 => self.advance(8, "truncated value"),
            2 => self.read_string().map(|_| ()),
            3 | 4 => self.advance(4, "truncated value"),
            5 => self.advance(1, "truncated value"),
            7 => self.skip_struct(object_format),
            11 | 13 | 14 | 15 => {
                let count = self.read_count()?;
                let element_size = match value_type {
                    11 => 8,
                    13 | 14 => 4,
                    15 => 1,
                    _ => unreachable!(),
                };
                self.advance(count.saturating_mul(element_size), "truncated value")
            }
            12 => {
                let count = self.read_count()?;
                for _ in 0..count {
                    self.read_string()?;
                }
                Ok(())
            }
            16 => self.advance(4, "truncated value"),
            17 => {
                let count = self.read_count()?;
                for _ in 0..count {
                    self.skip_struct(object_format)?;
                }
                Ok(())
            }
            _ => Err(VmadFormatError(format!(
                "unknown VMAD property type {value_type}"
            ))),
        }
    }

    fn read_count(&mut self) -> Result<usize, VmadFormatError> {
        let count = self.read_i32("truncated element count")?;
        if count < 0 {
            return Err(VmadFormatError("negative element count".to_string()));
        }
        Ok(count as usize)
    }

    fn skip_struct(&mut self, object_format: u16) -> Result<(), VmadFormatError> {
        let count = self.read_count()?;
        for _ in 0..count {
            self.read_string()?;
            self.skip_typed_value(object_format)?;
        }
        Ok(())
    }
}

fn subrecords_after_script_reconcile(
    record: &ScriptRecord,
    failed_script_keys: &HashSet<String>,
    invalid_condition_keys: &HashSet<(String, String)>,
    strip_vmad: bool,
    scripts_by_form_id: &HashMap<u32, Vec<String>>,
    attach_quest_rewards: bool,
) -> Result<(Vec<SubrecordPayload>, usize, usize, Vec<String>), String> {
    let mut subrecords = Vec::new();
    let mut stripped_conditions = 0usize;
    let mut stripped_vmad = 0usize;
    let mut notes = Vec::new();
    let mut index = 0usize;
    while index < record.subrecords.len() {
        let (signature, data, _) = &record.subrecords[index];
        if signature == "VMAD" {
            let (replacement, note) =
                vmad_after_script_prune(data, record, failed_script_keys, strip_vmad);
            if let Some(note) = note {
                notes.push(note);
            }
            if let Some(mut replacement) = replacement {
                if attach_quest_rewards {
                    let (with_reward, note) = vmad_with_quest_reward_script(&replacement, record)?;
                    replacement = with_reward;
                    if let Some(note) = note {
                        notes.push(note);
                    }
                }
                subrecords.push((signature.clone(), replacement, None));
            } else {
                stripped_vmad += 1;
            }
            index += 1;
            continue;
        }
        if signature == "CTDA"
            && let Some(condition) = condition_script_data_at(&record.subrecords, index)
        {
            let resolved = resolved_condition_scripts(&condition, scripts_by_form_id);
            let unresolved = resolved
                .iter()
                .filter(|script_name| {
                    let script_key = script_key(script_name);
                    failed_script_keys.contains(&script_key)
                        || invalid_condition_keys.contains(&(
                            script_key,
                            variable_key(condition.variable_name.as_deref()),
                        ))
                })
                .count();
            let should_strip =
                unresolved > 0 && (condition.script_name.is_some() || unresolved == resolved.len());
            if should_strip {
                stripped_conditions += 1;
                index = condition.cursor;
                continue;
            }
        }
        subrecords.push((signature.clone(), data.clone(), None));
        index += 1;
    }
    if stripped_conditions > 0 {
        sync_single_citc_condition_count(&mut subrecords);
    }
    Ok((subrecords, stripped_conditions, stripped_vmad, notes))
}

fn vmad_after_script_prune(
    data: &[u8],
    record: &ScriptRecord,
    failed_script_keys: &HashSet<String>,
    strip_vmad: bool,
) -> (Option<Vec<u8>>, Option<String>) {
    let label = record_label(record);
    if strip_vmad {
        return (
            None,
            Some(format!(
                "{label}: removed the whole VMAD; no bound script survived"
            )),
        );
    }
    let vmad = match parse_vmad(data) {
        Ok(vmad) => vmad,
        Err(error) => {
            return (
                Some(data.to_vec()),
                Some(format!(
                    "{label}: left VMAD untouched, unreadable ({error})"
                )),
            );
        }
    };
    let dropped = vmad
        .scripts
        .iter()
        .filter(|entry| failed_script_keys.contains(&script_key(&entry.name)))
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>();
    if dropped.is_empty() {
        return (Some(data.to_vec()), None);
    }
    let kept = vmad
        .scripts
        .iter()
        .filter(|entry| !failed_script_keys.contains(&script_key(&entry.name)))
        .cloned()
        .collect::<Vec<_>>();
    (
        Some(vmad.with_scripts(&kept)),
        Some(format!(
            "{label}: dropped {} failed binding(s) ({}); kept {}",
            dropped.len(),
            dropped.join(", "),
            kept.len()
        )),
    )
}

#[derive(Clone, Copy)]
struct QuestRewardAttachment {
    stage: i32,
    caps_amount: u32,
    item: u32,
    item_count: i32,
    reason: &'static str,
}

fn quest_reward_attachment(record: &ScriptRecord) -> Option<QuestRewardAttachment> {
    if record.signature != "QUST" {
        return None;
    }
    match record.form_id & 0x00FF_FFFF {
        0x0054_F1A4 => Some(QuestRewardAttachment {
            stage: 900,
            caps_amount: 0x0059_18F0,
            item: 0x005A_38E4,
            item_count: 1,
            reason: "COMP_RQ_Fetch stage 900",
        }),
        0x0056_FB76 => Some(QuestRewardAttachment {
            stage: 900,
            caps_amount: 0x0059_18F0,
            item: 0x005A_38E5,
            item_count: 1,
            reason: "COMP_RQ_Kill stage 900",
        }),
        0x0057_27AD => Some(QuestRewardAttachment {
            stage: 900,
            caps_amount: 0x0059_18F0,
            item: 0x005A_38E6,
            item_count: 1,
            reason: "COMP_RQ_Rescue stage 900",
        }),
        _ => None,
    }
}

fn vmad_with_quest_reward_script(
    data: &[u8],
    record: &ScriptRecord,
) -> Result<(Vec<u8>, Option<String>), String> {
    let Some(attachment) = quest_reward_attachment(record) else {
        return Ok((data.to_vec(), None));
    };
    let label = record_label(record);
    let vmad = match parse_vmad(data) {
        Ok(vmad) => vmad,
        Err(error) => {
            return Ok((
                data.to_vec(),
                Some(format!(
                    "{label}: no reward attach, VMAD unreadable ({error})"
                )),
            ));
        }
    };
    if vmad
        .scripts
        .iter()
        .any(|entry| script_key(&entry.name) == script_key(QUEST_REWARD_SCRIPT_NAME))
    {
        return Ok((data.to_vec(), None));
    }
    if vmad.scripts.len() == usize::from(u16::MAX) {
        return Err("'H' format requires 0 <= number <= 65535".to_string());
    }
    let plugin_index = record.form_id & 0xFF00_0000;
    let reward_script = build_vmad_script(
        QUEST_REWARD_SCRIPT_NAME,
        &[
            ("CapsStages", 13, int_array_value(&[attachment.stage])),
            (
                "RewardCaps",
                11,
                object_array_value(&[plugin_index | attachment.caps_amount], vmad.object_format),
            ),
            (
                "CapsItem",
                1,
                object_value(QUEST_REWARD_CAPS_MISC_FORM_ID, vmad.object_format),
            ),
            ("ItemStages", 13, int_array_value(&[attachment.stage])),
            (
                "RewardItems",
                11,
                object_array_value(&[plugin_index | attachment.item], vmad.object_format),
            ),
            (
                "RewardCounts",
                13,
                int_array_value(&[attachment.item_count]),
            ),
        ],
    );
    let mut scripts = vmad.scripts.clone();
    scripts.push(reward_script);
    Ok((
        vmad.with_scripts(&scripts),
        Some(format!(
            "{label}: attached {QUEST_REWARD_SCRIPT_NAME} ({})",
            attachment.reason
        )),
    ))
}

fn build_vmad_script(name: &str, properties: &[(&str, u8, Vec<u8>)]) -> VmadScriptEntry {
    let mut raw = write_vmad_string(name);
    raw.push(0);
    raw.extend_from_slice(&(properties.len() as u16).to_le_bytes());
    for (property_name, value_type, value) in properties {
        raw.extend_from_slice(&write_vmad_string(property_name));
        raw.push(*value_type);
        raw.push(1);
        raw.extend_from_slice(value);
    }
    VmadScriptEntry {
        name: name.to_string(),
        raw,
    }
}

fn write_vmad_string(value: &str) -> Vec<u8> {
    let (encoded, _, had_errors) = WINDOWS_1252.encode(value);
    assert!(
        !had_errors,
        "VMAD constant must be representable in Windows-1252"
    );
    let mut out = Vec::with_capacity(encoded.len() + 2);
    out.extend_from_slice(&(encoded.len() as u16).to_le_bytes());
    out.extend_from_slice(&encoded);
    out
}

fn object_value(form_id: u32, object_format: i16) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    match object_format {
        2 => {
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&(-1i16).to_le_bytes());
            out.extend_from_slice(&form_id.to_le_bytes());
        }
        1 => {
            out.extend_from_slice(&form_id.to_le_bytes());
            out.extend_from_slice(&(-1i16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
        }
        _ => unreachable!("validated VMAD object format"),
    }
    out
}

fn object_array_value(form_ids: &[u32], object_format: i16) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + form_ids.len() * 8);
    out.extend_from_slice(&(form_ids.len() as i32).to_le_bytes());
    for form_id in form_ids {
        out.extend_from_slice(&object_value(*form_id, object_format));
    }
    out
}

fn int_array_value(values: &[i32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + values.len() * 4);
    out.extend_from_slice(&(values.len() as i32).to_le_bytes());
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn sync_single_citc_condition_count(subrecords: &mut [SubrecordPayload]) {
    let indexes = subrecords
        .iter()
        .enumerate()
        .filter_map(|(index, (signature, _, _))| (signature == "CITC").then_some(index))
        .collect::<Vec<_>>();
    let [index] = indexes.as_slice() else {
        return;
    };
    if subrecords[*index].1.len() < 4 {
        return;
    }
    let condition_count = subrecords
        .iter()
        .filter(|(signature, _, _)| signature == "CTDA")
        .count() as u32;
    subrecords[*index].1[0..4].copy_from_slice(&condition_count.to_le_bytes());
}

fn unreached_quest_reward_attachments(state: &ScriptReferenceState) -> Vec<String> {
    let reached = state
        .candidate_records
        .iter()
        .filter_map(|(form_id, record)| {
            (record.signature == "QUST").then_some(form_id & 0x00FF_FFFF)
        })
        .collect::<HashSet<_>>();
    [
        (0x0054_F1A4, "COMP_RQ_Fetch stage 900", 900),
        (0x0056_FB76, "COMP_RQ_Kill stage 900", 900),
        (0x0057_27AD, "COMP_RQ_Rescue stage 900", 900),
    ]
    .into_iter()
    .filter(|(local, _, _)| !reached.contains(local))
    .map(|(local, reason, stage)| {
        format!(
            "quest reward attach never reached QUST {local:06X} ({reason}); \
             its stage-{stage} reward is unpaid"
        )
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn subrecord(signature: &str, data: Vec<u8>) -> SubrecordPayload {
        (signature.to_string(), data, None)
    }

    fn record(
        form_id: u32,
        signature: &str,
        editor_id: &str,
        subrecords: Vec<SubrecordPayload>,
    ) -> ScriptRecord {
        ScriptRecord {
            form_id,
            signature: signature.to_string(),
            editor_id: editor_id.to_string(),
            subrecords,
        }
    }

    fn script_entry(name: &str) -> Vec<u8> {
        let mut out = write_vmad_string(name);
        out.extend_from_slice(&[0, 0, 0]);
        out
    }

    fn script_with_properties(name: &str, properties: &[(&str, u8, Vec<u8>)]) -> Vec<u8> {
        build_vmad_script(name, properties).raw
    }

    fn vmad(names: &[&str], fragments: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&6i16.to_le_bytes());
        out.extend_from_slice(&2i16.to_le_bytes());
        out.extend_from_slice(&(names.len() as u16).to_le_bytes());
        for name in names {
            out.extend_from_slice(&script_entry(name));
        }
        out.extend_from_slice(fragments);
        out
    }

    fn ctda(function_id: u32, param1: u32) -> Vec<u8> {
        let mut out = vec![0; 28];
        out[8..12].copy_from_slice(&function_id.to_le_bytes());
        out[12..16].copy_from_slice(&param1.to_le_bytes());
        out
    }

    fn inspect_fixture(records: Vec<ScriptRecord>, aliases: bool) -> ScriptReferenceInspection {
        let records = Rc::new(RefCell::new(
            records
                .into_iter()
                .map(|record| (record.form_id, record))
                .collect::<HashMap<_, _>>(),
        ));
        let form_ids = records.borrow().keys().copied().collect::<Vec<_>>();
        let read_records = Rc::clone(&records);
        let write_records = Rc::clone(&records);
        inspect_script_references(
            form_ids,
            "Output.esm",
            aliases,
            move |form_id| Ok(read_records.borrow().get(&form_id).cloned()),
            move |form_id, subrecords| {
                write_records
                    .borrow_mut()
                    .get_mut(&form_id)
                    .unwrap()
                    .subrecords = subrecords;
                Ok(true)
            },
        )
        .unwrap()
    }

    fn resolution(
        script_name: &str,
        status: &str,
        pex_path: Option<&Path>,
    ) -> ScriptResolutionEvidence {
        ScriptResolutionEvidence {
            script_key: script_key(script_name),
            script_name: script_name.to_string(),
            status: status.to_string(),
            pex_path: pex_path.map(|path| path.to_string_lossy().into_owned()),
        }
    }

    fn compile_test_pex(path: &Path) {
        let compiled = papyrus_core::compiler::compile_source(
            "ScriptName MemberTest extends Quest\nInt Property Existing Auto\n",
            &[],
            papyrus_core::profile::Game::Fo4,
            None,
        );
        assert!(compiled.ok, "{:?}", compiled.diagnostics);
        std::fs::write(path, compiled.pex_bytes.unwrap()).unwrap();
    }

    #[test]
    fn inspection_preserves_vmad_then_condition_order_and_deduplicates() {
        let quest_id = 0x0701_A634;
        let quest = record(
            quest_id,
            "QUST",
            "Quest",
            vec![subrecord(
                "VMAD",
                vmad(&["QuestScript", "QuestScript"], &[]),
            )],
        );
        let info = record(
            0x003C_4727,
            "INFO",
            "",
            vec![
                subrecord("CTDA", ctda(629, quest_id)),
                subrecord("CIS2", b"::Member_var\0".to_vec()),
            ],
        );
        let records = HashMap::from([(quest.form_id, quest), (info.form_id, info)]);
        let order = vec![0x003C_4727, quest_id];
        let inspection = inspect_script_references(
            order,
            "SeventySix.esm",
            false,
            |form_id| Ok(records.get(&form_id).cloned()),
            |_form_id, _subrecords| Ok(true),
        )
        .unwrap();

        assert_eq!(inspection.references.len(), 2);
        assert_eq!(inspection.references[0].kind, "condition");
        assert_eq!(inspection.references[0].script_name, "QuestScript");
        assert!(inspection.references[0].condition_inferred);
        assert_eq!(inspection.references[1].kind, "vmad");
        assert_eq!(inspection.references[1].form_key, "01A634:SeventySix.esm");
        assert_eq!(inspection.state.candidate_count(), 2);
    }

    #[test]
    fn alias_write_controls_reference_name_and_counts() {
        let form_id = 0x0037_879E;
        let source = record(
            form_id,
            "MSTT",
            "CoolingTowerFX",
            vec![subrecord("VMAD", vmad(&["Default2StateActivator"], &[]))],
        );
        let failed = inspect_script_references(
            vec![form_id],
            "SeventySix.esm",
            true,
            |_form_id| Ok(Some(source.clone())),
            |_form_id, _subrecords| Ok(false),
        )
        .unwrap();
        assert_eq!(failed.references[0].script_name, "Default2StateActivator");
        assert_eq!(failed.alias_bindings_changed, 0);
        assert_eq!(failed.warnings.len(), 1);

        let applied = inspect_script_references(
            vec![form_id],
            "SeventySix.esm",
            true,
            |_form_id| Ok(Some(source.clone())),
            |_form_id, _subrecords| Ok(true),
        )
        .unwrap();
        assert_eq!(applied.references[0].script_name, "B21TwoStateActivator76");
        assert_eq!(applied.alias_records_changed, 1);
        assert_eq!(applied.alias_bindings_changed, 1);
        assert_eq!(
            hex::encode_upper(&applied.state.candidate_records[&form_id].subrecords[0].1),
            "060002000100160042323154776F5374617465416374697661746F723736000000"
        );
    }

    #[test]
    fn top_level_prune_preserves_nested_property_bytes_and_opaque_tail() {
        let mut nested_struct = 2i32.to_le_bytes().to_vec();
        nested_struct.extend_from_slice(&write_vmad_string("Objects"));
        nested_struct.extend_from_slice(&[11, 1]);
        nested_struct.extend_from_slice(&object_array_value(&[0x0100_0001], 2));
        nested_struct.extend_from_slice(&write_vmad_string("Values"));
        nested_struct.extend_from_slice(&[13, 1]);
        nested_struct.extend_from_slice(&int_array_value(&[7, 9]));

        let mut struct_array = 1i32.to_le_bytes().to_vec();
        struct_array.extend_from_slice(&nested_struct);
        let dropped = VmadScriptEntry {
            name: "BrokenScript".to_string(),
            raw: script_with_properties(
                "BrokenScript",
                &[
                    ("Nested", 7, nested_struct.clone()),
                    ("Rows", 17, struct_array),
                ],
            ),
        };
        let kept = VmadScriptEntry {
            name: "WorkingScript".to_string(),
            raw: script_with_properties(
                "WorkingScript",
                &[("Objects", 11, object_array_value(&[0x0100_0002], 2))],
            ),
        };
        let tail = b"opaque-fragment-tail".to_vec();
        let payload = Vmad {
            version: 6,
            object_format: 2,
            scripts: vec![dropped.clone(), kept.clone()],
            fragments: tail.clone(),
        }
        .with_scripts(&[dropped, kept.clone()]);
        let record = record(
            0x0800_0800,
            "PERK",
            "NestedProperties",
            vec![subrecord("VMAD", payload)],
        );

        let (replacement, note) = vmad_after_script_prune(
            &record.subrecords[0].1,
            &record,
            &HashSet::from(["brokenscript".to_string()]),
            false,
        );
        let replacement = replacement.unwrap();
        let expected = Vmad {
            version: 6,
            object_format: 2,
            scripts: vec![kept.clone()],
            fragments: tail.clone(),
        }
        .with_scripts(&[kept]);
        assert_eq!(replacement, expected);
        assert!(replacement.ends_with(&tail));
        assert_eq!(
            note.unwrap(),
            "PERK 08000800 NestedProperties: dropped 1 failed binding(s) (BrokenScript); kept 1"
        );
    }

    #[test]
    fn duplicate_form_ids_keep_first_position_and_last_snapshot() {
        let form_id = 0x0000_0800;
        let reads = Rc::new(RefCell::new(vec![
            record(
                form_id,
                "PERK",
                "First",
                vec![subrecord("VMAD", vmad(&["FirstScript"], &[]))],
            ),
            record(
                form_id,
                "PERK",
                "Last",
                vec![subrecord("VMAD", vmad(&["LastScript"], &[]))],
            ),
        ]));
        let read_records = Rc::clone(&reads);
        let inspection = inspect_script_references(
            vec![form_id, form_id],
            "Output.esp",
            false,
            move |_form_id| Ok(Some(read_records.borrow_mut().remove(0))),
            |_form_id, _subrecords| Ok(true),
        )
        .unwrap();

        assert_eq!(inspection.references[0].script_name, "LastScript");
        assert_eq!(inspection.references.len(), 1);
        assert_eq!(
            inspection
                .state
                .candidate_records
                .get(&form_id)
                .unwrap()
                .editor_id,
            "Last"
        );
    }

    #[test]
    fn quest_tail_names_match_authoring_traversal_order() {
        let mut tail = vec![1];
        tail.extend_from_slice(&1u16.to_le_bytes());
        tail.extend_from_slice(&write_vmad_string("QuestFragmentOwner"));
        tail.extend_from_slice(&[0, 0, 0]);
        tail.extend_from_slice(&900u16.to_le_bytes());
        tail.extend_from_slice(&0i16.to_le_bytes());
        tail.extend_from_slice(&0i32.to_le_bytes());
        tail.push(0);
        tail.extend_from_slice(&write_vmad_string("StageFragment"));
        tail.extend_from_slice(&write_vmad_string("Fragment_0900"));
        tail.extend_from_slice(&1u16.to_le_bytes());
        tail.extend_from_slice(&[0; 8]);
        tail.extend_from_slice(&1i16.to_le_bytes());
        tail.extend_from_slice(&2i16.to_le_bytes());
        tail.extend_from_slice(&1u16.to_le_bytes());
        tail.extend_from_slice(&script_entry("AliasScript"));

        assert_eq!(
            vmad_script_names(&vmad(&["TopScript"], &tail), "QUST"),
            [
                "TopScript",
                "QuestFragmentOwner",
                "StageFragment",
                "AliasScript"
            ]
        );
    }

    #[test]
    fn malformed_fragment_tail_keeps_top_level_script_names() {
        assert_eq!(
            vmad_script_names(&vmad(&["TopScript"], b"broken tail"), "QUST"),
            ["TopScript"]
        );
    }

    #[test]
    fn frozen_python_oracle_bytes_match_alias_prune_and_condition_strip() {
        let alias_input =
            hex::decode("060002000100160044656661756C74325374617465416374697661746F72000000")
                .unwrap();
        let alias_record = record(
            0x0037_879E,
            "MSTT",
            "CoolingTowerFX",
            vec![subrecord("VMAD", alias_input)],
        );
        let (alias_record, replacement_count) = rewrite_vmad_script_aliases(&alias_record);
        assert_eq!(replacement_count, 1);
        assert_eq!(
            hex::encode_upper(&alias_record.subrecords[0].1),
            "060002000100160042323154776F5374617465416374697661746F723736000000"
        );

        let prune_input = hex::decode(
            "0600020002000C0042726F6B656E5363726970740000000D00576F726B696E675363726970740000006F70617175652D7461696C",
        )
        .unwrap();
        let prune_record = record(
            0x0057_27BB,
            "PERK",
            "Perk",
            vec![subrecord("VMAD", prune_input)],
        );
        let (pruned, note) = vmad_after_script_prune(
            &prune_record.subrecords[0].1,
            &prune_record,
            &HashSet::from(["brokenscript".to_string()]),
            false,
        );
        assert_eq!(
            hex::encode_upper(pruned.unwrap()),
            "0600020001000D00576F726B696E675363726970740000006F70617175652D7461696C"
        );
        assert_eq!(
            note.unwrap(),
            "PERK 005727BB Perk: dropped 1 failed binding(s) (BrokenScript); kept 1"
        );

        let condition_record = record(
            0x003C_4727,
            "INFO",
            "",
            vec![
                subrecord("CITC", hex::decode("01000000").unwrap()),
                subrecord(
                    "CTDA",
                    hex::decode("00000000000000007502000034A60107000000000000000000000000")
                        .unwrap(),
                ),
                subrecord(
                    "CIS2",
                    hex::decode("3A3A6952616E646F6D506172745F76617200").unwrap(),
                ),
                subrecord("NAM1", hex::decode("6B657074").unwrap()),
            ],
        );
        let scripts = HashMap::from([
            (0x0701_A634, vec!["MTNZ05QuestScript".to_string()]),
            (0x0001_A634, vec!["MTNZ05QuestScript".to_string()]),
        ]);
        let (condition_output, stripped_conditions, stripped_vmad, _notes) =
            subrecords_after_script_reconcile(
                &condition_record,
                &HashSet::from(["mtnz05questscript".to_string()]),
                &HashSet::new(),
                false,
                &scripts,
                false,
            )
            .unwrap();
        assert_eq!((stripped_conditions, stripped_vmad), (1, 0));
        assert_eq!(
            condition_output
                .iter()
                .map(|(signature, data, _)| (signature.as_str(), hex::encode_upper(data)))
                .collect::<Vec<_>>(),
            [
                ("CITC", "00000000".to_string()),
                ("NAM1", "6B657074".to_string())
            ]
        );
    }

    #[test]
    fn reconcile_uses_inspection_snapshot_after_intermediate_target_change() {
        let form_id = 0x0057_27BB;
        let inspection = inspect_fixture(
            vec![record(
                form_id,
                "PERK",
                "Perk",
                vec![
                    subrecord("VMAD", vmad(&["BrokenScript", "WorkingScript"], &[])),
                    subrecord("NAM1", b"snapshot".to_vec()),
                ],
            )],
            false,
        );
        let written = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&written);
        let result = reconcile_script_references(
            &inspection.state,
            vec![
                resolution("BrokenScript", "compile_failed", None),
                resolution("WorkingScript", "target", None),
            ],
            move |_form_id, subrecords| {
                captured.borrow_mut().push(subrecords);
                Ok(true)
            },
        )
        .unwrap();

        assert_eq!(result.changed_records, 1);
        assert_eq!(written.borrow()[0][1].1, b"snapshot");
        let parsed = parse_vmad(&written.borrow()[0][0].1).unwrap();
        assert_eq!(parsed.scripts[0].name, "WorkingScript");
    }

    #[test]
    fn reconcile_error_can_retry_all_snapshots_after_a_partial_write() {
        let inspection = inspect_fixture(
            vec![
                record(
                    0x0800,
                    "PERK",
                    "First",
                    vec![subrecord("VMAD", vmad(&["Broken", "Working"], &[]))],
                ),
                record(
                    0x0801,
                    "PERK",
                    "Second",
                    vec![subrecord("VMAD", vmad(&["Broken", "Working"], &[]))],
                ),
            ],
            false,
        );
        let writes = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&writes);
        let error = reconcile_script_references(
            &inspection.state,
            vec![
                resolution("Broken", "compile_failed", None),
                resolution("Working", "target", None),
            ],
            move |form_id, subrecords| {
                if captured.borrow().is_empty() {
                    captured.borrow_mut().push((form_id, subrecords));
                    Ok(true)
                } else {
                    Err("second write failed".to_string())
                }
            },
        )
        .unwrap_err();
        assert_eq!(error, "second write failed");
        assert_eq!(writes.borrow().len(), 1);

        let retry = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&retry);
        let result = reconcile_script_references(
            &inspection.state,
            vec![
                resolution("Broken", "compile_failed", None),
                resolution("Working", "target", None),
            ],
            move |form_id, subrecords| {
                captured.borrow_mut().push((form_id, subrecords));
                Ok(true)
            },
        )
        .unwrap();
        assert_eq!(result.changed_records, 2);
        assert_eq!(retry.borrow().len(), 2);
    }

    #[test]
    fn inferred_condition_strips_only_when_every_candidate_is_invalid() {
        let quest_id = 0x0701_A634;
        let quest = record(
            quest_id,
            "QUST",
            "Quest",
            vec![subrecord(
                "VMAD",
                vmad(&["BrokenQuestScript", "WorkingQuestScript"], &[]),
            )],
        );
        let info_id = 0x003C_4727;
        let info = record(
            info_id,
            "INFO",
            "Info",
            vec![
                subrecord("CITC", 1u32.to_le_bytes().to_vec()),
                subrecord("CTDA", ctda(629, quest_id)),
                subrecord("CIS2", b"::Member_var\0".to_vec()),
                subrecord("NAM1", b"kept".to_vec()),
            ],
        );
        let records = HashMap::from([(quest_id, quest), (info_id, info)]);
        let inspection = inspect_script_references(
            vec![info_id, quest_id],
            "SeventySix.esm",
            false,
            |form_id| Ok(records.get(&form_id).cloned()),
            |_form_id, _subrecords| Ok(true),
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let pex_path = temp.path().join("MemberTest.pex");
        compile_test_pex(&pex_path);
        let writes = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&writes);
        reconcile_script_references(
            &inspection.state,
            vec![
                resolution("BrokenQuestScript", "compiled", Some(&pex_path)),
                resolution("WorkingQuestScript", "target", None),
            ],
            move |form_id, subrecords| {
                captured.borrow_mut().push((form_id, subrecords));
                Ok(true)
            },
        )
        .unwrap();
        assert!(writes.borrow().is_empty());

        let stripped = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&stripped);
        let result = reconcile_script_references(
            &inspection.state,
            vec![
                resolution("BrokenQuestScript", "compiled", Some(&pex_path)),
                resolution("WorkingQuestScript", "compiled", Some(&pex_path)),
            ],
            move |form_id, subrecords| {
                captured.borrow_mut().push((form_id, subrecords));
                Ok(true)
            },
        )
        .unwrap();
        assert_eq!(result.stripped_conditions, 1);
        let info_write = stripped
            .borrow()
            .iter()
            .find(|(form_id, _)| *form_id == info_id)
            .cloned()
            .unwrap();
        assert_eq!(
            info_write
                .1
                .iter()
                .map(|(signature, _, _)| signature.as_str())
                .collect::<Vec<_>>(),
            ["CITC", "NAM1"]
        );
        assert_eq!(&info_write.1[0].1[0..4], &0u32.to_le_bytes());
    }

    #[test]
    fn failed_update_keeps_notes_but_not_change_counts() {
        let inspection = inspect_fixture(
            vec![record(
                0x0057_27BB,
                "PERK",
                "Perk",
                vec![subrecord("VMAD", vmad(&["BrokenScript"], b"opaque"))],
            )],
            false,
        );
        let result = reconcile_script_references(
            &inspection.state,
            vec![resolution("BrokenScript", "compile_failed", None)],
            |_form_id, _subrecords| Ok(false),
        )
        .unwrap();
        assert_eq!(result.changed_records, 0);
        assert_eq!(result.stripped_vmad, 0);
        assert_eq!(result.vmad_notes.len(), 1);
    }

    #[test]
    fn reward_attachment_is_idempotent_and_reports_unreached_quests() {
        let quest_id = 0x0754_F1A4;
        let inspection = inspect_fixture(
            vec![record(
                quest_id,
                "QUST",
                "COMP_RQ_Fetch",
                vec![subrecord("VMAD", vmad(&["QuestScript"], &[]))],
            )],
            true,
        );
        let writes = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&writes);
        let result = reconcile_script_references(
            &inspection.state,
            vec![resolution("QuestScript", "target", None)],
            move |_form_id, subrecords| {
                captured.borrow_mut().push(subrecords);
                Ok(true)
            },
        )
        .unwrap();
        assert_eq!(result.changed_records, 1);
        assert_eq!(result.unreached_reward_notes.len(), 2);
        let reward_vmad = &writes.borrow()[0][0].1;
        assert!(
            parse_vmad(reward_vmad)
                .unwrap()
                .scripts
                .iter()
                .any(|entry| entry.name == QUEST_REWARD_SCRIPT_NAME)
        );
        let (twice, note) = vmad_with_quest_reward_script(
            reward_vmad,
            inspection.state.candidate_records.get(&quest_id).unwrap(),
        )
        .unwrap();
        assert_eq!(twice, *reward_vmad);
        assert!(note.is_none());
    }

    #[test]
    fn reward_script_parse_failure_prevents_reattachment() {
        let quest_id = 0x0754_F1A4;
        let inspection = inspect_fixture(
            vec![record(
                quest_id,
                "QUST",
                "COMP_RQ_Fetch",
                vec![
                    subrecord(
                        "VMAD",
                        vmad(&[QUEST_REWARD_SCRIPT_NAME, "WorkingScript"], &[]),
                    ),
                    subrecord("CTDA", ctda(629, quest_id)),
                    subrecord("CIS1", b"B21:QuestRewards\0".to_vec()),
                    subrecord("CIS2", b"::Missing_var\0".to_vec()),
                ],
            )],
            true,
        );
        let temp = tempfile::tempdir().unwrap();
        let broken_pex = temp.path().join("QuestRewards.pex");
        std::fs::write(&broken_pex, b"not a pex").unwrap();
        let writes = Rc::new(RefCell::new(Vec::new()));
        let captured = Rc::clone(&writes);

        let result = reconcile_script_references(
            &inspection.state,
            vec![
                resolution(QUEST_REWARD_SCRIPT_NAME, "compiled", Some(&broken_pex)),
                resolution("WorkingScript", "target", None),
            ],
            move |_form_id, subrecords| {
                captured.borrow_mut().push(subrecords);
                Ok(true)
            },
        )
        .unwrap();

        assert_eq!(result.parse_failures.len(), 1);
        assert_eq!(result.stripped_vmad, 0);
        let payload = &writes.borrow()[0][0].1;
        assert_eq!(vmad_script_names(payload, "QUST"), ["WorkingScript"]);
        assert!(
            !result
                .vmad_notes
                .iter()
                .any(|note| note.contains("attached B21:QuestRewards"))
        );
    }

    #[test]
    fn pex_member_cache_key_matches_platform_normcase_behavior() {
        let mixed = pex_member_cache_key(Path::new("Folder/Script.pex"));
        #[cfg(windows)]
        assert_eq!(mixed, pex_member_cache_key(Path::new("folder\\SCRIPT.PEX")));
        #[cfg(not(windows))]
        assert_ne!(mixed, pex_member_cache_key(Path::new("folder/Script.pex")));
    }

    #[test]
    fn unreadable_vmad_is_preserved_with_existing_error_text() {
        let record = record(
            0x002A_F222,
            "FURN",
            "76CharGenFaceChair",
            vec![subrecord(
                "VMAD",
                b"\x06\x00\x02\x00opaque VMAD payload".to_vec(),
            )],
        );
        let (replacement, note) = vmad_after_script_prune(
            &record.subrecords[0].1,
            &record,
            &HashSet::from(["chargenfacechairscript".to_string()]),
            false,
        );
        assert_eq!(replacement.unwrap(), record.subrecords[0].1);
        assert!(
            note.unwrap()
                .starts_with("FURN 002AF222 76CharGenFaceChair: left VMAD untouched, unreadable")
        );
    }
}
