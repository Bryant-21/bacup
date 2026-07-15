//! Fallout 3 / New Vegas split-NAVM to FO4 packed-NVNM bridge.
//!
//! Gamebryo Fallout stores NAVM geometry across NVER/DATA/NVVX/NVTR/NVCA/
//! NVDP/NVGD/NVEX. FO4 stores the graph in one NVNM v15 payload. The legacy
//! row layouts match Skyrim's v12 inputs, so this module assembles a v12-shaped
//! payload and delegates the proven v12 -> v15 structural conversion.

use std::collections::HashMap;

use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, effective_subrecords_for_record, plugin_handle_store_ref,
};

use crate::source_read::{SourceRecordSnapshot, raw_cell_is_interior};

#[derive(Debug, Default)]
pub(crate) struct LegacyFalloutNavmeshBatch {
    pub converted: HashMap<u32, Vec<u8>>,
    pub failures: HashMap<u32, String>,
}

#[derive(Clone, Copy, Debug)]
enum LegacyNavmeshParent {
    Interior { cell: u32 },
    Exterior { world: u32, x: i16, y: i16 },
}

pub(crate) fn prepare_legacy_fallout_navmeshes(
    source_handle_id: u64,
    records: &[SourceRecordSnapshot],
) -> Result<LegacyFalloutNavmeshBatch, String> {
    let parent_by_cell = {
        let store = plugin_handle_store_ref()
            .lock()
            .map_err(|error| format!("plugin handle store poisoned: {error}"))?;
        let slot = store
            .get(&source_handle_id)
            .ok_or_else(|| format!("unknown source plugin handle {source_handle_id}"))?;
        build_cell_parent_index(&slot.parsed.root_items)
    };

    let mut failures = HashMap::new();
    let mut assembled = Vec::with_capacity(records.len());
    for snapshot in records {
        match assemble_legacy_navmesh(&snapshot.raw_record, &parent_by_cell) {
            Ok(bytes) => assembled.push((snapshot.raw_record.form_id, bytes)),
            Err(error) => {
                failures.insert(snapshot.raw_record.form_id, error);
            }
        }
    }

    let entries = assembled
        .iter()
        .map(|(form_id, bytes)| (*form_id, bytes.as_slice()))
        .collect::<Vec<_>>();
    let converted = esp_authoring_core::nvnm::convert_skyrim_nvnm_set_to_fo4_lossy(&entries);
    let mut converted_by_form_id = HashMap::with_capacity(converted.converted.len());
    for row in converted.converted {
        converted_by_form_id.insert(row.form_id, row.bytes);
    }
    for failure in converted.failures {
        failures.insert(failure.form_id, failure.error.to_string());
    }
    Ok(LegacyFalloutNavmeshBatch {
        converted: converted_by_form_id,
        failures,
    })
}

fn build_cell_parent_index(items: &[ParsedItem]) -> HashMap<u32, LegacyNavmeshParent> {
    fn walk(
        items: &[ParsedItem],
        current_world: Option<u32>,
        out: &mut HashMap<u32, LegacyNavmeshParent>,
    ) {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.signature.as_str() == "CELL" => {
                    if raw_cell_is_interior(record) {
                        out.insert(
                            record.form_id,
                            LegacyNavmeshParent::Interior {
                                cell: record.form_id,
                            },
                        );
                    } else if let (Some(world), Some((x, y))) = (current_world, cell_grid(record)) {
                        out.insert(
                            record.form_id,
                            LegacyNavmeshParent::Exterior { world, x, y },
                        );
                    }
                }
                ParsedItem::Group(group) => {
                    let world = (group.group_type == 1)
                        .then(|| u32::from_le_bytes(group.label))
                        .or(current_world);
                    walk(&group.children, world, out);
                }
                _ => {}
            }
        }
    }
    let mut out = HashMap::new();
    walk(items, None, &mut out);
    out
}

fn cell_grid(record: &ParsedRecord) -> Option<(i16, i16)> {
    let subrecords = effective_subrecords_for_record(record);
    let xclc = subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "XCLC")?
        .data
        .as_ref();
    if xclc.len() < 8 {
        return None;
    }
    let x = i32::from_le_bytes(xclc[0..4].try_into().ok()?);
    let y = i32::from_le_bytes(xclc[4..8].try_into().ok()?);
    Some((i16::try_from(x).ok()?, i16::try_from(y).ok()?))
}

fn assemble_legacy_navmesh(
    record: &ParsedRecord,
    parent_by_cell: &HashMap<u32, LegacyNavmeshParent>,
) -> Result<Vec<u8>, String> {
    let subrecords = effective_subrecords_for_record(record);
    let nver = required_subrecord(&subrecords, "NVER", record.form_id)?;
    let version = read_u32(nver, 0)?;
    if version != 11 {
        return Err(format!(
            "legacy NAVM {:08X} NVER is {version}; expected 11",
            record.form_id
        ));
    }
    let data = required_subrecord(&subrecords, "DATA", record.form_id)?;
    if data.len() < 24 {
        return Err(format!(
            "legacy NAVM {:08X} DATA is {} bytes; expected 24",
            record.form_id,
            data.len()
        ));
    }
    let cell = read_u32(data, 0)?;
    let parent = parent_by_cell.get(&cell).copied().ok_or_else(|| {
        format!(
            "legacy NAVM {:08X} parent CELL {:08X} has no interior/world topology",
            record.form_id, cell
        )
    })?;
    let vertices = optional_subrecord(&subrecords, "NVVX");
    let triangles = optional_subrecord(&subrecords, "NVTR");
    let covers = optional_subrecord(&subrecords, "NVCA");
    let doors = optional_subrecord(&subrecords, "NVDP");
    let grid = optional_subrecord(&subrecords, "NVGD");
    let edges = optional_subrecord(&subrecords, "NVEX");

    validate_rows(record.form_id, "NVVX", vertices, 12, read_u32(data, 4)?)?;
    validate_rows(record.form_id, "NVTR", triangles, 16, read_u32(data, 8)?)?;
    validate_rows(record.form_id, "NVEX", edges, 10, read_u32(data, 12)?)?;
    validate_rows(record.form_id, "NVCA", covers, 2, read_u32(data, 16)?)?;
    validate_rows(record.form_id, "NVDP", doors, 8, read_u32(data, 20)?)?;

    let mut out = Vec::new();
    out.extend_from_slice(&12u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    match parent {
        LegacyNavmeshParent::Interior { cell } => {
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&cell.to_le_bytes());
        }
        LegacyNavmeshParent::Exterior { world, x, y } => {
            out.extend_from_slice(&world.to_le_bytes());
            out.extend_from_slice(&y.to_le_bytes());
            out.extend_from_slice(&x.to_le_bytes());
        }
    }
    push_counted_bytes(&mut out, vertices, 12)?;
    push_counted_bytes(&mut out, triangles, 16)?;
    push_counted_bytes(&mut out, edges, 10)?;
    push_legacy_doors(&mut out, doors)?;
    push_counted_bytes(&mut out, covers, 2)?;
    push_legacy_grid(&mut out, grid)?;
    Ok(out)
}

fn required_subrecord<'a>(
    subrecords: &'a [esp_authoring_core::plugin_runtime::ParsedSubrecord],
    signature: &str,
    form_id: u32,
) -> Result<&'a [u8], String> {
    subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == signature)
        .map(|subrecord| subrecord.data.as_ref())
        .ok_or_else(|| format!("legacy NAVM {form_id:08X} missing {signature}"))
}

fn optional_subrecord<'a>(
    subrecords: &'a [esp_authoring_core::plugin_runtime::ParsedSubrecord],
    signature: &str,
) -> &'a [u8] {
    subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == signature)
        .map(|subrecord| subrecord.data.as_ref())
        .unwrap_or_default()
}

fn validate_rows(
    form_id: u32,
    signature: &str,
    bytes: &[u8],
    stride: usize,
    declared: u32,
) -> Result<(), String> {
    if bytes.len() % stride != 0 || bytes.len() / stride != declared as usize {
        return Err(format!(
            "legacy NAVM {form_id:08X} {signature} rows={} bytes={} declared={declared}",
            bytes.len() / stride,
            bytes.len()
        ));
    }
    Ok(())
}

fn push_counted_bytes(out: &mut Vec<u8>, bytes: &[u8], stride: usize) -> Result<(), String> {
    if bytes.len() % stride != 0 {
        return Err(format!(
            "row bytes {} not divisible by {stride}",
            bytes.len()
        ));
    }
    let count = u32::try_from(bytes.len() / stride).map_err(|_| "row count exceeds u32")?;
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

fn push_legacy_doors(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() % 8 != 0 {
        return Err(format!(
            "NVDP byte count {} is not divisible by 8",
            bytes.len()
        ));
    }
    let count = u32::try_from(bytes.len() / 8).map_err(|_| "door count exceeds u32")?;
    out.extend_from_slice(&count.to_le_bytes());
    for row in bytes.chunks_exact(8) {
        out.extend_from_slice(&row[4..6]);
        out.extend_from_slice(&row[6..8]);
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&row[0..4]);
    }
    Ok(())
}

fn push_legacy_grid(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        out.extend_from_slice(&0u32.to_le_bytes());
        return Ok(());
    }
    if bytes.len() < 36 {
        return Err(format!(
            "NVGD is {} bytes; expected at least 36",
            bytes.len()
        ));
    }
    let divisor = read_u32(bytes, 0)? as usize;
    out.extend_from_slice(&(divisor as u32).to_le_bytes());
    if divisor == 0 {
        if bytes[4..].iter().any(|byte| *byte != 0) {
            return Err("NVGD divisor is zero but bounds payload is nonzero".to_string());
        }
        return Ok(());
    }
    out.extend_from_slice(&bytes[4..36]);
    let cell_count = divisor
        .checked_mul(divisor)
        .ok_or_else(|| "NVGD cell count overflow".to_string())?;
    let mut offset = 36usize;
    for _ in 0..cell_count {
        let count_bytes = bytes
            .get(offset..offset + 2)
            .ok_or_else(|| "NVGD missing cell triangle count".to_string())?;
        let count = u16::from_le_bytes(count_bytes.try_into().unwrap()) as usize;
        offset += 2;
        let end = offset
            .checked_add(count * 2)
            .ok_or_else(|| "NVGD cell triangle byte overflow".to_string())?;
        let rows = bytes
            .get(offset..end)
            .ok_or_else(|| "NVGD cell triangle rows truncated".to_string())?;
        out.extend_from_slice(&(count as u32).to_le_bytes());
        out.extend_from_slice(rows);
        offset = end;
    }
    if offset != bytes.len() {
        return Err(format!(
            "NVGD has {} trailing bytes",
            bytes.len().saturating_sub(offset)
        ));
    }
    Ok(())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let row = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("u32 at {offset} exceeds {} bytes", bytes.len()))?;
    Ok(u32::from_le_bytes(row.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use esp_authoring_core::nvnm::{NvnmParent, parse_nvnm};
    use smol_str::SmolStr;

    use super::*;

    fn subrecord(
        signature: &'static str,
        data: Vec<u8>,
    ) -> esp_authoring_core::plugin_runtime::ParsedSubrecord {
        esp_authoring_core::plugin_runtime::ParsedSubrecord {
            signature: SmolStr::new_static(signature),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn legacy_record(declared_vertices: u32) -> ParsedRecord {
        let mut data = Vec::new();
        for value in [0x000801, declared_vertices, 1, 1, 1, 1] {
            data.extend_from_slice(&value.to_le_bytes());
        }

        let mut vertices = Vec::new();
        for vertex in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for component in vertex {
                vertices.extend_from_slice(&component.to_le_bytes());
            }
        }

        let mut triangle = Vec::new();
        for vertex in [0_u16, 1, 2] {
            triangle.extend_from_slice(&vertex.to_le_bytes());
        }
        for link in [-1_i16; 3] {
            triangle.extend_from_slice(&link.to_le_bytes());
        }
        triangle.extend_from_slice(&0_u16.to_le_bytes());
        triangle.extend_from_slice(&0_u16.to_le_bytes());

        let mut edge = Vec::new();
        edge.extend_from_slice(&1_u32.to_le_bytes());
        edge.extend_from_slice(&0x000901_u32.to_le_bytes());
        edge.extend_from_slice(&0_u16.to_le_bytes());

        let mut door = Vec::new();
        door.extend_from_slice(&0x000A00_u32.to_le_bytes());
        door.extend_from_slice(&0_u16.to_le_bytes());
        door.extend_from_slice(&[0xAA, 0xBB]);

        let mut grid = Vec::new();
        grid.extend_from_slice(&1_u32.to_le_bytes());
        for value in [1.0_f32, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0] {
            grid.extend_from_slice(&value.to_le_bytes());
        }
        grid.extend_from_slice(&1_u16.to_le_bytes());
        grid.extend_from_slice(&0_u16.to_le_bytes());

        ParsedRecord {
            signature: SmolStr::new_static("NAVM"),
            form_id: 0x000900,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords: vec![
                subrecord("NVER", 11_u32.to_le_bytes().to_vec()),
                subrecord("DATA", data),
                subrecord("NVVX", vertices),
                subrecord("NVTR", triangle),
                subrecord("NVEX", edge),
                subrecord("NVDP", door),
                subrecord("NVCA", 0_u16.to_le_bytes().to_vec()),
                subrecord("NVGD", grid),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    #[test]
    fn assembles_all_legacy_rows_into_parseable_fo4_nvnm() {
        let parents = HashMap::from([(0x000801, LegacyNavmeshParent::Interior { cell: 0x000801 })]);
        let skyrim = assemble_legacy_navmesh(&legacy_record(3), &parents).expect("assemble");
        let converted = esp_authoring_core::nvnm::convert_skyrim_nvnm_set_to_fo4_lossy(&[(
            0x000900,
            skyrim.as_slice(),
        )]);
        assert!(converted.failures.is_empty());
        let converted = &converted.converted[0];
        assert_eq!(converted.report.edge_links_dropped, 1);
        let target = parse_nvnm(&converted.bytes).expect("parse FO4 NVNM");
        assert_eq!(target.version, 15);
        assert_eq!(target.parent, NvnmParent::Interior { cell: 0x000801 });
        assert_eq!(target.vertices.len(), 3);
        assert_eq!(target.triangles.len(), 1);
        assert!(target.edge_links.is_empty());
        assert_eq!(target.door_refs.len(), 1);
        assert_eq!(target.door_refs[0].door_ref_form_id, 0x000A00);
        assert_eq!(target.door_refs[0].padding, [0xAA, 0xBB, 0, 0]);
        assert_eq!(target.grid.divisor, 1);
        assert_eq!(target.grid.cells[0].triangle_indices, vec![0]);
    }

    #[test]
    fn rejects_declared_row_count_mismatch() {
        let parents = HashMap::from([(0x000801, LegacyNavmeshParent::Interior { cell: 0x000801 })]);
        let error = assemble_legacy_navmesh(&legacy_record(4), &parents)
            .expect_err("mismatched DATA count must fail");
        assert!(error.contains("NVVX"));
        assert!(error.contains("declared=4"));
    }
}
