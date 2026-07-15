use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::source_read::form_key_to_read_str;
use crate::sym::StringInterner;
use rustc_hash::FxHashMap;

const SYNTH_TXST_PLUGIN: &str = "__synth_ltex_txst__";

pub struct LtexTxstSynthFixup;

impl Fixup for LtexTxstSynthFixup {
    fn name(&self) -> &'static str {
        "ltex_txst_synth"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        ctx.source_handle_id != 0
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
        let target_schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let source_schema = session
            .source_schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let ltex_sig = SigCode::from_str("LTEX").map_err(FixupError::SchemaError)?;
        let txst_sig = SigCode::from_str("TXST").map_err(FixupError::SchemaError)?;
        let tnam_sig = SubrecordSig::from_str("TNAM").map_err(FixupError::SchemaError)?;

        let target_ltex_fks = session
            .form_keys_of_sig(ltex_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if target_ltex_fks.is_empty() {
            return Ok(FixupReport::empty());
        }

        let mut source_ltex_by_target = FxHashMap::default();
        for source_fk in session
            .source_form_keys_of_sig(ltex_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
        {
            if let Some(target_fk) = mapper.lookup(source_fk) {
                source_ltex_by_target.insert(target_fk, source_fk);
            }
        }

        let mut txst_by_texture = collect_target_txsts_by_texture(
            session,
            target_schema.as_ref(),
            mapper.interner,
            txst_sig,
        )?;
        let mut added_txsts = Vec::new();
        let mut changed_ltex = Vec::new();
        let mut report = FixupReport::empty();

        for target_ltex_fk in target_ltex_fks {
            let mut target_ltex = session
                .record_decoded(&target_ltex_fk, target_schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            if let Some(target_txst_fk) = formkey_field(&target_ltex, tnam_sig) {
                if target_record_signature(session, target_txst_fk, config, mapper.interner)?
                    .as_deref()
                    == Some("TXST")
                {
                    continue;
                }
            }

            let Some(source_ltex_fk) = source_ltex_by_target.get(&target_ltex_fk).copied() else {
                warn(
                    &mut report,
                    mapper.interner,
                    format!(
                        "ltex_txst_synth:source_ltex_missing:{:06X}",
                        target_ltex_fk.local
                    ),
                );
                continue;
            };
            let Some(texture_source) = source_texture(
                session,
                source_ltex_fk,
                source_schema.as_ref(),
                mapper.interner,
            )?
            else {
                warn(
                    &mut report,
                    mapper.interner,
                    format!(
                        "ltex_txst_synth:source_texture_missing:{:06X}",
                        source_ltex_fk.local
                    ),
                );
                continue;
            };
            let Some(texture_path) = normalize_texture_path(&texture_source.path) else {
                warn(
                    &mut report,
                    mapper.interner,
                    format!(
                        "ltex_txst_synth:source_texture_empty:{:06X}",
                        source_ltex_fk.local
                    ),
                );
                continue;
            };
            let texture_key = texture_path.to_ascii_lowercase();

            let target_txst_fk = if let Some(existing) = txst_by_texture.get(&texture_key).copied()
            {
                mapper.add_mapping(texture_source.allocation_key, existing);
                existing
            } else if let Some(mapped) = mapper.lookup(texture_source.allocation_key) {
                match target_record_signature(session, mapped, config, mapper.interner)?.as_deref()
                {
                    Some("TXST") => mapped,
                    None => {
                        added_txsts.push(build_txst(mapped, &texture_path, mapper.interner)?);
                        mapped
                    }
                    Some(_) => allocate_synthesized_txst(
                        texture_source.allocation_key,
                        &texture_path,
                        txst_sig,
                        mapper,
                        &mut added_txsts,
                    )?,
                }
            } else {
                allocate_synthesized_txst(
                    texture_source.allocation_key,
                    &texture_path,
                    txst_sig,
                    mapper,
                    &mut added_txsts,
                )?
            };

            mapper.add_mapping(texture_source.allocation_key, target_txst_fk);
            txst_by_texture.insert(texture_key, target_txst_fk);
            set_formkey_field(&mut target_ltex, tnam_sig, target_txst_fk);
            changed_ltex.push(target_ltex);
        }

        report.records_added = session
            .add_records(added_txsts, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        report.records_changed = session
            .replace_records_contents(changed_ltex, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        Ok(report)
    }
}

struct TextureSource {
    path: String,
    allocation_key: FormKey,
}

fn source_texture(
    session: &mut PluginSession,
    source_ltex_fk: FormKey,
    source_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> Result<Option<TextureSource>, FixupError> {
    let source_ltex = session
        .source_record_decoded(&source_ltex_fk, source_schema, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let tnam_sig = SubrecordSig::from_str("TNAM").map_err(FixupError::SchemaError)?;
    if let Some(source_txst_fk) = formkey_field(&source_ltex, tnam_sig) {
        if let Ok(source_txst) =
            session.source_record_decoded(&source_txst_fk, source_schema, interner)
        {
            let tx00_sig = SubrecordSig::from_str("TX00").map_err(FixupError::SchemaError)?;
            if let Some(path) = string_field(&source_txst, tx00_sig, interner) {
                return Ok(Some(TextureSource {
                    path,
                    allocation_key: source_txst_fk,
                }));
            }
        }
    }

    let icon_sig = SubrecordSig::from_str("ICON").map_err(FixupError::SchemaError)?;
    Ok(
        string_field(&source_ltex, icon_sig, interner).map(|path| TextureSource {
            path,
            allocation_key: FormKey {
                local: source_ltex_fk.local,
                plugin: interner.intern(SYNTH_TXST_PLUGIN),
            },
        }),
    )
}

fn collect_target_txsts_by_texture(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
    txst_sig: SigCode,
) -> Result<FxHashMap<String, FormKey>, FixupError> {
    let tx00_sig = SubrecordSig::from_str("TX00").map_err(FixupError::SchemaError)?;
    let mut by_texture = FxHashMap::default();
    for fk in session
        .form_keys_of_sig(txst_sig, interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?
    {
        let record = session
            .record_decoded(&fk, target_schema, interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if let Some(path) =
            string_field(&record, tx00_sig, interner).and_then(|path| normalize_texture_path(&path))
        {
            by_texture.entry(path.to_ascii_lowercase()).or_insert(fk);
        }
    }
    Ok(by_texture)
}

fn target_record_signature(
    session: &mut PluginSession,
    fk: FormKey,
    config: &FixupConfig,
    interner: &StringInterner,
) -> Result<Option<String>, FixupError> {
    let key = form_key_to_read_str(&fk, interner);
    for handle_id in
        std::iter::once(session.target_id()).chain(config.target_master_handle_ids.iter().copied())
    {
        if let Some(signature) = session
            .record_signature_in_handle(handle_id, &key)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
        {
            return Ok(Some(signature));
        }
    }
    Ok(None)
}

fn allocate_synthesized_txst(
    source_key: FormKey,
    texture_path: &str,
    txst_sig: SigCode,
    mapper: &mut FormKeyMapper,
    added_txsts: &mut Vec<Record>,
) -> Result<FormKey, FixupError> {
    let allocation_key = if mapper.lookup(source_key).is_some() {
        FormKey {
            local: source_key.local,
            plugin: mapper.interner.intern(SYNTH_TXST_PLUGIN),
        }
    } else {
        source_key
    };
    let target_fk = mapper.allocate_or_resolve(allocation_key, None, txst_sig);
    added_txsts.push(build_txst(target_fk, texture_path, mapper.interner)?);
    Ok(target_fk)
}

fn build_txst(
    form_key: FormKey,
    texture_path: &str,
    interner: &StringInterner,
) -> Result<Record, FixupError> {
    let mut record = Record::new(
        SigCode::from_str("TXST").map_err(FixupError::SchemaError)?,
        form_key,
    );
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("OBND").map_err(FixupError::SchemaError)?,
        value: FieldValue::Struct(
            [
                "object_bounds_x1",
                "object_bounds_y1",
                "object_bounds_z1",
                "object_bounds_x2",
                "object_bounds_y2",
                "object_bounds_z2",
            ]
            .into_iter()
            .map(|name| (interner.intern(name), FieldValue::Int(0)))
            .collect(),
        ),
    });
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("TX00").map_err(FixupError::SchemaError)?,
        value: FieldValue::String(interner.intern(texture_path)),
    });
    Ok(record)
}

fn formkey_field(record: &Record, sig: SubrecordSig) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig == sig)
        .and_then(|entry| match entry.value {
            FieldValue::FormKey(fk) if fk.local != 0 => Some(fk),
            _ => None,
        })
}

fn string_field(record: &Record, sig: SubrecordSig, interner: &StringInterner) -> Option<String> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig == sig)
        .and_then(|entry| match entry.value {
            FieldValue::String(sym) => interner.resolve(sym).map(str::to_owned),
            _ => None,
        })
}

fn set_formkey_field(record: &mut Record, sig: SubrecordSig, fk: FormKey) {
    record.fields.retain(|entry| entry.sig != sig);
    let insert_at = record
        .fields
        .iter()
        .rposition(|entry| entry.sig.as_str() == "EDID")
        .map_or(0, |index| index + 1);
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig,
            value: FieldValue::FormKey(fk),
        },
    );
}

fn normalize_texture_path(path: &str) -> Option<String> {
    let normalized = path.trim().replace('/', "\\");
    let normalized = normalized.trim_start_matches('\\');
    let relative = if normalized
        .get(..9)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("Textures\\"))
    {
        &normalized[9..]
    } else {
        normalized
    };
    if relative.is_empty() {
        return None;
    }
    Some(relative.to_string())
}

fn warn(report: &mut FixupReport, interner: &StringInterner, message: String) {
    report.warnings.push(interner.intern(&message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

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
        eid: &str,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        let eid = interner.intern(eid);
        let mut record = Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey {
                local,
                plugin: interner.intern(plugin),
            },
        );
        record.eid = Some(eid);
        record.fields.push(field("EDID", FieldValue::String(eid)));
        record.fields.extend(fields);
        record
    }

    fn seed(handle: u64, records: Vec<Record>, interner: &StringInterner) {
        let mut session = open_session(handle, None).unwrap();
        let schema = session.schema().unwrap();
        session
            .add_records(records, schema.as_ref(), interner)
            .unwrap();
    }

    #[test]
    fn synthesizes_one_txst_per_texture_path_and_wires_ltex_tnam() {
        let interner = StringInterner::new();
        let source_name = "FNV_FO3_Merged.esm";
        let target_name = "MojaveCapital.esm";
        let source = plugin_handle_new_native(source_name, Some("fnv")).unwrap();
        let target = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        let source_plugin = interner.intern(source_name);
        let target_plugin = interner.intern(target_name);
        let source_txst_a = FormKey {
            local: 0x900,
            plugin: source_plugin,
        };
        let source_txst_b = FormKey {
            local: 0x901,
            plugin: source_plugin,
        };
        let source_ltex_a = FormKey {
            local: 0xA00,
            plugin: source_plugin,
        };
        let source_ltex_b = FormKey {
            local: 0xA01,
            plugin: source_plugin,
        };
        let target_ltex_a = FormKey {
            local: 0xB00,
            plugin: target_plugin,
        };
        let target_ltex_b = FormKey {
            local: 0xB01,
            plugin: target_plugin,
        };
        let texture_a = interner.intern("Landscape\\Dirt01.dds");
        let texture_b = interner.intern("textures/landscape/dirt01.dds");

        seed(
            source,
            vec![
                record(
                    "TXST",
                    source_txst_a.local,
                    source_name,
                    "LandscapeDirt01A",
                    vec![field("TX00", FieldValue::String(texture_a))],
                    &interner,
                ),
                record(
                    "TXST",
                    source_txst_b.local,
                    source_name,
                    "LandscapeDirt01B",
                    vec![field("TX00", FieldValue::String(texture_b))],
                    &interner,
                ),
                record(
                    "LTEX",
                    source_ltex_a.local,
                    source_name,
                    "Dirt01A",
                    vec![field("TNAM", FieldValue::FormKey(source_txst_a))],
                    &interner,
                ),
                record(
                    "LTEX",
                    source_ltex_b.local,
                    source_name,
                    "Dirt01B",
                    vec![field("TNAM", FieldValue::FormKey(source_txst_b))],
                    &interner,
                ),
            ],
            &interner,
        );
        seed(
            target,
            vec![
                record(
                    "LTEX",
                    target_ltex_a.local,
                    target_name,
                    "Dirt01A",
                    vec![],
                    &interner,
                ),
                record(
                    "LTEX",
                    target_ltex_b.local,
                    target_name,
                    "Dirt01B",
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
        mapper.add_mapping(source_ltex_a, target_ltex_a);
        mapper.add_mapping(source_ltex_b, target_ltex_b);
        let mut session = open_session(target, Some(source)).unwrap();
        let report = LtexTxstSynthFixup
            .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
            .unwrap();
        drop(session);

        assert_eq!(report.records_added, 1);
        assert_eq!(report.records_changed, 2);

        let mut session = open_session(target, None).unwrap();
        let schema = session.schema().unwrap();
        let txst_sig = SigCode::from_str("TXST").unwrap();
        let txst_fks = session.form_keys_of_sig(txst_sig, &interner).unwrap();
        assert_eq!(txst_fks.len(), 1);
        let txst = session
            .record_decoded(&txst_fks[0], schema.as_ref(), &interner)
            .unwrap();
        let tx00 = SubrecordSig::from_str("TX00").unwrap();
        let diffuse = txst
            .fields
            .iter()
            .find(|entry| entry.sig == tx00)
            .and_then(|entry| match entry.value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            });
        assert_eq!(diffuse, Some("Landscape\\Dirt01.dds"));

        let tnam = SubrecordSig::from_str("TNAM").unwrap();
        for ltex_fk in [target_ltex_a, target_ltex_b] {
            let ltex = session
                .record_decoded(&ltex_fk, schema.as_ref(), &interner)
                .unwrap();
            let linked = ltex
                .fields
                .iter()
                .find(|entry| entry.sig == tnam)
                .and_then(|entry| match entry.value {
                    FieldValue::FormKey(fk) => Some(fk),
                    _ => None,
                });
            assert_eq!(linked, Some(txst_fks[0]));
        }
    }

    #[test]
    fn texture_paths_are_relative_to_textures_root() {
        assert_eq!(
            normalize_texture_path(" Landscape/Dirt01.dds ").as_deref(),
            Some("Landscape\\Dirt01.dds")
        );
        assert_eq!(
            normalize_texture_path("Textures\\Landscape\\Dirt01.dds").as_deref(),
            Some("Landscape\\Dirt01.dds")
        );
        assert_eq!(
            normalize_texture_path("textures/landscape/dirt01.dds").as_deref(),
            Some("landscape\\dirt01.dds")
        );
        assert_eq!(normalize_texture_path("Textures\\"), None);
    }

    #[test]
    fn unavailable_source_txst_uses_icon_or_warns_and_skips() {
        let interner = StringInterner::new();
        let source_name = "FNV_FO3_Merged.esm";
        let target_name = "MojaveCapital.esm";
        let source = plugin_handle_new_native(source_name, Some("fnv")).unwrap();
        let target = plugin_handle_new_native(target_name, Some("fo4")).unwrap();
        let source_plugin = interner.intern(source_name);
        let target_plugin = interner.intern(target_name);
        let source_ltex_with_icon = FormKey {
            local: 0xA10,
            plugin: source_plugin,
        };
        let source_ltex_without_icon = FormKey {
            local: 0xA11,
            plugin: source_plugin,
        };
        let target_ltex_with_icon = FormKey {
            local: 0xB10,
            plugin: target_plugin,
        };
        let target_ltex_without_icon = FormKey {
            local: 0xB11,
            plugin: target_plugin,
        };
        let unavailable_txst_with_icon = FormKey {
            local: 0x910,
            plugin: source_plugin,
        };
        let unavailable_txst_without_icon = FormKey {
            local: 0x911,
            plugin: source_plugin,
        };

        seed(
            source,
            vec![
                record(
                    "LTEX",
                    source_ltex_with_icon.local,
                    source_name,
                    "DirtWithIcon",
                    vec![
                        field("TNAM", FieldValue::FormKey(unavailable_txst_with_icon)),
                        field(
                            "ICON",
                            FieldValue::String(interner.intern("Textures/Landscape/Fallback.dds")),
                        ),
                    ],
                    &interner,
                ),
                record(
                    "LTEX",
                    source_ltex_without_icon.local,
                    source_name,
                    "DirtWithoutIcon",
                    vec![field(
                        "TNAM",
                        FieldValue::FormKey(unavailable_txst_without_icon),
                    )],
                    &interner,
                ),
            ],
            &interner,
        );
        seed(
            target,
            vec![
                record(
                    "LTEX",
                    target_ltex_with_icon.local,
                    target_name,
                    "DirtWithIcon",
                    vec![],
                    &interner,
                ),
                record(
                    "LTEX",
                    target_ltex_without_icon.local,
                    target_name,
                    "DirtWithoutIcon",
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
        mapper.add_mapping(source_ltex_with_icon, target_ltex_with_icon);
        mapper.add_mapping(source_ltex_without_icon, target_ltex_without_icon);
        let mut session = open_session(target, Some(source)).unwrap();
        let report = LtexTxstSynthFixup
            .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
            .unwrap();
        drop(session);

        assert_eq!(report.records_added, 1);
        assert_eq!(report.records_changed, 1);
        assert_eq!(report.warnings.len(), 1);
        assert_eq!(
            interner.resolve(report.warnings[0]),
            Some("ltex_txst_synth:source_texture_missing:000A11")
        );

        let mut session = open_session(target, None).unwrap();
        let schema = session.schema().unwrap();
        let txst_fks = session
            .form_keys_of_sig(SigCode::from_str("TXST").unwrap(), &interner)
            .unwrap();
        assert_eq!(txst_fks.len(), 1);
        let txst = session
            .record_decoded(&txst_fks[0], schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(
            string_field(&txst, SubrecordSig::from_str("TX00").unwrap(), &interner).as_deref(),
            Some("Landscape\\Fallback.dds")
        );

        let linked_ltex = session
            .record_decoded(&target_ltex_with_icon, schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(
            formkey_field(&linked_ltex, SubrecordSig::from_str("TNAM").unwrap()),
            Some(txst_fks[0])
        );
        let skipped_ltex = session
            .record_decoded(&target_ltex_without_icon, schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(
            formkey_field(&skipped_ltex, SubrecordSig::from_str("TNAM").unwrap()),
            None
        );
    }
}
