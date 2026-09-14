//! Per-record "is this a creature?" predicate.
//!
//! Lets the creature fixups run whole-plugin while touching only creatures.
//! Several mutate unconditionally (add `ActorTypeCreature` and combat perks,
//! strip INAM death items) and would corrupt every human NPC. Pure record
//! inspection; RACE resolution is a caller-supplied closure.
//!
//! An actor is a creature iff `ActorTypeCreature` (`Fallout4.esm:013795`) is in
//! its own `KWDA` or, more commonly, its RACE's. Keywords match on the low 24
//! bits because the master byte differs between converted (07) and
//! inherited-vanilla (00) plugins.
//!
//! Traits-template NPCs (ACBS `UseTraits`) inherit RACE from `TPLT`, so their
//! `RNAM` is irrelevant (e.g. the Gulper whose RNAM became a STAT but whose race
//! comes from `LCharGulper`). [`npc_is_creature_following_template`] walks the
//! template chain; [`npc_is_creature`] reads only the literal `RNAM`.

use crate::ids::FormKey;
use crate::ids::SubrecordSig;
use crate::record::{FieldValue, Record};

/// `Fallout4.esm:013795` — the `ActorTypeCreature` keyword. Master byte 0 is
/// Fallout4.esm in every FO4-target plugin; we match on the low 24 bits.
pub const ACTOR_TYPE_CREATURE_LOW24: u32 = 0x00_013795;

/// `Fallout4.esm:013794` — the `ActorTypeNPC` keyword (humanoids). Used only as
/// a tie-breaker / negative signal; presence of ActorTypeCreature wins.
pub const ACTOR_TYPE_NPC_LOW24: u32 = 0x00_013794;

/// `Fallout4.esm:06D7B6` — `ActorTypeSuperMutant`.
pub const ACTOR_TYPE_SUPER_MUTANT_LOW24: u32 = 0x00_06D7B6;

/// `ActorTypeHumanlike` — an FO76-authored keyword with no Fallout4.esm
/// equivalent, so this id is FO76-local (`SeventySix.esm:5F1198`). Whole-plugin
/// conversion preserves local ids, so the low 24 bits are stable in the output.
pub const ACTOR_TYPE_HUMANLIKE_LOW24: u32 = 0x00_5F1198;

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

/// Whether `record` (a RACE) wields real weapons despite FO76 tagging it
/// `ActorTypeCreature`.
///
/// FO76 tags mole miners, super mutants and Zetans as creatures, so "is a
/// creature" cannot mean "cannot hold a weapon" when stripping `RACE.VNAM`
/// equipment-type bits. `ActorTypeHumanlike` covers mole miners, Zetans and the
/// Rust King; `ActorTypeSuperMutant` covers the shielded/ghoul mutant races.
pub fn record_is_armed_humanoid(record: &Record) -> bool {
    let mut found = false;
    for_each_kwda_low24(record, |kw| {
        if kw == ACTOR_TYPE_HUMANLIKE_LOW24 || kw == ACTOR_TYPE_SUPER_MUTANT_LOW24 {
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

/// Classify an NPC by its own `KWDA` actor-type keyword, else by its `RNAM`
/// race's (`ActorTypeCreature` wins over `ActorTypeNPC`); `Unknown` when none is
/// reachable. `resolve_race` returns `None` for a race that can't be read
/// (dropped, cross-master, decode error).
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
/// 1. The NPC's own actor-type keyword decides if present.
/// 2. With `UseTraits` set and a resolvable `TPLT`, classify the template (NPC_
///    recursively, RACE by keyword, LVLN as Creature if any entry is one).
/// 3. Otherwise use the literal `RNAM` race.
/// 4. Anything unresolved is `Unknown`, so a human is never mis-flagged.
///
/// `resolve` decodes a RACE / NPC_ / LVLN or returns `None`; `lvln_entry_npcs`
/// yields a LVLN's NPC_ entries (empty for a non-LVLN or unreadable record).
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
                        return classify_lvln_following_template(
                            tplt_fk,
                            resolve,
                            lvln_entry_npcs,
                            depth - 1,
                        );
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

fn classify_lvln_following_template(
    lvln_fk: FormKey,
    resolve: &impl Fn(FormKey) -> Option<Record>,
    lvln_entry_npcs: &impl Fn(FormKey) -> Vec<FormKey>,
    depth: u32,
) -> CreatureVerdict {
    if depth == 0 {
        return CreatureVerdict::Unknown;
    }

    let mut saw_not_creature = false;
    for entry_fk in lvln_entry_npcs(lvln_fk) {
        let Some(entry) = resolve(entry_fk) else {
            continue;
        };
        let verdict = match entry.sig.as_str() {
            "NPC_" => {
                classify_actor_following_template(&entry, resolve, lvln_entry_npcs, depth - 1)
            }
            "RACE" => record_actor_type(&entry),
            "LVLN" => {
                classify_lvln_following_template(entry_fk, resolve, lvln_entry_npcs, depth - 1)
            }
            _ => CreatureVerdict::Unknown,
        };
        match verdict {
            CreatureVerdict::Creature => return CreatureVerdict::Creature,
            CreatureVerdict::NotCreature => saw_not_creature = true,
            CreatureVerdict::Unknown => {}
        }
    }

    if saw_not_creature {
        CreatureVerdict::NotCreature
    } else {
        CreatureVerdict::Unknown
    }
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
    fn traits_template_via_nested_lvln_classifies_terminal_creature() {
        let interner = StringInterner::new();
        let mut npc = empty_record("NPC_", &interner);
        push(
            &mut npc,
            "ACBS",
            acbs_with_template_flags(ACBS_TEMPLATE_FLAG_USE_TRAITS),
        );
        let parent_lvln_fk = fk(0x4FB1BA, "Output.esm", &interner);
        let branch_lvln_fk = fk(0x523CF7, "Output.esm", &interner);
        let entry_fk = fk(0x490772, "Output.esm", &interner);
        push(&mut npc, "TPLT", FieldValue::FormKey(parent_lvln_fk));

        let parent_lvln = empty_record("LVLN", &interner);
        let branch_lvln = empty_record("LVLN", &interner);
        let mut entry_npc = empty_record("NPC_", &interner);
        push(
            &mut entry_npc,
            "KWDA",
            kwda_list(&[ACTOR_TYPE_CREATURE_LOW24], "Fallout4.esm", &interner),
        );
        let resolve = |asked: FormKey| -> Option<Record> {
            if asked == parent_lvln_fk {
                Some(parent_lvln.clone())
            } else if asked == branch_lvln_fk {
                Some(branch_lvln.clone())
            } else if asked == entry_fk {
                Some(entry_npc.clone())
            } else {
                None
            }
        };
        let lvln = |asked: FormKey| {
            if asked == parent_lvln_fk {
                vec![branch_lvln_fk]
            } else if asked == branch_lvln_fk {
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
