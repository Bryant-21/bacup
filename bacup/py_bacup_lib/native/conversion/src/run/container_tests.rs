use super::*;

const CONTAINERS: &[(u32, &str, u32)] = &[
    (0x08C17D, "Vault_Locker_01", 0x0673B8),
    (0x1A720B, "HighTechFilingCabinet01AWeatheredTall", 0x0C446C),
];

#[test]
fn fo76_container_preflight_preserves_inventories_and_unique_editor_ids() {
    {
        let interner = StringInterner::new();
        let sig = SigCode::from_str("CONT").unwrap();
        for &(id, editor_id, _) in CONTAINERS {
            let source = FormKey::parse(&format!("{id:06X}@SeventySix.esm"), &interner).unwrap();
            let target = FormKey::parse(&format!("{id:06X}@Fallout4.esm"), &interner).unwrap();
            let eid = interner.intern(editor_id);
            let target_eid = interner.intern(&editor_id.to_ascii_lowercase());
            assert!(
                source_target_mappings_from_preflight(
                    [(eid, source, sig)],
                    &[(target_eid, target, sig)],
                    &interner,
                    Game::Fo76,
                    Game::Fo4,
                )
                .is_empty(),
                "{editor_id} must keep its source inventory"
            );
            let mut mapper = FormKeyMapper::new(
                [(target_eid, target, sig)],
                MapperOptions {
                    output_plugin_name: "SeventySix.esm".into(),
                    use_base_game_assets: true,
                    preserve_source_ids: true,
                    vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                        Game::Fo76,
                        Game::Fo4,
                    ),
                    ..Default::default()
                },
                &interner,
            );
            assert_eq!(
                mapper.allocate_or_resolve(source, Some(target_eid), sig),
                source
            );
            assert_eq!(mapper.lookup(source), Some(source));
        }
    }
    {
        let interner = StringInterner::new();
        let sig = SigCode::from_str("CONT").unwrap();
        for &(id, editor_id, loot_id) in CONTAINERS {
            let source = FormKey::parse(&format!("{id:06X}@SeventySix.esm"), &interner).unwrap();
            let target = FormKey::parse(&format!("{id:06X}@Fallout4.esm"), &interner).unwrap();
            let loot = FormKey::parse(&format!("{loot_id:06X}@SeventySix.esm"), &interner).unwrap();
            let mut record = Record::new(sig, source);
            record.eid = Some(interner.intern(editor_id));
            let contents = FieldValue::Struct(vec![
                (interner.intern("item"), FieldValue::FormKey(loot)),
                (interner.intern("count"), FieldValue::Uint(1)),
            ]);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("CNTO").unwrap(),
                value: contents.clone(),
            });
            let index = [(
                interner.intern(&editor_id.to_ascii_lowercase()),
                vec![(target, sig)],
            )]
            .into_iter()
            .collect();
            assert_eq!(
                rename_fo76_target_editor_id_collision(
                    &mut record,
                    &index,
                    &FxHashSet::default(),
                    &[],
                    &interner,
                    is_editor_id_collision_rename_forced(Game::Fo76, Game::Fo4, sig),
                ),
                Some((editor_id.to_owned(), format!("{editor_id}fo76")))
            );
            let mut donor = Record::new(sig, target);
            donor.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("CNTO").unwrap(),
                value: FieldValue::Uint(99),
            });
            crate::collision_donor::merge_target_collision_donor(&mut record, &donor, &interner);
            assert_eq!(
                record
                    .fields
                    .iter()
                    .find(|f| f.sig.as_str() == "CNTO")
                    .unwrap()
                    .value,
                contents
            );
        }
    }
    {
        assert!(allows_source_target_preflight_remap(
            Game::Fo76,
            Game::Fo4,
            SigCode::from_str("CONT").unwrap(),
            "ContainerMarker",
        ));
        let interner = StringInterner::new();
        let sig = SigCode::from_str("CONT").unwrap();
        let source = FormKey::parse("000800@SeventySix.esm", &interner).unwrap();
        let target = FormKey::parse("000800@Fallout4.esm", &interner).unwrap();
        let eid = interner.intern("containermarker");
        let index = [(eid, vec![(target, sig)])].into_iter().collect();
        let mut record = Record::new(sig, source);
        record.eid = Some(eid);
        assert_eq!(
            rename_fo76_target_editor_id_collision(
                &mut record,
                &index,
                &FxHashSet::default(),
                &[],
                &interner,
                is_editor_id_collision_rename_forced(Game::Fo76, Game::Fo4, sig),
            ),
            None
        );
    }
}
