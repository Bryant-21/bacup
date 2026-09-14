use super::*;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

fn converted_workbench() -> HkxFile {
    let bytes = std::fs::read(repo().join("extracted/fo76/meshes").join(WORKBENCH)).unwrap();
    let converted = havok_native::api::havok_convert_bytes_report(&bytes, VERSION).unwrap();
    HkxFile::read(&converted.bytes).unwrap()
}

#[test]
#[ignore = "requires extracted FO76/FO4 workbench graphs"]
fn gas_pump_exit_roundtrip_preserves_other_nodes_and_is_scoped() {
    let target = repo().join("extracted/fo4/meshes");
    let mut graph = converted_workbench();
    let clip = named(&graph, "hkbClipGenerator", "ExitToStand").unwrap();
    set_string(
        &mut graph.objects_mut()[clip],
        "animationName",
        &format!(
            "..\\B21_FO76\\Source\\fo4rig{}",
            GAS_PUMP_EXIT.replace('/', "\\")
        ),
    );
    let original = graph.clone();
    assert!(
        !repair_gas_pump_exit(
            &mut graph,
            "actors/character/behaviors/gunbehavior.hkx",
            &target
        )
        .unwrap()
    );
    assert_eq!(graph.save(), original.save());
    assert!(repair_gas_pump_exit(&mut graph, WORKBENCH, &target).unwrap());
    let state = named(&graph, "hkbStateMachineStateInfo", "Exit_To_Stand").unwrap();
    let modifier = pointer(&graph.objects()[state], "generator").unwrap();
    let changed: Vec<_> = original
        .objects()
        .iter()
        .zip(graph.objects())
        .enumerate()
        .filter_map(|(i, (a, b))| (a != b).then_some(i))
        .collect();
    let mut expected = vec![clip, state, modifier];
    expected.sort();
    assert_eq!(changed, expected);
    assert_eq!(graph.objects().len(), original.objects().len() + 2);
    let bytes = graph.save();
    let mut packed = HkxFile::read(&bytes).unwrap();
    let clip = named(&packed, "hkbClipGenerator", "ExitToStand").unwrap();
    let state = named(&packed, "hkbStateMachineStateInfo", "Exit_To_Stand").unwrap();
    let modifier = pointer(&packed.objects()[state], "generator").unwrap();
    assert_eq!(
        pointer(&packed.objects()[modifier], "generator"),
        Some(clip)
    );
    assert_eq!(
        value(&packed.objects()[clip], "playbackSpeed").and_then(number),
        Some(1.0)
    );
    let names = event_names(&packed).unwrap();
    let triggers = pointer(&packed.objects()[clip], "triggers").unwrap();
    let actual: Vec<_> = array(&packed.objects()[triggers], "triggers")
        .iter()
        .map(|trigger| {
            let fields = trigger.as_object_members().unwrap();
            let event = fields
                .iter()
                .find(|m| m.name == "event")
                .unwrap()
                .value
                .as_object_members()
                .unwrap();
            let id =
                number(&event.iter().find(|m| m.name == "id").unwrap().value).unwrap() as usize;
            let time =
                number(&fields.iter().find(|m| m.name == "localTime").unwrap().value).unwrap();
            (names[id].clone(), time)
        })
        .collect();
    assert_eq!(
        actual,
        vec![
            ("furnitureExitSlave".into(), -0.182),
            ("idleChairGetUp".into(), -0.1333333),
            ("ReevaluateGraphState".into(), -0.133),
        ]
    );
    let entry = pointer(&packed.objects()[state], "enterNotifyEvents").unwrap();
    let entry_names: Vec<_> = array(&packed.objects()[entry], "events")
        .iter()
        .map(|event| {
            let id = event
                .as_object_members()
                .unwrap()
                .iter()
                .find(|m| m.name == "id")
                .unwrap();
            names[number(&id.value).unwrap() as usize].clone()
        })
        .collect();
    assert_eq!(entry_names, ["HeadTrackingOn"]);
    assert!(!repair_gas_pump_exit(&mut packed, WORKBENCH, &target).unwrap());
    assert_eq!(packed.save(), bytes);
    let mut tinkers = original.clone();
    set_string(
        &mut tinkers.objects_mut()[clip],
        "animationName",
        "Animations/Furniture/WorkbenchTinkers/ExitToStand.hkx",
    );
    let before = tinkers.save();
    assert!(!repair_gas_pump_exit(&mut tinkers, WORKBENCH, &target).unwrap());
    assert_eq!(tinkers.save(), before);
}

#[test]
#[ignore = "requires the recorded Gas Pump baseline and tested loose probe"]
fn gas_pump_recorded_probe_equivalence() {
    let directory = repo().join("tmp/furniture_620b21_exit");
    let baseline = directory.join("deployed/meshes/actors/character/behaviors/b21_fo76_workbenchfurniturebehavior_8ec6da9f7003.hkx");
    let mut graph = read(&baseline).unwrap();
    assert!(
        repair_gas_pump_exit(&mut graph, WORKBENCH, &repo().join("extracted/fo4/meshes")).unwrap()
    );
    let output = directory.join("native-pipeline-exit.hkx");
    std::fs::write(&output, graph.save()).unwrap();
    let packed = read(&output).unwrap();
    let probe = read(&repo().join("mods/B21_FO76GasPumpExitProbe/data/Meshes/actors/character/behaviors/B21_FO76_workbenchfurniturebehavior_8ec6da9f7003.hkx")).unwrap();
    // The tested probe's XML roundtrip rounded FO4's trigger timestamps to six decimals.
    assert_eq!(
        havok_native::api::havok_hkx_to_xml(&packed.save()).unwrap(),
        havok_native::api::havok_hkx_to_xml(&probe.save()).unwrap(),
    );
    println!(
        "Native Gas Pump graph matches the tested loose probe: {}",
        output.display()
    );
}

#[test]
#[ignore = "requires extracted FO76/FO4 assets"]
fn gas_pump_and_tinkers_asset_pipeline() {
    use crate::fixups::face::build_additive_race_record::SubgraphBlock;
    use crate::sym::StringInterner;

    let source = repo().join("extracted/fo76/meshes");
    let target = repo().join("extracted/fo4/meshes");
    let project = Project {
        source: "actors/character".into(),
        destination: "actors/character".into(),
        vanilla: true,
    };
    let interner = StringInterner::new();
    let mut plan = Plan::default();
    let mut cache = BTreeMap::new();
    for folder in [
        "atx/actors/character/animation/furniture/atx_gaspump_distillingstation",
        "actors/character/animations/furniture/workbenchtinkers",
    ] {
        let block = SubgraphBlock {
            behaviour_graph: interner.intern(WORKBENCH),
            paths: vec![interner.intern(folder)],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };
        plan.routes.push(
            super::super::records::make_route(
                &source, WORKBENCH, &block, &project, &interner, &mut cache,
            )
            .unwrap(),
        );
    }
    plan.projects.insert(project.destination.clone(), project);
    let output = tempfile::Builder::new()
        .prefix("gas_pump_pipeline_")
        .tempdir_in(repo().join("tmp"))
        .unwrap();
    std::fs::create_dir_all(output.path().join("debug/fo76_behaviors")).unwrap();
    std::fs::write(
        output.path().join(PLAN_PATH),
        serde_json::to_vec_pretty(&plan).unwrap(),
    )
    .unwrap();
    assert!(super::super::assets::build(output.path(), &source, &target).unwrap() > 10);
    for (route, expected_speed) in plan.routes.iter().zip([1.0, -0.1]) {
        let path = output.path().join("data/Meshes").join(&route.destination);
        let graph = read(&path).unwrap();
        let clip = named(&graph, "hkbClipGenerator", "ExitToStand").unwrap();
        assert_eq!(
            value(&graph.objects()[clip], "playbackSpeed").and_then(number),
            Some(expected_speed)
        );
        assert_eq!(
            pointer(&graph.objects()[clip], "triggers").is_some(),
            expected_speed == 1.0
        );
    }
    assert!(
        !output
            .path()
            .join("data/Meshes/actors/character/behaviors/raiderrootbehavior.hkx")
            .exists()
    );
    println!(
        "Gas Pump and Tinkers pipeline output: {}",
        output.keep().display()
    );
}
