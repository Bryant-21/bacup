//! Dev harness: plan one CELL, build its `_Physics.NIF` the way the precombine
//! stage does, write it, and compare it with a CK-generated one.
//!
//! Args: <plugin> <cell object id hex> <output dir> <CK PreCombined dir> <master dir> <loose root> <archive>...
use std::collections::BTreeMap;
use std::path::PathBuf;

use previs_native::assets::AssetResolver;
use previs_native::combined_physics::{canonicalize_packfile, physics_file_name};
use previs_native::havok_mesh::{
    CompressedMesh, DEAD_PRIMITIVE, HKX_MAGIC, PrecombineSource, is_static_layer_one_system, parse_compressed_mesh,
    parse_precombine_source,
};
use previs_native::plugin::LoadOrder;
use previs_native::precombine::{Context, SourceCache, exterior_persistent_references, linked_references, plan_cell};
use previs_native::precombine_stage::build_cell_physics;

fn packfile(nif: &[u8]) -> &[u8] {
    let start = nif.windows(8).position(|w| w == HKX_MAGIC).unwrap();
    let size = u32::from_le_bytes(nif[start - 4..start].try_into().unwrap()) as usize;
    &nif[start..start + size]
}

/// NiNode root translation, counted back from the packfile at the end.
fn root_translation(nif: &[u8], packfile_size: usize) -> [f32; 3] {
    let footer = 4 + 4;
    let collision_object = 14;
    let bsx = 8;
    let root = 80;
    let at = nif.len() - footer - 4 - packfile_size - collision_object - bsx - root + 20;
    std::array::from_fn(|a| f32::from_le_bytes(nif[at + a * 4..][..4].try_into().unwrap()))
}

fn summary(label: &str, mesh: &CompressedMesh, nif: &[u8], pack: &[u8]) {
    let customs: usize = mesh.sections.iter().map(|s| s.custom_primitives.len()).sum();
    let dead = mesh.sections.iter().flat_map(|s| &s.primitives).filter(|p| **p == DEAD_PRIMITIVE).count();
    let standard = mesh.sections.iter().flat_map(|s| &s.primitives).filter(|p| !(p[1] == p[2] && p[2] == p[3]) && **p != DEAD_PRIMITIVE);
    let quads = standard.clone().filter(|p| p[2] != p[3]).count();
    let triangles = standard.filter(|p| p[2] == p[3]).count();
    let mut degenerate = 0;
    let mut collinear = 0;
    let mut seen: rustc_hash::FxHashMap<[[u32; 3]; 3], usize> = Default::default();
    for section in &mesh.sections {
        for p in section.primitives.iter().filter(|p| !(p[1] == p[2] && p[2] == p[3]) && **p != DEAD_PRIMITIVE) {
            let halves: &[[u8; 3]] = if p[2] == p[3] { &[[p[0], p[1], p[2]]] } else { &[[p[0], p[1], p[2]], [p[0], p[2], p[3]]] };
            for t in halves {
                let v = t.map(|i| section.vertices[i as usize]);
                let mut key = v.map(|p| p.map(f32::to_bits));
                key.sort();
                *seen.entry(key).or_default() += 1;
                if v[0] == v[1] || v[1] == v[2] || v[0] == v[2] {
                    degenerate += 1;
                    continue;
                }
                let e1: [f32; 3] = std::array::from_fn(|a| v[1][a] - v[0][a]);
                let e2: [f32; 3] = std::array::from_fn(|a| v[2][a] - v[0][a]);
                let cross = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                if cross.iter().map(|c| c * c).sum::<f32>() == 0.0 {
                    collinear += 1;
                }
            }
        }
    }
    let duplicates: usize = seen.values().map(|&c| c - 1).sum();
    println!(
        "{label}: quads={quads} triangles={triangles} triangle total={} degenerate={degenerate} zero-area={collinear} duplicate triangles={duplicates}",
        quads * 2 + triangles
    );
    let pages = mesh.sections.iter().map(|s| s.page).max().unwrap_or(0) + 1;
    println!(
        "{label}: nif={} packfile={} sections={} primitives={} dead={dead} customs={} shared_words={} pages={} materials={} aabb={:?}..{:?} root={:?}",
        nif.len(),
        pack.len(),
        mesh.sections.len(),
        mesh.primitive_count(),
        customs,
        mesh.shared_vertices.len(),
        pages,
        mesh.material_entries.len(),
        mesh.object_aabb_min,
        mesh.object_aabb_max,
        root_translation(nif, pack.len()),
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let target = PathBuf::from(&args[0]);
    let cell_object = u32::from_str_radix(&args[1], 16).unwrap();
    let output_dir = PathBuf::from(&args[2]);
    let ck_dir = PathBuf::from(&args[3]);
    let load_order = LoadOrder::open(&target, &[PathBuf::from(&args[4])]).unwrap();
    let assets = AssetResolver::new(vec![PathBuf::from(&args[5])], &args[6..].iter().map(PathBuf::from).collect::<Vec<_>>()).unwrap();
    let plugin = load_order.target();
    let cell = plugin.raw_for(&plugin.name, cell_object).unwrap();
    let sources = SourceCache::default();
    let linked_from = linked_references(plugin).unwrap();
    let exterior_persistent = exterior_persistent_references(plugin).unwrap();
    let context = Context {
        load_order: &load_order,
        assets: &assets,
        sources: &sources,
        linked_from: &linked_from,
        exterior_persistent: &exterior_persistent,
    };

    let pool = rayon::ThreadPoolBuilder::new().num_threads(previs_native::default_workers()).build().unwrap();
    let started = std::time::Instant::now();
    let plan = pool.install(|| plan_cell(&context, cell)).unwrap().unwrap();
    println!(
        "plan {:.1}s: references={} excluded={} groups={} sources={}",
        started.elapsed().as_secs_f64(),
        plan.placements.len(),
        plan.excluded.len(),
        plan.groups.len(),
        plan.sources.len()
    );
    let mut exclusions: BTreeMap<&str, usize> = BTreeMap::new();
    for excluded in &plan.excluded {
        *exclusions.entry(excluded.reason.as_str()).or_default() += 1;
    }
    for (reason, count) in &exclusions {
        println!("  excluded {count:>5}  {reason}");
    }
    let mut unscalable_models: BTreeMap<String, usize> = BTreeMap::new();
    for excluded in plan.excluded.iter().filter(|e| e.reason.contains("round-trip")) {
        let record = plugin.record(excluded.form_id).unwrap();
        let base_raw = previs_native::plugin::read_u32(plugin.subrecords_at(&record).unwrap().first(b"NAME").unwrap(), 0).unwrap();
        let (base_plugin, base) = load_order.resolve(plugin, base_raw).unwrap();
        let editor_id = base_plugin.subrecords_at(&base).unwrap().first(b"EDID").map(previs_native::plugin::zstring).unwrap_or_default();
        *unscalable_models.entry(format!("{} {editor_id}", String::from_utf8_lossy(&base.signature))).or_default() += 1;
    }
    println!("  unscalable cloned collision by base: {unscalable_models:?}");

    let mut routing: BTreeMap<String, (usize, &str)> = BTreeMap::new();
    for &(base, _) in &plan.placements {
        for source in &plan.sources[plan.bases[base].sources.clone()] {
            let Some(collision) = &source.model.collision else { continue };
            let parts = |merged: &[(usize, std::sync::Arc<PrecombineSource>)]| {
                merged.iter().flat_map(|(_, s)| &s.bodies).flat_map(|b| &b.parts).cloned().collect::<Vec<_>>()
            };
            let key = match &collision.merged {
                Some(_) if !source.merges_collision => "cloned (a base shape fails CanAddToCombinedShape)".to_string(),
                Some(merged) if parts(merged).iter().any(|p| p.mesh.sections.iter().any(|s| s.primitives.contains(&DEAD_PRIMITIVE))) => {
                    "merged (source has removed primitives)".to_string()
                }
                Some(merged) if collision.nodes.len() > 1 => format!("merged ({} collision nodes)", merged.len()),
                Some(merged) if parts(merged).iter().any(|p| p.instance.is_some()) => "merged (compound)".to_string(),
                Some(_) => "merged".to_string(),
                None if !is_static_layer_one_system(&collision.packfile).unwrap_or(false) => "cloned (not static layer 1)".into(),
                None => match parse_precombine_source(&collision.packfile) {
                    Ok(_) if collision.nodes.len() != 1 => format!("cloned fallback: {} collision nodes", collision.nodes.len()),
                    Ok(_) => "cloned (BSXFlags 0x40)".into(),
                    Err(error) => format!("cloned fallback: {error}"),
                },
            };
            routing.entry(key).or_insert((0, &source.model.path)).0 += 1;
        }
    }
    for (key, (count, example)) in &routing {
        println!("  instances {count:>6}  {key}  (e.g. {example})");
    }

    let started = std::time::Instant::now();
    let built = pool.install(|| build_cell_physics(&plan, &format!("Meshes\\PreCombined\\{}", plugin.name))).unwrap();
    let Some((output, nif)) = built else {
        println!("no merged collision");
        return;
    };
    println!("physics {:.1}s: {} {}", started.elapsed().as_secs_f64(), output.relative_path, output.sha256);
    let written = output_dir.join(physics_file_name(cell));
    std::fs::create_dir_all(&output_dir).unwrap();
    std::fs::write(&written, &nif).unwrap();

    let ours = packfile(&nif);
    let our_mesh = parse_compressed_mesh(ours).unwrap();
    summary("ours", &our_mesh, &nif, ours);
    let ck_nif = std::fs::read(ck_dir.join(physics_file_name(cell))).unwrap();
    let theirs = packfile(&ck_nif);
    let ck_mesh = parse_compressed_mesh(theirs).unwrap();
    summary("ck  ", &ck_mesh, &ck_nif, theirs);
    let same_prefix = our_mesh.material_entries.iter().zip(&ck_mesh.material_entries).take_while(|(a, b)| a == b).count();
    let only_ck: Vec<_> = ck_mesh.material_entries.iter().filter(|m| !our_mesh.material_entries.contains(m)).collect();
    let only_ours: Vec<_> = our_mesh.material_entries.iter().filter(|m| !ck_mesh.material_entries.contains(m)).collect();
    println!("materials: same order for the first {same_prefix}; only ck {only_ck:X?}; only ours {only_ours:X?}");
    let crcs = |mesh: &CompressedMesh| mesh.material_entries.iter().map(|m| format!("{:X}", m.1)).collect::<Vec<_>>().join(" ");
    println!("  ours {}\n  ck   {}", crcs(&our_mesh), crcs(&ck_mesh));
    let ck_order: Vec<u32> = ck_mesh.material_entries.iter().map(|m| m.1).collect();
    let order_of = |bases: &mut dyn Iterator<Item = usize>| {
        let mut order: Vec<u32> = Vec::new();
        for base in bases {
            for source in &plan.sources[plan.bases[base].sources.clone()] {
                let Some(merged) = source.model.collision.as_ref().and_then(|c| c.merged.as_ref()).filter(|_| source.merges_collision)
                else {
                    continue;
                };
                for part in merged.iter().flat_map(|(_, s)| &s.bodies).flat_map(|b| &b.parts) {
                    for &(value, _, _) in part.mesh.sections.iter().flat_map(|s| &s.primitive_data_runs) {
                        let crc = part.mesh.material_entries[value as usize].1;
                        if !order.contains(&crc) {
                            order.push(crc);
                        }
                    }
                }
            }
        }
        order.iter().zip(&ck_order).take_while(|(a, b)| a == b).count()
    };
    let group_bases = |reverse_groups: bool, reverse_members: bool| {
        let mut groups: Vec<&previs_native::precombine::PlannedGroup> = plan.groups.iter().collect();
        if reverse_groups {
            groups.reverse();
        }
        let mut bases = Vec::new();
        for group in groups {
            let mut members: Vec<usize> =
                group.members.iter().flat_map(|m| std::iter::repeat_n(m.base, m.instances.len())).collect();
            if reverse_members {
                members.reverse();
            }
            bases.extend(members);
        }
        bases
    };
    println!(
        "  CK-order prefix: reverse refs {} forward refs {} groups asc {} groups asc members rev {} groups desc {} groups desc members rev {}",
        order_of(&mut plan.placements.iter().rev().map(|p| p.0)),
        order_of(&mut plan.placements.iter().map(|p| p.0)),
        order_of(&mut group_bases(false, false).into_iter()),
        order_of(&mut group_bases(false, true).into_iter()),
        order_of(&mut group_bases(true, false).into_iter()),
        order_of(&mut group_bases(true, true).into_iter()),
    );
    let appearance = |mesh: &CompressedMesh| {
        let mut seen: Vec<u16> = Vec::new();
        for &(value, _, _) in mesh.sections.iter().flat_map(|s| &s.primitive_data_runs) {
            if !seen.contains(&value) {
                seen.push(value);
            }
        }
        seen.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")
    };
    println!("  index first appearance in sections: ours {}\n  ck {}", appearance(&our_mesh), appearance(&ck_mesh));
    let (ours, theirs) = (canonicalize_packfile(ours).unwrap(), canonicalize_packfile(theirs).unwrap());
    println!("canonical packfiles equal: {}", ours == theirs);

    let ck_havok_roots: Vec<String> = std::fs::read_dir(&ck_dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(&format!("{:08X}_", cell & 0x00FF_FFFF)))
        .filter(|p| p.to_string_lossy().ends_with("_OC.NIF"))
        .filter(|p| std::fs::read(p).unwrap().windows(9).any(|w| w == b"HavokRoot"))
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    let our_havok_roots: Vec<String> = plan
        .groups
        .iter()
        .filter(|g| {
            g.members.iter().any(|m| {
                plan.sources[plan.bases[m.base].sources.clone()]
                    .iter()
                    .any(|s| s.model.collision.is_some() && !s.merges_collision)
            })
        })
        .map(|g| g.root_name.clone())
        .collect();
    for group in plan.groups.iter().filter(|g| our_havok_roots.contains(&g.root_name) && !ck_havok_roots.contains(&g.root_name)) {
        for member in &group.members {
            for source in &plan.sources[plan.bases[member.base].sources.clone()] {
                if source.model.collision.is_some() && !source.merges_collision {
                    println!("  only ours {}: cloned {}", group.root_name, source.model.path);
                }
            }
        }
    }
    let only_ck = ck_havok_roots.iter().filter(|n| !our_havok_roots.contains(n)).count();
    let only_ours = our_havok_roots.iter().filter(|n| !ck_havok_roots.contains(n)).count();
    println!(
        "groups with HavokRoot: ck={} ours={} only ck={only_ck} only ours={only_ours}",
        ck_havok_roots.len(),
        our_havok_roots.len()
    );
}
