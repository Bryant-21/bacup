//! Clear ACBS template flags from NPCs whose template resolves to nothing.
//!
//! ACBS template flags tell the engine to read those fields through `TPLT`. With
//! the `Traits` bit (0x0001) set the RACE comes from the template, so a template
//! that yields no actor leaves no race and `QueuedCharacter::BackgroundClone ->
//! QueueModels -> CreateAnimationGraphManager` null-derefs while the cell streams
//! in. There is no log line, and records and meshes audit clean.
//!
//! FO3/FNV always pair the flags with a `TPLT` (847 point at an `LVLN`).
//! Conversion breaks the pair when the template is dropped (the reference is
//! nulled upstream) or when a template `LVLN` loses every entry. Both cases are
//! cleared here, so the NPC uses its own `RNAM`, `AIDT`, `CNTO`: a leveled
//! variant becomes one concrete actor instead of a load-time CTD.
//!
//! An unresolved `TPLT` is left alone. This runs after `PruneOrphanedRecords`,
//! so every own-plugin template is in `root_items` and an unresolved target is a
//! legitimate master template.

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{ParsedItem, ParsedRecord, WriteEffect};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::PluginSession;

/// Byte offset of the u16 template-flag field inside FO4's 20-byte `ACBS`.
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;

/// What a template target offers the NPC that points at it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TemplateTarget {
    /// A concrete `NPC_`, or an `LVLN` that still has entries to pick from.
    Usable,
    /// An `LVLN` whose entries were all dropped — it can never yield an actor.
    EmptyLeveledList,
}

pub struct ClearOrphanedNpcTemplateFlagsFixup;

impl Fixup for ClearOrphanedNpcTemplateFlagsFixup {
    fn name(&self) -> &'static str {
        "clear_orphaned_npc_template_flags"
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
        let items = &mut session.target_slot_mut().parsed.root_items;
        let mut targets = FxHashMap::default();
        index_template_targets(items, &mut targets);

        let mut changed_form_ids = SmallVec::<[u32; 4]>::new();
        let changed = clear_orphaned_flags_from_items(items, &targets, &mut changed_form_ids);
        if changed > 0 {
            session.record_effect(WriteEffect::RecordContents {
                form_ids: changed_form_ids,
            });
        }
        let mut report = FixupReport::empty();
        report.records_changed = changed;
        Ok(report)
    }
}

/// Record every own-plugin record an `NPC_` may legally template off, noting
/// which leveled lists have been emptied out.
fn index_template_targets(items: &[ParsedItem], targets: &mut FxHashMap<u32, TemplateTarget>) {
    for item in items {
        match item {
            ParsedItem::Group(group) => index_template_targets(&group.children, targets),
            ParsedItem::Record(record) => match record.signature.as_str() {
                "NPC_" => {
                    targets.insert(record.form_id, TemplateTarget::Usable);
                }
                "LVLN" => {
                    let has_entries = record
                        .subrecords
                        .iter()
                        .any(|sub| sub.signature.as_str() == "LVLO");
                    targets.insert(
                        record.form_id,
                        if has_entries {
                            TemplateTarget::Usable
                        } else {
                            TemplateTarget::EmptyLeveledList
                        },
                    );
                }
                _ => {}
            },
        }
    }
}

fn clear_orphaned_flags_from_items(
    items: &mut [ParsedItem],
    targets: &FxHashMap<u32, TemplateTarget>,
    changed_form_ids: &mut SmallVec<[u32; 4]>,
) -> u32 {
    let mut changed = 0;
    for item in items {
        match item {
            ParsedItem::Group(group) => {
                changed +=
                    clear_orphaned_flags_from_items(&mut group.children, targets, changed_form_ids);
            }
            ParsedItem::Record(record) if record.signature.as_str() == "NPC_" => {
                if clear_orphaned_flags_from_npc(record, targets) {
                    changed_form_ids.push(record.form_id);
                    changed += 1;
                }
            }
            _ => {}
        }
    }
    changed
}

fn template_reference(record: &ParsedRecord) -> Option<u32> {
    record
        .subrecords
        .iter()
        .find(|sub| sub.signature.as_str() == "TPLT")
        .filter(|sub| sub.data.len() >= 4)
        .map(|sub| u32::from_le_bytes([sub.data[0], sub.data[1], sub.data[2], sub.data[3]]))
}

/// Whether this NPC's template can still hand it the fields its flags delegate.
fn template_resolves(record: &ParsedRecord, targets: &FxHashMap<u32, TemplateTarget>) -> bool {
    match template_reference(record) {
        // No TPLT: nothing to inherit from.
        None => false,
        // Not one of our own records, so it is a master reference we must trust.
        Some(target) => targets.get(&target) != Some(&TemplateTarget::EmptyLeveledList),
    }
}

fn clear_orphaned_flags_from_npc(
    record: &mut ParsedRecord,
    targets: &FxHashMap<u32, TemplateTarget>,
) -> bool {
    if template_resolves(record, targets) {
        return false;
    }
    for subrecord in &mut record.subrecords {
        if subrecord.signature.as_str() != "ACBS"
            || subrecord.data.len() < ACBS_TEMPLATE_FLAGS_OFFSET + 2
        {
            continue;
        }
        let flags = u16::from_le_bytes([
            subrecord.data[ACBS_TEMPLATE_FLAGS_OFFSET],
            subrecord.data[ACBS_TEMPLATE_FLAGS_OFFSET + 1],
        ]);
        if flags == 0 {
            return false;
        }
        let mut data = subrecord.data.to_vec();
        data[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
            .copy_from_slice(&0u16.to_le_bytes());
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

    /// `LvlEnclave4OfficerVar`'s shipped flags: all ten template bits, Traits
    /// included, with its `TPLT` (LVLN `VarEnclave4Officer`) gone.
    const ALL_TEMPLATE_FLAGS: u16 = 0x03FF;
    const TRAITS: u16 = 0x0001;

    fn sub(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    /// 20-byte FO4 `ACBS` carrying `template_flags`; other fields stay zero
    /// except a non-zero disposition, so a wrong offset would be visible.
    fn acbs(template_flags: u16) -> Vec<u8> {
        let mut data = vec![0u8; 20];
        data[12..14].copy_from_slice(&35u16.to_le_bytes());
        data[ACBS_TEMPLATE_FLAGS_OFFSET..ACBS_TEMPLATE_FLAGS_OFFSET + 2]
            .copy_from_slice(&template_flags.to_le_bytes());
        data
    }

    fn npc(form_id: u32, template_flags: u16, tplt: Option<u32>) -> ParsedItem {
        let mut subrecords = vec![sub("ACBS", acbs(template_flags))];
        if let Some(target) = tplt {
            subrecords.push(sub("TPLT", target.to_le_bytes().to_vec()));
        }
        subrecords.push(sub("RNAM", 0x0001_3746u32.to_le_bytes().to_vec()));
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("NPC_"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        })
    }

    fn lvln(form_id: u32, entries: usize) -> ParsedItem {
        let mut subrecords = vec![sub("LLCT", vec![entries as u8])];
        for _ in 0..entries {
            subrecords.push(sub("LVLO", vec![0u8; 12]));
        }
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("LVLN"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        })
    }

    fn run(items: &mut Vec<ParsedItem>) -> (u32, SmallVec<[u32; 4]>) {
        let mut targets = FxHashMap::default();
        index_template_targets(items, &mut targets);
        let mut changed_ids = SmallVec::<[u32; 4]>::new();
        let changed = clear_orphaned_flags_from_items(items, &targets, &mut changed_ids);
        (changed, changed_ids)
    }

    fn template_flags_of(item: &ParsedItem) -> u16 {
        let ParsedItem::Record(record) = item else {
            panic!("expected record");
        };
        let data = &record
            .subrecords
            .iter()
            .find(|sub| sub.signature.as_str() == "ACBS")
            .expect("ACBS")
            .data;
        u16::from_le_bytes([
            data[ACBS_TEMPLATE_FLAGS_OFFSET],
            data[ACBS_TEMPLATE_FLAGS_OFFSET + 1],
        ])
    }

    fn disposition(item: &ParsedItem) -> u16 {
        let ParsedItem::Record(record) = item else {
            panic!("expected record");
        };
        let data = &record
            .subrecords
            .iter()
            .find(|sub| sub.signature.as_str() == "ACBS")
            .expect("ACBS")
            .data;
        u16::from_le_bytes([data[12], data[13]])
    }

    #[test]
    fn clears_flags_when_the_template_reference_is_gone() {
        let mut items = vec![npc(0x0707_D42A, ALL_TEMPLATE_FLAGS, None)];
        let (changed, changed_ids) = run(&mut items);

        assert_eq!(changed, 1);
        assert_eq!(template_flags_of(&items[0]), 0);
        assert_eq!(changed_ids.as_slice(), &[0x0707_D42A]);
        let ParsedItem::Record(record) = &items[0] else {
            unreachable!();
        };
        assert!(record.raw_payload.is_none());
    }

    #[test]
    fn clears_flags_when_the_template_is_an_emptied_leveled_list() {
        // 383 of the 440 LVLNs in the shipped FNV output have zero entries.
        let mut items = vec![
            npc(0x0707_D42A, ALL_TEMPLATE_FLAGS, Some(0x0707_D429)),
            lvln(0x0707_D429, 0),
        ];
        let (changed, _) = run(&mut items);

        assert_eq!(changed, 1);
        assert_eq!(template_flags_of(&items[0]), 0);
    }

    #[test]
    fn keeps_flags_when_the_leveled_template_still_has_entries() {
        let mut items = vec![
            npc(0x0707_D42A, ALL_TEMPLATE_FLAGS, Some(0x0707_D429)),
            lvln(0x0707_D429, 3),
        ];
        let (changed, changed_ids) = run(&mut items);

        assert_eq!(changed, 0);
        assert_eq!(template_flags_of(&items[0]), ALL_TEMPLATE_FLAGS);
        assert!(changed_ids.is_empty());
    }

    #[test]
    fn keeps_flags_when_the_template_is_a_concrete_npc() {
        // FFEnclaveCamp29TraineeCM -> FFEnclaveCamp24TraineeAM.
        let mut items = vec![
            npc(0x070C_191A, 0x03BE, Some(0x070B_FE4A)),
            npc(0x070B_FE4A, 0, None),
        ];
        let (changed, _) = run(&mut items);

        assert_eq!(changed, 0);
        assert_eq!(template_flags_of(&items[0]), 0x03BE);
    }

    #[test]
    fn keeps_flags_when_the_template_lives_in_a_master() {
        // A vanilla FO4 template actor is not in root_items and must be trusted.
        let mut items = vec![npc(0x0700_0001, TRAITS, Some(0x0001_3746))];
        let (changed, _) = run(&mut items);

        assert_eq!(changed, 0);
        assert_eq!(template_flags_of(&items[0]), TRAITS);
    }

    #[test]
    fn leaves_the_rest_of_acbs_alone() {
        let mut items = vec![npc(0x0707_D42A, TRAITS, None)];
        run(&mut items);
        assert_eq!(disposition(&items[0]), 35);
    }

    #[test]
    fn ignores_untemplated_npcs_without_a_tplt() {
        let mut items = vec![npc(0x0800_0001, 0, None)];
        let (changed, _) = run(&mut items);

        assert_eq!(changed, 0);
        let ParsedItem::Record(record) = &items[0] else {
            unreachable!();
        };
        assert!(record.raw_payload.is_some());
    }

    #[test]
    fn ignores_records_without_acbs() {
        let mut items = vec![ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("NPC_"),
            form_id: 0x0800_0002,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("RNAM", 0x0001_3746u32.to_le_bytes().to_vec())],
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        })];
        let (changed, _) = run(&mut items);

        assert_eq!(changed, 0);
    }

    #[test]
    fn descends_into_top_level_groups() {
        let mut items = vec![ParsedItem::Group(ParsedGroup {
            label: *b"NPC_",
            group_type: 0,
            tail: Bytes::new(),
            children: vec![npc(0x0707_D42A, ALL_TEMPLATE_FLAGS, None)],
        })];
        let (changed, _) = run(&mut items);

        assert_eq!(changed, 1);
        let ParsedItem::Group(group) = &items[0] else {
            unreachable!();
        };
        assert_eq!(template_flags_of(&group.children[0]), 0);
    }

    /// The index must see records nested in their type groups, not just at the
    /// root, or every grouped LVLN reads as a master reference.
    #[test]
    fn indexes_targets_across_groups() {
        let mut items = vec![
            ParsedItem::Group(ParsedGroup {
                label: *b"NPC_",
                group_type: 0,
                tail: Bytes::new(),
                children: vec![npc(0x0707_D42A, ALL_TEMPLATE_FLAGS, Some(0x0707_D429))],
            }),
            ParsedItem::Group(ParsedGroup {
                label: *b"LVLN",
                group_type: 0,
                tail: Bytes::new(),
                children: vec![lvln(0x0707_D429, 0)],
            }),
        ];
        let (changed, _) = run(&mut items);

        assert_eq!(changed, 1);
        let ParsedItem::Group(group) = &items[0] else {
            unreachable!();
        };
        assert_eq!(template_flags_of(&group.children[0]), 0);
    }

    /// A chain `A -> B` where B is the orphan: clearing B's flags makes B
    /// self-contained, so A's template resolves again without touching A.
    #[test]
    fn repairs_a_chain_by_fixing_only_its_dead_end() {
        let mut items = vec![
            npc(0x070C_191A, 0x03BE, Some(0x0707_D42A)),
            npc(0x0707_D42A, ALL_TEMPLATE_FLAGS, None),
        ];
        let (changed, _) = run(&mut items);

        assert_eq!(changed, 1);
        assert_eq!(template_flags_of(&items[0]), 0x03BE);
        assert_eq!(template_flags_of(&items[1]), 0);
    }
}
