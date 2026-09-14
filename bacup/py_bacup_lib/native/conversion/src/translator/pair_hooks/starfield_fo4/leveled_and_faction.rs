//! Starfield leveled-list, vendor, moveable-static and keyword data in FO4 layouts.

use super::STARFIELD_METERS_TO_FO4_UNITS;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

/// Calculate from all levels, calculate for each item in count, use all.
const FO4_LEVELED_LIST_FLAGS: u64 = 0x07;
const VENV_LEN: usize = 12;
const MSTT_HIDDEN_FROM_LOCAL_MAP: u32 = 0x0000_0200;

pub(crate) fn normalize(record: &mut Record, _interner: &StringInterner) {
    match &record.sig.0 {
        b"LVLI" | b"LVLN" => normalize_leveled_list(record),
        b"FACT" => normalize_vendor_values(record),
        b"MSTT" => normalize_moveable_static_data(record),
        // FO4 reads this as an AORU too, but Starfield's rules are creature
        // environment behaviours outside the record fence, so the FormID would
        // resolve to whatever unrelated form owns that ID in the target.
        b"KYWD" => record.fields.retain(|field| field.sig.0 != *b"DATA"),
        _ => {}
    }
}

fn normalize_leveled_list(record: &mut Record) {
    for field in &mut record.fields {
        match (&field.sig.0, &field.value) {
            (b"LVLD", FieldValue::Float(chance_none)) => {
                field.value = FieldValue::Uint(chance_none.round().clamp(0.0, 100.0) as u64);
            }
            (b"LVLF", FieldValue::Uint(flags)) => {
                field.value = FieldValue::Uint(flags & FO4_LEVELED_LIST_FLAGS);
            }
            _ => {}
        }
    }
}

/// Starfield's radius is a metre float where FO4 has a u16 radius and two
/// unused bytes; the three vendor booleans and the trailing byte keep offsets.
fn normalize_vendor_values(record: &mut Record) {
    for field in &mut record.fields {
        if field.sig.0 != *b"VENV" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut field.value else {
            continue;
        };
        if bytes.len() != VENV_LEN {
            continue;
        }
        let meters = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let units = (meters * STARFIELD_METERS_TO_FO4_UNITS)
            .round()
            .clamp(0.0, u16::MAX as f32) as u16;
        bytes[4..6].copy_from_slice(&units.to_le_bytes());
        bytes[6..8].fill(0);
    }
}

/// Starfield `DATA` is mass-override flags; FO4's is the On Local Map bool,
/// which vanilla never sets alongside the Hidden From Local Map header flag.
fn normalize_moveable_static_data(record: &mut Record) {
    let on_local_map = u64::from(record.flags.bits() & MSTT_HIDDEN_FROM_LOCAL_MAP == 0);
    for field in &mut record.fields {
        if field.sig.0 == *b"DATA" {
            field.value = FieldValue::Uint(on_local_map);
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_add_master_native,
        plugin_handle_close_native, plugin_handle_new_no_py, plugin_handle_store_ref,
    };
    use smol_str::SmolStr;

    use super::*;
    use crate::ids::FormKey;
    use crate::schema::AuthoringSchema;
    use crate::source_read::decode_record_from_parsed;

    const PLUGIN: &str = "Starfield.esm";

    /// Decodes shipped `Starfield.esm` subrecord bytes the way the source reader does.
    fn starfield_record(
        interner: &StringInterner,
        sig: &str,
        form_id: u32,
        flags: u32,
        subrecords: &[(&str, &str)],
    ) -> Record {
        let raw = ParsedRecord {
            signature: SmolStr::new(sig),
            form_id,
            flags,
            version_control: 0,
            form_version: Some(578),
            version2: None,
            subrecords: subrecords
                .iter()
                .map(|(sig, hex)| ParsedSubrecord {
                    signature: SmolStr::new(sig),
                    data: Bytes::from(hex::decode(hex).unwrap()),
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
        let schema = AuthoringSchema::for_game("starfield").unwrap();
        decode_record_from_parsed(&raw, &form_key, &schema, &[], PLUGIN, None, false, interner)
            .unwrap()
    }

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

    /// The subrecord payload the FO4 target writer actually emits.
    fn written_fo4_subrecord(
        record: &Record,
        sig: &str,
        interner: &StringInterner,
    ) -> Option<Vec<u8>> {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let handle = plugin_handle_new_no_py(PLUGIN, Some("fo4"));
        plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
        crate::target_write::add_record_native(handle, record.clone(), &schema, interner)
            .unwrap_or_else(|error| panic!("write {} failed: {error}", record.sig.as_str()));
        let data = {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&handle).unwrap();
            find_parsed(
                &slot.parsed.root_items,
                record.sig.as_str(),
                record.form_key.local,
            )
            .expect("written record")
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == sig)
            .map(|subrecord| subrecord.data.as_ref().to_vec())
        };
        assert!(plugin_handle_close_native(handle));
        data
    }

    #[test]
    fn leveled_item_chance_none_writes_one_fo4_percent_byte() {
        let interner = StringInterner::new();
        // LVLI 00B7C0 `LL_Loot_Legendary_Human_GeneralContainers_Large_Rare_ChanceNone`:
        // chance none 25.0, use all.
        let mut record = starfield_record(
            &interner,
            "LVLI",
            0x00_B7C0,
            0,
            &[("LVLD", "0000c841"), ("LVLM", "01"), ("LVLF", "0400")],
        );

        normalize(&mut record, &interner);

        assert_eq!(
            written_fo4_subrecord(&record, "LVLD", &interner),
            Some(vec![25])
        );
        assert_eq!(
            written_fo4_subrecord(&record, "LVLF", &interner),
            Some(vec![0x04])
        );
    }

    #[test]
    fn leveled_item_flags_keep_only_the_three_fo4_bits() {
        let interner = StringInterner::new();
        // LVLI 04B96E `LL_Loot_Book_Common_Any`: LVLF 0x011B is calc-from-all-levels,
        // each-item-in-count, both show-as-marker bits and do-all-before-repeating.
        let mut record = starfield_record(
            &interner,
            "LVLI",
            0x04_B96E,
            0,
            &[("LVLD", "00000000"), ("LVLM", "00"), ("LVLF", "1b01")],
        );

        normalize(&mut record, &interner);

        assert_eq!(
            written_fo4_subrecord(&record, "LVLF", &interner),
            Some(vec![0x03])
        );
        assert_eq!(
            written_fo4_subrecord(&record, "LVLD", &interner),
            Some(vec![0])
        );
    }

    #[test]
    fn leveled_npc_flags_drop_allow_shift_up_and_do_all_before_repeating() {
        let interner = StringInterner::new();
        // LVLN 0042A2 `_SQ_Group_Occupation_LChar_Old`: LVLF 0x0143.
        let mut record = starfield_record(
            &interner,
            "LVLN",
            0x00_42A2,
            0,
            &[("LVLD", "00000000"), ("LVLM", "00"), ("LVLF", "4301")],
        );

        normalize(&mut record, &interner);

        assert_eq!(
            written_fo4_subrecord(&record, "LVLF", &interner),
            Some(vec![0x03])
        );
        assert_eq!(
            written_fo4_subrecord(&record, "LVLD", &interner),
            Some(vec![0])
        );
    }

    #[test]
    fn vendor_radius_becomes_fo4_units_with_the_flag_bytes_in_place() {
        let interner = StringInterner::new();
        // FACT 09DBA6 `Vendor_City_CY_ManaakiFaction`: 0-24h, 20 m, fence,
        // specialized inventory, buys non-stolen.
        let mut record = starfield_record(
            &interner,
            "FACT",
            0x09_DBA6,
            0,
            &[("VENV", "000018000000a04101010100")],
        );

        normalize(&mut record, &interner);

        let venv = written_fo4_subrecord(&record, "VENV", &interner).unwrap();
        assert_eq!(hex::encode(&venv), "000018007805000001010100");
        assert_eq!(u16::from_le_bytes([venv[4], venv[5]]), 1400);
    }

    #[test]
    fn vendor_radius_past_the_fo4_u16_range_saturates() {
        let interner = StringInterner::new();
        // FACT 15B0CE `Vendor_City_Neon_DietrichSieghart_Faction`: 10,000 m.
        let mut record = starfield_record(
            &interner,
            "FACT",
            0x15_B0CE,
            0,
            &[("VENV", "0000180000401c4600010100")],
        );

        normalize(&mut record, &interner);

        assert_eq!(
            hex::encode(written_fo4_subrecord(&record, "VENV", &interner).unwrap()),
            "00001800ffff000000010100"
        );
    }

    #[test]
    fn moveable_static_mass_override_flags_become_fo4_on_local_map() {
        let interner = StringInterner::new();
        // MSTT 001621 `ThermalVentSmoke`: header 0x00010000, DATA 0x04 (Scale).
        let mut shown =
            starfield_record(&interner, "MSTT", 0x00_1621, 0x0001_0000, &[("DATA", "04")]);
        // MSTT 071A64 `FXP_UnityGroundRippleLg01`: header 0x00000300 carries Hidden
        // From Local Map, DATA 0x04.
        let mut hidden =
            starfield_record(&interner, "MSTT", 0x07_1A64, 0x0000_0300, &[("DATA", "04")]);

        normalize(&mut shown, &interner);
        normalize(&mut hidden, &interner);

        assert_eq!(
            written_fo4_subrecord(&shown, "DATA", &interner),
            Some(vec![1])
        );
        assert_eq!(
            written_fo4_subrecord(&hidden, "DATA", &interner),
            Some(vec![0])
        );
    }

    #[test]
    fn keyword_attraction_rule_is_dropped() {
        let interner = StringInterner::new();
        // KYWD 200ADC `CCT_Enviro_Raking` -> AORU 0D8784 `CCT_Enviro_Raking_Rule`.
        let mut record = starfield_record(
            &interner,
            "KYWD",
            0x20_0ADC,
            0,
            &[
                ("EDID", "4343545f456e7669726f5f52616b696e6700"),
                ("CNAM", "ffffff00"),
                ("TNAM", "0a000000"),
                ("FNAM", "00000000"),
                ("DATA", "84870d00"),
            ],
        );

        normalize(&mut record, &interner);

        let signatures: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(signatures, ["EDID", "CNAM", "TNAM", "FNAM"]);
    }

    #[test]
    fn unrelated_data_subrecords_are_untouched() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            crate::ids::SigCode(*b"STAT"),
            FormKey {
                local: 0x02_3C1D,
                plugin: interner.intern(PLUGIN),
            },
        );
        record.fields.push(crate::record::FieldEntry {
            sig: crate::ids::SubrecordSig(*b"DATA"),
            value: FieldValue::Uint(0x04),
        });

        normalize(&mut record, &interner);

        assert_eq!(record.fields[0].value, FieldValue::Uint(0x04));
    }
}
