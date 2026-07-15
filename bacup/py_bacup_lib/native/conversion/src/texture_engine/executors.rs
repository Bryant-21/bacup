//! Per-class executors. PassThrough = byte copy; PerTexel = u8 kernel over the
//! source's own mips (no f32, no mip regen); Bundle/SpecGloss = the legacy pub
//! math kernels + legacy mip regen + chain encoder (BC7 via GpuService);
//! SingleResidue = the unmodified legacy converter on a one-pair sub-request.

use std::fs;
use std::path::Path;

use materials_native::texture_convert::{
    TextureConversionParamsPayload, TexturePathInput, TexturePathOutput, TextureSetPathRequest,
    TextureSetPathResult, convert_texture_set_paths, f32_vec_to_bytes, fo76_bundle_to_fo4_buffers,
    fo76_reflectivity_lighting_to_fo4_specgloss_buffers, output_format_for_path,
};

use super::gpu_service::GpuService;
use super::triage::TextureTask;
use super::{TriageClass, kernel_normal_zero_b};

pub(crate) struct TextureOutputSink<'a> {
    pub(crate) data_root: &'a Path,
    pub(crate) sink: &'a crate::sinks::SinkSet,
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    Ok(())
}

fn write_output_bytes(
    output: &TexturePathOutput,
    bytes: &[u8],
    sink: Option<&TextureOutputSink<'_>>,
) -> Result<(), String> {
    if let Some(writer) = sink {
        if let Ok(rel) = output.path.strip_prefix(writer.data_root) {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            return writer.sink.write_asset(&rel_str, bytes);
        }
    }
    ensure_parent(&output.path)?;
    fs::write(&output.path, bytes).map_err(|e| format!("write {}: {e}", output.path.display()))
}

pub(crate) fn execute_pass_through(
    input: &TexturePathInput,
    output: &TexturePathOutput,
    sink: Option<&TextureOutputSink<'_>>,
) -> Result<(), String> {
    if sink.is_some() {
        let bytes =
            fs::read(&input.path).map_err(|e| format!("read {}: {e}", input.path.display()))?;
        return write_output_bytes(output, &bytes, sink);
    }
    ensure_parent(&output.path)?;
    fs::copy(&input.path, &output.path)
        .map(|_| ())
        .map_err(|e| {
            format!(
                "copy {} -> {}: {e}",
                input.path.display(),
                output.path.display()
            )
        })
}

/// Shared BC7 hook: adapts GpuService::encode_bc7 to the chain-encoder
/// callback shape. The extra Vec copy per level is accepted v1 overhead.
fn bc7_via_service<'a>(
    gpu: &'a GpuService,
    gpu_min_pixels: u32,
) -> impl Fn(&[(u32, u32, &[u8])], bool) -> Result<Vec<Vec<u8>>, String> + Sync + 'a {
    move |imgs, srgb| {
        let owned: Vec<(u32, u32, Vec<u8>)> =
            imgs.iter().map(|(w, h, p)| (*w, *h, p.to_vec())).collect();
        gpu.encode_bc7(owned, srgb, gpu_min_pixels)
    }
}

pub(crate) fn execute_per_texel(
    input: &TexturePathInput,
    output: &TexturePathOutput,
    target_format: &str,
    normal_kernel: bool,
    gpu: &GpuService,
    gpu_min_pixels: u32,
    sink: Option<&TextureOutputSink<'_>>,
) -> Result<(), String> {
    let chain = directxtex_native::read_dds_mips_rgba8(&input.path)?.mips;
    encode_per_texel_chain(
        chain,
        output,
        target_format,
        normal_kernel,
        gpu,
        gpu_min_pixels,
        sink,
        false,
    )
}

fn encode_per_texel_chain(
    mut chain: Vec<(u32, u32, Vec<u8>)>,
    output: &TexturePathOutput,
    target_format: &str,
    normal_kernel: bool,
    gpu: &GpuService,
    gpu_min_pixels: u32,
    sink: Option<&TextureOutputSink<'_>>,
    mip_flooding: bool,
) -> Result<(), String> {
    if normal_kernel {
        for (_, _, px) in chain.iter_mut() {
            kernel_normal_zero_b(px);
        }
    }
    let output_format = if mip_flooding {
        let (width, height, base) = chain
            .first()
            .ok_or_else(|| "cannot mip flood an empty texture chain".to_string())?;
        let output_format =
            directxtex_native::mip_flood_output_format(target_format, base).to_string();
        chain = directxtex_native::rgba8_mip_flood_chain(*width, *height, base)?;
        output_format
    } else {
        target_format.to_string()
    };
    let bc7 = bc7_via_service(gpu, gpu_min_pixels);
    let bytes =
        directxtex_native::encode_dds_from_rgba8_chain(&chain, &output_format, false, Some(&bc7))?;
    write_output_bytes(output, &bytes, sink)
}

/// PerTexel preserves the source's own mips, which is only legacy-parity-safe
/// when those mips are box-filter-consistent with mip0 (what legacy
/// regenerates). Authored-divergent chains exist in the real corpus
/// (facecustomization tint `_l` maps: 34/39 sampled exceeded post-encode RMSE
/// 24; some decal `_d`: 2/47) and headers cannot detect them — so the gate
/// runs here, where the decoded mips are already in hand. Threshold 12 is 2x
/// margin under the corpus gate's post-encode RMSE 24 (post ≈ pre,
/// empirically). Inconsistent chains demote to the legacy residue path.
const PER_TEXEL_MIP_CONSISTENCY_MAX_RMSE: f64 = 12.0;

fn source_mips_box_consistent(chain: &[(u32, u32, Vec<u8>)]) -> bool {
    let Some((w0, h0, base)) = chain.first() else {
        return false;
    };
    let Ok(regen) = directxtex_native::rgba8_box_mip_chain(*w0, *h0, base) else {
        return false;
    };
    for k in 1..chain.len().min(regen.len()) {
        let a = &chain[k].2;
        let b = &regen[k].2;
        if a.len() != b.len() {
            return false;
        }
        let sum: f64 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| {
                let d = f64::from(*x) - f64::from(*y);
                d * d
            })
            .sum();
        if (sum / a.len() as f64).sqrt() > PER_TEXEL_MIP_CONSISTENCY_MAX_RMSE {
            return false;
        }
    }
    true
}

/// f32 buffer -> u8 (exact legacy rounding) -> legacy box mip chain -> chain
/// encoder (BC7 via GpuService).
#[allow(clippy::too_many_arguments)]
fn write_f32_as_dds_with_mips(
    rgba_f32: &[f32],
    width: u32,
    height: u32,
    format: &str,
    out_path: &Path,
    gpu: &GpuService,
    gpu_min_pixels: u32,
    use_gpu: bool,
    sink: Option<(&TexturePathOutput, &TextureOutputSink<'_>)>,
    mip_flooding: bool,
) -> Result<(), String> {
    let rgba_u8: Vec<u8> = rgba_f32
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    let chain = if mip_flooding {
        directxtex_native::rgba8_mip_flood_chain(width, height, &rgba_u8)?
    } else {
        directxtex_native::rgba8_box_mip_chain(width, height, &rgba_u8)?
    };
    let min_pixels = if use_gpu { gpu_min_pixels } else { u32::MAX };
    let bc7 = bc7_via_service(gpu, min_pixels);
    let output_format = if mip_flooding {
        directxtex_native::mip_flood_output_format(format, &rgba_u8)
    } else {
        format
    };
    let bytes =
        directxtex_native::encode_dds_from_rgba8_chain(&chain, output_format, false, Some(&bc7))?;
    if let Some((output, writer)) = sink {
        return write_output_bytes(output, &bytes, Some(writer));
    }
    ensure_parent(out_path)?;
    fs::write(out_path, bytes).map_err(|e| format!("write {}: {e}", out_path.display()))
}

/// d+r+l bundle — mirrors convert_fo76_to_fo4_paths' bundle branch: same math
/// kernel, same dims (everything at diffuse dims), same per-output format
/// mapping. Returns outputs written.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_bundle(
    diffuse: &TexturePathInput,
    reflectivity: &TexturePathInput,
    lighting: &TexturePathInput,
    out_diffuse: Option<&TexturePathOutput>,
    out_specular: Option<&TexturePathOutput>,
    out_glow: Option<&TexturePathOutput>,
    force_diffuse_alpha_opaque: bool,
    conv_params: TextureConversionParamsPayload,
    gpu: &GpuService,
    use_gpu: bool,
    gpu_min_pixels: u32,
    sink: Option<&TextureOutputSink<'_>>,
    mip_flood_diffuse: bool,
) -> Result<u32, String> {
    let d = directxtex_native::read_dds_float_rgba_image(&diffuse.path)?;
    let r = directxtex_native::read_dds_float_rgba_image(&reflectivity.path)?;
    let l = directxtex_native::read_dds_float_rgba_image(&lighting.path)?;
    let mut outputs = fo76_bundle_to_fo4_buffers(
        &f32_vec_to_bytes(&d.rgba),
        &f32_vec_to_bytes(&r.rgba),
        &f32_vec_to_bytes(&l.rgba),
        d.width as usize,
        d.height as usize,
        r.width as usize,
        r.height as usize,
        l.width as usize,
        l.height as usize,
        conv_params.into(),
        out_glow.is_some(),
    )
    .map_err(|e| e.to_string())?;
    if force_diffuse_alpha_opaque {
        for pixel in outputs.diffuse.chunks_exact_mut(4) {
            pixel[3] = 1.0;
        }
    }

    let mut written = 0u32;
    if let Some(out) = out_diffuse {
        let format = output_format_for_path(&out.role, d.dxgi_format, &out.format, &out.path);
        write_f32_as_dds_with_mips(
            &outputs.diffuse,
            d.width,
            d.height,
            &format,
            &out.path,
            gpu,
            gpu_min_pixels,
            use_gpu,
            sink.map(|s| (out, s)),
            mip_flood_diffuse,
        )?;
        written += 1;
    }
    if let Some(out) = out_specular {
        let format = output_format_for_path(&out.role, r.dxgi_format, &out.format, &out.path);
        write_f32_as_dds_with_mips(
            &outputs.specgloss,
            d.width,
            d.height,
            &format,
            &out.path,
            gpu,
            gpu_min_pixels,
            use_gpu,
            sink.map(|s| (out, s)),
            false,
        )?;
        written += 1;
    }
    if let (Some(out), Some(glow)) = (out_glow, outputs.glow.as_ref()) {
        let format = output_format_for_path(&out.role, l.dxgi_format, &out.format, &out.path);
        write_f32_as_dds_with_mips(
            glow,
            d.width,
            d.height,
            &format,
            &out.path,
            gpu,
            gpu_min_pixels,
            use_gpu,
            sink.map(|s| (out, s)),
            false,
        )?;
        written += 1;
    }
    Ok(written)
}

/// r+l (no d) — mirrors the legacy converter (output at reflectivity dims).
pub(crate) fn execute_specgloss(
    reflectivity: &TexturePathInput,
    lighting: &TexturePathInput,
    out_specular: &TexturePathOutput,
    gpu: &GpuService,
    use_gpu: bool,
    gpu_min_pixels: u32,
    sink: Option<&TextureOutputSink<'_>>,
) -> Result<(), String> {
    let r = directxtex_native::read_dds_float_rgba_image(&reflectivity.path)?;
    let l = directxtex_native::read_dds_float_rgba_image(&lighting.path)?;
    let specgloss = fo76_reflectivity_lighting_to_fo4_specgloss_buffers(
        &f32_vec_to_bytes(&r.rgba),
        &f32_vec_to_bytes(&l.rgba),
        r.width as usize,
        r.height as usize,
        l.width as usize,
        l.height as usize,
    )
    .map_err(|e| e.to_string())?;
    let format = output_format_for_path(
        &out_specular.role,
        r.dxgi_format,
        &out_specular.format,
        &out_specular.path,
    );
    write_f32_as_dds_with_mips(
        &specgloss,
        r.width,
        r.height,
        &format,
        &out_specular.path,
        gpu,
        gpu_min_pixels,
        use_gpu,
        sink.map(|s| (out_specular, s)),
        false,
    )
}

/// Residue: the unmodified legacy converter on a one-pair sub-request.
pub(crate) fn execute_residue(
    input: &TexturePathInput,
    output: &TexturePathOutput,
    conv_params: TextureConversionParamsPayload,
    use_gpu: bool,
    gpu_min_pixels: u32,
) -> Result<TextureSetPathResult, String> {
    convert_texture_set_paths(TextureSetPathRequest {
        source_game: "fo76".to_string(),
        target_game: "fo4".to_string(),
        inputs: vec![input.clone()],
        outputs: vec![output.clone()],
        params: conv_params,
        use_gpu,
        gpu_min_pixels,
        parallel_compression: false,
    })
    .map_err(|e| e.to_string())
}

fn is_landscape_diffuse_output(output: &TexturePathOutput) -> bool {
    output.role.eq_ignore_ascii_case("diffuse")
        && output
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.to_ascii_lowercase().ends_with("_d.dds"))
        && output.path.components().any(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case("landscape")
        })
}

fn execute_mip_flooded_single(
    input: &TexturePathInput,
    output: &TexturePathOutput,
    target_format: &str,
    gpu: &GpuService,
    gpu_min_pixels: u32,
    sink: Option<&TextureOutputSink<'_>>,
) -> Result<(), String> {
    let decoded = directxtex_native::read_dds_mips_rgba8(&input.path)?;
    let format = if target_format.is_empty() {
        output_format_for_path(
            &output.role,
            decoded.dxgi_format,
            &output.format,
            &output.path,
        )
    } else {
        target_format.to_string()
    };
    encode_per_texel_chain(
        decoded.mips,
        output,
        &format,
        false,
        gpu,
        gpu_min_pixels,
        sink,
        true,
    )
}

fn rewrite_output_mip_flooded(
    output: &TexturePathOutput,
    gpu: &GpuService,
    gpu_min_pixels: u32,
    sink: Option<&TextureOutputSink<'_>>,
) -> Result<(), String> {
    let decoded = directxtex_native::read_dds_mips_rgba8(&output.path)?;
    let format = output_format_for_path(
        &output.role,
        decoded.dxgi_format,
        &output.format,
        &output.path,
    );
    encode_per_texel_chain(
        decoded.mips,
        output,
        &format,
        false,
        gpu,
        gpu_min_pixels,
        sink,
        true,
    )
}

/// Dispatch one triaged task. Returns
/// (outputs_written, outputs_skipped, per_texel_demoted_to_residue, mip_flooded_outputs).
pub(crate) fn execute_task_with_landscape_mip_flooding(
    task: &TextureTask,
    conv_params: TextureConversionParamsPayload,
    gpu: &GpuService,
    use_gpu: bool,
    gpu_min_pixels: u32,
    landscape_mip_flooding: bool,
    sink: Option<&TextureOutputSink<'_>>,
) -> Result<(u32, u32, bool, u32), String> {
    match task {
        TextureTask::Single {
            input,
            output,
            target_format,
            class,
            normal_kernel,
        } => {
            let mip_flooding = landscape_mip_flooding && is_landscape_diffuse_output(output);
            match class {
                TriageClass::PassThrough if mip_flooding => {
                    let min_pixels = if use_gpu { gpu_min_pixels } else { u32::MAX };
                    execute_mip_flooded_single(input, output, target_format, gpu, min_pixels, sink)
                        .map(|()| (1, 0, false, 1))
                }
                TriageClass::PassThrough => {
                    execute_pass_through(input, output, sink).map(|()| (1, 0, false, 0))
                }
                TriageClass::PerTexel => {
                    let min_pixels = if use_gpu { gpu_min_pixels } else { u32::MAX };
                    let chain = directxtex_native::read_dds_mips_rgba8(&input.path)?.mips;
                    if source_mips_box_consistent(&chain) {
                        encode_per_texel_chain(
                            chain,
                            output,
                            target_format,
                            *normal_kernel,
                            gpu,
                            min_pixels,
                            sink,
                            mip_flooding,
                        )
                        .map(|()| (1, 0, false, u32::from(mip_flooding)))
                    } else {
                        let result =
                            execute_residue(input, output, conv_params, use_gpu, gpu_min_pixels)?;
                        if mip_flooding && !result.converted.is_empty() {
                            rewrite_output_mip_flooded(output, gpu, min_pixels, sink)?;
                        }
                        Ok((
                            result.converted.len() as u32,
                            result.skipped.len() as u32,
                            true,
                            u32::from(mip_flooding && !result.converted.is_empty()),
                        ))
                    }
                }
                TriageClass::BundleRecompile => {
                    unreachable!("triage never emits Single with BundleRecompile")
                }
            }
        }
        TextureTask::SingleResidue { input, output } => {
            let result = execute_residue(input, output, conv_params, use_gpu, gpu_min_pixels)?;
            if landscape_mip_flooding
                && is_landscape_diffuse_output(output)
                && !result.converted.is_empty()
            {
                let min_pixels = if use_gpu { gpu_min_pixels } else { u32::MAX };
                rewrite_output_mip_flooded(output, gpu, min_pixels, sink)?;
            }
            Ok((
                result.converted.len() as u32,
                result.skipped.len() as u32,
                false,
                u32::from(
                    landscape_mip_flooding
                        && is_landscape_diffuse_output(output)
                        && !result.converted.is_empty(),
                ),
            ))
        }
        TextureTask::Bundle {
            diffuse,
            reflectivity,
            lighting,
            out_diffuse,
            out_specular,
            out_glow,
            force_diffuse_alpha_opaque,
        } => {
            let mip_flood_diffuse = landscape_mip_flooding
                && out_diffuse
                    .as_ref()
                    .is_some_and(is_landscape_diffuse_output);
            execute_bundle(
                diffuse,
                reflectivity,
                lighting,
                out_diffuse.as_ref(),
                out_specular.as_ref(),
                out_glow.as_ref(),
                *force_diffuse_alpha_opaque,
                conv_params,
                gpu,
                use_gpu,
                gpu_min_pixels,
                sink,
                mip_flood_diffuse,
            )
            .map(|written| (written, 0, false, u32::from(mip_flood_diffuse)))
        }
        TextureTask::SpecGloss {
            reflectivity,
            lighting,
            out_specular,
        } => execute_specgloss(
            reflectivity,
            lighting,
            out_specular,
            gpu,
            use_gpu,
            gpu_min_pixels,
            sink,
        )
        .map(|()| (1, 0, false, 0)),
    }
}

#[cfg(test)]
pub(crate) fn execute_task(
    task: &TextureTask,
    conv_params: TextureConversionParamsPayload,
    gpu: &GpuService,
    use_gpu: bool,
    gpu_min_pixels: u32,
    sink: Option<&TextureOutputSink<'_>>,
) -> Result<(u32, u32, bool), String> {
    execute_task_with_landscape_mip_flooding(
        task,
        conv_params,
        gpu,
        use_gpu,
        gpu_min_pixels,
        false,
        sink,
    )
    .map(|(written, skipped, demoted, _)| (written, skipped, demoted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use materials_native::texture_convert::{TexturePathInput, TexturePathOutput};
    use std::path::Path;

    pub(super) fn write_tex(dir: &Path, name: &str, w: u32, h: u32, format: &str, mips: bool) {
        let rgba: Vec<u8> = (0..(w as usize) * (h as usize) * 4)
            .map(|i| (i * 31 % 256) as u8)
            .collect();
        directxtex_native::write_dds_rgba_image(&dir.join(name), w, h, &rgba, format, mips)
            .unwrap();
    }

    #[test]
    fn pass_through_output_is_byte_identical_to_source() {
        let tmp = std::env::temp_dir().join("exec_pass_through");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "rock_d.dds", 16, 16, "BC7_UNORM", true);
        let input = TexturePathInput {
            role: "diffuse".to_string(),
            path: tmp.join("rock_d.dds"),
        };
        let output = TexturePathOutput {
            role: "diffuse".to_string(),
            path: tmp.join("out").join("sub").join("rock_d.dds"),
            format: "BC7_UNORM".to_string(),
        };

        execute_pass_through(&input, &output, None).unwrap();

        let src = std::fs::read(&input.path).unwrap();
        let out = std::fs::read(&output.path).unwrap();
        assert_eq!(out, src, "PassThrough must be a byte copy");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pass_through_sink_streams_source_bytes_without_loose_rewrite() {
        let tmp = std::env::temp_dir().join("exec_pass_through_sink_direct");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "crate_d.dds", 16, 16, "BC7_UNORM", true);
        let mod_root = tmp.join("mod");
        let data_root = mod_root.join("data");
        let input = TexturePathInput {
            role: "diffuse".to_string(),
            path: tmp.join("crate_d.dds"),
        };
        let output = TexturePathOutput {
            role: "diffuse".to_string(),
            path: data_root.join("Textures").join("Props").join("crate_d.dds"),
            format: "BC7_UNORM".to_string(),
        };
        let sink = crate::sinks::SinkSet {
            ba2: Some(crate::sinks::Ba2ShardWriter::new(tmp.join("spill")).unwrap()),
            loose: crate::sinks::LooseSink {
                enabled: false,
                mod_root,
            },
            terrain: crate::sinks::TerrainSidecarSink::default(),
        };
        let writer = TextureOutputSink {
            data_root: &data_root,
            sink: &sink,
        };

        execute_pass_through(&input, &output, Some(&writer)).unwrap();

        assert!(
            !output.path.exists(),
            "no-loose sink should not write {}",
            output.path.display()
        );
        assert_eq!(
            sink.ba2.as_ref().unwrap().streamed_rel_paths(),
            vec!["textures/props/crate_d.dds".to_string()]
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn read_mips(p: &Path) -> directxtex_native::DdsMipsRgba8 {
        directxtex_native::read_dds_mips_rgba8(p).unwrap()
    }

    pub(super) fn rmse(a: &[u8], b: &[u8]) -> f64 {
        assert_eq!(a.len(), b.len());
        let sum: f64 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| {
                let d = f64::from(*x) - f64::from(*y);
                d * d
            })
            .sum();
        (sum / a.len() as f64).sqrt()
    }

    #[test]
    fn per_texel_normal_zeroes_blue_and_preserves_mip_count() {
        let tmp = std::env::temp_dir().join("exec_per_texel_normal");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "armor_n.dds", 32, 32, "BC7_UNORM", true); // BC7 normal -> PerTexel
        let input = TexturePathInput {
            role: "normal".to_string(),
            path: tmp.join("armor_n.dds"),
        };
        let output = TexturePathOutput {
            role: "normal".to_string(),
            path: tmp.join("out").join("armor_n.dds"),
            format: "BC5_UNORM".to_string(), // request fallback; target_format overrides it
        };
        let svc = GpuService::start_cpu_only();

        execute_per_texel(&input, &output, "BC7_UNORM", true, &svc, 512 * 512, None).unwrap();

        let src = read_mips(&input.path);
        let out = read_mips(&output.path);
        assert_eq!(
            out.mips.len(),
            src.mips.len(),
            "mip count preserved, never regenerated"
        );
        for (_, _, px) in &out.mips {
            // BC7 is lossy; B was exactly 0 pre-encode so reconstruction stays tiny.
            assert!(
                px.chunks_exact(4).all(|p| p[2] <= 4),
                "blue channel must be ~0"
            );
        }
        svc.shutdown();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn per_texel_mip0_byte_matches_legacy_and_deep_mips_rmse_pass() {
        // Legacy: decode base -> f32 kernel -> u8 -> regen mips -> encode.
        // Engine: decode source mips -> u8 kernel -> encode.
        // Mip 0 inputs are identical, encoder identical => decoded mip 0 EXACT.
        // Deeper mips intentionally differ (source mips vs box regen) => RMSE.
        let tmp = std::env::temp_dir().join("exec_per_texel_vs_legacy");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "armor_n.dds", 32, 32, "BC7_UNORM", true);
        let input = TexturePathInput {
            role: "normal".to_string(),
            path: tmp.join("armor_n.dds"),
        };

        // Legacy single-pair conversion (use_gpu=false for determinism).
        let legacy_out = TexturePathOutput {
            role: "normal".to_string(),
            path: tmp.join("legacy").join("armor_n.dds"),
            format: "BC5_UNORM".to_string(),
        };
        convert_texture_set_paths(TextureSetPathRequest {
            source_game: "fo76".to_string(),
            target_game: "fo4".to_string(),
            inputs: vec![input.clone()],
            outputs: vec![legacy_out.clone()],
            params: TextureConversionParamsPayload::default(),
            use_gpu: false,
            gpu_min_pixels: 0,
            parallel_compression: false,
        })
        .unwrap();

        let engine_out = TexturePathOutput {
            role: "normal".to_string(),
            path: tmp.join("new").join("armor_n.dds"),
            format: "BC5_UNORM".to_string(),
        };
        let svc = GpuService::start_cpu_only();
        execute_per_texel(
            &input,
            &engine_out,
            "BC7_UNORM",
            true,
            &svc,
            512 * 512,
            None,
        )
        .unwrap();
        svc.shutdown();

        let legacy = read_mips(&legacy_out.path);
        let ours = read_mips(&engine_out.path);
        assert_eq!(ours.mips.len(), legacy.mips.len());
        assert_eq!(
            ours.mips[0].2, legacy.mips[0].2,
            "mip 0 must decode identically"
        );
        for k in 1..ours.mips.len() {
            let e = rmse(&ours.mips[k].2, &legacy.mips[k].2);
            assert!(e <= 24.0, "mip {k} RMSE {e} exceeds tolerance");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn per_texel_identity_format_change_bc3_to_bc7() {
        let tmp = std::env::temp_dir().join("exec_per_texel_bc3");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "wall_d.dds", 16, 16, "BC3_UNORM", true);
        let input = TexturePathInput {
            role: "diffuse".to_string(),
            path: tmp.join("wall_d.dds"),
        };
        let output = TexturePathOutput {
            role: "diffuse".to_string(),
            path: tmp.join("out").join("wall_d.dds"),
            format: "BC7_UNORM".to_string(),
        };
        let svc = GpuService::start_cpu_only();
        execute_per_texel(&input, &output, "BC7_UNORM", false, &svc, 512 * 512, None).unwrap();
        svc.shutdown();
        let probe = directxtex_native::read_dds_probe(&output.path).unwrap();
        assert_eq!(probe.dxgi_format, 98, "BC7_UNORM output");
        assert_eq!(probe.mip_levels, 5);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    use crate::phase::textures::{build_request, game_texture_suffixes, group_textures};
    use std::collections::HashMap;

    fn fo76_request(dir: &Path, names: &[&str], out_dir: &Path) -> TextureSetPathRequest {
        let paths: Vec<String> = names
            .iter()
            .map(|n| dir.join(n).to_string_lossy().to_string())
            .collect();
        let groups = group_textures(&paths, dir, game_texture_suffixes("fo76"), "fo76");
        assert_eq!(groups.len(), 1);
        build_request(
            &groups[0],
            out_dir,
            "fo76",
            "fo4",
            game_texture_suffixes("fo76"),
            game_texture_suffixes("fo4"),
            &HashMap::new(),
            TextureConversionParamsPayload::default(),
            false,
            0,
        )
        .unwrap()
    }

    fn assert_outputs_byte_equal(legacy: &TextureSetPathRequest, ours: &TextureSetPathRequest) {
        let mut compared = 0usize;
        for (l, n) in legacy.outputs.iter().zip(ours.outputs.iter()) {
            assert_eq!(l.role, n.role);
            // Some build_request outputs are phantom (e.g. the r+l-only specgloss
            // fallback emits a second `specular` output from the lighting filename
            // that NEITHER converter writes). The legacy converter is the source
            // of truth for what gets written: if it skipped this output, the
            // engine must skip it too (no extra files), then move on.
            match std::fs::read(&l.path) {
                Ok(lb) => {
                    let nb = std::fs::read(&n.path)
                        .unwrap_or_else(|_| panic!("engine {} missing", n.path.display()));
                    assert_eq!(nb, lb, "role {}: engine bytes must equal legacy", l.role);
                    compared += 1;
                }
                Err(_) => assert!(
                    !n.path.is_file(),
                    "engine wrote {} but legacy did not — divergence",
                    n.path.display()
                ),
            }
        }
        assert!(compared >= 1, "no outputs were actually compared");
    }

    #[test]
    fn bundle_outputs_byte_match_legacy_converter() {
        let tmp = std::env::temp_dir().join("exec_bundle_golden");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // Varied formats: diffuse BC7, reflectivity RGBA8, lighting BC7 (alpha
        // present -> glow output emitted by build_request's bundle branch).
        write_tex(&tmp, "kit_d.dds", 16, 16, "BC7_UNORM", true);
        write_tex(&tmp, "kit_r.dds", 8, 8, "R8G8B8A8_UNORM", true); // resized by the math
        write_tex(&tmp, "kit_l.dds", 16, 16, "BC7_UNORM", true);
        let names = ["kit_d.dds", "kit_r.dds", "kit_l.dds"];

        let legacy_req = fo76_request(&tmp, &names, &tmp.join("legacy"));
        convert_texture_set_paths(legacy_req.clone()).unwrap();

        let ours_req = fo76_request(&tmp, &names, &tmp.join("new"));
        let tasks = crate::texture_engine::triage_request(&ours_req, &HashMap::new(), false);
        let svc = GpuService::start_cpu_only();
        for task in &tasks {
            execute_task(
                task,
                TextureConversionParamsPayload::default(),
                &svc,
                false,
                0,
                None,
            )
            .unwrap();
        }
        svc.shutdown();

        assert_outputs_byte_equal(&legacy_req, &ours_req);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn pbr_carry_bundle_keeps_raw_sources_and_legacy_fallbacks() {
        let tmp = std::env::temp_dir().join("exec_bundle_pbr_carry");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "kit_d.dds", 16, 16, "BC7_UNORM", true);
        write_tex(&tmp, "kit_r.dds", 8, 8, "R8G8B8A8_UNORM", true);
        write_tex(&tmp, "kit_l.dds", 16, 16, "BC7_UNORM", true);
        write_tex(&tmp, "kit_n.dds", 16, 16, "BC5_UNORM", true);
        let names = ["kit_d.dds", "kit_r.dds", "kit_l.dds", "kit_n.dds"];

        let legacy_req = fo76_request(&tmp, &names, &tmp.join("legacy"));
        convert_texture_set_paths(legacy_req.clone()).unwrap();

        let ours_req = fo76_request(&tmp, &names, &tmp.join("new"));
        let tasks = crate::texture_engine::triage_request(&ours_req, &HashMap::new(), true);
        let svc = GpuService::start_cpu_only();
        for task in &tasks {
            execute_task(
                task,
                TextureConversionParamsPayload::default(),
                &svc,
                false,
                0,
                None,
            )
            .unwrap();
        }
        svc.shutdown();

        for suffix in ["d", "r", "l", "n"] {
            assert_eq!(
                std::fs::read(tmp.join("new").join(format!("kit_{suffix}.dds"))).unwrap(),
                std::fs::read(tmp.join(format!("kit_{suffix}.dds"))).unwrap(),
                "carried _{suffix} must be byte-identical to its source"
            );
        }
        for role in ["specular", "glow"] {
            let legacy = legacy_req
                .outputs
                .iter()
                .find(|output| output.role == role)
                .unwrap();
            let ours = ours_req
                .outputs
                .iter()
                .find(|output| output.role == role)
                .unwrap();
            assert_eq!(
                std::fs::read(&ours.path).unwrap(),
                std::fs::read(&legacy.path).unwrap(),
                "{role} fallback must retain legacy bytes"
            );
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn specgloss_pair_byte_matches_legacy_converter() {
        let tmp = std::env::temp_dir().join("exec_specgloss_golden");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "pipe_r.dds", 16, 16, "BC7_UNORM", true);
        write_tex(&tmp, "pipe_l.dds", 8, 8, "BC7_UNORM", true); // lighting resized to refl dims
        let names = ["pipe_r.dds", "pipe_l.dds"];

        let legacy_req = fo76_request(&tmp, &names, &tmp.join("legacy"));
        convert_texture_set_paths(legacy_req.clone()).unwrap();

        let ours_req = fo76_request(&tmp, &names, &tmp.join("new"));
        let tasks = crate::texture_engine::triage_request(&ours_req, &HashMap::new(), false);
        assert!(
            tasks
                .iter()
                .any(|t| matches!(t, TextureTask::SpecGloss { .. }))
        );
        let svc = GpuService::start_cpu_only();
        for task in &tasks {
            execute_task(
                task,
                TextureConversionParamsPayload::default(),
                &svc,
                false,
                0,
                None,
            )
            .unwrap();
        }
        svc.shutdown();

        assert_outputs_byte_equal(&legacy_req, &ours_req);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn residue_single_byte_matches_legacy_converter() {
        // Mipless source -> triage demotes to SingleResidue -> legacy code path.
        let tmp = std::env::temp_dir().join("exec_residue_golden");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "odd_d.dds", 16, 16, "BC7_UNORM", false);
        let names = ["odd_d.dds"];

        let legacy_req = fo76_request(&tmp, &names, &tmp.join("legacy"));
        convert_texture_set_paths(legacy_req.clone()).unwrap();

        let ours_req = fo76_request(&tmp, &names, &tmp.join("new"));
        let tasks = crate::texture_engine::triage_request(&ours_req, &HashMap::new(), false);
        assert!(matches!(tasks[0], TextureTask::SingleResidue { .. }));
        let svc = GpuService::start_cpu_only();
        for task in &tasks {
            execute_task(
                task,
                TextureConversionParamsPayload::default(),
                &svc,
                false,
                0,
                None,
            )
            .unwrap();
        }
        svc.shutdown();

        assert_outputs_byte_equal(&legacy_req, &ours_req);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn per_texel_divergent_source_mips_demote_to_residue_bytes() {
        // Source whose deep mips are NOT box-downscales of mip0 (authored
        // chains: facecustomization tint _l, some decals). Preserving them
        // would diverge from legacy beyond the parity gate — the executor
        // must fall back to the legacy path (byte-identical output).
        let tmp = std::env::temp_dir().join("exec_divergent_mips");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let mip0: Vec<u8> = (0..32usize * 32 * 4)
            .map(|i| (i * 31 % 256) as u8)
            .collect();
        let mut chain = vec![(32u32, 32u32, mip0)];
        let (mut w, mut h) = (16u32, 16u32);
        loop {
            chain.push((w, h, vec![255u8; (w as usize) * (h as usize) * 4]));
            if w == 1 && h == 1 {
                break;
            }
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        let bytes =
            directxtex_native::encode_dds_from_rgba8_chain(&chain, "BC3_UNORM", false, None)
                .unwrap();
        std::fs::write(tmp.join("weird_d.dds"), bytes).unwrap();
        let names = ["weird_d.dds"];

        let legacy_req = fo76_request(&tmp, &names, &tmp.join("legacy"));
        convert_texture_set_paths(legacy_req.clone()).unwrap();

        let ours_req = fo76_request(&tmp, &names, &tmp.join("new"));
        let tasks = crate::texture_engine::triage_request(&ours_req, &HashMap::new(), false);
        assert!(
            matches!(
                tasks[0],
                TextureTask::Single {
                    class: TriageClass::PerTexel,
                    ..
                }
            ),
            "header looks fine — triage must still classify PerTexel"
        );
        let svc = GpuService::start_cpu_only();
        let mut demoted_count = 0u32;
        for task in &tasks {
            let (_, _, demoted) = execute_task(
                task,
                TextureConversionParamsPayload::default(),
                &svc,
                false,
                0,
                None,
            )
            .unwrap();
            if demoted {
                demoted_count += 1;
            }
        }
        svc.shutdown();
        assert_eq!(
            demoted_count, 1,
            "divergent-mip PerTexel must demote to residue"
        );
        assert_outputs_byte_equal(&legacy_req, &ours_req);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
