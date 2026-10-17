//! FO76 displays its six per-part power armor attach points as "Paint". Fallout 4 ships the same
//! keywords at the same object ids and EditorIDs (`ap_PowerArmor_HeadMod` and the five body
//! parts) with no display name, so the EditorID remap points every FO76 reference at them and
//! FO76's named copies are never written.
//!
//! The FO4 workbench leaves out an attach point whose keyword has no display name
//! (`ExamineMenu::FindWeaponMods`). FO76 hangs each piece's mesh on these slots, along with every
//! paint and Atomic Shop skin that replaces that mesh, so none of them could be picked. This
//! writes a master override of each keyword carrying FO76's name.

use rustc_hash::FxHashSet;

use crate::fixups::master_record_numbering::record_decoded_in_master_for_target;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, Record};
use crate::session::PluginSession;
use crate::translator::pair_hooks::fo76_fo4::FO76_POWER_ARMOR_PART_ATTACH_POINT_OBJECT_IDS;

const DISPLAY_NAME: SubrecordSig = SubrecordSig(*b"FULL");

pub struct BridgeFo76PowerArmorPaintSlotsFixup;

impl Fixup for BridgeFo76PowerArmorPaintSlotsFixup {
    fn name(&self) -> &'static str {
        "bridge_fo76_power_armor_paint_slots"
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
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let Some(source_plugin) = session
            .source_slot_opt()
            .map(|slot| mapper.interner.intern(&slot.parsed.plugin_name))
        else {
            return Ok(report);
        };
        let output_plugin = mapper.output_plugin_sym();
        let schema = session.schema().map_err(handle_error)?;
        let source_schema = session.source_schema().map_err(handle_error)?;
        let master_handles: Vec<(String, u64)> = session
            .target_masters()
            .iter()
            .cloned()
            .zip(config.target_master_handle_ids.iter().copied())
            .collect();
        let existing_overrides: FxHashSet<FormKey> = session
            .form_keys_of_sig(SigCode(*b"KYWD"), mapper.interner)
            .map_err(handle_error)?
            .into_iter()
            .collect();

        let mut added = Vec::new();
        let mut replaced = Vec::new();
        for local in FO76_POWER_ARMOR_PART_ATTACH_POINT_OBJECT_IDS {
            let source = FormKey {
                local,
                plugin: source_plugin,
            };
            let Some(target) = mapper
                .lookup(source)
                .filter(|target| target.plugin != output_plugin)
            else {
                continue;
            };
            let Ok(fo76_keyword) =
                session.source_record_decoded(&source, &source_schema, mapper.interner)
            else {
                continue;
            };
            let Some(name) = fo76_keyword
                .fields
                .iter()
                .find(|field| field.sig == DISPLAY_NAME)
                .cloned()
            else {
                continue;
            };
            let exists = existing_overrides.contains(&target);
            let keyword = if exists {
                session.record_decoded(&target, &schema, mapper.interner)
            } else {
                let Some(handle) = mapper.interner.resolve(target.plugin).and_then(|plugin| {
                    master_handles
                        .iter()
                        .find(|(master, _)| master.eq_ignore_ascii_case(plugin))
                        .map(|(_, handle)| *handle)
                }) else {
                    continue;
                };
                record_decoded_in_master_for_target(
                    session,
                    handle,
                    &target,
                    &schema,
                    mapper.interner,
                )
            };
            let Ok(mut keyword) = keyword else {
                continue;
            };
            if !add_display_name(&mut keyword, name) {
                continue;
            }
            if exists {
                replaced.push(keyword);
            } else {
                added.push(keyword);
            }
        }

        for keyword in added {
            session
                .add_record(keyword, &schema, mapper.interner)
                .map_err(handle_error)?;
            report.records_added += 1;
        }
        if !replaced.is_empty() {
            report.records_changed = session
                .replace_records_contents(replaced, &schema, mapper.interner)
                .map_err(handle_error)?
                .try_into()
                .unwrap_or(u32::MAX);
        }
        report.message = Some(mapper.interner.intern(&format!(
            "fo76_power_armor_paint_slots:named={}",
            report.records_added + report.records_changed
        )));
        Ok(report)
    }
}

fn add_display_name(keyword: &mut Record, name: FieldEntry) -> bool {
    if keyword.fields.iter().any(|field| field.sig == DISPLAY_NAME) {
        return false;
    }
    keyword.fields.push(name);
    true
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::record::FieldValue;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

    fn keyword(
        form_key: FormKey,
        editor_id: &str,
        name: Option<&str>,
        interner: &StringInterner,
    ) -> Record {
        let editor_id = interner.intern(editor_id);
        let mut record = Record::new(SigCode(*b"KYWD"), form_key);
        record.eid = Some(editor_id);
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(editor_id),
        });
        if let Some(name) = name {
            record.fields.push(FieldEntry {
                sig: DISPLAY_NAME,
                value: FieldValue::String(interner.intern(name)),
            });
        }
        record
    }

    fn display_name(record: &Record, interner: &StringInterner) -> Option<String> {
        record
            .fields
            .iter()
            .find(|field| field.sig == DISPLAY_NAME)
            .and_then(|field| match field.value {
                FieldValue::String(text) => interner.resolve(text).map(str::to_owned),
                _ => None,
            })
    }

    #[test]
    fn names_only_the_remapped_part_roots_as_fallout4_master_overrides() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let source = interner.intern("SeventySixSource.esm");
        let output = interner.intern("SeventySix.esm");
        let fk = |local, plugin| FormKey { local, plugin };
        let head_mod = 0x03_DAFC;
        let body_mod = 0x05_5F8C;
        let paint = 0x17_DAA8;

        let source_handle = plugin_handle_new_native("SeventySixSource.esm", Some("fo76")).unwrap();
        let fallout4_handle = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let fo76_schema = crate::schema::AuthoringSchema::for_game("fo76").unwrap();
        let fo4_schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        {
            let mut session = open_session(source_handle, None).unwrap();
            for (local, editor_id, name) in [
                (head_mod, "ap_PowerArmor_HeadMod", "Paint"),
                (body_mod, "ap_PowerArmor_BodyMod", "Paint"),
                (paint, "ap_PowerArmor_Paint", "Material"),
            ] {
                session
                    .add_record(
                        keyword(fk(local, source), editor_id, Some(name), &interner),
                        fo76_schema.as_ref(),
                        &interner,
                    )
                    .unwrap();
            }
        }
        {
            let mut session = open_session(fallout4_handle, None).unwrap();
            for (local, editor_id) in [
                (head_mod, "ap_PowerArmor_HeadMod"),
                (paint, "ap_PowerArmor_Paint"),
            ] {
                session
                    .add_record(
                        keyword(fk(local, fallout4), editor_id, None, &interner),
                        fo4_schema.as_ref(),
                        &interner,
                    )
                    .unwrap();
            }
        }

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(fk(head_mod, source), fk(head_mod, fallout4));
        mapper.add_mapping(fk(paint, source), fk(paint, fallout4));
        // Written by the output itself, already carrying FO76's name.
        mapper.add_mapping(fk(body_mod, source), fk(body_mod, output));
        let config = FixupConfig {
            target_master_handle_ids: vec![fallout4_handle],
            ..Default::default()
        };
        let mut session = open_session(target_handle, Some(source_handle)).unwrap();
        let report = BridgeFo76PowerArmorPaintSlotsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();

        assert_eq!(report.records_added, 1);
        assert_eq!(
            session
                .form_keys_of_sig(SigCode(*b"KYWD"), &interner)
                .unwrap(),
            vec![fk(head_mod, fallout4)]
        );
        let head = session
            .record_decoded(&fk(head_mod, fallout4), fo4_schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(
            head.eid.and_then(|eid| interner.resolve(eid)),
            Some("ap_PowerArmor_HeadMod")
        );
        assert_eq!(display_name(&head, &interner).as_deref(), Some("Paint"));

        drop(session);
        assert!(plugin_handle_close_native(target_handle));
        assert!(plugin_handle_close_native(fallout4_handle));
        assert!(plugin_handle_close_native(source_handle));
    }

    #[test]
    fn keeps_a_display_name_the_keyword_already_has() {
        let interner = StringInterner::new();
        let head = FormKey {
            local: 0x03_DAFC,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut named = keyword(head, "ap_PowerArmor_HeadMod", Some("Chassis"), &interner);
        let fo76_name = FieldEntry {
            sig: DISPLAY_NAME,
            value: FieldValue::String(interner.intern("Paint")),
        };

        assert!(!add_display_name(&mut named, fo76_name.clone()));
        assert_eq!(display_name(&named, &interner).as_deref(), Some("Chassis"));

        let mut unnamed = keyword(head, "ap_PowerArmor_HeadMod", None, &interner);
        assert!(add_display_name(&mut unnamed, fo76_name));
        assert_eq!(display_name(&unnamed, &interner).as_deref(), Some("Paint"));
    }
}
