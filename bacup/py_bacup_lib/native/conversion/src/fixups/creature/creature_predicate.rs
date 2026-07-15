//! Per-record "is this a creature?" predicate.
//!
//! # Why this exists
//! The creature-race internal fixups (`fix_creature_npc_records`,
//! `nullify_creature_death_items`, `augment_creature_factions`,
//! `fix_creature_weapons_and_records`, ...) currently gate on
//! `config.root_sig == NPC_/LVLN` — i.e. they only run during a *creature-rooted*
//! bounded graph walk, where every in-scope record is by construction a
//! creature. On the whole-plugin path `root_sig` is `None`, so they self-skip;
//! naively un-gating them would run their mutations over EVERY NPC_/RACE in the
//! 3.7M-record plugin — and several mutate unconditionally (append
//! `ActorTypeCreature` keyword + creature combat perks, strip INAM death items,
//! ...), which would corrupt every HUMAN NPC.
//!
//! This module provides a per-record creature test so those fixups can run
//! whole-plugin while only touching actual creatures. It is intentionally a
//! standalone, IO-free helper: record inspection is pure, and RACE resolution is
//! supplied by the caller as a closure, so this file has no session/handle
//! coupling and is trivially unit-testable.
//!
//! # Signal
//! An actor is a creature iff its actor-type is `ActorTypeCreature`. In FO4 that
//! is the keyword `Fallout4.esm:013795`, attached either directly on the NPC's
//! `KWDA`/`KSIZ`, or — far more commonly — on the NPC's RACE (`RNAM`) `KWDA`.
//! We check the NPC first (cheap, no resolution) and fall back to resolving its
//! race. Keyword FormIDs are matched on their low 24 bits because the master
//! byte differs between converted (07-prefix) and inherited-vanilla (00-prefix)
//! plugins, but `ActorTypeCreature` is always Fallout4.esm-local `013795`.
//!
//! # Template chain (UseTraits)
//! Many FO76 creatures are Traits-template NPCs (ACBS `UseTraits` bit set): their
//! RACE is inherited from `TPLT` (an NPC_ or an LVLN leveled-character list), so
//! their literal `RNAM` is runtime-irrelevant (e.g. the Gulper whose own RNAM was
//! remapped to a STAT but whose real race comes from `LCharGulper`). Use
//! [`npc_is_creature_following_template`] for those — it walks the template chain
//! instead of trusting RNAM. [`npc_is_creature`] is the literal-RNAM-only variant
//! for callers that have already resolved templates or know the NPC isn't a
//! template inheritor.

use crate::ids::FormKey;
use crate::ids::SubrecordSig;
use crate::record::{FieldValue, Record};

/// `Fallout4.esm:013795` — the `ActorTypeCreature` keyword. Master byte 0 is
/// Fallout4.esm in every FO4-target plugin; we match on the low 24 bits.
pub const ACTOR_TYPE_CREATURE_LOW24: u32 = 0x00_013795;

/// `Fallout4.esm:013794` — the `ActorTypeNPC` keyword (humanoids). Used only as
/// a tie-breaker / negative signal; presence of ActorTypeCreature wins.
pub const ACTOR_TYPE_NPC_LOW24: u32 = 0x00_013794;

/// FO4 NPC_ ACBS `template_flags` bit for `UseTraits`. When set, the actor
/// inherits its Traits — INCLUDING its RACE — from the record pointed at by
/// `TPLT` (an NPC_ or an LVLN leveled-character list), NOT from its own `RNAM`.
/// So a Traits-template NPC's literal `RNAM` is runtime-irrelevant; classify it
/// by walking the template chain instead (see `npc_is_creature_following_template`).
pub const ACBS_TEMPLATE_FLAG_USE_TRAITS: u16 = 0x0001;

/// Byte offset of the u16 `template_flags` field inside the FO4 NPC_ `ACBS`
/// struct (`struct:I,h,H,H,H,h,H,H,B,B`): I(0..4) h(4..6) H(6..8) H(8..10)
/// H(10..12) h(12..14) → template_flags H at offset 14.
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;

/// Max template-chain depth before we give up (cycle / pathological guard).
const MAX_TEMPLATE_DEPTH: u32 = 8;

fn low24(form_id_or_local: u32) -> u32 {
    form_id_or_local & 0x00FF_FFFF
}

/// Invoke `visit` with the low-24-bit id of every keyword in the record's first
/// `KWDA`, handling both decode shapes: `FieldValue::List(FormKey...)` (the
/// `read_record` formid_array decode) and `FieldValue::Bytes` (raw 4-byte rows,
/// the target-session / fixup-mutated shape).
fn for_each_kwda_low24(record: &Record, mut visit: impl FnMut(u32)) {
    let Ok(kwda_sig) = SubrecordSig::from_str("KWDA") else {
        return;
    };
    for entry in &record.fields {
        if entry.sig != kwda_sig {
            continue;
        }
        match &entry.value {
            FieldValue::List(items) => {
                for item in items {
                    if let FieldValue::FormKey(fk) = item {
                        visit(low24(fk.local));
                    }
                }
            }
            FieldValue::FormKey(fk) => visit(low24(fk.local)),
            FieldValue::Bytes(data) => {
                for chunk in data.chunks_exact(4) {
                    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    visit(low24(raw));
                }
            }
            _ => {}
        }
    }
}

/// Whether `record` (an NPC_ or RACE) directly carries the `ActorTypeCreature`
/// keyword in its own `KWDA`.
pub fn record_has_actor_type_creature(record: &Record) -> bool {
    let mut found = false;
    for_each_kwda_low24(record, |kw| {
        if kw == ACTOR_TYPE_CREATURE_LOW24 {
            found = true;
        }
    });
    found
}

/// Whether `record` directly carries the `ActorTypeNPC` (humanoid) keyword.
pub fn record_has_actor_type_npc(record: &Record) -> bool {
    let mut found = false;
    for_each_kwda_low24(record, |kw| {
        if kw == ACTOR_TYPE_NPC_LOW24 {
            found = true;
        }
    });
    found
}

/// Extract the NPC's race FormKey from its `RNAM` subrecord, if present and
/// non-null. Returns `None` for a missing/null/non-FormKey RNAM.
pub fn npc_race_form_key(npc: &Record) -> Option<FormKey> {
    let rnam_sig = SubrecordSig::from_str("RNAM").ok()?;
    npc.fields.iter().find(|e| e.sig == rnam_sig).and_then(|e| {
        if let FieldValue::FormKey(fk) = &e.value {
            Some(*fk)
        } else {
            None
        }
    })
}

/// Extract the NPC's template FormKey from its `TPLT` subrecord (points at an
/// NPC_ or LVLN). `None` if missing/null.
pub fn npc_template_form_key(npc: &Record) -> Option<FormKey> {
    let tplt_sig = SubrecordSig::from_str("TPLT").ok()?;
    npc.fields.iter().find(|e| e.sig == tplt_sig).and_then(|e| {
        if let FieldValue::FormKey(fk) = &e.value {
            Some(*fk)
        } else {
            None
        }
    })
}

/// Read the u16 `template_flags` from the NPC's `ACBS` struct. `ACBS` decodes to
/// a `FieldValue::Bytes` blob on the target/fixup path; returns `None` if absent
/// or too short.
pub fn npc_acbs_template_flags(npc: &Record) -> Option<u16> {
    let acbs_sig = SubrecordSig::from_str("ACBS").ok()?;
    let entry = npc.fields.iter().find(|e| e.sig == acbs_sig)?;
    let FieldValue::Bytes(data) = &entry.value else {
        return None;
    };
    if data.len() < ACBS_TEMPLATE_FLAGS_OFFSET + 2 {
        return None;
    }
    Some(u16::from_le_bytes([
        data[ACBS_TEMPLATE_FLAGS_OFFSET],
        data[ACBS_TEMPLATE_FLAGS_OFFSET + 1],
    ]))
}

/// Whether the NPC inherits its RACE from its template (ACBS `UseTraits` bit
/// set). When true, the literal `RNAM` is runtime-irrelevant and the real race
/// comes from following `TPLT` (see `npc_is_creature_following_template`).
pub fn npc_inherits_traits_from_template(npc: &Record) -> bool {
    npc_acbs_template_flags(npc)
        .map(|f| f & ACBS_TEMPLATE_FLAG_USE_TRAITS != 0)
        .unwrap_or(false)
}

/// Verdict for `npc_is_creature`, distinguishing a confident NO (humanoid
/// keyword present) from an UNKNOWN (no actor-type keyword and race unresolved).
/// Callers that mutate creature-only data should treat `Unknown` conservatively
/// (skip the mutation) to avoid corrupting humans whose race we couldn't read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureVerdict {
    Creature,
    NotCreature,
    Unknown,
}

impl CreatureVerdict {
    /// True only for a confident creature classification.
    pub fn is_creature(self) -> bool {
        matches!(self, CreatureVerdict::Creature)
    }
}

/// Classify an NPC as creature / not-creature / unknown.
///
/// Order of evidence (first hit wins):
/// 1. NPC's own `KWDA` has `ActorTypeCreature` ⇒ Creature.
/// 2. NPC's own `KWDA` has `ActorTypeNPC` (and not creature) ⇒ NotCreature.
/// 3. Resolve the NPC's `RNAM` race via `resolve_race`; the race's `KWDA`
///    decides (ActorTypeCreature ⇒ Creature, ActorTypeNPC ⇒ NotCreature).
/// 4. No actor-type keyword anywhere reachable ⇒ Unknown.
///
/// `resolve_race` returns the decoded RACE `Record` for a FormKey, or `None`
/// when the race can't be read (dropped, cross-master, decode error) — keeping
/// this helper free of session/handle types.
pub fn npc_is_creature(
    npc: &Record,
    resolve_race: impl FnOnce(FormKey) -> Option<Record>,
) -> CreatureVerdict {
    match record_actor_type(npc) {
        CreatureVerdict::Unknown => {}
        v => return v,
    }
    let Some(race_fk) = npc_race_form_key(npc) else {
        return CreatureVerdict::Unknown;
    };
    let Some(race) = resolve_race(race_fk) else {
        return CreatureVerdict::Unknown;
    };
    record_actor_type(&race)
}

/// Classify a single record by its OWN actor-type keyword (no resolution).
fn record_actor_type(record: &Record) -> CreatureVerdict {
    if record_has_actor_type_creature(record) {
        CreatureVerdict::Creature
    } else if record_has_actor_type_npc(record) {
        CreatureVerdict::NotCreature
    } else {
        CreatureVerdict::Unknown
    }
}

/// Template-aware creature classification.
///
/// A Traits-template NPC (ACBS `UseTraits` set) inherits its RACE from `TPLT`
/// (an NPC_ or LVLN), so its literal `RNAM` is runtime-irrelevant — classifying
/// off RNAM would misread it (e.g. the FO76 Gulper whose own RNAM was remapped
/// to a STAT but whose real race comes from `LCharGulper`). This walks the
/// template chain instead when the Traits bit is set.
///
/// Resolution order:
/// 1. NPC's OWN actor-type keyword (definitive, no resolution) ⇒ that verdict.
/// 2. If `UseTraits` is set and `TPLT` resolves: recurse on the template
///    (NPC_ ⇒ recurse template-aware; RACE ⇒ its keyword; LVLN ⇒ classify each
///    entry the resolver yields, Creature if ANY entry is a creature).
/// 3. Else fall back to the literal `RNAM` race (the non-template path).
/// 4. Anything unresolved ⇒ `Unknown` (conservative — never mis-flag a human).
///
/// `resolve` returns the decoded `Record` for a FormKey (RACE / NPC_ / LVLN) or
/// `None` when unreadable, and `lvln_entry_npcs` yields the NPC_ FormKeys a LVLN
/// references (empty when the arg isn't a LVLN or can't be read) — both supplied
/// by the caller so this stays free of session/handle types. `depth` guards
/// against template cycles.
pub fn npc_is_creature_following_template(
    npc: &Record,
    resolve: &impl Fn(FormKey) -> Option<Record>,
    lvln_entry_npcs: &impl Fn(FormKey) -> Vec<FormKey>,
) -> CreatureVerdict {
    classify_actor_following_template(npc, resolve, lvln_entry_npcs, MAX_TEMPLATE_DEPTH)
}

fn classify_actor_following_template(
    npc: &Record,
    resolve: &impl Fn(FormKey) -> Option<Record>,
    lvln_entry_npcs: &impl Fn(FormKey) -> Vec<FormKey>,
    depth: u32,
) -> CreatureVerdict {
    // 1. Own keyword is definitive regardless of template.
    match record_actor_type(npc) {
        CreatureVerdict::Unknown => {}
        v => return v,
    }
    if depth == 0 {
        return CreatureVerdict::Unknown;
    }

    // 2. Traits-template NPC: race comes from TPLT, not RNAM.
    if npc_inherits_traits_from_template(npc) {
        if let Some(tplt_fk) = npc_template_form_key(npc) {
            if let Some(tmpl) = resolve(tplt_fk) {
                match tmpl.sig.as_str() {
                    "NPC_" => {
                        return classify_actor_following_template(
                            &tmpl,
                            resolve,
                            lvln_entry_npcs,
                            depth - 1,
                        );
                    }
                    "RACE" => return record_actor_type(&tmpl),
                    "LVLN" => {
                        // Creature if ANY leveled entry classifies as creature.
                        let mut saw_not_creature = false;
                        for entry_fk in lvln_entry_npcs(tplt_fk) {
                            if let Some(entry_npc) = resolve(entry_fk) {
                                match classify_actor_following_template(
                                    &entry_npc,
                                    resolve,
                                    lvln_entry_npcs,
                                    depth - 1,
                                ) {
                                    CreatureVerdict::Creature => return CreatureVerdict::Creature,
                                    CreatureVerdict::NotCreature => saw_not_creature = true,
                                    CreatureVerdict::Unknown => {}
                                }
                            }
                        }
                        return if saw_not_creature {
                            CreatureVerdict::NotCreature
                        } else {
                            CreatureVerdict::Unknown
                        };
                    }
                    _ => {}
                }
            }
        }
        // UseTraits set but template unresolved → can't tell from RNAM (it's
        // irrelevant) → Unknown.
        return CreatureVerdict::Unknown;
    }

    // 3. Non-template NPC: literal RNAM race.
    let Some(race_fk) = npc_race_form_key(npc) else {
        return CreatureVerdict::Unknown;
    };
    let Some(race) = resolve(race_fk) else {
        return CreatureVerdict::Unknown;
    };
    record_actor_type(&race)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn empty_record(sig_str: &str, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig_str).unwrap(),
            form_key: fk(0x000100, "Output.esm", interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push(record: &mut Record, sig_str: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig_str).unwrap(),
            value,
        });
    }

    fn kwda_list(locals: &[u32], plugin: &str, interner: &StringInterner) -> FieldValue {
        FieldValue::List(
            locals
                .iter()
                .map(|&l| FieldValue::FormKey(fk(l, plugin, interner)))
                .collect(),
        )
    }

    fn kwda_bytes(locals: &[u32]) -> FieldValue {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        for &l in locals {
            data.extend_from_slice(&l.to_le_bytes());
        }
        FieldValue::Bytes(data)
    }

    /// 20-byte FO4 ACBS with `template_flags` set to `flags` at offset 14.
    fn acbs_with_template_flags(flags: u16) -> FieldValue {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        data.resize(20, 0u8);
        let b = flags.to_le_bytes();
        data[ACBS_TEMPLATE_FLAGS_OFFSET] = b[0];
        data[ACBS_TEMPLATE_FLAGS_OFFSET + 1] = b[1];
        FieldValue::Bytes(data)
    }

    #[test]
    fn npc_with_creature_keyword_list_shape_is_creature() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "KWDA",
            kwda_list(
                &[ACTOR_TYPE_CREATURE_LOW24, 0xABCDEF],
                "Fallout4.esm",
                &interner,
            ),
        );
        let v = npc_is_creature(&npc, |_| None);
        assert_eq!(v, CreatureVerdict::Creature);
        assert!(v.is_creature());
    }

    #[test]
    fn npc_with_creature_keyword_bytes_shape_is_creature() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        // 07-prefix master byte must NOT defeat the low-24 match.
        push(&mut npc, "KWDA", kwda_bytes(&[0x07_013795]));
        assert_eq!(npc_is_creature(&npc, |_| None), CreatureVerdict::Creature);
    }

    #[test]
    fn npc_with_npc_keyword_is_not_creature() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "KWDA",
            kwda_list(&[ACTOR_TYPE_NPC_LOW24], "Fallout4.esm", &interner),
        );
        assert_eq!(
            npc_is_creature(&npc, |_| None),
            CreatureVerdict::NotCreature
        );
    }

    #[test]
    fn npc_without_keyword_resolves_via_race() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        let race_fk = fk(0x00D191, "Output.esm", &interner);
        push(&mut npc, "RNAM", FieldValue::FormKey(race_fk));

        // Race carries ActorTypeCreature → NPC is a creature.
        let mut race = empty_record("RACE", &interner);
        push(
            &mut race,
            "KWDA",
            kwda_list(&[ACTOR_TYPE_CREATURE_LOW24], "Fallout4.esm", &interner),
        );
        let v = npc_is_creature(&npc, |asked| {
            assert_eq!(asked, race_fk);
            Some(race.clone())
        });
        assert_eq!(v, CreatureVerdict::Creature);
    }

    #[test]
    fn npc_with_unresolvable_race_is_unknown() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "RNAM",
            FieldValue::FormKey(fk(0x0247C1, "Fallout4.esm", &interner)),
        );
        // Dropped race → resolver returns None → Unknown (NOT a false creature).
        assert_eq!(npc_is_creature(&npc, |_| None), CreatureVerdict::Unknown);
    }

    #[test]
    fn npc_with_no_keyword_and_no_rnam_is_unknown() {
        let interner = StringInterner::new();
        let npc = empty_record("NPC_", &interner);
        assert_eq!(npc_is_creature(&npc, |_| None), CreatureVerdict::Unknown);
    }

    #[test]
    fn npc_creature_keyword_wins_over_race_npc_keyword() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "KWDA",
            kwda_list(&[ACTOR_TYPE_CREATURE_LOW24], "Output.esm", &interner),
        );
        // Even if we (wrongly) had a race, the direct creature kwd short-circuits.
        let called = std::cell::Cell::new(false);
        let v = npc_is_creature(&npc, |_| {
            called.set(true);
            None
        });
        assert_eq!(v, CreatureVerdict::Creature);
        assert!(
            !called.get(),
            "race resolver must not be called when NPC self-identifies"
        );
    }

    // -----------------------------------------------------------------------
    // Template-chain (UseTraits) classification
    // -----------------------------------------------------------------------

    #[test]
    fn traits_template_npc_classifies_via_template_npc_race() {
        // The Gulper case: NPC's own RNAM is a STAT (irrelevant because UseTraits
        // is set); real race comes via TPLT → template NPC → its RACE (creature).
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "ACBS",
            acbs_with_template_flags(ACBS_TEMPLATE_FLAG_USE_TRAITS),
        );
        // Garbage RNAM that must be IGNORED because UseTraits is set.
        push(
            &mut npc,
            "RNAM",
            FieldValue::FormKey(fk(0x0247C1, "Fallout4.esm", &interner)),
        );
        let tmpl_fk = fk(0x110D7D, "Output.esm", &interner);
        push(&mut npc, "TPLT", FieldValue::FormKey(tmpl_fk));

        // Template NPC (no own keyword) → its RNAM race is a creature.
        let race_fk = fk(0x110D23, "Output.esm", &interner);
        let mut tmpl = empty_record("NPC_", &interner);
        push(&mut tmpl, "RNAM", FieldValue::FormKey(race_fk));
        let mut race = empty_record("RACE", &interner);
        push(
            &mut race,
            "KWDA",
            kwda_list(&[ACTOR_TYPE_CREATURE_LOW24], "Fallout4.esm", &interner),
        );

        let resolve = |asked: FormKey| -> Option<Record> {
            if asked == tmpl_fk {
                Some(tmpl.clone())
            } else if asked == race_fk {
                Some(race.clone())
            } else {
                None // 0247C1 STAT is never asked because RNAM is ignored
            }
        };
        let lvln = |_: FormKey| Vec::new();
        assert_eq!(
            npc_is_creature_following_template(&npc, &resolve, &lvln),
            CreatureVerdict::Creature
        );
    }

    #[test]
    fn traits_template_npc_ignores_literal_rnam() {
        // UseTraits set, TPLT unresolved → Unknown (must NOT classify off the
        // STAT RNAM, which would be a wrong NotCreature/garbage read).
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "ACBS",
            acbs_with_template_flags(ACBS_TEMPLATE_FLAG_USE_TRAITS),
        );
        push(
            &mut npc,
            "RNAM",
            FieldValue::FormKey(fk(0x0247C1, "Fallout4.esm", &interner)),
        );
        push(
            &mut npc,
            "TPLT",
            FieldValue::FormKey(fk(0x110D7D, "Output.esm", &interner)),
        );
        let resolve = |_: FormKey| None;
        let lvln = |_: FormKey| Vec::new();
        assert_eq!(
            npc_is_creature_following_template(&npc, &resolve, &lvln),
            CreatureVerdict::Unknown
        );
    }

    #[test]
    fn traits_template_via_lvln_any_creature_entry_wins() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "ACBS",
            acbs_with_template_flags(ACBS_TEMPLATE_FLAG_USE_TRAITS),
        );
        let lvln_fk = fk(0x110D7D, "Output.esm", &interner);
        push(&mut npc, "TPLT", FieldValue::FormKey(lvln_fk));

        let lvln_rec = empty_record("LVLN", &interner);
        let entry_fk = fk(0x200001, "Output.esm", &interner);
        let mut entry_npc = empty_record("NPC_", &interner);
        push(
            &mut entry_npc,
            "KWDA",
            kwda_list(&[ACTOR_TYPE_CREATURE_LOW24], "Fallout4.esm", &interner),
        );
        let resolve = |asked: FormKey| -> Option<Record> {
            if asked == lvln_fk {
                Some(lvln_rec.clone())
            } else if asked == entry_fk {
                Some(entry_npc.clone())
            } else {
                None
            }
        };
        let lvln = |asked: FormKey| {
            if asked == lvln_fk {
                vec![entry_fk]
            } else {
                Vec::new()
            }
        };
        assert_eq!(
            npc_is_creature_following_template(&npc, &resolve, &lvln),
            CreatureVerdict::Creature
        );
    }

    #[test]
    fn traits_template_full_chain_lvln_terminal_npc_via_rnam() {
        // The full "TPLT LVLN → terminal NPC (no own keyword) → its RNAM → RACE
        // keyword" chain the lead emphasized: the LVLN entry NPC has NO direct
        // keyword; its creature-ness must be recovered through ITS RNAM race.
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "ACBS",
            acbs_with_template_flags(ACBS_TEMPLATE_FLAG_USE_TRAITS),
        );
        let lvln_fk = fk(0x110D7D, "Output.esm", &interner);
        push(&mut npc, "TPLT", FieldValue::FormKey(lvln_fk));

        let lvln_rec = empty_record("LVLN", &interner);
        let entry_fk = fk(0x200001, "Output.esm", &interner);
        let race_fk = fk(0x110D23, "Output.esm", &interner);
        // Terminal NPC: no keyword, race only via RNAM.
        let mut entry_npc = empty_record("NPC_", &interner);
        push(&mut entry_npc, "RNAM", FieldValue::FormKey(race_fk));
        let mut race = empty_record("RACE", &interner);
        push(
            &mut race,
            "KWDA",
            kwda_list(&[ACTOR_TYPE_CREATURE_LOW24], "Fallout4.esm", &interner),
        );

        let resolve = |asked: FormKey| -> Option<Record> {
            if asked == lvln_fk {
                Some(lvln_rec.clone())
            } else if asked == entry_fk {
                Some(entry_npc.clone())
            } else if asked == race_fk {
                Some(race.clone())
            } else {
                None
            }
        };
        let lvln = |asked: FormKey| {
            if asked == lvln_fk {
                vec![entry_fk]
            } else {
                Vec::new()
            }
        };
        assert_eq!(
            npc_is_creature_following_template(&npc, &resolve, &lvln),
            CreatureVerdict::Creature,
            "must reach the race keyword through LVLN entry's RNAM"
        );
    }

    #[test]
    fn non_traits_npc_uses_literal_rnam_in_template_aware_path() {
        // UseTraits CLEAR → classify off RNAM even in the template-aware fn.
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(&mut npc, "ACBS", acbs_with_template_flags(0)); // no UseTraits
        let race_fk = fk(0x00D191, "Output.esm", &interner);
        push(&mut npc, "RNAM", FieldValue::FormKey(race_fk));
        let mut race = empty_record("RACE", &interner);
        push(
            &mut race,
            "KWDA",
            kwda_list(&[ACTOR_TYPE_CREATURE_LOW24], "Fallout4.esm", &interner),
        );
        let resolve = |asked: FormKey| {
            if asked == race_fk {
                Some(race.clone())
            } else {
                None
            }
        };
        let lvln = |_: FormKey| Vec::new();
        assert_eq!(
            npc_is_creature_following_template(&npc, &resolve, &lvln),
            CreatureVerdict::Creature
        );
    }

    #[test]
    fn template_flags_read_at_offset_14() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(&mut npc, "ACBS", acbs_with_template_flags(0x02b7));
        assert_eq!(npc_acbs_template_flags(&npc), Some(0x02b7));
        assert!(npc_inherits_traits_from_template(&npc)); // 0x02b7 & 0x0001 != 0
        let mut npc2 = empty_record("NPC_", &interner);
        push(&mut npc2, "ACBS", acbs_with_template_flags(0x0230));
        assert!(!npc_inherits_traits_from_template(&npc2)); // 0x0230 & 0x0001 == 0
    }
}
