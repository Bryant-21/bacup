use super::*;

fn sample(time: f32, value: [f32; 4]) -> SkyrimTimedMotionSample {
    SkyrimTimedMotionSample { time, value }
}

#[test]
fn skyrim_bound_motion_respects_authored_sample_times() {
    let motion = SkyrimBoundMotionSamples {
        duration: 1.0,
        translations: vec![
            sample(0.0, [0.0; 4]),
            sample(0.25, [0.0, 10.0, 0.0, 0.0]),
            sample(1.0, [0.0; 4]),
        ],
        rotations: vec![
            sample(0.0, [0.0, 0.0, 0.0, 1.0]),
            sample(0.25, [0.0, 0.0, 0.5_f32.sqrt(), 0.5_f32.sqrt()]),
            sample(1.0, [0.0, 0.0, 0.0, 1.0]),
        ],
    };
    let frames = planar_reference_frame_samples(&motion, 3).unwrap();
    assert!((frames[1][1] - 20.0 / 3.0).abs() < 1.0e-5);
    assert!((frames[1][3] - std::f32::consts::PI / 3.0).abs() < 1.0e-5);
    let frames = planar_reference_frame_samples(&motion, 5).unwrap();
    assert_eq!(frames[1][1], 10.0);
    assert!((frames[1][3] - std::f32::consts::FRAC_PI_2).abs() < 1.0e-5);
}

#[test]
fn skyrim_bound_motion_preserves_vertical_translation_and_unwraps_yaw() {
    let rotation = |degrees: f32| {
        let (sin, cos) = (degrees.to_radians() * 0.5).sin_cos();
        [0.0, 0.0, sin, cos]
    };
    let motion = SkyrimBoundMotionSamples {
        duration: 1.0,
        translations: vec![sample(0.0, [0.0; 4]), sample(1.0, [1.0, 2.0, 30.0, 0.0])],
        rotations: vec![sample(0.0, rotation(170.0)), sample(1.0, rotation(190.0))],
    };
    let frames = planar_reference_frame_samples(&motion, 2).unwrap();
    assert_eq!(frames[1][..3], [1.0, 2.0, 30.0]);
    assert!((frames[1][3] - 190.0_f32.to_radians()).abs() < 1.0e-5);
}
