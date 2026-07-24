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
use crate::fixups::clean_leveled_item_entries::CleanLeveledItemEntriesFixup;
use crate::fixups::clear_interior_hand_changed::ClearInteriorHandChangedFixup;
use crate::fixups::creature::augment_creature_factions::AugmentCreatureFactionsFixup;
use crate::fixups::creature::clean_creature_esp_check_fields::CleanCreatureEspCheckFieldsFixup;
use crate::fixups::creature::fix_creature_npc_records::FixCreatureNpcRecordsFixup;
use crate::fixups::creature::fix_creature_race_records::FixCreatureRaceRecordsFixup;
use crate::fixups::creature::fix_creature_weapon_fire_seconds::FixCreatureWeaponFireSecondsFixup;
use crate::fixups::creature::fix_creature_weapons_and_records::FixCreatureWeaponsAndRecordsFixup;
use crate::fixups::creature::normalize_creature_lvln_template_chains::NormalizeCreatureLvlnTemplateChainsFixup;
use crate::fixups::creature::nullify_creature_death_items::NullifyCreatureDeathItemsFixup;
use crate::fixups::creature::strip_creature_subgraph_additive_race::StripCreatureSubgraphAdditiveRaceFixup;
use crate::fixups::creature::synthesize_weapon_innr::SynthesizeWeaponInnrFixup;
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
use crate::fixups::havok::filter_unreferenced_behaviors::FilterUnreferencedBehaviorsFixup;
use crate::fixups::havok::fix_character_rig_path::FixCharacterRigPathFixup;
use crate::fixups::havok::fix_subcreature_skeleton_paths::FixSubcreatureSkeletonPathsFixup;
use crate::fixups::havok::inject_animation_names::InjectAnimationNamesFixup;
use crate::fixups::havok::inject_hitframe_events::InjectHitframeEventsFixup;
use crate::fixups::havok::repair_weapon_charge_reference_frames::RepairWeaponChargeReferenceFramesFixup;
use crate::fixups::havok::strip_source_game_events::StripSourceGameEventsFixup;
use crate::fixups::inject_cobjs_for_omods::InjectCobjsForOmodsFixup;
use crate::fixups::inject_required_child_blocks::InjectRequiredChildBlocksFixup;
use crate::fixups::inject_weap_extra_data::InjectWeapExtraDataFixup;
use crate::fixups::ltex_txst_synth::LtexTxstSynthFixup;
use crate::fixups::normalize_fo76_pack_templates::NormalizeFo76PackTemplatesFixup;
use crate::fixups::normalize_fo76_weather::NormalizeFo76WeatherFixup;
use crate::fixups::null_dangling_own_plugin_refs::NullDanglingOwnPluginRefsFixup;
use crate::fixups::preserve_packin_storage_cells::PreservePackinStorageCellsFixup;
use crate::fixups::prune_orphaned_records::PruneOrphanedRecordsFixup;
use crate::fixups::recover_fo76_leveled_list_values::RecoverFo76LeveledListValuesFixup;
use crate::fixups::remap_idle_anchor_actions::RemapIdleAnchorActionsFixup;
use crate::fixups::remap_light_gobo_to_fo4_base::RemapLightGoboToFo4BaseFixup;
use crate::fixups::repair_omod_target_keywords::RepairOmodTargetKeywordsFixup;
use crate::fixups::repair_quest_completion_xp::RepairQuestCompletionXpFixup;
use crate::fixups::repair_radio_scene_properties::RepairRadioScenePropertiesFixup;
use crate::fixups::resolve_addon_node_indices::ResolveAddonNodeIndicesFixup;
use crate::fixups::resolve_injected_stub_refs::ResolveInjectedStubRefsFixup;
use crate::fixups::restrict_translated_npc_for_slice::RestrictTranslatedNpcForSliceFixup;
use crate::fixups::rewrite_raw_lctn_formids::RewriteRawLctnFormIdsFixup;
use crate::fixups::rewrite_raw_object_template_formids::RewriteRawObjectTemplateFormIdsFixup;
use crate::fixups::rewrite_raw_wrld_large_refs::RewriteRawWrldLargeRefsFixup;
use crate::fixups::strip_perk_leveled_lists_from_containers::StripPerkLeveledListsFromContainersFixup;
use crate::fixups::sweep_unmapped_formkeys::SweepUnmappedFormKeysFixup;
use crate::fixups::sync_armo_hand_slots_from_addons::SyncArmoHandSlotsFromAddonsFixup;
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
        Segment::Fixup(|| Box::new(LtexTxstSynthFixup)),
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
        Segment::Fixup(|| Box::new(ApplyFo76WorkshopCatalogFixup)),
        Segment::Fixup(|| Box::new(SyncArmoHandSlotsFromAddonsFixup)),
        Segment::Fixup(|| Box::new(ExpandArmaRacesFromArmorRaceFixup)),
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
        Segment::Fixup(|| Box::new(RepairRadioScenePropertiesFixup)),
        Segment::Fixup(|| Box::new(DropUntranslatableLoadscreenRecordsFixup)),
        Segment::Fixup(|| Box::new(FixWaterSpellRefsFixup)),
        Segment::Fixup(|| Box::new(CleanLeveledItemEntriesFixup)),
        Segment::Fixup(|| Box::new(StripPerkLeveledListsFromContainersFixup)),
        Segment::Fixup(|| Box::new(RecoverFo76LeveledListValuesFixup)),
        Segment::Fixup(|| Box::new(FixStagSoundRefsFixup)),
        Segment::Fixup(|| Box::new(DropIncompatiblePlayerIdlesFixup)),
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
        Segment::Fixup(|| Box::new(SynthesizeWeaponInnrFixup)),
        Segment::Fixup(|| Box::new(FlattenNestedTraitsLvlnsFixup)),
        Segment::Fixup(|| Box::new(MaterializeLeveledTemplateNpcsFixup)),
        Segment::Fixup(|| Box::new(RestrictTranslatedNpcForSliceFixup)),
        Segment::Fixup(|| Box::new(InjectHumanNpcHeadPartsFixup)),
        Segment::Fixup(|| Box::new(StripUnbakedHumanNpcFaceMorphsFixup)),
        Segment::Fixup(|| Box::new(StripInvalidNpcFaceMorphsFixup)),
        Segment::Fixup(|| Box::new(FixCreatureNpcRecordsFixup)),
        Segment::Fixup(|| Box::new(NormalizeCreatureLvlnTemplateChainsFixup)),
        Segment::Fixup(|| Box::new(FixCreatureWeaponsAndRecordsFixup)),
        Segment::Fixup(|| Box::new(FixCreatureRaceRecordsFixup)),
        Segment::Fixup(|| Box::new(CleanCreatureEspCheckFieldsFixup)),
        Segment::Fixup(|| Box::new(AugmentCreatureFactionsFixup)),
        Segment::Fixup(|| Box::new(NullifyCreatureDeathItemsFixup)),
        Segment::Fixup(|| Box::new(StripCreatureSubgraphAdditiveRaceFixup)),
        Segment::Fixup(|| Box::new(FixCreatureWeaponFireSecondsFixup)),
        Segment::Sweep("sweep@41", || vec![Box::new(CleanupBodypartDataVisitor)]),
        Segment::Fixup(|| Box::new(StripSourceGameEventsFixup)),
        Segment::Fixup(|| Box::new(InjectHitframeEventsFixup)),
        Segment::Fixup(|| Box::new(RepairWeaponChargeReferenceFramesFixup)),
        Segment::Fixup(|| Box::new(FilterUnreferencedBehaviorsFixup)),
        Segment::Fixup(|| Box::new(InjectAnimationNamesFixup)),
        Segment::Fixup(|| Box::new(FixCharacterRigPathFixup)),
        Segment::Fixup(|| Box::new(FixSubcreatureSkeletonPathsFixup)),
        Segment::Fixup(|| Box::new(InjectRequiredChildBlocksFixup)),
        Segment::Sweep("sweep@49", || vec![Box::new(StripAtxCobjConditionsVisitor)]),
        Segment::Fixup(|| Box::new(HarvestModtFixup)),
        Segment::Fixup(|| Box::new(PruneOrphanedRecordsFixup)),
        Segment::Fixup(|| Box::new(RepairOmodTargetKeywordsFixup)),
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
        assert!(!plan_names.contains(&"filter_lchar_template_npcs"));
        assert!(plan_names.contains(&"inject_animation_names"));
        assert!(plan_names.contains(&"normalize_creature_lvln_template_chains"));
        assert!(plan_names.contains(&"normalize_fo76_weather"));
        assert!(plan_names.contains(&"apply_fo76_workshop_catalog"));
        assert!(plan_names.contains(&"repair_quest_completion_xp"));
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
