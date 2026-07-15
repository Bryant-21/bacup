//! Wraps the existing translate_all native function in the Phase trait.

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::run::{ConversionRun, RunError};

pub struct TranslatePhase;

impl Phase for TranslatePhase {
    fn name(&self) -> &'static str {
        "translate"
    }
    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let stats = ctx
            .run
            .translate_all()
            .map_err(|e: RunError| PhaseError::Internal(e.to_string()))?;
        let relocation_warnings = surface_relocation_warnings(ctx.run);
        Ok(PhaseReport {
            records_changed: stats.records_translated,
            records_added: 0,
            records_vanilla_remapped: stats.records_vanilla_remapped,
            records_dropped: stats.records_dropped,
            records_deferred: stats.records_deferred,
            assets_written: 0,
            warnings: stats.records_failed + relocation_warnings,
            elapsed_ms: 0, // filled in by dispatcher
            items_failed: 0,
        })
    }
}

/// Emit relocation-build warnings (e.g. FO4 extracted dir missing the configured
/// mesh roots → collision detection disabled) as Warn-level log events on the
/// first phase after `create_run`, and return the count to fold into the report.
fn surface_relocation_warnings(run: &ConversionRun) -> u32 {
    for message in &run.relocation_warnings {
        let _ = run.event_tx.try_send(PhaseEvent::Log {
            phase: "translate",
            level: LogLevel::Warn,
            message: message.clone(),
        });
    }
    run.relocation_warnings.len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::{RunConfig, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;

    #[test]
    fn relocation_warnings_are_surfaced_as_log_events() {
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap();

        let (count, messages) = with_run(id, |run| -> Result<(u32, Vec<String>), RunError> {
            run.relocation_warnings.clear();
            run.relocation_warnings
                .push("relocation: FO4 extracted dir missing mesh roots".to_string());
            let count = surface_relocation_warnings(run);
            let messages: Vec<String> = run
                .event_rx
                .try_iter()
                .filter_map(|e| match e {
                    PhaseEvent::Log { message, .. } => Some(message),
                    _ => None,
                })
                .collect();
            Ok((count, messages))
        })
        .unwrap();

        assert_eq!(count, 1);
        assert!(
            messages
                .iter()
                .any(|m| m.contains("FO4 extracted dir missing mesh roots")),
            "expected relocation warning in log events, got {messages:?}"
        );
        drop_run(id).unwrap();
    }
}
