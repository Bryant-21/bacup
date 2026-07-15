use std::path::{Path, PathBuf};

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, ParsedSubrecord, insert_parsed_record_in_slot,
    plugin_handle_close_native, plugin_handle_new_native, plugin_handle_save_no_py,
    plugin_handle_store_ref,
};

pub(crate) fn edid_sub(edid: &str) -> ParsedSubrecord {
    let mut data = edid.as_bytes().to_vec();
    data.push(0);
    ParsedSubrecord {
        signature: "EDID".into(),
        data: data.into(),
        semantic_type: None,
    }
}

pub(crate) fn rec(sig: &str, form_id: u32, edid: &str) -> ParsedRecord {
    ParsedRecord {
        signature: sig.into(),
        form_id,
        flags: 0,
        version_control: 0,
        form_version: None,
        version2: None,
        subrecords: if edid.is_empty() {
            Vec::new()
        } else {
            vec![edid_sub(edid)]
        },
        raw_payload: None,
        parse_error: None,
    }
}

pub(crate) fn write_test_plugin(
    dir: &Path,
    name: &str,
    game: &str,
    records: Vec<ParsedRecord>,
) -> PathBuf {
    write_test_plugin_with_masters(dir, name, game, Vec::new(), records)
}

pub(crate) fn write_test_plugin_with_masters(
    dir: &Path,
    name: &str,
    game: &str,
    masters: Vec<String>,
    records: Vec<ParsedRecord>,
) -> PathBuf {
    let path = dir.join(name);
    let handle = plugin_handle_new_native(name, Some(game)).expect("new test plugin");
    {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle).unwrap();
        slot.parsed.header.masters = masters;
        for record in records {
            insert_parsed_record_in_slot(slot, record);
        }
    }
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).expect("save test plugin");
    plugin_handle_close_native(handle);
    path
}

pub(crate) fn write_test_plugin_items(
    dir: &Path,
    name: &str,
    game: &str,
    masters: Vec<String>,
    items: Vec<ParsedItem>,
) -> PathBuf {
    let path = dir.join(name);
    let handle = plugin_handle_new_native(name, Some(game)).expect("new test plugin");
    {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle).unwrap();
        slot.parsed.header.masters = masters;
        slot.parsed.root_items = items;
        slot.invalidate_sections();
    }
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).expect("save test plugin");
    plugin_handle_close_native(handle);
    path
}

pub(crate) fn formid_sub(signature: &str, form_id: u32) -> ParsedSubrecord {
    ParsedSubrecord {
        signature: signature.into(),
        data: Bytes::copy_from_slice(&form_id.to_le_bytes()),
        semantic_type: Some("formid".to_string()),
    }
}
