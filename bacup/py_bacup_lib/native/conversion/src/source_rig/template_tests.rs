use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::schema::AuthoringSchema;
use crate::session::open_session;
use crate::sym::StringInterner;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_load_no_py,
    plugin_handle_new_native, plugin_handle_read_raw_subrecords_no_py, plugin_handle_save_no_py,
};

fn bind_recipe(mut recipe: SourceRigExecutableRecipe) -> SourceRigExecutableRecipe {
    recipe.field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
            &recipe.rig,
            &recipe.graph,
            &recipe.projection,
            &recipe.key_plan,
            &recipe.batch_intent,
            None,
            "fallout_new_vegas",
        );
    SourceRigExecutableRecipe::from_json(&recipe.canonical_json().unwrap()).unwrap()
}

#[test]
fn cross_family_template_branches_commit_one_native_roll_and_roundtrip() {
    let mut first = executable_recipe_fixture("fnv", "FalloutNV.esm", "fallout_new_vegas");
    let plugin = first.projection.base.target_plugin.clone();
    let root = first.projection.source_primary_identity.clone();
    let generated = |tag: &str| {
        source_identity(
            "fnv",
            &format!(
                "B21_LegacyTemplate_{}.esm",
                blake3::hash(tag.as_bytes()).to_hex()
            ),
            root.local_form_id,
        )
    };
    let mut second = first.clone();
    let second_base = record_manifest("B21_SecondFamily", &plugin, 0xE00);
    second.projection.base = second_base.clone();
    second.projection.source_primary_identity = generated("second");
    second.projection.variants[0].source_identity = generated("second");
    second.projection.variants[0].form_key = second_base.form_keys.npc.clone();
    second.projection.variants[0].editor_id = second_base.editor_ids.npc.clone();
    second.key_plan.planned_source_identity = generated("second");
    second.key_plan.base = second_base.form_keys.clone();
    second.key_plan.armor_addons = vec![second_base.form_keys.armor_addon.clone()];
    second.key_plan.npc_variants[0].source_identity = generated("second");
    second.key_plan.npc_variants[0].form_key = second_base.form_keys.npc.clone();
    second.key_plan.melee_attacks[0].form_key = second_base.form_keys.unarmed_weapon.clone();
    if let CreatureAttackRecordProjection::MeleeUnarmed {
        weapon_form_key,
        weapon_editor_id,
        ..
    } = &mut second.projection.attacks[0].projection
    {
        *weapon_form_key = second_base.form_keys.unarmed_weapon.clone();
        *weapon_editor_id = second_base.editor_ids.unarmed_weapon.clone();
    }
    second.batch_intent.family_id = "second-family".into();
    second.batch_intent.primary_mapping = CreaturePrimaryRecordMapping {
        source: generated("second"),
        target: second_base.form_keys.npc.clone(),
    };
    let mut first_leaf = first.projection.variants[0].clone();
    first_leaf.primary = false;
    first_leaf.source_identity = generated("first");
    first_leaf.form_key = TargetFormKey::new(0xD10, &plugin);
    first_leaf.editor_id = "B21_FirstTemplateLeaf".into();
    first.key_plan.npc_variants.push(CreatureNpcRecordKeyPlan {
        source_identity: first_leaf.source_identity.clone(),
        form_key: first_leaf.form_key.clone(),
        primary: false,
    });
    first.projection.variants.push(first_leaf.clone());
    let root_list = TargetFormKey::new(0xD11, &plugin);
    let child_list = TargetFormKey::new(0xD12, &plugin);
    let entry = |level, count, signature, key| CreatureLeveledNpcEntry {
        level,
        count,
        reference: CreatureTargetRecordReference::new(signature, key),
        ownership: None,
    };
    let mut owned = entry(9, 2, "NPC_", second_base.form_keys.npc.clone());
    owned.ownership = Some(CreatureNpcInventoryOwnership::OwnerGlobal {
        owner: Some(CreatureTargetRecordReference::new(
            "NPC_",
            first_leaf.form_key.clone(),
        )),
        global: None,
        condition: 0.75,
    });
    first.projection.templates = Some(CreatureTemplateProjection {
        wrapper: first.projection.base.form_keys.npc.clone(),
        template: CreatureTargetRecordReference::new("LVLN", root_list.clone()),
        template_flags: 0x1fbf,
        leveled_lists: vec![
            CreatureLeveledNpcProjection {
                form_key: root_list.clone(),
                editor_id: "B21_TemplateRoot".into(),
                chance_none: 0,
                flags: 1,
                global: None,
                entries: vec![
                    entry(1, 1, "NPC_", first_leaf.form_key.clone()),
                    owned,
                    entry(15, 3, "LVLN", child_list.clone()),
                ],
            },
            CreatureLeveledNpcProjection {
                form_key: child_list.clone(),
                editor_id: "B21_TemplateNested".into(),
                chance_none: 0,
                flags: 0,
                global: None,
                entries: vec![
                    entry(1, 1, "NPC_", second_base.form_keys.npc.clone()),
                    entry(1, 1, "NPC_", second_base.form_keys.npc.clone()),
                ],
            },
        ],
    });
    first.batch_intent.required_target_records =
        first.projection.templates.as_ref().unwrap().dependencies();
    let recipes = vec![bind_recipe(first), bind_recipe(second)];
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let families = recipes
        .iter()
        .map(|recipe| recipe.rebuild_record_family_batch(&interner).unwrap())
        .collect::<Vec<_>>();
    let template_rows = families[0].closures[0]
        .closure
        .records
        .iter()
        .find(|record| record.form_key == root_list.intern(&interner))
        .unwrap();
    assert_eq!(
        template_rows
            .fields
            .iter()
            .filter(|field| matches!(field.sig.as_str(), "LVLO" | "COED"))
            .map(|field| field.sig.as_str())
            .collect::<Vec<_>>(),
        ["LVLO", "LVLO", "COED", "LVLO"]
    );
    let handle = plugin_handle_new_native(&plugin, Some("fo4")).unwrap();
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    let mut mapper = MapperState::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: plugin.clone(),
            ..Default::default()
        },
    );
    {
        let mut session = open_session(handle, None).unwrap();
        assert!(
            prepare_creature_record_families_batch(
                &mut session,
                &mut mapper,
                vec![families[0].clone()],
                &schema,
                &interner
            )
            .is_err(),
            "missing cross-family leaf must block publication"
        );
        assert!(mapper.source_to_target.is_empty());
        let prepared = prepare_creature_record_families_batch(
            &mut session,
            &mut mapper,
            families,
            &schema,
            &interner,
        )
        .unwrap();
        let committed = prepare_creature_record_commit_batch(
            prepared, &plugin, "fo4", &recipes, &schema, &interner,
        )
        .unwrap()
        .commit();
        assert_eq!(committed.receipt().family_ids.len(), 2);
        assert_eq!(committed.receipt().mapping_count, 2);
    }
    assert_eq!(
        mapper
            .source_to_target
            .get(&crate::ids::FormKey {
                plugin: interner.intern("FalloutNV.esm"),
                local: root.local_form_id,
            })
            .unwrap()
            .local,
        recipes[0].projection.base.form_keys.npc.local
    );
    let output = tempfile::tempdir().unwrap();
    let path = output.path().join(&plugin);
    plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
    plugin_handle_close_native(handle);
    let loaded =
        plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true).unwrap();
    let read = |key: &TargetFormKey, sig| {
        plugin_handle_read_raw_subrecords_no_py(
            loaded,
            &format!("{}:{:06X}", key.plugin, key.local),
            sig,
        )
        .unwrap()
    };
    let wrapper = &recipes[0].projection.base.form_keys.npc;
    let local = |bytes: &[u8]| u32::from_le_bytes(bytes.try_into().unwrap()) & 0x00ff_ffff;
    assert_eq!(local(&read(wrapper, "TPLT")[0]), root_list.local);
    let acbs = read(wrapper, "ACBS");
    assert_eq!(
        u32::from_le_bytes(acbs[0][0..4].try_into().unwrap()) & 0x300,
        0x100
    );
    assert_eq!(&acbs[0][14..16], &0x1fbfu16.to_le_bytes());
    let rows = read(&root_list, "LVLO");
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| (
                u16::from_le_bytes(row[0..2].try_into().unwrap()),
                local(&row[4..8]),
                u16::from_le_bytes(row[8..10].try_into().unwrap())
            ))
            .collect::<Vec<_>>(),
        [
            (1, first_leaf.form_key.local, 1),
            (9, second_base.form_keys.npc.local, 2),
            (15, child_list.local, 3)
        ]
    );
    assert_eq!(read(&child_list, "LVLO")[0], read(&child_list, "LVLO")[1]);
    assert_eq!(
        local(&read(&root_list, "COED")[0][0..4]),
        first_leaf.form_key.local
    );
    plugin_handle_close_native(loaded);
}
