use super::*;
use crate::ids::{FormKey, SigCode};
use crate::record::RecordFlags;

#[test]
fn player_fear_cache_matches_the_gameplay_confirmed_bucket() {
    let temp = tempfile::tempdir().unwrap();
    let core = r"actors\B21_FO76\wendigocolossus\behaviors\B21_FO76_wendigocolossuscorebehavior_be32004fa6ca.hkx";
    let graph = temp.path().join(core.replace('\\', "/"));
    std::fs::create_dir_all(graph.parent().unwrap()).unwrap();
    let original = include_bytes!("colossus_core.hkx");
    std::fs::write(&graph, original).unwrap();
    let entries = clip_generator_entries(&graph);
    assert!(!entries.is_empty());
    let bucket = temp.path().join(format!(
        "AnimTextData/ClipGeneratorData/{}.txt",
        name_id(core)
    ));
    std::fs::create_dir_all(bucket.parent().unwrap()).unwrap();
    std::fs::write(&bucket, clip_generator_data_body(core, &entries)).unwrap();
    let subgraphs = [SubgraphInput {
        core_behavior: core.into(),
        sapt_chain: vec![],
        race_dir: None,
    }];
    for _ in 0..2 {
        write_selection_timing(&subgraphs, temp.path(), temp.path()).unwrap();
        let actual = std::fs::read(&bucket).unwrap();
        let expected = include_bytes!("colossus_tested_cache.bin");
        assert_eq!(actual.len(), expected.len());
        let differences: Vec<_> = actual
            .iter()
            .zip(expected)
            .enumerate()
            .filter_map(|(index, (actual, expected))| {
                (actual != expected).then_some((index, actual, expected))
            })
            .collect();
        assert!(
            differences.is_empty(),
            "cache differences: {:?}",
            &differences[..differences.len().min(20)]
        );
        assert_eq!(std::fs::read(&graph).unwrap(), original);
    }
}

fn fear_entry(name: &str, time: f32, flag: u8) -> ClipGenEntry {
    ClipGenEntry {
        clip_name: name.into(),
        anim_name: "AttackFear".into(),
        playback_speed: 1.0,
        crop_start: 0.0,
        crop_end: 0.0,
        x0: 0,
        dynamic: false,
        triggers: vec![ClipTrigger {
            name: "SpawnExplosionOnGround".into(),
            time,
            flag,
        }],
    }
}

#[test]
fn player_fear_selection_preserves_authored_timing_and_existing_hits() {
    let mut entries = [
        fear_entry("AttackFear", 0.5, 1),
        fear_entry("AttackSummon", 2.5, 0),
    ];
    assert!(add_selection_time(&mut entries));
    assert_eq!(entries[0].triggers[1].name, "HitFrame");
    assert_eq!(entries[0].triggers[1].time, 0.5);
    assert_eq!(entries[0].triggers[1].flag, 1);
    assert_eq!(entries[1].triggers.len(), 1);
    entries[0].triggers[1].time = 1.25;
    assert!(!add_selection_time(&mut entries));
    assert_eq!(entries[0].triggers[1].time, 1.25);
}

fn explosion(interner: &StringInterner, eid: &str) -> Record {
    Record {
        sig: SigCode::from_str("EXPL").unwrap(),
        form_key: FormKey {
            local: 0x900,
            plugin: interner.intern("B21_Remapped.esp"),
        },
        eid: Some(interner.intern(eid)),
        flags: RecordFlags::empty(),
        warnings: Default::default(),
        fields: [
            FieldEntry {
                sig: SubrecordSig::from_str("EITM").unwrap(),
                value: FieldValue::FormKey(FormKey {
                    local: 0x901,
                    plugin: interner.intern("B21_Remapped.esp"),
                }),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("DATA").unwrap(),
                value: FieldValue::Bytes(vec![0; 76].into()),
            },
        ]
        .into_iter()
        .collect(),
    }
}

#[test]
fn player_fear_anchor_preserves_explosion_data_and_survives_asset_cleanup() {
    let interner = StringInterner::new();
    let temp = tempfile::tempdir().unwrap();
    let mut record = explosion(&interner, "crExplosionWendigoColossusFear");
    let original = record.fields.clone();
    assert!(repair_explosion(&mut record, &interner, Some(temp.path())).unwrap());
    assert_eq!(&record.fields[1..], original.as_slice());
    assert_eq!(record.fields[0].sig.as_str(), "MODL");
    let path = temp
        .path()
        .join("data/Meshes")
        .join(ANCHOR_MODEL.replace('\\', "/"));
    assert_eq!(std::fs::read(&path).unwrap(), ANCHOR_NIF);
    nif_core_native::model::NifFile::load(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(!repair_explosion(&mut record, &interner, Some(temp.path())).unwrap());
    assert_eq!(std::fs::read(&path).unwrap(), ANCHOR_NIF);
}

#[test]
fn player_fear_anchor_leaves_other_explosions_and_custom_models_alone() {
    let interner = StringInterner::new();
    let temp = tempfile::tempdir().unwrap();
    let mut record = explosion(&interner, "B21_OtherExplosion");
    let original = record.fields.clone();
    assert!(!repair_explosion(&mut record, &interner, Some(temp.path())).unwrap());
    assert_eq!(record.fields, original);
    record.eid = Some(interner.intern(EXPLOSION_EID));
    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MODL").unwrap(),
        value: FieldValue::String(interner.intern("B21/CustomExplosion.nif")),
    });
    let original = record.fields.clone();
    assert!(!repair_explosion(&mut record, &interner, Some(temp.path())).unwrap());
    assert_eq!(record.fields, original);
    assert!(!temp.path().join("data").exists());
}
