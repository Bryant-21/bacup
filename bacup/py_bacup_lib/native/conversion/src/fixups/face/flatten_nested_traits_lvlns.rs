//! Flatten humanoid leveled-list entries whose NPC delegates Traits to another LVLN.
//!
//! FO76 uses chains such as `LVLN -> NPC_ -> Traits LVLN -> NPC_` to choose a
//! faction role and then a concrete face/gender. FO4 can consume a leveled list
//! as a template, but this nested selection leaves the outer actor without a
//! concrete face and matching FaceGen asset. Replace the intermediate NPC entry
//! with the delegated list's entries while preserving level and count. When a
//! source list instead uses conditions that FO4 translation drops, redirect NPC
//! template slots to its single unconditional fallback actor. Preserve role NPCs
//! that delegate non-Traits slots because flattening them would discard their
//! inventory, factions, AI, and other role data.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const LVLO_REFERENCE_OFFSET: usize = 4;
const LVLO_COUNT_OFFSET: usize = 8;
const LVLO_MIN_LEN: usize = 12;
const MAX_LVLO_ENTRIES: usize = u8::MAX as usize;

pub struct FlattenNestedTraitsLvlnsFixup;

impl Fixup for FlattenNestedTraitsLvlnsFixup {
    fn name(&self) -> &'static str {
        "flatten_nested_traits_lvlns"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::WholePluginSafe
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn convergent(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config
            .root_sig
            .is_none_or(|sig| matches!(sig.as_str(), "NPC_" | "LVLN"))
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
        let target_masters = session.target_masters().to_vec();
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let lvln_sig = SigCode::from_str("LVLN")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let npc_sig = SigCode::from_str("NPC_")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;

        let mut lvln_records = FxHashMap::default();
        for form_key in session
            .form_keys_of_sig(lvln_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
        {
            if let Ok(record) = session.record_decoded(&form_key, schema.as_ref(), mapper.interner)
            {
                lvln_records.insert(form_key, record);
            }
        }
        if lvln_records.is_empty() {
            return Ok(FixupReport::empty());
        }

        let mut delegated_traits = FxHashMap::default();
        let mut non_traits_template_lists = FxHashSet::default();
        let mut named_npcs = FxHashMap::default();
        let mut name_candidates = Vec::new();
        let mut npc_records = FxHashMap::default();
        for form_key in session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
        {
            let Ok(record) = session.record_decoded(&form_key, schema.as_ref(), mapper.interner)
            else {
                continue;
            };
            let traits_template =
                traits_template_form_key(&record, &target_masters, &target_plugin, mapper.interner);
            if let Some(target) = traits_template.filter(|target| lvln_records.contains_key(target))
            {
                delegated_traits.insert(form_key, target);
            }
            non_traits_template_lists.extend(
                non_traits_template_form_keys(
                    &record,
                    &target_masters,
                    &target_plugin,
                    mapper.interner,
                )
                .filter(|target| lvln_records.contains_key(target)),
            );
            if let Some(name) = npc_own_display_name(&record) {
                named_npcs.insert(form_key, name);
            } else if traits_template.is_some() {
                if let Some(default_template) = default_template_form_key(
                    &record,
                    &target_masters,
                    &target_plugin,
                    mapper.interner,
                )
                .filter(|target| lvln_records.contains_key(target))
                {
                    name_candidates.push((form_key, default_template));
                }
            }
            npc_records.insert(form_key, record);
        }

        let conditional_fallbacks = conditional_template_fallbacks(
            session,
            mapper,
            &non_traits_template_lists,
            &npc_records,
            &target_masters,
            &target_plugin,
        );
        let mut changed_records = FxHashMap::default();
        let mut template_rewrites = FxHashSet::default();
        let mut names_baked = Vec::new();
        let mut lists_flattened = Vec::new();
        let mut role_entries_preserved = Vec::new();
        for (form_key, record) in &npc_records {
            let mut changed = record.clone();
            let replacements = rewrite_npc_template_refs(
                &mut changed,
                &conditional_fallbacks,
                &target_masters,
                &target_plugin,
                mapper.interner,
            );
            if !replacements.is_empty() {
                template_rewrites.extend(
                    replacements
                        .into_iter()
                        .map(|(list, fallback)| (*form_key, list, fallback)),
                );
                changed_records.insert(*form_key, changed);
            }
        }
        for (form_key, default_template) in name_candidates {
            let Some((full, shrt)) = unique_reachable_display_name(
                default_template,
                &lvln_records,
                &named_npcs,
                &target_masters,
                &target_plugin,
                mapper.interner,
            ) else {
                continue;
            };
            let Some(mut record) = changed_records
                .remove(&form_key)
                .or_else(|| npc_records.get(&form_key).cloned())
            else {
                continue;
            };
            if bake_display_name(&mut record, full, shrt) {
                changed_records.insert(form_key, record);
                names_baked.push((form_key, default_template));
            }
        }
        for record in lvln_records.values() {
            let mut changed = record.clone();
            if flatten_nested_traits_entries(
                &mut changed,
                &lvln_records,
                &delegated_traits,
                &non_traits_template_lists,
                &npc_records,
                &target_masters,
                &target_plugin,
                mapper.interner,
                &mut role_entries_preserved,
            ) {
                lists_flattened.push(changed.form_key);
                changed_records.insert(changed.form_key, changed);
            }
        }

        let changed = session
            .replace_records_contents(
                changed_records.into_values().collect(),
                schema.as_ref(),
                mapper.interner,
            )
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let mut report = FixupReport::empty();
        report.records_changed = changed.try_into().unwrap_or(u32::MAX);
        if report.records_changed != 0 || !role_entries_preserved.is_empty() {
            report.message = Some(mapper.interner.intern(&nested_traits_log_message(
                &template_rewrites,
                &names_baked,
                &lists_flattened,
                &role_entries_preserved,
            )));
        }
        Ok(report)
    }
}

const LOG_ITEM_LIMIT: usize = 12;

fn nested_traits_log_message(
    template_rewrites: &FxHashSet<(FormKey, FormKey, FormKey)>,
    names_baked: &[(FormKey, FormKey)],
    lists_flattened: &[FormKey],
    role_entries_preserved: &[(FormKey, FormKey, FormKey, FormKey)],
) -> String {
    let mut rewrites = template_rewrites
        .iter()
        .map(|(actor, list, fallback)| {
            format!(
                "{:06X}:{:06X}->{:06X}",
                actor.local & 0x00FF_FFFF,
                list.local & 0x00FF_FFFF,
                fallback.local & 0x00FF_FFFF
            )
        })
        .collect::<Vec<_>>();
    let mut names = names_baked
        .iter()
        .map(|(actor, list)| {
            format!(
                "{:06X}<-{:06X}",
                actor.local & 0x00FF_FFFF,
                list.local & 0x00FF_FFFF
            )
        })
        .collect::<Vec<_>>();
    let mut lists = lists_flattened
        .iter()
        .map(|list| format!("{:06X}", list.local & 0x00FF_FFFF))
        .collect::<Vec<_>>();
    let mut preserved = role_entries_preserved
        .iter()
        .map(|(list, actor, traits, non_traits)| {
            format!(
                "{:06X}:{:06X}->{:06X}(non_traits={:06X})",
                list.local & 0x00FF_FFFF,
                actor.local & 0x00FF_FFFF,
                traits.local & 0x00FF_FFFF,
                non_traits.local & 0x00FF_FFFF
            )
        })
        .collect::<Vec<_>>();
    rewrites.sort_unstable();
    names.sort_unstable();
    lists.sort_unstable();
    preserved.sort_unstable();
    preserved.dedup();

    format!(
        "template_fallbacks={}{} names_baked={}{} lists_flattened={}{} role_entries_preserved={}{}",
        rewrites.len(),
        format_log_items(&rewrites),
        names.len(),
        format_log_items(&names),
        lists.len(),
        format_log_items(&lists),
        preserved.len(),
        format_all_log_items(&preserved),
    )
}

fn format_log_items(items: &[String]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let visible = items
        .iter()
        .take(LOG_ITEM_LIMIT)
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    if items.len() > LOG_ITEM_LIMIT {
        format!(" [{visible},+{} more]", items.len() - LOG_ITEM_LIMIT)
    } else {
        format!(" [{visible}]")
    }
}

fn format_all_log_items(items: &[String]) -> String {
    if items.is_empty() {
        String::new()
    } else {
        format!(" [{}]", items.join(","))
    }
}

fn conditional_template_fallbacks(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    template_lists: &FxHashSet<FormKey>,
    target_npcs: &FxHashMap<FormKey, Record>,
    target_masters: &[String],
    target_plugin: &str,
) -> FxHashMap<FormKey, FormKey> {
    if template_lists.is_empty() {
        return FxHashMap::default();
    }
    let Ok(source_schema) = session.source_schema() else {
        return FxHashMap::default();
    };
    let Some(source_slot) = session.source_slot_opt() else {
        return FxHashMap::default();
    };
    let source_masters = source_slot.parsed.header.masters.clone();
    let source_plugin = source_slot.parsed.plugin_name.clone();
    let target_to_source = mapper
        .source_to_target_iter()
        .map(|(source, target)| (target, source))
        .collect::<FxHashMap<_, _>>();
    let mut replacements = FxHashMap::default();

    for target_list in template_lists {
        let Some(source_list) = target_to_source.get(target_list).copied() else {
            continue;
        };
        let Ok(source_record) =
            session.source_record_decoded(&source_list, source_schema.as_ref(), mapper.interner)
        else {
            continue;
        };
        let Some(source_fallback) = source_conditional_fallback(
            &source_record,
            &source_masters,
            &source_plugin,
            mapper.interner,
        ) else {
            continue;
        };
        let Some(target_fallback) = mapper.lookup(source_fallback) else {
            continue;
        };
        if target_npcs.contains_key(&target_fallback)
            && encode_raw_form_id(
                target_fallback,
                target_masters,
                target_plugin,
                mapper.interner,
            )
            .is_some()
        {
            replacements.insert(*target_list, target_fallback);
        }
    }

    replacements
}

fn source_conditional_fallback(
    record: &Record,
    source_masters: &[String],
    source_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    let mut current_entry = None;
    let mut current_has_condition = false;
    let mut saw_condition = false;
    let mut unconditional_entries = Vec::new();

    for field in &record.fields {
        if field.sig.as_str() == "LVLO" {
            if let Some(entry) = current_entry.take()
                && !current_has_condition
            {
                unconditional_entries.push(entry);
            }
            current_entry = lvlo_reference(&field.value, source_masters, source_plugin, interner);
            current_has_condition = false;
        } else if current_entry.is_some() && matches!(field.sig.as_str(), "CTDA" | "CTDT") {
            current_has_condition = true;
            saw_condition = true;
        }
    }
    if let Some(entry) = current_entry
        && !current_has_condition
    {
        unconditional_entries.push(entry);
    }

    (saw_condition && unconditional_entries.len() == 1).then(|| unconditional_entries[0])
}

fn rewrite_npc_template_refs(
    record: &mut Record,
    replacements: &FxHashMap<FormKey, FormKey>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> FxHashSet<(FormKey, FormKey)> {
    if replacements.is_empty() {
        return FxHashSet::default();
    }
    let mut applied = FxHashSet::default();
    for field in &mut record.fields {
        if !matches!(field.sig.as_str(), "TPLT" | "TPTA") {
            continue;
        }
        rewrite_template_value(
            &mut field.value,
            replacements,
            target_masters,
            target_plugin,
            interner,
            &mut applied,
        );
    }
    applied
}

fn rewrite_template_value(
    value: &mut FieldValue,
    replacements: &FxHashMap<FormKey, FormKey>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
    applied: &mut FxHashSet<(FormKey, FormKey)>,
) -> bool {
    match value {
        FieldValue::FormKey(form_key) => {
            let original = *form_key;
            let Some(replacement) = replacements.get(&original).copied() else {
                return false;
            };
            *form_key = replacement;
            applied.insert((original, replacement));
            true
        }
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
            let Some(form_key) =
                resolve_raw_form_id(*raw as u32, target_masters, target_plugin, interner)
            else {
                return false;
            };
            let Some(replacement_form_key) = replacements.get(&form_key).copied() else {
                return false;
            };
            let Some(replacement) = encode_raw_form_id(
                replacement_form_key,
                target_masters,
                target_plugin,
                interner,
            ) else {
                return false;
            };
            *raw = replacement as u64;
            applied.insert((form_key, replacement_form_key));
            true
        }
        FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => {
            let mut unsigned = FieldValue::Uint(*raw as u64);
            let changed = rewrite_template_value(
                &mut unsigned,
                replacements,
                target_masters,
                target_plugin,
                interner,
                applied,
            );
            if let FieldValue::Uint(replacement) = unsigned {
                *raw = replacement as i64;
            }
            changed
        }
        FieldValue::Bytes(bytes) => {
            let mut changed = false;
            for chunk in bytes.chunks_exact_mut(4) {
                let raw = u32::from_le_bytes(chunk.try_into().unwrap());
                let Some(form_key) =
                    resolve_raw_form_id(raw, target_masters, target_plugin, interner)
                else {
                    continue;
                };
                let Some(replacement_form_key) = replacements.get(&form_key).copied() else {
                    continue;
                };
                let Some(replacement) = encode_raw_form_id(
                    replacement_form_key,
                    target_masters,
                    target_plugin,
                    interner,
                ) else {
                    continue;
                };
                chunk.copy_from_slice(&replacement.to_le_bytes());
                applied.insert((form_key, replacement_form_key));
                changed = true;
            }
            changed
        }
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_template_value(
                item,
                replacements,
                target_masters,
                target_plugin,
                interner,
                applied,
            ) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            rewrite_template_value(
                item,
                replacements,
                target_masters,
                target_plugin,
                interner,
                applied,
            ) | changed
        }),
        _ => false,
    }
}

fn encode_raw_form_id(
    form_key: FormKey,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<u32> {
    if form_key.local == 0 || form_key.local > 0x00FF_FFFF {
        return None;
    }
    let plugin = interner.resolve(form_key.plugin)?;
    let master_index = if plugin.eq_ignore_ascii_case(target_plugin) {
        target_masters.len()
    } else {
        target_masters
            .iter()
            .position(|master| master.eq_ignore_ascii_case(plugin))?
    };
    (master_index <= u8::MAX as usize).then(|| ((master_index as u32) << 24) | form_key.local)
}

fn traits_template_form_key(
    record: &Record,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "TPTA")
        .find_map(|field| match &field.value {
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => resolve_raw_form_id(
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                target_masters,
                target_plugin,
                interner,
            ),
            FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
                interner
                    .resolve(*name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("traits"))
                    .then(|| form_key_value(value, target_masters, target_plugin, interner))
                    .flatten()
            }),
            _ => None,
        })
}

fn non_traits_template_form_keys<'a>(
    record: &'a Record,
    target_masters: &'a [String],
    target_plugin: &'a str,
    interner: &'a StringInterner,
) -> impl Iterator<Item = FormKey> + 'a {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "TPTA")
        .flat_map(move |field| match &field.value {
            FieldValue::Bytes(bytes) => bytes
                .chunks_exact(4)
                .skip(1)
                .filter_map(|chunk| {
                    resolve_raw_form_id(
                        u32::from_le_bytes(chunk.try_into().unwrap()),
                        target_masters,
                        target_plugin,
                        interner,
                    )
                })
                .collect::<Vec<_>>(),
            FieldValue::Struct(fields) => fields
                .iter()
                .filter(|(name, _)| {
                    !interner
                        .resolve(*name)
                        .is_some_and(|name| name.eq_ignore_ascii_case("traits"))
                })
                .filter_map(|(_, value)| {
                    form_key_value(value, target_masters, target_plugin, interner)
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
}

fn default_template_form_key(
    record: &Record,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "TPLT")
        .and_then(|field| form_key_value(&field.value, target_masters, target_plugin, interner))
}

fn npc_own_display_name(record: &Record) -> Option<(FieldEntry, Option<FieldEntry>)> {
    let full = record
        .fields
        .iter()
        .rev()
        .find(|field| field.sig.as_str() == "FULL")?
        .clone();
    let shrt = record
        .fields
        .iter()
        .rev()
        .find(|field| field.sig.as_str() == "SHRT")
        .cloned();
    Some((full, shrt))
}

fn unique_reachable_display_name(
    start: FormKey,
    lvln_records: &FxHashMap<FormKey, Record>,
    named_npcs: &FxHashMap<FormKey, (FieldEntry, Option<FieldEntry>)>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<(FieldEntry, Option<FieldEntry>)> {
    let mut names = Vec::new();
    collect_reachable_display_names(
        start,
        lvln_records,
        named_npcs,
        target_masters,
        target_plugin,
        interner,
        &mut FxHashSet::default(),
        0,
        &mut names,
    );
    let first = names.first()?.clone();
    names
        .iter()
        .all(|(full, _)| full.value == first.0.value)
        .then_some(first)
}

#[allow(clippy::too_many_arguments)]
fn collect_reachable_display_names(
    current: FormKey,
    lvln_records: &FxHashMap<FormKey, Record>,
    named_npcs: &FxHashMap<FormKey, (FieldEntry, Option<FieldEntry>)>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
    visited: &mut FxHashSet<FormKey>,
    depth: u8,
    names: &mut Vec<(FieldEntry, Option<FieldEntry>)>,
) {
    if depth > 8 || !visited.insert(current) {
        return;
    }
    let Some(record) = lvln_records.get(&current) else {
        return;
    };
    for field in &record.fields {
        if field.sig.as_str() != "LVLO" {
            continue;
        }
        let Some(target) = lvlo_reference(&field.value, target_masters, target_plugin, interner)
        else {
            continue;
        };
        if let Some(name) = named_npcs.get(&target) {
            names.push(name.clone());
        } else if lvln_records.contains_key(&target) {
            collect_reachable_display_names(
                target,
                lvln_records,
                named_npcs,
                target_masters,
                target_plugin,
                interner,
                visited,
                depth + 1,
                names,
            );
        }
    }
}

fn bake_display_name(record: &mut Record, full: FieldEntry, shrt: Option<FieldEntry>) -> bool {
    if record
        .fields
        .iter()
        .any(|field| field.sig.as_str() == "FULL")
    {
        return false;
    }
    let mut position = record
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "DATA")
        .unwrap_or(record.fields.len());
    record.fields.insert(position, full);
    if let Some(shrt) = shrt {
        position += 1;
        record.fields.insert(position, shrt);
    }
    true
}

fn flatten_nested_traits_entries(
    record: &mut Record,
    lvln_records: &FxHashMap<FormKey, Record>,
    delegated_traits: &FxHashMap<FormKey, FormKey>,
    non_traits_template_lists: &FxHashSet<FormKey>,
    npc_records: &FxHashMap<FormKey, Record>,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
    role_entries_preserved: &mut Vec<(FormKey, FormKey, FormKey, FormKey)>,
) -> bool {
    if non_traits_template_lists.contains(&record.form_key) {
        return false;
    }
    let Ok(lvlo_sig) = SubrecordSig::from_str("LVLO") else {
        return false;
    };
    let mut changed = false;
    let mut output: SmallVec<[FieldEntry; 8]> = SmallVec::with_capacity(record.fields.len());

    for field in record.fields.drain(..) {
        if field.sig != lvlo_sig {
            output.push(field);
            continue;
        }
        let Some(entry_npc) = lvlo_reference(&field.value, target_masters, target_plugin, interner)
        else {
            output.push(field);
            continue;
        };
        let Some(target_lvln) = delegated_traits.get(&entry_npc).copied() else {
            output.push(field);
            continue;
        };
        // Flattening a role NPC would replace all of its delegated slots with
        // face-list entries, losing inventory, factions, AI, and packages.
        if let Some(non_traits) = npc_records.get(&entry_npc).and_then(|npc| {
            non_traits_template_form_keys(npc, target_masters, target_plugin, interner).next()
        }) {
            role_entries_preserved.push((record.form_key, entry_npc, target_lvln, non_traits));
            output.push(field);
            continue;
        }
        if target_lvln == record.form_key {
            output.push(field);
            continue;
        }
        let Some(delegate) = lvln_records.get(&target_lvln) else {
            output.push(field);
            continue;
        };

        let parent_level = lvlo_level(&field.value, interner).unwrap_or(1);
        let parent_count = lvlo_count(&field.value, interner).unwrap_or(1).max(1);
        let mut replacements = delegate
            .fields
            .iter()
            .filter(|candidate| candidate.sig == lvlo_sig)
            .cloned()
            .collect::<Vec<_>>();
        if replacements.is_empty() {
            output.push(field);
            continue;
        }
        if crate::drop_trace::enabled() {
            let reason = format!(
                "replaced_nested_traits_role entry={:06X} traits={:06X} replacements={}",
                entry_npc.local & 0x00FF_FFFF,
                target_lvln.local & 0x00FF_FFFF,
                replacements.len()
            );
            crate::drop_trace::trace(
                "flatten_nested_traits_lvlns",
                "LVLN",
                record.form_key.local,
                "LVLO",
                &reason,
            );
        }
        for replacement in &mut replacements {
            let level = parent_level.max(lvlo_level(&replacement.value, interner).unwrap_or(1));
            let count = parent_count
                .saturating_mul(lvlo_count(&replacement.value, interner).unwrap_or(1).max(1));
            set_lvlo_level_and_count(&mut replacement.value, level, count, interner);
        }
        let remaining = MAX_LVLO_ENTRIES.saturating_sub(
            output
                .iter()
                .filter(|candidate| candidate.sig == lvlo_sig)
                .count(),
        );
        output.extend(replacements.into_iter().take(remaining));
        changed = true;
    }

    if changed {
        sync_llct_count(&mut output);
    }
    record.fields = output;
    changed
}

fn lvlo_reference(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= LVLO_REFERENCE_OFFSET + 4 => {
            resolve_raw_form_id(
                u32::from_le_bytes(
                    bytes[LVLO_REFERENCE_OFFSET..LVLO_REFERENCE_OFFSET + 4]
                        .try_into()
                        .unwrap(),
                ),
                target_masters,
                target_plugin,
                interner,
            )
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
            interner
                .resolve(*name)
                .is_some_and(|name| {
                    name.eq_ignore_ascii_case("npc") || name.eq_ignore_ascii_case("reference")
                })
                .then(|| form_key_value(value, target_masters, target_plugin, interner))
                .flatten()
        }),
        _ => None,
    }
}

fn form_key_value(
    value: &FieldValue,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) if form_key.local != 0 => Some(*form_key),
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
            resolve_raw_form_id(*raw as u32, target_masters, target_plugin, interner)
        }
        FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => {
            resolve_raw_form_id(*raw as u32, target_masters, target_plugin, interner)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => resolve_raw_form_id(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
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
    let master_index = (raw >> 24) as usize;
    let plugin = target_masters
        .get(master_index)
        .map(String::as_str)
        .unwrap_or(target_plugin);
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
    })
}

fn lvlo_level(value: &FieldValue, interner: &StringInterner) -> Option<u16> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes(bytes[0..2].try_into().unwrap()))
        }
        FieldValue::Struct(fields) => named_u16(fields, "level", interner),
        _ => None,
    }
}

fn lvlo_count(value: &FieldValue, interner: &StringInterner) -> Option<u16> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= LVLO_MIN_LEN => Some(u16::from_le_bytes(
            bytes[LVLO_COUNT_OFFSET..LVLO_COUNT_OFFSET + 2]
                .try_into()
                .unwrap(),
        )),
        FieldValue::Struct(fields) => named_u16(fields, "count", interner),
        _ => None,
    }
}

fn named_u16(
    fields: &[(crate::sym::Sym, FieldValue)],
    wanted: &str,
    interner: &StringInterner,
) -> Option<u16> {
    fields.iter().find_map(|(name, value)| {
        if !interner
            .resolve(*name)
            .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
        {
            return None;
        }
        match value {
            FieldValue::Uint(value) => (*value).try_into().ok(),
            FieldValue::Int(value) => (*value).try_into().ok(),
            _ => None,
        }
    })
}

fn set_lvlo_level_and_count(
    value: &mut FieldValue,
    level: u16,
    count: u16,
    interner: &StringInterner,
) {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= LVLO_MIN_LEN => {
            bytes[0..2].copy_from_slice(&level.to_le_bytes());
            bytes[LVLO_COUNT_OFFSET..LVLO_COUNT_OFFSET + 2].copy_from_slice(&count.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            set_named_u16(fields, "level", level, interner);
            set_named_u16(fields, "count", count, interner);
        }
        _ => {}
    }
}

fn set_named_u16(
    fields: &mut [(crate::sym::Sym, FieldValue)],
    wanted: &str,
    replacement: u16,
    interner: &StringInterner,
) {
    for (name, value) in fields {
        if !interner
            .resolve(*name)
            .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
        {
            continue;
        }
        match value {
            FieldValue::Uint(value) => *value = replacement as u64,
            FieldValue::Int(value) => *value = replacement as i64,
            _ => {}
        }
        return;
    }
}

fn sync_llct_count(fields: &mut SmallVec<[FieldEntry; 8]>) {
    let count = fields
        .iter()
        .filter(|field| field.sig.as_str() == "LVLO")
        .count()
        .min(MAX_LVLO_ENTRIES) as u64;
    if let Some(field) = fields.iter_mut().find(|field| field.sig.as_str() == "LLCT") {
        field.value = FieldValue::Uint(count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{Record, RecordFlags};

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn record(sig: &str, local: u32, fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
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

    fn lvlo(level: u16, npc: FormKey, count: u16, interner: &StringInterner) -> FieldEntry {
        field(
            "LVLO",
            FieldValue::Struct(vec![
                (interner.intern("level"), FieldValue::Uint(level as u64)),
                (interner.intern("npc"), FieldValue::FormKey(npc)),
                (interner.intern("count"), FieldValue::Uint(count as u64)),
            ]),
        )
    }

    #[test]
    fn flattens_real_rust_raider_role_to_concrete_face_entries() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let outer = FormKey {
            local: 0x7F0A60,
            plugin,
        };
        let role = FormKey {
            local: 0x7F0A63,
            plugin,
        };
        let faces = FormKey {
            local: 0x7F0A5F,
            plugin,
        };
        let face_a = FormKey {
            local: 0x83E0D3,
            plugin,
        };
        let face_b = FormKey {
            local: 0x83E0D4,
            plugin,
        };
        let mut outer_record = record(
            "LVLN",
            outer.local,
            vec![
                field("LLCT", FieldValue::Uint(1)),
                lvlo(5, role, 2, &interner),
            ],
            &interner,
        );
        let faces_record = record(
            "LVLN",
            faces.local,
            vec![
                field("LLCT", FieldValue::Uint(2)),
                lvlo(1, face_a, 1, &interner),
                lvlo(10, face_b, 3, &interner),
            ],
            &interner,
        );
        let mut lists = FxHashMap::default();
        lists.insert(outer, outer_record.clone());
        lists.insert(faces, faces_record);
        let mut delegates = FxHashMap::default();
        delegates.insert(role, faces);
        let npc_records = FxHashMap::default();
        let mut role_entries_preserved = Vec::new();

        assert!(flatten_nested_traits_entries(
            &mut outer_record,
            &lists,
            &delegates,
            &FxHashSet::default(),
            &npc_records,
            &[],
            "SeventySix.esm",
            &interner,
            &mut role_entries_preserved,
        ));
        assert!(role_entries_preserved.is_empty());
        let entries = outer_record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "LVLO")
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            lvlo_reference(&entries[0].value, &[], "SeventySix.esm", &interner),
            Some(face_a)
        );
        assert_eq!(lvlo_level(&entries[0].value, &interner), Some(5));
        assert_eq!(lvlo_count(&entries[0].value, &interner), Some(2));
        assert_eq!(
            lvlo_reference(&entries[1].value, &[], "SeventySix.esm", &interner),
            Some(face_b)
        );
        assert_eq!(lvlo_level(&entries[1].value, &interner), Some(10));
        assert_eq!(lvlo_count(&entries[1].value, &interner), Some(6));
        assert_eq!(
            outer_record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "LLCT")
                .map(|field| &field.value),
            Some(&FieldValue::Uint(2))
        );
    }

    #[test]
    fn traits_slot_resolves_the_delegated_face_list() {
        let interner = StringInterner::new();
        let target = FormKey {
            local: 0x7F0A5F,
            plugin: interner.intern("SeventySix.esm"),
        };
        let npc = record(
            "NPC_",
            0x7F0A63,
            vec![field(
                "TPTA",
                FieldValue::Struct(vec![(
                    interner.intern("Traits"),
                    FieldValue::FormKey(target),
                )]),
            )],
            &interner,
        );
        assert_eq!(
            traits_template_form_key(&npc, &[], "SeventySix.esm", &interner),
            Some(target)
        );
    }

    #[test]
    fn preserves_role_entry_that_delegates_non_traits_slots() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let outer = FormKey {
            local: 0x65321F,
            plugin,
        };
        let role = FormKey {
            local: 0x653237,
            plugin,
        };
        let faces = FormKey {
            local: 0x6529FB,
            plugin,
        };
        let inventory = FormKey {
            local: 0x65322F,
            plugin,
        };
        let face = FormKey {
            local: 0x6529FC,
            plugin,
        };
        let mut outer_record = record(
            "LVLN",
            outer.local,
            vec![
                field("LLCT", FieldValue::Uint(1)),
                lvlo(1, role, 1, &interner),
            ],
            &interner,
        );
        let faces_record = record(
            "LVLN",
            faces.local,
            vec![
                field("LLCT", FieldValue::Uint(1)),
                lvlo(1, face, 1, &interner),
            ],
            &interner,
        );
        let role_record = record(
            "NPC_",
            role.local,
            vec![field(
                "TPTA",
                FieldValue::Struct(vec![
                    (interner.intern("Traits"), FieldValue::FormKey(faces)),
                    (interner.intern("Inventory"), FieldValue::FormKey(inventory)),
                ]),
            )],
            &interner,
        );
        let lists = FxHashMap::from_iter([(outer, outer_record.clone()), (faces, faces_record)]);
        let delegates = FxHashMap::from_iter([(role, faces)]);
        let npc_records = FxHashMap::from_iter([(role, role_record)]);
        let mut role_entries_preserved = Vec::new();

        assert!(!flatten_nested_traits_entries(
            &mut outer_record,
            &lists,
            &delegates,
            &FxHashSet::default(),
            &npc_records,
            &[],
            "SeventySix.esm",
            &interner,
            &mut role_entries_preserved,
        ));
        assert_eq!(
            role_entries_preserved,
            vec![(outer, role, faces, inventory)]
        );
        let entry = outer_record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "LVLO")
            .unwrap();
        assert_eq!(
            lvlo_reference(&entry.value, &[], "SeventySix.esm", &interner),
            Some(role)
        );
        assert_eq!(
            nested_traits_log_message(&FxHashSet::default(), &[], &[], &role_entries_preserved,),
            "template_fallbacks=0 names_baked=0 lists_flattened=0 role_entries_preserved=1 [65321F:653237->6529FB(non_traits=65322F)]"
        );
    }

    #[test]
    fn preserves_role_list_used_by_non_traits_template_slots() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let outer = FormKey {
            local: 0x7F0A62,
            plugin,
        };
        let role = FormKey {
            local: 0x7F0A69,
            plugin,
        };
        let faces = FormKey {
            local: 0x7F0A5F,
            plugin,
        };
        let face = FormKey {
            local: 0x83E0D3,
            plugin,
        };
        let mut outer_record = record(
            "LVLN",
            outer.local,
            vec![
                field("LLCT", FieldValue::Uint(1)),
                lvlo(1, role, 1, &interner),
            ],
            &interner,
        );
        let faces_record = record(
            "LVLN",
            faces.local,
            vec![
                field("LLCT", FieldValue::Uint(1)),
                lvlo(1, face, 1, &interner),
            ],
            &interner,
        );
        let mut lists = FxHashMap::default();
        lists.insert(outer, outer_record.clone());
        lists.insert(faces, faces_record);
        let mut delegates = FxHashMap::default();
        delegates.insert(role, faces);
        let protected = FxHashSet::from_iter([outer]);
        let npc_records = FxHashMap::default();
        let mut role_entries_preserved = Vec::new();

        assert!(!flatten_nested_traits_entries(
            &mut outer_record,
            &lists,
            &delegates,
            &protected,
            &npc_records,
            &[],
            "SeventySix.esm",
            &interner,
            &mut role_entries_preserved,
        ));
        assert!(role_entries_preserved.is_empty());
        let entry = outer_record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "LVLO")
            .unwrap();
        assert_eq!(
            lvlo_reference(&entry.value, &[], "SeventySix.esm", &interner),
            Some(role)
        );
    }

    #[test]
    fn finds_non_traits_template_lists() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let traits = FormKey {
            local: 0x7F0A5F,
            plugin,
        };
        let factions = FormKey {
            local: 0x7F0A62,
            plugin,
        };
        let npc = record(
            "NPC_",
            0x7F0A68,
            vec![field(
                "TPTA",
                FieldValue::Struct(vec![
                    (interner.intern("Traits"), FieldValue::FormKey(traits)),
                    (interner.intern("Factions"), FieldValue::FormKey(factions)),
                ]),
            )],
            &interner,
        );

        assert_eq!(
            non_traits_template_form_keys(&npc, &[], "SeventySix.esm", &interner)
                .collect::<Vec<_>>(),
            vec![factions]
        );
    }

    #[test]
    fn bakes_unique_name_reachable_through_nested_default_template_lists() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let root = FormKey {
            local: 0x59002E,
            plugin,
        };
        let nested = FormKey {
            local: 0x564200,
            plugin,
        };
        let unnamed_face = FormKey {
            local: 0x58925F,
            plugin,
        };
        let named_role = FormKey {
            local: 0x56419D,
            plugin,
        };
        let mut lists = FxHashMap::default();
        lists.insert(
            root,
            record(
                "LVLN",
                root.local,
                vec![lvlo(1, nested, 1, &interner)],
                &interner,
            ),
        );
        lists.insert(
            nested,
            record(
                "LVLN",
                nested.local,
                vec![
                    lvlo(1, unnamed_face, 1, &interner),
                    lvlo(1, named_role, 1, &interner),
                ],
                &interner,
            ),
        );
        let full = field("FULL", FieldValue::String(interner.intern("Settler")));
        let mut names = FxHashMap::default();
        names.insert(named_role, (full.clone(), None));

        let resolved =
            unique_reachable_display_name(root, &lists, &names, &[], "SeventySix.esm", &interner);
        assert_eq!(resolved, Some((full.clone(), None)));

        let mut npc = record(
            "NPC_",
            0x597657,
            vec![field("DATA", FieldValue::None)],
            &interner,
        );
        assert!(bake_display_name(&mut npc, full, None));
        assert_eq!(
            npc.fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["FULL", "DATA"]
        );
    }

    #[test]
    fn w05_conditional_template_uses_the_unconditional_scavenger_fallback() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let template_list = FormKey {
            local: 0x597639,
            plugin,
        };
        let lite_ally = FormKey {
            local: 0x58E6FC,
            plugin,
        };
        let scavenger = FormKey {
            local: 0x597659,
            plugin,
        };
        let source_list = record(
            "LVLN",
            template_list.local,
            vec![
                lvlo(1, lite_ally, 1, &interner),
                field("CTDA", FieldValue::Bytes(SmallVec::from_vec(vec![0; 32]))),
                lvlo(1, scavenger, 1, &interner),
            ],
            &interner,
        );

        assert_eq!(
            source_conditional_fallback(&source_list, &[], "SeventySix.esm", &interner),
            Some(scavenger)
        );

        let unrelated = FormKey {
            local: 0x59002E,
            plugin,
        };
        let mut actor = record(
            "NPC_",
            0x5858D2,
            vec![
                field("TPLT", FieldValue::FormKey(template_list)),
                field(
                    "TPTA",
                    FieldValue::Struct(vec![
                        (
                            interner.intern("Traits"),
                            FieldValue::FormKey(template_list),
                        ),
                        (interner.intern("Stats"), FieldValue::FormKey(template_list)),
                        (interner.intern("Inventory"), FieldValue::FormKey(unrelated)),
                    ]),
                ),
            ],
            &interner,
        );
        let replacements = FxHashMap::from_iter([(template_list, scavenger)]);

        let applied =
            rewrite_npc_template_refs(&mut actor, &replacements, &[], "SeventySix.esm", &interner);
        assert_eq!(applied, FxHashSet::from_iter([(template_list, scavenger)]));
        assert_eq!(
            default_template_form_key(&actor, &[], "SeventySix.esm", &interner),
            Some(scavenger)
        );
        assert_eq!(
            traits_template_form_key(&actor, &[], "SeventySix.esm", &interner),
            Some(scavenger)
        );
        assert_eq!(
            non_traits_template_form_keys(&actor, &[], "SeventySix.esm", &interner)
                .collect::<Vec<_>>(),
            vec![scavenger, unrelated]
        );
        assert_eq!(
            nested_traits_log_message(
                &FxHashSet::from_iter([(actor.form_key, template_list, scavenger)]),
                &[],
                &[],
                &[],
            ),
            "template_fallbacks=1 [5858D2:597639->597659] names_baked=0 lists_flattened=0 role_entries_preserved=0"
        );
    }

    #[test]
    fn ordinary_random_template_list_has_no_forced_fallback() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let list = record(
            "LVLN",
            0x597639,
            vec![
                lvlo(
                    1,
                    FormKey {
                        local: 0x58E6FC,
                        plugin,
                    },
                    1,
                    &interner,
                ),
                lvlo(
                    1,
                    FormKey {
                        local: 0x597659,
                        plugin,
                    },
                    1,
                    &interner,
                ),
            ],
            &interner,
        );

        assert_eq!(
            source_conditional_fallback(&list, &[], "SeventySix.esm", &interner),
            None
        );
    }
}
