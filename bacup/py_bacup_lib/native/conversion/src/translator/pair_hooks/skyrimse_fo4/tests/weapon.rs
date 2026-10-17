#[test]
fn pair_hook_projects_audited_steel_battleaxe_before_generic_translation() {
    let interner = StringInterner::new();
    let plugin = interner.intern("Skyrim.esm");
    let mut record = Record::new(
        SigCode::from_str("WEAP").unwrap(),
        FormKey {
            local: 0x013984,
            plugin,
        },
    );
    record.eid = Some(interner.intern("SteelBattleaxe"));
    let mut dnam = [0_u8; 100];
    dnam[0] = 6;
    record.fields.extend([
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(interner.intern("SteelBattleaxe")),
        },
        FieldEntry {
            sig: SubrecordSig(*b"OBND"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0; 12])),
        },
        FieldEntry {
            sig: SubrecordSig(*b"FULL"),
            value: FieldValue::String(interner.intern("Steel Battleaxe")),
        },
        FieldEntry {
            sig: SubrecordSig(*b"MODL"),
            value: FieldValue::String(interner.intern("Weapons\\Steel\\SteelBattleAxe.nif")),
        },
        FieldEntry {
            sig: SubrecordSig(*b"ETYP"),
            value: FieldValue::FormKey(FormKey {
                local: 0x013F45,
                plugin,
            }),
        },
        FieldEntry {
            sig: SubrecordSig(*b"WNAM"),
            value: FieldValue::FormKey(FormKey {
                local: 0x020E27,
                plugin,
            }),
        },
        FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[
                100, 0, 0, 0, 0, 0, 168, 65, 18, 0,
            ])),
        },
        FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&dnam)),
        },
    ]);

    let translator = Translator::new(Game::SkyrimSe, Game::Fo4).unwrap();
    translator
        .pre_translate(&mut PairCtx::new(&interner), &mut record)
        .unwrap();
    let mut record = match translator.translate(&record, &interner) {
        TranslateResult::Translated(record) => record,
        other => panic!("projected Steel Battleaxe must translate, got {other:?}"),
    };
    translator
        .post_translate(&mut PairCtx::new(&interner), &mut record)
        .unwrap();

    assert!(record.fields.iter().any(|field| {
        field.sig.0 == *b"MOD4"
            && matches!(&field.value, FieldValue::String(path) if interner.resolve(*path) == Some("Weapons\\Steel\\1stPersonSteelBattleAxe.nif"))
    }));
    assert!(matches!(
        record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"ETYP")
            .map(|field| &field.value),
        Some(FieldValue::FormKey(form_key))
            if form_key.local == 0x013F42
                && interner.resolve(form_key.plugin) == Some("Fallout4.esm")
    ));
    let FieldValue::Struct(dnam) = &record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DNAM")
        .unwrap()
        .value
    else {
        panic!("generic translation overwrote projected DNAM");
    };
    assert!(dnam.iter().any(|(name, value)| {
        interner.resolve(*name) == Some("animation_type") && *value == FieldValue::Uint(5)
    }));
    assert_eq!(
        record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CRDT")
            .count(),
        1
    );
}

fn skyrim_weapon_layout_fixture(interner: &StringInterner) -> Record {
    let mut record = Record::new(
        SigCode(*b"WEAP"),
        FormKey::parse("013985@Skyrim.esm", interner).unwrap(),
    );
    record.eid = Some(interner.intern("HuntingBow"));
    let mut dnam = [0u8; 100];
    dnam[0] = 7;
    dnam[4..8].copy_from_slice(&0.9375f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1.0f32.to_le_bytes());
    dnam[24] = 5;
    dnam[25] = 255;
    dnam[26] = 1;
    dnam[28..32].copy_from_slice(&500f32.to_le_bytes());
    dnam[32..36].copy_from_slice(&2000f32.to_le_bytes());
    dnam[44..48].copy_from_slice(&1f32.to_le_bytes());
    dnam[48..52].copy_from_slice(&5f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&8u32.to_le_bytes());
    dnam[88..92].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut data = [0u8; 10];
    data[..4].copy_from_slice(&50u32.to_le_bytes());
    data[4..8].copy_from_slice(&7f32.to_le_bytes());
    data[8..10].copy_from_slice(&7u16.to_le_bytes());
    let mut critical = [0u8; 24];
    critical[..2].copy_from_slice(&3u16.to_le_bytes());
    critical[2..4].copy_from_slice(&0xB5Au16.to_le_bytes());
    critical[4..8].copy_from_slice(&1f32.to_le_bytes());
    critical[8] = 1;
    for (signature, bytes) in [
        (*b"DNAM", &dnam[..]),
        (*b"DATA", &data[..]),
        (*b"CRDT", &critical[..]),
    ] {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(signature),
            value: FieldValue::Bytes(bytes.into()),
        });
    }
    record.fields.extend([
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(interner.intern("HuntingBow")),
        },
        FieldEntry {
            sig: SubrecordSig(*b"VNAM"),
            value: FieldValue::Uint(2),
        },
        FieldEntry {
            sig: SubrecordSig(*b"WNAM"),
            value: FieldValue::FormKey(FormKey::parse("023B5A@Skyrim.esm", interner).unwrap()),
        },
        FieldEntry {
            sig: SubrecordSig(*b"EITM"),
            value: FieldValue::FormKey(FormKey::parse("000900@Skyrim.esm", interner).unwrap()),
        },
        FieldEntry {
            sig: SubrecordSig(*b"TNAM"),
            value: FieldValue::FormKey(FormKey::parse("03C730@Skyrim.esm", interner).unwrap()),
        },
        FieldEntry {
            sig: SubrecordSig(*b"NAM9"),
            value: FieldValue::FormKey(FormKey::parse("03D78C@Skyrim.esm", interner).unwrap()),
        },
    ]);
    record
}

fn skyrim_weapon_layout_value<'a>(
    record: &'a Record,
    signature: &[u8; 4],
    name: &str,
    interner: &StringInterner,
) -> &'a FieldValue {
    let FieldValue::Struct(fields) = &record
        .fields
        .iter()
        .find(|field| &field.sig.0 == signature)
        .unwrap()
        .value
    else {
        panic!("expected structured field")
    };
    &fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .unwrap()
        .1
}

#[test]
fn skyrim_weapon_layout_rebuilds_ranged_stats_and_preserves_dependencies() {
    let interner = StringInterner::new();
    let mut record = skyrim_weapon_layout_fixture(&interner);
    let ctx = PairCtx::with_source(&interner, "Skyrim.esm", &[]);
    weapon::normalize_skyrim_weapon(&mut record, &ctx).unwrap();
    for (name, expected) in [
        ("ammo", FieldValue::Uint(0)),
        ("resist", FieldValue::Uint(0)),
        ("skill", FieldValue::Uint(0)),
        ("speed", FieldValue::Float(0.9375)),
        ("min_range", FieldValue::Float(500.0)),
        ("max_range", FieldValue::Float(2000.0)),
        ("weight", FieldValue::Float(7.0)),
        ("damage_base", FieldValue::Uint(7)),
        ("value", FieldValue::Uint(50)),
        ("animation_type", FieldValue::Uint(7)),
        ("flags", FieldValue::Uint(256 | 0x800)),
        ("capacity", FieldValue::Uint(0)),
        ("sound_level", FieldValue::Uint(2)),
    ] {
        assert_eq!(
            skyrim_weapon_layout_value(&record, b"DNAM", name, &interner),
            &expected,
            "{name}"
        );
    }
    assert_eq!(
        skyrim_weapon_layout_value(&record, b"FNAM", "projectiles", &interner),
        &FieldValue::Uint(1)
    );
    assert_eq!(
        skyrim_weapon_layout_value(&record, b"DNAM", "sound_equip_sound", &interner),
        &FieldValue::FormKey(FormKey::parse("03D78C@Skyrim.esm", &interner).unwrap())
    );
    assert!(record.fields.iter().any(|field| field.sig.0 == *b"WNAM"));
    assert!(record.fields.iter().any(|field| field.sig.0 == *b"EITM"));
    assert!(!record.fields.iter().any(|field| field.sig.0 == *b"DATA"));
    let before = record.fields.clone();
    weapon::normalize_skyrim_weapon(&mut record, &ctx).unwrap();
    assert_eq!(record.fields, before);
}

#[test]
fn skyrim_bows_use_native_hold_power_and_inventory_arrows_without_magazine_reload() {
    let interner = StringInterner::new();
    for (animation_type, capacity, hold_power) in [(7, 0, true), (9, 1, false), (1, 0, false)] {
        let mut record = skyrim_weapon_layout_fixture(&interner);
        let FieldValue::Bytes(bytes) = &mut record.fields[0].value else {
            unreachable!()
        };
        bytes[0] = animation_type;
        weapon::normalize_skyrim_weapon(&mut record, &PairCtx::new(&interner)).unwrap();
        let FieldValue::Uint(flags) =
            skyrim_weapon_layout_value(&record, b"DNAM", "flags", &interner)
        else {
            unreachable!()
        };
        assert_eq!(flags & 0x800 != 0, hold_power);
        assert_eq!(
            skyrim_weapon_layout_value(&record, b"DNAM", "capacity", &interner),
            &FieldValue::Uint(capacity)
        );
        assert_eq!(
            skyrim_weapon_layout_value(&record, b"DNAM", "min_power_per_shot", &interner),
            &FieldValue::Float(0.0)
        );
    }
}

#[test]
fn skyrim_weapon_layout_resolves_critical_effect_owner_and_only_compatible_flags() {
    let interner = StringInterner::new();
    let mut record = skyrim_weapon_layout_fixture(&interner);
    for field in &mut record.fields {
        if let FieldValue::Bytes(bytes) = &mut field.value {
            if field.sig.0 == *b"DNAM" {
                bytes[12..14].copy_from_slice(&255u16.to_le_bytes());
                bytes[40..44].copy_from_slice(&0x3FFFu32.to_le_bytes());
            } else if field.sig.0 == *b"CRDT" {
                bytes[16..20].copy_from_slice(&0x01000800u32.to_le_bytes());
            }
        }
    }
    let masters = ["Skyrim.esm".into()];
    let ctx = PairCtx::with_source(&interner, "Dawnguard.esm", &masters);
    weapon::normalize_skyrim_weapon(&mut record, &ctx).unwrap();
    assert_eq!(
        skyrim_weapon_layout_value(&record, b"CRDT", "crit_effect", &interner),
        &FieldValue::FormKey(FormKey::parse("000800@Dawnguard.esm", &interner).unwrap())
    );
    assert_eq!(
        skyrim_weapon_layout_value(&record, b"DNAM", "flags", &interner),
        &FieldValue::Uint(
            1 | 2 | 16 | 32 | 64 | 256 | 0x800 | 4096 | 8192 | 16384 | 131072 | 524288 | 1048576
        )
    );
}

#[test]
fn skyrim_weapon_layout_rejects_truncated_source_without_mutation() {
    let interner = StringInterner::new();
    let mut record = skyrim_weapon_layout_fixture(&interner);
    let FieldValue::Bytes(bytes) = &mut record.fields[0].value else {
        unreachable!()
    };
    bytes.truncate(96);
    let before = record.fields.clone();
    assert!(weapon::normalize_skyrim_weapon(&mut record, &PairCtx::new(&interner)).is_err());
    assert_eq!(record.fields, before);
}

#[test]
fn skyrim_weapon_layout_survives_translate_normalize_save_reopen() {
    let interner = StringInterner::new();
    let mut record = skyrim_weapon_layout_fixture(&interner);
    let translator = Translator::new(Game::SkyrimSe, Game::Fo4).unwrap();
    let mut ctx = PairCtx::with_source(&interner, "Skyrim.esm", &[]);
    translator.pre_translate(&mut ctx, &mut record).unwrap();
    let TranslateResult::Translated(mut record) = translator.translate(&record, &interner) else {
        panic!("weapon translation failed")
    };
    translator.post_translate(&mut ctx, &mut record).unwrap();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let TargetRecordNormalization::Keep(record) =
        TargetRecordNormalizer::target_only_with_interner(&schema, &interner).normalize(record)
    else {
        panic!("weapon normalization failed")
    };
    let handle = plugin_handle_new_no_py("Skyrim.esm", Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    crate::target_write::add_record_native(handle, record, &schema, &interner).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Skyrim.esm");
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        let saved = jzargo_find_parsed(&slot.parsed.root_items, "WEAP", 0x013985).unwrap();
        let dnam = jzargo_parsed_subrecord(saved, "DNAM");
        assert_eq!(dnam.len(), 132);
        assert_eq!(jzargo_parsed_u32(dnam, 0), 0);
        assert_eq!(jzargo_parsed_u32(dnam, 44), 0);
        assert_eq!(jzargo_parsed_u32(dnam, 48), 256 | 0x800);
        assert_eq!(dnam[54], 7);
        assert_eq!(f32::from_le_bytes(dnam[59..63].try_into().unwrap()), 7.0);
        assert_eq!(u16::from_le_bytes(dnam[67..69].try_into().unwrap()), 7);
        assert_eq!(jzargo_parsed_u32(dnam, 93), 0x0103D78C);
        assert_eq!(jzargo_parsed_subrecord(saved, "FNAM").len(), 41);
        assert_eq!(jzargo_parsed_subrecord(saved, "FNAM")[28], 1);
        assert_eq!(jzargo_parsed_subrecord(saved, "CRDT").len(), 12);
        assert_eq!(
            f32::from_le_bytes(
                jzargo_parsed_subrecord(saved, "CRDT")[..4]
                    .try_into()
                    .unwrap()
            ),
            2.0
        );
    }
    assert!(plugin_handle_close_native(reopened));
}

#[test]
#[ignore = "requires the installed Skyrim Special Edition source masters"]
fn skyrim_weapon_layout_accepts_installed_master_corpus() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let mut total = 0;
    let mut failures = Vec::new();
    for plugin in [
        "Skyrim.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let path =
            std::path::Path::new("N:/Steam Games/steamapps/common/Skyrim Special Edition/Data")
                .join(plugin);
        let handle =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("skyrimse"), None, None, true)
                .unwrap();
        let (name, masters) = crate::source_read::plugin_context_for_handle(handle).unwrap();
        let keys = crate::source_read::iter_form_keys_of_sig(handle, SigCode(*b"WEAP"), &interner)
            .unwrap();
        let ctx = PairCtx::with_source(&interner, &name, &masters);
        for key in keys {
            let mut record = crate::source_read::read_record_relayout_by_form_key(
                handle, &key, &schema, &interner, None,
            )
            .unwrap();
            if let Err(error) = weapon::normalize_skyrim_weapon(&mut record, &ctx) {
                failures.push(format!("{plugin}:{:06X}: {error}", key.local));
            }
            total += 1;
        }
        assert!(plugin_handle_close_native(handle));
    }
    eprintln!(
        "Skyrim source weapon layout corpus: {total} records, {} failures",
        failures.len()
    );
    assert!(total > 300);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
