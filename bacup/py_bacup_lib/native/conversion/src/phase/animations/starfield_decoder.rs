use std::path::Path;

use serde::{Deserialize, Serialize};

#[path = "starfield_suffix.rs"]
pub(super) mod suffix;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StarfieldKey<T> {
    pub frame: u16,
    pub value: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StarfieldBone {
    pub name: String,
    pub parent: i32,
    pub rotation: [f64; 4],
    pub translation: [f64; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StarfieldBoneTrack {
    pub bone_index: usize,
    pub bone_name: String,
    pub rotations: Vec<StarfieldKey<[f64; 4]>>,
    pub translations: Vec<StarfieldKey<[f64; 3]>>,
    pub scalars: Vec<StarfieldKey<f32>>,
    pub priorities: Vec<StarfieldKey<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct AfHeader {
    pub magic: u64,
    pub rotation: [f32; 4],
    pub translation: [f32; 3],
    pub flags: [u8; 4],
    pub version: u16,
    pub bone_count: u16,
    pub frame_count: u16,
    pub index_atlas_count: u16,
    pub fill_count: u16,
    pub preamble_offset: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DecodedStarfieldAnimation {
    pub title: String,
    // CALUMI's universal scene omits the source header and reports the last key
    // frame rather than the authored duration. Keep both for graph integration.
    pub header: AfHeader,
    pub last_key_frame: u16,
    pub bones: Vec<StarfieldBone>,
    pub tracks: Vec<StarfieldBoneTrack>,
    pub weapon_suffix_node_hashes: Vec<u32>,
}

#[derive(Debug, Serialize)]
pub(super) struct DecodedWeaponSuffixAnimation {
    pub source_header: AfHeader,
    pub nodes: Vec<suffix::SuffixNode>,
    pub animation: DecodedStarfieldAnimation,
}

fn af_header(bytes: &[u8]) -> Result<AfHeader, String> {
    if bytes.len() < 64 {
        return Err("Starfield AF header is truncated".into());
    }
    let u16_at = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let f32_at = |offset| f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let header = AfHeader {
        magic: u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        rotation: [f32_at(12), f32_at(16), f32_at(20), f32_at(8)],
        translation: [f32_at(24), f32_at(28), f32_at(32)],
        flags: bytes[36..40].try_into().unwrap(),
        version: u16_at(40),
        bone_count: u16_at(42),
        frame_count: u16_at(44),
        index_atlas_count: u16_at(46),
        fill_count: u16_at(48),
        preamble_offset: u16_at(50),
    };
    if header.version != 5 {
        return Err(format!(
            "unsupported Starfield AF version {}",
            header.version
        ));
    }
    if header
        .rotation
        .iter()
        .chain(&header.translation)
        .any(|v| !v.is_finite())
    {
        return Err("Starfield AF header contains nonfinite transforms".into());
    }
    Ok(header)
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std::ffi::{CStr, c_char, c_void};
    use std::os::windows::ffi::OsStrExt;

    type Ptr = *mut c_void;
    type Indexed = unsafe extern "C" fn(Ptr, i32, Ptr) -> Ptr;
    type Count = unsafe extern "C" fn(Ptr) -> usize;
    type Name = unsafe extern "C" fn(Ptr) -> *const c_char;
    type Value = unsafe extern "C" fn(Ptr) -> Ptr;
    type Frame = unsafe extern "C" fn(Ptr) -> u16;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryExW(path: *const u16, file: Ptr, flags: u32) -> Ptr;
        fn GetProcAddress(module: Ptr, name: *const c_char) -> Ptr;
        fn FreeLibrary(module: Ptr) -> i32;
    }

    struct Library(Ptr);

    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }

    macro_rules! api {
        ($($field:ident: $ty:ty = $symbol:literal),* $(,)?) => {
            struct Api { $($field: $ty,)* }
            impl Api {
                unsafe fn load(library: &Library) -> Result<Self, String> {
                    Ok(Self { $($field: {
                        let ptr = unsafe { GetProcAddress(library.0, concat!($symbol, "\0").as_ptr().cast()) };
                        if ptr.is_null() {
                            return Err(format!("CALUMI DLL is missing {}", $symbol));
                        }
                        unsafe { std::mem::transmute::<Ptr, $ty>(ptr) }
                    },)* })
                }
            }
        };
    }

    api! {
        load_scene: unsafe extern "C" fn(*const *const u16, i32, Ptr) -> Ptr = "LoadAnimationSceneFromSFBGSFormatC",
        delete_scene: unsafe extern "C" fn(Ptr) -> bool = "DeleteAnimationSceneC",
        animation_count: Count = "GetAnimationCountC",
        animation: Indexed = "GetAnimationC",
        animation_title: Name = "GetAnimationTitleC",
        block_count: Count = "GetAnimationBlockCountC",
        block: Indexed = "GetAnimationBlockC",
        last_frame: Count = "GetFrameCountC",
        bone_name: Name = "GetAnimBlockBoneNameC",
        bone_index: unsafe extern "C" fn(Ptr) -> i32 = "GetAnimBlockBoneIndexC",
        rotation_count: Count = "GetRotationSqSizeC",
        translation_count: Count = "GetTranslationSqSizeC",
        scalar_count: Count = "GetScalarSqSizeC",
        priority_count: Count = "GetPrioritySqSizeC",
        rotation: Indexed = "GetRotationFromSqC",
        translation: Indexed = "GetTranslationFromSqC",
        scalar: Indexed = "GetScalarFromSqC",
        priority: Indexed = "GetPriorityFromSqC",
        rotation_frame: Frame = "GetFrameFromRotationEntryC",
        translation_frame: Frame = "GetFrameFromTranslationEntryC",
        scalar_frame: Frame = "GetFrameFromScalarEntryC",
        priority_frame: Frame = "GetFrameFromPriorityEntryC",
        rotation_value: Value = "GetValueFromRotationEntryC",
        translation_value: Value = "GetValueFromTranslationEntryC",
        scalar_value: unsafe extern "C" fn(Ptr) -> f32 = "GetValueFromScalarEntryC",
        priority_value: unsafe extern "C" fn(Ptr) -> u8 = "GetValueFromPriorityEntryC",
        qx: unsafe extern "C" fn(Ptr) -> f32 = "GetQuaternionX",
        qy: unsafe extern "C" fn(Ptr) -> f32 = "GetQuaternionY",
        qz: unsafe extern "C" fn(Ptr) -> f32 = "GetQuaternionZ",
        qw: unsafe extern "C" fn(Ptr) -> f32 = "GetQuaternionW",
        tx: unsafe extern "C" fn(Ptr) -> f64 = "GetVector3DX",
        ty: unsafe extern "C" fn(Ptr) -> f64 = "GetVector3DY",
        tz: unsafe extern "C" fn(Ptr) -> f64 = "GetVector3DZ",
        rig: Value = "GetSkeletonRigC",
        rig_bone_count: Count = "GetSkeletonRigBoneCountC",
        rig_bone: Indexed = "GetSkeletonBoneC",
        rig_bone_name: Name = "GetSkeletonBoneNameC",
        rig_bone_parent: unsafe extern "C" fn(Ptr) -> i32 = "GetSkeletonBoneParentIndexC",
        rig_bone_rotation: unsafe extern "C" fn(Ptr, bool) -> Ptr = "GetSkeletonBoneRotationC",
        rig_bone_position: unsafe extern "C" fn(Ptr, bool) -> Ptr = "GetSkeletonBonePositionC",
        px: unsafe extern "C" fn(Ptr) -> f32 = "GetVector3X",
        py: unsafe extern "C" fn(Ptr) -> f32 = "GetVector3Y",
        pz: unsafe extern "C" fn(Ptr) -> f32 = "GetVector3Z",
    }

    pub(crate) struct CalumiDecoder {
        api: Api,
        _library: Library,
    }

    struct Scene<'a> {
        ptr: Ptr,
        api: &'a Api,
    }

    impl Drop for Scene<'_> {
        fn drop(&mut self) {
            unsafe {
                (self.api.delete_scene)(self.ptr);
            }
        }
    }

    fn nonnull(ptr: Ptr, label: &str) -> Result<Ptr, String> {
        if ptr.is_null() {
            Err(format!("CALUMI returned null {label}"))
        } else {
            Ok(ptr)
        }
    }

    unsafe fn name(ptr: Ptr, get: Name) -> Result<String, String> {
        let chars = unsafe { get(ptr) };
        if chars.is_null() {
            return Err("CALUMI returned a null name".into());
        }
        unsafe { CStr::from_ptr(chars) }
            .to_str()
            .map(str::to_owned)
            .map_err(|e| e.to_string())
    }

    fn wide(path: &Path) -> Result<Vec<u16>, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut units: Vec<_> = path.as_os_str().encode_wide().collect();
        if units.contains(&0) {
            return Err("path contains NUL".into());
        }
        units.push(0);
        Ok(units)
    }

    impl CalumiDecoder {
        pub(crate) fn open(dll_path: &Path) -> Result<Self, String> {
            let wide = wide(dll_path)?;
            // Restrict dependency resolution to the requested DLL directory and
            // system/application defaults, excluding the current working directory.
            let ptr =
                unsafe { LoadLibraryExW(wide.as_ptr(), std::ptr::null_mut(), 0x100 | 0x1000) };
            let library = Library(nonnull(
                ptr,
                &format!(
                    "DLL {} ({})",
                    dll_path.display(),
                    std::io::Error::last_os_error()
                ),
            )?);
            let api = unsafe { Api::load(&library)? };
            Ok(Self {
                api,
                _library: library,
            })
        }

        pub(crate) fn decode(
            &self,
            rig_path: &Path,
            af_path: &Path,
        ) -> Result<DecodedStarfieldAnimation, String> {
            let af_bytes =
                std::fs::read(af_path).map_err(|e| format!("{}: {e}", af_path.display()))?;
            let header = af_header(&af_bytes)?;
            let weapon_suffix_node_hashes = suffix::inspect_suffix(&af_bytes)?.hashes;
            let rig_bytes =
                std::fs::read(rig_path).map_err(|e| format!("{}: {e}", rig_path.display()))?;
            if rig_bytes.len() < 80 || rig_bytes[..4] != 5u32.to_le_bytes() {
                return Err("Starfield RIG requires a complete version 5 header".into());
            }
            let count = u16::from_le_bytes(rig_bytes[56..58].try_into().unwrap()) as usize;
            if count == 0 || count > 512 || rig_bytes.len() < 80 + count * 96 {
                return Err(
                    "Starfield RIG bone array is empty, truncated or exceeds CALUMI's limit".into(),
                );
            }
            if usize::from(header.bone_count) > count {
                return Err("Starfield AF actor bone count exceeds its source RIG".into());
            }
            let rig_wide = wide(rig_path)?;
            let af_wide = wide(af_path)?;
            let paths = [rig_wide.as_ptr(), af_wide.as_ptr()];
            unsafe {
                let api = &self.api;
                let ptr = nonnull(
                    (api.load_scene)(paths.as_ptr(), 2, std::ptr::null_mut()),
                    "animation scene",
                )?;
                let scene = Scene { ptr, api };
                if (api.animation_count)(scene.ptr) != 1 {
                    return Err("CALUMI did not return exactly one animation".into());
                }
                let rig = nonnull((api.rig)(scene.ptr), "skeleton rig")?;
                if (api.rig_bone_count)(rig) != count {
                    return Err("CALUMI rig count differs from source header".into());
                }
                let mut bones = Vec::with_capacity(count);
                for index in 0..count {
                    let bone = nonnull(
                        (api.rig_bone)(rig, index as i32, std::ptr::null_mut()),
                        "rig bone",
                    )?;
                    let rotation = nonnull((api.rig_bone_rotation)(bone, false), "rig rotation")?;
                    let position = nonnull((api.rig_bone_position)(bone, false), "rig position")?;
                    bones.push(StarfieldBone {
                        name: name(bone, api.rig_bone_name)?,
                        parent: (api.rig_bone_parent)(bone),
                        rotation: self.quaternion(rotation),
                        translation: [
                            (api.px)(position) as f64,
                            (api.py)(position) as f64,
                            (api.pz)(position) as f64,
                        ],
                    });
                }
                let animation = nonnull(
                    (api.animation)(scene.ptr, 0, std::ptr::null_mut()),
                    "animation",
                )?;
                let block_count = (api.block_count)(animation);
                if block_count > count {
                    return Err("CALUMI animation has more tracks than rig bones".into());
                }
                let mut tracks = Vec::with_capacity(block_count);
                for index in 0..block_count {
                    let block = nonnull(
                        (api.block)(animation, index as i32, std::ptr::null_mut()),
                        "animation block",
                    )?;
                    let bone_index = usize::try_from((api.bone_index)(block))
                        .map_err(|_| "CALUMI returned a negative bone index")?;
                    let bone_name = name(block, api.bone_name)?;
                    if bones.get(bone_index).map(|b| &b.name) != Some(&bone_name) {
                        return Err(format!(
                            "CALUMI track {bone_name} has an inconsistent rig index"
                        ));
                    }
                    tracks.push(StarfieldBoneTrack {
                        bone_index,
                        bone_name,
                        rotations: self.keys(
                            block,
                            api.rotation_count,
                            api.rotation,
                            api.rotation_frame,
                            |entry| {
                                Ok(self.quaternion(nonnull(
                                    (api.rotation_value)(entry),
                                    "quaternion",
                                )?))
                            },
                        )?,
                        translations: self.keys(
                            block,
                            api.translation_count,
                            api.translation,
                            api.translation_frame,
                            |entry| {
                                let p = nonnull((api.translation_value)(entry), "translation")?;
                                Ok([(api.tx)(p), (api.ty)(p), (api.tz)(p)])
                            },
                        )?,
                        scalars: self.keys(
                            block,
                            api.scalar_count,
                            api.scalar,
                            api.scalar_frame,
                            |e| Ok((api.scalar_value)(e)),
                        )?,
                        priorities: self.keys(
                            block,
                            api.priority_count,
                            api.priority,
                            api.priority_frame,
                            |e| Ok((api.priority_value)(e)),
                        )?,
                    });
                }
                let result = DecodedStarfieldAnimation {
                    title: name(animation, api.animation_title)?,
                    header,
                    last_key_frame: u16::try_from((api.last_frame)(animation))
                        .map_err(|_| "CALUMI frame index exceeds AF range")?,
                    bones,
                    tracks,
                    weapon_suffix_node_hashes,
                };
                validate_decoded(&result)?;
                Ok(result)
            }
        }

        pub(crate) fn decode_weapon_suffixes(
            &self,
            rig_path: &Path,
            af_path: &Path,
            model_node_names: &[String],
        ) -> Result<DecodedWeaponSuffixAnimation, String> {
            let source = std::fs::read(af_path).map_err(|error| error.to_string())?;
            let view = suffix::inspect_suffix(&source)?;
            if view.hashes.is_empty() {
                return Err("Starfield animation has no weapon suffix channels".into());
            }
            let nodes = suffix::resolve_nodes(&view.hashes, model_node_names)?;
            let rig = suffix::rig_view(
                &std::fs::read(rig_path).map_err(|error| error.to_string())?,
                &nodes,
            )?;
            let scratch = tempfile::tempdir().map_err(|error| error.to_string())?;
            let rig_view = scratch.path().join("WeaponSuffix.rig");
            let af_view = scratch.path().join("WeaponSuffix.af");
            std::fs::write(&rig_view, rig).map_err(|error| error.to_string())?;
            std::fs::write(&af_view, view.animation_view()?).map_err(|error| error.to_string())?;
            let animation = self.decode(&rig_view, &af_view)?;
            Ok(DecodedWeaponSuffixAnimation {
                source_header: view.source_header,
                nodes,
                animation,
            })
        }

        unsafe fn quaternion(&self, ptr: Ptr) -> [f64; 4] {
            unsafe {
                [
                    (self.api.qx)(ptr) as f64,
                    (self.api.qy)(ptr) as f64,
                    (self.api.qz)(ptr) as f64,
                    (self.api.qw)(ptr) as f64,
                ]
            }
        }

        unsafe fn keys<T>(
            &self,
            block: Ptr,
            count: Count,
            get: Indexed,
            frame: Frame,
            value: impl Fn(Ptr) -> Result<T, String>,
        ) -> Result<Vec<StarfieldKey<T>>, String> {
            let count = unsafe { count(block) };
            if count > u16::MAX as usize + 1 {
                return Err("CALUMI key count exceeds AF frame range".into());
            }
            (0..count)
                .map(|i| {
                    let entry = nonnull(
                        unsafe { get(block, i as i32, std::ptr::null_mut()) },
                        "keyframe",
                    )?;
                    Ok(StarfieldKey {
                        frame: unsafe { frame(entry) },
                        value: value(entry)?,
                    })
                })
                .collect()
        }
    }
}

fn validate_decoded(animation: &DecodedStarfieldAnimation) -> Result<(), String> {
    let finite = |values: &[f64]| values.iter().all(|v| v.is_finite());
    for (index, bone) in animation.bones.iter().enumerate() {
        if bone.parent < -1
            || bone.parent >= index as i32
            || !finite(&bone.rotation)
            || !finite(&bone.translation)
        {
            return Err(format!("invalid Starfield rig bone {}", bone.name));
        }
    }
    for track in &animation.tracks {
        if track.rotations.iter().any(|k| !finite(&k.value))
            || track.translations.iter().any(|k| !finite(&k.value))
            || track.scalars.iter().any(|k| !k.value.is_finite())
        {
            return Err(format!("nonfinite Starfield key in {}", track.bone_name));
        }
        for frames in [
            track.rotations.iter().map(|k| k.frame).collect::<Vec<_>>(),
            track.translations.iter().map(|k| k.frame).collect(),
            track.scalars.iter().map(|k| k.frame).collect(),
            track.priorities.iter().map(|k| k.frame).collect(),
        ] {
            if frames.windows(2).any(|p| p[0] >= p[1])
                || frames
                    .last()
                    .is_some_and(|f| *f > animation.header.frame_count)
            {
                return Err(format!(
                    "invalid Starfield key frame order/range in {}",
                    track.bone_name
                ));
            }
        }
    }
    Ok(())
}

#[cfg(windows)]
pub(super) use windows::CalumiDecoder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_header_preserves_duration_and_xyzw() {
        let mut bytes = [0u8; 64];
        bytes[8..12].copy_from_slice(&1f32.to_le_bytes());
        bytes[24..28].copy_from_slice(&2f32.to_le_bytes());
        bytes[40..42].copy_from_slice(&5u16.to_le_bytes());
        bytes[44..46].copy_from_slice(&102u16.to_le_bytes());
        let header = af_header(&bytes).unwrap();
        assert_eq!(header.rotation, [0., 0., 0., 1.]);
        assert_eq!(header.translation, [2., 0., 0.]);
        assert_eq!(header.frame_count, 102);
        assert!(af_header(&bytes[..63]).is_err());
        bytes[40] = 6;
        assert!(af_header(&bytes).is_err());
    }

    #[test]
    fn malformed_decoded_keys_are_rejected() {
        let mut bytes = [0u8; 64];
        bytes[40] = 5;
        bytes[44] = 2;
        let mut animation = DecodedStarfieldAnimation {
            title: "test".into(),
            header: af_header(&bytes).unwrap(),
            last_key_frame: 2,
            bones: vec![StarfieldBone {
                name: "Root".into(),
                parent: -1,
                rotation: [0., 0., 0., 1.],
                translation: [0.; 3],
            }],
            tracks: vec![StarfieldBoneTrack {
                bone_index: 0,
                bone_name: "Root".into(),
                rotations: vec![],
                translations: vec![],
                scalars: vec![],
                priorities: vec![
                    StarfieldKey {
                        frame: 2,
                        value: 55,
                    },
                    StarfieldKey {
                        frame: 1,
                        value: 26,
                    },
                ],
            }],
            weapon_suffix_node_hashes: vec![],
        };
        assert!(validate_decoded(&animation).is_err());
        animation.tracks[0].priorities.reverse();
        assert!(validate_decoded(&animation).is_ok());
        animation.tracks[0].scalars.push(StarfieldKey {
            frame: 1,
            value: f32::NAN,
        });
        assert!(validate_decoded(&animation).is_err());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires extracted Eon source assets and the existing CALUMI DLL"]
    fn real_eon_suffix_channels_are_separate_from_actor_channels() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let decoder =
            CalumiDecoder::open(&root.join("refs/sf_animation_io/CALUMI.Animation.dll")).unwrap();
        let meshes = root.join("extracted/starfield/Meshes");
        let names: Vec<String> = [
            "Magazine",
            "Magazine01",
            "TRIGGER",
            "MiscB01",
            "MiscB02",
            "MiscB03",
            "MiscB04",
            "MiscB05",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for (pov, action, actor_tracks, suffix_tracks, last_frame) in [
            ("_1stperson/", "firesingle", 62, 0, 25),
            ("_1stperson/", "reload", 68, 4, 101),
            ("_1stperson/", "gun_firesingle", 0, 0, 26),
            ("", "player/reload", 64, 4, 101),
        ] {
            let rig = meshes.join(format!("actors/human/{pov}characterassets/skeleton.rig"));
            let af = meshes.join(format!(
                "actors/human/{pov}animations/gun/grippistol/stinger/{action}.af"
            ));
            let actor = decoder.decode(&rig, &af).unwrap();
            let suffix = decoder.decode_weapon_suffixes(&rig, &af, &names).unwrap();
            assert_eq!(actor.tracks.len(), actor_tracks);
            assert_eq!(suffix.animation.tracks.len(), suffix_tracks);
            assert_eq!(suffix.nodes.len(), 8);
            assert_eq!(suffix.source_header.frame_count, last_frame);
            assert_eq!(suffix.animation.header.frame_count, last_frame);
            assert!(
                !actor
                    .tracks
                    .iter()
                    .any(|track| names.contains(&track.bone_name))
            );
            if suffix_tracks != 0 {
                let magazine = suffix
                    .animation
                    .tracks
                    .iter()
                    .find(|track| track.bone_name == "Magazine")
                    .unwrap();
                assert!(
                    magazine
                        .translations
                        .windows(2)
                        .any(|keys| keys[0].value != keys[1].value)
                );
                assert!(
                    magazine
                        .rotations
                        .windows(2)
                        .any(|keys| keys[0].value != keys[1].value)
                );
                assert!(
                    !suffix
                        .animation
                        .tracks
                        .iter()
                        .any(|track| track.bone_name == "TRIGGER")
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires extracted Starfield source assets and the existing CALUMI DLL"]
    fn real_stinger_clips_decode_source_transforms() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
        let decoder =
            CalumiDecoder::open(&root.join("refs/sf_animation_io/CALUMI.Animation.dll")).unwrap();
        let meshes = root.join("extracted/starfield/Meshes");
        for (pov, action, expected_blocks, expected_last, expected_suffixes) in [
            ("", "reload", 64, 101, 9),
            ("_1stperson/", "firesingle", 62, 25, 8),
            ("_1stperson/", "reload", 68, 101, 8),
        ] {
            let rig = meshes.join(format!("actors/human/{pov}characterassets/skeleton.rig"));
            let af = meshes.join(format!(
                "actors/human/{pov}animations/gun/grippistol/stinger/{action}.af"
            ));
            let animation = decoder.decode(&rig, &af).unwrap();
            assert_eq!(animation.tracks.len(), expected_blocks);
            assert_eq!(animation.last_key_frame, expected_last);
            assert_eq!(animation.weapon_suffix_node_hashes.len(), expected_suffixes);
            assert!(animation.bones.iter().any(|b| b.name == "Weapon"));
            assert!(
                animation
                    .tracks
                    .iter()
                    .any(|t| t.rotations.windows(2).any(|p| p[0].value != p[1].value))
            );
            assert!(
                animation
                    .tracks
                    .iter()
                    .any(|t| t.translations.windows(2).any(|p| p[0].value != p[1].value))
            );
            println!(
                "{pov}{action}: {} rig bones, {} tracks, header frames {}, last key {}",
                animation.bones.len(),
                animation.tracks.len(),
                animation.header.frame_count,
                animation.last_key_frame
            );
            if pov == "_1stperson/" {
                for name in ["C_Chest", "R_Biceps", "R_Wrist", "Weapon"] {
                    let bone = animation
                        .bones
                        .iter()
                        .find(|bone| bone.name == name)
                        .unwrap();
                    let track = animation
                        .tracks
                        .iter()
                        .find(|track| track.bone_name == name)
                        .unwrap();
                    println!(
                        "{name}: bind q{:?} t{:?}; first q{:?} t{:?}",
                        bone.rotation,
                        bone.translation,
                        track.rotations.first().map(|key| key.value),
                        track.translations.first().map(|key| key.value)
                    );
                }
            }
        }
    }
}
