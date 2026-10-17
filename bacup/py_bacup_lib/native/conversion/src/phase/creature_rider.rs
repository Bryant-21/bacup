use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::animations::skyrim_rider::{self, RiderClipReceipt, RiderRegistration};
use super::creature_corpus::{extracted_mesh_root, write_json};
use super::{PhaseCtx, PhaseError};
use crate::fixups::FixupConfig;
use crate::formkey_mapper::FormKeyMapper;
use crate::run::ConversionRun;
use crate::session::PluginSession;
use crate::skyrimse_fo4_runtime::{creature_motion, creature_mvp_live};

pub(crate) const REGISTRATION_RECEIPT: &str = "debug/creature_corpus/rider_registration.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RiderAssetReceipt {
    pub path: String,
    pub blake3: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RiderStageReceipt {
    pub version: u32,
    pub heading_radians: f32,
    pub assets: Vec<RiderAssetReceipt>,
    pub clips: Vec<RiderClipReceipt>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RiderPublicationReceipt {
    pub version: u32,
    pub keyword: String,
    pub additive_race: String,
    pub enter_idle: String,
    pub exit_idle: String,
    pub quick_exit_idle: String,
    pub horse_enter_idle: String,
    pub horse_exit_idle: String,
    pub horse_quick_exit_idle: String,
    pub stage: RiderStageReceipt,
}

pub(super) fn stage(
    ctx: &PhaseCtx<'_>,
    families: &creature_mvp_live::SkyrimCreatureLivePreparationLedger,
    motion: &creature_motion::SkyrimCreatureMotionCatalog,
    work_root: &Path,
) -> Result<(), PhaseError> {
    let Some(horse) = families
        .families
        .iter()
        .find(|family| family.family_id == "horse")
    else {
        return Ok(());
    };
    let creature_mvp_live::SkyrimCreatureLivePreparedFamilyDisposition::Ready {
        asset_recipe, ..
    } = &horse.disposition
    else {
        return Err(PhaseError::Internal(
            "horse rider requires a validated source horse family".into(),
        ));
    };
    let target_root = ctx.target_extracted_dir.ok_or_else(|| {
        PhaseError::Internal("horse rider requires extracted FO4 character contracts".into())
    })?;
    let staged = skyrim_rider::stage_horse_rider(
        &extracted_mesh_root(ctx.source_extracted_dir),
        &extracted_mesh_root(target_root),
        motion,
        &asset_recipe.rig,
        &asset_recipe.graph,
    )
    .map_err(PhaseError::Internal)?;
    let heading_radians = staged
        .receipts
        .iter()
        .find(|clip| clip.name == "Mount")
        .and_then(|clip| clip.paired_origin.as_ref())
        .map(|origin| origin.heading_radians)
        .ok_or_else(|| {
            PhaseError::Internal("rider mount has no source placement receipt".into())
        })?;
    let stage_root = work_root.join("rider");
    let mut assets = Vec::new();
    for (path, bytes) in staged.assets {
        let path = format!("Meshes/{path}");
        let destination = stage_root.join("data").join(&path);
        std::fs::create_dir_all(destination.parent().unwrap()).map_err(io_error)?;
        std::fs::write(destination, &bytes).map_err(io_error)?;
        assets.push(RiderAssetReceipt {
            path,
            blake3: blake3::hash(&bytes).to_hex().to_string(),
        });
    }
    write_json(
        &stage_root.join("stage.json"),
        &RiderStageReceipt {
            version: 1,
            heading_radians,
            assets,
            clips: staged.receipts,
        },
    )
}

pub(super) fn read_staged(
    mod_root: &Path,
    debug_dir: &str,
) -> Result<Option<(RiderStageReceipt, Vec<(String, PathBuf)>)>, PhaseError> {
    let stage_root = mod_root.join(debug_dir).join("live_prepared/rider");
    let receipt_path = stage_root.join("stage.json");
    if !receipt_path.exists() {
        return Ok(None);
    }
    let receipt: RiderStageReceipt =
        serde_json::from_slice(&std::fs::read(&receipt_path).map_err(io_error)?)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
    if receipt.version != 1 || !receipt.heading_radians.is_finite() || receipt.assets.is_empty() {
        return Err(PhaseError::Internal("invalid staged rider receipt".into()));
    }
    let mut files = Vec::new();
    let mut names = std::collections::HashSet::new();
    for asset in &receipt.assets {
        if !asset.path.starts_with("Meshes/Actors/Character/")
            || asset.path.contains('\\')
            || asset.path.contains(':')
            || asset
                .path
                .split('/')
                .any(|part| matches!(part, ".." | "." | ""))
            || !names.insert(asset.path.to_ascii_lowercase())
        {
            return Err(PhaseError::Internal(
                "invalid rider asset receipt path".into(),
            ));
        }
        let path = stage_root.join("data").join(&asset.path);
        let bytes = std::fs::read(&path).map_err(io_error)?;
        if blake3::hash(&bytes).to_hex().as_str() != asset.blake3 {
            return Err(PhaseError::Internal(format!(
                "staged rider asset changed: {}",
                asset.path
            )));
        }
        files.push((asset.path.clone(), path));
    }
    Ok(Some((receipt, files)))
}

pub(super) fn prepare_records(
    run: &mut ConversionRun,
    session: &mut PluginSession<'_>,
    target_extracted: Option<&Path>,
) -> Result<RiderRegistration, PhaseError> {
    let config = FixupConfig {
        target_master_handle_ids: run.master_handle_ids.clone(),
        target_schema: Some(run.schema_target.clone()),
        source_schema: Some(run.schema_source.clone()),
        target_extracted_dir: target_extracted.map(Path::to_path_buf),
        ..Default::default()
    };
    let state = run
        .mapper_state
        .as_mut()
        .ok_or_else(|| PhaseError::Internal("rider registration requires mapper state".into()))?;
    let mut mapper = FormKeyMapper::from_state(state, &run.interner);
    let registration = skyrim_rider::prepare_registration(session, &mut mapper, &config)
        .map_err(PhaseError::Internal)?;
    crate::target_write::prepare_record_batch_in_slot(
        session.target_slot(),
        registration.records.clone(),
        &run.schema_target,
        &run.interner,
    )
    .map_err(|error| {
        PhaseError::Internal(format!(
            "rider registration {}: {}",
            error.record_index, error.source
        ))
    })?;
    Ok(registration)
}

pub(super) fn commit_records(
    run: &ConversionRun,
    session: &mut PluginSession<'_>,
    registration: RiderRegistration,
    stage: RiderStageReceipt,
    mod_root: &Path,
) -> Result<(), PhaseError> {
    let receipt = RiderPublicationReceipt {
        version: 1,
        keyword: registration.keyword.format(&run.interner),
        additive_race: registration.additive_race.format(&run.interner),
        enter_idle: registration.enter_idle.format(&run.interner),
        exit_idle: registration.exit_idle.format(&run.interner),
        quick_exit_idle: registration.quick_exit_idle.format(&run.interner),
        horse_enter_idle: registration.horse_enter_idle.format(&run.interner),
        horse_exit_idle: registration.horse_exit_idle.format(&run.interner),
        horse_quick_exit_idle: registration.horse_quick_exit_idle.format(&run.interner),
        stage,
    };
    // Prepare against the committed creature string table so this batch cannot restore an earlier table.
    let prepared = crate::target_write::prepare_record_batch_in_slot(
        session.target_slot(),
        registration.records,
        &run.schema_target,
        &run.interner,
    )
    .map_err(|error| {
        PhaseError::Internal(format!(
            "validated rider registration changed before commit: {}",
            error.source
        ))
    })?;
    session.commit_prepared_record_batch(prepared);
    write_json(&mod_root.join(REGISTRATION_RECEIPT), &receipt)
}

fn io_error(error: std::io::Error) -> PhaseError {
    PhaseError::Internal(error.to_string())
}
