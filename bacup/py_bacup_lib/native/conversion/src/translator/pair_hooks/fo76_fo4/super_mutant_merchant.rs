use super::*;
use crate::sym::StringInterner;

const MERCHANT_QUEST_FORM_ID: u32 = 0x0016_AECD;
const MERCHANT_QUEST_EID: &str = "gq_supermutantmerchant";
const FO4_HAS_KEYWORD_FUNCTION: u16 = 560;
const FO4_HAS_REF_TYPE_FUNCTION: u16 = 561;

/// Grahm's destination aliases and the selector FO76 searched each one with:
/// (alias, ALFF form, FO4 condition that keeps the same restriction).
/// WorkshopRefType and LocationCenterMarker are LCRTs; CampWorkshopKeyword is a KYWD.
const DESTINATION_SELECTORS: &[(u32, u32, u16)] = &[
    (2, 0x0002_34E9, FO4_HAS_REF_TYPE_FUNCTION),
    (3, 0x0002_34E9, FO4_HAS_REF_TYPE_FUNCTION),
    (11, 0x0005_231A, FO4_HAS_KEYWORD_FUNCTION),
    (13, 0x0001_F40F, FO4_HAS_REF_TYPE_FUNCTION),
];

fn alias_range(record: &Record, id: u32) -> Option<(usize, usize)> {
    let start = record.fields.iter().position(|field| {
        field.sig.0 == *b"ALST" && field_value_to_u32(&field.value) == Some(id)
    })?;
    let end = record.fields[start..]
        .iter()
        .position(|field| field.sig.0 == *b"ALED")?
        + start;
    Some((start, end))
}

fn selector_condition(function: u16, form: u32) -> FieldEntry {
    let mut condition = [0u8; 32];
    condition[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    condition[8..10].copy_from_slice(&function.to_le_bytes());
    condition[12..16].copy_from_slice(&form.to_le_bytes());
    condition[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
    FieldEntry {
        sig: SubrecordSig(*b"CTDA"),
        value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&condition)),
    }
}

/// FO76 fills Grahm's possible stops with a scope-free `ALFF` search that FO4
/// lacks, so the generic strip leaves conditions that match any persistent ref
/// in Appalachia. Keep each search's restriction as an FO4 condition instead.
pub(super) fn restrict_destination_aliases(interner: &StringInterner, record: &mut Record) {
    if record.sig.0 != *b"QUST"
        || record.form_key.local != MERCHANT_QUEST_FORM_ID
        || !interner
            .resolve(record.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
        || qust_eid_lower(interner, record).as_deref() != Some(MERCHANT_QUEST_EID)
    {
        return;
    }
    let mut insertions = Vec::new();
    for &(alias_id, selector, function) in DESTINATION_SELECTORS {
        let Some((start, end)) = alias_range(record, alias_id) else {
            return;
        };
        let alias = &record.fields[start..end];
        let selectors: Vec<_> = alias.iter().filter(|field| field.sig.0 == *b"ALFF").collect();
        let [only] = selectors.as_slice() else {
            return;
        };
        if !matches!(only.value, FieldValue::FormKey(key)
            if key.plugin == record.form_key.plugin && key.local == selector)
        {
            return;
        }
        let Some(first_condition) = alias.iter().position(|field| field.sig.0 == *b"CTDA") else {
            return;
        };
        insertions.push((start + first_condition, selector_condition(function, selector)));
    }
    for (index, condition) in insertions.into_iter().rev() {
        record.fields.insert(index, condition);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AuthoringSchema;
    use crate::translator::{Game, TranslateResult, Translator};

    fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }
    }

    fn raw(hex_text: &str) -> FieldValue {
        FieldValue::Bytes(smallvec::SmallVec::from_slice(&hex::decode(hex_text).unwrap()))
    }

    fn fo76_key(interner: &StringInterner, local: u32) -> FieldValue {
        FieldValue::FormKey(FormKey {
            local,
            plugin: interner.intern(FO76_MASTER_NAME),
        })
    }

    // GetInWorldspace(Appalachia) and GetDistance(Merchant) >= 15000, as FO76 ships them.
    const IN_APPALACHIA: &str = "000000000000803F3601000015DA2500000000000000000000000000FFFFFFFF";
    const FAR_FROM_MERCHANT: &str = "6200000000606A460100000001000000000000000000000000000000FFFFFFFF";

    fn merchant_quest(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey {
                local: MERCHANT_QUEST_FORM_ID,
                plugin: interner.intern(FO76_MASTER_NAME),
            },
        );
        record.eid = Some(interner.intern("GQ_SuperMutantMerchant"));
        record.fields.push(field(b"ANAM", FieldValue::Uint(14)));
        record.fields.extend([
            field(b"ALST", FieldValue::Uint(1)),
            field(b"ALID", raw("4D65726368616E7400")),
            field(b"FNAM", FieldValue::Uint(0x40)),
            field(b"ALUA", fo76_key(interner, 0x0017_F096)),
            field(b"ALED", FieldValue::None),
        ]);
        for &(alias_id, selector, _) in DESTINATION_SELECTORS {
            record.fields.extend([
                field(b"ALST", FieldValue::Uint(u64::from(alias_id))),
                field(b"ALID", raw("44657374696E6174696F6E00")),
                field(b"FNAM", FieldValue::Uint(2)),
                field(b"CTDA", raw(IN_APPALACHIA)),
                field(b"CTDA", raw(FAR_FROM_MERCHANT)),
                field(b"ALFF", fo76_key(interner, selector)),
                field(b"ALED", FieldValue::None),
            ]);
        }
        record
    }

    fn conditions(record: &Record, alias_id: u32) -> Vec<FieldValue> {
        let (start, end) = alias_range(record, alias_id).expect("alias");
        record.fields[start..end]
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .map(|field| field.value.clone())
            .collect()
    }

    #[test]
    fn destination_selectors_survive_translation_as_fo4_conditions() {
        let interner = StringInterner::new();
        let mut record = merchant_quest(&interner);
        let merchant = alias_range(&record, 1).map(|(start, end)| record.fields[start..end].to_vec());
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        translator.pre_translate(&mut PairCtx::new(&interner), &mut record).unwrap();
        assert_eq!(
            alias_range(&record, 1).map(|(start, end)| record.fields[start..end].to_vec()),
            merchant,
            "the merchant alias is untouched"
        );
        let once = record.fields.clone();
        translator.pre_translate(&mut PairCtx::new(&interner), &mut record).unwrap();
        assert_eq!(record.fields, once, "repair must be idempotent");
        assert!(!record.fields.iter().any(|field| field.sig.0 == *b"ALFF"));

        let TranslateResult::Translated(record) = translator.translate(&record, &interner) else {
            panic!("merchant quest must translate");
        };
        let fo76 = AuthoringSchema::for_game("fo76").unwrap();
        let fo4 = AuthoringSchema::for_game("fo4").unwrap();
        let normalizer = crate::target_normalize::TargetRecordNormalizer {
            target_schema: &fo4,
            source_record_def: fo76.record_def("QUST"),
            interner: Some(&interner),
        };
        let crate::target_normalize::TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("merchant quest must survive normalization");
        };
        for &(alias_id, selector, function) in DESTINATION_SELECTORS {
            let conditions = conditions(&record, alias_id);
            assert_eq!(conditions.len(), 3, "alias {alias_id} keeps its search restriction");
            let FieldValue::Bytes(filter) = &conditions[0] else { panic!("raw CTDA") };
            assert_eq!(filter[0], 0, "AND, equal, subject");
            assert_eq!(f32::from_le_bytes(filter[4..8].try_into().unwrap()), 1.0);
            assert_eq!(u16::from_le_bytes(filter[8..10].try_into().unwrap()), function);
            assert_eq!(u32::from_le_bytes(filter[12..16].try_into().unwrap()), selector);
            assert_eq!(&filter[16..28], &[0; 12]);
            assert_eq!(u32::from_le_bytes(filter[28..32].try_into().unwrap()), u32::MAX);
            assert_eq!(conditions[1], raw(IN_APPALACHIA));
            assert_eq!(conditions[2], raw(FAR_FROM_MERCHANT));
        }
    }

    #[test]
    fn destination_repair_requires_the_exact_source_contract() {
        let interner = StringInterner::new();
        for mutation in 0..5 {
            let mut record = merchant_quest(&interner);
            match mutation {
                0 => record.form_key.local += 1,
                1 => record.form_key.plugin = interner.intern("Other.esm"),
                2 => record.eid = Some(interner.intern("GQ_SuperMutantMerchant_Copy")),
                3 => {
                    record
                        .fields
                        .iter_mut()
                        .rfind(|field| field.sig.0 == *b"ALFF")
                        .unwrap()
                        .value = fo76_key(&interner, 0x0002_34E9)
                }
                _ => {
                    let (start, end) = alias_range(&record, 11).unwrap();
                    record.fields.drain(start..=end);
                }
            }
            let before = record.fields.clone();
            restrict_destination_aliases(&interner, &mut record);
            assert_eq!(record.fields, before, "mutation {mutation} must leave the quest alone");
        }
    }
}
