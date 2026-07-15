//! Repair FO76 first-person charge holds for the FO4 runtime.
//!
//! Some FO76 ready and sighted charge-hold clips have a null `extractedMotion`.
//! FO4's equivalent charge clips carry a neutral `hkaDefaultAnimatedReferenceFrame`;
//! without it the first-person behavior graph cannot leave the charge state.

use std::path::{Path, PathBuf};

use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxMember, HkxObject, read_packfile};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

const REFERENCE_FRAME_SIGNATURE: u32 = 0x60f8_e0b8;
const FIRST_PERSON_ANIMATIONS_PREFIX: &str = "actors/character/_1stperson/animations/";
const CHARGE_HOLD_CLIP_NAMES: [&str; 2] =
    ["wpnchargeholdreadyadd.hkx", "wpnchargeholdsightedadd.hkx"];

pub struct RepairWeaponChargeReferenceFramesFixup;

impl Fixup for RepairWeaponChargeReferenceFramesFixup {
    fn name(&self) -> &'static str {
        "repair_weapon_charge_reference_frames"
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
        repair_weapon_charge_reference_frames_in_mod_path(mod_path)
    }
}

pub fn repair_weapon_charge_reference_frames_in_mod_path(
    mod_path: &Path,
) -> Result<FixupReport, FixupError> {
    let mut repaired = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_target_clips(&meshes_root, &meshes_root, &mut |path| {
            if repair_charge_reference_frame(path).unwrap_or(false) {
                repaired += 1;
            }
        });
    }
    Ok(FixupReport {
        records_changed: repaired,
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

fn walk_target_clips(root: &Path, dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_target_clips(root, &path, f);
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let normalized = relative
            .components()
            .filter_map(|part| part.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/")
            .to_ascii_lowercase();
        if is_first_person_charge_hold(&normalized) {
            f(&path);
        }
    }
}

fn is_first_person_charge_hold(path: &str) -> bool {
    path.starts_with(FIRST_PERSON_ANIMATIONS_PREFIX)
        && path
            .rsplit_once('/')
            .is_some_and(|(_, name)| CHARGE_HOLD_CLIP_NAMES.contains(&name))
}

fn repair_charge_reference_frame(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let mut hkx = read_packfile(&data)?;

    let animation_index = hkx.objects().iter().position(|object| {
        matches!(
            object.class_name.as_str(),
            "hkaLosslessCompressedAnimation"
                | "hkaSplineCompressedAnimation"
                | "hkaInterleavedUncompressedAnimation"
        )
    });
    let Some(animation_index) = animation_index else {
        return Ok(false);
    };

    let animation = &hkx.objects()[animation_index];
    let duration = animation
        .members
        .iter()
        .find_map(|member| (member.name == "duration").then_some(&member.value))
        .and_then(|value| match value {
            HkxValue::F32(value) => Some(*value),
            _ => None,
        });
    let Some(duration) = duration else {
        return Ok(false);
    };
    let needs_reference_frame = animation.members.iter().any(|member| {
        member.name == "extractedMotion" && matches!(member.value, HkxValue::Pointer(None))
    });
    if !needs_reference_frame {
        return Ok(false);
    }

    let reference_frame_index = hkx.push_object(HkxObject {
        name: None,
        offset: 0,
        signature: REFERENCE_FRAME_SIGNATURE,
        class_name: "hkaDefaultAnimatedReferenceFrame".to_string(),
        members: vec![
            HkxMember {
                name: "up".to_string(),
                value: HkxValue::F32List(vec![0.0, 0.0, 1.0, 0.0]),
            },
            HkxMember {
                name: "forward".to_string(),
                value: HkxValue::F32List(vec![0.0, 1.0, 0.0, 0.0]),
            },
            HkxMember {
                name: "duration".to_string(),
                value: HkxValue::F32(duration),
            },
            HkxMember {
                name: "referenceFrameSamples".to_string(),
                value: HkxValue::Array(vec![
                    HkxValue::F32List(vec![0.0, 0.0, 0.0, 0.0]),
                    HkxValue::F32List(vec![0.0, 0.0, 0.0, 0.0]),
                ]),
            },
        ],
    });

    let animation = &mut hkx.objects_mut()[animation_index];
    let extracted_motion = animation
        .members
        .iter_mut()
        .find(|member| member.name == "extractedMotion")
        .expect("checked extractedMotion above");
    extracted_motion.value = HkxValue::Pointer(Some(reference_frame_index));

    std::fs::write(path, hkx.save())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use havok_native::hkx::descriptors::DescriptorRegistry;
    use havok_native::hkx::{HkxFile, write_hkx};

    fn charge_clip_path(mod_path: &Path, weapon: &str, name: &str) -> PathBuf {
        mod_path
            .join("data/Meshes/Actors/Character/_1stPerson/Animations")
            .join(weapon)
            .join(name)
    }

    fn write_charge_clip(path: &Path, extracted_motion: HkxValue) {
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
                        value: HkxValue::F32(1.833_333),
                    },
                    HkxMember {
                        name: "extractedMotion".to_string(),
                        value: extracted_motion,
                    },
                ],
            }],
        );
        let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
        let data = write_hkx(&hkx, &mut registry);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }

    #[test]
    fn repairs_null_reference_frame_on_any_first_person_charge_hold() {
        let tmp = tempfile::tempdir().unwrap();
        for weapon in ["GaussPistol", "CompoundBow", "FutureChargeWeapon"] {
            for name in ["wpnchargeholdreadyadd.hkx", "wpnchargeholdsightedadd.hkx"] {
                write_charge_clip(
                    &charge_clip_path(tmp.path(), weapon, name),
                    HkxValue::Pointer(None),
                );
            }
        }

        let report = repair_weapon_charge_reference_frames_in_mod_path(tmp.path()).unwrap();
        assert_eq!(report.records_changed, 6);

        let data = std::fs::read(charge_clip_path(
            tmp.path(),
            "CompoundBow",
            "wpnchargeholdreadyadd.hkx",
        ))
        .unwrap();
        let hkx = read_packfile(&data).unwrap();
        let animation = hkx
            .objects()
            .iter()
            .find(|object| object.class_name == "hkaSplineCompressedAnimation")
            .unwrap();
        let reference_index = animation
            .members
            .iter()
            .find(|member| member.name == "extractedMotion")
            .and_then(|member| match member.value {
                HkxValue::Pointer(index) => index,
                _ => None,
            })
            .expect("non-null extractedMotion");
        let reference = &hkx.objects()[reference_index];
        assert_eq!(reference.class_name, "hkaDefaultAnimatedReferenceFrame");
        assert!(reference.members.iter().any(|member| {
            member.name == "duration"
                && matches!(member.value, HkxValue::F32(value) if (value - 1.833_333).abs() < 0.000_001)
        }));
    }

    #[test]
    fn ignores_unrelated_animation_paths_and_non_null_reference_frames() {
        let tmp = tempfile::tempdir().unwrap();
        let third_person = tmp.path().join(
            "data/Meshes/Actors/Character/Animations/Weapon/GaussPistol/wpnchargeholdreadyadd.hkx",
        );
        write_charge_clip(&third_person, HkxValue::Pointer(None));

        let unrelated = charge_clip_path(tmp.path(), "GaussPistol", "wpnchargeup_additive.hkx");
        write_charge_clip(&unrelated, HkxValue::Pointer(None));

        let target = charge_clip_path(tmp.path(), "GaussPistol", "wpnchargeholdreadyadd.hkx");
        write_charge_clip(&target, HkxValue::Pointer(Some(0)));

        let report = repair_weapon_charge_reference_frames_in_mod_path(tmp.path()).unwrap();
        assert!(report.is_no_op());
    }

    #[test]
    fn invalid_target_hkx_is_tolerated() {
        let tmp = tempfile::tempdir().unwrap();
        let path = charge_clip_path(tmp.path(), "GaussPistol", "wpnchargeholdreadyadd.hkx");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"not hkx").unwrap();

        let report = repair_weapon_charge_reference_frames_in_mod_path(tmp.path()).unwrap();
        assert!(report.is_no_op());
    }
}
