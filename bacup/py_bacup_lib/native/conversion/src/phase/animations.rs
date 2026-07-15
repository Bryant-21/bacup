// Phase: convert_animations
//
// Params shape (JSON):
// {
//   "animations": [
//     {
//       "source_path":   "Meshes/AnimsHumanFemale/idle.kf",  // relative game path
//       "resolved_path": "/abs/path/idle.kf",                 // absolute disk path
//       "asset_type":    "animation" | "kf_animation"         // optional
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
// Implementation notes:
//   KF .kf files are NIF binary files with NiControllerSequence as root block.
//   Conversion pipeline:
//     1. Read .kf via nif_core_native (binary NIF parse)
//     2. Extract AnimationClip (bone channels, events, float channels)
//     3. Remap events via inline event_map param
//     4. Serialise to hkaInterleavedUncompressedAnimation XML
//     5. Pack to binary .hkx via havok_native
//
//   Weapon-family classification (weapon_family_table.yaml embedded) is
//   available for callers that need overlay synthesis, but weapon overlay
//   synthesis itself is left to the Python orchestrator because it requires
//   per-weapon EditorID context from the translated WEAP records.
//

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nif_core_native::model::{NifFile, NifValue};
use serde_json::Value as JsonValue;

use crate::phase::progress::ProgressReporter;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

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
            let out_path = animation_output_path(&mod_path, &asset.source_path, ".kf", ".hkx");

            if !overwrite_existing && out_path.exists() {
                if !register_with_sink(&out_path) {
                    sink_failures += 1;
                }
                continue;
            }

            match convert_kf_to_hkx(
                resolved,
                &out_path,
                &inline_event_map,
                ctx,
                &asset.source_path,
            ) {
                Ok(clip_warnings) => {
                    assets_written += 1;
                    if !register_with_sink(&out_path) {
                        sink_failures += 1;
                    }
                    for w in &clip_warnings {
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
                            "[KF->HKX] {} -> {}",
                            asset.source_path,
                            out_path.display()
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

/// Returns Ok(warnings) on success, Err(message) on failure.
fn convert_kf_to_hkx(
    kf_path: &Path,
    out_path: &Path,
    event_map: &HashMap<String, String>,
    _ctx: &mut PhaseCtx<'_>,
    source_rel: &str,
) -> Result<Vec<String>, String> {
    // 1. Read KF bytes and parse the NIF structure.
    let kf_bytes = std::fs::read(kf_path).map_err(|e| format!("read: {e}"))?;
    let mut clip = parse_kf_bytes(&kf_bytes, source_rel)?;

    // 2. Remap animation events using the inline map.
    clip.events = clip
        .events
        .into_iter()
        .filter_map(|ev| {
            match event_map.get(&ev.text) {
                Some(mapped) if mapped.is_empty() => None, // explicit drop
                Some(mapped) => Some(AnimationEvent {
                    time: ev.time,
                    text: mapped.clone(),
                }),
                None => Some(ev),
            }
        })
        .collect();

    let parse_warnings = std::mem::take(&mut clip.warnings);

    // 3. Serialise the clip to Havok animation XML.
    let xml = clip_to_havok_xml(&clip)?;

    // 4. Pack XML → HKX via havok_native.
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }

    let hkx_bytes = havok_native::api::havok_xml_to_hkx(&xml).map_err(|e| e.to_string())?;
    std::fs::write(out_path, &hkx_bytes).map_err(|e| format!("write: {e}"))?;

    Ok(parse_warnings)
}

// ---------------------------------------------------------------------------
// KF NIF parser
// ---------------------------------------------------------------------------
//
// .kf files are standard NIF binary files. We parse them using
// nif_core_native::model::NifFile::from_bytes, then query NifBlock fields
// via the NifBlockExt trait defined below.

#[derive(Debug, Clone)]
struct AnimationEvent {
    time: f64,
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
}

#[allow(dead_code)]
impl Interpolation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "LINEAR_KEY",
            Self::Quadratic => "QUADRATIC_KEY",
            Self::Tbc => "TBC_KEY",
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
    target_name: String,
    property_type: String,
    controller_type: String,
    keyframes: Vec<AnimationKeyframe>,
}

#[derive(Debug, Default)]
struct AnimationClip {
    name: String,
    duration: f64,
    cycle_type: String,
    frequency: f64,
    #[allow(dead_code)]
    accum_root: String,
    channels: Vec<BoneChannel>,
    #[allow(dead_code)]
    float_channels: Vec<FloatChannel>,
    events: Vec<AnimationEvent>,
    warnings: Vec<String>,
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

fn parse_kf_bytes(bytes: &[u8], source_path: &str) -> Result<AnimationClip, String> {
    let nif = NifFile::from_bytes(bytes, None)
        .map_err(|e| format!("NIF parse failed ({source_path}): {e}"))?;

    // Find NiControllerSequence block.
    let seq = nif
        .blocks
        .iter()
        .find(|b| b.type_name == "NiControllerSequence")
        .ok_or_else(|| format!("No NiControllerSequence in {source_path}"))?;

    let name = seq.string_field("Name").unwrap_or_default();
    let frequency = seq.f64_field("Frequency").unwrap_or(1.0);
    let start_time = seq.f64_field("Start Time").unwrap_or(0.0);
    let stop_time = seq.f64_field("Stop Time").unwrap_or(0.0);
    let duration = (stop_time - start_time).max(0.0);
    let cycle_raw = seq.u32_field("Cycle Type").unwrap_or(1);
    let cycle_type = match cycle_raw {
        0 => "loop",
        2 => "reverse",
        _ => "clamp",
    }
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
                    events.push(AnimationEvent { time, text });
                }
            }
        }
    }

    // Controlled blocks → channels.
    let mut bone_channels: Vec<BoneChannel> = Vec::new();
    let mut float_channels: Vec<FloatChannel> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for cb in seq.array_of_structs("Controlled Blocks") {
        let interp_ref = cb.block_ref_or("Interpolator", -1);
        let node_name = cb.string_or("Node Name", "");
        let ctrl_type = cb.string_or("Controller Type", "");
        let priority = cb.u32_or("Priority", 26);
        let prop_type = cb.string_or("Property Type", "");

        if interp_ref < 0 {
            warnings.push(format!("No interpolator for '{node_name}'"));
            continue;
        }

        let interp = match nif.blocks.get(interp_ref as usize) {
            Some(b) => b,
            None => {
                warnings.push(format!(
                    "Invalid interpolator ref {interp_ref} for '{node_name}'"
                ));
                continue;
            }
        };

        match interp.type_name.as_str() {
            "NiTransformInterpolator" => {
                let ch = read_transform_channel(&nif, interp, &node_name, priority, &mut warnings);
                bone_channels.push(ch);
            }
            "NiFloatInterpolator" | "NiBoolInterpolator" => {
                if let Some(fc) = read_float_channel(
                    &nif,
                    interp,
                    &node_name,
                    &ctrl_type,
                    &prop_type,
                    &mut warnings,
                ) {
                    float_channels.push(fc);
                }
            }
            other => {
                warnings.push(format!(
                    "Unsupported interpolator {other} for '{node_name}'"
                ));
            }
        }
    }

    Ok(AnimationClip {
        name,
        duration,
        cycle_type,
        frequency,
        accum_root,
        channels: bone_channels,
        float_channels,
        events,
        warnings,
    })
}

fn read_transform_channel(
    nif: &NifFile,
    interp: &nif_core_native::model::NifBlock,
    bone_name: &str,
    priority: u32,
    warnings: &mut Vec<String>,
) -> BoneChannel {
    let empty = BoneChannel {
        bone_name: bone_name.to_string(),
        priority,
        rotations: Vec::new(),
        translations: Vec::new(),
        scales: Vec::new(),
    };

    let data_ref = match interp.block_ref_field("Data") {
        Some(r) => r,
        None => return empty,
    };

    let data = match nif.blocks.get(data_ref) {
        Some(b) if b.type_name == "NiTransformData" => b,
        Some(b) => {
            warnings.push(format!(
                "Expected NiTransformData at block {data_ref} for '{bone_name}', got {}",
                b.type_name
            ));
            return empty;
        }
        None => {
            warnings.push(format!("No block {data_ref} for '{bone_name}'"));
            return empty;
        }
    };

    BoneChannel {
        bone_name: bone_name.to_string(),
        priority,
        rotations: read_rotation_keys(data, bone_name, warnings),
        translations: read_translation_keys(data),
        scales: read_scale_keys(data),
    }
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
    warnings: &mut Vec<String>,
) -> Option<FloatChannel> {
    let data_ref = interp.block_ref_field("Data")?;
    let data = nif.blocks.get(data_ref)?;

    // Try sub-object "Data" then "Keys" for raw key list.
    let raw_keys = if let Some(d) = data.struct_field("Data") {
        d.array_of_structs("Keys")
    } else if let Some(keys) = data.struct_field("Keys") {
        // Wrap the array directly
        vec![keys]
    } else {
        data.array_of_structs("Keys")
    };

    if raw_keys.is_empty() {
        warnings.push(format!("No key data for float channel '{target_name}'"));
        return None;
    }

    let keyframes: Vec<AnimationKeyframe> = raw_keys
        .iter()
        .map(|k| AnimationKeyframe {
            time: k.f64_or("Time", 0.0),
            value: vec![k.f64_or("Value", 0.0)],
            interpolation: Interpolation::Linear,
            forward: None,
            backward: None,
            tbc: None,
        })
        .collect();

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

    Some(FloatChannel {
        target_name: target_name.to_string(),
        property_type: prop.to_string(),
        controller_type: controller_type.to_string(),
        keyframes,
    })
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
    let v = interp_keyframes(keys, t);
    match v.as_slice() {
        [x, y, z, w, ..] => (*x, *y, *z, *w),
        _ => (0.0, 0.0, 0.0, 1.0),
    }
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

fn clip_to_havok_xml(clip: &AnimationClip) -> Result<String, String> {
    let num_bones = clip.channels.len();
    let duration = clip.duration.max(0.001);
    let frequency = if clip.frequency > 0.0 {
        clip.frequency
    } else {
        30.0
    };
    let num_frames = ((duration * frequency).ceil() as usize).max(1);
    let frame_duration = 1.0 / frequency;

    // Build per-frame interleaved transforms: bone0@t0, bone1@t0, ..., bone0@t1, ...
    let mut transform_data = String::new();
    for frame in 0..num_frames {
        let t = frame as f64 * frame_duration;
        for ch in &clip.channels {
            let (tx, ty, tz) = sample_translation(&ch.translations, t);
            let (qx, qy, qz, qw) = sample_rotation(&ch.rotations, t);
            let scale = sample_scale_val(&ch.scales, t);
            transform_data.push_str(&format!(
                "\n                    ({tx:.6} {ty:.6} {tz:.6})\
                 ({qx:.6} {qy:.6} {qz:.6} {qw:.6})\
                 ({scale:.6} {scale:.6} {scale:.6})"
            ));
        }
    }
    let total_transforms = num_frames * num_bones;

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

    let cycle_mode = match clip.cycle_type.as_str() {
        "loop" => "CYCLIC",
        "reverse" => "CYCLIC_SYMMETRIC",
        _ => "ACYCLIC",
    };

    let bone_indices: String = (0..num_bones)
        .map(|i| format!("\n                    {i}"))
        .collect();

    let xml = format!(
        r##"<?xml version="1.0" encoding="ascii"?>
<hkpackfile classversion="8" contentsversion="hk_2014.1.0-r1" toplevelobject="#0001">
    <hksection name="__data__">
        <hkobject name="#0001" class="hkRootLevelContainer" signature="0x2772c11e">
            <hkparam name="namedVariants" numelements="1">
                <hkobject>
                    <hkparam name="name">Merged Animation Container</hkparam>
                    <hkparam name="className">hkaAnimationContainer</hkparam>
                    <hkparam name="variant">#0002</hkparam>
                </hkobject>
            </hkparam>
        </hkobject>
        <hkobject name="#0002" class="hkaAnimationContainer" signature="0x8dc20333">
            <hkparam name="skeletons" numelements="0"></hkparam>
            <hkparam name="animations" numelements="1">#0003</hkparam>
            <hkparam name="bindings" numelements="1">#0004</hkparam>
            <hkparam name="attachments" numelements="0"></hkparam>
            <hkparam name="skins" numelements="0"></hkparam>
        </hkobject>
        <hkobject name="#0003" class="hkaInterleavedUncompressedAnimation" signature="0x930af031">
            <hkparam name="duration">{duration:.6}</hkparam>
            <hkparam name="numberOfTransformTracks">{num_bones}</hkparam>
            <hkparam name="numberOfFloatTracks">0</hkparam>
            <hkparam name="annotationTracks" numelements="{num_bones}">{annotation_section}
            </hkparam>
            <hkparam name="transforms" numelements="{total_transforms}">{transform_data}
            </hkparam>
            <hkparam name="floats" numelements="0"></hkparam>
        </hkobject>
        <hkobject name="#0004" class="hkaAnimationBinding" signature="0x35b1a3b0">
            <hkparam name="originalSkeletonName"></hkparam>
            <hkparam name="animation">#0003</hkparam>
            <hkparam name="blendHint">{cycle_mode}</hkparam>
            <hkparam name="transformTrackToBoneIndices" numelements="{num_bones}">{bone_indices}
            </hkparam>
            <hkparam name="floatTrackToFloatSlotIndices" numelements="0"></hkparam>
        </hkobject>
    </hksection>
</hkpackfile>
"##
    );

    Ok(xml)
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

struct AnimationAsset {
    source_path: String,
    resolved_path: String,
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
            Ok(AnimationAsset {
                source_path,
                resolved_path,
            })
        })
        .collect()
}

fn animation_output_path(
    mod_path: &Path,
    source_path: &str,
    source_ext: &str,
    target_ext: &str,
) -> PathBuf {
    let rel = mesh_relative_animation_path(source_path);
    let rel = if rel.to_lowercase().ends_with(source_ext) {
        format!("{}{target_ext}", &rel[..rel.len() - source_ext.len()])
    } else {
        rel.to_string()
    };

    let mut out = mod_path.to_path_buf();
    out.push("data");
    out.push("Meshes");
    for component in rel.split('/') {
        if !component.is_empty() {
            out.push(component);
        }
    }
    out
}

fn mesh_relative_animation_path(source_path: &str) -> String {
    let mut rel = source_path.replace('\\', "/");
    rel = rel.trim_start_matches('/').to_string();
    if rel.len() >= 5 && rel[..5].eq_ignore_ascii_case("data/") {
        rel = rel[5..].to_string();
    }
    if rel.len() >= 7 && rel[..7].eq_ignore_ascii_case("meshes/") {
        rel = rel[7..].to_string();
    }
    strip_known_asset_prefix(&rel).to_string()
}

fn strip_known_asset_prefix(path: &str) -> &str {
    let Some((first, rest)) = path.split_once('/') else {
        return path;
    };
    if is_known_asset_prefix(first) {
        rest
    } else {
        path
    }
}

fn is_known_asset_prefix(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "fo4" | "fo76" | "fnv" | "fo3" | "skyrim" | "skyrimse" | "starfield" | "oblivion"
    )
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
    fn base_game_asset_is_dropped() {
        let id = make_run();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let cancel = Arc::new(AtomicBool::new(false));
            let params = serde_json::json!({
                "animations": [
                    {
                        "source_path": "Meshes/Actors/Character/idle.kf",
                        "resolved_path": "/nonexistent/idle.kf",
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
        let out = mod_dir.join("data/Meshes/AnimsHumanFemale/idle.hkx");
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
            vec!["meshes/animshumanfemale/idle.hkx"]
        );
        drop_run(id).unwrap();
    }

    #[test]
    fn animation_output_path_strips_source_prefix_component() {
        let base = Path::new("/mod");
        let out = animation_output_path(base, "Meshes/fnv/AnimsHumanFemale/idle.kf", ".kf", ".hkx");
        assert_eq!(out, Path::new("/mod/data/Meshes/AnimsHumanFemale/idle.hkx"));
    }

    #[test]
    fn animation_output_path_unprefixed() {
        let base = Path::new("/mod");
        let out = animation_output_path(base, "Meshes/AnimsHumanFemale/idle.kf", ".kf", ".hkx");
        assert_eq!(out, Path::new("/mod/data/Meshes/AnimsHumanFemale/idle.hkx"));
    }

    #[test]
    fn animation_output_path_adds_meshes_root_for_actor_relative_path() {
        let base = Path::new("/mod");
        let out = animation_output_path(
            base,
            "Actors/GraftonMonster/animations/attack1.hkx",
            ".kf",
            ".hkx",
        );
        assert_eq!(
            out,
            Path::new("/mod/data/Meshes/Actors/GraftonMonster/animations/attack1.hkx")
        );
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
}
