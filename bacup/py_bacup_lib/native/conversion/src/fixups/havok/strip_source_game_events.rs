//! Strip source-game-only annotation events from converted .hkx files.
//!

//!
//! # What this does
//! Walks every `.hkx` file under `ctx.mod_path/meshes/` and strips or renames
//! annotation events that are FO76-only using a built-in allowlist.  Only
//! `hkaSplineCompressedAnimation` and `hkaInterleavedUncompressedAnimation`
//! objects are touched — their `annotationTracks[*].annotations` arrays are
//! filtered.  `hkbBehaviorGraphStringData.eventNames` is intentionally left
//! untouched because those entries are referenced by index.
//!
//! # Event mapping (simplified, embedded)
//! Rather than loading the EventMapper YAML at runtime (which would require
//! an additional file dependency), we embed the FO76→FO4 event-name rules:
//! - Events whose names begin with `FO76_` are dropped.
//! - Events in `FO76_DROP_EVENTS` are dropped.
//! - All others pass through unchanged.
//!
//! This is a conservative port — the full EventMapper YAML path is a TODO for
//! when the runtime data-loading story is settled (see TODO(C.4.1) below).
//!
//! # FixupReport mapping
//! `records_changed` = number of .hkx files that had at least one annotation
//! dropped or renamed.

use std::path::{Path, PathBuf};

use havok_native::hkx::read_packfile;
use havok_native::hkx::types::HkxValue;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::{AssetPhaseFlags, FixupScope};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Known FO76-only event names (conservative set — drop on conversion)
// ---------------------------------------------------------------------------

/// FO76-only annotation event names that have no FO4 equivalent and should be
/// dropped from converted animation tracks.
///
/// TODO(C.4.1): replace with full EventMapper YAML load once a runtime data
/// path is available in FixupContext.
const FO76_DROP_EVENTS: &[&str] = &[
    "FO76_WeaponAttackStart",
    "FO76_WeaponAttackStop",
    "FO76_MeleeAttackStart",
    "FO76_MeleeAttackStop",
    "FO76_SprintStart",
    "FO76_SprintStop",
    "FO76_VaultStart",
    "FO76_VaultEnd",
    "FO76_CrouchSprintStart",
    "FO76_CrouchSprintStop",
];

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct StripSourceGameEventsFixup;

impl Fixup for StripSourceGameEventsFixup {
    fn name(&self) -> &'static str {
        "strip_source_game_events"
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
        strip_source_game_events_in_mod_path(mod_path)
    }
}

// ---------------------------------------------------------------------------
// Mod-path entry point (used by postprocess wave)
// ---------------------------------------------------------------------------

pub fn strip_source_game_events_in_mod_path(mod_path: &Path) -> Result<FixupReport, FixupError> {
    let mut files_changed = 0u32;
    for meshes_root in mesh_roots_for_mod_path(mod_path) {
        walk_hkx_files(
            &meshes_root,
            &mut |hkx_path| match process_strip_events(hkx_path) {
                Ok(true) => files_changed += 1,
                Ok(false) | Err(_) => {}
            },
        );
    }
    Ok(FixupReport {
        records_changed: files_changed,
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

/// Returns `true` if the file was modified (had events dropped/renamed).
fn process_strip_events(hkx_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let data = std::fs::read(hkx_path)?;
    let mut hkx = read_packfile(&data)?;

    let mut changed = false;

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
                for track in tracks.iter_mut() {
                    if let HkxValue::Object(track_members) = track {
                        for tm in track_members.iter_mut() {
                            if tm.name != "annotations" {
                                continue;
                            }
                            if let HkxValue::Array(anns) = &mut tm.value {
                                let before = anns.len();
                                anns.retain(|ann| !should_drop_annotation(ann));
                                if anns.len() != before {
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if changed {
        let out = hkx.save();
        std::fs::write(hkx_path, out)?;
    }

    Ok(changed)
}

/// Returns `true` if the annotation object should be dropped.
fn should_drop_annotation(ann: &HkxValue) -> bool {
    let members = match ann.as_object_members() {
        Some(m) => m,
        None => return false,
    };

    for m in members {
        if m.name != "text" {
            continue;
        }
        if let HkxValue::String { value, .. } = &m.value {
            let lower = value.to_lowercase();
            // Drop events starting with "fo76_"
            if lower.starts_with("fo76_") {
                return true;
            }
            // Drop events in the static list
            for drop in FO76_DROP_EVENTS {
                if value.eq_ignore_ascii_case(drop) {
                    return true;
                }
            }
        }
        break;
    }

    false
}

// ---------------------------------------------------------------------------
// Directory walker
// ---------------------------------------------------------------------------

/// Recursively walk `dir` and call `f` for every `.hkx` file found.
fn walk_hkx_files(dir: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_hkx_files(&path, f);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("hkx"))
            .unwrap_or(false)
        {
            f(&path);
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
    fn no_mod_path_returns_empty() {
        use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
        use crate::session::open_session;
        use crate::sym::StringInterner;

        let target_handle = esp_authoring_core::plugin_runtime::plugin_handle_new_native(
            "StripEventsTest.esp",
            Some("fo4"),
        )
        .expect("test plugin handle");
        let config = FixupConfig::default();
        let mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");

        let fixup = StripSourceGameEventsFixup;
        assert!(!fixup.applies_to_session(&session, &config));
        let report = fixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert!(report.is_no_op());
    }

    #[test]
    fn should_drop_fo76_prefixed_events() {
        // Build a minimal annotation Value with text "FO76_WeaponAttackStart"
        use havok_native::hkx::types::HkxValue;
        let ann = HkxValue::Object(vec![
            havok_native::hkx::model::HkxMember {
                name: "time".to_string(),
                value: HkxValue::F32(0.5),
            },
            havok_native::hkx::model::HkxMember {
                name: "text".to_string(),
                value: HkxValue::String {
                    value: "FO76_WeaponAttackStart".to_string(),
                    is_null: false,
                },
            },
        ]);
        assert!(should_drop_annotation(&ann));
    }

    #[test]
    fn should_keep_hitframe_event() {
        use havok_native::hkx::types::HkxValue;
        let ann = HkxValue::Object(vec![
            havok_native::hkx::model::HkxMember {
                name: "time".to_string(),
                value: HkxValue::F32(0.5),
            },
            havok_native::hkx::model::HkxMember {
                name: "text".to_string(),
                value: HkxValue::String {
                    value: "HitFrame".to_string(),
                    is_null: false,
                },
            },
        ]);
        assert!(!should_drop_annotation(&ann));
    }
}
