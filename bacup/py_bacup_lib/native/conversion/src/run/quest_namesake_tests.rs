use super::*;

// FO76's public event "Line in the Sand" and Fallout 4's radiant "Cleansing the
// Commonwealth" share the EditorID BoSr01 at different object ids.
const LINE_IN_THE_SAND: &str = "311433@SeventySix.esm";
const CLEANSING_THE_COMMONWEALTH: &str = "064EC7@Fallout4.esm";
const FO76_PERKS_QUEST: &str = "04A09E@SeventySix.esm";
const FO4_PERKS_QUEST: &str = "04A09E@Fallout4.esm";

fn quest_sig() -> SigCode {
    SigCode::from_str("QUST").unwrap()
}

fn fk(text: &str, interner: &StringInterner) -> FormKey {
    FormKey::parse(text, interner).unwrap()
}

#[test]
fn fo76_quest_namesakes_are_not_seeded_and_keep_their_own_vmad() {
    {
        let interner = StringInterner::new();
        let sig = quest_sig();
        let mappings = source_target_mappings_from_preflight(
            [
                (
                    interner.intern("BoSr01"),
                    fk(LINE_IN_THE_SAND, &interner),
                    sig,
                ),
                (
                    interner.intern("PerksQuest"),
                    fk(FO76_PERKS_QUEST, &interner),
                    sig,
                ),
            ],
            &[
                (
                    interner.intern("bosr01"),
                    fk(CLEANSING_THE_COMMONWEALTH, &interner),
                    sig,
                ),
                (
                    interner.intern("perksquest"),
                    fk(FO4_PERKS_QUEST, &interner),
                    sig,
                ),
            ],
            &interner,
            Game::Fo76,
            Game::Fo4,
        );

        assert_eq!(
            mappings,
            vec![(
                fk(FO76_PERKS_QUEST, &interner),
                fk(FO4_PERKS_QUEST, &interner)
            )]
        );
    }
    {
        let interner = StringInterner::new();
        let sig = quest_sig();
        let bosr01 = interner.intern("bosr01");
        let perks_quest = interner.intern("perksquest");
        let mut mapper = FormKeyMapper::new(
            [
                (bosr01, fk(CLEANSING_THE_COMMONWEALTH, &interner), sig),
                (perks_quest, fk(FO4_PERKS_QUEST, &interner), sig),
            ],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                vanilla_remap_blocked_signatures: editor_id_vanilla_remap_blocked_sigs(
                    Game::Fo76,
                    Game::Fo4,
                ),
                shared_dna_remap_signatures: editor_id_shared_dna_remap_sigs(Game::Fo76, Game::Fo4),
                ..Default::default()
            },
            &interner,
        );

        let line_in_the_sand = fk(LINE_IN_THE_SAND, &interner);
        assert_eq!(
            mapper.allocate_or_resolve(line_in_the_sand, Some(bosr01), sig),
            line_in_the_sand
        );
        assert_eq!(
            mapper.allocate_or_resolve(fk(FO76_PERKS_QUEST, &interner), Some(perks_quest), sig),
            fk(FO4_PERKS_QUEST, &interner)
        );
        assert!(editor_id_shared_dna_remap_sigs(Game::SkyrimSe, Game::Fo4).is_empty());
    }
    {
        let interner = StringInterner::new();
        let sig = quest_sig();
        let shared_dna = editor_id_shared_dna_remap_sigs(Game::Fo76, Game::Fo4);
        let cleansing = fk(CLEANSING_THE_COMMONWEALTH, &interner);
        let index = [
            (interner.intern("bosr01"), vec![(cleansing, sig)]),
            (
                interner.intern("perksquest"),
                vec![(fk(FO4_PERKS_QUEST, &interner), sig)],
            ),
        ]
        .into_iter()
        .collect();
        let vmad = FieldEntry {
            sig: SubrecordSig::from_str("VMAD").unwrap(),
            value: FieldValue::Bytes(b"BoSr01_Script".as_slice().into()),
        };
        let mut record = Record::new(sig, fk(LINE_IN_THE_SAND, &interner));
        record.eid = Some(interner.intern("BoSr01"));
        record.fields.push(vmad.clone());

        assert_eq!(
            rename_fo76_target_editor_id_collision(
                &mut record,
                &index,
                &FxHashSet::default(),
                &shared_dna,
                &interner,
                is_editor_id_collision_rename_forced(Game::Fo76, Game::Fo4, sig),
            ),
            Some(("BoSr01".to_owned(), "BoSr01fo76".to_owned()))
        );
        assert_eq!(
            target_collision_donor_form_key(&index, &shared_dna, &interner, "BoSr01", sig),
            None
        );
        assert_eq!(
            target_collision_donor_form_key(&index, &[], &interner, "BoSr01", sig),
            Some(cleansing)
        );
        assert_eq!(
            record.fields.iter().find(|field| field.sig == vmad.sig),
            Some(&vmad)
        );

        let mut inherited = Record::new(sig, fk(FO76_PERKS_QUEST, &interner));
        inherited.eid = Some(interner.intern("PerksQuest"));
        assert_eq!(
            rename_fo76_target_editor_id_collision(
                &mut inherited,
                &index,
                &FxHashSet::default(),
                &shared_dna,
                &interner,
                false,
            ),
            None
        );
    }
}
