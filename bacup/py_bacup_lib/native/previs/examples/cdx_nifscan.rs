//! Dev harness: prints one JSON line per NIF with its shader materials,
//! texture-set slots and packed-geometry references (CDX oracle input).
use nif_core_native::model::{NifFile, NifValue};

fn text(value: &NifValue) -> Option<String> {
    match value {
        NifValue::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn main() {
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap();
        let nif = NifFile::from_bytes(&bytes, None).unwrap();
        let mut shaders = Vec::new();
        let mut geometry = Vec::new();
        for block in &nif.blocks {
            let field = |name: &str| block.fields.get(name);
            match block.type_name.as_str() {
                "BSLightingShaderProperty" | "BSEffectShaderProperty" => {
                    let textures = match field("Texture Set") {
                        Some(NifValue::Ref(r)) if *r >= 0 => match nif.blocks[*r as usize]
                            .fields
                            .get("Textures")
                        {
                            Some(NifValue::Array(items)) => items.iter().map(|t| text(t).unwrap_or_default()).collect(),
                            _ => Vec::new(),
                        },
                        _ => Vec::new(),
                    };
                    let effect: serde_json::Map<String, serde_json::Value> = ["Source Texture", "Greyscale Texture", "Env Map Texture", "Normal Texture", "Env Mask Texture", "Reflectance Texture", "Lighting Texture", "Emit Gradient Texture"]
                        .iter()
                        .filter_map(|k| field(k).and_then(text).map(|v| (k.to_string(), serde_json::Value::String(v))))
                        .collect();
                    shaders.push(serde_json::json!({
                        "type": block.type_name,
                        "name": field("Name").and_then(text),
                        "textures": textures,
                        "effect_textures": effect,
                    }));
                }
                "BSPackedCombinedSharedGeomDataExtra" => {
                    if let Some(NifValue::Array(objects)) = field("Object") {
                        for object in objects {
                            if let NifValue::Struct(s) = object {
                                let get = |k: &str| match s.get(k) {
                                    Some(NifValue::UInt(v)) => *v,
                                    _ => u64::MAX,
                                };
                                geometry.push((get("Data Offset"), get("Filename Hash")));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        println!("{}", serde_json::json!({"path": path, "shaders": shaders, "geometry": geometry}));
    }
}
