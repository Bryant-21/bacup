use super::*;
use materials_native::texture_convert::{
    TexturePathInput, TexturePathOutput, TextureSetPathRequest, convert_texture_set_paths,
};

#[test]
fn demotion_reads_once_and_preserves_legacy_output_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let service = gpu_service::GpuService::start_cpu_only();
    for (index, (source_format, role, output_role, name)) in [
        ("BC3_UNORM", "diffuse", "diffuse", "colour_d.dds"),
        ("BC3_UNORM_SRGB", "diffuse", "diffuse", "lightgobo_d.dds"),
        ("BC7_UNORM_SRGB", "normal", "normal", "normal_n.dds"),
        ("BC5_UNORM", "normal", "normal", "normal_n.dds"),
        ("R8G8B8A8_UNORM", "lighting", "specular", "lighting_l.dds"),
        ("R8G8B8A8_UNORM_SRGB", "glow", "glow", "glow_g.dds"),
    ]
    .into_iter()
    .enumerate()
    {
        let base: Vec<u8> = (0..32 * 32 * 4).map(|i| (i * 31 % 256) as u8).collect();
        let mut chain = directxtex_native::rgba8_box_mip_chain(32, 32, &base).unwrap();
        for (_, _, pixels) in chain.iter_mut().skip(1) {
            pixels.fill(255);
        }
        let input = TexturePathInput {
            role: role.into(),
            path: temp.path().join(format!("{index}_{name}")),
        };
        std::fs::write(
            &input.path,
            directxtex_native::encode_dds_from_rgba8_chain(&chain, source_format, false, None)
                .unwrap(),
        )
        .unwrap();
        let legacy = TexturePathOutput {
            role: output_role.into(),
            path: temp.path().join(format!("legacy_{index}/{name}")),
            format: "BC7_UNORM".into(),
        };
        let output = TexturePathOutput {
            path: temp.path().join(format!("new_{index}/{name}")),
            ..legacy.clone()
        };
        convert_texture_set_paths(TextureSetPathRequest {
            source_game: "fo76".into(),
            target_game: "fo4".into(),
            inputs: vec![input.clone()],
            outputs: vec![legacy.clone()],
            params: Default::default(),
            use_gpu: false,
            gpu_min_pixels: u32::MAX,
            parallel_compression: false,
        })
        .unwrap();
        let task = triage::TextureTask::Single {
            input,
            output: output.clone(),
            target_format: "BC7_UNORM".into(),
            class: TriageClass::PerTexel,
            normal_kernel: role == "normal",
        };
        let (result, timings) = directxtex_native::profiling::capture(|| {
            executors::execute_task(&task, Default::default(), &service, false, u32::MAX, None)
        });
        assert_eq!(result.unwrap(), (1, 0, true));
        assert_eq!(timings.read_calls, 1);
        assert_eq!(
            std::fs::read(&output.path).unwrap(),
            std::fs::read(&legacy.path).unwrap(),
            "{source_format} / {role}"
        );
    }
}

#[test]
#[ignore = "Paired GPU submission benchmark, including exact DDS comparison"]
fn benchmark_gpu_submission_ownership() {
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    use std::time::Instant;
    let service = gpu_service::GpuService::start(8);
    let copied_bytes = AtomicU64::new(0);
    let copy_ns = AtomicU64::new(0);
    let mut old_seconds = 0.0;
    let mut new_seconds = 0.0;
    for side in [256, 512, 1024] {
        let pixels = (0..side * side * 4)
            .map(|i| (i * 31 % 256) as u8)
            .collect::<Vec<_>>();
        let source = directxtex_native::rgba8_box_mip_chain(side, side, &pixels).unwrap();
        for iteration in 0..4 {
            let mut outputs = Vec::new();
            for owned in if iteration % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let chain = source.clone();
                let started = Instant::now();
                let bytes = if owned {
                    let encoder = |images: Arc<directxtex_native::Rgba8MipChain>, srgb| {
                        service.encode_bc7_shared(images, srgb, 512 * 512)
                    };
                    directxtex_native::encode_dds_from_owned_rgba8_chain(
                        chain,
                        "BC7_UNORM",
                        false,
                        Some(&encoder),
                    )
                    .unwrap()
                } else {
                    let encoder = |images: &[(u32, u32, &[u8])], srgb| {
                        let started = Instant::now();
                        let chain = images
                            .iter()
                            .map(|(w, h, pixels)| (*w, *h, pixels.to_vec()))
                            .collect();
                        copy_ns.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
                        copied_bytes.fetch_add(
                            images
                                .iter()
                                .map(|(_, _, pixels)| pixels.len() as u64)
                                .sum::<u64>(),
                            Ordering::Relaxed,
                        );
                        service.encode_bc7(chain, srgb, 512 * 512)
                    };
                    directxtex_native::encode_dds_from_rgba8_chain(
                        &chain,
                        "BC7_UNORM",
                        false,
                        Some(&encoder),
                    )
                    .unwrap()
                };
                let seconds = started.elapsed().as_secs_f64();
                if owned {
                    new_seconds += seconds;
                } else {
                    old_seconds += seconds;
                }
                outputs.push(bytes);
            }
            assert_eq!(outputs[0], outputs[1]);
        }
    }
    let stats = service.stats();
    let result = serde_json::json!({"legacy_seconds":old_seconds,"shared_seconds":new_seconds,
        "removed_copy_bytes":copied_bytes.load(Ordering::Relaxed), "legacy_copy_ms":copy_ns.load(Ordering::Relaxed) as f64 / 1e6,
        "gpu_submissions":stats.gpu_submissions,"cpu_encodes":stats.cpu_encodes,"gpu_failures":stats.gpu_failures});
    eprintln!("{result}");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap()
        .join("tmp/texture_optimization_20260909");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("gpu_benchmark.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
}
