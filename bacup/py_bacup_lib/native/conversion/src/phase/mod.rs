//! Phase trait + dispatcher — the seam that lets units add subsystems
//! without merge conflicts. See CLAUDE.md.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde_json::Value as JsonValue;
use thiserror::Error;

use crate::run::ConversionRun;

pub mod animations;
pub mod apply_registry_mappings;
pub mod btos;
pub mod build_esp;
pub mod copy_materialized_facegen;
pub mod copy_textures;
pub mod creatures;
pub mod drivers;
pub mod emit_modt_manifest;
pub mod equipment;
pub mod face;
pub mod fixups_v2;
pub mod fnv_legacy;
pub mod gamebryo_nifs;
pub mod graft_terrain;
pub mod havok;
pub mod havok_postprocess;
pub mod interior_cells;
pub mod lod_assets;
pub mod lod_paths;
pub mod materials;
pub mod materials_v2;
pub mod merge_sources;
pub mod mswp_material_paths;
pub mod nifs;
pub mod precombines;
pub mod progress;
pub mod projected_navi;
pub mod projected_navmeshes;
pub mod rebuild_cell_offsets;
pub mod record_translation;
pub mod regenerate_modt;
pub mod scaffold;
pub mod skeleton;
pub mod sounds;
pub mod story_manager;
pub mod synthesize_object_lod;
pub mod terrain;
pub mod textures;
pub mod textures_v2;
pub mod translate;
pub mod translate_v2;
pub mod walk;

// ---------------------------------------------------------------------------
// Types crossing the dispatcher boundary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub enum PhaseEvent {
    Started {
        phase: &'static str,
    },
    Progress {
        phase: &'static str,
        current: u32,
        total: u32,
        item: Option<String>,
    },
    Log {
        phase: &'static str,
        level: LogLevel,
        message: String,
    },
    Completed {
        phase: &'static str,
        report: PhaseReport,
    },
    /// Pipeline (stage-DAG) events — emitted by `crate::pipeline`.
    StageStarted {
        stage: &'static str,
    },
    StageCompleted {
        stage: &'static str,
        items_done: u64,
        items_failed: u64,
        elapsed_ms: u64,
    },
    StageFailed {
        stage: &'static str,
        message: String,
    },
}

#[derive(Debug, Default, Clone)]
pub struct PhaseReport {
    pub records_changed: u32,
    pub records_added: u32,
    pub records_vanilla_remapped: u32,
    pub records_dropped: u32,
    pub records_deferred: u32,
    pub assets_written: u32,
    pub warnings: u32,
    pub elapsed_ms: u64,
    /// Per-item failures isolated by the phase. Populated by the sink-wired
    /// asset phases; 0 elsewhere.
    pub items_failed: u32,
}

#[derive(Debug, Error)]
pub enum PhaseError {
    #[error("cancelled")]
    Cancelled,
    #[error("bad params: {0}")]
    BadParams(String),
    #[error("phase {0} not yet implemented")]
    NotImplemented(&'static str),
    #[error("{0}")]
    Internal(String),
}

pub struct PhaseCtx<'a> {
    pub run: &'a mut ConversionRun,
    pub mod_path: &'a Path,
    pub source_extracted_dir: &'a Path,
    pub target_extracted_dir: Option<&'a Path>,
    pub target_data_dir: Option<&'a Path>,
    pub params: &'a JsonValue,
    pub cancel: &'a AtomicBool,
}

impl<'a> PhaseCtx<'a> {
    pub fn check_cancel(&self) -> Result<(), PhaseError> {
        if self.cancel.load(Ordering::Relaxed) {
            Err(PhaseError::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub trait Phase: Send + Sync {
    fn name(&self) -> &'static str;
    fn requires_source_plugin(&self) -> bool {
        true
    }
    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError>;
}

struct SourceFreePhase<P>(P);

impl<P: Phase> Phase for SourceFreePhase<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn requires_source_plugin(&self) -> bool {
        false
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        self.0.run(ctx)
    }
}

fn source_free<P: Phase + 'static>(phase: P) -> Box<dyn Phase> {
    Box::new(SourceFreePhase(phase))
}

fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<unknown panic payload>".to_string()
    }
}

fn phase_panic_message(phase_name: &str, payload: &(dyn std::any::Any + Send)) -> String {
    let raw = panic_payload_message(payload);
    if raw.contains("FailedAllocation")
        || raw.contains("Failed to allocate memory")
        || raw.contains("Failed to get or intern string")
        || raw.contains("string interner allocation failed")
    {
        return format!(
            "phase {phase_name} ran out of memory while interning conversion strings; \
             reduce conversion_workers to 1 and close other memory-heavy apps. Details: {raw}"
        );
    }
    format!("phase {phase_name} panicked: {raw}")
}

// ---------------------------------------------------------------------------
// Stub registered for every future phase name. Replaced by real impls.
// ---------------------------------------------------------------------------

pub struct NotImplementedPhase {
    pub name: &'static str,
}

impl Phase for NotImplementedPhase {
    fn name(&self) -> &'static str {
        self.name
    }
    fn run(&self, _ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        Err(PhaseError::NotImplemented(self.name))
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub struct PhaseRegistry {
    inner: HashMap<&'static str, Box<dyn Phase>>,
}

impl PhaseRegistry {
    pub fn get(&self, name: &str) -> Option<&dyn Phase> {
        self.inner.get(name).map(|b| &**b)
    }

    pub fn names(&self) -> Vec<&'static str> {
        let mut v: Vec<_> = self.inner.keys().copied().collect();
        v.sort_unstable();
        v
    }
}

fn build_registry() -> PhaseRegistry {
    let mut inner: HashMap<&'static str, Box<dyn Phase>> = HashMap::new();

    inner.insert("translate", Box::new(translate::TranslatePhase));
    inner.insert("translate_v2", Box::new(translate_v2::TranslateV2Phase));
    inner.insert(
        "emit_story_manager_subset",
        Box::new(story_manager::EmitStoryManagerSubsetPhase),
    );
    inner.insert("fixups_v2", Box::new(fixups_v2::FixupsV2Phase));
    inner.insert("fnv_legacy", source_free(fnv_legacy::FnvLegacyPhase));
    inner.insert(
        "apply_registry_mappings",
        source_free(apply_registry_mappings::ApplyRegistryMappingsPhase),
    );

    inner.insert("convert_btos", source_free(btos::ConvertBtosPhase));
    inner.insert("convert_btos_v2", source_free(btos::ConvertBtosV2Phase));
    inner.insert("convert_nifs_v2", source_free(nifs::ConvertNifsV2Phase));
    inner.insert(
        "convert_gamebryo_nifs",
        source_free(gamebryo_nifs::ConvertGamebryoNifsPhase),
    );
    inner.insert("convert_terrain", Box::new(terrain::ConvertTerrainPhase));
    inner.insert(
        "prepare_graft_terrain",
        source_free(graft_terrain::PrepareGraftTerrainPhase),
    );
    inner.insert(
        "graft_terrain",
        source_free(graft_terrain::GraftTerrainPhase),
    );
    inner.insert(
        "emit_projected_navmeshes",
        Box::new(projected_navmeshes::EmitProjectedNavmeshesPhase),
    );
    inner.insert(
        "rebuild_projected_navi",
        Box::new(projected_navi::RebuildProjectedNaviPhase),
    );
    inner.insert(
        "convert_interior_cells",
        Box::new(interior_cells::ConvertInteriorCellsPhase),
    );
    inner.insert(
        "convert_materials_v2",
        source_free(materials_v2::ConvertMaterialsV2Phase),
    );
    inner.insert(
        "rewrite_mswp_material_paths",
        source_free(mswp_material_paths::RewriteMswpMaterialPathsPhase),
    );
    inner.insert("copy_sounds", source_free(sounds::CopySoundsPhase));
    inner.insert(
        "copy_textures",
        source_free(copy_textures::CopyTexturesPhase),
    );
    inner.insert(
        "copy_materialized_facegen",
        source_free(copy_materialized_facegen::CopyMaterializedFacegenPhase),
    );
    inner.insert(
        "merge_sources",
        source_free(merge_sources::MergeSourcesPhase),
    );
    inner.insert("convert_havok", source_free(havok::ConvertHavokPhase));
    inner.insert(
        "postprocess_havok_assets",
        source_free(havok_postprocess::PostprocessHavokAssetsPhase),
    );
    inner.insert(
        "synthesize_drivers",
        source_free(drivers::SynthesizeDriversPhase),
    );
    inner.insert(
        "synthesize_object_lod",
        Box::new(synthesize_object_lod::SynthesizeObjectLodPhase),
    );

    inner.insert("walk", Box::new(walk::WalkPhase));

    inner.insert(
        "convert_creatures",
        Box::new(creatures::ConvertCreaturesPhase),
    );
    inner.insert(
        "convert_textures_v2",
        source_free(textures_v2::ConvertTexturesV2Phase),
    );

    inner.insert(
        "record_translation_maps",
        source_free(record_translation::RecordTranslationMapsPhase),
    );

    inner.insert(
        "convert_equipment",
        Box::new(equipment::ConvertEquipmentPhase),
    );
    inner.insert("extract_atx", source_free(equipment::ExtractAtxPhase));

    inner.insert("convert_face", Box::new(face::ConvertFacePhase));
    inner.insert(
        "convert_animations",
        source_free(animations::ConvertAnimationsPhase),
    );

    inner.insert(
        "convert_skeleton",
        source_free(skeleton::ConvertSkeletonPhase),
    );

    inner.insert("scaffold", source_free(scaffold::ScaffoldPhase));
    inner.insert("build_esp", source_free(build_esp::BuildEspPhase));

    // Post-asset — MODT (re)population (Plan B). Runs after build_esp.
    inner.insert(
        "regenerate_modt",
        source_free(regenerate_modt::RegenerateModtPhase),
    );

    // MODT compute-manifest PRODUCER (Plan B). Emits the mesh->graph manifest
    // that `regenerate_modt` consumes. Runs after the asset waves.
    inner.insert(
        "emit_modt_manifest",
        source_free(emit_modt_manifest::EmitModtManifestPhase),
    );

    // CK-free precombine generation (v0 spike). Source-free: reads/writes
    // only the open target handle. Belongs beside the post-asset MODT phases.
    inner.insert(
        "generate_precombines",
        source_free(precombines::GeneratePrecombinesPhase),
    );

    // WRLD OFST/CLSZ cell seek tables. The tables encode the serialized file
    // layout, so this must be the LAST record mutation before the final save.
    inner.insert(
        "rebuild_cell_offsets",
        source_free(rebuild_cell_offsets::RebuildCellOffsetsPhase),
    );

    // Test-only phases (inert outside #[cfg(test)] builds).
    #[cfg(test)]
    {
        inner.insert(
            "test_handshake_left",
            Box::new(test_phases::HandshakeLeftPhase),
        );
        inner.insert(
            "test_handshake_right",
            Box::new(test_phases::HandshakeRightPhase),
        );
        inner.insert("test_event_burst", Box::new(test_phases::EventBurstPhase));
        inner.insert("test_noop", Box::new(test_phases::NoopPhase));
        inner.insert("test_panic", Box::new(test_phases::PanicPhase));
    }

    PhaseRegistry { inner }
}

#[cfg(test)]
pub(crate) mod test_phases {
    //! cfg(test)-only phases used by the per-run-lock concurrency tests
    //! and the pipeline plan tests.

    use std::sync::OnceLock;
    use std::time::Duration;

    use crossbeam_channel::{Receiver, Sender, bounded};

    use super::{Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

    pub(crate) struct HandshakeChans {
        pub left_to_right: (Sender<()>, Receiver<()>),
        pub right_to_left: (Sender<()>, Receiver<()>),
    }

    pub(crate) fn handshake_chans() -> &'static HandshakeChans {
        static CHANS: OnceLock<HandshakeChans> = OnceLock::new();
        CHANS.get_or_init(|| HandshakeChans {
            left_to_right: bounded(1),
            right_to_left: bounded(1),
        })
    }

    /// Sends to the right phase, then waits for the right phase's send.
    /// Only completes when both phases are in-flight at the same time.
    pub(crate) struct HandshakeLeftPhase;
    impl Phase for HandshakeLeftPhase {
        fn name(&self) -> &'static str {
            "test_handshake_left"
        }
        fn run(&self, _ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
            let c = handshake_chans();
            c.left_to_right
                .0
                .send(())
                .map_err(|_| PhaseError::Internal("handshake send failed".into()))?;
            c.right_to_left
                .1
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| PhaseError::Internal("no overlap".into()))?;
            Ok(PhaseReport::default())
        }
    }

    pub(crate) struct HandshakeRightPhase;
    impl Phase for HandshakeRightPhase {
        fn name(&self) -> &'static str {
            "test_handshake_right"
        }
        fn run(&self, _ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
            let c = handshake_chans();
            c.right_to_left
                .0
                .send(())
                .map_err(|_| PhaseError::Internal("handshake send failed".into()))?;
            c.left_to_right
                .1
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| PhaseError::Internal("no overlap".into()))?;
            Ok(PhaseReport::default())
        }
    }

    pub(crate) fn burst_release() -> &'static (Sender<()>, Receiver<()>) {
        static REL: OnceLock<(Sender<()>, Receiver<()>)> = OnceLock::new();
        REL.get_or_init(|| bounded(1))
    }

    /// Trivial phase for plan-DAG ordering tests.
    pub(crate) struct NoopPhase;
    impl Phase for NoopPhase {
        fn name(&self) -> &'static str {
            "test_noop"
        }
        fn run(&self, _ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
            Ok(PhaseReport::default())
        }
    }

    pub(crate) struct PanicPhase;
    impl Phase for PanicPhase {
        fn name(&self) -> &'static str {
            "test_panic"
        }
        fn run(&self, _ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
            panic!("intentional test panic");
        }
    }

    /// Sends 4 progress events then blocks until released — used to prove
    /// mid-phase event draining works without the run lock.
    pub(crate) struct EventBurstPhase;
    impl Phase for EventBurstPhase {
        fn name(&self) -> &'static str {
            "test_event_burst"
        }
        fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
            for i in 0..4u32 {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                    phase: "test_event_burst",
                    current: i,
                    total: 4,
                    item: None,
                });
            }
            burst_release()
                .1
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| PhaseError::Internal("never released".into()))?;
            Ok(PhaseReport::default())
        }
    }
}

pub fn registry() -> &'static PhaseRegistry {
    static REG: OnceLock<PhaseRegistry> = OnceLock::new();
    REG.get_or_init(build_registry)
}

// ---------------------------------------------------------------------------
// Dispatcher entry point — called by python_api::conversion_run_phase
// ---------------------------------------------------------------------------

pub struct DispatchParams {
    pub mod_path: PathBuf,
    pub source_extracted_dir: PathBuf,
    pub target_extracted_dir: Option<PathBuf>,
    pub target_data_dir: Option<PathBuf>,
    pub params: JsonValue,
}

pub fn run_phase(
    run_id: u64,
    name: &str,
    params: DispatchParams,
) -> Result<PhaseReport, PhaseError> {
    let phase = registry()
        .get(name)
        .ok_or_else(|| PhaseError::BadParams(format!("unknown phase: {name}")))?;

    // Per-run slot: phases on distinct runs run concurrently, and the run's
    // own cancel flag reaches PhaseCtx (conversion_run_cancel cancels
    // running phases).
    let slot = crate::run::run_slot(run_id)
        .map_err(|e| PhaseError::Internal(format!("run registry: {e}")))?;
    let started = Instant::now();

    // Run the phase body while holding a mutable borrow of THIS run only.
    let phase_result: Result<PhaseReport, PhaseError> = {
        let mut run = slot
            .run
            .lock()
            .map_err(|_| PhaseError::Internal("run lock poisoned".into()))?;
        if phase.requires_source_plugin() && run.source_handle().is_none() {
            return Err(PhaseError::Internal(
                "this phase requires a source plugin".into(),
            ));
        }
        let mut ctx = PhaseCtx {
            run: &mut run,
            mod_path: &params.mod_path,
            source_extracted_dir: &params.source_extracted_dir,
            target_extracted_dir: params.target_extracted_dir.as_deref(),
            target_data_dir: params.target_data_dir.as_deref(),
            params: &params.params,
            cancel: &slot.cancel,
        };
        match catch_unwind(AssertUnwindSafe(|| phase.run(&mut ctx))) {
            Ok(result) => result,
            Err(payload) => Err(PhaseError::Internal(phase_panic_message(
                phase.name(),
                payload.as_ref(),
            ))),
        }
    };

    let mut report = phase_result?;
    report.elapsed_ms = started.elapsed().as_millis() as u64;

    // Push the completion event through the channel (no run lock needed).
    let _ = slot.event_tx.try_send(PhaseEvent::Completed {
        phase: phase.name(),
        report: report.clone(),
    });

    Ok(report)
}

#[cfg(test)]
pub(crate) mod dispatcher_tests {
    use super::*;
    use crate::run::{RunConfig, RunParams, create_run, drop_run};
    use crate::translator::Game;

    pub(crate) fn test_run() -> u64 {
        create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .expect("create_run")
    }

    pub(crate) fn test_dispatch() -> DispatchParams {
        DispatchParams {
            mod_path: PathBuf::new(),
            source_extracted_dir: PathBuf::new(),
            target_extracted_dir: None,
            target_data_dir: None,
            params: serde_json::json!({}),
        }
    }

    #[test]
    fn legacy_fixups_phase_is_not_registered() {
        let names = registry().names();
        assert!(!names.contains(&"fixups"));
        assert!(names.contains(&"fixups_v2"));
    }

    #[test]
    fn registry_marks_source_required_and_source_free_phases() {
        for name in ["translate_v2", "walk", "convert_terrain"] {
            assert!(
                registry().get(name).unwrap().requires_source_plugin(),
                "{name}"
            );
        }
        for name in ["record_translation_maps", "convert_nifs_v2", "build_esp"] {
            assert!(
                !registry().get(name).unwrap().requires_source_plugin(),
                "{name}"
            );
        }
    }

    /// Two phases on two distinct runs must be able to execute concurrently.
    /// Under a whole-registry lock the second run_phase blocks until the first
    /// finishes, the handshake recv_timeout fires, and the phases error out.
    #[test]
    fn phases_on_distinct_runs_run_concurrently() {
        let id_a = test_run();
        let id_b = test_run();

        let ja =
            std::thread::spawn(move || run_phase(id_a, "test_handshake_left", test_dispatch()));
        let jb =
            std::thread::spawn(move || run_phase(id_b, "test_handshake_right", test_dispatch()));
        let ra = ja.join().expect("left thread");
        let rb = jb.join().expect("right thread");

        assert!(ra.is_ok(), "left phase: {ra:?}");
        assert!(rb.is_ok(), "right phase: {rb:?}");

        drop_run(id_a).unwrap();
        drop_run(id_b).unwrap();
    }

    /// Draining events from a run must work WHILE a phase on that run is
    /// mid-flight (the receiver lives outside the run lock).
    #[test]
    fn drain_during_phase_does_not_block() {
        let id = test_run();

        let j = std::thread::spawn(move || run_phase(id, "test_event_burst", test_dispatch()));

        // Drain WHILE the phase is blocked inside its body.
        let slot = crate::run::run_slot(id).expect("run slot");
        let mut got = 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while got < 4 && std::time::Instant::now() < deadline {
            match slot.events.try_recv() {
                Ok(PhaseEvent::Progress { phase, .. }) => {
                    assert_eq!(phase, "test_event_burst");
                    got += 1;
                }
                Ok(_) => {}
                Err(_) => std::thread::yield_now(),
            }
        }
        assert_eq!(got, 4, "expected 4 events drained mid-phase");

        // Release the phase and let it finish.
        test_phases::burst_release().0.send(()).unwrap();
        let r = j.join().expect("phase thread");
        assert!(r.is_ok(), "burst phase: {r:?}");

        drop_run(id).unwrap();
    }

    #[test]
    fn phase_panic_does_not_poison_run_lock() {
        let id = test_run();

        let err = run_phase(id, "test_panic", test_dispatch()).expect_err("panic phase errors");
        let msg = err.to_string();
        assert!(
            msg.contains("phase test_panic panicked: intentional test panic"),
            "unexpected panic error: {msg}"
        );

        let recovered = run_phase(id, "test_noop", test_dispatch());
        assert!(
            recovered.is_ok(),
            "run lock should remain usable: {recovered:?}"
        );

        drop_run(id).unwrap();
    }
}
