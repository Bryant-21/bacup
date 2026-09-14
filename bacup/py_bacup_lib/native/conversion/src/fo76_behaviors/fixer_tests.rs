use super::*;
use ck_native::anim_text_data::{core, emit, graph::GraphResolver, offsets, speed};

#[test]
#[ignore = "requires converted output, extracted assets and B21_FIXER_GAME_MESHES"]
fn live_fixer_route_assets() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap();
    let probe = repo.join("mods/B21_FO76FixerAnimations");
    let source = repo.join("extracted/fo76/meshes");
    let target = repo.join("extracted/fo4/meshes");
    let live = PathBuf::from(std::env::var("B21_FIXER_GAME_MESHES").unwrap());
    let scratch = repo.join("tmp/fixer-animation/native-build");
    let rows: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(probe.join("testing/source-block.json")).unwrap())
            .unwrap();
    let interner = StringInterner::new();
    let source_graph = rows
        .iter()
        .find_map(|r| r["BehaviourGraph"].as_str())
        .unwrap();
    let paths: Vec<_> = rows.iter().filter_map(|r| r["Path"].as_str()).collect();
    let block = SubgraphBlock {
        behaviour_graph: interner.intern(source_graph),
        paths: paths.iter().map(|p| interner.intern(p)).collect(),
        subgraph_keywords: vec![],
        target_keywords: vec![],
        flags_bytes: Some(smallvec::smallvec![1, 0, 0, 0]),
    };
    let project = Project {
        source: "actors/character".into(),
        destination: "actors/character".into(),
        vanilla: true,
    };
    let route = make_route(
        &source,
        &clean(source_graph),
        &block,
        &project,
        &interner,
        &mut BTreeMap::new(),
    )
    .unwrap();
    assert!(
        route
            .animations
            .values()
            .any(|p| p.contains("combatshotgun") && p.contains("reload"))
    );
    let mut plan = Plan::default();
    plan.projects.insert(project.destination.clone(), project);
    plan.routes.push(route.clone());
    std::fs::create_dir_all(scratch.join("debug/fo76_behaviors")).unwrap();
    std::fs::write(
        scratch.join(PLAN_PATH),
        serde_json::to_vec_pretty(&plan).unwrap(),
    )
    .unwrap();
    let written = super::super::assets::build(&scratch, &source, &target).unwrap();
    let meshes = scratch.join("data/Meshes");
    let graph = read(&meshes.join(&route.destination)).unwrap();
    assert!(
        graph
            .objects()
            .iter()
            .any(|o| o.class_name == "hkbClipGenerator"
                && string(o, "animationName").contains("combatshotgun"))
    );

    // Add this route's draw event to the already deployed union without discarding other ports.
    let mut root_paths = Vec::new();
    for name in [
        "RootBehavior.hkx",
        "RaiderRootBehavior.hkx",
        "FemaleRootBehavior.hkx",
    ] {
        let relative = format!("actors/character/behaviors/{name}");
        let path = live.join(&relative);
        if !path.is_file() {
            continue;
        }
        let mut root = read(&path).unwrap();
        crate::fixups::havok::normalize_weapon_behavior_contracts::merge_behavior_contracts(
            &mut root, &graph,
        )
        .unwrap();
        std::fs::write(meshes.join(&relative), root.save()).unwrap();
        root_paths.push(relative);
    }
    assert!(
        root_paths
            .iter()
            .any(|p| p.ends_with("RaiderRootBehavior.hkx"))
    );
    let chain: Vec<_> = paths
        .iter()
        .map(|p| format!("Actors\\B21_FO76\\Source\\fo4rig\\{}", p))
        .collect();
    let core_path = route.destination.replace('/', "\\");
    let id = core::subgraph_id(
        &core_path,
        &chain.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let roots = [meshes.as_path(), target.as_path()];
    let body = speed::build_speed_info_body_weapon(&core_path, &roots, &chain).unwrap();
    let decoded = speed::decode_speed_info(&body).unwrap();
    assert!(
        decoded
            .roots
            .iter()
            .any(|r| r.state_machine_path.ends_with("/Ready_IdleLocomotion"))
    );
    let loops = speed::speed_info_leaf_basenames(&core_path, &roots, &chain);
    let mut resolver = GraphResolver::new(roots.iter().map(|p| p.to_path_buf()).collect());
    let offset_body = offsets::build_subgraph_offsets_body_weapon(
        &mut resolver,
        &core_path,
        &roots,
        &chain,
        &loops,
    )
    .unwrap();
    for (bucket, bytes) in [
        ("AnimationSpeedInfo", body),
        ("AnimationOffsets", offset_body),
    ] {
        let dir = meshes.join("AnimTextData").join(bucket);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.txt")), bytes).unwrap();
    }
    let subgraph = emit::SubgraphInput {
        core_behavior: core_path,
        sapt_chain: chain.clone(),
        race_dir: Some("actors\\character".into()),
    };
    assert_eq!(
        emit::emit_animation_file_data(&[subgraph], &meshes, &meshes, Some(&target)).unwrap(),
        1
    );
    std::fs::write(
        probe.join("testing/route.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "route": route, "paths": chain, "id": id.to_string(), "roots": root_paths,
        }))
        .unwrap(),
    )
    .unwrap();
    eprintln!(
        "Fixer route {}: {written} assets, metadata id {id}",
        plan.routes[0].destination
    );
}
