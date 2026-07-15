//! fixups_v2 phase: runs the canonical store2 segment plan.

use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};

pub struct FixupsV2Phase;

impl Phase for FixupsV2Phase {
    fn name(&self) -> &'static str {
        "fixups_v2"
    }
    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let reports = ctx
            .run
            .apply_fixups_v2()
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let mut total = PhaseReport::default();
        for (_name, r) in &reports {
            total.records_changed += r.records_changed;
            total.records_added += r.records_added;
            total.records_dropped += r.records_dropped;
            total.warnings += r.warnings.len() as u32;
        }
        Ok(total)
    }
}
