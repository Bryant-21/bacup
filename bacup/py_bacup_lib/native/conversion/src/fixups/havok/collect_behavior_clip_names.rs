//! Collect unique animation names from hkbClipGenerator objects.
//!

//!
//! # What this does
//! Parses all `.hkx` files in a behavior directory, finds `hkbClipGenerator`
//! objects, and returns their `animationName` values (e.g.
//! `"Animations\Idle.hkt"`).  These are the animations the behavior graph
//! actually references — only these should appear in
//! `character.hkx`'s `animationBundleNameData.assetNames`.
//!
//! # Usage in this crate
//! This module exposes `collect_behavior_clip_names` as a free function
//! (not a `Fixup` trait implementor) because it is a pure analysis function
//! that returns a data structure rather than mutating records or files.
//!
//! It is used internally by `InjectAnimationNamesFixup` (C.4.4) to prefer
//! behavior-referenced clips over disk-scanning.
//!

use std::collections::HashSet;
use std::path::Path;

// Re-export the inner implementation from inject_animation_names so there is
// only one copy of the logic.
pub use super::inject_animation_names::collect_behavior_clip_names_from_dir as collect_behavior_clip_names;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dir_returns_empty_set() {
        let dir = tempfile::tempdir().unwrap();
        let names = collect_behavior_clip_names(dir.path());
        assert!(names.is_empty());
    }

    #[test]
    fn nonexistent_dir_returns_empty_set() {
        let names = collect_behavior_clip_names(Path::new("nonexistent_path_that_does_not_exist"));
        assert!(names.is_empty());
    }
}
