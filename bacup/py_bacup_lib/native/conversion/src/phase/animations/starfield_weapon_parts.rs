use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use havok_native::animation::pose::{PoseSkeleton, QsTransform, quat_mul, quat_rotate};
use indexmap::IndexMap;
use nif_core_native::model::{NifFile, NifValue};
use serde::Serialize;

use super::starfield_afx::AfxContract;
use super::starfield_decoder::{DecodedWeaponSuffixAnimation, StarfieldKey};
use super::weapon_parts::{
    WeaponPartSequenceReceipt, attach_behavior, attach_empty_idle,
    attach_named_weapon_part_sequence, build_named_weapon_part_behavior,
};

const METERS_TO_FO4: f32 = 69.99125;

#[derive(Debug, Serialize)]
pub(crate) struct StarfieldPartReceipt {
    pub source_suffix_nodes: Vec<String>,
    pub active_tracks: Vec<String>,
    pub visibility_tracks: Vec<String>,
    pub sequence: Option<WeaponPartSequenceReceipt>,
    pub rest_basis: &'static str,
    pub functionality_certified: bool,
}

pub(crate) struct StarfieldPartAssets {
    pub nif: Vec<u8>,
    pub graph_files: Vec<(String, Vec<u8>)>,
    pub receipt: StarfieldPartReceipt,
}

fn key<T>(key: &StarfieldKey<T>, value: NifValue) -> NifValue {
    NifValue::Struct(IndexMap::from([
        ("Time".into(), NifValue::Float(f64::from(key.frame) / 30.)),
        ("Value".into(), value),
    ]))
}

fn nif_quaternion(value: [f32; 4]) -> NifValue {
    NifValue::Quaternion([value[3], value[0], value[1], value[2]])
}

fn compose_translation(rest: &QsTransform, delta: [f64; 3]) -> [f32; 3] {
    let offset = quat_rotate(
        &rest.rotation,
        &std::array::from_fn(|axis| delta[axis] as f32 * METERS_TO_FO4 * rest.scale[axis]),
    );
    std::array::from_fn(|axis| rest.translation[axis] + offset[axis])
}

pub(crate) fn suffix_sequence(
    suffix: &DecodedWeaponSuffixAnimation,
    model_rest: &PoseSkeleton,
) -> Result<NifFile, String> {
    let mut rests = HashMap::new();
    for (name, rest) in model_rest
        .bone_names
        .iter()
        .zip(&model_rest.reference_local)
    {
        if rests.insert(name.as_str(), rest).is_some() {
            return Err(format!(
                "Starfield weapon part rest name is ambiguous: {name}"
            ));
        }
    }
    let mut output = NifFile::new("fo4");
    let sequence = output.add_block("NiControllerSequence", None);
    for (name, value) in [
        ("Start Time", NifValue::Float(0.)),
        (
            "Stop Time",
            NifValue::Float(f64::from(suffix.source_header.frame_count) / 30.),
        ),
        ("Frequency", NifValue::Float(1.)),
    ] {
        output.blocks[sequence].set_field(name, value);
    }
    let mut links = Vec::new();
    let mut names = HashSet::new();
    for track in &suffix.animation.tracks {
        if !track.scalars.is_empty() || !track.priorities.is_empty() {
            return Err(format!(
                "Starfield weapon part {} has unsupported scalar/priority keys",
                track.bone_name
            ));
        }
        if !names.insert(&track.bone_name) {
            return Err(format!(
                "duplicate Starfield weapon part track: {}",
                track.bone_name
            ));
        }
        let rest = *rests.get(track.bone_name.as_str()).ok_or_else(|| {
            format!(
                "Starfield weapon part {} has no actual model rest transform",
                track.bone_name
            )
        })?;
        if rest
            .translation
            .iter()
            .chain(&rest.rotation)
            .chain(&rest.scale)
            .any(|value| !value.is_finite())
            || rest.scale.iter().any(|value| *value <= 0.)
        {
            return Err(format!(
                "Starfield weapon part {} has an invalid rest transform",
                track.bone_name
            ));
        }
        if (rest.scale[0] - rest.scale[1]).abs() > 1e-5
            || (rest.scale[0] - rest.scale[2]).abs() > 1e-5
        {
            return Err(format!(
                "Starfield weapon part {} has nonuniform NIF scale",
                track.bone_name
            ));
        }
        let data = output.add_block("NiTransformData", None);
        output.blocks[data].set_field(
            "Num Rotation Keys",
            NifValue::UInt(track.rotations.len() as u64),
        );
        output.blocks[data].set_field("Rotation Type", NifValue::UInt(1));
        output.blocks[data].set_field(
            "Quaternion Keys",
            NifValue::Array(
                track
                    .rotations
                    .iter()
                    .map(|entry| {
                        key(
                            entry,
                            nif_quaternion(quat_mul(
                                &rest.rotation,
                                &entry.value.map(|value| value as f32),
                            )),
                        )
                    })
                    .collect(),
            ),
        );
        output.blocks[data].set_field(
            "Translations",
            NifValue::Struct(IndexMap::from([
                (
                    "Num Keys".into(),
                    NifValue::UInt(track.translations.len() as u64),
                ),
                ("Interpolation".into(), NifValue::UInt(1)),
                (
                    "Keys".into(),
                    NifValue::Array(
                        track
                            .translations
                            .iter()
                            .map(|entry| {
                                key(
                                    entry,
                                    NifValue::Vec3(compose_translation(rest, entry.value)),
                                )
                            })
                            .collect(),
                    ),
                ),
            ])),
        );
        let interpolator = output.add_block("NiTransformInterpolator", None);
        for (field, value) in [
            ("Translation", NifValue::Vec3(rest.translation)),
            ("Rotation", nif_quaternion(rest.rotation)),
            ("Scale", NifValue::Float(f64::from(rest.scale[0]))),
            ("Data", NifValue::Ref(data as i32)),
        ] {
            output.blocks[interpolator].set_field(field, value);
        }
        links.push(NifValue::Struct(IndexMap::from([
            (
                "Node Name".into(),
                NifValue::String(track.bone_name.clone()),
            ),
            (
                "Controller Type".into(),
                NifValue::String("NiTransformController".into()),
            ),
            ("Interpolator".into(), NifValue::Ref(interpolator as i32)),
            ("Controller".into(), NifValue::Ref(-1)),
        ])));
    }
    output.blocks[sequence].set_field("Num Controlled Blocks", NifValue::UInt(links.len() as u64));
    output.blocks[sequence].set_field("Controlled Blocks", NifValue::Array(links));
    Ok(output)
}

fn add_visibility_keys(
    kf: &mut NifFile,
    afx: &AfxContract,
    model: &NifFile,
    last_frame: u16,
) -> Result<Vec<String>, String> {
    let mut groups = BTreeMap::<String, Vec<(f64, bool)>>::new();
    for event in &afx.events {
        if !matches!(event.name.as_str(), "CullBone" | "UncullBone") {
            continue;
        }
        if !event.frame.is_finite() || event.frame < 0. || event.frame > f64::from(last_frame) {
            return Err("Starfield part visibility frame is outside its authored clip".into());
        }
        groups
            .entry(event.payload.clone())
            .or_default()
            .push((event.frame, event.name == "UncullBone"));
    }
    let sequence = kf
        .blocks
        .iter()
        .position(|block| block.type_name == "NiControllerSequence")
        .ok_or("Starfield part KF sequence is missing")?;
    let mut links = match kf.blocks[sequence].get_field("Controlled Blocks") {
        Some(NifValue::Array(values)) => values.clone(),
        _ => vec![],
    };
    let mut names = Vec::new();
    for (name, mut events) in groups {
        let nodes: Vec<_> = model
            .blocks
            .iter()
            .filter(|block| {
                block.get_field("Name") == Some(&NifValue::String(name.clone()))
                    && block.get_field("Translation").is_some()
            })
            .collect();
        let [node] = nodes.as_slice() else {
            return Err(format!(
                "Starfield visibility target {name} is missing or ambiguous"
            ));
        };
        let flags = match node.get_field("Flags") {
            Some(NifValue::UInt(value)) => *value,
            Some(NifValue::Int(value)) if *value >= 0 => *value as u64,
            _ => {
                return Err(format!(
                    "Starfield visibility target {name} has no initial flags"
                ));
            }
        };
        let initial_visible = flags & 1 == 0;
        events.sort_by(|left, right| left.0.total_cmp(&right.0));
        if events.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(format!(
                "Starfield visibility target {name} has simultaneous events"
            ));
        }
        if events[0].0 != 0. {
            events.insert(0, (0., initial_visible));
        }
        let data = kf.add_block("NiBoolData", None);
        kf.blocks[data].set_field(
            "Data",
            NifValue::Struct(IndexMap::from([
                ("Num Keys".into(), NifValue::UInt(events.len() as u64)),
                ("Interpolation".into(), NifValue::UInt(5)),
                (
                    "Keys".into(),
                    NifValue::Array(
                        events
                            .into_iter()
                            .map(|(frame, visible)| {
                                NifValue::Struct(IndexMap::from([
                                    ("Time".into(), NifValue::Float(frame / 30.)),
                                    ("Value".into(), NifValue::Bool(visible)),
                                ]))
                            })
                            .collect(),
                    ),
                ),
            ])),
        );
        let interpolator = kf.add_block("NiBoolInterpolator", None);
        kf.blocks[interpolator].set_field("Value", NifValue::Bool(initial_visible));
        kf.blocks[interpolator].set_field("Data", NifValue::Ref(data as i32));
        links.push(NifValue::Struct(IndexMap::from([
            ("Node Name".into(), NifValue::String(name.clone())),
            (
                "Controller Type".into(),
                NifValue::String("NiVisController".into()),
            ),
            ("Interpolator".into(), NifValue::Ref(interpolator as i32)),
            ("Controller".into(), NifValue::Ref(-1)),
        ])));
        names.push(name);
    }
    kf.blocks[sequence].set_field("Num Controlled Blocks", NifValue::UInt(links.len() as u64));
    kf.blocks[sequence].set_field("Controlled Blocks", NifValue::Array(links));
    Ok(names)
}

pub(crate) fn stage_reload_parts(
    weapon: &NifFile,
    suffix: &DecodedWeaponSuffixAnimation,
    afx: &AfxContract,
    target_extracted: &Path,
    graph_id: &str,
    playback_speed: f64,
) -> Result<StarfieldPartAssets, String> {
    if afx.tag != "Reload"
        || afx
            .flags
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("Additive"))
    {
        return Err(
            "Starfield reload part emitter requires a normal authored Reload action".into(),
        );
    }
    if graph_id.is_empty()
        || !graph_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_')
    {
        return Err("invalid Starfield weapon part graph identifier".into());
    }
    let rest = crate::phase::skeleton::nif_reference_pose(weapon)?;
    let mut kf = suffix_sequence(suffix, &rest)?;
    let visibility_tracks =
        add_visibility_keys(&mut kf, afx, weapon, suffix.source_header.frame_count)?;
    let active_tracks: Vec<_> = suffix
        .animation
        .tracks
        .iter()
        .map(|track| track.bone_name.clone())
        .collect();
    let mut receipt = StarfieldPartReceipt {
        source_suffix_nodes: suffix
            .nodes
            .iter()
            .map(|node| node.node_name.clone())
            .collect(),
        active_tracks,
        visibility_tracks,
        sequence: None,
        rest_basis: "converted_model_local",
        functionality_certified: false,
    };
    let mut weapon = weapon.clone();
    let mut graph_files = Vec::new();
    if !receipt.active_tracks.is_empty() || !receipt.visibility_tracks.is_empty() {
        let names = receipt
            .active_tracks
            .iter()
            .chain(&receipt.visibility_tracks)
            .cloned()
            .collect();
        let sequence = attach_named_weapon_part_sequence(
            &mut weapon,
            &kf,
            "StarfieldReload",
            playback_speed,
            &names,
        )?;
        if !sequence.unmatched_part_tracks.is_empty()
            || sequence.matched_tracks.len()
                != receipt.active_tracks.len() + receipt.visibility_tracks.len()
        {
            return Err(
                "Starfield reload part sequence did not bind every active source channel".into(),
            );
        }
        receipt.sequence = Some(sequence);
        attach_empty_idle(&mut weapon)?;
        let project = format!("UniqueBehaviors/{graph_id}/{graph_id}.hkx");
        attach_behavior(&mut weapon, &project)?;
        let template = target_extracted.join("meshes/UniqueBehaviors/BroadsiderFX");
        for (relative, source) in [
            (project, template.join("BroadsiderFX.hkx")),
            (
                format!("UniqueBehaviors/{graph_id}/Characters/Character.hkx"),
                template.join("Characters/Character.hkx"),
            ),
        ] {
            let bytes =
                std::fs::read(&source).map_err(|error| format!("{}: {error}", source.display()))?;
            havok_native::hkx::HkxFile::read(&bytes).map_err(|error| error.to_string())?;
            graph_files.push((relative, bytes));
        }
        let behavior = build_named_weapon_part_behavior(
            "StarfieldReload",
            vec!["reloadStart".into(), "ReloadStart".into()],
        )?;
        graph_files.push((
            format!("UniqueBehaviors/{graph_id}/Behaviors/Behavior.hkx"),
            behavior,
        ));
    }
    let nif = weapon.to_bytes().map_err(|error| error.to_string())?;
    NifFile::from_bytes(&nif, None).map_err(|error| error.to_string())?;
    Ok(StarfieldPartAssets {
        nif,
        graph_files,
        receipt,
    })
}

#[cfg(test)]
mod tests {
    use super::super::starfield_decoder::{
        AfHeader, DecodedStarfieldAnimation, StarfieldBoneTrack,
    };
    use super::*;

    fn suffix() -> DecodedWeaponSuffixAnimation {
        let header = AfHeader {
            magic: 0,
            rotation: [0., 0., 0., 1.],
            translation: [0.; 3],
            flags: [0; 4],
            version: 5,
            bone_count: 1,
            frame_count: 30,
            index_atlas_count: 2,
            fill_count: 1,
            preamble_offset: 0,
        };
        DecodedWeaponSuffixAnimation {
            source_header: header.clone(),
            nodes: vec![],
            animation: DecodedStarfieldAnimation {
                title: "Reload".into(),
                header,
                last_key_frame: 30,
                bones: vec![],
                weapon_suffix_node_hashes: vec![],
                tracks: vec![StarfieldBoneTrack {
                    bone_name: "Magazine".into(),
                    bone_index: 0,
                    rotations: vec![StarfieldKey {
                        frame: 0,
                        value: [0., 0., 0., 1.],
                    }],
                    translations: vec![
                        StarfieldKey {
                            frame: 0,
                            value: [0.; 3],
                        },
                        StarfieldKey {
                            frame: 30,
                            value: [1., 0., 0.],
                        },
                    ],
                    scalars: vec![],
                    priorities: vec![],
                }],
            },
        }
    }

    #[test]
    fn neutral_part_keys_preserve_actual_model_rest_and_delta_uses_rest_axes() {
        let rest = QsTransform {
            translation: [12., 34., 56.],
            rotation: [0., 0., 0.5_f32.sqrt(), 0.5_f32.sqrt()],
            scale: [2.; 3],
        };
        let pose = PoseSkeleton {
            bone_names: vec!["Magazine".into()],
            parent_indices: vec![-1],
            reference_local: vec![rest.clone()],
        };
        let kf = suffix_sequence(&suffix(), &pose).unwrap();
        let data = kf
            .blocks
            .iter()
            .find(|block| block.type_name == "NiTransformData")
            .unwrap();
        let NifValue::Array(rotations) = data.get_field("Quaternion Keys").unwrap() else {
            panic!()
        };
        let NifValue::Struct(rotation) = &rotations[0] else {
            panic!()
        };
        assert_eq!(rotation["Value"], nif_quaternion(rest.rotation));
        let NifValue::Struct(group) = data.get_field("Translations").unwrap() else {
            panic!()
        };
        let NifValue::Array(keys) = &group["Keys"] else {
            panic!()
        };
        let NifValue::Struct(first) = &keys[0] else {
            panic!()
        };
        assert_eq!(first["Value"], NifValue::Vec3(rest.translation));
        let NifValue::Struct(last) = &keys[1] else {
            panic!()
        };
        let NifValue::Vec3(value) = last["Value"] else {
            panic!()
        };
        assert!((value[0] - 12.).abs() < 1e-4);
        assert!((value[1] - (34. + 2. * METERS_TO_FO4)).abs() < 1e-4);
        assert!((value[2] - 56.).abs() < 1e-4);
        let interpolator = kf
            .blocks
            .iter()
            .find(|block| block.type_name == "NiTransformInterpolator")
            .unwrap();
        assert_eq!(interpolator.get_field("Scale"), Some(&NifValue::Float(2.)));
    }

    #[test]
    fn missing_ambiguous_or_unsupported_part_rest_channels_fail() {
        let mut pose = PoseSkeleton {
            bone_names: vec![],
            parent_indices: vec![],
            reference_local: vec![],
        };
        assert!(suffix_sequence(&suffix(), &pose).is_err());
        pose.bone_names = vec!["Magazine".into(), "Magazine".into()];
        pose.reference_local = vec![QsTransform::IDENTITY; 2];
        assert!(suffix_sequence(&suffix(), &pose).is_err());
        pose.bone_names.pop();
        pose.reference_local.pop();
        let mut source = suffix();
        source.animation.tracks[0].scalars.push(StarfieldKey {
            frame: 0,
            value: 1.,
        });
        assert!(suffix_sequence(&source, &pose).is_err());
    }

    #[test]
    fn visibility_uses_actual_initial_state_and_authored_cull_times() {
        let mut model = NifFile::new("fo4");
        model.blocks[0].set_field("Name", NifValue::String("Magazine".into()));
        model.blocks[0].set_field("Flags", NifValue::UInt(14));
        let mut afx = super::super::starfield_afx::parse_afx("<root><is_state>0</is_state><tag>Reload</tag><filename>Reload.af</filename><rampout>0</rampout><events><event><frame>25</frame><name>CullBone</name><payload>Magazine</payload></event><event><frame>28</frame><name>UncullBone</name><payload>Magazine</payload></event></events></root>",30.).unwrap();
        let pose = PoseSkeleton {
            bone_names: vec!["Magazine".into()],
            parent_indices: vec![-1],
            reference_local: vec![QsTransform::IDENTITY],
        };
        let mut kf = suffix_sequence(&suffix(), &pose).unwrap();
        assert_eq!(
            add_visibility_keys(&mut kf, &afx, &model, 30).unwrap(),
            vec!["Magazine"]
        );
        let block = kf
            .blocks
            .iter()
            .find(|block| block.type_name == "NiBoolData")
            .unwrap();
        let NifValue::Struct(data) = block.get_field("Data").unwrap() else {
            panic!()
        };
        let NifValue::Array(keys) = &data["Keys"] else {
            panic!()
        };
        let pairs: Vec<_> = keys
            .iter()
            .map(|key| {
                let NifValue::Struct(key) = key else { panic!() };
                (key["Time"].clone(), key["Value"].clone())
            })
            .collect();
        assert_eq!(
            pairs,
            vec![
                (NifValue::Float(0.), NifValue::Bool(true)),
                (NifValue::Float(25. / 30.), NifValue::Bool(false)),
                (NifValue::Float(28. / 30.), NifValue::Bool(true))
            ]
        );
        afx.events[1].frame = 25.;
        assert!(add_visibility_keys(&mut kf, &afx, &model, 30).is_err());
        afx.events[1].frame = 28.;
        afx.events[0].payload = "Missing".into();
        assert!(add_visibility_keys(&mut kf, &afx, &model, 30).is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires actual Starfield Eon receiver/AF and target FO4 FX templates"]
    fn real_eon_receiver_preserves_rest_suffix_and_visibility_in_saved_part_bundle() {
        use super::super::starfield_decoder::CalumiDecoder;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let meshes = root.join("extracted/starfield/meshes");
        let output = tempfile::tempdir().unwrap();
        let receiver = output.path().join("Receiver.nif");
        let report = nif_core_native::sf_convert::convert_starfield_nif(
            &meshes.join("weapons/stingerpistol/stinger.nif"),
            &receiver,
            &nif_core_native::sf_convert::SfConvertOptions {
                geometries_root: root.join("extracted/starfield/geometries"),
                material_path_rewriter: None,
                target_skeleton: None,
                translation_maps_dir: None,
                weapon_role: Some("gun".into()),
                cloth_companions: vec![],
            },
        )
        .unwrap();
        assert_eq!(report.shapes, 12);
        let model = NifFile::load(&receiver).unwrap();
        let rest = crate::phase::skeleton::nif_reference_pose(&model).unwrap();
        let names = rest.bone_names.clone();
        let magazine = names.iter().position(|name| name == "Magazine").unwrap();
        let magazine_rest = &rest.reference_local[magazine];
        assert!(magazine_rest.rotation[0].abs() > 0.1);
        assert!(
            magazine_rest
                .translation
                .iter()
                .any(|value| value.abs() > 0.001)
        );
        let decoder =
            CalumiDecoder::open(&root.join("refs/sf_animation_io/CALUMI.Animation.dll")).unwrap();
        for (perspective, folder, action, expected_links) in [
            ("1st", "_1stperson/", "reload", 4),
            ("3rd", "", "player/reload", 5),
        ] {
            let rig = meshes.join(format!("actors/human/{folder}characterassets/skeleton.rig"));
            let af = meshes.join(format!(
                "actors/human/{folder}animations/gun/grippistol/stinger/{action}.af"
            ));
            let suffix = decoder.decode_weapon_suffixes(&rig, &af, &names).unwrap();
            let afx = super::super::starfield_afx::parse_afx(
                &std::fs::read_to_string(af.with_extension("afx")).unwrap(),
                f64::from(suffix.source_header.frame_count),
            )
            .unwrap();
            let assets = stage_reload_parts(
                &model,
                &suffix,
                &afx,
                &root.join("extracted/fo4"),
                &format!("B21_SF_Eon_{perspective}"),
                1.,
            )
            .unwrap();
            let saved = NifFile::from_bytes(&assets.nif, None).unwrap();
            assert!(saved.blocks.iter().all(|block| {
                nif_core_native::fo4_block_types::is_fo4_block_type(&block.type_name)
            }));
            assert_eq!(assets.receipt.active_tracks.len(), 4);
            assert_eq!(
                assets
                    .receipt
                    .sequence
                    .as_ref()
                    .unwrap()
                    .matched_tracks
                    .len(),
                expected_links
            );
            assert_eq!(assets.graph_files.len(), 3);
            let sequence = saved
                .blocks
                .iter()
                .find(|block| {
                    block.type_name == "NiControllerSequence"
                        && block.get_field("Name")
                            == Some(&NifValue::String("StarfieldReload".into()))
                })
                .unwrap();
            let NifValue::Array(links) = sequence.get_field("Controlled Blocks").unwrap() else {
                panic!()
            };
            assert_eq!(links.len(), expected_links);
            let transform = links
                .iter()
                .find_map(|link| match link {
                    NifValue::Struct(fields)
                        if fields.get("Node Name")
                            == Some(&NifValue::String("Magazine".into()))
                            && fields.get("Controller Type")
                                == Some(&NifValue::String("NiTransformController".into())) =>
                    {
                        Some(fields)
                    }
                    _ => None,
                })
                .unwrap();
            let interpolator = &saved.blocks[transform["Interpolator"].as_i64() as usize];
            let data = &saved.blocks[interpolator.get_field("Data").unwrap().as_i64() as usize];
            let NifValue::Struct(group) = data.get_field("Translations").unwrap() else {
                panic!()
            };
            let NifValue::Array(keys) = &group["Keys"] else {
                panic!()
            };
            let NifValue::Struct(first) = &keys[0] else {
                panic!()
            };
            let NifValue::Struct(value) = &first["Value"] else {
                panic!()
            };
            for (axis, name) in ["x", "y", "z"].iter().enumerate() {
                let NifValue::Float(actual) = value[*name] else {
                    panic!()
                };
                assert!((actual - f64::from(magazine_rest.translation[axis])).abs() < 1e-6);
            }
            if perspective == "3rd" {
                let bool_data = saved
                    .blocks
                    .iter()
                    .find(|block| block.type_name == "NiBoolData")
                    .unwrap();
                let NifValue::Struct(group) = bool_data.get_field("Data").unwrap() else {
                    panic!()
                };
                let NifValue::Array(keys) = &group["Keys"] else {
                    panic!()
                };
                for (key, (time, visible)) in
                    keys.iter().zip([(0., 1), (25. / 30., 0), (28. / 30., 1)])
                {
                    let NifValue::Struct(key) = key else { panic!() };
                    let NifValue::Float(actual) = key["Time"] else {
                        panic!()
                    };
                    assert!((actual - time).abs() < 1e-6);
                    assert_eq!(key["Value"].as_i64(), visible);
                }
            }
            for (_, bytes) in &assets.graph_files {
                havok_native::hkx::HkxFile::read(bytes).unwrap();
            }
            assert!(!assets.receipt.functionality_certified);
        }
    }
}
