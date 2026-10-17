use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct TerrainTextureJob {
    pub diffuse_path: String,
    pub normal_path: String,
    pub reflectivity_path: String,
    pub lighting_path: String,
    pub output_prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_source_prefix: Option<String>,
}

/// Serializable mirror of `terrain_native::texture_bridge::TextureBundle`.
/// Field names must match the bridge's `Deserialize` field names exactly.
/// We add `assets` on `GrassEntry` (bridge ignores unknown fields).
#[derive(Debug, Clone, Default, Serialize)]
pub struct TextureManifest {
    pub textures: Vec<TextureBundle>,
    /// FO76 BTD sources with a `.btd4`: every GCVR in the BTD's table, for the v5 grass points.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ground_covers: Vec<GroundCoverManifest>,
}

/// One FO76 GCVR as `BGSGroundCover::Load` keeps it (mirrors the bridge's `GroundCoverManifest`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GroundCoverManifest {
    pub form_key: String,
    pub editor_id: String,
    /// Texture-weight threshold, unclamped; 0.286 when the record has no YNAM.
    pub ynam: f32,
    pub ltex_form_keys: Vec<String>,
    /// GNAMs followed by a DNAM, in file order; a repeated GNAM keeps its first entry.
    pub grasses: Vec<GroundCoverGrass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroundCoverGrass {
    pub form_key: String,
    /// Density percent, or 0xFFFF for the GRAS DNAM density byte.
    pub dnam: u16,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TextureBundle {
    pub source_ltex_form_key: String,
    pub source_ltex_editor_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_gcvr_form_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_gcvr_editor_id: Option<String>,
    pub source_txst_form_key: String,
    pub source_txst_editor_id: String,
    pub diffuse_path: String,
    pub normal_path: String,
    pub reflectivity_path: String,
    pub lighting_path: String,
    pub output_prefix: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_source_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_material_rows: Option<[[f32; 4]; 3]>,
    /// Delivered for the .btd4 raw source bank only; never emitted as LTEX/TXST/GRAS records.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub raw_bank_only: bool,
    pub output_material_path: Option<String>,
    /// Original BGSM relative path from the LTEX record (e.g. "Landscape\\Ground\\ForestGrass01.BGSM").
    /// Empty for TXST-branch bundles.
    #[serde(default)]
    pub source_material_path: String,
    /// Absolute path to the extracted FO76 BGSM file on disk.
    /// Empty for TXST-branch bundles.
    #[serde(default)]
    pub source_material_file: String,
    pub material_type_object_id: Option<String>,
    pub havok_friction: Option<u8>,
    pub havok_restitution: Option<u8>,
    #[serde(default)]
    pub grass: Vec<GrassEntry>,
}

impl From<&TextureBundle> for TerrainTextureJob {
    fn from(bundle: &TextureBundle) -> Self {
        Self {
            diffuse_path: bundle.diffuse_path.clone(),
            normal_path: bundle.normal_path.clone(),
            reflectivity_path: bundle.reflectivity_path.clone(),
            lighting_path: bundle.lighting_path.clone(),
            output_prefix: bundle.output_prefix.clone(),
            raw_source_prefix: bundle.raw_source_prefix.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GrassEntry {
    pub source_form_key: String,
    pub source_editor_id: String,
    #[serde(default)]
    pub object_bounds: ObjectBounds,
    pub model_file_name: String,
    pub model_information: String,
    pub density: u8,
    pub max_slope: u8,
    pub position_range: f32,
    pub height_range: f32,
    pub color_range: f32,
    pub wave_period: f32,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub position_range_normalized: bool,
    /// Source GCVR records that place this grass. The `.btd4` GCVR channel maps each
    /// GCVR mask sample to its grasses through this list; empty for LTEX-direct grass.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_gcvr_form_keys: Vec<String>,
    /// Target-game GRAS with the same EditorID (`0BA520:Fallout4.esm`). The terrain import
    /// drops the emitted GRAS for it, so the `.btd4` sidecar must reference this record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_form_key: Option<String>,
    /// FO76 sources only: the raw GRAS DNAM, which places the `.btd4` v5 grass points.
    /// The emitted GRAS record keeps its own converted density and ranges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fo76_dnam: Option<terrain_native::fo76_grass::GrassDnam>,
    /// Populated by grass-asset conversion; ignored by terrain_native bridge.
    #[serde(default)]
    pub assets: Vec<GrassAsset>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ObjectBounds {
    pub object_bounds_x1: i16,
    pub object_bounds_y1: i16,
    pub object_bounds_z1: i16,
    pub object_bounds_x2: i16,
    pub object_bounds_y2: i16,
    pub object_bounds_z2: i16,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GrassAsset {
    pub asset_type: String,
    pub source_path: String,
    pub resolved_path: String,
}
