//! Fixup: drop CTDA conditions whose quest-typed Parameter #1 does not resolve
//! to a QUST, and strip alias references that do not exist on the owning quest.
//!
//! Six FO4 condition functions take a QUST FormID in Parameter #1
//! (`wbDefinitionsFO4.pas`, `Paramtype1: ptQuest`): GetQuestRunning(56),
//! GetStage(58), GetStageDone(59), GetQuestCompleted(543),
//! GetVMQuestVariable(629), HasValidRumorTopic(664). A 0/NULL quest param can be
//! resolved from an owning QNAM/PNAM on quest-context records; a non-zero value is
//! validated as a literal FormID. Converted FO76 rows often carry a quest-stage
//! number (e.g. 500, 9000), a dropped-quest id, or a non-quest form there, which
//! xEdit reports as "<X> is not a Quest record". The translation-time pair hook
//! (`drop_fo4_incompatible_conditions`) drops the NULL and RunOn==Quest-Alias cases;
//! the non-null wrong-type case needs FormID resolution against the output plugin
//! and target masters, so it is handled here.
//!
//! Exact FO76 quest-context stage gates are first lowered to FO4's layout: the
//! owning QUST/PNAM/QNAM or INFO topic supplies the QUST, and the stage stays in
//! Parameter #2 (`GetStageDone`) or the comparison value (`GetStage`). Then every
//! CTDA/CTDT with a quest-param-1 function and a non-zero, non-QUST Parameter #1 is
//! dropped, as are alias-index conditions whose alias the owning quest lacks, each
//! with its trailing CIS1/CIS2 (an orphaned CIS is an xEdit "out of order
//! subrecord"). The pair hook defers exact current-quest `GetStage` rows to this
//! pass and still drops other unresolved NULL quest parameters. Records with nothing
//! to repair or drop stay byte-identical.
//!
//! CTDAs whose function needs a non-null FLST/KYWD/LCTN Parameter #2 (e.g.
//! `GetInCurrentLocFormList`, 576, the `GQ_MiscRegionPointer*` family) but ship it
//! NULL are dropped too: FO4 rejects it ("Found a NULL reference, expected:
//! FLST,KYWD,LCTN"), and the owning quest can't supply Parameter #2.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::StringInterner;
use esp_authoring_core::plugin_runtime::{ParsedItem, effective_subrecords_for_record};

/// FO4 condition functions whose Parameter #1 is a QUST FormID
/// (mirrors `FO4_QUEST_PARAMETER_1_CONDITION_FUNCTION_IDS` in the pair hook).
const QUEST_PARAMETER_1_FUNCTION_IDS: &[u16] = &[56, 58, 59, 543, 629, 664];

/// `drop_trace` stage tag for every CTDA this pass removes. Mirrors the
/// translation-time `fo76_fo4.conditions` tag so a single
/// `MODBOX_TRACE_DROPS=1` run shows both condition choke points.
const CONDITION_TRACE_STAGE: &str = "fixups.quest_conditions";

const GET_STAGE_FUNCTION_ID: u16 = 58;
const GET_STAGE_DONE_FUNCTION_ID: u16 = 59;
const CTDA_RUN_ON_SUBJECT: u32 = 0;
const CTDA_COMPARISON_GLOBAL_FLAG: u8 = 0x04;

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

/// FO4 condition functions whose Parameter #2 is a required FLST/KYWD/LCTN
/// formlink, notably `GetInCurrentLocFormList` (576). FO76 quests (e.g. the
/// `GQ_MiscRegionPointer*` family) ship it NULL, which xEdit reports as "Found a NULL
/// reference, expected: FLST,KYWD,LCTN". Unrepairable, so the CTDA and its trailing
/// CIS1/CIS2 are dropped on every record sig, quest-context types included.
const QUEST_PARAMETER_2_REQUIRED_FORMLINK_FUNCTION_IDS: &[u16] = &[576];

/// `GetInWorldspace`: Parameter #1 is a required WRLD FormID. FO76 conditions
/// can retain the source plugin's load byte (`00`) after the WRLD itself has
/// moved into the output plugin (for example `0025DA15` -> `0725DA15`).
const GET_IN_WORLDSPACE_FUNCTION_ID: u16 = 310;

/// Reference-parameter functions repaired only after every placed child has
/// been materialized: GetDistance and GetWithinDistance.
const FINAL_PERSISTENT_REFERENCE_FUNCTION_IDS: &[u16] = &[1, 639];
const CELL_PERSISTENT_GROUP: i32 = 8;
/// CTDA operator byte bit 0x02 ("Use Aliases"). When set, the reference-typed
/// Parameter #1 of `GetDistance`/`GetWithinDistance` is a quest alias index, not a
/// FormID; validating it as one drops or retargets a valid row (alias `0x0A` becomes
/// `0x8B00000A`). The gate also checks function id: `GetItemCount` (47) ships with
/// the flag set and a real FormID in Parameter #1 that still needs its remap.
const CTDA_USE_ALIASES_FLAG: u8 = 0x02;

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

        let quest_index =
            collect_quest_condition_index(session, mapper, config, target_schema, &mut report)?;
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
    lower_source_pack_stage_conditions: bool,
    lowered_source_pack_conditions_by_target: FxHashMap<FormKey, LoweredSourcePackConditions>,
    quest_alias_ids_by_encoded_quest: FxHashMap<u32, Option<FxHashSet<u32>>>,
    /// Alias ids the SOURCE plugin's quest declares, keyed by the TARGET encoded
    /// quest id. Corroborating evidence for the alias-existence guard: see
    /// [`alias_id_is_invalid`].
    source_alias_ids_by_encoded_quest: FxHashMap<u32, FxHashSet<u32>>,
    quest_stage_ids_by_encoded_quest: FxHashMap<u32, Option<FxHashSet<u32>>>,
    encoded_quest_by_form_key: FxHashMap<FormKey, u32>,
    info_owner_quest_by_local: FxHashMap<u32, u32>,
    valid_typed_param1_ids_by_function: FxHashMap<u16, FxHashSet<u32>>,
    valid_worldspace_ids: FxHashSet<u32>,
    output_worldspace_id_by_object_id: FxHashMap<u32, u32>,
    target_masters: Vec<String>,
}

struct LoweredSourcePackConditions {
    owner_quest: u32,
    groups: Vec<LoweredSourcePackConditionGroup>,
}

struct LoweredSourcePackConditionGroup {
    conditions: Vec<SmallVec<[u8; 32]>>,
    previous_anchor_sig: Option<[u8; 4]>,
    next_anchor_sig: Option<[u8; 4]>,
}

impl QuestConditionIndex {
    fn has_usable_quest_index(&self) -> bool {
        self.quest_index_complete && !self.valid_quest_ids.is_empty()
    }

    pub(crate) fn can_classify_conditions(&self) -> bool {
        self.has_usable_quest_index() || !self.valid_worldspace_ids.is_empty()
    }
}

fn is_fo76_to_fo4_game_pair(source_game: Option<&str>, target_game: Option<&str>) -> bool {
    source_game == Some("fo76") && target_game == Some("fo4")
}

pub(crate) fn collect_quest_condition_index(
    session: &mut PluginSession,
    mapper: &FormKeyMapper<'_>,
    config: &FixupConfig,
    target_schema: &AuthoringSchema,
    report: &mut FixupReport,
) -> Result<QuestConditionIndex, FixupError> {
    let interner = mapper.interner;
    let lower_source_pack_stage_conditions = is_fo76_to_fo4_game_pair(
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref()),
        session.target_slot().parsed.game.as_deref(),
    );
    let (valid_quest_ids, quest_index_complete) =
        collect_valid_quest_encoded_ids(session, interner, config, report)?;
    let target_masters = session.target_masters().to_vec();
    let qust_sig = SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut quest_alias_ids_by_encoded_quest = FxHashMap::default();
    let mut quest_stage_ids_by_encoded_quest = FxHashMap::default();
    let mut encoded_quest_by_form_key = FxHashMap::default();

    let output_fks = session
        .form_keys_of_sig(qust_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in output_fks {
        let Some(encoded) = encode_form_id(&fk, interner, &target_masters) else {
            continue;
        };
        encoded_quest_by_form_key.insert(fk, encoded);
        let record = session.record_decoded(&fk, target_schema, interner).ok();
        let alias_ids = record
            .as_ref()
            .map(|record| collect_alias_ids_from_record(record));
        let stage_ids = record
            .as_ref()
            .map(|record| collect_stage_ids_from_record(record, interner));
        quest_alias_ids_by_encoded_quest.insert(encoded, alias_ids);
        quest_stage_ids_by_encoded_quest.insert(encoded, stage_ids);
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
            encoded_quest_by_form_key.insert(fk, encoded);
            let record = session
                .record_decoded_in_handle(handle_id, &fk, target_schema, interner)
                .ok();
            let alias_ids = record
                .as_ref()
                .map(|record| collect_alias_ids_from_record(record));
            let stage_ids = record
                .as_ref()
                .map(|record| collect_stage_ids_from_record(record, interner));
            quest_alias_ids_by_encoded_quest.insert(encoded, alias_ids);
            quest_stage_ids_by_encoded_quest.insert(encoded, stage_ids);
        }
    }

    let valid_typed_param1_ids_by_function =
        collect_typed_parameter_1_encoded_ids(session, interner, config, &target_masters, report)?;
    let (valid_worldspace_ids, output_worldspace_id_by_object_id) =
        collect_worldspace_encoded_ids(session, interner, config, &target_masters, report)?;
    for (source, target) in mapper.source_to_target_iter() {
        if let Some(encoded) = encode_form_id(&target, interner, &target_masters)
            && valid_quest_ids.contains(&encoded)
        {
            encoded_quest_by_form_key.insert(source, encoded);
            encoded_quest_by_form_key.insert(target, encoded);
        }
    }
    let lowered_source_pack_conditions_by_target = if lower_source_pack_stage_conditions {
        config
            .source_schema
            .as_deref()
            .map(|source_schema| {
                collect_lowered_source_pack_conditions(
                    session,
                    mapper,
                    source_schema,
                    &valid_quest_ids,
                    &quest_stage_ids_by_encoded_quest,
                    &encoded_quest_by_form_key,
                )
            })
            .transpose()?
            .unwrap_or_default()
    } else {
        FxHashMap::default()
    };
    let info_owner_quest_by_local =
        collect_info_owner_quests(&session.target_slot().parsed.root_items);
    let source_alias_ids_by_encoded_quest = collect_source_quest_alias_ids(
        session,
        mapper,
        config,
        qust_sig,
        &target_masters,
        &encoded_quest_by_form_key,
    );

    // An empty source table is the silent-degradation mode of the alias guard:
    // every alias verdict falls back to "no evidence" and the guard stops
    // working. It is not an error (the guard fails safe), but it must never be
    // invisible again.
    if source_alias_ids_by_encoded_quest.is_empty() && !quest_alias_ids_by_encoded_quest.is_empty()
    {
        report.warnings.push(interner.intern(&format!(
            "strip_invalid_quest_condition_params: source quest alias table is EMPTY \
             ({} target quests indexed, source_slot={}, source_schema={}); alias-existence \
             validation is disabled for this run",
            quest_alias_ids_by_encoded_quest.len(),
            session.source_slot_opt().is_some(),
            config.source_schema.is_some(),
        )));
    }

    if crate::drop_trace::enabled() {
        // The alias guard silently degrades when any of these tables is empty
        // (an absent source table turns the corroboration into a no-op), so the
        // sizes are recorded once per index build.
        crate::drop_trace::trace(
            CONDITION_TRACE_STAGE,
            "QUST",
            0,
            "",
            &format!(
                "index built; valid_quests={} quest_index_complete={quest_index_complete} \
                 target_alias_quests={} source_alias_quests={} info_owners={} \
                 worldspaces={} source_slot={} source_schema={}",
                valid_quest_ids.len(),
                quest_alias_ids_by_encoded_quest.len(),
                source_alias_ids_by_encoded_quest.len(),
                info_owner_quest_by_local.len(),
                valid_worldspace_ids.len(),
                session.source_slot_opt().is_some(),
                config.source_schema.is_some(),
            ),
        );
    }

    Ok(QuestConditionIndex {
        valid_quest_ids,
        quest_index_complete,
        lower_source_pack_stage_conditions,
        lowered_source_pack_conditions_by_target,
        quest_alias_ids_by_encoded_quest,
        source_alias_ids_by_encoded_quest,
        quest_stage_ids_by_encoded_quest,
        encoded_quest_by_form_key,
        info_owner_quest_by_local,
        valid_typed_param1_ids_by_function,
        valid_worldspace_ids,
        output_worldspace_id_by_object_id,
        target_masters,
    })
}

/// Alias ids declared by each SOURCE quest, keyed by the encoded id of the quest
/// it maps to in the output. Unlike the target-side table this is read from the
/// immutable source plugin, so it is unaffected by how far the output plugin has
/// progressed through the pipeline.
fn collect_source_quest_alias_ids(
    session: &mut PluginSession,
    mapper: &FormKeyMapper<'_>,
    config: &FixupConfig,
    qust_sig: SigCode,
    target_masters: &[String],
    encoded_quest_by_form_key: &FxHashMap<FormKey, u32>,
) -> FxHashMap<u32, FxHashSet<u32>> {
    let interner = mapper.interner;
    let mut out = FxHashMap::default();
    let Some(source_schema) = config.source_schema.as_deref() else {
        return out;
    };
    if session.source_slot_opt().is_none() {
        return out;
    }
    let Ok(source_keys) = session.source_form_keys_of_sig(qust_sig, interner) else {
        return out;
    };
    for source_key in source_keys {
        // The guard looks a quest up by whichever encoding the owning record
        // carries, which depends on how far the mapper and output have progressed.
        // A missed key degrades the guard to target-only, so the row is filed
        // under every encoding the quest can answer to.
        let mut encodings: SmallVec<[u32; 3]> = SmallVec::new();
        let mut push = |encoded: Option<u32>| {
            if let Some(encoded) = encoded
                && !encodings.contains(&encoded)
            {
                encodings.push(encoded);
            }
        };
        if let Some(target) = mapper.lookup(source_key) {
            push(encoded_quest_by_form_key.get(&target).copied());
            push(encode_form_id(&target, interner, target_masters));
        }
        push(encoded_quest_by_form_key.get(&source_key).copied());
        push(encode_form_id(&source_key, interner, target_masters));
        if encodings.is_empty() {
            continue;
        }
        let Ok(record) = session.source_record_decoded(&source_key, source_schema, interner) else {
            continue;
        };
        let alias_ids = collect_alias_ids_from_record(&record);
        for encoded in encodings {
            out.entry(encoded)
                .or_insert_with(FxHashSet::default)
                .extend(alias_ids.iter().copied());
        }
    }
    out
}

fn collect_info_owner_quests(items: &[ParsedItem]) -> FxHashMap<u32, u32> {
    const TOPIC_CHILD_GROUP: i32 = 7;
    const OBJECT_ID_MASK: u32 = 0x00FF_FFFF;

    fn walk(
        items: &[ParsedItem],
        topic_quests: &mut FxHashMap<u32, u32>,
        info_topics: &mut Vec<(u32, u32)>,
    ) {
        for item in items {
            match item {
                ParsedItem::Record(record) => {
                    if record.signature.eq_ignore_ascii_case("DIAL")
                        && let Some(owner_quest) = effective_subrecords_for_record(record)
                            .iter()
                            .find(|subrecord| subrecord.signature.eq_ignore_ascii_case("QNAM"))
                            .and_then(|subrecord| {
                                (subrecord.data.len() >= 4).then(|| {
                                    u32::from_le_bytes(subrecord.data[..4].try_into().unwrap())
                                })
                            })
                            .filter(|owner_quest| *owner_quest != 0)
                    {
                        topic_quests.insert(record.form_id & OBJECT_ID_MASK, owner_quest);
                    }
                }
                ParsedItem::Group(group) => {
                    if group.group_type == TOPIC_CHILD_GROUP {
                        let topic = u32::from_le_bytes(group.label) & OBJECT_ID_MASK;
                        for child in &group.children {
                            if let ParsedItem::Record(record) = child
                                && record.signature.eq_ignore_ascii_case("INFO")
                            {
                                info_topics.push((record.form_id & OBJECT_ID_MASK, topic));
                            }
                        }
                    }
                    walk(&group.children, topic_quests, info_topics);
                }
                _ => {}
            }
        }
    }

    let mut topic_quests = FxHashMap::default();
    let mut info_topics = Vec::new();
    walk(items, &mut topic_quests, &mut info_topics);
    info_topics
        .into_iter()
        .filter_map(|(info, topic)| topic_quests.get(&topic).copied().map(|quest| (info, quest)))
        .collect()
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

fn collect_stage_ids_from_record(record: &Record, interner: &StringInterner) -> FxHashSet<u32> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"INDX")
        .filter_map(|entry| stage_index_from_value(&entry.value, interner))
        .collect()
}

fn stage_index_from_value(value: &FieldValue, interner: &StringInterner) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u16::try_from(*value).ok().map(u32::from),
        FieldValue::Int(value) => u16::try_from(*value).ok().map(u32::from),
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u32::from(u16::from_le_bytes([bytes[0], bytes[1]])))
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
            interner
                .resolve(*name)
                .is_some_and(|name| name.replace('_', "").eq_ignore_ascii_case("stageindex"))
                .then(|| stage_index_from_value(value, interner))
                .flatten()
        }),
        _ => None,
    }
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
    Known {
        /// Alias ids on the owning quest as it currently stands in the output.
        target: &'a FxHashSet<u32>,
        /// Alias ids the same quest declares in the source plugin.
        source: Option<&'a FxHashSet<u32>>,
    },
    Unknown,
    None,
}

impl<'a> AliasContext<'a> {
    fn known_ids(self) -> Option<&'a FxHashSet<u32>> {
        match self {
            AliasContext::Known { target, .. } => Some(target),
            AliasContext::Unknown | AliasContext::None => None,
        }
    }

    fn can_validate(self) -> bool {
        !matches!(self, AliasContext::Unknown)
    }

    /// Human-readable state for `drop_trace`. Call only when tracing is on.
    fn describe(self) -> String {
        match self {
            AliasContext::Known { target, source } => format!(
                "alias_ctx=known target_aliases={} source_aliases={}",
                target.len(),
                source.map_or("absent".to_string(), |source| source.len().to_string())
            ),
            AliasContext::Unknown => "alias_ctx=unknown".to_string(),
            AliasContext::None => "alias_ctx=none".to_string(),
        }
    }
}

fn alias_context_for_encoded_quest<'a>(
    encoded: u32,
    index: &'a QuestConditionIndex,
) -> AliasContext<'a> {
    let source = index.source_alias_ids_by_encoded_quest.get(&encoded);
    match index.quest_alias_ids_by_encoded_quest.get(&encoded) {
        Some(Some(alias_ids)) => AliasContext::Known {
            target: alias_ids,
            source,
        },
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
        return index
            .info_owner_quest_by_local
            .get(&record.form_key.local)
            .copied()
            .map(|encoded| alias_context_for_encoded_quest(encoded, index))
            .unwrap_or(AliasContext::Unknown);
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

fn raw_condition_parameter_3(bytes: &[u8]) -> Option<u32> {
    (bytes.len() >= 32).then(|| u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]))
}

fn raw_condition_operator(bytes: &[u8]) -> Option<u8> {
    bytes.first().map(|value| (value >> 5) & 0x07)
}

fn raw_condition_comparison_value(bytes: &[u8]) -> Option<f32> {
    if bytes.len() < 8 || bytes[0] & CTDA_COMPARISON_GLOBAL_FLAG != 0 {
        return None;
    }
    Some(f32::from_le_bytes(bytes[4..8].try_into().ok()?))
}

fn has_source_stage_condition_shape(bytes: &[u8]) -> bool {
    bytes.len() == 32 && raw_condition_operator(bytes).is_some_and(|operator| operator <= 5)
}

fn normalize_lowered_quest_stage_condition(bytes: &mut [u8]) {
    bytes[20..24].copy_from_slice(&CTDA_RUN_ON_SUBJECT.to_le_bytes());
    bytes[24..28].copy_from_slice(&0_u32.to_le_bytes());
    bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
}

fn integral_comparison_stage(bytes: &[u8]) -> Option<u32> {
    let comparison = raw_condition_comparison_value(bytes)?;
    (comparison.is_finite()
        && comparison >= 0.0
        && comparison <= u16::MAX as f32
        && comparison.fract() == 0.0)
        .then_some(comparison as u32)
}

fn encoded_pack_owner_quest_from_maps(
    record: &Record,
    valid_quest_ids: &FxHashSet<u32>,
    encoded_quest_by_form_key: &FxHashMap<FormKey, u32>,
) -> Option<u32> {
    if record.sig.0 != *b"PACK" {
        return None;
    }
    let mut qnams = record.fields.iter().filter(|entry| entry.sig.0 == *b"QNAM");
    let qnam = qnams.next()?;
    if qnams.next().is_some() {
        return None;
    }
    match &qnam.value {
        FieldValue::FormKey(form_key) => encoded_quest_by_form_key.get(form_key).copied(),
        value => field_value_u32(value).filter(|encoded| valid_quest_ids.contains(encoded)),
    }
}

fn encoded_pack_owner_quest(record: &Record, index: &QuestConditionIndex) -> Option<u32> {
    encoded_pack_owner_quest_from_maps(
        record,
        &index.valid_quest_ids,
        &index.encoded_quest_by_form_key,
    )
}

fn lower_source_shaped_stage_condition(
    bytes: &mut [u8],
    owner_quest: u32,
    stage_ids: &FxHashSet<u32>,
    valid_quest_ids: &FxHashSet<u32>,
) -> bool {
    if !has_source_stage_condition_shape(bytes) {
        return false;
    }
    match raw_condition_function_id(bytes) {
        Some(GET_STAGE_DONE_FUNCTION_ID) => {
            if bytes[0] & CTDA_COMPARISON_GLOBAL_FLAG != 0 {
                return false;
            }
            let Some(stage) = raw_condition_parameter_1(bytes) else {
                return false;
            };
            if raw_condition_parameter_2(bytes) != Some(0)
                || valid_quest_ids.contains(&stage)
                || !stage_ids.contains(&stage)
            {
                return false;
            }
            bytes[12..16].copy_from_slice(&owner_quest.to_le_bytes());
            bytes[16..20].copy_from_slice(&stage.to_le_bytes());
            normalize_lowered_quest_stage_condition(bytes);
            true
        }
        Some(GET_STAGE_FUNCTION_ID) => {
            if raw_condition_parameter_1(bytes) != Some(0)
                || raw_condition_parameter_2(bytes) != Some(0)
            {
                return false;
            }
            let Some(stage) = integral_comparison_stage(bytes) else {
                return false;
            };
            if !stage_ids.contains(&stage) {
                return false;
            }
            bytes[12..16].copy_from_slice(&owner_quest.to_le_bytes());
            normalize_lowered_quest_stage_condition(bytes);
            true
        }
        _ => false,
    }
}

fn encoded_stage_context_owner_quest(
    record: &Record,
    index: &QuestConditionIndex,
    interner: &StringInterner,
) -> Option<u32> {
    if record.sig.0 == *b"QUST" {
        return encode_form_id(&record.form_key, interner, &index.target_masters)
            .filter(|encoded| index.valid_quest_ids.contains(encoded));
    }
    if record.sig.0 == *b"INFO" {
        return index
            .info_owner_quest_by_local
            .get(&record.form_key.local)
            .copied()
            .filter(|encoded| index.valid_quest_ids.contains(encoded));
    }

    let owner_sig = match &record.sig.0 {
        b"SCEN" => *b"PNAM",
        b"PACK" | b"DIAL" => *b"QNAM",
        _ => return None,
    };
    if record.sig.0 == *b"SCEN" {
        // Package actions also carry PNAM. The scene owner is the terminal PNAM
        // after all action rows, so use the final occurrence.
        let owner = record
            .fields
            .iter()
            .rev()
            .find(|entry| entry.sig.0 == owner_sig)?;
        return match &owner.value {
            FieldValue::FormKey(form_key) => index.encoded_quest_by_form_key.get(form_key).copied(),
            value => field_value_encoded_form_id(value, interner, &index.target_masters),
        }
        .filter(|encoded| index.valid_quest_ids.contains(encoded));
    }
    let mut owners = record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == owner_sig);
    let owner = owners.next()?;
    if owners.next().is_some() {
        return None;
    }
    match &owner.value {
        FieldValue::FormKey(form_key) => index.encoded_quest_by_form_key.get(form_key).copied(),
        value => field_value_encoded_form_id(value, interner, &index.target_masters),
    }
    .filter(|encoded| index.valid_quest_ids.contains(encoded))
}

fn lower_source_shaped_quest_context_stage_conditions(
    record: &mut Record,
    index: &QuestConditionIndex,
    interner: &StringInterner,
) -> bool {
    let Some(owner_quest) = encoded_stage_context_owner_quest(record, index, interner) else {
        return false;
    };
    let Some(Some(stage_ids)) = index.quest_stage_ids_by_encoded_quest.get(&owner_quest) else {
        return false;
    };

    let mut changed = false;
    for entry in &mut record.fields {
        if entry.sig.0 != *b"CTDA" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= lower_source_shaped_stage_condition(
            bytes,
            owner_quest,
            stage_ids,
            &index.valid_quest_ids,
        );
    }
    changed
}

fn is_condition_group_field(sig: &[u8; 4]) -> bool {
    matches!(sig, b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2")
}

fn unique_signature_position(record: &Record, sig: [u8; 4]) -> Option<usize> {
    let mut positions = record
        .fields
        .iter()
        .enumerate()
        .filter_map(|(position, entry)| (entry.sig.0 == sig).then_some(position));
    let position = positions.next()?;
    positions.next().is_none().then_some(position)
}

fn lowered_source_pack_condition_groups(
    record: &Record,
    owner_quest: u32,
    stage_ids: &FxHashSet<u32>,
    valid_quest_ids: &FxHashSet<u32>,
) -> Vec<LoweredSourcePackConditionGroup> {
    let mut groups = Vec::new();
    let mut field_index = 0;

    while field_index < record.fields.len() {
        if !is_condition_group_field(&record.fields[field_index].sig.0) {
            field_index += 1;
            continue;
        }

        let group_start = field_index;
        while field_index < record.fields.len()
            && is_condition_group_field(&record.fields[field_index].sig.0)
        {
            field_index += 1;
        }
        let group_end = field_index;
        let previous_anchor_sig = group_start
            .checked_sub(1)
            .map(|position| record.fields[position].sig.0);
        let next_anchor_sig = record.fields.get(group_end).map(|entry| entry.sig.0);
        if previous_anchor_sig.is_none() && next_anchor_sig.is_none() {
            continue;
        }

        if previous_anchor_sig == next_anchor_sig
            || previous_anchor_sig
                .is_some_and(|sig| unique_signature_position(record, sig) != Some(group_start - 1))
            || next_anchor_sig
                .is_some_and(|sig| unique_signature_position(record, sig) != Some(group_end))
        {
            continue;
        }

        let mut conditions = Vec::with_capacity(group_end - group_start);
        let exact_group = record.fields[group_start..group_end].iter().all(|entry| {
            if entry.sig.0 != *b"CTDA" {
                return false;
            }
            let FieldValue::Bytes(bytes) = &entry.value else {
                return false;
            };
            let mut lowered = bytes.clone();
            if !lower_source_shaped_stage_condition(
                &mut lowered,
                owner_quest,
                stage_ids,
                valid_quest_ids,
            ) {
                return false;
            }
            conditions.push(lowered);
            true
        });
        if exact_group {
            groups.push(LoweredSourcePackConditionGroup {
                conditions,
                previous_anchor_sig,
                next_anchor_sig,
            });
        }
    }

    groups
}

fn collect_lowered_source_pack_conditions(
    session: &mut PluginSession,
    mapper: &FormKeyMapper<'_>,
    source_schema: &AuthoringSchema,
    valid_quest_ids: &FxHashSet<u32>,
    quest_stage_ids_by_encoded_quest: &FxHashMap<u32, Option<FxHashSet<u32>>>,
    encoded_quest_by_form_key: &FxHashMap<FormKey, u32>,
) -> Result<FxHashMap<FormKey, LoweredSourcePackConditions>, FixupError> {
    let pack_sig = SigCode::from_str("PACK").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let source_pack_keys = session
        .source_form_keys_of_sig(pack_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut lowered_by_target = FxHashMap::default();
    let mut ambiguous_targets = FxHashSet::default();

    for source_pack_key in source_pack_keys {
        let Some(target_pack_key) = mapper.lookup(source_pack_key) else {
            continue;
        };
        if ambiguous_targets.contains(&target_pack_key) {
            continue;
        }
        let Ok(source_record) =
            session.source_record_decoded(&source_pack_key, source_schema, mapper.interner)
        else {
            continue;
        };
        let Some(owner_quest) = encoded_pack_owner_quest_from_maps(
            &source_record,
            valid_quest_ids,
            encoded_quest_by_form_key,
        ) else {
            continue;
        };
        let Some(Some(stage_ids)) = quest_stage_ids_by_encoded_quest.get(&owner_quest) else {
            continue;
        };

        let groups = lowered_source_pack_condition_groups(
            &source_record,
            owner_quest,
            stage_ids,
            valid_quest_ids,
        );
        if groups.is_empty() {
            continue;
        }
        let recovered = LoweredSourcePackConditions {
            owner_quest,
            groups,
        };
        match lowered_by_target.entry(target_pack_key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(recovered);
            }
            std::collections::hash_map::Entry::Occupied(entry) => {
                entry.remove();
                ambiguous_targets.insert(target_pack_key);
            }
        }
    }

    Ok(lowered_by_target)
}

fn restore_missing_source_pack_stage_conditions(
    record: &mut Record,
    index: &QuestConditionIndex,
) -> bool {
    let Some(recovered) = index
        .lowered_source_pack_conditions_by_target
        .get(&record.form_key)
    else {
        return false;
    };
    if encoded_pack_owner_quest(record, index) != Some(recovered.owner_quest) {
        return false;
    }

    let mut changed = false;
    for group in &recovered.groups {
        let insert_at = match (group.previous_anchor_sig, group.next_anchor_sig) {
            (Some(previous_sig), Some(next_sig)) => {
                let Some(previous_anchor) = unique_signature_position(record, previous_sig) else {
                    continue;
                };
                let Some(next_anchor) = unique_signature_position(record, next_sig) else {
                    continue;
                };
                if next_anchor != previous_anchor + 1 {
                    continue;
                }
                next_anchor
            }
            (None, Some(next_sig)) => {
                let Some(next_anchor) = unique_signature_position(record, next_sig) else {
                    continue;
                };
                if next_anchor != 0 {
                    continue;
                }
                0
            }
            (Some(previous_sig), None) => {
                let Some(previous_anchor) = unique_signature_position(record, previous_sig) else {
                    continue;
                };
                if previous_anchor + 1 != record.fields.len() {
                    continue;
                }
                record.fields.len()
            }
            (None, None) => continue,
        };

        for (offset, condition) in group.conditions.iter().cloned().enumerate() {
            record.fields.insert(
                insert_at + offset,
                FieldEntry {
                    sig: SubrecordSig(*b"CTDA"),
                    value: FieldValue::Bytes(condition),
                },
            );
        }
        changed = true;
    }
    if changed && record.fields.iter().any(|entry| entry.sig.0 == *b"CITC") {
        record.sync_condition_count();
    }
    changed
}

/// True when a CTDA's function requires a non-null FLST/KYWD/LCTN in Parameter #2
/// (e.g. `GetInCurrentLocFormList`, 576) but Parameter #2 is NULL — an FO4-invalid
/// condition that cannot be repaired.
fn is_null_required_formlink_param(bytes: &[u8]) -> bool {
    let function_id = raw_condition_function_id(bytes).unwrap_or(0);
    let parameter_2 = raw_condition_parameter_2(bytes).unwrap_or(0);
    QUEST_PARAMETER_2_REQUIRED_FORMLINK_FUNCTION_IDS.contains(&function_id) && parameter_2 == 0
}

/// True when the "Use Aliases" flag makes Parameter #1 a quest alias index
/// rather than a FormID. Only the reference-parameter functions are affected.
fn condition_parameter_1_is_alias_index(bytes: &[u8]) -> bool {
    let Some(flags) = bytes.first() else {
        return false;
    };
    flags & CTDA_USE_ALIASES_FLAG != 0
        && raw_condition_function_id(bytes)
            .is_some_and(|id| FINAL_PERSISTENT_REFERENCE_FUNCTION_IDS.contains(&id))
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

/// True only for an alias id that is provably not an alias of the owning quest.
///
/// The target alias table is read mid-pipeline, so absence there is not proof: a
/// snapshot taken before the table is final reports valid aliases as missing, and
/// the drops are permanent (on SeventySix, 2,254 `GetIsAliasRef` speaker gates, 2,234
/// of them naming an alias the source quest declares; the lines then played for any
/// actor the topic reached). A drop needs positive source evidence: the owning quest
/// has a row in the source alias table and does not declare the alias. A missing row
/// (any source handle/schema/decode/encoding failure yields an empty or partial
/// table) means no evidence and no drop.
///
/// If the output quest really lost the alias, the surviving condition evaluates
/// false and the record stops playing, which is stricter than dropping an ANDed
/// condition (the record would play unconditionally).
fn alias_id_is_invalid(alias_id: u32, alias_context: AliasContext<'_>) -> bool {
    match alias_context {
        AliasContext::Known {
            target,
            source: Some(source),
        } => !target.contains(&alias_id) && !source.contains(&alias_id),
        AliasContext::Known { source: None, .. } | AliasContext::Unknown | AliasContext::None => {
            false
        }
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
        // Quest Alias stores its alias index in Parameter #3; Reference remains unused.
        let alias_id = raw_condition_parameter_3(bytes).unwrap_or(u32::MAX);
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
                    if !FINAL_PERSISTENT_REFERENCE_FUNCTION_IDS.contains(&function_id)
                        || condition_parameter_1_is_alias_index(bytes)
                    {
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
    let mut changed = index.lower_source_pack_stage_conditions
        && lower_source_shaped_quest_context_stage_conditions(record, index, interner);
    if index.lower_source_pack_stage_conditions {
        changed |= restore_missing_source_pack_stage_conditions(record, index);
    }
    let trace_on = crate::drop_trace::enabled();
    // `SigCode` is `Copy`, so this costs nothing when tracing is off — the sweep
    // runs this closure over every record in the plugin.
    let trace_sig = record.sig.0;
    let trace_local = record.form_key.local;
    let trace_alias_context = if trace_on {
        alias_context.describe()
    } else {
        String::new()
    };
    changed |= drop_conditions_matching(record, |bytes| {
        let (invalid_worldspace, repaired) = repair_or_reject_worldspace_parameter(bytes, index);
        repaired_worldspace |= repaired;
        let function_id = raw_condition_function_id(bytes).unwrap_or(0);
        let parameter_1 = raw_condition_parameter_1(bytes).unwrap_or(0);
        let invalid_quest_param = can_validate_quests
            && QUEST_PARAMETER_1_FUNCTION_IDS.contains(&function_id)
            && !index.valid_quest_ids.contains(&parameter_1);
        let reason = if invalid_worldspace {
            Some("worldspace param_1 does not resolve to a WRLD")
        } else if invalid_quest_param {
            Some("quest-typed param_1 is not a known QUST")
        } else if can_validate_quests && condition_has_invalid_alias_ref(bytes, alias_context) {
            Some("alias reference not found on the owning quest")
        } else if condition_has_invalid_typed_param1(bytes, index) {
            Some("typed param_1 is null or the wrong record type")
        } else if is_null_required_formlink_param(bytes) {
            Some("required param_2 formlink is NULL")
        } else {
            None
        };
        let Some(reason) = reason else {
            return false;
        };
        if trace_on {
            let run_on = raw_condition_run_on(bytes).unwrap_or(0);
            let run_on_reference = raw_condition_run_on_reference(bytes).unwrap_or(0);
            let parameter_3 = raw_condition_parameter_3(bytes).unwrap_or(u32::MAX);
            crate::drop_trace::trace(
                CONDITION_TRACE_STAGE,
                std::str::from_utf8(&trace_sig).unwrap_or("????"),
                trace_local,
                "CTDA",
                &format!(
                    "{reason}; function={function_id} parameter_1={parameter_1:08X} \
                     run_on={run_on} run_on_reference={run_on_reference:08X} \
                     parameter_3={parameter_3:08X} \
                     {trace_alias_context}"
                ),
            );
        }
        true
    });
    changed |= repaired_worldspace;
    if can_validate_quests && alias_context.can_validate() {
        changed |=
            drop_invalid_alea_external_aliases(record, index, alias_context.known_ids(), interner);
    }
    if can_validate_quests && matches!(alias_context, AliasContext::Known { .. }) {
        changed |= scrub_invalid_vmad_aliases(record, index, alias_context.known_ids());
    }
    if can_validate_quests {
        changed |= neutralize_dangling_pack_alias_data_targets(record, alias_context);
    }
    changed
}

// PACK target (PTDA) and location (PLDT/PLVD) unions can select a quest-alias
// variant: the offset-4 value is an alias index into the package's owning quest
// (QNAM), not a FormID. Quests and their aliases are translated, so these bindings
// are kept (e.g. AC_MQ01_Opportunity_Abbie_Breakdown's "target RefAlias 18").
// Neutralize to the benign non-alias selector only when the owning quest is known
// and provably lacks the alias id; otherwise preserve.
const PACK_TARGET_ALIAS_TYPE: i32 = 4;
const PACK_TARGET_SELF_TYPE: i32 = 6;
const PACK_LOCATION_ALIAS_TYPES: &[i32] = &[8, 9, 14];
const PACK_LOCATION_NEAR_PACKAGE_START_TYPE: i32 = 2;

fn neutralize_dangling_pack_alias_data_targets(
    record: &mut Record,
    alias_context: AliasContext<'_>,
) -> bool {
    if record.sig.0 != *b"PACK" {
        return false;
    }
    let mut changed = false;
    for entry in &mut record.fields {
        let (alias_types, replacement) = match &entry.sig.0 {
            b"PLDT" | b"PLVD" => (
                PACK_LOCATION_ALIAS_TYPES,
                PACK_LOCATION_NEAR_PACKAGE_START_TYPE,
            ),
            b"PTDA" => (
                std::slice::from_ref(&PACK_TARGET_ALIAS_TYPE),
                PACK_TARGET_SELF_TYPE,
            ),
            _ => continue,
        };
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.len() < 8 {
            continue;
        }
        let type_value = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if !alias_types.contains(&type_value) {
            continue;
        }
        let alias_index = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        // `alias_id_is_invalid` is true ONLY for a Known owning quest whose alias
        // set excludes this index — a proven dangle. `Unknown` (quest present,
        // aliases not indexed) and `None` (no resolvable owning quest) preserve, so
        // a valid alias binding is never severed.
        if !alias_id_is_invalid(alias_index, alias_context) {
            continue;
        }
        bytes[0..4].copy_from_slice(&replacement.to_le_bytes());
        // The benign replacement types treat offset 4 as cpIgnore; the dead alias
        // index is zeroed.
        bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
        changed = true;
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

fn scrub_invalid_vmad_aliases(
    record: &mut Record,
    index: &QuestConditionIndex,
    alias_ids: Option<&FxHashSet<u32>>,
) -> bool {
    let mut changed = false;
    let record_sig = record.sig.0;
    for entry in &mut record.fields {
        if entry.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if scrub_invalid_vmad_aliases_in_blob(bytes.as_mut_slice(), &record_sig, index, alias_ids) {
            changed = true;
        }
    }
    changed
}

fn scrub_invalid_vmad_aliases_in_blob(
    data: &mut [u8],
    record_sig: &[u8; 4],
    index: &QuestConditionIndex,
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
        if scrub_vmad_script_entry(
            data,
            &mut offset,
            object_format,
            index,
            alias_ids,
            &mut changed,
        )
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
                index,
                alias_ids,
                &mut changed,
            );
        }
        b"SCEN" => {
            scrub_vmad_scen_fragments(
                data,
                &mut offset,
                object_format,
                index,
                alias_ids,
                &mut changed,
            );
        }
        b"QUST" => {
            scrub_vmad_qust_after_scripts(
                data,
                &mut offset,
                object_format,
                index,
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
    index: &QuestConditionIndex,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<u8> {
    vmad_advance(offset, 1, data.len())?; // i8 version
    let flags = vmad_read_u8_advance(data, offset)?;
    scrub_vmad_script_entry(data, offset, object_format, index, alias_ids, changed)?;
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
    index: &QuestConditionIndex,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    scrub_vmad_info_pack_fragments(data, offset, object_format, index, alias_ids, changed)?;
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
    index: &QuestConditionIndex,
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
            scrub_vmad_property_value(
                data,
                offset,
                prop_type,
                object_format,
                index,
                alias_ids,
                changed,
            )?;
        }
    }

    for _ in 0..fragment_count {
        vmad_advance(offset, 9, data.len())?;
        vmad_skip_string(data, offset)?;
        vmad_skip_string(data, offset)?;
    }

    let alias_count = vmad_read_u16_advance(data, offset)? as usize;
    for _ in 0..alias_count {
        scrub_vmad_object_alias(data, offset, object_format, index, alias_ids, changed)?;
        vmad_advance(offset, 2, data.len())?;
        let alias_obj_format = vmad_read_u16_advance(data, offset)?;
        let alias_script_count = vmad_read_u16_advance(data, offset)? as usize;
        for _ in 0..alias_script_count {
            scrub_vmad_script_entry(data, offset, alias_obj_format, index, alias_ids, changed)?;
        }
    }
    Some(())
}

fn scrub_vmad_script_entry(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    index: &QuestConditionIndex,
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
            index,
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
    index: &QuestConditionIndex,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => scrub_vmad_object_alias(data, offset, object_format, index, alias_ids, changed),
        2 => {
            vmad_skip_string(data, offset)?;
            Some(())
        }
        3 | 4 => vmad_advance(offset, 4, data.len()),
        5 => vmad_advance(offset, 1, data.len()),
        7 => scrub_vmad_struct(data, offset, object_format, index, alias_ids, changed),
        11 => {
            let count = vmad_read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                scrub_vmad_object_alias(data, offset, object_format, index, alias_ids, changed)?;
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
                scrub_vmad_struct(data, offset, object_format, index, alias_ids, changed)?;
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
    index: &QuestConditionIndex,
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
        scrub_vmad_property_value(
            data,
            offset,
            member_type,
            object_format,
            index,
            alias_ids,
            changed,
        )?;
    }
    Some(())
}

fn scrub_vmad_object_alias(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    index: &QuestConditionIndex,
    alias_ids: Option<&FxHashSet<u32>>,
    changed: &mut bool,
) -> Option<()> {
    let object_offset = *offset;
    let alias_offset = if object_format == 2 {
        object_offset.checked_add(2)?
    } else {
        object_offset.checked_add(4)?
    };
    let form_id_offset = if object_format == 2 {
        object_offset.checked_add(4)?
    } else {
        object_offset
    };
    vmad_advance(offset, 8, data.len())?;
    let alias = vmad_read_u16(data, alias_offset)?;
    if alias == VMAD_NO_ALIAS {
        return Some(());
    }
    let encoded_quest = vmad_read_u32(data, form_id_offset)?;
    let property_alias_ids = match alias_context_for_encoded_quest(encoded_quest, index) {
        AliasContext::Known { target, .. } => Some(target),
        AliasContext::Unknown => return Some(()),
        AliasContext::None => alias_ids,
    };
    if alias_id_missing_from(u32::from(alias), property_alias_ids) {
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
    use esp_authoring_core::plugin_runtime::{ParsedGroup, ParsedRecord, ParsedSubrecord};
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

    fn pack_stage_ctda(
        function_id: u16,
        parameter_1: u32,
        parameter_2: u32,
        comparison: f32,
        operator: u8,
    ) -> FieldEntry {
        let mut bytes = vec![0u8; 32];
        bytes[0] = operator << 5;
        bytes[4..8].copy_from_slice(&comparison.to_le_bytes());
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[16..20].copy_from_slice(&parameter_2.to_le_bytes());
        bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
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
            lower_source_pack_stage_conditions: true,
            lowered_source_pack_conditions_by_target: FxHashMap::default(),
            quest_alias_ids_by_encoded_quest: aliases
                .iter()
                .map(|(quest, ids)| (*quest, Some(ids.iter().copied().collect())))
                .collect(),
            source_alias_ids_by_encoded_quest: FxHashMap::default(),
            quest_stage_ids_by_encoded_quest: FxHashMap::default(),
            encoded_quest_by_form_key: FxHashMap::default(),
            info_owner_quest_by_local: FxHashMap::default(),
            valid_typed_param1_ids_by_function: FxHashMap::default(),
            valid_worldspace_ids: FxHashSet::default(),
            output_worldspace_id_by_object_id: FxHashMap::default(),
            target_masters: Vec::new(),
        }
    }

    fn quest_index_with_stages(owner: u32, stages: &[u32]) -> QuestConditionIndex {
        let mut index = quest_index(&[owner], &[]);
        index
            .quest_stage_ids_by_encoded_quest
            .insert(owner, Some(stages.iter().copied().collect()));
        index
    }

    fn quest_index_with_typed(
        valid: &[u32],
        aliases: &[(u32, &[u32])],
        typed: &[(u16, &[u32])],
    ) -> QuestConditionIndex {
        QuestConditionIndex {
            valid_quest_ids: valid.iter().copied().collect(),
            quest_index_complete: true,
            lower_source_pack_stage_conditions: true,
            lowered_source_pack_conditions_by_target: FxHashMap::default(),
            quest_alias_ids_by_encoded_quest: aliases
                .iter()
                .map(|(quest, ids)| (*quest, Some(ids.iter().copied().collect())))
                .collect(),
            source_alias_ids_by_encoded_quest: FxHashMap::default(),
            quest_stage_ids_by_encoded_quest: FxHashMap::default(),
            encoded_quest_by_form_key: FxHashMap::default(),
            info_owner_quest_by_local: FxHashMap::default(),
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
            lower_source_pack_stage_conditions: true,
            lowered_source_pack_conditions_by_target: FxHashMap::default(),
            quest_alias_ids_by_encoded_quest: FxHashMap::default(),
            source_alias_ids_by_encoded_quest: FxHashMap::default(),
            quest_stage_ids_by_encoded_quest: FxHashMap::default(),
            encoded_quest_by_form_key: FxHashMap::default(),
            info_owner_quest_by_local: FxHashMap::default(),
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
    fn collects_info_quest_owner_from_dialogue_group_topology() {
        let owner = 0x0700_1234_u32;
        let topic = 0x0700_2000;
        let info = 0x0700_2001;
        let mut dial = parsed_record("DIAL", topic, 0);
        dial.subrecords.push(ParsedSubrecord {
            signature: "QNAM".into(),
            data: Bytes::copy_from_slice(&owner.to_le_bytes()),
            semantic_type: None,
        });
        let items = vec![
            ParsedItem::Record(dial),
            ParsedItem::Group(ParsedGroup {
                label: topic.to_le_bytes(),
                group_type: 7,
                tail: Bytes::new(),
                children: vec![ParsedItem::Record(parsed_record("INFO", info, 0))],
            }),
        ];

        assert_eq!(
            collect_info_owner_quests(&items).get(&(info & 0x00FF_FFFF)),
            Some(&owner)
        );
    }

    #[test]
    fn scrub_lowers_source_shaped_get_stage_done_on_scene() {
        let interner = StringInterner::new();
        let owner = 0x0700_1234;
        let stage = 20;
        let index = quest_index_with_stages(owner, &[stage]);
        let mut rec = record_with_interner(
            "SCEN",
            0x900,
            vec![
                field("PNAM", &owner.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, stage, 0, 1.0, 0),
            ],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
            panic!("raw CTDA expected");
        };
        assert_eq!(raw_condition_parameter_1(bytes), Some(owner));
        assert_eq!(raw_condition_parameter_2(bytes), Some(stage));
    }

    #[test]
    fn scrub_uses_terminal_scene_owner_after_package_action_pnam() {
        let interner = StringInterner::new();
        let package = 0x0700_4321_u32;
        let owner = 0x0700_1234_u32;
        let stage = 410_u32;
        let index = quest_index_with_stages(owner, &[stage]);
        let mut rec = record_with_interner(
            "SCEN",
            0x40BD15,
            vec![
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, stage, 0, 1.0, 0),
                field("ANAM", &1_u16.to_le_bytes()),
                field("PNAM", &package.to_le_bytes()),
                field("ANAM", &[]),
                field("PNAM", &owner.to_le_bytes()),
            ],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[0].value else {
            panic!("raw CTDA expected");
        };
        assert_eq!(raw_condition_parameter_1(bytes), Some(owner));
        assert_eq!(raw_condition_parameter_2(bytes), Some(stage));
    }

    /// A QUST's own alias-fill gates carry `GetStageDone` with the stage in
    /// Parameter #1 and Parameter #2 = 0 — the same source shape as SCEN/INFO.
    /// `W05_MQ_001P_Wayward` (405E14) ships seven of them (stages 300/400/105/
    /// 301/302, operator 0x00, comparison 0.0) and every one was being dropped
    /// instead of lowered, taking the quest's alias gating with it.
    #[test]
    fn scrub_lowers_source_shaped_get_stage_done_on_qust() {
        let interner = StringInterner::new();
        let stage = 300;
        let mut rec = record_with_interner(
            "QUST",
            0x0040_5E14,
            vec![pack_stage_ctda(
                GET_STAGE_DONE_FUNCTION_ID,
                stage,
                0,
                0.0,
                0,
            )],
            &interner,
        );
        let owner = encode_form_id(
            &rec.form_key,
            &interner,
            &quest_index(&[], &[]).target_masters,
        )
        .expect("QUST form key must encode");
        let index = quest_index_with_stages(owner, &[stage]);

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[0].value else {
            panic!("raw CTDA expected");
        };
        assert_eq!(raw_condition_parameter_1(bytes), Some(owner));
        assert_eq!(raw_condition_parameter_2(bytes), Some(stage));
    }

    /// Every `GetStageDone` row a source QUST carries must lower, not just some.
    /// `W05_MQ_003P_Muscle` (41A39D) ships eighteen of them; a converted build
    /// retained only four, so this pins the whole set against the real bytes and
    /// the quest's real stage list.
    #[test]
    fn scrub_lowers_every_source_shaped_get_stage_done_on_a_real_qust() {
        const MUSCLE_GET_STAGE_DONE_ROWS: &[&str] = &[
            "00000000000000003B0000002C010000000000000000000000000000FFFFFFFF",
            "00000000000000003B00000090010000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000F4010000000000000000000000000000FFFFFFFF",
            "00000000000000003B00000090010000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000F4010000000000000000000000000000FFFFFFFF",
            "00000000000000003B00000084030000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000F4010000000000000000000000000000FFFFFFFF",
            "000000000000803F3B000000C6020000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000EC040000000000000000000000000000FFFFFFFF",
            "010000000000803F3B000000EC040000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000EC040000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000E7040000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000EC040000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000E8040000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000EC040000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000EC040000000000000000000000000000FFFFFFFF",
            "00000000000000003B000000B5040000000000000000000000000000FFFFFFFF",
            "00000000000000003B00000028050000000000000000000000000000FFFFFFFF",
        ];
        const MUSCLE_STAGES: &[u32] = &[
            1, 2, 3, 4, 5, 6, 7, 8, 10, 100, 103, 150, 199, 200, 300, 400, 410, 415, 450, 460, 470,
            471, 473, 475, 476, 499, 500, 550, 600, 605, 610, 615, 620, 700, 710, 715, 725, 790,
            800, 900, 998, 999, 1000, 1005, 1015, 1020, 1025, 1050, 1100, 1150, 1155, 1175, 1180,
            1200, 1203, 1205, 1210, 1220, 1221, 1222, 1223, 1224, 1225, 1226, 1227, 1228, 1229,
            1230, 1232, 1233, 1235, 1240, 1251, 1255, 1256, 1260, 1270, 1275, 1280, 1300, 1310,
            1311, 1312, 1320, 1321, 1325, 1330, 1390, 1500, 9000, 10000,
        ];

        let interner = StringInterner::new();
        let rows: Vec<Vec<u8>> = MUSCLE_GET_STAGE_DONE_ROWS
            .iter()
            .map(|hex| hex::decode(hex).unwrap())
            .collect();
        let expected_stages: Vec<u32> = rows
            .iter()
            .map(|bytes| raw_condition_parameter_1(bytes).unwrap())
            .collect();
        let mut rec = record_with_interner(
            "QUST",
            0x0041_A39D,
            rows.iter().map(|bytes| field("CTDA", bytes)).collect(),
            &interner,
        );
        let owner = encode_form_id(
            &rec.form_key,
            &interner,
            &quest_index(&[], &[]).target_masters,
        )
        .expect("QUST form key must encode");
        let index = quest_index_with_stages(owner, MUSCLE_STAGES);

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(
            rec.fields.len(),
            MUSCLE_GET_STAGE_DONE_ROWS.len(),
            "no GetStageDone row may be dropped"
        );
        for (entry, stage) in rec.fields.iter().zip(&expected_stages) {
            let FieldValue::Bytes(bytes) = &entry.value else {
                panic!("raw CTDA expected");
            };
            assert_eq!(raw_condition_parameter_1(bytes), Some(owner));
            assert_eq!(raw_condition_parameter_2(bytes), Some(*stage));
        }
    }

    #[test]
    fn scrub_lowers_source_shaped_get_stage_done_on_info() {
        let interner = StringInterner::new();
        let owner = 0x0700_1234;
        let stage = 50;
        let mut index = quest_index_with_stages(owner, &[stage]);
        index.info_owner_quest_by_local.insert(0x900, owner);
        let mut rec = record_with_interner(
            "INFO",
            0x900,
            vec![pack_stage_ctda(
                GET_STAGE_DONE_FUNCTION_ID,
                stage,
                0,
                1.0,
                0,
            )],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[0].value else {
            panic!("raw CTDA expected");
        };
        assert_eq!(raw_condition_parameter_1(bytes), Some(owner));
        assert_eq!(raw_condition_parameter_2(bytes), Some(stage));
    }

    #[test]
    fn scrub_lowers_crash_landing_quest_alias_stage_gate_on_info() {
        let interner = StringInterner::new();
        let owner = 0x0754_EB40;
        let stage = 1300;
        let mut index = quest_index_with_stages(owner, &[stage]);
        index.info_owner_quest_by_local.insert(0x55F670, owner);
        let mut rec = record_with_interner(
            "INFO",
            0x55F670,
            vec![
                field(
                    "CTDA",
                    &hex::decode(
                        "000000000000803F3602000000000000000000000100000000000000FFFFFFFF",
                    )
                    .unwrap(),
                ),
                field(
                    "CTDA",
                    &hex::decode(
                        "00000000000000003B0000001405000000000000050000000000000000000000",
                    )
                    .unwrap(),
                ),
            ],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(rec.fields.len(), 2, "the PANDORA speaker gate must survive");
        let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
            panic!("raw CTDA expected");
        };
        assert_eq!(raw_condition_parameter_1(bytes), Some(owner));
        assert_eq!(raw_condition_parameter_2(bytes), Some(stage));
        assert_eq!(raw_condition_run_on(bytes), Some(CTDA_RUN_ON_SUBJECT));
        assert_eq!(raw_condition_run_on_reference(bytes), Some(0));
        assert_eq!(raw_condition_parameter_3(bytes), Some(u32::MAX));
    }

    #[test]
    fn scrub_lowers_forging_trust_target_stage_gates_on_info() {
        let interner = StringInterner::new();
        let owner = 0x075C_70CC;
        let quest_stages = [100, 110];
        let source_condition_stages = [110, 100];
        let mut index = quest_index_with_stages(owner, &quest_stages);
        index.info_owner_quest_by_local.insert(0x5C715F, owner);
        let mut rec = record_with_interner(
            "INFO",
            0x5C715F,
            vec![
                field(
                    "CTDA",
                    &hex::decode(
                        "00000000000000003B0000006E000000000000000100000000000000FFFFFFFF",
                    )
                    .unwrap(),
                ),
                field(
                    "CTDA",
                    &hex::decode(
                        "000000000000803F3B00000064000000000000000100000000000000FFFFFFFF",
                    )
                    .unwrap(),
                ),
            ],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(rec.fields.len(), source_condition_stages.len());
        for (entry, stage) in rec.fields.iter().zip(source_condition_stages) {
            let FieldValue::Bytes(bytes) = &entry.value else {
                panic!("raw CTDA expected");
            };
            assert_eq!(raw_condition_parameter_1(bytes), Some(owner));
            assert_eq!(raw_condition_parameter_2(bytes), Some(stage));
            assert_eq!(raw_condition_run_on(bytes), Some(CTDA_RUN_ON_SUBJECT));
            assert_eq!(raw_condition_run_on_reference(bytes), Some(0));
            assert_eq!(raw_condition_parameter_3(bytes), Some(u32::MAX));
        }
    }

    #[test]
    fn scrub_does_not_lower_pack_stage_conditions_outside_fo76_to_fo4() {
        let interner = StringInterner::new();
        let owner = 0x0040_5E14;
        let stage = 530;
        let pairs = [
            (Some("fnv"), Some("fo4")),
            (Some("fo3"), Some("fo4")),
            (Some("skyrimse"), Some("fo4")),
            (Some("fo76"), Some("skyrimse")),
            (None, Some("fo4")),
        ];

        assert!(is_fo76_to_fo4_game_pair(Some("fo76"), Some("fo4")));
        for (source_game, target_game) in pairs {
            let mut index = quest_index_with_stages(owner, &[stage]);
            index.lower_source_pack_stage_conditions =
                is_fo76_to_fo4_game_pair(source_game, target_game);
            let mut rec = record_with_interner(
                "PACK",
                0x0040_BD22,
                vec![
                    field("QNAM", &owner.to_le_bytes()),
                    pack_stage_ctda(GET_STAGE_FUNCTION_ID, 0, 0, stage as f32, 0),
                    pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, stage, 0, 1.0, 0),
                ],
                &interner,
            );

            assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
            assert_eq!(
                sigs(&rec),
                vec!["QNAM"],
                "unexpected lowering for {source_game:?}->{target_game:?}"
            );
        }
    }

    #[test]
    fn scrub_lowers_exact_wayward_pack_get_stage_done_conditions() {
        let interner = StringInterner::new();
        let owner = 0x0040_5E14;
        let index = quest_index_with_stages(owner, &[450, 470, 530, 550]);
        let fixtures = [
            (
                0x0040_BD1E,
                "000000000000803F3B000000C2010000000000000000000000000000FFFFFFFF",
                450,
            ),
            (
                0x0040_BD1E,
                "00000000000000003B000000D6010000000000000000000000000000FFFFFFFF",
                470,
            ),
            (
                0x0040_BD22,
                "000000000000803F3B00944312020000000000000000000000000000FFFFFFFF",
                530,
            ),
            (
                0x0058_52E4,
                "000000000000803F3B00944326020000000000000000000000000000FFFFFFFF",
                550,
            ),
        ];

        for (local, raw_hex, stage) in fixtures {
            let source = hex::decode(raw_hex).unwrap();
            let mut rec = record_with_interner(
                "PACK",
                local,
                vec![field("QNAM", &owner.to_le_bytes()), field("CTDA", &source)],
                &interner,
            );

            assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
            let FieldValue::Bytes(lowered) = &rec.fields[1].value else {
                panic!("raw CTDA expected");
            };
            let mut expected = source;
            expected[12..16].copy_from_slice(&owner.to_le_bytes());
            expected[16..20].copy_from_slice(&(stage as u32).to_le_bytes());
            assert_eq!(lowered.as_slice(), expected);
        }
    }

    #[test]
    fn scrub_lowers_exact_mqa206_pack_get_stage_done_conditions() {
        let interner = StringInterner::new();
        let owner = 0x0054_EDB9;
        let index = quest_index_with_stages(owner, &[200, 600, 9999]);
        let fixtures = [
            (
                0x0055_8978,
                vec![
                    (
                        "000000000000803F3B009443C8000000000000000000000000000000FFFFFFFF",
                        200,
                    ),
                    (
                        "00000000000000003B0094430F270000000000000000000000000000FFFFFFFF",
                        9999,
                    ),
                ],
            ),
            (
                0x0055_8977,
                vec![
                    (
                        "000000000000803F3B009443C8000000000000000000000000000000FFFFFFFF",
                        200,
                    ),
                    (
                        "00000000000000003B0094430F270000000000000000000000000000FFFFFFFF",
                        9999,
                    ),
                ],
            ),
            (
                0x0056_74A3,
                vec![(
                    "000000000000803F3B00944358020000000000000000000000000000FFFFFFFF",
                    600,
                )],
            ),
        ];

        for (local, conditions) in fixtures {
            let mut fields = vec![field("QNAM", &owner.to_le_bytes())];
            for (raw, _) in &conditions {
                fields.push(field("CTDA", &hex::decode(raw).unwrap()));
            }
            let mut record = record_with_interner("PACK", local, fields, &interner);

            assert!(scrub_invalid_quest_references(
                &mut record,
                &index,
                &interner
            ));
            assert_eq!(
                sigs(&record),
                std::iter::once("QNAM")
                    .chain(std::iter::repeat_n("CTDA", conditions.len()))
                    .collect::<Vec<_>>()
            );
            for (entry, (_, stage)) in record.fields.iter().skip(1).zip(&conditions) {
                let FieldValue::Bytes(bytes) = &entry.value else {
                    panic!("raw CTDA expected");
                };
                assert_eq!(raw_condition_parameter_1(bytes), Some(owner));
                assert_eq!(raw_condition_parameter_2(bytes), Some(*stage));
            }
        }
    }

    #[test]
    fn scrub_lowers_every_audited_w05_settler_pack_stage_condition() {
        const FIXTURES: &[(u32, u32, &[u32], &[&str])] = &[
            (
                0x0059_7801,
                0x003F_28C3,
                &[1200, 9000],
                &[
                    "000000000000803F3B009443B004000000000000050000000000000000000000",
                    "00000000000000003B0094432823000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_0450,
                0x003F_28C3,
                &[600, 625],
                &[
                    "000000000000803F3B00944358020000000000000000000000000000FFFFFFFF",
                    "00000000000000003B00944371020000000000000000000000000000FFFFFFFF",
                ],
            ),
            (
                0x0059_0451,
                0x003F_28C3,
                &[800, 875],
                &[
                    "000000000000803F3B00944320030000000000000000000000000000FFFFFFFF",
                    "00000000000000003B0094436B030000000000000000000000000000FFFFFFFF",
                ],
            ),
            (
                0x0059_0452,
                0x003F_28C3,
                &[1000, 1025],
                &[
                    "000000000000803F3B009443E8030000000000000000000000000000FFFFFFFF",
                    "000000000000803F3B00944301040000000000000000000000000000FFFFFFFF",
                ],
            ),
            (
                0x0058_7308,
                0x003F_28C3,
                &[500, 525],
                &[
                    "000000000000803F3B009443F4010000000000000000000000000000FFFFFFFF",
                    "00000000000000003B0094430D020000000000000000000000000000FFFFFFFF",
                ],
            ),
            (
                0x0059_DB1E,
                0x0040_571C,
                &[1600, 1700],
                &[
                    "000000000000803F3B0094434006000000000000050000000000000000000000",
                    "00000000000000003B009443A406000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_DB1F,
                0x0040_571C,
                &[100, 150, 200],
                &[
                    "000000000000803F3B0094436400000000000000050000000000000000000000",
                    "00000000000000003B0094439600000000000000050000000000000000000000",
                    "00000000000000003B009443C800000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_DB20,
                0x0040_571C,
                &[100, 200],
                &[
                    "000000000000803F3B0094436400000000000000050000000000000000000000",
                    "00000000000000003B009443C800000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_DB21,
                0x0040_571C,
                &[1600, 1700],
                &[
                    "000000000000803F3B0094434006000000000000050000000000000000000000",
                    "00000000000000003B009443A406000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_DB22,
                0x0040_571C,
                &[1600, 1700],
                &[
                    "000000000000803F3B0094434006000000000000050000000000000000000000",
                    "00000000000000003B009443A406000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_DB23,
                0x0040_571C,
                &[100, 200],
                &[
                    "000000000000803F3B0094436400000000000000050000000000000000000000",
                    "00000000000000003B009443C800000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_DB24,
                0x0040_571C,
                &[1600, 1700],
                &[
                    "000000000000803F3B0094434006000000000000050000000000000000000000",
                    "00000000000000003B009443A406000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_DB25,
                0x0040_571C,
                &[100, 150],
                &[
                    "000000000000803F3B0094436400000000000000050000000000000000000000",
                    "00000000000000003B0094439600000000000000050000000000000000000000",
                ],
            ),
            (
                0x0059_7800,
                0x0040_571C,
                &[1700, 9000],
                &[
                    "000000000000803F3B009443A406000000000000050000000000000000000000",
                    "00000000000000003B0094432823000000000000050000000000000000000000",
                ],
            ),
            (
                0x005A_26E2,
                0x0040_571C,
                &[1600, 1700],
                &[
                    "000000000000803F3B0094434006000000000000050000000000000000000000",
                    "00000000000000003B009443A406000000000000050000000000000000000000",
                ],
            ),
            (
                0x005A_26E3,
                0x0040_571C,
                &[1500, 1700],
                &[
                    "000000000000803F3B009443DC05000000000000050000000000000000000000",
                    "00000000000000003B009443A406000000000000050000000000000000000000",
                ],
            ),
            (
                0x0057_4019,
                0x0040_C458,
                &[370],
                &["000000000000803F3B00944372010000000000000000000000000000FFFFFFFF"],
            ),
            (
                0x0057_0D66,
                0x0041_CB6D,
                &[1600],
                &["000000000000803F3B00944340060000000000000000000000000000FFFFFFFF"],
            ),
        ];

        let interner = StringInterner::new();
        for &(local, owner, stages, raw_conditions) in FIXTURES {
            let source_conditions: Vec<Vec<u8>> = raw_conditions
                .iter()
                .map(|raw| hex::decode(raw).unwrap())
                .collect();
            let mut fields = vec![field("QNAM", &owner.to_le_bytes())];
            fields.extend(source_conditions.iter().map(|bytes| field("CTDA", bytes)));
            let mut record = record_with_interner("PACK", local, fields, &interner);
            let index = quest_index_with_stages(owner, stages);

            assert!(
                scrub_invalid_quest_references(&mut record, &index, &interner),
                "PACK {local:06X} must lower"
            );
            assert_eq!(
                record.fields.len(),
                source_conditions.len() + 1,
                "PACK {local:06X} lost a source condition"
            );
            for (entry, source) in record.fields.iter().skip(1).zip(&source_conditions) {
                let FieldValue::Bytes(lowered) = &entry.value else {
                    panic!("PACK {local:06X} CTDA must remain raw bytes");
                };
                let stage = raw_condition_parameter_1(source).unwrap();
                let mut expected = source.clone();
                expected[12..16].copy_from_slice(&owner.to_le_bytes());
                expected[16..20].copy_from_slice(&stage.to_le_bytes());
                expected[20..24].copy_from_slice(&CTDA_RUN_ON_SUBJECT.to_le_bytes());
                expected[24..28].copy_from_slice(&0_u32.to_le_bytes());
                expected[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
                assert_eq!(
                    lowered.as_slice(),
                    expected,
                    "PACK {local:06X} stage {stage} was not lowered exactly"
                );
                assert_eq!(
                    raw_condition_function_id(lowered),
                    Some(GET_STAGE_DONE_FUNCTION_ID)
                );
                assert_eq!(raw_condition_parameter_1(lowered), Some(owner));
                assert_eq!(raw_condition_parameter_2(lowered), Some(stage));
            }
        }
    }

    #[test]
    fn source_pack_group_recovery_rejects_partial_mixed_and_global_groups() {
        let interner = StringInterner::new();
        let owner = 0x0040_5E14u32;
        let stages: FxHashSet<_> = [450, 470].into_iter().collect();
        let valid_quests: FxHashSet<_> = [owner].into_iter().collect();
        let source = record_with_interner(
            "PACK",
            0x0040_BD1E,
            vec![
                field("QNAM", &owner.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, 450, 0, 1.0, 0),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, 470, 0, 0.0, 0),
                field("ANAM", &[0x18]),
            ],
            &interner,
        );
        let groups = lowered_source_pack_condition_groups(&source, owner, &stages, &valid_quests);
        assert_eq!(groups.len(), 1);

        let mut target = record_with_interner(
            "PACK",
            0x0040_BD1E,
            vec![
                field("QNAM", &owner.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, 450, 0, 1.0, 0),
                field("ANAM", &[0x18]),
            ],
            &interner,
        );
        let mut index = quest_index_with_stages(owner, &[450, 470]);
        index.lowered_source_pack_conditions_by_target.insert(
            target.form_key,
            LoweredSourcePackConditions {
                owner_quest: owner,
                groups,
            },
        );

        assert!(scrub_invalid_quest_references(
            &mut target,
            &index,
            &interner
        ));
        assert_eq!(sigs(&target), vec!["QNAM", "CTDA", "ANAM"]);
        let surviving = target
            .fields
            .iter()
            .find_map(|entry| match (&entry.sig.0, &entry.value) {
                (b"CTDA", FieldValue::Bytes(bytes)) => Some(bytes),
                _ => None,
            })
            .unwrap();
        assert_eq!(raw_condition_parameter_1(surviving), Some(owner));
        assert_eq!(raw_condition_parameter_2(surviving), Some(450));
        let after_partial = target.fields.clone();
        assert!(!scrub_invalid_quest_references(
            &mut target,
            &index,
            &interner
        ));
        assert_eq!(target.fields, after_partial);

        let mixed = record_with_interner(
            "PACK",
            0x0040_BD22,
            vec![
                field("QNAM", &owner.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, 450, 0, 1.0, 0),
                ctda(46, 0x0000_1234),
                field("ANAM", &[0x18]),
            ],
            &interner,
        );
        assert!(
            lowered_source_pack_condition_groups(&mixed, owner, &stages, &valid_quests).is_empty()
        );

        let mut global = pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, 450, 0, 1.0, 0);
        let FieldValue::Bytes(global_bytes) = &mut global.value else {
            unreachable!();
        };
        global_bytes[0] |= CTDA_COMPARISON_GLOBAL_FLAG;
        let global_source = record_with_interner(
            "PACK",
            0x0058_52E4,
            vec![
                field("QNAM", &owner.to_le_bytes()),
                global.clone(),
                field("ANAM", &[0x18]),
            ],
            &interner,
        );
        assert!(
            lowered_source_pack_condition_groups(&global_source, owner, &stages, &valid_quests)
                .is_empty()
        );

        let direct_index = quest_index_with_stages(owner, &[450]);
        let mut global_target = global_source;
        assert!(scrub_invalid_quest_references(
            &mut global_target,
            &direct_index,
            &interner
        ));
        assert_eq!(sigs(&global_target), vec!["QNAM", "ANAM"]);
    }

    #[test]
    fn source_pack_group_recovery_preserves_multiple_group_order_and_is_idempotent() {
        let interner = StringInterner::new();
        let owner = 0x0040_5E14u32;
        let stages: FxHashSet<_> = [450, 470].into_iter().collect();
        let valid_quests: FxHashSet<_> = [owner].into_iter().collect();
        let source = record_with_interner(
            "PACK",
            0x0040_BD1E,
            vec![
                field("PKDT", &[0; 12]),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, 450, 0, 1.0, 0),
                field("PSDT", &[0; 12]),
                field("QNAM", &owner.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, 470, 0, 0.0, 0),
                field("ANAM", &[0x18]),
            ],
            &interner,
        );
        let groups = lowered_source_pack_condition_groups(&source, owner, &stages, &valid_quests);
        assert_eq!(groups.len(), 2);

        let mut target = record_with_interner(
            "PACK",
            0x0040_BD1E,
            vec![
                field("PKDT", &[0; 12]),
                field("PSDT", &[0; 12]),
                field("QNAM", &owner.to_le_bytes()),
                field("ANAM", &[0x18]),
            ],
            &interner,
        );
        let mut index = quest_index_with_stages(owner, &[450, 470]);
        index.lowered_source_pack_conditions_by_target.insert(
            target.form_key,
            LoweredSourcePackConditions {
                owner_quest: owner,
                groups,
            },
        );

        assert!(scrub_invalid_quest_references(
            &mut target,
            &index,
            &interner
        ));
        assert_eq!(
            sigs(&target),
            vec!["PKDT", "CTDA", "PSDT", "QNAM", "CTDA", "ANAM"]
        );
        let recovered_stages: Vec<_> = target
            .fields
            .iter()
            .filter_map(|entry| match (&entry.sig.0, &entry.value) {
                (b"CTDA", FieldValue::Bytes(bytes)) => {
                    Some(raw_condition_parameter_2(bytes).unwrap())
                }
                _ => None,
            })
            .collect();
        assert_eq!(recovered_stages, vec![450, 470]);

        let after_recovery = target.fields.clone();
        assert!(!scrub_invalid_quest_references(
            &mut target,
            &index,
            &interner
        ));
        assert_eq!(target.fields, after_recovery);
    }

    #[test]
    fn scrub_lowers_pack_get_stage_and_preserves_operator_comparison_and_cis_order() {
        let interner = StringInterner::new();
        let owner = 0x0040_5E14;
        let stage = 410u32;
        let index = quest_index_with_stages(owner, &[stage]);
        let untouched = ctda(46, 0x0000_1234);
        let source_stage = pack_stage_ctda(GET_STAGE_FUNCTION_ID, 0, 0, stage as f32, 3);
        let mut rec = record_with_interner(
            "PACK",
            0x0040_BD14,
            vec![
                field("CITC", &2u32.to_le_bytes()),
                field("QNAM", &owner.to_le_bytes()),
                untouched.clone(),
                field("CIS1", b"keep-first\0"),
                source_stage.clone(),
                field("CIS2", b"keep-stage\0"),
                field("FULL", b"tail\0"),
            ],
            &interner,
        );

        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(
            sigs(&rec),
            vec!["CITC", "QNAM", "CTDA", "CIS1", "CTDA", "CIS2", "FULL"]
        );
        assert_eq!(
            rec.fields[0].value,
            field("CITC", &2u32.to_le_bytes()).value
        );
        assert_eq!(rec.fields[2], untouched);
        assert_eq!(rec.fields[3], field("CIS1", b"keep-first\0"));
        assert_eq!(rec.fields[5], field("CIS2", b"keep-stage\0"));

        let FieldValue::Bytes(source_bytes) = source_stage.value else {
            panic!("raw source CTDA expected");
        };
        let FieldValue::Bytes(lowered) = &rec.fields[4].value else {
            panic!("raw lowered CTDA expected");
        };
        let mut expected = source_bytes.to_vec();
        expected[12..16].copy_from_slice(&owner.to_le_bytes());
        assert_eq!(lowered.as_slice(), expected);
        assert_eq!(raw_condition_operator(lowered), Some(3));
        assert_eq!(raw_condition_comparison_value(lowered), Some(stage as f32));
    }

    #[test]
    fn scrub_pack_stage_lowering_fails_closed_on_unproven_shapes() {
        let interner = StringInterner::new();
        let owner = 0x0040_5E14;
        let stage = 530;
        let index = quest_index_with_stages(owner, &[stage]);

        let mut missing_qnam = record_with_interner(
            "PACK",
            0x900,
            vec![pack_stage_ctda(
                GET_STAGE_DONE_FUNCTION_ID,
                stage,
                0,
                1.0,
                0,
            )],
            &interner,
        );
        assert!(scrub_invalid_quest_references(
            &mut missing_qnam,
            &index,
            &interner
        ));
        assert!(missing_qnam.fields.is_empty());

        let mut wrong_qnam = record_with_interner(
            "PACK",
            0x901,
            vec![
                field("QNAM", &0x0000_0555u32.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, stage, 0, 1.0, 0),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(
            &mut wrong_qnam,
            &index,
            &interner
        ));
        assert_eq!(sigs(&wrong_qnam), vec!["QNAM"]);

        let mut duplicate_with_invalid_qnam = record_with_interner(
            "PACK",
            0x902,
            vec![
                field("QNAM", &owner.to_le_bytes()),
                field("QNAM", &0x0000_0555u32.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, stage, 0, 1.0, 0),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(
            &mut duplicate_with_invalid_qnam,
            &index,
            &interner
        ));
        assert_eq!(sigs(&duplicate_with_invalid_qnam), vec!["QNAM", "QNAM"]);

        let mut duplicate_with_unresolved_qnam = record_with_interner(
            "PACK",
            0x903,
            vec![
                field("QNAM", &owner.to_le_bytes()),
                FieldEntry {
                    sig: SubrecordSig::from_str("QNAM").unwrap(),
                    value: FieldValue::FormKey(FormKey {
                        local: 0x0555,
                        plugin: interner.intern("Missing.esm"),
                    }),
                },
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, stage, 0, 1.0, 0),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(
            &mut duplicate_with_unresolved_qnam,
            &index,
            &interner
        ));
        assert_eq!(sigs(&duplicate_with_unresolved_qnam), vec!["QNAM", "QNAM"]);

        let mut nonexistent_stage = record_with_interner(
            "PACK",
            0x904,
            vec![
                field("QNAM", &owner.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, 999, 0, 1.0, 0),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(
            &mut nonexistent_stage,
            &index,
            &interner
        ));
        assert_eq!(sigs(&nonexistent_stage), vec!["QNAM"]);

        // A record sig with no quest context at all. SCEN/INFO/QUST/DIAL are NOT
        // valid here any more: they are quest-context sigs and their stage gates
        // lower from PNAM/QNAM/topic topology (see the `..._on_scene`/`_on_info`/
        // `_on_qust` tests). Only a sig outside that set still fails closed.
        let mut non_quest_context = record_with_interner(
            "TERM",
            0x905,
            vec![
                field("PNAM", &owner.to_le_bytes()),
                pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, stage, 0, 1.0, 0),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(
            &mut non_quest_context,
            &index,
            &interner
        ));
        assert_eq!(sigs(&non_quest_context), vec!["PNAM"]);

        let already_fo4 = pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, owner, stage, 1.0, 0);
        let mut valid = record_with_interner(
            "PACK",
            0x906,
            vec![field("QNAM", &owner.to_le_bytes()), already_fo4.clone()],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(
            &mut valid, &index, &interner
        ));
        assert_eq!(valid.fields[1], already_fo4);
    }

    #[test]
    fn scrub_normalizes_pack_stage_condition_target_metadata() {
        let interner = StringInterner::new();
        let owner = 0x0040_5E14;
        let stage = 530;
        let index = quest_index_with_stages(owner, &[stage]);

        for offset in [20usize, 24, 28] {
            let mut condition = pack_stage_ctda(GET_STAGE_DONE_FUNCTION_ID, stage, 0, 1.0, 0);
            let FieldValue::Bytes(bytes) = &mut condition.value else {
                unreachable!();
            };
            bytes[offset..offset + 4].copy_from_slice(&1u32.to_le_bytes());
            let mut rec = record_with_interner(
                "PACK",
                0x905,
                vec![field("QNAM", &owner.to_le_bytes()), condition],
                &interner,
            );
            assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
            assert_eq!(sigs(&rec), vec!["QNAM", "CTDA"], "offset {offset}");
            let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
                unreachable!();
            };
            assert_eq!(raw_condition_run_on(bytes), Some(CTDA_RUN_ON_SUBJECT));
            assert_eq!(raw_condition_run_on_reference(bytes), Some(0));
            assert_eq!(raw_condition_parameter_3(bytes), Some(u32::MAX));
        }
    }

    /// The exact 32-byte `GetIsAliasRef` CTDA carried by FO76 INFO `00D6AC`
    /// (`DebugKurtQuest02`, topic `00D5BD`): comparison 1.0, function 566,
    /// Parameter #1 = alias 9 (`TestActor`), RunOn Subject, Parameter #3 = -1.
    fn info_00d6ac_get_is_alias_ref() -> FieldEntry {
        let bytes = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x3F, 0x36, 0x02, 0x00, 0x00, 0x09, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];
        FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&bytes)),
        }
    }

    fn quest_index_with_source_aliases(
        valid: &[u32],
        target_aliases: &[(u32, &[u32])],
        source_aliases: &[(u32, &[u32])],
    ) -> QuestConditionIndex {
        let mut index = quest_index(valid, target_aliases);
        index.source_alias_ids_by_encoded_quest = source_aliases
            .iter()
            .map(|(quest, ids)| (*quest, ids.iter().copied().collect()))
            .collect();
        index
    }

    /// Regression: the owning quest's target-side alias table is read from the
    /// output plugin mid-pipeline. When it is incomplete, a valid speaker gate
    /// must survive because the source quest still declares the alias.
    #[test]
    fn scrub_keeps_info_alias_condition_the_source_quest_declares() {
        let interner = StringInterner::new();
        let owner = 0x0800_F66B;
        // Target table missing 9 and 14 (the mid-pipeline snapshot); source
        // quest DebugKurtQuest02 declares the full FO76 alias table.
        let index = quest_index_with_source_aliases(
            &[owner],
            &[(owner, &[0, 1, 3, 4, 5, 7, 8, 10, 11, 12, 13, 16])],
            &[(
                owner,
                &[
                    0, 1, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 19, 20, 21, 22, 23, 24, 25,
                ],
            )],
        );
        let mut index = index;
        index.info_owner_quest_by_local.insert(0x00_D6AC, owner);
        let mut rec = record_with_interner(
            "INFO",
            0x00_D6AC,
            vec![
                field("ENAM", &[0u8; 4]),
                info_00d6ac_get_is_alias_ref(),
                ctda(566, 14),
                field("NAM0", &[0u8]),
            ],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ENAM", "CTDA", "CTDA", "NAM0"]);
    }

    /// The source alias table came back empty (source handle/schema/decode or
    /// encoded-id resolution degraded), so there is no corroboration. Consulting it
    /// opportunistically would fall back to the target-only verdict and drop the
    /// gate; the guard must have no opinion (the `00D6AC` case).
    #[test]
    fn scrub_keeps_info_alias_condition_when_source_alias_table_is_unavailable() {
        let interner = StringInterner::new();
        let owner = 0x0800_F66B;
        let mut index = quest_index_with_source_aliases(
            &[owner],
            &[(owner, &[0, 1, 3, 4, 5, 7, 8, 10, 11, 12, 13, 16])],
            &[],
        );
        index.info_owner_quest_by_local.insert(0x00_D6AC, owner);
        let mut rec = record_with_interner(
            "INFO",
            0x00_D6AC,
            vec![
                field("ENAM", &[0u8; 4]),
                info_00d6ac_get_is_alias_ref(),
                ctda(566, 14),
                field("NAM0", &[0u8]),
            ],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ENAM", "CTDA", "CTDA", "NAM0"]);
    }

    /// Negative: an alias id that neither the target quest nor the source quest
    /// declares is a genuine dangle and is still dropped, with its CIS1.
    #[test]
    fn scrub_drops_info_alias_condition_absent_from_target_and_source() {
        let interner = StringInterner::new();
        let owner = 0x0800_F66B;
        let mut index =
            quest_index_with_source_aliases(&[owner], &[(owner, &[9, 10])], &[(owner, &[9, 10])]);
        index.info_owner_quest_by_local.insert(0x00_D6AC, owner);
        let mut rec = record_with_interner(
            "INFO",
            0x00_D6AC,
            vec![
                field("ENAM", &[0u8; 4]),
                info_00d6ac_get_is_alias_ref(),
                ctda(566, 77),
                field("CIS1", b"alias\0"),
                field("NAM0", &[0u8]),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["ENAM", "CTDA", "NAM0"]);
    }

    /// The source corroboration applies to every consumer of the shared alias
    /// guard, not just `GetIsAliasRef`: a RunOn=Quest-Alias condition naming a
    /// source-declared alias survives, one naming an unknown alias does not.
    #[test]
    fn scrub_alias_run_on_follows_source_corroboration() {
        let interner = StringInterner::new();
        let owner = 0x0800_F66B;
        let mut index =
            quest_index_with_source_aliases(&[owner], &[(owner, &[10])], &[(owner, &[9, 10])]);
        index.info_owner_quest_by_local.insert(0x00_D6AC, owner);

        let alias_run_on = |alias: u32| {
            let mut bytes = vec![0u8; 32];
            bytes[8..10].copy_from_slice(&72u16.to_le_bytes());
            bytes[20..24].copy_from_slice(&CTDA_RUN_ON_QUEST_ALIAS.to_le_bytes());
            bytes[28..32].copy_from_slice(&alias.to_le_bytes());
            FieldEntry {
                sig: SubrecordSig::from_str("CTDA").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
            }
        };

        let mut kept = record_with_interner("INFO", 0x00_D6AC, vec![alias_run_on(9)], &interner);
        assert!(!scrub_invalid_quest_references(
            &mut kept, &index, &interner
        ));
        assert_eq!(sigs(&kept), vec!["CTDA"]);

        let mut dropped =
            record_with_interner("INFO", 0x00_D6AC, vec![alias_run_on(77)], &interner);
        assert!(scrub_invalid_quest_references(
            &mut dropped,
            &index,
            &interner
        ));
        assert!(sigs(&dropped).is_empty());
    }

    #[test]
    fn scrub_keeps_bs01_radio_scene_player_alias_conditions() {
        let interner = StringInterner::new();
        let owner = 0x0800_F66B;
        let index = quest_index_with_source_aliases(&[owner], &[(owner, &[3])], &[(owner, &[3])]);
        let radio_on =
            hex::decode("000000000000803F650200000000000000000000050000000000000003000000")
                .unwrap();
        let radio_frequency =
            hex::decode("000000009A99A642660200000000000000000000050000000000000003000000")
                .unwrap();
        let mut rec = record_with_interner(
            "SCEN",
            0x5E9598,
            vec![
                field("CTDA", &radio_on),
                field("CTDA", &radio_frequency),
                field("PNAM", &owner.to_le_bytes()),
            ],
            &interner,
        );

        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(sigs(&rec), vec!["CTDA", "CTDA", "PNAM"]);
    }

    #[test]
    fn scrub_drops_qust_stage_alias_condition_missing_from_alias_table() {
        let interner = StringInterner::new();
        let self_quest = 0x0000_0800;
        // Neither the target nor the source quest declares alias 1 — a proven
        // dangle. The source row is what authorizes the drop at all.
        let index = quest_index_with_source_aliases(
            &[self_quest],
            &[(self_quest, &[2])],
            &[(self_quest, &[2])],
        );
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

    /// With the "Use Aliases" operator flag set, Parameter #1 of GetDistance /
    /// GetWithinDistance is an alias index. Validating it as a FormID dropped the
    /// row (or, worse, rewrote alias index `0x0A` into `0x8B00000A`). The flag
    /// must not exempt functions whose Parameter #1 stays a FormID.
    #[test]
    fn final_alias_pass_leaves_use_aliases_reference_parameters_verbatim() {
        let index = final_reference_index(&[0x0700_1234], &[(0x0000_000A, 0x8B00_000A)]);
        let use_aliases = |function_id: u16, parameter_1: u32| {
            let mut entry = ctda(function_id, parameter_1);
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                unreachable!();
            };
            bytes[0] |= CTDA_USE_ALIASES_FLAG;
            entry
        };
        let mut rec = record(
            "QUST",
            vec![
                field("ALST", &19u32.to_le_bytes()),
                use_aliases(1, 0x0000_000A),
                use_aliases(639, 0x0000_0002),
                field("ALED", &[]),
            ],
        );
        assert!(!repair_final_quest_alias_conditions(&mut rec, &index));
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
        assert_eq!(params, vec![0x0000_000A, 0x0000_0002]);

        // GetItemCount(47) carries the same flag with a real FormID in
        // Parameter #1 and is not a reference-parameter function, so it is
        // untouched by this pass either way.
        assert!(!condition_parameter_1_is_alias_index(
            match &use_aliases(47, 0x004F_54A8).value {
                FieldValue::Bytes(bytes) => bytes.as_slice(),
                _ => unreachable!(),
            }
        ));
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
    fn scrub_preserves_pack_vmad_alias_when_owner_is_not_quest() {
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
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
            panic!("VMAD should remain bytes");
        };
        assert_eq!(alias_at(bytes, alias_offset), 2);
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
    fn scrub_preserves_pack_fragment_alias_bound_to_a_different_quest() {
        let interner = StringInterner::new();
        let owner_quest = 0x0000_0800;
        let bound_quest = 0x0000_0900;
        let index = quest_index(
            &[owner_quest, bound_quest],
            &[(owner_quest, &[2]), (bound_quest, &[23, 24, 25])],
        );
        let (vmad, alias_offset) = info_fragment_vmad_object_property(25, bound_quest);
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
        assert_eq!(alias_at(bytes, alias_offset), 25);
        assert_eq!(form_id_at(bytes, alias_offset), bound_quest);
    }

    #[test]
    fn scrub_ff08_shutdown_pack_fragment_preserves_unowned_alias() {
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

        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        let FieldValue::Bytes(bytes) = &rec.fields[1].value else {
            panic!("VMAD should remain bytes");
        };
        assert_eq!(alias_at(bytes, alias_offset), 10);
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

    fn ptda_field(type_value: i32, target: u32) -> FieldEntry {
        let mut b = Vec::with_capacity(12);
        b.extend_from_slice(&type_value.to_le_bytes());
        b.extend_from_slice(&target.to_le_bytes());
        b.extend_from_slice(&1i32.to_le_bytes()); // Count / Distance
        field("PTDA", &b)
    }

    fn pldt_field(sig: &str, type_value: i32, location: u32) -> FieldEntry {
        let mut b = Vec::with_capacity(16);
        b.extend_from_slice(&type_value.to_le_bytes());
        b.extend_from_slice(&location.to_le_bytes());
        b.extend_from_slice(&(-1i32).to_le_bytes()); // Radius
        b.extend_from_slice(&0u32.to_le_bytes()); // Collection Index
        field(sig, &b)
    }

    fn union_type_and_value(rec: &Record, sig: &str) -> (i32, u32) {
        let f = rec.fields.iter().find(|f| f.sig.as_str() == sig).unwrap();
        let FieldValue::Bytes(b) = &f.value else {
            panic!("{sig} bytes");
        };
        (
            i32::from_le_bytes(b[0..4].try_into().unwrap()),
            u32::from_le_bytes(b[4..8].try_into().unwrap()),
        )
    }

    #[test]
    fn pack_alias_target_preserved_when_owner_quest_has_alias() {
        // The reported 6C2DAF case: a Ref-Alias package target whose owning quest
        // converted WITH that alias — must survive translation intact.
        let interner = StringInterner::new();
        let owner_quest = 0x0000_0800;
        let index = quest_index(&[owner_quest], &[(owner_quest, &[18])]);
        let mut rec = record_with_interner(
            "PACK",
            0x900,
            vec![
                field("QNAM", &owner_quest.to_le_bytes()),
                ptda_field(4, 18),         // target: Ref Alias 18
                pldt_field("PLDT", 8, 18), // location: Ref Alias 18
            ],
            &interner,
        );
        assert!(!scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(union_type_and_value(&rec, "PTDA"), (4, 18));
        assert_eq!(union_type_and_value(&rec, "PLDT"), (8, 18));
    }

    #[test]
    fn pack_alias_target_severed_when_owner_quest_lacks_alias() {
        // Owning quest converted, but this alias index was dropped during QUST
        // conversion — a proven dangle → neutralize to the benign non-alias kind.
        let interner = StringInterner::new();
        let owner_quest = 0x0000_0800;
        let index = quest_index_with_source_aliases(
            &[owner_quest],
            &[(owner_quest, &[2])],
            &[(owner_quest, &[2])],
        );
        let mut rec = record_with_interner(
            "PACK",
            0x900,
            vec![
                field("QNAM", &owner_quest.to_le_bytes()),
                ptda_field(4, 18),
                pldt_field("PLVD", 14, 18),
            ],
            &interner,
        );
        assert!(scrub_invalid_quest_references(&mut rec, &index, &interner));
        assert_eq!(union_type_and_value(&rec, "PTDA"), (6, 0)); // Self
        assert_eq!(union_type_and_value(&rec, "PLVD"), (2, 0)); // Near Package Start
    }

    #[test]
    fn pack_alias_target_preserved_when_owner_context_unresolvable() {
        // Unknown (quest present, alias table not indexed) and None (QNAM not a
        // known quest) both preserve — we cannot prove the alias dangles.
        let interner = StringInterner::new();
        let owner_quest = 0x0000_0800;
        let mut unknown = quest_index(&[owner_quest], &[]);
        unknown
            .quest_alias_ids_by_encoded_quest
            .insert(owner_quest, None);
        let none = quest_index(&[owner_quest], &[(owner_quest, &[2])]);

        for (index, qnam) in [(&unknown, owner_quest), (&none, 0x0000_0555u32)] {
            let mut rec = record_with_interner(
                "PACK",
                0x900,
                vec![
                    field("QNAM", &qnam.to_le_bytes()),
                    ptda_field(4, 18),
                    pldt_field("PLDT", 8, 18),
                ],
                &interner,
            );
            assert!(!scrub_invalid_quest_references(&mut rec, index, &interner));
            assert_eq!(union_type_and_value(&rec, "PTDA"), (4, 18));
            assert_eq!(union_type_and_value(&rec, "PLDT"), (8, 18));
        }
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
