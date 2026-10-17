use super::*;
use crate::formkey_mapper::MapperOptions;

#[test]
fn muzzle_alias_preserves_animated_source_transform_and_is_idempotent() {
    let mut nif = NifFile::new("fo4");
    nif.blocks[0].set_field("Name", NifValue::String("Root".to_string()));
    let parent = nif.add_block("NiNode", None);
    nif.blocks[parent].set_field(
        "Name",
        NifValue::String("ProjectileNode_RHandLaser".to_string()),
    );
    nif.blocks[parent].set_field("Translation", NifValue::Vec3([7.0, -11.0, 13.0]));
    nif.blocks[parent].set_field(
        "Rotation",
        NifValue::Matrix33([[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]),
    );
    nif.blocks[parent].set_field("Scale", NifValue::Float(1.25));
    nif.blocks[0].set_field("Num Children", NifValue::UInt(1));
    nif.blocks[0].set_field(
        "Children",
        NifValue::Array(vec![NifValue::Ref(parent as i32)]),
    );
    let original = nif.blocks[parent].clone();
    let nodes = vec!["ProjectileNode_RHandLaser".to_string()];
    assert_eq!(
        install_projectile_node_aliases(&mut nif, &nodes).unwrap(),
        1
    );
    assert_eq!(
        install_projectile_node_aliases(&mut nif, &nodes).unwrap(),
        0
    );
    for field in ["Name", "Translation", "Rotation", "Scale"] {
        assert_eq!(
            nif.blocks[parent].fields.get(field),
            original.fields.get(field)
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("skeleton.nif");
    nif.save(Some(path.clone())).unwrap();
    let mut reopened = NifFile::load(&path).unwrap();
    assert_eq!(
        install_projectile_node_aliases(&mut reopened, &nodes).unwrap(),
        0
    );
    assert!(install_projectile_node_aliases(&mut reopened, &["Missing".to_string()]).is_err());
}

#[test]
fn embedded_slots_keep_collision_safe_targets_across_mapper_reset() {
    let interner = StringInterner::new();
    let options = MapperOptions {
        output_plugin_name: "Converted.esm".into(),
        source_plugin_name: "FalloutNV.esm".into(),
        ..Default::default()
    };
    let mut state = MapperState::new([], options.clone());
    state.reserved_generated_object_ids.extend(0x800..0x820);
    let mut cached = BTreeMap::new();
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let records = allocate_embedded_weapon_slots(
        &mut mapper,
        &mut cached,
        [
            "058717:FalloutNV.esm".to_string(),
            "018B9E:FalloutNV.esm".to_string(),
        ],
        &interner,
    );
    assert_eq!(records.len(), 2);
    assert!(cached.values().all(|target| target.local >= 0x820));
    let previous = cached.clone();
    let mut reset = MapperState::new([], options);
    let mut mapper = FormKeyMapper::from_state(&mut reset, &interner);
    assert!(
        allocate_embedded_weapon_slots(
            &mut mapper,
            &mut cached,
            ["058717:falloutnv.esm".to_string()],
            &interner
        )
        .is_empty()
    );
    assert_eq!(cached, previous);
    for _ in 0..40 {
        let next = mapper.allocate_generated();
        assert!(!cached.values().any(|target| *target == next));
    }
}

#[test]
fn shared_muzzle_weapons_keep_distinct_slots_and_sparse_candidate_ownership() {
    use super::super::creature_catalog::StableFormKey;
    use super::super::creature_weapon_attacks::weapon_attack_evidence;
    let interner = StringInterner::new();
    let mut state = MapperState::new(
        [],
        MapperOptions {
            source_plugin_name: "FalloutNV.esm".into(),
            output_plugin_name: "Converted.esm".into(),
            ..Default::default()
        },
    );
    let mut cached = BTreeMap::new();
    let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
    let slots = allocate_embedded_weapon_slots(
        &mut mapper,
        &mut cached,
        ["057E8E:FalloutNV.esm".into(), "057E8F:FalloutNV.esm".into()],
        &interner,
    );
    let mut weapons = Vec::new();
    for (index, local) in [0x057E8E, 0x057E8F].into_iter().enumerate() {
        let source = FormKey {
            local,
            plugin: interner.intern("FalloutNV.esm"),
        };
        mapper.add_mapping(
            source,
            FormKey {
                local: 0x900 + index as u32,
                plugin: interner.intern("Converted.esm"),
            },
        );
        let mut record = Record::new(SigCode(*b"WEAP"), source);
        let mut dnam = vec![0; 204];
        dnam[..4].copy_from_slice(&9u32.to_le_bytes());
        dnam[4..8].copy_from_slice(&1f32.to_le_bytes());
        dnam[60..64].copy_from_slice(&1f32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(dnam.into()),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"NNAM"),
            value: FieldValue::String(interner.intern("ProjectileNode_RGatling")),
        });
        let mut evidence = weapon_attack_evidence(&record, &interner).unwrap();
        evidence.equipment_group = Some(evidence.source.clone());
        evidence.equipment_index = Some(index as u8);
        bind_embedded_weapon_slot(&mut record, &interner);
        assert_eq!(
            record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "ETYP")
                .unwrap()
                .value,
            FieldValue::FormKey(embedded_slot_source_key(
                &format!("{local:06X}:FalloutNV.esm"),
                &interner
            ))
        );
        weapons.push(evidence);
    }
    let bindings = family_equipment_slots(
        &weapons,
        &[StableFormKey {
            local: 0x057E8F,
            plugin: "FalloutNV.esm".into(),
        }],
        &state,
        &interner,
    )
    .unwrap();
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].node, bindings[1].node);
    assert_ne!(
        bindings[0].equip_slot.form_key,
        bindings[1].equip_slot.form_key
    );
    assert!(bindings[0].weapons.is_empty());
    assert_eq!(bindings[0].weapon_types, 0);
    assert_eq!(bindings[1].weapon_types, 1 << 9);
    assert_eq!(bindings[1].weapons[0].form_key.local, 0x901);
    assert_eq!(slots.len(), 2);
    assert_eq!(family_muzzle_nodes(&weapons), ["ProjectileNode_RGatling"]);

    for weapon in &mut weapons {
        weapon.embedded_weapon_node = None;
        weapon.held_weapon_node = Some("Bip01 R Hand".to_string());
        weapon.equipment_group = None;
        weapon.equipment_index = Some(0);
    }
    weapons[0].animation.animation_type = 2;
    let owned = weapons.iter().map(|weapon| weapon.source.clone()).collect::<Vec<_>>();
    let held = family_equipment_slots(&weapons, &owned, &state, &interner).unwrap();
    assert_eq!(held.len(), 2);
    assert_eq!(held[0].equip_slot.form_key, TargetFormKey::new(0x013F42, "Fallout4.esm"));
    assert_eq!(held[1].equip_slot.form_key, TargetFormKey::new(0x013F43, "Fallout4.esm"));
    assert_eq!(held[0].weapon_types, (1 << 5) | (1 << 9));
    assert_eq!(held[0].weapons.len(), 2);
    assert!(held.iter().all(|slot| slot.preserve_weapon_slots));
    assert!(held[1].node.is_empty() && held[1].weapons.is_empty());
    assert!(family_muzzle_nodes(&weapons).is_empty());
    let mut nif = NifFile::new("fo4");
    assert!(validate_weapon_attachment_nodes(&nif, &weapons).is_err());
    nif.blocks[0].set_field("Name", NifValue::String("Bip01 R Hand".to_string()));
    validate_weapon_attachment_nodes(&nif, &weapons).unwrap();

    weapons[0].animation.animation_type = 10;
    weapons[0].equipment_index = Some(2);
    let held = family_equipment_slots(&weapons, &owned, &state, &interner).unwrap();
    assert_eq!(held.len(), 3);
    assert_eq!(held[2].equip_slot.form_key, TargetFormKey::new(0x046AAC, "Fallout4.esm"));
    assert_eq!(held[2].weapon_types, 1 << 10);

    weapons[0].animation.animation_type = 2;
    weapons[0].equipment_index = Some(0);
    weapons[0].held_weapon_node = Some("Bip01 R Foretwist".into());
    for (index, node) in [(0, "Bip01 R Foretwist"), (1, "Bip01 R Hand")] {
        let held = family_equipment_slots(
            &weapons, &[weapons[index].source.clone()], &state, &interner,
        ).unwrap();
        assert_eq!(held.len(), 2);
        assert_eq!(held[0].node, node);
        assert_eq!(held[0].weapons.len(), 1);
    }
    assert!(family_equipment_slots(&weapons, &owned, &state, &interner)
        .unwrap_err().contains("conflicting owned held weapon attachments"));
}
