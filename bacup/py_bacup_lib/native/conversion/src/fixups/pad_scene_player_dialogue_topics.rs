//! Fixup: pad every `SCEN` player-dialogue action out to a full set of topic
//! slots, minting an empty scene `DIAL` for each gap.
//!
//! XDI (Extended Dialogue Interface, a required master for this conversion) walks a
//! `BGSSceneActionPlayerDialogue`'s four player-topic slots in lockstep with its
//! four NPC-response slots and dereferences the player topic without a null check:
//!
//! ```text
//! 0004E06  lea  rcx, [rsi + 0x98]      ; &action->pNPCResponseTopicsA[0]
//! 0004E20  mov  rax, [rcx - 0x78]      ; player topic slot i (action+0x20 + 8i)
//! 0004E28  mov  r10, [rcx]             ; npc response slot i
//! 0004E2F  movsxd r11, [rax + 0x60]    ; TESTopic::numTopicInfos — NO null check
//! 0004E37  test r10, r10 / je          ; the NPC slot IS null-checked
//! 0005139  inc rdx / add rcx,8 / cmp rdx,4 / jl 0x4e20   ; exactly 4 iterations
//! ```
//!
//! FO76 stores player choices as a variable-length `ESCS` list, so the translator
//! emits one slot per choice: a two-choice action ships `PTOP`+`NTOP` and leaves
//! `NETO`/`QTOP` null at runtime. 1831 of 3386 converted actions were partial, each
//! a CTD when that scene's player menu opens (`XDI.dll+0004E2F`,
//! `movsxd r11,[rax+0x60]`, rax=0).
//!
//! Ground truth is `CWPointLookoutFO4.esm`, a shipping XDI-exclusive port: all 117
//! player-dialogue actions fill all eight slots with distinct DIALs, and 73% park at
//! least one empty topic (`TIFC` = 0) in an unused slot; XDI reads count 0 and skips
//! the inner loop. Its scene DIALs carry exactly `PNAM QNAM DATA SNAM TIFC` (no
//! `EDID`/`FULL`), so each pad copies a sibling slot's DIAL (priority, quest,
//! category/subtype, `SNAM` "SCEN") with the count zeroed.
//!
//! DIAL parentage is group topology, not a field: pads go into the owning QUST's
//! quest-children group via `PluginSession::add_quest_child_records`, not
//! `add_records`. Idempotent: a full slot family is left untouched.

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, write_u32_field};
use crate::session::PluginSession;

/// The four player-choice topic slots, in the order XDI indexes them.
const PLAYER_SLOTS: [&str; 4] = ["PTOP", "NTOP", "NETO", "QTOP"];
/// The four NPC-response topic slots, paired with `PLAYER_SLOTS` by index.
const NPC_SLOTS: [&str; 4] = ["NPOT", "NNGT", "NNUT", "NQUT"];
/// Copied verbatim from the donor topic; everything else is dropped.
const DONOR_FIELDS: [&str; 4] = ["PNAM", "QNAM", "DATA", "SNAM"];

pub struct PadScenePlayerDialogueTopicsFixup;

impl Fixup for PadScenePlayerDialogueTopicsFixup {
    fn name(&self) -> &'static str {
        "pad_scene_player_dialogue_topics"
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
        let interner = mapper.interner;
        let own_plugin = interner.intern(&session.target_slot().parsed.plugin_name);

        let scen_sig =
            SigCode::from_str("SCEN").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let qust_sig =
            SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let dial_sig =
            SigCode::from_str("DIAL").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        // A pad can only be parented if its quest is in this plugin. Actions whose
        // topics hang off a master's quest are left alone: half-padding a row would
        // point a slot at a DIAL that never got written.
        let own_quests: FxHashSet<u32> = session
            .form_keys_of_sig(qust_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .into_iter()
            .filter(|fk| fk.plugin == own_plugin)
            .map(|fk| fk.local)
            .collect();

        let mut scen_fks = session
            .form_keys_of_sig(scen_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        scen_fks.sort_unstable_by_key(|fk| fk.local);

        // Pass 1 — reads only. Every structural insert invalidates the target read
        // indexes, so all decoding happens before the first write.
        let mut scenes: Vec<(Record, Vec<ActionPlan>)> = Vec::new();
        let mut donor_fks: FxHashSet<FormKey> = FxHashSet::default();
        for scen_fk in scen_fks {
            let Ok(record) = session.record_decoded(&scen_fk, target_schema, interner) else {
                continue;
            };
            let plans = plan_scene(&record);
            if plans.is_empty() {
                continue;
            }
            for plan in &plans {
                donor_fks.insert(plan.player_donor);
                donor_fks.insert(plan.npc_donor);
            }
            scenes.push((record, plans));
        }
        if scenes.is_empty() {
            return Ok(report);
        }

        let mut donors: FxHashMap<FormKey, DonorTopic> = FxHashMap::default();
        for donor_fk in donor_fks {
            if donor_fk.plugin != own_plugin {
                continue;
            }
            let Ok(record) = session.record_decoded(&donor_fk, target_schema, interner) else {
                continue;
            };
            if let Some(donor) = DonorTopic::from_record(&record, &own_quests, own_plugin) {
                donors.insert(donor_fk, donor);
            }
        }

        mapper.reserve_object_ids(
            session
                .local_object_ids_in_handle(session.target_id())
                .map_err(|e| FixupError::HandleError(e.to_string()))?,
        );

        // Pass 2 — mint the pads and rewrite the rows that got a full set.
        let mut pad_records: Vec<Record> = Vec::new();
        let mut changed_scenes: Vec<Record> = Vec::new();
        let mut unparentable = 0u32;
        for (mut record, plans) in scenes {
            let mut filled: FxHashMap<usize, Vec<(SubrecordSig, FormKey)>> = FxHashMap::default();
            for plan in plans {
                let (Some(player_donor), Some(npc_donor)) =
                    (donors.get(&plan.player_donor), donors.get(&plan.npc_donor))
                else {
                    unparentable += 1;
                    continue;
                };
                let mut minted = Vec::with_capacity(plan.missing.len());
                for (slot, family) in &plan.missing {
                    let donor = match family {
                        SlotFamily::Player => player_donor,
                        SlotFamily::Npc => npc_donor,
                    };
                    let pad_fk = mapper.allocate_generated();
                    pad_records.push(donor.empty_clone(dial_sig, pad_fk));
                    minted.push((*slot, pad_fk));
                }
                filled.insert(plan.row_start, minted);
            }
            if filled.is_empty() {
                continue;
            }
            apply_filled_slots(&mut record, &filled);
            changed_scenes.push(record);
        }
        // Never silent: an action left partial here is still an XDI null deref
        // waiting to happen, and the only cause is a topic parented outside this
        // plugin, which is worth seeing in the log.
        if unparentable > 0 {
            report.warnings.push(interner.intern(&format!(
                "pad_scene_player_dialogue_topics left {unparentable} actions partial: no donor topic on a local quest"
            )));
        }
        if pad_records.is_empty() {
            return Ok(report);
        }

        let expected_pads = pad_records.len();
        let expected_scenes = changed_scenes.len();
        report.records_changed = session
            .replace_records_contents(changed_scenes, target_schema, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        if report.records_changed as usize != expected_scenes {
            return Err(FixupError::HandleError(format!(
                "pad_scene_player_dialogue_topics rewrote {} of {expected_scenes} scenes",
                report.records_changed
            )));
        }
        let added = session
            .add_quest_child_records(pad_records, target_schema, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if added != expected_pads {
            return Err(FixupError::HandleError(format!(
                "pad_scene_player_dialogue_topics parented {added} of {expected_pads} empty topics"
            )));
        }
        report.records_added = added.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SlotFamily {
    Player,
    Npc,
}

/// One player-dialogue action row that is missing at least one topic slot.
#[derive(Debug)]
struct ActionPlan {
    /// Index of the row's `ANAM` in the SCEN's field list — the row's identity.
    row_start: usize,
    missing: Vec<(SubrecordSig, SlotFamily)>,
    player_donor: FormKey,
    npc_donor: FormKey,
}

/// The fields an empty pad inherits from a sibling slot in the same action.
struct DonorTopic {
    fields: SmallVec<[FieldEntry; 8]>,
    tifc: FieldEntry,
}

impl DonorTopic {
    fn from_record(
        record: &Record,
        own_quests: &FxHashSet<u32>,
        own_plugin: crate::sym::Sym,
    ) -> Option<Self> {
        let quest_is_local = record.fields.iter().any(|entry| {
            entry.sig.as_str() == "QNAM"
                && matches!(&entry.value, FieldValue::FormKey(fk)
                    if fk.plugin == own_plugin && own_quests.contains(&fk.local))
        });
        if !quest_is_local {
            return None;
        }
        let mut fields = SmallVec::new();
        for signature in DONOR_FIELDS {
            let entry = record
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == signature)?;
            fields.push(entry.clone());
        }
        let mut tifc = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "TIFC")
            .cloned()
            .unwrap_or_else(|| FieldEntry {
                sig: SubrecordSig::from_str("TIFC").expect("TIFC signature"),
                value: FieldValue::Uint(0),
            });
        write_u32_field(&mut tifc.value, 0);
        Some(DonorTopic { fields, tifc })
    }

    fn empty_clone(&self, dial_sig: SigCode, form_key: FormKey) -> Record {
        let mut pad = Record::new(dial_sig, form_key);
        pad.fields = self.fields.clone();
        pad.fields.push(self.tifc.clone());
        pad
    }
}

/// Field-index spans of the SCEN's action rows. Everything ahead of the first
/// `ANAM` is the record prefix; each `ANAM` opens the next row. Plan and rewrite
/// both key off `start`, so they must agree on this split.
fn action_row_spans(record: &Record) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    for (index, entry) in record.fields.iter().enumerate() {
        if index > 0 && entry.sig.as_str() == "ANAM" {
            spans.push((start, index));
            start = index;
        }
    }
    spans.push((start, record.fields.len()));
    spans
}

/// Split a SCEN into action rows and plan the gaps in each one.
///
/// The player family is always completed — that is the slot XDI dereferences
/// blind. The NPC family is completed only when the action already answers at
/// least one choice: vanilla `Fallout4.esm` ships 601 player-dialogue actions
/// with no NPC responses at all, and XDI null-checks that side.
fn plan_scene(record: &Record) -> Vec<ActionPlan> {
    if record.sig.as_str() != "SCEN" {
        return Vec::new();
    }
    action_row_spans(record)
        .into_iter()
        .filter_map(|(start, end)| plan_action(start, &record.fields[start..end]))
        .collect()
}

/// The slot a family entry occupies, but only when it actually references a
/// topic: an earlier pass can null a dangling `PTOP` in place, and a nulled slot
/// is exactly as fatal to XDI as an absent one.
fn family_slot(entry: &FieldEntry, family: &[&str; 4]) -> Option<(usize, FormKey)> {
    let FieldValue::FormKey(fk) = entry.value else {
        return None;
    };
    if fk.local == 0 {
        return None;
    }
    family
        .iter()
        .position(|signature| *signature == entry.sig.as_str())
        .map(|slot| (slot, fk))
}

fn plan_action(row_start: usize, row: &[FieldEntry]) -> Option<ActionPlan> {
    let player: Vec<(usize, FormKey)> = row
        .iter()
        .filter_map(|entry| family_slot(entry, &PLAYER_SLOTS))
        .collect();
    let npc: Vec<(usize, FormKey)> = row
        .iter()
        .filter_map(|entry| family_slot(entry, &NPC_SLOTS))
        .collect();
    if player.is_empty() && npc.is_empty() {
        return None;
    }
    let player_donor = player.first().or_else(|| npc.first())?.1;
    let npc_donor = npc.first().map_or(player_donor, |(_, fk)| *fk);

    let mut missing = Vec::new();
    for (slot, signature) in PLAYER_SLOTS.iter().enumerate() {
        if !player.iter().any(|(present, _)| *present == slot) {
            missing.push((
                SubrecordSig::from_str(signature).expect("player slot signature"),
                SlotFamily::Player,
            ));
        }
    }
    if !npc.is_empty() {
        for (slot, signature) in NPC_SLOTS.iter().enumerate() {
            if !npc.iter().any(|(present, _)| *present == slot) {
                missing.push((
                    SubrecordSig::from_str(signature).expect("npc slot signature"),
                    SlotFamily::Npc,
                ));
            }
        }
    }
    if missing.is_empty() {
        return None;
    }
    Some(ActionPlan {
        row_start,
        missing,
        player_donor,
        npc_donor,
    })
}

/// Splice the minted slots into their rows. Each family is emitted as one
/// contiguous block where that family already started, so interleaved
/// `ONAM`/`DALC`/`DTID` entries keep their position relative to the topic blocks.
fn apply_filled_slots(
    record: &mut Record,
    filled: &FxHashMap<usize, Vec<(SubrecordSig, FormKey)>>,
) {
    let spans = action_row_spans(record);
    let original: Vec<FieldEntry> = std::mem::take(&mut record.fields).into_vec();
    let mut out: SmallVec<[FieldEntry; 8]> = SmallVec::with_capacity(original.len() + 8);
    for (start, end) in spans {
        emit_row(&mut out, &original[start..end], filled.get(&start));
    }
    record.fields = out;
}

fn emit_row(
    out: &mut SmallVec<[FieldEntry; 8]>,
    row: &[FieldEntry],
    pads: Option<&Vec<(SubrecordSig, FormKey)>>,
) {
    let Some(pads) = pads else {
        out.extend(row.iter().cloned());
        return;
    };
    let player = complete_family(row, &PLAYER_SLOTS, pads);
    let npc = complete_family(row, &NPC_SLOTS, pads);
    let mut player_done = false;
    let mut npc_done = false;

    for entry in row {
        let signature = entry.sig.as_str();
        if PLAYER_SLOTS.contains(&signature) {
            if !player_done {
                player_done = true;
                out.extend(player.iter().flatten().cloned());
            }
            continue;
        }
        if NPC_SLOTS.contains(&signature) {
            if !npc_done {
                // A row with NPC responses but no usable player choice still
                // needs the player family, ahead of the responses.
                if !player_done {
                    player_done = true;
                    out.extend(player.iter().flatten().cloned());
                }
                npc_done = true;
                out.extend(npc.iter().flatten().cloned());
            }
            continue;
        }
        out.push(entry.clone());
    }
    if !player_done {
        out.extend(player.iter().flatten().cloned());
    }
    if !npc_done {
        out.extend(npc.iter().flatten().cloned());
    }
}

/// One family's four slots in canonical order: the row's own entry where it
/// holds a real reference, the minted pad everywhere else.
fn complete_family(
    row: &[FieldEntry],
    family: &[&str; 4],
    pads: &[(SubrecordSig, FormKey)],
) -> [Option<FieldEntry>; 4] {
    std::array::from_fn(|slot| {
        row.iter()
            .find(|entry| family_slot(entry, family).is_some_and(|(found, _)| found == slot))
            .cloned()
            .or_else(|| {
                pads.iter()
                    .find(|(sig, _)| sig.as_str() == family[slot])
                    .map(|(sig, fk)| FieldEntry {
                        sig: *sig,
                        value: FieldValue::FormKey(*fk),
                    })
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SigCode;
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;
    use smallvec::smallvec;

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn fk(local: u32, plugin: crate::sym::Sym) -> FormKey {
        FormKey { local, plugin }
    }

    fn scene(fields: SmallVec<[FieldEntry; 8]>, plugin: crate::sym::Sym) -> Record {
        Record {
            sig: SigCode::from_str("SCEN").unwrap(),
            form_key: fk(0x84BF6A, plugin),
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    fn slot_sequence(record: &Record) -> Vec<String> {
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str().to_string())
            .collect()
    }

    /// The measured crash case: SCEN 0884BF6A had PTOP/NTOP/NETO but no QTOP,
    /// and XDI faulted on slot 3.
    #[test]
    fn plans_the_missing_fourth_player_slot() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let record = scene(
            smallvec![
                field("EDID", FieldValue::Bytes(SmallVec::new())),
                field("ANAM", FieldValue::Uint(0)),
                field("PTOP", FieldValue::FormKey(fk(0x84BECE, plugin))),
                field("NTOP", FieldValue::FormKey(fk(0x84BECD, plugin))),
                field("NETO", FieldValue::FormKey(fk(0x84BECC, plugin))),
                field("NPOT", FieldValue::FormKey(fk(0x84BEC0, plugin))),
                field("NNGT", FieldValue::FormKey(fk(0x84BEC1, plugin))),
                field("NNUT", FieldValue::FormKey(fk(0x84BEC2, plugin))),
                field("DTGT", FieldValue::Uint(0)),
            ],
            plugin,
        );

        let plans = plan_scene(&record);
        assert_eq!(plans.len(), 1);
        let missing: Vec<&str> = plans[0]
            .missing
            .iter()
            .map(|(sig, _)| sig.as_str())
            .collect();
        assert_eq!(missing, vec!["QTOP", "NQUT"]);
        assert_eq!(plans[0].player_donor, fk(0x84BECE, plugin));
        assert_eq!(plans[0].npc_donor, fk(0x84BEC0, plugin));
    }

    #[test]
    fn action_without_topics_and_full_action_are_both_skipped() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let record = scene(
            smallvec![
                field("ANAM", FieldValue::Uint(1)),
                field("ALID", FieldValue::Uint(0)),
                field("ANAM", FieldValue::Uint(0)),
                field("PTOP", FieldValue::FormKey(fk(1, plugin))),
                field("NTOP", FieldValue::FormKey(fk(2, plugin))),
                field("NETO", FieldValue::FormKey(fk(3, plugin))),
                field("QTOP", FieldValue::FormKey(fk(4, plugin))),
            ],
            plugin,
        );

        assert!(plan_scene(&record).is_empty());
    }

    /// Vanilla `Fallout4.esm` ships 601 actions with a full player family and no
    /// NPC responses; XDI null-checks that side, so they must not be padded.
    #[test]
    fn player_only_action_does_not_gain_npc_responses() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let record = scene(
            smallvec![
                field("ANAM", FieldValue::Uint(0)),
                field("PTOP", FieldValue::FormKey(fk(1, plugin))),
                field("NTOP", FieldValue::FormKey(fk(2, plugin))),
            ],
            plugin,
        );

        let plans = plan_scene(&record);
        let missing: Vec<&str> = plans[0]
            .missing
            .iter()
            .map(|(sig, _)| sig.as_str())
            .collect();
        assert_eq!(missing, vec!["NETO", "QTOP"]);
    }

    #[test]
    fn splice_restores_canonical_order_and_keeps_the_rest_of_the_row() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let mut record = scene(
            smallvec![
                field("EDID", FieldValue::Bytes(SmallVec::new())),
                field("ANAM", FieldValue::Uint(0)),
                field("ENAM", FieldValue::Uint(0)),
                field("PTOP", FieldValue::FormKey(fk(1, plugin))),
                field("NTOP", FieldValue::FormKey(fk(2, plugin))),
                field("ONAM", FieldValue::Uint(0)),
                field("NPOT", FieldValue::FormKey(fk(5, plugin))),
                field("NNGT", FieldValue::FormKey(fk(6, plugin))),
                field("DTGT", FieldValue::Uint(0)),
            ],
            plugin,
        );
        let mut filled = FxHashMap::default();
        filled.insert(
            1usize,
            vec![
                (SubrecordSig::from_str("NETO").unwrap(), fk(0xF01, plugin)),
                (SubrecordSig::from_str("QTOP").unwrap(), fk(0xF02, plugin)),
                (SubrecordSig::from_str("NNUT").unwrap(), fk(0xF03, plugin)),
                (SubrecordSig::from_str("NQUT").unwrap(), fk(0xF04, plugin)),
            ],
        );

        apply_filled_slots(&mut record, &filled);

        assert_eq!(
            slot_sequence(&record),
            vec![
                "EDID", "ANAM", "ENAM", "PTOP", "NTOP", "NETO", "QTOP", "ONAM", "NPOT", "NNGT",
                "NNUT", "NQUT", "DTGT",
            ]
        );
        assert!(plan_scene(&record).is_empty(), "padding must be idempotent");
    }

    /// Only `TIFC` separates an empty Point Lookout scene topic from a real one,
    /// and neither carries an `EDID`.
    #[test]
    fn pad_clones_the_donor_shape_with_a_zero_info_count() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let quest = fk(0x080003, plugin);
        let own_quests: FxHashSet<u32> = [quest.local].into_iter().collect();
        let donor_record = Record {
            sig: SigCode::from_str("DIAL").unwrap(),
            form_key: fk(0x84BECE, plugin),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                field("EDID", FieldValue::Bytes(SmallVec::new())),
                field("PNAM", FieldValue::Float(50.0)),
                field("QNAM", FieldValue::FormKey(quest)),
                field(
                    "DATA",
                    FieldValue::Bytes(SmallVec::from_slice(&[0, 2, 17, 0]))
                ),
                field("SNAM", FieldValue::Bytes(SmallVec::from_slice(b"SCEN"))),
                field("TIFC", FieldValue::Uint(3)),
            ],
            warnings: SmallVec::new(),
        };

        let donor = DonorTopic::from_record(&donor_record, &own_quests, plugin).unwrap();
        let pad = donor.empty_clone(SigCode::from_str("DIAL").unwrap(), fk(0xF01, plugin));

        assert_eq!(
            slot_sequence(&pad),
            vec!["PNAM", "QNAM", "DATA", "SNAM", "TIFC"]
        );
        assert!(pad.eid.is_none());
        assert!(matches!(
            pad.fields.last().unwrap().value,
            FieldValue::Uint(0)
        ));
        assert_eq!(pad.form_key, fk(0xF01, plugin));
    }

    /// A topic whose quest lives in a master cannot be parented, so its action is
    /// left partial rather than pointed at a DIAL that never gets written.
    #[test]
    fn donor_on_a_foreign_quest_is_rejected() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let master = interner.intern("Fallout4.esm");
        let donor_record = Record {
            sig: SigCode::from_str("DIAL").unwrap(),
            form_key: fk(0x84BECE, plugin),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                field("PNAM", FieldValue::Float(50.0)),
                field("QNAM", FieldValue::FormKey(fk(0x0123, master))),
                field(
                    "DATA",
                    FieldValue::Bytes(SmallVec::from_slice(&[0, 2, 17, 0]))
                ),
                field("SNAM", FieldValue::Bytes(SmallVec::from_slice(b"SCEN"))),
                field("TIFC", FieldValue::Uint(1)),
            ],
            warnings: SmallVec::new(),
        };

        assert!(DonorTopic::from_record(&donor_record, &FxHashSet::default(), plugin).is_none());
    }
}
