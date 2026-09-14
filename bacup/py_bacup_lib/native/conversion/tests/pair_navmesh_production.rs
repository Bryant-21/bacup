use std::io::Write;

use conversion_native::run::{RunConfig, RunParams, create_run, drop_run, with_run};
use conversion_native::translator::Game;
use esp_authoring_core::nvnm::{NvnmParent, parse_nvnm};
use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, plugin_handle_close_native, plugin_handle_load_no_py,
    plugin_handle_new_no_py, plugin_handle_store_ref,
};

const FNV_CELL: u32 = 0x0008_01;
const FNV_NAVM: u32 = 0x0009_00;
const FO3_CELL: u32 = 0x0008_11;
const FO3_NAVM: u32 = 0x0009_11;
const EXTERIOR_WORLD: u32 = 0x0008_21;
const EXTERIOR_CELL: u32 = 0x0008_22;
const EXTERIOR_ACTI: u32 = 0x0008_23;
const EXTERIOR_REFR: u32 = 0x0008_24;
const EXTERIOR_NAVM_A: u32 = 0x0009_21;
const EXTERIOR_NAVM_B: u32 = 0x0009_22;
const SKYRIM_CELL: u32 = 0x0001_3A7E;
const SKYRIM_NAVM: u32 = 0x000E_537D;

#[test]
fn fnv_v2_tail_rebuilds_legacy_navmesh_into_fo4_temporary_topology_and_fresh_navi() {
    let bytes = plugin(
        &[interior_cell_tree(
            FNV_CELL,
            legacy_fallout_navmesh(FNV_CELL, FNV_NAVM),
        )],
        &[legacy_navi(FNV_NAVM, FNV_CELL, 11)],
    );
    let fixture = write_plugin(&bytes);
    let target = run_v2(Game::Fnv, fixture.path());

    let root = target_root(&target);
    let navm = only_record(&root, "NAVM");
    let nvnm = nvnm(&navm);
    assert_eq!(nvnm.version, 15);
    assert_eq!(nvnm.parent, NvnmParent::Interior { cell: FNV_CELL });
    assert!(record_is_inside_temporary_cell_group(
        &root,
        navm.form_id,
        FNV_CELL
    ));

    let navi = only_record(&root, "NAVI");
    assert_eq!(nver(&navi), 15);
    assert_eq!(
        navi.subrecords
            .iter()
            .filter(|sub| sub.signature.as_str() == "NVMI")
            .count(),
        1
    );
    assert_eq!(navi_nvmi_navmesh_ids(&navi), vec![navm.form_id]);
    assert_eq!(
        navi.subrecords
            .iter()
            .find(|sub| sub.signature.as_str() == "NVPP")
            .unwrap()
            .data
            .as_ref(),
        &[0; 8],
        "source NAVI pointers must not be copied into FO4"
    );

    close_run(target);
}

#[test]
fn fo3_v2_tail_rebuilds_legacy_navmesh_into_fo4_temporary_topology_and_fresh_navi() {
    let bytes = plugin(
        &[interior_cell_tree(
            FO3_CELL,
            legacy_fallout_navmesh(FO3_CELL, FO3_NAVM),
        )],
        &[legacy_navi(FO3_NAVM, FO3_CELL, 11)],
    );
    let fixture = write_plugin(&bytes);
    let target = run_v2(Game::Fo3, fixture.path());

    let root = target_root(&target);
    let navm = only_record(&root, "NAVM");
    let nvnm = nvnm(&navm);
    assert_eq!(nvnm.version, 15);
    assert_eq!(nvnm.parent, NvnmParent::Interior { cell: FO3_CELL });
    assert!(record_is_inside_temporary_cell_group(
        &root,
        navm.form_id,
        FO3_CELL
    ));

    let navi = only_record(&root, "NAVI");
    assert_eq!(nver(&navi), 15);
    assert_eq!(navi_nvmi_navmesh_ids(&navi), vec![navm.form_id]);
    assert_eq!(
        navi_nvmi_edge_targets(&navi),
        vec![(navm.form_id, Vec::new())]
    );

    close_run(target);
}

#[test]
fn fnv_v2_tail_rebuilds_exterior_navmeshes_under_worldspace_temporary_groups() {
    let bytes = plugin(
        &[
            anchor_activator(),
            exterior_worldspace_tree(
                EXTERIOR_WORLD,
                EXTERIOR_CELL,
                &[
                    legacy_exterior_navmesh(
                        EXTERIOR_CELL,
                        EXTERIOR_NAVM_A,
                        EXTERIOR_NAVM_B,
                        2,
                        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    ),
                    legacy_exterior_navmesh(
                        EXTERIOR_CELL,
                        EXTERIOR_NAVM_B,
                        EXTERIOR_NAVM_A,
                        1,
                        [[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    ),
                ],
            ),
        ],
        &[legacy_navi(EXTERIOR_NAVM_A, EXTERIOR_CELL, 11)],
    );
    let fixture = write_plugin(&bytes);
    let target = run_v2(Game::Fnv, fixture.path());

    let root = target_root(&target);
    let mut navmeshes = Vec::new();
    collect_records(&root, "NAVM", &mut navmeshes);
    assert_eq!(navmeshes.len(), 2);
    let navmesh_ids = navmeshes
        .iter()
        .map(|record| record.form_id)
        .collect::<Vec<_>>();
    for navm in &navmeshes {
        let nvnm = nvnm(navm);
        assert_eq!(nvnm.version, 15);
        assert_eq!(
            nvnm.parent,
            NvnmParent::Exterior {
                world: EXTERIOR_WORLD,
                grid_x: 0,
                grid_y: 0,
            }
        );
        assert!(record_is_inside_temporary_cell_group(
            &root,
            navm.form_id,
            EXTERIOR_CELL
        ));
    }
    assert!(cell_is_nested_under_worldspace(
        &root,
        EXTERIOR_WORLD,
        EXTERIOR_CELL
    ));
    let navi = only_record(&root, "NAVI");
    assert_eq!(nver(&navi), 15);
    assert_eq!(navi_nvmi_navmesh_ids(&navi), navmesh_ids);
    for (source, edges) in navi_nvmi_edge_targets(&navi) {
        assert_eq!(edges.len(), 1);
        assert!(navmesh_ids.contains(&source));
        assert!(navmesh_ids.contains(&edges[0]));
        assert_ne!(source, edges[0]);
    }

    close_run(target);
}

#[test]
fn skyrim_v2_tail_lowers_v12_navmesh_into_fo4_temporary_topology_and_fresh_navi() {
    let source_nvnm = hex::decode(
        include_str!("../../../../../py_creation_lib/native/esp/src/nvnm/tests/fixtures/0e537d_skyrim_v12.nvnm.hex").trim(),
    )
    .unwrap();
    let bytes = plugin(
        &[interior_cell_tree(
            SKYRIM_CELL,
            record(b"NAVM", SKYRIM_NAVM, &subrecord(b"NVNM", &source_nvnm)),
        )],
        &[legacy_navi(SKYRIM_NAVM, SKYRIM_CELL, 12)],
    );
    let fixture = write_plugin(&bytes);
    let target = run_v2(Game::SkyrimSe, fixture.path());

    let root = target_root(&target);
    let navm = only_record(&root, "NAVM");
    let nvnm = nvnm(&navm);
    assert_eq!(nvnm.version, 15);
    assert_eq!(nvnm.parent, NvnmParent::Interior { cell: SKYRIM_CELL });
    assert_eq!(nvnm.vertices.len(), 95);
    assert_eq!(nvnm.triangles.len(), 98);
    assert!(record_is_inside_temporary_cell_group(
        &root,
        navm.form_id,
        SKYRIM_CELL
    ));

    let navi = only_record(&root, "NAVI");
    assert_eq!(nver(&navi), 15, "source NVER=12 must not survive into FO4");
    assert_eq!(
        navi.subrecords
            .iter()
            .filter(|sub| sub.signature.as_str() == "NVMI")
            .count(),
        1
    );
    assert_eq!(navi_nvmi_navmesh_ids(&navi), vec![navm.form_id]);
    assert_eq!(
        navi.subrecords
            .iter()
            .find(|sub| sub.signature.as_str() == "NVPP")
            .unwrap()
            .data
            .as_ref(),
        &[0; 8],
        "source NAVI pointers must not be copied into FO4"
    );

    close_run(target);
}

struct TargetRun {
    run_id: u64,
    source_handle: u64,
    target_handle: u64,
}

fn run_v2(source_game: Game, fixture: &std::path::Path) -> TargetRun {
    let source_handle = plugin_handle_load_no_py(
        &fixture.to_string_lossy(),
        Some(source_game.as_str()),
        None,
        None,
        false,
    )
    .unwrap();
    let target_handle = plugin_handle_new_no_py("NavmeshProduction.esp", Some("fo4"));
    let run_id = create_run(RunParams {
        source: source_game,
        target: Game::Fo4,
        source_handle_id: source_handle,
        target_handle_id: target_handle,
        master_handle_ids: Vec::new(),
        config: RunConfig {
            output_plugin_name: "NavmeshProduction.esp".into(),
            is_whole_plugin: true,
            preserve_source_ids: true,
            fnv_quest_slice: matches!(source_game, Game::Fnv | Game::Fo3),
            ..RunConfig::default()
        },
    })
    .unwrap();
    with_run(run_id, |run| run.translate_all_v2(fixture)).unwrap();
    TargetRun {
        run_id,
        source_handle,
        target_handle,
    }
}

fn close_run(target: TargetRun) {
    drop_run(target.run_id).unwrap();
    assert!(plugin_handle_close_native(target.source_handle));
    assert!(plugin_handle_close_native(target.target_handle));
}

fn target_root(target: &TargetRun) -> Vec<ParsedItem> {
    let store = plugin_handle_store_ref().lock().unwrap();
    store
        .get(&target.target_handle)
        .unwrap()
        .parsed
        .root_items
        .clone()
}

fn only_record(items: &[ParsedItem], signature: &str) -> ParsedRecord {
    let mut records = Vec::new();
    collect_records(items, signature, &mut records);
    assert_eq!(
        records.len(),
        1,
        "expected one {signature}, found {}",
        records.len()
    );
    records.pop().unwrap()
}

fn collect_records(items: &[ParsedItem], signature: &str, out: &mut Vec<ParsedRecord>) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == signature => {
                out.push(record.clone())
            }
            ParsedItem::Group(group) => collect_records(&group.children, signature, out),
            _ => {}
        }
    }
}

fn nvnm(record: &ParsedRecord) -> esp_authoring_core::nvnm::NvnmPayload {
    parse_nvnm(
        record
            .subrecords
            .iter()
            .find(|sub| sub.signature.as_str() == "NVNM")
            .unwrap()
            .data
            .as_ref(),
    )
    .unwrap()
}

fn nver(record: &ParsedRecord) -> u32 {
    u32::from_le_bytes(
        record
            .subrecords
            .iter()
            .find(|sub| sub.signature.as_str() == "NVER")
            .unwrap()
            .data[0..4]
            .try_into()
            .unwrap(),
    )
}

fn navi_nvmi_navmesh_ids(record: &ParsedRecord) -> Vec<u32> {
    record
        .subrecords
        .iter()
        .filter(|sub| sub.signature.as_str() == "NVMI")
        .map(|sub| u32::from_le_bytes(sub.data[0..4].try_into().unwrap()))
        .collect()
}

fn navi_nvmi_edge_targets(record: &ParsedRecord) -> Vec<(u32, Vec<u32>)> {
    record
        .subrecords
        .iter()
        .filter(|sub| sub.signature.as_str() == "NVMI")
        .map(|sub| {
            let data = sub.data.as_ref();
            let source = u32::from_le_bytes(data[0..4].try_into().unwrap());
            let edge_count = u32::from_le_bytes(data[24..28].try_into().unwrap()) as usize;
            let edges = data[28..28 + edge_count * 4]
                .chunks_exact(4)
                .map(|edge| u32::from_le_bytes(edge.try_into().unwrap()))
                .collect();
            (source, edges)
        })
        .collect()
}

fn record_is_inside_temporary_cell_group(
    items: &[ParsedItem],
    navmesh_form_id: u32,
    cell_form_id: u32,
) -> bool {
    fn walk(items: &[ParsedItem], navmesh_form_id: u32, cell_form_id: u32) -> bool {
        items.iter().any(|item| {
            match item {
            ParsedItem::Group(group)
                if group.group_type == 9 && u32::from_le_bytes(group.label) == cell_form_id =>
            {
                group.children.iter().any(|child| matches!(
                    child,
                    ParsedItem::Record(record)
                        if record.signature.as_str() == "NAVM" && record.form_id == navmesh_form_id
                ))
            }
            ParsedItem::Group(group) => walk(&group.children, navmesh_form_id, cell_form_id),
            _ => false,
        }
        })
    }
    walk(items, navmesh_form_id, cell_form_id)
}

fn cell_is_nested_under_worldspace(
    items: &[ParsedItem],
    world_form_id: u32,
    cell_form_id: u32,
) -> bool {
    fn contains_cell(items: &[ParsedItem], cell_form_id: u32) -> bool {
        items.iter().any(|item| match item {
            ParsedItem::Record(record) => {
                record.signature.as_str() == "CELL" && record.form_id == cell_form_id
            }
            ParsedItem::Group(group) => contains_cell(&group.children, cell_form_id),
        })
    }

    items.iter().any(|item| match item {
        ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"WRLD" => {
            group.children.iter().any(|child| match child {
                ParsedItem::Group(world_children)
                    if world_children.group_type == 1
                        && u32::from_le_bytes(world_children.label) == world_form_id =>
                {
                    contains_cell(&world_children.children, cell_form_id)
                }
                _ => false,
            })
        }
        _ => false,
    })
}

fn write_plugin(bytes: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new().suffix(".esm").tempfile().unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
    file
}

fn plugin(groups: &[Vec<u8>], extra_top_level: &[Vec<u8>]) -> Vec<u8> {
    let mut hedr = Vec::new();
    hedr.extend_from_slice(&1.0f32.to_le_bytes());
    hedr.extend_from_slice(&0u32.to_le_bytes());
    hedr.extend_from_slice(&0x800u32.to_le_bytes());
    let mut out = record(b"TES4", 0, &subrecord(b"HEDR", &hedr));
    for group in groups {
        out.extend_from_slice(group);
    }
    for record in extra_top_level {
        out.extend_from_slice(record);
    }
    out
}

fn interior_cell_tree(cell_form_id: u32, navmesh: Vec<u8>) -> Vec<u8> {
    let cell = record(b"CELL", cell_form_id, &subrecord(b"DATA", &[1]));
    let temporary = group(&cell_form_id.to_le_bytes(), 9, &[navmesh]);
    let children = group(&cell_form_id.to_le_bytes(), 6, &[temporary]);
    group(b"CELL", 0, &[cell, children])
}

fn exterior_worldspace_tree(
    world_form_id: u32,
    cell_form_id: u32,
    navmeshes: &[Vec<u8>],
) -> Vec<u8> {
    let world = record(
        b"WRLD",
        world_form_id,
        &subrecord(b"EDID", b"NavmeshWorld\0"),
    );
    let mut cell_payload = subrecord(b"DATA", &[0]);
    let mut xclc = Vec::new();
    xclc.extend_from_slice(&0i32.to_le_bytes());
    xclc.extend_from_slice(&0i32.to_le_bytes());
    xclc.extend_from_slice(&0u32.to_le_bytes());
    cell_payload.extend_from_slice(&subrecord(b"XCLC", &xclc));
    let cell = record(b"CELL", cell_form_id, &cell_payload);
    let mut temporary_records = Vec::with_capacity(navmeshes.len() + 1);
    temporary_records.push(anchor_reference());
    temporary_records.extend_from_slice(navmeshes);
    let temporary = group(&cell_form_id.to_le_bytes(), 9, &temporary_records);
    let cell_children = group(&cell_form_id.to_le_bytes(), 6, &[temporary]);
    let subblock = group(&0i32.to_le_bytes(), 5, &[cell, cell_children]);
    let block = group(&0i32.to_le_bytes(), 4, &[subblock]);
    let world_children = group(&world_form_id.to_le_bytes(), 1, &[block]);
    group(b"WRLD", 0, &[world, world_children])
}

fn anchor_activator() -> Vec<u8> {
    group(
        b"ACTI",
        0,
        &[record(
            b"ACTI",
            EXTERIOR_ACTI,
            &subrecord(b"EDID", b"NavmeshAnchor\0"),
        )],
    )
}

fn anchor_reference() -> Vec<u8> {
    let mut payload = subrecord(b"NAME", &EXTERIOR_ACTI.to_le_bytes());
    payload.extend_from_slice(&subrecord(b"DATA", &[0; 24]));
    record(b"REFR", EXTERIOR_REFR, &payload)
}

fn legacy_fallout_navmesh(cell_form_id: u32, navmesh_form_id: u32) -> Vec<u8> {
    let mut data = Vec::new();
    for value in [cell_form_id, 3, 1, 0, 0, 0] {
        data.extend_from_slice(&value.to_le_bytes());
    }

    let mut vertices = Vec::new();
    for vertex in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for component in vertex {
            vertices.extend_from_slice(&component.to_le_bytes());
        }
    }

    let mut triangle = Vec::new();
    for vertex in [0_u16, 1, 2] {
        triangle.extend_from_slice(&vertex.to_le_bytes());
    }
    for link in [-1_i16; 3] {
        triangle.extend_from_slice(&link.to_le_bytes());
    }
    triangle.extend_from_slice(&0u16.to_le_bytes());
    triangle.extend_from_slice(&0u16.to_le_bytes());

    let mut payload = subrecord(b"NVER", &11u32.to_le_bytes());
    payload.extend_from_slice(&subrecord(b"DATA", &data));
    payload.extend_from_slice(&subrecord(b"NVVX", &vertices));
    payload.extend_from_slice(&subrecord(b"NVTR", &triangle));
    record(b"NAVM", navmesh_form_id, &payload)
}

fn legacy_exterior_navmesh(
    cell_form_id: u32,
    navmesh_form_id: u32,
    linked_navmesh_form_id: u32,
    external_slot: usize,
    vertices: [[f32; 3]; 3],
) -> Vec<u8> {
    let mut data = Vec::new();
    for value in [cell_form_id, 3, 1, 1, 0, 0] {
        data.extend_from_slice(&value.to_le_bytes());
    }

    let mut vertex_bytes = Vec::new();
    for vertex in vertices {
        for component in vertex {
            vertex_bytes.extend_from_slice(&component.to_le_bytes());
        }
    }
    let mut triangle = Vec::new();
    for vertex in [0_u16, 1, 2] {
        triangle.extend_from_slice(&vertex.to_le_bytes());
    }
    for slot in 0..3 {
        let link = (slot == external_slot).then_some(0i16).unwrap_or(-1);
        triangle.extend_from_slice(&link.to_le_bytes());
    }
    triangle.extend_from_slice(&(0x0800u16 | (1 << external_slot)).to_le_bytes());
    triangle.extend_from_slice(&0u16.to_le_bytes());

    let mut edge = Vec::new();
    edge.extend_from_slice(&0u32.to_le_bytes());
    edge.extend_from_slice(&linked_navmesh_form_id.to_le_bytes());
    edge.extend_from_slice(&0i16.to_le_bytes());

    let mut payload = subrecord(b"NVER", &11u32.to_le_bytes());
    payload.extend_from_slice(&subrecord(b"DATA", &data));
    payload.extend_from_slice(&subrecord(b"NVVX", &vertex_bytes));
    payload.extend_from_slice(&subrecord(b"NVTR", &triangle));
    payload.extend_from_slice(&subrecord(b"NVEX", &edge));
    record(b"NAVM", navmesh_form_id, &payload)
}

fn legacy_navi(navmesh_form_id: u32, cell_form_id: u32, version: u32) -> Vec<u8> {
    let mut nvmi = Vec::new();
    nvmi.extend_from_slice(&navmesh_form_id.to_le_bytes());
    nvmi.extend_from_slice(&0u32.to_le_bytes());
    for value in [10.0_f32, 20.0, 30.0, 0.0] {
        nvmi.extend_from_slice(&value.to_le_bytes());
    }
    nvmi.extend_from_slice(&1u32.to_le_bytes());
    nvmi.extend_from_slice(&0x00FF_FFFFu32.to_le_bytes());
    nvmi.extend_from_slice(&0u32.to_le_bytes());
    nvmi.extend_from_slice(&0u32.to_le_bytes());
    nvmi.push(0);
    nvmi.extend_from_slice(&0xAABB_CCDDu32.to_le_bytes());
    nvmi.extend_from_slice(&0u32.to_le_bytes());
    nvmi.extend_from_slice(&cell_form_id.to_le_bytes());

    let mut payload = subrecord(b"NVER", &version.to_le_bytes());
    payload.extend_from_slice(&subrecord(b"NVMI", &nvmi));
    payload.extend_from_slice(&subrecord(b"NVPP", &[0xCC; 8]));
    record(b"NAVI", 0x000F_F1, &payload)
}

fn record(signature: &[u8; 4], form_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(signature);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&form_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&44u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn subrecord(signature: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(signature);
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn group(label: &[u8; 4], group_type: i32, children: &[Vec<u8>]) -> Vec<u8> {
    let children_len: usize = children.iter().map(Vec::len).sum();
    let mut out = Vec::new();
    out.extend_from_slice(b"GRUP");
    out.extend_from_slice(&((children_len + 24) as u32).to_le_bytes());
    out.extend_from_slice(label);
    out.extend_from_slice(&group_type.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    for child in children {
        out.extend_from_slice(child);
    }
    out
}
