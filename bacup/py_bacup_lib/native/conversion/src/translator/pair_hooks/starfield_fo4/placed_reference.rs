//! Starfield REFR light and map-marker data and CELL flags in FO4 layouts.

use super::STARFIELD_METERS_TO_FO4_UNITS;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

const STARFIELD_XLIG_LEN: usize = 32;
/// `ExtraLightDataStruct::fDefaultShadowDepthBias` in Fallout4.exe; the other
/// light-data defaults are zero.
const FO4_DEFAULT_SHADOW_DEPTH_BIAS: f32 = 1.0;
const FO4_DEFAULT_VOLUMETRIC_INTENSITY: f32 = 0.0;
/// Bits 0-3, 5-8 and 10-13 mean the same in both games. Bits 4, 9, 14 and 15 are
/// unnamed in both and never set by the FO4 masters; bits 16+ are Starfield-only.
const FO4_CELL_DATA_FLAGS: u32 = 0x3DEF;
/// Generic `POIMarker` icon, exported by FO4's HUDMenu and Pipboy_MapPage.
const FO4_POI_MARKER_TYPE: u8 = 77;

pub(crate) fn normalize(record: &mut Record, _interner: &StringInterner) {
    let is_reference = match &record.sig.0 {
        b"REFR" => true,
        b"CELL" => false,
        _ => return,
    };
    for field in &mut record.fields {
        let relaid = match (is_reference, &field.sig.0) {
            (true, b"XLIG") => relay_light_data(&field.value),
            (true, b"TNAM") => scalar(&field.value).map(|source_type| {
                FieldValue::Bytes(smallvec::smallvec![fo4_map_marker_type(source_type), 0])
            }),
            (false, b"DATA") => scalar(&field.value).map(|flags| {
                FieldValue::Bytes(
                    ((flags & FO4_CELL_DATA_FLAGS) as u16)
                        .to_le_bytes()
                        .into_iter()
                        .collect(),
                )
            }),
            _ => None,
        };
        if let Some(value) = relaid {
            field.value = value;
        }
    }
}

/// Starfield orders its overrides FOV, luminous scale, end distance cap, near
/// clip, inner FOV, shadow offset, then an 8-byte tail holding one unknown flag;
/// FO4 orders them FOV, fade, end distance cap, shadow depth bias, near clip,
/// volumetric intensity.
fn relay_light_data(value: &FieldValue) -> Option<FieldValue> {
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() != STARFIELD_XLIG_LEN {
        return None;
    }
    let float =
        |index: usize| f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
    // Starfield multiplies the base light's lumens by the scale, while
    // TESObjectREFR::GetFade adds the override to the base fade, which defaults
    // to 1.0. A zero scale is unset, not dark: Starfield.esm only ships it in
    // all-zero structs on lit ship-interior lights. Inner FOV has no FO4 field,
    // and Starfield.esm never sets a shadow offset, so FO4's depth bias keeps
    // its default.
    let luminous_scale = float(1);
    let fade = if luminous_scale == 0.0 {
        0.0
    } else {
        luminous_scale - 1.0
    };
    let fo4 = [
        float(0),
        fade,
        float(2) * STARFIELD_METERS_TO_FO4_UNITS,
        FO4_DEFAULT_SHADOW_DEPTH_BIAS,
        float(3) * STARFIELD_METERS_TO_FO4_UNITS,
        FO4_DEFAULT_VOLUMETRIC_INTENSITY,
    ];
    Some(FieldValue::Bytes(
        fo4.iter().flat_map(|value| value.to_le_bytes()).collect(),
    ))
}

fn scalar(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => Some(*value as u32),
        FieldValue::Bytes(bytes) if !bytes.is_empty() => Some(
            bytes
                .iter()
                .take(4)
                .enumerate()
                .fold(0, |bits, (index, byte)| {
                    bits | (u32::from(*byte) << (index * 8))
                }),
        ),
        _ => None,
    }
}

/// The FO4 icons are indexed by type, so a Starfield index past FO4's table, or
/// one landing on an unrelated FO4 landmark, breaks the compass.
fn fo4_map_marker_type(source_type: u32) -> u8 {
    match source_type {
        // Akila City, Cydonia, Dazra, Neon, New Atlantis, The Well.
        0..=4 | 61 => 1,
        // Ryujin Industries tower, like FO4's Mass Fusion and Trinity Tower.
        5 => 54,
        // Scaled Citadel.
        6 => 53,
        // The Lodge, like FO4's Cabot House; Bar, Food, Vendors and Realtor,
        // like FO4's Shamrock Taphouse and Fallon's Department Store.
        7 | 51..=55 | 58 => 9,
        // The MAST, like FO4's Massachusetts State House; NASA; Starborn Obelisk.
        8 | 23 | 50 => 5,
        // Cave, Crystal Grotto, Animal Den.
        10 | 18 | 27 => 0,
        // Mining Base.
        11 => 40,
        // Outpost.
        12 => 3,
        // Science Lab, like FO4's Cambridge Polymer Labs.
        13 => 33,
        // Natural Landmark, Fractured Earth, Mountain Peak, Coral Colony, Fossil
        // Outcropping, Geysers, Icy and Rocky Asteroids.
        14 | 16 | 17 | 25 | 26 | 29 | 40 | 41 => 8,
        // Mech Graveyard, Space Graveyard, Debris Field.
        15 | 39 | 46 => 36,
        // Crashed Starship, like FO4's Skylanes Flight 1981; Legendary Ship, The
        // Vigilance, Derelict Ship, Ship.
        19 | 36..=38 | 47 => 16,
        // Farm, Industrial, Military Base.
        20 => 26,
        21 => 4,
        22 => 7,
        // Starborn Temple and the six shrines.
        24 | 64..=69 => 20,
        // Frozen Watersource, Boiling Hotsprings.
        28 | 30 => 39,
        // Acid Fog, Radiation Zone.
        31 | 32 => 41,
        // Star Station, Star Yard, The Eye, The Key.
        33 | 34 | 43 | 44 => 62,
        // Settlement, Surface Settlement; Player House, like FO4's Diamond City home.
        35 | 48 | 59 => 13,
        // Distress Call.
        49 => 42,
        // Aid Vendor, Surgery.
        56 | 57 => 31,
        // Transit, like FO4's Nuka-World Transit Center.
        60 => 73,
        // Trackers Alliance.
        62 => 58,
        // POI, The Rock, The Unity and the unnamed type 63.
        _ => FO4_POI_MARKER_TYPE,
    }
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::schema::AuthoringSchema;
    use crate::target_write::encode_field_pub;

    fn record(
        interner: &StringInterner,
        sig: &[u8; 4],
        local: u32,
        fields: &[(&[u8; 4], FieldValue)],
    ) -> Record {
        let form_key = FormKey {
            local,
            plugin: interner.intern("Starfield.esm"),
        };
        let mut record = Record::new(SigCode(*sig), form_key);
        record
            .fields
            .extend(fields.iter().map(|(sig, value)| FieldEntry {
                sig: SubrecordSig(**sig),
                value: value.clone(),
            }));
        record
    }

    fn bytes(hex: &str) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(hex::decode(hex).unwrap()))
    }

    /// The payload the FO4 writer emits for `sig` after normalization.
    fn fo4_payload(interner: &StringInterner, source: Record, sig: &[u8; 4]) -> String {
        let mut record = source;
        normalize(&mut record, interner);
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let field = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *sig)
            .unwrap();
        hex::encode(
            encode_field_pub(field, schema.record_def(record.sig.as_str()), interner).unwrap(),
        )
    }

    // Shipped `Starfield.esm` REFR XLIG payloads. FO4 fade is added to the base
    // light's fade, so a 0.5 lumen scale becomes -0.5; distances are metres.
    #[test]
    fn xlig_relays_each_starfield_light_field_to_its_fo4_meaning() {
        let interner = StringInterner::new();
        for (local, source, expected) in [
            // luminous scale 0.5 only.
            (
                0x23_BBF3,
                "000000000000003f00000000000000000000000000000000abababab00ababab",
                "00000000000000bf000000000000803f0000000000000000",
            ),
            // FOV -6.94, scale 1.236, end cap 11.25 m, near clip 0.45 m.
            (
                0x20_24F1,
                "e007dec076409e3f4af733416a66e63e0000000000000000abababab00ababab",
                "e007dec0b003723e2cd044440000803ff3f7fb4100000000",
            ),
            // Inner FOV -60 must not become the FO4 near clip.
            (
                0x09_0556,
                "000000000000803e24c31041ec51783e000070c200000000abababab00ababab",
                "00000000000040bf5e501e440000803f74c8874100000000",
            ),
            // All-zero override on a lit Frontier interior light: scale 0 is unset.
            (
                0x01_55F9,
                "000000000000000000000000000000000000000000000000abababab00ababab",
                "0000000000000000000000000000803f0000000000000000",
            ),
        ] {
            let refr = record(&interner, b"REFR", local, &[(b"XLIG", bytes(source))]);

            assert_eq!(
                fo4_payload(&interner, refr, b"XLIG"),
                expected,
                "{local:06X}"
            );
        }
    }

    // Shipped map markers: 192566 Industrial, 1DC399 POI, 27CBB6 Neon, 259FC0
    // Military Base, plus a type past Starfield's table. Starfield's type is
    // uint16; FO4 reads a type byte and a pad.
    #[test]
    fn map_marker_type_maps_by_meaning_into_the_fo4_two_byte_struct() {
        let interner = StringInterner::new();
        for (local, source_type, expected) in [
            (0x19_2566, 21, "0400"),
            (0x1D_C399, 45, "4d00"),
            (0x27_CBB6, 3, "0100"),
            (0x25_9FC0, 22, "0700"),
            (0x00_0800, 70, "4d00"),
        ] {
            let refr = record(
                &interner,
                b"REFR",
                local,
                &[
                    (b"XMRK", FieldValue::None),
                    (b"TNAM", FieldValue::Uint(source_type)),
                ],
            );

            assert_eq!(
                fo4_payload(&interner, refr, b"TNAM"),
                expected,
                "{local:06X}"
            );
        }
    }

    #[test]
    fn map_marker_type_already_narrowed_to_two_bytes_is_still_mapped() {
        let interner = StringInterner::new();
        let refr = record(&interner, b"REFR", 0x19_2566, &[(b"TNAM", bytes("1500"))]);

        assert_eq!(fo4_payload(&interner, refr, b"TNAM"), "0400");
    }

    // Shipped cells: 3ADB30 (interior + unnamed bit 15), 236220 (Use Location
    // as Name + Use Planet Gravity + IS volume criteria), 255DAA (sky lighting
    // with sunlight shadows), 0BFA18 (Use Location as Name).
    #[test]
    fn cell_data_keeps_only_flags_fo4_defines_with_the_same_meaning() {
        let interner = StringInterner::new();
        for (local, source, expected) in [
            (0x3A_DB30, 0x0000_8001, "0100"),
            (0x23_6220, 0x000B_0005, "0500"),
            (0x25_5DAA, 0x0000_0901, "0109"),
            (0x0B_FA18, 0x0001_0005, "0500"),
        ] {
            let cell = record(
                &interner,
                b"CELL",
                local,
                &[(b"DATA", FieldValue::Uint(source))],
            );

            assert_eq!(
                fo4_payload(&interner, cell, b"DATA"),
                expected,
                "{local:06X}"
            );
        }
    }

    #[test]
    fn same_signatures_on_other_records_are_untouched() {
        let interner = StringInterner::new();
        let mut keyword = record(
            &interner,
            b"KYWD",
            0x80_0000,
            &[(b"TNAM", FieldValue::Uint(52))],
        );
        let mut worldspace = record(
            &interner,
            b"WRLD",
            0x80_0001,
            &[(b"DATA", FieldValue::Uint(0x8001))],
        );

        normalize(&mut keyword, &interner);
        normalize(&mut worldspace, &interner);

        assert_eq!(keyword.fields[0].value, FieldValue::Uint(52));
        assert_eq!(worldspace.fields[0].value, FieldValue::Uint(0x8001));
    }
}
