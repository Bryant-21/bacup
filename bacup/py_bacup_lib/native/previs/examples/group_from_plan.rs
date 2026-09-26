//! Dev harness: build one group NIF from a research group-plan JSON.
use previs_native::assets::AssetResolver;
use previs_native::f32ops::Transform;
use previs_native::group::{self, GroupRequest, GroupSource, SourceModel};
use previs_native::psg;
use previs_native::swap::SwapLayers;

fn transform(value: &serde_json::Value) -> Transform {
    if let Some(e) = value.get("Euler") {
        let t = &value["Translation"];
        let g = |k: &str| e[k].as_f64().unwrap() as f32;
        return Transform {
            rotation: previs_native::f32ops::euler_rotation([g("x"), g("y"), g("z")]),
            translation: [t["x"].as_f64().unwrap() as f32, t["y"].as_f64().unwrap() as f32, t["z"].as_f64().unwrap() as f32],
            scale: value["Scale"].as_f64().unwrap() as f32,
        };
    }
    let r = &value["Rotation"];
    let g = |k: &str| r[k].as_f64().unwrap() as f32;
    let t = &value["Translation"];
    Transform {
        rotation: [[g("m11"), g("m21"), g("m31")], [g("m12"), g("m22"), g("m32")], [g("m13"), g("m23"), g("m33")]],
        translation: [t["x"].as_f64().unwrap() as f32, t["y"].as_f64().unwrap() as f32, t["z"].as_f64().unwrap() as f32],
        scale: value["Scale"].as_f64().unwrap() as f32,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (nif_path, plan_path, resource, output) = (&args[1], &args[2], &args[3], &args[4]);
    let root = std::path::PathBuf::from(&args[5]);
    let assets = AssetResolver::new(vec![root], &[]).unwrap();
    let bytes = std::fs::read(nif_path).unwrap();
    let model = match SourceModel::load(nif_path, &bytes, &assets, SwapLayers::default(), None) {
        Ok(model) => model,
        Err(error) => {
            eprintln!("REJECT {error}");
            std::process::exit(3);
        }
    };
    let Ok(plan_text) = std::fs::read_to_string(plan_path) else {
        eprintln!("ACCEPT");
        return;
    };
    let plan: serde_json::Value = serde_json::from_str(&plan_text).unwrap();
    let blocks: Vec<usize> = model.shapes.iter().map(|s| s.block).collect();
    eprintln!("retained {:?} plan {:?}", blocks, plan["source_shape_blocks"]);
    let repeat = plan.get("repeat_instances").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let mut instances: Vec<Transform> = plan["instances"].as_array().unwrap().iter().map(transform).collect();
    instances = instances.iter().cycle().take(instances.len() * repeat).cloned().collect();
    let buffers: Vec<&[u8]> = model.shapes.iter().map(|s| s.geometry.as_slice()).collect();
    let offsets = psg::deduplicated_offsets(&buffers);
    let export: Vec<u8> = plan.get("export_info_hex").and_then(|v| v.as_str()).map(|h| {
        (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect()
    }).unwrap_or(group::EXPORT_INFO_STAT.to_vec());
    let bytes = group::build_group(&GroupRequest {
        root_name: plan["root_name"].as_str().unwrap().to_string(),
        sources: vec![GroupSource {
            model: &model,
            instances,
            base_record_flags: 0,
            leaf: None,
            editor_id: "",
            data_offsets: offsets,
            merges_collision: model.collision.as_ref().is_some_and(|c| c.merged.is_some()),
        }],
        psg_file_name: resource.clone(),
        export_info: export.try_into().unwrap(),
    }).unwrap();
    std::fs::write(output, bytes).unwrap();
}
