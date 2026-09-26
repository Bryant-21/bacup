use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Component, Path};

use esp_authoring_core::plugin_runtime::{
    ParsedSubrecord, compiled_schema_for_game_str, effective_subrecords_for_record,
    schema_record_spec, schema_subrecord_spec,
};
use serde_json::{Value, json};

use crate::fixups::rewrite_raw_object_template_formids::fo76_condition_references;
use crate::ids::{FormKey, SigCode};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::session::open_session;
use crate::translator::Game;

pub struct EquipmentConditionPhase;

const TYPES: &[&str] = &[
    "WEAP", "ARMO", "OMOD", "PERK", "COBJ", "UTIL", "CURV", "ENCH", "MGEF", "SPEL", "KYWD", "FLST",
    "CNDF", "GLOB", "AVIF",
];

fn editor_id(fields: &[ParsedSubrecord]) -> String {
    fields
        .iter()
        .find(|f| f.signature == "EDID")
        .map(|f| {
            String::from_utf8_lossy(&f.data)
                .trim_end_matches('\0')
                .to_owned()
        })
        .unwrap_or_default()
}

fn is_root(signature: &str, fields: &[ParsedSubrecord]) -> bool {
    match signature {
        "WEAP" | "ARMO" | "OMOD" => true,
        "COBJ" => fields.iter().any(|f| {
            matches!(f.signature.as_str(), "RCND" | "REPR") && f.data.iter().any(|b| *b != 0)
        }),
        "UTIL" => matches!(
            editor_id(fields).as_str(),
            "ATX_Utility_RepairKit_Basic" | "Utility_RepairKit_Improved"
        ),
        "PERK" => {
            let mut entry_point = false;
            fields.iter().any(|f| {
                if f.signature == "PRKE" {
                    entry_point = f.data.first() == Some(&2);
                }
                if f.signature == "PRKF" {
                    entry_point = false;
                }
                entry_point
                    && f.signature == "DATA"
                    && f.data.len() == 4
                    && matches!(f.data[0], 163 | 166 | 176)
            })
        }
        _ => false,
    }
}

fn keep_field(signature: &str, field: &str) -> bool {
    if matches!(field, "EDID" | "KWDA" | "KSIZ") {
        return true;
    }
    match signature {
        "WEAP" | "ARMO" => matches!(
            field,
            "DATA"
                | "DNAM"
                | "CVT0"
                | "CVT1"
                | "CVT2"
                | "CVT3"
                | "CVT4"
                | "DAMA"
                | "EITM"
                | "ETYP"
                | "BOD2"
                | "OBTS"
                | "EILV"
                | "IBSD"
        ),
        "OMOD" => field == "DATA",
        "PERK" => matches!(
            field,
            "DATA"
                | "PRKE"
                | "PRKC"
                | "CTDA"
                | "CIS1"
                | "CIS2"
                | "EPFT"
                | "EPFB"
                | "EPFD"
                | "EPF2"
                | "EPF3"
                | "EPF4"
                | "PRKF"
                | "NNAM"
        ),
        "COBJ" => matches!(
            field,
            "FVPA" | "REPR" | "REPM" | "CNAM" | "BNAM" | "FNAM" | "DNAM" | "INTV" | "RCND" | "CTDA"
        ),
        "CURV" => matches!(field, "CRVE" | "JASF"),
        "MGEF" => matches!(field, "DATA" | "CTDA"),
        "ENCH" | "SPEL" => matches!(field, "ENIT" | "SPIT" | "EFID" | "EFIT" | "CTDA"),
        "FLST" => field == "LNAM",
        "CNDF" => field == "CTDA",
        "GLOB" => matches!(field, "FNAM" | "FLTV"),
        "UTIL" => matches!(field, "DATA" | "UITE" | "UITO" | "UITV" | "UIFL" | "UIUS"),
        _ => false,
    }
}

fn source_key(raw: u32, masters: &[String], plugin: &str) -> Result<String, PhaseError> {
    let index = (raw >> 24) as usize;
    let owner = if index == masters.len() || index == 0xff {
        plugin
    } else {
        masters.get(index).map(String::as_str).ok_or_else(|| {
            PhaseError::Internal(format!("invalid condition catalog FormID {raw:08X}"))
        })?
    };
    Ok(format!("{:06X}:{owner}", raw & 0xffffff))
}

fn curve_path(root: &Path, relative: &str) -> Result<std::path::PathBuf, PhaseError> {
    let normalized = relative.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || normalized.contains(':')
    {
        return Err(PhaseError::Internal(format!(
            "invalid condition curve path {relative:?}"
        )));
    }
    Ok(root.join("misc/curvetables/json").join(path))
}

impl Phase for EquipmentConditionPhase {
    fn name(&self) -> &'static str {
        "emit_equipment_condition"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        if ctx.mod_path.as_os_str().is_empty() {
            return Err(PhaseError::BadParams(
                "condition catalog requires mod_path".into(),
            ));
        }
        let schema = compiled_schema_for_game_str("fo76").map_err(PhaseError::Internal)?;
        let mut session = open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let (masters, plugin) = session
            .handle_load_order(ctx.run.source_handle_id)
            .map(|(masters, plugin)| (masters.to_vec(), plugin.to_string()))
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let scan = session
            .handle_raw_scan(ctx.run.source_handle_id)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let mut candidates = BTreeMap::new();
        let mut pending = BTreeSet::new();
        for signature in TYPES {
            for raw in scan.raw_form_ids_of_sig(SigCode::from_str(signature).unwrap()) {
                ctx.check_cancel()?;
                let root = scan
                    .with_record(raw, |record| {
                        if record.flags & 0x20 != 0 {
                            return None;
                        }
                        Some(is_root(signature, &effective_subrecords_for_record(record)))
                    })
                    .flatten();
                if let Some(root) = root {
                    candidates.insert(raw, *signature);
                    if root {
                        pending.insert(raw);
                    }
                }
            }
        }
        let source_root = ctx
            .run
            .config
            .source_extracted_dir
            .as_deref()
            .unwrap_or(ctx.source_extracted_dir);
        let mut records = BTreeMap::new();
        let mut references = BTreeSet::new();
        let mut missing_curves = Vec::new();
        while let Some(raw) = pending.pop_first() {
            if records.contains_key(&raw) {
                continue;
            }
            ctx.check_cancel()?;
            let signature = candidates[&raw];
            let (fields, eid) = scan
                .with_record(raw, |record| {
                    if let Some(error) = &record.parse_error {
                        return Err(PhaseError::Internal(format!(
                            "condition record {raw:08X}: {error}"
                        )));
                    }
                    let all = effective_subrecords_for_record(record);
                    Ok((
                        all.iter()
                            .filter(|f| keep_field(signature, &f.signature))
                            .cloned()
                            .collect::<Vec<_>>(),
                        editor_id(&all),
                    ))
                })
                .ok_or_else(|| {
                    PhaseError::Internal(format!("missing condition record {raw:08X}"))
                })??;
            let spec = schema_record_spec(&schema, signature).ok_or_else(|| {
                PhaseError::Internal(format!("missing condition schema {signature}"))
            })?;
            let mut occurrences = BTreeMap::new();
            let mut links = Vec::new();
            let mut perk_data_type = None;
            for field in &fields {
                let occurrence = occurrences.entry(field.signature.clone()).or_insert(0);
                if signature == "PERK" && field.signature == "PRKE" {
                    perk_data_type = None;
                }
                if signature == "PERK" && field.signature == "EPFT" {
                    perk_data_type = field.data.first().copied();
                }
                if signature == "PERK" && field.signature == "EPF3" && perk_data_type == Some(8) {
                    if field.data.len() != 4 {
                        return Err(PhaseError::Internal(format!(
                            "invalid perk actor value at {raw:08X}"
                        )));
                    }
                    links.push(u32::from_le_bytes(field.data.as_ref().try_into().unwrap()));
                } else if signature == "COBJ" && matches!(field.signature.as_str(), "REPR" | "FVPA")
                {
                    if field.data.len() % 12 != 0 {
                        return Err(PhaseError::Internal(format!(
                            "invalid repair component table at {raw:08X}"
                        )));
                    }
                    for component in field.data.chunks_exact(12) {
                        links.push(u32::from_le_bytes(component[0..4].try_into().unwrap()));
                        links.push(u32::from_le_bytes(component[8..12].try_into().unwrap()));
                    }
                } else if field.signature == "CTDA" {
                    if field.data.len() != 32 {
                        return Err(PhaseError::Internal(format!(
                            "invalid condition size at {raw:08X}"
                        )));
                    }
                    links.extend(fo76_condition_references(&field.data));
                } else if let Some(field_spec) =
                    schema_subrecord_spec(spec, &field.signature, *occurrence)
                {
                    esp_authoring_core::plugin_runtime::authoring::authoring_serialize::extract_nested_form_ids(
                        field_spec, &schema, &field.data, &mut links);
                }
                *occurrence += 1;
            }
            for link in links.into_iter().filter(|v| *v != 0) {
                source_key(link, &masters, &plugin)?;
                references.insert(link);
                if candidates.contains_key(&link) && !records.contains_key(&link) {
                    pending.insert(link);
                }
            }
            // Ordered subrecords preserve perk effect/tab boundaries and OR condition groups.
            let mut saved = json!({"source_form_id": format!("{raw:08X}"),
                "form_key": source_key(raw, &masters, &plugin)?, "signature": signature, "editor_id": eid,
                "fields": fields.iter().map(|f| json!({"signature": f.signature.as_str(), "hex": hex::encode(&f.data)})).collect::<Vec<_>>()});
            if signature == "CURV" {
                if let Some(field) = fields
                    .iter()
                    .find(|f| matches!(f.signature.as_str(), "CRVE" | "JASF"))
                {
                    let relative = String::from_utf8_lossy(&field.data)
                        .trim_end_matches('\0')
                        .to_owned();
                    let path = curve_path(source_root, &relative)?;
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            saved["curve"] =
                                serde_json::from_slice::<Value>(&bytes).map_err(|e| {
                                    PhaseError::Internal(format!("{}: {e}", path.display()))
                                })?;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            missing_curves.push(relative)
                        }
                        Err(e) => {
                            return Err(PhaseError::Internal(format!("{}: {e}", path.display())));
                        }
                    }
                } else {
                    missing_curves.push(format!("{raw:08X}: missing CRVE/JASF"));
                }
            }
            records.insert(raw, saved);
        }
        references.extend(records.keys());
        drop(scan);
        let mut target_forms = BTreeMap::new();
        for raw in references {
            let key = source_key(raw, &masters, &plugin)?;
            let owner = key.split_once(':').unwrap().1;
            let source = FormKey {
                local: raw & 0xffffff,
                plugin: ctx.run.interner.intern(owner),
            };
            let target = ctx
                .run
                .mapper_state
                .as_ref()
                .and_then(|s| s.source_to_target.get(&source));
            let mut identity = None;
            if let Some(target) = target {
                let owner = ctx.run.interner.resolve(target.plugin).unwrap_or("");
                let target_key = format!("{owner}:{:06X}", target.local);
                for handle in std::iter::once(ctx.run.target_handle_id)
                    .chain(ctx.run.master_handle_ids.iter().copied())
                {
                    let scan = session
                        .handle_raw_scan(handle)
                        .map_err(|e| PhaseError::Internal(e.to_string()))?;
                    if let Some((raw, signature, _)) = scan.record_metadata_by_form_key(&target_key)
                    {
                        identity = scan.with_record(raw, |r| json!({"plugin": owner, "object_id": format!("{:06X}", target.local),
                            "signature": signature, "editor_id": editor_id(&effective_subrecords_for_record(r))}));
                        break;
                    }
                }
            }
            target_forms.insert(key, identity);
        }
        let catalog = json!({"schema_version": 1, "source_plugin": plugin, "source_masters": masters,
            "output_plugin": ctx.run.config.output_plugin_name, "mapping_available": ctx.run.mapper_state.is_some(),
            "missing_curves": missing_curves, "records": records.into_values().collect::<Vec<_>>(), "target_forms": target_forms});
        let directory = ctx
            .mod_path
            .join("F4SE/Plugins/B21_TalesFromAppalachia/Condition");
        let filename = Path::new(&ctx.run.config.output_plugin_name)
            .file_name()
            .ok_or_else(|| PhaseError::BadParams("missing output plugin filename".into()))?;
        std::fs::create_dir_all(&directory).map_err(|e| PhaseError::Internal(e.to_string()))?;
        let mut temporary = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        temporary
            .write_all(
                &serde_json::to_vec(&catalog).map_err(|e| PhaseError::Internal(e.to_string()))?,
            )
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        temporary
            .persist(directory.join(format!("{}.json", filename.to_string_lossy())))
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        Ok(PhaseReport {
            assets_written: 1,
            warnings: missing_curves.len() as u32,
            ..PhaseReport::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(signature: &str, data: &[u8]) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: data.to_vec().into(),
            semantic_type: None,
        }
    }

    #[test]
    fn retains_weapon_damage_level_and_break_sound_inputs() {
        for name in ["CVT0", "EILV", "IBSD"] {
            assert!(keep_field("WEAP", name));
        }
        assert!(keep_field("ARMO", "EILV"));
        assert!(keep_field("ARMO", "IBSD"));
    }

    #[test]
    fn condition_perks_are_selected_by_entry_point_not_name() {
        assert!(is_root(
            "PERK",
            &[field("PRKE", &[2, 0, 0]), field("DATA", &[163, 3, 2, 1])]
        ));
        assert!(is_root(
            "PERK",
            &[field("PRKE", &[2, 0, 0]), field("DATA", &[176, 3, 2, 0])]
        ));
        assert!(!is_root(
            "PERK",
            &[field("PRKE", &[0, 0, 0]), field("DATA", &[163, 3, 2, 0])]
        ));
        assert!(!is_root(
            "PERK",
            &[
                field("PRKE", &[2, 0, 0]),
                field("PRKF", &[]),
                field("DATA", &[163, 3, 2, 0])
            ]
        ));
    }

    #[test]
    fn curve_paths_cannot_escape_the_extracted_tree() {
        let root = Path::new("source");
        assert_eq!(
            curve_path(root, "ItemCondition\\Weapon.json").unwrap(),
            root.join("misc/curvetables/json/ItemCondition/Weapon.json")
        );
        for path in [
            "../secret",
            "C:\\secret",
            "/secret",
            "\\\\server\\secret",
            "",
        ] {
            assert!(curve_path(root, path).is_err(), "{path}");
        }
    }

    #[test]
    fn phase_preserves_curves_perk_scopes_and_verified_remapped_identity() {
        use crate::formkey_mapper::{MapperOptions, MapperState};
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use esp_authoring_core::plugin_runtime::{
            ParsedItem, ParsedRecord, ensure_core_section, ensure_records_section,
            plugin_handle_close_native, plugin_handle_new_no_py, plugin_handle_store_ref,
        };

        fn record(signature: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedItem {
            ParsedItem::Record(ParsedRecord {
                signature: signature.into(),
                form_id,
                flags: 0,
                version_control: 0,
                form_version: Some(209),
                version2: Some(1),
                subrecords,
                raw_payload: None,
                parse_error: None,
            })
        }
        let mut condition = [0u8; 32];
        condition[8..10].copy_from_slice(&448u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x804u32.to_le_bytes());
        let perk_fields = vec![
            field("EDID", b"B21_Durability\0"),
            field("DATA", &[1, 0, 1]),
            field("PRKE", &[2, 1]),
            field("DATA", &[163, 3, 2, 0]),
            field("PRKC", &[0]),
            field("CTDA", &condition),
            field("EPFT", &[1]),
            field("EPFD", &0.8f32.to_le_bytes()),
            field("PRKF", &[]),
        ];
        let source = plugin_handle_new_no_py("Source.esm", Some("fo76"));
        let target = plugin_handle_new_no_py("Renamed.esp", Some("fo4"));
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            for (handle, records) in [
                (
                    source,
                    vec![
                        record(
                            "WEAP",
                            0x800,
                            vec![
                                field("EDID", b"B21_Weapon\0"),
                                field("CVT1", &0x801u32.to_le_bytes()),
                            ],
                        ),
                        record(
                            "CURV",
                            0x801,
                            vec![field("CRVE", b"ItemCondition\\Weapon.json\0")],
                        ),
                        record("PERK", 0x803, perk_fields.clone()),
                        record(
                            "PERK",
                            0x804,
                            vec![
                                field("EDID", b"B21_HigherRank\0"),
                                field("DATA", &[1, 0, 1]),
                            ],
                        ),
                        record(
                            "UTIL",
                            0x805,
                            vec![
                                field("EDID", b"Utility_RepairKit_Improved\0"),
                                field("UITE", &0u32.to_le_bytes()),
                                field("UITO", &0u32.to_le_bytes()),
                                field("UITV", &1.5f32.to_le_bytes()),
                            ],
                        ),
                        record("PERK", 0x806, vec![field("EDID", b"B21_Unrelated\0")]),
                        record(
                            "COBJ",
                            0x808,
                            vec![
                                field("CNAM", &0x800u32.to_le_bytes()),
                                field("REPR", &[0x07, 0x08, 0, 0, 2, 0, 0, 0, 1, 8, 0, 0]),
                            ],
                        ),
                    ],
                ),
                (
                    target,
                    vec![
                        record("WEAP", 0x900, vec![field("EDID", b"B21_ConvertedWeapon\0")]),
                        record("CMPO", 0x907, vec![field("EDID", b"B21_RepairComponent\0")]),
                    ],
                ),
            ] {
                let slot = store.get_mut(&handle).unwrap();
                slot.parsed.root_items = records;
                let _ = ensure_core_section(slot);
                let _ = ensure_records_section(slot);
            }
        }
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Renamed.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let run_phase = || {
            with_run(id, |run| -> Result<PhaseReport, RunError> {
                let mut mapper = MapperState::new([], MapperOptions::default());
                mapper.source_to_target.insert(
                    FormKey {
                        local: 0x800,
                        plugin: run.interner.intern("Source.esm"),
                    },
                    FormKey {
                        local: 0x900,
                        plugin: run.interner.intern("Renamed.esp"),
                    },
                );
                mapper.source_to_target.insert(
                    FormKey {
                        local: 0x807,
                        plugin: run.interner.intern("Source.esm"),
                    },
                    FormKey {
                        local: 0x907,
                        plugin: run.interner.intern("Renamed.esp"),
                    },
                );
                // A mapping to an absent target must never become a usable runtime FormID.
                mapper.source_to_target.insert(
                    FormKey {
                        local: 0x803,
                        plugin: run.interner.intern("Source.esm"),
                    },
                    FormKey {
                        local: 0x999,
                        plugin: run.interner.intern("Renamed.esp"),
                    },
                );
                run.mapper_state = Some(mapper);
                EquipmentConditionPhase
                    .run(&mut PhaseCtx {
                        run,
                        mod_path: directory.path(),
                        source_extracted_dir: directory.path(),
                        target_extracted_dir: None,
                        target_data_dir: None,
                        params: &Value::Null,
                        cancel: &std::sync::atomic::AtomicBool::new(false),
                    })
                    .map_err(|e| RunError::InvalidConfig(e.to_string()))
            })
            .unwrap()
        };
        assert_eq!(run_phase().warnings, 1);
        let curve = curve_path(directory.path(), "ItemCondition/Weapon.json").unwrap();
        std::fs::create_dir_all(curve.parent().unwrap()).unwrap();
        std::fs::write(curve, br#"{"curve":[{"x":1,"y":30},{"x":50,"y":70}]}"#).unwrap();
        assert_eq!(run_phase().warnings, 0);
        let catalog: Value = serde_json::from_slice(
            &std::fs::read(
                directory
                    .path()
                    .join("F4SE/Plugins/B21_TalesFromAppalachia/Condition/Renamed.esp.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            catalog["target_forms"]["000800:Source.esm"]["object_id"],
            "000900"
        );
        assert!(catalog["target_forms"]["000803:Source.esm"].is_null());
        let records = catalog["records"].as_array().unwrap();
        assert_eq!(records.len(), 6);
        assert_eq!(
            catalog["target_forms"]["000807:Source.esm"]["object_id"],
            "000907"
        );
        assert!(records.iter().any(|r| r["source_form_id"] == "00000804"));
        let perk = records
            .iter()
            .find(|r| r["source_form_id"] == "00000803")
            .unwrap();
        for (saved, original) in perk["fields"]
            .as_array()
            .unwrap()
            .iter()
            .zip(perk_fields.iter())
        {
            assert_eq!(saved["signature"], original.signature.as_str());
            assert_eq!(saved["hex"], hex::encode(&original.data));
        }
        let curve = records.iter().find(|r| r["signature"] == "CURV").unwrap();
        assert_eq!(curve["curve"]["curve"][1]["y"], 70);
        let kit = records.iter().find(|r| r["signature"] == "UTIL").unwrap();
        assert_eq!(kit["fields"][3]["hex"], hex::encode(1.5f32.to_le_bytes()));
        drop_run(id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
    }
}
