//! Restore the local klaxon-sound edge for the Vault 79 reactor-door encounter.
//!
//! FO76's placed klaxon activators survive conversion and remain activation
//! children of the bound klaxon dummy. Their linked sound markers also survive,
//! but the target-side two-state adapter does not consume FO76's
//! `LinkKlaxonSound` link. A short `LinkCustom01` chain from the bound klaxon
//! dummy gives the recovered controller an exact, FO4-native way to enable both
//! sound markers without changing the shared activator adapter.

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{ParsedItem, ParsedRecord, ParsedSubrecord, WriteEffect};
use smallvec::smallvec;
use smol_str::SmolStr;

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::PluginSession;

const VAULT79_MAIN_CELL: u32 = 0x401116;
const REACTOR_KLAXON_DUMMY: u32 = 0x539BAE;
const REACTOR_KLAXON_SOUND_REFS: [u32; 2] = [0x539B8C, 0x539B8F];
const DEFAULT_DUMMY_BASE: u32 = 0x03B695;
const KLAXON_SOUND_BASE: u32 = 0x193B93;
const LINK_CUSTOM_01: u32 = 0x05D5E6;
const CELL_CHILD_GROUP: i32 = 6;
const CELL_PERSISTENT_GROUP: i32 = 8;

fn read_form_id(data: &[u8]) -> Option<u32> {
    data.get(..4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte slice")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkedRefOutcome {
    Added,
    AlreadyPresent,
    Conflict,
}

fn existing_linked_ref_outcome(
    record: &ParsedRecord,
    keyword: u32,
    target: u32,
) -> Option<LinkedRefOutcome> {
    let mut existing = record.subrecords.iter().filter(|subrecord| {
        subrecord.signature.as_str() == "XLKR"
            && read_form_id(subrecord.data.as_ref()) == Some(keyword)
    });
    let linked_ref = existing.next()?;
    let exact_target =
        linked_ref.data.len() == 8 && read_form_id(&linked_ref.data[4..]) == Some(target);
    if exact_target && existing.next().is_none() {
        Some(LinkedRefOutcome::AlreadyPresent)
    } else {
        Some(LinkedRefOutcome::Conflict)
    }
}

fn add_linked_ref(record: &mut ParsedRecord, keyword: u32, target: u32) -> LinkedRefOutcome {
    if let Some(outcome) = existing_linked_ref_outcome(record, keyword, target) {
        return outcome;
    }

    let insert_at = record
        .subrecords
        .iter()
        .position(|subrecord| subrecord.signature.as_str() == "DATA")
        .unwrap_or(record.subrecords.len());
    let mut xlkr = keyword.to_le_bytes().to_vec();
    xlkr.extend_from_slice(&target.to_le_bytes());
    record.subrecords.splice(
        insert_at..insert_at,
        [ParsedSubrecord {
            signature: SmolStr::new("XLKR"),
            data: Bytes::from(xlkr),
            semantic_type: None,
        }],
    );
    record.raw_payload = None;
    LinkedRefOutcome::Added
}

fn has_exact_form_id(record: &ParsedRecord, signature: &str, expected: u32) -> bool {
    let mut matching = record
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.signature.as_str() == signature);
    let Some(subrecord) = matching.next() else {
        return false;
    };
    subrecord.data.len() == 4
        && read_form_id(subrecord.data.as_ref()) == Some(expected)
        && matching.next().is_none()
}

fn is_exact_bridge_record(record: &ParsedRecord, expected_base: u32, expected_flags: u32) -> bool {
    record.signature.as_str() == "REFR"
        && record.flags == expected_flags
        && has_exact_form_id(record, "NAME", expected_base)
}

fn has_exact_persistent_identity(items: &[ParsedItem], placed: u32, parent_cell: u32) -> bool {
    fn walk(
        items: &[ParsedItem],
        placed: u32,
        parent_cell: u32,
        current_cell: Option<u32>,
        persistent_section: Option<u32>,
        cell_count: &mut usize,
        placed_count: &mut usize,
        exact_count: &mut usize,
    ) {
        for item in items {
            match item {
                ParsedItem::Group(group) => {
                    let label = u32::from_le_bytes(group.label);
                    let next_cell = if group.group_type == CELL_CHILD_GROUP {
                        Some(label)
                    } else {
                        current_cell
                    };
                    let next_persistent = if group.group_type == CELL_CHILD_GROUP {
                        None
                    } else if group.group_type == CELL_PERSISTENT_GROUP {
                        Some(label)
                    } else {
                        persistent_section
                    };
                    walk(
                        &group.children,
                        placed,
                        parent_cell,
                        next_cell,
                        next_persistent,
                        cell_count,
                        placed_count,
                        exact_count,
                    );
                }
                ParsedItem::Record(record) => {
                    if record.form_id == parent_cell && record.signature.as_str() == "CELL" {
                        *cell_count += 1;
                    }
                    if record.form_id == placed {
                        *placed_count += 1;
                        if record.signature.as_str() == "REFR"
                            && current_cell == Some(parent_cell)
                            && persistent_section == Some(parent_cell)
                        {
                            *exact_count += 1;
                        }
                    }
                }
            }
        }
    }

    let mut cell_count = 0;
    let mut placed_count = 0;
    let mut exact_count = 0;
    walk(
        items,
        placed,
        parent_cell,
        None,
        None,
        &mut cell_count,
        &mut placed_count,
        &mut exact_count,
    );
    cell_count == 1 && placed_count == 1 && exact_count == 1
}

pub fn repair_vault79_reactor_door_topology(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    _config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let own_load_index = session.target_masters().len();
    if own_load_index > u8::MAX as usize {
        return Ok(report);
    }

    let Some(fallout4_index) = session
        .target_masters()
        .iter()
        .position(|master| master.eq_ignore_ascii_case("Fallout4.esm"))
    else {
        return Ok(report);
    };
    let keyword = ((fallout4_index as u32) << 24) | LINK_CUSTOM_01;
    let own = (own_load_index as u32) << 24;
    let parent_cell = own | VAULT79_MAIN_CELL;
    let dummy = own | REACTOR_KLAXON_DUMMY;

    if session.record(dummy).is_err() {
        return Ok(report);
    }

    let fallout4 = (fallout4_index as u32) << 24;
    let sound_base = own | KLAXON_SOUND_BASE;
    let sound_base_is_exact = session
        .record(sound_base)
        .is_ok_and(|record| record.signature.as_str() == "SOUN");
    if !sound_base_is_exact {
        report.warnings.push(mapper.interner.intern(
            "repair_vault79_reactor_door_topology:193B93:sound_base_missing_or_wrong_signature",
        ));
        return Ok(report);
    }

    for (local, base, flags) in [
        (REACTOR_KLAXON_DUMMY, fallout4 | DEFAULT_DUMMY_BASE, 0x400),
        (REACTOR_KLAXON_SOUND_REFS[0], sound_base, 0xC00),
        (REACTOR_KLAXON_SOUND_REFS[1], sound_base, 0xC00),
    ] {
        let raw = own | local;
        if !has_exact_persistent_identity(
            &session.target_slot().parsed.root_items,
            raw,
            parent_cell,
        ) {
            report.warnings.push(mapper.interner.intern(&format!(
                "repair_vault79_reactor_door_topology:{local:06X}:persistent_topology_mismatch"
            )));
            return Ok(report);
        }
        let exact_record = session
            .record(raw)
            .is_ok_and(|record| is_exact_bridge_record(record, base, flags));
        if !exact_record {
            report.warnings.push(mapper.interner.intern(&format!(
                "repair_vault79_reactor_door_topology:{local:06X}:signature_base_or_flags_mismatch"
            )));
            return Ok(report);
        }
    }

    let edges = [
        (REACTOR_KLAXON_DUMMY, REACTOR_KLAXON_SOUND_REFS[0]),
        (REACTOR_KLAXON_SOUND_REFS[0], REACTOR_KLAXON_SOUND_REFS[1]),
    ];
    for (owner_id, target_id) in edges {
        let record = session
            .record(own | owner_id)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if existing_linked_ref_outcome(record, keyword, own | target_id)
            == Some(LinkedRefOutcome::Conflict)
        {
            report.warnings.push(mapper.interner.intern(&format!(
                "repair_vault79_reactor_door_topology:{owner_id:06X}:link_custom_01_conflict"
            )));
            return Ok(report);
        }
    }

    for (owner_id, target_id) in edges {
        let raw_form_id = own | owner_id;
        let record = session
            .record_mut(raw_form_id)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        match add_linked_ref(record, keyword, own | target_id) {
            LinkedRefOutcome::Added => {
                session.record_effect(WriteEffect::RecordContents {
                    form_ids: smallvec![raw_form_id],
                });
                report.records_changed = report.records_changed.saturating_add(1);
            }
            LinkedRefOutcome::AlreadyPresent => {}
            LinkedRefOutcome::Conflict => unreachable!("link conflicts were preflighted"),
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use esp_authoring_core::plugin_runtime::ParsedGroup;

    fn subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(signature),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn sound_marker() -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("REFR"),
            form_id: 0x0753_9B8C,
            flags: 0xC00,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                subrecord("NAME", 0x0719_3B93u32.to_le_bytes().to_vec()),
                subrecord("DATA", vec![0; 24]),
            ],
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        }
    }

    fn record(signature: &str, form_id: u32, flags: u32, base: Option<u32>) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new(signature),
            form_id,
            flags,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: base
                .map(|value| subrecord("NAME", value.to_le_bytes().to_vec()))
                .into_iter()
                .collect(),
            raw_payload: None,
            parse_error: None,
        }
    }

    fn group(group_type: i32, label: u32, children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label: label.to_le_bytes(),
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    fn persistent_fixture(placed: ParsedRecord) -> Vec<ParsedItem> {
        let parent_cell = 0x0740_1116;
        vec![
            ParsedItem::Record(record("CELL", parent_cell, 0, None)),
            group(
                CELL_CHILD_GROUP,
                parent_cell,
                vec![group(
                    CELL_PERSISTENT_GROUP,
                    parent_cell,
                    vec![ParsedItem::Record(placed)],
                )],
            ),
        ]
    }

    #[test]
    fn validates_exact_bridge_record_identity() {
        let marker = sound_marker();
        assert!(is_exact_bridge_record(&marker, 0x0719_3B93, 0xC00));

        let mut wrong_base = marker.clone();
        wrong_base.subrecords[0].data = Bytes::copy_from_slice(&0x0719_3B94u32.to_le_bytes());
        assert!(!is_exact_bridge_record(&wrong_base, 0x0719_3B93, 0xC00));

        let mut wrong_flags = marker.clone();
        wrong_flags.flags = 0x400;
        assert!(!is_exact_bridge_record(&wrong_flags, 0x0719_3B93, 0xC00));

        let mut wrong_signature = marker;
        wrong_signature.signature = SmolStr::new("ACHR");
        assert!(!is_exact_bridge_record(
            &wrong_signature,
            0x0719_3B93,
            0xC00
        ));
    }

    #[test]
    fn requires_one_persistent_refr_in_the_exact_parent_cell() {
        let parent_cell = 0x0740_1116;
        let placed = 0x0753_9B8C;
        let exact = persistent_fixture(sound_marker());
        assert!(has_exact_persistent_identity(&exact, placed, parent_cell));

        let temporary = vec![
            ParsedItem::Record(record("CELL", parent_cell, 0, None)),
            group(
                CELL_CHILD_GROUP,
                parent_cell,
                vec![group(
                    9,
                    parent_cell,
                    vec![ParsedItem::Record(sound_marker())],
                )],
            ),
        ];
        assert!(!has_exact_persistent_identity(
            &temporary,
            placed,
            parent_cell
        ));

        let mut duplicate = exact;
        duplicate.push(ParsedItem::Record(sound_marker()));
        assert!(!has_exact_persistent_identity(
            &duplicate,
            placed,
            parent_cell
        ));
    }

    #[test]
    fn adds_one_linked_ref_before_placed_data() {
        let mut record = sound_marker();
        let keyword: u32 = 0x0005_D5E6;
        let target: u32 = 0x0753_9B8F;

        assert_eq!(
            add_linked_ref(&mut record, keyword, target),
            LinkedRefOutcome::Added
        );
        assert_eq!(
            record
                .subrecords
                .iter()
                .map(|subrecord| subrecord.signature.as_str())
                .collect::<Vec<_>>(),
            ["NAME", "XLKR", "DATA"]
        );
        let xlkr = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "XLKR")
            .expect("linked ref");
        assert_eq!(read_form_id(xlkr.data.as_ref()), Some(keyword));
        assert_eq!(read_form_id(&xlkr.data[4..]), Some(target));
        assert!(record.raw_payload.is_none());
    }

    #[test]
    fn does_not_duplicate_the_link_custom_01_edge() {
        let mut record = sound_marker();
        let keyword = 0x0005_D5E6;

        let target = 0x0753_9B8F;
        assert_eq!(
            add_linked_ref(&mut record, keyword, target),
            LinkedRefOutcome::Added
        );
        assert_eq!(
            add_linked_ref(&mut record, keyword, target),
            LinkedRefOutcome::AlreadyPresent
        );
        assert_eq!(
            record
                .subrecords
                .iter()
                .filter(|subrecord| subrecord.signature.as_str() == "XLKR")
                .count(),
            1
        );
    }

    #[test]
    fn does_not_overwrite_a_conflicting_link_custom_01_edge() {
        let mut record = sound_marker();
        let keyword = 0x0005_D5E6;
        let existing_target = 0x0753_9B90;

        assert_eq!(
            add_linked_ref(&mut record, keyword, existing_target),
            LinkedRefOutcome::Added
        );
        assert_eq!(
            add_linked_ref(&mut record, keyword, 0x0753_9B8F),
            LinkedRefOutcome::Conflict
        );
        let xlkr = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "XLKR")
            .expect("linked ref");
        assert_eq!(read_form_id(&xlkr.data[4..]), Some(existing_target));
    }

    #[test]
    fn rejects_duplicate_or_malformed_link_custom_01_edges() {
        let mut record = sound_marker();
        let keyword: u32 = 0x0005_D5E6;
        let target: u32 = 0x0753_9B8F;
        let mut linked_ref = keyword.to_le_bytes().to_vec();
        linked_ref.extend_from_slice(&target.to_le_bytes());
        record
            .subrecords
            .push(subrecord("XLKR", linked_ref.clone()));
        record.subrecords.push(subrecord("XLKR", linked_ref));

        assert_eq!(
            add_linked_ref(&mut record, keyword, target),
            LinkedRefOutcome::Conflict
        );

        let mut malformed = sound_marker();
        malformed
            .subrecords
            .push(subrecord("XLKR", keyword.to_le_bytes().to_vec()));
        assert_eq!(
            add_linked_ref(&mut malformed, keyword, target),
            LinkedRefOutcome::Conflict
        );
    }
}
