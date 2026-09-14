use esp_authoring_core::nvnm::parse_nvnm;

const STARFIELD_NVNM_VERSION: u32 = 17;
const FO4_NVNM_VERSION: u32 = 15;
const STARFIELD_EMBEDDED_VERTEX_SIZE: usize = 16;
const STARFIELD_NAVM_VERTEX_SIZE: usize = 12;
const TRIANGLE_SIZE: usize = 21;
const EDGE_LINK_SIZE: usize = 11;
const DOOR_TRIANGLE_SIZE: usize = 10;
const STARFIELD_COVER_SIZE: usize = 12;
const FO4_COVER_SIZE: usize = 8;
const COVER_MAPPING_SIZE: usize = 4;
const WAYPOINT_SIZE: usize = 18;
const STARFIELD_TRAILER_SIZE: usize = 4;
const STARFIELD_TRAILER_EXTENSION_SIZE: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VertexLayout {
    Embedded16,
    Navm12,
    NoVertices,
}

pub(crate) fn convert_nvnm_to_fo4(bytes: &[u8], scale: f32) -> Result<Vec<u8>, String> {
    select_nvnm_layout(bytes, scale).map(|(output, _)| output)
}

fn select_nvnm_layout(bytes: &[u8], scale: f32) -> Result<(Vec<u8>, VertexLayout), String> {
    let embedded = convert_nvnm_layout(bytes, scale, STARFIELD_EMBEDDED_VERTEX_SIZE);
    let navm = convert_nvnm_layout(bytes, scale, STARFIELD_NAVM_VERTEX_SIZE);
    match (embedded, navm) {
        (Ok(embedded), Err(_)) => Ok((embedded, VertexLayout::Embedded16)),
        (Err(_), Ok(navm)) => Ok((navm, VertexLayout::Navm12)),
        (Ok(embedded), Ok(navm)) if embedded == navm => Ok((embedded, VertexLayout::NoVertices)),
        (Ok(_), Ok(_)) => Err(
            "ambiguous Starfield NVNM layout: both 12-byte and 16-byte vertex rows validate"
                .to_string(),
        ),
        (Err(embedded_error), Err(navm_error)) => Err(format!(
            "unsupported Starfield NVNM layout (16-byte vertices: {embedded_error}; \
             12-byte vertices: {navm_error})"
        )),
    }
}

fn convert_nvnm_layout(bytes: &[u8], scale: f32, vertex_size: usize) -> Result<Vec<u8>, String> {
    let mut input = Cursor::new(bytes);
    let version = input.u32("version")?;
    if version != STARFIELD_NVNM_VERSION {
        return Err(format!(
            "unsupported Starfield NVNM version {version} (expected {STARFIELD_NVNM_VERSION})"
        ));
    }

    let mut output = Vec::with_capacity(bytes.len());
    output.extend_from_slice(&FO4_NVNM_VERSION.to_le_bytes());
    output.extend_from_slice(input.take(12, "pathing cell")?);

    copy_count(&mut input, &mut output, "vertices")?;
    let vertex_count = last_u32(&output) as usize;
    for _ in 0..vertex_count {
        let row = input.take(vertex_size, "vertex")?;
        write_scaled_vec3(&mut output, row, scale);
    }

    let triangle_count = input.u32("triangle count")?;
    output.extend_from_slice(&triangle_count.to_le_bytes());
    for _ in 0..triangle_count {
        let row = input.take(TRIANGLE_SIZE, "triangle")?;
        output.extend_from_slice(&row[..12]);
        let height = f32::from_le_bytes(row[12..16].try_into().unwrap()) * scale;
        output.extend_from_slice(&height.to_le_bytes());
        output.extend_from_slice(&row[16..]);
    }
    copy_fixed_rows(&mut input, &mut output, EDGE_LINK_SIZE, "edge links")?;
    copy_fixed_rows(
        &mut input,
        &mut output,
        DOOR_TRIANGLE_SIZE,
        "door triangles",
    )?;

    let cover_count = input.u32("cover count")?;
    output.extend_from_slice(&cover_count.to_le_bytes());
    for _ in 0..cover_count {
        let row = input.take(STARFIELD_COVER_SIZE, "cover")?;
        output.extend_from_slice(&row[..FO4_COVER_SIZE]);
    }

    copy_fixed_rows(
        &mut input,
        &mut output,
        COVER_MAPPING_SIZE,
        "cover triangle mappings",
    )?;

    let waypoint_count = input.u32("waypoint count")?;
    output.extend_from_slice(&waypoint_count.to_le_bytes());
    for _ in 0..waypoint_count {
        let row = input.take(WAYPOINT_SIZE, "waypoint")?;
        write_scaled_vec3(&mut output, row, scale);
        output.extend_from_slice(&row[12..]);
    }

    let divisor = input.u32("grid divisor")?;
    if divisor > 12 {
        return Err(format!("grid divisor {divisor} exceeds engine maximum 12"));
    }
    output.extend_from_slice(&divisor.to_le_bytes());
    if divisor > 0 {
        let bounds = input.take(32, "grid bounds")?;
        for component in bounds.chunks_exact(4) {
            let value = f32::from_le_bytes(component.try_into().unwrap()) * scale;
            output.extend_from_slice(&value.to_le_bytes());
        }
        let cell_count = (divisor as usize) * (divisor as usize);
        for _ in 0..cell_count {
            let triangle_count = input.u32("grid cell triangle count")?;
            output.extend_from_slice(&triangle_count.to_le_bytes());
            let rows = (triangle_count as usize)
                .checked_mul(2)
                .ok_or_else(|| "grid cell triangle byte count overflow".to_string())?;
            output.extend_from_slice(input.take(rows, "grid cell triangles")?);
        }
    }

    input.take(STARFIELD_TRAILER_SIZE, "Starfield trailer")?;
    // Some CE2 embedded navmeshes append a 32-byte opaque extension after the
    // common four-byte trailer.  Neither trailer exists in FO4's v15 payload,
    // so consume the extension instead of rejecting otherwise complete
    // geometry.
    match input.remaining() {
        0 => {}
        STARFIELD_TRAILER_EXTENSION_SIZE => {
            input.take(
                STARFIELD_TRAILER_EXTENSION_SIZE,
                "Starfield trailer extension",
            )?;
        }
        remaining => {
            return Err(format!(
                "unsupported Starfield NVNM trailer extension length {remaining} \
                 (expected 0 or {STARFIELD_TRAILER_EXTENSION_SIZE})"
            ));
        }
    }
    parse_nvnm(&output)
        .map_err(|error| format!("converted FO4 NVNM failed validation: {error}"))?;
    Ok(output)
}

fn copy_count(
    input: &mut Cursor<'_>,
    output: &mut Vec<u8>,
    label: &'static str,
) -> Result<(), String> {
    let count = input.u32(label)?;
    output.extend_from_slice(&count.to_le_bytes());
    Ok(())
}

fn copy_fixed_rows(
    input: &mut Cursor<'_>,
    output: &mut Vec<u8>,
    row_size: usize,
    label: &'static str,
) -> Result<(), String> {
    let count = input.u32(label)?;
    output.extend_from_slice(&count.to_le_bytes());
    let byte_count = (count as usize)
        .checked_mul(row_size)
        .ok_or_else(|| format!("{label} byte count overflow"))?;
    output.extend_from_slice(input.take(byte_count, label)?);
    Ok(())
}

fn last_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[bytes.len() - 4..].try_into().unwrap())
}

fn write_scaled_vec3(output: &mut Vec<u8>, row: &[u8], scale: f32) {
    for component in row[..12].chunks_exact(4) {
        let value = f32::from_le_bytes(component.try_into().unwrap()) * scale;
        output.extend_from_slice(&value.to_le_bytes());
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, count: usize, label: &'static str) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| format!("{label} offset overflow"))?;
        if end > self.bytes.len() {
            return Err(format!(
                "truncated {label} at offset {} (need {count}, have {})",
                self.offset,
                self.remaining()
            ));
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn u32(&mut self, label: &'static str) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4, label)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use esp_authoring_core::plugin_runtime::{ParsedItem, parse_plugin_file};

    fn starfield_fixture() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&17_u32.to_le_bytes());
        bytes.extend_from_slice(&0xA5E9_A03C_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0x25_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        for value in [1.0_f32, -2.0, 3.0, 99.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_i16.to_le_bytes());
        bytes.extend_from_slice(&0_i16.to_le_bytes());
        bytes.extend_from_slice(&0_i16.to_le_bytes());
        bytes.extend_from_slice(&(-1_i16).to_le_bytes());
        bytes.extend_from_slice(&(-1_i16).to_le_bytes());
        bytes.extend_from_slice(&(-1_i16).to_le_bytes());
        bytes.extend_from_slice(&2.0_f32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0, 0]);
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&[1, 0, 2, 0, 3, 4, 5, 6, 7, 8, 9, 10]);
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        for value in [4.0_f32, 5.0, 6.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&(-1_i16).to_le_bytes());
        bytes.extend_from_slice(&7_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes
    }

    fn starfield_navm_fixture() -> Vec<u8> {
        let embedded = starfield_fixture();
        let mut navm = Vec::with_capacity(embedded.len() - 4);
        navm.extend_from_slice(&embedded[..20]);
        navm.extend_from_slice(&embedded[20..32]);
        navm.extend_from_slice(&embedded[36..]);
        navm
    }

    #[test]
    fn converts_ce2_rows_to_fo4_and_scales_coordinates() {
        let converted = convert_nvnm_to_fo4(&starfield_fixture(), 10.0).unwrap();
        let parsed = parse_nvnm(&converted).unwrap();

        assert_eq!(parsed.version, 15);
        assert_eq!(
            (
                parsed.vertices[0].x,
                parsed.vertices[0].y,
                parsed.vertices[0].z
            ),
            (10.0, -20.0, 30.0)
        );
        assert_eq!(parsed.cover_array.len(), 1);
        assert_eq!(parsed.cover_array[0].data_byte_4, 6);
        assert_eq!(
            f32::from_le_bytes(parsed.triangles[0].cover_marker[..4].try_into().unwrap()),
            20.0
        );
        assert_eq!(
            (
                parsed.waypoints[0].x,
                parsed.waypoints[0].y,
                parsed.waypoints[0].z
            ),
            (40.0, 50.0, 60.0)
        );
    }

    #[test]
    fn rejects_truncated_payload_instead_of_emitting_crash_data() {
        let mut fixture = starfield_fixture();
        fixture.truncate(fixture.len() - 5);
        assert!(convert_nvnm_to_fo4(&fixture, 10.0).is_err());
    }

    #[test]
    fn ignores_opaque_ce2_trailer_extension() {
        let mut fixture = starfield_fixture();
        fixture.extend_from_slice(&[0xA5; 32]);

        let converted = convert_nvnm_to_fo4(&fixture, 10.0).unwrap();

        assert_eq!(parse_nvnm(&converted).unwrap().version, 15);
    }

    #[test]
    fn rejects_unrecognized_ce2_trailer_extension_lengths() {
        for length in [1, 4, 31, 33] {
            let mut fixture = starfield_fixture();
            fixture.extend(std::iter::repeat_n(0xA5, length));

            let error = convert_nvnm_to_fo4(&fixture, 10.0).unwrap_err();

            assert!(
                error.contains("trailer extension length"),
                "length {length}: {error}"
            );
        }
    }

    #[test]
    fn converts_standalone_navm_xyz_vertex_rows() {
        let converted = convert_nvnm_to_fo4(&starfield_navm_fixture(), 10.0).unwrap();
        let parsed = parse_nvnm(&converted).unwrap();

        assert_eq!(parsed.version, 15);
        assert_eq!(
            (
                parsed.vertices[0].x,
                parsed.vertices[0].y,
                parsed.vertices[0].z
            ),
            (10.0, -20.0, 30.0)
        );
        assert_eq!(parsed.triangles.len(), 1);
    }

    #[test]
    fn live_starfield_nvnm_payloads_convert_when_source_is_available() {
        let Some(data_dir) = std::env::var_os("STARFIELD_DATA_DIR") else {
            return;
        };
        let plugin_path = std::path::Path::new(&data_dir).join("Starfield.esm");
        if !plugin_path.exists() {
            return;
        }
        let plugin = parse_plugin_file(
            &plugin_path.to_string_lossy(),
            Some("starfield".to_owned()),
            true,
        )
        .expect("parse live Starfield.esm");
        let mut failures = Vec::new();
        let mut counts = LayoutCounts::default();
        collect_conversion_results(&plugin.root_items, &mut counts, &mut failures);
        assert!(
            failures.is_empty(),
            "live Starfield NVNM conversion failures:\n{}",
            failures.join("\n")
        );
        assert!(
            counts.visited > 0,
            "live Starfield corpus contained no NVNM payloads"
        );
        assert!(
            counts.embedded_16 > 0,
            "no 16-byte NVNM payloads were exercised: {counts:?}"
        );
        assert!(
            counts.navm_12 > 0,
            "no 12-byte NVNM payloads were exercised: {counts:?}"
        );
        eprintln!("live Starfield NVNM layout coverage: {counts:?}");
    }

    #[derive(Debug, Default)]
    struct LayoutCounts {
        visited: usize,
        embedded_16: usize,
        navm_12: usize,
        no_vertices: usize,
    }

    fn collect_conversion_results(
        items: &[ParsedItem],
        counts: &mut LayoutCounts,
        failures: &mut Vec<String>,
    ) {
        for item in items {
            match item {
                ParsedItem::Group(group) => {
                    collect_conversion_results(&group.children, counts, failures);
                }
                ParsedItem::Record(record) => {
                    for subrecord in &record.subrecords {
                        if subrecord.signature.as_str() != "NVNM" {
                            continue;
                        }
                        counts.visited += 1;
                        match select_nvnm_layout(&subrecord.data, 69.99125) {
                            Ok((_, VertexLayout::Embedded16)) => counts.embedded_16 += 1,
                            Ok((_, VertexLayout::Navm12)) => counts.navm_12 += 1,
                            Ok((_, VertexLayout::NoVertices)) => counts.no_vertices += 1,
                            Err(error) => failures.push(format!(
                                "{}:{:06X}:{} bytes: {error}",
                                record.signature,
                                record.form_id,
                                subrecord.data.len(),
                            )),
                        }
                    }
                }
            }
        }
    }
}
