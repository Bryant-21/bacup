use super::*;
use crate::source_rig::tests::executable_recipe_fixture;

fn actor_data(plugin: &str) -> CreatureNpcActorData {
    let reference = |signature: &str, local| CreatureTargetRecordReference {
        signature: signature.into(),
        form_key: TargetFormKey::new(local, plugin),
    };
    let mut ai = [0u8; 24];
    ai[..8].copy_from_slice(&[2, 3, 65, 1, 2, 1, 1, 0]);
    ai[8..12].copy_from_slice(&400u32.to_le_bytes());
    ai[12..16].copy_from_slice(&700u32.to_le_bytes());
    ai[16..20].copy_from_slice(&900u32.to_le_bytes());
    ai[20] = 1;
    CreatureNpcActorData {
        flags: 0x802,
        disposition_base: -17,
        factions: vec![CreatureNpcFaction { faction: reference("FACT", 0xE10), rank: -1 }, CreatureNpcFaction { faction: reference("FACT", 0xE11), rank: 4 }],
        ai_data: Some(ai),
        packages: vec![reference("PACK", 0xE13), reference("PACK", 0xE12)],
        combat_style: Some(reference("CSTY", 0xE14)),
        voice_type: Some(reference("VTYP", 0xE15)),
        script: Some(CreatureNpcScriptAttachment {
            source_script: CreatureSourceRecordReference {
                source_identity: SourceCreatureIdentity { namespace: "fnv".into(), plugin: "FalloutNV.esm".into(), local_form_id: 0x123 },
                signature: "SCPT".into(),
            },
            target_vmad: esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload(
                &serde_json::json!({"Version":6,"Object Format":2,"Scripts":[{"ScriptName":"B21_TestCreature","Flags":0,"Properties":[]}]}),
                &["Fallout4.esm".into()], plugin,
            ).unwrap(),
            dependencies: Vec::new(),
            target_masters: vec!["Fallout4.esm".into()],
        }),
    }
}

#[test]
fn creature_actor_data_survives_plugin_roundtrip_and_detects_loss() {
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_no_py, plugin_handle_close_native, plugin_handle_load_no_py,
        plugin_handle_new_no_py, plugin_handle_read_raw_subrecords_no_py, plugin_handle_save_no_py,
    };
    let mut recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let plugin = recipe.projection.base.target_plugin.clone();
    let expected = actor_data(&plugin);
    recipe.projection.variants[0].actor_data = Some(expected.clone());
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let emitted = emit_creature_capability_record_projection(
        &recipe.rig,
        &recipe.graph,
        &recipe.projection,
        &interner,
    )
    .unwrap();
    assert_eq!(emitted.npc_scripts.len(), 1);
    for dependency in expected.dependencies() {
        assert!(emitted.required_target_records.contains(&dependency));
    }
    let handle = plugin_handle_new_no_py(&plugin, Some("fo4"));
    plugin_handle_add_master_no_py(handle, "Fallout4.esm", None).unwrap();
    for record in emitted.closure.records {
        crate::target_write::add_record_native(handle, record, &schema, &interner).unwrap();
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(&plugin);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    plugin_handle_close_native(handle);
    let handle =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    let variant = &recipe.projection.variants[0];
    let form = format!("{plugin}:{:06X}", variant.form_key.local);
    let raw =
        |signature| plugin_handle_read_raw_subrecords_no_py(handle, &form, signature).unwrap();
    let acbs = raw("ACBS");
    assert_eq!(&acbs[0][..4], &expected.flags.to_le_bytes());
    assert_eq!(&acbs[0][12..14], &expected.disposition_base.to_le_bytes());
    assert_eq!(raw("AIDT"), vec![expected.ai_data.unwrap().to_vec()]);
    let factions = raw("SNAM");
    assert_eq!(
        factions.iter().map(|row| row[4] as i8).collect::<Vec<_>>(),
        [-1, 4]
    );
    assert!(factions.iter().all(|row| row.len() == 5));
    let packages = raw("PKID");
    assert_eq!(
        packages
            .iter()
            .map(|row| u32::from_le_bytes(row[..4].try_into().unwrap()) & 0x00ff_ffff)
            .collect::<Vec<_>>(),
        [0xE13, 0xE12]
    );
    assert_eq!(
        raw("VMAD"),
        vec![expected.script.as_ref().unwrap().target_vmad.clone()]
    );
    let mut actual = crate::source_read::read_record(handle, &form, &schema, &interner).unwrap();
    normalize_records(std::slice::from_mut(&mut actual), &schema, &interner).unwrap();
    verify_actor_data(variant, Some(&actual), &schema, &interner).unwrap();
    let mut missing_package = actual.clone();
    let index = missing_package
        .fields
        .iter()
        .position(|field| field.sig.as_str() == "PKID")
        .unwrap();
    missing_package.fields.remove(index);
    assert!(verify_actor_data(variant, Some(&missing_package), &schema, &interner).is_err());
    actual.fields.retain(|field| field.sig.as_str() != "VMAD");
    assert!(verify_actor_data(variant, Some(&actual), &schema, &interner).is_err());
    plugin_handle_close_native(handle);
}

#[test]
fn creature_actor_data_rejects_wrong_dependency_types_and_invalid_script_payload() {
    let recipe = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let mut actor = actor_data(&recipe.projection.base.target_plugin);
    actor.validate(&recipe.projection.base).unwrap();
    actor.factions[0].faction.signature = "CSTY".into();
    assert!(actor.validate(&recipe.projection.base).is_err());
    actor.factions[0].faction.signature = "FACT".into();
    actor.script.as_mut().unwrap().target_vmad.truncate(3);
    assert!(actor.validate(&recipe.projection.base).is_err());
}
