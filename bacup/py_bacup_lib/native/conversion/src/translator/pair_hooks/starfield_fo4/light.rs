//! Starfield `LIGH.DAT2` (76 bytes) as FO4's required `DATA` (64 bytes at form
//! version 131) and `FNAM` fade.
//!
//! Distances are Starfield metres. The byte colour is kept as authored: with
//! Use PBR Value set, the Creation Kit already stores the temperature's
//! blackbody RGB (3000 K is 255,177,109), while other lights pair the 5200 K
//! default with an unrelated RGB. Starfield brightness is luminous power with
//! inverse-square falloff, while an FO4 light is as bright as its fade out to
//! its radius. The lumens are therefore left in FO4's item `Value` slot (no
//! Starfield light can be carried) for `fixups::normalize_light_radii`, which
//! derives the falloff radius from them and rescales the placed `XRDS` deltas
//! in lockstep, exactly as it does for FO76. `FNAM` stays at FO4's 1.0 default,
//! which `placed_reference` assumes when it turns `XLIG` luminous scale into a
//! fade delta.

use super::STARFIELD_METERS_TO_FO4_UNITS;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

const STARFIELD_DAT2_LEN: usize = 76;
const FO4_DATA_LEN: usize = 64;

/// Unknown 0 and Can be Carried are bits 0 and 1 in both games.
const SHARED_FLAGS: u16 = 0x0003;
const SF_OFF_BY_DEFAULT: u16 = 0x0010;
const SF_DISABLE_SPECULAR: u16 = 0x0020;
const SF_DISABLE_DISTANCE_ATTENUATION: u16 = 0x0040;
const SF_SHADOW_SPOTLIGHT: u8 = 1;
const SF_NON_SHADOW_SPOTLIGHT: u8 = 2;
const SF_FLICKER: u8 = 1;
const SF_PULSE: u8 = 2;

const FO4_FLICKER: u32 = 0x0008;
const FO4_OFF_BY_DEFAULT: u32 = 0x0020;
const FO4_PULSE: u32 = 0x0080;
const FO4_SHADOW_SPOTLIGHT: u32 = 0x0400;
const FO4_NON_SHADOW_SPOTLIGHT: u32 = 0x4000;
const FO4_NON_SPECULAR: u32 = 0x8000;

/// `normalize_light_radii`'s ceiling. Clamping first keeps a 100 km Starfield
/// space light a sane FO4 radius even where that pass does not run.
const FO4_MAX_RADIUS: f32 = 2048.0;
/// FO4 renders a full shadow map per shadow caster in range; one vanilla shadow
/// light exceeds 1024, the ceiling the FO76 hook also applies.
const FO4_MAX_SHADOW_CASTER_RADIUS: f32 = 1024.0;
/// The smallest near clip vanilla `Fallout4.esm` ships (174 of 801 lights);
/// Starfield's 0.001 m would be 0.07 units and starve shadow depth precision.
const FO4_MIN_NEAR_CLIP: f32 = 1.0;
/// Most common vanilla values for the FO4-only members and the fade.
const FO4_DEFAULT_SCALAR: f32 = 1.0;
const FO4_DEFAULT_EXPONENT: f32 = 2.0;
const FO4_DEFAULT_FADE: f32 = 1.0;

pub(crate) fn normalize(record: &mut Record, _interner: &StringInterner) {
    if record.sig.0 != *b"LIGH" {
        return;
    }
    let Some(index) = record
        .fields
        .iter()
        .position(|field| field.sig.0 == *b"DAT2")
    else {
        return;
    };
    let FieldValue::Bytes(dat2) = &record.fields[index].value else {
        return;
    };
    if dat2.len() != STARFIELD_DAT2_LEN {
        return;
    }
    let data = fo4_data(dat2);
    record.fields[index] = FieldEntry {
        sig: SubrecordSig(*b"DATA"),
        value: FieldValue::Bytes(data.into_iter().collect()),
    };
    record.fields.insert(
        index + 1,
        FieldEntry {
            sig: SubrecordSig(*b"FNAM"),
            value: FieldValue::Float(FO4_DEFAULT_FADE),
        },
    );
}

fn fo4_data(source: &[u8]) -> Vec<u8> {
    let float = |offset: usize| f32::from_le_bytes(source[offset..offset + 4].try_into().unwrap());
    let flags = u16::from_le_bytes([source[12], source[13]]);
    let lumens = u32::from_le_bytes(source[52..56].try_into().unwrap());
    let light_type = source[56];
    let max_radius = if light_type == SF_SHADOW_SPOTLIGHT {
        FO4_MAX_SHADOW_CASTER_RADIUS
    } else {
        FO4_MAX_RADIUS
    };
    // A light without distance attenuation already stays bright to its radius,
    // as every FO4 light does, so it gets no lumens-derived reach.
    let reach_lumens = if flags & SF_DISABLE_DISTANCE_ATTENUATION == 0 {
        lumens
    } else {
        0
    };

    let mut data = Vec::with_capacity(FO4_DATA_LEN);
    data.extend_from_slice(&source[0..4]);
    let radius = (float(4) * STARFIELD_METERS_TO_FO4_UNITS)
        .min(max_radius)
        .round() as u32;
    data.extend_from_slice(&radius.to_le_bytes());
    data.extend_from_slice(&source[8..11]);
    data.push(0);
    data.extend_from_slice(&fo4_flags(flags, light_type, source[57]).to_le_bytes());
    // Falloff exponent and FOV (degrees).
    data.extend_from_slice(&source[16..24]);
    push_f32(
        &mut data,
        (float(24) * STARFIELD_METERS_TO_FO4_UNITS).max(FO4_MIN_NEAR_CLIP),
    );
    // Flicker period (seconds) and intensity amplitude; movement is a distance.
    data.extend_from_slice(&source[28..36]);
    push_f32(&mut data, float(36) * STARFIELD_METERS_TO_FO4_UNITS);
    push_f32(&mut data, 0.0);
    push_f32(&mut data, FO4_DEFAULT_SCALAR);
    push_f32(&mut data, FO4_DEFAULT_EXPONENT);
    push_f32(&mut data, 0.0);
    data.extend_from_slice(&reach_lumens.to_le_bytes());
    push_f32(&mut data, 0.0);
    data
}

fn push_f32(data: &mut Vec<u8>, value: f32) {
    data.extend_from_slice(&value.to_le_bytes());
}

/// Starfield-only bits (Disable Distance Attenuation, Is Direct Spotlight, Use
/// PBR Value, Focus Spotlight Beam, Externally Controlled) are dropped. Omni
/// lights cast no shadow in Starfield, so no FO4 hemisphere or omni shadow bit.
fn fo4_flags(flags: u16, light_type: u8, flicker_effect: u8) -> u32 {
    let mut fo4 = u32::from(flags & SHARED_FLAGS);
    if flags & SF_OFF_BY_DEFAULT != 0 {
        fo4 |= FO4_OFF_BY_DEFAULT;
    }
    if flags & SF_DISABLE_SPECULAR != 0 {
        fo4 |= FO4_NON_SPECULAR;
    }
    fo4 |= match light_type {
        SF_SHADOW_SPOTLIGHT => FO4_SHADOW_SPOTLIGHT,
        SF_NON_SHADOW_SPOTLIGHT => FO4_NON_SHADOW_SPOTLIGHT,
        _ => 0,
    };
    fo4 |= match flicker_effect {
        SF_FLICKER => FO4_FLICKER,
        SF_PULSE => FO4_PULSE,
        _ => 0,
    };
    fo4
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::ids::{FormKey, SigCode};

    /// Shipped `Starfield.esm` LIGH `DAT2` payloads.
    /// 0027B9 `LGT_SpaceStation_Spot_SS_Red_003_1k`: shadow spotlight, 5 m, 1000 lm.
    const SHADOW_SPOT_RED_1K: &str = "ffffffff0000a040b60a0a00010000000000803f0000b442cdcccc3d000000000000000000000000000000000000000050140000e80300000100010000000000000060400000604000008040";
    /// 33253E `LGT_Interior_Spot_SS_Warm_015_8k`: Use PBR Value, 3000 K, 8000 lm.
    const SHADOW_SPOT_PBR_3000K: &str = "ffffffff00000041ffb16d00010100000000803f0000f0429a99193e0000000000000000000000000000000000000000b80b0000401f0000010000ff00000000000080c00000604100008040";
    /// 04D330 `LGT_Interior_Omni_NS_Warm_003_8k`: omni with Disable Specular.
    const OMNI_NO_SPECULAR_8K: &str = "ffffffff00000040fee9a90021000000000000400000f0420000803e000000000000000000000000000000000000000000002041401f00000000000000000000000080c00000604100008040";
    /// 00B8BE `LGT_MemorialFire`: flickering omni with 0.12 m movement, flags 0.
    const MEMORIAL_FIRE: &str = "ffffffff3333d340c6401300000000000000803f0000b442cdcccc3d0000003f0000803f8fc2f53d000000000000000000000000a8610000000100ab00000000000080c0000060410000e040";
    /// 0940AE `LGT_ShipInterior_Omni_NS_Neutral_50k`: pulsing omni.
    const PULSE_OMNI_50K: &str = "ffffffff00000041ffffff000100000000000040000016439a99193e0000803f0000803f0000000000000000000000000000204150c30000000201ab00000000000060400000604000008040";
    /// 127E90 `LGT_Interior_Spot_NS_Warm_003_Flicker_4k`: flickering non-shadow spotlight.
    const FLICKER_NON_SHADOW_SPOT_4K: &str = "ffffffff00000041fee9a90001000000000000400000f0420000803e0000803f0000803f00000000000000000000000000002041a00f00000201000000000000000080c00000604100008040";
    /// 01ED2D `LGT_Helmet_Flashlight`: Off by Default | Disable Distance
    /// Attenuation shadow spotlight, 25 m.
    const HELMET_FLASHLIGHT: &str = "ffffffff0000c841e1eff70051000000c3f5a83e00004842cdcc4c3e000000000000000000000000000000000000044200000000e80300000100010000000000000040400000104100008040";
    /// 0289F7 `LGT_Helmet_Miner_FaceLight_Face_NS`: 0.001 m near clip.
    const HELMET_FACE_LIGHT: &str = "ffffffffec51383e9df5fd00210000000000803f0000b0426f12833a00000000000000000000000000000000000070418813000088130000010001ab00000000000060400000604000008040";
    /// 01D491 `defaultLight01`: an FO4-era record whose 256 radius and 7.2174
    /// near clip Starfield now reads as metres.
    const DEFAULT_LIGHT_01: &str = "ffffffff00008043ffffff00010000000000803f0000b442f1f4e6400000803f0000000000000000000000000000000000002041102700000000000000000000000080c00000604100008040";

    fn bytes(hex_payload: &str) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(hex::decode(hex_payload).unwrap()))
    }

    fn light(interner: &StringInterner, dat2: FieldValue) -> Record {
        let mut record = Record::new(
            SigCode(*b"LIGH"),
            FormKey {
                local: 0x0027B9,
                plugin: interner.intern("Starfield.esm"),
            },
        );
        for (sig, value) in [
            (*b"OBND", FieldValue::Bytes(SmallVec::from_vec(vec![0; 12]))),
            (*b"DAT2", dat2),
            (*b"FLBD", FieldValue::Bytes(SmallVec::from_vec(vec![0; 24]))),
            (*b"FVLD", FieldValue::Float(1.0)),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(sig),
                value,
            });
        }
        record
    }

    fn convert(hex_dat2: &str) -> Vec<u8> {
        let interner = StringInterner::new();
        let mut record = light(&interner, bytes(hex_dat2));
        normalize(&mut record, &interner);
        let data = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DATA")
            .expect("DATA");
        let FieldValue::Bytes(data) = &data.value else {
            panic!("DATA is raw bytes: {:?}", data.value);
        };
        assert_eq!(data.len(), 64, "FO4 form version 131 DATA");
        data.to_vec()
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn f32_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn assert_close(bytes: &[u8], offset: usize, expected: f32, what: &str) {
        let actual = f32_at(bytes, offset);
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what}: {actual}, expected {expected}"
        );
    }

    #[test]
    fn shadow_spotlight_lands_every_field_in_the_fo4_layout() {
        let interner = StringInterner::new();
        let mut record = light(&interner, bytes(SHADOW_SPOT_RED_1K));

        normalize(&mut record, &interner);

        let sigs: Vec<_> = record.fields.iter().map(|field| field.sig.0).collect();
        assert_eq!(sigs, [*b"OBND", *b"DATA", *b"FNAM", *b"FLBD", *b"FVLD"]);
        assert_eq!(
            record.fields[2].value,
            FieldValue::Float(1.0),
            "FNAM fade: no Starfield source, 582/801 vanilla lights use 1.0"
        );
        let data = convert(SHADOW_SPOT_RED_1K);
        assert_eq!(data[0..4], (-1_i32).to_le_bytes(), "time: infinite");
        assert_eq!(u32_at(&data, 4), 350, "5 m radius in units");
        assert_eq!(data[8..12], [182, 10, 10, 0], "RGB, unused fourth byte");
        assert_eq!(u32_at(&data, 12), 0x401, "Unknown 0 | Shadow Spotlight");
        assert_eq!(f32_at(&data, 16), 1.0, "falloff exponent");
        assert_eq!(f32_at(&data, 20), 90.0, "FOV degrees");
        assert_close(&data, 24, 6.9991, "0.1 m near clip");
        assert_eq!(f32_at(&data, 28), 0.0, "flicker period");
        assert_eq!(f32_at(&data, 32), 0.0, "flicker intensity amplitude");
        assert_eq!(f32_at(&data, 36), 0.0, "flicker movement amplitude");
        assert_eq!(f32_at(&data, 40), 0.0, "constant: vanilla 795/801");
        assert_eq!(f32_at(&data, 44), 1.0, "scalar: vanilla 653/801");
        assert_eq!(f32_at(&data, 48), 2.0, "exponent: vanilla 651/801");
        assert_eq!(
            f32_at(&data, 52),
            0.0,
            "god rays near clip: vanilla 589/801"
        );
        assert_eq!(
            u32_at(&data, 56),
            1000,
            "lumens handed to normalize_light_radii in the Value slot"
        );
        assert_eq!(f32_at(&data, 60), 0.0, "weight");
    }

    #[test]
    fn kelvin_temperature_is_already_baked_into_the_rgb() {
        let data = convert(SHADOW_SPOT_PBR_3000K);

        assert_eq!(data[8..12], [255, 177, 109, 0], "3000 K blackbody RGB");
        assert_eq!(u32_at(&data, 12), 0x401, "Use PBR Value has no FO4 bit");
        assert_eq!(u32_at(&data, 4), 560, "8 m radius in units");
        assert_eq!(f32_at(&data, 20), 120.0);
        assert_close(&data, 24, 10.4987, "0.15 m near clip");
        assert_eq!(u32_at(&data, 56), 8000);
    }

    #[test]
    fn disable_specular_omni_becomes_fo4_non_specular() {
        let data = convert(OMNI_NO_SPECULAR_8K);

        assert_eq!(
            u32_at(&data, 12),
            0x8001,
            "Unknown 0 | Non Specular, no type bit"
        );
        assert_eq!(u32_at(&data, 4), 140, "2 m radius in units");
        assert_eq!(f32_at(&data, 16), 2.0, "falloff exponent");
        assert_close(&data, 24, 17.4978, "0.25 m near clip");
        assert_eq!(data[8..12], [254, 233, 169, 0]);
        assert_eq!(u32_at(&data, 56), 8000);
    }

    #[test]
    fn flicker_and_pulse_effects_become_fo4_flag_bits() {
        let fire = convert(MEMORIAL_FIRE);
        assert_eq!(u32_at(&fire, 12), 0x8, "Flicker; source Unknown 0 is clear");
        assert_eq!(f32_at(&fire, 28), 0.5, "flicker period seconds");
        assert_eq!(f32_at(&fire, 32), 1.0, "flicker intensity amplitude");
        assert_close(&fire, 36, 8.3989, "0.12 m flicker movement in units");
        assert_eq!(u32_at(&fire, 4), 462, "6.6 m radius in units");
        assert_eq!(fire[8..12], [198, 64, 19, 0]);
        assert_eq!(u32_at(&fire, 56), 25000);

        let pulse = convert(PULSE_OMNI_50K);
        assert_eq!(u32_at(&pulse, 12), 0x81, "Unknown 0 | Pulse");
        assert_eq!(f32_at(&pulse, 20), 150.0);

        let spot = convert(FLICKER_NON_SHADOW_SPOT_4K);
        assert_eq!(
            u32_at(&spot, 12),
            0x4009,
            "Unknown 0 | Flicker | NonShadow Spotlight"
        );
    }

    #[test]
    fn unattenuated_flashlight_keeps_its_radius_under_the_shadow_ceiling() {
        let data = convert(HELMET_FLASHLIGHT);

        assert_eq!(
            u32_at(&data, 12),
            0x421,
            "Unknown 0 | Off By Default | Shadow Spotlight"
        );
        assert_eq!(
            u32_at(&data, 4),
            1024,
            "25 m is 1750 units, above FO4's shadow-caster ceiling"
        );
        assert_eq!(
            u32_at(&data, 56),
            0,
            "no inverse-square falloff, so no lumens-derived reach"
        );
        assert_close(&data, 16, 0.33, "falloff exponent");
        assert_eq!(f32_at(&data, 20), 50.0);
        assert_close(&data, 24, 13.9983, "0.2 m near clip");
    }

    #[test]
    fn near_clip_is_floored_at_the_vanilla_minimum() {
        let data = convert(HELMET_FACE_LIGHT);

        assert_eq!(f32_at(&data, 24), 1.0, "0.001 m would be 0.07 units");
        assert_eq!(u32_at(&data, 4), 13, "0.18 m radius in units");
        assert_eq!(u32_at(&data, 12), 0x8401);
        assert_eq!(f32_at(&data, 20), 88.0);
    }

    #[test]
    fn fo4_era_default_light_is_still_read_as_metres() {
        let data = convert(DEFAULT_LIGHT_01);

        assert_eq!(
            u32_at(&data, 4),
            2048,
            "256 m clamps to FO4's radius ceiling"
        );
        assert_close(&data, 24, 505.155, "7.2174 m near clip");
        assert_eq!(u32_at(&data, 12), 0x1);
        assert_eq!(
            u32_at(&data, 56),
            10000,
            "the Kelvin slot's float 10.0 is not read"
        );
    }

    #[test]
    fn other_records_and_unexpected_dat2_payloads_are_left_alone() {
        let interner = StringInterner::new();
        let mut reference = light(&interner, bytes(SHADOW_SPOT_RED_1K));
        reference.sig = SigCode(*b"REFR");
        let before = reference.fields.clone();
        normalize(&mut reference, &interner);
        assert_eq!(reference.fields, before);

        for dat2 in [
            FieldValue::Bytes(SmallVec::from_vec(vec![7; 64])),
            FieldValue::Float(1.0),
        ] {
            let mut record = light(&interner, dat2);
            let before = record.fields.clone();
            normalize(&mut record, &interner);
            assert_eq!(record.fields, before);
        }
    }
}
