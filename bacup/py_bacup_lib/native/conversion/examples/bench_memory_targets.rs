use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

use esp_authoring_core::plugin_runtime::{parse_plugin_file, parse_plugin_file_projection};
use terrain_native::authoring_emit::{
    ConvertOptions, convert_prepared_btd_with_structured_cell_sink, prepare_terrain_texture_scan,
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    pyo3::Python::initialize();
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build_global()
        .unwrap();
    let started = Instant::now();
    match args[0].as_str() {
        "terrain" => {
            let options: ConvertOptions =
                serde_json::from_slice(&std::fs::read(&args[1]).unwrap()).unwrap();
            let mut scan_options = options.clone();
            scan_options.emit_textures = false;
            scan_options.texture_manifest_path.clear();
            let prepared = prepare_terrain_texture_scan(scan_options).unwrap();
            let scan_seconds = started.elapsed().as_secs_f64();
            let mut records = BTreeMap::new();
            let mut cells = BTreeMap::new();
            let report = convert_prepared_btd_with_structured_cell_sink(
                prepared.source,
                options,
                &mut |record| {
                    records.insert(
                        record.relative_path,
                        blake3::hash(record.yaml.as_bytes()).to_hex().to_string(),
                    );
                    Ok(())
                },
                &mut |record| {
                    cells.insert(
                        record.relative_path,
                        blake3::hash(&serde_json::to_vec(&record.value).unwrap())
                            .to_hex()
                            .to_string(),
                    );
                    Ok(())
                },
            )
            .unwrap();
            let result = serde_json::json!({"seconds":started.elapsed().as_secs_f64(), "scan_seconds":scan_seconds, "report":report, "records":records, "cells":cells});
            std::fs::write(&args[2], serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        }
        "anim" => {
            let projected = args.get(4).is_some_and(|arg| arg == "projected");
            let load = |path: &str, signatures: &[[u8; 4]]| {
                if projected {
                    parse_plugin_file_projection(path, Some("fo4".into()), signatures)
                } else {
                    parse_plugin_file(path, Some("fo4".into()), false)
                }
                .unwrap()
            };
            let target = load(&args[1], &[*b"RACE", *b"IDLE"]);
            let base = load(&args[2], &[*b"RACE"]);
            let load_seconds = started.elapsed().as_secs_f64();
            let decoded =
                ck_native::anim_text_data::race_decode::subgraph_inputs_from_parsed_plugins(
                    &target,
                    &[&base],
                )
                .unwrap();
            std::fs::write(Path::new(&args[3]), format!("{decoded:#?}")).unwrap();
            println!(
                "{}",
                serde_json::json!({"load_seconds":load_seconds,"seconds":started.elapsed().as_secs_f64(),"races":decoded.race_record_count,"subgraphs":decoded.subgraphs.len()})
            );
        }
        "records" => {
            use esp_authoring_core::plugin_runtime::{
                build_locator_section, ensure_core_section, ensure_records_section,
                plugin_handle_close_native, plugin_handle_load_lazy_records_no_py,
                plugin_handle_load_no_py, plugin_handle_store_ref,
            };
            let mut measurements = Vec::new();
            let mut sample = |stage: &str| {
                esp_authoring_core::trim_allocator();
                let memory = memory_stats::memory_stats().unwrap();
                measurements.push(serde_json::json!({"stage": stage,
                    "seconds": started.elapsed().as_secs_f64(),
                    "private_bytes": memory.virtual_mem, "working_set_bytes": memory.physical_mem}));
            };
            let source = if args.get(4).is_some_and(|arg| arg == "compact") {
                plugin_handle_load_lazy_records_no_py(
                    &args[1],
                    "fo76",
                    None,
                    &[*b"REFR", *b"ACHR", *b"PGRE", *b"PHZD"],
                )
                .unwrap()
            } else {
                plugin_handle_load_no_py(&args[1], Some("fo76"), None, None, false).unwrap()
            };
            sample("source_loaded");
            {
                let mut store = plugin_handle_store_ref().lock().unwrap();
                let slot = store.get_mut(&source).unwrap();
                ensure_core_section(slot);
                ensure_records_section(slot);
            }
            sample("source_with_core_and_records");
            let target =
                plugin_handle_load_no_py(&args[2], Some("fo4"), None, None, false).unwrap();
            sample("source_and_target_loaded");
            let locator = {
                let store = plugin_handle_store_ref().lock().unwrap();
                build_locator_section(&store[&source].parsed)
            };
            sample("with_copy_source_locator");
            drop(locator);
            sample("after_copy_source_locator_drop");
            plugin_handle_close_native(source);
            sample("target_only");
            plugin_handle_close_native(target);
            sample("after_close");
            std::fs::write(&args[3], serde_json::to_vec_pretty(&measurements).unwrap()).unwrap();
        }
        "graft" => {
            use esp_authoring_core::plugin_runtime::{
                graft_terrain_navmesh_from_handle, graft_terrain_navmesh_object_ids_from_handle,
                plugin_handle_close_native, plugin_handle_load_lazy_records_no_py,
                plugin_handle_load_no_py, plugin_handle_save_no_py,
            };
            let prior = if args.get(5).is_some_and(|arg| arg == "compact") {
                plugin_handle_load_lazy_records_no_py(
                    &args[1],
                    "fo4",
                    None,
                    &[*b"REFR", *b"ACHR", *b"PGRE", *b"PHZD"],
                )
            } else {
                plugin_handle_load_no_py(&args[1], Some("fo4"), None, None, false)
            }
            .unwrap();
            let load_seconds = started.elapsed().as_secs_f64();
            let reservations = graft_terrain_navmesh_object_ids_from_handle(prior).unwrap();
            let target = plugin_handle_load_no_py(&args[2], Some("fo4"), None, None, false).unwrap();
            let report = graft_terrain_navmesh_from_handle(prior, target).unwrap();
            let graft_seconds = started.elapsed().as_secs_f64();
            plugin_handle_close_native(prior);
            plugin_handle_save_no_py(target, &args[4]).unwrap();
            plugin_handle_close_native(target);
            let result = serde_json::json!({
                "seconds": started.elapsed().as_secs_f64(), "load_seconds": load_seconds,
                "graft_seconds": graft_seconds, "reservations": reservations,
                "cells": report.cells, "lands": report.lands, "navms": report.navms,
                "txst": report.txst, "ltex": report.ltex, "gras": report.gras,
                "object_ids": report.object_ids, "warnings": report.warnings,
            });
            std::fs::write(&args[3], serde_json::to_vec_pretty(&result).unwrap()).unwrap();
        }
        _ => panic!(
            "usage: bench_memory_targets terrain <options.json> <result.json> | anim <target> <base> <decoded.txt> [projected] | records <source> <target> <report.json> [compact] | graft <prior> <target> <report.json> <output> [compact]"
        ),
    }
}
