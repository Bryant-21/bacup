//! FnvFo4Hook — FNV→FO4 pair-level record hook.
//!
//! Ports the retired Python `FnvToFo4Hooks` implementation.
//!
//! # Behaviors ported
//!
//! 1. **Global field drop** (`pre_translate`) — removes subrecords whose
//!    four-byte sig is `SCRI`. Maps to `global_drop_fields=frozenset({"SCRI"})`
//!    in the Python constructor.
//!
//! 2. **SCRI metadata capture** — the Python `capture_metadata` extracts a
//!    deferred-script-link when a non-empty `SCRI` string is present. In the
//!    Rust port this is represented as a pure method `capture_scri_target`
//!    that returns the target string from the `SCRI` field value when present.
//!    The orchestrator calls it before the field is dropped.
//!
//! `post_translate` also normalizes model paths so any source-game-prefixed
//! model refs from earlier conversion stages are stripped before FO4 output.

mod armor;
mod common;
mod misc;
mod projectiles;
mod references;
mod serial;
mod terminals;

use super::fo4_layouts::{self, SourceFamily};
use super::model_paths;
use crate::ids::SubrecordSig;
use crate::record::{FieldValue, Record};
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};
use armor::{LegacyArmorSource, relayout_arma_models, relayout_armo_loader_fields};
use misc::clear_legacy_bptd_ragdoll_payloads;
use projectiles::relayout_proj_data;
use references::{relayout_addn_dnam, relayout_refr_xrmr};
use terminals::rewrite_term_menu_rows;

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
        clear_legacy_bptd_ragdoll_payloads(record);
        match record.sig.0 {
            sig if sig == *b"ARMO" => {
                relayout_armo_loader_fields(record, LegacyArmorSource::Fnv, interner)
            }
            sig if sig == *b"ARMA" => relayout_arma_models(record, interner),
            // DEBR is a repeated DATA/MODT row format. FNV MODT bytes use the
            // source game's layout; the post-asset phase rebuilds FO4 MODT.
            sig if sig == *b"DEBR" => record.fields.retain(|entry| entry.sig.0 != *b"MODT"),
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
            sig if sig == *b"WEAP" => record.fields.retain(|entry| entry.sig.0 != *b"NNAM"),
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
            _ => {}
        }
    }

    /// Extract the SCRI target string from the record, if present and non-empty.
    ///
    /// Returns `None` when `SCRI` is absent, when its value is not a `String`,
    /// or when the resolved string is blank. The caller should invoke this
    /// **before** calling `pre_translate` (which drops `SCRI`).
    ///
    /// Mirrors Python:
    /// ```python
    /// scri_target = source.get("SCRI")
    /// if not isinstance(scri_target, str) or not scri_target.strip():
    ///     return {}
    /// return {"deferred_script_link": {"record_type": ..., "scri_target": scri_target}}
    /// ```
    pub fn capture_scri_target<'r>(
        record: &'r Record,
        interner: &'r crate::sym::StringInterner,
    ) -> Option<&'r str> {
        let scri_sig = SubrecordSig(*b"SCRI");
        let entry = record.fields.iter().find(|e| e.sig == scri_sig)?;
        let sym = match entry.value {
            crate::record::FieldValue::String(s) => s,
            _ => return None,
        };
        let s = interner.resolve(sym)?;
        let trimmed = s.trim();
        if trimmed.is_empty() { None } else { Some(s) }
    }
}

impl PairHook for FnvFo4Hook {
    /// Drop FNV-only global fields (`SCRI`) before field translation begins.
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::drop_global_fields(record);
        Self::drop_incompatible_fields(record, ctx.interner);
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        model_paths::normalize_model_paths(ctx.interner, record);
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
/// here and the schema-identical legacy ARMA model layout are shared. Unrelated
/// FNV TERM/ADDN rewrites remain excluded.
pub struct Fo3Fo4Hook;

impl PairHook for Fo3Fo4Hook {
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        clear_legacy_bptd_ragdoll_payloads(record);
        match record.sig.0 {
            sig if sig == *b"ARMO" => {
                relayout_armo_loader_fields(record, LegacyArmorSource::Fo3, ctx.interner)
            }
            sig if sig == *b"ARMA" => relayout_arma_models(record, ctx.interner),
            sig if sig == *b"PROJ" => relayout_proj_data(record),
            sig if sig == *b"REFR" => fo4_layouts::normalize_refr_xloc(record, ctx.interner),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::LegacyFallout, ctx.interner)
            }
            sig if sig == *b"WTHR" => fo4_layouts::normalize_legacy_wthr(record, ctx.interner),
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
    include!("tests/terminals.rs");
    include!("tests/references.rs");
    include!("tests/serial.rs");
}
