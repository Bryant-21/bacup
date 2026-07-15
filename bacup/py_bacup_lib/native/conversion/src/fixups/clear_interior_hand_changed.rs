//! Clear the FO76-only/CK-generated `Hand Changed` CELL DATA bit from interiors.

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{ParsedItem, ParsedRecord, WriteEffect};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::{PluginSession, open_session};

const WORLD_CHILD_GROUP: i32 = 1;
const FLAG_INTERIOR: u16 = 0x0001;
const FLAG_HAND_CHANGED: u16 = 0x0040;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InteriorHandChangedReport {
    pub records_changed: u32,
}

pub struct ClearInteriorHandChangedFixup;

impl Fixup for ClearInteriorHandChangedFixup {
    fn name(&self) -> &'static str {
        "clear_interior_hand_changed"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let changed = clear_interior_hand_changed_in_session(session);
        let mut report = FixupReport::empty();
        report.records_changed = changed;
        Ok(report)
    }
}

pub fn clear_interior_hand_changed_flags(
    target_handle_id: u64,
) -> Result<InteriorHandChangedReport, FixupError> {
    let mut session =
        open_session(target_handle_id, None).map_err(|e| FixupError::HandleError(e.to_string()))?;
    let records_changed = clear_interior_hand_changed_in_session(&mut session);
    session.flush_pending_effects();
    Ok(InteriorHandChangedReport { records_changed })
}

pub(crate) fn clear_interior_hand_changed_in_session(session: &mut PluginSession) -> u32 {
    let mut changed_form_ids = SmallVec::<[u32; 4]>::new();
    let changed = clear_interior_hand_changed_from_items(
        &mut session.target_slot_mut().parsed.root_items,
        false,
        &mut changed_form_ids,
    );
    if changed > 0 {
        session.record_effect(WriteEffect::RecordContents {
            form_ids: changed_form_ids,
        });
    }
    changed
}

fn clear_interior_hand_changed_from_items(
    items: &mut [ParsedItem],
    under_world_children: bool,
    changed_form_ids: &mut SmallVec<[u32; 4]>,
) -> u32 {
    let mut changed = 0;
    for item in items {
        match item {
            ParsedItem::Group(group) => {
                let child_under_world =
                    under_world_children || group.group_type == WORLD_CHILD_GROUP;
                changed += clear_interior_hand_changed_from_items(
                    &mut group.children,
                    child_under_world,
                    changed_form_ids,
                );
            }
            ParsedItem::Record(record)
                if record.signature.as_str() == "CELL" && !under_world_children =>
            {
                if clear_interior_hand_changed_from_cell(record) {
                    changed_form_ids.push(record.form_id);
                    changed += 1;
                }
            }
            _ => {}
        }
    }
    changed
}

fn clear_interior_hand_changed_from_cell(record: &mut ParsedRecord) -> bool {
    for subrecord in &mut record.subrecords {
        if subrecord.signature.as_str() != "DATA" || subrecord.data.len() < 2 {
            continue;
        }
        let flags = u16::from_le_bytes([subrecord.data[0], subrecord.data[1]]);
        if flags & FLAG_INTERIOR == 0 || flags & FLAG_HAND_CHANGED == 0 {
            return false;
        }
        let mut data = subrecord.data.to_vec();
        let updated = flags & !FLAG_HAND_CHANGED;
        data[0..2].copy_from_slice(&updated.to_le_bytes());
        subrecord.data = Bytes::from(data);
        record.raw_payload = None;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use esp_authoring_core::plugin_runtime::{ParsedGroup, ParsedSubrecord};
    use smol_str::SmolStr;

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn cell(form_id: u32, flags: u16) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("CELL"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("DATA", flags.to_le_bytes().to_vec())],
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        })
    }

    fn group(group_type: i32, label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            label,
            group_type,
            tail: Bytes::new(),
            children,
        })
    }

    fn cell_flags(item: &ParsedItem) -> u16 {
        let ParsedItem::Record(record) = item else {
            panic!("expected record");
        };
        let data = &record
            .subrecords
            .iter()
            .find(|sub| sub.signature.as_str() == "DATA")
            .expect("DATA")
            .data;
        u16::from_le_bytes([data[0], data[1]])
    }

    #[test]
    fn clears_hand_changed_from_interior_cells() {
        let mut items = vec![cell(0x0700_1000, FLAG_INTERIOR | FLAG_HAND_CHANGED)];
        let mut changed_ids = SmallVec::<[u32; 4]>::new();

        assert_eq!(
            clear_interior_hand_changed_from_items(&mut items, false, &mut changed_ids),
            1
        );
        assert_eq!(cell_flags(&items[0]), FLAG_INTERIOR);
        assert_eq!(changed_ids.as_slice(), &[0x0700_1000]);
        let ParsedItem::Record(record) = &items[0] else {
            unreachable!();
        };
        assert!(record.raw_payload.is_none());
    }

    #[test]
    fn leaves_non_hand_changed_interior_cells_unchanged() {
        let mut items = vec![cell(0x0700_1000, FLAG_INTERIOR)];
        let mut changed_ids = SmallVec::<[u32; 4]>::new();

        assert_eq!(
            clear_interior_hand_changed_from_items(&mut items, false, &mut changed_ids),
            0
        );
        assert_eq!(cell_flags(&items[0]), FLAG_INTERIOR);
        assert!(changed_ids.is_empty());
    }

    #[test]
    fn leaves_world_child_cells_unchanged() {
        let world_label = 0x0000_1234_u32.to_le_bytes();
        let mut items = vec![group(
            WORLD_CHILD_GROUP,
            world_label,
            vec![cell(0x0700_2000, FLAG_INTERIOR | FLAG_HAND_CHANGED)],
        )];
        let mut changed_ids = SmallVec::<[u32; 4]>::new();

        assert_eq!(
            clear_interior_hand_changed_from_items(&mut items, false, &mut changed_ids),
            0
        );
        let ParsedItem::Group(group) = &items[0] else {
            unreachable!();
        };
        assert_eq!(
            cell_flags(&group.children[0]),
            FLAG_INTERIOR | FLAG_HAND_CHANGED
        );
    }
}
