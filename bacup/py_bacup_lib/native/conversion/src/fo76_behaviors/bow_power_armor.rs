use super::*;
use havok_native::animation::{
    clip::extract_clip,
    expand::expand,
    parsers::{parse_skeleton_xml, SkeletonRecord},
    pose::{quat_conjugate, quat_mul, quat_normalize, quat_rotate, QsTransform},
};
use havok_native::hkx::tagxml::write_tagxml_string;

pub(super) struct Retargeter {
    source: SkeletonRecord,
    target: SkeletonRecord,
    source_pose: Vec<QsTransform>,
    target_pose: Vec<QsTransform>,
    target_to_source: Vec<Option<usize>>,
}

fn pose(rig: &SkeletonRecord) -> Vec<QsTransform> {
    rig.reference_pose
        .iter()
        .map(|p| QsTransform {
            translation: p.t,
            rotation: quat_normalize(&p.q),
            scale: p.s,
        })
        .collect()
}

fn model_pose(local: &[QsTransform], parents: &[i32]) -> Vec<QsTransform> {
    let mut model = Vec::with_capacity(local.len());
    for (bone, transform) in local.iter().enumerate() {
        model.push(if parents[bone] < 0 {
            transform.clone()
        } else {
            QsTransform::compose(&model[parents[bone] as usize], transform)
        });
    }
    model
}

fn relative(parent: &QsTransform, child: &QsTransform) -> QsTransform {
    let inverse = quat_conjugate(&parent.rotation);
    let offset = std::array::from_fn(|i| child.translation[i] - parent.translation[i]);
    let translation = quat_rotate(&inverse, &offset);
    QsTransform {
        translation: std::array::from_fn(|i| translation[i] / parent.scale[i]),
        rotation: quat_normalize(&quat_mul(&inverse, &child.rotation)),
        scale: std::array::from_fn(|i| child.scale[i] / parent.scale[i]),
    }
}

impl Retargeter {
    pub(super) fn is_identity(&self) -> bool {
        self.source.bone_names == self.target.bone_names
            && self.source.parent_indices == self.target.parent_indices
            && self.source_pose == self.target_pose
    }

    pub(super) fn load(
        source_root: &Path,
        target_root: &Path,
        project: &str,
    ) -> Result<Self, String> {
        let load = |root: &Path, dir: &str| {
            let file = super::assets::project_rig(root, dir)?;
            parse_skeleton_xml(&write_tagxml_string(&file).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())
        };
        Self::new(
            load(source_root, &project.replace("powerarmor", "character"))?,
            load(target_root, project)?,
        )
    }

    fn new(source: SkeletonRecord, target: SkeletonRecord) -> Result<Self, String> {
        for rig in [&source, &target] {
            if rig.bone_names.len() != rig.reference_pose.len()
                || rig.bone_names.len() != rig.parent_indices.len()
                || rig
                    .parent_indices
                    .iter()
                    .enumerate()
                    .any(|(i, &p)| p < -1 || p >= i as i32)
                || rig
                    .reference_pose
                    .iter()
                    .any(|p| p.s.iter().any(|s| s.abs() < 1e-6))
            {
                return Err(format!("invalid bow skeleton {}", rig.name));
            }
        }
        let target_to_source = target
            .bone_names
            .iter()
            .map(|name| source.bone_names.iter().position(|n| n == name))
            .collect();
        Ok(Self {
            source_pose: pose(&source),
            target_pose: pose(&target),
            source,
            target,
            target_to_source,
        })
    }

    fn source_relative(
        &self,
        model: &[QsTransform],
        target_bone: usize,
    ) -> Result<QsTransform, String> {
        let source_bone = self.target_to_source[target_bone].ok_or("unmapped bow bone")?;
        let parent = self.target.parent_indices[target_bone];
        if parent < 0 {
            return Ok(model[source_bone].clone());
        }
        let source_parent = self.target_to_source[parent as usize]
            .ok_or_else(|| format!("unmapped parent of {}", self.target.bone_names[target_bone]))?;
        Ok(relative(&model[source_parent], &model[source_bone]))
    }

    fn retarget_frame(
        &self,
        local: &[QsTransform],
        bones: &[usize],
    ) -> Result<Vec<QsTransform>, String> {
        let reference = model_pose(&self.source_pose, &self.source.parent_indices);
        let model = model_pose(local, &self.source.parent_indices);
        bones
            .iter()
            .map(|&bone| {
                // Hands attach to ForeArm1 on PA, bypassing the human twist-bone chain.
                let reference = self.source_relative(&reference, bone)?;
                let animated = self.source_relative(&model, bone)?;
                let target = &self.target_pose[bone];
                Ok(QsTransform {
                    translation: std::array::from_fn(|i| {
                        target.translation[i] + animated.translation[i] - reference.translation[i]
                    }),
                    rotation: quat_normalize(&quat_mul(
                        &target.rotation,
                        &quat_mul(&quat_conjugate(&reference.rotation), &animated.rotation),
                    )),
                    scale: std::array::from_fn(|i| {
                        target.scale[i] * animated.scale[i] / reference.scale[i]
                    }),
                })
            })
            .collect()
    }

    pub(super) fn apply(&self, file: &mut HkxFile) -> Result<(), String> {
        if self.is_identity() {
            return Ok(());
        }
        let binding = file
            .objects()
            .iter()
            .position(|o| o.class_name == "hkaAnimationBinding")
            .ok_or("bow animation has no binding")?;
        let anim =
            pointer(&file.objects()[binding], "animation").ok_or("bow binding has no animation")?;
        let original = file.objects()[anim].clone();
        if !matches!(
            original.class_name.as_str(),
            "hkaLosslessCompressedAnimation"
                | "hkaSplineCompressedAnimation"
                | "hkaInterleavedUncompressedAnimation"
        ) || value(&original, "numberOfFloatTracks")
            .and_then(number)
            .unwrap_or(0.0)
            != 0.0
        {
            return Err(format!(
                "unsupported bow animation payload {}",
                original.class_name
            ));
        }
        let blend = value(&file.objects()[binding], "blendHint")
            .and_then(number)
            .unwrap_or(0.0) as u8;
        if blend > 2 {
            return Err(format!("unsupported bow blend hint {blend}"));
        }
        let clip = extract_clip(&write_tagxml_string(file).map_err(|e| e.to_string())?, None)
            .map_err(|e| e.to_string())?;
        if !clip.warnings.is_empty() {
            return Err(format!("bow decode warnings: {:?}", clip.warnings));
        }
        let indices: Vec<usize> = if clip.track_to_bone_indices.is_empty() {
            (0..clip.channels.len()).collect()
        } else {
            clip.track_to_bone_indices
                .iter()
                .map(|i| *i as usize)
                .collect()
        };
        if indices.len() != clip.channels.len()
            || indices.iter().any(|i| *i >= self.source_pose.len())
        {
            return Err(format!(
                "bow bindings exceed source rig: {} tracks / {} bones",
                indices.len(),
                self.source_pose.len()
            ));
        }
        let bones: Vec<usize> = self
            .target_to_source
            .iter()
            .enumerate()
            .filter_map(|(target, source)| {
                source
                    .filter(|source| indices.contains(source))
                    .map(|_| target)
            })
            .collect();
        if bones.is_empty() {
            return Err("bow animation has no mapped tracks".into());
        }
        let dense = expand(&clip, clip.native_fps.max(1.0));
        let mut transforms = Vec::with_capacity(dense.frame_count * bones.len());
        for frame in 0..dense.frame_count {
            let mut local = self.source_pose.clone();
            for (track, &bone) in indices.iter().enumerate() {
                let channel = &dense.channels[track];
                let sample = QsTransform {
                    translation: channel.translations[frame],
                    rotation: channel.rotations[frame],
                    scale: if clip.channels[track].scales.is_empty() {
                        [1.0; 3]
                    } else {
                        channel.scales[frame]
                    },
                };
                local[bone] = if blend == 0 {
                    sample
                } else {
                    apply_additive(&self.source_pose[bone], &sample, blend)
                };
            }
            for (bone, mut sample) in bones.iter().zip(self.retarget_frame(&local, &bones)?) {
                if blend != 0 {
                    sample = remove_additive(&self.target_pose[*bone], &sample, 1);
                }
                transforms.push(HkxValue::F32List(vec![
                    sample.translation[0],
                    sample.translation[1],
                    sample.translation[2],
                    0.0,
                    sample.rotation[0],
                    sample.rotation[1],
                    sample.rotation[2],
                    sample.rotation[3],
                    sample.scale[0],
                    sample.scale[1],
                    sample.scale[2],
                    0.0,
                ]));
            }
        }
        let mut replacement = original.clone();
        replacement.class_name = "hkaInterleavedUncompressedAnimation".into();
        replacement.members.retain(|m| {
            matches!(
                m.name.as_str(),
                "duration" | "numberOfFloatTracks" | "extractedMotion" | "annotationTracks"
            )
        });
        set(&mut replacement, "type", HkxValue::I32(1));
        set(
            &mut replacement,
            "numberOfTransformTracks",
            HkxValue::I32(bones.len() as i32),
        );
        set(&mut replacement, "transforms", HkxValue::Array(transforms));
        set(&mut replacement, "floats", HkxValue::Array(vec![]));
        // Retain annotation track positions while dropping source-only bone tracks.
        let annotations = array(&original, "annotationTracks");
        if annotations.len() == indices.len() {
            set(
                &mut replacement,
                "annotationTracks",
                HkxValue::Array(
                    bones
                        .iter()
                        .map(|bone| {
                            let source = self.target_to_source[*bone].unwrap();
                            annotations[indices.iter().position(|i| *i == source).unwrap()].clone()
                        })
                        .collect(),
                ),
            );
        }
        file.objects_mut()[anim] = replacement;
        let binding = &mut file.objects_mut()[binding];
        if blend != 0 {
            set(binding, "blendHint", HkxValue::U8(1));
        }
        set(
            binding,
            "transformTrackToBoneIndices",
            HkxValue::Array(bones.iter().map(|i| HkxValue::I16(*i as i16)).collect()),
        );
        set_string(binding, "originalSkeletonName", &self.target.name);
        Ok(())
    }
}

fn apply_additive(base: &QsTransform, delta: &QsTransform, blend: u8) -> QsTransform {
    QsTransform {
        translation: std::array::from_fn(|i| base.translation[i] + delta.translation[i]),
        rotation: quat_normalize(&if blend == 1 {
            quat_mul(&base.rotation, &delta.rotation)
        } else {
            quat_mul(&delta.rotation, &base.rotation)
        }),
        scale: std::array::from_fn(|i| base.scale[i] * delta.scale[i]),
    }
}

fn remove_additive(base: &QsTransform, pose: &QsTransform, blend: u8) -> QsTransform {
    QsTransform {
        translation: std::array::from_fn(|i| pose.translation[i] - base.translation[i]),
        rotation: quat_normalize(&if blend == 1 {
            quat_mul(&quat_conjugate(&base.rotation), &pose.rotation)
        } else {
            quat_mul(&pose.rotation, &quat_conjugate(&base.rotation))
        }),
        scale: std::array::from_fn(|i| pose.scale[i] / base.scale[i]),
    }
}

#[cfg(test)]
#[path = "bow_power_armor_tests.rs"]
mod tests;
