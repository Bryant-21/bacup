//! Whole-plugin previs planning: every exterior cluster and interior CELL.

use std::path::{Path, PathBuf};
use std::time::Instant;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::assets::AssetResolver;
use crate::error::{PrevisError, Result};
use crate::metadata::CellMetadata;
use crate::plugin::LoadOrder;
use crate::precombine_stage::XcriEntry;
use crate::previs::{self, ClusterPlan, Context, ModelCache, PlannedReference};
use crate::records::{self, ReferencePolicy};
use crate::scene::{SceneBuilder, SceneSink, TeeSink, exterior_volume};
use crate::sha256_hex;
use crate::umbra_solve::{Aabb, Occupancy, OccupancySink, OccupancyTiming, Reach, SolveParams, SolveTiming, Solved, scene_volume, solve_occupancy_profiled};

#[derive(Debug, Deserialize)]
pub struct PlanRequest {
    pub plugin: PathBuf,
    /// Directories searched for the plugin's masters, in priority order.
    pub master_dirs: Vec<PathBuf>,
    /// Loose asset roots (each containing `meshes\`, `materials\`), highest priority first.
    pub loose_roots: Vec<PathBuf>,
    pub archives: Vec<PathBuf>,
    /// Where scene files go: every planned scene without `solve_into`, and
    /// with it only when `keep_scenes` asks for them (for debugging).
    pub scene_dir: PathBuf,
    /// World EditorIDs to process; all worlds when empty.
    #[serde(default)]
    pub worlds: Vec<String>,
    #[serde(default)]
    pub interiors: bool,
    #[serde(default)]
    pub accept_reference_layer: bool,
    /// When set, each cluster is solved as it is planned and its tome written
    /// to `<solve_into>\<uvd_relative>`. The planner streams its geometry
    /// straight into the solve's voxel occupancy; no scene is built.
    #[serde(default)]
    pub solve_into: Option<PathBuf>,
    /// With `solve_into`, also build and write each scene file.
    #[serde(default)]
    pub keep_scenes: bool,
    /// Leave exterior CELLs out of the index when the stage rewrites it
    /// (`precombine_stage::write_index`).
    #[serde(default)]
    pub interior_cdx_only: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CellEntry {
    pub cell_form_id: u32,
    #[serde(default = "yes")]
    pub previs: bool,
    #[serde(default)]
    pub root_visibility_cell: Option<u32>,
    #[serde(default)]
    pub previs_reference_ids: Vec<u32>,
    #[serde(default)]
    pub precombine: bool,
    #[serde(default)]
    pub xcri: Option<XcriEntry>,
}

#[derive(Debug, Serialize)]
pub struct Job {
    pub root_cell: u32,
    pub world: Option<String>,
    pub grid: Option<(i32, i32)>,
    pub scene_name: String,
    /// Empty when no scene file was written (see `PlanRequest::keep_scenes`).
    pub scene_path: PathBuf,
    pub scene_sha256: String,
    pub uvd_relative: String,
    pub task_bounds: Option<[f32; 6]>,
    pub cells: Vec<CellEntry>,
    pub references: Vec<PlannedReference>,
    /// Empty for CK byte-identical scenes; otherwise why this one is not
    /// (see `previs::NonParity`).
    pub non_parity: Vec<String>,
    /// The tome is already written (`PlanRequest::solve_into`).
    pub solved: bool,
}

#[derive(Debug, Serialize)]
pub struct Stamp {
    pub root_cell: u32,
    pub world: String,
    pub grid: (i32, i32),
    pub cells: Vec<CellEntry>,
}

#[derive(Debug, Serialize)]
pub struct Skip {
    pub world: Option<String>,
    pub grid: Option<(i32, i32)>,
    pub cell: Option<u32>,
    /// `true` for inputs outside the verified subset; `false` for malformed data.
    pub unsupported: bool,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct PlanReport {
    pub plugin_name: String,
    pub jobs: Vec<Job>,
    pub stamps: Vec<Stamp>,
    pub skipped: Vec<Skip>,
    #[serde(skip)]
    pub timing: PlanTiming,
}

#[derive(Debug, Default, Serialize)]
pub struct PlanTiming {
    pub load_seconds: f64,
    pub assets_seconds: f64,
    pub enumerate_seconds: f64,
    pub clusters_wall_seconds: f64,
    pub workers: usize,
    pub clusters: Vec<ClusterTiming>,
    pub summaries: Vec<ClusterSummary>,
}

#[derive(Debug, Serialize)]
pub struct ClusterSummary {
    pub kind: &'static str,
    pub count: usize,
    pub cluster_elapsed_sum_seconds: f64,
    pub prepare_sum_seconds: f64,
    pub solve_sum_seconds: f64,
    pub p50_seconds: f64,
    pub p95_seconds: f64,
    pub max_seconds: f64,
    pub slowest_cells: Vec<Option<u32>>,
}

impl PlanTiming {
    fn summarize(&mut self) {
        self.summaries = [("interior", false), ("exterior", true)]
            .into_iter()
            .map(|(kind, exterior)| {
                let mut clusters: Vec<_> = self.clusters.iter().filter(|c| c.world.is_some() == exterior).collect();
                clusters.sort_by(|a, b| a.total_seconds.total_cmp(&b.total_seconds));
                let percentile = |percent: usize| {
                    clusters.get((clusters.len() * percent).div_ceil(100).saturating_sub(1)).map_or(0.0, |c| c.total_seconds)
                };
                ClusterSummary {
                    kind,
                    count: clusters.len(),
                    cluster_elapsed_sum_seconds: clusters.iter().map(|c| c.total_seconds).sum(),
                    prepare_sum_seconds: clusters.iter().map(|c| c.prepare_seconds).sum(),
                    solve_sum_seconds: clusters.iter().map(|c| c.solve_seconds).sum(),
                    p50_seconds: percentile(50),
                    p95_seconds: percentile(95),
                    max_seconds: percentile(100),
                    slowest_cells: clusters.iter().rev().take(10).map(|c| c.cell).collect(),
                }
            })
            .collect();
    }
}

#[derive(Debug, Default, Serialize)]
pub struct ClusterTiming {
    pub outcome: &'static str,
    pub world: Option<String>,
    pub grid: Option<(i32, i32)>,
    pub cell: Option<u32>,
    pub total_seconds: f64,
    pub prepare_seconds: f64,
    pub camera_seconds: f64,
    pub occupancy: OccupancyTiming,
    pub scene_seconds: f64,
    pub solve_seconds: f64,
    pub build_seconds: f64,
    pub validate_seconds: f64,
    pub encode_seconds: f64,
    pub write_seconds: f64,
    pub uvd_bytes: usize,
    pub solver: SolveTiming,
}

enum Tome {
    Written,
    NoViewVolume,
}

/// Solves an occupancy in its planned view volume and writes the validated
/// tome to `<root>\<uvd>`.
fn solve_tome(root: &Path, uvd: &str, occupancy: Occupancy, volume: &[f32; 12], params: &SolveParams, reach: &Reach, timing: &mut ClusterTiming) -> Result<Tome> {
    let started = Instant::now();
    let (solved, solver) = solve_occupancy_profiled(occupancy, scene_volume(std::slice::from_ref(volume)).unwrap(), params, reach);
    timing.solve_seconds = started.elapsed().as_secs_f64();
    timing.solver = solver;
    let model = match solved {
        Solved::Model(model) => model,
        Solved::NoViewVolume => return Ok(Tome::NoViewVolume),
    };
    let started = Instant::now();
    let tome = crate::umbra_build::build_from_model(&model)?;
    drop(model);
    timing.build_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    crate::umbra_validate::validate(&tome)?;
    timing.validate_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let bytes = crate::umbra_tome::write(&tome);
    timing.encode_seconds = started.elapsed().as_secs_f64();
    timing.uvd_bytes = bytes.len();
    let started = Instant::now();
    let path = root.join(uvd.replace('\\', std::path::MAIN_SEPARATOR_STR));
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, bytes))
        .map_err(|e| PrevisError::io(path.display().to_string(), e))?;
    timing.write_seconds = started.elapsed().as_secs_f64();
    Ok(Tome::Written)
}

pub fn uvd_relative(plugin_name: &str, root_cell: u32) -> String {
    format!("Vis\\{plugin_name}\\{:08X}.uvd", root_cell & 0x00FF_FFFF)
}

fn cell_entries(cells: &[(u32, CellMetadata)]) -> Vec<CellEntry> {
    cells
        .iter()
        .map(|(cell, metadata)| CellEntry {
            cell_form_id: *cell,
            previs: metadata.previs,
            root_visibility_cell: metadata.root_visibility_cell,
            previs_reference_ids: metadata.previs_reference_ids.clone(),
            precombine: metadata.precombine,
            xcri: metadata.xcri.as_ref().map(|x| XcriEntry {
                mesh_ids: x.mesh_ids.clone(),
                reference_mesh_pairs: x.reference_mesh_pairs.clone(),
            }),
        })
        .collect()
}

/// Root of the 3x3 cluster containing a CELL grid coordinate.
fn cluster_root(value: i32) -> i32 {
    3 * (value + 1).div_euclid(3)
}

enum Target {
    Exterior { world: u32, world_name: String, x: i32, y: i32 },
    Interior { cell: u32 },
}

pub fn plan(request: &PlanRequest) -> Result<PlanReport> {
    plan_with_progress(request, &|_, _| {})
}

/// `progress(done, total)` runs on the worker that finished each cluster.
pub fn plan_with_progress(request: &PlanRequest, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<PlanReport> {
    let started = Instant::now();
    let load_order = LoadOrder::open(&request.plugin, &request.master_dirs)?;
    let mut timing = PlanTiming { load_seconds: started.elapsed().as_secs_f64(), workers: rayon::current_num_threads(), ..PlanTiming::default() };
    let started = Instant::now();
    let assets = AssetResolver::new(request.loose_roots.clone(), &request.archives)?;
    timing.assets_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let models = ModelCache::default();
    let context = Context {
        load_order: &load_order,
        assets: &assets,
        models: &models,
        policy: ReferencePolicy {
            accept_layer: request.accept_reference_layer,
        },
    };
    let plugin = load_order.target();
    if request.solve_into.is_none() || request.keep_scenes {
        std::fs::create_dir_all(&request.scene_dir)
            .map_err(|e| PrevisError::io(request.scene_dir.display().to_string(), e))?;
    }

    let mut targets = Vec::new();
    let wanted: Vec<String> = request.worlds.iter().map(|w| w.to_ascii_lowercase()).collect();
    for world_record in plugin.records_of(b"WRLD") {
        let editor_id = plugin
            .subrecords_at(&world_record)?
            .first(b"EDID")
            .map(crate::plugin::zstring)
            .unwrap_or_default();
        if !wanted.is_empty() && !wanted.contains(&editor_id.to_ascii_lowercase()) {
            continue;
        }
        let world = match records::read_world(plugin, &world_record) {
            Ok(world) => world,
            Err(error) => {
                targets.push(Err((Some(editor_id), error.to_string())));
                continue;
            }
        };
        let mut roots = std::collections::BTreeSet::new();
        for &cell in plugin.world_cells.get(&world_record.form_id).map(Vec::as_slice).unwrap_or(&[]) {
            let record = plugin.record(cell).unwrap();
            if let Ok(records::Cell { grid: Some((x, y)), .. }) = records::read_cell(plugin, &record) {
                roots.insert((cluster_root(x), cluster_root(y)));
            }
        }
        for (x, y) in roots {
            targets.push(Ok(Target::Exterior {
                world: world_record.form_id,
                world_name: world.editor_id.clone(),
                x,
                y,
            }));
        }
    }
    if request.interiors {
        for cell in plugin.records_of(b"CELL") {
            let is_interior = plugin.cells.get(&cell.form_id).is_none_or(|t| t.world.is_none())
                && records::read_cell(plugin, &cell).is_ok_and(|c| c.interior);
            if is_interior {
                targets.push(Ok(Target::Interior { cell: cell.form_id }));
            }
        }
    }

    timing.enumerate_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let total = targets.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let outcomes: Vec<(Outcome, ClusterTiming)> = targets
        .into_par_iter()
        .map(|target| {
            let started = Instant::now();
            let mut timing = ClusterTiming::default();
            match &target {
                Ok(Target::Exterior { world_name, x, y, .. }) => {
                    timing.world = Some(world_name.clone());
                    timing.grid = Some((*x, *y));
                }
                Ok(Target::Interior { cell }) => timing.cell = Some(*cell),
                Err((world, _)) => timing.world = world.clone(),
            }
            let outcome = match target {
                Err((world, reason)) => Outcome::Skip(Skip {
                    world,
                    grid: None,
                    cell: None,
                    unsupported: true,
                    reason,
                }),
                Ok(Target::Exterior { world, world_name, x, y }) => {
                    let volume = exterior_volume(x, y, 0.0);
                    let footprint = Aabb {
                        min: [volume[0], volume[1], f32::NEG_INFINITY],
                        max: [volume[3], volume[4], f32::INFINITY],
                    };
                    let mut sinks = Sinks::new(request, Some(footprint), Reach { sky: true, points: Vec::new() });
                    match sinks.plan(request, |sink| previs::plan_exterior_cluster(&context, world, x, y, sink)) {
                        Ok(plan) => {
                            timing.prepare_seconds = started.elapsed().as_secs_f64();
                            finish(request, &plugin.name, Some(world_name), plan, sinks, &mut timing)
                        }
                        Err(error) => Outcome::Skip(Skip {
                            world: Some(world_name),
                            grid: Some((x, y)),
                            cell: None,
                            unsupported: error.is_unsupported(),
                            reason: error.to_string(),
                        }),
                    }
                }
                Ok(Target::Interior { cell }) => {
                    let camera = previs::interior_camera(plugin, cell);
                    timing.camera_seconds = started.elapsed().as_secs_f64();
                    let planned = camera.and_then(|camera| {
                        let mut sinks = Sinks::new(request, camera.region, Reach { sky: false, points: camera.seeds });
                        let plan = sinks.plan(request, |sink| previs::plan_interior_cell(&context, cell, sink))?;
                        Ok((plan, sinks))
                    });
                    match planned {
                        Ok((plan, sinks)) => {
                            timing.prepare_seconds = started.elapsed().as_secs_f64();
                            finish(request, &plugin.name, None, plan, sinks, &mut timing)
                        }
                        Err(error) => Outcome::Skip(Skip {
                            world: None,
                            grid: None,
                            cell: Some(cell),
                            unsupported: error.is_unsupported(),
                            reason: error.to_string(),
                        }),
                    }
                }
            };
            timing.outcome = match &outcome {
                Outcome::Job(job) => if job.solved { "solved" } else { "planned" },
                Outcome::Stamp(_) => "stamp_only",
                Outcome::Skip(skip) => if skip.unsupported { "unsupported" } else { "failed" },
                Outcome::Nothing => "nothing",
            };
            timing.total_seconds = started.elapsed().as_secs_f64();
            if timing.prepare_seconds == 0.0 {
                timing.prepare_seconds = timing.total_seconds;
            }
            progress(done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1, total);
            (outcome, timing)
        })
        .collect();
    timing.clusters_wall_seconds = started.elapsed().as_secs_f64();

    let mut report = PlanReport {
        plugin_name: plugin.name.clone(),
        jobs: Vec::new(),
        stamps: Vec::new(),
        skipped: Vec::new(),
        timing,
    };
    for (outcome, timing) in outcomes {
        report.timing.clusters.push(timing);
        match outcome {
            Outcome::Job(job) => report.jobs.push(job),
            Outcome::Stamp(stamp) => report.stamps.push(stamp),
            Outcome::Skip(skip) => report.skipped.push(skip),
            Outcome::Nothing => {}
        }
    }
    report.timing.summarize();
    Ok(report)
}

enum Outcome {
    Job(Job),
    Stamp(Stamp),
    Skip(Skip),
    Nothing,
}

/// Where one cluster's planned geometry goes: the solve's occupancy, a
/// scene, or both.
struct Sinks {
    params: SolveParams,
    occupancy: OccupancySink,
    scene: SceneBuilder,
    reach: Reach,
}

impl Sinks {
    /// `footprint` limits the occupancy to an exterior cluster's CELLs or an
    /// interior's camera region.
    fn new(request: &PlanRequest, footprint: Option<Aabb>, reach: Reach) -> Self {
        let params = SolveParams::default();
        let occupancy = OccupancySink::new(&params, footprint.filter(|_| request.solve_into.is_some()));
        Sinks { params, occupancy, scene: SceneBuilder::default(), reach }
    }

    fn plan(&mut self, request: &PlanRequest, planner: impl FnOnce(&mut dyn SceneSink) -> Result<ClusterPlan>) -> Result<ClusterPlan> {
        match (&request.solve_into, request.keep_scenes) {
            (Some(_), false) => planner(&mut self.occupancy),
            (Some(_), true) => planner(&mut TeeSink(&mut self.occupancy, &mut self.scene)),
            (None, _) => planner(&mut self.scene),
        }
    }
}

fn finish(request: &PlanRequest, plugin_name: &str, world: Option<String>, plan: ClusterPlan, sinks: Sinks, timing: &mut ClusterTiming) -> Outcome {
    timing.cell = Some(plan.root_cell);
    timing.occupancy = sinks.occupancy.timing();
    let Some(volume) = &plan.volume else {
        return match (plan.cells.is_empty(), world, plan.root_grid) {
            (false, Some(world), Some(grid)) => Outcome::Stamp(Stamp {
                root_cell: plan.root_cell,
                world,
                grid,
                cells: cell_entries(&plan.cells),
            }),
            _ => Outcome::Nothing,
        };
    };
    let uvd = uvd_relative(plugin_name, plan.root_cell);
    let skip = |world: Option<String>, unsupported: bool, reason: String| {
        Outcome::Skip(Skip { world, grid: plan.root_grid, cell: Some(plan.root_cell), unsupported, reason })
    };
    let started = Instant::now();
    let (scene_path, scene_sha256) = if request.solve_into.is_none() || request.keep_scenes {
        let bytes = sinks.scene.finish(*volume).encode();
        let path = request.scene_dir.join(format!("{:08X}.scene", plan.root_cell));
        if let Err(error) = write_scene(&path, &bytes) {
            return skip(world, false, error.to_string());
        }
        (path, sha256_hex(&bytes))
    } else {
        (PathBuf::new(), String::new())
    };
    timing.scene_seconds = started.elapsed().as_secs_f64();
    let solved = match &request.solve_into {
        Some(root) => match solve_tome(root, &uvd, sinks.occupancy.finish(), volume, &sinks.params, &sinks.reach, timing) {
            Ok(Tome::Written) => true,
            Ok(Tome::NoViewVolume) => return skip(world, true, "scene has no view volume".to_string()),
            Err(error) => return skip(world, false, format!("solve failed: {error}")),
        },
        None => false,
    };
    Outcome::Job(Job {
        root_cell: plan.root_cell,
        world,
        grid: plan.root_grid,
        scene_name: plan.scene_name,
        scene_path,
        scene_sha256,
        uvd_relative: uvd,
        task_bounds: plan.task_bounds,
        cells: cell_entries(&plan.cells),
        references: plan.references,
        non_parity: plan.non_parity,
        solved,
    })
}

fn write_scene(path: &Path, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| PrevisError::io(path.display().to_string(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cluster_roots_centre_on_multiples_of_three() {
        assert_eq!([-4, -3, -2, -1, 0, 1, 2, 3, 4].map(cluster_root), [-3, -3, -3, 0, 0, 0, 3, 3, 3]);
    }

    #[test]
    fn uvd_path_uses_the_plugin_local_object_id() {
        assert_eq!(uvd_relative("Fallout4.esm", 0x0006_B1CC), "Vis\\Fallout4.esm\\0006B1CC.uvd");
        assert_eq!(uvd_relative("Test.esp", 0x0100_0800), "Vis\\Test.esp\\00000800.uvd");
    }
}
