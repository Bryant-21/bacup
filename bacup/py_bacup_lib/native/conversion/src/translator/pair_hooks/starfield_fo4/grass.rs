//! Starfield `GRAS.DNAM` (38 bytes) to the FO4 `GRAS.DATA` layout (32 bytes).

use smallvec::SmallVec;

use super::STARFIELD_METERS_TO_FO4_UNITS;
use crate::ids::SubrecordSig;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

const STARFIELD_DNAM_LEN: usize = 38;

/// Vertex Lighting, Uniform Scaling and Fit to Slope share bits with Starfield,
/// which adds Apply Between.
const FO4_FLAG_MASK: u8 = 0x07;
const APPLY_BETWEEN: u8 = 0x08;

/// Starfield has no placement footprint. Fallout4.esm GRAS split 10/10 between
/// 16 and 32 units; 32 is the Creation Kit default.
const FO4_POSITION_RANGE: f32 = 32.0;

const ABOVE_AT_LEAST: u32 = 0;
const BELOW_AT_LEAST: u32 = 2;
const EITHER_AT_LEAST: u32 = 4;
const EITHER_AT_MOST: u32 = 5;
const EITHER_AT_MOST_ABOVE: u32 = 6;
const EITHER_AT_MOST_BELOW: u32 = 7;

pub(crate) fn normalize(record: &mut Record, _interner: &StringInterner) {
    if record.sig.0 != *b"GRAS" {
        return;
    }
    for field in &mut record.fields {
        if field.sig.0 != *b"DNAM" {
            continue;
        }
        let data = match &field.value {
            FieldValue::Bytes(dnam) if dnam.len() == STARFIELD_DNAM_LEN => fo4_data(dnam),
            _ => continue,
        };
        field.sig = SubrecordSig(*b"DATA");
        field.value = FieldValue::Bytes(data);
    }
}

fn fo4_data(dnam: &[u8]) -> SmallVec<[u8; 32]> {
    let float = |offset: usize| f32::from_le_bytes(dnam[offset..offset + 4].try_into().unwrap());
    let (max_density, min_slope, max_slope, flags) = (dnam[28], dnam[29], dnam[30], dnam[31]);
    let (units_from_water, units_from_water_type) =
        units_from_water(float(20), float(24), flags & APPLY_BETWEEN != 0);

    let mut data = SmallVec::new();
    data.extend_from_slice(&[max_density, min_slope, max_slope, 0]);
    data.extend_from_slice(&units_from_water.to_le_bytes());
    data.extend_from_slice(&[0, 0]);
    data.extend_from_slice(&units_from_water_type.to_le_bytes());
    // The FO4 wiki defines Wave Period as waves per minute, the quantity
    // Starfield renamed Wind Frequency; both default to 10.
    for value in [FO4_POSITION_RANGE, float(8), float(12), float(16)] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[flags & FO4_FLAG_MASK, 0, 0, 0]);
    data
}

/// Starfield grows grass at least `above` metres over the water line or at
/// least `below` metres under it, FLT_MAX disabling a clamp; Apply Between
/// keeps the band between the two clamps instead. FO4 has one distance and a
/// direction, so a two-sided band keeps the enclosing FO4 rule.
fn units_from_water(above: f32, below: f32, apply_between: bool) -> (u16, u32) {
    let clamp = |meters: f32| (meters < f32::MAX).then(|| fo4_units(meters));
    match (clamp(above), clamp(below), apply_between) {
        (None, None, _) => (u16::MAX, EITHER_AT_MOST_BELOW),
        (Some(above), None, false) => (above, ABOVE_AT_LEAST),
        (Some(above), None, true) => (above, EITHER_AT_MOST_ABOVE),
        (None, Some(below), false) => (below, BELOW_AT_LEAST),
        (None, Some(below), true) => (below, EITHER_AT_MOST_BELOW),
        (Some(above), Some(below), false) => (above.min(below), EITHER_AT_LEAST),
        (Some(above), Some(below), true) => (above.max(below), EITHER_AT_MOST),
    }
}

fn fo4_units(meters: f32) -> u16 {
    (meters * STARFIELD_METERS_TO_FO4_UNITS)
        .round()
        .clamp(0.0, f32::from(u16::MAX)) as u16
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

    /// Decodes shipped `Starfield.esm` GRAS bytes as the source reader does, runs
    /// the pair translation and returns the `DATA` payload the FO4 writer emits.
    fn written_fo4_data(form_id: u32, editor_id: &str, dnam: &str) -> Option<Vec<u8>> {
        let interner = StringInterner::new();
        let raw = ParsedRecord {
            signature: SmolStr::new("GRAS"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(578),
            version2: None,
            subrecords: [("EDID", format!("{}00", hex::encode(editor_id))), ("DNAM", dnam.into())]
                .into_iter()
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
            panic!("GRAS should translate");
        };

        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let handle = plugin_handle_new_no_py(PLUGIN, Some("fo4"));
        plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
        crate::target_write::add_record_native(handle, record, &schema, &interner).unwrap();
        let data = {
            let store = plugin_handle_store_ref().lock().unwrap();
            find_parsed(&store.get(&handle).unwrap().parsed.root_items, "GRAS", form_id)
                .expect("written GRAS")
                .subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "DATA")
                .map(|subrecord| subrecord.data.to_vec())
        };
        assert!(plugin_handle_close_native(handle));
        data
    }

    struct Fo4GrassData {
        density: u8,
        min_slope: u8,
        max_slope: u8,
        units_from_water: u16,
        units_from_water_type: u32,
        position_range: f32,
        height_range: f32,
        color_range: f32,
        wave_period: f32,
        flags: u8,
    }

    fn parse(data: &[u8]) -> Fo4GrassData {
        assert_eq!(data.len(), 32, "FO4 GRAS DATA is 32 bytes");
        assert_eq!([data[3], data[6], data[7], data[29], data[30], data[31]], [0; 6]);
        let float = |offset: usize| f32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        Fo4GrassData {
            density: data[0],
            min_slope: data[1],
            max_slope: data[2],
            units_from_water: u16::from_le_bytes([data[4], data[5]]),
            units_from_water_type: u32::from_le_bytes(data[8..12].try_into().unwrap()),
            position_range: float(12),
            height_range: float(16),
            color_range: float(20),
            wave_period: float(24),
            flags: data[28],
        }
    }

    // GrassFieldDried04A: contrast 15, cluster scale 0.2, height range 0.35,
    // color range 0.1, wind frequency 10, above-water clamp 0 m, no below-water
    // clamp, max density 94, slope 0-50, Fits Slope, coverage 90, dirtiness 127.
    #[test]
    fn dried_field_grass_takes_the_fo4_data_layout() {
        let data = written_fo4_data(
            0x00_1CF8,
            "GrassFieldDried04A",
            "00007041cdcc4c3e3333b33ecdcccc3d0000204100000000ffff7f7f5e0032040000b4427f7f",
        )
        .expect("FO4 GRAS carries DATA");

        let data = parse(&data);
        assert_eq!((data.density, data.min_slope, data.max_slope), (94, 0, 50));
        // Above - At Least 0: land only, FO4's own land-grass setting.
        assert_eq!((data.units_from_water, data.units_from_water_type), (0, 0));
        assert_eq!(data.position_range, 32.0);
        assert_eq!(data.height_range, 0.35);
        assert_eq!(data.color_range, 0.1);
        assert_eq!(data.wave_period, 10.0);
        assert_eq!(data.flags, 0x04, "Fit to Slope");
    }

    // Starfield water clamps are metres with FLT_MAX for "off"; Apply Between
    // (0x08) keeps the band between the clamps instead of outside it.
    #[test]
    fn water_clamps_become_fo4_units_from_water_by_meaning() {
        for (form_id, editor_id, dnam, expected_units, expected_type, expected_flags) in [
            // Above 0.5 m, no below clamp: Above - At Least 35 units.
            (
                0x15_BDB7,
                "GrassFernForest01",
                "000096420000803f0000803ecdcc4c3e000020410000003fffff7f7f37002c0600000c427f7f",
                35,
                0,
                0x06,
            ),
            // Below 2 m only: Below - At Least 140 units.
            (
                0x1E_0903,
                "GrassSeaweed03A",
                "00008c423333b33fcdcc4c3e0000003f00002041ffff7f7f000000403700370600000c427f7f",
                140,
                2,
                0x06,
            ),
            // Below 0.7 m with Apply Between: Either - At Most Below 49 units.
            (
                0x1E_60AA,
                "GrassBullrush01A",
                "0000ac429a99993e0000803e0000000000002041ffff7f7f3333333f53002d0a000018427f7f",
                49,
                7,
                0x02,
            ),
            // Neither clamp: unrestricted, as Either - At Most Below the u16 limit.
            (
                0x0F_569E,
                "GrassIceChunks01",
                "000094420000803f0000803e000000000000803fffff7f7fffff7f7f16002d0e000080417f7f",
                u16::MAX,
                7,
                0x06,
            ),
            // Above 0 m or below 0.1 m: Either - At Least the nearer clamp.
            (
                0x1E_66A3,
                "GrassWetlands02A",
                "0000a8426666663fcdcc4c3e000000000000803f00000000cdcccc3d28003704000070427f7f",
                0,
                4,
                0x04,
            ),
        ] {
            let data = parse(&written_fo4_data(form_id, editor_id, dnam).expect(editor_id));
            assert_eq!(
                (data.units_from_water, data.units_from_water_type, data.flags),
                (expected_units, expected_type, expected_flags),
                "{editor_id}"
            );
        }
    }
}
