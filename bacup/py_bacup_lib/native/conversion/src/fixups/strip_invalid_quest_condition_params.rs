//! Fixup: drop CTDA conditions whose quest-typed Parameter #1 does not resolve
//! to a QUST, and strip alias references that do not exist on the owning quest.
//!
//! # Why
//! Six FO4 condition functions take a QUST FormID in Parameter #1
//! (`wbDefinitionsFO4.pas`, `Paramtype1: ptQuest`): GetQuestRunning(56),
//! GetStage(58), GetStageDone(59), GetQuestCompleted(543),
//! GetVMQuestVariable(629), HasValidRumorTopic(664). A 0/NULL quest param can be
//! resolved from an owning QNAM/PNAM on quest-context records, but any non-zero
//! value is validated as a literal FormID. After a FO76→FO4 conversion many of
//! these carry a FO76 quest-stage number (e.g. 500, 9000), a dropped-quest id, or
//! a non-quest form sitting in the QUST slot, which xEdit reports as "<X> is not
//! a Quest record".
//!
//! The translation-time pair hook (`drop_fo4_incompatible_conditions`) already
//! drops the NULL (Parameter #1 == 0) case and the RunOn==Quest-Alias case, but
//! the *non-null wrong-type* case can only be decided with FormID resolution
//! against the output plugin + target masters, which the pair hook cannot reach.
//! This session-level fixup closes that remainder.
//!
//! # What this does
//! Builds the set of every valid QUST encoded FormID (output plugin + target
//! masters), plus each output quest's alias ids. Drops every CTDA/CTDT whose
//! function is in the quest-param-1 set and whose Parameter #1 is non-zero but
//! not a known QUST, and drops alias-index conditions whose alias cannot be
//! resolved from the owning quest — together with trailing CIS1/CIS2 strings (an
//! orphaned CIS is an xEdit "out of order subrecord").
//!
//! Idempotent with the pair-hook pass: Parameter #1 == 0 was already dropped, so
//! the non-zero predicate here never re-touches those; a record with no such
//! CTDA is left byte-identical.
//!
//! This fixup also drops CTDA whose function requires a non-null FLST/KYWD/LCTN
//! in Parameter #2 (e.g. `GetInCurrentLocFormList`, 576 — the
//! `GQ_MiscRegionPointer*` region-pointer family) but whose Parameter #2 is NULL.
//! FO76 ships these with Parameter #2 already NULL; FO4 rejects it ("Found a NULL
//! reference, expected: FLST,KYWD,LCTN"). That NULL is in Parameter #2, so it
//! cannot be resolved from the owning-quest context.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::StringInterner;
use esp_authoring_core::plugin_runtime::ParsedItem;

/// FO4 condition functions whose Parameter #1 is a QUST FormID
/// (mirrors `FO4_QUEST_PARAMETER_1_CONDITION_FUNCTION_IDS` in the pair hook).
const QUEST_PARAMETER_1_FUNCTION_IDS: &[u16] = &[56, 58, 59, 543, 629, 664];

/// FO4 condition function whose Parameter #1 is a quest-alias INDEX
/// (GetIsAliasRef). FO76 INFOs carry these with Parameter #1 set to a procedural
/// `0x07A0xxxx` runtime id rather than a real alias index; xEdit reports
/// "Quest Alias [N] not found". Those procedural values can never resolve to a
/// valid alias, so the condition is dropped.
const QUEST_ALIAS_PARAMETER_1_FUNCTION_IDS: &[u16] = &[566];

/// High 16 bits of the FO76 procedural / runtime-generated form-id range
/// (`0x07A0xxxx`): default world, procedural aliases, generated objects. These
/// have no FO4 equivalent and never resolve.
const FO76_PROCEDURAL_FORM_ID_PREFIX: u32 = 0x07A0;
const CTDA_RUN_ON_QUEST_ALIAS: u32 = 5;
const VMAD_NO_ALIAS: u16 = u16::MAX;
const QUST_ALIAS_ANCHOR_SIGS: &[[u8; 4]] = &[*b"ALST", *b"ALLS", *b"ALCS"];

/// FO4 condition functions whose Parameter #2 is a required formlink
/// (FLST/KYWD/LCTN), notably `GetInCurrentLocFormList` (576). FO76 quests
/// (e.g. the `GQ_MiscRegionPointer*` region-pointer family) carry these with
/// Parameter #2 already NULL; FO4's schema rejects a NULL there, so xEdit
/// reports "Found a NULL reference, expected: FLST,KYWD,LCTN". The condition has
/// no target to match and cannot be repaired, so the whole CTDA (plus its
/// trailing CIS1/CIS2) is dropped. Unlike the quest-param-1 rule this applies on
/// every record sig, including the quest-context types, because the NULL is in
/// Parameter #2 (not resolvable from the owning-quest context).
const QUEST_PARAMETER_2_REQUIRED_FORMLINK_FUNCTION_IDS: &[u16] = &[576];

/// `GetInWorldspace`: Parameter #1 is a required WRLD FormID. FO76 conditions
/// can retain the source plugin's load byte (`00`) after the WRLD itself has
/// moved into the output plugin (for example `0025DA15` -> `0725DA15`).
const GET_IN_WORLDSPACE_FUNCTION_ID: u16 = 310;

/// Reference-parameter functions repaired only after every placed child has
/// been materialized: GetDistance and GetWithinDistance.
const FINAL_PERSISTENT_REFERENCE_FUNCTION_IDS: &[u16] = &[1, 639];
const CELL_PERSISTENT_GROUP: i32 = 8;

struct TypedParameter1Rule {
    function_id: u16,
    allowed_sigs: &'static [&'static str],
}

/// FO4 condition functions whose Parameter #1 is a required typed FormID. These
/// are the xEdit-confirmed FO76->FO4 residue classes from dialogue/scene/terminal
/// conditions: raw zero in the slot is invalid, and a non-zero FormID must resolve
/// to one of the listed target signatures.
const TYPED_PARAMETER_1_RULES: &[TypedParameter1Rule] = &[
    TypedParameter1Rule {
        function_id: 14,
        allowed_sigs: &["AVIF"],
    },
    TypedParameter1Rule {
        function_id: 74,
        allowed_sigs: &["GLOB"],
    },
    TypedParameter1Rule {
        function_id: 163,
        allowed_sigs: &["FLST", "FURN"],
    },
    TypedParameter1Rule {
        function_id: 248,
        allowed_sigs: &["SCEN"],
    },
    TypedParameter1Rule {
        function_id: 426,
        allowed_sigs: &["FLST", "VTYP"],
    },
];

pub struct StripInvalidQuestConditionParamsFixup;

impl Fixup for StripInvalidQuestConditionParamsFixup {
    fn name(&self) -> &'static str {
        "strip_invalid_quest_condition_params"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let quest_index = collect_quest_condition_index(
            session,
            mapper.interner,
            config,
            target_schema,
            &mut report,
        )?;
        // No QUST or WRLD anywhere (degenerate) → cannot classify; do nothing
        // rather than drop typed conditions blind.
        if !quest_index.can_classify_conditions() {
            return Ok(report);
        }

        let cnam_sigs = condition_carrier_sigs(session, mapper.interner);
        if cnam_sigs.is_empty() {
            return Ok(report);
        }

        let mut changed_records = Vec::new();
        for sig in cnam_sigs {
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let changed =
                    scrub_invalid_quest_references(&mut record, &quest_index, mapper.interner);
                if changed {
                    changed_records.push(record);
                }
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "strip_invalid_quest_condition_params replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

/// Record signatures present in the output plugin that can carry a CTDA. We can't
/// cheaply ask the index "which sigs have a CTDA", so we conservatively consider
/// every output signature and let `drop_invalid_quest_conditions` no-op on those
/// without a matching CTDA.
fn condition_carrier_sigs(session: &mut PluginSession, _interner: &StringInterner) -> Vec<SigCode> {
    session.target_signatures().unwrap_or_default()
}

pub(crate) struct QuestConditionIndex {
    pub(crate) valid_quest_ids: FxHashSet<u32>,
    quest_index_complete: bool,
    quest_alias_ids_by_encoded_quest: FxHashMap<u32, Option<FxHashSet<u32>>>,
    valid_typed_param1_ids_by_function: FxHashMap<u16, FxHashSet<u32>>,
    valid_worldspace_ids: FxHashSet<u32>,
    output_worldspace_id_by_object_id: FxHashMap<u32, u32>,
    target_masters: Vec<String>,
}

impl QuestConditionIndex {
    fn has_usable_quest_index(&self) -> bool {
        self.quest_index_complete && !self.valid_quest_ids.is_empty()
    }

    pub(crate) fn can_classify_conditions(&self) -> bool {
        self.has_usable_quest_index() || !self.valid_worldspace_ids.is_empty()
    }
}

pub(crate) fn collect_quest_condition_index(
    session: &mut PluginSession,
    interner: &StringInterner,
    config: &FixupConfig,
    target_schema: &AuthoringSchema,
    report: &mut FixupReport,
) -> Result<QuestConditionIndex, FixupError> {
    let (valid_quest_ids, quest_index_complete) =
        collect_valid_quest_encoded_ids(session, interner, config, report)?;
    let target_masters = session.target_masters().to_vec();
    let qust_sig = SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut quest_alias_ids_by_encoded_quest = FxHashMap::default();

    let output_fks = session
        .form_keys_of_sig(qust_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in output_fks {
        let Some(encoded) = encode_form_id(&fk, interner, &target_masters) else {
            continue;
        };
        let alias_ids = session
            .record_decoded(&fk, target_schema, interner)
            .ok()
            .map(|record| collect_alias_ids_from_record(&record));
        quest_alias_ids_by_encoded_quest.insert(encoded, alias_ids);
    }
    for &handle_id in &config.target_master_handle_ids {
        let master_fks = match session.form_keys_of_sig_in_handle(handle_id, qust_sig, interner) {
            Ok(form_keys) => form_keys,
            Err(_) => continue,
        };
        for fk in master_fks {
            let Some(encoded) = encode_form_id(&fk, interner, &target_masters) else {
                continue;
            };
            let alias_ids = session
                .record_decoded_in_handle(handle_id, &fk, target_schema, interner)
                .ok()
                .map(|record| collect_alias_ids_from_record(&record));
            quest_alias_ids_by_encoded_quest.insert(encoded, alias_ids);
        }
    }

    let valid_typed_param1_ids_by_function =
        collect_typed_parameter_1_encoded_ids(session, interner, config, &target_masters, report)?;
    let (valid_worldspace_ids, output_worldspace_id_by_object_id) =
        collect_worldspace_encoded_ids(session, interner, config, &target_masters, report)?;

    Ok(QuestConditionIndex {
        valid_quest_ids,
        quest_index_complete,
        quest_alias_ids_by_encoded_quest,
        valid_typed_param1_ids_by_function,
        valid_worldspace_ids,
        output_worldspace_id_by_object_id,
        target_masters,
    })
}

fn collect_worldspace_encoded_ids(
    session: &mut PluginSession,
    interner: &StringInterner,
    config: &FixupConfig,
    target_masters: &[String],
    report: &mut FixupReport,
) -> Result<(FxHashSet<u32>, FxHashMap<u32, u32>), FixupError> {
    let wrld_sig = SigCode::from_str("WRLD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut valid_ids = FxHashSet::default();
    let mut output_by_object_id = FxHashMap::default();

    for fk in session
        .form_keys_of_sig(wrld_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        if let Some(encoded) = encode_form_id(&fk, interner, target_masters) {
            valid_ids.insert(encoded);
            output_by_object_id.insert(fk.local, encoded);
        }
    }

    for &handle_id in &config.target_master_handle_ids {
        let fks = match session.form_keys_of_sig_in_handle(handle_id, wrld_sig, interner) {
            Ok(fks) => fks,
            Err(e) => {
                let warning = interner.intern(&format!(
                    "strip_invalid_quest_condition_params_worldspace:{e}"
                ));
                report.warnings.push(warning);
                continue;
            }
        };
        for fk in fks {
            if let Some(encoded) = encode_form_id(&fk, interner, target_masters) {
                valid_ids.insert(encoded);
            }
        }
    }

    Ok((valid_ids, output_by_object_id))
}

struct FinalPersistentReferenceIndex {
    valid_encoded_ids: FxHashSet<u32>,
    output_encoded_by_object_id: FxHashMap<u32, u32>,
}

/// Final post-copy repair for QUST alias conditions whose reference parameters
/// could not be classified before persistent CELL children existed.
pub fn repair_final_quest_reference_conditions(
    session: &mut PluginSession,
    interner: &StringInterner,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let target_schema = config
        .target_schema
        .as_deref()
        .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
    let mut report = FixupReport::empty();
    let index = collect_final_persistent_reference_index(session, interner, config, &mut report)?;
    let qust_sig = SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let quest_keys = session
        .form_keys_of_sig(qust_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut changed_records = Vec::new();

    for fk in quest_keys {
        let mut record = match session.record_decoded(&fk, target_schema, interner) {
            Ok(record) => record,
            Err(_) => continue,
        };
        if repair_final_quest_alias_conditions(&mut record, &index) {
            changed_records.push(record);
        }
    }

    let expected = changed_records.len();
    if expected == 0 {
        return Ok(report);
    }
    let replaced = session
        .replace_records_contents(changed_records, target_schema, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if replaced != expected {
        return Err(FixupError::HandleError(format!(
            "repair_final_quest_reference_conditions replaced {replaced} of {expected} expected records"
        )));
    }
    report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
    Ok(report)
}

fn collect_final_persistent_reference_index(
    session: &mut PluginSession,
    interner: &StringInterner,
    config: &FixupConfig,
    report: &mut FixupReport,
) -> Result<FinalPersistentReferenceIndex, FixupError> {
    let target_masters = session.target_masters().to_vec();
    let output_master_index = target_masters.len();
    let mut output_object_ids = FxHashSet::default();
    collect_output_persistent_refr_object_ids(
        &session.target_slot().parsed.root_items,
        false,
        &mut output_object_ids,
    );

    let mut valid_encoded_ids = FxHashSet::default();
    let mut output_encoded_by_object_id = FxHashMap::default();
    if output_master_index <= u8::MAX as usize {
        for object_id in output_object_ids {
            let encoded = ((output_master_index as u32) << 24) | object_id;
            valid_encoded_ids.insert(encoded);
            output_encoded_by_object_id.insert(object_id, encoded);
        }
    }

    let refr_sig = SigCode::from_str("REFR").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let target_schema = config
        .target_schema
        .as_deref()
        .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
    for &handle_id in &config.target_master_handle_ids {
        let form_keys = match session.form_keys_of_sig_in_handle(handle_id, refr_sig, interner) {
            Ok(form_keys) => form_keys,
            Err(e) => {
                report.warnings.push(interner.intern(&format!(
                    "repair_final_quest_reference_conditions_master:{e}"
                )));
                continue;
            }
        };
        for fk in form_keys {
            let persistent = session
                .record_decoded_in_handle(handle_id, &fk, target_schema, interner)
                .is_ok_and(|record| record.flags.contains(RecordFlags::PERSISTENT));
            if persistent {
                if let Some(encoded) = encode_form_id(&fk, interner, &target_masters) {
                    valid_encoded_ids.insert(encoded);
                }
            }
        }
    }

    Ok(FinalPersistentReferenceIndex {
        valid_encoded_ids,
        output_encoded_by_object_id,
    })
}

fn collect_output_persistent_refr_object_ids(
    items: &[ParsedItem],
    in_persistent_group: bool,
    output: &mut FxHashSet<u32>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                if in_persistent_group
                    && record.signature.as_str() == "REFR"
                    && record.flags & RecordFlags::PERSISTENT.bits() != 0
                {
                    let object_id = record.form_id & 0x00FF_FFFF;
                    if object_id != 0 {
                        output.insert(object_id);
                    }
                }
            }
            ParsedItem::Group(group) => {
                let child_is_persistent = if group.group_type == CELL_PERSISTENT_GROUP {
                    true
                } else if matches!(group.group_type, 9 | 10) {
                    false
                } else {
                    in_persistent_group
                };
                collect_output_persistent_refr_object_ids(
                    &group.children,
                    child_is_persistent,
                    output,
                );
            }
        }
    }
}

fn collect_typed_parameter_1_encoded_ids(
    session: &mut PluginSession,
    interner: &StringInterner,
    config: &FixupConfig,
    target_masters: &[String],
    report: &mut FixupReport,
) -> Result<FxHashMap<u16, FxHashSet<u32>>, FixupError> {
    let mut out = FxHashMap::default();
    for rule in TYPED_PARAMETER_1_RULES {
        let mut ids = FxHashSet::default();
        for sig_str in rule.allowed_sigs {
            let sig =
                SigCode::from_str(sig_str).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            for fk in session
                .form_keys_of_sig(sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?
            {
                if let Some(encoded) = encode_form_id(&fk, interner, target_masters) {
                    ids.insert(encoded);
                }
            }
            for &handle_id in &config.target_master_handle_ids {
                let fks = match session.form_keys_of_sig_in_handle(handle_id, sig, interner) {
                    Ok(fks) => fks,
                    Err(e) => {
                        let w = interner.intern(&format!(
                            "strip_invalid_quest_condition_params_typed_param:{sig_str}:{e}"
                        ));
                        report.warnings.push(w);
                        continue;
                    }
                };
                for fk in fks {
                    if let Some(encoded) = encode_form_id(&fk, interner, target_masters) {
                        ids.insert(encoded);
                    }
                }
            }
        }
        out.insert(rule.function_id, ids);
    }
    Ok(out)
}

/// Build the set of every valid QUST encoded FormID across the output plugin and
/// the target masters, in the same `(load_index << 24) | local` encoding the
/// CTDA Parameter #1 bytes use.
pub(crate) fn collect_valid_quest_encoded_ids(
    session: &mut PluginSession,
    interner: &StringInterner,
    config: &FixupConfig,
    report: &mut FixupReport,
) -> Result<(FxHashSet<u32>, bool), FixupError> {
    let qust_sig = SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let target_masters = session.target_masters().to_vec();
    let mut out = FxHashSet::default();
    let mut complete = config.target_master_handle_ids.len() == target_masters.len();

    // Output-plugin QUSTs (self load index = masters.len()).
    let output_fks = session
        .form_keys_of_sig(qust_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in output_fks {
        if let Some(encoded) = encode_form_id(&fk, interner, &target_masters) {
            out.insert(encoded);
        }
    }

    // Master QUSTs (load index = position in the master list).
    for &handle_id in &config.target_master_handle_ids {
        let fks = match session.form_keys_of_sig_in_handle(handle_id, qust_sig, interner) {
            Ok(fks) => fks,
            Err(e) => {
                complete = false;
                let w =
                    interner.intern(&format!("strip_invalid_quest_condition_params_master:{e}"));
                report.warnings.push(w);
                continue;
            }
        };
        for fk in fks {
            if let Some(encoded) = encode_form_id(&fk, interner, &target_masters) {
                out.insert(encoded);
            }
        }
    }

    Ok((out, complete))
}

/// Encode a FormKey to `(load_index << 24) | local`, where `load_index` is the
/// plugin's position in the target master list, or `masters.len()` (the output
/// plugin's own index) when not a listed master.
fn encode_form_id(fk: &FormKey, interner: &StringInterner, masters: &[String]) -> Option<u32> {
    if fk.local == 0 {
        return None;
    }
    let plugin_name = interner.resolve(fk.plugin)?;
    let load_index = masters
        .iter()
        .position(|m| m.eq_ignore_ascii_case(plugin_name))
        .unwrap_or(masters.len());
    if load_index > u8::MAX as usize || fk.local > 0x00FF_FFFF {
        return None;
    }
    Some(((load_index as u32) << 24) | fk.local)
}

fn collect_alias_ids_from_record(record: &Record) -> FxHashSet<u32> {
    record
        .fields
        .iter()
        .filter(|entry| QUST_ALIAS_ANCHOR_SIGS.contains(&entry.sig.0))
        .filter_map(|entry| field_value_u32(&entry.value))
        .collect()
}

fn field_value_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(v) => u32::try_from(*v).ok(),
        FieldValue::Int(v) if *v >= 0 => u32::try_from(*v).ok(),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[..4].try_into().ok()?))
        }
        _ => None,
    }
}

fn field_value_encoded_form_id(
    value: &FieldValue,
    interner: &StringInterner,
    masters: &[String],
) -> Option<u32> {
    match value {
        FieldValue::FormKey(fk) => encode_form_id(fk, interner, masters),
        _ => field_value_u32(value),
    }
}

#[derive(Clone, Copy)]
enum AliasContext<'a> {
    Known(&'a FxHashSet<u32>),
    Unknown,
    None,
}

impl<'a> AliasContext<'a> {
    fn known_ids(self) -> Option<&'a FxHashSet<u32>> {
        match self {
            AliasContext::Known(ids) => Some(ids),
            AliasContext::Unknown | AliasContext::None => None,
        }
    }

    fn can_validate(self) -> bool {
        !matches!(self, AliasContext::Unknown)
    }
}

fn alias_context_for_encoded_quest<'a>(
    encoded: u32,
    index: &'a QuestConditionIndex,
) -> AliasContext<'a> {
    match index.quest_alias_ids_by_encoded_quest.get(&encoded) {
        Some(Some(alias_ids)) => AliasContext::Known(alias_ids),
        Some(None) => AliasContext::Unknown,
        None if index.valid_quest_ids.contains(&encoded) => AliasContext::Unknown,
        None => AliasContext::None,
    }
}

fn quest_alias_context_for_record<'a>(
    record: &Record,
    index: &'a QuestConditionIndex,
    interner: &StringInterner,
) -> AliasContext<'a> {
    if record.sig.0 == *b"QUST" {
        let Some(encoded) = encode_form_id(&record.form_key, interner, &index.target_masters)
        else {
            return AliasContext::None;
        };
        return alias_context_for_encoded_quest(encoded, index);
    }

    if record.sig.0 == *b"INFO" {
        // INFO ownership is group topology, not a decoded owner subrecord.
        return AliasContext::Unknown;
    }

    let owner_sigs: &[[u8; 4]] = match &record.sig.0 {
        b"SCEN" => &[*b"PNAM"],
        b"PACK" | b"DIAL" => &[*b"QNAM"],
        _ => &[],
    };
    for entry in &record.fields {
        if !owner_sigs.contains(&entry.sig.0) {
            continue;
        }
        let Some(encoded) =
            field_value_encoded_form_id(&entry.value, interner, &index.target_masters)
        else {
            continue;
        };
        let context = alias_context_for_encoded_quest(encoded, index);
        if !matches!(context, AliasContext::None) {
            return context;
        }
    }
    AliasContext::None
}

fn raw_condition_function_id(bytes: &[u8]) -> Option<u16> {
    (bytes.len() >= 10).then(|| u16::from_le_bytes([bytes[8], bytes[9]]))
}

fn raw_condition_parameter_1(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= 16).then(|| u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]))
}

fn raw_condition_parameter_2(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= 20).then(|| u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]))
}

fn raw_condition_run_on(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= 24).then(|| u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]))
}

fn raw_condition_run_on_reference(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= 28).then(|| u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]))
}

/// True when a CTDA's function requires a non-null FLST/KYWD/LCTN in Parameter #2
/// (e.g. `GetInCurrentLocFormList`, 576) but Parameter #2 is NULL — an FO4-invalid
/// condition that cannot be repaired.
fn is_null_required_formlink_param(bytes: &[u8]) -> bool {
    let function_id = raw_condition_function_id(bytes).unwrap_or(0);
    let parameter_2 = raw_condition_parameter_2(bytes).unwrap_or(0);
    QUEST_PARAMETER_2_REQUIRED_FORMLINK_FUNCTION_IDS.contains(&function_id) && parameter_2 == 0
}

fn is_procedural_alias_param(bytes: &[u8]) -> bool {
    let function_id = raw_condition_function_id(bytes).unwrap_or(0);
    let parameter_1 = raw_condition_parameter_1(bytes).unwrap_or(0);
    QUEST_ALIAS_PARAMETER_1_FUNCTION_IDS.contains(&function_id)
        && (parameter_1 >> 16) == FO76_PROCEDURAL_FORM_ID_PREFIX
}

fn alias_id_missing_from(alias_id: u32, alias_ids: Option<&FxHashSet<u32>>) -> bool {
    match alias_ids {
        Some(ids) => !ids.contains(&alias_id),
        None => true,
    }
}

fn alias_id_is_invalid(alias_id: u32, alias_context: AliasContext<'_>) -> bool {
    match alias_context {
        AliasContext::Known(ids) => !ids.contains(&alias_id),
        AliasContext::Unknown | AliasContext::None => false,
    }
}

fn condition_has_invalid_alias_ref(bytes: &[u8], alias_context: AliasContext<'_>) -> bool {
    let function_id = raw_condition_function_id(bytes).unwrap_or(0);
    let parameter_1 = raw_condition_parameter_1(bytes).unwrap_or(0);
    if QUEST_ALIAS_PARAMETER_1_FUNCTION_IDS.contains(&function_id) {
        return is_procedural_alias_param(bytes) || alias_id_is_invalid(parameter_1, alias_context);
    }

    let run_on = raw_condition_run_on(bytes).unwrap_or(0);
    if run_on == CTDA_RUN_ON_QUEST_ALIAS {
        let alias_id = raw_condition_run_on_reference(bytes).unwrap_or(0);
        return alias_id_is_invalid(alias_id, alias_context);
    }
    false
}

fn condition_has_invalid_typed_param1(bytes: &[u8], index: &QuestConditionIndex) -> bool {
    let function_id = raw_condition_function_id(bytes).unwrap_or(0);
    let Some(_) = TYPED_PARAMETER_1_RULES
        .iter()
        .find(|rule| rule.function_id == function_id)
    else {
        return false;
    };
    let parameter_1 = raw_condition_parameter_1(bytes).unwrap_or(0);
    if parameter_1 == 0 {
        return true;
    }
    index
        .valid_typed_param1_ids_by_function
        .get(&function_id)
        .is_some_and(|ids| !ids.is_empty() && !ids.contains(&parameter_1))
}

fn repair_or_reject_worldspace_parameter(
    bytes: &mut [u8],
    index: &QuestConditionIndex,
) -> (bool, bool) {
    if raw_condition_function_id(bytes) != Some(GET_IN_WORLDSPACE_FUNCTION_ID) {
        return (false, false);
    }
    let Some(parameter_1) = raw_condition_parameter_1(bytes) else {
        return (true, false);
    };
    if index.valid_worldspace_ids.contains(&parameter_1) {
        return (false, false);
    }

    let object_id = parameter_1 & 0x00FF_FFFF;
    let Some(&output_id) = index.output_worldspace_id_by_object_id.get(&object_id) else {
        return (true, false);
    };
    bytes[12..16].copy_from_slice(&output_id.to_le_bytes());
    (false, true)
}

fn repair_final_quest_alias_conditions(
    record: &mut Record,
    index: &FinalPersistentReferenceIndex,
) -> bool {
    if record.sig.0 != *b"QUST" {
        return false;
    }

    let old_fields = std::mem::take(&mut record.fields);
    let mut retained = SmallVec::new();
    let mut in_alias = false;
    let mut dropping_condition_strings = false;
    let mut changed = false;
    let mut dropped = false;

    for mut entry in old_fields {
        if QUST_ALIAS_ANCHOR_SIGS.contains(&entry.sig.0) {
            in_alias = true;
            dropping_condition_strings = false;
        } else if entry.sig.0 == *b"ALED" {
            in_alias = false;
            dropping_condition_strings = false;
        }

        if in_alias && matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
            let drop = match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    let function_id = raw_condition_function_id(bytes).unwrap_or(0);
                    if !FINAL_PERSISTENT_REFERENCE_FUNCTION_IDS.contains(&function_id) {
                        false
                    } else {
                        let parameter_1 = raw_condition_parameter_1(bytes).unwrap_or(0);
                        if index.valid_encoded_ids.contains(&parameter_1) {
                            false
                        } else {
                            let object_id = parameter_1 & 0x00FF_FFFF;
                            if let Some(&output_id) =
                                index.output_encoded_by_object_id.get(&object_id)
                            {
                                bytes[12..16].copy_from_slice(&output_id.to_le_bytes());
                                changed = true;
                                false
                            } else {
                                true
                            }
                        }
                    }
                }
                _ => false,
            };
            dropping_condition_strings = drop;
            if drop {
                changed = true;
                dropped = true;
                continue;
            }
        } else if in_alias && matches!(&entry.sig.0, b"CIS1" | b"CIS2") {
            if dropping_condition_strings {
                changed = true;
                continue;
            }
        } else {
            dropping_condition_strings = false;
        }
        retained.push(entry);
    }

    record.fields = retained;
    if dropped {
        record.sync_condition_count();
    }
    changed
}

fn drop_conditions_matching(
    record: &mut Record,
    mut should_drop: impl FnMut(&mut [u8]) -> bool,
) -> bool {
    let before = record.fields.len();
    let mut dropping_condition_strings = false;
    record
        .fields
        .retain(|entry: &mut FieldEntry| match &entry.sig.0 {
            b"CTDA" | b"CTDT" => {
                let drop = match &mut entry.value {
                    FieldValue::Bytes(bytes) => should_drop(bytes.as_mut_slice()),
                    _ => false,
                };
                dropping_condition_strings = drop;
                !drop
            }
            b"CIS1" | b"CIS2" => !dropping_condition_strings,
            _ => {
                dropping_condition_strings = false;
                true
            }
        });
    let dropped = record.fields.len() < before;
    if dropped {
        // Keep the CITC condition count in lockstep with the surviving CTDA
        // rows; a stale overcount crashes FO4's condition evaluation.
        record.sync_condition_count();
    }
    dropped
}

/// Drop every CTDA/CTDT whose function requires a non-null Parameter #2 formlink
/// that is NULL, along with its trailing CIS1/CIS2. Returns `true` when at least
/// one was removed. Idempotent on records without such a condition.
pub(crate) fn drop_null_required_formlink_conditions(record: &mut Record) -> bool {
    drop_conditions_matching(record, |bytes| is_null_required_formlink_param(bytes))
}

/// Legacy focused helper for condition cases that cannot be repaired by an
/// owning quest context: procedural alias ids and NULL required Parameter #2
/// formlinks.
pub(crate) fn drop_invalid_context_quest_conditions(record: &mut Record) -> bool {
    drop_conditions_matching(record, |bytes| {
        is_procedural_alias_param(bytes) || is_null_required_formlink_param(bytes)
    })
}

pub(crate) fn scrub_invalid_quest_references(
    record: &mut Record,
    index: &QuestConditionIndex,
    interner: &StringInterner,
) -> bool {
    let alias_context = quest_alias_context_for_record(record, index, interner);
    let can_validate_quests = index.has_usable_quest_index();
    let mut repaired_worldspace = false;
    let mut changed = drop_conditions_matching(record, |bytes| {
        let (invalid_worldspace, repaired) = repair_or_reject_worldspace_parameter(bytes, index);
        repaired_worldspace |= repaired;
        let function_id = raw_condition_function_id(bytes).unwrap_or(0);
        let parameter_1 = raw_condition_parameter_1(bytes).unwrap_or(0);
        let invalid_quest_param = can_validate_quests
            && QUEST_PARAMETER_1_FUNCTION_IDS.contains(&function_id)
            && parameter_1 != 0
            && !index.valid_quest_ids.contains(&parameter_1);
        invalid_worldspace
            || invalid_quest_param
            || (can_validate_quests && condition_has_invalid_alias_ref(bytes, alias_context))
            || condition_has_invalid_typed_param1(bytes, index)
            || is_null_required_formlink_param(bytes)
    });
    changed |= repaired_worldspace;
    if can_validate_quests && alias_context.can_validate() {
        changed |=
            drop_invalid_alea_external_aliases(record, index, alias_context.known_ids(), interner);
    }
    if can_validate_quests && alias_context.can_validate() {
        changed |= scrub_invalid_vmad_aliases(record, alias_context.known_ids());
    }
    changed
}

/// Drop every CTDA/CTDT that is either (a) a function taking a QUST in
/// Parameter #1 whose Parameter #1 is non-zero but not a known QUST, or (b) a
/// quest-alias function (GetIsAliasRef) whose Parameter #1 is a FO76 procedural
/// `0x07A0xxxx` id that can never resolve to a valid alias — along with its
/// trailing CIS1/CIS2. Returns `true` when at least one CTDA was removed.
pub(crate) fn drop_invalid_quest_conditions(
    record: &mut Record,
    valid_quest_ids: &FxHashSet<u32>,
) -> bool {
    drop_conditions_matching(record, |bytes| {
        let function_id = raw_condition_function_id(bytes).unwrap_or(0);
        let parameter_1 = raw_condition_parameter_1(bytes).unwrap_or(0);
        let invalid_quest_param = QUEST_PARAMETER_1_FUNCTION_IDS.contains(&function_id)
            && parameter_1 != 0
            && !valid_quest_ids.contains(&parameter_1);
        invalid_quest_param
            || is_procedural_alias_param(bytes)
            || is_null_required_formlink_param(bytes)
    })
}

fn drop_invalid_alea_external_aliases(
    record: &mut Record,
    index: &QuestConditionIndex,
    current_alias_ids: Option<&FxHashSet<u32>>,
    interner: &StringInterner,
) -> bool {
    if record.sig.0 != *b"QUST" {
        return false;
    }

    let old_fields = std::mem::take(&mut record.fields);
    let mut retained: SmallVec<[FieldEntry; 8]> = SmallVec::new();
    let mut iter = old_fields.into_iter().peekable();
    let mut changed = false;

    while let Some(entry) = iter.next() {
        if entry.sig.0 == *b"ALEQ" {
            if iter.peek().is_some_and(|next| next.sig.0 == *b"ALEA") {
                let alea_entry = iter.next().expect("peeked ALEA");
                let external_quest =
                    field_value_encoded_form_id(&entry.value, interner, &index.target_masters);
                let external_context = external_quest
                    .map(|quest| alias_context_for_encoded_quest(quest, index))
                    .unwrap_or(AliasContext::None);
                if matches!(external_context, AliasContext::Unknown) {
                    retained.push(entry);
                    retained.push(alea_entry);
                    continue;
                }
                let alias_ids = external_context.known_ids().or(current_alias_ids);
                let alias_id = field_value_u32(&alea_entry.value);
                let invalid_alias = match alias_id {
                    Some(alias) => alias_id_missing_from(alias, alias_ids),
                    None => true,
                };
                if invalid_alias {
                    changed = true;
                    continue;
                }
                retained.push(entry);
                retained.push(alea_entry);
                continue;
            }
        } else if entry.sig.0 == *b"ALEA" {
            let alias_id = field_value_u32(&entry.value);
            let invalid_alias = match alias_id {
                Some(alias) => alias_id_missing_from(alias, current_alias_ids),
                None => true,
            };
            if invalid_alias {
                changed = true;
                continue;
            }
        }
        retained.push(entry);
    }

    record.fields = retained;
    changed
}

fn scrub_invalid_vmad_aliases(record: &mut Record, alias_ids: Option<&FxHashSet<u32>>) -> bool {
    let mut changed = false;
    let record_sig = record.sig.0;
    for entry in &mut record.fields {
        if entry.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if scrub_invalid_vmad_aliases_in_blob(bytes.as_mut_slice(), &record_sig, alias_ids) {
            changed = true;
        }
    }
    changed
}

fn scrub_invalid_vmad_aliases_in_blob(
    data: &mut [u8],
    record_sig: &[u8; 4],
    alias_ids: Option<&FxHashSet<u32>>,
) -> bool {
    let Some(version) = vmad_read_u16(data, 0) else {
        return false;
    };
    let Some(object_format) = vmad_read_u16(data, 2) else {
        return false;
    };
    let Some(script_count) = vmad_read_u16(data, 4) else {
        return false;
    };
    if version == 0 || !matches!(object_format, 1 | 2) {
        return false;
    }

    let mut offset = 6usize;
    let mut changed = false;
    for _ in 0..script_count {
        if scrub_vmad_script_entry(data, &mut offset, object_format, alias_ids, &mut changed)
            .is_none()
        {
            return changed;
        }
    }

    if offset >= data.len() {
        return changed;
    }
    match record_sig {
        b"INFO" | b"PACK" => {
            scrub_vmad_info_pack_fragments(
                data,
                &mut offset,
                object_format,
                alias_ids,
                &mut changed,
            );
        }
        b"SCEN" => {
            scrub_vmad_scen_fragments(data, &mut offset, object_format, alias_ids, &mut changed);
        }
        b"QUST" => {
            scrub_vmad_qust_after_scripts(
                data,
                &mut offset,
                object_format,
                alias_ids,
                &mut changed,
            );
        }
        _ => {}
    }
    changed
}

fn scrub_vmad_info_pack_fragments(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<u8> {
    vmad_advance(offset, 1, data.len())?; // i8 version
    let flags = vmad_read_u8_advance(data, offset)?;
    scrub_vmad_script_entry(data, offset, object_format, alias_ids, changed)?;
    for _ in 0..(flags as u32).count_ones() {
        vmad_advance(offset, 1, data.len())?; // i8 unknown
        vmad_skip_string(data, offset)?;
        vmad_skip_string(data, offset)?;
    }
    Some(flags)
}

fn scrub_vmad_scen_fragments(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    scrub_vmad_info_pack_fragments(data, offset, object_format, alias_ids, changed)?;
    let phase_count = vmad_read_u16_advance(data, offset)? as usize;
    for _ in 0..phase_count {
        vmad_advance(offset, 6, data.len())?;
        vmad_skip_string(data, offset)?;
        vmad_skip_string(data, offset)?;
    }
    Some(())
}

fn scrub_vmad_qust_after_scripts(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    vmad_advance(offset, 1, data.len())?; // i8 version
    let fragment_count = vmad_read_u16_advance(data, offset)? as usize;

    let script_name_len = vmad_read_u16_advance(data, offset)? as usize;
    if script_name_len > 0 {
        vmad_advance(offset, script_name_len, data.len())?;
        vmad_advance(offset, 1, data.len())?; // flags
        let prop_count = vmad_read_u16_advance(data, offset)? as usize;
        for _ in 0..prop_count {
            vmad_skip_string(data, offset)?;
            let prop_type = vmad_read_u8_advance(data, offset)?;
            vmad_advance(offset, 1, data.len())?;
            scrub_vmad_property_value(data, offset, prop_type, object_format, alias_ids, changed)?;
        }
    }

    for _ in 0..fragment_count {
        vmad_advance(offset, 9, data.len())?;
        vmad_skip_string(data, offset)?;
        vmad_skip_string(data, offset)?;
    }

    let alias_count = vmad_read_u16_advance(data, offset)? as usize;
    for _ in 0..alias_count {
        scrub_vmad_object_alias(data, offset, object_format, alias_ids, changed)?;
        vmad_advance(offset, 2, data.len())?;
        let alias_obj_format = vmad_read_u16_advance(data, offset)?;
        let alias_script_count = vmad_read_u16_advance(data, offset)? as usize;
        for _ in 0..alias_script_count {
            scrub_vmad_script_entry(data, offset, alias_obj_format, alias_ids, changed)?;
        }
    }
    Some(())
}

fn scrub_vmad_script_entry(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    vmad_skip_string(data, offset)?;
    vmad_advance(offset, 1, data.len())?;
    let property_count = vmad_read_u16_advance(data, offset)? as usize;
    for _ in 0..property_count {
        vmad_skip_string(data, offset)?;
        let property_type = vmad_read_u8_advance(data, offset)?;
        vmad_advance(offset, 1, data.len())?;
        scrub_vmad_property_value(
            data,
            offset,
            property_type,
            object_format,
            alias_ids,
            changed,
        )?;
    }
    Some(())
}

fn scrub_vmad_property_value(
    data: &mut [u8],
    offset: &mut usize,
    property_type: u8,
    object_format: u16,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => scrub_vmad_object_alias(data, offset, object_format, alias_ids, changed),
        2 => {
            vmad_skip_string(data, offset)?;
            Some(())
        }
        3 | 4 => vmad_advance(offset, 4, data.len()),
        5 => vmad_advance(offset, 1, data.len()),
        7 => scrub_vmad_struct(data, offset, object_format, alias_ids, changed),
        11 => {
            let count = vmad_read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                scrub_vmad_object_alias(data, offset, object_format, alias_ids, changed)?;
            }
            Some(())
        }
        12 => {
            let count = vmad_read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                vmad_skip_string(data, offset)?;
            }
            Some(())
        }
        13 | 14 => {
            let count = vmad_read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            vmad_advance(offset, (count as usize).checked_mul(4)?, data.len())
        }
        15 => {
            let count = vmad_read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            vmad_advance(offset, count as usize, data.len())
        }
        16 => vmad_advance(offset, 4, data.len()),
        17 => {
            let count = vmad_read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                scrub_vmad_struct(data, offset, object_format, alias_ids, changed)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn scrub_vmad_struct(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    let count = vmad_read_i32_advance(data, offset)?;
    if count < 0 {
        return None;
    }
    for _ in 0..count {
        vmad_skip_string(data, offset)?;
        let member_type = vmad_read_u8_advance(data, offset)?;
        vmad_advance(offset, 1, data.len())?;
        scrub_vmad_property_value(data, offset, member_type, object_format, alias_ids, changed)?;
    }
    Some(())
}

fn scrub_vmad_object_alias(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    let alias_offset = if object_format == 2 {
        (*offset).checked_add(2)?
    } else {
        (*offset).checked_add(4)?
    };
    vmad_advance(offset, 8, data.len())?;
    let alias = vmad_read_u16(data, alias_offset)?;
    if alias == VMAD_NO_ALIAS {
        return Some(());
    }
    if alias_id_missing_from(u32::from(alias), alias_ids) {
        data.get_mut(alias_offset..alias_offset.checked_add(2)?)?
            .copy_from_slice(&VMAD_NO_ALIAS.to_le_bytes());
        *changed = true;
    }
    Some(())
}

fn vmad_read_u8(data: &[u8], offset: usize) -> Option<u8> {
    data.get(offset).copied()
}

fn vmad_read_u8_advance(data: &[u8], offset: &mut usize) -> Option<u8> {
    let value = vmad_read_u8(data, *offset)?;
    *offset = (*offset).checked_add(1)?;
    Some(value)
}

fn vmad_read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn vmad_read_u16_advance(data: &[u8], offset: &mut usize) -> Option<u16> {
    let value = vmad_read_u16(data, *offset)?;
    *offset = (*offset).checked_add(2)?;
    Some(value)
}

fn vmad_read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn vmad_read_i32_advance(data: &[u8], offset: &mut usize) -> Option<i32> {
    let value = vmad_read_u32(data, *offset)? as i32;
    *offset = (*offset).checked_add(4)?;
    Some(value)
}

fn vmad_advance(offset: &mut usize, by: usize, len: usize) -> Option<()> {
    let next = offset.checked_add(by)?;
    if next > len {
        return None;
    }
    *offset = next;
    Some(())
}

fn vmad_skip_string(data: &[u8], offset: &mut usize) -> Option<()> {
    let len = vmad_read_u16_advance(data, offset)? as usize;
    vmad_advance(offset, len, data.len())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{ParsedGroup, ParsedRecord};
    use smallvec::SmallVec;

    fn ctda(function_id: u16, parameter_1: u32) -> FieldEntry {
        let mut bytes = vec![0u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn ctda_p2(function_id: u16, parameter_1: u32, parameter_2: u32) -> FieldEntry {
        let mut bytes = vec![0u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[16..20].copy_from_slice(&parameter_2.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn field(sig: &str, bytes: &[u8]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
        }
    }

    fn record(sig: &str, fields: Vec<FieldEntry>) -> Record {
        let interner = StringInterner::new();
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local: 0x000800,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    fn record_with_interner(
        sig: &str,
        local: u32,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    fn quest_index(valid: &[u32], aliases: &[(u32, &[u32])]) -> QuestConditionIndex {
        QuestConditionIndex {
            valid_quest_ids: valid.iter().copied().collect(),
            quest_index_complete: true,
            quest_alias_ids_by_encoded_quest: aliases
                .iter()
                .map(|(quest, ids)| (*quest, Some(ids.iter().copied().collect())))
                .collect(),
            valid_typed_param1_ids_by_function: FxHashMap::default(),
            valid_worldspace_ids: FxHashSet::default(),
            output_worldspace_id_by_object_id: FxHashMap::default(),
            target_masters: Vec::new(),
        }
    }

    fn quest_index_with_typed(
        valid: &[u32],
        aliases: &[(u32, &[u32])],
        typed: &[(u16, &[u32])],
    ) -> QuestConditionIndex {
        QuestConditionIndex {
            valid_quest_ids: valid.iter().copied().collect(),
            quest_index_complete: true,
            quest_alias_ids_by_encoded_quest: aliases
                .iter()
                .map(|(quest, ids)| (*quest, Some(ids.iter().copied().collect())))
                .collect(),
            valid_typed_param1_ids_by_function: typed
                .iter()
                .map(|(function_id, ids)| (*function_id, ids.iter().copied().collect()))
                .collect(),
            valid_worldspace_ids: FxHashSet::default(),
            output_worldspace_id_by_object_id: FxHashMap::default(),
            target_masters: Vec::new(),
        }
    }

    fn quest_index_with_worldspaces(
        valid_worldspaces: &[u32],
        output_worldspaces: &[(u32, u32)],
    ) -> QuestConditionIndex {
        QuestConditionIndex {
            valid_quest_ids: [0x0700_0800].into_iter().collect(),
            quest_index_complete: true,
            quest_alias_ids_by_encoded_quest: FxHashMap::default(),
            valid_typed_param1_ids_by_function: FxHashMap::default(),
            valid_worldspace_ids: valid_worldspaces.iter().copied().collect(),
            output_worldspace_id_by_object_id: output_worldspaces.iter().copied().collect(),
            target_masters: vec![
                "Fallout4.esm".into(),
                "DLCRobot.esm".into(),
                "DLCworkshop01.esm".into(),
                "DLCCoast.esm".into(),
                "DLCworkshop02.esm".into(),
                "DLCworkshop03.esm".into(),
                "DLCNukaWorld.esm".into(),
            ],
        }
    }

    fn final_reference_index(
        valid: &[u32],
        output: &[(u32, u32)],
    ) -> FinalPersistentReferenceIndex {
        FinalPersistentReferenceIndex {
            valid_encoded_ids: valid.iter().copied().collect(),
            output_encoded_by_object_id: output.iter().copied().collect(),
        }
    }

    fn parsed_record(sig: &str, form_id: u32, flags: u32) -> ParsedRecord {
        ParsedRecord {
            signature: sig.into(),
            form_id,
            flags,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: Vec::new(),
            raw_payload: None,
            parse_error: None,
        }
    }

    fn parsed_group(group_type: i32, children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label: [0; 4],
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    fn condition_param1(record: &Record) -> u32 {
        let field = record
            .fields
            .iter()
            .find(|field| matches!(&field.sig.0, b"CTDA" | b"CTDT"))
            .expect("condition");
        let FieldValue::Bytes(bytes) = &field.value else {
            panic!("condition bytes");
        };
        raw_condition_parameter_1(bytes).expect("parameter 1")
    }

    fn push_vmad_string(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u16).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    fn vmad_object_property(alias: i16, form_id: u32) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes()); // version
        out.extend_from_slice(&2u16.to_le_bytes()); // object format
        out.extend_from_slice(&1u16.to_le_bytes()); // script count
        push_vmad_string(&mut out, "Script");
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        push_vmad_string(&mut out, "AliasProp");
        out.push(1); // Object
        out.push(0);
        out.extend_from_slice(&0u16.to_le_bytes());
        let alias_offset = out.len();
        out.extend_from_slice(&alias.to_le_bytes());
        out.extend_from_slice(&form_id.to_le_bytes());
        (out, alias_offset)
    }

    fn info_fragment_vmad_object_property(alias: i16, form_id: u32) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes()); // version
        out.extend_from_slice(&2u16.to_le_bytes()); // object format
        out.extend_from_slice(&0u16.to_le_bytes()); // no top-level scripts
        out.push(1); // fragment block version
        out.push(1); // flags -> one fragment row
        push_vmad_string(&mut out, "FragmentScript");
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        push_vmad_string(&mut out, "AliasProp");
        out.push(1); // Object
        out.push(0);
        out.extend_from_slice(&0u16.to_le_bytes());
        let alias_offset = out.len();
        out.extend_from_slice(&alias.to_le_bytes());
        out.extend_from_slice(&form_id.to_le_bytes());
        out.push(0); // simple fragment unknown
        push_vmad_string(&mut out, "FragmentScript");
        push_vmad_string(&mut out, "Fragment_0");
        (out, alias_offset)
    }

    fn alias_at(bytes: &[u8], offset: usize) -> i16 {
        i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
    }

    fn form_id_at(bytes: &[u8], alias_offset: usize) -> u32 {
        u32::from_le_bytes(
            bytes[alias_offset + 2..alias_offset + 6]
                .try_into()
                .expect("object FormID"),
        )
    }

    fn sigs(record: &Record) -> Vec<&str> {
        record.fields.iter().map(|f| f.sig.as_str()).collect()
    }

    #[test]
    fn drops_quest_param_ctda_pointing_at_non_quest_and_its_cis() {
        let mut valid = FxHashSet::default();
        valid.insert(0x0700_1234); // a real converted QUST
        // GetStageDone(59) with Param1=500 (a FO76 stage number) → not a QUST → drop.
        let mut rec = record(
            "ACTI",
            vec![
                field("EDID", b"X\0"),
                ctda(59, 500),
                field("CIS2", b"alias\0"),
            ],
        );
        assert!(drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(sigs(&rec), vec!["EDID"], "CTDA and its CIS2 both dropped");
    }

    #[test]
    fn dropping_ctda_reconciles_citc_count() {
        let mut valid = FxHashSet::default();
        valid.insert(0x0700_1234);
        // A CITC-bearing record (e.g. MUST) with two conditions, one of which
        // points at a non-quest and is dropped. CITC must follow 2 -> 1, else
        // FO4 evaluates a phantom condition and crashes.
        let mut rec = record(
            "MUST",
            vec![
                field("CITC", &2u32.to_le_bytes()),
                ctda(58, 0x0700_1234), // valid quest → kept
                ctda(58, 0x0040_0500), // non-quest → dropped
            ],
        );
        assert!(drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(sigs(&rec), vec!["CITC", "CTDA"], "one CTDA survives");
        let citc = rec
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CITC")
            .unwrap();
        assert_eq!(
            citc.value,
            FieldValue::Bytes(SmallVec::from_slice(&1u32.to_le_bytes())),
            "CITC reconciled to surviving CTDA count"
        );
    }

    #[test]
    fn keeps_quest_param_ctda_pointing_at_real_quest() {
        let mut valid = FxHashSet::default();
        valid.insert(0x0700_1234);
        let mut rec = record("ACTI", vec![ctda(58, 0x0700_1234), field("CIS1", b"a\0")]);
        assert!(!drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(
            sigs(&rec),
            vec!["CTDA", "CIS1"],
            "valid quest ref preserved"
        );
    }

    #[test]
    fn keeps_zero_param_ctda_handled_by_pair_hook() {
        let valid = FxHashSet::default();
        // Param1==0 is the pair-hook's job; this fixup must not touch it (and an
        // empty valid set must not cause a null param to be dropped here).
        let mut rec = record("ACTI", vec![ctda(58, 0)]);
        assert!(!drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(sigs(&rec), vec!["CTDA"]);
    }

    #[test]
    fn keeps_non_quest_function_with_bogus_param() {
        let valid = FxHashSet::default();
        // Function 560 does not take a QUST in Param1 → leave its param alone.
        let mut rec = record("TERM", vec![ctda(560, 500)]);
        assert!(!drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(sigs(&rec), vec!["CTDA"]);
    }

    #[test]
    fn drops_only_the_offending_ctda_in_a_run() {
        let mut valid = FxHashSet::default();
        valid.insert(0x0700_1234);
        let mut rec = record(
            "TERM",
            vec![
                ctda(58, 0x0700_1234), // valid → keep
                field("CIS1", b"keep\0"),
                ctda(59, 9000), // bogus → drop with its CIS2
                field("CIS2", b"drop\0"),
                field("FULL", b"name\0"), // unrelated, keep
            ],
        );
        assert!(drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(sigs(&rec), vec!["CTDA", "CIS1", "FULL"]);
    }

    #[test]
    fn drops_quest_param_ctda_with_non_quest_param_on_info() {
        let mut valid = FxHashSet::default();
        valid.insert(0x0700_1234);
        // GetStageDone(59) with Param1=0x32 — the FO76 INFO case that resolves to
        // a vanilla STAT (COCMarkerHeading).
        let mut rec = record(
            "INFO",
            vec![ctda(59, 0x0000_0032), field("CIS2", b"alias\0")],
        );
        assert!(drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(
            sigs(&rec),
            Vec::<&str>::new(),
            "CTDA and CIS2 dropped on INFO"
        );
    }

    #[test]
    fn scrub_drops_wrong_type_quest_param_on_info_without_alias_context() {
        let interner = StringInterner::new();
        let index = quest_index(&[0x0700_1234], &[(0x0700_1234, &[2])]);
        let mut rec = record_with_interner(
            "INFO",
            0x900,
            vec![ctda(59, 0x0000_0032), field("CIS2", b"alias\0")],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), Vec::<&str>::new());
    }

    #[test]
    fn scrub_drops_wrong_type_quest_param_on_scen_context() {
        let interner = StringInterner::new();
        let index = quest_index(&[0x0000_1234], &[]);
        // SCEN phase/start conditions from xEdit: Function GetStage(58) with
        // Parameter #1 resolving to a STAT/WEAP/etc. is still invalid even
        // though SCEN has an owning quest context.
        let mut rec = record_with_interner(
            "SCEN",
            0x900,
            vec![ctda(58, 0x0000_0064), field("CIS2", b"stage\0")],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), Vec::<&str>::new());
    }

    #[test]
    fn scrub_drops_qust_stage_alias_condition_missing_from_alias_table() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let index = quest_index(&[self_quest], &[(self_quest, &[2])]);
        // QUST \ Stages ... CTDA Parameter #1 -> Quest Alias [1] not found.
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![
                field("ALST", &2u32.to_le_bytes()),
                ctda(566, 1),
                field("CIS1", b"alias\0"),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ALST"]);
    }

    #[test]
    fn scrub_keeps_qust_alias_condition_present_in_alias_table() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let index = quest_index(&[self_quest], &[(self_quest, &[2])]);
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![
                field("ALST", &2u32.to_le_bytes()),
                ctda(566, 2),
                field("CIS1", b"alias\0"),
            ],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ALST", "CTDA", "CIS1"]);
    }

    #[test]
    fn scrub_keeps_info_alias_condition_without_owner_context() {
        let interner = StringInterner::new();
        let index = quest_index(&[0x0000_0800], &[(0x0000_0800, &[2])]);
        let mut rec = record_with_interner(
            "INFO",
            0x900,
            vec![ctda(566, 11), field("CIS1", b"alias\0")],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["CTDA", "CIS1"]);
    }

    #[test]
    fn scrub_drops_info_procedural_alias_without_owner_context() {
        let interner = StringInterner::new();
        let index = quest_index(&[0x0000_0800], &[(0x0000_0800, &[2])]);
        let mut rec = record_with_interner(
            "INFO",
            0x900,
            vec![ctda(566, 0x07A0_000A), field("CIS1", b"alias\0")],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), Vec::<&str>::new());
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
    }

    #[test]
    fn scrub_drops_scen_procedural_alias_with_known_owner() {
        let interner = StringInterner::new();
        let owner_quest = 0x0000_0800;
        let index = quest_index(&[owner_quest], &[(owner_quest, &[2])]);
        let mut rec = record_with_interner(
            "SCEN",
            0x900,
            vec![
                field("PNAM", &owner_quest.to_le_bytes()),
                ctda(566, 0x07A0_002E),
                field("CIS2", b"alias\0"),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["PNAM"]);
    }

    #[test]
    fn scrub_drops_qust_procedural_alias_with_known_owner() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let index = quest_index(&[self_quest], &[(self_quest, &[2])]);
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![
                field("ALST", &2u32.to_le_bytes()),
                ctda(566, 0x07A0_0031),
                field("CIS1", b"alias\0"),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ALST"]);
    }

    #[test]
    fn scrub_keeps_low_alias_without_known_owner() {
        let interner = StringInterner::new();
        let index = quest_index(&[0x0000_0800], &[(0x0000_0800, &[2])]);
        let mut rec = record_with_interner(
            "SCEN",
            0x900,
            vec![ctda(566, 6), field("CIS1", b"alias\0")],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["CTDA", "CIS1"]);
    }

    #[test]
    fn scrub_drops_null_required_typed_param1_condition() {
        let interner = StringInterner::new();
        let index = quest_index_with_typed(&[0x0000_0800], &[(0x0000_0800, &[2])], &[]);
        let mut rec = record_with_interner(
            "INFO",
            0x900,
            vec![ctda(74, 0), field("CIS1", b"global\0")],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), Vec::<&str>::new());
    }

    #[test]
    fn scrub_drops_wrong_type_typed_param1_condition_when_target_set_known() {
        let interner = StringInterner::new();
        let index = quest_index_with_typed(
            &[0x0000_0800],
            &[(0x0000_0800, &[2])],
            &[(163, &[0x0000_0123])],
        );
        let mut rec = record_with_interner(
            "SCEN",
            0x900,
            vec![ctda(163, 0x0000_0456), field("CIS1", b"target\0")],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), Vec::<&str>::new());
    }

    #[test]
    fn scrub_keeps_typed_param1_condition_when_target_exists() {
        let interner = StringInterner::new();
        let index = quest_index_with_typed(
            &[0x0000_0800],
            &[(0x0000_0800, &[2])],
            &[(163, &[0x0000_0456])],
        );
        let mut rec = record_with_interner(
            "SCEN",
            0x900,
            vec![ctda(163, 0x0000_0456), field("CIS1", b"target\0")],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["CTDA", "CIS1"]);
    }

    #[test]
    fn scrub_repairs_get_in_worldspace_source_load_byte_to_output() {
        let interner = StringInterner::new();
        let index = quest_index_with_worldspaces(&[0x0725_DA15], &[(0x0025_DA15, 0x0725_DA15)]);
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![ctda(310, 0x0025_DA15), field("CIS1", b"world\0")],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(condition_param1(&rec), 0x0725_DA15);
        assert_eq!(sigs(&rec), vec!["CTDA", "CIS1"]);
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
    }

    #[test]
    fn scrub_preserves_valid_master_and_output_worldspaces() {
        let interner = StringInterner::new();
        let index = quest_index_with_worldspaces(
            &[0x0000_003C, 0x0725_DA15],
            &[(0x0025_DA15, 0x0725_DA15)],
        );
        for parameter in [0x0000_003C, 0x0725_DA15] {
            let mut rec =
                record_with_interner("QUST", 0x800, vec![ctda(310, parameter)], &interner);
            assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
            assert_eq!(condition_param1(&rec), parameter);
        }
    }

    #[test]
    fn scrub_drops_get_in_worldspace_with_wrong_type() {
        let interner = StringInterner::new();
        let index = quest_index_with_worldspaces(&[0x0725_DA15], &[(0x0025_DA15, 0x0725_DA15)]);
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![ctda(310, 0x0700_1234), field("CIS2", b"wrong-type\0")],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), Vec::<&str>::new());
    }

    #[test]
    fn scrub_drops_get_in_worldspace_with_missing_target_and_syncs_citc() {
        let interner = StringInterner::new();
        let index = quest_index_with_worldspaces(&[0x0725_DA15], &[(0x0025_DA15, 0x0725_DA15)]);
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![
                field("CITC", &2u32.to_le_bytes()),
                ctda(310, 0x0000_BEEF),
                field("CIS1", b"missing\0"),
                ctda(310, 0x0725_DA15),
            ],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["CITC", "CTDA"]);
        let citc = rec
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CITC")
            .and_then(|field| field_value_u32(&field.value));
        assert_eq!(citc, Some(1));
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
    }

    #[test]
    fn worldspace_index_does_not_authorize_incomplete_quest_validation() {
        let interner = StringInterner::new();
        let mut index = quest_index(&[], &[]);
        index.quest_index_complete = false;
        index.valid_worldspace_ids.insert(0x0725_DA15);
        index
            .output_worldspace_id_by_object_id
            .insert(0x0025_DA15, 0x0725_DA15);
        let mut rec = record_with_interner(
            "SCEN",
            0x900,
            vec![ctda(58, 0x0000_DEAD), ctda(310, 0x0025_DA15)],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["CTDA", "CTDA"]);
        let params: Vec<u32> = rec
            .fields
            .iter()
            .filter_map(|field| match &field.value {
                FieldValue::Bytes(bytes) if field.sig.0 == *b"CTDA" => {
                    raw_condition_parameter_1(bytes)
                }
                _ => None,
            })
            .collect();
        assert_eq!(params, vec![0x0000_DEAD, 0x0725_DA15]);
    }

    #[test]
    fn final_alias_pass_repairs_exact_get_distance_and_within_distance_collisions() {
        let index = final_reference_index(
            &[0x070B_1051, 0x0706_16AC, 0x0705_C8D2],
            &[
                (0x000B_1051, 0x070B_1051),
                (0x0006_16AC, 0x0706_16AC),
                (0x0005_C8D2, 0x0705_C8D2),
            ],
        );
        let mut rec = record(
            "QUST",
            vec![
                field("ALST", &19u32.to_le_bytes()),
                ctda(1, 0x000B_1051),
                field("CIS1", b"vault\0"),
                ctda(1, 0x0006_16AC),
                ctda(639, 0x0005_C8D2),
                field("ALED", &[]),
            ],
        );

        assert!(repair_final_quest_alias_conditions(&mut rec, &index));
        let params: Vec<u32> = rec
            .fields
            .iter()
            .filter_map(|field| match &field.value {
                FieldValue::Bytes(bytes) if field.sig.0 == *b"CTDA" => {
                    raw_condition_parameter_1(bytes)
                }
                _ => None,
            })
            .collect();
        assert_eq!(params, vec![0x070B_1051, 0x0706_16AC, 0x0705_C8D2]);
        assert_eq!(
            sigs(&rec),
            vec!["ALST", "CTDA", "CIS1", "CTDA", "CTDA", "ALED"]
        );
        assert!(!repair_final_quest_alias_conditions(&mut rec, &index));
    }

    #[test]
    fn final_alias_pass_preserves_valid_master_and_output_references() {
        let index =
            final_reference_index(&[0x0000_1234, 0x0700_5678], &[(0x0000_5678, 0x0700_5678)]);
        let mut rec = record(
            "QUST",
            vec![
                field("ALST", &1u32.to_le_bytes()),
                ctda(1, 0x0000_1234),
                ctda(639, 0x0700_5678),
                field("ALED", &[]),
            ],
        );

        assert!(!repair_final_quest_alias_conditions(&mut rec, &index));
    }

    #[test]
    fn final_alias_pass_drops_wrong_type_and_nonpersistent_targets_with_cis() {
        let index = final_reference_index(&[], &[]);
        let mut rec = record(
            "QUST",
            vec![
                field("CITC", &2u32.to_le_bytes()),
                field("ALST", &1u32.to_le_bytes()),
                ctda(1, 0x0000_BEEF),
                field("CIS1", b"wrong-type\0"),
                ctda(639, 0x0700_CAFE),
                field("CIS2", b"nonpersistent\0"),
                field("ALED", &[]),
            ],
        );

        assert!(repair_final_quest_alias_conditions(&mut rec, &index));
        assert_eq!(sigs(&rec), vec!["CITC", "ALST", "ALED"]);
        let citc = rec
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"CITC")
            .and_then(|field| field_value_u32(&field.value));
        assert_eq!(citc, Some(0));
        assert!(!repair_final_quest_alias_conditions(&mut rec, &index));
    }

    #[test]
    fn final_alias_pass_does_not_touch_quest_level_conditions() {
        let index = final_reference_index(&[], &[]);
        let mut rec = record("QUST", vec![ctda(1, 0x0000_BEEF)]);
        assert!(!repair_final_quest_alias_conditions(&mut rec, &index));
        assert_eq!(sigs(&rec), vec!["CTDA"]);
    }

    #[test]
    fn output_reference_requires_persistent_group_and_flag() {
        let persistent = RecordFlags::PERSISTENT.bits();
        let items = vec![
            parsed_group(
                8,
                vec![
                    ParsedItem::Record(parsed_record("REFR", 0x0700_1001, persistent)),
                    ParsedItem::Record(parsed_record("REFR", 0x0700_1002, 0)),
                    ParsedItem::Record(parsed_record("ACHR", 0x0700_1003, persistent)),
                ],
            ),
            parsed_group(
                9,
                vec![ParsedItem::Record(parsed_record(
                    "REFR",
                    0x0700_1004,
                    persistent,
                ))],
            ),
        ];
        let mut output = FxHashSet::default();

        collect_output_persistent_refr_object_ids(&items, false, &mut output);

        assert_eq!(output, [0x1001].into_iter().collect());
    }

    #[test]
    fn scrub_drops_qust_alea_pair_when_alias_is_missing() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let index = quest_index(&[self_quest], &[(self_quest, &[2])]);
        // QUST \ Aliases \ External Alias Reference \ ALEA - Alias -> Quest
        // Alias [6] not found. Drop the ALEQ/ALEA pair, leave the alias row.
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![
                field("ALST", &2u32.to_le_bytes()),
                field("ALEQ", &self_quest.to_le_bytes()),
                field("ALEA", &6u32.to_le_bytes()),
                field("ALED", &[]),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ALST", "ALED"]);
    }

    #[test]
    fn scrub_keeps_qust_alea_pair_when_external_alias_exists() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let external_quest = 0x0000_1234;
        let index = quest_index(
            &[self_quest, external_quest],
            &[(self_quest, &[2]), (external_quest, &[6])],
        );
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![
                field("ALST", &2u32.to_le_bytes()),
                field("ALEQ", &external_quest.to_le_bytes()),
                field("ALEA", &6u32.to_le_bytes()),
                field("ALED", &[]),
            ],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ALST", "ALEQ", "ALEA", "ALED"]);
    }

    #[test]
    fn scrub_preserves_external_alias_when_alias_table_is_unknown() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let external_quest = 0x0000_1234;
        let mut index = quest_index(&[self_quest, external_quest], &[(self_quest, &[2])]);
        index
            .quest_alias_ids_by_encoded_quest
            .insert(external_quest, None);
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![
                field("ALST", &2u32.to_le_bytes()),
                field("ALEQ", &external_quest.to_le_bytes()),
                field("ALEA", &99u32.to_le_bytes()),
                field("ALED", &[]),
            ],
            &interner,
        );

        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ALST", "ALEQ", "ALEA", "ALED"]);
    }

    #[test]
    fn scrub_clears_invalid_qust_vmad_alias_property() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let index = quest_index(&[self_quest], &[(self_quest, &[2])]);
        let (vmad, alias_offset) = vmad_object_property(31, 0x0000_1234);
        let mut rec = record_with_interner(
            "QUST",
            0x800,
            vec![
                field("ALST", &2u32.to_le_bytes()),
                FieldEntry {
                    sig: SubrecordSig::from_str("VMAD").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(vmad)),
                },
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        let vmad = rec
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "VMAD")
            .unwrap();
        let FieldValue::Bytes(bytes) = &vmad.value else {
            panic!("VMAD should remain bytes");
        };
        assert_eq!(alias_at(bytes, alias_offset), -1);
    }

    #[test]
    fn scrub_clears_pack_vmad_alias_when_owner_is_not_quest() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let index = quest_index(&[self_quest], &[(self_quest, &[2])]);
        let (vmad, alias_offset) = vmad_object_property(2, 0x0000_1234);
        let mut rec = record_with_interner(
            "PACK",
            0x900,
            vec![
                field("QNAM", &0x0000_0555u32.to_le_bytes()),
                FieldEntry {
                    sig: SubrecordSig::from_str("VMAD").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(vmad)),
                },
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
            panic!("VMAD should remain bytes");
        };
        assert_eq!(alias_at(bytes, alias_offset), -1);
    }

    #[test]
    fn scrub_preserves_pack_vmad_alias_when_owner_alias_table_is_unknown() {
        let interner = StringInterner::new();
        let owner_quest = 0x0000_0800;
        for explicit_unknown in [false, true] {
            let mut index = quest_index(&[owner_quest], &[]);
            if explicit_unknown {
                index
                    .quest_alias_ids_by_encoded_quest
                    .insert(owner_quest, None);
            }
            let (vmad, alias_offset) = vmad_object_property(10, owner_quest);
            let mut rec = record_with_interner(
                "PACK",
                0x900,
                vec![
                    field("QNAM", &owner_quest.to_le_bytes()),
                    FieldEntry {
                        sig: SubrecordSig::from_str("VMAD").unwrap(),
                        value: FieldValue::Bytes(SmallVec::from_vec(vmad)),
                    },
                ],
                &interner,
            );

            assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
            let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
                panic!("VMAD should remain bytes");
            };
            assert_eq!(alias_at(bytes, alias_offset), 10);
        }
    }

    #[test]
    fn scrub_ff08_shutdown_pack_fragment_clears_only_alias() {
        let interner = StringInterner::new();
        let index = quest_index(&[0x0000_0800], &[(0x0000_0800, &[2])]);
        let cobj = 0x0004_695C;
        let (vmad, alias_offset) = info_fragment_vmad_object_property(10, cobj);
        let mut rec = record_with_interner(
            "PACK",
            0x2B_00CE,
            vec![
                field("QNAM", &cobj.to_le_bytes()),
                FieldEntry {
                    sig: SubrecordSig::from_str("VMAD").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(vmad)),
                },
            ],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
            panic!("VMAD should remain bytes");
        };
        assert_eq!(alias_at(bytes, alias_offset), -1);
        assert_eq!(form_id_at(bytes, alias_offset), cobj);
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
    }

    #[test]
    fn scrub_clears_scen_fragment_vmad_alias_missing_from_owner_quest() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        let index = quest_index(&[self_quest], &[(self_quest, &[2])]);
        let (mut vmad, alias_offset) = info_fragment_vmad_object_property(7, 0x0000_1234);
        vmad.extend_from_slice(&0u16.to_le_bytes());
        let mut rec = record_with_interner(
            "SCEN",
            0x900,
            vec![
                field("PNAM", &self_quest.to_le_bytes()),
                FieldEntry {
                    sig: SubrecordSig::from_str("VMAD").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(vmad)),
                },
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
            panic!("VMAD should remain bytes");
        };
        assert_eq!(alias_at(bytes, alias_offset), -1);
    }

    #[test]
    fn scrub_keeps_info_fragment_vmad_alias_without_owner_context() {
        let interner = StringInterner::new();
        let index = quest_index(&[0x0000_0800], &[(0x0000_0800, &[2])]);
        let (vmad, alias_offset) = info_fragment_vmad_object_property(11, 0x0000_1234);
        let mut rec = record_with_interner(
            "INFO",
            0x900,
            vec![FieldEntry {
                sig: SubrecordSig::from_str("VMAD").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(vmad)),
            }],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[0].value else {
            panic!("VMAD should remain bytes");
        };
        assert_eq!(alias_at(bytes, alias_offset), 11);
    }

    #[test]
    fn drops_procedural_alias_ctda_and_its_cis() {
        // GetIsAliasRef(566) with Param1=0x07A00016 (the 127926294 sentinel) —
        // a FO76 procedural id that can never be a valid alias → drop.
        let valid = FxHashSet::default();
        let mut rec = record(
            "INFO",
            vec![ctda(566, 0x07A0_0016), field("CIS1", b"alias\0")],
        );
        assert!(drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(
            sigs(&rec),
            Vec::<&str>::new(),
            "procedural alias CTDA + CIS dropped"
        );
    }

    #[test]
    fn drops_procedural_alias_ctda_on_quest_context_record() {
        let mut rec = record(
            "QUST",
            vec![
                field("FULL", b"q\0"),
                ctda(566, 0x07A0_002E),
                field("CIS1", b"alias\0"),
                field("NEXT", b"x\0"),
            ],
        );
        assert!(drop_invalid_context_quest_conditions(&mut rec));
        assert_eq!(
            sigs(&rec),
            vec!["FULL", "NEXT"],
            "procedural alias CTDA + CIS dropped on QUST"
        );
    }

    #[test]
    fn keeps_small_alias_index_on_quest_context_record() {
        let mut rec = record("QUST", vec![ctda(566, 10), field("CIS1", b"alias\0")]);
        assert!(!drop_invalid_context_quest_conditions(&mut rec));
        assert_eq!(sigs(&rec), vec!["CTDA", "CIS1"]);
    }

    #[test]
    fn keeps_non_procedural_alias_index_ctda() {
        // GetIsAliasRef(566) with a small in-range alias index (10) — a legitimate
        // alias reference the owning quest can resolve → keep.
        let valid = FxHashSet::default();
        let mut rec = record("INFO", vec![ctda(566, 0x0000_000A)]);
        assert!(!drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(sigs(&rec), vec!["CTDA"]);
    }

    #[test]
    fn keeps_procedural_param_on_non_alias_function() {
        // A 0x07A0xxxx param on a function that is neither a quest-param nor the
        // alias function must be left alone (other owners handle it).
        let valid = FxHashSet::default();
        let mut rec = record("INFO", vec![ctda(560, 0x07A0_0016)]);
        assert!(!drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(sigs(&rec), vec!["CTDA"]);
    }

    #[test]
    fn drops_fn576_null_param2_on_quest_context_record() {
        // GetInCurrentLocFormList(576) with Parameter #2 == NULL — the FO76
        // GQ_MiscRegionPointer pattern. The QUEST_CONTEXT path uses this helper.
        let mut rec = record(
            "QUST",
            vec![
                field("FULL", b"q\0"),
                ctda_p2(576, 0, 0),
                field("CIS2", b"loc\0"),
                field("NEXT", b"x\0"),
            ],
        );
        assert!(drop_null_required_formlink_conditions(&mut rec));
        assert_eq!(
            sigs(&rec),
            vec!["FULL", "NEXT"],
            "fn576 null-param2 CTDA and its CIS2 dropped, surrounding fields kept"
        );
    }

    #[test]
    fn keeps_fn576_with_non_null_param2() {
        // A real FLST/KYWD/LCTN in Parameter #2 is valid → keep.
        let mut rec = record("QUST", vec![ctda_p2(576, 0, 0x0001_2345)]);
        assert!(!drop_null_required_formlink_conditions(&mut rec));
        assert_eq!(sigs(&rec), vec!["CTDA"]);
    }

    #[test]
    fn keeps_other_function_with_null_param2() {
        // A different function with NULL Parameter #2 (where NULL is legal) must
        // not be dropped by the param2 rule.
        let mut rec = record("QUST", vec![ctda_p2(561, 0, 0)]);
        assert!(!drop_null_required_formlink_conditions(&mut rec));
        assert_eq!(sigs(&rec), vec!["CTDA"]);
    }

    #[test]
    fn drop_invalid_quest_conditions_also_drops_fn576_null_param2() {
        // On non-context records the combined predicate must catch the param2 rule
        // too, with its trailing CIS dropped.
        let valid = FxHashSet::default();
        let mut rec = record(
            "TERM",
            vec![
                ctda_p2(576, 0, 0),
                field("CIS1", b"a\0"),
                field("FULL", b"n\0"),
            ],
        );
        assert!(drop_invalid_quest_conditions(&mut rec, &valid));
        assert_eq!(sigs(&rec), vec!["FULL"]);
    }
}
