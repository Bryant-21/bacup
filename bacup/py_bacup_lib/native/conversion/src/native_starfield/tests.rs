use super::*;
use crate::phase::{
    Phase, materials_v2::ConvertMaterialsV2Phase, nifs::ConvertNifsV2Phase,
    textures_v2::ConvertTexturesV2Phase,
};
use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};
use std::sync::{Arc, atomic::AtomicBool};

fn put(root: &Path, relative: &str, bytes: &[u8]) -> PathBuf {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    path
}

fn run_phase(
    source: &Path,
    output: &Path,
    native: bool,
    params: Value,
    phase: &dyn Phase,
    sink: Option<Arc<SinkSet>>,
) -> Result<PhaseReport, RunError> {
    let id = create_run(RunParams {
        source: Game::Starfield,
        target: Game::Fo4,
        source_handle_id: 9999,
        target_handle_id: 9998,
        master_handle_ids: vec![],
        config: RunConfig {
            output_plugin_name: "Output.esp".into(),
            native_assets: native,
            source_extracted_dir: Some(source.into()),
            ..Default::default()
        },
    })
    .unwrap();
    let result = with_run(id, |run| {
        run.output_sink = sink;
        let cancel = AtomicBool::new(false);
        let mut ctx = PhaseCtx {
            run,
            mod_path: output,
            source_extracted_dir: source,
            target_extracted_dir: None,
            target_data_dir: None,
            params: &params,
            cancel: &cancel,
        };
        phase
            .run(&mut ctx)
            .map_err(|error| RunError::InvalidConfig(error.to_string()))
    });
    drop_run(id).unwrap();
    result
}

#[test]
fn inherited_layers_and_every_texture_channel_reach_loose_and_archive_sinks() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let output = temp.path().join("mod");
    let material = put(&source, "Materials/Weapons/test.mat", br#"{"Objects":[{"Parent":"Data\\Materials\\Layered\\base.mat"}],"Summary":{"Layers":[{"Parent":"materials/layers/steel.mat"}]}}"#);
    put(&source, "Materials/Layered/base.mat", br#"{"Objects":[{"Parent":"materials/layered/root/material.mat"}],"Summary":{"Opacity":"Data/Textures/Test/opacity.dds"}}"#);
    let channels = ["color", "normal", "rough", "metal", "ao", "opacity"];
    let textures: Vec<_> = channels
        .iter()
        .map(|channel| format!("textures/test/{channel}.dds"))
        .collect();
    put(
        &source,
        "Materials/Layers/steel.mat",
        &serde_json::to_vec(&serde_json::json!({"Textures": textures})).unwrap(),
    );
    for channel in channels {
        put(
            &source,
            &format!("Textures/Test/{channel}.dds"),
            channel.as_bytes(),
        );
    }
    let sink = Arc::new(SinkSet {
        ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
        loose: LooseSink {
            enabled: true,
            mod_root: output.clone(),
        },
        terrain: TerrainSidecarSink::default(),
    });
    let params = serde_json::json!({"materials":[{"source_path":"Materials/Weapons/test.mat","resolved_path":material}]});
    let report = run_phase(
        &source,
        &output,
        true,
        params.clone(),
        &ConvertMaterialsV2Phase,
        Some(sink.clone()),
    )
    .unwrap();
    assert_eq!(report.assets_written, 9);
    assert_eq!(sink.ba2.as_ref().unwrap().entry_count(), 9);
    let written: Value = serde_json::from_slice(
        &std::fs::read(output.join("data/materials/Starfield/weapons/test.mat")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        written["Objects"][0]["Parent"],
        "materials/Starfield/layered/base.mat"
    );
    for channel in channels {
        assert_eq!(
            std::fs::read(output.join(format!("data/textures/Starfield/test/{channel}.dds")))
                .unwrap(),
            channel.as_bytes()
        );
    }
    assert!(
        !output
            .join("data/materials/Starfield/weapons/test.bgsm")
            .exists()
    );
    put(
        &output,
        "data/textures/Starfield/test/rough.dds",
        b"stale converted output",
    );
    run_phase(
        &source,
        &output,
        true,
        params,
        &ConvertMaterialsV2Phase,
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(output.join("data/textures/Starfield/test/rough.dds")).unwrap(),
        b"rough"
    );
}

#[test]
fn standalone_textures_are_byte_exact_even_when_skip_existing_is_set() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let output = temp.path().join("mod");
    let texture = put(&source, "textures/test/rough.dds", b"authored DDS bytes");
    put(&output, "data/textures/test/rough.dds", b"converted bytes");
    let report = run_phase(
        &source,
        &output,
        true,
        serde_json::json!({"textures":[texture],"skip_existing":true}),
        &ConvertTexturesV2Phase,
        None,
    )
    .unwrap();
    assert_eq!(report.assets_written, 1);
    assert_eq!(
        std::fs::read(output.join("data/textures/test/rough.dds")).unwrap(),
        b"authored DDS bytes"
    );
    assert!(!output.join("data/textures/test/rough_s.dds").exists());
}

#[test]
fn missing_parents_fail_and_paths_cannot_escape_data() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let mat = put(
        &source,
        "materials/test.mat",
        br#"{"Parent":"materials/missing.mat"}"#,
    );
    let params =
        serde_json::json!({"materials":[{"source_path":"materials/test.mat","resolved_path":mat}]});
    let error = run_phase(
        &source,
        &temp.path().join("out"),
        true,
        params,
        &ConvertMaterialsV2Phase,
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("materials/missing.mat"));
    for path in [
        "../test.mat",
        "materials/../../test.mat",
        "C:/test.mat",
        "/test.mat",
    ] {
        assert!(resource_path(path, "materials").is_err());
    }
}

#[test]
#[ignore = "requires extracted Starfield weapon assets"]
fn real_weapon_nifs_switch_material_modes_and_ship_native_dependencies() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join("extracted/starfield")
        .canonicalize()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    for model in [
        "shotty/shotty.nif",
        "russianassaultrifle/russianassaultrifle_receiver.nif",
        "tombstone/Tombstonerifle.nif",
        "novablast/novablast.nif",
    ] {
        let relative = format!("meshes/weapons/{model}");
        let params = serde_json::json!({"source_game":"starfield","target_game":"fo4","nif_paths":[{"source_path":relative,"resolved_path":source.join(&relative)}],"skip_existing":true});
        for native in [true, false, true] {
            if native {
                run_phase(&source, temp.path(), native, params.clone(), &ConvertMaterialsV2Phase, None).unwrap();
            }
            let report = run_phase(
                &source,
                temp.path(),
                native,
                params.clone(),
                &ConvertNifsV2Phase,
                None,
            )
            .unwrap();
            assert_eq!(report.items_failed, 0, "{model}");
            let converted = NifFile::load(temp.path().join("data").join(&relative)).unwrap();
            let materials = converted.referenced_asset_paths().materials;
            assert!(!materials.is_empty(), "{model}");
            for material in materials {
                assert!(
                    material.ends_with(if native { ".mat" } else { ".bgsm" }),
                    "{material}"
                );
                if native {
                    assert!(
                        material.to_ascii_lowercase().contains("starfield"),
                        "{material}"
                    );
                    let path = resource_path(&material, "materials").unwrap();
                    assert!(temp.path().join("data").join(path).is_file(), "{material}");
                }
            }
        }
    }
}
