//! Collect unique animation names from hkbClipGenerator objects.
//!
//! Returns the `animationName` of every `hkbClipGenerator` in a behavior
//! directory's `.hkx` files (e.g. `"Animations\Idle.hkt"`): the clips the graph
//! references, and the only ones that belong in `character.hkx`'s
//! `animationBundleNameData.assetNames`. `InjectAnimationNamesFixup` prefers
//! these over a disk scan.

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
