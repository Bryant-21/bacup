//! conversion_native — Bethesda cross-game record/asset conversion.
//!
//! See `CLAUDE.md` in this crate for the phase contract.

use pyo3::prelude::*;
use pyo3::types::PyModule;

pub mod anim_namespace;
mod asset_collection;
pub mod collision_donor;
pub mod drop_trace;
pub mod embedded;
pub mod errors;
pub mod filesystem;
pub mod fixups;
pub mod fnv_legacy_scripting;
pub mod fo76_behaviors;
pub mod fo76_navmesh;
pub mod fo76_rdot;
pub mod formkey_mapper;
pub mod full_plugin;
pub mod ids;
mod infestations;
mod legacy_fallout_navmesh;
pub mod legacy_pack_preflight;
pub mod material_source_overrides;
mod map_markers;
mod image_resize;
pub mod materialized_npc_facegen;
pub mod merge_sources;
pub mod modt_compute;
pub mod modt_manifest;
pub mod native_nifs;
pub mod native_overwrites;
mod native_starfield;
pub mod phase;
pub mod pipeline;
pub mod pipboy_quest_catalog;
pub mod plugin_catalog;
pub mod plugin_header;
pub mod projected_placed;
pub mod python_api;
pub mod quest_runtime;
pub mod quest_runtime_inventory;
pub mod record;
pub mod relocation;
pub mod run;
pub mod schema;
pub mod script_conversion;
pub mod script_headers;
pub mod script_patches;
pub mod script_references;
pub mod session;
pub mod sinks;
pub mod skyrim_navmesh;
pub mod skyrimse_fo4_runtime;
pub mod source_inventory;
pub mod source_read;
pub mod source_rig;
mod starfield_armor_closure;
pub(crate) mod starfield_navmesh;
pub mod starfield_worldspace;
pub mod store2;
pub mod struct_codec;
pub mod struct_relayout;
pub mod sym;
pub mod target_assets;
pub(crate) mod target_fo4_melee;
pub mod target_normalize;
mod target_preflight;
pub mod target_write;
pub mod terrain_textures;
pub mod texture_engine;
pub mod translator;
pub mod ui_animation;
mod ui_icon;
mod item_icons;
pub mod worker_pool;
mod wwise;

#[cfg(test)]
pub mod test_fixtures;

pub fn register_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    python_api::register(m)?;
    script_headers::register(m)?;
    script_patches::register(m)?;
    target_preflight::register(m)
}

#[pymodule]
fn conversion_native(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    register_module(m)
}
