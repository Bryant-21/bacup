use super::*;

#[test]
fn perk_names_and_object_ids_are_not_cross_game_effect_identity() {
    let interner = StringInterner::new();
    let sig = SigCode(*b"PERK");
    for target_id in [0x800, 0x900] {
        let source = FormKey::parse("000800@SeventySix.esm", &interner).unwrap();
        let target = FormKey::parse(&format!("{target_id:06X}@Fallout4.esm"), &interner).unwrap();
        let eid = interner.intern("sameperk01");
        assert!(
            source_target_mappings_from_preflight(
                [(eid, source, sig)],
                &[(eid, target, sig)],
                &interner,
                Game::Fo76,
                Game::Fo4
            )
            .is_empty()
        );
        let mut mapper = FormKeyMapper::new(
            [(eid, target, sig)],
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
        assert_eq!(mapper.allocate_or_resolve(source, Some(eid), sig), source);
        let index = [(eid, vec![(target, sig)])].into_iter().collect();
        assert!(
            target_collision_donor_form_key(&index, &[], &interner, "sameperk01", sig).is_none()
        );
        let mut record = Record::new(sig, source);
        record.eid = Some(eid);
        assert_eq!(
            rename_fo76_target_editor_id_collision(
                &mut record,
                &index,
                &FxHashSet::default(),
                &[],
                &interner,
                is_editor_id_collision_rename_forced(Game::Fo76, Game::Fo4, sig)
            ),
            Some(("sameperk01".into(), "sameperk01fo76".into()))
        );
    }
}
