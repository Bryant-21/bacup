use super::*;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

fn compound_route(route: &Route) -> bool {
    matches!(
        clean(&route.project).as_str(),
        "actors/character"
            | "actors/character/_1stperson"
            | "actors/powerarmor"
            | "actors/powerarmor/_1stperson"
    ) && route.animations.values().any(|path| {
        let path = clean(path);
        path.ends_with("/wpnidleready.hkx")
            && (path.starts_with("actors/character/animations/weapon/compoundbow/")
                || path.starts_with("actors/character/_1stperson/animations/compoundbow/"))
    })
}

pub(super) fn power_armor_route(route: &Route) -> bool {
    clean(&route.project).starts_with("actors/powerarmor") && compound_route(route)
}

pub(super) fn animation_namespace(project: &Project, route: &Route) -> &'static str {
    if project.vanilla && power_armor_route(route) {
        "fo4rig_pa_bow"
    } else if project.vanilla {
        "fo4rig"
    } else {
        "source_rig"
    }
}

pub(super) fn suppress_power_armor_first_person_fidgets(
    file: &mut HkxFile,
    route: &Route,
) -> usize {
    if !power_armor_route(route) || clean(&route.project) != "actors/powerarmor/_1stperson" {
        return 0;
    }
    let Some(event) = file
        .objects()
        .iter()
        .find(|o| o.class_name == "hkbBehaviorGraphStringData")
        .and_then(|o| {
            array(o, "eventNames")
                .iter()
                .position(|e| text(e) == Some("fidgetStart"))
        })
    else {
        return 0;
    };
    let mut changed = 0;
    for object in file.objects_mut() {
        if object.class_name != "hkbStateMachineTransitionInfoArray" {
            continue;
        }
        let mut transitions = array(object, "transitions");
        let before = transitions.len();
        transitions.retain(|transition| {
            !transition.as_object_members().is_some_and(|fields| {
                fields
                    .iter()
                    .any(|m| m.name == "eventId" && number(&m.value) == Some(event as f32))
            })
        });
        if transitions.len() != before {
            changed += before - transitions.len();
            set(object, "transitions", HkxValue::Array(transitions));
        }
    }
    changed
}

pub(super) fn repair_player_locomotion(file: &mut HkxFile, route: &Route) -> usize {
    if !compound_route(route) || clean(&route.project).contains("_1stperson") {
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
    // Ready, Sneak and Relaxed locomotion each pick their human eight-way set by IsHuman.
    let sets: std::collections::BTreeSet<usize> = file
        .objects()
        .iter()
        .filter(|o| o.class_name == "hkbManualSelectorGenerator")
        .filter_map(|o| pointer(o, "variableBindingSet"))
        .collect();
    let mut changed = 0;
    for set_index in sets {
        let mut bindings = array(&file.objects()[set_index], "bindings");
        let before = changed;
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
        if changed != before {
            set(
                &mut file.objects_mut()[set_index],
                "bindings",
                HkxValue::Array(bindings),
            );
        }
    }
    changed
}

pub(super) fn repair_release_condition(
    record: &mut Record,
    route: &Route,
    interner: &StringInterner,
) {
    if !compound_route(route) || clean(&route.project).contains("_1stperson")
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
            ("actors/powerarmor", "compoundbow", 2),
            ("actors/powerarmor/_1stperson", "compoundbow", 3),
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
}
