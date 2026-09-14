use conversion_native::ids::{FormKey, SigCode, SubrecordSig};
use conversion_native::record::{FieldEntry, FieldValue, Record};
use conversion_native::sym::{StringInterner, Sym};
use conversion_native::translator::pair_hook::PairCtx;
use conversion_native::translator::target_hook::TargetCtx;
use conversion_native::translator::{Game, TranslateResult, Translator};

// Exact W05_MQR_204P and Vault79EntranceLocation_Exterior topology read from
// both the FO76 source plugin and the current converted master.
const QUEST: u32 = 0x0053_5E55;
const KEYPAD_ALIAS: u32 = 36;
const ENTRANCE_ALIAS: u32 = 38;
const KEYPAD_REF: u32 = 0x0056_BA45;
const ENTRANCE_REF: u32 = 0x0040_A82C;
const KEYPAD_LCRT: u32 = 0x0059_7A63;
const ENTRANCE_LCRT: u32 = 0x0059_7A64;
const VAULT79_EXTERIOR_LOCATION: u32 = 0x0058_D8C3;
const APPALACHIA_WORLD: u32 = 0x0025_DA15;

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).unwrap(),
        value,
    }
}

fn form(plugin: Sym, local: u32) -> FieldValue {
    FieldValue::FormKey(FormKey { local, plugin })
}

fn translate(record: &Record, interner: &StringInterner) -> Record {
    let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
    let mut source = record.clone();
    translator
        .pre_translate(&mut PairCtx::new(interner), &mut source)
        .unwrap();
    let mut translated = match translator.translate(&source, interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("expected translated record, got {other:?}"),
    };
    translator
        .post_translate(&mut PairCtx::new(interner), &mut translated)
        .unwrap();
    translator
        .run_target_hook(&mut TargetCtx { interner }, &mut translated)
        .unwrap();
    translated
}

fn alias_fields(record: &Record, alias_id: u32) -> Vec<&FieldEntry> {
    let mut in_alias = false;
    let mut fields = Vec::new();
    for entry in &record.fields {
        if matches!(&entry.sig.0, b"ALST" | b"ALLS" | b"ALCS") {
            in_alias = entry.sig.0 == *b"ALST"
                && matches!(entry.value, FieldValue::Uint(value) if value as u32 == alias_id);
        }
        if in_alias {
            fields.push(entry);
        }
        if in_alias && entry.sig.0 == *b"ALED" {
            break;
        }
    }
    fields
}

fn field_form(fields: &[&FieldEntry], signature: &[u8; 4]) -> Option<FormKey> {
    fields.iter().find_map(|entry| {
        (entry.sig.0 == *signature).then(|| match entry.value {
            FieldValue::FormKey(form_key) => form_key,
            ref value => panic!("{} must be a FormKey, got {value:?}", entry.sig.as_str()),
        })
    })
}

fn vault79_quest(interner: &StringInterner) -> Record {
    let plugin = interner.intern("SeventySix.esm");
    let mut quest = Record::new(
        SigCode::from_str("QUST").unwrap(),
        FormKey {
            local: QUEST,
            plugin,
        },
    );
    quest.eid = Some(interner.intern("W05_MQR_204P"));
    quest.fields.extend([
        field("ANAM", FieldValue::Uint(43)),
        field("ALLS", FieldValue::Uint(37)),
        field("ALID", FieldValue::String(interner.intern("VaultExtLoc"))),
        field("FNAM", FieldValue::Uint(66_056)),
        field("ALFL", form(plugin, VAULT79_EXTERIOR_LOCATION)),
        field("ALED", FieldValue::None),
        field("ALST", FieldValue::Uint(KEYPAD_ALIAS.into())),
        field("ALID", FieldValue::String(interner.intern("Vault79Keypad"))),
        field("FNAM", FieldValue::Uint(664)),
        field("ALFA", FieldValue::Uint(37)),
        field("ALRT", form(plugin, KEYPAD_LCRT)),
        field("ALED", FieldValue::None),
        field("ALST", FieldValue::Uint(ENTRANCE_ALIAS.into())),
        field(
            "ALID",
            FieldValue::String(interner.intern("Vault79Entrance")),
        ),
        field("FNAM", FieldValue::Uint(664)),
        field("ALFA", FieldValue::Uint(37)),
        field("ALRT", form(plugin, ENTRANCE_LCRT)),
        field("ALED", FieldValue::None),
    ]);
    quest
}

fn lcsr_row(interner: &StringInterner, plugin: Sym, ref_type: u32, reference: u32) -> FieldValue {
    FieldValue::Struct(vec![
        (
            interner.intern("MasterSpecialReferencesLocRefType"),
            form(plugin, ref_type),
        ),
        (
            interner.intern("MasterSpecialReferencesRef"),
            form(plugin, reference),
        ),
        (
            interner.intern("MasterSpecialReferencesWorldCell"),
            form(plugin, APPALACHIA_WORLD),
        ),
        (
            interner.intern("MasterSpecialReferencesGridY"),
            FieldValue::Int(49),
        ),
        (
            interner.intern("MasterSpecialReferencesGridX"),
            FieldValue::Int(35),
        ),
    ])
}

fn row_form(row: &FieldValue, interner: &StringInterner, name: &str) -> Option<FormKey> {
    let FieldValue::Struct(members) = row else {
        return None;
    };
    members.iter().find_map(|(member_name, value)| {
        interner
            .resolve(*member_name)
            .is_some_and(|candidate| candidate == name)
            .then(|| match value {
                FieldValue::FormKey(form_key) => *form_key,
                other => panic!("{name} must be a FormKey, got {other:?}"),
            })
    })
}

#[test]
fn w05_mqr_204p_keeps_both_required_location_ref_alias_fills() {
    let interner = StringInterner::new();
    let source = vault79_quest(&interner);
    let translated = translate(&source, &interner);
    let plugin = interner.intern("SeventySix.esm");

    for (alias_id, ref_type) in [(KEYPAD_ALIAS, KEYPAD_LCRT), (ENTRANCE_ALIAS, ENTRANCE_LCRT)] {
        let fields = alias_fields(&translated, alias_id);
        assert_eq!(
            fields
                .iter()
                .find(|entry| entry.sig.0 == *b"ALFA")
                .and_then(|entry| match entry.value {
                    FieldValue::Uint(value) => Some(value as u32),
                    _ => None,
                }),
            Some(37)
        );
        assert_eq!(
            field_form(&fields, b"ALRT"),
            Some(FormKey {
                local: ref_type,
                plugin,
            })
        );
        assert!(fields.iter().all(|entry| entry.sig.0 != *b"ALFR"));
        let flags = fields
            .iter()
            .find(|entry| entry.sig.0 == *b"FNAM")
            .and_then(|entry| match entry.value {
                FieldValue::Uint(value) => Some(value as u32),
                _ => None,
            })
            .expect("alias flags");
        assert_eq!(flags & 0x2, 0, "both proven aliases remain required");
    }

    let translated_again = translate(&source, &interner);
    assert_eq!(translated.fields, translated_again.fields);
}

#[test]
fn vault79_location_master_special_references_survive_translation() {
    let interner = StringInterner::new();
    let plugin = interner.intern("SeventySix.esm");
    let mappings = [(KEYPAD_LCRT, KEYPAD_REF), (ENTRANCE_LCRT, ENTRANCE_REF)];

    let mut location = Record::new(
        SigCode::from_str("LCTN").unwrap(),
        FormKey {
            local: VAULT79_EXTERIOR_LOCATION,
            plugin,
        },
    );
    location.eid = Some(interner.intern("Vault79EntranceLocation_Exterior"));
    location.fields.push(field(
        "LCSR",
        FieldValue::List(
            mappings
                .iter()
                .map(|(ref_type, reference)| lcsr_row(&interner, plugin, *ref_type, *reference))
                .collect(),
        ),
    ));
    let translated_location = translate(&location, &interner);
    let lcsr = translated_location
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *b"LCSR")
        .expect("Vault 79 exterior master special references");
    let FieldValue::List(rows) = &lcsr.value else {
        panic!("LCSR must remain a row list");
    };
    for (ref_type, reference) in mappings {
        assert!(rows.iter().any(|row| {
            row_form(row, &interner, "MasterSpecialReferencesLocRefType")
                == Some(FormKey {
                    local: ref_type,
                    plugin,
                })
                && row_form(row, &interner, "MasterSpecialReferencesRef")
                    == Some(FormKey {
                        local: reference,
                        plugin,
                    })
        }));
    }
}
