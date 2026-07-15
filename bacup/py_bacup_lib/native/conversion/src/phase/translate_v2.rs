//! translate_v2 phase — parallel translate (store2). Source record
//! bytes come from an mmap of the source handle's backing file; everything
//! else (mapper, hooks, encode) matches the legacy "translate" phase.

use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::run::{ConversionRun, TranslateStats};

pub struct TranslateV2Phase;

impl Phase for TranslateV2Phase {
    fn name(&self) -> &'static str {
        "translate_v2"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let source_path = {
            // Short lock: read the source slot's backing file path.
            let store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
                .lock()
                .unwrap();
            let slot = store.get(&ctx.run.source_handle_id).ok_or_else(|| {
                PhaseError::Internal(format!(
                    "no source plugin handle: {}",
                    ctx.run.source_handle_id
                ))
            })?;
            std::path::PathBuf::from(slot.parsed.file_path.clone())
        };
        if source_path.as_os_str().is_empty() {
            return Err(PhaseError::Internal(
                "translate_v2: source handle has no backing file path (in-memory plugin)".into(),
            ));
        }
        log_translate_v2(ctx.run, "translate_v2 phase: run start");
        let stats = ctx
            .run
            .translate_all_v2(&source_path)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        log_translate_v2(ctx.run, "translate_v2 phase: run returned stats");
        let relocation_warnings = surface_relocation_warnings(ctx.run);
        log_translate_v2(ctx.run, "translate_v2 phase: relocation warnings surfaced");
        Ok(report_from_stats(stats, relocation_warnings))
    }
}

fn report_from_stats(stats: TranslateStats, relocation_warnings: u32) -> PhaseReport {
    PhaseReport {
        records_changed: stats.records_translated,
        records_vanilla_remapped: stats.records_vanilla_remapped,
        records_dropped: stats.records_dropped,
        records_deferred: stats.records_deferred,
        warnings: stats.records_failed + relocation_warnings,
        ..Default::default()
    }
}

fn log_translate_v2(run: &ConversionRun, message: impl Into<String>) {
    let _ = run.event_tx.try_send(PhaseEvent::Log {
        phase: "translate_v2",
        level: LogLevel::Info,
        message: message.into(),
    });
}

/// Mirror `phase::translate::surface_relocation_warnings` but tag the events
/// with this phase's name so the relocation-build warnings are attributed to
/// `translate_v2` when v2 is the first phase after `create_run`.
fn surface_relocation_warnings(run: &ConversionRun) -> u32 {
    for message in &run.relocation_warnings {
        let _ = run.event_tx.try_send(PhaseEvent::Log {
            phase: "translate_v2",
            level: LogLevel::Warn,
            message: message.clone(),
        });
    }
    run.relocation_warnings.len() as u32
}

#[cfg(test)]
mod tests {
    use crate::phase::registry;
    use crate::run::TranslateStats;

    use super::report_from_stats;

    #[test]
    fn translate_v2_is_registered() {
        assert!(
            registry().get("translate_v2").is_some(),
            "translate_v2 phase must be registered in build_registry"
        );
        assert!(registry().names().contains(&"translate_v2"));
    }

    #[test]
    fn translate_v2_report_preserves_record_outcomes() {
        let report = report_from_stats(
            TranslateStats {
                records_translated: 36,
                records_vanilla_remapped: 4,
                records_dropped: 3,
                records_deferred: 2,
                records_failed: 1,
                ..Default::default()
            },
            0,
        );

        assert_eq!(report.records_changed, 36);
        assert_eq!(report.records_vanilla_remapped, 4);
        assert_eq!(report.records_dropped, 3);
        assert_eq!(report.records_deferred, 2);
        assert_eq!(report.warnings, 1);
    }
}
