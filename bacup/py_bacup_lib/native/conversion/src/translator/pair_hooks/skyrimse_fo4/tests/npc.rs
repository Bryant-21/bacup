fn npc_field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).unwrap(),
        value,
    }
}

fn npc_form_key(record: &Record, signature: &str) -> FormKey {
    record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == signature)
        .and_then(|field| match field.value {
            FieldValue::FormKey(form_key) => Some(form_key),
            _ => None,
        })
        .unwrap()
}

#[test]
fn skyrim_human_npc_uses_fo4_female_voice_and_class_and_drops_behavior_dependencies() {
    let interner = StringInterner::new();
    let source = interner.intern("Skyrim.esm");
    let fallout4 = interner.intern("Fallout4.esm");
    let flags = interner.intern("Flags");
    let female = interner.intern("Female");
    let mut record = Record::new(
        SigCode::from_str("NPC_").unwrap(),
        FormKey {
            local: 0x0A2C8E,
            plugin: source,
        },
    );
    record.fields.extend([
        npc_field(
            "ACBS",
            FieldValue::Struct(vec![(
                flags,
                FieldValue::List(vec![FieldValue::String(female)]),
            )]),
        ),
        npc_field(
            "RNAM",
            FieldValue::FormKey(FormKey {
                local: 0x013746,
                plugin: fallout4,
            }),
        ),
        npc_field(
            "VTCK",
            FieldValue::FormKey(FormKey {
                local: 0x013B00,
                plugin: source,
            }),
        ),
        npc_field(
            "CNAM",
            FieldValue::FormKey(FormKey {
                local: 0x013200,
                plugin: source,
            }),
        ),
        npc_field(
            "PKID",
            FieldValue::FormKey(FormKey {
                local: 0x013300,
                plugin: source,
            }),
        ),
        npc_field(
            "SPLO",
            FieldValue::FormKey(FormKey {
                local: 0x013400,
                plugin: source,
            }),
        ),
    ]);

    npc::normalize_skyrim_npc(&mut record, &interner);

    assert_eq!(
        npc_form_key(&record, "VTCK"),
        FormKey {
            local: 0x013ADD,
            plugin: fallout4,
        }
    );
    assert_eq!(
        npc_form_key(&record, "CNAM"),
        FormKey {
            local: 0x01326B,
            plugin: fallout4,
        }
    );
    assert!(
        record
            .fields
            .iter()
            .all(|field| !matches!(field.sig.as_str(), "PKID" | "SPLO"))
    );
}

#[test]
fn skyrim_child_npc_uses_matching_fo4_child_voice() {
    let interner = StringInterner::new();
    let source = interner.intern("Skyrim.esm");
    let fallout4 = interner.intern("Fallout4.esm");
    let mut record = Record::new(
        SigCode::from_str("NPC_").unwrap(),
        FormKey {
            local: 0x0A2C8E,
            plugin: source,
        },
    );
    record.fields.extend([
        npc_field("ACBS", FieldValue::Bytes([1, 0, 0, 0].as_slice().into())),
        npc_field(
            "RNAM",
            FieldValue::FormKey(FormKey {
                local: 0x11D83F,
                plugin: fallout4,
            }),
        ),
        npc_field(
            "VTCK",
            FieldValue::FormKey(FormKey {
                local: 0x013B00,
                plugin: source,
            }),
        ),
    ]);

    npc::normalize_skyrim_npc(&mut record, &interner);

    assert_eq!(
        npc_form_key(&record, "VTCK"),
        FormKey {
            local: 0x013AE9,
            plugin: fallout4,
        }
    );
}
