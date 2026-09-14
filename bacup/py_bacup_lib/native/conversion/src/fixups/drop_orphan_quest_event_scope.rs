//! Fixup: drop `ENAM` from converted QUSTs that no Story Manager node selects.
//!
//! A quest with `ENAM` (Quest Event) is event-scoped: FO4 starts it only through
//! the Story Manager and rejects `Start()`/`SetStage()` elsewhere with
//! `attempting to start event scoped quest outside of story manager`. FO76 can
//! name the start keyword on the quest itself in the FO76-only `QSSK`. FO4 drops
//! `QSSK` but keeps `ENAM`, so the quest can never start, and nothing is logged
//! (e.g. `EN05_MQ_Officer`, `0010DBEA`, `QSSK = 0x00182161`).
//!
//! Any output QUST with `ENAM` that no `SMQN` node's `NNAM` selects (output
//! plugin or target masters) loses its `ENAM`, so a script `Start()` works. No
//! generic Story Manager node is synthesized because its conditions can't be
//! derived. Exact source contracts such as `NPE_DQ01_BetterTomorrow` are adapted
//! earlier and keep `ENAM`.
//!
//! FO4 never auto-starts a quest with `ENAM`, so dropping it alone would start
//! `StartGameEnabled` quests at game load; the flag is cleared with it.
//! Idempotent: quests without `ENAM`, or selected by a node, stay byte-identical.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fo76_fo4::qust_uses_player_connect_autostart_fallback;

pub struct DropOrphanQuestEventScopeFixup;

impl Fixup for DropOrphanQuestEventScopeFixup {
    fn name(&self) -> &'static str {
        "drop_orphan_quest_event_scope"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let smqn_sig =
            SigCode::from_str("SMQN").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let qust_sig =
            SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let mut node_quests: FxHashSet<FormKey> = FxHashSet::default();
        let node_fks = session
            .form_keys_of_sig(smqn_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in node_fks {
            let Ok(node) = session.record_decoded(&fk, target_schema, mapper.interner) else {
                continue;
            };
            node_quests.extend(smqn_quest_form_keys(&node));
        }
        for &handle_id in &config.target_master_handle_ids {
            let Ok(master_fks) =
                session.form_keys_of_sig_in_handle(handle_id, smqn_sig, mapper.interner)
            else {
                continue;
            };
            for fk in master_fks {
                let Ok(node) = session.record_decoded_in_handle(
                    handle_id,
                    &fk,
                    target_schema,
                    mapper.interner,
                ) else {
                    continue;
                };
                node_quests.extend(smqn_quest_form_keys(&node));
            }
        }

        let fks = session
            .form_keys_of_sig(qust_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut changed_records = Vec::new();
        for fk in fks {
            if node_quests.contains(&fk) {
                continue;
            }
            let Ok(mut record) = session.record_decoded(&fk, target_schema, mapper.interner) else {
                continue;
            };
            if drop_quest_event_scope(&mut record, mapper.interner) {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "drop_orphan_quest_event_scope replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

/// Quests an `SMQN` selects, via its `NNAM` quest links.
fn smqn_quest_form_keys(node: &Record) -> Vec<FormKey> {
    node.fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"NNAM")
        .filter_map(|entry| match &entry.value {
            crate::record::FieldValue::FormKey(form_key) => Some(*form_key),
            _ => None,
        })
        .filter(|form_key| form_key.local != 0)
        .collect()
}

/// `QUST.DNAM.flags` bit 0x1 — "Start Game Enabled".
const QUST_DNAM_START_GAME_ENABLED: u64 = 0x0001;

pub(crate) fn drop_quest_event_scope(record: &mut Record, interner: &StringInterner) -> bool {
    if record.sig.0 != *b"QUST" {
        return false;
    }
    let before = record.fields.len();
    record.fields.retain(|entry| entry.sig.0 != *b"ENAM");
    if before == record.fields.len() {
        return false;
    }
    // Clear StartGameEnabled only once the event scope is gone; leaving it set
    // turns an unstartable quest into an always-running one. Autostart-allowlist
    // quests keep the bit: FO4 has no producer for their event, so it is their
    // only start path.
    if !qust_uses_player_connect_autostart_fallback(interner, record) {
        clear_start_game_enabled(record, interner);
    }
    true
}

/// Clears the auto-start flag on the quest-level `DNAM`. Stage-level `DNAM` is a
/// reward `FormKey`, so it is skipped by construction.
///
/// `DNAM` arrives as raw `Bytes` or a decoded `Struct` depending on the record's
/// path; the pipeline decode is `Bytes`, so both shapes must be handled (as in
/// `story_manager.rs::clear_qust_autostart_value`).
fn clear_start_game_enabled(record: &mut Record, interner: &StringInterner) -> bool {
    let mut cleared = false;
    for entry in record.fields.iter_mut() {
        if entry.sig.0 != *b"DNAM" {
            continue;
        }
        if let FieldValue::Bytes(bytes) = &mut entry.value {
            if bytes.len() >= 2 {
                let old = u16::from_le_bytes([bytes[0], bytes[1]]);
                let new = old & !(QUST_DNAM_START_GAME_ENABLED as u16);
                if old != new {
                    bytes[0..2].copy_from_slice(&new.to_le_bytes());
                    cleared = true;
                }
            }
            continue;
        }
        let FieldValue::Struct(members) = &mut entry.value else {
            continue;
        };
        for (name, value) in members.iter_mut() {
            let is_flags = interner
                .resolve(*name)
                .is_some_and(|name| name.replace('_', "").eq_ignore_ascii_case("flags"));
            if !is_flags {
                continue;
            }
            match value {
                FieldValue::Uint(bits) if *bits & QUST_DNAM_START_GAME_ENABLED != 0 => {
                    *bits &= !QUST_DNAM_START_GAME_ENABLED;
                    cleared = true;
                }
                FieldValue::Int(bits) if *bits & (QUST_DNAM_START_GAME_ENABLED as i64) != 0 => {
                    *bits &= !(QUST_DNAM_START_GAME_ENABLED as i64);
                    cleared = true;
                }
                _ => {}
            }
        }
    }
    cleared
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};

    /// Quest-level `DNAM`, as the schema decodes it: a struct whose `flags`
    /// member is the 16-bit bitfield.
    fn dnam(interner: &StringInterner, flags: u64) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Struct(vec![
                (interner.intern("flags"), FieldValue::Uint(flags)),
                (interner.intern("priority"), FieldValue::Uint(50)),
            ]),
        }
    }

    fn quest_flags(record: &Record, interner: &StringInterner) -> Option<u64> {
        record.fields.iter().find_map(|entry| {
            if entry.sig.0 != *b"DNAM" {
                return None;
            }
            let FieldValue::Struct(members) = &entry.value else {
                return None;
            };
            members.iter().find_map(|(name, value)| {
                (interner.resolve(*name) == Some("flags")).then(|| match value {
                    FieldValue::Uint(bits) => *bits,
                    _ => u64::MAX,
                })
            })
        })
    }

    fn quest(interner: &StringInterner, local: u32, event_scoped: bool) -> Record {
        let mut fields: SmallVec<[FieldEntry; 8]> = SmallVec::new();
        if event_scoped {
            fields.push(FieldEntry {
                sig: SubrecordSig(*b"ENAM"),
                value: FieldValue::Uint(1_414_546_259),
            });
        }
        fields.push(FieldEntry {
            sig: SubrecordSig(*b"FLTR"),
            value: FieldValue::Bytes(SmallVec::new()),
        });
        Record {
            sig: SigCode::from_str("QUST").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    /// `EN05_MQ_Officer` shape: event-scoped, no node anywhere. The `ENAM` is
    /// what refuses `Start()`, so it has to go or the quest is unreachable.
    #[test]
    fn drops_event_scope_from_a_quest_no_node_selects() {
        let interner = StringInterner::new();
        let mut record = quest(&interner, 0x0010_DBEA, true);

        assert!(drop_quest_event_scope(&mut record, &interner));
        assert!(record.fields.iter().all(|entry| entry.sig.0 != *b"ENAM"));
        assert!(
            record.fields.iter().any(|entry| entry.sig.0 == *b"FLTR"),
            "unrelated fields must survive"
        );
    }

    /// `Storm_MQ01_Breadcrumb_Radio` shape: event-scoped, its `SMQN` node was
    /// never emitted, and it is on the autostart allowlist. Dropping `ENAM` is
    /// required (it is what refuses `Start()`), but clearing start-game-enabled
    /// on top of it leaves the station's quest stopped forever — the Pip-Boy
    /// never lists it and the broadcast scene never begins.
    #[test]
    fn keeps_autostart_on_an_allowlisted_quest_it_de_scopes() {
        let interner = StringInterner::new();
        let mut record = quest(&interner, 0x0069_9466, true);
        record.eid = Some(interner.intern("Storm_MQ01_Breadcrumb_Radio"));
        record.fields.push(dnam(&interner, 0x8511)); // StartGameEnabled|StartsEnabled|RunOnce|WarnOnAliasFill|HasDialogueData

        assert!(drop_quest_event_scope(&mut record, &interner));
        assert!(record.fields.iter().all(|entry| entry.sig.0 != *b"ENAM"));
        assert_eq!(quest_flags(&record, &interner), Some(0x8511));
    }

    /// The exemption must be scoped to the allowlist: an identical quest that is
    /// not on it still loses the bit, or an unstartable quest becomes always-on.
    #[test]
    fn still_clears_autostart_on_a_quest_outside_the_allowlist() {
        let interner = StringInterner::new();
        let mut record = quest(&interner, 0x0010_DBEA, true);
        record.eid = Some(interner.intern("EN05_MQ_Officer"));
        record.fields.push(dnam(&interner, 0x8511));

        assert!(drop_quest_event_scope(&mut record, &interner));
        assert_eq!(quest_flags(&record, &interner), Some(0x8510));
    }

    #[test]
    fn de_scopes_en05_course_quests_without_story_manager_producers() {
        let interner = StringInterner::new();
        let node_quests: FxHashSet<FormKey> = FxHashSet::default();

        for local in [0x0008_D23B, 0x0009_C824, 0x0008_C881] {
            let mut record = quest(&interner, local, true);

            assert!(!node_quests.contains(&record.form_key));
            assert!(drop_quest_event_scope(&mut record, &interner));
            assert!(record.fields.iter().all(|entry| entry.sig.0 != *b"ENAM"));
            assert!(record.fields.iter().any(|entry| entry.sig.0 == *b"FLTR"));
        }
    }

    #[test]
    fn leaves_a_quest_without_event_scope_untouched() {
        let interner = StringInterner::new();
        let mut record = quest(&interner, 0x0010_DBEA, false);

        assert!(!drop_quest_event_scope(&mut record, &interner));
    }

    /// `NPE_DQ01_BetterTomorrow` (`006FD072`) shape: event-scoped AND
    /// `StartGameEnabled`. FO76 let the event scope suppress the auto-start.
    /// Dropping `ENAM` without clearing the flag made it start at game load, and
    /// its priority-50 Greeting DIAL hijacked every human NPC's dialogue.
    #[test]
    fn clears_start_game_enabled_when_it_drops_the_event_scope() {
        let interner = StringInterner::new();
        let mut record = quest(&interner, 0x006F_D072, true);
        // StartGameEnabled | RunOnce | WarnOnAliasFillFailure | HasDialogueData
        record
            .fields
            .push(dnam(&interner, 0x0001 | 0x0100 | 0x0400 | 0x8000));

        assert!(drop_quest_event_scope(&mut record, &interner));
        let flags = quest_flags(&record, &interner).expect("DNAM flags must survive");
        assert_eq!(
            flags & 0x0001,
            0,
            "StartGameEnabled must be cleared or the quest auto-starts"
        );
        assert_eq!(
            flags,
            0x0100 | 0x0400 | 0x8000,
            "every other flag must be preserved exactly"
        );
    }

    /// The real pipeline hands `DNAM` over as raw `Bytes`, not a decoded struct.
    /// Handling only the struct shape shipped a silent no-op — this pins the
    /// shape that actually occurs in production.
    #[test]
    fn clears_start_game_enabled_when_dnam_arrives_as_raw_bytes() {
        let interner = StringInterner::new();
        let mut record = quest(&interner, 0x006F_D072, true);
        // flags u16 LE = StartGameEnabled|RunOnce|WarnOnAliasFill|HasDialogueData,
        // then priority 50, then the rest of the DNAM payload.
        let flags: u16 = 0x0001 | 0x0100 | 0x0400 | 0x8000;
        let mut payload: SmallVec<[u8; 32]> = SmallVec::new();
        payload.extend_from_slice(&flags.to_le_bytes());
        payload.extend_from_slice(&[50, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0]);
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(payload),
        });

        assert!(drop_quest_event_scope(&mut record, &interner));
        let entry = record
            .fields
            .iter()
            .find(|e| e.sig.0 == *b"DNAM")
            .expect("DNAM must survive");
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("DNAM should still be raw bytes");
        };
        let got = u16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(got & 0x0001, 0, "StartGameEnabled must be cleared");
        assert_eq!(got, 0x0100 | 0x0400 | 0x8000, "other flags preserved");
        assert_eq!(bytes[2], 50, "priority byte must be untouched");
        assert_eq!(bytes.len(), 14, "payload length must not change");
    }

    /// The flag is only unsafe *because* the event scope is gone. A quest a node
    /// selects keeps both, so its Story Manager start path is unchanged.
    #[test]
    fn leaves_start_game_enabled_alone_when_there_is_no_event_scope_to_drop() {
        let interner = StringInterner::new();
        let mut record = quest(&interner, 0x006F_D072, false);
        record.fields.push(dnam(&interner, 0x0001 | 0x8000));

        assert!(!drop_quest_event_scope(&mut record, &interner));
        assert_eq!(quest_flags(&record, &interner), Some(0x0001 | 0x8000));
    }

    /// Stage-level `DNAM` is a reward FormKey, not the quest flag struct.
    #[test]
    fn stage_level_dnam_reward_links_are_untouched() {
        let interner = StringInterner::new();
        let mut record = quest(&interner, 0x006F_D072, true);
        let reward = FormKey {
            local: 0x0002_C6,
            plugin: interner.intern("SeventySix.esm"),
        };
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::FormKey(reward),
        });

        assert!(drop_quest_event_scope(&mut record, &interner));
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.value == FieldValue::FormKey(reward)),
            "stage reward DNAM must survive untouched"
        );
    }

    /// The selection happens before this helper is reached, so prove the node
    /// index actually claims the quest it links.
    #[test]
    fn smqn_quest_links_are_collected_from_nnam() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let quest_fk = FormKey {
            local: 0x0008_C87F,
            plugin,
        };
        let mut fields: SmallVec<[FieldEntry; 8]> = SmallVec::new();
        fields.push(FieldEntry {
            sig: SubrecordSig(*b"NNAM"),
            value: FieldValue::FormKey(quest_fk),
        });
        let node = Record {
            sig: SigCode::from_str("SMQN").unwrap(),
            form_key: FormKey {
                local: 0x0018_2075,
                plugin,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        };

        assert_eq!(smqn_quest_form_keys(&node), vec![quest_fk]);
    }
}
