use super::*;

use crate::phase::creature_corpus::weapon_publication::runtime_catalog;
use crate::source_rig::creature_weapon_runtime::{
    CreatureWeaponGraphGate, CreatureWeaponRuntimeKind, build_creature_weapon_runtime_catalog,
};

#[test]
fn creature_weapon_catalog_checks_committed_race_equipment_and_serialized_keys() {
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_no_py, plugin_handle_close_native, plugin_handle_load_no_py,
        plugin_handle_new_no_py, plugin_handle_read_raw_subrecords_no_py, plugin_handle_save_no_py,
    };
    let mut recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    recipe.projection.attacks[0].target_data.attack_flags = 4;
    let interner = crate::sym::StringInterner::new();
    let emitted = emit_creature_capability_record_projection(
        &recipe.rig,
        &recipe.graph,
        &recipe.projection,
        &interner,
    )
    .unwrap();
    let plugin = &recipe.projection.base.target_plugin;
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
    let attack_data = plugin_handle_read_raw_subrecords_no_py(
        handle,
        &format!(
            "{plugin}:{:06X}",
            recipe.projection.base.form_keys.race.local
        ),
        "ATKD",
    )
    .unwrap();
    assert_eq!(&attack_data[0][12..16], &4u32.to_le_bytes());
    let lookup = |key: &TargetFormKey| {
        crate::source_read::read_record(
            handle,
            &format!("{}:{:06X}", key.plugin, key.local),
            &schema,
            &interner,
        )
        .map(Some)
        .map_err(|error| error.to_string())
    };
    let mut catalog = build_creature_weapon_runtime_catalog(std::slice::from_ref(&recipe)).unwrap();
    let actor = &catalog.actors[0];
    assert_eq!(actor.attacks[0].kind, CreatureWeaponRuntimeKind::Natural);
    assert_eq!(actor.attacks[0].equip_index, 0);
    let data = runtime_catalog(&catalog, &interner, lookup).unwrap();
    assert_eq!(
        data["actors"][0]["npc"],
        format!(
            "{:06X}:{plugin}",
            recipe.projection.base.form_keys.npc.local
        )
    );
    assert_eq!(
        data["actors"][0]["race"],
        format!(
            "{:06X}:{plugin}",
            recipe.projection.base.form_keys.race.local
        )
    );

    let attack = &mut catalog.actors[0].attacks[0];
    attack.kind = CreatureWeaponRuntimeKind::Melee;
    attack.weapons = vec![recipe.projection.base.form_keys.unarmed_weapon.clone()];
    attack.graph_gate = Some(CreatureWeaponGraphGate {
        name: "B21_AttackGroup".into(),
        value: 26,
        movement_opening: false,
    });
    let data = runtime_catalog(&catalog, &interner, lookup).unwrap();
    assert_eq!(
        data["actors"][0]["attacks"][0]["weapons"][0],
        format!(
            "{:06X}:{plugin}",
            recipe.projection.base.form_keys.unarmed_weapon.local
        )
    );
    assert_eq!(data["actors"][0]["attacks"][0]["graph_gate"]["value"], 26);
    catalog.actors[0].attacks[0].equip_index = 1;
    assert!(
        runtime_catalog(&catalog, &interner, lookup)
            .unwrap_err()
            .contains("ATKW")
    );
    catalog.actors[0].attacks[0].equip_index = 0;
    catalog.actors[0].attacks[0].weapons[0] = recipe.projection.base.form_keys.npc.clone();
    assert!(
        runtime_catalog(&catalog, &interner, lookup)
            .unwrap_err()
            .contains("WEAP")
    );
    plugin_handle_close_native(handle);
}

#[test]
fn creature_weapon_catalog_rejects_conflicting_race_profiles() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let mut conflicting = recipe.clone();
    conflicting.projection.base.form_keys.npc.local += 100;
    conflicting.projection.attacks.clear();
    assert!(
        build_creature_weapon_runtime_catalog(&[recipe, conflicting])
            .unwrap_err()
            .contains("conflicting native weapon")
    );
}

#[test]
fn creature_weapon_catalog_exports_only_the_aim_bank_claiming_the_exact_attack() {
    let mut recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let event = recipe.projection.attacks[0].event.clone();
    let mut aim = recipe.graph.roles[0].clone();
    aim.role = CreatureClipRole::WeaponAim;
    aim.trigger_event = None;
    aim.generator = CapabilityRoleGenerator::SourceAim {
        bank_index: 1, attack_events: vec![event],
        aim_children: vec![CapabilityBlenderChild { clip_name: aim.clip_name.clone(), weight_bits: 0.0f32.to_bits(), world_from_model_weight_bits: 1.0f32.to_bits() }],
    };
    recipe.graph.roles.push(aim.clone());
    let catalog = build_creature_weapon_runtime_catalog(std::slice::from_ref(&recipe)).unwrap();
    assert_eq!(catalog.actors[0].attacks[0].aim_bank, Some(1));
    if let CapabilityRoleGenerator::SourceAim { bank_index, .. } = &mut aim.generator { *bank_index = 4; }
    recipe.graph.roles.push(aim);
    assert!(build_creature_weapon_runtime_catalog(&[recipe]).unwrap_err().contains("multiple source aim banks"));
}

#[test]
fn creature_weapon_catalog_preserves_movement_source_order_and_rejects_conflicts() {
    let mut recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let clip_name = recipe.graph.roles[0].clip_name.clone();
    let path = recipe
        .rig
        .clips
        .iter()
        .find(|clip| clip.name == clip_name)
        .unwrap()
        .path
        .clone();
    let source = crate::source_rig::SourceMovementClip {
        clip_name,
        source_order: 7,
        keys: vec![
            crate::source_rig::SourceMovementKey {
                time_bits: 0.7f32.to_bits(),
                group: 26,
            },
            crate::source_rig::SourceMovementKey {
                time_bits: 0.2f32.to_bits(),
                group: 32,
            },
        ],
    };
    recipe.graph.roles[0].generator = CapabilityRoleGenerator::SourceMovement {
        clips: vec![source.clone(), source.clone()],
        generator: Box::new(CapabilityRoleGenerator::Single),
    };
    let catalog = build_creature_weapon_runtime_catalog(std::slice::from_ref(&recipe)).unwrap();
    let movement = &catalog.actors[0].movement;
    assert_eq!(movement.len(), 1);
    assert_eq!(movement[0].animation, path);
    assert_eq!(movement[0].source_order, 7);
    assert_eq!(
        movement[0]
            .keys
            .iter()
            .map(|key| (key.time, key.group))
            .collect::<Vec<_>>(),
        [(0.7, 26), (0.2, 32)]
    );
    if let CapabilityRoleGenerator::SourceMovement { clips, .. } =
        &mut recipe.graph.roles[0].generator
    {
        clips[1].source_order += 1;
    }
    assert!(
        build_creature_weapon_runtime_catalog(&[recipe])
            .unwrap_err()
            .contains("conflicting runtime descriptions")
    );
}
