use super::*;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_close_native, plugin_handle_load_index_no_py,
};

#[test]
#[ignore = "requires B21_BEHAVIOR_SOURCE_ESM and the current SeventySix output/extracted assets"]
fn live_additive_rigs_and_asset_build() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .unwrap();
    let source_path = std::env::var("B21_BEHAVIOR_SOURCE_ESM").unwrap();
    let source_root = repo.join("extracted/fo76/meshes");
    let target_root = repo.join("extracted/fo4/meshes");
    let source = plugin_handle_load_index_no_py(&source_path, Some("fo76"), None, None).unwrap();
    let target = plugin_handle_load_index_no_py(
        repo.join("mods/SeventySix/SeventySix.esm")
            .to_str()
            .unwrap(),
        Some("fo4"),
        None,
        None,
    )
    .unwrap();
    let previous: Plan = serde_json::from_slice(
        &std::fs::read(repo.join("mods/SeventySix").join(PLAN_PATH)).unwrap(),
    )
    .unwrap();
    let previous_routes: HashMap<_, _> = previous
        .routes
        .iter()
        .map(|r| (clean(&r.destination), r))
        .collect();
    let interner = StringInterner::new();
    let mut session = crate::session::open_session(target, Some(source)).unwrap();
    let schema = session.schema().unwrap();
    let source_schema = session.source_schema().unwrap();
    let source_keys: HashSet<_> = session
        .source_form_keys_of_sig(SigCode::from_str("RACE").unwrap(), &interner)
        .unwrap()
        .into_iter()
        .collect();
    let mut plan = Plan::default();
    let mut cache = BTreeMap::new();
    let mut checked_dirs = std::collections::BTreeSet::new();
    let mut built_races = std::collections::BTreeSet::new();
    let mut additive_count = 0;
    for fk in session
        .form_keys_of_sig(SigCode::from_str("RACE").unwrap(), &interner)
        .unwrap()
    {
        let race = session.record_decoded(&fk, &schema, &interner).unwrap();
        if !race.fields.iter().any(|f| f.sig.as_str() == "SADD") {
            continue;
        }
        additive_count += 1;
        let eid = race
            .eid
            .and_then(|s| interner.resolve(s))
            .unwrap_or_default();
        let target_project = uses_target_project(&race, source_keys.contains(&fk));
        let creature_project = if target_project {
            None
        } else {
            let original = session
                .source_record_decoded(&fk, &source_schema, &interner)
                .unwrap();
            Some(
                select_creature_project(&original, &race, &schema, &source_root, &interner)
                    .unwrap()
                    .unwrap(),
            )
        };
        for mut block in parse_canonical_subgraphs(&race) {
            let path = clean(interner.resolve(block.behaviour_graph).unwrap());
            let Some(old) = previous_routes.get(&path) else {
                continue;
            };
            block.behaviour_graph = interner.intern(&old.source);
            for path in &mut block.paths {
                let original = clean(interner.resolve(*path).unwrap());
                let original = original
                    .strip_prefix("actors/b21_fo76/source/fo4rig/")
                    .or_else(|| original.strip_prefix("actors/b21_fo76/source/source_rig/"))
                    .unwrap_or(&original);
                *path = interner.intern(original);
            }
            let project = if target_project {
                let dir = perspective_project_directory(
                    &target_project_directory(&race, &interner).unwrap(),
                    &block,
                    &interner,
                );
                assert_ne!(dir, "actors/shared");
                assert_ne!(dir, "actors/megasloth");
                if !dir.contains("_1stperson") && checked_dirs.insert(dir.clone()) {
                    let order =
                        super::super::assets::bone_order(&source_root, &target_root, &dir).unwrap();
                    assert!(
                        order.iter().any(Option::is_some),
                        "no matching bones for {dir}"
                    );
                }
                Project {
                    source: dir.clone(),
                    destination: dir,
                    vanilla: true,
                }
            } else {
                let original = creature_project.as_ref().unwrap();
                let dir = original.rsplit_once('/').unwrap().0;
                let project = Project {
                    source: original.clone(),
                    destination: format!(
                        "actors/B21_FO76/{}",
                        dir.strip_prefix("actors/").unwrap()
                    ),
                    vanilla: false,
                };
                assert_eq!(eid, "OguaRace");
                assert_eq!(project.source, "actors/megasloth/megaslothproject.hkx");
                project
            };
            let candidate = eid == "OguaRace"
                || ([
                    "HumanRaceAdditivePluginPort",
                    "DLC01RoboBrainRaceAdditivePluginPort",
                ]
                .contains(&eid)
                    && old.source.starts_with("actors/shared/"));
            if candidate && built_races.insert(eid.to_string()) {
                let route = make_route(
                    &source_root,
                    &old.source,
                    &block,
                    &project,
                    &interner,
                    &mut cache,
                )
                .unwrap();
                eprintln!("Building {eid}: {} -> {}", route.source, route.project);
                plan.routes.push(route);
                plan.projects.insert(project.destination.clone(), project);
            }
        }
    }
    drop(session);
    plugin_handle_close_native(target);
    plugin_handle_close_native(source);
    assert_eq!(additive_count, 39);
    assert_eq!(built_races.len(), 3, "{built_races:?}");
    let temp = tempfile::Builder::new()
        .prefix("fo76_additive_rigs_")
        .tempdir_in(repo.join("tmp"))
        .unwrap();
    std::fs::create_dir_all(temp.path().join("debug/fo76_behaviors")).unwrap();
    std::fs::write(
        temp.path().join(PLAN_PATH),
        serde_json::to_vec_pretty(&plan).unwrap(),
    )
    .unwrap();
    let written = super::super::assets::build(temp.path(), &source_root, &target_root).unwrap();
    assert!(
        temp.path()
            .join("data/Meshes/actors/B21_FO76/megasloth/characterassets/skeleton.hkx")
            .is_file()
    );
    assert!(
        !temp
            .path()
            .join("data/Meshes/actors/shared/characterassets/skeleton.hkx")
            .exists()
    );
    eprintln!(
        "Checked {additive_count} additive races and {} target rig directories; built {written} files: {}",
        checked_dirs.len(),
        temp.keep().display()
    );
}
