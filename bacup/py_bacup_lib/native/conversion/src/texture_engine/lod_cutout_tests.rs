use super::*;
use materials_native::texture_convert::{TexturePathInput, TexturePathOutput};

fn coverage(pixels: &[u8]) -> f32 {
    pixels.chunks_exact(4).filter(|px| px[3] >= 128).count() as f32 / (pixels.len() / 4) as f32
}

/// FO76's shipped LOD cutouts: 1-bit BC1 whose box-filtered mips fill in solid.
fn write_cutout(path: &std::path::Path) -> f32 {
    let size = 64u32;
    let mut base = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            if x % 5 == 0 || (x + 2 * y) % 11 == 0 {
                let i = ((y * size + x) * 4) as usize;
                base[i..i + 4].copy_from_slice(&[90, 60, 30, 255]);
            }
        }
    }
    let chain = directxtex_native::rgba8_box_mip_chain(size, size, &base).unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        directxtex_native::encode_dds_from_rgba8_chain(&chain, "BC1_UNORM_SRGB", false, None).unwrap(),
    )
    .unwrap();
    coverage(&base)
}

fn convert(source_rel: &str) -> (f32, Vec<(u32, u32, Vec<u8>)>) {
    let temp = tempfile::tempdir().unwrap();
    let service = gpu_service::GpuService::start_cpu_only();
    let input = TexturePathInput {
        role: "diffuse".into(),
        path: temp.path().join("in").join(source_rel),
    };
    let top = write_cutout(&input.path);
    let output = TexturePathOutput {
        role: "diffuse".into(),
        path: temp.path().join("out").join(source_rel),
        format: "BC7_UNORM".into(),
    };
    let task = triage::TextureTask::Single {
        input,
        output: output.clone(),
        target_format: "BC1_UNORM_SRGB".into(),
        class: TriageClass::PassThrough,
        normal_kernel: false,
    };
    executors::execute_task_with_landscape_mip_flooding(
        &task,
        Default::default(),
        &service,
        false,
        u32::MAX,
        false,
        None,
    )
    .unwrap();
    (top, directxtex_native::read_dds_mips_rgba8(&output.path).unwrap().mips)
}

#[test]
fn lod_cutout_array_textures_keep_their_coverage_down_the_mips() {
    for group in ["translucent", "alphatested"] {
        let (top, mips) = convert(&format!(
            "textures/lod/generated/texturearrays/{group}/{group}_63_d.dds"
        ));
        for (w, h, pixels) in mips.iter().filter(|(w, _, _)| (4..64).contains(w)) {
            let got = coverage(pixels);
            assert!((got - top).abs() <= 0.08, "{group} {w}x{h}: {got} vs {top}");
        }
    }
}

#[test]
fn other_cutouts_keep_their_shipped_mips() {
    let (top, mips) = convert("textures/landscape/trees/branch_d.dds");
    let (_, _, eight) = mips.iter().find(|(w, _, _)| *w == 8).unwrap();
    assert!((coverage(eight) - top).abs() > 0.15);
}
