//! Restore mapped OMOD target keywords, then isolate genuinely unassociated OMODs.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use esp_authoring_core::plugin_runtime::ensure_core_section;

const DUMMY_KEYWORD_EDITOR_ID: &str = "B21_ma_none";
const DUMMY_KEYWORD_SOURCE_PLUGIN: &str = "__synth_omod_none__";
const KEYWORD_TYPE_MOD_ASSOCIATION: u64 = 5;

pub struct RepairOmodTargetKeywordsFixup;

impl Fixup for RepairOmodTargetKeywordsFixup {
    fn name(&self) -> &'static str {
        "repair_omod_target_keywords"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let target_is_fo4 = session
            .target_slot()
            .parsed
            .game
            .as_deref()
            .is_some_and(|game| game.eq_ignore_ascii_case("fo4"));
        let source_is_fo76 = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            .is_some_and(|game| game.eq_ignore_ascii_case("fo76"));
        target_is_fo4 && source_is_fo76
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let omod_sig = SigCode::from_str("OMOD")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let npc_sig = SigCode::from_str("NPC_")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let kywd_sig = SigCode::from_str("KYWD")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let target_omods = session
            .form_keys_of_sig(omod_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if target_omods.is_empty() {
            return Ok(FixupReport::empty());
        }

        let target_omod_set: FxHashSet<FormKey> = target_omods.iter().copied().collect();
        let target_masters = session.target_masters().to_vec();
        let npc_object_template_omods = collect_target_npc_object_template_omods(
            session,
            mapper.interner,
            &target_omod_set,
            &target_masters,
            npc_sig,
            target_schema.as_ref(),
        )?;
        let mut target_keyword_set: FxHashSet<FormKey> = session
            .form_keys_of_sig(kywd_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .into_iter()
            .collect();
        let source_omod_keywords =
            collect_source_omod_keywords(session, mapper, &target_omod_set, omod_sig)?;
        let mut master_keyword_cache = FxHashMap::default();
        let mut repairs = FxHashMap::default();
        for (target_omod, source_keywords) in source_omod_keywords {
            let mut mapped = Vec::new();
            for source_keyword in source_keywords {
                let Some(target_keyword) = mapper.lookup(source_keyword) else {
                    continue;
                };
                if is_keyword_target(
                    session,
                    mapper.interner,
                    &target_keyword_set,
                    &target_masters,
                    &config.target_master_handle_ids,
                    &mut master_keyword_cache,
                    target_keyword,
                ) && !mapped.contains(&target_keyword)
                {
                    mapped.push(target_keyword);
                }
            }
            if !mapped.is_empty() {
                repairs.insert(target_omod, mapped);
            }
        }

        let mut report = FixupReport::empty();
        let mut pending = Vec::new();
        let mut needs_dummy = false;
        for target_fk in target_omods {
            let mut record =
                match session.record_decoded(&target_fk, target_schema.as_ref(), mapper.interner) {
                    Ok(record) => record,
                    Err(error) => {
                        report.warnings.push(
                            mapper
                                .interner
                                .intern(&format!("repair_omod_target_keywords:read_err:{error}")),
                        );
                        continue;
                    }
                };
            let changed = repairs
                .get(&target_fk)
                .is_some_and(|keywords| merge_target_keywords(&mut record, keywords) > 0);
            let record_needs_dummy =
                omod_needs_dummy_target_keyword(&record, target_fk, &npc_object_template_omods);
            needs_dummy |= record_needs_dummy;
            if changed || record_needs_dummy {
                pending.push((record, changed, record_needs_dummy));
            }
        }

        let dummy_keyword = if needs_dummy {
            let (form_key, added) = ensure_dummy_keyword(
                session,
                mapper,
                target_schema.as_ref(),
                kywd_sig,
                &mut target_keyword_set,
            )?;
            report.records_added = u32::from(added);
            Some(form_key)
        } else {
            None
        };

        let mut changed_records = Vec::with_capacity(pending.len());
        for (mut record, restored, record_needs_dummy) in pending {
            let isolated = record_needs_dummy
                && dummy_keyword.is_some_and(|keyword| {
                    set_target_keywords(&mut record, vec![keyword]);
                    true
                });
            if restored || isolated {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "repair_omod_target_keywords replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn collect_target_npc_object_template_omods(
    session: &mut PluginSession,
    interner: &crate::sym::StringInterner,
    target_omods: &FxHashSet<FormKey>,
    target_masters: &[String],
    npc_sig: SigCode,
    target_schema: &crate::schema::AuthoringSchema,
) -> Result<FxHashSet<FormKey>, FixupError> {
    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let npc_form_keys = session
        .form_keys_of_sig(npc_sig, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let mut referenced = FxHashSet::default();
    for npc_form_key in npc_form_keys {
        let Ok(npc) = session.record_decoded(&npc_form_key, target_schema, interner) else {
            continue;
        };
        for entry in &npc.fields {
            if entry.sig.0 != *b"OBTS" {
                continue;
            }
            collect_obts_omod_references(
                &entry.value,
                &target_plugin,
                target_masters,
                interner,
                &mut referenced,
            );
        }
    }
    referenced.retain(|form_key| target_omods.contains(form_key));
    Ok(referenced)
}

fn collect_obts_omod_references(
    value: &FieldValue,
    own_plugin: &str,
    masters: &[String],
    interner: &crate::sym::StringInterner,
    out: &mut FxHashSet<FormKey>,
) {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 18 => {
            let include_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
            let includes_start = 18 + bytes[15] as usize * 4;
            for index in 0..include_count {
                let offset = includes_start + index * 7;
                let Some(raw) = bytes
                    .get(offset..offset + 4)
                    .map(|row| u32::from_le_bytes(row.try_into().unwrap()))
                else {
                    break;
                };
                if let Some(form_key) = form_key_from_raw(raw, own_plugin, masters, interner) {
                    out.insert(form_key);
                }
            }
        }
        FieldValue::Struct(fields) => {
            let Some(FieldValue::List(includes)) = named_struct_value(fields, "includes", interner)
            else {
                return;
            };
            for include in includes {
                let FieldValue::Struct(include_fields) = include else {
                    continue;
                };
                let Some(mod_value) = named_struct_value(include_fields, "mod", interner) else {
                    continue;
                };
                if let Some(form_key) =
                    form_key_from_value(mod_value, own_plugin, masters, interner)
                {
                    out.insert(form_key);
                }
            }
        }
        _ => {}
    }
}

fn named_struct_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    wanted: &str,
    interner: &crate::sym::StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(name, _)| {
            interner.resolve(*name).is_some_and(|name| {
                name.chars()
                    .filter(|character| character.is_ascii_alphanumeric())
                    .map(|character| character.to_ascii_lowercase())
                    .eq(wanted
                        .chars()
                        .filter(|character| character.is_ascii_alphanumeric())
                        .map(|character| character.to_ascii_lowercase()))
            })
        })
        .map(|(_, value)| value)
}

fn form_key_from_value(
    value: &FieldValue,
    own_plugin: &str,
    masters: &[String],
    interner: &crate::sym::StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) if form_key.local != 0 => Some(*form_key),
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
            form_key_from_raw(*raw as u32, own_plugin, masters, interner)
        }
        FieldValue::Int(raw) if *raw > 0 && *raw <= u32::MAX as i64 => {
            form_key_from_raw(*raw as u32, own_plugin, masters, interner)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => form_key_from_raw(
            u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            own_plugin,
            masters,
            interner,
        ),
        _ => None,
    }
}

fn collect_source_omod_keywords(
    session: &mut PluginSession,
    mapper: &FormKeyMapper,
    target_omods: &FxHashSet<FormKey>,
    omod_sig: SigCode,
) -> Result<FxHashMap<FormKey, Vec<FormKey>>, FixupError> {
    let source_id = session
        .source_id()
        .ok_or_else(|| FixupError::HandleError("source handle is unavailable".into()))?;
    let (source_plugin, source_masters) = {
        let slot = session
            .source_slot_opt()
            .ok_or_else(|| FixupError::HandleError("source handle is unavailable".into()))?;
        (
            slot.parsed.plugin_name.clone(),
            slot.parsed.header.masters.clone(),
        )
    };
    let scan = session
        .handle_raw_scan(source_id)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let mut source_keywords: FxHashMap<FormKey, Vec<FormKey>> = FxHashMap::default();
    for raw_omod in scan.raw_form_ids_of_sig(omod_sig) {
        let Some(source_omod) =
            form_key_from_raw(raw_omod, &source_plugin, &source_masters, mapper.interner)
        else {
            continue;
        };
        let Some(target_omod) = mapper.lookup(source_omod) else {
            continue;
        };
        if !target_omods.contains(&target_omod) {
            continue;
        }
        let keywords = scan
            .with_record_subrecords(raw_omod, |subrecords| {
                subrecords
                    .iter()
                    .filter(|subrecord| subrecord.signature.as_str() == "MNAM")
                    .flat_map(|subrecord| subrecord.data.chunks_exact(4))
                    .filter_map(|chunk| {
                        let raw_keyword = u32::from_le_bytes(chunk.try_into().ok()?);
                        form_key_from_raw(
                            raw_keyword,
                            &source_plugin,
                            &source_masters,
                            mapper.interner,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !keywords.is_empty() {
            source_keywords
                .entry(target_omod)
                .or_default()
                .extend(keywords);
        }
    }
    for keywords in source_keywords.values_mut() {
        let mut seen = FxHashSet::default();
        keywords.retain(|form_key| seen.insert(*form_key));
    }
    Ok(source_keywords)
}

fn is_keyword_target(
    session: &mut PluginSession,
    interner: &crate::sym::StringInterner,
    target_keywords: &FxHashSet<FormKey>,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    cache: &mut FxHashMap<FormKey, bool>,
    target: FormKey,
) -> bool {
    if target_keywords.contains(&target) {
        return true;
    }
    if let Some(cached) = cache.get(&target) {
        return *cached;
    }
    let plugin = interner.resolve(target.plugin).unwrap_or("");
    let is_keyword = target_masters
        .iter()
        .zip(target_master_handle_ids.iter())
        .find(|(master, _)| master.eq_ignore_ascii_case(plugin))
        .and_then(|(master, handle)| {
            session
                .record_signature_in_handle(
                    *handle,
                    &format!("{master}:{:06X}", target.local & 0x00FF_FFFF),
                )
                .ok()
                .flatten()
        })
        .is_some_and(|signature| signature == "KYWD");
    cache.insert(target, is_keyword);
    is_keyword
}

fn form_key_from_raw(
    raw: u32,
    own_plugin: &str,
    masters: &[String],
    interner: &crate::sym::StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let load_index = (raw >> 24) as usize;
    let plugin = if load_index < masters.len() {
        &masters[load_index]
    } else if load_index == masters.len() {
        own_plugin
    } else {
        return None;
    };
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
    })
}

fn merge_target_keywords(record: &mut Record, keywords: &[FormKey]) -> usize {
    let mnam = SubrecordSig(*b"MNAM");
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == mnam) {
        let FieldValue::List(existing) = &mut entry.value else {
            entry.value =
                FieldValue::List(keywords.iter().copied().map(FieldValue::FormKey).collect());
            return keywords.len();
        };
        let mut present: FxHashSet<FormKey> = existing
            .iter()
            .filter_map(|item| match item {
                FieldValue::FormKey(form_key) if form_key.local != 0 => Some(*form_key),
                _ => None,
            })
            .collect();
        let before = existing.len();
        for keyword in keywords {
            if present.insert(*keyword) {
                existing.push(FieldValue::FormKey(*keyword));
            }
        }
        return existing.len() - before;
    }
    set_target_keywords(record, keywords.to_vec());
    keywords.len()
}

fn has_non_null_target_keyword(record: &Record) -> bool {
    record.fields.iter().any(|entry| {
        entry.sig.0 == *b"MNAM"
            && matches!(&entry.value, FieldValue::List(items) if items.iter().any(|item| matches!(item, FieldValue::FormKey(form_key) if form_key.local != 0)))
    })
}

fn omod_needs_dummy_target_keyword(
    record: &Record,
    form_key: FormKey,
    npc_object_template_omods: &FxHashSet<FormKey>,
) -> bool {
    !has_non_null_target_keyword(record) && !npc_object_template_omods.contains(&form_key)
}

fn set_target_keywords(record: &mut Record, keywords: Vec<FormKey>) {
    let value = FieldValue::List(keywords.into_iter().map(FieldValue::FormKey).collect());
    if let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"MNAM")
    {
        entry.value = value;
        return;
    }
    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(&entry.sig.0, b"FNAM" | b"LNAM" | b"NAM1" | b"FLTR"))
        .unwrap_or(record.fields.len());
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: SubrecordSig(*b"MNAM"),
            value,
        },
    );
}

fn ensure_dummy_keyword(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    kywd_sig: SigCode,
    target_keywords: &mut FxHashSet<FormKey>,
) -> Result<(FormKey, bool), FixupError> {
    reserve_existing_target_object_ids(session, mapper);
    let synthetic_source = FormKey {
        local: 1,
        plugin: mapper.interner.intern(DUMMY_KEYWORD_SOURCE_PLUGIN),
    };
    let editor_id = mapper.interner.intern(DUMMY_KEYWORD_EDITOR_ID);
    let target = mapper
        .lookup(synthetic_source)
        .unwrap_or_else(|| mapper.allocate_or_resolve(synthetic_source, Some(editor_id), kywd_sig));
    if target_keywords.contains(&target) {
        return Ok((target, false));
    }

    let mut keyword = Record::new(kywd_sig, target);
    keyword.eid = Some(editor_id);
    keyword.fields.push(FieldEntry {
        sig: SubrecordSig(*b"EDID"),
        value: FieldValue::String(editor_id),
    });
    keyword.fields.push(FieldEntry {
        sig: SubrecordSig(*b"TNAM"),
        value: FieldValue::Uint(KEYWORD_TYPE_MOD_ASSOCIATION),
    });
    session
        .add_record(keyword, target_schema, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    target_keywords.insert(target);
    Ok((target, true))
}

fn reserve_existing_target_object_ids(session: &mut PluginSession, mapper: &mut FormKeyMapper) {
    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let object_ids = ensure_core_section(session.target_slot_mut())
        .by_form_key
        .values()
        .filter(|entry| entry.master_plugin.eq_ignore_ascii_case(&target_plugin))
        .map(|entry| entry.object_id)
        .collect::<Vec<_>>();
    mapper.reserve_object_ids(object_ids);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_new_native,
    };

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn record(
        sig: &str,
        local: u32,
        plugin: &str,
        editor_id: &str,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        let editor_id = interner.intern(editor_id);
        let mut record = Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey {
                local,
                plugin: interner.intern(plugin),
            },
        );
        record.eid = Some(editor_id);
        record
            .fields
            .push(field("EDID", FieldValue::String(editor_id)));
        record.fields.extend(fields);
        record
    }

    fn keyword(local: u32, plugin: &str, editor_id: &str, interner: &StringInterner) -> Record {
        record(
            "KYWD",
            local,
            plugin,
            editor_id,
            vec![field(
                "TNAM",
                FieldValue::Uint(KEYWORD_TYPE_MOD_ASSOCIATION),
            )],
            interner,
        )
    }

    fn omod(
        local: u32,
        plugin: &str,
        editor_id: &str,
        target_keywords: Vec<FormKey>,
        interner: &StringInterner,
    ) -> Record {
        let fields = (!target_keywords.is_empty())
            .then(|| {
                field(
                    "MNAM",
                    FieldValue::List(
                        target_keywords
                            .into_iter()
                            .map(FieldValue::FormKey)
                            .collect(),
                    ),
                )
            })
            .into_iter()
            .collect();
        record("OMOD", local, plugin, editor_id, fields, interner)
    }

    fn seed(handle: u64, records: Vec<Record>, interner: &StringInterner) {
        let mut session = open_session(handle, None).unwrap();
        let schema = session.schema().unwrap();
        session
            .add_records(records, schema.as_ref(), interner)
            .unwrap();
    }

    fn target_keywords(
        session: &mut PluginSession,
        form_key: FormKey,
        interner: &StringInterner,
    ) -> Vec<FormKey> {
        let schema = session.schema().unwrap();
        let record = session
            .record_decoded(&form_key, schema.as_ref(), interner)
            .unwrap();
        record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"MNAM")
            .and_then(|entry| match &entry.value {
                FieldValue::List(values) => Some(
                    values
                        .iter()
                        .filter_map(|value| match value {
                            FieldValue::FormKey(form_key) => Some(*form_key),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn npc_object_template_omod_is_not_given_a_dummy_target_keyword() {
        let interner = StringInterner::new();
        let plugin = "Output.esm";
        let form_key = FormKey {
            local: 0x38CB60,
            plugin: interner.intern(plugin),
        };
        let record = omod(
            form_key.local,
            plugin,
            "Bot_Liberator_BodyArmor",
            vec![],
            &interner,
        );
        let referenced = FxHashSet::from_iter([form_key]);

        assert!(!omod_needs_dummy_target_keyword(
            &record,
            form_key,
            &referenced
        ));
        assert!(omod_needs_dummy_target_keyword(
            &record,
            form_key,
            &FxHashSet::default()
        ));
    }

    #[test]
    fn reads_raw_and_structured_npc_object_template_omod_references() {
        let interner = StringInterner::new();
        let own_plugin = "Output.esm";
        let masters = vec!["DLCNukaWorld.esm".to_string()];
        let target = FormKey {
            local: 0x38CB60,
            plugin: interner.intern(own_plugin),
        };
        let mut raw = vec![0u8; 25];
        raw[0..4].copy_from_slice(&1u32.to_le_bytes());
        raw[18..22].copy_from_slice(&0x0138_CB60_u32.to_le_bytes());
        let mut references = FxHashSet::default();
        collect_obts_omod_references(
            &FieldValue::Bytes(smallvec::SmallVec::from_vec(raw)),
            own_plugin,
            &masters,
            &interner,
            &mut references,
        );
        assert!(references.contains(&target));

        references.clear();
        collect_obts_omod_references(
            &FieldValue::Struct(vec![(
                interner.intern("Includes"),
                FieldValue::List(vec![FieldValue::Struct(vec![(
                    interner.intern("Mod"),
                    FieldValue::FormKey(target),
                )])]),
            )]),
            own_plugin,
            &masters,
            &interner,
            &mut references,
        );
        assert!(references.contains(&target));
    }

    #[test]
    fn restores_dlc_keyword_and_isolates_only_genuinely_unassociated_omods() {
        let interner = StringInterner::new();
        let source_name = "SeventySix.esm";
        let target_name = "Output.esm";
        let dlc_name = "DLCNukaWorld.esm";
        let source = plugin_handle_new_native(source_name, Some("fo76")).unwrap();
        let target = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        let dlc = plugin_handle_new_native(dlc_name, Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, dlc_name, None).unwrap();

        let source_plugin = interner.intern(source_name);
        let target_plugin = interner.intern(target_name);
        let dlc_plugin = interner.intern(dlc_name);
        let source_generic_keyword = FormKey {
            local: 0x37D0B2,
            plugin: source_plugin,
        };
        let source_handmade_keyword = FormKey {
            local: 0x113855,
            plugin: source_plugin,
        };
        let target_generic_keyword = FormKey {
            local: 0x800,
            plugin: target_plugin,
        };
        let target_handmade_keyword = FormKey {
            local: 0x033B61,
            plugin: dlc_plugin,
        };
        let source_partial_omod = FormKey {
            local: 0x5C44E7,
            plugin: source_plugin,
        };
        let source_r91_omod = FormKey {
            local: 0x59560C,
            plugin: source_plugin,
        };
        let source_cut_omod = FormKey {
            local: 0x700001,
            plugin: source_plugin,
        };
        let target_partial_omod = FormKey {
            local: 0x900,
            plugin: target_plugin,
        };
        let target_r91_omod = FormKey {
            local: 0x901,
            plugin: target_plugin,
        };
        let target_cut_omod = FormKey {
            local: 0x902,
            plugin: target_plugin,
        };

        seed(
            dlc,
            vec![keyword(
                target_handmade_keyword.local,
                dlc_name,
                "DLC04_ma_HandmadeAssaultRifle",
                &interner,
            )],
            &interner,
        );
        seed(
            source,
            vec![
                keyword(
                    source_generic_keyword.local,
                    source_name,
                    "TargetOMODKeyword",
                    &interner,
                ),
                keyword(
                    source_handmade_keyword.local,
                    source_name,
                    "DLC04_ma_HandmadeAssaultRifle",
                    &interner,
                ),
                omod(
                    source_partial_omod.local,
                    source_name,
                    "ATX_mod_HandMadeGun_Weapon_ModelSwap_ScreamingEagle",
                    vec![source_generic_keyword, source_handmade_keyword],
                    &interner,
                ),
                omod(
                    source_r91_omod.local,
                    source_name,
                    "ATX_mod_HandMadeGun_Weapon_ModelSwap_R91",
                    vec![source_handmade_keyword],
                    &interner,
                ),
                omod(
                    source_cut_omod.local,
                    source_name,
                    "zzz_CutUnassociatedOMOD",
                    vec![],
                    &interner,
                ),
            ],
            &interner,
        );
        seed(
            target,
            vec![
                keyword(
                    target_generic_keyword.local,
                    target_name,
                    "TargetOMODKeyword",
                    &interner,
                ),
                omod(
                    target_partial_omod.local,
                    target_name,
                    "ATX_mod_HandMadeGun_Weapon_ModelSwap_ScreamingEagle",
                    vec![target_generic_keyword],
                    &interner,
                ),
                omod(
                    target_r91_omod.local,
                    target_name,
                    "ATX_mod_HandMadeGun_Weapon_ModelSwap_R91",
                    vec![],
                    &interner,
                ),
                omod(
                    target_cut_omod.local,
                    target_name,
                    "zzz_CutUnassociatedOMOD",
                    vec![],
                    &interner,
                ),
            ],
            &interner,
        );

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: target_name.into(),
                source_plugin_name: source_name.into(),
                generated_object_id_floor: 0x800,
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(source_generic_keyword, target_generic_keyword);
        mapper.add_mapping(source_handmade_keyword, target_handmade_keyword);
        mapper.add_mapping(source_partial_omod, target_partial_omod);
        mapper.add_mapping(source_r91_omod, target_r91_omod);
        mapper.add_mapping(source_cut_omod, target_cut_omod);

        let mut config = FixupConfig::default();
        config.target_master_handle_ids = vec![dlc];
        let mut session = open_session(target, Some(source)).unwrap();
        let report = RepairOmodTargetKeywordsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();

        assert_eq!(report.records_added, 1);
        assert_eq!(report.records_changed, 3);
        assert_eq!(
            target_keywords(&mut session, target_partial_omod, &interner),
            vec![target_generic_keyword, target_handmade_keyword]
        );
        assert_eq!(
            target_keywords(&mut session, target_r91_omod, &interner),
            vec![target_handmade_keyword]
        );
        let cut_keywords = target_keywords(&mut session, target_cut_omod, &interner);
        assert_eq!(cut_keywords.len(), 1);
        let dummy_keyword = cut_keywords[0];
        assert_eq!(dummy_keyword.plugin, target_plugin);
        assert_eq!(dummy_keyword.local, 0x801);

        let schema = session.schema().unwrap();
        let dummy_record = session
            .record_decoded(&dummy_keyword, schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(
            dummy_record.eid.and_then(|eid| interner.resolve(eid)),
            Some(DUMMY_KEYWORD_EDITOR_ID)
        );
        assert!(dummy_record.fields.iter().any(|entry| {
            entry.sig.0 == *b"TNAM"
                && matches!(entry.value, FieldValue::Uint(KEYWORD_TYPE_MOD_ASSOCIATION))
        }));

        let second_report = RepairOmodTargetKeywordsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(second_report.records_added, 0);
        assert_eq!(second_report.records_changed, 0);
        assert_eq!(
            session
                .form_keys_of_sig(SigCode::from_str("KYWD").unwrap(), &interner)
                .unwrap()
                .len(),
            2
        );
    }
}
