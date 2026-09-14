//! QUST translation.
//!
//! The compatibility API extracts legacy stage scripts for the Papyrus
//! pipeline. `lower_qust_record` independently rebuilds a typed FO4 QUST
//! payload and leaves VMAD attachment to post-compile reconciliation.

use std::collections::HashSet;

use encoding_rs::WINDOWS_1252;
use regex::Regex;
use serde_json::{Map, Value, json};
use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue};
use crate::translator::pair_hooks::fnv_conditions::{
    LegacyConditionFamily, normalize_legacy_condition_scope,
};
use crate::translator::pair_hooks::fnv_fo4::PlacedActorAliasResolver;

use super::form_keys::object_id_from_form_key;
use super::naming::quest_fragment_name;
use super::quest_alias::{
    LegacyRawFormIdResolver, StableQuestAlias, alias_id_for_source, allocate_stable_aliases,
    resolve_legacy_target,
};
use super::quest_ir::{
    LegacyConditionScope, LegacyQuestIr, QuestFragmentMetadata, QuestLossAccounting,
    QuestLoweringError,
};
use super::{FnvScriptContext, TranslateError, translate_to_papyrus};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single translated quest stage fragment.
#[derive(Debug, Clone)]
pub struct StageFragment {
    pub stage_index: i32,
    pub stage_item_index: u16,
    pub psc_function_name: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestFragmentProperty {
    pub name: String,
    pub papyrus_type: String,
    pub source_form_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestFragmentAdaptationKind {
    TargetNativeNoOp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestFragmentAdaptation {
    pub stage_index: u16,
    pub stage_item_index: u16,
    pub source_command: String,
    pub source_dependency_form_key: String,
    pub kind: QuestFragmentAdaptationKind,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestRecordDependencyMapping {
    pub scope: String,
    pub evidence_hex: String,
    pub source_dependency_form_key: String,
    pub target_dependency_form_key: String,
}

/// Result of translating one QUST record.
#[derive(Debug, Clone)]
pub struct TranslatedQuest {
    pub source_editor_id: String,
    pub source_form_key: String,
    pub fragment_class_name: String,
    /// Papyrus `.psc` source text (caller writes this to disk if needed).
    pub fragment_psc_text: String,
    pub aliases: Vec<AliasEntry>,
    pub stage_fragments: Vec<StageFragment>,
    pub fragment_properties: Vec<QuestFragmentProperty>,
    pub fragment_adaptations: Vec<QuestFragmentAdaptation>,
    pub unresolved_reference_names: Vec<String>,
    /// Compatibility payload with legacy runtime fields removed. New callers
    /// should use `lower_qust_record` for an FO4-shaped record.
    pub authoring_record_payload: Option<Value>,
    pub warnings: Vec<String>,
}

/// Alias synthesized for a reference variable discovered in stage scripts.
#[derive(Debug, Clone)]
pub struct AliasEntry {
    pub name: String,
    pub fill_type: String,
    pub flags: Vec<String>,
}

/// Typed QUST lowering output consumed by the record writer and the separate
/// Papyrus/VMAD owner. The record payload contains no legacy script stream.
#[derive(Debug, Clone)]
pub struct QuestTranslation {
    pub source_editor_id: String,
    pub source_form_key: String,
    pub target_form_key: String,
    pub fragment_class_name: String,
    pub authoring_record_payload: Value,
    pub aliases: Vec<StableQuestAlias>,
    pub fragment_metadata: Vec<QuestFragmentMetadata>,
    pub losses: QuestLossAccounting,
}

fn authoring_form_reference(plugin: &str, object_id: u32) -> Value {
    json!({
        "reference": {
            "plugin": plugin,
            "object_id": format!("{object_id:06X}")
        }
    })
}

fn parse_authoring_form_reference(form_key: &str, field: &str) -> Result<Value, String> {
    let (object_id, plugin) = form_key
        .trim()
        .split_once(':')
        .ok_or_else(|| format!("{field} requires an object-id:plugin FormKey"))?;
    let plugin = plugin.trim();
    if plugin.is_empty() {
        return Err(format!("{field} FormKey has no plugin"));
    }
    let object_id = object_id
        .trim()
        .strip_prefix("0x")
        .or_else(|| object_id.trim().strip_prefix("0X"))
        .unwrap_or(object_id.trim());
    let object_id = u32::from_str_radix(object_id, 16)
        .map_err(|_| format!("{field} FormKey has an invalid object id"))?;
    if object_id == 0 || object_id > 0x00FF_FFFF {
        return Err(format!(
            "{field} FormKey object id must be in 000001..FFFFFF"
        ));
    }
    Ok(authoring_form_reference(plugin, object_id))
}

fn compact_quest_fragment_class_name(
    mod_prefix: &str,
    source_form_key: &str,
) -> Result<String, String> {
    let object_id = object_id_from_form_key(source_form_key);
    let object_id = object_id.strip_prefix("0X").unwrap_or(object_id.as_str());
    let source_local = u32::from_str_radix(object_id, 16)
        .map_err(|_| format!("invalid quest source FormKey '{source_form_key}'"))?;
    if source_local > 0x00FF_FFFF {
        return Err(format!(
            "quest source FormKey '{source_form_key}' is not a local object id"
        ));
    }
    let class_name = quest_fragment_name(mod_prefix, source_local);
    if class_name.len() > 38 {
        return Err(format!(
            "quest fragment class '{class_name}' exceeds the 38-character Papyrus limit"
        ));
    }
    Ok(class_name)
}

pub fn append_unfilled_package_data_alias(
    translation: &mut QuestTranslation,
    alias_name: &str,
    target_package_form_key: &str,
) -> Result<i32, String> {
    if alias_name.trim().is_empty() || target_package_form_key.trim().is_empty() {
        return Err("package data alias requires a name and mapped PACK FormKey".into());
    }
    let target_package_reference = parse_authoring_form_reference(target_package_form_key, "ALPC")?;
    let fields = translation
        .authoring_record_payload
        .get_mut("fields")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "lowered QUST payload has no fields array".to_string())?;
    if fields.iter().any(|field| {
        field
            .get("ALID")
            .and_then(Value::as_str)
            .is_some_and(|name| name.eq_ignore_ascii_case(alias_name))
    }) {
        return Err(format!("QUST data alias '{alias_name}' already exists"));
    }
    let alias_id = fields
        .iter()
        .filter_map(|field| field.get("ALST").and_then(Value::as_i64))
        .max()
        .unwrap_or(-1)
        .checked_add(1)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| "QUST alias id overflow".to_string())?;
    let anam = fields
        .iter_mut()
        .find_map(|field| field.get_mut("ANAM"))
        .ok_or_else(|| "lowered QUST payload has no ANAM alias count".to_string())?;
    let prior_count = anam
        .as_u64()
        .ok_or_else(|| "lowered QUST ANAM alias count is not an integer".to_string())?;
    *anam = json!(prior_count + 1);
    fields.extend([
        json!({ "ALST": alias_id }),
        json!({ "ALID": alias_name }),
        json!({ "FNAM": 0 }),
        json!({ "ALPC": target_package_reference }),
        json!({ "ALED": null }),
    ]);
    Ok(alias_id)
}

/// Lower a legacy FNV/FO3 QUST into an FO4-shaped authoring payload.
///
/// `raw_formids` resolves direct legacy QSTA raw FormIDs to their source
/// identity. `placed_aliases` then proves that the referenced placement has an
/// emitted FO4 ACHR identity. Both resolutions are required; neither is guessed.
pub(crate) fn lower_qust_record(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
    target_form_key: &str,
    family: LegacyConditionFamily,
    mapper: &mut FormKeyMapper<'_>,
    raw_formids: &dyn LegacyRawFormIdResolver,
    placed_aliases: &PlacedActorAliasResolver,
) -> Result<QuestTranslation, QuestLoweringError> {
    let mut quest = LegacyQuestIr::parse(record)?;
    let interner = mapper.interner;
    let mut resolved_targets = Vec::new();
    for objective in &quest.objectives {
        for target in &objective.targets {
            let resolved =
                resolve_legacy_target(&target.identity, raw_formids, placed_aliases, interner)
                    .map_err(|error| QuestLoweringError::MissingRequiredTarget {
                        objective: objective.index,
                        reason: error.to_string(),
                    })?;
            resolved_targets.push(resolved);
        }
    }
    let aliases = allocate_stable_aliases(resolved_targets.iter().copied(), interner);
    let fragment_class_name = compact_quest_fragment_class_name(mod_prefix, source_form_key)
        .map_err(|reason| QuestLoweringError::MalformedField {
            signature: "form_id".into(),
            reason,
        })?;
    let payload = emit_fo4_quest_payload(
        &mut quest,
        target_form_key,
        family,
        mapper,
        raw_formids,
        placed_aliases,
        &aliases,
    )?;
    Ok(QuestTranslation {
        source_editor_id: quest.editor_id,
        source_form_key: source_form_key.to_string(),
        target_form_key: target_form_key.to_string(),
        fragment_class_name,
        authoring_record_payload: payload,
        aliases,
        fragment_metadata: quest.fragments,
        losses: quest.losses,
    })
}

// ---------------------------------------------------------------------------
// synthesize_aliases_for_refs
// ---------------------------------------------------------------------------

/// Build alias entries for every unique reference variable name found in stage
/// scripts (matches Python's `synthesize_aliases_for_refs`).
pub fn synthesize_aliases_for_refs(ref_var_names: &[String]) -> Vec<AliasEntry> {
    let mut seen: HashSet<&str> = HashSet::new();
    ref_var_names
        .iter()
        .filter(|name| seen.insert(name.as_str()))
        .map(|name| AliasEntry {
            name: name.clone(),
            fill_type: "specific_reference".into(),
            flags: vec![],
        })
        .collect()
}

// ---------------------------------------------------------------------------
// translate_qust_record
// ---------------------------------------------------------------------------

/// Translate a single QUST record value.
///
/// `ctx` must already be loaded (`FnvScriptContext::load()`).
pub fn translate_qust_record(
    record: &Value,
    mod_prefix: &str,
    strict: bool,
    source_form_key: &str,
) -> Result<TranslatedQuest, TranslateError> {
    let eid = record_eid(record);
    let class_name = compact_quest_fragment_class_name(mod_prefix, source_form_key)
        .map_err(TranslateError::Semantic)?;
    let fragment_adaptations =
        exact_target_native_quest_fragment_adaptations(record, &eid, source_form_key)?;

    let ctx = FnvScriptContext::load_for_exact_slice_record(&eid, source_form_key, mod_prefix)
        .map_err(|e| TranslateError::Semantic(format!("load ctx: {e}")))?;

    let mut fragments: Vec<StageFragment> = Vec::new();
    let mut raw_fragment_sources: Vec<String> = Vec::new();
    let mut current_stage: Option<i32> = None;
    let mut current_stage_item: Option<u16> = None;
    let mut next_stage_item = 0_u16;

    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            if let Some(indx) = field_value(field, "INDX") {
                current_stage = indx.as_i64().map(|n| n as i32);
                current_stage_item = None;
                next_stage_item = 0;
                continue;
            }
            if field_value(field, "QSDT").is_some() {
                current_stage_item = Some(next_stage_item);
                next_stage_item = next_stage_item.saturating_add(1);
                continue;
            }
            if let Some(sctx) = field_value(field, "SCTX") {
                if let Some(source) = decode_script_source(sctx) {
                    let source = source.trim();
                    if !source.is_empty() {
                        if let Some(stage) = current_stage {
                            let stage_item_index = current_stage_item.unwrap_or(0);
                            let normalized_source = normalize_quest_fragment_command_syntax(
                                source,
                                &eid,
                                source_form_key,
                                stage,
                                stage_item_index,
                                &fragment_adaptations,
                            );
                            let wrapped = format!("begin GameMode\n{normalized_source}\nend\n");
                            let papyrus =
                                translate_to_papyrus(&wrapped, &ctx, &class_name, "Quest")?;
                            let body = extract_first_event_body(&papyrus);
                            fragments.push(StageFragment {
                                stage_index: stage,
                                stage_item_index,
                                psc_function_name: format!(
                                    "Fragment_Stage_{stage:04}_Item_{stage_item_index:02}"
                                ),
                                body,
                            });
                            raw_fragment_sources.push(source.to_string());
                            current_stage_item = None;
                        }
                    }
                }
            }
        }
    }

    let alias_names = collect_reference_names(&raw_fragment_sources);
    let aliases = synthesize_aliases_for_refs(&alias_names);
    let fragment_properties =
        exact_quest_fragment_properties(record, &eid, source_form_key, mod_prefix)?;
    let psc_text = build_quest_psc(&class_name, &fragments, &fragment_properties);

    let authoring_payload = build_quest_payload(record, strict);

    Ok(TranslatedQuest {
        source_editor_id: eid,
        source_form_key: source_form_key.to_string(),
        fragment_class_name: class_name,
        fragment_psc_text: psc_text,
        aliases,
        stage_fragments: fragments,
        fragment_properties,
        fragment_adaptations,
        unresolved_reference_names: alias_names,
        authoring_record_payload: Some(authoring_payload),
        warnings: vec![],
    })
}

fn emit_fo4_quest_payload(
    quest: &mut LegacyQuestIr,
    target_form_key: &str,
    family: LegacyConditionFamily,
    mapper: &mut FormKeyMapper<'_>,
    raw_formids: &dyn LegacyRawFormIdResolver,
    placed_aliases: &PlacedActorAliasResolver,
    aliases: &[StableQuestAlias],
) -> Result<Value, QuestLoweringError> {
    let mut fields = Vec::new();
    if let Some(name) = &quest.name {
        fields.push(json!({ "FULL": name }));
    }
    let target_flags = lower_general_flags(quest.general.flags, &mut quest.losses);
    fields.push(json!({
        "DNAM": {
            "Flags": target_flags,
            "Priority": quest.general.priority,
            "UnknownByte3": 0,
            "DelayTime": quest.general.delay_time,
            "Type": 0,
            "UnknownByte6": 0,
            "UnknownByte7": 0,
            "UnknownByte8": 0
        }
    }));
    append_conditions(
        &mut fields,
        &quest.start_conditions,
        "start",
        family,
        mapper,
    )?;

    for stage in &quest.stages {
        fields.push(json!({
            "INDX": {
                "StageIndex": stage.index,
                "Flags": 0,
                "Unknown": 0
            }
        }));
        for (entry_index, entry) in stage.entries.iter().enumerate() {
            fields.push(json!({ "QSDT": entry.flags }));
            append_conditions(
                &mut fields,
                &entry.conditions,
                &format!("stage:{}:item:{entry_index}", stage.index),
                family,
                mapper,
            )?;
            if let Some(note) = &entry.note {
                fields.push(json!({ "NAM2": note }));
            }
            if let Some(log_entry) = &entry.log_entry {
                fields.push(json!({ "CNAM": log_entry }));
            }
        }
    }

    for objective in &quest.objectives {
        fields.push(json!({ "QOBJ": objective.index }));
        fields.push(json!({ "FNAM": objective.flags }));
        if let Some(display_text) = &objective.display_text {
            fields.push(json!({ "NNAM": display_text }));
        }
        for (target_index, target) in objective.targets.iter().enumerate() {
            let resolved = resolve_legacy_target(
                &target.identity,
                raw_formids,
                placed_aliases,
                mapper.interner,
            )
            .map_err(|error| QuestLoweringError::MissingRequiredTarget {
                objective: objective.index,
                reason: error.to_string(),
            })?;
            let alias = alias_id_for_source(aliases, resolved.source_placed).ok_or_else(|| {
                QuestLoweringError::MissingRequiredTarget {
                    objective: objective.index,
                    reason: format!(
                        "stable alias missing for placed {:06X}",
                        resolved.source_placed.local
                    ),
                }
            })?;
            fields.push(json!({
                "QSTA": {
                    "Alias": alias,
                    "Flags": u32::from(target.flags & 1),
                    "Keyword": null
                }
            }));
            append_conditions(
                &mut fields,
                &target.conditions,
                &format!("objective:{}:target:{target_index}", objective.index),
                family,
                mapper,
            )?;
        }
    }

    fields.push(json!({ "ANAM": aliases.len() }));
    for alias in aliases {
        let target_plugin = mapper
            .interner
            .resolve(alias.target_placed.plugin)
            .filter(|plugin| !plugin.is_empty())
            .ok_or_else(|| QuestLoweringError::MalformedField {
                signature: "ALFR".into(),
                reason: format!(
                    "alias {} target plugin identity is unavailable",
                    alias.alias_id
                ),
            })?;
        if alias.target_placed.local == 0 || alias.target_placed.local > 0x00FF_FFFF {
            return Err(QuestLoweringError::MalformedField {
                signature: "ALFR".into(),
                reason: format!(
                    "alias {} target object id {:08X} is outside 000001..FFFFFF",
                    alias.alias_id, alias.target_placed.local
                ),
            });
        }
        fields.push(json!({ "ALST": alias.alias_id }));
        fields.push(json!({ "ALID": alias.name }));
        fields.push(json!({ "FNAM": 0 }));
        fields.push(json!({
            "ALFR": authoring_form_reference(target_plugin, alias.target_placed.local)
        }));
        fields.push(json!({ "ALED": null }));
    }

    Ok(json!({
        "signature": "QUST",
        "form_id": target_form_key,
        "form_version": 131,
        "version2": 5,
        "eid": quest.editor_id,
        "fields": fields
    }))
}

fn lower_general_flags(source: u8, losses: &mut QuestLossAccounting) -> u16 {
    let mut target = 0_u16;
    if source & 0x01 != 0 {
        target |= 0x01;
    }
    if source & 0x08 != 0 {
        target |= 0x08;
    }
    if source & 0x02 != 0 {
        losses.terminal(
            "record",
            "DATA.flags:0x02",
            "legacy flag meaning is unknown",
        );
    }
    if source & 0x04 != 0 {
        losses.externalized(
            "record",
            "DATA.flags:0x04",
            "AllowRepeatedConversationTopics is preserved by leaving converted INFO responses repeatable",
        );
    }
    if source & 0x10 != 0 {
        losses.externalized(
            "record",
            "DATA.flags:0x10",
            "legacy script-processing delay is owned by Papyrus lowering",
        );
    }
    let unknown = source & !0x1f;
    if unknown != 0 {
        losses.terminal(
            "record",
            &format!("DATA.flags:0x{unknown:02X}"),
            "legacy flag bits have no verified FO4 QUST mapping",
        );
    }
    target
}

fn decode_script_source(value: &Value) -> Option<String> {
    if let Some(source) = value.as_str() {
        return Some(source.trim_end_matches('\0').to_string());
    }
    let bytes = super::quest_ir::raw_bytes(value)?;
    let (decoded, _, _) = WINDOWS_1252.decode(&bytes);
    Some(decoded.trim_end_matches('\0').to_string())
}

pub(crate) fn normalize_fragment_command_syntax(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let (code, comment) = line
                .split_once(';')
                .map_or((line, None), |(code, comment)| (code, Some(comment)));
            let trimmed = code.trim();
            let mut parts = trimmed.split_whitespace();
            let Some(command) = parts.next() else {
                return line.to_string();
            };
            if command.contains('(') {
                return line.to_string();
            }
            let command_name = command.rsplit('.').next().unwrap_or(command);
            let expected_args = match command_name.to_ascii_lowercase().as_str() {
                "addperk"
                | "completequest"
                | "rewardxp"
                | "setpccanusepowerarmor"
                | "showmessage"
                | "stopquest" => 1,
                "setstage" => 2,
                "addreputation" | "setobjectivecompleted" | "setobjectivedisplayed" => 3,
                _ => return line.to_string(),
            };
            let args = parts.collect::<Vec<_>>();
            if args.len() != expected_args {
                return line.to_string();
            }
            let indentation = &code[..code.len() - code.trim_start().len()];
            let mut normalized = format!("{indentation}{command}({})", args.join(", "));
            if let Some(comment) = comment {
                normalized.push(';');
                normalized.push_str(comment);
            }
            normalized
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_quest_fragment_command_syntax(
    source: &str,
    quest_editor_id: &str,
    source_form_key: &str,
    stage_index: i32,
    stage_item_index: u16,
    adaptations: &[QuestFragmentAdaptation],
) -> String {
    if adaptations.iter().any(|adaptation| {
        adaptation.stage_index == u16::try_from(stage_index).unwrap_or(u16::MAX)
            && adaptation.stage_item_index == stage_item_index
            && adaptation.kind == QuestFragmentAdaptationKind::TargetNativeNoOp
    }) {
        return "Return".to_string();
    }
    let source = source
        .lines()
        .map(|line| {
            let (code, comment) = line
                .split_once(';')
                .map_or((line, None), |(code, comment)| (code, Some(comment)));
            let mut parts = code.split_whitespace().collect::<Vec<_>>();
            if exact_quest_source(source_form_key, 0x11F935)
                && quest_editor_id.eq_ignore_ascii_case("VTechatticup")
                && parts
                    .first()
                    .is_some_and(|command| command.eq_ignore_ascii_case("AddReputation"))
                && parts.len() == 4
                && parts[1].eq_ignore_ascii_case("RepNVNCR")
            {
                let indentation = &code[..code.len() - code.trim_start().len()];
                let mut normalized =
                    format!("{indentation}ModRepNVNCR({}, {})", parts[2], parts[3]);
                if let Some(comment) = comment {
                    normalized.push(';');
                    normalized.push_str(comment);
                }
                return normalized;
            }
            let targets_quest = parts.first().is_some_and(|command| {
                matches!(
                    command.to_ascii_lowercase().as_str(),
                    "completequest"
                        | "setobjectivecompleted"
                        | "setobjectivedisplayed"
                        | "setstage"
                        | "stopquest"
                )
            });
            if targets_quest
                && parts
                    .get(1)
                    .is_some_and(|target| target.eq_ignore_ascii_case(quest_editor_id))
            {
                parts[1] = "Self";
                let indentation = &code[..code.len() - code.trim_start().len()];
                let mut normalized = format!("{indentation}{}", parts.join(" "));
                if let Some(comment) = comment {
                    normalized.push(';');
                    normalized.push_str(comment);
                }
                normalized
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    normalize_fragment_command_syntax(&source)
}

pub fn exact_target_native_quest_fragment_adaptations(
    record: &Value,
    editor_id: &str,
    source_form_key: &str,
) -> Result<Vec<QuestFragmentAdaptation>, TranslateError> {
    if !exact_quest_source(source_form_key, 0x11F935)
        || !editor_id.eq_ignore_ascii_case("VTechatticup")
    {
        return Ok(Vec::new());
    }
    let Some(fields) = record.get("fields").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut current_stage = None;
    let mut next_stage_item = 0_u16;
    let mut saw_exact_slot = false;
    let mut adaptations = Vec::new();
    let mut index = 0;
    while index < fields.len() {
        if let Some(stage) = field_value(&fields[index], "INDX").and_then(Value::as_u64) {
            current_stage = u16::try_from(stage).ok();
            next_stage_item = 0;
            index += 1;
            continue;
        }
        if field_value(&fields[index], "QSDT").is_none() {
            index += 1;
            continue;
        }
        let stage_item_index = next_stage_item;
        next_stage_item = next_stage_item.saturating_add(1);
        let entry_start = index + 1;
        let entry_end = (entry_start..fields.len())
            .find(|candidate| {
                ["QSDT", "INDX", "QOBJ", "ANAM", "ALST", "ALLS", "ALCS"]
                    .iter()
                    .any(|signature| field_value(&fields[*candidate], signature).is_some())
            })
            .unwrap_or(fields.len());
        let sources = fields[entry_start..entry_end]
            .iter()
            .filter_map(|field| field_value(field, "SCTX"))
            .filter_map(decode_script_source)
            .collect::<Vec<_>>();
        let is_exact_slot = current_stage == Some(110) && stage_item_index == 0;
        if is_exact_slot {
            saw_exact_slot = true;
            if sources.as_slice() != ["StopQuest VMS20"] {
                return Err(TranslateError::Semantic(format!(
                    "QUST 'VTechatticup' stage 110 item 0 target-native adaptation requires exact `StopQuest VMS20`, observed {sources:?}"
                )));
            }
            let references = fields[entry_start..entry_end]
                .iter()
                .filter_map(|field| field_value(field, "SCRO"))
                .filter_map(quest_form_key)
                .collect::<Vec<_>>();
            if references.as_slice() != ["10E908:FalloutNV.esm"] {
                return Err(TranslateError::Semantic(format!(
                    "QUST 'VTechatticup' stage 110 item 0 target-native adaptation requires only SCRO 10E908:FalloutNV.esm, observed {references:?}"
                )));
            }
            adaptations.push(QuestFragmentAdaptation {
                stage_index: 110,
                stage_item_index: 0,
                source_command: "StopQuest VMS20".into(),
                source_dependency_form_key: references[0].clone(),
                kind: QuestFragmentAdaptationKind::TargetNativeNoOp,
                reason: "VMS20 is outside the selected slice; the selected quest's FailQuest stage remains authoritative in FO4".into(),
            });
        } else if sources.iter().any(|source| source == "StopQuest VMS20") {
            return Err(TranslateError::Semantic(format!(
                "QUST 'VTechatticup' target-native VMS20 adaptation moved from stage 110 item 0 to stage {:?} item {stage_item_index}",
                current_stage
            )));
        }
        index = entry_end;
    }
    if saw_exact_slot && adaptations.len() != 1 {
        return Err(TranslateError::Semantic(
            "QUST 'VTechatticup' stage 110 item 0 target-native adaptation was not accounted"
                .into(),
        ));
    }
    Ok(adaptations)
}

pub fn exact_quest_record_dependency_mappings(
    record: &Value,
    editor_id: &str,
    source_form_key: &str,
) -> Result<Vec<QuestRecordDependencyMapping>, TranslateError> {
    if !exact_quest_source(source_form_key, 0x06136D)
        || !editor_id.eq_ignore_ascii_case("FreeformPowerArmor")
    {
        return Ok(Vec::new());
    }
    const ROOT_CTDA_HEX: &str = "0000000000000000C1010000DF8F0500000000000200000014000000";
    let root_conditions = record
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take_while(|field| field_value(field, "INDX").is_none())
        .filter_map(|field| field_value(field, "CTDA"))
        .filter_map(super::quest_ir::raw_bytes)
        .map(hex::encode_upper)
        .collect::<Vec<_>>();
    if root_conditions.as_slice() != [ROOT_CTDA_HEX] {
        return Err(TranslateError::Semantic(format!(
            "QUST 'FreeformPowerArmor' root Player dependency requires exact CTDA {ROOT_CTDA_HEX}, observed {root_conditions:?}"
        )));
    }
    Ok(vec![QuestRecordDependencyMapping {
        scope: "root CTDA run-on reference".into(),
        evidence_hex: ROOT_CTDA_HEX.into(),
        source_dependency_form_key: "000014:FalloutNV.esm".into(),
        target_dependency_form_key: "000014:Fallout4.esm".into(),
    }])
}

fn append_conditions(
    output: &mut Vec<Value>,
    conditions: &[LegacyConditionScope],
    scope_name: &str,
    family: LegacyConditionFamily,
    mapper: &mut FormKeyMapper<'_>,
) -> Result<(), QuestLoweringError> {
    for condition in conditions {
        let native = condition_scope_to_native(condition, mapper.interner)?;
        let (normalized, _) =
            normalize_legacy_condition_scope(&native, family, mapper).map_err(|error| {
                QuestLoweringError::ConditionLowering {
                    scope: scope_name.to_string(),
                    reason: format!("{error:?}"),
                }
            })?;
        for (index, field) in normalized.iter().enumerate() {
            let value = if field.sig.0 == *b"CTDA" {
                let FieldValue::Bytes(bytes) = &field.value else {
                    return Err(QuestLoweringError::ConditionLowering {
                        scope: scope_name.to_string(),
                        reason: "normalized CTDA is not raw bytes".to_string(),
                    });
                };
                json!({ "raw_hex": hex::encode_upper(bytes) })
            } else {
                condition.fields[index].1.clone()
            };
            output.push(json!({ field.sig.as_str(): value }));
        }
    }
    Ok(())
}

fn condition_scope_to_native(
    condition: &LegacyConditionScope,
    interner: &crate::sym::StringInterner,
) -> Result<Vec<FieldEntry>, QuestLoweringError> {
    condition
        .fields
        .iter()
        .map(|(signature, value)| {
            let sig = SubrecordSig::from_str(signature).map_err(|_| {
                QuestLoweringError::MalformedField {
                    signature: signature.clone(),
                    reason: "invalid condition signature".to_string(),
                }
            })?;
            let value = if let Some(bytes) = super::quest_ir::raw_bytes(value) {
                FieldValue::Bytes(SmallVec::from_vec(bytes))
            } else if let Some(text) = value.as_str() {
                FieldValue::String(interner.intern(text))
            } else {
                return Err(QuestLoweringError::MalformedField {
                    signature: signature.clone(),
                    reason: "condition field must be raw bytes or string".to_string(),
                });
            };
            Ok(FieldEntry { sig, value })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// PSC builder
// ---------------------------------------------------------------------------

fn build_quest_psc(
    class_name: &str,
    fragments: &[StageFragment],
    properties: &[QuestFragmentProperty],
) -> String {
    let mut out = format!("ScriptName {class_name} extends Quest\n\n");
    for property in properties {
        out.push_str(&format!(
            "{} Property {} Auto Const\n",
            property.papyrus_type, property.name
        ));
    }
    if !properties.is_empty() {
        out.push('\n');
    }
    for fragment in fragments {
        out.push_str(&format!("Function {}()\n", fragment.psc_function_name));
        for line in indent_fragment_lines(&fragment.body) {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("EndFunction\n\n");
    }
    out.trim_end().to_string() + "\n"
}

fn exact_quest_fragment_properties(
    record: &Value,
    editor_id: &str,
    source_form_key: &str,
    mod_prefix: &str,
) -> Result<Vec<QuestFragmentProperty>, TranslateError> {
    let exact_freeform = exact_quest_source(source_form_key, 0x06136D)
        && editor_id.eq_ignore_ascii_case("FreeformPowerArmor");
    let exact_vtech = exact_quest_source(source_form_key, 0x11F935)
        && editor_id.eq_ignore_ascii_case("VTechatticup");
    if !exact_freeform && !exact_vtech {
        return Ok(Vec::new());
    }
    let mut properties = vec![QuestFragmentProperty {
        name: "FNVSliceCompat".into(),
        papyrus_type: format!("{mod_prefix}_FnvSliceCompat"),
        source_form_key: source_form_key.into(),
    }];
    if exact_vtech {
        return Ok(properties);
    }
    let references = record
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|field| field_value(field, "SCRO"))
        .filter_map(quest_form_key)
        .collect::<Vec<_>>();
    for (name, papyrus_type, local) in [
        ("PowerArmorTraining", "Perk", 0x058FDF),
        ("PowerArmorTrainingPerkMsg", "Message", 0x070A14),
    ] {
        let matches = references
            .iter()
            .filter(|reference| exact_quest_source(reference, local))
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(TranslateError::Semantic(format!(
                "QUST 'FreeformPowerArmor' requires one audited {papyrus_type} SCRO {local:06X}, observed {}",
                matches.len()
            )));
        }
        properties.push(QuestFragmentProperty {
            name: name.into(),
            papyrus_type: papyrus_type.into(),
            source_form_key: (*matches[0]).clone(),
        });
    }
    Ok(properties)
}

fn quest_form_key(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }
    let reference = value.get("reference").unwrap_or(value);
    if let (Some(plugin), Some(object_id)) = (
        reference.get("plugin").and_then(Value::as_str),
        reference.get("object_id").and_then(Value::as_str),
    ) {
        return Some(format!("{object_id}:{plugin}"));
    }
    let bytes = super::quest_ir::raw_bytes(value)?;
    (bytes.len() == 4).then(|| {
        let local = u32::from_le_bytes(bytes.try_into().unwrap()) & 0x00FF_FFFF;
        format!("{local:06X}:FalloutNV.esm")
    })
}

fn exact_quest_source(form_key: &str, expected_local: u32) -> bool {
    let Some((local, plugin)) = form_key.split_once([':', '@']) else {
        return false;
    };
    u32::from_str_radix(local.trim_start_matches("0x"), 16).ok() == Some(expected_local)
        && plugin.eq_ignore_ascii_case("FalloutNV.esm")
}

fn indent_fragment_lines(body: &str) -> Vec<String> {
    body.lines()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else if line.starts_with("    ") {
                line.to_string()
            } else {
                format!("    {}", line.trim_start())
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Payload builder
// ---------------------------------------------------------------------------

fn build_quest_payload(record: &Value, _strict: bool) -> Value {
    let drop_fields: HashSet<&str> = [
        "SCRI",
        "DATA",
        "CTDA",
        "CIS1",
        "CIS2",
        "INDX",
        "QSDT",
        "SCHR",
        "SCDA",
        "SCTX",
        "SLSD",
        "SCVR",
        "SCRO",
        "SCRV",
        "QOBJ",
        "QSTA",
        "VMAD",
        "VirtualMachineAdapter",
    ]
    .into_iter()
    .collect();
    filtered_payload(record, &drop_fields)
}

// ---------------------------------------------------------------------------
// Helpers shared across modules
// ---------------------------------------------------------------------------

/// Extract the text value of a named subrecord key from a field object.
pub(super) fn field_value<'a>(field: &'a Value, sig: &str) -> Option<&'a Value> {
    field.as_object()?.get(sig)
}

/// Read the EditorID (EDID) from a record value.
fn record_eid(record: &Value) -> String {
    if let Some(eid) = record.get("eid").and_then(|v| v.as_str()) {
        return eid.to_string();
    }
    if let Some(fields) = record.get("fields").and_then(|f| f.as_array()) {
        for field in fields {
            if let Some(edid) = field_value(field, "EDID") {
                if let Some(s) = edid.as_str() {
                    return s.to_string();
                }
            }
        }
    }
    "Unnamed".to_string()
}

/// Filter out drop_fields from a record value, removing `__source_form_key`.
pub(super) fn filtered_payload(record: &Value, drop_fields: &HashSet<&str>) -> Value {
    let mut payload: Map<String, Value> = Map::new();
    if let Some(obj) = record.as_object() {
        for (k, v) in obj {
            if k == "__source_form_key" {
                continue;
            }
            if k == "fields" {
                continue; // rebuilt below
            }
            payload.insert(k.clone(), v.clone());
        }
    }
    let mut fields: Vec<Value> = Vec::new();
    if let Some(field_arr) = record.get("fields").and_then(|f| f.as_array()) {
        for field in field_arr {
            if let Some(obj) = field.as_object() {
                if obj.len() != 1 {
                    continue;
                }
                let key = obj.keys().next().unwrap().as_str();
                if drop_fields.contains(key) {
                    continue;
                }
                fields.push(field.clone());
            }
        }
    }
    payload.insert("fields".into(), Value::Array(fields));
    Value::Object(payload)
}

/// Insert or replace a named field in the record payload's `fields` array.
pub(super) fn upsert_field(payload: &mut Value, key: &str, value: Value) {
    let fields = payload
        .as_object_mut()
        .and_then(|o| o.get_mut("fields"))
        .and_then(|f| f.as_array_mut());
    if let Some(fields) = fields {
        for entry in fields.iter_mut() {
            if entry.as_object().and_then(|o| o.get(key)).is_some() {
                *entry = json!({ key: value });
                return;
            }
        }
        fields.insert(0, json!({ key: value }));
    } else {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("fields".into(), json!([{ key: value }]));
        }
    }
}

/// Collect unique reference variable names (pattern `r[A-Za-z0-9_]+`) from
/// FNV script source strings.
fn collect_reference_names(sources: &[String]) -> Vec<String> {
    let re = Regex::new(r"\b(r[A-Za-z0-9_]+)\b").unwrap();
    let mut seen: HashSet<String> = HashSet::new();
    let mut names: Vec<String> = Vec::new();
    for source in sources {
        for cap in re.captures_iter(source) {
            let name = cap[1].to_string();
            if seen.insert(name.clone()) {
                names.push(name);
            }
        }
    }
    names
}

/// Extract the body of the first event block from emitted Papyrus text.
///
/// Returns everything inside the first `Event OnInit()` … `EndEvent` block
/// (or the generic event block), or an empty string if none is found.
pub(super) fn extract_first_event_body(papyrus: &str) -> String {
    let mut in_event = false;
    let mut body_lines: Vec<&str> = Vec::new();
    for line in papyrus.lines() {
        let trimmed = line.trim();
        if !in_event {
            if trimmed.starts_with("Event ") || trimmed.starts_with("Function ") {
                in_event = true;
            }
            continue;
        }
        if trimmed == "EndEvent" || trimmed == "EndFunction" {
            break;
        }
        body_lines.push(line);
    }
    body_lines.join("\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, ResolutionMode};
    use crate::ids::FormKey;
    use crate::sym::StringInterner;
    use crate::translator::pair_hooks::fnv_fo4::PlacedActorAliasTarget;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, ParsedSubrecord, insert_authoring_record_value,
        insert_interior_cell_with_children, insert_parsed_record, plugin_handle_add_master_no_py,
        plugin_handle_close_native, plugin_handle_load_no_py, plugin_handle_new_no_py,
        plugin_handle_read_authoring_record_value_json, plugin_handle_save_no_py,
        plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    fn raw_hex_value(bytes: &[u8]) -> Value {
        json!({
            "encoding": "raw-bytes-hex",
            "hex": hex::encode_upper(bytes)
        })
    }

    fn legacy_qsta(local: u32) -> Value {
        let mut bytes = local.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0, 249, 36, 28]);
        raw_hex_value(&bytes)
    }

    fn parsed_record(
        signature: &'static str,
        form_id: u32,
        subrecords: Vec<ParsedSubrecord>,
    ) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new_static(signature),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: Some(5),
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn parsed_subrecord(signature: &'static str, data: impl Into<Bytes>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new_static(signature),
            data: data.into(),
            semantic_type: None,
        }
    }

    fn find_parsed_record<'a>(
        items: &'a [ParsedItem],
        signature: &str,
        local: u32,
    ) -> Option<&'a ParsedRecord> {
        items.iter().find_map(|item| match item {
            ParsedItem::Record(record)
                if record.signature.as_str() == signature
                    && record.form_id & 0x00FF_FFFF == local =>
            {
                Some(record)
            }
            ParsedItem::Group(group) => find_parsed_record(&group.children, signature, local),
            ParsedItem::Record(_) => None,
        })
    }

    fn mapper<'a>(interner: &'a StringInterner) -> FormKeyMapper<'a> {
        FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "FalloutNV.esm".to_string(),
                source_plugin_name: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
                target_master_names: Vec::new(),
                resolution_mode: ResolutionMode::Strict,
                ..MapperOptions::default()
            },
            interner,
        )
    }

    fn register_alias_targets(
        aliases: &mut PlacedActorAliasResolver,
        interner: &StringInterner,
        source_locals: &[u32],
    ) {
        let plugin = interner.intern("FalloutNV.esm");
        for source_local in source_locals {
            aliases
                .register(PlacedActorAliasTarget {
                    source_placed: FormKey {
                        local: *source_local,
                        plugin,
                    },
                    target_placed: FormKey {
                        local: source_local + 0x200000,
                        plugin,
                    },
                    source_base: FormKey {
                        local: source_local + 1,
                        plugin,
                    },
                    target_base: FormKey {
                        local: source_local + 0x300000,
                        plugin,
                    },
                })
                .unwrap();
        }
    }

    fn vtechatticup_fixture() -> Value {
        json!({
            "eid": "VTechatticup",
            "fields": [
                { "EDID": "VTechatticup" },
                { "SCRI": "11FC64:FalloutNV.esm" },
                { "FULL": "Anywhere I Wander" },
                { "DATA": raw_hex_value(&[0x11, 50, 226, 0, 0, 0, 0, 0]) },
                { "INDX": 10 },
                { "QSDT": 0 },
                { "SCHR": raw_hex_value(&[0; 20]) },
                { "SCDA": raw_hex_value(&hex::decode("A3110F0003007201006E0A0000006E01000000").unwrap()) },
                { "SCTX": raw_hex_value(&hex::decode("5365744F626A656374697665446973706C617965642056546563686174746963757020313020310D0A").unwrap()) },
                { "SCRO": "11F935:FalloutNV.esm" },
                { "INDX": 20 },
                { "QSDT": 0 },
                { "SCTX": raw_hex_value(&hex::decode("5365744F626A656374697665436F6D706C657465642056746563686174746963757020313020310D0A5365744F626A656374697665446973706C617965642056546563686174746963757020323020310D0A").unwrap()) },
                { "INDX": 100 },
                { "QSDT": 1 },
                { "CNAM": "Quest Completed." },
                { "SCTX": raw_hex_value(&hex::decode("5365744F626A656374697665436F6D706C657465642056746563686174746963757020323020310D0A5265776172645850203130300D0A41646452657075746174696F6E205265704E564E435220312034203B202831315F375F3130292042756723203430383831202D4554420D0A436F6D706C6574655175657374205674656368617474696375700D0A3B636F6D706C657465616C6C6F626A65637469766573207674656368617474696375700D0A").unwrap()) },
                { "INDX": 110 },
                { "QSDT": 2 },
                { "CNAM": "NCR hostages are dead." },
                { "SCTX": raw_hex_value(&hex::decode("53746F70517565737420564D533230").unwrap()) },
                { "SCRO": "10E908:FalloutNV.esm" },
                { "QOBJ": 10 },
                { "NNAM": "Rescue the NCR hostages from the Techatticup mine." },
                { "QSTA": legacy_qsta(0x12319B) },
                { "QSTA": legacy_qsta(0x12319C) },
                { "QOBJ": 20 },
                { "NNAM": "Report back to Private Renolds." },
                { "QSTA": legacy_qsta(0x134B9C) },
                { "QOBJ": 110 },
                { "NNAM": "The NCR Hostages are dead." }
            ]
        })
    }

    #[test]
    fn synthesize_aliases_deduplicates() {
        let names = vec!["rNpc".to_string(), "rNpc".to_string(), "rItem".to_string()];
        let aliases = synthesize_aliases_for_refs(&names);
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases[0].name, "rNpc");
        assert_eq!(aliases[0].fill_type, "specific_reference");
    }

    #[test]
    fn build_quest_psc_format() {
        let frags = vec![StageFragment {
            stage_index: 10,
            stage_item_index: 0,
            psc_function_name: "Fragment_10".into(),
            body: "x = 1".into(),
        }];
        let psc = build_quest_psc("QF_B21_001234", &frags, &[]);
        assert!(psc.contains("ScriptName QF_B21_001234 extends Quest"));
        assert!(psc.contains("Function Fragment_10()"));
        assert!(psc.contains("EndFunction"));
        assert!(psc.contains("    x = 1"), "psc:\n{psc}");
    }

    #[test]
    fn collect_reference_names_regex() {
        let sources = vec!["set rNpc to GetRef rItem".to_string()];
        let names = collect_reference_names(&sources);
        assert!(names.contains(&"rNpc".to_string()));
        assert!(names.contains(&"rItem".to_string()));
    }

    #[test]
    fn filtered_payload_drops_sctx_and_indx() {
        let record = json!({
            "eid": "Q1",
            "fields": [
                { "INDX": 10 },
                { "SCTX": "set x to 1" },
                { "FULL": "My Quest" },
            ]
        });
        let drop: HashSet<&str> = ["INDX", "SCTX", "VMAD", "VirtualMachineAdapter"]
            .into_iter()
            .collect();
        let payload = filtered_payload(&record, &drop);
        let fields = payload["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert!(fields[0].get("FULL").is_some());
    }

    #[test]
    fn upsert_field_inserts_new() {
        let mut payload = json!({ "fields": [] });
        upsert_field(&mut payload, "VMAD", json!({ "Version": 5 }));
        let fields = payload["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert!(fields[0]["VMAD"]["Version"] == 5);
    }

    #[test]
    fn upsert_field_replaces_existing() {
        let mut payload = json!({ "fields": [{ "VMAD": { "Version": 1 } }] });
        upsert_field(&mut payload, "VMAD", json!({ "Version": 5 }));
        let fields = payload["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["VMAD"]["Version"], 5);
    }

    #[test]
    fn translate_qust_record_smoke() {
        // Empty QUST record with no SCTX fields should produce a valid TranslatedQuest
        // with no fragments and the correct class name.
        let record = json!({
            "eid": "TestQuest",
            "fields": []
        });
        let result = translate_qust_record(&record, "B21", false, "001234:FNV.esm");
        let tq = result.expect("translate ok");
        assert_eq!(tq.source_editor_id, "TestQuest");
        assert_eq!(tq.fragment_class_name, "QF_B21_001234");
        assert!(
            tq.fragment_psc_text
                .starts_with("ScriptName QF_B21_001234 extends Quest")
        );
        assert!(tq.fragment_class_name.len() <= 38);
        assert!(tq.stage_fragments.is_empty());
    }

    #[test]
    fn quest_fragment_class_rejects_compiler_unsafe_length() {
        let record = json!({ "eid": "TestQuest", "fields": [] });
        let error = translate_qust_record(
            &record,
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ123456",
            false,
            "001234:FNV.esm",
        )
        .unwrap_err();
        assert!(error.to_string().contains("38-character Papyrus limit"));
    }

    #[test]
    fn translate_qust_record_with_stage_fragment() {
        let record = json!({
            "eid": "TestQuest",
            "fields": [
                { "INDX": 10 },
                { "SCTX": "set x to 1" },
            ]
        });
        let result = translate_qust_record(&record, "B21", false, "001234:FNV.esm");
        let tq = result.expect("translate ok");
        assert_eq!(tq.stage_fragments.len(), 1);
        assert_eq!(tq.stage_fragments[0].stage_index, 10);
        assert_eq!(tq.stage_fragments[0].stage_item_index, 0);
        assert!(tq.fragment_psc_text.contains("Fragment_Stage_0010_Item_00"));
    }

    #[test]
    fn two_scripted_entries_in_one_stage_keep_distinct_fragment_identity() {
        let record = json!({
            "eid": "TwoItems",
            "fields": [
                { "INDX": 10 },
                { "QSDT": 0 },
                { "SCTX": raw_hex_value(b"SetStage TwoItems 20") },
                { "QSDT": 0 },
                { "SCTX": raw_hex_value(b"StopQuest TwoItems") }
            ]
        });
        let translated =
            translate_qust_record(&record, "FNV_FO3", true, "00ABCD:FalloutNV.esm").unwrap();
        assert_eq!(translated.stage_fragments.len(), 2);
        assert_eq!(translated.stage_fragments[0].stage_item_index, 0);
        assert_eq!(translated.stage_fragments[1].stage_item_index, 1);
        assert!(
            translated
                .fragment_psc_text
                .contains("Fragment_Stage_0010_Item_00")
        );
        assert!(
            translated
                .fragment_psc_text
                .contains("Fragment_Stage_0010_Item_01")
        );
        assert!(translated.fragment_psc_text.contains("Self.SetStage(20)"));
        assert!(translated.fragment_psc_text.contains("Self.Stop()"));
    }

    #[test]
    fn objective_commands_use_the_owning_quest_context() {
        let record = json!({
            "eid": "ObjectiveQuest",
            "fields": [
                { "INDX": 10 },
                { "QSDT": 0 },
                { "SCTX": raw_hex_value(
                    b"SetObjectiveDisplayed ObjectiveQuest 20 1\r\nSetObjectiveCompleted ObjectiveQuest 20 1"
                ) }
            ]
        });
        let translated =
            translate_qust_record(&record, "FNV_FO3", true, "00ABCE:FalloutNV.esm").unwrap();
        assert!(
            translated
                .fragment_psc_text
                .contains("Self.SetObjectiveDisplayed(20, 1 != 0)")
        );
        assert!(
            translated
                .fragment_psc_text
                .contains("Self.SetObjectiveCompleted(20, 1 != 0)")
        );
    }

    #[test]
    fn vtechatticup_golden_rebuilds_stages_objectives_and_forced_aliases() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let mut mapper = mapper(&interner);
        let mut placed_aliases = PlacedActorAliasResolver::default();
        register_alias_targets(
            &mut placed_aliases,
            &interner,
            &[0x12319B, 0x12319C, 0x134B9C],
        );
        let raw_formids = |raw: u32| {
            Some(FormKey {
                local: raw & 0x00FF_FFFF,
                plugin,
            })
        };

        let translated = lower_qust_record(
            &vtechatticup_fixture(),
            "B21",
            "11F935:FalloutNV.esm",
            "21F935:FalloutNV.esm",
            LegacyConditionFamily::Fnv,
            &mut mapper,
            &raw_formids,
            &placed_aliases,
        )
        .unwrap();
        assert_eq!(translated.fragment_class_name, "QF_B21_11F935");
        assert!(translated.fragment_class_name.len() <= 38);
        let fields = translated.authoring_record_payload["fields"]
            .as_array()
            .unwrap();
        let stage_indices = fields
            .iter()
            .filter_map(|field| field.get("INDX"))
            .map(|value| value["StageIndex"].as_u64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(stage_indices, [10, 20, 100, 110]);
        let objective_indices = fields
            .iter()
            .filter_map(|field| field.get("QOBJ").and_then(Value::as_u64))
            .collect::<Vec<_>>();
        assert_eq!(objective_indices, [10, 20, 110]);
        assert_eq!(
            fields
                .iter()
                .filter(|field| field.get("QSTA").is_some())
                .count(),
            3
        );
        assert_eq!(translated.aliases.len(), 3);
        assert_eq!(translated.aliases[0].source_placed.local, 0x12319B);
        assert_eq!(translated.aliases[0].target_placed.local, 0x32319B);
        assert!(fields.iter().any(|field| field.get("ALFR")
            == Some(&json!({
                "reference": {
                    "plugin": "FalloutNV.esm",
                    "object_id": "32319B"
                }
            }))));
        assert!(!fields.iter().any(|field| field.get("VTCK").is_some()));
        assert_eq!(translated.fragment_metadata.len(), 4);
        assert_eq!(translated.fragment_metadata[2].stage_index, 100);
        assert_eq!(translated.losses.terminal_count(), 0);
        for forbidden in ["SCRI", "SCHR", "SCDA", "SCTX", "SCRO"] {
            assert!(!fields.iter().any(|field| field.get(forbidden).is_some()));
        }
    }

    #[test]
    fn freeform_power_armor_golden_preserves_stage_100_and_normalizes_condition() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "FalloutNV.esm".to_string(),
                source_plugin_name: "FalloutNV.esm".to_string(),
                source_master_names: Vec::new(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                resolution_mode: ResolutionMode::Strict,
                ..MapperOptions::default()
            },
            &interner,
        );
        for (source, target, target_plugin) in
            [(0x058FDF, 0x158FDF, plugin), (0x000014, 0x000014, fallout4)]
        {
            mapper.add_mapping(
                FormKey {
                    local: source,
                    plugin,
                },
                FormKey {
                    local: target,
                    plugin: target_plugin,
                },
            );
        }
        let raw_formids = |raw: u32| {
            Some(FormKey {
                local: raw & 0x00FF_FFFF,
                plugin,
            })
        };
        let record = json!({
            "eid": "FreeformPowerArmor",
            "fields": [
                { "EDID": "FreeformPowerArmor" },
                { "DATA": raw_hex_value(&[0x1D, 59, 0, 0, 0, 0, 0, 0]) },
                { "CTDA": raw_hex_value(&hex::decode("0000000000000000C1010000DF8F0500000000000200000014000000").unwrap()) },
                { "INDX": 10 }, { "QSDT": 0 },
                { "SCTX": raw_hex_value(&hex::decode("3B506C6179657220686173206265656E20646972656374656420746F2047756E6E79").unwrap()) },
                { "INDX": 20 }, { "QSDT": 0 },
                { "SCTX": raw_hex_value(&hex::decode("3B506C6179657220686173207065726D697373696F6E2066726F6D204C796F6E73").unwrap()) },
                { "INDX": 100 }, { "QSDT": 0 },
                { "SCTX": raw_hex_value(&hex::decode("3B506C6179657220697320747261696E65640D0A506C617965722E4164645065726B20506F77657241726D6F72547261696E696E67200D0A536574504343616E557365506F77657241726D6F7220310D0A53686F774D65737361676520506F77657241726D6F72547261696E696E675065726B4D7367").unwrap()) },
                { "INDX": 200 }, { "QSDT": 0 },
                { "SCTX": raw_hex_value(&hex::decode("3B53746F702071756573740D0A53746F70517565737420467265666F726D506F77657241726D6F72").unwrap()) }
            ]
        });
        let translated = lower_qust_record(
            &record,
            "B21",
            "06136D:FalloutNV.esm",
            "16136D:FalloutNV.esm",
            LegacyConditionFamily::Fnv,
            &mut mapper,
            &raw_formids,
            &PlacedActorAliasResolver::default(),
        )
        .unwrap();
        assert_eq!(translated.fragment_class_name, "QF_B21_06136D");
        assert!(translated.fragment_class_name.len() <= 38);
        let fields = translated.authoring_record_payload["fields"]
            .as_array()
            .unwrap();
        assert!(
            fields
                .iter()
                .any(|field| field["INDX"]["StageIndex"] == 100)
        );
        let ctda = fields.iter().find_map(|field| field.get("CTDA")).unwrap();
        let bytes = hex::decode(ctda["raw_hex"].as_str().unwrap()).unwrap();
        assert_eq!(bytes.len(), 32);
        assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 448);
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x01158FDF
        );
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            0x000014
        );
        assert_eq!(translated.aliases.len(), 0);
        assert_eq!(translated.losses.terminal_count(), 0);
        assert_eq!(
            fields
                .iter()
                .find_map(|field| field.get("DNAM"))
                .and_then(|dnam| dnam.get("Flags"))
                .and_then(Value::as_u64),
            Some(0x09)
        );
    }

    #[test]
    fn freeform_root_condition_survives_native_insert_save_and_reopen() {
        const OUTPUT_PLUGIN: &str = "FalloutNV.esm";
        const EXPECTED_TARGET_CTDA_HEX: &str =
            "0000000000000000C0010000DF8F1501000000000200000014000000FFFFFFFF";

        let interner = StringInterner::new();
        let output_plugin = interner.intern(OUTPUT_PLUGIN);
        let fallout4 = interner.intern("Fallout4.esm");
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: OUTPUT_PLUGIN.into(),
                source_plugin_name: OUTPUT_PLUGIN.into(),
                source_master_names: Vec::new(),
                target_master_names: vec!["Fallout4.esm".into()],
                resolution_mode: ResolutionMode::Strict,
                ..MapperOptions::default()
            },
            &interner,
        );
        for (source_local, target) in [
            (
                0x058FDF,
                FormKey {
                    local: 0x158FDF,
                    plugin: output_plugin,
                },
            ),
            (
                0x000014,
                FormKey {
                    local: 0x000014,
                    plugin: fallout4,
                },
            ),
        ] {
            mapper.add_mapping(
                FormKey {
                    local: source_local,
                    plugin: output_plugin,
                },
                target,
            );
        }
        let record = json!({
            "eid": "FreeformPowerArmor",
            "fields": [
                { "DATA": raw_hex_value(&[0x1D, 59, 0, 0, 0, 0, 0, 0]) },
                { "CTDA": raw_hex_value(&hex::decode("0000000000000000C1010000DF8F0500000000000200000014000000").unwrap()) },
                { "INDX": 10 },
                { "QSDT": 0 }
            ]
        });
        let raw_formids = |_raw: u32| None;
        let translated = lower_qust_record(
            &record,
            "FNV_FO3",
            "06136D:FalloutNV.esm",
            "16136D:FalloutNV.esm",
            LegacyConditionFamily::Fnv,
            &mut mapper,
            &raw_formids,
            &PlacedActorAliasResolver::default(),
        )
        .unwrap();
        assert_eq!(
            translated.authoring_record_payload["fields"]
                .as_array()
                .unwrap()
                .iter()
                .find_map(|field| field.get("CTDA"))
                .and_then(|condition| condition.get("raw_hex"))
                .and_then(Value::as_str),
            Some(EXPECTED_TARGET_CTDA_HEX)
        );

        let handle = plugin_handle_new_no_py(OUTPUT_PLUGIN, Some("fo4"));
        plugin_handle_add_master_no_py(handle, "Fallout4.esm", None).unwrap();
        insert_authoring_record_value(handle, &translated.authoring_record_payload).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(OUTPUT_PLUGIN);
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        assert!(plugin_handle_close_native(handle));
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&reopened).unwrap();
            let quest = find_parsed_record(&slot.parsed.root_items, "QUST", 0x16136D).unwrap();
            let condition = quest
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "CTDA")
                .unwrap();
            assert_eq!(
                hex::encode_upper(condition.data.as_ref()),
                EXPECTED_TARGET_CTDA_HEX
            );
            assert_eq!(
                u16::from_le_bytes(condition.data[8..10].try_into().unwrap()),
                448
            );
            assert_eq!(
                u32::from_le_bytes(condition.data[12..16].try_into().unwrap()),
                0x01158FDF
            );
            assert_eq!(
                u32::from_le_bytes(condition.data[20..24].try_into().unwrap()),
                2
            );
            assert_eq!(
                u32::from_le_bytes(condition.data[24..28].try_into().unwrap()),
                0x000014
            );
        }
        assert!(plugin_handle_close_native(reopened));
    }

    #[test]
    fn freeform_root_player_dependency_is_exact_ctda_gated() {
        let record = json!({
            "eid": "FreeformPowerArmor",
            "fields": [
                { "EDID": "FreeformPowerArmor" },
                { "DATA": raw_hex_value(&[0x1D, 59, 0, 0, 0, 0, 0, 0]) },
                { "CTDA": raw_hex_value(&hex::decode("0000000000000000C1010000DF8F0500000000000200000014000000").unwrap()) },
                { "INDX": 10 },
                { "QSDT": 0 }
            ]
        });
        let mappings = exact_quest_record_dependency_mappings(
            &record,
            "FreeformPowerArmor",
            "06136D:FalloutNV.esm",
        )
        .unwrap();
        assert_eq!(
            mappings,
            [QuestRecordDependencyMapping {
                scope: "root CTDA run-on reference".into(),
                evidence_hex: "0000000000000000C1010000DF8F0500000000000200000014000000".into(),
                source_dependency_form_key: "000014:FalloutNV.esm".into(),
                target_dependency_form_key: "000014:Fallout4.esm".into(),
            }]
        );

        for (label, near_miss) in [
            ("changed condition function", {
                let mut value = record.clone();
                value["fields"][2]["CTDA"] = raw_hex_value(
                    &hex::decode("0000000000000000C2010000DF8F0500000000000200000014000000")
                        .unwrap(),
                );
                value
            }),
            ("changed perk parameter", {
                let mut value = record.clone();
                value["fields"][2]["CTDA"] = raw_hex_value(
                    &hex::decode("0000000000000000C1010000E08F0500000000000200000014000000")
                        .unwrap(),
                );
                value
            }),
            ("changed run-on reference", {
                let mut value = record.clone();
                value["fields"][2]["CTDA"] = raw_hex_value(
                    &hex::decode("0000000000000000C1010000DF8F0500000000000200000015000000")
                        .unwrap(),
                );
                value
            }),
            ("extra root condition", {
                let mut value = record.clone();
                let extra = value["fields"][2].clone();
                value["fields"].as_array_mut().unwrap().insert(3, extra);
                value
            }),
        ] {
            let error = exact_quest_record_dependency_mappings(
                &near_miss,
                "FreeformPowerArmor",
                "06136D:FalloutNV.esm",
            )
            .unwrap_err();
            assert!(error.to_string().contains("exact CTDA"), "{label}: {error}");
        }

        assert!(
            exact_quest_record_dependency_mappings(
                &record,
                "FreeformPowerArmor",
                "06136D:FalloutNV.esm"
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            exact_quest_record_dependency_mappings(&record, "WrongQuest", "06136D:FalloutNV.esm")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn live_freeform_power_armor_raw_sctx_uses_real_prefix_adapter() {
        let record = json!({
            "eid": "FreeformPowerArmor",
            "fields": [
                { "INDX": 100 },
                { "QSDT": 0 },
                { "SCTX": raw_hex_value(&hex::decode("3B506C6179657220697320747261696E65640D0A506C617965722E4164645065726B20506F77657241726D6F72547261696E696E67200D0A536574504343616E557365506F77657241726D6F7220310D0A53686F774D65737361676520506F77657241726D6F72547261696E696E675065726B4D7367").unwrap()) },
                { "SCRO": "058FDF:FalloutNV.esm" },
                { "SCRO": "070A14:FalloutNV.esm" }
            ]
        });
        let translated =
            translate_qust_record(&record, "FNV_FO3", true, "06136D:FalloutNV.esm").unwrap();
        assert_eq!(translated.fragment_class_name, "QF_FNV_FO3_06136D");
        assert!(
            translated
                .fragment_psc_text
                .starts_with("ScriptName QF_FNV_FO3_06136D extends Quest")
        );
        assert_eq!(translated.stage_fragments.len(), 1);
        assert_eq!(translated.stage_fragments[0].stage_index, 100);
        assert_eq!(translated.stage_fragments[0].stage_item_index, 0);
        assert!(
            translated
                .fragment_psc_text
                .contains("FNVSliceCompat.SetPCCanUsePowerArmor(1 != 0)")
        );
        assert!(!translated.fragment_psc_text.contains("Self as"));
        assert!(
            translated
                .fragment_psc_text
                .contains("Game.GetPlayer().AddPerk(PowerArmorTraining, true)")
        );
        assert!(!translated.fragment_psc_text.contains("B21_FnvSliceCompat"));
        assert!(
            translated
                .fragment_psc_text
                .contains("Message Property PowerArmorTrainingPerkMsg Auto Const")
        );
        assert!(
            translated
                .fragment_psc_text
                .contains("FNV_FO3_FnvSliceCompat Property FNVSliceCompat Auto Const")
        );
        assert_eq!(
            translated
                .fragment_properties
                .iter()
                .map(|property| (property.name.as_str(), property.papyrus_type.as_str()))
                .collect::<Vec<_>>(),
            [
                ("FNVSliceCompat", "FNV_FO3_FnvSliceCompat"),
                ("PowerArmorTraining", "Perk"),
                ("PowerArmorTrainingPerkMsg", "Message"),
            ]
        );
        assert_eq!(
            translated.fragment_properties[0].source_form_key,
            "06136D:FalloutNV.esm"
        );

        let mut wrong_plugin = record.clone();
        wrong_plugin["fields"][4]["SCRO"] = json!("070A14:FalloutNV.esm");
        let error = translate_qust_record(&wrong_plugin, "FNV_FO3", true, "06136D:FalloutNV.esm")
            .unwrap_err();
        assert!(error.to_string().contains("Message SCRO 070A14"));
    }

    #[test]
    fn live_vtech_stage_100_completes_the_quest() {
        let record = json!({
            "eid": "VTechatticup",
            "fields": [
                { "INDX": 100 },
                { "QSDT": 1 },
                { "SCTX": raw_hex_value(b"SetObjectiveCompleted Vtechatticup 20 1\r\nRewardXP 100\r\nAddReputation RepNVNCR 1 4 ; (11_7_10) Bug# 40881 -ETB\r\nCompleteQuest Vtechatticup\r\n;completeallobjectives vtechatticup\r\n") },
                { "SCRO": "0F43DE:FalloutNV.esm" }
            ]
        });
        let translated =
            translate_qust_record(&record, "FNV_FO3", true, "11F935:FalloutNV.esm").unwrap();
        assert_eq!(translated.fragment_class_name, "QF_FNV_FO3_11F935");
        assert!(
            translated
                .fragment_psc_text
                .contains("Self.CompleteQuest()")
        );
        assert!(
            !translated
                .fragment_psc_text
                .contains("Self.CompleteAllObjectives()")
        );
        assert!(
            translated
                .fragment_psc_text
                .contains("FNV_FO3_FnvSliceCompat Property FNVSliceCompat Auto Const")
        );
        assert!(!translated.fragment_psc_text.contains("Self as"));
    }

    #[test]
    fn vtech_reputation_fragment_uses_compat_state_without_repu_property() {
        let record = json!({
            "eid": "VTechatticup",
            "fields": [
                { "INDX": 100 },
                { "QSDT": 1 },
                { "SCTX": raw_hex_value(b"AddReputation RepNVNCR 1 4") },
                { "SCRO": "0F43DE:FalloutNV.esm" }
            ]
        });
        let translated =
            translate_qust_record(&record, "FNV_FO3", true, "11F935:FalloutNV.esm").unwrap();
        assert!(
            translated
                .fragment_psc_text
                .contains("FNVSliceCompat.ModRepNVNCR(1, 4)")
        );
        assert!(
            translated
                .fragment_psc_text
                .contains("FNV_FO3_FnvSliceCompat Property FNVSliceCompat Auto Const")
        );
        assert!(!translated.fragment_psc_text.contains("Self as"));
        assert!(!translated.fragment_psc_text.contains("RepNVNCR Property"));
        assert_eq!(
            translated.fragment_properties,
            [QuestFragmentProperty {
                name: "FNVSliceCompat".into(),
                papyrus_type: "FNV_FO3_FnvSliceCompat".into(),
                source_form_key: "11F935:FalloutNV.esm".into(),
            }]
        );
    }

    #[test]
    fn compat_fragment_property_is_exact_quest_gated() {
        let record = json!({ "fields": [] });
        for (editor_id, source_form_key) in [
            ("VTechatticup", "11F935:FalloutNV.esm"),
            ("WrongQuest", "11F935:FalloutNV.esm"),
            ("VTechatticup", "11F936:FalloutNV.esm"),
            ("FreeformPowerArmor", "06136D:FalloutNV.esm"),
            ("WrongQuest", "06136D:FalloutNV.esm"),
            ("FreeformPowerArmor", "06136E:FalloutNV.esm"),
        ] {
            assert!(
                exact_quest_fragment_properties(&record, editor_id, source_form_key, "FNV_FO3",)
                    .unwrap()
                    .is_empty(),
                "unexpected compatibility property for {editor_id} at {source_form_key}"
            );
        }
    }

    #[test]
    fn live_vtech_stage_110_consumes_vms20_as_accounted_target_native_noop() {
        let record = json!({
            "eid": "VTechatticup",
            "fields": [
                { "INDX": 110 },
                { "QSDT": 2 },
                { "CNAM": "NCR hostages are dead." },
                { "SCHR": raw_hex_value(&hex::decode("0100000009000000000000000000000001000000").unwrap()) },
                { "SCDA": raw_hex_value(&hex::decode("371005000100720100").unwrap()) },
                { "SCTX": raw_hex_value(&hex::decode("53746F70517565737420564D533230").unwrap()) },
                { "SCRO": "10E908:FalloutNV.esm" }
            ]
        });
        let translated =
            translate_qust_record(&record, "FNV_FO3", true, "11F935:FalloutNV.esm").unwrap();
        assert_eq!(translated.fragment_adaptations.len(), 1);
        assert_eq!(
            translated.fragment_adaptations[0],
            QuestFragmentAdaptation {
                stage_index: 110,
                stage_item_index: 0,
                source_command: "StopQuest VMS20".into(),
                source_dependency_form_key: "10E908:FalloutNV.esm".into(),
                kind: QuestFragmentAdaptationKind::TargetNativeNoOp,
                reason: "VMS20 is outside the selected slice; the selected quest's FailQuest stage remains authoritative in FO4".into(),
            }
        );
        assert!(translated.fragment_psc_text.contains("    Return"));
        assert!(!translated.fragment_psc_text.contains("VMS20"));
        assert!(!translated.fragment_psc_text.contains(".Stop()"));
        assert_eq!(
            translated.fragment_properties,
            [QuestFragmentProperty {
                name: "FNVSliceCompat".into(),
                papyrus_type: "FNV_FO3_FnvSliceCompat".into(),
                source_form_key: "11F935:FalloutNV.esm".into(),
            }]
        );

        for (label, near_miss) in [
            ("wrong dependency plugin", {
                let mut value = record.clone();
                value["fields"][6]["SCRO"] = json!("10E908:FalloutNV.esm");
                value
            }),
            ("wrong dependency local", {
                let mut value = record.clone();
                value["fields"][6]["SCRO"] = json!("10E909:FalloutNV.esm");
                value
            }),
            ("wrong stage", {
                let mut value = record.clone();
                value["fields"][0]["INDX"] = json!(111);
                value
            }),
            ("wrong stage item", {
                let mut value = record.clone();
                value["fields"]
                    .as_array_mut()
                    .unwrap()
                    .insert(1, json!({ "QSDT": 0 }));
                value
            }),
            ("changed command", {
                let mut value = record.clone();
                value["fields"][5]["SCTX"] = raw_hex_value(b"StopQuest VMS21");
                value
            }),
        ] {
            let error = translate_qust_record(&near_miss, "FNV_FO3", true, "11F935:FalloutNV.esm")
                .unwrap_err();
            assert!(
                error.to_string().contains("target-native"),
                "{label}: {error}"
            );
        }

        assert!(
            exact_target_native_quest_fragment_adaptations(
                &record,
                "VTechatticup",
                "11F935:FalloutNV.esm"
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn package_data_alias_appends_after_existing_aliases_and_updates_anam() {
        let mut translation = QuestTranslation {
            source_editor_id: "VTechatticup".into(),
            source_form_key: "11F935:FalloutNV.esm".into(),
            target_form_key: "21F935:Target.esp".into(),
            fragment_class_name: "QF_Test".into(),
            authoring_record_payload: json!({
                "fields": [
                    { "ANAM": 2 },
                    { "ALST": 0 }, { "ALID": "Hostage01" }, { "ALED": null },
                    { "ALST": 1 }, { "ALID": "Hostage02" }, { "ALED": null }
                ]
            }),
            aliases: Vec::new(),
            fragment_metadata: Vec::new(),
            losses: QuestLossAccounting::default(),
        };
        let alias_id = append_unfilled_package_data_alias(
            &mut translation,
            "TecMineHostageEscapeData",
            "2231B6:Target.esp",
        )
        .unwrap();
        assert_eq!(alias_id, 2);
        let fields = translation.authoring_record_payload["fields"]
            .as_array()
            .unwrap();
        assert_eq!(fields[0]["ANAM"], 3);
        assert!(fields.iter().any(|field| field["ALST"] == 2));
        assert!(fields.iter().any(|field| field["ALPC"]
            == json!({
                "reference": {
                    "plugin": "Target.esp",
                    "object_id": "2231B6"
                }
            })));
        assert!(
            append_unfilled_package_data_alias(
                &mut translation,
                "TecMineHostageEscapeData",
                "2231B6:Target.esp"
            )
            .is_err()
        );

        let before = translation.authoring_record_payload.clone();
        assert!(
            append_unfilled_package_data_alias(
                &mut translation,
                "MalformedPackageData",
                "1000000:Target.esp"
            )
            .is_err()
        );
        assert_eq!(translation.authoring_record_payload, before);
    }

    #[test]
    fn forced_and_package_alias_references_survive_insert_save_and_reopen() {
        const OUTPUT_PLUGIN: &str = "AliasRoundtrip.esp";
        const OWN_INDEX: u32 = 0x0100_0000;

        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let output_plugin = interner.intern(OUTPUT_PLUGIN);
        let fallout4 = interner.intern("Fallout4.esm");
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: OUTPUT_PLUGIN.into(),
                source_plugin_name: "FalloutNV.esm".into(),
                source_master_names: Vec::new(),
                target_master_names: vec!["Fallout4.esm".into()],
                resolution_mode: ResolutionMode::Strict,
                ..MapperOptions::default()
            },
            &interner,
        );
        let mut placed_aliases = PlacedActorAliasResolver::default();
        for (source_local, target_local) in [(0x12319B, 0x000A00), (0x12319C, 0x000A01)] {
            placed_aliases
                .register(PlacedActorAliasTarget {
                    source_placed: FormKey {
                        local: source_local,
                        plugin: source_plugin,
                    },
                    target_placed: FormKey {
                        local: target_local,
                        plugin: output_plugin,
                    },
                    source_base: FormKey {
                        local: source_local + 1,
                        plugin: source_plugin,
                    },
                    target_base: FormKey {
                        local: 0x000007,
                        plugin: fallout4,
                    },
                })
                .unwrap();
        }
        let raw_formids = |raw: u32| {
            Some(FormKey {
                local: raw & 0x00FF_FFFF,
                plugin: source_plugin,
            })
        };
        let record = json!({
            "eid": "AliasRoundtripQuest",
            "fields": [
                { "DATA": raw_hex_value(&[0, 50, 0, 0, 0, 0, 0, 0]) },
                { "INDX": 10 },
                { "QSDT": 0 },
                { "QOBJ": 10 },
                { "NNAM": "Reach the actors." },
                { "QSTA": legacy_qsta(0x12319B) },
                { "QSTA": legacy_qsta(0x12319C) }
            ]
        });
        let mut translated = lower_qust_record(
            &record,
            "B21",
            "000800:FalloutNV.esm",
            "000900:AliasRoundtrip.esp",
            LegacyConditionFamily::Fnv,
            &mut mapper,
            &raw_formids,
            &placed_aliases,
        )
        .unwrap();
        append_unfilled_package_data_alias(
            &mut translated,
            "PackageData",
            "000B00:AliasRoundtrip.esp",
        )
        .unwrap();

        let handle = plugin_handle_new_no_py(OUTPUT_PLUGIN, Some("fo4"));
        plugin_handle_add_master_no_py(handle, "Fallout4.esm", None).unwrap();
        let cell = parsed_record(
            "CELL",
            OWN_INDEX | 0x000A10,
            vec![parsed_subrecord("DATA", Bytes::from(vec![1]))],
        );
        let actors = [0x000A00, 0x000A01]
            .into_iter()
            .map(|local| {
                parsed_record(
                    "ACHR",
                    OWN_INDEX | local,
                    vec![
                        parsed_subrecord("EDID", Bytes::from(format!("AliasActor{local:06X}\0"))),
                        parsed_subrecord(
                            "NAME",
                            Bytes::from(0x0000_0007_u32.to_le_bytes().to_vec()),
                        ),
                    ],
                )
            })
            .collect();
        insert_interior_cell_with_children(handle, cell, actors, Vec::new()).unwrap();
        insert_parsed_record(
            handle,
            parsed_record(
                "PACK",
                OWN_INDEX | 0x000B00,
                vec![parsed_subrecord(
                    "EDID",
                    Bytes::from_static(b"AliasPackage\0"),
                )],
            ),
        )
        .unwrap();
        insert_authoring_record_value(handle, &translated.authoring_record_payload)
            .expect("typed QUST aliases must insert through the native authoring boundary");

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(OUTPUT_PLUGIN);
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        assert!(plugin_handle_close_native(handle));
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let quest =
            plugin_handle_read_authoring_record_value_json(reopened, "AliasRoundtrip.esp:000900")
                .unwrap()
                .unwrap();
        let fields = quest["fields"].as_array().unwrap();
        let forced_references = fields
            .iter()
            .filter_map(|field| field.get("Reference"))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            forced_references,
            [
                authoring_form_reference(OUTPUT_PLUGIN, 0x000A00),
                authoring_form_reference(OUTPUT_PLUGIN, 0x000A01),
            ]
        );
        assert!(fields.iter().any(|field| {
            field.get("Package") == Some(&authoring_form_reference(OUTPUT_PLUGIN, 0x000B00))
        }));
        for (form_key, eid) in [
            ("AliasRoundtrip.esp:000A00", "AliasActor000A00"),
            ("AliasRoundtrip.esp:000A01", "AliasActor000A01"),
            ("AliasRoundtrip.esp:000B00", "AliasPackage"),
        ] {
            let target = plugin_handle_read_authoring_record_value_json(reopened, form_key)
                .unwrap()
                .unwrap();
            assert_eq!(target["eid"], eid);
        }
        assert!(plugin_handle_close_native(reopened));
    }

    #[test]
    fn objective_index_outside_fo4_range_fails_closed() {
        let record = json!({
            "eid": "BadObjective",
            "fields": [
                { "INDX": 10 }, { "QSDT": 0 },
                { "QOBJ": 65536 }
            ]
        });
        assert!(matches!(
            LegacyQuestIr::parse(&record),
            Err(QuestLoweringError::ObjectiveOutOfRange { value: 65536 })
        ));
    }

    #[test]
    fn unmapped_required_objective_target_fails_closed() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FalloutNV.esm");
        let mut mapper = mapper(&interner);
        let raw_formids = |raw: u32| {
            Some(FormKey {
                local: raw & 0x00FF_FFFF,
                plugin,
            })
        };
        let record = json!({
            "eid": "MissingTarget",
            "fields": [
                { "INDX": 10 }, { "QSDT": 0 },
                { "QOBJ": 10 }, { "QSTA": legacy_qsta(0x12319B) }
            ]
        });
        let error = lower_qust_record(
            &record,
            "B21",
            "000100:FalloutNV.esm",
            "100100:FalloutNV.esm",
            LegacyConditionFamily::Fnv,
            &mut mapper,
            &raw_formids,
            &PlacedActorAliasResolver::default(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            QuestLoweringError::MissingRequiredTarget { objective: 10, .. }
        ));
    }

    #[test]
    fn unmapped_required_condition_reference_fails_closed() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let record = json!({
            "eid": "MissingConditionReference",
            "fields": [
                { "DATA": raw_hex_value(&[0; 8]) },
                { "CTDA": raw_hex_value(&hex::decode("0000000000000000C1010000DF8F0500000000000000000000000000").unwrap()) },
                { "INDX": 10 }, { "QSDT": 0 }
            ]
        });
        let raw_formids = |_raw: u32| None;
        let error = lower_qust_record(
            &record,
            "B21",
            "000100:FalloutNV.esm",
            "100100:FalloutNV.esm",
            LegacyConditionFamily::Fnv,
            &mut mapper,
            &raw_formids,
            &PlacedActorAliasResolver::default(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            QuestLoweringError::ConditionLowering { scope, .. } if scope == "start"
        ));
    }
}
