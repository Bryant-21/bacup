// Phase: postprocess_havok_assets

use crate::fixups::havok::copy_character_support_files::copy_character_support_files_in_mod_path;
use crate::fixups::havok::declare_engine_driven_locomotion::declare_engine_driven_locomotion_in_mod_path;
use crate::fixups::havok::filter_unreferenced_behaviors::filter_unreferenced_behaviors_preserving;
use crate::fixups::havok::fix_character_rig_path::fix_character_rig_path_in_mod_path;
use crate::fixups::havok::inject_animation_names::inject_animation_names_in_mod_path;
use crate::fixups::havok::inject_attack_stop_events::inject_attack_stop_events_in_mod_path;
use crate::fixups::havok::inject_hitframe_events::inject_hitframe_events_in_mod_path;
use crate::fixups::havok::normalize_character_properties::normalize_character_properties_in_mod_path;
use crate::fixups::havok::normalize_spaced_asset_names::{
    normalize_spaced_asset_names_in_mod_path, normalized_reference,
};
use crate::fixups::havok::normalize_weapon_behavior_contracts::normalize_weapon_behavior_contracts_in_mod_path;
use crate::fixups::havok::repair_weapon_charge_reference_frames::repair_weapon_charge_reference_frames_in_mod_path;
use crate::fixups::havok::retime_reload_complete_events::retime_reload_complete_events_in_mod_path;
use crate::fixups::havok::sanitize_ragdoll_contact_bones::sanitize_ragdoll_contact_bones_in_mod_path;
use crate::fixups::havok::strip_source_game_events::strip_source_game_events_in_mod_path;
use crate::fixups::havok::synthesize_weapon_animation_aliases::{
    synthesize_melee_animation_aliases_in_mod_path, synthesize_mt_animation_aliases_in_mod_path,
    synthesize_weapon_animation_aliases_in_mod_path,
};
use crate::fixups::havok::wire_honeybeast_swarm_death::wire_honeybeast_swarm_death_in_mod_path;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

pub struct PostprocessHavokAssetsPhase;

pub(crate) fn owns_record_fixup(name: &str) -> bool {
    matches!(
        name,
        "strip_source_game_events"
            | "inject_hitframe_events"
            | "inject_attack_stop_events"
            | "repair_weapon_charge_reference_frames"
            | "retime_reload_complete_events"
            | "sanitize_ragdoll_contact_bones"
            | "normalize_weapon_behavior_contracts"
            | "declare_engine_driven_locomotion"
            | "synthesize_weapon_animation_aliases"
            | "filter_unreferenced_behaviors"
            | "inject_animation_names"
            | "normalize_character_properties"
            | "fix_character_rig_path"
    )
}

impl Phase for PostprocessHavokAssetsPhase {
    fn name(&self) -> &'static str {
        "postprocess_havok_assets"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;

        let mut assets_written = 0u32;
        let mut warnings = 0u32;
        let mut records_dropped = 0u32;
        macro_rules! log_timing {
            ($name:expr, $started:expr, $changed:expr, $dropped:expr, $warnings:expr) => {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "postprocess_havok_assets",
                    level: LogLevel::Info,
                    message: format!(
                        "[havok_postprocess_timing] pass={} elapsed_ms={} changed={} dropped={} warnings={}",
                        $name,
                        $started.elapsed().as_millis(),
                        $changed,
                        $dropped,
                        $warnings,
                    ),
                });
            };
        }
        let source_behaviors = ctx.run.source == crate::translator::Game::Fo76
            && ctx.run.target == crate::translator::Game::Fo4
            && ctx
                .mod_path
                .join(crate::fo76_behaviors::PLAN_PATH)
                .is_file();
        let manifest_path = ctx.mod_path.join("debug/fo76_behaviors/assets.json");
        let mut generated_manifest: Option<serde_json::Value> = if source_behaviors {
            let started = std::time::Instant::now();
            let target = ctx.target_extracted_dir.ok_or_else(|| {
                PhaseError::Internal(
                    "FO76 behavior ports require the extracted FO4 project contracts".into(),
                )
            })?;
            let generation = crate::fo76_behaviors::assets::build_profiled(
                ctx.mod_path,
                ctx.source_extracted_dir,
                target,
            )
            .map_err(PhaseError::Internal)?;
            let generated = generation.assets_written;
            assets_written += generated;
            log_timing!("fo76_behavior_generation", started, generated, 0, 0);
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "postprocess_havok_assets",
                level: LogLevel::Info,
                message: format!(
                    "[fo76_behavior_generation_timing] discovery_ms={} parsing_ms={} route_construction_ms={} writes_ms={} parsed_inputs={} shared_input_cache_hits={} bone_order_cache_hits={}",
                    generation.discovery.as_millis(),
                    generation.parsing.as_millis(),
                    generation.route_construction.as_millis(),
                    generation.writes.as_millis(),
                    generation.parsed_inputs,
                    generation.shared_input_cache_hits,
                    generation.bone_order_cache_hits,
                ),
            });
            Some(
                serde_json::from_slice(
                    &std::fs::read(&manifest_path)
                        .map_err(|e| PhaseError::Internal(e.to_string()))?,
                )
                .map_err(|e| PhaseError::Internal(e.to_string()))?,
            )
        } else {
            None
        };

        // Execution order matters:
        // 0. normalize_spaced_asset_names — final on-disk names before anything
        //    derives creature names or project references from them.
        let started = std::time::Instant::now();
        let r0 = normalize_spaced_asset_names_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "normalize_spaced_asset_names",
            started,
            r0.records_changed,
            0,
            r0.warnings.len()
        );
        assets_written += r0.records_changed;
        let mut generated_files = Vec::new();
        if let Some(manifest) = &mut generated_manifest {
            if let Some(files) = manifest["files"].as_array_mut() {
                for file in files {
                    if let Some(relative) = file.as_str() {
                        let relative =
                            normalized_reference(relative).unwrap_or_else(|| relative.to_owned());
                        *file = serde_json::Value::String(relative.clone());
                        generated_files.push(relative);
                    }
                }
            }
        }

        // 1. strip_source_game_events
        let started = std::time::Instant::now();
        let r1 = strip_source_game_events_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "strip_source_game_events",
            started,
            r1.records_changed,
            0,
            r1.warnings.len()
        );
        assets_written += r1.records_changed;

        // 2. inject_hitframe_events
        let started = std::time::Instant::now();
        let r2 = inject_hitframe_events_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "inject_hitframe_events",
            started,
            r2.records_changed,
            0,
            r2.warnings.len()
        );
        assets_written += r2.records_changed;

        // 2b. inject_attack_stop_events — after HitFrame injection so the
        // exit event can clamp against the full swing/hit family.
        let started = std::time::Instant::now();
        let r2b = inject_attack_stop_events_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "inject_attack_stop_events",
            started,
            r2b.records_changed,
            0,
            r2b.warnings.len()
        );
        assets_written += r2b.records_changed;

        // 3. repair_weapon_charge_reference_frames
        let started = std::time::Instant::now();
        let r3 = repair_weapon_charge_reference_frames_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "repair_weapon_charge_reference_frames",
            started,
            r3.records_changed,
            0,
            r3.warnings.len()
        );
        assets_written += r3.records_changed;

        let started = std::time::Instant::now();
        let reload_events = retime_reload_complete_events_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "retime_reload_complete_events",
            started,
            reload_events.records_changed,
            0,
            reload_events.warnings.len()
        );
        assets_written += reload_events.records_changed;

        // FO4 does not bounds-check contact indices before using the ragdoll's
        // bone-to-body map.
        let started = std::time::Instant::now();
        let ragdoll_contacts = sanitize_ragdoll_contact_bones_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "sanitize_ragdoll_contact_bones",
            started,
            ragdoll_contacts.records_changed,
            0,
            ragdoll_contacts.warnings.len()
        );
        assets_written += ragdoll_contacts.records_changed;

        // Converted custom roots must expose the FO4 WeaponBehavior name
        // contract before RACE subgraphs can select the replacement graph.
        if !source_behaviors {
            let started = std::time::Instant::now();
            let weapon_contracts = normalize_weapon_behavior_contracts_in_mod_path(
                ctx.mod_path,
                ctx.target_extracted_dir,
            )
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
            log_timing!(
                "normalize_weapon_behavior_contracts",
                started,
                weapon_contracts.records_changed,
                0,
                weapon_contracts.warnings.len()
            );
            assets_written += weapon_contracts.records_changed;
        }

        // FO76 authors some creatures with in-place locomotion clips and lets the
        // engine supply the movement. Graph-driven is FO4's default, and it would
        // derive their translation from root motion that is zero, so they never
        // leave idle and never close to attack range.
        let started = std::time::Instant::now();
        let engine_driven = declare_engine_driven_locomotion_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "declare_engine_driven_locomotion",
            started,
            engine_driven.records_changed,
            0,
            engine_driven.warnings.len()
        );
        assets_written += engine_driven.records_changed;

        // FO4's shared WeaponBehavior uses different ready/additive clip names
        // than FO76's GunBehavior. Supply aliases before character asset-name
        // injection so the replacement graph can bind its firing states.
        if !source_behaviors {
            let started = std::time::Instant::now();
            let weapon_animation_aliases = ctx
                .target_extracted_dir
                .map(|target| synthesize_weapon_animation_aliases_in_mod_path(ctx.mod_path, target))
                .transpose()
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            if let Some(report) = weapon_animation_aliases {
                log_timing!(
                    "synthesize_weapon_animation_aliases",
                    started,
                    report.records_changed,
                    0,
                    report.warnings.len()
                );
                assets_written += report.records_changed;
            }

            // FO4's shared MTBehavior expects lean-named movement clips. FO76 actor
            // directories provide equivalent left/right clips.
            let started = std::time::Instant::now();
            let mt_animation_aliases = ctx
                .target_extracted_dir
                .map(|target| synthesize_mt_animation_aliases_in_mod_path(ctx.mod_path, target))
                .transpose()
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            if let Some(report) = mt_animation_aliases {
                log_timing!(
                    "synthesize_mt_animation_aliases",
                    started,
                    report.records_changed,
                    0,
                    report.warnings.len()
                );
                assets_written += report.records_changed;
            }

            // FO4's shared MeleeBehavior requests backward, sprinting, and
            // diagonal clip names that FO76 creature H2H folders omit.
            let started = std::time::Instant::now();
            let melee_animation_aliases = ctx
                .target_extracted_dir
                .map(|target| synthesize_melee_animation_aliases_in_mod_path(ctx.mod_path, target))
                .transpose()
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            if let Some(report) = melee_animation_aliases {
                log_timing!(
                    "synthesize_melee_animation_aliases",
                    started,
                    report.records_changed,
                    0,
                    report.warnings.len()
                );
                assets_written += report.records_changed;
            }
        }

        let started = std::time::Instant::now();
        let swarm_death = wire_honeybeast_swarm_death_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "wire_honeybeast_swarm_death",
            started,
            swarm_death.records_changed,
            0,
            swarm_death.warnings.len()
        );
        assets_written += swarm_death.records_changed;

        // 4. inject_animation_names
        let started = std::time::Instant::now();
        let r4 = inject_animation_names_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "inject_animation_names",
            started,
            r4.records_changed,
            0,
            r4.warnings.len()
        );
        assets_written += r4.records_changed;
        warnings += r4.warnings.len() as u32;

        // 4b. Declare FO4's shared character properties. After
        // inject_animation_names, which prunes orphan objects and trailing
        // variant values this borrows a slot from.
        if !source_behaviors {
            let started = std::time::Instant::now();
            let character_properties =
                normalize_character_properties_in_mod_path(ctx.mod_path, ctx.target_extracted_dir)
                    .map_err(|e| PhaseError::Internal(e.to_string()))?;
            log_timing!(
                "normalize_character_properties",
                started,
                character_properties.records_changed,
                0,
                character_properties.warnings.len()
            );
            assets_written += character_properties.records_changed;
        }

        // 5. filter_unreferenced_behaviors
        let preserved_files: Vec<_> = generated_files
            .iter()
            .map(|relative| ctx.mod_path.join("data/Meshes").join(relative))
            .collect();
        let started = std::time::Instant::now();
        let r5 = filter_unreferenced_behaviors_preserving(ctx.mod_path, &preserved_files)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "filter_unreferenced_behaviors",
            started,
            r5.records_changed,
            r5.records_dropped,
            r5.warnings.len()
        );
        records_dropped += r5.records_dropped;

        // 6. fix_character_rig_path
        let started = std::time::Instant::now();
        let r6 = fix_character_rig_path_in_mod_path(ctx.mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "fix_character_rig_path",
            started,
            r6.records_changed,
            0,
            r6.warnings.len()
        );
        assets_written += r6.records_changed;

        // 7. copy_character_support_files — .ssf/bonelodsetting.txt from source
        let started = std::time::Instant::now();
        let r7 = copy_character_support_files_in_mod_path(ctx.mod_path, ctx.source_extracted_dir)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_timing!(
            "copy_character_support_files",
            started,
            r7.records_changed,
            0,
            r7.warnings.len()
        );
        assets_written += r7.records_changed;

        if let Some(manifest) = generated_manifest {
            for relative in &generated_files {
                let path = ctx.mod_path.join("data/Meshes").join(relative);
                if !path.is_file() {
                    return Err(PhaseError::Internal(format!(
                        "Generated behavior asset missing after postprocessing: {}",
                        path.display()
                    )));
                }
                if let Some(sink) = &ctx.run.output_sink {
                    sink.add_existing_file(&format!("Meshes/{relative}"), &path)
                        .map_err(|e| PhaseError::Internal(e.to_string()))?;
                }
            }
            std::fs::write(
                &manifest_path,
                serde_json::to_vec_pretty(&manifest)
                    .map_err(|e| PhaseError::Internal(e.to_string()))?,
            )
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        }

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
    use crate::fixups::havok::normalize_weapon_behavior_contracts::normalize_weapon_behavior_contracts_in_mod_path;
    use crate::fixups::havok::repair_weapon_charge_reference_frames::repair_weapon_charge_reference_frames_in_mod_path;
    use crate::fixups::havok::retime_reload_complete_events::retime_reload_complete_events_in_mod_path;
    use crate::fixups::havok::sanitize_ragdoll_contact_bones::sanitize_ragdoll_contact_bones_in_mod_path;
    use crate::fixups::havok::strip_source_game_events::strip_source_game_events_in_mod_path;
    use crate::fixups::havok::synthesize_weapon_animation_aliases::{
        synthesize_melee_animation_aliases_in_mod_path,
        synthesize_mt_animation_aliases_in_mod_path,
        synthesize_weapon_animation_aliases_in_mod_path,
    };
    use crate::fixups::havok::wire_honeybeast_swarm_death::wire_honeybeast_swarm_death_in_mod_path;
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

    /// Verify that all transforms are called when `data/Meshes/` is the only
    /// root (no legacy `meshes/` dir).  The key observable effect without valid
    /// HKX content is `filter_unreferenced_behaviors`, which removes
    /// `ambushbehavior.hkx` from a `Behaviors/` directory.
    #[test]
    fn all_transforms_run_over_data_meshes_root() {
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path();
        setup_mod_tree(mod_path);

        // Run each transform in phase order (mirrors what the Phase does).
        let r1 = strip_source_game_events_in_mod_path(mod_path).unwrap();
        let r2 = inject_hitframe_events_in_mod_path(mod_path).unwrap();
        let r3 = repair_weapon_charge_reference_frames_in_mod_path(mod_path).unwrap();
        let reload_events = retime_reload_complete_events_in_mod_path(mod_path).unwrap();
        let ragdoll_contacts = sanitize_ragdoll_contact_bones_in_mod_path(mod_path).unwrap();
        let weapon_contracts =
            normalize_weapon_behavior_contracts_in_mod_path(mod_path, None).unwrap();
        let weapon_animation_aliases = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(weapon_animation_aliases.path().join("Meshes")).unwrap();
        let aliases = synthesize_weapon_animation_aliases_in_mod_path(
            mod_path,
            weapon_animation_aliases.path(),
        )
        .unwrap();
        let mt_aliases =
            synthesize_mt_animation_aliases_in_mod_path(mod_path, weapon_animation_aliases.path())
                .unwrap();
        let melee_aliases = synthesize_melee_animation_aliases_in_mod_path(
            mod_path,
            weapon_animation_aliases.path(),
        )
        .unwrap();
        let swarm_death = wire_honeybeast_swarm_death_in_mod_path(mod_path).unwrap();
        let r4 = inject_animation_names_in_mod_path(mod_path).unwrap();
        let r5 = filter_unreferenced_behaviors_in_mod_path(mod_path).unwrap();
        let r6 = fix_character_rig_path_in_mod_path(mod_path).unwrap();

        // All transforms except the unreferenced-behavior filter tolerate invalid HKX
        // files and return 0 changed.
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
        assert_eq!(
            reload_events.records_changed, 0,
            "retime_reload_complete_events: wrong count"
        );
        assert_eq!(
            ragdoll_contacts.records_changed, 0,
            "sanitize_ragdoll_contact_bones: wrong count"
        );
        assert_eq!(
            weapon_contracts.records_changed, 0,
            "normalize_weapon_behavior_contracts: wrong count"
        );
        assert_eq!(
            aliases.records_changed, 0,
            "synthesize_weapon_animation_aliases: wrong count"
        );
        assert_eq!(
            mt_aliases.records_changed, 0,
            "synthesize_mt_animation_aliases: wrong count"
        );
        assert_eq!(
            melee_aliases.records_changed, 0,
            "synthesize_melee_animation_aliases: wrong count"
        );
        assert_eq!(
            swarm_death.records_changed, 0,
            "wire_honeybeast_swarm_death: wrong count"
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
            normalize_weapon_behavior_contracts_in_mod_path(mod_path, None)
                .unwrap()
                .is_no_op()
        );
        let target = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(target.path().join("Meshes")).unwrap();
        assert!(
            synthesize_weapon_animation_aliases_in_mod_path(mod_path, target.path())
                .unwrap()
                .is_no_op()
        );
        assert!(
            synthesize_mt_animation_aliases_in_mod_path(mod_path, target.path())
                .unwrap()
                .is_no_op()
        );
        assert!(
            synthesize_melee_animation_aliases_in_mod_path(mod_path, target.path())
                .unwrap()
                .is_no_op()
        );
        assert!(
            wire_honeybeast_swarm_death_in_mod_path(mod_path)
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
