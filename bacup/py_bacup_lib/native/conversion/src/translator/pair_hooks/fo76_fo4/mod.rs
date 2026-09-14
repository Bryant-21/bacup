//! Fo76Fo4Hook: FO76→FO4 pair-level record hook.
//!
//! - `pre_translate` drops every subrecord in `GLOBAL_DROP_SIGS`.
//! - `is_effects_synthetic` names the record types whose `Effects` group the
//!   orchestrator synthesizes rather than decoding from the source ESP.
//! - `translate_effects_key` re-keys `Effects` fields on ALCH/ENCH/SPEL/PERK;
//!   the orchestrator applies it during field dispatch.

use super::model_paths;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};
use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use esp_authoring_core::xcri::{decode_fo76, encode_fo4};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};

mod actors;
mod common;
mod conditions;
mod dialogue;
mod expedition_aliases;
mod furniture;
mod leveled_lists;
mod lights;
mod magic;
mod misc;
mod object_mods;
mod object_template_inherit;
mod packages;
mod projectiles;
mod quest_vmad;
mod quests;
mod textures;
mod vendors;
mod weather;
mod workshop;
mod world;

use actors::*;
use common::*;
use conditions::*;
use dialogue::*;
use expedition_aliases::*;
use furniture::*;
use leveled_lists::*;
use lights::*;
use magic::*;
use misc::*;
use object_mods::*;
use quest_vmad::*;
use quests::*;
use textures::*;
use workshop::*;
use world::*;

pub(crate) use conditions::{
    ConditionFormCatalog, FO76_REMAPPED_CONDITION_FUNCTION_IDS, build_condition_form_catalog,
    close_condition_gate, install_condition_form_catalog,
    is_fo4_incompatible_condition_function_id,
};
#[allow(unused_imports)]
pub(crate) use dialogue::{
    PlayerDialogueInfoSplit, ScenDialogueAction, XDI_MASTER_NAME, XDI_SCENE_KEYWORD_FORM_ID,
    XdiDialoguePlan, apply_xdi_dial_info_count, build_xdi_dialogue_plan,
    combined_player_dialogue_info_candidates, info_tree_links, is_combined_player_dialogue_root,
    retarget_player_info_tree_links, scen_dialogue_actions, scope_start_scene_info_to_actor_alias,
    split_fo76_combined_player_dialogue_info,
};
pub use magic::{EFFECTS_SYNTHETIC_RECORD_SIGS, EffectsKeyRoute};
pub(crate) use misc::{fo76_misc_static_model_variant, rewritten_fo76_font_aliases_for_fo4};
pub(crate) use object_template_inherit::{
    build_inherited_object_template_catalog, install_inherited_object_template_catalog,
};
pub(crate) use quest_vmad::{
    qust_has_untranslatable_event_alias, qust_has_untranslatable_event_alias_for_source,
};
pub(crate) use quests::{
    qust_eid_is_dialogue_conversation, qust_uses_player_connect_autostart_fallback,
};
pub(crate) use weather::fo76_fo4_voli_gdry_substitution_mappings;

/// FO76→FO4 pair hook.
pub struct Fo76Fo4Hook;

impl PairHook for Fo76Fo4Hook {
    /// Drop FO76-only global fields before field translation begins.
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::normalize_weapon_cone_force(ctx, record);
        Self::normalize_workshop_power_connection_keyword(ctx.interner, record);
        Self::route_skip_havok_misc_to_static_model(ctx.interner, record);
        Self::strip_bee_swarm_ant_limb_replacements(ctx.interner, record);
        // Before any condition normalization: the spliced-in rows must go
        // through the same remap / drop / FormID paths as the record's own.
        Self::inline_fo76_condition_forms(record);
        Self::normalize_fo76_get_is_player_conditions(record);
        Self::normalize_fo76_editor_location_has_keyword_conditions(record);
        Self::normalize_child_underarmor_slots(record);
        Self::normalize_arma_upper_body_skin_slots(record);
        // Before the global field drop, so a spliced-in object template is
        // subject to exactly the same drops as a natively carried one.
        Self::materialize_inherited_object_template(record);
        Self::normalize_chinese_stealth_arma_pipboy_slot(ctx.interner, record);
        Self::drop_global_fields(record);
        Self::lower_music_instrument_to_vmad(ctx, record);
        crate::target_normalize::strip_unsupported_race_tint_tables(record);
        Self::convert_nif_backed_empty_scol_to_stat(ctx.interner, record);
        Self::strip_wrld_runtime_tables(record);
        Self::strip_redundant_omod_target_keywords(ctx.interner, record);
        Self::strip_tesla_cannon_receiver_model(ctx.interner, record);
        Self::strip_material_omod_models(ctx.interner, record);
        Self::normalize_npc_perk_entries(ctx.interner, record);
        Self::normalize_npc_raw_form_refs(ctx.interner, record);
        Self::normalize_cont_raw_form_refs(ctx.interner, record);
        Self::normalize_info_response_flags(record);
        Self::strip_info_editor_id(record);
        Self::strip_orphan_term_conditions(record);
        Self::normalize_refr_map_marker_tnam(record);
        Self::rename_furniture_marker_parameters(record);
        Self::inherit_storm_cell_fog_from_fo4_template(ctx.interner, record);
        Self::inherit_ambient_light_for_unlit_fo76_cells(record);
        Self::strip_zero_health_cont_destructibles(ctx.interner, record);
        Self::normalize_explosive_mstt_destruction_flags(record);
        Self::normalize_note_scene_ref(record);
        Self::convert_innr_filter_to_fo4_target(ctx.interner, record);
        Self::convert_txst_decal_data_to_fo4_layout(record);
        Self::convert_mgef_data_to_fo4_layout(record);
        Self::normalize_scen_headtracking_aliases(record);
        Self::normalize_wayward_blueprint_package_completion(record);
        Self::normalize_scen_player_dialogue_choices(ctx.interner, record);
        Self::convert_fo76_leveled_list_entries(ctx.interner, record);
        Self::convert_or_drop_cell_combined_reference_index(ctx.interner, record);
        Self::convert_qust_data_to_fo4_dnam(ctx.interner, record);
        Self::adapt_direct_start_quest_aliases(ctx.interner, record);
        Self::strip_qust_runtime_scopes(ctx.interner, record);
        normalize_re_alias_vmad_properties(record);
        Self::translate_weather_volumetric_lighting(record);
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::dedupe_mapped_keyword_arrays(record);
        rewrite_fo76_font_aliases_in_record(record, ctx.interner);
        Self::ensure_anio_unload_event(ctx.interner, record);
        Self::strip_wsbunker_intercom_radio(ctx.interner, record);
        Self::drop_fo4_incompatible_conditions(ctx.interner, record);
        Self::normalize_workshop_cobj(ctx.interner, record);
        Self::convert_mgef_data_to_fo4_layout(record);
        Self::normalize_perk_entry_layout(record);
        Self::drop_mstt_omod_data(ctx.interner, record);
        Self::strip_invalid_object_mod_properties(ctx.interner, record);
        Self::normalize_omod_material_swap_functions(ctx.interner, record);
        Self::retarget_power_armor_paint_attach_point(ctx.interner, record);
        Self::expose_power_armor_paint_slot(ctx.interner, record);
        Self::repair_liberator_body_omod_attach_point(ctx.interner, record);
        Self::ensure_liberator_race_armor_attach_slot(ctx.interner, record);
        Self::normalize_npc_raw_form_refs(ctx.interner, record);
        Self::normalize_rd01_assassin_combat_style(ctx.interner, record);
        Self::ensure_w05_nuke_reaction_base_outfit(ctx, record);
        Self::convert_or_drop_region_objects(ctx.interner, record);
        Self::map_fo76_fallback_package_procedure(ctx.interner, record);
        Self::normalize_fo76_pack_procedure_tree(ctx.interner, record);
        Self::strip_fo76_only_subrecord_tails(record);
        Self::normalize_idlm_flags(record);
        Self::ensure_default_one_state_activator_animation(record);
        Self::drop_furniture_workbench_data_without_bench_type(ctx.interner, record);
        Self::ensure_power_armor_furniture_vmad(ctx.interner, record);
        Self::ensure_workbench_script_vmad(ctx.interner, record);
        Self::strip_term_looping_sound_snam(record);
        Self::clear_invalid_furniture_active_marker_bits(record);
        Self::ensure_terminal_player_path_keyword(ctx.interner, record);
        Self::ensure_light_radius(ctx.interner, record);
        Self::normalize_sunlight_radius_for_fo4(ctx.interner, record);
        Self::normalize_light_data_for_fo4(ctx.interner, record);
        Self::normalize_cage_bulb_gobo_light_for_fo4(ctx.interner, record);
        Self::clamp_shadow_caster_radius_for_fo4(ctx.interner, record);
        Self::ensure_light_fade_value(record);
        Self::translate_weather_visual_effect(ctx.interner, record);
        Self::ensure_flora_ingredient_production(record);
        Self::normalize_vending_machine_vendor_faction(record);
        model_paths::normalize_model_paths(ctx.interner, record);
        Ok(())
    }

    /// No synthetic records produced by this pair.
    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::RecordFlags;
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use crate::translator::{Game, TranslateResult, Translator};
    use smallvec::SmallVec;

    include!("tests/test_support.rs");
    include!("tests/conditions.rs");
    include!("tests/leveled_lists.rs");
    include!("tests/quests.rs");
    include!("tests/textures.rs");
    include!("tests/quest_vmad.rs");
    include!("tests/dialogue.rs");
    include!("tests/expedition_aliases.rs");
    include!("tests/w05_radical_dialogue.rs");
    include!("tests/object_mods.rs");
    include!("tests/lights.rs");
    include!("tests/workshop.rs");
    include!("tests/packages.rs");
    include!("tests/projectiles.rs");
    include!("tests/magic.rs");
    include!("tests/actors.rs");
    include!("tests/world.rs");
    include!("tests/furniture.rs");
    include!("tests/misc.rs");
}
