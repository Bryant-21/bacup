use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

pub struct MaterializeLegacyNpcOutfitsFixup;

impl Fixup for MaterializeLegacyNpcOutfitsFixup {
    fn name(&self) -> &'static str {
        "materialize_legacy_npc_outfits"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        matches!(source_game, Some("fnv" | "fo3")) && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let source_schema = session
            .source_schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let source_slot = session
            .source_slot_opt()
            .ok_or_else(|| FixupError::Other("source plugin is required".to_string()))?;
        let source_plugin = source_slot.parsed.plugin_name.clone();
        let source_masters = source_slot.parsed.header.masters.clone();
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let npc_sig = SigCode::from_str("NPC_").expect("NPC_ signature");
        let mut source_npcs = session
            .source_form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        source_npcs.sort_unstable_by_key(|form_key| form_key.local);

        let mut generated_outfits = Vec::new();
        let mut changed_npcs = Vec::new();
        for source_npc_fk in source_npcs {
            let Some(target_npc_fk) = mapper.lookup(source_npc_fk) else {
                continue;
            };
            // A source actor can map onto a vanilla FO4 record instead of one we
            // emit — `FalloutNV.esm:000007` (`Player`) resolves to
            // `Fallout4.esm:000007`. There is nothing to decode in this plugin,
            // and handing the base game's Player a generated outfit would be
            // wrong even if there were.
            if !mapper
                .interner
                .resolve(target_npc_fk.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case(&target_plugin))
            {
                continue;
            }
            let source_npc = session
                .source_record_decoded(&source_npc_fk, source_schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            let mut target_npc = session
                .record_decoded(&target_npc_fk, target_schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            if target_npc.fields.iter().any(|field| {
                field.sig.as_str() == "DOFT"
                    && matches!(&field.value, FieldValue::FormKey(key) if key.local != 0)
            }) {
                continue;
            }

            let mut seen = FxHashSet::default();
            let mut armor = Vec::new();
            for source_item in source_npc
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "CNTO")
                .filter_map(|field| {
                    first_form_key(
                        &field.value,
                        &source_masters,
                        &source_plugin,
                        mapper.interner,
                    )
                })
            {
                let Ok(source_item_record) = session.source_record_decoded(
                    &source_item,
                    source_schema.as_ref(),
                    mapper.interner,
                ) else {
                    continue;
                };
                if source_item_record.sig.as_str() != "ARMO" {
                    continue;
                }
                let Some(target_item) = mapper.lookup(source_item) else {
                    continue;
                };
                if target_signature(session, target_item, config, mapper)? != Some("ARMO".into()) {
                    continue;
                }
                if seen.insert(target_item) {
                    armor.push(target_item);
                }
            }
            if armor.is_empty() {
                continue;
            }

            let outfit_fk = mapper.allocate_generated();
            let output_stem = mapper
                .interner
                .resolve(outfit_fk.plugin)
                .unwrap_or("Output")
                .trim_end_matches(".esm")
                .trim_end_matches(".esp")
                .trim_end_matches(".esl");
            let editor_id = mapper
                .interner
                .intern(&format!("{output_stem}_OTFT_{:06X}", source_npc_fk.local));
            let mut outfit = Record::new(
                SigCode::from_str("OTFT").expect("OTFT signature"),
                outfit_fk,
            );
            outfit.eid = Some(editor_id);
            outfit
                .fields
                .push(field("EDID", FieldValue::String(editor_id))?);
            outfit.fields.push(field(
                "INAM",
                FieldValue::List(armor.into_iter().map(FieldValue::FormKey).collect()),
            )?);

            target_npc
                .fields
                .retain(|field| field.sig.as_str() != "DOFT");
            target_npc
                .fields
                .push(field("DOFT", FieldValue::FormKey(outfit_fk))?);
            generated_outfits.push(outfit);
            changed_npcs.push(target_npc);
        }

        let mut report = FixupReport::empty();
        report.records_added = session
            .add_records(generated_outfits, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        report.records_changed = session
            .replace_records_contents(changed_npcs, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn first_form_key(
    value: &FieldValue,
    masters: &[String],
    plugin_name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => resolve_raw_form_id(
            u32::from_le_bytes(bytes[..4].try_into().ok()?),
            masters,
            plugin_name,
            interner,
        ),
        FieldValue::List(values) => values
            .iter()
            .find_map(|value| first_form_key(value, masters, plugin_name, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| first_form_key(value, masters, plugin_name, interner)),
        _ => None,
    }
}

fn resolve_raw_form_id(
    raw: u32,
    masters: &[String],
    plugin_name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let index = (raw >> 24) as usize;
    let plugin = if index < masters.len() {
        masters.get(index)?.as_str()
    } else if index == masters.len() {
        plugin_name
    } else {
        return None;
    };
    Some(FormKey {
        plugin: interner.intern(plugin),
        local: raw & 0x00FF_FFFF,
    })
}

fn target_signature(
    session: &mut PluginSession,
    form_key: FormKey,
    config: &FixupConfig,
    mapper: &FormKeyMapper,
) -> Result<Option<String>, FixupError> {
    let plugin = mapper
        .interner
        .resolve(form_key.plugin)
        .ok_or_else(|| FixupError::Other("outfit target plugin is not interned".to_string()))?;
    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let target_masters = session.target_masters().to_vec();
    let handle_id = if plugin.eq_ignore_ascii_case(&target_plugin) {
        Some(session.target_id())
    } else {
        target_masters
            .iter()
            .position(|master| master.eq_ignore_ascii_case(plugin))
            .and_then(|index| config.target_master_handle_ids.get(index).copied())
    };
    let Some(handle_id) = handle_id else {
        return Ok(None);
    };
    session
        .record_signature_in_handle(handle_id, &format!("{plugin}:{:06X}", form_key.local))
        .map_err(|error| FixupError::HandleError(error.to_string()))
}

fn field(signature: &str, value: FieldValue) -> Result<FieldEntry, FixupError> {
    Ok(FieldEntry {
        sig: SubrecordSig::from_str(signature)
            .map_err(|error| FixupError::Other(error.to_string()))?,
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use crate::target_write::add_record_native;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_load_no_py, plugin_handle_new_native,
        plugin_handle_read_authoring_record_value_json,
        plugin_handle_save_preserving_identity_no_py,
    };

    #[test]
    fn easy_pete_inventory_becomes_default_outfit_with_hat_and_clothes() {
        let interner = StringInterner::new();
        let source_plugin = "FalloutNV.esm";
        let output_plugin = "FNV_FO3_Output.esm";
        let source_handle = plugin_handle_new_native(source_plugin, Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native(output_plugin, Some("fo4")).unwrap();
        let source_schema = AuthoringSchema::for_game("fnv").unwrap();
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        let source_sym = interner.intern(source_plugin);
        let target_sym = interner.intern(output_plugin);
        let source_npc = FormKey {
            plugin: source_sym,
            local: 0x104C7F,
        };
        let target_npc = FormKey {
            plugin: target_sym,
            local: 0x104C7F,
        };
        let source_hat = FormKey {
            plugin: source_sym,
            local: 0x1083E0,
        };
        let source_clothes = FormKey {
            plugin: source_sym,
            local: 0x0EF1CC,
        };
        let target_hat = FormKey {
            plugin: target_sym,
            local: 0x1083E0,
        };
        let target_clothes = FormKey {
            plugin: target_sym,
            local: 0x0EF1CC,
        };
        let source_dynamite = FormKey {
            plugin: source_sym,
            local: 0x0BA0F3,
        };
        let target_dynamite = FormKey {
            plugin: target_sym,
            local: 0x0BA0F3,
        };

        for (form_key, editor_id) in [
            (source_hat, "CattlemanCowboyHat"),
            (source_clothes, "FieldHandOutfit"),
        ] {
            let mut armor = Record::new(SigCode::from_str("ARMO").unwrap(), form_key);
            armor.eid = Some(interner.intern(editor_id));
            armor
                .fields
                .push(field("EDID", FieldValue::String(armor.eid.unwrap())).unwrap());
            add_record_native(source_handle, armor, &source_schema, &interner).unwrap();
        }
        let mut dynamite = Record::new(SigCode::from_str("WEAP").unwrap(), source_dynamite);
        dynamite.eid = Some(interner.intern("Dynamite"));
        dynamite
            .fields
            .push(field("EDID", FieldValue::String(dynamite.eid.unwrap())).unwrap());
        add_record_native(source_handle, dynamite, &source_schema, &interner).unwrap();
        let mut source_easy_pete = Record::new(SigCode::from_str("NPC_").unwrap(), source_npc);
        source_easy_pete.eid = Some(interner.intern("GSEasyPete"));
        source_easy_pete
            .fields
            .push(field("EDID", FieldValue::String(source_easy_pete.eid.unwrap())).unwrap());
        for (item, count) in [
            (source_dynamite, 2_u32),
            (source_hat, 1_u32),
            (source_clothes, 1_u32),
        ] {
            source_easy_pete.fields.push(
                field(
                    "CNTO",
                    FieldValue::Struct(vec![
                        (interner.intern("Item"), FieldValue::FormKey(item)),
                        (interner.intern("Count"), FieldValue::Uint(u64::from(count))),
                    ]),
                )
                .unwrap(),
            );
        }
        add_record_native(source_handle, source_easy_pete, &source_schema, &interner).unwrap();

        for (form_key, signature, editor_id) in [
            (target_npc, "NPC_", "GSEasyPete"),
            (target_dynamite, "WEAP", "Dynamite"),
            (target_hat, "ARMO", "CattlemanCowboyHat"),
            (target_clothes, "ARMO", "FieldHandOutfit"),
        ] {
            let mut record = Record::new(SigCode::from_str(signature).unwrap(), form_key);
            record.eid = Some(interner.intern(editor_id));
            record
                .fields
                .push(field("EDID", FieldValue::String(record.eid.unwrap())).unwrap());
            add_record_native(target_handle, record, &target_schema, &interner).unwrap();
        }

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: output_plugin.to_string(),
                ..Default::default()
            },
            &interner,
        );
        mapper.add_mapping(source_npc, target_npc);
        mapper.add_mapping(source_dynamite, target_dynamite);
        mapper.add_mapping(source_hat, target_hat);
        mapper.add_mapping(source_clothes, target_clothes);
        let mut session = open_session(target_handle, Some(source_handle)).unwrap();
        let decoded_source = session
            .source_record_decoded(&source_npc, &source_schema, &interner)
            .unwrap();
        let decoded_items = decoded_source
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "CNTO")
            .filter_map(|field| first_form_key(&field.value, &[], source_plugin, &interner))
            .collect::<Vec<_>>();
        assert_eq!(decoded_items, [source_dynamite, source_hat, source_clothes]);
        for (form_key, expected_signature) in [
            (source_dynamite, "WEAP"),
            (source_hat, "ARMO"),
            (source_clothes, "ARMO"),
        ] {
            assert_eq!(
                session
                    .source_record_decoded(&form_key, &source_schema, &interner)
                    .unwrap()
                    .sig
                    .as_str(),
                expected_signature
            );
        }
        assert_eq!(
            target_signature(&mut session, target_hat, &FixupConfig::default(), &mapper).unwrap(),
            Some("ARMO".to_string())
        );
        let report = MaterializeLegacyNpcOutfitsFixup
            .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
            .unwrap();
        session.flush_pending_effects();
        drop(session);

        assert_eq!(report.records_added, 1);
        assert_eq!(report.records_changed, 1);
        let temp = tempfile::tempdir().unwrap();
        let saved = temp.path().join(output_plugin);
        plugin_handle_save_preserving_identity_no_py(target_handle, saved.to_str().unwrap())
            .unwrap();
        let reopened =
            plugin_handle_load_no_py(saved.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let npc = plugin_handle_read_authoring_record_value_json(
            reopened,
            &format!("{output_plugin}:104C7F"),
        )
        .unwrap()
        .unwrap();
        let outfit_reference = npc["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|field| field.get("DefaultOutfit"))
            .and_then(|value| value.get("reference"))
            .unwrap();
        assert_eq!(outfit_reference["plugin"], output_plugin);
        let outfit_local = outfit_reference["object_id"].as_str().unwrap();
        let outfit = plugin_handle_read_authoring_record_value_json(
            reopened,
            &format!("{output_plugin}:{outfit_local}"),
        )
        .unwrap()
        .unwrap();
        let items = outfit["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|field| field.get("Items"))
            .and_then(serde_json::Value::as_array)
            .unwrap()
            .iter()
            .filter_map(|value| value.get("reference"))
            .map(|reference| reference["object_id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(items, ["1083E0", "0EF1CC"]);

        plugin_handle_close_native(reopened);
        plugin_handle_close_native(target_handle);
        plugin_handle_close_native(source_handle);
    }

    /// `FalloutNV.esm:000007` (`Player`) maps to the vanilla `Fallout4.esm:000007`,
    /// not to an emitted record. Decoding it from the output plugin would abort the
    /// whole `fixups_v2` phase with `record not found: Fallout4.esm:000007`.
    #[test]
    fn skips_actors_that_map_onto_a_vanilla_record() {
        let interner = StringInterner::new();
        let source_plugin = "FalloutNV.esm";
        let output_plugin = "FNV_FO3_VanillaMap.esm";
        let source_handle = plugin_handle_new_native(source_plugin, Some("fnv")).unwrap();
        let target_handle = plugin_handle_new_native(output_plugin, Some("fo4")).unwrap();
        let source_schema = AuthoringSchema::for_game("fnv").unwrap();
        let interner_ref = &interner;
        let source_sym = interner_ref.intern(source_plugin);
        let source_player = FormKey {
            plugin: source_sym,
            local: 0x000007,
        };
        let source_armor = FormKey {
            plugin: source_sym,
            local: 0x0EF1CC,
        };
        let vanilla_player = FormKey {
            plugin: interner_ref.intern("Fallout4.esm"),
            local: 0x000007,
        };

        let mut armor = Record::new(SigCode::from_str("ARMO").unwrap(), source_armor);
        armor.eid = Some(interner.intern("FieldHandOutfit"));
        armor
            .fields
            .push(field("EDID", FieldValue::String(armor.eid.unwrap())).unwrap());
        add_record_native(source_handle, armor, &source_schema, &interner).unwrap();

        let mut player = Record::new(SigCode::from_str("NPC_").unwrap(), source_player);
        player.eid = Some(interner.intern("Player"));
        player
            .fields
            .push(field("EDID", FieldValue::String(player.eid.unwrap())).unwrap());
        player.fields.push(
            field(
                "CNTO",
                FieldValue::Struct(vec![
                    (interner.intern("Item"), FieldValue::FormKey(source_armor)),
                    (interner.intern("Count"), FieldValue::Uint(1)),
                ]),
            )
            .unwrap(),
        );
        add_record_native(source_handle, player, &source_schema, &interner).unwrap();

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: output_plugin.to_string(),
                ..Default::default()
            },
            &interner,
        );
        mapper.add_mapping(source_player, vanilla_player);

        let mut session = open_session(target_handle, Some(source_handle)).unwrap();
        let report = MaterializeLegacyNpcOutfitsFixup
            .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
            .expect("a vanilla-mapped actor must not fail the phase");
        drop(session);

        assert_eq!(report.records_added, 0);
        assert_eq!(report.records_changed, 0);

        plugin_handle_close_native(target_handle);
        plugin_handle_close_native(source_handle);
    }
}
