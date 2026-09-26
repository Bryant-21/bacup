use std::io::Write;

use conversion_native::run::{RunConfig, RunParams, create_run, drop_run, with_run};
use conversion_native::translator::Game;
use esp_authoring_core::nvnm::{NvnmParent, parse_nvnm};
use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, plugin_handle_close_native, plugin_handle_load_no_py,
    plugin_handle_new_no_py, plugin_handle_store_ref,
};

const SKYRIM_CELL: u32 = 0x0001_3A7E;
const SKYRIM_NAVM: u32 = 0x000E_537D;

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
