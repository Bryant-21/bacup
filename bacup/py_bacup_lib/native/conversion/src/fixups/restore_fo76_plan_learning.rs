use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::quest_script_vmad::{AttachResult, attach_script_bytes};
use crate::fixups::repair_quest_completion_rewards::source_book_is_recipe;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const SCRIPT: &str = "B21:PlanLearnOnRead";
const GLOBAL_PREFIX: &str = "B21_PlanLearned_";

pub struct RestoreFo76PlanLearningFixup;

impl Fixup for RestoreFo76PlanLearningFixup {
    fn name(&self) -> &'static str {
        "restore_fo76_plan_learning"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _: &FixupConfig) -> bool {
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let source_schema = session.source_schema().map_err(handle_error)?;
        let target_schema = session.schema().map_err(handle_error)?;
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let output_sym = mapper.interner.intern(&target_plugin);
        let mut report = FixupReport::empty();
        let mut recipes = FxHashMap::default();
        let mut expanded: FxHashMap<u32, Vec<FormKey>> = FxHashMap::default();
        for fk in session
            .form_keys_of_sig(SigCode(*b"COBJ"), mapper.interner)
            .map_err(handle_error)?
        {
            let record = session
                .record_decoded(&fk, &target_schema, mapper.interner)
                .map_err(handle_error)?;
            if !matches!(field(&record, b"CNAM"), Some(FieldValue::FormKey(fk)) if fk.local != 0) {
                continue;
            }
            if let Some(parent) = record
                .eid
                .and_then(|eid| mapper.interner.resolve(eid))
                .and_then(expanded_parent)
            {
                expanded.entry(parent).or_default().push(fk);
            }
            recipes.insert(fk, record);
        }
        let mut by_book: FxHashMap<FormKey, Vec<FormKey>> = FxHashMap::default();
        let mut vanilla_books = FxHashSet::default();
        for fk in session
            .source_form_keys_of_sig(SigCode(*b"COBJ"), mapper.interner)
            .map_err(handle_error)?
        {
            let record = session
                .source_record_decoded(&fk, &source_schema, mapper.interner)
                .map_err(handle_error)?;
            let Some(book) = plan_book(&record, mapper.interner) else {
                continue;
            };
            if let Some(target) = mapper.lookup(fk) {
                if recipes.contains_key(&target) {
                    by_book.entry(book).or_default().push(target);
                } else if target.plugin != output_sym {
                    vanilla_books.insert(book);
                }
            }
            if let Some(children) = expanded.get(&fk.local) {
                by_book.entry(book).or_default().extend(children);
            }
        }

        let mut globals = FxHashMap::default();
        for fk in session
            .form_keys_of_sig(SigCode(*b"GLOB"), mapper.interner)
            .map_err(handle_error)?
        {
            let record = session
                .record_decoded(&fk, &target_schema, mapper.interner)
                .map_err(handle_error)?;
            if let Some(eid) = record.eid.and_then(|eid| mapper.interner.resolve(eid)) {
                globals.insert(eid.to_string(), fk);
            }
        }
        let target_books: FxHashSet<_> = session
            .form_keys_of_sig(SigCode(*b"BOOK"), mapper.interner)
            .map_err(handle_error)?
            .into_iter()
            .collect();
        let target_quests: FxHashSet<_> = session
            .form_keys_of_sig(SigCode(*b"QUST"), mapper.interner)
            .map_err(handle_error)?
            .into_iter()
            .collect();
        let mut books = session
            .source_form_keys_of_sig(SigCode(*b"BOOK"), mapper.interner)
            .map_err(handle_error)?;
        books.sort_unstable_by_key(|fk| fk.local);
        let mut changed = Vec::new();
        let mut changed_recipes = FxHashSet::default();
        let mut added = Vec::new();
        let mut unavailable = 0;
        let mut supported = 0;
        for source_fk in books {
            let Some(target_fk) = mapper
                .lookup(source_fk)
                .filter(|fk| target_books.contains(fk))
            else {
                continue;
            };
            let source = session
                .source_record_decoded(&source_fk, &source_schema, mapper.interner)
                .map_err(handle_error)?;
            if source_book_is_recipe(&source, mapper.interner) != Some(true) {
                continue;
            }
            let mut book = session
                .record_decoded(&target_fk, &target_schema, mapper.interner)
                .map_err(handle_error)?;
            let mut linked = by_book.remove(&source_fk).unwrap_or_default();
            linked.sort_unstable_by_key(|fk| fk.local);
            linked.dedup();
            let crane = source.eid.and_then(|eid| mapper.interner.resolve(eid))
                == Some("W05_MQ_002P_Radical_Recipe_Workshop_CraneRadioTransmitter");
            let read_quest = crane
                .then(|| {
                    mapper.lookup(FormKey {
                        local: 0x40F5BE,
                        plugin: source_fk.plugin,
                    })
                })
                .flatten()
                .filter(|fk| target_quests.contains(fk));
            let available = (!linked.is_empty() || vanilla_books.contains(&source_fk))
                && (!crane || read_quest.is_some());
            let global = if available {
                let eid = format!("{GLOBAL_PREFIX}{:06X}", target_fk.local);
                let fk = if let Some(fk) = globals.get(&eid) {
                    *fk
                } else {
                    let synthetic = FormKey {
                        local: 1,
                        plugin: mapper
                            .interner
                            .intern(&format!("B21_PlanLearning_{:06X}", source_fk.local)),
                    };
                    let fk = mapper.allocate_or_resolve(synthetic, None, SigCode(*b"GLOB"));
                    added.push(learned_global(fk, &eid, linked.is_empty(), mapper.interner));
                    globals.insert(eid, fk);
                    fk
                };
                Some(fk)
            } else {
                unavailable += 1;
                report.warnings.push(mapper.interner.intern(&format!(
                    "restore_fo76_plan_learning:{:06X}:no_supported_recipe_or_quest",
                    source_fk.local
                )));
                None
            };
            let vmad = plan_vmad(
                global,
                read_quest,
                &target_masters,
                &target_plugin,
                mapper.interner,
            )
            .ok_or_else(|| FixupError::SchemaError("plan VMAD encode failed".into()))?;
            let mut edited = attach_plan_script(&mut book, &vmad)?;
            edited |= ensure_readable(&mut book, mapper.interner);
            if edited {
                changed.push(book);
            }
            if let Some(global) = global {
                let raw = ((target_masters.len() as u32) << 24) | global.local;
                for fk in linked {
                    let recipe = recipes.get_mut(&fk).expect("indexed recipe");
                    if add_learning_condition(recipe, raw) {
                        supported += 1;
                        changed_recipes.insert(fk);
                    }
                }
            }
        }
        // Unchanged records are not rewritten: their raw/localized payloads stay intact.
        changed.extend(
            recipes
                .into_values()
                .filter(|record| changed_recipes.contains(&record.form_key)),
        );
        report.records_added = added.len() as u32;
        session
            .add_records(added, &target_schema, mapper.interner)
            .map_err(handle_error)?;
        report.records_changed = session
            .replace_records_contents(changed, &target_schema, mapper.interner)
            .map_err(handle_error)? as u32;
        report.diagnostics.push(mapper.interner.intern(&format!(
            "restore_fo76_plan_learning:recipes_gated={supported}:unavailable_books={unavailable}"
        )));
        Ok(report)
    }
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn field<'a>(record: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *sig)
        .map(|entry| &entry.value)
}

fn plan_book(record: &Record, interner: &StringInterner) -> Option<FormKey> {
    let learned = match field(record, b"LRNM")? {
        FieldValue::String(name) => interner.resolve(*name) == Some("LearnedFromPlan"),
        FieldValue::Uint(value) => *value == 4,
        FieldValue::Int(value) => *value == 4,
        FieldValue::Bytes(bytes) => bytes.as_slice() == [4],
        _ => false,
    };
    if !learned {
        return None;
    }
    match field(record, b"GNAM")? {
        FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
        _ => None,
    }
}

fn expanded_parent(eid: &str) -> Option<u32> {
    let suffix = eid.strip_prefix("B21_FO76Workshop_")?;
    u32::from_str_radix(suffix.split('_').next()?, 16).ok()
}

fn learned_global(
    fk: FormKey,
    eid: &str,
    already_available: bool,
    interner: &StringInterner,
) -> Record {
    let mut record = Record::new(SigCode(*b"GLOB"), fk);
    record.eid = Some(interner.intern(eid));
    record.fields.extend([
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(record.eid.unwrap()),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FNAM"),
            value: FieldValue::Bytes(vec![b'f'].into()),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FLTV"),
            value: FieldValue::Float(if already_available { 1.0 } else { 0.0 }),
        },
    ]);
    record
}

fn plan_vmad(
    learned: Option<FormKey>,
    quest: Option<FormKey>,
    masters: &[String],
    plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let mut properties = Vec::new();
    for (name, fk) in [("Learned", learned), ("ReadQuest", quest)] {
        if let Some(fk) = fk {
            properties.push(serde_json::json!({"propertyName": name, "Type": "Object", "Flags": 1,
                "Value": {"Alias": -1, "FormID": {"reference": {"plugin": interner.resolve(fk.plugin)?, "object_id": format!("{:06X}", fk.local)}}}}));
        }
    }
    if quest.is_some() {
        for (name, stage) in [("PrereqStage", 110), ("StageToSet", 200)] {
            properties.push(serde_json::json!({"propertyName": name, "Type": "Int32", "Flags": 1, "Value": stage}));
        }
    }
    build_vmad_bytes_from_payload(
        &serde_json::json!({"Version": 6, "Object Format": 2,
        "Scripts": [{"ScriptName": SCRIPT, "Flags": 0, "Properties": properties}]}),
        masters,
        plugin,
    )
}

fn attach_plan_script(book: &mut Record, vmad: &[u8]) -> Result<bool, FixupError> {
    if let Some(entry) = book.fields.iter_mut().find(|entry| entry.sig.0 == *b"VMAD") {
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            return Err(FixupError::SchemaError("plan VMAD is not raw".into()));
        };
        let mut patched = bytes.to_vec();
        return match attach_script_bytes(&mut patched, SCRIPT, vmad) {
            AttachResult::Changed => {
                *bytes = patched.into();
                Ok(true)
            }
            AttachResult::AlreadyPresent => Ok(false),
            AttachResult::Conflict(reason) => Err(FixupError::Other(format!(
                "plan {:06X}: {reason}",
                book.form_key.local
            ))),
        };
    }
    let index = book
        .fields
        .iter()
        .take_while(|entry| entry.sig.0 == *b"EDID")
        .count();
    book.fields.insert(
        index,
        FieldEntry {
            sig: SubrecordSig(*b"VMAD"),
            value: FieldValue::Bytes(vmad.into()),
        },
    );
    Ok(true)
}

fn has_text(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::String(text) => interner
            .resolve(*text)
            .is_some_and(|text| !text.trim().is_empty()),
        FieldValue::Struct(fields) => fields.iter().any(|(name, value)| {
            interner
                .resolve(*name)
                .is_some_and(|name| matches!(name, "Value" | "String" | "Values"))
                && has_text(value, interner)
        }),
        FieldValue::List(values) => values.iter().any(|value| has_text(value, interner)),
        _ => false,
    }
}

fn ensure_readable(book: &mut Record, interner: &StringInterner) -> bool {
    let text = FieldValue::String(interner.intern("Study these plans to learn their recipes."));
    if let Some(entry) = book.fields.iter_mut().find(|entry| entry.sig.0 == *b"DESC") {
        if has_text(&entry.value, interner) {
            return false;
        }
        entry.value = text;
    } else {
        let index = book
            .fields
            .iter()
            .position(|entry| {
                matches!(
                    &entry.sig.0,
                    b"YNAM" | b"ZNAM" | b"KSIZ" | b"DATA" | b"DNAM"
                )
            })
            .unwrap_or(book.fields.len());
        book.fields.insert(
            index,
            FieldEntry {
                sig: SubrecordSig(*b"DESC"),
                value: text,
            },
        );
    }
    true
}

fn learning_condition(raw_global: u32) -> FieldEntry {
    let mut data = vec![0u8; 32];
    data[4..8].copy_from_slice(&1.0f32.to_le_bytes());
    data[8..12].copy_from_slice(&74u32.to_le_bytes());
    data[12..16].copy_from_slice(&raw_global.to_le_bytes());
    data[28..32].copy_from_slice(&(-1i32).to_le_bytes());
    FieldEntry {
        sig: SubrecordSig(*b"CTDA"),
        value: FieldValue::Bytes(data.into()),
    }
}

fn add_learning_condition(recipe: &mut Record, raw_global: u32) -> bool {
    let condition = learning_condition(raw_global);
    if recipe.fields.contains(&condition) {
        return false;
    }
    // A separate first AND term cannot become part of an existing OR chain.
    let index = recipe
        .fields
        .iter()
        .position(|entry| {
            matches!(
                &entry.sig.0,
                b"CTDA" | b"CNAM" | b"BNAM" | b"FNAM" | b"INTV"
            )
        })
        .unwrap_or(recipe.fields.len());
    recipe.fields.insert(index, condition);
    true
}

#[cfg(test)]
mod tests;
