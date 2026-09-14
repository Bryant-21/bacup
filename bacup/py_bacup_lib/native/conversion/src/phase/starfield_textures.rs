// Params shape (JSON passed via `run_phase`):
// {
//   "texture_sets": [
//     {
//       "base_name":     "wall01",                         // output filename stem (no suffix/ext)
//       "output_subdir": "Architecture",                     // optional, relative under data/Textures
//       "color":         "Textures/Architecture/wall01_d.dds",  // required; FO4 _d source
//       "normal":        "Textures/Architecture/wall01_n.dds",  // optional; FO4 _n source
//       "spec_gloss":    "Textures/Architecture/wall01_s.dds",  // optional; FO4 _s source (R=spec, G=gloss)
//       "emissive":      "Textures/Architecture/wall01_g.dds",  // optional; FO4 _g source
//       "environment_mapping": false                          // from the source BGSM header
//     },
//     ...
//   ],
//   "source_extracted": "/abs/path/to/extracted"    // source game BA2-extracted root; relative
//                                                     // set paths above are resolved against it
// }
//
// Channel math: _d -> _color (BC7_UNORM_SRGB, mips preserved), _n -> _normal (BC5_UNORM,
// mips preserved), _s (R=spec, G=gloss) -> _rough (BC4_UNORM, rough = 255 - gloss) +
// _metal (BC4_UNORM, all-zero unless environment_mapping, then metal = spec), _g -> _emissive
// (BC7_UNORM_SRGB) when present. A single shared 1x1 white BC4 `_ao` texture is written once
// under data/Textures and reused by every set (Starfield has no per-set AO input from FO4).
//
// Phase output: DDS files written under `mod_path/data/Textures/...`.
// PhaseReport.assets_written = total DDS files written (including the shared AO texture).
// PhaseReport.warnings       = texture sets that failed to convert.
// Custom counters (`sets_converted`, `mips_min`) don't fit the shared PhaseReport shape, so
// they're emitted as an Info Log summary instead — see the `sets_converted=.. mips_min=..`
// message at the end of `run`.

use std::path::{Path, PathBuf};

use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

const COLOR_FORMAT: &str = "BC7_UNORM_SRGB";
const NORMAL_FORMAT: &str = "BC5_UNORM";
const SINGLE_CHANNEL_FORMAT: &str = "BC4_UNORM";

/// Relative to `data/Textures`. Shared by every converted material set — Starfield's
/// AO slot has no FO4 source input, so we ship one constant white map.
pub const SHARED_AO_OUTPUT_REL: &str = "PBR/starfield_shared_ao.dds";

pub struct TextureSetPaths {
    pub base_name: String,
    pub color: PathBuf,
    pub normal: Option<PathBuf>,
    pub spec_gloss: Option<PathBuf>,
    pub emissive: Option<PathBuf>,
}

pub struct BgsmFlags {
    pub environment_mapping: bool,
}

#[derive(Debug, Default)]
pub struct TextureSetOutput {
    pub files_written: Vec<PathBuf>,
    /// Minimum mip count observed among outputs with max(width, height) >= 4.
    /// `None` if every texture in this set was smaller than 4px.
    pub mips_min: Option<u32>,
}

struct EncodedTexture {
    bytes: Vec<u8>,
    width: u32,
    height: u32,
    mip_count: u32,
}

fn read_and_encode(source: &Path, target_format: &str) -> Result<EncodedTexture, String> {
    let decoded = directxtex_native::read_dds_mips_rgba8(source)
        .map_err(|error| format!("{}: {error}", source.display()))?;
    let bytes =
        directxtex_native::encode_dds_from_rgba8_chain(&decoded.mips, target_format, false, None)
            .map_err(|error| format!("{}: {error}", source.display()))?;
    Ok(EncodedTexture {
        bytes,
        width: decoded.width,
        height: decoded.height,
        mip_count: decoded.mips.len() as u32,
    })
}

fn note_mips(result: &mut TextureSetOutput, width: u32, height: u32, mip_count: u32) {
    if width.max(height) >= 4 {
        result.mips_min = Some(result.mips_min.map_or(mip_count, |m| m.min(mip_count)));
    }
}

fn write_output_dds(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(dest, bytes).map_err(|error| error.to_string())
}

/// Splits an FO4 `_s` (R=spec, G=gloss) mip chain into a Starfield `_rough` chain
/// (`rough = 255 - gloss`) and `_metal` chain (all-zero unless `environment_mapping`,
/// in which case `metal = spec`). Only the R channel of each output pixel is
/// meaningful — BC4 encoding reads R only — the rest are filled for a valid RGBA8 buffer.
fn rough_metal_chains(
    mips: &[(u32, u32, Vec<u8>)],
    environment_mapping: bool,
) -> (Vec<(u32, u32, Vec<u8>)>, Vec<(u32, u32, Vec<u8>)>) {
    let mut rough = Vec::with_capacity(mips.len());
    let mut metal = Vec::with_capacity(mips.len());
    for (width, height, px) in mips {
        let mut rough_px = vec![0u8; px.len()];
        let mut metal_px = vec![0u8; px.len()];
        for chunk in 0..(px.len() / 4) {
            let i = chunk * 4;
            let spec = px[i];
            let gloss = px[i + 1];
            rough_px[i] = 255 - gloss;
            rough_px[i + 3] = 255;
            metal_px[i] = if environment_mapping { spec } else { 0 };
            metal_px[i + 3] = 255;
        }
        rough.push((*width, *height, rough_px));
        metal.push((*width, *height, metal_px));
    }
    (rough, metal)
}

/// Writes the shared 1x1 white AO texture under `textures_root` if it doesn't already
/// exist. Idempotent so every set can call it without re-encoding.
pub fn write_shared_ao_texture(textures_root: &Path) -> Result<PathBuf, String> {
    let dest = textures_root.join(SHARED_AO_OUTPUT_REL.replace('/', std::path::MAIN_SEPARATOR_STR));
    if dest.is_file() {
        return Ok(dest);
    }
    let white_pixel = vec![255u8, 255, 255, 255];
    let bytes = directxtex_native::encode_dds_from_rgba8_chain(
        &[(1, 1, white_pixel)],
        SINGLE_CHANNEL_FORMAT,
        false,
        None,
    )?;
    write_output_dds(&dest, &bytes)?;
    Ok(dest)
}

/// Converts one FO4 texture set (`_d`/`_n`/`_s`/`_g`) to its Starfield equivalents
/// under `out_dir`. `paths.color` is required; the rest are optional per what the
/// source BGSM actually referenced.
pub fn convert_texture_set(
    paths: &TextureSetPaths,
    flags: &BgsmFlags,
    out_dir: &Path,
) -> Result<TextureSetOutput, String> {
    let mut result = TextureSetOutput::default();

    let color = read_and_encode(&paths.color, COLOR_FORMAT)?;
    let color_dest = out_dir.join(format!("{}_color.dds", paths.base_name));
    write_output_dds(&color_dest, &color.bytes)?;
    note_mips(&mut result, color.width, color.height, color.mip_count);
    result.files_written.push(color_dest);

    if let Some(normal_src) = &paths.normal {
        let normal = read_and_encode(normal_src, NORMAL_FORMAT)?;
        let dest = out_dir.join(format!("{}_normal.dds", paths.base_name));
        write_output_dds(&dest, &normal.bytes)?;
        note_mips(&mut result, normal.width, normal.height, normal.mip_count);
        result.files_written.push(dest);
    }

    if let Some(spec_gloss_src) = &paths.spec_gloss {
        let decoded = directxtex_native::read_dds_mips_rgba8(spec_gloss_src)
            .map_err(|error| format!("{}: {error}", spec_gloss_src.display()))?;
        let (rough_chain, metal_chain) =
            rough_metal_chains(&decoded.mips, flags.environment_mapping);

        let rough_bytes = directxtex_native::encode_dds_from_rgba8_chain(
            &rough_chain,
            SINGLE_CHANNEL_FORMAT,
            false,
            None,
        )
        .map_err(|error| format!("{}: {error}", spec_gloss_src.display()))?;
        let rough_dest = out_dir.join(format!("{}_rough.dds", paths.base_name));
        write_output_dds(&rough_dest, &rough_bytes)?;
        note_mips(
            &mut result,
            decoded.width,
            decoded.height,
            rough_chain.len() as u32,
        );
        result.files_written.push(rough_dest);

        let metal_bytes = directxtex_native::encode_dds_from_rgba8_chain(
            &metal_chain,
            SINGLE_CHANNEL_FORMAT,
            false,
            None,
        )
        .map_err(|error| format!("{}: {error}", spec_gloss_src.display()))?;
        let metal_dest = out_dir.join(format!("{}_metal.dds", paths.base_name));
        write_output_dds(&metal_dest, &metal_bytes)?;
        note_mips(
            &mut result,
            decoded.width,
            decoded.height,
            metal_chain.len() as u32,
        );
        result.files_written.push(metal_dest);
    }

    if let Some(emissive_src) = &paths.emissive {
        let emissive = read_and_encode(emissive_src, COLOR_FORMAT)?;
        let dest = out_dir.join(format!("{}_emissive.dds", paths.base_name));
        write_output_dds(&dest, &emissive.bytes)?;
        note_mips(
            &mut result,
            emissive.width,
            emissive.height,
            emissive.mip_count,
        );
        result.files_written.push(dest);
    }

    Ok(result)
}

struct TextureSetEntry {
    base_name: String,
    output_subdir: String,
    paths: TextureSetPaths,
    flags: BgsmFlags,
}

fn resolve_path(source_extracted: &Path, raw: &str) -> PathBuf {
    let p = Path::new(raw);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        source_extracted.join(p)
    }
}

fn optional_path(entry: &JsonValue, key: &str, source_extracted: &Path) -> Option<PathBuf> {
    entry
        .get(key)
        .and_then(JsonValue::as_str)
        .map(|raw| resolve_path(source_extracted, raw))
}

fn parse_texture_sets(
    params: &JsonValue,
    source_extracted: &Path,
) -> Result<Vec<TextureSetEntry>, PhaseError> {
    let Some(arr) = params.get("texture_sets").and_then(JsonValue::as_array) else {
        return Ok(Vec::new());
    };
    arr.iter()
        .enumerate()
        .map(|(index, entry)| {
            let base_name = entry
                .get("base_name")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    PhaseError::BadParams(format!("texture_sets[{index}].base_name missing"))
                })?
                .to_string();
            let output_subdir = entry
                .get("output_subdir")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_string();
            let color_raw = entry
                .get("color")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    PhaseError::BadParams(format!("texture_sets[{index}].color missing"))
                })?;
            let environment_mapping = entry
                .get("environment_mapping")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false);
            Ok(TextureSetEntry {
                paths: TextureSetPaths {
                    base_name: base_name.clone(),
                    color: resolve_path(source_extracted, color_raw),
                    normal: optional_path(entry, "normal", source_extracted),
                    spec_gloss: optional_path(entry, "spec_gloss", source_extracted),
                    emissive: optional_path(entry, "emissive", source_extracted),
                },
                flags: BgsmFlags {
                    environment_mapping,
                },
                base_name,
                output_subdir,
            })
        })
        .collect()
}

pub struct StarfieldTexturesPhase;

impl Phase for StarfieldTexturesPhase {
    fn name(&self) -> &'static str {
        "starfield_textures"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let sets = parse_texture_sets(ctx.params, ctx.source_extracted_dir)?;
        let textures_root = ctx.mod_path.join("data").join("Textures");

        let mut assets_written = 0u32;
        let mut warnings = 0u32;
        let mut sets_converted = 0u32;
        let mut mips_min: Option<u32> = None;

        if !sets.is_empty() {
            match write_shared_ao_texture(&textures_root) {
                Ok(_) => assets_written += 1,
                Err(error) => {
                    warnings += 1;
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "starfield_textures",
                        level: LogLevel::Error,
                        message: format!("starfield_textures: shared AO texture failed: {error}"),
                    });
                }
            }
        }

        let reporter = ProgressReporter::new(
            "starfield_textures",
            sets.len() as u32,
            ctx.run.event_tx.clone(),
        );

        for entry in &sets {
            ctx.check_cancel()?;
            reporter.set_item(entry.base_name.clone());
            let out_dir = if entry.output_subdir.is_empty() {
                textures_root.clone()
            } else {
                textures_root.join(&entry.output_subdir)
            };
            match convert_texture_set(&entry.paths, &entry.flags, &out_dir) {
                Ok(result) => {
                    assets_written += result.files_written.len() as u32;
                    sets_converted += 1;
                    if let Some(set_min) = result.mips_min {
                        mips_min = Some(mips_min.map_or(set_min, |m| m.min(set_min)));
                    }
                }
                Err(error) => {
                    warnings += 1;
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "starfield_textures",
                        level: LogLevel::Error,
                        message: format!("starfield_textures: {}: {error}", entry.base_name),
                    });
                }
            }
            reporter.inc(1);
        }
        reporter.finish();

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "starfield_textures",
            level: LogLevel::Info,
            message: format!(
                "starfield_textures: sets_converted={sets_converted} mips_min={}",
                mips_min
                    .map(|m| m.to_string())
                    .unwrap_or_else(|| "n/a".to_string())
            ),
        });

        Ok(PhaseReport {
            assets_written,
            warnings,
            items_failed: warnings,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_constant_dds(path: &Path, width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) {
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        for px in rgba.chunks_exact_mut(4) {
            px[0] = r;
            px[1] = g;
            px[2] = b;
            px[3] = a;
        }
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        directxtex_native::write_dds_rgba_image(path, width, height, &rgba, "R8G8B8A8_UNORM", true)
            .unwrap();
    }

    #[test]
    fn convert_texture_set_computes_rough_from_gloss_and_preserves_mip_chain() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");

        // 8x8 -> full mip chain of 4 levels (8,4,2,1).
        write_constant_dds(&src_dir.join("wall01_d.dds"), 8, 8, 200, 100, 50, 255);
        // spec=200, gloss=180 constant -> rough should be 255-180=75.
        write_constant_dds(&src_dir.join("wall01_s.dds"), 8, 8, 200, 180, 0, 255);

        let out_dir = tmp.path().join("out");
        let paths = TextureSetPaths {
            base_name: "wall01".to_string(),
            color: src_dir.join("wall01_d.dds"),
            normal: None,
            spec_gloss: Some(src_dir.join("wall01_s.dds")),
            emissive: None,
        };
        let flags = BgsmFlags {
            environment_mapping: false,
        };

        let result = convert_texture_set(&paths, &flags, &out_dir).expect("conversion succeeds");

        let rough_path = out_dir.join("wall01_rough.dds");
        assert!(result.files_written.contains(&rough_path));
        let rough_decoded = directxtex_native::read_dds_mips_rgba8(&rough_path).unwrap();
        assert_eq!(
            rough_decoded.mips.len(),
            4,
            "mip count preserved from source"
        );
        assert_eq!(rough_decoded.mips[0].2[0], 255 - 180);

        let color_path = out_dir.join("wall01_color.dds");
        assert!(result.files_written.contains(&color_path));
        let color_probe = directxtex_native::read_dds_probe(&color_path).unwrap();
        assert_eq!(
            color_probe.dxgi_format, 99,
            "expected DXGI_FORMAT_BC7_UNORM_SRGB (99)"
        );
        let color_decoded = directxtex_native::read_dds_mips_rgba8(&color_path).unwrap();
        assert_eq!(color_decoded.mips.len(), 4);

        assert_eq!(result.mips_min, Some(4));
    }

    #[test]
    fn metal_is_zero_unless_environment_mapping_then_equals_spec() {
        let tmp = tempfile::tempdir().unwrap();
        let src_dir = tmp.path().join("src");
        write_constant_dds(&src_dir.join("gun_d.dds"), 4, 4, 10, 20, 30, 255);
        write_constant_dds(&src_dir.join("gun_s.dds"), 4, 4, 220, 90, 0, 255);

        let base_paths = |spec_gloss: PathBuf| TextureSetPaths {
            base_name: "gun".to_string(),
            color: src_dir.join("gun_d.dds"),
            normal: None,
            spec_gloss: Some(spec_gloss),
            emissive: None,
        };

        let out_no_env = tmp.path().join("out_no_env");
        convert_texture_set(
            &base_paths(src_dir.join("gun_s.dds")),
            &BgsmFlags {
                environment_mapping: false,
            },
            &out_no_env,
        )
        .unwrap();
        let metal_off =
            directxtex_native::read_dds_mips_rgba8(&out_no_env.join("gun_metal.dds")).unwrap();
        assert_eq!(metal_off.mips[0].2[0], 0);

        let out_env = tmp.path().join("out_env");
        convert_texture_set(
            &base_paths(src_dir.join("gun_s.dds")),
            &BgsmFlags {
                environment_mapping: true,
            },
            &out_env,
        )
        .unwrap();
        let metal_on =
            directxtex_native::read_dds_mips_rgba8(&out_env.join("gun_metal.dds")).unwrap();
        assert_eq!(metal_on.mips[0].2[0], 220);
    }

    #[test]
    fn shared_ao_texture_is_one_by_one_white_and_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let textures_root = tmp.path().join("data/Textures");

        let first = write_shared_ao_texture(&textures_root).unwrap();
        let bytes_after_first = std::fs::read(&first).unwrap();

        let decoded = directxtex_native::read_dds_mips_rgba8(&first).unwrap();
        assert_eq!(decoded.width, 1);
        assert_eq!(decoded.height, 1);
        assert_eq!(decoded.mips.len(), 1);
        assert_eq!(decoded.mips[0].2[0], 255);

        let second = write_shared_ao_texture(&textures_root).unwrap();
        assert_eq!(second, first);
        assert_eq!(std::fs::read(&second).unwrap(), bytes_after_first);
    }

    #[test]
    fn phase_run_converts_one_set_and_writes_shared_ao() {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        use std::sync::atomic::AtomicBool;

        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let mod_path = tmp.path().join("mod");
        write_constant_dds(
            &source.join("Textures/Weapons/gun01_d.dds"),
            4,
            4,
            50,
            60,
            70,
            255,
        );
        write_constant_dds(
            &source.join("Textures/Weapons/gun01_s.dds"),
            4,
            4,
            150,
            200,
            0,
            255,
        );

        let id = create_run(RunParams {
            source: Game::Fo4,
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

        let params = serde_json::json!({
            "texture_sets": [{
                "base_name": "gun01",
                "output_subdir": "Weapons",
                "color": "Textures/Weapons/gun01_d.dds",
                "spec_gloss": "Textures/Weapons/gun01_s.dds",
                "environment_mapping": false
            }],
            "source_extracted": source.to_string_lossy(),
        });

        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = AtomicBool::new(false);
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_path,
                source_extracted_dir: &source,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            StarfieldTexturesPhase
                .run(&mut ctx)
                .map_err(|error| RunError::InvalidConfig(error.to_string()))
        })
        .unwrap();

        // color + rough + metal for the one set, plus the shared AO texture.
        assert_eq!(report.assets_written, 4);
        assert_eq!(report.warnings, 0);

        assert!(
            mod_path
                .join("data/Textures/Weapons/gun01_color.dds")
                .is_file()
        );
        assert!(
            mod_path
                .join("data/Textures/Weapons/gun01_rough.dds")
                .is_file()
        );
        assert!(
            mod_path
                .join("data/Textures/Weapons/gun01_metal.dds")
                .is_file()
        );
        assert!(
            mod_path
                .join("data/Textures")
                .join(SHARED_AO_OUTPUT_REL.replace('/', std::path::MAIN_SEPARATOR_STR))
                .is_file()
        );

        let events = with_run(id, |run| -> Result<Vec<PhaseEvent>, RunError> {
            Ok(run.event_rx.try_iter().collect())
        })
        .unwrap();
        assert!(events.iter().any(|event| matches!(
            event,
            PhaseEvent::Log { level: LogLevel::Info, message, .. }
                if message.contains("sets_converted=1") && message.contains("mips_min=")
        )));

        drop_run(id).unwrap();
    }
}
