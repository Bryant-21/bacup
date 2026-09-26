//! Dev harness: tabulates which placed references retail CK precombined
//! (XCRI) against their plugin-visible facts, to recover eligibility.
//!
//! Args: <plugin> <masters dir>
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use previs_native::plugin::{LoadOrder, read_u32};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let order = LoadOrder::open(&PathBuf::from(&args[0]), &[PathBuf::from(&args[1])]).unwrap();
    let plugin = order.target();
    let self_index = plugin.self_index();

    let mut linked_to: BTreeSet<u32> = BTreeSet::new();
    for sig in [b"REFR", b"ACHR"] {
        for record in plugin.records_of(sig) {
            let subs = plugin.subrecords_at(&record).unwrap();
            for (s, d) in subs.iter() {
                if &s == b"XLKR" && d.len() >= 8 {
                    linked_to.insert(read_u32(d, 4).unwrap());
                }
            }
        }
    }

    let mut by_kind: BTreeMap<String, [u32; 2]> = BTreeMap::new();
    let mut by_sub: BTreeMap<String, [u32; 2]> = BTreeMap::new();
    let mut by_flag: BTreeMap<u32, [u32; 2]> = BTreeMap::new();
    let mut by_base: BTreeMap<String, [u32; 2]> = BTreeMap::new();
    let mut cells = 0;
    let mut samples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for cell in plugin.records_of(b"CELL").filter(|c| c.form_id >> 24 == self_index) {
        let subs = plugin.subrecords_at(&cell).unwrap();
        let Some(xcri) = subs.first(b"XCRI") else { continue };
        let meshes = read_u32(xcri, 0).unwrap() as usize;
        let words = read_u32(xcri, 4).unwrap() as usize;
        let combined: BTreeSet<u32> =
            (0..words / 2).map(|i| read_u32(xcri, 8 + 4 * (meshes + 2 * i)).unwrap()).collect();
        let Some(topology) = plugin.cells.get(&cell.form_id) else { continue };
        if topology.world.is_some() {
            continue;
        }
        cells += 1;
        for (persistent, children) in [(false, &topology.temporary), (true, &topology.persistent)] {
            for child in children.iter() {
                if &child.signature != b"REFR" {
                    continue;
                }
                let subs = plugin.subrecords_at(child).unwrap();
                let Some(name) = subs.first(b"NAME") else { continue };
                let Some((base_plugin, base)) = order.resolve(plugin, read_u32(name, 0).unwrap()) else {
                    continue;
                };
                let base_sig = String::from_utf8_lossy(&base.signature).to_string();
                let hit = combined.contains(&child.form_id) as usize;
                by_kind.entry(format!("{base_sig} persistent={persistent}")).or_default()[hit] += 1;
                if base_sig != "STAT" && base_sig != "SCOL" {
                    continue;
                }
                let mut sigs: BTreeSet<String> =
                    subs.iter().map(|(s, _)| String::from_utf8_lossy(&s).to_string()).collect();
                if linked_to.contains(&child.form_id) {
                    sigs.insert("<linked-from>".into());
                }
                if persistent {
                    sigs.insert("<persistent>".into());
                }
                for sig in &sigs {
                    by_sub.entry(sig.clone()).or_default()[hit] += 1;
                }
                for bit in 0..32 {
                    if child.flags & (1 << bit) != 0 {
                        by_flag.entry(bit).or_default()[hit] += 1;
                    }
                }
                let base_subs = base_plugin.subrecords_at(&base).unwrap();
                let mut base_sigs: BTreeSet<String> =
                    base_subs.iter().map(|(s, _)| format!("base:{}", String::from_utf8_lossy(&s))).collect();
                for bit in 0..32 {
                    if base.flags & (1 << bit) != 0 {
                        base_sigs.insert(format!("baseflag:{bit}"));
                    }
                }
                for sig in &base_sigs {
                    by_base.entry(sig.clone()).or_default()[hit] += 1;
                }
                let key = sigs.iter().filter(|s| !matches!(s.as_str(), "NAME" | "DATA" | "EDID" | "XSCL")).cloned().collect::<Vec<_>>().join(",");
                let entry = samples.entry(format!("{}|{}", if hit == 1 { "IN" } else { "OUT" }, key)).or_default();
                if entry.len() < 2 {
                    entry.push(format!("{:08X}@{:08X}", child.form_id, cell.form_id));
                }
            }
        }
    }
    println!("{}: {cells} XCRI interior cells", plugin.name);
    let show = |title: &str, rows: Vec<(String, [u32; 2])>| {
        println!("-- {title} (out, in)");
        for (k, [o, i]) in rows {
            println!("  {k:<32} out={o:<6} in={i}");
        }
    };
    show("base kind", by_kind.into_iter().collect());
    show("REFR subrecords (STAT/SCOL)", by_sub.into_iter().collect());
    show("REFR flag bits", by_flag.into_iter().map(|(b, v)| (format!("bit {b}"), v)).collect());
    show("base facts", by_base.into_iter().collect());
    println!("-- signature combos (first samples)");
    for (k, v) in samples {
        println!("  {k}  {}", v.join(" "));
    }
}
