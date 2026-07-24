use std::path::{Component, Path, PathBuf};

use crate::materialized_npc_facegen::{FacegenAlias, FacegenAliasManifest, manifest_path};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

pub struct CopyMaterializedFacegenPhase;

impl Phase for CopyMaterializedFacegenPhase {
    fn name(&self) -> &'static str {
        "copy_materialized_facegen"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let path = manifest_path(ctx.mod_path);
        if !path.is_file() {
            return Ok(PhaseReport::default());
        }
        let manifest: FacegenAliasManifest =
            serde_json::from_slice(&std::fs::read(&path).map_err(|error| {
                PhaseError::Internal(format!("read {}: {error}", path.display()))
            })?)
            .map_err(|error| PhaseError::Internal(format!("decode {}: {error}", path.display())))?;

        let mut report = PhaseReport::default();
        let data_root =
            find_child_ci(ctx.mod_path, "data").unwrap_or_else(|| ctx.mod_path.join("data"));
        let source_roots = facegen_source_roots(ctx, &data_root);
        for alias in &manifest.aliases {
            ctx.check_cancel()?;
            match copy_facegen_asset(&source_roots, &data_root, alias, AssetKind::Geometry) {
                Ok(Some(output)) => {
                    report.assets_written += 1;
                    if !register_with_sink(ctx, &data_root, &output) {
                        report.items_failed += 1;
                    }
                }
                Ok(None) if target_asset_exists(&data_root, alias, AssetKind::Geometry) => {}
                Ok(None) => {
                    report.warnings += 1;
                    report.items_failed += 1;
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "copy_materialized_facegen",
                        level: LogLevel::Warn,
                        message: format!(
                            "materialized NPC {:06X}: converted FaceGen geometry for {:06X} was not found",
                            alias.target_local, alias.source_local
                        ),
                    });
                }
                Err(error) => return Err(error),
            }
            match copy_facegen_asset(&source_roots, &data_root, alias, AssetKind::Tint) {
                Ok(Some(output)) => {
                    report.assets_written += 1;
                    if !register_with_sink(ctx, &data_root, &output) {
                        report.items_failed += 1;
                    }
                }
                Ok(None) => {}
                Err(error) => return Err(error),
            }
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "copy_materialized_facegen",
            level: LogLevel::Info,
            message: format!(
                "copy_materialized_facegen: aliases={} assets_written={} missing_geometry={}",
                manifest.aliases.len(),
                report.assets_written,
                report.warnings
            ),
        });
        Ok(report)
    }
}

#[derive(Clone, Copy)]
enum AssetKind {
    Geometry,
    Tint,
}

impl AssetKind {
    fn relative_path(self, plugin: &str, local: u32) -> PathBuf {
        match self {
            Self::Geometry => PathBuf::from("Meshes")
                .join("Actors")
                .join("Character")
                .join("FaceGenData")
                .join("FaceGeom")
                .join(plugin)
                .join(format!("{local:08x}.nif")),
            Self::Tint => PathBuf::from("Textures")
                .join("Actors")
                .join("Character")
                .join("FaceGenData")
                .join("FaceTint")
                .join(plugin)
                .join(format!("{local:08x}.dds")),
        }
    }
}

fn copy_facegen_asset(
    source_roots: &[PathBuf],
    data_root: &Path,
    alias: &FacegenAlias,
    kind: AssetKind,
) -> Result<Option<PathBuf>, PhaseError> {
    validate_plugin_dir(&alias.source_plugin)?;
    validate_plugin_dir(&alias.target_plugin)?;
    let source_relative = kind.relative_path(&alias.source_plugin, alias.source_local);
    let Some(source) = source_roots
        .iter()
        .find_map(|root| resolve_path_ci(root, &source_relative))
    else {
        return Ok(None);
    };
    let output = data_root.join(kind.relative_path(&alias.target_plugin, alias.target_local));
    if source == output {
        return Ok(Some(output));
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            PhaseError::Internal(format!("mkdir {}: {error}", parent.display()))
        })?;
    }
    std::fs::copy(&source, &output).map_err(|error| {
        PhaseError::Internal(format!(
            "copy {} -> {}: {error}",
            source.display(),
            output.display()
        ))
    })?;
    Ok(Some(output))
}

fn facegen_source_roots(ctx: &PhaseCtx<'_>, data_root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![data_root.to_path_buf()];
    for candidate in [ctx.target_extracted_dir, ctx.target_data_dir]
        .into_iter()
        .flatten()
    {
        if let Some(data) = find_child_ci(candidate, "data") {
            roots.push(data);
        }
        roots.push(candidate.to_path_buf());
    }
    roots.dedup();
    roots
}

fn target_asset_exists(data_root: &Path, alias: &FacegenAlias, kind: AssetKind) -> bool {
    resolve_path_ci(
        data_root,
        &kind.relative_path(&alias.target_plugin, alias.target_local),
    )
    .is_some()
}

fn validate_plugin_dir(plugin: &str) -> Result<(), PhaseError> {
    let path = Path::new(plugin);
    if plugin.is_empty()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(PhaseError::BadParams(format!(
            "unsafe FaceGen plugin directory: {plugin}"
        )));
    }
    Ok(())
}

fn find_child_ci(parent: &Path, name: &str) -> Option<PathBuf> {
    let expected = name.to_ascii_lowercase();
    std::fs::read_dir(parent)
        .ok()?
        .flatten()
        .find(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase() == expected)
        .map(|entry| entry.path())
}

fn resolve_path_ci(root: &Path, relative: &Path) -> Option<PathBuf> {
    let mut resolved = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return None;
        };
        let expected = name.to_string_lossy().to_ascii_lowercase();
        resolved = std::fs::read_dir(&resolved)
            .ok()?
            .flatten()
            .find(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase() == expected)?
            .path();
    }
    resolved.is_file().then_some(resolved)
}

fn register_with_sink(ctx: &PhaseCtx<'_>, data_root: &Path, output: &Path) -> bool {
    let Some(sink) = ctx.run.output_sink.as_deref() else {
        return true;
    };
    let Ok(relative) = output.strip_prefix(data_root) else {
        return false;
    };
    sink.add_existing_file(&relative.to_string_lossy().replace('\\', "/"), output)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_case_insensitive_facegen_geometry_to_clone_form_id() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let source = data
            .join("Meshes")
            .join("actors")
            .join("character")
            .join("facegendata")
            .join("facegeom")
            .join("seventysix.esm")
            .join("006529fe.nif");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"converted face").unwrap();
        let alias = FacegenAlias {
            source_plugin: "SeventySix.esm".into(),
            source_local: 0x6529FE,
            target_plugin: "SeventySix.esm".into(),
            target_local: 0xF00001,
        };

        let output = copy_facegen_asset(
            std::slice::from_ref(&data),
            &data,
            &alias,
            AssetKind::Geometry,
        )
        .unwrap()
        .unwrap();

        assert_eq!(std::fs::read(output).unwrap(), b"converted face");
        assert!(target_asset_exists(&data, &alias, AssetKind::Geometry));
    }
}
