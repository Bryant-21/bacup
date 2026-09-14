//! Havok behavior-graph and animation fixups.
//!
//! These fixups operate on `.hkx` files on disk (not on plugin records).
//! They walk directories under `ctx.mod_path/meshes/` and mutate HKX files
//! in place using the `havok_native` crate.
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `normalize_spaced_asset_names` | Rename `stem .ext` typo files + patch project refs |
//! | `normalize_weapon_behavior_contracts` | Extend custom actor roots with FO4 weapon graph names |
//! | `normalize_character_properties` | Declare FO4's shared character properties on character.hkx |
//! | `declare_engine_driven_locomotion` | Clear `bGraphDriven` on creatures whose locomotion clips animate in place |
//! | `declare_locomotion_run_threshold` | Declare `fSpeedRun` on root graphs that carry only `fSpeedWalk` |
//! | `synthesize_weapon_animation_aliases` | Supply FO4 fire, MT, and melee clip names for FO76 animations |
//! | `copy_character_support_files` | Port creature `*.ssf` + `bonelodsetting.txt` from source |
//! | `strip_source_game_events` | Drop FO76-only annotation events from animations |
//! | `inject_hitframe_events`   | Inject missing HitFrame into attack animations |
//! | `inject_attack_stop_events` | Inject missing attackStop exit events into attack animations |
//! | `retime_reload_complete_events` | Keep short reload clips inside FO4's reload state |
//! | `filter_unreferenced_behaviors` | Remove FO76-only generic behavior files |
//! | `inject_animation_names`   | Populate character.hkx assetNames list |
//! | `fix_character_rig_path`   | Rewrite FO76 skeleton paths to FO4 layout |
//! | `fix_subcreature_skeleton_paths` | Rewrite RACE skeletal model paths |
//! | `collect_behavior_clip_names` | Free function: collect hkbClipGenerator names |
//! | `anim_text_data_emit` | Decode RACE/IDLE records and drive `ck_native` AnimTextData generation |
//! | `repair_weapon_charge_reference_frames` | Add the neutral extracted-motion frame required by FO4 charge holds |
//! | `sanitize_ragdoll_contact_bones` | Remove contact-list indices outside the target ragdoll |
//! | `normalize_creature_death_phase` | Replace FO76 death gates with FO4's active-modifier phase |
//! | `wire_honeybeast_swarm_death` | Route FO4 ragdoll events to the bee swarm death sequence |

pub mod anim_text_data_emit;
pub mod collect_behavior_clip_names;
pub mod copy_character_support_files;
pub mod declare_engine_driven_locomotion;
pub mod declare_locomotion_run_threshold;
pub mod filter_unreferenced_behaviors;
pub mod fix_character_rig_path;
pub mod fix_subcreature_skeleton_paths;
pub mod inject_animation_names;
pub mod inject_attack_stop_events;
pub mod inject_hitframe_events;
pub mod normalize_character_properties;
pub mod normalize_creature_death_phase;
pub mod normalize_spaced_asset_names;
pub mod normalize_weapon_behavior_contracts;
mod postprocess_scan;
pub mod repair_weapon_charge_reference_frames;
pub mod retime_reload_complete_events;
pub mod sanitize_ragdoll_contact_bones;
pub mod strip_source_game_events;
pub mod synthesize_weapon_animation_aliases;
pub mod wire_honeybeast_swarm_death;

#[cfg(test)]
mod postprocess_performance_tests;
