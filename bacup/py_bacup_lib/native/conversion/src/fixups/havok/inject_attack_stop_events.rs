//! Inject missing attackStop annotation events into attack animations.
//!
//! FO4 melee attack states exit on the clip's `attackStop` annotation. Every
//! vanilla FO4 attack clip has one (26/26 SuperMutant h2h+2hm, 38/38 converted
//! Scorched, which descend from FO4 human clips), just before clip end
//! (duration − 0.07..0.17s, always after HitFrame). FO76-native creature clips
//! (e.g. MoleMiner H2H) have `preHitFrame`/`HitFrame`/`weaponSwing` but no exit
//! event, so the actor lands one attack and freezes in the pose.
//!
//! `attack*.hkx` files with `preHitFrame` or `HitFrame` but no `attackStop` get
//! one at `duration − 0.1667`, clamped after the last swing/hit event. Gun-bash
//! clips (`riflemelee*`) are excluded; vanilla ones have no `attackStop` either.
//! `records_changed` counts patched files.

use std::path::Path;

use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxMember, read_packfile};

use crate::fixups::havok::inject_hitframe_events::{
    collect_events, mesh_roots_for_mod_path, walk_attack_hkx_files,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

/// Vanilla placement: attackStop sits duration − 0.07..0.17 across the
/// SuperMutant/Scorched census; use the far edge so recovery frames still play.
const END_OFFSET: f32 = 0.1667;
/// attackStop must land after the swing/hit family or the hit never registers
/// before the state exits.
const MIN_GAP_AFTER_HIT: f32 = 0.05;

pub struct InjectAttackStopEventsFixup;

impl Fixup for InjectAttackStopEventsFixup {
    fn name(&self) -> &'static str {
        "inject_attack_stop_events"
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
        inject_attack_stop_events_in_mod_path(mod_path)
    }
}

// ---------------------------------------------------------------------------
// Mod-path entry point (used by postprocess wave)
// ---------------------------------------------------------------------------

pub fn inject_attack_stop_events_in_mod_path(mod_path: &Path) -> Result<FixupReport, FixupError> {
    let mut files_patched = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_attack_hkx_files(
            &meshes_root,
            &mut |hkx_path| match process_inject_attack_stop(hkx_path) {
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

// ---------------------------------------------------------------------------
// Core algorithm
// ---------------------------------------------------------------------------

/// Returns `true` if the file was modified.
fn process_inject_attack_stop(hkx_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(hkx_path)?;
    let mut hkx = read_packfile(&data)?;

    let mut modified = false;

    for obj in hkx.objects_mut() {
        if obj.class_name != "hkaSplineCompressedAnimation"
            && obj.class_name != "hkaInterleavedUncompressedAnimation"
        {
            continue;
        }

        let duration = obj.members.iter().find_map(|m| {
            if m.name == "duration" {
                if let HkxValue::F32(d) = m.value {
                    return Some(d);
                }
            }
            None
        });
        let Some(duration) = duration else {
            continue;
        };

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
                                if inject_attack_stop_into(anns, duration) {
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

fn make_attack_stop_annotation(time: f32) -> HkxValue {
    HkxValue::Object(vec![
        HkxMember {
            name: "time".to_string(),
            value: HkxValue::F32(time),
        },
        HkxMember {
            name: "text".to_string(),
            value: HkxValue::String {
                value: "attackStop".to_string(),
                is_null: false,
            },
        },
    ])
}

/// Try to inject an attackStop annotation into `anns`.  Returns `true` if injected.
fn inject_attack_stop_into(anns: &mut Vec<HkxValue>, duration: f32) -> bool {
    let events = collect_events(anns);

    let has_attack_stop = events
        .iter()
        .any(|(_, t)| t.eq_ignore_ascii_case("attackStop"));
    let is_attack_clip = events
        .iter()
        .any(|(_, t)| t.eq_ignore_ascii_case("HitFrame") || t.eq_ignore_ascii_case("preHitFrame"));

    if has_attack_stop || !is_attack_clip {
        return false;
    }

    let last_hit_event = events
        .iter()
        .filter(|(_, t)| {
            t.eq_ignore_ascii_case("HitFrame")
                || t.eq_ignore_ascii_case("preHitFrame")
                || t.eq_ignore_ascii_case("weaponSwing")
        })
        .map(|(t, _)| *t)
        .fold(0.0f32, f32::max);

    let cap = duration - 0.01;
    let stop_time = (duration - END_OFFSET)
        .max(last_hit_event + MIN_GAP_AFTER_HIT)
        .min(cap);

    let insert_idx = events.iter().filter(|(t, _)| *t <= stop_time).count();
    anns.insert(insert_idx, make_attack_stop_annotation(stop_time));
    true
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ann(time: f32, text: &str) -> HkxValue {
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
    }

    #[test]
    fn injects_before_clip_end() {
        // MoleMiner h2h_attackforwarda shape: HitFrame 0.97, duration 1.67.
        let mut anns = vec![
            make_ann(0.77, "weaponSwing"),
            make_ann(0.90, "preHitFrame"),
            make_ann(0.97, "HitFrame"),
        ];
        assert!(inject_attack_stop_into(&mut anns, 1.6667));
        let events = collect_events(&anns);
        let stop = events.iter().find(|(_, t)| t == "attackStop").unwrap();
        assert!(
            (stop.0 - 1.5).abs() < 1e-3,
            "expected t≈1.50 (duration − 0.1667), got {}",
            stop.0
        );
        // Must stay sorted by time.
        let times: Vec<f32> = events.iter().map(|(t, _)| *t).collect();
        let mut sorted = times.clone();
        sorted.sort_by(f32::total_cmp);
        assert_eq!(times, sorted);
    }

    #[test]
    fn skips_when_attack_stop_present() {
        let mut anns = vec![
            make_ann(0.5, "HitFrame"),
            make_ann(1.2, "AttackStop"), // case-insensitive match
        ];
        assert!(!inject_attack_stop_into(&mut anns, 1.6667));
        assert_eq!(anns.len(), 2);
    }

    #[test]
    fn skips_non_attack_clips() {
        // Locomotion-style annotations only — no hit family, no injection.
        let mut anns = vec![make_ann(0.2, "FootLeft"), make_ann(0.6, "FootRight")];
        assert!(!inject_attack_stop_into(&mut anns, 1.0));
        assert_eq!(anns.len(), 2);
    }

    #[test]
    fn clamps_after_late_hit_frame() {
        // HitFrame very close to clip end: stop must land after it, before end.
        let mut anns = vec![make_ann(1.55, "hitFrame")];
        assert!(inject_attack_stop_into(&mut anns, 1.6667));
        let events = collect_events(&anns);
        let stop = events.iter().find(|(_, t)| t == "attackStop").unwrap();
        assert!(
            stop.0 > 1.55 && stop.0 <= 1.6667 - 0.01 + 1e-4,
            "expected 1.55 < t ≤ 1.6567, got {}",
            stop.0
        );
    }

    #[test]
    #[ignore = "live validation: set ATTACK_STOP_LIVE_CLIP to a converted attack .hkx path"]
    fn live_clip_round_trip() {
        let Some(src) = std::env::var_os("ATTACK_STOP_LIVE_CLIP") else {
            return;
        };
        let tmp = tempfile::tempdir().expect("tempdir");
        let clip = tmp.path().join("attackforwarda.hkx");
        std::fs::copy(&src, &clip).expect("copy live clip");

        let modified = process_inject_attack_stop(&clip).expect("process clip");
        assert!(modified, "expected live clip to gain attackStop");

        let data = std::fs::read(&clip).expect("re-read clip");
        let mut hkx = read_packfile(&data).expect("re-parse clip");
        let mut found = false;
        for obj in hkx.objects_mut() {
            for member in &obj.members {
                if member.name != "annotationTracks" {
                    continue;
                }
                if let HkxValue::Array(tracks) = &member.value {
                    if let Some(HkxValue::Object(track_members)) = tracks.first() {
                        for tm in track_members {
                            if tm.name != "annotations" {
                                continue;
                            }
                            if let HkxValue::Array(anns) = &tm.value {
                                found |= collect_events(anns)
                                    .iter()
                                    .any(|(_, t)| t.eq_ignore_ascii_case("attackStop"));
                            }
                        }
                    }
                }
            }
        }
        assert!(found, "attackStop missing after round-trip");
    }

    #[test]
    fn no_mod_path_returns_empty() {
        use crate::fixups::FixupConfig;
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
        use crate::session::open_session;
        use crate::sym::StringInterner;

        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "InjectAttackStopTest.esp",
            Some("fo4"),
        )
        .expect("test plugin handle");
        let config = FixupConfig::default();
        let mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let fixup = InjectAttackStopEventsFixup;
        assert!(!fixup.applies_to_session(&session, &config));
        let report = fixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert!(report.is_no_op());
    }
}
