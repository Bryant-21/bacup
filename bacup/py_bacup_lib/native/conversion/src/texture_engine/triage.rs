//! Probe-driven triage. Inputs come from the legacy `build_request` so file
//! selection, role mapping, output naming, and format choice are exactly the
//! legacy ones. Triage only decides HOW each output gets produced. Any doubt →
//! SingleResidue (the unmodified legacy converter on a one-pair sub-request).

use std::collections::HashMap;

use directxtex_native::{full_mip_count, read_dds_probe};
use materials_native::texture_convert::{
    TexturePathInput, TexturePathOutput, TextureSetPathRequest, mapped_fo76_output_role,
    output_format_for_path,
};

use super::TriageClass;

#[derive(Debug)]
pub enum TextureTask {
    /// PassThrough (byte copy) or PerTexel (u8 kernel on existing mips).
    Single {
        input: TexturePathInput,
        output: TexturePathOutput,
        /// Final on-disk format string (== source format for PassThrough).
        target_format: String,
        class: TriageClass,
        /// true → apply kernel_normal_zero_b (PerTexel normals only).
        normal_kernel: bool,
    },
    /// Unclassifiable single pair → legacy converter sub-request
    /// (class BundleRecompile, residue flavor).
    SingleResidue {
        input: TexturePathInput,
        output: TexturePathOutput,
    },
    /// d+r+l bundle → diffuse+specgloss(+glow) via legacy math.
    Bundle {
        diffuse: TexturePathInput,
        reflectivity: TexturePathInput,
        lighting: TexturePathInput,
        out_diffuse: Option<TexturePathOutput>,
        out_specular: Option<TexturePathOutput>,
        out_glow: Option<TexturePathOutput>,
        force_diffuse_alpha_opaque: bool,
    },
    /// r+l (no d) → specgloss via legacy math.
    SpecGloss {
        reflectivity: TexturePathInput,
        lighting: TexturePathInput,
        out_specular: TexturePathOutput,
    },
    /// FNV/FO3/Skyrim normal (+ optional env mask) → FO4 normal + spec/gloss.
    LegacySpecGloss {
        normal: TexturePathInput,
        envmask: Option<TexturePathInput>,
        out_normal: Option<TexturePathOutput>,
        out_specular: TexturePathOutput,
        /// Per-game vanilla `Glossiness` default, normalized to FO4's 0..1
        /// smoothness domain. Captured here because triage has the source game
        /// and the executor does not.
        gloss_baseline: f32,
    },
    /// Source `_e` environment map → FO4 six-face cubemap.
    CubemapNormalize {
        input: TexturePathInput,
        output: TexturePathOutput,
    },
}

impl TextureTask {
    pub fn class(&self) -> TriageClass {
        match self {
            TextureTask::Single { class, .. } => *class,
            TextureTask::SingleResidue { .. }
            | TextureTask::Bundle { .. }
            | TextureTask::SpecGloss { .. }
            | TextureTask::LegacySpecGloss { .. }
            | TextureTask::CubemapNormalize { .. } => TriageClass::BundleRecompile,
        }
    }

    /// Every output path this task will write (driver uses this for
    /// skip_existing / base-owned checks alignment with the legacy request).
    pub fn output_paths(&self) -> Vec<&std::path::Path> {
        match self {
            TextureTask::Single { output, .. } | TextureTask::SingleResidue { output, .. } => {
                vec![output.path.as_path()]
            }
            TextureTask::Bundle {
                out_diffuse,
                out_specular,
                out_glow,
                ..
            } => [out_diffuse, out_specular, out_glow]
                .into_iter()
                .flatten()
                .map(|o| o.path.as_path())
                .collect(),
            TextureTask::SpecGloss { out_specular, .. } => vec![out_specular.path.as_path()],
            TextureTask::LegacySpecGloss {
                out_normal,
                out_specular,
                ..
            } => out_normal
                .iter()
                .map(|o| o.path.as_path())
                .chain(std::iter::once(out_specular.path.as_path()))
                .collect(),
            TextureTask::CubemapNormalize { output, .. } => vec![output.path.as_path()],
        }
    }
}

fn is_gamebryo_source(source_game: &str) -> bool {
    matches!(
        source_game.to_ascii_lowercase().as_str(),
        "fnv" | "fo3" | "skyrim" | "skyrimse"
    )
}

/// FO4 `_s.G` baseline from each source game's vanilla `Glossiness` default.
/// Skyrim: `BSLightingShaderProperty.Glossiness` 80.0 / 100 (nif.xml:7773).
/// FNV/FO3: `NiMaterialProperty.Glossiness` 10.0 / 100 (nif.xml:4906).
fn legacy_gloss_baseline(source_game: &str) -> f32 {
    match source_game.to_ascii_lowercase().as_str() {
        "skyrim" | "skyrimse" => 0.8,
        _ => 0.1,
    }
}

/// Role → output-role for Gamebryo sources. `envmask` is deliberately absent:
/// it is consumed into `_s.R` by LegacySpecGloss and has no FO4 slot.
fn mapped_legacy_output_role(role: &str) -> Option<&'static str> {
    match role {
        "normal" => Some("normal"),
        "diffuse" => Some("diffuse"),
        "glow" => Some("glow"),
        "subsurface" => Some("subsurface"),
        "cubemap" => Some("cubemap"),
        _ => None,
    }
}

fn find_input<'a>(request: &'a TextureSetPathRequest, role: &str) -> Option<&'a TexturePathInput> {
    request.inputs.iter().find(|i| i.role == role)
}

fn find_output<'a>(
    request: &'a TextureSetPathRequest,
    role: &str,
) -> Option<&'a TexturePathOutput> {
    request.outputs.iter().find(|o| o.role == role)
}

fn pbr_carry_output(
    input: &TexturePathInput,
    output_dir: &std::path::Path,
) -> Option<TexturePathOutput> {
    Some(TexturePathOutput {
        role: input.role.clone(),
        path: output_dir.join(input.path.file_name()?),
        format: String::new(),
    })
}

/// Formats encode_dds_from_rgba8_chain can produce (mirror its match arms).
fn chain_encodable(format: &str) -> bool {
    matches!(
        format,
        "R8G8B8A8_UNORM"
            | "R8G8B8A8_UNORM_SRGB"
            | "BC1_UNORM"
            | "BC1_UNORM_SRGB"
            | "BC2_UNORM"
            | "BC2_UNORM_SRGB"
            | "BC3_UNORM"
            | "BC3_UNORM_SRGB"
            | "BC4_UNORM"
            | "BC5_UNORM"
            | "BC7_UNORM"
            | "BC7_UNORM_SRGB"
    )
}

/// Probe dxgi → the same format-name strings output_format_for_source emits.
fn probe_format_name(dxgi: u32) -> Option<&'static str> {
    Some(match dxgi {
        71 => "BC1_UNORM",
        72 => "BC1_UNORM_SRGB",
        74 => "BC2_UNORM",
        77 => "BC3_UNORM",
        78 => "BC3_UNORM_SRGB",
        80 => "BC4_UNORM",
        83 => "BC5_UNORM",
        98 => "BC7_UNORM",
        99 => "BC7_UNORM_SRGB",
        28 => "R8G8B8A8_UNORM",
        29 => "R8G8B8A8_UNORM_SRGB",
        61 => "R8_UNORM",
        49 => "R8G8_UNORM",
        _ => return None,
    })
}

fn classify_single(
    input: &TexturePathInput,
    output: &TexturePathOutput,
    format_overrides: &HashMap<String, String>,
) -> TextureTask {
    let residue = || TextureTask::SingleResidue {
        input: input.clone(),
        output: output.clone(),
    };

    // Per-file overrides keep exact legacy behavior (rare; not worth proving).
    let filename = input
        .path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if format_overrides.contains_key(filename) {
        return residue();
    }

    let Ok(probe) = read_dds_probe(&input.path) else {
        return residue();
    };
    if probe.is_cubemap || probe.array_size > 1 || probe.depth > 1 || probe.dxgi_format == 0 {
        return residue();
    }
    let Some(source_format) = probe_format_name(probe.dxgi_format) else {
        return residue();
    };
    // Same format decision the legacy writer makes (sRGB-preserving — never
    // hardcode): specular outputs keep the request fallback (BC5); everything
    // else maps from the source format.
    let target_format = output_format_for_path(
        &output.role,
        probe.dxgi_format,
        &output.format,
        &output.path,
    );

    if probe.mip_levels != full_mip_count(probe.width, probe.height) {
        // Mip invariant: outputs must carry full chains; only the legacy
        // recompile path regenerates one.
        return residue();
    }

    // Identity unless the normal kernel applies (zero-B). BC5 sources decode
    // with B already 0 (decode_bc5_unorm_image) — the kernel is a no-op there.
    let is_normal = input.role == "normal";
    let identity = !is_normal || probe.dxgi_format == 83;

    if identity && target_format == source_format {
        return TextureTask::Single {
            input: input.clone(),
            output: output.clone(),
            target_format,
            class: TriageClass::PassThrough,
            normal_kernel: false,
        };
    }
    if chain_encodable(&target_format) {
        return TextureTask::Single {
            input: input.clone(),
            output: output.clone(),
            target_format,
            class: TriageClass::PerTexel,
            normal_kernel: is_normal && probe.dxgi_format != 83,
        };
    }
    residue()
}

/// Classify one legacy request into engine tasks. Mirrors the dispatch order
/// of convert_fo76_to_fo4_paths: bundle → specgloss fallback → individual
/// pairs.
pub fn triage_request(
    request: &TextureSetPathRequest,
    format_overrides: &HashMap<String, String>,
    pbr_carry: bool,
) -> Vec<TextureTask> {
    triage_request_impl(request, format_overrides, pbr_carry, false)
}

pub fn triage_terrain_request(
    request: &TextureSetPathRequest,
    format_overrides: &HashMap<String, String>,
) -> Vec<TextureTask> {
    triage_request_impl(request, format_overrides, false, true)
}

fn triage_request_impl(
    request: &TextureSetPathRequest,
    format_overrides: &HashMap<String, String>,
    pbr_carry: bool,
    force_diffuse_alpha_opaque: bool,
) -> Vec<TextureTask> {
    let mut tasks = Vec::new();
    let diffuse = find_input(request, "diffuse");
    let reflectivity = find_input(request, "reflectivity");
    let lighting = find_input(request, "lighting");

    let mut bundled_roles: &[&str] = &[];

    // FNV/FO3/Skyrim → FO4 dispatch. Returns early so the FO76 chain below is
    // untouched for these sources.
    if is_gamebryo_source(&request.source_game)
        && let Some(normal) = find_input(request, "normal")
        && let Some(out_specular) = find_output(request, "specular")
    {
        tasks.push(TextureTask::LegacySpecGloss {
            normal: normal.clone(),
            envmask: find_input(request, "envmask").cloned(),
            out_normal: find_output(request, "normal").cloned(),
            out_specular: out_specular.clone(),
            gloss_baseline: legacy_gloss_baseline(&request.source_game),
        });
        for input in &request.inputs {
            if matches!(input.role.as_str(), "normal" | "envmask") {
                continue;
            }
            let Some(out_role) = mapped_legacy_output_role(&input.role) else {
                continue;
            };
            let Some(output) = find_output(request, out_role) else {
                continue;
            };
            if input.role == "cubemap" {
                tasks.push(TextureTask::CubemapNormalize {
                    input: input.clone(),
                    output: output.clone(),
                });
                continue;
            }
            tasks.push(classify_single(input, output, format_overrides));
        }
        return tasks;
    }

    if let (Some(d), Some(r), Some(l)) = (diffuse, reflectivity, lighting) {
        let out_diffuse = find_output(request, "diffuse").cloned();
        tasks.push(TextureTask::Bundle {
            diffuse: d.clone(),
            reflectivity: r.clone(),
            lighting: l.clone(),
            out_diffuse: if pbr_carry { None } else { out_diffuse.clone() },
            out_specular: find_output(request, "specular").cloned(),
            out_glow: find_output(request, "glow").cloned(),
            force_diffuse_alpha_opaque,
        });
        if pbr_carry
            && let Some(diffuse_output) = out_diffuse
            && let Some(output_dir) = diffuse_output
                .path
                .parent()
                .map(std::path::Path::to_path_buf)
        {
            let carry_outputs = [
                Some(diffuse_output),
                pbr_carry_output(r, &output_dir),
                pbr_carry_output(l, &output_dir),
            ];
            for (input, output) in [d, r, l]
                .into_iter()
                .zip(carry_outputs)
                .filter_map(|(input, output)| output.map(|output| (input, output)))
            {
                tasks.push(TextureTask::Single {
                    input: input.clone(),
                    output,
                    target_format: String::new(),
                    class: TriageClass::PassThrough,
                    normal_kernel: false,
                });
            }
        }
        bundled_roles = &["diffuse", "reflectivity", "lighting"];
    } else if let (Some(r), Some(l), Some(out)) =
        (reflectivity, lighting, find_output(request, "specular"))
    {
        tasks.push(TextureTask::SpecGloss {
            reflectivity: r.clone(),
            lighting: l.clone(),
            out_specular: out.clone(),
        });
        bundled_roles = &["reflectivity", "lighting"];
    }

    for input in &request.inputs {
        if bundled_roles.contains(&input.role.as_str()) {
            continue;
        }
        let Some(out_role) = mapped_fo76_output_role(&input.role) else {
            continue; // unsupported role — legacy skips it too
        };
        let Some(output) = find_output(request, out_role) else {
            continue; // no matching output — legacy skips it too
        };
        tasks.push(classify_single(input, output, format_overrides));
    }
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::textures::{build_request, game_texture_suffixes, group_textures};
    use materials_native::texture_convert::TextureConversionParamsPayload;
    use std::collections::HashMap;
    use std::path::Path;

    fn write_tex(dir: &Path, name: &str, w: u32, h: u32, format: &str, mips: bool) {
        let rgba: Vec<u8> = (0..(w as usize) * (h as usize) * 4)
            .map(|i| (i % 256) as u8)
            .collect();
        directxtex_native::write_dds_rgba_image(&dir.join(name), w, h, &rgba, format, mips)
            .unwrap();
    }

    fn tasks_for(dir: &Path, names: &[&str]) -> Vec<TextureTask> {
        let paths: Vec<String> = names
            .iter()
            .map(|n| dir.join(n).to_string_lossy().to_string())
            .collect();
        let groups = group_textures(&paths, dir, game_texture_suffixes("fo76"), "fo76");
        let mut tasks = Vec::new();
        for group in groups {
            let request = build_request(
                &group,
                &dir.join("out"),
                "fo76",
                "fo4",
                game_texture_suffixes("fo76"),
                game_texture_suffixes("fo4"),
                &HashMap::new(),
                TextureConversionParamsPayload::default(),
                false,
                0,
            )
            .expect("request");
            tasks.extend(triage_request(&request, &HashMap::new(), false));
        }
        tasks
    }

    #[test]
    fn full_mip_bc7_diffuse_is_pass_through() {
        let tmp = std::env::temp_dir().join("triage_pt_diffuse");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "rock_d.dds", 16, 16, "BC7_UNORM", true);
        let tasks = tasks_for(&tmp, &["rock_d.dds"]);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].class(), TriageClass::PassThrough);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn missing_mips_demote_pass_through_to_recompile() {
        // Mip invariant: converted textures MUST carry mip chains; a mipless
        // source can't be byte-copied — it must take the legacy path (which
        // regenerates mips).
        let tmp = std::env::temp_dir().join("triage_mipless");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "rock_d.dds", 16, 16, "BC7_UNORM", false);
        let tasks = tasks_for(&tmp, &["rock_d.dds"]);
        assert_eq!(tasks[0].class(), TriageClass::BundleRecompile);
        assert!(matches!(tasks[0], TextureTask::SingleResidue { .. }));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn bc5_normal_is_pass_through_but_bc7_normal_is_per_texel() {
        let tmp = std::env::temp_dir().join("triage_normals");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "a_n.dds", 16, 16, "BC5_UNORM", true);
        write_tex(&tmp, "b_n.dds", 16, 16, "BC7_UNORM", true);
        let a = tasks_for(&tmp, &["a_n.dds"]);
        let b = tasks_for(&tmp, &["b_n.dds"]);
        assert_eq!(a[0].class(), TriageClass::PassThrough);
        assert_eq!(b[0].class(), TriageClass::PerTexel);
        match &b[0] {
            TextureTask::Single { normal_kernel, .. } => assert!(*normal_kernel),
            other => panic!("expected Single, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn bc3_diffuse_needs_bc7_reencode_per_texel() {
        // output_format_for_source maps BC3 (77) -> BC7_UNORM: format change,
        // identity math -> PerTexel.
        let tmp = std::env::temp_dir().join("triage_bc3");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "c_d.dds", 16, 16, "BC3_UNORM", true);
        let tasks = tasks_for(&tmp, &["c_d.dds"]);
        assert_eq!(tasks[0].class(), TriageClass::PerTexel);
        match &tasks[0] {
            TextureTask::Single { target_format, .. } => assert_eq!(target_format, "BC7_UNORM"),
            other => panic!("expected Single, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn bc3_effect_diffuse_stays_bc3_and_passes_through() {
        let tmp = std::env::temp_dir()
            .join("triage_effect_bc3")
            .join("Effects");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(
            &tmp,
            "SmokeNuke76PuffsTile_d.dds",
            16,
            16,
            "BC3_UNORM_SRGB",
            true,
        );
        let tasks = tasks_for(&tmp, &["SmokeNuke76PuffsTile_d.dds"]);
        assert_eq!(tasks[0].class(), TriageClass::PassThrough);
        match &tasks[0] {
            TextureTask::Single { target_format, .. } => {
                assert_eq!(target_format, "BC3_UNORM_SRGB")
            }
            other => panic!("expected Single, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn d_r_l_trio_is_bundle_and_extra_normal_stays_single() {
        let tmp = std::env::temp_dir().join("triage_bundle");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        for n in ["set_d.dds", "set_r.dds", "set_l.dds"] {
            write_tex(&tmp, n, 16, 16, "BC7_UNORM", true);
        }
        write_tex(&tmp, "set_n.dds", 16, 16, "BC5_UNORM", true);
        let tasks = tasks_for(&tmp, &["set_d.dds", "set_r.dds", "set_l.dds", "set_n.dds"]);
        let bundles: Vec<_> = tasks
            .iter()
            .filter(|t| matches!(t, TextureTask::Bundle { .. }))
            .collect();
        let singles: Vec<_> = tasks
            .iter()
            .filter(|t| !matches!(t, TextureTask::Bundle { .. }))
            .collect();
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].class(), TriageClass::BundleRecompile);
        assert_eq!(
            singles.len(),
            1,
            "the _n input must remain an independent single"
        );
        assert_eq!(singles[0].class(), TriageClass::PassThrough);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn r_l_pair_without_diffuse_is_specgloss_bundle() {
        let tmp = std::env::temp_dir().join("triage_rl");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        // group_textures adds implicit bundle siblings only when files exist on
        // disk; here only _r and _l exist so the trio check fails -> specgloss.
        write_tex(&tmp, "p_r.dds", 16, 16, "BC7_UNORM", true);
        write_tex(&tmp, "p_l.dds", 16, 16, "BC7_UNORM", true);
        let tasks = tasks_for(&tmp, &["p_r.dds", "p_l.dds"]);
        assert_eq!(tasks.len(), 1);
        assert!(matches!(tasks[0], TextureTask::SpecGloss { .. }));
        assert_eq!(tasks[0].class(), TriageClass::BundleRecompile);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn format_override_disables_pass_through() {
        let tmp = std::env::temp_dir().join("triage_override");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        write_tex(&tmp, "o_d.dds", 16, 16, "BC7_UNORM", true);
        let paths = vec![tmp.join("o_d.dds").to_string_lossy().to_string()];
        let groups = group_textures(&paths, &tmp, game_texture_suffixes("fo76"), "fo76");
        let mut overrides = HashMap::new();
        overrides.insert("o_d.dds".to_string(), "BC1_UNORM".to_string());
        let request = build_request(
            &groups[0],
            &tmp.join("out"),
            "fo76",
            "fo4",
            game_texture_suffixes("fo76"),
            game_texture_suffixes("fo4"),
            &overrides,
            TextureConversionParamsPayload::default(),
            false,
            0,
        )
        .unwrap();
        let tasks = triage_request(&request, &overrides, false);
        assert_ne!(tasks[0].class(), TriageClass::PassThrough);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
