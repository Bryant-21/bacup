use super::{Phase, PhaseCtx, PhaseError, PhaseReport};

pub struct PrepareFnvRadiationRuntimePhase;

impl Phase for PrepareFnvRadiationRuntimePhase {
    fn name(&self) -> &'static str {
        "prepare_fnv_radiation_runtime"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let actor_stats = crate::translator::pair_hooks::fnv_fo4::creature_actor_stats::prepare(ctx.run)
            .map_err(PhaseError::Internal)?;
        let output = crate::translator::pair_hooks::fnv_fo4::creature_magic_runtime::prepare_fnv_radiation_output(ctx.run)
            .map_err(PhaseError::Internal)?;
        let image_space =
            crate::translator::pair_hooks::fnv_fo4::creature_imagespace::prepare_output(ctx.run)
                .map_err(PhaseError::Internal)?;
        crate::translator::pair_hooks::fnv_fo4::creature_magic_runtime::remove_stale_native_outputs(
            ctx.run, ctx.mod_path, output.is_some(), image_space.is_some(),
        ).map_err(PhaseError::Internal)?;
        let mut assets_written = 0;
        if let Some(output) = actor_stats {
            output.write(ctx.mod_path).map_err(PhaseError::Internal)?;
            assets_written += 1;
        } else if matches!(ctx.run.source, crate::translator::Game::Fnv | crate::translator::Game::Fo3)
            && ctx.run.target == crate::translator::Game::Fo4 && ctx.run.source_handle().is_some() {
            let plugin = &ctx.run.config.output_plugin_name;
            if plugin.is_empty() || plugin.contains(['/', '\\', ':']) || matches!(plugin.as_str(), "." | "..") {
                return Err(PhaseError::Internal("Legacy stat output plugin must be a file name".into()));
            }
            let path = ctx.mod_path.join("F4SE/Plugins/B21_ConversionRuntime/LegacyStats").join(format!("{plugin}.json"));
            match std::fs::remove_file(path) {
                Ok(()) => {},
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return Err(PhaseError::Internal(error.to_string())),
            }
        }
        if let Some(output) = output {
            output.write(ctx.mod_path).map_err(PhaseError::Internal)?;
            assets_written += 1;
        }
        if let Some(output) = image_space {
            output.write(ctx.mod_path).map_err(PhaseError::Internal)?;
            assets_written += 1;
        }
        Ok(PhaseReport {
            assets_written,
            ..Default::default()
        })
    }
}
