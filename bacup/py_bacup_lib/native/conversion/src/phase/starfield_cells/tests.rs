use super::*;

use std::sync::atomic::AtomicBool;

use esp_authoring_core::plugin_runtime::{
    ParsedSubrecord, plugin_handle_close_native, plugin_handle_new_native,
};

use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};

// ---------------------------------------------------------------------------
// Fixture ids
// ---------------------------------------------------------------------------

const WRLD_ID: u32 = 0x0000_0050;
const PERSISTENT_CELL_ID: u32 = 0x0000_0060;
const EXT_CELL_A_ID: u32 = 0x0000_0061; // FO4 grid (0, 0)
const EXT_CELL_B_ID: u32 = 0x0000_0062; // FO4 grid (2, 0)
const PERSISTENT_REF_ID: u32 = 0x0000_0070;
const REF_A_ID: u32 = 0x0000_0071;
const REF_B_ID: u32 = 0x0000_0072;
const REF_ORPHAN_BASE_ID: u32 = 0x0000_0073;
const EXT_DOOR_REF_ID: u32 = 0x0000_0074;
const INTERIOR_CELL_ID: u32 = 0x0000_0080;
const INT_DOOR_REF_ID: u32 = 0x0000_0081;
const STAT_BASE_ID: u32 = 0x0000_0010;
const DOOR_BASE_ID: u32 = 0x0000_0011;
const EXCLUDED_BASE_ID: u32 = 0x0000_00FF;

const REF_A_POS: (f32, f32, f32) = (1000.0, 2000.0, 300.0);
const REF_B_POS: (f32, f32, f32) = (10000.0, 2000.0, 0.0);
const REF_A_ROT: (f32, f32, f32) = (0.25, 0.5, 0.75);

fn subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
    ParsedSubrecord {
        signature: SmolStr::new(signature),
        data: Bytes::from(data),
        semantic_type: None,
    }
}

fn zstring(value: &str) -> Vec<u8> {
    let mut out = value.as_bytes().to_vec();
    out.push(0);
    out
}

fn parsed(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
    ParsedRecord {
        signature: SmolStr::new(sig),
        form_id,
        flags: 0,
        version_control: 0,
        form_version: Some(131),
        version2: Some(0),
        subrecords,
        raw_payload: None,
        parse_error: None,
    }
}

fn refr_data(position: (f32, f32, f32), rotation: (f32, f32, f32)) -> Vec<u8> {
    let mut out = Vec::with_capacity(24);
    for value in [
        position.0, position.1, position.2, rotation.0, rotation.1, rotation.2,
    ] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// `struct:I,f,f,f,f,f,f,I,I` — door, position xyz, rotation xyz, flags,
/// transition interior.
fn xtel_bytes(door: u32, position: (f32, f32, f32), transition: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(36);
    out.extend_from_slice(&door.to_le_bytes());
    for value in [position.0, position.1, position.2, 0.0, 0.0, 0.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&transition.to_le_bytes());
    out
}

fn refr(form_id: u32, base: u32, position: (f32, f32, f32)) -> ParsedRecord {
    parsed(
        "REFR",
        form_id,
        vec![
            subrecord("NAME", base.to_le_bytes().to_vec()),
            subrecord("DATA", refr_data(position, (0.0, 0.0, 0.0))),
        ],
    )
}

fn parsed_group(group_type: i32, label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
    ParsedItem::Group(ParsedGroup {
        label,
        group_type,
        tail: Bytes::new(),
        children,
    })
}

fn cell(form_id: u32, editor_id: &str, flags: u16, grid: Option<(i32, i32)>) -> ParsedRecord {
    let mut subrecords = vec![
        subrecord("EDID", zstring(editor_id)),
        subrecord("DATA", flags.to_le_bytes().to_vec()),
    ];
    if let Some((x, y)) = grid {
        subrecords.push(subrecord("XCLC", exterior_grid_payload(x, y)));
    }
    parsed("CELL", form_id, subrecords)
}

/// The whole fo4-side fixture: one worldspace (persistent cell + two exterior
/// cells that collapse onto two different Starfield buckets) plus one interior
/// cell wired to the exterior by an XTEL door pair.
fn source_root_items() -> Vec<ParsedItem> {
    let persistent_ref = refr(PERSISTENT_REF_ID, STAT_BASE_ID, (10.0, 20.0, 30.0));
    let ref_a = {
        let mut record = refr(REF_A_ID, STAT_BASE_ID, REF_A_POS);
        record.subrecords[1] = subrecord("DATA", refr_data(REF_A_POS, REF_A_ROT));
        record
            .subrecords
            .push(subrecord("XSCL", 2.0f32.to_le_bytes().to_vec()));
        record.subrecords.push(subrecord("XPRM", {
            let mut out = Vec::new();
            for value in [128.0f32, 256.0, 512.0, 1.0, 0.5, 0.25, 0.0] {
                out.extend_from_slice(&value.to_le_bytes());
            }
            out.extend_from_slice(&1u32.to_le_bytes());
            out
        }));
        record
    };
    let ext_door = {
        let mut record = refr(EXT_DOOR_REF_ID, DOOR_BASE_ID, (1500.0, 1500.0, 0.0));
        record.subrecords.push(subrecord(
            "XTEL",
            xtel_bytes(INT_DOOR_REF_ID, (700.0, 0.0, 0.0), INTERIOR_CELL_ID),
        ));
        record
    };

    let world_children = parsed_group(
        WORLD_CHILDREN_GROUP,
        WRLD_ID.to_le_bytes(),
        vec![
            ParsedItem::Record(cell(PERSISTENT_CELL_ID, "TestWorldPersistent", 0, None)),
            parsed_group(
                CELL_CHILDREN_GROUP,
                PERSISTENT_CELL_ID.to_le_bytes(),
                vec![parsed_group(
                    CELL_PERSISTENT_GROUP,
                    PERSISTENT_CELL_ID.to_le_bytes(),
                    vec![ParsedItem::Record(persistent_ref)],
                )],
            ),
            parsed_group(
                EXTERIOR_BLOCK_GROUP,
                encode_exterior_grid_label(0, 0),
                vec![parsed_group(
                    EXTERIOR_SUB_BLOCK_GROUP,
                    encode_exterior_grid_label(0, 0),
                    vec![
                        ParsedItem::Record(cell(EXT_CELL_A_ID, "TestWorldCellA", 2, Some((0, 0)))),
                        parsed_group(
                            CELL_CHILDREN_GROUP,
                            EXT_CELL_A_ID.to_le_bytes(),
                            vec![parsed_group(
                                CELL_TEMPORARY_GROUP,
                                EXT_CELL_A_ID.to_le_bytes(),
                                vec![ParsedItem::Record(ref_a), ParsedItem::Record(ext_door)],
                            )],
                        ),
                        ParsedItem::Record(cell(EXT_CELL_B_ID, "TestWorldCellB", 2, Some((2, 0)))),
                        parsed_group(
                            CELL_CHILDREN_GROUP,
                            EXT_CELL_B_ID.to_le_bytes(),
                            vec![parsed_group(
                                CELL_TEMPORARY_GROUP,
                                EXT_CELL_B_ID.to_le_bytes(),
                                vec![
                                    ParsedItem::Record(refr(REF_B_ID, STAT_BASE_ID, REF_B_POS)),
                                    ParsedItem::Record(refr(
                                        REF_ORPHAN_BASE_ID,
                                        EXCLUDED_BASE_ID,
                                        REF_B_POS,
                                    )),
                                ],
                            )],
                        ),
                    ],
                )],
            ),
        ],
    );

    let int_door = {
        let mut record = refr(INT_DOOR_REF_ID, DOOR_BASE_ID, (0.0, 0.0, 0.0));
        record.subrecords.push(subrecord(
            "XTEL",
            xtel_bytes(EXT_DOOR_REF_ID, (0.0, 0.0, 0.0), EXT_CELL_A_ID),
        ));
        record
    };

    vec![
        parsed_group(
            TOP_GROUP,
            *b"WRLD",
            vec![
                ParsedItem::Record(parsed(
                    "WRLD",
                    WRLD_ID,
                    vec![subrecord("EDID", zstring("TestWorld"))],
                )),
                world_children,
            ],
        ),
        parsed_group(
            TOP_GROUP,
            *b"CELL",
            vec![parsed_group(
                INTERIOR_BLOCK_GROUP,
                8i32.to_le_bytes(),
                vec![parsed_group(
                    INTERIOR_SUB_BLOCK_GROUP,
                    2i32.to_le_bytes(),
                    vec![
                        ParsedItem::Record(cell(INTERIOR_CELL_ID, "TestInterior", 1, None)),
                        parsed_group(
                            CELL_CHILDREN_GROUP,
                            INTERIOR_CELL_ID.to_le_bytes(),
                            vec![parsed_group(
                                CELL_TEMPORARY_GROUP,
                                INTERIOR_CELL_ID.to_le_bytes(),
                                vec![ParsedItem::Record(int_door)],
                            )],
                        ),
                    ],
                )],
            )],
        ),
    ]
}

fn target_root_items() -> Vec<ParsedItem> {
    vec![
        parsed_group(
            TOP_GROUP,
            *b"WRLD",
            vec![ParsedItem::Record({
                let mut record = parsed(
                    "WRLD",
                    WRLD_ID,
                    vec![subrecord("EDID", zstring("TestWorld"))],
                );
                record.form_version = Some(581);
                record
            })],
        ),
        parsed_group(
            TOP_GROUP,
            *b"STAT",
            vec![ParsedItem::Record({
                let mut record = parsed(
                    "STAT",
                    STAT_BASE_ID,
                    vec![subrecord("EDID", zstring("TestStat"))],
                );
                record.form_version = Some(581);
                record
            })],
        ),
        parsed_group(
            TOP_GROUP,
            *b"DOOR",
            vec![ParsedItem::Record({
                let mut record = parsed(
                    "DOOR",
                    DOOR_BASE_ID,
                    vec![subrecord("EDID", zstring("TestDoor"))],
                );
                record.form_version = Some(581);
                record
            })],
        ),
    ]
}

fn seed_handle(handle: u64, items: Vec<ParsedItem>) {
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store.get_mut(&handle).unwrap();
    slot.parsed.root_items = items;
    slot.clear_record_count_cache();
    slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
}

struct Fixture {
    source: u64,
    target: u64,
    run_id: u64,
}

impl Fixture {
    fn new(source_game: Game, target_game: Game) -> Self {
        let source = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        let target = plugin_handle_new_native("Fallout4_SF.esm", Some("starfield")).unwrap();
        seed_handle(source, source_root_items());
        seed_handle(target, target_root_items());
        let run_id = create_run(RunParams {
            source: source_game,
            target: target_game,
            source_handle_id: source,
            target_handle_id: target,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Fallout4_SF.esm".into(),
                is_whole_plugin: true,
                preserve_source_ids: true,
                ..Default::default()
            },
        })
        .unwrap();
        Fixture {
            source,
            target,
            run_id,
        }
    }

    fn seed_mapper(&self, with_state: bool) {
        with_run(self.run_id, |run| -> Result<(), RunError> {
            if !with_state {
                run.mapper_state = None;
                return Ok(());
            }
            let mut state = MapperState::new(
                std::iter::empty(),
                MapperOptions {
                    output_plugin_name: "Fallout4_SF.esm".into(),
                    target_master_names: vec![],
                    ..Default::default()
                },
            );
            {
                let mut mapper = FormKeyMapper::from_state(&mut state, &run.interner);
                let source_plugin = run.interner.intern("Fallout4.esm");
                let target_plugin = run.interner.intern("Fallout4_SF.esm");
                for id in [WRLD_ID, STAT_BASE_ID, DOOR_BASE_ID] {
                    mapper.add_mapping(
                        FormKey {
                            local: id,
                            plugin: source_plugin,
                        },
                        FormKey {
                            local: id,
                            plugin: target_plugin,
                        },
                    );
                }
            }
            run.mapper_state = Some(state);
            Ok(())
        })
        .unwrap();
    }

    fn run_phase(&self) -> Result<PhaseReport, PhaseError> {
        let tmp = tempfile::tempdir().unwrap();
        let mod_path = tmp.path().to_path_buf();
        let params = serde_json::json!({});
        with_run(
            self.run_id,
            |run| -> Result<Result<PhaseReport, PhaseError>, RunError> {
                let cancel = AtomicBool::new(false);
                let source_dir = mod_path.clone();
                let mut ctx = PhaseCtx {
                    run,
                    mod_path: &mod_path,
                    source_extracted_dir: &source_dir,
                    target_extracted_dir: None,
                    target_data_dir: None,
                    params: &params,
                    cancel: &cancel,
                };
                Ok(StarfieldCellsPhase.run(&mut ctx))
            },
        )
        .unwrap()
    }

    fn target_items(&self) -> Vec<ParsedItem> {
        let store = plugin_handle_store_ref().lock().unwrap();
        store.get(&self.target).unwrap().parsed.root_items.clone()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = drop_run(self.run_id);
        plugin_handle_close_native(self.source);
        plugin_handle_close_native(self.target);
    }
}

// ---------------------------------------------------------------------------
// Tree-walk helpers for assertions
// ---------------------------------------------------------------------------

fn collect_records<'a>(items: &'a [ParsedItem], sig: &str, out: &mut Vec<&'a ParsedRecord>) {
    for item in items {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == sig => out.push(record),
            ParsedItem::Group(group) => collect_records(&group.children, sig, out),
            _ => {}
        }
    }
}

fn records<'a>(items: &'a [ParsedItem], sig: &str) -> Vec<&'a ParsedRecord> {
    let mut out = Vec::new();
    collect_records(items, sig, &mut out);
    out
}

/// The chain of group types from `items`' root down to the record with
/// `form_id`, e.g. `[0, 1, 4, 5, 6, 9]`.
fn group_path(items: &[ParsedItem], form_id: u32) -> Option<Vec<i32>> {
    fn walk(items: &[ParsedItem], form_id: u32, path: &mut Vec<i32>) -> bool {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id == form_id => return true,
                ParsedItem::Group(group) => {
                    path.push(group.group_type);
                    if walk(&group.children, form_id, path) {
                        return true;
                    }
                    path.pop();
                }
                _ => {}
            }
        }
        false
    }
    let mut path = Vec::new();
    walk(items, form_id, &mut path).then_some(path)
}

fn sub_data<'a>(record: &'a ParsedRecord, sig: &str) -> Option<&'a [u8]> {
    find_subrecord(record, sig)
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

#[test]
fn exterior_grid_label_puts_y_in_the_low_half() {
    // Measured on Starfield.esm (738/738 exterior cells): the label is
    // [y_i16, x_i16] little-endian, exactly as FO4 lays it out.
    assert_eq!(encode_exterior_grid_label(3, -2), [0xFE, 0xFF, 0x03, 0x00]);
}

#[test]
fn interior_buckets_match_the_measured_starfield_convention() {
    // 11985/11985 vanilla Starfield interior cells satisfy this.
    assert_eq!(interior_bucket_indices(0x0000_0080), (8, 2));
    assert_eq!(interior_bucket_indices(0x0000_007B), (3, 2));
}

#[test]
fn fo4_cells_collapse_onto_the_hundred_metre_starfield_lattice() {
    // 1 FO4 cell = 4096 units = 58.52 m; 1 SF cell = 100 m. FO4 cells 0 and 1
    // share SF cell 0; FO4 cell 2's centre is the first past 100 m.
    assert_eq!(home_grid(Some((0, 0))), (0, 0));
    assert_eq!(home_grid(Some((1, 0))), (0, 0));
    assert_eq!(home_grid(Some((2, 0))), (1, 0));
    assert_eq!(home_grid(Some((-1, 0))), (-1, 0));
}

#[test]
fn placed_ref_rescale_touches_only_world_distances() {
    let interner = crate::sym::StringInterner::new();
    let plugin = interner.intern("Fallout4.esm");
    let mut record = Record::new(
        SigCode(*b"REFR"),
        FormKey {
            local: REF_A_ID,
            plugin,
        },
    );
    record.fields.push(FieldEntry {
        sig: sub("DATA"),
        value: FieldValue::Bytes(SmallVec::from_vec(refr_data(REF_A_POS, REF_A_ROT))),
    });
    record.fields.push(FieldEntry {
        sig: sub("XSCL"),
        value: FieldValue::Float(2.0),
    });
    record.fields.push(FieldEntry {
        sig: sub("XRDS"),
        value: FieldValue::Float(4096.0),
    });
    record.fields.push(FieldEntry {
        sig: sub("XTEL"),
        value: FieldValue::Bytes(SmallVec::from_vec(xtel_bytes(
            INT_DOOR_REF_ID,
            (700.0, 0.0, 0.0),
            INTERIOR_CELL_ID,
        ))),
    });
    record.fields.push(FieldEntry {
        sig: sub("XPRM"),
        value: FieldValue::Bytes(SmallVec::from_vec({
            let mut out = Vec::new();
            for value in [128.0f32, 256.0, 512.0, 1.0, 0.5, 0.25, 0.0] {
                out.extend_from_slice(&value.to_le_bytes());
            }
            out.extend_from_slice(&7u32.to_le_bytes());
            out
        })),
    });

    rescale_placed_ref(&mut record);

    let data = field_bytes(&record, &sub("DATA")).unwrap();
    assert!((f32_at(data, 0) - REF_A_POS.0 * FO4_TO_SF_SPATIAL).abs() < 1e-6);
    assert!((f32_at(data, 4) - REF_A_POS.1 * FO4_TO_SF_SPATIAL).abs() < 1e-6);
    assert!((f32_at(data, 8) - REF_A_POS.2 * FO4_TO_SF_SPATIAL).abs() < 1e-6);
    // Rotations are radians — carried verbatim.
    assert_eq!(f32_at(data, 12), REF_A_ROT.0);
    assert_eq!(f32_at(data, 16), REF_A_ROT.1);
    assert_eq!(f32_at(data, 20), REF_A_ROT.2);

    // XSCL is dimensionless.
    assert!(matches!(
        record
            .fields
            .iter()
            .find(|entry| entry.sig == sub("XSCL"))
            .map(|entry| &entry.value),
        Some(FieldValue::Float(value)) if (*value - 2.0).abs() < 1e-6
    ));
    // XRDS is a world distance.
    assert!(matches!(
        record
            .fields
            .iter()
            .find(|entry| entry.sig == sub("XRDS"))
            .map(|entry| &entry.value),
        Some(FieldValue::Float(value)) if (*value - 4096.0 * FO4_TO_SF_SPATIAL).abs() < 1e-4
    ));

    let xtel = field_bytes(&record, &sub("XTEL")).unwrap();
    assert_eq!(
        u32::from_le_bytes(xtel[0..4].try_into().unwrap()),
        INT_DOOR_REF_ID
    );
    assert!((f32_at(xtel, 4) - 700.0 * FO4_TO_SF_SPATIAL).abs() < 1e-6);
    assert_eq!(f32_at(xtel, 16), 0.0);
    assert_eq!(
        u32::from_le_bytes(xtel[32..36].try_into().unwrap()),
        INTERIOR_CELL_ID
    );

    let xprm = field_bytes(&record, &sub("XPRM")).unwrap();
    assert!((f32_at(xprm, 0) - 128.0 * FO4_TO_SF_SPATIAL).abs() < 1e-6);
    assert!((f32_at(xprm, 8) - 512.0 * FO4_TO_SF_SPATIAL).abs() < 1e-6);
    // Colour + type are not distances.
    assert_eq!(f32_at(xprm, 12), 1.0);
    assert_eq!(u32::from_le_bytes(xprm[28..32].try_into().unwrap()), 7);
}

#[test]
fn xtel_with_an_unresolvable_door_is_reported_dangling() {
    let mut raw_map = HashMap::new();
    raw_map.insert(INT_DOOR_REF_ID, 0x0000_0901);
    raw_map.insert(INTERIOR_CELL_ID, 0x0000_0902);
    let emitted: HashSet<u32> = [0x0000_0902u32].into_iter().collect();

    let mut resolved = xtel_bytes(INT_DOOR_REF_ID, (0.0, 0.0, 0.0), INTERIOR_CELL_ID);
    assert!(remap_xtel_bytes(&mut resolved, &raw_map, &emitted));
    assert_eq!(
        u32::from_le_bytes(resolved[0..4].try_into().unwrap()),
        0x0000_0901
    );
    assert_eq!(
        u32::from_le_bytes(resolved[32..36].try_into().unwrap()),
        0x0000_0902
    );

    let mut dangling = xtel_bytes(0x0000_0BAD, (0.0, 0.0, 0.0), INTERIOR_CELL_ID);
    assert!(!remap_xtel_bytes(&mut dangling, &raw_map, &emitted));
}

#[test]
fn xtel_transition_at_a_collapsed_cell_is_zeroed_not_left_dangling() {
    // The mapper hands back a target id for every source cell it allocated,
    // including cells that collapsed into a neighbouring 100 m bucket and were
    // never written. Resolvability alone is not enough.
    let mut raw_map = HashMap::new();
    raw_map.insert(INT_DOOR_REF_ID, 0x0000_0901);
    raw_map.insert(EXT_CELL_B_ID, 0x0000_0903);
    let emitted: HashSet<u32> = [0x0000_0901u32].into_iter().collect();

    let mut bytes = xtel_bytes(INT_DOOR_REF_ID, (0.0, 0.0, 0.0), EXT_CELL_B_ID);
    assert!(remap_xtel_bytes(&mut bytes, &raw_map, &emitted));
    assert_eq!(u32::from_le_bytes(bytes[32..36].try_into().unwrap()), 0);
}

#[test]
fn source_plan_collection_finds_both_worldspace_and_interior_cells() {
    let plans = collect_source_plans(&source_root_items());
    assert_eq!(plans.worlds.len(), 1);
    let world = &plans.worlds[0];
    assert_eq!(world.world_form_id, WRLD_ID);
    assert_eq!(world.persistent_cells.len(), 1);
    assert_eq!(world.persistent_cells[0].form_id, PERSISTENT_CELL_ID);
    assert_eq!(world.lattice_cells.len(), 2);
    assert_eq!(world.lattice_cells[0].grid, Some((0, 0)));
    assert_eq!(world.lattice_cells[1].grid, Some((2, 0)));
    // Cell B holds the in-scope ref plus the one whose base is fenced out.
    assert_eq!(world.lattice_cells[1].sections.len(), 1);
    assert_eq!(world.lattice_cells[1].sections[0].children.len(), 2);
    assert_eq!(plans.interiors.len(), 1);
    assert_eq!(plans.interiors[0].form_id, INTERIOR_CELL_ID);
}

#[test]
fn collection_tolerates_a_block_group_with_no_sub_block_level() {
    // Hand-built fixture plugins (the Python vertical slice among them) nest
    // exterior cells straight under the block group. A real FO4 plugin always
    // has the sub-block level, but losing every cell on the flattened shape
    // would be a silent total content drop.
    let items = vec![parsed_group(
        TOP_GROUP,
        *b"WRLD",
        vec![
            ParsedItem::Record(parsed(
                "WRLD",
                WRLD_ID,
                vec![subrecord("EDID", zstring("TestWorld"))],
            )),
            parsed_group(
                WORLD_CHILDREN_GROUP,
                WRLD_ID.to_le_bytes(),
                vec![parsed_group(
                    EXTERIOR_BLOCK_GROUP,
                    encode_exterior_grid_label(0, 0),
                    vec![
                        ParsedItem::Record(cell(EXT_CELL_A_ID, "Flat", 2, Some((0, 0)))),
                        parsed_group(
                            CELL_CHILDREN_GROUP,
                            EXT_CELL_A_ID.to_le_bytes(),
                            vec![parsed_group(
                                CELL_TEMPORARY_GROUP,
                                EXT_CELL_A_ID.to_le_bytes(),
                                vec![ParsedItem::Record(refr(REF_A_ID, STAT_BASE_ID, REF_A_POS))],
                            )],
                        ),
                    ],
                )],
            ),
        ],
    )];
    let plans = collect_source_plans(&items);
    assert_eq!(plans.worlds.len(), 1);
    assert_eq!(plans.worlds[0].lattice_cells.len(), 1);
    assert_eq!(plans.worlds[0].lattice_cells[0].sections.len(), 1);
    assert!(plans.worlds[0].persistent_cells.is_empty());
}

#[test]
fn a_gridded_cell_directly_under_world_children_is_lattice_not_persistent() {
    let items = vec![parsed_group(
        TOP_GROUP,
        *b"WRLD",
        vec![
            ParsedItem::Record(parsed(
                "WRLD",
                WRLD_ID,
                vec![subrecord("EDID", zstring("TestWorld"))],
            )),
            parsed_group(
                WORLD_CHILDREN_GROUP,
                WRLD_ID.to_le_bytes(),
                vec![
                    ParsedItem::Record(cell(PERSISTENT_CELL_ID, "Persistent", 0, None)),
                    ParsedItem::Record(cell(EXT_CELL_A_ID, "Stray", 2, Some((0, 0)))),
                ],
            ),
        ],
    )];
    let plans = collect_source_plans(&items);
    assert_eq!(plans.worlds[0].persistent_cells.len(), 1);
    assert_eq!(
        plans.worlds[0].persistent_cells[0].form_id,
        PERSISTENT_CELL_ID
    );
    assert_eq!(plans.worlds[0].lattice_cells.len(), 1);
    assert_eq!(plans.worlds[0].lattice_cells[0].form_id, EXT_CELL_A_ID);
}

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

#[test]
fn phase_is_inert_for_other_pairs() {
    let fixture = Fixture::new(Game::Fo76, Game::Fo4);
    fixture.seed_mapper(true);
    let report = fixture.run_phase().unwrap();
    assert_eq!(report.records_added, 0);
    assert!(records(&fixture.target_items(), "CELL").is_empty());
}

#[test]
fn phase_hard_errors_without_mapper_state() {
    let fixture = Fixture::new(Game::Fo4, Game::Starfield);
    fixture.seed_mapper(false);
    match fixture.run_phase() {
        Err(PhaseError::Internal(message)) => {
            assert!(message.contains("mapper_state"), "{message}");
        }
        other => panic!("expected Internal error, got {other:?}"),
    }
}

#[test]
fn phase_emits_the_vanilla_worldspace_and_interior_topology() {
    let fixture = Fixture::new(Game::Fo4, Game::Starfield);
    fixture.seed_mapper(true);
    let report = fixture.run_phase().unwrap();
    assert!(report.records_added > 0, "{report:?}");
    let items = fixture.target_items();

    // Exterior cells: the two FO4 cells sit in different 100 m buckets, so
    // both survive as their own Starfield CELL.
    let cells = records(&items, "CELL");
    let with_grid: Vec<_> = cells
        .iter()
        .filter(|record| find_subrecord(record, "XCLC").is_some())
        .collect();
    assert_eq!(with_grid.len(), 2, "expected two exterior buckets");
    let grids: Vec<(i32, i32)> = with_grid
        .iter()
        .map(|record| {
            let data = find_subrecord(record, "XCLC").unwrap();
            (
                i32::from_le_bytes(data[0..4].try_into().unwrap()),
                i32::from_le_bytes(data[4..8].try_into().unwrap()),
            )
        })
        .collect();
    assert!(
        grids.contains(&(0, 0)) && grids.contains(&(1, 0)),
        "{grids:?}"
    );
    for record in &with_grid {
        assert_eq!(find_subrecord(record, "XCLC").unwrap().len(), 12);
        // Vanilla Starfield exteriors are uniformly DATA == 2 (uint32).
        assert_eq!(find_subrecord(record, "DATA").unwrap(), &2u32.to_le_bytes());
    }

    // Topology: world(0) -> world children(1) -> block(4) -> sub-block(5) ->
    // CELL, and the placed ref under cell children(6) -> temporary(9).
    let ext_cell_a = with_grid
        .iter()
        .find(|record| {
            let data = find_subrecord(record, "XCLC").unwrap();
            i32::from_le_bytes(data[0..4].try_into().unwrap()) == 0
        })
        .expect("bucket (0,0)");
    assert_eq!(
        group_path(&items, ext_cell_a.form_id).unwrap(),
        vec![
            TOP_GROUP,
            WORLD_CHILDREN_GROUP,
            EXTERIOR_BLOCK_GROUP,
            EXTERIOR_SUB_BLOCK_GROUP
        ]
    );

    let placed = records(&items, "REFR");
    let ref_a = placed
        .iter()
        .find(|record| {
            find_subrecord(record, "XSCL").is_some() && find_subrecord(record, "XPRM").is_some()
        })
        .expect("ref A");
    assert_eq!(
        group_path(&items, ref_a.form_id).unwrap(),
        vec![
            TOP_GROUP,
            WORLD_CHILDREN_GROUP,
            EXTERIOR_BLOCK_GROUP,
            EXTERIOR_SUB_BLOCK_GROUP,
            CELL_CHILDREN_GROUP,
            CELL_TEMPORARY_GROUP
        ]
    );
    let data = sub_data(ref_a, "DATA").unwrap();
    assert!((f32_at(data, 0) - REF_A_POS.0 * FO4_TO_SF_SPATIAL).abs() < 1e-5);
    assert!((f32_at(data, 4) - REF_A_POS.1 * FO4_TO_SF_SPATIAL).abs() < 1e-5);
    assert_eq!(f32_at(data, 12), REF_A_ROT.0);

    // Persistent cell keeps its own children under section 8.
    let persistent_ref = placed
        .iter()
        .find(|record| {
            group_path(&items, record.form_id)
                .is_some_and(|path| path.contains(&CELL_PERSISTENT_GROUP))
        })
        .expect("persistent ref");
    assert_eq!(
        group_path(&items, persistent_ref.form_id).unwrap(),
        vec![
            TOP_GROUP,
            WORLD_CHILDREN_GROUP,
            CELL_CHILDREN_GROUP,
            CELL_PERSISTENT_GROUP
        ]
    );

    // Interior cell: top CELL group -> block(2) -> sub-block(3).
    let interior = cells
        .iter()
        .find(|record| {
            group_path(&items, record.form_id)
                .is_some_and(|path| path.contains(&INTERIOR_BLOCK_GROUP))
        })
        .expect("interior cell");
    assert_eq!(
        group_path(&items, interior.form_id).unwrap(),
        vec![TOP_GROUP, INTERIOR_BLOCK_GROUP, INTERIOR_SUB_BLOCK_GROUP]
    );
    let interior_flags = u32::from_le_bytes(
        find_subrecord(interior, "DATA").unwrap()[0..4]
            .try_into()
            .unwrap(),
    );
    assert_eq!(
        interior_flags & 1,
        1,
        "interior bit must survive DATA widening"
    );

    // Placed ref whose base was fenced out never reaches the target.
    assert_eq!(report.records_dropped, 1, "{report:?}");
}

#[test]
fn phase_resolves_both_ends_of_the_xtel_door_graph() {
    let fixture = Fixture::new(Game::Fo4, Game::Starfield);
    fixture.seed_mapper(true);
    fixture.run_phase().unwrap();
    let items = fixture.target_items();

    let doors: Vec<&ParsedRecord> = records(&items, "REFR")
        .into_iter()
        .filter(|record| find_subrecord(record, "XTEL").is_some())
        .collect();
    assert_eq!(doors.len(), 2, "both halves of the door pair must survive");

    let interior_door = doors
        .iter()
        .find(|record| {
            group_path(&items, record.form_id)
                .is_some_and(|path| path.contains(&INTERIOR_BLOCK_GROUP))
        })
        .expect("interior door");
    let exterior_door = doors
        .iter()
        .find(|record| record.form_id != interior_door.form_id)
        .expect("exterior door");

    let exterior_xtel = find_subrecord(exterior_door, "XTEL").unwrap();
    let interior_xtel = find_subrecord(interior_door, "XTEL").unwrap();
    assert_eq!(
        u32::from_le_bytes(exterior_xtel[0..4].try_into().unwrap()),
        interior_door.form_id,
        "exterior door must point at the emitted interior door"
    );
    assert_eq!(
        u32::from_le_bytes(interior_xtel[0..4].try_into().unwrap()),
        exterior_door.form_id,
        "interior door must point back at the emitted exterior door"
    );
    // The destination offset is a world distance and must be rescaled.
    assert!((f32_at(exterior_xtel, 4) - 700.0 * FO4_TO_SF_SPATIAL).abs() < 1e-5);

    // Both transition-interior slots must resolve to a CELL that exists.
    let cell_ids: Vec<u32> = records(&items, "CELL")
        .iter()
        .map(|record| record.form_id)
        .collect();
    let transition = u32::from_le_bytes(exterior_xtel[32..36].try_into().unwrap());
    assert!(
        transition == 0 || cell_ids.contains(&transition),
        "dangling XTEL transition cell {transition:06X}"
    );
}

#[test]
fn phase_is_idempotent_for_an_already_nested_world() {
    let fixture = Fixture::new(Game::Fo4, Game::Starfield);
    fixture.seed_mapper(true);
    fixture.run_phase().unwrap();
    let first = records(&fixture.target_items(), "CELL").len();
    let report = fixture.run_phase().unwrap();
    let second = records(&fixture.target_items(), "CELL").len();
    assert!(
        report.records_added <= (second - first) as u32 + report.records_added,
        "sanity"
    );
    // The worldspace is skipped the second time; only the interior branch may
    // re-add, so the exterior cell count must not have doubled.
    let grids = records(&fixture.target_items(), "CELL")
        .into_iter()
        .filter(|record| find_subrecord(record, "XCLC").is_some())
        .count();
    assert_eq!(grids, 2, "exterior lattice must not be duplicated");
}
