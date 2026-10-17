use super::{Phase, PhaseCtx, PhaseError, PhaseReport};

#[path = "skyrim_creature_cast_publication.rs"]
mod casting;
pub(super) use casting::publish_creature_cast_catalog;

#[path = "skyrim_trap_publication.rs"]
mod traps;

pub struct PrepareSkyrimMagicRuntimePhase;

impl Phase for PrepareSkyrimMagicRuntimePhase {
    fn name(&self) -> &'static str {
        "prepare_skyrim_magic_runtime"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        let records_added =
            crate::skyrimse_fo4_runtime::skill_modifiers::ensure_skill_modifier_keywords(ctx.run)
                .map_err(PhaseError::Internal)?;
        let weapon_assets =
            crate::skyrimse_fo4_runtime::weapons_runtime::prepare_weapon_runtime_output(ctx.run)
                .map_err(PhaseError::Internal)?
                .map(|output| output.write(ctx.mod_path))
                .transpose()
                .map_err(PhaseError::Internal)?
                .unwrap_or(0);
        let riding_assets = publish_riding_runtime(ctx)?;
        let trap_assets = traps::publish_trap_scripts(ctx)?;
        let Some(output) =
            crate::skyrimse_fo4_runtime::magic_resources::prepare_magic_runtime_output(ctx.run)
                .map_err(PhaseError::Internal)?
        else {
            return Ok(PhaseReport {
                records_added,
                assets_written: weapon_assets + riding_assets + trap_assets,
                ..Default::default()
            });
        };
        output.write(ctx.mod_path).map_err(PhaseError::Internal)?;
        Ok(PhaseReport {
            records_added,
            records_changed: output.records_changed,
            assets_written: 1 + weapon_assets + riding_assets + trap_assets,
            ..Default::default()
        })
    }
}

fn publish_riding_runtime(ctx: &PhaseCtx<'_>) -> Result<u32, PhaseError> {
    use crate::ids::FormKey;
    use crate::translator::Game;

    if ctx.run.source != Game::SkyrimSe || ctx.run.target != Game::Fo4 {
        return Ok(0);
    }
    let path = ctx
        .mod_path
        .join(super::creature_rider::REGISTRATION_RECEIPT);
    if !path.exists() {
        return Ok(0);
    }
    let receipt: super::creature_rider::RiderPublicationReceipt = serde_json::from_slice(
        &std::fs::read(&path).map_err(|error| PhaseError::Internal(error.to_string()))?,
    )
    .map_err(|error| PhaseError::Internal(error.to_string()))?;
    if receipt.version != 1 || receipt.stage.version != 1 {
        return Err(PhaseError::Internal(
            "Unsupported rider publication receipt".into(),
        ));
    }
    let key = |value: &str| FormKey::parse(value, &ctx.run.interner).map_err(PhaseError::Internal);
    for (target, signature) in [
        (&receipt.keyword, *b"KYWD"),
        (&receipt.additive_race, *b"RACE"),
        (&receipt.enter_idle, *b"IDLE"),
        (&receipt.exit_idle, *b"IDLE"),
        (&receipt.quick_exit_idle, *b"IDLE"),
        (&receipt.horse_enter_idle, *b"IDLE"),
        (&receipt.horse_exit_idle, *b"IDLE"),
        (&receipt.horse_quick_exit_idle, *b"IDLE"),
    ] {
        let record = ctx
            .run
            .read_target_record_if_available(key(target)?)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        if !record.is_some_and(|record| record.sig.0 == signature) {
            return Err(PhaseError::Internal(format!(
                "Rider publication target {target} must resolve to {}",
                String::from_utf8_lossy(&signature)
            )));
        }
    }
    let output = crate::skyrimse_fo4_runtime::riding::prepare_riding_runtime_output(
        ctx.run,
        ctx.source_extracted_dir,
        &ctx.mod_path.join("data"),
        key(&receipt.enter_idle)?,
        key(&receipt.exit_idle)?,
        key(&receipt.keyword)?,
        receipt.stage.heading_radians,
    )
    .map_err(PhaseError::Internal)?;
    let Some(output) = output else {
        return Err(PhaseError::Internal(
            "Rider publication has no converted horse race".into(),
        ));
    };
    output.write(ctx.mod_path).map_err(PhaseError::Internal)?;
    Ok(1)
}
