use std::collections::{BTreeMap, HashMap, HashSet};

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{CompiledSchema, ParsedItem, ParsedRecord};

use super::repoint::{DanglingReference, collect_merge_formids_in_subrecords};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SanitizationStats {
    pub occurrences: u64,
    pub by_context: BTreeMap<String, u64>,
    pub by_action: BTreeMap<String, u64>,
    pub dropped_owners: BTreeMap<String, u64>,
    pub dangling: Vec<String>,
}

type PendingKey = (u32, String, String, u32);

pub(crate) fn sanitize_dangling_references(
    items: &mut Vec<ParsedItem>,
    mut pending: Vec<DanglingReference>,
    primary_schema: Option<&CompiledSchema>,
    grafted_schema: Option<&CompiledSchema>,
    grafted_record_ids: &HashSet<u32>,
) -> SanitizationStats {
    let mut stats = SanitizationStats::default();
    loop {
        if pending.is_empty() {
            return stats;
        }
        if pending.iter().any(|reference| !is_supported(reference)) {
            stats.dangling = render_sorted(&pending);
            return stats;
        }
        let (remaining, dropped_ids) = sanitize_items(items, &pending, &mut stats);
        if !remaining.is_empty() {
            stats.dangling = render_sorted(&remaining);
            return stats;
        }
        if dropped_ids.is_empty() {
            return stats;
        }
        pending = collect_references_to_dropped_owners(
            items,
            &dropped_ids,
            primary_schema,
            grafted_schema,
            grafted_record_ids,
        );
    }
}

fn render_sorted(references: &[DanglingReference]) -> Vec<String> {
    let mut rendered = references
        .iter()
        .map(DanglingReference::render)
        .collect::<Vec<_>>();
    rendered.sort();
    rendered
}

fn is_supported(reference: &DanglingReference) -> bool {
    matches!(
        (
            reference.owner_signature.as_str(),
            reference.subrecord_signature.as_deref()
        ),
        ("CELL", Some("XCLR"))
            | ("NAVM", Some("NVEX"))
            | ("REFR", Some("XAPR" | "XNDP" | "XLKR"))
            | ("ACRE", Some("XESP"))
            | ("INFO", Some("QSTI" | "PNAM"))
    )
}

fn collect_references_to_dropped_owners(
    items: &[ParsedItem],
    dropped_ids: &HashSet<u32>,
    primary_schema: Option<&CompiledSchema>,
    grafted_schema: Option<&CompiledSchema>,
    grafted_record_ids: &HashSet<u32>,
) -> Vec<DanglingReference> {
    let mut references = Vec::new();
    collect_references_into(
        items,
        dropped_ids,
        primary_schema,
        grafted_schema,
        grafted_record_ids,
        &mut references,
    );
    references
}

fn collect_references_into(
    items: &[ParsedItem],
    dropped_ids: &HashSet<u32>,
    primary_schema: Option<&CompiledSchema>,
    grafted_schema: Option<&CompiledSchema>,
    grafted_record_ids: &HashSet<u32>,
    references: &mut Vec<DanglingReference>,
) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                let schema = if grafted_record_ids.contains(&record.form_id) {
                    grafted_schema
                } else {
                    primary_schema
                };
                collect_merge_formids_in_subrecords(
                    record.signature.as_str(),
                    &record.subrecords,
                    schema,
                    &mut |subrecord_signature, raw| {
                        if dropped_ids.contains(&raw) {
                            references.push(DanglingReference::record(
                                record.signature.as_str(),
                                record.form_id,
                                subrecord_signature,
                                raw,
                            ));
                        }
                    },
                );
            }
            ParsedItem::Group(group) => {
                if matches!(group.group_type, 1 | 6 | 7 | 8 | 9 | 10) {
                    let raw = u32::from_le_bytes(group.label);
                    if dropped_ids.contains(&raw) {
                        references.push(DanglingReference::group(raw));
                    }
                }
                collect_references_into(
                    &group.children,
                    dropped_ids,
                    primary_schema,
                    grafted_schema,
                    grafted_record_ids,
                    references,
                );
            }
        }
    }
}

fn sanitize_items(
    items: &mut Vec<ParsedItem>,
    pending: &[DanglingReference],
    stats: &mut SanitizationStats,
) -> (Vec<DanglingReference>, HashSet<u32>) {
    let mut counts = pending_counts(pending);
    let owner_ids = pending
        .iter()
        .map(|reference| reference.owner_form_id)
        .collect::<HashSet<_>>();
    let mut dropped_ids = HashSet::new();
    sanitize_items_with_counts(items, &owner_ids, &mut counts, &mut dropped_ids, stats);
    let remaining = remaining_references(pending, &counts);
    (remaining, dropped_ids)
}

fn remaining_references(
    pending: &[DanglingReference],
    counts: &HashMap<PendingKey, u64>,
) -> Vec<DanglingReference> {
    let mut remaining_counts = counts.clone();
    let mut remaining = Vec::new();
    for reference in pending {
        let Some(subrecord) = &reference.subrecord_signature else {
            remaining.push(reference.clone());
            continue;
        };
        let key = (
            reference.owner_form_id,
            reference.owner_signature.clone(),
            subrecord.clone(),
            reference.raw,
        );
        let Some(count) = remaining_counts.get_mut(&key) else {
            continue;
        };
        remaining.push(reference.clone());
        *count -= 1;
        if *count == 0 {
            remaining_counts.remove(&key);
        }
    }
    remaining
}

fn pending_counts(pending: &[DanglingReference]) -> HashMap<PendingKey, u64> {
    let mut counts = HashMap::new();
    for reference in pending {
        let Some(subrecord) = &reference.subrecord_signature else {
            continue;
        };
        *counts
            .entry((
                reference.owner_form_id,
                reference.owner_signature.clone(),
                subrecord.clone(),
                reference.raw,
            ))
            .or_default() += 1;
    }
    counts
}

fn sanitize_items_with_counts(
    items: &mut Vec<ParsedItem>,
    owner_ids: &HashSet<u32>,
    counts: &mut HashMap<PendingKey, u64>,
    dropped_ids: &mut HashSet<u32>,
    stats: &mut SanitizationStats,
) {
    items.retain_mut(|item| match item {
        ParsedItem::Record(record) => {
            if !owner_ids.contains(&record.form_id) {
                true
            } else if sanitize_record(record, counts, stats) {
                dropped_ids.insert(record.form_id);
                false
            } else {
                true
            }
        }
        ParsedItem::Group(group) => {
            sanitize_items_with_counts(&mut group.children, owner_ids, counts, dropped_ids, stats);
            true
        }
    });
}

fn sanitize_record(
    record: &mut ParsedRecord,
    counts: &mut HashMap<PendingKey, u64>,
    stats: &mut SanitizationStats,
) -> bool {
    if record.signature.as_str() == "ACRE" {
        let occurrences = take_owner_context(counts, record, "XESP");
        if occurrences != 0 {
            stats.note("ACRE.XESP", "drop_owner", occurrences);
            *stats.dropped_owners.entry("ACRE".to_string()).or_default() += 1;
            return true;
        }
    }
    if record.signature.as_str() == "INFO" {
        let qsti = take_owner_context(counts, record, "QSTI");
        let pnam = take_owner_context(counts, record, "PNAM");
        if qsti != 0 || pnam != 0 {
            stats.note("INFO.QSTI", "drop_owner", qsti);
            stats.note("INFO.PNAM", "drop_owner", pnam);
            *stats.dropped_owners.entry("INFO".to_string()).or_default() += 1;
            return true;
        }
    }
    let changed = match record.signature.as_str() {
        "CELL" => sanitize_cell(record, counts, stats),
        "NAVM" => sanitize_navm(record, counts, stats),
        "REFR" => sanitize_refr(record, counts, stats),
        _ => false,
    };
    if changed {
        record.raw_payload = None;
    }
    false
}

fn take_owner_context(
    counts: &mut HashMap<PendingKey, u64>,
    record: &ParsedRecord,
    subrecord_signature: &str,
) -> u64 {
    let keys = counts
        .keys()
        .filter(|(owner_id, owner_signature, subrecord, _)| {
            *owner_id == record.form_id
                && owner_signature.as_str() == record.signature.as_str()
                && subrecord == subrecord_signature
        })
        .cloned()
        .collect::<Vec<_>>();
    keys.into_iter().filter_map(|key| counts.remove(&key)).sum()
}

fn take_one(
    counts: &mut HashMap<PendingKey, u64>,
    owner_form_id: u32,
    owner_signature: &str,
    subrecord_signature: &str,
    raw: u32,
) -> bool {
    let key = (
        owner_form_id,
        owner_signature.to_string(),
        subrecord_signature.to_string(),
        raw,
    );
    let Some(count) = counts.get_mut(&key) else {
        return false;
    };
    *count -= 1;
    if *count == 0 {
        counts.remove(&key);
    }
    true
}

fn sanitize_cell(
    record: &mut ParsedRecord,
    counts: &mut HashMap<PendingKey, u64>,
    stats: &mut SanitizationStats,
) -> bool {
    let mut removed = 0_u64;
    let owner_form_id = record.form_id;
    let owner_signature = record.signature.to_string();
    record.subrecords.retain_mut(|subrecord| {
        if subrecord.signature.as_str() != "XCLR" {
            return true;
        }
        let mut kept = Vec::with_capacity(subrecord.data.len());
        let chunks = subrecord.data.len() / 4;
        for chunk in subrecord.data[..chunks * 4].chunks_exact(4) {
            let raw = u32::from_le_bytes(chunk.try_into().unwrap());
            if take_one(counts, owner_form_id, &owner_signature, "XCLR", raw) {
                removed += 1;
            } else {
                kept.extend_from_slice(chunk);
            }
        }
        kept.extend_from_slice(&subrecord.data[chunks * 4..]);
        subrecord.data = Bytes::from(kept);
        !subrecord.data.is_empty()
    });
    stats.note("CELL.XCLR", "remove_array_element", removed);
    removed != 0
}

fn sanitize_navm(
    record: &mut ParsedRecord,
    counts: &mut HashMap<PendingKey, u64>,
    stats: &mut SanitizationStats,
) -> bool {
    let mut zeroed = 0_u64;
    let owner_form_id = record.form_id;
    let owner_signature = record.signature.to_string();
    for subrecord in &mut record.subrecords {
        if subrecord.signature.as_str() != "NVEX" {
            continue;
        }
        let mut data = subrecord.data.to_vec();
        for entry in data.chunks_exact_mut(10) {
            let raw = u32::from_le_bytes(entry[4..8].try_into().unwrap());
            if take_one(counts, owner_form_id, &owner_signature, "NVEX", raw) {
                entry[4..8].fill(0);
                zeroed += 1;
            }
        }
        subrecord.data = Bytes::from(data);
    }
    stats.note("NAVM.NVEX", "zero_formid", zeroed);
    zeroed != 0
}

fn sanitize_refr(
    record: &mut ParsedRecord,
    counts: &mut HashMap<PendingKey, u64>,
    stats: &mut SanitizationStats,
) -> bool {
    let mut removed_xapr = 0_u64;
    let owner_form_id = record.form_id;
    let owner_signature = record.signature.to_string();
    record.subrecords.retain_mut(|subrecord| {
        if subrecord.signature.as_str() != "XAPR" {
            return true;
        }
        let mut kept = Vec::with_capacity(subrecord.data.len());
        let entries = subrecord.data.len() / 8;
        for entry in subrecord.data[..entries * 8].chunks_exact(8) {
            let raw = u32::from_le_bytes(entry[0..4].try_into().unwrap());
            if take_one(counts, owner_form_id, &owner_signature, "XAPR", raw) {
                removed_xapr += 1;
            } else {
                kept.extend_from_slice(entry);
            }
        }
        kept.extend_from_slice(&subrecord.data[entries * 8..]);
        subrecord.data = Bytes::from(kept);
        !subrecord.data.is_empty()
    });
    if removed_xapr != 0
        && !record
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature.as_str() == "XAPR")
    {
        record
            .subrecords
            .retain(|subrecord| subrecord.signature.as_str() != "XAPD");
    }
    stats.note("REFR.XAPR", "remove_occurrence", removed_xapr);

    let mut removed_xndp = 0_u64;
    record.subrecords.retain(|subrecord| {
        if subrecord.signature.as_str() != "XNDP" {
            return true;
        }
        let remove = read_u32(&subrecord.data, 0)
            .is_some_and(|raw| take_one(counts, owner_form_id, &owner_signature, "XNDP", raw));
        if remove {
            removed_xndp += 1;
        }
        !remove
    });
    stats.note("REFR.XNDP", "remove_subrecord", removed_xndp);

    let mut removed_xlkr = 0_u64;
    let mut subrecords = std::mem::take(&mut record.subrecords)
        .into_iter()
        .peekable();
    let mut kept = Vec::new();
    while let Some(subrecord) = subrecords.next() {
        let remove = subrecord.signature.as_str() == "XLKR"
            && read_u32(&subrecord.data, 0)
                .is_some_and(|raw| take_one(counts, owner_form_id, &owner_signature, "XLKR", raw));
        if !remove {
            kept.push(subrecord);
            continue;
        }
        removed_xlkr += 1;
        if matches!(subrecords.peek(), Some(companion) if companion.signature.as_str() == "XCLP") {
            subrecords.next();
        }
    }
    record.subrecords = kept;
    stats.note("REFR.XLKR", "remove_link_and_companion", removed_xlkr);
    removed_xapr != 0 || removed_xndp != 0 || removed_xlkr != 0
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
}

impl SanitizationStats {
    fn note(&mut self, context: &str, action: &str, occurrences: u64) {
        if occurrences == 0 {
            return;
        }
        self.occurrences += occurrences;
        *self.by_context.entry(context.to_string()).or_default() += occurrences;
        *self.by_action.entry(action.to_string()).or_default() += occurrences;
    }

    pub(crate) fn dropped_owner_count(&self) -> u64 {
        self.dropped_owners.values().sum()
    }
}

#[cfg(test)]
mod tests {
    use esp_authoring_core::plugin_runtime::{
        ParsedGroup, ParsedSubrecord, compiled_schema_for_game_str,
    };

    use super::*;
    use crate::merge_sources::test_util::rec;

    fn subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn dangling(owner: &ParsedRecord, subrecord: &str, raw: u32) -> DanglingReference {
        DanglingReference::record(owner.signature.as_str(), owner.form_id, subrecord, raw)
    }

    fn first_record(items: &[ParsedItem], form_id: u32) -> Option<&ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record) if record.form_id == form_id => return Some(record),
                ParsedItem::Group(group) => {
                    if let Some(record) = first_record(&group.children, form_id) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn exact_allowlist_actions_preserve_companion_payloads() {
        let primary_schema = compiled_schema_for_game_str("fnv").unwrap();
        let grafted_schema = compiled_schema_for_game_str("fo3").unwrap();
        let mut cell = rec("CELL", 0x9000, "Cell");
        let mut xclr = 0xA000_u32.to_le_bytes().to_vec();
        xclr.extend_from_slice(&0xBEEF_u32.to_le_bytes());
        cell.subrecords.push(subrecord("XCLR", xclr));
        let mut empty_cell = rec("CELL", 0x9003, "EmptyCell");
        empty_cell
            .subrecords
            .push(subrecord("XCLR", 0xBEEF_u32.to_le_bytes().to_vec()));
        let mut navm = rec("NAVM", 0xA000, "Navmesh");
        let data_payload = (0_u8..16).collect::<Vec<_>>();
        navm.subrecords
            .push(subrecord("DATA", data_payload.clone()));
        let mut nvex = vec![1, 2, 3, 4];
        nvex.extend_from_slice(&0xBEEF_u32.to_le_bytes());
        nvex.extend_from_slice(&[5, 6, 7, 8, 9, 10]);
        nvex.extend_from_slice(&0xA000_u32.to_le_bytes());
        nvex.extend_from_slice(&[11, 12]);
        navm.subrecords.push(subrecord("NVEX", nvex.clone()));
        let mut refr = rec("REFR", 0x9001, "Reference");
        refr.subrecords.push(subrecord("XAPD", vec![1]));
        let mut missing_parent = 0xBEEF_u32.to_le_bytes().to_vec();
        missing_parent.extend_from_slice(&1.25_f32.to_le_bytes());
        refr.subrecords.push(subrecord("XAPR", missing_parent));
        let mut valid_parent = 0xA000_u32.to_le_bytes().to_vec();
        valid_parent.extend_from_slice(&2.5_f32.to_le_bytes());
        refr.subrecords
            .push(subrecord("XAPR", valid_parent.clone()));
        let mut xndp = 0xBEEF_u32.to_le_bytes().to_vec();
        xndp.extend_from_slice(&[13, 14, 15, 16]);
        refr.subrecords.push(subrecord("XNDP", xndp));
        refr.subrecords
            .push(subrecord("XLKR", 0xBEEF_u32.to_le_bytes().to_vec()));
        refr.subrecords
            .push(subrecord("XCLP", vec![21, 22, 23, 24, 25, 26, 27, 28]));
        let pending = vec![
            dangling(&cell, "XCLR", 0xBEEF),
            dangling(&empty_cell, "XCLR", 0xBEEF),
            dangling(&navm, "NVEX", 0xBEEF),
            dangling(&refr, "XAPR", 0xBEEF),
            dangling(&refr, "XNDP", 0xBEEF),
            dangling(&refr, "XLKR", 0xBEEF),
        ];
        let mut items = vec![
            ParsedItem::Record(cell),
            ParsedItem::Record(empty_cell),
            ParsedItem::Record(navm),
            ParsedItem::Record(refr),
        ];
        let stats = sanitize_dangling_references(
            &mut items,
            pending,
            Some(&primary_schema),
            Some(&grafted_schema),
            &HashSet::new(),
        );
        assert_eq!(stats.occurrences, 6);
        assert!(stats.dangling.is_empty());
        assert_eq!(stats.by_context.values().sum::<u64>(), stats.occurrences);
        assert_eq!(stats.by_action.values().sum::<u64>(), stats.occurrences);
        let cell = first_record(&items, 0x9000).unwrap();
        assert_eq!(
            cell.subrecords[1].data,
            Bytes::copy_from_slice(&0xA000_u32.to_le_bytes())
        );
        assert!(
            first_record(&items, 0x9003)
                .unwrap()
                .subrecords
                .iter()
                .all(|subrecord| subrecord.signature.as_str() != "XCLR")
        );
        let navm = first_record(&items, 0xA000).unwrap();
        let nvex_subrecord = navm
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "NVEX")
            .unwrap();
        assert_eq!(nvex_subrecord.data[0..4], nvex[0..4]);
        assert_eq!(nvex_subrecord.data[4..8], [0, 0, 0, 0]);
        assert_eq!(nvex_subrecord.data[8..], nvex[8..]);
        assert_eq!(
            navm.subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "DATA")
                .unwrap()
                .data,
            Bytes::from(data_payload)
        );
        let refr = first_record(&items, 0x9001).unwrap();
        assert!(
            refr.subrecords
                .iter()
                .any(|subrecord| subrecord.signature.as_str() == "XAPD")
        );
        assert_eq!(
            refr.subrecords
                .iter()
                .find(|subrecord| subrecord.signature.as_str() == "XAPR")
                .unwrap()
                .data,
            Bytes::from(valid_parent)
        );
        assert!(
            refr.subrecords
                .iter()
                .all(|subrecord| !matches!(subrecord.signature.as_str(), "XNDP" | "XLKR" | "XCLP"))
        );
    }

    #[test]
    fn removing_last_xapr_removes_xapd() {
        let schema = compiled_schema_for_game_str("fnv").unwrap();
        let mut refr = rec("REFR", 0x9000, "Reference");
        refr.subrecords.push(subrecord("XAPD", vec![1]));
        let mut xapr = 0xBEEF_u32.to_le_bytes().to_vec();
        xapr.extend_from_slice(&0.0_f32.to_le_bytes());
        refr.subrecords.push(subrecord("XAPR", xapr));
        let pending = vec![dangling(&refr, "XAPR", 0xBEEF)];
        let mut items = vec![ParsedItem::Record(refr)];
        let stats = sanitize_dangling_references(
            &mut items,
            pending,
            Some(&schema),
            Some(&schema),
            &HashSet::new(),
        );
        assert_eq!(stats.occurrences, 1);
        let refr = first_record(&items, 0x9000).unwrap();
        assert!(
            refr.subrecords
                .iter()
                .all(|subrecord| !matches!(subrecord.signature.as_str(), "XAPR" | "XAPD"))
        );
    }

    #[test]
    fn removing_xlkr_removes_only_its_adjacent_xclp_companion() {
        let schema = compiled_schema_for_game_str("fnv").unwrap();
        let mut refr = rec("REFR", 0x9000, "Reference");
        let removed_color = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let preserved_color = vec![11, 12, 13, 14, 15, 16, 17, 18];
        refr.subrecords
            .push(subrecord("XLKR", 0xBEEF_u32.to_le_bytes().to_vec()));
        refr.subrecords
            .push(subrecord("XCLP", removed_color.clone()));
        refr.subrecords
            .push(subrecord("XLKR", 0xA000_u32.to_le_bytes().to_vec()));
        refr.subrecords
            .push(subrecord("XCLP", preserved_color.clone()));
        let pending = vec![dangling(&refr, "XLKR", 0xBEEF)];
        let mut items = vec![ParsedItem::Record(refr)];

        let stats = sanitize_dangling_references(
            &mut items,
            pending,
            Some(&schema),
            Some(&schema),
            &HashSet::new(),
        );

        assert_eq!(stats.occurrences, 1);
        let refr = first_record(&items, 0x9000).unwrap();
        let linked = refr
            .subrecords
            .iter()
            .filter(|subrecord| matches!(subrecord.signature.as_str(), "XLKR" | "XCLP"))
            .collect::<Vec<_>>();
        assert_eq!(linked.len(), 2);
        assert_eq!(linked[0].signature.as_str(), "XLKR");
        assert_eq!(
            linked[0].data,
            Bytes::copy_from_slice(&0xA000_u32.to_le_bytes())
        );
        assert_eq!(linked[1].signature.as_str(), "XCLP");
        assert_eq!(linked[1].data, Bytes::from(preserved_color));
        assert_ne!(linked[1].data, Bytes::from(removed_color));
    }

    #[test]
    fn owner_drops_reach_fixed_point_without_pruning_groups_and_rerun_is_noop() {
        let primary_schema = compiled_schema_for_game_str("fnv").unwrap();
        let grafted_schema = compiled_schema_for_game_str("fo3").unwrap();
        let mut acre = rec("ACRE", 0x9000, "Actor");
        let mut xesp = 0xBEEF_u32.to_le_bytes().to_vec();
        xesp.extend_from_slice(&[0, 0, 0, 0]);
        acre.subrecords.push(subrecord("XESP", xesp));
        let mut info = rec("INFO", 0x9001, "");
        info.subrecords
            .push(subrecord("QSTI", 0xBEEF_u32.to_le_bytes().to_vec()));
        info.subrecords
            .push(subrecord("PNAM", 0xDEAD_u32.to_le_bytes().to_vec()));
        let mut cell = rec("CELL", 0x9002, "Cell");
        let mut xclr = 0x9000_u32.to_le_bytes().to_vec();
        xclr.extend_from_slice(&0x9001_u32.to_le_bytes());
        xclr.extend_from_slice(&0xDEAD_u32.to_le_bytes());
        cell.subrecords.push(subrecord("XCLR", xclr));
        let pending = vec![
            dangling(&acre, "XESP", 0xBEEF),
            dangling(&info, "QSTI", 0xBEEF),
            dangling(&info, "PNAM", 0xDEAD),
        ];
        let group = ParsedGroup {
            label: *b"TEST",
            group_type: 0,
            tail: Bytes::new(),
            children: vec![ParsedItem::Record(acre), ParsedItem::Record(info)],
        };
        let mut items = vec![ParsedItem::Group(group), ParsedItem::Record(cell)];
        let stats = sanitize_dangling_references(
            &mut items,
            pending,
            Some(&primary_schema),
            Some(&grafted_schema),
            &HashSet::from([0x9000, 0x9001]),
        );
        assert_eq!(stats.occurrences, 5);
        assert_eq!(stats.dropped_owner_count(), 2);
        assert_eq!(stats.by_context.values().sum::<u64>(), stats.occurrences);
        assert_eq!(stats.by_action.values().sum::<u64>(), stats.occurrences);
        assert!(matches!(&items[0], ParsedItem::Group(group) if group.children.is_empty()));
        assert_eq!(
            first_record(&items, 0x9002).unwrap().subrecords[1].data,
            Bytes::copy_from_slice(&0xDEAD_u32.to_le_bytes())
        );
        let rerun_pending = collect_references_to_dropped_owners(
            &items,
            &HashSet::from([0x9000, 0x9001]),
            Some(&primary_schema),
            Some(&grafted_schema),
            &HashSet::from([0x9000, 0x9001]),
        );
        assert!(rerun_pending.is_empty());
        let second = sanitize_dangling_references(
            &mut items,
            rerun_pending,
            Some(&primary_schema),
            Some(&grafted_schema),
            &HashSet::new(),
        );
        assert_eq!(second, SanitizationStats::default());
    }

    #[test]
    fn later_round_unsupported_context_hard_fails_without_partial_round_mutation() {
        let primary_schema = compiled_schema_for_game_str("fnv").unwrap();
        let grafted_schema = compiled_schema_for_game_str("fo3").unwrap();
        let mut acre = rec("ACRE", 0x9000, "Actor");
        let mut xesp = 0xBEEF_u32.to_le_bytes().to_vec();
        xesp.extend_from_slice(&[0, 0, 0, 0]);
        acre.subrecords.push(subrecord("XESP", xesp));
        let mut activator = rec("ACTI", 0x9001, "Activator");
        activator.subrecords.push(ParsedSubrecord {
            signature: "SCRI".into(),
            data: Bytes::copy_from_slice(&0x9000_u32.to_le_bytes()),
            semantic_type: Some("formid".to_string()),
        });
        let mut cell = rec("CELL", 0x9002, "Cell");
        cell.subrecords
            .push(subrecord("XCLR", 0x9000_u32.to_le_bytes().to_vec()));
        let pending = vec![dangling(&acre, "XESP", 0xBEEF)];
        let mut items = vec![
            ParsedItem::Record(acre),
            ParsedItem::Record(activator),
            ParsedItem::Record(cell),
        ];

        let stats = sanitize_dangling_references(
            &mut items,
            pending,
            Some(&primary_schema),
            Some(&grafted_schema),
            &HashSet::from([0x9000]),
        );

        assert_eq!(stats.occurrences, 1);
        assert_eq!(stats.dropped_owners["ACRE"], 1);
        assert_eq!(
            stats.dangling,
            ["ACTI:00009001:SCRI:00009000", "CELL:00009002:XCLR:00009000"]
        );
        assert!(first_record(&items, 0x9000).is_none());
        assert_eq!(
            first_record(&items, 0x9001).unwrap().subrecords[1].data,
            Bytes::copy_from_slice(&0x9000_u32.to_le_bytes())
        );
        assert_eq!(
            first_record(&items, 0x9002).unwrap().subrecords[1].data,
            Bytes::copy_from_slice(&0x9000_u32.to_le_bytes())
        );
    }

    #[test]
    fn unsupported_context_hard_fails_without_touching_supported_context() {
        let schema = compiled_schema_for_game_str("fnv").unwrap();
        let mut activator = rec("ACTI", 0x9000, "Activator");
        activator.subrecords.push(ParsedSubrecord {
            signature: "SCRI".into(),
            data: Bytes::copy_from_slice(&0xBEEF_u32.to_le_bytes()),
            semantic_type: Some("formid".to_string()),
        });
        let mut cell = rec("CELL", 0x9001, "Cell");
        cell.subrecords
            .push(subrecord("XCLR", 0xBEEF_u32.to_le_bytes().to_vec()));
        let pending = vec![
            dangling(&activator, "SCRI", 0xBEEF),
            dangling(&cell, "XCLR", 0xBEEF),
        ];
        let mut items = vec![ParsedItem::Record(activator), ParsedItem::Record(cell)];
        let stats = sanitize_dangling_references(
            &mut items,
            pending,
            Some(&schema),
            Some(&schema),
            &HashSet::new(),
        );
        assert_eq!(stats.occurrences, 0);
        assert_eq!(
            stats.dangling,
            ["ACTI:00009000:SCRI:0000BEEF", "CELL:00009001:XCLR:0000BEEF"]
        );
        assert_eq!(
            first_record(&items, 0x9001).unwrap().subrecords[1].data,
            Bytes::copy_from_slice(&0xBEEF_u32.to_le_bytes())
        );
    }

    #[test]
    fn raw_payload_is_invalidated_only_for_mutated_compressed_records() {
        let schema = compiled_schema_for_game_str("fnv").unwrap();
        let mut cell = rec("CELL", 0x9000, "Cell");
        cell.raw_payload = Some(Bytes::from_static(b"stale-cell-payload"));
        cell.subrecords
            .push(subrecord("XCLR", 0xBEEF_u32.to_le_bytes().to_vec()));
        let pending = vec![dangling(&cell, "XCLR", 0xBEEF)];
        let mut untouched = rec("ACTI", 0x9001, "Untouched");
        let untouched_payload = Bytes::from_static(b"untouched-payload");
        untouched.raw_payload = Some(untouched_payload.clone());
        let mut items = vec![ParsedItem::Record(cell), ParsedItem::Record(untouched)];

        let stats = sanitize_dangling_references(
            &mut items,
            pending,
            Some(&schema),
            Some(&schema),
            &HashSet::new(),
        );

        assert_eq!(stats.occurrences, 1);
        assert!(first_record(&items, 0x9000).unwrap().raw_payload.is_none());
        assert_eq!(
            first_record(&items, 0x9001).unwrap().raw_payload.as_ref(),
            Some(&untouched_payload)
        );
    }
}
