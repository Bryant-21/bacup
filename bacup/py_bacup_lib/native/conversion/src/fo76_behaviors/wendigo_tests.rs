use super::*;
use crate::fixups::havok::normalize_weapon_behavior_contracts::merge_behavior_contracts;

fn object(class: &str, name: &str) -> HkxObject {
    let mut object = HkxObject {
        name: None,
        offset: 0,
        signature: 0,
        class_name: class.into(),
        members: vec![],
    };
    set_string(&mut object, "name", name);
    object
}

fn refs(indices: &[usize]) -> HkxValue {
    HkxValue::Array(
        indices
            .iter()
            .map(|i| HkxValue::Pointer(Some(*i)))
            .collect(),
    )
}

fn graph() -> HkxFile {
    let mut file = HkxFile::from_tagxml(11, VERSION, vec![]);
    let root = add(&mut file, object("hkbBehaviorGraph", "Wendigo"));
    let data = add(&mut file, object("hkbBehaviorGraphData", ""));
    let strings = add(&mut file, object("hkbBehaviorGraphStringData", ""));
    let values = add(&mut file, object("hkbVariableValueSet", ""));
    set(
        &mut file.objects_mut()[root],
        "data",
        HkxValue::Pointer(Some(data)),
    );
    set(
        &mut file.objects_mut()[data],
        "stringData",
        HkxValue::Pointer(Some(strings)),
    );
    set(
        &mut file.objects_mut()[data],
        "variableInitialValues",
        HkxValue::Pointer(Some(values)),
    );
    set(
        &mut file.objects_mut()[strings],
        "variableNames",
        HkxValue::Array(vec![HkxValue::String {
            value: "Speed".into(),
            is_null: false,
        }]),
    );
    set(
        &mut file.objects_mut()[data],
        "variableInfos",
        HkxValue::Array(vec![HkxValue::Object(vec![HkxMember {
            name: "type".into(),
            value: HkxValue::I8(4),
        }])]),
    );
    set(
        &mut file.objects_mut()[values],
        "wordVariableValues",
        HkxValue::Array(vec![HkxValue::Object(vec![HkxMember {
            name: "value".into(),
            value: HkxValue::I32(0),
        }])]),
    );
    for field in ["eventNames", "characterPropertyNames"] {
        set(
            &mut file.objects_mut()[strings],
            field,
            HkxValue::Array(vec![]),
        );
    }
    for field in ["eventInfos", "characterPropertyInfos"] {
        set(
            &mut file.objects_mut()[data],
            field,
            HkxValue::Array(vec![]),
        );
    }
    for field in ["quadVariableValues", "variantVariableValues"] {
        set(
            &mut file.objects_mut()[values],
            field,
            HkxValue::Array(vec![]),
        );
    }
    add(&mut file, object("hkbModifierGenerator", "WrapperTemplate"));
    file
}

fn clip(file: &mut HkxFile, source: &Path, name: &str, speed: f32) -> usize {
    let mut frame = object("hkaDefaultAnimatedReferenceFrame", "");
    set(
        &mut frame,
        "referenceFrameSamples",
        HkxValue::Array(vec![
            HkxValue::F32List(vec![0.0, 0.0, 0.0, 0.0]),
            HkxValue::F32List(vec![0.0, speed * 2.0, 0.0, 0.0]),
        ]),
    );
    set(&mut frame, "duration", HkxValue::F32(2.0));
    std::fs::write(
        source.join(name),
        HkxFile::from_tagxml(11, VERSION, vec![frame]).save(),
    )
    .unwrap();
    let mut clip = object("hkbClipGenerator", name);
    set_string(&mut clip, "animationName", name);
    add(file, clip)
}

fn machine(file: &mut HkxFile, name: &str, generators: &[usize]) -> usize {
    let states: Vec<_> = generators
        .iter()
        .map(|generator| {
            let mut state = object("hkbStateMachineStateInfo", "");
            set(&mut state, "generator", HkxValue::Pointer(Some(*generator)));
            add(file, state)
        })
        .collect();
    let mut machine = object("hkbStateMachine", name);
    set(&mut machine, "states", refs(&states));
    add(file, machine)
}

fn vanilla() -> HkxFile {
    let mut file = HkxFile::from_tagxml(11, VERSION, vec![]);
    let child = add(&mut file, object("hkbBlenderGeneratorChild", ""));
    let mut blend = object("hkbBlenderGenerator", "ReadyTurnRightSlowBlend");
    set(&mut blend, "children", refs(&[child]));
    set(&mut blend, "flags", HkxValue::I16(17));
    add(&mut file, blend);
    add(
        &mut file,
        object("BSInterpValueModifier", "InterpRagdollToAnim"),
    );
    file
}

fn names(file: &HkxFile) -> Vec<HkxValue> {
    array(
        file.objects()
            .iter()
            .find(|o| o.class_name == "hkbBehaviorGraphStringData")
            .unwrap(),
        "variableNames",
    )
}

#[test]
fn wendigo_manual_gaits_use_forward_motion_and_promote_the_speed_variable() {
    let source = tempfile::tempdir().unwrap();
    let mut file = graph();
    let mut host = graph();
    let walk = clip(&mut file, source.path(), "walk.hkx", 110.0);
    let backward = clip(&mut file, source.path(), "backward.hkx", -100.0);
    let jog = clip(&mut file, source.path(), "jog.hkx", 306.0);
    let run1 = clip(&mut file, source.path(), "run1.hkx", 510.0);
    let run2 = clip(&mut file, source.path(), "run2.hkx", 520.0);
    let walk = machine(&mut file, "WalkDirection", &[walk, backward]);
    let run = machine(&mut file, "RunRandom", &[run1, run2, run1]);
    let normal = machine(&mut file, "Locomotion_SM", &[walk, run]);
    let combat = machine(&mut file, "CombatLocomotion_SM", &[walk, jog, run]);
    set(
        &mut file.objects_mut()[0],
        "rootGenerator",
        HkxValue::Pointer(Some(combat)),
    );
    let animations = [
        "walk.hkx",
        "backward.hkx",
        "jog.hkx",
        "run1.hkx",
        "run2.hkx",
    ]
    .into_iter()
    .map(|name| (name.into(), name.into()))
    .collect();
    let clips_before: Vec<_> = file
        .objects()
        .iter()
        .filter(|o| o.class_name == "hkbClipGenerator")
        .cloned()
        .collect();
    repair(
        &mut file,
        source.path(),
        &animations,
        "actors/wendigo/behaviors/wendigocorebehavior.hkx",
        &vanilla(),
    )
    .unwrap();
    for (index, expected) in [
        (normal, vec![110.0, 515.0]),
        (combat, vec![110.0, 306.0, 515.0]),
    ] {
        let blend = &file.objects()[index];
        assert_eq!(blend.class_name, "hkbBlenderGenerator");
        let speeds: Vec<_> = pointers(blend, "children")
            .iter()
            .map(|i| number(value(&file.objects()[*i], "weight").unwrap()).unwrap())
            .collect();
        assert_eq!(speeds, expected);
    }
    let clips_after: Vec<_> = file
        .objects()
        .iter()
        .filter(|o| o.class_name == "hkbClipGenerator")
        .cloned()
        .collect();
    assert_eq!(format!("{clips_before:?}"), format!("{clips_after:?}"));
    assert!(
        names(&file)
            .iter()
            .any(|v| text(v) == Some("B21_LocomotionSpeed"))
    );
    assert!(
        !names(&host)
            .iter()
            .any(|v| text(v) == Some("B21_LocomotionSpeed"))
    );
    merge_behavior_contracts(&mut host, &file).unwrap();
    merge_behavior_contracts(&mut host, &file).unwrap();
    assert_eq!(
        names(&host)
            .iter()
            .filter(|v| text(v) == Some("B21_LocomotionSpeed"))
            .count(),
        1
    );
    let once = format!("{:?}", file.objects());
    repair(
        &mut file,
        source.path(),
        &animations,
        "actors/wendigo/behaviors/wendigocorebehavior.hkx",
        &vanilla(),
    )
    .unwrap();
    assert_eq!(once, format!("{:?}", file.objects()));
}

#[test]
fn other_creatures_and_in_place_wendigo_gaits_are_unchanged() {
    let source = tempfile::tempdir().unwrap();
    let animations = ["walk.hkx", "run.hkx"]
        .into_iter()
        .map(|n| (n.into(), n.into()))
        .collect();
    for (origin, walk_speed, run_speed) in [
        (
            "actors/snallygaster/behaviors/snallygastercorebehavior.hkx",
            110.0,
            515.0,
        ),
        ("actors/wendigo/behaviors/wendigocorebehavior.hkx", 0.0, 0.0),
    ] {
        let mut file = graph();
        let walk = clip(&mut file, source.path(), "walk.hkx", walk_speed);
        let run = clip(&mut file, source.path(), "run.hkx", run_speed);
        machine(&mut file, "Locomotion_SM", &[walk, run]);
        let before = format!("{:?}", file.objects());
        repair(&mut file, source.path(), &animations, origin, &vanilla()).unwrap();
        assert_eq!(before, format!("{:?}", file.objects()));
    }
}

#[test]
#[ignore = "requires B21_BEHAVIOR_PLAN and extracted FO76/FO4 meshes"]
fn live_wendigo_asset_build_matches_verified_gaits_and_host_contract() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap();
    let mut plan: Plan = serde_json::from_slice(
        &std::fs::read(std::env::var("B21_BEHAVIOR_PLAN").unwrap()).unwrap(),
    )
    .unwrap();
    plan.routes
        .retain(|r| r.source == "actors/wendigo/behaviors/wendigocorebehavior.hkx");
    assert!(!plan.routes.is_empty());
    plan.projects
        .retain(|key, _| plan.routes.iter().any(|r| &r.project == key));
    let output = tempfile::Builder::new()
        .prefix("wendigo_pipeline_")
        .tempdir_in(repo.join("tmp"))
        .unwrap();
    std::fs::create_dir_all(output.path().join("debug/fo76_behaviors")).unwrap();
    std::fs::write(
        output.path().join(PLAN_PATH),
        serde_json::to_vec(&plan).unwrap(),
    )
    .unwrap();
    let count = assets::build(
        output.path(),
        &repo.join("extracted/fo76"),
        &repo.join("extracted/fo4"),
    )
    .unwrap();
    let route = plan
        .routes
        .iter()
        .find(|r| {
            r.destination
                .ends_with("B21_FO76_wendigocorebehavior_578ee489552e.hkx")
        })
        .unwrap();
    let core = read(&output.path().join("data/Meshes").join(&route.destination)).unwrap();
    for (name, expected) in [
        (
            "B21_Wendigo_Locomotion_SM_SpeedBlend",
            vec![134.917, 515.107],
        ),
        (
            "B21_Wendigo_CombatLocomotion_SM_SpeedBlend",
            vec![110.280, 306.072, 515.107],
        ),
    ] {
        let blend = template(&core, name).unwrap();
        assert_eq!(blend.class_name, "hkbBlenderGenerator");
        let children = pointers(&blend, "children");
        assert_eq!(children.len(), expected.len());
        for (child, expected) in children.iter().zip(expected) {
            let measured = number(value(&core.objects()[*child], "weight").unwrap()).unwrap();
            assert!(
                (measured - expected).abs() < 0.02,
                "{name}: {measured} != {expected}"
            );
        }
    }
    let root = read(
        &output
            .path()
            .join("data/Meshes/actors/B21_FO76/wendigo/behaviors/wendigorootbehavior.hkx"),
    )
    .unwrap();
    assert_eq!(
        names(&root)
            .iter()
            .filter(|v| text(v) == Some("B21_LocomotionSpeed"))
            .count(),
        1
    );
    println!(
        "Verified {count} generated files across {} Wendigo routes, including the host speed declaration",
        plan.routes.len()
    );
}
