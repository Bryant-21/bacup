//! Dev harness: compares retail XCRI mesh IDs with room/multibound marker
//! REFR FormIDs in the same CELL.
use std::collections::BTreeSet;
use std::path::PathBuf;

use previs_native::plugin::{LoadOrder, read_u32};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let order = LoadOrder::open(&PathBuf::from(&args[0]), &[PathBuf::from(&args[1])]).unwrap();
    let plugin = order.target();
    for cell in plugin.records_of(b"CELL").filter(|c| c.form_id >> 24 == plugin.self_index()) {
        let subs = plugin.subrecords_at(&cell).unwrap();
        let Some(xcri) = subs.first(b"XCRI") else { continue };
        let meshes: BTreeSet<u32> = (0..read_u32(xcri, 0).unwrap() as usize).map(|i| read_u32(xcri, 8 + 4 * i).unwrap()).collect();
        let Some(topology) = plugin.cells.get(&cell.form_id) else { continue };
        let mut markers = BTreeSet::new();
        for child in topology.temporary.iter().chain(&topology.persistent) {
            let s = plugin.subrecords_at(child).unwrap();
            let Some(name) = s.first(b"NAME") else { continue };
            let raw = read_u32(name, 0).unwrap();
            if let Some((p, base)) = order.resolve(plugin, raw) {
                let (owner, id) = plugin.owner_of(raw);
                let _ = p;
                if owner.eq_ignore_ascii_case("Fallout4.esm") && (id == 0x1F || id == 0x15) {
                    markers.insert(child.form_id);
                }
                let _ = base;
            }
        }
        let hits = meshes.intersection(&markers).count();
        let big = meshes.iter().filter(|m| **m & 0x4000_0000 != 0 && **m & 0x8000_0000 == 0).count();
        let b31 = meshes.iter().filter(|m| **m & 0x8000_0000 != 0).count();
        let both = meshes.iter().filter(|m| **m & 0xC000_0000 == 0xC000_0000).count();
        println!("   bit31={b31} bit30only={big} both={both}");
        println!("CELL {:08X}: meshes={} room/multibound markers={} mesh-ids-that-are-marker-formids={} mesh-ids>=2^30={}", cell.form_id, meshes.len(), markers.len(), hits, big);
    }
}
