mod misc;
mod sound;
mod weather;
mod world;

use super::fo4_layouts::{self, SourceFamily};
use super::model_paths;
use crate::record::Record;
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};

pub(crate) use weather::{
    normalize_skyrim_weather, rewrite_skyrim_weather_master_refs,
    skyrimse_fo4_voli_gdry_substitution_mappings,
};

pub struct SkyrimSeFo4Hook;
impl PairHook for SkyrimSeFo4Hook {
    fn pre_translate(&self, _ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::drop_vmad(record);
        Self::drop_incompatible_debr_modt(record);
        Self::normalize_refr_map_marker_tnam(record);
        Self::normalize_sopm_attenuation(record);
        match record.sig.0 {
            sig if sig == *b"REFR" => fo4_layouts::normalize_refr_xloc(record, _ctx.interner),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::SkyrimSe, _ctx.interner)
            }
            sig if sig == *b"WTHR" => normalize_skyrim_weather(record, _ctx.interner),
            sig if sig == *b"PROJ" => fo4_layouts::normalize_skyrim_proj(record),
            _ => {}
        }
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        model_paths::normalize_model_paths(ctx.interner, record);
        Ok(())
    }

    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

impl SkyrimSeFo4Hook {
    fn drop_vmad(record: &mut Record) {
        record.fields.retain(|entry| entry.sig.0 != *b"VMAD");
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
    use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};

    include!("tests/misc.rs");
    include!("tests/world.rs");
    include!("tests/sound.rs");
}
