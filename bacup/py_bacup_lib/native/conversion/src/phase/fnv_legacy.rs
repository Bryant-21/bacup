//! Wraps fnv_legacy_scripting_from_deferred in the Phase trait.
//!
//! Expected params (JSON object):
//!   { "mod_prefix": "B21_MyMod", "source_plugin": "Source.esm" }

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

pub struct FnvLegacyPhase;

impl Phase for FnvLegacyPhase {
    fn name(&self) -> &'static str {
        "fnv_legacy"
    }
    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let mod_prefix = ctx
            .params
            .get("mod_prefix")
            .and_then(|v| v.as_str())
            .unwrap_or("B21");
        let source_plugin = ctx
            .params
            .get("source_plugin")
            .and_then(|v| v.as_str())
            .unwrap_or("Source.esm");
        let mod_path = ctx.mod_path.to_str().unwrap_or("");

        let result = ctx
            .run
            .run_fnv_legacy_scripting_from_deferred(mod_prefix, source_plugin, mod_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        Ok(PhaseReport {
            records_added: result.records_written,
            warnings: result.warnings.len() as u32,
            ..PhaseReport::default()
        })
    }
}
