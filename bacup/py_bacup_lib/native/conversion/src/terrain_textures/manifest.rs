use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Serialize)]
pub struct TerrainTextureJob {
    pub diffuse_path: String,
    pub normal_path: String,
    pub reflectivity_path: String,
    pub lighting_path: String,
    pub output_prefix: String,
}

/// Serializable mirror of `terrain_native::texture_bridge::TextureBundle`.
/// Field names must match the bridge's `Deserialize` field names exactly.
/// We add `assets` on `GrassEntry` (bridge ignores unknown fields).
#[derive(Debug, Clone, Default, Serialize)]
pub struct TextureManifest {
    pub textures: Vec<TextureBundle>,
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
