//! FnvFo4Hook: FNV→FO4 pair-level record hook.
//!
//! - `pre_translate` drops every `SCRI` subrecord.
//! - `capture_scri_target` returns the deferred-script-link target from a
//!   non-empty `SCRI`; the orchestrator calls it before the field is dropped.
//! - `post_translate` strips source-game prefixes that earlier conversion
//!   stages left on model paths.

mod acre;
mod armor;
mod common;
pub(crate) mod creature_catalog;
pub(crate) mod creature_dependencies;
pub(crate) mod creature_live_builder;
pub(crate) mod creature_live_preparations;
pub(crate) mod creature_motion;
pub(crate) mod creature_mvp_adapter;
pub(crate) mod creature_race_data;
pub(crate) mod creature_recipe;
pub(crate) mod gecko_creature;
mod landscape;
mod legacy_ammo;
mod legacy_ammo_effects;
mod legacy_appearance;
mod legacy_body_parts;
mod legacy_combat_styles;
mod legacy_debris;
mod legacy_explosions;
mod legacy_factions;
mod legacy_idles;
mod legacy_impact_datasets;
mod legacy_impacts;
mod legacy_keys;
mod legacy_leveled_items;
mod legacy_misc;
mod legacy_npc_appearance;
mod legacy_package_references;
mod legacy_packages;
mod legacy_race_appearance;
mod legacy_ragdolls;
mod legacy_statics;
mod legacy_trees;
mod leveled_creatures;
mod misc;
mod projectiles;
mod references;
mod serial;
mod sounds;
pub(crate) mod source_rig_bridge;
mod terminals;
mod weapon;

use super::fo4_layouts::{self, SourceFamily};
use super::model_paths;
use crate::ids::SubrecordSig;
use crate::record::{FieldValue, Record};
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};
pub(crate) use acre::{
    PlacedActorAliasResolver, PlacedActorAliasTarget, finalize_lowered_acre, lower_acre_signature,
    placed_actor_base_formkey,
};
use armor::{LegacyArmorSource, relayout_arma_models, relayout_armo_loader_fields};
use landscape::{normalize_legacy_land_layers, relayout_legacy_ltex};
use legacy_ammo::{LegacyAmmoSourceFamily, decode_legacy_ammo_dat2, lower_legacy_ammo};
use legacy_body_parts::{LegacyBodyPartSourceFamily, lower_legacy_body_part_data};
use legacy_combat_styles::lower_legacy_combat_style;
use legacy_debris::lower_legacy_debris;
use legacy_explosions::lower_legacy_explosion;
use legacy_factions::lower_legacy_faction;
use legacy_impact_datasets::lower_legacy_impact_dataset;
use legacy_impacts::{lower_legacy_impact, lower_legacy_texture_set};
use legacy_leveled_items::decode_legacy_leveled_item_entries;
use legacy_packages::lower_generic_legacy_package;
use legacy_statics::repair_legacy_static_model;
use legacy_trees::rewrite_legacy_speedtree_model;
pub(crate) use leveled_creatures::decode_legacy_lvlc_entries;
use leveled_creatures::lower_legacy_lvlc_to_lvln;
use misc::clear_legacy_bptd_ragdoll_payloads;
use projectiles::relayout_proj_data;
use references::{relayout_addn_dnam, relayout_refr_xrmr};
use sounds::rewrite_legacy_sound_descriptor;
use terminals::{drop_incompatible_legacy_fallout_conditions, rewrite_term_menu_rows};
use weapon::relayout_weap_equipment_type;

pub(crate) use sounds::legacy_sound_descriptor_substitution_mappings;
pub(crate) use weapon::{classify_hatchet, project_hatchet};

pub(crate) use serial::{
    LegacySerialDiagnostic, LegacySerialNormalizationState, LegacySerialNormalizeReport,
    normalize_legacy_serial_record_once,
};

/// FNV→FO4 pair hook.
pub struct FnvFo4Hook;
impl FnvFo4Hook {
    /// The single global-drop sig for FNV→FO4: `SCRI` (legacy Papyrus script ref).
    const DROP_SIG: [u8; 4] = *b"SCRI";

    pub(crate) fn is_unused_ingredient_sentinel(
        record: &Record,
        interner: &crate::sym::StringInterner,
    ) -> bool {
        record.sig.0 == *b"INGR"
            && record.form_key.local == 0x03_135B
            && record.eid.is_some_and(|editor_id| {
                interner.resolve(editor_id)
                    == Some("DoNotCreateNewIngredientsWeArentUsingThemInFallout")
            })
    }

    /// Drop the `SCRI` subrecord from the record before translation.
    fn drop_global_fields(record: &mut Record) {
        record.fields.retain(|entry| entry.sig.0 != Self::DROP_SIG);
    }

    fn drop_incompatible_fields(record: &mut Record, interner: &crate::sym::StringInterner) {
        // Legacy condition registry slots are not stable across games. These
        // ids resolve in FO4 as incompatible functions; id 53 in particular
        // becomes a four-parameter function that crashes the CK validator.
        drop_incompatible_legacy_fallout_conditions(record, interner);
        clear_legacy_bptd_ragdoll_payloads(record);
        match record.sig.0 {
            sig if sig == *b"ARMO" => {
                relayout_armo_loader_fields(record, LegacyArmorSource::Fnv, interner)
            }
            sig if sig == *b"ARMA" => relayout_arma_models(record, interner),
            sig if sig == *b"MUSC" => record.fields.retain(|entry| entry.sig.0 != *b"FNAM"),
            sig if sig == *b"INFO" => record.fields.retain(|entry| entry.sig.0 != *b"DNAM"),
            sig if sig == *b"TERM" => {
                // FNV SNAM is one four-byte looping-sound FormID. The FO4 v131
                // TERM loader uses the same 4CC for repeatable 24-byte sound
                // rows; preserving the FNV payload fail-fasts in that loader.
                // FNV PNAM is likewise a password NOTE, not FO4 marker color.
                record.fields.retain(
                    |entry| !matches!(entry.sig.0, field if field == *b"SNAM" || field == *b"PNAM"),
                );
                rewrite_term_menu_rows(record, interner);
            }
            sig if sig == *b"WEAP" => {
                record.fields.retain(|entry| entry.sig.0 != *b"NNAM");
                relayout_weap_equipment_type(record, interner);
            }
            // FO4 never carries ETYP on a consumable (231/231 vanilla ALCH have
            // none), so the legacy ordinal has no target meaning at all.
            sig if sig == *b"ALCH" || sig == *b"INGR" => {
                record.fields.retain(|entry| entry.sig.0 != *b"ETYP")
            }
            sig if sig == *b"NPC_" => {
                // FNV's NAM4 is Impact Material and NAM7 is body weight.  FO4
                // assigns those signatures to HeightMax and an unused float,
                // respectively, so neither payload can be carried verbatim.
                record.fields.retain(
                    |entry| !matches!(entry.sig.0, field if field == *b"NAM4" || field == *b"NAM7"),
                );
            }
            sig if sig == *b"REFR" => {
                record.fields.retain(|entry| entry.sig.0 != *b"XRDO");
                relayout_refr_xrmr(record, interner);
                fo4_layouts::normalize_refr_xloc(record, interner);
            }
            sig if sig == *b"ADDN" => {
                for entry in &mut record.fields {
                    if entry.sig.0 == *b"DNAM" {
                        entry.value =
                            relayout_addn_dnam(&entry.value, interner).unwrap_or(FieldValue::None);
                    }
                }
                record
                    .fields
                    .retain(|entry| entry.sig.0 != *b"DNAM" || entry.value != FieldValue::None);
            }
            sig if sig == *b"PROJ" => relayout_proj_data(record),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::LegacyFallout, interner)
            }
            sig if sig == *b"WTHR" => fo4_layouts::normalize_legacy_wthr(record, interner),
            sig if sig == *b"WATR" => fo4_layouts::normalize_legacy_watr(record, interner),
            sig if sig == *b"LTEX" => relayout_legacy_ltex(record, interner),
            sig if sig == *b"LAND" => normalize_legacy_land_layers(record, interner),
            sig if sig == *b"SOUN" => rewrite_legacy_sound_descriptor(record, interner),
            _ => {}
        }
    }

    /// Extract a legacy script-link key from `SCRI`, if present and non-empty.
    ///
    /// FNV's decoded `NPC_.SCRI` is a `FormKey`, while older callers may still
    /// supply the Python-era string form.  FormKeys render as the legacy
    /// `XXXXXX:Plugin.esm` syntax consumed by the FNV scripting phase.  The
    /// caller must invoke this **before** `pre_translate` drops `SCRI`.
    pub fn capture_scri_target(
        record: &Record,
        interner: &crate::sym::StringInterner,
    ) -> Option<String> {
        let scri_sig = SubrecordSig(*b"SCRI");
        let entry = record.fields.iter().find(|e| e.sig == scri_sig)?;
        match entry.value {
            crate::record::FieldValue::String(sym) => {
                let value = interner.resolve(sym)?;
                (!value.trim().is_empty()).then(|| value.to_string())
            }
            crate::record::FieldValue::FormKey(form_key) => {
                let plugin = interner.resolve(form_key.plugin)?;
                Some(format!("{:06X}:{plugin}", form_key.local))
            }
            _ => None,
        }
    }
}

impl PairHook for FnvFo4Hook {
    /// Drop FNV-only global fields (`SCRI`) before field translation begins.
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        if record.sig.0 == *b"LVLC" {
            lower_legacy_lvlc_to_lvln(record, ctx.interner)
                .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        }
        if project_hatchet(record, ctx.interner) {
            return Ok(());
        }
        if record.sig.0 == *b"AMMO" {
            if let Some(source_plugin_name) = ctx.source_plugin_name {
                let _ = decode_legacy_ammo_dat2(
                    record,
                    source_plugin_name,
                    ctx.source_master_names,
                    ctx.interner,
                );
            }
            lower_legacy_ammo(record, LegacyAmmoSourceFamily::Fnv, ctx.interner);
        }
        if record.sig.0 == *b"LVLI"
            && let Some(source_plugin_name) = ctx.source_plugin_name
        {
            decode_legacy_leveled_item_entries(
                record,
                source_plugin_name,
                ctx.source_master_names,
                ctx.interner,
            )
            .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        }
        if record.sig.0 == *b"PACK"
            && let Some(source_plugin_name) = ctx.source_plugin_name
        {
            lower_generic_legacy_package(
                record,
                crate::translator::pair_hooks::fnv_pack::LegacyPackSourceFamily::Fnv,
                source_plugin_name,
                ctx.source_master_names,
                ctx.interner,
            )
            .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        }
        lower_legacy_combat_style(record);
        lower_legacy_body_part_data(record, LegacyBodyPartSourceFamily::Fnv);
        lower_legacy_debris(record, "fnv", ctx.interner)
            .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        lower_legacy_explosion(record);
        if record.sig.0 == *b"FACT"
            && let Some(source_plugin_name) = ctx.source_plugin_name
        {
            lower_legacy_faction(
                record,
                source_plugin_name,
                ctx.source_master_names,
                ctx.interner,
            )
            .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        }
        if record.sig.0 == *b"IPDS"
            && let Some(source_plugin_name) = ctx.source_plugin_name
        {
            lower_legacy_impact_dataset(
                record,
                source_plugin_name,
                ctx.source_master_names,
                ctx.interner,
            );
        }
        lower_legacy_impact(record, ctx.interner);
        lower_legacy_texture_set(record, ctx.interner);
        Self::drop_global_fields(record);
        Self::drop_incompatible_fields(record, ctx.interner);
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        model_paths::normalize_model_paths(ctx.interner, record);
        repair_legacy_static_model(ctx.interner, record);
        rewrite_legacy_speedtree_model(ctx.interner, record);
        fo4_layouts::normalize_txst_smooth_spec(record, SourceFamily::LegacyFallout, ctx.interner);
        Ok(())
    }

    /// No synthetic records produced by this pair.
    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

/// FO3→FO4 hook restricted to cross-game layouts proven shared with FNV.
///
/// FO3 records must not run the FNV-specific TERM/REFR/ADDN rewrites in
/// `FnvFo4Hook`; only the explicit PROJ/REFR.XLOC/EFSH/WTHR contracts handled
/// here and the schema-identical legacy ARMA model layout are shared.
pub struct Fo3Fo4Hook;

impl PairHook for Fo3Fo4Hook {
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        if record.sig.0 == *b"LVLC" {
            lower_legacy_lvlc_to_lvln(record, ctx.interner)
                .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        }
        drop_incompatible_legacy_fallout_conditions(record, ctx.interner);
        clear_legacy_bptd_ragdoll_payloads(record);
        lower_legacy_body_part_data(record, LegacyBodyPartSourceFamily::Fo3);
        lower_legacy_debris(record, "fo3", ctx.interner)
            .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        lower_legacy_explosion(record);
        if record.sig.0 == *b"FACT"
            && let Some(source_plugin_name) = ctx.source_plugin_name
        {
            lower_legacy_faction(
                record,
                source_plugin_name,
                ctx.source_master_names,
                ctx.interner,
            )
            .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        }
        if record.sig.0 == *b"AMMO" {
            lower_legacy_ammo(record, LegacyAmmoSourceFamily::Fo3, ctx.interner);
        }
        if record.sig.0 == *b"LVLI"
            && let Some(source_plugin_name) = ctx.source_plugin_name
        {
            decode_legacy_leveled_item_entries(
                record,
                source_plugin_name,
                ctx.source_master_names,
                ctx.interner,
            )
            .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        }
        if record.sig.0 == *b"PACK"
            && let Some(source_plugin_name) = ctx.source_plugin_name
        {
            lower_generic_legacy_package(
                record,
                crate::translator::pair_hooks::fnv_pack::LegacyPackSourceFamily::Fo3,
                source_plugin_name,
                ctx.source_master_names,
                ctx.interner,
            )
            .map_err(crate::translator::pair_hook::HookError::Runtime)?;
        }
        lower_legacy_combat_style(record);
        if record.sig.0 == *b"IPDS"
            && let Some(source_plugin_name) = ctx.source_plugin_name
        {
            lower_legacy_impact_dataset(
                record,
                source_plugin_name,
                ctx.source_master_names,
                ctx.interner,
            );
        }
        lower_legacy_impact(record, ctx.interner);
        lower_legacy_texture_set(record, ctx.interner);
        match record.sig.0 {
            sig if sig == *b"ARMO" => {
                relayout_armo_loader_fields(record, LegacyArmorSource::Fo3, ctx.interner)
            }
            sig if sig == *b"ARMA" => relayout_arma_models(record, ctx.interner),
            sig if sig == *b"WEAP" => relayout_weap_equipment_type(record, ctx.interner),
            sig if sig == *b"ALCH" || sig == *b"INGR" => {
                record.fields.retain(|entry| entry.sig.0 != *b"ETYP")
            }
            sig if sig == *b"PROJ" => relayout_proj_data(record),
            sig if sig == *b"REFR" => fo4_layouts::normalize_refr_xloc(record, ctx.interner),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::LegacyFallout, ctx.interner)
            }
            sig if sig == *b"WTHR" => fo4_layouts::normalize_legacy_wthr(record, ctx.interner),
            sig if sig == *b"WATR" => fo4_layouts::normalize_legacy_watr(record, ctx.interner),
            sig if sig == *b"LTEX" => relayout_legacy_ltex(record, ctx.interner),
            sig if sig == *b"LAND" => normalize_legacy_land_layers(record, ctx.interner),
            sig if sig == *b"SOUN" => rewrite_legacy_sound_descriptor(record, ctx.interner),
            _ => {}
        }
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        model_paths::normalize_model_paths(ctx.interner, record);
        repair_legacy_static_model(ctx.interner, record);
        rewrite_legacy_speedtree_model(ctx.interner, record);
        fo4_layouts::normalize_txst_smooth_spec(record, SourceFamily::LegacyFallout, ctx.interner);
        Ok(())
    }

    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, ResolutionMode};
    use crate::ids::{FormKey, SigCode};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::sym::StringInterner;
    use crate::translator::Game;

    include!("tests/test_support.rs");
    include!("tests/misc.rs");
    include!("tests/projectiles.rs");
    include!("tests/armor.rs");
    include!("tests/weapon_equipment_type.rs");
    include!("tests/acre.rs");
    include!("tests/statics.rs");
    include!("tests/trees.rs");
    include!("tests/sounds.rs");
    include!("tests/terminals.rs");
    include!("tests/references.rs");
    include!("tests/landscape.rs");
    include!("tests/serial.rs");
}
