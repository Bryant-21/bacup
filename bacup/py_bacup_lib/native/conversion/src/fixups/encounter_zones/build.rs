//! Build synthesized ECZN records and rewritten LCTN keyword fields.

use crate::fixups::encounter_zones::model::{
    KW_CLEARABLE, KW_SETTLEMENT, KW_WORKSHOP, KW_WORKSHOP_SETTLEMENT, WorkshopClass,
};
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::sym::{StringInterner, Sym};
use smallvec::SmallVec;

/// Build an FO4 `ECZN`. DATA = Struct(owner FK, location FK, rank/min/flags/max
/// as single-byte Bytes) → encodes to exactly 12 bytes (the encoder concatenates
/// struct members; FormKey→4 bytes master-resolved, Bytes→verbatim).
pub fn build_eczn_record(
    eczn_fk: FormKey,
    editor_id: &str,
    location: FormKey,
    min_level: i8,
    flags: u8,
    max_level: i8,
    interner: &StringInterner,
) -> Record {
    let eid_sym = interner.intern(editor_id);
    let null_owner = FormKey {
        local: 0,
        plugin: location.plugin,
    };
    let one = |b: u8| -> FieldValue { FieldValue::Bytes(SmallVec::from_slice(&[b])) };
    let data = FieldValue::Struct(vec![
        (interner.intern("owner"), FieldValue::FormKey(null_owner)),
        (interner.intern("location"), FieldValue::FormKey(location)),
        (interner.intern("rank"), one(0)),
        (interner.intern("min_level"), one(min_level as u8)),
        (interner.intern("flags"), one(flags)),
        (interner.intern("max_level"), one(max_level as u8)),
    ]);
    let mut fields: SmallVec<[FieldEntry; 8]> = SmallVec::new();
    fields.push(FieldEntry {
        sig: SubrecordSig::from_str("EDID").expect("EDID"),
        value: FieldValue::String(eid_sym),
    });
    fields.push(FieldEntry {
        sig: SubrecordSig::from_str("DATA").expect("DATA"),
        value: data,
    });
    Record {
        sig: SigCode::from_str("ECZN").expect("ECZN"),
        form_key: eczn_fk,
        eid: Some(eid_sym),
        flags: RecordFlags::empty(),
        fields,
        warnings: SmallVec::new(),
    }
}

/// Rewrite a workshop `LCTN`'s `KWDA`/`KSIZ` to the FO4 settlement contract:
/// drop the FO76-only keyword FormKeys (`drop_fks`), keep everything else, and
/// ensure the class-required keywords are present as `Fallout4.esm` references.
pub fn rebuild_keyword_fields(
    record: &mut Record,
    class: WorkshopClass,
    drop_fks: &[FormKey],
    fallout4: Sym,
    _interner: &StringInterner,
) {
    let required: &[u32] = match class {
        WorkshopClass::Settlement => &[
            KW_WORKSHOP,
            KW_WORKSHOP_SETTLEMENT,
            KW_SETTLEMENT,
            KW_CLEARABLE,
        ],
        WorkshopClass::Shelter => &[KW_WORKSHOP, KW_CLEARABLE],
        WorkshopClass::NonWorkshop => return,
    };
    let mut kept: Vec<FormKey> = Vec::new();
    for f in &record.fields {
        if f.sig.as_str() != "KWDA" {
            continue;
        }
        if let FieldValue::List(items) = &f.value {
            for it in items {
                if let FieldValue::FormKey(fk) = it {
                    if drop_fks
                        .iter()
                        .any(|d| d.local == fk.local && d.plugin == fk.plugin)
                    {
                        continue;
                    }
                    kept.push(*fk);
                }
            }
        }
    }
    for req in required {
        if !kept
            .iter()
            .any(|fk| fk.local == *req && fk.plugin == fallout4)
        {
            kept.push(FormKey {
                local: *req,
                plugin: fallout4,
            });
        }
    }
    let count = kept.len() as u64;
    let kwda = FieldValue::List(kept.into_iter().map(FieldValue::FormKey).collect());
    if let Some(f) = record.fields.iter_mut().find(|f| f.sig.as_str() == "KWDA") {
        f.value = kwda;
    } else {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KWDA").expect("KWDA"),
            value: kwda,
        });
    }
    if let Some(f) = record.fields.iter_mut().find(|f| f.sig.as_str() == "KSIZ") {
        f.value = FieldValue::Uint(count);
    } else {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KSIZ").expect("KSIZ"),
            value: FieldValue::Uint(count),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eczn_has_edid_and_data() {
        let i = StringInterner::new();
        let out = i.intern("Converted.esp");
        let lctn = FormKey {
            local: 0x0989F5,
            plugin: out,
        };
        let eczn = FormKey {
            local: 0x000801,
            plugin: out,
        };
        let r = build_eczn_record(eczn, "WhitespringEncounterZone", lctn, 20, 9, 99, &i);
        assert_eq!(r.sig.as_str(), "ECZN");
        assert_eq!(
            r.eid.and_then(|s| i.resolve(s)).map(str::to_string),
            Some("WhitespringEncounterZone".into())
        );
        let data = r.fields.iter().find(|f| f.sig.as_str() == "DATA").unwrap();
        let FieldValue::Struct(fields) = &data.value else {
            panic!()
        };
        let get = |n: &str| {
            fields
                .iter()
                .find(|(s, _)| i.resolve(*s) == Some(n))
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("location"), Some(FieldValue::FormKey(lctn)));
        match get("owner") {
            Some(FieldValue::FormKey(fk)) => assert_eq!(fk.local, 0),
            _ => panic!(),
        }
        assert_eq!(
            get("min_level"),
            Some(FieldValue::Bytes(SmallVec::from_slice(&[20])))
        );
        assert_eq!(
            get("flags"),
            Some(FieldValue::Bytes(SmallVec::from_slice(&[9])))
        );
        assert_eq!(
            get("max_level"),
            Some(FieldValue::Bytes(SmallVec::from_slice(&[99])))
        );
    }

    #[test]
    fn settlement_adds_required_drops_fo76() {
        let i = StringInterner::new();
        let f4 = i.intern("Fallout4.esm");
        let out = i.intern("Converted.esp");
        let fk = |l, p| FormKey {
            local: l,
            plugin: p,
        };
        let mut r = Record::new(SigCode::from_str("LCTN").unwrap(), fk(0x063DC7, out));
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KSIZ").unwrap(),
            value: FieldValue::Uint(4),
        });
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KWDA").unwrap(),
            value: FieldValue::List(vec![
                FieldValue::FormKey(fk(KW_WORKSHOP, f4)),
                FieldValue::FormKey(fk(KW_CLEARABLE, f4)),
                FieldValue::FormKey(fk(0x05A001, out)),
                FieldValue::FormKey(fk(0x800001, out)),
            ]),
        });
        rebuild_keyword_fields(
            &mut r,
            WorkshopClass::Settlement,
            &[fk(0x800001, out)],
            f4,
            &i,
        );
        let FieldValue::List(items) = &r
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "KWDA")
            .unwrap()
            .value
        else {
            panic!()
        };
        let locals: Vec<(u32, _)> = items
            .iter()
            .map(|v| match v {
                FieldValue::FormKey(k) => (k.local, k.plugin),
                _ => panic!(),
            })
            .collect();
        assert!(!locals.iter().any(|(l, _)| *l == 0x800001));
        assert!(locals.iter().any(|(l, _)| *l == 0x05A001));
        for req in [
            KW_WORKSHOP,
            KW_CLEARABLE,
            KW_SETTLEMENT,
            KW_WORKSHOP_SETTLEMENT,
        ] {
            assert!(
                locals.iter().any(|(l, p)| *l == req && *p == f4),
                "missing {req:06X}"
            );
        }
        let ksiz = r.fields.iter().find(|f| f.sig.as_str() == "KSIZ").unwrap();
        assert_eq!(ksiz.value, FieldValue::Uint(locals.len() as u64));
    }

    #[test]
    fn shelter_never_adds_settlement() {
        let i = StringInterner::new();
        let f4 = i.intern("Fallout4.esm");
        let out = i.intern("Converted.esp");
        let fk = |l, p| FormKey {
            local: l,
            plugin: p,
        };
        let mut r = Record::new(SigCode::from_str("LCTN").unwrap(), fk(1, out));
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KSIZ").unwrap(),
            value: FieldValue::Uint(1),
        });
        r.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KWDA").unwrap(),
            value: FieldValue::List(vec![FieldValue::FormKey(fk(0x900001, out))]),
        });
        rebuild_keyword_fields(&mut r, WorkshopClass::Shelter, &[fk(0x900001, out)], f4, &i);
        let FieldValue::List(items) = &r
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "KWDA")
            .unwrap()
            .value
        else {
            panic!()
        };
        let locals: Vec<u32> = items
            .iter()
            .map(|v| match v {
                FieldValue::FormKey(k) => k.local,
                _ => panic!(),
            })
            .collect();
        assert!(locals.contains(&KW_WORKSHOP) && locals.contains(&KW_CLEARABLE));
        assert!(!locals.contains(&KW_WORKSHOP_SETTLEMENT) && !locals.contains(&KW_SETTLEMENT));
        assert!(!locals.contains(&0x900001));
    }
}
