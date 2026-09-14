//! Starfield `REVB`, `KSSM` and `IMGS` carry WWise references and reflection
//! blobs where FO4 stores reverb parameters, a sound descriptor and image-space
//! values, so each takes its FO4 subrecords from a vanilla Fallout4.esm record.

use smallvec::SmallVec;

use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fnv_mgef::null_reference;

/// Fallout4.esm IMGS 0016C4 `DefaultImageSpaceF4`, the LUT-free default that
/// vanilla interiors use.
const FO4_DEFAULT_HDR: [f32; 9] = [10.0, 0.02, 0.33, 0.1, 10.0, 0.5, 1.0, 1.0, 0.18];
const FO4_DEFAULT_CINEMATIC: [f32; 3] = [1.0, 1.0, 1.0];
const FO4_DEFAULT_TINT: [f32; 4] = [0.0, 1.0, 1.0, 1.0];
/// Strength, distance and range; the 62.0 is the record's two unused bytes and
/// sky/blur radius word (0x4278) as they read in the original 16-byte float
/// form. The zero vignette radius and strength complete FO4's 24-byte layout.
const FO4_DEFAULT_DEPTH_OF_FIELD: [f32; 6] = [1.0, 500.0, 3500.0, 62.0, 0.0, 0.0];

pub(crate) fn normalize(record: &mut Record, interner: &StringInterner) {
    match &record.sig.0 {
        b"REVB" => {
            let editor_id = record.eid.and_then(|eid| interner.resolve(eid));
            let data = fo4_reverb_data(editor_id.unwrap_or_default());
            push(record, b"DATA", FieldValue::Bytes(SmallVec::from_slice(&data)));
        }
        // Starfield maps keywords to a WWise event, which has no FO4 SNDR.
        b"KSSM" => push(record, b"DNAM", null_reference()),
        b"IMGS" => {
            push(record, b"HNAM", floats(&FO4_DEFAULT_HDR));
            push(record, b"CNAM", floats(&FO4_DEFAULT_CINEMATIC));
            push(record, b"TNAM", floats(&FO4_DEFAULT_TINT));
            push(record, b"DNAM", floats(&FO4_DEFAULT_DEPTH_OF_FIELD));
        }
        _ => {}
    }
}

/// Starfield kept these Fallout4.esm REVB object ids, renamed for the same
/// room, so each takes the parameters of the FO4 record with its id (cited
/// above each arm). Starfield-only reverbs have no FO4 parameters to carry.
fn fo4_reverb_data(editor_id: &str) -> [u8; 14] {
    match editor_id {
        // 0C5B6C IntRoomStoneMediumReverb
        "Reverb_C_IntRoomStoneMedium" => reverb(933, 5000, -6, -4, -5, 2, 100, 29, 35, 50, 75),
        // 0E322E IntRoomStoneNarrow
        // 22D145 IntIndustrialRoomsReverb
        // 2313D8 IntHighTechRoomsReverb
        // 239B34 IntIndustrialMediumReverb
        "Reverb_B_IntRoomStoneNarrow"
        | "Reverb_B_IntIndustrialMedium"
        | "Reverb_B_IntHighTechMedium"
        | "Reverb_C_IntIndustrialMedium" => reverb(285, 1600, -4, -3, 0, 3, 100, 20, 26, 13, 60),
        // 0E3246 IntRoomStoneLargeReverb
        // 1E4F32 IntSubwayTunnelsReverb
        // 2313D7 IntHighTechReverbMedium
        // 238BD6 IntIndustrialLargeReverb
        // 23AB4D IntHighTechReverbLarge
        "Reverb_D_IntRoomStoneLarge"
        | "Reverb_B_IntStoneCaveTunnels"
        | "Reverb_C_IntHighTechMedium"
        | "Reverb_D_IntIndustrialLarge"
        | "Reverb_D_IntHighTechLarge" => reverb(2877, 2994, -3, -4, -7, 0, 100, 35, 47, 83, 83),
        // 1FFC21 IntGenericAReverb
        // 1FFC22 IntGenericBReverb
        "Reverb_B_IntGenericMedium" | "Reverb_C_IntGenericMedium" => {
            reverb(1396, 5000, -4, -10, -5, 3, 100, 44, 0, 10, 100)
        }
        // 238A9B IntCaveLargeExtraReverb
        "Reverb_E_IntStoneCaveLarge" => reverb(3710, 2994, -1, -2, -4, 0, 100, 45, 63, 83, 83),
        // 238CD4 IntGenericCReverb
        "Reverb_D_IntGenericLarge" => reverb(2044, 5000, -3, -10, -5, 3, 100, 44, 0, 10, 100),
        // 239E57 IntFakeBrickSmall
        // 239E62 IntFakeMetalSmall
        // 239E65 IntFakeWoodSmall
        "Reverb_B_IntFakeBrickMedium"
        | "Reverb_B_IntFakeMetalMedium"
        | "Reverb_B_IntFakeWoodMedium" => reverb(100, 1972, -1, -1, 4, 7, 100, 0, 10, 25, 47),
        // 239E58 IntFakeBrickMedium
        // 239E61 IntFakeMetalMedium
        // 239E64 IntFakeWoodMedium
        "Reverb_C_IntFakeBrickMedium"
        | "Reverb_C_IntFakeMetalMedium"
        | "Reverb_C_IntFakeWoodMedium" => reverb(933, 5000, -3, -1, 1, 0, 64, 14, 16, 100, 100),
        // 239E60 IntFakeMetalLarge
        "Reverb_D_IntFakeMetalLarge" => reverb(1766, 5000, -6, -6, -100, 5, 83, 40, 61, 29, 92),
        // 0C5B6E DefaultReverb, which Starfield also kept: every level at -100.
        _ => reverb(100, 20, -100, -100, -100, -100, 10, 0, 0, 100, 100),
    }
}

#[allow(clippy::too_many_arguments)]
fn reverb(
    decay_time_ms: u16,
    hf_reference_hz: u16,
    room_filter: i8,
    room_hf_filter: i8,
    reflections: i8,
    reverb_amp: i8,
    decay_hf_ratio: u8,
    reflect_delay_ms_scaled: u8,
    reverb_delay_ms: u8,
    diffusion: u8,
    density: u8,
) -> [u8; 14] {
    let [decay_low, decay_high] = decay_time_ms.to_le_bytes();
    let [hf_low, hf_high] = hf_reference_hz.to_le_bytes();
    [
        decay_low,
        decay_high,
        hf_low,
        hf_high,
        room_filter as u8,
        room_hf_filter as u8,
        reflections as u8,
        reverb_amp as u8,
        decay_hf_ratio,
        reflect_delay_ms_scaled,
        reverb_delay_ms,
        diffusion,
        density,
        0,
    ]
}

fn floats(values: &[f32]) -> FieldValue {
    FieldValue::Bytes(values.iter().flat_map(|value| value.to_le_bytes()).collect())
}

fn push(record: &mut Record, sig: &[u8; 4], value: FieldValue) {
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*sig),
        value,
    });
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_add_master_native,
        plugin_handle_close_native, plugin_handle_new_no_py, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    use crate::ids::FormKey;
    use crate::schema::AuthoringSchema;
    use crate::source_read::decode_record_from_parsed;
    use crate::sym::StringInterner;
    use crate::translator::pair_hook::PairCtx;
    use crate::translator::{Game, TranslateResult, Translator};

    const PLUGIN: &str = "Starfield.esm";

    fn find_parsed<'a>(items: &'a [ParsedItem], sig: &str, local: u32) -> Option<&'a ParsedRecord> {
        items.iter().find_map(|item| match item {
            ParsedItem::Record(record)
                if record.signature.as_str() == sig && record.form_id & 0x00FF_FFFF == local =>
            {
                Some(record)
            }
            ParsedItem::Group(group) => find_parsed(&group.children, sig, local),
            _ => None,
        })
    }

    /// Decodes shipped `Starfield.esm` bytes as the source reader does, runs the
    /// pair translation and returns every `(signature, hex payload)` the FO4
    /// writer emits for the record.
    fn written_fo4_subrecords(
        sig: &str,
        form_id: u32,
        subrecords: &[(&str, &str)],
    ) -> Vec<(String, String)> {
        let interner = StringInterner::new();
        let raw = ParsedRecord {
            signature: SmolStr::new(sig),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(578),
            version2: None,
            subrecords: subrecords
                .iter()
                .map(|(sig, payload)| ParsedSubrecord {
                    signature: SmolStr::new(sig),
                    data: Bytes::from(hex::decode(payload).unwrap()),
                    semantic_type: None,
                })
                .collect(),
            raw_payload: None,
            parse_error: None,
        };
        let form_key = FormKey {
            local: form_id,
            plugin: interner.intern(PLUGIN),
        };
        let source_schema = AuthoringSchema::for_game("starfield").unwrap();
        let mut record = decode_record_from_parsed(
            &raw,
            &form_key,
            &source_schema,
            &[],
            PLUGIN,
            None,
            false,
            &interner,
        )
        .unwrap();

        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        translator
            .pre_translate(&mut PairCtx::new(&interner), &mut record)
            .unwrap();
        let TranslateResult::Translated(record) = translator.translate(&record, &interner) else {
            panic!("{sig} should translate");
        };

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let handle = plugin_handle_new_no_py(PLUGIN, Some("fo4"));
        plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
        crate::target_write::add_record_native(handle, record, &schema, &interner).unwrap();
        let written = {
            let store = plugin_handle_store_ref().lock().unwrap();
            find_parsed(&store.get(&handle).unwrap().parsed.root_items, sig, form_id)
                .unwrap_or_else(|| panic!("written {sig}"))
                .subrecords
                .iter()
                .map(|subrecord| {
                    (
                        subrecord.signature.to_string(),
                        hex::encode(&subrecord.data),
                    )
                })
                .collect()
        };
        assert!(plugin_handle_close_native(handle));
        written
    }

    fn payload<'a>(written: &'a [(String, String)], sig: &str) -> Option<&'a str> {
        written
            .iter()
            .find(|(written_sig, _)| written_sig == sig)
            .map(|(_, payload)| payload.as_str())
    }

    // Starfield REVB 0E322E kept Fallout4.esm REVB 0E322E `IntRoomStoneNarrow`
    // and only renamed it, so the FO4 reverb parameters are that record's.
    #[test]
    fn stone_narrow_reverb_takes_the_fo4_reverb_that_shares_its_form() {
        let written = written_fo4_subrecords(
            "REVB",
            0x0E_322E,
            &[
                ("EDID", "5265766572625f425f496e74526f6f6d53746f6e654e6172726f7700"),
                ("RABG", "3f4bb704ffa17a513938699fd98f9180"),
                ("ANAM", "02000000"),
            ],
        );

        assert_eq!(payload(&written, "DATA"), Some("1d014006fcfd000364141a0d3c00"));
        assert_eq!(payload(&written, "ANAM"), Some("02000000"), "Class B, four bytes");
        assert_eq!(payload(&written, "RABG"), None, "WWise aux bus has no FO4 field");
    }

    #[test]
    fn starfield_only_reverb_falls_back_to_the_fo4_default_reverb() {
        let written = written_fo4_subrecords(
            "REVB",
            0x1E_C6D6,
            &[
                ("EDID", "5265766572625f445f496e744e656f6e436c75624c6172676500"),
                ("RABG", "124b36e4785565fb7480705e56e838be"),
                ("ANAM", "04000000"),
            ],
        );

        // Fallout4.esm REVB 0C5B6E `DefaultReverb`.
        assert_eq!(payload(&written, "DATA"), Some("640014009c9c9c9c0a0000646400"));
        assert_eq!(payload(&written, "ANAM"), Some("04000000"));
    }

    // A WWise start event has no FO4 sound descriptor to point at.
    #[test]
    fn weapon_sound_mapping_gets_a_null_fo4_primary_descriptor() {
        let written = written_fo4_subrecords(
            "KSSM",
            0x00_5C96,
            &[
                (
                    "EDID",
                    "534b4d536869705f50726f6a656374696c6543616e6e6f6e5f42616c6c6973746963536f6c7574696f6e735f415f4e504300",
                ),
                (
                    "WED0",
                    "23464035f23965dd08e2b9b8a07be8b3000000000000000000000000000000000000000000000000",
                ),
                ("KNAM", "945c0000"),
                ("RSMC", "00000000"),
            ],
        );

        assert_eq!(payload(&written, "DNAM"), Some("00000000"));
        assert_eq!(payload(&written, "KNAM").map(str::len), Some(8));
        assert_eq!(payload(&written, "WED0"), None);
    }

    // Converted interior cells point XCIM at these records. The 4728-byte RDIF
    // reflection blob is left out: nothing reads it.
    #[test]
    fn image_space_takes_the_fo4_default_image_space() {
        let written = written_fo4_subrecords(
            "IMGS",
            0x0D_B200,
            &[
                (
                    "EDID",
                    "49534578746572696f72556e69717565416b696c615261696e446179303100",
                ),
                ("RFDP", "a6830e00"),
            ],
        );

        // Fallout4.esm IMGS 0016C4 `DefaultImageSpaceF4`; FO4's 24-byte DNAM
        // adds a zero vignette to its 16-byte form.
        assert_eq!(
            payload(&written, "HNAM"),
            Some("000020410ad7a33cc3f5a83ecdcccc3d000020410000003f0000803f0000803fec51383e")
        );
        assert_eq!(payload(&written, "CNAM"), Some("0000803f0000803f0000803f"));
        assert_eq!(payload(&written, "TNAM"), Some("000000000000803f0000803f0000803f"));
        assert_eq!(
            payload(&written, "DNAM"),
            Some("0000803f0000fa4300c05a45000078420000000000000000")
        );
        assert_eq!(payload(&written, "RFDP"), None);
    }
}
