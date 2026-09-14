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
fn source_creature_additives_do_not_select_target_rigs() {
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

#[test]
fn shared_graphs_follow_race_project_and_perspective() {
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

#[test]
fn ungendered_projects_keep_authored_order_including_shared_critter_graphs() {
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

#[test]
fn gendered_projects_are_not_silently_collapsed() {
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

#[test]
fn repaired_source_projects_take_priority_but_foreign_projects_do_not() {
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

#[test]
#[ignore = "requires B21_BEHAVIOR_SOURCE_ESM, B21_BEHAVIOR_TARGET_ESM and B21_BEHAVIOR_SOURCE_MESHES"]
fn live_race_project_selection_audit() {
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_load_index_no_py,
    };
    let source_path = std::env::var("B21_BEHAVIOR_SOURCE_ESM").unwrap();
    let target_path = std::env::var("B21_BEHAVIOR_TARGET_ESM").unwrap();
    let meshes = PathBuf::from(std::env::var("B21_BEHAVIOR_SOURCE_MESHES").unwrap());
    let source = plugin_handle_load_index_no_py(&source_path, Some("fo76"), None, None).unwrap();
    let target = plugin_handle_load_index_no_py(&target_path, Some("fo4"), None, None).unwrap();
    let interner = StringInterner::new();
    let mut session = crate::session::open_session(target, Some(source)).unwrap();
    let schema = session.schema().unwrap();
    let source_schema = session.source_schema().unwrap();
    let mut checked = 0;
    let mut mismatched = 0;
    let mut failures = Vec::new();
    let mut selections = BTreeMap::new();
    for fk in session
        .source_form_keys_of_sig(SigCode::from_str("RACE").unwrap(), &interner)
        .unwrap()
    {
        let original = session
            .source_record_decoded(&fk, &source_schema, &interner)
            .unwrap();
        let Ok(repaired) = session.record_decoded(&fk, &schema, &interner) else {
            continue;
        };
        checked += 1;
        let projects = race_projects(&original, &interner);
        if projects.len() > 1 {
            mismatched += 1;
        }
        match select_creature_project(&original, &repaired, &schema, &meshes, &interner) {
            Ok(Some(project)) => {
                if projects.len() > 1 && !meshes.join(&project).is_file() {
                    failures.push(format!(
                        "{:06X}: selected source project missing: {project}",
                        fk.local
                    ));
                }
                selections.insert(fk.local, project);
            }
            Ok(None) => {}
            Err(e) => failures.push(e.to_string()),
        }
    }
    drop(session);
    plugin_handle_close_native(target);
    plugin_handle_close_native(source);
    assert_eq!(
        selections.get(&0x111233).map(String::as_str),
        Some("actors/dlc03/radchicken/dlc03_radchickenproject.hkx")
    );
    eprintln!(
        "Project audit: {checked} converted races checked, {mismatched} source gender mismatches, {} failures",
        failures.len()
    );
    assert!(checked > 0 && mismatched > 0);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "requires B21_BEHAVIOR_SOURCE_ESM, B21_BEHAVIOR_TARGET_ESM and B21_BEHAVIOR_SOURCE_MESHES"]
fn live_source_behavior_dependency_audit() {
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_load_index_no_py,
    };
    let source_path = std::env::var("B21_BEHAVIOR_SOURCE_ESM").unwrap();
    let target_path = std::env::var("B21_BEHAVIOR_TARGET_ESM").unwrap();
    let meshes = PathBuf::from(std::env::var("B21_BEHAVIOR_SOURCE_MESHES").unwrap());
    let source = plugin_handle_load_index_no_py(&source_path, Some("fo76"), None, None).unwrap();
    let target = plugin_handle_load_index_no_py(&target_path, Some("fo4"), None, None).unwrap();
    let interner = StringInterner::new();
    let mut session = crate::session::open_session(target, Some(source)).unwrap();
    let schema = session.schema().unwrap();
    let source_schema = session.source_schema().unwrap();
    let mut cache = BTreeMap::new();
    let mut graphs = std::collections::BTreeSet::new();
    let mut projects = std::collections::BTreeSet::new();
    let mut failures = Vec::new();
    let mut skipped = Vec::new();
    for fk in session
        .source_form_keys_of_sig(SigCode::from_str("RACE").unwrap(), &interner)
        .unwrap()
    {
        let original = session
            .source_record_decoded(&fk, &source_schema, &interner)
            .unwrap();
        let blocks = parse_canonical_subgraphs(&original);
        if !missing_source_cores(&meshes, &blocks, &interner)
            .unwrap()
            .is_empty()
        {
            skipped.push(fk.local);
            continue;
        }
        for block in &blocks {
            let graph = clean(interner.resolve(block.behaviour_graph).unwrap());
            if preserved_graph(&graph) {
                graphs.insert(graph);
            }
        }
        if blocks.is_empty() {
            continue;
        }
        let Ok(repaired) = session.record_decoded(&fk, &schema, &interner) else {
            continue;
        };
        if let Some(project) =
            select_creature_project(&original, &repaired, &schema, &meshes, &interner).unwrap()
        {
            projects.insert(project);
        }
    }
    for project in &projects {
        match project_roots(&meshes, project) {
            Ok(roots) => graphs.extend(roots),
            Err(e) => failures.push(e),
        }
    }
    for graph in &graphs {
        if let Err(e) = dependencies(&meshes, graph, &mut cache) {
            failures.push(e);
        }
    }
    drop(session);
    plugin_handle_close_native(target);
    plugin_handle_close_native(source);
    eprintln!(
        "Dependency audit: {} projects, {} direct/root graphs, {} dependency files, {} skipped races, {} failures\n{}",
        projects.len(),
        graphs.len(),
        cache.len(),
        skipped.len(),
        failures.len(),
        failures.join("\n")
    );
    assert!(!graphs.is_empty());
    assert_eq!(
        skipped,
        [0x6820AD],
        "only the unshipped MechTest core should be skipped"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
