// Phase: convert_face
//
// Params shape (JSON):
// {
//   "npc_form_keys":      ["FalloutNV.esm:000B6C"],   // NPC_ form keys to process
//   "source_extracted":   "/path/to/source/extracted", // extracted FNV game data
//   "target_extracted":   "/path/to/target/extracted", // extracted FO4 game data
//   "target_race":        "HumanRace",                 // unused currently
//   "morph_weight_cap":   0.5,
//   "auto_skin_reference_body": "/path/to/ref.nif",    // unused currently
//   "emit_first_person":  false,
//   "output_plugin_name": "B21_Output.esp",
//   "correspondence_path_male":   "/path/fnv_to_fo4_correspondence_male.npz",
//   "correspondence_path_female": "/path/fnv_to_fo4_correspondence_female.npz",
//   "uv_lut_path_male":   "/path/fnv_to_fo4_facetint_uv_lut_male.npz",
//   "uv_lut_path_female": "/path/fnv_to_fo4_facetint_uv_lut_female.npz",
//   "hair_table_path":    "/path/hair_lookup.yaml",   // optional; uses embedded if absent
//   "named_bones_path":   "/path/named_bones.yaml",   // optional; uses embedded if absent
//   "precomputed_bone_solves": null
// }
//
// Phase output: writes FaceGeom NIFs and FaceTint DDS files under mod_path.
// PhaseReport:
//   records_changed = NPCs successfully baked
//   records_dropped = NPCs degraded to race defaults
//   warnings        = NPCs that failed bake + are skipped

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::source_read::read_record;
use crate::sym::StringInterner;
use crate::target_write::add_record_native;

// ---------------------------------------------------------------------------
// Embedded default resource data
// ---------------------------------------------------------------------------

static EMBEDDED_HAIR_LOOKUP_YAML: &str = include_str!("resources/face/hair_lookup.yaml");

static EMBEDDED_NAMED_BONES_YAML: &str = include_str!("resources/face/named_bones.yaml");

// ---------------------------------------------------------------------------
// FNV race classification
// ---------------------------------------------------------------------------

const FNV_HUMAN_MALE_IDS: &[&str] = &[
    "000019", "000023", "00001b", "00001d", "0038e5", "0038e6", "0038e7", "0038e8", "00f43d",
    "00f43c",
];

const FNV_HUMAN_FEMALE_IDS: &[&str] = &[
    "00001a", "00001c", "00001e", "000024", "0038e9", "0038ea", "0038eb", "0038ec",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RaceClass {
    HumanMale,
    HumanFemale,
    Unknown,
}

fn classify_race(object_id: &str) -> RaceClass {
    let lower = object_id.trim().to_ascii_lowercase();
    if FNV_HUMAN_MALE_IDS.contains(&lower.as_str()) {
        return RaceClass::HumanMale;
    }
    if FNV_HUMAN_FEMALE_IDS.contains(&lower.as_str()) {
        return RaceClass::HumanFemale;
    }
    RaceClass::Unknown
}

fn should_attempt_bake(race_class: RaceClass, coefficients: &[f32]) -> bool {
    if !matches!(race_class, RaceClass::HumanMale | RaceClass::HumanFemale) {
        return false;
    }
    if coefficients.len() != 50 {
        return false;
    }
    coefficients.iter().any(|&v| v.abs() > 1e-6)
}

// ---------------------------------------------------------------------------
// NPZ / NPY minimal parser
// ---------------------------------------------------------------------------

/// Parse a numpy .npy file and return f32 array + shape (flattened, row-major).
fn parse_npy_f32(data: &[u8]) -> Result<(Vec<f32>, Vec<usize>), String> {
    if data.len() < 10 {
        return Err("NPY file too short".into());
    }
    // Magic: 0x93, "NUMPY"
    if &data[0..6] != b"\x93NUMPY" {
        return Err("Not an NPY file (bad magic)".into());
    }
    let major = data[6];
    let header_len = if major == 1 {
        if data.len() < 10 {
            return Err("NPY v1 header too short".into());
        }
        u16::from_le_bytes([data[8], data[9]]) as usize
    } else if major == 2 {
        if data.len() < 12 {
            return Err("NPY v2 header too short".into());
        }
        u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize
    } else {
        return Err(format!("Unsupported NPY major version {major}"));
    };
    let header_start = if major == 1 { 10 } else { 12 };
    if data.len() < header_start + header_len {
        return Err("NPY header truncated".into());
    }
    let header_str = std::str::from_utf8(&data[header_start..header_start + header_len])
        .map_err(|e| format!("NPY header UTF-8: {e}"))?;

    // Parse dtype and shape from header dict string like:
    // {'descr': '<f4', 'fortran_order': False, 'shape': (N, 3), }
    let dtype = extract_npy_field(header_str, "descr");
    let shape_str = extract_npy_field(header_str, "shape");
    let fortran = extract_npy_field(header_str, "fortran_order");

    if fortran.trim() == "True" {
        return Err("Fortran-order NPY arrays not supported".into());
    }

    let element_bytes: usize = match dtype.trim() {
        "'<f4'" | "\"<f4\"" | "<f4" => 4,
        "'<f8'" | "\"<f8\"" | "<f8" => 8, // f64 — we'll convert
        "'<i4'" | "\"<i4\"" | "<i4" => 4, // i32
        "'<i2'" | "\"<i2\"" | "<i2" => 2, // i16
        other => return Err(format!("Unsupported NPY dtype: {other}")),
    };

    let shape = parse_npy_shape(&shape_str)?;
    let total_elements: usize = shape.iter().product();
    let data_start = header_start + header_len;
    let expected_bytes = total_elements * element_bytes;
    if data.len() < data_start + expected_bytes {
        return Err(format!(
            "NPY data truncated: need {expected_bytes} bytes at offset {data_start}, got {}",
            data.len() - data_start
        ));
    }

    let raw = &data[data_start..data_start + expected_bytes];

    let floats: Vec<f32> = match dtype.trim() {
        "'<f4'" | "\"<f4\"" | "<f4" => raw
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
        "'<f8'" | "\"<f8\"" | "<f8" => raw
            .chunks_exact(8)
            .map(|b| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32)
            .collect(),
        "'<i4'" | "\"<i4\"" | "<i4" => raw
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32)
            .collect(),
        "'<i2'" | "\"<i2\"" | "<i2" => raw
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32)
            .collect(),
        _ => unreachable!(),
    };

    Ok((floats, shape))
}

/// Parse a numpy .npy file and return i32 array + shape.
fn parse_npy_i32(data: &[u8]) -> Result<(Vec<i32>, Vec<usize>), String> {
    if data.len() < 10 {
        return Err("NPY file too short".into());
    }
    if &data[0..6] != b"\x93NUMPY" {
        return Err("Not an NPY file (bad magic)".into());
    }
    let major = data[6];
    let header_len = if major == 1 {
        u16::from_le_bytes([data[8], data[9]]) as usize
    } else if major == 2 {
        u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize
    } else {
        return Err(format!("Unsupported NPY major version {major}"));
    };
    let header_start = if major == 1 { 10 } else { 12 };
    let header_str = std::str::from_utf8(&data[header_start..header_start + header_len])
        .map_err(|e| format!("NPY header UTF-8: {e}"))?;

    let dtype = extract_npy_field(header_str, "descr");
    let shape_str = extract_npy_field(header_str, "shape");
    let shape = parse_npy_shape(&shape_str)?;
    let total_elements: usize = shape.iter().product();
    let data_start = header_start + header_len;

    let ints: Vec<i32> = match dtype.trim() {
        "'<i4'" | "\"<i4\"" | "<i4" => {
            let raw = &data[data_start..data_start + total_elements * 4];
            raw.chunks_exact(4)
                .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect()
        }
        "'<i2'" | "\"<i2\"" | "<i2" => {
            let raw = &data[data_start..data_start + total_elements * 2];
            raw.chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]) as i32)
                .collect()
        }
        other => return Err(format!("Unsupported i32 NPY dtype: {other}")),
    };
    Ok((ints, shape))
}

fn extract_npy_field<'a>(header: &'a str, key: &str) -> &'a str {
    // Find  'key': value  or  "key": value
    for quote in &["'", "\""] {
        let needle = format!("{quote}{key}{quote}");
        if let Some(pos) = header.find(&needle) {
            let after = &header[pos + needle.len()..];
            let after = after.trim_start_matches(':').trim_start();
            // If value starts with '(' it's a tuple — extract until matching ')'
            if after.starts_with('(') {
                if let Some(end) = after.find(')') {
                    return after[..=end].trim();
                }
            }
            // Otherwise extract up to comma or closing brace
            return after
                .split(|c| c == ',' || c == '}')
                .next()
                .unwrap_or("")
                .trim();
        }
    }
    ""
}

fn parse_npy_shape(s: &str) -> Result<Vec<usize>, String> {
    let s = s.trim().trim_matches('(').trim_matches(')').trim();
    if s.is_empty() {
        return Ok(vec![]);
    }
    s.split(',')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            p.trim()
                .parse::<usize>()
                .map_err(|e| format!("Shape parse: {e}"))
        })
        .collect()
}

/// Minimal ZIP reader — extracts named file bytes from an unencrypted ZIP.
/// Returns HashMap<filename, bytes>.
fn read_zip_entries(data: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    use flate2::read::DeflateDecoder;
    use std::io::Read;

    let mut entries: HashMap<String, Vec<u8>> = HashMap::new();
    let mut pos = 0usize;

    while pos + 4 <= data.len() {
        // Local file header signature: PK\x03\x04
        if &data[pos..pos + 4] == b"PK\x03\x04" {
            if pos + 30 > data.len() {
                break;
            }
            let compression = u16::from_le_bytes([data[pos + 8], data[pos + 9]]);
            let compressed_size = u32::from_le_bytes([
                data[pos + 18],
                data[pos + 19],
                data[pos + 20],
                data[pos + 21],
            ]) as usize;
            let uncompressed_size = u32::from_le_bytes([
                data[pos + 22],
                data[pos + 23],
                data[pos + 24],
                data[pos + 25],
            ]) as usize;
            let fname_len = u16::from_le_bytes([data[pos + 26], data[pos + 27]]) as usize;
            let extra_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;
            let header_end = pos + 30 + fname_len + extra_len;
            let data_end = header_end + compressed_size;

            if data_end > data.len() {
                break;
            }

            let fname = String::from_utf8_lossy(&data[pos + 30..pos + 30 + fname_len]).into_owned();
            let raw = &data[header_end..data_end];

            let decompressed = match compression {
                0 => raw.to_vec(), // stored
                8 => {
                    // deflate
                    let mut decoder = DeflateDecoder::new(raw);
                    let mut out = Vec::with_capacity(uncompressed_size);
                    decoder
                        .read_to_end(&mut out)
                        .map_err(|e| format!("ZIP deflate: {e}"))?;
                    out
                }
                other => return Err(format!("ZIP compression method {other} not supported")),
            };

            entries.insert(fname, decompressed);
            pos = data_end;
        } else if &data[pos..pos + 4] == b"PK\x01\x02" {
            // Central directory — stop scanning local entries
            break;
        } else {
            pos += 1;
        }
    }

    Ok(entries)
}

/// Load a .npz file and return f32 array by key + shape.
fn load_npz_f32(path: &Path, key: &str) -> Result<(Vec<f32>, Vec<usize>), String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let entries = read_zip_entries(&data)?;
    let npy_name = format!("{key}.npy");
    let npy_data = entries
        .get(&npy_name)
        .or_else(|| entries.get(key))
        .ok_or_else(|| format!("Key '{key}' not found in {}", path.display()))?;
    parse_npy_f32(npy_data)
}

fn load_npz_i32(path: &Path, key: &str) -> Result<(Vec<i32>, Vec<usize>), String> {
    let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let entries = read_zip_entries(&data)?;
    let npy_name = format!("{key}.npy");
    let npy_data = entries
        .get(&npy_name)
        .or_else(|| entries.get(key))
        .ok_or_else(|| format!("Key '{key}' not found in {}", path.display()))?;
    parse_npy_i32(npy_data)
}

// ---------------------------------------------------------------------------
// Correspondence table
// ---------------------------------------------------------------------------

/// Triangle/barycentric correspondence for FNV→FO4 vertex interpolation.
struct Correspondence {
    /// (sample_count, 3) — vertex indices into the FNV mesh
    triangle_indices: Vec<[i32; 3]>,
    /// (sample_count, 3) — barycentric weights
    barycentrics: Vec<[f32; 3]>,
    sample_count: usize,
}

impl Correspondence {
    fn load(path: &Path) -> Result<Self, String> {
        let (ti_flat, ti_shape) = load_npz_i32(path, "triangle_indices")?;
        let (bc_flat, bc_shape) = load_npz_f32(path, "barycentrics")?;

        if ti_shape.len() != 2 || ti_shape[1] != 3 {
            return Err(format!(
                "triangle_indices shape must be (N,3), got {:?}",
                ti_shape
            ));
        }
        if bc_shape.len() != 2 || bc_shape[1] != 3 {
            return Err(format!(
                "barycentrics shape must be (N,3), got {:?}",
                bc_shape
            ));
        }
        let n = ti_shape[0];
        if bc_shape[0] != n {
            return Err("triangle_indices and barycentrics row counts differ".into());
        }

        let triangle_indices: Vec<[i32; 3]> = ti_flat
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        let barycentrics: Vec<[f32; 3]> = bc_flat
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();

        Ok(Self {
            triangle_indices,
            barycentrics,
            sample_count: n,
        })
    }

    /// Interpolate FO4 vertex positions from FNV vertices.
    /// source_vertices: (N_fnv, 3), returns (sample_count, 3)
    fn interpolate(&self, source_vertices: &[[f32; 3]]) -> Result<Vec<[f32; 3]>, String> {
        let n_src = source_vertices.len();
        let mut out = Vec::with_capacity(self.sample_count);
        for i in 0..self.sample_count {
            let ti = &self.triangle_indices[i];
            let bc = &self.barycentrics[i];
            for &idx in ti {
                if idx < 0 || idx as usize >= n_src {
                    return Err(format!(
                        "Correspondence triangle index {idx} out of range [0, {n_src})"
                    ));
                }
            }
            let v0 = &source_vertices[ti[0] as usize];
            let v1 = &source_vertices[ti[1] as usize];
            let v2 = &source_vertices[ti[2] as usize];
            out.push([
                v0[0] * bc[0] + v1[0] * bc[1] + v2[0] * bc[2],
                v0[1] * bc[0] + v1[1] * bc[1] + v2[1] * bc[2],
                v0[2] * bc[0] + v1[2] * bc[1] + v2[2] * bc[2],
            ]);
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// TRI file parser
// ---------------------------------------------------------------------------

const TRI_MAGIC: &[u8; 8] = b"FRTRI003";
const TRI_HEADER_SIZE: usize = 64;

/// Parse an FNV .tri file and return the neutral vertex positions.
fn parse_tri_neutral_vertices(path: &Path) -> Result<Vec<[f32; 3]>, String> {
    let data = std::fs::read(path).map_err(|e| format!("read TRI {}: {e}", path.display()))?;
    if data.len() < TRI_HEADER_SIZE {
        return Err(format!("TRI file too short: {}", path.display()));
    }
    if &data[..8] != TRI_MAGIC {
        return Err(format!("TRI bad magic: {}", path.display()));
    }
    // Header struct: 8s + 10×u32 + 4 bytes pad = 64
    // Offsets: 0=magic(8), 8=vertex_count(4), 12=face_count(4), ..., 28=morph_count(4),
    //          32=modifier_morph_count(4), 36=modifier_vertex_count(4)
    let vertex_count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let modifier_vertex_count =
        u32::from_le_bytes([data[36], data[37], data[38], data[39]]) as usize;
    let total_vertices = vertex_count + modifier_vertex_count;

    let needed = TRI_HEADER_SIZE + total_vertices * 12;
    if data.len() < needed {
        return Err(format!(
            "TRI truncated: need {needed} bytes, got {}",
            data.len()
        ));
    }

    let mut vertices = Vec::with_capacity(total_vertices);
    let mut offset = TRI_HEADER_SIZE;
    for _ in 0..total_vertices {
        let x = f32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        let y = f32::from_le_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);
        let z = f32::from_le_bytes([
            data[offset + 8],
            data[offset + 9],
            data[offset + 10],
            data[offset + 11],
        ]);
        vertices.push([x, y, z]);
        offset += 12;
    }
    Ok(vertices)
}

// ---------------------------------------------------------------------------
// EGM file parser
// ---------------------------------------------------------------------------

const EGM_HEADER_SIZE: usize = 64;
const EGM_MAGIC: &[u8; 5] = b"FREGM";

/// Parse EGM morph basis for `num_differences` morphs.
/// Returns: (num_differences, num_vertices, 3) as flat Vec<f32> in row-major order.
fn parse_egm_basis(path: &Path) -> Result<(usize, usize, Vec<f32>), String> {
    let data = std::fs::read(path).map_err(|e| format!("read EGM {}: {e}", path.display()))?;
    if data.len() < EGM_HEADER_SIZE {
        return Err(format!("EGM file too short: {}", path.display()));
    }
    if &data[..5] != EGM_MAGIC {
        return Err(format!("EGM bad magic: {}", path.display()));
    }
    let num_vertices = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let num_differences = u32::from_le_bytes([data[12], data[13], data[14], data[15]]) as usize;
    let num_asymmetric = u32::from_le_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let total_morphs = num_differences + num_asymmetric;
    let morph_stride = 4 + num_vertices * 3 * 2;
    let expected = EGM_HEADER_SIZE + total_morphs * morph_stride;
    if data.len() < expected {
        return Err(format!(
            "EGM truncated: expected {expected} bytes, got {}",
            data.len()
        ));
    }

    // Only decode the first num_differences morphs (symmetric)
    let mut basis = vec![0.0f32; num_differences * num_vertices * 3];
    let mut offset = EGM_HEADER_SIZE;
    for morph_idx in 0..num_differences {
        let scale = f32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;
        for vert_xyz in 0..(num_vertices * 3) {
            let packed = i16::from_le_bytes([data[offset], data[offset + 1]]) as f32;
            basis[morph_idx * num_vertices * 3 + vert_xyz] = packed * scale;
            offset += 2;
        }
        if morph_idx < num_differences {}
    }
    // Skip asymmetric morphs
    // (already accounted for by indexing only first num_differences)

    Ok((num_differences, num_vertices, basis))
}

/// Reconstruct FNV face vertices from TRI neutral + EGM basis + coefficients.
fn reconstruct_fnv_face(
    base_head_nif_path: &Path, // loads the sibling .tri file next to this path
    egm_path: &Path,
    coefficients: &[f32],
) -> Result<Vec<[f32; 3]>, String> {
    let tri_path = base_head_nif_path.with_extension("tri");
    let neutral = parse_tri_neutral_vertices(&tri_path)?;
    let (num_diffs, num_verts, basis) = parse_egm_basis(egm_path)?;

    if coefficients.len() != num_diffs {
        return Err(format!(
            "EGM coefficient count {} != basis count {}",
            coefficients.len(),
            num_diffs
        ));
    }
    if neutral.len() != num_verts {
        return Err(format!(
            "EGM vertex count {} != TRI neutral vertex count {}",
            num_verts,
            neutral.len()
        ));
    }

    // displacement[v][xyz] = sum_k(coeff[k] * basis[k][v][xyz])
    let mut displaced = neutral.clone();
    for k in 0..num_diffs {
        let c = coefficients[k];
        if c.abs() < 1e-9 {
            continue;
        }
        for v in 0..num_verts {
            let base = k * num_verts * 3 + v * 3;
            displaced[v][0] += c * basis[base];
            displaced[v][1] += c * basis[base + 1];
            displaced[v][2] += c * basis[base + 2];
        }
    }
    Ok(displaced)
}

// ---------------------------------------------------------------------------
// FO4 base head NIF: extract neutral vertex positions
// ---------------------------------------------------------------------------

fn load_fo4_neutral_vertices(nif_path: &Path) -> Result<Vec<[f32; 3]>, String> {
    use nif_core_native::model::{NifFile, NifValue};

    let nif = NifFile::load(nif_path.to_path_buf())
        .map_err(|e| format!("load NIF {}: {e}", nif_path.display()))?;

    let schema = nif_core_native::schema::NifSchema::from_generated();

    let shape_ids: Vec<usize> = (0..nif.blocks.len())
        .filter(|&i| {
            if let Some(block) = nif.get_block(i) {
                schema.is_subtype_of(&block.type_name, "BSTriShape")
                    && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(_)))
            } else {
                false
            }
        })
        .collect();

    if shape_ids.is_empty() {
        return Err(format!(
            "No BSTriShape with vertex data in {}",
            nif_path.display()
        ));
    }
    if shape_ids.len() > 1 {
        return Err(format!(
            "Expected exactly 1 BSTriShape in face NIF {}, got {}",
            nif_path.display(),
            shape_ids.len()
        ));
    }

    let block = nif.get_block(shape_ids[0]).unwrap();
    let vd = match block.get_field("Vertex Data") {
        Some(NifValue::Array(arr)) => arr,
        _ => return Err("Vertex Data is not an array".into()),
    };

    let mut verts = Vec::with_capacity(vd.len());
    for (i, entry) in vd.iter().enumerate() {
        if let NifValue::Struct(s) = entry {
            if let Some(NifValue::Struct(vertex)) = s.get("Vertex") {
                let x = extract_struct_f32(vertex, "x").unwrap_or(0.0);
                let y = extract_struct_f32(vertex, "y").unwrap_or(0.0);
                let z = extract_struct_f32(vertex, "z").unwrap_or(0.0);
                verts.push([x, y, z]);
            } else {
                return Err(format!("Vertex Data entry {i} has no Vertex sub-struct"));
            }
        } else {
            return Err(format!("Vertex Data entry {i} is not a struct"));
        }
    }
    Ok(verts)
}

fn extract_struct_f32(
    s: &indexmap::IndexMap<String, nif_core_native::model::NifValue>,
    key: &str,
) -> Option<f32> {
    use nif_core_native::model::NifValue;
    match s.get(key) {
        Some(NifValue::Float(f)) => Some(*f as f32),
        Some(NifValue::Int(i)) => Some(*i as f32),
        Some(NifValue::UInt(u)) => Some(*u as f32),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Write facegeom NIF
// ---------------------------------------------------------------------------

fn write_facegeom_nif(
    base_nif_path: &Path,
    deformation: &[[f32; 3]],
    out_path: &Path,
) -> Result<(), String> {
    use nif_core_native::model::{NifFile, NifValue};

    let mut nif = NifFile::load(base_nif_path.to_path_buf())
        .map_err(|e| format!("load base head NIF: {e}"))?;
    let schema = nif_core_native::schema::NifSchema::from_generated();

    let shape_ids: Vec<usize> = (0..nif.blocks.len())
        .filter(|&i| {
            if let Some(b) = nif.get_block(i) {
                schema.is_subtype_of(&b.type_name, "BSTriShape")
                    && matches!(b.get_field("Vertex Data"), Some(NifValue::Array(_)))
            } else {
                false
            }
        })
        .collect();

    if shape_ids.len() != 1 {
        return Err(format!(
            "Expected 1 BSTriShape in face NIF, got {}",
            shape_ids.len()
        ));
    }

    let shape_id = shape_ids[0];
    let vertex_count = {
        let b = nif.get_block(shape_id).unwrap();
        match b.get_field("Vertex Data") {
            Some(NifValue::Array(arr)) => arr.len(),
            _ => return Err("Vertex Data not an array".into()),
        }
    };

    if deformation.len() != vertex_count {
        return Err(format!(
            "Deformation vertex count {} != NIF vertex count {}",
            deformation.len(),
            vertex_count
        ));
    }

    // Build updated vertex data array
    let updated_vd = {
        let b = nif.get_block(shape_id).unwrap();
        let arr = match b.get_field("Vertex Data") {
            Some(NifValue::Array(a)) => a.clone(),
            _ => unreachable!(),
        };
        let mut new_arr = arr;
        for (i, entry) in new_arr.iter_mut().enumerate() {
            if let NifValue::Struct(s) = entry {
                if let Some(NifValue::Struct(v)) = s.get("Vertex") {
                    let base_x = extract_struct_f32(v, "x").unwrap_or(0.0);
                    let base_y = extract_struct_f32(v, "y").unwrap_or(0.0);
                    let base_z = extract_struct_f32(v, "z").unwrap_or(0.0);
                    let new_vertex = {
                        let mut nv = indexmap::IndexMap::new();
                        nv.insert(
                            "x".into(),
                            NifValue::Float((base_x + deformation[i][0]) as f64),
                        );
                        nv.insert(
                            "y".into(),
                            NifValue::Float((base_y + deformation[i][1]) as f64),
                        );
                        nv.insert(
                            "z".into(),
                            NifValue::Float((base_z + deformation[i][2]) as f64),
                        );
                        nv
                    };
                    s.insert("Vertex".into(), NifValue::Struct(new_vertex));
                }
            }
        }
        new_arr
    };

    {
        let b = nif.blocks.get_mut(shape_id).unwrap();
        b.set_field("Vertex Data", NifValue::Array(updated_vd));
        b.set_field("Num Vertices", NifValue::UInt(vertex_count as u64));
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create dirs {}: {e}", parent.display()))?;
    }
    nif.save(Some(out_path.to_path_buf()))
        .map_err(|e| format!("save facegeom NIF {}: {e}", out_path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Write facetint DDS (fallback solid color)
// ---------------------------------------------------------------------------

/// Write a fallback solid-color 1024×1024 RGBA DDS (BC7_UNORM).
fn write_fallback_facetint_dds(out_path: &Path, color_rgba: [u8; 4]) -> Result<(), String> {
    const W: u32 = 1024;
    const H: u32 = 1024;
    let rgba: Vec<u8> = (0..(W * H))
        .flat_map(|_| color_rgba.iter().copied())
        .collect();

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create dirs {}: {e}", parent.display()))?;
    }

    directxtex_native::write_dds_rgba_image(out_path, W, H, &rgba, "BC7_UNORM", false)
        .map_err(|e| format!("write facetint DDS: {e}"))
}

/// Deterministic fallback color based on formid.
fn deterministic_facetint_color(formid_hex: &str) -> [u8; 4] {
    let seed = u32::from_str_radix(formid_hex, 16).unwrap_or(0);
    [
        96 + ((seed & 0x3F) as u8),
        80 + (((seed >> 8) & 0x3F) as u8),
        72 + (((seed >> 16) & 0x3F) as u8),
        255,
    ]
}

// ---------------------------------------------------------------------------
// UV remap and facetint with source DDS
// ---------------------------------------------------------------------------

/// Load UV LUT from NPZ.
/// Returns: (flat_uv, height, width) where flat_uv has shape H×W×2 row-major.
fn load_uv_lut(path: &Path) -> Result<(Vec<f32>, usize, usize), String> {
    let (flat, shape) = load_npz_f32(path, "uv")?;
    if shape.len() != 3 || shape[2] != 2 {
        return Err(format!("UV LUT shape must be (H,W,2), got {:?}", shape));
    }
    Ok((flat, shape[0], shape[1]))
}

/// Remap a source RGBA image using the UV LUT.
/// source: H_src×W_src×4, lut: H_out×W_out×2 (u,v) in [0,1]
fn remap_rgba_with_uv_lut(
    source_rgba: &[u8],
    src_w: u32,
    src_h: u32,
    uv_flat: &[f32],
    lut_h: usize,
    lut_w: usize,
) -> Vec<u8> {
    let mut out = vec![0u8; lut_h * lut_w * 4];
    let sw = src_w as usize;
    let sh = src_h as usize;

    for oy in 0..lut_h {
        for ox in 0..lut_w {
            let lut_idx = (oy * lut_w + ox) * 2;
            let u = uv_flat[lut_idx];
            let v = uv_flat[lut_idx + 1];
            if !u.is_finite() || !v.is_finite() {
                continue;
            }
            let sx = (u * (sw as f32 - 1.0)).round().clamp(0.0, sw as f32 - 1.0) as usize;
            let sy = (v * (sh as f32 - 1.0)).round().clamp(0.0, sh as f32 - 1.0) as usize;
            let src_i = (sy * sw + sx) * 4;
            let dst_i = (oy * lut_w + ox) * 4;
            out[dst_i..dst_i + 4].copy_from_slice(&source_rgba[src_i..src_i + 4]);
        }
    }
    out
}

/// Try to write a UV-remapped facetint DDS from a source DDS file.
/// Falls back to solid color on any error.
fn write_facetint_dds(
    out_path: &Path,
    source_dds_path: Option<&Path>,
    uv_lut_path: &Path,
    fallback_color: [u8; 4],
) -> Result<(), String> {
    if let Some(src_path) = source_dds_path {
        if src_path.is_file() {
            if let Ok(uv) = load_uv_lut(uv_lut_path) {
                let (uv_flat, lut_h, lut_w) = uv;
                if let Ok(src_img) = directxtex_native::read_dds_rgba_image(src_path) {
                    let remapped = remap_rgba_with_uv_lut(
                        &src_img.rgba,
                        src_img.width,
                        src_img.height,
                        &uv_flat,
                        lut_h,
                        lut_w,
                    );
                    if let Some(parent) = out_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    return directxtex_native::write_dds_rgba_image(
                        out_path,
                        lut_w as u32,
                        lut_h as u32,
                        &remapped,
                        "BC7_UNORM",
                        false,
                    )
                    .map_err(|e| format!("write remapped facetint: {e}"));
                }
            }
        }
    }
    write_fallback_facetint_dds(out_path, fallback_color)
}

// ---------------------------------------------------------------------------
// Bone solve
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct NamedBone {
    name: String,
    small_index: u32,
    weight_threshold: f32,
}

fn load_named_bones(yaml_text: &str) -> Result<Vec<NamedBone>, String> {
    let val: JsonValue =
        serde_saphyr::from_str(yaml_text).map_err(|e| format!("named_bones YAML: {e}"))?;
    let bones = val
        .get("bones")
        .and_then(|b| b.as_array())
        .ok_or("named_bones.yaml: missing 'bones' list")?;

    bones
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("bone[{i}]: missing name"))?
                .to_string();
            let small_index = entry
                .get("small_index")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| format!("bone[{i}]: missing small_index"))?
                as u32;
            let weight_threshold = entry
                .get("weight_threshold")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| format!("bone[{i}]: missing weight_threshold"))?
                as f32;
            Ok(NamedBone {
                name,
                small_index,
                weight_threshold,
            })
        })
        .collect()
}

/// Bone offset result: small_index → (x, y, z)
type BoneOffsets = HashMap<u32, [f32; 3]>;

/// Extract skin data from FO4 face bones NIF.
/// Returns: (weights, bone_indices, bone_names)
/// weights: (n_verts, max_bones) — f32
/// bone_indices: (n_verts, max_bones) — i32
fn extract_face_bones_skin(
    nif_path: &Path,
) -> Result<(Vec<Vec<f32>>, Vec<Vec<i32>>, Vec<String>), String> {
    use nif_core_native::model::{NifFile, NifValue};

    let nif =
        NifFile::load(nif_path.to_path_buf()).map_err(|e| format!("load face bones NIF: {e}"))?;
    let schema = nif_core_native::schema::NifSchema::from_generated();

    // Find the BSSubIndexTriShape (or BSTriShape) with skin
    let shape_id = (0..nif.blocks.len())
        .find(|&i| {
            if let Some(b) = nif.get_block(i) {
                schema.is_subtype_of(&b.type_name, "BSTriShape")
                    && b.get_field("Vertex Data").is_some()
            } else {
                false
            }
        })
        .ok_or("No BSTriShape found in face bones NIF")?;

    let block = nif.get_block(shape_id).unwrap();
    let vd = match block.get_field("Vertex Data") {
        Some(NifValue::Array(arr)) => arr,
        _ => return Err("Vertex Data not an array in face bones NIF".into()),
    };

    let n = vd.len();
    const MAX_BONES: usize = 4;
    let mut weights = vec![vec![0.0f32; MAX_BONES]; n];
    let mut bone_indices = vec![vec![0i32; MAX_BONES]; n];

    for (i, entry) in vd.iter().enumerate() {
        if let NifValue::Struct(s) = entry {
            // Try "Bone Weights" + "Bone Indices" (BSSkin::Instance format)
            let bw_list = s.get("Bone Weights").or_else(|| s.get("BoneWeights"));
            let bi_list = s.get("Bone Indices");

            if let (Some(NifValue::Array(bw_arr)), Some(NifValue::Array(bi_arr))) =
                (bw_list, bi_list)
            {
                for j in 0..MAX_BONES.min(bw_arr.len()).min(bi_arr.len()) {
                    weights[i][j] = match &bw_arr[j] {
                        NifValue::Float(f) => *f as f32,
                        NifValue::Int(v) => *v as f32,
                        NifValue::UInt(v) => *v as f32,
                        _ => 0.0,
                    };
                    bone_indices[i][j] = match &bi_arr[j] {
                        NifValue::Int(v) => *v as i32,
                        NifValue::UInt(v) => *v as i32,
                        _ => 0,
                    };
                }
            }
        }
    }

    // Extract bone names from BSSkin::Instance
    let bone_names = extract_face_bone_names(&nif, shape_id);

    Ok((weights, bone_indices, bone_names))
}

fn extract_face_bone_names(nif: &nif_core_native::model::NifFile, shape_id: usize) -> Vec<String> {
    use nif_core_native::model::NifValue;

    // BSSkin::Instance is referenced via "Skin" field on the shape
    let skin_ref = nif
        .get_block(shape_id)
        .and_then(|b| b.get_field("Skin"))
        .and_then(|v| match v {
            NifValue::Ref(r) if *r >= 0 => Some(*r as usize),
            _ => None,
        });

    let skin_instance_id = match skin_ref {
        Some(id) => id,
        None => return vec![],
    };

    let skin_instance = match nif.get_block(skin_instance_id) {
        Some(b) => b,
        None => return vec![],
    };

    // Bone refs array
    let bones_arr = match skin_instance.get_field("Bones") {
        Some(NifValue::Array(arr)) => arr,
        _ => return vec![],
    };

    bones_arr
        .iter()
        .filter_map(|v| match v {
            NifValue::Ref(r) if *r >= 0 => nif
                .get_block(*r as usize)
                .and_then(|b| b.get_field("Name"))
                .and_then(|n| match n {
                    NifValue::String(s) => Some(s.clone()),
                    _ => None,
                }),
            _ => None,
        })
        .collect()
}

fn solve_bone_offsets(
    rest_vertices: &[[f32; 3]],
    deformed_vertices: &[[f32; 3]],
    weights: &[Vec<f32>],
    bone_indices: &[Vec<i32>],
    bone_names: &[String],
    named_bones: &[NamedBone],
) -> BoneOffsets {
    let n = rest_vertices.len();
    let bone_name_to_idx: HashMap<&str, usize> = bone_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();

    let mut result = BoneOffsets::new();
    for bone in named_bones {
        let skin_idx = bone_name_to_idx.get(bone.name.as_str()).copied();
        let offset = if let Some(skin_bone_idx) = skin_idx {
            let mut sum = [0.0f32; 3];
            let mut count = 0u32;
            for v in 0..n {
                let qualifies = bone_indices[v]
                    .iter()
                    .zip(weights[v].iter())
                    .any(|(&bi, &w)| bi as usize == skin_bone_idx && w >= bone.weight_threshold);
                if qualifies {
                    sum[0] += deformed_vertices[v][0] - rest_vertices[v][0];
                    sum[1] += deformed_vertices[v][1] - rest_vertices[v][1];
                    sum[2] += deformed_vertices[v][2] - rest_vertices[v][2];
                    count += 1;
                }
            }
            if count > 0 {
                [
                    sum[0] / count as f32,
                    sum[1] / count as f32,
                    sum[2] / count as f32,
                ]
            } else {
                [0.0, 0.0, 0.0]
            }
        } else {
            [0.0, 0.0, 0.0]
        };
        result.insert(bone.small_index, offset);
    }
    result
}

// ---------------------------------------------------------------------------
// Hair table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct HairRef {
    plugin: String,
    object_id: String,
}

impl HairRef {
    fn normalize_key(&self) -> String {
        format!(
            "{}:{}",
            self.plugin.to_ascii_lowercase(),
            self.object_id.to_ascii_lowercase()
        )
    }
}

struct HairTable {
    explicit: HashMap<String, HairRef>,
    male_default: Option<HairRef>,
    female_default: Option<HairRef>,
}

impl HairTable {
    fn load(yaml_text: &str) -> Result<Self, String> {
        let val: JsonValue =
            serde_saphyr::from_str(yaml_text).map_err(|e| format!("hair_table YAML: {e}"))?;

        let mut explicit: HashMap<String, HairRef> = HashMap::new();
        if let Some(mappings) = val.get("mappings").and_then(|m| m.as_array()) {
            for entry in mappings {
                let fnv = entry.get("fnv");
                let fo4 = entry.get("fo4");
                if let (Some(f), Some(t)) = (fnv, fo4) {
                    let key = HairRef {
                        plugin: f
                            .get("plugin")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        object_id: f
                            .get("object_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    }
                    .normalize_key();
                    explicit.insert(
                        key,
                        HairRef {
                            plugin: t
                                .get("plugin")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            object_id: t
                                .get("object_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        },
                    );
                }
            }
        }

        let defaults = val.get("race_defaults");
        let male_default = defaults.and_then(|d| d.get("human_male")).map(|v| HairRef {
            plugin: v
                .get("plugin")
                .and_then(|p| p.as_str())
                .unwrap_or("")
                .to_string(),
            object_id: v
                .get("object_id")
                .and_then(|o| o.as_str())
                .unwrap_or("")
                .to_string(),
        });
        let female_default = defaults
            .and_then(|d| d.get("human_female"))
            .map(|v| HairRef {
                plugin: v
                    .get("plugin")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string(),
                object_id: v
                    .get("object_id")
                    .and_then(|o| o.as_str())
                    .unwrap_or("")
                    .to_string(),
            });

        Ok(Self {
            explicit,
            male_default,
            female_default,
        })
    }

    fn lookup(&self, fnv_ref: Option<&HairRef>, race_class: RaceClass) -> Option<&HairRef> {
        if let Some(r) = fnv_ref {
            let key = r.normalize_key();
            if let Some(mapped) = self.explicit.get(&key) {
                return Some(mapped);
            }
        }
        match race_class {
            RaceClass::HumanMale => self.male_default.as_ref(),
            RaceClass::HumanFemale => self.female_default.as_ref(),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Body weight mapping
// ---------------------------------------------------------------------------

fn fnv_weight_to_fo4_morphs(weight: f32) -> (f32, f32, f32) {
    let clamped = weight.clamp(-1.0, 1.0);
    if clamped <= -0.33 {
        (clamped.abs(), 0.0, 0.0) // Thin, Muscular, Fat
    } else if clamped >= 0.33 {
        (0.0, 0.0, clamped)
    } else {
        (0.0, clamped, 0.0)
    }
}

// ---------------------------------------------------------------------------
// NPC source record extraction helpers
// ---------------------------------------------------------------------------

fn extract_fggs_coefficients(record: &Record) -> Option<Vec<f32>> {
    for entry in &record.fields {
        if entry.sig.as_str() == "FGGS" {
            if let FieldValue::Bytes(bytes) = &entry.value {
                if bytes.len() % 4 == 0 {
                    let coeffs: Vec<f32> = bytes
                        .chunks(4)
                        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                        .collect();
                    return Some(coeffs);
                }
            }
        }
    }
    None
}

/// Returns true if the female bit is set in ACBS.
fn extract_is_female(record: &Record) -> bool {
    for entry in &record.fields {
        if entry.sig.as_str() == "ACBS" {
            if let FieldValue::Bytes(bytes) = &entry.value {
                if bytes.len() >= 4 {
                    let flags = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    return flags & 1 != 0;
                }
            }
        }
    }
    false
}

/// Extract RNAM as a (plugin, object_id) pair.
fn extract_rnam_ref(record: &Record, interner: &StringInterner) -> Option<(String, String)> {
    for entry in &record.fields {
        if entry.sig.as_str() == "RNAM" {
            if let FieldValue::FormKey(fk) = &entry.value {
                let plugin = interner.resolve(fk.plugin).unwrap_or_default().to_string();
                let object_id = format!("{:06X}", fk.local).to_ascii_lowercase();
                return Some((plugin, object_id));
            }
        }
    }
    None
}

/// Extract HNAM (hair form key reference).
fn extract_hnam_ref(record: &Record, interner: &StringInterner) -> Option<HairRef> {
    for entry in &record.fields {
        if entry.sig.as_str() == "HNAM" {
            if let FieldValue::FormKey(fk) = &entry.value {
                let plugin = interner.resolve(fk.plugin).unwrap_or_default().to_string();
                let object_id = format!("{:06X}", fk.local).to_ascii_lowercase();
                return Some(HairRef { plugin, object_id });
            }
        }
    }
    None
}

fn extract_nam7_weight(record: &Record) -> f32 {
    for entry in &record.fields {
        if entry.sig.as_str() == "NAM7" {
            if let FieldValue::Float(f) = &entry.value {
                return *f as f32;
            }
            if let FieldValue::Bytes(bytes) = &entry.value {
                if bytes.len() >= 4 {
                    return f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                }
            }
        }
    }
    0.0
}

// ---------------------------------------------------------------------------
// Path resolution helpers
// ---------------------------------------------------------------------------

fn find_source_head_nif(source_extracted: &Path, sex: &str) -> Option<PathBuf> {
    let candidates: &[&str] = if sex == "female" {
        &[
            "Meshes/Characters/Head/headfemale.nif",
            "meshes/characters/head/headfemale.nif",
        ]
    } else {
        &[
            "Meshes/Characters/Head/headhuman.nif",
            "meshes/characters/head/headhuman.nif",
        ]
    };
    for c in candidates {
        let path = source_extracted.join(c);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_source_head_egm(source_extracted: &Path, sex: &str) -> Option<PathBuf> {
    let candidates: &[&str] = if sex == "female" {
        &[
            "Meshes/Characters/Head/headfemale.egm",
            "meshes/characters/head/headfemale.egm",
        ]
    } else {
        &[
            "Meshes/Characters/Head/headhuman.egm",
            "meshes/characters/head/headhuman.egm",
        ]
    };
    for c in candidates {
        let path = source_extracted.join(c);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_target_base_head_nif(target_extracted: &Path, sex: &str) -> Option<PathBuf> {
    let candidates: &[&str] = if sex == "female" {
        &[
            "Meshes/Actors/Character/CharacterAssets/BaseFemaleHead.nif",
            "meshes/actors/character/characterassets/basefemalehead.nif",
            "Meshes/Actors/Character/CharacterAssets/FaceParts/FemaleHead.nif",
        ]
    } else {
        &[
            "Meshes/Actors/Character/CharacterAssets/BaseMaleHead.nif",
            "meshes/actors/character/characterassets/basemalehead.nif",
            "Meshes/Actors/Character/CharacterAssets/FaceParts/MaleHead.nif",
        ]
    };
    for c in candidates {
        let path = target_extracted.join(c);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_target_face_bones_nif(target_extracted: &Path, sex: &str) -> Option<PathBuf> {
    let candidates: &[&str] = if sex == "female" {
        &[
            "Meshes/Actors/Character/CharacterAssets/BaseFemaleHead_faceBones.nif",
            "meshes/actors/character/characterassets/basefemalehead_facebones.nif",
        ]
    } else {
        &[
            "Meshes/Actors/Character/CharacterAssets/BaseMaleHead_faceBones.nif",
            "meshes/actors/character/characterassets/basemalehead_facebones.nif",
        ]
    };
    for c in candidates {
        let path = target_extracted.join(c);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn facegeom_relpath(output_plugin_name: &str, formid_hex: &str) -> String {
    format!("Meshes/Actors/Character/FaceGenData/FaceGeom/{output_plugin_name}/{formid_hex}.nif")
}

fn facetint_relpath(output_plugin_name: &str, formid_hex: &str) -> String {
    format!("Textures/Actors/Character/FaceGenData/FaceTint/{output_plugin_name}/{formid_hex}.dds")
}

fn normalize_formid_hex(form_key_str: &str) -> String {
    let object_id = form_key_str.split(':').next().unwrap_or(form_key_str);
    let object_id = object_id.trim_start_matches("0x").trim_start_matches("0X");
    let val = u32::from_str_radix(object_id, 16).unwrap_or(0) & 0xFFFFFFFF;
    format!("{val:08X}")
}

fn find_source_facetint_dds(
    source_extracted: &Path,
    source_plugin_name: &str,
    formid_hex: &str,
) -> Option<PathBuf> {
    let plugin_dir = source_plugin_name.to_ascii_lowercase();
    let root = source_extracted
        .join("textures")
        .join("characters")
        .join("facemods")
        .join(&plugin_dir);
    let hex_lower = formid_hex.to_ascii_lowercase();
    let val = u32::from_str_radix(&hex_lower, 16).ok()?;
    let alt = format!("{val:x}_0.dds");
    for stem in &[format!("{hex_lower}_0.dds"), alt] {
        let p = root.join(stem);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// NPC record field writing helpers (FO4 binary subrecord format)
// ---------------------------------------------------------------------------

/// Pack a FO4 FormKey (plugin:XXXXXX) as 4 little-endian bytes suitable for
/// a `formid`-codec subrecord.
///
/// `masters` is the ordered list of masters already in the target handle.
fn pack_formid(plugin: &str, object_id_hex: &str, masters: &[String]) -> [u8; 4] {
    let local =
        u32::from_str_radix(object_id_hex.trim_start_matches("0x"), 16).unwrap_or(0) & 0x00FF_FFFF;

    let master_idx = masters
        .iter()
        .position(|m| m.eq_ignore_ascii_case(plugin))
        .unwrap_or(masters.len()); // own plugin = own index

    let raw = ((master_idx as u32 & 0xFF) << 24) | local;
    raw.to_le_bytes()
}

/// Replace all face-related subrecords (NAM7, PNAM, HCLF, FMIN, DOFT/SOFT) in an FO4 NPC_ record.
fn replace_face_fields_in_record(
    record: &mut Record,
    pnam_refs: &[(String, String)], // (plugin, object_id)
    hclf_ref: Option<(&str, &str)>, // hair color (plugin, object_id)
    bone_offsets: &BoneOffsets,     // small_index → [x,y,z]
    body_weight: f32,
    named_bones: &[NamedBone],
    masters: &[String],
    interner: &StringInterner,
) {
    use smallvec::SmallVec;

    // Remove existing face-related subrecords from the record
    const FACE_SIGS: &[&str] = &[
        "PNAM", "HCLF", "BCLF", "DOFT", "SOFT", "MSDK", "MSDV", "MRSV", "FMRI", "FMRS", "FMIN",
        "NAM7",
    ];

    record
        .fields
        .retain(|f| !FACE_SIGS.contains(&f.sig.as_str()));

    // NAM7 — body weight
    let nam7_sig = crate::ids::SubrecordSig::from_str("NAM7").expect("NAM7 is 4 bytes");
    let weight_bytes: SmallVec<[u8; 32]> = body_weight.to_le_bytes().iter().copied().collect();
    record.fields.push(FieldEntry {
        sig: nam7_sig,
        value: FieldValue::Bytes(weight_bytes),
    });

    // PNAM — head parts (one per head part, repeatable)
    for (plugin, object_id) in pnam_refs {
        let pnam_sig = crate::ids::SubrecordSig::from_str("PNAM").expect("PNAM is 4 bytes");
        let bytes: SmallVec<[u8; 32]> = pack_formid(plugin, object_id, masters)
            .iter()
            .copied()
            .collect();
        record.fields.push(FieldEntry {
            sig: pnam_sig,
            value: FieldValue::Bytes(bytes),
        });
    }

    // HCLF — hair color form key
    if let Some((plugin, object_id)) = hclf_ref {
        let hclf_sig = crate::ids::SubrecordSig::from_str("HCLF").expect("HCLF is 4 bytes");
        let bytes: SmallVec<[u8; 32]> = pack_formid(plugin, object_id, masters)
            .iter()
            .copied()
            .collect();
        record.fields.push(FieldEntry {
            sig: hclf_sig,
            value: FieldValue::Bytes(bytes),
        });
    }

    // FMIN — facial morph intensity (f32 = 1.0)
    let fmin_sig = crate::ids::SubrecordSig::from_str("FMIN").expect("FMIN is 4 bytes");
    let fmin_bytes: SmallVec<[u8; 32]> = 1.0f32.to_le_bytes().iter().copied().collect();
    record.fields.push(FieldEntry {
        sig: fmin_sig,
        value: FieldValue::Bytes(fmin_bytes),
    });

    // DOFT — bone offsets: each is a pair of DOFT (index u16, 2 bytes padding?) + SOFT (position)
    // FO4 format: DOFT = u16 small_index, SOFT = struct { x:f32, y:f32, z:f32, unknown:u64 }
    let mut sorted_offsets: Vec<(u32, [f32; 3])> =
        bone_offsets.iter().map(|(&k, &v)| (k, v)).collect();
    sorted_offsets.sort_by_key(|&(k, _)| k);

    for (small_index, [x, y, z]) in sorted_offsets {
        let doft_sig = crate::ids::SubrecordSig::from_str("DOFT").expect("DOFT is 4 bytes");
        let soft_sig = crate::ids::SubrecordSig::from_str("SOFT").expect("SOFT is 4 bytes");

        // DOFT = u16 index
        let doft_bytes: SmallVec<[u8; 32]> =
            (small_index as u16).to_le_bytes().iter().copied().collect();
        record.fields.push(FieldEntry {
            sig: doft_sig,
            value: FieldValue::Bytes(doft_bytes),
        });

        // SOFT = f32 x, f32 y, f32 z, u64 unknown (0)
        let mut soft_bytes: SmallVec<[u8; 32]> = SmallVec::new();
        soft_bytes.extend_from_slice(&x.to_le_bytes());
        soft_bytes.extend_from_slice(&y.to_le_bytes());
        soft_bytes.extend_from_slice(&z.to_le_bytes());
        soft_bytes.extend_from_slice(&0u64.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: soft_sig,
            value: FieldValue::Bytes(soft_bytes),
        });
    }

    let _ = (named_bones, interner); // suppress unused warnings
}

// ---------------------------------------------------------------------------
// Cached bake results for threaded fanout
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct BakeResult {
    facegeom_relpath: String,
    facetint_relpath: String,
    bone_offsets: BoneOffsets,
    body_weight_triple: (f32, f32, f32),
    hair_fo4_ref: Option<(String, String)>, // (plugin, object_id)
}

// ---------------------------------------------------------------------------
// Phase implementation
// ---------------------------------------------------------------------------

pub struct ConvertFacePhase;

#[derive(Default)]
struct TargetFaceAssets {
    male_head: Option<PathBuf>,
    female_head: Option<PathBuf>,
    male_face_bones: Option<PathBuf>,
    female_face_bones: Option<PathBuf>,
}

impl TargetFaceAssets {
    fn from_params(params: &JsonValue) -> Self {
        let path = |key: &str| {
            params
                .get(key)
                .and_then(JsonValue::as_str)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Self {
            male_head: path("target_base_head_male"),
            female_head: path("target_base_head_female"),
            male_face_bones: path("target_face_bones_male"),
            female_face_bones: path("target_face_bones_female"),
        }
    }

    fn head(&self, sex: &str) -> Option<&Path> {
        if sex == "female" {
            self.female_head.as_deref()
        } else {
            self.male_head.as_deref()
        }
    }

    fn face_bones(&self, sex: &str) -> Option<&Path> {
        if sex == "female" {
            self.female_face_bones.as_deref()
        } else {
            self.male_face_bones.as_deref()
        }
    }
}

impl Phase for ConvertFacePhase {
    fn name(&self) -> &'static str {
        "convert_face"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        // --- Parse params ---
        let npc_form_key_strs: Vec<String> = p
            .get("npc_form_keys")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let source_extracted = p
            .get("source_extracted")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| ctx.source_extracted_dir.to_path_buf());

        let target_extracted: PathBuf = p
            .get("target_extracted")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| ctx.target_extracted_dir.map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("/nonexistent"));
        let target_face_assets = TargetFaceAssets::from_params(p);

        let output_plugin_name = p
            .get("output_plugin_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Output.esp")
            .to_string();

        let correspondence_path_male: Option<PathBuf> = p
            .get("correspondence_path_male")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let correspondence_path_female: Option<PathBuf> = p
            .get("correspondence_path_female")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        let uv_lut_path_male: Option<PathBuf> = p
            .get("uv_lut_path_male")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let uv_lut_path_female: Option<PathBuf> = p
            .get("uv_lut_path_female")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);

        // Load named bones
        let named_bones_yaml = if let Some(path) = p
            .get("named_bones_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            std::fs::read_to_string(path)
                .map_err(|e| PhaseError::BadParams(format!("named_bones_path: {e}")))?
        } else {
            EMBEDDED_NAMED_BONES_YAML.to_string()
        };
        let named_bones = load_named_bones(&named_bones_yaml)
            .map_err(|e| PhaseError::Internal(format!("named_bones: {e}")))?;

        // Load hair table
        let hair_table_yaml = if let Some(path) = p
            .get("hair_table_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            std::fs::read_to_string(path)
                .map_err(|e| PhaseError::BadParams(format!("hair_table_path: {e}")))?
        } else {
            EMBEDDED_HAIR_LOOKUP_YAML.to_string()
        };
        let hair_table = HairTable::load(&hair_table_yaml)
            .map_err(|e| PhaseError::Internal(format!("hair_table: {e}")))?;

        let source_handle_id = ctx.run.source_handle_id;
        let target_handle_id = ctx.run.target_handle_id;
        let schema_source = ctx.run.schema_source.clone();
        let schema_target = ctx.run.schema_target.clone();
        let mod_path = ctx.mod_path.to_path_buf();

        let mut records_changed: u32 = 0;
        let mut records_dropped: u32 = 0;
        let mut warnings: u32 = 0;

        let total = npc_form_key_strs.len() as u32;

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
            phase: self.name(),
            current: 0,
            total,
            item: None,
        });

        for (i, fk_str) in npc_form_key_strs.iter().enumerate() {
            ctx.check_cancel()?;

            let mut interner = StringInterner::new();

            // Read source NPC record
            let source_record =
                match read_record(source_handle_id, fk_str, &schema_source, &mut interner) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[convert_face] WARN: cannot read source NPC {fk_str}: {e}");
                        warnings += 1;
                        continue;
                    }
                };

            let eid = source_record
                .eid
                .and_then(|sym| interner.resolve(sym))
                .unwrap_or("")
                .to_string();

            let is_female = extract_is_female(&source_record);
            let sex = if is_female { "female" } else { "male" };

            let race_ref = extract_rnam_ref(&source_record, &interner);
            let race_class = if let Some((_, ref oid)) = race_ref {
                classify_race(oid)
            } else {
                RaceClass::Unknown
            };

            let coefficients = extract_fggs_coefficients(&source_record).unwrap_or_default();

            // Read target NPC record (target handle uses the same form key for now)
            let mut target_record =
                match read_record(target_handle_id, fk_str, &schema_target, &mut interner) {
                    Ok(r) => r,
                    Err(_) => {
                        warnings += 1;
                        continue;
                    }
                };

            let body_weight = extract_nam7_weight(&source_record);
            let hair_ref_source = extract_hnam_ref(&source_record, &interner);

            if !matches!(race_class, RaceClass::HumanMale | RaceClass::HumanFemale) {
                // Degrade: use race defaults
                records_dropped += 1;
                apply_race_default_fields(&mut target_record, &hair_table, race_class, body_weight);
                if let Err(e) =
                    add_record_native(target_handle_id, target_record, &schema_target, &interner)
                {
                    eprintln!("[convert_face] WARN: write degraded NPC {fk_str}: {e}");
                }
                continue;
            }

            if !should_attempt_bake(race_class, &coefficients) {
                // Zero coefficients — degrade with race defaults
                records_dropped += 1;
                apply_race_default_fields(&mut target_record, &hair_table, race_class, body_weight);
                if let Err(e) =
                    add_record_native(target_handle_id, target_record, &schema_target, &interner)
                {
                    eprintln!("[convert_face] WARN: write zero-coeff NPC {fk_str}: {e}");
                }
                continue;
            }

            // Attempt face bake
            let formid_hex = normalize_formid_hex(fk_str);
            let source_plugin_name = fk_str.split(':').nth(1).unwrap_or("").to_string();

            let bake_result = attempt_face_bake(
                &formid_hex,
                &coefficients,
                sex,
                &source_extracted,
                &target_extracted,
                &target_face_assets,
                &mod_path,
                &output_plugin_name,
                &source_plugin_name,
                correspondence_path_male.as_deref(),
                correspondence_path_female.as_deref(),
                uv_lut_path_male.as_deref(),
                uv_lut_path_female.as_deref(),
                &named_bones,
            );

            match bake_result {
                Ok(result) => {
                    // Assemble baked fields onto target record
                    let hair_fo4 = hair_table
                        .lookup(hair_ref_source.as_ref(), race_class)
                        .map(|r| (r.plugin.as_str(), r.object_id.as_str()));

                    let pnam_refs: Vec<(String, String)> = Vec::new(); // Race defaults via external call
                    // We need to use race defaults for PNAM since we don't have target master access
                    // in the pure Rust phase — emit the hair ref
                    let hclf = result
                        .hair_fo4_ref
                        .as_ref()
                        .map(|(p, o)| (p.as_str(), o.as_str()))
                        .or(hair_fo4);

                    let masters: Vec<String> = Vec::new(); // We use raw bytes for now

                    replace_face_fields_in_record(
                        &mut target_record,
                        &pnam_refs,
                        hclf,
                        &result.bone_offsets,
                        body_weight,
                        &named_bones,
                        &masters,
                        &interner,
                    );

                    if let Err(e) = add_record_native(
                        target_handle_id,
                        target_record,
                        &schema_target,
                        &interner,
                    ) {
                        eprintln!("[convert_face] WARN: write baked NPC {fk_str}: {e}");
                        warnings += 1;
                    } else {
                        records_changed += 1;
                    }
                }
                Err(e) => {
                    eprintln!("[convert_face] WARN [{eid}] bake failed: {e}");
                    warnings += 1;
                    // Degrade
                    apply_race_default_fields(
                        &mut target_record,
                        &hair_table,
                        race_class,
                        body_weight,
                    );
                    let _ = add_record_native(
                        target_handle_id,
                        target_record,
                        &schema_target,
                        &interner,
                    );
                    records_dropped += 1;
                }
            }

            let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                phase: self.name(),
                current: (i + 1) as u32,
                total,
                item: Some(eid.clone()),
            });
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "convert_face: {records_changed} baked, {records_dropped} degraded, {warnings} failed"
            ),
        });

        Ok(PhaseReport {
            records_changed,
            records_dropped,
            warnings,
            ..Default::default()
        })
    }
}

fn apply_race_default_fields(
    record: &mut Record,
    hair_table: &HairTable,
    race_class: RaceClass,
    body_weight: f32,
) {
    use smallvec::SmallVec;

    const FACE_SIGS: &[&str] = &[
        "PNAM", "HCLF", "BCLF", "DOFT", "SOFT", "MSDK", "MSDV", "MRSV", "FMRI", "FMRS", "FMIN",
        "NAM7",
    ];
    record
        .fields
        .retain(|f| !FACE_SIGS.contains(&f.sig.as_str()));

    // FMIN = 1.0
    let fmin_sig = crate::ids::SubrecordSig::from_str("FMIN").expect("FMIN is 4 bytes");
    let fmin_bytes: SmallVec<[u8; 32]> = 1.0f32.to_le_bytes().iter().copied().collect();
    record.fields.push(FieldEntry {
        sig: fmin_sig,
        value: FieldValue::Bytes(fmin_bytes),
    });

    // NAM7 — body weight
    let nam7_sig = crate::ids::SubrecordSig::from_str("NAM7").expect("NAM7 is 4 bytes");
    let w_bytes: SmallVec<[u8; 32]> = body_weight.to_le_bytes().iter().copied().collect();
    record.fields.push(FieldEntry {
        sig: nam7_sig,
        value: FieldValue::Bytes(w_bytes),
    });

    // Hair table default — this is only metadata, actual head parts come from target masters
    let _ = (hair_table, race_class); // suppressed — head parts require target master access
}

/// Perform the full geometry bake for one NPC.
#[allow(clippy::too_many_arguments)]
fn attempt_face_bake(
    formid_hex: &str,
    coefficients: &[f32],
    sex: &str,
    source_extracted: &Path,
    target_extracted: &Path,
    target_face_assets: &TargetFaceAssets,
    mod_path: &Path,
    output_plugin_name: &str,
    source_plugin_name: &str,
    correspondence_path_male: Option<&Path>,
    correspondence_path_female: Option<&Path>,
    uv_lut_path_male: Option<&Path>,
    uv_lut_path_female: Option<&Path>,
    named_bones: &[NamedBone],
) -> Result<BakeResult, String> {
    // Resolve paths
    let source_head_nif = find_source_head_nif(source_extracted, sex).ok_or_else(|| {
        format!(
            "source head NIF not found in {}",
            source_extracted.display()
        )
    })?;
    let source_head_egm = find_source_head_egm(source_extracted, sex).ok_or_else(|| {
        format!(
            "source head EGM not found in {}",
            source_extracted.display()
        )
    })?;
    let target_base_head_nif = target_face_assets
        .head(sex)
        .map(Path::to_path_buf)
        .or_else(|| find_target_base_head_nif(target_extracted, sex))
        .ok_or_else(|| {
            format!(
                "target base head NIF not found in {}",
                target_extracted.display()
            )
        })?;

    // Reconstruct FNV face
    let fnv_vertices = reconstruct_fnv_face(&source_head_nif, &source_head_egm, coefficients)?;

    // Choose correspondence path
    let corr_path = if sex == "female" {
        correspondence_path_female
    } else {
        correspondence_path_male
    }
    .ok_or("No correspondence path provided for this sex")?;

    let correspondence = Correspondence::load(corr_path)?;
    let fo4_vertices = correspondence.interpolate(&fnv_vertices)?;

    // Load FO4 neutral vertices
    let fo4_neutral = load_fo4_neutral_vertices(&target_base_head_nif)?;

    if fo4_vertices.len() != fo4_neutral.len() {
        return Err(format!(
            "Correspondence output {} != FO4 neutral vertex count {}",
            fo4_vertices.len(),
            fo4_neutral.len()
        ));
    }

    // Compute deformation
    let deformation: Vec<[f32; 3]> = fo4_vertices
        .iter()
        .zip(fo4_neutral.iter())
        .map(|(v, n)| [v[0] - n[0], v[1] - n[1], v[2] - n[2]])
        .collect();

    // Write facegeom NIF
    let facegeom_rel = facegeom_relpath(output_plugin_name, formid_hex);
    let facegeom_out = mod_path
        .join("data")
        .join(facegeom_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    write_facegeom_nif(&target_base_head_nif, &deformation, &facegeom_out)?;

    // Solve bone offsets
    let bone_offsets = if let Some(bones_nif_path) = target_face_assets
        .face_bones(sex)
        .map(Path::to_path_buf)
        .or_else(|| find_target_face_bones_nif(target_extracted, sex))
    {
        match extract_face_bones_skin(&bones_nif_path) {
            Ok((weights, bone_indices, bone_names)) => solve_bone_offsets(
                &fo4_neutral,
                &fo4_vertices,
                &weights,
                &bone_indices,
                &bone_names,
                named_bones,
            ),
            Err(e) => {
                eprintln!("[convert_face] WARN: bone solve failed: {e}");
                BoneOffsets::new()
            }
        }
    } else {
        BoneOffsets::new()
    };

    // Write facetint DDS
    let facetint_rel = facetint_relpath(output_plugin_name, formid_hex);
    let facetint_out = mod_path
        .join("data")
        .join(facetint_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let fallback_color = deterministic_facetint_color(formid_hex);
    let source_dds = find_source_facetint_dds(source_extracted, source_plugin_name, formid_hex);
    let uv_lut_path = if sex == "female" {
        uv_lut_path_female
    } else {
        uv_lut_path_male
    };
    if let Some(lut_path) = uv_lut_path {
        let _ = write_facetint_dds(
            &facetint_out,
            source_dds.as_deref(),
            lut_path,
            fallback_color,
        );
    } else {
        let _ = write_fallback_facetint_dds(&facetint_out, fallback_color);
    }

    let body_weight_triple = fnv_weight_to_fo4_morphs(
        coefficients.last().copied().unwrap_or(0.0), // placeholder — actual weight from NAM7
    );

    Ok(BakeResult {
        facegeom_relpath: facegeom_rel,
        facetint_relpath: facetint_rel,
        bone_offsets,
        body_weight_triple,
        hair_fo4_ref: None, // resolved via hair_table at call site
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── NPY parser ──────────────────────────────────────────────────────────

    fn make_npy_f32(shape: &[usize], data: &[f32]) -> Vec<u8> {
        // Build a minimal NPY v1.0 file
        let total: usize = shape.iter().product();
        assert_eq!(data.len(), total);
        let shape_str = if shape.len() == 1 {
            format!("({},)", shape[0])
        } else {
            let parts: Vec<String> = shape.iter().map(|n| n.to_string()).collect();
            format!("({})", parts.join(", "))
        };
        let header = format!("{{'descr': '<f4', 'fortran_order': False, 'shape': {shape_str}, }}");
        // Pad header to multiple of 64
        let header_raw = header.as_bytes();
        let header_len_value = ((header_raw.len() + 1 + 63) / 64) * 64; // +1 for newline
        let mut padded_header = vec![b' '; header_len_value];
        padded_header[..header_raw.len()].copy_from_slice(header_raw);
        padded_header[header_len_value - 1] = b'\n';

        let mut out = vec![
            0x93, b'N', b'U', b'M', b'P', b'Y', // magic
            1, 0, // version
        ];
        let len_u16 = header_len_value as u16;
        out.extend_from_slice(&len_u16.to_le_bytes());
        out.extend_from_slice(&padded_header);
        for &f in data {
            out.extend_from_slice(&f.to_le_bytes());
        }
        out
    }

    fn make_npy_i32(shape: &[usize], data: &[i32]) -> Vec<u8> {
        let total: usize = shape.iter().product();
        assert_eq!(data.len(), total);
        let shape_str = if shape.len() == 1 {
            format!("({},)", shape[0])
        } else {
            let parts: Vec<String> = shape.iter().map(|n| n.to_string()).collect();
            format!("({})", parts.join(", "))
        };
        let header = format!("{{'descr': '<i4', 'fortran_order': False, 'shape': {shape_str}, }}");
        let header_raw = header.as_bytes();
        let header_len_value = ((header_raw.len() + 1 + 63) / 64) * 64;
        let mut padded_header = vec![b' '; header_len_value];
        padded_header[..header_raw.len()].copy_from_slice(header_raw);
        padded_header[header_len_value - 1] = b'\n';

        let mut out = vec![0x93, b'N', b'U', b'M', b'P', b'Y', 1, 0];
        out.extend_from_slice(&(header_len_value as u16).to_le_bytes());
        out.extend_from_slice(&padded_header);
        for &i in data {
            out.extend_from_slice(&i.to_le_bytes());
        }
        out
    }

    #[test]
    fn parse_npy_f32_roundtrip() {
        let data = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let bytes = make_npy_f32(&[2, 3], &data);
        let (floats, shape) = parse_npy_f32(&bytes).unwrap();
        assert_eq!(shape, vec![2, 3]);
        assert_eq!(floats.len(), 6);
        for (a, &b) in floats.iter().zip(data.iter()) {
            assert!((a - b).abs() < 1e-6, "{a} != {b}");
        }
    }

    #[test]
    fn parse_npy_i32_roundtrip() {
        let data = [0i32, 1, 2, 3, 4, 5];
        let bytes = make_npy_i32(&[3, 2], &data);
        let (ints, shape) = parse_npy_i32(&bytes).unwrap();
        assert_eq!(shape, vec![3, 2]);
        assert_eq!(ints, data.to_vec());
    }

    // ── Race classification ──────────────────────────────────────────────────

    #[test]
    fn classify_human_male_races() {
        for oid in ["000019", "000023", "00001b", "00f43d"] {
            assert_eq!(classify_race(oid), RaceClass::HumanMale, "oid={oid}");
        }
    }

    #[test]
    fn classify_human_female_races() {
        for oid in ["00001a", "00001c", "000024", "0038e9"] {
            assert_eq!(classify_race(oid), RaceClass::HumanFemale, "oid={oid}");
        }
    }

    #[test]
    fn classify_unknown_race() {
        assert_eq!(classify_race("ffffff"), RaceClass::Unknown);
    }

    // ── should_attempt_bake ──────────────────────────────────────────────────

    #[test]
    fn should_bake_human_male_with_nonzero_coeffs() {
        let coeffs = vec![0.5f32; 50];
        assert!(should_attempt_bake(RaceClass::HumanMale, &coeffs));
    }

    #[test]
    fn should_not_bake_zero_coeffs() {
        let coeffs = vec![0.0f32; 50];
        assert!(!should_attempt_bake(RaceClass::HumanMale, &coeffs));
    }

    #[test]
    fn should_not_bake_wrong_count() {
        let coeffs = vec![0.5f32; 49];
        assert!(!should_attempt_bake(RaceClass::HumanMale, &coeffs));
    }

    // ── Bone solve ───────────────────────────────────────────────────────────

    #[test]
    fn solve_bone_offsets_basic() {
        let rest = vec![[0.0f32, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]];
        let deformed = vec![[1.0, 1.0, 1.0], [4.0, 2.0, 0.0], [3.0, 5.0, 7.0]];
        // weights: vert 0 → bone 1 (w=0.7), vert 1 → bone 2 (w=0.8), vert 2 → bone 2 (w=0.65)
        let weights = vec![
            vec![0.1, 0.7, 0.0, 0.0],  // vert 0: j0=bone1 w=0.1, j1=bone0 w=0.7
            vec![0.8, 0.2, 0.0, 0.0],  // vert 1: j0=bone2 w=0.8, j1=bone0 w=0.2
            vec![0.65, 0.1, 0.0, 0.0], // vert 2: j0=bone2 w=0.65, j1=bone2 w=0.1
        ];
        let bone_indices = vec![
            vec![1i32, 0, 0, 0], // vert 0: j0=bone1, j1=bone0
            vec![2, 0, 0, 0],    // vert 1: j0=bone2, j1=bone0
            vec![2, 2, 0, 0],    // vert 2: j0=bone2, j1=bone2
        ];
        let bone_names = vec!["Jaw".to_string(), "Spine".to_string(), "Root".to_string()];
        let named_bones = vec![
            NamedBone {
                name: "Root".into(),
                small_index: 25,
                weight_threshold: 0.5,
            },
            NamedBone {
                name: "Jaw".into(),
                small_index: 7,
                weight_threshold: 0.6,
            },
        ];

        let offsets = solve_bone_offsets(
            &rest,
            &deformed,
            &weights,
            &bone_indices,
            &bone_names,
            &named_bones,
        );

        // Root (bone 2, threshold 0.5): vert 1 qualifies (w=0.8), vert 2 qualifies (w=0.65)
        // Average of deformation [4,2,0] + [3,5,7] = [3.5, 3.5, 3.5]
        let root = offsets[&25];
        assert!((root[0] - 3.5).abs() < 1e-5, "root x={}", root[0]);

        // Jaw (skin_idx=0, threshold 0.6): vert 0 qualifies via j=1 (bone_idx=0, w=0.7 ≥ 0.6) → delta=[1,1,1]
        let jaw = offsets[&7];
        assert!((jaw[0] - 1.0).abs() < 1e-5, "jaw x={}", jaw[0]);
    }

    // ── formid_hex normalization ─────────────────────────────────────────────

    #[test]
    fn normalize_formid_hex_strips_plugin() {
        assert_eq!(normalize_formid_hex("000800:FalloutNV.esm"), "00000800");
    }

    #[test]
    fn normalize_formid_hex_plain() {
        assert_eq!(normalize_formid_hex("000800"), "00000800");
    }

    // ── FaceGeom / FaceTint path helpers ────────────────────────────────────

    #[test]
    fn facegeom_relpath_format() {
        let rel = facegeom_relpath("B21_Test.esp", "00000800");
        assert_eq!(
            rel,
            "Meshes/Actors/Character/FaceGenData/FaceGeom/B21_Test.esp/00000800.nif"
        );
    }

    #[test]
    fn facetint_relpath_format() {
        let rel = facetint_relpath("B21_Test.esp", "00000800");
        assert_eq!(
            rel,
            "Textures/Actors/Character/FaceGenData/FaceTint/B21_Test.esp/00000800.dds"
        );
    }

    // ── Hair table ──────────────────────────────────────────────────────────

    #[test]
    fn hair_table_loads_embedded() {
        let table = HairTable::load(EMBEDDED_HAIR_LOOKUP_YAML).unwrap();
        assert!(
            table.male_default.is_some(),
            "male_default should be present"
        );
        assert!(
            table.female_default.is_some(),
            "female_default should be present"
        );
    }

    #[test]
    fn hair_table_lookup_race_default() {
        let table = HairTable::load(EMBEDDED_HAIR_LOOKUP_YAML).unwrap();
        let result = table.lookup(None, RaceClass::HumanMale);
        assert!(result.is_some(), "Should find male default");
        let r = result.unwrap();
        assert_eq!(r.plugin.to_ascii_lowercase(), "fallout4.esm");
    }

    // ── Named bones ─────────────────────────────────────────────────────────

    #[test]
    fn named_bones_load_embedded() {
        let bones = load_named_bones(EMBEDDED_NAMED_BONES_YAML).unwrap();
        assert!(bones.len() >= 12, "expected ≥12 bones, got {}", bones.len());
        let names: Vec<&str> = bones.iter().map(|b| b.name.as_str()).collect();
        assert!(
            names.contains(&"skin_bone_C_Chin"),
            "missing skin_bone_C_Chin"
        );
    }

    // ── Body weight mapping ──────────────────────────────────────────────────

    #[test]
    fn body_weight_positive_is_fat() {
        let (thin, musc, fat) = fnv_weight_to_fo4_morphs(0.5);
        assert_eq!(thin, 0.0);
        assert_eq!(musc, 0.0);
        assert!((fat - 0.5).abs() < 1e-6);
    }

    #[test]
    fn body_weight_negative_is_thin() {
        let (thin, musc, fat) = fnv_weight_to_fo4_morphs(-0.5);
        assert!((thin - 0.5).abs() < 1e-6);
        assert_eq!(musc, 0.0);
        assert_eq!(fat, 0.0);
    }

    // ── Empty NPC list → zero report ────────────────────────────────────────

    #[test]
    fn convert_face_empty_list_produces_zero_report() {
        use crate::phase::PhaseCtx;
        use crate::run::{
            ConversionRun, RunConfig, RunError, RunParams, create_run, drop_run, with_run,
        };
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let id = create_run(RunParams {
            source: Game::Fnv,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let report = with_run(id, |run| -> Result<crate::phase::PhaseReport, RunError> {
            let cancel = std::sync::Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "npc_form_keys": [],
                "source_extracted": "/nonexistent",
                "output_plugin_name": "Output.esp"
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertFacePhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.records_changed, 0);
        assert_eq!(report.records_dropped, 0);
        assert_eq!(report.warnings, 0);
        drop_run(id).unwrap();
    }
}
