//! Canonical fixup segment plan.
//!
//! Each fixup is either fused into a sweep or runs as a single-fixup segment
//! through the registry mechanics for scope skips, `applies_to_session`,
//! convergence, and worklist overrides.
//!
//! This file is the ordered fixup source of truth used by regen.

use crate::fixups::Fixup;
use crate::store2::visitor::{RecordVisitor, Sweep};

use crate::fixups::apply_fo76_workshop_catalog::ApplyFo76WorkshopCatalogFixup;
use crate::fixups::apply_weapon_sound_defaults::ApplyWeaponSoundDefaultsFixup;
use crate::fixups::assign_legacy_worldspace_music::AssignLegacyWorldspaceMusicFixup;
use crate::fixups::attach_fo76_camp_collectors::AttachFo76CampCollectorsFixup;
use crate::fixups::attach_fo76_furniture_buffs::AttachFo76FurnitureBuffsFixup;
use crate::fixups::bridge_fo76_combat_music::BridgeFo76CombatMusicFixup;
use crate::fixups::clean_leveled_item_entries::CleanLeveledItemEntriesFixup;
use crate::fixups::clear_interior_hand_changed::ClearInteriorHandChangedFixup;
use crate::fixups::clear_orphaned_npc_template_flags::ClearOrphanedNpcTemplateFlagsFixup;
use crate::fixups::clear_protected_on_hostile_actors::ClearProtectedOnHostileActorsFixup;
use crate::fixups::collapse_projectile_compound_loop_sounds::CollapseProjectileCompoundLoopSoundsFixup;
use crate::fixups::creature::augment_creature_factions::AugmentCreatureFactionsFixup;
use crate::fixups::creature::bind_creature_scripts::BindCreatureScriptsFixup;
use crate::fixups::creature::clean_creature_esp_check_fields::CleanCreatureEspCheckFieldsFixup;
use crate::fixups::creature::fix_creature_npc_records::FixCreatureNpcRecordsFixup;
use crate::fixups::creature::fix_creature_race_records::FixCreatureRaceRecordsFixup;
use crate::fixups::creature::fix_creature_weapon_fire_seconds::FixCreatureWeaponFireSecondsFixup;
use crate::fixups::creature::fix_creature_weapons_and_records::FixCreatureWeaponsAndRecordsFixup;
use crate::fixups::creature::normalize_creature_lvln_template_chains::NormalizeCreatureLvlnTemplateChainsFixup;
use crate::fixups::creature::normalize_movement_type_names::NormalizeMovementTypeNamesFixup;
use crate::fixups::creature::nullify_creature_death_items::NullifyCreatureDeathItemsFixup;
use crate::fixups::creature::retarget_moleminer_movement_speeds::RetargetMoleminerMovementSpeedsFixup;
use crate::fixups::creature::strip_creature_subgraph_additive_race::StripCreatureSubgraphAdditiveRaceFixup;
use crate::fixups::drop_incompatible_player_idles::DropIncompatiblePlayerIdlesFixup;
use crate::fixups::drop_untranslatable_loadscreen_records::DropUntranslatableLoadscreenRecordsFixup;
use crate::fixups::expand_arma_races_from_armor_race::ExpandArmaRacesFromArmorRaceFixup;
use crate::fixups::face::flatten_nested_traits_lvlns::FlattenNestedTraitsLvlnsFixup;
use crate::fixups::face::generate_additive_races::GenerateAdditiveRacesFixup;
use crate::fixups::face::inject_human_npc_head_parts::InjectHumanNpcHeadPartsFixup;
use crate::fixups::face::materialize_leveled_template_npcs::MaterializeLeveledTemplateNpcsFixup;
use crate::fixups::face::strip_invalid_npc_face_morphs::StripInvalidNpcFaceMorphsFixup;
use crate::fixups::face::strip_unbaked_human_npc_face_morphs::StripUnbakedHumanNpcFaceMorphsFixup;
use crate::fixups::filter_non_vanilla_races_for_weapon_roots::FilterNonVanillaRacesForWeaponRootsFixup;
use crate::fixups::fix_invalid_target_formkeys::FixInvalidTargetFormKeysFixup;
use crate::fixups::fix_stag_sound_refs::FixStagSoundRefsFixup;
use crate::fixups::fix_water_spell_refs::FixWaterSpellRefsFixup;
use crate::fixups::flatten_npc_property_curves::FlattenNpcPropertyCurvesFixup;
use crate::fixups::flatten_omod_includes::FlattenOmodIncludesFixup;
use crate::fixups::harvest_modt::HarvestModtFixup;
use crate::fixups::havok::declare_engine_driven_locomotion::DeclareEngineDrivenLocomotionFixup;
use crate::fixups::havok::declare_locomotion_run_threshold::DeclareLocomotionRunThresholdFixup;
use crate::fixups::havok::filter_unreferenced_behaviors::FilterUnreferencedBehaviorsFixup;
use crate::fixups::havok::fix_character_rig_path::FixCharacterRigPathFixup;
use crate::fixups::havok::fix_subcreature_skeleton_paths::FixSubcreatureSkeletonPathsFixup;
use crate::fixups::havok::inject_animation_names::InjectAnimationNamesFixup;
use crate::fixups::havok::inject_attack_stop_events::InjectAttackStopEventsFixup;
use crate::fixups::havok::inject_hitframe_events::InjectHitframeEventsFixup;
use crate::fixups::havok::normalize_character_properties::NormalizeCharacterPropertiesFixup;
use crate::fixups::havok::normalize_creature_death_phase::NormalizeCreatureDeathPhaseFixup;
use crate::fixups::havok::normalize_weapon_behavior_contracts::NormalizeWeaponBehaviorContractsFixup;
use crate::fixups::havok::repair_weapon_charge_reference_frames::RepairWeaponChargeReferenceFramesFixup;
use crate::fixups::havok::retime_reload_complete_events::RetimeReloadCompleteEventsFixup;
use crate::fixups::havok::sanitize_ragdoll_contact_bones::SanitizeRagdollContactBonesFixup;
use crate::fixups::havok::strip_source_game_events::StripSourceGameEventsFixup;
use crate::fixups::havok::synthesize_weapon_animation_aliases::SynthesizeWeaponAnimationAliasesFixup;
use crate::fixups::inject_cobjs_for_omods::InjectCobjsForOmodsFixup;
use crate::fixups::inject_required_child_blocks::InjectRequiredChildBlocksFixup;
use crate::fixups::inject_weap_extra_data::InjectWeapExtraDataFixup;
use crate::fixups::ltex_txst_synth::LtexTxstSynthFixup;
use crate::fixups::mark_shelter_workshop_surfaces::MarkShelterWorkshopSurfacesFixup;
use crate::fixups::materialize_fo76_local_encounter_waves::MaterializeFo76LocalEncounterWavesFixup;
use crate::fixups::materialize_inherited_npc_object_templates::MaterializeInheritedNpcObjectTemplatesFixup;
use crate::fixups::materialize_legacy_npc_outfits::MaterializeLegacyNpcOutfitsFixup;
use crate::fixups::normalize_fo76_pack_templates::NormalizeFo76PackTemplatesFixup;
use crate::fixups::normalize_fo76_weather::NormalizeFo76WeatherFixup;
use crate::fixups::null_dangling_own_plugin_refs::NullDanglingOwnPluginRefsFixup;
use crate::fixups::pad_scene_player_dialogue_topics::PadScenePlayerDialogueTopicsFixup;
use crate::fixups::preserve_packin_storage_cells::PreservePackinStorageCellsFixup;
use crate::fixups::prune_orphaned_records::PruneOrphanedRecordsFixup;
use crate::fixups::recover_fo76_leveled_list_values::RecoverFo76LeveledListValuesFixup;
use crate::fixups::remap_creature_weapon_to_playable_twin::RemapCreatureWeaponToPlayableTwinFixup;
use crate::fixups::remap_idle_anchor_actions::RemapIdleAnchorActionsFixup;
use crate::fixups::remap_light_gobo_to_fo4_base::RemapLightGoboToFo4BaseFixup;
use crate::fixups::remap_nonplayable_armor_to_playable_twin::RemapNonplayableArmorToPlayableTwinFixup;
use crate::fixups::repair_expedition_mission_rewards::RepairExpeditionMissionRewardsFixup;
use crate::fixups::repair_fo76_ingestible_effects::RepairFo76IngestibleEffectsFixup;
use crate::fixups::repair_omod_target_keywords::RepairOmodTargetKeywordsFixup;
use crate::fixups::repair_quest_completion_rewards::RepairQuestCompletionRewardsFixup;
use crate::fixups::repair_quest_completion_xp::RepairQuestCompletionXpFixup;
use crate::fixups::repair_radio_scene_properties::RepairRadioScenePropertiesFixup;
use crate::fixups::resolve_addon_node_indices::ResolveAddonNodeIndicesFixup;
use crate::fixups::resolve_fo76_magic_effect_globals::ResolveFo76MagicEffectGlobalsFixup;
use crate::fixups::resolve_injected_stub_refs::ResolveInjectedStubRefsFixup;
use crate::fixups::restrict_translated_npc_for_slice::RestrictTranslatedNpcForSliceFixup;
use crate::fixups::rewrite_raw_lctn_formids::RewriteRawLctnFormIdsFixup;
use crate::fixups::rewrite_raw_object_template_formids::RewriteRawObjectTemplateFormIdsFixup;
use crate::fixups::rewrite_raw_wrld_large_refs::RewriteRawWrldLargeRefsFixup;
use crate::fixups::strip_alias_created_base_loc_ref_types::StripAliasCreatedBaseLocRefTypesFixup;
use crate::fixups::strip_perk_leveled_lists_from_containers::StripPerkLeveledListsFromContainersFixup;
use crate::fixups::sweep_unmapped_formkeys::SweepUnmappedFormKeysFixup;
use crate::fixups::sync_armo_hand_slots_from_addons::SyncArmoHandSlotsFromAddonsFixup;
use crate::fixups::synthesize_legacy_music::SynthesizeLegacyMusicFixup;
use crate::fixups::synthesize_weap_data_blocks::SynthesizeWeapDataBlocksFixup;
use crate::fixups::validate_reference_target_types::ValidateReferenceTargetTypesFixup;

use crate::store2::visitors::apply_weapon_sound_defaults::ApplyWeaponSoundDefaultsVisitor;
use crate::store2::visitors::cleanup_bodypart_data::CleanupBodypartDataVisitor;
use crate::store2::visitors::null_dangling_misc_refs::NullDanglingMiscRefsVisitor;
use crate::store2::visitors::null_dangling_vmad_refs::NullDanglingVmadRefsVisitor;
use crate::store2::visitors::null_invalid_qust_alla_keywords::NullInvalidQustAllaKeywordsVisitor;
use crate::store2::visitors::prune_faction_relations::PruneFactionRelationsVisitor;
use crate::store2::visitors::remap_struct_internal_formids::RemapStructInternalFormIdsVisitor;
use crate::store2::visitors::repair_scen_htid_sound_refs::RepairScenHtidSoundRefsVisitor;
use crate::store2::visitors::strip_atx_cobj_conditions::StripAtxCobjConditionsVisitor;
use crate::store2::visitors::strip_crafting_recipe_filters::StripCraftingRecipeFiltersVisitor;
use crate::store2::visitors::strip_dead_workshop_conditions::StripDeadWorkshopConditionsVisitor;
use crate::store2::visitors::strip_invalid_quest_condition_params::StripInvalidQuestConditionParamsVisitor;

pub enum Segment {
    Sweep(&'static str, fn() -> Vec<Box<dyn RecordVisitor>>),
    Fixup(fn() -> Box<dyn Fixup>),
}

impl Segment {
    pub fn fixup_names(&self) -> Vec<&'static str> {
        match self {
            Segment::Sweep(_, make) => make().iter().map(|v| v.name()).collect(),
            Segment::Fixup(make) => vec![make().name()],
        }
    }

    pub fn build_sweep(&self) -> Option<Sweep> {
        match self {
            Segment::Sweep(label, make) => Some(Sweep {
                label,
                visitors: make(),
            }),
            Segment::Fixup(_) => None,
        }
    }
}

/// One entry per fixup, in execution order.
pub fn build_default_segment_plan() -> Vec<Segment> {
    vec![
        Segment::Sweep("sweep@1", || {
            vec![Box::new(ApplyWeaponSoundDefaultsVisitor)]
        }),
        Segment::Sweep("sweep@3", || vec![Box::new(PruneFactionRelationsVisitor)]),
        Segment::Fixup(|| Box::new(InjectCobjsForOmodsFixup)),
        Segment::Fixup(|| Box::new(ResolveAddonNodeIndicesFixup)),
        Segment::Fixup(|| Box::new(ResolveInjectedStubRefsFixup)),
        Segment::Fixup(|| Box::new(PreservePackinStorageCellsFixup)),
        Segment::Fixup(|| Box::new(RepairQuestCompletionXpFixup)),
        Segment::Fixup(|| Box::new(RepairQuestCompletionRewardsFixup)),
        Segment::Fixup(|| Box::new(RepairExpeditionMissionRewardsFixup)),
        Segment::Fixup(|| Box::new(MaterializeFo76LocalEncounterWavesFixup)),
        Segment::Fixup(|| Box::new(LtexTxstSynthFixup)),
        Segment::Fixup(|| Box::new(SynthesizeLegacyMusicFixup)),
        Segment::Fixup(|| Box::new(AssignLegacyWorldspaceMusicFixup)),
        Segment::Fixup(|| Box::new(BridgeFo76CombatMusicFixup)),
        Segment::Fixup(|| Box::new(ClearInteriorHandChangedFixup)),
        Segment::Fixup(|| Box::new(SweepUnmappedFormKeysFixup)),
        Segment::Fixup(|| Box::new(RewriteRawLctnFormIdsFixup)),
        Segment::Fixup(|| Box::new(RewriteRawWrldLargeRefsFixup)),
        Segment::Fixup(|| Box::new(RewriteRawObjectTemplateFormIdsFixup)),
        Segment::Sweep("sweep@12", || {
            vec![Box::new(RemapStructInternalFormIdsVisitor)]
        }),
        Segment::Fixup(|| Box::new(FlattenOmodIncludesFixup)),
        Segment::Fixup(|| Box::new(NormalizeFo76PackTemplatesFixup)),
        Segment::Fixup(|| Box::new(NormalizeFo76WeatherFixup)),
        Segment::Fixup(|| {
            Box::new(crate::fixups::normalize_light_radii::NormalizeStarfieldLightRadiiFixup)
        }),
        Segment::Fixup(|| Box::new(ApplyFo76WorkshopCatalogFixup)),
        Segment::Fixup(|| Box::new(MarkShelterWorkshopSurfacesFixup)),
        // Reads the source RESO records, so it must run while the source handle
        // is still open, and before the VMAD sweep that validates the script
        // properties it attaches.
        Segment::Fixup(|| Box::new(AttachFo76CampCollectorsFixup)),
        Segment::Fixup(|| Box::new(SyncArmoHandSlotsFromAddonsFixup)),
        Segment::Fixup(|| Box::new(ExpandArmaRacesFromArmorRaceFixup)),
        Segment::Fixup(|| Box::new(MaterializeLegacyNpcOutfitsFixup)),
        Segment::Fixup(|| Box::new(RepairFo76IngestibleEffectsFixup)),
        Segment::Fixup(|| Box::new(ResolveFo76MagicEffectGlobalsFixup)),
        Segment::Fixup(|| Box::new(AttachFo76FurnitureBuffsFixup)),
        Segment::Fixup(|| Box::new(FixInvalidTargetFormKeysFixup)),
        Segment::Fixup(|| Box::new(ValidateReferenceTargetTypesFixup)),
        Segment::Sweep("sweep@17", || vec![Box::new(NullDanglingMiscRefsVisitor)]),
        Segment::Sweep("sweep@18", || {
            vec![Box::new(RepairScenHtidSoundRefsVisitor)]
        }),
        Segment::Fixup(|| Box::new(NullDanglingOwnPluginRefsFixup)),
        Segment::Sweep("sweep-C@20-22", || {
            vec![
                Box::new(NullDanglingVmadRefsVisitor),
                Box::new(StripInvalidQuestConditionParamsVisitor),
                Box::new(NullInvalidQustAllaKeywordsVisitor),
            ]
        }),
        // Must follow the sweep above: it reads the final SMQN set, so it has to
        // see every node the Story Manager phase emitted.
        Segment::Fixup(|| {
            Box::new(crate::fixups::drop_orphan_quest_event_scope::DropOrphanQuestEventScopeFixup)
        }),
        // Must follow the fixup above: it reads each quest's final
        // StartGameEnabled bit, which that pass clears when it removes dead event
        // scoping. Running earlier would leave the barks of a quest that is about
        // to become non-autostart ungated.
        Segment::Fixup(|| {
            Box::new(crate::fixups::gate_event_quest_barks::GateEventQuestBarksFixup)
        }),
        Segment::Fixup(|| Box::new(RepairRadioScenePropertiesFixup)),
        Segment::Fixup(|| Box::new(DropUntranslatableLoadscreenRecordsFixup)),
        Segment::Fixup(|| Box::new(FixWaterSpellRefsFixup)),
        Segment::Fixup(|| Box::new(CleanLeveledItemEntriesFixup)),
        Segment::Fixup(|| Box::new(StripPerkLeveledListsFromContainersFixup)),
        Segment::Fixup(|| Box::new(RecoverFo76LeveledListValuesFixup)),
        // Must precede PruneOrphanedRecords: repointing outfit entries at the
        // playable twin is what leaves the NPC-only duplicate unreferenced.
        Segment::Fixup(|| Box::new(RemapNonplayableArmorToPlayableTwinFixup)),
        // Same shape for weapons, and same reason to precede PruneOrphanedRecords.
        // Must also precede SynthesizeWeapDataBlocks/InjectWeapExtraData below, so
        // those see the twin an NPC actually ends up carrying.
        Segment::Fixup(|| Box::new(RemapCreatureWeaponToPlayableTwinFixup)),
        Segment::Fixup(|| Box::new(FixStagSoundRefsFixup)),
        // Runs after the dangling-ref sweeps above: it prefers the compound
        // layer that already resolves into Fallout4.esm, so every sound-ref
        // repoint and null-out has to have settled first.
        Segment::Fixup(|| Box::new(CollapseProjectileCompoundLoopSoundsFixup)),
        Segment::Fixup(|| Box::new(RemapIdleAnchorActionsFixup)),
        Segment::Fixup(|| Box::new(RemapLightGoboToFo4BaseFixup)),
        Segment::Fixup(|| {
            Box::new(crate::fixups::strip_orphan_race_properties::StripOrphanRacePropertiesFixup)
        }),
        Segment::Fixup(|| Box::new(FlattenNpcPropertyCurvesFixup)),
        Segment::Fixup(|| Box::new(SynthesizeWeapDataBlocksFixup)),
        Segment::Fixup(|| Box::new(InjectWeapExtraDataFixup)),
        Segment::Fixup(|| Box::new(FilterNonVanillaRacesForWeaponRootsFixup)),
        Segment::Fixup(|| Box::new(GenerateAdditiveRacesFixup)),
        Segment::Fixup(|| Box::new(FlattenNestedTraitsLvlnsFixup)),
        Segment::Fixup(|| Box::new(MaterializeLeveledTemplateNpcsFixup)),
        Segment::Fixup(|| Box::new(MaterializeInheritedNpcObjectTemplatesFixup)),
        // Must follow the two materialize passes above: NPCs they realize from
        // LVLN/template chains inherit ACBS verbatim, so they carry `Protected`
        // too and would otherwise escape the clear.
        Segment::Fixup(|| Box::new(ClearProtectedOnHostileActorsFixup)),
        Segment::Fixup(|| Box::new(RestrictTranslatedNpcForSliceFixup)),
        Segment::Fixup(|| Box::new(InjectHumanNpcHeadPartsFixup)),
        Segment::Fixup(|| Box::new(StripUnbakedHumanNpcFaceMorphsFixup)),
        Segment::Fixup(|| Box::new(StripInvalidNpcFaceMorphsFixup)),
        Segment::Fixup(|| Box::new(FixCreatureNpcRecordsFixup)),
        Segment::Fixup(|| Box::new(NormalizeCreatureLvlnTemplateChainsFixup)),
        Segment::Fixup(|| Box::new(FixCreatureWeaponsAndRecordsFixup)),
        Segment::Fixup(|| Box::new(FixCreatureRaceRecordsFixup)),
        Segment::Fixup(|| Box::new(RetargetMoleminerMovementSpeedsFixup)),
        Segment::Fixup(|| Box::new(NormalizeMovementTypeNamesFixup)),
        Segment::Fixup(|| Box::new(CleanCreatureEspCheckFieldsFixup)),
        Segment::Fixup(|| Box::new(AugmentCreatureFactionsFixup)),
        Segment::Fixup(|| Box::new(NullifyCreatureDeathItemsFixup)),
        // Before PruneOrphanedRecords: the VMAD properties it writes are the
        // only thing referencing the three crExplosionFloater*Death records.
        Segment::Fixup(|| Box::new(BindCreatureScriptsFixup)),
        Segment::Fixup(|| Box::new(StripCreatureSubgraphAdditiveRaceFixup)),
        Segment::Fixup(|| Box::new(FixCreatureWeaponFireSecondsFixup)),
        Segment::Sweep("sweep@41", || vec![Box::new(CleanupBodypartDataVisitor)]),
        Segment::Fixup(|| Box::new(StripSourceGameEventsFixup)),
        Segment::Fixup(|| Box::new(InjectHitframeEventsFixup)),
        Segment::Fixup(|| Box::new(InjectAttackStopEventsFixup)),
        Segment::Fixup(|| Box::new(RepairWeaponChargeReferenceFramesFixup)),
        Segment::Fixup(|| Box::new(RetimeReloadCompleteEventsFixup)),
        Segment::Fixup(|| Box::new(SanitizeRagdollContactBonesFixup)),
        Segment::Fixup(|| Box::new(NormalizeWeaponBehaviorContractsFixup)),
        Segment::Fixup(|| Box::new(DeclareEngineDrivenLocomotionFixup)),
        // Before InjectAnimationNames, which prunes orphan objects and trailing
        // variant values — appending a variable after that prune would reintroduce
        // the tail entries it just trimmed.
        Segment::Fixup(|| Box::new(DeclareLocomotionRunThresholdFixup)),
        // After contract normalization, which requires eventNames/eventInfos to
        // stay aligned; before InjectAnimationNames prunes the orphaned FO76 gates.
        Segment::Fixup(|| Box::new(NormalizeCreatureDeathPhaseFixup)),
        Segment::Fixup(|| Box::new(SynthesizeWeaponAnimationAliasesFixup)),
        Segment::Fixup(|| Box::new(FilterUnreferencedBehaviorsFixup)),
        Segment::Fixup(|| Box::new(InjectAnimationNamesFixup)),
        // After InjectAnimationNames: that fixup prunes orphan objects and
        // trailing variant values, which would shift the slot borrowed here.
        Segment::Fixup(|| Box::new(NormalizeCharacterPropertiesFixup)),
        Segment::Fixup(|| Box::new(FixCharacterRigPathFixup)),
        Segment::Fixup(|| Box::new(FixSubcreatureSkeletonPathsFixup)),
        Segment::Fixup(|| Box::new(InjectRequiredChildBlocksFixup)),
        // All three scan COBJ on the decoded lane, so they fuse into one pass.
        // Must stay after ApplyFo76WorkshopCatalogFixup: the filter strip reads
        // the workshop BNAM that fixup stamps to tell a camp recipe from a
        // crafting one.
        Segment::Sweep("sweep@49", || {
            vec![
                Box::new(StripAtxCobjConditionsVisitor),
                Box::new(StripCraftingRecipeFiltersVisitor),
                Box::new(StripDeadWorkshopConditionsVisitor),
            ]
        }),
        // Last, after every NPC materialization pass above: those can mint new
        // base records (and repoint alias ALCO at them), and each new base
        // inherits the FO76 FTYP this pass has to remove.
        Segment::Fixup(|| Box::new(StripAliasCreatedBaseLocRefTypesFixup)),
        // After NullDanglingOwnPluginRefsFixup, which can null a SCEN PTOP in
        // place and open a new gap, and after every pass that can add scenes:
        // this one mints the DIALs those gaps need.
        Segment::Fixup(|| Box::new(PadScenePlayerDialogueTopicsFixup)),
        Segment::Fixup(|| Box::new(HarvestModtFixup)),
        Segment::Fixup(|| Box::new(PruneOrphanedRecordsFixup)),
        // After PruneOrphanedRecords, the last pass that can retire a
        // template target: an NPC left with ACBS template flags and no
        // TPLT has no resolvable RACE and hard-crashes the engine while
        // streaming the cell in, so this clears the orphaned half.
        Segment::Fixup(|| Box::new(ClearOrphanedNpcTemplateFlagsFixup)),
        Segment::Fixup(|| Box::new(RepairOmodTargetKeywordsFixup)),
        Segment::Fixup(|| {
            Box::new(crate::fixups::restore_fo76_plan_learning::RestoreFo76PlanLearningFixup)
        }),
        Segment::Fixup(|| Box::new(crate::fo76_behaviors::records::PreserveFo76BehaviorsFixup)),
        // Keep source action branches available until their private graph families are cloned.
        Segment::Fixup(|| Box::new(DropIncompatiblePlayerIdlesFixup)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_segment_plan_has_unique_fixup_names() {
        let plan_names: Vec<&str> = build_default_segment_plan()
            .iter()
            .flat_map(|s| s.fixup_names())
            .collect();
        let mut unique = plan_names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            plan_names.len(),
            "duplicate fixup name in canonical segment plan"
        );
        assert!(plan_names.contains(&"drop_incompatible_player_idles"));
        assert!(plan_names.contains(&"flatten_nested_traits_lvlns"));
        assert!(plan_names.contains(&"materialize_leveled_template_npcs"));
        assert!(plan_names.contains(&"materialize_inherited_npc_object_templates"));
        assert!(plan_names.contains(&"materialize_legacy_npc_outfits"));
        assert!(
            plan_names
                .iter()
                .position(|name| *name == "materialize_inherited_npc_object_templates")
                < plan_names
                    .iter()
                    .position(|name| *name == "restrict_translated_npc_for_slice")
        );
        assert!(
            plan_names
                .iter()
                .position(|name| *name == "materialize_legacy_npc_outfits")
                < plan_names
                    .iter()
                    .position(|name| *name == "restrict_translated_npc_for_slice")
        );
        assert!(!plan_names.contains(&"filter_lchar_template_npcs"));
        assert!(plan_names.contains(&"inject_animation_names"));
        assert!(plan_names.contains(&"sanitize_ragdoll_contact_bones"));
        assert!(plan_names.contains(&"normalize_creature_death_phase"));
        assert!(plan_names.contains(&"retime_reload_complete_events"));
        assert!(plan_names.contains(&"normalize_weapon_behavior_contracts"));
        assert!(plan_names.contains(&"synthesize_weapon_animation_aliases"));
        assert!(
            plan_names
                .iter()
                .position(|name| *name == "normalize_weapon_behavior_contracts")
                < plan_names
                    .iter()
                    .position(|name| *name == "normalize_creature_death_phase")
        );
        assert!(
            plan_names
                .iter()
                .position(|name| *name == "normalize_creature_death_phase")
                < plan_names
                    .iter()
                    .position(|name| *name == "inject_animation_names")
        );
        assert!(
            plan_names
                .iter()
                .position(|name| *name == "synthesize_weapon_animation_aliases")
                < plan_names
                    .iter()
                    .position(|name| *name == "inject_animation_names")
        );
        assert!(plan_names.contains(&"normalize_creature_lvln_template_chains"));
        assert!(plan_names.contains(&"normalize_fo76_weather"));
        assert!(plan_names.contains(&"apply_fo76_workshop_catalog"));
        assert!(plan_names.contains(&"bridge_fo76_combat_music"));
        assert!(plan_names.contains(&"mark_shelter_workshop_surfaces"));
        assert!(plan_names.contains(&"repair_quest_completion_xp"));
        assert!(plan_names.contains(&"repair_quest_completion_rewards"));
        assert!(plan_names.contains(&"repair_expedition_mission_rewards"));
        assert!(plan_names.contains(&"repair_fo76_ingestible_effects"));
        assert!(plan_names.contains(&"resolve_fo76_magic_effect_globals"));
        assert!(
            plan_names
                .iter()
                .position(|name| *name == "repair_fo76_ingestible_effects")
                < plan_names
                    .iter()
                    .position(|name| *name == "validate_reference_target_types")
        );
        assert!(
            plan_names
                .iter()
                .position(|name| *name == "resolve_fo76_magic_effect_globals")
                < plan_names
                    .iter()
                    .position(|name| *name == "validate_reference_target_types")
        );
        assert!(
            plan_names
                .iter()
                .position(|name| *name == "repair_quest_completion_rewards")
                < plan_names
                    .iter()
                    .position(|name| *name == "repair_expedition_mission_rewards")
        );
        assert!(plan_names.contains(&"materialize_fo76_local_encounter_waves"));
        assert!(plan_names.contains(&"repair_radio_scene_properties"));
    }
}

#[cfg(test)]
mod keystone_tests {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::run::{RunConfig, RunParams, create_run, drop_run, with_run};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use crate::translator::Game;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
    use smallvec::SmallVec;

    fn rec(
        sig: &str,
        local: u32,
        eid: &str,
        extra: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        let eid_sym = interner.intern(eid);
        let mut fields: SmallVec<[FieldEntry; 8]> = smallvec::smallvec![FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(eid_sym),
        }];
        fields.extend(extra);
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("FixupsV2.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    fn bytes_entry(sig: &str, b: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(b)),
        }
    }

    fn ctda(function_id: u16, param1: u32, param2: u32) -> Vec<u8> {
        let mut b = vec![0u8; 32];
        b[8..10].copy_from_slice(&function_id.to_le_bytes());
        b[12..16].copy_from_slice(&param1.to_le_bytes());
        b[16..20].copy_from_slice(&param2.to_le_bytes());
        b
    }

    /// Seed records that exercise sweeps @1, @3, @17, sweep-C and the no-op /
    /// skip paths of the single-fixup segments.
    fn seed_target(handle: u64) {
        let interner = StringInterner::new();
        let mut session = open_session(handle, None).expect("session");
        let schema = session.schema().expect("schema");
        let records = vec![
            rec(
                "WEAP",
                0x801,
                "WeapZeroSounds",
                vec![bytes_entry("DNAM", vec![0u8; 105])],
                &interner,
            ),
            rec(
                "FACT",
                0x802,
                "FactA",
                vec![bytes_entry("XNAM", {
                    let mut b = Vec::new();
                    b.extend_from_slice(&0x003B_A686u32.to_le_bytes()); // unknown → pruned
                    b.extend_from_slice(&[0u8; 8]);
                    b
                })],
                &interner,
            ),
            rec("KYWD", 0x900, "GoodKW", vec![], &interner),
            rec(
                "QUST",
                0x803,
                "TripleQust",
                vec![
                    bytes_entry("CTDA", ctda(576, 0, 0)),
                    bytes_entry("ALST", 1u32.to_le_bytes().to_vec()),
                    bytes_entry("ALLA", {
                        let mut b = Vec::new();
                        b.extend_from_slice(&0x0002_FD66u32.to_le_bytes());
                        b.extend_from_slice(&1i32.to_le_bytes());
                        b
                    }),
                ],
                &interner,
            ),
            rec(
                "IDLE",
                0x804,
                "IdleDangling",
                vec![bytes_entry("ANAM", 0x0000_0999u32.to_le_bytes().to_vec())],
                &interner,
            ),
        ];
        for r in records {
            session.add_record(r, schema.as_ref(), &interner).unwrap();
        }
    }

    fn run_fixups_v2() -> Vec<(String, u32)> {
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native("FixupsV2.esp", Some("fo4")).unwrap();
        seed_target(target);
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "FixupsV2.esp".into(),
                is_whole_plugin: true,
                ..Default::default()
            },
        })
        .unwrap();
        let reports = with_run(id, |run| {
            run.apply_fixups_v2().map_err(crate::run::RunError::from)
        })
        .unwrap();
        drop_run(id).unwrap();
        reports
            .into_iter()
            .map(|(name, r)| (name, r.records_changed))
            .collect()
    }

    #[test]
    fn apply_fixups_v2_runs_canonical_segment_plan() {
        let reports = run_fixups_v2();

        assert!(
            reports.iter().any(|(_, changed)| *changed > 0),
            "fixture exercised no mutation — strengthen the fixture"
        );
        assert!(
            reports
                .iter()
                .any(|(name, _)| name == "apply_weapon_sound_defaults")
        );
        assert!(
            reports
                .iter()
                .any(|(name, _)| name == "inject_animation_names")
        );
        assert!(
            reports
                .iter()
                .any(|(name, _)| name == "repair_omod_target_keywords")
        );
    }
}
