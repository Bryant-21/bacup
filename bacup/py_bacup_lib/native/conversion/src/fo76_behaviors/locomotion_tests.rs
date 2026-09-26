use super::*;

fn reference_frame(samples: Vec<f32>, duration: f32) -> HkxFile {
    let mut frame = HkxObject {
        name: None,
        offset: 0,
        signature: 0,
        class_name: "hkaDefaultAnimatedReferenceFrame".into(),
        members: vec![],
    };
    set(
        &mut frame,
        "referenceFrameSamples",
        HkxValue::F32List(samples),
    );
    set(&mut frame, "duration", HkxValue::F32(duration));
    HkxFile::from_tagxml(11, VERSION, vec![frame])
}

#[test]
fn locomotion_direction_uses_measured_motion_over_clip_name() {
    {
        let empty = HkxFile::from_tagxml(11, VERSION, vec![]);
        for file in [
            empty,
            reference_frame(vec![0.0; 4], 1.0),
            reference_frame(vec![0.0; 8], 1.0),
            reference_frame(vec![0.0; 8], 0.0),
        ] {
            for (name, direction) in [
                (r"Animations\WPNJogForwardRelaxed.hkt", 0.0),
                ("WPNWalkRight.hkx", 0.25),
                ("WPNRunBackward.hkx", 0.5),
                ("WPNWalkBackpedal.hkx", 0.5),
                ("WPNRunLeft.hkx", 0.75),
                ("WPNRunForwardLeft.hkx", 0.875),
            ] {
                assert_eq!(motion(&file, name), Some((direction, 0.0)), "{name}");
            }
            assert_eq!(motion(&file, "Idle.hkx"), None);
        }
    }
    {
        let mut file = reference_frame(vec![0.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 0.0], 2.0);
        assert_eq!(motion(&file, "WPNRunForward.hkx"), Some((0.25, 5.0)));
        set(
            &mut file.objects_mut()[0],
            "referenceFrameSamples",
            HkxValue::Array(vec![
                HkxValue::F32List(vec![0.0; 4]),
                HkxValue::F32List(vec![0.0, 10.0, 0.0, 0.0]),
            ]),
        );
        assert_eq!(motion(&file, "WPNRunLeft.hkx"), Some((0.0, 5.0)));
    }
}
