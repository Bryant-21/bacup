//! Dev probe: one JSON line per group NIF with each combined shape's counts,
//! LOD sizes, shader fields, texture slots and alpha state (used to compare
//! generated groups with CK output field by field).
use nif_core_native::model::{NifFile, NifValue};

fn value(v: &NifValue) -> serde_json::Value {
    match v {
        NifValue::String(s) => serde_json::Value::String(s.clone()),
        NifValue::UInt(x) => serde_json::json!(x),
        NifValue::Int(x) => serde_json::json!(x),
        NifValue::Float(x) => serde_json::json!(*x as f32),
        NifValue::FloatNan(bits) => serde_json::json!(format!("nan:{bits:x}")),
        NifValue::Ref(r) => serde_json::json!(format!("ref:{r}")),
        NifValue::Struct(fields) => serde_json::Value::Object(fields.iter().map(|(k, v)| (k.clone(), value(v))).collect()),
        NifValue::Array(items) => serde_json::Value::Array(items.iter().map(value).collect()),
        other => serde_json::json!(format!("{other:?}")),
    }
}

fn reference(v: Option<&NifValue>) -> i64 {
    match v {
        Some(NifValue::Ref(r)) => *r as i64,
        _ => -1,
    }
}

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let nif = NifFile::from_bytes(&bytes, None).unwrap();
        let mut shapes = Vec::new();
        for block in &nif.blocks {
            if !matches!(block.type_name.as_str(), "BSTriShape" | "BSMeshLODTriShape" | "BSSubIndexTriShape") {
                continue;
            }
            let get = |name: &str| block.get_field(name).map(value).unwrap_or(serde_json::Value::Null);
            let shader_id = reference(block.get_field("Shader Property"));
            let alpha_id = reference(block.get_field("Alpha Property"));
            let mut combined = 0u64;
            if let Some(NifValue::Array(extras)) = block.get_field("Extra Data List") {
                for extra in extras {
                    let id = reference(Some(extra));
                    if id < 0 {
                        continue;
                    }
                    if let Some(NifValue::Array(data)) = nif.blocks[id as usize].get_field("Object Data") {
                        for item in data {
                            if let NifValue::Struct(fields) = item {
                                combined += fields.get("Num Combined").map(NifValue::as_i64).unwrap_or(0) as u64;
                            }
                        }
                    }
                }
            }
            let shader = (shader_id >= 0).then(|| {
                let shader = &nif.blocks[shader_id as usize];
                let mut fields = serde_json::Map::new();
                for (name, v) in &shader.fields {
                    if !matches!(name.as_str(), "Name" | "Texture Set" | "Controller" | "Extra Data List" | "Num Extra Data List") {
                        fields.insert(name.clone(), value(v));
                    }
                }
                let texture_set = reference(shader.get_field("Texture Set"));
                let textures = (texture_set >= 0).then(|| nif.blocks[texture_set as usize].get_field("Textures").map(value));
                serde_json::json!({"type": shader.type_name, "name": shader.get_field("Name").map(value), "fields": fields, "textures": textures})
            });
            let alpha = (alpha_id >= 0).then(|| {
                let alpha = &nif.blocks[alpha_id as usize];
                serde_json::json!({"flags": alpha.get_field("Flags").map(value), "threshold": alpha.get_field("Threshold").map(value)})
            });
            shapes.push(serde_json::json!({
                "type": block.type_name,
                "flags": get("Flags"),
                "verts": get("Num Vertices"),
                "tris": get("Num Triangles"),
                "combined": combined,
                "lod": [get("LOD0 Size"), get("LOD1 Size"), get("LOD2 Size")],
                "shader": shader,
                "alpha": alpha,
            }));
        }
        println!("{}", serde_json::json!({"path": path, "shapes": shapes}));
    }
}
