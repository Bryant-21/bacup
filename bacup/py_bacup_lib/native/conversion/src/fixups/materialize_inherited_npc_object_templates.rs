//! Materialize inherited object-template blocks on concrete FO76 NPCs.
//!
//! FO76 modular robots can keep `OBTE`..`STOP` only on the NPC selected by the
//! active inventory-template slot. FO4's concrete robot records carry that
//! block directly; leaving it inherited produces a live actor with dialogue
//! and AI but no assembled body. This whole-plugin pass copies the first block
//! found along an output-owned inventory-template chain without changing the
//! inheritance flags or replacing an authored block.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::creature::creature_predicate::npc_acbs_template_flags;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const INVENTORY_TEMPLATE_SLOT: usize = 8;
const INVENTORY_TEMPLATE_OFFSET: usize = INVENTORY_TEMPLATE_SLOT * 4;
const NPC_TEMPLATE_INVENTORY: u16 = 0x0100;
const MAX_TEMPLATE_DEPTH: usize = 64;

#[derive(Clone, Default)]
struct NpcTemplateInfo {
    inventory_template: Option<FormKey>,
    object_template_block: Option<Vec<FieldEntry>>,
}

pub struct MaterializeInheritedNpcObjectTemplatesFixup;

impl Fixup for MaterializeInheritedNpcObjectTemplatesFixup {
    fn name(&self) -> &'static str {
        "materialize_inherited_npc_object_templates"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::WholePluginSafe
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        config.is_whole_plugin
            && session
                .source_slot_opt()
                .and_then(|slot| slot.parsed.game.as_deref())
                .is_some_and(|game| game.eq_ignore_ascii_case("fo76"))
            && session
                .target_slot()
                .parsed
                .game
                .as_deref()
                .is_some_and(|game| game.eq_ignore_ascii_case("fo4"))
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let npc_sig = SigCode::from_str("NPC_")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let npc_form_keys = session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let target_masters = session.target_masters().to_vec();
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_plugin_sym = mapper.interner.intern(&target_plugin);
        let mut report = FixupReport::empty();
        let mut templates = FxHashMap::default();

        for form_key in &npc_form_keys {
            let record = match session.record_decoded(form_key, schema.as_ref(), mapper.interner) {
                Ok(record) => record,
                Err(error) => {
                    report.warnings.push(mapper.interner.intern(&format!(
                        "materialize_inherited_npc_object_templates_read:{error}"
                    )));
                    continue;
                }
            };
            templates.insert(
                *form_key,
                NpcTemplateInfo {
                    inventory_template: active_inventory_template(
                        &record,
                        &target_masters,
                        &target_plugin,
                        mapper.interner,
                    ),
                    object_template_block: object_template_block(&record),
                },
            );
        }

        let mut candidates = 0u32;
        let mut replacements = Vec::new();
        for form_key in &npc_form_keys {
            let Some(info) = templates.get(form_key) else {
                continue;
            };
            if info.object_template_block.is_some() {
                continue;
            }
            let Some(parent) = info.inventory_template else {
                continue;
            };
            if parent.plugin != target_plugin_sym {
                continue;
            }
            candidates = candidates.saturating_add(1);
            let Some(block) = inherited_object_template_block(parent, &templates) else {
                continue;
            };
            let mut record =
                match session.record_decoded(form_key, schema.as_ref(), mapper.interner) {
                    Ok(record) => record,
                    Err(error) => {
                        report.warnings.push(mapper.interner.intern(&format!(
                            "materialize_inherited_npc_object_templates_replace_read:{error}"
                        )));
                        continue;
                    }
                };
            if !insert_object_template_block(&mut record, block) {
                continue;
            }
            replacements.push(record);
        }

        if !replacements.is_empty() {
            let materialized = u32::try_from(replacements.len()).unwrap_or(u32::MAX);
            session
                .replace_records(replacements, schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            report.records_changed = report.records_changed.saturating_add(materialized);
        }

        if candidates != 0 {
            report.message = Some(mapper.interner.intern(&format!(
                "candidates={candidates} materialized={}",
                report.records_changed
            )));
        }
        Ok(report)
    }
}

#[cfg(test)]
fn run_sequential_for_test(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
) -> Result<FixupReport, FixupError> {
    let schema = session
        .schema()
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let npc_sig =
        SigCode::from_str("NPC_").map_err(|error| FixupError::SchemaError(error.to_string()))?;
    let npc_form_keys = session
        .form_keys_of_sig(npc_sig, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let target_masters = session.target_masters().to_vec();
    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let target_plugin_sym = mapper.interner.intern(&target_plugin);
    let mut report = FixupReport::empty();
    let mut templates = FxHashMap::default();
    for form_key in &npc_form_keys {
        let record = match session.record_decoded(form_key, schema.as_ref(), mapper.interner) {
            Ok(record) => record,
            Err(error) => {
                report.warnings.push(mapper.interner.intern(&format!(
                    "materialize_inherited_npc_object_templates_read:{error}"
                )));
                continue;
            }
        };
        templates.insert(
            *form_key,
            NpcTemplateInfo {
                inventory_template: active_inventory_template(
                    &record,
                    &target_masters,
                    &target_plugin,
                    mapper.interner,
                ),
                object_template_block: object_template_block(&record),
            },
        );
    }
    let mut candidates = 0u32;
    for form_key in &npc_form_keys {
        let Some(info) = templates.get(form_key) else {
            continue;
        };
        if info.object_template_block.is_some() {
            continue;
        }
        let Some(parent) = info.inventory_template else {
            continue;
        };
        if parent.plugin != target_plugin_sym {
            continue;
        }
        candidates = candidates.saturating_add(1);
        let Some(block) = inherited_object_template_block(parent, &templates) else {
            continue;
        };
        let mut record = match session.record_decoded(form_key, schema.as_ref(), mapper.interner) {
            Ok(record) => record,
            Err(error) => {
                report.warnings.push(mapper.interner.intern(&format!(
                    "materialize_inherited_npc_object_templates_replace_read:{error}"
                )));
                continue;
            }
        };
        if insert_object_template_block(&mut record, block) {
            session
                .replace_record(record, schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            report.records_changed = report.records_changed.saturating_add(1);
        }
    }
    if candidates != 0 {
        report.message = Some(mapper.interner.intern(&format!(
            "candidates={candidates} materialized={}",
            report.records_changed
        )));
    }
    Ok(report)
}

fn active_inventory_template(
    record: &Record,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if npc_acbs_template_flags(record).is_none_or(|flags| flags & NPC_TEMPLATE_INVENTORY == 0) {
        return None;
    }
    let tpta = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "TPTA")?;
    match &tpta.value {
        FieldValue::Bytes(bytes) if bytes.len() >= INVENTORY_TEMPLATE_OFFSET + 4 => {
            resolve_raw_form_id(
                u32::from_le_bytes(
                    bytes[INVENTORY_TEMPLATE_OFFSET..INVENTORY_TEMPLATE_OFFSET + 4]
                        .try_into()
                        .unwrap(),
                ),
                target_masters,
                target_plugin,
                interner,
            )
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
            (normalized_name(*name, interner) == "inventory")
                .then(|| form_key_value(value, target_masters, target_plugin, interner))
                .flatten()
        }),
        FieldValue::List(values) => values
            .get(INVENTORY_TEMPLATE_SLOT)
            .and_then(|value| form_key_value(value, target_masters, target_plugin, interner)),
        _ => None,
    }
}

fn object_template_block(record: &Record) -> Option<Vec<FieldEntry>> {
    let start = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "OBTE")?;
    let end = record.fields[start..]
        .iter()
        .position(|field| field.sig.as_str() == "STOP")?
        + start;
    Some(record.fields[start..=end].to_vec())
}

fn inherited_object_template_block<'a>(
    start: FormKey,
    templates: &'a FxHashMap<FormKey, NpcTemplateInfo>,
) -> Option<&'a [FieldEntry]> {
    let mut current = start;
    let mut seen = FxHashSet::default();
    for _ in 0..MAX_TEMPLATE_DEPTH {
        if !seen.insert(current) {
            return None;
        }
        let info = templates.get(&current)?;
        if let Some(block) = &info.object_template_block {
            return Some(block);
        }
        current = info.inventory_template?;
    }
    None
}

fn insert_object_template_block(record: &mut Record, block: &[FieldEntry]) -> bool {
    if block.is_empty()
        || record
            .fields
            .iter()
            .any(|field| field.sig.as_str() == "OBTE")
    {
        return false;
    }
    let insert_at = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "CNAM")
        .or_else(|| {
            record
                .fields
                .iter()
                .position(|field| field.sig.as_str() == "FULL")
        })
        .unwrap_or(record.fields.len());
    for (offset, field) in block.iter().cloned().enumerate() {
        record.fields.insert(insert_at + offset, field);
    }
    true
}

fn form_key_value(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) if form_key.local != 0 => Some(*form_key),
        FieldValue::Uint(raw) if *raw <= u64::from(u32::MAX) => {
            resolve_raw_form_id(*raw as u32, target_masters, target_plugin, interner)
        }
        FieldValue::Int(raw) if *raw > 0 && *raw <= i64::from(u32::MAX) => {
            resolve_raw_form_id(*raw as u32, target_masters, target_plugin, interner)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => resolve_raw_form_id(
            u32::from_le_bytes(bytes[..4].try_into().unwrap()),
            target_masters,
            target_plugin,
            interner,
        ),
        _ => None,
    }
}

fn resolve_raw_form_id(
    raw: u32,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let index = (raw >> 24) as usize;
    let plugin = if index < target_masters.len() {
        target_masters[index].as_str()
    } else if index == target_masters.len() {
        target_plugin
    } else {
        return None;
    };
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
    })
}

fn normalized_name(name: crate::sym::Sym, interner: &StringInterner) -> String {
    interner
        .resolve(name)
        .unwrap_or_default()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::ids::SubrecordSig;
    use crate::record::RecordFlags;
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_new_native, plugin_handle_save_no_py,
    };
    use smallvec::{SmallVec, smallvec};

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn bytes(value: Vec<u8>) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(value))
    }

    fn npc(local: u32, plugin: crate::sym::Sym) -> Record {
        Record {
            sig: SigCode::from_str("NPC_").unwrap(),
            form_key: FormKey { local, plugin },
            eid: None,
            flags: RecordFlags::empty(),
            fields: SmallVec::new(),
            warnings: SmallVec::new(),
        }
    }

    fn active_inventory_fields(raw_template: u32) -> SmallVec<[FieldEntry; 8]> {
        let mut acbs = vec![0u8; 20];
        acbs[14..16].copy_from_slice(&NPC_TEMPLATE_INVENTORY.to_le_bytes());
        let mut tpta = vec![0u8; 13 * 4];
        tpta[INVENTORY_TEMPLATE_OFFSET..INVENTORY_TEMPLATE_OFFSET + 4]
            .copy_from_slice(&raw_template.to_le_bytes());
        smallvec![field("ACBS", bytes(acbs)), field("TPTA", bytes(tpta))]
    }

    fn object_block() -> Vec<FieldEntry> {
        vec![
            field("OBTE", bytes(1u32.to_le_bytes().to_vec())),
            field("OBTF", bytes(Vec::new())),
            field("FULL", bytes(b"Default\0".to_vec())),
            field("OBTS", bytes(vec![0u8; 16])),
            field("STOP", bytes(Vec::new())),
        ]
    }

    #[test]
    fn follows_inventory_chain_and_inserts_block_before_class() {
        let interner = StringInterner::new();
        let output = interner.intern("Output.esp");
        let masters = vec!["Fallout4.esm".to_string()];
        let parent = FormKey {
            local: 0x900,
            plugin: output,
        };
        let ancestor = FormKey {
            local: 0x901,
            plugin: output,
        };
        let mut templates = FxHashMap::default();
        templates.insert(
            parent,
            NpcTemplateInfo {
                inventory_template: Some(ancestor),
                object_template_block: None,
            },
        );
        templates.insert(
            ancestor,
            NpcTemplateInfo {
                inventory_template: None,
                object_template_block: Some(object_block()),
            },
        );

        let mut child = npc(0x800, output);
        child.fields = active_inventory_fields(0x0100_0900);
        child.fields.push(field(
            "RNAM",
            FieldValue::FormKey(FormKey {
                local: 0xDFB33,
                plugin: interner.intern("Fallout4.esm"),
            }),
        ));
        child
            .fields
            .push(field("CNAM", bytes(0u32.to_le_bytes().to_vec())));
        child.fields.push(field("FULL", bytes(b"Sunny\0".to_vec())));

        assert_eq!(
            active_inventory_template(&child, &masters, "Output.esp", &interner),
            Some(parent)
        );
        let block = inherited_object_template_block(parent, &templates).unwrap();
        assert!(insert_object_template_block(&mut child, block));
        assert_eq!(
            child
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec![
                "ACBS", "TPTA", "RNAM", "OBTE", "OBTF", "FULL", "OBTS", "STOP", "CNAM", "FULL"
            ]
        );
    }

    #[test]
    fn preserves_authored_object_template_block() {
        let interner = StringInterner::new();
        let output = interner.intern("Output.esp");
        let mut child = npc(0x800, output);
        child.fields.extend(object_block());
        let before = child.fields.clone();

        assert!(!insert_object_template_block(&mut child, &object_block()));
        assert_eq!(child.fields, before);
    }

    #[test]
    fn ignores_inactive_inventory_slot_and_external_master_parent() {
        let interner = StringInterner::new();
        let output = interner.intern("Output.esp");
        let masters = vec!["Fallout4.esm".to_string()];
        let mut inactive = npc(0x800, output);
        inactive.fields.push(field("ACBS", bytes(vec![0u8; 20])));
        inactive
            .fields
            .push(active_inventory_fields(0x0100_0900).pop().unwrap());
        assert_eq!(
            active_inventory_template(&inactive, &masters, "Output.esp", &interner),
            None
        );

        let mut external = npc(0x801, output);
        external.fields = active_inventory_fields(0x0000_0900);
        assert_ne!(
            active_inventory_template(&external, &masters, "Output.esp", &interner)
                .unwrap()
                .plugin,
            output
        );
    }

    #[test]
    fn stops_on_inventory_template_cycle() {
        let interner = StringInterner::new();
        let output = interner.intern("Output.esp");
        let left = FormKey {
            local: 0x900,
            plugin: output,
        };
        let right = FormKey {
            local: 0x901,
            plugin: output,
        };
        let mut templates = FxHashMap::default();
        templates.insert(
            left,
            NpcTemplateInfo {
                inventory_template: Some(right),
                object_template_block: None,
            },
        );
        templates.insert(
            right,
            NpcTemplateInfo {
                inventory_template: Some(left),
                object_template_block: None,
            },
        );

        assert!(inherited_object_template_block(left, &templates).is_none());
    }

    #[test]
    fn session_fixup_batches_materialized_npcs_and_preserves_compression() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let parent_key = FormKey {
            local: 0x900,
            plugin: output,
        };
        let child_key = FormKey {
            local: 0x800,
            plugin: output,
        };
        let second_child_key = FormKey {
            local: 0x801,
            plugin: output,
        };
        let mut parent = npc(parent_key.local, output);
        parent.fields.extend(object_block());
        let mut child = npc(child_key.local, output);
        child.fields = active_inventory_fields(parent_key.local);
        child.flags = RecordFlags::COMPRESSED;
        let mut second_child = npc(second_child_key.local, output);
        second_child.fields = active_inventory_fields(parent_key.local);

        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        let sequential_target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        let schema = {
            let mut session = open_session(target, None).unwrap();
            let schema = session.schema().unwrap();
            session
                .add_record(parent.clone(), schema.as_ref(), &interner)
                .unwrap();
            session
                .add_record(child.clone(), schema.as_ref(), &interner)
                .unwrap();
            session
                .add_record(second_child.clone(), schema.as_ref(), &interner)
                .unwrap();
            schema
        };
        {
            let mut session = open_session(sequential_target, None).unwrap();
            session
                .add_record(parent, schema.as_ref(), &interner)
                .unwrap();
            session
                .add_record(child, schema.as_ref(), &interner)
                .unwrap();
            session
                .add_record(second_child, schema.as_ref(), &interner)
                .unwrap();
        }
        let mut mapper_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
        let mut sequential_mapper_state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                preserve_source_ids: true,
                ..Default::default()
            },
        );
        let mut sequential_mapper =
            FormKeyMapper::from_state(&mut sequential_mapper_state, &interner);
        let config = FixupConfig {
            is_whole_plugin: true,
            ..Default::default()
        };

        let mut session = open_session(target, Some(source)).unwrap();
        assert!(MaterializeInheritedNpcObjectTemplatesFixup.applies_to_session(&session, &config));
        let report = MaterializeInheritedNpcObjectTemplatesFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(report.records_changed, 2);
        let updated = session
            .record_decoded(&child_key, schema.as_ref(), &interner)
            .unwrap();
        assert!(object_template_block(&updated).is_some());
        assert!(updated.flags.contains(RecordFlags::COMPRESSED));
        let second_updated = session
            .record_decoded(&second_child_key, schema.as_ref(), &interner)
            .unwrap();
        assert!(object_template_block(&second_updated).is_some());
        drop(session);
        let mut sequential_session = open_session(sequential_target, None).unwrap();
        let sequential_report =
            run_sequential_for_test(&mut sequential_session, &mut sequential_mapper).unwrap();
        assert_eq!(format!("{report:?}"), format!("{sequential_report:?}"));
        assert_eq!(report.records_changed, sequential_report.records_changed);
        assert_eq!(report.records_dropped, sequential_report.records_dropped);
        assert_eq!(report.warnings, sequential_report.warnings);
        assert_eq!(report.message, sequential_report.message);
        let sequential_updated = sequential_session
            .record_decoded(&child_key, schema.as_ref(), &interner)
            .unwrap();
        let sequential_second_updated = sequential_session
            .record_decoded(&second_child_key, schema.as_ref(), &interner)
            .unwrap();
        assert!(object_template_block(&sequential_updated).is_some());
        assert!(sequential_updated.flags.contains(RecordFlags::COMPRESSED));
        assert!(object_template_block(&sequential_second_updated).is_some());
        drop(sequential_session);

        let temp = tempfile::tempdir().unwrap();
        let batched_path = temp.path().join("batched.esp");
        let sequential_path = temp.path().join("sequential.esp");
        plugin_handle_save_no_py(target, batched_path.to_str().unwrap()).unwrap();
        plugin_handle_save_no_py(sequential_target, sequential_path.to_str().unwrap()).unwrap();
        assert_eq!(
            std::fs::read(batched_path).unwrap(),
            std::fs::read(sequential_path).unwrap()
        );

        assert!(plugin_handle_close_native(source));
        assert!(plugin_handle_close_native(target));
        assert!(plugin_handle_close_native(sequential_target));
    }

    #[test]
    #[ignore = "representative cold phase benchmark"]
    fn benchmark_batched_materialization_against_sequential() {
        const SAMPLES: usize = 6;
        for sample in 0..SAMPLES {
            let mut pair = Vec::new();
            for sequential in if sample % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                let interner = StringInterner::new();
                let output = interner.intern("SeventySix.esm");
                let parent_key = FormKey {
                    local: 0x900,
                    plugin: output,
                };
                let mut records = Vec::with_capacity(50_101);
                let mut parent = npc(parent_key.local, output);
                parent.fields.extend(object_block());
                records.push(parent);
                for index in 0..100u32 {
                    let mut child = npc(0x1000 + index, output);
                    child.fields = active_inventory_fields(parent_key.local);
                    if index % 3 == 0 {
                        child.flags = RecordFlags::COMPRESSED;
                    }
                    records.push(child);
                }
                for index in 0..50_000u32 {
                    records.push(npc(0x20_000 + index, output));
                }
                let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
                let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
                let schema = {
                    let mut session = open_session(target, None).unwrap();
                    let schema = session.schema().unwrap();
                    session
                        .add_records(records, schema.as_ref(), &interner)
                        .unwrap();
                    schema
                };
                let mut mapper_state = MapperState::new(
                    [],
                    MapperOptions {
                        output_plugin_name: "SeventySix.esm".into(),
                        preserve_source_ids: true,
                        ..Default::default()
                    },
                );
                let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
                let config = FixupConfig {
                    is_whole_plugin: true,
                    ..Default::default()
                };
                let started = std::time::Instant::now();
                let mut session = open_session(target, Some(source)).unwrap();
                let report = if sequential {
                    run_sequential_for_test(&mut session, &mut mapper).unwrap()
                } else {
                    MaterializeInheritedNpcObjectTemplatesFixup
                        .run_with_session(&mut session, &mut mapper, &config)
                        .unwrap()
                };
                session.flush_pending_effects();
                drop(session);
                let elapsed = started.elapsed().as_secs_f64();
                let temp = tempfile::tempdir().unwrap();
                let output_path = temp.path().join("output.esp");
                plugin_handle_save_no_py(target, output_path.to_str().unwrap()).unwrap();
                let bytes = std::fs::read(output_path).unwrap();
                eprintln!(
                    "npc_object_template sample={} mode={} elapsed_ms={:.3} bytes={} report={report:?}",
                    sample + 1,
                    if sequential { "sequential" } else { "batched" },
                    elapsed * 1000.0,
                    bytes.len(),
                );
                pair.push((sequential, format!("{report:?}"), bytes));
                assert!(plugin_handle_close_native(source));
                assert!(plugin_handle_close_native(target));
            }
            assert_eq!(pair[0].1, pair[1].1);
            assert_eq!(pair[0].2, pair[1].2);
        }
    }
}
