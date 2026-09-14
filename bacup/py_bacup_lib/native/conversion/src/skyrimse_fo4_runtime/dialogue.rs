use std::collections::{HashMap, HashSet};

use smallvec::SmallVec;

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::sym::{StringInterner, Sym};

const SKYRIM_GET_VM_QUEST_VARIABLE: u16 = 629;
const FO4_GET_VM_QUEST_VARIABLE: u16 = 629;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimDialogueTopologyKind {
    TopLevel,
    QuestChild,
    TopicChild,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueMappingReceipt {
    pub source: FormKey,
    pub target: FormKey,
    pub signature: SigCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueTopologyReceipt {
    pub source_parent: FormKey,
    pub source_child: FormKey,
    pub target_parent: FormKey,
    pub target_child: FormKey,
    pub kind: SkyrimDialogueTopologyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueSubtypeCapabilityReceipt {
    pub source_topic: FormKey,
    pub target_topic: FormKey,
    pub source_category: u8,
    pub source_subtype: u16,
    pub target_category: u8,
    pub target_subtype: u16,
    pub subtype_name: [u8; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimDialogueConditionParameterKind {
    Form,
    Alias,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueConditionCapabilityReceipt {
    pub source_info: FormKey,
    pub target_info: FormKey,
    pub condition_index: usize,
    pub source_function: u16,
    pub target_function: u16,
    pub parameter_kind: SkyrimDialogueConditionParameterKind,
    pub run_on: u32,
    pub vm_variable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueVoiceIntent {
    pub source_info: FormKey,
    pub target_info: FormKey,
    pub source_speaker: FormKey,
    pub target_speaker: FormKey,
    pub response_number: u8,
    pub transcript: String,
}

pub type SkyrimDialogueVoiceRequirement = SkyrimDialogueVoiceIntent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueFragmentIntent {
    pub source_info: FormKey,
    pub source_fields: Vec<SubrecordSig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkyrimDialogueLocalizedTable {
    Strings,
    IlStrings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyrimDialogueLocalizationIntent {
    pub source_record: FormKey,
    pub target_record: FormKey,
    pub signature: SubrecordSig,
    pub table: SkyrimDialogueLocalizedTable,
    pub ordinal: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimDialogueRecordReceipt {
    pub signature: SigCode,
    pub form_key: FormKey,
    pub editor_id: Option<Sym>,
    pub flags: RecordFlags,
    pub fields: Vec<FieldEntry>,
    pub warnings: Vec<Sym>,
}

impl SkyrimDialogueRecordReceipt {
    fn from_record(record: &Record) -> Self {
        Self {
            signature: record.sig,
            form_key: record.form_key,
            editor_id: record.eid,
            flags: record.flags,
            fields: record.fields.iter().cloned().collect(),
            warnings: record.warnings.iter().copied().collect(),
        }
    }

    fn matches(&self, record: &Record) -> bool {
        self.signature == record.sig
            && self.form_key == record.form_key
            && self.editor_id == record.eid
            && self.flags == record.flags
            && self.fields.as_slice() == record.fields.as_slice()
            && self.warnings.as_slice() == record.warnings.as_slice()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyrimDialogueReceipt {
    pub mappings: Vec<SkyrimDialogueMappingReceipt>,
    pub topology: Vec<SkyrimDialogueTopologyReceipt>,
    pub subtype_capabilities: Vec<SkyrimDialogueSubtypeCapabilityReceipt>,
    pub condition_capabilities: Vec<SkyrimDialogueConditionCapabilityReceipt>,
    pub voice_intents: Vec<SkyrimDialogueVoiceIntent>,
    pub fragment_intents: Vec<SkyrimDialogueFragmentIntent>,
    pub localization_intents: Vec<SkyrimDialogueLocalizationIntent>,
    pub expected_records: Vec<SkyrimDialogueRecordReceipt>,
    pub unsupported_reason: Option<String>,
}

impl SkyrimDialogueReceipt {
    pub fn validate_materialized(
        &self,
        records: &[Record],
        topic_parent_by_info: &HashMap<FormKey, FormKey>,
    ) -> Result<(), String> {
        if let Some(reason) = &self.unsupported_reason {
            return Err(format!("unsupported Skyrim dialogue component: {reason}"));
        }
        if records.len() != self.expected_records.len() {
            return Err(format!(
                "dialogue record inventory changed: expected {}, got {}",
                self.expected_records.len(),
                records.len()
            ));
        }
        let mut actual = HashMap::new();
        for record in records {
            if actual.insert(record.form_key, record).is_some() {
                return Err(format!(
                    "duplicate materialized dialogue record {:06X}",
                    record.form_key.local
                ));
            }
        }
        for expected in &self.expected_records {
            let record = actual.get(&expected.form_key).ok_or_else(|| {
                format!(
                    "materialized dialogue record {:06X} is missing",
                    expected.form_key.local
                )
            })?;
            if !expected.matches(record) {
                return Err(format!(
                    "materialized dialogue record {:06X} fields changed",
                    expected.form_key.local
                ));
            }
        }

        let selected_topics = self
            .expected_records
            .iter()
            .filter(|record| record.signature.0 == *b"DIAL")
            .map(|record| record.form_key)
            .collect::<HashSet<_>>();
        let expected_info_edges = self
            .topology
            .iter()
            .filter(|edge| edge.kind == SkyrimDialogueTopologyKind::TopicChild)
            .map(|edge| (edge.target_child, edge.target_parent))
            .collect::<HashMap<_, _>>();
        for (info, parent) in &expected_info_edges {
            if topic_parent_by_info.get(info) != Some(parent) {
                return Err(format!(
                    "materialized INFO {:06X} parent changed",
                    info.local
                ));
            }
        }
        if topic_parent_by_info.iter().any(|(info, parent)| {
            selected_topics.contains(parent) && !expected_info_edges.contains_key(info)
        }) {
            return Err("materialized dialogue acquired an extra INFO child".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SkyrimDialoguePlan {
    source_records: Vec<Record>,
    source_topic_children: HashMap<FormKey, Vec<FormKey>>,
    fragment_intents: Vec<SkyrimDialogueFragmentIntent>,
    unsupported_reason: Option<String>,
}

impl SkyrimDialoguePlan {
    pub fn derive(
        source_records: &[Record],
        source_topic_children: &HashMap<FormKey, Vec<FormKey>>,
        interner: &StringInterner,
    ) -> Result<Self, String> {
        let source_plugin = source_records
            .first()
            .map(|record| record.form_key.plugin)
            .ok_or("cannot plan an empty Skyrim dialogue component")?;
        let mut records = source_records.to_vec();
        records.sort_by_key(|record| {
            (
                signature_rank(record.sig),
                form_key_sort_key(record.form_key, interner),
            )
        });
        let fragment_intents = collect_fragment_intents(&records);
        let unsupported_reason =
            validate_source_component(&records, source_topic_children, source_plugin, interner)
                .err();
        Ok(Self {
            source_records: records,
            source_topic_children: source_topic_children.clone(),
            fragment_intents,
            unsupported_reason,
        })
    }

    pub fn unsupported_reason(&self) -> Option<&str> {
        self.unsupported_reason.as_deref()
    }

    pub fn preflight_receipt(&self) -> SkyrimDialogueReceipt {
        SkyrimDialogueReceipt {
            mappings: Vec::new(),
            topology: Vec::new(),
            subtype_capabilities: Vec::new(),
            condition_capabilities: Vec::new(),
            voice_intents: Vec::new(),
            fragment_intents: self.fragment_intents.clone(),
            localization_intents: Vec::new(),
            expected_records: Vec::new(),
            unsupported_reason: self.unsupported_reason.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SkyrimProjectedDialogueInfo {
    pub parent_topic: FormKey,
    pub record: Record,
}

#[derive(Debug, Clone)]
pub struct SkyrimDialogueProjection {
    pub top_level: Vec<Record>,
    pub quest_children: Vec<Record>,
    pub topic_children: Vec<SkyrimProjectedDialogueInfo>,
    pub receipt: SkyrimDialogueReceipt,
}

pub fn project_dialogue(
    plan: &SkyrimDialoguePlan,
    target_by_source: &HashMap<FormKey, FormKey>,
    fallout4_master: Sym,
    interner: &StringInterner,
) -> Result<SkyrimDialogueProjection, String> {
    if let Some(reason) = &plan.unsupported_reason {
        return Err(format!("unsupported Skyrim dialogue component: {reason}"));
    }
    let mut mapped_dialogue_records = HashSet::new();
    for source in &plan.source_records {
        let target = mapped(target_by_source, source.form_key)?;
        if !mapped_dialogue_records.insert(target) {
            return Err("dialogue target mapping is not one-to-one".to_string());
        }
    }
    let mut top_level = Vec::new();
    let mut quest_children = Vec::new();
    let mut topic_children = Vec::new();
    let mut mappings = Vec::new();
    let mut topology = Vec::new();
    let mut subtype_capabilities = Vec::new();
    let mut condition_capabilities = Vec::new();
    let mut voice_intents = Vec::new();
    let mut localization_intents = Vec::new();

    for source in &plan.source_records {
        let target = match source.sig.0 {
            sig if sig == *b"DLVW" => lower_view(source, target_by_source)?,
            sig if sig == *b"DLBR" => lower_branch(source, target_by_source)?,
            sig if sig == *b"DIAL" => lower_topic(
                source,
                plan.source_topic_children
                    .get(&source.form_key)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                target_by_source,
                interner,
                &mut subtype_capabilities,
                &mut localization_intents,
            )?,
            sig if sig == *b"INFO" => {
                let source_parent =
                    source_parent_for_info(source.form_key, &plan.source_topic_children)?;
                let target_parent = mapped(target_by_source, source_parent)?;
                let lowered = lower_info(
                    source,
                    target_by_source,
                    fallout4_master,
                    interner,
                    &mut condition_capabilities,
                    &mut voice_intents,
                    &mut localization_intents,
                )?;
                topic_children.push(SkyrimProjectedDialogueInfo {
                    parent_topic: target_parent,
                    record: lowered.clone(),
                });
                topology.push(SkyrimDialogueTopologyReceipt {
                    source_parent,
                    source_child: source.form_key,
                    target_parent,
                    target_child: lowered.form_key,
                    kind: SkyrimDialogueTopologyKind::TopicChild,
                });
                lowered
            }
            _ => unreachable!("source component validation gates signatures"),
        };
        mappings.push(SkyrimDialogueMappingReceipt {
            source: source.form_key,
            target: target.form_key,
            signature: source.sig,
        });
        if source.sig.0 == *b"DLVW" {
            let source_owner = exactly_one_form_key(source, b"QNAM")?;
            topology.push(SkyrimDialogueTopologyReceipt {
                source_parent: source_owner,
                source_child: source.form_key,
                target_parent: mapped(target_by_source, source_owner)?,
                target_child: target.form_key,
                kind: SkyrimDialogueTopologyKind::TopLevel,
            });
            top_level.push(target);
        } else if source.sig.0 != *b"INFO" {
            let source_owner = exactly_one_form_key(source, b"QNAM")?;
            topology.push(SkyrimDialogueTopologyReceipt {
                source_parent: source_owner,
                source_child: source.form_key,
                target_parent: mapped(target_by_source, source_owner)?,
                target_child: target.form_key,
                kind: SkyrimDialogueTopologyKind::QuestChild,
            });
            quest_children.push(target);
        }
    }

    mappings.sort_by_key(|mapping| form_key_sort_key(mapping.source, interner));
    topology.sort_by_key(|edge| {
        (
            form_key_sort_key(edge.target_parent, interner),
            form_key_sort_key(edge.target_child, interner),
        )
    });
    subtype_capabilities.sort_by_key(|row| form_key_sort_key(row.target_topic, interner));
    condition_capabilities.sort_by_key(|row| {
        (
            form_key_sort_key(row.target_info, interner),
            row.condition_index,
        )
    });
    voice_intents.sort_by_key(|row| {
        (
            form_key_sort_key(row.target_info, interner),
            row.response_number,
        )
    });
    localization_intents.sort_by_key(|row| {
        (
            form_key_sort_key(row.target_record, interner),
            row.signature.0,
            row.ordinal,
        )
    });
    quest_children.sort_by_key(|record| {
        (
            signature_rank(record.sig),
            form_key_sort_key(record.form_key, interner),
        )
    });
    top_level.sort_by_key(|record| form_key_sort_key(record.form_key, interner));
    topic_children.sort_by_key(|child| form_key_sort_key(child.record.form_key, interner));

    let expected_records = top_level
        .iter()
        .chain(quest_children.iter())
        .chain(topic_children.iter().map(|child| &child.record))
        .map(SkyrimDialogueRecordReceipt::from_record)
        .collect();
    let receipt = SkyrimDialogueReceipt {
        mappings,
        topology,
        subtype_capabilities,
        condition_capabilities,
        voice_intents,
        fragment_intents: Vec::new(),
        localization_intents,
        expected_records,
        unsupported_reason: None,
    };
    Ok(SkyrimDialogueProjection {
        top_level,
        quest_children,
        topic_children,
        receipt,
    })
}

fn validate_source_component(
    records: &[Record],
    topology: &HashMap<FormKey, Vec<FormKey>>,
    source_plugin: Sym,
    interner: &StringInterner,
) -> Result<(), String> {
    let mut by_key = HashMap::new();
    for record in records {
        if record.form_key.plugin != source_plugin {
            return Err("dialogue component crosses source plugins".to_string());
        }
        if !matches!(record.sig.0, sig if sig == *b"DLVW" || sig == *b"DLBR" || sig == *b"DIAL" || sig == *b"INFO")
        {
            return Err(format!(
                "unsupported dialogue signature {}",
                record.sig.as_str()
            ));
        }
        validate_supported_field_inventory(record)?;
        if by_key.insert(record.form_key, record).is_some() {
            return Err(format!(
                "duplicate dialogue record {:06X}",
                record.form_key.local
            ));
        }
    }
    let topics = records
        .iter()
        .filter(|record| record.sig.0 == *b"DIAL")
        .map(|record| record.form_key)
        .collect::<HashSet<_>>();
    let infos = records
        .iter()
        .filter(|record| record.sig.0 == *b"INFO")
        .map(|record| record.form_key)
        .collect::<HashSet<_>>();
    if topology.keys().copied().collect::<HashSet<_>>() != topics {
        return Err("dialogue topic topology does not exactly cover DIAL records".to_string());
    }
    let mut observed_infos = HashSet::new();
    for (topic, children) in topology {
        if children.iter().any(|child| !infos.contains(child)) {
            return Err(format!(
                "DIAL {:06X} has an out-of-component INFO child",
                topic.local
            ));
        }
        for child in children {
            if !observed_infos.insert(*child) {
                return Err(format!("INFO {:06X} has multiple parents", child.local));
            }
        }
    }
    if observed_infos != infos {
        return Err("dialogue topology does not parent every INFO exactly once".to_string());
    }

    for record in records {
        match record.sig.0 {
            sig if sig == *b"DLVW" => {
                require_unique_fields(record, &[b"ENAM", b"DNAM"])?;
                exactly_one_form_key(record, b"QNAM")?;
                for branch in all_form_keys(record, b"BNAM")? {
                    require_signature(&by_key, branch, b"DLBR")?;
                }
            }
            sig if sig == *b"DLBR" => {
                require_unique_fields(record, &[b"TNAM", b"DNAM"])?;
                exactly_one_form_key(record, b"QNAM")?;
                require_signature(&by_key, exactly_one_form_key(record, b"SNAM")?, b"DIAL")?;
            }
            sig if sig == *b"DIAL" => {
                exactly_one_form_key(record, b"QNAM")?;
                require_unique_fields(record, &[b"DATA", b"SNAM", b"TIFC"])?;
                require_at_most_one(record, b"FULL")?;
                require_at_most_one(record, b"PNAM")?;
                optional_text_field(record, b"FULL", interner)?;
                let branches = all_form_keys(record, b"BNAM")?;
                if branches.len() > 1 {
                    return Err(format!(
                        "DIAL {:06X} has multiple branches",
                        record.form_key.local
                    ));
                }
                if let Some(branch) = branches.first() {
                    require_signature(&by_key, *branch, b"DLBR")?;
                }
                let (_, category, subtype) = dial_data(record, interner).ok_or_else(|| {
                    format!("DIAL {:06X} has malformed DATA", record.form_key.local)
                })?;
                let (_, _, expected_name) = map_topic_subtype(category, subtype)?;
                if four_byte_field(record, b"SNAM")? != expected_name {
                    return Err(format!(
                        "DIAL {:06X} subtype name disagrees with its semantic subtype",
                        record.form_key.local
                    ));
                }
                let expected_children = topology[&record.form_key].len() as u64;
                if scalar_field(record, b"TIFC", Some("info_count"), interner)
                    != Some(expected_children)
                {
                    return Err(format!(
                        "DIAL {:06X} INFO count disagrees with topology",
                        record.form_key.local
                    ));
                }
            }
            sig if sig == *b"INFO" => {
                if exactly_one_form_key(record, b"ANAM")?.local == 0 {
                    return Err(format!(
                        "INFO {:06X} has no concrete speaker",
                        record.form_key.local
                    ));
                }
                if !all_form_keys(record, b"TPIC")?.is_empty()
                    && !all_form_keys(record, b"TCLT")?.is_empty()
                {
                    return Err(format!(
                        "INFO {:06X} mixes previous-topic and linked-topic layouts",
                        record.form_key.local
                    ));
                }
                require_exactly_one_of(record, &[*b"DATA", *b"ENAM"])?;
                require_at_most_one(record, b"PNAM")?;
                require_at_most_one(record, b"DNAM")?;
                require_at_most_one(record, b"RNAM")?;
                optional_text_field(record, b"RNAM", interner)?;
                for link in all_form_keys(record, b"TPIC")?
                    .into_iter()
                    .chain(all_form_keys(record, b"TCLT")?)
                {
                    require_signature(&by_key, link, b"DIAL")?;
                }
                let responses = parse_source_responses(record, source_plugin, interner)?;
                if responses.is_empty()
                    || responses
                        .iter()
                        .enumerate()
                        .any(|(index, response)| response.number as usize != index + 1)
                {
                    return Err(format!(
                        "INFO {:06X} response numbering is unsupported",
                        record.form_key.local
                    ));
                }
                for scope in condition_scopes(record)? {
                    parse_source_condition(&scope, source_plugin, interner)?;
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn require_signature(
    records: &HashMap<FormKey, &Record>,
    form_key: FormKey,
    signature: &[u8; 4],
) -> Result<(), String> {
    if records.get(&form_key).map(|record| record.sig.0) != Some(*signature) {
        return Err(format!(
            "dialogue link {:06X} does not resolve to {}",
            form_key.local,
            std::str::from_utf8(signature).unwrap()
        ));
    }
    Ok(())
}

fn collect_fragment_intents(records: &[Record]) -> Vec<SkyrimDialogueFragmentIntent> {
    records
        .iter()
        .filter(|record| record.sig.0 == *b"INFO")
        .filter_map(|record| {
            let mut source_fields = record
                .fields
                .iter()
                .filter(|field| {
                    matches!(field.sig.0, sig if sig == *b"VMAD" || sig == *b"SCHR" || sig == *b"QNAM" || sig == *b"NEXT")
                })
                .map(|field| field.sig)
                .collect::<Vec<_>>();
            source_fields.sort_by_key(|signature| signature.0);
            source_fields.dedup();
            (!source_fields.is_empty()).then_some(SkyrimDialogueFragmentIntent {
                source_info: record.form_key,
                source_fields,
            })
        })
        .collect()
}

fn validate_supported_field_inventory(record: &Record) -> Result<(), String> {
    if !record.warnings.is_empty() {
        return Err(format!(
            "{} {:06X} has source decode warnings",
            record.sig.as_str(),
            record.form_key.local
        ));
    }
    if record.sig.0 == *b"INFO"
        && record.fields.iter().any(|field| {
            matches!(field.sig.0, sig if sig == *b"VMAD" || sig == *b"SCHR" || sig == *b"QNAM" || sig == *b"NEXT")
        })
    {
        return Err(format!(
            "INFO {:06X} requires unsupported fragment semantics",
            record.form_key.local
        ));
    }
    if record.sig.0 == *b"INFO"
        && record
            .fields
            .iter()
            .any(|field| matches!(field.sig.0, sig if sig == *b"ONAM" || sig == *b"TWAT"))
    {
        return Err(format!(
            "INFO {:06X} requires unsupported sound/voice semantics",
            record.form_key.local
        ));
    }
    let supported: &[[u8; 4]] = match record.sig.0 {
        sig if sig == *b"DLVW" => &[*b"EDID", *b"QNAM", *b"BNAM", *b"ENAM", *b"DNAM"],
        sig if sig == *b"DLBR" => &[*b"EDID", *b"QNAM", *b"TNAM", *b"DNAM", *b"SNAM"],
        sig if sig == *b"DIAL" => &[
            *b"EDID", *b"FULL", *b"PNAM", *b"BNAM", *b"QNAM", *b"DATA", *b"SNAM", *b"TIFC",
        ],
        sig if sig == *b"INFO" => &[
            *b"EDID", *b"DATA", *b"ENAM", *b"TPIC", *b"PNAM", *b"TCLT", *b"DNAM", *b"TRDT",
            *b"NAM1", *b"NAM2", *b"NAM3", *b"SNAM", *b"LNAM", *b"CTDA", *b"CIS1", *b"CIS2",
            *b"RNAM", *b"ANAM",
        ],
        _ => &[],
    };
    if let Some(field) = record
        .fields
        .iter()
        .find(|field| !supported.contains(&field.sig.0))
    {
        return Err(format!(
            "{} {:06X} contains unsupported field {}",
            record.sig.as_str(),
            record.form_key.local,
            field.sig.as_str()
        ));
    }
    Ok(())
}

fn require_exactly_one_of(record: &Record, signatures: &[[u8; 4]]) -> Result<(), String> {
    let count = record
        .fields
        .iter()
        .filter(|field| signatures.contains(&field.sig.0))
        .count();
    if count != 1 {
        return Err(format!(
            "{} {:06X} must contain exactly one source data layout",
            record.sig.as_str(),
            record.form_key.local
        ));
    }
    Ok(())
}

fn require_at_most_one(record: &Record, signature: &[u8; 4]) -> Result<(), String> {
    if record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *signature)
        .count()
        > 1
    {
        return Err(format!(
            "{} {:06X} has duplicate {} fields",
            record.sig.as_str(),
            record.form_key.local,
            std::str::from_utf8(signature).unwrap()
        ));
    }
    Ok(())
}

fn lower_view(
    source: &Record,
    target_by_source: &HashMap<FormKey, FormKey>,
) -> Result<Record, String> {
    let mut target = base_target(source, target_by_source)?;
    push_editor_id(&mut target, source.eid);
    target.fields.push(form_key_field(
        b"QNAM",
        mapped(target_by_source, exactly_one_form_key(source, b"QNAM")?)?,
    ));
    for branch in all_form_keys(source, b"BNAM")? {
        target
            .fields
            .push(form_key_field(b"BNAM", mapped(target_by_source, branch)?));
    }
    copy_exact_fields(source, &mut target, &[b"ENAM", b"DNAM"]);
    require_unique_fields(&target, &[b"ENAM", b"DNAM"])?;
    Ok(target)
}

fn lower_branch(
    source: &Record,
    target_by_source: &HashMap<FormKey, FormKey>,
) -> Result<Record, String> {
    let mut target = base_target(source, target_by_source)?;
    push_editor_id(&mut target, source.eid);
    target.fields.push(form_key_field(
        b"QNAM",
        mapped(target_by_source, exactly_one_form_key(source, b"QNAM")?)?,
    ));
    copy_exact_fields(source, &mut target, &[b"TNAM", b"DNAM"]);
    require_unique_fields(&target, &[b"TNAM", b"DNAM"])?;
    target.fields.push(form_key_field(
        b"SNAM",
        mapped(target_by_source, exactly_one_form_key(source, b"SNAM")?)?,
    ));
    Ok(target)
}

fn lower_topic(
    source: &Record,
    children: &[FormKey],
    target_by_source: &HashMap<FormKey, FormKey>,
    interner: &StringInterner,
    capabilities: &mut Vec<SkyrimDialogueSubtypeCapabilityReceipt>,
    localization: &mut Vec<SkyrimDialogueLocalizationIntent>,
) -> Result<Record, String> {
    let mut target = base_target(source, target_by_source)?;
    push_editor_id(&mut target, source.eid);
    copy_exact_fields(source, &mut target, &[b"FULL", b"PNAM"]);
    if let Some(branch) = all_form_keys(source, b"BNAM")?.first() {
        target
            .fields
            .push(form_key_field(b"BNAM", mapped(target_by_source, *branch)?));
    }
    target.fields.push(form_key_field(
        b"QNAM",
        mapped(target_by_source, exactly_one_form_key(source, b"QNAM")?)?,
    ));
    let (flags, category, source_subtype) = dial_data(source, interner).unwrap();
    let (target_category, target_subtype, subtype_name) =
        map_topic_subtype(category, source_subtype)?;
    let source_name = four_byte_field(source, b"SNAM")?;
    if source_name != subtype_name {
        return Err(format!(
            "DIAL {:06X} subtype name disagrees with its capability mapping",
            source.form_key.local
        ));
    }
    target.fields.push(field(
        b"DATA",
        FieldValue::Struct(vec![
            (interner.intern("topic_flags"), FieldValue::Uint(flags)),
            (
                interner.intern("category"),
                FieldValue::Uint(target_category),
            ),
            (interner.intern("subtype"), FieldValue::Uint(target_subtype)),
        ]),
    ));
    target.fields.push(field(
        b"SNAM",
        FieldValue::Bytes(SmallVec::from_slice(&subtype_name)),
    ));
    target
        .fields
        .push(field(b"TIFC", FieldValue::Uint(children.len() as u64)));
    capabilities.push(SkyrimDialogueSubtypeCapabilityReceipt {
        source_topic: source.form_key,
        target_topic: target.form_key,
        source_category: category as u8,
        source_subtype: source_subtype as u16,
        target_category: target_category as u8,
        target_subtype: target_subtype as u16,
        subtype_name,
    });
    if target.fields.iter().any(|field| field.sig.0 == *b"FULL") {
        localization.push(SkyrimDialogueLocalizationIntent {
            source_record: source.form_key,
            target_record: target.form_key,
            signature: SubrecordSig(*b"FULL"),
            table: SkyrimDialogueLocalizedTable::Strings,
            ordinal: 0,
        });
    }
    Ok(target)
}

fn lower_info(
    source: &Record,
    target_by_source: &HashMap<FormKey, FormKey>,
    fallout4_master: Sym,
    interner: &StringInterner,
    capabilities: &mut Vec<SkyrimDialogueConditionCapabilityReceipt>,
    voice: &mut Vec<SkyrimDialogueVoiceIntent>,
    localization: &mut Vec<SkyrimDialogueLocalizationIntent>,
) -> Result<Record, String> {
    let mut target = base_target(source, target_by_source)?;
    push_editor_id(&mut target, source.eid);
    let flags = source_info_flags(source, interner).ok_or_else(|| {
        format!(
            "INFO {:06X} has malformed response flags",
            source.form_key.local
        )
    })?;
    target.fields.push(field(
        b"ENAM",
        FieldValue::Bytes(SmallVec::from_slice(&[
            flags as u8,
            (flags >> 8) as u8,
            0,
            0,
        ])),
    ));
    for link in all_form_keys(source, b"TPIC")?
        .into_iter()
        .chain(all_form_keys(source, b"TCLT")?)
    {
        target
            .fields
            .push(form_key_field(b"TPIC", mapped(target_by_source, link)?));
    }
    if let Some(previous) = source.fields.iter().find(|field| field.sig.0 == *b"PNAM") {
        if let Some(previous) = first_form_key(&previous.value) {
            let value = if previous.local == 0 {
                FieldValue::Uint(0)
            } else {
                FieldValue::FormKey(mapped(target_by_source, previous)?)
            };
            target.fields.push(field(b"PNAM", value));
        }
    }
    if let Some(shared) = optional_form_key(source, b"DNAM")? {
        target
            .fields
            .push(form_key_field(b"DNAM", mapped(target_by_source, shared)?));
    }
    let prompt = optional_text_field(source, b"RNAM", interner)?;

    let source_speaker = exactly_one_form_key(source, b"ANAM")?;
    let target_speaker = mapped(target_by_source, source_speaker)?;
    for (ordinal, response) in parse_source_responses(source, source.form_key.plugin, interner)?
        .into_iter()
        .enumerate()
    {
        target.fields.push(field(
            b"TRDA",
            lower_response_data(&response, fallout4_master, interner),
        ));
        target.fields.push(field(
            b"NAM1",
            FieldValue::String(interner.intern(&response.text)),
        ));
        target.fields.push(field(
            b"NAM2",
            FieldValue::String(interner.intern(&response.notes)),
        ));
        target.fields.push(field(
            b"NAM3",
            FieldValue::String(interner.intern(&response.edits)),
        ));
        target
            .fields
            .push(field(b"NAM4", FieldValue::String(interner.intern(""))));
        if let Some(idle) = response.speaker_idle {
            target
                .fields
                .push(form_key_field(b"SNAM", mapped(target_by_source, idle)?));
        }
        if let Some(idle) = response.listener_idle {
            target
                .fields
                .push(form_key_field(b"LNAM", mapped(target_by_source, idle)?));
        }
        localization.push(SkyrimDialogueLocalizationIntent {
            source_record: source.form_key,
            target_record: target.form_key,
            signature: SubrecordSig(*b"NAM1"),
            table: SkyrimDialogueLocalizedTable::IlStrings,
            ordinal,
        });
        voice.push(SkyrimDialogueVoiceIntent {
            source_info: source.form_key,
            target_info: target.form_key,
            source_speaker,
            target_speaker,
            response_number: response.number,
            transcript: response.text,
        });
    }

    for (condition_index, scope) in condition_scopes(source)?.into_iter().enumerate() {
        let condition = parse_source_condition(&scope, source.form_key.plugin, interner)?;
        target.fields.push(field(
            b"CTDA",
            lower_condition(&condition, target_by_source, interner)?,
        ));
        if let Some(variable) = &condition.vm_variable {
            target.fields.push(field(
                b"CIS2",
                FieldValue::String(interner.intern(variable)),
            ));
        }
        capabilities.push(SkyrimDialogueConditionCapabilityReceipt {
            source_info: source.form_key,
            target_info: target.form_key,
            condition_index,
            source_function: condition.source_function,
            target_function: condition.target_function,
            parameter_kind: condition.parameter_kind.into(),
            run_on: condition.run_on,
            vm_variable: condition.vm_variable,
        });
    }
    if let Some(prompt) = prompt {
        target
            .fields
            .push(field(b"RNAM", FieldValue::String(interner.intern(&prompt))));
        localization.push(SkyrimDialogueLocalizationIntent {
            source_record: source.form_key,
            target_record: target.form_key,
            signature: SubrecordSig(*b"RNAM"),
            table: SkyrimDialogueLocalizedTable::Strings,
            ordinal: 0,
        });
    }
    target.fields.push(form_key_field(b"ANAM", target_speaker));
    Ok(target)
}

fn copy_exact_fields(source: &Record, target: &mut Record, signatures: &[&[u8; 4]]) {
    for source_field in &source.fields {
        if signatures
            .iter()
            .any(|signature| source_field.sig.0 == **signature)
        {
            target.fields.push(source_field.clone());
        }
    }
}

fn require_unique_fields(record: &Record, signatures: &[&[u8; 4]]) -> Result<(), String> {
    for signature in signatures {
        if record
            .fields
            .iter()
            .filter(|field| field.sig.0 == **signature)
            .count()
            != 1
        {
            return Err(format!(
                "{} {:06X} must contain one {}",
                record.sig.as_str(),
                record.form_key.local,
                std::str::from_utf8(*signature).unwrap()
            ));
        }
    }
    Ok(())
}

fn source_parent_for_info(
    info: FormKey,
    topology: &HashMap<FormKey, Vec<FormKey>>,
) -> Result<FormKey, String> {
    let parents = topology
        .iter()
        .filter_map(|(parent, children)| children.contains(&info).then_some(*parent))
        .collect::<Vec<_>>();
    match parents.as_slice() {
        [parent] => Ok(*parent),
        _ => Err(format!("INFO {:06X} does not have one parent", info.local)),
    }
}

fn map_topic_subtype(category: u64, subtype: u64) -> Result<(u64, u64, [u8; 4]), String> {
    match (category, subtype) {
        (0, 0) => Ok((0, 0, *b"CUST")),
        (0, 1) => Ok((0, 1, *b"FGRE")),
        (0, 2) => Ok((0, 2, *b"RUMO")),
        (7, 73) => Ok((7, 82, *b"HELO")),
        _ => Err(format!(
            "unsupported Skyrim dialogue category/subtype {category}/{subtype}"
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConditionParameterKind {
    Form,
    Alias,
}

impl From<ConditionParameterKind> for SkyrimDialogueConditionParameterKind {
    fn from(value: ConditionParameterKind) -> Self {
        match value {
            ConditionParameterKind::Form => Self::Form,
            ConditionParameterKind::Alias => Self::Alias,
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedCondition {
    condition_type: u8,
    comparison_value: f32,
    source_function: u16,
    target_function: u16,
    parameter_kind: ConditionParameterKind,
    parameter_1: u32,
    source_plugin: Sym,
    run_on: u32,
    vm_variable: Option<String>,
}

#[derive(Debug)]
struct ConditionScope<'a> {
    ctda: &'a FieldEntry,
    companions: &'a [FieldEntry],
}

fn condition_scopes(record: &Record) -> Result<Vec<ConditionScope<'_>>, String> {
    let mut scopes = Vec::new();
    let mut index = 0;
    while index < record.fields.len() {
        if record.fields[index].sig.0 != *b"CTDA" {
            if matches!(record.fields[index].sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2") {
                return Err(format!(
                    "INFO {:06X} has an orphan condition string",
                    record.form_key.local
                ));
            }
            index += 1;
            continue;
        }
        let ctda = &record.fields[index];
        index += 1;
        let start = index;
        while index < record.fields.len()
            && matches!(record.fields[index].sig.0, sig if sig == *b"CIS1" || sig == *b"CIS2")
        {
            index += 1;
        }
        scopes.push(ConditionScope {
            ctda,
            companions: &record.fields[start..index],
        });
    }
    Ok(scopes)
}

fn parse_source_condition(
    scope: &ConditionScope<'_>,
    source_plugin: Sym,
    interner: &StringInterner,
) -> Result<ParsedCondition, String> {
    let FieldValue::Bytes(bytes) = &scope.ctda.value else {
        return Err("Skyrim dialogue CTDA is not a raw 32-byte row".to_string());
    };
    if bytes.len() != 32
        || bytes[1..4] != [0, 0, 0]
        || bytes[10..12] != [0, 0]
        || u32::from_le_bytes(bytes[24..28].try_into().unwrap()) != 0
        || i32::from_le_bytes(bytes[28..32].try_into().unwrap()) != -1
    {
        return Err("Skyrim dialogue CTDA has an unsupported payload shape".to_string());
    }
    let source_function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    let run_on = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
    let (target_function, parameter_kind, requires_vm_variable) =
        condition_translation(source_function, run_on).ok_or_else(|| {
            if matches!(source_function, 47 | 58 | 72 | 566 | SKYRIM_GET_VM_QUEST_VARIABLE) {
                format!(
                    "Skyrim dialogue condition function {source_function} has unsupported run-on {run_on}"
                )
            } else {
                format!("unsupported Skyrim dialogue condition function {source_function}")
            }
        })?;
    let parameter_1 = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if parameter_kind == ConditionParameterKind::Form && parameter_1 & 0x00FF_FFFF == 0 {
        return Err("Skyrim dialogue condition has a null Form parameter".to_string());
    }
    let parameter_2 = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let companion_strings = scope
        .companions
        .iter()
        .map(|field| {
            if field.sig.0 != *b"CIS2" {
                return None;
            }
            text_value(&field.value, interner)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or("Skyrim dialogue condition has an unsupported CIS companion")?;
    let vm_variable = match (requires_vm_variable, companion_strings.as_slice()) {
        (true, [variable]) if parameter_2 != 0 && !variable.is_empty() => Some(variable.clone()),
        (false, []) if parameter_2 == 0 => None,
        _ => {
            return Err(
                "Skyrim dialogue condition VM-variable token/CIS2 shape is invalid".to_string(),
            );
        }
    };
    Ok(ParsedCondition {
        condition_type: bytes[0],
        comparison_value: f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        source_function,
        target_function,
        parameter_kind,
        parameter_1,
        source_plugin,
        run_on,
        vm_variable,
    })
}

fn condition_translation(
    source_function: u16,
    run_on: u32,
) -> Option<(u16, ConditionParameterKind, bool)> {
    Some(match source_function {
        47 if matches!(run_on, 0 | 1) => (47, ConditionParameterKind::Form, false),
        58 | 72 if run_on == 0 => (source_function, ConditionParameterKind::Form, false),
        566 if run_on == 0 => (566, ConditionParameterKind::Alias, false),
        SKYRIM_GET_VM_QUEST_VARIABLE if run_on == 0 => (
            FO4_GET_VM_QUEST_VARIABLE,
            ConditionParameterKind::Form,
            true,
        ),
        _ => return None,
    })
}

fn lower_condition(
    condition: &ParsedCondition,
    target_by_source: &HashMap<FormKey, FormKey>,
    interner: &StringInterner,
) -> Result<FieldValue, String> {
    let parameter_1 = match condition.parameter_kind {
        ConditionParameterKind::Alias => FieldValue::Uint(condition.parameter_1 as u64),
        ConditionParameterKind::Form => FieldValue::FormKey(mapped(
            target_by_source,
            FormKey {
                local: condition.parameter_1 & 0x00FF_FFFF,
                plugin: condition.source_plugin,
            },
        )?),
    };
    Ok(FieldValue::Struct(vec![
        (
            interner.intern("type"),
            FieldValue::Uint(condition.condition_type as u64),
        ),
        (interner.intern("unknown_u8_1"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_2"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
        (
            interner.intern("comparison_value"),
            FieldValue::Float(condition.comparison_value),
        ),
        (
            interner.intern("function"),
            FieldValue::Uint(condition.target_function as u64),
        ),
        (interner.intern("unknown_u8_6"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_7"), FieldValue::Uint(0)),
        (interner.intern("parameter_1"), parameter_1),
        (interner.intern("parameter_2"), FieldValue::Uint(0)),
        (
            interner.intern("run_on"),
            FieldValue::Uint(condition.run_on as u64),
        ),
        (interner.intern("reference"), FieldValue::Uint(0)),
        (interner.intern("parameter_3"), FieldValue::Int(-1)),
    ]))
}

#[derive(Debug, Clone, Copy)]
enum Emotion {
    Neutral,
    Angry,
    Afraid,
    Sad,
    Happy,
    Surprised,
    Puzzled,
}

impl Emotion {
    fn from_source(value: u32) -> Option<Self> {
        Some(match value {
            0 => Self::Neutral,
            1 => Self::Angry,
            3 => Self::Afraid,
            4 => Self::Sad,
            5 => Self::Happy,
            6 => Self::Surprised,
            7 => Self::Puzzled,
            _ => return None,
        })
    }

    fn from_name(value: &str) -> Option<Self> {
        Some(match value {
            "Neutral" => Self::Neutral,
            "Anger" | "Angry" => Self::Angry,
            "Fear" | "Afraid" => Self::Afraid,
            "Sad" => Self::Sad,
            "Happy" => Self::Happy,
            "Surprise" | "Surprised" => Self::Surprised,
            "Puzzled" => Self::Puzzled,
            _ => return None,
        })
    }

    fn target_keyword(self) -> u32 {
        match self {
            Self::Neutral => 0x0D755D,
            Self::Angry => 0x0FA84A,
            Self::Afraid => 0x0FA84B,
            Self::Sad => 0x0FA848,
            Self::Happy => 0x0FA847,
            Self::Surprised => 0x0C8672,
            Self::Puzzled => 0x0C8673,
        }
    }
}

#[derive(Debug, Clone)]
struct ParsedResponse {
    text: String,
    notes: String,
    edits: String,
    emotion: Emotion,
    number: u8,
    speaker_idle: Option<FormKey>,
    listener_idle: Option<FormKey>,
}

fn parse_source_responses(
    source: &Record,
    source_plugin: Sym,
    interner: &StringInterner,
) -> Result<Vec<ParsedResponse>, String> {
    let mut responses = Vec::new();
    let mut index = 0;
    while index < source.fields.len() {
        if source.fields[index].sig.0 != *b"TRDT" {
            if matches!(source.fields[index].sig.0, sig if sig == *b"NAM1" || sig == *b"NAM2" || sig == *b"NAM3" || sig == *b"SNAM" || sig == *b"LNAM")
            {
                return Err(format!(
                    "INFO {:06X} has an orphan response field",
                    source.form_key.local
                ));
            }
            index += 1;
            continue;
        }
        let (emotion, number, sound) =
            parse_source_trdt(&source.fields[index].value, source_plugin, interner)
                .ok_or_else(|| format!("INFO {:06X} has malformed TRDT", source.form_key.local))?;
        if sound.is_some() {
            return Err(format!(
                "INFO {:06X} uses an explicit response sound",
                source.form_key.local
            ));
        }
        index += 1;
        let text = response_text(source, &mut index, b"NAM1", interner)?;
        let notes = response_text(source, &mut index, b"NAM2", interner)?;
        let edits = response_text(source, &mut index, b"NAM3", interner)?;
        let speaker_idle = response_idle(source, &mut index, b"SNAM", source_plugin)?;
        let listener_idle = response_idle(source, &mut index, b"LNAM", source_plugin)?;
        responses.push(ParsedResponse {
            text,
            notes,
            edits,
            emotion,
            number,
            speaker_idle,
            listener_idle,
        });
    }
    Ok(responses)
}

fn response_text(
    source: &Record,
    index: &mut usize,
    signature: &[u8; 4],
    interner: &StringInterner,
) -> Result<String, String> {
    let Some(entry) = source.fields.get(*index) else {
        return Err(format!(
            "INFO {:06X} has an incomplete response",
            source.form_key.local
        ));
    };
    if entry.sig.0 != *signature {
        return Err(format!(
            "INFO {:06X} response companion order is unsupported",
            source.form_key.local
        ));
    }
    *index += 1;
    text_value(&entry.value, interner).ok_or_else(|| {
        format!(
            "INFO {:06X} {} response companion is not text",
            source.form_key.local,
            std::str::from_utf8(signature).unwrap()
        )
    })
}

fn response_idle(
    source: &Record,
    index: &mut usize,
    signature: &[u8; 4],
    source_plugin: Sym,
) -> Result<Option<FormKey>, String> {
    let Some(entry) = source.fields.get(*index) else {
        return Ok(None);
    };
    if entry.sig.0 != *signature {
        return Ok(None);
    }
    *index += 1;
    nullable_source_form_key(&entry.value, source_plugin).ok_or_else(|| {
        format!(
            "INFO {:06X} {} response companion is not a FormID",
            source.form_key.local,
            std::str::from_utf8(signature).unwrap()
        )
    })
}

fn parse_source_trdt(
    value: &FieldValue,
    source_plugin: Sym,
    interner: &StringInterner,
) -> Option<(Emotion, u8, Option<FormKey>)> {
    match value {
        FieldValue::Bytes(bytes)
            if bytes.len() == 24
                && bytes[4..12] == [0; 8]
                && bytes[13..16] == [0; 3]
                && bytes[20..24] == [0; 4] =>
        {
            let emotion = Emotion::from_source(u32::from_le_bytes(bytes[0..4].try_into().ok()?))?;
            let sound = u32::from_le_bytes(bytes[16..20].try_into().ok()?);
            Some((
                emotion,
                bytes[12],
                (sound != 0).then_some(FormKey {
                    local: sound & 0x00FF_FFFF,
                    plugin: source_plugin,
                }),
            ))
        }
        FieldValue::Struct(fields) => {
            if !source_trdt_struct_is_supported(fields, interner) {
                return None;
            }
            let emotion = named_value(fields, "emotion_type", interner)
                .and_then(|value| emotion_value(value, interner))?;
            let number = named_value(fields, "response_number", interner)
                .and_then(|value| scalar_value(value, None, Some(interner)))?
                as u8;
            let sound = named_value(fields, "sound", interner)
                .and_then(|value| nullable_source_form_key(value, source_plugin))
                .flatten();
            Some((emotion, number, sound))
        }
        _ => None,
    }
}

fn source_trdt_struct_is_supported(
    fields: &[(Sym, FieldValue)],
    interner: &StringInterner,
) -> bool {
    fields.iter().all(|(name, value)| {
        let Some(name) = interner.resolve(*name).map(canonical) else {
            return false;
        };
        match name.as_str() {
            "emotiontype" | "responsenumber" | "sound" => true,
            "emotionvalue"
            | "unknownu82"
            | "unknownu83"
            | "unknownu84"
            | "unknownu85"
            | "unknownu87"
            | "unknownu88"
            | "unknownu89"
            | "useemotionanimation"
            | "unknownu812"
            | "unknownu813"
            | "unknownu814" => scalar_value(value, None, Some(interner)) == Some(0),
            _ => false,
        }
    })
}

fn lower_response_data(
    response: &ParsedResponse,
    fallout4_master: Sym,
    interner: &StringInterner,
) -> FieldValue {
    FieldValue::Struct(vec![
        (
            interner.intern("emotion"),
            FieldValue::FormKey(FormKey {
                local: response.emotion.target_keyword(),
                plugin: fallout4_master,
            }),
        ),
        (
            interner.intern("response_number"),
            FieldValue::Uint(response.number as u64),
        ),
        (interner.intern("sound_file"), FieldValue::Uint(0)),
        (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
        (interner.intern("interrupt_percentage"), FieldValue::Uint(0)),
        (interner.intern("camera_target_alias"), FieldValue::Int(-1)),
        (
            interner.intern("camera_location_alias"),
            FieldValue::Int(-1),
        ),
    ])
}

fn source_info_flags(source: &Record, interner: &StringInterner) -> Option<u16> {
    let field = source
        .fields
        .iter()
        .find(|field| matches!(field.sig.0, sig if sig == *b"DATA" || sig == *b"ENAM"))?;
    match &field.value {
        FieldValue::Bytes(bytes)
            if field.sig.0 == *b"DATA"
                && bytes.len() == 8
                && bytes[0..2] == [0; 2]
                && bytes[4..8] == [0; 4] =>
        {
            Some(u16::from_le_bytes([bytes[2], bytes[3]]))
        }
        FieldValue::Bytes(bytes)
            if field.sig.0 == *b"ENAM" && bytes.len() == 4 && bytes[2..4] == [0; 2] =>
        {
            Some(u16::from_le_bytes([bytes[0], bytes[1]]))
        }
        FieldValue::Struct(fields) if source_info_data_struct_is_supported(fields, interner) => {
            named_value(fields, "response_flags", interner)
                .and_then(|value| scalar_value(value, None, Some(interner)))
                .and_then(|value| u16::try_from(value).ok())
        }
        _ => None,
    }
}

fn source_info_data_struct_is_supported(
    fields: &[(Sym, FieldValue)],
    interner: &StringInterner,
) -> bool {
    let mut response_flags = 0;
    for (name, value) in fields {
        let Some(name) = interner.resolve(*name).map(canonical) else {
            return false;
        };
        match name.as_str() {
            "responseflags" => response_flags += 1,
            "questdialoguetab" | "resetdays" | "resethours" => {
                if !value_is_zero(value, interner) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    response_flags == 1
}

fn value_is_zero(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Float(value) => value.to_bits() == 0.0_f32.to_bits(),
        _ => scalar_value(value, None, Some(interner)) == Some(0),
    }
}

fn dial_data(source: &Record, interner: &StringInterner) -> Option<(u64, u64, u64)> {
    let value = &source
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DATA")?
        .value;
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 4 => Some((
            bytes[0] as u64,
            bytes[1] as u64,
            u16::from_le_bytes([bytes[2], bytes[3]]) as u64,
        )),
        FieldValue::Struct(fields) if dial_data_struct_is_supported(fields, interner) => Some((
            named_value(fields, "do_all_before_repeating", interner)
                .or_else(|| named_value(fields, "topic_flags", interner))
                .and_then(|value| scalar_value(value, None, Some(interner)))?,
            named_value(fields, "category", interner)
                .and_then(|value| scalar_value(value, None, Some(interner)))?,
            named_value(fields, "subtype", interner)
                .and_then(|value| scalar_value(value, None, Some(interner)))?,
        )),
        _ => None,
    }
}

fn dial_data_struct_is_supported(fields: &[(Sym, FieldValue)], interner: &StringInterner) -> bool {
    let mut flags = 0;
    let mut category = 0;
    let mut subtype = 0;
    for (name, _) in fields {
        let Some(name) = interner.resolve(*name).map(canonical) else {
            return false;
        };
        match name.as_str() {
            "doallbeforerepeating" | "topicflags" => flags += 1,
            "category" => category += 1,
            "subtype" => subtype += 1,
            _ => return false,
        }
    }
    flags == 1 && category == 1 && subtype == 1
}

fn four_byte_field(record: &Record, signature: &[u8; 4]) -> Result<[u8; 4], String> {
    let values = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *signature)
        .filter_map(|field| four_byte_array(&field.value))
        .collect::<Vec<_>>();
    match values.as_slice() {
        [value] => Ok(*value),
        _ => Err(format!(
            "{} {:06X} must have one four-byte {}",
            record.sig.as_str(),
            record.form_key.local,
            std::str::from_utf8(signature).unwrap()
        )),
    }
}

fn four_byte_array(value: &FieldValue) -> Option<[u8; 4]> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 4 => bytes.as_slice().try_into().ok(),
        FieldValue::Uint(value) => Some((*value as u32).to_le_bytes()),
        FieldValue::Int(value) => Some((*value as i32).to_le_bytes()),
        FieldValue::Struct(fields) => fields.first().and_then(|(_, value)| four_byte_array(value)),
        _ => None,
    }
}

fn mapped(
    target_by_source: &HashMap<FormKey, FormKey>,
    source: FormKey,
) -> Result<FormKey, String> {
    target_by_source.get(&source).copied().ok_or_else(|| {
        format!(
            "dialogue source FormKey {:06X} has no target mapping",
            source.local
        )
    })
}

fn base_target(
    source: &Record,
    target_by_source: &HashMap<FormKey, FormKey>,
) -> Result<Record, String> {
    let mut target = Record::new(source.sig, mapped(target_by_source, source.form_key)?);
    target.flags = source.flags;
    Ok(target)
}

fn push_editor_id(record: &mut Record, editor_id: Option<Sym>) {
    record.eid = editor_id;
    if let Some(editor_id) = editor_id {
        record
            .fields
            .push(field(b"EDID", FieldValue::String(editor_id)));
    }
}

fn field(signature: &[u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*signature),
        value,
    }
}

fn form_key_field(signature: &[u8; 4], value: FormKey) -> FieldEntry {
    field(signature, FieldValue::FormKey(value))
}

fn exactly_one_form_key(record: &Record, signature: &[u8; 4]) -> Result<FormKey, String> {
    let values = all_form_keys(record, signature)?;
    match values.as_slice() {
        [value] => Ok(*value),
        _ => Err(format!(
            "{} {:06X} must have one {} FormKey",
            record.sig.as_str(),
            record.form_key.local,
            std::str::from_utf8(signature).unwrap()
        )),
    }
}

fn all_form_keys(record: &Record, signature: &[u8; 4]) -> Result<Vec<FormKey>, String> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *signature)
        .map(|field| {
            first_form_key(&field.value).ok_or_else(|| {
                format!(
                    "{} {:06X} {} is not a FormKey",
                    record.sig.as_str(),
                    record.form_key.local,
                    std::str::from_utf8(signature).unwrap()
                )
            })
        })
        .collect()
}

fn optional_form_key(record: &Record, signature: &[u8; 4]) -> Result<Option<FormKey>, String> {
    let values = all_form_keys(record, signature)?;
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(*value)),
        _ => Err(format!(
            "{} {:06X} must have at most one {} FormKey",
            record.sig.as_str(),
            record.form_key.local,
            std::str::from_utf8(signature).unwrap()
        )),
    }
}

fn optional_text_field(
    record: &Record,
    signature: &[u8; 4],
    interner: &StringInterner,
) -> Result<Option<String>, String> {
    let values = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *signature)
        .map(|field| text_value(&field.value, interner))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            format!(
                "{} {:06X} {} is not text",
                record.sig.as_str(),
                record.form_key.local,
                std::str::from_utf8(signature).unwrap()
            )
        })?;
    match values.as_slice() {
        [] => Ok(None),
        [value] => Ok(Some(value.clone())),
        _ => Err(format!(
            "{} {:06X} has duplicate {} fields",
            record.sig.as_str(),
            record.form_key.local,
            std::str::from_utf8(signature).unwrap()
        )),
    }
}

fn first_form_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(value) => Some(*value),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_form_key(value)),
        FieldValue::List(values) => values.iter().find_map(first_form_key),
        _ => None,
    }
}

fn nullable_source_form_key(value: &FieldValue, source_plugin: Sym) -> Option<Option<FormKey>> {
    match value {
        FieldValue::FormKey(value) => Some((value.local != 0).then_some(*value)),
        FieldValue::Uint(value) => Some((*value != 0).then_some(FormKey {
            local: *value as u32 & 0x00FF_FFFF,
            plugin: source_plugin,
        })),
        FieldValue::Int(value) if *value >= 0 => Some((*value != 0).then_some(FormKey {
            local: *value as u32 & 0x00FF_FFFF,
            plugin: source_plugin,
        })),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            let value = u32::from_le_bytes(bytes.as_slice().try_into().ok()?);
            Some((value != 0).then_some(FormKey {
                local: value & 0x00FF_FFFF,
                plugin: source_plugin,
            }))
        }
        FieldValue::Struct(fields) if fields.len() == 1 => {
            nullable_source_form_key(&fields[0].1, source_plugin)
        }
        _ => None,
    }
}

fn scalar_field(
    record: &Record,
    signature: &[u8; 4],
    name: Option<&str>,
    interner: &StringInterner,
) -> Option<u64> {
    let value = &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *signature)?
        .value;
    scalar_value(value, name, Some(interner))
}

fn scalar_value(
    value: &FieldValue,
    name: Option<&str>,
    interner: Option<&StringInterner>,
) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) if *value >= 0 => Some(*value as u64),
        FieldValue::Bool(value) => Some(u64::from(*value)),
        FieldValue::Bytes(bytes) => match bytes.len() {
            1 => Some(bytes[0] as u64),
            2 => Some(u16::from_le_bytes(bytes[..2].try_into().ok()?) as u64),
            4.. => Some(u32::from_le_bytes(bytes[..4].try_into().ok()?) as u64),
            _ => None,
        },
        FieldValue::Struct(fields) => {
            let selected = match (name, interner) {
                (Some(name), Some(interner)) => named_value(fields, name, interner),
                _ => fields.first().map(|(_, value)| value),
            }?;
            scalar_value(selected, None, interner)
        }
        FieldValue::List(values) => values
            .first()
            .and_then(|value| scalar_value(value, None, interner)),
        FieldValue::String(value) => interner?.resolve(*value)?.parse().ok(),
        _ => None,
    }
}

fn named_value<'a>(
    fields: &'a [(Sym, FieldValue)],
    expected: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    let expected = canonical(expected);
    fields.iter().find_map(|(name, value)| {
        (interner.resolve(*name).map(canonical).as_deref() == Some(expected.as_str()))
            .then_some(value)
    })
}

fn canonical(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn text_value(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(value) => interner.resolve(*value).map(str::to_string),
        FieldValue::Bytes(bytes) => std::str::from_utf8(bytes)
            .ok()
            .map(|value| value.trim_end_matches('\0').to_string()),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| text_value(value, interner)),
        FieldValue::List(values) => values.iter().find_map(|value| text_value(value, interner)),
        _ => None,
    }
}

fn emotion_value(value: &FieldValue, interner: &StringInterner) -> Option<Emotion> {
    match value {
        FieldValue::Uint(value) => Emotion::from_source(*value as u32),
        FieldValue::Int(value) if *value >= 0 => Emotion::from_source(*value as u32),
        FieldValue::String(value) => Emotion::from_name(interner.resolve(*value)?),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| emotion_value(value, interner)),
        _ => None,
    }
}

fn signature_rank(signature: SigCode) -> u8 {
    match signature.0 {
        sig if sig == *b"DLVW" => 0,
        sig if sig == *b"DLBR" => 1,
        sig if sig == *b"DIAL" => 2,
        sig if sig == *b"INFO" => 3,
        _ => 4,
    }
}

fn form_key_sort_key(form_key: FormKey, interner: &StringInterner) -> (String, u32) {
    (
        interner
            .resolve(form_key.plugin)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        form_key.local,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(signature: &[u8; 4], form_key: FormKey) -> Record {
        Record::new(SigCode(*signature), form_key)
    }

    fn fk(local: u32, plugin: Sym) -> FormKey {
        FormKey { local, plugin }
    }

    #[test]
    fn projects_source_derived_dialogue_and_freezes_receipt() {
        let interner = StringInterner::new();
        let source = interner.intern("FixtureSource.esm");
        let target = interner.intern("FixtureTarget.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let quest = fk(1, source);
        let speaker = fk(2, source);
        let view_key = fk(3, source);
        let branch_key = fk(4, source);
        let topic_key = fk(5, source);
        let info_key = fk(6, source);
        let shared_info = fk(7, source);
        let speaker_idle = fk(8, source);
        let listener_idle = fk(9, source);

        let mut view = record(b"DLVW", view_key);
        view.fields.push(form_key_field(b"QNAM", quest));
        view.fields.push(form_key_field(b"BNAM", branch_key));
        view.fields.push(field(b"ENAM", FieldValue::Uint(0)));
        view.fields.push(field(b"DNAM", FieldValue::Uint(1)));

        let mut branch = record(b"DLBR", branch_key);
        branch.fields.push(form_key_field(b"QNAM", quest));
        branch.fields.push(field(b"TNAM", FieldValue::Uint(0)));
        branch.fields.push(field(b"DNAM", FieldValue::Uint(1)));
        branch.fields.push(form_key_field(b"SNAM", topic_key));

        let mut topic = record(b"DIAL", topic_key);
        topic.fields.push(field(
            b"FULL",
            FieldValue::String(interner.intern("Fixture topic.")),
        ));
        topic.fields.push(form_key_field(b"BNAM", branch_key));
        topic.fields.push(form_key_field(b"QNAM", quest));
        topic.fields.push(field(
            b"DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("do_all_before_repeating"),
                    FieldValue::Uint(0),
                ),
                (interner.intern("category"), FieldValue::Uint(7)),
                (interner.intern("subtype"), FieldValue::Uint(73)),
            ]),
        ));
        topic.fields.push(field(
            b"SNAM",
            FieldValue::Bytes(SmallVec::from_slice(b"HELO")),
        ));
        topic.fields.push(field(b"TIFC", FieldValue::Uint(1)));

        let mut info = record(b"INFO", info_key);
        info.fields.push(field(
            b"DATA",
            FieldValue::Struct(vec![(
                interner.intern("response_flags"),
                FieldValue::Uint(0),
            )]),
        ));
        info.fields.push(field(
            b"TRDT",
            FieldValue::Struct(vec![
                (interner.intern("emotion_type"), FieldValue::Uint(0)),
                (interner.intern("response_number"), FieldValue::Uint(1)),
                (interner.intern("sound"), FieldValue::Uint(0)),
            ]),
        ));
        info.fields.push(field(
            b"NAM1",
            FieldValue::String(interner.intern("Fixture response.")),
        ));
        info.fields
            .push(field(b"NAM2", FieldValue::String(interner.intern(""))));
        info.fields
            .push(field(b"NAM3", FieldValue::String(interner.intern(""))));
        info.fields.push(form_key_field(b"SNAM", speaker_idle));
        info.fields.push(form_key_field(b"LNAM", listener_idle));
        let mut condition = vec![0u8; 32];
        condition[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
        condition[8..10].copy_from_slice(&629_u16.to_le_bytes());
        condition[12..16].copy_from_slice(&quest.local.to_le_bytes());
        condition[16..20].copy_from_slice(&0x1234_u32.to_le_bytes());
        condition[28..32].copy_from_slice(&(-1_i32).to_le_bytes());
        info.fields.push(field(
            b"CTDA",
            FieldValue::Bytes(SmallVec::from_vec(condition)),
        ));
        info.fields.push(field(
            b"CIS2",
            FieldValue::String(interner.intern("::Fixture_var")),
        ));
        info.fields.push(form_key_field(b"DNAM", shared_info));
        info.fields.push(field(
            b"RNAM",
            FieldValue::String(interner.intern("Fixture prompt.")),
        ));
        info.fields.push(form_key_field(b"ANAM", speaker));

        let records = vec![view, branch, topic, info];
        let topology = HashMap::from([(topic_key, vec![info_key])]);
        let plan = SkyrimDialoguePlan::derive(&records, &topology, &interner).unwrap();
        assert_eq!(plan.unsupported_reason(), None);

        let target_map = [
            quest,
            speaker,
            view_key,
            branch_key,
            topic_key,
            info_key,
            shared_info,
            speaker_idle,
            listener_idle,
        ]
        .into_iter()
        .map(|source_key| {
            (
                source_key,
                FormKey {
                    local: source_key.local + 0x100,
                    plugin: target,
                },
            )
        })
        .collect();
        let projection = project_dialogue(&plan, &target_map, fallout4, &interner).unwrap();
        assert_eq!(projection.top_level.len(), 1);
        assert_eq!(projection.top_level[0].sig.0, *b"DLVW");
        assert_eq!(projection.quest_children.len(), 2);
        assert_eq!(projection.topic_children.len(), 1);
        assert!(projection.receipt.topology.iter().any(|edge| {
            edge.target_child == projection.top_level[0].form_key
                && edge.kind == SkyrimDialogueTopologyKind::TopLevel
        }));
        assert_eq!(
            projection.receipt.condition_capabilities[0].source_function,
            629
        );
        assert_eq!(
            projection.receipt.condition_capabilities[0].target_function,
            629
        );
        assert_eq!(projection.receipt.voice_intents.len(), 1);
        assert_eq!(projection.receipt.subtype_capabilities.len(), 1);
        assert_eq!(projection.receipt.localization_intents.len(), 3);
        assert!(
            projection
                .receipt
                .localization_intents
                .iter()
                .any(|intent| {
                    intent.signature.0 == *b"NAM1"
                        && intent.table == SkyrimDialogueLocalizedTable::IlStrings
                })
        );
        assert!(
            projection
                .receipt
                .localization_intents
                .iter()
                .any(|intent| {
                    intent.signature.0 == *b"RNAM"
                        && intent.table == SkyrimDialogueLocalizedTable::Strings
                })
        );
        let target_topic = projection
            .quest_children
            .iter()
            .find(|record| record.sig.0 == *b"DIAL")
            .unwrap();
        assert_eq!(dial_data(target_topic, &interner), Some((0, 7, 82)));
        let target_info = &projection.topic_children[0].record;
        assert_eq!(
            optional_form_key(target_info, b"DNAM").unwrap(),
            Some(target_map[&shared_info])
        );
        assert_eq!(
            optional_text_field(target_info, b"RNAM", &interner).unwrap(),
            Some("Fixture prompt.".to_string())
        );
        assert!(target_info.fields.iter().any(|entry| {
            entry.sig.0 == *b"SNAM"
                && first_form_key(&entry.value) == Some(target_map[&speaker_idle])
        }));
        assert!(target_info.fields.iter().any(|entry| {
            entry.sig.0 == *b"LNAM"
                && first_form_key(&entry.value) == Some(target_map[&listener_idle])
        }));

        let materialized = projection
            .top_level
            .iter()
            .chain(projection.quest_children.iter())
            .chain(projection.topic_children.iter().map(|child| &child.record))
            .cloned()
            .collect::<Vec<_>>();
        let parents = HashMap::from([(
            projection.topic_children[0].record.form_key,
            projection.topic_children[0].parent_topic,
        )]);
        projection
            .receipt
            .validate_materialized(&materialized, &parents)
            .unwrap();

        let mut changed = materialized;
        changed[0].fields.push(field(b"FULL", FieldValue::Uint(1)));
        assert!(
            projection
                .receipt
                .validate_materialized(&changed, &parents)
                .unwrap_err()
                .contains("fields changed")
        );
    }

    #[test]
    fn unsupported_semantics_are_reported_before_projection() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FixtureSource.esm");
        let quest = fk(1, plugin);
        let topic_key = fk(2, plugin);
        let mut topic = record(b"DIAL", topic_key);
        topic.fields.push(form_key_field(b"QNAM", quest));
        topic.fields.push(field(
            b"DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("do_all_before_repeating"),
                    FieldValue::Uint(0),
                ),
                (interner.intern("category"), FieldValue::Uint(7)),
                (interner.intern("subtype"), FieldValue::Uint(999)),
            ]),
        ));
        topic.fields.push(field(
            b"SNAM",
            FieldValue::Bytes(SmallVec::from_slice(b"TEST")),
        ));
        topic.fields.push(field(b"TIFC", FieldValue::Uint(0)));
        let plan = SkyrimDialoguePlan::derive(
            &[topic],
            &HashMap::from([(topic_key, Vec::new())]),
            &interner,
        )
        .unwrap();
        assert!(
            plan.unsupported_reason()
                .unwrap()
                .contains("category/subtype")
        );
    }

    fn condition_field(
        function: u16,
        parameter_1: u32,
        parameter_2: u32,
        run_on: u32,
    ) -> FieldEntry {
        let mut bytes = vec![0u8; 32];
        bytes[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
        bytes[8..10].copy_from_slice(&function.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[16..20].copy_from_slice(&parameter_2.to_le_bytes());
        bytes[20..24].copy_from_slice(&run_on.to_le_bytes());
        bytes[28..32].copy_from_slice(&(-1_i32).to_le_bytes());
        field(b"CTDA", FieldValue::Bytes(SmallVec::from_vec(bytes)))
    }

    fn minimal_topic(
        topic: FormKey,
        quest: FormKey,
        child_count: usize,
        interner: &StringInterner,
    ) -> Record {
        let mut record = record(b"DIAL", topic);
        record.fields.push(form_key_field(b"QNAM", quest));
        record.fields.push(field(
            b"DATA",
            FieldValue::Struct(vec![
                (interner.intern("topic_flags"), FieldValue::Uint(0)),
                (interner.intern("category"), FieldValue::Uint(0)),
                (interner.intern("subtype"), FieldValue::Uint(0)),
            ]),
        ));
        record.fields.push(field(
            b"SNAM",
            FieldValue::Bytes(SmallVec::from_slice(b"CUST")),
        ));
        record
            .fields
            .push(field(b"TIFC", FieldValue::Uint(child_count as u64)));
        record
    }

    fn minimal_info(info: FormKey, speaker: FormKey, interner: &StringInterner) -> Record {
        let mut record = record(b"INFO", info);
        record.fields.push(field(
            b"ENAM",
            FieldValue::Struct(vec![(
                interner.intern("response_flags"),
                FieldValue::Uint(0),
            )]),
        ));
        record.fields.push(field(
            b"TRDT",
            FieldValue::Struct(vec![
                (interner.intern("emotion_type"), FieldValue::Uint(0)),
                (interner.intern("response_number"), FieldValue::Uint(1)),
                (interner.intern("sound"), FieldValue::Uint(0)),
            ]),
        ));
        record.fields.push(field(
            b"NAM1",
            FieldValue::String(interner.intern("Response.")),
        ));
        record
            .fields
            .push(field(b"NAM2", FieldValue::String(interner.intern(""))));
        record
            .fields
            .push(field(b"NAM3", FieldValue::String(interner.intern(""))));
        record.fields.push(form_key_field(b"ANAM", speaker));
        record
    }

    #[test]
    fn subtype_capability_matrix_is_explicit_and_stable() {
        assert_eq!(map_topic_subtype(0, 0).unwrap(), (0, 0, *b"CUST"));
        assert_eq!(map_topic_subtype(0, 1).unwrap(), (0, 1, *b"FGRE"));
        assert_eq!(map_topic_subtype(0, 2).unwrap(), (0, 2, *b"RUMO"));
        assert_eq!(map_topic_subtype(7, 73).unwrap(), (7, 82, *b"HELO"));
        assert!(map_topic_subtype(1, 12).is_err());
        assert!(map_topic_subtype(7, 79).is_err());
    }

    #[test]
    fn condition_capabilities_cover_form_alias_and_vm_variable_shapes() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FixtureSource.esm");
        let form = fk(0x1234, plugin);

        let form_entry = condition_field(47, form.local, 0, 1);
        let form_scope = ConditionScope {
            ctda: &form_entry,
            companions: &[],
        };
        let parsed = parse_source_condition(&form_scope, plugin, &interner).unwrap();
        assert_eq!(parsed.parameter_kind, ConditionParameterKind::Form);
        assert_eq!(parsed.run_on, 1);

        let alias_entry = condition_field(566, 3, 0, 0);
        let alias_scope = ConditionScope {
            ctda: &alias_entry,
            companions: &[],
        };
        let parsed = parse_source_condition(&alias_scope, plugin, &interner).unwrap();
        assert_eq!(parsed.parameter_kind, ConditionParameterKind::Alias);
        assert_eq!(parsed.parameter_1, 3);

        let vm_entry = condition_field(629, form.local, 1, 0);
        let companions = [field(
            b"CIS2",
            FieldValue::String(interner.intern("::QuestVariable_var")),
        )];
        let vm_scope = ConditionScope {
            ctda: &vm_entry,
            companions: &companions,
        };
        let parsed = parse_source_condition(&vm_scope, plugin, &interner).unwrap();
        assert_eq!(parsed.vm_variable.as_deref(), Some("::QuestVariable_var"));

        let invalid_alias = condition_field(566, 3, 0, 1);
        let invalid_scope = ConditionScope {
            ctda: &invalid_alias,
            companions: &[],
        };
        assert!(
            parse_source_condition(&invalid_scope, plugin, &interner)
                .unwrap_err()
                .contains("unsupported run-on")
        );
    }

    #[test]
    fn condition_companion_layouts_fail_closed() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FixtureSource.esm");
        let entry = condition_field(629, 0x1234, 1, 0);
        let cis1 = [field(
            b"CIS1",
            FieldValue::String(interner.intern("::WrongSlot_var")),
        )];
        let scope = ConditionScope {
            ctda: &entry,
            companions: &cis1,
        };
        assert!(
            parse_source_condition(&scope, plugin, &interner)
                .unwrap_err()
                .contains("unsupported CIS companion")
        );

        let duplicate = [
            field(b"CIS2", FieldValue::String(interner.intern("::One_var"))),
            field(b"CIS2", FieldValue::String(interner.intern("::Two_var"))),
        ];
        let scope = ConditionScope {
            ctda: &entry,
            companions: &duplicate,
        };
        assert!(
            parse_source_condition(&scope, plugin, &interner)
                .unwrap_err()
                .contains("token/CIS2 shape")
        );

        let mut info = record(b"INFO", fk(1, plugin));
        info.fields.push(field(
            b"CIS2",
            FieldValue::String(interner.intern("::Orphan_var")),
        ));
        assert!(condition_scopes(&info).unwrap_err().contains("orphan"));
    }

    #[test]
    fn response_order_and_emotion_matrix_are_deterministic() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FixtureSource.esm");
        let mut info = record(b"INFO", fk(1, plugin));
        for (number, emotion) in [0u64, 1, 3, 4, 5, 6, 7].into_iter().enumerate() {
            info.fields.push(field(
                b"TRDT",
                FieldValue::Struct(vec![
                    (interner.intern("emotion_type"), FieldValue::Uint(emotion)),
                    (
                        interner.intern("response_number"),
                        FieldValue::Uint(number as u64 + 1),
                    ),
                    (interner.intern("sound"), FieldValue::Uint(0)),
                ]),
            ));
            info.fields.push(field(
                b"NAM1",
                FieldValue::String(interner.intern(&format!("Response {number}"))),
            ));
            info.fields
                .push(field(b"NAM2", FieldValue::String(interner.intern("notes"))));
            info.fields
                .push(field(b"NAM3", FieldValue::String(interner.intern("edits"))));
        }
        let responses = parse_source_responses(&info, plugin, &interner).unwrap();
        assert_eq!(responses.len(), 7);
        assert_eq!(responses[0].notes, "notes");
        assert_eq!(responses[6].edits, "edits");
        assert_eq!(responses[0].emotion.target_keyword(), 0x0D755D);
        assert_eq!(responses[6].emotion.target_keyword(), 0x0C8673);

        info.fields.swap(1, 2);
        assert!(
            parse_source_responses(&info, plugin, &interner)
                .unwrap_err()
                .contains("companion order")
        );
    }

    #[test]
    fn duplicate_topic_topology_rejects_the_component() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FixtureSource.esm");
        let quest = fk(1, plugin);
        let speaker = fk(2, plugin);
        let first_topic = fk(3, plugin);
        let second_topic = fk(4, plugin);
        let info_key = fk(5, plugin);
        let records = vec![
            minimal_topic(first_topic, quest, 1, &interner),
            minimal_topic(second_topic, quest, 1, &interner),
            minimal_info(info_key, speaker, &interner),
        ];
        let topology = HashMap::from([
            (first_topic, vec![info_key]),
            (second_topic, vec![info_key]),
        ]);
        let plan = SkyrimDialoguePlan::derive(&records, &topology, &interner).unwrap();
        assert!(
            plan.unsupported_reason()
                .unwrap()
                .contains("multiple parents")
        );
    }

    #[test]
    fn fragment_and_voice_dependencies_are_explicit_and_fail_closed() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FixtureSource.esm");
        let quest = fk(1, plugin);
        let speaker = fk(2, plugin);
        let topic_key = fk(3, plugin);
        let info_key = fk(4, plugin);
        let topic = minimal_topic(topic_key, quest, 1, &interner);

        let mut fragment_info = minimal_info(info_key, speaker, &interner);
        fragment_info
            .fields
            .push(field(b"VMAD", FieldValue::Bytes(SmallVec::new())));
        let plan = SkyrimDialoguePlan::derive(
            &[topic.clone(), fragment_info],
            &HashMap::from([(topic_key, vec![info_key])]),
            &interner,
        )
        .unwrap();
        assert!(
            plan.unsupported_reason()
                .unwrap()
                .contains("fragment semantics")
        );
        let receipt = plan.preflight_receipt();
        assert_eq!(receipt.fragment_intents.len(), 1);
        assert_eq!(
            receipt.fragment_intents[0].source_fields,
            vec![SubrecordSig(*b"VMAD")]
        );

        let mut voice_info = minimal_info(info_key, speaker, &interner);
        voice_info
            .fields
            .push(form_key_field(b"ONAM", fk(5, plugin)));
        let plan = SkyrimDialoguePlan::derive(
            &[topic, voice_info],
            &HashMap::from([(topic_key, vec![info_key])]),
            &interner,
        )
        .unwrap();
        assert!(
            plan.unsupported_reason()
                .unwrap()
                .contains("sound/voice semantics")
        );

        let mut sound_info = minimal_info(info_key, speaker, &interner);
        let response = sound_info
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == *b"TRDT")
            .unwrap();
        let FieldValue::Struct(fields) = &mut response.value else {
            unreachable!()
        };
        fields
            .iter_mut()
            .find(|(name, _)| interner.resolve(*name) == Some("sound"))
            .unwrap()
            .1 = FieldValue::FormKey(fk(6, plugin));
        let plan = SkyrimDialoguePlan::derive(
            &[minimal_topic(topic_key, quest, 1, &interner), sound_info],
            &HashMap::from([(topic_key, vec![info_key])]),
            &interner,
        )
        .unwrap();
        assert!(
            plan.unsupported_reason()
                .unwrap()
                .contains("explicit response sound")
        );
    }

    #[test]
    fn non_text_localization_rejects_the_component() {
        let interner = StringInterner::new();
        let plugin = interner.intern("FixtureSource.esm");
        let quest = fk(1, plugin);
        let speaker = fk(2, plugin);
        let topic_key = fk(3, plugin);
        let info_key = fk(4, plugin);
        let topic = minimal_topic(topic_key, quest, 1, &interner);
        let mut info = minimal_info(info_key, speaker, &interner);
        info.fields.push(field(b"RNAM", FieldValue::Uint(1)));
        let plan = SkyrimDialoguePlan::derive(
            &[topic, info],
            &HashMap::from([(topic_key, vec![info_key])]),
            &interner,
        )
        .unwrap();
        assert!(
            plan.unsupported_reason()
                .unwrap()
                .contains("RNAM is not text")
        );
    }
}
