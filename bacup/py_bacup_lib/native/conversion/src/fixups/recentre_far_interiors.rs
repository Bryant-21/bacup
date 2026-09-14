//! Fixup: keep FO76 interior cells inside FO4's interior coordinate range.
//!
//! # Why
//! When an interior loads, FO4 moves each placed ref lying more than 30,000
//! units from the origin on Y (X is assumed to behave the same) onto the cell's
//! arrival point. FO76 places some interiors far out (the three missile silos
//! span Y -37,728..-22,594), so thousands of refs and their collision stack on
//! the COC marker and the player's contact gathering stalls the load. No vanilla
//! FO4 interior ref lies beyond ±24,276.
//!
//! # How
//! `ConversionRun::emit_interior_cells` calls [`recentre_cell_children`] for each
//! cell after translating its children and before inserting them. Each crossing
//! axis is centred, moving every placed child's `DATA` position and the cell's
//! NAVM geometry by one offset. The offsets stay on the run ([`InteriorRecentre`])
//! so positions stored outside the cell follow:
//! - [`shift_navi_navmesh_infos`]: the FO76 NAVI rebuild copies each `NVMI`
//!   location and island geometry from the source.
//! - [`shift_teleport_destinations`]: `XTEL` landing positions of doors whose
//!   destination door moved.
//!
//! A cell too wide to fit, or one whose previs is carried (baked in cell
//! coordinates), stays where it is and is reported as a warning.

use rustc_hash::FxHashMap;

use esp_authoring_core::plugin_runtime::WriteEffect;

use crate::fixups::{FixupError, FixupReport};
use crate::ids::SigCode;
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

/// FO4 relocates an interior's placed refs beyond this distance from the origin.
pub const INTERIOR_COORDINATE_LIMIT: f32 = 30_000.0;
/// Headroom inside the limit for actors and physics objects that move after load.
const RECENTRED_EXTENT_LIMIT: f32 = 29_000.0;
/// Normalized `DATA` position field names; translated structs spell them either
/// `PositionRotationPositionX` or `position_rotation_position_x`.
const POSITION_FIELDS: [&str; 3] = [
    "positionrotationpositionx",
    "positionrotationpositiony",
    "positionrotationpositionz",
];
const XTEL_DESTINATION_OFFSET: usize = 4;
const NVMI_LOCATION_OFFSET: usize = 8;
const NVMI_FIXED_LEN: usize = 24;

/// Offsets applied by [`recentre_cell_children`], keyed by own object id.
#[derive(Debug, Default)]
pub struct InteriorRecentre {
    pub placed_ref_offsets: FxHashMap<u32, [f32; 3]>,
    pub navmesh_offsets: FxHashMap<u32, [f32; 3]>,
}

impl InteriorRecentre {
    pub fn clear(&mut self) {
        self.placed_ref_offsets.clear();
        self.navmesh_offsets.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RecentrePlan {
    Unchanged,
    Shift([f32; 3]),
    TooWide { span: [f32; 2] },
    PrevisCarried([f32; 3]),
}

/// Centre each axis whose placed-child positions cross the engine limit.
pub fn plan_interior_offset(positions: &[[f32; 3]]) -> RecentrePlan {
    let bounds = [0, 1].map(|axis| {
        positions.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(min, max), position| (min.min(position[axis]), max.max(position[axis])),
        )
    });
    let crossing = bounds
        .map(|(min, max)| min < -INTERIOR_COORDINATE_LIMIT || max > INTERIOR_COORDINATE_LIMIT);
    if !crossing.contains(&true) {
        return RecentrePlan::Unchanged;
    }
    let span = bounds.map(|(min, max)| max - min);
    if (0..2).any(|axis| crossing[axis] && span[axis] > 2.0 * RECENTRED_EXTENT_LIMIT) {
        return RecentrePlan::TooWide { span };
    }
    let [dx, dy] = [0, 1].map(|axis| {
        let (min, max) = bounds[axis];
        if crossing[axis] {
            (-(min + max) / 2.0).round()
        } else {
            0.0
        }
    });
    RecentrePlan::Shift([dx, dy, 0.0])
}

/// Shift one interior's translated children when they cross the engine limit,
/// recording the offsets of own records for the follow-up repairs.
pub fn recentre_cell_children(
    persistent: &mut [Record],
    temporary: &mut [Record],
    own_plugin: Sym,
    allow_shift: bool,
    interner: &StringInterner,
    recentre: &mut InteriorRecentre,
) -> Result<RecentrePlan, String> {
    let positions: Vec<[f32; 3]> = persistent
        .iter()
        .chain(temporary.iter())
        .filter_map(|record| placed_position(record, interner))
        .collect();
    let plan = plan_interior_offset(&positions);
    let RecentrePlan::Shift(offset) = plan else {
        return Ok(plan);
    };
    if !allow_shift {
        return Ok(RecentrePlan::PrevisCarried(offset));
    }
    // A navmesh that fails to parse is left behind; aborting midway would leave
    // the cell half moved.
    let mut navmesh_errors = Vec::new();
    for record in persistent.iter_mut().chain(temporary.iter_mut()) {
        let offsets = if record.sig.as_str() == "NAVM" {
            match crate::target_write::offset_record_nvnm_geometry(record, offset) {
                Ok(shifted) => (shifted > 0).then_some(&mut recentre.navmesh_offsets),
                Err(e) => {
                    navmesh_errors.push(format!("{:06X}:{e}", record.form_key.local & 0x00FF_FFFF));
                    None
                }
            }
        } else {
            shift_placed_position(record, offset, interner)
                .then_some(&mut recentre.placed_ref_offsets)
        };
        if let Some(offsets) = offsets {
            if record.form_key.plugin == own_plugin {
                offsets.insert(record.form_key.local & 0x00FF_FFFF, offset);
            }
        }
    }
    if navmesh_errors.is_empty() {
        Ok(plan)
    } else {
        Err(format!(
            "navmeshes left in place: {}",
            navmesh_errors.join(", ")
        ))
    }
}

/// A placed child's `DATA` position, from the decoded struct or raw bytes.
pub fn placed_position(record: &Record, interner: &StringInterner) -> Option<[f32; 3]> {
    let data = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "DATA")?;
    match &data.value {
        FieldValue::Struct(fields) => {
            let mut position = [None; 3];
            for (name, value) in fields {
                if let (Some(axis), FieldValue::Float(number)) =
                    (position_axis(*name, interner), value)
                {
                    position[axis] = Some(*number);
                }
            }
            Some([position[0]?, position[1]?, position[2]?])
        }
        FieldValue::Bytes(raw) if raw.len() == 24 => Some([0, 4, 8].map(|at| read_f32(raw, at))),
        _ => None,
    }
}

/// Add `offset` to a placed child's `DATA` position; rotation is untouched.
pub fn shift_placed_position(
    record: &mut Record,
    offset: [f32; 3],
    interner: &StringInterner,
) -> bool {
    if placed_position(record, interner).is_none() {
        return false;
    }
    let Some(data) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "DATA")
    else {
        return false;
    };
    match &mut data.value {
        FieldValue::Struct(fields) => {
            for (name, value) in fields.iter_mut() {
                if let (Some(axis), FieldValue::Float(number)) =
                    (position_axis(*name, interner), value)
                {
                    *number += offset[axis];
                }
            }
        }
        FieldValue::Bytes(raw) => add_f32x3(raw, 0, offset),
        _ => {}
    }
    true
}

/// Move the location and island geometry (bounds and vertices) of a FO4 `NVMI`
/// entry. Nothing is written unless the whole layout parses.
pub fn shift_nvmi_positions(data: &mut [u8], offset: [f32; 3]) -> Result<(), String> {
    if data.len() < NVMI_FIXED_LEN {
        return Err(format!(
            "NVMI is {} bytes; expected at least {NVMI_FIXED_LEN}",
            data.len()
        ));
    }
    let mut points = vec![NVMI_LOCATION_OFFSET];
    let mut at = NVMI_FIXED_LEN;
    for (label, row_len) in [
        ("merged navmeshes", 4),
        ("preferred merges", 4),
        ("linked doors", 8),
    ] {
        let count = read_count(data, &mut at, label)?;
        at = rows_end(at, count, row_len, data.len(), label)?;
    }
    let is_island = *data.get(at).ok_or("NVMI ends before its island flag")? != 0;
    at += 1;
    if is_island {
        points.extend([at, at + 12]);
        at = rows_end(at, 2, 12, data.len(), "island bounds")?;
        let triangles = read_count(data, &mut at, "island triangles")?;
        at = rows_end(at, triangles, 6, data.len(), "island triangles")?;
        let vertices = read_count(data, &mut at, "island vertices")?;
        rows_end(at, vertices, 12, data.len(), "island vertices")?;
        points.extend((0..vertices).map(|index| at + index * 12));
    }
    for point in points {
        add_f32x3(data, point, offset);
    }
    Ok(())
}

/// Shift the `NVMI` entries of moved own navmeshes in the output NAVI.
pub fn shift_navi_navmesh_infos(
    session: &mut PluginSession,
    interner: &StringInterner,
    navmesh_offsets: &FxHashMap<u32, [f32; 3]>,
) -> Result<u32, FixupError> {
    let own_index = session.target_masters().len() as u32;
    let navi_sig = SigCode::from_str("NAVI").map_err(FixupError::SchemaError)?;
    let navi_form_keys = session
        .form_keys_of_sig(navi_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut shifted = 0_u32;
    for form_key in navi_form_keys {
        let raw_form_id = session
            .raw_form_id_for_form_key(&form_key)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let record = session
            .record_mut(raw_form_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut changed = false;
        for subrecord in record.subrecords.iter_mut() {
            if subrecord.signature.as_str() != "NVMI" {
                continue;
            }
            let Some(offset) = own_offset(&subrecord.data, own_index, navmesh_offsets) else {
                continue;
            };
            let mut bytes = subrecord.data.to_vec();
            shift_nvmi_positions(&mut bytes, offset).map_err(FixupError::HandleError)?;
            subrecord.data = bytes.into();
            shifted += 1;
            changed = true;
        }
        if changed {
            session.record_effect(WriteEffect::RecordContents {
                form_ids: smallvec::smallvec![raw_form_id],
            });
        }
    }
    Ok(shifted)
}

/// Shift the `XTEL` landing position of every door whose destination door
/// moved. Runs after the teleport-door repair, once destination FormIDs name
/// the output plugin.
pub fn shift_teleport_destinations(
    session: &mut PluginSession,
    interner: &StringInterner,
    placed_ref_offsets: &FxHashMap<u32, [f32; 3]>,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    if placed_ref_offsets.is_empty() {
        return Ok(report);
    }
    let own_index = session.target_masters().len() as u32;
    let refr_sig = SigCode::from_str("REFR").map_err(FixupError::SchemaError)?;
    let refr_form_keys = session
        .form_keys_of_sig(refr_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for form_key in refr_form_keys {
        let Ok(Some(xtel)) = session.first_subrecord_bytes(&form_key, "XTEL") else {
            continue;
        };
        let Some(offset) = own_offset(&xtel, own_index, placed_ref_offsets) else {
            continue;
        };
        if xtel.len() < XTEL_DESTINATION_OFFSET + 12 {
            continue;
        }
        let raw_form_id = session
            .raw_form_id_for_form_key(&form_key)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let record = session
            .record_mut(raw_form_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let Some(subrecord) = record
            .subrecords
            .iter_mut()
            .find(|subrecord| subrecord.signature.as_str() == "XTEL")
        else {
            continue;
        };
        let mut bytes = subrecord.data.to_vec();
        add_f32x3(&mut bytes, XTEL_DESTINATION_OFFSET, offset);
        subrecord.data = bytes.into();
        session.record_effect(WriteEffect::RecordContents {
            form_ids: smallvec::smallvec![raw_form_id],
        });
        report.records_changed = report.records_changed.saturating_add(1);
    }
    Ok(report)
}

/// The offset recorded for the own record whose FormID starts `data`.
fn own_offset(data: &[u8], own_index: u32, offsets: &FxHashMap<u32, [f32; 3]>) -> Option<[f32; 3]> {
    let raw = u32::from_le_bytes(data.get(..4)?.try_into().ok()?);
    if raw >> 24 != own_index {
        return None;
    }
    offsets.get(&(raw & 0x00FF_FFFF)).copied()
}

fn position_axis(name: Sym, interner: &StringInterner) -> Option<usize> {
    let normalized: String = interner
        .resolve(name)?
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ch.to_ascii_lowercase())
        .collect();
    POSITION_FIELDS
        .iter()
        .position(|field| *field == normalized)
}

fn read_f32(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

fn add_f32x3(bytes: &mut [u8], at: usize, offset: [f32; 3]) {
    for (axis, delta) in offset.into_iter().enumerate() {
        let start = at + axis * 4;
        let shifted = read_f32(bytes, start) + delta;
        bytes[start..start + 4].copy_from_slice(&shifted.to_le_bytes());
    }
}

fn read_count(data: &[u8], at: &mut usize, label: &str) -> Result<usize, String> {
    let bytes = data
        .get(*at..*at + 4)
        .ok_or_else(|| format!("NVMI {label} count is truncated"))?;
    *at += 4;
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")) as usize)
}

fn rows_end(
    start: usize,
    count: usize,
    row_len: usize,
    len: usize,
    label: &str,
) -> Result<usize, String> {
    count
        .checked_mul(row_len)
        .and_then(|bytes| start.checked_add(bytes))
        .filter(|end| *end <= len)
        .ok_or_else(|| format!("NVMI {label} run past the end"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, RecordFlags};
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_new_native,
    };
    use smallvec::SmallVec;

    const SILO_OFFSET: [f32; 3] = [0.0, 30_161.0, 0.0];

    fn record_with(
        sig: &str,
        local: u32,
        fields: Vec<(&str, FieldValue)>,
        interner: &StringInterner,
    ) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields
                .into_iter()
                .map(|(sig, value)| FieldEntry {
                    sig: SubrecordSig::from_str(sig).unwrap(),
                    value,
                })
                .collect(),
            warnings: SmallVec::new(),
        }
    }

    fn position_struct(
        names: [&str; 4],
        values: [f32; 4],
        interner: &StringInterner,
    ) -> FieldValue {
        FieldValue::Struct(
            names
                .into_iter()
                .zip(values)
                .map(|(name, value)| (interner.intern(name), FieldValue::Float(value)))
                .collect(),
        )
    }

    fn bytes(data: Vec<u8>) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(data))
    }

    fn f32s(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn interiors_inside_the_limit_stay_put() {
        assert_eq!(
            plan_interior_offset(&[[-24_276.0, 16_668.0, 0.0], [100.0, -29_999.0, 0.0]]),
            RecentrePlan::Unchanged
        );
        assert_eq!(plan_interior_offset(&[]), RecentrePlan::Unchanged);
    }

    #[test]
    fn silo_extents_centre_only_the_crossing_axis() {
        let silo = [[-15_252.0, -37_728.0, 1792.0], [-1216.0, -22_594.0, 6080.0]];
        assert_eq!(
            plan_interior_offset(&silo),
            RecentrePlan::Shift(SILO_OFFSET)
        );
    }

    #[test]
    fn cells_wider_than_the_limit_are_reported_not_moved() {
        let plan =
            plan_interior_offset(&[[-216_060.0, -172_548.0, 0.0], [195_323.0, 223_090.0, 0.0]]);
        assert_eq!(
            plan,
            RecentrePlan::TooWide {
                span: [411_383.0, 395_638.0]
            }
        );
    }

    #[test]
    fn shifts_decoded_position_and_keeps_rotation() {
        let interner = StringInterner::new();
        let data = position_struct(
            [
                "PositionRotationPositionX",
                "PositionRotationPositionY",
                "PositionRotationPositionZ",
                "PositionRotationRotationZ",
            ],
            [-4992.0, -35_490.0, 3072.0, 1.85],
            &interner,
        );
        let mut record = record_with("REFR", 0x3DD946, vec![("DATA", data)], &interner);
        assert!(shift_placed_position(&mut record, SILO_OFFSET, &interner));
        assert_eq!(
            placed_position(&record, &interner),
            Some([-4992.0, -5329.0, 3072.0])
        );
        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("DATA stays a struct");
        };
        assert!(matches!(fields[3].1, FieldValue::Float(rotation) if rotation == 1.85));
    }

    #[test]
    fn reads_snake_case_position_names() {
        let interner = StringInterner::new();
        let data = position_struct(
            [
                "position_rotation_position_x",
                "position_rotation_position_y",
                "position_rotation_position_z",
                "position_rotation_rotation_x",
            ],
            [1.0, 2.0, 3.0, 4.0],
            &interner,
        );
        let record = record_with("REFR", 0x10, vec![("DATA", data)], &interner);
        assert_eq!(placed_position(&record, &interner), Some([1.0, 2.0, 3.0]));
    }

    #[test]
    fn shifts_raw_position_bytes_only() {
        let interner = StringInterner::new();
        let raw = f32s(&[-10_000.0, -35_000.0, 50.0, 0.1, 0.2, 0.3]);
        let mut record = record_with("REFR", 0x10, vec![("DATA", bytes(raw.clone()))], &interner);
        assert!(shift_placed_position(&mut record, SILO_OFFSET, &interner));
        assert_eq!(
            placed_position(&record, &interner),
            Some([-10_000.0, -4_839.0, 50.0])
        );
        let FieldValue::Bytes(after) = &record.fields[0].value else {
            panic!("DATA stays raw");
        };
        assert_eq!(&after[12..], &raw[12..], "rotation keeps its bytes");
    }

    /// An NVMI entry and the byte offsets of the xyz triples that must move.
    fn nvmi(navmesh: u32, island: bool) -> (Vec<u8>, Vec<usize>) {
        let mut out = navmesh.to_le_bytes().to_vec();
        out.extend(7u32.to_le_bytes());
        let mut points = vec![out.len()];
        out.extend(f32s(&[-8000.0, -30_000.0, 3000.0]));
        out.extend(0u32.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(0x0100_0555u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(0xDEAD_BEEFu32.to_le_bytes());
        out.extend(0x0100_0777u32.to_le_bytes());
        out.push(u8::from(island));
        if island {
            for corner in [[-9000.0, -31_000.0, 2900.0], [-7000.0, -29_000.0, 3100.0]] {
                points.push(out.len());
                out.extend(f32s(&corner));
            }
            out.extend(1u32.to_le_bytes());
            out.extend([0u8, 0, 1, 0, 2, 0]);
            out.extend(2u32.to_le_bytes());
            for vertex in [[-9000.0, -31_000.0, 2900.0], [-7000.0, -29_000.0, 3100.0]] {
                points.push(out.len());
                out.extend(f32s(&vertex));
            }
        }
        out.extend(0x1234u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(0x0100_42F4u32.to_le_bytes());
        (out, points)
    }

    fn shifted_at(mut data: Vec<u8>, points: &[usize]) -> Vec<u8> {
        for &point in points {
            add_f32x3(&mut data, point, SILO_OFFSET);
        }
        data
    }

    #[test]
    fn nvmi_location_and_island_geometry_move_and_nothing_else() {
        for island in [false, true] {
            let (mut data, points) = nvmi(0x0100_0400, island);
            let expected = shifted_at(data.clone(), &points);
            shift_nvmi_positions(&mut data, SILO_OFFSET).unwrap();
            assert_eq!(data, expected, "island={island}");
        }
    }

    #[test]
    fn truncated_nvmi_is_rejected_untouched() {
        let (mut data, _) = nvmi(0x0100_0400, true);
        data.truncate(data.len() - 20);
        let before = data.clone();
        assert!(shift_nvmi_positions(&mut data, SILO_OFFSET).is_err());
        assert_eq!(data, before);
    }

    /// An output plugin with one master, so own records use load index 1.
    fn target_with(records: Vec<Record>, interner: &StringInterner) -> u64 {
        let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        let mut session = open_session(target, None).unwrap();
        let schema = session.schema().unwrap();
        for record in records {
            session
                .add_record(record, schema.as_ref(), interner)
                .unwrap();
        }
        target
    }

    fn xtel(door: u32, destination: [f32; 3]) -> Vec<u8> {
        let mut out = door.to_le_bytes().to_vec();
        out.extend(f32s(&destination));
        out.extend(f32s(&[0.0, 0.0, 1.5]));
        out.extend([0u8; 8]);
        out
    }

    #[test]
    fn doors_into_a_moved_interior_land_at_the_moved_position() {
        let interner = StringInterner::new();
        let door =
            |local, payload| record_with("REFR", local, vec![("XTEL", bytes(payload))], &interner);
        let target = target_with(
            vec![
                door(0x100, xtel(0x0100_0200, [100.0, -35_000.0, 50.0])),
                door(0x101, xtel(0x0100_0300, [1.0, 2.0, 3.0])),
                door(0x102, xtel(0x0000_0200, [1.0, 2.0, 3.0])),
            ],
            &interner,
        );
        let offsets: FxHashMap<u32, [f32; 3]> = [(0x200, SILO_OFFSET)].into_iter().collect();
        let mut session = open_session(target, None).unwrap();

        let report = shift_teleport_destinations(&mut session, &interner, &offsets).unwrap();

        assert_eq!(report.records_changed, 1);
        let mut read = |local| {
            let form_key = FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            };
            session
                .first_subrecord_bytes(&form_key, "XTEL")
                .unwrap()
                .unwrap()
        };
        assert_eq!(read(0x100), xtel(0x0100_0200, [100.0, -4_839.0, 50.0]));
        assert_eq!(read(0x101), xtel(0x0100_0300, [1.0, 2.0, 3.0]));
        assert_eq!(
            read(0x102),
            xtel(0x0000_0200, [1.0, 2.0, 3.0]),
            "a master door sharing the object id is not ours"
        );
    }

    #[test]
    fn navi_entries_of_moved_navmeshes_follow_the_cell() {
        let interner = StringInterner::new();
        let (moved, points) = nvmi(0x0100_0400, true);
        let (untouched, _) = nvmi(0x0100_0500, true);
        let navi = record_with(
            "NAVI",
            0xF00,
            vec![
                ("NVER", bytes(15u32.to_le_bytes().to_vec())),
                ("NVMI", bytes(moved.clone())),
                ("NVMI", bytes(untouched.clone())),
            ],
            &interner,
        );
        let target = target_with(vec![navi], &interner);
        let offsets: FxHashMap<u32, [f32; 3]> = [(0x400, SILO_OFFSET)].into_iter().collect();
        let mut session = open_session(target, None).unwrap();

        let shifted = shift_navi_navmesh_infos(&mut session, &interner, &offsets).unwrap();

        assert_eq!(shifted, 1);
        let raw_form_id = session
            .raw_form_id_for_form_key(&FormKey {
                local: 0xF00,
                plugin: interner.intern("SeventySix.esm"),
            })
            .unwrap();
        let entries: Vec<Vec<u8>> = session
            .record(raw_form_id)
            .unwrap()
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "NVMI")
            .map(|subrecord| subrecord.data.to_vec())
            .collect();
        assert_eq!(entries, [shifted_at(moved, &points), untouched]);
    }
}
