//! Dev harness: JSON lines of retail-combined interior references with their
//! position and any color remap sources (base MODC, swap CNAM entries).
//!
//! Args: <plugin> <masters dir>
use std::collections::BTreeSet;
use std::path::PathBuf;

use previs_native::plugin::{LoadOrder, read_f32, read_u32, zstring};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let order = LoadOrder::open(&PathBuf::from(&args[0]), &[PathBuf::from(&args[1])]).unwrap();
    let plugin = order.target();
    let self_index = plugin.self_index();
    for cell in plugin.records_of(b"CELL").filter(|c| c.form_id >> 24 == self_index) {
        let subs = plugin.subrecords_at(&cell).unwrap();
        let Some(xcri) = subs.first(b"XCRI") else { continue };
        let meshes = read_u32(xcri, 0).unwrap() as usize;
        let words = read_u32(xcri, 4).unwrap() as usize;
        let combined: BTreeSet<u32> = (0..words / 2).map(|i| read_u32(xcri, 8 + 4 * (meshes + 2 * i)).unwrap()).collect();
        let Some(topology) = plugin.cells.get(&cell.form_id) else { continue };
        for child in topology.temporary.iter().chain(&topology.persistent) {
            if !combined.contains(&child.form_id) {
                continue;
            }
            let subs = plugin.subrecords_at(child).unwrap();
            let Some(name) = subs.first(b"NAME") else { continue };
            let Some((bp, base)) = order.resolve(plugin, read_u32(name, 0).unwrap()) else { continue };
            let bsubs = bp.subrecords_at(&base).unwrap();
            let modc = bsubs.first(b"MODC").and_then(|d| read_f32(d, 0));
            let swap_raw = subs.first(b"XMSP").map(|d| (plugin, read_u32(d, 0).unwrap()))
                .or_else(|| bsubs.first(b"MODS").map(|d| (bp, read_u32(d, 0).unwrap())));
            let mut cnams = Vec::new();
            if let Some((owner, raw)) = swap_raw {
                if let Some((mp, mswp)) = order.resolve(owner, raw) {
                    let mut current = String::new();
                    for (s, d) in mp.subrecords_at(&mswp).unwrap().iter() {
                        match &s {
                            b"BNAM" => current = zstring(d),
                            b"CNAM" => cnams.push(format!("{{\"b\":{:?},\"c\":{}}}", current, read_f32(d, 0).unwrap())),
                            _ => {}
                        }
                    }
                }
            }
            if modc.is_none() && cnams.is_empty() {
                continue;
            }
            let data = subs.first(b"DATA").unwrap();
            let p = |i: usize| read_f32(data, i * 4).unwrap();
            let edid = bsubs.first(b"EDID").map(zstring).unwrap_or_default();
            println!(
                "{{\"cell\":\"{:08x}\",\"ref\":\"{:08X}\",\"base\":{:?},\"pos\":[{},{},{}],\"modc\":{},\"cnam\":[{}]}}",
                cell.form_id & 0xFFFFFF,
                child.form_id,
                edid,
                p(0),
                p(1),
                p(2),
                modc.map_or("null".to_string(), |v| v.to_string()),
                cnams.join(",")
            );
        }
    }
}
