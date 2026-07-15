//! Pure `(world_objid, grid_x, grid_y) -> cell_objid` index over a parsed tree,
//! plus (when requested) the set of every record object-id and every `LCTN` id
//! in the target.
//!
//! Applied to the TARGET tree post-copy (exterior CELLs are present only after
//! the cell-slice copy). World-children groups have `group_type == 1` and a
//! `label` that is the parent WRLD form-id (LE); exterior CELL grid lives in the
//! first 8 bytes of `XCLC` (gridX i32 @0, gridY i32 @4).
//!
//! The id sets feed ECZN allocation: reserving every target id stops a
//! synthesized ECZN from stealing terrain/cell IDs in whole-plugin runs and
//! preserved LCTN IDs in slice (`identity_resolve`) runs. The `LCTN` id set lets
//! synthesis resolve `DATA.location` by preserved-id identity when the run has
//! no `translate_all` mapper state.

use esp_authoring_core::plugin_runtime::ParsedItem;
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use std::collections::HashMap;

const WORLD_CHILD_GROUP: i32 = 1;
const CELL_CHILD_GROUP: i32 = 6;
const PARALLEL_ITEM_THRESHOLD: usize = 64;
pub type CellGridIndex = HashMap<(u32, i32, i32), u32>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedActorRef {
    pub ref_objid: u32,
    pub cell_objid: u32,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TargetIndex {
    pub grid: CellGridIndex,
    /// Every output-plugin record object-id present in the target tree.
    pub all_object_ids: FxHashSet<u32>,
    /// Object-ids of `LCTN` records present in the target tree.
    pub lctn_object_ids: FxHashSet<u32>,
    /// Interior CELLs (not under a WRLD world-children group): `(cell_objid,
    /// xlcn_objid)`. `xlcn_objid` is the cell's `XLCN` (Persist Location → LCTN)
    /// masked to 24 bits, or `None` when the cell has no `XLCN`.
    pub interior_cells: Vec<(u32, Option<u32>)>,
    /// Target object-ids named by some placed record's `XEZN` (REFR/ACHR/PGRE/
    /// PHZD/PGRD). FO76 placed refs carry `XEZN`→LCTN; post-copy the formid is
    /// already target-remapped, so this is the target LCTN object-id (24-bit) the
    /// ref's encounter zone points at. A non-null entry that lacks a synthesized
    /// ECZN is what the repoint safety net strips.
    pub placed_xezn_targets: FxHashSet<u32>,
    /// Object-ids of the placed records that actually carry a non-null `XEZN`.
    /// The repoint pass decodes ONLY these instead of every placed record in the
    /// worldspace — found by the same single raw walk that builds the index.
    pub placed_xezn_ref_objids: Vec<u32>,
    /// Target CELL object-id -> target LCTN object-id from `XLCN`.
    pub cell_locations: HashMap<u32, u32>,
    /// Placed ACHR records paired with their owning target CELL object-id.
    pub placed_actors: Vec<PlacedActorRef>,
}

impl TargetIndex {
    fn merge(&mut self, other: Self) {
        for (key, cell) in other.grid {
            self.grid.entry(key).or_insert(cell);
        }
        self.all_object_ids.extend(other.all_object_ids);
        self.lctn_object_ids.extend(other.lctn_object_ids);
        self.interior_cells.extend(other.interior_cells);
        self.placed_xezn_targets.extend(other.placed_xezn_targets);
        self.placed_xezn_ref_objids
            .extend(other.placed_xezn_ref_objids);
        self.cell_locations.extend(other.cell_locations);
        self.placed_actors.extend(other.placed_actors);
    }
}

/// Placed-record signatures that carry an `XEZN`→encounter-zone pointer.
pub const PLACED_SIGNATURES: [&str; 5] = ["REFR", "ACHR", "PGRE", "PHZD", "PGRD"];

fn grid_from_xclc(d: &[u8]) -> Option<(i32, i32)> {
    if d.len() < 8 {
        return None;
    }
    Some((
        i32::from_le_bytes([d[0], d[1], d[2], d[3]]),
        i32::from_le_bytes([d[4], d[5], d[6], d[7]]),
    ))
}

fn formid_from_first4(d: &[u8]) -> Option<u32> {
    if d.len() < 4 {
        return None;
    }
    Some(u32::from_le_bytes([d[0], d[1], d[2], d[3]]) & 0x00FF_FFFF)
}

fn walk(
    items: &[ParsedItem],
    world: Option<u32>,
    cell: Option<u32>,
    collect_ids: bool,
    out: &mut TargetIndex,
) {
    for item in items {
        match item {
            ParsedItem::Group(g) => {
                let w = if g.group_type == WORLD_CHILD_GROUP {
                    Some(u32::from_le_bytes(g.label) & 0x00FF_FFFF)
                } else {
                    world
                };
                let c = if g.group_type == CELL_CHILD_GROUP {
                    Some(u32::from_le_bytes(g.label) & 0x00FF_FFFF)
                } else {
                    cell
                };
                walk(&g.children, w, c, collect_ids, out);
            }
            ParsedItem::Record(r) => {
                let objid = r.form_id & 0x00FF_FFFF;
                if collect_ids {
                    out.all_object_ids.insert(objid);
                    if r.signature.as_str() == "LCTN" {
                        out.lctn_object_ids.insert(objid);
                    }
                }
                if PLACED_SIGNATURES.contains(&r.signature.as_str()) {
                    if let Some(target) = r
                        .subrecords
                        .iter()
                        .find(|s| s.signature.as_str() == "XEZN")
                        .and_then(|s| formid_from_first4(&s.data))
                    {
                        if target != 0 {
                            out.placed_xezn_targets.insert(target);
                            out.placed_xezn_ref_objids.push(objid);
                        }
                    }
                }
                if r.signature.as_str() == "ACHR" {
                    if let Some(cell_objid) = cell {
                        out.placed_actors.push(PlacedActorRef {
                            ref_objid: objid,
                            cell_objid,
                        });
                    }
                }
                if r.signature.as_str() == "CELL" {
                    let xlcn = r
                        .subrecords
                        .iter()
                        .find(|s| s.signature.as_str() == "XLCN")
                        .and_then(|s| formid_from_first4(&s.data));
                    if let Some(loc) = xlcn.filter(|loc| *loc != 0) {
                        out.cell_locations.insert(objid, loc);
                    }
                    match world {
                        Some(w) => {
                            if let Some(sub) =
                                r.subrecords.iter().find(|s| s.signature.as_str() == "XCLC")
                            {
                                if let Some((gx, gy)) = grid_from_xclc(&sub.data) {
                                    out.grid.entry((w, gx, gy)).or_insert(objid);
                                }
                            }
                        }
                        // Not under a WRLD world-children group → interior CELL.
                        // Its Location lives on the cell itself (XLCN).
                        None => {
                            out.interior_cells.push((objid, xlcn));
                        }
                    }
                }
            }
        }
    }
}

fn walk_parallel(
    items: &[ParsedItem],
    world: Option<u32>,
    cell: Option<u32>,
    collect_ids: bool,
) -> TargetIndex {
    if items.len() < PARALLEL_ITEM_THRESHOLD {
        let mut out = TargetIndex::default();
        for item in items {
            match item {
                ParsedItem::Group(group) => {
                    let child_world = if group.group_type == WORLD_CHILD_GROUP {
                        Some(u32::from_le_bytes(group.label) & 0x00FF_FFFF)
                    } else {
                        world
                    };
                    let child_cell = if group.group_type == CELL_CHILD_GROUP {
                        Some(u32::from_le_bytes(group.label) & 0x00FF_FFFF)
                    } else {
                        cell
                    };
                    out.merge(walk_parallel(
                        &group.children,
                        child_world,
                        child_cell,
                        collect_ids,
                    ));
                }
                ParsedItem::Record(_) => {
                    walk(
                        std::slice::from_ref(item),
                        world,
                        cell,
                        collect_ids,
                        &mut out,
                    );
                }
            }
        }
        return out;
    }

    let task_count = rayon::current_num_threads().saturating_mul(4).max(1);
    let chunk_size = items.len().div_ceil(task_count);
    let partials: Vec<TargetIndex> = items
        .par_chunks(chunk_size)
        .map(|chunk| {
            let mut out = TargetIndex::default();
            for item in chunk {
                match item {
                    ParsedItem::Group(group) => {
                        let child_world = if group.group_type == WORLD_CHILD_GROUP {
                            Some(u32::from_le_bytes(group.label) & 0x00FF_FFFF)
                        } else {
                            world
                        };
                        let child_cell = if group.group_type == CELL_CHILD_GROUP {
                            Some(u32::from_le_bytes(group.label) & 0x00FF_FFFF)
                        } else {
                            cell
                        };
                        out.merge(walk_parallel(
                            &group.children,
                            child_world,
                            child_cell,
                            collect_ids,
                        ));
                    }
                    ParsedItem::Record(_) => {
                        walk(
                            std::slice::from_ref(item),
                            world,
                            cell,
                            collect_ids,
                            &mut out,
                        );
                    }
                }
            }
            out
        })
        .collect();

    let mut out = TargetIndex::default();
    for partial in partials {
        out.merge(partial);
    }
    out
}

/// Build the target index. `collect_ids` gates object-id sets for callers that
/// only need the cell grid or placed-XEZN scans.
pub fn build_target_index(root_items: &[ParsedItem], collect_ids: bool) -> TargetIndex {
    walk_parallel(root_items, None, None, collect_ids)
}

pub fn build_cell_grid_index(root_items: &[ParsedItem]) -> CellGridIndex {
    build_target_index(root_items, false).grid
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{ParsedGroup, ParsedRecord, ParsedSubrecord};
    use smol_str::SmolStr;

    fn cell(form_id: u32, gx: i32, gy: i32) -> ParsedItem {
        let mut data = Vec::new();
        data.extend_from_slice(&gx.to_le_bytes());
        data.extend_from_slice(&gy.to_le_bytes());
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("CELL"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords: vec![ParsedSubrecord {
                signature: SmolStr::new("XCLC"),
                data: Bytes::from(data),
                semantic_type: None,
            }],
            raw_payload: None,
            parse_error: None,
        })
    }
    fn interior_cell(form_id: u32, xlcn: Option<u32>) -> ParsedItem {
        let mut subrecords = vec![ParsedSubrecord {
            signature: SmolStr::new("DATA"),
            data: Bytes::from(vec![0x01u8, 0x00]),
            semantic_type: None,
        }];
        if let Some(loc) = xlcn {
            subrecords.push(ParsedSubrecord {
                signature: SmolStr::new("XLCN"),
                data: Bytes::from(loc.to_le_bytes().to_vec()),
                semantic_type: None,
            });
        }
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("CELL"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        })
    }
    fn bare(sig: &str, form_id: u32) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new(sig),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords: vec![],
            raw_payload: None,
            parse_error: None,
        })
    }
    fn group(group_type: i32, label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label,
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    #[test]
    fn indexes_cell_by_world_grid() {
        let world: u32 = 0x25DA15;
        let tree = vec![group(
            0,
            *b"WRLD",
            vec![group(
                WORLD_CHILD_GROUP,
                world.to_le_bytes(),
                vec![cell(0x00ABCD, -2, -16)],
            )],
        )];
        let idx = build_cell_grid_index(&tree);
        assert_eq!(idx.get(&(world, -2, -16)).copied(), Some(0x00ABCD));
    }

    #[test]
    fn collect_ids_gathers_all_and_lctn_ids() {
        let world: u32 = 0x25DA15;
        let tree = vec![
            group(
                0,
                *b"LCTN",
                vec![bare("LCTN", 0x01063DC7), bare("LCTN", 0x010989F5)],
            ),
            group(
                0,
                *b"WRLD",
                vec![group(
                    WORLD_CHILD_GROUP,
                    world.to_le_bytes(),
                    vec![cell(0x0100ABCD, -2, -16)],
                )],
            ),
        ];
        let idx = build_target_index(&tree, true);
        // object-ids are masked to 24 bits
        assert!(idx.all_object_ids.contains(&0x063DC7));
        assert!(idx.all_object_ids.contains(&0x0989F5));
        assert!(idx.all_object_ids.contains(&0x00ABCD));
        assert_eq!(idx.lctn_object_ids.len(), 2);
        assert!(idx.lctn_object_ids.contains(&0x063DC7));
        assert!(idx.lctn_object_ids.contains(&0x0989F5));
        assert!(!idx.lctn_object_ids.contains(&0x00ABCD));
        assert_eq!(idx.grid.get(&(world, -2, -16)).copied(), Some(0x00ABCD));
    }

    #[test]
    fn interior_cells_collected_with_xlcn_not_exterior() {
        let world: u32 = 0x25DA15;
        let tree = vec![
            // Interior CELL top-group: Block(2)/Sub-Block(3) topology.
            group(
                0,
                *b"CELL",
                vec![group(
                    2,
                    0i32.to_le_bytes(),
                    vec![group(
                        3,
                        9i32.to_le_bytes(),
                        vec![
                            interior_cell(0x01275EDE, Some(0x010989F5)),
                            interior_cell(0x01275EDF, None),
                        ],
                    )],
                )],
            ),
            // Exterior CELL under a WRLD world-children group — must NOT be
            // collected as interior.
            group(
                0,
                *b"WRLD",
                vec![group(
                    WORLD_CHILD_GROUP,
                    world.to_le_bytes(),
                    vec![cell(0x0100ABCD, -2, -16)],
                )],
            ),
        ];
        let idx = build_target_index(&tree, false);
        assert_eq!(idx.interior_cells.len(), 2, "two interior cells collected");
        assert!(
            idx.interior_cells.contains(&(0x275EDE, Some(0x0989F5))),
            "interior cell with XLCN (masked to 24 bits)"
        );
        assert!(
            idx.interior_cells.contains(&(0x275EDF, None)),
            "interior cell without XLCN"
        );
        assert!(
            !idx.interior_cells.iter().any(|(c, _)| *c == 0x00ABCD),
            "exterior cell is not interior"
        );
    }

    fn placed_with_xezn(sig: &str, form_id: u32, xezn: Option<u32>) -> ParsedItem {
        let mut subrecords = vec![ParsedSubrecord {
            signature: SmolStr::new("NAME"),
            data: Bytes::from(vec![0u8; 4]),
            semantic_type: None,
        }];
        if let Some(t) = xezn {
            subrecords.push(ParsedSubrecord {
                signature: SmolStr::new("XEZN"),
                data: Bytes::from(t.to_le_bytes().to_vec()),
                semantic_type: None,
            });
        }
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new(sig),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        })
    }

    fn placed_actor(form_id: u32) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("ACHR"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords: vec![ParsedSubrecord {
                signature: SmolStr::new("NAME"),
                data: Bytes::from(vec![0u8; 4]),
                semantic_type: None,
            }],
            raw_payload: None,
            parse_error: None,
        })
    }

    #[test]
    fn placed_xezn_targets_collected_masked() {
        let tree = vec![group(
            0,
            *b"CELL",
            vec![group(
                2,
                0i32.to_le_bytes(),
                vec![group(
                    3,
                    9i32.to_le_bytes(),
                    vec![
                        // XEZN→ LCTN 0x010989F5 (master byte 0x01) → masked 0x0989F5
                        placed_with_xezn("REFR", 0x01300001, Some(0x010989F5)),
                        // null XEZN ignored
                        placed_with_xezn("ACHR", 0x01300002, Some(0)),
                        // no XEZN
                        placed_with_xezn("PHZD", 0x01300003, None),
                    ],
                )],
            )],
        )];
        let idx = build_target_index(&tree, false);
        assert!(idx.placed_xezn_targets.contains(&0x0989F5));
        assert_eq!(idx.placed_xezn_targets.len(), 1);
        // Only the REFR with a non-null XEZN is recorded as needing a repoint;
        // the null-XEZN and no-XEZN placed records are not (object-ids masked).
        assert_eq!(idx.placed_xezn_ref_objids, vec![0x300001]);
    }

    #[test]
    fn collect_ids_false_leaves_id_sets_empty() {
        let tree = vec![group(0, *b"LCTN", vec![bare("LCTN", 0x01063DC7)])];
        let idx = build_target_index(&tree, false);
        assert!(idx.all_object_ids.is_empty());
        assert!(idx.lctn_object_ids.is_empty());
    }

    #[test]
    fn collects_cell_locations_and_placed_actor_parent_cells() {
        let cell_id = 0x00ABCD;
        let loc_id = 0x0989F5;
        let tree = vec![group(
            0,
            *b"CELL",
            vec![group(
                2,
                0i32.to_le_bytes(),
                vec![group(
                    3,
                    9i32.to_le_bytes(),
                    vec![
                        interior_cell(cell_id, Some(loc_id)),
                        group(
                            CELL_CHILD_GROUP,
                            cell_id.to_le_bytes(),
                            vec![group(
                                9,
                                cell_id.to_le_bytes(),
                                vec![placed_actor(0x01300002)],
                            )],
                        ),
                    ],
                )],
            )],
        )];
        let idx = build_target_index(&tree, true);
        assert_eq!(idx.cell_locations.get(&cell_id).copied(), Some(loc_id));
        assert_eq!(
            idx.placed_actors,
            vec![PlacedActorRef {
                ref_objid: 0x300002,
                cell_objid: cell_id,
            }]
        );
    }

    #[test]
    fn parallel_index_matches_serial_walk() {
        let world: u32 = 0x25DA15;
        let mut world_children = Vec::new();
        let mut locations = Vec::new();
        for index in 0..128u32 {
            let cell_id = 0x100000 + index;
            let location_id = 0x200000 + index;
            world_children.push(cell(cell_id, index as i32, -(index as i32)));
            world_children.push(group(
                CELL_CHILD_GROUP,
                cell_id.to_le_bytes(),
                vec![placed_with_xezn(
                    "ACHR",
                    0x300000 + index,
                    Some(location_id),
                )],
            ));
            locations.push(bare("LCTN", location_id));
        }
        let tree = vec![
            group(0, *b"LCTN", locations),
            group(
                0,
                *b"WRLD",
                vec![group(
                    WORLD_CHILD_GROUP,
                    world.to_le_bytes(),
                    world_children,
                )],
            ),
        ];

        let mut expected = TargetIndex::default();
        walk(&tree, None, None, true, &mut expected);
        let actual = build_target_index(&tree, true);

        assert_eq!(actual, expected);
    }
}
