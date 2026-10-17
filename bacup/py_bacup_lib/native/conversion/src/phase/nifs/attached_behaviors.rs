use super::*;
use std::collections::BTreeMap;

pub(super) struct AttachedBehaviorPlan {
    graph_paths: Vec<String>,
    assets: BTreeMap<PathBuf, Vec<u8>>,
}

impl AttachedBehaviorPlan {
    pub(super) fn destinations(&self) -> impl Iterator<Item = &PathBuf> {
        self.assets.keys()
    }

    pub(super) fn stage(&self, staged: &mut StagedNifDestination) -> Result<(), String> {
        let mut nif = NifFile::load(&staged.path).map_err(|error| error.to_string())?;
        let blocks = nif.blocks.iter_mut().filter(|block| {
            block.type_name == "BSBehaviorGraphExtraData"
                && matches!(block.get_field("Behaviour Graph File"), Some(NifValue::String(path)) if !path.is_empty())
        }).collect::<Vec<_>>();
        if blocks.len() != self.graph_paths.len() {
            return Err("converted NIF changed its attached behavior project count".into());
        }
        for (block, path) in blocks.into_iter().zip(&self.graph_paths) {
            block.set_field("Behaviour Graph File", NifValue::String(path.clone()));
        }
        nif.save(Some(staged.path.clone()))
            .map_err(|error| error.to_string())?;
        for (index, (destination, bytes)) in self.assets.iter().enumerate() {
            let path = staged
                .directory
                .path()
                .join(format!("behavior_{index}.hkx"));
            std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
            staged.attached_behaviors.push(ArtifactPublication {
                staged: path,
                destination: destination.clone(),
            });
        }
        Ok(())
    }
}

pub(super) fn prepare(
    source: &Path,
    destination: &Path,
    mod_path: &Path,
) -> Option<Result<AttachedBehaviorPlan, String>> {
    let mut nif = NifFile::load(source).ok()?;
    if !nif.blocks.iter().any(|block| block.type_name == "BSBehaviorGraphExtraData"
        && matches!(block.get_field("Behaviour Graph File"), Some(NifValue::String(path)) if !path.is_empty())) {
        return None;
    }
    Some((|| {
        let meshes = source
            .ancestors()
            .find(|path| {
                path.file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("meshes"))
            })
            .ok_or("attached behavior source has no meshes root")?;
        let source_root = meshes
            .parent()
            .ok_or("attached behavior source has no data root")?;
        let identity = windows_destination_key(
            destination
                .strip_prefix(mod_path.join("data"))
                .map_err(|_| "attached behavior destination is outside mod data")?,
        );
        let hash = blake3::hash(identity.as_bytes()).to_hex();
        let namespace = format!("b21_attached/{}", &hash[..16]);
        let assets = nif_core_native::creature_closure::stage_attached_behavior_closure(
            &mut nif,
            source_root,
            &namespace,
        )?;
        let graph_paths = nif
            .blocks
            .iter()
            .filter_map(|block| {
                if block.type_name != "BSBehaviorGraphExtraData" {
                    return None;
                }
                match block.get_field("Behaviour Graph File") {
                    Some(NifValue::String(path)) if !path.is_empty() => Some(path.clone()),
                    _ => None,
                }
            })
            .collect();
        Ok(AttachedBehaviorPlan {
            graph_paths,
            assets: assets
                .into_iter()
                .map(|(relative, bytes)| (mod_path.join("data").join(relative), bytes))
                .collect(),
        })
    })())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attached_behavior_publication_requires_reservation_and_commits_with_the_nif() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("data/meshes/bow.nif");
        let companion = directory.path().join("data/meshes/b21_attached/bow.hkx");
        let mut staged = StagedNifDestination::new(&destination).unwrap();
        std::fs::write(&staged.path, b"new nif").unwrap();
        let hkx = staged.directory.path().join("behavior_0.hkx");
        std::fs::write(&hkx, b"new graph").unwrap();
        staged.attached_behaviors.push(ArtifactPublication {
            staged: hkx,
            destination: companion.clone(),
        });
        let context = EntryArtifactContext {
            owner: 0,
            material_relative: None,
            cleanup_first_person: false,
            attached_behaviors: None,
        };
        let mut ownership = DestinationOwnership::default();
        ownership
            .owners
            .insert(windows_destination_key(&destination), 0);
        assert!(
            build_artifact_plan(
                &ConvertFileReport::default(),
                &staged,
                &destination,
                None,
                &context,
                &ownership
            )
            .is_err()
        );
        ownership
            .owners
            .insert(windows_destination_key(&companion), 0);
        let plan = build_artifact_plan(
            &ConvertFileReport::default(),
            &staged,
            &destination,
            None,
            &context,
            &ownership,
        )
        .unwrap();
        assert_eq!(plan.publications.len(), 2);
        assert_eq!(
            plan.behavior_destinations,
            vec![companion.to_string_lossy().into_owned()]
        );
        let result = publish_staged_result(
            NifConversionResult::Written(ConvertFileReport {
                supported: true,
                ..Default::default()
            }),
            &mut staged,
            &destination,
            None,
            &context,
            &ownership,
            &Mutex::new(()),
        );
        let NifConversionResult::Written(report) = result else {
            panic!("publication failed")
        };
        assert_eq!(std::fs::read(&destination).unwrap(), b"new nif");
        assert_eq!(std::fs::read(&companion).unwrap(), b"new graph");
        assert_eq!(report.emitted_behaviors.len(), 1);
    }

    #[test]
    fn attached_behavior_publication_rolls_back_when_primary_nif_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let nif = directory.path().join("bow.nif");
        let hkx = directory.path().join("bow.hkx");
        let staged_hkx = directory.path().join("new.hkx");
        std::fs::write(&nif, b"old nif").unwrap();
        std::fs::write(&hkx, b"old graph").unwrap();
        std::fs::write(&staged_hkx, b"new graph").unwrap();
        let publications = vec![
            ArtifactPublication {
                staged: staged_hkx,
                destination: hkx.clone(),
            },
            ArtifactPublication {
                staged: directory.path().join("missing.nif"),
                destination: nif.clone(),
            },
        ];
        assert!(
            publish_artifact_transaction(&publications, &[], &directory.path().join("backups"))
                .is_err()
        );
        assert_eq!(std::fs::read(nif).unwrap(), b"old nif");
        assert_eq!(std::fs::read(hkx).unwrap(), b"old graph");
    }

    #[test]
    #[ignore = "requires extracted Skyrim bow and crossbow meshes and own HKX projects"]
    fn skyrim_bow_crossbow_attached_closures_publish_complete_own_rigs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let output = tempfile::tempdir().unwrap();
        for relative in [
            "weapons/steel/steelbow.nif",
            "dlc01/weapons/crossbow/crossbow.nif",
            "dlc01/weapons/dwarven crossbow/dwarvencrossbow.nif",
            "dlc01/weapons/auriel/aurielsbow.nif",
            "dlc01/weapons/dragonbone/bow.nif",
            "dlc02/weapons/dwarven/dwarvenbowkarztev01.nif",
            "dlc02/weapons/nordic/nordicbow.nif",
            "dlc02/weapons/stahlrim/stahlrimbow.nif",
            "weapons/boundweapons/boundbow.nif",
            "weapons/daedric/daedricbowskinned.nif",
            "weapons/draugr/draugrbowskinned.nif",
            "weapons/dwarven/dwarvenbowskinned.nif",
            "weapons/ebony/ebonybowskinned.nif",
            "weapons/elven/elvenbowskinned.nif",
            "weapons/falmer/falmerbow.nif",
            "weapons/glass/glassbowskinned.nif",
            "weapons/iron/ironbow.nif",
            "weapons/nightingale/nightingalebow.nif",
            "weapons/nordhero/nordherobow.nif",
            "weapons/orcish/orcishbow.nif",
            "weapons/wooden/woodenbow.nif",
        ] {
            let source = root.join("extracted/skyrimse/meshes").join(relative);
            let destination = output.path().join("data/meshes").join(relative);
            let plan = prepare(&source, &destination, output.path())
                .unwrap()
                .unwrap_or_else(|error| panic!("{relative}: {error}"));
            assert!(
                plan.assets.len() >= 6,
                "{relative}: incomplete attached project"
            );
            let mut has_skeleton = false;
            let mut clips = 0;
            let mut skeleton = None;
            let mut animations = Vec::new();
            for (path, bytes) in &plan.assets {
                let hkx = havok_native::hkx::HkxFile::read(bytes).unwrap();
                assert_eq!(hkx.contents_version(), "hk_2014.1.0-r1");
                for character in hkx
                    .objects()
                    .iter()
                    .filter(|object| object.class_name == "hkbCharacterStringData")
                {
                    for member in character
                        .members
                        .iter()
                        .filter(|member| member.name.starts_with("luaScriptOn"))
                    {
                        assert!(
                            matches!(&member.value, havok_native::hkx::types::HkxValue::String { value, is_null } if *is_null || value.is_empty()),
                            "{relative}: fabricated Lua hook {}",
                            member.name
                        );
                    }
                }
                has_skeleton |= hkx
                    .objects()
                    .iter()
                    .any(|object| object.class_name == "hkaSkeleton");
                clips += usize::from(
                    hkx.objects()
                        .iter()
                        .any(|object| object.class_name == "hkaSplineCompressedAnimation"),
                );
                if hkx
                    .objects()
                    .iter()
                    .any(|object| object.class_name == "hkaSkeleton")
                {
                    skeleton = Some(
                        havok_native::animation::parsers::parse_skeleton_xml(
                            &havok_native::api::havok_hkx_to_xml(bytes).unwrap(),
                        )
                        .unwrap(),
                    );
                }
                if hkx
                    .objects()
                    .iter()
                    .any(|object| object.class_name == "hkaSplineCompressedAnimation")
                {
                    animations.push(
                        havok_native::animation::clip::extract_clip(
                            &havok_native::api::havok_hkx_to_xml(bytes).unwrap(),
                            None,
                        )
                        .unwrap(),
                    );
                }
                assert!(path.starts_with(output.path().join("data/meshes/b21_attached")));
            }
            assert!(
                has_skeleton && clips >= 2,
                "{relative}: missing own rig or draw/release clips"
            );
            let mut staged = StagedNifDestination::new(&destination).unwrap();
            let report = convert_nif_file(
                &source,
                &staged.path,
                "skyrimse",
                "fo4",
                Some(&staged.materials_root),
                &ConvertFileOptions::default(),
            )
            .unwrap();
            assert!(
                report.supported && report.errors.is_empty(),
                "{relative}: {report:?}"
            );
            plan.stage(&mut staged).unwrap();
            let reread = NifFile::load(&staged.path).unwrap();
            let skeleton = skeleton.unwrap();
            for bone in &skeleton.bone_names {
                assert!(
                    reread
                        .blocks
                        .iter()
                        .any(|block| block.get_field("Name")
                            == Some(&NifValue::String(bone.clone()))),
                    "{relative}: own animation bone {bone} missing from NIF"
                );
            }
            for animation in animations {
                assert_eq!(
                    animation.original_skeleton_name.as_deref(),
                    Some(skeleton.name.as_str())
                );
                assert!(
                    animation
                        .track_to_bone_indices
                        .iter()
                        .all(|index| (*index as usize) < skeleton.bone_names.len())
                );
            }
            let attachment = if relative.contains("crossbow") {
                "Weapon"
            } else {
                "WeaponLeft"
            };
            assert!(reread.blocks.iter().any(|block| matches!(block.get_field("String Data"), Some(NifValue::String(value)) if value == attachment)));
            for path in &plan.graph_paths {
                assert!(
                    reread
                        .blocks
                        .iter()
                        .any(|block| block.get_field("Behaviour Graph File")
                            == Some(&NifValue::String(path.clone())))
                );
                let relative = PathBuf::from(path.replace('\\', "/"));
                assert!(
                    plan.assets
                        .contains_key(&output.path().join("data/meshes").join(relative))
                );
            }
            assert_eq!(staged.attached_behaviors.len(), plan.assets.len());
            eprintln!(
                "{relative}: {} attached HKX assets, {clips} own-rig animation clips",
                plan.assets.len()
            );
        }
    }
}
