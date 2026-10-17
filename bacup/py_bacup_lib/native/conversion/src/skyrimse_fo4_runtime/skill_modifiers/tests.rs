use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState, ResolutionMode};
use crate::schema::AuthoringSchema;
use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, plugin_handle_add_master_native, plugin_handle_close_native,
    plugin_handle_load_no_py, plugin_handle_new_no_py, plugin_handle_save_no_py,
    plugin_handle_store_ref,
};

#[test]
fn skill_modifier_bindings_use_reserved_output_ids_and_survive_mapper_reset() {
    let interner = StringInterner::new();
    let options = MapperOptions {
        output_plugin_name: "Converted.esm".into(),
        source_plugin_name: "Skyrim.esm".into(),
        target_master_names: vec!["Fallout4.esm".into()],
        resolution_mode: ResolutionMode::Strict,
        ..Default::default()
    };
    let mut state = MapperState::new([], options.clone());
    state.reserved_generated_object_ids.extend(0x800..0x810);
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let (targets, records) = allocate_skill_modifier_records(&mut mapper, None, &interner);
    assert_eq!(records.len(), 5);
    assert!(targets.iter().all(|key| key.local >= 0x810));
    for (index, (number, name, _)) in MODIFIERS.iter().enumerate() {
        assert_eq!(
            skill_modifier_source_key(&FieldValue::Uint((*number).into()), &interner),
            Some(source_key(index, &interner))
        );
        assert_eq!(
            skill_modifier_source_key(&FieldValue::String(interner.intern(name)), &interner),
            Some(source_key(index, &interner))
        );
    }
    let mut effect = Record::new(SigCode(*b"MGEF"), mapper.allocate_generated());
    effect.fields.push(field(
        *b"DATA",
        structure(
            &interner,
            [
                ("ActorValue", FieldValue::FormKey(source_key(0, &interner))),
                ("ActorValue1", FieldValue::FormKey(source_key(1, &interner))),
                (
                    "PerkToApply",
                    FieldValue::FormKey(skill_modifier_perk_source_key(&interner)),
                ),
            ],
        ),
    ));
    mapper.rewrite_record(&mut effect).unwrap();
    let FieldValue::Struct(values) = &effect.fields[0].value else {
        panic!()
    };
    assert_eq!(values[0].1, FieldValue::FormKey(targets[0]));
    assert_eq!(values[1].1, FieldValue::FormKey(targets[1]));
    assert_eq!(values[2].1, FieldValue::FormKey(targets[4]));
    for index in 5..11 {
        let mut keyword = Record::new(SigCode(*b"KYWD"), source_key(index, &interner));
        assert_eq!(
            mapper.allocate_or_resolve(keyword.form_key, keyword.eid, keyword.sig),
            targets[index]
        );
        keyword.fields.push(field(
            *b"KWDA",
            FieldValue::List(vec![FieldValue::FormKey(source_key(index, &interner))]),
        ));
        mapper.rewrite_record(&mut keyword).unwrap();
        assert_eq!(
            keyword.fields[0].value,
            FieldValue::List(vec![FieldValue::FormKey(targets[index])])
        );
    }

    let mut reset_state = MapperState::new([], options);
    let mut reset_mapper = FormKeyMapper::from_state(&mut reset_state, &interner);
    let (restored, duplicate_records) =
        allocate_skill_modifier_records(&mut reset_mapper, Some(targets), &interner);
    assert_eq!(restored, targets);
    assert!(duplicate_records.is_empty());
    for _ in 0..32 {
        assert!(!targets.contains(&reset_mapper.allocate_generated()));
    }
}

#[test]
fn detects_skill_modifiers_before_and_after_source_struct_relayout() {
    let interner = StringInterner::new();
    let mut record = Record::new(SigCode(*b"MGEF"), source_key(0, &interner));
    let mut raw = vec![0; 152];
    raw[68..72].copy_from_slice(&96u32.to_le_bytes());
    raw[88..92].copy_from_slice(&97u32.to_le_bytes());
    record.fields.push(field(*b"DATA", bytes(&raw)));
    assert!(record_uses_skill_modifiers(&record, &interner));
    record.fields[0].value = structure(
        &interner,
        [
            ("ActorValue", FieldValue::Uint(103)),
            (
                "SecondActorValue",
                FieldValue::String(interner.intern("LockpickingModifier")),
            ),
        ],
    );
    assert!(record_uses_skill_modifiers(&record, &interner));
    record.fields[0].value = bytes(&[0; 152]);
    assert!(!record_uses_skill_modifiers(&record, &interner));
}

fn parsed_records<'a>(items: &'a [ParsedItem], output: &mut Vec<&'a ParsedRecord>) {
    for item in items {
        match item {
            ParsedItem::Record(record) => output.push(record),
            ParsedItem::Group(group) => parsed_records(&group.children, output),
        }
    }
}

#[test]
fn skill_modifier_perk_roundtrip_preserves_consumers_and_additive_disease_stacking() {
    let mut interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let mut state = MapperState::new(
        [],
        MapperOptions {
            output_plugin_name: "Converted.esm".into(),
            source_plugin_name: "Skyrim.esm".into(),
            target_master_names: vec!["Fallout4.esm".into()],
            ..Default::default()
        },
    );
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let (targets, records) = allocate_skill_modifier_records(&mut mapper, None, &interner);
    let handle = plugin_handle_new_no_py("Converted.esm", Some("fo4"));
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    for record in records {
        crate::target_write::add_record_native(handle, record, &schema, &interner).unwrap();
    }
    let mut existing_keyword = Record::new(SigCode(*b"KYWD"), targets[5]);
    existing_keyword.eid = Some(interner.intern("WeapTypeMace"));
    crate::target_write::add_record_native(handle, existing_keyword, &schema, &interner).unwrap();
    assert_eq!(
        ensure_keyword_records(handle, targets, &schema, &mut interner).unwrap(),
        5
    );
    assert_eq!(
        ensure_keyword_records(handle, targets, &schema, &mut interner).unwrap(),
        0
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Converted.esm");
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    assert!(plugin_handle_close_native(handle));
    let reopened =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    let store = plugin_handle_store_ref().lock().unwrap();
    let slot = store.get(&reopened).unwrap();
    assert_eq!(slot.parsed.header.masters.as_slice(), ["Fallout4.esm"]);
    let mut saved = Vec::new();
    parsed_records(&slot.parsed.root_items, &mut saved);
    assert_eq!(
        saved
            .iter()
            .filter(|record| record.signature.as_str() == "AVIF")
            .count(),
        4
    );
    assert_eq!(
        saved
            .iter()
            .filter(|record| record.signature.as_str() == "KYWD")
            .count(),
        6
    );
    let perk = saved
        .iter()
        .find(|record| record.signature.as_str() == "PERK")
        .unwrap();
    let data = perk
        .subrecords
        .iter()
        .filter(|field| field.signature.as_str() == "DATA")
        .collect::<Vec<_>>();
    assert_eq!(data.len(), 5);
    assert_eq!(data[0].data.as_ref(), [0, 0, 1, 0, 1]);
    let payloads = perk
        .subrecords
        .iter()
        .filter(|field| field.signature.as_str() == "EPFD")
        .collect::<Vec<_>>();
    assert_eq!(payloads.len(), 4);
    for (index, field) in payloads.iter().enumerate() {
        assert_eq!(data[index + 1].data.len(), 3);
        assert_eq!(
            schema
                .enum_def("entry_points_enum")
                .unwrap()
                .token_for_value(data[index + 1].data[0].into()),
            Some(
                [
                    "mod_weapon_attack_damage",
                    "mod_weapon_attack_damage",
                    "mod_pickpocket_chance",
                    "mod_lockpick_sweet_spot"
                ][index]
            )
        );
        assert_eq!(data[index + 1].data[1], 14);
        assert_eq!(field.data.len(), 8);
        let av = u32::from_le_bytes(field.data[0..4].try_into().unwrap());
        assert_eq!(av, 0x0100_0000 | targets[index].local);
        let factor = f32::from_le_bytes(field.data[4..8].try_into().unwrap());
        assert_eq!(factor, 0.01);
        for (active_magnitudes, expected) in [
            (&[25.0, 15.0][..], 0.6),
            (&[15.0][..], 0.85),
            (&[][..], 1.0),
        ] {
            let actor_value: f32 = -active_magnitudes.iter().sum::<f32>();
            assert!(((1.0 + actor_value * factor) - expected).abs() < 0.00001);
        }
    }
    let conditions = perk
        .subrecords
        .iter()
        .filter(|field| field.signature.as_str() == "CTDA")
        .collect::<Vec<_>>();
    assert_eq!(conditions.len(), 6);
    for (index, field) in conditions.into_iter().enumerate() {
        assert_eq!(field.data.len(), 32);
        assert_eq!(field.data[0], if index % 3 < 2 { 1 } else { 0 });
        assert_eq!(
            u32::from_le_bytes(field.data[8..12].try_into().unwrap()),
            560
        );
        assert_eq!(
            u32::from_le_bytes(field.data[12..16].try_into().unwrap()),
            0x0100_0000 | targets[5 + index].local
        );
    }
    drop(store);
    assert!(plugin_handle_close_native(reopened));
}
