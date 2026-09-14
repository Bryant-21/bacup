use super::*;
use crate::fixups::face::build_additive_race_record::parse_canonical_subgraphs;
use crate::fixups::havok::anim_text_data_emit::generate_anim_text_data_for_handle_with_base_race_handles;
use crate::fixups::{Fixup, FixupConfig};
use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
use crate::ids::{FormKey, SigCode};
use crate::session::open_session;
use crate::sym::StringInterner;
use ck_native::anim_text_data::emit::AnimTextDataInputs;
use ck_native::anim_text_data::race_decode::subgraph_inputs_from_plugin;
use esp_authoring_core::plugin_runtime::{
    ParsedRecord, ParsedSubrecord, insert_parsed_record_in_slot, plugin_handle_close_native,
    plugin_handle_load_no_py, plugin_handle_new_native, plugin_handle_save_no_py,
    plugin_handle_store_ref,
};

struct Handle(u64);
impl Drop for Handle {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            plugin_handle_close_native(self.0);
        }
    }
}

fn raw(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
    ParsedSubrecord {
        signature: signature.into(),
        data: data.into(),
        semantic_type: None,
    }
}

fn string_field(signature: &str, value: &str) -> ParsedSubrecord {
    raw(signature, value.bytes().chain([0]).collect())
}

fn record(signature: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
    ParsedRecord {
        signature: signature.into(),
        form_id,
        flags: 0,
        version_control: 0,
        form_version: Some(131),
        version2: None,
        subrecords,
        raw_payload: None,
        parse_error: None,
    }
}

#[test]
#[ignore = "requires extracted FO76/FO4 assets and B21_FURNITURE_TEST_FO4_MASTER"]
fn tinkers_routes_actions_assets_and_native_metadata() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap();
    let source_root = repo.join("extracted/fo76");
    let target_root = repo.join("extracted/fo4");
    let target_master = std::env::var("B21_FURNITURE_TEST_FO4_MASTER").unwrap();
    let base =
        Handle(plugin_handle_load_no_py(&target_master, Some("fo4"), None, None, true).unwrap());
    let source = Handle(plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap());
    let target =
        Handle(plugin_handle_new_native("B21_FurniturePipeline.esp", Some("fo4")).unwrap());
    let graph = r"Actors\Character\Behaviors\WorkbenchFurnitureBehavior.hkx";
    let animations = r"Actors\Character\Animations\Furniture\WorkbenchTinkers";
    {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&target.0).unwrap();
        slot.parsed.header.masters = vec!["Fallout4.esm".into()];
        for rec in [
            record(
                "KYWD",
                0x01000810,
                vec![string_field("EDID", "B21_AnimTinkers")],
            ),
            record(
                "FURN",
                0x01000800,
                vec![
                    string_field("EDID", "B21_Tinkers"),
                    string_field(
                        "MODL",
                        r"Furniture\Workstations\WorkbenchTinkers\WorkstationTinkers01.nif",
                    ),
                    raw("KSIZ", 1u32.to_le_bytes().to_vec()),
                    raw("KWDA", 0x01000810u32.to_le_bytes().to_vec()),
                    raw("WBDT", vec![5]),
                ],
            ),
            record(
                "RACE",
                0x01000820,
                vec![
                    string_field("EDID", "B21_HumanFurnitureAdditive"),
                    raw("SADD", 0x00166729u32.to_le_bytes().to_vec()),
                    raw("STKD", 0x01000810u32.to_le_bytes().to_vec()),
                    string_field("SGNM", graph),
                    string_field("SAPT", animations),
                    raw("SRAF", vec![2, 0, 0, 0]),
                ],
            ),
        ] {
            insert_parsed_record_in_slot(slot, rec);
        }
    }
    let interner = StringInterner::new();
    let mut mapper = FormKeyMapper::new(
        [],
        MapperOptions {
            output_plugin_name: "B21_FurniturePipeline.esp".into(),
            preserve_source_ids: true,
            ..Default::default()
        },
        &interner,
    );
    for (local, signature) in [(0x800, "FURN"), (0x810, "KYWD"), (0x820, "RACE")] {
        let key = FormKey {
            plugin: interner.intern("B21_FurniturePipeline.esp"),
            local,
        };
        assert_eq!(
            mapper.allocate_or_resolve(key, None, SigCode::from_str(signature).unwrap()),
            key
        );
    }
    let temp = tempfile::Builder::new()
        .prefix("fo76_furniture_pipeline_")
        .tempdir_in(repo.join("tmp"))
        .unwrap();
    let config = FixupConfig {
        mod_path: Some(temp.path().to_path_buf()),
        source_extracted_dir: Some(source_root.clone()),
        target_master_handle_ids: vec![base.0],
        ..Default::default()
    };
    let race_key = FormKey {
        plugin: interner.intern("B21_FurniturePipeline.esp"),
        local: 0x820,
    };
    let mut session = open_session(target.0, Some(source.0)).unwrap();
    let schema = session.schema().unwrap();
    let report = records::PreserveFo76BehaviorsFixup
        .run_with_session(&mut session, &mut mapper, &config)
        .unwrap();
    assert_eq!(report.records_changed, 1);
    assert_eq!(report.records_added, 47);
    let race = session
        .record_decoded(&race_key, &schema, &interner)
        .unwrap();
    let blocks = parse_canonical_subgraphs(&race);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].target_keywords[0].local, 0x810);
    assert!(blocks[0].subgraph_keywords.is_empty());
    assert_eq!(blocks[0].flags_bytes.as_deref().unwrap(), &[2, 0, 0, 0]);
    assert!(
        session
            .form_keys_of_sig(SigCode::from_str("COBJ").unwrap(), &interner)
            .unwrap()
            .is_empty()
    );
    let before = std::fs::read(temp.path().join(PLAN_PATH)).unwrap();
    let second = records::PreserveFo76BehaviorsFixup
        .run_with_session(&mut session, &mut mapper, &config)
        .unwrap();
    assert_eq!((second.records_changed, second.records_added), (0, 0));
    assert_eq!(std::fs::read(temp.path().join(PLAN_PATH)).unwrap(), before);
    drop(session);
    let plugin = temp.path().join("B21_FurniturePipeline.esp");
    plugin_handle_save_no_py(target.0, plugin.to_str().unwrap()).unwrap();
    let inputs =
        AnimTextDataInputs::from(subgraph_inputs_from_plugin(&plugin, "fo4", &[]).unwrap());
    for event in ["sitStartFromStand", "standStart"] {
        assert!(
            inputs.event_candidates.iter().any(|e| e == event),
            "missing {event}"
        );
    }
    assert_eq!(inputs.subgraphs.len(), 1);
    let plan: Plan = serde_json::from_slice(&before).unwrap();
    assert_eq!(plan.routes.len(), 1);
    let route = &plan.routes[0];
    assert_eq!(route.source, clean(graph));
    assert!(route.draw_event.is_none());
    for clip in [
        "enterfromstand.hkx",
        "exittostand.hkx",
        "posea_idle1.hkx",
        "posea_idleflavor1.hkx",
        "posea_idleflavor2.hkx",
        "posea_idleflavor3.hkx",
    ] {
        assert!(
            route.animations.values().any(|p| p.ends_with(clip)),
            "missing {clip}"
        );
    }
    assert!(assets::build(temp.path(), &source_root, &target_root).unwrap() > 10);
    let output = temp.path().join("data/Meshes");
    for global in [
        "actors/character/behaviors/raiderrootbehavior.hkx",
        "actors/character/characters/raidercharacter.hkx",
    ] {
        assert!(
            !output.join(global).exists(),
            "furniture must not supply global override {global}"
        );
    }
    let private_graph = read(&output.join(&route.destination)).unwrap();
    let project = &plan.projects[&route.project];
    for origin in route.animations.values() {
        let relocated = format!("actors/B21_FO76/source/fo4rig/{origin}");
        assert!(
            output.join(&relocated).is_file(),
            "missing source animation {origin}"
        );
    }
    let idle = route
        .animations
        .values()
        .find(|p| p.ends_with("/posea_idle1.hkx"))
        .unwrap();
    let idle_path = relative_path(
        &project.destination,
        &format!("actors/B21_FO76/source/fo4rig/{idle}"),
    );
    assert!(
        private_graph
            .objects()
            .iter()
            .any(|o| o.class_name == "hkbClipGenerator"
                && clean(&string(o, "animationName")) == clean(&idle_path))
    );
    let count = generate_anim_text_data_for_handle_with_base_race_handles(
        target.0,
        &[base.0],
        &output,
        &output,
        Some(&target_root.join("meshes")),
        Some("B21"),
    )
    .unwrap();
    assert!(count > 0);
    let retained = temp.keep();
    println!("Furniture pipeline output: {}", retained.display());
    let subgraph = &inputs.subgraphs[0];
    let metadata = output.join("AnimTextData");
    let files =
        std::fs::read_to_string(metadata.join(format!("AnimationFileData/{}.txt", subgraph.id())))
            .unwrap();
    let lines: Vec<_> = files.lines().collect();
    assert_eq!(clean(lines[4]), clean(&route.destination));
    for path in &lines[4..4 + lines[3].parse::<usize>().unwrap()] {
        assert!(
            output.join(path.replace('\\', "/")).is_file(),
            "missing manifest member {path}"
        );
    }
    let offsets =
        std::fs::read(metadata.join(format!("AnimationOffsets/{}.txt", subgraph.id()))).unwrap();
    for clip in ["standing enter", "exittostand"] {
        assert!(
            offsets
                .windows(clip.len())
                .any(|bytes| bytes.eq_ignore_ascii_case(clip.as_bytes())),
            "missing offset {clip}"
        );
    }
    let events = std::fs::read_dir(metadata.join("AnimEventInfo"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<String>();
    for event in ["sitStartFromStand", "standStart"] {
        assert!(events.contains(event), "missing generated event {event}");
    }
    println!(
        "Furniture pipeline proof: {} files, {} actions, output {}",
        count,
        report.records_added,
        retained.display()
    );
}
