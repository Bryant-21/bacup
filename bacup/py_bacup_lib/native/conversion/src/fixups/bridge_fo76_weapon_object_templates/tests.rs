use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
};

const OUTPUT: &str = "SeventySix.esm";
const FALLOUT4: &str = "Fallout4.esm";
const ROBOT: &str = "DLCRobot.esm";
const NUKA_WORLD: &str = "DLCNukaWorld.esm";
/// `44`, the .44 pistol: shared with Fallout4.esm, so the mapper reuses vanilla's record.
const PISTOL: u32 = 0x0CE97D;
/// `DLC04_HandMadeGun`, its standard receiver and its ammo, all owned by Nuka-World.
const HANDMADE: u32 = 0x033B60;
const HANDMADE_RECEIVER: u32 = 0x03787D;
const HANDMADE_AMMO: u32 = 0x037897;
/// FO76 weapon that maps onto the same vanilla weapon as `PISTOL`.
const SECOND_SOURCE_WEAPON: u32 = 0x5A0001;
const SECOND_UNIQUE_KEYWORD: u32 = 0x8DC2E0;
/// WEAP DNAM is 132 bytes and opens with the ammo FormID.
const WEAP_DNAM_LEN: usize = 132;
/// A keyword both games key a .44 combination on.
const SHARED_KEYWORD: u32 = 0x160404;
/// `if_tmp_MedicalMalpractice` and the mods its combination adds.
const UNIQUE_KEYWORD: u32 = 0x8DC2D4;
const RECEIVER: u32 = 0x2E5B30;
const CUSTOM_MOD: u32 = 0x8DC2D7;
/// An include whose mod was not converted.
const DROPPED_MOD: u32 = 0x9A0001;
const DEFAULT_MOD: u32 = 0x1A05D8;
/// FO76-only attach point that `mod_Custom_MedicalMalpractice` hangs from.
const CUSTOM_ATTACH_POINT: u32 = 0x47A264;
const RECEIVER_ATTACH_POINT: u32 = 0x024004;

fn obts(keywords: &[u32], includes: &[u32]) -> Vec<u8> {
    let mut bytes = vec![0u8; OBTS_FIXED_HEADER_LEN];
    bytes[0..4].copy_from_slice(&u32::try_from(includes.len()).unwrap().to_le_bytes());
    bytes[OBTS_KEYWORD_COUNT_OFFSET] = u8::try_from(keywords.len()).unwrap();
    for keyword in keywords {
        bytes.extend_from_slice(&keyword.to_le_bytes());
    }
    bytes.extend_from_slice(&[0; OBTS_INCLUDE_PADDING_LEN]);
    for include in includes {
        bytes.extend_from_slice(&include.to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 1]);
    }
    bytes
}

fn bytes_field(sig: &[u8; 4], data: Vec<u8>) -> FieldEntry {
    field(sig, FieldValue::Bytes(data.into()))
}

fn weapon(
    plugin: &str,
    local: u32,
    slots: &[u32],
    combinations: &[Vec<u8>],
    interner: &StringInterner,
) -> Record {
    let mut record = Record::new(
        SigCode(*b"WEAP"),
        FormKey {
            plugin: interner.intern(plugin),
            local,
        },
    );
    let eid = interner.intern("44");
    record.eid = Some(eid);
    record.fields.push(field(b"EDID", FieldValue::String(eid)));
    record.fields.push(bytes_field(
        b"APPR",
        slots.iter().flat_map(|slot| slot.to_le_bytes()).collect(),
    ));
    record.fields.push(bytes_field(
        b"OBTE",
        u32::try_from(combinations.len())
            .unwrap()
            .to_le_bytes()
            .to_vec(),
    ));
    for combination in combinations {
        record
            .fields
            .push(bytes_field(b"OBTS", combination.clone()));
    }
    record.fields.push(bytes_field(b"STOP", Vec::new()));
    record
}

fn combinations(record: &Record) -> Vec<Vec<u8>> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"OBTS")
        .map(|field| match &field.value {
            FieldValue::Bytes(bytes) => bytes.to_vec(),
            other => panic!("OBTS decoded as {other:?}"),
        })
        .collect()
}

#[test]
fn unique_combination_is_added_to_a_fallout4_master_override() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native(OUTPUT, Some("fo76")).unwrap();
    let fallout4 = plugin_handle_new_native(FALLOUT4, Some("fo4")).unwrap();
    let target = plugin_handle_new_native(OUTPUT, Some("fo4")).unwrap();
    plugin_handle_add_master_native(target, FALLOUT4, None).unwrap();

    let default_combination = obts(&[], &[DEFAULT_MOD]);
    let shared_combination = obts(&[SHARED_KEYWORD], &[DEFAULT_MOD]);
    {
        let mut session = open_session(source, None).unwrap();
        let schema = session.schema().unwrap();
        let record = weapon(
            OUTPUT,
            PISTOL,
            &[CUSTOM_ATTACH_POINT, RECEIVER_ATTACH_POINT],
            &[
                default_combination.clone(),
                shared_combination.clone(),
                obts(&[UNIQUE_KEYWORD], &[RECEIVER, CUSTOM_MOD, DROPPED_MOD]),
            ],
            &interner,
        );
        session.add_record(record, &schema, &interner).unwrap();
    }
    {
        let mut session = open_session(fallout4, None).unwrap();
        let schema = session.schema().unwrap();
        let record = weapon(
            FALLOUT4,
            PISTOL,
            &[RECEIVER_ATTACH_POINT],
            &[default_combination.clone(), shared_combination.clone()],
            &interner,
        );
        session.add_record(record, &schema, &interner).unwrap();
    }

    let mut state = MapperState::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: OUTPUT.into(),
            preserve_source_ids: true,
            ..Default::default()
        },
    );
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let output = interner.intern(OUTPUT);
    let vanilla = interner.intern(FALLOUT4);
    for (local, plugin) in [
        (PISTOL, vanilla),
        (SHARED_KEYWORD, vanilla),
        (RECEIVER, vanilla),
        (DEFAULT_MOD, vanilla),
        (RECEIVER_ATTACH_POINT, vanilla),
        (UNIQUE_KEYWORD, output),
        (CUSTOM_MOD, output),
        (CUSTOM_ATTACH_POINT, output),
    ] {
        mapper.add_mapping(
            FormKey {
                plugin: output,
                local,
            },
            FormKey { plugin, local },
        );
    }
    let config = FixupConfig {
        target_master_handle_ids: vec![fallout4],
        ..Default::default()
    };

    let mut session = open_session(target, Some(source)).unwrap();
    let report = BridgeFo76WeaponObjectTemplatesFixup
        .run_with_session(&mut session, &mut mapper, &config)
        .unwrap();
    assert_eq!(
        report.records_added,
        1,
        "{:?}",
        report.message.and_then(|m| interner.resolve(m))
    );

    let schema = session.schema().unwrap();
    let pistol = session
        .record_decoded(
            &FormKey {
                plugin: vanilla,
                local: PISTOL,
            },
            &schema,
            &interner,
        )
        .unwrap();
    let encoded_output = |local: u32| (1 << 24) | local;
    assert_eq!(
        combinations(&pistol),
        vec![
            default_combination,
            shared_combination,
            obts(
                &[encoded_output(UNIQUE_KEYWORD)],
                &[RECEIVER, encoded_output(CUSTOM_MOD)],
            ),
        ]
    );
    let count = pistol
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"OBTE")
        .and_then(|field| field_value_u32(&field.value));
    assert_eq!(count, Some(3));
    let slots = pistol
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"APPR")
        .map(|field| field.value.clone())
        .unwrap();
    let custom_slot = FormKey {
        plugin: output,
        local: CUSTOM_ATTACH_POINT,
    };
    let has_custom_slot = match &slots {
        FieldValue::List(values) => values.contains(&FieldValue::FormKey(custom_slot)),
        FieldValue::Bytes(bytes) => bytes.chunks_exact(4).any(|raw| {
            u32::from_le_bytes(raw.try_into().unwrap()) == encoded_output(CUSTOM_ATTACH_POINT)
        }),
        other => panic!("APPR decoded as {other:?}"),
    };
    assert!(has_custom_slot, "{slots:?}");

    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(fallout4));
    assert!(plugin_handle_close_native(source));
}

fn with_form_id_property(mut combination: Vec<u8>, value: u32) -> Vec<u8> {
    combination[4..8].copy_from_slice(&1u32.to_le_bytes());
    let mut row = [0u8; 24];
    row[0] = 4;
    row[12..16].copy_from_slice(&value.to_le_bytes());
    combination.extend_from_slice(&row);
    combination
}

#[test]
fn dlc_weapon_override_renumbers_raw_form_ids_into_output_masters() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native(OUTPUT, Some("fo76")).unwrap();
    let fallout4 = plugin_handle_new_native(FALLOUT4, Some("fo4")).unwrap();
    let robot = plugin_handle_new_native(ROBOT, Some("fo4")).unwrap();
    plugin_handle_add_master_native(robot, FALLOUT4, None).unwrap();
    let nuka_world = plugin_handle_new_native(NUKA_WORLD, Some("fo4")).unwrap();
    plugin_handle_add_master_native(nuka_world, FALLOUT4, None).unwrap();
    let target = plugin_handle_new_native(OUTPUT, Some("fo4")).unwrap();
    for master in [FALLOUT4, ROBOT, NUKA_WORLD] {
        plugin_handle_add_master_native(target, master, None).unwrap();
    }

    // Nuka-World names its own forms as master 01, which the output calls DLCRobot.esm.
    let nuka_world_own = |local: u32| (1 << 24) | local;
    let output_nuka_world = |local: u32| (2 << 24) | local;
    let output_own = |local: u32| (3 << 24) | local;
    let default_combination =
        |receiver: u32, ammo: u32| with_form_id_property(obts(&[], &[receiver, DEFAULT_MOD]), ammo);
    let mut dnam = vec![0u8; WEAP_DNAM_LEN];
    dnam[0..4].copy_from_slice(&nuka_world_own(HANDMADE_AMMO).to_le_bytes());
    {
        let mut session = open_session(nuka_world, None).unwrap();
        let schema = session.schema().unwrap();
        let mut record = weapon(
            NUKA_WORLD,
            HANDMADE,
            &[RECEIVER_ATTACH_POINT],
            &[default_combination(
                nuka_world_own(HANDMADE_RECEIVER),
                nuka_world_own(HANDMADE_AMMO),
            )],
            &interner,
        );
        record.fields.push(bytes_field(b"DNAM", dnam));
        session.add_record(record, &schema, &interner).unwrap();
    }
    {
        let mut session = open_session(source, None).unwrap();
        let schema = session.schema().unwrap();
        let record = weapon(
            OUTPUT,
            HANDMADE,
            &[CUSTOM_ATTACH_POINT],
            &[obts(&[UNIQUE_KEYWORD], &[CUSTOM_MOD])],
            &interner,
        );
        session.add_record(record, &schema, &interner).unwrap();
    }

    let mut state = MapperState::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: OUTPUT.into(),
            preserve_source_ids: true,
            ..Default::default()
        },
    );
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let output = interner.intern(OUTPUT);
    let dlc = interner.intern(NUKA_WORLD);
    for (local, plugin) in [
        (HANDMADE, dlc),
        (UNIQUE_KEYWORD, output),
        (CUSTOM_MOD, output),
        (CUSTOM_ATTACH_POINT, output),
    ] {
        mapper.add_mapping(
            FormKey {
                plugin: output,
                local,
            },
            FormKey { plugin, local },
        );
    }
    let config = FixupConfig {
        target_master_handle_ids: vec![fallout4, robot, nuka_world],
        ..Default::default()
    };

    let mut session = open_session(target, Some(source)).unwrap();
    let report = BridgeFo76WeaponObjectTemplatesFixup
        .run_with_session(&mut session, &mut mapper, &config)
        .unwrap();
    assert_eq!(report.records_added, 1);

    let schema = session.schema().unwrap();
    let handmade = session
        .record_decoded(
            &FormKey {
                plugin: dlc,
                local: HANDMADE,
            },
            &schema,
            &interner,
        )
        .unwrap();
    assert_eq!(
        combinations(&handmade),
        vec![
            default_combination(
                output_nuka_world(HANDMADE_RECEIVER),
                output_nuka_world(HANDMADE_AMMO),
            ),
            obts(&[output_own(UNIQUE_KEYWORD)], &[output_own(CUSTOM_MOD)]),
        ]
    );
    let ammo = handmade
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"DNAM")
        .and_then(|field| field_value_u32(&field.value));
    assert_eq!(ammo, Some(output_nuka_world(HANDMADE_AMMO)));

    drop(session);
    for handle in [target, nuka_world, robot, fallout4, source] {
        assert!(plugin_handle_close_native(handle));
    }
}

#[test]
fn source_weapons_sharing_a_vanilla_weapon_write_one_override() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native(OUTPUT, Some("fo76")).unwrap();
    let fallout4 = plugin_handle_new_native(FALLOUT4, Some("fo4")).unwrap();
    let target = plugin_handle_new_native(OUTPUT, Some("fo4")).unwrap();
    plugin_handle_add_master_native(target, FALLOUT4, None).unwrap();

    let default_combination = obts(&[], &[DEFAULT_MOD]);
    {
        let mut session = open_session(source, None).unwrap();
        let schema = session.schema().unwrap();
        for (local, keyword) in [
            (PISTOL, UNIQUE_KEYWORD),
            (SECOND_SOURCE_WEAPON, SECOND_UNIQUE_KEYWORD),
        ] {
            let record = weapon(
                OUTPUT,
                local,
                &[CUSTOM_ATTACH_POINT],
                &[obts(&[keyword], &[CUSTOM_MOD])],
                &interner,
            );
            session.add_record(record, &schema, &interner).unwrap();
        }
    }
    {
        let mut session = open_session(fallout4, None).unwrap();
        let schema = session.schema().unwrap();
        let record = weapon(
            FALLOUT4,
            PISTOL,
            &[RECEIVER_ATTACH_POINT],
            &[default_combination.clone()],
            &interner,
        );
        session.add_record(record, &schema, &interner).unwrap();
    }

    let mut state = MapperState::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: OUTPUT.into(),
            preserve_source_ids: true,
            ..Default::default()
        },
    );
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let output = interner.intern(OUTPUT);
    let vanilla = interner.intern(FALLOUT4);
    for (local, plugin) in [
        (PISTOL, vanilla),
        (UNIQUE_KEYWORD, output),
        (SECOND_UNIQUE_KEYWORD, output),
        (CUSTOM_MOD, output),
        (CUSTOM_ATTACH_POINT, output),
    ] {
        mapper.add_mapping(
            FormKey {
                plugin: output,
                local,
            },
            FormKey { plugin, local },
        );
    }
    mapper.add_mapping(
        FormKey {
            plugin: output,
            local: SECOND_SOURCE_WEAPON,
        },
        FormKey {
            plugin: vanilla,
            local: PISTOL,
        },
    );
    let config = FixupConfig {
        target_master_handle_ids: vec![fallout4],
        ..Default::default()
    };

    let mut session = open_session(target, Some(source)).unwrap();
    let report = BridgeFo76WeaponObjectTemplatesFixup
        .run_with_session(&mut session, &mut mapper, &config)
        .unwrap();
    assert_eq!(report.records_added, 1);

    let schema = session.schema().unwrap();
    let pistol = session
        .record_decoded(
            &FormKey {
                plugin: vanilla,
                local: PISTOL,
            },
            &schema,
            &interner,
        )
        .unwrap();
    let encoded_output = |local: u32| (1 << 24) | local;
    let mut bridged = combinations(&pistol);
    bridged.sort();
    let mut expected = vec![
        default_combination,
        obts(
            &[encoded_output(UNIQUE_KEYWORD)],
            &[encoded_output(CUSTOM_MOD)],
        ),
        obts(
            &[encoded_output(SECOND_UNIQUE_KEYWORD)],
            &[encoded_output(CUSTOM_MOD)],
        ),
    ];
    expected.sort();
    assert_eq!(bridged, expected);

    drop(session);
    for handle in [target, fallout4, source] {
        assert!(plugin_handle_close_native(handle));
    }
}

#[test]
fn retain_includes_drops_unconverted_rows_and_keeps_properties() {
    let mut bytes: SmallVec<[u8; 32]> =
        SmallVec::from_vec(obts(&[UNIQUE_KEYWORD], &[RECEIVER, DROPPED_MOD]));
    let property = [4u8; 24];
    bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&property);

    assert_eq!(
        retain_includes(&mut bytes, |raw| raw != DROPPED_MOD),
        Some(1)
    );

    let mut expected = obts(&[UNIQUE_KEYWORD], &[RECEIVER]);
    expected[4..8].copy_from_slice(&1u32.to_le_bytes());
    expected.extend_from_slice(&property);
    assert_eq!(bytes.to_vec(), expected);
}

/// `ma_Gun_Appearance`, which the mapper forces onto `ma_WeaponMaterialSwaps`.
const FO76_MATERIAL_KEYWORD: u32 = 0x37_D0B2;
const FO4_MATERIAL_KEYWORD: u32 = 0x24_A0D7;
const UNRELATED_KEYWORD: u32 = 0x5A_5A5A;
const VANILLA_KEYWORD: u32 = 0x01_7E69;

fn with_keywords(mut record: Record, keywords: &[u32]) -> Record {
    record.fields.push(bytes_field(
        b"KSIZ",
        u32::try_from(keywords.len())
            .unwrap()
            .to_le_bytes()
            .to_vec(),
    ));
    record.fields.push(bytes_field(
        b"KWDA",
        keywords.iter().flat_map(|raw| raw.to_le_bytes()).collect(),
    ));
    record
}

#[test]
fn material_keyword_is_carried_onto_the_override_and_other_keywords_are_not() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native(OUTPUT, Some("fo76")).unwrap();
    let fallout4 = plugin_handle_new_native(FALLOUT4, Some("fo4")).unwrap();
    let target = plugin_handle_new_native(OUTPUT, Some("fo4")).unwrap();
    plugin_handle_add_master_native(target, FALLOUT4, None).unwrap();

    let default_combination = obts(&[], &[DEFAULT_MOD]);
    let unique_combination = obts(&[UNIQUE_KEYWORD], &[CUSTOM_MOD]);
    {
        let mut session = open_session(source, None).unwrap();
        let schema = session.schema().unwrap();
        let record = with_keywords(
            weapon(
                OUTPUT,
                PISTOL,
                &[],
                &[default_combination.clone(), unique_combination],
                &interner,
            ),
            &[FO76_MATERIAL_KEYWORD, UNRELATED_KEYWORD],
        );
        session.add_record(record, &schema, &interner).unwrap();
    }
    {
        let mut session = open_session(fallout4, None).unwrap();
        let schema = session.schema().unwrap();
        let record = with_keywords(
            weapon(FALLOUT4, PISTOL, &[], &[default_combination], &interner),
            &[VANILLA_KEYWORD],
        );
        session.add_record(record, &schema, &interner).unwrap();
    }

    let mut state = MapperState::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: OUTPUT.into(),
            preserve_source_ids: true,
            ..Default::default()
        },
    );
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let output = interner.intern(OUTPUT);
    let vanilla = interner.intern(FALLOUT4);
    for (source_local, target_local, plugin) in [
        (PISTOL, PISTOL, vanilla),
        (DEFAULT_MOD, DEFAULT_MOD, vanilla),
        (FO76_MATERIAL_KEYWORD, FO4_MATERIAL_KEYWORD, vanilla),
        (UNRELATED_KEYWORD, UNRELATED_KEYWORD, vanilla),
        (UNIQUE_KEYWORD, UNIQUE_KEYWORD, output),
        (CUSTOM_MOD, CUSTOM_MOD, output),
    ] {
        mapper.add_mapping(
            FormKey {
                plugin: output,
                local: source_local,
            },
            FormKey {
                plugin,
                local: target_local,
            },
        );
    }
    let config = FixupConfig {
        target_master_handle_ids: vec![fallout4],
        ..Default::default()
    };

    let mut session = open_session(target, Some(source)).unwrap();
    BridgeFo76WeaponObjectTemplatesFixup
        .run_with_session(&mut session, &mut mapper, &config)
        .unwrap();

    let schema = session.schema().unwrap();
    let pistol = session
        .record_decoded(
            &FormKey {
                plugin: vanilla,
                local: PISTOL,
            },
            &schema,
            &interner,
        )
        .unwrap();
    let keywords: Vec<u32> = match &pistol
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"KWDA")
        .unwrap()
        .value
    {
        FieldValue::List(values) => values
            .iter()
            .map(|value| match value {
                FieldValue::FormKey(key) => key.local,
                other => panic!("KWDA entry decoded as {other:?}"),
            })
            .collect(),
        FieldValue::Bytes(bytes) => bytes
            .chunks_exact(4)
            .map(|raw| u32::from_le_bytes(raw.try_into().unwrap()))
            .collect(),
        other => panic!("KWDA decoded as {other:?}"),
    };
    assert_eq!(keywords, vec![VANILLA_KEYWORD, FO4_MATERIAL_KEYWORD]);
    let count = pistol
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"KSIZ")
        .and_then(|field| field_value_u32(&field.value));
    assert_eq!(count, Some(2));

    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(fallout4));
    assert!(plugin_handle_close_native(source));
}

/// Runs the fixup on a source weapon with no keyword-filtered combination and returns how many
/// overrides it wrote, plus the override's keyword local ids.
fn bridge_weapon_without_combinations(source_keywords: &[u32]) -> (u32, Vec<u32>) {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native(OUTPUT, Some("fo76")).unwrap();
    let fallout4 = plugin_handle_new_native(FALLOUT4, Some("fo4")).unwrap();
    let target = plugin_handle_new_native(OUTPUT, Some("fo4")).unwrap();
    plugin_handle_add_master_native(target, FALLOUT4, None).unwrap();

    let default_combination = obts(&[], &[DEFAULT_MOD]);
    {
        let mut session = open_session(source, None).unwrap();
        let schema = session.schema().unwrap();
        let record = with_keywords(
            weapon(
                OUTPUT,
                PISTOL,
                &[RECEIVER_ATTACH_POINT],
                &[default_combination.clone()],
                &interner,
            ),
            source_keywords,
        );
        session.add_record(record, &schema, &interner).unwrap();
    }
    {
        let mut session = open_session(fallout4, None).unwrap();
        let schema = session.schema().unwrap();
        let record = with_keywords(
            weapon(FALLOUT4, PISTOL, &[], &[default_combination], &interner),
            &[VANILLA_KEYWORD],
        );
        session.add_record(record, &schema, &interner).unwrap();
    }

    let mut state = MapperState::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: OUTPUT.into(),
            preserve_source_ids: true,
            ..Default::default()
        },
    );
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let output = interner.intern(OUTPUT);
    let vanilla = interner.intern(FALLOUT4);
    for (source_local, target_local) in [
        (PISTOL, PISTOL),
        (RECEIVER_ATTACH_POINT, RECEIVER_ATTACH_POINT),
        (FO76_MATERIAL_KEYWORD, FO4_MATERIAL_KEYWORD),
        (UNRELATED_KEYWORD, UNRELATED_KEYWORD),
    ] {
        mapper.add_mapping(
            FormKey {
                plugin: output,
                local: source_local,
            },
            FormKey {
                plugin: vanilla,
                local: target_local,
            },
        );
    }
    let config = FixupConfig {
        target_master_handle_ids: vec![fallout4],
        ..Default::default()
    };

    let mut session = open_session(target, Some(source)).unwrap();
    let report = BridgeFo76WeaponObjectTemplatesFixup
        .run_with_session(&mut session, &mut mapper, &config)
        .unwrap();
    let schema = session.schema().unwrap();
    let written = session
        .record_decoded(
            &FormKey {
                plugin: vanilla,
                local: PISTOL,
            },
            &schema,
            &interner,
        )
        .ok()
        .filter(|_| report.records_added > 0);
    let keywords = written
        .and_then(|record| {
            record
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"KWDA")
                .map(|field| match &field.value {
                    FieldValue::List(values) => values
                        .iter()
                        .filter_map(|value| match value {
                            FieldValue::FormKey(key) => Some(key.local),
                            _ => None,
                        })
                        .collect(),
                    other => panic!("KWDA decoded as {other:?}"),
                })
        })
        .unwrap_or_default();

    let added = report.records_added;
    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(fallout4));
    assert!(plugin_handle_close_native(source));
    (added, keywords)
}

#[test]
fn weapon_with_only_a_material_keyword_still_gets_an_override() {
    let (added, keywords) =
        bridge_weapon_without_combinations(&[FO76_MATERIAL_KEYWORD, UNRELATED_KEYWORD]);
    assert_eq!(added, 1);
    assert_eq!(keywords, vec![VANILLA_KEYWORD, FO4_MATERIAL_KEYWORD]);
}

#[test]
fn weapon_without_combinations_or_material_keyword_gets_no_override() {
    let (added, _) = bridge_weapon_without_combinations(&[UNRELATED_KEYWORD]);
    assert_eq!(added, 0);
}
