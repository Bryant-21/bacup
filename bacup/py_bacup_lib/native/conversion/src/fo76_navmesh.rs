//! FO76-specific NAVM/NVNM conversion helpers.
//!
//! FO76 wraps `NVNM` in a record-size union: a zero-byte marker variant or the
//! normal navmesh geometry payload. FO4 only defines the geometry payload. The
//! converter keeps complex struct payloads as raw bytes, so this module rewrites
//! the embedded FormIDs in-place and drops FO76 marker payloads before FO4 write.

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use esp_authoring_core::plugin_runtime::plugin_handle_store_ref;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

const NVNM: [u8; 4] = *b"NVNM";
const MNAM: [u8; 4] = *b"MNAM";
const CELL: SigCode = SigCode(*b"CELL");
const NAVM: SigCode = SigCode(*b"NAVM");
const REFR: SigCode = SigCode(*b"REFR");
const WRLD: SigCode = SigCode(*b"WRLD");
const NAVMESH_GEN_CELL_LOCAL: u32 = 0x000025;
// FO4 CK validates projected navmesh area more strictly than FO76 data does.
const NAVMESH_VERTEX_WELD_EPSILON: f32 = 0.03125;
const NAVMESH_MIN_PROJECTED_TRIANGLE_AREA: f32 = 1.0;
const TRIANGLE_ROW_SIZE: usize = 21;
const TRIANGLE_LINKS_OFFSET: usize = 6;
const TRIANGLE_FLAGS_OFFSET: usize = 17;
const TRIANGLE_EDGE_EXTRA_INFO_FLAGS: [u16; 3] = [0x0001, 0x0002, 0x0004];

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct NvnmRewriteReport {
    pub fields_dropped: u32,
    pub formids_rewritten: u32,
    pub mnam_fields_dropped: u32,
    pub mnam_formids_rewritten: u32,
}

#[derive(Debug)]
pub(crate) enum NvnmRewriteError {
    MissingHandle(u64),
    MalformedPayload(String),
}

impl std::fmt::Display for NvnmRewriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingHandle(handle_id) => write!(f, "missing plugin handle {handle_id}"),
            Self::MalformedPayload(message) => write!(f, "malformed navmesh payload: {message}"),
        }
    }
}

impl std::error::Error for NvnmRewriteError {}

#[derive(Clone, Debug)]
pub(crate) struct FormIdContext {
    plugin_name: String,
    masters: Vec<String>,
}

impl FormIdContext {
    #[cfg(test)]
    fn new(plugin_name: &str, masters: &[&str]) -> Self {
        Self {
            plugin_name: plugin_name.to_string(),
            masters: masters.iter().map(|master| master.to_string()).collect(),
        }
    }
}

pub(crate) fn rewrite_record_nvnm_for_fo4(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
    source_handle_id: u64,
    target_handle_id: u64,
) -> Result<NvnmRewriteReport, NvnmRewriteError> {
    if !record
        .fields
        .iter()
        .any(|entry| entry.sig.0 == NVNM || entry.sig.0 == MNAM)
    {
        return Ok(NvnmRewriteReport::default());
    }

    let source = snapshot_formid_context(source_handle_id)?;
    let target = snapshot_formid_context(target_handle_id)?;
    rewrite_record_nvnm_with_context(record, mapper, &source, &target)
}

pub(crate) fn raw_formid_mappings_for_context(
    source_to_target: impl IntoIterator<Item = (FormKey, FormKey)>,
    interner: &StringInterner,
    source_handle_id: u64,
    target_handle_id: u64,
) -> Result<Vec<(u32, u32)>, NvnmRewriteError> {
    let source = snapshot_formid_context(source_handle_id)?;
    let target = snapshot_formid_context(target_handle_id)?;
    Ok(source_to_target
        .into_iter()
        .filter_map(|(source_fk, target_fk)| {
            let source_raw = target_form_id(source_fk, &source, interner);
            let target_raw = target_form_id(target_fk, &target, interner);
            (source_raw != 0 && target_raw != 0).then_some((source_raw, target_raw))
        })
        .collect())
}

pub(crate) fn rewrite_record_nvnm_with_context(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
    source: &FormIdContext,
    target: &FormIdContext,
) -> Result<NvnmRewriteReport, NvnmRewriteError> {
    let mut report = NvnmRewriteReport::default();
    let mut retained: SmallVec<[FieldEntry; 8]> = SmallVec::with_capacity(record.fields.len());
    let mut triangle_count = None;
    let mut triangle_remap = None;

    for mut entry in record.fields.drain(..) {
        if entry.sig.0 != NVNM {
            retained.push(entry);
            continue;
        }

        let FieldValue::Bytes(bytes) = &mut entry.value else {
            retained.push(entry);
            continue;
        };

        if bytes.is_empty() {
            report.fields_dropped += 1;
            continue;
        }

        let geometry_report = rewrite_geometry_nvnm(bytes, mapper, source, target)?;
        report.formids_rewritten += geometry_report.formids_rewritten;
        triangle_count = Some(geometry_report.triangle_count);
        triangle_remap = geometry_report.triangle_remap;
        retained.push(entry);
    }

    let mut final_fields: SmallVec<[FieldEntry; 8]> = SmallVec::with_capacity(retained.len());
    for mut entry in retained.drain(..) {
        if entry.sig.0 != MNAM {
            final_fields.push(entry);
            continue;
        }

        let mut bytes = match std::mem::replace(&mut entry.value, FieldValue::None) {
            FieldValue::Bytes(bytes) => bytes,
            value => {
                entry.value = value;
                final_fields.push(entry);
                continue;
            }
        };

        match rewrite_precut_mnam(
            &mut bytes,
            mapper,
            source,
            target,
            triangle_count,
            triangle_remap.as_deref(),
        ) {
            Ok(rewritten) => {
                report.mnam_formids_rewritten += rewritten;
                entry.value = FieldValue::Bytes(bytes);
                final_fields.push(entry);
            }
            Err(error) => {
                report.mnam_fields_dropped += 1;
                let warning = mapper
                    .interner
                    .intern(&format!("fo76_mnam_dropped:{error}"));
                record.warnings.push(warning);
            }
        }
    }

    record.fields = final_fields;
    Ok(report)
}

pub(crate) fn snapshot_formid_context(handle_id: u64) -> Result<FormIdContext, NvnmRewriteError> {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store
        .get_mut(&handle_id)
        .ok_or(NvnmRewriteError::MissingHandle(handle_id))?;
    Ok(FormIdContext {
        plugin_name: slot.parsed.plugin_name.clone(),
        masters: slot.parsed.header.masters.clone(),
    })
}

struct GeometryRewriteReport {
    formids_rewritten: u32,
    triangle_count: usize,
    triangle_remap: Option<Vec<Option<usize>>>,
}

fn rewrite_geometry_nvnm(
    data: &mut SmallVec<[u8; 32]>,
    mapper: &mut FormKeyMapper<'_>,
    source: &FormIdContext,
    target: &FormIdContext,
) -> Result<GeometryRewriteReport, NvnmRewriteError> {
    if data.len() < 16 {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "geometry payload is {} bytes; expected at least 16",
            data.len()
        )));
    }

    let mut rewritten = 0_u32;
    let payload = data.as_mut_slice();
    let parent_world = read_u32(payload, 8, "pathing cell parent world")?;
    rewritten += rewrite_formid_at(payload, 8, WRLD, mapper, source, target)?;
    if parent_world == 0 {
        rewritten += rewrite_parent_cell_at(payload, 12, mapper, source, target)?;
    }

    let cleanup_report = cleanup_navmesh_geometry(data)?;

    let payload = data.as_mut_slice();
    let mut offset = 16_usize;
    skip_counted_rows(payload, &mut offset, 12, "vertices")?;
    let triangle_table = normalize_triangle_edge_slots(payload, &mut offset)?;
    let (edge_rewrites, edge_link_count) =
        rewrite_edge_links(payload, &mut offset, mapper, source, target)?;
    rewritten += edge_rewrites;
    clear_invalid_triangle_extra_info_flags(payload, triangle_table, edge_link_count)?;
    rewritten += rewrite_door_links(payload, &mut offset, mapper, source, target)?;
    skip_counted_rows(payload, &mut offset, 8, "cover array")?;
    skip_counted_rows(payload, &mut offset, 4, "cover triangle mappings")?;
    skip_counted_rows(payload, &mut offset, 18, "waypoints")?;

    Ok(GeometryRewriteReport {
        formids_rewritten: rewritten,
        triangle_count: cleanup_report.triangle_count,
        triangle_remap: cleanup_report.triangle_remap,
    })
}

#[derive(Clone, Copy)]
struct NvnmVertex {
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Clone)]
struct NvnmTriangleRow {
    vertices: [u16; 3],
    links: [i16; 3],
    tail: [u8; 9],
}

impl NvnmTriangleRow {
    fn flags(&self) -> u16 {
        u16::from_le_bytes([self.tail[5], self.tail[6]])
    }

    fn set_flags(&mut self, flags: u16) {
        self.tail[5..7].copy_from_slice(&flags.to_le_bytes());
    }

    fn has_edge_extra_info(&self, slot: usize) -> bool {
        self.flags() & triangle_edge_extra_info_flag(slot) != 0
    }

    fn clear_edge_extra_info(&mut self, slot: usize) {
        let flags = self.flags() & !triangle_edge_extra_info_flag(slot);
        self.set_flags(flags);
    }
}

struct NvnmGrid {
    fixed: [u8; 36],
    cells: Vec<Vec<i16>>,
}

struct NvnmGeometry {
    header: [u8; 16],
    vertices: Vec<NvnmVertex>,
    triangles: Vec<NvnmTriangleRow>,
    edge_links: Vec<[u8; 11]>,
    door_links: Vec<[u8; 10]>,
    cover_array: Vec<[u8; 8]>,
    cover_triangle_mappings: Vec<[u8; 4]>,
    waypoints: Vec<[u8; 18]>,
    grid: Option<NvnmGrid>,
    trailing: Vec<u8>,
}

struct NvnmCleanupReport {
    triangle_count: usize,
    triangle_remap: Option<Vec<Option<usize>>>,
}

impl NvnmGeometry {
    fn parse(data: &[u8]) -> Result<Self, NvnmRewriteError> {
        if data.len() < 16 {
            return Err(NvnmRewriteError::MalformedPayload(format!(
                "geometry payload is {} bytes; expected at least 16",
                data.len()
            )));
        }

        let mut offset = 16_usize;
        let vertices = read_nvnm_vertices(data, &mut offset)?;
        let triangles = read_nvnm_triangles(data, &mut offset)?;
        let edge_links = read_fixed_rows::<11>(data, &mut offset, "edge links")?;
        let door_links = read_fixed_rows::<10>(data, &mut offset, "door links")?;
        let cover_array = read_fixed_rows::<8>(data, &mut offset, "cover array")?;
        let cover_triangle_mappings =
            read_fixed_rows::<4>(data, &mut offset, "cover triangle mappings")?;
        let waypoints = read_fixed_rows::<18>(data, &mut offset, "waypoints")?;
        let grid = if offset < data.len() && data.len() - offset >= 36 {
            Some(read_nvnm_grid(data, &mut offset)?)
        } else {
            None
        };
        let trailing = data[offset..].to_vec();

        Ok(Self {
            header: data[0..16].try_into().unwrap(),
            vertices,
            triangles,
            edge_links,
            door_links,
            cover_array,
            cover_triangle_mappings,
            waypoints,
            grid,
            trailing,
        })
    }

    fn encode(&self) -> Result<Vec<u8>, NvnmRewriteError> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.header);
        append_vertices(&mut out, &self.vertices)?;
        append_triangles(&mut out, &self.triangles)?;
        append_fixed_rows(&mut out, &self.edge_links)?;
        append_fixed_rows(&mut out, &self.door_links)?;
        append_fixed_rows(&mut out, &self.cover_array)?;
        append_fixed_rows(&mut out, &self.cover_triangle_mappings)?;
        append_fixed_rows(&mut out, &self.waypoints)?;
        if let Some(grid) = &self.grid {
            out.extend_from_slice(&grid.fixed);
            for cell in &grid.cells {
                push_count(&mut out, cell.len(), "navmesh grid cell")?;
                for triangle in cell {
                    out.extend_from_slice(&triangle.to_le_bytes());
                }
            }
        }
        out.extend_from_slice(&self.trailing);
        Ok(out)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct VertexBucket(i32, i32, i32);

fn cleanup_navmesh_geometry(
    data: &mut SmallVec<[u8; 32]>,
) -> Result<NvnmCleanupReport, NvnmRewriteError> {
    let mut geometry = NvnmGeometry::parse(data.as_slice())?;
    let vertex_remap = build_vertex_weld_remap(&geometry.vertices)?;
    let mut vertices_changed = false;
    for triangle in &mut geometry.triangles {
        for vertex in &mut triangle.vertices {
            let Some(&remapped) = vertex_remap.get(*vertex as usize) else {
                continue;
            };
            if *vertex != remapped {
                *vertex = remapped;
                vertices_changed = true;
            }
        }
    }
    let mut winding_changed = false;
    for triangle in &mut geometry.triangles {
        if has_downfacing_projected_normal(triangle, &geometry.vertices) {
            flip_triangle_winding(triangle);
            winding_changed = true;
        }
    }

    let original_triangles = geometry.triangles.clone();
    let overlap_removals = find_overlap_sliver_removals(&original_triangles, &geometry.vertices);
    let mut triangle_remap = vec![None; original_triangles.len()];
    let mut kept_old_indices = Vec::new();
    let mut kept_triangles = Vec::new();
    for (old_index, triangle) in original_triangles.iter().enumerate() {
        if overlap_removals[old_index] || is_degenerate_triangle(triangle, &geometry.vertices) {
            continue;
        }
        triangle_remap[old_index] = Some(kept_triangles.len());
        kept_old_indices.push(old_index);
        kept_triangles.push(triangle.clone());
    }

    if kept_triangles.len() == original_triangles.len() && !vertices_changed {
        if winding_changed {
            *data = SmallVec::from_vec(geometry.encode()?);
        }
        return Ok(NvnmCleanupReport {
            triangle_count: original_triangles.len(),
            triangle_remap: None,
        });
    }

    remap_cover_vertices(&mut geometry.cover_array, &vertex_remap);
    remap_triangle_ref_rows(&mut geometry.door_links, 0, &triangle_remap)?;
    remap_triangle_ref_rows(&mut geometry.cover_triangle_mappings, 2, &triangle_remap)?;
    remap_triangle_ref_rows(&mut geometry.waypoints, 12, &triangle_remap)?;
    if let Some(grid) = &mut geometry.grid {
        remap_grid_cells(grid, &triangle_remap)?;
    }
    rebuild_triangle_links(
        &mut kept_triangles,
        &original_triangles,
        &kept_old_indices,
        geometry.edge_links.len(),
    )?;

    geometry.triangles = kept_triangles;
    let triangle_count = geometry.triangles.len();
    *data = SmallVec::from_vec(geometry.encode()?);
    Ok(NvnmCleanupReport {
        triangle_count,
        triangle_remap: Some(triangle_remap),
    })
}

fn read_nvnm_vertices(
    data: &[u8],
    offset: &mut usize,
) -> Result<Vec<NvnmVertex>, NvnmRewriteError> {
    let count = read_count(data, offset, "vertices")?;
    let rows_start = *offset;
    let rows_end = checked_rows_end(rows_start, count, 12, data.len(), "vertices")?;
    let mut vertices = Vec::with_capacity(count);
    for index in 0..count {
        let row = rows_start + index * 12;
        vertices.push(NvnmVertex {
            x: read_f32(data, row, "vertex x")?,
            y: read_f32(data, row + 4, "vertex y")?,
            z: read_f32(data, row + 8, "vertex z")?,
        });
    }
    *offset = rows_end;
    Ok(vertices)
}

fn read_nvnm_triangles(
    data: &[u8],
    offset: &mut usize,
) -> Result<Vec<NvnmTriangleRow>, NvnmRewriteError> {
    let count = read_count(data, offset, "triangles")?;
    let rows_start = *offset;
    let rows_end = checked_rows_end(rows_start, count, 21, data.len(), "triangles")?;
    let mut triangles = Vec::with_capacity(count);
    for index in 0..count {
        let row = rows_start + index * 21;
        triangles.push(NvnmTriangleRow {
            vertices: [
                read_u16(data, row, "triangle vertex 0")?,
                read_u16(data, row + 2, "triangle vertex 1")?,
                read_u16(data, row + 4, "triangle vertex 2")?,
            ],
            links: [
                read_i16(data, row + 6, "triangle edge 0")?,
                read_i16(data, row + 8, "triangle edge 1")?,
                read_i16(data, row + 10, "triangle edge 2")?,
            ],
            tail: data[row + 12..row + 21].try_into().unwrap(),
        });
    }
    *offset = rows_end;
    Ok(triangles)
}

fn read_fixed_rows<const N: usize>(
    data: &[u8],
    offset: &mut usize,
    label: &'static str,
) -> Result<Vec<[u8; N]>, NvnmRewriteError> {
    let count = read_count(data, offset, label)?;
    let rows_start = *offset;
    let rows_end = checked_rows_end(rows_start, count, N, data.len(), label)?;
    let mut rows = Vec::with_capacity(count);
    for index in 0..count {
        let row = rows_start + index * N;
        rows.push(data[row..row + N].try_into().unwrap());
    }
    *offset = rows_end;
    Ok(rows)
}

fn read_nvnm_grid(data: &[u8], offset: &mut usize) -> Result<NvnmGrid, NvnmRewriteError> {
    let fixed_start = *offset;
    let fixed_end = fixed_start + 36;
    if fixed_end > data.len() {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "navmesh grid fixed block exceeds payload: offset={fixed_start} len={}",
            data.len()
        )));
    }
    let fixed: [u8; 36] = data[fixed_start..fixed_end].try_into().unwrap();
    let divisor = u32::from_le_bytes(fixed[0..4].try_into().unwrap()) as usize;
    let cell_count = divisor.checked_mul(divisor).ok_or_else(|| {
        NvnmRewriteError::MalformedPayload("navmesh grid cell count overflow".into())
    })?;
    *offset = fixed_end;

    let mut cells = Vec::with_capacity(cell_count);
    for _ in 0..cell_count {
        let triangle_count = read_count(data, offset, "navmesh grid cell")?;
        let rows_start = *offset;
        let rows_end = checked_rows_end(
            rows_start,
            triangle_count,
            2,
            data.len(),
            "navmesh grid cell",
        )?;
        let mut cell = Vec::with_capacity(triangle_count);
        for index in 0..triangle_count {
            cell.push(read_i16(
                data,
                rows_start + index * 2,
                "navmesh grid cell triangle",
            )?);
        }
        *offset = rows_end;
        cells.push(cell);
    }

    Ok(NvnmGrid { fixed, cells })
}

fn append_vertices(out: &mut Vec<u8>, vertices: &[NvnmVertex]) -> Result<(), NvnmRewriteError> {
    push_count(out, vertices.len(), "vertices")?;
    for vertex in vertices {
        out.extend_from_slice(&vertex.x.to_le_bytes());
        out.extend_from_slice(&vertex.y.to_le_bytes());
        out.extend_from_slice(&vertex.z.to_le_bytes());
    }
    Ok(())
}

fn append_triangles(
    out: &mut Vec<u8>,
    triangles: &[NvnmTriangleRow],
) -> Result<(), NvnmRewriteError> {
    push_count(out, triangles.len(), "triangles")?;
    for triangle in triangles {
        for vertex in triangle.vertices {
            out.extend_from_slice(&vertex.to_le_bytes());
        }
        for link in triangle.links {
            out.extend_from_slice(&link.to_le_bytes());
        }
        out.extend_from_slice(&triangle.tail);
    }
    Ok(())
}

fn append_fixed_rows<const N: usize>(
    out: &mut Vec<u8>,
    rows: &[[u8; N]],
) -> Result<(), NvnmRewriteError> {
    push_count(out, rows.len(), "row array")?;
    for row in rows {
        out.extend_from_slice(row);
    }
    Ok(())
}

fn push_count(
    out: &mut Vec<u8>,
    count: usize,
    label: &'static str,
) -> Result<(), NvnmRewriteError> {
    let count = u32::try_from(count)
        .map_err(|_| NvnmRewriteError::MalformedPayload(format!("{label} count exceeds u32")))?;
    out.extend_from_slice(&count.to_le_bytes());
    Ok(())
}

fn build_vertex_weld_remap(vertices: &[NvnmVertex]) -> Result<Vec<u16>, NvnmRewriteError> {
    let mut buckets: FxHashMap<VertexBucket, Vec<usize>> = FxHashMap::default();
    let mut remap = Vec::with_capacity(vertices.len());

    for (index, vertex) in vertices.iter().enumerate() {
        let index_u16 = u16::try_from(index).map_err(|_| {
            NvnmRewriteError::MalformedPayload("NVNM vertex index exceeds u16".into())
        })?;
        let Some(bucket) = vertex_bucket(*vertex) else {
            remap.push(index_u16);
            continue;
        };

        let mut representative = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let neighbor = VertexBucket(bucket.0 + dx, bucket.1 + dy, bucket.2 + dz);
                    let Some(candidates) = buckets.get(&neighbor) else {
                        continue;
                    };
                    representative = candidates.iter().copied().find(|candidate| {
                        vertices_within_weld_epsilon(*vertex, vertices[*candidate])
                    });
                    if representative.is_some() {
                        break;
                    }
                }
                if representative.is_some() {
                    break;
                }
            }
            if representative.is_some() {
                break;
            }
        }

        if let Some(representative) = representative {
            remap.push(u16::try_from(representative).map_err(|_| {
                NvnmRewriteError::MalformedPayload("NVNM vertex index exceeds u16".into())
            })?);
        } else {
            remap.push(index_u16);
            buckets.entry(bucket).or_default().push(index);
        }
    }

    Ok(remap)
}

fn vertex_bucket(vertex: NvnmVertex) -> Option<VertexBucket> {
    if !(vertex.x.is_finite() && vertex.y.is_finite() && vertex.z.is_finite()) {
        return None;
    }
    Some(VertexBucket(
        (vertex.x / NAVMESH_VERTEX_WELD_EPSILON).floor() as i32,
        (vertex.y / NAVMESH_VERTEX_WELD_EPSILON).floor() as i32,
        (vertex.z / NAVMESH_VERTEX_WELD_EPSILON).floor() as i32,
    ))
}

fn vertices_within_weld_epsilon(left: NvnmVertex, right: NvnmVertex) -> bool {
    (left.x - right.x).abs() <= NAVMESH_VERTEX_WELD_EPSILON
        && (left.y - right.y).abs() <= NAVMESH_VERTEX_WELD_EPSILON
        && (left.z - right.z).abs() <= NAVMESH_VERTEX_WELD_EPSILON
}

fn is_degenerate_triangle(triangle: &NvnmTriangleRow, vertices: &[NvnmVertex]) -> bool {
    let [a, b, c] = triangle.vertices;
    if a == b || b == c || c == a {
        return true;
    }
    let Some(a) = vertices.get(a as usize) else {
        return true;
    };
    let Some(b) = vertices.get(b as usize) else {
        return true;
    };
    let Some(c) = vertices.get(c as usize) else {
        return true;
    };
    projected_triangle_area(*a, *b, *c) < NAVMESH_MIN_PROJECTED_TRIANGLE_AREA
}

fn has_downfacing_projected_normal(triangle: &NvnmTriangleRow, vertices: &[NvnmVertex]) -> bool {
    let [a, b, c] = triangle.vertices;
    let (Some(a), Some(b), Some(c)) = (
        vertices.get(a as usize),
        vertices.get(b as usize),
        vertices.get(c as usize),
    ) else {
        return false;
    };
    projected_triangle_cross_z(*a, *b, *c) < 0.0
}

fn flip_triangle_winding(triangle: &mut NvnmTriangleRow) {
    triangle.vertices.swap(1, 2);
    triangle.links = [triangle.links[2], triangle.links[1], triangle.links[0]];

    let flags = triangle.flags();
    let mut remapped_flags = flags & !0x0007;
    if flags & triangle_edge_extra_info_flag(0) != 0 {
        remapped_flags |= triangle_edge_extra_info_flag(2);
    }
    if flags & triangle_edge_extra_info_flag(1) != 0 {
        remapped_flags |= triangle_edge_extra_info_flag(1);
    }
    if flags & triangle_edge_extra_info_flag(2) != 0 {
        remapped_flags |= triangle_edge_extra_info_flag(0);
    }
    triangle.set_flags(remapped_flags);
}

fn projected_triangle_area(a: NvnmVertex, b: NvnmVertex, c: NvnmVertex) -> f32 {
    (projected_triangle_cross_z(a, b, c).abs() * 0.5) as f32
}

/// Mark overlapping "sliver" triangles for removal. Two upfacing triangles that
/// emit the SAME oriented edge (after the winding flip) sit on the same side of
/// that edge — they overlap in projection (FO76 zigzag/sliver fans). Creation
/// Kit derives triangle adjacency from shared-edge geometry on Finalize, links
/// such a pair, and then logs "opposite normals but linked / edges should be
/// linked but are not / vertices do not match". CK does NOT repair this, so the
/// overlap has to be gone before export. Greedily drop the smallest-area
/// triangle from each same-oriented-edge conflict until none remain; the FO76
/// fans always have one small central sliver wedged between two larger,
/// non-overlapping neighbours, so this culls exactly the redundant triangle.
fn find_overlap_sliver_removals(
    triangles: &[NvnmTriangleRow],
    vertices: &[NvnmVertex],
) -> Vec<bool> {
    let mut removed = vec![false; triangles.len()];
    loop {
        let mut edge_owners: FxHashMap<[u16; 2], SmallVec<[usize; 2]>> = FxHashMap::default();
        for (index, triangle) in triangles.iter().enumerate() {
            if removed[index] || is_degenerate_triangle(triangle, vertices) {
                continue;
            }
            for slot in 0..3 {
                edge_owners
                    .entry(triangle_edge_vertices(triangle.vertices, slot))
                    .or_default()
                    .push(index);
            }
        }

        let mut victim: Option<usize> = None;
        let mut victim_area = f32::INFINITY;
        for owners in edge_owners.values() {
            if owners.len() < 2 {
                continue;
            }
            for &index in owners {
                let area = triangle_projected_area(triangles[index].vertices, vertices);
                if area < victim_area {
                    victim_area = area;
                    victim = Some(index);
                }
            }
        }

        match victim {
            Some(index) => removed[index] = true,
            None => return removed,
        }
    }
}

fn triangle_projected_area(indices: [u16; 3], vertices: &[NvnmVertex]) -> f32 {
    let (Some(a), Some(b), Some(c)) = (
        vertices.get(indices[0] as usize),
        vertices.get(indices[1] as usize),
        vertices.get(indices[2] as usize),
    ) else {
        return f32::INFINITY;
    };
    projected_triangle_area(*a, *b, *c)
}

fn projected_triangle_cross_z(a: NvnmVertex, b: NvnmVertex, c: NvnmVertex) -> f64 {
    let ab_x = (b.x - a.x) as f64;
    let ab_y = (b.y - a.y) as f64;
    let ac_x = (c.x - a.x) as f64;
    let ac_y = (c.y - a.y) as f64;
    ab_x * ac_y - ab_y * ac_x
}

fn remap_cover_vertices(rows: &mut [[u8; 8]], vertex_remap: &[u16]) {
    for row in rows {
        for offset in [0_usize, 2] {
            let old = u16::from_le_bytes(row[offset..offset + 2].try_into().unwrap());
            if let Some(&new) = vertex_remap.get(old as usize) {
                row[offset..offset + 2].copy_from_slice(&new.to_le_bytes());
            }
        }
    }
}

fn remap_triangle_ref_rows<const N: usize>(
    rows: &mut Vec<[u8; N]>,
    triangle_offset: usize,
    triangle_remap: &[Option<usize>],
) -> Result<(), NvnmRewriteError> {
    let mut retained = Vec::with_capacity(rows.len());
    for mut row in rows.drain(..) {
        let old = i16::from_le_bytes(
            row[triangle_offset..triangle_offset + 2]
                .try_into()
                .unwrap(),
        );
        let Some(new) = remap_triangle_ref(old, triangle_remap)? else {
            continue;
        };
        row[triangle_offset..triangle_offset + 2].copy_from_slice(&new.to_le_bytes());
        retained.push(row);
    }
    *rows = retained;
    Ok(())
}

fn remap_grid_cells(
    grid: &mut NvnmGrid,
    triangle_remap: &[Option<usize>],
) -> Result<(), NvnmRewriteError> {
    for cell in &mut grid.cells {
        let mut retained = Vec::with_capacity(cell.len());
        for triangle in cell.drain(..) {
            if let Some(new) = remap_triangle_ref(triangle, triangle_remap)? {
                retained.push(new);
            }
        }
        *cell = retained;
    }
    Ok(())
}

fn remap_triangle_ref(
    old: i16,
    triangle_remap: &[Option<usize>],
) -> Result<Option<i16>, NvnmRewriteError> {
    if old < 0 {
        return Ok(Some(old));
    }
    let Some(Some(new)) = triangle_remap.get(old as usize) else {
        return Ok(None);
    };
    Ok(Some(checked_i16_triangle_index(*new)?))
}

fn rebuild_triangle_links(
    triangles: &mut [NvnmTriangleRow],
    original_triangles: &[NvnmTriangleRow],
    kept_old_indices: &[usize],
    edge_link_count: usize,
) -> Result<(), NvnmRewriteError> {
    let mut external_links = vec![[None; 3]; triangles.len()];
    for (new_index, &old_index) in kept_old_indices.iter().enumerate() {
        for slot in 0..3 {
            if is_external_edge_link(old_index, slot, original_triangles, edge_link_count) {
                external_links[new_index][slot] = Some(original_triangles[old_index].links[slot]);
            }
        }
    }

    for (index, triangle) in triangles.iter_mut().enumerate() {
        for slot in 0..3 {
            if external_links[index][slot].is_none() {
                triangle.clear_edge_extra_info(slot);
            }
        }
        triangle.links = [-1; 3];
    }

    let mut edge_map: FxHashMap<[u16; 2], SmallVec<[(usize, usize); 2]>> = FxHashMap::default();
    for (triangle_index, triangle) in triangles.iter().enumerate() {
        for slot in 0..3 {
            edge_map
                .entry(edge_key(triangle_edge_vertices(triangle.vertices, slot)))
                .or_default()
                .push((triangle_index, slot));
        }
    }

    for entries in edge_map.values() {
        if entries.len() != 2 {
            continue;
        }
        let (left_triangle, left_slot) = entries[0];
        let (right_triangle, right_slot) = entries[1];
        triangles[left_triangle].links[left_slot] = checked_i16_triangle_index(right_triangle)?;
        triangles[right_triangle].links[right_slot] = checked_i16_triangle_index(left_triangle)?;
    }

    for (triangle_index, slots) in external_links.into_iter().enumerate() {
        for (slot, external_link) in slots.into_iter().enumerate() {
            if triangles[triangle_index].links[slot] < 0 {
                if let Some(external_link) = external_link {
                    triangles[triangle_index].links[slot] = external_link;
                }
            }
        }
    }

    Ok(())
}

fn is_external_edge_link(
    triangle_index: usize,
    slot: usize,
    triangles: &[NvnmTriangleRow],
    edge_link_count: usize,
) -> bool {
    if !triangles[triangle_index].has_edge_extra_info(slot) {
        return false;
    }
    let value = triangles[triangle_index].links[slot];
    value >= 0 && (value as usize) < edge_link_count
}

fn edge_key(edge: [u16; 2]) -> [u16; 2] {
    if edge[0] <= edge[1] {
        edge
    } else {
        [edge[1], edge[0]]
    }
}

fn checked_i16_triangle_index(index: usize) -> Result<i16, NvnmRewriteError> {
    i16::try_from(index)
        .map_err(|_| NvnmRewriteError::MalformedPayload("NVNM triangle index exceeds i16".into()))
}

#[derive(Clone, Copy)]
struct TriangleEdgeSlots {
    vertices: [u16; 3],
    links: [i16; 3],
    flags: u16,
    row_offset: usize,
}

#[derive(Clone, Copy)]
struct TriangleTable {
    rows_start: usize,
    count: usize,
}

fn normalize_triangle_edge_slots(
    data: &mut [u8],
    offset: &mut usize,
) -> Result<TriangleTable, NvnmRewriteError> {
    let count = read_count(data, offset, "triangles")?;
    let rows_start = *offset;
    let rows_end = checked_rows_end(
        rows_start,
        count,
        TRIANGLE_ROW_SIZE,
        data.len(),
        "triangles",
    )?;
    let mut triangles = Vec::with_capacity(count);
    for index in 0..count {
        let row = rows_start + index * TRIANGLE_ROW_SIZE;
        triangles.push(TriangleEdgeSlots {
            vertices: [
                read_u16(data, row, "triangle vertex 0")?,
                read_u16(data, row + 2, "triangle vertex 1")?,
                read_u16(data, row + 4, "triangle vertex 2")?,
            ],
            links: [
                read_i16(data, row + 6, "triangle edge 0")?,
                read_i16(data, row + 8, "triangle edge 1")?,
                read_i16(data, row + 10, "triangle edge 2")?,
            ],
            flags: read_u16(data, row + TRIANGLE_FLAGS_OFFSET, "triangle flags")?,
            row_offset: row,
        });
    }

    for index in 0..triangles.len() {
        let original = triangles[index].links;
        let mut normalized = original;
        let mut moves = smallvec::SmallVec::<[(usize, usize, i16); 3]>::new();
        let mut stable_slots = [false; 3];

        for slot in 0..3 {
            if triangles[index].flags & triangle_edge_extra_info_flag(slot) != 0 {
                continue;
            }
            let linked_index = original[slot];
            if linked_index < 0 {
                continue;
            }
            let linked_index = linked_index as usize;
            if linked_index >= triangles.len() {
                continue;
            }
            let linked = triangles[linked_index];
            if !linked.links.iter().any(|value| *value == index as i16) {
                continue;
            }
            let Some(actual_slot) = shared_edge_slot(triangles[index].vertices, linked.vertices)
            else {
                continue;
            };
            if actual_slot == slot {
                stable_slots[slot] = true;
                continue;
            }
            if original[actual_slot] == original[slot] {
                continue;
            }
            moves.push((slot, actual_slot, original[slot]));
        }

        if moves.is_empty() {
            continue;
        }

        let moved_from = moves.iter().fold([false; 3], |mut slots, (from, _, _)| {
            slots[*from] = true;
            slots
        });
        for (from, _, _) in moves.iter().copied() {
            normalized[from] = -1;
        }
        for (_, to, linked_index) in moves.iter().copied() {
            if stable_slots[to] {
                continue;
            }
            if original[to] >= 0 && !moved_from[to] {
                continue;
            }
            normalized[to] = linked_index;
        }

        for slot in 0..3 {
            if normalized[slot] != original[slot] {
                write_i16(
                    data,
                    triangles[index].row_offset + TRIANGLE_LINKS_OFFSET + slot * 2,
                    normalized[slot],
                    "triangle edge",
                )?;
            }
        }
        triangles[index].links = normalized;
    }

    *offset = rows_end;
    Ok(TriangleTable { rows_start, count })
}

fn clear_invalid_triangle_extra_info_flags(
    data: &mut [u8],
    triangle_table: TriangleTable,
    edge_link_count: usize,
) -> Result<(), NvnmRewriteError> {
    for index in 0..triangle_table.count {
        let row = triangle_table.rows_start + index * TRIANGLE_ROW_SIZE;
        let mut flags = read_u16(data, row + TRIANGLE_FLAGS_OFFSET, "triangle flags")?;
        let original_flags = flags;

        for (slot, bit) in TRIANGLE_EDGE_EXTRA_INFO_FLAGS.into_iter().enumerate() {
            if flags & bit == 0 {
                continue;
            }
            let link = read_i16(
                data,
                row + TRIANGLE_LINKS_OFFSET + slot * 2,
                "triangle edge",
            )?;
            if link < 0 || link as usize >= edge_link_count {
                flags &= !bit;
            }
        }

        if flags != original_flags {
            write_u16(data, row + TRIANGLE_FLAGS_OFFSET, flags, "triangle flags")?;
        }
    }
    Ok(())
}

fn triangle_edge_extra_info_flag(slot: usize) -> u16 {
    TRIANGLE_EDGE_EXTRA_INFO_FLAGS[slot]
}

fn shared_edge_slot(current: [u16; 3], linked: [u16; 3]) -> Option<usize> {
    let mut slot = None;
    for current_slot in 0..3 {
        let current_edge = triangle_edge_vertices(current, current_slot);
        for linked_slot in 0..3 {
            if unordered_edge_eq(current_edge, triangle_edge_vertices(linked, linked_slot)) {
                if slot.is_some() {
                    return None;
                }
                slot = Some(current_slot);
            }
        }
    }
    slot
}

fn triangle_edge_vertices(vertices: [u16; 3], slot: usize) -> [u16; 2] {
    match slot {
        0 => [vertices[0], vertices[1]],
        1 => [vertices[1], vertices[2]],
        2 => [vertices[2], vertices[0]],
        _ => unreachable!("triangle edge slot is always 0..3"),
    }
}

fn unordered_edge_eq(left: [u16; 2], right: [u16; 2]) -> bool {
    (left[0] == right[0] && left[1] == right[1]) || (left[0] == right[1] && left[1] == right[0])
}

fn rewrite_edge_links(
    data: &mut [u8],
    offset: &mut usize,
    mapper: &mut FormKeyMapper<'_>,
    source: &FormIdContext,
    target: &FormIdContext,
) -> Result<(u32, usize), NvnmRewriteError> {
    let count = read_count(data, offset, "edge links")?;
    let rows_start = *offset;
    let rows_end = checked_rows_end(rows_start, count, 11, data.len(), "edge links")?;
    let mut rewritten = 0_u32;
    for index in 0..count {
        let row = rows_start + index * 11;
        rewritten += rewrite_formid_at(data, row + 4, NAVM, mapper, source, target)?;
    }
    *offset = rows_end;
    Ok((rewritten, count))
}

fn rewrite_door_links(
    data: &mut [u8],
    offset: &mut usize,
    mapper: &mut FormKeyMapper<'_>,
    source: &FormIdContext,
    target: &FormIdContext,
) -> Result<u32, NvnmRewriteError> {
    let count = read_count(data, offset, "door links")?;
    let rows_start = *offset;
    let rows_end = checked_rows_end(rows_start, count, 10, data.len(), "door links")?;
    let mut rewritten = 0_u32;
    for index in 0..count {
        let row = rows_start + index * 10;
        rewritten += rewrite_formid_at(data, row + 6, REFR, mapper, source, target)?;
    }
    *offset = rows_end;
    Ok(rewritten)
}

fn rewrite_parent_cell_at(
    data: &mut [u8],
    offset: usize,
    mapper: &mut FormKeyMapper<'_>,
    source: &FormIdContext,
    target: &FormIdContext,
) -> Result<u32, NvnmRewriteError> {
    let raw = read_u32(data, offset, "parent cell")?;
    if raw == NAVMESH_GEN_CELL_LOCAL {
        let fallout4 = mapper.interner.intern("Fallout4.esm");
        let target_raw = target_form_id(
            FormKey {
                local: NAVMESH_GEN_CELL_LOCAL,
                plugin: fallout4,
            },
            target,
            mapper.interner,
        );
        write_u32(data, offset, target_raw, "parent cell")?;
        return Ok(u32::from(raw != target_raw));
    }
    rewrite_formid_at(data, offset, CELL, mapper, source, target)
}

fn skip_counted_rows(
    data: &[u8],
    offset: &mut usize,
    row_size: usize,
    label: &'static str,
) -> Result<(), NvnmRewriteError> {
    let count = read_count(data, offset, label)?;
    *offset = checked_rows_end(*offset, count, row_size, data.len(), label)?;
    Ok(())
}

fn read_count(
    data: &[u8],
    offset: &mut usize,
    label: &'static str,
) -> Result<usize, NvnmRewriteError> {
    let count = read_u32(data, *offset, label)? as usize;
    *offset += 4;
    Ok(count)
}

fn checked_rows_end(
    offset: usize,
    count: usize,
    row_size: usize,
    len: usize,
    label: &'static str,
) -> Result<usize, NvnmRewriteError> {
    let bytes = count.checked_mul(row_size).ok_or_else(|| {
        NvnmRewriteError::MalformedPayload(format!("{label} row byte count overflow"))
    })?;
    let end = offset
        .checked_add(bytes)
        .ok_or_else(|| NvnmRewriteError::MalformedPayload(format!("{label} row end overflow")))?;
    if end > len {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "{label} rows exceed payload: offset={offset} count={count} row_size={row_size} len={len}"
        )));
    }
    Ok(end)
}

fn rewrite_precut_mnam(
    data: &mut SmallVec<[u8; 32]>,
    mapper: &mut FormKeyMapper<'_>,
    source: &FormIdContext,
    target: &FormIdContext,
    triangle_count: Option<usize>,
    triangle_remap: Option<&[Option<usize>]>,
) -> Result<u32, NvnmRewriteError> {
    let mut offset = 0_usize;
    let mut rewritten = 0_u32;
    let mut out = Vec::with_capacity(data.len());

    while offset < data.len() {
        if offset + 6 > data.len() {
            return Err(NvnmRewriteError::MalformedPayload(format!(
                "MNAM entry header exceeds payload: offset={offset} len={}",
                data.len()
            )));
        }

        let raw_ref = read_u32(data, offset, "MNAM reference")?;
        let (target_ref, changed) = mapped_raw_formid(raw_ref, REFR, mapper, source, target)?;
        rewritten += u32::from(changed);

        let count = read_u16(data, offset + 4, "MNAM triangle count")? as usize;
        let triangles_start = offset + 6;
        let triangles_end =
            checked_rows_end(triangles_start, count, 2, data.len(), "MNAM triangle list")?;

        let mut triangles = Vec::with_capacity(count);
        for index in 0..count {
            let old_triangle =
                read_u16(data, triangles_start + index * 2, "MNAM triangle index")? as usize;
            let Some(new_triangle) =
                remap_mnam_triangle(old_triangle, triangle_count, triangle_remap)
            else {
                continue;
            };
            let new_triangle = u16::try_from(new_triangle).map_err(|_| {
                NvnmRewriteError::MalformedPayload("MNAM triangle index exceeds u16".into())
            })?;
            triangles.push(new_triangle);
        }

        let new_count = u16::try_from(triangles.len()).map_err(|_| {
            NvnmRewriteError::MalformedPayload("MNAM triangle count exceeds u16".into())
        })?;
        out.extend_from_slice(&target_ref.to_le_bytes());
        out.extend_from_slice(&new_count.to_le_bytes());
        for triangle in triangles {
            out.extend_from_slice(&triangle.to_le_bytes());
        }

        offset = triangles_end;
    }

    *data = SmallVec::from_vec(out);
    Ok(rewritten)
}

fn remap_mnam_triangle(
    old_triangle: usize,
    triangle_count: Option<usize>,
    triangle_remap: Option<&[Option<usize>]>,
) -> Option<usize> {
    if let Some(remap) = triangle_remap {
        return remap.get(old_triangle).and_then(|value| *value);
    }
    if let Some(count) = triangle_count {
        return (old_triangle < count).then_some(old_triangle);
    }
    Some(old_triangle)
}

fn rewrite_formid_at(
    data: &mut [u8],
    offset: usize,
    expected_sig: SigCode,
    mapper: &mut FormKeyMapper<'_>,
    source: &FormIdContext,
    target: &FormIdContext,
) -> Result<u32, NvnmRewriteError> {
    let raw = read_u32(data, offset, expected_sig.as_str())?;
    let (target_raw, changed) = mapped_raw_formid(raw, expected_sig, mapper, source, target)?;
    write_u32(data, offset, target_raw, expected_sig.as_str())?;
    Ok(u32::from(changed))
}

fn mapped_raw_formid(
    raw: u32,
    expected_sig: SigCode,
    mapper: &mut FormKeyMapper<'_>,
    source: &FormIdContext,
    target: &FormIdContext,
) -> Result<(u32, bool), NvnmRewriteError> {
    if raw == 0 {
        return Ok((0, false));
    }
    let Some(source_fk) = source_form_key(raw, source, mapper.interner) else {
        return Ok((raw, false));
    };
    let target_fk = mapper
        .lookup(source_fk)
        .unwrap_or_else(|| mapper.allocate_or_resolve(source_fk, None, expected_sig));
    let target_raw = target_form_id(target_fk, target, mapper.interner);
    Ok((target_raw, raw != target_raw))
}

fn read_u16(data: &[u8], offset: usize, label: &str) -> Result<u16, NvnmRewriteError> {
    let end = offset + 2;
    if end > data.len() {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "{label} u16 at offset {offset} exceeds len {}",
            data.len()
        )));
    }
    Ok(u16::from_le_bytes(data[offset..end].try_into().unwrap()))
}

fn read_i16(data: &[u8], offset: usize, label: &str) -> Result<i16, NvnmRewriteError> {
    let end = offset + 2;
    if end > data.len() {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "{label} i16 at offset {offset} exceeds len {}",
            data.len()
        )));
    }
    Ok(i16::from_le_bytes(data[offset..end].try_into().unwrap()))
}

fn read_f32(data: &[u8], offset: usize, label: &str) -> Result<f32, NvnmRewriteError> {
    let end = offset + 4;
    if end > data.len() {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "{label} f32 at offset {offset} exceeds len {}",
            data.len()
        )));
    }
    Ok(f32::from_le_bytes(data[offset..end].try_into().unwrap()))
}

fn source_form_key(raw: u32, source: &FormIdContext, interner: &StringInterner) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let master_index = ((raw >> 24) & 0xFF) as usize;
    let object_id = raw & 0x00FF_FFFF;
    let plugin = source
        .masters
        .get(master_index)
        .map(String::as_str)
        .unwrap_or(source.plugin_name.as_str());
    Some(FormKey {
        local: object_id,
        plugin: interner.intern(plugin),
    })
}

pub(crate) fn target_form_id(
    fk: FormKey,
    target: &FormIdContext,
    interner: &StringInterner,
) -> u32 {
    let object_id = fk.local & 0x00FF_FFFF;
    if object_id == 0 {
        return 0;
    }
    let plugin_name = interner.resolve(fk.plugin).unwrap_or_default();
    let master_index = target
        .masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin_name))
        .unwrap_or(target.masters.len()) as u32;
    (master_index << 24) | object_id
}

fn read_u32(data: &[u8], offset: usize, label: &str) -> Result<u32, NvnmRewriteError> {
    let end = offset + 4;
    if end > data.len() {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "{label} u32 at offset {offset} exceeds len {}",
            data.len()
        )));
    }
    Ok(u32::from_le_bytes(data[offset..end].try_into().unwrap()))
}

fn write_u32(
    data: &mut [u8],
    offset: usize,
    value: u32,
    label: &str,
) -> Result<(), NvnmRewriteError> {
    let end = offset + 4;
    if end > data.len() {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "{label} u32 at offset {offset} exceeds len {}",
            data.len()
        )));
    }
    data[offset..end].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_i16(
    data: &mut [u8],
    offset: usize,
    value: i16,
    label: &str,
) -> Result<(), NvnmRewriteError> {
    let end = offset + 2;
    if end > data.len() {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "{label} i16 at offset {offset} exceeds len {}",
            data.len()
        )));
    }
    data[offset..end].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u16(
    data: &mut [u8],
    offset: usize,
    value: u16,
    label: &str,
) -> Result<(), NvnmRewriteError> {
    let end = offset + 2;
    if end > data.len() {
        return Err(NvnmRewriteError::MalformedPayload(format!(
            "{label} u16 at offset {offset} exceeds len {}",
            data.len()
        )));
    }
    data[offset..end].copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::SubrecordSig;
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn make_record(nvnm: Vec<u8>, interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("STAT").unwrap(),
            FormKey::parse("000800@SeventySix.esm", interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NVNM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(nvnm)),
        });
        record
    }

    fn make_navm_record(nvnm: Vec<u8>, mnam: Vec<u8>, interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("NAVM").unwrap(),
            FormKey::parse("000800@SeventySix.esm", interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NVNM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(nvnm)),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(mnam)),
        });
        record
    }

    fn mapper<'a>(interner: &'a StringInterner) -> FormKeyMapper<'a> {
        FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esp".to_string(),
                preserve_source_ids: true,
                ..Default::default()
            },
            interner,
        )
    }

    fn raw(raw: u32) -> [u8; 4] {
        raw.to_le_bytes()
    }

    fn zero_count(data: &mut Vec<u8>) {
        data.extend_from_slice(&0_u32.to_le_bytes());
    }

    fn triangle_row(vertices: [u16; 3]) -> [u8; 21] {
        let mut row = [0_u8; 21];
        row[0..2].copy_from_slice(&vertices[0].to_le_bytes());
        row[2..4].copy_from_slice(&vertices[1].to_le_bytes());
        row[4..6].copy_from_slice(&vertices[2].to_le_bytes());
        row[6..8].copy_from_slice(&(-1_i16).to_le_bytes());
        row[8..10].copy_from_slice(&(-1_i16).to_le_bytes());
        row[10..12].copy_from_slice(&(-1_i16).to_le_bytes());
        row
    }

    fn geometry_with_two_triangles() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&raw(0x000903));
        data.extend_from_slice(&4_u32.to_le_bytes());
        for (x, y, z) in [
            (0.0_f32, 0.0_f32, 0.0_f32),
            (10.0, 0.0, 0.0),
            (0.0, 10.0, 0.0),
            (10.0, 10.0, 0.0),
        ] {
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
            data.extend_from_slice(&z.to_le_bytes());
        }
        data.extend_from_slice(&2_u32.to_le_bytes());
        data.extend_from_slice(&triangle_row([0, 1, 2]));
        data.extend_from_slice(&triangle_row([1, 3, 2]));
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        data
    }

    fn geometry_with_overlapping_sliver() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&raw(0x000903));
        data.extend_from_slice(&4_u32.to_le_bytes());
        for (x, y, z) in [
            (0.0_f32, 0.0_f32, 0.0_f32),
            (10.0, 0.0, 0.0),
            (0.0, 10.0, 0.0),
            (2.0, 1.0, 0.0),
        ] {
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
            data.extend_from_slice(&z.to_le_bytes());
        }
        // Both triangles emit oriented edge (0,1); their third vertices (v2, v3)
        // are on the same side of it, so they overlap in projection. v3's
        // triangle is the smaller sliver and must be the one culled.
        data.extend_from_slice(&2_u32.to_le_bytes());
        data.extend_from_slice(&triangle_row([0, 1, 2]));
        data.extend_from_slice(&triangle_row([0, 1, 3]));
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        data
    }

    fn geometry_with_degenerate_then_valid_triangle() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&raw(0x000903));
        data.extend_from_slice(&3_u32.to_le_bytes());
        for (x, y, z) in [
            (0.0_f32, 0.0_f32, 0.0_f32),
            (10.0, 0.0, 0.0),
            (0.0, 10.0, 0.0),
        ] {
            data.extend_from_slice(&x.to_le_bytes());
            data.extend_from_slice(&y.to_le_bytes());
            data.extend_from_slice(&z.to_le_bytes());
        }
        data.extend_from_slice(&2_u32.to_le_bytes());
        data.extend_from_slice(&triangle_row([0, 0, 1]));
        data.extend_from_slice(&triangle_row([0, 1, 2]));
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        zero_count(&mut data);
        data
    }

    fn mnam_entry(reference: u32, triangles: &[u16]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&reference.to_le_bytes());
        data.extend_from_slice(&(triangles.len() as u16).to_le_bytes());
        for triangle in triangles {
            data.extend_from_slice(&triangle.to_le_bytes());
        }
        data
    }

    fn push_vertex(data: &mut Vec<u8>, x: f32, y: f32, z: f32) {
        data.extend_from_slice(&x.to_le_bytes());
        data.extend_from_slice(&y.to_le_bytes());
        data.extend_from_slice(&z.to_le_bytes());
    }

    fn push_triangle(data: &mut Vec<u8>, vertices: [u16; 3], links: [i16; 3]) {
        push_triangle_with_flags(data, vertices, links, 0);
    }

    fn push_triangle_with_flags(
        data: &mut Vec<u8>,
        vertices: [u16; 3],
        links: [i16; 3],
        flags: u16,
    ) {
        for vertex in vertices {
            data.extend_from_slice(&vertex.to_le_bytes());
        }
        for link in links {
            data.extend_from_slice(&link.to_le_bytes());
        }
        data.extend_from_slice(&0.0_f32.to_le_bytes());
        data.push(0);
        data.extend_from_slice(&flags.to_le_bytes());
        data.extend_from_slice(&0_u16.to_le_bytes());
    }

    fn geometry_with_parent_world_edge_and_door() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes()); // version
        data.extend_from_slice(&0_u32.to_le_bytes()); // pathing cell crc
        data.extend_from_slice(&raw(0x000900)); // parent world
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid y
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid x
        zero_count(&mut data); // vertices
        zero_count(&mut data); // triangles
        data.extend_from_slice(&1_u32.to_le_bytes()); // edge links
        data.extend_from_slice(&0_u32.to_le_bytes()); // edge type
        data.extend_from_slice(&raw(0x000901)); // linked navmesh
        data.extend_from_slice(&3_i16.to_le_bytes()); // triangle
        data.push(2); // edge index
        data.extend_from_slice(&1_u32.to_le_bytes()); // door links
        data.extend_from_slice(&4_u16.to_le_bytes()); // triangle
        data.extend_from_slice(&0_u32.to_le_bytes()); // crc
        data.extend_from_slice(&raw(0x000902)); // door ref
        zero_count(&mut data); // cover array
        zero_count(&mut data); // cover triangle mappings
        zero_count(&mut data); // waypoints
        data
    }

    fn geometry_with_mis_slotted_internal_link() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes()); // version
        data.extend_from_slice(&0_u32.to_le_bytes()); // pathing cell crc
        data.extend_from_slice(&raw(0x000900)); // parent world
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid y
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid x
        data.extend_from_slice(&4_u32.to_le_bytes()); // vertices
        push_vertex(&mut data, 0.0, 0.0, 0.0);
        push_vertex(&mut data, 10.0, 0.0, 0.0);
        push_vertex(&mut data, 0.0, 10.0, 0.0);
        push_vertex(&mut data, 0.0, -10.0, 0.0);
        data.extend_from_slice(&2_u32.to_le_bytes()); // triangles
        push_triangle(&mut data, [0, 1, 2], [-1, 1, -1]);
        push_triangle(&mut data, [3, 0, 1], [-1, 0, -1]);
        zero_count(&mut data); // edge links
        zero_count(&mut data); // door links
        zero_count(&mut data); // cover array
        zero_count(&mut data); // cover triangle mappings
        zero_count(&mut data); // waypoints
        data
    }

    fn geometry_with_duplicate_external_edge_index() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes()); // version
        data.extend_from_slice(&0_u32.to_le_bytes()); // pathing cell crc
        data.extend_from_slice(&raw(0x000900)); // parent world
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid y
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid x
        data.extend_from_slice(&4_u32.to_le_bytes()); // vertices
        push_vertex(&mut data, 0.0, 0.0, 0.0);
        push_vertex(&mut data, 10.0, 0.0, 0.0);
        push_vertex(&mut data, 0.0, 10.0, 0.0);
        push_vertex(&mut data, 0.0, -10.0, 0.0);
        data.extend_from_slice(&2_u32.to_le_bytes()); // triangles
        push_triangle(&mut data, [0, 1, 2], [1, 1, -1]);
        push_triangle(&mut data, [3, 0, 1], [-1, 0, -1]);
        zero_count(&mut data); // edge links
        zero_count(&mut data); // door links
        zero_count(&mut data); // cover array
        zero_count(&mut data); // cover triangle mappings
        zero_count(&mut data); // waypoints
        data
    }

    fn geometry_with_invalid_extra_info_flag() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes()); // version
        data.extend_from_slice(&0_u32.to_le_bytes()); // pathing cell crc
        data.extend_from_slice(&raw(0x000900)); // parent world
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid y
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid x
        data.extend_from_slice(&3_u32.to_le_bytes()); // vertices
        push_vertex(&mut data, 0.0, 0.0, 0.0);
        push_vertex(&mut data, 10.0, 0.0, 0.0);
        push_vertex(&mut data, 0.0, 10.0, 0.0);
        data.extend_from_slice(&1_u32.to_le_bytes()); // triangles
        push_triangle_with_flags(&mut data, [0, 1, 2], [-1, -1, -1], 0x0004);
        zero_count(&mut data); // edge links
        zero_count(&mut data); // door links
        zero_count(&mut data); // cover array
        zero_count(&mut data); // cover triangle mappings
        zero_count(&mut data); // waypoints
        data
    }

    fn geometry_with_downfacing_external_edge() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes()); // version
        data.extend_from_slice(&0_u32.to_le_bytes()); // pathing cell crc
        data.extend_from_slice(&raw(0x000900)); // parent world
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid y
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid x
        data.extend_from_slice(&3_u32.to_le_bytes()); // vertices
        push_vertex(&mut data, 0.0, 0.0, 0.0);
        push_vertex(&mut data, 10.0, 0.0, 0.0);
        push_vertex(&mut data, 0.0, 10.0, 0.0);
        data.extend_from_slice(&1_u32.to_le_bytes()); // triangles
        push_triangle_with_flags(
            &mut data,
            [0, 2, 1],
            [0, -1, -1],
            triangle_edge_extra_info_flag(0),
        );
        data.extend_from_slice(&1_u32.to_le_bytes()); // edge links
        data.extend_from_slice(&[0; 11]);
        zero_count(&mut data); // door links
        zero_count(&mut data); // cover array
        zero_count(&mut data); // cover triangle mappings
        zero_count(&mut data); // waypoints
        data
    }

    fn geometry_with_welded_sliver_refs() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes()); // version
        data.extend_from_slice(&0_u32.to_le_bytes()); // pathing cell crc
        data.extend_from_slice(&raw(0x000900)); // parent world
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid y
        data.extend_from_slice(&0_i16.to_le_bytes()); // parent grid x
        data.extend_from_slice(&5_u32.to_le_bytes()); // vertices
        push_vertex(&mut data, 0.0, 0.0, 0.0);
        push_vertex(&mut data, 10.0, 0.0, 0.0);
        push_vertex(&mut data, 0.0, 10.0, 0.0);
        push_vertex(&mut data, 10.0, 10.0, 0.0);
        push_vertex(&mut data, 10.01, 0.0, 0.0);
        data.extend_from_slice(&3_u32.to_le_bytes()); // triangles
        push_triangle(&mut data, [0, 1, 2], [-1, 1, -1]);
        push_triangle(&mut data, [1, 4, 2], [2, -1, 0]);
        push_triangle_with_flags(&mut data, [4, 3, 2], [2, -1, 1], 0x0001);
        data.extend_from_slice(&3_u32.to_le_bytes()); // edge links
        data.extend_from_slice(&[0; 33]);
        data.extend_from_slice(&2_u32.to_le_bytes()); // door links
        data.extend_from_slice(&1_i16.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&2_i16.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&0_u32.to_le_bytes());
        data.extend_from_slice(&1_u32.to_le_bytes()); // cover array
        data.extend_from_slice(&4_u16.to_le_bytes());
        data.extend_from_slice(&2_u16.to_le_bytes());
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(&2_u32.to_le_bytes()); // cover triangle mappings
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(&1_i16.to_le_bytes());
        data.extend_from_slice(&0_u16.to_le_bytes());
        data.extend_from_slice(&2_i16.to_le_bytes());
        data.extend_from_slice(&2_u32.to_le_bytes()); // waypoints
        for triangle in [1_i16, 2] {
            data.extend_from_slice(&0.0_f32.to_le_bytes());
            data.extend_from_slice(&0.0_f32.to_le_bytes());
            data.extend_from_slice(&0.0_f32.to_le_bytes());
            data.extend_from_slice(&triangle.to_le_bytes());
            data.extend_from_slice(&0_u32.to_le_bytes());
        }
        data.extend_from_slice(&1_u32.to_le_bytes()); // navmesh grid divisor
        for value in [1.0_f32, 1.0, 0.0, 0.0, 0.0, 10.0, 10.0, 0.0] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.extend_from_slice(&3_u32.to_le_bytes()); // one grid cell
        data.extend_from_slice(&0_i16.to_le_bytes());
        data.extend_from_slice(&1_i16.to_le_bytes());
        data.extend_from_slice(&2_i16.to_le_bytes());
        data
    }

    fn geometry_with_parent_cell() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes()); // version
        data.extend_from_slice(&0_u32.to_le_bytes()); // pathing cell crc
        data.extend_from_slice(&0_u32.to_le_bytes()); // no parent world
        data.extend_from_slice(&raw(0x000903)); // parent cell
        zero_count(&mut data); // vertices
        zero_count(&mut data); // triangles
        zero_count(&mut data); // edge links
        zero_count(&mut data); // door links
        zero_count(&mut data); // cover array
        zero_count(&mut data); // cover triangle mappings
        zero_count(&mut data); // waypoints
        data
    }

    fn geometry_with_navmesh_gen_cell() -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&15_u32.to_le_bytes()); // version
        data.extend_from_slice(&0_u32.to_le_bytes()); // pathing cell crc
        data.extend_from_slice(&0_u32.to_le_bytes()); // no parent world
        data.extend_from_slice(&raw(NAVMESH_GEN_CELL_LOCAL)); // NavMeshGenCell
        zero_count(&mut data); // vertices
        zero_count(&mut data); // triangles
        zero_count(&mut data); // edge links
        zero_count(&mut data); // door links
        zero_count(&mut data); // cover array
        zero_count(&mut data); // cover triangle mappings
        zero_count(&mut data); // waypoints
        data
    }

    #[test]
    fn drops_zero_size_fo76_nvnm_marker() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_record(Vec::new(), &interner);

        let report =
            rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        assert_eq!(report.fields_dropped, 1);
        assert!(record.fields.is_empty());
    }

    #[test]
    fn rewrites_geometry_parent_world_edge_link_and_door_link() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        mapper.add_mapping(
            FormKey::parse("000900@SeventySix.esm", &interner).unwrap(),
            FormKey::parse("000100@Output.esp", &interner).unwrap(),
        );
        mapper.add_mapping(
            FormKey::parse("000901@SeventySix.esm", &interner).unwrap(),
            FormKey::parse("000101@Output.esp", &interner).unwrap(),
        );
        mapper.add_mapping(
            FormKey::parse("000902@SeventySix.esm", &interner).unwrap(),
            FormKey::parse("000777@Fallout4.esm", &interner).unwrap(),
        );
        let mut record = make_record(geometry_with_parent_world_edge_and_door(), &interner);

        let report =
            rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        assert_eq!(report.formids_rewritten, 3);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        assert_eq!(read_u32(bytes, 8, "parent").unwrap(), 0x01000100);
        assert_eq!(read_u32(bytes, 32, "edge navmesh").unwrap(), 0x01000101);
        assert_eq!(read_u32(bytes, 49, "door ref").unwrap(), 0x00000777);
    }

    #[test]
    fn rewrites_parent_cell_when_parent_world_is_null() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        mapper.add_mapping(
            FormKey::parse("000903@SeventySix.esm", &interner).unwrap(),
            FormKey::parse("000333@Output.esp", &interner).unwrap(),
        );
        let mut record = make_record(geometry_with_parent_cell(), &interner);

        let report =
            rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        assert_eq!(report.formids_rewritten, 1);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        assert_eq!(read_u32(bytes, 12, "parent cell").unwrap(), 0x01000333);
    }

    #[test]
    fn keeps_navmesh_gen_cell_parent_cell_on_target_master() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_record(geometry_with_navmesh_gen_cell(), &interner);

        let report =
            rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        assert_eq!(report.formids_rewritten, 0);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        assert_eq!(
            read_u32(bytes, 12, "parent cell").unwrap(),
            NAVMESH_GEN_CELL_LOCAL
        );
    }

    #[test]
    fn allocates_forward_navmesh_refs_before_target_record_is_seen() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_record(geometry_with_parent_world_edge_and_door(), &interner);

        rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        assert_eq!(read_u32(bytes, 8, "parent").unwrap(), 0x01000900);
        assert_eq!(read_u32(bytes, 32, "edge navmesh").unwrap(), 0x01000901);
        assert_eq!(read_u32(bytes, 49, "door ref").unwrap(), 0x01000902);
    }

    #[test]
    fn rewrites_mnam_reference_and_preserves_triangle_tail() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        mapper.add_mapping(
            FormKey::parse("001234@SeventySix.esm", &interner).unwrap(),
            FormKey::parse("000555@Output.esp", &interner).unwrap(),
        );
        let mut record = make_navm_record(
            geometry_with_two_triangles(),
            mnam_entry(0x001234, &[0, 1]),
            &interner,
        );

        let report =
            rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        assert_eq!(report.mnam_formids_rewritten, 1);
        let mnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "MNAM")
            .expect("MNAM should be retained");
        let FieldValue::Bytes(bytes) = &mnam.value else {
            panic!("expected MNAM bytes");
        };
        assert_eq!(bytes.len(), 10);
        assert_eq!(read_u32(bytes, 0, "MNAM ref").unwrap(), 0x01000555);
        assert_eq!(read_u16(bytes, 4, "MNAM count").unwrap(), 2);
        assert_eq!(read_u16(bytes, 6, "MNAM tri 0").unwrap(), 0);
        assert_eq!(read_u16(bytes, 8, "MNAM tri 1").unwrap(), 1);
    }

    #[test]
    fn drops_malformed_mnam_instead_of_writing_ck_overflow_payload() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_navm_record(
            geometry_with_two_triangles(),
            raw(0x001234).to_vec(),
            &interner,
        );

        let report =
            rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        assert_eq!(report.mnam_fields_dropped, 1);
        assert!(
            !record
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "MNAM")
        );
        assert_eq!(record.warnings.len(), 1);
    }

    #[test]
    fn remaps_mnam_triangles_after_nvnm_triangle_cleanup() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_navm_record(
            geometry_with_degenerate_then_valid_triangle(),
            mnam_entry(0x001234, &[0, 1]),
            &interner,
        );

        rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        let mnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "MNAM")
            .expect("MNAM should be retained");
        let FieldValue::Bytes(bytes) = &mnam.value else {
            panic!("expected MNAM bytes");
        };
        assert_eq!(read_u16(bytes, 4, "MNAM count").unwrap(), 1);
        assert_eq!(read_u16(bytes, 6, "MNAM remapped triangle").unwrap(), 0);
    }

    #[test]
    fn normalizes_mis_slotted_internal_triangle_edges() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_record(geometry_with_mis_slotted_internal_link(), &interner);

        rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        let first_triangle = 20 + 4 * 12 + 4;
        assert_eq!(read_i16(bytes, first_triangle + 6, "edge 0").unwrap(), 1);
        assert_eq!(read_i16(bytes, first_triangle + 8, "edge 1").unwrap(), -1);
        assert_eq!(read_i16(bytes, first_triangle + 10, "edge 2").unwrap(), -1);
    }

    #[test]
    fn preserves_duplicate_external_edge_index_values() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_record(geometry_with_duplicate_external_edge_index(), &interner);

        rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        let first_triangle = 20 + 4 * 12 + 4;
        assert_eq!(read_i16(bytes, first_triangle + 6, "edge 0").unwrap(), 1);
        assert_eq!(read_i16(bytes, first_triangle + 8, "edge 1").unwrap(), 1);
        assert_eq!(read_i16(bytes, first_triangle + 10, "edge 2").unwrap(), -1);
    }

    #[test]
    fn clears_extra_info_flags_without_valid_edge_link_rows() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_record(geometry_with_invalid_extra_info_flag(), &interner);

        rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        let first_triangle = 20 + 3 * 12 + 4;
        assert_eq!(
            read_u16(bytes, first_triangle + TRIANGLE_FLAGS_OFFSET, "flags").unwrap() & 0x0007,
            0
        );
    }

    #[test]
    fn flips_downfacing_triangle_and_remaps_external_edge_slot() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_record(geometry_with_downfacing_external_edge(), &interner);

        rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        let first_triangle = 20 + 3 * 12 + 4;
        assert_eq!(read_u16(bytes, first_triangle, "tri vertex 0").unwrap(), 0);
        assert_eq!(
            read_u16(bytes, first_triangle + 2, "tri vertex 1").unwrap(),
            1
        );
        assert_eq!(
            read_u16(bytes, first_triangle + 4, "tri vertex 2").unwrap(),
            2
        );
        assert_eq!(read_i16(bytes, first_triangle + 6, "edge 0").unwrap(), -1);
        assert_eq!(read_i16(bytes, first_triangle + 8, "edge 1").unwrap(), -1);
        assert_eq!(read_i16(bytes, first_triangle + 10, "edge 2").unwrap(), 0);
        assert_eq!(
            read_u16(bytes, first_triangle + TRIANGLE_FLAGS_OFFSET, "flags").unwrap() & 0x0007,
            triangle_edge_extra_info_flag(2)
        );
    }

    #[test]
    fn culls_overlapping_sliver_triangle() {
        let mut data: SmallVec<[u8; 32]> = SmallVec::from_vec(geometry_with_overlapping_sliver());
        let report = cleanup_navmesh_geometry(&mut data).expect("cleanup");
        assert_eq!(
            report.triangle_count, 1,
            "the overlapping sliver triangle must be culled"
        );
        let remap = report.triangle_remap.expect("triangle remap present");
        assert_eq!(remap[0], Some(0), "larger triangle kept and reindexed to 0");
        assert_eq!(remap[1], None, "sliver triangle removed");
    }

    #[test]
    fn welds_and_culls_sliver_triangles_then_remaps_refs() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = FormIdContext::new("SeventySix.esm", &[]);
        let target = FormIdContext::new("Output.esp", &["Fallout4.esm"]);
        let mut record = make_record(geometry_with_welded_sliver_refs(), &interner);

        rewrite_record_nvnm_with_context(&mut record, &mut mapper, &source, &target).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected NVNM bytes");
        };
        let mut offset = 16_usize;
        let vertex_count = read_u32(bytes, offset, "vertex count").unwrap() as usize;
        offset += 4 + vertex_count * 12;
        assert_eq!(vertex_count, 5);

        let triangle_count = read_u32(bytes, offset, "triangle count").unwrap() as usize;
        offset += 4;
        assert_eq!(triangle_count, 2);
        assert_eq!(read_u16(bytes, offset + 2, "tri0 vertex 1").unwrap(), 1);
        assert_eq!(read_i16(bytes, offset + 8, "tri0 edge 1").unwrap(), 1);
        let second_triangle = offset + 21;
        assert_eq!(
            read_u16(bytes, second_triangle, "tri1 vertex 0").unwrap(),
            1
        );
        assert_eq!(
            read_i16(bytes, second_triangle + 6, "tri1 edge 0").unwrap(),
            2
        );
        assert_eq!(
            read_i16(bytes, second_triangle + 10, "tri1 edge 2").unwrap(),
            0
        );
        offset += triangle_count * 21;

        assert_eq!(read_u32(bytes, offset, "edge link count").unwrap(), 3);
        offset += 4 + 3 * 11;
        assert_eq!(read_u32(bytes, offset, "door link count").unwrap(), 1);
        offset += 4;
        assert_eq!(read_i16(bytes, offset, "door link triangle").unwrap(), 1);
        offset += 10;

        assert_eq!(read_u32(bytes, offset, "cover count").unwrap(), 1);
        offset += 4;
        assert_eq!(read_u16(bytes, offset, "cover vertex 1").unwrap(), 1);
        offset += 8;

        assert_eq!(read_u32(bytes, offset, "cover mapping count").unwrap(), 1);
        offset += 4;
        assert_eq!(
            read_i16(bytes, offset + 2, "cover mapping triangle").unwrap(),
            1
        );
        offset += 4;

        assert_eq!(read_u32(bytes, offset, "waypoint count").unwrap(), 1);
        offset += 4;
        assert_eq!(
            read_i16(bytes, offset + 12, "waypoint triangle").unwrap(),
            1
        );
        offset += 18;

        assert_eq!(read_u32(bytes, offset, "grid divisor").unwrap(), 1);
        offset += 36;
        assert_eq!(read_u32(bytes, offset, "grid cell count").unwrap(), 2);
        offset += 4;
        assert_eq!(read_i16(bytes, offset, "grid cell tri0").unwrap(), 0);
        assert_eq!(read_i16(bytes, offset + 2, "grid cell tri1").unwrap(), 1);
    }
}
