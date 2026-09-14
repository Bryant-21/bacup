//! Production SCPT → Papyrus integration.

use std::collections::{HashMap, HashSet};

use encoding_rs::WINDOWS_1252;
use fnv_script_native::ast::{BinOp, Block, Expr, FunctionCall, LValue, Script, Stmt};
use fnv_script_native::context::{
    FnvScriptContext as NativeScriptContext, SymbolMetadata, TargetMetadata,
};
use fnv_script_native::emit::emit_psc;
use fnv_script_native::lower::lower;
use fnv_script_native::parser::parse_script;
use serde::Serialize;
use serde_json::Value;

use super::TranslateError;
use super::function_map::{
    load_native_actor_value_map, load_native_function_map_for_exact_slice_record,
};
use super::naming::standalone_script_name;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PapyrusType {
    ObjectReference,
    Actor,
    Quest,
    ActiveMagicEffect,
}

impl PapyrusType {
    pub fn extends_class(self) -> &'static str {
        match self {
            Self::ObjectReference => "ObjectReference",
            Self::Actor => "Actor",
            Self::Quest => "Quest",
            Self::ActiveMagicEffect => "ActiveMagicEffect",
        }
    }

    pub fn from_schr_type(value: i64) -> Self {
        match value {
            1 => Self::Quest,
            0x100 => Self::ActiveMagicEffect,
            _ => Self::ObjectReference,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptCompileStatus {
    SourceGeneratedPendingCompile,
    CompiledSuccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptTerminalStatus {
    SourceGeneratedPendingCompile,
    MissingSource,
    ScdaOnlyUnsupported,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslatedProperty {
    pub source_name: String,
    pub papyrus_name: String,
    pub papyrus_type: String,
    pub source_form_key: String,
    pub target_form_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScptAttachmentEvidence {
    pub source_target_form_key: String,
    pub target_signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExactConsumedScroDependency {
    pub source_symbol: &'static str,
    pub source_local: u32,
    pub semantic: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageDataAliasContract {
    pub source_script_form_key: String,
    pub owner_quest_property: String,
    pub source_quest_form_key: String,
    pub target_quest_form_key: String,
    pub actor_expression: String,
    pub property_name: String,
    pub requested_package_property: String,
    pub source_package_form_key: String,
    pub target_package_form_key: String,
    pub alias_package_subrecord: String,
    pub operation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DedicatedTopicResponseContract {
    pub source_info_form_key: String,
    pub response_text: String,
    pub goodbye: bool,
    pub random: bool,
    pub random_end: bool,
    pub emotion: String,
    pub emotion_value: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DedicatedTopicContract {
    pub source_script_form_key: String,
    pub keyword_property_name: String,
    pub generated_keyword_editor_id: String,
    pub source_dialogue_form_key: String,
    pub owner_quest_form_key: String,
    pub condition_function_id: u16,
    pub condition_actor_form_key: String,
    pub source_voice_type_form_key: String,
    pub target_info_count: u16,
    pub route_to_scene: bool,
    pub invocation: String,
    pub responses: Vec<DedicatedTopicResponseContract>,
}

#[derive(Debug, Clone)]
pub struct TranslatedScript {
    pub source_editor_id: String,
    pub source_form_key: String,
    pub script_class_name: String,
    pub papyrus_type: PapyrusType,
    pub psc_text: String,
    pub properties: Vec<TranslatedProperty>,
    pub package_data_aliases: Vec<PackageDataAliasContract>,
    pub dedicated_topics: Vec<DedicatedTopicContract>,
    pub compile_status: ScriptCompileStatus,
    pub terminal_status: ScriptTerminalStatus,
}

#[derive(Debug, Clone)]
pub struct ScptTranslationOutcome {
    pub source_editor_id: String,
    pub source_form_key: String,
    pub status: ScriptTerminalStatus,
    pub translated: Option<TranslatedScript>,
    pub diagnostic: Option<String>,
}

pub fn translate_scpt_record(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
) -> Result<TranslatedScript, TranslateError> {
    let mapped = mapped_form_keys_from_payload(record);
    translate_scpt_record_with_form_key_map(record, mod_prefix, source_form_key, &mapped)
}

pub fn translate_scpt_record_with_form_key_map(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
    mapped_form_keys: &HashMap<String, String>,
) -> Result<TranslatedScript, TranslateError> {
    translate_scpt_record_with_form_key_map_and_attachments(
        record,
        mod_prefix,
        source_form_key,
        mapped_form_keys,
        &[],
    )
}

pub fn translate_scpt_record_with_form_key_map_and_attachments(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
    mapped_form_keys: &HashMap<String, String>,
    attachments: &[ScptAttachmentEvidence],
) -> Result<TranslatedScript, TranslateError> {
    let outcome = translate_scpt_record_outcome_with_attachments(
        record,
        mod_prefix,
        source_form_key,
        mapped_form_keys,
        attachments,
    );
    match outcome.translated {
        Some(translated) => Ok(translated),
        None => Err(TranslateError::Semantic(outcome.diagnostic.unwrap_or_else(
            || format!("SCPT translation ended as {:?}", outcome.status),
        ))),
    }
}

pub fn translate_scpt_record_with_attachments(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
    attachments: &[ScptAttachmentEvidence],
) -> Result<TranslatedScript, TranslateError> {
    let mapped = mapped_form_keys_from_payload(record);
    translate_scpt_record_with_form_key_map_and_attachments(
        record,
        mod_prefix,
        source_form_key,
        &mapped,
        attachments,
    )
}

pub fn translate_scpt_record_outcome(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
    mapped_form_keys: &HashMap<String, String>,
) -> ScptTranslationOutcome {
    translate_scpt_record_outcome_with_attachments(
        record,
        mod_prefix,
        source_form_key,
        mapped_form_keys,
        &[],
    )
}

pub fn translate_scpt_record_outcome_with_attachments(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
    mapped_form_keys: &HashMap<String, String>,
    attachments: &[ScptAttachmentEvidence],
) -> ScptTranslationOutcome {
    let eid = extract_eid(record);
    match translate_scpt_record_inner(
        record,
        mod_prefix,
        source_form_key,
        mapped_form_keys,
        attachments,
    ) {
        Ok(translated) => ScptTranslationOutcome {
            source_editor_id: eid,
            source_form_key: source_form_key.to_string(),
            status: ScriptTerminalStatus::SourceGeneratedPendingCompile,
            translated: Some(translated),
            diagnostic: None,
        },
        Err((status, diagnostic)) => ScptTranslationOutcome {
            source_editor_id: eid,
            source_form_key: source_form_key.to_string(),
            status,
            translated: None,
            diagnostic: Some(diagnostic),
        },
    }
}

fn translate_scpt_record_inner(
    record: &Value,
    mod_prefix: &str,
    source_form_key: &str,
    mapped_form_keys: &HashMap<String, String>,
    attachments: &[ScptAttachmentEvidence],
) -> Result<TranslatedScript, (ScriptTerminalStatus, String)> {
    let eid = extract_eid(record);
    let source_local = source_form_key
        .split([':', '@'])
        .next()
        .and_then(|local| {
            u32::from_str_radix(local.trim_start_matches("0x").trim_start_matches("0X"), 16).ok()
        })
        .filter(|local| *local <= 0x00FF_FFFF)
        .ok_or_else(|| {
            (
                ScriptTerminalStatus::Unsupported,
                format!("SCPT '{eid}' has invalid source FormKey '{source_form_key}'"),
            )
        })?;
    let class_name = standalone_script_name(mod_prefix, source_local);
    if class_name.len() > 38 {
        return Err((
            ScriptTerminalStatus::Unsupported,
            format!(
                "SCPT '{eid}' compact class name '{class_name}' exceeds the FO4 Papyrus 38-character limit"
            ),
        ));
    }
    let source = match extract_script_source(record) {
        SourcePayload::Decoded(source) if !source.trim().is_empty() => source,
        SourcePayload::ScdaOnly => {
            return Err((
                ScriptTerminalStatus::ScdaOnlyUnsupported,
                format!(
                    "SCPT '{eid}' has SCDA bytes but no SCTX source; SCDA decompilation is unsupported"
                ),
            ));
        }
        SourcePayload::Missing | SourcePayload::Decoded(_) => {
            return Err((
                ScriptTerminalStatus::MissingSource,
                format!("SCPT '{eid}' has no usable SCTX source"),
            ));
        }
        SourcePayload::Invalid(error) => {
            return Err((
                ScriptTerminalStatus::Unsupported,
                format!("SCPT '{eid}': {error}"),
            ));
        }
    };

    let mut script = parse_script(&source).map_err(|error| {
        (
            ScriptTerminalStatus::Unsupported,
            format!("SCPT '{eid}' parse failed: {error}"),
        )
    })?;
    validate_exact_slice_source(&eid, source_form_key, &source)
        .map_err(|error| (ScriptTerminalStatus::Unsupported, error))?;
    normalize_exact_slice_ast(&mut script, &eid, source_form_key)
        .map_err(|error| (ScriptTerminalStatus::Unsupported, error))?;
    let papyrus_type = papyrus_type_for_record(record, &eid, source_form_key, attachments)
        .map_err(|error| (ScriptTerminalStatus::Unsupported, error))?;
    let mut target = TargetMetadata::for_extends(papyrus_type.extends_class());
    if exact_source_identity(source_form_key, 0x123191)
        && eid.eq_ignore_ascii_case("TecMineHostage")
    {
        target.insert_symbol(
            "akKiller",
            SymbolMetadata::new("akKiller", "Actor").intrinsic(),
        );
    }
    let mut properties = populate_external_symbols(
        &script,
        record,
        &eid,
        source_form_key,
        mod_prefix,
        papyrus_type,
        mapped_form_keys,
        &mut target,
    )
    .map_err(|error| (ScriptTerminalStatus::Unsupported, error))?;
    let package_data_aliases = exact_package_data_aliases(&eid, source_form_key, &properties)
        .map_err(|error| (ScriptTerminalStatus::Unsupported, error))?;
    let dedicated_topics = exact_dedicated_topic_contracts(&eid, source_form_key);
    add_exact_slice_bindings(
        &eid,
        source_form_key,
        mod_prefix,
        &mut target,
        &mut properties,
    )
    .map_err(|error| (ScriptTerminalStatus::Unsupported, error))?;

    let ctx = NativeScriptContext {
        function_map: load_native_function_map_for_exact_slice_record(
            &eid,
            source_form_key,
            mod_prefix,
        )
        .map_err(|error| {
            (
                ScriptTerminalStatus::Unsupported,
                format!("SCPT '{eid}' load function map: {error}"),
            )
        })?,
        actor_value_map: load_native_actor_value_map().map_err(|error| {
            (
                ScriptTerminalStatus::Unsupported,
                format!("SCPT '{eid}' load actor values: {error}"),
            )
        })?,
        mod_prefix: mod_prefix.to_string(),
        strict: true,
        script_class_name: class_name.clone(),
        papyrus_extends: papyrus_type.extends_class().to_string(),
        target,
    };
    let module = lower(&script, &ctx).map_err(|error| {
        (
            ScriptTerminalStatus::Unsupported,
            format!("SCPT '{eid}' lowering failed: {error}"),
        )
    })?;

    let mut psc_text = emit_psc(&module);
    if exact_source_identity(source_form_key, 0x11FC64)
        && eid.eq_ignore_ascii_case("VTechatticupQuestScript")
    {
        psc_text.push_str(
            "\nFunction AddFreedHostage()\n    NumHostages += 1\nEndFunction\n\nFunction MarkHostageDead()\n    HostagesDead = 1\nEndFunction\n",
        );
    }
    if exact_source_identity(source_form_key, 0x123191)
        && eid.eq_ignore_ascii_case("TecMineHostage")
    {
        psc_text.push_str(
            "\nEvent OnPackageEnd(Package akOldPackage)\n    If akOldPackage == TecMineHostageEscape\n        Self.Disable()\n    EndIf\nEndEvent\n",
        );
    }
    for contract in &package_data_aliases {
        insert_psc_property(
            &mut psc_text,
            &format!(
                "ReferenceAlias Property {} Auto Const",
                contract.property_name
            ),
        );
    }
    Ok(TranslatedScript {
        source_editor_id: eid,
        source_form_key: source_form_key.to_string(),
        script_class_name: class_name,
        papyrus_type,
        psc_text,
        properties,
        package_data_aliases,
        dedicated_topics,
        compile_status: ScriptCompileStatus::SourceGeneratedPendingCompile,
        terminal_status: ScriptTerminalStatus::SourceGeneratedPendingCompile,
    })
}

fn validate_exact_slice_source(
    editor_id: &str,
    source_form_key: &str,
    source: &str,
) -> Result<(), String> {
    let local = source_form_key
        .split([':', '@'])
        .next()
        .and_then(|value| u32::from_str_radix(value.trim_start_matches("0x"), 16).ok());
    let normalized = source
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    let required = match (local, editor_id.to_ascii_lowercase().as_str()) {
        (Some(0x134491), "nvtechatticuprenoldsdialoguescript") => {
            ["nvtecncrrenoldsref.addscriptpackage techaticupncrrenoldsdialoguepackage"].as_slice()
        }
        (Some(0x123191), "tecminehostage") => [
            "addreputation repnvncr 0 3",
            "addscriptpackage tecminehostageescape",
        ]
        .as_slice(),
        _ => return Ok(()),
    };
    for expected in required {
        if !normalized.contains(expected) {
            return Err(format!(
                "SCPT '{editor_id}' exact-slice semantic audit mismatch: missing `{expected}`"
            ));
        }
    }
    Ok(())
}

const HOSTAGE_SEMANTIC_SCRO: [ExactConsumedScroDependency; 3] = [
    ExactConsumedScroDependency {
        source_symbol: "Player",
        source_local: 0x000014,
        semantic: "Player is resolved by the Game.GetPlayer intrinsic",
    },
    ExactConsumedScroDependency {
        source_symbol: "RepNVNCR",
        source_local: 0x0F43DE,
        semantic: "FNV reputation state is externalized to FnvSliceCompat",
    },
    ExactConsumedScroDependency {
        source_symbol: "GREETING",
        source_local: 0x0000C8,
        semantic: "global GREETING is projected to the audited three-INFO hostage Topic",
    },
];

const RENOLDS_TRIGGER_SEMANTIC_SCRO: [ExactConsumedScroDependency; 1] =
    [ExactConsumedScroDependency {
        source_symbol: "Player",
        source_local: 0x000014,
        semantic: "Player is resolved by the Game.GetPlayer intrinsic",
    }];

pub fn exact_consumed_scro_dependencies(
    editor_id: &str,
    source_form_key: &str,
) -> &'static [ExactConsumedScroDependency] {
    if exact_source_identity(source_form_key, 0x123191)
        && editor_id.eq_ignore_ascii_case("TecMineHostage")
    {
        &HOSTAGE_SEMANTIC_SCRO
    } else if exact_source_identity(source_form_key, 0x134491)
        && editor_id.eq_ignore_ascii_case("NVTechatticupRenoldsDialogueScript")
    {
        &RENOLDS_TRIGGER_SEMANTIC_SCRO
    } else {
        &[]
    }
}

pub fn exact_dedicated_topic_contracts(
    editor_id: &str,
    source_form_key: &str,
) -> Vec<DedicatedTopicContract> {
    if !exact_source_identity(source_form_key, 0x123191)
        || !editor_id.eq_ignore_ascii_case("TecMineHostage")
    {
        return Vec::new();
    }
    vec![DedicatedTopicContract {
        source_script_form_key: source_form_key.into(),
        keyword_property_name: "TecMineHostageFreedGreeting".into(),
        generated_keyword_editor_id: "FNV_FO3_TecMineHostageFreedGreeting".into(),
        source_dialogue_form_key: "0000C8:FalloutNV.esm".into(),
        owner_quest_form_key: "11F935:FalloutNV.esm".into(),
        condition_function_id: 72,
        condition_actor_form_key: "123193:FalloutNV.esm".into(),
        source_voice_type_form_key: "02AB62:FalloutNV.esm".into(),
        target_info_count: 3,
        route_to_scene: false,
        invocation: "Self.SayCustom(TecMineHostageFreedGreeting, None, false, Game.GetPlayer())"
            .into(),
        responses: vec![
            DedicatedTopicResponseContract {
                source_info_form_key: "15734B:FalloutNV.esm".into(),
                response_text: "I'm getting out of here.".into(),
                goodbye: true,
                random: true,
                random_end: false,
                emotion: "Fear".into(),
                emotion_value: 20,
            },
            DedicatedTopicResponseContract {
                source_info_form_key: "15734C:FalloutNV.esm".into(),
                response_text: "Fuck this place.".into(),
                goodbye: true,
                random: true,
                random_end: false,
                emotion: "Anger".into(),
                emotion_value: 20,
            },
            DedicatedTopicResponseContract {
                source_info_form_key: "15734D:FalloutNV.esm".into(),
                response_text: "The Legion will pay for this.".into(),
                goodbye: true,
                random: true,
                random_end: true,
                emotion: "Anger".into(),
                emotion_value: 20,
            },
        ],
    }]
}

pub fn is_exact_compat_consumed_scro(
    editor_id: &str,
    source_form_key: &str,
    source_scro_form_key: &str,
) -> bool {
    let Some((local, plugin)) = form_key_identity(source_scro_form_key) else {
        return false;
    };
    plugin.eq_ignore_ascii_case("FalloutNV.esm")
        && exact_consumed_scro_dependencies(editor_id, source_form_key)
            .iter()
            .any(|dependency| dependency.source_local == local)
}

fn form_key_identity(form_key: &str) -> Option<(u32, &str)> {
    let (local, plugin) = form_key.split_once([':', '@'])?;
    let local = u32::from_str_radix(local.trim_start_matches("0x"), 16).ok()?;
    Some((local, plugin))
}

fn exact_source_identity(form_key: &str, expected_local: u32) -> bool {
    matches!(
        form_key_identity(form_key),
        Some((local, plugin))
            if local == expected_local && plugin.eq_ignore_ascii_case("FalloutNV.esm")
    )
}

fn exact_package_data_aliases(
    editor_id: &str,
    source_form_key: &str,
    properties: &[TranslatedProperty],
) -> Result<Vec<PackageDataAliasContract>, String> {
    let audited = if exact_source_identity(source_form_key, 0x123191)
        && editor_id.eq_ignore_ascii_case("TecMineHostage")
    {
        Some((
            "Self",
            "TecMineHostageEscapeData",
            "TecMineHostageEscape",
            0x1231B6,
        ))
    } else if exact_source_identity(source_form_key, 0x134491)
        && editor_id.eq_ignore_ascii_case("NVTechatticupRenoldsDialogueScript")
    {
        Some((
            "NVTecNCRRenoldsREF",
            "TechaticupNCRRenoldsDialoguePackageData",
            "TechaticupNCRRenoldsDialoguePackage",
            0x13289E,
        ))
    } else {
        None
    };
    let Some((actor_expression, property_name, package_property, package_local)) = audited else {
        return Ok(Vec::new());
    };
    let quest = properties
        .iter()
        .find(|property| property.source_name.eq_ignore_ascii_case("VTechatticup"))
        .ok_or_else(|| format!("SCPT '{editor_id}' package alias requires VTechatticup QUST"))?;
    let target_quest_form_key = quest.target_form_key.clone().ok_or_else(|| {
        format!("SCPT '{editor_id}' package alias requires mapped VTechatticup QUST")
    })?;
    let package = properties
        .iter()
        .find(|property| {
            property.source_name.eq_ignore_ascii_case(package_property)
                && form_key_has_local(&property.source_form_key, package_local)
        })
        .ok_or_else(|| {
            format!("SCPT '{editor_id}' package alias requires audited PACK {package_local:06X}")
        })?;
    let target_package_form_key = package.target_form_key.clone().ok_or_else(|| {
        format!("SCPT '{editor_id}' package alias requires mapped PACK {package_local:06X}")
    })?;
    Ok(vec![PackageDataAliasContract {
        source_script_form_key: source_form_key.to_string(),
        owner_quest_property: "VTechatticup".into(),
        source_quest_form_key: quest.source_form_key.clone(),
        target_quest_form_key,
        actor_expression: actor_expression.into(),
        property_name: property_name.into(),
        requested_package_property: package_property.into(),
        source_package_form_key: package.source_form_key.clone(),
        target_package_form_key,
        alias_package_subrecord: "ALPC".into(),
        operation: format!(
            "{property_name}.ApplyToRef({actor_expression}); {actor_expression}.EvaluatePackage(true)"
        ),
    }])
}

fn normalize_exact_slice_ast(
    script: &mut Script,
    editor_id: &str,
    source_form_key: &str,
) -> Result<(), String> {
    if !exact_source_identity(source_form_key, 0x123191)
        || !editor_id.eq_ignore_ascii_case("TecMineHostage")
    {
        return Ok(());
    }
    merge_exact_hostage_death_blocks(script)?;
    let mut normalized_reputation = 0;
    let mut normalized_greeting = 0;
    for block in &mut script.blocks {
        normalize_hostage_statements(
            &mut block.statements,
            &mut normalized_reputation,
            &mut normalized_greeting,
        )?;
    }
    if normalized_reputation != 1 {
        return Err(format!(
            "SCPT '{editor_id}' expected one exact RepNVNCR reputation call, observed {normalized_reputation}"
        ));
    }
    if normalized_greeting != 1 {
        return Err(format!(
            "SCPT '{editor_id}' expected one exact SayTo player GREETING call, observed {normalized_greeting}"
        ));
    }
    let mut freed_updates = 0;
    let mut death_updates = 0;
    for block in &mut script.blocks {
        normalize_hostage_state_statements(
            &mut block.statements,
            &mut freed_updates,
            &mut death_updates,
        );
    }
    if freed_updates != 1 || death_updates != 1 {
        return Err(format!(
            "SCPT '{editor_id}' hostage quest-state audit changed: freed={freed_updates} dead={death_updates}"
        ));
    }
    Ok(())
}

fn merge_exact_hostage_death_blocks(script: &mut Script) -> Result<(), String> {
    let death_indices = script
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.event.eq_ignore_ascii_case("OnDeath"))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if death_indices.len() != 2 {
        return Err(format!(
            "TecMineHostage death event audit changed: expected=2 observed={}",
            death_indices.len()
        ));
    }
    let unfiltered = death_indices
        .iter()
        .copied()
        .find(|index| script.blocks[*index].args.is_empty())
        .ok_or_else(|| "TecMineHostage lacks unfiltered OnDeath block".to_string())?;
    let player_filtered = death_indices
        .iter()
        .copied()
        .find(|index| {
            matches!(script.blocks[*index].args.as_slice(), [Expr::Ident(name)] if name.eq_ignore_ascii_case("Player"))
        })
        .ok_or_else(|| "TecMineHostage lacks player-filtered OnDeath block".to_string())?;
    let filtered = script.blocks.remove(player_filtered);
    let unfiltered = if player_filtered < unfiltered {
        unfiltered - 1
    } else {
        unfiltered
    };
    script.blocks[unfiltered].statements.push(Stmt::If {
        cond: Expr::BinOp {
            op: BinOp::Eq,
            lhs: Box::new(Expr::Ident("akKiller".into())),
            rhs: Box::new(Expr::Ident("Player".into())),
        },
        then_branch: filtered.statements,
        elif_branches: Vec::new(),
        else_branch: Vec::new(),
    });
    Ok(())
}

fn normalize_hostage_state_statements(
    statements: &mut [Stmt],
    freed_updates: &mut usize,
    death_updates: &mut usize,
) {
    for statement in statements {
        let replacement = match statement {
            Stmt::Set {
                target: LValue::Member { receiver, name },
                value,
            } if expr_ident_eq(receiver, "VTechatticup")
                && name.eq_ignore_ascii_case("HostagesDead")
                && matches!(value, Expr::Int(1)) =>
            {
                *death_updates += 1;
                Some("MarkHostageDead")
            }
            Stmt::Set {
                target: LValue::Member { receiver, name },
                value:
                    Expr::BinOp {
                        op: BinOp::Add,
                        lhs,
                        rhs,
                    },
            } if expr_ident_eq(receiver, "VTechatticup")
                && name.eq_ignore_ascii_case("NumHostages")
                && matches!(lhs.as_ref(), Expr::Member { receiver, name } if expr_ident_eq(receiver.as_ref(), "VTechatticup") && name.eq_ignore_ascii_case("NumHostages"))
                && matches!(rhs.as_ref(), Expr::Int(1)) =>
            {
                *freed_updates += 1;
                Some("AddFreedHostage")
            }
            _ => None,
        };
        if let Some(name) = replacement {
            *statement = Stmt::Call(FunctionCall {
                name: name.into(),
                receiver: Some(Box::new(Expr::Ident("VTechatticup".into()))),
                args: Vec::new(),
            });
            continue;
        }
        if let Stmt::If {
            then_branch,
            elif_branches,
            else_branch,
            ..
        } = statement
        {
            normalize_hostage_state_statements(then_branch, freed_updates, death_updates);
            for (_, branch) in elif_branches {
                normalize_hostage_state_statements(branch, freed_updates, death_updates);
            }
            normalize_hostage_state_statements(else_branch, freed_updates, death_updates);
        }
    }
}

fn expr_ident_eq(expression: &Expr, expected: &str) -> bool {
    matches!(expression, Expr::Ident(name) if name.eq_ignore_ascii_case(expected))
}

fn normalize_hostage_statements(
    statements: &mut [Stmt],
    normalized_reputation: &mut usize,
    normalized_greeting: &mut usize,
) -> Result<(), String> {
    for statement in statements {
        match statement {
            Stmt::Set { value, .. } => {
                normalize_hostage_expr(value, normalized_reputation, normalized_greeting)?
            }
            Stmt::If {
                cond,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                normalize_hostage_expr(cond, normalized_reputation, normalized_greeting)?;
                normalize_hostage_statements(
                    then_branch,
                    normalized_reputation,
                    normalized_greeting,
                )?;
                for (condition, branch) in elif_branches {
                    normalize_hostage_expr(condition, normalized_reputation, normalized_greeting)?;
                    normalize_hostage_statements(
                        branch,
                        normalized_reputation,
                        normalized_greeting,
                    )?;
                }
                normalize_hostage_statements(
                    else_branch,
                    normalized_reputation,
                    normalized_greeting,
                )?;
            }
            Stmt::Call(call) => {
                normalize_hostage_call(call, normalized_reputation, normalized_greeting)?
            }
            Stmt::Return | Stmt::ScriptBlockEnd => {}
        }
    }
    Ok(())
}

fn normalize_hostage_expr(
    expr: &mut Expr,
    normalized_reputation: &mut usize,
    normalized_greeting: &mut usize,
) -> Result<(), String> {
    match expr {
        Expr::Member { receiver, .. }
        | Expr::UnaryOp {
            operand: receiver, ..
        } => {
            normalize_hostage_expr(receiver, normalized_reputation, normalized_greeting)?;
        }
        Expr::BinOp { lhs, rhs, .. } => {
            normalize_hostage_expr(lhs, normalized_reputation, normalized_greeting)?;
            normalize_hostage_expr(rhs, normalized_reputation, normalized_greeting)?;
        }
        Expr::Call(call) => {
            normalize_hostage_call(call, normalized_reputation, normalized_greeting)?
        }
        Expr::Int(_) | Expr::Float(_) | Expr::String(_) | Expr::Ident(_) => {}
    }
    Ok(())
}

fn normalize_hostage_call(
    call: &mut FunctionCall,
    normalized_reputation: &mut usize,
    normalized_greeting: &mut usize,
) -> Result<(), String> {
    if let Some(receiver) = call.receiver.as_mut() {
        normalize_hostage_expr(receiver, normalized_reputation, normalized_greeting)?;
    }
    for argument in &mut call.args {
        normalize_hostage_expr(argument, normalized_reputation, normalized_greeting)?;
    }
    if call.name.eq_ignore_ascii_case("AddReputation") {
        if call.receiver.is_some()
            || call.args.len() != 3
            || !matches!(&call.args[0], Expr::Ident(name) if name.eq_ignore_ascii_case("RepNVNCR"))
        {
            return Err(
                "TecMineHostage AddReputation must be exact global RepNVNCR <mode> <tier> shape"
                    .to_string(),
            );
        }
        call.name = "ModRepNVNCR".to_string();
        call.args.remove(0);
        *normalized_reputation += 1;
    } else if call.name.eq_ignore_ascii_case("SayTo") {
        if call.receiver.is_some()
            || call.args.len() != 2
            || !matches!(&call.args[0], Expr::Ident(name) if name.eq_ignore_ascii_case("Player"))
            || !matches!(&call.args[1], Expr::Ident(name) if name.eq_ignore_ascii_case("GREETING"))
        {
            return Err(
                "TecMineHostage SayTo must be exact global player GREETING shape".to_string(),
            );
        }
        call.name = "SayTecMineHostageFreedGreeting".to_string();
        call.args.pop();
        *normalized_greeting += 1;
    }
    Ok(())
}

fn add_exact_slice_bindings(
    editor_id: &str,
    source_form_key: &str,
    mod_prefix: &str,
    target: &mut TargetMetadata,
    properties: &mut Vec<TranslatedProperty>,
) -> Result<(), String> {
    let local = source_form_key
        .split([':', '@'])
        .next()
        .and_then(|value| u32::from_str_radix(value.trim_start_matches("0x"), 16).ok());
    if local != Some(0x123191) || !editor_id.eq_ignore_ascii_case("TecMineHostage") {
        return Ok(());
    }
    let quest = properties
        .iter()
        .find(|property| property.source_name.eq_ignore_ascii_case("VTechatticup"))
        .cloned()
        .ok_or_else(|| {
            "SCPT 'TecMineHostage' compatibility binding requires source quest VTechatticup"
                .to_string()
        })?;
    let target_form_key = quest.target_form_key.clone().ok_or_else(|| {
        "SCPT 'TecMineHostage' compatibility binding requires mapped VTechatticup QUST".to_string()
    })?;
    let compatibility_type = format!("{mod_prefix}_FnvSliceCompat");
    target.insert_symbol(
        "FNVSliceCompat",
        SymbolMetadata::new("FNVSliceCompat", &compatibility_type),
    );
    properties.push(TranslatedProperty {
        source_name: "FNVSliceCompat".into(),
        papyrus_name: "FNVSliceCompat".into(),
        papyrus_type: compatibility_type,
        source_form_key: quest.source_form_key,
        target_form_key: Some(target_form_key),
    });
    target.insert_symbol(
        "TecMineHostageFreedGreeting",
        SymbolMetadata::new("TecMineHostageFreedGreeting", "Keyword"),
    );
    properties.push(TranslatedProperty {
        source_name: "TecMineHostageFreedGreeting".into(),
        papyrus_name: "TecMineHostageFreedGreeting".into(),
        papyrus_type: "Keyword".into(),
        source_form_key: "generated:FNV_FO3_TecMineHostageFreedGreeting".into(),
        target_form_key: None,
    });
    Ok(())
}

enum SourcePayload {
    Decoded(String),
    ScdaOnly,
    Missing,
    Invalid(String),
}

fn extract_script_source(record: &Value) -> SourcePayload {
    if let Some(value) = field_value_any(record, &["SCTX", "ScriptSource"]) {
        match decode_sctx(value) {
            Ok(source) if !source.trim().is_empty() => return SourcePayload::Decoded(source),
            Ok(_) => {}
            Err(error) => return SourcePayload::Invalid(error),
        }
    }
    if field_value_any(record, &["SCDA", "CompiledScript"])
        .and_then(extract_raw_bytes)
        .is_some()
    {
        SourcePayload::ScdaOnly
    } else {
        SourcePayload::Missing
    }
}

fn decode_sctx(value: &Value) -> Result<String, String> {
    if let Some(source) = value.as_str() {
        return Ok(source.trim_end_matches('\0').to_string());
    }
    let bytes = extract_raw_bytes(value)
        .ok_or_else(|| "SCTX is not a raw-bytes-hex payload".to_string())?;
    let (decoded, _, _) = WINDOWS_1252.decode(&bytes);
    Ok(decoded.trim_end_matches('\0').to_string())
}

fn extract_raw_bytes(value: &Value) -> Option<Vec<u8>> {
    if let Some(object) = value.as_object() {
        if object.get("encoding").and_then(Value::as_str) == Some("raw-bytes-hex") {
            return object
                .get("hex")
                .and_then(Value::as_str)
                .and_then(|hex| hex::decode(hex).ok());
        }
        if let Some(hex) = object.get("raw_hex").and_then(Value::as_str) {
            return hex::decode(hex).ok();
        }
        if let Some(value) = object.get("compiled_script") {
            return extract_raw_bytes(value);
        }
    }
    value.as_array().map(|values| {
        values
            .iter()
            .filter_map(|value| value.as_u64().map(|value| value as u8))
            .collect()
    })
}

fn extract_eid(record: &Value) -> String {
    record
        .get("eid")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            field_value_any(record, &["EDID", "EditorID"])
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        })
        .unwrap_or("Unnamed")
        .to_string()
}

fn papyrus_type_for_record(
    record: &Value,
    eid: &str,
    source_form_key: &str,
    attachments: &[ScptAttachmentEvidence],
) -> Result<PapyrusType, String> {
    let claims_vtechatticup_quest_script = form_key_has_local(source_form_key, 0x11FC64)
        || eid.eq_ignore_ascii_case("VTechatticupQuestScript");
    let exact_vtechatticup_quest_script = exact_source_identity(source_form_key, 0x11FC64)
        && eid.eq_ignore_ascii_case("VTechatticupQuestScript");
    if claims_vtechatticup_quest_script && !exact_vtechatticup_quest_script {
        return Err(format!(
            "SCPT 'VTechatticupQuestScript' Quest typing requires exact source 11FC64:FalloutNV.esm and EDID, observed '{eid}' at '{source_form_key}'"
        ));
    }
    if exact_vtechatticup_quest_script {
        let exact_attachment = attachments.iter().any(|attachment| {
            attachment.target_signature.eq_ignore_ascii_case("QUST")
                && exact_source_identity(&attachment.source_target_form_key, 0x11F935)
        });
        if !exact_attachment {
            return Err(
                "SCPT 'VTechatticupQuestScript' requires audited QUST 11F935 attachment evidence"
                    .to_string(),
            );
        }
        if attachments.len() != 1 {
            return Err(format!(
                "SCPT 'VTechatticupQuestScript' attachment set changed: expected=1 observed={}",
                attachments.len()
            ));
        }
        return Ok(PapyrusType::Quest);
    }
    if exact_source_identity(source_form_key, 0x123191)
        && eid.eq_ignore_ascii_case("TecMineHostage")
    {
        return Ok(PapyrusType::Actor);
    }
    if exact_source_identity(source_form_key, 0x166305)
        && eid.eq_ignore_ascii_case("NVTechNCRRenoldsSCRIPT")
    {
        let exact_attachment = attachments.iter().any(|attachment| {
            attachment.target_signature.eq_ignore_ascii_case("NPC_")
                && exact_source_identity(&attachment.source_target_form_key, 0x1300F0)
        });
        if !exact_attachment {
            return Err(
                "SCPT 'NVTechNCRRenoldsSCRIPT' requires audited NPC_ 1300F0 attachment evidence"
                    .to_string(),
            );
        }
        if attachments.len() != 1 {
            return Err(format!(
                "SCPT 'NVTechNCRRenoldsSCRIPT' attachment set changed: expected=1 observed={}",
                attachments.len()
            ));
        }
        return Ok(PapyrusType::Actor);
    }
    let value = field_value_any(record, &["SCHR", "BasicScriptData"]);
    let raw_type = value
        .and_then(|value| value.get("type").or_else(|| value.get("Type")))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
                .or_else(|| value.get("value").and_then(Value::as_i64))
        })
        .unwrap_or(0);
    Ok(PapyrusType::from_schr_type(raw_type))
}

fn populate_external_symbols(
    script: &Script,
    record: &Value,
    eid: &str,
    source_form_key: &str,
    mod_prefix: &str,
    papyrus_type: PapyrusType,
    mapped_form_keys: &HashMap<String, String>,
    target: &mut TargetMetadata,
) -> Result<Vec<TranslatedProperty>, String> {
    let mut names = external_symbol_names(script);
    if exact_source_identity(source_form_key, 0x123191)
        && eid.eq_ignore_ascii_case("TecMineHostage")
    {
        names.retain(|name| {
            !matches!(
                name.to_ascii_lowercase().as_str(),
                "getself" | "getdead" | "getbuttonpressed" | "getsitting"
            )
        });
    }
    let references =
        order_exact_slice_references(eid, source_form_key, &names, external_form_keys(record))?;
    if names.len() != references.len() {
        return Err(format!(
            "SCPT '{eid}' external symbol accounting mismatch: source_names={} SCRO={}",
            names.len(),
            references.len()
        ));
    }

    let mut properties = Vec::new();
    for (name, source_reference_form_key) in names.into_iter().zip(references) {
        let binding =
            explicit_symbol_binding(eid, source_form_key, &name, mod_prefix, papyrus_type)?;
        target.insert_symbol(&name, binding.metadata);
        if let Some(papyrus_type) = binding.property_type {
            properties.push(TranslatedProperty {
                source_name: name.clone(),
                papyrus_name: binding.papyrus_name,
                papyrus_type,
                target_form_key: if exact_source_identity(source_form_key, 0x123191)
                    && name.eq_ignore_ascii_case("GREETING")
                {
                    None
                } else {
                    mapped_form_keys
                        .get(&normalize_form_key(&source_reference_form_key))
                        .cloned()
                },
                source_form_key: source_reference_form_key,
            });
        }
    }
    Ok(properties)
}

fn order_exact_slice_references(
    editor_id: &str,
    source_form_key: &str,
    names: &[String],
    mut references: Vec<String>,
) -> Result<Vec<String>, String> {
    let consumed = exact_consumed_scro_dependencies(editor_id, source_form_key);
    references.retain(|reference| {
        !consumed.iter().any(|dependency| {
            form_key_has_local(reference, dependency.source_local)
                && !names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(dependency.source_symbol))
        })
    });
    let expected: &[(&str, u32)] = if exact_source_identity(source_form_key, 0x11FC64)
        && editor_id.eq_ignore_ascii_case("VTechatticupQuestScript")
    {
        &[("NVTecNCRRenoldsREF", 0x134B9C), ("VTechatticup", 0x11F935)]
    } else if exact_source_identity(source_form_key, 0x134491)
        && editor_id.eq_ignore_ascii_case("NVTechatticupRenoldsDialogueScript")
    {
        &[
            ("Player", 0x000014),
            ("VTechatticup", 0x11F935),
            ("NVTecNCRRenoldsREF", 0x134B9C),
            ("TechaticupNCRRenoldsDialoguePackage", 0x13289E),
        ]
    } else if exact_source_identity(source_form_key, 0x123191)
        && editor_id.eq_ignore_ascii_case("TecMineHostage")
    {
        &[
            ("Player", 0x000014),
            ("TecMineHostageMSG", 0x1231B8),
            ("FFSupermutantCaptiveNoActivateMessage", 0x097183),
            ("VTechatticup", 0x11F935),
            ("NCRFactionNV", 0x0A46E7),
            ("TecMineHostageEscape", 0x1231B6),
            ("CaesarsLegionTechMineFaction", 0x1400F9),
        ]
    } else {
        return Ok(references);
    };
    if names.len() != expected.len() {
        return Err(format!(
            "SCPT '{editor_id}' audited symbol count changed: expected={} observed={} names={names:?}",
            expected.len(),
            names.len()
        ));
    }
    let mut ordered = Vec::with_capacity(names.len());
    for name in names {
        let local = expected
            .iter()
            .find(|(expected_name, _)| name.eq_ignore_ascii_case(expected_name))
            .map(|(_, local)| *local)
            .ok_or_else(|| format!("SCPT '{editor_id}' unaudited external symbol '{name}'"))?;
        let reference = references
            .iter()
            .find(|reference| exact_source_identity(reference, local))
            .cloned()
            .ok_or_else(|| {
                format!("SCPT '{editor_id}' missing audited SCRO local {local:06X} for '{name}'")
            })?;
        ordered.push(reference);
    }
    Ok(ordered)
}

fn form_key_has_local(form_key: &str, expected: u32) -> bool {
    form_key
        .split([':', '@'])
        .next()
        .and_then(|local| u32::from_str_radix(local.trim_start_matches("0x"), 16).ok())
        == Some(expected)
}

struct ExplicitSymbolBinding {
    papyrus_name: String,
    property_type: Option<String>,
    metadata: SymbolMetadata,
}

fn explicit_symbol_binding(
    script_eid: &str,
    source_form_key: &str,
    source_name: &str,
    mod_prefix: &str,
    papyrus_type: PapyrusType,
) -> Result<ExplicitSymbolBinding, String> {
    if source_name.eq_ignore_ascii_case("Player") || source_name.eq_ignore_ascii_case("PlayerRef") {
        return Ok(intrinsic("Actor", "Game.GetPlayer()"));
    }
    if source_name.eq_ignore_ascii_case("VTechatticup") {
        let quest_class = standalone_script_name(mod_prefix, 0x11FC64);
        let exact_owner_script = exact_source_identity(source_form_key, 0x11FC64)
            && script_eid.eq_ignore_ascii_case("VTechatticupQuestScript");
        let metadata = SymbolMetadata::new(
            if exact_owner_script {
                "Self"
            } else {
                "VTechatticup"
            },
            if exact_owner_script {
                "Quest"
            } else {
                &quest_class
            },
        )
        .with_static_record_kind("quest")
        .with_member("HostagesDead", "HostagesDead")
        .with_member("NumHostages", "NumHostages")
        .with_member("HostagesFreed", "HostagesFreed")
        .with_member("HostageStorVar", "HostageStorVar")
        .with_member("AddFreedHostage", "AddFreedHostage")
        .with_member("MarkHostageDead", "MarkHostageDead");
        if exact_owner_script && papyrus_type == PapyrusType::Quest {
            let metadata = metadata.intrinsic();
            return Ok(ExplicitSymbolBinding {
                papyrus_name: "Self".into(),
                property_type: None,
                metadata,
            });
        }
        return Ok(external(source_name, &quest_class, metadata));
    }
    let property_type = match source_name.to_ascii_lowercase().as_str() {
        "nvtecncrrenoldsref" | "nvtechncrrenoldsref" => "Actor",
        "tecminehostagemsg" | "ffsupermutantcaptivenoactivatemessage" => "Message",
        "ncrfactionnv" | "caesarslegiontechminefaction" => "Faction",
        "tecminehostageescape" | "techaticupncrrenoldsdialoguepackage" => "Package",
        "greeting" => "Topic",
        other => {
            return Err(format!(
                "SCPT '{script_eid}' external symbol '{source_name}' has no verified FO4 type ({other})"
            ));
        }
    };
    Ok(external(
        source_name,
        property_type,
        SymbolMetadata::new(source_name, property_type),
    ))
}

fn intrinsic(papyrus_type: &str, expression: &str) -> ExplicitSymbolBinding {
    ExplicitSymbolBinding {
        papyrus_name: expression.to_string(),
        property_type: None,
        metadata: SymbolMetadata::new(expression, papyrus_type).intrinsic(),
    }
}

fn external(
    papyrus_name: &str,
    property_type: &str,
    metadata: SymbolMetadata,
) -> ExplicitSymbolBinding {
    ExplicitSymbolBinding {
        papyrus_name: papyrus_name.to_string(),
        property_type: Some(property_type.to_string()),
        metadata,
    }
}

fn external_symbol_names(script: &Script) -> Vec<String> {
    let declared = script
        .variables
        .iter()
        .map(|variable| variable.name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    for block in &script.blocks {
        collect_block_symbols(block, &declared, &mut seen, &mut names);
    }
    names
}

fn collect_block_symbols(
    block: &Block,
    declared: &HashSet<String>,
    seen: &mut HashSet<String>,
    names: &mut Vec<String>,
) {
    for argument in &block.args {
        collect_expr_symbols(argument, declared, seen, names);
    }
    collect_stmt_symbols(&block.statements, declared, seen, names);
}

fn collect_stmt_symbols(
    statements: &[Stmt],
    declared: &HashSet<String>,
    seen: &mut HashSet<String>,
    names: &mut Vec<String>,
) {
    for statement in statements {
        match statement {
            Stmt::Set { target, value } => {
                if let LValue::Member { receiver, .. } = target {
                    collect_expr_symbols(receiver, declared, seen, names);
                }
                collect_expr_symbols(value, declared, seen, names);
            }
            Stmt::If {
                cond,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                collect_expr_symbols(cond, declared, seen, names);
                collect_stmt_symbols(then_branch, declared, seen, names);
                for (condition, branch) in elif_branches {
                    collect_expr_symbols(condition, declared, seen, names);
                    collect_stmt_symbols(branch, declared, seen, names);
                }
                collect_stmt_symbols(else_branch, declared, seen, names);
            }
            Stmt::Call(call) => collect_call_symbols(call, declared, seen, names),
            Stmt::Return | Stmt::ScriptBlockEnd => {}
        }
    }
}

fn collect_call_symbols(
    call: &FunctionCall,
    declared: &HashSet<String>,
    seen: &mut HashSet<String>,
    names: &mut Vec<String>,
) {
    if let Some(receiver) = &call.receiver {
        collect_expr_symbols(receiver, declared, seen, names);
    }
    for argument in &call.args {
        collect_expr_symbols(argument, declared, seen, names);
    }
}

fn collect_expr_symbols(
    expression: &Expr,
    declared: &HashSet<String>,
    seen: &mut HashSet<String>,
    names: &mut Vec<String>,
) {
    match expression {
        Expr::Ident(name) => {
            let normalized = name.to_ascii_lowercase();
            if normalized != "self"
                && normalized != "akkiller"
                && !declared.contains(&normalized)
                && seen.insert(normalized)
            {
                names.push(name.clone());
            }
        }
        Expr::Member { receiver, .. } => collect_expr_symbols(receiver, declared, seen, names),
        Expr::BinOp { lhs, rhs, .. } => {
            collect_expr_symbols(lhs, declared, seen, names);
            collect_expr_symbols(rhs, declared, seen, names);
        }
        Expr::UnaryOp { operand, .. } => collect_expr_symbols(operand, declared, seen, names),
        Expr::Call(call) => collect_call_symbols(call, declared, seen, names),
        Expr::Int(_) | Expr::Float(_) | Expr::String(_) => {}
    }
}

fn external_form_keys(record: &Value) -> Vec<String> {
    let mut output = Vec::new();
    let Some(fields) = record.get("fields").and_then(Value::as_array) else {
        return output;
    };
    for field in fields {
        let Some(object) = field.as_object() else {
            continue;
        };
        for signature in ["SCRO", "GlobalReference"] {
            if let Some(value) = object.get(signature)
                && let Some(form_key) = value_as_form_key(value)
            {
                output.push(form_key);
            }
        }
    }
    output
}

fn value_as_form_key(value: &Value) -> Option<String> {
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }
    let reference = value.get("reference").unwrap_or(value);
    let plugin = reference.get("plugin")?.as_str()?;
    let object_id = reference.get("object_id")?.as_str()?;
    Some(format!("{object_id}:{plugin}"))
}

fn mapped_form_keys_from_payload(record: &Value) -> HashMap<String, String> {
    record
        .get("__mapped_form_keys")
        .and_then(Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter_map(|(source, target)| {
                    target
                        .as_str()
                        .map(|target| (normalize_form_key(source), target.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_form_key(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn insert_psc_property(psc_text: &mut String, declaration: &str) {
    let insert_at = psc_text
        .find('\n')
        .map_or(psc_text.len(), |index| index + 1);
    psc_text.insert_str(insert_at, &format!("\n{declaration}\n"));
}

fn field_value_any<'a>(record: &'a Value, signatures: &[&str]) -> Option<&'a Value> {
    let fields = record.get("fields")?.as_array()?;
    fields.iter().find_map(|entry| {
        let object = entry.as_object()?;
        signatures
            .iter()
            .find_map(|signature| object.get(*signature))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw_source(source: &str) -> Value {
        json!({ "encoding": "raw-bytes-hex", "hex": hex::encode_upper(source.as_bytes()) })
    }

    #[test]
    fn production_raw_sctx_decodes_windows_1252_and_trailing_nul() {
        let record = json!({
            "fields": [{ "SCTX": { "encoding": "raw-bytes-hex", "hex": "73636E20546573748000" } }]
        });
        let SourcePayload::Decoded(source) = extract_script_source(&record) else {
            panic!("expected decoded source");
        };
        assert_eq!(source, "scn Test€");
    }

    #[test]
    fn raw_scda_without_source_has_terminal_status() {
        let record = json!({
            "eid": "CompiledOnly",
            "fields": [{ "SCDA": { "encoding": "raw-bytes-hex", "hex": "010203" } }]
        });
        let outcome =
            translate_scpt_record_outcome(&record, "B21", "001234:FNV.esm", &HashMap::new());
        assert_eq!(outcome.status, ScriptTerminalStatus::ScdaOnlyUnsupported);
        assert!(outcome.translated.is_none());
    }

    #[test]
    fn production_vtech_quest_uses_shared_lowerer_and_typed_binding() {
        let live_sctx_hex = concat!(
            "73636E2056546563686174746963757051756573745363726970740D0A0D0A3B5175657374205661726961626C65730D0A0D0A53686F7274204E756D",
            "486F7374616765730909093B4E756D626572206F6620486F7374616765732046726565640D0A53686F727420486F7374616765734672656564090909",
            "3B313D506C6179657220667265656420616C6C20686F7374616765730D0A53686F727420486F737461676573446561640909093B313D486F73746167",
            "657320446561640D0A53686F727420486F737461676553746F725661720909093B313D52656E6F6C647320746F6C642074686520706C617965722061",
            "626F75742074686520686F7374616765732E20323D20506C61796572206163636570746564207468652071756573742E20333D20517565737420636F",
            "6D706C6574650D0A53686F7274204772656574696E67446F6E650909093B313D52656E6F6C647320477265657465642074686520706C617965720D0A",
            "53686F727420446F4F6E636509090909093B50726576656E7420746865207363726970742066726F6D20726570656174696E6720616674657220636F",
            "6D706C6574696E6720746865206F626A6563746976652E0D0A0D0A424547494E2047616D654D6F64650D0A09496620446F4F6E6365203D3D31090909",
            "093B50726576656E7473207363726970742066726F6D20726570656174696E670D0A090952657475726E0D0A09456C7365696620284E565465634E43",
            "5252656E6F6C64735245462E47657444656164203D3D2031290D0A0909736574537461676520565465636861747469637570203131300D0A09456C73",
            "6569662028486F73746167657344656164203D3D2031290D0A090973657420446F4F6E636520746F20310D0A09456C7365696620284E756D486F7374",
            "61676573203D3D2032292026262028486F737461676573467265656420213D31290D0A090973657420486F737461676573467265656420746F203109",
            "0D0A09456C736549662028486F7374616765734672656564203D3D2031292026262028676574537461676520565465636861747469637570203D3D20",
            "3130290D0A09097365745374616765205654656368617474696375702032300D0A090973657420446F4F6E636520746F20310D0A09456E6449660D0A",
            "456E64",
        );
        let record = json!({
            "eid": "VTechatticupQuestScript",
            "fields": [
                { "SCHR": { "encoding": "raw-bytes-hex", "hex": "0000000002000000D10000000C00000001000100" } },
                { "SCTX": { "raw_hex": live_sctx_hex } },
                { "SCRO": "134B9C:FalloutNV.esm" },
                { "SCRO": "11F935:FalloutNV.esm" }
            ],
            "__mapped_form_keys": {
                "134B9C:FalloutNV.esm": "234B9C:FalloutNV.esm",
                "11F935:FalloutNV.esm": "21F935:FalloutNV.esm"
            }
        });
        let attachment = [ScptAttachmentEvidence {
            source_target_form_key: "11F935:FalloutNV.esm".into(),
            target_signature: "QUST".into(),
        }];
        let translated = translate_scpt_record_with_attachments(
            &record,
            "B21",
            "11FC64:FalloutNV.esm",
            &attachment,
        )
        .unwrap();
        assert_eq!(translated.papyrus_type, PapyrusType::Quest);
        assert_eq!(translated.script_class_name, "B21_S_11FC64");
        assert!(
            translated
                .psc_text
                .starts_with("ScriptName B21_S_11FC64 extends Quest")
        );
        assert!(translated.psc_text.contains("extends Quest"));
        assert!(
            translated
                .psc_text
                .contains("Int Property NumHostages Auto")
        );
        assert!(
            translated
                .psc_text
                .contains("Int Property HostagesDead Auto")
        );
        assert!(
            translated
                .psc_text
                .contains("Actor Property NVTecNCRRenoldsREF Auto Const")
        );
        assert!(translated.psc_text.contains("Self.SetStage(110)"));
        assert!(translated.psc_text.contains("Self.GetStage()"));
        assert!(!translated.psc_text.contains("Quest Property VTechatticup"));
        assert_eq!(translated.properties.len(), 1);
        assert_eq!(translated.properties[0].papyrus_type, "Actor");
        assert_eq!(
            translated.properties[0].target_form_key.as_deref(),
            Some("234B9C:FalloutNV.esm")
        );
        assert_eq!(
            translated.compile_status,
            ScriptCompileStatus::SourceGeneratedPendingCompile
        );

        let missing =
            translate_scpt_record_with_attachments(&record, "B21", "11FC64:FalloutNV.esm", &[])
                .unwrap_err();
        assert!(missing.to_string().contains("requires audited QUST 11F935"));

        let wrong_attachment = [ScptAttachmentEvidence {
            source_target_form_key: "1300F0:FalloutNV.esm".into(),
            target_signature: "NPC_".into(),
        }];
        let wrong = translate_scpt_record_with_attachments(
            &record,
            "B21",
            "11FC64:FalloutNV.esm",
            &wrong_attachment,
        )
        .unwrap_err();
        assert!(wrong.to_string().contains("requires audited QUST 11F935"));

        let extra_attachment = [
            attachment[0].clone(),
            ScptAttachmentEvidence {
                source_target_form_key: "1300F0:FalloutNV.esm".into(),
                target_signature: "NPC_".into(),
            },
        ];
        let extra = translate_scpt_record_with_attachments(
            &record,
            "B21",
            "11FC64:FalloutNV.esm",
            &extra_attachment,
        )
        .unwrap_err();
        assert!(extra.to_string().contains("expected=1 observed=2"));

        let wrong_plugin = translate_scpt_record_with_attachments(
            &record,
            "B21",
            "11FC64:FalloutNV.esm",
            &attachment,
        )
        .unwrap_err();
        assert!(wrong_plugin.to_string().contains("requires exact source"));

        let mut wrong_eid = record.clone();
        wrong_eid["eid"] = json!("WrongQuestScript");
        let wrong_eid = translate_scpt_record_with_attachments(
            &wrong_eid,
            "B21",
            "11FC64:FalloutNV.esm",
            &attachment,
        )
        .unwrap_err();
        assert!(wrong_eid.to_string().contains("requires exact source"));
    }

    #[test]
    fn unsupported_external_symbol_fails_closed() {
        let source = "scn UnknownScript\nBegin OnLoad\n  UnknownREF.Disable\nEnd";
        let record = json!({
            "eid": "UnknownScript",
            "fields": [
                { "SCTX": raw_source(source) },
                { "SCRO": "123456:FNV.esm" }
            ]
        });
        let outcome =
            translate_scpt_record_outcome(&record, "B21", "100000:FNV.esm", &HashMap::new());
        assert_eq!(outcome.status, ScriptTerminalStatus::Unsupported);
        assert!(outcome.diagnostic.unwrap().contains("no verified FO4 type"));
    }

    #[test]
    fn production_aliases_are_accepted_for_authoring_inspection() {
        let record = json!({
            "eid": "AliasShape",
            "fields": [
                { "BasicScriptData": { "Type": { "value": 1 } } },
                { "ScriptSource": { "raw_hex": hex::encode_upper(b"scn AliasShape\nshort x\nBegin GameMode\nset x to 1\nEnd") } }
            ]
        });
        let translated = translate_scpt_record(&record, "B21", "001234:FNV.esm").unwrap();
        assert_eq!(translated.papyrus_type, PapyrusType::Quest);
        assert!(translated.psc_text.contains("extends Quest"));
    }

    #[test]
    fn record_134491_uses_evaluate_package_only_for_the_audited_call() {
        let source = "scn NVTechatticupRenoldsDialogueScript\r\n\r\nBegin OnTriggerEnter Player\r\n\t\r\n\tIf GetStage VTechatticup < 10\r\n\t\tNVTecNCRRenoldsREF.AddScriptPackage TechaticupNCRRenoldsDialoguePackage\r\n\tEndif\r\nEnd";
        assert_eq!(
            hex::encode_upper(source.as_bytes()),
            "73636E204E56546563686174746963757052656E6F6C64734469616C6F6775655363726970740D0A0D0A426567696E204F6E54726967676572456E74657220506C617965720D0A090D0A09496620476574537461676520565465636861747469637570203C2031300D0A09094E565465634E435252656E6F6C64735245462E4164645363726970745061636B61676520546563686174696375704E435252656E6F6C64734469616C6F6775655061636B6167650D0A09456E6469660D0A456E64"
        );
        let record = json!({
            "eid": "NVTechatticupRenoldsDialogueScript",
            "fields": [
                { "SCTX": raw_source(source) },
                { "SCRO": "134B9C:FalloutNV.esm" },
                { "SCRO": "000014:FalloutNV.esm" },
                { "SCRO": "11F935:FalloutNV.esm" },
                { "SCRO": "13289E:FalloutNV.esm" }
            ],
            "__mapped_form_keys": {
                "134B9C:FalloutNV.esm": "234B9C:FalloutNV.esm",
                "11F935:FalloutNV.esm": "21F935:FalloutNV.esm",
                "13289E:FalloutNV.esm": "23289E:FalloutNV.esm"
            }
        });
        let translated = translate_scpt_record(&record, "B21", "134491:FalloutNV.esm").unwrap();
        assert_eq!(translated.script_class_name, "B21_S_134491");
        assert!(
            translated
                .psc_text
                .contains("B21_S_11FC64 Property VTechatticup Auto Const")
        );
        assert!(
            translated
                .psc_text
                .contains("NVTecNCRRenoldsREF.EvaluatePackage(true)")
        );
        assert!(
            translated
                .psc_text
                .contains("TechaticupNCRRenoldsDialoguePackageData.ApplyToRef(NVTecNCRRenoldsREF)")
        );
        assert!(!translated.psc_text.contains("AddScriptPackage"));
        assert!(
            translated
                .psc_text
                .contains("Package Property TechaticupNCRRenoldsDialoguePackage Auto Const")
        );
        assert_eq!(translated.package_data_aliases.len(), 1);
        assert_eq!(
            translated.package_data_aliases[0].property_name,
            "TechaticupNCRRenoldsDialoguePackageData"
        );
        assert_eq!(
            translated.package_data_aliases[0].requested_package_property,
            "TechaticupNCRRenoldsDialoguePackage"
        );
        assert_eq!(
            translated.package_data_aliases[0].source_package_form_key,
            "13289E:FalloutNV.esm"
        );
        assert_eq!(
            translated.package_data_aliases[0].target_package_form_key,
            "23289E:FalloutNV.esm"
        );

        let mut stale_spelling = record.clone();
        stale_spelling["fields"][0]["SCTX"] = raw_source(&source.replace(
            "TechaticupNCRRenoldsDialoguePackage",
            "TechatticupNCRRenoldsDialoguePackage",
        ));
        let error =
            translate_scpt_record(&stale_spelling, "B21", "134491:FalloutNV.esm").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("exact-slice semantic audit mismatch")
        );

        let wrong_id = translate_scpt_record(&record, "B21", "134492:FalloutNV.esm").unwrap_err();
        assert!(wrong_id.to_string().contains("AddScriptPackage"));

        let wrong_plugin =
            translate_scpt_record(&record, "B21", "134491:FalloutNV.esm").unwrap_err();
        assert!(wrong_plugin.to_string().contains("AddScriptPackage"));
    }

    #[test]
    fn record_123191_zero_arg_activate_uses_onactivate_action_ref() {
        let script = parse_script("scn TecMineHostage\nBegin OnActivate\n  Activate\nEnd")
            .expect("live zero-argument Activate shape parses");
        let ctx = NativeScriptContext {
            function_map: load_native_function_map_for_exact_slice_record(
                "TecMineHostage",
                "123191:FalloutNV.esm",
                "FNV_FO3",
            )
            .unwrap(),
            actor_value_map: load_native_actor_value_map().unwrap(),
            mod_prefix: "FNV_FO3".into(),
            strict: true,
            script_class_name: "FNV_FO3_S_123191".into(),
            papyrus_extends: "Actor".into(),
            target: TargetMetadata::for_extends("Actor"),
        };
        let module = lower(&script, &ctx).expect("exact adapter lowers");
        assert!(emit_psc(&module).contains("Self.Activate(akActionRef, true)"));
    }

    #[test]
    fn live_hostage_contract_preserves_ui_reputation_alarm_package_and_dialogue() {
        let source = r#"scn TecMineHostage
short Freed
short Button
short DoOnce
ref hostage
Begin OnLoad
  IgnoreCrime 1
  SetRestrained 1
  set hostage to GetSelf
End
Begin OnActivate
  if GetDead == 0
    ShowMessage TecMineHostageMSG
    ShowMessage FFSupermutantCaptiveNoActivateMessage
  else
    Activate
  endif
End
Begin OnDeath
  SetStage VTechatticup 110
  set VTechatticup.HostagesDead to 1
End
Begin OnDeath Player
  if Freed == 0
    AddReputation RepNVNCR 0 3
  endif
End
Begin GameMode
  set Button to GetButtonPressed
  if Button == 1
    SayTo Player GREETING
    IgnoreCrime 0
    AddToFaction NCRFactionNV 0
    AddScriptPackage TecMineHostageEscape
    set VTechatticup.NumHostages to VTechatticup.NumHostages + 1
    SendAssaultAlarm Player CaesarsLegionTechMineFaction
  endif
End"#;
        let record = json!({
            "eid": "TecMineHostage",
            "fields": [
                { "SCTX": raw_source(source) },
                { "SCRO": "000014:FalloutNV.esm" },
                { "SCRO": "1231B8:FalloutNV.esm" },
                { "SCRO": "097183:FalloutNV.esm" },
                { "SCRO": "11F935:FalloutNV.esm" },
                { "SCRO": "0F43DE:FalloutNV.esm" },
                { "SCRO": "0000C8:FalloutNV.esm" },
                { "SCRO": "0A46E7:FalloutNV.esm" },
                { "SCRO": "1231B6:FalloutNV.esm" },
                { "SCRO": "1400F9:FalloutNV.esm" }
            ],
            "__mapped_form_keys": {
                "1231B8:FalloutNV.esm": "2231B8:Target.esp",
                "097183:FalloutNV.esm": "197183:Target.esp",
                "11F935:FalloutNV.esm": "21F935:Target.esp",
                "0A46E7:FalloutNV.esm": "1A46E7:Target.esp",
                "1231B6:FalloutNV.esm": "2231B6:Target.esp",
                "1400F9:FalloutNV.esm": "2400F9:Target.esp"
            }
        });
        let translated = translate_scpt_record(&record, "FNV_FO3", "123191:FalloutNV.esm").unwrap();
        let psc = &translated.psc_text;
        assert_eq!(translated.script_class_name, "FNV_FO3_S_123191");
        assert!(psc.starts_with("ScriptName FNV_FO3_S_123191 extends Actor"));
        assert!(psc.contains("FNV_FO3_S_11FC64 Property VTechatticup Auto Const"));
        assert!(psc.contains("Button = TecMineHostageMSG.Show()"));
        assert!(psc.contains("Self.Activate(akActionRef, true)"));
        assert!(psc.contains("Self.SetCrimeFaction(None)"));
        assert!(psc.contains("Self.SetCrimeFaction(NCRFactionNV)"));
        assert!(psc.contains("CaesarsLegionTechMineFaction.SendAssaultAlarm()"));
        assert!(psc.contains("FNVSliceCompat.ModRepNVNCR(0, 3)"));
        assert!(psc.contains("VTechatticup.MarkHostageDead()"));
        assert!(psc.contains("VTechatticup.AddFreedHostage()"));
        assert!(psc.contains("TecMineHostageEscapeData.ApplyToRef(Self)"));
        assert!(psc.contains("Self.EvaluatePackage(true)"));
        assert!(psc.contains(
            "Self.SayCustom(TecMineHostageFreedGreeting, None, false, Game.GetPlayer())"
        ));
        assert_eq!(
            psc.matches("Keyword Property TecMineHostageFreedGreeting Auto Const")
                .count(),
            1
        );
        assert_eq!(psc.matches("Event OnDeath(Actor akKiller)").count(), 1);
        assert!(psc.contains("Event OnPackageEnd(Package akOldPackage)"));
        assert!(
            !translated
                .properties
                .iter()
                .any(|property| property.source_name.eq_ignore_ascii_case("RepNVNCR"))
        );
        assert_eq!(translated.package_data_aliases.len(), 1);
        assert_eq!(translated.dedicated_topics.len(), 1);
        let greeting_binding = translated
            .properties
            .iter()
            .filter(|property| property.source_name == "TecMineHostageFreedGreeting")
            .collect::<Vec<_>>();
        assert_eq!(greeting_binding.len(), 1);
        assert_eq!(greeting_binding[0].papyrus_type, "Keyword");
        assert_eq!(
            greeting_binding[0].source_form_key,
            "generated:FNV_FO3_TecMineHostageFreedGreeting"
        );
        assert!(greeting_binding[0].target_form_key.is_none());
    }

    #[test]
    fn renolds_death_script_requires_exact_actor_attachment_evidence() {
        let source = "scn NVTechNCRRenoldsSCRIPT\nBegin OnDeath NVTechNCRRenoldsREF\nSetStage VTechatticup 110\nEnd";
        let record = json!({
            "eid": "NVTechNCRRenoldsSCRIPT",
            "fields": [
                { "SCTX": raw_source(source) },
                { "SCRO": "134B9C:FalloutNV.esm" },
                { "SCRO": "11F935:FalloutNV.esm" }
            ],
            "__mapped_form_keys": {
                "134B9C:FalloutNV.esm": "234B9C:Target.esp",
                "11F935:FalloutNV.esm": "21F935:Target.esp"
            }
        });
        let attachment = [ScptAttachmentEvidence {
            source_target_form_key: "1300F0:FalloutNV.esm".into(),
            target_signature: "NPC_".into(),
        }];
        let translated = translate_scpt_record_with_attachments(
            &record,
            "FNV_FO3",
            "166305:FalloutNV.esm",
            &attachment,
        )
        .unwrap();
        assert_eq!(translated.papyrus_type, PapyrusType::Actor);
        assert!(
            translated
                .psc_text
                .contains("Event OnDeath(Actor akKiller)")
        );

        for bad in [
            ScptAttachmentEvidence {
                source_target_form_key: "1300F0:Wrong.esm".into(),
                target_signature: "NPC_".into(),
            },
            ScptAttachmentEvidence {
                source_target_form_key: "1300F0:FalloutNV.esm".into(),
                target_signature: "REFR".into(),
            },
        ] {
            let error = translate_scpt_record_with_attachments(
                &record,
                "FNV_FO3",
                "166305:FalloutNV.esm",
                &[bad],
            )
            .unwrap_err();
            assert!(error.to_string().contains("attachment evidence"));
        }
    }

    #[test]
    fn semantic_scro_plan_is_exact_source_and_plugin_gated() {
        let plan = exact_consumed_scro_dependencies("TecMineHostage", "123191:FalloutNV.esm");
        assert_eq!(plan.len(), 3);
        assert!(is_exact_compat_consumed_scro(
            "TecMineHostage",
            "123191:FalloutNV.esm",
            "000014:FalloutNV.esm"
        ));
        assert!(is_exact_compat_consumed_scro(
            "TecMineHostage",
            "123191:FalloutNV.esm",
            "0F43DE:FalloutNV.esm"
        ));
        assert!(is_exact_compat_consumed_scro(
            "TecMineHostage",
            "123191:FalloutNV.esm",
            "0000C8:FalloutNV.esm"
        ));
        assert!(!is_exact_compat_consumed_scro(
            "TecMineHostage",
            "123191:FalloutNV.esm",
            "0F43DE:FalloutNV.esm"
        ));
        assert!(
            exact_consumed_scro_dependencies("TecMineHostage", "123191:FalloutNV.esm").is_empty()
        );

        let renolds_plan = exact_consumed_scro_dependencies(
            "NVTechatticupRenoldsDialogueScript",
            "134491:FalloutNV.esm",
        );
        assert_eq!(renolds_plan, &RENOLDS_TRIGGER_SEMANTIC_SCRO);
        assert!(is_exact_compat_consumed_scro(
            "NVTechatticupRenoldsDialogueScript",
            "134491:FalloutNV.esm",
            "000014:FalloutNV.esm"
        ));
        assert!(!is_exact_compat_consumed_scro(
            "NVTechatticupRenoldsDialogueScript",
            "134491:FalloutNV.esm",
            "000014:FalloutNV.esm"
        ));
        assert!(!is_exact_compat_consumed_scro(
            "NVTechatticupRenoldsDialogueScript",
            "123191:FalloutNV.esm",
            "000014:FalloutNV.esm"
        ));
    }
}
