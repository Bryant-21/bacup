//! Fo76Fo4Hook — FO76→FO4 pair-level record hook.
//!
//! Ports the retired Python `Fo76ToFo4Hooks` implementation.
//!
//! # Behaviors ported
//!
//! 1. **Global field drop** (`pre_translate`) — removes subrecords whose four-byte
//!    sig matches any entry in `GLOBAL_DROP_SIGS`. Maps to `_GLOBAL_DROP_FIELDS`
//!    in the Python source.
//!
//! 2. **Synthetic-source-field marking** — `synthetic_source_fields()` returns the
//!    set of field names the translator should treat as synthesized (not decoded
//!    from the source ESP) for certain record types. Exposed as a pure method; the
//!    orchestrator calls it during setup. No `Record` mutation needed.
//!
//! 3. **Effects key routing** (`pre_translate` metadata, via `PairCtx`) — for
//!    ALCH/ENCH/SPEL/PERK records carrying an `Effects`-bearing subrecord, certain
//!    field names must be re-keyed. The routing table is expressed as a pure method
//!    `translate_effects_key`; the orchestrator applies it during field dispatch.
//!    No `Record` mutation needed here.
//!
//! `pre_process_source` is a no-op in the Python source and has no Rust equivalent.

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
mod furniture;
mod leveled_lists;
mod lights;
mod magic;
mod misc;
mod object_mods;
mod packages;
mod quest_vmad;
mod quests;
mod vendors;
mod weather;
mod workshop;
mod world;

use actors::*;
use common::*;
use conditions::*;
use dialogue::*;
use furniture::*;
use leveled_lists::*;
use lights::*;
use magic::*;
use misc::*;
use object_mods::*;
use quest_vmad::*;
use quests::*;
use workshop::*;
use world::*;

pub(crate) use conditions::{
    FO76_REMAPPED_CONDITION_FUNCTION_IDS, is_fo4_incompatible_condition_function_id,
};
#[allow(unused_imports)]
pub(crate) use dialogue::{
    PlayerDialogueInfoSplit, ScenDialogueAction, XDI_MASTER_NAME, XDI_SCENE_KEYWORD_FORM_ID,
    XdiDialoguePlan, apply_xdi_dial_info_count, apply_xdi_scene_player_padding,
    build_xdi_dialogue_plan, combined_player_dialogue_info_candidates, scen_dialogue_actions,
    split_fo76_combined_player_dialogue_info,
};
pub use magic::{EFFECTS_SYNTHETIC_RECORD_SIGS, EffectsKeyRoute};
pub(crate) use misc::{
    FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET, namespace_fo76_radio_frequency,
    rewritten_fo76_font_aliases_for_fo4,
};
pub(crate) use quest_vmad::qust_has_untranslatable_event_alias;
pub(crate) use quests::qust_eid_is_dialogue_conversation;
pub(crate) use weather::fo76_fo4_voli_gdry_substitution_mappings;

/// FO76→FO4 pair hook.
pub struct Fo76Fo4Hook;

impl PairHook for Fo76Fo4Hook {
    /// Drop FO76-only global fields before field translation begins.
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::normalize_fo76_get_is_player_conditions(record);
        Self::normalize_fo76_editor_location_has_keyword_conditions(record);
        Self::normalize_arma_upper_body_skin_slots(record);
        Self::normalize_chinese_stealth_arma_pipboy_slot(ctx.interner, record);
        Self::drop_global_fields(record);
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
        Self::strip_zero_health_cont_destructibles(ctx.interner, record);
        Self::normalize_note_scene_ref(record);
        Self::convert_mgef_data_to_fo4_layout(record);
        Self::normalize_scen_headtracking_aliases(record);
        Self::normalize_scen_player_dialogue_choices(ctx.interner, record);
        Self::convert_fo76_leveled_list_entries(ctx.interner, record);
        Self::convert_or_drop_cell_combined_reference_index(ctx.interner, record);
        Self::convert_qust_data_to_fo4_dnam(ctx.interner, record);
        Self::strip_qust_runtime_scopes(ctx.interner, record);
        Self::translate_weather_volumetric_lighting(record);
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        rewrite_fo76_font_aliases_in_record(record, ctx.interner);
        Self::strip_wsbunker_intercom_radio(ctx.interner, record);
        Self::namespace_radio_receiver_frequency(ctx.interner, record);
        Self::drop_fo4_incompatible_conditions(ctx.interner, record);
        Self::normalize_workshop_cobj(ctx.interner, record);
        Self::convert_mgef_data_to_fo4_layout(record);
        Self::drop_perk_vmad(record);
        Self::drop_mstt_omod_data(ctx.interner, record);
        Self::strip_invalid_object_mod_properties(ctx.interner, record);
        Self::normalize_omod_material_swap_functions(ctx.interner, record);
        Self::repair_liberator_body_omod_attach_point(ctx.interner, record);
        Self::normalize_npc_raw_form_refs(ctx.interner, record);
        Self::normalize_rd01_assassin_combat_style(ctx.interner, record);
        Self::convert_or_drop_region_objects(ctx.interner, record);
        Self::map_fo76_fallback_package_procedure(ctx.interner, record);
        Self::normalize_fo76_pack_procedure_tree(ctx.interner, record);
        Self::strip_fo76_only_subrecord_tails(record);
        Self::normalize_idlm_flags(record);
        Self::ensure_power_armor_furniture_vmad(ctx.interner, record);
        Self::ensure_workbench_script_vmad(ctx.interner, record);
        Self::strip_term_looping_sound_snam(record);
        Self::clear_invalid_furniture_active_marker_bits(record);
        Self::ensure_terminal_player_path_keyword(ctx.interner, record);
        Self::ensure_light_radius(ctx.interner, record);
        Self::normalize_light_data_for_fo4(ctx.interner, record);
        Self::normalize_cage_bulb_gobo_light_for_fo4(ctx.interner, record);
        Self::ensure_light_fade_value(record);
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
    include!("tests/quest_vmad.rs");
    include!("tests/dialogue.rs");
    include!("tests/object_mods.rs");
    include!("tests/lights.rs");
    include!("tests/workshop.rs");
    include!("tests/packages.rs");
    include!("tests/magic.rs");
    include!("tests/actors.rs");
    include!("tests/world.rs");
    include!("tests/furniture.rs");
    include!("tests/misc.rs");
}
