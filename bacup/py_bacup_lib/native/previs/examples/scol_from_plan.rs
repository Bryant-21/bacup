//! Dev harness: build a multi-source (SCOL) group NIF and PSG from a research plan.
use previs_native::assets::AssetResolver;
use previs_native::f32ops::Transform;
use previs_native::group::{self, GroupRequest, GroupSource, SourceModel};
use previs_native::psg::{self, PsgGeometry};
use previs_native::swap::SwapLayers;

fn transform(value: &serde_json::Value) -> Transform {
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
    let (plan_path, resource, output, psg_output) = (&args[1], &args[2], &args[3], &args[4]);
    let assets = AssetResolver::new(vec![std::path::PathBuf::from(&args[5])], &[]).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(plan_path).unwrap()).unwrap();
    let specs = plan["sources"].as_array().unwrap();
    let models: Vec<SourceModel> = specs
        .iter()
        .map(|s| {
            let path = s["source_nif"].as_str().unwrap();
            SourceModel::load(path, &std::fs::read(path).unwrap(), &assets, SwapLayers::default(), None).unwrap()
        })
        .collect();
    let entries: Vec<PsgGeometry> = models
        .iter()
        .flat_map(|m| m.shapes.iter())
        .map(|s| PsgGeometry { vertex_desc: s.vertex_desc, vertex_count: s.vertex_count, triangle_count: s.triangle_count, data: &s.geometry })
        .collect();
    let buffers: Vec<&[u8]> = entries.iter().map(|e| e.data).collect();
    let mut offsets = psg::deduplicated_offsets(&buffers).into_iter();
    let parent = plan.get("parent_transform").map(transform);
    let sources: Vec<GroupSource> = models
        .iter()
        .zip(specs)
        .map(|(model, spec)| {
            let blocks: Vec<u64> = model.shapes.iter().map(|s| s.block as u64).collect();
            eprintln!("{} retained {:?} plan {}", model.path, blocks, spec["source_shape_blocks"]);
            let leaf = spec["leaf_amplitude"].as_f64().zip(spec["leaf_frequency"].as_f64()).map(|(a, f)| (a as f32, f as f32));
            GroupSource {
                model,
                instances: spec["instances"].as_array().unwrap().iter().map(transform).map(|i| match &parent { Some(p) => p.compose(&i), None => i }).collect(),
                base_record_flags: spec["base_record_flags"].as_u64().unwrap_or(0) as u32,
                leaf,
                editor_id: "",
                data_offsets: offsets.by_ref().take(model.shapes.len()).collect(),
                merges_collision: model.collision.as_ref().is_some_and(|c| c.merged.is_some()),
            }
        })
        .collect();
    let export: Vec<u8> = (0..20).step_by(2).map(|i| u8::from_str_radix(&plan["export_info_hex"].as_str().unwrap()[i..i + 2], 16).unwrap()).collect();
    let bytes = group::build_group(&GroupRequest {
        root_name: plan["root_name"].as_str().unwrap().to_string(),
        sources,
        psg_file_name: resource.clone(),
        export_info: export.try_into().unwrap(),
    })
    .unwrap();
    std::fs::write(output, bytes).unwrap();
    std::fs::write(psg_output, psg::build(&entries)).unwrap();
}
