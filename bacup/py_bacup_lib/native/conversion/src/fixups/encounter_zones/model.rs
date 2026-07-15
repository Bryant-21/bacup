//! Pure decode/classify/band logic for FO76→FO4 encounter-zone synthesis.
//!
//! No `PluginSession`/`FormKeyMapper` dependencies — every function here is pure
//! over already-decoded values, so it is unit-testable in isolation.

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::{StringInterner, Sym};
use std::collections::HashMap;

/// Keyword object-ids (local FormID, no master byte). The three "shared"
/// keywords have identical object-ids in FO76 and FO4.
pub const KW_CLEARABLE: u32 = 0x064EDE; // LocTypeClearable
pub const KW_WORKSHOP: u32 = 0x0234F1; // LocTypeWorkshop
pub const KW_SETTLEMENT: u32 = 0x022611; // LocTypeSettlement (FO4)
pub const KW_WORKSHOP_SETTLEMENT: u32 = 0x083C9A; // LocTypeWorkshopSettlement (FO4)
pub const KW_WORKSHOP_PUBLIC: u32 = 0x3AEDF7; // LocTypeWorkshopPublic (FO76-only)
pub const KW_WORKSHOP_SHELTER: u32 = 0x72E3D7; // LocTypeWorkshopShelter (FO76-only)

/// FO76 `LCTN.DATA.location_type` enum value for a workshop location.
pub const LOCATION_TYPE_WORKSHOP: u8 = 9;

/// ECZN.DATA.flags bits.
pub const ECZN_FLAG_NEVER_RESETS: u8 = 1;
pub const ECZN_FLAG_WORKSHOP: u8 = 8;

/// How an FO76 workshop Location maps into FO4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkshopClass {
    /// Public/plain workshop → FO4 buildable settlement.
    Settlement,
    /// FO76 shelter → FO4 buildable private home (no settlement keyword).
    Shelter,
    /// Not a workshop location.
    NonWorkshop,
}

/// Decoded subset of an FO76 `LCTN` needed for encounter-zone synthesis.
#[derive(Debug, Clone)]
pub struct LctnInfo {
    pub form_key: FormKey,
    /// `(min, max)` if this Location's own DATA carries a non-zero band.
    pub own_band: Option<(u8, u8)>,
    pub location_type: u8,
    pub parent: Option<FormKey>,
    /// Keyword object-ids (local FormID only), in array order.
    pub keyword_locals: Vec<u32>,
}

fn struct_field<'a>(
    fields: &'a [(Sym, FieldValue)],
    interner: &StringInterner,
    name: &str,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(s, _)| interner.resolve(*s) == Some(name))
        .map(|(_, v)| v)
}

fn as_u8(v: Option<&FieldValue>) -> u8 {
    match v {
        Some(FieldValue::Uint(u)) => *u as u8,
        Some(FieldValue::Int(n)) => *n as u8,
        _ => 0,
    }
}

fn as_i32(v: Option<&FieldValue>) -> i32 {
    match v {
        Some(FieldValue::Int(n)) => *n as i32,
        Some(FieldValue::Uint(u)) => *u as i32,
        _ => 0,
    }
}

pub fn decode_lctn_info(record: &Record, interner: &StringInterner) -> LctnInfo {
    let mut location_type = 0u8;
    let mut own_band = None;
    let mut parent = None;
    let mut keyword_locals = Vec::new();
    for f in &record.fields {
        match f.sig.as_str() {
            "DATA" => {
                if let FieldValue::Struct(fields) = &f.value {
                    let min = as_u8(struct_field(fields, interner, "min_location_level"));
                    let max = as_u8(struct_field(fields, interner, "max_location_level"));
                    location_type = as_u8(struct_field(fields, interner, "location_type"));
                    if min != 0 || max != 0 {
                        own_band = Some((min, max));
                    }
                } else if let FieldValue::Bytes(b) = &f.value {
                    // FO76 LCTN.DATA is a `struct:I,B,B,B,B` codec that the source
                    // decoder rawifies to Bytes (see source_read.rs generic struct
                    // branch). Layout (8 bytes): [0..4] unknown_int, [4]
                    // unknown_byte, [5] min_location_level, [6] location_type,
                    // [7] max_location_level.
                    if b.len() >= 8 {
                        let min = b[5];
                        location_type = b[6];
                        let max = b[7];
                        if min != 0 || max != 0 {
                            own_band = Some((min, max));
                        }
                    }
                }
            }
            "PNAM" => {
                if let FieldValue::FormKey(fk) = &f.value {
                    if fk.local != 0 {
                        parent = Some(*fk);
                    }
                }
            }
            "KWDA" => {
                if let FieldValue::List(items) = &f.value {
                    for it in items {
                        if let FieldValue::FormKey(fk) = it {
                            keyword_locals.push(fk.local);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    LctnInfo {
        form_key: record.form_key,
        own_band,
        location_type,
        parent,
        keyword_locals,
    }
}

/// Returns `(world, grid_x, grid_y)` for every cell in the Location's `LCEC`
/// footprint(s).
///
/// FO76 `LCEC` is a `struct:I` + `row_array (grid_y:i16, grid_x:i16)` codec that
/// the source decoder rawifies to Bytes. `source_masters` / `source_plugin_name`
/// resolve the raw `world` form_id's master-index high byte against the LCTN's
/// source plugin master order (mirrors `source_read.rs::resolve_form_id`); the
/// Struct branch ignores them (its world is already a resolved FormKey).
pub fn decode_lcec_footprint(
    record: &Record,
    interner: &StringInterner,
    source_masters: &[String],
    source_plugin_name: &str,
) -> Vec<(FormKey, i32, i32)> {
    let mut out = Vec::new();
    for f in &record.fields {
        if f.sig.as_str() != "LCEC" {
            continue;
        }
        match &f.value {
            FieldValue::Struct(top) => {
                let world = match struct_field(top, interner, "world") {
                    Some(FieldValue::FormKey(fk)) => *fk,
                    _ => continue,
                };
                let Some(FieldValue::List(cells)) = struct_field(top, interner, "cells") else {
                    continue;
                };
                for c in cells {
                    if let FieldValue::Struct(pair) = c {
                        out.push((
                            world,
                            as_i32(struct_field(pair, interner, "grid_x")),
                            as_i32(struct_field(pair, interner, "grid_y")),
                        ));
                    }
                }
            }
            FieldValue::Bytes(b) => {
                // Layout: [0..4] world (raw form_id, source master order), then
                // 4-byte rows from [4..]: grid_y i16 @+0, grid_x i16 @+2.
                if b.len() < 4 {
                    continue;
                }
                let raw_world = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                let Some(world) =
                    resolve_raw_form_id(raw_world, source_masters, source_plugin_name, interner)
                else {
                    continue;
                };
                for row in b[4..].chunks_exact(4) {
                    let grid_y = i16::from_le_bytes([row[0], row[1]]) as i32;
                    let grid_x = i16::from_le_bytes([row[2], row[3]]) as i32;
                    out.push((world, grid_x, grid_y));
                }
            }
            _ => continue,
        }
    }
    out
}

/// Resolve a raw 32-bit FormID into a source-plugin `FormKey`, mapping the
/// master-index high byte to a plugin name via the LCTN's source master order
/// (own-plugin index → `source_plugin_name`). Mirrors
/// `source_read.rs::resolve_form_id` but returns a `FormKey` directly. `None`
/// for a null (zero) form_id.
fn resolve_raw_form_id(
    raw: u32,
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let master_index = ((raw >> 24) & 0xFF) as usize;
    let object_id = raw & 0x00FF_FFFF;
    let plugin = masters
        .get(master_index)
        .map(String::as_str)
        .unwrap_or(plugin_name);
    Some(FormKey {
        local: object_id,
        plugin: interner.intern(plugin),
    })
}

/// Classify an FO76 Location from its `location_type` and keyword object-ids.
pub fn classify(location_type: u8, keyword_locals: &[u32]) -> WorkshopClass {
    let has = |k: u32| keyword_locals.contains(&k);
    if has(KW_WORKSHOP_SHELTER) {
        return WorkshopClass::Shelter;
    }
    if location_type == LOCATION_TYPE_WORKSHOP || has(KW_WORKSHOP_PUBLIC) || has(KW_WORKSHOP) {
        return WorkshopClass::Settlement;
    }
    WorkshopClass::NonWorkshop
}

/// Resolve the effective `(min,max)` band for `start` (an LCTN object-id),
/// walking `parents` until a Location with an own band is found. `None` when no
/// ancestor carries one. Cycle-safe via a visited set.
pub fn resolve_band(
    start: u32,
    bands: &HashMap<u32, Option<(u8, u8)>>,
    parents: &HashMap<u32, Option<u32>>,
) -> Option<(u8, u8)> {
    let mut cur = Some(start);
    let mut seen = Vec::new();
    while let Some(o) = cur {
        if seen.contains(&o) {
            break;
        }
        seen.push(o);
        if let Some(Some(b)) = bands.get(&o) {
            return Some(*b);
        }
        cur = parents.get(&o).and_then(|p| *p);
    }
    None
}

pub fn eczn_flags(c: WorkshopClass) -> u8 {
    match c {
        WorkshopClass::Settlement => ECZN_FLAG_WORKSHOP | ECZN_FLAG_NEVER_RESETS,
        WorkshopClass::Shelter => ECZN_FLAG_NEVER_RESETS,
        WorkshopClass::NonWorkshop => 0,
    }
}

pub fn eczn_editor_id(lctn_eid: &str) -> String {
    format!("{lctn_eid}EncounterZone")
}

/// Clamp a u8 band value into the FO4 `i8` ECZN level range `[0,127]`.
pub fn clamp_level(v: u8) -> i8 {
    v.min(127) as i8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::sym::StringInterner;
    use std::collections::HashMap;

    fn lctn(
        interner: &StringInterner,
        local: u32,
        data: Option<(u8, u8, u8)>,
        parent: Option<FormKey>,
        kws: &[u32],
    ) -> Record {
        let p = interner.intern("SeventySix.esm");
        let mut r = Record::new(
            SigCode::from_str("LCTN").unwrap(),
            FormKey { local, plugin: p },
        );
        if let Some((min, lt, max)) = data {
            r.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("DATA").unwrap(),
                value: FieldValue::Struct(vec![
                    (interner.intern("unknown_int"), FieldValue::Uint(0)),
                    (interner.intern("unknown_byte"), FieldValue::Uint(0)),
                    (
                        interner.intern("min_location_level"),
                        FieldValue::Uint(min as u64),
                    ),
                    (
                        interner.intern("location_type"),
                        FieldValue::Uint(lt as u64),
                    ),
                    (
                        interner.intern("max_location_level"),
                        FieldValue::Uint(max as u64),
                    ),
                ]),
            });
        }
        if let Some(pk) = parent {
            r.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("PNAM").unwrap(),
                value: FieldValue::FormKey(pk),
            });
        }
        if !kws.is_empty() {
            r.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("KWDA").unwrap(),
                value: FieldValue::List(
                    kws.iter()
                        .map(|k| {
                            FieldValue::FormKey(FormKey {
                                local: *k,
                                plugin: p,
                            })
                        })
                        .collect(),
                ),
            });
        }
        r
    }

    #[test]
    fn decode_reads_band_type_parent_keywords() {
        let i = StringInterner::new();
        let parent = FormKey {
            local: 0x01558C,
            plugin: i.intern("SeventySix.esm"),
        };
        let r = lctn(
            &i,
            0x0989F5,
            Some((20, 9, 99)),
            Some(parent),
            &[KW_WORKSHOP],
        );
        let info = decode_lctn_info(&r, &i);
        assert_eq!(info.own_band, Some((20, 99)));
        assert_eq!(info.location_type, 9);
        assert_eq!(info.parent.map(|p| p.local), Some(0x01558C));
        assert_eq!(info.keyword_locals, vec![KW_WORKSHOP]);
    }

    #[test]
    fn decode_zero_band_is_none() {
        let i = StringInterner::new();
        let r = lctn(&i, 1, Some((0, 9, 0)), None, &[]);
        assert_eq!(decode_lctn_info(&r, &i).own_band, None);
    }

    /// Raw-Bytes DATA (the `struct:I,B,B,B,B` codec rawified by the source
    /// decoder) must decode band + location_type. Layout: [5]=min, [6]=type,
    /// [7]=max.
    #[test]
    fn decode_raw_bytes_data_reads_band_and_type() {
        let i = StringInterner::new();
        let mut r = Record::new(
            SigCode::from_str("LCTN").unwrap(),
            FormKey {
                local: 0x0989F5,
                plugin: i.intern("SeventySix.esm"),
            },
        );
        // [0..4] unknown_int, [4] unknown_byte, [5] min=20, [6] type=9, [7] max=99
        let bytes: smallvec::SmallVec<[u8; 32]> =
            smallvec::SmallVec::from_slice(&[0, 0, 0, 0, 0, 20, 9, 99]);
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(bytes),
        });
        let info = decode_lctn_info(&r, &i);
        assert_eq!(info.own_band, Some((20, 99)));
        assert_eq!(info.location_type, 9);
    }

    #[test]
    fn decode_raw_bytes_data_zero_band_is_none() {
        let i = StringInterner::new();
        let mut r = Record::new(
            SigCode::from_str("LCTN").unwrap(),
            FormKey {
                local: 1,
                plugin: i.intern("SeventySix.esm"),
            },
        );
        let bytes: smallvec::SmallVec<[u8; 32]> =
            smallvec::SmallVec::from_slice(&[0, 0, 0, 0, 0, 0, 9, 0]);
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(bytes),
        });
        let info = decode_lctn_info(&r, &i);
        assert_eq!(info.own_band, None);
        assert_eq!(info.location_type, 9);
    }

    /// Raw-Bytes LCEC (the `struct:I` + `row_array h,h` codec rawified by the
    /// source decoder) must decode the world form_id + grid rows. Row layout:
    /// grid_y i16 @+0, grid_x i16 @+2.
    #[test]
    fn decode_raw_bytes_lcec_reads_world_and_grids() {
        let i = StringInterner::new();
        // world raw form_id 0x0025DA15: master index 0x00 → masters[0].
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&0x0025DA15u32.to_le_bytes());
        // row 1: grid_y=-16, grid_x=-2
        data.extend_from_slice(&(-16i16).to_le_bytes());
        data.extend_from_slice(&(-2i16).to_le_bytes());
        // row 2: grid_y=-15, grid_x=-4
        data.extend_from_slice(&(-15i16).to_le_bytes());
        data.extend_from_slice(&(-4i16).to_le_bytes());
        let mut r = Record::new(
            SigCode::from_str("LCTN").unwrap(),
            FormKey {
                local: 1,
                plugin: i.intern("SeventySix.esm"),
            },
        );
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LCEC").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&data)),
        });
        let masters = vec!["Appalachia.esm".to_string()];
        let footprint = decode_lcec_footprint(&r, &i, &masters, "SeventySix.esm");
        let world = FormKey {
            local: 0x25DA15,
            plugin: i.intern("Appalachia.esm"),
        };
        assert_eq!(footprint, vec![(world, -2, -16), (world, -4, -15)]);
    }

    /// A world whose master-index high byte is the LCTN's own-plugin index
    /// resolves to the source plugin name.
    #[test]
    fn decode_raw_bytes_lcec_own_plugin_world() {
        let i = StringInterner::new();
        // one master → own index is 1 (0x01 high byte).
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&0x0125DA15u32.to_le_bytes());
        data.extend_from_slice(&0i16.to_le_bytes());
        data.extend_from_slice(&0i16.to_le_bytes());
        let mut r = Record::new(
            SigCode::from_str("LCTN").unwrap(),
            FormKey {
                local: 1,
                plugin: i.intern("SeventySix.esm"),
            },
        );
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LCEC").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&data)),
        });
        let masters = vec!["Fallout4.esm".to_string()];
        let footprint = decode_lcec_footprint(&r, &i, &masters, "SeventySix.esm");
        assert_eq!(footprint.len(), 1);
        assert_eq!(footprint[0].0.local, 0x25DA15);
        assert_eq!(footprint[0].0.plugin, i.intern("SeventySix.esm"));
    }

    #[test]
    fn decode_lcec_reads_world_and_grids() {
        let i = StringInterner::new();
        let world = FormKey {
            local: 0x25DA15,
            plugin: i.intern("SeventySix.esm"),
        };
        let lcec = FieldValue::Struct(vec![
            (i.intern("world"), FieldValue::FormKey(world)),
            (
                i.intern("cells"),
                FieldValue::List(vec![
                    FieldValue::Struct(vec![
                        (i.intern("grid_y"), FieldValue::Int(-16)),
                        (i.intern("grid_x"), FieldValue::Int(-2)),
                    ]),
                    FieldValue::Struct(vec![
                        (i.intern("grid_y"), FieldValue::Int(-15)),
                        (i.intern("grid_x"), FieldValue::Int(-4)),
                    ]),
                ]),
            ),
        ]);
        let mut r = Record::new(
            SigCode::from_str("LCTN").unwrap(),
            FormKey {
                local: 1,
                plugin: world.plugin,
            },
        );
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LCEC").unwrap(),
            value: lcec,
        });
        assert_eq!(
            decode_lcec_footprint(&r, &i, &[], "SeventySix.esm"),
            vec![(world, -2, -16), (world, -4, -15)]
        );
    }

    #[test]
    fn classify_rules() {
        assert_eq!(
            classify(9, &[KW_WORKSHOP, KW_WORKSHOP_SHELTER]),
            WorkshopClass::Shelter
        );
        assert_eq!(
            classify(9, &[KW_WORKSHOP, KW_WORKSHOP_PUBLIC]),
            WorkshopClass::Settlement
        );
        assert_eq!(classify(0, &[KW_WORKSHOP]), WorkshopClass::Settlement);
        assert_eq!(
            classify(LOCATION_TYPE_WORKSHOP, &[]),
            WorkshopClass::Settlement
        );
        assert_eq!(classify(0, &[KW_CLEARABLE]), WorkshopClass::NonWorkshop);
    }

    #[test]
    fn band_walks_parents() {
        let p = (0x01558Cu32, 0x0989F5u32, 0x129906u32);
        let mut bands: HashMap<u32, Option<(u8, u8)>> = HashMap::new();
        let mut parents: HashMap<u32, Option<u32>> = HashMap::new();
        bands.insert(p.0, Some((20, 99)));
        bands.insert(p.1, None);
        bands.insert(p.2, None);
        parents.insert(p.2, Some(p.1));
        parents.insert(p.1, Some(p.0));
        parents.insert(p.0, None);
        assert_eq!(resolve_band(p.2, &bands, &parents), Some((20, 99)));
        let mut b2: HashMap<u32, Option<(u8, u8)>> = HashMap::new();
        b2.insert(9, None);
        let mut pa2: HashMap<u32, Option<u32>> = HashMap::new();
        pa2.insert(9, None);
        assert_eq!(resolve_band(9, &b2, &pa2), None);
    }

    #[test]
    fn flags_and_eid() {
        assert_eq!(eczn_flags(WorkshopClass::Settlement), 9);
        assert_eq!(eczn_flags(WorkshopClass::Shelter), 1);
        assert_eq!(eczn_flags(WorkshopClass::NonWorkshop), 0);
        assert_eq!(
            eczn_editor_id("LocWhitespring"),
            "LocWhitespringEncounterZone"
        );
        assert_eq!(clamp_level(200), 127);
        assert_eq!(clamp_level(50), 50);
    }
}
