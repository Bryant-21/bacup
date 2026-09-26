use super::*;
use havok_native::animation::parsers::BonePose;

fn skeleton(parents: &[i32], translations: &[[f32; 3]]) -> SkeletonRecord {
    SkeletonRecord {
        name: "B21_TestRig".into(),
        bone_count: parents.len(),
        bone_names: (0..parents.len()).map(|i| format!("Bone{i}")).collect(),
        parent_indices: parents.to_vec(),
        reference_pose: translations
            .iter()
            .map(|t| BonePose {
                t: *t,
                q: [0., 0., 0., 1.],
                s: [1.; 3],
            })
            .collect(),
        lock_translation: vec![false; parents.len()],
        float_count: 0,
        float_slots: vec![],
        reference_floats: vec![],
        partition_names: vec![],
    }
}

#[test]
fn power_armor_bow_preserves_proportions_rotation_orders_and_authored_motion() {
    {
        let source = skeleton(
            &[-1, 0, 1, 2],
            &[[0.; 3], [1., 0., 0.], [2., 0., 0.], [3., 0., 0.]],
        );
        let target = skeleton(
            &[-1, 0, 1, 1, 0],
            &[[0.; 3], [2., 0., 0.], [0.; 3], [9., 0., 0.], [0.; 3]],
        );
        let mapper = Retargeter::new(source, target).unwrap();
        assert!(!mapper.is_identity());
        let mapped = mapper
            .retarget_frame(&mapper.source_pose, &[0, 1, 2, 3])
            .unwrap();
        assert_eq!(mapped, mapper.target_pose[..4]);
        let mut animated = mapper.source_pose.clone();
        animated[2].translation[1] = 2.;
        animated[3].translation[2] = 1.;
        let mapped = mapper.retarget_frame(&animated, &[3]).unwrap();
        assert_eq!(mapped[0].translation, [9., 2., 1.]);
        for blend in [1, 2] {
            let local: Vec<_> = mapper
                .source_pose
                .iter()
                .map(|base| apply_additive(base, &QsTransform::IDENTITY, blend))
                .collect();
            for (bone, result) in mapper
                .retarget_frame(&local, &[0, 1, 2, 3])
                .unwrap()
                .iter()
                .enumerate()
            {
                assert_eq!(
                    remove_additive(&mapper.target_pose[bone], result, blend),
                    QsTransform::IDENTITY
                );
            }
        }
    }
    {
        let base = QsTransform {
            rotation: quat_normalize(&[0.2, 0.3, 0.1, 0.9]),
            ..QsTransform::IDENTITY
        };
        let delta = QsTransform {
            rotation: quat_normalize(&[0.1, -0.2, 0.2, 0.8]),
            translation: [1., 2., 3.],
            ..QsTransform::IDENTITY
        };
        assert_ne!(
            apply_additive(&base, &delta, 1),
            apply_additive(&base, &delta, 2)
        );
        for blend in [1, 2] {
            let actual = remove_additive(&base, &apply_additive(&base, &delta, blend), blend);
            assert_eq!(actual.translation, delta.translation);
            for (a, b) in actual.rotation.iter().zip(delta.rotation) {
                assert!((a - b).abs() < 1e-5);
            }
        }
    }
    {
        let source = skeleton(&[-1, 0, 1], &[[0.; 3], [2., 0., 0.], [4., 0., 0.]]);
        let mut target = skeleton(&[-1, 0, 1], &[[0.; 3], [3., 0., 0.], [7., 0., 0.]]);
        target.lock_translation = vec![false, true, true];
        let mapper = Retargeter::new(source, target).unwrap();
        let mut local = mapper.source_pose.clone();
        local[0].translation = [1., 2., 3.];
        local[1].translation = [15., 6., 7.];
        local[2].translation = [10., 11., 12.];
        local[2].rotation = quat_normalize(&[0.2, 0.3, 0.1, 0.9]);
        let result = mapper.retarget_frame(&local, &[0, 1, 2]).unwrap();
        assert_eq!(result[0].translation, local[0].translation);
        assert_eq!(result[1].translation, [16., 6., 7.]);
        assert_eq!(result[2].translation, [13., 11., 12.]);
        assert_ne!(result[2].rotation, mapper.target_pose[2].rotation);
    }
}
