use std::collections::BTreeMap;
use std::io::Write;

use esp_authoring_core::plugin_runtime::{ParsedSubrecord, effective_subrecords_for_record};
use serde_json::json;

use crate::ids::{FormKey, SigCode};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::session::open_session;
use crate::translator::Game;

pub struct GoldBullionPhase;

fn price_global(fields: &[ParsedSubrecord]) -> Result<Option<u32>, PhaseError> {
    let Some(field) = fields.iter().find(|field| field.signature == "BVGO") else {
        return Ok(None);
    };
    let bytes: [u8; 4] = field
        .data
        .as_ref()
        .try_into()
        .map_err(|_| PhaseError::Internal("invalid bullion price global reference".into()))?;
    let raw = u32::from_le_bytes(bytes);
    Ok((raw != 0).then_some(raw))
}

fn global_value(fields: &[ParsedSubrecord]) -> Option<u32> {
    let field = fields.iter().find(|field| field.signature == "FLTV")?;
    let value = f32::from_le_bytes(field.data.as_ref().try_into().ok()?);
    (value.is_finite() && value > 0.0 && value < 2147483648.0 && value.fract() == 0.0)
        .then_some(value as u32)
}

fn source_key(raw: u32, masters: &[String], plugin: &str) -> Result<(String, u32), PhaseError> {
    let index = (raw >> 24) as usize;
    let owner = if index == masters.len() || index == 0xff {
        plugin
    } else {
        masters
            .get(index)
            .map(String::as_str)
            .ok_or_else(|| PhaseError::Internal(format!("invalid bullion FormID {raw:08X}")))?
    };
    Ok((owner.into(), raw & 0xffffff))
}

impl Phase for GoldBullionPhase {
    fn name(&self) -> &'static str {
        "emit_gold_bullion"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        if ctx.mod_path.as_os_str().is_empty() {
            return Err(PhaseError::BadParams(
                "bullion catalog requires mod_path".into(),
            ));
        }
        let mut session = open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let (masters, plugin) = session
            .handle_load_order(ctx.run.source_handle_id)
            .map(|(masters, plugin)| (masters.to_vec(), plugin.to_string()))
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let scan = session
            .handle_raw_scan(ctx.run.source_handle_id)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let mut source_prices = Vec::new();
        for signature in ["BOOK", "MISC", "ALCH", "ARMO", "WEAP", "AMMO"] {
            for raw in scan.raw_form_ids_of_sig(SigCode::from_str(signature).unwrap()) {
                ctx.check_cancel()?;
                let global = scan
                    .with_record(raw, |record| {
                        if record.flags & 0x20 != 0 {
                            return Ok(None);
                        }
                        price_global(&effective_subrecords_for_record(record))
                    })
                    .ok_or_else(|| {
                        PhaseError::Internal(format!("missing bullion item {raw:08X}"))
                    })??;
                let Some(global) = global else {
                    continue;
                };
                let value = scan
                    .with_record(global, |record| {
                        global_value(&effective_subrecords_for_record(record))
                    })
                    .flatten()
                    .ok_or_else(|| {
                        PhaseError::Internal(format!("missing bullion price GLOB {global:08X}"))
                    })?;
                source_prices.push((raw, value));
            }
        }
        let mut catalogs = vec![("GoldBullion.json", source_prices)];
        for (filename, root) in [
            ("Stamps.json", 0x63966F),
            ("TadpoleBadges.json", 0x3FC7DF),
            ("PossumBadges.json", 0x426912),
        ] {
            if !scan.with_record(root, |_| true).unwrap_or(false) {
                continue;
            }
            let mut prices = Vec::new();
            let mut pending = vec![root];
            let mut visited = std::collections::HashSet::new();
            while !pending.is_empty() {
                ctx.check_cancel()?;
                let raw = pending.pop().unwrap();
                if !visited.insert(raw) {
                    continue;
                }
                let (signature, fields) = scan
                    .with_record(raw, |record| {
                        (
                            record.signature.clone(),
                            effective_subrecords_for_record(record).into_owned(),
                        )
                    })
                    .ok_or_else(|| {
                        PhaseError::Internal(format!("missing {filename} stock {raw:08X}"))
                    })?;
                if raw == root && signature != "LVLI" {
                    return Err(PhaseError::Internal(format!(
                        "expected stock list at {root:08X}"
                    )));
                }
                if signature == "LVLI" {
                    for field in fields.iter().filter(|f| f.signature == "LVLO") {
                        let bytes: [u8; 4] = field.data.as_ref().try_into().map_err(|_| {
                            PhaseError::Internal(format!("invalid {filename} stock entry"))
                        })?;
                        pending.push(u32::from_le_bytes(bytes));
                    }
                } else if matches!(signature.as_str(), "BOOK" | "ALCH" | "MISC" | "ARMO") {
                    let price_field = if signature == "ALCH" { "ENIT" } else { "DATA" };
                    let price = fields
                        .iter()
                        .find(|f| f.signature == price_field)
                        .and_then(|f| f.data.get(..4))
                        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
                        .filter(|v| *v > 0 && *v <= i32::MAX as u32)
                        .ok_or_else(|| {
                            PhaseError::Internal(format!("invalid {filename} price {raw:08X}"))
                        })?;
                    prices.push((raw, price));
                } else {
                    return Err(PhaseError::Internal(format!(
                        "unsupported {filename} stock {raw:08X}: {signature}"
                    )));
                }
            }
            catalogs.push((filename, prices));
        }
        drop(scan);
        let mut report = PhaseReport::default();
        for (filename, source_prices) in catalogs {
            let mut prices = BTreeMap::new();
            let mut skipped = 0;
            for (raw, price) in source_prices {
                let (owner, local) = source_key(raw, &masters, &plugin)?;
                let source = FormKey {
                    local,
                    plugin: ctx.run.interner.intern(&owner),
                };
                let Some(target) = ctx
                    .run
                    .mapper_state
                    .as_ref()
                    .and_then(|map| map.source_to_target.get(&source))
                else {
                    skipped += 1;
                    continue;
                };
                let owner = ctx
                    .run
                    .interner
                    .resolve(target.plugin)
                    .unwrap_or("")
                    .to_string();
                let key = format!("{owner}:{:06X}", target.local);
                let mut exists = false;
                for handle in std::iter::once(ctx.run.target_handle_id)
                    .chain(ctx.run.master_handle_ids.iter().copied())
                {
                    let scan = session
                        .handle_raw_scan(handle)
                        .map_err(|e| PhaseError::Internal(e.to_string()))?;
                    if scan.record_metadata_by_form_key(&key).is_some() {
                        exists = true;
                        break;
                    }
                }
                if !exists {
                    skipped += 1;
                    continue;
                }
                if let Some(previous) = prices.insert((owner, target.local), price) {
                    if previous != price {
                        return Err(PhaseError::Internal(format!(
                            "conflicting bullion prices for {key}"
                        )));
                    }
                }
            }
            let rows: Vec<_> = prices.into_iter().map(|((plugin, object_id), price)|
                json!({"plugin": plugin, "object_id": object_id, "price": price})).collect();
            let catalog = json!({"schema_version": 1, "prices": rows, "skipped_unmapped": skipped});
            let directory = ctx.mod_path.join("F4SE/Plugins/B21_TalesFromAppalachia");
            std::fs::create_dir_all(&directory).map_err(|e| PhaseError::Internal(e.to_string()))?;
            let mut pending = tempfile::NamedTempFile::new_in(&directory)
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            pending
                .write_all(
                    &serde_json::to_vec(&catalog)
                        .map_err(|e| PhaseError::Internal(e.to_string()))?,
                )
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            pending
                .persist(directory.join(filename))
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            report.assets_written += 1;
            report.warnings += skipped;
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(signature: &str, bytes: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: bytes.into(),
            semantic_type: None,
        }
    }

    #[test]
    fn bullion_uses_bvgo_global_and_rejects_invalid_prices() {
        let fields = vec![
            field("DATA", 125_u32.to_le_bytes().to_vec()),
            field("BVGO", 0x005a504d_u32.to_le_bytes().to_vec()),
        ];
        assert_eq!(price_global(&fields).unwrap(), Some(0x005a504d));
        assert_eq!(
            global_value(&[field("FLTV", 1250_f32.to_le_bytes().to_vec())]),
            Some(1250)
        );
        assert!(price_global(&[field("BVGO", vec![0; 3])]).is_err());
        assert_eq!(price_global(&[field("DATA", vec![0; 4])]).unwrap(), None);

        for value in [f32::NAN, f32::INFINITY, -1.0, 0.0, 1.5] {
            assert_eq!(
                global_value(&[field("FLTV", value.to_le_bytes().to_vec())]),
                None
            );
        }
        let masters = vec!["Base.esm".into()];
        assert_eq!(
            source_key(0x01000800, &masters, "Source.esm").unwrap(),
            ("Source.esm".into(), 0x800)
        );
        assert_eq!(
            source_key(0x00000800, &masters, "Source.esm").unwrap(),
            ("Base.esm".into(), 0x800)
        );
        assert!(source_key(0x02000800, &masters, "Source.esm").is_err());
    }

    #[test]
    fn phase_writes_remapped_prices_and_reports_missing_items() {
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
                form_version: Some(131),
                version2: Some(1),
                subrecords,
                raw_payload: None,
                parse_error: None,
            })
        }
        let source = plugin_handle_new_no_py("Source.esm", Some("fo76"));
        let target = plugin_handle_new_no_py("Renamed.esp", Some("fo4"));
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            for (handle, records) in [
                (
                    source,
                    vec![
                        record(
                            "GLOB",
                            0x800,
                            vec![field("FLTV", 1250_f32.to_le_bytes().to_vec())],
                        ),
                        record(
                            "BOOK",
                            0x801,
                            vec![field("BVGO", 0x800_u32.to_le_bytes().to_vec())],
                        ),
                        record(
                            "BOOK",
                            0x802,
                            vec![field("BVGO", 0x800_u32.to_le_bytes().to_vec())],
                        ),
                        record(
                            "LVLI",
                            0x63966F,
                            vec![field("LVLO", 0x820_u32.to_le_bytes().to_vec())],
                        ),
                        record(
                            "LVLI",
                            0x820,
                            vec![
                                field("LVLO", 0x821_u32.to_le_bytes().to_vec()),
                                field("LVLO", 0x822_u32.to_le_bytes().to_vec()),
                                field("LVLO", 0x823_u32.to_le_bytes().to_vec()),
                            ],
                        ),
                        record(
                            "BOOK",
                            0x821,
                            vec![field(
                                "DATA",
                                [50_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat(),
                            )],
                        ),
                        record(
                            "BOOK",
                            0x822,
                            vec![field(
                                "DATA",
                                [500_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat(),
                            )],
                        ),
                        record(
                            "ALCH",
                            0x823,
                            vec![
                                field("DATA", 1_f32.to_le_bytes().to_vec()),
                                field("ENIT", [20_u32.to_le_bytes(), 0_u32.to_le_bytes()].concat()),
                            ],
                        ),
                        record(
                            "LVLI",
                            0x3FC7DF,
                            vec![field("LVLO", 0x824_u32.to_le_bytes().to_vec())],
                        ),
                        record(
                            "LVLI",
                            0x426912,
                            vec![field("LVLO", 0x825_u32.to_le_bytes().to_vec())],
                        ),
                        record(
                            "ARMO",
                            0x824,
                            vec![field(
                                "DATA",
                                [
                                    3_u32.to_le_bytes(),
                                    0_f32.to_le_bytes(),
                                    0_u32.to_le_bytes(),
                                ]
                                .concat(),
                            )],
                        ),
                        record(
                            "BOOK",
                            0x825,
                            vec![field(
                                "DATA",
                                [8_u32.to_le_bytes(), 0_f32.to_le_bytes()].concat(),
                            )],
                        ),
                    ],
                ),
                (
                    target,
                    vec![
                        record("BOOK", 0x900, vec![field("EDID", b"B21_Plan\0".to_vec())]),
                        record(
                            "BOOK",
                            0x901,
                            vec![field("EDID", b"B21_StampPlan\0".to_vec())],
                        ),
                        record(
                            "ALCH",
                            0x902,
                            vec![field("EDID", b"B21_StampBox\0".to_vec())],
                        ),
                        record(
                            "ARMO",
                            0x903,
                            vec![field("EDID", b"B21_TadpoleHat\0".to_vec())],
                        ),
                        record(
                            "BOOK",
                            0x904,
                            vec![field("EDID", b"B21_PossumPlan\0".to_vec())],
                        ),
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
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let mut mapper = MapperState::new([], MapperOptions::default());
            mapper.source_to_target.insert(
                FormKey {
                    local: 0x801,
                    plugin: run.interner.intern("Source.esm"),
                },
                FormKey {
                    local: 0x900,
                    plugin: run.interner.intern("Renamed.esp"),
                },
            );
            run.mapper_state = Some(mapper);
            run.mapper_state.as_mut().unwrap().source_to_target.insert(
                FormKey {
                    local: 0x821,
                    plugin: run.interner.intern("Source.esm"),
                },
                FormKey {
                    local: 0x901,
                    plugin: run.interner.intern("Renamed.esp"),
                },
            );
            run.mapper_state.as_mut().unwrap().source_to_target.insert(
                FormKey {
                    local: 0x823,
                    plugin: run.interner.intern("Source.esm"),
                },
                FormKey {
                    local: 0x902,
                    plugin: run.interner.intern("Renamed.esp"),
                },
            );
            for (source, target) in [(0x824, 0x903), (0x825, 0x904)] {
                run.mapper_state.as_mut().unwrap().source_to_target.insert(
                    FormKey {
                        local: source,
                        plugin: run.interner.intern("Source.esm"),
                    },
                    FormKey {
                        local: target,
                        plugin: run.interner.intern("Renamed.esp"),
                    },
                );
            }
            let mut ctx = PhaseCtx {
                run,
                mod_path: directory.path(),
                source_extracted_dir: directory.path(),
                target_extracted_dir: None,
                target_data_dir: None,
                params: &serde_json::Value::Null,
                cancel: &std::sync::atomic::AtomicBool::new(false),
            };
            GoldBullionPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();
        assert_eq!(report.assets_written, 4);
        assert_eq!(report.warnings, 2);
        let catalog: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                directory
                    .path()
                    .join("F4SE/Plugins/B21_TalesFromAppalachia/GoldBullion.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            catalog["prices"],
            json!([{"plugin": "Renamed.esp", "object_id": 0x900, "price": 1250}])
        );
        let stamps: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                directory
                    .path()
                    .join("F4SE/Plugins/B21_TalesFromAppalachia/Stamps.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            stamps["prices"],
            json!([
                {"plugin": "Renamed.esp", "object_id": 0x901, "price": 50},
                {"plugin": "Renamed.esp", "object_id": 0x902, "price": 20},
            ])
        );
        assert_eq!(stamps["skipped_unmapped"], 1);
        for (name, object_id, price) in [("TadpoleBadges", 0x903, 3), ("PossumBadges", 0x904, 8)] {
            let badges: serde_json::Value = serde_json::from_slice(
                &std::fs::read(
                    directory
                        .path()
                        .join(format!("F4SE/Plugins/B21_TalesFromAppalachia/{name}.json")),
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(
                badges["prices"],
                json!([{"plugin": "Renamed.esp", "object_id": object_id, "price": price}])
            );
        }
        drop_run(id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
    }
}
