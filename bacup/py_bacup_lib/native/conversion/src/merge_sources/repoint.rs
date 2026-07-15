use std::collections::{HashMap, HashSet};

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::{
    extract_nested_form_ids, extract_skyrim_nvmi_form_ids, rewrite_schema_form_ids_in_subrecord,
    rewrite_skyrim_nvmi_form_ids,
};
use esp_authoring_core::plugin_runtime::{
    CompiledSchema, ParsedRecord, ParsedSubrecord, schema_record_spec, schema_subrecord_spec,
};
use rustc_hash::FxHashMap;

#[derive(Debug, Default)]
pub(crate) struct RepointStats {
    pub remapped: u64,
    pub dangling: Vec<DanglingReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct DanglingReference {
    pub owner_signature: String,
    pub owner_form_id: u32,
    pub subrecord_signature: Option<String>,
    pub raw: u32,
}

impl DanglingReference {
    pub(crate) fn record(
        owner_signature: &str,
        owner_form_id: u32,
        subrecord_signature: &str,
        raw: u32,
    ) -> Self {
        Self {
            owner_signature: owner_signature.to_string(),
            owner_form_id,
            subrecord_signature: Some(subrecord_signature.to_string()),
            raw,
        }
    }

    pub(crate) fn group(raw: u32) -> Self {
        Self {
            owner_signature: "GRUP".to_string(),
            owner_form_id: 0,
            subrecord_signature: None,
            raw,
        }
    }

    pub(crate) fn render(&self) -> String {
        match &self.subrecord_signature {
            Some(subrecord) => format!(
                "{}:{:08X}:{}:{:08X}",
                self.owner_signature, self.owner_form_id, subrecord, self.raw
            ),
            None => format!("{}:{:08X}", self.owner_signature, self.raw),
        }
    }
}

pub(crate) fn repoint_record(
    record: &mut ParsedRecord,
    remap: &FxHashMap<u32, u32>,
    primary_ids: &HashSet<u32>,
    stats: &mut RepointStats,
) {
    repoint_record_with_schema(record, remap, primary_ids, stats, None);
}

pub(crate) fn repoint_record_with_schema(
    record: &mut ParsedRecord,
    remap: &FxHashMap<u32, u32>,
    primary_ids: &HashSet<u32>,
    stats: &mut RepointStats,
    schema: Option<&CompiledSchema>,
) {
    let signature = record.signature.to_string();
    let owner_form_id = record.form_id;
    let mut rewrite = |subrecord_signature: &str, raw: u32| {
        if raw == 0 || raw < 0x800 || raw >= 0xFF00_0000 {
            return None;
        }
        if let Some(&mapped) = remap.get(&raw) {
            if mapped != raw {
                stats.remapped += 1;
                return Some(mapped);
            }
        } else if !primary_ids.contains(&raw) {
            stats.dangling.push(DanglingReference::record(
                &signature,
                owner_form_id,
                subrecord_signature,
                raw,
            ));
        }
        None
    };
    if rewrite_merge_formids_in_subrecords(&signature, &mut record.subrecords, schema, &mut rewrite)
    {
        record.raw_payload = None;
    }
}

pub(crate) fn rewrite_merge_formids_in_subrecords(
    record_signature: &str,
    subrecords: &mut [ParsedSubrecord],
    schema: Option<&CompiledSchema>,
    rewrite: &mut dyn FnMut(&str, u32) -> Option<u32>,
) -> bool {
    let record_spec = schema.and_then(|schema| schema_record_spec(schema, record_signature));
    let skyrim_navi_v12 = is_skyrim_navi_v12(record_signature, subrecords);
    let mut occurrence_counts = HashMap::new();
    let mut changed = false;
    for subrecord in subrecords {
        let occurrence = {
            let count = occurrence_counts
                .entry(subrecord.signature.clone())
                .or_insert(0_usize);
            let current = *count;
            *count += 1;
            current
        };
        let single_formid = subrecord.semantic_type.as_deref() == Some("formid")
            || (record_signature == "LAND"
                && matches!(subrecord.signature.as_str(), "BTXT" | "ATXT"));
        if skyrim_navi_v12 && subrecord.signature.as_str() == "NVMI" {
            let mut data = subrecord.data.to_vec();
            let signature = subrecord.signature.to_string();
            let mut nvmi_rewrite = |raw| rewrite(&signature, raw);
            if rewrite_skyrim_nvmi_form_ids(&mut data, &mut nvmi_rewrite) {
                subrecord.data = Bytes::from(data);
                changed = true;
            }
            continue;
        }
        if single_formid && subrecord.data.len() >= 4 {
            changed |= rewrite_at(
                &mut subrecord.data,
                0,
                subrecord.signature.as_str(),
                rewrite,
            );
            continue;
        }
        if subrecord.semantic_type.as_deref() == Some("formid_array")
            && subrecord.data.len() % 4 == 0
        {
            for offset in (0..subrecord.data.len()).step_by(4) {
                changed |= rewrite_at(
                    &mut subrecord.data,
                    offset,
                    subrecord.signature.as_str(),
                    rewrite,
                );
            }
            continue;
        }
        if matches!(
            subrecord.signature.as_str(),
            "MODS" | "MO2S" | "MO3S" | "MO4S"
        ) {
            changed |=
                rewrite_mods_formids(&mut subrecord.data, subrecord.signature.as_str(), rewrite);
            continue;
        }
        if let (Some(schema), Some(record_spec)) = (schema, record_spec)
            && let Some(subrecord_spec) =
                schema_subrecord_spec(record_spec, subrecord.signature.as_str(), occurrence)
        {
            let mut data = subrecord.data.to_vec();
            let signature = subrecord.signature.to_string();
            let mut schema_rewrite = |raw| rewrite(&signature, raw);
            if rewrite_schema_form_ids_in_subrecord(
                subrecord_spec,
                schema,
                &mut data,
                &mut schema_rewrite,
            ) {
                subrecord.data = Bytes::from(data);
                changed = true;
            }
        }
    }
    changed
}

pub(crate) fn collect_merge_formids_in_subrecords(
    record_signature: &str,
    subrecords: &[ParsedSubrecord],
    schema: Option<&CompiledSchema>,
    visit: &mut dyn FnMut(&str, u32),
) {
    let record_spec = schema.and_then(|schema| schema_record_spec(schema, record_signature));
    let skyrim_navi_v12 = is_skyrim_navi_v12(record_signature, subrecords);
    let mut occurrence_counts = HashMap::new();
    for subrecord in subrecords {
        let occurrence = {
            let count = occurrence_counts
                .entry(subrecord.signature.clone())
                .or_insert(0_usize);
            let current = *count;
            *count += 1;
            current
        };
        let signature = subrecord.signature.as_str();
        let single_formid = subrecord.semantic_type.as_deref() == Some("formid")
            || (record_signature == "LAND" && matches!(signature, "BTXT" | "ATXT"));
        if skyrim_navi_v12 && signature == "NVMI" {
            let mut form_ids = Vec::new();
            extract_skyrim_nvmi_form_ids(&subrecord.data, &mut form_ids);
            for raw in form_ids {
                visit(signature, raw);
            }
            continue;
        }
        if single_formid && subrecord.data.len() >= 4 {
            visit(
                signature,
                u32::from_le_bytes(subrecord.data[0..4].try_into().unwrap()),
            );
            continue;
        }
        if subrecord.semantic_type.as_deref() == Some("formid_array")
            && subrecord.data.len() % 4 == 0
        {
            for chunk in subrecord.data.chunks_exact(4) {
                visit(signature, u32::from_le_bytes(chunk.try_into().unwrap()));
            }
            continue;
        }
        if matches!(signature, "MODS" | "MO2S" | "MO3S" | "MO4S") {
            collect_mods_formids(&subrecord.data, signature, visit);
            continue;
        }
        if let (Some(schema), Some(record_spec)) = (schema, record_spec)
            && let Some(subrecord_spec) = schema_subrecord_spec(record_spec, signature, occurrence)
        {
            let mut form_ids = Vec::new();
            extract_nested_form_ids(subrecord_spec, schema, &subrecord.data, &mut form_ids);
            for raw in form_ids {
                visit(signature, raw);
            }
        }
    }
}

fn is_skyrim_navi_v12(record_signature: &str, subrecords: &[ParsedSubrecord]) -> bool {
    record_signature == "NAVI"
        && subrecords.iter().any(|subrecord| {
            subrecord.signature.as_str() == "NVER"
                && subrecord.data.get(0..4) == Some(12u32.to_le_bytes().as_slice())
        })
}

fn collect_mods_formids(data: &[u8], subrecord_signature: &str, visit: &mut dyn FnMut(&str, u32)) {
    if data.len() < 4 {
        return;
    }
    let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let mut offset = 4_usize;
    for _ in 0..count {
        let Some(length_bytes) = data.get(offset..offset + 4) else {
            return;
        };
        let name_length = u32::from_le_bytes(length_bytes.try_into().unwrap()) as usize;
        offset = offset.saturating_add(4).saturating_add(name_length);
        let Some(form_id_bytes) = data.get(offset..offset + 4) else {
            return;
        };
        visit(
            subrecord_signature,
            u32::from_le_bytes(form_id_bytes.try_into().unwrap()),
        );
        offset = offset.saturating_add(8);
    }
}

fn rewrite_mods_formids(
    data: &mut Bytes,
    subrecord_signature: &str,
    rewrite: &mut dyn FnMut(&str, u32) -> Option<u32>,
) -> bool {
    if data.len() < 4 {
        return false;
    }
    let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let mut offset = 4_usize;
    let mut changed = false;
    for _ in 0..count {
        let Some(length_bytes) = data.get(offset..offset + 4) else {
            return changed;
        };
        let name_length = u32::from_le_bytes(length_bytes.try_into().unwrap()) as usize;
        offset = offset.saturating_add(4).saturating_add(name_length);
        if data.len() < offset + 8 {
            return changed;
        }
        changed |= rewrite_at(data, offset, subrecord_signature, rewrite);
        offset += 8;
    }
    changed
}

fn rewrite_at(
    data: &mut Bytes,
    offset: usize,
    subrecord_signature: &str,
    rewrite: &mut dyn FnMut(&str, u32) -> Option<u32>,
) -> bool {
    if data.len() < offset + 4 {
        return false;
    }
    let raw = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let Some(mapped) = rewrite(subrecord_signature, raw) else {
        return false;
    };
    if mapped == raw {
        return false;
    }
    let mut owned = data.to_vec();
    owned[offset..offset + 4].copy_from_slice(&mapped.to_le_bytes());
    *data = Bytes::from(owned);
    true
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{ParsedSubrecord, compiled_schema_for_game_str};

    use super::*;
    use crate::merge_sources::test_util::{formid_sub, rec};

    fn value(subrecord: &ParsedSubrecord, offset: usize) -> u32 {
        u32::from_le_bytes(subrecord.data[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn formid_field_is_repointed() {
        let mut record = rec("ACTI", 0xA000, "Item");
        record.subrecords.push(formid_sub("SCRI", 0x9900));
        let mut stats = RepointStats::default();
        repoint_record(
            &mut record,
            &FxHashMap::from_iter([(0x9900, 0x1200)]),
            &HashSet::new(),
            &mut stats,
        );
        assert_eq!(value(record.subrecords.last().unwrap(), 0), 0x1200);
        assert_eq!(stats.remapped, 1);
    }

    #[test]
    fn formid_array_repoints_each_entry() {
        let mut record = rec("FLST", 0xA000, "List");
        let mut data = Vec::new();
        for id in [0x9900_u32, 0x9A00, 0x14] {
            data.extend_from_slice(&id.to_le_bytes());
        }
        record.subrecords.push(ParsedSubrecord {
            signature: "LNAM".into(),
            data: data.into(),
            semantic_type: Some("formid_array".to_string()),
        });
        let mut stats = RepointStats::default();
        repoint_record(
            &mut record,
            &FxHashMap::from_iter([(0x9900, 0x1200), (0x9A00, 0x1300)]),
            &HashSet::new(),
            &mut stats,
        );
        let subrecord = record.subrecords.last().unwrap();
        assert_eq!(value(subrecord, 0), 0x1200);
        assert_eq!(value(subrecord, 4), 0x1300);
        assert_eq!(value(subrecord, 8), 0x14);
        assert_eq!(stats.remapped, 2);
    }

    #[test]
    fn reserved_zero_and_runtime_ids_pass_through() {
        for id in [0_u32, 0x14, 0xFF00_0001] {
            let mut record = rec("XXXX", 0xA000, "Record");
            record.subrecords.push(formid_sub("DATA", id));
            let mut stats = RepointStats::default();
            repoint_record(
                &mut record,
                &FxHashMap::default(),
                &HashSet::new(),
                &mut stats,
            );
            assert_eq!(value(record.subrecords.last().unwrap(), 0), id);
            assert!(stats.dangling.is_empty());
        }
    }

    #[test]
    fn unknown_non_primary_id_is_dangling() {
        let mut record = rec("XXXX", 0xA000, "Record");
        record.subrecords.push(formid_sub("DATA", 0xBEEF));
        let mut stats = RepointStats::default();
        repoint_record(
            &mut record,
            &FxHashMap::default(),
            &HashSet::new(),
            &mut stats,
        );
        assert_eq!(stats.dangling[0].render(), "XXXX:0000A000:DATA:0000BEEF");
    }

    #[test]
    fn primary_id_passes_silently() {
        let mut record = rec("XXXX", 0xA000, "Record");
        record.subrecords.push(formid_sub("DATA", 0xBEEF));
        let mut stats = RepointStats::default();
        repoint_record(
            &mut record,
            &FxHashMap::default(),
            &HashSet::from([0xBEEF]),
            &mut stats,
        );
        assert!(stats.dangling.is_empty());
    }

    #[test]
    fn land_texture_layer_is_repointed_without_semantic_type() {
        let mut record = rec("LAND", 0xA000, "");
        record.subrecords.push(ParsedSubrecord {
            signature: "BTXT".into(),
            data: Bytes::copy_from_slice(&0x9900_u32.to_le_bytes()),
            semantic_type: None,
        });
        let mut stats = RepointStats::default();
        repoint_record(
            &mut record,
            &FxHashMap::from_iter([(0x9900, 0x1200)]),
            &HashSet::new(),
            &mut stats,
        );
        assert_eq!(value(record.subrecords.last().unwrap(), 0), 0x1200);
    }

    #[test]
    fn schema_float_with_contextual_signature_is_not_treated_as_formid() {
        let mut record = rec("FACT", 0xA000, "Faction");
        record.subrecords.push(ParsedSubrecord {
            signature: "CNAM".into(),
            data: Bytes::copy_from_slice(&1.0_f32.to_le_bytes()),
            semantic_type: None,
        });
        let schema = compiled_schema_for_game_str("fnv").unwrap();
        let mut stats = RepointStats::default();
        repoint_record_with_schema(
            &mut record,
            &FxHashMap::default(),
            &HashSet::new(),
            &mut stats,
            Some(&schema),
        );
        assert!(stats.dangling.is_empty());
        assert_eq!(value(record.subrecords.last().unwrap(), 0), 0x3F80_0000);
    }

    #[test]
    fn mods_variable_length_name_repoints_texture_formid() {
        let name = b"dSoundHead\0";
        let mut data = Vec::new();
        data.extend_from_slice(&1_u32.to_le_bytes());
        data.extend_from_slice(&(name.len() as u32).to_le_bytes());
        data.extend_from_slice(name);
        data.extend_from_slice(&0x9900_u32.to_le_bytes());
        data.extend_from_slice(&0_i32.to_le_bytes());
        let mut record = rec("HDPT", 0xA000, "HeadPart");
        record.subrecords.push(ParsedSubrecord {
            signature: "MODS".into(),
            data: Bytes::from(data),
            semantic_type: None,
        });
        let schema = compiled_schema_for_game_str("fnv").unwrap();
        let mut stats = RepointStats::default();
        repoint_record_with_schema(
            &mut record,
            &FxHashMap::from_iter([(0x9900, 0x1200)]),
            &HashSet::new(),
            &mut stats,
            Some(&schema),
        );
        let mods = record.subrecords.last().unwrap();
        assert_eq!(value(mods, 8 + name.len()), 0x1200);
    }
}
