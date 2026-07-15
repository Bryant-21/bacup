//! Wraps apply_registry_mappings in the Phase trait.
//!
//! Expected params: a JSON object mapping old FormKey strings to new FormKey strings,
//! e.g. { "000800:Source.esp": "001000:Target.esp", ... }

use std::collections::HashMap;

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

pub struct ApplyRegistryMappingsPhase;

impl Phase for ApplyRegistryMappingsPhase {
    fn name(&self) -> &'static str {
        "apply_registry_mappings"
    }
    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let mappings: HashMap<String, String> = serde_json::from_value(ctx.params.clone())
            .map_err(|e| {
                PhaseError::BadParams(format!("expected JSON object {{old_fk: new_fk, ...}}: {e}"))
            })?;
        let count = ctx
            .run
            .apply_registry_mappings(&mappings)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        Ok(PhaseReport {
            records_changed: count as u32,
            ..PhaseReport::default()
        })
    }
}
