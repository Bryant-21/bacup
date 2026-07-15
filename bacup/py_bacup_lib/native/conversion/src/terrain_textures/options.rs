use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordOutputMode {
    AuthoringDir,
    TargetHandle,
}

impl Default for RecordOutputMode {
    fn default() -> Self {
        Self::AuthoringDir
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TargetRecordReuseRef {
    pub editor_id: String,
    pub signature: String,
    pub form_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Options {
    #[serde(default)]
    pub source_game: String,
    #[serde(default)]
    pub target_game: String,
    #[serde(default)]
    pub source_plugin_path: String,
    #[serde(skip)]
    pub source_handle_id: u64,
    pub fo76_data_dir: String,
    #[serde(default)]
    pub source_extracted_dir: String,
    pub btd_path: String,
    pub output_authoring_dir: String,
    pub plugin_name: String,
    pub worldspace_editor_id: String,
    pub source_min_x: i32,
    pub source_min_y: i32,
    pub source_max_x: i32,
    pub source_max_y: i32,
    #[serde(default)]
    pub world_form_id: u32,
    #[serde(default)]
    pub first_cell_form_id: u32,
    #[serde(default = "default_sample4")]
    pub resample_mode: String,
    #[serde(default)]
    pub debug_output_dir: String,
    #[serde(default = "default_true")]
    pub emit_textures: bool,
    #[serde(default = "default_true")]
    pub write_materials: bool,
    #[serde(default)]
    pub export_heightmap: bool,
    #[serde(default)]
    pub debug_flat_land: bool,
    #[serde(default = "default_true")]
    pub preserve_source_ids: bool,
    #[serde(default)]
    pub reserved_object_ids: Vec<u32>,
    #[serde(default)]
    pub source_worldspace_authoring_dir: String,
    #[serde(default)]
    pub source_worldspace_terrain_ids_json: String,
    #[serde(default)]
    pub heightmap_output_path: String,
    #[serde(default)]
    pub btd4_output_path: String,
    #[serde(default)]
    pub water_manifest_path: String,
    #[serde(default = "default_true")]
    pub populate_grass_assets: bool,
    #[serde(default = "default_true")]
    pub convert_grass_assets: bool,
    #[serde(skip)]
    pub record_output_mode: RecordOutputMode,
    #[serde(skip)]
    pub target_handle_id: Option<u64>,
    #[serde(default)]
    pub target_cell_editor_ids: Vec<String>,
    #[serde(default)]
    pub target_record_reuse: Vec<TargetRecordReuseRef>,
    /// Retained for compatibility; texture conversion workers are now owned by
    /// `convert_textures_v2`.
    #[serde(default)]
    pub conversion_workers: Option<usize>,
    /// When true, LAND BTXT/ATXT layers resolve the PLAIN base LTEX instead of the
    /// `{base}_GC_{gcvr}` ground-cover composite (whose incomplete TXST renders the
    /// max-density quad black in FO4). Ground cover still flows to the `.btd4` GCVR
    /// channel for the native scatter. Default false preserves baked-GC behavior.
    #[serde(default)]
    pub land_skip_ground_cover_variants: bool,
    /// Retained for compatibility; terrain no longer encodes DDS files.
    #[serde(default)]
    pub reuse_existing_textures: bool,
}

fn default_sample4() -> String {
    "sample4".to_owned()
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub cells_written: u32,
    pub textures_resolved: u32,
    pub grass_resolved: u32,
    pub ground_cover_layers: u32,
    pub no_ground_cover_layers: u32,
    pub grass_ltex_variants: u32,
    pub gcvr_records_resolved: u32,
    pub gcvr_records_missing: u32,
    pub grass_position_range_normalized: u32,
    pub materials_written: u32,
    pub grass_nifs_collected: u32,
    pub grass_textures_collected: u32,
    pub grass_materials_collected: u32,
    pub grass_nifs_converted: u32,
    pub grass_textures_converted: u32,
    pub grass_materials_converted: u32,
    pub dropped_texture_layers: u32,
    pub records_imported: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btd4_output_path: Option<String>,
    #[serde(default)]
    pub layers_recovered: u32,
    pub warnings: Vec<String>,
    pub timings: Vec<TimingEntry>,
    #[serde(skip)]
    pub emitted_records: Vec<terrain_native::authoring_emit::AuthoringRecordPayload>,
    /// Explicit LAND bundles queued for `convert_textures_v2`.
    #[serde(skip)]
    pub terrain_texture_jobs: Vec<crate::terrain_textures::manifest::TerrainTextureJob>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimingEntry {
    pub name: String,
    pub elapsed_seconds: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_options_json() -> serde_json::Value {
        json!({
            "source_plugin_path": "X:/Fallout76/Data/SeventySix.esm",
            "fo76_data_dir": "X:/Fallout76/Data",
            "btd_path": "X:/Fallout76/Data/Terrain/Appalachia.btd",
            "output_authoring_dir": "X:/mods/B21_Test/yaml",
            "plugin_name": "B21_Test.esp",
            "worldspace_editor_id": "B21_Test",
            "source_min_x": 0,
            "source_min_y": 0,
            "source_max_x": 0,
            "source_max_y": 0
        })
    }

    #[test]
    fn record_output_mode_defaults_to_authoring_dir() {
        let opts: Options = serde_json::from_value(minimal_options_json()).unwrap();

        assert_eq!(opts.record_output_mode, RecordOutputMode::AuthoringDir);
    }

    #[test]
    fn record_output_mode_is_not_a_public_option() {
        let mut value = minimal_options_json();
        value["record_output_mode"] = json!("target_handle");

        let opts: Options = serde_json::from_value(value).unwrap();

        assert_eq!(opts.record_output_mode, RecordOutputMode::AuthoringDir);
    }

    #[test]
    fn populate_grass_assets_defaults_to_true() {
        let opts: Options = serde_json::from_value(minimal_options_json()).unwrap();

        assert!(opts.populate_grass_assets);
    }

    #[test]
    fn populate_grass_assets_accepts_false() {
        let mut value = minimal_options_json();
        value["populate_grass_assets"] = json!(false);

        let opts: Options = serde_json::from_value(value).unwrap();

        assert!(!opts.populate_grass_assets);
    }

    #[test]
    fn convert_grass_assets_defaults_to_true() {
        let opts: Options = serde_json::from_value(minimal_options_json()).unwrap();

        assert!(opts.convert_grass_assets);
    }

    #[test]
    fn convert_grass_assets_accepts_false() {
        let mut value = minimal_options_json();
        value["convert_grass_assets"] = json!(false);

        let opts: Options = serde_json::from_value(value).unwrap();

        assert!(!opts.convert_grass_assets);
    }

    #[test]
    fn write_materials_defaults_to_true() {
        let opts: Options = serde_json::from_value(minimal_options_json()).unwrap();

        assert!(opts.write_materials);
    }

    #[test]
    fn write_materials_accepts_false() {
        let mut value = minimal_options_json();
        value["write_materials"] = json!(false);

        let opts: Options = serde_json::from_value(value).unwrap();

        assert!(!opts.write_materials);
    }
}
