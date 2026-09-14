//! Attach converter-owned Papyrus adapters to copied placed records.
//!
//! Policy is declarative and pair-scoped. The engine resolves source FormKeys
//! through the conversion mapper, verifies the copied record and its exact CELL
//! topology in place, and only then appends an idempotent VMAD script binding.

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, ParsedSubrecord, WriteEffect, build_vmad_bytes_from_payload,
};

use crate::fixups::{FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};
use crate::translator::Game;

const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;
const CELL_CHILD_GROUP: i32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellSection {
    Persistent,
    Temporary,
}

impl CellSection {
    fn group_type(self) -> i32 {
        match self {
            Self::Persistent => 8,
            Self::Temporary => 9,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Persistent => "Persistent",
            Self::Temporary => "Temporary",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RecordRequirement {
    pub signature: &'static str,
    pub local: u32,
}

impl RecordRequirement {
    pub(crate) const fn new(signature: &'static str, local: u32) -> Self {
        Self { signature, local }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ScriptAdapter {
    StoryEventOnTriggerEnter {
        target_quest: RecordRequirement,
        story_event_keyword: RecordRequirement,
        stage_to_set: i32,
    },
    StoryEventOnActivateStartScene {
        target_quest: RecordRequirement,
        story_event_keyword: RecordRequirement,
        scene_to_start: RecordRequirement,
    },
    BindExistingPlacedObject {
        script_name: &'static str,
        property_name: &'static str,
        target: RecordRequirement,
        target_parent_cell: RecordRequirement,
        target_section: CellSection,
        target_base: RecordRequirement,
    },
}

impl ScriptAdapter {
    fn script_name(self) -> &'static str {
        match self {
            Self::StoryEventOnTriggerEnter { .. } => "B21:StoryEventOnTriggerEnter",
            Self::StoryEventOnActivateStartScene { .. } => "B21:StoryEventOnActivateStartScene",
            Self::BindExistingPlacedObject { script_name, .. } => script_name,
        }
    }

    fn requirements(self) -> [Option<(&'static str, RecordRequirement)>; 3] {
        match self {
            Self::StoryEventOnTriggerEnter {
                target_quest,
                story_event_keyword,
                ..
            } => [
                Some(("TargetQuest", target_quest)),
                Some(("StoryEventKeyword", story_event_keyword)),
                None,
            ],
            Self::StoryEventOnActivateStartScene {
                target_quest,
                story_event_keyword,
                scene_to_start,
            } => [
                Some(("TargetQuest", target_quest)),
                Some(("StoryEventKeyword", story_event_keyword)),
                Some(("SceneToStart", scene_to_start)),
            ],
            Self::BindExistingPlacedObject { .. } => [None, None, None],
        }
    }

    fn placed_object_binding(
        self,
    ) -> Option<(
        &'static str,
        RecordRequirement,
        RecordRequirement,
        CellSection,
        RecordRequirement,
    )> {
        match self {
            Self::BindExistingPlacedObject {
                property_name,
                target,
                target_parent_cell,
                target_section,
                target_base,
                ..
            } => Some((
                property_name,
                target,
                target_parent_cell,
                target_section,
                target_base,
            )),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PlacedRecordVmadPolicy {
    pub label: &'static str,
    pub source_plugin: &'static str,
    pub placed: RecordRequirement,
    pub parent_cell: RecordRequirement,
    pub section: CellSection,
    pub base: RecordRequirement,
    pub reference_type: Option<RecordRequirement>,
    pub adapter: ScriptAdapter,
    pub evidence: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlacementContext {
    parent_cell_raw: u32,
    section: CellSection,
    section_label_raw: u32,
}

#[derive(Debug, PartialEq, Eq)]
struct PlacementInspection {
    contexts: Vec<PlacementContext>,
    placed_identity_count: usize,
    parent_cell_identity_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResolvedRecord {
    form_key: FormKey,
    raw_form_id: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum AttachResult {
    Changed,
    AlreadyPresent,
    Conflict(&'static str),
}

fn warning(
    report: &mut FixupReport,
    interner: &StringInterner,
    policy: &PlacedRecordVmadPolicy,
    reason: impl std::fmt::Display,
) {
    report.warnings.push(interner.intern(&format!(
        "placed_record_vmad:{}:skipped:{}; evidence={}",
        policy.label, reason, policy.evidence
    )));
}

fn diagnostic(
    report: &mut FixupReport,
    interner: &StringInterner,
    policy: &PlacedRecordVmadPolicy,
    outcome: &str,
) {
    report
        .diagnostics
        .push(interner.intern(&format!("placed_record_vmad:{}:{outcome}", policy.label)));
}

fn signature_code(signature: &str) -> Result<SigCode, FixupError> {
    SigCode::from_str(signature).map_err(|error| FixupError::SchemaError(error.to_string()))
}

fn mapped_form_key(
    mapper: &FormKeyMapper,
    source_plugin: Sym,
    requirement: RecordRequirement,
) -> Option<FormKey> {
    mapper.lookup(FormKey {
        local: requirement.local,
        plugin: source_plugin,
    })
}

fn resolve_mapped_record(
    session: &mut PluginSession,
    mapper: &FormKeyMapper,
    target_plugin: Sym,
    source_plugin: Sym,
    requirement: RecordRequirement,
) -> Result<Result<ResolvedRecord, String>, FixupError> {
    let Some(form_key) = mapped_form_key(mapper, source_plugin, requirement) else {
        return Ok(Err(format!(
            "unmapped_source:{}:{:06X}",
            requirement.signature, requirement.local
        )));
    };
    if form_key.plugin != target_plugin {
        return Ok(Err(format!(
            "mapped_outside_output:{}:{:06X}",
            requirement.signature, requirement.local
        )));
    }

    let signature = signature_code(requirement.signature)?;
    let exists = session
        .form_keys_of_sig(signature, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?
        .into_iter()
        .any(|candidate| candidate == form_key);
    if !exists {
        return Ok(Err(format!(
            "target_absent_or_wrong_signature:{}:{:06X}",
            requirement.signature, form_key.local
        )));
    }

    let raw_form_id = session
        .raw_form_id_for_form_key(&form_key)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    Ok(Ok(ResolvedRecord {
        form_key,
        raw_form_id,
    }))
}

fn resolve_preserved_local_record(
    session: &mut PluginSession,
    mapper: &FormKeyMapper,
    target_plugin: Sym,
    requirement: RecordRequirement,
) -> Result<Result<ResolvedRecord, String>, FixupError> {
    let form_key = FormKey {
        local: requirement.local & 0x00FF_FFFF,
        plugin: target_plugin,
    };
    let signature = signature_code(requirement.signature)?;
    let matches = session
        .form_keys_of_sig(signature, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?
        .into_iter()
        .filter(|candidate| *candidate == form_key)
        .count();
    if matches != 1 {
        return Ok(Err(format!(
            "preserved_local_absent_or_ambiguous:{}:{:06X}",
            requirement.signature, form_key.local
        )));
    }
    let raw_form_id = session
        .raw_form_id_for_form_key(&form_key)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    Ok(Ok(ResolvedRecord {
        form_key,
        raw_form_id,
    }))
}

fn resolve_projected_placement_identity(
    session: &mut PluginSession,
    mapper: &FormKeyMapper,
    target_plugin: Sym,
    source_plugin: Sym,
    requirement: RecordRequirement,
) -> Result<Result<ResolvedRecord, String>, FixupError> {
    if mapped_form_key(mapper, source_plugin, requirement).is_some() {
        resolve_mapped_record(session, mapper, target_plugin, source_plugin, requirement)
    } else {
        resolve_preserved_local_record(session, mapper, target_plugin, requirement)
    }
}

fn exact_single_form_id(record: &ParsedRecord, signature: &str, expected: u32) -> bool {
    let matching: Vec<_> = record
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.signature.as_str() == signature)
        .collect();
    if matching.is_empty()
        || matching
            .iter()
            .any(|subrecord| subrecord.data.is_empty() || subrecord.data.len() % 4 != 0)
    {
        return false;
    }
    let mut values = matching
        .into_iter()
        .flat_map(|subrecord| subrecord.data.chunks_exact(4))
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte chunk")));
    values.next() == Some(expected) && values.next().is_none()
}

fn has_no_form_id(record: &ParsedRecord, signature: &str) -> bool {
    !record
        .subrecords
        .iter()
        .any(|subrecord| subrecord.signature.as_str() == signature)
}

fn inspect_placement(
    items: &[ParsedItem],
    placed_raw: u32,
    placed_signature: &str,
    parent_cell_raw: u32,
) -> PlacementInspection {
    fn walk(
        items: &[ParsedItem],
        placed_raw: u32,
        placed_signature: &str,
        expected_parent_cell_raw: u32,
        parent_cell_raw: Option<u32>,
        section: Option<(CellSection, u32)>,
        inspection: &mut PlacementInspection,
    ) {
        for item in items {
            match item {
                ParsedItem::Group(group) => {
                    let label = u32::from_le_bytes(group.label);
                    let next_cell = if group.group_type == CELL_CHILD_GROUP {
                        Some(label)
                    } else {
                        parent_cell_raw
                    };
                    let next_section = match (group.group_type, next_cell) {
                        (8, Some(_)) => Some((CellSection::Persistent, label)),
                        (9, Some(_)) => Some((CellSection::Temporary, label)),
                        _ if group.group_type == CELL_CHILD_GROUP => None,
                        _ => section,
                    };
                    walk(
                        &group.children,
                        placed_raw,
                        placed_signature,
                        expected_parent_cell_raw,
                        next_cell,
                        next_section,
                        inspection,
                    );
                }
                ParsedItem::Record(record) => {
                    if record.form_id == placed_raw && record.signature.as_str() == placed_signature
                    {
                        inspection.placed_identity_count += 1;
                        if let (Some(parent_cell_raw), Some((section, section_label_raw))) =
                            (parent_cell_raw, section)
                        {
                            inspection.contexts.push(PlacementContext {
                                parent_cell_raw,
                                section,
                                section_label_raw,
                            });
                        }
                    }
                    if record.form_id == expected_parent_cell_raw
                        && record.signature.as_str() == "CELL"
                    {
                        inspection.parent_cell_identity_count += 1;
                    }
                }
            }
        }
    }

    let mut inspection = PlacementInspection {
        contexts: Vec::new(),
        placed_identity_count: 0,
        parent_cell_identity_count: 0,
    };
    walk(
        items,
        placed_raw,
        placed_signature,
        parent_cell_raw,
        None,
        None,
        &mut inspection,
    );
    inspection
}

fn object_property(
    name: &str,
    form_key: FormKey,
    interner: &StringInterner,
) -> Option<serde_json::Value> {
    let plugin = interner.resolve(form_key.plugin)?;
    Some(serde_json::json!({
        "propertyName": name,
        "Type": "Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": {
            "Alias": -1,
            "FormID": {
                "reference": {
                    "plugin": plugin,
                    "object_id": format!("{:06X}", form_key.local),
                },
            },
        },
    }))
}

fn script_vmad(
    adapter: ScriptAdapter,
    resolved_properties: &[FormKey],
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let properties = match adapter {
        ScriptAdapter::StoryEventOnTriggerEnter { stage_to_set, .. } => vec![
            object_property("TargetQuest", resolved_properties[0], interner)?,
            object_property("StoryEventKeyword", resolved_properties[1], interner)?,
            serde_json::json!({
                "propertyName": "StageToSet",
                "Type": "Int32",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": stage_to_set,
            }),
        ],
        ScriptAdapter::StoryEventOnActivateStartScene { .. } => vec![
            object_property("TargetQuest", resolved_properties[0], interner)?,
            object_property("StoryEventKeyword", resolved_properties[1], interner)?,
            object_property("SceneToStart", resolved_properties[2], interner)?,
        ],
        ScriptAdapter::BindExistingPlacedObject { .. } => return None,
    };
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": adapter.script_name(),
                "Flags": 0,
                "Properties": properties,
            }],
        }),
        target_masters,
        target_plugin,
    )
}

struct VmadReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> VmadReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(count)?;
        let value = self.bytes.get(self.offset..end)?;
        self.offset = end;
        Some(value)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes = self.take(2)?;
        Some(u16::from_le_bytes(bytes.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        let bytes = self.take(4)?;
        Some(i32::from_le_bytes(bytes.try_into().ok()?))
    }

    fn string_bytes(&mut self) -> Option<&'a [u8]> {
        let length = self.u16()? as usize;
        self.take(length)
    }

    fn nonnegative_count(&mut self) -> Option<usize> {
        usize::try_from(self.i32()?).ok()
    }
}

fn skip_struct(reader: &mut VmadReader<'_>, object_format: u16) -> Option<()> {
    let count = reader.nonnegative_count()?;
    for _ in 0..count {
        reader.string_bytes()?;
        let property_type = reader.u8()?;
        reader.u8()?;
        skip_property_value(reader, property_type, object_format)?;
    }
    Some(())
}

fn skip_property_value(
    reader: &mut VmadReader<'_>,
    property_type: u8,
    object_format: u16,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => {
            if object_format != 1 && object_format != 2 {
                return None;
            }
            reader.take(8)?;
            Some(())
        }
        2 => {
            reader.string_bytes()?;
            Some(())
        }
        3 | 4 => {
            reader.take(4)?;
            Some(())
        }
        5 => {
            reader.take(1)?;
            Some(())
        }
        7 => skip_struct(reader, object_format),
        11 => {
            let count = reader.nonnegative_count()?;
            reader.take(count.checked_mul(8)?)?;
            Some(())
        }
        12 => {
            let count = reader.nonnegative_count()?;
            for _ in 0..count {
                reader.string_bytes()?;
            }
            Some(())
        }
        13 | 14 => {
            let count = reader.nonnegative_count()?;
            reader.take(count.checked_mul(4)?)?;
            Some(())
        }
        15 => {
            let count = reader.nonnegative_count()?;
            reader.take(count)?;
            Some(())
        }
        16 => {
            reader.take(4)?;
            Some(())
        }
        17 => {
            let count = reader.nonnegative_count()?;
            for _ in 0..count {
                skip_struct(reader, object_format)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn script_spans<'a>(bytes: &'a [u8], script_name: &str) -> Option<(u16, Vec<&'a [u8]>)> {
    let mut reader = VmadReader::new(bytes);
    let version = reader.u16()?;
    let object_format = reader.u16()?;
    if version != VMAD_VERSION || object_format != VMAD_OBJECT_FORMAT {
        return None;
    }
    let script_count = reader.u16()?;
    let mut matching = Vec::new();
    for _ in 0..script_count {
        let start = reader.offset;
        let name = reader.string_bytes()?;
        reader.u8()?;
        let property_count = reader.u16()?;
        for _ in 0..property_count {
            reader.string_bytes()?;
            let property_type = reader.u8()?;
            reader.u8()?;
            skip_property_value(&mut reader, property_type, object_format)?;
        }
        if name == script_name.as_bytes() {
            matching.push(&bytes[start..reader.offset]);
        }
    }
    (reader.offset == bytes.len()).then_some((script_count, matching))
}

fn attach_script(record: &mut ParsedRecord, script_name: &str, script_vmad: &[u8]) -> AttachResult {
    if script_vmad.len() < 6 {
        return AttachResult::Conflict("generated_vmad_invalid");
    }

    let vmad_indices: Vec<_> = record
        .subrecords
        .iter()
        .enumerate()
        .filter_map(|(index, subrecord)| (subrecord.signature.as_str() == "VMAD").then_some(index))
        .collect();
    if vmad_indices.len() > 1 {
        return AttachResult::Conflict("multiple_vmad_subrecords");
    }

    if let Some(index) = vmad_indices.first().copied() {
        let existing = record.subrecords[index].data.as_ref();
        let Some((script_count, matching)) = script_spans(existing, script_name) else {
            return AttachResult::Conflict("unsupported_or_malformed_vmad");
        };
        match matching.as_slice() {
            [] => {}
            [existing_script] if *existing_script == &script_vmad[6..] => {
                return AttachResult::AlreadyPresent;
            }
            [_] => return AttachResult::Conflict("same_script_different_binding"),
            _ => return AttachResult::Conflict("duplicate_script_binding"),
        }
        let Some(next_count) = script_count.checked_add(1) else {
            return AttachResult::Conflict("script_count_overflow");
        };
        let mut bytes = existing.to_vec();
        bytes[4..6].copy_from_slice(&next_count.to_le_bytes());
        bytes.extend_from_slice(&script_vmad[6..]);
        record.subrecords[index].data = Bytes::from(bytes);
        record.raw_payload = None;
        return AttachResult::Changed;
    }

    let insert_at = record
        .subrecords
        .iter()
        .position(|subrecord| subrecord.signature.as_str() == "NAME")
        .unwrap_or(record.subrecords.len());
    record.subrecords.insert(
        insert_at,
        ParsedSubrecord {
            signature: "VMAD".into(),
            data: Bytes::copy_from_slice(script_vmad),
            semantic_type: None,
        },
    );
    record.raw_payload = None;
    AttachResult::Changed
}

fn attach_object_property(
    record: &mut ParsedRecord,
    script_name: &str,
    property_name: &str,
    target: FormKey,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> AttachResult {
    let vmad_indices: Vec<_> = record
        .subrecords
        .iter()
        .enumerate()
        .filter_map(|(index, subrecord)| (subrecord.signature.as_str() == "VMAD").then_some(index))
        .collect();
    let [index] = vmad_indices.as_slice() else {
        return AttachResult::Conflict(if vmad_indices.is_empty() {
            "missing_vmad"
        } else {
            "multiple_vmad_subrecords"
        });
    };

    let existing = record.subrecords[*index].data.as_ref();
    let Some(mut payload) = compact_vmad_payload_json(
        existing,
        target_masters,
        target_plugin,
        Some(record.signature.as_str()),
    ) else {
        return AttachResult::Conflict("unsupported_or_malformed_vmad");
    };
    let Some(scripts) = payload
        .get_mut("Scripts")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return AttachResult::Conflict("missing_scripts_array");
    };
    let matching_scripts: Vec<_> = scripts
        .iter()
        .enumerate()
        .filter_map(|(index, script)| {
            (script.get("ScriptName").and_then(serde_json::Value::as_str) == Some(script_name))
                .then_some(index)
        })
        .collect();
    let [script_index] = matching_scripts.as_slice() else {
        return AttachResult::Conflict(if matching_scripts.is_empty() {
            "missing_target_script"
        } else {
            "duplicate_script_binding"
        });
    };
    let script = &mut scripts[*script_index];
    let Some(properties) = script
        .get_mut("Properties")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return AttachResult::Conflict("missing_properties_array");
    };
    let Some(desired) = object_property(property_name, target, interner) else {
        return AttachResult::Conflict("unresolved_property_target_plugin");
    };
    let matching_properties: Vec<_> = properties
        .iter()
        .filter(|property| {
            property
                .get("propertyName")
                .and_then(serde_json::Value::as_str)
                == Some(property_name)
        })
        .collect();
    match matching_properties.as_slice() {
        [] => properties.push(desired),
        [existing_property] if **existing_property == desired => {
            return AttachResult::AlreadyPresent;
        }
        [_] => return AttachResult::Conflict("same_property_different_binding"),
        _ => return AttachResult::Conflict("duplicate_property_binding"),
    }

    let Some(encoded) = build_vmad_bytes_from_payload(&payload, target_masters, target_plugin)
    else {
        return AttachResult::Conflict("vmad_encode_failed");
    };
    record.subrecords[*index].data = Bytes::from(encoded);
    record.raw_payload = None;
    AttachResult::Changed
}

fn apply_policy(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    policy: &PlacedRecordVmadPolicy,
    report: &mut FixupReport,
) -> Result<(), FixupError> {
    let interner = mapper.interner;
    let source_plugin = interner.intern(policy.source_plugin);
    let target_plugin_name = session.target_slot().parsed.plugin_name.clone();
    let target_plugin = interner.intern(&target_plugin_name);

    let placed = match resolve_projected_placement_identity(
        session,
        mapper,
        target_plugin,
        source_plugin,
        policy.placed,
    )? {
        Ok(record) => record,
        Err(reason) => {
            warning(report, interner, policy, reason);
            return Ok(());
        }
    };

    let parent_cell = match resolve_projected_placement_identity(
        session,
        mapper,
        target_plugin,
        source_plugin,
        policy.parent_cell,
    )? {
        Ok(record) => record,
        Err(reason) => {
            warning(report, interner, policy, reason);
            return Ok(());
        }
    };
    let base =
        match resolve_mapped_record(session, mapper, target_plugin, source_plugin, policy.base)? {
            Ok(record) => record,
            Err(reason) => {
                warning(report, interner, policy, reason);
                return Ok(());
            }
        };
    let reference_type = if let Some(requirement) = policy.reference_type {
        match resolve_mapped_record(session, mapper, target_plugin, source_plugin, requirement)? {
            Ok(record) => Some(record),
            Err(reason) => {
                warning(report, interner, policy, reason);
                return Ok(());
            }
        }
    } else {
        None
    };

    let mut resolved_properties = Vec::new();
    for (property_name, requirement) in policy.adapter.requirements().into_iter().flatten() {
        match resolve_mapped_record(session, mapper, target_plugin, source_plugin, requirement)? {
            Ok(record) => resolved_properties.push(record.form_key),
            Err(reason) => {
                warning(
                    report,
                    interner,
                    policy,
                    format!("property:{property_name}:{reason}"),
                );
                return Ok(());
            }
        }
    }

    let placed_object_target = if let Some((
        property_name,
        target_requirement,
        target_parent_requirement,
        target_section,
        target_base_requirement,
    )) = policy.adapter.placed_object_binding()
    {
        let target = match resolve_projected_placement_identity(
            session,
            mapper,
            target_plugin,
            source_plugin,
            target_requirement,
        )? {
            Ok(record) => record,
            Err(reason) => {
                warning(
                    report,
                    interner,
                    policy,
                    format!("property:{property_name}:{reason}"),
                );
                return Ok(());
            }
        };
        let target_parent = match resolve_projected_placement_identity(
            session,
            mapper,
            target_plugin,
            source_plugin,
            target_parent_requirement,
        )? {
            Ok(record) => record,
            Err(reason) => {
                warning(
                    report,
                    interner,
                    policy,
                    format!("property:{property_name}:parent:{reason}"),
                );
                return Ok(());
            }
        };
        let target_base = match resolve_mapped_record(
            session,
            mapper,
            target_plugin,
            source_plugin,
            target_base_requirement,
        )? {
            Ok(record) => record,
            Err(reason) => {
                warning(
                    report,
                    interner,
                    policy,
                    format!("property:{property_name}:base:{reason}"),
                );
                return Ok(());
            }
        };
        let target_inspection = inspect_placement(
            &session.target_slot().parsed.root_items,
            target.raw_form_id,
            target_requirement.signature,
            target_parent.raw_form_id,
        );
        let expected_target_context = PlacementContext {
            parent_cell_raw: target_parent.raw_form_id,
            section: target_section,
            section_label_raw: target_parent.raw_form_id,
        };
        if target_inspection.placed_identity_count != 1
            || target_inspection.parent_cell_identity_count != 1
            || target_inspection.contexts.as_slice() != [expected_target_context]
        {
            warning(
                report,
                interner,
                policy,
                format!(
                    "property:{property_name}:topology_mismatch:placed={};parent_cell={};actual={:?}",
                    target_inspection.placed_identity_count,
                    target_inspection.parent_cell_identity_count,
                    target_inspection.contexts,
                ),
            );
            return Ok(());
        }
        {
            let target_record = session
                .record(target.raw_form_id)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            if !exact_single_form_id(target_record, "NAME", target_base.raw_form_id) {
                warning(
                    report,
                    interner,
                    policy,
                    format!(
                        "property:{property_name}:base_mismatch:expected={:08X}",
                        target_base.raw_form_id
                    ),
                );
                return Ok(());
            }
        }
        Some((property_name, target.form_key))
    } else {
        None
    };

    let inspection = inspect_placement(
        &session.target_slot().parsed.root_items,
        placed.raw_form_id,
        policy.placed.signature,
        parent_cell.raw_form_id,
    );
    if inspection.placed_identity_count != 1 || inspection.parent_cell_identity_count != 1 {
        warning(
            report,
            interner,
            policy,
            format!(
                "physical_identity_mismatch:placed={};parent_cell={}",
                inspection.placed_identity_count, inspection.parent_cell_identity_count
            ),
        );
        return Ok(());
    }
    let expected_context = PlacementContext {
        parent_cell_raw: parent_cell.raw_form_id,
        section: policy.section,
        section_label_raw: parent_cell.raw_form_id,
    };
    if inspection.contexts.as_slice() != [expected_context] {
        warning(
            report,
            interner,
            policy,
            format!(
                "topology_mismatch:expected=CELL:{:08X}/{}({});actual={:?}",
                parent_cell.raw_form_id,
                policy.section.label(),
                policy.section.group_type(),
                inspection.contexts,
            ),
        );
        return Ok(());
    }

    {
        let record = session
            .record(placed.raw_form_id)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if !exact_single_form_id(record, "NAME", base.raw_form_id) {
            warning(
                report,
                interner,
                policy,
                format!("base_mismatch:expected={:08X}", base.raw_form_id),
            );
            return Ok(());
        }
        match reference_type {
            Some(reference_type)
                if !exact_single_form_id(record, "XLRT", reference_type.raw_form_id) =>
            {
                warning(
                    report,
                    interner,
                    policy,
                    format!(
                        "reference_type_mismatch:expected={:08X}",
                        reference_type.raw_form_id
                    ),
                );
                return Ok(());
            }
            None if !has_no_form_id(record, "XLRT") => {
                warning(report, interner, policy, "unexpected_reference_type");
                return Ok(());
            }
            _ => {}
        }
    }

    let outcome = {
        let target_masters = session.target_masters().to_vec();
        let record = session
            .record_mut(placed.raw_form_id)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if let Some((property_name, target)) = placed_object_target {
            attach_object_property(
                record,
                policy.adapter.script_name(),
                property_name,
                target,
                &target_masters,
                &target_plugin_name,
                interner,
            )
        } else {
            let script_vmad = script_vmad(
                policy.adapter,
                &resolved_properties,
                &target_masters,
                &target_plugin_name,
                interner,
            )
            .ok_or_else(|| {
                FixupError::SchemaError(format!(
                    "placed_record_vmad:{}:failed to encode {}",
                    policy.label,
                    policy.adapter.script_name()
                ))
            })?;
            attach_script(record, policy.adapter.script_name(), &script_vmad)
        }
    };
    match outcome {
        AttachResult::Changed => {
            session.target_slot_mut().clear_record_count_cache();
            session.record_effect(WriteEffect::RecordContents {
                form_ids: smallvec::smallvec![placed.raw_form_id],
            });
            report.records_changed = report.records_changed.saturating_add(1);
            diagnostic(report, interner, policy, "attached");
        }
        AttachResult::AlreadyPresent => {
            diagnostic(report, interner, policy, "already_present");
        }
        AttachResult::Conflict(reason) => {
            warning(
                report,
                interner,
                policy,
                format!(
                    "vmad_conflict:{reason}:script={}",
                    policy.adapter.script_name()
                ),
            );
        }
    }

    Ok(())
}

pub fn apply_fo76_fo4_placed_record_vmad_catalog(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    source: Game,
    target: Game,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    if (source, target) != (Game::Fo76, Game::Fo4) {
        return Ok(report);
    }
    for policy in super::placed_record_vmad_catalog::FO76_TO_FO4_POLICIES {
        apply_policy(session, mapper, policy, &mut report)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, plugin_handle_add_master_native, plugin_handle_close_native,
        plugin_handle_load_no_py, plugin_handle_new_native,
        plugin_handle_read_authoring_record_value_json, plugin_handle_save_no_py,
        plugin_handle_store_ref,
    };

    use crate::fixups::FixupConfig;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::schema::AuthoringSchema;

    const TARGET_PLUGIN: &str = "PlacedRecordVmadFixture.esp";
    const OWN_INDEX: u32 = 2;

    fn trigger_policy() -> &'static PlacedRecordVmadPolicy {
        &super::super::placed_record_vmad_catalog::FO76_TO_FO4_POLICIES[0]
    }

    fn activation_policy() -> &'static PlacedRecordVmadPolicy {
        &super::super::placed_record_vmad_catalog::FO76_TO_FO4_POLICIES[1]
    }

    fn interior_policy() -> &'static PlacedRecordVmadPolicy {
        &super::super::placed_record_vmad_catalog::FO76_TO_FO4_POLICIES[2]
    }

    fn mine_collapse_policy() -> &'static PlacedRecordVmadPolicy {
        &super::super::placed_record_vmad_catalog::FO76_TO_FO4_POLICIES[3]
    }

    fn own(local: u32) -> u32 {
        (OWN_INDEX << 24) | local
    }

    fn subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn record(signature: &str, local: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        ParsedRecord {
            signature: signature.into(),
            form_id: own(local),
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn group(group_type: i32, label: u32, children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label: label.to_le_bytes(),
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    #[derive(Clone)]
    struct PolicyFixtureOptions {
        actual_parent_local: Option<u32>,
        actual_section: Option<CellSection>,
        base_local: Option<u32>,
        reference_type_local: Option<Option<u32>>,
        omitted_target: Option<RecordRequirement>,
        existing_vmad: Option<Vec<u8>>,
        bound_target_parent_local: Option<u32>,
        bound_target_section: Option<CellSection>,
        bound_target_base_local: Option<u32>,
    }

    impl Default for PolicyFixtureOptions {
        fn default() -> Self {
            Self {
                actual_parent_local: None,
                actual_section: None,
                base_local: None,
                reference_type_local: None,
                omitted_target: None,
                existing_vmad: None,
                bound_target_parent_local: None,
                bound_target_section: None,
                bound_target_base_local: None,
            }
        }
    }

    struct Fixture {
        target: u64,
        masters: [u64; 2],
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            plugin_handle_close_native(self.target);
            for master in self.masters {
                plugin_handle_close_native(master);
            }
        }
    }

    fn requirements(policy: &PlacedRecordVmadPolicy) -> Vec<RecordRequirement> {
        let mut values = vec![policy.placed, policy.parent_cell, policy.base];
        if let Some(reference_type) = policy.reference_type {
            values.push(reference_type);
        }
        values.extend(
            policy
                .adapter
                .requirements()
                .into_iter()
                .flatten()
                .map(|(_, requirement)| requirement),
        );
        if let ScriptAdapter::BindExistingPlacedObject {
            target,
            target_parent_cell,
            target_base,
            ..
        } = policy.adapter
        {
            values.extend([target, target_parent_cell, target_base]);
        }
        values
    }

    fn fixture(policies: &[&PlacedRecordVmadPolicy], options: &[PolicyFixtureOptions]) -> Fixture {
        assert_eq!(policies.len(), options.len());
        let master_a = plugin_handle_new_native("MasterA.esm", Some("fo4")).unwrap();
        let master_b = plugin_handle_new_native("MasterB.esm", Some("fo4")).unwrap();
        let target = plugin_handle_new_native(TARGET_PLUGIN, Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "MasterA.esm", None).unwrap();
        plugin_handle_add_master_native(target, "MasterB.esm", None).unwrap();

        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target).unwrap();
            let mut inserted = Vec::new();
            for (policy, options) in policies.iter().zip(options) {
                let placed_object_binding = policy.adapter.placed_object_binding();
                for requirement in requirements(policy) {
                    if requirement == policy.placed
                        || placed_object_binding
                            .is_some_and(|(_, target, _, _, _)| requirement == target)
                        || Some(requirement) == options.omitted_target
                        || inserted.contains(&requirement)
                    {
                        continue;
                    }
                    slot.parsed.root_items.push(ParsedItem::Record(record(
                        requirement.signature,
                        requirement.local,
                        vec![],
                    )));
                    inserted.push(requirement);
                }

                let actual_parent = options
                    .actual_parent_local
                    .unwrap_or(policy.parent_cell.local);
                let actual_section = options.actual_section.unwrap_or(policy.section);
                let base = options.base_local.unwrap_or(policy.base.local);
                let mut subrecords = vec![subrecord("NAME", own(base).to_le_bytes().to_vec())];
                match options
                    .reference_type_local
                    .unwrap_or(policy.reference_type.map(|requirement| requirement.local))
                {
                    Some(reference_type) => subrecords.push(subrecord(
                        "XLRT",
                        own(reference_type).to_le_bytes().to_vec(),
                    )),
                    None => {}
                }
                if let Some(vmad) = &options.existing_vmad {
                    subrecords.insert(0, subrecord("VMAD", vmad.clone()));
                }
                subrecords.push(subrecord("DATA", vec![0; 24]));
                let placed = ParsedItem::Record(record(
                    policy.placed.signature,
                    policy.placed.local,
                    subrecords,
                ));
                slot.parsed.root_items.push(group(
                    CELL_CHILD_GROUP,
                    own(actual_parent),
                    vec![group(
                        actual_section.group_type(),
                        own(actual_parent),
                        vec![placed],
                    )],
                ));

                if let Some((_, target, target_parent, target_section, target_base)) =
                    placed_object_binding
                {
                    let actual_target_parent = options
                        .bound_target_parent_local
                        .unwrap_or(target_parent.local);
                    let actual_target_section =
                        options.bound_target_section.unwrap_or(target_section);
                    let actual_target_base =
                        options.bound_target_base_local.unwrap_or(target_base.local);
                    let target_record = ParsedItem::Record(record(
                        target.signature,
                        target.local,
                        vec![
                            subrecord("NAME", own(actual_target_base).to_le_bytes().to_vec()),
                            subrecord("DATA", vec![0; 24]),
                        ],
                    ));
                    slot.parsed.root_items.push(group(
                        CELL_CHILD_GROUP,
                        own(actual_target_parent),
                        vec![group(
                            actual_target_section.group_type(),
                            own(actual_target_parent),
                            vec![target_record],
                        )],
                    ));
                }
            }
            slot.clear_record_count_cache();
        }

        Fixture {
            target,
            masters: [master_a, master_b],
        }
    }

    fn edit_placed_record(
        fixture: &Fixture,
        policy: &PlacedRecordVmadPolicy,
        edit: &mut dyn FnMut(&mut ParsedRecord),
    ) {
        fn walk(
            items: &mut [ParsedItem],
            raw_form_id: u32,
            edit: &mut dyn FnMut(&mut ParsedRecord),
        ) -> usize {
            items
                .iter_mut()
                .map(|item| match item {
                    ParsedItem::Group(group) => walk(&mut group.children, raw_form_id, edit),
                    ParsedItem::Record(record) if record.form_id == raw_form_id => {
                        edit(record);
                        1
                    }
                    ParsedItem::Record(_) => 0,
                })
                .sum()
        }

        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&fixture.target).unwrap();
        assert_eq!(
            walk(&mut slot.parsed.root_items, own(policy.placed.local), edit),
            1
        );
        slot.clear_record_count_cache();
    }

    fn mapper_state() -> MapperState {
        MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: TARGET_PLUGIN.to_string(),
                target_master_names: vec!["MasterA.esm".to_string(), "MasterB.esm".to_string()],
                ..Default::default()
            },
        )
    }

    fn add_mappings(mapper: &mut FormKeyMapper, policies: &[&PlacedRecordVmadPolicy]) {
        add_mappings_except(mapper, policies, &[]);
    }

    fn add_mappings_except(
        mapper: &mut FormKeyMapper,
        policies: &[&PlacedRecordVmadPolicy],
        excluded: &[RecordRequirement],
    ) {
        let target_plugin = mapper.interner.intern(TARGET_PLUGIN);
        for policy in policies {
            let source_plugin = mapper.interner.intern(policy.source_plugin);
            for requirement in requirements(policy) {
                if excluded.contains(&requirement) {
                    continue;
                }
                mapper.add_mapping(
                    FormKey {
                        local: requirement.local,
                        plugin: source_plugin,
                    },
                    FormKey {
                        local: requirement.local,
                        plugin: target_plugin,
                    },
                );
            }
        }
    }

    fn apply_one(
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        policy: &PlacedRecordVmadPolicy,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        apply_policy(session, mapper, policy, &mut report)?;
        Ok(report)
    }

    fn vmad_bytes_with_interner(
        session: &mut PluginSession,
        policy: &PlacedRecordVmadPolicy,
        interner: &StringInterner,
    ) -> Option<Vec<u8>> {
        vmad_bytes_at_local(session, policy.placed.local, interner)
    }

    fn vmad_bytes_at_local(
        session: &mut PluginSession,
        local: u32,
        interner: &StringInterner,
    ) -> Option<Vec<u8>> {
        session
            .first_subrecord_bytes(
                &FormKey {
                    local,
                    plugin: interner.intern(TARGET_PLUGIN),
                },
                "VMAD",
            )
            .unwrap()
    }

    fn topology(session: &PluginSession, policy: &PlacedRecordVmadPolicy) -> Vec<PlacementContext> {
        inspect_placement(
            &session.target_slot().parsed.root_items,
            own(policy.placed.local),
            policy.placed.signature,
            own(policy.parent_cell.local),
        )
        .contexts
    }

    fn valid_unrelated_vmad() -> Vec<u8> {
        build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [{
                    "ScriptName": "B21:ExistingPlacedAdapter",
                    "Properties": [{
                        "propertyName": "ExistingValue",
                        "Type": "Int32",
                        "Flags": VMAD_PROPERTY_FLAG_EDITED,
                        "Value": 17,
                    }],
                }],
            }),
            &["MasterA.esm".to_string(), "MasterB.esm".to_string()],
            TARGET_PLUGIN,
        )
        .unwrap()
    }

    fn mine_collapse_empty_vmad() -> Vec<u8> {
        build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [{
                    "ScriptName": "Vault79MineCollapseScript",
                    "Properties": [],
                }],
            }),
            &["MasterA.esm".to_string(), "MasterB.esm".to_string()],
            TARGET_PLUGIN,
        )
        .unwrap()
    }

    #[derive(Debug, PartialEq, Eq)]
    enum ParsedValue {
        Object(u32),
        Int(i32),
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ParsedProperty {
        name: String,
        property_type: u8,
        flags: u8,
        value: ParsedValue,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct ParsedScript {
        name: String,
        flags: u8,
        properties: Vec<ParsedProperty>,
    }

    fn parsed_manifest(bytes: &[u8]) -> Option<(u16, u16, Vec<ParsedScript>)> {
        let mut reader = VmadReader::new(bytes);
        let version = reader.u16()?;
        let object_format = reader.u16()?;
        let script_count = reader.u16()?;
        let mut scripts = Vec::new();
        for _ in 0..script_count {
            let name = String::from_utf8(reader.string_bytes()?.to_vec()).ok()?;
            let flags = reader.u8()?;
            let property_count = reader.u16()?;
            let mut properties = Vec::new();
            for _ in 0..property_count {
                let property_name = String::from_utf8(reader.string_bytes()?.to_vec()).ok()?;
                let property_type = reader.u8()?;
                let property_flags = reader.u8()?;
                let value = match property_type {
                    1 if object_format == VMAD_OBJECT_FORMAT => {
                        reader.take(4)?;
                        let bytes: [u8; 4] = reader.take(4)?.try_into().ok()?;
                        ParsedValue::Object(u32::from_le_bytes(bytes))
                    }
                    3 => ParsedValue::Int(reader.i32()?),
                    _ => return None,
                };
                properties.push(ParsedProperty {
                    name: property_name,
                    property_type,
                    flags: property_flags,
                    value,
                });
            }
            scripts.push(ParsedScript {
                name,
                flags,
                properties,
            });
        }
        (reader.offset == bytes.len()).then_some((version, object_format, scripts))
    }

    fn warning_text(report: &FixupReport, interner: &StringInterner) -> Vec<String> {
        report
            .warnings
            .iter()
            .filter_map(|warning| interner.resolve(*warning).map(str::to_string))
            .collect()
    }

    fn resolved_property_form_keys(
        policy: &PlacedRecordVmadPolicy,
        interner: &StringInterner,
    ) -> Vec<FormKey> {
        let plugin = interner.intern(TARGET_PLUGIN);
        policy
            .adapter
            .requirements()
            .into_iter()
            .flatten()
            .map(|(_, requirement)| FormKey {
                local: requirement.local,
                plugin,
            })
            .collect()
    }

    #[test]
    fn mine_collapse_policy_binds_the_same_cell_persistent_collapse_marker() {
        let policy = mine_collapse_policy();
        let options = PolicyFixtureOptions {
            existing_vmad: Some(mine_collapse_empty_vmad()),
            ..Default::default()
        };
        let fixture = fixture(&[policy], &[options]);
        let interner = StringInterner::new();
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let ScriptAdapter::BindExistingPlacedObject {
            target,
            target_parent_cell,
            ..
        } = policy.adapter
        else {
            panic!("mine collapse policy must bind a placed object");
        };
        add_mappings_except(
            &mut mapper,
            &[policy],
            &[
                policy.placed,
                policy.parent_cell,
                target,
                target_parent_cell,
            ],
        );

        let (first, second, vmad) = {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            let first = apply_one(&mut session, &mut mapper, policy).unwrap();
            let second = apply_one(&mut session, &mut mapper, policy).unwrap();
            let vmad = vmad_bytes_with_interner(&mut session, policy, &interner).unwrap();
            (first, second, vmad)
        };

        assert_eq!(first.records_changed, 1);
        assert!(first.warnings.is_empty());
        assert_eq!(second.records_changed, 0);
        assert!(second.warnings.is_empty());
        let (_, _, scripts) = parsed_manifest(&vmad).unwrap();
        assert_eq!(
            scripts,
            vec![ParsedScript {
                name: "Vault79MineCollapseScript".to_string(),
                flags: 0,
                properties: vec![ParsedProperty {
                    name: "myCollapseMarker".to_string(),
                    property_type: 1,
                    flags: VMAD_PROPERTY_FLAG_EDITED,
                    value: ParsedValue::Object(own(target.local)),
                }],
            }]
        );
    }

    #[test]
    fn mine_collapse_target_topology_and_base_mismatches_fail_closed() {
        let policy = mine_collapse_policy();
        let cases = [
            PolicyFixtureOptions {
                existing_vmad: Some(mine_collapse_empty_vmad()),
                bound_target_parent_local: Some(policy.parent_cell.local + 1),
                ..Default::default()
            },
            PolicyFixtureOptions {
                existing_vmad: Some(mine_collapse_empty_vmad()),
                bound_target_section: Some(CellSection::Temporary),
                ..Default::default()
            },
            PolicyFixtureOptions {
                existing_vmad: Some(mine_collapse_empty_vmad()),
                bound_target_base_local: Some(0x0055_862F),
                ..Default::default()
            },
        ];

        for options in cases {
            let expected_vmad = options.existing_vmad.clone().unwrap();
            let fixture = fixture(&[policy], &[options]);
            let interner = StringInterner::new();
            let mut state = mapper_state();
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            add_mappings(&mut mapper, &[policy]);

            let (report, after) = {
                let mut session = crate::session::open_session(fixture.target, None).unwrap();
                let report = apply_one(&mut session, &mut mapper, policy).unwrap();
                let after = vmad_bytes_with_interner(&mut session, policy, &interner).unwrap();
                (report, after)
            };

            assert_eq!(report.records_changed, 0);
            assert!(!report.warnings.is_empty());
            assert_eq!(after, expected_vmad);
        }
    }

    #[test]
    fn projected_activation_attaches_without_placed_or_parent_mappings() {
        let policy = activation_policy();
        let fixture = fixture(&[policy], &[PolicyFixtureOptions::default()]);
        let interner = StringInterner::new();
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        add_mappings_except(&mut mapper, &[policy], &[policy.placed, policy.parent_cell]);

        let (report, vmad) = {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            let report = apply_one(&mut session, &mut mapper, policy).unwrap();
            let vmad = vmad_bytes_with_interner(&mut session, policy, &interner);
            (report, vmad)
        };

        assert_eq!(report.records_changed, 1);
        assert!(report.warnings.is_empty());
        assert!(vmad.is_some());
    }

    #[test]
    fn mapped_nonidentity_placed_record_wins_over_preserved_local_candidate() {
        let policy = activation_policy();
        let mapped_local = policy.placed.local + 0x100;
        let fixture = fixture(&[policy], &[PolicyFixtureOptions::default()]);
        edit_placed_record(&fixture, policy, &mut |record| {
            record.form_id = own(mapped_local);
        });
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&fixture.target).unwrap();
            slot.parsed.root_items.push(ParsedItem::Record(record(
                policy.placed.signature,
                policy.placed.local,
                vec![
                    subrecord("NAME", own(policy.base.local).to_le_bytes().to_vec()),
                    subrecord("DATA", vec![0; 24]),
                ],
            )));
            slot.clear_record_count_cache();
        }
        let interner = StringInterner::new();
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        add_mappings_except(&mut mapper, &[policy], &[policy.placed]);
        mapper.add_mapping(
            FormKey {
                local: policy.placed.local,
                plugin: interner.intern(policy.source_plugin),
            },
            FormKey {
                local: mapped_local,
                plugin: interner.intern(TARGET_PLUGIN),
            },
        );

        let (report, mapped_vmad, preserved_vmad) = {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            let report = apply_one(&mut session, &mut mapper, policy).unwrap();
            let mapped_vmad = vmad_bytes_at_local(&mut session, mapped_local, &interner);
            let preserved_vmad = vmad_bytes_at_local(&mut session, policy.placed.local, &interner);
            (report, mapped_vmad, preserved_vmad)
        };

        assert_eq!(report.records_changed, 1);
        assert!(report.warnings.is_empty());
        assert!(mapped_vmad.is_some());
        assert!(preserved_vmad.is_none());
    }

    #[test]
    fn preserved_local_placement_resolution_fails_closed() {
        let policy = activation_policy();
        for (case, expected_reason) in [
            (0, "preserved_local_absent_or_ambiguous"),
            (1, "preserved_local_absent_or_ambiguous"),
            (2, "physical_identity_mismatch"),
            (3, "physical_identity_mismatch"),
        ] {
            let fixture = fixture(&[policy], &[PolicyFixtureOptions::default()]);
            match case {
                0 => edit_placed_record(&fixture, policy, &mut |record| {
                    record.form_id = own(policy.placed.local + 1);
                }),
                1 => edit_placed_record(&fixture, policy, &mut |record| {
                    record.signature = "REFR".into();
                }),
                2 => {
                    let mut store = plugin_handle_store_ref().lock().unwrap();
                    let slot = store.get_mut(&fixture.target).unwrap();
                    slot.parsed.root_items.push(ParsedItem::Record(record(
                        policy.placed.signature,
                        policy.placed.local,
                        vec![],
                    )));
                    slot.clear_record_count_cache();
                }
                3 => {
                    let mut store = plugin_handle_store_ref().lock().unwrap();
                    let slot = store.get_mut(&fixture.target).unwrap();
                    slot.parsed.root_items.push(ParsedItem::Record(record(
                        policy.parent_cell.signature,
                        policy.parent_cell.local,
                        vec![],
                    )));
                    slot.clear_record_count_cache();
                }
                _ => unreachable!(),
            }
            let interner = StringInterner::new();
            let mut state = mapper_state();
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            add_mappings_except(&mut mapper, &[policy], &[policy.placed, policy.parent_cell]);

            let report = {
                let mut session = crate::session::open_session(fixture.target, None).unwrap();
                apply_one(&mut session, &mut mapper, policy).unwrap()
            };

            assert_eq!(report.records_changed, 0);
            assert!(
                warning_text(&report, &interner)
                    .iter()
                    .any(|message| message.contains(expected_reason))
            );
        }
    }

    #[test]
    fn catalog_attaches_exact_typed_manifests_without_moving_placed_records() {
        let existing = valid_unrelated_vmad();
        let policies = [
            trigger_policy(),
            activation_policy(),
            interior_policy(),
            mine_collapse_policy(),
        ];
        let fixture = fixture(
            &policies,
            &[
                PolicyFixtureOptions {
                    existing_vmad: Some(existing),
                    ..Default::default()
                },
                PolicyFixtureOptions::default(),
                PolicyFixtureOptions::default(),
                PolicyFixtureOptions {
                    existing_vmad: Some(mine_collapse_empty_vmad()),
                    ..Default::default()
                },
            ],
        );
        let interner = StringInterner::new();
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        add_mappings(&mut mapper, &policies);

        let (
            report,
            before,
            trigger_vmad,
            activation_vmad,
            interior_vmad,
            mine_collapse_vmad,
            after,
        ) = {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            let before = policies
                .iter()
                .map(|policy| topology(&session, policy))
                .collect::<Vec<_>>();
            let report = apply_fo76_fo4_placed_record_vmad_catalog(
                &mut session,
                &mut mapper,
                Game::Fo76,
                Game::Fo4,
            )
            .unwrap();
            let trigger_vmad =
                vmad_bytes_with_interner(&mut session, trigger_policy(), &interner).unwrap();
            let activation_vmad =
                vmad_bytes_with_interner(&mut session, activation_policy(), &interner).unwrap();
            let interior_vmad =
                vmad_bytes_with_interner(&mut session, interior_policy(), &interner).unwrap();
            let mine_collapse_vmad =
                vmad_bytes_with_interner(&mut session, mine_collapse_policy(), &interner).unwrap();
            let after = policies
                .iter()
                .map(|policy| topology(&session, policy))
                .collect::<Vec<_>>();
            (
                report,
                before,
                trigger_vmad,
                activation_vmad,
                interior_vmad,
                mine_collapse_vmad,
                after,
            )
        };

        assert_eq!(report.records_changed, 4);
        assert_eq!(before, after);
        assert!(report.warnings.is_empty());

        let (version, object_format, trigger_scripts) = parsed_manifest(&trigger_vmad).unwrap();
        assert_eq!((version, object_format), (6, 2));
        assert_eq!(trigger_scripts.len(), 2);
        assert_eq!(trigger_scripts[0].name, "B21:ExistingPlacedAdapter");
        assert_eq!(
            trigger_scripts[1],
            ParsedScript {
                name: "B21:StoryEventOnTriggerEnter".to_string(),
                flags: 0,
                properties: vec![
                    ParsedProperty {
                        name: "TargetQuest".to_string(),
                        property_type: 1,
                        flags: 1,
                        value: ParsedValue::Object(own(match trigger_policy().adapter {
                            ScriptAdapter::StoryEventOnTriggerEnter { target_quest, .. } =>
                                target_quest.local,
                            _ => unreachable!(),
                        })),
                    },
                    ParsedProperty {
                        name: "StoryEventKeyword".to_string(),
                        property_type: 1,
                        flags: 1,
                        value: ParsedValue::Object(own(match trigger_policy().adapter {
                            ScriptAdapter::StoryEventOnTriggerEnter {
                                story_event_keyword,
                                ..
                            } => story_event_keyword.local,
                            _ => unreachable!(),
                        })),
                    },
                    ParsedProperty {
                        name: "StageToSet".to_string(),
                        property_type: 3,
                        flags: 1,
                        value: ParsedValue::Int(400),
                    },
                ],
            }
        );

        let (_, _, activation_scripts) = parsed_manifest(&activation_vmad).unwrap();
        assert_eq!(
            activation_scripts,
            vec![ParsedScript {
                name: "B21:StoryEventOnActivateStartScene".to_string(),
                flags: 0,
                properties: match activation_policy().adapter {
                    ScriptAdapter::StoryEventOnActivateStartScene {
                        target_quest,
                        story_event_keyword,
                        scene_to_start,
                    } => vec![
                        ParsedProperty {
                            name: "TargetQuest".to_string(),
                            property_type: 1,
                            flags: 1,
                            value: ParsedValue::Object(own(target_quest.local)),
                        },
                        ParsedProperty {
                            name: "StoryEventKeyword".to_string(),
                            property_type: 1,
                            flags: 1,
                            value: ParsedValue::Object(own(story_event_keyword.local)),
                        },
                        ParsedProperty {
                            name: "SceneToStart".to_string(),
                            property_type: 1,
                            flags: 1,
                            value: ParsedValue::Object(own(scene_to_start.local)),
                        },
                    ],
                    _ => unreachable!(),
                },
            }]
        );

        let (_, _, interior_scripts) = parsed_manifest(&interior_vmad).unwrap();
        assert_eq!(
            interior_scripts,
            vec![ParsedScript {
                name: "B21:StoryEventOnTriggerEnter".to_string(),
                flags: 0,
                properties: vec![
                    ParsedProperty {
                        name: "TargetQuest".to_string(),
                        property_type: 1,
                        flags: 1,
                        value: ParsedValue::Object(own(0x0040_5E14)),
                    },
                    ParsedProperty {
                        name: "StoryEventKeyword".to_string(),
                        property_type: 1,
                        flags: 1,
                        value: ParsedValue::Object(own(0x0040_5EC6)),
                    },
                    ParsedProperty {
                        name: "StageToSet".to_string(),
                        property_type: 3,
                        flags: 1,
                        value: ParsedValue::Int(500),
                    },
                ],
            }]
        );

        let (_, _, mine_collapse_scripts) = parsed_manifest(&mine_collapse_vmad).unwrap();
        assert_eq!(
            mine_collapse_scripts,
            vec![ParsedScript {
                name: "Vault79MineCollapseScript".to_string(),
                flags: 0,
                properties: vec![ParsedProperty {
                    name: "myCollapseMarker".to_string(),
                    property_type: 1,
                    flags: VMAD_PROPERTY_FLAG_EDITED,
                    value: ParsedValue::Object(own(0x0055_862E)),
                }],
            }]
        );
    }

    #[test]
    fn topology_mismatch_fails_closed_for_parent_cell_and_section() {
        for options in [
            PolicyFixtureOptions {
                actual_parent_local: Some(trigger_policy().parent_cell.local + 1),
                ..Default::default()
            },
            PolicyFixtureOptions {
                actual_section: Some(CellSection::Persistent),
                ..Default::default()
            },
        ] {
            let policy = trigger_policy();
            let fixture = fixture(&[policy], &[options]);
            let interner = StringInterner::new();
            let mut state = mapper_state();
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            add_mappings(&mut mapper, &[policy]);
            let report = {
                let mut session = crate::session::open_session(fixture.target, None).unwrap();
                apply_one(&mut session, &mut mapper, policy).unwrap()
            };
            assert_eq!(report.records_changed, 0);
            assert!(
                warning_text(&report, &interner)
                    .iter()
                    .any(|message| message.contains("topology_mismatch"))
            );
        }
    }

    #[test]
    fn base_and_reference_type_mismatches_fail_closed() {
        for options in [
            PolicyFixtureOptions {
                base_local: Some(trigger_policy().base.local + 1),
                ..Default::default()
            },
            PolicyFixtureOptions {
                reference_type_local: Some(
                    trigger_policy()
                        .reference_type
                        .map(|requirement| requirement.local + 1),
                ),
                ..Default::default()
            },
        ] {
            let policy = trigger_policy();
            let fixture = fixture(&[policy], &[options]);
            let interner = StringInterner::new();
            let mut state = mapper_state();
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            add_mappings(&mut mapper, &[policy]);
            let report = {
                let mut session = crate::session::open_session(fixture.target, None).unwrap();
                apply_one(&mut session, &mut mapper, policy).unwrap()
            };
            assert_eq!(report.records_changed, 0);
            assert!(!report.warnings.is_empty());
        }
    }

    #[test]
    fn mapped_but_absent_target_fails_closed_with_actionable_diagnostic() {
        let policy = trigger_policy();
        let target_quest = match policy.adapter {
            ScriptAdapter::StoryEventOnTriggerEnter { target_quest, .. } => target_quest,
            _ => unreachable!(),
        };
        let fixture = fixture(
            &[policy],
            &[PolicyFixtureOptions {
                omitted_target: Some(target_quest),
                ..Default::default()
            }],
        );
        let interner = StringInterner::new();
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        add_mappings(&mut mapper, &[policy]);
        let report = {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            apply_one(&mut session, &mut mapper, policy).unwrap()
        };

        assert_eq!(report.records_changed, 0);
        assert!(warning_text(&report, &interner).iter().any(|message| {
            message.contains("property:TargetQuest:target_absent_or_wrong_signature:QUST")
                && message.contains(policy.evidence)
        }));
    }

    #[test]
    fn exact_existing_script_is_idempotent() {
        let policy = trigger_policy();
        let interner = StringInterner::new();
        let existing = script_vmad(
            policy.adapter,
            &resolved_property_form_keys(policy, &interner),
            &["MasterA.esm".to_string(), "MasterB.esm".to_string()],
            TARGET_PLUGIN,
            &interner,
        )
        .unwrap();
        let fixture = fixture(
            &[policy],
            &[PolicyFixtureOptions {
                existing_vmad: Some(existing.clone()),
                ..Default::default()
            }],
        );
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        add_mappings(&mut mapper, &[policy]);

        let (first, second, after) = {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            let first = apply_one(&mut session, &mut mapper, policy).unwrap();
            let second = apply_one(&mut session, &mut mapper, policy).unwrap();
            let after = vmad_bytes_with_interner(&mut session, policy, &interner).unwrap();
            (first, second, after)
        };
        assert_eq!(first.records_changed, 0);
        assert_eq!(second.records_changed, 0);
        assert_eq!(after, existing);
    }

    #[test]
    fn conflicting_same_script_binding_is_preserved_and_rejected() {
        let policy = trigger_policy();
        let interner = StringInterner::new();
        let resolved = resolved_property_form_keys(policy, &interner);
        let conflicting = build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [{
                    "ScriptName": policy.adapter.script_name(),
                    "Properties": [
                        object_property("TargetQuest", resolved[0], &interner).unwrap(),
                        object_property("StoryEventKeyword", resolved[1], &interner).unwrap(),
                        {
                            "propertyName": "StageToSet",
                            "Type": "Int32",
                            "Flags": VMAD_PROPERTY_FLAG_EDITED,
                            "Value": 401,
                        },
                    ],
                }],
            }),
            &["MasterA.esm".to_string(), "MasterB.esm".to_string()],
            TARGET_PLUGIN,
        )
        .unwrap();
        let fixture = fixture(
            &[policy],
            &[PolicyFixtureOptions {
                existing_vmad: Some(conflicting.clone()),
                ..Default::default()
            }],
        );
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        add_mappings(&mut mapper, &[policy]);
        let (report, after) = {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            let report = apply_one(&mut session, &mut mapper, policy).unwrap();
            let after = vmad_bytes_with_interner(&mut session, policy, &interner).unwrap();
            (report, after)
        };

        assert_eq!(report.records_changed, 0);
        assert_eq!(after, conflicting);
        assert!(
            warning_text(&report, &interner)
                .iter()
                .any(|message| message.contains("same_script_different_binding"))
        );
    }

    #[test]
    fn attached_vmads_survive_dangling_reference_validation() {
        let policies = [
            trigger_policy(),
            activation_policy(),
            mine_collapse_policy(),
        ];
        let fixture = fixture(
            &policies,
            &[
                PolicyFixtureOptions::default(),
                PolicyFixtureOptions::default(),
                PolicyFixtureOptions {
                    existing_vmad: Some(mine_collapse_empty_vmad()),
                    ..Default::default()
                },
            ],
        );
        let interner = StringInterner::new();
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        add_mappings(&mut mapper, &policies);
        let (attached, validated) = {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            let attached = apply_fo76_fo4_placed_record_vmad_catalog(
                &mut session,
                &mut mapper,
                Game::Fo76,
                Game::Fo4,
            )
            .unwrap();
            let config = FixupConfig {
                target_master_handle_ids: fixture.masters.to_vec(),
                target_schema: Some(AuthoringSchema::for_game("fo4").expect("fo4 schema")),
                ..Default::default()
            };
            let validated = crate::fixups::null_dangling_vmad_refs::repair_dangling_vmad_refs(
                &mut session,
                &mut mapper,
                &config,
            )
            .unwrap();
            (attached, validated)
        };

        assert_eq!(attached.records_changed, 3);
        assert_eq!(validated.records_changed, 0);
    }

    #[test]
    fn attached_vmad_and_topology_survive_save_reload() {
        let policy = trigger_policy();
        let fixture = fixture(&[policy], &[PolicyFixtureOptions::default()]);
        let interner = StringInterner::new();
        let mut state = mapper_state();
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        add_mappings(&mut mapper, &[policy]);
        {
            let mut session = crate::session::open_session(fixture.target, None).unwrap();
            apply_fo76_fo4_placed_record_vmad_catalog(
                &mut session,
                &mut mapper,
                Game::Fo76,
                Game::Fo4,
            )
            .unwrap();
        }

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(TARGET_PLUGIN);
        plugin_handle_save_no_py(fixture.target, path.to_str().unwrap()).unwrap();
        let reloaded =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let (authoring, bytes, contexts) = {
            let authoring = plugin_handle_read_authoring_record_value_json(
                reloaded,
                &format!("{TARGET_PLUGIN}:{:06X}", policy.placed.local),
            )
            .unwrap()
            .unwrap();
            let mut session = crate::session::open_session(reloaded, None).unwrap();
            let bytes = vmad_bytes_with_interner(&mut session, policy, &interner).unwrap();
            let contexts = topology(&session, policy);
            (authoring, bytes, contexts)
        };
        plugin_handle_close_native(reloaded);

        assert!(authoring.to_string().contains(policy.adapter.script_name()));
        assert!(parsed_manifest(&bytes).is_some());
        assert_eq!(
            contexts,
            vec![PlacementContext {
                parent_cell_raw: own(policy.parent_cell.local),
                section: policy.section,
                section_label_raw: own(policy.parent_cell.local),
            }]
        );
    }

    #[test]
    fn catalog_is_strictly_gated_to_fo76_fo4() {
        let policies = [trigger_policy(), activation_policy()];
        for (source, target) in [(Game::SkyrimSe, Game::Fo4), (Game::Fo76, Game::SkyrimSe)] {
            let fixture = fixture(
                &policies,
                &[
                    PolicyFixtureOptions::default(),
                    PolicyFixtureOptions::default(),
                ],
            );
            let interner = StringInterner::new();
            let mut state = mapper_state();
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            add_mappings(&mut mapper, &policies);
            let (report, trigger, activation) = {
                let mut session = crate::session::open_session(fixture.target, None).unwrap();
                let report = apply_fo76_fo4_placed_record_vmad_catalog(
                    &mut session,
                    &mut mapper,
                    source,
                    target,
                )
                .unwrap();
                let trigger = vmad_bytes_with_interner(&mut session, trigger_policy(), &interner);
                let activation =
                    vmad_bytes_with_interner(&mut session, activation_policy(), &interner);
                (report, trigger, activation)
            };
            assert_eq!(report.records_changed, 0);
            assert!(report.warnings.is_empty());
            assert!(trigger.is_none());
            assert!(activation.is_none());
        }
    }

    #[test]
    fn production_catalog_call_stays_between_placed_copy_repair_and_vmad_validation() {
        let source = include_str!("../run.rs");
        let start = source.find("pub fn repair_placed_child_refs").unwrap();
        let end = source[start..]
            .find("pub fn synthesize_encounter_zones")
            .map(|offset| start + offset)
            .unwrap();
        let body = &source[start..end];
        let placed_repair = body
            .find("null_dangling_own_plugin_refs::repair_placed_child_refs")
            .unwrap();
        let adapter = body
            .find("placed_record_vmad::apply_fo76_fo4_placed_record_vmad_catalog")
            .unwrap();
        let vmad_validation = body
            .find("null_dangling_vmad_refs::repair_dangling_vmad_refs")
            .unwrap();
        assert!(placed_repair < adapter);
        assert!(adapter < vmad_validation);
    }
}
