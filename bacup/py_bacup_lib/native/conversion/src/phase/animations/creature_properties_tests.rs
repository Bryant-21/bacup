use super::*;

fn source_sequence(priority: u64) -> NifFile {
    let mut nif = NifFile::new("fo4");
    let sequence = nif.add_block("NiControllerSequence", None);
    for (field, value) in [("Start Time", 2.0), ("Stop Time", 6.0), ("Frequency", 2.0)] {
        nif.blocks[sequence].set_field(field, NifValue::Float(value));
    }
    let interpolator = nif.add_block("NiFloatInterpolator", None);
    nif.blocks[interpolator].set_field("Value", NifValue::Float(0.5));
    set_array(&mut nif, sequence, "Num Controlled Blocks", "Controlled Blocks", vec![NifValue::Struct(IndexMap::from([
        ("Node Name".into(), NifValue::String("Screen01:0".into())),
        ("Property Type".into(), NifValue::String("NiTexturingProperty".into())),
        ("Controller Type".into(), NifValue::String("NiTextureTransformController".into())),
        ("Controller ID".into(), NifValue::String("0-0-TT_TRANSLATE_U".into())),
        ("Interpolator ID".into(), NifValue::String(String::new())),
        ("Priority".into(), NifValue::UInt(priority)),
        ("Interpolator".into(), NifValue::Ref(interpolator as i32)),
    ]))]);
    nif
}

fn target_model() -> NifFile {
    let mut nif = NifFile::new("fo4");
    let shape = nif.add_block("BSTriShape", None);
    nif.blocks[shape].set_field("Name", NifValue::String("Screen01:0".into()));
    let shader = nif.add_block("BSLightingShaderProperty", None);
    nif.blocks[shape].set_field("Shader Property", NifValue::Ref(shader as i32));
    set_array(&mut nif, 0, "Num Children", "Children", vec![NifValue::Ref(shape as i32)]);
    nif
}

#[test]
fn creature_property_sequences_preserve_priority_and_segment_timing_on_one_controller() {
    let mut nif = target_model();
    for (name, priority) in [("SourceIdle", 50), ("SourceFidget", 60)] {
        let receipt = attach_creature_property_sequence(&mut nif, &source_sequence(priority), 0, name, 0.25, Some(1.25), 1.5, true).unwrap();
        assert_eq!(receipt.matched_tracks, ["Screen01:0:NiTextureTransformController"]);
        assert!(receipt.unmatched_part_tracks.is_empty());
    }
    let reloaded = NifFile::from_bytes(&nif.to_bytes().unwrap(), None).unwrap();
    let controllers = reloaded.blocks.iter().filter(|block| block.type_name == "BSLightingShaderPropertyFloatController").collect::<Vec<_>>();
    assert_eq!(controllers.len(), 1);
    assert_eq!(controllers[0].get_field("Controlled Variable"), Some(&NifValue::UInt(20)));
    for (name, priority) in [("SourceIdle", 50), ("SourceFidget", 60)] {
        let sequence = reloaded.blocks.iter().find(|block| block.string_field("Name").as_deref() == Some(name)).unwrap();
        assert_eq!(sequence.f64_field("Start Time"), Some(2.5));
        assert_eq!(sequence.f64_field("Stop Time"), Some(4.5));
        assert_eq!(sequence.f64_field("Frequency"), Some(3.0));
        assert_eq!(sequence.string_field("Accum Root Name").as_deref(), Some(""));
        let text_keys = reference(sequence.get_field("Text Keys")).unwrap();
        assert!(array(reloaded.blocks[text_keys].get_field("Text Keys")).is_empty());
        let NifValue::Struct(link) = &array(sequence.get_field("Controlled Blocks"))[0] else { panic!() };
        assert_eq!(link.get("Priority"), Some(&NifValue::UInt(priority)));
        assert_eq!(link.get("Controller Type"), Some(&NifValue::String("BSLightingShaderPropertyFloatController".into())));
    }
}

#[test]
fn creature_property_sequence_instances_keep_independent_interpolators_and_shared_targets() {
    let mut nif = target_model();
    attach_creature_property_sequence(&mut nif, &source_sequence(60), 0, "Base", 0.25, Some(1.25), 1.0, true).unwrap();
    instantiate_creature_property_sequence(&mut nif, "Base", &["Base__Clock1".into(), "Base__Clock2".into()]).unwrap();
    let reloaded = NifFile::from_bytes(&nif.to_bytes().unwrap(), None).unwrap();
    let sequences = reloaded.blocks.iter().filter(|block| block.type_name == "NiControllerSequence").collect::<Vec<_>>();
    assert_eq!(sequences.len(), 2);
    assert!(!sequences.iter().any(|sequence| sequence.string_field("Name").as_deref() == Some("Base")));
    let links = sequences.iter().map(|sequence| {
        let NifValue::Struct(link) = &array(sequence.get_field("Controlled Blocks"))[0] else { panic!() };
        link
    }).collect::<Vec<_>>();
    assert_eq!(links[0].get("Controller"), links[1].get("Controller"));
    assert_ne!(links[0].get("Interpolator"), links[1].get("Interpolator"));
    assert_eq!(links[0].get("Priority"), Some(&NifValue::UInt(60)));
    assert_eq!(links[1].get("Priority"), Some(&NifValue::UInt(60)));
}

#[test]
fn creature_property_sequences_reject_duplicate_targets_and_controller_identities() {
    let mut nif = target_model();
    let duplicate = nif.add_block("BSTriShape", None);
    nif.blocks[duplicate].set_field("Name", NifValue::String("Screen01:0".into()));
    assert!(attach_creature_property_sequence(&mut nif, &source_sequence(50), 0, "Idle", 0.0, None, 1.0, true).unwrap_err().contains("ambiguous"));
    let mut source = source_sequence(50);
    let sequence = source.blocks.iter().position(|block| block.type_name == "NiControllerSequence").unwrap();
    let link = array(source.blocks[sequence].get_field("Controlled Blocks"))[0].clone();
    set_array(&mut source, sequence, "Num Controlled Blocks", "Controlled Blocks", vec![link.clone(), link]);
    assert!(attach_creature_property_sequence(&mut target_model(), &source, 0, "Idle", 0.0, None, 1.0, true).unwrap_err().contains("duplicate creature property controller identity"));
}

#[test]
#[ignore = "requires B21_FNV_SOURCE_MESHES pointing to extracted source meshes"]
fn real_securitron_screen_sequences_drive_converted_shader_with_source_priorities() {
    let root = std::path::PathBuf::from(std::env::var("B21_FNV_SOURCE_MESHES").unwrap());
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("body.nif");
    nif_core_native::convert_file::stage_preserve_source_rig_nif(
        nif_core_native::convert_file::SourceRigNifKind::Body,
        &root.join("creatures/nvsecuritron/nvsecuritroninfantryscreen01.nif"),
        &output, "actors/test/body.nif", "fnv", None, &Default::default(),
    ).unwrap();
    let mut target = NifFile::load(output.clone()).unwrap();
    for (path, name, priority) in [
        ("creatures/nvsecuritron/mtidle.kf", "Idle", 50),
        ("creatures/nvsecuritron/idleanims/specialidle_fidgeta.kf", "Fidget", 60),
    ] {
        let source = NifFile::load(root.join(path)).unwrap();
        let receipt = attach_creature_property_sequence(&mut target, &source, 0, name, 0.0, None, 1.0, true).unwrap();
        assert_eq!(receipt.matched_tracks.len(), 5, "{receipt:?}");
        assert_eq!(receipt.neutral_shader_tracks.len(), 1);
        assert!(receipt.unmatched_part_tracks.iter().all(|target| target == "ScreenStatic:0" || target == "##VoiceBox_talk:0"), "{receipt:?}");
        let sequence = target.blocks.iter().find(|block| block.string_field("Name").as_deref() == Some(name)).unwrap();
        for link in array(sequence.get_field("Controlled Blocks")) {
            let NifValue::Struct(fields) = link else { panic!() };
            assert_eq!(fields.get("Priority"), Some(&NifValue::UInt(priority)));
            assert_ne!(fields.get("Controller Type"), Some(&NifValue::String("NiTransformController".into())));
        }
    }
    target.save(Some(output.clone())).unwrap();
    let reloaded = NifFile::load(output).unwrap();
    assert_eq!(reloaded.blocks.iter().filter(|block| block.type_name == "NiControllerSequence").count(), 2);
    assert_eq!(reloaded.blocks.iter().filter(|block| block.type_name == "BSEffectShaderPropertyFloatController").count(), 4);
    assert_eq!(reloaded.blocks.iter().filter(|block| block.type_name == "BSEffectShaderPropertyColorController").count(), 1);
}
