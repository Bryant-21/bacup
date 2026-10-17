use super::*;
use crate::formkey_mapper::{MapperOptions, MapperState};
use crate::phase::creature_corpus::{prepare_bow_stagger_rows, project_recipe_stagger};
use crate::schema::AuthoringSchema;
use crate::session::open_session;
use crate::sym::StringInterner;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
};

fn rebind(mut recipe: SourceRigExecutableRecipe) -> SourceRigExecutableRecipe {
    recipe.field_receipts =
        SourceRigExecutableRecipe::required_field_receipts_from_source_for_intent(
            &recipe.rig,
            &recipe.graph,
            &recipe.projection,
            &recipe.key_plan,
            &recipe.batch_intent,
            None,
            "skyrim_special_edition",
        );
    SourceRigExecutableRecipe::from_json(&recipe.canonical_json().unwrap()).unwrap()
}

fn stagger_recipe() -> SourceRigExecutableRecipe {
    let mut recipe = executable_recipe_fixture("skyrimse", "Skyrim.esm", "skyrim_special_edition");
    let attack = &mut recipe.projection.attacks[0];
    attack.target_data.source_atkd = Some(CreatureAttackSourceDataReceipt {
        schema_id: "skyrimse-race-atkd-v1".into(),
        damage_multiplier_bits: attack.damage_multiplier.to_bits(),
        chance_bits: attack.chance.to_bits(),
        attack_spell: None,
        attack_flags: 0,
        attack_angle_bits: 0.0f32.to_bits(),
        strike_angle_bits: attack.strike_angle.to_bits(),
        stagger_bits: 0.4f32.to_bits(),
        attack_type: None,
        knockdown_bits: 0.0f32.to_bits(),
        recovery_time_bits: 0.0f32.to_bits(),
        stamina_multiplier_bits: 1.0f32.to_bits(),
        event: attack.event.clone(),
        ordinal: 0,
        attack_type_policy: CreatureAttackTypePolicy::NullOmitted,
        attack_spell_policy: CreatureAttackSpellPolicy::NoSourceSpell,
        stamina_multiplier_policy: CreatureAttackStaminaPolicy::PreserveAsActionPointsMultiplier {
            proof: semantic_proof(
                ATTACK_STAMINA_TO_ACTION_POINTS_PROOF_SCHEMA,
                serde_json::json!({
                    "evidence_blake3": blake3::hash(b"source stamina policy").to_hex().to_string(),
                    "field": "stamina_multiplier", "policy": "preserve_as_action_points_multiplier", "relation": "bit_identical"
                }),
            ),
        },
        stagger_offset_policy: CreatureAttackStaggerOffsetPolicy::ExplicitZero {
            proof: semantic_proof(
                ATTACK_STAGGER_OFFSET_ZERO_PROOF_SCHEMA,
                serde_json::json!({
                    "evidence_blake3": blake3::hash(b"source stagger policy").to_hex().to_string(),
                    "field": "stagger_offset", "policy": "explicit_zero", "target_value": 0
                }),
            ),
        },
    });
    rebind(recipe)
}

#[test]
fn bow_stagger_uses_final_events_and_rejects_wrong_target_before_commit() {
    let recipe = stagger_recipe();
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let plugin = recipe.projection.base.target_plugin.clone();
    let handle = plugin_handle_new_native(&plugin, Some("fo4")).unwrap();
    plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
    let mut mapper = MapperState::new(
        [],
        MapperOptions {
            output_plugin_name: plugin.clone(),
            ..Default::default()
        },
    );
    let mut retained = Vec::new();
    let mut session = open_session(handle, None).unwrap();
    {
        let prepared = prepare_creature_record_families_batch(
            &mut session,
            &mut mapper,
            vec![recipe.rebuild_record_family_batch(&interner).unwrap()],
            &schema,
            &interner,
        )
        .unwrap();
        let rows =
            prepare_bow_stagger_rows(std::slice::from_ref(&recipe), &retained, prepared.receipt())
                .unwrap();
        assert_eq!(rows[0].attacks[0].stagger.to_bits(), 0.4f32.to_bits());
        assert!(retained.is_empty());
        let mut wrong_race = recipe.clone();
        wrong_race.projection.base.form_keys.race.local += 0x100;
        assert!(prepare_bow_stagger_rows(&[wrong_race], &retained, prepared.receipt()).is_err());
        let mut wrong_event = recipe.clone();
        wrong_event.projection.attacks[0].event = "notAnEmittedEvent".into();
        assert_eq!(
            project_recipe_stagger(&wrong_event).unwrap().attacks[0].event,
            "notAnEmittedEvent"
        );
        assert_eq!(
            project_recipe_stagger(&wrong_event).unwrap().attacks[0].source_event,
            recipe.projection.attacks[0].event
        );
        assert!(prepare_bow_stagger_rows(&[wrong_event], &retained, prepared.receipt()).is_err());
        let mut wrong_plugin = recipe.clone();
        wrong_plugin.projection.base.form_keys.race.plugin = "Skyrim.esm".into();
        assert!(project_recipe_stagger(&wrong_plugin).is_err());
        let mut changed_records = recipe.clone();
        changed_records.projection.variants[0].health += 1;
        assert!(
            prepare_creature_record_commit_batch(
                prepared,
                &plugin,
                "fo4",
                &[rebind(changed_records)],
                &schema,
                &interner
            )
            .is_err()
        );
        assert!(
            retained.is_empty(),
            "a failed atomic record binding must not retain BowStagger rows"
        );
        assert!(mapper.source_to_target.is_empty());
    }
    {
        let prepared = prepare_creature_record_families_batch(
            &mut session,
            &mut mapper,
            vec![recipe.rebuild_record_family_batch(&interner).unwrap()],
            &schema,
            &interner,
        )
        .unwrap();
        let prepared = prepare_creature_record_commit_batch(
            prepared,
            &plugin,
            "fo4",
            std::slice::from_ref(&recipe),
            &schema,
            &interner,
        )
        .unwrap();
        let rows = prepare_bow_stagger_rows(
            &[recipe.clone(), recipe.clone()],
            &retained,
            prepared.receipt(),
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        let mut conflicting = recipe.clone();
        conflicting.projection.attacks[0]
            .target_data
            .source_atkd
            .as_mut()
            .unwrap()
            .stagger_bits = 0.5f32.to_bits();
        assert!(
            prepare_bow_stagger_rows(
                &[recipe.clone(), conflicting],
                &retained,
                prepared.receipt()
            )
            .is_err()
        );
        let committed = prepared.commit();
        retained = rows;
        assert_eq!(retained[0].race, recipe.projection.base.form_keys.race);
        assert_eq!(
            committed.receipt().families[0].races[0].attack_events,
            vec![retained[0].attacks[0].event.clone()]
        );
    }
    drop(session);
    plugin_handle_close_native(handle);
}
