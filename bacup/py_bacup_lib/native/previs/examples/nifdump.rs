fn main() {
    let path = std::env::args().nth(1).unwrap();
    let ids: Vec<usize> = std::env::args().skip(2).map(|s| s.parse().unwrap()).collect();
    let bytes = std::fs::read(&path).unwrap();
    let nif = nif_core_native::model::NifFile::from_bytes(&bytes, None).unwrap();
    for (i, b) in nif.blocks.iter().enumerate() {
        if !ids.is_empty() && !ids.contains(&i) { continue; }
        println!("== {} {}", i, b.type_name);
        for (k, v) in &b.fields {
            let s = format!("{:?}", v);
            println!("  {} = {}", k, if s.len() > 200 { &s[..200] } else { &s });
        }
    }
    println!("{:?}", nif.header.strings);
}
