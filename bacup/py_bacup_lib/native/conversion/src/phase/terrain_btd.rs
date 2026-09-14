//! Phase: `terrain_btd_write`: FO4 LAND -> Starfield `.btd`.
//!
//! Ground truth: `bacup/docs/starfield_target/R4-btd-layout.md` (§3 format,
//! §4 height codec, §5 alphas, §7 FO4->BTD geometry, §7.7 SFBK) and
//! `bacup/docs/starfield_target/R1-esm-wrld-btd.md` (§4 WRLD<->SFBK linkage
//! chain). Writes through `terrain_native::btd_write`.
//!
//! All logic lives in pub core fns so it is unit-testable without a plugin
//! handle; [`TerrainBtdWritePhase::run`] is a thin shell that gathers
//! [`LandRecordJson`] from the run's source handle, calls
//! [`build_btd_input`], calls `write_starfield_btd`, and synthesizes the
//! `SFBK` + `WRLD` overlay-component records the linkage requires.
//!
//! ## Live-schema adapter
//!
//! [`LandRecordJson::value`] follows THIS phase's own normalized contract
//! (`{"vhgt": {"base": f32, "deltas": [[i8;33];33]}, "quadrants": [...; 4]}`),
//! not the verbatim `plugin_handle_read_authoring_record_value_json` shape.
//! The VHGT half matches `esp_authoring_core::land::heightmap`'s custom-codec
//! output verbatim. The BTXT/ATXT/VTXT -> `quadrants` adapter
//! (`land_record_to_json_contract`) follows the schema field names in
//! `esp/generated/fo4.rs`; a corpus test runs it on a real `Fallout4.esm`
//! LAND. If a live shape differs, `build_btd_input` degrades that cell to
//! heights-only terrain; it never invents texture data.
//!
//! ## LTEX palette resolution
//!
//! [`build_btd_input`] interns the SOURCE FO4 `BTXT`/`ATXT` texture ids, which
//! is all it can see. The BTD's `ltex_form_ids` table is engine-resolved
//! against the live load order, so [`TerrainBtdWritePhase::run`] remaps the
//! palette through the run's `FormKeyMapper` before writing (see
//! [`remap_ltex_palette`]). This is why the phase must run after `translate`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use bytes::Bytes;
use serde_json::Value as JsonValue;
use smol_str::SmolStr;

use esp_authoring_core::plugin_runtime::{
    ParsedGroup, ParsedItem, ParsedRecord, ParsedSubrecord,
    plugin_handle_collect_worldspace_terrain_ids_json,
    plugin_handle_read_authoring_record_value_json, plugin_handle_store_ref,
};
use terrain_native::btd_write::{BtdWriteCell, BtdWriteInput, BtdWriteReport, write_starfield_btd};
use terrain_native::land_encode::{EncodedVhgt, decode_vhgt_heights};

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::FormKey;
use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::sym::Sym;

// ---------------------------------------------------------------------------
// Constants (R4 §7.1/§7.3/§7.6)
// ---------------------------------------------------------------------------

/// FO4 render units -> metres (== Starfield world units). Applied EXACTLY
/// ONCE to heights (R4 §7.1).
pub const FO4_TO_SF_SPATIAL: f64 = 1.0 / 69.99125;
const SF_CELL_METRES: f64 = 100.0;
const SF_CELL_SAMPLES: usize = 128;
const SF_SAMPLE_SPACING_M: f64 = SF_CELL_METRES / SF_CELL_SAMPLES as f64; // 0.78125
const FO4_CELL_UNITS: f64 = 4096.0;
const FO4_LAND_GRID: usize = 33; // VHGT samples per axis, shared edges
const MAX_LTEX: usize = 255; // R4 RISK-3
const MAX_QUAD_SLOTS: usize = 6; // 1 base + 5 blend, R4 §5.2
const QUAD_ALPHA_SAMPLES: usize = 17; // R4 §7.4: 17x17 per FO4 quadrant

// ---------------------------------------------------------------------------
// 1. VHGT decode (reuses terrain_native::land_encode::decode_vhgt_heights)
// ---------------------------------------------------------------------------

/// Extracts FO4 world-Z heights from this phase's VHGT JSON contract
/// (`{"base": f32, "deltas": [[i8;33];33]}`, the verbatim output of
/// `esp_authoring_core::land::heightmap::heightmap_to_yaml`) by repacking the
/// raw 1096-byte VHGT subrecord and decoding it with
/// `terrain_native::land_encode::decode_vhgt_heights`. Returns 33x33 row-major
/// heights in FO4 world units (the decoder applies the x8.0 HEIGHT_STEP).
pub fn vhgt_heights_from_authoring(vhgt_json: &JsonValue) -> Result<Vec<f32>, String> {
    let base = vhgt_json
        .get("base")
        .and_then(JsonValue::as_f64)
        .ok_or_else(|| "VHGT.base missing or not a number".to_string())? as f32;
    let deltas = vhgt_json
        .get("deltas")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "VHGT.deltas missing or not an array".to_string())?;
    if deltas.len() != FO4_LAND_GRID {
        return Err(format!(
            "VHGT.deltas must have {FO4_LAND_GRID} rows, got {}",
            deltas.len()
        ));
    }
    let mut raw = Vec::with_capacity(4 + FO4_LAND_GRID * FO4_LAND_GRID + 3);
    raw.extend_from_slice(&base.to_le_bytes());
    for (r, row) in deltas.iter().enumerate() {
        let row = row
            .as_array()
            .ok_or_else(|| format!("VHGT.deltas[{r}] is not an array"))?;
        if row.len() != FO4_LAND_GRID {
            return Err(format!(
                "VHGT.deltas[{r}] must have {FO4_LAND_GRID} entries, got {}",
                row.len()
            ));
        }
        for (c, v) in row.iter().enumerate() {
            let d = v
                .as_i64()
                .ok_or_else(|| format!("VHGT.deltas[{r}][{c}] is not an integer"))?;
            if !(-128..=127).contains(&d) {
                return Err(format!("VHGT.deltas[{r}][{c}] = {d} outside i8 range"));
            }
            raw.push(d as i8 as u8);
        }
    }
    raw.extend_from_slice(&[0, 0, 0]);
    decode_vhgt_heights(&EncodedVhgt { offset: base, raw })
}

// ---------------------------------------------------------------------------
// 2. Merge FO4 LAND cells into one continuous heightfield + Catmull-Rom
// ---------------------------------------------------------------------------

/// A continuous heightfield merged from one or more FO4 LAND cells (R4 §7.2
/// point 2: adjacent cells share edge samples, so the merged raster is
/// `cells_x*32+1` wide, not `cells_x*33`). Values are FO4 world units;
/// sample spacing is 128 FO4 units (4096-unit cell / 32 intervals).
pub struct MergedHeightfield {
    pub min_cell_x: i32,
    pub min_cell_y: i32,
    pub width: usize,
    pub height: usize,
    pub values: Vec<f32>,
}

pub fn merge_land_cells(cells: &[(i32, i32, Vec<f32>)]) -> Result<MergedHeightfield, String> {
    if cells.is_empty() {
        return Err("merge_land_cells: no cells supplied".to_string());
    }
    let min_cell_x = cells.iter().map(|c| c.0).min().unwrap();
    let max_cell_x = cells.iter().map(|c| c.0).max().unwrap();
    let min_cell_y = cells.iter().map(|c| c.1).min().unwrap();
    let max_cell_y = cells.iter().map(|c| c.1).max().unwrap();
    let width = ((max_cell_x - min_cell_x) as usize + 1) * (FO4_LAND_GRID - 1) + 1;
    let height = ((max_cell_y - min_cell_y) as usize + 1) * (FO4_LAND_GRID - 1) + 1;
    let mut values = vec![f32::NAN; width * height];
    for (cx, cy, heights) in cells {
        if heights.len() != FO4_LAND_GRID * FO4_LAND_GRID {
            return Err(format!(
                "cell ({cx}, {cy}) heights.len() = {}, expected {}",
                heights.len(),
                FO4_LAND_GRID * FO4_LAND_GRID
            ));
        }
        let ox = ((cx - min_cell_x) as usize) * (FO4_LAND_GRID - 1);
        let oy = ((cy - min_cell_y) as usize) * (FO4_LAND_GRID - 1);
        for ly in 0..FO4_LAND_GRID {
            for lx in 0..FO4_LAND_GRID {
                values[(oy + ly) * width + (ox + lx)] = heights[ly * FO4_LAND_GRID + lx];
            }
        }
    }
    Ok(MergedHeightfield {
        min_cell_x,
        min_cell_y,
        width,
        height,
        values,
    })
}

impl MergedHeightfield {
    fn get_clamped(&self, gx: isize, gy: isize) -> f32 {
        let x = gx.clamp(0, self.width as isize - 1) as usize;
        let y = gy.clamp(0, self.height as isize - 1) as usize;
        let v = self.values[y * self.width + x];
        if v.is_nan() { 0.0 } else { v }
    }

    /// Bicubic Catmull-Rom sample at an arbitrary grid position (fractional
    /// `gx`/`gy` in merged-grid units), edge-clamped beyond the field's
    /// bounds (R4 §7.2).
    pub fn sample_catmull_rom(&self, gx: f64, gy: f64) -> f32 {
        let x0 = gx.floor();
        let y0 = gy.floor();
        let tx = (gx - x0) as f32;
        let ty = (gy - y0) as f32;
        let ix = x0 as isize;
        let iy = y0 as isize;
        let mut rows = [0f32; 4];
        for (j, row) in rows.iter_mut().enumerate() {
            let jy = iy - 1 + j as isize;
            let p0 = self.get_clamped(ix - 1, jy);
            let p1 = self.get_clamped(ix, jy);
            let p2 = self.get_clamped(ix + 1, jy);
            let p3 = self.get_clamped(ix + 2, jy);
            *row = catmull_rom_1d(p0, p1, p2, p3, tx);
        }
        catmull_rom_1d(rows[0], rows[1], rows[2], rows[3], ty)
    }

    /// Sample by FO4 world position (world units, not grid units).
    pub fn sample_world(&self, world_x_units: f64, world_y_units: f64) -> f32 {
        let gx = (world_x_units - self.min_cell_x as f64 * FO4_CELL_UNITS) / FO4_LAND_GRID_SPACING;
        let gy = (world_y_units - self.min_cell_y as f64 * FO4_CELL_UNITS) / FO4_LAND_GRID_SPACING;
        self.sample_catmull_rom(gx, gy)
    }
}

const FO4_LAND_GRID_SPACING: f64 = FO4_CELL_UNITS / (FO4_LAND_GRID as f64 - 1.0); // 128.0

fn catmull_rom_1d(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

// ---------------------------------------------------------------------------
// 3. Grid extent (R4 §7.6): N = 2*ceil(half_span_m/100), res = N*128
// ---------------------------------------------------------------------------

/// `(cells_n, resolution)` for an origin-centred Starfield BTD grid covering
/// `half_span_m` metres either side of the origin, or the explicit
/// `crop_cells` override (the phase's `terrain_extent_cells` param) when set.
pub fn compute_grid_extent(half_span_m: f64, crop_cells: Option<u32>) -> (u32, u32) {
    let n = crop_cells.unwrap_or_else(|| (2.0 * (half_span_m / SF_CELL_METRES).ceil()) as u32);
    let n = n.max(2);
    (n, n * SF_CELL_SAMPLES as u32)
}

// ---------------------------------------------------------------------------
// 4. Coverage-weighted quadrant-layer selection (R4 §7.4, RISK-2)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct LtexCandidate {
    pub ltex_form_id: u32,
    pub weight: f64,
}

/// Sorts candidates descending by weight and keeps the top `max_slots`.
/// Returns `(selected, dropped_count)`. The highest-weight survivor is
/// `selected[0]` (the quadrant's base per this phase's convention: the
/// single highest-weight candidate overall becomes the base, the rest
/// become the up-to-5 blend layers).
pub fn select_top_layers(
    candidates: &[LtexCandidate],
    max_slots: usize,
) -> (Vec<LtexCandidate>, usize) {
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let dropped = sorted.len().saturating_sub(max_slots);
    sorted.truncate(max_slots);
    (sorted, dropped)
}

// ---------------------------------------------------------------------------
// 5. SFBK field derivation from a BtdWriteReport (R4 §7.7 / R1 §4.3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct SfbkFields {
    pub dnam: (u32, u32),
    pub enam: (f32, f32),
    pub fnam_len_bytes: usize,
}

/// `DNAM = (res_x>>7, res_y>>7)`, `ENAM = (height_min, height_max)`,
/// `FNAM` byte length = `cells_x*cells_y*2` (R1 §4.3, verified 475/482
/// vanilla pairs; R4 §7.7 -- only FNAM's LENGTH is load-bearing).
pub fn derive_sfbk_fields(report: &BtdWriteReport) -> SfbkFields {
    let cells_x = report.res_x >> 7;
    let cells_y = report.res_y >> 7;
    SfbkFields {
        dnam: (cells_x, cells_y),
        enam: (report.height_min, report.height_max),
        fnam_len_bytes: (cells_x as usize) * (cells_y as usize) * 2,
    }
}

// ---------------------------------------------------------------------------
// FO4 quadrant geometry helpers (R4 §3.3 point 3: q>>1 = y-half, q&1 = x-half)
// ---------------------------------------------------------------------------

/// Given a world position in FO4 units, returns `(cell_x, cell_y, quadrant,
/// u, v)` where `quadrant` follows R4's `q>>1`=y-half / `q&1`=x-half
/// convention and `u`/`v` are 0..1 across that quadrant's 17x17 sample grid
/// (a 2048-unit footprint at 128-unit spacing -- the same spacing as VHGT).
fn locate_fo4_quadrant(world_x: f64, world_y: f64) -> (i32, i32, usize, f64, f64) {
    let cell_x = (world_x / FO4_CELL_UNITS).floor();
    let cell_y = (world_y / FO4_CELL_UNITS).floor();
    let local_x = world_x - cell_x * FO4_CELL_UNITS;
    let local_y = world_y - cell_y * FO4_CELL_UNITS;
    let half = FO4_CELL_UNITS / 2.0;
    let qx = if local_x >= half { 1 } else { 0 };
    let qy = if local_y >= half { 1 } else { 0 };
    let quadrant = qy * 2 + qx;
    let ox = local_x - (qx as f64) * half;
    let oy = local_y - (qy as f64) * half;
    let u = (ox / half).clamp(0.0, 1.0);
    let v = (oy / half).clamp(0.0, 1.0);
    (cell_x as i32, cell_y as i32, quadrant, u, v)
}

fn bilinear_sample_grid17(grid: &[f32], u: f64, v: f64) -> f32 {
    let last = (QUAD_ALPHA_SAMPLES - 1) as f64;
    let fx = u * last;
    let fy = v * last;
    let x0 = fx.floor().clamp(0.0, last) as usize;
    let y0 = fy.floor().clamp(0.0, last) as usize;
    let x1 = (x0 + 1).min(QUAD_ALPHA_SAMPLES - 1);
    let y1 = (y0 + 1).min(QUAD_ALPHA_SAMPLES - 1);
    let tx = (fx - x0 as f64) as f32;
    let ty = (fy - y0 as f64) as f32;
    let g = |x: usize, y: usize| grid[y * QUAD_ALPHA_SAMPLES + x];
    let top = g(x0, y0) * (1.0 - tx) + g(x1, y0) * tx;
    let bot = g(x0, y1) * (1.0 - tx) + g(x1, y1) * tx;
    top * (1.0 - ty) + bot * ty
}

fn quantize_alpha_3bit(alpha: f32) -> u8 {
    (alpha.clamp(0.0, 1.0) * 7.0).round() as u8
}

/// Raw on-disk FormID for a mapped target `FormKey`. Replicates
/// `formkey_mapper.rs`'s private `raw_formid_for_target` / `target_write.rs`'s
/// private `own_index` (neither is `pub`) — the output plugin's own records
/// index at `target_master_names.len()`, master records at their master index.
fn raw_target_formid(
    fk: FormKey,
    interner: &crate::sym::StringInterner,
    output_plugin_name: &str,
    target_master_names: &[String],
) -> Option<u32> {
    let plugin_name = interner.resolve(fk.plugin)?;
    let index = if plugin_name.eq_ignore_ascii_case(output_plugin_name) {
        target_master_names.len()
    } else {
        target_master_names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(plugin_name))?
    };
    (index <= 0xFF).then(|| ((index as u32) << 24) | (fk.local & 0x00FF_FFFF))
}

/// Rewrites the BTD's LTEX palette in place, from the SOURCE FO4 object ids
/// `build_btd_input` interned (BTXT/ATXT `Texture` references) to the raw
/// on-disk FormIDs of the LTEX records this run actually wrote.
///
/// Palette entries are engine-resolved FormIDs: a source FO4 id would resolve
/// against `Starfield.esm` (master index 0) and land on an unrelated vanilla
/// record. An entry with no mapping is zeroed (FormID 0 = no landscape
/// texture, the "unused" value the `ltex_map` encoder emits) and reported.
///
/// LOAD-ORDER CAVEAT: a `.btd` is a loose data file with no master table, so
/// these FormIDs resolve against the LIVE load order as-is. Vanilla never
/// exercises this (every vanilla palette entry is a `Starfield.esm` record at
/// master index 0). An own-plugin entry is only correct while the output
/// plugin loads at `target_master_names.len()`, i.e. right after its masters.
/// A palette that looks right in isolation and wrong beside other mods is this.
///
/// Returns one human-readable note per unmapped palette entry.
fn remap_ltex_palette(
    ltex_form_ids: &mut [u32],
    source_plugin: Sym,
    mapper: &FormKeyMapper<'_>,
    output_plugin_name: &str,
    target_master_names: &[String],
) -> Vec<String> {
    let mut notes = Vec::new();
    for slot in ltex_form_ids.iter_mut() {
        let source_fk = FormKey {
            local: *slot & 0x00FF_FFFF,
            plugin: source_plugin,
        };
        let raw = mapper.lookup(source_fk).and_then(|target_fk| {
            raw_target_formid(
                target_fk,
                mapper.interner,
                output_plugin_name,
                target_master_names,
            )
        });
        match raw {
            Some(raw) => *slot = raw,
            None => {
                notes.push(format!(
                    "LTEX {:#08X} has no translated counterpart; palette slot zeroed",
                    *slot
                ));
                *slot = 0;
            }
        }
    }
    notes
}

fn intern_ltex(id: u32, table: &mut Vec<u32>, index: &mut HashMap<u32, usize>) -> usize {
    if let Some(&i) = index.get(&id) {
        return i;
    }
    let i = table.len();
    table.push(id);
    index.insert(id, i);
    i
}

// ---------------------------------------------------------------------------
// build_btd_input — the pure core the Phase impl is a thin shell over
// ---------------------------------------------------------------------------

/// One FO4 LAND record in this phase's JSON contract (see module docs for
/// the live-schema-adapter caveat):
/// `{"vhgt": {"base": f32, "deltas": [[i8;33];33]}, "quadrants": [q0,q1,q2,q3]}`
/// where each `qN = {"base_ltex": u32, "layers": [{"ltex": u32, "alpha": [f32; 289]}, ...]}`.
pub struct LandRecordJson {
    pub cell_x: i32,
    pub cell_y: i32,
    pub value: JsonValue,
}

pub struct TerrainBtdParams {
    pub worldspace_editor_id: String,
    /// Optional crop override (phase param `terrain_extent_cells`). `None`
    /// measures the extent from the supplied LAND cells' bounds.
    pub terrain_extent_cells: Option<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct BuildBtdReport {
    pub cells_written: u32,
    pub cells_defaulted: u32,
    pub layers_dropped: u32,
    pub ltex_merges: u32,
    /// Human-readable detail per drop/merge, for Log events (no silent drops).
    pub dropped_detail: Vec<String>,
}

#[derive(Default, Clone)]
struct ResolvedQuadrant {
    base_ltex: Option<u32>,
    layers: Vec<(u32, Vec<f32>)>, // (ltex_form_id, 17x17 row-major alpha)
}

struct ResolvedCell {
    cell_x: i32,
    cell_y: i32,
    heights: Vec<f32>, // 33x33
    quadrants: [ResolvedQuadrant; 4],
}

fn parse_quadrants(land_json: &JsonValue) -> Result<[ResolvedQuadrant; 4], String> {
    let arr = land_json
        .get("quadrants")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "missing quadrants array".to_string())?;
    if arr.len() != 4 {
        return Err(format!("quadrants must have 4 entries, got {}", arr.len()));
    }
    let mut out: [ResolvedQuadrant; 4] = Default::default();
    for (i, q) in arr.iter().enumerate() {
        let base_ltex = q
            .get("base_ltex")
            .and_then(JsonValue::as_u64)
            .map(|v| v as u32)
            .filter(|&v| v != 0);
        let layers_json = q
            .get("layers")
            .and_then(JsonValue::as_array)
            .cloned()
            .unwrap_or_default();
        let mut layers = Vec::with_capacity(layers_json.len());
        for l in &layers_json {
            let ltex = l
                .get("ltex")
                .and_then(JsonValue::as_u64)
                .ok_or_else(|| format!("quadrants[{i}].layers[..] missing ltex"))?
                as u32;
            let alpha_json = l
                .get("alpha")
                .and_then(JsonValue::as_array)
                .ok_or_else(|| format!("quadrants[{i}].layers[..] missing alpha"))?;
            let expected = QUAD_ALPHA_SAMPLES * QUAD_ALPHA_SAMPLES;
            if alpha_json.len() != expected {
                return Err(format!(
                    "quadrants[{i}].layers[..].alpha must have {expected} entries, got {}",
                    alpha_json.len()
                ));
            }
            let alpha: Vec<f32> = alpha_json
                .iter()
                .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                .collect();
            layers.push((ltex, alpha));
        }
        out[i] = ResolvedQuadrant { base_ltex, layers };
    }
    Ok(out)
}

fn global_ltex_popularity(resolved: &[ResolvedCell]) -> HashMap<u32, f64> {
    let mut pop: HashMap<u32, f64> = HashMap::new();
    for cell in resolved {
        for q in &cell.quadrants {
            if let Some(base) = q.base_ltex {
                *pop.entry(base).or_insert(0.0) += 1.0;
            }
            for (ltex, alpha) in &q.layers {
                let mean: f64 =
                    alpha.iter().map(|&a| a as f64).sum::<f64>() / (alpha.len().max(1) as f64);
                *pop.entry(*ltex).or_insert(0.0) += mean.max(0.01);
            }
        }
    }
    pop
}

/// Builds a [`BtdWriteInput`] from a worldspace's FO4 LAND records. Pure,
/// no I/O -- the `Phase` impl gathers `lands` from the run's source handle
/// and calls this. See R4 §7.2 (resample, not per-cell conversion), §7.3
/// (unshifted frame), §7.4 (coverage-weighted layer selection), §7.6 (grid
/// sizing), RISK-2/RISK-3 (LTEX budget).
pub fn build_btd_input(
    lands: &[LandRecordJson],
    params: &TerrainBtdParams,
) -> Result<(BtdWriteInput, BuildBtdReport), String> {
    if lands.is_empty() {
        return Err("build_btd_input: no LAND cells supplied".to_string());
    }
    let mut report = BuildBtdReport::default();

    let mut resolved = Vec::with_capacity(lands.len());
    for land in lands {
        let vhgt = land
            .value
            .get("vhgt")
            .ok_or_else(|| format!("cell ({}, {}) missing vhgt", land.cell_x, land.cell_y))?;
        let heights = vhgt_heights_from_authoring(vhgt)
            .map_err(|e| format!("cell ({}, {}) VHGT: {e}", land.cell_x, land.cell_y))?;
        let quadrants = parse_quadrants(&land.value)
            .map_err(|e| format!("cell ({}, {}) quadrants: {e}", land.cell_x, land.cell_y))?;
        resolved.push(ResolvedCell {
            cell_x: land.cell_x,
            cell_y: land.cell_y,
            heights,
            quadrants,
        });
    }

    let merge_input: Vec<(i32, i32, Vec<f32>)> = resolved
        .iter()
        .map(|c| (c.cell_x, c.cell_y, c.heights.clone()))
        .collect();
    let field = merge_land_cells(&merge_input)?;

    let min_cx = resolved.iter().map(|c| c.cell_x).min().unwrap();
    let max_cx = resolved.iter().map(|c| c.cell_x).max().unwrap();
    let min_cy = resolved.iter().map(|c| c.cell_y).min().unwrap();
    let max_cy = resolved.iter().map(|c| c.cell_y).max().unwrap();

    let span_x_m = (max_cx - min_cx + 1) as f64 * FO4_CELL_UNITS * FO4_TO_SF_SPATIAL;
    let span_y_m = (max_cy - min_cy + 1) as f64 * FO4_CELL_UNITS * FO4_TO_SF_SPATIAL;
    let half_span_m = span_x_m.max(span_y_m) / 2.0;
    let (cells_n, _res) = compute_grid_extent(half_span_m, params.terrain_extent_cells);

    let cell_min = -((cells_n as i32) >> 1);
    let cell_max = cell_min + cells_n as i32 - 1;

    let land_min_x = min_cx as f64 * FO4_CELL_UNITS;
    let land_max_x = (max_cx + 1) as f64 * FO4_CELL_UNITS;
    let land_min_y = min_cy as f64 * FO4_CELL_UNITS;
    let land_max_y = (max_cy + 1) as f64 * FO4_CELL_UNITS;

    let by_cell: HashMap<(i32, i32), &ResolvedCell> =
        resolved.iter().map(|c| ((c.cell_x, c.cell_y), c)).collect();

    // LTEX budget (RISK-3): decide, from source-side usage, which textures
    // survive if the worldspace's palette exceeds 255. Computed once, from
    // the (cheap) input record count -- not from the (expensive) resampled
    // SF grid -- and applied as a candidate filter below, so a `None`
    // (no drops) result costs nothing extra.
    let popularity = global_ltex_popularity(&resolved);
    let kept_set: Option<HashSet<u32>> = if popularity.len() > MAX_LTEX {
        let mut sorted: Vec<(u32, f64)> = popularity.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let dropped: Vec<u32> = sorted[MAX_LTEX..].iter().map(|(id, _)| *id).collect();
        report.ltex_merges = dropped.len() as u32;
        for id in &dropped {
            report.dropped_detail.push(format!(
                "LTEX budget: {id:#010X} merged out ({} unique textures > {MAX_LTEX} cap)",
                sorted.len()
            ));
        }
        Some(sorted[..MAX_LTEX].iter().map(|(id, _)| *id).collect())
    } else {
        None
    };
    let is_kept = |id: u32| kept_set.as_ref().is_none_or(|s| s.contains(&id));

    let mut ltex_table: Vec<u32> = Vec::new();
    let mut ltex_index: HashMap<u32, usize> = HashMap::new();
    let mut cells = Vec::new();

    for cy in cell_min..=cell_max {
        for cx in cell_min..=cell_max {
            let sf_min_x = cx as f64 * SF_CELL_METRES;
            let sf_min_y = cy as f64 * SF_CELL_METRES;
            let fo4_min_x = sf_min_x / FO4_TO_SF_SPATIAL;
            let fo4_min_y = sf_min_y / FO4_TO_SF_SPATIAL;
            let fo4_max_x = (sf_min_x + SF_CELL_METRES) / FO4_TO_SF_SPATIAL;
            let fo4_max_y = (sf_min_y + SF_CELL_METRES) / FO4_TO_SF_SPATIAL;

            let overlaps = fo4_max_x > land_min_x
                && fo4_min_x < land_max_x
                && fo4_max_y > land_min_y
                && fo4_min_y < land_max_y;
            if !overlaps {
                report.cells_defaulted += 1;
                continue;
            }

            let mut heights = vec![0f32; SF_CELL_SAMPLES * SF_CELL_SAMPLES];
            for sy in 0..SF_CELL_SAMPLES {
                let wy = sf_min_y + sy as f64 * SF_SAMPLE_SPACING_M;
                let fy = wy / FO4_TO_SF_SPATIAL;
                for sx in 0..SF_CELL_SAMPLES {
                    let wx = sf_min_x + sx as f64 * SF_SAMPLE_SPACING_M;
                    let fx = wx / FO4_TO_SF_SPATIAL;
                    let h_fo4 = field.sample_world(fx, fy);
                    heights[sy * SF_CELL_SAMPLES + sx] = (h_fo4 as f64 * FO4_TO_SF_SPATIAL) as f32;
                }
            }

            let mut quad_layers = [[0xFFu8; 6]; 4];
            let mut quad_alphas = vec![0u16; SF_CELL_SAMPLES * SF_CELL_SAMPLES];

            for quadrant in 0..4usize {
                let qy0 = if quadrant >= 2 { 64 } else { 0 };
                let qx0 = if quadrant % 2 == 1 { 64 } else { 0 };

                // Pass 1: coverage-weighted candidate accumulation (R4 §7.4).
                let mut weights: HashMap<u32, f64> = HashMap::new();
                for ly in 0..64usize {
                    let sy = qy0 + ly;
                    let wy = sf_min_y + sy as f64 * SF_SAMPLE_SPACING_M;
                    let fy = wy / FO4_TO_SF_SPATIAL;
                    for lx in 0..64usize {
                        let sx = qx0 + lx;
                        let wx = sf_min_x + sx as f64 * SF_SAMPLE_SPACING_M;
                        let fx = wx / FO4_TO_SF_SPATIAL;
                        let (fcx, fcy, fquad, u, v) = locate_fo4_quadrant(fx, fy);
                        let Some(fo4_cell) = by_cell.get(&(fcx, fcy)) else {
                            continue;
                        };
                        let q = &fo4_cell.quadrants[fquad];
                        if let Some(base) = q.base_ltex {
                            if is_kept(base) {
                                *weights.entry(base).or_insert(0.0) += 1.0;
                            }
                        }
                        for (ltex, alpha) in &q.layers {
                            if is_kept(*ltex) {
                                let a = bilinear_sample_grid17(alpha, u, v) as f64;
                                *weights.entry(*ltex).or_insert(0.0) += a;
                            }
                        }
                    }
                }

                if weights.is_empty() {
                    continue;
                }
                let candidates: Vec<LtexCandidate> = weights
                    .into_iter()
                    .map(|(id, weight)| LtexCandidate {
                        ltex_form_id: id,
                        weight,
                    })
                    .collect();
                let (selected, dropped) = select_top_layers(&candidates, MAX_QUAD_SLOTS);
                if dropped > 0 {
                    report.layers_dropped += dropped as u32;
                    let dropped_ids: Vec<u32> = candidates
                        .iter()
                        .filter(|c| !selected.iter().any(|s| s.ltex_form_id == c.ltex_form_id))
                        .map(|c| c.ltex_form_id)
                        .collect();
                    report.dropped_detail.push(format!(
                        "SF cell ({cx}, {cy}) quadrant {quadrant}: dropped {dropped} LTEX candidate(s): {dropped_ids:#010X?}"
                    ));
                }

                let base_id = selected[0].ltex_form_id;
                let blend: Vec<u32> = selected[1..].iter().map(|c| c.ltex_form_id).collect();

                quad_layers[quadrant][0] =
                    intern_ltex(base_id, &mut ltex_table, &mut ltex_index) as u8;
                for (j, ltex) in blend.iter().enumerate() {
                    quad_layers[quadrant][j + 1] =
                        intern_ltex(*ltex, &mut ltex_table, &mut ltex_index) as u8;
                }

                // Pass 2: write the actual per-sample packed alpha words.
                for ly in 0..64usize {
                    let sy = qy0 + ly;
                    let wy = sf_min_y + sy as f64 * SF_SAMPLE_SPACING_M;
                    let fy = wy / FO4_TO_SF_SPATIAL;
                    for lx in 0..64usize {
                        let sx = qx0 + lx;
                        let wx = sf_min_x + sx as f64 * SF_SAMPLE_SPACING_M;
                        let fx = wx / FO4_TO_SF_SPATIAL;
                        let (fcx, fcy, fquad, u, v) = locate_fo4_quadrant(fx, fy);
                        let Some(fo4_cell) = by_cell.get(&(fcx, fcy)) else {
                            continue;
                        };
                        let q = &fo4_cell.quadrants[fquad];
                        let mut word = 0u16;
                        for (j, ltex) in blend.iter().enumerate() {
                            if let Some((_, alpha)) = q.layers.iter().find(|(id, _)| id == ltex) {
                                let a = bilinear_sample_grid17(alpha, u, v);
                                let q3 = quantize_alpha_3bit(a);
                                word |= (q3 as u16) << (j * 3);
                            }
                        }
                        quad_alphas[sy * SF_CELL_SAMPLES + sx] = word;
                    }
                }
            }

            cells.push(BtdWriteCell {
                cell_x: cx,
                cell_y: cy,
                heights,
                quad_layers,
                quad_alphas,
            });
            report.cells_written += 1;
        }
    }

    if ltex_table.len() > MAX_LTEX {
        return Err(format!(
            "build_btd_input: internal error -- ltex_table grew to {} despite the {MAX_LTEX} budget filter",
            ltex_table.len()
        ));
    }

    let input = BtdWriteInput {
        cells_x: cells_n,
        cells_y: cells_n,
        ltex_form_ids: ltex_table,
        cells,
    };
    Ok((input, report))
}

// ---------------------------------------------------------------------------
// Phase shell: gather source LAND records, write the .btd, synthesize SFBK
// ---------------------------------------------------------------------------

/// Adapter from the live LAND authoring dict into this phase's
/// `{"vhgt": ..., "quadrants": [...; 4]}` contract. The
/// `plugin_handle_read_authoring_record_value_json` dict is `{"form_id",
/// "form_version", "fields": [{Label: value}, ...]}`, one object per
/// subrecord IN FILE ORDER (repeated subrecords appear as separate array
/// entries, not grouped under one key).
/// `custom_codec` fields (VHGT) key by their display label with spaces
/// stripped ("Vertex Height Map" -> `"VertexHeightMap"`); plain `parsed`
/// struct fields (BTXT/ATXT) key by their raw subrecord id. A `formid`-kind
/// sub-field renders as `{"reference": {"plugin": ..., "object_id": "<hex>"}}`.
/// VTXT (`scope_id: "layers"`) has no id of its own in this dict -- it keys
/// by ITS display label `"AlphaLayerData"` and is the fields-array entry
/// immediately following the ATXT it belongs to. A `Quadrant`/`Layer` field
/// equal to its schema default (0) is omitted entirely by the serializer,
/// so an absent `Quadrant` means quadrant 0, not "skip this entry".
fn land_record_to_json_contract(
    cell_x: i32,
    cell_y: i32,
    land_value: &JsonValue,
) -> LandRecordJson {
    let fields = land_value
        .get("fields")
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();

    let vhgt = fields
        .iter()
        .find_map(|f| f.as_object().and_then(|o| o.get("VertexHeightMap")))
        .cloned()
        .unwrap_or(JsonValue::Null);

    let mut quadrants = vec![serde_json::json!({"base_ltex": 0, "layers": []}); 4];

    let mut i = 0usize;
    while i < fields.len() {
        let Some(obj) = fields[i].as_object() else {
            i += 1;
            continue;
        };
        if let Some(btxt) = obj.get("BTXT") {
            let q = btxt
                .get("Quadrant")
                .and_then(JsonValue::as_u64)
                .unwrap_or(0) as usize;
            if q < 4 {
                if let Some(ltex) = btxt.get("Texture").and_then(form_id_from_field) {
                    quadrants[q]["base_ltex"] = serde_json::json!(ltex);
                }
            }
        } else if let Some(atxt) = obj.get("ATXT") {
            let q = atxt
                .get("Quadrant")
                .and_then(JsonValue::as_u64)
                .unwrap_or(0) as usize;
            let ltex = atxt.get("Texture").and_then(form_id_from_field);
            let mut alpha = vec![0f32; QUAD_ALPHA_SAMPLES * QUAD_ALPHA_SAMPLES];
            if let Some(next) = fields.get(i + 1).and_then(JsonValue::as_object) {
                if let Some(positions) = next.get("AlphaLayerData").and_then(JsonValue::as_array) {
                    for p in positions {
                        let pos = p.get("AlphaLayerDataPosition").and_then(JsonValue::as_u64);
                        let opacity = p.get("AlphaLayerDataOpacity").and_then(JsonValue::as_f64);
                        if let (Some(pos), Some(opacity)) = (pos, opacity) {
                            if (pos as usize) < alpha.len() {
                                alpha[pos as usize] = opacity as f32;
                            }
                        }
                    }
                }
            }
            if q < 4 {
                if let Some(ltex) = ltex {
                    if let Some(layers) = quadrants[q]
                        .get_mut("layers")
                        .and_then(|v| v.as_array_mut())
                    {
                        layers.push(serde_json::json!({"ltex": ltex, "alpha": alpha}));
                    }
                }
            }
        }
        i += 1;
    }

    LandRecordJson {
        cell_x,
        cell_y,
        value: serde_json::json!({"vhgt": vhgt, "quadrants": quadrants}),
    }
}

/// Pulls a `u32` FormID out of a `formid`-kind field's authoring value:
/// a plain integer, a `"plugin:objid"` string, or the nested
/// `{"reference": {"plugin": ..., "object_id": "<hex>"}}` shape (verified
/// live -- see `land_record_to_json_contract`'s doc comment).
fn form_id_from_field(value: &JsonValue) -> Option<u32> {
    if let Some(n) = value.as_u64() {
        return Some(n as u32);
    }
    if let Some(s) = value.as_str() {
        let hex_part = s.rsplit(':').next().unwrap_or(s);
        return u32::from_str_radix(
            hex_part.trim_start_matches("0x").trim_start_matches("0X"),
            16,
        )
        .ok();
    }
    let object_id = value
        .get("reference")
        .and_then(|r| r.get("object_id"))
        .and_then(JsonValue::as_str)?;
    u32::from_str_radix(
        object_id.trim_start_matches("0x").trim_start_matches("0X"),
        16,
    )
    .ok()
}

fn collect_source_land_records(
    source_handle_id: u64,
    worldspace_editor_id: &str,
) -> Result<Vec<LandRecordJson>, String> {
    let payload_json = plugin_handle_collect_worldspace_terrain_ids_json(
        source_handle_id,
        worldspace_editor_id,
        0,
        0,
        -1,
        -1,
    )
    .map_err(|e| e.to_string())?;
    let payload: JsonValue = serde_json::from_str(&payload_json).map_err(|e| e.to_string())?;
    let cells = payload
        .get("cells")
        .and_then(JsonValue::as_array)
        .cloned()
        .unwrap_or_default();

    // `land_form_id` in the payload is the low-24-bit object id, unqualified
    // by plugin (cell_slice.rs masks it). `by_form_key` indexes records under
    // a plugin-QUALIFIED FormKey (FormKey's Eq/Hash compare the plugin field
    // too -- an empty-plugin "raw" key never matches), so a plain 8-hex-digit
    // lookup silently finds nothing. Resolve the owning plugin name once.
    let plugin_name =
        crate::source_read::plugin_name_for_handle(source_handle_id).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    for cell in &cells {
        let x = cell.get("x").and_then(JsonValue::as_i64).unwrap_or(0) as i32;
        let y = cell.get("y").and_then(JsonValue::as_i64).unwrap_or(0) as i32;
        let Some(land_form_id) = cell.get("land_form_id").and_then(JsonValue::as_u64) else {
            continue;
        };
        let form_key = format!("{plugin_name}:{:06X}", (land_form_id as u32) & 0x00FF_FFFF);
        let record = plugin_handle_read_authoring_record_value_json(source_handle_id, &form_key)
            .map_err(|e| e.to_string())?;
        let Some(land_value) = record else { continue };
        out.push(land_record_to_json_contract(x, y, &land_value));
    }
    Ok(out)
}

fn zstring_bytes(s: &str) -> Vec<u8> {
    let mut b = s.as_bytes().to_vec();
    b.push(0);
    b
}

fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
    ParsedSubrecord {
        signature: SmolStr::new(sig),
        data: Bytes::from(data),
        semantic_type: None,
    }
}

fn editor_id_of(record: &ParsedRecord) -> Option<String> {
    let subrecord = record
        .subrecords
        .iter()
        .find(|s| s.signature.as_str() == "EDID")?;
    let bytes = subrecord.data.as_ref();
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).ok().map(str::to_owned)
}

const TOP_GROUP_TYPE: i32 = 0;

/// Synthesizes one `SFBK` record (R1 §4.3) and appends the WRLD-side
/// `BGSWorldSpaceOverlay_Component` (R1 §4.4) to the target worldspace by
/// direct `ParsedItem`/`ParsedRecord` manipulation, like
/// `starfield_worldspace_gate.rs`/`interior_cells.rs`. Subrecords are raw
/// bytes: the schema's `--authoring` view renders SFBK and these scoped WRLD
/// fields as `raw_hex` too (R1 §7), and their schema field names are
/// scope-ambiguous.
fn synthesize_sfbk_and_wrld_overlay(
    target_handle_id: u64,
    worldspace_editor_id: &str,
    fields: &SfbkFields,
) -> Result<u32, String> {
    let sfbk_eid = format!("OverlayBlock{worldspace_editor_id}");
    let anam = format!("Data\\Terrain\\{worldspace_editor_id}.btd");
    // R4 §7.7 / R1 §4.3: FNAM content is unknown; only its LENGTH is
    // load-bearing (verified 475/482 vanilla pairs), so it is zero-filled.
    // OverlayBlockNewAtlantis (Starfield.esm 034763) has a 392-byte FNAM
    // (14*14*2, matching newatlantis.btd's DNAM) of 196 i16 values in
    // -30660..31088. Quantizing each cell's min or max height from the
    // .btd's per-cell table (offset 0x2C + ltex_count*4) into the header's
    // 0.0..260.502 range correlates at |r| ~= 0.7 (negative) with zero exact
    // matches, so it is not a plain per-cell height encoding.
    let fnam = vec![0u8; fields.fnam_len_bytes];

    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|_| "plugin handle store poisoned".to_string())?;
    let slot = store
        .get_mut(&target_handle_id)
        .ok_or_else(|| format!("unknown target plugin handle: {target_handle_id}"))?;

    let object_id = slot.parsed.header.next_object_id & 0x00FF_FFFF;
    slot.parsed.header.next_object_id = (object_id + 1) & 0x00FF_FFFF;
    let local_index = slot.parsed.header.masters.len() as u32;
    let sfbk_form_id = (local_index << 24) | object_id;

    let subrecords = vec![
        sub("EDID", zstring_bytes(&sfbk_eid)),
        sub("ANAM", zstring_bytes(&anam)),
        sub(
            "DNAM",
            [fields.dnam.0.to_le_bytes(), fields.dnam.1.to_le_bytes()].concat(),
        ),
        sub(
            "ENAM",
            [fields.enam.0.to_le_bytes(), fields.enam.1.to_le_bytes()].concat(),
        ),
        sub("FNAM", fnam),
        sub("GNAM", vec![0u8]),
        sub("HNAM", 0i16.to_le_bytes().to_vec()),
        sub("INAM", vec![0u8]),
        sub("JNAM", vec![0u8]),
        sub("KNAM", vec![0u8]),
        sub("WHGT", f32::MIN.to_le_bytes().to_vec()), // -FLT_MAX = "no water" (R1 §4.3)
        sub("NAM1", zstring_bytes("OverlayBlock")),
        sub("NAM2", [0i32.to_le_bytes(), 0i32.to_le_bytes()].concat()),
        sub(
            "NAM3",
            [
                (fields.dnam.0 as i32 - 1).to_le_bytes(),
                (fields.dnam.1 as i32 - 1).to_le_bytes(),
            ]
            .concat(),
        ),
        sub("NAM4", [0f32.to_le_bytes(), 0f32.to_le_bytes()].concat()),
    ];

    let sfbk_record = ParsedRecord {
        signature: SmolStr::new("SFBK"),
        form_id: sfbk_form_id,
        flags: 0,
        version_control: 0,
        form_version: Some(581), // R1 §1: Starfield's uniform record form version
        version2: Some(0),
        subrecords,
        raw_payload: None,
        parse_error: None,
    };

    let sfbk_top_idx = slot.parsed.root_items.iter().position(
        |item| matches!(item, ParsedItem::Group(g) if g.group_type == TOP_GROUP_TYPE && &g.label == b"SFBK"),
    );
    match sfbk_top_idx {
        Some(idx) => {
            if let ParsedItem::Group(g) = &mut slot.parsed.root_items[idx] {
                g.children.push(ParsedItem::Record(sfbk_record));
            }
        }
        None => {
            slot.parsed.root_items.push(ParsedItem::Group(ParsedGroup {
                label: *b"SFBK",
                group_type: TOP_GROUP_TYPE,
                tail: Bytes::new(),
                children: vec![ParsedItem::Record(sfbk_record)],
            }));
        }
    }

    let wrld_top = slot
        .parsed
        .root_items
        .iter_mut()
        .find_map(|item| match item {
            ParsedItem::Group(g) if g.group_type == TOP_GROUP_TYPE && &g.label == b"WRLD" => {
                Some(g)
            }
            _ => None,
        })
        .ok_or_else(|| "target plugin has no WRLD top group".to_string())?;

    let wrld_record = wrld_top
        .children
        .iter_mut()
        .find_map(|item| match item {
            ParsedItem::Record(r)
                if r.signature.as_str() == "WRLD"
                    && editor_id_of(r).as_deref() == Some(worldspace_editor_id) =>
            {
                Some(r)
            }
            _ => None,
        })
        .ok_or_else(|| format!("target WRLD '{worldspace_editor_id}' not found"))?;

    wrld_record
        .subrecords
        .push(sub("BFCB", zstring_bytes("BGSWorldSpaceOverlay_Component")));
    wrld_record
        .subrecords
        .push(sub("SNAM", 0i32.to_le_bytes().to_vec()));
    wrld_record
        .subrecords
        .push(sub("PNAM", 0i32.to_le_bytes().to_vec()));
    wrld_record
        .subrecords
        .push(sub("BNAM", sfbk_form_id.to_le_bytes().to_vec()));
    wrld_record.subrecords.push(sub("BFCE", Vec::new()));

    Ok(sfbk_form_id)
}

fn parse_params(params: &JsonValue) -> Result<TerrainBtdParams, PhaseError> {
    let worldspace_editor_id = params
        .get("worldspace_editor_id")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| PhaseError::BadParams("missing worldspace_editor_id".into()))?
        .to_string();
    let terrain_extent_cells = params
        .get("terrain_extent_cells")
        .and_then(JsonValue::as_u64)
        .map(|v| v as u32);
    Ok(TerrainBtdParams {
        worldspace_editor_id,
        terrain_extent_cells,
    })
}

pub struct TerrainBtdWritePhase;

impl Phase for TerrainBtdWritePhase {
    fn name(&self) -> &'static str {
        "terrain_btd_write"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let params = parse_params(ctx.params)?;

        let source_handle_id = ctx.run.source_handle().ok_or_else(|| {
            PhaseError::Internal("terrain_btd_write requires a source plugin".into())
        })?;

        let lands = collect_source_land_records(source_handle_id, &params.worldspace_editor_id)
            .map_err(PhaseError::Internal)?;
        if lands.is_empty() {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Warn,
                message: format!(
                    "terrain_btd_write: worldspace '{}' has no LAND records; nothing to write",
                    params.worldspace_editor_id
                ),
            });
            return Ok(PhaseReport::default());
        }

        let reporter =
            ProgressReporter::new(self.name(), lands.len() as u32, ctx.run.event_tx.clone());
        reporter.set_item(params.worldspace_editor_id.clone());

        let (mut input, build_report) =
            build_btd_input(&lands, &params).map_err(PhaseError::Internal)?;
        reporter.inc(lands.len() as u32);
        reporter.finish();

        for detail in &build_report.dropped_detail {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Warn,
                message: format!("terrain_btd_write: {detail}"),
            });
        }

        let ltex_notes = {
            let source_plugin_name = crate::source_read::plugin_name_for_handle(source_handle_id)
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            let source_plugin = ctx.run.interner.intern(&source_plugin_name);
            let output_plugin_name = ctx.run.config.output_plugin_name.clone();
            let target_master_names = ctx.run.config.target_master_names.clone();
            let state = ctx.run.mapper_state.as_mut().ok_or_else(|| {
                PhaseError::Internal(
                    "terrain_btd_write: mapper_state not initialized; must run after translate"
                        .into(),
                )
            })?;
            let mapper = FormKeyMapper::from_state(state, &ctx.run.interner);
            remap_ltex_palette(
                &mut input.ltex_form_ids,
                source_plugin,
                &mapper,
                &output_plugin_name,
                &target_master_names,
            )
        };
        for note in &ltex_notes {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Warn,
                message: format!("terrain_btd_write: {note}"),
            });
        }

        let terrain_dir = ctx.mod_path.join("data").join("terrain");
        std::fs::create_dir_all(&terrain_dir).map_err(|e| PhaseError::Internal(e.to_string()))?;
        let out_path = terrain_dir.join(format!("{}.btd", params.worldspace_editor_id));

        let write_report = write_starfield_btd(&out_path, &input).map_err(PhaseError::Internal)?;
        let btd_bytes = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);

        let sfbk_fields = derive_sfbk_fields(&write_report);
        let target_handle_id = ctx.run.target_handle_id;
        let sfbk_result = synthesize_sfbk_and_wrld_overlay(
            target_handle_id,
            &params.worldspace_editor_id,
            &sfbk_fields,
        );
        let (records_added, records_changed) = match &sfbk_result {
            Ok(_) => (1, 1),
            Err(error) => {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: self.name(),
                    level: LogLevel::Error,
                    message: format!(
                        "terrain_btd_write: SFBK/WRLD overlay synthesis failed for '{}': {error}",
                        params.worldspace_editor_id
                    ),
                });
                (0, 0)
            }
        };

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "terrain_btd_write: cells_written={} cells_defaulted={} layers_dropped={} ltex_merges={} res={}x{} btd_bytes={btd_bytes}",
                build_report.cells_written,
                build_report.cells_defaulted,
                build_report.layers_dropped,
                build_report.ltex_merges,
                write_report.res_x,
                write_report.res_y,
            ),
        });

        Ok(PhaseReport {
            records_added,
            records_changed,
            assets_written: 1,
            warnings: build_report.layers_dropped + build_report.ltex_merges,
            ..PhaseReport::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- (a) VHGT decode vs hand-computed fixture ---------------------------

    #[test]
    fn vhgt_heights_from_authoring_matches_hand_computed_deltas() {
        // base = 100.0 units -> 800.0 world; deltas: col0=+3, col1=-2, rest 0
        // row0: units = [3, 1, 1, 1, ...]; row1 col0 delta=+5 -> units=[6,6,...]
        let mut deltas = vec![vec![0i64; 33]; 33];
        deltas[0][0] = 3;
        deltas[0][1] = -2;
        deltas[1][0] = 5;
        let vhgt = serde_json::json!({"base": 100.0, "deltas": deltas});

        let heights = vhgt_heights_from_authoring(&vhgt).expect("decode");
        assert_eq!(heights.len(), 33 * 33);

        // Hand-computed: row0 units accumulate 3, then 3-2=1 (held for col>=1).
        assert!((heights[0] - (100.0 + 3.0) * 8.0).abs() < 1e-4, "row0 col0");
        assert!((heights[1] - (100.0 + 1.0) * 8.0).abs() < 1e-4, "row0 col1");
        assert!(
            (heights[2] - (100.0 + 1.0) * 8.0).abs() < 1e-4,
            "row0 col2 (delta 0, holds)"
        );
        // row1 col0: row_start(row0 col0)=3, + delta 5 = 8 units.
        assert!(
            (heights[33] - (100.0 + 8.0) * 8.0).abs() < 1e-4,
            "row1 col0"
        );
    }

    #[test]
    fn vhgt_heights_from_authoring_rejects_wrong_row_count() {
        let deltas: Vec<Vec<i64>> = vec![vec![0i64; 33]; 10];
        let vhgt = serde_json::json!({"base": 0.0, "deltas": deltas});
        let err = vhgt_heights_from_authoring(&vhgt).expect_err("must reject");
        assert!(err.contains("33 rows"), "{err}");
    }

    // -- (b) resample continuity across the FO4 cell seam --------------------

    #[test]
    fn catmull_rom_resample_has_no_seam_discontinuity_on_a_linear_ramp() {
        // Globally-linear ramp across a 2x2 FO4 cell block:
        // height(global_row, global_col) = BASE + STEP*(global_row+global_col).
        const BASE: f32 = 1000.0;
        const STEP: f32 = 8.0;
        let mut cells = Vec::new();
        for cy in 0..=1i32 {
            for cx in 0..=1i32 {
                let mut heights = vec![0f32; 33 * 33];
                for ly in 0..33usize {
                    for lx in 0..33usize {
                        let global_row = cy as usize * 32 + ly;
                        let global_col = cx as usize * 32 + lx;
                        heights[ly * 33 + lx] = BASE + STEP * (global_row + global_col) as f32;
                    }
                }
                cells.push((cx, cy, heights));
            }
        }
        let field = merge_land_cells(&cells).expect("merge");
        assert_eq!(field.width, 65);
        assert_eq!(field.height, 65);

        let analytic = |world_x: f64, world_y: f64| -> f32 {
            let gx = world_x / FO4_LAND_GRID_SPACING;
            let gy = world_y / FO4_LAND_GRID_SPACING;
            BASE + STEP * (gx + gy) as f32
        };

        // Positions straddling the seam at cell-x boundary (world x = 4096).
        for offset in [-64.0, -1.0, 0.0, 1.0, 64.0] {
            let wx = 4096.0 + offset;
            let wy = 2000.0;
            let sampled = field.sample_world(wx, wy);
            let expected = analytic(wx, wy);
            assert!(
                (sampled - expected).abs() < 1e-2,
                "seam sample at x={wx}: sampled={sampled} expected={expected}"
            );
        }

        // Continuity: no jump immediately either side of the seam.
        let just_before = field.sample_world(4095.9, 2000.0);
        let just_after = field.sample_world(4096.1, 2000.0);
        assert!(
            (just_before - just_after).abs() < 0.1,
            "discontinuity at seam: {just_before} vs {just_after}"
        );
    }

    // -- (c) N/extent computation ---------------------------------------------

    #[test]
    fn commonwealth_extent_matches_r4_measured_192_cell_span() {
        // R4 §7.6 / R1 §6: 192x192 FO4 cells -> N=114, res=14592.
        let total_span_fo4_units = 192.0 * FO4_CELL_UNITS;
        let half_span_m = (total_span_fo4_units * FO4_TO_SF_SPATIAL) / 2.0;
        let (n, res) = compute_grid_extent(half_span_m, None);
        assert_eq!(n, 114);
        assert_eq!(res, 14592);
    }

    #[test]
    fn crop_param_overrides_measured_extent() {
        let (n, res) = compute_grid_extent(999_999.0, Some(32));
        assert_eq!(n, 32);
        assert_eq!(res, 4096);
    }

    // -- (d) coverage-weighted slot selection ----------------------------------

    #[test]
    fn select_top_layers_keeps_six_highest_weight_of_seven() {
        let candidates: Vec<LtexCandidate> = (0..7)
            .map(|i| LtexCandidate {
                ltex_form_id: 100 + i,
                weight: (i as f64) + 1.0, // weights 1.0..7.0
            })
            .collect();
        let (selected, dropped) = select_top_layers(&candidates, 6);
        assert_eq!(selected.len(), 6);
        assert_eq!(dropped, 1);
        assert!(
            !selected.iter().any(|c| c.ltex_form_id == 100),
            "lowest-weight candidate (weight 1.0) must be dropped"
        );
        assert!(
            selected.iter().any(|c| c.ltex_form_id == 106),
            "highest-weight candidate (weight 7.0) must be kept"
        );
        assert_eq!(selected[0].ltex_form_id, 106, "sorted descending by weight");
    }

    // -- (e) SFBK field derivation ----------------------------------------------

    #[test]
    fn derive_sfbk_fields_matches_r1_newatlantis_worked_example() {
        // R1 §4.3 worked example: OverlayBlockNewAtlantis.
        let report = BtdWriteReport {
            res_x: 1792,
            res_y: 1792,
            height_min: 0.0,
            height_max: 260.502_04,
            cell_min_max: Vec::new(),
        };
        let fields = derive_sfbk_fields(&report);
        assert_eq!(fields.dnam, (14, 14));
        assert_eq!(fields.enam, (0.0, 260.502_04));
        assert_eq!(fields.fnam_len_bytes, 392);
    }

    // -- Step-1 fixture: build_btd_input end-to-end ------------------------------

    fn load_2x2_fixture() -> Vec<LandRecordJson> {
        let raw = include_str!("../test_fixtures/terrain_btd/land_cells_2x2.json");
        let doc: JsonValue = serde_json::from_str(raw).expect("fixture parses");
        let cells = doc
            .get("cells")
            .and_then(JsonValue::as_array)
            .expect("cells array");
        cells
            .iter()
            .map(|c| LandRecordJson {
                cell_x: c.get("cell_x").and_then(JsonValue::as_i64).unwrap() as i32,
                cell_y: c.get("cell_y").and_then(JsonValue::as_i64).unwrap() as i32,
                value: serde_json::json!({"vhgt": c["vhgt"], "quadrants": c["quadrants"]}),
            })
            .collect()
    }

    #[test]
    fn build_btd_input_ramp_fixture_heights_and_monotone_alpha_and_frame() {
        let lands = load_2x2_fixture();
        let params = TerrainBtdParams {
            worldspace_editor_id: "TestWorld".into(),
            // N=4 -> cell range [-2, 1] (cell_min = -(N>>1)), wide enough to
            // hold both the origin-anchored cell (0,0) and a cell overlapping
            // the far edge of the 2x2 FO4 LAND fixture (up to world FO4 8192
            // units =~ 117 m on each axis).
            terrain_extent_cells: Some(4),
        };
        let (input, report) = build_btd_input(&lands, &params).expect("build_btd_input");

        assert_eq!(input.cells_x, 4);
        assert_eq!(input.cells_y, 4);
        assert!(report.cells_written > 0);

        // Frame check (R4 §7.3, unshifted): SF cell (0,0) sample (0,0) sits at
        // world (0,0), which is the fixture's ramp anchor (base_units=125.0 ->
        // 1000.0 FO4 world units), converted to metres.
        let cell00 = input
            .cells
            .iter()
            .find(|c| c.cell_x == 0 && c.cell_y == 0)
            .expect("SF cell (0,0) present");
        let expected_origin_m = 1000.0 * FO4_TO_SF_SPATIAL as f32;
        assert!(
            (cell00.heights[0] - expected_origin_m).abs() < 0.05,
            "origin height: got {} expected {expected_origin_m}",
            cell00.heights[0]
        );

        // The far corner of the fixture's LAND coverage (world FO4 (8192,8192))
        // is globally linear too: height = 1000 + 8*((row)+(col)) world units,
        // row=col=64 there -> 1000 + 8*128 = 2024 world units.
        // Evaluate the analytic ramp at whatever exact world position the last
        // SF sample maps to (Catmull-Rom reproduces a linear function exactly
        // regardless of grid alignment).
        let far_cell = input.cells.iter().find(|c| c.cell_x == 1 && c.cell_y == 1);
        let far_cell = far_cell.expect("SF cell (1,1) present in the [-2,1] grid");
        // world position of far_cell's sample (0,0) = (100.0, 100.0) m -> FO4
        // units (100/FO4_TO_SF_SPATIAL, same on both axes). Global VHGT units
        // there = world_units/128.0 on each axis; analytic height (world
        // units) = 1000 + 8*(gx+gy), matching the fixture's construction.
        let world_fo4_units = 100.0 / FO4_TO_SF_SPATIAL;
        let g = world_fo4_units / FO4_LAND_GRID_SPACING;
        let expected_far_world = 1000.0 + 8.0 * (g + g);
        let expected_far_m = (expected_far_world * FO4_TO_SF_SPATIAL) as f32;
        assert!(
            (far_cell.heights[0] - expected_far_m).abs() < 0.05,
            "far cell (1,1) sample(0,0) height: got {} expected {expected_far_m}",
            far_cell.heights[0]
        );

        // Monotone alpha: cell (0,0) quadrant 0 carries the fixture's single
        // ATXT layer with a gradient increasing along local x. Its selected
        // blend slot is the only candidate, so it always lands at bit-group 0.
        // Sample along increasing x at a fixed y, but ONLY within the source
        // FO4 quadrant's own footprint (2048 FO4 units = ~29.26 m = ~37.4 SF
        // samples at 0.78125 m spacing) -- an SF quadrant is 50 m, wider than
        // one FO4 quadrant (R4 §7.4's "~2.92 FO4 quadrants per SF quadrant"),
        // so beyond that the SF quadrant span crosses into a neighbouring
        // FO4 quadrant with no layer of its own and legitimately drops to 0.
        let fo4_quadrant_span_samples =
            (2048.0 * FO4_TO_SF_SPATIAL / SF_SAMPLE_SPACING_M).floor() as usize;
        let mut prev = -1i32;
        let mut saw_increase = false;
        for sx in 0..fo4_quadrant_span_samples.min(64) {
            let word = cell00.quad_alphas[10 * SF_CELL_SAMPLES + sx];
            let group0 = (word & 0x7) as i32;
            if group0 < prev {
                panic!("alpha group0 decreased at sx={sx}: {group0} < {prev}");
            }
            if group0 > prev && prev >= 0 {
                saw_increase = true;
            }
            prev = group0;
        }
        assert!(
            saw_increase,
            "expected a monotone increase across the gradient"
        );
    }

    #[test]
    fn build_btd_input_rejects_empty_input() {
        let params = TerrainBtdParams {
            worldspace_editor_id: "Empty".into(),
            terrain_extent_cells: Some(2),
        };
        let err = match build_btd_input(&[], &params) {
            Err(e) => e,
            Ok(_) => panic!("must reject empty input"),
        };
        assert!(err.contains("no LAND cells"), "{err}");
    }

    #[test]
    fn remap_ltex_palette_resolves_mapped_ids_and_zeroes_the_rest() {
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
        use crate::sym::StringInterner;

        let interner = StringInterner::new();
        let source_plugin = interner.intern("Fallout4.esm");
        let output_plugin_name = "Fallout4_SF.esm".to_string();
        let target_master_names = vec!["Starfield.esm".to_string()];

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: output_plugin_name.clone(),
                target_master_names: target_master_names.clone(),
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        // Translated into the output plugin, and remapped onto a vanilla
        // Starfield master record — the two index cases the raw encoding has.
        mapper.add_mapping(
            FormKey {
                local: 0x00_2000,
                plugin: source_plugin,
            },
            FormKey {
                local: 0x00_0801,
                plugin: interner.intern(&output_plugin_name),
            },
        );
        mapper.add_mapping(
            FormKey {
                local: 0x00_3000,
                plugin: source_plugin,
            },
            FormKey {
                local: 0x01_1EAB,
                plugin: interner.intern("Starfield.esm"),
            },
        );

        let mut palette = vec![0x00_2000u32, 0x00_3000, 0x00_9999];
        let notes = remap_ltex_palette(
            &mut palette,
            source_plugin,
            &mapper,
            &output_plugin_name,
            &target_master_names,
        );

        // own_index == target_master_names.len() == 1.
        assert_eq!(palette, vec![0x0100_0801, 0x0001_1EAB, 0]);
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].contains("0X009999") || notes[0].contains("0x009999"),
            "{notes:?}"
        );
    }

    // -- Phase-level smoke test (thin-shell wiring) ------------------------------

    /// Live-verified LAND authoring dict shape for BTXT/ATXT/VTXT (see
    /// `land_record_to_json_contract`'s doc comment): dumped once via a
    /// throwaway plugin handle to pin the adapter's field-key assumptions.
    #[test]
    fn land_record_to_json_contract_parses_live_btxt_atxt_vtxt_shape() {
        let record = serde_json::json!({
            "form_id": "000500",
            "form_version": 131,
            "fields": [
                {"BTXT": {"Texture": {"reference": {"plugin": "Fallout4.esm", "object_id": "002000"}}, "Layer": -1}},
                {"ATXT": {"Texture": {"reference": {"plugin": "Fallout4.esm", "object_id": "003000"}}}},
                {"AlphaLayerData": [{"AlphaLayerDataPosition": 5, "AlphaLayerDataOpacity": 0.5}]}
            ]
        });
        let land = land_record_to_json_contract(0, 0, &record);
        let quadrants = land
            .value
            .get("quadrants")
            .and_then(JsonValue::as_array)
            .unwrap();
        assert_eq!(quadrants[0]["base_ltex"], serde_json::json!(0x2000));
        let layers = quadrants[0]["layers"].as_array().unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0]["ltex"], serde_json::json!(0x3000));
        let alpha = layers[0]["alpha"].as_array().unwrap();
        assert_eq!(alpha.len(), QUAD_ALPHA_SAMPLES * QUAD_ALPHA_SAMPLES);
        assert!((alpha[5].as_f64().unwrap() - 0.5).abs() < 1e-6);
    }

    /// Corpus-test idiom (mirrors `texture_engine/corpus_tests.rs`): resolve
    /// the real FO4 install from `.env`'s `FO4_DIR`, skip cleanly if it
    /// isn't configured or `Fallout4.esm` isn't there. Runs
    /// `land_record_to_json_contract` against one real Commonwealth LAND
    /// record; the test above only covers synthetic dumps.
    fn resolve_fo4_data_dir_for_test() -> Option<std::path::PathBuf> {
        let candidate_from_env = |root: &str| -> Option<std::path::PathBuf> {
            let data_dir = std::path::PathBuf::from(root.trim().trim_matches('"')).join("Data");
            data_dir.join("Fallout4.esm").is_file().then_some(data_dir)
        };
        if let Ok(root) = std::env::var("FO4_DIR") {
            if let Some(dir) = candidate_from_env(&root) {
                return Some(dir);
            }
        }
        let env_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../.env");
        let text = std::fs::read_to_string(&env_path).ok()?;
        for line in text.lines() {
            let line = line.trim();
            if let Some(root) = line.strip_prefix("FO4_DIR=") {
                if let Some(dir) = candidate_from_env(root) {
                    return Some(dir);
                }
            }
        }
        None
    }

    #[test]
    fn land_record_to_json_contract_matches_a_live_commonwealth_land_record() {
        let Some(fo4_data_dir) = resolve_fo4_data_dir_for_test() else {
            eprintln!(
                "skip: land_record_to_json_contract_matches_a_live_commonwealth_land_record \
                 -- FO4_DIR not configured (.env) or Fallout4.esm not present"
            );
            return;
        };
        let esm_path = fo4_data_dir.join("Fallout4.esm");

        let handle_id = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            &esm_path.to_string_lossy(),
            Some("fo4"),
            None,
            None,
            false,
        )
        .expect("load real Fallout4.esm");

        let ids_json = plugin_handle_collect_worldspace_terrain_ids_json(
            handle_id,
            "Commonwealth",
            0,
            0,
            -1,
            -1,
        )
        .expect("collect Commonwealth terrain ids");
        let ids: JsonValue = serde_json::from_str(&ids_json).expect("parse terrain ids json");
        let cells = ids
            .get("cells")
            .and_then(JsonValue::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(
            !cells.is_empty(),
            "Commonwealth worldspace has no LAND cells: {ids_json}"
        );

        let mut verified = 0usize;
        for cell in cells.iter().take(200) {
            let Some(land_form_id) = cell.get("land_form_id").and_then(JsonValue::as_u64) else {
                continue;
            };
            let x = cell.get("x").and_then(JsonValue::as_i64).unwrap_or(0) as i32;
            let y = cell.get("y").and_then(JsonValue::as_i64).unwrap_or(0) as i32;
            let form_key = format!("Fallout4.esm:{:06X}", (land_form_id as u32) & 0x00FF_FFFF);
            let Some(land_value) =
                plugin_handle_read_authoring_record_value_json(handle_id, &form_key)
                    .expect("read live LAND authoring json")
            else {
                continue;
            };

            let contract = land_record_to_json_contract(x, y, &land_value);

            let vhgt = contract.value.get("vhgt").expect("vhgt key present");
            let Some(deltas) = vhgt.get("deltas").and_then(JsonValue::as_array) else {
                continue;
            };
            assert_eq!(deltas.len(), 33, "VHGT 33 rows for live cell ({x},{y})");

            let quadrants = contract
                .value
                .get("quadrants")
                .and_then(JsonValue::as_array)
                .expect("quadrants array present");
            assert_eq!(quadrants.len(), 4, "4 quadrants for live cell ({x},{y})");
            let any_base_ltex = quadrants
                .iter()
                .any(|q| q.get("base_ltex").and_then(JsonValue::as_u64).unwrap_or(0) != 0);
            if !any_base_ltex {
                continue;
            }

            let heights = vhgt_heights_from_authoring(vhgt).expect("decode live VHGT heights");
            let min = heights.iter().cloned().fold(f32::INFINITY, f32::min);
            let max = heights.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            assert!(
                min.is_finite() && max.is_finite(),
                "finite height range for live cell ({x},{y}): min={min} max={max}"
            );
            // Commonwealth is a wasteland at modest elevation; any plausible
            // FO4 world-unit height (pre metre-scale) fits comfortably here.
            assert!(
                min > -50_000.0 && max < 50_000.0,
                "plausible FO4 world-unit height range for live cell ({x},{y}): min={min} max={max}"
            );

            verified += 1;
            break;
        }
        assert!(
            verified >= 1,
            "no Commonwealth LAND record with a resolvable BTXT+VHGT payload was found \
             among the first 200 cells to verify the adapter against"
        );
    }

    #[test]
    fn phase_run_writes_btd_and_reports_cells_written() {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_new_native,
        };
        use std::sync::atomic::AtomicBool;

        let source = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        let target = plugin_handle_new_native("Fallout4_SF.esm", Some("starfield")).unwrap();

        // Seed the source handle with 4 LAND cells + their CELL grid parents.
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&source).unwrap();
            let mut cell_children = Vec::new();
            for (cell_x, cell_y, ltex_hex) in [
                (0i32, 0i32, "00001000"),
                (1, 0, "00001000"),
                (0, 1, "00001000"),
                (1, 1, "00001000"),
            ] {
                let form_id = 0x0000_0100u32 + ((cell_x + 1) as u32) * 0x10 + (cell_y + 1) as u32;
                let land_form_id = form_id + 0x1000;
                let xclc = [cell_x.to_le_bytes(), cell_y.to_le_bytes()].concat();
                let cell_record = ParsedItem::Record(ParsedRecord {
                    signature: SmolStr::new("CELL"),
                    form_id,
                    flags: 0,
                    version_control: 0,
                    form_version: Some(131),
                    version2: Some(0),
                    subrecords: vec![sub("XCLC", xclc)],
                    raw_payload: None,
                    parse_error: None,
                });
                let mut deltas = vec![vec![0i64; 33]; 33];
                deltas[0][0] = 1;
                let vhgt_bytes = {
                    let mut raw = Vec::new();
                    raw.extend_from_slice(&100.0f32.to_le_bytes());
                    for row in &deltas {
                        for v in row {
                            raw.push(*v as i8 as u8);
                        }
                    }
                    raw.extend_from_slice(&[0, 0, 0]);
                    raw
                };
                let land_record = ParsedItem::Record(ParsedRecord {
                    signature: SmolStr::new("LAND"),
                    form_id: land_form_id,
                    flags: 0,
                    version_control: 0,
                    form_version: Some(131),
                    version2: Some(0),
                    subrecords: vec![sub("VHGT", vhgt_bytes)],
                    raw_payload: None,
                    parse_error: None,
                });
                let cell_child_group = ParsedItem::Group(ParsedGroup {
                    label: form_id.to_le_bytes(),
                    group_type: 6, // cell children
                    tail: Bytes::new(),
                    children: vec![land_record],
                });
                let _ = ltex_hex;
                // Exterior cells must NOT sit as direct children of the
                // world-children group -- `direct_world_persistent_cell_id`
                // (esp/src/cell_slice.rs) treats the FIRST direct CELL child
                // of world_children as the worldspace's persistent cell and
                // excludes it from terrain collection. Nest under a block
                // group (group_type 4, exterior cell block) as real FO4
                // plugins do.
                let exterior_block = ParsedItem::Group(ParsedGroup {
                    label: [0, 0, 0, 0],
                    group_type: 4,
                    tail: Bytes::new(),
                    children: vec![cell_record, cell_child_group],
                });
                cell_children.push(exterior_block);
            }
            let wrld_record = ParsedItem::Record(ParsedRecord {
                signature: SmolStr::new("WRLD"),
                form_id: 0x0000_0050,
                flags: 0,
                version_control: 0,
                form_version: Some(131),
                version2: Some(0),
                subrecords: vec![sub("EDID", zstring_bytes("TestWorld"))],
                raw_payload: None,
                parse_error: None,
            });
            let world_children = ParsedItem::Group(ParsedGroup {
                label: 0x0000_0050u32.to_le_bytes(),
                group_type: 1, // world children
                tail: Bytes::new(),
                children: cell_children,
            });
            slot.parsed.root_items = vec![ParsedItem::Group(ParsedGroup {
                label: *b"WRLD",
                group_type: TOP_GROUP_TYPE,
                tail: Bytes::new(),
                children: vec![wrld_record, world_children],
            })];
        }
        // Target needs a WRLD with the same editor id for overlay wiring.
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&target).unwrap();
            let wrld_record = ParsedItem::Record(ParsedRecord {
                signature: SmolStr::new("WRLD"),
                form_id: 0x0000_0050,
                flags: 0,
                version_control: 0,
                form_version: Some(581),
                version2: Some(0),
                subrecords: vec![sub("EDID", zstring_bytes("TestWorld"))],
                raw_payload: None,
                parse_error: None,
            });
            slot.parsed.root_items = vec![ParsedItem::Group(ParsedGroup {
                label: *b"WRLD",
                group_type: TOP_GROUP_TYPE,
                tail: Bytes::new(),
                children: vec![wrld_record],
            })];
        }

        let run_id = create_run(RunParams {
            source: Game::Fo4,
            target: Game::Starfield,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Fallout4_SF.esm".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let params =
            serde_json::json!({"worldspace_editor_id": "TestWorld", "terrain_extent_cells": 2});

        let report = with_run(run_id, |run| -> Result<PhaseReport, RunError> {
            // The phase resolves its LTEX palette through the mapper, so it is
            // only dispatchable after `translate` has seeded `mapper_state`.
            run.mapper_state = Some(crate::formkey_mapper::MapperState::new(
                std::iter::empty(),
                crate::formkey_mapper::MapperOptions {
                    output_plugin_name: "Fallout4_SF.esm".into(),
                    target_master_names: vec!["Starfield.esm".into()],
                    ..Default::default()
                },
            ));
            let cancel = AtomicBool::new(false);
            let source_dir = mod_path.clone();
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            TerrainBtdWritePhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 1);
        assert!(mod_path.join("data/terrain/TestWorld.btd").is_file());

        drop_run(run_id).unwrap();
        plugin_handle_close_native(source);
        plugin_handle_close_native(target);
    }
}
