use super::*;
use crate::formkey_mapper::MapperOptions;
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::{plugin_handle_close_native, plugin_handle_new_native};

fn fk(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        plugin: interner.intern("SeventySix.esm"),
        local,
    }
}

#[test]
fn buff_perks_survive_serialization_and_refresh_is_idempotent() {
    let interner = StringInterner::new();
    let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
    let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
    let mut session = open_session(target, Some(source)).unwrap();
    session
        .target_slot_mut()
        .parsed
        .header
        .masters
        .push("Fallout4.esm".into());
    let schema = session.schema().unwrap();
    let mut mapper = FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            source_plugin_name: "SeventySix.esm".into(),
            output_plugin_name: "SeventySix.esm".into(),
            ..MapperOptions::default()
        },
        &interner,
    );
    for (local, actor_value) in [
        (0x76B52D, 0x01900001u32),
        (0x89ADB4, 0x01900002),
        (0x897406, 0x000BA447),
    ] {
        mapper.add_mapping(fk(local, &interner), fk(local, &interner));
        let mut data = vec![0; 152];
        data[64..68].copy_from_slice(&2u32.to_le_bytes());
        data[MGEF_ACTOR_VALUE..MGEF_ACTOR_VALUE + 4].copy_from_slice(&actor_value.to_le_bytes());
        let mut effect = Record::new(SigCode(*b"MGEF"), fk(local, &interner));
        effect.fields.push(raw(b"DATA", data));
        session.add_record(effect, &schema, &interner).unwrap();
    }
    let mut spell = Record::new(SigCode(*b"SPEL"), fk(0x897408, &interner));
    mapper.add_mapping(spell.form_key, spell.form_key);
    let mut efit = 0.25f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&3600u32.to_le_bytes());
    spell.fields.extend([
        FieldEntry {
            sig: SubrecordSig(*b"EFID"),
            value: FieldValue::FormKey(fk(0x897406, &interner)),
        },
        raw(b"EFIT", efit),
    ]);
    session.add_record(spell, &schema, &interner).unwrap();
    let report = repair_buff_effects(&mut session, &mut mapper).unwrap();
    assert_eq!(report.records_added, 2);
    assert_eq!(report.records_changed, 4);
    for (local, av, point, tabs) in [
        (0x76B52D, 0x01900001, 22, 3),
        (0x89ADB4, 0x01900002, 123, 1),
    ] {
        let data = session
            .first_subrecord_bytes(&fk(local, &interner), "DATA")
            .unwrap()
            .unwrap();
        let raw_perk = read_u32(&data, MGEF_PERK);
        assert_eq!(raw_perk >> 24, 1);
        let perk_fk = fk(raw_perk & 0xFFFFFF, &interner);
        let perk = session
            .record_decoded(&perk_fk, &schema, &interner)
            .unwrap();
        let entry_data = perk
            .fields
            .iter()
            .skip_while(|entry| entry.sig.0 != *b"PRKE")
            .find(|entry| entry.sig.0 == *b"DATA")
            .unwrap();
        assert_eq!(entry_data.value, raw(b"DATA", [point, 14, tabs]).value);
        let parameter = session
            .first_subrecord_bytes(&perk_fk, "EPFD")
            .unwrap()
            .unwrap();
        assert_eq!(read_u32(&parameter, 0), av);
        assert_eq!(
            f32::from_le_bytes(parameter[4..8].try_into().unwrap()),
            0.01
        );
        assert_eq!(
            session
                .first_subrecord_bytes(&perk_fk, "DATA")
                .unwrap()
                .unwrap(),
            [0, 0, 1, 0, 1]
        );
    }
    let efit = session
        .first_subrecord_bytes(&fk(0x897408, &interner), "EFIT")
        .unwrap()
        .unwrap();
    assert_eq!(f32::from_le_bytes(efit[..4].try_into().unwrap()), 25.0);
    assert_eq!(read_u32(&efit, 8), 3600);
    let data = session
        .first_subrecord_bytes(&fk(0x897406, &interner), "DATA")
        .unwrap()
        .unwrap();
    assert_eq!(read_u32(&data, MGEF_PERK), 0x0BA448);
    let report = repair_buff_effects(&mut session, &mut mapper).unwrap();
    assert_eq!(report.records_added, 0);
    assert_eq!(report.records_changed, 0);
    drop(session);
    assert!(plugin_handle_close_native(target));
    assert!(plugin_handle_close_native(source));
}

#[test]
fn restores_xp_and_loot_entry_parameters_without_changing_conditions() {
    let interner = StringInterner::new();
    let mut mapper = FormKeyMapper::new(std::iter::empty(), MapperOptions::default(), &interner);
    mapper.add_mapping(fk(0x8D1C98, &interner), fk(0x910001, &interner));
    for (local, entry_point, function, tabs, parameter, is_form) in [
        (0x3CD031, 22, 3, 3, 1.05f32.to_le_bytes(), false),
        (0x8D1C9B, 9, 8, 2, 0x8D1C98u32.to_le_bytes(), true),
    ] {
        let mut source = Record::new(SigCode(*b"PERK"), fk(local, &interner));
        source.fields.extend([
            raw(b"DATA", [0, 0, 1]),
            raw(b"PRKE", [2, 0]),
            raw(b"DATA", [entry_point, function, tabs + 1, 0]),
            raw(b"EPFD", parameter),
            raw(b"PRKF", []),
        ]);
        let condition = raw(b"CTDA", vec![42; 32]);
        let mut target = source.clone();
        target.fields.retain(|entry| entry.sig.0 != *b"EPFD");
        target.fields.insert(3, condition.clone());
        assert!(
            restore_perk_entry(
                &source,
                &mut target,
                tabs,
                is_form,
                &mapper,
                &["Fallout4.esm".into()]
            )
            .unwrap()
        );
        assert!(target.fields.contains(&condition));
        assert_eq!(
            target.fields[2].value,
            raw(b"DATA", [entry_point, function, tabs]).value
        );
        let expected = if is_form {
            0x01910001u32.to_le_bytes()
        } else {
            parameter
        };
        assert_eq!(field(&target, b"EPFD"), Some(&raw(b"EPFD", expected).value));
        assert!(
            !restore_perk_entry(
                &source,
                &mut target,
                tabs,
                is_form,
                &mapper,
                &["Fallout4.esm".into()]
            )
            .unwrap()
        );
    }
}

#[test]
fn rest_bonus_keeps_the_spotlight_and_homebody_gates_on_each_effect() {
    let interner = StringInterner::new();
    let mut mapper = FormKeyMapper::new(std::iter::empty(), MapperOptions::default(), &interner);
    for (source, target) in [(0x4836A3, 0x910001), (0x393F6E, 0x910002)] {
        mapper.add_mapping(fk(source, &interner), fk(target, &interner));
    }
    let mut source = Record::new(SigCode(*b"SPEL"), fk(0x3CD033, &interner));
    for index in 0..4 {
        source.fields.push(FieldEntry {
            sig: SubrecordSig(*b"EFID"),
            value: FieldValue::FormKey(fk(0x3CD032, &interner)),
        });
        source.fields.push(raw(b"EFIT", vec![0; 12]));
        let mut condition = vec![0; 32];
        condition[0] = if index % 2 == 0 { 0x20 } else { 0 };
        condition[4..8].copy_from_slice(&1.0f32.to_le_bytes());
        condition[8..10].copy_from_slice(&74u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0x4836A3u32.to_le_bytes());
        source.fields.push(raw(b"CTDA", condition.clone()));
        if index >= 2 {
            condition[0] = 0;
            condition[8..10].copy_from_slice(&448u16.to_le_bytes());
            condition[12..16].copy_from_slice(&0x393F6Eu32.to_le_bytes());
            source.fields.push(raw(b"CTDA", condition));
        }
    }
    let mut target = source.clone();
    target.fields.retain(|entry| entry.sig.0 != *b"CTDA");
    let masters = ["Fallout4.esm".to_string()];
    assert!(restore_rest_conditions(&source, &mut target, &mapper, &masters).unwrap());
    let refs: Vec<_> = target
        .fields
        .iter()
        .filter_map(|entry| {
            if entry.sig.0 != *b"CTDA" {
                return None;
            }
            let FieldValue::Bytes(bytes) = &entry.value else {
                panic!()
            };
            Some(read_u32(bytes, 12))
        })
        .collect();
    assert_eq!(
        refs,
        [
            0x01910001, 0x01910001, 0x01910001, 0x01910002, 0x01910001, 0x01910002
        ]
    );
    assert!(!restore_rest_conditions(&source, &mut target, &mapper, &masters).unwrap());
    let handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
    let mut session = open_session(handle, None).unwrap();
    session.target_slot_mut().parsed.header.masters = masters.to_vec();
    let schema = session.schema().unwrap();
    session.add_record(target, &schema, &interner).unwrap();
    let decoded = session
        .record_decoded(&source.form_key, &schema, &interner)
        .unwrap();
    assert_eq!(
        decoded
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"CTDA")
            .count(),
        6
    );
    drop(session);
    assert!(plugin_handle_close_native(handle));
}
