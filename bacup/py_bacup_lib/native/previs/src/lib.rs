//! Standalone Fallout 4 previs generation reproducing Creation Kit 1.11.137.
//!
//! Scene construction, record ingestion and CELL metadata are independent
//! implementations of the clean-room evidence in
//! `tools/re/projects/ck_previs_precombine/`. There is no visibility solver
//! yet: previs stays out of the pipeline until a clean-room one exists.

pub mod assets;
pub mod cdx;
pub mod collision_scale;
pub mod combined;
pub mod combined_physics;
pub mod crc;
pub mod csg;
pub mod error;
pub mod f32ops;
pub mod generation;
pub mod group;
pub mod havok_mesh;
mod memory;
pub mod metadata;
pub mod nif;
pub mod plugin;
pub mod precombine;
pub mod precombine_generation;
pub mod precombine_stage;
pub mod previs;
pub mod psg;
pub mod python_api;
pub mod records;
pub mod scene;
pub mod shader;
pub mod stage;
pub mod swap;
pub mod tome;
pub mod umbra_build;
pub mod umbra_hierarchy;
pub mod umbra_query;
pub mod umbra_solve;
pub mod umbra_tome;
pub mod umbra_validate;

pub use python_api::register_module;

/// Half the logical CPUs: the stages share the machine with the game,
/// editors and other conversions, so they never default to every core.
pub fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get()).div_ceil(2).max(1)
}

/// A rayon pool of `workers` threads (default [`default_workers`]).
pub fn worker_pool(workers: Option<usize>) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers.filter(|&n| n > 0).unwrap_or_else(default_workers))
        .build()
        .expect("rayon pool")
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).iter().map(|b| format!("{b:02x}")).collect()
}
