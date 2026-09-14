//! Placeholder weather for the fo4→starfield pair.
//!
//! The translation map lists `WTHR` and `CLMT` under `skip_records`, so neither
//! record type is ever emitted into the Starfield plugin. That leaves every
//! surviving reference *into* the weather system pointing at a form that does
//! not exist:
//!
//! - `WRLD.CNAM` (Climate) — the map drops it outright, so an exterior
//!   worldspace arrives with no climate at all.
//! - `REGN.RDWT` (Weather Types) — the map carries it. Its rows decode to live
//!   `FieldValue::FormKey`s aimed at FO4 `WTHR` records; because those records
//!   are skipped, `FormKeyMapper` cannot map them, and in the pipeline's
//!   default `DeferAndFixup` mode it leaves the unmapped source key in place.
//!   The row would ship a dangling reference.
//!
//! The map cannot fix either one: its `defaults:` block is parsed but never
//! applied (`translator/maps.rs` lints it as an ignored top-level key), and a
//! `remap_formkey` transform only rewrites plugin names inside *string* leaves.
//!
//! Both are therefore repointed here at vanilla `Starfield.esm` forms. This
//! runs in `post_translate`, after `FormKeyMapper::rewrite_record`, so the
//! Starfield keys installed below are not themselves put through the map.

use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use smallvec::SmallVec;

const STARFIELD_MASTER: &str = "Starfield.esm";

/// `Starfield.esm` `CLMT` `00015F` `DefaultClimate` — the vanilla default. Its
/// weather list is a single 100%-chance entry pointing at `DefaultWeather`.
const SF_DEFAULT_CLIMATE: u32 = 0x0001_5F;

/// `Starfield.esm` `WTHR` `00015E` `DefaultWeather`. Starfield ships only three
/// `WTHR` records in total (`DefaultWeather`, `SpaceWeather`,
/// `NewAtlantisWeather50`); this is the clear-sky default the base climate uses.
const SF_DEFAULT_WEATHER: u32 = 0x0001_5E;

/// Field ids of a `REGN.RDWT` row, matching `decode_regn_rdwt` in
/// `source_read.rs` so a rewritten row is shaped exactly like a decoded one.
const RDWT_WEATHER: &str = "WeatherTypesWeather";
const RDWT_CHANCE: &str = "WeatherTypesChance";
const RDWT_GLOBAL: &str = "WeatherTypesGlobal";

fn starfield_form_key(interner: &StringInterner, local: u32) -> FormKey {
    FormKey {
        local,
        plugin: interner.intern(STARFIELD_MASTER),
    }
}

pub(super) fn point_weather_refs_at_vanilla_starfield(
    interner: &StringInterner,
    record: &mut Record,
) {
    match &record.sig.0 {
        b"WRLD" => set_worldspace_climate(interner, record),
        b"REGN" => collapse_region_weather_types(interner, record),
        _ => {}
    }
}

/// Give every converted worldspace the vanilla default climate.
///
/// Applied unconditionally rather than only when the FO4 source had a `CNAM`:
/// the map has already dropped the source value by the time this runs, and a
/// Starfield exterior with no climate has no sky at all, which is worse than
/// the vanilla default for any FO4 worldspace.
fn set_worldspace_climate(interner: &StringInterner, record: &mut Record) {
    let climate = FieldValue::FormKey(starfield_form_key(interner, SF_DEFAULT_CLIMATE));
    match record.fields.iter_mut().find(|e| e.sig.0 == *b"CNAM") {
        Some(entry) => entry.value = climate,
        None => record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CNAM").expect("literal 4CC"),
            value: climate,
        }),
    }
}

/// Collapse a region's weather table to one 100%-chance vanilla entry.
///
/// Every FO4 weather in the table maps to the same Starfield placeholder, so
/// preserving the original row count would only produce N identical rows with
/// chances that no longer sum meaningfully. The per-row Global override is
/// nulled: this pair's map doesn't translate FO4 `GLOB` records either, so the
/// reference would dangle.
fn collapse_region_weather_types(interner: &StringInterner, record: &mut Record) {
    let Some(entry) = record.fields.iter_mut().find(|e| e.sig.0 == *b"RDWT") else {
        return;
    };
    // An empty table stays empty — a region with no weather override inherits
    // the worldspace climate, which is already the placeholder.
    if matches!(&entry.value, FieldValue::List(rows) if rows.is_empty()) {
        return;
    }
    entry.value = FieldValue::List(vec![FieldValue::Struct(vec![
        (
            interner.intern(RDWT_WEATHER),
            FieldValue::FormKey(starfield_form_key(interner, SF_DEFAULT_WEATHER)),
        ),
        (interner.intern(RDWT_CHANCE), FieldValue::Uint(100)),
        // A struct member holding `FieldValue::None` encodes to zero bytes, so
        // the null Global must be written as an explicit zero FormID.
        (
            interner.intern(RDWT_GLOBAL),
            FieldValue::Bytes(SmallVec::from_slice(&[0u8; 4])),
        ),
    ])]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SigCode;

    fn new_record(sig: &str, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey::parse("000800@Fallout4.esm", interner).unwrap(),
        )
    }

    fn fo4_rdwt_row(interner: &StringInterner, weather_local: u32) -> FieldValue {
        FieldValue::Struct(vec![
            (
                interner.intern(RDWT_WEATHER),
                FieldValue::FormKey(FormKey {
                    local: weather_local,
                    plugin: interner.intern("Fallout4.esm"),
                }),
            ),
            (interner.intern(RDWT_CHANCE), FieldValue::Uint(50)),
            (interner.intern(RDWT_GLOBAL), FieldValue::None),
        ])
    }

    #[test]
    fn worldspace_without_climate_gains_the_vanilla_default() {
        let interner = StringInterner::new();
        let mut rec = new_record("WRLD", &interner);

        point_weather_refs_at_vanilla_starfield(&interner, &mut rec);

        let cnam = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "CNAM")
            .unwrap();
        assert_eq!(
            cnam.value,
            FieldValue::FormKey(starfield_form_key(&interner, SF_DEFAULT_CLIMATE))
        );
        assert_eq!(
            interner.resolve(starfield_form_key(&interner, 0).plugin),
            Some(STARFIELD_MASTER)
        );
    }

    #[test]
    fn worldspace_with_a_stale_fo4_climate_is_overwritten() {
        let interner = StringInterner::new();
        let mut rec = new_record("WRLD", &interner);
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CNAM").unwrap(),
            value: FieldValue::FormKey(FormKey::parse("00015B@Fallout4.esm", &interner).unwrap()),
        });

        point_weather_refs_at_vanilla_starfield(&interner, &mut rec);

        let cnam = rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "CNAM")
            .unwrap();
        assert_eq!(
            cnam.value,
            FieldValue::FormKey(starfield_form_key(&interner, SF_DEFAULT_CLIMATE))
        );
        assert_eq!(
            rec.fields
                .iter()
                .filter(|e| e.sig.as_str() == "CNAM")
                .count(),
            1
        );
    }

    #[test]
    fn region_weather_table_collapses_to_one_vanilla_row() {
        let interner = StringInterner::new();
        let mut rec = new_record("REGN", &interner);
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDWT").unwrap(),
            value: FieldValue::List(vec![
                fo4_rdwt_row(&interner, 0x0002_C6B0),
                fo4_rdwt_row(&interner, 0x0002_C6B1),
            ]),
        });

        point_weather_refs_at_vanilla_starfield(&interner, &mut rec);

        let FieldValue::List(rows) = &rec
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "RDWT")
            .unwrap()
            .value
        else {
            panic!("RDWT should stay a list of rows");
        };
        assert_eq!(rows.len(), 1);
        let FieldValue::Struct(members) = &rows[0] else {
            panic!("row should be a struct");
        };
        assert_eq!(
            members[0].1,
            FieldValue::FormKey(starfield_form_key(&interner, SF_DEFAULT_WEATHER))
        );
        assert_eq!(members[1].1, FieldValue::Uint(100));
        // 4 explicit zero bytes, not FieldValue::None, so the row still encodes
        // to the 12 bytes `array_struct:I,I,I` requires.
        assert_eq!(
            members[2].1,
            FieldValue::Bytes(SmallVec::from_slice(&[0u8; 4]))
        );
    }

    #[test]
    fn no_fo4_weather_formkey_survives_in_a_region() {
        let interner = StringInterner::new();
        let mut rec = new_record("REGN", &interner);
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDWT").unwrap(),
            value: FieldValue::List(vec![fo4_rdwt_row(&interner, 0x0002_C6B0)]),
        });

        point_weather_refs_at_vanilla_starfield(&interner, &mut rec);

        let mut plugins = Vec::new();
        collect_plugins(
            &rec.fields
                .iter()
                .find(|e| e.sig.as_str() == "RDWT")
                .unwrap()
                .value,
            &interner,
            &mut plugins,
        );
        assert_eq!(plugins, vec![STARFIELD_MASTER.to_string()]);
    }

    fn collect_plugins(value: &FieldValue, interner: &StringInterner, out: &mut Vec<String>) {
        match value {
            FieldValue::FormKey(fk) => {
                out.push(interner.resolve(fk.plugin).unwrap_or_default().to_string())
            }
            FieldValue::List(items) => {
                for item in items {
                    collect_plugins(item, interner, out);
                }
            }
            FieldValue::Struct(members) => {
                for (_, v) in members {
                    collect_plugins(v, interner, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn empty_region_weather_table_is_left_empty() {
        let interner = StringInterner::new();
        let mut rec = new_record("REGN", &interner);
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDWT").unwrap(),
            value: FieldValue::List(Vec::new()),
        });

        point_weather_refs_at_vanilla_starfield(&interner, &mut rec);

        assert_eq!(
            rec.fields
                .iter()
                .find(|e| e.sig.as_str() == "RDWT")
                .unwrap()
                .value,
            FieldValue::List(Vec::new())
        );
    }

    #[test]
    fn unrelated_records_are_untouched() {
        let interner = StringInterner::new();
        let mut rec = new_record("STAT", &interner);

        point_weather_refs_at_vanilla_starfield(&interner, &mut rec);

        assert!(rec.fields.is_empty());
    }
}
