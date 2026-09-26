//! Dev harness: shape of the material swaps on interior STAT/SCOL references
//! and whether retail combined them.
//!
//! Args: <plugin> <masters dir>
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use previs_native::plugin::{LoadOrder, read_f32, read_u32, zstring};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let order = LoadOrder::open(&PathBuf::from(&args[0]), &[PathBuf::from(&args[1])]).unwrap();
    let plugin = order.target();
    let self_index = plugin.self_index();
    let mut stats: BTreeMap<String, [u32; 2]> = BTreeMap::new();
    let mut swaps: BTreeSet<(String, u32)> = BTreeSet::new();
    let mut bump = |key: String, hit: usize| stats.entry(key).or_default()[hit] += 1;
    for cell in plugin.records_of(b"CELL").filter(|c| c.form_id >> 24 == self_index) {
        let subs = plugin.subrecords_at(&cell).unwrap();
        let combined: BTreeSet<u32> = match subs.first(b"XCRI") {
            Some(xcri) => {
                let meshes = read_u32(xcri, 0).unwrap() as usize;
                let words = read_u32(xcri, 4).unwrap() as usize;
                (0..words / 2).map(|i| read_u32(xcri, 8 + 4 * (meshes + 2 * i)).unwrap()).collect()
            }
            None => continue,
        };
        let Some(topology) = plugin.cells.get(&cell.form_id) else { continue };
        if topology.world.is_some() {
            continue;
        }
        for child in topology.temporary.iter().chain(&topology.persistent) {
            if &child.signature != b"REFR" {
                continue;
            }
            let subs = plugin.subrecords_at(child).unwrap();
            let Some(name) = subs.first(b"NAME") else { continue };
            let Some((base_plugin, base)) = order.resolve(plugin, read_u32(name, 0).unwrap()) else { continue };
            if !matches!(&base.signature, b"STAT" | b"SCOL") {
                continue;
            }
            let hit = combined.contains(&child.form_id) as usize;
            let base_mods = base_plugin.subrecords_at(&base).unwrap().first(b"MODS").and_then(|d| read_u32(d, 0));
            let xmsp = subs.first(b"XMSP").and_then(|d| read_u32(d, 0));
            match (xmsp, base_mods) {
                (None, None) => continue,
                (Some(_), None) => bump("ref XMSP only".into(), hit),
                (None, Some(_)) => bump("base MODS only".into(), hit),
                (Some(a), Some(b)) => bump(format!("both (same raw={})", a == b), hit),
            }
            let (swap_owner, swap_raw) = match xmsp {
                Some(raw) => (plugin, raw),
                None => (base_plugin, base_mods.unwrap()),
            };
            let Some((mp, mswp)) = order.resolve(swap_owner, swap_raw) else {
                bump("swap unresolved".into(), hit);
                continue;
            };
            let msubs = mp.subrecords_at(&mswp).unwrap();
            let mut entries = 0;
            let mut cnam = 0;
            let mut glob = false;
            let mut prefixed = false;
            let mut empty = 0;
            let mut fields = BTreeSet::new();
            for (s, d) in msubs.iter() {
                fields.insert(String::from_utf8_lossy(&s).to_string());
                match &s {
                    b"BNAM" => {
                        entries += 1;
                        let b = zstring(d);
                        glob |= b.contains(['*', '?', '[']);
                        prefixed |= b.to_ascii_lowercase().starts_with("materials\\");
                    }
                    b"SNAM" => {
                        let v = zstring(d);
                        empty += v.is_empty() as u32;
                        glob |= v.contains('*');
                        prefixed |= v.to_ascii_lowercase().starts_with("materials\\");
                    }
                    b"CNAM" => cnam += (read_f32(d, 0).unwrap_or(f32::MAX) < f32::MAX) as u32,
                    _ => {}
                }
            }
            bump(format!("fields {}", fields.into_iter().collect::<Vec<_>>().join(",")), hit);
            bump(format!("entries={}", entries.min(5)), hit);
            if cnam > 0 {
                bump("has CNAM".into(), hit);
            }
            if glob {
                bump("glob".into(), hit);
            }
            if prefixed {
                bump("materials\\ prefix".into(), hit);
            }
            if empty > 0 {
                bump("empty SNAM".into(), hit);
            }
            swaps.insert((mp.name.clone(), mswp.form_id));
        }
    }
    println!("{}: {} distinct swaps (out, in)", plugin.name, swaps.len());
    for (k, [o, i]) in stats {
        println!("  {k:<40} out={o:<6} in={i}");
    }
}
