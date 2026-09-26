//! Decoders for the record fields previs generation consumes.
//!
//! Subrecords whose effect on CK's output was proved by a controlled or
//! shipped-asset oracle decode on the parity path. Others that cannot change
//! visibility geometry are tolerated and reported, so the caller can mark the
//! scene `non_parity`; only data that would give wrong geometry is refused.

use crate::error::{PrevisError, Result};
use crate::f32ops::{Transform, euler_rotation};
use crate::plugin::{Plugin, RecordRef, read_f32, read_i32, read_u32, zstring};

pub const CELL_FLAG_INTERIOR: u16 = 0x0001;
pub const RECORD_FLAG_DELETED: u32 = 0x0000_0020;
pub const BASE_FLAG_IS_MARKER: u32 = 0x0080_0000;

const WRLD_SMALL_WORLD: u8 = 0x01;
const WRLD_NO_LANDSCAPE: u8 = 0x10;
const WRLD_FIXED_DIMENSIONS: u8 = 0x40;
const WRLD_PARENT_USE_LAND: u16 = 0x0001;

const LAND_HAS_HEIGHTS: u32 = 0x01;
const LAND_HAS_COLORS: u32 = 0x02;
const LAND_HAS_LAYERS: u32 = 0x04;
const LAND_UNKNOWN4: u32 = 0x08;
const LAND_AUTO_CALC_NORMALS: u32 = 0x10;

/// Surface paint and normals do not affect the visibility model.
const LAND_IGNORED: [&[u8; 4]; 5] = [b"VNML", b"VCLR", b"BTXT", b"ATXT", b"VTXT"];

/// REFR subrecords the verified direct path accepts. `XLYR` (editor layer)
/// is admitted by `ReferencePolicy::accept_layer` only.
const REFERENCE_FIELDS: [&[u8; 4]; 4] = [b"EDID", b"NAME", b"DATA", b"XSCL"];

/// REFR fields marking state that can change at runtime (scripts, enable
/// parents, links, attachment, quest location roles). CK's own eligibility
/// predicate (`precombine::reference_gate`) rejects them all, and no
/// reference with VMAD, XESP or XLKR appears in the object lists of the 307
/// retail Fallout4.esm interior tomes. Such a reference may be disabled or
/// moved, so it is left out of visibility entirely: never an occluder, never
/// culled.
const DYNAMIC_REFERENCE_FIELDS: [(&[u8; 4], &str); 5] = [
    (b"VMAD", "scripted"),
    (b"XESP", "enable parented"),
    (b"XLKR", "linked"),
    (b"XATR", "attached"),
    (b"XLRT", "location ref type"),
];

/// REFR fields whose rendered shape is not the base model's static pose.
const SHAPE_OVERRIDE_FIELDS: [(&[u8; 4], &str); 2] = [(b"XBSD", "spline-shaped"), (b"XRGD", "ragdoll-posed")];

/// REFR fields that can make the rendered surface see-through: a material
/// swap (whose replacement materials are not resolved here) or an alpha fade.
/// Such a reference stays a target but never occludes.
const NON_OCCLUDING_FIELDS: [(&[u8; 4], &str); 2] = [(b"XMSP", "material-swapped"), (b"XALP", "alpha-faded")];

pub struct Cell {
    pub form_id: u32,
    pub editor_id: Option<String>,
    pub interior: bool,
    pub grid: Option<(i32, i32)>,
    /// Raw XCLC land-flag byte; see [`Cell::verified_land_hide_flags`].
    pub land_flags: u8,
}

impl Cell {
    /// `HideQuad1..4` bits; other land flags have no verified effect.
    pub fn verified_land_hide_flags(&self) -> Result<u8> {
        if self.land_flags & 0xF0 != 0 {
            return Err(PrevisError::unsupported(format!(
                "CELL {:08X} has unverified XCLC land flags 0x{:02X}",
                self.form_id, self.land_flags
            )));
        }
        Ok(self.land_flags)
    }
}

pub fn read_cell(plugin: &Plugin, record: &RecordRef) -> Result<Cell> {
    let subrecords = plugin.subrecords_at(record)?;
    let flags = subrecords.first(b"DATA").map_or(0u16, |data| match data.len() {
        0 => 0,
        1 => data[0] as u16,
        _ => u16::from_le_bytes([data[0], data[1]]),
    });
    let (grid, land_flags) = match subrecords.first(b"XCLC") {
        Some(data) if data.len() >= 9 => (
            Some((read_i32(data, 0).unwrap(), read_i32(data, 4).unwrap())),
            data[8],
        ),
        Some(data) if data.len() >= 8 => (Some((read_i32(data, 0).unwrap(), read_i32(data, 4).unwrap())), 0),
        _ => (None, 0),
    };
    Ok(Cell {
        form_id: record.form_id,
        editor_id: subrecords.first(b"EDID").map(zstring),
        interior: flags & CELL_FLAG_INTERIOR != 0,
        grid,
        land_flags,
    })
}

pub struct World {
    pub form_id: u32,
    pub editor_id: String,
    pub no_landscape: bool,
    /// Raw FormID (as seen from the world's plugin) of the `UseLandData` parent.
    pub land_parent: Option<u32>,
    /// DATA flags outside the verified subset (fast travel, LOD water, sky,
    /// grass, ...). None changes the visibility geometry: `non_parity` only.
    pub unverified_flags: u8,
}

pub fn read_world(plugin: &Plugin, record: &RecordRef) -> Result<World> {
    let subrecords = plugin.subrecords_at(record)?;
    let editor_id = subrecords
        .first(b"EDID")
        .map(zstring)
        .ok_or_else(|| PrevisError::invalid(format!("WRLD {:08X} has no EditorID", record.form_id)))?;
    let flags = subrecords.first(b"DATA").and_then(|d| d.first().copied()).unwrap_or(0);
    let known = WRLD_SMALL_WORLD | WRLD_NO_LANDSCAPE | WRLD_FIXED_DIMENSIONS;
    if subrecords.first(b"NAM0").is_none() || subrecords.first(b"NAM9").is_none() {
        return Err(PrevisError::invalid(format!("WRLD {editor_id} requires Min and Max bounds")));
    }
    let parent = subrecords.first(b"WNAM").and_then(|d| read_u32(d, 0));
    let parent_flags = subrecords
        .first(b"PNAM")
        .map(|d| if d.len() >= 2 { u16::from_le_bytes([d[0], d[1]]) } else { d.first().copied().unwrap_or(0) as u16 })
        .unwrap_or(0);
    Ok(World {
        form_id: record.form_id,
        editor_id,
        no_landscape: flags & WRLD_NO_LANDSCAPE != 0,
        land_parent: parent.filter(|_| parent_flags & WRLD_PARENT_USE_LAND != 0),
        unverified_flags: flags & !known,
    })
}

pub enum LandHeights {
    /// No VHGT: CK uses a flat `-2048` surface.
    Flat,
    Vhgt { base: f32, deltas: Box<[[i8; 33]; 33]> },
}

pub fn read_land(plugin: &Plugin, record: &RecordRef) -> Result<LandHeights> {
    let subrecords = plugin.subrecords_at(record)?;
    let mut flags = None;
    let mut vhgt = None;
    for (signature, data) in subrecords.iter() {
        if LAND_IGNORED.contains(&&signature) {
            continue;
        }
        let slot = match &signature {
            b"DATA" => &mut flags,
            b"VHGT" => &mut vhgt,
            _ => {
                return Err(PrevisError::unsupported(format!(
                    "LAND {:08X} has unverified field {}",
                    record.form_id,
                    String::from_utf8_lossy(&signature)
                )));
            }
        };
        if slot.replace(data).is_some() {
            return Err(PrevisError::invalid(format!("LAND {:08X} repeats a field", record.form_id)));
        }
    }
    let flags = flags.and_then(|d| read_u32(d, 0)).unwrap_or(0);
    let Some(vhgt) = vhgt else {
        if flags != 0 {
            return Err(PrevisError::unsupported(format!(
                "LAND {:08X} has flags without a VertexHeightMap",
                record.form_id
            )));
        }
        return Ok(LandHeights::Flat);
    };
    let required = LAND_HAS_HEIGHTS | LAND_AUTO_CALC_NORMALS;
    let known = required | LAND_HAS_COLORS | LAND_HAS_LAYERS | LAND_UNKNOWN4;
    if flags & required != required || flags & !known != 0 {
        return Err(PrevisError::unsupported(format!(
            "LAND {:08X} VHGT flags 0x{flags:X} are outside the verified subset",
            record.form_id
        )));
    }
    if vhgt.len() < 4 + 33 * 33 {
        return Err(PrevisError::invalid(format!("LAND {:08X} VHGT is truncated", record.form_id)));
    }
    let mut deltas = Box::new([[0i8; 33]; 33]);
    for (index, &value) in vhgt[4..4 + 33 * 33].iter().enumerate() {
        deltas[index / 33][index % 33] = value as i8;
    }
    Ok(LandHeights::Vhgt {
        base: read_f32(vhgt, 0).unwrap(),
        deltas,
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReferencePolicy {
    pub accept_layer: bool,
}

pub enum Reference {
    /// Collision/trigger primitive: never visibility geometry.
    Primitive,
    /// Deleted, initially disabled, runtime-dynamic (see
    /// [`DYNAMIC_REFERENCE_FIELDS`]) or not shaped like its model (see
    /// [`SHAPE_OVERRIDE_FIELDS`]); the reason names which. `non_parity`.
    Excluded(&'static str),
    Placed {
        base: u32,
        transform: Transform,
        /// First field outside the verified set. The field does not affect
        /// geometry, but the scene is no longer CK byte-identical: `non_parity`.
        unverified_field: Option<String>,
        /// Why the reference must not occlude (see [`NON_OCCLUDING_FIELDS`]).
        non_occluder: Option<&'static str>,
    },
}

pub fn read_reference(plugin: &Plugin, record: &RecordRef, policy: ReferencePolicy) -> Result<Reference> {
    if record.flags & RECORD_FLAG_DELETED != 0 {
        return Ok(Reference::Excluded("deleted"));
    }
    if record.flags & crate::precombine::REFERENCE_FLAG_INITIALLY_DISABLED != 0 {
        return Ok(Reference::Excluded("initially disabled"));
    }
    let subrecords = plugin.subrecords_at(record)?;
    if subrecords.first(b"XPRM").is_some() {
        return Ok(Reference::Primitive);
    }
    let mut base = None;
    let mut data = None;
    let mut scale = None;
    let mut unverified_field = None;
    let mut non_occluder = None;
    for (signature, payload) in subrecords.iter() {
        match &signature {
            b"NAME" => base = read_u32(payload, 0),
            b"DATA" => data = Some(payload),
            b"XSCL" => scale = read_f32(payload, 0),
            b"XLYR" if policy.accept_layer => {}
            other if REFERENCE_FIELDS.contains(&other) => {}
            other => {
                let reason_for = |table: &[(&[u8; 4], &'static str)]| {
                    table.iter().find(|(field, _)| *field == other).map(|(_, reason)| *reason)
                };
                if let Some(reason) = reason_for(&DYNAMIC_REFERENCE_FIELDS).or_else(|| reason_for(&SHAPE_OVERRIDE_FIELDS)) {
                    return Ok(Reference::Excluded(reason));
                }
                if let Some(reason) = reason_for(&NON_OCCLUDING_FIELDS) {
                    non_occluder.get_or_insert(reason);
                }
                unverified_field.get_or_insert_with(|| String::from_utf8_lossy(other).into_owned());
            }
        }
    }
    let base = base.ok_or_else(|| PrevisError::invalid(format!("REFR {:08X} has no base", record.form_id)))?;
    Ok(Reference::Placed {
        base,
        transform: placement(data, scale),
        unverified_field,
        non_occluder,
    })
}

/// A reference's world transform from its `DATA` position/rotation and `XSCL`.
pub fn placement(data: Option<&[u8]>, scale: Option<f32>) -> Transform {
    let value = |offset| data.and_then(|d| read_f32(d, offset)).unwrap_or(0.0);
    let euler = [value(12), value(16), value(20)];
    Transform {
        rotation: if euler.iter().all(|&v| v == 0.0) {
            Transform::IDENTITY.rotation
        } else {
            euler_rotation(euler)
        },
        translation: [value(0), value(4), value(8)],
        scale: scale.unwrap_or(1.0),
    }
}

pub struct Base {
    pub signature: [u8; 4],
    pub is_marker: bool,
    pub model: Option<String>,
}

pub fn read_base(plugin: &Plugin, record: &RecordRef) -> Result<Base> {
    let subrecords = plugin.subrecords_at(record)?;
    let models: Vec<String> = subrecords
        .iter()
        .filter(|(s, _)| s == b"MODL")
        .map(|(_, d)| zstring(d))
        .filter(|m| !m.is_empty())
        .collect();
    if models.len() > 1 {
        return Err(PrevisError::unsupported(format!(
            "base {:08X} has more than one model path",
            record.form_id
        )));
    }
    Ok(Base {
        signature: record.signature,
        is_marker: record.flags & BASE_FLAG_IS_MARKER != 0,
        model: models.into_iter().next(),
    })
}
