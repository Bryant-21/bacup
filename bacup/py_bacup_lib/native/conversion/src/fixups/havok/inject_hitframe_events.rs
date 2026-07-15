//! Inject missing HitFrame annotation events into attack animations.
//!

//!
//! # What this does
//! FO76 attack animations have `preHitFrame` and `weaponSwing` but no `HitFrame`.
//! FO4 requires `HitFrame` for damage timing.  This fixup scans all `attack*.hkx`
//! files in `ctx.mod_path/meshes/.../animations/` directories, and injects a
//! `HitFrame` annotation at the most appropriate time:
//!   1. `WeaponSweepAttackStart` time (if present) — the actual impact moment.
//!   2. Midpoint between `weaponSwing` and the next later event (fallback).
//!   3. `weaponSwing + 0.1` if weaponSwing is the last event.
//!   4. `preHitFrame + 0.1` if no weaponSwing exists.
//!
//! Only fires if the file has `preHitFrame` but no `HitFrame`.
//!
//! # FixupReport mapping
//! `records_changed` = number of animation files patched.

use std::path::{Path, PathBuf};

use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxMember, read_packfile};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct InjectHitframeEventsFixup;

impl Fixup for InjectHitframeEventsFixup {
    fn name(&self) -> &'static str {
        "inject_hitframe_events"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::AssetOnly
    }

    fn asset_phase_allowed(&self, phases: &AssetPhaseFlags) -> bool {
        phases.animations
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
        let mod_path = match config.mod_path.as_deref() {
            Some(path) => path,
            None => return Ok(FixupReport::empty()),
        };
        inject_hitframe_events_in_mod_path(mod_path)
    }
}

// ---------------------------------------------------------------------------
// Mod-path entry point (used by postprocess wave)
// ---------------------------------------------------------------------------

pub fn inject_hitframe_events_in_mod_path(mod_path: &Path) -> Result<FixupReport, FixupError> {
    let mut files_patched = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_attack_hkx_files(
            &meshes_root,
            &mut |hkx_path| match process_inject_hitframe(hkx_path) {
                Ok(true) => files_patched += 1,
                Ok(false) | Err(_) => {}
            },
        );
    }
    Ok(FixupReport {
        records_changed: files_patched,
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

// ---------------------------------------------------------------------------
// Core algorithm
// ---------------------------------------------------------------------------

/// Returns `true` if the file was modified.
fn process_inject_hitframe(hkx_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(hkx_path)?;
    let mut hkx = read_packfile(&data)?;

    let mut modified = false;

    for obj in hkx.objects_mut() {
        if obj.class_name != "hkaSplineCompressedAnimation"
            && obj.class_name != "hkaInterleavedUncompressedAnimation"
        {
            continue;
        }

        for member in &mut obj.members {
            if member.name != "annotationTracks" {
                continue;
            }
            if let HkxValue::Array(tracks) = &mut member.value {
                // Only check track 0 — root bone track where events live.
                if let Some(track) = tracks.first_mut() {
                    if let HkxValue::Object(track_members) = track {
                        for tm in track_members.iter_mut() {
                            if tm.name != "annotations" {
                                continue;
                            }
                            if let HkxValue::Array(anns) = &mut tm.value {
                                if inject_hitframe_into(anns) {
                                    modified = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if modified {
        let out = hkx.save();
        std::fs::write(hkx_path, out)?;
    }

    Ok(modified)
}

/// Collect `(time, text)` pairs from an annotation array.
fn collect_events(anns: &[HkxValue]) -> Vec<(f32, String)> {
    let mut events = Vec::new();
    for ann in anns {
        let members = match ann.as_object_members() {
            Some(m) => m,
            None => continue,
        };
        let mut time_val: Option<f32> = None;
        let mut text_val: Option<String> = None;
        for m in members {
            if m.name == "time" {
                if let HkxValue::F32(t) = m.value {
                    time_val = Some(t);
                }
            }
            if m.name == "text" {
                if let HkxValue::String { value, .. } = &m.value {
                    text_val = Some(value.clone());
                }
            }
        }
        if let (Some(t), Some(txt)) = (time_val, text_val) {
            events.push((t, txt));
        }
    }
    events
}

/// Build a minimal `hkaAnnotationTrackAnnotation` object as an `HkxValue::Object`.
fn make_hitframe_annotation(time: f32) -> HkxValue {
    HkxValue::Object(vec![
        HkxMember {
            name: "time".to_string(),
            value: HkxValue::F32(time),
        },
        HkxMember {
            name: "text".to_string(),
            value: HkxValue::String {
                value: "HitFrame".to_string(),
                is_null: false,
            },
        },
    ])
}

/// Try to inject a HitFrame annotation into `anns`.  Returns `true` if injected.
fn inject_hitframe_into(anns: &mut Vec<HkxValue>) -> bool {
    let events = collect_events(anns);

    let has_hitframe = events
        .iter()
        .any(|(_, t)| t.eq_ignore_ascii_case("HitFrame"));
    let has_prehitframe = events
        .iter()
        .any(|(_, t)| t.eq_ignore_ascii_case("preHitFrame"));

    if has_hitframe || !has_prehitframe {
        return false;
    }

    // Determine HitFrame time.
    let hitframe_time: f32 = {
        // Prefer WeaponSweepAttackStart time.
        let sweep: Vec<f32> = events
            .iter()
            .filter(|(_, t)| t.eq_ignore_ascii_case("weaponSweepAttackStart"))
            .map(|(t, _)| *t)
            .collect();
        if !sweep.is_empty() {
            sweep[0]
        } else {
            // Find weaponSwing.
            let ws: Vec<f32> = events
                .iter()
                .filter(|(_, t)| t.eq_ignore_ascii_case("weaponSwing"))
                .map(|(t, _)| *t)
                .collect();
            if !ws.is_empty() {
                let ws_time = ws[0];
                let later: Vec<f32> = events
                    .iter()
                    .filter(|(t, _)| *t > ws_time)
                    .map(|(t, _)| *t)
                    .collect();
                if !later.is_empty() {
                    (ws_time + later.iter().cloned().fold(f32::INFINITY, f32::min)) / 2.0
                } else {
                    ws_time + 0.1
                }
            } else {
                // No weaponSwing — put HitFrame slightly after preHitFrame.
                let phf: Vec<f32> = events
                    .iter()
                    .filter(|(_, t)| t.eq_ignore_ascii_case("preHitFrame"))
                    .map(|(t, _)| *t)
                    .collect();
                phf[0] + 0.1
            }
        }
    };

    // Find insert position (sorted by time).
    let insert_idx = events.iter().filter(|(t, _)| *t <= hitframe_time).count();

    anns.insert(insert_idx, make_hitframe_annotation(hitframe_time));
    true
}

// ---------------------------------------------------------------------------
// Directory walker — only "attack*.hkx" files
// ---------------------------------------------------------------------------

fn walk_attack_hkx_files(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_attack_hkx_files(&path, f);
        } else {
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let lower = fname.to_lowercase();
            if lower.starts_with("attack") && lower.ends_with(".hkx") {
                f(&path);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_prehitframe_returns_false() {
        // No preHitFrame → no injection needed.
        let mut anns = vec![make_hitframe_annotation(0.5)];
        // Already has HitFrame, so should not inject again.
        assert!(!inject_hitframe_into(&mut anns));
    }

    #[test]
    fn injects_after_prehitframe_fallback() {
        // Only preHitFrame at t=0.3, no weaponSwing → HitFrame at 0.4.
        use havok_native::hkx::types::HkxValue;
        let prehit = HkxValue::Object(vec![
            HkxMember {
                name: "time".to_string(),
                value: HkxValue::F32(0.3),
            },
            HkxMember {
                name: "text".to_string(),
                value: HkxValue::String {
                    value: "preHitFrame".to_string(),
                    is_null: false,
                },
            },
        ]);
        let mut anns = vec![prehit];
        let injected = inject_hitframe_into(&mut anns);
        assert!(injected);
        assert_eq!(anns.len(), 2);
        // New HitFrame should be at ~0.4
        let events = collect_events(&anns);
        let hitframe = events.iter().find(|(_, t)| t == "HitFrame");
        assert!(hitframe.is_some());
        let (t, _) = hitframe.unwrap();
        assert!((*t - 0.4).abs() < 1e-5, "expected t≈0.4, got {t}");
    }

    #[test]
    fn prefers_weapon_sweep_attack_start_time() {
        use havok_native::hkx::types::HkxValue;
        let make_ann = |time: f32, text: &str| {
            HkxValue::Object(vec![
                HkxMember {
                    name: "time".to_string(),
                    value: HkxValue::F32(time),
                },
                HkxMember {
                    name: "text".to_string(),
                    value: HkxValue::String {
                        value: text.to_string(),
                        is_null: false,
                    },
                },
            ])
        };
        let mut anns = vec![
            make_ann(0.1, "preHitFrame"),
            make_ann(0.5, "weaponSwing"),
            make_ann(0.7, "WeaponSweepAttackStart"),
        ];
        assert!(inject_hitframe_into(&mut anns));
        let events = collect_events(&anns);
        let hf = events.iter().find(|(_, t)| t == "HitFrame").unwrap();
        assert!(
            (hf.0 - 0.7).abs() < 1e-5,
            "expected t≈0.7 (sweep time), got {}",
            hf.0
        );
    }

    #[test]
    fn no_mod_path_returns_empty() {
        use crate::fixups::FixupConfig;
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
        use crate::session::open_session;
        use crate::sym::StringInterner;

        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "InjectHitframeTest.esp",
            Some("fo4"),
        )
        .expect("test plugin handle");
        let config = FixupConfig::default();
        let mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let fixup = InjectHitframeEventsFixup;
        assert!(!fixup.applies_to_session(&session, &config));
        let report = fixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert!(report.is_no_op());
    }
}
