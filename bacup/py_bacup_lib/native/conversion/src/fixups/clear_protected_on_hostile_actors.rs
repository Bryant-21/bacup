//! Clear the FO4 `Protected` ACBS bit from converted actors that attack on sight.
//!
//! FO4 and FO76 share the ACBS bit map, so `Protected` (0x800) copies verbatim.
//! In FO4 it means only the player may kill the actor: `kah` (`KillAllHostile`),
//! companion kills and NPC-vs-NPC kills all no-op. FO76 sets it on hostile
//! creatures for its own event damage rules. Vanilla FO4 reserves it for
//! friendlies, so this clears it on `VeryAggressive`/`Frenzied` actors and leaves
//! `Unaggressive`/`Aggressive` ones alone.
//!
//! Aggression comes from the record's own `AIDT`, even when the ACBS AIData
//! template bit is set. FO76 template chains often end in an `LVLN` (e.g.
//! `SSE_Beezlebub`), which yields no aggression for exactly these records.

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{ParsedItem, ParsedRecord, WriteEffect};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::PluginSession;

/// FO4 NPC_ `ACBS` flag bit 11 — "only the player may kill this actor".
const ACBS_FLAG_PROTECTED: u32 = 0x0000_0800;

/// `Aggression` is the first byte of `AIDT`: 0 Unaggressive, 1 Aggressive,
/// 2 Very Aggressive, 3 Frenzied.
const AIDT_AGGRESSION_OFFSET: usize = 0;

/// Lowest aggression that attacks anyone who is not an ally — the player
/// included.
const AGGRESSION_VERY_AGGRESSIVE: u8 = 2;

pub struct ClearProtectedOnHostileActorsFixup;

impl Fixup for ClearProtectedOnHostileActorsFixup {
    fn name(&self) -> &'static str {
        "clear_protected_on_hostile_actors"
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
        let mut changed_form_ids = SmallVec::<[u32; 4]>::new();
        let changed = clear_protected_from_items(
            &mut session.target_slot_mut().parsed.root_items,
            &mut changed_form_ids,
        );
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

fn clear_protected_from_items(
    items: &mut [ParsedItem],
    changed_form_ids: &mut SmallVec<[u32; 4]>,
) -> u32 {
    let mut changed = 0;
    for item in items {
        match item {
            ParsedItem::Group(group) => {
                changed += clear_protected_from_items(&mut group.children, changed_form_ids);
            }
            ParsedItem::Record(record) if record.signature.as_str() == "NPC_" => {
                if clear_protected_from_npc(record) {
                    changed_form_ids.push(record.form_id);
                    changed += 1;
                }
            }
            _ => {}
        }
    }
    changed
}

fn subrecord_data<'a>(record: &'a ParsedRecord, sig: &str) -> Option<&'a Bytes> {
    record
        .subrecords
        .iter()
        .find(|sub| sub.signature.as_str() == sig)
        .map(|sub| &sub.data)
}

fn attacks_on_sight(record: &ParsedRecord) -> bool {
    subrecord_data(record, "AIDT")
        .and_then(|data| data.get(AIDT_AGGRESSION_OFFSET).copied())
        .is_some_and(|aggression| aggression >= AGGRESSION_VERY_AGGRESSIVE)
}

fn clear_protected_from_npc(record: &mut ParsedRecord) -> bool {
    if !attacks_on_sight(record) {
        return false;
    }
    for subrecord in &mut record.subrecords {
        if subrecord.signature.as_str() != "ACBS" || subrecord.data.len() < 4 {
            continue;
        }
        let flags = u32::from_le_bytes([
            subrecord.data[0],
            subrecord.data[1],
            subrecord.data[2],
            subrecord.data[3],
        ]);
        if flags & ACBS_FLAG_PROTECTED == 0 {
            return false;
        }
        let mut data = subrecord.data.to_vec();
        data[0..4].copy_from_slice(&(flags & !ACBS_FLAG_PROTECTED).to_le_bytes());
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

    /// 20-byte FO4 `ACBS` carrying `flags`; the remaining fields stay zero.
    fn acbs(flags: u32) -> Vec<u8> {
        let mut data = vec![0u8; 20];
        data[0..4].copy_from_slice(&flags.to_le_bytes());
        data
    }

    /// 24-byte FO4 `AIDT` whose leading `Aggression` byte is `aggression`.
    fn aidt(aggression: u8) -> Vec<u8> {
        let mut data = vec![0u8; 24];
        data[AIDT_AGGRESSION_OFFSET] = aggression;
        data
    }

    fn npc(form_id: u32, flags: u32, aggression: u8) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("NPC_"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("ACBS", acbs(flags)), sub("AIDT", aidt(aggression))],
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        })
    }

    fn npc_flags(item: &ParsedItem) -> u32 {
        let ParsedItem::Record(record) = item else {
            panic!("expected record");
        };
        let data = subrecord_data(record, "ACBS").expect("ACBS");
        u32::from_le_bytes([data[0], data[1], data[2], data[3]])
    }

    const AUTO_CALC_STATS: u32 = 0x0000_0010;

    #[test]
    fn clears_protected_from_very_aggressive_actor() {
        // SSE_Beezlebub's exact shape: AutoCalcStats + Protected, VeryAggressive.
        let mut items = vec![npc(
            0x0879_AAA6,
            AUTO_CALC_STATS | ACBS_FLAG_PROTECTED,
            AGGRESSION_VERY_AGGRESSIVE,
        )];
        let mut changed_ids = SmallVec::<[u32; 4]>::new();

        assert_eq!(clear_protected_from_items(&mut items, &mut changed_ids), 1);
        assert_eq!(npc_flags(&items[0]), AUTO_CALC_STATS);
        assert_eq!(changed_ids.as_slice(), &[0x0879_AAA6]);
        let ParsedItem::Record(record) = &items[0] else {
            unreachable!();
        };
        assert!(record.raw_payload.is_none());

        let mut items = vec![ParsedItem::Record(ParsedRecord {
            signature: SmolStr::new("NPC_"),
            form_id: 0x0800_0005,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![sub("ACBS", acbs(ACBS_FLAG_PROTECTED))],
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        })];
        let mut changed_ids = SmallVec::<[u32; 4]>::new();

        assert_eq!(clear_protected_from_items(&mut items, &mut changed_ids), 0);
        assert_eq!(npc_flags(&items[0]), ACBS_FLAG_PROTECTED);

        let mut items = vec![ParsedItem::Group(ParsedGroup {
            label: *b"NPC_",
            group_type: 0,
            tail: Bytes::new(),
            children: vec![npc(
                0x0879_AAA6,
                ACBS_FLAG_PROTECTED,
                AGGRESSION_VERY_AGGRESSIVE,
            )],
        })];
        let mut changed_ids = SmallVec::<[u32; 4]>::new();

        assert_eq!(clear_protected_from_items(&mut items, &mut changed_ids), 1);
        let ParsedItem::Group(group) = &items[0] else {
            unreachable!();
        };
        assert_eq!(npc_flags(&group.children[0]), 0);
    }

    #[test]
    fn protected_is_cleared_only_from_protected_actors_hostile_to_the_player() {
        for (name, flags, aggression, expected_flags) in [
            ("frenzied", ACBS_FLAG_PROTECTED, 3, 0),
            (
                "unaggressive settler",
                ACBS_FLAG_PROTECTED,
                0,
                ACBS_FLAG_PROTECTED,
            ),
            (
                "aggressive quest npc",
                ACBS_FLAG_PROTECTED,
                1,
                ACBS_FLAG_PROTECTED,
            ),
            (
                "unprotected hostile",
                AUTO_CALC_STATS,
                AGGRESSION_VERY_AGGRESSIVE,
                AUTO_CALC_STATS,
            ),
        ] {
            let mut items = vec![npc(0x0800_0001, flags, aggression)];
            let mut changed_ids = SmallVec::<[u32; 4]>::new();

            let changed = clear_protected_from_items(&mut items, &mut changed_ids);
            assert_eq!(changed, u32::from(flags != expected_flags), "{name}");
            assert_eq!(npc_flags(&items[0]), expected_flags, "{name}");
            assert_eq!(changed_ids.len(), changed as usize, "{name}");
        }
    }
}
