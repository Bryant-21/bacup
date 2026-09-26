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
