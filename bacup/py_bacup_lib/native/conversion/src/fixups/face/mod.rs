//! Face/race fixups for the FO76→FO4 conversion pipeline.
//!
//! - `generate_additive_races` (top-level Fixup, registered)
//! - `build_additive_race_record` (helper, not registered)
//! - `load_target_race_template` (helper, not registered)
//! - `derive_pa_subgraph_blocks` (helper, not registered)
//! - `rewrite_subgraph_block_formkeys` (helper, not registered)
//! - `filter_lchar_template_npcs` (top-level Fixup, registered)
//! - `inject_human_npc_head_parts` (top-level Fixup, registered)
//! - `strip_unbaked_human_npc_face_morphs` (top-level Fixup, registered)
//! - `strip_invalid_npc_face_morphs` (top-level Fixup, registered)
pub mod build_additive_race_record;
pub mod derive_pa_subgraph_blocks;
pub mod filter_lchar_template_npcs;
pub mod generate_additive_races;
pub mod inject_human_npc_head_parts;
pub mod load_target_race_template;
pub mod rewrite_subgraph_block_formkeys;
pub mod strip_invalid_npc_face_morphs;
pub mod strip_unbaked_human_npc_face_morphs;

// Re-export the canonical block representation at module-level so callers
// outside this directory can import it from a single path.
pub use build_additive_race_record::SubgraphBlock;
