//! FO4→Starfield struct relayouts for the five subrecords the translation map
//! could only drop.
//!
//! Every layout below was read off `py_creation_lib/native/esp/generated/{fo4,
//! starfield}.rs` and cross-checked against decoded vanilla payloads
//! (`modkit esp list-records --has-subrecord <SIG> --include-subrecord-data`
//! over `Fallout4.esm` / `Starfield.esm`). Where a Starfield field has no FO4
//! source, the constant written here is the dominant vanilla Starfield value
//! for that field, cited at its definition.
//!
//! Struct-codec subrecords reach a pair hook as raw `FieldValue::Bytes` — the
//! generic decoder never expands them (`source_read.rs`) — so these functions
//! read and write little-endian bytes directly. The one exception is
//! `SCOL.ONAM`, whose FO4 codec is a plain `formid` and therefore arrives as a
//! `FieldValue::FormKey`.
//!
//! Failure policy: a payload this module cannot interpret is **removed**, never
//! left in place. An FO4-shaped payload surviving into a Starfield-schema
//! record is the exact corruption the map's `drop:` lists exist to prevent.
//! `SCOL.DATA` is the one exception — its layout is already correct and only
//! its units are wrong, so an uninterpretable payload is kept rather than
//! deleted (see `rescale_scol_placements`).

use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use smallvec::SmallVec;

/// FO4 game units → Starfield metres (1 / 69.99125).
/// Duplicated rather than imported from `phase::terrain_btd` to keep the
/// translator free of a dependency on the phase layer.
///
/// Ground truth for the light case: FO4 `LIGH.DATA` Near Clip is 7.2174 in 48%
/// of vanilla `Fallout4.esm` lights; 7.2174 × this factor = 0.1031 m, and
/// Starfield's most common vanilla `DAT2` Near Clip is 0.15 m.
const FO4_TO_SF_SPATIAL: f32 = 1.0 / 69.99125;

fn f32_at(src: &[u8], off: usize) -> f32 {
    f32::from_le_bytes([src[off], src[off + 1], src[off + 2], src[off + 3]])
}

fn u32_at(src: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([src[off], src[off + 1], src[off + 2], src[off + 3]])
}

fn i16_at(src: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([src[off], src[off + 1]])
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// Truncate-or-zero-pad `raw` to exactly `len` bytes.
///
/// Both games ship trailing-truncated variants of these structs (FO4
/// `LIGH.DATA` is 64 or 60 bytes in vanilla; `LGTM.DATA` is 136, 128 or 92;
/// `WATR.DNAM` is 201 or 188), so every reader here normalises first.
fn fit(raw: &[u8], len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    let take = raw.len().min(len);
    out[..take].copy_from_slice(&raw[..take]);
    out
}

fn bytes_field(sig: &str, payload: Vec<u8>) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(sig).expect("literal 4CC"),
        value: FieldValue::Bytes(SmallVec::from_vec(payload)),
    }
}

// ---------------------------------------------------------------------------
// LIGH.DATA (FO4, 64 B) -> LIGH.DAT2 (Starfield, 76 B)
// ---------------------------------------------------------------------------

/// Starfield `LIGH` has no `DATA` subrecord at all; the light's behaviour lives
/// in `DAT2`, so this is a rename *and* a repack.
const FO4_LIGH_DATA_LEN: usize = 64;
const SF_LIGH_DAT2_LEN: usize = 76;
/// Shortest FO4 payload that still carries time/radius/colour/flags/falloff/FOV
/// — anything shorter cannot produce a usable light and is discarded.
const FO4_LIGH_DATA_MIN_LEN: usize = 24;

// FO4 LIGH.DATA.flags bits (fo4.rs enum `LIGH.DATA.flags`).
const FO4_LIGH_CAN_BE_CARRIED: u32 = 0x0000_0002;
const FO4_LIGH_FLICKER: u32 = 0x0000_0008;
const FO4_LIGH_OFF_BY_DEFAULT: u32 = 0x0000_0020;
const FO4_LIGH_PULSE: u32 = 0x0000_0080;
const FO4_LIGH_SHADOW_SPOTLIGHT: u32 = 0x0000_0400;
const FO4_LIGH_NONSHADOW_SPOTLIGHT: u32 = 0x0000_4000;
const FO4_LIGH_NON_SPECULAR: u32 = 0x0000_8000;

// Starfield LIGH.DAT2.flags bits (starfield.rs enum `LIGH.DAT2.flags`).
// Bit 0 is unnamed but set in 88% of the 600 sampled vanilla Starfield lights
// and in every one of the 12 remaining flag values, so it is treated as
// mandatory rather than optional.
const SF_LIGH_UNKNOWN_0: u16 = 0x0001;
const SF_LIGH_CAN_BE_CARRIED: u16 = 0x0002;
const SF_LIGH_OFF_BY_DEFAULT: u16 = 0x0010;
const SF_LIGH_DISABLE_SPECULAR: u16 = 0x0020;

// Starfield LIGH.DAT2.light_type enum.
const SF_LIGHT_TYPE_OMNIDIRECTIONAL: u8 = 0;
const SF_LIGHT_TYPE_SHADOW_SPOTLIGHT: u8 = 1;
const SF_LIGHT_TYPE_NONSHADOW_SPOTLIGHT: u8 = 2;

// Starfield LIGH.DAT2.flicker_effect enum.
const SF_FLICKER_NONE: u8 = 0;
const SF_FLICKER_FLICKER: u8 = 1;
const SF_FLICKER_PULSE: u8 = 2;

// Starfield-only DAT2 fields with no FO4 source. Values are the dominant
// vanilla `Starfield.esm` values over a 600-record `DAT2` sample:
//   shadow_offset 0.0 (100%), inner_fov 0.0 (95%), adaptive-lighting
//   enable 0 (71%) / Ec 0.0 (98%) / Ev100 min -4.0 (70%) / max 14.0 (71%),
//   radius falloff exponent 4.0 (99%).
const SF_LIGH_SHADOW_OFFSET_DEFAULT: f32 = 0.0;
const SF_LIGH_INNER_FOV_DEFAULT: f32 = 0.0;
/// Raw `uint32` bit pattern; 75% of vanilla lights carry exactly this value in
/// the "PBR - Light Temperature (K)" slot. Copied verbatim rather than
/// reinterpreted because the field is inert unless the `Use PBR Value` flag
/// (0x0100) is set, and this hook never sets it.
const SF_LIGH_PBR_TEMPERATURE_DEFAULT: u32 = 0x4120_0000;
/// Left at zero: with `Use PBR Value` unset, luminous power is not consulted,
/// and fabricating a lumen figure from an FO4 radius risks blowing out a scene
/// if a downstream pass ever enables PBR.
const SF_LIGH_PBR_LUMENS_DEFAULT: u32 = 0;
const SF_LIGH_ADAPTIVE_ENABLE_DEFAULT: u8 = 0;
const SF_LIGH_ADAPTIVE_UNKNOWN_DEFAULT: u8 = 0;
const SF_LIGH_ADAPTIVE_EC_DEFAULT: f32 = 0.0;
const SF_LIGH_ADAPTIVE_EV100_MIN_DEFAULT: f32 = -4.0;
const SF_LIGH_ADAPTIVE_EV100_MAX_DEFAULT: f32 = 14.0;
const SF_LIGH_RADIUS_FALLOFF_EXPONENT_DEFAULT: f32 = 4.0;

/// FO4 flag word → Starfield flag half-word.
///
/// Only three FO4 bits have a same-meaning Starfield bit. FO4's flicker/pulse
/// and shadow-type bits are *not* flags in Starfield — they moved into the
/// `light_type` and `flicker_effect` enum bytes and are handled separately.
/// FO4-only concepts with no Starfield bit (Attenuation Only, Ambient Only,
/// Ignore Roughness, No Rim Lighting, NonShadow Box, Shadow Hemisphere) are
/// dropped; Starfield-only bits (Use PBR Value, Focus Spotlight Beam,
/// Externally Controlled, Disable Distance Attenuation, Is Direct Spotlight)
/// have no FO4 source and stay clear.
fn sf_light_flags(fo4_flags: u32) -> u16 {
    let mut flags = SF_LIGH_UNKNOWN_0;
    if fo4_flags & FO4_LIGH_CAN_BE_CARRIED != 0 {
        flags |= SF_LIGH_CAN_BE_CARRIED;
    }
    if fo4_flags & FO4_LIGH_OFF_BY_DEFAULT != 0 {
        flags |= SF_LIGH_OFF_BY_DEFAULT;
    }
    if fo4_flags & FO4_LIGH_NON_SPECULAR != 0 {
        flags |= SF_LIGH_DISABLE_SPECULAR;
    }
    flags
}

/// Starfield has no hemisphere or box light; both collapse to omnidirectional,
/// which is also the value FO4's Shadow OmniDirectional bit maps to.
fn sf_light_type(fo4_flags: u32) -> u8 {
    if fo4_flags & FO4_LIGH_SHADOW_SPOTLIGHT != 0 {
        SF_LIGHT_TYPE_SHADOW_SPOTLIGHT
    } else if fo4_flags & FO4_LIGH_NONSHADOW_SPOTLIGHT != 0 {
        SF_LIGHT_TYPE_NONSHADOW_SPOTLIGHT
    } else {
        SF_LIGHT_TYPE_OMNIDIRECTIONAL
    }
}

fn sf_flicker_effect(fo4_flags: u32) -> u8 {
    if fo4_flags & FO4_LIGH_FLICKER != 0 {
        SF_FLICKER_FLICKER
    } else if fo4_flags & FO4_LIGH_PULSE != 0 {
        SF_FLICKER_PULSE
    } else {
        SF_FLICKER_NONE
    }
}

/// FO4 `LIGH.DATA` member offsets (fo4.rs codec
/// `i,I,B,B,B,B,I,f,f,f,f,f,f,f,f,f,f,I,f`).
mod fo4_ligh {
    pub(super) const TIME: usize = 0;
    pub(super) const RADIUS: usize = 4;
    pub(super) const COLOR: usize = 8; // 4 bytes RGBA
    pub(super) const FLAGS: usize = 12;
    pub(super) const FALLOFF_EXPONENT: usize = 16;
    pub(super) const FOV: usize = 20;
    pub(super) const NEAR_CLIP: usize = 24;
    pub(super) const FLICKER_BLOCK: usize = 28; // 3 × f32
}

pub(super) fn fo4_ligh_data_to_starfield_dat2(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() < FO4_LIGH_DATA_MIN_LEN {
        return None;
    }
    let src = fit(raw, FO4_LIGH_DATA_LEN);
    let fo4_flags = u32_at(&src, fo4_ligh::FLAGS);

    let mut out = Vec::with_capacity(SF_LIGH_DAT2_LEN);
    out.extend_from_slice(&src[fo4_ligh::TIME..][..4]); // Time (i32), verbatim.
    // Radius: FO4 uint32 game units -> Starfield float32 metres.
    push_f32(
        &mut out,
        u32_at(&src, fo4_ligh::RADIUS) as f32 * FO4_TO_SF_SPATIAL,
    );
    out.extend_from_slice(&src[fo4_ligh::COLOR..][..3]); // Colour R,G,B.
    out.push(0); // Colour byte 4 — 0 in 100% of vanilla Starfield lights.
    out.extend_from_slice(&sf_light_flags(fo4_flags).to_le_bytes());
    out.push(0); // Unknown Byte 8 — 0 in 100% of vanilla.
    out.push(0); // Unknown Byte 9 — 0 in 100% of vanilla.
    out.extend_from_slice(&src[fo4_ligh::FALLOFF_EXPONENT..][..4]);
    out.extend_from_slice(&src[fo4_ligh::FOV..][..4]); // Degrees, not a distance.
    // Near Clip is a distance in game units.
    push_f32(
        &mut out,
        f32_at(&src, fo4_ligh::NEAR_CLIP) * FO4_TO_SF_SPATIAL,
    );
    // Flicker period / intensity amplitude / movement amplitude.
    out.extend_from_slice(&src[fo4_ligh::FLICKER_BLOCK..][..12]);
    push_f32(&mut out, SF_LIGH_SHADOW_OFFSET_DEFAULT);
    push_f32(&mut out, SF_LIGH_INNER_FOV_DEFAULT);
    push_u32(&mut out, SF_LIGH_PBR_TEMPERATURE_DEFAULT);
    push_u32(&mut out, SF_LIGH_PBR_LUMENS_DEFAULT);
    out.push(sf_light_type(fo4_flags));
    out.push(sf_flicker_effect(fo4_flags));
    out.push(SF_LIGH_ADAPTIVE_ENABLE_DEFAULT);
    out.push(SF_LIGH_ADAPTIVE_UNKNOWN_DEFAULT);
    push_f32(&mut out, SF_LIGH_ADAPTIVE_EC_DEFAULT);
    push_f32(&mut out, SF_LIGH_ADAPTIVE_EV100_MIN_DEFAULT);
    push_f32(&mut out, SF_LIGH_ADAPTIVE_EV100_MAX_DEFAULT);
    push_f32(&mut out, SF_LIGH_RADIUS_FALLOFF_EXPONENT_DEFAULT);
    debug_assert_eq!(out.len(), SF_LIGH_DAT2_LEN);
    Some(out)
}

/// FO4 fields with no Starfield slot and therefore no destination: Constant,
/// Scalar, Exponent and God Rays - Near Clip (FO4's volumetric god-ray tuning,
/// which Starfield drives from `GDRY`/`VOLI` instead), plus Value and Weight
/// (the carryable-light inventory stats — Starfield keeps no such pair on the
/// light record).
pub(super) fn relayout_ligh_data_to_dat2(record: &mut Record) {
    if record.sig.0 != *b"LIGH" {
        return;
    }
    let Some(index) = record.fields.iter().position(|e| e.sig.0 == *b"DATA") else {
        return;
    };
    let repacked = match &record.fields[index].value {
        FieldValue::Bytes(raw) => fo4_ligh_data_to_starfield_dat2(raw),
        _ => None,
    };
    match repacked {
        Some(payload) => record.fields[index] = bytes_field("DAT2", payload),
        None => {
            record.fields.remove(index);
        }
    }
}

// ---------------------------------------------------------------------------
// SCOL.ONAM: FO4 `formid` (4 B) -> Starfield `struct:I,B,B,B,B` (8 B)
// ---------------------------------------------------------------------------

const SF_SCOL_ONAM_LEN: usize = 8;

/// All 83 `ONAM` rows across the 15 vanilla `Starfield.esm` SCOLs that carry
/// one have these four trailing bytes zeroed, so the FO4 part FormKey is simply
/// zero-padded up to the Starfield row width.
const SF_SCOL_ONAM_TRAILING_BYTES: [&str; 4] = [
    "unknown_u8_1",
    "unknown_u8_2",
    "unknown_u8_3",
    "unknown_u8_4",
];

/// Emitted as a `FieldValue::Struct` (not flattened bytes) so the `Static`
/// member stays a live `FieldValue::FormKey`: `FormKeyMapper::rewrite_record`
/// recurses into struct members and remaps it, and `target_write::encode_value`
/// resolves its master index against the output plugin. Flattening here would
/// bake an unremapped FO4 FormID into the Starfield plugin. Mirrors the
/// `REFR.XLKR formid -> struct:I,I` case in `target_normalize.rs`.
fn sf_scol_onam_row(interner: &StringInterner, static_ref: FieldValue) -> FieldValue {
    let mut members = Vec::with_capacity(5);
    members.push((interner.intern("static"), static_ref));
    for name in SF_SCOL_ONAM_TRAILING_BYTES {
        members.push((
            interner.intern(name),
            FieldValue::Bytes(SmallVec::from_slice(&[0u8])),
        ));
    }
    FieldValue::Struct(members)
}

pub(super) fn relayout_scol_onam(interner: &StringInterner, record: &mut Record) {
    if record.sig.0 != *b"SCOL" {
        return;
    }
    rewrite_entries(record, b"ONAM", |value| match value {
        FieldValue::FormKey(fk) => {
            Relayout::Replace(sf_scol_onam_row(interner, FieldValue::FormKey(*fk)))
        }
        // A null part reference still has to occupy its 4-byte slot, and
        // `FieldValue::None` inside a struct encodes to zero bytes.
        FieldValue::None => Relayout::Replace(sf_scol_onam_row(
            interner,
            FieldValue::Bytes(SmallVec::from_slice(&[0u8; 4])),
        )),
        // Already Starfield-shaped (re-run safety): leave untouched.
        FieldValue::Bytes(raw) if raw.len() == SF_SCOL_ONAM_LEN => Relayout::Keep,
        FieldValue::Bytes(raw) if raw.len() == 4 => Relayout::Replace(sf_scol_onam_row(
            interner,
            FieldValue::Bytes(SmallVec::from_slice(raw)),
        )),
        // Anything else is uninterpretable; drop the row rather than emit a
        // malformed part.
        _ => Relayout::Remove,
    });
}

// ---------------------------------------------------------------------------
// SCOL.DATA: per-part placement, same layout both games, FO4 units -> metres
// ---------------------------------------------------------------------------

/// One `Placements` row: `wbVec3Pos` (3 × f32) + `wbVec3Rot` (3 × f32) +
/// `wbFloat('Scale')`.
///
/// FO4 and Starfield share a *single* xEdit definition for this subrecord —
/// `wbStaticPartPlacements` in `wbDefinitionsCommon.pas:790`, referenced
/// verbatim by `wbDefinitionsFO4.pas:16418` and `wbDefinitionsSF1.pas:18680`
/// (and by FO3/FNV/FO76/TES5). The generated schemas agree: both games declare
/// `array_struct:f,f,f,f,f,f,f` with fields `placements_position_{x,y,z}`,
/// `placements_{x,y,z}` (the rotation vector) and `placements_scale`. So the
/// byte layout is identical and the codec matches — only the position units
/// differ, which is exactly why the translation map carries this subrecord raw
/// and why it still needs rescaling here.
const SCOL_PLACEMENT_ROW_LEN: usize = 28;
const SCOL_PLACEMENT_POSITION_AXES: usize = 3;

/// Rescale only the position vector of every part placement.
///
/// Rotation is left alone: both games store radians (vanilla ranges measured at
/// ±π in Starfield and 0..2π in FO4 — same unit, different wrap convention),
/// and an angle has no length to scale. Per-part Scale is a dimensionless
/// multiplier (median 1.0 in both corpora) and must not be touched either —
/// scaling it would shrink each part's mesh on top of moving it.
///
/// **Not idempotent.** Unlike the other relayouts in this module there is no
/// length change to detect a second pass, so this must run exactly once per
/// record — i.e. only from `Fo4StarfieldHook::pre_translate`. Do not call it
/// from a fixup that re-runs the hook over an already-translated record.
pub(super) fn scale_scol_placements(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.is_empty() || raw.len() % SCOL_PLACEMENT_ROW_LEN != 0 {
        return None;
    }
    let mut out = raw.to_vec();
    for row in out.chunks_exact_mut(SCOL_PLACEMENT_ROW_LEN) {
        for axis in 0..SCOL_PLACEMENT_POSITION_AXES {
            let offset = axis * 4;
            let metres = f32_at(row, offset) * FO4_TO_SF_SPATIAL;
            row[offset..offset + 4].copy_from_slice(&metres.to_le_bytes());
        }
    }
    Some(out)
}

/// A payload that is not a whole number of rows is left **unscaled** rather
/// than dropped. `DATA` is `required: true` on both schemas, so removing it
/// would make the SCOL structurally invalid; a mis-scaled but well-formed
/// placement is the lesser failure.
pub(super) fn rescale_scol_placements(record: &mut Record) {
    if record.sig.0 != *b"SCOL" {
        return;
    }
    rewrite_entries(record, b"DATA", |value| match value {
        FieldValue::Bytes(raw) => match scale_scol_placements(raw) {
            Some(scaled) => Relayout::Replace(FieldValue::Bytes(SmallVec::from_vec(scaled))),
            None => Relayout::Keep,
        },
        _ => Relayout::Keep,
    });
}

// ---------------------------------------------------------------------------
// OBND: FO4 game units -> Starfield metres (same 12 B `struct:h,h,h,h,h,h`
// layout in both games, verified against both generated schemas).
//
// `fo4_to_starfield.yaml` declares a `scale_nested` transform for OBND on
// every record family below, but `scale_nested` only operates on a decoded
// `FieldValue::Struct` (transforms/scale_nested.rs) and OBND reaches the
// translator as raw `FieldValue::Bytes` — the map's own general struct-codec
// caveat (see this file's header comment) — so the transform silently no-ops
// and the map entries are dead. This hook owns the rescale instead, the same
// way it already owns `SCOL.DATA`'s position rescale above.
// ---------------------------------------------------------------------------

const OBND_LEN: usize = 12;
const OBND_AXES: usize = 6;

/// Record signatures the map declares an OBND `scale_nested` transform for.
const OBND_SCALED_SIGS: [[u8; 4]; 7] = [
    *b"STAT", *b"SCOL", *b"MSTT", *b"ACTI", *b"LIGH", *b"TXST", *b"ASPC",
];

/// Round-half-away-from-zero, saturating into `i16`'s range (an FO4 bound
/// this far from origin — over 32767 game units, ~468 m — is not a shape
/// this hook can represent losslessly; clamping beats silently wrapping).
fn scale_obnd_axis(value: i16) -> i16 {
    (f32::from(value) * FO4_TO_SF_SPATIAL).round() as i16
}

fn scale_object_bounds(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() != OBND_LEN {
        return None;
    }
    let mut out = raw.to_vec();
    for axis in 0..OBND_AXES {
        let offset = axis * 2;
        let scaled = scale_obnd_axis(i16_at(&out, offset));
        out[offset..offset + 2].copy_from_slice(&scaled.to_le_bytes());
    }
    Some(out)
}

/// **Not idempotent**, same caveat as `rescale_scol_placements`: there is no
/// length change to detect a second pass, so this must run exactly once per
/// record, only from `Fo4StarfieldHook::pre_translate`.
pub(super) fn rescale_object_bounds(record: &mut Record) {
    if !OBND_SCALED_SIGS.contains(&record.sig.0) {
        return;
    }
    rewrite_entries(record, b"OBND", |value| match value {
        FieldValue::Bytes(raw) => match scale_object_bounds(raw) {
            Some(scaled) => Relayout::Replace(FieldValue::Bytes(SmallVec::from_vec(scaled))),
            None => Relayout::Keep,
        },
        _ => Relayout::Keep,
    });
}

// ---------------------------------------------------------------------------
// WATR.DNAM: FO4 69-field struct (201 B) -> Starfield 41-field struct (152 B)
// ---------------------------------------------------------------------------

const FO4_WATR_DNAM_LEN: usize = 201;
const SF_WATR_DNAM_LEN: usize = 152;
/// The shortest vanilla FO4 variant (188 B) still ends after the noise-falloff
/// block, which is the last field this mapping reads.
const FO4_WATR_DNAM_MIN_LEN: usize = 188;

// Starfield-only WATR fields with no FO4 counterpart. Values are read off
// vanilla `Starfield.esm` `WaterClear` (000018); the three colour-absorption
// ranges are identical in 14 of the 15 vanilla WATR records, and flowmap scale
// is 1.0 in all 15.
const SF_WATR_ABSORPTION_RED: f32 = 0.165_580_00;
const SF_WATR_ABSORPTION_GREEN: f32 = 0.096_239_00;
const SF_WATR_ABSORPTION_BLUE: f32 = 0.076_271_00;
const SF_WATR_CONCENTRATION_PHYTOPLANKTON: f32 = 8.84;
const SF_WATR_CONCENTRATION_SEDIMENT: f32 = 6.594;
const SF_WATR_CONCENTRATION_YELLOW_MATTER: f32 = 4.71;
const SF_WATR_CONCENTRATION_OCEANNESS: f32 = 0.5145;
const SF_WATR_FLOWMAP_SCALE: f32 = 1.0;
const SF_WATR_ROUGHNESS: f32 = 0.08;

/// FO4 `WATR.DNAM` field offsets consumed by the mapping below. Derived from
/// the fo4.rs codec `f,B×8,f×6,B×4,f×14,B×4,f×23,B×8,B`.
mod fo4_watr {
    pub(super) const DEPTH_AMOUNT: usize = 0;
    pub(super) const UNDERWATER_COLOR: usize = 36; // 4 bytes RGBA
    pub(super) const UNDERWATER_FOG_AMOUNT: usize = 40;
    pub(super) const UNDERWATER_NEAR_FOG: usize = 44;
    pub(super) const UNDERWATER_FAR_FOG: usize = 48;
    pub(super) const NORMAL_MAGNITUDE: usize = 52;
    pub(super) const SHALLOW_NORMAL_FALLOFF: usize = 56;
    pub(super) const DEEP_NORMAL_FALLOFF: usize = 60;
    pub(super) const SURFACE_EFFECT_FALLOFF: usize = 72;
    pub(super) const DISPLACEMENT_BLOCK: usize = 76; // 5 × f32
    pub(super) const NOISE_WIND_DIRECTION: usize = 128; // 3 × f32
    pub(super) const NOISE_WIND_SPEED: usize = 140; // 3 × f32
    pub(super) const NOISE_AMPLITUDE_SCALE: usize = 152; // 3 × f32
    pub(super) const NOISE_UV_SCALE: usize = 164; // 3 × f32
    pub(super) const NOISE_FALLOFF: usize = 176; // 3 × f32
}

/// Punted FO4 fields (no Starfield destination, or no defensible conversion):
/// the shallow/deep fog colours and their colour/alpha ranges — Starfield
/// replaced that model with per-channel absorption ranges, which cannot be
/// derived from two RGB endpoints; Reflectivity Amount and Fresnel Amount;
/// Reflection Colour; the whole Specular Properties block (sun specular/sparkle
/// power and magnitude, interior specular radius/brightness/power); Silt
/// Amount and the two silt colours — Starfield's Sediment Concentration is on a
/// different scale (vanilla 5.9–19.3 vs FO4's 0.0/1.0), so it keeps the vanilla
/// default rather than a bogus rescale; and the Screen Space Reflections flag.
pub(super) fn fo4_watr_dnam_to_starfield(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() < FO4_WATR_DNAM_MIN_LEN {
        return None;
    }
    let src = fit(raw, FO4_WATR_DNAM_LEN);
    let mut out = Vec::with_capacity(SF_WATR_DNAM_LEN);

    // Depth Amount is a distance in game units.
    push_f32(
        &mut out,
        f32_at(&src, fo4_watr::DEPTH_AMOUNT) * FO4_TO_SF_SPATIAL,
    );
    push_f32(&mut out, SF_WATR_ABSORPTION_RED);
    push_f32(&mut out, SF_WATR_ABSORPTION_GREEN);
    push_f32(&mut out, SF_WATR_ABSORPTION_BLUE);
    push_f32(&mut out, SF_WATR_CONCENTRATION_PHYTOPLANKTON);
    push_f32(&mut out, SF_WATR_CONCENTRATION_SEDIMENT);
    push_f32(&mut out, SF_WATR_CONCENTRATION_YELLOW_MATTER);
    push_f32(&mut out, SF_WATR_CONCENTRATION_OCEANNESS);

    let uw = fo4_watr::UNDERWATER_COLOR;
    out.extend_from_slice(&src[uw..uw + 4]); // Underwater colour RGBA.
    out.extend_from_slice(&src[fo4_watr::UNDERWATER_FOG_AMOUNT..][..4]);
    // Near/far underwater fog are distances.
    push_f32(
        &mut out,
        f32_at(&src, fo4_watr::UNDERWATER_NEAR_FOG) * FO4_TO_SF_SPATIAL,
    );
    push_f32(
        &mut out,
        f32_at(&src, fo4_watr::UNDERWATER_FAR_FOG) * FO4_TO_SF_SPATIAL,
    );

    out.extend_from_slice(&src[fo4_watr::NORMAL_MAGNITUDE..][..4]);
    out.extend_from_slice(&src[fo4_watr::SHALLOW_NORMAL_FALLOFF..][..4]);
    out.extend_from_slice(&src[fo4_watr::DEEP_NORMAL_FALLOFF..][..4]);
    out.extend_from_slice(&src[fo4_watr::SURFACE_EFFECT_FALLOFF..][..4]);
    // Displacement simulator: force, velocity, falloff, dampener, starting
    // size — same five fields, same order, same units in both games.
    out.extend_from_slice(&src[fo4_watr::DISPLACEMENT_BLOCK..][..20]);

    // Noise layers 1-3: direction (degrees) and speed (rate) carry raw;
    // amplitude scale is a ratio; UV scale and falloff are world distances.
    out.extend_from_slice(&src[fo4_watr::NOISE_WIND_DIRECTION..][..12]);
    out.extend_from_slice(&src[fo4_watr::NOISE_WIND_SPEED..][..12]);
    out.extend_from_slice(&src[fo4_watr::NOISE_AMPLITUDE_SCALE..][..12]);
    for layer in 0..3 {
        push_f32(
            &mut out,
            f32_at(&src, fo4_watr::NOISE_UV_SCALE + layer * 4) * FO4_TO_SF_SPATIAL,
        );
    }
    for layer in 0..3 {
        push_f32(
            &mut out,
            f32_at(&src, fo4_watr::NOISE_FALLOFF + layer * 4) * FO4_TO_SF_SPATIAL,
        );
    }

    push_f32(&mut out, SF_WATR_FLOWMAP_SCALE);
    push_f32(&mut out, SF_WATR_ROUGHNESS);
    debug_assert_eq!(out.len(), SF_WATR_DNAM_LEN);
    Some(out)
}

pub(super) fn relayout_watr_dnam(record: &mut Record) {
    if record.sig.0 != *b"WATR" {
        return;
    }
    replace_or_remove(
        record,
        b"DNAM",
        SF_WATR_DNAM_LEN,
        fo4_watr_dnam_to_starfield,
    );
}

// ---------------------------------------------------------------------------
// LGTM.DATA: FO4 76-field struct (136 B) -> Starfield 108 B
// LGTM.DALC: FO4 29-field struct (32 B)  -> Starfield 24 B
// ---------------------------------------------------------------------------

const FO4_LGTM_DATA_LEN: usize = 136;
const SF_LGTM_DATA_LEN: usize = 108;
/// The shortest vanilla FO4 variant (92 B) ends at the 4-byte unused slot; the
/// two trailing colour fields it lacks are zero-filled.
const FO4_LGTM_DATA_MIN_LEN: usize = 92;
const FO4_LGTM_DALC_LEN: usize = 32;
const SF_LGTM_DALC_LEN: usize = 24;
const FO4_LGTM_DALC_MIN_LEN: usize = 24;

/// Offsets of the `LGTM.DATA` members that hold a world distance and therefore
/// need rescaling. Every other member is a colour, an integer rotation, or a
/// unit-less ratio (Directional Fade, Fog Power, Fog Max) and carries raw.
const LGTM_DATA_DISTANCE_OFFSETS: [usize; 7] = [
    12, // Fog Near
    16, // Fog Far
    32, // Fog Clip Distance
    80, // Light Fade Begin
    84, // Light Fade End
    92, // Near Height Mid
    96, // Near Height Range
];

/// Both `wbUnused` runs xEdit declares inside the struct (`SF1` and `FO4`
/// definitions agree on 32 bytes at +40 and 4 bytes at +88). Zeroed rather than
/// carried so no FO4 leftovers ride into the Starfield record.
const LGTM_DATA_UNUSED_RUNS: [(usize, usize); 2] = [(40, 32), (88, 4)];

/// Starfield's `LGTM.DATA` is byte-for-byte the first 108 bytes of FO4's — same
/// members, same order, same two `wbUnused` runs — and simply stops after Fog
/// Colour High Far. Verified three ways: the two generated schemas, the xEdit
/// `wbDefinitionsSF1.pas` / `wbDefinitionsFO4.pas` `LGTM` definitions, and the
/// vanilla payload lengths (108 B in all 6 `Starfield.esm` LGTMs).
///
/// The 7 FO4 members past the cut have no Starfield destination: High Density
/// Scale, Fog Near/Far Scale, Fog High Near/Far Scale, Far Height Mid and Far
/// Height Range.
pub(super) fn fo4_lgtm_data_to_starfield(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() < FO4_LGTM_DATA_MIN_LEN {
        return None;
    }
    let mut out = fit(&fit(raw, FO4_LGTM_DATA_LEN), SF_LGTM_DATA_LEN);
    for (offset, len) in LGTM_DATA_UNUSED_RUNS {
        out[offset..offset + len].fill(0);
    }
    for offset in LGTM_DATA_DISTANCE_OFFSETS {
        let scaled = f32_at(&out, offset) * FO4_TO_SF_SPATIAL;
        out[offset..offset + 4].copy_from_slice(&scaled.to_le_bytes());
    }
    Some(out)
}

/// Starfield's `DALC` keeps only the six directional ambient colours; FO4 adds
/// a Specular colour and a Fresnel Power float after them (xEdit
/// `wbAmbientColors`, which differs between the two definition files by exactly
/// those two members). Truncation is the whole conversion.
pub(super) fn fo4_lgtm_dalc_to_starfield(raw: &[u8]) -> Option<Vec<u8>> {
    if raw.len() < FO4_LGTM_DALC_MIN_LEN {
        return None;
    }
    Some(fit(&fit(raw, FO4_LGTM_DALC_LEN), SF_LGTM_DALC_LEN))
}

pub(super) fn relayout_lgtm_data(record: &mut Record) {
    if record.sig.0 != *b"LGTM" {
        return;
    }
    replace_or_remove(
        record,
        b"DATA",
        SF_LGTM_DATA_LEN,
        fo4_lgtm_data_to_starfield,
    );
}

pub(super) fn relayout_lgtm_dalc(record: &mut Record) {
    if record.sig.0 != *b"LGTM" {
        return;
    }
    replace_or_remove(
        record,
        b"DALC",
        SF_LGTM_DALC_LEN,
        fo4_lgtm_dalc_to_starfield,
    );
}

/// What `rewrite_entries` should do with one matched subrecord.
enum Relayout {
    Keep,
    Replace(FieldValue),
    Remove,
}

/// Apply `decide` to every entry whose sig is `sig`, preserving the position of
/// the entries it keeps or replaces. Order matters for `SCOL`, whose repeated
/// `ONAM`/`DATA` pairs are one part each.
fn rewrite_entries(record: &mut Record, sig: &[u8; 4], decide: impl Fn(&FieldValue) -> Relayout) {
    let mut kept: SmallVec<[FieldEntry; 8]> = SmallVec::new();
    for mut entry in record.fields.drain(..) {
        if entry.sig.0 != *sig {
            kept.push(entry);
            continue;
        }
        match decide(&entry.value) {
            Relayout::Keep => kept.push(entry),
            Relayout::Replace(value) => {
                entry.value = value;
                kept.push(entry);
            }
            Relayout::Remove => {}
        }
    }
    record.fields = kept;
}

/// Rewrite every `sig` entry through `convert`, dropping any payload it
/// rejects. A payload already at the Starfield length is left alone so a second
/// pass cannot re-truncate or re-scale it.
fn replace_or_remove(
    record: &mut Record,
    sig: &[u8; 4],
    target_len: usize,
    convert: fn(&[u8]) -> Option<Vec<u8>>,
) {
    rewrite_entries(record, sig, |value| match value {
        FieldValue::Bytes(raw) if raw.len() == target_len => Relayout::Keep,
        FieldValue::Bytes(raw) => match convert(raw) {
            Some(payload) => Relayout::Replace(FieldValue::Bytes(SmallVec::from_vec(payload))),
            None => Relayout::Remove,
        },
        _ => Relayout::Remove,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};

    fn new_record(sig: &str, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey::parse("000800@Fallout4.esm", interner).unwrap(),
        )
    }

    fn push_bytes(record: &mut Record, sig: &str, payload: Vec<u8>) {
        record.fields.push(bytes_field(sig, payload));
    }

    fn find<'a>(record: &'a Record, sig: &str) -> Option<&'a FieldValue> {
        record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == sig)
            .map(|e| &e.value)
    }

    fn raw_of<'a>(record: &'a Record, sig: &str) -> &'a [u8] {
        match find(record, sig).expect("subrecord present") {
            FieldValue::Bytes(raw) => raw,
            other => panic!("{sig} should be raw Bytes, got {other:?}"),
        }
    }

    fn f32s(raw: &[u8]) -> Vec<f32> {
        raw.chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect()
    }

    /// Vanilla `Fallout4.esm` shape: Time -1, Radius 256 units (64% of sampled
    /// lights), warm white, flags 0x0009 (Unknown 0 | Flicker), falloff 1.0,
    /// FOV 90, near clip 7.2174 (48%), flicker 1.0/0.1/0.0, then the four
    /// god-ray floats, Value and Weight.
    fn vanilla_fo4_light_data() -> Vec<u8> {
        let mut d = Vec::new();
        d.extend_from_slice(&(-1i32).to_le_bytes());
        d.extend_from_slice(&256u32.to_le_bytes());
        d.extend_from_slice(&[255, 214, 170, 0]);
        d.extend_from_slice(&0x0000_0009u32.to_le_bytes());
        for v in [1.0f32, 90.0, 7.2174, 1.0, 0.1, 0.0, 0.0, 1.0, 2.0, 10.0] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&0u32.to_le_bytes()); // Value
        d.extend_from_slice(&0.0f32.to_le_bytes()); // Weight
        assert_eq!(d.len(), FO4_LIGH_DATA_LEN);
        d
    }

    #[test]
    fn ligh_data_becomes_starfield_dat2_byte_for_byte() {
        let interner = StringInterner::new();
        let mut rec = new_record("LIGH", &interner);
        push_bytes(&mut rec, "DATA", vanilla_fo4_light_data());

        relayout_ligh_data_to_dat2(&mut rec);

        assert!(find(&rec, "DATA").is_none(), "FO4 DATA must not survive");
        let out = raw_of(&rec, "DAT2");
        assert_eq!(out.len(), SF_LIGH_DAT2_LEN);

        let mut expected = Vec::new();
        expected.extend_from_slice(&(-1i32).to_le_bytes());
        expected.extend_from_slice(&(256.0f32 * FO4_TO_SF_SPATIAL).to_le_bytes());
        expected.extend_from_slice(&[255, 214, 170, 0]);
        // FO4 0x0009 = Unknown 0 | Flicker. Flicker is not a Starfield flag
        // bit, so only Unknown 0 survives as 0x0001.
        expected.extend_from_slice(&0x0001u16.to_le_bytes());
        expected.extend_from_slice(&[0, 0]);
        expected.extend_from_slice(&1.0f32.to_le_bytes()); // falloff
        expected.extend_from_slice(&90.0f32.to_le_bytes()); // fov
        expected.extend_from_slice(&(7.2174f32 * FO4_TO_SF_SPATIAL).to_le_bytes());
        for v in [1.0f32, 0.1, 0.0] {
            expected.extend_from_slice(&v.to_le_bytes());
        }
        expected.extend_from_slice(&0.0f32.to_le_bytes()); // shadow offset
        expected.extend_from_slice(&0.0f32.to_le_bytes()); // inner FOV
        expected.extend_from_slice(&SF_LIGH_PBR_TEMPERATURE_DEFAULT.to_le_bytes());
        expected.extend_from_slice(&0u32.to_le_bytes()); // lumens
        expected.extend_from_slice(&[SF_LIGHT_TYPE_OMNIDIRECTIONAL, SF_FLICKER_FLICKER, 0, 0]);
        expected.extend_from_slice(&0.0f32.to_le_bytes());
        expected.extend_from_slice(&(-4.0f32).to_le_bytes());
        expected.extend_from_slice(&14.0f32.to_le_bytes());
        expected.extend_from_slice(&4.0f32.to_le_bytes());
        assert_eq!(out, expected.as_slice());
    }

    #[test]
    fn ligh_radius_and_near_clip_land_in_vanilla_starfield_range() {
        // 256 units -> 3.66 m and 7.2174 -> 0.103 m, against vanilla Starfield
        // radii of 3-10 m and a dominant near clip of 0.15 m.
        let out = fo4_ligh_data_to_starfield_dat2(&vanilla_fo4_light_data()).unwrap();
        let radius = f32::from_le_bytes(out[4..8].try_into().unwrap());
        let near_clip = f32::from_le_bytes(out[24..28].try_into().unwrap());
        assert!((3.0..11.0).contains(&radius), "radius {radius}");
        assert!((0.05..0.30).contains(&near_clip), "near clip {near_clip}");
    }

    #[test]
    fn ligh_shadow_spotlight_and_pulse_move_into_their_enum_bytes() {
        let mut data = vanilla_fo4_light_data();
        let flags = FO4_LIGH_SHADOW_SPOTLIGHT
            | FO4_LIGH_PULSE
            | FO4_LIGH_CAN_BE_CARRIED
            | FO4_LIGH_OFF_BY_DEFAULT
            | FO4_LIGH_NON_SPECULAR;
        data[fo4_ligh::FLAGS..fo4_ligh::FLAGS + 4].copy_from_slice(&flags.to_le_bytes());

        let out = fo4_ligh_data_to_starfield_dat2(&data).unwrap();

        assert_eq!(
            u16::from_le_bytes(out[12..14].try_into().unwrap()),
            SF_LIGH_UNKNOWN_0
                | SF_LIGH_CAN_BE_CARRIED
                | SF_LIGH_OFF_BY_DEFAULT
                | SF_LIGH_DISABLE_SPECULAR
        );
        assert_eq!(out[56], SF_LIGHT_TYPE_SHADOW_SPOTLIGHT);
        assert_eq!(out[57], SF_FLICKER_PULSE);
    }

    #[test]
    fn ligh_short_vanilla_variant_is_zero_padded_not_dropped() {
        // 60-byte DATA is 18% of vanilla Fallout4.esm lights.
        let mut data = vanilla_fo4_light_data();
        data.truncate(60);
        let out = fo4_ligh_data_to_starfield_dat2(&data).unwrap();
        assert_eq!(out.len(), SF_LIGH_DAT2_LEN);
    }

    #[test]
    fn ligh_unusable_data_is_removed_never_carried() {
        let interner = StringInterner::new();
        let mut rec = new_record("LIGH", &interner);
        push_bytes(&mut rec, "DATA", vec![1, 2, 3, 4]);

        relayout_ligh_data_to_dat2(&mut rec);

        assert!(rec.fields.is_empty());
    }

    #[test]
    fn scol_onam_gains_four_zero_bytes_and_keeps_a_live_formkey() {
        let interner = StringInterner::new();
        let mut rec = new_record("SCOL", &interner);
        let part = FormKey::parse("0247C1@Fallout4.esm", &interner).unwrap();
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ONAM").unwrap(),
            value: FieldValue::FormKey(part),
        });

        relayout_scol_onam(&interner, &mut rec);

        let FieldValue::Struct(members) = find(&rec, "ONAM").unwrap() else {
            panic!("ONAM must become a struct so the FormKey stays remappable");
        };
        assert_eq!(members.len(), 5);
        assert_eq!(members[0].1, FieldValue::FormKey(part));
        for member in &members[1..] {
            assert_eq!(member.1, FieldValue::Bytes(SmallVec::from_slice(&[0u8])));
        }
    }

    #[test]
    fn scol_onam_rows_stay_paired_with_their_data_rows() {
        let interner = StringInterner::new();
        let mut rec = new_record("SCOL", &interner);
        for local in ["0247C1", "0247C4"] {
            rec.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("ONAM").unwrap(),
                value: FieldValue::FormKey(
                    FormKey::parse(&format!("{local}@Fallout4.esm"), &interner).unwrap(),
                ),
            });
            push_bytes(&mut rec, "DATA", vec![0u8; 28]);
        }

        relayout_scol_onam(&interner, &mut rec);

        let order: Vec<&str> = rec.fields.iter().map(|e| e.sig.as_str()).collect();
        assert_eq!(order, vec!["ONAM", "DATA", "ONAM", "DATA"]);
    }

    #[test]
    fn scol_onam_already_starfield_shaped_is_left_alone() {
        let interner = StringInterner::new();
        let mut rec = new_record("SCOL", &interner);
        push_bytes(&mut rec, "ONAM", vec![1, 2, 3, 4, 0, 0, 0, 0]);

        relayout_scol_onam(&interner, &mut rec);

        assert_eq!(raw_of(&rec, "ONAM"), &[1, 2, 3, 4, 0, 0, 0, 0]);
    }

    /// One FO4 `SCOL` part placement: position in game units, rotation in
    /// radians, per-part scale.
    fn fo4_placement_row(pos: [f32; 3], rot: [f32; 3], scale: f32) -> Vec<u8> {
        let mut row = Vec::with_capacity(SCOL_PLACEMENT_ROW_LEN);
        for v in pos.iter().chain(rot.iter()).chain(std::iter::once(&scale)) {
            row.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(row.len(), SCOL_PLACEMENT_ROW_LEN);
        row
    }

    #[test]
    fn scol_placement_scales_position_only_byte_for_byte() {
        // Magnitudes taken from the vanilla Fallout4.esm SCOL corpus (median
        // |position| 142 units, p90 844).
        let src = fo4_placement_row([256.0, -512.0, 128.0], [0.5, -1.25, 3.0], 2.0);

        let out = scale_scol_placements(&src).unwrap();

        let mut expected = Vec::new();
        for v in [256.0f32, -512.0, 128.0] {
            expected.extend_from_slice(&(v * FO4_TO_SF_SPATIAL).to_le_bytes());
        }
        // Rotation (radians) and per-part Scale (dimensionless) carry verbatim.
        expected.extend_from_slice(&src[12..28]);
        assert_eq!(out, expected);
        assert_eq!(out.len(), SCOL_PLACEMENT_ROW_LEN);
    }

    #[test]
    fn scol_placement_scales_every_row_of_a_multi_part_collection() {
        let mut src = fo4_placement_row([100.0, 200.0, 300.0], [0.0, 0.0, 0.0], 1.0);
        src.extend(fo4_placement_row([-400.0, 0.0, 50.0], [1.0, 2.0, 3.0], 0.5));

        let out = scale_scol_placements(&src).unwrap();

        let v = f32s(&out);
        assert_eq!(v.len(), 14);
        assert_eq!(
            &v[0..3],
            &[
                100.0 * FO4_TO_SF_SPATIAL,
                200.0 * FO4_TO_SF_SPATIAL,
                300.0 * FO4_TO_SF_SPATIAL
            ]
        );
        assert_eq!(&v[3..7], &[0.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            &v[7..10],
            &[-400.0 * FO4_TO_SF_SPATIAL, 0.0, 50.0 * FO4_TO_SF_SPATIAL]
        );
        assert_eq!(&v[10..14], &[1.0, 2.0, 3.0, 0.5]);
    }

    #[test]
    fn scol_placement_lands_in_the_vanilla_starfield_magnitude_range() {
        // Vanilla Starfield SCOL placements measure |pos| p50 0.19 m, p90 2.58 m,
        // max 11.58 m across 200 rows. The FO4 p90 of 844 units must land in
        // metres, not stay at 844.
        let out = scale_scol_placements(&fo4_placement_row(
            [844.0, 142.0, 0.0],
            [0.0, 0.0, 0.0],
            1.0,
        ))
        .unwrap();
        let v = f32s(&out);
        assert!((10.0..14.0).contains(&v[0]), "p90 offset {} m", v[0]);
        assert!((1.0..3.0).contains(&v[1]), "median offset {} m", v[1]);
    }

    #[test]
    fn scol_placement_runs_from_the_record_entry_point() {
        let interner = StringInterner::new();
        let mut rec = new_record("SCOL", &interner);
        push_bytes(
            &mut rec,
            "DATA",
            fo4_placement_row([700.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0),
        );

        rescale_scol_placements(&mut rec);

        let v = f32s(raw_of(&rec, "DATA"));
        assert_eq!(v[0], 700.0 * FO4_TO_SF_SPATIAL);
    }

    #[test]
    fn scol_placement_with_a_ragged_payload_is_kept_unscaled_not_dropped() {
        let interner = StringInterner::new();
        let mut rec = new_record("SCOL", &interner);
        let ragged = vec![9u8; SCOL_PLACEMENT_ROW_LEN + 3];
        push_bytes(&mut rec, "DATA", ragged.clone());

        rescale_scol_placements(&mut rec);

        // DATA is required:true on both schemas — a malformed row count must not
        // cost the record its placements.
        assert_eq!(raw_of(&rec, "DATA"), ragged.as_slice());
    }

    #[test]
    fn scol_placement_is_untouched_on_other_record_signatures() {
        let interner = StringInterner::new();
        let mut rec = new_record("STAT", &interner);
        let row = fo4_placement_row([256.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0);
        push_bytes(&mut rec, "DATA", row.clone());

        rescale_scol_placements(&mut rec);

        assert_eq!(raw_of(&rec, "DATA"), row.as_slice());
    }

    // -----------------------------------------------------------------
    // OBND (Object Bounds) rescale
    // -----------------------------------------------------------------

    fn obnd_row(bounds: [i16; 6]) -> Vec<u8> {
        let mut row = Vec::new();
        for v in bounds {
            row.extend_from_slice(&v.to_le_bytes());
        }
        row
    }

    fn i16s(raw: &[u8]) -> Vec<i16> {
        raw.chunks_exact(2)
            .map(|c| i16::from_le_bytes(c.try_into().unwrap()))
            .collect()
    }

    #[test]
    fn obnd_scales_every_bound_with_exact_rounding() {
        // 700 * 0.0142875 = 10.00125 -> rounds to 10 exactly.
        let src = obnd_row([-700, -700, 0, 700, 700, 1400]);

        let out = scale_object_bounds(&src).unwrap();

        assert_eq!(out.len(), OBND_LEN);
        assert_eq!(i16s(&out), vec![-10, -10, 0, 10, 10, 20]);
    }

    #[test]
    fn obnd_rejects_a_payload_that_is_not_twelve_bytes() {
        assert!(scale_object_bounds(&[0u8; 10]).is_none());
        assert!(scale_object_bounds(&[0u8; 14]).is_none());
    }

    #[test]
    fn obnd_scale_runs_from_the_record_entry_point_for_every_declared_sig() {
        // Every sig `fo4_to_starfield.yaml` declared (now dead) `scale_nested`
        // OBND transforms for -- STAT, SCOL, MSTT, ACTI, LIGH, TXST, ASPC.
        let interner = StringInterner::new();
        for sig in ["STAT", "SCOL", "MSTT", "ACTI", "LIGH", "TXST", "ASPC"] {
            let mut rec = new_record(sig, &interner);
            push_bytes(&mut rec, "OBND", obnd_row([-70, -70, 0, 70, 70, 140]));

            rescale_object_bounds(&mut rec);

            let v = i16s(raw_of(&rec, "OBND"));
            assert_eq!(v, vec![-1, -1, 0, 1, 1, 2], "sig {sig}");
        }
    }

    #[test]
    fn obnd_scale_is_untouched_on_a_signature_the_map_never_declared_it_for() {
        let interner = StringInterner::new();
        let mut rec = new_record("WATR", &interner);
        let row = obnd_row([-70, -70, 0, 70, 70, 140]);
        push_bytes(&mut rec, "OBND", row.clone());

        rescale_object_bounds(&mut rec);

        assert_eq!(raw_of(&rec, "OBND"), row.as_slice());
    }

    /// A synthetic FO4 `WATR.DNAM` whose every f32 slot holds its own field
    /// index, so a mis-mapped offset shows up as the wrong number.
    fn indexed_fo4_water_dnam() -> Vec<u8> {
        let mut d = vec![0u8; FO4_WATR_DNAM_LEN];
        let float_offsets = [0usize, 12, 16, 20, 24, 28, 32]
            .into_iter()
            .chain((40..96).step_by(4))
            .chain((100..192).step_by(4));
        for offset in float_offsets {
            d[offset..offset + 4].copy_from_slice(&(offset as f32).to_le_bytes());
        }
        d[36..40].copy_from_slice(&[11, 22, 33, 44]); // underwater colour RGBA
        d
    }

    #[test]
    fn watr_dnam_maps_underwater_surface_and_noise_blocks() {
        let src = indexed_fo4_water_dnam();
        let out = fo4_watr_dnam_to_starfield(&src).unwrap();
        assert_eq!(out.len(), SF_WATR_DNAM_LEN);
        let v = f32s(&out);

        assert_eq!(v[0], 0.0 * FO4_TO_SF_SPATIAL); // depth amount (fo4 +0)
        assert_eq!(v[1], SF_WATR_ABSORPTION_RED);
        assert_eq!(v[2], SF_WATR_ABSORPTION_GREEN);
        assert_eq!(v[3], SF_WATR_ABSORPTION_BLUE);
        assert_eq!(v[4], SF_WATR_CONCENTRATION_PHYTOPLANKTON);
        assert_eq!(v[7], SF_WATR_CONCENTRATION_OCEANNESS);
        assert_eq!(&out[32..36], &[11, 22, 33, 44]); // underwater colour
        assert_eq!(v[9], 40.0); // underwater fog amount (fo4 +40), unscaled
        assert_eq!(v[10], 44.0 * FO4_TO_SF_SPATIAL); // near fog (fo4 +44)
        assert_eq!(v[11], 48.0 * FO4_TO_SF_SPATIAL); // far fog (fo4 +48)
        assert_eq!(v[12], 52.0); // normal magnitude (fo4 +52)
        assert_eq!(v[13], 56.0); // shallow normal falloff (fo4 +56)
        assert_eq!(v[14], 60.0); // deep normal falloff (fo4 +60)
        assert_eq!(v[15], 72.0); // surface effect falloff (fo4 +72)
        assert_eq!(&v[16..21], &[76.0, 80.0, 84.0, 88.0, 92.0]); // displacement
        assert_eq!(&v[21..24], &[128.0, 132.0, 136.0]); // noise wind direction
        assert_eq!(&v[24..27], &[140.0, 144.0, 148.0]); // noise wind speed
        assert_eq!(&v[27..30], &[152.0, 156.0, 160.0]); // noise amplitude
        assert_eq!(
            &v[30..33],
            &[
                164.0 * FO4_TO_SF_SPATIAL,
                168.0 * FO4_TO_SF_SPATIAL,
                172.0 * FO4_TO_SF_SPATIAL
            ]
        );
        assert_eq!(
            &v[33..36],
            &[
                176.0 * FO4_TO_SF_SPATIAL,
                180.0 * FO4_TO_SF_SPATIAL,
                184.0 * FO4_TO_SF_SPATIAL
            ]
        );
        assert_eq!(v[36], SF_WATR_FLOWMAP_SCALE);
        assert_eq!(v[37], SF_WATR_ROUGHNESS);
    }

    #[test]
    fn watr_dnam_short_vanilla_variant_still_converts() {
        let mut src = indexed_fo4_water_dnam();
        src.truncate(188);
        let out = fo4_watr_dnam_to_starfield(&src).unwrap();
        assert_eq!(out.len(), SF_WATR_DNAM_LEN);
    }

    #[test]
    fn watr_dnam_replaces_the_field_in_place() {
        let interner = StringInterner::new();
        let mut rec = new_record("WATR", &interner);
        push_bytes(&mut rec, "DNAM", indexed_fo4_water_dnam());

        relayout_watr_dnam(&mut rec);

        assert_eq!(raw_of(&rec, "DNAM").len(), SF_WATR_DNAM_LEN);
    }

    #[test]
    fn watr_dnam_too_short_is_removed() {
        let interner = StringInterner::new();
        let mut rec = new_record("WATR", &interner);
        push_bytes(&mut rec, "DNAM", vec![0u8; 100]);

        relayout_watr_dnam(&mut rec);

        assert!(rec.fields.is_empty());
    }

    fn indexed_fo4_lgtm_data() -> Vec<u8> {
        let mut d = vec![0u8; FO4_LGTM_DATA_LEN];
        for offset in (0..FO4_LGTM_DATA_LEN).step_by(4) {
            d[offset..offset + 4].copy_from_slice(&(offset as f32).to_le_bytes());
        }
        d[0..12].copy_from_slice(&[1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0]); // colours
        d[20..28].copy_from_slice(&[0, 0, 0, 0, 90, 0, 0, 0]); // rotations XY, Z
        d
    }

    #[test]
    fn lgtm_data_truncates_to_108_and_rescales_only_distances() {
        let out = fo4_lgtm_data_to_starfield(&indexed_fo4_lgtm_data()).unwrap();
        assert_eq!(out.len(), SF_LGTM_DATA_LEN);

        assert_eq!(&out[0..12], &[1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9, 0]);
        assert_eq!(i32::from_le_bytes(out[20..24].try_into().unwrap()), 0);
        assert_eq!(i32::from_le_bytes(out[24..28].try_into().unwrap()), 90);
        for offset in LGTM_DATA_DISTANCE_OFFSETS {
            assert_eq!(
                f32_at(&out, offset),
                offset as f32 * FO4_TO_SF_SPATIAL,
                "offset {offset} should be rescaled"
            );
        }
        // Directional Fade, Fog Power, Fog Max carry raw.
        for offset in [28usize, 36, 76] {
            assert_eq!(f32_at(&out, offset), offset as f32, "offset {offset}");
        }
        for (offset, len) in LGTM_DATA_UNUSED_RUNS {
            assert!(out[offset..offset + len].iter().all(|b| *b == 0));
        }
        // Fog Colour High Near / High Far are the last two members and survive.
        assert_eq!(&out[100..108], &indexed_fo4_lgtm_data()[100..108]);
    }

    #[test]
    fn lgtm_data_short_vanilla_variant_still_converts() {
        let mut src = indexed_fo4_lgtm_data();
        src.truncate(92);
        let out = fo4_lgtm_data_to_starfield(&src).unwrap();
        assert_eq!(out.len(), SF_LGTM_DATA_LEN);
        assert_eq!(&out[92..108], &[0u8; 16]);
    }

    #[test]
    fn lgtm_dalc_keeps_only_the_six_directional_colours() {
        let mut src = vec![0u8; FO4_LGTM_DALC_LEN];
        for (i, byte) in src.iter_mut().enumerate() {
            *byte = i as u8;
        }
        let out = fo4_lgtm_dalc_to_starfield(&src).unwrap();
        assert_eq!(out.len(), SF_LGTM_DALC_LEN);
        assert_eq!(out.as_slice(), &src[..SF_LGTM_DALC_LEN]);
    }

    #[test]
    fn lgtm_relayouts_run_together_on_one_record() {
        let interner = StringInterner::new();
        let mut rec = new_record("LGTM", &interner);
        push_bytes(&mut rec, "DATA", indexed_fo4_lgtm_data());
        push_bytes(&mut rec, "DALC", vec![7u8; FO4_LGTM_DALC_LEN]);

        relayout_lgtm_data(&mut rec);
        relayout_lgtm_dalc(&mut rec);

        assert_eq!(raw_of(&rec, "DATA").len(), SF_LGTM_DATA_LEN);
        assert_eq!(raw_of(&rec, "DALC").len(), SF_LGTM_DALC_LEN);
    }

    #[test]
    fn relayouts_are_idempotent_on_already_converted_payloads() {
        let interner = StringInterner::new();
        let mut rec = new_record("LGTM", &interner);
        push_bytes(&mut rec, "DATA", indexed_fo4_lgtm_data());
        push_bytes(&mut rec, "DALC", vec![7u8; FO4_LGTM_DALC_LEN]);

        relayout_lgtm_data(&mut rec);
        relayout_lgtm_dalc(&mut rec);
        let first_data = raw_of(&rec, "DATA").to_vec();
        let first_dalc = raw_of(&rec, "DALC").to_vec();

        relayout_lgtm_data(&mut rec);
        relayout_lgtm_dalc(&mut rec);

        assert_eq!(raw_of(&rec, "DATA"), first_data.as_slice());
        assert_eq!(raw_of(&rec, "DALC"), first_dalc.as_slice());
    }

    #[test]
    fn other_record_signatures_are_untouched() {
        let interner = StringInterner::new();
        let mut rec = new_record("STAT", &interner);
        push_bytes(&mut rec, "DATA", vec![9u8; 64]);
        push_bytes(&mut rec, "DNAM", vec![9u8; 201]);

        relayout_ligh_data_to_dat2(&mut rec);
        relayout_watr_dnam(&mut rec);
        relayout_lgtm_data(&mut rec);
        relayout_lgtm_dalc(&mut rec);
        relayout_scol_onam(&interner, &mut rec);

        assert_eq!(raw_of(&rec, "DATA").len(), 64);
        assert_eq!(raw_of(&rec, "DNAM").len(), 201);
    }
}
