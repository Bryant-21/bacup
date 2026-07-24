use std::collections::{BTreeMap, HashMap, HashSet};

use esp_authoring_core::plugin_runtime::{ParsedItem, editor_id_from_effective_subrecords};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use super::SigCounts;

pub(crate) struct Classification {
    pub remap: FxHashMap<u32, u32>,
    pub dropped: FxHashSet<u32>,
    pub by_signature: BTreeMap<SmolStr, SigCounts>,
}

pub(crate) fn classify_grafted(
    grafted_root_items: &[ParsedItem],
    eid_index: &HashMap<(String, SmolStr), u32>,
    used_ids: &mut HashSet<u32>,
) -> Classification {
    let mut classification = Classification {
        remap: FxHashMap::default(),
        dropped: FxHashSet::default(),
        by_signature: BTreeMap::new(),
    };
    let mut next_candidate = used_ids
        .iter()
        .copied()
        .max()
        .unwrap_or(0x7ff)
        .saturating_add(1)
        .max(0x800);
    classify_items(
        grafted_root_items,
        eid_index,
        used_ids,
        &mut next_candidate,
        &mut classification,
    );
    classification
}

fn classify_items(
    items: &[ParsedItem],
    eid_index: &HashMap<(String, SmolStr), u32>,
    used_ids: &mut HashSet<u32>,
    next_candidate: &mut u32,
    classification: &mut Classification,
) {
    for item in items {
        match item {
            ParsedItem::Group(group) => classify_items(
                &group.children,
                eid_index,
                used_ids,
                next_candidate,
                classification,
            ),
            ParsedItem::Record(record) => {
                let counts = classification
                    .by_signature
                    .entry(record.signature.clone())
                    .or_default();
                let editor_id = editor_id_from_effective_subrecords(&record.subrecords);
                if !matches!(record.signature.as_str(), "CELL" | "PACK")
                    && !editor_id.is_empty()
                    && let Some(&primary_id) =
                        eid_index.get(&(editor_id.to_lowercase(), record.signature.clone()))
                {
                    classification.remap.insert(record.form_id, primary_id);
                    classification.dropped.insert(record.form_id);
                    counts.deduped += 1;
                    continue;
                }

                let output_id = if used_ids.insert(record.form_id) {
                    record.form_id
                } else {
                    while used_ids.contains(next_candidate) {
                        *next_candidate = next_candidate.saturating_add(1);
                    }
                    let allocated = *next_candidate;
                    used_ids.insert(allocated);
                    *next_candidate = next_candidate.saturating_add(1);
                    allocated
                };
                classification.remap.insert(record.form_id, output_id);
                counts.copied += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{ParsedGroup, ParsedItem};

    use super::*;
    use crate::merge_sources::test_util::rec;

    fn primary_index() -> HashMap<(String, SmolStr), u32> {
        HashMap::from([(("timescale".to_string(), "GLOB".into()), 0x1200)])
    }

    #[test]
    fn matching_nonempty_editor_id_deduplicates_case_insensitively() {
        let items = vec![ParsedItem::Record(rec("GLOB", 0x9900, "TimeScale"))];
        let mut used = HashSet::from([0x1200]);
        let result = classify_grafted(&items, &primary_index(), &mut used);
        assert_eq!(result.remap[&0x9900], 0x1200);
        assert!(result.dropped.contains(&0x9900));
    }

    #[test]
    fn free_id_is_preserved() {
        let items = vec![ParsedItem::Record(rec("WEAP", 0xA000, "10mmPistol"))];
        let mut used = HashSet::new();
        let result = classify_grafted(&items, &HashMap::new(), &mut used);
        assert_eq!(result.remap[&0xA000], 0xA000);
        assert!(used.contains(&0xA000));
        assert!(!result.dropped.contains(&0xA000));
    }

    #[test]
    fn colliding_id_is_reallocated_above_current_maximum() {
        let items = vec![ParsedItem::Record(rec("WEAP", 0xA000, "10mmPistol"))];
        let mut used = HashSet::from([0xA000]);
        let result = classify_grafted(&items, &HashMap::new(), &mut used);
        assert!(result.remap[&0xA000] > 0xA000);
        assert!(!result.dropped.contains(&0xA000));
    }

    #[test]
    fn empty_editor_id_never_deduplicates() {
        let items = vec![ParsedItem::Record(rec("REFR", 0x9900, ""))];
        let mut used = HashSet::new();
        let result = classify_grafted(&items, &HashMap::new(), &mut used);
        assert_eq!(result.remap[&0x9900], 0x9900);
        assert!(result.dropped.is_empty());
    }

    #[test]
    fn cell_editor_id_is_not_global_identity() {
        let items = vec![ParsedItem::Record(rec("CELL", 0x162A, "Wilderness"))];
        let index = HashMap::from([(("wilderness".to_string(), "CELL".into()), 0xDDCAB)]);
        let mut used = HashSet::from([0x162A, 0xDDCAB]);

        let result = classify_grafted(&items, &index, &mut used);

        assert_ne!(result.remap[&0x162A], 0xDDCAB);
        assert!(!result.dropped.contains(&0x162A));
    }

    #[test]
    fn pack_editor_id_is_not_cross_game_identity() {
        let items = vec![ParsedItem::Record(rec("PACK", 0x162A, "FollowPlayer"))];
        let index = HashMap::from([(("followplayer".to_string(), "PACK".into()), 0xDDCAB)]);
        let mut used = HashSet::from([0x162A, 0xDDCAB]);

        let result = classify_grafted(&items, &index, &mut used);

        assert_ne!(result.remap[&0x162A], 0xDDCAB);
        assert!(!result.dropped.contains(&0x162A));
        assert_eq!(result.by_signature["PACK"].deduped, 0);
        assert_eq!(result.by_signature["PACK"].copied, 1);
    }

    #[test]
    fn nested_records_are_classified() {
        let items = vec![ParsedItem::Group(ParsedGroup {
            label: *b"GLOB",
            group_type: 0,
            tail: Bytes::new(),
            children: vec![ParsedItem::Record(rec("GLOB", 0x9900, "TimeScale"))],
        })];
        let mut used = HashSet::from([0x1200]);
        let result = classify_grafted(&items, &primary_index(), &mut used);
        assert_eq!(result.remap[&0x9900], 0x1200);
    }
}
