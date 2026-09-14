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
//   "correspondence_path_ghoul_male":   "/path/fnv_to_fo4_correspondence_ghoul_male.npz",
//   "correspondence_path_ghoul_female": "/path/fnv_to_fo4_correspondence_ghoul_female.npz",
//   "uv_lut_path_ghoul_male":   "/path/fnv_to_fo4_facetint_uv_lut_ghoul_male.npz",
//   "uv_lut_path_ghoul_female": "/path/fnv_to_fo4_facetint_uv_lut_ghoul_female.npz",
//   "correspondence_path_child_male":   "/path/fnv_to_fo4_correspondence_child_male.npz",
//   "correspondence_path_child_female": "/path/fnv_to_fo4_correspondence_child_female.npz",
//   "uv_lut_path_child_male":   "/path/fnv_to_fo4_facetint_uv_lut_child_male.npz",
//   "uv_lut_path_child_female": "/path/fnv_to_fo4_facetint_uv_lut_child_female.npz",
//   "hair_table_path":    "/path/hair_lookup.yaml",   // optional; uses embedded if absent
//   "named_bones_path":   "/path/named_bones.yaml",   // optional; uses embedded if absent
//   "translation_maps_dir": "/path/translation_maps",  // required for Skyrim beast FaceGeom
//   "precomputed_bone_solves": null
// }
//
// Phase output: writes FaceGeom NIFs and FaceTint DDS files under mod_path.
// PhaseReport:
//   records_changed = NPCs successfully baked
//   records_dropped = NPCs degraded to race defaults
//   warnings        = NPCs that failed bake + are skipped

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use rayon::prelude::*;
use serde_json::Value as JsonValue;

use crate::ids::{FormKey, SigCode};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::skyrimse_fo4_runtime::humanoid::{SkyrimHumanoidFaceKind, source_humanoid_face_kind};
use crate::source_read::{
    form_key_to_read_str, iter_form_keys_of_sig, parse_form_key_str, read_record,
};
use crate::source_rig::{
    CreatureAncillaryNpcArtifactKind, CreatureAncillaryNpcArtifactReceipt,
    CreatureAncillaryNpcFacegenGenerationReceipt, CreatureAncillaryNpcSourceArtifactKind,
    CreatureAncillaryNpcSourceArtifactReceipt, CreatureAncillaryNpcTemplateReceipt, TargetFormKey,
    creature_ancillary_npc_facegen_generation_receipt,
};
use crate::sym::StringInterner;
use crate::target_write::replace_records_native;

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
const FNV_GHOUL_IDS: &[&str] = &["003b3e", "0083d7"];
const FNV_CHILD_IDS: &[&str] = &["0042be", "0042c0", "0042c2", "0042c4"];
const FNV_OLD_RACE_LOCALS: &[u32] = &[
    0x0000_42BF,
    0x0000_42C1,
    0x0000_42C3,
    0x0000_42C5,
    0x0009_87DC,
    0x0009_87DD,
    0x0009_87DE,
    0x0009_87DF,
];
const FO4_HUMAN_RACE_LOCAL: u32 = 0x0001_3746;
const FO4_GHOUL_RACE_LOCAL: u32 = 0x000E_AFB6;
const FO4_HUMAN_CHILD_RACE_LOCAL: u32 = 0x0011_D83F;
const FO4_HUMAN_MALE_HEAD_PARTS: &[u32] = &[
    0x0001_F735,
    0x0005_1631,
    0x0014_108D,
    0x0004_23AF,
    0x0008_44A7,
    0x0011_CCBD,
    0x0001_EEBB,
];
const FO4_HUMAN_FEMALE_HEAD_PARTS: &[u32] = &[
    0x001E_55C4,
    0x0004_D0EC,
    0x000F_159E,
    0x0014_EC22,
    0x0004_D0E9,
    0x000C_FB4E,
    0x000C_FB3F,
];
const FO4_GHOUL_MALE_HEAD_PARTS: &[u32] = &[0x0014_1091, 0x0017_A5EA, 0x000E_AFBB];
const FO4_GHOUL_FEMALE_HEAD_PARTS: &[u32] = &[
    0x0014_4A9F,
    0x0014_4A9E,
    0x0004_D0E9,
    0x000C_FB4E,
    0x0017_A5EB,
    0x0014_EC22,
    0x000F_159E,
    0x0004_D0EC,
    0x000F_6E5A,
];
const FO4_CHILD_MALE_HEAD_PARTS: &[u32] = &[0x0024_7507, 0x0017_B410];
const FO4_CHILD_FEMALE_HEAD_PARTS: &[u32] = &[0x0024_7508, 0x0017_B417];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RaceClass {
    HumanMale,
    HumanFemale,
    GhoulMale,
    GhoulFemale,
    ChildMale,
    ChildFemale,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SourceHeadProfile {
    Standard,
    Old,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyCreatureFacegenProfile {
    HumanMale,
    HumanFemale,
    GhoulMale,
    GhoulFemale,
    ChildMale,
    ChildFemale,
}

impl LegacyCreatureFacegenProfile {
    fn race_class(self) -> RaceClass {
        match self {
            Self::HumanMale => RaceClass::HumanMale,
            Self::HumanFemale => RaceClass::HumanFemale,
            Self::GhoulMale => RaceClass::GhoulMale,
            Self::GhoulFemale => RaceClass::GhoulFemale,
            Self::ChildMale => RaceClass::ChildMale,
            Self::ChildFemale => RaceClass::ChildFemale,
        }
    }

    fn resource_name(self) -> &'static str {
        match self {
            Self::HumanMale => "male",
            Self::HumanFemale => "female",
            Self::GhoulMale => "ghoul_male",
            Self::GhoulFemale => "ghoul_female",
            Self::ChildMale => "child_male",
            Self::ChildFemale => "child_female",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LegacyCreatureFacegenSourceArtifactInput {
    pub path: PathBuf,
    pub receipt: CreatureAncillaryNpcSourceArtifactReceipt,
}

#[derive(Debug, Clone)]
pub(crate) struct LegacyCreatureFacegenPartInput {
    pub source_model: Option<LegacyCreatureFacegenSourceArtifactInput>,
    pub target_model_nif: PathBuf,
    pub shapes: Vec<LegacyCreatureFacegenShapeInput>,
}

#[derive(Debug, Clone)]
pub(crate) struct LegacyCreatureFacegenShapeInput {
    pub source_shape_name: String,
    pub target_shape_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct LegacyCreatureFacegenBuildInput {
    pub profile: LegacyCreatureFacegenProfile,
    pub target_npc: TargetFormKey,
    pub template_receipt: CreatureAncillaryNpcTemplateReceipt,
    pub target_appearance_closure_blake3: String,
    pub symmetric_morph_bytes: Vec<u8>,
    pub asymmetric_morph_bytes: Vec<u8>,
    pub source_head_tri: LegacyCreatureFacegenSourceArtifactInput,
    pub source_head_egm: LegacyCreatureFacegenSourceArtifactInput,
    pub source_facetint_dds: LegacyCreatureFacegenSourceArtifactInput,
    pub target_base_head_nif: PathBuf,
    pub target_facegeom_template_nif: PathBuf,
    pub selected_parts: Vec<LegacyCreatureFacegenPartInput>,
    pub private_staging_data_root: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct LegacyCreatureFacegenBuildOutput {
    pub artifacts: Vec<CreatureAncillaryNpcArtifactReceipt>,
    pub generation: CreatureAncillaryNpcFacegenGenerationReceipt,
}

fn classify_source_head_profile(
    source_race: Option<FormKey>,
    race_class: RaceClass,
) -> SourceHeadProfile {
    if matches!(race_class, RaceClass::HumanMale | RaceClass::HumanFemale)
        && source_race.is_some_and(|race| FNV_OLD_RACE_LOCALS.contains(&race.local))
    {
        SourceHeadProfile::Old
    } else {
        SourceHeadProfile::Standard
    }
}

impl RaceClass {
    fn is_bakeable(self) -> bool {
        !matches!(self, Self::Unknown)
    }

    fn mandatory_head_parts(self) -> &'static [u32] {
        match self {
            Self::HumanMale => FO4_HUMAN_MALE_HEAD_PARTS,
            Self::HumanFemale => FO4_HUMAN_FEMALE_HEAD_PARTS,
            Self::GhoulMale => FO4_GHOUL_MALE_HEAD_PARTS,
            Self::GhoulFemale => FO4_GHOUL_FEMALE_HEAD_PARTS,
            Self::ChildMale => FO4_CHILD_MALE_HEAD_PARTS,
            Self::ChildFemale => FO4_CHILD_FEMALE_HEAD_PARTS,
            Self::Unknown => &[],
        }
    }

    fn template_facegeom_form_id(self) -> Option<&'static str> {
        match self {
            Self::HumanMale => Some("000A0E2F"),
            Self::HumanFemale => Some("00002F1E"),
            Self::GhoulMale => Some("00022613"),
            Self::GhoulFemale => Some("00022952"),
            Self::ChildMale => Some("001338EE"),
            Self::ChildFemale => Some("00002F20"),
            Self::Unknown => None,
        }
    }

    fn primary_head_shape_name(self) -> Option<&'static str> {
        match self {
            Self::HumanMale => Some("MaleHeadHuman"),
            Self::HumanFemale => Some("FemaleHeadHuman"),
            Self::GhoulMale => Some("MaleHeadGhoul"),
            Self::GhoulFemale => Some("FemaleHeadGhoul"),
            Self::ChildMale => Some("MaleHeadHumanChild"),
            Self::ChildFemale => Some("FemaleHeadHumanChild"),
            Self::Unknown => None,
        }
    }

    fn template_hair_color(self) -> Option<u32> {
        match self {
            Self::HumanMale => Some(0x000A_042F),
            Self::HumanFemale => Some(0x0019_EE5E),
            Self::GhoulMale => Some(0x000A_042E),
            Self::GhoulFemale => Some(0x000A_042D),
            Self::ChildMale | Self::ChildFemale => Some(0x0019_EE67),
            Self::Unknown => None,
        }
    }
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

fn classify_source_race(object_id: &str, is_female: bool) -> RaceClass {
    let human = classify_race(object_id);
    if human != RaceClass::Unknown {
        return human;
    }
    let lower = object_id.trim().to_ascii_lowercase();
    if FNV_GHOUL_IDS.contains(&lower.as_str()) {
        return if is_female {
            RaceClass::GhoulFemale
        } else {
            RaceClass::GhoulMale
        };
    }
    if FNV_CHILD_IDS.contains(&lower.as_str()) {
        return if is_female {
            RaceClass::ChildFemale
        } else {
            RaceClass::ChildMale
        };
    }
    RaceClass::Unknown
}

fn classify_mapped_race(target_race: Option<FormKey>, is_female: bool) -> RaceClass {
    match target_race.map(|race| race.local) {
        Some(FO4_HUMAN_RACE_LOCAL) if is_female => RaceClass::HumanFemale,
        Some(FO4_HUMAN_RACE_LOCAL) => RaceClass::HumanMale,
        Some(FO4_GHOUL_RACE_LOCAL) if is_female => RaceClass::GhoulFemale,
        Some(FO4_GHOUL_RACE_LOCAL) => RaceClass::GhoulMale,
        Some(FO4_HUMAN_CHILD_RACE_LOCAL) if is_female => RaceClass::ChildFemale,
        Some(FO4_HUMAN_CHILD_RACE_LOCAL) => RaceClass::ChildMale,
        _ => RaceClass::Unknown,
    }
}

fn should_attempt_bake(race_class: RaceClass, coefficients: &[f32]) -> bool {
    if !race_class.is_bakeable() {
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
            let compressed_size_32 = u32::from_le_bytes([
                data[pos + 18],
                data[pos + 19],
                data[pos + 20],
                data[pos + 21],
            ]);
            let uncompressed_size_32 = u32::from_le_bytes([
                data[pos + 22],
                data[pos + 23],
                data[pos + 24],
                data[pos + 25],
            ]);
            let fname_len = u16::from_le_bytes([data[pos + 26], data[pos + 27]]) as usize;
            let extra_len = u16::from_le_bytes([data[pos + 28], data[pos + 29]]) as usize;
            let header_end = pos + 30 + fname_len + extra_len;
            if header_end > data.len() {
                break;
            }

            let mut compressed_size = compressed_size_32 as u64;
            let mut uncompressed_size = uncompressed_size_32 as u64;
            if compressed_size_32 == u32::MAX || uncompressed_size_32 == u32::MAX {
                let extra = &data[pos + 30 + fname_len..header_end];
                let mut extra_pos = 0usize;
                while extra_pos + 4 <= extra.len() {
                    let header_id = u16::from_le_bytes([extra[extra_pos], extra[extra_pos + 1]]);
                    let field_len =
                        u16::from_le_bytes([extra[extra_pos + 2], extra[extra_pos + 3]]) as usize;
                    extra_pos += 4;
                    if extra_pos + field_len > extra.len() {
                        break;
                    }
                    if header_id == 0x0001 {
                        let field = &extra[extra_pos..extra_pos + field_len];
                        let mut value_pos = 0usize;
                        let mut read_u64 = || -> Result<u64, String> {
                            if value_pos + 8 > field.len() {
                                return Err("ZIP64 extra field is truncated".to_string());
                            }
                            let value = u64::from_le_bytes(
                                field[value_pos..value_pos + 8].try_into().unwrap(),
                            );
                            value_pos += 8;
                            Ok(value)
                        };
                        if uncompressed_size_32 == u32::MAX {
                            uncompressed_size = read_u64()?;
                        }
                        if compressed_size_32 == u32::MAX {
                            compressed_size = read_u64()?;
                        }
                        break;
                    }
                    extra_pos += field_len;
                }
            }
            let compressed_size = usize::try_from(compressed_size)
                .map_err(|_| "ZIP compressed entry is too large".to_string())?;
            let uncompressed_size = usize::try_from(uncompressed_size)
                .map_err(|_| "ZIP uncompressed entry is too large".to_string())?;
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

    fn interpolate_deformation(
        &self,
        neutral_vertices: &[[f32; 3]],
        deformed_vertices: &[[f32; 3]],
    ) -> Result<Vec<[f32; 3]>, String> {
        if neutral_vertices.len() != deformed_vertices.len() {
            return Err(format!(
                "Neutral/deformed source vertex counts differ: {}/{}",
                neutral_vertices.len(),
                deformed_vertices.len()
            ));
        }

        let neutral = self.interpolate(neutral_vertices)?;
        let deformed = self.interpolate(deformed_vertices)?;
        Ok(deformed
            .iter()
            .zip(neutral.iter())
            .map(|(d, n)| [d[0] - n[0], d[1] - n[1], d[2] - n[2]])
            .collect())
    }

    fn nearest_normalized(
        source_vertices: &[[f32; 3]],
        target_vertices: &[[f32; 3]],
    ) -> Result<Self, String> {
        if source_vertices.len() < 3 {
            return Err("source head needs at least three vertices".to_string());
        }
        let (source_min, source_max) = vertex_bounds(source_vertices)?;
        let (target_min, target_max) = vertex_bounds(target_vertices)?;
        let normalized_source = source_vertices
            .iter()
            .map(|vertex| normalize_vertex(*vertex, source_min, source_max))
            .collect::<Vec<_>>();

        let mut triangle_indices = Vec::with_capacity(target_vertices.len());
        let mut barycentrics = Vec::with_capacity(target_vertices.len());
        for target in target_vertices {
            let target = normalize_vertex(*target, target_min, target_max);
            let mut nearest = [(f32::INFINITY, 0usize); 3];
            for (index, source) in normalized_source.iter().enumerate() {
                let distance = (source[0] - target[0]).powi(2)
                    + (source[1] - target[1]).powi(2)
                    + (source[2] - target[2]).powi(2);
                if distance < nearest[2].0 {
                    nearest[2] = (distance, index);
                    nearest.sort_by(|left, right| left.0.total_cmp(&right.0));
                }
            }

            let weights = if nearest[0].0 <= f32::EPSILON {
                [1.0, 0.0, 0.0]
            } else {
                let inverse = nearest.map(|(distance, _)| 1.0 / distance.sqrt().max(1.0e-6));
                let total = inverse.iter().sum::<f32>();
                [inverse[0] / total, inverse[1] / total, inverse[2] / total]
            };
            triangle_indices.push([
                nearest[0].1 as i32,
                nearest[1].1 as i32,
                nearest[2].1 as i32,
            ]);
            barycentrics.push(weights);
        }

        Ok(Self {
            triangle_indices,
            barycentrics,
            sample_count: target_vertices.len(),
        })
    }
}

fn vertex_bounds(vertices: &[[f32; 3]]) -> Result<([f32; 3], [f32; 3]), String> {
    let Some(first) = vertices.first().copied() else {
        return Err("vertex array is empty".to_string());
    };
    let mut minimum = first;
    let mut maximum = first;
    for vertex in vertices.iter().skip(1) {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(vertex[axis]);
            maximum[axis] = maximum[axis].max(vertex[axis]);
        }
    }
    Ok((minimum, maximum))
}

fn normalize_vertex(vertex: [f32; 3], minimum: [f32; 3], maximum: [f32; 3]) -> [f32; 3] {
    let mut normalized = [0.0; 3];
    for axis in 0..3 {
        let extent = maximum[axis] - minimum[axis];
        if extent.abs() > f32::EPSILON {
            normalized[axis] = (vertex[axis] - minimum[axis]) / extent;
        }
    }
    normalized
}

fn deformation_scale(
    source_vertices: &[[f32; 3]],
    target_vertices: &[[f32; 3]],
) -> Result<[f32; 3], String> {
    let (source_min, source_max) = vertex_bounds(source_vertices)?;
    let (target_min, target_max) = vertex_bounds(target_vertices)?;
    let mut scale = [1.0; 3];
    for axis in 0..3 {
        let source_extent = source_max[axis] - source_min[axis];
        if source_extent.abs() > f32::EPSILON {
            scale[axis] = (target_max[axis] - target_min[axis]) / source_extent;
        }
    }
    Ok(scale)
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
    // EGM spans the TRI's base vertices plus its modifier vertices.
    let vertex_count = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let modifier_vertex_count =
        u32::from_le_bytes([data[44], data[45], data[46], data[47]]) as usize;
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
    let parsed = parse_egm_basis_full(path)?;
    let symmetric_values = parsed.symmetric_count * parsed.vertex_count * 3;
    Ok((
        parsed.symmetric_count,
        parsed.vertex_count,
        parsed.basis[..symmetric_values].to_vec(),
    ))
}

#[derive(Debug)]
struct ParsedEgmBasis {
    symmetric_count: usize,
    asymmetric_count: usize,
    vertex_count: usize,
    basis: Vec<f32>,
}

fn parse_egm_basis_full(path: &Path) -> Result<ParsedEgmBasis, String> {
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
    if data.len() != expected {
        return Err(format!(
            "EGM payload length mismatch: expected {expected} bytes, got {}",
            data.len()
        ));
    }

    let mut basis = vec![0.0f32; total_morphs * num_vertices * 3];
    let mut offset = EGM_HEADER_SIZE;
    for morph_idx in 0..total_morphs {
        let scale = f32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        if !scale.is_finite() {
            return Err(format!("EGM morph {morph_idx} has a non-finite scale"));
        }
        offset += 4;
        for vert_xyz in 0..(num_vertices * 3) {
            let packed = i16::from_le_bytes([data[offset], data[offset + 1]]) as f32;
            basis[morph_idx * num_vertices * 3 + vert_xyz] = packed * scale;
            offset += 2;
        }
    }

    Ok(ParsedEgmBasis {
        symmetric_count: num_differences,
        asymmetric_count: num_asymmetric,
        vertex_count: num_vertices,
        basis,
    })
}

fn reconstruct_fnv_face_from_basis(
    neutral: &[[f32; 3]],
    num_diffs: usize,
    num_verts: usize,
    basis: &[f32],
    coefficients: &[f32],
) -> Result<Vec<[f32; 3]>, String> {
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
    if basis.len() != num_diffs * num_verts * 3 {
        return Err(format!(
            "EGM basis value count {} != expected {}",
            basis.len(),
            num_diffs * num_verts * 3
        ));
    }
    if coefficients
        .iter()
        .any(|coefficient| !coefficient.is_finite())
    {
        return Err("EGM coefficients contain a non-finite value".to_string());
    }
    if basis.iter().any(|value| !value.is_finite())
        || neutral.iter().flatten().any(|value| !value.is_finite())
    {
        return Err("EGM source geometry contains a non-finite value".to_string());
    }

    // displacement[v][xyz] = sum_k(coeff[k] * basis[k][v][xyz])
    let mut displaced = neutral.to_vec();
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

fn load_skyrim_head_vertices(nif_path: &Path) -> Result<Vec<[f32; 3]>, String> {
    use nif_core_native::model::{NifFile, NifValue};

    let nif = NifFile::load(nif_path.to_path_buf())
        .map_err(|error| format!("load Skyrim head NIF {}: {error}", nif_path.display()))?;
    let mut candidates = nif
        .blocks
        .iter()
        .filter_map(|block| {
            let name = match block.get_field("Name") {
                Some(NifValue::String(name)) => name.trim_end_matches('\0').to_ascii_lowercase(),
                _ => return None,
            };
            if !name.contains("head")
                || name.contains("hair")
                || name.contains("brow")
                || name.contains("mouth")
                || name.contains("eye")
            {
                return None;
            }
            let vertices = match block.get_field("Vertices") {
                Some(NifValue::Array(values)) => values
                    .iter()
                    .filter_map(|value| match value {
                        NifValue::Vec3(vertex) => Some(*vertex),
                        NifValue::Vec4(vertex) => Some([vertex[0], vertex[1], vertex[2]]),
                        NifValue::Struct(vertex) => Some([
                            extract_struct_f32(vertex, "x")?,
                            extract_struct_f32(vertex, "y")?,
                            extract_struct_f32(vertex, "z")?,
                        ]),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                _ => return None,
            };
            (!vertices.is_empty()).then_some(vertices)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(Vec::len);
    candidates.pop().ok_or_else(|| {
        format!(
            "no Skyrim face head shape with legacy vertices in {}",
            nif_path.display()
        )
    })
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

#[cfg(test)]
fn write_facegeom_nif(
    template_nif_path: &Path,
    neutral_vertices: &[[f32; 3]],
    deformation: &[[f32; 3]],
    race_class: RaceClass,
    out_path: &Path,
) -> Result<(), String> {
    use nif_core_native::model::{NifFile, NifValue};

    let nif = NifFile::load(template_nif_path.to_path_buf())
        .map_err(|e| format!("load FaceGen template NIF: {e}"))?;
    let schema = nif_core_native::schema::NifSchema::from_generated();
    let expected_name = race_class
        .primary_head_shape_name()
        .ok_or_else(|| "unknown race has no primary head shape".to_string())?;

    let shape_ids: Vec<usize> = (0..nif.blocks.len())
        .filter(|&i| {
            if let Some(b) = nif.get_block(i) {
                schema.is_subtype_of(&b.type_name, "BSTriShape")
                    && matches!(b.get_field("Name"), Some(NifValue::String(name)) if name.trim_end_matches('\0').eq_ignore_ascii_case(expected_name))
                    && matches!(b.get_field("Vertex Data"), Some(NifValue::Array(_)))
            } else {
                false
            }
        })
        .collect();

    if shape_ids.len() != 1 {
        return Err(format!(
            "Expected one {expected_name} shape in FaceGen template {}, got {}",
            template_nif_path.display(),
            shape_ids.len(),
        ));
    }

    write_facegeom_nif_from_template(&nif, shape_ids[0], neutral_vertices, deformation, out_path)
}

fn write_facegeom_nif_from_template(
    template: &nif_core_native::model::NifFile,
    shape_id: usize,
    neutral_vertices: &[[f32; 3]],
    deformation: &[[f32; 3]],
    out_path: &Path,
) -> Result<(), String> {
    use nif_core_native::model::NifValue;

    let mut nif = template.clone();
    let vertex_count = {
        let b = nif.get_block(shape_id).unwrap();
        match b.get_field("Vertex Data") {
            Some(NifValue::Array(arr)) => arr.len(),
            _ => return Err("Vertex Data not an array".into()),
        }
    };

    if neutral_vertices.len() != vertex_count || deformation.len() != vertex_count {
        return Err(format!(
            "Neutral/deformation vertex counts {}/{} != NIF vertex count {}",
            neutral_vertices.len(),
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
                if let Some(NifValue::Struct(_)) = s.get("Vertex") {
                    let new_vertex = {
                        let mut nv = indexmap::IndexMap::new();
                        nv.insert(
                            "x".into(),
                            NifValue::Float((neutral_vertices[i][0] + deformation[i][0]) as f64),
                        );
                        nv.insert(
                            "y".into(),
                            NifValue::Float((neutral_vertices[i][1] + deformation[i][1]) as f64),
                        );
                        nv.insert(
                            "z".into(),
                            NifValue::Float((neutral_vertices[i][2] + deformation[i][2]) as f64),
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

fn write_facetint_dds_with_uv(
    out_path: &Path,
    source_dds_path: Option<&Path>,
    uv_lut: Option<&(Vec<f32>, usize, usize)>,
    fallback_color: [u8; 4],
) -> Result<(), String> {
    if let Some(src_path) = source_dds_path {
        if src_path.is_file() {
            if let Some((uv_flat, lut_h, lut_w)) = uv_lut {
                if let Ok(src_img) = directxtex_native::read_dds_rgba_image(src_path) {
                    let remapped = remap_rgba_with_uv_lut(
                        &src_img.rgba,
                        src_img.width,
                        src_img.height,
                        uv_flat,
                        *lut_h,
                        *lut_w,
                    );
                    if let Some(parent) = out_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    return directxtex_native::write_dds_rgba_image(
                        out_path,
                        *lut_w as u32,
                        *lut_h as u32,
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

pub(crate) fn generate_legacy_creature_facegen(
    input: LegacyCreatureFacegenBuildInput,
) -> Result<LegacyCreatureFacegenBuildOutput, String> {
    if input.target_npc.local == 0
        || input.target_npc.plugin.trim().is_empty()
        || input.target_npc.plugin.contains(['/', '\\'])
    {
        return Err("legacy creature FaceGen target NPC is invalid".to_string());
    }
    if input.selected_parts.is_empty() {
        return Err("legacy creature FaceGen has no exact selected head parts".to_string());
    }
    validate_hash_text(
        &input.target_appearance_closure_blake3,
        "target appearance closure",
    )?;
    let symmetric = decode_exact_f32_payload(&input.symmetric_morph_bytes, "FGGS")?;
    let asymmetric = decode_exact_f32_payload(&input.asymmetric_morph_bytes, "FGGA")?;
    validate_source_facegen_input(
        &input.source_head_tri,
        CreatureAncillaryNpcSourceArtifactKind::Tri,
    )?;
    validate_source_facegen_input(
        &input.source_head_egm,
        CreatureAncillaryNpcSourceArtifactKind::Egm,
    )?;
    validate_source_facegen_input(
        &input.source_facetint_dds,
        CreatureAncillaryNpcSourceArtifactKind::Dds,
    )?;

    let parsed = parse_egm_basis_full(&input.source_head_egm.path)?;
    if symmetric.len() != parsed.symmetric_count || asymmetric.len() != parsed.asymmetric_count {
        return Err(format!(
            "legacy creature FaceGen morph counts FGGS {}/FGGA {} do not match EGM {}/{}",
            symmetric.len(),
            asymmetric.len(),
            parsed.symmetric_count,
            parsed.asymmetric_count
        ));
    }
    let mut coefficients = symmetric;
    coefficients.extend(asymmetric);
    let source_neutral = parse_tri_neutral_vertices(&input.source_head_tri.path)?;
    let source_deformed = reconstruct_fnv_face_from_basis(
        &source_neutral,
        parsed.symmetric_count + parsed.asymmetric_count,
        parsed.vertex_count,
        &parsed.basis,
        &coefficients,
    )?;

    let resource_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/phase/resources/face");
    let profile_name = input.profile.resource_name();
    let correspondence_path =
        resource_root.join(format!("fnv_to_fo4_correspondence_{profile_name}.npz"));
    let uv_lut_path = resource_root.join(format!("fnv_to_fo4_facetint_uv_lut_{profile_name}.npz"));
    let correspondence = Correspondence::load(&correspondence_path)?;
    let target_neutral = load_fo4_neutral_vertices(&input.target_base_head_nif)?;
    if correspondence.sample_count != target_neutral.len() {
        return Err(format!(
            "legacy creature FaceGen correspondence output {} does not match target head vertices {}",
            correspondence.sample_count,
            target_neutral.len()
        ));
    }
    let deformation = correspondence.interpolate_deformation(&source_neutral, &source_deformed)?;

    let mut template = load_fo4_facegen_nif(
        &input.target_facegeom_template_nif,
        "target FaceGeom template",
    )?;
    strip_all_face_shapes(&mut template)?;
    let face_node = find_face_node(&template)
        .ok_or_else(|| "target FaceGeom template has no face node".to_string())?;
    let mut primary_shape_ids = Vec::new();
    let mut facegeom_sources = vec![
        input.source_head_egm.receipt.clone(),
        input.source_head_tri.receipt.clone(),
    ];
    for (part_index, part) in input.selected_parts.iter().enumerate() {
        if part.shapes.is_empty() {
            return Err(format!(
                "legacy creature FaceGen part {part_index} has no exact shapes"
            ));
        }
        let target_part = load_fo4_facegen_nif(&part.target_model_nif, "target head part")?;
        let (part_nif, skin_reference) = match &part.source_model {
            Some(source) => {
                validate_source_facegen_input(source, CreatureAncillaryNpcSourceArtifactKind::Nif)?;
                let mut converted =
                    nif_core_native::model::NifFile::load(&source.path).map_err(|error| {
                        format!(
                            "load legacy creature face part {}: {error}",
                            source.path.display()
                        )
                    })?;
                if nif_core_native::convert_file::prepare_legacy_face_part_for_fo4(&mut converted)
                    == 0
                {
                    return Err(format!(
                        "legacy creature face part {} has no target-convertible geometry",
                        source.path.display()
                    ));
                }
                facegeom_sources.push(source.receipt.clone());
                (converted, Some(target_part))
            }
            None => (target_part, None),
        };
        let mut names = std::collections::BTreeSet::new();
        for shape in &part.shapes {
            let source_name = shape.source_shape_name.trim_end_matches('\0');
            let target_name = shape.target_shape_name.trim_end_matches('\0');
            if source_name.is_empty()
                || target_name.is_empty()
                || !names.insert(source_name.to_ascii_lowercase())
            {
                return Err(format!(
                    "legacy creature FaceGen part {part_index} has invalid or duplicate shape identity"
                ));
            }
            let source_shape = exact_face_shape_id(&part_nif, source_name)?;
            let copied = append_face_part_shape(
                &mut template,
                &part_nif,
                source_shape,
                face_node,
                target_name,
            )?;
            if let Some(reference) = skin_reference.as_ref() {
                nif_core_native::skin::rig_facegen_shape_from_reference(
                    &mut template,
                    copied,
                    reference,
                )?;
            }
            if target_name.eq_ignore_ascii_case(
                input
                    .profile
                    .race_class()
                    .primary_head_shape_name()
                    .ok_or_else(|| {
                        "legacy creature FaceGen profile has no head shape".to_string()
                    })?,
            ) {
                primary_shape_ids.push(copied);
            }
        }
    }
    if primary_shape_ids.len() != 1 {
        return Err(format!(
            "legacy creature FaceGen requires exactly one primary head shape, got {}",
            primary_shape_ids.len()
        ));
    }
    nif_core_native::convert_file::finalize_assembled_facegeom_for_fo4(&mut template);

    let form_id = format!("{:08X}", input.target_npc.local);
    let facegeom_runtime = facegeom_relpath(&input.target_npc.plugin, &form_id).replace('/', "\\");
    let facetint_runtime = facetint_relpath(&input.target_npc.plugin, &form_id).replace('/', "\\");
    let facegeom_path = input
        .private_staging_data_root
        .join(facegeom_runtime.replace('\\', std::path::MAIN_SEPARATOR_STR));
    let facetint_path = input
        .private_staging_data_root
        .join(facetint_runtime.replace('\\', std::path::MAIN_SEPARATOR_STR));
    write_facegeom_nif_from_template(
        &template,
        primary_shape_ids[0],
        &target_neutral,
        &deformation,
        &facegeom_path,
    )?;
    validate_fo4_facegen_output(&facegeom_path)?;

    let uv_lut = load_uv_lut(&uv_lut_path)?;
    write_facetint_dds_with_required_uv(&facetint_path, &input.source_facetint_dds.path, &uv_lut)?;

    facegeom_sources.sort_by_key(source_artifact_key);
    facegeom_sources
        .dedup_by(|left, right| source_artifact_key(left) == source_artifact_key(right));
    let mut artifacts = vec![
        artifact_receipt_from_file(
            CreatureAncillaryNpcArtifactKind::Nif,
            facegeom_sources,
            facegeom_runtime,
            &facegeom_path,
        )?,
        artifact_receipt_from_file(
            CreatureAncillaryNpcArtifactKind::Dds,
            vec![input.source_facetint_dds.receipt],
            facetint_runtime,
            &facetint_path,
        )?,
    ];
    artifacts.sort_by(|left, right| {
        left.target_data_path
            .to_ascii_lowercase()
            .cmp(&right.target_data_path.to_ascii_lowercase())
    });
    let pipeline_resources_blake3 = hash_facegen_pipeline_resources(
        &correspondence_path,
        &uv_lut_path,
        &input.target_base_head_nif,
        &input.target_facegeom_template_nif,
        &input.selected_parts,
    )?;
    let generation = creature_ancillary_npc_facegen_generation_receipt(
        &input.template_receipt,
        pipeline_resources_blake3,
        input.target_appearance_closure_blake3,
        &artifacts,
    )
    .map_err(|error| error.to_string())?;
    Ok(LegacyCreatureFacegenBuildOutput {
        artifacts,
        generation,
    })
}

fn decode_exact_f32_payload(bytes: &[u8], label: &str) -> Result<Vec<f32>, String> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Err(format!("{label} is not an exact nonempty f32 payload"));
    }
    bytes
        .chunks_exact(4)
        .enumerate()
        .map(|(index, chunk)| {
            let value = f32::from_le_bytes(chunk.try_into().unwrap());
            value
                .is_finite()
                .then_some(value)
                .ok_or_else(|| format!("{label}[{index}] is non-finite"))
        })
        .collect()
}

fn validate_source_facegen_input(
    input: &LegacyCreatureFacegenSourceArtifactInput,
    expected_kind: CreatureAncillaryNpcSourceArtifactKind,
) -> Result<(), String> {
    if input.receipt.kind != expected_kind {
        return Err(format!(
            "legacy creature FaceGen input {:?} has kind {:?}, expected {expected_kind:?}",
            input.path, input.receipt.kind
        ));
    }
    let bytes = std::fs::read(&input.path)
        .map_err(|error| format!("read FaceGen input {}: {error}", input.path.display()))?;
    if input.receipt.byte_len != bytes.len() as u64
        || input.receipt.blake3 != blake3::hash(&bytes).to_hex().as_str()
    {
        return Err(format!(
            "legacy creature FaceGen input {} does not match its byte receipt",
            input.path.display()
        ));
    }
    Ok(())
}

fn validate_hash_text(hash: &str, label: &str) -> Result<(), String> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!("{label} is not a lowercase BLAKE3 hash"));
    }
    Ok(())
}

fn load_fo4_facegen_nif(
    path: &Path,
    label: &str,
) -> Result<nif_core_native::model::NifFile, String> {
    let nif = nif_core_native::model::NifFile::load(path)
        .map_err(|error| format!("load {label} {}: {error}", path.display()))?;
    if nif.header.version != (20, 2, 0, 7)
        || nif.header.user_version != 12
        || nif.header.bs_version != 130
    {
        return Err(format!("{label} {} is not Fallout 4", path.display()));
    }
    Ok(nif)
}

fn exact_face_shape_id(nif: &nif_core_native::model::NifFile, name: &str) -> Result<usize, String> {
    use nif_core_native::model::NifValue;

    let schema = nif_core_native::schema::NifSchema::from_generated();
    let ids = nif
        .blocks
        .iter()
        .filter(|block| {
            schema.is_subtype_of(&block.type_name, "BSTriShape")
                && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(_)))
                && matches!(block.get_field("Name"), Some(NifValue::String(actual)) if actual.trim_end_matches('\0').eq_ignore_ascii_case(name))
        })
        .map(|block| block.block_id)
        .collect::<Vec<_>>();
    if ids.len() != 1 {
        return Err(format!(
            "legacy creature FaceGen shape {name:?} has {} exact matches",
            ids.len()
        ));
    }
    Ok(ids[0])
}

fn write_facetint_dds_with_required_uv(
    output: &Path,
    source: &Path,
    uv_lut: &(Vec<f32>, usize, usize),
) -> Result<(), String> {
    let source = directxtex_native::read_dds_rgba_image(source)
        .map_err(|error| format!("read exact source FaceTint DDS: {error}"))?;
    let remapped = remap_rgba_with_uv_lut(
        &source.rgba,
        source.width,
        source.height,
        &uv_lut.0,
        uv_lut.1,
        uv_lut.2,
    );
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create FaceTint output directory: {error}"))?;
    }
    directxtex_native::write_dds_rgba_image(
        output,
        uv_lut.2 as u32,
        uv_lut.1 as u32,
        &remapped,
        "BC7_UNORM",
        false,
    )
    .map_err(|error| format!("write exact remapped FaceTint DDS: {error}"))
}

fn validate_fo4_facegen_output(path: &Path) -> Result<(), String> {
    load_fo4_facegen_nif(path, "generated FaceGeom").map(|_| ())
}

fn source_artifact_key(
    artifact: &CreatureAncillaryNpcSourceArtifactReceipt,
) -> (String, String, CreatureAncillaryNpcSourceArtifactKind) {
    (
        artifact.source_game.to_ascii_lowercase(),
        artifact.source_data_path.to_ascii_lowercase(),
        artifact.kind,
    )
}

fn artifact_receipt_from_file(
    kind: CreatureAncillaryNpcArtifactKind,
    source_artifacts: Vec<CreatureAncillaryNpcSourceArtifactReceipt>,
    target_data_path: String,
    target_path: &Path,
) -> Result<CreatureAncillaryNpcArtifactReceipt, String> {
    let bytes = std::fs::read(target_path).map_err(|error| {
        format!(
            "read generated FaceGen artifact {}: {error}",
            target_path.display()
        )
    })?;
    Ok(CreatureAncillaryNpcArtifactReceipt {
        kind,
        source_artifacts,
        target_data_path,
        target_byte_len: bytes.len() as u64,
        target_blake3: blake3::hash(&bytes).to_hex().to_string(),
    })
}

fn hash_facegen_pipeline_resources(
    correspondence: &Path,
    uv_lut: &Path,
    target_base_head: &Path,
    target_template: &Path,
    parts: &[LegacyCreatureFacegenPartInput],
) -> Result<String, String> {
    let mut paths = vec![
        ("correspondence".to_string(), correspondence.to_path_buf()),
        ("uv_lut".to_string(), uv_lut.to_path_buf()),
        (
            "target_base_head".to_string(),
            target_base_head.to_path_buf(),
        ),
        ("target_template".to_string(), target_template.to_path_buf()),
    ];
    paths.extend(parts.iter().enumerate().map(|(index, part)| {
        (
            format!("target_part_{index:04}"),
            part.target_model_nif.clone(),
        )
    }));
    let mut receipts = Vec::with_capacity(paths.len());
    for (label, path) in paths {
        let bytes = std::fs::read(&path).map_err(|error| {
            format!("read FaceGen pipeline resource {}: {error}", path.display())
        })?;
        receipts.push((
            label,
            bytes.len() as u64,
            blake3::hash(&bytes).to_hex().to_string(),
        ));
    }
    let json = serde_json::to_vec(&receipts)
        .map_err(|error| format!("serialize FaceGen pipeline resources: {error}"))?;
    Ok(blake3::hash(&json).to_hex().to_string())
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

    fn lookup(&self, fnv_ref: Option<&HairRef>, race_class: RaceClass) -> Option<HairRef> {
        if let Some(r) = fnv_ref {
            let key = r.normalize_key();
            if let Some(mapped) = self.explicit.get(&key) {
                return Some(mapped.clone());
            }
        }
        if fnv_ref.is_none() {
            return match race_class {
                RaceClass::HumanMale | RaceClass::GhoulMale => self.male_default.clone(),
                RaceClass::HumanFemale | RaceClass::GhoulFemale => self.female_default.clone(),
                RaceClass::ChildMale => Some(HairRef {
                    plugin: "Fallout4.esm".to_string(),
                    object_id: "20D6D6".to_string(),
                }),
                RaceClass::ChildFemale => Some(HairRef {
                    plugin: "Fallout4.esm".to_string(),
                    object_id: "0CFF71".to_string(),
                }),
                RaceClass::Unknown => None,
            };
        }
        let source_local = fnv_ref
            .and_then(|source| {
                u32::from_str_radix(source.object_id.trim_start_matches("0x"), 16).ok()
            })
            .unwrap_or(0);
        let (plugin, object_id) = match race_class {
            RaceClass::HumanMale | RaceClass::GhoulMale => {
                if source_local & 1 == 0 {
                    ("Fallout4.esm", "094D04")
                } else {
                    ("Fallout4.esm", "094D05")
                }
            }
            RaceClass::HumanFemale | RaceClass::GhoulFemale => {
                if source_local & 1 == 0 {
                    ("Fallout4.esm", "09C29B")
                } else {
                    ("Fallout4.esm", "09C29C")
                }
            }
            RaceClass::ChildMale => ("Fallout4.esm", "20D6D6"),
            RaceClass::ChildFemale => ("Fallout4.esm", "0CFF71"),
            RaceClass::Unknown => return None,
        };
        Some(HairRef {
            plugin: plugin.to_string(),
            object_id: object_id.to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HeadPartSpec {
    plugin: String,
    local: u32,
    model_path: String,
    source_model_path: Option<String>,
    shape_name: String,
}

fn target_hair_spec(hair: &HairRef) -> Option<HeadPartSpec> {
    let local = u32::from_str_radix(hair.object_id.trim_start_matches("0x"), 16).ok()?;
    let (model_path, shape_name) = match local {
        0x0009_4D04 => (
            "Meshes/Actors/Character/CharacterAssets/Hair/Male/Hair19.nif",
            "HairMale19",
        ),
        0x0009_4D05 => (
            "Meshes/Actors/Character/CharacterAssets/Hair/Male/Hair20.nif",
            "HairMale20",
        ),
        0x0014_77D0 => (
            "Meshes/Actors/Character/CharacterAssets/Hair/Male/Hair18.nif",
            "HairMale18",
        ),
        0x0009_C29B => (
            "Meshes/Actors/Character/CharacterAssets/Hair/Female/FemaleHair19.nif",
            "HairFemale19",
        ),
        0x0009_C29C => (
            "Meshes/Actors/Character/CharacterAssets/Hair/Female/FemaleHair20.nif",
            "HairFemale20",
        ),
        0x0020_D6D6 => (
            "Meshes/Actors/Character/CharacterAssets/Hair/Male/BoyHair03.nif",
            "BoyHair03",
        ),
        0x000C_FF71 => (
            "Meshes/Actors/Character/CharacterAssets/Hair/Female/GirlHair02.nif",
            "GirlHair02",
        ),
        _ => return None,
    };
    Some(HeadPartSpec {
        plugin: hair.plugin.clone(),
        local,
        model_path: model_path.to_string(),
        source_model_path: None,
        shape_name: shape_name.to_string(),
    })
}

fn exact_legacy_hair_spec(
    source_npc_local: u32,
    source: &HairRef,
    race_class: RaceClass,
) -> Option<HeadPartSpec> {
    if source_npc_local != 0x0010_4C7F
        || race_class != RaceClass::HumanMale
        || !source.plugin.eq_ignore_ascii_case("FalloutNV.esm")
        || u32::from_str_radix(source.object_id.trim_start_matches("0x"), 16).ok()? != 0x0009_87D9
    {
        return None;
    }
    Some(HeadPartSpec {
        plugin: "Fallout4.esm".to_string(),
        local: 0x0014_77D0,
        model_path: "Meshes/Actors/Character/CharacterAssets/Hair/Male/Hair18.nif".to_string(),
        source_model_path: Some("Meshes/Characters/Hair/HairBaseOld.NIF".to_string()),
        shape_name: "Hat".to_string(),
    })
}

const FO4_BEARD_FORM_IDS: &[u32] = &[
    0x000E_12A4,
    0x0013_5A95,
    0x0013_5A9E,
    0x0013_5AA3,
    0x0013_5A97,
    0x0013_5A9A,
    0x0013_5A9F,
    0x0013_5A96,
    0x0013_5AA0,
    0x0013_5A9B,
    0x0013_5A9D,
    0x0013_5AA4,
    0x0013_5A99,
    0x0013_5A98,
    0x0013_5AA1,
    0x0013_5AA2,
    0x0013_5A9C,
    0x0002_9667,
    0x0002_966C,
    0x0002_9671,
];

fn target_beard_spec(source: &HairRef, race_class: RaceClass) -> Option<HeadPartSpec> {
    if !matches!(race_class, RaceClass::HumanMale | RaceClass::GhoulMale) {
        return None;
    }
    let source_local = u32::from_str_radix(source.object_id.trim_start_matches("0x"), 16).ok()?;
    if source_local == 0x0009_F923 {
        return Some(HeadPartSpec {
            plugin: "Fallout4.esm".to_string(),
            local: 0x0006_17BC,
            model_path: "Meshes/Actors/Character/CharacterAssets/Beards/BigBeard03.nif".to_string(),
            source_model_path: Some("Meshes/Characters/Hair/BeardFullOld.NIF".to_string()),
            shape_name: "BeardFullOld:0".to_string(),
        });
    }
    if matches!(
        source_local,
        0x000B_5BEA
            | 0x000B_6DE4
            | 0x000B_4016
            | 0x000B_4017
            | 0x000B_4018
            | 0x000B_4019
            | 0x0007_3D56
    ) {
        return None;
    }
    let index = match source_local {
        0x0006_6FD0 | 0x0007_B47B => 19, // The Gentleman
        0x0006_6FCF => 8,                // Gunslinger
        0x0007_B489 | 0x0009_F921 => 10, // Swashbuckler
        0x0007_B474 | 0x0009_F920 => 12, // Man's Man
        0x000B_22C0 => 18,               // Chin Dusting
        0x000B_22C2 => 14,               // Daddy O
        0x000B_CBF3 => 11,               // Mephistopheles
        0x000B_CBF7 => 9,                // Renegade
        0x000B_CBFC => 7,                // Dashing Rogue
        _ => source_local as usize % FO4_BEARD_FORM_IDS.len(),
    };
    let local = FO4_BEARD_FORM_IDS[index];
    let beard_number = index + 1;
    let beard_file_number = if beard_number == 1 {
        "1".to_string()
    } else {
        format!("{beard_number:02}")
    };
    Some(HeadPartSpec {
        plugin: "Fallout4.esm".to_string(),
        local,
        model_path: format!(
            "Meshes/Actors/Character/CharacterAssets/Beards/HumanBeard{beard_file_number}.nif"
        ),
        source_model_path: None,
        shape_name: format!("Beard{beard_number:02}"),
    })
}

fn extract_pnam_refs(record: &Record, interner: &StringInterner) -> Vec<HairRef> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "PNAM")
        .filter_map(|entry| match &entry.value {
            FieldValue::FormKey(key) => Some(HairRef {
                plugin: interner.resolve(key.plugin).unwrap_or_default().to_string(),
                object_id: format!("{:06X}", key.local),
            }),
            _ => None,
        })
        .collect()
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

/// Extract the NPC's source race.
fn extract_rnam_form_key(record: &Record) -> Option<FormKey> {
    for entry in &record.fields {
        if entry.sig.as_str() == "RNAM" {
            if let FieldValue::FormKey(fk) = &entry.value {
                return Some(*fk);
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

fn extract_nam7_weight_value(record: &Record) -> Option<f32> {
    for entry in &record.fields {
        if entry.sig.as_str() == "NAM7" {
            if let FieldValue::Float(f) = &entry.value {
                return Some(*f as f32);
            }
            if let FieldValue::Bytes(bytes) = &entry.value {
                if bytes.len() >= 4 {
                    return Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
                }
            }
        }
    }
    None
}

fn extract_nam7_weight(record: &Record) -> f32 {
    extract_nam7_weight_value(record).unwrap_or(0.0)
}

fn extract_skyrim_body_weight(record: &Record) -> f32 {
    extract_nam7_weight_value(record)
        .map(|weight| weight.clamp(0.0, 100.0) / 50.0 - 1.0)
        .unwrap_or(0.0)
}

fn extract_hclr_rgb(record: &Record, interner: &StringInterner) -> Option<[u8; 3]> {
    let value = &record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "HCLR")?
        .value;
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 3 => Some([bytes[0], bytes[1], bytes[2]]),
        FieldValue::Struct(fields) => {
            let component = |name: &str| {
                fields.iter().find_map(|(field_name, value)| {
                    interner
                        .resolve(*field_name)
                        .is_some_and(|field_name| field_name.eq_ignore_ascii_case(name))
                        .then(|| match value {
                            FieldValue::Int(value) => u8::try_from(*value).ok(),
                            FieldValue::Uint(value) => u8::try_from(*value).ok(),
                            _ => None,
                        })
                        .flatten()
                })
            };
            Some([component("red")?, component("green")?, component("blue")?])
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Path resolution helpers
// ---------------------------------------------------------------------------

fn find_source_head_nif(
    source_extracted: &Path,
    race_class: RaceClass,
    source_profile: SourceHeadProfile,
) -> Option<PathBuf> {
    let candidates: &[&str] = match (race_class, source_profile) {
        (RaceClass::HumanFemale, SourceHeadProfile::Old) => &[
            "Meshes/Characters/Head/headoldfemale.nif",
            "meshes/characters/head/headoldfemale.nif",
        ],
        (RaceClass::HumanMale, SourceHeadProfile::Old) => &[
            "Meshes/Characters/Head/headold.nif",
            "meshes/characters/head/headold.nif",
        ],
        (RaceClass::ChildFemale, _) => &[
            "Meshes/Characters/Head/headchildfemale.nif",
            "meshes/characters/head/headchildfemale.nif",
        ],
        (RaceClass::ChildMale, _) => &[
            "Meshes/Characters/Head/headchild.nif",
            "meshes/characters/head/headchild.nif",
        ],
        (RaceClass::GhoulFemale, _) => &[
            "Meshes/Characters/Head/headghoulfemale.nif",
            "meshes/characters/head/headghoulfemale.nif",
        ],
        (RaceClass::GhoulMale, _) => &[
            "Meshes/Characters/Head/headghoul.nif",
            "meshes/characters/head/headghoul.nif",
        ],
        (RaceClass::HumanFemale, _) => &[
            "Meshes/Characters/Head/headfemale.nif",
            "meshes/characters/head/headfemale.nif",
        ],
        (RaceClass::HumanMale, _) => &[
            "Meshes/Characters/Head/headhuman.nif",
            "meshes/characters/head/headhuman.nif",
        ],
        (RaceClass::Unknown, _) => &[],
    };
    for c in candidates {
        let path = source_extracted.join(c);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_source_head_egm(
    source_extracted: &Path,
    race_class: RaceClass,
    source_profile: SourceHeadProfile,
) -> Option<PathBuf> {
    let candidates: &[&str] = match (race_class, source_profile) {
        (RaceClass::HumanFemale, SourceHeadProfile::Old) => &[
            "Meshes/Characters/Head/headoldfemale.egm",
            "meshes/characters/head/headoldfemale.egm",
        ],
        (RaceClass::HumanMale, SourceHeadProfile::Old) => &[
            "Meshes/Characters/Head/headold.egm",
            "meshes/characters/head/headold.egm",
        ],
        (RaceClass::ChildFemale, _) => &[
            "Meshes/Characters/Head/headchildfemale.egm",
            "meshes/characters/head/headchildfemale.egm",
        ],
        (RaceClass::ChildMale, _) => &[
            "Meshes/Characters/Head/headchild.egm",
            "meshes/characters/head/headchild.egm",
        ],
        (RaceClass::GhoulFemale, _) => &[
            "Meshes/Characters/Head/headghoulfemale.egm",
            "meshes/characters/head/headghoulfemale.egm",
        ],
        (RaceClass::GhoulMale, _) => &[
            "Meshes/Characters/Head/headghoul.egm",
            "meshes/characters/head/headghoul.egm",
        ],
        (RaceClass::HumanFemale, _) => &[
            "Meshes/Characters/Head/headfemale.egm",
            "meshes/characters/head/headfemale.egm",
        ],
        (RaceClass::HumanMale, _) => &[
            "Meshes/Characters/Head/headhuman.egm",
            "meshes/characters/head/headhuman.egm",
        ],
        (RaceClass::Unknown, _) => &[],
    };
    for c in candidates {
        let path = source_extracted.join(c);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_target_base_head_nif(target_extracted: &Path, race_class: RaceClass) -> Option<PathBuf> {
    let candidates: &[&str] = match race_class {
        RaceClass::ChildFemale => &[
            "Meshes/Actors/Character/CharacterAssets/ChildFemaleHead.nif",
            "meshes/actors/character/characterassets/childfemalehead.nif",
        ],
        RaceClass::ChildMale => &[
            "Meshes/Actors/Character/CharacterAssets/ChildMaleHead.nif",
            "meshes/actors/character/characterassets/childmalehead.nif",
        ],
        RaceClass::GhoulFemale => &[
            "Meshes/Actors/Character/CharacterAssets/FemaleGhoulHead.nif",
            "meshes/actors/character/characterassets/femaleghoulhead.nif",
        ],
        RaceClass::GhoulMale => &[
            "Meshes/Actors/Character/CharacterAssets/MaleGhoulHead.nif",
            "meshes/actors/character/characterassets/maleghoulhead.nif",
        ],
        RaceClass::HumanFemale => &[
            "Meshes/Actors/Character/CharacterAssets/BaseFemaleHead.nif",
            "meshes/actors/character/characterassets/basefemalehead.nif",
            "Meshes/Actors/Character/CharacterAssets/FaceParts/FemaleHead.nif",
        ],
        RaceClass::HumanMale => &[
            "Meshes/Actors/Character/CharacterAssets/BaseMaleHead.nif",
            "meshes/actors/character/characterassets/basemalehead.nif",
            "Meshes/Actors/Character/CharacterAssets/FaceParts/MaleHead.nif",
        ],
        RaceClass::Unknown => &[],
    };
    for c in candidates {
        let path = target_extracted.join(c);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_target_facegeom_template(
    target_extracted: &Path,
    race_class: RaceClass,
) -> Option<PathBuf> {
    let form_id = race_class.template_facegeom_form_id()?;
    let relative_roots = [
        "Meshes/Actors/Character/FaceGenData/FaceGeom/Fallout4.esm",
        "meshes/actors/character/facegendata/facegeom/fallout4.esm",
    ];
    for root in relative_roots {
        for file_name in [
            format!("{form_id}.nif"),
            format!("{}.nif", form_id.to_ascii_lowercase()),
        ] {
            let path = target_extracted.join(root).join(file_name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn find_target_face_bones_nif(target_extracted: &Path, race_class: RaceClass) -> Option<PathBuf> {
    let candidates: &[&str] = match race_class {
        RaceClass::ChildFemale | RaceClass::ChildMale => &[],
        RaceClass::GhoulFemale => &[
            "Meshes/Actors/Character/CharacterAssets/FemaleGhoulHead_faceBones.nif",
            "meshes/actors/character/characterassets/femaleghoulhead_facebones.nif",
        ],
        RaceClass::GhoulMale => &[
            "Meshes/Actors/Character/CharacterAssets/MaleGhoulHead_faceBones.nif",
            "meshes/actors/character/characterassets/maleghoulhead_facebones.nif",
        ],
        RaceClass::HumanFemale => &[
            "Meshes/Actors/Character/CharacterAssets/BaseFemaleHead_faceBones.nif",
            "meshes/actors/character/characterassets/basefemalehead_facebones.nif",
        ],
        RaceClass::HumanMale => &[
            "Meshes/Actors/Character/CharacterAssets/BaseMaleHead_faceBones.nif",
            "meshes/actors/character/characterassets/basemalehead_facebones.nif",
        ],
        RaceClass::Unknown => &[],
    };
    for c in candidates {
        let path = target_extracted.join(c);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_skyrim_base_head_nif(source_extracted: &Path, is_female: bool) -> Option<PathBuf> {
    let file_name = if is_female {
        "FemaleHead.nif"
    } else {
        "MaleHead.nif"
    };
    for root in [
        "Meshes/Actors/Character/Character Assets",
        "meshes/actors/character/character assets",
    ] {
        let path = source_extracted.join(root).join(file_name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_skyrim_facegeom_nif(
    source_extracted: &Path,
    source_plugin_name: &str,
    formid_hex: &str,
) -> Option<PathBuf> {
    for root in [
        "Meshes/Actors/Character/FaceGenData/FaceGeom",
        "meshes/actors/character/facegendata/facegeom",
    ] {
        for file_name in [
            format!("{formid_hex}.nif"),
            format!("{}.nif", formid_hex.to_ascii_lowercase()),
        ] {
            let path = source_extracted
                .join(root)
                .join(source_plugin_name)
                .join(file_name);
            if path.is_file() {
                return Some(path);
            }
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
    let hex_lower = formid_hex.to_ascii_lowercase();
    let val = u32::from_str_radix(&hex_lower, 16).ok()?;
    let alt = format!("{val:x}_0.dds");
    let plugin_dirs = [source_plugin_name, "FalloutNV.esm", "Fallout3.esm"];
    for plugin_dir in plugin_dirs {
        let root = source_extracted
            .join("textures")
            .join("characters")
            .join("facemods")
            .join(plugin_dir.to_ascii_lowercase());
        for stem in &[format!("{hex_lower}_0.dds"), alt.clone()] {
            let p = root.join(stem);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// NPC record field writing helpers (FO4 binary subrecord format)
// ---------------------------------------------------------------------------

fn fo4_body_morph_bytes(body_weight: f32) -> smallvec::SmallVec<[u8; 32]> {
    let (thin, muscular, fat) = fnv_weight_to_fo4_morphs(body_weight);
    let mut bytes = smallvec::SmallVec::new();
    bytes.extend_from_slice(&thin.to_le_bytes());
    bytes.extend_from_slice(&muscular.to_le_bytes());
    bytes.extend_from_slice(&fat.to_le_bytes());
    bytes
}

fn add_head_part(record: &mut Record, plugin: &str, local: u32, interner: &StringInterner) {
    let pnam_sig = crate::ids::SubrecordSig::from_str("PNAM").expect("PNAM is 4 bytes");
    let head_part = FormKey {
        local,
        plugin: interner.intern(plugin),
    };
    if !record
        .fields
        .iter()
        .any(|entry| entry.sig == pnam_sig && entry.value == FieldValue::FormKey(head_part))
    {
        record.fields.push(FieldEntry {
            sig: pnam_sig,
            value: FieldValue::FormKey(head_part),
        });
    }
}

fn target_hair_color(source_rgb: Option<[u8; 3]>, race_class: RaceClass) -> Option<u32> {
    if let Some([red, green, blue]) = source_rgb {
        let minimum = red.min(green).min(blue);
        let maximum = red.max(green).max(blue);
        if maximum - minimum <= 12 {
            if maximum >= 176 {
                return Some(0x0019_EE5A);
            }
            if maximum >= 96 {
                return Some(0x0019_EE5C);
            }
        }
    }
    race_class.template_hair_color()
}

fn set_template_hair_color(
    record: &mut Record,
    source_rgb: Option<[u8; 3]>,
    race_class: RaceClass,
    interner: &StringInterner,
) {
    let Some(local) = target_hair_color(source_rgb, race_class) else {
        return;
    };
    let hclf_sig = crate::ids::SubrecordSig::from_str("HCLF").expect("HCLF is 4 bytes");
    record.fields.retain(|entry| entry.sig != hclf_sig);
    record.fields.push(FieldEntry {
        sig: hclf_sig,
        value: FieldValue::FormKey(FormKey {
            local,
            plugin: interner.intern("Fallout4.esm"),
        }),
    });
}

/// Replace baked morph fields while preserving FO4 head parts and colors.
fn replace_face_fields_in_record(
    record: &mut Record,
    pnam_refs: &[(String, String)], // (plugin, object_id)
    bone_offsets: &BoneOffsets,     // small_index → [x,y,z]
    body_weight: f32,
    source_hair_color: Option<[u8; 3]>,
    race_class: RaceClass,
    named_bones: &[NamedBone],
    interner: &StringInterner,
) {
    // Remove existing face-related subrecords from the record
    const FACE_SIGS: &[&str] = &[
        "BCLF", "PNAM", "HCLF", "MSDK", "MSDV", "MRSV", "FMRI", "FMRS", "FMIN", "MWGT", "NAM7",
    ];

    record
        .fields
        .retain(|f| !FACE_SIGS.contains(&f.sig.as_str()));

    // MWGT — FO4 body morph weights.
    let mwgt_sig = crate::ids::SubrecordSig::from_str("MWGT").expect("MWGT is 4 bytes");
    record.fields.push(FieldEntry {
        sig: mwgt_sig,
        value: FieldValue::Bytes(fo4_body_morph_bytes(body_weight)),
    });

    // PNAM — head parts (one per head part, repeatable)
    for (plugin, object_id) in pnam_refs {
        let Ok(local) = u32::from_str_radix(object_id.trim_start_matches("0x"), 16) else {
            continue;
        };
        add_head_part(record, plugin, local, interner);
    }
    set_template_hair_color(record, source_hair_color, race_class, interner);

    // FMIN — facial morph intensity (f32 = 1.0)
    let fmin_sig = crate::ids::SubrecordSig::from_str("FMIN").expect("FMIN is 4 bytes");
    let fmin_bytes = smallvec::SmallVec::from_slice(&1.0f32.to_le_bytes());
    record.fields.push(FieldEntry {
        sig: fmin_sig,
        value: FieldValue::Bytes(fmin_bytes),
    });

    let _ = (bone_offsets, named_bones);
}

// ---------------------------------------------------------------------------
// Cached bake resources and threaded jobs
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct BakeResult {
    facegeom_relpath: String,
    facetint_relpath: String,
    bone_offsets: BoneOffsets,
}

type FaceBonesSkin = (Vec<Vec<f32>>, Vec<Vec<i32>>, Vec<String>);

struct FaceBakeProfile {
    fnv_neutral: Vec<[f32; 3]>,
    egm_num_diffs: usize,
    egm_num_verts: usize,
    egm_basis: Vec<f32>,
    correspondence: Correspondence,
    fo4_neutral: Vec<[f32; 3]>,
    template: nif_core_native::model::NifFile,
    template_shape_id: usize,
    face_bones: Option<FaceBonesSkin>,
    uv_lut: Option<(Vec<f32>, usize, usize)>,
}

struct SkyrimFaceBakeProfile {
    race_class: RaceClass,
    skyrim_neutral: Vec<[f32; 3]>,
    correspondence: Correspondence,
    deformation_scale: [f32; 3],
    fo4_neutral: Vec<[f32; 3]>,
    template: nif_core_native::model::NifFile,
    template_shape_id: usize,
    face_bones: Option<FaceBonesSkin>,
    face_bones_reference: Option<nif_core_native::model::NifFile>,
}

struct FacePartAsset {
    spec: HeadPartSpec,
    nif: nif_core_native::model::NifFile,
    shape_ids: Vec<usize>,
    skin_reference: Option<nif_core_native::model::NifFile>,
}

struct FaceBakeJob {
    target_record: Record,
    coefficients: Vec<f32>,
    race_class: RaceClass,
    body_weight: f32,
    source_hair_color: Option<[u8; 3]>,
    source_head_profile: SourceHeadProfile,
    hair_ref_source: Option<HairRef>,
    variable_parts: Vec<Arc<FacePartAsset>>,
    pnam_refs_target: Vec<(String, String)>,
    formid_hex: String,
    source_formid_hex: String,
    source_plugin_name: String,
    eid: String,
}

struct SkyrimFaceBakeJob {
    target_record: Record,
    source_facegeom: PathBuf,
    source_face_kind: SkyrimHumanoidFaceKind,
    race_class: RaceClass,
    body_weight: f32,
    source_hair_color: Option<[u8; 3]>,
    variable_parts: Vec<Arc<FacePartAsset>>,
    pnam_refs_target: Vec<(String, String)>,
    formid_hex: String,
    eid: String,
}

fn target_head_part_refs(
    race_class: RaceClass,
    variable_parts: &[Arc<FacePartAsset>],
) -> Vec<(String, String)> {
    let mut refs = race_class
        .mandatory_head_parts()
        .iter()
        .map(|local| ("Fallout4.esm".to_string(), format!("{local:06X}")))
        .collect::<Vec<_>>();
    refs.extend(variable_parts.iter().map(|asset| {
        (
            asset.spec.plugin.clone(),
            format!("{:06X}", asset.spec.local),
        )
    }));
    refs
}

fn target_preserved_face_part_refs(race_class: RaceClass) -> Vec<(String, String)> {
    let locals: &[u32] = match race_class {
        RaceClass::HumanMale => &[0x0005_1631, 0x0014_108D, 0x0001_EEBB],
        RaceClass::HumanFemale => &[0x001E_55C4, 0x000C_FB4E, 0x000C_FB3F],
        _ => return target_head_part_refs(race_class, &[]),
    };
    locals
        .iter()
        .map(|local| ("Fallout4.esm".to_string(), format!("{local:06X}")))
        .collect()
}

fn is_variable_face_shape(name: &str) -> bool {
    let lower = name.trim_end_matches('\0').to_ascii_lowercase();
    lower.starts_with("hair")
        || lower.starts_with("beard")
        || lower.starts_with("boyhair")
        || lower.starts_with("girlhair")
}

fn strip_variable_face_shapes(nif: &mut nif_core_native::model::NifFile) {
    use nif_core_native::model::NifValue;

    let schema = nif_core_native::schema::NifSchema::from_generated();
    let remove = nif
        .blocks
        .iter()
        .filter(|block| {
            schema.is_subtype_of(&block.type_name, "BSTriShape")
                && matches!(block.get_field("Name"), Some(NifValue::String(name)) if is_variable_face_shape(name))
        })
        .map(|block| block.block_id)
        .collect::<HashSet<_>>();
    if remove.is_empty() {
        return;
    }
    for block in &mut nif.blocks {
        let Some(NifValue::Array(children)) = block.get_field("Children").cloned() else {
            continue;
        };
        let retained = children
            .into_iter()
            .filter(|child| !matches!(child, NifValue::Ref(id) if *id >= 0 && remove.contains(&(*id as usize))))
            .collect::<Vec<_>>();
        block.set_field("Num Children", NifValue::UInt(retained.len() as u64));
        block.set_field("Children", NifValue::Array(retained));
    }
    let mut ids = remove.into_iter().collect::<Vec<_>>();
    ids.sort_unstable();
    nif.remove_blocks(&ids);
}

fn strip_all_face_shapes(nif: &mut nif_core_native::model::NifFile) -> Result<usize, String> {
    use nif_core_native::model::NifValue;

    let face_node = find_face_node(nif)
        .ok_or_else(|| "FaceGen template has no BSFaceGenNiNodeSkinned node".to_string())?;
    let shape_ids = match nif.blocks[face_node].get_field("Children") {
        Some(NifValue::Array(children)) => children
            .iter()
            .filter_map(|child| match child {
                NifValue::Ref(id) if *id >= 0 => Some(*id as usize),
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    nif.blocks[face_node].set_field("Num Children", NifValue::UInt(0));
    nif.blocks[face_node].set_field("Children", NifValue::Array(Vec::new()));
    if !shape_ids.is_empty() {
        nif.remove_blocks(&shape_ids);
    }
    Ok(shape_ids.len())
}

fn preserved_face_shape_name(source_name: &str, race_class: RaceClass) -> String {
    let lower = source_name.trim_end_matches('\0').to_ascii_lowercase();
    if lower.contains("eyes") {
        return match race_class {
            RaceClass::HumanFemale => "FemaleEyesHumanHazelGreen",
            _ => "MaleEyesHumanBlue",
        }
        .to_string();
    }
    if lower.contains("mouth") {
        return match race_class {
            RaceClass::HumanFemale => "FemaleMouthHumanoidDefault",
            _ => "MaleMouthHumanoidDefault",
        }
        .to_string();
    }
    if lower.contains("head") && !lower.contains("hair") {
        return race_class
            .primary_head_shape_name()
            .unwrap_or(source_name)
            .to_string();
    }
    source_name.trim_end_matches('\0').to_string()
}

fn find_face_node(nif: &nif_core_native::model::NifFile) -> Option<usize> {
    use nif_core_native::model::NifValue;

    nif.blocks.iter().find_map(|block| {
        matches!(block.get_field("Name"), Some(NifValue::String(name)) if name.trim_end_matches('\0').eq_ignore_ascii_case("BSFaceGenNiNodeSkinned"))
            .then_some(block.block_id)
    })
}

fn load_face_part_asset(
    target_extracted: &Path,
    source_extracted: &Path,
    spec: HeadPartSpec,
) -> Result<FacePartAsset, String> {
    use nif_core_native::model::{NifFile, NifValue};

    let legacy = spec
        .source_model_path
        .as_deref()
        .and_then(|source_model_path| {
            let path = source_extracted
                .join(source_model_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let mut nif = NifFile::load(&path).ok()?;
            (nif_core_native::convert_file::prepare_legacy_face_part_for_fo4(&mut nif) > 0)
                .then_some((path, nif))
        });
    let (path, nif, using_source_geometry) = match legacy {
        Some((path, nif)) => (path, nif, true),
        None => {
            let path =
                target_extracted.join(spec.model_path.replace('/', std::path::MAIN_SEPARATOR_STR));
            let nif = NifFile::load(path.clone())
                .map_err(|error| format!("load head part {}: {error}", path.display()))?;
            (path, nif, false)
        }
    };
    let schema = nif_core_native::schema::NifSchema::from_generated();
    let shape_ids = nif
        .blocks
        .iter()
        .filter(|block| {
            schema.is_subtype_of(&block.type_name, "BSTriShape")
                && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(_)))
                && (!using_source_geometry
                    || matches!(
                        block.get_field("Name"),
                        Some(NifValue::String(name)) if name.eq_ignore_ascii_case(&spec.shape_name)
                    ))
        })
        .map(|block| block.block_id)
        .collect::<Vec<_>>();
    if shape_ids.is_empty() {
        return Err(format!("no geometry in head part {}", path.display()));
    }
    let skin_reference = if using_source_geometry {
        let reference_path =
            target_extracted.join(spec.model_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        Some(NifFile::load(reference_path.clone()).map_err(|error| {
            format!(
                "load FO4 skin reference {}: {error}",
                reference_path.display()
            )
        })?)
    } else {
        None
    };
    Ok(FacePartAsset {
        spec,
        nif,
        shape_ids,
        skin_reference,
    })
}

fn append_face_part_shape(
    target: &mut nif_core_native::model::NifFile,
    source: &nif_core_native::model::NifFile,
    source_shape_id: usize,
    target_parent_id: usize,
    shape_name: &str,
) -> Result<usize, String> {
    use nif_core_native::model::{NifFile, NifValue};

    let schema = nif_core_native::schema::NifSchema::from_generated();
    let mut target_nodes = HashMap::new();
    for block in &target.blocks {
        if schema.is_subtype_of(&block.type_name, "NiNode") {
            if let Some(NifValue::String(name)) = block.get_field("Name") {
                target_nodes.insert(
                    name.trim_end_matches('\0').to_ascii_lowercase(),
                    block.block_id,
                );
            }
        }
    }

    let mut id_map = HashMap::<i32, i32>::new();
    let mut copy_ids = Vec::new();
    let mut visited = HashSet::new();
    fn visit(
        source: &NifFile,
        source_id: usize,
        schema: &nif_core_native::schema::NifSchema,
        target_nodes: &HashMap<String, usize>,
        id_map: &mut HashMap<i32, i32>,
        copy_ids: &mut Vec<usize>,
        visited: &mut HashSet<usize>,
    ) -> Result<(), String> {
        if !visited.insert(source_id) {
            return Ok(());
        }
        let block = source
            .get_block(source_id)
            .ok_or_else(|| format!("missing source NIF block {source_id}"))?;
        if schema.is_subtype_of(&block.type_name, "NiNode") {
            if let Some(NifValue::String(name)) = block.get_field("Name") {
                if let Some(target_id) =
                    target_nodes.get(&name.trim_end_matches('\0').to_ascii_lowercase())
                {
                    id_map.insert(source_id as i32, *target_id as i32);
                    return Ok(());
                }
            }
            if source_id == 0 {
                id_map.insert(0, 0);
                return Ok(());
            }
            return Err(format!(
                "head part requires missing target bone block {source_id}"
            ));
        }
        for (field_name, refs) in block.get_all_ref_fields(schema) {
            if matches!(field_name.as_str(), "Target" | "Manager" | "Scene") {
                for source_ref in refs {
                    id_map.insert(source_ref, 0);
                }
                continue;
            }
            for source_ref in refs {
                if source_ref >= 0 {
                    visit(
                        source,
                        source_ref as usize,
                        schema,
                        target_nodes,
                        id_map,
                        copy_ids,
                        visited,
                    )?;
                }
            }
        }
        copy_ids.push(source_id);
        Ok(())
    }
    visit(
        source,
        source_shape_id,
        &schema,
        &target_nodes,
        &mut id_map,
        &mut copy_ids,
        &mut visited,
    )?;

    let first_new_id = target.blocks.len();
    for (offset, source_id) in copy_ids.iter().enumerate() {
        id_map.insert(*source_id as i32, (first_new_id + offset) as i32);
    }
    let mut copied = copy_ids
        .iter()
        .map(|source_id| source.blocks[*source_id].clone())
        .collect::<Vec<_>>();
    for (offset, block) in copied.iter_mut().enumerate() {
        block.block_id = first_new_id + offset;
    }
    let mut remap_file = NifFile::default();
    remap_file.blocks = copied;
    remap_file.remap_refs(&id_map);
    let copied_shape_id = *id_map
        .get(&(source_shape_id as i32))
        .ok_or_else(|| "copied head part shape was not remapped".to_string())?
        as usize;
    if let Some(shape) = remap_file
        .blocks
        .iter_mut()
        .find(|block| block.block_id == copied_shape_id)
    {
        shape.set_field("Name", NifValue::String(shape_name.to_string()));
    }
    target.blocks.extend(remap_file.blocks);
    target.rebuild_header();

    let parent = target
        .get_block(target_parent_id)
        .cloned()
        .ok_or_else(|| format!("missing FaceGen parent block {target_parent_id}"))?;
    let mut children = match parent.get_field("Children") {
        Some(NifValue::Array(children)) => children.clone(),
        _ => Vec::new(),
    };
    children.push(NifValue::Ref(copied_shape_id as i32));
    let parent = &mut target.blocks[target_parent_id];
    parent.set_field("Num Children", NifValue::UInt(children.len() as u64));
    parent.set_field("Children", NifValue::Array(children));
    Ok(copied_shape_id)
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
    ghoul_male_head: Option<PathBuf>,
    ghoul_female_head: Option<PathBuf>,
    ghoul_male_face_bones: Option<PathBuf>,
    ghoul_female_face_bones: Option<PathBuf>,
    child_male_head: Option<PathBuf>,
    child_female_head: Option<PathBuf>,
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
            ghoul_male_head: path("target_base_head_ghoul_male"),
            ghoul_female_head: path("target_base_head_ghoul_female"),
            ghoul_male_face_bones: path("target_face_bones_ghoul_male"),
            ghoul_female_face_bones: path("target_face_bones_ghoul_female"),
            child_male_head: path("target_base_head_child_male"),
            child_female_head: path("target_base_head_child_female"),
        }
    }

    fn head(&self, race_class: RaceClass) -> Option<&Path> {
        match race_class {
            RaceClass::HumanMale => self.male_head.as_deref(),
            RaceClass::HumanFemale => self.female_head.as_deref(),
            RaceClass::GhoulMale => self.ghoul_male_head.as_deref(),
            RaceClass::GhoulFemale => self.ghoul_female_head.as_deref(),
            RaceClass::ChildMale => self.child_male_head.as_deref(),
            RaceClass::ChildFemale => self.child_female_head.as_deref(),
            RaceClass::Unknown => None,
        }
    }

    fn face_bones(&self, race_class: RaceClass) -> Option<&Path> {
        match race_class {
            RaceClass::HumanMale => self.male_face_bones.as_deref(),
            RaceClass::HumanFemale => self.female_face_bones.as_deref(),
            RaceClass::GhoulMale => self.ghoul_male_face_bones.as_deref(),
            RaceClass::GhoulFemale => self.ghoul_female_face_bones.as_deref(),
            RaceClass::ChildMale | RaceClass::ChildFemale => None,
            RaceClass::Unknown => None,
        }
    }
}

#[derive(Default)]
struct FaceResourcePaths {
    human_male_correspondence: Option<PathBuf>,
    human_female_correspondence: Option<PathBuf>,
    human_male_uv_lut: Option<PathBuf>,
    human_female_uv_lut: Option<PathBuf>,
    ghoul_male_correspondence: Option<PathBuf>,
    ghoul_female_correspondence: Option<PathBuf>,
    ghoul_male_uv_lut: Option<PathBuf>,
    ghoul_female_uv_lut: Option<PathBuf>,
    child_male_correspondence: Option<PathBuf>,
    child_female_correspondence: Option<PathBuf>,
    child_male_uv_lut: Option<PathBuf>,
    child_female_uv_lut: Option<PathBuf>,
}

impl FaceResourcePaths {
    fn from_params(params: &JsonValue) -> Self {
        let path = |key: &str| {
            params
                .get(key)
                .and_then(JsonValue::as_str)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Self {
            human_male_correspondence: path("correspondence_path_male"),
            human_female_correspondence: path("correspondence_path_female"),
            human_male_uv_lut: path("uv_lut_path_male"),
            human_female_uv_lut: path("uv_lut_path_female"),
            ghoul_male_correspondence: path("correspondence_path_ghoul_male"),
            ghoul_female_correspondence: path("correspondence_path_ghoul_female"),
            ghoul_male_uv_lut: path("uv_lut_path_ghoul_male"),
            ghoul_female_uv_lut: path("uv_lut_path_ghoul_female"),
            child_male_correspondence: path("correspondence_path_child_male"),
            child_female_correspondence: path("correspondence_path_child_female"),
            child_male_uv_lut: path("uv_lut_path_child_male"),
            child_female_uv_lut: path("uv_lut_path_child_female"),
        }
    }

    fn correspondence(&self, race_class: RaceClass) -> Option<&Path> {
        match race_class {
            RaceClass::HumanMale => self.human_male_correspondence.as_deref(),
            RaceClass::HumanFemale => self.human_female_correspondence.as_deref(),
            RaceClass::GhoulMale => self.ghoul_male_correspondence.as_deref(),
            RaceClass::GhoulFemale => self.ghoul_female_correspondence.as_deref(),
            RaceClass::ChildMale => self.child_male_correspondence.as_deref(),
            RaceClass::ChildFemale => self.child_female_correspondence.as_deref(),
            RaceClass::Unknown => None,
        }
    }

    fn uv_lut(&self, race_class: RaceClass) -> Option<&Path> {
        match race_class {
            RaceClass::HumanMale => self.human_male_uv_lut.as_deref(),
            RaceClass::HumanFemale => self.human_female_uv_lut.as_deref(),
            RaceClass::GhoulMale => self.ghoul_male_uv_lut.as_deref(),
            RaceClass::GhoulFemale => self.ghoul_female_uv_lut.as_deref(),
            RaceClass::ChildMale => self.child_male_uv_lut.as_deref(),
            RaceClass::ChildFemale => self.child_female_uv_lut.as_deref(),
            RaceClass::Unknown => None,
        }
    }
}

fn load_face_bake_profile(
    race_class: RaceClass,
    source_profile: SourceHeadProfile,
    source_extracted: &Path,
    target_extracted: &Path,
    target_face_assets: &TargetFaceAssets,
    face_resources: &FaceResourcePaths,
) -> Result<FaceBakeProfile, String> {
    use nif_core_native::model::{NifFile, NifValue};

    let source_head_nif = find_source_head_nif(source_extracted, race_class, source_profile)
        .ok_or_else(|| {
            format!(
                "source head NIF not found for {race_class:?} in {}",
                source_extracted.display()
            )
        })?;
    let source_head_egm = find_source_head_egm(source_extracted, race_class, source_profile)
        .ok_or_else(|| {
            format!(
                "source head EGM not found for {race_class:?} in {}",
                source_extracted.display()
            )
        })?;
    let target_base_head_nif = target_face_assets
        .head(race_class)
        .map(Path::to_path_buf)
        .or_else(|| find_target_base_head_nif(target_extracted, race_class))
        .ok_or_else(|| {
            format!(
                "target base head NIF not found for {race_class:?} in {}",
                target_extracted.display()
            )
        })?;
    let target_facegeom_template = find_target_facegeom_template(target_extracted, race_class)
        .ok_or_else(|| {
            format!(
                "target FaceGen template not found for {race_class:?} in {}",
                target_extracted.display()
            )
        })?;
    let correspondence_path = face_resources
        .correspondence(race_class)
        .ok_or_else(|| format!("no correspondence path for {race_class:?}"))?;

    let fnv_neutral = parse_tri_neutral_vertices(&source_head_nif.with_extension("tri"))?;
    let (egm_num_diffs, egm_num_verts, egm_basis) = parse_egm_basis(&source_head_egm)?;
    if fnv_neutral.len() != egm_num_verts {
        return Err(format!(
            "EGM vertex count {} != TRI neutral vertex count {} for {race_class:?}",
            egm_num_verts,
            fnv_neutral.len()
        ));
    }

    let correspondence = Correspondence::load(correspondence_path)?;
    let fo4_neutral = load_fo4_neutral_vertices(&target_base_head_nif)?;
    if correspondence.sample_count != fo4_neutral.len() {
        return Err(format!(
            "correspondence output {} != FO4 neutral vertex count {} for {race_class:?}",
            correspondence.sample_count,
            fo4_neutral.len()
        ));
    }

    let mut template = NifFile::load(target_facegeom_template.clone())
        .map_err(|e| format!("load FaceGen template NIF: {e}"))?;
    strip_variable_face_shapes(&mut template);
    let expected_name = race_class
        .primary_head_shape_name()
        .ok_or_else(|| "unknown race has no primary head shape".to_string())?;
    let schema = nif_core_native::schema::NifSchema::from_generated();
    let shape_ids: Vec<usize> = (0..template.blocks.len())
        .filter(|&index| {
            template.get_block(index).is_some_and(|block| {
                schema.is_subtype_of(&block.type_name, "BSTriShape")
                    && matches!(block.get_field("Name"), Some(NifValue::String(name)) if name.trim_end_matches('\0').eq_ignore_ascii_case(expected_name))
                    && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(_)))
            })
        })
        .collect();
    if shape_ids.len() != 1 {
        return Err(format!(
            "expected one {expected_name} shape in {}, got {}",
            target_facegeom_template.display(),
            shape_ids.len()
        ));
    }

    let face_bones = target_face_assets
        .face_bones(race_class)
        .map(Path::to_path_buf)
        .or_else(|| find_target_face_bones_nif(target_extracted, race_class))
        .and_then(|path| match extract_face_bones_skin(&path) {
            Ok(value) => Some(value),
            Err(error) => {
                eprintln!("[convert_face] WARN: cached bone solve input failed: {error}");
                None
            }
        });
    let uv_lut = face_resources
        .uv_lut(race_class)
        .and_then(|path| load_uv_lut(path).ok());

    Ok(FaceBakeProfile {
        fnv_neutral,
        egm_num_diffs,
        egm_num_verts,
        egm_basis,
        correspondence,
        fo4_neutral,
        template,
        template_shape_id: shape_ids[0],
        face_bones,
        uv_lut,
    })
}

fn load_skyrim_face_bake_profile(
    race_class: RaceClass,
    source_extracted: &Path,
    target_extracted: &Path,
    target_face_assets: &TargetFaceAssets,
) -> Result<SkyrimFaceBakeProfile, String> {
    use nif_core_native::model::{NifFile, NifValue};

    let is_female = matches!(race_class, RaceClass::HumanFemale);
    if !matches!(race_class, RaceClass::HumanMale | RaceClass::HumanFemale) {
        return Err(format!("unsupported Skyrim face profile {race_class:?}"));
    }
    let source_head_nif =
        find_skyrim_base_head_nif(source_extracted, is_female).ok_or_else(|| {
            format!(
                "Skyrim base {} head NIF not found in {}",
                if is_female { "female" } else { "male" },
                source_extracted.display()
            )
        })?;
    let target_base_head_nif = target_face_assets
        .head(race_class)
        .map(Path::to_path_buf)
        .or_else(|| find_target_base_head_nif(target_extracted, race_class))
        .ok_or_else(|| {
            format!(
                "target base head NIF not found for {race_class:?} in {}",
                target_extracted.display()
            )
        })?;
    let target_facegeom_template = find_target_facegeom_template(target_extracted, race_class)
        .ok_or_else(|| {
            format!(
                "target FaceGen template not found for {race_class:?} in {}",
                target_extracted.display()
            )
        })?;

    let skyrim_neutral = load_skyrim_head_vertices(&source_head_nif)?;
    let fo4_neutral = load_fo4_neutral_vertices(&target_base_head_nif)?;
    let correspondence = Correspondence::nearest_normalized(&skyrim_neutral, &fo4_neutral)?;
    let deformation_scale = deformation_scale(&skyrim_neutral, &fo4_neutral)?;

    let mut template = NifFile::load(target_facegeom_template.clone())
        .map_err(|error| format!("load FaceGen template NIF: {error}"))?;
    strip_variable_face_shapes(&mut template);
    let expected_name = race_class
        .primary_head_shape_name()
        .ok_or_else(|| "unknown race has no primary head shape".to_string())?;
    let schema = nif_core_native::schema::NifSchema::from_generated();
    let shape_ids = (0..template.blocks.len())
        .filter(|&index| {
            template.get_block(index).is_some_and(|block| {
                schema.is_subtype_of(&block.type_name, "BSTriShape")
                    && matches!(block.get_field("Name"), Some(NifValue::String(name)) if name.trim_end_matches('\0').eq_ignore_ascii_case(expected_name))
                    && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(_)))
            })
        })
        .collect::<Vec<_>>();
    if shape_ids.len() != 1 {
        return Err(format!(
            "expected one {expected_name} shape in {}, got {}",
            target_facegeom_template.display(),
            shape_ids.len()
        ));
    }

    let face_bones_path = target_face_assets
        .face_bones(race_class)
        .map(Path::to_path_buf)
        .or_else(|| find_target_face_bones_nif(target_extracted, race_class));
    let face_bones =
        face_bones_path
            .as_deref()
            .and_then(|path| match extract_face_bones_skin(path) {
                Ok(value) => Some(value),
                Err(error) => {
                    eprintln!("[convert_face] WARN: Skyrim bone solve input failed: {error}");
                    None
                }
            });
    let face_bones_reference = face_bones_path.and_then(|path| match NifFile::load(path.clone()) {
        Ok(nif) => Some(nif),
        Err(error) => {
            eprintln!(
                "[convert_face] WARN: Skyrim face skin reference {} failed: {error}",
                path.display()
            );
            None
        }
    });

    Ok(SkyrimFaceBakeProfile {
        race_class,
        skyrim_neutral,
        correspondence,
        deformation_scale,
        fo4_neutral,
        template,
        template_shape_id: shape_ids[0],
        face_bones,
        face_bones_reference,
    })
}

fn run_skyrim_face_phase(ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    let p = ctx.params;
    let auto_discover_npcs = p.get("npc_form_keys").is_none();
    let requested_npc_form_keys = p
        .get("npc_form_keys")
        .and_then(JsonValue::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let source_extracted = p
        .get("source_extracted")
        .and_then(JsonValue::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| ctx.source_extracted_dir.to_path_buf());
    let target_extracted = p
        .get("target_extracted")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| ctx.target_extracted_dir.map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("/nonexistent"));
    let output_plugin_name = p
        .get("output_plugin_name")
        .and_then(JsonValue::as_str)
        .unwrap_or("Output.esp")
        .to_string();
    let target_face_assets = TargetFaceAssets::from_params(p);
    let translation_maps_dir = p
        .get("translation_maps_dir")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    let named_bones_yaml = if let Some(path) = p
        .get("named_bones_path")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
    {
        std::fs::read_to_string(path)
            .map_err(|error| PhaseError::BadParams(format!("named_bones_path: {error}")))?
    } else {
        EMBEDDED_NAMED_BONES_YAML.to_string()
    };
    let named_bones = load_named_bones(&named_bones_yaml)
        .map_err(|error| PhaseError::Internal(format!("named_bones: {error}")))?;
    let hair_table_yaml = if let Some(path) = p
        .get("hair_table_path")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
    {
        std::fs::read_to_string(path)
            .map_err(|error| PhaseError::BadParams(format!("hair_table_path: {error}")))?
    } else {
        EMBEDDED_HAIR_LOOKUP_YAML.to_string()
    };
    let hair_table = HairTable::load(&hair_table_yaml)
        .map_err(|error| PhaseError::Internal(format!("hair_table: {error}")))?;

    let source_handle_id = ctx.run.source_handle_id;
    let target_handle_id = ctx.run.target_handle_id;
    let schema_source = ctx.run.schema_source.clone();
    let schema_target = ctx.run.schema_target.clone();
    let source_to_target = ctx
        .run
        .mapper_state
        .as_ref()
        .map(|state| state.source_to_target.clone())
        .unwrap_or_default();
    let mut npc_form_keys = if auto_discover_npcs {
        iter_form_keys_of_sig(
            source_handle_id,
            SigCode::from_str("NPC_").expect("NPC_ signature"),
            &ctx.run.interner,
        )
        .map_err(|error| PhaseError::Internal(error.to_string()))?
    } else {
        requested_npc_form_keys
            .iter()
            .filter_map(|value| {
                parse_form_key_str(value, &ctx.run.interner).map(|(_, form_key)| form_key)
            })
            .collect()
    };
    npc_form_keys.retain(|form_key| source_to_target.contains_key(form_key));
    npc_form_keys.sort_by_key(|form_key| form_key.local);

    let total = npc_form_keys.len() as u32;
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
        phase: "convert_face",
        current: 0,
        total,
        item: None,
    });

    let mut bake_jobs = Vec::new();
    let mut records_to_replace = Vec::new();
    let mut face_part_assets: HashMap<HeadPartSpec, Result<Arc<FacePartAsset>, String>> =
        HashMap::new();
    let mut source_face_kinds = HashMap::<FormKey, Option<SkyrimHumanoidFaceKind>>::new();
    let mut records_dropped = 0u32;
    let mut warnings = 0u32;
    let mut completed = 0u32;

    for source_form_key in npc_form_keys {
        ctx.check_cancel()?;
        let interner = &ctx.run.interner;
        let source_fk = form_key_to_read_str(&source_form_key, interner);
        let Some(target_form_key) = source_to_target.get(&source_form_key).copied() else {
            continue;
        };
        let target_fk = form_key_to_read_str(&target_form_key, interner);
        let source_record =
            match read_record(source_handle_id, &source_fk, &schema_source, interner) {
                Ok(record) => record,
                Err(error) => {
                    eprintln!("[convert_face] WARN: cannot read Skyrim NPC {source_fk}: {error}");
                    warnings += 1;
                    completed += 1;
                    continue;
                }
            };
        let mut target_record =
            match read_record(target_handle_id, &target_fk, &schema_target, interner) {
                Ok(record) => record,
                Err(_) => {
                    warnings += 1;
                    completed += 1;
                    continue;
                }
            };
        let eid = source_record
            .eid
            .and_then(|symbol| interner.resolve(symbol))
            .unwrap_or("")
            .to_string();
        let is_female = extract_is_female(&source_record);
        let Some(source_race) = extract_rnam_form_key(&source_record) else {
            completed += 1;
            continue;
        };
        let source_face_kind = *source_face_kinds.entry(source_race).or_insert_with(|| {
            let race_fk = form_key_to_read_str(&source_race, interner);
            read_record(source_handle_id, &race_fk, &schema_source, interner)
                .ok()
                .and_then(|race| race.eid)
                .and_then(|symbol| interner.resolve(symbol))
                .and_then(source_humanoid_face_kind)
        });
        let Some(source_face_kind) = source_face_kind else {
            completed += 1;
            continue;
        };
        let mapped_race = source_to_target.get(&source_race).copied();
        let race_class = classify_mapped_race(mapped_race, is_female);
        if !matches!(
            race_class,
            RaceClass::HumanMale
                | RaceClass::HumanFemale
                | RaceClass::ChildMale
                | RaceClass::ChildFemale
        ) {
            completed += 1;
            continue;
        }

        let mut variable_parts = Vec::new();
        if matches!(source_face_kind, SkyrimHumanoidFaceKind::Human)
            && let Some(spec) = hair_table
                .lookup(None, race_class)
                .and_then(|hair| target_hair_spec(&hair))
        {
            let loaded = face_part_assets
                .entry(spec.clone())
                .or_insert_with(|| {
                    load_face_part_asset(&target_extracted, &source_extracted, spec).map(Arc::new)
                })
                .clone();
            match loaded {
                Ok(asset) => variable_parts.push(asset),
                Err(error) => {
                    eprintln!("[convert_face] WARN [{eid}] {error}");
                    warnings += 1;
                }
            }
        }
        let pnam_refs_target = if matches!(
            source_face_kind,
            SkyrimHumanoidFaceKind::Argonian
                | SkyrimHumanoidFaceKind::Khajiit
                | SkyrimHumanoidFaceKind::Dremora
        ) {
            target_preserved_face_part_refs(race_class)
        } else {
            target_head_part_refs(race_class, &variable_parts)
        };
        let body_weight = extract_skyrim_body_weight(&source_record);
        let source_hair_color = None;

        if matches!(race_class, RaceClass::ChildMale | RaceClass::ChildFemale) {
            apply_race_default_fields(
                &mut target_record,
                &hair_table,
                None,
                race_class,
                body_weight,
                source_hair_color,
                &pnam_refs_target,
                interner,
            );
            records_to_replace.push(target_record);
            records_dropped += 1;
            completed += 1;
            continue;
        }

        let source_plugin_name = interner
            .resolve(source_form_key.plugin)
            .unwrap_or("")
            .to_string();
        let source_formid_hex = format!("{:08X}", source_form_key.local);
        let Some(source_facegeom) =
            find_skyrim_facegeom_nif(&source_extracted, &source_plugin_name, &source_formid_hex)
        else {
            apply_race_default_fields(
                &mut target_record,
                &hair_table,
                None,
                race_class,
                body_weight,
                source_hair_color,
                &pnam_refs_target,
                interner,
            );
            records_to_replace.push(target_record);
            records_dropped += 1;
            completed += 1;
            continue;
        };

        bake_jobs.push(SkyrimFaceBakeJob {
            target_record,
            source_facegeom,
            source_face_kind,
            race_class,
            body_weight,
            source_hair_color,
            variable_parts,
            pnam_refs_target,
            formid_hex: format!("{:08X}", target_form_key.local),
            eid,
        });
    }

    let mut profiles = HashMap::new();
    for race_class in bake_jobs.iter().map(|job| job.race_class) {
        profiles.entry(race_class).or_insert_with(|| {
            load_skyrim_face_bake_profile(
                race_class,
                &source_extracted,
                &target_extracted,
                &target_face_assets,
            )
        });
    }
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "convert_face",
        level: LogLevel::Info,
        message: format!(
            "convert_face: baking {} Skyrim NPC assets with {} worker(s)",
            bake_jobs.len(),
            rayon::current_num_threads()
        ),
    });

    let completed = AtomicU32::new(completed);
    let event_tx = ctx.run.event_tx.clone();
    let cancel = ctx.cancel;
    let mod_path = ctx.mod_path.to_path_buf();
    let bake_results = bake_jobs
        .par_iter()
        .map(|job| {
            let result = if cancel.load(Ordering::Relaxed) {
                Err("cancelled".to_string())
            } else {
                match profiles.get(&job.race_class) {
                    Some(Ok(profile)) => attempt_skyrim_face_bake_cached(
                        &job.formid_hex,
                        &job.source_facegeom,
                        &mod_path,
                        &output_plugin_name,
                        profile,
                        &named_bones,
                        &job.variable_parts,
                        job.source_face_kind,
                        translation_maps_dir.as_deref(),
                    ),
                    Some(Err(error)) => Err(error.clone()),
                    None => Err(format!("missing cached profile for {:?}", job.race_class)),
                }
            };
            let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = event_tx.try_send(PhaseEvent::Progress {
                phase: "convert_face",
                current,
                total,
                item: Some(job.eid.clone()),
            });
            result
        })
        .collect::<Vec<_>>();

    ctx.check_cancel()?;
    let mut records_changed = 0u32;
    let interner = &ctx.run.interner;
    for (job, bake_result) in bake_jobs.into_iter().zip(bake_results) {
        let mut target_record = job.target_record;
        match bake_result {
            Ok(result) => {
                replace_face_fields_in_record(
                    &mut target_record,
                    &job.pnam_refs_target,
                    &result.bone_offsets,
                    job.body_weight,
                    job.source_hair_color,
                    job.race_class,
                    &named_bones,
                    interner,
                );
                records_changed += 1;
            }
            Err(error) => {
                eprintln!(
                    "[convert_face] WARN [{}] Skyrim bake failed: {error}",
                    job.eid
                );
                apply_race_default_fields(
                    &mut target_record,
                    &hair_table,
                    None,
                    job.race_class,
                    job.body_weight,
                    job.source_hair_color,
                    &job.pnam_refs_target,
                    interner,
                );
                records_dropped += 1;
                warnings += 1;
            }
        }
        records_to_replace.push(target_record);
    }

    replace_records_native(
        target_handle_id,
        records_to_replace,
        &schema_target,
        interner,
    )
    .map_err(|error| PhaseError::Internal(format!("batch write Skyrim NPC faces: {error}")))?;
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "convert_face",
        level: LogLevel::Info,
        message: format!(
            "convert_face: {records_changed} Skyrim faces baked, {records_dropped} defaulted, {warnings} warnings"
        ),
    });
    Ok(PhaseReport {
        records_changed,
        records_dropped,
        warnings,
        ..Default::default()
    })
}

impl Phase for ConvertFacePhase {
    fn name(&self) -> &'static str {
        "convert_face"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source == crate::translator::Game::SkyrimSe {
            return run_skyrim_face_phase(ctx);
        }
        let p = ctx.params;

        // --- Parse params ---
        let auto_discover_npcs = p.get("npc_form_keys").is_none();
        let requested_npc_form_keys: Vec<String> = p
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
        let face_resources = FaceResourcePaths::from_params(p);

        let output_plugin_name = p
            .get("output_plugin_name")
            .and_then(|v| v.as_str())
            .unwrap_or("Output.esp")
            .to_string();

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
        let source_to_target = ctx
            .run
            .mapper_state
            .as_ref()
            .map(|state| state.source_to_target.clone())
            .unwrap_or_default();
        let mut npc_form_keys = if auto_discover_npcs {
            iter_form_keys_of_sig(
                source_handle_id,
                SigCode::from_str("NPC_").expect("NPC_ signature"),
                &ctx.run.interner,
            )
            .map_err(|error| PhaseError::Internal(error.to_string()))?
        } else {
            requested_npc_form_keys
                .iter()
                .filter_map(|value| {
                    parse_form_key_str(value, &ctx.run.interner).map(|(_, key)| key)
                })
                .collect()
        };
        npc_form_keys.retain(|key| source_to_target.contains_key(key));
        npc_form_keys.sort_by_key(|key| key.local);

        let mut records_changed: u32 = 0;
        let mut records_dropped: u32 = 0;
        let mut warnings: u32 = 0;
        let mut bake_jobs = Vec::new();
        let mut records_to_replace = Vec::new();
        let mut face_part_assets: HashMap<HeadPartSpec, Result<Arc<FacePartAsset>, String>> =
            HashMap::new();
        let mut completed = 0u32;

        let total = npc_form_keys.len() as u32;

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
            phase: self.name(),
            current: 0,
            total,
            item: None,
        });

        for source_form_key in &npc_form_keys {
            ctx.check_cancel()?;

            let interner = &ctx.run.interner;
            let fk_str = form_key_to_read_str(source_form_key, interner);
            let Some(target_form_key) = source_to_target.get(source_form_key).copied() else {
                warnings += 1;
                continue;
            };
            let target_fk_str = form_key_to_read_str(&target_form_key, interner);

            // Read source NPC record
            let source_record =
                match read_record(source_handle_id, &fk_str, &schema_source, interner) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("[convert_face] WARN: cannot read source NPC {fk_str}: {e}");
                        warnings += 1;
                        completed += 1;
                        let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                            phase: self.name(),
                            current: completed,
                            total,
                            item: Some(fk_str),
                        });
                        continue;
                    }
                };

            let eid = source_record
                .eid
                .and_then(|sym| interner.resolve(sym))
                .unwrap_or("")
                .to_string();

            let is_female = extract_is_female(&source_record);
            let source_race = extract_rnam_form_key(&source_record);
            let mapped_race = source_race.and_then(|race| source_to_target.get(&race).copied());
            let race_class = match classify_mapped_race(mapped_race, is_female) {
                RaceClass::Unknown => source_race
                    .map(|race| classify_source_race(&format!("{:06X}", race.local), is_female))
                    .unwrap_or(RaceClass::Unknown),
                class => class,
            };
            let source_head_profile = classify_source_head_profile(source_race, race_class);

            let coefficients = extract_fggs_coefficients(&source_record).unwrap_or_default();

            // Read target NPC record
            let mut target_record =
                match read_record(target_handle_id, &target_fk_str, &schema_target, interner) {
                    Ok(r) => r,
                    Err(_) => {
                        warnings += 1;
                        completed += 1;
                        let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                            phase: self.name(),
                            current: completed,
                            total,
                            item: Some(eid),
                        });
                        continue;
                    }
                };

            let body_weight = extract_nam7_weight(&source_record);
            let source_hair_color = extract_hclr_rgb(&source_record, interner);
            let hair_ref_source = extract_hnam_ref(&source_record, interner);
            let pnam_refs_source = extract_pnam_refs(&source_record, interner);
            let mut variable_specs = Vec::new();
            if let Some(spec) = hair_ref_source.as_ref().and_then(|source| {
                exact_legacy_hair_spec(source_form_key.local, source, race_class)
            }) {
                variable_specs.push(spec);
            } else if let Some(hair) = hair_table.lookup(hair_ref_source.as_ref(), race_class) {
                if let Some(spec) = target_hair_spec(&hair) {
                    variable_specs.push(spec);
                }
            }
            if let Some(beard) = pnam_refs_source
                .iter()
                .filter_map(|source| target_beard_spec(source, race_class))
                .next_back()
            {
                variable_specs.push(beard);
            }
            variable_specs.sort_by_key(|spec| spec.local);
            variable_specs.dedup_by_key(|spec| spec.local);

            let mut variable_parts = Vec::new();
            for spec in &variable_specs {
                let loaded = face_part_assets
                    .entry(spec.clone())
                    .or_insert_with(|| {
                        load_face_part_asset(&target_extracted, &source_extracted, spec.clone())
                            .map(Arc::new)
                    })
                    .clone();
                match loaded {
                    Ok(asset) => variable_parts.push(asset),
                    Err(error) => {
                        eprintln!("[convert_face] WARN [{eid}] {error}");
                        warnings += 1;
                    }
                }
            }
            let pnam_refs_target = target_head_part_refs(race_class, &variable_parts);

            if !race_class.is_bakeable() {
                // Degrade: use race defaults
                records_dropped += 1;
                apply_race_default_fields(
                    &mut target_record,
                    &hair_table,
                    hair_ref_source.as_ref(),
                    race_class,
                    body_weight,
                    source_hair_color,
                    &pnam_refs_target,
                    interner,
                );
                records_to_replace.push(target_record);
                completed += 1;
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                    phase: self.name(),
                    current: completed,
                    total,
                    item: Some(eid),
                });
                continue;
            }

            if !should_attempt_bake(race_class, &coefficients) {
                // Zero coefficients — degrade with race defaults
                records_dropped += 1;
                apply_race_default_fields(
                    &mut target_record,
                    &hair_table,
                    hair_ref_source.as_ref(),
                    race_class,
                    body_weight,
                    source_hair_color,
                    &pnam_refs_target,
                    interner,
                );
                records_to_replace.push(target_record);
                completed += 1;
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                    phase: self.name(),
                    current: completed,
                    total,
                    item: Some(eid),
                });
                continue;
            }

            bake_jobs.push(FaceBakeJob {
                target_record,
                coefficients,
                race_class,
                body_weight,
                source_hair_color,
                source_head_profile,
                hair_ref_source,
                variable_parts,
                pnam_refs_target,
                formid_hex: format!("{:08X}", target_form_key.local),
                source_formid_hex: format!("{:08X}", source_form_key.local),
                source_plugin_name: interner
                    .resolve(source_form_key.plugin)
                    .unwrap_or("")
                    .to_string(),
                eid,
            });
        }

        let mut profiles = HashMap::new();
        for profile_key in bake_jobs
            .iter()
            .map(|job| (job.race_class, job.source_head_profile))
        {
            profiles.entry(profile_key).or_insert_with(|| {
                load_face_bake_profile(
                    profile_key.0,
                    profile_key.1,
                    &source_extracted,
                    &target_extracted,
                    &target_face_assets,
                    &face_resources,
                )
            });
        }

        let worker_count = rayon::current_num_threads();
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "convert_face: baking {} NPC assets with {worker_count} worker(s)",
                bake_jobs.len()
            ),
        });

        let completed = AtomicU32::new(completed);
        let event_tx = ctx.run.event_tx.clone();
        let cancel = ctx.cancel;
        let bake_results: Vec<Result<BakeResult, String>> = bake_jobs
            .par_iter()
            .map(|job| {
                let result = if cancel.load(Ordering::Relaxed) {
                    Err("cancelled".to_string())
                } else {
                    match profiles.get(&(job.race_class, job.source_head_profile)) {
                        Some(Ok(profile)) => attempt_face_bake_cached(
                            &job.formid_hex,
                            &job.source_formid_hex,
                            &job.coefficients,
                            &source_extracted,
                            &mod_path,
                            &output_plugin_name,
                            &job.source_plugin_name,
                            profile,
                            &named_bones,
                            &job.variable_parts,
                        ),
                        Some(Err(error)) => Err(error.clone()),
                        None => Err(format!("missing cached profile for {:?}", job.race_class)),
                    }
                };
                let current = completed.fetch_add(1, Ordering::Relaxed) + 1;
                let _ = event_tx.try_send(PhaseEvent::Progress {
                    phase: "convert_face",
                    current,
                    total,
                    item: Some(job.eid.clone()),
                });
                result
            })
            .collect();

        ctx.check_cancel()?;

        let interner = &ctx.run.interner;
        for (job, bake_result) in bake_jobs.into_iter().zip(bake_results) {
            let mut target_record = job.target_record;
            match bake_result {
                Ok(result) => {
                    // Assemble baked fields onto target record
                    replace_face_fields_in_record(
                        &mut target_record,
                        &job.pnam_refs_target,
                        &result.bone_offsets,
                        job.body_weight,
                        job.source_hair_color,
                        job.race_class,
                        &named_bones,
                        interner,
                    );

                    records_to_replace.push(target_record);
                    records_changed += 1;
                }
                Err(e) => {
                    eprintln!("[convert_face] WARN [{}] bake failed: {e}", job.eid);
                    warnings += 1;
                    // Degrade
                    apply_race_default_fields(
                        &mut target_record,
                        &hair_table,
                        job.hair_ref_source.as_ref(),
                        job.race_class,
                        job.body_weight,
                        job.source_hair_color,
                        &job.pnam_refs_target,
                        interner,
                    );
                    records_to_replace.push(target_record);
                    records_dropped += 1;
                }
            }
        }

        replace_records_native(
            target_handle_id,
            records_to_replace,
            &schema_target,
            interner,
        )
        .map_err(|error| PhaseError::Internal(format!("batch write NPC faces: {error}")))?;

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
    _hair_table: &HairTable,
    _source_hair: Option<&HairRef>,
    race_class: RaceClass,
    body_weight: f32,
    source_hair_color: Option<[u8; 3]>,
    pnam_refs: &[(String, String)],
    interner: &StringInterner,
) {
    const FACE_SIGS: &[&str] = &[
        "BCLF", "PNAM", "HCLF", "MSDK", "MSDV", "MRSV", "FMRI", "FMRS", "FMIN", "MWGT", "NAM7",
    ];
    record
        .fields
        .retain(|f| !FACE_SIGS.contains(&f.sig.as_str()));

    // FMIN = 1.0
    let fmin_sig = crate::ids::SubrecordSig::from_str("FMIN").expect("FMIN is 4 bytes");
    let fmin_bytes = smallvec::SmallVec::from_slice(&1.0f32.to_le_bytes());
    record.fields.push(FieldEntry {
        sig: fmin_sig,
        value: FieldValue::Bytes(fmin_bytes),
    });

    // MWGT — FO4 body morph weights.
    let mwgt_sig = crate::ids::SubrecordSig::from_str("MWGT").expect("MWGT is 4 bytes");
    record.fields.push(FieldEntry {
        sig: mwgt_sig,
        value: FieldValue::Bytes(fo4_body_morph_bytes(body_weight)),
    });

    for (plugin, object_id) in pnam_refs {
        let Ok(local) = u32::from_str_radix(object_id.trim_start_matches("0x"), 16) else {
            continue;
        };
        add_head_part(record, plugin, local, interner);
    }
    set_template_hair_color(record, source_hair_color, race_class, interner);
}

/// Perform the full geometry bake for one NPC.
#[allow(clippy::too_many_arguments)]
fn attempt_face_bake_cached(
    formid_hex: &str,
    source_formid_hex: &str,
    coefficients: &[f32],
    source_extracted: &Path,
    mod_path: &Path,
    output_plugin_name: &str,
    source_plugin_name: &str,
    profile: &FaceBakeProfile,
    named_bones: &[NamedBone],
    variable_parts: &[Arc<FacePartAsset>],
) -> Result<BakeResult, String> {
    let fnv_vertices = reconstruct_fnv_face_from_basis(
        &profile.fnv_neutral,
        profile.egm_num_diffs,
        profile.egm_num_verts,
        &profile.egm_basis,
        coefficients,
    )?;
    let deformation = profile
        .correspondence
        .interpolate_deformation(&profile.fnv_neutral, &fnv_vertices)?;
    let fo4_neutral = &profile.fo4_neutral;

    if deformation.len() != fo4_neutral.len() {
        return Err(format!(
            "Correspondence output {} != FO4 neutral vertex count {}",
            deformation.len(),
            fo4_neutral.len()
        ));
    }

    let fo4_vertices: Vec<[f32; 3]> = fo4_neutral
        .iter()
        .zip(deformation.iter())
        .map(|(n, d)| [n[0] + d[0], n[1] + d[1], n[2] + d[2]])
        .collect();

    // Write facegeom NIF
    let facegeom_rel = facegeom_relpath(output_plugin_name, formid_hex);
    let facegeom_out = mod_path
        .join("data")
        .join(facegeom_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let mut assembled_template = profile.template.clone();
    let face_node = find_face_node(&assembled_template)
        .ok_or_else(|| "FaceGen template has no BSFaceGenNiNodeSkinned node".to_string())?;
    for asset in variable_parts {
        for (shape_index, source_shape_id) in asset.shape_ids.iter().enumerate() {
            let shape_name = if shape_index == 0 {
                asset.spec.shape_name.as_str()
            } else {
                asset
                    .nif
                    .get_block(*source_shape_id)
                    .and_then(|block| block.get_field("Name"))
                    .and_then(|value| match value {
                        nif_core_native::model::NifValue::String(name) => Some(name.as_str()),
                        _ => None,
                    })
                    .unwrap_or(asset.spec.shape_name.as_str())
            };
            let copied_shape_id = append_face_part_shape(
                &mut assembled_template,
                &asset.nif,
                *source_shape_id,
                face_node,
                shape_name,
            )?;
            if let Some(reference) = asset.skin_reference.as_ref() {
                nif_core_native::skin::rig_facegen_shape_from_reference(
                    &mut assembled_template,
                    copied_shape_id,
                    reference,
                )?;
            }
        }
    }
    nif_core_native::convert_file::finalize_assembled_facegeom_for_fo4(&mut assembled_template);
    write_facegeom_nif_from_template(
        &assembled_template,
        profile.template_shape_id,
        &fo4_neutral,
        &deformation,
        &facegeom_out,
    )?;

    let bone_offsets = if let Some((weights, bone_indices, bone_names)) = &profile.face_bones {
        solve_bone_offsets(
            fo4_neutral,
            &fo4_vertices,
            weights,
            bone_indices,
            bone_names,
            named_bones,
        )
    } else {
        BoneOffsets::new()
    };

    // Write facetint DDS
    let facetint_rel = facetint_relpath(output_plugin_name, formid_hex);
    let facetint_out = mod_path
        .join("data")
        .join(facetint_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let fallback_color = deterministic_facetint_color(formid_hex);
    let source_dds =
        find_source_facetint_dds(source_extracted, source_plugin_name, source_formid_hex);
    let _ = write_facetint_dds_with_uv(
        &facetint_out,
        source_dds.as_deref(),
        profile.uv_lut.as_ref(),
        fallback_color,
    );

    Ok(BakeResult {
        facegeom_relpath: facegeom_rel,
        facetint_relpath: facetint_rel,
        bone_offsets,
    })
}

fn attempt_skyrim_face_bake_cached(
    formid_hex: &str,
    source_facegeom: &Path,
    mod_path: &Path,
    output_plugin_name: &str,
    profile: &SkyrimFaceBakeProfile,
    named_bones: &[NamedBone],
    variable_parts: &[Arc<FacePartAsset>],
    source_face_kind: SkyrimHumanoidFaceKind,
    translation_maps_dir: Option<&Path>,
) -> Result<BakeResult, String> {
    if !matches!(source_face_kind, SkyrimHumanoidFaceKind::Human) {
        return attempt_preserved_skyrim_face_bake_cached(
            formid_hex,
            source_facegeom,
            mod_path,
            output_plugin_name,
            profile,
            source_face_kind,
            translation_maps_dir.ok_or_else(|| {
                format!("{source_face_kind:?} FaceGeom requires translation_maps_dir")
            })?,
        );
    }

    let skyrim_vertices = load_skyrim_head_vertices(source_facegeom)?;
    if skyrim_vertices.len() != profile.skyrim_neutral.len() {
        return Err(format!(
            "Skyrim FaceGeom vertex count {} != neutral head vertex count {} in {}",
            skyrim_vertices.len(),
            profile.skyrim_neutral.len(),
            source_facegeom.display()
        ));
    }
    let mut deformation = profile
        .correspondence
        .interpolate_deformation(&profile.skyrim_neutral, &skyrim_vertices)?;
    for vertex in &mut deformation {
        for axis in 0..3 {
            vertex[axis] *= profile.deformation_scale[axis];
        }
    }
    if deformation.len() != profile.fo4_neutral.len() {
        return Err(format!(
            "Skyrim correspondence output {} != FO4 neutral vertex count {}",
            deformation.len(),
            profile.fo4_neutral.len()
        ));
    }
    let fo4_vertices = profile
        .fo4_neutral
        .iter()
        .zip(&deformation)
        .map(|(neutral, delta)| {
            [
                neutral[0] + delta[0],
                neutral[1] + delta[1],
                neutral[2] + delta[2],
            ]
        })
        .collect::<Vec<_>>();

    let facegeom_rel = facegeom_relpath(output_plugin_name, formid_hex);
    let facegeom_out = mod_path
        .join("data")
        .join(facegeom_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let mut assembled_template = profile.template.clone();
    let face_node = find_face_node(&assembled_template)
        .ok_or_else(|| "FaceGen template has no BSFaceGenNiNodeSkinned node".to_string())?;
    for asset in variable_parts {
        for (shape_index, source_shape_id) in asset.shape_ids.iter().enumerate() {
            let shape_name = if shape_index == 0 {
                asset.spec.shape_name.as_str()
            } else {
                asset
                    .nif
                    .get_block(*source_shape_id)
                    .and_then(|block| block.get_field("Name"))
                    .and_then(|value| match value {
                        nif_core_native::model::NifValue::String(name) => Some(name.as_str()),
                        _ => None,
                    })
                    .unwrap_or(asset.spec.shape_name.as_str())
            };
            let copied_shape_id = append_face_part_shape(
                &mut assembled_template,
                &asset.nif,
                *source_shape_id,
                face_node,
                shape_name,
            )?;
            if let Some(reference) = asset.skin_reference.as_ref() {
                nif_core_native::skin::rig_facegen_shape_from_reference(
                    &mut assembled_template,
                    copied_shape_id,
                    reference,
                )?;
            }
        }
    }
    nif_core_native::convert_file::finalize_assembled_facegeom_for_fo4(&mut assembled_template);
    write_facegeom_nif_from_template(
        &assembled_template,
        profile.template_shape_id,
        &profile.fo4_neutral,
        &deformation,
        &facegeom_out,
    )?;

    let bone_offsets = if let Some((weights, bone_indices, bone_names)) = &profile.face_bones {
        solve_bone_offsets(
            &profile.fo4_neutral,
            &fo4_vertices,
            weights,
            bone_indices,
            bone_names,
            named_bones,
        )
    } else {
        BoneOffsets::new()
    };

    let facetint_rel = facetint_relpath(output_plugin_name, formid_hex);
    let facetint_out = mod_path
        .join("data")
        .join(facetint_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    let _ = write_facetint_dds_with_uv(
        &facetint_out,
        None,
        None,
        deterministic_facetint_color(formid_hex),
    );

    Ok(BakeResult {
        facegeom_relpath: facegeom_rel,
        facetint_relpath: facetint_rel,
        bone_offsets,
    })
}

fn attempt_preserved_skyrim_face_bake_cached(
    formid_hex: &str,
    source_facegeom: &Path,
    mod_path: &Path,
    output_plugin_name: &str,
    profile: &SkyrimFaceBakeProfile,
    source_face_kind: SkyrimHumanoidFaceKind,
    translation_maps_dir: &Path,
) -> Result<BakeResult, String> {
    use nif_core_native::convert_file::{ConvertFileOptions, convert_nif_file};
    use nif_core_native::model::{NifFile, NifValue};

    let skin_reference = profile
        .face_bones_reference
        .as_ref()
        .ok_or_else(|| format!("missing FO4 face-bones skin reference for {source_face_kind:?}"))?;
    let temporary = tempfile::Builder::new()
        .prefix("bacup-skyrim-facegeom-")
        .tempdir()
        .map_err(|error| format!("create temporary FaceGeom directory: {error}"))?;
    let converted_source_path = temporary.path().join("converted.nif");
    let materials_root = mod_path.join("data").join("Materials");
    let options = ConvertFileOptions {
        translation_maps_dir: Some(translation_maps_dir.to_path_buf()),
        ..ConvertFileOptions::default()
    };
    let report = convert_nif_file(
        source_facegeom,
        &converted_source_path,
        "skyrimse",
        "fo4",
        Some(&materials_root),
        &options,
    )
    .map_err(|error| format!("convert Skyrim {source_face_kind:?} FaceGeom: {error}"))?;
    if !report.supported || !report.errors.is_empty() {
        return Err(format!(
            "convert Skyrim {source_face_kind:?} FaceGeom: {}",
            report.errors.join("; ")
        ));
    }

    let mut source = NifFile::load(converted_source_path)
        .map_err(|error| format!("reopen converted Skyrim FaceGeom: {error}"))?;
    let schema = nif_core_native::schema::NifSchema::from_generated();
    let source_shape_ids = source
        .blocks
        .iter()
        .filter(|block| {
            schema.is_subtype_of(&block.type_name, "BSTriShape")
                && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(vertices)) if !vertices.is_empty())
        })
        .map(|block| block.block_id)
        .collect::<Vec<_>>();
    if source_shape_ids.is_empty() {
        return Err(format!(
            "converted {source_face_kind:?} FaceGeom has no geometry"
        ));
    }
    for shape_id in &source_shape_ids {
        let shape = &mut source.blocks[*shape_id];
        shape.set_field("Skin", NifValue::Ref(-1));
        shape.set_field("Skin Instance", NifValue::Ref(-1));
    }

    let mut assembled = profile.template.clone();
    strip_all_face_shapes(&mut assembled)?;
    let face_node = find_face_node(&assembled)
        .ok_or_else(|| "FaceGen template has no BSFaceGenNiNodeSkinned node".to_string())?;
    let mut has_head = false;
    let mut has_eyes = false;
    let mut has_mouth = false;
    for source_shape_id in source_shape_ids {
        let source_name = source.blocks[source_shape_id]
            .get_field("Name")
            .and_then(|value| match value {
                NifValue::String(name) => Some(name.as_str()),
                _ => None,
            })
            .unwrap_or("SkyrimFacePart");
        let lower = source_name.trim_end_matches('\0').to_ascii_lowercase();
        has_eyes |= lower.contains("eyes");
        has_mouth |= lower.contains("mouth");
        has_head |= lower.contains("head") && !lower.contains("hair");
        let target_name = preserved_face_shape_name(source_name, profile.race_class);
        let copied_shape_id = append_face_part_shape(
            &mut assembled,
            &source,
            source_shape_id,
            face_node,
            &target_name,
        )?;
        nif_core_native::skin::rig_facegen_shape_from_reference(
            &mut assembled,
            copied_shape_id,
            skin_reference,
        )?;
    }
    if !has_head || !has_eyes || !has_mouth {
        return Err(format!(
            "converted {source_face_kind:?} FaceGeom is incomplete (head={has_head}, eyes={has_eyes}, mouth={has_mouth})"
        ));
    }

    nif_core_native::convert_file::finalize_assembled_facegeom_for_fo4(&mut assembled);
    let facegeom_rel = facegeom_relpath(output_plugin_name, formid_hex);
    let facegeom_out = mod_path
        .join("data")
        .join(facegeom_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    if let Some(parent) = facegeom_out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    assembled
        .save(Some(facegeom_out.clone()))
        .map_err(|error| format!("write {}: {error}", facegeom_out.display()))?;

    let facetint_rel = facetint_relpath(output_plugin_name, formid_hex);
    let facetint_out = mod_path
        .join("data")
        .join(facetint_rel.replace('/', std::path::MAIN_SEPARATOR_STR));
    write_fallback_facetint_dds(&facetint_out, deterministic_facetint_color(formid_hex))?;

    Ok(BakeResult {
        facegeom_relpath: facegeom_rel,
        facetint_relpath: facetint_rel,
        bone_offsets: BoneOffsets::new(),
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

    #[test]
    fn tri_neutral_includes_modifier_vertices_from_header_offset_44() {
        let mut bytes = vec![0u8; TRI_HEADER_SIZE + 5 * 12];
        bytes[..8].copy_from_slice(TRI_MAGIC);
        bytes[8..12].copy_from_slice(&2u32.to_le_bytes());
        bytes[36..40].copy_from_slice(&1u32.to_le_bytes());
        bytes[44..48].copy_from_slice(&3u32.to_le_bytes());
        for (index, chunk) in bytes[TRI_HEADER_SIZE..].chunks_exact_mut(12).enumerate() {
            chunk[..4].copy_from_slice(&(index as f32).to_le_bytes());
        }
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), bytes).unwrap();

        let vertices = parse_tri_neutral_vertices(file.path()).unwrap();

        assert_eq!(vertices.len(), 5);
        assert_eq!(vertices[4][0], 4.0);
    }

    #[test]
    fn egm_parser_retains_symmetric_and_asymmetric_morphs_exactly() {
        let mut bytes = vec![0u8; EGM_HEADER_SIZE];
        bytes[..EGM_MAGIC.len()].copy_from_slice(EGM_MAGIC);
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        for (scale, values) in [(0.5f32, [2i16, 4, 6]), (0.25, [8, 12, 16])] {
            bytes.extend_from_slice(&scale.to_le_bytes());
            for value in values {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), bytes).unwrap();

        let parsed = parse_egm_basis_full(file.path()).unwrap();
        assert_eq!(parsed.symmetric_count, 1);
        assert_eq!(parsed.asymmetric_count, 1);
        assert_eq!(parsed.vertex_count, 1);
        assert_eq!(parsed.basis, vec![1.0, 2.0, 3.0, 2.0, 3.0, 4.0]);
        let deformed = reconstruct_fnv_face_from_basis(
            &[[10.0, 20.0, 30.0]],
            2,
            1,
            &parsed.basis,
            &[1.0, 2.0],
        )
        .unwrap();
        assert_eq!(deformed, vec![[15.0, 28.0, 41.0]]);
    }

    #[test]
    fn strict_facegen_rejects_unknown_egm_bytes_and_fallback_tint() {
        let mut bytes = vec![0u8; EGM_HEADER_SIZE];
        bytes[..EGM_MAGIC.len()].copy_from_slice(EGM_MAGIC);
        bytes.push(0);
        let egm = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(egm.path(), bytes).unwrap();
        assert!(
            parse_egm_basis_full(egm.path())
                .unwrap_err()
                .contains("payload length mismatch")
        );

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.dds");
        let output = temp.path().join("output.dds");
        std::fs::write(&source, b"not a DDS").unwrap();
        assert!(
            write_facetint_dds_with_required_uv(&output, &source, &(vec![0.0, 0.0], 1, 1))
                .unwrap_err()
                .contains("read exact source FaceTint DDS")
        );
        assert!(!output.exists());
    }

    #[test]
    fn checked_in_correspondence_loads_from_zip64_npz() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/phase/resources/face/fnv_to_fo4_correspondence_male.npz");

        let correspondence = Correspondence::load(&path).unwrap();

        assert_eq!(correspondence.sample_count, 1696);
        assert_eq!(correspondence.triangle_indices.len(), 1696);
        assert_eq!(correspondence.barycentrics.len(), 1696);
    }

    #[test]
    fn correspondence_transfers_deformation_not_source_coordinate_frame() {
        let correspondence = Correspondence {
            triangle_indices: vec![[0, 1, 2]],
            barycentrics: vec![[0.25, 0.25, 0.5]],
            sample_count: 1,
        };
        let neutral = vec![[10.0, 20.0, 30.0], [14.0, 20.0, 30.0], [10.0, 24.0, 30.0]];
        let deformed = vec![[11.0, 18.0, 33.0], [15.0, 18.0, 33.0], [11.0, 22.0, 33.0]];

        let deformation = correspondence
            .interpolate_deformation(&neutral, &deformed)
            .unwrap();

        assert_eq!(deformation, vec![[1.0, -2.0, 3.0]]);
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

    #[test]
    fn classify_source_ghoul_uses_npc_sex() {
        assert_eq!(classify_source_race("003b3e", false), RaceClass::GhoulMale);
        assert_eq!(classify_source_race("003b3e", true), RaceClass::GhoulFemale);
        assert_eq!(classify_source_race("0083d7", false), RaceClass::GhoulMale);
    }

    #[test]
    fn classify_source_child_uses_npc_sex() {
        for object_id in FNV_CHILD_IDS {
            assert_eq!(classify_source_race(object_id, false), RaceClass::ChildMale);
            assert_eq!(
                classify_source_race(object_id, true),
                RaceClass::ChildFemale
            );
        }
    }

    // ── should_attempt_bake ──────────────────────────────────────────────────

    #[test]
    fn should_bake_human_male_with_nonzero_coeffs() {
        let coeffs = vec![0.5f32; 50];
        assert!(should_attempt_bake(RaceClass::HumanMale, &coeffs));
    }

    #[test]
    fn should_bake_ghoul_with_nonzero_coeffs() {
        let coeffs = vec![0.5f32; 50];
        assert!(should_attempt_bake(RaceClass::GhoulMale, &coeffs));
        assert!(should_attempt_bake(RaceClass::GhoulFemale, &coeffs));
    }

    #[test]
    fn should_bake_child_with_nonzero_coeffs() {
        let coeffs = vec![0.5f32; 50];
        assert!(should_attempt_bake(RaceClass::ChildMale, &coeffs));
        assert!(should_attempt_bake(RaceClass::ChildFemale, &coeffs));
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

    #[test]
    fn unmapped_source_hair_does_not_collapse_every_npc_to_one_style() {
        let table = HairTable::load(EMBEDDED_HAIR_LOOKUP_YAML).unwrap();
        let even = HairRef {
            plugin: "FalloutNV.esm".to_string(),
            object_id: "000100".to_string(),
        };
        let odd = HairRef {
            plugin: "FalloutNV.esm".to_string(),
            object_id: "000101".to_string(),
        };

        assert_ne!(
            table.lookup(Some(&even), RaceClass::HumanMale),
            table.lookup(Some(&odd), RaceClass::HumanMale)
        );
        assert_ne!(
            table.lookup(Some(&even), RaceClass::HumanFemale),
            table.lookup(Some(&odd), RaceClass::HumanFemale)
        );
    }

    #[test]
    fn easy_pete_old_hair_uses_source_geometry_with_fo4_combover_fallback() {
        let table = HairTable::load(EMBEDDED_HAIR_LOOKUP_YAML).unwrap();
        for plugin in ["FalloutNV.esm"] {
            let source = HairRef {
                plugin: plugin.to_string(),
                object_id: "0987D9".to_string(),
            };
            let exact = exact_legacy_hair_spec(0x0010_4C7F, &source, RaceClass::HumanMale).unwrap();
            assert_eq!(exact.shape_name, "Hat");
            assert_eq!(
                exact.source_model_path.as_deref(),
                Some("Meshes/Characters/Hair/HairBaseOld.NIF")
            );
            let mapped = table.lookup(Some(&source), RaceClass::HumanMale).unwrap();
            assert_eq!(mapped.plugin, "Fallout4.esm");
            assert_eq!(mapped.object_id, "1477D0");
            assert_eq!(
                target_hair_spec(&mapped).unwrap(),
                HeadPartSpec {
                    plugin: "Fallout4.esm".to_string(),
                    local: 0x0014_77D0,
                    model_path: "Meshes/Actors/Character/CharacterAssets/Hair/Male/Hair18.nif"
                        .to_string(),
                    source_model_path: None,
                    shape_name: "HairMale18".to_string(),
                }
            );
            assert!(exact_legacy_hair_spec(0x0010_4C80, &source, RaceClass::HumanMale).is_none());
        }
    }

    #[test]
    fn source_eyebrows_are_not_misclassified_as_beards() {
        for object_id in ["0B5BEA", "0B6DE4", "0B4016", "0B4017", "0B4018", "0B4019"] {
            let source = HairRef {
                plugin: "FalloutNV.esm".to_string(),
                object_id: object_id.to_string(),
            };
            assert!(target_beard_spec(&source, RaceClass::HumanMale).is_none());
        }
    }

    #[test]
    fn known_source_beard_uses_the_matching_fo4_style() {
        let source = HairRef {
            plugin: "FalloutNV.esm".to_string(),
            object_id: "066FD0".to_string(),
        };
        let target = target_beard_spec(&source, RaceClass::HumanMale).unwrap();

        assert_eq!(target.local, 0x0002_9671);
        assert_eq!(target.shape_name, "Beard20");
    }

    #[test]
    fn easy_pete_uses_old_head_white_hair_and_full_beard() {
        let interner = StringInterner::new();
        let old_race = FormKey {
            plugin: interner.intern("FalloutNV.esm"),
            local: 0x0987DE,
        };
        let mut source = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey {
                plugin: interner.intern("FalloutNV.esm"),
                local: 0x104C7F,
            },
        );
        source.fields.push(FieldEntry {
            sig: crate::ids::SubrecordSig::from_str("HCLR").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![192, 192, 192, 0]),
        });
        let beard = target_beard_spec(
            &HairRef {
                plugin: "FalloutNV.esm".to_string(),
                object_id: "09F923".to_string(),
            },
            RaceClass::HumanMale,
        )
        .unwrap();

        assert_eq!(
            classify_source_head_profile(Some(old_race), RaceClass::HumanMale),
            SourceHeadProfile::Old
        );
        assert_eq!(extract_hclr_rgb(&source, &interner), Some([192; 3]));
        assert_eq!(
            target_hair_color(extract_hclr_rgb(&source, &interner), RaceClass::HumanMale),
            Some(0x19EE5A)
        );
        assert_eq!(beard.local, 0x0617BC);
        assert_eq!(beard.shape_name, "BeardFullOld:0");
        assert_eq!(
            beard.source_model_path.as_deref(),
            Some("Meshes/Characters/Hair/BeardFullOld.NIF")
        );
    }

    #[test]
    fn easy_pete_face_part_uses_converted_beard_full_old_geometry_when_assets_exist() {
        use nif_core_native::model::{NifFile, NifValue};

        let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| {
                path.join("extracted/fnv").is_dir() && path.join("extracted/fo4").is_dir()
            })
        else {
            return;
        };
        let source_root = repo_root.join("extracted/fnv");
        let source_path = source_root.join("Meshes/Characters/Hair/BeardFullOld.NIF");
        if !source_path.is_file() {
            return;
        }
        let source_nif = NifFile::load(&source_path).unwrap();
        let source_vertices = source_nif
            .blocks
            .iter()
            .find(|block| block.type_name == "NiTriShapeData")
            .and_then(|block| block.get_field("Num Vertices"))
            .and_then(|value| match value {
                NifValue::UInt(value) => Some(*value),
                _ => None,
            })
            .unwrap();
        let asset = load_face_part_asset(
            &repo_root.join("extracted/fo4"),
            &source_root,
            target_beard_spec(
                &HairRef {
                    plugin: "FalloutNV.esm".to_string(),
                    object_id: "09F923".to_string(),
                },
                RaceClass::HumanMale,
            )
            .unwrap(),
        )
        .unwrap();
        let shape = asset.nif.get_block(asset.shape_ids[0]).unwrap();

        assert_eq!(shape.type_name, "BSSubIndexTriShape");
        assert!(matches!(
            shape.get_field("Name"),
            Some(NifValue::String(name)) if name == "BeardFullOld:0"
        ));
        assert_eq!(
            shape.get_field("Num Vertices"),
            Some(&NifValue::UInt(source_vertices))
        );
        assert!(asset.nif.blocks.iter().any(|block| {
            block.type_name == "BSShaderTextureSet"
                && matches!(block.get_field("Textures"), Some(NifValue::Array(paths)) if paths.iter().any(|path| matches!(path, NifValue::String(path) if path.eq_ignore_ascii_case("textures\\characters\\hair\\BeardFull.dds"))))
        }));
    }

    #[test]
    fn easy_pete_face_part_uses_only_the_source_hat_hair_variant_when_assets_exist() {
        use nif_core_native::model::NifValue;

        let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| {
                path.join("extracted/fnv").is_dir() && path.join("extracted/fo4").is_dir()
            })
        else {
            return;
        };
        let source_root = repo_root.join("extracted/fnv");
        if !source_root
            .join("Meshes/Characters/Hair/HairBaseOld.NIF")
            .is_file()
        {
            return;
        }
        let asset = load_face_part_asset(
            &repo_root.join("extracted/fo4"),
            &source_root,
            exact_legacy_hair_spec(
                0x0010_4C7F,
                &HairRef {
                    plugin: "FalloutNV.esm".to_string(),
                    object_id: "0987D9".to_string(),
                },
                RaceClass::HumanMale,
            )
            .unwrap(),
        )
        .unwrap();

        assert_eq!(asset.shape_ids.len(), 1);
        let shape = asset.nif.get_block(asset.shape_ids[0]).unwrap();
        assert!(matches!(
            shape.get_field("Name"),
            Some(NifValue::String(name)) if name == "Hat"
        ));
    }

    #[test]
    fn easy_pete_source_beard_and_hair_are_valid_skinned_fo4_facegen_parts() {
        use nif_core_native::model::{NifFile, NifValue};

        let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| {
                path.join("extracted/fnv").is_dir() && path.join("extracted/fo4").is_dir()
            })
        else {
            return;
        };
        let source_root = repo_root.join("extracted/fnv");
        let target_root = repo_root.join("extracted/fo4");
        let Some(template_path) = find_target_facegeom_template(&target_root, RaceClass::HumanMale)
        else {
            return;
        };
        let hair = load_face_part_asset(
            &target_root,
            &source_root,
            exact_legacy_hair_spec(
                0x0010_4C7F,
                &HairRef {
                    plugin: "FalloutNV.esm".to_string(),
                    object_id: "0987D9".to_string(),
                },
                RaceClass::HumanMale,
            )
            .unwrap(),
        )
        .unwrap();
        let beard = load_face_part_asset(
            &target_root,
            &source_root,
            target_beard_spec(
                &HairRef {
                    plugin: "FalloutNV.esm".to_string(),
                    object_id: "09F923".to_string(),
                },
                RaceClass::HumanMale,
            )
            .unwrap(),
        )
        .unwrap();
        let variable_parts = vec![Arc::new(hair), Arc::new(beard)];
        let refs = target_head_part_refs(RaceClass::HumanMale, &variable_parts);
        assert!(refs.contains(&("Fallout4.esm".to_string(), "1477D0".to_string())));
        assert!(refs.contains(&("Fallout4.esm".to_string(), "0617BC".to_string())));

        let mut assembled = NifFile::load(template_path).unwrap();
        strip_variable_face_shapes(&mut assembled);
        let face_node = find_face_node(&assembled).unwrap();
        let mut copied = Vec::new();
        for asset in &variable_parts {
            let reference = asset.skin_reference.as_ref().unwrap();
            for source_shape_id in &asset.shape_ids {
                let copied_shape = append_face_part_shape(
                    &mut assembled,
                    &asset.nif,
                    *source_shape_id,
                    face_node,
                    &asset.spec.shape_name,
                )
                .unwrap();
                nif_core_native::skin::rig_facegen_shape_from_reference(
                    &mut assembled,
                    copied_shape,
                    reference,
                )
                .unwrap();
                copied.push((asset.spec.shape_name.clone(), copied_shape));
            }
        }
        nif_core_native::convert_file::finalize_assembled_facegeom_for_fo4(&mut assembled);

        let temp = tempfile::tempdir().unwrap();
        let saved_path = temp.path().join("00104C7F.nif");
        assembled.save(Some(saved_path.clone())).unwrap();
        let assembled = NifFile::load(saved_path).unwrap();

        for (expected_name, shape_id) in copied {
            let shape = assembled.get_block(shape_id).unwrap();
            assert!(matches!(
                shape.get_field("Name"),
                Some(NifValue::String(name)) if name == &expected_name
            ));
            assert_eq!(shape.get_field("Flags"), Some(&NifValue::UInt(14)));
            let skin_id = match shape.get_field("Skin") {
                Some(NifValue::Ref(id)) if *id >= 0 => *id as usize,
                other => panic!("{expected_name} has no FO4 skin: {other:?}"),
            };
            let skin = assembled.get_block(skin_id).unwrap();
            assert_eq!(skin.type_name, "BSSkin::Instance");
            assert!(
                matches!(skin.get_field("Num Bones"), Some(NifValue::UInt(count)) if *count > 0)
            );
            let vertex_desc = shape
                .get_field("Vertex Desc")
                .map(NifValue::as_i64)
                .unwrap();
            assert_ne!(
                (vertex_desc >> 44) & 0x40,
                0,
                "{expected_name} is not skinned"
            );
            let has_vertex_colors = ((vertex_desc >> 44) & 0x20) != 0;
            assert_eq!(
                ((vertex_desc >> 28) & 0xF) * 4,
                if has_vertex_colors { 24 } else { 20 },
                "{expected_name} has an invalid FO4 skin-data offset"
            );
            let vertices = match shape.get_field("Vertex Data") {
                Some(NifValue::Array(vertices)) => vertices,
                other => panic!("{expected_name} has no vertex data: {other:?}"),
            };
            assert!(vertices.iter().all(|vertex| {
                matches!(vertex, NifValue::Struct(fields)
                    if matches!(fields.get("Bone Weights"), Some(NifValue::Array(weights)) if weights.iter().any(|weight| matches!(weight, NifValue::Float(value) if *value > 0.0)))
                    && matches!(fields.get("Bone Indices"), Some(NifValue::Array(indices)) if indices.len() == 4))
            }));
            let shader_id = match shape.get_field("Shader Property") {
                Some(NifValue::Ref(id)) if *id >= 0 => *id as usize,
                other => panic!("{expected_name} has no shader: {other:?}"),
            };
            let shader = assembled.get_block(shader_id).unwrap();
            assert_ne!(
                shader
                    .get_field("Shader Flags 1")
                    .map(NifValue::as_i64)
                    .unwrap()
                    & 0x02,
                0
            );
        }
    }

    #[test]
    fn easy_pete_old_face_tint_preserves_source_detail_instead_of_solid_fallback() {
        let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| {
                path.join("extracted/fnv").is_dir() && path.join("extracted/fo4").is_dir()
            })
        else {
            return;
        };
        let source_root = repo_root.join("extracted/fnv");
        let source = find_source_facetint_dds(&source_root, "FalloutNV.esm", "00104C7F")
            .expect("Easy Pete source FaceTint should be extracted");
        assert!(source.ends_with("falloutnv.esm/00104c7f_0.dds"));

        let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/phase/resources/face");
        let lut = load_uv_lut(&resources.join("fnv_to_fo4_facetint_uv_lut_male.npz")).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("00104C7F.dds");
        let fallback = deterministic_facetint_color("00104C7F");
        write_facetint_dds_with_uv(&output, Some(&source), Some(&lut), fallback).unwrap();

        let source_image = directxtex_native::read_dds_rgba_image(&source).unwrap();
        let output_image = directxtex_native::read_dds_rgba_image(&output).unwrap();
        assert_eq!(output_image.width, lut.2 as u32);
        assert_eq!(output_image.height, lut.1 as u32);

        let distinct_rgb = |rgba: &[u8]| {
            rgba.chunks_exact(4)
                .filter(|pixel| pixel[3] != 0)
                .map(|pixel| [pixel[0], pixel[1], pixel[2]])
                .collect::<std::collections::HashSet<_>>()
                .len()
        };
        assert!(
            distinct_rgb(&source_image.rgba) > 32,
            "Easy Pete source FaceTint has no facial texture detail"
        );
        assert!(
            distinct_rgb(&output_image.rgba) > 32,
            "Easy Pete remapped FaceTint collapsed to a solid fallback"
        );
        assert!(output_image.rgba.chunks_exact(4).any(|pixel| {
            pixel[3] != 0 && [pixel[0], pixel[1], pixel[2], pixel[3]] != fallback
        }));
    }

    #[test]
    fn old_human_profile_selects_headold_assets() {
        let temp = tempfile::tempdir().unwrap();
        let head = temp.path().join("Meshes/Characters/Head");
        std::fs::create_dir_all(&head).unwrap();
        std::fs::write(head.join("headhuman.nif"), []).unwrap();
        std::fs::write(head.join("headhuman.egm"), []).unwrap();
        std::fs::write(head.join("headold.nif"), []).unwrap();
        std::fs::write(head.join("headold.egm"), []).unwrap();

        assert_eq!(
            find_source_head_nif(temp.path(), RaceClass::HumanMale, SourceHeadProfile::Old)
                .unwrap(),
            head.join("headold.nif")
        );
        assert_eq!(
            find_source_head_egm(temp.path(), RaceClass::HumanMale, SourceHeadProfile::Old)
                .unwrap(),
            head.join("headold.egm")
        );
    }

    #[test]
    fn old_human_profile_loads_against_fo4_correspondence_when_assets_exist() {
        let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| {
                path.join("extracted/fnv").is_dir() && path.join("extracted/fo4").is_dir()
            })
        else {
            return;
        };
        let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/phase/resources/face");
        let face_resources = FaceResourcePaths {
            human_male_correspondence: Some(resources.join("fnv_to_fo4_correspondence_male.npz")),
            human_male_uv_lut: Some(resources.join("fnv_to_fo4_facetint_uv_lut_male.npz")),
            ..Default::default()
        };

        load_face_bake_profile(
            RaceClass::HumanMale,
            SourceHeadProfile::Old,
            &repo_root.join("extracted/fnv"),
            &repo_root.join("extracted/fo4"),
            &TargetFaceAssets::default(),
            &face_resources,
        )
        .unwrap();
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

    #[test]
    fn body_weight_serializes_as_fo4_morph_triple() {
        let bytes = fo4_body_morph_bytes(0.5);

        assert_eq!(bytes.len(), 12);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 0.0);
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0.0);
        assert_eq!(f32::from_le_bytes(bytes[8..12].try_into().unwrap()), 0.5);
    }

    #[test]
    fn skyrim_weight_maps_zero_to_thin_and_hundred_to_fat() {
        let nam7 = crate::ids::SubrecordSig::from_str("NAM7").unwrap();
        let record = |weight: f32| {
            let mut record = Record::new(
                SigCode::from_str("NPC_").unwrap(),
                FormKey {
                    local: 0x800,
                    plugin: StringInterner::new().intern("Skyrim.esm"),
                },
            );
            record.fields.push(FieldEntry {
                sig: nam7,
                value: FieldValue::Float(weight),
            });
            record
        };

        assert_eq!(extract_skyrim_body_weight(&record(0.0)), -1.0);
        assert_eq!(extract_skyrim_body_weight(&record(50.0)), 0.0);
        assert_eq!(extract_skyrim_body_weight(&record(100.0)), 1.0);
    }

    #[test]
    fn normalized_nearest_correspondence_transfers_source_deformation() {
        let source_neutral = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [0.0, 0.0, 6.0],
        ];
        let target_neutral = vec![
            [10.0, 10.0, 10.0],
            [20.0, 10.0, 10.0],
            [10.0, 30.0, 10.0],
            [10.0, 10.0, 40.0],
        ];
        let source_deformed = source_neutral
            .iter()
            .map(|vertex| [vertex[0] + 0.25, vertex[1] - 0.5, vertex[2] + 1.0])
            .collect::<Vec<_>>();
        let correspondence =
            Correspondence::nearest_normalized(&source_neutral, &target_neutral).unwrap();
        let deformation = correspondence
            .interpolate_deformation(&source_neutral, &source_deformed)
            .unwrap();

        for vertex in deformation {
            assert!((vertex[0] - 0.25).abs() < 1.0e-5);
            assert!((vertex[1] + 0.5).abs() < 1.0e-5);
            assert!((vertex[2] - 1.0).abs() < 1.0e-5);
        }
        assert_eq!(
            deformation_scale(&source_neutral, &target_neutral).unwrap(),
            [5.0, 5.0, 5.0]
        );
    }

    #[test]
    #[ignore = "requires extracted Skyrim SE and Fallout 4 assets"]
    fn real_skyrim_facegeom_transfers_to_fo4() {
        let repo_root = std::env::var_os("MODKIT_REPO_ROOT")
            .map(PathBuf::from)
            .expect("MODKIT_REPO_ROOT must point at the repository");
        let source_extracted = repo_root.join("extracted/skyrimse");
        let target_extracted = repo_root.join("extracted/fo4");
        let source_facegeom = source_extracted
            .join("Meshes/Actors/Character/FaceGenData/FaceGeom/Skyrim.esm/000A2C8E.NIF");
        let target_assets = TargetFaceAssets::from_params(&serde_json::json!({}));
        let profile = load_skyrim_face_bake_profile(
            RaceClass::HumanFemale,
            &source_extracted,
            &target_extracted,
            &target_assets,
        )
        .unwrap();
        let output_root = std::env::temp_dir().join("bacup-skyrim-face-smoke");
        let named_bones = load_named_bones(EMBEDDED_NAMED_BONES_YAML).unwrap();
        let hair_table = HairTable::load(EMBEDDED_HAIR_LOOKUP_YAML).unwrap();
        let hair = hair_table
            .lookup(None, RaceClass::HumanFemale)
            .and_then(|hair| target_hair_spec(&hair))
            .map(|spec| {
                Arc::new(load_face_part_asset(&target_extracted, &source_extracted, spec).unwrap())
            })
            .into_iter()
            .collect::<Vec<_>>();

        let result = attempt_skyrim_face_bake_cached(
            "000A2C8E",
            &source_facegeom,
            &output_root,
            "SkyrimFaceSmoke.esp",
            &profile,
            &named_bones,
            &hair,
            SkyrimHumanoidFaceKind::Human,
            None,
        )
        .unwrap();
        let output = output_root.join("data").join(
            result
                .facegeom_relpath
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        assert!(output.is_file());
        nif_core_native::model::NifFile::load(output).unwrap();
    }

    fn run_preserved_skyrim_facegeom_smoke(
        source_relative: &str,
        race_class: RaceClass,
        source_face_kind: SkyrimHumanoidFaceKind,
        expected_head_vertices: usize,
        preserved_shape_fragment: &str,
    ) {
        use nif_core_native::model::NifValue;

        let repo_root = std::env::var_os("MODKIT_REPO_ROOT")
            .map(PathBuf::from)
            .expect("MODKIT_REPO_ROOT must point at the repository");
        let source_extracted = repo_root.join("extracted/skyrimse");
        let target_extracted = repo_root.join("extracted/fo4");
        let source_facegeom = source_extracted.join(source_relative);
        let target_assets = TargetFaceAssets::from_params(&serde_json::json!({}));
        let profile = load_skyrim_face_bake_profile(
            race_class,
            &source_extracted,
            &target_extracted,
            &target_assets,
        )
        .unwrap();
        let output_root = tempfile::tempdir().unwrap();
        let translation_maps_dir =
            repo_root.join("bacup/py_bacup_lib/native/conversion/src/embedded/translation_maps");

        let result = attempt_skyrim_face_bake_cached(
            "00000001",
            &source_facegeom,
            output_root.path(),
            "SkyrimBeastFaceSmoke.esp",
            &profile,
            &[],
            &[],
            source_face_kind,
            Some(&translation_maps_dir),
        )
        .unwrap();
        let output = output_root.path().join("data").join(
            result
                .facegeom_relpath
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let converted = nif_core_native::model::NifFile::load(output).unwrap();
        assert_eq!(converted.header.bs_version, 130);
        assert!(!converted.blocks.iter().any(|block| {
            matches!(
                block.type_name.as_str(),
                "BSDynamicTriShape"
                    | "NiSkinInstance"
                    | "BSDismemberSkinInstance"
                    | "NiSkinData"
                    | "NiSkinPartition"
            )
        }));
        let geometry = converted
            .blocks
            .iter()
            .filter(|block| block.type_name == "BSSubIndexTriShape")
            .collect::<Vec<_>>();
        assert!(geometry.len() >= 3);
        assert!(geometry.iter().all(|block| {
            matches!(block.get_field("Skin"), Some(NifValue::Ref(id)) if *id >= 0)
                && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(vertices)) if !vertices.is_empty())
        }));
        let expected_head_name = race_class.primary_head_shape_name().unwrap();
        let head = geometry
            .iter()
            .find(|block| {
                matches!(block.get_field("Name"), Some(NifValue::String(name)) if name == expected_head_name)
            })
            .expect("preserved Skyrim head geometry");
        assert_eq!(
            match head.get_field("Vertex Data") {
                Some(NifValue::Array(vertices)) => vertices.len(),
                _ => 0,
            },
            expected_head_vertices
        );
        assert!(geometry.iter().any(|block| {
            matches!(block.get_field("Name"), Some(NifValue::String(name)) if name.contains(preserved_shape_fragment))
        }));
        assert!(
            output_root
                .path()
                .join("data")
                .join(
                    result
                        .facetint_relpath
                        .replace('/', std::path::MAIN_SEPARATOR_STR)
                )
                .is_file()
        );
    }

    #[test]
    #[ignore = "requires extracted Skyrim SE and Fallout 4 assets"]
    fn real_argonian_facegeom_preserves_species_topology_in_fo4() {
        run_preserved_skyrim_facegeom_smoke(
            "Meshes/Actors/Character/FaceGenData/FaceGeom/Skyrim.esm/00103512.nif",
            RaceClass::HumanMale,
            SkyrimHumanoidFaceKind::Argonian,
            1219,
            "MaleEyesHumanBlue",
        );
    }

    #[test]
    #[ignore = "requires extracted Skyrim SE and Fallout 4 assets"]
    fn real_female_argonian_facegeom_preserves_species_topology_in_fo4() {
        run_preserved_skyrim_facegeom_smoke(
            "Meshes/Actors/Character/FaceGenData/FaceGeom/Skyrim.esm/00103511.nif",
            RaceClass::HumanFemale,
            SkyrimHumanoidFaceKind::Argonian,
            1219,
            "FemaleEyesHumanHazelGreen",
        );
    }

    #[test]
    #[ignore = "requires extracted Skyrim SE and Fallout 4 assets"]
    fn real_khajiit_facegeom_preserves_fur_and_hair_in_fo4() {
        run_preserved_skyrim_facegeom_smoke(
            "Meshes/Actors/Character/FaceGenData/FaceGeom/Skyrim.esm/00105553.nif",
            RaceClass::HumanMale,
            SkyrimHumanoidFaceKind::Khajiit,
            1356,
            "HairKhajiitMale09",
        );
    }

    #[test]
    #[ignore = "requires extracted Skyrim SE and Fallout 4 assets"]
    fn real_female_khajiit_facegeom_preserves_fur_and_hair_in_fo4() {
        run_preserved_skyrim_facegeom_smoke(
            "Meshes/Actors/Character/FaceGenData/FaceGeom/Skyrim.esm/00103516.nif",
            RaceClass::HumanFemale,
            SkyrimHumanoidFaceKind::Khajiit,
            1342,
            "HairKhajiitFemale03",
        );
    }

    #[test]
    #[ignore = "requires extracted Skyrim SE and Fallout 4 assets"]
    fn real_dremora_facegeom_preserves_horns_in_fo4() {
        run_preserved_skyrim_facegeom_smoke(
            "Meshes/Actors/Character/FaceGenData/FaceGeom/Dragonborn.esm/0001EEB1.nif",
            RaceClass::HumanMale,
            SkyrimHumanoidFaceKind::Dremora,
            898,
            "HairHornsMaleDremora02",
        );
    }

    #[test]
    fn mapped_human_race_uses_npc_sex() {
        let interner = StringInterner::new();
        let human = FormKey {
            local: FO4_HUMAN_RACE_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert_eq!(
            classify_mapped_race(Some(human), false),
            RaceClass::HumanMale
        );
        assert_eq!(
            classify_mapped_race(Some(human), true),
            RaceClass::HumanFemale
        );
    }

    #[test]
    fn mapped_ghoul_race_uses_npc_sex() {
        let interner = StringInterner::new();
        let ghoul = FormKey {
            local: FO4_GHOUL_RACE_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert_eq!(
            classify_mapped_race(Some(ghoul), false),
            RaceClass::GhoulMale
        );
        assert_eq!(
            classify_mapped_race(Some(ghoul), true),
            RaceClass::GhoulFemale
        );
        assert_eq!(
            RaceClass::GhoulMale.mandatory_head_parts(),
            FO4_GHOUL_MALE_HEAD_PARTS
        );
        assert_eq!(
            RaceClass::GhoulFemale.mandatory_head_parts(),
            FO4_GHOUL_FEMALE_HEAD_PARTS
        );
    }

    #[test]
    fn mapped_child_race_uses_npc_sex() {
        let interner = StringInterner::new();
        let child = FormKey {
            local: FO4_HUMAN_CHILD_RACE_LOCAL,
            plugin: interner.intern("Fallout4.esm"),
        };

        assert_eq!(
            classify_mapped_race(Some(child), false),
            RaceClass::ChildMale
        );
        assert_eq!(
            classify_mapped_race(Some(child), true),
            RaceClass::ChildFemale
        );
        assert_eq!(
            RaceClass::ChildMale.mandatory_head_parts(),
            FO4_CHILD_MALE_HEAD_PARTS
        );
        assert_eq!(
            RaceClass::ChildFemale.mandatory_head_parts(),
            FO4_CHILD_FEMALE_HEAD_PARTS
        );
    }

    #[test]
    fn every_supported_race_has_a_complete_facegen_template_profile() {
        for race_class in [
            RaceClass::HumanMale,
            RaceClass::HumanFemale,
            RaceClass::GhoulMale,
            RaceClass::GhoulFemale,
            RaceClass::ChildMale,
            RaceClass::ChildFemale,
        ] {
            assert!(race_class.template_facegeom_form_id().is_some());
            assert!(race_class.primary_head_shape_name().is_some());
            assert!(!race_class.mandatory_head_parts().is_empty());
            assert!(race_class.template_hair_color().is_some());
        }
    }

    #[test]
    fn baked_face_fields_replace_stale_head_parts_and_hair_color() {
        use crate::record::RecordFlags;

        let interner = StringInterner::new();
        let mut record = Record {
            sig: SigCode::from_str("NPC_").unwrap(),
            form_key: FormKey {
                plugin: interner.intern("FalloutNV.esm"),
                local: 0x000A6E,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: crate::ids::SubrecordSig::from_str("PNAM").unwrap(),
                    value: FieldValue::FormKey(FormKey {
                        plugin: interner.intern("FalloutNV.esm"),
                        local: 0x066FD0,
                    }),
                },
                FieldEntry {
                    sig: crate::ids::SubrecordSig::from_str("HCLF").unwrap(),
                    value: FieldValue::FormKey(FormKey {
                        plugin: interner.intern("FalloutNV.esm"),
                        local: 0x000001,
                    }),
                },
            ],
            warnings: smallvec::SmallVec::new(),
        };
        let mut pnam_refs = FO4_HUMAN_MALE_HEAD_PARTS
            .iter()
            .map(|local| ("Fallout4.esm".to_string(), format!("{local:06X}")))
            .collect::<Vec<_>>();
        pnam_refs.push(("Fallout4.esm".to_string(), "094D04".to_string()));
        pnam_refs.push(("Fallout4.esm".to_string(), "029671".to_string()));

        replace_face_fields_in_record(
            &mut record,
            &pnam_refs,
            &BoneOffsets::new(),
            0.0,
            None,
            RaceClass::HumanMale,
            &[],
            &interner,
        );

        let head_parts = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "PNAM")
            .collect::<Vec<_>>();
        assert_eq!(head_parts.len(), FO4_HUMAN_MALE_HEAD_PARTS.len() + 2);
        assert!(head_parts.iter().all(|entry| {
            matches!(&entry.value, FieldValue::FormKey(key) if interner.resolve(key.plugin) == Some("Fallout4.esm"))
        }));
        let hair_colors = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "HCLF")
            .collect::<Vec<_>>();
        assert_eq!(hair_colors.len(), 1);
        assert!(matches!(
            &hair_colors[0].value,
            FieldValue::FormKey(key)
                if key.local == RaceClass::HumanMale.template_hair_color().unwrap()
                    && interner.resolve(key.plugin) == Some("Fallout4.esm")
        ));
    }

    #[test]
    fn exact_fnv_slice_npcs_reopen_with_real_outfits_and_fo4_weight() {
        use crate::schema::AuthoringSchema;
        use crate::target_write::add_record_native;
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_add_master_no_py, plugin_handle_close_native, plugin_handle_load_no_py,
            plugin_handle_new_native, plugin_handle_read_authoring_record_value_json,
            plugin_handle_save_preserving_identity_no_py,
        };

        let interner = StringInterner::new();
        let output_plugin = "FalloutNV.esm";
        let base_master = interner.intern("Fallout4.esm");
        let target_handle = plugin_handle_new_native(output_plugin, Some("fo4")).unwrap();
        plugin_handle_add_master_no_py(target_handle, "Fallout4.esm", None).unwrap();
        let schema = AuthoringSchema::for_game("fo4").unwrap();

        for npc_local in [0x123193, 0x1300F0] {
            let mut record = Record::new(
                SigCode::from_str("NPC_").unwrap(),
                FormKey {
                    local: npc_local,
                    plugin: interner.intern(output_plugin),
                },
            );
            record.fields.extend([
                FieldEntry {
                    sig: crate::ids::SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(interner.intern(&format!("Npc{npc_local:06X}"))),
                },
                FieldEntry {
                    sig: crate::ids::SubrecordSig::from_str("DOFT").unwrap(),
                    value: FieldValue::FormKey(FormKey {
                        plugin: base_master,
                        local: 0x1C269A,
                    }),
                },
                FieldEntry {
                    sig: crate::ids::SubrecordSig::from_str("SOFT").unwrap(),
                    value: FieldValue::FormKey(FormKey {
                        plugin: base_master,
                        local: 0x1C269A,
                    }),
                },
            ]);

            replace_face_fields_in_record(
                &mut record,
                &[],
                &BoneOffsets::new(),
                0.5,
                None,
                RaceClass::HumanMale,
                &[],
                &interner,
            );
            assert!(
                record
                    .fields
                    .iter()
                    .any(|field| field.sig.as_str() == "DOFT")
            );
            assert!(
                record
                    .fields
                    .iter()
                    .any(|field| field.sig.as_str() == "SOFT")
            );
            assert!(
                record
                    .fields
                    .iter()
                    .any(|field| field.sig.as_str() == "MWGT")
            );
            assert!(
                !record
                    .fields
                    .iter()
                    .any(|field| field.sig.as_str() == "NAM7")
            );
            add_record_native(target_handle, record, &schema, &interner).unwrap();
        }

        let temp = tempfile::tempdir().unwrap();
        let saved_path = temp.path().join(output_plugin);
        plugin_handle_save_preserving_identity_no_py(target_handle, saved_path.to_str().unwrap())
            .unwrap();
        let reopened =
            plugin_handle_load_no_py(saved_path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();

        for npc_local in [0x123193, 0x1300F0] {
            let record = plugin_handle_read_authoring_record_value_json(
                reopened,
                &format!("{output_plugin}:{npc_local:06X}"),
            )
            .unwrap()
            .unwrap_or_else(|| panic!("reopened NPC {npc_local:06X}"));
            let fields = record["fields"].as_array().unwrap();
            for outfit in ["DefaultOutfit", "SleepingOutfit"] {
                let reference = fields
                    .iter()
                    .find_map(|field| field.get(outfit))
                    .and_then(|outfit| outfit.get("reference"))
                    .unwrap_or_else(|| panic!("NPC {npc_local:06X} missing {outfit}"));
                assert_eq!(reference["plugin"], "Fallout4.esm");
                assert_eq!(reference["object_id"], "1C269A");
            }
            assert!(fields.iter().any(|field| field.get("Weight").is_some()));
            assert!(
                !fields.iter().any(|field| field.get("HeightMax").is_some()),
                "NPC {npc_local:06X} must not carry FNV NAM4 as FO4 HeightMax"
            );
        }

        plugin_handle_close_native(reopened);
        plugin_handle_close_native(target_handle);
    }

    #[test]
    fn facegeom_replaces_donor_variable_parts_with_selected_npc_parts_when_assets_exist() {
        use nif_core_native::model::{NifFile, NifValue};

        let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| path.join("extracted/fo4").is_dir())
        else {
            return;
        };
        let target = repo_root.join("extracted/fo4");
        let Some(base_head) = find_target_base_head_nif(&target, RaceClass::HumanMale) else {
            return;
        };
        let Some(template) = find_target_facegeom_template(&target, RaceClass::HumanMale) else {
            return;
        };
        let neutral = load_fo4_neutral_vertices(&base_head).unwrap();
        let deformation = vec![[0.0; 3]; neutral.len()];
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("face.nif");

        let mut assembled = NifFile::load(template).unwrap();
        strip_variable_face_shapes(&mut assembled);
        let face_node = find_face_node(&assembled).unwrap();
        let primary_shape = assembled
            .blocks
            .iter()
            .find(|block| {
                matches!(block.get_field("Name"), Some(NifValue::String(name)) if name == "MaleHeadHuman")
            })
            .unwrap()
            .block_id;
        let hair = load_face_part_asset(
            &target,
            &target,
            target_hair_spec(&HairRef {
                plugin: "Fallout4.esm".to_string(),
                object_id: "094D04".to_string(),
            })
            .unwrap(),
        )
        .unwrap();
        let beard = load_face_part_asset(
            &target,
            &target,
            target_beard_spec(
                &HairRef {
                    plugin: "FalloutNV.esm".to_string(),
                    object_id: "000013".to_string(),
                },
                RaceClass::HumanMale,
            )
            .unwrap(),
        )
        .unwrap();
        for asset in [&hair, &beard] {
            for &shape_id in &asset.shape_ids {
                append_face_part_shape(
                    &mut assembled,
                    &asset.nif,
                    shape_id,
                    face_node,
                    &asset.spec.shape_name,
                )
                .unwrap();
            }
        }
        write_facegeom_nif_from_template(
            &assembled,
            primary_shape,
            &neutral,
            &deformation,
            &output,
        )
        .unwrap();

        let nif = NifFile::load(output).unwrap();
        let names = nif
            .blocks
            .iter()
            .filter_map(|block| match block.get_field("Name") {
                Some(NifValue::String(name)) => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for expected in [
            "MaleHeadHuman",
            "MaleEyesHumanWet",
            "MaleEyesHumanLashes",
            "HairMale19",
            "Beard20",
            "MaleMouthHumanoidDefault",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
        assert!(!names.contains(&"HairMale20"), "donor hair survived");
        assert!(!names.contains(&"Beard02"), "donor beard survived");
    }

    #[test]
    fn cached_profile_supports_parallel_asset_bakes_when_assets_exist() {
        let Some(repo_root) = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| path.join("extracted/fo4").is_dir())
        else {
            return;
        };
        let source = repo_root.join("extracted/fnv");
        let target = repo_root.join("extracted/fo4");
        if !source.is_dir() {
            return;
        }

        let resources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/phase/resources/face");
        let face_resources = FaceResourcePaths {
            human_male_correspondence: Some(resources.join("fnv_to_fo4_correspondence_male.npz")),
            human_male_uv_lut: Some(resources.join("fnv_to_fo4_facetint_uv_lut_male.npz")),
            ..Default::default()
        };
        let profile = load_face_bake_profile(
            RaceClass::HumanMale,
            SourceHeadProfile::Standard,
            &source,
            &target,
            &TargetFaceAssets::default(),
            &face_resources,
        )
        .unwrap();
        let named_bones = load_named_bones(EMBEDDED_NAMED_BONES_YAML).unwrap();
        let output = tempfile::tempdir().unwrap();
        let coefficients = vec![0.0; profile.egm_num_diffs];
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .unwrap();

        let results = pool.install(|| {
            (1..=8u32)
                .into_par_iter()
                .map(|form_id| {
                    attempt_face_bake_cached(
                        &format!("{form_id:08X}"),
                        &format!("{form_id:08X}"),
                        &coefficients,
                        &source,
                        output.path(),
                        "ParallelFaces.esp",
                        "FalloutNV.esm",
                        &profile,
                        &named_bones,
                        &[],
                    )
                })
                .collect::<Vec<_>>()
        });

        assert!(results.iter().all(Result::is_ok));
        assert_eq!(
            std::fs::read_dir(
                output
                    .path()
                    .join("data/Meshes/Actors/Character/FaceGenData/FaceGeom/ParallelFaces.esp")
            )
            .unwrap()
            .count(),
            8
        );
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
