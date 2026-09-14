//! Keep converted reload annotations compatible with FO4's WeaponBehavior.
//!
//! FO4's shared WeaponBehavior emits `reloadEnd` half a second before the
//! animation ends. Some short FO76 reload clips place `reloadComplete` after
//! that point, so the graph leaves the reload state before ammo is transferred.

use std::path::{Path, PathBuf};

use havok_native::hkx::read_packfile;
use havok_native::hkx::types::HkxValue;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

const RELOAD_END_OFFSET_FROM_CLIP_END: f32 = 0.5;
const RELOAD_COMPLETE_OFFSET_FROM_CLIP_END: f32 = 1.0;
const TIME_EPSILON: f32 = 0.0001;

pub struct RetimeReloadCompleteEventsFixup;

impl Fixup for RetimeReloadCompleteEventsFixup {
    fn name(&self) -> &'static str {
        "retime_reload_complete_events"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.animations
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
        retime_reload_complete_events_in_mod_path(mod_path)
    }
}

pub fn retime_reload_complete_events_in_mod_path(
    mod_path: &Path,
) -> Result<FixupReport, FixupError> {
    let mut files_patched = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_reload_hkx_files(
            &meshes_root,
            &mut |hkx_path| match process_reload_animation(hkx_path) {
                Ok(true) => files_patched += 1,
                Ok(false) | Err(_) => {}
            },
        );
    }
    Ok(FixupReport {
        records_changed: files_patched,
        ..FixupReport::empty()
    })
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

fn process_reload_animation(hkx_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(hkx_path)?;
    let mut hkx = read_packfile(&data)?;
    let mut modified = false;

    for object in hkx.objects_mut() {
        if object.class_name != "hkaSplineCompressedAnimation"
            && object.class_name != "hkaInterleavedUncompressedAnimation"
        {
            continue;
        }

        let Some(duration) = object.members.iter().find_map(|member| {
            if member.name != "duration" {
                return None;
            }
            match &member.value {
                HkxValue::F32(value) => Some(*value),
                _ => None,
            }
        }) else {
            continue;
        };

        for member in &mut object.members {
            if member.name != "annotationTracks" {
                continue;
            }
            let HkxValue::Array(tracks) = &mut member.value else {
                continue;
            };
            let Some(track_members) = tracks.first_mut().and_then(HkxValue::as_object_members_mut)
            else {
                continue;
            };
            let Some(HkxValue::Array(annotations)) = track_members
                .iter_mut()
                .find(|track_member| track_member.name == "annotations")
                .map(|track_member| &mut track_member.value)
            else {
                continue;
            };
            modified |= retime_reload_complete(annotations, duration);
        }
    }

    if modified {
        std::fs::write(hkx_path, hkx.save())?;
    }

    Ok(modified)
}

fn retime_reload_complete(annotations: &mut [HkxValue], duration: f32) -> bool {
    if !duration.is_finite() || duration <= 0.0 {
        return false;
    }

    let reload_end_time = (duration - RELOAD_END_OFFSET_FROM_CLIP_END).max(0.0);
    let target_time = (duration - RELOAD_COMPLETE_OFFSET_FROM_CLIP_END).max(0.0);
    let mut modified = false;

    for annotation in annotations {
        let Some(members) = annotation.as_object_members_mut() else {
            continue;
        };
        let is_reload_complete = members.iter().any(|member| {
            member.name == "text"
                && matches!(
                    &member.value,
                    HkxValue::String { value, .. }
                        if value.eq_ignore_ascii_case("reloadComplete")
                )
        });
        if !is_reload_complete {
            continue;
        }

        let Some(time_member) = members.iter_mut().find(|member| member.name == "time") else {
            continue;
        };
        let HkxValue::F32(time) = &mut time_member.value else {
            continue;
        };
        if *time + TIME_EPSILON >= reload_end_time && (*time - target_time).abs() > TIME_EPSILON {
            *time = target_time;
            modified = true;
        }
    }

    modified
}

fn walk_reload_hkx_files(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_reload_hkx_files(&path, visit);
            continue;
        }
        let is_reload = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("wpnreload.hkx"));
        if is_reload {
            visit(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use havok_native::hkx::HkxMember;
    use havok_native::hkx::descriptors::DescriptorRegistry;
    use havok_native::hkx::{HkxFile, HkxObject, write_hkx};

    use super::*;

    fn annotation(time: f32, text: &str) -> HkxValue {
        HkxValue::Object(vec![
            HkxMember {
                name: "time".to_string(),
                value: HkxValue::F32(time),
            },
            HkxMember {
                name: "text".to_string(),
                value: HkxValue::String {
                    value: text.to_string(),
                    is_null: false,
                },
            },
        ])
    }

    fn annotation_time(annotation: &HkxValue) -> f32 {
        annotation
            .as_object_members()
            .unwrap()
            .iter()
            .find_map(|member| match (&*member.name, &member.value) {
                ("time", HkxValue::F32(value)) => Some(*value),
                _ => None,
            })
            .unwrap()
    }

    fn write_reload_clip(path: &Path, duration: f32, reload_complete_time: f32) {
        let hkx = HkxFile::from_tagxml(
            11,
            "hk_2014.1.0-r1",
            vec![HkxObject {
                name: Some("#0001".to_string()),
                offset: 0,
                signature: 0,
                class_name: "hkaSplineCompressedAnimation".to_string(),
                members: vec![
                    HkxMember {
                        name: "duration".to_string(),
                        value: HkxValue::F32(duration),
                    },
                    HkxMember {
                        name: "annotationTracks".to_string(),
                        value: HkxValue::Array(vec![HkxValue::Object(vec![HkxMember {
                            name: "annotations".to_string(),
                            value: HkxValue::Array(vec![annotation(
                                reload_complete_time,
                                "reloadComplete",
                            )]),
                        }])]),
                    },
                ],
            }],
        );
        let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, write_hkx(&hkx, &mut registry)).unwrap();
    }

    #[test]
    fn retimes_short_reload_before_fo4_reload_end() {
        let duration = 1.333_333;
        let mut annotations = vec![
            annotation(0.033_343, "SoundPlay.ReloadMissileLauncher"),
            annotation(1.300_010, "reloadComplete"),
        ];

        assert!(retime_reload_complete(&mut annotations, duration));
        assert!((annotation_time(&annotations[1]) - 0.333_333).abs() < TIME_EPSILON);
    }

    #[test]
    fn leaves_compatible_reload_timing_unchanged() {
        let duration = 3.333_333;
        let mut annotations = vec![annotation(2.333_344, "reloadComplete")];

        assert!(!retime_reload_complete(&mut annotations, duration));
        assert!((annotation_time(&annotations[0]) - 2.333_344).abs() < TIME_EPSILON);
    }

    #[test]
    fn ignores_non_reload_annotations() {
        let mut annotations = vec![annotation(1.3, "reloadEnd")];

        assert!(!retime_reload_complete(&mut annotations, 1.333_333));
        assert!((annotation_time(&annotations[0]) - 1.3).abs() < TIME_EPSILON);
    }

    #[test]
    fn patches_wpnreload_file_in_actor_animation_tree() {
        let temp = tempfile::tempdir().unwrap();
        let reload_path = temp
            .path()
            .join("data/Meshes/Actors/MoleMiner/Animations/MissileLauncher/WPNReload.hkx");
        write_reload_clip(&reload_path, 1.333_333, 1.300_010);

        let report = retime_reload_complete_events_in_mod_path(temp.path()).unwrap();

        assert_eq!(report.records_changed, 1);
        let data = std::fs::read(reload_path).unwrap();
        let hkx = read_packfile(&data).unwrap();
        let animation = hkx
            .objects()
            .iter()
            .find(|object| object.class_name == "hkaSplineCompressedAnimation")
            .unwrap();
        let annotations = animation
            .members
            .iter()
            .find(|member| member.name == "annotationTracks")
            .and_then(|member| match &member.value {
                HkxValue::Array(tracks) => tracks.first(),
                _ => None,
            })
            .and_then(HkxValue::as_object_members)
            .and_then(|track_members| {
                track_members
                    .iter()
                    .find(|member| member.name == "annotations")
            })
            .and_then(|member| match &member.value {
                HkxValue::Array(annotations) => annotations.first(),
                _ => None,
            })
            .unwrap();
        assert!((annotation_time(annotations) - 0.333_333).abs() < TIME_EPSILON);
    }
}
