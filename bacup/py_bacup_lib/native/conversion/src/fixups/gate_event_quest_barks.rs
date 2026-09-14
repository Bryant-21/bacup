//! Fixup: gate converted bark topics on their owning quest actually running.
//!
//! FO76 scopes event-quest barks with `GetIsAliasRef(<alias>) == 0` or
//! `GetStage(<quest>) < N`. Both read 0 from a dormant quest (unfilled alias,
//! unstarted stage), so both pass when the quest is not running. FO4 has no
//! equivalent scoping, so `GetIsVoiceType` is the only surviving discriminator
//! and every actor with that voice type gets the lines map-wide. Example:
//! `TW003` ("Activity: Manhunt", `0010E200`) topic `00522EDB` defers via `GNAM`
//! to shared info `00522EDC` (`GetIsAliasRef(13) == 0 AND
//! (GetIsVoiceType(CrSuperMutant01) OR ...)`), so every super mutant shouts
//! "Freedom!".
//!
//! INFOs under a bark topic (`DIAL.SNAM` outside the player/scene subtypes)
//! whose `DIAL.QNAM` quest is in the output plugin and not `StartGameEnabled` get
//! `GetQuestRunning(<quest>) == 1` prepended. Appended, it would join the
//! trailing `GetIsVoiceType` OR group and loosen the gate. Master-owned topics
//! are left alone, and nothing is dropped, so the barks return if the quest ever
//! runs. Idempotent: an INFO that already has the row stays byte-identical.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, effective_subrecords_for_record,
};

/// `GetQuestRunning`, Parameter #1 = QUST FormID (`wbDefinitionsFO4.pas`,
/// `Index: 56; Paramtype1: ptQuest`). The one condition in this family that reads
/// false from a dormant quest, which is why it is the row worth adding.
const GET_QUEST_RUNNING_FUNCTION_ID: u16 = 56;

const CTDA_LEN: usize = 32;
/// Operator `Equal to` with no OR flag, so the row ANDs with everything after it.
const CTDA_OPERATOR_EQUAL_TO_AND: u8 = 0x00;
const CTDA_RUN_ON_SUBJECT: u32 = 0;
/// FO76 and FO4 both write `-1` in the trailing parameter slot.
const CTDA_UNUSED_PARAMETER: i32 = -1;

const QUST_START_GAME_ENABLED: u16 = 0x0001;
const TOPIC_CHILD_GROUP: i32 = 7;

/// `ParsedRecord::form_id` and a topic child group's `label` carry the load
/// index in the high byte; `FormKey::local` never does, so topic and INFO joins
/// key on the masked object id. `QNAM` stays unmasked: it uses the same
/// `(load_index << 24) | local` encoding as a QUST's `form_id`, so quest
/// identity compares exactly and can't collide with a master's object id.
const OBJECT_ID_MASK: u32 = 0x00FF_FFFF;

/// `DIAL.SNAM` subtypes that are *not* spontaneous barks and so are left alone.
///
/// `SCEN` topics only play from a running scene, which already carries the
/// scoping. `CUST` and `GREE` are player-driven — gating them would silence
/// dialogue that the player reaches deliberately, a wider change than this fixup
/// is scoped to make.
const PLAYER_DRIVEN_TOPIC_SUBTYPES: &[[u8; 4]] = &[*b"SCEN", *b"CUST", *b"GREE"];

pub struct GateEventQuestBarksFixup;

impl Fixup for GateEventQuestBarksFixup {
    fn name(&self) -> &'static str {
        "gate_event_quest_barks"
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

        let (owner_quest_by_info_local, stats) =
            collect_ungated_bark_info_owners(&session.target_slot().parsed.root_items);
        // Always reported, including the all-zero case: a bare `changed=0` cannot
        // distinguish "nothing to gate" from a join that silently matched nothing,
        // which is exactly how the first cut of this pass failed.
        report.diagnostics.push(mapper.interner.intern(&format!(
            "gate_event_quest_barks: quests={} non_autostart={} bark_topics={} \
             gated_topics={} infos_selected={}",
            stats.quests,
            stats.non_autostart_quests,
            stats.bark_topics,
            stats.gated_topics,
            owner_quest_by_info_local.len(),
        )));
        if owner_quest_by_info_local.is_empty() {
            return Ok(report);
        }

        let info_sig =
            SigCode::from_str("INFO").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let fks = session
            .form_keys_of_sig(info_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut changed_records = Vec::new();
        for fk in fks {
            let Some(owner_quest) = owner_quest_by_info_local.get(&fk.local).copied() else {
                continue;
            };
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(_) => continue,
            };
            if prepend_quest_running_condition(&mut record, owner_quest) {
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
                "gate_event_quest_barks replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

/// Join counts, reported so a `changed=0` run says which stage matched nothing.
#[derive(Default)]
struct BarkSelectionStats {
    quests: usize,
    non_autostart_quests: usize,
    bark_topics: usize,
    gated_topics: usize,
}

/// Map every INFO that sits under an ungated bark topic to its owning quest's
/// encoded FormID.
///
/// A topic qualifies when its `SNAM` is a bark subtype and its `QNAM` names an
/// output-plugin QUST whose `DNAM` lacks `StartGameEnabled`. Master quests are
/// absent from `root_items` and never qualify, so vanilla FO4 bark quests stay
/// untouched.
fn collect_ungated_bark_info_owners(
    items: &[ParsedItem],
) -> (FxHashMap<u32, u32>, BarkSelectionStats) {
    fn walk(
        items: &[ParsedItem],
        autostart_quests: &mut FxHashSet<u32>,
        output_quests: &mut FxHashSet<u32>,
        bark_topic_owners: &mut FxHashMap<u32, u32>,
        info_topics: &mut Vec<(u32, u32)>,
    ) {
        for item in items {
            match item {
                ParsedItem::Record(record) => {
                    if record.signature.eq_ignore_ascii_case("QUST") {
                        output_quests.insert(record.form_id);
                        if let Some(flags) = subrecord_u16(record, "DNAM")
                            && flags & QUST_START_GAME_ENABLED != 0
                        {
                            autostart_quests.insert(record.form_id);
                        }
                    } else if record.signature.eq_ignore_ascii_case("DIAL")
                        && is_bark_topic(record)
                        && let Some(owner_quest) =
                            subrecord_u32(record, "QNAM").filter(|owner_quest| *owner_quest != 0)
                    {
                        bark_topic_owners.insert(record.form_id & OBJECT_ID_MASK, owner_quest);
                    }
                }
                ParsedItem::Group(group) => {
                    if group.group_type == TOPIC_CHILD_GROUP {
                        let topic = u32::from_le_bytes(group.label) & OBJECT_ID_MASK;
                        for child in &group.children {
                            if let ParsedItem::Record(record) = child
                                && record.signature.eq_ignore_ascii_case("INFO")
                            {
                                info_topics.push((record.form_id & OBJECT_ID_MASK, topic));
                            }
                        }
                    }
                    walk(
                        &group.children,
                        autostart_quests,
                        output_quests,
                        bark_topic_owners,
                        info_topics,
                    );
                }
            }
        }
    }

    let mut autostart_quests = FxHashSet::default();
    let mut output_quests = FxHashSet::default();
    let mut bark_topic_owners = FxHashMap::default();
    let mut info_topics = Vec::new();
    walk(
        items,
        &mut autostart_quests,
        &mut output_quests,
        &mut bark_topic_owners,
        &mut info_topics,
    );

    // Only an output-plugin quest whose DNAM we can see, and only one that will
    // not start itself at game load.
    let bark_topics = bark_topic_owners.len();
    let gated_topics: FxHashMap<u32, u32> = bark_topic_owners
        .into_iter()
        .filter(|(_, owner_quest)| {
            output_quests.contains(owner_quest) && !autostart_quests.contains(owner_quest)
        })
        .collect();

    let stats = BarkSelectionStats {
        quests: output_quests.len(),
        non_autostart_quests: output_quests.len().saturating_sub(autostart_quests.len()),
        bark_topics,
        gated_topics: gated_topics.len(),
    };

    let map = info_topics
        .into_iter()
        .filter_map(|(info, topic)| {
            gated_topics
                .get(&topic)
                .copied()
                .map(|owner_quest| (info, owner_quest))
        })
        .collect();
    (map, stats)
}

/// True when a `DIAL`'s `SNAM` subtype is one FO4 plays spontaneously.
///
/// A topic with no `SNAM` is treated as player-driven and skipped: without a
/// subtype there is nothing to justify re-conditioning it.
fn is_bark_topic(record: &esp_authoring_core::plugin_runtime::ParsedRecord) -> bool {
    effective_subrecords_for_record(record)
        .iter()
        .find(|subrecord| subrecord.signature.eq_ignore_ascii_case("SNAM"))
        .and_then(|subrecord| {
            <[u8; 4]>::try_from(&subrecord.data[..4.min(subrecord.data.len())]).ok()
        })
        .is_some_and(|snam| !PLAYER_DRIVEN_TOPIC_SUBTYPES.contains(&snam))
}

/// `effective_subrecords_for_record` hands back an owned collection, so each
/// reader copies its value out rather than borrowing into that temporary.
fn subrecord_u16(record: &ParsedRecord, sig: &str) -> Option<u16> {
    effective_subrecords_for_record(record)
        .iter()
        .find(|subrecord| subrecord.signature.eq_ignore_ascii_case(sig))
        .and_then(|subrecord| {
            (subrecord.data.len() >= 2)
                .then(|| u16::from_le_bytes(subrecord.data[..2].try_into().unwrap()))
        })
}

fn subrecord_u32(record: &ParsedRecord, sig: &str) -> Option<u32> {
    effective_subrecords_for_record(record)
        .iter()
        .find(|subrecord| subrecord.signature.eq_ignore_ascii_case(sig))
        .and_then(|subrecord| {
            (subrecord.data.len() >= 4)
                .then(|| u32::from_le_bytes(subrecord.data[..4].try_into().unwrap()))
        })
}

/// Build `GetQuestRunning(<quest>) == 1`, ANDed with whatever follows it.
fn quest_running_condition(encoded_quest: u32) -> FieldEntry {
    let mut bytes = vec![0u8; CTDA_LEN];
    bytes[0] = CTDA_OPERATOR_EQUAL_TO_AND;
    bytes[4..8].copy_from_slice(&1.0f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&GET_QUEST_RUNNING_FUNCTION_ID.to_le_bytes());
    bytes[12..16].copy_from_slice(&encoded_quest.to_le_bytes());
    bytes[20..24].copy_from_slice(&CTDA_RUN_ON_SUBJECT.to_le_bytes());
    bytes[28..32].copy_from_slice(&CTDA_UNUSED_PARAMETER.to_le_bytes());
    FieldEntry {
        sig: SubrecordSig(*b"CTDA"),
        value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
    }
}

/// True when this record already gates on the given quest running, so a second
/// pass leaves it byte-identical.
fn already_gated(record: &Record, encoded_quest: u32) -> bool {
    record.fields.iter().any(|entry| {
        if entry.sig.0 != *b"CTDA" {
            return false;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            return false;
        };
        bytes.len() >= 16
            && u16::from_le_bytes(bytes[8..10].try_into().unwrap()) == GET_QUEST_RUNNING_FUNCTION_ID
            && u32::from_le_bytes(bytes[12..16].try_into().unwrap()) == encoded_quest
    })
}

/// Where a leading `CTDA` belongs in an FO4 `INFO`.
///
/// Ahead of any existing condition when there is one, otherwise ahead of the
/// response block (`TRDA`/`NAM1`) that conditions precede. Falling back to the
/// end of the record keeps a shape we do not recognise from being reordered.
fn condition_insert_position(record: &Record) -> usize {
    const RESPONSE_ANCHORS: &[[u8; 4]] = &[*b"TRDA", *b"NAM1", *b"NAM0"];
    record
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"CTDA")
        .or_else(|| {
            record
                .fields
                .iter()
                .position(|entry| RESPONSE_ANCHORS.contains(&entry.sig.0))
        })
        .unwrap_or(record.fields.len())
}

/// Prepend `GetQuestRunning(<quest>) == 1` to `record`'s condition block.
///
/// Returns `true` when the record was modified.
fn prepend_quest_running_condition(record: &mut Record, encoded_quest: u32) -> bool {
    if encoded_quest == 0 || already_gated(record, encoded_quest) {
        return false;
    }
    let at = condition_insert_position(record);
    record
        .fields
        .insert(at, quest_running_condition(encoded_quest));
    // A stale condition count crashes FO4's condition evaluation, so keep CITC
    // in lockstep whenever the record carries one.
    if record.fields.iter().any(|entry| entry.sig.0 == *b"CITC") {
        record.sync_condition_count();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FormKey;
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;

    fn field(sig: &[u8; 4], bytes: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn ctda(function_id: u16, parameter_1: u32, operator: u8) -> FieldEntry {
        let mut bytes = vec![0u8; CTDA_LEN];
        bytes[0] = operator;
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        field(b"CTDA", bytes)
    }

    fn info(fields: Vec<FieldEntry>) -> Record {
        let interner = StringInterner::new();
        Record {
            sig: SigCode::from_str("INFO").unwrap(),
            form_key: FormKey {
                local: 0x52_2EDE,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    /// The TW003 shape: no conditions of its own, everything deferred to a
    /// shared info through `GNAM`.
    #[test]
    fn gates_a_conditionless_bark_info() {
        let mut record = info(vec![
            field(b"ENAM", vec![0, 0, 0, 0]),
            field(b"GNAM", 0x0052_2EDCu32.to_le_bytes().to_vec()),
            field(b"TRDA", vec![0; 12]),
            field(b"NAM1", vec![0, 0, 0, 0]),
        ]);
        assert!(prepend_quest_running_condition(&mut record, 0x0810_E200));

        let ctda_at = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"CTDA")
            .expect("condition inserted");
        let trda_at = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"TRDA")
            .unwrap();
        assert!(
            ctda_at < trda_at,
            "condition must precede the response block"
        );

        let FieldValue::Bytes(bytes) = &record.fields[ctda_at].value else {
            panic!("expected raw condition bytes");
        };
        assert_eq!(bytes[0] & 0x01, 0, "must AND, never OR");
        assert_eq!(
            u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
            GET_QUEST_RUNNING_FUNCTION_ID
        );
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x0810_E200
        );
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1.0);
    }

    /// The shared-info shape: the trailing `GetIsVoiceType` rows carry the OR
    /// flag, so the new row has to land ahead of them or it joins their group.
    #[test]
    fn prepends_ahead_of_an_or_chained_voice_type_group() {
        let mut record = info(vec![
            field(b"ENAM", vec![0, 0, 0, 0]),
            ctda(566, 13, 0x00),          // GetIsAliasRef(13) == 0
            ctda(426, 0x0806_D7B7, 0x01), // OR GetIsVoiceType(CrSuperMutant01)
            ctda(426, 0x000C_7203, 0x01), // OR GetIsVoiceType(CrSuperMutant02)
            field(b"NAM0", vec![0]),
        ]);
        assert!(prepend_quest_running_condition(&mut record, 0x0810_E200));

        let first_ctda = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"CTDA")
            .unwrap();
        let FieldValue::Bytes(bytes) = &record.fields[first_ctda].value else {
            panic!("expected raw condition bytes");
        };
        assert_eq!(
            u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
            GET_QUEST_RUNNING_FUNCTION_ID,
            "the new row must be first, not appended into the OR group"
        );
        // The original rows survive untouched, in order.
        let functions: Vec<u16> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"CTDA")
            .map(|entry| {
                let FieldValue::Bytes(bytes) = &entry.value else {
                    unreachable!()
                };
                u16::from_le_bytes(bytes[8..10].try_into().unwrap())
            })
            .collect();
        assert_eq!(
            functions,
            vec![GET_QUEST_RUNNING_FUNCTION_ID, 566, 426, 426]
        );
    }

    #[test]
    fn is_idempotent() {
        let mut record = info(vec![
            field(b"ENAM", vec![0, 0, 0, 0]),
            field(b"NAM1", vec![0, 0, 0, 0]),
        ]);
        assert!(prepend_quest_running_condition(&mut record, 0x0810_E200));
        let after_first = record.fields.len();
        assert!(!prepend_quest_running_condition(&mut record, 0x0810_E200));
        assert_eq!(record.fields.len(), after_first);
    }

    #[test]
    fn leaves_player_driven_subtypes_out_of_the_bark_set() {
        for subtype in PLAYER_DRIVEN_TOPIC_SUBTYPES {
            assert!(
                PLAYER_DRIVEN_TOPIC_SUBTYPES.contains(subtype),
                "{subtype:?} must stay excluded"
            );
        }
        assert!(!PLAYER_DRIVEN_TOPIC_SUBTYPES.contains(b"ATCK"));
        assert!(!PLAYER_DRIVEN_TOPIC_SUBTYPES.contains(b"IDLE"));
    }
}
