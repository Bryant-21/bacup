//! Bridges the different engine-selected combat music records used by FO76 and FO4.
//! FO76's default type also contains activity tracks whose runtime context does not survive FO4.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

const FO4_COMBAT_MUSIC_LOCAL: u32 = 0x01_ED18;
const FO76_COMBAT_MUSIC_LOCAL: u32 = 0x05_2E1F;
const FO4_MASTER_NAME: &str = "Fallout4.esm";
const FO76_GENERAL_COMBAT_TRACK_LOCALS: &[u32] = &[
    0x05_2E2E, 0x00_E217, 0x11_995B, 0x1D_49BC, 0x1D_49B4, 0x4F_E503, 0x3C_9B94, 0x4F_E504,
    0x11_995C, 0x3D_7C4F, 0x59_BC13,
];

pub struct BridgeFo76CombatMusicFixup;

impl Fixup for BridgeFo76CombatMusicFixup {
    fn name(&self) -> &'static str {
        "bridge_fo76_combat_music"
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
        let schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let musc_sig = SigCode::from_str("MUSC").map_err(FixupError::SchemaError)?;
        let fallout4 = mapper.interner.intern(FO4_MASTER_NAME);
        let output_plugin = mapper.output_plugin_sym();
        let fo76_combat_fk = FormKey {
            local: FO76_COMBAT_MUSIC_LOCAL,
            plugin: output_plugin,
        };
        let fallout4_combat_fk = FormKey {
            local: FO4_COMBAT_MUSIC_LOCAL,
            plugin: fallout4,
        };
        let target_music_types = session
            .form_keys_of_sig(musc_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if !target_music_types.contains(&fo76_combat_fk) {
            return Ok(report);
        }

        let fo76_combat = session
            .record_decoded(&fo76_combat_fk, schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let fo76_tracks: Vec<FormKey> = music_track_form_keys(&fo76_combat)
            .into_iter()
            .filter(|track| {
                track.plugin == output_plugin
                    && FO76_GENERAL_COMBAT_TRACK_LOCALS.contains(&track.local)
            })
            .collect();
        if fo76_tracks.is_empty() {
            return Ok(report);
        }

        let target_masters = session.target_masters().to_vec();
        let Some(fallout4_handle) = target_masters
            .iter()
            .zip(config.target_master_handle_ids.iter().copied())
            .find_map(|(name, handle)| {
                name.eq_ignore_ascii_case(FO4_MASTER_NAME).then_some(handle)
            })
        else {
            return Ok(report);
        };
        let fallout4_combat = session
            .record_decoded_in_handle(
                fallout4_handle,
                &fallout4_combat_fk,
                schema.as_ref(),
                mapper.interner,
            )
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let merged = build_combat_music_override(fallout4_combat, &fo76_tracks);

        if target_music_types.contains(&fallout4_combat_fk) {
            report.records_changed = session
                .replace_records_contents(vec![merged], schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?
                .try_into()
                .unwrap_or(u32::MAX);
        } else {
            session
                .add_record(merged, schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            report.records_added = 1;
        }
        Ok(report)
    }
}

fn build_combat_music_override(mut fallout4_combat: Record, fo76_tracks: &[FormKey]) -> Record {
    let mut seen = FxHashSet::default();
    let tracks: Vec<FieldValue> = music_track_form_keys(&fallout4_combat)
        .into_iter()
        .chain(fo76_tracks.iter().copied())
        .filter(|track| seen.insert(*track))
        .map(FieldValue::FormKey)
        .collect();
    let track_sig = SubrecordSig::from_str("TNAM").expect("TNAM is a valid subrecord signature");
    if let Some(field) = fallout4_combat
        .fields
        .iter_mut()
        .find(|field| field.sig == track_sig)
    {
        field.value = FieldValue::List(tracks);
    } else {
        fallout4_combat.fields.push(FieldEntry {
            sig: track_sig,
            value: FieldValue::List(tracks),
        });
    }
    fallout4_combat
}

fn music_track_form_keys(record: &Record) -> Vec<FormKey> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "TNAM")
        .filter_map(|field| match &field.value {
            FieldValue::List(values) => Some(values),
            _ => None,
        })
        .flatten()
        .filter_map(|value| match value {
            FieldValue::FormKey(form_key) => Some(*form_key),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

    fn music_type(
        form_key: FormKey,
        editor_id: &str,
        tracks: Vec<FormKey>,
        interner: &StringInterner,
    ) -> Record {
        let editor_id = interner.intern(editor_id);
        let mut record = Record::new(SigCode::from_str("MUSC").unwrap(), form_key);
        record.eid = Some(editor_id);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(editor_id),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("WNAM").unwrap(),
            value: FieldValue::Float(20.0),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TNAM").unwrap(),
            value: FieldValue::List(tracks.into_iter().map(FieldValue::FormKey).collect()),
        });
        record
    }

    #[test]
    fn merges_fo76_tracks_into_fallout4_combat_music() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern(FO4_MASTER_NAME);
        let output_plugin = interner.intern("SeventySix.esm");
        let vanilla_track = FormKey {
            local: 0x01_ED17,
            plugin: fallout4,
        };
        let shared_track = FormKey {
            local: 0x03_418B,
            plugin: fallout4,
        };
        let fo76_track = FormKey {
            local: 0x4F_E504,
            plugin: output_plugin,
        };
        let fallout4_combat = music_type(
            FormKey {
                local: FO4_COMBAT_MUSIC_LOCAL,
                plugin: fallout4,
            },
            "MUSzCombat",
            vec![vanilla_track, shared_track],
            &interner,
        );
        let fo76_combat = music_type(
            FormKey {
                local: FO76_COMBAT_MUSIC_LOCAL,
                plugin: output_plugin,
            },
            "MUS76Combat",
            vec![shared_track, fo76_track],
            &interner,
        );

        let output =
            build_combat_music_override(fallout4_combat, &music_track_form_keys(&fo76_combat));

        assert_eq!(output.form_key.local, FO4_COMBAT_MUSIC_LOCAL);
        assert_eq!(
            interner.resolve(output.form_key.plugin),
            Some(FO4_MASTER_NAME)
        );
        assert_eq!(
            output.eid.and_then(|eid| interner.resolve(eid)),
            Some("MUSzCombat")
        );
        assert!(output.fields.iter().any(|entry| {
            entry.sig.as_str() == "WNAM" && entry.value == FieldValue::Float(20.0)
        }));
        assert_eq!(
            music_track_form_keys(&output),
            vec![vanilla_track, shared_track, fo76_track]
        );
    }

    #[test]
    fn writes_only_general_tracks_as_a_fallout4_master_override() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern(FO4_MASTER_NAME);
        let output_plugin = interner.intern("SeventySix.esm");
        let vanilla_track = FormKey {
            local: 0x01_ED17,
            plugin: fallout4,
        };
        let fo76_track = FormKey {
            local: 0x4F_E504,
            plugin: output_plugin,
        };
        let daily_ops_track = FormKey {
            local: 0x61_3991,
            plugin: output_plugin,
        };
        let expedition_track = FormKey {
            local: 0x6E_02C1,
            plugin: output_plugin,
        };
        let source_handle = plugin_handle_new_native("SeventySixSource.esm", Some("fo76")).unwrap();
        let fallout4_handle = plugin_handle_new_native(FO4_MASTER_NAME, Some("fo4")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, FO4_MASTER_NAME, None).unwrap();
        let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();

        {
            let mut session = open_session(fallout4_handle, None).unwrap();
            session
                .add_record(
                    music_type(
                        FormKey {
                            local: FO4_COMBAT_MUSIC_LOCAL,
                            plugin: fallout4,
                        },
                        "MUSzCombat",
                        vec![vanilla_track],
                        &interner,
                    ),
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
        }
        {
            let mut session = open_session(target_handle, None).unwrap();
            session
                .add_record(
                    music_type(
                        FormKey {
                            local: FO76_COMBAT_MUSIC_LOCAL,
                            plugin: output_plugin,
                        },
                        "MUS76Combat",
                        vec![fo76_track, daily_ops_track, expedition_track],
                        &interner,
                    ),
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
        }

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let config = FixupConfig {
            target_master_handle_ids: vec![fallout4_handle],
            ..Default::default()
        };
        let mut session = open_session(target_handle, Some(source_handle)).unwrap();
        let report = BridgeFo76CombatMusicFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();

        assert_eq!(report.records_added, 1);
        let merged = session
            .record_decoded(
                &FormKey {
                    local: FO4_COMBAT_MUSIC_LOCAL,
                    plugin: fallout4,
                },
                schema.as_ref(),
                &interner,
            )
            .unwrap();
        assert_eq!(
            music_track_form_keys(&merged),
            vec![vanilla_track, fo76_track]
        );

        drop(session);
        assert!(plugin_handle_close_native(target_handle));
        assert!(plugin_handle_close_native(fallout4_handle));
        assert!(plugin_handle_close_native(source_handle));
    }
}
