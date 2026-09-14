// Phase: convert_animations
//
// Params shape (JSON):
// {
//   "animations": [
//     {
//       "source_path":   "Meshes/AnimsHumanFemale/idle.kf",  // relative game path
//       "resolved_path": "/abs/path/idle.kf",                 // absolute disk path
//       "asset_type":    "animation" | "kf_animation",        // optional
//       "output_clip_path": "Actors/B21_FNVGecko/Animations/Idle.hkx",
//       "ordered_source_bone_names": ["Bip01", ...],           // canonical Skeleton.hkx order
//       "ordered_source_float_slot_names": ["..."],             // exact emitted hkaSkeleton floatSlots order
//       "source_skeleton_path": "/abs/path/skeleton.nif",       // alternative to inline order
//       "runtime_skeleton_path": "Actors/.../Skeleton.hkx",
//       "original_skeleton_name": "NVGecko",                   // hkaAnimationBinding metadata
//       "target_sample_rate_hz": 30.0,                          // optional; defaults to 30 Hz
//       "sequence_index": 0,                                    // required when a KF has multiple sequences
//       "extracted_motion_policy": "extract_planar_reference_frame" // required for moving clips
//     },
//     ...
//   ],
//   "source_extracted": "/abs/path/to/source/extracted",
//   "weapon_family":    "AssaultRifle",     // optional, single-weapon override
//   "event_map":        { "ATTACK_HIT": "weaponFire" },  // optional inline event remap
//   "asset_prefix":     "fnv",             // accepted for compatibility; output is unprefixed
//   "target_behaviors": ["actors/...", ...],  // base-game paths to skip
//   "overwrite_existing": true
// }
//
// Phase output: writes .hkx files to mod_path/data/...
// PhaseReport:
//   assets_written  = KF animations successfully converted
//   records_dropped = base-game-skipped count
//   warnings        = failed conversions
//
// .kf files are NIF binaries rooted at NiControllerSequence. Each clip's bone,
// event, and float channels are extracted, events remapped via event_map, then
// written as hkaInterleavedUncompressedAnimation XML and packed by havok_native.
//
// weapon_family_table.yaml is embedded for callers that need overlay synthesis.
// Weapon overlay synthesis itself stays in the Python orchestrator because it
// needs per-weapon EditorIDs from the translated WEAP records.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nif_core_native::model::{NifFile, NifValue};
use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::source_rig::{ClipBinding, ClipDecl};

// ---------------------------------------------------------------------------
// Embedded weapon-family table
// ---------------------------------------------------------------------------

const WEAPON_FAMILY_YAML: &str = include_str!("animations/weapon_family_table.yaml");

// ---------------------------------------------------------------------------
// Public phase struct
// ---------------------------------------------------------------------------

pub struct ConvertAnimationsPhase;

impl Phase for ConvertAnimationsPhase {
    fn name(&self) -> &'static str {
        "convert_animations"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let p = ctx.params;

        let _asset_prefix = p
            .get("asset_prefix")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let overwrite_existing = p
            .get("overwrite_existing")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let target_behaviors: HashSet<String> = p
            .get("target_behaviors")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.replace('\\', "/").to_lowercase())
                    .collect()
            })
            .unwrap_or_default();

        // Optional inline event map (caller-supplied overrides).
        let inline_event_map: HashMap<String, String> = p
            .get("event_map")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let animations = parse_animation_assets(p)?;
        let total = animations.len() as u32;
        let mod_path = ctx.mod_path.to_path_buf();
        let sink = ctx.run.output_sink.clone();
        let data_root = mod_path.join("data");
        let register_with_sink = |dst: &Path| -> bool {
            let Some(s) = &sink else { return true };
            let Ok(rel) = dst.strip_prefix(&data_root) else {
                return true;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            s.add_existing_file(&rel_str, dst).is_ok()
        };

        if animations.is_empty() {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Progress {
                phase: "convert_animations",
                current: 0,
                total: 0,
                item: None,
            });
            return Ok(PhaseReport::default());
        }

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_animations",
            level: LogLevel::Info,
            message: format!("Animation phase: {} KF asset(s) to process", total),
        });

        let mut assets_written: u32 = 0;
        let mut records_dropped: u32 = 0;
        let mut warnings: u32 = 0;
        let mut sink_failures: u32 = 0;

        let reporter = Arc::new(ProgressReporter::new(
            "convert_animations",
            total,
            ctx.run.event_tx.clone(),
        ));

        for asset in animations.iter() {
            ctx.check_cancel()?;

            reporter.set_item(asset.source_path.clone());
            reporter.inc(1);

            // Base-game dedup check.
            let norm = asset.source_path.replace('\\', "/").to_lowercase();
            let db_key = norm.strip_prefix("meshes/").unwrap_or(&norm).to_string();
            if target_behaviors.contains(&db_key) || target_behaviors.contains(&norm) {
                records_dropped += 1;
                continue;
            }

            // Source file must exist.
            let resolved = Path::new(&asset.resolved_path);
            if !resolved.exists() {
                let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                    phase: "convert_animations",
                    level: LogLevel::Error,
                    message: format!("KF not found: {}", asset.source_path),
                });
                warnings += 1;
                continue;
            }

            // Compute output path.
            let out_path = output_clip_deploy_path(&mod_path, &asset.output_clip_path);

            if !overwrite_existing && out_path.exists() {
                if !register_with_sink(&out_path) {
                    sink_failures += 1;
                }
                continue;
            }

            match convert_kf_to_hkx_with_channels(
                resolved,
                &out_path,
                &inline_event_map,
                &asset.source_path,
                &asset.output_clip_path,
                asset.ordered_source_bone_names.as_deref(),
                asset.ordered_source_float_slot_names.as_deref(),
                asset.source_skeleton_path.as_deref().map(Path::new),
                asset.runtime_skeleton_path.as_deref(),
                asset.original_skeleton_name.as_deref(),
                asset.target_sample_rate_hz,
                asset.extracted_motion_policy,
                asset.sequence_index,
            ) {
                Ok(outcome) => {
                    assets_written += 1;
                    if !register_with_sink(&out_path) {
                        sink_failures += 1;
                    }
                    for w in &outcome.warnings {
                        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                            phase: "convert_animations",
                            level: LogLevel::Warn,
                            message: format!("[KF parse] {w}"),
                        });
                    }
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "convert_animations",
                        level: LogLevel::Info,
                        message: format!(
                            "[KF->HKX] {} -> {} ({})",
                            asset.source_path,
                            outcome.output_clip_path,
                            out_path.display()
                        ),
                    });
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "convert_animations",
                        level: LogLevel::Info,
                        message: format!(
                            "[KF motion policy] {}: {}",
                            asset.source_path,
                            outcome.motion.phase_report()
                        ),
                    });
                }
                Err(e) => {
                    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                        phase: "convert_animations",
                        level: LogLevel::Error,
                        message: format!("KF->HKX failed: {}: {e}", asset.source_path),
                    });
                    warnings += 1;
                }
            }
        }

        reporter.finish();

        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "convert_animations",
            level: LogLevel::Info,
            message: format!(
                "Animation phase complete: written={assets_written}, skipped={records_dropped}, \
                 failed={warnings}, total={total}"
            ),
        });

        Ok(PhaseReport {
            assets_written,
            records_dropped,
            warnings,
            items_failed: warnings + sink_failures,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// KF → HKX conversion pipeline
// ---------------------------------------------------------------------------

const DEFAULT_TARGET_SAMPLE_RATE_HZ: f64 = 30.0;

#[derive(Debug, thiserror::Error)]
pub(crate) enum FnvKfConversionError {
    #[error("invalid output_clip_path {path:?}: {reason}")]
    InvalidOutputClipPath { path: String, reason: String },
    #[error("missing ordered_source_bone_names for KF animation")]
    MissingOrderedSourceBoneNames,
    #[error(
        "KF animation supplied both ordered_source_bone_names and source_skeleton_path; choose one canonical skeleton source"
    )]
    AmbiguousSourceSkeleton,
    #[error("source_skeleton_path for KF animation is empty")]
    EmptySourceSkeletonPath,
    #[error("failed to read source skeleton {path}: {source}")]
    ReadSourceSkeleton {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse source skeleton {path}: {message}")]
    ParseSourceSkeleton { path: PathBuf, message: String },
    #[error("source skeleton {path} contains no named NiNode bones")]
    SourceSkeletonHasNoNamedNodes { path: PathBuf },
    #[error("missing runtime_skeleton_path for KF animation")]
    MissingRuntimeSkeletonPath,
    #[error("runtime_skeleton_path for KF animation is empty")]
    EmptyRuntimeSkeletonPath,
    #[error("missing original_skeleton_name for KF animation")]
    MissingOriginalSkeletonName,
    #[error("original_skeleton_name for KF animation is empty")]
    EmptyOriginalSkeletonName,
    #[error("invalid target_sample_rate_hz {0}; expected a finite positive value")]
    InvalidTargetSampleRate(f64),
    #[error(transparent)]
    FloatBinding(#[from] FnvKfFloatBindingError),
    #[error(transparent)]
    Binding(#[from] FnvKfBindingError),
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse {path}: {message}")]
    Parse { path: String, message: String },
    #[error(
        "KF {source_path} contains {count} NiControllerSequence blocks; select one explicitly: {names:?}"
    )]
    MultipleSequences {
        source_path: String,
        count: usize,
        names: Vec<String>,
    },
    #[error("KF {source_path} sequence index {index} is out of range (sequence count {count})")]
    SequenceIndexOutOfRange {
        source_path: String,
        index: usize,
        count: usize,
    },
    #[error("failed to decode compact spline track {bone_name:?} in {path}: {source}")]
    CompactSplineDecode {
        path: String,
        bone_name: String,
        #[source]
        source: CompactSplineError,
    },
    #[error("failed to decode {interpolator_type} channel {target_name:?} in {path}: {source}")]
    ChannelDecode {
        path: String,
        target_name: String,
        interpolator_type: String,
        #[source]
        source: ChannelDecodeError,
    },
    #[error(
        "KF binding declares {declared} transform tracks with {mapped} mappings for {channels} parsed channels"
    )]
    InvalidBindingTrackCount {
        declared: usize,
        mapped: usize,
        channels: usize,
    },
    #[error("KF binding maps {mapped} float tracks for {channels} parsed float channels")]
    InvalidFloatBindingTrackCount { mapped: usize, channels: usize },
    #[error("KF float track {track} sample at frame {frame} is absent or non-finite")]
    InvalidFloatSample { track: usize, frame: usize },
    #[error("KF accumulation root {name:?} has no bound transform track")]
    MissingAccumRootTrack { name: String },
    #[error(
        "KF accumulation root track {track} ({name:?}) has animated vertical displacement {max_delta}; vertical extracted motion is unsupported"
    )]
    UnsupportedVerticalRootMotion {
        track: usize,
        name: String,
        max_delta: f64,
    },
    #[error(
        "KF accumulation root track {track} ({name:?}) has animated pitch/roll (max pitch {max_pitch}, max roll {max_roll}); only translation plus yaw extracted motion is supported"
    )]
    UnsupportedPitchRollRootMotion {
        track: usize,
        name: String,
        max_pitch: f64,
        max_roll: f64,
    },
    #[error("failed to load hk2014 descriptor {class}: {message}")]
    DescriptorLookup { class: String, message: String },
    #[error("bundled hk2014 classxml has no descriptor for {class}")]
    MissingDescriptor { class: String },
    #[error("bundled hk2014 descriptor {class} is missing required member {member}")]
    MissingDescriptorMember { class: String, member: String },
    #[error(
        "KF clip contains nonzero bound planar translation/yaw root motion but extracted_motion_policy is not extract_planar_reference_frame"
    )]
    ExtractPlanarReferenceFramePolicyRequired,
    #[error("failed to create output directory {path}: {source}")]
    CreateOutputDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to pack FO4 HKX: {0}")]
    Pack(#[source] havok_native::error::HavokError),
    #[error("failed to reread staged FO4 HKX: {0}")]
    ValidatePacked(#[source] havok_native::error::HavokError),
    #[error("staged FO4 HKX has an invalid target header: {detail}")]
    InvalidPackedTargetHeader { detail: String },
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FnvClipMotion {
    InPlace,
    ExtractedPlanar {
        root_track_index: usize,
        root_bone_index: usize,
        sample_count: usize,
        end_displacement: [f64; 2],
    },
    ExtractedPlanarYaw {
        root_track_index: usize,
        root_bone_index: usize,
        sample_count: usize,
        end_displacement: [f64; 2],
        end_yaw_radians: f64,
    },
}

impl FnvClipMotion {
    fn phase_report(&self) -> String {
        match self {
            Self::InPlace => "in_place; extractedMotion=#null".to_string(),
            Self::ExtractedPlanar {
                root_track_index,
                root_bone_index,
                sample_count,
                end_displacement,
            } => format!(
                "extracted_planar; policy=extract_planar_reference_frame; root_track={root_track_index}; root_bone={root_bone_index}; samples={sample_count}; end=({:.6},{:.6})",
                end_displacement[0], end_displacement[1]
            ),
            Self::ExtractedPlanarYaw {
                root_track_index,
                root_bone_index,
                sample_count,
                end_displacement,
                end_yaw_radians,
            } => format!(
                "extracted_planar_yaw; policy=extract_planar_reference_frame; root_track={root_track_index}; root_bone={root_bone_index}; samples={sample_count}; end=({:.6},{:.6}); end_yaw={end_yaw_radians:.6}",
                end_displacement[0], end_displacement[1]
            ),
        }
    }
}

#[derive(Debug)]
pub(crate) struct FnvKfConversionOutcome {
    pub(crate) warnings: Vec<String>,
    pub(crate) motion: FnvClipMotion,
    pub(crate) output_clip_path: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FnvKfStageRequest<'a> {
    pub(crate) source_kf: &'a str,
    pub(crate) output_clip_path: &'a str,
    pub(crate) sequence_index: Option<usize>,
    pub(crate) skeleton: FnvKfSkeletonContract<'a>,
    pub(crate) original_skeleton_name: &'a str,
    pub(crate) event_map: &'a HashMap<String, String>,
    pub(crate) target_sample_rate_hz: Option<f64>,
    pub(crate) extracted_motion_policy: FnvExtractedMotionPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FnvKfArtifactReceipt {
    pub(crate) source_kf: String,
    pub(crate) sequence_index: usize,
    pub(crate) sequence_name: String,
    pub(crate) output_clip_path: String,
    pub(crate) runtime_skeleton_path: String,
    pub(crate) original_skeleton_name: String,
    pub(crate) transform_track_count: usize,
    pub(crate) float_track_count: usize,
    pub(crate) sample_count: usize,
    pub(crate) event_count: usize,
    pub(crate) events: Vec<FnvKfTextKeyEvidence>,
    pub(crate) warnings: Vec<String>,
    pub(crate) motion: FnvClipMotion,
    pub(crate) binding: ClipBinding,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StagedFnvKfArtifact {
    pub(crate) hkx_bytes: Vec<u8>,
    pub(crate) receipt: FnvKfArtifactReceipt,
}

pub(crate) fn convert_kf_to_hkx(
    kf_path: &Path,
    out_path: &Path,
    event_map: &HashMap<String, String>,
    source_rel: &str,
    output_clip_path: &str,
    ordered_source_bone_names: Option<&[String]>,
    source_skeleton_path: Option<&Path>,
    runtime_skeleton_path: Option<&str>,
    original_skeleton_name: Option<&str>,
    target_sample_rate_hz: Option<f64>,
    extracted_motion_policy: FnvExtractedMotionPolicy,
) -> Result<FnvKfConversionOutcome, FnvKfConversionError> {
    convert_kf_to_hkx_with_channels(
        kf_path,
        out_path,
        event_map,
        source_rel,
        output_clip_path,
        ordered_source_bone_names,
        None,
        source_skeleton_path,
        runtime_skeleton_path,
        original_skeleton_name,
        target_sample_rate_hz,
        extracted_motion_policy,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn convert_kf_to_hkx_with_channels(
    kf_path: &Path,
    out_path: &Path,
    event_map: &HashMap<String, String>,
    source_rel: &str,
    output_clip_path: &str,
    ordered_source_bone_names: Option<&[String]>,
    ordered_source_float_slot_names: Option<&[String]>,
    source_skeleton_path: Option<&Path>,
    runtime_skeleton_path: Option<&str>,
    original_skeleton_name: Option<&str>,
    target_sample_rate_hz: Option<f64>,
    extracted_motion_policy: FnvExtractedMotionPolicy,
    sequence_index: Option<usize>,
) -> Result<FnvKfConversionOutcome, FnvKfConversionError> {
    if ordered_source_bone_names.is_some() && source_skeleton_path.is_some() {
        return Err(FnvKfConversionError::AmbiguousSourceSkeleton);
    }
    if matches!(source_skeleton_path, Some(path) if path.as_os_str().is_empty()) {
        return Err(FnvKfConversionError::EmptySourceSkeletonPath);
    }
    let loaded_source_bone_names = source_skeleton_path
        .map(load_ordered_source_skeleton_names)
        .transpose()?;
    let ordered_source_bone_names = ordered_source_bone_names
        .or(loaded_source_bone_names.as_deref())
        .ok_or(FnvKfConversionError::MissingOrderedSourceBoneNames)?;
    let runtime_skeleton_path =
        runtime_skeleton_path.ok_or(FnvKfConversionError::MissingRuntimeSkeletonPath)?;
    let original_skeleton_name =
        original_skeleton_name.ok_or(FnvKfConversionError::MissingOriginalSkeletonName)?;
    validate_output_clip_path(output_clip_path).map_err(|reason| {
        FnvKfConversionError::InvalidOutputClipPath {
            path: output_clip_path.to_string(),
            reason,
        }
    })?;
    if runtime_skeleton_path.trim().is_empty() {
        return Err(FnvKfConversionError::EmptyRuntimeSkeletonPath);
    }
    if original_skeleton_name.trim().is_empty() {
        return Err(FnvKfConversionError::EmptyOriginalSkeletonName);
    }
    let checked_sample_rate = target_sample_rate_hz.unwrap_or(DEFAULT_TARGET_SAMPLE_RATE_HZ);
    if !checked_sample_rate.is_finite() || checked_sample_rate <= 0.0 {
        return Err(FnvKfConversionError::InvalidTargetSampleRate(
            checked_sample_rate,
        ));
    }
    let kf_bytes = std::fs::read(kf_path).map_err(|source| FnvKfConversionError::Read {
        path: kf_path.to_path_buf(),
        source,
    })?;
    let staged = stage_fnv_kf_bytes_with_channels(
        &kf_bytes,
        event_map,
        source_rel,
        output_clip_path,
        ordered_source_bone_names,
        ordered_source_float_slot_names,
        runtime_skeleton_path,
        original_skeleton_name,
        target_sample_rate_hz,
        extracted_motion_policy,
        sequence_index,
    )?;

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| {
            FnvKfConversionError::CreateOutputDirectory {
                path: parent.to_path_buf(),
                source,
            }
        })?;
    }
    std::fs::write(out_path, &staged.hkx_bytes).map_err(|source| FnvKfConversionError::Write {
        path: out_path.to_path_buf(),
        source,
    })?;

    Ok(FnvKfConversionOutcome {
        warnings: staged.receipt.warnings,
        motion: staged.receipt.motion,
        output_clip_path: staged.receipt.output_clip_path,
    })
}

pub(crate) fn stage_fnv_creature_kf(
    kf_bytes: &[u8],
    request: FnvKfStageRequest<'_>,
) -> Result<StagedFnvKfArtifact, FnvKfConversionError> {
    stage_fnv_kf_bytes_with_channels(
        kf_bytes,
        request.event_map,
        request.source_kf,
        request.output_clip_path,
        request.skeleton.ordered_bone_names,
        Some(request.skeleton.ordered_float_slot_names),
        request.skeleton.skeleton_path,
        request.original_skeleton_name,
        request.target_sample_rate_hz,
        request.extracted_motion_policy,
        request.sequence_index,
    )
}

#[allow(clippy::too_many_arguments)]
fn stage_fnv_kf_bytes_with_channels(
    kf_bytes: &[u8],
    event_map: &HashMap<String, String>,
    source_rel: &str,
    output_clip_path: &str,
    ordered_source_bone_names: &[String],
    ordered_source_float_slot_names: Option<&[String]>,
    runtime_skeleton_path: &str,
    original_skeleton_name: &str,
    target_sample_rate_hz: Option<f64>,
    extracted_motion_policy: FnvExtractedMotionPolicy,
    sequence_index: Option<usize>,
) -> Result<StagedFnvKfArtifact, FnvKfConversionError> {
    let runtime_clip_path = validate_output_clip_path(output_clip_path).map_err(|reason| {
        FnvKfConversionError::InvalidOutputClipPath {
            path: output_clip_path.to_string(),
            reason,
        }
    })?;
    if runtime_skeleton_path.trim().is_empty() {
        return Err(FnvKfConversionError::EmptyRuntimeSkeletonPath);
    }
    if original_skeleton_name.trim().is_empty() {
        return Err(FnvKfConversionError::EmptyOriginalSkeletonName);
    }
    let target_sample_rate_hz = target_sample_rate_hz.unwrap_or(DEFAULT_TARGET_SAMPLE_RATE_HZ);
    if !target_sample_rate_hz.is_finite() || target_sample_rate_hz <= 0.0 {
        return Err(FnvKfConversionError::InvalidTargetSampleRate(
            target_sample_rate_hz,
        ));
    }

    let parsed = match sequence_index {
        Some(index) => parse_kf_sequence_bytes(&kf_bytes, source_rel, index),
        None => parse_kf_bytes(&kf_bytes, source_rel),
    };
    let mut clip = match parsed {
        Ok(clip) => clip,
        Err(KfParseError::CompactSpline { bone_name, source }) => {
            return Err(FnvKfConversionError::CompactSplineDecode {
                path: source_rel.to_string(),
                bone_name,
                source,
            });
        }
        Err(KfParseError::Channel {
            target_name,
            interpolator_type,
            source,
        }) => {
            return Err(FnvKfConversionError::ChannelDecode {
                path: source_rel.to_string(),
                target_name,
                interpolator_type,
                source,
            });
        }
        Err(KfParseError::MultipleSequences {
            source_path,
            count,
            names,
        }) => {
            return Err(FnvKfConversionError::MultipleSequences {
                source_path,
                count,
                names,
            });
        }
        Err(KfParseError::SequenceIndexOutOfRange {
            source_path,
            index,
            count,
        }) => {
            return Err(FnvKfConversionError::SequenceIndexOutOfRange {
                source_path,
                index,
                count,
            });
        }
        Err(error) => {
            return Err(FnvKfConversionError::Parse {
                path: source_rel.to_string(),
                message: error.to_string(),
            });
        }
    };

    omit_missing_weapon_attachment_track(&mut clip, ordered_source_bone_names);
    omit_unbound_stationary_scene_tracks(&mut clip, ordered_source_bone_names, source_rel);

    clip.events = clip
        .events
        .into_iter()
        .filter_map(|ev| {
            match event_map.get(&ev.text) {
                Some(mapped) if mapped.is_empty() => None, // explicit drop
                Some(mapped) => Some(AnimationEvent {
                    time: ev.time,
                    source_time: ev.source_time,
                    text: mapped.clone(),
                }),
                None => Some(ev),
            }
        })
        .collect();

    let parse_warnings = std::mem::take(&mut clip.warnings);
    let sequence_name = clip.name.clone();
    let transform_track_count = clip.channels.len();
    let float_track_count = clip.float_channels.len();
    let event_count = clip.events.len();
    let events = clip
        .events
        .iter()
        .map(|event| FnvKfTextKeyEvidence {
            time: event.time,
            text: event.text.clone(),
        })
        .collect();
    let sample_count = target_sample_count(clip.duration, target_sample_rate_hz);

    let float_track_to_float_slot_indices =
        bind_fnv_kf_float_tracks(ordered_source_float_slot_names, &clip.float_channels)?;
    let declaration = bind_fnv_kf_clip(
        ordered_source_bone_names,
        &clip,
        runtime_skeleton_path,
        &runtime_clip_path,
        original_skeleton_name,
        &float_track_to_float_slot_indices,
    )?;
    let (xml, motion) = clip_to_havok_xml_with_float_tracks(
        &clip,
        &declaration,
        &float_track_to_float_slot_indices,
        original_skeleton_name,
        target_sample_rate_hz,
        extracted_motion_policy,
    )?;
    if matches!(
        motion,
        FnvClipMotion::ExtractedPlanar { .. } | FnvClipMotion::ExtractedPlanarYaw { .. }
    ) && extracted_motion_policy != FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame
    {
        return Err(FnvKfConversionError::ExtractPlanarReferenceFramePolicyRequired);
    }

    let hkx_bytes =
        havok_native::api::havok_xml_to_hkx(&xml).map_err(FnvKfConversionError::Pack)?;
    validate_staged_fo4_animation(&hkx_bytes)?;

    Ok(StagedFnvKfArtifact {
        hkx_bytes,
        receipt: FnvKfArtifactReceipt {
            source_kf: source_rel.to_string(),
            sequence_index: sequence_index.unwrap_or(0),
            sequence_name,
            output_clip_path: runtime_clip_path,
            runtime_skeleton_path: runtime_skeleton_path.to_string(),
            original_skeleton_name: original_skeleton_name.to_string(),
            transform_track_count,
            float_track_count,
            sample_count,
            event_count,
            events,
            warnings: parse_warnings,
            motion,
            binding: declaration.binding,
        },
    })
}

fn validate_staged_fo4_animation(hkx_bytes: &[u8]) -> Result<(), FnvKfConversionError> {
    let xml = havok_native::api::havok_hkx_to_xml(hkx_bytes)
        .map_err(FnvKfConversionError::ValidatePacked)?;
    let document = roxmltree::Document::parse(&xml).map_err(|error| {
        FnvKfConversionError::InvalidPackedTargetHeader {
            detail: error.to_string(),
        }
    })?;
    let packfile = document
        .descendants()
        .find(|node| node.has_tag_name("hkpackfile"))
        .ok_or_else(|| FnvKfConversionError::InvalidPackedTargetHeader {
            detail: "missing hkpackfile root".to_string(),
        })?;
    let class_version = packfile.attribute("classversion");
    let contents_version = packfile.attribute("contentsversion");
    if class_version != Some("11") || contents_version != Some("hk_2014.1.0-r1") {
        return Err(FnvKfConversionError::InvalidPackedTargetHeader {
            detail: format!(
                "expected classversion 11 / hk_2014.1.0-r1, got {class_version:?} / {contents_version:?}"
            ),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// KF NIF parser
// ---------------------------------------------------------------------------
//
// .kf files are standard NIF binaries, parsed with NifFile::from_bytes and
// read through the NifBlockExt trait below.

#[derive(Debug, Clone)]
struct AnimationEvent {
    time: f64,
    source_time: f64,
    text: String,
}

#[derive(Debug, Clone)]
struct AnimationKeyframe {
    time: f64,
    /// Quaternion (x,y,z,w) for rotation; (x,y,z) for translation; (s,) for scale.
    value: Vec<f64>,
    interpolation: Interpolation,
    forward: Option<Vec<f64>>,
    backward: Option<Vec<f64>>,
    tbc: Option<(f64, f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Interpolation {
    Linear,
    Quadratic,
    Tbc,
    Constant,
}

#[allow(dead_code)]
impl Interpolation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "LINEAR_KEY",
            Self::Quadratic => "QUADRATIC_KEY",
            Self::Tbc => "TBC_KEY",
            Self::Constant => "CONST_KEY",
        }
    }
}

#[derive(Debug, Clone)]
struct BoneChannel {
    bone_name: String,
    priority: u32,
    rotations: Vec<AnimationKeyframe>,
    translations: Vec<AnimationKeyframe>,
    scales: Vec<AnimationKeyframe>,
}

#[derive(Debug, Clone)]
struct FloatChannel {
    slot_name: String,
    target_name: String,
    property_type: String,
    controller_type: String,
    controller_id: String,
    keyframes: Vec<AnimationKeyframe>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct AnimationClip {
    name: String,
    source_start_time: f64,
    source_stop_time: f64,
    duration: f64,
    cycle_type: String,
    frequency: f64,
    #[allow(dead_code)]
    accum_root: String,
    channels: Vec<BoneChannel>,
    #[allow(dead_code)]
    float_channels: Vec<FloatChannel>,
    controller_types: Vec<String>,
    interpolator_types: Vec<String>,
    events: Vec<AnimationEvent>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FnvKfCycle {
    Loop,
    Reverse,
    Clamp,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FnvKfTextKeyEvidence {
    pub(crate) time: f64,
    pub(crate) text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FnvKfBindingCompatibility {
    Verified,
    Unverified { detail: String },
    Incompatible { detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FnvKfBindingEvidence {
    pub(crate) skeleton_path: Option<String>,
    pub(crate) transform_track_count: usize,
    pub(crate) float_track_count: usize,
    pub(crate) controller_types: Vec<String>,
    pub(crate) interpolator_types: Vec<String>,
    pub(crate) target_names: Vec<String>,
    pub(crate) required_float_slots: Vec<String>,
    pub(crate) compatibility: FnvKfBindingCompatibility,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FnvKfRootMotionEvidence {
    None,
    Stationary {
        accum_root: Option<String>,
    },
    Planar {
        accum_root: String,
        distance: f64,
        yaw_radians: f64,
    },
    Unsupported {
        accum_root: Option<String>,
        detail: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedFnvCreatureKf {
    pub(crate) source_kf: String,
    pub(crate) sequence_index: usize,
    pub(crate) sequence_name: String,
    pub(crate) cycle: FnvKfCycle,
    pub(crate) start_time: f64,
    pub(crate) stop_time: f64,
    pub(crate) frequency: f64,
    pub(crate) text_keys: Vec<FnvKfTextKeyEvidence>,
    pub(crate) binding: FnvKfBindingEvidence,
    pub(crate) root_motion: FnvKfRootMotionEvidence,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FnvKfSkeletonContract<'a> {
    pub(crate) skeleton_path: &'a str,
    pub(crate) ordered_bone_names: &'a [String],
    pub(crate) ordered_float_slot_names: &'a [String],
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum FnvKfFloatBindingError {
    #[error(
        "KF contains {track_count} float tracks but ordered_source_float_slot_names is missing"
    )]
    MissingFloatSlots { track_count: usize },
    #[error("source animation float slot {index} has an empty name")]
    EmptyFloatSlotName { index: usize },
    #[error("duplicate source animation float slot {name:?} at indices {first} and {second}")]
    DuplicateFloatSlot {
        name: String,
        first: usize,
        second: usize,
    },
    #[error(
        "ambiguous normalized source animation float slot {normalized:?}: {first_name:?} at {first} and {second_name:?} at {second}"
    )]
    AmbiguousFloatSlot {
        normalized: String,
        first_name: String,
        first: usize,
        second_name: String,
        second: usize,
    },
    #[error("duplicate KF float target {name:?} at tracks {first} and {second}")]
    DuplicateFloatTrack {
        name: String,
        first: usize,
        second: usize,
    },
    #[error(
        "KF float track {track} target {name:?} is absent from the source animation float slots"
    )]
    MissingFloatTrackTarget { track: usize, name: String },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum FnvKfBindingError {
    #[error("source animation skeleton has no bones")]
    EmptySkeleton,
    #[error("source animation skeleton bone {index} has an empty name")]
    EmptySkeletonBoneName { index: usize },
    #[error("KF transform track {index} has an empty target name")]
    EmptyTrackName { index: usize },
    #[error(
        "duplicate source animation skeleton bone name {name:?} at indices {first} and {second}"
    )]
    DuplicateSkeletonBone {
        name: String,
        first: usize,
        second: usize,
    },
    #[error(
        "ambiguous normalized source animation skeleton bone name {normalized:?}: {first_name:?} at {first} and {second_name:?} at {second}"
    )]
    AmbiguousSkeletonBone {
        normalized: String,
        first_name: String,
        first: usize,
        second_name: String,
        second: usize,
    },
    #[error("duplicate KF transform target {name:?} at tracks {first} and {second}")]
    DuplicateTrackTarget {
        name: String,
        first: usize,
        second: usize,
    },
    #[error(
        "KF transform track {track} target {name:?} is absent from the source animation skeleton"
    )]
    MissingTrackTarget { track: usize, name: String },
    #[error(
        "KF overlay track {track} target {name:?} has no declared source-rig animation-node/property binding"
    )]
    UnsupportedOverlayChannel { track: usize, name: String },
    #[error(
        "KF transform track {track} maps to out-of-range source bone {bone_index} (bone count {bone_count})"
    )]
    OutOfRangeMapping {
        track: usize,
        bone_index: usize,
        bone_count: usize,
    },
    #[error(
        "KF transform tracks {first_track} and {second_track} both map to source bone {bone_index}"
    )]
    DuplicateBoneMapping {
        bone_index: usize,
        first_track: usize,
        second_track: usize,
    },
}

/// Bind named FNV/FO3 KF transform tracks to an explicitly ordered animation skeleton.
///
/// `ordered_source_bone_names` must already be the canonical order used to emit the
/// source rig's Skeleton.hkx. Body-mesh or arbitrary NIF node order is not inferred here.
/// KF targets such as `##BatonHandle` are weapon/scene-node overlays, not implicit bones.
pub(crate) fn bind_fnv_kf_clip(
    ordered_source_bone_names: &[String],
    parsed_clip: &AnimationClip,
    runtime_skeleton_path: &str,
    runtime_clip_path: &str,
    original_skeleton_name: &str,
    float_track_to_float_slot_indices: &[usize],
) -> Result<ClipDecl, FnvKfBindingError> {
    let mapping = bind_fnv_kf_transform_tracks(ordered_source_bone_names, parsed_clip)?;

    Ok(ClipDecl {
        name: parsed_clip.name.clone(),
        path: runtime_clip_path.to_string(),
        binding: ClipBinding {
            skeleton_path: runtime_skeleton_path.to_string(),
            original_skeleton_name: original_skeleton_name.to_string(),
            declared_transform_tracks: mapping.len(),
            transform_track_to_bone_indices: mapping,
            declared_float_tracks: float_track_to_float_slot_indices.len(),
            float_track_to_float_slot_indices: float_track_to_float_slot_indices.to_vec(),
        },
        looping: parsed_clip.cycle_type == "loop",
    })
}

fn bind_fnv_kf_transform_tracks(
    ordered_source_bone_names: &[String],
    parsed_clip: &AnimationClip,
) -> Result<Vec<usize>, FnvKfBindingError> {
    if ordered_source_bone_names.is_empty() {
        return Err(FnvKfBindingError::EmptySkeleton);
    }

    let mut exact_bones: HashMap<&str, usize> =
        HashMap::with_capacity(ordered_source_bone_names.len());
    let mut normalized_bones: HashMap<String, (usize, &str)> =
        HashMap::with_capacity(ordered_source_bone_names.len());
    for (index, name) in ordered_source_bone_names.iter().enumerate() {
        if name.trim().is_empty() {
            return Err(FnvKfBindingError::EmptySkeletonBoneName { index });
        }
        if let Some(first) = exact_bones.insert(name.as_str(), index) {
            return Err(FnvKfBindingError::DuplicateSkeletonBone {
                name: name.clone(),
                first,
                second: index,
            });
        }

        let normalized = normalize_fnv_bone_name(name);
        if let Some(&(first, first_name)) = normalized_bones.get(&normalized) {
            return Err(FnvKfBindingError::AmbiguousSkeletonBone {
                normalized,
                first_name: first_name.to_string(),
                first,
                second_name: name.clone(),
                second: index,
            });
        }
        normalized_bones.insert(normalized, (index, name.as_str()));
    }

    let mut seen_tracks = HashMap::with_capacity(parsed_clip.channels.len());
    let mut mapping = Vec::with_capacity(parsed_clip.channels.len());
    for (track, channel) in parsed_clip.channels.iter().enumerate() {
        if channel.bone_name.trim().is_empty() {
            return Err(FnvKfBindingError::EmptyTrackName { index: track });
        }
        let normalized = normalize_fnv_bone_name(&channel.bone_name);
        if let Some(first) = seen_tracks.insert(normalized.clone(), track) {
            return Err(FnvKfBindingError::DuplicateTrackTarget {
                name: channel.bone_name.clone(),
                first,
                second: track,
            });
        }

        let bone_index = exact_bones
            .get(channel.bone_name.as_str())
            .copied()
            .or_else(|| normalized_bones.get(&normalized).map(|(index, _)| *index));
        let bone_index = match bone_index {
            Some(index) => index,
            None if channel.bone_name.trim_start().starts_with("##") => {
                return Err(FnvKfBindingError::UnsupportedOverlayChannel {
                    track,
                    name: channel.bone_name.clone(),
                });
            }
            None => {
                return Err(FnvKfBindingError::MissingTrackTarget {
                    track,
                    name: channel.bone_name.clone(),
                });
            }
        };
        mapping.push(bone_index);
    }
    validate_fnv_kf_mapping(&mapping, ordered_source_bone_names.len())?;

    Ok(mapping)
}

fn normalize_fnv_bone_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn omit_missing_weapon_attachment_track(
    clip: &mut AnimationClip,
    ordered_source_bone_names: &[String],
) {
    if ordered_source_bone_names.is_empty()
        || ordered_source_bone_names
            .iter()
            .any(|name| normalize_fnv_bone_name(name) == "weapon")
    {
        return;
    }
    let before = clip.channels.len();
    clip.channels
        .retain(|channel| normalize_fnv_bone_name(&channel.bone_name) != "weapon");
    if clip.channels.len() != before {
        clip.warnings
            .push("ignored KF Weapon attachment track absent from source skeleton".to_string());
    }
}

fn omit_unbound_stationary_scene_tracks(
    clip: &mut AnimationClip,
    ordered_source_bone_names: &[String],
    source_kf: &str,
) {
    if ordered_source_bone_names.is_empty() {
        return;
    }
    let normalized_path = source_kf.replace('\\', "/").to_ascii_lowercase();
    if ![
        "creatures/nvmrhouse/",
        "creatures/nvpenthouemaincomputer/",
        "nvdlc03/creatures/braintank/",
    ]
    .iter()
    .any(|family| normalized_path.contains(family))
    {
        return;
    }
    let bones = ordered_source_bone_names
        .iter()
        .map(|name| normalize_fnv_bone_name(name))
        .collect::<std::collections::HashSet<_>>();
    let before = clip.channels.len();
    clip.channels
        .retain(|channel| bones.contains(&normalize_fnv_bone_name(&channel.bone_name)));
    let omitted = before - clip.channels.len();
    if omitted > 0 {
        clip.warnings.push(format!(
            "ignored {omitted} non-skeletal stationary scene-controller transform track(s)"
        ));
    }
}

fn bind_fnv_kf_float_tracks(
    ordered_source_float_slot_names: Option<&[String]>,
    channels: &[FloatChannel],
) -> Result<Vec<usize>, FnvKfFloatBindingError> {
    if channels.is_empty() {
        return Ok(Vec::new());
    }
    let slots =
        ordered_source_float_slot_names.ok_or(FnvKfFloatBindingError::MissingFloatSlots {
            track_count: channels.len(),
        })?;
    let mut exact_slots: HashMap<&str, usize> = HashMap::with_capacity(slots.len());
    let mut normalized_slots: HashMap<String, (usize, &str)> = HashMap::with_capacity(slots.len());
    for (index, name) in slots.iter().enumerate() {
        if name.trim().is_empty() {
            return Err(FnvKfFloatBindingError::EmptyFloatSlotName { index });
        }
        if let Some(first) = exact_slots.insert(name, index) {
            return Err(FnvKfFloatBindingError::DuplicateFloatSlot {
                name: name.clone(),
                first,
                second: index,
            });
        }
        let normalized = normalize_fnv_bone_name(name);
        if let Some(&(first, first_name)) = normalized_slots.get(&normalized) {
            return Err(FnvKfFloatBindingError::AmbiguousFloatSlot {
                normalized,
                first_name: first_name.to_string(),
                first,
                second_name: name.clone(),
                second: index,
            });
        }
        normalized_slots.insert(normalized, (index, name));
    }

    let mut seen_tracks = HashMap::with_capacity(channels.len());
    let mut mapping = Vec::with_capacity(channels.len());
    for (track, channel) in channels.iter().enumerate() {
        let normalized = normalize_fnv_bone_name(&channel.slot_name);
        if let Some(first) = seen_tracks.insert(normalized.clone(), track) {
            return Err(FnvKfFloatBindingError::DuplicateFloatTrack {
                name: channel.slot_name.clone(),
                first,
                second: track,
            });
        }
        let slot_index = exact_slots
            .get(channel.slot_name.as_str())
            .copied()
            .or_else(|| normalized_slots.get(&normalized).map(|(index, _)| *index))
            .ok_or_else(|| FnvKfFloatBindingError::MissingFloatTrackTarget {
                track,
                name: channel.slot_name.clone(),
            })?;
        mapping.push(slot_index);
    }
    Ok(mapping)
}

fn validate_fnv_kf_mapping(mapping: &[usize], bone_count: usize) -> Result<(), FnvKfBindingError> {
    let mut first_track_by_bone = HashMap::with_capacity(mapping.len());
    for (track, &bone_index) in mapping.iter().enumerate() {
        if bone_index >= bone_count {
            return Err(FnvKfBindingError::OutOfRangeMapping {
                track,
                bone_index,
                bone_count,
            });
        }
        if let Some(first_track) = first_track_by_bone.insert(bone_index, track) {
            return Err(FnvKfBindingError::DuplicateBoneMapping {
                bone_index,
                first_track,
                second_track: track,
            });
        }
    }
    Ok(())
}

pub(crate) fn load_ordered_source_skeleton_names(
    path: &Path,
) -> Result<Vec<String>, FnvKfConversionError> {
    let bytes = std::fs::read(path).map_err(|source| FnvKfConversionError::ReadSourceSkeleton {
        path: path.to_path_buf(),
        source,
    })?;
    let nif = NifFile::from_bytes(&bytes, None).map_err(|source| {
        FnvKfConversionError::ParseSourceSkeleton {
            path: path.to_path_buf(),
            message: source.to_string(),
        }
    })?;
    let names = ordered_named_nif_nodes(&nif);
    if names.is_empty() {
        return Err(FnvKfConversionError::SourceSkeletonHasNoNamedNodes {
            path: path.to_path_buf(),
        });
    }
    Ok(names)
}

fn ordered_named_nif_nodes(nif: &NifFile) -> Vec<String> {
    let named_nodes: Vec<(usize, &nif_core_native::model::NifBlock)> = nif
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| {
            block.type_name == "NiNode"
                && matches!(block.get_field("Name"), Some(NifValue::String(name)) if !name.is_empty())
        })
        .collect();
    let node_ids: HashSet<usize> = named_nodes.iter().map(|(id, _)| *id).collect();
    let mut parent_by_child = HashMap::new();
    for (parent, block) in &named_nodes {
        let Some(NifValue::Array(children)) = block.get_field("Children") else {
            continue;
        };
        for child in children {
            let child = match child {
                NifValue::Ref(index) if *index >= 0 => *index as usize,
                NifValue::Int(index) if *index >= 0 => *index as usize,
                _ => continue,
            };
            if node_ids.contains(&child) {
                parent_by_child.insert(child, *parent);
            }
        }
    }

    fn visit(
        id: usize,
        named_nodes: &[(usize, &nif_core_native::model::NifBlock)],
        node_ids: &HashSet<usize>,
        ordered: &mut Vec<usize>,
        visited: &mut HashSet<usize>,
    ) {
        if !node_ids.contains(&id) || !visited.insert(id) {
            return;
        }
        ordered.push(id);
        let Some((_, block)) = named_nodes.iter().find(|(block_id, _)| *block_id == id) else {
            return;
        };
        let Some(NifValue::Array(children)) = block.get_field("Children") else {
            return;
        };
        for child in children {
            let child = match child {
                NifValue::Ref(index) if *index >= 0 => *index as usize,
                NifValue::Int(index) if *index >= 0 => *index as usize,
                _ => continue,
            };
            visit(child, named_nodes, node_ids, ordered, visited);
        }
    }

    let mut ordered = Vec::with_capacity(named_nodes.len());
    let mut visited = HashSet::with_capacity(named_nodes.len());
    for (id, _) in named_nodes
        .iter()
        .filter(|(id, _)| !parent_by_child.contains_key(id))
    {
        visit(*id, &named_nodes, &node_ids, &mut ordered, &mut visited);
    }
    for (id, _) in &named_nodes {
        visit(*id, &named_nodes, &node_ids, &mut ordered, &mut visited);
    }

    ordered
        .into_iter()
        .filter_map(|id| {
            let (_, block) = named_nodes.iter().find(|(block_id, _)| *block_id == id)?;
            match block.get_field("Name") {
                Some(NifValue::String(name)) => Some(name.clone()),
                _ => None,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// NifBlock field accessor helpers
// ---------------------------------------------------------------------------

trait NifBlockExt {
    fn string_field(&self, name: &str) -> Option<String>;
    fn f64_field(&self, name: &str) -> Option<f64>;
    fn u32_field(&self, name: &str) -> Option<u32>;
    fn block_ref_field(&self, name: &str) -> Option<usize>;
    fn array_of_structs(&self, name: &str) -> Vec<KfEntry>;
    fn struct_field(&self, name: &str) -> Option<KfEntry>;
}

impl NifBlockExt for nif_core_native::model::NifBlock {
    fn string_field(&self, name: &str) -> Option<String> {
        match self.get_field(name)? {
            NifValue::String(s) => Some(s.clone()),
            NifValue::Char(s) => Some(s.clone()),
            _ => None,
        }
    }

    fn f64_field(&self, name: &str) -> Option<f64> {
        match self.get_field(name)? {
            NifValue::Float(f) => Some(*f),
            NifValue::Int(i) => Some(*i as f64),
            NifValue::UInt(u) => Some(*u as f64),
            _ => None,
        }
    }

    fn u32_field(&self, name: &str) -> Option<u32> {
        match self.get_field(name)? {
            NifValue::UInt(u) => Some(*u as u32),
            NifValue::Int(i) if *i >= 0 => Some(*i as u32),
            NifValue::Float(f) => Some(*f as u32),
            _ => None,
        }
    }

    fn block_ref_field(&self, name: &str) -> Option<usize> {
        match self.get_field(name)? {
            NifValue::Ref(r) if *r >= 0 => Some(*r as usize),
            NifValue::Int(i) if *i >= 0 => Some(*i as usize),
            _ => None,
        }
    }

    fn array_of_structs(&self, name: &str) -> Vec<KfEntry> {
        match self.get_field(name) {
            Some(NifValue::Array(arr)) => arr.iter().map(|v| KfEntry(v.clone())).collect(),
            _ => Vec::new(),
        }
    }

    fn struct_field(&self, name: &str) -> Option<KfEntry> {
        self.get_field(name).map(|v| KfEntry(v.clone()))
    }
}

/// Thin wrapper around a `NifValue` for querying sub-fields in struct/array contexts.
struct KfEntry(NifValue);

impl KfEntry {
    fn f64_or(&self, name: &str, default: f64) -> f64 {
        match &self.0 {
            NifValue::Struct(m) => match m.get(name) {
                Some(NifValue::Float(f)) => *f,
                Some(NifValue::Int(i)) => *i as f64,
                Some(NifValue::UInt(u)) => *u as f64,
                _ => default,
            },
            _ => default,
        }
    }

    fn f64_opt(&self, name: &str) -> Option<f64> {
        match &self.0 {
            NifValue::Struct(m) => match m.get(name) {
                Some(NifValue::Float(f)) => Some(*f),
                Some(NifValue::Int(i)) => Some(*i as f64),
                Some(NifValue::UInt(u)) => Some(*u as f64),
                _ => None,
            },
            _ => None,
        }
    }

    fn string_or(&self, name: &str, default: &str) -> String {
        match &self.0 {
            NifValue::Struct(m) => match m.get(name) {
                Some(NifValue::String(s)) => s.clone(),
                Some(NifValue::Char(s)) => s.clone(),
                _ => default.to_string(),
            },
            _ => default.to_string(),
        }
    }

    fn u32_or(&self, name: &str, default: u32) -> u32 {
        match &self.0 {
            NifValue::Struct(m) => match m.get(name) {
                Some(NifValue::UInt(u)) => *u as u32,
                Some(NifValue::Int(i)) if *i >= 0 => *i as u32,
                _ => default,
            },
            _ => default,
        }
    }

    fn block_ref_or(&self, name: &str, default: i64) -> i64 {
        match &self.0 {
            NifValue::Struct(m) => match m.get(name) {
                Some(NifValue::Ref(r)) => *r as i64,
                Some(NifValue::Int(i)) => *i,
                _ => default,
            },
            _ => default,
        }
    }

    fn array_of_structs(&self, name: &str) -> Vec<KfEntry> {
        match &self.0 {
            NifValue::Struct(m) => match m.get(name) {
                Some(NifValue::Array(arr)) => arr.iter().map(|v| KfEntry(v.clone())).collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn struct_field(&self, name: &str) -> Option<KfEntry> {
        match &self.0 {
            NifValue::Struct(m) => m.get(name).map(|v| KfEntry(v.clone())),
            _ => None,
        }
    }

    fn has_field(&self, name: &str) -> bool {
        match &self.0 {
            NifValue::Struct(m) => m.contains_key(name),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// parse_kf_bytes
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub(crate) enum ChannelDecodeError {
    #[error("missing {0}")]
    MissingField(&'static str),
    #[error("invalid {name} block reference {index}")]
    InvalidReference { name: &'static str, index: usize },
    #[error("{name} block {index} is {actual}, expected {expected}")]
    WrongBlockType {
        name: &'static str,
        index: usize,
        actual: String,
        expected: &'static str,
    },
    #[error("float control-point array is missing or contains a non-finite value")]
    InvalidFloatControlPointArray,
    #[error("{channel} base value is absent or invalid")]
    InvalidBaseValue { channel: &'static str },
    #[error("{channel} key data is malformed")]
    InvalidKeyData { channel: &'static str },
    #[error("{channel} produced a non-scalar value")]
    NonScalarValue { channel: &'static str },
    #[error(transparent)]
    Spline(#[from] CompactSplineError),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum KfParseError {
    #[error("{0}")]
    Message(String),
    #[error(
        "KF {source_path} contains {count} NiControllerSequence blocks; select one explicitly: {names:?}"
    )]
    MultipleSequences {
        source_path: String,
        count: usize,
        names: Vec<String>,
    },
    #[error("KF {source_path} sequence index {index} is out of range (sequence count {count})")]
    SequenceIndexOutOfRange {
        source_path: String,
        index: usize,
        count: usize,
    },
    #[error("compact spline for {bone_name:?}: {source}")]
    CompactSpline {
        bone_name: String,
        #[source]
        source: CompactSplineError,
    },
    #[error("{interpolator_type} channel {target_name:?}: {source}")]
    Channel {
        target_name: String,
        interpolator_type: String,
        #[source]
        source: ChannelDecodeError,
    },
}

fn parse_kf_bytes(bytes: &[u8], source_path: &str) -> Result<AnimationClip, KfParseError> {
    let nif = NifFile::from_bytes(bytes, None)
        .map_err(|e| KfParseError::Message(format!("NIF parse failed ({source_path}): {e}")))?;
    let catalog = catalog_fnv_kf_sequences_from_nif(&nif);
    match catalog.as_slice() {
        [] => Err(KfParseError::Message(format!(
            "No NiControllerSequence in {source_path}"
        ))),
        [entry] => parse_kf_sequence(&nif, entry.block_index, source_path),
        entries => Err(KfParseError::MultipleSequences {
            source_path: source_path.to_string(),
            count: entries.len(),
            names: entries.iter().map(|entry| entry.name.clone()).collect(),
        }),
    }
}

fn parse_kf_sequence_bytes(
    bytes: &[u8],
    source_path: &str,
    sequence_index: usize,
) -> Result<AnimationClip, KfParseError> {
    let nif = NifFile::from_bytes(bytes, None)
        .map_err(|e| KfParseError::Message(format!("NIF parse failed ({source_path}): {e}")))?;
    let catalog = catalog_fnv_kf_sequences_from_nif(&nif);
    let entry =
        catalog
            .get(sequence_index)
            .ok_or_else(|| KfParseError::SequenceIndexOutOfRange {
                source_path: source_path.to_string(),
                index: sequence_index,
                count: catalog.len(),
            })?;
    parse_kf_sequence(&nif, entry.block_index, source_path)
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FnvKfSequenceCatalogEntry {
    pub(crate) index: usize,
    pub(crate) name: String,
    pub(crate) start_time: f64,
    pub(crate) stop_time: f64,
    block_index: usize,
}

pub(crate) fn catalog_fnv_kf_sequences(
    bytes: &[u8],
    source_path: &str,
) -> Result<Vec<FnvKfSequenceCatalogEntry>, KfParseError> {
    let nif = NifFile::from_bytes(bytes, None)
        .map_err(|e| KfParseError::Message(format!("NIF parse failed ({source_path}): {e}")))?;
    Ok(catalog_fnv_kf_sequences_from_nif(&nif))
}

fn catalog_fnv_kf_sequences_from_nif(nif: &NifFile) -> Vec<FnvKfSequenceCatalogEntry> {
    nif.blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.type_name == "NiControllerSequence")
        .enumerate()
        .map(|(index, (block_index, block))| FnvKfSequenceCatalogEntry {
            index,
            name: block.string_field("Name").unwrap_or_default(),
            start_time: block.f64_field("Start Time").unwrap_or(0.0),
            stop_time: block.f64_field("Stop Time").unwrap_or(0.0),
            block_index,
        })
        .collect()
}

pub(crate) fn parse_fnv_creature_kf(
    bytes: &[u8],
    source_kf: &str,
    sequence_index: Option<usize>,
    skeleton_contract: Option<FnvKfSkeletonContract<'_>>,
) -> Result<ParsedFnvCreatureKf, KfParseError> {
    let nif = NifFile::from_bytes(bytes, None).map_err(|error| {
        KfParseError::Message(format!("NIF parse failed ({source_kf}): {error}"))
    })?;
    let catalog = catalog_fnv_kf_sequences_from_nif(&nif);
    let entry = match sequence_index {
        Some(index) => catalog
            .get(index)
            .ok_or_else(|| KfParseError::SequenceIndexOutOfRange {
                source_path: source_kf.to_string(),
                index,
                count: catalog.len(),
            })?,
        None => match catalog.as_slice() {
            [] => {
                return Err(KfParseError::Message(format!(
                    "No NiControllerSequence in {source_kf}"
                )));
            }
            [entry] => entry,
            entries => {
                return Err(KfParseError::MultipleSequences {
                    source_path: source_kf.to_string(),
                    count: entries.len(),
                    names: entries.iter().map(|entry| entry.name.clone()).collect(),
                });
            }
        },
    };
    let mut clip = parse_kf_sequence(&nif, entry.block_index, source_kf)?;
    if let Some(contract) = skeleton_contract {
        omit_missing_weapon_attachment_track(&mut clip, contract.ordered_bone_names);
        omit_unbound_stationary_scene_tracks(&mut clip, contract.ordered_bone_names, source_kf);
    }
    let cycle = match clip.cycle_type.as_str() {
        "loop" => FnvKfCycle::Loop,
        "reverse" => FnvKfCycle::Reverse,
        "clamp" => FnvKfCycle::Clamp,
        _ => unreachable!("parse_kf_sequence validates cycle type"),
    };
    let transform_targets = clip
        .channels
        .iter()
        .map(|channel| channel.bone_name.clone())
        .collect::<Vec<_>>();
    let required_float_slots = clip
        .float_channels
        .iter()
        .map(|channel| channel.slot_name.clone())
        .collect::<Vec<_>>();
    let mut target_names = transform_targets.clone();
    target_names.extend(
        clip.float_channels
            .iter()
            .map(|channel| channel.target_name.clone()),
    );
    let (skeleton_path, compatibility) = match skeleton_contract {
        None => (
            None,
            FnvKfBindingCompatibility::Unverified {
                detail: format!(
                    "missing preserve-source-rig skeleton contract: require emitted skeleton path, ordered bone names covering transform targets {transform_targets:?}, and exact ordered float slots {required_float_slots:?}"
                ),
            },
        ),
        Some(contract) => {
            let skeleton_path = Some(contract.skeleton_path.to_string());
            if contract.skeleton_path.trim().is_empty() {
                (
                    skeleton_path,
                    FnvKfBindingCompatibility::Incompatible {
                        detail: "preserve-source-rig skeleton path is empty".to_string(),
                    },
                )
            } else {
                let transform_binding =
                    bind_fnv_kf_transform_tracks(contract.ordered_bone_names, &clip);
                let float_binding = bind_fnv_kf_float_tracks(
                    Some(contract.ordered_float_slot_names),
                    &clip.float_channels,
                );
                let compatibility = match (transform_binding, float_binding) {
                    (Ok(_), Ok(_)) => FnvKfBindingCompatibility::Verified,
                    (transform, float) => {
                        let mut failures = Vec::new();
                        if let Err(error) = transform {
                            failures.push(format!("transform binding: {error}"));
                        }
                        if let Err(error) = float {
                            failures.push(format!("float binding: {error}"));
                        }
                        FnvKfBindingCompatibility::Incompatible {
                            detail: failures.join("; "),
                        }
                    }
                };
                (skeleton_path, compatibility)
            }
        }
    };
    let root_motion = creature_kf_root_motion_evidence(&clip);
    Ok(ParsedFnvCreatureKf {
        source_kf: source_kf.to_string(),
        sequence_index: entry.index,
        sequence_name: clip.name,
        cycle,
        start_time: clip.source_start_time,
        stop_time: clip.source_stop_time,
        frequency: clip.frequency,
        text_keys: clip
            .events
            .into_iter()
            .map(|event| FnvKfTextKeyEvidence {
                time: event.source_time,
                text: event.text,
            })
            .collect(),
        binding: FnvKfBindingEvidence {
            skeleton_path,
            transform_track_count: clip.channels.len(),
            float_track_count: clip.float_channels.len(),
            controller_types: clip.controller_types,
            interpolator_types: clip.interpolator_types,
            target_names,
            required_float_slots,
            compatibility,
        },
        root_motion,
    })
}

fn creature_kf_root_motion_evidence(clip: &AnimationClip) -> FnvKfRootMotionEvidence {
    if clip.accum_root.trim().is_empty() {
        return FnvKfRootMotionEvidence::None;
    }
    let transform_track_to_bone_indices = (0..clip.channels.len()).collect::<Vec<_>>();
    let sample_count = target_sample_count(clip.duration, DEFAULT_TARGET_SAMPLE_RATE_HZ);
    match extract_planar_root_motion_from_mapping(
        clip,
        &transform_track_to_bone_indices,
        sample_count,
        FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
    ) {
        Ok(Some(motion)) => {
            let end = motion.samples.last().copied().unwrap_or([0.0; 4]);
            FnvKfRootMotionEvidence::Planar {
                accum_root: clip.accum_root.clone(),
                distance: end[0].hypot(end[1]),
                yaw_radians: end[3],
            }
        }
        Ok(None) | Err(FnvKfConversionError::MissingAccumRootTrack { .. }) => {
            FnvKfRootMotionEvidence::Stationary {
                accum_root: Some(clip.accum_root.clone()),
            }
        }
        Err(error) => FnvKfRootMotionEvidence::Unsupported {
            accum_root: Some(clip.accum_root.clone()),
            detail: error.to_string(),
        },
    }
}

fn parse_kf_sequence(
    nif: &NifFile,
    sequence_block_index: usize,
    source_path: &str,
) -> Result<AnimationClip, KfParseError> {
    let seq = &nif.blocks[sequence_block_index];

    let name = seq.string_field("Name").unwrap_or_default();
    let frequency = seq.f64_field("Frequency").unwrap_or(1.0);
    if !frequency.is_finite() || frequency <= 0.0 {
        return Err(KfParseError::Message(format!(
            "Invalid NiControllerSequence Frequency {frequency} in {source_path}"
        )));
    }
    let start_time = seq.f64_field("Start Time").unwrap_or(0.0);
    let stop_time = seq.f64_field("Stop Time").unwrap_or(0.0);
    if !start_time.is_finite() || !stop_time.is_finite() || stop_time < start_time {
        return Err(KfParseError::Message(format!(
            "Invalid NiControllerSequence time range {start_time}..{stop_time} in {source_path}"
        )));
    }
    let duration = (stop_time - start_time) / frequency;
    let cycle_raw = seq.u32_field("Cycle Type").unwrap_or(2);
    let cycle_type = fnv_cycle_type(cycle_raw)
        .ok_or_else(|| {
            KfParseError::Message(format!(
                "Invalid NiControllerSequence Cycle Type {cycle_raw}"
            ))
        })?
        .to_string();
    let accum_root = seq.string_field("Accum Root Name").unwrap_or_default();

    // Events from NiTextKeyExtraData.
    let mut events: Vec<AnimationEvent> = Vec::new();
    if let Some(tk_ref) = seq.block_ref_field("Text Keys") {
        if let Some(tk) = nif.blocks.get(tk_ref) {
            if tk.type_name == "NiTextKeyExtraData" {
                for entry in tk.array_of_structs("Text Keys") {
                    let time = entry.f64_or("Time", 0.0);
                    let text = entry.string_or("Value", "");
                    events.push(AnimationEvent {
                        time,
                        source_time: time,
                        text,
                    });
                }
            }
        }
    }

    // Controlled blocks → channels.
    let mut bone_channels: Vec<BoneChannel> = Vec::new();
    let mut float_channels: Vec<FloatChannel> = Vec::new();
    let mut controller_types = Vec::new();
    let mut interpolator_types = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for (controlled_index, cb) in seq
        .array_of_structs("Controlled Blocks")
        .into_iter()
        .enumerate()
    {
        let interp_ref = cb.block_ref_or("Interpolator", -1);
        let node_name = cb.string_or("Node Name", "");
        let ctrl_type = cb.string_or("Controller Type", "");
        let ctrl_id = cb.string_or("Controller ID", "");
        let priority = cb.u32_or("Priority", 26);
        let prop_type = cb.string_or("Property Type", "");

        if interp_ref < 0 {
            return Err(KfParseError::Channel {
                target_name: node_name,
                interpolator_type: "<missing>".to_string(),
                source: ChannelDecodeError::InvalidReference {
                    name: "Interpolator",
                    index: usize::MAX,
                },
            });
        }

        let interp = match nif.blocks.get(interp_ref as usize) {
            Some(b) => b,
            None => {
                return Err(KfParseError::Channel {
                    target_name: node_name,
                    interpolator_type: "<invalid-reference>".to_string(),
                    source: ChannelDecodeError::InvalidReference {
                        name: "Interpolator",
                        index: interp_ref as usize,
                    },
                });
            }
        };
        controller_types.push(ctrl_type.clone());
        interpolator_types.push(interp.type_name.clone());

        match interp.type_name.as_str() {
            "NiTransformInterpolator" | "BSRotAccumTransfInterpolator" => {
                if interp.type_name == "BSRotAccumTransfInterpolator"
                    && interp.block_ref_field("Data").is_none()
                    && compact_base_rotation(interp).is_none()
                    && compact_base_translation(interp).is_none()
                    && compact_base_scale(interp).is_none()
                {
                    warnings.push(format!(
                        "ignored empty BSRotAccumTransfInterpolator for {node_name}"
                    ));
                    continue;
                }
                let ch = read_transform_channel(nif, interp, &node_name, priority, &mut warnings)
                    .map_err(|source| KfParseError::Channel {
                    target_name: node_name.clone(),
                    interpolator_type: interp.type_name.clone(),
                    source,
                })?;
                bone_channels.push(ch);
            }
            "NiBSplineTransformInterpolator" => {
                let ch = read_spline_transform_channel(nif, interp, &node_name, priority).map_err(
                    |source| KfParseError::Channel {
                        target_name: node_name.clone(),
                        interpolator_type: interp.type_name.clone(),
                        source,
                    },
                )?;
                bone_channels.push(ch);
            }
            "NiBSplineCompTransformInterpolator" => {
                let ch = read_compact_spline_transform_channel(nif, interp, &node_name, priority)
                    .map_err(|source| KfParseError::CompactSpline {
                    bone_name: node_name.clone(),
                    source,
                })?;
                bone_channels.push(ch);
            }
            "BSTreadTransfInterpolator" => {
                let channels =
                    read_tread_transform_channels(nif, interp, priority).map_err(|source| {
                        KfParseError::Channel {
                            target_name: node_name.clone(),
                            interpolator_type: interp.type_name.clone(),
                            source,
                        }
                    })?;
                bone_channels.extend(channels);
            }
            "NiFloatInterpolator" | "NiBoolInterpolator" | "NiBoolTimelineInterpolator" => {
                let mut fc =
                    read_float_channel(nif, interp, &node_name, &ctrl_type, &prop_type, &ctrl_id)
                        .map_err(|source| KfParseError::Channel {
                        target_name: node_name.clone(),
                        interpolator_type: interp.type_name.clone(),
                        source,
                    })?;
                qualify_float_slot(&mut fc, controlled_index);
                float_channels.push(fc);
            }
            "NiBSplineCompFloatInterpolator" => {
                let mut fc = read_compact_spline_float_channel(
                    nif, interp, &node_name, &ctrl_type, &prop_type, &ctrl_id,
                )
                .map_err(|source| KfParseError::Channel {
                    target_name: node_name.clone(),
                    interpolator_type: interp.type_name.clone(),
                    source,
                })?;
                qualify_float_slot(&mut fc, controlled_index);
                float_channels.push(fc);
            }
            "NiBSplineCompPoint3Interpolator" => {
                let mut channels = read_compact_spline_point3_channels(
                    nif, interp, &node_name, &ctrl_type, &prop_type, &ctrl_id,
                )
                .map_err(|source| KfParseError::Channel {
                    target_name: node_name.clone(),
                    interpolator_type: interp.type_name.clone(),
                    source,
                })?;
                for channel in &mut channels {
                    qualify_float_slot(channel, controlled_index);
                }
                float_channels.extend(channels);
            }
            "NiPoint3Interpolator" => {
                let mut channels =
                    read_point3_channels(nif, interp, &node_name, &ctrl_type, &prop_type, &ctrl_id)
                        .map_err(|source| KfParseError::Channel {
                            target_name: node_name.clone(),
                            interpolator_type: interp.type_name.clone(),
                            source,
                        })?;
                for channel in &mut channels {
                    qualify_float_slot(channel, controlled_index);
                }
                float_channels.extend(channels);
            }
            other => {
                return Err(KfParseError::Channel {
                    target_name: node_name,
                    interpolator_type: other.to_string(),
                    source: ChannelDecodeError::InvalidKeyData {
                        channel: "unsupported interpolator",
                    },
                });
            }
        }
    }

    for channel in &mut bone_channels {
        normalize_kf_keyframe_times(&mut channel.rotations, start_time, frequency);
        normalize_kf_keyframe_times(&mut channel.translations, start_time, frequency);
        normalize_kf_keyframe_times(&mut channel.scales, start_time, frequency);
    }
    for channel in &mut float_channels {
        normalize_kf_keyframe_times(&mut channel.keyframes, start_time, frequency);
    }
    for event in &mut events {
        event.time = (event.time - start_time) / frequency;
    }

    Ok(AnimationClip {
        name,
        source_start_time: start_time,
        source_stop_time: stop_time,
        duration,
        cycle_type,
        frequency,
        accum_root,
        channels: bone_channels,
        float_channels,
        controller_types,
        interpolator_types,
        events,
        warnings,
    })
}

fn fnv_cycle_type(raw: u32) -> Option<&'static str> {
    match raw {
        0 => Some("loop"),
        1 => Some("reverse"),
        2 => Some("clamp"),
        _ => None,
    }
}

fn normalize_kf_keyframe_times(
    keyframes: &mut [AnimationKeyframe],
    start_time: f64,
    frequency: f64,
) {
    for keyframe in keyframes {
        keyframe.time = (keyframe.time - start_time) / frequency;
    }
}

const COMPACT_SPLINE_DEGREE: usize = 3;
const COMPACT_SPLINE_INVALID_HANDLE: u32 = u16::MAX as u32;
const COMPACT_SPLINE_SHORT_MAX: f64 = i16::MAX as f64;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CompactSplineError {
    #[error("missing {0}")]
    MissingField(&'static str),
    #[error("invalid {name} block reference {index}")]
    InvalidReference { name: &'static str, index: usize },
    #[error("{name} block {index} is {actual}, expected {expected}")]
    WrongBlockType {
        name: &'static str,
        index: usize,
        actual: String,
        expected: &'static str,
    },
    #[error("compact control-point array is missing or contains a non-short value")]
    InvalidControlPointArray,
    #[error("cubic B-spline needs at least 4 control points, got {0}")]
    InvalidControlPointCount(usize),
    #[error("invalid spline time range {start}..{stop}")]
    InvalidTimeRange { start: f64, stop: f64 },
    #[error("invalid {channel} compression offset/range ({offset}, {half_range})")]
    InvalidCompression {
        channel: &'static str,
        offset: f64,
        half_range: f64,
    },
    #[error(
        "{channel} handle {handle} needs {required} compact values, but only {available} exist"
    )]
    ControlPointRange {
        channel: &'static str,
        handle: usize,
        required: usize,
        available: usize,
    },
    #[error("{channel} sample {sample} is not finite")]
    NonFiniteSample {
        channel: &'static str,
        sample: usize,
    },
    #[error("rotation sample {0} is a degenerate quaternion")]
    DegenerateQuaternion(usize),
}

struct CompactSplineSampling {
    times: Vec<f64>,
    weights: Vec<Vec<f64>>,
}

impl CompactSplineSampling {
    fn new(num_control_points: usize, start: f64, stop: f64) -> Result<Self, CompactSplineError> {
        if num_control_points <= COMPACT_SPLINE_DEGREE {
            return Err(CompactSplineError::InvalidControlPointCount(
                num_control_points,
            ));
        }
        if !start.is_finite() || !stop.is_finite() || stop < start {
            return Err(CompactSplineError::InvalidTimeRange { start, stop });
        }

        let denominator = (num_control_points - 1) as f64;
        let parameter_end = (num_control_points - COMPACT_SPLINE_DEGREE) as f64;
        let mut times = Vec::with_capacity(num_control_points);
        let mut weights = Vec::with_capacity(num_control_points);
        for sample in 0..num_control_points {
            let fraction = sample as f64 / denominator;
            times.push(start + (stop - start) * fraction);
            weights.push(open_uniform_bspline_weights(
                num_control_points,
                parameter_end * fraction,
            ));
        }
        Ok(Self { times, weights })
    }
}

fn read_compact_spline_transform_channel(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    bone_name: &str,
    priority: u32,
) -> Result<BoneChannel, CompactSplineError> {
    let mut channel = BoneChannel {
        bone_name: bone_name.to_string(),
        priority,
        rotations: Vec::new(),
        translations: Vec::new(),
        scales: Vec::new(),
    };

    let has_animated_handle = ["Translation Handle", "Rotation Handle", "Scale Handle"]
        .into_iter()
        .any(|field| {
            interp
                .u32_field(field)
                .unwrap_or(COMPACT_SPLINE_INVALID_HANDLE)
                != COMPACT_SPLINE_INVALID_HANDLE
        });

    if has_animated_handle {
        (|| -> Result<(), CompactSplineError> {
            let spline_ref = interp
                .block_ref_field("Spline Data")
                .ok_or(CompactSplineError::MissingField("Spline Data"))?;
            let spline_data =
                compact_spline_block(nif, spline_ref, "Spline Data", "NiBSplineData")?;
            let basis_ref = interp
                .block_ref_field("Basis Data")
                .ok_or(CompactSplineError::MissingField("Basis Data"))?;
            let basis_data =
                compact_spline_block(nif, basis_ref, "Basis Data", "NiBSplineBasisData")?;

            let num_control_points = basis_data
                .u32_field("Num Control Points")
                .ok_or(CompactSplineError::MissingField("Num Control Points"))?
                as usize;
            let start = interp
                .f64_field("Start Time")
                .ok_or(CompactSplineError::MissingField("Start Time"))?;
            let stop = interp
                .f64_field("Stop Time")
                .ok_or(CompactSplineError::MissingField("Stop Time"))?;
            let sampling = CompactSplineSampling::new(num_control_points, start, stop)?;
            let compact_points = compact_control_points(spline_data)?;

            channel.translations = read_compact_vector_channel(
                interp,
                &compact_points,
                &sampling,
                num_control_points,
                "translation",
                "Translation Handle",
                "Translation Offset",
                "Translation Half Range",
                3,
            )?;
            channel.rotations = read_compact_rotation_channel(
                interp,
                &compact_points,
                &sampling,
                num_control_points,
            )?;
            channel.scales = read_compact_vector_channel(
                interp,
                &compact_points,
                &sampling,
                num_control_points,
                "scale",
                "Scale Handle",
                "Scale Offset",
                "Scale Half Range",
                1,
            )?;

            Ok(())
        })()?;
    }

    let start = interp.f64_field("Start Time").unwrap_or(0.0);
    if channel.translations.is_empty()
        && let Some(value) = compact_base_translation(interp)
    {
        channel.translations.push(linear_keyframe(start, value));
    }
    if channel.rotations.is_empty()
        && let Some(value) = compact_base_rotation(interp)
    {
        channel.rotations.push(linear_keyframe(start, value));
    }
    if channel.scales.is_empty()
        && let Some(value) = compact_base_scale(interp)
    {
        channel.scales.push(linear_keyframe(start, vec![value]));
    }
    if channel.translations.is_empty() && channel.rotations.is_empty() && channel.scales.is_empty()
    {
        return Err(CompactSplineError::MissingField(
            "animated handle or valid transform base",
        ));
    }

    Ok(channel)
}

fn read_spline_transform_channel(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    bone_name: &str,
    priority: u32,
) -> Result<BoneChannel, ChannelDecodeError> {
    let spline_ref = interp
        .block_ref_field("Spline Data")
        .ok_or(ChannelDecodeError::MissingField("Spline Data"))?;
    let spline_data = compact_spline_block(nif, spline_ref, "Spline Data", "NiBSplineData")?;
    let basis_ref = interp
        .block_ref_field("Basis Data")
        .ok_or(ChannelDecodeError::MissingField("Basis Data"))?;
    let basis_data = compact_spline_block(nif, basis_ref, "Basis Data", "NiBSplineBasisData")?;
    let num_control_points = basis_data
        .u32_field("Num Control Points")
        .ok_or(ChannelDecodeError::MissingField("Num Control Points"))?
        as usize;
    let start = interp
        .f64_field("Start Time")
        .ok_or(ChannelDecodeError::MissingField("Start Time"))?;
    let stop = interp
        .f64_field("Stop Time")
        .ok_or(ChannelDecodeError::MissingField("Stop Time"))?;
    let sampling = CompactSplineSampling::new(num_control_points, start, stop)?;
    let float_points = float_control_points(spline_data)?;

    let mut channel = BoneChannel {
        bone_name: bone_name.to_string(),
        priority,
        translations: read_float_spline_vector_channel(
            interp,
            &float_points,
            &sampling,
            num_control_points,
            "translation",
            "Translation Handle",
            3,
        )?,
        rotations: read_float_spline_vector_channel(
            interp,
            &float_points,
            &sampling,
            num_control_points,
            "rotation",
            "Rotation Handle",
            4,
        )?,
        scales: read_float_spline_vector_channel(
            interp,
            &float_points,
            &sampling,
            num_control_points,
            "scale",
            "Scale Handle",
            1,
        )?,
    };
    for (sample, key) in channel.rotations.iter_mut().enumerate() {
        let normalized = normalize_quaternion_wxyz(&key.value)
            .ok_or(CompactSplineError::DegenerateQuaternion(sample))?;
        key.value = vec![normalized[1], normalized[2], normalized[3], normalized[0]];
    }
    if channel.translations.is_empty()
        && let Some(value) = compact_base_translation(interp)
    {
        channel.translations.push(linear_keyframe(start, value));
    }
    if channel.rotations.is_empty()
        && let Some(value) = compact_base_rotation(interp)
    {
        channel.rotations.push(linear_keyframe(start, value));
    }
    if channel.scales.is_empty()
        && let Some(value) = compact_base_scale(interp)
    {
        channel.scales.push(linear_keyframe(start, vec![value]));
    }
    if channel.translations.is_empty() && channel.rotations.is_empty() && channel.scales.is_empty()
    {
        return Err(ChannelDecodeError::InvalidBaseValue {
            channel: "transform",
        });
    }
    Ok(channel)
}

fn read_tread_transform_channels(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    priority: u32,
) -> Result<Vec<BoneChannel>, ChannelDecodeError> {
    let data_ref = interp
        .block_ref_field("Data")
        .ok_or(ChannelDecodeError::MissingField("Data"))?;
    let data = nif
        .blocks
        .get(data_ref)
        .ok_or(ChannelDecodeError::InvalidReference {
            name: "Data",
            index: data_ref,
        })?;
    if data.type_name != "NiFloatData" {
        return Err(ChannelDecodeError::WrongBlockType {
            name: "Data",
            index: data_ref,
            actual: data.type_name.clone(),
            expected: "NiFloatData",
        });
    }
    let scalar_data = data
        .struct_field("Data")
        .ok_or(ChannelDecodeError::InvalidKeyData { channel: "tread" })?;
    let interpolation = interpolation_from_key_type(scalar_data.u32_or("Interpolation", 1));
    let scalar_keys = scalar_data.array_of_structs("Keys");
    if scalar_keys.is_empty() {
        return Err(ChannelDecodeError::InvalidKeyData { channel: "tread" });
    }

    let treads = interp.array_of_structs("Tread Transforms");
    if treads.is_empty() {
        return Err(ChannelDecodeError::InvalidKeyData { channel: "tread" });
    }
    treads
        .into_iter()
        .map(|tread| {
            let bone_name = tread.string_or("Name", "");
            if bone_name.trim().is_empty() {
                return Err(ChannelDecodeError::InvalidKeyData { channel: "tread" });
            }
            let first = read_tread_transform(&tread, "Transform 1")?;
            let second = read_tread_transform(&tread, "Transform 2")?;
            let translations = scalar_keys
                .iter()
                .map(|key| {
                    let amount = tread_scalar_value(key)?;
                    Ok(linear_or_tangent_keyframe(
                        key,
                        interpolation,
                        first
                            .translation
                            .iter()
                            .zip(second.translation)
                            .map(|(first, second)| first + (second - first) * amount)
                            .collect(),
                    ))
                })
                .collect::<Result<Vec<_>, ChannelDecodeError>>()?;
            let rotations = scalar_keys
                .iter()
                .map(|key| {
                    let amount = tread_scalar_value(key)?;
                    let value =
                        interpolate_quaternion_xyzw(first.rotation, second.rotation, amount)
                            .ok_or(ChannelDecodeError::InvalidBaseValue {
                                channel: "tread rotation",
                            })?;
                    Ok(linear_keyframe(key.f64_or("Time", 0.0), value.to_vec()))
                })
                .collect::<Result<Vec<_>, ChannelDecodeError>>()?;
            let scales = match (first.scale, second.scale) {
                (None, None) => Vec::new(),
                (Some(first), Some(second)) => scalar_keys
                    .iter()
                    .map(|key| {
                        let amount = tread_scalar_value(key)?;
                        Ok(linear_or_tangent_keyframe(
                            key,
                            interpolation,
                            vec![first + (second - first) * amount],
                        ))
                    })
                    .collect::<Result<Vec<_>, ChannelDecodeError>>()?,
                _ => {
                    return Err(ChannelDecodeError::InvalidBaseValue {
                        channel: "tread scale",
                    });
                }
            };
            Ok(BoneChannel {
                bone_name,
                priority,
                rotations,
                translations,
                scales,
            })
        })
        .collect()
}

#[derive(Clone, Copy)]
struct TreadTransform {
    translation: [f64; 3],
    rotation: [f64; 4],
    scale: Option<f64>,
}

fn read_tread_transform(
    tread: &KfEntry,
    field: &'static str,
) -> Result<TreadTransform, ChannelDecodeError> {
    let transform = tread
        .struct_field(field)
        .ok_or(ChannelDecodeError::MissingField(field))?;
    let translation = transform
        .struct_field("Translation")
        .ok_or(ChannelDecodeError::MissingField("Translation"))?;
    let translation = [
        translation.f64_or("x", f64::NAN),
        translation.f64_or("y", f64::NAN),
        translation.f64_or("z", f64::NAN),
    ];
    if !translation.iter().all(|value| valid_compact_float(*value)) {
        return Err(ChannelDecodeError::InvalidBaseValue {
            channel: "tread translation",
        });
    }
    let rotation = transform
        .struct_field("Rotation")
        .ok_or(ChannelDecodeError::MissingField("Rotation"))?;
    let rotation = normalize_quaternion_wxyz(&[
        rotation.f64_or("w", f64::NAN),
        rotation.f64_or("x", f64::NAN),
        rotation.f64_or("y", f64::NAN),
        rotation.f64_or("z", f64::NAN),
    ])
    .ok_or(ChannelDecodeError::InvalidBaseValue {
        channel: "tread rotation",
    })?;
    let scale = transform
        .f64_opt("Scale")
        .filter(|value| valid_compact_float(*value));
    Ok(TreadTransform {
        translation,
        rotation: [rotation[1], rotation[2], rotation[3], rotation[0]],
        scale,
    })
}

fn tread_scalar_value(key: &KfEntry) -> Result<f64, ChannelDecodeError> {
    key.f64_opt("Value")
        .filter(|value| value.is_finite())
        .ok_or(ChannelDecodeError::InvalidKeyData { channel: "tread" })
}

fn interpolate_quaternion_xyzw(
    first: [f64; 4],
    mut second: [f64; 4],
    amount: f64,
) -> Option<[f64; 4]> {
    let dot = first
        .iter()
        .zip(second)
        .map(|(first, second)| first * second)
        .sum::<f64>();
    if dot < 0.0 {
        for value in &mut second {
            *value = -*value;
        }
    }
    let mut value = [0.0; 4];
    for index in 0..4 {
        value[index] = first[index] + (second[index] - first[index]) * amount;
    }
    let norm = value.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= f64::EPSILON {
        return None;
    }
    for component in &mut value {
        *component /= norm;
    }
    Some(value)
}

fn float_control_points(
    spline_data: &nif_core_native::model::NifBlock,
) -> Result<Vec<f64>, ChannelDecodeError> {
    let Some(NifValue::Array(values)) = spline_data.get_field("Float Control Points") else {
        return Err(ChannelDecodeError::InvalidFloatControlPointArray);
    };
    values
        .iter()
        .map(|value| match value {
            NifValue::Float(value) if value.is_finite() => Some(*value),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(ChannelDecodeError::InvalidFloatControlPointArray)
}

fn read_float_spline_vector_channel(
    interp: &nif_core_native::model::NifBlock,
    float_points: &[f64],
    sampling: &CompactSplineSampling,
    num_control_points: usize,
    channel: &'static str,
    handle_field: &'static str,
    dimensions: usize,
) -> Result<Vec<AnimationKeyframe>, ChannelDecodeError> {
    let handle = interp
        .u32_field(handle_field)
        .unwrap_or(COMPACT_SPLINE_INVALID_HANDLE);
    if handle == COMPACT_SPLINE_INVALID_HANDLE {
        return Ok(Vec::new());
    }
    let required = num_control_points * dimensions;
    let end = (handle as usize)
        .checked_add(required)
        .filter(|end| *end <= float_points.len())
        .ok_or(CompactSplineError::ControlPointRange {
            channel,
            handle: handle as usize,
            required,
            available: float_points.len(),
        })?;
    let control = &float_points[handle as usize..end];
    let samples = sampling
        .weights
        .iter()
        .enumerate()
        .map(|(sample_index, weights)| {
            let mut value = vec![0.0; dimensions];
            for (control_index, weight) in weights.iter().copied().enumerate() {
                for component in 0..dimensions {
                    value[component] += weight * control[control_index * dimensions + component];
                }
            }
            if value.iter().all(|value| value.is_finite()) {
                Ok(value)
            } else {
                Err(CompactSplineError::NonFiniteSample {
                    channel,
                    sample: sample_index,
                })
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(sampling
        .times
        .iter()
        .copied()
        .zip(samples)
        .map(|(time, value)| linear_keyframe(time, value))
        .collect())
}

fn read_compact_spline_float_channel(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    target_name: &str,
    controller_type: &str,
    property_type: &str,
    controller_id: &str,
) -> Result<FloatChannel, ChannelDecodeError> {
    let keyframes = read_compact_spline_value(
        nif,
        interp,
        "float",
        "Handle",
        "Float Offset",
        "Float Half Range",
        1,
    )?;
    let keyframes = if keyframes.is_empty() {
        let value = interp
            .f64_field("Value")
            .filter(|value| valid_compact_float(*value))
            .ok_or(ChannelDecodeError::InvalidBaseValue { channel: "float" })?;
        vec![linear_keyframe(
            interp.f64_field("Start Time").unwrap_or(0.0),
            vec![value],
        )]
    } else {
        keyframes
    };
    Ok(FloatChannel {
        slot_name: canonical_float_slot_name(
            target_name,
            property_type,
            controller_type,
            controller_id,
            None,
        ),
        target_name: target_name.to_string(),
        property_type: property_type.to_string(),
        controller_type: controller_type.to_string(),
        controller_id: controller_id.to_string(),
        keyframes,
    })
}

fn read_compact_spline_point3_channels(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    target_name: &str,
    controller_type: &str,
    property_type: &str,
    controller_id: &str,
) -> Result<Vec<FloatChannel>, ChannelDecodeError> {
    let mut vector_keys = read_compact_spline_value(
        nif,
        interp,
        "point3",
        "Handle",
        "Position Offset",
        "Position Half Range",
        3,
    )?;
    if vector_keys.is_empty() {
        let value = interp
            .struct_field("Value")
            .map(|value| {
                vec![
                    value.f64_or("x", f64::NAN),
                    value.f64_or("y", f64::NAN),
                    value.f64_or("z", f64::NAN),
                ]
            })
            .filter(|value| value.iter().all(|value| valid_compact_float(*value)))
            .ok_or(ChannelDecodeError::InvalidBaseValue { channel: "point3" })?;
        vector_keys.push(linear_keyframe(
            interp.f64_field("Start Time").unwrap_or(0.0),
            value,
        ));
    }
    split_point3_float_channels(
        &vector_keys,
        target_name,
        controller_type,
        property_type,
        controller_id,
    )
}

fn read_point3_channels(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    target_name: &str,
    controller_type: &str,
    property_type: &str,
    controller_id: &str,
) -> Result<Vec<FloatChannel>, ChannelDecodeError> {
    let vector_keys = if let Some(data_ref) = interp.block_ref_field("Data") {
        let data = nif
            .blocks
            .get(data_ref)
            .ok_or(ChannelDecodeError::InvalidReference {
                name: "Data",
                index: data_ref,
            })?;
        if data.type_name != "NiPosData" {
            return Err(ChannelDecodeError::WrongBlockType {
                name: "Data",
                index: data_ref,
                actual: data.type_name.clone(),
                expected: "NiPosData",
            });
        }
        let group = data
            .struct_field("Data")
            .ok_or(ChannelDecodeError::InvalidKeyData { channel: "point3" })?;
        let interpolation = interpolation_from_key_type(group.u32_or("Interpolation", 1));
        let keys = group.array_of_structs("Keys");
        if keys.is_empty() {
            return Err(ChannelDecodeError::InvalidKeyData { channel: "point3" });
        }
        keys.into_iter()
            .map(|key| {
                let value = key
                    .struct_field("Value")
                    .map(|value| {
                        vec![
                            value.f64_or("x", f64::NAN),
                            value.f64_or("y", f64::NAN),
                            value.f64_or("z", f64::NAN),
                        ]
                    })
                    .filter(|value| value.iter().all(|value| value.is_finite()))
                    .ok_or(ChannelDecodeError::InvalidKeyData { channel: "point3" })?;
                Ok(AnimationKeyframe {
                    time: key.f64_or("Time", 0.0),
                    value,
                    interpolation,
                    forward: None,
                    backward: None,
                    tbc: None,
                })
            })
            .collect::<Result<Vec<_>, ChannelDecodeError>>()?
    } else {
        let value = interp
            .struct_field("Value")
            .map(|value| {
                vec![
                    value.f64_or("x", f64::NAN),
                    value.f64_or("y", f64::NAN),
                    value.f64_or("z", f64::NAN),
                ]
            })
            .filter(|value| value.iter().all(|value| valid_compact_float(*value)))
            .ok_or(ChannelDecodeError::InvalidBaseValue { channel: "point3" })?;
        vec![linear_keyframe(0.0, value)]
    };
    split_point3_float_channels(
        &vector_keys,
        target_name,
        controller_type,
        property_type,
        controller_id,
    )
}

fn split_point3_float_channels(
    vector_keys: &[AnimationKeyframe],
    target_name: &str,
    controller_type: &str,
    property_type: &str,
    controller_id: &str,
) -> Result<Vec<FloatChannel>, ChannelDecodeError> {
    ["x", "y", "z"]
        .into_iter()
        .enumerate()
        .map(|(component_index, component)| {
            let keyframes = vector_keys
                .iter()
                .map(|key| {
                    let value = *key
                        .value
                        .get(component_index)
                        .ok_or(ChannelDecodeError::NonScalarValue { channel: "point3" })?;
                    Ok(AnimationKeyframe {
                        value: vec![value],
                        ..key.clone()
                    })
                })
                .collect::<Result<Vec<_>, ChannelDecodeError>>()?;
            Ok(FloatChannel {
                slot_name: canonical_float_slot_name(
                    target_name,
                    property_type,
                    controller_type,
                    controller_id,
                    Some(component),
                ),
                target_name: target_name.to_string(),
                property_type: property_type.to_string(),
                controller_type: controller_type.to_string(),
                controller_id: controller_id.to_string(),
                keyframes,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn read_compact_spline_value(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    channel: &'static str,
    handle_field: &'static str,
    offset_field: &'static str,
    half_range_field: &'static str,
    dimensions: usize,
) -> Result<Vec<AnimationKeyframe>, ChannelDecodeError> {
    let handle = interp
        .u32_field(handle_field)
        .unwrap_or(COMPACT_SPLINE_INVALID_HANDLE);
    if handle == COMPACT_SPLINE_INVALID_HANDLE {
        return Ok(Vec::new());
    }
    let spline_ref = interp
        .block_ref_field("Spline Data")
        .ok_or(ChannelDecodeError::MissingField("Spline Data"))?;
    let spline_data = compact_spline_block(nif, spline_ref, "Spline Data", "NiBSplineData")?;
    let basis_ref = interp
        .block_ref_field("Basis Data")
        .ok_or(ChannelDecodeError::MissingField("Basis Data"))?;
    let basis_data = compact_spline_block(nif, basis_ref, "Basis Data", "NiBSplineBasisData")?;
    let num_control_points = basis_data
        .u32_field("Num Control Points")
        .ok_or(ChannelDecodeError::MissingField("Num Control Points"))?
        as usize;
    let start = interp
        .f64_field("Start Time")
        .ok_or(ChannelDecodeError::MissingField("Start Time"))?;
    let stop = interp
        .f64_field("Stop Time")
        .ok_or(ChannelDecodeError::MissingField("Stop Time"))?;
    let sampling = CompactSplineSampling::new(num_control_points, start, stop)?;
    let compact_points = compact_control_points(spline_data)?;
    read_compact_vector_channel(
        interp,
        &compact_points,
        &sampling,
        num_control_points,
        channel,
        handle_field,
        offset_field,
        half_range_field,
        dimensions,
    )
    .map_err(ChannelDecodeError::from)
}

fn compact_spline_block<'a>(
    nif: &'a NifFile,
    index: usize,
    name: &'static str,
    expected: &'static str,
) -> Result<&'a nif_core_native::model::NifBlock, CompactSplineError> {
    let block = nif
        .blocks
        .get(index)
        .ok_or(CompactSplineError::InvalidReference { name, index })?;
    if block.type_name != expected {
        return Err(CompactSplineError::WrongBlockType {
            name,
            index,
            actual: block.type_name.clone(),
            expected,
        });
    }
    Ok(block)
}

fn compact_control_points(
    spline_data: &nif_core_native::model::NifBlock,
) -> Result<Vec<i16>, CompactSplineError> {
    let Some(NifValue::Array(values)) = spline_data.get_field("Compact Control Points") else {
        return Err(CompactSplineError::InvalidControlPointArray);
    };
    values
        .iter()
        .map(|value| match value {
            NifValue::Int(value) => i16::try_from(*value).ok(),
            NifValue::UInt(value) => i16::try_from(*value).ok(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(CompactSplineError::InvalidControlPointArray)
}

#[allow(clippy::too_many_arguments)]
fn read_compact_vector_channel(
    interp: &nif_core_native::model::NifBlock,
    compact_points: &[i16],
    sampling: &CompactSplineSampling,
    num_control_points: usize,
    channel: &'static str,
    handle_field: &'static str,
    offset_field: &'static str,
    half_range_field: &'static str,
    dimensions: usize,
) -> Result<Vec<AnimationKeyframe>, CompactSplineError> {
    let handle = interp
        .u32_field(handle_field)
        .unwrap_or(COMPACT_SPLINE_INVALID_HANDLE);
    if handle == COMPACT_SPLINE_INVALID_HANDLE {
        return Ok(Vec::new());
    }
    let offset = interp
        .f64_field(offset_field)
        .ok_or(CompactSplineError::MissingField(offset_field))?;
    let half_range = interp
        .f64_field(half_range_field)
        .ok_or(CompactSplineError::MissingField(half_range_field))?;
    if !valid_compact_float(offset) || !valid_compact_float(half_range) {
        return Err(CompactSplineError::InvalidCompression {
            channel,
            offset,
            half_range,
        });
    }

    let samples = evaluate_compact_channel(
        compact_points,
        handle as usize,
        num_control_points,
        dimensions,
        offset,
        half_range,
        sampling,
        channel,
    )?;
    Ok(sampling
        .times
        .iter()
        .copied()
        .zip(samples)
        .map(|(time, value)| linear_keyframe(time, value))
        .collect())
}

fn read_compact_rotation_channel(
    interp: &nif_core_native::model::NifBlock,
    compact_points: &[i16],
    sampling: &CompactSplineSampling,
    num_control_points: usize,
) -> Result<Vec<AnimationKeyframe>, CompactSplineError> {
    let mut keys = read_compact_vector_channel(
        interp,
        compact_points,
        sampling,
        num_control_points,
        "rotation",
        "Rotation Handle",
        "Rotation Offset",
        "Rotation Half Range",
        4,
    )?;
    for (sample, key) in keys.iter_mut().enumerate() {
        let normalized = normalize_quaternion_wxyz(&key.value)
            .ok_or(CompactSplineError::DegenerateQuaternion(sample))?;
        key.value = vec![normalized[1], normalized[2], normalized[3], normalized[0]];
    }
    Ok(keys)
}

#[allow(clippy::too_many_arguments)]
fn evaluate_compact_channel(
    compact_points: &[i16],
    handle: usize,
    num_control_points: usize,
    dimensions: usize,
    offset: f64,
    half_range: f64,
    sampling: &CompactSplineSampling,
    channel: &'static str,
) -> Result<Vec<Vec<f64>>, CompactSplineError> {
    let required = num_control_points * dimensions;
    let end = handle
        .checked_add(required)
        .filter(|end| *end <= compact_points.len())
        .ok_or(CompactSplineError::ControlPointRange {
            channel,
            handle,
            required,
            available: compact_points.len(),
        })?;
    let control = &compact_points[handle..end];

    sampling
        .weights
        .iter()
        .enumerate()
        .map(|(sample_index, weights)| {
            let mut value = vec![0.0; dimensions];
            for (control_index, weight) in weights.iter().copied().enumerate() {
                for component in 0..dimensions {
                    value[component] += weight
                        * decompress_compact(
                            control[control_index * dimensions + component],
                            offset,
                            half_range,
                        );
                }
            }
            if value.iter().all(|component| component.is_finite()) {
                Ok(value)
            } else {
                Err(CompactSplineError::NonFiniteSample {
                    channel,
                    sample: sample_index,
                })
            }
        })
        .collect()
}

fn decompress_compact(value: i16, offset: f64, half_range: f64) -> f64 {
    offset + value as f64 * half_range / COMPACT_SPLINE_SHORT_MAX
}

fn open_uniform_bspline_weights(num_control_points: usize, parameter: f64) -> Vec<f64> {
    let order = COMPACT_SPLINE_DEGREE + 1;
    let endpoint = (num_control_points - COMPACT_SPLINE_DEGREE) as f64;
    if parameter >= endpoint {
        let mut weights = vec![0.0; num_control_points];
        weights[num_control_points - 1] = 1.0;
        return weights;
    }

    let knot_count = num_control_points + order;
    let mut knots = vec![0.0; knot_count];
    for (index, knot) in knots.iter_mut().enumerate() {
        *knot = if index < order {
            0.0
        } else if index < num_control_points {
            (index - order + 1) as f64
        } else {
            endpoint
        };
    }

    let mut basis = vec![0.0; knot_count - 1];
    for index in 0..basis.len() {
        if knots[index] <= parameter && parameter < knots[index + 1] {
            basis[index] = 1.0;
        }
    }
    for current_order in 2..=order {
        let basis_count = knot_count - current_order;
        let mut next = vec![0.0; knot_count - 1];
        for index in 0..basis_count {
            let left_denominator = knots[index + current_order - 1] - knots[index];
            let right_denominator = knots[index + current_order] - knots[index + 1];
            if left_denominator != 0.0 {
                next[index] += (parameter - knots[index]) / left_denominator * basis[index];
            }
            if right_denominator != 0.0 {
                next[index] += (knots[index + current_order] - parameter) / right_denominator
                    * basis[index + 1];
            }
        }
        basis = next;
    }
    basis.truncate(num_control_points);
    basis
}

fn compact_base_translation(interp: &nif_core_native::model::NifBlock) -> Option<Vec<f64>> {
    let translation = interp
        .struct_field("Transform")?
        .struct_field("Translation")?;
    let value = vec![
        translation.f64_or("x", f64::NAN),
        translation.f64_or("y", f64::NAN),
        translation.f64_or("z", f64::NAN),
    ];
    value
        .iter()
        .all(|value| valid_compact_float(*value))
        .then_some(value)
}

fn compact_base_rotation(interp: &nif_core_native::model::NifBlock) -> Option<Vec<f64>> {
    let rotation = interp.struct_field("Transform")?.struct_field("Rotation")?;
    let wxyz = vec![
        rotation.f64_or("w", f64::NAN),
        rotation.f64_or("x", f64::NAN),
        rotation.f64_or("y", f64::NAN),
        rotation.f64_or("z", f64::NAN),
    ];
    if !wxyz.iter().all(|value| valid_compact_float(*value)) {
        return None;
    }
    let normalized = normalize_quaternion_wxyz(&wxyz)?;
    Some(vec![
        normalized[1],
        normalized[2],
        normalized[3],
        normalized[0],
    ])
}

fn compact_base_scale(interp: &nif_core_native::model::NifBlock) -> Option<f64> {
    let value = interp.struct_field("Transform")?.f64_opt("Scale")?;
    valid_compact_float(value).then_some(value)
}

fn valid_compact_float(value: f64) -> bool {
    value.is_finite() && value.abs() < f32::MAX as f64
}

fn normalize_quaternion_wxyz(value: &[f64]) -> Option<[f64; 4]> {
    let &[w, x, y, z, ..] = value else {
        return None;
    };
    let norm = (w * w + x * x + y * y + z * z).sqrt();
    if !norm.is_finite() || norm <= f64::EPSILON {
        return None;
    }
    Some([w / norm, x / norm, y / norm, z / norm])
}

fn linear_keyframe(time: f64, value: Vec<f64>) -> AnimationKeyframe {
    AnimationKeyframe {
        time,
        value,
        interpolation: Interpolation::Linear,
        forward: None,
        backward: None,
        tbc: None,
    }
}

fn read_transform_channel(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    bone_name: &str,
    priority: u32,
    warnings: &mut Vec<String>,
) -> Result<BoneChannel, ChannelDecodeError> {
    let start = interp.f64_field("Start Time").unwrap_or(0.0);
    let mut channel = BoneChannel {
        bone_name: bone_name.to_string(),
        priority,
        rotations: compact_base_rotation(interp)
            .map(|value| vec![linear_keyframe(start, value)])
            .unwrap_or_default(),
        translations: compact_base_translation(interp)
            .map(|value| vec![linear_keyframe(start, value)])
            .unwrap_or_default(),
        scales: compact_base_scale(interp)
            .map(|value| vec![linear_keyframe(start, vec![value])])
            .unwrap_or_default(),
    };

    let data_ref = match interp.block_ref_field("Data") {
        Some(r) => r,
        None => {
            if channel.rotations.is_empty()
                && channel.translations.is_empty()
                && channel.scales.is_empty()
            {
                return Err(ChannelDecodeError::InvalidBaseValue {
                    channel: "transform",
                });
            }
            return Ok(channel);
        }
    };

    let data = match nif.blocks.get(data_ref) {
        Some(b) if b.type_name == "NiTransformData" => b,
        Some(b) => {
            return Err(ChannelDecodeError::WrongBlockType {
                name: "Data",
                index: data_ref,
                actual: b.type_name.clone(),
                expected: "NiTransformData",
            });
        }
        None => {
            return Err(ChannelDecodeError::InvalidReference {
                name: "Data",
                index: data_ref,
            });
        }
    };

    let rotations = read_rotation_keys(data, bone_name, warnings);
    if !rotations.is_empty() {
        channel.rotations = rotations;
    }
    let translations = read_translation_keys(data);
    if !translations.is_empty() {
        channel.translations = translations;
    }
    let scales = read_scale_keys(data);
    if !scales.is_empty() {
        channel.scales = scales;
    }
    if channel.rotations.is_empty() && channel.translations.is_empty() && channel.scales.is_empty()
    {
        return Err(ChannelDecodeError::InvalidKeyData {
            channel: "transform",
        });
    }
    Ok(channel)
}

fn read_rotation_keys(
    data: &nif_core_native::model::NifBlock,
    bone_name: &str,
    warnings: &mut Vec<String>,
) -> Vec<AnimationKeyframe> {
    let rot_type = data.u32_field("Rotation Type").unwrap_or(0);
    let num_keys = data.u32_field("Num Rotation Keys").unwrap_or(0);

    if num_keys == 0 && rot_type != 4 {
        return Vec::new();
    }

    if rot_type == 4 {
        return read_xyz_rotation_keys(data, bone_name, warnings);
    }

    let interp = match rot_type {
        2 => Interpolation::Quadratic,
        3 => Interpolation::Tbc,
        _ => Interpolation::Linear,
    };

    let mut keyframes = Vec::new();
    for qk in data.array_of_structs("Quaternion Keys") {
        let time = qk.f64_or("Time", 0.0);
        let val = qk.struct_field("Value");
        let w = val.as_ref().map_or(1.0, |v| v.f64_or("w", 1.0));
        let x = val.as_ref().map_or(0.0, |v| v.f64_or("x", 0.0));
        let y = val.as_ref().map_or(0.0, |v| v.f64_or("y", 0.0));
        let z = val.as_ref().map_or(0.0, |v| v.f64_or("z", 0.0));
        let tbc = if interp == Interpolation::Tbc {
            qk.struct_field("TBC")
                .map(|t| (t.f64_or("t", 0.0), t.f64_or("b", 0.0), t.f64_or("c", 0.0)))
        } else {
            None
        };
        keyframes.push(AnimationKeyframe {
            time,
            value: vec![x, y, z, w],
            interpolation: interp,
            forward: None,
            backward: None,
            tbc,
        });
    }
    keyframes
}

fn read_xyz_rotation_keys(
    data: &nif_core_native::model::NifBlock,
    bone_name: &str,
    warnings: &mut Vec<String>,
) -> Vec<AnimationKeyframe> {
    let xyz_entries = data.array_of_structs("XYZ Rotations");
    if xyz_entries.len() < 3 {
        warnings.push(format!("Missing XYZ Rotations data for '{bone_name}'"));
        return Vec::new();
    }

    let mut axis_keys: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut all_times: Vec<f64> = Vec::new();

    for axis in xyz_entries.iter().take(3) {
        let pairs: Vec<(f64, f64)> = axis
            .array_of_structs("Keys")
            .iter()
            .map(|k| (k.f64_or("Time", 0.0), k.f64_or("Value", 0.0)))
            .collect();
        for (t, _) in &pairs {
            if !all_times.iter().any(|x| (x - t).abs() < 1e-12) {
                all_times.push(*t);
            }
        }
        axis_keys.push(pairs);
    }

    all_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    all_times
        .iter()
        .map(|&t| {
            let rx = sample_at_time(&axis_keys[0], t);
            let ry = sample_at_time(&axis_keys[1], t);
            let rz = sample_at_time(&axis_keys[2], t);
            let (qx, qy, qz, qw) = euler_to_quat(rx, ry, rz);
            AnimationKeyframe {
                time: t,
                value: vec![qx, qy, qz, qw],
                interpolation: Interpolation::Linear,
                forward: None,
                backward: None,
                tbc: None,
            }
        })
        .collect()
}

fn read_translation_keys(data: &nif_core_native::model::NifBlock) -> Vec<AnimationKeyframe> {
    let trans = match data.struct_field("Translations") {
        Some(t) => t,
        None => return Vec::new(),
    };
    if trans.u32_or("Num Keys", 0) == 0 {
        return Vec::new();
    }
    let interp_val = trans.u32_or("Interpolation", 1);
    let interp = match interp_val {
        2 => Interpolation::Quadratic,
        3 => Interpolation::Tbc,
        _ => Interpolation::Linear,
    };

    trans
        .array_of_structs("Keys")
        .iter()
        .map(|k| {
            let time = k.f64_or("Time", 0.0);
            let val = k.struct_field("Value");
            let x = val.as_ref().map_or(0.0, |v| v.f64_or("x", 0.0));
            let y = val.as_ref().map_or(0.0, |v| v.f64_or("y", 0.0));
            let z = val.as_ref().map_or(0.0, |v| v.f64_or("z", 0.0));
            let forward = if interp == Interpolation::Quadratic {
                k.struct_field("Forward")
                    .map(|f| vec![f.f64_or("x", 0.0), f.f64_or("y", 0.0), f.f64_or("z", 0.0)])
            } else {
                None
            };
            let backward = if interp == Interpolation::Quadratic {
                k.struct_field("Backward")
                    .map(|b| vec![b.f64_or("x", 0.0), b.f64_or("y", 0.0), b.f64_or("z", 0.0)])
            } else {
                None
            };
            let tbc = if interp == Interpolation::Tbc {
                k.struct_field("TBC")
                    .map(|t| (t.f64_or("t", 0.0), t.f64_or("b", 0.0), t.f64_or("c", 0.0)))
            } else {
                None
            };
            AnimationKeyframe {
                time,
                value: vec![x, y, z],
                interpolation: interp,
                forward,
                backward,
                tbc,
            }
        })
        .collect()
}

fn read_scale_keys(data: &nif_core_native::model::NifBlock) -> Vec<AnimationKeyframe> {
    let scales = match data.struct_field("Scales") {
        Some(s) => s,
        None => return Vec::new(),
    };
    if scales.u32_or("Num Keys", 0) == 0 {
        return Vec::new();
    }
    let keys = scales.array_of_structs("Keys");
    let interp = if keys.first().map_or(false, |k| k.has_field("Forward")) {
        Interpolation::Quadratic
    } else if keys.first().map_or(false, |k| k.has_field("TBC")) {
        Interpolation::Tbc
    } else {
        Interpolation::Linear
    };

    keys.iter()
        .map(|k| {
            let time = k.f64_or("Time", 0.0);
            let val = k.f64_or("Value", 1.0);
            let forward = k.f64_opt("Forward").map(|f| vec![f]);
            let backward = k.f64_opt("Backward").map(|b| vec![b]);
            let tbc = if interp == Interpolation::Tbc {
                k.struct_field("TBC")
                    .map(|t| (t.f64_or("t", 0.0), t.f64_or("b", 0.0), t.f64_or("c", 0.0)))
            } else {
                None
            };
            AnimationKeyframe {
                time,
                value: vec![val],
                interpolation: interp,
                forward,
                backward,
                tbc,
            }
        })
        .collect()
}

fn read_float_channel(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    target_name: &str,
    controller_type: &str,
    property_type: &str,
    controller_id: &str,
) -> Result<FloatChannel, ChannelDecodeError> {
    let is_bool = matches!(
        interp.type_name.as_str(),
        "NiBoolInterpolator" | "NiBoolTimelineInterpolator"
    );
    let keyframes = if let Some(data_ref) = interp.block_ref_field("Data") {
        let data = nif
            .blocks
            .get(data_ref)
            .ok_or(ChannelDecodeError::InvalidReference {
                name: "Data",
                index: data_ref,
            })?;
        let expected = if is_bool { "NiBoolData" } else { "NiFloatData" };
        if data.type_name != expected {
            return Err(ChannelDecodeError::WrongBlockType {
                name: "Data",
                index: data_ref,
                actual: data.type_name.clone(),
                expected,
            });
        }
        let group = data
            .struct_field("Data")
            .ok_or(ChannelDecodeError::InvalidKeyData { channel: "scalar" })?;
        let interpolation = if is_bool {
            Interpolation::Constant
        } else {
            interpolation_from_key_type(group.u32_or("Interpolation", 1))
        };
        let keys = group.array_of_structs("Keys");
        if keys.is_empty() {
            return Err(ChannelDecodeError::InvalidKeyData { channel: "scalar" });
        }
        keys.into_iter()
            .map(|key| {
                let value = key
                    .f64_opt("Value")
                    .ok_or(ChannelDecodeError::InvalidKeyData { channel: "scalar" })?;
                if !value.is_finite() {
                    return Err(ChannelDecodeError::InvalidKeyData { channel: "scalar" });
                }
                Ok(linear_or_tangent_keyframe(&key, interpolation, vec![value]))
            })
            .collect::<Result<Vec<_>, ChannelDecodeError>>()?
    } else {
        let value = interp
            .f64_field("Value")
            .filter(|value| value.is_finite() && (!is_bool || *value <= 1.0))
            .ok_or(ChannelDecodeError::InvalidBaseValue { channel: "scalar" })?;
        vec![linear_keyframe(
            interp.f64_field("Start Time").unwrap_or(0.0),
            vec![value],
        )]
    };

    let prop = if property_type.is_empty() {
        if controller_type.contains("Visibility") {
            "visibility"
        } else if controller_type.contains("Alpha") {
            "alpha"
        } else {
            "float"
        }
    } else {
        property_type
    };

    Ok(FloatChannel {
        slot_name: canonical_float_slot_name(
            target_name,
            prop,
            controller_type,
            controller_id,
            None,
        ),
        target_name: target_name.to_string(),
        property_type: prop.to_string(),
        controller_type: controller_type.to_string(),
        controller_id: controller_id.to_string(),
        keyframes,
    })
}

fn interpolation_from_key_type(value: u32) -> Interpolation {
    match value {
        2 => Interpolation::Quadratic,
        3 => Interpolation::Tbc,
        5 => Interpolation::Constant,
        _ => Interpolation::Linear,
    }
}

fn linear_or_tangent_keyframe(
    key: &KfEntry,
    interpolation: Interpolation,
    value: Vec<f64>,
) -> AnimationKeyframe {
    let forward = (interpolation == Interpolation::Quadratic)
        .then(|| key.f64_opt("Forward").map(|value| vec![value]))
        .flatten();
    let backward = (interpolation == Interpolation::Quadratic)
        .then(|| key.f64_opt("Backward").map(|value| vec![value]))
        .flatten();
    let tbc = (interpolation == Interpolation::Tbc)
        .then(|| {
            key.struct_field("TBC").map(|tbc| {
                (
                    tbc.f64_or("t", 0.0),
                    tbc.f64_or("b", 0.0),
                    tbc.f64_or("c", 0.0),
                )
            })
        })
        .flatten();
    AnimationKeyframe {
        time: key.f64_or("Time", 0.0),
        value,
        interpolation,
        forward,
        backward,
        tbc,
    }
}

fn canonical_float_slot_name(
    target_name: &str,
    property_type: &str,
    controller_type: &str,
    controller_id: &str,
    component: Option<&str>,
) -> String {
    let mut name = format!(
        "{}|{}|{}|{}",
        target_name.trim(),
        property_type.trim(),
        controller_type.trim(),
        controller_id.trim()
    );
    if let Some(component) = component {
        name.push('|');
        name.push_str(component);
    }
    name
}

fn qualify_float_slot(channel: &mut FloatChannel, controlled_index: usize) {
    channel
        .slot_name
        .push_str(&format!("|controlled:{controlled_index}"));
}

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

fn sample_at_time(pairs: &[(f64, f64)], t: f64) -> f64 {
    if pairs.is_empty() {
        return 0.0;
    }
    if pairs.len() == 1 || t <= pairs[0].0 {
        return pairs[0].1;
    }
    if t >= pairs[pairs.len() - 1].0 {
        return pairs[pairs.len() - 1].1;
    }
    for i in 0..pairs.len() - 1 {
        let (t0, v0) = pairs[i];
        let (t1, v1) = pairs[i + 1];
        if t0 <= t && t <= t1 {
            if (t1 - t0).abs() < 1e-9 {
                return v0;
            }
            return v0 + (t - t0) / (t1 - t0) * (v1 - v0);
        }
    }
    pairs[pairs.len() - 1].1
}

fn euler_to_quat(rx: f64, ry: f64, rz: f64) -> (f64, f64, f64, f64) {
    let (cx, sx) = ((rx / 2.0).cos(), (rx / 2.0).sin());
    let (cy, sy) = ((ry / 2.0).cos(), (ry / 2.0).sin());
    let (cz, sz) = ((rz / 2.0).cos(), (rz / 2.0).sin());
    let w = cx * cy * cz + sx * sy * sz;
    let x = sx * cy * cz - cx * sy * sz;
    let y = cx * sy * cz + sx * cy * sz;
    let z = cx * cy * sz - sx * sy * cz;
    (x, y, z, w)
}

// ---------------------------------------------------------------------------
// Havok animation XML generation
//
// Produces a minimal hkaInterleavedUncompressedAnimation XML that
// havok_native::api::havok_xml_to_hkx can pack to binary .hkx.
// ---------------------------------------------------------------------------

fn interp_keyframes(keys: &[AnimationKeyframe], t: f64) -> Vec<f64> {
    if keys.is_empty() {
        return Vec::new();
    }
    if keys.len() == 1 || t <= keys[0].time {
        return keys[0].value.clone();
    }
    if t >= keys[keys.len() - 1].time {
        return keys[keys.len() - 1].value.clone();
    }
    for i in 0..keys.len() - 1 {
        let (t0, t1) = (keys[i].time, keys[i + 1].time);
        if t0 <= t && t <= t1 {
            if keys[i].interpolation == Interpolation::Constant {
                if t < t1 {
                    return keys[i].value.clone();
                }
                continue;
            }
            if (t1 - t0).abs() < 1e-9 {
                return keys[i].value.clone();
            }
            let frac = (t - t0) / (t1 - t0);
            return keys[i]
                .value
                .iter()
                .zip(keys[i + 1].value.iter())
                .map(|(a, b)| a + frac * (b - a))
                .collect();
        }
    }
    keys[keys.len() - 1].value.clone()
}

fn sample_translation(keys: &[AnimationKeyframe], t: f64) -> (f64, f64, f64) {
    let v = interp_keyframes(keys, t);
    match v.as_slice() {
        [x, y, z, ..] => (*x, *y, *z),
        [x, y] => (*x, *y, 0.0),
        [x] => (*x, 0.0, 0.0),
        _ => (0.0, 0.0, 0.0),
    }
}

fn sample_rotation(keys: &[AnimationKeyframe], t: f64) -> (f64, f64, f64, f64) {
    let normalize = |value: &[f64]| {
        let [x, y, z, w, ..] = value else {
            return (0.0, 0.0, 0.0, 1.0);
        };
        let norm = (x * x + y * y + z * z + w * w).sqrt();
        if !norm.is_finite() || norm <= f64::EPSILON {
            (0.0, 0.0, 0.0, 1.0)
        } else {
            (x / norm, y / norm, z / norm, w / norm)
        }
    };
    if keys.is_empty() {
        return (0.0, 0.0, 0.0, 1.0);
    }
    if keys.len() == 1 || t <= keys[0].time {
        return normalize(&keys[0].value);
    }
    if t >= keys[keys.len() - 1].time {
        return normalize(&keys[keys.len() - 1].value);
    }
    for pair in keys.windows(2) {
        let (first, second) = (&pair[0], &pair[1]);
        if first.time <= t && t <= second.time {
            if (second.time - first.time).abs() < 1.0e-9 {
                return normalize(&first.value);
            }
            let first = normalize(&first.value);
            let mut second = normalize(&second.value);
            let dot =
                first.0 * second.0 + first.1 * second.1 + first.2 * second.2 + first.3 * second.3;
            if dot < 0.0 {
                second = (-second.0, -second.1, -second.2, -second.3);
            }
            let fraction = (t - pair[0].time) / (pair[1].time - pair[0].time);
            return normalize(&[
                first.0 + fraction * (second.0 - first.0),
                first.1 + fraction * (second.1 - first.1),
                first.2 + fraction * (second.2 - first.2),
                first.3 + fraction * (second.3 - first.3),
            ]);
        }
    }
    normalize(&keys[keys.len() - 1].value)
}

fn sample_scale_val(keys: &[AnimationKeyframe], t: f64) -> f64 {
    let v = interp_keyframes(keys, t);
    v.first().copied().unwrap_or(1.0)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn target_sample_count(duration: f64, target_sample_rate_hz: f64) -> usize {
    if duration <= 0.0 {
        return 1;
    }
    ((duration * target_sample_rate_hz).round() as usize).max(1) + 1
}

#[derive(Debug)]
struct PlanarRootMotion {
    root_track_index: usize,
    root_bone_index: usize,
    samples: Vec<[f64; 4]>,
    has_yaw: bool,
    start_rotation: (f64, f64, f64, f64),
}

struct Fo4AnimationClassLayout {
    root_level_container_signature: String,
    animation_container_signature: String,
    interleaved_animation_signature: String,
    animation_binding_signature: String,
    default_reference_frame_signature: String,
}

fn require_fo4_class_layout(
    registry: &mut havok_native::hkx::descriptors::DescriptorRegistry,
    class: &str,
    required_members: &[&str],
) -> Result<String, FnvKfConversionError> {
    let signature = registry
        .get(class)
        .map_err(|source| FnvKfConversionError::DescriptorLookup {
            class: class.to_string(),
            message: source.to_string(),
        })?
        .ok_or_else(|| FnvKfConversionError::MissingDescriptor {
            class: class.to_string(),
        })?
        .signature
        .clone();
    let members = registry.get_all_members(class).map_err(|source| {
        FnvKfConversionError::DescriptorLookup {
            class: class.to_string(),
            message: source.to_string(),
        }
    })?;
    for required_member in required_members {
        if !members.iter().any(|member| member.name == *required_member) {
            return Err(FnvKfConversionError::MissingDescriptorMember {
                class: class.to_string(),
                member: (*required_member).to_string(),
            });
        }
    }
    Ok(signature)
}

fn load_fo4_animation_class_layout() -> Result<Fo4AnimationClassLayout, FnvKfConversionError> {
    let mut registry =
        havok_native::hkx::descriptors::DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
    let root_level_container_signature =
        require_fo4_class_layout(&mut registry, "hkRootLevelContainer", &["namedVariants"])?;
    require_fo4_class_layout(
        &mut registry,
        "hkRootLevelContainerNamedVariant",
        &["name", "className", "variant"],
    )?;
    let animation_container_signature = require_fo4_class_layout(
        &mut registry,
        "hkaAnimationContainer",
        &[
            "skeletons",
            "animations",
            "bindings",
            "attachments",
            "skins",
        ],
    )?;
    let interleaved_animation_signature = require_fo4_class_layout(
        &mut registry,
        "hkaInterleavedUncompressedAnimation",
        &[
            "type",
            "duration",
            "numberOfTransformTracks",
            "numberOfFloatTracks",
            "extractedMotion",
            "annotationTracks",
            "transforms",
            "floats",
        ],
    )?;
    require_fo4_class_layout(
        &mut registry,
        "hkaAnnotationTrack",
        &["trackName", "annotations"],
    )?;
    require_fo4_class_layout(
        &mut registry,
        "hkaAnnotationTrackAnnotation",
        &["time", "text"],
    )?;
    let animation_binding_signature = require_fo4_class_layout(
        &mut registry,
        "hkaAnimationBinding",
        &[
            "originalSkeletonName",
            "animation",
            "transformTrackToBoneIndices",
            "floatTrackToFloatSlotIndices",
            "partitionIndices",
            "blendHint",
        ],
    )?;
    let default_reference_frame_signature = require_fo4_class_layout(
        &mut registry,
        "hkaDefaultAnimatedReferenceFrame",
        &["up", "forward", "duration", "referenceFrameSamples"],
    )?;
    Ok(Fo4AnimationClassLayout {
        root_level_container_signature,
        animation_container_signature,
        interleaved_animation_signature,
        animation_binding_signature,
        default_reference_frame_signature,
    })
}

fn extract_planar_root_motion(
    clip: &AnimationClip,
    declaration: &ClipDecl,
    sample_count: usize,
    extracted_motion_policy: FnvExtractedMotionPolicy,
) -> Result<Option<PlanarRootMotion>, FnvKfConversionError> {
    extract_planar_root_motion_from_mapping(
        clip,
        &declaration.binding.transform_track_to_bone_indices,
        sample_count,
        extracted_motion_policy,
    )
}

fn extract_planar_root_motion_from_mapping(
    clip: &AnimationClip,
    transform_track_to_bone_indices: &[usize],
    sample_count: usize,
    extracted_motion_policy: FnvExtractedMotionPolicy,
) -> Result<Option<PlanarRootMotion>, FnvKfConversionError> {
    if clip.accum_root.trim().is_empty() {
        return Ok(None);
    }
    let normalized_accum_root = normalize_fnv_bone_name(&clip.accum_root);
    let root_track_index = clip
        .channels
        .iter()
        .position(|channel| normalize_fnv_bone_name(&channel.bone_name) == normalized_accum_root);
    let Some(root_track_index) = root_track_index else {
        return match extracted_motion_policy {
            FnvExtractedMotionPolicy::RejectNonzero => Ok(None),
            FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame => {
                Err(FnvKfConversionError::MissingAccumRootTrack {
                    name: clip.accum_root.clone(),
                })
            }
        };
    };
    let root_bone_index = transform_track_to_bone_indices[root_track_index];
    let root_channel = &clip.channels[root_track_index];
    let start = sample_translation(&root_channel.translations, 0.0);
    let start_rotation = sample_rotation(&root_channel.rotations, 0.0);
    let mut samples = Vec::with_capacity(sample_count);
    let mut max_planar_delta = 0.0_f64;
    let mut max_vertical_delta = 0.0_f64;
    let mut max_pitch_delta = 0.0_f64;
    let mut max_roll_delta = 0.0_f64;
    let mut max_yaw_delta = 0.0_f64;
    let mut previous_yaw = 0.0_f64;
    for frame in 0..sample_count {
        let time = if sample_count > 1 {
            clip.duration * frame as f64 / (sample_count - 1) as f64
        } else {
            0.0
        };
        let translation = sample_translation(&root_channel.translations, time);
        let planar = [translation.0 - start.0, translation.1 - start.1];
        let rotation = sample_rotation(&root_channel.rotations, time);
        let delta = quaternion_multiply(quaternion_conjugate(start_rotation), rotation);
        let (roll, pitch, mut yaw) = quaternion_to_euler(delta);
        while yaw - previous_yaw > std::f64::consts::PI {
            yaw -= std::f64::consts::TAU;
        }
        while yaw - previous_yaw < -std::f64::consts::PI {
            yaw += std::f64::consts::TAU;
        }
        previous_yaw = yaw;
        max_planar_delta = max_planar_delta.max(planar[0].hypot(planar[1]));
        max_vertical_delta = max_vertical_delta.max((translation.2 - start.2).abs());
        max_roll_delta = max_roll_delta.max(roll.abs());
        max_pitch_delta = max_pitch_delta.max(pitch.abs());
        max_yaw_delta = max_yaw_delta.max(yaw.abs());
        samples.push([planar[0], planar[1], 0.0, yaw]);
    }

    if max_vertical_delta > 1.0e-4 {
        return Err(FnvKfConversionError::UnsupportedVerticalRootMotion {
            track: root_track_index,
            name: root_channel.bone_name.clone(),
            max_delta: max_vertical_delta,
        });
    }
    if max_pitch_delta > 1.0e-4 || max_roll_delta > 1.0e-4 {
        return Err(FnvKfConversionError::UnsupportedPitchRollRootMotion {
            track: root_track_index,
            name: root_channel.bone_name.clone(),
            max_pitch: max_pitch_delta,
            max_roll: max_roll_delta,
        });
    }
    let has_yaw = max_yaw_delta > 1.0e-4;
    if max_planar_delta <= 1.0e-4 && !has_yaw {
        return Ok(None);
    }
    Ok(Some(PlanarRootMotion {
        root_track_index,
        root_bone_index,
        samples,
        has_yaw,
        start_rotation,
    }))
}

fn quaternion_conjugate(value: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    (-value.0, -value.1, -value.2, value.3)
}

fn quaternion_multiply(
    left: (f64, f64, f64, f64),
    right: (f64, f64, f64, f64),
) -> (f64, f64, f64, f64) {
    (
        left.3 * right.0 + left.0 * right.3 + left.1 * right.2 - left.2 * right.1,
        left.3 * right.1 - left.0 * right.2 + left.1 * right.3 + left.2 * right.0,
        left.3 * right.2 + left.0 * right.1 - left.1 * right.0 + left.2 * right.3,
        left.3 * right.3 - left.0 * right.0 - left.1 * right.1 - left.2 * right.2,
    )
}

fn quaternion_to_euler(value: (f64, f64, f64, f64)) -> (f64, f64, f64) {
    let (x, y, z, w) = value;
    let roll = (2.0 * (w * x + y * z)).atan2(1.0 - 2.0 * (x * x + y * y));
    let pitch_term = (2.0 * (w * y - z * x)).clamp(-1.0, 1.0);
    let pitch = pitch_term.asin();
    let yaw = (2.0 * (w * z + x * y)).atan2(1.0 - 2.0 * (y * y + z * z));
    (roll, pitch, yaw)
}

fn clip_to_havok_xml(
    clip: &AnimationClip,
    declaration: &ClipDecl,
    original_skeleton_name: &str,
    target_sample_rate_hz: f64,
    extracted_motion_policy: FnvExtractedMotionPolicy,
) -> Result<(String, FnvClipMotion), FnvKfConversionError> {
    clip_to_havok_xml_with_float_tracks(
        clip,
        declaration,
        &[],
        original_skeleton_name,
        target_sample_rate_hz,
        extracted_motion_policy,
    )
}

fn clip_to_havok_xml_with_float_tracks(
    clip: &AnimationClip,
    declaration: &ClipDecl,
    float_track_to_float_slot_indices: &[usize],
    original_skeleton_name: &str,
    target_sample_rate_hz: f64,
    extracted_motion_policy: FnvExtractedMotionPolicy,
) -> Result<(String, FnvClipMotion), FnvKfConversionError> {
    let num_bones = clip.channels.len();
    let declared = declaration.binding.declared_transform_tracks;
    let mapped = declaration.binding.transform_track_to_bone_indices.len();
    if declared != num_bones || mapped != num_bones {
        return Err(FnvKfConversionError::InvalidBindingTrackCount {
            declared,
            mapped,
            channels: num_bones,
        });
    }
    if float_track_to_float_slot_indices.len() != clip.float_channels.len() {
        return Err(FnvKfConversionError::InvalidFloatBindingTrackCount {
            mapped: float_track_to_float_slot_indices.len(),
            channels: clip.float_channels.len(),
        });
    }
    if original_skeleton_name.trim().is_empty() {
        return Err(FnvKfConversionError::EmptyOriginalSkeletonName);
    }
    if !target_sample_rate_hz.is_finite() || target_sample_rate_hz <= 0.0 {
        return Err(FnvKfConversionError::InvalidTargetSampleRate(
            target_sample_rate_hz,
        ));
    }

    let duration = clip.duration.max(0.0);
    let num_frames = target_sample_count(duration, target_sample_rate_hz);
    let root_motion =
        extract_planar_root_motion(clip, declaration, num_frames, extracted_motion_policy)?;
    let class_layout = load_fo4_animation_class_layout()?;

    // Build per-frame interleaved transforms: bone0@t0, bone1@t0, ..., bone0@t1, ...
    let mut transform_data = String::new();
    for frame in 0..num_frames {
        let t = if num_frames > 1 {
            duration * frame as f64 / (num_frames - 1) as f64
        } else {
            0.0
        };
        for (track_index, ch) in clip.channels.iter().enumerate() {
            let (mut tx, mut ty, tz) = sample_translation(&ch.translations, t);
            if root_motion
                .as_ref()
                .is_some_and(|motion| motion.root_track_index == track_index)
            {
                tx = 0.0;
                ty = 0.0;
            }
            let (qx, qy, qz, qw) = if let Some(motion) = root_motion
                .as_ref()
                .filter(|motion| motion.root_track_index == track_index && motion.has_yaw)
            {
                motion.start_rotation
            } else {
                sample_rotation(&ch.rotations, t)
            };
            let scale = sample_scale_val(&ch.scales, t);
            transform_data.push_str(&format!(
                "\n                    ({tx:.6} {ty:.6} {tz:.6})\
                 ({qx:.6} {qy:.6} {qz:.6} {qw:.6})\
                 ({scale:.6} {scale:.6} {scale:.6})"
            ));
        }
    }
    let total_transforms = num_frames * num_bones;
    let num_float_tracks = clip.float_channels.len();
    let mut float_data = String::new();
    for frame in 0..num_frames {
        let time = if num_frames > 1 {
            duration * frame as f64 / (num_frames - 1) as f64
        } else {
            0.0
        };
        for (track, channel) in clip.float_channels.iter().enumerate() {
            let value = interp_keyframes(&channel.keyframes, time)
                .first()
                .copied()
                .filter(|value| value.is_finite())
                .ok_or(FnvKfConversionError::InvalidFloatSample { track, frame })?;
            float_data.push_str(&format!("\n                    {value:.6}"));
        }
    }
    let total_float_samples = num_frames * num_float_tracks;

    // Build per-bone annotation tracks (events go on first track only).
    let mut annotation_section = String::new();
    for (i, ch) in clip.channels.iter().enumerate() {
        let annot_content = if i == 0 && !clip.events.is_empty() {
            let count = clip.events.len();
            let mut rows = String::new();
            for ev in &clip.events {
                rows.push_str(&format!(
                    "\n                            <hkobject>\
                     \n                                <hkparam name=\"time\">{:.6}</hkparam>\
                     \n                                <hkparam name=\"text\">{}</hkparam>\
                     \n                            </hkobject>",
                    ev.time,
                    xml_escape(&ev.text)
                ));
            }
            format!(
                "\n                    <hkparam name=\"annotations\" numelements=\"{count}\">{rows}\n                    </hkparam>"
            )
        } else {
            "\n                    <hkparam name=\"annotations\" numelements=\"0\"></hkparam>"
                .to_string()
        };
        annotation_section.push_str(&format!(
            "\n                <hkobject>\
             \n                    <hkparam name=\"trackName\">{}</hkparam>{annot_content}\
             \n                </hkobject>",
            xml_escape(&ch.bone_name)
        ));
    }

    let bone_indices: String = declaration
        .binding
        .transform_track_to_bone_indices
        .iter()
        .map(|index| format!("\n                    {index}"))
        .collect();
    let float_slot_indices: String = float_track_to_float_slot_indices
        .iter()
        .map(|index| format!("\n                    {index}"))
        .collect();

    let (extracted_motion_ref, reference_frame_section, motion) = match root_motion {
        Some(root_motion) => {
            let end_sample = root_motion
                .samples
                .last()
                .copied()
                .unwrap_or([0.0, 0.0, 0.0, 0.0]);
            let end_displacement = [end_sample[0], end_sample[1]];
            let reference_frame_samples = root_motion
                .samples
                .iter()
                .map(|sample| {
                    format!(
                        "\n                    ({:.6} {:.6} {:.6} {:.6})",
                        sample[0], sample[1], sample[2], sample[3]
                    )
                })
                .collect::<String>();
            let section = format!(
                r##"
        <hkobject name="#0005" class="hkaDefaultAnimatedReferenceFrame" signature="{reference_frame_signature}">
            <hkparam name="up">(0.000000 0.000000 1.000000 0.000000)</hkparam>
            <hkparam name="forward">(0.000000 1.000000 0.000000 0.000000)</hkparam>
            <hkparam name="duration">{duration:.6}</hkparam>
            <hkparam name="referenceFrameSamples" numelements="{num_frames}">{reference_frame_samples}
            </hkparam>
        </hkobject>"##,
                reference_frame_signature = class_layout.default_reference_frame_signature
            );
            (
                "#0005",
                section,
                if root_motion.has_yaw {
                    FnvClipMotion::ExtractedPlanarYaw {
                        root_track_index: root_motion.root_track_index,
                        root_bone_index: root_motion.root_bone_index,
                        sample_count: num_frames,
                        end_displacement,
                        end_yaw_radians: end_sample[3],
                    }
                } else {
                    FnvClipMotion::ExtractedPlanar {
                        root_track_index: root_motion.root_track_index,
                        root_bone_index: root_motion.root_bone_index,
                        sample_count: num_frames,
                        end_displacement,
                    }
                },
            )
        }
        None => ("#null", String::new(), FnvClipMotion::InPlace),
    };

    let xml = format!(
        r##"<?xml version="1.0" encoding="ascii"?>
<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1" toplevelobject="#0001">
    <hksection name="__data__">
        <hkobject name="#0001" class="hkRootLevelContainer" signature="{root_level_container_signature}">
            <hkparam name="namedVariants" numelements="1">
                <hkobject>
                    <hkparam name="name">Merged Animation Container</hkparam>
                    <hkparam name="className">hkaAnimationContainer</hkparam>
                    <hkparam name="variant">#0002</hkparam>
                </hkobject>
            </hkparam>
        </hkobject>
        <hkobject name="#0002" class="hkaAnimationContainer" signature="{animation_container_signature}">
            <hkparam name="skeletons" numelements="0"></hkparam>
            <hkparam name="animations" numelements="1">#0003</hkparam>
            <hkparam name="bindings" numelements="1">#0004</hkparam>
            <hkparam name="attachments" numelements="0"></hkparam>
            <hkparam name="skins" numelements="0"></hkparam>
        </hkobject>
        <hkobject name="#0003" class="hkaInterleavedUncompressedAnimation" signature="{interleaved_animation_signature}">
            <hkparam name="type">HK_INTERLEAVED_ANIMATION</hkparam>
            <hkparam name="duration">{duration:.6}</hkparam>
            <hkparam name="numberOfTransformTracks">{num_bones}</hkparam>
            <hkparam name="numberOfFloatTracks">{num_float_tracks}</hkparam>
            <hkparam name="extractedMotion">{extracted_motion_ref}</hkparam>
            <hkparam name="annotationTracks" numelements="{num_bones}">{annotation_section}
            </hkparam>
            <hkparam name="transforms" numelements="{total_transforms}">{transform_data}
            </hkparam>
            <hkparam name="floats" numelements="{total_float_samples}">{float_data}
            </hkparam>
        </hkobject>
        <hkobject name="#0004" class="hkaAnimationBinding" signature="{animation_binding_signature}">
            <hkparam name="originalSkeletonName">{}</hkparam>
            <hkparam name="animation">#0003</hkparam>
            <hkparam name="transformTrackToBoneIndices" numelements="{num_bones}">{bone_indices}
            </hkparam>
            <hkparam name="floatTrackToFloatSlotIndices" numelements="{num_float_tracks}">{float_slot_indices}
            </hkparam>
            <hkparam name="partitionIndices" numelements="0"></hkparam>
            <hkparam name="blendHint">NORMAL</hkparam>
        </hkobject>
        {reference_frame_section}
    </hksection>
</hkpackfile>
"##,
        xml_escape(original_skeleton_name),
        root_level_container_signature = class_layout.root_level_container_signature,
        animation_container_signature = class_layout.animation_container_signature,
        interleaved_animation_signature = class_layout.interleaved_animation_signature,
        animation_binding_signature = class_layout.animation_binding_signature,
    );

    Ok((xml, motion))
}

// ---------------------------------------------------------------------------
// Weapon family table
// ---------------------------------------------------------------------------

#[allow(dead_code)]
struct WeaponFamily {
    fo4_subgraph: String,
    weapon_bones: Vec<String>,
    bone_remap: HashMap<String, String>,
}

struct WeaponFamilyTable {
    families: HashMap<String, WeaponFamily>,
    weapons: HashMap<String, String>,
    unclassified_fallbacks: HashMap<String, String>,
}

impl WeaponFamilyTable {
    fn load(yaml_text: &str) -> Self {
        let val: serde_json::Value = match serde_saphyr::from_str(yaml_text) {
            Ok(v) => v,
            Err(_) => return Self::empty(),
        };

        let families = val
            .get("families")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| {
                        let subgraph = v
                            .get("fo4_subgraph")
                            .and_then(|s| s.as_str())
                            .unwrap_or(k)
                            .to_string();
                        let weapon_bones: Vec<String> = v
                            .get("weapon_bones")
                            .and_then(|a| a.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|s| s.as_str())
                                    .map(|s| s.to_string())
                                    .collect()
                            })
                            .unwrap_or_default();
                        let bone_remap: HashMap<String, String> = v
                            .get("bone_remap")
                            .and_then(|m| m.as_object())
                            .map(|m| {
                                m.iter()
                                    .filter_map(|(k2, v2)| {
                                        v2.as_str().map(|s| (k2.clone(), s.to_string()))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        (
                            k.clone(),
                            WeaponFamily {
                                fo4_subgraph: subgraph,
                                weapon_bones,
                                bone_remap,
                            },
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let weapons = val
            .get("weapons")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let unclassified_fallbacks = val
            .get("unclassified_fallbacks")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        Self {
            families,
            weapons,
            unclassified_fallbacks,
        }
    }

    fn empty() -> Self {
        Self {
            families: HashMap::new(),
            weapons: HashMap::new(),
            unclassified_fallbacks: HashMap::new(),
        }
    }

    #[allow(dead_code)]
    fn classify<'a>(&'a self, weap_eid: &str, animation_type: &str) -> Option<&'a WeaponFamily> {
        let family_name = self
            .weapons
            .get(weap_eid)
            .or_else(|| self.unclassified_fallbacks.get(animation_type))?;
        self.families.get(family_name)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FnvExtractedMotionPolicy {
    #[default]
    RejectNonzero,
    ExtractPlanarReferenceFrame,
}

struct AnimationAsset {
    source_path: String,
    resolved_path: String,
    output_clip_path: String,
    ordered_source_bone_names: Option<Vec<String>>,
    ordered_source_float_slot_names: Option<Vec<String>>,
    source_skeleton_path: Option<String>,
    runtime_skeleton_path: Option<String>,
    original_skeleton_name: Option<String>,
    target_sample_rate_hz: Option<f64>,
    extracted_motion_policy: FnvExtractedMotionPolicy,
    sequence_index: Option<usize>,
}

fn parse_animation_assets(p: &JsonValue) -> Result<Vec<AnimationAsset>, PhaseError> {
    let arr = match p.get("animations").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Ok(Vec::new()),
    };

    arr.iter()
        .enumerate()
        .map(|(i, entry)| {
            let source_path = entry["source_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!("animations[{i}].source_path missing"))
                })?
                .to_string();
            let resolved_path = entry["resolved_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!("animations[{i}].resolved_path missing"))
                })?
                .to_string();
            let output_clip_path = entry["output_clip_path"]
                .as_str()
                .ok_or_else(|| {
                    PhaseError::BadParams(format!(
                        "animations[{i}].output_clip_path missing"
                    ))
                })?;
            let output_clip_path = validate_output_clip_path(output_clip_path).map_err(|reason| {
                PhaseError::BadParams(format!(
                    "animations[{i}].output_clip_path is invalid: {reason}"
                ))
            })?;
            let ordered_source_bone_names = entry
                .get("ordered_source_bone_names")
                .and_then(|value| value.as_array())
                .map(|names| {
                    names
                        .iter()
                        .enumerate()
                        .map(|(bone_index, name)| {
                            name.as_str().map(str::to_string).ok_or_else(|| {
                                PhaseError::BadParams(format!(
                                    "animations[{i}].ordered_source_bone_names[{bone_index}] must be a string"
                                ))
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let ordered_source_float_slot_names = entry
                .get("ordered_source_float_slot_names")
                .and_then(|value| value.as_array())
                .map(|names| {
                    names
                        .iter()
                        .enumerate()
                        .map(|(slot_index, name)| {
                            name.as_str().map(str::to_string).ok_or_else(|| {
                                PhaseError::BadParams(format!(
                                    "animations[{i}].ordered_source_float_slot_names[{slot_index}] must be a string"
                                ))
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            let runtime_skeleton_path = entry
                .get("runtime_skeleton_path")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let source_skeleton_path = entry
                .get("source_skeleton_path")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let original_skeleton_name = entry
                .get("original_skeleton_name")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let target_sample_rate_hz = entry
                .get("target_sample_rate_hz")
                .and_then(|value| value.as_f64());
            let sequence_index = entry
                .get("sequence_index")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize);
            let extracted_motion_policy = match entry
                .get("extracted_motion_policy")
                .and_then(|value| value.as_str())
            {
                None | Some("reject_nonzero") => FnvExtractedMotionPolicy::RejectNonzero,
                Some("extract_planar_reference_frame") => {
                    FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame
                }
                Some(value) => {
                    return Err(PhaseError::BadParams(format!(
                        "animations[{i}].extracted_motion_policy {value:?} is unsupported"
                    )));
                }
            };
            Ok(AnimationAsset {
                source_path,
                resolved_path,
                output_clip_path,
                ordered_source_bone_names,
                ordered_source_float_slot_names,
                source_skeleton_path,
                runtime_skeleton_path,
                original_skeleton_name,
                target_sample_rate_hz,
                extracted_motion_policy,
                sequence_index,
            })
        })
        .collect()
}

fn output_clip_deploy_path(mod_path: &Path, output_clip_path: &str) -> PathBuf {
    let mut out = mod_path.to_path_buf();
    out.push("data");
    out.push("Meshes");
    for component in output_clip_path.split(['/', '\\']) {
        out.push(component);
    }
    out
}

fn validate_output_clip_path(value: &str) -> Result<String, String> {
    let normalized = value.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err("expected a non-empty path relative to the Meshes directory".to_string());
    }
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err("expected a relative path without a drive or root prefix".to_string());
    }
    if normalized
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(
            "empty, current-directory, and parent-directory components are forbidden".to_string(),
        );
    }
    if !normalized.to_ascii_lowercase().ends_with(".hkx") {
        return Err(
            "expected an .hkx target; .xml/.hkt and other extensions are forbidden".to_string(),
        );
    }
    let lowercase = normalized.to_ascii_lowercase();
    if lowercase.starts_with("meshes/") {
        return Err(
            "path must be relative to Meshes and must not include the Meshes prefix".to_string(),
        );
    }
    if lowercase.starts_with("data/") {
        return Err("path must not include the Data prefix".to_string());
    }
    Ok(normalized.replace('/', "\\"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::{PhaseCtx, PhaseReport};
    use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
    use crate::translator::Game;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn make_run() -> u64 {
        create_run(RunParams {
            source: Game::Fo4,
            target: Game::Fo4,
            source_handle_id: 9999,
            target_handle_id: 9998,
            master_handle_ids: vec![],
            config: RunConfig {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        })
        .unwrap()
    }

    fn fnv_animation_fixture(relative_path: &str) -> Option<PathBuf> {
        legacy_animation_fixture("fnv", relative_path)
    }

    fn fo3_animation_fixture(relative_path: &str) -> Option<PathBuf> {
        legacy_animation_fixture("fo3", relative_path)
    }

    fn legacy_animation_fixture(game: &str, relative_path: &str) -> Option<PathBuf> {
        let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|path| path.join("extracted").join(game).is_dir())?;
        let path = repo_root.join("extracted").join(game).join(relative_path);
        path.is_file().then_some(path)
    }

    fn recursive_kf_paths(root: &Path) -> Vec<PathBuf> {
        let mut pending = vec![root.to_path_buf()];
        let mut paths = Vec::new();
        while let Some(directory) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("kf"))
                {
                    paths.push(path);
                }
            }
        }
        paths.sort();
        paths
    }

    fn ordered_fixture_skeleton_names(bytes: &[u8]) -> Vec<String> {
        let nif = NifFile::from_bytes(bytes, None).unwrap();
        ordered_named_nif_nodes(&nif)
    }

    fn binding_test_clip(track_names: &[&str]) -> AnimationClip {
        AnimationClip {
            name: "Idle".to_string(),
            duration: 2.5,
            cycle_type: "loop".to_string(),
            events: vec![AnimationEvent {
                time: 1.0,
                source_time: 1.0,
                text: "sound".to_string(),
            }],
            channels: track_names
                .iter()
                .map(|name| BoneChannel {
                    bone_name: (*name).to_string(),
                    priority: 0,
                    rotations: Vec::new(),
                    translations: Vec::new(),
                    scales: Vec::new(),
                })
                .collect(),
            ..Default::default()
        }
    }

    fn compact_track_names(bytes: &[u8]) -> HashSet<String> {
        let nif = NifFile::from_bytes(bytes, None).unwrap();
        let sequence = nif
            .blocks
            .iter()
            .find(|block| block.type_name == "NiControllerSequence")
            .unwrap();
        sequence
            .array_of_structs("Controlled Blocks")
            .into_iter()
            .filter_map(|controlled| {
                let reference = controlled.block_ref_or("Interpolator", -1);
                let block = (reference >= 0)
                    .then(|| nif.blocks.get(reference as usize))
                    .flatten()?;
                (block.type_name == "NiBSplineCompTransformInterpolator")
                    .then(|| controlled.string_or("Node Name", ""))
            })
            .collect()
    }

    fn malformed_compact_kf_bytes(relative_path: &str) -> Option<(Vec<u8>, String)> {
        let path = fnv_animation_fixture(relative_path)?;
        let mut nif = NifFile::from_bytes(&std::fs::read(path).unwrap(), None).unwrap();
        let sequence = nif
            .blocks
            .iter()
            .find(|block| block.type_name == "NiControllerSequence")?;
        let (interpolator_index, bone_name) = sequence
            .array_of_structs("Controlled Blocks")
            .into_iter()
            .find_map(|controlled| {
                let reference = controlled.block_ref_or("Interpolator", -1);
                let interpolator = (reference >= 0)
                    .then(|| nif.blocks.get(reference as usize))
                    .flatten()?;
                (interpolator.type_name == "NiBSplineCompTransformInterpolator")
                    .then(|| (reference as usize, controlled.string_or("Node Name", "")))
            })?;
        let basis_index = nif.blocks[interpolator_index].block_ref_field("Basis Data")?;
        nif.blocks[basis_index].set_field("Num Control Points", NifValue::UInt(3));
        Some((nif.to_bytes().unwrap(), bone_name))
    }

    fn assert_clip_samples(clip: &AnimationClip, compact_names: &HashSet<String>) {
        assert!(clip.duration.is_finite() && clip.duration > 0.0);
        assert!(
            clip.warnings.is_empty(),
            "unexpected KF parse warnings: {:?}",
            clip.warnings
        );

        let unique_names: HashSet<&str> = clip
            .channels
            .iter()
            .map(|channel| channel.bone_name.as_str())
            .collect();
        assert_eq!(unique_names.len(), clip.channels.len());
        assert!(unique_names.iter().all(|name| !name.is_empty()));

        for channel in &clip.channels {
            if compact_names.contains(&channel.bone_name) {
                assert!(
                    !channel.rotations.is_empty()
                        || !channel.translations.is_empty()
                        || !channel.scales.is_empty(),
                    "compact track '{}' produced no transform samples",
                    channel.bone_name
                );
            }
            for keys in [&channel.rotations, &channel.translations, &channel.scales] {
                for key in keys {
                    assert!(key.time.is_finite());
                    assert!(key.time >= -1.0e-6 && key.time <= clip.duration + 1.0e-5);
                    assert!(key.value.iter().all(|value| value.is_finite()));
                }
            }
            if compact_names.contains(&channel.bone_name) {
                for key in &channel.rotations {
                    let norm = key
                        .value
                        .iter()
                        .map(|value| value * value)
                        .sum::<f64>()
                        .sqrt();
                    assert!(
                        (norm - 1.0).abs() < 1.0e-6,
                        "compact rotation key for '{}' has norm {norm}",
                        channel.bone_name
                    );
                }
            }

            for time in [0.0, clip.duration * 0.5, clip.duration] {
                let translation = sample_translation(&channel.translations, time);
                assert!(
                    [translation.0, translation.1, translation.2]
                        .iter()
                        .all(|value| value.is_finite())
                );
                let rotation = sample_rotation(&channel.rotations, time);
                let rotation_norm = (rotation.0 * rotation.0
                    + rotation.1 * rotation.1
                    + rotation.2 * rotation.2
                    + rotation.3 * rotation.3)
                    .sqrt();
                assert!((rotation_norm - 1.0).abs() < 1.0e-6);
                assert!(sample_scale_val(&channel.scales, time).is_finite());
            }
        }
    }

    fn assert_packed_fo4_animation_signatures(
        packed: &havok_native::hkx::HkxFile,
        expect_reference_frame: bool,
    ) {
        for (class_name, expected_signature) in [
            ("hkRootLevelContainer", 0x2772_c11e),
            ("hkaAnimationContainer", 0x2685_9f4c),
            ("hkaInterleavedUncompressedAnimation", 0xa5ef_f3f2),
            ("hkaAnimationBinding", 0x0faf_9150),
        ] {
            let object = packed
                .objects()
                .iter()
                .find(|object| object.class_name == class_name)
                .unwrap_or_else(|| panic!("packed HKX missing {class_name}"));
            assert_eq!(object.signature, expected_signature, "{class_name}");
        }
        let reference_frame = packed
            .objects()
            .iter()
            .find(|object| object.class_name == "hkaDefaultAnimatedReferenceFrame");
        if expect_reference_frame {
            assert_eq!(reference_frame.unwrap().signature, 0x60f8_e0b8);
        } else {
            assert!(reference_frame.is_none());
        }
    }

    fn reference_frame_samples(xml: &str) -> (f64, Vec<[f64; 4]>) {
        let document = roxmltree::Document::parse(xml).unwrap();
        let reference_frame = document
            .descendants()
            .find(|node| {
                node.has_tag_name("hkobject")
                    && node.attribute("class") == Some("hkaDefaultAnimatedReferenceFrame")
            })
            .expect("round-trip XML missing extracted-motion reference frame");
        let param = |name| {
            reference_frame
                .children()
                .find(|node| node.has_tag_name("hkparam") && node.attribute("name") == Some(name))
                .unwrap()
        };
        let duration = param("duration").text().unwrap().trim().parse().unwrap();
        let text = param("referenceFrameSamples").text().unwrap_or_default();
        let samples = text
            .split('(')
            .skip(1)
            .filter_map(|group| group.split_once(')').map(|(values, _)| values))
            .map(|values| {
                let values = values
                    .split_whitespace()
                    .map(|value| value.parse::<f64>().unwrap())
                    .collect::<Vec<_>>();
                [values[0], values[1], values[2], values[3]]
            })
            .collect();
        (duration, samples)
    }

    #[test]
    fn compact_cubic_bspline_evaluates_endpoints_and_midpoint() {
        let sampling = CompactSplineSampling::new(4, 0.0, 1.0).unwrap();
        let samples =
            evaluate_compact_channel(&[0, 0, 0, i16::MAX], 0, 4, 1, 0.0, 1.0, &sampling, "test")
                .unwrap();

        assert!((samples[0][0] - 0.0).abs() < 1.0e-12);
        assert!((samples[1][0] - 1.0 / 27.0).abs() < 1.0e-12);
        assert!((samples[2][0] - 8.0 / 27.0).abs() < 1.0e-12);
        assert!((samples[3][0] - 1.0).abs() < 1.0e-12);
        for weights in &sampling.weights {
            assert!((weights.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
        }
    }

    #[test]
    fn compact_sampling_rejects_malformed_metadata() {
        assert!(matches!(
            CompactSplineSampling::new(3, 0.0, 1.0),
            Err(CompactSplineError::InvalidControlPointCount(3))
        ));
        assert!(matches!(
            CompactSplineSampling::new(4, 2.0, 1.0),
            Err(CompactSplineError::InvalidTimeRange {
                start: 2.0,
                stop: 1.0
            })
        ));
    }

    #[test]
    fn compact_short_decompression_uses_offset_and_half_range() {
        assert!((decompress_compact(i16::MIN + 1, 2.0, 1.5) - 0.5).abs() < 1.0e-12);
        assert!((decompress_compact(0, 2.0, 1.5) - 2.0).abs() < 1.0e-12);
        assert!((decompress_compact(i16::MAX, 2.0, 1.5) - 3.5).abs() < 1.0e-12);
    }

    #[test]
    fn fnv_cycle_type_matches_nif_schema() {
        assert_eq!(fnv_cycle_type(0), Some("loop"));
        assert_eq!(fnv_cycle_type(1), Some("reverse"));
        assert_eq!(fnv_cycle_type(2), Some("clamp"));
        assert_eq!(fnv_cycle_type(3), None);
    }

    #[test]
    fn fnv_binding_uses_canonical_names_and_leaves_untracked_bones_unmapped() {
        let clip = binding_test_clip(&["root", " BIP01   Spine "]);
        let duration = clip.duration;
        let events: Vec<(f64, String)> = clip
            .events
            .iter()
            .map(|event| (event.time, event.text.clone()))
            .collect();
        let declaration = bind_fnv_kf_clip(
            &[
                "Root".to_string(),
                "Bip01 Spine".to_string(),
                "Unused".to_string(),
            ],
            &clip,
            "Actors\\Test\\CharacterAssets\\Skeleton.hkx",
            "Actors\\Test\\Animations\\Idle.hkx",
            "TestSkeleton",
            &[],
        )
        .unwrap();

        assert_eq!(declaration.name, "Idle");
        assert_eq!(declaration.path, "Actors\\Test\\Animations\\Idle.hkx");
        assert!(declaration.looping);
        assert_eq!(
            declaration.binding.skeleton_path,
            "Actors\\Test\\CharacterAssets\\Skeleton.hkx"
        );
        assert_eq!(declaration.binding.original_skeleton_name, "TestSkeleton");
        assert_eq!(declaration.binding.declared_transform_tracks, 2);
        assert_eq!(
            declaration.binding.transform_track_to_bone_indices,
            vec![0, 1]
        );
        assert_eq!(declaration.binding.declared_float_tracks, 0);
        assert!(
            declaration
                .binding
                .float_track_to_float_slot_indices
                .is_empty()
        );
        assert_eq!(clip.duration, duration);
        assert_eq!(
            clip.events
                .iter()
                .map(|event| (event.time, event.text.clone()))
                .collect::<Vec<_>>(),
            events
        );
    }

    #[test]
    fn fnv_binding_rejects_duplicate_ambiguous_missing_and_invalid_mappings() {
        let clip = binding_test_clip(&["Root"]);
        assert!(matches!(
            bind_fnv_kf_transform_tracks(&["Root".to_string(), "Root".to_string()], &clip),
            Err(FnvKfBindingError::DuplicateSkeletonBone { .. })
        ));
        assert!(matches!(
            bind_fnv_kf_transform_tracks(&["Root".to_string(), " root ".to_string()], &clip),
            Err(FnvKfBindingError::AmbiguousSkeletonBone { .. })
        ));

        let duplicate_tracks = binding_test_clip(&["Root", " root "]);
        assert!(matches!(
            bind_fnv_kf_transform_tracks(&["Root".to_string()], &duplicate_tracks),
            Err(FnvKfBindingError::DuplicateTrackTarget { .. })
        ));

        let missing = binding_test_clip(&["Missing"]);
        assert!(matches!(
            bind_fnv_kf_transform_tracks(&["Root".to_string()], &missing),
            Err(FnvKfBindingError::MissingTrackTarget { .. })
        ));
        assert!(matches!(
            validate_fnv_kf_mapping(&[0, 2], 2),
            Err(FnvKfBindingError::OutOfRangeMapping { .. })
        ));
        assert!(matches!(
            validate_fnv_kf_mapping(&[0, 0], 2),
            Err(FnvKfBindingError::DuplicateBoneMapping { .. })
        ));
    }

    #[test]
    fn extracted_motion_policy_param_is_exact_and_fail_closed() {
        let entry = |policy: Option<&str>| {
            let mut animation = serde_json::json!({
                "source_path": "meshes/creatures/nvgecko/mtidle.kf",
                "resolved_path": "mtidle.kf",
                "output_clip_path": "Actors/B21_FNVGecko/Animations/Idle.hkx"
            });
            if let Some(policy) = policy {
                animation["extracted_motion_policy"] = JsonValue::String(policy.to_string());
            }
            serde_json::json!({ "animations": [animation] })
        };

        assert_eq!(
            parse_animation_assets(&entry(None)).unwrap()[0].extracted_motion_policy,
            FnvExtractedMotionPolicy::RejectNonzero
        );
        assert_eq!(
            parse_animation_assets(&entry(Some("reject_nonzero"))).unwrap()[0]
                .extracted_motion_policy,
            FnvExtractedMotionPolicy::RejectNonzero
        );
        assert_eq!(
            parse_animation_assets(&entry(Some("extract_planar_reference_frame"))).unwrap()[0]
                .extracted_motion_policy,
            FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame
        );
        assert!(matches!(
            parse_animation_assets(&entry(Some("animation_driven"))),
            Err(PhaseError::BadParams(_))
        ));

        let missing_output = serde_json::json!({
            "animations": [{
                "source_path": "meshes/creatures/nvgecko/mtidle.kf",
                "resolved_path": "mtidle.kf"
            }]
        });
        assert!(matches!(
            parse_animation_assets(&missing_output),
            Err(PhaseError::BadParams(message)) if message.contains("output_clip_path missing")
        ));
    }

    #[test]
    fn kf_conversion_requires_explicit_skeleton_contract() {
        let error = convert_kf_to_hkx(
            Path::new("missing.kf"),
            Path::new("missing.hkx"),
            &HashMap::new(),
            "creatures/nvgecko/mtidle.kf",
            "Actors/Test/Animations/Idle.hkx",
            None,
            None,
            Some("Actors\\NVGecko\\CharacterAssets\\Skeleton.hkx"),
            Some("NVGecko"),
            None,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            FnvKfConversionError::MissingOrderedSourceBoneNames
        ));

        let bones = vec!["Bip01".to_string()];
        let error = convert_kf_to_hkx(
            Path::new("missing.kf"),
            Path::new("missing.hkx"),
            &HashMap::new(),
            "creatures/nvgecko/mtidle.kf",
            "Actors/Test/Animations/Idle.hkx",
            Some(&bones),
            None,
            None,
            Some("NVGecko"),
            None,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            FnvKfConversionError::MissingRuntimeSkeletonPath
        ));

        let error = convert_kf_to_hkx(
            Path::new("missing.kf"),
            Path::new("missing.hkx"),
            &HashMap::new(),
            "creatures/nvgecko/mtidle.kf",
            "Actors/Test/Animations/Idle.hkx",
            Some(&bones),
            None,
            Some("Actors\\NVGecko\\CharacterAssets\\Skeleton.hkx"),
            None,
            None,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            FnvKfConversionError::MissingOriginalSkeletonName
        ));

        let error = convert_kf_to_hkx(
            Path::new("missing.kf"),
            Path::new("missing.hkx"),
            &HashMap::new(),
            "creatures/nvgecko/mtidle.kf",
            "Actors/Test/Animations/Idle.hkx",
            Some(&bones),
            Some(Path::new("skeleton.nif")),
            Some("Actors\\NVGecko\\CharacterAssets\\Skeleton.hkx"),
            Some("NVGecko"),
            None,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            FnvKfConversionError::AmbiguousSourceSkeleton
        ));
    }

    #[test]
    fn fo4_xml_uses_fixed_output_rate_and_strict_binding() {
        let mut clip = binding_test_clip(&["Root", "Child"]);
        clip.frequency = 120.0;
        let declaration = ClipDecl {
            name: "Idle".to_string(),
            path: "Actors\\Test\\Animations\\Idle.hkx".to_string(),
            binding: ClipBinding {
                skeleton_path: "Actors\\Test\\CharacterAssets\\Skeleton.hkx".to_string(),
                original_skeleton_name: "TestSkeleton".to_string(),
                declared_transform_tracks: 2,
                transform_track_to_bone_indices: vec![2, 0],
                declared_float_tracks: 0,
                float_track_to_float_slot_indices: Vec::new(),
            },
            looping: true,
        };

        let (xml, motion) = clip_to_havok_xml(
            &clip,
            &declaration,
            "TestSkeleton",
            30.0,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap();
        assert_eq!(motion, FnvClipMotion::InPlace);
        let document = roxmltree::Document::parse(&xml).unwrap();
        let root = document.root_element();
        assert_eq!(root.attribute("classversion"), Some("11"));
        assert_eq!(root.attribute("contentsversion"), Some("hk_2014.1.0-r1"));
        let layout = load_fo4_animation_class_layout().unwrap();
        assert_eq!(layout.root_level_container_signature, "0x2772c11e");
        assert_eq!(layout.animation_container_signature, "0x26859f4c");
        assert_eq!(layout.interleaved_animation_signature, "0xa5eff3f2");
        assert_eq!(layout.animation_binding_signature, "0x0faf9150");
        assert_eq!(layout.default_reference_frame_signature, "0x60f8e0b8");
        for signature in ["0x2772c11e", "0x26859f4c", "0xa5eff3f2", "0x0faf9150"] {
            assert!(xml.contains(&format!("signature=\"{signature}\"")));
        }
        assert!(xml.contains("name=\"partitionIndices\" numelements=\"0\""));
        let transforms = document
            .descendants()
            .find(|node| {
                node.has_tag_name("hkparam") && node.attribute("name") == Some("transforms")
            })
            .unwrap();
        assert_eq!(target_sample_count(clip.duration, 30.0), 76);
        assert_eq!(transforms.attribute("numelements"), Some("152"));
        assert!(xml.contains(">TestSkeleton</hkparam>"));
        assert!(xml.contains("\n                    2\n                    0"));
    }

    #[test]
    fn vertical_root_motion_fails_typed() {
        let mut clip = binding_test_clip(&["Root"]);
        clip.duration = 1.0;
        clip.accum_root = "Root".to_string();
        clip.channels[0].translations = vec![
            AnimationKeyframe {
                time: 0.0,
                value: vec![0.0, 0.0, 0.0],
                interpolation: Interpolation::Linear,
                forward: None,
                backward: None,
                tbc: None,
            },
            AnimationKeyframe {
                time: 1.0,
                value: vec![0.0, 0.0, 10.0],
                interpolation: Interpolation::Linear,
                forward: None,
                backward: None,
                tbc: None,
            },
        ];
        let declaration = bind_fnv_kf_clip(
            &["Root".to_string()],
            &clip,
            "Actors\\Test\\CharacterAssets\\Skeleton.hkx",
            "Actors\\Test\\Animations\\Vertical.hkx",
            "TestSkeleton",
            &[],
        )
        .unwrap();
        let error = clip_to_havok_xml(
            &clip,
            &declaration,
            "TestSkeleton",
            30.0,
            FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            FnvKfConversionError::UnsupportedVerticalRootMotion { track: 0, .. }
        ));
    }

    #[test]
    fn malformed_real_compact_track_is_typed_fatal_and_writes_nothing() {
        let relative_path = "meshes/creatures/nvgecko/mtidle.kf";
        let Some((malformed_bytes, expected_bone)) = malformed_compact_kf_bytes(relative_path)
        else {
            return;
        };
        let parse_error = parse_kf_bytes(&malformed_bytes, relative_path).unwrap_err();
        assert!(matches!(
            parse_error,
            KfParseError::CompactSpline {
                bone_name: ref actual_bone,
                source: CompactSplineError::InvalidControlPointCount(3),
            } if actual_bone == &expected_bone
        ));
        assert!(matches!(
            parse_fnv_creature_kf(&malformed_bytes, relative_path, None, None),
            Err(KfParseError::CompactSpline {
                bone_name: ref actual_bone,
                source: CompactSplineError::InvalidControlPointCount(3),
            }) if actual_bone == &expected_bone
        ));
        assert!(matches!(
            stage_fnv_creature_kf(
                &malformed_bytes,
                FnvKfStageRequest {
                    source_kf: relative_path,
                    output_clip_path: "Actors/Test/Animations/Malformed.hkx",
                    sequence_index: None,
                    skeleton: FnvKfSkeletonContract {
                        skeleton_path: "Actors/Test/CharacterAssets/Skeleton.hkx",
                        ordered_bone_names: &[],
                        ordered_float_slot_names: &[],
                    },
                    original_skeleton_name: "Test",
                    event_map: &HashMap::new(),
                    target_sample_rate_hz: None,
                    extracted_motion_policy: FnvExtractedMotionPolicy::RejectNonzero,
                }
            ),
            Err(FnvKfConversionError::CompactSplineDecode {
                bone_name: ref actual_bone,
                source: CompactSplineError::InvalidControlPointCount(3),
                ..
            }) if actual_bone == &expected_bone
        ));

        let Some(skeleton_path) = fnv_animation_fixture("meshes/creatures/nvgecko/skeleton.nif")
        else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let malformed_path = temp.path().join("malformed-mtidle.kf");
        let output_path = temp.path().join("Idle.hkx");
        std::fs::write(&malformed_path, malformed_bytes).unwrap();
        let conversion_error = convert_kf_to_hkx(
            &malformed_path,
            &output_path,
            &HashMap::new(),
            relative_path,
            "Actors/B21_FNVGecko/Animations/Idle.hkx",
            None,
            Some(&skeleton_path),
            Some("Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"),
            Some("NVGecko"),
            None,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap_err();
        assert!(matches!(
            conversion_error,
            FnvKfConversionError::CompactSplineDecode {
                bone_name: ref actual_bone,
                source: CompactSplineError::InvalidControlPointCount(3),
                ..
            } if actual_bone == &expected_bone
        ));
        assert!(!output_path.exists());
    }

    #[test]
    fn real_fnv_human_attack_decodes_all_compact_tracks() {
        let Some(path) = fnv_animation_fixture("meshes/characters/_male/1hmattackpower.kf") else {
            return;
        };
        let Some(skeleton_path) = fnv_animation_fixture("meshes/characters/_male/skeleton.nif")
        else {
            return;
        };
        let bytes = std::fs::read(&path).unwrap();
        let skeleton_names =
            ordered_fixture_skeleton_names(&std::fs::read(&skeleton_path).unwrap());
        let compact_names = compact_track_names(&bytes);
        let clip = parse_kf_bytes(&bytes, "characters/_male/1hmattackpower.kf").unwrap();
        let duration = clip.duration;
        let events: Vec<(f64, String)> = clip
            .events
            .iter()
            .map(|event| (event.time, event.text.clone()))
            .collect();
        let binding_error = bind_fnv_kf_transform_tracks(&skeleton_names, &clip).unwrap_err();
        assert_eq!(
            binding_error,
            FnvKfBindingError::UnsupportedOverlayChannel {
                track: 56,
                name: "##batonhandle".to_string(),
            }
        );

        let overlay_targets: Vec<(usize, &str)> = clip
            .channels
            .iter()
            .enumerate()
            .filter(|(_, channel)| channel.bone_name.starts_with("##"))
            .map(|(index, channel)| (index, channel.bone_name.as_str()))
            .collect();
        assert_eq!(
            overlay_targets,
            vec![(56, "##batonhandle"), (57, "##BatonShaft"), (58, "##Latch"),]
        );
        assert!(
            overlay_targets
                .iter()
                .all(|(_, name)| !compact_names.contains(*name))
        );
        let mut skeletal_clip = clip.clone();
        skeletal_clip
            .channels
            .retain(|channel| !channel.bone_name.starts_with("##"));
        let mapping = bind_fnv_kf_transform_tracks(&skeleton_names, &skeletal_clip).unwrap();

        eprintln!(
            "FNV human 1hmattackpower: total={} skeletal={} overlay={} compact={}",
            clip.channels.len(),
            mapping.len(),
            overlay_targets.len(),
            compact_names.len()
        );
        assert_eq!(clip.channels.len(), 63);
        assert_eq!(compact_names.len(), 42);
        assert_eq!(skeleton_names.len(), 65);
        assert!(!compact_names.contains("##batonhandle"));
        assert!((clip.duration - 1.2).abs() < 1.0e-5);
        assert!(clip.events.iter().any(|event| event.text == "hit"));
        assert_eq!(skeletal_clip.channels.len(), 60);
        assert_eq!(mapping.len(), 60);
        assert_eq!(mapping.iter().copied().collect::<HashSet<_>>().len(), 60);
        assert!(mapping.iter().all(|index| *index < skeleton_names.len()));
        assert_eq!(clip.duration, duration);
        assert_eq!(
            clip.events
                .iter()
                .map(|event| (event.time, event.text.clone()))
                .collect::<Vec<_>>(),
            events
        );
        assert_clip_samples(&clip, &compact_names);
    }

    #[test]
    fn real_fnv_gecko_idle_decodes_all_compact_tracks() {
        let Some(path) = fnv_animation_fixture("meshes/creatures/nvgecko/mtidle.kf") else {
            return;
        };
        let Some(skeleton_path) = fnv_animation_fixture("meshes/creatures/nvgecko/skeleton.nif")
        else {
            return;
        };
        let bytes = std::fs::read(&path).unwrap();
        let skeleton_names =
            ordered_fixture_skeleton_names(&std::fs::read(&skeleton_path).unwrap());
        let compact_names = compact_track_names(&bytes);
        let clip = parse_kf_bytes(&bytes, "creatures/nvgecko/mtidle.kf").unwrap();
        let duration = clip.duration;
        let events: Vec<(f64, String)> = clip
            .events
            .iter()
            .map(|event| (event.time, event.text.clone()))
            .collect();
        let declaration = bind_fnv_kf_clip(
            &skeleton_names,
            &clip,
            "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx",
            "Actors\\B21_FNVGecko\\Animations\\Idle.hkx",
            "NVGecko",
            &[],
        )
        .unwrap();

        eprintln!(
            "FNV Gecko mtidle: total={} compact={}",
            clip.channels.len(),
            compact_names.len()
        );
        assert_eq!(clip.channels.len(), 84);
        assert_eq!(compact_names.len(), 44);
        assert!((clip.duration - 13.333_334).abs() < 1.0e-5);
        assert_eq!(skeleton_names.len(), 87);
        assert_eq!(declaration.binding.declared_transform_tracks, 84);
        assert_eq!(
            declaration.path,
            "Actors\\B21_FNVGecko\\Animations\\Idle.hkx"
        );
        assert_eq!(
            declaration.binding.skeleton_path,
            "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"
        );
        assert_eq!(
            declaration.binding.transform_track_to_bone_indices.len(),
            84
        );
        assert_eq!(
            declaration
                .binding
                .transform_track_to_bone_indices
                .iter()
                .copied()
                .collect::<HashSet<_>>()
                .len(),
            84
        );
        assert!(
            declaration
                .binding
                .transform_track_to_bone_indices
                .iter()
                .all(|index| *index < skeleton_names.len())
        );
        assert_ne!(
            declaration.binding.transform_track_to_bone_indices,
            (0..84).collect::<Vec<_>>(),
            "binding must follow canonical bone names, not KF track position"
        );
        assert_eq!(clip.duration, duration);
        assert_eq!(
            clip.events
                .iter()
                .map(|event| (event.time, event.text.clone()))
                .collect::<Vec<_>>(),
            events
        );
        assert_clip_samples(&clip, &compact_names);

        let temp = tempfile::tempdir().unwrap();
        let rejected_extract_path = temp.path().join("mtidle-extract.hkx");
        let rejected_extract = convert_kf_to_hkx(
            &path,
            &rejected_extract_path,
            &HashMap::new(),
            "Meshes/creatures/nvgecko/mtidle.kf",
            "Actors/B21_FNVGecko/Animations/Idle.hkx",
            None,
            Some(&skeleton_path),
            Some("Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"),
            Some("NVGecko"),
            None,
            FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
        )
        .unwrap_err();
        assert!(matches!(
            rejected_extract,
            FnvKfConversionError::MissingAccumRootTrack { ref name } if name == "Bip01"
        ));
        assert!(!rejected_extract_path.exists());

        let output_path = temp.path().join("mtidle.hkx");
        let conversion_outcome = convert_kf_to_hkx(
            &path,
            &output_path,
            &HashMap::new(),
            "Meshes/creatures/nvgecko/mtidle.kf",
            "Actors/B21_FNVGecko/Animations/Idle.hkx",
            None,
            Some(&skeleton_path),
            Some("Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"),
            Some("NVGecko"),
            None,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap();
        assert!(conversion_outcome.warnings.is_empty());
        assert_eq!(conversion_outcome.motion, FnvClipMotion::InPlace);
        assert_eq!(
            conversion_outcome.output_clip_path,
            "Actors\\B21_FNVGecko\\Animations\\Idle.hkx"
        );

        let packed_bytes = std::fs::read(&output_path).unwrap();
        let packed = havok_native::hkx::HkxFile::read(&packed_bytes).unwrap();
        assert_eq!(packed.class_version(), 11);
        assert_eq!(packed.contents_version(), "hk_2014.1.0-r1");
        assert_eq!(packed.packfile().header.pointer_size, 8);
        assert_packed_fo4_animation_signatures(&packed, false);

        let roundtrip_xml = havok_native::api::havok_hkx_to_xml(&packed_bytes).unwrap();
        let roundtrip_clip =
            havok_native::animation::clip::extract_clip(&roundtrip_xml, None).unwrap();
        assert!((roundtrip_clip.duration - clip.duration as f32).abs() < 1.0e-4);
        assert_eq!(
            roundtrip_clip.original_skeleton_name.as_deref(),
            Some("NVGecko")
        );
        assert_eq!(roundtrip_clip.channels.len(), 84);
        assert!(roundtrip_clip.extracted_motion_ref.is_empty());
        assert_eq!(roundtrip_clip.channels[0].translations.len(), 401);
        assert_eq!(target_sample_count(clip.duration, 30.0), 401);
        assert_eq!(
            roundtrip_clip.track_to_bone_indices,
            declaration
                .binding
                .transform_track_to_bone_indices
                .iter()
                .map(|index| *index as u32)
                .collect::<Vec<_>>()
        );
        assert_ne!(
            roundtrip_clip.track_to_bone_indices,
            (0..84).collect::<Vec<_>>()
        );

        let endpoint_track = clip
            .channels
            .iter()
            .position(|channel| !channel.translations.is_empty())
            .unwrap();
        let source_start = sample_translation(&clip.channels[endpoint_track].translations, 0.0);
        let source_end =
            sample_translation(&clip.channels[endpoint_track].translations, clip.duration);
        let emitted = &roundtrip_clip.channels[endpoint_track].translations;
        for (actual, expected) in
            emitted[0]
                .value
                .iter()
                .zip([source_start.0, source_start.1, source_start.2])
        {
            assert!((*actual as f64 - expected).abs() < 1.0e-4);
        }
        for (actual, expected) in
            emitted
                .last()
                .unwrap()
                .value
                .iter()
                .zip([source_end.0, source_end.1, source_end.2])
        {
            assert!((*actual as f64 - expected).abs() < 1.0e-4);
        }
    }

    #[test]
    fn real_fnv_gecko_motion_extracts_planar_reference_frame() {
        let Some(skeleton_path) = fnv_animation_fixture("meshes/creatures/nvgecko/skeleton.nif")
        else {
            return;
        };
        let skeleton_names = load_ordered_source_skeleton_names(&skeleton_path).unwrap();
        assert_eq!(skeleton_names.len(), 87);

        for (
            relative_path,
            output_clip_path,
            expected_tracks,
            expected_cycle,
            expected_samples,
            expected_end_y,
        ) in [
            (
                "meshes/creatures/nvgecko/swimmtforward.kf",
                "Actors/B21_FNVGecko/Animations/WalkForward.hkx",
                85,
                "loop",
                49,
                232.454_22,
            ),
            (
                "meshes/creatures/nvgecko/h2hattackforwardpower.kf",
                "Actors/B21_FNVGecko/Animations/Attack1.hkx",
                86,
                "clamp",
                52,
                374.279_013,
            ),
        ] {
            let Some(path) = fnv_animation_fixture(relative_path) else {
                return;
            };
            let bytes = std::fs::read(&path).unwrap();
            let clip = parse_kf_bytes(&bytes, relative_path).unwrap();
            assert_eq!(clip.channels.len(), expected_tracks);
            assert_eq!(clip.cycle_type, expected_cycle);
            let event_map = if output_clip_path.ends_with("Attack1.hkx") {
                assert!(clip.events.iter().any(|event| event.text == "Hit"));
                HashMap::from([("Hit".to_string(), "HitFrame".to_string())])
            } else {
                HashMap::new()
            };
            let declaration = bind_fnv_kf_clip(
                &skeleton_names,
                &clip,
                "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx",
                &output_clip_path.replace('/', "\\"),
                "NVGecko",
                &[],
            )
            .unwrap();
            assert_ne!(
                declaration.binding.transform_track_to_bone_indices,
                (0..expected_tracks).collect::<Vec<_>>()
            );
            assert_eq!(declaration.path, output_clip_path.replace('/', "\\"));
            assert_eq!(
                declaration.binding.skeleton_path,
                "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"
            );

            let root_track_index = clip
                .channels
                .iter()
                .position(|channel| {
                    normalize_fnv_bone_name(&channel.bone_name)
                        == normalize_fnv_bone_name(&clip.accum_root)
                })
                .unwrap();
            let root_bone_index =
                declaration.binding.transform_track_to_bone_indices[root_track_index];
            let root_channel = &clip.channels[root_track_index];
            let source_start = sample_translation(&root_channel.translations, 0.0);
            let source_end = sample_translation(&root_channel.translations, clip.duration);
            let expected_end = [source_end.0 - source_start.0, source_end.1 - source_start.1];
            assert!(expected_end[0].hypot(expected_end[1]) > 1.0);
            assert!((expected_end[1] - expected_end_y).abs() < 1.0e-2);
            assert!((source_end.2 - source_start.2).abs() <= 1.0e-4);

            let temp = tempfile::tempdir().unwrap();
            let blocked_path = temp.path().join("blocked.hkx");
            let blocked = convert_kf_to_hkx(
                &path,
                &blocked_path,
                &event_map,
                relative_path,
                output_clip_path,
                None,
                Some(&skeleton_path),
                Some("Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"),
                Some("NVGecko"),
                None,
                FnvExtractedMotionPolicy::RejectNonzero,
            )
            .unwrap_err();
            assert!(matches!(
                blocked,
                FnvKfConversionError::ExtractPlanarReferenceFramePolicyRequired
            ));
            assert!(!blocked_path.exists());

            let output_path = temp.path().join("motion.hkx");
            let outcome = convert_kf_to_hkx(
                &path,
                &output_path,
                &event_map,
                relative_path,
                output_clip_path,
                None,
                Some(&skeleton_path),
                Some("Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"),
                Some("NVGecko"),
                None,
                FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
            )
            .unwrap();
            assert!(outcome.warnings.is_empty());
            assert_eq!(
                outcome.output_clip_path,
                output_clip_path.replace('/', "\\")
            );
            let sample_count = target_sample_count(clip.duration, 30.0);
            assert_eq!(sample_count, expected_samples);
            eprintln!(
                "{relative_path}: tracks={expected_tracks} samples={sample_count} end=({:.6},{:.6})",
                expected_end[0], expected_end[1]
            );
            assert_eq!(
                outcome.motion,
                FnvClipMotion::ExtractedPlanar {
                    root_track_index,
                    root_bone_index,
                    sample_count,
                    end_displacement: expected_end,
                }
            );

            let packed_bytes = std::fs::read(&output_path).unwrap();
            let packed = havok_native::hkx::HkxFile::read(&packed_bytes).unwrap();
            assert_eq!(packed.class_version(), 11);
            assert_eq!(packed.contents_version(), "hk_2014.1.0-r1");
            assert_eq!(packed.packfile().header.pointer_size, 8);
            assert_packed_fo4_animation_signatures(&packed, true);

            let roundtrip_xml = havok_native::api::havok_hkx_to_xml(&packed_bytes).unwrap();
            if output_clip_path.ends_with("Attack1.hkx") {
                let document = roxmltree::Document::parse(&roundtrip_xml).unwrap();
                let annotation_texts = document
                    .descendants()
                    .filter(|node| {
                        node.has_tag_name("hkparam") && node.attribute("name") == Some("text")
                    })
                    .filter_map(|node| node.text())
                    .collect::<Vec<_>>();
                assert!(annotation_texts.contains(&"HitFrame"));
                assert!(!annotation_texts.contains(&"Hit"));
            }
            let roundtrip_clip =
                havok_native::animation::clip::extract_clip(&roundtrip_xml, None).unwrap();
            assert!(!roundtrip_clip.extracted_motion_ref.is_empty());
            assert_eq!(
                roundtrip_clip.track_to_bone_indices,
                declaration
                    .binding
                    .transform_track_to_bone_indices
                    .iter()
                    .map(|index| *index as u32)
                    .collect::<Vec<_>>()
            );
            let emitted_root = &roundtrip_clip.channels[root_track_index];
            assert_eq!(emitted_root.translations.len(), sample_count);
            assert!(
                emitted_root
                    .translations
                    .iter()
                    .all(|key| { key.value[0].abs() < 1.0e-5 && key.value[1].abs() < 1.0e-5 })
            );

            let (reference_duration, reference_samples) = reference_frame_samples(&roundtrip_xml);
            assert!((reference_duration - clip.duration).abs() < 1.0e-4);
            assert_eq!(reference_samples.len(), sample_count);
            assert!(
                reference_samples[0]
                    .iter()
                    .all(|value| value.abs() < 1.0e-6)
            );
            let reference_end = reference_samples.last().unwrap();
            assert!((reference_end[0] - expected_end[0]).abs() < 1.0e-4);
            assert!((reference_end[1] - expected_end[1]).abs() < 1.0e-4);
            assert!(reference_end[2].abs() < 1.0e-6);
            assert!(reference_end[3].abs() < 1.0e-6);
        }
    }

    #[test]
    fn empty_animations_succeeds() {
        let id = make_run();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({ "animations": [] });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertAnimationsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 0);
        drop_run(id).unwrap();
    }

    #[test]
    fn missing_resolved_path_counts_as_warning() {
        let id = make_run();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "animations": [
                    {
                        "source_path": "Meshes/AnimsHumanFemale/idle.kf",
                        "resolved_path": "/nonexistent/idle.kf",
                        "output_clip_path": "Actors/Test/Animations/Idle.hkx",
                        "asset_type": "kf_animation"
                    }
                ]
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertAnimationsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);
        drop_run(id).unwrap();
    }

    #[test]
    fn malformed_compact_phase_counts_failure_without_output_or_registration() {
        use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};

        let Some((malformed_bytes, _)) =
            malformed_compact_kf_bytes("meshes/creatures/nvgecko/mtidle.kf")
        else {
            return;
        };
        let Some(skeleton_path) = fnv_animation_fixture("meshes/creatures/nvgecko/skeleton.nif")
        else {
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let source_path = temp
            .path()
            .join("source/meshes/creatures/nvgecko/mtidle.kf");
        std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        std::fs::write(&source_path, malformed_bytes).unwrap();
        let mod_dir = temp.path().join("mod");
        let sink = Arc::new(SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: mod_dir.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        });

        let id = make_run();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            run.output_sink = Some(sink.clone());
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "animations": [{
                    "source_path": "meshes/creatures/nvgecko/mtidle.kf",
                    "resolved_path": source_path.to_string_lossy(),
                    "output_clip_path": "Actors/B21_FNVGecko/Animations/Idle.hkx",
                    "source_skeleton_path": skeleton_path.to_string_lossy(),
                    "runtime_skeleton_path": "Actors/B21_FNVGecko/CharacterAssets/Skeleton.hkx",
                    "original_skeleton_name": "NVGecko",
                    "extracted_motion_policy": "reject_nonzero"
                }]
            });
            let source_dir = temp.path().join("source");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertAnimationsPhase
                .run(&mut ctx)
                .map_err(|error| RunError::InvalidConfig(error.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 1);
        assert_eq!(report.items_failed, 1);
        assert!(
            !mod_dir
                .join("data/Meshes/Actors/B21_FNVGecko/Animations/Idle.hkx")
                .exists()
        );
        assert!(sink.ba2.as_ref().unwrap().streamed_rel_paths().is_empty());
        drop_run(id).unwrap();
    }

    #[test]
    fn base_game_asset_is_dropped() {
        let id = make_run();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "animations": [
                    {
                        "source_path": "Meshes/Actors/Character/idle.kf",
                        "resolved_path": "/nonexistent/idle.kf",
                        "output_clip_path": "Actors/Test/Animations/Idle.hkx",
                        "asset_type": "kf_animation"
                    }
                ],
                "target_behaviors": ["actors/character/idle.kf"]
            });
            let source_dir = std::path::PathBuf::from("/nonexistent");
            let mod_dir = std::path::PathBuf::from("/nonexistent");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertAnimationsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.records_dropped, 1);
        assert_eq!(report.assets_written, 0);
        assert_eq!(report.warnings, 0);
        drop_run(id).unwrap();
    }

    #[test]
    fn attached_sink_registers_skipped_existing_animation() {
        use crate::sinks::{Ba2ShardWriter, LooseSink, SinkSet, TerrainSidecarSink};

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source/Meshes/AnimsHumanFemale/idle.kf");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"source kf placeholder").unwrap();

        let mod_dir = temp.path().join("mod");
        let out = mod_dir.join("data/Meshes/Actors/B21_FNVGecko/Animations/Idle.hkx");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        std::fs::write(&out, b"existing hkx").unwrap();

        let sink = std::sync::Arc::new(SinkSet {
            ba2: Some(Ba2ShardWriter::new(temp.path().join("spill")).unwrap()),
            loose: LooseSink {
                enabled: false,
                mod_root: mod_dir.clone(),
            },
            terrain: TerrainSidecarSink::default(),
        });

        let id = make_run();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            run.output_sink = Some(sink.clone());
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "animations": [
                    {
                        "source_path": "Meshes/AnimsHumanFemale/idle.kf",
                        "resolved_path": source.to_string_lossy(),
                        "output_clip_path": "Actors/B21_FNVGecko/Animations/Idle.hkx",
                        "asset_type": "kf_animation"
                    }
                ],
                "overwrite_existing": false
            });
            let source_dir = temp.path().join("source");
            let mut ctx = PhaseCtx {
                run,
                mod_path: &mod_dir,
                source_extracted_dir: &source_dir,
                target_extracted_dir: None,
                target_data_dir: None,
                params: &params,
                cancel: &cancel,
            };
            ConvertAnimationsPhase
                .run(&mut ctx)
                .map_err(|e| RunError::InvalidConfig(e.to_string()))
        })
        .unwrap();

        assert_eq!(report.assets_written, 0);
        assert_eq!(report.items_failed, 0);
        assert_eq!(
            sink.ba2.as_ref().unwrap().streamed_rel_paths(),
            vec!["meshes/actors/b21_fnvgecko/animations/idle.hkx"]
        );
        drop_run(id).unwrap();
    }

    #[test]
    fn output_clip_path_drives_runtime_binding_and_deploy_destination() {
        let base = Path::new("/mod");
        let runtime = validate_output_clip_path("Actors/B21_FNVGecko/Animations/Idle.hkx").unwrap();
        assert_eq!(runtime, "Actors\\B21_FNVGecko\\Animations\\Idle.hkx");
        assert_eq!(
            output_clip_deploy_path(base, &runtime),
            Path::new("/mod/data/Meshes/Actors/B21_FNVGecko/Animations/Idle.hkx")
        );
    }

    #[test]
    fn output_clip_path_rejects_non_hkx_prefixes_and_traversal() {
        for invalid in [
            "",
            "../Idle.hkx",
            "Actors/../Idle.hkx",
            "C:/Actors/Idle.hkx",
            "/Actors/Idle.hkx",
            "Meshes/Actors/Idle.hkx",
            "Data/Meshes/Actors/Idle.hkx",
            "Actors/Idle.xml",
            "Actors/Idle.hkt",
        ] {
            assert!(
                validate_output_clip_path(invalid).is_err(),
                "unexpectedly accepted {invalid:?}"
            );
        }
    }

    #[test]
    fn euler_to_quat_identity() {
        let (x, y, z, w) = euler_to_quat(0.0, 0.0, 0.0);
        assert!(x.abs() < 1e-9);
        assert!(y.abs() < 1e-9);
        assert!(z.abs() < 1e-9);
        assert!((w - 1.0).abs() < 1e-9);
    }

    #[test]
    fn sample_at_time_interpolates() {
        let pairs = vec![(0.0f64, 0.0f64), (1.0, 10.0)];
        let v = sample_at_time(&pairs, 0.5);
        assert!((v - 5.0).abs() < 1e-6, "expected 5.0, got {v}");
    }

    #[test]
    fn weapon_family_table_loads() {
        let table = WeaponFamilyTable::load(WEAPON_FAMILY_YAML);
        assert!(!table.families.is_empty(), "families must be non-empty");
        assert!(!table.weapons.is_empty(), "weapons must be non-empty");
        assert!(
            table.families.contains_key("PipeGun"),
            "PipeGun family must be present"
        );
    }

    #[test]
    fn weapon_family_classify() {
        let table = WeaponFamilyTable::load(WEAPON_FAMILY_YAML);
        let fam = table.classify("WeapNV10mmPistol", "");
        assert!(fam.is_some(), "WeapNV10mmPistol should classify");
        let fam2 = table.classify("UnknownGun", "Rifle");
        assert!(fam2.is_some(), "Rifle fallback should classify");
    }

    #[test]
    fn real_fnv_requested_channel_families_decode_without_warnings() {
        let fixtures = [
            (
                "meshes/characters/_1stperson/1hpattack3.kf",
                "NiBSplineCompFloatInterpolator",
            ),
            (
                "meshes/creatures/nvmrhouse/idleanims/wakeup.kf",
                "NiBSplineCompPoint3Interpolator",
            ),
            (
                "meshes/characters/_male/idleanims/dynamicidle_nvdieingsoldier.kf",
                "NiBSplineTransformInterpolator",
            ),
            (
                "meshes/creatures/nvmrhouse/idleanims/wakeup.kf",
                "NiBoolTimelineInterpolator",
            ),
            (
                "meshes/characters/_1stperson/2hmattackspin.kf",
                "BSRotAccumTransfInterpolator",
            ),
        ];
        for (relative_path, expected_type) in fixtures {
            let Some(path) = fnv_animation_fixture(relative_path) else {
                return;
            };
            let bytes = std::fs::read(path).unwrap();
            let nif = NifFile::from_bytes(&bytes, None).unwrap();
            assert!(
                nif.blocks
                    .iter()
                    .any(|block| block.type_name == expected_type),
                "fixture {relative_path} lost {expected_type}"
            );
            let clip = parse_kf_bytes(&bytes, relative_path).unwrap();
            assert!(
                clip.warnings.is_empty(),
                "{relative_path}: {:?}",
                clip.warnings
            );
            assert!(clip.duration.is_finite() && clip.duration >= 0.0);
            for channel in &clip.float_channels {
                assert!(!channel.slot_name.is_empty());
                assert!(!channel.keyframes.is_empty());
                assert!(channel.keyframes.iter().all(|key| {
                    key.time.is_finite() && key.value.len() == 1 && key.value[0].is_finite()
                }));
            }
            if matches!(
                expected_type,
                "NiBSplineCompPoint3Interpolator"
                    | "NiBSplineCompFloatInterpolator"
                    | "NiBoolTimelineInterpolator"
            ) {
                assert!(!clip.float_channels.is_empty(), "{relative_path}");
            }
            if relative_path == "meshes/creatures/nvmrhouse/idleanims/wakeup.kf" {
                assert_eq!(clip.float_channels.len(), 53);
                assert_eq!(
                    bind_fnv_kf_float_tracks(None, &clip.float_channels),
                    Err(FnvKfFloatBindingError::MissingFloatSlots { track_count: 53 })
                );
                let slots = clip
                    .float_channels
                    .iter()
                    .map(|channel| channel.slot_name.clone())
                    .collect::<Vec<_>>();
                assert_eq!(
                    bind_fnv_kf_float_tracks(Some(&slots), &clip.float_channels).unwrap(),
                    (0..53).collect::<Vec<_>>()
                );
            }
            if expected_type == "BSRotAccumTransfInterpolator" {
                let saw = clip
                    .channels
                    .iter()
                    .find(|channel| channel.bone_name == "##SawBlade")
                    .unwrap();
                assert!(!saw.rotations.is_empty());
            }
        }
    }

    #[test]
    fn real_chimera_empty_rot_accum_channel_is_ignored() {
        let relative_path = "meshes/dlcanch/creatures/chimera/1hpattackright.kf";
        let Some(path) = fnv_animation_fixture(relative_path) else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let clip = parse_kf_bytes(&bytes, relative_path).unwrap();
        assert!(
            clip.warnings
                .iter()
                .any(|warning| warning.contains("ignored empty BSRotAccumTransfInterpolator"))
        );
        assert!(
            clip.channels
                .iter()
                .all(|channel| channel.bone_name != "Bip01 L Foot")
        );
        assert!(
            clip.channels
                .iter()
                .any(|channel| channel.bone_name == "Bip01 R Foot")
        );
    }

    #[test]
    fn real_evolved_centaur_omits_absent_weapon_attachment_track() {
        let relative_path = "meshes/creatures/centaur/h2hattackleft.kf";
        let Some(kf_path) = fnv_animation_fixture(relative_path) else {
            return;
        };
        let Some(skeleton_path) =
            fnv_animation_fixture("meshes/creatures/centaur/skeletonevolved.nif")
        else {
            return;
        };
        let bytes = std::fs::read(kf_path).unwrap();
        let raw = parse_kf_bytes(&bytes, relative_path).unwrap();
        assert!(
            raw.channels
                .iter()
                .any(|channel| channel.bone_name == "Weapon")
        );
        let float_slots = raw
            .float_channels
            .iter()
            .map(|channel| channel.slot_name.clone())
            .collect::<Vec<_>>();
        let bone_names = load_ordered_source_skeleton_names(&skeleton_path).unwrap();
        let contract = FnvKfSkeletonContract {
            skeleton_path: "Actors/B21_FNVCentaurEvolved/CharacterAssets/Skeleton.hkx",
            ordered_bone_names: &bone_names,
            ordered_float_slot_names: &float_slots,
        };
        let parsed = parse_fnv_creature_kf(&bytes, relative_path, None, Some(contract)).unwrap();
        assert_eq!(
            parsed.binding.compatibility,
            FnvKfBindingCompatibility::Verified
        );
        assert!(
            !parsed
                .binding
                .target_names
                .iter()
                .any(|name| name == "Weapon")
        );
        let staged = stage_fnv_creature_kf(
            &bytes,
            FnvKfStageRequest {
                source_kf: relative_path,
                output_clip_path: "Actors/B21_FNVCentaurEvolved/Animations/Melee.hkx",
                sequence_index: None,
                skeleton: contract,
                original_skeleton_name: "FNVCentaurEvolved",
                event_map: &HashMap::new(),
                target_sample_rate_hz: None,
                extracted_motion_policy: FnvExtractedMotionPolicy::RejectNonzero,
            },
        )
        .unwrap();
        assert!(
            staged
                .receipt
                .warnings
                .iter()
                .any(|warning| warning.contains("Weapon attachment track"))
        );
    }

    #[test]
    fn real_fnv_stage_returns_validated_artifact_receipt_without_writing() {
        let relative_path = "meshes/creatures/nvgecko/mtidle.kf";
        let Some(kf_path) = fnv_animation_fixture(relative_path) else {
            return;
        };
        let Some(skeleton_path) = fnv_animation_fixture("meshes/creatures/nvgecko/skeleton.nif")
        else {
            return;
        };
        let bytes = std::fs::read(kf_path).unwrap();
        let bone_names = load_ordered_source_skeleton_names(&skeleton_path).unwrap();
        let staged = stage_fnv_creature_kf(
            &bytes,
            FnvKfStageRequest {
                source_kf: relative_path,
                output_clip_path: "Actors/B21_FNVGecko/Animations/Idle.hkx",
                sequence_index: None,
                skeleton: FnvKfSkeletonContract {
                    skeleton_path: "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx",
                    ordered_bone_names: &bone_names,
                    ordered_float_slot_names: &[],
                },
                original_skeleton_name: "NVGecko",
                event_map: &HashMap::new(),
                target_sample_rate_hz: None,
                extracted_motion_policy: FnvExtractedMotionPolicy::RejectNonzero,
            },
        )
        .unwrap();

        assert!(!staged.hkx_bytes.is_empty());
        assert_eq!(staged.receipt.sequence_index, 0);
        assert_eq!(staged.receipt.sequence_name, "Idle");
        assert_eq!(staged.receipt.transform_track_count, 84);
        assert_eq!(staged.receipt.binding.declared_transform_tracks, 84);
        assert_eq!(
            staged.receipt.binding.transform_track_to_bone_indices.len(),
            84
        );
        assert_eq!(staged.receipt.float_track_count, 0);
        assert_eq!(staged.receipt.sample_count, 401);
        assert_eq!(
            staged.receipt.output_clip_path,
            "Actors\\B21_FNVGecko\\Animations\\Idle.hkx"
        );
        assert_eq!(
            staged.receipt.runtime_skeleton_path,
            "Actors\\B21_FNVGecko\\CharacterAssets\\Skeleton.hkx"
        );
        assert_eq!(staged.receipt.original_skeleton_name, "NVGecko");
        assert_eq!(staged.receipt.motion, FnvClipMotion::InPlace);
    }

    #[test]
    fn real_fnv_stationary_scene_idle_requires_float_slots_then_stages() {
        let relative_path = "meshes/creatures/nvmrhouse/idleanims/wakeup.kf";
        let Some(kf_path) = fnv_animation_fixture(relative_path) else {
            return;
        };
        let Some(skeleton_path) = fnv_animation_fixture("meshes/creatures/nvmrhouse/skeleton.nif")
        else {
            return;
        };
        let bytes = std::fs::read(kf_path).unwrap();
        let clip = parse_kf_bytes(&bytes, relative_path).unwrap();
        let float_slots = clip
            .float_channels
            .iter()
            .map(|channel| channel.slot_name.clone())
            .collect::<Vec<_>>();
        assert_eq!(float_slots.len(), 53);
        let bone_names = load_ordered_source_skeleton_names(&skeleton_path).unwrap();
        let event_map = HashMap::new();
        let stage = |ordered_float_slot_names| {
            stage_fnv_creature_kf(
                &bytes,
                FnvKfStageRequest {
                    source_kf: relative_path,
                    output_clip_path: "Actors/B21_FNVMrHouse/Animations/Wakeup.hkx",
                    sequence_index: None,
                    skeleton: FnvKfSkeletonContract {
                        skeleton_path: "Actors\\B21_FNVMrHouse\\CharacterAssets\\Skeleton.hkx",
                        ordered_bone_names: &bone_names,
                        ordered_float_slot_names,
                    },
                    original_skeleton_name: "NVMrHouse",
                    event_map: &event_map,
                    target_sample_rate_hz: None,
                    extracted_motion_policy: FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
                },
            )
        };

        let missing_slots_error = stage(&[]).unwrap_err();
        assert!(
            matches!(
                &missing_slots_error,
                FnvKfConversionError::FloatBinding(
                    FnvKfFloatBindingError::MissingFloatTrackTarget { .. }
                )
            ),
            "{missing_slots_error:?}"
        );
        let staged = stage(&float_slots).unwrap();
        assert!(
            staged
                .receipt
                .warnings
                .iter()
                .any(|warning| warning.contains("stationary scene-controller"))
        );
        assert!(staged.receipt.transform_track_count > 0);
        assert_eq!(staged.receipt.float_track_count, 53);
    }

    #[test]
    fn real_stationary_scene_creature_idles_bind_and_stage() {
        let fixtures = [
            (
                "meshes/creatures/nvmrhouse/mtidle.kf",
                "meshes/creatures/nvmrhouse/skeleton.nif",
                "Actors/B21_FNVMrHouse/Animations/Idle.hkx",
                "FNVMrHouse",
            ),
            (
                "meshes/creatures/nvpenthouemaincomputer/mtidle.kf",
                "meshes/creatures/nvpenthouemaincomputer/skeleton.nif",
                "Actors/B21_FNVPenthouseComputer/Animations/Idle.hkx",
                "FNVPenthouseComputer",
            ),
            (
                "meshes/nvdlc03/creatures/braintank/mtidle.kf",
                "meshes/nvdlc03/creatures/braintank/skeleton.nif",
                "Actors/B21_FNVBrainTank/Animations/Idle.hkx",
                "FNVBrainTank",
            ),
        ];
        for (relative_path, relative_skeleton, output_path, skeleton_name) in fixtures {
            let Some(kf_path) = fnv_animation_fixture(relative_path) else {
                return;
            };
            let Some(skeleton_path) = fnv_animation_fixture(relative_skeleton) else {
                return;
            };
            let bytes = std::fs::read(kf_path).unwrap();
            let raw = parse_kf_bytes(&bytes, relative_path).unwrap();
            let float_slots = raw
                .float_channels
                .iter()
                .map(|channel| channel.slot_name.clone())
                .collect::<Vec<_>>();
            let bone_names = load_ordered_source_skeleton_names(&skeleton_path).unwrap();
            let contract = FnvKfSkeletonContract {
                skeleton_path: output_path,
                ordered_bone_names: &bone_names,
                ordered_float_slot_names: &float_slots,
            };
            let parsed =
                parse_fnv_creature_kf(&bytes, relative_path, None, Some(contract)).unwrap();
            assert_eq!(
                parsed.binding.compatibility,
                FnvKfBindingCompatibility::Verified,
                "{relative_path}"
            );
            let staged = stage_fnv_creature_kf(
                &bytes,
                FnvKfStageRequest {
                    source_kf: relative_path,
                    output_clip_path: output_path,
                    sequence_index: None,
                    skeleton: contract,
                    original_skeleton_name: skeleton_name,
                    event_map: &HashMap::new(),
                    target_sample_rate_hz: None,
                    extracted_motion_policy: FnvExtractedMotionPolicy::RejectNonzero,
                },
            )
            .unwrap();
            assert!(
                staged
                    .receipt
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("stationary scene-controller"))
            );
        }
    }

    #[test]
    fn real_fo3_stage_applies_event_map_and_reports_planar_motion() {
        let relative_path = "meshes/creatures/mirelurk/locomotion/mtforward.kf";
        let Some(kf_path) = fo3_animation_fixture(relative_path) else {
            return;
        };
        let Some(skeleton_path) = fo3_animation_fixture("meshes/creatures/mirelurk/skeleton.nif")
        else {
            return;
        };
        let bytes = std::fs::read(kf_path).unwrap();
        let bone_names = load_ordered_source_skeleton_names(&skeleton_path).unwrap();
        let event_map = HashMap::from([("m:R".to_string(), "FootRight".to_string())]);
        let staged = stage_fnv_creature_kf(
            &bytes,
            FnvKfStageRequest {
                source_kf: relative_path,
                output_clip_path: "Actors/B21_FO3Mirelurk/Animations/WalkForward.hkx",
                sequence_index: Some(0),
                skeleton: FnvKfSkeletonContract {
                    skeleton_path: "Actors\\B21_FO3Mirelurk\\CharacterAssets\\Skeleton.hkx",
                    ordered_bone_names: &bone_names,
                    ordered_float_slot_names: &[],
                },
                original_skeleton_name: "Mirelurk",
                event_map: &event_map,
                target_sample_rate_hz: None,
                extracted_motion_policy: FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
            },
        )
        .unwrap();

        assert_eq!(staged.receipt.event_count, 6);
        assert!(
            staged
                .receipt
                .events
                .iter()
                .any(|event| event.text == "FootRight")
        );
        assert!(
            !staged
                .receipt
                .events
                .iter()
                .any(|event| event.text == "m:R")
        );
        assert!(matches!(
            staged.receipt.motion,
            FnvClipMotion::ExtractedPlanar { .. } | FnvClipMotion::ExtractedPlanarYaw { .. }
        ));
        let roundtrip = havok_native::api::havok_hkx_to_xml(&staged.hkx_bytes).unwrap();
        assert!(roundtrip.contains(">FootRight</hkparam>"));
        assert!(!roundtrip.contains(">m:R</hkparam>"));
    }

    #[test]
    fn real_compact_float_requires_explicit_slots_then_packs_nonidentity_mapping() {
        let relative_path = "meshes/characters/_1stperson/1hpattack3.kf";
        let Some(path) = fnv_animation_fixture(relative_path) else {
            return;
        };
        let mut clip = parse_kf_bytes(&std::fs::read(path).unwrap(), relative_path).unwrap();
        assert!(!clip.float_channels.is_empty());
        clip.channels = vec![BoneChannel {
            bone_name: "Root".to_string(),
            priority: 0,
            rotations: Vec::new(),
            translations: Vec::new(),
            scales: Vec::new(),
        }];
        clip.accum_root.clear();
        assert!(matches!(
            bind_fnv_kf_float_tracks(None, &clip.float_channels),
            Err(FnvKfFloatBindingError::MissingFloatSlots { .. })
        ));
        let slots = clip
            .float_channels
            .iter()
            .rev()
            .map(|channel| channel.slot_name.clone())
            .collect::<Vec<_>>();
        let mapping = bind_fnv_kf_float_tracks(Some(&slots), &clip.float_channels).unwrap();
        assert_eq!(
            mapping,
            (0..slots.len()).rev().collect::<Vec<_>>(),
            "explicit slot order must drive the binding"
        );
        let declaration = bind_fnv_kf_clip(
            &["Root".to_string()],
            &clip,
            "Actors\\Test\\CharacterAssets\\Skeleton.hkx",
            "Actors\\Test\\Animations\\Clip.hkx",
            "TestSkeleton",
            &mapping,
        )
        .unwrap();
        assert_eq!(declaration.binding.original_skeleton_name, "TestSkeleton");
        assert_eq!(declaration.binding.declared_float_tracks, mapping.len());
        assert_eq!(
            declaration.binding.float_track_to_float_slot_indices,
            mapping
        );
        let (xml, motion) = clip_to_havok_xml_with_float_tracks(
            &clip,
            &declaration,
            &mapping,
            "TestSkeleton",
            30.0,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap();
        assert_eq!(motion, FnvClipMotion::InPlace);
        let packed_bytes = havok_native::api::havok_xml_to_hkx(&xml).unwrap();
        let packed = havok_native::hkx::HkxFile::read(&packed_bytes).unwrap();
        assert_packed_fo4_animation_signatures(&packed, false);
        let roundtrip = havok_native::api::havok_hkx_to_xml(&packed_bytes).unwrap();
        let document = roxmltree::Document::parse(&roundtrip).unwrap();
        let parameter = |name| {
            document
                .descendants()
                .find(|node| node.has_tag_name("hkparam") && node.attribute("name") == Some(name))
                .unwrap()
        };
        assert_eq!(
            parameter("numberOfFloatTracks")
                .text()
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap(),
            clip.float_channels.len()
        );
        let reread_mapping = parameter("floatTrackToFloatSlotIndices")
            .text()
            .unwrap_or_default()
            .split_whitespace()
            .map(|value| value.parse::<usize>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(reread_mapping, mapping);
        let expected_samples = target_sample_count(clip.duration, 30.0) * clip.float_channels.len();
        assert_eq!(
            parameter("floats")
                .attribute("numelements")
                .unwrap()
                .parse::<usize>()
                .unwrap(),
            expected_samples
        );
    }

    #[test]
    fn malformed_compact_float_is_typed_fatal_before_output() {
        let relative_path = "meshes/characters/_1stperson/1hpattack3.kf";
        let Some(path) = fnv_animation_fixture(relative_path) else {
            return;
        };
        let mut nif = NifFile::from_bytes(&std::fs::read(&path).unwrap(), None).unwrap();
        let interpolator_index = nif
            .blocks
            .iter()
            .position(|block| block.type_name == "NiBSplineCompFloatInterpolator")
            .unwrap();
        nif.blocks[interpolator_index].set_field("Handle", NifValue::UInt(65_000));
        let malformed = nif.to_bytes().unwrap();
        assert!(matches!(
            parse_kf_bytes(&malformed, relative_path),
            Err(KfParseError::Channel {
                source: ChannelDecodeError::Spline(CompactSplineError::ControlPointRange {
                    channel: "float",
                    handle: 65_000,
                    ..
                }),
                ..
            })
        ));

        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("malformed.kf");
        let output = temp.path().join("must_not_exist.hkx");
        std::fs::write(&source, malformed).unwrap();
        let error = convert_kf_to_hkx_with_channels(
            &source,
            &output,
            &HashMap::new(),
            relative_path,
            "Actors/Test/Animations/Malformed.hkx",
            Some(&["Root".to_string()]),
            Some(&[]),
            None,
            Some("Actors/Test/CharacterAssets/Skeleton.hkx"),
            Some("TestSkeleton"),
            None,
            FnvExtractedMotionPolicy::RejectNonzero,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, FnvKfConversionError::ChannelDecode { .. }));
        assert!(!output.exists());
    }

    #[test]
    fn real_point_scalar_and_bool_tracks_pack_all_53_slots() {
        let relative_path = "meshes/creatures/nvmrhouse/idleanims/wakeup.kf";
        let Some(path) = fnv_animation_fixture(relative_path) else {
            return;
        };
        let mut clip = parse_kf_bytes(&std::fs::read(path).unwrap(), relative_path).unwrap();
        assert_eq!(clip.float_channels.len(), 53);
        assert!(
            clip.float_channels
                .iter()
                .flat_map(|channel| &channel.keyframes)
                .any(|key| key.interpolation == Interpolation::Constant)
        );
        clip.channels = vec![BoneChannel {
            bone_name: "Root".to_string(),
            priority: 0,
            rotations: Vec::new(),
            translations: Vec::new(),
            scales: Vec::new(),
        }];
        clip.accum_root.clear();
        let slots = clip
            .float_channels
            .iter()
            .map(|channel| channel.slot_name.clone())
            .collect::<Vec<_>>();
        let mapping = bind_fnv_kf_float_tracks(Some(&slots), &clip.float_channels).unwrap();
        assert_eq!(mapping, (0..53).collect::<Vec<_>>());
        let declaration = bind_fnv_kf_clip(
            &["Root".to_string()],
            &clip,
            "Actors\\Test\\CharacterAssets\\Skeleton.hkx",
            "Actors\\Test\\Animations\\Wakeup.hkx",
            "MrHouse",
            &mapping,
        )
        .unwrap();
        let (xml, motion) = clip_to_havok_xml_with_float_tracks(
            &clip,
            &declaration,
            &mapping,
            "MrHouse",
            2.0,
            FnvExtractedMotionPolicy::RejectNonzero,
        )
        .unwrap();
        assert_eq!(motion, FnvClipMotion::InPlace);
        let packed = havok_native::api::havok_xml_to_hkx(&xml).unwrap();
        let roundtrip = havok_native::api::havok_hkx_to_xml(&packed).unwrap();
        let document = roxmltree::Document::parse(&roundtrip).unwrap();
        let float_count = document
            .descendants()
            .find(|node| {
                node.has_tag_name("hkparam")
                    && node.attribute("name") == Some("numberOfFloatTracks")
            })
            .unwrap()
            .text()
            .unwrap()
            .trim()
            .parse::<usize>()
            .unwrap();
        assert_eq!(float_count, 53);
        let binding_count = document
            .descendants()
            .find(|node| {
                node.has_tag_name("hkparam")
                    && node.attribute("name") == Some("floatTrackToFloatSlotIndices")
            })
            .unwrap()
            .attribute("numelements")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        assert_eq!(binding_count, 53);
    }

    #[test]
    fn multi_sequence_kf_is_cataloged_and_requires_explicit_index() {
        let relative_path = "meshes/characters/_male/idleanims/1stp_stooldynamicidle.kf";
        let Some(path) = fnv_animation_fixture(relative_path) else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let catalog = catalog_fnv_kf_sequences(&bytes, relative_path).unwrap();
        assert_eq!(catalog.len(), 2);
        assert_eq!(
            catalog.iter().map(|entry| entry.index).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert!(catalog.iter().all(|entry| !entry.name.is_empty()));
        assert!(matches!(
            parse_kf_bytes(&bytes, relative_path),
            Err(KfParseError::MultipleSequences { count: 2, .. })
        ));
        assert!(matches!(
            parse_fnv_creature_kf(&bytes, relative_path, None, None),
            Err(KfParseError::MultipleSequences { count: 2, .. })
        ));
        assert!(matches!(
            stage_fnv_creature_kf(
                &bytes,
                FnvKfStageRequest {
                    source_kf: relative_path,
                    output_clip_path: "Actors/Test/Animations/Multi.hkx",
                    sequence_index: None,
                    skeleton: FnvKfSkeletonContract {
                        skeleton_path: "Actors/Test/CharacterAssets/Skeleton.hkx",
                        ordered_bone_names: &[],
                        ordered_float_slot_names: &[],
                    },
                    original_skeleton_name: "Test",
                    event_map: &HashMap::new(),
                    target_sample_rate_hz: None,
                    extracted_motion_policy: FnvExtractedMotionPolicy::RejectNonzero,
                }
            ),
            Err(FnvKfConversionError::MultipleSequences { count: 2, .. })
        ));
        let selected = parse_fnv_creature_kf(&bytes, relative_path, Some(1), None).unwrap();
        assert_eq!(selected.sequence_index, 1);
        assert_eq!(selected.sequence_name, catalog[1].name);
        assert!(parse_kf_sequence_bytes(&bytes, relative_path, 0).is_ok());
        assert!(parse_kf_sequence_bytes(&bytes, relative_path, 1).is_ok());
        assert!(matches!(
            parse_kf_sequence_bytes(&bytes, relative_path, 2),
            Err(KfParseError::SequenceIndexOutOfRange {
                index: 2,
                count: 2,
                ..
            })
        ));
    }

    #[test]
    fn planar_yaw_root_motion_roundtrips_and_zeroes_root_yaw() {
        let key = |time, value| AnimationKeyframe {
            time,
            value,
            interpolation: Interpolation::Linear,
            forward: None,
            backward: None,
            tbc: None,
        };
        let mut clip = binding_test_clip(&["Root"]);
        clip.duration = 1.0;
        clip.accum_root = "Root".to_string();
        clip.channels[0].translations = vec![
            key(0.0, vec![0.0, 0.0, 0.0]),
            key(1.0, vec![10.0, 0.0, 0.0]),
        ];
        clip.channels[0].rotations = vec![
            key(0.0, vec![0.0, 0.0, 0.0, 1.0]),
            key(
                1.0,
                vec![
                    0.0,
                    0.0,
                    (std::f64::consts::FRAC_PI_4).sin(),
                    (std::f64::consts::FRAC_PI_4).cos(),
                ],
            ),
        ];
        let declaration = bind_fnv_kf_clip(
            &["Root".to_string()],
            &clip,
            "Actors\\Test\\Skeleton.hkx",
            "Actors\\Test\\Yaw.hkx",
            "TestSkeleton",
            &[],
        )
        .unwrap();
        let (xml, motion) = clip_to_havok_xml(
            &clip,
            &declaration,
            "TestSkeleton",
            30.0,
            FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
        )
        .unwrap();
        assert!(matches!(
            motion,
            FnvClipMotion::ExtractedPlanarYaw {
                end_displacement: [x, y],
                end_yaw_radians,
                ..
            } if (x - 10.0).abs() < 1.0e-6
                && y.abs() < 1.0e-6
                && (end_yaw_radians - std::f64::consts::FRAC_PI_2).abs() < 1.0e-6
        ));
        let packed = havok_native::api::havok_xml_to_hkx(&xml).unwrap();
        let roundtrip = havok_native::api::havok_hkx_to_xml(&packed).unwrap();
        let (_, samples) = reference_frame_samples(&roundtrip);
        assert!((samples.last().unwrap()[0] - 10.0).abs() < 1.0e-5);
        assert!((samples.last().unwrap()[3] - std::f64::consts::FRAC_PI_2).abs() < 1.0e-5);
        let roundtrip_clip = havok_native::animation::clip::extract_clip(&roundtrip, None).unwrap();
        assert!(roundtrip_clip.channels[0].rotations.iter().all(|key| {
            key.value[0].abs() < 1.0e-6
                && key.value[1].abs() < 1.0e-6
                && key.value[2].abs() < 1.0e-6
                && (key.value[3] - 1.0).abs() < 1.0e-6
        }));
    }

    #[test]
    fn root_motion_rejects_vertical_pitch_and_roll() {
        let key = |time, value| AnimationKeyframe {
            time,
            value,
            interpolation: Interpolation::Linear,
            forward: None,
            backward: None,
            tbc: None,
        };
        let mut clip = binding_test_clip(&["Root"]);
        clip.duration = 1.0;
        clip.accum_root = "Root".to_string();
        let declaration = bind_fnv_kf_clip(
            &["Root".to_string()],
            &clip,
            "Actors\\Test\\Skeleton.hkx",
            "Actors\\Test\\Unsupported.hkx",
            "TestSkeleton",
            &[],
        )
        .unwrap();

        clip.channels[0].translations =
            vec![key(0.0, vec![0.0, 0.0, 0.0]), key(1.0, vec![0.0, 0.0, 1.0])];
        assert!(matches!(
            extract_planar_root_motion(
                &clip,
                &declaration,
                31,
                FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
            ),
            Err(FnvKfConversionError::UnsupportedVerticalRootMotion { .. })
        ));

        clip.channels[0].translations = Vec::new();
        clip.channels[0].rotations = vec![
            key(0.0, vec![0.0, 0.0, 0.0, 1.0]),
            key(1.0, vec![(0.05_f64).sin(), 0.0, 0.0, (0.05_f64).cos()]),
        ];
        assert!(matches!(
            extract_planar_root_motion(
                &clip,
                &declaration,
                31,
                FnvExtractedMotionPolicy::ExtractPlanarReferenceFrame,
            ),
            Err(FnvKfConversionError::UnsupportedPitchRollRootMotion { .. })
        ));
    }

    #[test]
    #[ignore = "bounded manual FNV creatures corpus audit; extracted assets are optional"]
    fn fnv_creature_kf_corpus_has_no_silent_channel_warnings() {
        let Some(sample) = fnv_animation_fixture("meshes/creatures/nvgecko/mtidle.kf") else {
            return;
        };
        let root = sample.ancestors().nth(2).unwrap();
        let mut parsed_sequences = 0usize;
        let mut failures = Vec::new();
        for path in recursive_kf_paths(root) {
            let bytes = std::fs::read(&path).unwrap();
            let relative = path.strip_prefix(root).unwrap().to_string_lossy();
            let catalog = match catalog_fnv_kf_sequences(&bytes, &relative) {
                Ok(catalog) => catalog,
                Err(error) => {
                    failures.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            for entry in catalog {
                match parse_kf_sequence_bytes(&bytes, &relative, entry.index) {
                    Ok(clip) if clip.warnings.is_empty() => parsed_sequences += 1,
                    Ok(clip) => failures.push(format!(
                        "{}[{}]: warnings {:?}",
                        path.display(),
                        entry.index,
                        clip.warnings
                    )),
                    Err(error) => {
                        failures.push(format!("{}[{}]: {error}", path.display(), entry.index))
                    }
                }
            }
        }
        assert!(parsed_sequences > 0);
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn interp_keyframes_single() {
        let keys = vec![AnimationKeyframe {
            time: 0.0,
            value: vec![1.0, 2.0, 3.0],
            interpolation: Interpolation::Linear,
            forward: None,
            backward: None,
            tbc: None,
        }];
        let v = interp_keyframes(&keys, 5.0);
        assert_eq!(v, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn sample_rotation_uses_shortest_quaternion_hemisphere() {
        let key = |time, value| AnimationKeyframe {
            time,
            value,
            interpolation: Interpolation::Linear,
            forward: None,
            backward: None,
            tbc: None,
        };
        let rotation = sample_rotation(
            &[
                key(0.0, vec![0.0, 0.0, 0.0, 1.0]),
                key(1.0, vec![0.0, 0.0, 0.0, -1.0]),
            ],
            0.5,
        );
        assert!(rotation.0.abs() < 1.0e-9);
        assert!(rotation.1.abs() < 1.0e-9);
        assert!(rotation.2.abs() < 1.0e-9);
        assert!((rotation.3.abs() - 1.0).abs() < 1.0e-9);
    }
}
