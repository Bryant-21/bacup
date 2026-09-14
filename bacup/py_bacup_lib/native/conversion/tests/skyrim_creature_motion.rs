use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use conversion_native::skyrimse_fo4_runtime::creature_motion::{
    SkyrimClipDisposition, SkyrimControllerEvidenceDisposition, SkyrimControllerLayoutEvidence,
    SkyrimCreatureMotionRole, SkyrimLivingFamilyDisposition, SkyrimLivingTemplateDisposition,
    SkyrimRequiredRoleDisposition, SkyrimRootMotion, load_extracted_skyrim_creature_motion_catalog,
};

#[test]
fn exact_extracted_44_family_motion_catalog_is_terminal_and_source_evidenced() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let actors = repo.join("extracted/skyrimse/meshes/actors");
    let animation_data = repo.join("extracted/skyrimse/meshes/animationdata");
    if !actors.exists() {
        return;
    }

    let catalog =
        load_extracted_skyrim_creature_motion_catalog(&actors, &animation_data, &[]).unwrap();
    eprintln!(
        "Skyrim creature motion accounting: {:?}",
        catalog.accounting
    );

    assert_eq!(catalog.accounting.families, 44);
    assert_eq!(catalog.accounting.unique_clips, 2_327);
    assert_eq!(catalog.accounting.paired_clips, 93);
    assert_eq!(catalog.accounting.havok_root_motion_clips, 13);
    assert!(catalog.accounting.bound_root_motion_clips > 0);
    assert!(catalog.accounting.evidence_events > 0);
    assert_eq!(
        catalog.accounting.role_clips
            + catalog.accounting.paired_clips
            + catalog.accounting.overlay_clips
            + catalog.accounting.unsupported_clips
            + catalog.accounting.ambiguous_clips,
        2_327
    );
    assert!(catalog.clips.iter().all(|clip| !clip.family_ids.is_empty()));
    assert_eq!(
        catalog.accounting.havok_root_motion_clips
            + catalog.accounting.bound_root_motion_clips
            + catalog.accounting.unknown_root_motion_clips,
        2_327
    );
    assert!(catalog.clips.iter().any(|clip| {
        matches!(
            clip.disposition,
            SkyrimClipDisposition::Role {
                role: SkyrimCreatureMotionRole::MeleeAttack,
                ..
            }
        )
    }));

    assert_eq!(catalog.inventories.len(), 44);
    assert!(catalog.inventories.iter().all(|inventory| {
        inventory.project_path.starts_with("Actors\\")
            && !inventory.character_paths.is_empty()
            && !inventory.animation_skeleton_paths.is_empty()
            && !inventory.behavior_paths.is_empty()
            && !inventory.controllers.is_empty()
    }));
    let controller_count = catalog
        .inventories
        .iter()
        .map(|inventory| inventory.controllers.len())
        .sum::<usize>();
    let character_count = catalog
        .inventories
        .iter()
        .map(|inventory| inventory.character_paths.len())
        .sum::<usize>();
    let skeleton_count = catalog
        .inventories
        .iter()
        .flat_map(|inventory| &inventory.animation_skeleton_paths)
        .map(|path| path.to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .len();
    let behavior_count = catalog
        .inventories
        .iter()
        .flat_map(|inventory| &inventory.behavior_paths)
        .map(|path| path.to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .len();
    let complete_controllers = catalog
        .inventories
        .iter()
        .flat_map(|inventory| &inventory.controllers)
        .filter(|controller| {
            controller.disposition == SkyrimControllerEvidenceDisposition::Complete
        })
        .count();
    let controllers = catalog
        .inventories
        .iter()
        .flat_map(|inventory| &inventory.controllers)
        .collect::<Vec<_>>();
    let disposition_counts = controllers.iter().fold(
        BTreeMap::<&'static str, usize>::new(),
        |mut counts, controller| {
            let key = match controller.disposition {
                SkyrimControllerEvidenceDisposition::Complete => "complete",
                SkyrimControllerEvidenceDisposition::MissingControllerSetup => {
                    "missing_controller_setup"
                }
                SkyrimControllerEvidenceDisposition::MissingRigidBodySetup => {
                    "missing_rigid_body_setup"
                }
                SkyrimControllerEvidenceDisposition::MissingShapeSetup => "missing_shape_setup",
                SkyrimControllerEvidenceDisposition::InvalidCapsuleDimensions { .. } => {
                    "invalid_capsule_dimensions"
                }
                SkyrimControllerEvidenceDisposition::LegacyLayoutUnsupported { .. } => {
                    "legacy_layout_unsupported"
                }
                SkyrimControllerEvidenceDisposition::InvalidModelTransform => {
                    "invalid_model_transform"
                }
            };
            *counts.entry(key).or_default() += 1;
            counts
        },
    );
    assert_eq!(controller_count, 44);
    assert_eq!(character_count, 44);
    assert_eq!(disposition_counts.values().sum::<usize>(), controller_count);
    assert_eq!(
        complete_controllers, controller_count,
        "{disposition_counts:?}"
    );
    assert!(controllers.iter().all(|controller| {
        let capsule = controller.capsule.as_ref().expect("complete capsule");
        let model = controller.model.as_ref().expect("complete model transform");
        controller.contents_version == "hk_2010.2.0-r1"
            && controller.character_data_signature == 0x300d_6808
            && controller.controller_class.as_deref()
                == Some("hkbCharacterDataCharacterControllerInfo")
            && controller.controller_signature == Some(0xa0f4_15bf)
            && matches!(
                &controller.layout,
                Some(
                    SkyrimControllerLayoutEvidence::LegacyCharacterControllerInfo {
                        character_data_signature: 0x300d_6808,
                        controller_signature: 0xa0f4_15bf,
                        ..
                    }
                )
            )
            && controller.controller_cinfo_class.is_none()
            && controller.architecture.is_none()
            && controller.rigid_body_type.is_none()
            && controller.shape_type.is_none()
            && capsule.total_height.is_finite()
            && capsule.total_height > 0.0
            && capsule.total_height.to_bits() != 1
            && capsule.radius.is_finite()
            && capsule.radius > 0.0
            && capsule.radius.to_bits() != 0
            && model.scale.is_finite()
            && model.scale > 0.0
            && [model.up_ms, model.forward_ms, model.right_ms]
                .into_iter()
                .flatten()
                .all(f32::is_finite)
            && controller
                .legacy_controller_recipe_evidence()
                .is_some_and(|recipe| recipe.controller_cinfo_class.is_none())
    }));
    let wolf = catalog.inventory("wolf").unwrap();
    assert_eq!(wolf.project_path, "Actors\\canine\\wolfproject.hkx");
    assert_eq!(wolf.character_paths.len(), 1);
    assert!(
        wolf.character_paths[0].eq_ignore_ascii_case("Actors\\canine\\Characters Wolf\\Wolf.hkx")
    );
    assert!(wolf.animation_skeleton_paths.iter().any(|path| {
        path.eq_ignore_ascii_case("Actors\\canine\\Character Assets Wolf\\skeleton.HKX")
    }));
    eprintln!(
        "Skyrim controller inventory: characters={character_count}; unique_skeletons={skeleton_count}; unique_behaviors={behavior_count}; controllers={controller_count}; dispositions={disposition_counts:?}"
    );

    let living = catalog.living_family_evidence();
    let ready = living
        .iter()
        .filter(|family| family.disposition == SkyrimLivingFamilyDisposition::Ready)
        .map(|family| family.family_id.as_str())
        .collect::<Vec<_>>();
    let template_counts = living.iter().fold(
        BTreeMap::<&'static str, usize>::new(),
        |mut counts, family| {
            let key = match family.template {
                SkyrimLivingTemplateDisposition::Proven { .. } => "proven",
                SkyrimLivingTemplateDisposition::Ambiguous { .. } => "ambiguous",
                SkyrimLivingTemplateDisposition::Unsupported { .. } => "unsupported",
            };
            *counts.entry(key).or_default() += 1;
            counts
        },
    );
    let role_counts = living
        .iter()
        .flat_map(|family| &family.required_roles)
        .fold(
            BTreeMap::<&'static str, usize>::new(),
            |mut counts, role| {
                let key = match role {
                    SkyrimRequiredRoleDisposition::Ready { .. } => "ready",
                    SkyrimRequiredRoleDisposition::MissingRole { .. } => "missing_role",
                    SkyrimRequiredRoleDisposition::AmbiguousRole { .. } => "ambiguous_role",
                    SkyrimRequiredRoleDisposition::MissingTrigger { .. } => "missing_trigger",
                    SkyrimRequiredRoleDisposition::AmbiguousTrigger { .. } => "ambiguous_trigger",
                    SkyrimRequiredRoleDisposition::UnknownRootMotion { .. } => {
                        "unknown_root_motion"
                    }
                    SkyrimRequiredRoleDisposition::UnsupportedRootMotion { .. } => {
                        "unsupported_root_motion"
                    }
                };
                *counts.entry(key).or_default() += 1;
                counts
            },
        );
    eprintln!(
        "Skyrim living evidence: ready={ready:?}; templates={template_counts:?}; required_roles={role_counts:?}"
    );
    assert_eq!(living.len(), 44);
    assert_eq!(template_counts.values().sum::<usize>(), 44);
    assert!(living.iter().all(|family| {
        let terminal_roles = family.required_roles.iter().filter(|role| {
            matches!(
                role,
                SkyrimRequiredRoleDisposition::Ready { .. }
                    | SkyrimRequiredRoleDisposition::MissingRole { .. }
                    | SkyrimRequiredRoleDisposition::AmbiguousRole { .. }
                    | SkyrimRequiredRoleDisposition::MissingTrigger { .. }
                    | SkyrimRequiredRoleDisposition::AmbiguousTrigger { .. }
                    | SkyrimRequiredRoleDisposition::UnknownRootMotion { .. }
                    | SkyrimRequiredRoleDisposition::UnsupportedRootMotion { .. }
            )
        });
        terminal_roles.count() == family.required_roles.len()
    }));
    assert!(
        living
            .iter()
            .flat_map(|family| &family.required_roles)
            .all(|role| {
                let SkyrimRequiredRoleDisposition::Ready { selected, .. } = role else {
                    return true;
                };
                selected.triggers.len() == 1
                    && selected.root_motion_locator.is_some()
                    && matches!(
                        selected.root_motion,
                        SkyrimRootMotion::Stationary { .. } | SkyrimRootMotion::Sampled { .. }
                    )
                    && !selected.role_locators.is_empty()
            })
    );
    assert!(living.iter().all(|family| {
        (family.disposition == SkyrimLivingFamilyDisposition::Ready)
            == (!family.required_roles.is_empty()
                && family
                    .required_roles
                    .iter()
                    .all(|role| matches!(role, SkyrimRequiredRoleDisposition::Ready { .. })))
    }));
}
