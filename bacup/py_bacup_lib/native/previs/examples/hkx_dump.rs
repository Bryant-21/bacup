//! Dev harness: print the typed member tree of each bhkPhysicsSystem packfile
//! in a NIF (arrays truncated).
//!
//! Args: <nif>
use havok_native::hkx::read_packfile;
use havok_native::hkx::types::HkxValue;
use nif_core_native::model::{NifFile, NifValue};

fn show(value: &HkxValue, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    match value {
        HkxValue::Object(members) => {
            let mut out = String::from("{\n");
            for m in members {
                out += &format!("{pad}  {} = {}\n", m.name, show(&m.value, depth + 1));
            }
            out + &pad + "}"
        }
        HkxValue::Array(items) => {
            let shown: Vec<String> = items.iter().take(3).map(|v| show(v, depth + 1)).collect();
            format!("[{} items] {}", items.len(), shown.join(", "))
        }
        HkxValue::F32List(v) => format!("{:?}", &v[..v.len().min(8)]),
        other => format!("{other:?}").chars().take(120).collect(),
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let nif = NifFile::from_bytes(&std::fs::read(&path).unwrap(), None).unwrap();
    for block in nif.blocks.iter().filter(|b| b.type_name == "bhkPhysicsSystem") {
        let Some(NifValue::Struct(f)) = block.get_field("Binary Data") else { continue };
        let blob: Vec<u8> = match f.get("Data") {
            Some(NifValue::Array(items)) => items.iter().map(|v| v.as_i64() as u8).collect(),
            Some(NifValue::Bytes(b)) => b.clone(),
            _ => continue,
        };
        let hkx = read_packfile(&blob).unwrap();
        for (index, object) in hkx.objects().iter().enumerate() {
            println!("#{index} {}", object.class_name);
            for m in &object.members {
                println!("  {} = {}", m.name, show(&m.value, 1));
            }
        }
    }
}
