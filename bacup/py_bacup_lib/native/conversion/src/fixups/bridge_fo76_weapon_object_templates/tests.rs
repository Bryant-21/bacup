use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
};

const OUTPUT: &str = "SeventySix.esm";
const FALLOUT4: &str = "Fallout4.esm";
/// `44`, the .44 pistol: shared with Fallout4.esm, so the mapper reuses vanilla's record.
const PISTOL: u32 = 0x0CE97D;
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
    slots: &[u32],
    combinations: &[Vec<u8>],
    interner: &StringInterner,
) -> Record {
    let mut record = Record::new(
        SigCode(*b"WEAP"),
        FormKey {
            plugin: interner.intern(plugin),
            local: PISTOL,
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
