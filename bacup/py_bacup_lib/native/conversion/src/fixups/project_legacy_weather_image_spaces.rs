use std::collections::{HashMap, HashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

pub struct ProjectLegacyWeatherImageSpacesFixup;

impl Fixup for ProjectLegacyWeatherImageSpacesFixup {
    fn name(&self) -> &'static str {
        "project_legacy_weather_image_spaces"
    }
    fn uses_session(&self) -> bool {
        true
    }
    fn applies_to_session(&self, session: &PluginSession, _: &FixupConfig) -> bool {
        matches!(
            session
                .source_slot_opt()
                .and_then(|slot| slot.parsed.game.as_deref()),
            Some("fnv" | "fo3")
        ) && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }
    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let source_schema = session
            .source_schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let source_slot = session
            .source_slot_opt()
            .ok_or_else(|| FixupError::Other("missing source".into()))?;
        let source_plugin = source_slot.parsed.plugin_name.clone();
        let source_masters = source_slot.parsed.header.masters.clone();
        let row_size = if source_slot.parsed.game.as_deref() == Some("fo3") {
            8
        } else {
            12
        };
        let worlds = session
            .source_form_keys_of_sig(SigCode(*b"WRLD"), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut weather_images = HashMap::<FormKey, HashSet<FormKey>>::new();
        for key in worlds {
            if mapper.lookup(key).is_none() {
                continue;
            }
            let world = session
                .source_record_decoded(&key, &source_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let (Some(image), Some(climate)) =
                (reference(&world, *b"INAM"), reference(&world, *b"CNAM"))
            else {
                continue;
            };
            let Some(image) = mapper.lookup(image) else {
                continue;
            };
            let climate = session
                .source_record_decoded(&climate, &source_schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for field in climate
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"WLST")
            {
                let FieldValue::Bytes(bytes) = &field.value else {
                    continue;
                };
                for row in bytes.chunks_exact(row_size) {
                    if let Some(weather) = raw_reference(
                        u32::from_le_bytes(row[..4].try_into().unwrap()),
                        &source_plugin,
                        &source_masters,
                        mapper.interner,
                    )
                    .and_then(|weather| mapper.lookup(weather))
                    {
                        weather_images.entry(weather).or_default().insert(image);
                    }
                }
            }
        }
        let plugin = session.target_slot().parsed.plugin_name.clone();
        let masters = session.target_slot().parsed.header.masters.clone();
        let mut changed = Vec::new();
        let mut report = FixupReport::empty();
        for (weather, images) in weather_images {
            if images.len() != 1 {
                report.warnings.push(
                    mapper
                        .interner
                        .intern("legacy_weather_shared_by_different_image_spaces"),
                );
                continue;
            }
            if mapper.interner.resolve(weather.plugin) != Some(plugin.as_str()) {
                continue;
            }
            let image = *images.iter().next().unwrap();
            let image_plugin = mapper.interner.resolve(image.plugin).unwrap_or_default();
            let index = if image_plugin.eq_ignore_ascii_case(&plugin) {
                masters.len()
            } else if let Some(index) = masters
                .iter()
                .position(|master| master.eq_ignore_ascii_case(image_plugin))
            {
                index
            } else {
                continue;
            };
            let image_id = ((index as u32) << 24) | image.local;
            let Ok(mut record) = session.record_decoded(&weather, &target_schema, mapper.interner)
            else {
                continue;
            };
            if set_baseline(&mut record, image_id) {
                changed.push(record);
            }
        }
        report.records_changed = session
            .replace_records_contents(changed, &target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn reference(record: &Record, signature: [u8; 4]) -> Option<FormKey> {
    record.fields.iter().find_map(|field| match field.value {
        FieldValue::FormKey(key) if field.sig.0 == signature => Some(key),
        _ => None,
    })
}

fn raw_reference(
    raw: u32,
    plugin: &str,
    masters: &[String],
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let index = (raw >> 24) as usize;
    let plugin = if index == masters.len() {
        plugin
    } else {
        masters.get(index)?.as_str()
    };
    Some(FormKey {
        plugin: interner.intern(plugin),
        local: raw & 0x00FF_FFFF,
    })
}

fn set_baseline(record: &mut Record, image_id: u32) -> bool {
    if record.fields.iter().any(|field| field.sig.0 == *b"IMSP"
        && !matches!(&field.value, FieldValue::Bytes(bytes) if bytes.iter().all(|byte| *byte == 0))) { return false; }
    // FO4 has no WRLD.INAM. Its weather owns the base image spaces instead.
    // Legacy timed IMAD curves are separate and must not be used as IMGS refs.
    record.fields.retain(|field| field.sig.0 != *b"IMSP");
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"IMSP"),
        value: FieldValue::Bytes(
            std::iter::repeat_n(image_id.to_le_bytes(), 8)
                .flatten()
                .collect(),
        ),
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::target_write::add_record_native;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

    #[test]
    fn worldspace_image_space_reaches_climate_weather_with_target_master_index() {
        for conflict in [false, true] {
            let interner = StringInterner::new();
            let source_plugin = "FalloutNV.esm";
            let output_plugin = "FNV_FO3_Output.esm";
            let source = plugin_handle_new_native(source_plugin, Some("fnv")).unwrap();
            let target = plugin_handle_new_native(output_plugin, Some("fo4")).unwrap();
            plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
            let source_schema = AuthoringSchema::for_game("fnv").unwrap();
            let target_schema = AuthoringSchema::for_game("fo4").unwrap();
            let source_key = |local| FormKey {
                local,
                plugin: interner.intern(source_plugin),
            };
            let target_key = |local| FormKey {
                local,
                plugin: interner.intern(output_plugin),
            };
            let field = |sig, value| FieldEntry {
                sig: SubrecordSig(sig),
                value,
            };
            let mut mapper = FormKeyMapper::new(
                [],
                MapperOptions {
                    output_plugin_name: output_plugin.into(),
                    ..Default::default()
                },
                &interner,
            );
            for (world, image) in [
                (0x800, 0x803),
                (0x804, if conflict { 0x805 } else { 0x803 }),
            ] {
                let mut record = Record::new(SigCode(*b"WRLD"), source_key(world));
                record.fields.extend([
                    field(*b"CNAM", FieldValue::FormKey(source_key(0x801))),
                    field(*b"INAM", FieldValue::FormKey(source_key(image))),
                ]);
                add_record_native(source, record, &source_schema, &interner).unwrap();
                mapper.add_mapping(source_key(world), target_key(world));
                mapper.add_mapping(source_key(image), target_key(image));
            }
            let mut climate = Record::new(SigCode(*b"CLMT"), source_key(0x801));
            climate.fields.push(field(
                *b"WLST",
                FieldValue::Bytes(
                    [
                        0x802_u32.to_le_bytes(),
                        100_u32.to_le_bytes(),
                        0_u32.to_le_bytes(),
                    ]
                    .into_iter()
                    .flatten()
                    .collect(),
                ),
            ));
            add_record_native(source, climate, &source_schema, &interner).unwrap();
            add_record_native(
                target,
                Record::new(SigCode(*b"WTHR"), target_key(0x802)),
                &target_schema,
                &interner,
            )
            .unwrap();
            mapper.add_mapping(source_key(0x802), target_key(0x802));
            let mut session = open_session(target, Some(source)).unwrap();
            let report = ProjectLegacyWeatherImageSpacesFixup
                .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
                .unwrap();
            assert_eq!(report.records_changed, if conflict { 0 } else { 1 });
            assert_eq!(report.warnings.len(), usize::from(conflict));
            if !conflict {
                let weather = session
                    .record_decoded(&target_key(0x802), &target_schema, &interner)
                    .unwrap();
                let FieldValue::Bytes(bytes) = &weather
                    .fields
                    .iter()
                    .find(|field| field.sig.0 == *b"IMSP")
                    .unwrap()
                    .value
                else {
                    panic!("IMSP");
                };
                assert_eq!(bytes.len(), 32);
                assert!(
                    bytes
                        .chunks_exact(4)
                        .all(|row| row == 0x0100_0803_u32.to_le_bytes())
                );
            }
            session.flush_pending_effects();
            drop(session);
            assert!(plugin_handle_close_native(source));
            assert!(plugin_handle_close_native(target));
        }
    }
    #[test]
    fn weather_baseline_fills_all_periods_and_preserves_existing_assignments() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode(*b"WTHR"),
            FormKey {
                plugin: interner.intern("Output.esm"),
                local: 0x800,
            },
        );
        assert!(set_baseline(&mut record, 0x0108_809D));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("IMSP");
        };
        assert_eq!(bytes.len(), 32);
        assert!(
            bytes
                .chunks_exact(4)
                .all(|row| row == 0x0108_809D_u32.to_le_bytes())
        );
        assert!(!set_baseline(&mut record, 0x1234));
    }
}
