//! Remove ragdoll contact-list indices that cannot resolve in the target skeleton.

use std::path::{Path, PathBuf};

use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxFile, HkxObject, read_packfile};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

pub struct SanitizeRagdollContactBonesFixup;

impl Fixup for SanitizeRagdollContactBonesFixup {
    fn name(&self) -> &'static str {
        "sanitize_ragdoll_contact_bones"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.havok
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.mod_path.is_some()
    }

    fn run_with_session(
        &self,
        _session: &mut PluginSession,
        _mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let Some(mod_path) = config.mod_path.as_deref() else {
            return Ok(FixupReport::empty());
        };
        sanitize_ragdoll_contact_bones_in_mod_path(mod_path)
    }
}

pub fn sanitize_ragdoll_contact_bones_in_mod_path(
    mod_path: &Path,
) -> Result<FixupReport, FixupError> {
    let mut files_changed = 0u32;

    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_skeleton_files(&meshes_root, &mut |skeleton_path| {
            let Ok(Some(bone_count)) = ragdoll_bone_count_from_file(skeleton_path) else {
                return;
            };
            let Some(actor_root) = skeleton_path.parent().and_then(Path::parent) else {
                return;
            };
            let Some(behaviors_dir) = find_child_dir(actor_root, "behaviors") else {
                return;
            };

            walk_hkx_files(&behaviors_dir, &mut |behavior_path| {
                if sanitize_behavior_file(behavior_path, bone_count)
                    .is_ok_and(|removed| removed > 0)
                {
                    files_changed += 1;
                }
            });
        });
    }

    Ok(FixupReport {
        records_changed: files_changed,
        ..FixupReport::empty()
    })
}

fn sanitize_behavior_file(
    behavior_path: &Path,
    ragdoll_bone_count: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    let data = std::fs::read(behavior_path)?;
    let mut hkx = read_packfile(&data)?;
    let removed = sanitize_contact_bone_arrays(&mut hkx, ragdoll_bone_count);
    if removed > 0 {
        std::fs::write(behavior_path, hkx.save())?;
    }
    Ok(removed)
}

fn ragdoll_bone_count_from_file(
    skeleton_path: &Path,
) -> Result<Option<usize>, Box<dyn std::error::Error>> {
    let data = std::fs::read(skeleton_path)?;
    let hkx = read_packfile(&data)?;
    Ok(ragdoll_bone_count(&hkx))
}

fn ragdoll_bone_count(hkx: &HkxFile) -> Option<usize> {
    hkx.objects()
        .iter()
        .filter(|object| object.class_name == "hknpRagdollData")
        .filter_map(|object| {
            array_member_len(object, "boneToBodyMap")
                .or_else(|| array_member_len(object, "bodyCinfos"))
        })
        .filter(|count| *count > 0)
        .min()
}

fn array_member_len(object: &HkxObject, name: &str) -> Option<usize> {
    object
        .members
        .iter()
        .find(|member| member.name == name)
        .and_then(|member| match &member.value {
            HkxValue::Array(values) => Some(values.len()),
            _ => None,
        })
}

fn sanitize_contact_bone_arrays(hkx: &mut HkxFile, ragdoll_bone_count: usize) -> usize {
    let mut bone_array_indices: Vec<usize> = hkx
        .objects()
        .iter()
        .filter(|object| object.class_name == "BSRagdollContactListenerModifier")
        .filter_map(|object| {
            object
                .members
                .iter()
                .find(|member| member.name == "bones")
                .and_then(|member| match &member.value {
                    HkxValue::Pointer(Some(index)) => Some(*index),
                    _ => None,
                })
        })
        .collect();
    bone_array_indices.sort_unstable();
    bone_array_indices.dedup();

    let mut removed = 0usize;
    for object_index in bone_array_indices {
        let Some(object) = hkx.objects_mut().get_mut(object_index) else {
            continue;
        };
        if object.class_name != "hkbBoneIndexArray" {
            continue;
        }
        let Some(member) = object
            .members
            .iter_mut()
            .find(|member| member.name == "boneIndices")
        else {
            continue;
        };
        let HkxValue::Array(values) = &mut member.value else {
            continue;
        };

        let before = values.len();
        values.retain(|value| bone_index_is_valid(value, ragdoll_bone_count));
        removed += before - values.len();
    }
    removed
}

fn bone_index_is_valid(value: &HkxValue, ragdoll_bone_count: usize) -> bool {
    let index = match value {
        HkxValue::I8(value) => i64::from(*value),
        HkxValue::U8(value) => i64::from(*value),
        HkxValue::I16(value) => i64::from(*value),
        HkxValue::U16(value) => i64::from(*value),
        HkxValue::I32(value) => i64::from(*value),
        HkxValue::U32(value) => i64::from(*value),
        HkxValue::I64(value) => *value,
        HkxValue::U64(value) => return *value < ragdoll_bone_count as u64,
        _ => return true,
    };
    index >= 0 && (index as usize) < ragdoll_bone_count
}

fn mesh_roots_for_mod_path(mod_path: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let data_meshes = mod_path.join("data").join("Meshes");
    if data_meshes.is_dir() {
        roots.push(data_meshes);
    }
    let legacy_meshes = mod_path.join("meshes");
    if legacy_meshes.is_dir() {
        roots.push(legacy_meshes);
    }
    roots
}

fn walk_skeleton_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_skeleton_files(&path, visit);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("skeleton.hkx"))
            && path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("characterassets"))
        {
            visit(&path);
        }
    }
}

fn walk_hkx_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_hkx_files(&path, visit);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("hkx"))
        {
            visit(&path);
        }
    }
}

fn find_child_dir(parent: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(parent).ok()?.flatten().find_map(|entry| {
        let path = entry.path();
        (path.is_dir()
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(name)))
        .then_some(path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use havok_native::hkx::{HkxMember, HkxObject};

    fn object(class_name: &str, members: Vec<HkxMember>) -> HkxObject {
        HkxObject {
            name: None,
            offset: 0,
            signature: 0,
            class_name: class_name.to_string(),
            members,
        }
    }

    fn member(name: &str, value: HkxValue) -> HkxMember {
        HkxMember {
            name: name.to_string(),
            value,
        }
    }

    fn behavior_with_contact_bones(indices: &[i16]) -> HkxFile {
        HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![
                object(
                    "BSRagdollContactListenerModifier",
                    vec![member("bones", HkxValue::Pointer(Some(1)))],
                ),
                object(
                    "hkbBoneIndexArray",
                    vec![member(
                        "boneIndices",
                        HkxValue::Array(indices.iter().copied().map(HkxValue::I16).collect()),
                    )],
                ),
            ],
        )
    }

    fn contact_bones(hkx: &HkxFile) -> Vec<i16> {
        let member = hkx.objects()[1]
            .members
            .iter()
            .find(|member| member.name == "boneIndices")
            .unwrap();
        let HkxValue::Array(values) = &member.value else {
            panic!("boneIndices is not an array");
        };
        values
            .iter()
            .map(|value| match value {
                HkxValue::I16(value) => *value,
                _ => panic!("bone index is not i16"),
            })
            .collect()
    }

    #[test]
    fn frog_contact_index_outside_seventeen_bone_ragdoll_is_removed() {
        let mut behavior = behavior_with_contact_bones(&[0, 1, 2, 3, 9, 10, 18, 12, 13, 6, 14]);

        assert_eq!(sanitize_contact_bone_arrays(&mut behavior, 17), 1);
        assert_eq!(
            contact_bones(&behavior),
            vec![0, 1, 2, 3, 9, 10, 12, 13, 6, 14]
        );
    }

    #[test]
    fn wendigo_contact_indices_within_nineteen_bone_ragdoll_are_unchanged() {
        let mut behavior = behavior_with_contact_bones(&[0, 1, 2, 5, 8, 12, 15]);

        assert_eq!(sanitize_contact_bone_arrays(&mut behavior, 19), 0);
        assert_eq!(contact_bones(&behavior), vec![0, 1, 2, 5, 8, 12, 15]);
    }

    #[test]
    fn ragdoll_count_uses_bone_to_body_map() {
        let skeleton = HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![object(
                "hknpRagdollData",
                vec![member(
                    "boneToBodyMap",
                    HkxValue::Array((0..17).map(HkxValue::I32).collect()),
                )],
            )],
        );

        assert_eq!(ragdoll_bone_count(&skeleton), Some(17));
    }

    #[test]
    fn mod_tree_sanitizer_rewrites_the_frog_behavior() {
        let temp = tempfile::tempdir().unwrap();
        let actor_root = temp
            .path()
            .join("data")
            .join("Meshes")
            .join("Actors")
            .join("Frog");
        let character_assets = actor_root.join("CharacterAssets");
        let behaviors = actor_root.join("Behaviors");
        std::fs::create_dir_all(&character_assets).unwrap();
        std::fs::create_dir_all(&behaviors).unwrap();

        let skeleton = HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![object(
                "hknpRagdollData",
                vec![member(
                    "boneToBodyMap",
                    HkxValue::Array((0..17).map(HkxValue::I32).collect()),
                )],
            )],
        );
        std::fs::write(character_assets.join("skeleton.hkx"), skeleton.save()).unwrap();

        let behavior_path = behaviors.join("FrogRootBehavior.hkx");
        let behavior = behavior_with_contact_bones(&[0, 1, 2, 3, 9, 10, 18, 12, 13, 6, 14]);
        std::fs::write(&behavior_path, behavior.save()).unwrap();

        let report = sanitize_ragdoll_contact_bones_in_mod_path(temp.path()).unwrap();
        let rewritten = read_packfile(&std::fs::read(behavior_path).unwrap()).unwrap();

        assert_eq!(report.records_changed, 1);
        assert_eq!(
            contact_bones(&rewritten),
            vec![0, 1, 2, 3, 9, 10, 12, 13, 6, 14]
        );
    }
}
