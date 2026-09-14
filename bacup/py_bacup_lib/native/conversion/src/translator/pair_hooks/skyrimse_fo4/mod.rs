mod armor;
mod misc;
mod npc;
mod runtime_magic;
mod sound;
mod weather;
mod world;

use super::fo4_layouts::{self, SourceFamily};
use super::model_paths;
use crate::record::Record;
use crate::translator::pair_hook::{HookError, HookResult, PairCtx, PairHook};

#[allow(unused_imports)]
pub(crate) use runtime_magic::{
    MagicArchetype, MagicCast, MagicComponent, MagicComponentRequirements, MagicContractError,
    MagicDelivery, MagicDonorIdentity, MagicDonorRole, MagicEffectEdge, MagicEffectFlag,
    MagicLinkedRecordRequirement, MagicLinkedRole, MagicLoweringPlan, MagicLoweringReceipt,
    MagicSupport, MagicUnsupportedReason, MagicVmadIntent, classify_magic_component,
    lower_magic_record_for_fo4, lower_supported_magic_record, magic_donor_identity,
    normalize_standalone_vmad_for_fo4, strip_source_vmad, validate_magic_donor,
};
pub(crate) use weather::{
    normalize_skyrim_weather, rewrite_skyrim_weather_master_refs,
    skyrimse_fo4_voli_gdry_substitution_mappings,
};

pub struct SkyrimSeFo4Hook;
impl PairHook for SkyrimSeFo4Hook {
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        crate::skyrimse_fo4_runtime::weapon::project_simple_melee_weapon(record, ctx.interner);
        runtime_magic::lower_magic_record_for_fo4(record, ctx.interner)
            .map_err(|error| HookError::Runtime(error.to_string()))?;
        if matches!(
            &record.sig.0,
            b"QUST" | b"DIAL" | b"INFO" | b"SCEN" | b"SMBN" | b"SMEN" | b"SMQN"
        ) {
            runtime_magic::strip_source_vmad(record);
        }
        runtime_magic::normalize_standalone_vmad_for_fo4(record);
        Self::drop_incompatible_debr_modt(record);
        Self::normalize_refr_map_marker_tnam(record);
        Self::normalize_sopm_attenuation(record);
        match record.sig.0 {
            sig if sig == *b"REFR" => fo4_layouts::normalize_refr_xloc(record, ctx.interner),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::SkyrimSe, ctx.interner)
            }
            sig if sig == *b"WTHR" => normalize_skyrim_weather(record, ctx.interner),
            sig if sig == *b"PROJ" => fo4_layouts::normalize_skyrim_proj(record),
            _ => {}
        }
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        armor::normalize_skyrim_armor(record, ctx.interner);
        npc::normalize_skyrim_npc(record, ctx.interner);
        model_paths::normalize_model_paths(ctx.interner, record);
        fo4_layouts::normalize_txst_smooth_spec(record, SourceFamily::SkyrimSe, ctx.interner);
        Ok(())
    }

    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::schema::AuthoringSchema;
    use crate::source_read::decode_record_from_parsed;
    use crate::sym::StringInterner;
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use crate::translator::target_hook::TargetCtx;
    use crate::translator::{Game, TranslateResult, Translator};
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_add_master_native,
        plugin_handle_close_native, plugin_handle_load_no_py, plugin_handle_new_no_py,
        plugin_handle_save_no_py, plugin_handle_store_ref,
    };

    include!("tests/misc.rs");
    include!("tests/armor.rs");
    include!("tests/npc.rs");
    include!("tests/magic.rs");
    include!("tests/runtime_magic.rs");
    include!("tests/world.rs");
    include!("tests/sound.rs");
    include!("tests/weapon.rs");
}
