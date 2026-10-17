use std::ops::ControlFlow;
use std::path::Path;

use esp_authoring_core::plugin_runtime::{effective_subrecords_for_record, plugin_handle_store_ref, ParsedRecord};

use crate::run::OwnedPluginHandle;

fn marker_type(record: &ParsedRecord) -> Option<u16> {
    let fields = effective_subrecords_for_record(record);
    if !fields.iter().any(|field| field.signature == "XMRK") {
        return None;
    }
    fields.iter().find(|field| field.signature == "TNAM" && field.data.len() >= 2)
        .map(|field| u16::from_le_bytes([field.data[0], field.data[1]]))
}

pub fn collect(source_path: &Path) -> Result<Vec<(u32, u16)>, String> {
    let source = OwnedPluginHandle::load_index(source_path, "fo76").map_err(|error| error.to_string())?;
    let store = plugin_handle_store_ref().lock().map_err(|error| error.to_string())?;
    let slot = store.get(&source.id()).ok_or("source plugin closed")?;
    let mut markers = Vec::new();
    slot.visit_lazy_records(&["REFR"], |record| {
        if let Some(kind) = marker_type(record) {
            markers.push((record.form_id & 0xFFFFFF, kind));
        }
        ControlFlow::Continue(())
    })?;
    Ok(markers)
}
