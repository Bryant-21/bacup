use super::*;
use std::collections::BTreeSet;

fn reference_frame(samples: Vec<f32>, duration: f32) -> HkxFile {
    let mut frame = HkxObject {
        name: None,
        offset: 0,
        signature: 0,
        class_name: "hkaDefaultAnimatedReferenceFrame".into(),
        members: vec![],
    };
    set(
        &mut frame,
        "referenceFrameSamples",
        HkxValue::F32List(samples),
    );
    set(&mut frame, "duration", HkxValue::F32(duration));
    HkxFile::from_tagxml(11, VERSION, vec![frame])
}

#[test]
fn in_place_directions_do_not_require_extracted_motion() {
    let empty = HkxFile::from_tagxml(11, VERSION, vec![]);
    for file in [
        empty,
        reference_frame(vec![0.0; 4], 1.0),
        reference_frame(vec![0.0; 8], 1.0),
        reference_frame(vec![0.0; 8], 0.0),
    ] {
        for (name, direction) in [
            (r"Animations\WPNJogForwardRelaxed.hkt", 0.0),
            ("WPNWalkRight.hkx", 0.25),
            ("WPNRunBackward.hkx", 0.5),
            ("WPNWalkBackpedal.hkx", 0.5),
            ("WPNRunLeft.hkx", 0.75),
            ("WPNRunForwardLeft.hkx", 0.875),
        ] {
            assert_eq!(motion(&file, name), Some((direction, 0.0)), "{name}");
        }
        assert_eq!(motion(&file, "Idle.hkx"), None);
    }
}

#[test]
fn measured_motion_takes_precedence_over_clip_name() {
    let mut file = reference_frame(vec![0.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0], 2.0);
    assert_eq!(motion(&file, "WPNRunForward.hkx"), Some((0.25, 5.0)));
    set(
        &mut file.objects_mut()[0],
        "referenceFrameSamples",
        HkxValue::Array(vec![
            HkxValue::F32List(vec![0.0; 4]),
            HkxValue::F32List(vec![0.0, 10.0, 0.0, 0.0]),
        ]),
    );
    assert_eq!(motion(&file, "WPNRunLeft.hkx"), Some((0.0, 5.0)));
}

#[test]
#[ignore = "requires B21_LOCOMOTION_PROOF_INPUT and extracted FO4 assets"]
fn live_locomotion_smoothing_proof() {
    use crate::fixups::havok::normalize_weapon_behavior_contracts::merge_behavior_contracts;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap();
    let input = std::env::var("B21_LOCOMOTION_PROOF_INPUT").unwrap();
    let mut graph = read(Path::new(&input)).unwrap();
    let vanilla =
        read(&repo.join("extracted/fo4/meshes/actors/character/behaviors/weaponbehavior.hkx"))
            .unwrap();
    let blends: Vec<_> = graph
        .objects()
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o.class_name == "hkbBlenderGenerator"
                && matches!(
                    string(o, "name").as_str(),
                    "Ready_BSLocomotionBlendGenerator"
                        | "Relaxed_BSLocomotionBlendGenerator"
                        | "Sneak_BSLocomotionBlendGenerator"
                )
        })
        .map(|(i, _)| i)
        .collect();
    assert_eq!(blends.len(), 3);
    let old_clips: Vec<_> = graph
        .objects()
        .iter()
        .filter(|o| o.class_name == "hkbClipGenerator")
        .cloned()
        .collect();
    smooth_speed(&mut graph, &blends, &vanilla).unwrap();
    let first = graph.save();
    smooth_speed(&mut graph, &blends, &vanilla).unwrap();
    assert_eq!(first, graph.save());
    let reloaded = HkxFile::read(&first).unwrap();
    let masks: Vec<_> = reloaded
        .objects()
        .iter()
        .filter(|o| string(o, "name").starts_with("FO76_BoneMaskSelector_"))
        .collect();
    assert_eq!(masks.len(), 22);
    for mask in masks {
        assert_eq!(mask.class_name, "hkbBlenderGenerator");
        assert_eq!(value(mask, "flags").and_then(number), Some(24.0));
        let children = pointers(mask, "children");
        assert_eq!(children.len(), 2);
        assert_eq!(
            value(&reloaded.objects()[children[0]], "weight").and_then(number),
            Some(0.0)
        );
        let full_mask_speed = value(&reloaded.objects()[children[1]], "weight")
            .and_then(number)
            .unwrap();
        assert!((full_mask_speed - 72.34179).abs() < 0.001);
    }
    let strings = reloaded
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
        .unwrap();
    let names = array(strings, "variableNames");
    let variable = names
        .iter()
        .position(|v| text(v) == Some("B21_LocomotionSpeed"))
        .unwrap();
    for blend in blends {
        let binding = pointer(&reloaded.objects()[blend], "variableBindingSet").unwrap();
        let entries = array(&reloaded.objects()[binding], "bindings");
        let HkxValue::Object(fields) = &entries[0] else {
            panic!("invalid binding")
        };
        assert!(
            fields
                .iter()
                .any(|m| m.name == "variableIndex" && number(&m.value) == Some(variable as f32))
        );
    }
    let new_clips: Vec<_> = graph
        .objects()
        .iter()
        .filter(|o| o.class_name == "hkbClipGenerator")
        .cloned()
        .collect();
    assert_eq!(format!("{old_clips:?}"), format!("{new_clips:?}"));
    let mut parent = read(
        &repo.join("mods/SeventySix/data/Meshes/actors/character/behaviors/RaiderRootBehavior.hkx"),
    )
    .unwrap();
    merge_behavior_contracts(&mut parent, &graph).unwrap();
    let parent = HkxFile::read(&parent.save()).unwrap();
    let strings = parent
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
        .unwrap();
    assert_eq!(
        array(strings, "variableNames")
            .iter()
            .filter(|v| text(v) == Some("B21_LocomotionSpeed"))
            .count(),
        1
    );
    let output = repo.join("tmp/nitro-aim/native-smoothed");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("B21_FO76_gunbehavior_ae25f94fc849.hkx"), first).unwrap();
    std::fs::write(output.join("RaiderRootBehavior.hkx"), parent.save()).unwrap();
    println!("proof output: {}", output.display());
}

#[test]
#[ignore = "requires B21_BEHAVIOR_PLAN and extracted FO76/FO4 meshes"]
fn live_plan_locomotion_audit() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap();
    let source = repo.join("extracted/fo76/meshes");
    let plan: Plan = serde_json::from_slice(
        &std::fs::read(std::env::var("B21_BEHAVIOR_PLAN").unwrap()).unwrap(),
    )
    .unwrap();
    let vanilla =
        read(&repo.join("extracted/fo4/meshes/actors/character/behaviors/weaponbehavior.hkx"))
            .unwrap();
    let mut converted = BTreeMap::new();
    let mut clips = BTreeMap::new();
    let mut failures = BTreeSet::new();
    let mut repaired = 0;
    for (index, route) in plan.routes.iter().enumerate() {
        if index % 100 == 0 {
            println!("auditing route {index}/{}", plan.routes.len());
        }
        for origin in route.dependencies.keys() {
            let graph = converted.entry(origin.clone()).or_insert_with(|| {
                let report = havok_native::api::havok_convert_bytes_report(
                    &std::fs::read(source.join(origin)).unwrap(),
                    VERSION,
                )
                .unwrap();
                HkxFile::read(&report.bytes).unwrap()
            });
            let mut has_locomotion = false;
            for cyclic in graph.objects().iter().filter(|o| {
                o.class_name == "BSCyclicBlendTransitionGenerator"
                    && string(o, "name").contains("LocomotionBlendGenerator")
            }) {
                has_locomotion = true;
                let blend = pointer(cyclic, "pBlenderGenerator").unwrap();
                for child in pointers(&graph.objects()[blend], "children") {
                    let clip = pointer(&graph.objects()[child], "generator").unwrap();
                    let name = string(&graph.objects()[clip], "animationName");
                    let path = &route.animations[&name];
                    let result = clips
                        .entry((path.clone(), name.clone()))
                        .or_insert_with(|| motion(&read(&source.join(path)).unwrap(), &name));
                    if result.is_none() {
                        failures.insert(format!("{path} ({name}): no locomotion direction"));
                    }
                }
            }
            if has_locomotion {
                let mut graph = graph.clone();
                if let Err(error) = repair(&mut graph, &source, &route.animations, origin, &vanilla)
                {
                    failures.insert(format!("{origin}: {error}"));
                } else {
                    repaired += 1;
                }
            }
        }
    }
    println!(
        "routes={} graphs={} repaired={} locomotion_clips={} in_place={}",
        plan.routes.len(),
        converted.len(),
        repaired,
        clips.len(),
        clips
            .values()
            .filter(|m| matches!(m, Some((_, speed)) if *speed == 0.0))
            .count()
    );
    assert!(
        failures.is_empty(),
        "{}",
        failures.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
#[ignore = "requires B21_BEHAVIOR_PLAN and extracted FO76/FO4 meshes; builds all planned assets"]
fn live_plan_asset_build() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap();
    let output = tempfile::Builder::new()
        .prefix("fo76_full_behavior_plan_")
        .tempdir_in(repo.join("tmp"))
        .unwrap()
        .keep();
    std::fs::create_dir_all(output.join("debug/fo76_behaviors")).unwrap();
    std::fs::copy(
        std::env::var("B21_BEHAVIOR_PLAN").unwrap(),
        output.join(PLAN_PATH),
    )
    .unwrap();
    println!("asset output: {}", output.display());
    let count = assets::build(
        &output,
        &repo.join("extracted/fo76"),
        &repo.join("extracted/fo4"),
    )
    .unwrap();
    assert!(count > 0);
    println!("built {count} files");
}
