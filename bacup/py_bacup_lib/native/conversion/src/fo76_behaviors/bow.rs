use super::*;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

fn compound_route(route: &Route) -> bool {
    matches!(
        clean(&route.project).as_str(),
        "actors/character" | "actors/character/_1stperson"
    ) && route.animations.values().any(|path| {
        let path = clean(path);
        path.ends_with("/wpnidleready.hkx")
            && (path.starts_with("actors/character/animations/weapon/compoundbow/")
                || path.starts_with("actors/character/_1stperson/animations/compoundbow/"))
    })
}

pub(super) fn repair_player_locomotion(file: &mut HkxFile, route: &Route) -> usize {
    if !compound_route(route) || clean(&route.project) != "actors/character" {
        return 0;
    }
    let Some(strings) = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
    else {
        return 0;
    };
    let names = array(strings, "variableNames");
    let (Some(human), Some(player)) = (
        names.iter().position(|v| text(v) == Some("IsHuman")),
        names.iter().position(|v| text(v) == Some("iIsPlayer")),
    ) else {
        return 0;
    };
    let Some(binding) = file
        .objects()
        .iter()
        .find(|o| {
            o.class_name == "hkbManualSelectorGenerator"
                && string(o, "name") == "Ready_Locomotion Selector"
        })
        .and_then(|o| pointer(o, "variableBindingSet"))
    else {
        return 0;
    };
    let mut bindings = array(&file.objects()[binding], "bindings");
    let mut changed = 0;
    for binding in &mut bindings {
        let Some(fields) = binding.as_object_members_mut() else {
            continue;
        };
        if !fields
            .iter()
            .any(|m| m.name == "memberPath" && text(&m.value) == Some("selectedGeneratorIndex"))
            || !fields
                .iter()
                .any(|m| m.name == "bindingType" && number(&m.value) == Some(0.0))
        {
            continue;
        }
        if let Some(index) = fields
            .iter_mut()
            .find(|m| m.name == "variableIndex" && number(&m.value) == Some(human as f32))
        {
            // FO4 leaves IsHuman at zero; iIsPlayer selects the source human bow poses for the player.
            index.value = HkxValue::I32(player as i32);
            changed += 1;
        }
    }
    if changed != 0 {
        set(
            &mut file.objects_mut()[binding],
            "bindings",
            HkxValue::Array(bindings),
        );
    }
    changed
}

pub(super) fn repair_release_condition(
    record: &mut Record,
    route: &Route,
    interner: &StringInterner,
) {
    if !compound_route(route) || clean(&route.project) != "actors/character"
        || !record.fields.iter().any(|f| f.sig.as_str() == "ENAM"
            && matches!(&f.value, FieldValue::String(s) if interner.resolve(*s) == Some("attackReleaseChargingHoldForceFire"))) {
        return;
    }
    // The FO76 charge-time OR alternative is unavailable; the separate bow adapter gates readiness.
    record.fields.retain(|field| {
        !matches!(&field.value, FieldValue::Bytes(bytes)
        if field.sig.as_str() == "CTDA" && bytes.as_slice() == [
            0x61, 0, 0, 0, 0, 0, 0x10, 0x41, 0xE2, 2, 0x14, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF,
        ])
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    fn route(project: &str, weapon: &str) -> Route {
        Route {
            project: project.into(),
            source: String::new(),
            destination: String::new(),
            dependencies: BTreeMap::new(),
            draw_event: None,
            animations: BTreeMap::from([(
                "idle".into(),
                format!("actors/character/animations/weapon/{weapon}/player/wpnidleready.hkx"),
            )]),
        }
    }

    #[test]
    fn compound_bow_release_preserves_unrelated_conditions_and_routes() {
        let interner = StringInterner::new();
        let bytes = hex::decode("6100000000001041E202140000000000000000000000000000000000FFFFFFFF")
            .unwrap();
        let mut record = Record::new(
            SigCode::from_str("IDLE").unwrap(),
            FormKey {
                local: 0x800,
                plugin: interner.intern("B21_Test.esp"),
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ENAM").unwrap(),
            value: FieldValue::String(interner.intern("attackReleaseChargingHoldForceFire")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(bytes.clone().into()),
        });
        let mut other = bytes;
        other[6..8].copy_from_slice(&[0, 0x40]);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(other.into()),
        });
        for (project, weapon, count) in [
            ("actors/character", "compoundbow", 2),
            ("actors/character/_1stperson", "compoundbow", 3),
            ("actors/character", "regularbow", 3),
            ("actors/scorched", "compoundbow", 3),
        ] {
            let mut candidate = record.clone();
            repair_release_condition(&mut candidate, &route(project, weapon), &interner);
            assert_eq!(candidate.fields.len(), count);
            let once = candidate.fields.clone();
            repair_release_condition(&mut candidate, &route(project, weapon), &interner);
            assert_eq!(candidate.fields, once);
        }
        let mut rifle = route("actors/character", "assaultrifle");
        rifle.animations.insert(
            "charge".into(),
            "actors/character/animations/weapon/compoundbow/player/wpnchargeholdreadyadd.hkx"
                .into(),
        );
        assert!(!compound_route(&rifle));
    }

    #[test]
    #[ignore = "requires the September 9 bow proof HKX fixtures"]
    fn live_compound_bow_locomotion_matches_user_validated_graph() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .unwrap();
        let evidence = repo.join("mods/B21_FO76BowReleaseProbe/testing/evidence");
        let original = read(&evidence.join("third-before-human-locomotion.hkx")).unwrap();
        let expected = read(&repo.join("mods/B21_FO76BowReleaseProbe/data/Meshes/actors/character/behaviors/B21_FO76_gunbehavior_1563fc0113cf.hkx")).unwrap();
        let mut graph = original.clone();
        assert_eq!(
            repair_player_locomotion(&mut graph, &route("actors/character", "compoundbow")),
            1
        );
        assert_eq!(graph.save(), expected.save());
        assert_eq!(
            repair_player_locomotion(&mut graph, &route("actors/character", "compoundbow")),
            0
        );
        for r in [
            route("actors/character", "regularbow"),
            route("actors/scorched", "compoundbow"),
            route("actors/character/_1stperson", "compoundbow"),
        ] {
            let mut graph = original.clone();
            assert_eq!(repair_player_locomotion(&mut graph, &r), 0);
            assert_eq!(graph.save(), original.save());
        }
    }

    #[test]
    #[ignore = "requires the converted plan and extracted FO76/FO4 assets"]
    fn live_compound_bow_asset_pipeline() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(4)
            .unwrap();
        let mut plan: Plan = serde_json::from_slice(
            &std::fs::read(repo.join("mods/SeventySix").join(PLAN_PATH)).unwrap(),
        )
        .unwrap();
        plan.routes.retain(compound_route);
        assert_eq!(plan.routes.len(), 2);
        plan.projects
            .retain(|key, _| plan.routes.iter().any(|r| &r.project == key));
        let output = tempfile::Builder::new()
            .prefix("bow_pipeline_")
            .tempdir_in(repo.join("tmp"))
            .unwrap()
            .keep();
        std::fs::create_dir_all(output.join("debug/fo76_behaviors")).unwrap();
        std::fs::write(
            output.join(PLAN_PATH),
            serde_json::to_vec_pretty(&plan).unwrap(),
        )
        .unwrap();
        assets::build(
            &output,
            &repo.join("extracted/fo76/meshes"),
            &repo.join("extracted/fo4/meshes"),
        )
        .unwrap();
        let third = plan
            .routes
            .iter()
            .find(|r| r.project == "actors/character")
            .unwrap();
        let path = third
            .dependencies
            .iter()
            .find(|(source, _)| source.ends_with("/gunbehavior.hkx"))
            .unwrap()
            .1;
        let graph = read(&output.join("data/Meshes").join(path)).unwrap();
        let names = array(
            graph
                .objects()
                .iter()
                .find(|o| o.class_name == "hkbBehaviorGraphStringData")
                .unwrap(),
            "variableNames",
        );
        let selector = graph
            .objects()
            .iter()
            .find(|o| string(o, "name") == "Ready_Locomotion Selector")
            .unwrap();
        let bindings = array(
            &graph.objects()[pointer(selector, "variableBindingSet").unwrap()],
            "bindings",
        );
        let index = bindings[0]
            .as_object_members()
            .unwrap()
            .iter()
            .find(|m| m.name == "variableIndex")
            .and_then(|m| number(&m.value))
            .unwrap() as usize;
        assert_eq!(text(&names[index]), Some("iIsPlayer"));
        let repaired = crate::fixups::havok::repair_weapon_charge_reference_frames::repair_weapon_charge_reference_frames_in_mod_path(&output).unwrap();
        assert_eq!(repaired.records_changed, 2);
        for leaf in ["wpnchargeholdreadyadd.hkx", "wpnchargeholdsightedadd.hkx"] {
            let relative = format!(
                "data/Meshes/actors/B21_FO76/Source/fo4rig/actors/character/_1stperson/animations/compoundbow/{leaf}"
            );
            let actual = read(&output.join(&relative)).unwrap();
            let expected = read(&repo.join("mods/B21_FO76BowReleaseProbe").join(relative)).unwrap();
            assert_eq!(actual.save(), expected.save(), "{leaf}");
        }
        println!("Compound Bow pipeline fixture: {}", output.display());
    }
}
