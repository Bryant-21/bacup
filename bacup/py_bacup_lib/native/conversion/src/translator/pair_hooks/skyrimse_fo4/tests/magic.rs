fn jzargo_record(
    interner: &StringInterner,
    signature: &str,
    local: u32,
    editor_id: &str,
) -> Record {
    let mut record = Record::new(
        SigCode::from_str(signature).unwrap(),
        FormKey::parse(&format!("{local:06X}@Skyrim.esm"), interner).unwrap(),
    );
    record.eid = Some(interner.intern(editor_id));
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"EDID"),
        value: FieldValue::String(interner.intern(editor_id)),
    });
    record
}

fn jzargo_push(record: &mut Record, signature: [u8; 4], value: FieldValue) {
    record.fields.push(FieldEntry {
        sig: SubrecordSig(signature),
        value,
    });
}

fn jzargo_source_form(record: &Record, local: u32) -> FieldValue {
    FieldValue::FormKey(FormKey {
        local,
        plugin: record.form_key.plugin,
    })
}

fn jzargo_object<const N: usize>(
    interner: &StringInterner,
    values: [(&str, FieldValue); N],
) -> FieldValue {
    FieldValue::Struct(
        values
            .into_iter()
            .map(|(name, value)| (interner.intern(name), value))
            .collect(),
    )
}

fn jzargo_effect_source(
    interner: &StringInterner,
    signature: &str,
    local: u32,
    editor_id: &str,
    effect: u32,
) -> Record {
    let mut record = jzargo_record(interner, signature, local, editor_id);
    let effect = jzargo_source_form(&record, effect);
    jzargo_push(&mut record, *b"EFID", effect);
    jzargo_push(
        &mut record,
        *b"EFIT",
        jzargo_object(
            interner,
            [
                ("Magnitude", FieldValue::Float(99.0)),
                ("Duration", FieldValue::Uint(99)),
            ],
        ),
    );
    record
}

fn jzargo_scroll_source(interner: &StringInterner) -> Record {
    let mut record = jzargo_effect_source(interner, "SCRL", 0x0967E3, "MGRJzargo1Scroll", 0x097EE2);
    record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"EFIT")
        .unwrap()
        .value = jzargo_object(
        interner,
        [
            ("Magnitude", FieldValue::Float(10.0)),
            ("Duration", FieldValue::Uint(30)),
        ],
    );
    jzargo_push(
        &mut record,
        *b"OBND",
        jzargo_object(interner, [("ObjectBoundsX1", FieldValue::Int(-13))]),
    );
    jzargo_push(
        &mut record,
        *b"FULL",
        FieldValue::String(interner.intern("J'zargo's Flame Cloak Scroll")),
    );
    jzargo_push(
        &mut record,
        *b"MODL",
        FieldValue::String(interner.intern("Clutter\\Common\\Scroll06.nif")),
    );
    jzargo_push(
        &mut record,
        *b"MODT",
        FieldValue::Bytes(smallvec::smallvec![1, 2]),
    );
    jzargo_push(
        &mut record,
        *b"DATA",
        jzargo_object(
            interner,
            [
                ("Value", FieldValue::Uint(100)),
                ("Weight", FieldValue::Float(0.5)),
            ],
        ),
    );
    jzargo_push(
        &mut record,
        *b"SPIT",
        jzargo_object(
            interner,
            [
                ("BaseCost", FieldValue::Uint(147)),
                ("CastType", FieldValue::String(interner.intern("Scroll"))),
            ],
        ),
    );
    record
}

fn jzargo_outer_spell_source(interner: &StringInterner) -> Record {
    let mut record =
        jzargo_effect_source(interner, "SPEL", 0x097EDF, "MGRJzargoFlameCloak", 0x097EE2);
    let source_effect = jzargo_object(
        interner,
        [
            ("Magnitude", FieldValue::Float(10.0)),
            ("Duration", FieldValue::Uint(30)),
        ],
    );
    record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"EFIT")
        .unwrap()
        .value = source_effect;
    jzargo_push(
        &mut record,
        *b"SPIT",
        jzargo_object(
            interner,
            [(
                "CastType",
                FieldValue::String(interner.intern("FireAndForget")),
            )],
        ),
    );
    record
}

fn jzargo_inner_spell_source(interner: &StringInterner) -> Record {
    let mut record = jzargo_effect_source(
        interner,
        "SPEL",
        0x097EE0,
        "MGRJzargoFlameCloakDmg",
        0x097EE1,
    );
    let source_effect = jzargo_object(
        interner,
        [
            ("Magnitude", FieldValue::Float(5.0)),
            ("Duration", FieldValue::Uint(5)),
        ],
    );
    record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"EFIT")
        .unwrap()
        .value = source_effect;
    jzargo_push(
        &mut record,
        *b"SPIT",
        jzargo_object(
            interner,
            [
                (
                    "CastType",
                    FieldValue::String(interner.intern("Concentration")),
                ),
                ("TargetType", FieldValue::String(interner.intern("Aimed"))),
            ],
        ),
    );
    record
}

fn jzargo_outer_effect_source(interner: &StringInterner) -> Record {
    let mut record = jzargo_record(interner, "MGEF", 0x097EE2, "MGRJZargoFireCloakFFSelf");
    let associated_spell = jzargo_source_form(&record, 0x097EE0);
    jzargo_push(
        &mut record,
        *b"DATA",
        jzargo_object(
            interner,
            [
                (
                    "Flags",
                    FieldValue::List(
                        [
                            "Detrimental",
                            "NoArea",
                            "FXPersist",
                            "NoRecast",
                            "PowerAffectsMagnitude",
                            "NoDeathDispel",
                        ]
                        .into_iter()
                        .map(|flag| FieldValue::String(interner.intern(flag)))
                        .collect(),
                    ),
                ),
                ("AssocItem", associated_spell),
                ("Archtype", FieldValue::Uint(35)),
                (
                    "CastingType",
                    FieldValue::String(interner.intern("FireAndForget")),
                ),
            ],
        ),
    );
    record
}

fn jzargo_inner_effect_source(interner: &StringInterner) -> Record {
    let mut record = jzargo_record(interner, "MGEF", 0x097EE1, "MGRJzargoFireDamageConcAimed");
    jzargo_push(
        &mut record,
        *b"VMAD",
        FieldValue::Bytes(smallvec::smallvec![5, 0, 2, 0, 1, 0]),
    );
    let projectile = jzargo_source_form(&record, 0x012FCF);
    jzargo_push(
        &mut record,
        *b"DATA",
        jzargo_object(
            interner,
            [
                (
                    "Flags",
                    FieldValue::List(
                        [
                            "Hostile",
                            "Detrimental",
                            "NoArea",
                            "FXPersist",
                            "GoryVisuals",
                            "NoRecast",
                            "PowerAffectsMagnitude",
                            "NoDeathDispel",
                        ]
                        .into_iter()
                        .map(|flag| FieldValue::String(interner.intern(flag)))
                        .collect(),
                    ),
                ),
                (
                    "ResistValue",
                    FieldValue::String(interner.intern("ResistFire")),
                ),
                ("ActorValue", FieldValue::String(interner.intern("Health"))),
                ("Projectile", projectile),
                (
                    "CastingType",
                    FieldValue::String(interner.intern("Concentration")),
                ),
                ("Delivery", FieldValue::String(interner.intern("Aimed"))),
            ],
        ),
    );
    jzargo_push(
        &mut record,
        *b"SNDD",
        FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
    );
    record
}

fn jzargo_run(
    record: &mut Record,
    interner: &StringInterner,
) -> Result<MagicLoweringReceipt, MagicContractError> {
    let plan = classify_magic_component(record, interner)
        .plan()
        .cloned()
        .ok_or(MagicContractError::PlanDrift {
            form_key: record.form_key,
        })?;
    let receipt = lower_supported_magic_record(record, &plan, interner)?;
    normalize_standalone_vmad_for_fo4(record);
    Ok(receipt)
}

fn jzargo_run_default(record: &mut Record, interner: &StringInterner) -> HookResult {
    SkyrimSeFo4Hook.pre_translate(&mut PairCtx::new(interner), record)
}

fn jzargo_field<'a>(record: &'a Record, signature: [u8; 4]) -> &'a FieldValue {
    &record
        .fields
        .iter()
        .find(|field| field.sig.0 == signature)
        .unwrap_or_else(|| panic!("missing {}", std::str::from_utf8(&signature).unwrap()))
        .value
}

fn jzargo_named<'a>(
    record: &'a Record,
    signature: [u8; 4],
    name: &str,
    interner: &StringInterner,
) -> &'a FieldValue {
    let FieldValue::Struct(values) = jzargo_field(record, signature) else {
        panic!("expected structured field")
    };
    values
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
        .unwrap_or_else(|| panic!("missing {name}"))
}

fn jzargo_assert_form(value: &FieldValue, local: u32, plugin: &str, interner: &StringInterner) {
    let FieldValue::FormKey(form) = value else {
        panic!("expected FormKey, found {value:?}")
    };
    assert_eq!(form.local, local);
    assert_eq!(interner.resolve(form.plugin), Some(plugin));
}

#[test]
fn exact_source_fixture_matches_authoritative_aimed_concentration_shape() {
    let interner = StringInterner::new();
    let inner_spell = jzargo_inner_spell_source(&interner);
    let inner_effect = jzargo_inner_effect_source(&interner);

    assert_eq!(
        jzargo_named(&inner_spell, *b"SPIT", "CastType", &interner),
        &FieldValue::String(interner.intern("Concentration"))
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"SPIT", "TargetType", &interner),
        &FieldValue::String(interner.intern("Aimed"))
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"EFIT", "Duration", &interner),
        &FieldValue::Uint(5)
    );
    assert_eq!(
        jzargo_named(&inner_effect, *b"DATA", "Delivery", &interner),
        &FieldValue::String(interner.intern("Aimed"))
    );
    jzargo_assert_form(
        jzargo_named(&inner_effect, *b"DATA", "Projectile", &interner),
        0x012FCF,
        "Skyrim.esm",
        &interner,
    );
}

#[test]
fn lowers_scroll_fixture_to_one_effect_fo4_aid_consumable() {
    let interner = StringInterner::new();
    let mut record = jzargo_scroll_source(&interner);

    jzargo_run(&mut record, &interner).unwrap();

    assert_eq!(record.sig.as_str(), "ALCH");
    assert_eq!(jzargo_field(&record, *b"DATA"), &FieldValue::Float(0.5));
    assert_eq!(
        jzargo_named(&record, *b"ENIT", "Value", &interner),
        &FieldValue::Int(100)
    );
    assert_eq!(
        jzargo_named(&record, *b"ENIT", "Flags", &interner),
        &FieldValue::Uint(0x0001_0001)
    );
    assert_eq!(
        record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"EFID")
            .count(),
        1
    );
    jzargo_assert_form(
        jzargo_field(&record, *b"EFID"),
        0x097EE2,
        "Skyrim.esm",
        &interner,
    );
    assert_eq!(
        jzargo_named(&record, *b"EFIT", "Magnitude", &interner),
        &FieldValue::Float(10.0)
    );
    assert_eq!(
        jzargo_named(&record, *b"EFIT", "Duration", &interner),
        &FieldValue::Uint(30)
    );
    for forbidden in [*b"MODL", *b"MODT", *b"SPIT"] {
        assert!(record.fields.iter().all(|field| field.sig.0 != forbidden));
    }
}

#[test]
fn lowers_fixture_spell_and_effect_chain_to_fo4_delivery() {
    let interner = StringInterner::new();
    let mut outer_spell = jzargo_outer_spell_source(&interner);
    let mut inner_spell = jzargo_inner_spell_source(&interner);
    let mut outer_effect = jzargo_outer_effect_source(&interner);
    let mut inner_effect = jzargo_inner_effect_source(&interner);

    for record in [
        &mut outer_spell,
        &mut inner_spell,
        &mut outer_effect,
        &mut inner_effect,
    ] {
        jzargo_run(record, &interner).unwrap();
    }

    assert_eq!(
        jzargo_named(&outer_spell, *b"SPIT", "CastType", &interner),
        &FieldValue::Uint(1)
    );
    assert_eq!(
        jzargo_named(&outer_spell, *b"SPIT", "TargetType", &interner),
        &FieldValue::Uint(0)
    );
    assert_eq!(
        jzargo_named(&outer_spell, *b"EFIT", "Duration", &interner),
        &FieldValue::Uint(30)
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"SPIT", "CastType", &interner),
        &FieldValue::Uint(2)
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"SPIT", "TargetType", &interner),
        &FieldValue::Uint(2)
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"EFIT", "Magnitude", &interner),
        &FieldValue::Float(5.0)
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"EFIT", "Duration", &interner),
        &FieldValue::Uint(5)
    );
    jzargo_assert_form(
        jzargo_named(&outer_effect, *b"DATA", "AssocItem", &interner),
        0x097EE0,
        "Skyrim.esm",
        &interner,
    );
    assert_eq!(
        jzargo_named(&outer_effect, *b"DATA", "Archetype", &interner),
        &FieldValue::Uint(35)
    );
    assert_eq!(
        jzargo_named(&outer_effect, *b"DATA", "SpellmakingArea", &interner),
        &FieldValue::Uint(0)
    );
    jzargo_assert_form(
        jzargo_named(&inner_effect, *b"DATA", "ActorValue", &interner),
        0x0002D4,
        "Fallout4.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&inner_effect, *b"DATA", "ResistValue", &interner),
        0x0002EB,
        "Fallout4.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&inner_effect, *b"DATA", "HitShader", &interner),
        0x14237E,
        "Fallout4.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&inner_effect, *b"DATA", "Projectile", &interner),
        0x204172,
        "Fallout4.esm",
        &interner,
    );
    assert_eq!(
        jzargo_named(&inner_effect, *b"DATA", "Delivery", &interner),
        &FieldValue::Uint(2)
    );
    for forbidden in [*b"SNDD", *b"PROJ", *b"ARTO"] {
        assert!(
            inner_effect
                .fields
                .iter()
                .all(|field| field.sig.0 != forbidden)
        );
    }
}

#[test]
fn default_pair_hook_lowers_supported_magic_before_generic_translation() {
    let interner = StringInterner::new();
    let mut record = jzargo_scroll_source(&interner);
    jzargo_push(
        &mut record,
        *b"VMAD",
        FieldValue::Bytes(smallvec::smallvec![5, 0, 2, 0, 0, 0]),
    );

    jzargo_run_default(&mut record, &interner).unwrap();

    assert_eq!(record.sig.as_str(), "ALCH");
    assert!(record.fields.iter().all(|field| field.sig.0 != *b"MODL"));
    assert!(record.fields.iter().any(|field| field.sig.0 == *b"ENIT"));
    jzargo_assert_form(
        jzargo_field(&record, *b"EFID"),
        0x097EE2,
        "Skyrim.esm",
        &interner,
    );
    assert!(record.fields.iter().any(|field| field.sig.0 == *b"VMAD"));
}

#[test]
fn default_pair_hook_matches_direct_lowering_for_every_supported_magic_shape() {
    let interner = StringInterner::new();
    let sources = [
        jzargo_scroll_source(&interner),
        jzargo_outer_spell_source(&interner),
        jzargo_inner_spell_source(&interner),
        jzargo_outer_effect_source(&interner),
        jzargo_inner_effect_source(&interner),
    ];

    for source in sources {
        let mut expected = source.clone();
        jzargo_run(&mut expected, &interner).unwrap();
        let mut actual = source;
        jzargo_run_default(&mut actual, &interner).unwrap();

        assert_eq!(actual.sig, expected.sig);
        assert_eq!(actual.form_key, expected.form_key);
        assert_eq!(actual.eid, expected.eid);
        assert_eq!(actual.flags, expected.flags);
        assert_eq!(actual.fields, expected.fields);
        assert_eq!(actual.warnings, expected.warnings);
    }
}

#[test]
fn routes_unclassified_skyrim_spells_through_safe_fallback() {
    let interner = StringInterner::new();
    let mut record = jzargo_record(&interner, "SPEL", 0x012345, "UnrelatedSpell");
    jzargo_push(
        &mut record,
        *b"SPIT",
        FieldValue::Bytes(smallvec::smallvec![1, 2, 3]),
    );
    jzargo_push(
        &mut record,
        *b"VMAD",
        FieldValue::Bytes(smallvec::smallvec![5, 0, 2, 0, 0, 0]),
    );

    jzargo_run_default(&mut record, &interner).unwrap();

    assert_eq!(record.sig.as_str(), "SPEL");
    assert_eq!(record.eid, Some(interner.intern("UnrelatedSpell")));
    assert_eq!(
        jzargo_named(&record, *b"SPIT", "CastType", &interner),
        &FieldValue::Uint(1)
    );
    assert!(record.fields.iter().any(|field| field.sig.0 == *b"VMAD"));
}

#[test]
fn routes_unclassified_scrolls_and_shouts_to_fo4_record_types() {
    let interner = StringInterner::new();
    let mut scroll = jzargo_scroll_source(&interner);
    let second_effect = jzargo_source_form(&scroll, 0x012345);
    jzargo_push(&mut scroll, *b"EFID", second_effect);
    jzargo_push(
        &mut scroll,
        *b"EFIT",
        jzargo_object(
            &interner,
            [
                ("Magnitude", FieldValue::Float(1.0)),
                ("Area", FieldValue::Uint(0)),
                ("Duration", FieldValue::Uint(1)),
            ],
        ),
    );
    assert!(matches!(
        classify_magic_component(&scroll, &interner),
        MagicSupport::Unsupported { .. }
    ));
    jzargo_run_default(&mut scroll, &interner).unwrap();
    assert_eq!(scroll.sig.as_str(), "ALCH");
    assert_eq!(
        scroll
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"EFID")
            .count(),
        2
    );

    let mut shout = jzargo_record(&interner, "SHOU", 0x0A82BC, "CreatureShout");
    jzargo_run_default(&mut shout, &interner).unwrap();
    assert_eq!(shout.sig.as_str(), "SPEL");
    assert_eq!(
        jzargo_named(&shout, *b"SPIT", "TargetType", &interner),
        &FieldValue::Uint(0)
    );
}

#[test]
fn target_normalizer_accepts_every_lowered_jzargo_magic_record() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let mut records = vec![
        jzargo_scroll_source(&interner),
        jzargo_outer_spell_source(&interner),
        jzargo_inner_spell_source(&interner),
        jzargo_outer_effect_source(&interner),
        jzargo_inner_effect_source(&interner),
    ];

    for record in &mut records {
        jzargo_run(record, &interner).unwrap();
        let normalized = TargetRecordNormalizer::target_only_with_interner(&schema, &interner)
            .normalize(record.clone());
        let TargetRecordNormalization::Keep(normalized) = normalized else {
            panic!("{} must be supported by FO4", record.sig.as_str())
        };
        assert_eq!(normalized.sig, record.sig);
        assert!(
            normalized
                .fields
                .iter()
                .all(|field| field.sig.0 != *b"MODL")
        );
    }
}

fn jzargo_find_parsed<'a>(
    items: &'a [ParsedItem],
    signature: &str,
    local: u32,
) -> Option<&'a ParsedRecord> {
    for item in items {
        match item {
            ParsedItem::Record(record)
                if record.signature.as_str() == signature
                    && (record.form_id & 0x00FF_FFFF) == local =>
            {
                return Some(record);
            }
            ParsedItem::Group(group) => {
                if let Some(record) = jzargo_find_parsed(&group.children, signature, local) {
                    return Some(record);
                }
            }
            _ => {}
        }
    }
    None
}

fn jzargo_parsed_subrecord<'a>(record: &'a ParsedRecord, signature: &str) -> &'a [u8] {
    record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == signature)
        .unwrap_or_else(|| panic!("missing saved {} {signature}", record.signature))
        .data
        .as_ref()
}

fn jzargo_parsed_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

#[test]
fn lowered_jzargo_chain_survives_native_save_reopen() {
    const OUTPUT: &str = "Skyrim.esm";
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let mut records = vec![
        jzargo_scroll_source(&interner),
        jzargo_outer_spell_source(&interner),
        jzargo_inner_spell_source(&interner),
        jzargo_outer_effect_source(&interner),
        jzargo_inner_effect_source(&interner),
    ];
    for record in &mut records {
        jzargo_run(record, &interner).unwrap();
        let normalized = TargetRecordNormalizer::target_only_with_interner(&schema, &interner)
            .normalize(record.clone());
        let TargetRecordNormalization::Keep(normalized) = normalized else {
            panic!("{} must be supported by FO4", record.sig.as_str())
        };
        *record = normalized;
    }

    let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    for record in &records {
        crate::target_write::add_record_native(handle, record.clone(), &schema, &interner)
            .unwrap_or_else(|error| panic!("write {} failed: {error}", record.sig.as_str()));
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(OUTPUT);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));

    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    let store = plugin_handle_store_ref().lock().unwrap();
    let slot = store.get(&reopened).unwrap();
    assert!(jzargo_find_parsed(&slot.parsed.root_items, "SCRL", 0x0967E3).is_none());

    let consumable = jzargo_find_parsed(&slot.parsed.root_items, "ALCH", 0x0967E3).unwrap();
    assert_eq!(
        consumable
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "EFID")
            .count(),
        1
    );
    assert_eq!(
        jzargo_parsed_u32(jzargo_parsed_subrecord(consumable, "EFID"), 0),
        0x01097EE2
    );
    let consumable_efit = jzargo_parsed_subrecord(consumable, "EFIT");
    assert_eq!(consumable_efit.len(), 12);
    assert_eq!(
        f32::from_le_bytes(consumable_efit[0..4].try_into().unwrap()),
        10.0
    );
    assert_eq!(jzargo_parsed_u32(consumable_efit, 8), 30);

    let inner_spell = jzargo_find_parsed(&slot.parsed.root_items, "SPEL", 0x097EE0).unwrap();
    let spell_data = jzargo_parsed_subrecord(inner_spell, "SPIT");
    assert_eq!(spell_data.len(), 36, "saved SPIT bytes: {spell_data:02X?}");
    assert_eq!(jzargo_parsed_u32(spell_data, 16), 2);
    assert_eq!(jzargo_parsed_u32(spell_data, 20), 2);
    assert_eq!(
        jzargo_parsed_u32(jzargo_parsed_subrecord(inner_spell, "EFIT"), 8),
        5
    );

    let outer_effect = jzargo_find_parsed(&slot.parsed.root_items, "MGEF", 0x097EE2).unwrap();
    let outer_effect_data = jzargo_parsed_subrecord(outer_effect, "DATA");
    assert_eq!(outer_effect_data.len(), 152);
    assert_eq!(jzargo_parsed_u32(outer_effect_data, 8), 0x01097EE0);

    let inner_effect = jzargo_find_parsed(&slot.parsed.root_items, "MGEF", 0x097EE1).unwrap();
    let inner_effect_data = jzargo_parsed_subrecord(inner_effect, "DATA");
    assert_eq!(inner_effect_data.len(), 152);
    assert_eq!(jzargo_parsed_u32(inner_effect_data, 72), 0x00204172);
    assert_eq!(jzargo_parsed_u32(inner_effect_data, 80), 2);
    assert_eq!(jzargo_parsed_u32(inner_effect_data, 84), 2);
    assert!(
        inner_effect
            .subrecords
            .iter()
            .any(|subrecord| subrecord.signature.as_str() == "VMAD")
    );

    drop(store);
    assert!(plugin_handle_close_native(reopened));
}
