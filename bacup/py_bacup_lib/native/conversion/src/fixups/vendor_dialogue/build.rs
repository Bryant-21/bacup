//! Build the synthesized vendor-dialogue gate FACT and clone-append an NPC
//! faction-membership entry.

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::sym::StringInterner;
use smallvec::SmallVec;

/// Minimal FO4 gate FACT: `EDID` + `DATA`(flags = 0). Modeled on the vanilla
/// `DialogueMerchantsFaction` (a dialogue-only faction) without its incidental
/// `CRVA`/`VENV`. `DATA` is the u32 faction-flags word; 0 = a plain, non-crime,
/// NPC-visible faction — exactly what a dialogue gate needs.
pub fn build_vendor_dialogue_faction(
    fact_fk: FormKey,
    editor_id: &str,
    interner: &StringInterner,
) -> Record {
    let eid_sym = interner.intern(editor_id);
    let mut fields: SmallVec<[FieldEntry; 8]> = SmallVec::new();
    fields.push(FieldEntry {
        sig: SubrecordSig::from_str("EDID").expect("EDID"),
        value: FieldValue::String(eid_sym),
    });
    fields.push(FieldEntry {
        sig: SubrecordSig::from_str("DATA").expect("DATA"),
        value: FieldValue::Uint(0),
    });
    Record {
        sig: SigCode::from_str("FACT").expect("FACT"),
        form_key: fact_fk,
        eid: Some(eid_sym),
        flags: RecordFlags::empty(),
        fields,
        warnings: SmallVec::new(),
    }
}

/// Every faction an NPC belongs to: the FormKey member of each `SNAM` Factions
/// struct (FO4 NPC_ faction membership is one `SNAM` per faction).
pub fn npc_faction_formkeys(record: &Record) -> Vec<FormKey> {
    record
        .fields
        .iter()
        .filter(|f| f.sig.as_str() == "SNAM")
        .filter_map(|f| struct_formkey(&f.value))
        .collect()
}

fn struct_formkey(v: &FieldValue) -> Option<FormKey> {
    match v {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Struct(members) => members.iter().find_map(|(_, mv)| match mv {
            FieldValue::FormKey(fk) => Some(*fk),
            _ => None,
        }),
        _ => None,
    }
}

/// Append a faction-membership entry to `record`, cloning the shape of an
/// existing `SNAM` (Factions) entry so the struct layout always matches the
/// live schema, then swapping in `faction_fk` at rank 0. Returns false if the
/// NPC already belongs to `faction_fk` or has no `SNAM` template to clone.
pub fn enroll_npc_in_faction(record: &mut Record, faction_fk: FormKey) -> bool {
    if npc_faction_formkeys(record)
        .iter()
        .any(|f| *f == faction_fk)
    {
        return false;
    }
    let Some(template) = record
        .fields
        .iter()
        .find(|f| f.sig.as_str() == "SNAM")
        .cloned()
    else {
        return false;
    };
    let mut entry = template;
    set_faction_and_zero_rank(&mut entry.value, faction_fk);
    // Keep the SNAM block contiguous: insert right after the last existing one.
    let insert_at = record
        .fields
        .iter()
        .rposition(|f| f.sig.as_str() == "SNAM")
        .map(|i| i + 1)
        .unwrap_or(record.fields.len());
    record.fields.insert(insert_at, entry);
    true
}

fn set_faction_and_zero_rank(v: &mut FieldValue, fk: FormKey) {
    match v {
        FieldValue::FormKey(slot) => *slot = fk,
        FieldValue::Struct(members) => {
            for (_, mv) in members.iter_mut() {
                match mv {
                    FieldValue::FormKey(slot) => *slot = fk,
                    FieldValue::Int(n) => *n = 0,
                    FieldValue::Uint(n) => *n = 0,
                    FieldValue::Bytes(b) => b.iter_mut().for_each(|x| *x = 0),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::Sym;

    fn fk(local: u32, plugin: Sym) -> FormKey {
        FormKey { local, plugin }
    }

    fn snam_entry(i: &StringInterner, faction: FormKey, rank: i64) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Struct(vec![
                (i.intern("faction"), FieldValue::FormKey(faction)),
                (i.intern("rank"), FieldValue::Int(rank)),
            ]),
        }
    }

    #[test]
    fn faction_has_edid_and_zero_data() {
        let i = StringInterner::new();
        let out = i.intern("SeventySix.esm");
        let r = build_vendor_dialogue_faction(fk(0x080A01, out), "B21_VendorDialogueFaction", &i);
        assert_eq!(r.sig.as_str(), "FACT");
        assert_eq!(
            r.eid.and_then(|s| i.resolve(s)),
            Some("B21_VendorDialogueFaction")
        );
        let data = r.fields.iter().find(|f| f.sig.as_str() == "DATA").unwrap();
        assert_eq!(data.value, FieldValue::Uint(0));
    }

    #[test]
    fn extracts_membership_formkeys() {
        let i = StringInterner::new();
        let f4 = i.intern("Fallout4.esm");
        let mut r = Record::new(SigCode::from_str("NPC_").unwrap(), fk(1, f4));
        r.fields.push(snam_entry(&i, fk(0x0130B7, f4), 0));
        r.fields.push(snam_entry(&i, fk(0x4124AA, f4), 1));
        let members = npc_faction_formkeys(&r);
        assert_eq!(members.len(), 2);
        assert!(members.iter().any(|m| m.local == 0x4124AA));
    }

    #[test]
    fn enroll_clones_shape_swaps_faction_zero_rank() {
        let i = StringInterner::new();
        let out = i.intern("SeventySix.esm");
        let gate = fk(0x080A01, out);
        let mut r = Record::new(SigCode::from_str("NPC_").unwrap(), fk(1, out));
        r.fields.push(snam_entry(&i, fk(0x4124AA, out), 7));
        assert!(enroll_npc_in_faction(&mut r, gate));
        let members = npc_faction_formkeys(&r);
        assert!(members.iter().any(|m| *m == gate));
        // appended entry mirrors the template struct shape with rank zeroed
        let new = r
            .fields
            .iter()
            .filter(|f| f.sig.as_str() == "SNAM")
            .find(|f| struct_formkey(&f.value) == Some(gate))
            .unwrap();
        let FieldValue::Struct(members) = &new.value else {
            panic!()
        };
        assert!(members.iter().any(|(_, v)| matches!(v, FieldValue::Int(0))));
    }

    #[test]
    fn enroll_is_idempotent() {
        let i = StringInterner::new();
        let out = i.intern("SeventySix.esm");
        let gate = fk(0x080A01, out);
        let mut r = Record::new(SigCode::from_str("NPC_").unwrap(), fk(1, out));
        r.fields.push(snam_entry(&i, fk(0x4124AA, out), 0));
        assert!(enroll_npc_in_faction(&mut r, gate));
        assert!(!enroll_npc_in_faction(&mut r, gate)); // already a member
        assert_eq!(
            npc_faction_formkeys(&r)
                .iter()
                .filter(|m| **m == gate)
                .count(),
            1
        );
    }

    #[test]
    fn enroll_noop_without_snam_template() {
        let i = StringInterner::new();
        let out = i.intern("SeventySix.esm");
        let mut r = Record::new(SigCode::from_str("NPC_").unwrap(), fk(1, out));
        assert!(!enroll_npc_in_faction(&mut r, fk(0x080A01, out)));
    }
}
