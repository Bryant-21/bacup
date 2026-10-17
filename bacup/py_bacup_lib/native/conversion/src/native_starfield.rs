use std::collections::HashSet;
use std::path::{Path, PathBuf};

use materials_native::convert::ConvertMaterialsRequest;
use nif_core_native::model::NifFile;
use serde_json::Value;

use crate::phase::{PhaseCtx, PhaseError, PhaseReport};
use crate::run::ConversionRun;
use crate::source_inventory::Inventory;
use crate::translator::Game;

pub fn active(run: &ConversionRun) -> bool {
    crate::run::native_assets_active(run) && run.source == Game::Starfield
}

pub fn material_namespace(run: &ConversionRun) -> String {
    let namespace = crate::run::base_asset_namespace_for_run(run);
    if namespace.is_empty() {
        "Starfield".into()
    } else {
        namespace
    }
}

fn resource_path(path: &str, root: &str) -> Result<String, PhaseError> {
    let path = path
        .trim_end_matches('\0')
        .replace('\\', "/")
        .to_ascii_lowercase();
    let path = path.strip_prefix("data/").unwrap_or(&path);
    if path.is_empty()
        || path.contains(':')
        || path.starts_with('/')
        || path
            .split('/')
            .any(|part| part == ".." || part == "." || part.is_empty())
    {
        return Err(PhaseError::BadParams(format!(
            "invalid Starfield resource path: {path}"
        )));
    }
    Ok(if path.starts_with(&format!("{root}/")) {
        path.into()
    } else {
        format!("{root}/{path}")
    })
}

fn source_root<'a>(ctx: &'a PhaseCtx<'_>) -> &'a Path {
    ctx.run
        .config
        .source_extracted_dir
        .as_deref()
        .unwrap_or(ctx.source_extracted_dir)
}

fn resolve(inventory: &mut Inventory, root: &Path, path: &str) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    for part in path.split('/') {
        current = inventory.child_case_insensitive(&current, part)?;
    }
    current.is_file().then_some(current)
}

fn namespaced(path: &str, namespace: &str) -> String {
    crate::relocation::insert_namespace_after_root(path, namespace)
}

fn write(ctx: &PhaseCtx<'_>, path: &str, bytes: &[u8]) -> Result<(), PhaseError> {
    crate::sinks::sink_write_or_fs(
        ctx.run.output_sink.as_ref(),
        &ctx.mod_path.join("data").join(path),
        path,
        bytes,
    )
    .map_err(PhaseError::Internal)
}

fn read(path: &Path) -> Result<Vec<u8>, PhaseError> {
    std::fs::read(path)
        .map_err(|error| PhaseError::Internal(format!("{}: {error}", path.display())))
}

fn rewrite_dependencies(
    value: &mut Value,
    namespace: &str,
    dependencies: &mut Vec<String>,
) -> Result<(), PhaseError> {
    match value {
        Value::String(path) => {
            let lower = path.to_ascii_lowercase();
            let root = if lower.ends_with(".mat") {
                "materials"
            } else if lower.ends_with(".dds") {
                "textures"
            } else {
                return Ok(());
            };
            let source = resource_path(path, root)?;
            // These are engine schema identifiers, not files in the extracted material tree.
            if source.starts_with("materials/layered/root/") {
                return Ok(());
            }
            *path = namespaced(&source, namespace);
            dependencies.push(source);
        }
        Value::Array(values) => {
            for value in values {
                rewrite_dependencies(value, namespace, dependencies)?;
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                rewrite_dependencies(value, namespace, dependencies)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn copy_materials(ctx: &PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    let request = ConvertMaterialsRequest::from_json(ctx.params).map_err(PhaseError::BadParams)?;
    let source = source_root(ctx);
    let namespace = material_namespace(ctx.run);
    let mut inventory = Inventory::default();
    let mut pending = Vec::new();
    for entry in request.materials {
        pending.push((
            resource_path(&entry.source_path, "materials")?,
            (!entry.resolved_path.is_empty()).then(|| PathBuf::from(entry.resolved_path)),
        ));
    }
    if request.convert_all {
        for path in inventory.files(
            &source.join("materials"),
            &[".mat".into()],
            true,
            true,
            false,
        ) {
            let file = PathBuf::from(path);
            let relative = file
                .strip_prefix(source)
                .map_err(|error| PhaseError::Internal(error.to_string()))?;
            pending.push((
                resource_path(&relative.to_string_lossy(), "materials")?,
                Some(file),
            ));
        }
    }
    if let Some(nifs) = ctx.params["nif_paths"].as_array() {
        for entry in nifs {
            ctx.check_cancel()?;
            let Some(path) = entry["resolved_path"]
                .as_str()
                .filter(|path| !path.is_empty())
            else {
                continue;
            };
            let references = NifFile::load_referenced_asset_paths(Path::new(path))
                .map_err(|error| PhaseError::Internal(format!("{path}: {error}")))?;
            for material in references.materials {
                if material.to_ascii_lowercase().ends_with(".mat") {
                    pending.push((resource_path(&material, "materials")?, None));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    let mut report = PhaseReport::default();
    while let Some((path, resolved)) = pending.pop() {
        ctx.check_cancel()?;
        if !seen.insert(path.clone()) {
            continue;
        }
        let resolved = resolved.filter(|file| file.is_file()).or_else(|| resolve(&mut inventory, source, &path))
            .ok_or_else(|| PhaseError::Internal(format!("Starfield native dependency missing: {path}; extract JSON .mat files and their textures")))?;
        let mut bytes = read(&resolved)?;
        if path.ends_with(".mat") {
            let mut material: Value = serde_json::from_slice(&bytes).map_err(|error| {
                PhaseError::Internal(format!("{path}: expected JSON .mat: {error}"))
            })?;
            let mut dependencies = Vec::new();
            rewrite_dependencies(&mut material, &namespace, &mut dependencies)?;
            pending.extend(dependencies.into_iter().map(|path| (path, None)));
            bytes = serde_json::to_vec(&material)
                .map_err(|error| PhaseError::Internal(error.to_string()))?;
        }
        write(ctx, &namespaced(&path, &namespace), &bytes)?;
        report.assets_written += 1;
    }
    Ok(report)
}

pub fn copy_textures(ctx: &PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    let source = source_root(ctx);
    let mut inventory = Inventory::default();
    let mut entries = crate::phase::textures::parse_texture_entries(ctx.params.get("textures"));
    if ctx.params["convert_all"].as_bool().unwrap_or(false) {
        entries.extend(
            inventory
                .files(
                    &source.join("textures"),
                    &[".dds".into()],
                    true,
                    true,
                    false,
                )
                .into_iter()
                .map(|source_path| crate::phase::textures::TextureEntry {
                    source_path,
                    output_subpath: None,
                }),
        );
    }
    let mut seen = HashSet::new();
    let mut report = PhaseReport::default();
    for entry in entries {
        ctx.check_cancel()?;
        let candidate = Path::new(&entry.source_path);
        let relative = candidate.strip_prefix(source).unwrap_or(candidate);
        let relative = resource_path(&relative.to_string_lossy(), "textures")?;
        let output = if let Some(path) = entry.output_subpath {
            resource_path(&path, "textures")?
        } else if ctx.run.relocation_members.contains(&relative) {
            namespaced(&relative, &material_namespace(ctx.run))
        } else {
            relative.clone()
        };
        if !seen.insert(output.to_ascii_lowercase()) {
            continue;
        }
        let target_owned = ctx
            .run
            .target_assets
            .as_ref()
            .is_some_and(|store| store.has_asset(&output))
            || [ctx.target_extracted_dir, ctx.target_data_dir]
                .into_iter()
                .flatten()
                .any(|root| root.join(&output).is_file());
        if target_owned {
            report.records_dropped += 1;
            continue;
        }
        let resolved = resolve(&mut inventory, source, &relative).ok_or_else(|| {
            PhaseError::Internal(format!("Starfield texture missing: {relative}"))
        })?;
        // Always replace an earlier converted DDS, even with skip-existing enabled.
        write(ctx, &output, &read(&resolved)?)?;
        report.assets_written += 1;
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
