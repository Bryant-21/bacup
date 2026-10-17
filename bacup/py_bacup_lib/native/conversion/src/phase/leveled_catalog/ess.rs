//! FO76 encounter-spawn (ESS) data for the leveled-list catalog (design 2.7, 4.3).

use std::collections::{BTreeSet, HashMap};

use esp_authoring_core::plugin_runtime::{ParsedSubrecord, effective_subrecords_for_record};
use serde_json::{Map, Value, json};

use super::decode::{self, Keys};
use crate::ids::SigCode;
use crate::session::HandleRawScan;

/// Catalog axis name and the FO76 FLST that lists its AVIFs.
pub(crate) const AXES: [(&str, &str); 5] = [
    ("main", "EMSMainKeywordMap"),
    ("sub", "EMSSubKeywordMap"),
    ("variant", "EMSVariantKeywordMap"),
    ("critter_a", "EMSCritterKeywordMapA"),
    ("critter_b", "EMSCritterKeywordMapB"),
];
/// FO76 executable defaults `fEMSLocEncounterRerollTimeSeconds` and `fEMSRunRecheckTime`.
pub(crate) const REROLL_SECONDS: f32 = 1200.0;
pub(crate) const RECHECK_SECONDS: f32 = 60.0;

#[derive(Debug, Default)]
pub(crate) struct SourceEss {
    /// Per axis, in FLST order: (AVIF, its NAM3 keyword).
    pub axes: [Vec<(u32, u32)>; 5],
    pub ignored_keyword: Option<u32>,
    pub allow_loaded_reroll_keyword: Option<u32>,
    /// LCTN -> PRPS rows (AVIF, weight).
    pub location_properties: Vec<(u32, Vec<(u32, f32)>)>,
}

pub(crate) fn axis_of(list_editor_id: &str) -> Option<usize> {
    AXES.iter().position(|(_, editor_id)| *editor_id == list_editor_id)
}

fn first_form(fields: &[ParsedSubrecord], signature: &str) -> Option<u32> {
    fields
        .iter()
        .find(|field| field.signature == signature)
        .and_then(|field| field.data.get(0..4))
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .filter(|raw| *raw != 0)
}

pub(crate) fn form_list(fields: &[ParsedSubrecord]) -> Vec<u32> {
    fields
        .iter()
        .filter(|field| field.signature == "LNAM")
        .filter_map(|field| field.data.get(0..4))
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
        .filter(|raw| *raw != 0)
        .collect()
}

pub(crate) fn has_keyword(fields: &[ParsedSubrecord], keyword: u32) -> bool {
    fields
        .iter()
        .filter(|field| field.signature == "KWDA")
        .flat_map(|field| field.data.chunks_exact(4))
        .any(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()) == keyword)
}

/// LCTN PRPS rows are 12 bytes: AVIF, value, unused.
pub(crate) fn location_properties(fields: &[ParsedSubrecord]) -> Vec<(u32, f32)> {
    fields
        .iter()
        .filter(|field| field.signature == "PRPS")
        .flat_map(|field| {
            let data: &[u8] = field.data.as_ref();
            data.chunks_exact(12)
                .map(|row| {
                    (
                        u32::from_le_bytes(row[0..4].try_into().unwrap()),
                        f32::from_le_bytes(row[4..8].try_into().unwrap()),
                    )
                })
                .collect::<Vec<_>>()
        })
        .filter(|(avif, weight)| *avif != 0 && weight.is_finite())
        .collect()
}

pub(crate) fn read(scan: &HandleRawScan<'_>) -> SourceEss {
    let fields_of = |raw: u32, read: &dyn Fn(&[ParsedSubrecord]) -> Option<u32>| {
        scan.with_record(raw, |record| read(&effective_subrecords_for_record(record))).flatten()
    };
    let editor_id = |raw: u32| {
        scan.with_record(raw, |record| decode::editor_id(&effective_subrecords_for_record(record)))
            .unwrap_or_default()
    };
    let list = |raw: u32| {
        scan.with_record(raw, |record| form_list(&effective_subrecords_for_record(record)))
            .unwrap_or_default()
    };
    let mut ess = SourceEss::default();
    for raw in scan.raw_form_ids_of_sig(SigCode::from_str("DFOB").unwrap()) {
        let target = fields_of(raw, &|fields| first_form(fields, "DATA"));
        match editor_id(raw).as_str() {
            "EMSActorValueLists_DO" => {
                for axis_list in target.map(list).unwrap_or_default() {
                    let Some(axis) = axis_of(&editor_id(axis_list)) else { continue };
                    ess.axes[axis] = list(axis_list)
                        .into_iter()
                        .filter_map(|avif| fields_of(avif, &|fields| first_form(fields, "NAM3")).map(|keyword| (avif, keyword)))
                        .collect();
                }
            }
            "EMSIgnoredLocationKeyword_DO" => ess.ignored_keyword = target,
            "EMSAllowLoadedRerollKeyword_DO" => ess.allow_loaded_reroll_keyword = target,
            _ => {}
        }
    }
    for raw in scan.raw_form_ids_of_sig(SigCode::from_str("LCTN").unwrap()) {
        // FO76's ESS never rolls a location carrying EMSIgnoredLocationKeyword (design 2.7).
        let (properties, ignored) = scan
            .with_record(raw, |record| {
                let fields = effective_subrecords_for_record(record);
                let ignored = ess.ignored_keyword.is_some_and(|keyword| has_keyword(&fields, keyword));
                (location_properties(&fields), ignored)
            })
            .unwrap_or_default();
        if !properties.is_empty() && !ignored {
            ess.location_properties.push((raw, properties));
        }
    }
    ess
}

pub(crate) fn references(source: &SourceEss, placed: &[(u32, u32)], own_index: u32, out: &mut BTreeSet<u32>) {
    for axis in &source.axes {
        for (avif, keyword) in axis {
            out.insert(*avif);
            out.insert(*keyword);
        }
    }
    out.extend(source.ignored_keyword);
    out.extend(source.allow_loaded_reroll_keyword);
    out.extend(source.location_properties.iter().map(|(location, _)| *location));
    out.extend(placed.iter().map(|(_, location)| own_index << 24 | location));
}

pub(crate) fn catalog_json(
    source: &SourceEss,
    keys: &dyn Keys,
    placed: &[(u32, u32)],
    output_plugin: &str,
    own_index: u32,
) -> Value {
    let mut axes = Map::new();
    let mut axis_of_avif: HashMap<u32, (usize, u32)> = HashMap::new();
    for (index, (name, _)) in AXES.iter().enumerate() {
        let mut rows = Vec::new();
        for (avif, keyword) in &source.axes[index] {
            axis_of_avif.insert(*avif, (index, *keyword));
            if let Some(keyword) = keys.output(*keyword) {
                rows.push(json!({ "actor_value": keys.output(*avif), "keyword": keyword }));
            }
        }
        axes.insert((*name).to_string(), Value::Array(rows));
    }
    let mut locations = Vec::new();
    for (location, properties) in &source.location_properties {
        let Some(location) = keys.output(*location) else { continue };
        let mut weights = Map::new();
        for (avif, weight) in properties {
            let Some((axis, keyword)) = axis_of_avif.get(avif) else { continue };
            let Some(keyword) = keys.output(*keyword) else { continue };
            weights
                .entry(AXES[*axis].0)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap()
                .push(json!([keyword, weight]));
        }
        if !weights.is_empty() {
            locations.push(json!({ "location": location, "weights": weights }));
        }
    }
    let placed_locations: Map<String, Value> = placed
        .iter()
        .filter_map(|(reference, location)| {
            keys.output(own_index << 24 | location)
                .map(|location| (format!("{output_plugin}:{reference:06X}"), json!(location)))
        })
        .collect();
    json!({
        "axes": axes,
        "ignored_keyword": source.ignored_keyword.and_then(|keyword| keys.output(keyword)),
        "allow_loaded_reroll_keyword": source.allow_loaded_reroll_keyword.and_then(|keyword| keys.output(keyword)),
        "reroll_seconds": REROLL_SECONDS,
        "recheck_seconds": RECHECK_SECONDS,
        "locations": locations,
        "placed_locations": placed_locations,
    })
}

#[cfg(test)]
mod tests {
    use esp_authoring_core::plugin_runtime::ParsedSubrecord;

    use super::super::decode::fake::FakeKeys;
    use super::*;

    fn field(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord { signature: signature.into(), data: data.into(), semantic_type: None }
    }

    fn prps(rows: &[(u32, f32)]) -> ParsedSubrecord {
        let mut data = Vec::new();
        for (avif, weight) in rows {
            data.extend(avif.to_le_bytes());
            data.extend(weight.to_le_bytes());
            data.extend(0u32.to_le_bytes());
        }
        field("PRPS", data)
    }

    #[test]
    fn axis_lists_are_named_by_editor_id() {
        assert_eq!(axis_of("EMSMainKeywordMap"), Some(0));
        assert_eq!(axis_of("EMSCritterKeywordMapB"), Some(4));
        assert_eq!(axis_of("EMSSomethingElse"), None);
    }

    #[test]
    fn ignored_locations_are_found_by_their_keyword() {
        let mut kwda = Vec::new();
        kwda.extend(0xC0u32.to_le_bytes());
        kwda.extend(0xC1u32.to_le_bytes());
        let fields = [field("KWSZ", 2u32.to_le_bytes().to_vec()), field("KWDA", kwda)];
        assert!(has_keyword(&fields, 0xC1));
        assert!(!has_keyword(&fields, 0xC2));
        assert!(!has_keyword(&[], 0xC1));
    }

    #[test]
    fn location_properties_skip_empty_and_non_finite_rows() {
        let rows = location_properties(&[prps(&[(0x10, 5.0), (0, 1.0), (0x11, f32::NAN)])]);
        assert_eq!(rows, vec![(0x10, 5.0)]);
    }

    #[test]
    fn locations_get_weights_per_axis_and_placed_actors_get_output_keys() {
        let mut source = SourceEss::default();
        source.axes[0] = vec![(0xA1, 0xB1), (0xA2, 0xB2)];
        // FO76's Sub list names one Main keyword (design 4.1 `ess.axes`).
        source.axes[1] = vec![(0xA3, 0xB1)];
        source.ignored_keyword = Some(0xC1);
        source.location_properties = vec![(0x0009_A451, vec![(0xA1, 5.0), (0xA3, 2.0)]), (0x0009_A452, vec![(0xFF, 1.0)])];
        let keys = FakeKeys::with(&[
            (0xB1, "SeventySix.esm:0000B1"),
            (0xB2, "SeventySix.esm:0000B2"),
            (0xC1, "SeventySix.esm:0000C1"),
            (0x0009_A451, "SeventySix.esm:09A451"),
            (0x0009_A452, "SeventySix.esm:09A452"),
        ]);
        let json = catalog_json(&source, &keys, &[(0x12_3456, 0x0009_A451)], "SeventySix.esm", 0);
        assert_eq!(json["axes"]["main"].as_array().unwrap().len(), 2);
        assert_eq!(json["axes"]["sub"][0]["keyword"], "SeventySix.esm:0000B1");
        assert_eq!(json["ignored_keyword"], "SeventySix.esm:0000C1");
        assert_eq!(json["allow_loaded_reroll_keyword"], Value::Null);
        assert_eq!(json["reroll_seconds"], 1200.0);
        assert_eq!(json["recheck_seconds"], 60.0);
        let locations = json["locations"].as_array().unwrap();
        assert_eq!(locations.len(), 1, "a location without axis weights is left out");
        assert_eq!(locations[0]["weights"]["main"][0], json!(["SeventySix.esm:0000B1", 5.0]));
        assert_eq!(locations[0]["weights"]["sub"][0], json!(["SeventySix.esm:0000B1", 2.0]));
        assert_eq!(json["placed_locations"]["SeventySix.esm:123456"], "SeventySix.esm:09A451");
    }
}
