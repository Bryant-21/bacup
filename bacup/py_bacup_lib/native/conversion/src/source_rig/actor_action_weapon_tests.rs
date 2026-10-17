use super::*;

#[test]
fn indexed_weapon_actor_action_conditions_survive_plugin_roundtrip() {
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_no_py, plugin_handle_close_native, plugin_handle_load_no_py,
        plugin_handle_new_no_py, plugin_handle_read_raw_subrecords_no_py, plugin_handle_save_no_py,
    };

    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let interner = crate::sym::StringInterner::new();
    let mut emitted = emit_creature_capability_record_projection(
        &recipe.rig,
        &recipe.graph,
        &recipe.projection,
        &interner,
    )
    .unwrap();
    let plugin = &recipe.projection.base.target_plugin;
    let weapon = recipe.projection.base.form_keys.unarmed_weapon.clone();
    let plan = CreatureActorActionRecordPlan {
        requirement: ActorActionRequirement {
            kind: ActorActionKind::FireAuto,
            parent_editor_id: "ActionFireAuto".into(),
            parent_form_id: 0x004A5C,
            behavior_path: recipe.rig.paths.root_behavior.clone(),
            animation_event: "SourceFire_W000123".into(),
        },
        form_key: TargetFormKey::new(0xD80, plugin),
        editor_id: "B21_SourceWeaponAuto".into(),
        weapon_condition: Some(CreatureActorActionWeaponCondition {
            weapon: weapon.clone(),
            equipment_index: Some(3),
        }),
        weapon_exclusions: Vec::new(),
        graph_condition: Some(CreatureActorActionGraphCondition {
            name: "B21_SourceAttackGroup".into(),
            value: 26,
        }),
    };
    append_creature_actor_action_records(&mut emitted, std::slice::from_ref(&plan), &interner)
        .unwrap();
    let mut fallback = plan.clone();
    fallback.form_key.local += 1;
    fallback.editor_id = "B21_SourceUnarmedFallback".into();
    fallback.requirement.kind = ActorActionKind::Melee;
    fallback.requirement.parent_editor_id = "ActionMelee".into();
    fallback.requirement.parent_form_id = 0x004A59;
    fallback.requirement.animation_event = "SourceUnarmed".into();
    fallback.weapon_condition = None;
    fallback.weapon_exclusions = vec![weapon.clone()];
    fallback.graph_condition = None;
    append_creature_actor_action_records(&mut emitted, &[fallback], &interner).unwrap();
    let schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
    let handle = plugin_handle_new_no_py(plugin, Some("fo4"));
    plugin_handle_add_master_no_py(handle, "Fallout4.esm", None).unwrap();
    for record in emitted.closure.records {
        crate::target_write::add_record_native(handle, record, &schema, &interner).unwrap();
    }
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join(plugin);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    plugin_handle_close_native(handle);
    let handle =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    let key = format!("{plugin}:000D80");
    let conditions = plugin_handle_read_raw_subrecords_no_py(handle, &key, "CTDA").unwrap();
    let parent = plugin_handle_read_raw_subrecords_no_py(handle, &key, "ANAM").unwrap();
    let event = plugin_handle_read_raw_subrecords_no_py(handle, &key, "ENAM").unwrap();
    let graph_names = plugin_handle_read_raw_subrecords_no_py(handle, &key, "CIS1").unwrap();
    let fallback_conditions =
        plugin_handle_read_raw_subrecords_no_py(handle, &format!("{plugin}:000D81"), "CTDA")
            .unwrap();
    plugin_handle_close_native(handle);
    assert_eq!(conditions.len(), 3);
    assert!(conditions.iter().all(|bytes| bytes.len() == 32));
    assert_eq!(&conditions[0][8..10], &182u16.to_le_bytes());
    assert_eq!(&conditions[0][4..8], &1f32.to_le_bytes());
    assert_eq!(
        &conditions[0][12..16],
        &(0x01000000 | weapon.local).to_le_bytes()
    );
    assert_eq!(&conditions[1][8..10], &771u16.to_le_bytes());
    assert_eq!(&conditions[1][4..8], &3f32.to_le_bytes());
    assert_eq!(&conditions[1][12..28], &[0; 16]);
    assert_eq!(&conditions[2][8..10], &675u16.to_le_bytes());
    assert_eq!(&conditions[2][4..8], &26f32.to_le_bytes());
    assert_eq!(graph_names, [b"B21_SourceAttackGroup\0".to_vec()]);
    assert_eq!(&parent[0][..4], &0x004A5Cu32.to_le_bytes());
    assert_eq!(event, [b"SourceFire_W000123\0".to_vec()]);
    assert_eq!(fallback_conditions.len(), 1);
    assert_eq!(&fallback_conditions[0][8..10], &182u16.to_le_bytes());
    assert_eq!(&fallback_conditions[0][4..8], &0f32.to_le_bytes());
    assert_eq!(
        &fallback_conditions[0][12..16],
        &(0x01000000 | weapon.local).to_le_bytes()
    );
}

#[test]
fn automatic_actor_actions_admit_only_entry_and_route_release() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let mut graph = recipe.graph.clone();
    let base = graph
        .roles
        .iter()
        .find(|role| role.role == CreatureClipRole::MeleeAttack)
        .unwrap()
        .clone();
    for (phase, state) in [
        (ContinuousAttackPhase::Start, "Start"),
        (ContinuousAttackPhase::Loop, "Loop"),
        (ContinuousAttackPhase::Stop, "Stop"),
    ] {
        let mut role = base.clone();
        role.role = CreatureClipRole::ProjectileAttack;
        role.state_name = state.into();
        role.trigger_event = Some(format!("Profile{state}"));
        role.trigger_aliases.clear();
        role.generator = CapabilityRoleGenerator::ContinuousAttackPhase {
            phase,
            start_state: Some("Start".into()),
            loop_state: "Loop".into(),
            stop_state: Some("Stop".into()),
        };
        graph.roles.push(role);
    }
    graph.explicit_events.push(EventDecl {
        name: "attackRelease".into(),
        usage: EventUsage::Generic,
        flags: 0,
    });
    let requirements = required_actor_action_records(&graph, &recipe.rig.paths.root_behavior);
    let fire = requirements
        .iter()
        .filter(|requirement| {
            matches!(
                requirement.kind,
                ActorActionKind::FireAuto | ActorActionKind::FireSingle
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(fire.len(), 1);
    assert_eq!(fire[0].kind, ActorActionKind::FireAuto);
    assert_eq!(fire[0].parent_form_id, 0x004A5C);
    assert_eq!(fire[0].animation_event, "ProfileStart");
    assert!(
        requirements
            .iter()
            .any(|requirement| requirement.kind == ActorActionKind::FireStop
                && requirement.animation_event == "attackRelease"
                && requirement.parent_form_id == 0x013454)
    );
}

#[test]
fn thrown_profile_uses_native_throw_action_instead_of_gun_fire() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let mut graph = recipe.graph.clone();
    let mut role = graph
        .roles
        .iter()
        .find(|role| role.role == CreatureClipRole::MeleeAttack)
        .unwrap()
        .clone();
    role.role = CreatureClipRole::ProjectileAttack;
    role.generator = CapabilityRoleGenerator::ThrownAttack;
    role.equipment_index = Some(2);
    role.trigger_event = Some("SourceGrenadeThrow".into());
    role.trigger_aliases.clear();
    graph.roles.push(role);
    let requirements = required_actor_action_records(&graph, &recipe.rig.paths.root_behavior);
    let throw = requirements
        .iter()
        .filter(|action| action.animation_event == "SourceGrenadeThrow")
        .collect::<Vec<_>>();
    assert_eq!(throw.len(), 1);
    assert_eq!(throw[0].kind, ActorActionKind::Throw);
    assert_eq!(
        throw[0].parent_form_key(),
        TargetFormKey::new(0x004E32, "Fallout4.esm")
    );
}
