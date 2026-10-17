use super::{PhaseCtx, PhaseError};
use crate::ids::SigCode;
use crate::skyrimse_fo4_runtime::creature_attack_scripts;
use crate::translator::Game;

pub(super) fn publish_trap_scripts(ctx: &PhaseCtx<'_>) -> Result<u32, PhaseError> {
    if ctx.run.source != Game::SkyrimSe || ctx.run.target != Game::Fo4 {
        return Ok(0);
    }
    let mut names = std::collections::BTreeSet::new();
    for key in crate::source_read::iter_form_keys_of_sig(
        ctx.run.target_handle_id,
        SigCode(*b"ACTI"),
        &ctx.run.interner,
    )
    .map_err(|error| PhaseError::Internal(error.to_string()))?
    {
        let record = ctx
            .run
            .read_target_record_if_available(key)
            .map_err(|error| PhaseError::Internal(error.to_string()))?
            .ok_or_else(|| {
                PhaseError::Internal(format!(
                    "missing converted trap {}",
                    key.format(&ctx.run.interner)
                ))
            })?;
        let required = creature_attack_scripts::traps::required_target_scripts(&record)
            .map_err(PhaseError::Internal)?;
        if !required.is_empty() {
            validate_trap_dependencies(ctx, &record).map_err(PhaseError::Internal)?;
            names.extend(required);
        }
    }
    if names.is_empty() {
        return Ok(0);
    }
    let staging_root = ctx.mod_path.join(".bacup/skyrim_trap_scripts");
    let headers = crate::script_headers::prepare_headers(
        ctx.target_extracted_dir,
        ctx.target_data_dir,
        &staging_root,
        ctx.params
            .get("bundled_corpus_root")
            .and_then(serde_json::Value::as_str)
            .map(std::path::Path::new),
        ctx.run.target_assets.as_deref(),
        "fo4",
    )
    .map_err(PhaseError::Internal)?;
    if headers.import_roots.is_empty() {
        return Err(PhaseError::Internal(
            "Skyrim trap conversion requires Fallout 4 Papyrus headers".into(),
        ));
    }
    let imports = headers
        .import_roots
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let flags = headers
        .flags_file
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());
    let artifacts = creature_attack_scripts::stage_attack_scripts(
        &names,
        ctx.source_extracted_dir,
        &staging_root,
        &imports,
        flags.as_deref(),
    )
    .map_err(PhaseError::Internal)?;
    creature_attack_scripts::publish_attack_scripts(&staging_root, ctx.mod_path)
        .map_err(PhaseError::Internal)?;
    Ok((artifacts.len() * 2) as u32)
}

fn validate_trap_dependencies(
    ctx: &PhaseCtx<'_>,
    target: &crate::record::Record,
) -> Result<(), String> {
    let run = &ctx.run;
    let mappings = &run
        .mapper_state
        .as_ref()
        .ok_or("trap scripts require source mappings")?
        .source_to_target;
    let source_key = mappings
        .iter()
        .find_map(|(source, mapped)| (*mapped == target.form_key).then_some(*source))
        .ok_or_else(|| {
            format!(
                "converted trap {} has no source mapping",
                target.form_key.format(&run.interner)
            )
        })?;
    let source = crate::source_read::read_record_relayout_by_form_key(
        run.source_handle_id,
        &source_key,
        &run.schema_source,
        &run.interner,
        None,
    )
    .map_err(|error| error.to_string())?;
    let (source_plugin, source_masters) =
        crate::source_read::plugin_context_for_handle(run.source_handle_id)
            .map_err(|error| error.to_string())?;
    let (target_plugin, target_masters) =
        crate::source_read::plugin_context_for_handle(run.target_handle_id)
            .map_err(|error| error.to_string())?;
    let payload = |record: &crate::record::Record, plugin: &str, masters: &[String]| {
        let bytes = record
            .fields
            .iter()
            .find_map(|field| match (&field.sig.0, &field.value) {
                (b"VMAD", crate::record::FieldValue::Bytes(bytes)) => Some(bytes),
                _ => None,
            })
            .ok_or("trap VMAD is missing")?;
        esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(bytes, masters, plugin, Some("ACTI"))
            .ok_or("cannot decode trap properties")
    };
    let source_payload = payload(&source, &source_plugin, &source_masters)?;
    let target_payload = payload(target, &target_plugin, &target_masters)?;
    creature_attack_scripts::traps::validate_properties(
        &source_payload,
        &target_payload,
        &mut |key| {
            let source = crate::ids::FormKey::parse(key, &run.interner)?;
            let mapped = *mappings
                .get(&source)
                .ok_or_else(|| format!("unmapped trap dependency {key}"))?;
            if run
                .read_target_record_if_available(mapped)
                .map_err(|error| error.to_string())?
                .is_none()
            {
                return Err(format!(
                    "trap dependency {key} was not emitted as {}",
                    mapped.format(&run.interner)
                ));
            }
            Ok(mapped.format(&run.interner))
        },
    )
}
