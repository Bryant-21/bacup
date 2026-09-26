use super::*;
use crate::schema::AuthoringSchema;

fn race(projects: &[&str], ungendered: bool, interner: &StringInterner) -> Record {
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let layout =
        schema.struct_field_layout_versioned("RACE", "DATA", Some(FO4_TARGET_FORM_VERSION));
    let size = layout.iter().map(|f| f.offset + f.width).max().unwrap();
    let offset = layout
        .iter()
        .find(|f| f.field_id == "flags_2")
        .unwrap()
        .offset;
    let mut data = vec![0; size];
    if ungendered {
        data[offset..offset + 4].copy_from_slice(&(1u32 << 9).to_le_bytes());
    }
    let mut record = Record::new(
        SigCode::from_str("RACE").unwrap(),
        FormKey {
            plugin: interner.intern("B21_Test.esp"),
            local: 0x111233,
        },
    );
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("DATA").unwrap(),
        value: FieldValue::Bytes(data.into()),
    });
    for project in projects {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(interner.intern(project)),
        });
    }
    record
}

#[test]
fn graphs_follow_race_project_without_selecting_source_additives() {
    {
        let interner = StringInterner::new();
        let mut ogua = race(&["actors/megasloth/megaslothproject.hkx"], true, &interner);
        ogua.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SADD").unwrap(),
            value: FieldValue::FormKey(FormKey {
                plugin: ogua.form_key.plugin,
                local: 0x13BA06,
            }),
        });
        assert!(!uses_target_project(&ogua, true));
        let mut human = ogua;
        human.fields.last_mut().unwrap().value = FieldValue::FormKey(FormKey {
            plugin: interner.intern("Fallout4.esm"),
            local: 0x166729,
        });
        assert!(uses_target_project(&human, true));
        assert!(uses_target_project(&human, false));
    }
    {
        let interner = StringInterner::new();
        let mut block = SubgraphBlock {
            behaviour_graph: interner.intern("Actors/Shared/Behaviors/AmbushBehavior.hkx"),
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: Some(smallvec::smallvec![1, 0, 0, 0]),
        };
        for dir in [
            "actors/character",
            "actors/powerarmor",
            "actors/dlc01/createabot",
        ] {
            let record = race(&[&format!("{dir}/project.hkx")], true, &interner);
            let selected = target_project_directory(&record, &interner).unwrap();
            assert_eq!(selected, dir);
            assert_eq!(
                perspective_project_directory(&selected, &block, &interner),
                dir
            );
        }
        block.flags_bytes = Some(smallvec::smallvec![1, 0, 1, 0]);
        assert_eq!(
            perspective_project_directory("actors/powerarmor", &block, &interner),
            "actors/powerarmor/_1stperson"
        );
    }
}

#[test]
fn project_selection_is_ordered_gendered_and_prefers_repaired_sources() {
    {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        for (primary, unused) in [
            (
                "actors/dlc03/radchicken/dlc03_radchickenproject.hkx",
                "actors/dogmeat/dogmeat.hkx",
            ),
            (
                "actors/sheepsquatch/sheepsquatchproject.hkx",
                "actors/deathclaw/deathclawproject.hkx",
            ),
            (
                "actors/turret/turretworkshop.hkx",
                "actors/turret/turretstanding.hkx",
            ),
        ] {
            let mut original = race(&[primary, unused], true, &interner);
            put(
                &mut original,
                "SGNM",
                "Actors\\DLC03\\Critter\\Behaviors\\CritterCore.hkx",
                &interner,
            );
            let mut repaired = original.clone();
            repaired.fields.reverse();
            assert_eq!(
                select_creature_project(&original, &repaired, &schema, Path::new(""), &interner)
                    .unwrap()
                    .as_deref(),
                Some(primary)
            );
        }
    }
    {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let original = race(
            &["actors/a/male.hkx", "actors/a/female.hkx"],
            false,
            &interner,
        );
        assert!(
            select_creature_project(&original, &original, &schema, Path::new(""), &interner)
                .unwrap_err()
                .to_string()
                .contains("separate gender projects")
        );
    }
    {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let source = tempfile::tempdir().unwrap();
        let original = race(
            &[
                "actors/cat_pet/catpet.hkx",
                "actors/molerat/moleratproject.hkx",
            ],
            true,
            &interner,
        );
        let corrected = "actors/cat_pet/cat_petproject.hkx";
        std::fs::create_dir_all(source.path().join("actors/cat_pet")).unwrap();
        std::fs::write(source.path().join(corrected), []).unwrap();
        for (repaired_project, expected) in [
            (corrected, corrected),
            (
                "actors/molerat/moleratproject.hkx",
                "actors/molerat/moleratproject.hkx",
            ),
            (
                "actors/character/raiderproject.hkx",
                "actors/cat_pet/catpet.hkx",
            ),
            ("actors/cat_pet/missing.hkx", "actors/cat_pet/catpet.hkx"),
        ] {
            let repaired = race(&[repaired_project], true, &interner);
            assert_eq!(
                select_creature_project(&original, &repaired, &schema, source.path(), &interner)
                    .unwrap()
                    .as_deref(),
                Some(expected)
            );
        }
    }
}
