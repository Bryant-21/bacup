//! Shared test helpers for store2 equivalence tests. Test-only — the module is
//! `#[cfg(test)]`-gated in `store2/mod.rs`.

use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, effective_subrecords_for_record, plugin_handle_store_ref,
};

pub(crate) fn flatten_records(items: &[ParsedItem], out: &mut Vec<ParsedRecord>) {
    for item in items {
        match item {
            ParsedItem::Record(r) => out.push(r.clone()),
            ParsedItem::Group(g) => flatten_records(&g.children, out),
        }
    }
}

pub(crate) fn handle_records(handle: u64) -> Vec<ParsedRecord> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store.get_mut(&handle).unwrap();
    let mut out = Vec::new();
    flatten_records(&slot.parsed.root_items, &mut out);
    out
}

/// Assert two plugin handles hold byte-identical record streams (record order,
/// header fields, every subrecord's bytes) and identical localized-strings
/// state. Order-sensitive on purpose: group repositioning is a real diff.
pub(crate) fn assert_handles_equal(old: u64, new: u64) {
    let old_recs = handle_records(old);
    let new_recs = handle_records(new);
    assert_eq!(
        old_recs.len(),
        new_recs.len(),
        "record count differs: legacy={} v2={}",
        old_recs.len(),
        new_recs.len()
    );
    for (o, n) in old_recs.iter().zip(new_recs.iter()) {
        assert_eq!(o.signature, n.signature, "signature order differs");
        assert_eq!(o.form_id, n.form_id, "form_id differs for {}", o.signature);
        assert_eq!(o.flags, n.flags, "flags differ for {:08X}", o.form_id);
        assert_eq!(
            o.form_version, n.form_version,
            "form_version differs for {:08X}",
            o.form_id
        );
        assert_eq!(
            o.version_control, n.version_control,
            "version_control differs for {:08X}",
            o.form_id
        );
        let o_subs = effective_subrecords_for_record(o);
        let n_subs = effective_subrecords_for_record(n);
        assert_eq!(
            o_subs.len(),
            n_subs.len(),
            "subrecord count differs for {:08X}",
            o.form_id
        );
        for (os, ns) in o_subs.iter().zip(n_subs.iter()) {
            assert_eq!(
                os.signature, ns.signature,
                "subrecord sig differs in {:08X}",
                o.form_id
            );
            assert_eq!(
                os.data.as_ref(),
                ns.data.as_ref(),
                "subrecord {} bytes differ in {:08X}",
                os.signature,
                o.form_id
            );
        }
    }
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let old_strings = store.get_mut(&old).unwrap().strings_ref().clone();
    let new_strings = store.get_mut(&new).unwrap().strings_ref().clone();
    assert_eq!(
        old_strings.by_language, new_strings.by_language,
        "localized strings by_language differs"
    );
    assert_eq!(
        old_strings.table_types, new_strings.table_types,
        "localized strings table_types differs"
    );
}
