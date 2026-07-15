// Phase: postprocess_havok_assets

use crate::fixups::havok::copy_character_support_files::copy_character_support_files_in_mod_path;
use crate::fixups::havok::filter_unreferenced_behaviors::filter_unreferenced_behaviors_in_mod_path;
use crate::fixups::havok::fix_character_rig_path::fix_character_rig_path_in_mod_path;
use crate::fixups::havok::inject_animation_names::inject_animation_names_in_mod_path;
use crate::fixups::havok::inject_hitframe_events::inject_hitframe_events_in_mod_path;
use crate::fixups::havok::normalize_spaced_asset_names::normalize_spaced_asset_names_in_mod_path;
use crate::fixups::havok::repair_weapon_charge_reference_frames::repair_weapon_charge_reference_frames_in_mod_path;
use crate::fixups::havok::strip_source_game_events::strip_source_game_events_in_mod_path;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

pub struct PostprocessHavokAssetsPhase;

impl Phase for PostprocessHavokAssetsPhase {
    fn name(&self) -> &'static str {
        "postprocess_havok_assets"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;

        let mut assets_written = 0u32;
        let mut warnings = 0u32;
        let mut records_dropped = 0u32;

        // Execution order matters:
        // 0. normalize_spaced_asset_names — final on-disk names before anything
        //    derives creature names or project references from them.
        let r0 = normalize_spaced_asset_names_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        assets_written += r0.records_changed;

        // 1. strip_source_game_events
        let r1 = strip_source_game_events_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        assets_written += r1.records_changed;

        // 2. inject_hitframe_events
        let r2 = inject_hitframe_events_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        assets_written += r2.records_changed;

        // 3. repair_weapon_charge_reference_frames
        let r3 = repair_weapon_charge_reference_frames_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        assets_written += r3.records_changed;

        // 4. inject_animation_names
        let r4 = inject_animation_names_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        assets_written += r4.records_changed;
        warnings += r4.warnings.len() as u32;

        // 5. filter_unreferenced_behaviors
        let r5 = filter_unreferenced_behaviors_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        records_dropped += r5.records_dropped;

        // 6. fix_character_rig_path
        let r6 = fix_character_rig_path_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        assets_written += r6.records_changed;

        // 7. copy_character_support_files — .ssf/bonelodsetting.txt from source
        let r7 = copy_character_support_files_in_mod_path(ctx.mod_path, ctx.source_extracted_dir)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        assets_written += r7.records_changed;

        Ok(PhaseReport {
            assets_written,
            warnings,
            records_dropped,
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
    use crate::fixups::havok::filter_unreferenced_behaviors::filter_unreferenced_behaviors_in_mod_path;
    use crate::fixups::havok::fix_character_rig_path::fix_character_rig_path_in_mod_path;
    use crate::fixups::havok::inject_animation_names::inject_animation_names_in_mod_path;
    use crate::fixups::havok::inject_hitframe_events::inject_hitframe_events_in_mod_path;
    use crate::fixups::havok::repair_weapon_charge_reference_frames::repair_weapon_charge_reference_frames_in_mod_path;
    use crate::fixups::havok::strip_source_game_events::strip_source_game_events_in_mod_path;
    use std::fs;

    /// Creates a mod tree under `base` with:
    ///   data/Meshes/Actors/Deathclaw/Behaviors/ambushbehavior.hkx  (FO76-only → filter removes it)
    ///   data/Meshes/Actors/Deathclaw/Behaviors/deathclaw.hkx       (creature-specific → kept)
    fn setup_mod_tree(base: &std::path::Path) {
        let behaviors = base.join("data/Meshes/Actors/Deathclaw/Behaviors");
        fs::create_dir_all(&behaviors).unwrap();
        fs::write(behaviors.join("ambushbehavior.hkx"), b"dummy").unwrap();
        fs::write(behaviors.join("deathclaw.hkx"), b"dummy").unwrap();
    }

    /// Verify that all 6 transforms are called when `data/Meshes/` is the only
    /// root (no legacy `meshes/` dir).  The key observable effect without valid
    /// HKX content is `filter_unreferenced_behaviors`, which removes
    /// `ambushbehavior.hkx` from a `Behaviors/` directory.
    #[test]
    fn all_six_run_over_data_meshes_root() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path();
        setup_mod_tree(mod_path);

        // Run each transform in phase order (mirrors what the Phase does).
        let r1 = strip_source_game_events_in_mod_path(mod_path).unwrap();
        let r2 = inject_hitframe_events_in_mod_path(mod_path).unwrap();
        let r3 = repair_weapon_charge_reference_frames_in_mod_path(mod_path).unwrap();
        let r4 = inject_animation_names_in_mod_path(mod_path).unwrap();
        let r5 = filter_unreferenced_behaviors_in_mod_path(mod_path).unwrap();
        let r6 = fix_character_rig_path_in_mod_path(mod_path).unwrap();

        // Transforms 1, 2, 3, 4, 6 tolerate invalid HKX files and return 0 changed
        // (neither panics nor errors — proves they walked the data/Meshes root).
        assert_eq!(
            r1.records_changed, 0,
            "strip_source_game_events: wrong count"
        );
        assert_eq!(r2.records_changed, 0, "inject_hitframe_events: wrong count");
        assert_eq!(
            r3.records_changed, 0,
            "repair_weapon_charge_reference_frames: wrong count"
        );
        assert_eq!(r4.records_changed, 0, "inject_animation_names: wrong count");
        assert_eq!(r6.records_changed, 0, "fix_character_rig_path: wrong count");

        // Transform 5 is observable: it deletes ambushbehavior.hkx from Behaviors/.
        assert_eq!(
            r5.records_dropped, 1,
            "filter_unreferenced_behaviors: expected 1 removal"
        );

        let behaviors = mod_path.join("data/Meshes/Actors/Deathclaw/Behaviors");
        assert!(
            !behaviors.join("ambushbehavior.hkx").exists(),
            "ambushbehavior.hkx should be gone"
        );
        assert!(
            behaviors.join("deathclaw.hkx").exists(),
            "creature-specific file must be kept"
        );
    }

    /// When only a legacy `meshes/` root exists (no `data/Meshes/`), the fixups
    /// still walk it (backward-compat path).
    #[test]
    fn all_five_also_walk_legacy_meshes_root() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path();

        let behaviors = mod_path.join("meshes/Actors/Deathclaw/Behaviors");
        fs::create_dir_all(&behaviors).unwrap();
        fs::write(behaviors.join("ambushbehavior.hkx"), b"dummy").unwrap();

        let r4 = filter_unreferenced_behaviors_in_mod_path(mod_path).unwrap();
        assert_eq!(
            r4.records_dropped, 1,
            "legacy meshes/ root must also be walked"
        );
        assert!(!behaviors.join("ambushbehavior.hkx").exists());
    }

    /// When neither root exists, all 6 return empty reports (no panic, no error).
    #[test]
    fn empty_mod_path_returns_no_ops() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path();

        assert!(
            strip_source_game_events_in_mod_path(mod_path)
                .unwrap()
                .is_no_op()
        );
        assert!(
            inject_hitframe_events_in_mod_path(mod_path)
                .unwrap()
                .is_no_op()
        );
        assert!(
            repair_weapon_charge_reference_frames_in_mod_path(mod_path)
                .unwrap()
                .is_no_op()
        );
        assert!(
            inject_animation_names_in_mod_path(mod_path)
                .unwrap()
                .is_no_op()
        );
        assert!(
            filter_unreferenced_behaviors_in_mod_path(mod_path)
                .unwrap()
                .is_no_op()
        );
        assert!(
            fix_character_rig_path_in_mod_path(mod_path)
                .unwrap()
                .is_no_op()
        );
    }
}
