use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use conversion_native::fixups::{FixupConfig, FixupReport};
use conversion_native::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
use conversion_native::session::open_session;
use conversion_native::sym::StringInterner;
use conversion_native::translator::Game;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_load_index_no_py, plugin_handle_load_no_py, plugin_handle_save_no_py,
};

fn report(value: &FixupReport, interner: &StringInterner) -> serde_json::Value {
    serde_json::json!({
        "changed": value.records_changed, "dropped": value.records_dropped,
        "added": value.records_added,
        "warnings": value.warnings.iter().map(|s| interner.resolve(*s)).collect::<Vec<_>>(),
        "diagnostics": value.diagnostics.iter().map(|s| interner.resolve(*s)).collect::<Vec<_>>()
    })
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(
        args.len(),
        4,
        "<plugin> <master directory> <output plugin> <report.json>"
    );
    assert_ne!(
        std::path::Path::new(&args[0]),
        std::path::Path::new(&args[2])
    );
    let stage = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicBool::new(false));
    let sampler = {
        let stage = Arc::clone(&stage);
        let done = Arc::clone(&done);
        std::thread::spawn(move || {
            let mut peaks = [[0usize; 2]; 5];
            while !done.load(Ordering::Relaxed) {
                let index = stage.load(Ordering::Relaxed);
                if let Some(memory) = memory_stats::memory_stats() {
                    peaks[index][0] = peaks[index][0].max(memory.physical_mem);
                    peaks[index][1] = peaks[index][1].max(memory.virtual_mem);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            peaks
        })
    };
    let started = Instant::now();
    let target = plugin_handle_load_no_py(&args[0], Some("fo4"), None, None, true).unwrap();
    let masters = open_session(target, None)
        .unwrap()
        .target_masters()
        .to_vec();
    let mut config = FixupConfig::default();
    for master in masters {
        let path = std::path::Path::new(&args[1]).join(master);
        config.target_master_handle_ids.push(
            plugin_handle_load_index_no_py(path.to_str().unwrap(), Some("fo4"), None, None)
                .unwrap(),
        );
    }
    let load_seconds = started.elapsed().as_secs_f64();
    stage.store(1, Ordering::Relaxed);
    let started = Instant::now();
    conversion_native::fixups::normalize_placed_records::normalize_copied_placed_records(
        target,
        &["REFR", "ACHR", "PGRE", "PHZD"].map(str::to_owned),
    )
    .unwrap();
    let normalize_seconds = started.elapsed().as_secs_f64();
    let interner = StringInterner::new();
    let mut state = MapperState::new([], MapperOptions::default());
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let mut session = open_session(target, None).unwrap();
    stage.store(2, Ordering::Relaxed);
    let started = Instant::now();
    let vmad =
        conversion_native::fixups::placed_record_vmad::apply_fo76_fo4_placed_record_vmad_catalog(
            &mut session,
            &mut mapper,
            Game::Fo76,
            Game::Fo4,
        )
        .unwrap();
    let vmad_seconds = started.elapsed().as_secs_f64();
    stage.store(3, Ordering::Relaxed);
    let started = Instant::now();
    let teleport =
        conversion_native::fixups::repair_placed_teleport_doors::repair_placed_references(
            &mut session,
            &mut mapper,
            &config,
        )
        .unwrap();
    let teleport_seconds = started.elapsed().as_secs_f64();
    drop(session);
    stage.store(4, Ordering::Relaxed);
    let started = Instant::now();
    plugin_handle_save_no_py(target, &args[2]).unwrap();
    let save_seconds = started.elapsed().as_secs_f64();
    done.store(true, Ordering::Relaxed);
    let peaks = sampler.join().unwrap();
    let results = serde_json::json!({
        "load_seconds": load_seconds, "normalize_seconds": normalize_seconds,
        "vmad_seconds": vmad_seconds, "teleport_seconds": teleport_seconds,
        "save_seconds": save_seconds, "vmad": report(&vmad, &interner),
        "teleport": report(&teleport, &interner), "peak_bytes_by_stage": peaks,
        "stage_names": ["load", "normalize", "vmad", "teleport", "save"],
        "memory_columns": ["working_set", "private_bytes"], "sample_interval_ms": 10
    });
    std::fs::write(&args[3], serde_json::to_vec_pretty(&results).unwrap()).unwrap();
}
