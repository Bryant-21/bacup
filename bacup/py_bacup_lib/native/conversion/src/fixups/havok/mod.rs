//! Havok behavior-graph and animation fixups (Phase C.4).
//!
//! These fixups operate on `.hkx` files on disk (not on plugin records).
//! They walk directories under `ctx.mod_path/meshes/` and mutate HKX files
//! in place using the `havok_native` crate.
//!
//! | Module | Purpose |
//! |--------|---------|
//! | `normalize_spaced_asset_names` | Rename `stem .ext` typo files + patch project refs |
//! | `copy_character_support_files` | Port creature `*.ssf` + `bonelodsetting.txt` from source |
//! | `strip_source_game_events` | Drop FO76-only annotation events from animations |
//! | `inject_hitframe_events`   | Inject missing HitFrame into attack animations |
//! | `filter_unreferenced_behaviors` | Remove FO76-only generic behavior files |
//! | `inject_animation_names`   | Populate character.hkx assetNames list |
//! | `fix_character_rig_path`   | Rewrite FO76 skeleton paths to FO4 layout |
//! | `fix_subcreature_skeleton_paths` | Rewrite RACE skeletal model paths |
//! | `collect_behavior_clip_names` | Free function: collect hkbClipGenerator names |
//! | `anim_text_data_emit` | Decode RACE/IDLE records and drive `ck_native` AnimTextData generation |
//! | `repair_weapon_charge_reference_frames` | C.4.13 | Add the neutral extracted-motion frame required by FO4 charge holds |

pub mod anim_text_data_emit;
pub mod collect_behavior_clip_names;
pub mod copy_character_support_files;
pub mod filter_unreferenced_behaviors;
pub mod fix_character_rig_path;
pub mod fix_subcreature_skeleton_paths;
pub mod inject_animation_names;
pub mod inject_hitframe_events;
pub mod normalize_spaced_asset_names;
pub mod repair_weapon_charge_reference_frames;
pub mod strip_source_game_events;
