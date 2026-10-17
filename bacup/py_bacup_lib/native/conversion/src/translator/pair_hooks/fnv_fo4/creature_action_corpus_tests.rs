use super::super::super::creature_catalog::CreatureProvenance;
use super::super::super::creature_dependencies::{
    CreatureDependencyOptions, LegacyPluginLoadOrder,
    build_creature_dependency_ledger_with_load_orders,
    required_fnv_fo3_creature_runtime_dependency_signatures,
};
use super::super::super::creature_live_builder::{
    FnvFo3LiveRecipeBuildInput, build_live_fnv_fo3_creature_recipe_evidence,
};
use super::*;

#[test]
#[ignore = "requires the source plugin inventory and distinct extracted FNV/FO3 roots"]
fn real_full_source_creature_actions_stage_and_pack() {
    let workspace = PathBuf::from(std::env::var("B21_FNV_CREATURE_ACTION_CORPUS").unwrap());
    let inventory: serde_json::Value = serde_json::from_slice(
        &fs::read(workspace.join("tmp/creature_body_part_plugins.json")).unwrap(),
    )
    .unwrap();
    let interner = StringInterner::new();
    let signatures = required_fnv_fo3_creature_runtime_dependency_signatures();
    let signature_bytes = signatures
        .iter()
        .map(|signature| signature.as_bytes().try_into().unwrap())
        .collect::<Vec<[u8; 4]>>();
    let mut source_records = Vec::new();
    let mut load_orders = Vec::new();
    for (precedence, entry) in inventory.as_array().unwrap().iter().enumerate() {
        let (game, game_name) = match entry["game"].as_str().unwrap() {
            "fnv" => (LegacyCreatureGame::Fnv, "fnv"),
            "fo3" => (LegacyCreatureGame::Fo3, "fo3"),
            _ => continue,
        };
        let path = PathBuf::from(entry["path"].as_str().unwrap());
        let handle =
            crate::run::OwnedPluginHandle::load_projection(&path, game_name, &signature_bytes)
                .unwrap();
        let (plugin, masters) = crate::source_read::plugin_context_for_handle(handle.id()).unwrap();
        load_orders.push(LegacyPluginLoadOrder {
            game,
            source_plugin: plugin.clone(),
            source_master_names: masters,
        });
        let schema = crate::schema::AuthoringSchema::for_game(game_name).unwrap();
        for signature in signatures {
            for key in esp_authoring_core::plugin_runtime::plugin_handle_record_form_keys_by_signature_no_py(handle.id(), signature).unwrap() {
                let record = crate::source_read::read_record(handle.id(), &key, &schema, &interner).unwrap();
                source_records.push((record, CreatureProvenance { game, source_plugin: plugin.clone(), precedence: precedence as u32 }));
            }
        }
    }
    assert_eq!(
        load_orders.len(),
        12,
        "all installed FNV/FO3 masters must be inspected"
    );
    let sources = source_records
        .iter()
        .map(|(record, provenance)| LegacyRecordSource {
            record,
            provenance: provenance.clone(),
        })
        .collect::<Vec<_>>();
    let dependency = build_creature_dependency_ledger_with_load_orders(
        &sources,
        CreatureDependencyOptions::default(),
        &load_orders,
        &interner,
    )
    .unwrap();
    let source_root = workspace.join("extracted");
    let evidence = build_live_fnv_fo3_creature_recipe_evidence(
        FnvFo3LiveRecipeBuildInput {
            winning_records: &sources,
            dependency_ledger: &dependency,
            source_data_root: &source_root,
            expected_creature_winners: None,
        },
        &interner,
    )
    .unwrap();
    fs::write(
        workspace.join("tmp/fnv_fo3_full_source_action_evidence.json"),
        serde_json::to_vec_pretty(&evidence.recipe_ledger).unwrap(),
    )
    .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut rows = Vec::new();
    let mut failures = Vec::new();
    for family in &evidence.recipe_ledger.families {
        let id = canonical_fnv_fo3_family_id(&family.rig);
        match stage_source_family(family, &source_root, temp.path()) {
            Ok(row) => rows.push(row),
            Err(error) => failures.push(serde_json::json!({"family": id, "error": error})),
        }
    }
    fs::write(workspace.join("tmp/fnv_fo3_full_source_action_audit.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "source_plugins": load_orders.iter().map(|order| &order.source_plugin).collect::<Vec<_>>(),
        "accounting": evidence.recipe_ledger.accounting,
        "families": rows,
        "failures": failures,
    })).unwrap()).unwrap();
    assert!(
        failures.is_empty(),
        "source creature action failures: {failures:#?}"
    );
    assert_eq!(rows.len(), evidence.recipe_ledger.families.len());
    assert!(
        rows.len() > 63,
        "original per-game families must exceed the old merged corpus"
    );
}

fn stage_source_family(
    family: &CreatureFamilyRecipeJob,
    source_root: &Path,
    output: &Path,
) -> Result<serde_json::Value, String> {
    let selected = select_family_graph(family)?;
    let family_id = canonical_fnv_fo3_family_id(&family.rig);
    let hash = blake3::hash(family_id.as_bytes()).to_hex();
    let creature_name = format!("FnvFo3Creature_{}", &hash[..16]);
    let runtime_root = format!("Actors\\{creature_name}");
    let skeleton_path = format!("{runtime_root}\\CharacterAssets\\Skeleton.hkx");
    let claim = family
        .assets
        .iter()
        .find(|asset| asset.skeleton_runtime_name.is_some())
        .ok_or("source family has no skeleton claim")?;
    let game_root = game_data_root(source_root, family.rig.game);
    let skeleton = crate::phase::skeleton::convert_source_owned_skeleton_artifact(
        &source_mesh_path(&game_root, &family.rig.skeleton_path),
        &skeleton_path,
        claim.skeleton_runtime_name.as_deref().unwrap(),
        &claim.skeleton_float_slots,
    )
    .map_err(|error| error.to_string())?;
    let mut policies = BTreeMap::new();
    let mut clips = Vec::new();
    let mut clip_rows = Vec::new();
    for selection in &selected.clips {
        let path = format!("{runtime_root}\\Animations\\{}.hkx", selection.name);
        let bytes = fs::read(source_mesh_path(&game_root, &selection.candidate.source_kf))
            .map_err(|error| error.to_string())?;
        let event_map = HashMap::new();
        let mut request = stage_request_from_motion_candidate(
            &selection.candidate,
            &family.rig.skeleton_path,
            &skeleton,
            &path,
            &event_map,
            None,
        )
        .map_err(|error| error.to_string())?;
        configure_attack_request(family, selection, &mut request)?;
        let staged = crate::phase::animations::stage_fnv_creature_kf(&bytes, request)
            .map_err(|error| format!("{}: {error}", selection.candidate.source_kf))?;
        clip_rows.push(serde_json::json!({"source":selection.candidate.source_kf,"role":selection.role,"looping":selection.target_looping,"events":staged.receipt.events.iter().map(|event| serde_json::json!({"text":event.text,"time":event.time})).collect::<Vec<_>>() }));
        policies.insert(
            selection.name.clone(),
            staged_clip_motion_policy(&staged.receipt.motion),
        );
        clips.push(ClipDecl {
            name: selection.name.clone(),
            path,
            binding: staged.receipt.binding,
            looping: selection.target_looping,
        });
    }
    for overlay in &selected.overlays {
        let path = format!("{runtime_root}\\Animations\\{}.hkx", overlay.clip_name);
        let bytes = fs::read(source_mesh_path(&game_root, &overlay.candidate.source_kf))
            .map_err(|error| error.to_string())?;
        let event_map = HashMap::new();
        let request = stage_request_from_motion_candidate(
            &overlay.candidate,
            &family.rig.skeleton_path,
            &skeleton,
            &path,
            &event_map,
            None,
        )
        .map_err(|error| error.to_string())?;
        let staged = crate::phase::animations::stage_fnv_creature_kf(&bytes, request)
            .map_err(|error| error.to_string())?;
        policies.insert(
            overlay.clip_name.clone(),
            staged_clip_motion_policy(&staged.receipt.motion),
        );
        clips.push(ClipDecl {
            name: overlay.clip_name.clone(),
            path,
            binding: staged.receipt.binding,
            looping: overlay.candidate.looping,
        });
    }
    let graph = build_capability_graph(&selected, Some(&policies))?;
    let motion = family
        .motion_evidence
        .as_ref()
        .ok_or("source family has no classified motion")?;
    for (source_role, target_role, direction) in [
        (MotionRole::Turn, CreatureClipRole::TurnLeft, "left"),
        (MotionRole::Turn, CreatureClipRole::TurnRight, "right"),
        (MotionRole::Hurt, CreatureClipRole::Hurt, ""),
        (MotionRole::Death, CreatureClipRole::Death, ""),
    ] {
        let available = motion.role(source_role).is_some_and(|entry| {
            entry.candidates.iter().any(|candidate| {
                candidate.looping == clip_role_expects_looping(target_role)
                    && candidate
                        .sequence_name
                        .to_ascii_lowercase()
                        .contains(direction)
                    && !leaf(candidate).contains("swim")
            })
        });
        if available && !graph.roles.iter().any(|role| role.role == target_role) {
            return Err(format!("source-backed {target_role:?} action was omitted"));
        }
    }
    let source_idles =
        super::super::super::creature_source_idles::selected_source_idle_routes(family);
    for (source_idle, _) in &source_idles {
        let event = super::super::super::creature_source_idles::source_idle_event(
            family.rig.game,
            source_idle,
        );
        if !graph
            .roles
            .iter()
            .any(|role| role.trigger_event.as_ref() == Some(&event))
        {
            return Err(format!(
                "source IDLE {source_idle} has no graph event route"
            ));
        }
    }
    let ragdoll = RagdollDisposition::NoRagdoll {
        reason: NoRagdollReason::SourceHasNoRagdoll,
    };
    let mut declarations = graph_declarations(&graph, &ragdoll);
    if graph.template == CreatureGraphTemplate::StationaryTurret {
        add_stationary_aim_declarations(&mut declarations);
    }
    let capsule = family
        .race_data
        .as_ref()
        .ok_or("source family has no controller measurements")?
        .measured_evidence
        .capsule
        .as_ref()
        .ok_or("source family has no source controller capsule")?;
    let rig = CreatureManifest {
        property_sequences: Vec::new(),
        creature_name,
        visual_skeleton_nif: format!("{runtime_root}\\CharacterAssets\\Skeleton.nif"),
        animation_skeleton: SkeletonDecl {
            path: skeleton_path,
            runtime_name: skeleton.skeleton_name,
            bones: skeleton
                .ordered_bone_names
                .into_iter()
                .zip(skeleton.parent_indices)
                .map(|(name, parent)| BoneDecl {
                    name,
                    parent_index: usize::try_from(parent).ok(),
                })
                .collect(),
            float_slots: skeleton.float_slot_names,
        },
        controller: CreatureControllerDecl {
            collision_filter_info: 1,
            rigid_body_type: 255,
            model_up_ms: [0.0, 0.0, 1.0, 0.0],
            model_forward_ms: [1.0, 0.0, 0.0, 0.0],
            model_right_ms: [0.0, -1.0, 0.0, 0.0],
            model_scale: 1.0,
        },
        ragdoll,
        clips,
        idle_clip: selected
            .clips
            .iter()
            .find(|clip| is_primary_idle(selected.template, clip.role))
            .ok_or("source family has no idle")?
            .name
            .clone(),
        capsule: Capsule {
            height: capsule.total_height,
            radius: capsule.radius,
        },
        paths: ScaffoldPaths {
            project: format!("{runtime_root}\\FnvFo3Project.hkx"),
            character: format!("{runtime_root}\\Characters\\FnvFo3Character.hkx"),
            root_behavior: format!("{runtime_root}\\Behaviors\\FnvFo3RootBehavior.hkx"),
            core_behavior: format!("{runtime_root}\\Behaviors\\FnvFo3CoreBehavior.hkx"),
        },
        root: declarations.clone(),
        core: declarations,
    };
    let packed = crate::source_rig::pack_capability_scaffold(&rig, &graph, output.join(&family_id))
        .map_err(|error| error.to_string())?;
    Ok(
        serde_json::json!({"family":family_id,"states":graph.roles.len(),"graph_roles":graph.roles,"clips":clip_rows,"packed_artifacts":packed.artifacts.len(),"source_idles":source_idles}),
    )
}
