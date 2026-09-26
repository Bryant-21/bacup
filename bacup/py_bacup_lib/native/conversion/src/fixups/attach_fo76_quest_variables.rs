//! Back FO76 quest variables in quest text with globals.
//!
//! FO76 objective and log text substitutes `<Variable=Name>` from per-quest
//! variables, which FO4 does not have. Each variable becomes a `GLOB` listed in
//! the quest's text display globals (`QTGL`), the tag becomes
//! `<Global=EditorID>` in every language of the string table, and
//! `B21:QuestVariables` maps the FO76 names to those globals so event scripts
//! can update the displayed value. FO76 `QTVR` carries only names, so every
//! global starts at 0.

use std::collections::BTreeMap;

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{
    LocalizedStringsState, ParsedRecord, ParsedSubrecord, WriteEffect,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use crate::fixups::normalize_fo76_pack_templates::encode_target_form_id;
use crate::fixups::quest_script_binding::{
    attach_quest_script, object_array_property, script_vmad, string_array_property,
};
use crate::fixups::quest_script_vmad::AttachResult;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::source_read::form_key_to_read_str;
use crate::sym::StringInterner;

const SCRIPT_NAME: &str = "B21:QuestVariables";
const GLOBAL_EDITOR_ID_PREFIX: &str = "B21_QuestVar_";
const VARIABLE_TAG: &str = "<Variable=";
const GLOBAL_TAG: &str = "<Global=";
/// Objective display text and quest description (`NNAM`), stage log entries (`CNAM`).
const TEXT_SIGS: &[&str] = &["NNAM", "CNAM"];
/// Quest subrecords that follow the text display globals.
const QTGL_SUCCESSOR_SIGS: &[&str] = &[
    "FLTR", "CTDA", "NEXT", "INDX", "QOBJ", "ANAM", "ALST", "ALLS", "ALCS",
];
const TES4_FLAG_LOCALIZED: u32 = 0x80;
const GLOBAL_TYPE_SHORT: u8 = b's';

pub struct AttachFo76QuestVariablesFixup;

struct QuestText {
    raw_form_id: u32,
    quest: FormKey,
    /// `(subrecord index, string id)` of each localized text subrecord.
    texts: Vec<(usize, u32)>,
}

/// Every language's text for one string id.
type Texts = Vec<(String, String)>;

struct TextMove {
    subrecord_index: usize,
    string_id: u32,
    texts: Texts,
}

struct QuestPlan {
    raw_form_id: u32,
    quest: FormKey,
    names: Vec<String>,
    /// Text shared with a quest planned earlier, rewritten under a fresh id.
    moves: Vec<TextMove>,
}

#[derive(Default)]
struct Plan {
    quests: Vec<QuestPlan>,
    in_place: BTreeMap<u32, Texts>,
    invalid_names: Vec<(u32, String)>,
}

impl Fixup for AttachFo76QuestVariablesFixup {
    fn name(&self) -> &'static str {
        "attach_fo76_quest_variables"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
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
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let interner = mapper.interner;
        if session.target_slot().parsed.header.flags & TES4_FLAG_LOCALIZED == 0 {
            report.message =
                Some(interner.intern("fo76_quest_variables:skipped=target_not_localized"));
            return Ok(report);
        }
        let quests = scan_quest_texts(session, interner)?;
        let plan = plan_rewrites(quests, session.target_slot().strings_ref());
        for (quest_local, name) in &plan.invalid_names {
            warn(
                &mut report,
                interner,
                *quest_local,
                &format!("invalid_variable_name={name}"),
            );
        }

        let target_id = session.target_id();
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let schema = session.schema().map_err(handle_error)?;
        let mut added_globals = Vec::new();
        let mut quest_globals = Vec::with_capacity(plan.quests.len());
        for quest in &plan.quests {
            let mut globals = Vec::with_capacity(quest.names.len());
            for name in &quest.names {
                let editor_id = global_editor_id(quest.quest.local, name);
                let synthetic = FormKey {
                    plugin: interner.intern(&editor_id),
                    local: 1,
                };
                let global = mapper.allocate_or_resolve(synthetic, None, SigCode(*b"GLOB"));
                if !session
                    .record_exists_in_handle(target_id, &form_key_to_read_str(&global, interner))
                    .map_err(handle_error)?
                {
                    added_globals.push(variable_global(global, &editor_id, interner));
                }
                globals.push(global);
            }
            quest_globals.push(globals);
        }

        let text_moves = apply_string_rewrites(session.target_slot_mut().strings_mut(), &plan);

        let mut changed_quests = FxHashSet::default();
        let mut attached = 0u32;
        let mut present = 0u32;
        let mut unresolved = 0u32;
        for ((quest, globals), moves) in plan.quests.iter().zip(&quest_globals).zip(&text_moves) {
            let quest_local = quest.quest.local;
            let raw_globals = globals
                .iter()
                .map(|global| encode_target_form_id(*global, interner, &target_masters))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| FixupError::Other("cannot encode quest variable global".into()))?;
            let patched = {
                let record = session
                    .record_mut(quest.raw_form_id)
                    .map_err(handle_error)?;
                patch_quest_record(record, moves, &raw_globals)
            };
            match patched {
                Ok(true) => {
                    changed_quests.insert(quest.raw_form_id);
                    session.target_slot_mut().clear_record_count_cache();
                    session.record_effect(WriteEffect::RecordContents {
                        form_ids: smallvec::smallvec![quest.raw_form_id],
                    });
                }
                Ok(false) => {}
                Err(reason) => {
                    unresolved += 1;
                    warn(&mut report, interner, quest_local, reason);
                    continue;
                }
            }

            let Some(vmad) = variables_vmad(
                &quest.names,
                globals,
                &target_masters,
                &target_plugin,
                interner,
            ) else {
                unresolved += 1;
                warn(&mut report, interner, quest_local, "vmad_encode_failed");
                continue;
            };
            // Primes the FormKey cache so the attach skips a whole-plugin lookup.
            session
                .indexed_raw_form_id_and_signature(&quest.quest, interner)
                .map_err(handle_error)?;
            match attach_quest_script(session, &quest.quest, SCRIPT_NAME, &vmad)? {
                Some(AttachResult::Changed) => {
                    changed_quests.insert(quest.raw_form_id);
                    attached += 1;
                }
                Some(AttachResult::AlreadyPresent) => present += 1,
                Some(AttachResult::Conflict(reason)) => {
                    unresolved += 1;
                    warn(&mut report, interner, quest_local, reason);
                }
                None => {
                    unresolved += 1;
                    warn(&mut report, interner, quest_local, "quest_vmad_missing");
                }
            }
        }

        let globals_added = added_globals.len();
        if !added_globals.is_empty() {
            session
                .add_records(added_globals, schema.as_ref(), interner)
                .map_err(handle_error)?;
        }
        report.records_added = globals_added as u32;
        report.records_changed = changed_quests.len() as u32;
        report.message = Some(interner.intern(&format!(
            "fo76_quest_variables:quests={};variables={};globals_added={globals_added};strings_rewritten={};strings_moved={};attached={attached};present={present};unresolved={unresolved}",
            plan.quests.len(),
            quest_globals.iter().map(Vec::len).sum::<usize>(),
            plan.in_place.len(),
            text_moves.iter().flatten().count(),
        )));
        Ok(report)
    }
}

fn scan_quest_texts(
    session: &mut PluginSession,
    interner: &StringInterner,
) -> Result<Vec<QuestText>, FixupError> {
    let plugin = interner.intern(&session.target_slot().parsed.plugin_name);
    let own_index = session.target_masters().len() as u32;
    let target_id = session.target_id();
    let scan = session.handle_raw_scan(target_id).map_err(handle_error)?;
    let mut raw_form_ids = scan
        .raw_form_ids_of_sig(SigCode(*b"QUST"))
        .into_iter()
        .filter(|raw| raw >> 24 >= own_index)
        .collect::<Vec<_>>();
    raw_form_ids.sort_unstable();
    Ok(raw_form_ids
        .into_iter()
        .filter_map(|raw_form_id| {
            scan.with_record_subrecords(raw_form_id, |subrecords| QuestText {
                raw_form_id,
                quest: FormKey {
                    plugin,
                    local: raw_form_id & 0x00FF_FFFF,
                },
                texts: text_string_ids(subrecords),
            })
        })
        .collect())
}

fn text_string_ids(subrecords: &[ParsedSubrecord]) -> Vec<(usize, u32)> {
    subrecords
        .iter()
        .enumerate()
        .filter(|(_, subrecord)| TEXT_SIGS.contains(&subrecord.signature.as_str()))
        .filter_map(|(index, subrecord)| {
            let id = u32::from_le_bytes(subrecord.data.as_ref().try_into().ok()?);
            (id != 0).then_some((index, id))
        })
        .collect()
}

/// Plan every quest's text rewrite from the unmodified string table.
///
/// Identical text converted from different quests can share one string id, but
/// each quest's globals are its own, so only the first quest rewrites a shared
/// id in place and later ones move to a fresh id.
fn plan_rewrites(quests: Vec<QuestText>, strings: &LocalizedStringsState) -> Plan {
    let languages = ordered_languages(strings);
    let mut plan = Plan::default();
    let mut claims = FxHashMap::<u32, u32>::default();
    for quest in quests {
        let texts = quest
            .texts
            .iter()
            .map(|(index, id)| {
                let texts = languages
                    .iter()
                    .filter_map(|language| {
                        Some((language.clone(), strings.resolve(language, *id)?))
                    })
                    .collect::<Texts>();
                (*index, *id, texts)
            })
            .collect::<Vec<_>>();
        let quest_local = quest.quest.local;
        let (names, invalid) = referenced_variables(
            texts
                .iter()
                .flat_map(|(_, _, texts)| texts.iter().map(|(_, text)| text.as_str())),
            quest_local,
        );
        plan.invalid_names
            .extend(invalid.into_iter().map(|name| (quest_local, name)));
        if names.is_empty() {
            continue;
        }

        let mut moves = Vec::new();
        for (subrecord_index, string_id, texts) in texts {
            let mut changed = false;
            let rewritten = texts
                .into_iter()
                .map(
                    |(language, text)| match rewrite_variable_tags(&text, quest_local, &names) {
                        Some(rewritten) => {
                            changed = true;
                            (language, rewritten)
                        }
                        None => (language, text),
                    },
                )
                .collect::<Texts>();
            if !changed {
                continue;
            }
            match *claims.entry(string_id).or_insert(quest_local) {
                owner if owner != quest_local => moves.push(TextMove {
                    subrecord_index,
                    string_id,
                    texts: rewritten,
                }),
                _ => {
                    plan.in_place.insert(string_id, rewritten);
                }
            }
        }
        plan.quests.push(QuestPlan {
            raw_form_id: quest.raw_form_id,
            quest: quest.quest,
            names,
            moves,
        });
    }
    plan
}

fn ordered_languages(strings: &LocalizedStringsState) -> Vec<String> {
    let mut languages = strings.language_codes();
    if let Some(default) = languages
        .iter()
        .position(|language| *language == strings.default_language)
    {
        let default = languages.remove(default);
        languages.insert(0, default);
    }
    languages
}

/// Write the planned text; returns each quest's `(subrecord index, old id, new id)` moves.
fn apply_string_rewrites(
    strings: &mut LocalizedStringsState,
    plan: &Plan,
) -> Vec<Vec<(usize, u32, u32)>> {
    let has_moves = plan.quests.iter().any(|quest| !quest.moves.is_empty());
    if plan.in_place.is_empty() && !has_moves {
        return vec![Vec::new(); plan.quests.len()];
    }
    strings.materialize_all();
    for (string_id, texts) in &plan.in_place {
        write_texts(strings, *string_id, texts);
    }
    let mut next_id = strings
        .by_language
        .values()
        .flat_map(|table| table.keys())
        .chain(strings.table_types.keys())
        .copied()
        .max()
        .unwrap_or(0)
        .saturating_add(1);
    plan.quests
        .iter()
        .map(|quest| {
            let mut moved = FxHashMap::<u32, u32>::default();
            quest
                .moves
                .iter()
                .map(|text_move| {
                    let new_id = *moved.entry(text_move.string_id).or_insert_with(|| {
                        let new_id = next_id;
                        next_id = next_id.saturating_add(1);
                        write_texts(strings, new_id, &text_move.texts);
                        if let Some(table_type) = strings.table_types.get(&text_move.string_id) {
                            strings.table_types.insert(new_id, table_type.clone());
                        }
                        new_id
                    });
                    (text_move.subrecord_index, text_move.string_id, new_id)
                })
                .collect()
        })
        .collect()
}

fn write_texts(strings: &mut LocalizedStringsState, string_id: u32, texts: &Texts) {
    for (language, text) in texts {
        strings
            .by_language
            .entry(language.clone())
            .or_default()
            .insert(string_id, text.clone());
    }
}

/// Repoint moved text subrecords and list any missing variable globals in `QTGL`.
fn patch_quest_record(
    record: &mut ParsedRecord,
    moves: &[(usize, u32, u32)],
    globals: &[u32],
) -> Result<bool, &'static str> {
    if record.subrecords.is_empty() {
        return Err("quest_subrecords_unavailable");
    }
    let moves_valid = moves.iter().all(|(index, old_id, _)| {
        record.subrecords.get(*index).is_some_and(|subrecord| {
            TEXT_SIGS.contains(&subrecord.signature.as_str())
                && subrecord.data.as_ref() == old_id.to_le_bytes()
        })
    });
    if !moves_valid {
        return Err("quest_text_subrecord_changed");
    }
    for (index, _, new_id) in moves {
        record.subrecords[*index].data = Bytes::copy_from_slice(&new_id.to_le_bytes());
    }

    let listed = record
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.signature.as_str() == "QTGL")
        .filter_map(|subrecord| Some(u32::from_le_bytes(subrecord.data.as_ref().try_into().ok()?)))
        .collect::<FxHashSet<_>>();
    let missing = globals
        .iter()
        .filter(|global| !listed.contains(global))
        .map(|global| ParsedSubrecord {
            signature: SmolStr::new("QTGL"),
            data: Bytes::copy_from_slice(&global.to_le_bytes()),
            semantic_type: None,
        })
        .collect::<Vec<_>>();
    let changed = !moves.is_empty() || !missing.is_empty();
    if !missing.is_empty() {
        let insert_at = record
            .subrecords
            .iter()
            .rposition(|subrecord| subrecord.signature.as_str() == "QTGL")
            .map(|last| last + 1)
            .or_else(|| {
                record.subrecords.iter().position(|subrecord| {
                    QTGL_SUCCESSOR_SIGS.contains(&subrecord.signature.as_str())
                })
            })
            .unwrap_or(record.subrecords.len());
        record.subrecords.splice(insert_at..insert_at, missing);
    }
    if changed {
        record.raw_payload = None;
    }
    Ok(changed)
}

/// `(start, end, name)` of every `open…>` tag in `text`, matched case-insensitively.
fn tags<'a>(text: &'a str, open: &str) -> Vec<(usize, usize, &'a str)> {
    let lower = text.to_ascii_lowercase();
    let open = open.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(start) = lower[cursor..].find(&open).map(|offset| cursor + offset) {
        let name_start = start + open.len();
        let Some(length) = text[name_start..].find('>') else {
            break;
        };
        let end = name_start + length + 1;
        found.push((start, end, &text[name_start..end - 1]));
        cursor = end;
    }
    found
}

fn is_valid_variable_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn global_editor_id(quest_local: u32, name: &str) -> String {
    format!("{GLOBAL_EDITOR_ID_PREFIX}{quest_local:06X}_{name}")
}

/// Variable names a quest's text references, sorted, and the malformed ones.
///
/// Names come from `<Variable=Name>` tags and, once rewritten, from this
/// quest's own `<Global=…>` tags, so a rerun rebuilds the same set. Names
/// compare case-insensitively like EditorIDs; the first spelling wins.
fn referenced_variables<'a>(
    texts: impl IntoIterator<Item = &'a str>,
    quest_local: u32,
) -> (Vec<String>, Vec<String>) {
    let own_prefix = global_editor_id(quest_local, "");
    let mut names = BTreeMap::<String, String>::new();
    let mut invalid = BTreeMap::<String, String>::new();
    for text in texts {
        let variables = tags(text, VARIABLE_TAG)
            .into_iter()
            .map(|(_, _, name)| name);
        let own_globals = tags(text, GLOBAL_TAG)
            .into_iter()
            .filter_map(|(_, _, editor_id)| {
                let prefix = editor_id.get(..own_prefix.len())?;
                prefix
                    .eq_ignore_ascii_case(&own_prefix)
                    .then(|| &editor_id[own_prefix.len()..])
            });
        for name in variables.chain(own_globals) {
            let target = if is_valid_variable_name(name) {
                &mut names
            } else {
                &mut invalid
            };
            target
                .entry(name.to_ascii_lowercase())
                .or_insert_with(|| name.to_string());
        }
    }
    (
        names.into_values().collect(),
        invalid.into_values().collect(),
    )
}

fn rewrite_variable_tags(text: &str, quest_local: u32, names: &[String]) -> Option<String> {
    let mut rewritten = String::with_capacity(text.len() + 32);
    let mut cursor = 0;
    for (start, end, name) in tags(text, VARIABLE_TAG) {
        let Some(name) = names.iter().find(|known| known.eq_ignore_ascii_case(name)) else {
            continue;
        };
        rewritten.push_str(&text[cursor..start]);
        rewritten.push_str(GLOBAL_TAG);
        rewritten.push_str(&global_editor_id(quest_local, name));
        rewritten.push('>');
        cursor = end;
    }
    (cursor > 0).then(|| {
        rewritten.push_str(&text[cursor..]);
        rewritten
    })
}

fn variable_global(global: FormKey, editor_id: &str, interner: &StringInterner) -> Record {
    let mut record = Record::new(SigCode(*b"GLOB"), global);
    let editor_id = interner.intern(editor_id);
    record.eid = Some(editor_id);
    record.fields.extend([
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(editor_id),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FNAM"),
            value: FieldValue::Bytes(vec![GLOBAL_TYPE_SHORT].into()),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FLTV"),
            value: FieldValue::Float(0.0),
        },
    ]);
    record
}

fn variables_vmad(
    names: &[String],
    globals: &[FormKey],
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let properties = vec![
        string_array_property("VariableNames", names),
        object_array_property("VariableGlobals", globals, interner)?,
    ];
    script_vmad(SCRIPT_NAME, properties, target_masters, target_plugin)
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn warn(report: &mut FixupReport, interner: &StringInterner, quest_local: u32, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "attach_fo76_quest_variables:{quest_local:06X}:{reason}"
    )));
}

#[cfg(test)]
mod tests;
