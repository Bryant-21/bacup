use crate::merge_sources::{self, MergeOptions};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

pub struct MergeSourcesPhase;

impl Phase for MergeSourcesPhase {
    fn name(&self) -> &'static str {
        "merge_sources"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let options: MergeOptions = serde_json::from_value(ctx.params.clone())
            .map_err(|error| PhaseError::BadParams(error.to_string()))?;
        let report = merge_sources::run(&options)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        ctx.check_cancel()?;
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: self.name(),
            level: LogLevel::Info,
            message: format!(
                "merge_sources: primary={}, grafted={}, deduped={}, copied={}",
                report.primary_records, report.grafted_records, report.deduped, report.copied
            ),
        });
        Ok(PhaseReport {
            records_added: report.copied.min(u32::MAX as u64) as u32,
            records_dropped: report.deduped.min(u32::MAX as u64) as u32,
            warnings: report.dangling.len().min(u32::MAX as usize) as u32,
            ..Default::default()
        })
    }
}
