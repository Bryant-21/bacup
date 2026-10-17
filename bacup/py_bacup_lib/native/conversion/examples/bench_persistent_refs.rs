use conversion_native::ids::SigCode;
use conversion_native::record::RecordFlags;
use conversion_native::session::open_session;
use conversion_native::sym::StringInterner;
use esp_authoring_core::plugin_runtime::plugin_handle_load_index_no_py;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert_eq!(args.len(), 4, "<master.esm> <all|filtered> <report.json>");
    let handle = plugin_handle_load_index_no_py(&args[1], Some("fo4"), None, None).unwrap();
    let mut session = open_session(handle, None).unwrap();
    let schema = session.schema().unwrap();
    let interner = StringInterner::new();
    let sig = SigCode::from_str("REFR").unwrap();
    let started = std::time::Instant::now();
    let keys = if args[2] == "filtered" {
        session.form_keys_of_sig_with_flags_in_handle(
            handle,
            sig,
            RecordFlags::PERSISTENT.bits(),
            &interner,
        )
    } else {
        session.form_keys_of_sig_in_handle(handle, sig, &interner)
    }
    .unwrap();
    let candidates = keys.len();
    let valid: Vec<_> = keys
        .into_iter()
        .filter(|key| {
            session
                .record_decoded_in_handle(handle, key, &schema, &interner)
                .is_ok_and(|record| record.flags.contains(RecordFlags::PERSISTENT))
        })
        .map(|key| (interner.resolve(key.plugin).unwrap().to_string(), key.local))
        .collect();
    let elapsed = started.elapsed().as_secs_f64();
    let report = serde_json::json!({"candidates": candidates, "seconds": elapsed, "valid": valid});
    std::fs::write(&args[3], serde_json::to_vec(&report).unwrap()).unwrap();
    println!(
        "{candidates} candidates, {} persistent references in {elapsed:.3}s",
        valid.len()
    );
}
