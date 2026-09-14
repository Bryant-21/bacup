use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::formkey_mapper::FormKeyMapper;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::record::Record;
use crate::skyrimse_fo4_runtime::wolf_creature::{
    SkyrimWolfRecordTarget, SkyrimWolfSourceRecords, adapt_skyrim_wolf_records,
    canonical_skyrim_wolf_runtime_contract,
};
use crate::source_rig::{
    CreatureRecordClosure, CreatureRecordFormKeys, TargetFormKey, pack_mvp_scaffold,
    pack_mvp_scaffold_with_motion,
};
use crate::translator::Game;
use crate::translator::pair_hooks::fnv_fo4::gecko_creature::{
    adapt_canonical_gecko, canonical_gecko_asset_evidence,
};

const WOLF_PROFILE: &str = "skyrim_wolf";
const GECKO_PROFILE: &str = "fnv_gecko";

pub struct EmitMvpCreaturePhase;

impl Phase for EmitMvpCreaturePhase {
    fn name(&self) -> &'static str {
        "emit_mvp_creature"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let profile = profile(ctx.params)?;
        validate_profile_gate(ctx, profile)?;
        if ctx
            .params
            .get("register_only")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            return register_scaffold(ctx, profile);
        }

        let target_plugin = crate::source_read::plugin_name_for_handle(ctx.run.target_handle_id)
            .map_err(|error| internal(format!("read output plugin name: {error}")))?;
        let form_keys = allocate_form_keys(ctx, &target_plugin)?;
        let (mut closure, rig, graph, motion) = match profile {
            WOLF_PROFILE => {
                let records = read_wolf_source_records(ctx)?;
                let projection = adapt_skyrim_wolf_records(
                    SkyrimWolfSourceRecords {
                        npc: &records[0],
                        race: &records[1],
                        skin: &records[2],
                        armor_addon: &records[3],
                        body_part_data: &records[4],
                        unarmed_weapon: &records[5],
                        right_hand: &records[6],
                        left_hand: &records[7],
                        both_hands: &records[8],
                    },
                    SkyrimWolfRecordTarget {
                        target_plugin: target_plugin.clone(),
                        form_keys,
                        runtime_root: "Actors\\B21_SkyrimWolf".to_string(),
                        resolved_source_name: "Wolf".to_string(),
                    },
                    &ctx.run.interner,
                )
                .map_err(|error| internal(error.to_string()))?;
                let (rig, graph, motion) = canonical_skyrim_wolf_runtime_contract();
                let closure = crate::source_rig::emit_creature_record_closure(
                    &rig,
                    &graph,
                    &projection.manifest,
                    &projection.profile,
                    &ctx.run.interner,
                )
                .map_err(|error| internal(error.to_string()))?;
                (closure, rig, graph, motion)
            }
            GECKO_PROFILE => {
                let source = read_source_record(ctx, "10CD73@FalloutNV.esm")?;
                let adapted = adapt_canonical_gecko(
                    &source,
                    &canonical_gecko_asset_evidence(),
                    &target_plugin,
                    form_keys,
                    "Actors\\B21_FNVGecko",
                    &ctx.run.interner,
                )
                .map_err(|error| internal(error.to_string()))?;
                let closure = adapted
                    .emit_records(&ctx.run.interner)
                    .map_err(|error| internal(error.to_string()))?;
                (closure, adapted.rig, adapted.graph, adapted.motion)
            }
            _ => unreachable!(),
        };

        insert_target_race_data(ctx, &mut closure)?;
        let output_root = ctx.mod_path.join("data").join("Meshes");
        let packed = if profile == GECKO_PROFILE {
            pack_mvp_scaffold_with_motion(&rig, &graph, &motion, &output_root)
        } else {
            pack_mvp_scaffold(&rig, &graph, &output_root)
        }
        .map_err(|error| internal(format!("pack {profile} scaffold: {error}")))?;
        for artifact in &packed.artifacts {
            std::fs::remove_file(&artifact.xml_path).map_err(|error| {
                internal(format!(
                    "remove validated scaffold XML {}: {error}",
                    artifact.xml_path.display()
                ))
            })?;
        }
        write_closure(ctx, closure)?;

        Ok(PhaseReport {
            records_added: 6,
            assets_written: packed.artifacts.len() as u32,
            ..Default::default()
        })
    }
}

pub struct ConvertMvpSkyrimWolfHavokPhase;

impl Phase for ConvertMvpSkyrimWolfHavokPhase {
    fn name(&self) -> &'static str {
        "convert_mvp_skyrim_wolf_havok"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        validate_profile_gate(ctx, WOLF_PROFILE)?;
        let assets = exact_wolf_havok_assets(ctx.params)?;
        let data_root = ctx.mod_path.join("data");
        let mut written = 0_u32;
        for (source, target) in assets {
            ctx.check_cancel()?;
            let bytes = std::fs::read(&source).map_err(|error| {
                internal(format!(
                    "read Skyrim Wolf HKX {}: {error}",
                    source.display()
                ))
            })?;
            let converted =
                havok_native::api::havok_reemit_skyrim_2010_animation_asset_to_fo4(&bytes)
                    .map_err(|error| internal(format!("convert {}: {error}", source.display())))?;
            let output = join_data_path(&data_root, &target)?;
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| internal(format!("create {}: {error}", parent.display())))?;
            }
            std::fs::write(&output, converted)
                .map_err(|error| internal(format!("write {}: {error}", output.display())))?;
            if let Some(sink) = &ctx.run.output_sink {
                let relative = output
                    .strip_prefix(&data_root)
                    .map_err(|error| internal(error.to_string()))?
                    .to_string_lossy()
                    .replace('\\', "/");
                sink.add_existing_file(&relative, &output)
                    .map_err(|error| {
                        internal(format!("register {relative} with output sink: {error}"))
                    })?;
            }
            written += 1;
        }
        Ok(PhaseReport {
            assets_written: written,
            ..Default::default()
        })
    }
}

fn profile(params: &JsonValue) -> Result<&str, PhaseError> {
    let profile = params
        .get("profile")
        .and_then(JsonValue::as_str)
        .ok_or_else(|| PhaseError::BadParams("missing profile".to_string()))?;
    match profile {
        WOLF_PROFILE | GECKO_PROFILE => Ok(profile),
        _ => Err(PhaseError::BadParams(format!(
            "unsupported MVP creature profile {profile:?}"
        ))),
    }
}

fn validate_profile_gate(ctx: &PhaseCtx<'_>, profile: &str) -> Result<(), PhaseError> {
    let expected = match profile {
        WOLF_PROFILE => Game::SkyrimSe,
        GECKO_PROFILE => Game::Fnv,
        _ => unreachable!(),
    };
    if ctx.run.source != expected
        || ctx.run.target != Game::Fo4
        || !ctx
            .run
            .config
            .has_complete_mvp_weapon_closure(expected, Game::Fo4)
    {
        return Err(PhaseError::BadParams(format!(
            "{profile} requires the complete {:?}->FO4 MVP exception closure",
            expected
        )));
    }
    Ok(())
}

fn allocate_form_keys(
    ctx: &mut PhaseCtx<'_>,
    target_plugin: &str,
) -> Result<CreatureRecordFormKeys, PhaseError> {
    let state = ctx
        .run
        .mapper_state
        .as_mut()
        .ok_or_else(|| internal("MVP creature allocation requires translated mapper state"))?;
    let mut mapper = FormKeyMapper::from_state(state, &ctx.run.interner);
    let generated = (0..6)
        .map(|_| mapper.allocate_generated())
        .collect::<Vec<_>>();
    if generated.iter().any(|form_key| {
        ctx.run
            .interner
            .resolve(form_key.plugin)
            .is_none_or(|plugin| !plugin.eq_ignore_ascii_case(target_plugin))
    }) {
        return Err(internal(
            "generated MVP creature FormKeys are not owned by the output plugin",
        ));
    }
    let key = |index: usize| TargetFormKey::new(generated[index].local, target_plugin);
    Ok(CreatureRecordFormKeys {
        race: key(0),
        npc: key(1),
        skin: key(2),
        armor_addon: key(3),
        body_part_data: key(4),
        unarmed_weapon: key(5),
    })
}

fn read_wolf_source_records(ctx: &PhaseCtx<'_>) -> Result<Vec<Record>, PhaseError> {
    [
        "0753CE@Skyrim.esm",
        "01320A@Skyrim.esm",
        "04E886@Skyrim.esm",
        "04E885@Skyrim.esm",
        "04FBF5@Skyrim.esm",
        "0001F4@Skyrim.esm",
        "013F42@Skyrim.esm",
        "013F43@Skyrim.esm",
        "013F45@Skyrim.esm",
    ]
    .into_iter()
    .map(|form_key| read_source_record(ctx, form_key))
    .collect()
}

fn read_source_record(ctx: &PhaseCtx<'_>, form_key: &str) -> Result<Record, PhaseError> {
    crate::source_read::read_record(
        ctx.run.source_handle_id,
        form_key,
        &ctx.run.schema_source,
        &ctx.run.interner,
    )
    .map_err(|error| internal(format!("read MVP source record {form_key}: {error}")))
}

fn insert_target_race_data(
    ctx: &PhaseCtx<'_>,
    closure: &mut CreatureRecordClosure,
) -> Result<(), PhaseError> {
    let donor = ctx
        .run
        .read_explicit_target_master_record("01D810@Fallout4.esm")
        .map_err(|error| internal(format!("read MoleratRace DATA donor: {error}")))?;
    let mut donor_data = donor
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "DATA");
    let data = donor_data
        .next()
        .cloned()
        .filter(|_| donor_data.next().is_none())
        .ok_or_else(|| internal("MoleratRace must contain exactly one DATA field"))?;
    let race = closure
        .records
        .iter_mut()
        .find(|record| record.sig.as_str() == "RACE")
        .ok_or_else(|| internal("MVP creature closure has no RACE"))?;
    if race.fields.iter().any(|field| field.sig.as_str() == "DATA") {
        return Err(internal("MVP creature RACE already contains DATA"));
    }
    let insert_at = race
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "MNAM")
        .unwrap_or(race.fields.len());
    race.fields.insert(insert_at, data);
    Ok(())
}

fn write_closure(ctx: &mut PhaseCtx<'_>, closure: CreatureRecordClosure) -> Result<(), PhaseError> {
    if closure.records.len() != 6 {
        return Err(internal(format!(
            "MVP creature closure emitted {} records, expected 6",
            closure.records.len()
        )));
    }
    for record in closure.records {
        crate::target_write::add_record_native(
            ctx.run.target_handle_id,
            record,
            &ctx.run.schema_target,
            &ctx.run.interner,
        )
        .map_err(|error| internal(format!("write MVP creature record: {error}")))?;
    }
    Ok(())
}

fn register_scaffold(ctx: &PhaseCtx<'_>, profile: &str) -> Result<PhaseReport, PhaseError> {
    let sink =
        ctx.run.output_sink.as_ref().ok_or_else(|| {
            internal("MVP creature scaffold registration requires an output sink")
        })?;
    let paths = scaffold_paths(profile);
    let data_root = ctx.mod_path.join("data");
    for path in paths {
        let relative = format!("Meshes/{}", path.replace('\\', "/"));
        let file = join_data_path(&data_root, &relative)?;
        if !file.is_file() {
            return Err(internal(format!(
                "MVP creature scaffold is missing {}",
                file.display()
            )));
        }
        sink.add_existing_file(&relative, &file).map_err(|error| {
            internal(format!(
                "register scaffold {relative} with output sink: {error}"
            ))
        })?;
    }
    Ok(PhaseReport {
        assets_written: 4,
        ..Default::default()
    })
}

fn scaffold_paths(profile: &str) -> [&'static str; 4] {
    match profile {
        WOLF_PROFILE => [
            "Actors\\B21_SkyrimWolf\\B21_SkyrimWolfProject.hkx",
            "Actors\\B21_SkyrimWolf\\Characters\\B21_SkyrimWolfCharacter.hkx",
            "Actors\\B21_SkyrimWolf\\Behaviors\\B21_SkyrimWolfRootBehavior.hkx",
            "Actors\\B21_SkyrimWolf\\Behaviors\\B21_SkyrimWolfCoreBehavior.hkx",
        ],
        GECKO_PROFILE => [
            "Actors\\B21_FNVGecko\\B21_FNVGeckoProject.hkx",
            "Actors\\B21_FNVGecko\\Characters\\B21_FNVGeckoCharacter.hkx",
            "Actors\\B21_FNVGecko\\Behaviors\\B21_FNVGeckoRootBehavior.hkx",
            "Actors\\B21_FNVGecko\\Behaviors\\B21_FNVGeckoCoreBehavior.hkx",
        ],
        _ => unreachable!(),
    }
}

fn exact_wolf_havok_assets(params: &JsonValue) -> Result<Vec<(PathBuf, String)>, PhaseError> {
    let expected = HashMap::from([
        (
            "actors/canine/character assets wolf/skeleton.hkx",
            "Meshes/Actors/B21_SkyrimWolf/CharacterAssets/Skeleton.hkx",
        ),
        (
            "actors/canine/animations/mt_idle_wolf.hkx",
            "Meshes/Actors/B21_SkyrimWolf/Animations/Idle.hkx",
        ),
        (
            "actors/canine/animations/walkforward_wolf.hkx",
            "Meshes/Actors/B21_SkyrimWolf/Animations/WalkForward.hkx",
        ),
        (
            "actors/canine/animations/turncannedl90.hkx",
            "Meshes/Actors/B21_SkyrimWolf/Animations/TurnLeft90.hkx",
        ),
        (
            "actors/canine/animations/turncannedr90.hkx",
            "Meshes/Actors/B21_SkyrimWolf/Animations/TurnRight90.hkx",
        ),
        (
            "actors/canine/animations/attack1.hkx",
            "Meshes/Actors/B21_SkyrimWolf/Animations/Attack1.hkx",
        ),
    ]);
    let assets = params
        .get("assets")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| PhaseError::BadParams("missing assets".to_string()))?;
    if assets.len() != expected.len() {
        return Err(PhaseError::BadParams(format!(
            "Skyrim Wolf Havok closure has {} assets, expected {}",
            assets.len(),
            expected.len()
        )));
    }
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(assets.len());
    for asset in assets {
        let source_path = asset
            .get("source_path")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| PhaseError::BadParams("asset source_path missing".to_string()))?;
        let resolved_path = asset
            .get("resolved_path")
            .and_then(JsonValue::as_str)
            .filter(|path| !path.is_empty())
            .ok_or_else(|| PhaseError::BadParams("asset resolved_path missing".to_string()))?;
        let normalized = normalize_mesh_relative(source_path)?;
        let target = expected.get(normalized.as_str()).ok_or_else(|| {
            PhaseError::BadParams(format!("unexpected Skyrim Wolf HKX {source_path:?}"))
        })?;
        if !seen.insert(normalized) {
            return Err(PhaseError::BadParams(format!(
                "duplicate Skyrim Wolf HKX {source_path:?}"
            )));
        }
        result.push((PathBuf::from(resolved_path), (*target).to_string()));
    }
    if seen.len() != expected.len() {
        return Err(PhaseError::BadParams(
            "Skyrim Wolf Havok closure is incomplete".to_string(),
        ));
    }
    Ok(result)
}

fn normalize_mesh_relative(path: &str) -> Result<String, PhaseError> {
    let parts = safe_relative_parts(path)?;
    let mut normalized = parts
        .into_iter()
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>();
    for root in ["data", "meshes"] {
        if normalized.first().is_some_and(|part| part == root) {
            normalized.remove(0);
        }
    }
    Ok(normalized.join("/"))
}

fn join_data_path(data_root: &Path, relative: &str) -> Result<PathBuf, PhaseError> {
    Ok(safe_relative_parts(relative)?
        .into_iter()
        .fold(data_root.to_path_buf(), |path, part| path.join(part)))
}

fn safe_relative_parts(path: &str) -> Result<Vec<String>, PhaseError> {
    let mut parts = Vec::new();
    for part in path.replace('\\', "/").split('/') {
        let part = part.trim();
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') {
            return Err(PhaseError::BadParams(format!(
                "unsafe relative path {path:?}"
            )));
        }
        parts.push(part.to_string());
    }
    if parts.is_empty() {
        return Err(PhaseError::BadParams("empty relative path".to_string()));
    }
    Ok(parts)
}

fn internal(message: impl Into<String>) -> PhaseError {
    PhaseError::Internal(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wolf_havok_closure_is_exact() {
        let params = serde_json::json!({
            "assets": [
                {"source_path": "Actors/Canine/Character Assets Wolf/skeleton.hkx", "resolved_path": "X:/skeleton.hkx"},
                {"source_path": "Actors/Canine/Animations/mt_idle_wolf.hkx", "resolved_path": "X:/idle.hkx"},
                {"source_path": "Actors/Canine/Animations/walkforward_wolf.hkx", "resolved_path": "X:/walk.hkx"},
                {"source_path": "Actors/Canine/Animations/turncannedl90.hkx", "resolved_path": "X:/left.hkx"},
                {"source_path": "Actors/Canine/Animations/turncannedr90.hkx", "resolved_path": "X:/right.hkx"},
                {"source_path": "Actors/Canine/Animations/attack1.hkx", "resolved_path": "X:/attack.hkx"},
            ]
        });
        assert_eq!(exact_wolf_havok_assets(&params).unwrap().len(), 6);
        let mut missing = params;
        missing["assets"].as_array_mut().unwrap().pop();
        assert!(exact_wolf_havok_assets(&missing).is_err());
    }
}
