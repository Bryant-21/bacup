//! Dev harness: plan CELL precombines (interior or exterior) and diff their
//! combined references and group membership against a CK-generated plugin's
//! XCRI, and optionally the group names against CK's `_OC.NIF` files.
//!
//! Args: <plugin> <ck plugin> <cell hex>[,<cell hex>...] <out json> <master dir>... [--loose <dir>...]
//! [--archive-dir <FO4 Data>] [--ck-meshes <CK Meshes\PreCombined\<plugin> dir>]
//! [--write-groups <dir>] (writes our group NIFs for CK-identical groups)
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use previs_native::assets::AssetResolver;
use previs_native::plugin::{LoadOrder, Plugin, read_u32};
use previs_native::precombine::{
    CellPrecombine, Context, SourceCache, exterior_persistent_references, linked_references, plan_cell,
};

type Key = (String, u32);

fn xcri_groups(plugin: &Plugin, cell: u32) -> BTreeMap<u32, BTreeSet<Key>> {
    let record = plugin
        .records_of(b"CELL")
        .find(|r| r.form_id & 0x00FF_FFFF == cell)
        .expect("CK plugin has the CELL");
    let subrecords = plugin.subrecords_at(&record).unwrap();
    let Some(xcri) = subrecords.first(b"XCRI") else { return BTreeMap::new() };
    let meshes = read_u32(xcri, 0).unwrap() as usize;
    let words = read_u32(xcri, 4).unwrap() as usize;
    let mut groups: BTreeMap<u32, BTreeSet<Key>> = BTreeMap::new();
    for pair in 0..words / 2 {
        let offset = 8 + 4 * meshes + 8 * pair;
        let (owner, id) = plugin.owner_of(read_u32(xcri, offset).unwrap());
        groups.entry(read_u32(xcri, offset + 4).unwrap()).or_default().insert((owner.to_ascii_lowercase(), id));
    }
    groups
}

/// The references CK's previs kept uncombined (XPRI).
fn xpri_references(plugin: &Plugin, cell: u32) -> BTreeSet<Key> {
    let Some(record) = plugin.records_of(b"CELL").find(|r| r.form_id & 0x00FF_FFFF == cell) else {
        return BTreeSet::new();
    };
    let subrecords = plugin.subrecords_at(&record).unwrap();
    let Some(xpri) = subrecords.first(b"XPRI") else { return BTreeSet::new() };
    xpri.chunks_exact(4)
        .map(|raw| {
            let (owner, id) = plugin.owner_of(u32::from_le_bytes(raw.try_into().unwrap()));
            (owner.to_ascii_lowercase(), id)
        })
        .collect()
}

/// CK's group file names for a CELL: `<cell>_<key>_OC` (uppercase, no extension).
fn ck_group_names(dir: &PathBuf, cell_local: u32) -> BTreeSet<String> {
    let prefix = format!("{cell_local:08X}_");
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .map(|name| name.to_ascii_uppercase())
        .filter(|name| name.starts_with(&prefix) && name.ends_with("_OC.NIF"))
        .map(|name| name.trim_end_matches(".NIF").to_string())
        .collect()
}

#[derive(Default)]
struct Totals {
    cells: usize,
    ours_refs: usize,
    ck_refs: usize,
    both: usize,
    same_group: usize,
    ours_groups: usize,
    ck_groups: usize,
    identical_groups: usize,
    ck_names: usize,
    matched_names: usize,
    xpri: usize,
    xpri_combined: usize,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let plugin_path = PathBuf::from(args.next().unwrap());
    let ck_path = PathBuf::from(args.next().unwrap());
    let cells: Vec<u32> = args.next().unwrap().split(',').map(|c| u32::from_str_radix(c, 16).unwrap()).collect();
    let out_path = PathBuf::from(args.next().unwrap());
    let (mut master_dirs, mut loose_roots, mut archives) = (Vec::new(), Vec::new(), Vec::new());
    let (mut groups_dir, mut ck_meshes, mut ck_tomes): (Option<PathBuf>, Option<PathBuf>, Option<PathBuf>) =
        (None, None, None);
    let mut mode = String::from("master");
    for arg in args {
        match arg.as_str() {
            "--loose" | "--archive-dir" | "--write-groups" | "--ck-meshes" | "--ck-tomes" => mode = arg,
            _ if mode == "--write-groups" => groups_dir = Some(PathBuf::from(arg)),
            _ if mode == "--ck-meshes" => ck_meshes = Some(PathBuf::from(arg)),
            _ if mode == "--ck-tomes" => ck_tomes = Some(PathBuf::from(arg)),
            _ if mode == "--loose" => loose_roots.push(PathBuf::from(arg)),
            _ if mode == "--archive-dir" => {
                let mut found: Vec<PathBuf> = std::fs::read_dir(&arg)
                    .unwrap()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        let name = p.file_name().unwrap().to_string_lossy().to_lowercase();
                        name.ends_with(".ba2") && (name.contains(" - meshes") || name.contains(" - materials"))
                    })
                    .collect();
                found.sort();
                archives.extend(found);
            }
            _ => master_dirs.push(PathBuf::from(arg)),
        }
    }
    let load_order = LoadOrder::open(&plugin_path, &master_dirs).unwrap();
    let assets = AssetResolver::new(loose_roots, &archives).unwrap();
    let sources = SourceCache::default();
    let target = load_order.target();
    let linked_from = linked_references(target).unwrap();
    let exterior_persistent = exterior_persistent_references(target).unwrap();
    let context = Context {
        load_order: &load_order,
        assets: &assets,
        sources: &sources,
        linked_from: &linked_from,
        exterior_persistent: &exterior_persistent,
    };
    let ck_plugin = Plugin::open(&ck_path).unwrap();
    let key_of = |raw: u32| -> Key {
        let (owner, id) = target.owner_of(raw);
        (owner.to_ascii_lowercase(), id)
    };
    let mut totals = Totals::default();
    let mut ck_only_reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows = Vec::new();
    // Exterior grid (mod 32, as combined object IDs carry it) to our and CK's
    // (group key, shape count) lists.
    let mut mesh_lists: BTreeMap<(u32, u32), (Vec<(u32, usize)>, Vec<(u32, usize)>)> = BTreeMap::new();
    let mut same_ref_groups: BTreeMap<(u32, u32), BTreeSet<u32>> = BTreeMap::new();
    for cell_local in cells {
        let cell = (target.self_index() << 24) | cell_local;
        let plan = plan_cell(&context, cell).unwrap();
        let details = previs_native::records::read_cell(target, &target.record(cell).unwrap()).unwrap();
        if let (false, Some((x, y)), Some(meshes)) = (details.interior, details.grid, &ck_meshes) {
            let ck_keys: Vec<u32> = xcri_groups(&ck_plugin, cell_local).into_keys().collect();
            let ours = plan.as_ref().map(our_shape_counts).unwrap_or_default();
            mesh_lists.insert((x.rem_euclid(32) as u32, y.rem_euclid(32) as u32), (ours, ck_shape_counts(meshes, cell_local, &ck_keys)));
        }
        let mut ours: BTreeMap<u32, BTreeSet<Key>> = BTreeMap::new();
        for &(reference, mesh) in plan.iter().flat_map(|p| &p.xcri.reference_mesh_pairs) {
            ours.entry(mesh).or_default().insert(key_of(reference));
        }
        let ck = xcri_groups(&ck_plugin, cell_local);
        let flatten = |groups: &BTreeMap<u32, BTreeSet<Key>>| -> BTreeMap<Key, u32> {
            groups.iter().flat_map(|(&mesh, refs)| refs.iter().map(move |r| (r.clone(), mesh))).collect()
        };
        let (ours_refs, ck_refs) = (flatten(&ours), flatten(&ck));
        let both = ours_refs.keys().filter(|r| ck_refs.contains_key(*r)).count();
        let same_group = ours_refs.iter().filter(|(r, m)| ck_refs.get(*r) == Some(*m)).count();
        let identical = ours.iter().filter(|(mesh, refs)| ck.get(*mesh) == Some(*refs)).count();
        if let (false, Some((x, y))) = (details.interior, details.grid) {
            let same: BTreeSet<u32> = ours.iter().filter(|(mesh, refs)| ck.get(*mesh) == Some(*refs)).map(|(m, _)| *m).collect();
            same_ref_groups.insert((x.rem_euclid(32) as u32, y.rem_euclid(32) as u32), same);
        }
        let names = ck_meshes.as_ref().map(|dir| {
            let ck_names = ck_group_names(dir, cell_local);
            let our_names: BTreeSet<String> = plan.iter().flat_map(|p| &p.groups).map(|g| g.root_name.clone()).collect();
            (ck_names.len(), ck_names.intersection(&our_names).count())
        });
        let xpri = xpri_references(&ck_plugin, cell_local);
        let xpri_combined = xpri.iter().filter(|k| ours_refs.contains_key(*k)).count();
        println!(
            "CELL {cell_local:06X}: refs ours {} CK {} both {both} same-group {same_group} (CK only {}, ours only {}) | groups ours {} CK {} identical {identical}{} | CK XPRI {} (we combine {xpri_combined})",
            ours_refs.len(),
            ck_refs.len(),
            ck_refs.len() - both,
            ours_refs.len() - both,
            ours.len(),
            ck.len(),
            names.map_or(String::new(), |(ck_names, matched)| format!(" | CK NIF names {matched}/{ck_names}")),
            xpri.len(),
        );
        totals.xpri += xpri.len();
        totals.xpri_combined += xpri_combined;
        let excluded: BTreeMap<u32, &str> =
            plan.iter().flat_map(|p| &p.excluded).map(|e| (e.form_id, e.reason.as_str())).collect();
        for key in ck_refs.keys().filter(|k| !ours_refs.contains_key(*k)) {
            let raw = target.raw_for(&key.0, key.1).unwrap_or(0);
            let reason = excluded.get(&raw).copied().unwrap_or("not a candidate of this CELL");
            // Collapse per-model detail so systematic causes aggregate.
            let reason = reason.split(": ").last().unwrap_or(reason).to_string();
            *ck_only_reasons.entry(reason).or_default() += 1;
        }
        totals.cells += 1;
        totals.ours_refs += ours_refs.len();
        totals.ck_refs += ck_refs.len();
        totals.both += both;
        totals.same_group += same_group;
        totals.ours_groups += ours.len();
        totals.ck_groups += ck.len();
        totals.identical_groups += identical;
        if let Some((ck_names, matched)) = names {
            totals.ck_names += ck_names;
            totals.matched_names += matched;
        }
        if let (Some(dir), Some(plan)) = (&groups_dir, &plan) {
            write_identical_groups(dir, plan, &ck, &ours);
        }
        let placed = exterior_persistent.get(&cell).map_or(&[][..], Vec::as_slice);
        rows.extend(reference_rows(&load_order, cell, placed, &plan, &ours_refs, &ck_refs, &key_of));
    }
    println!(
        "TOTAL {} CELLs: refs ours {} CK {} both {} same-group {} | groups ours {} CK {} identical {} | CK NIF names {}/{} | CK XPRI {} (we combine {})",
        totals.cells,
        totals.ours_refs,
        totals.ck_refs,
        totals.both,
        totals.same_group,
        totals.ours_groups,
        totals.ck_groups,
        totals.identical_groups,
        totals.matched_names,
        totals.ck_names,
        totals.xpri,
        totals.xpri_combined
    );
    if let Some(dir) = &ck_tomes {
        compare_combined_object_ids(dir, &mesh_lists, &same_ref_groups);
    }
    println!("CK-combined references we leave uncombined, by reason:");
    let mut reasons: Vec<_> = ck_only_reasons.into_iter().collect();
    reasons.sort_by(|a, b| b.1.cmp(&a.1));
    for (reason, count) in reasons {
        println!("  {count:6}  {reason}");
    }
    std::fs::write(&out_path, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
}

/// Decodes every exterior combined object ID (`0xFD`) in CK's tomes into its
/// CELL grid and XCRI mesh index, and checks that our XCRI puts the group CK
/// indexed at that position.
fn compare_combined_object_ids(
    dir: &PathBuf,
    mesh_lists: &BTreeMap<(u32, u32), (Vec<(u32, usize)>, Vec<(u32, usize)>)>,
    same_ref_groups: &BTreeMap<(u32, u32), BTreeSet<u32>>,
) {
    let (mut total, mut planned, mut beyond_ck, mut same_shape) = (0, 0, 0, 0);
    let mut cells_seen = BTreeSet::new();
    let locate = |shapes: &[(u32, usize)], mut index: usize| -> Option<(u32, usize)> {
        for &(key, count) in shapes {
            if index < count {
                return Some((key, index));
            }
            index -= count;
        }
        None
    };
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        if !entry.file_name().to_string_lossy().to_ascii_lowercase().ends_with(".uvd") {
            continue;
        }
        let tome = previs_native::umbra_tome::parse(&std::fs::read(entry.path()).unwrap()).unwrap();
        for id in tome.user_ids.iter().flatten().copied().filter(|id| id >> 24 == 0xFD) {
            total += 1;
            let slot = (id >> 14) & 0x3FF;
            let Some((ours, ck)) = mesh_lists.get(&(slot >> 5, slot & 31)) else { continue };
            planned += 1;
            cells_seen.insert(slot);
            let index = (id & 0x3FFF) as usize;
            match locate(ck, index) {
                None => beyond_ck += 1,
                Some(found) => same_shape += (locate(ours, index) == Some(found)) as usize,
            }
        }
    }
    let identical = cells_seen.iter().filter(|slot| mesh_lists.get(&(*slot >> 5, *slot & 31)).is_some_and(|(o, c)| o == c)).count();
    let (mut shape_diffs, mut same_refs_diffs, mut same_refs_groups) = (BTreeMap::<(usize, usize), usize>::new(), 0, 0);
    for (grid, (ours, ck)) in mesh_lists {
        let same_refs = &same_ref_groups[grid];
        let ours: BTreeMap<u32, usize> = ours.iter().copied().collect();
        for (key, ck_shapes) in ck {
            same_refs_groups += same_refs.contains(key) as usize;
            if let Some(&our_shapes) = ours.get(key).filter(|&&n| n != *ck_shapes) {
                *shape_diffs.entry((our_shapes, *ck_shapes)).or_default() += 1;
                if same_refs.contains(key) {
                    same_refs_diffs += 1;
                    println!("  shape count differs with the same references: grid {grid:?} group {key:08X} ours {our_shapes} CK {ck_shapes}");
                }
            }
        }
    }
    println!(
        "  shared groups whose shape counts differ: {} ({same_refs_diffs} of the {same_refs_groups} with identical reference sets); (ours, CK): {shape_diffs:?}",
        shape_diffs.values().sum::<usize>()
    );
    println!(
        "CK tome combined object IDs: {total}, in planned CELLs {planned}, past CK's shapes {beyond_ck}, naming the same group shape in ours {same_shape}; CELLs with identical (group, shape count) lists {identical}/{}",
        cells_seen.len()
    );
}

const SHAPE_TYPES: [&str; 3] = ["BSTriShape", "BSMeshLODTriShape", "BSSubIndexTriShape"];

fn shape_count(bytes: &[u8]) -> usize {
    let nif = nif_core_native::model::NifFile::from_bytes_raw_arrays(bytes, None).unwrap();
    nif.blocks.iter().filter(|b| SHAPE_TYPES.contains(&b.type_name.as_str())).count()
}

/// Shapes per group in XCRI order, as the exterior combined object index counts them.
fn our_shape_counts(plan: &CellPrecombine) -> Vec<(u32, usize)> {
    let offsets = plan_offsets(plan);
    plan.groups.iter().map(|group| (group.mesh_id, group_bytes(plan, group, &offsets).map_or(0, |b| shape_count(&b)))).collect()
}

/// CK's shapes per XCRI group; a group CK wrote no NIF for has none.
fn ck_shape_counts(dir: &PathBuf, cell_local: u32, keys: &[u32]) -> Vec<(u32, usize)> {
    keys.iter()
        .map(|&key| {
            let path = dir.join(format!("{cell_local:08X}_{key:08X}_OC.NIF"));
            (key, std::fs::read(path).map_or(0, |bytes| shape_count(&bytes)))
        })
        .collect()
}

fn plan_offsets(plan: &CellPrecombine) -> Vec<Vec<u32>> {
    let buffers: Vec<&[u8]> = plan.sources.iter().flat_map(|s| s.model.shapes.iter()).map(|s| s.geometry.as_slice()).collect();
    let mut offsets = previs_native::psg::deduplicated_offsets(&buffers).into_iter();
    plan.sources.iter().map(|s| offsets.by_ref().take(s.model.shapes.len()).collect()).collect()
}

fn group_bytes(
    plan: &CellPrecombine,
    group: &previs_native::precombine::PlannedGroup,
    source_offsets: &[Vec<u32>],
) -> previs_native::error::Result<Vec<u8>> {
    let mut sources = Vec::new();
    for member in &group.members {
        for index in plan.bases[member.base].sources.clone() {
            let source = &plan.sources[index];
            sources.push(previs_native::group::GroupSource {
                model: &source.model,
                instances: match &source.component {
                    Some(component) => member.instances.iter().map(|i| i.compose(component)).collect(),
                    None => member.instances.clone(),
                },
                base_record_flags: source.base_record_flags,
                leaf: source.leaf,
                editor_id: &source.editor_id,
                data_offsets: source_offsets[index].clone(),
                merges_collision: source.merges_collision,
            });
        }
    }
    previs_native::group::build_group(&previs_native::group::GroupRequest {
        root_name: group.root_name.clone(),
        sources,
        psg_file_name: "SeventySix - Geometry.psg".into(),
        export_info: plan.bases[group.members[0].base].export_info,
    })
}

fn write_identical_groups(
    dir: &PathBuf,
    plan: &CellPrecombine,
    ck: &BTreeMap<u32, BTreeSet<Key>>,
    ours: &BTreeMap<u32, BTreeSet<Key>>,
) {
    std::fs::create_dir_all(dir).unwrap();
    let source_offsets = plan_offsets(plan);
    let (mut written, mut failed) = (0, 0);
    for group in &plan.groups {
        if ck.get(&group.mesh_id) != ours.get(&group.mesh_id) {
            continue;
        }
        match group_bytes(plan, group, &source_offsets) {
            Ok(bytes) => {
                std::fs::write(dir.join(format!("{}.NIF", group.root_name)), bytes).unwrap();
                written += 1;
            }
            Err(error) => {
                println!("group {} failed: {error}", group.root_name);
                failed += 1;
            }
        }
    }
    println!("wrote {written} CK-identical groups, {failed} failed");
}

/// One JSON row per REFR the CELL loads (world-persistent ones included),
/// with our and CK's group and our exclusion reason.
fn reference_rows(
    load_order: &LoadOrder,
    cell: u32,
    placed: &[previs_native::plugin::RecordRef],
    plan: &Option<CellPrecombine>,
    ours_refs: &BTreeMap<Key, u32>,
    ck_refs: &BTreeMap<Key, u32>,
    key_of: &dyn Fn(u32) -> Key,
) -> Vec<serde_json::Value> {
    let target = load_order.target();
    let excluded: BTreeMap<u32, &str> = plan.iter().flat_map(|p| &p.excluded).map(|e| (e.form_id, e.reason.as_str())).collect();
    let topology = target.cells.get(&cell).cloned().unwrap_or_default();
    let mut rows = Vec::new();
    for child in placed.iter().chain(&topology.persistent).chain(&topology.temporary) {
        if &child.signature != b"REFR" {
            continue;
        }
        let subrecords = target.subrecords_at(child).unwrap();
        let Some(base_raw) = subrecords.first(b"NAME").and_then(|d| read_u32(d, 0)) else { continue };
        let fields: Vec<String> = subrecords.iter().map(|(s, _)| String::from_utf8_lossy(&s).into_owned()).collect();
        let base = load_order.resolve(target, base_raw);
        let (base_owner, base_id) = target.owner_of(base_raw);
        let key = key_of(child.form_id);
        let swap_entries = |from: &Plugin, raw: Option<u32>| -> Option<Vec<String>> {
            let (plugin, record) = load_order.resolve(from, raw?)?;
            let subrecords = plugin.subrecords_at(&record).ok()?;
            Some(
                subrecords
                    .iter()
                    .filter(|(s, _)| matches!(s, b"BNAM" | b"SNAM" | b"CNAM"))
                    .map(|(s, d)| match &s {
                        b"CNAM" => format!("CNAM={:?}", previs_native::plugin::read_f32(d, 0)),
                        _ => format!("{}={}", String::from_utf8_lossy(&s), previs_native::plugin::zstring(d)),
                    })
                    .collect(),
            )
        };
        let reference_swap = swap_entries(target, subrecords.first(b"XMSP").and_then(|d| read_u32(d, 0)));
        let base_swap = base.as_ref().and_then(|(p, r)| {
            let raw = p.subrecords_at(r).ok()?.first(b"MODS").and_then(|d| read_u32(d, 0));
            swap_entries(p, raw)
        });
        let components: Vec<serde_json::Value> = base
            .as_ref()
            .filter(|(_, r)| &r.signature == b"SCOL")
            .map(|(p, r)| {
                let subrecords = p.subrecords_at(r).unwrap();
                subrecords
                    .iter()
                    .filter(|(s, _)| s == b"ONAM")
                    .filter_map(|(_, d)| load_order.resolve(p, read_u32(d, 0)?).map(|found| (p, found)))
                    .map(|(owner, (cp, cr))| {
                        let mods = cp.subrecords_at(&cr).ok().and_then(|s| s.first(b"MODS").and_then(|d| read_u32(d, 0)));
                        serde_json::json!({
                            "model": previs_native::records::read_base(cp, &cr).ok().and_then(|b| b.model),
                            "swap": mods.and_then(|raw| swap_entries(cp, Some(raw))),
                            "owner": owner.name.clone(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        rows.push(serde_json::json!({
            "cell": format!("{cell:08X}"),
            "ref": format!("{}:{:06X}", key.0, key.1),
            "ref_flags": child.flags,
            "fields": fields,
            "base": format!("{base_owner}:{base_id:06X}"),
            "base_sig": base.as_ref().map(|(_, r)| String::from_utf8_lossy(&r.signature).into_owned()),
            "base_flags": base.as_ref().map(|(_, r)| r.flags),
            "base_plugin": base.as_ref().map(|(p, _)| p.name.clone()),
            "model": base.as_ref().and_then(|(p, r)| previs_native::records::read_base(p, r).ok()).and_then(|b| b.model),
            "ours": ours_refs.get(&key).map(|m| format!("{m:08X}")),
            "ck": ck_refs.get(&key).map(|m| format!("{m:08X}")),
            "reason": excluded.get(&child.form_id),
            "reference_swap": reference_swap,
            "base_swap": base_swap,
            "components": components,
        }));
    }
    rows
}
