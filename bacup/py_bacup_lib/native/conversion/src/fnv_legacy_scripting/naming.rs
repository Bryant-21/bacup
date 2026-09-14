//! Papyrus script naming conventions for FNV-imported scripts.
//!
//! Mirrors `fnv_legacy_scripting/naming.py`.

/// Standalone (SCPT) script class name: `<prefix>_S_<source-local>`.
pub fn standalone_script_name(mod_prefix: &str, source_local: u32) -> String {
    format!("{mod_prefix}_S_{source_local:06X}")
}

/// Quest fragment class name: `QF_<prefix>_<source-local>`.
pub fn quest_fragment_name(mod_prefix: &str, source_local: u32) -> String {
    format!("QF_{mod_prefix}_{source_local:06X}")
}

/// Topic-info fragment class name: `TIF__<form_id>`.
pub fn topic_info_fragment_name(form_id: &str) -> String {
    format!("TIF__{form_id}")
}

/// Scene action fragment class name: `SF_<editor_id>_<form_id>`.
pub fn scene_action_fragment_name(scene_editor_id: &str, form_id: &str) -> String {
    format!("SF_{scene_editor_id}_{form_id}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_name() {
        assert_eq!(standalone_script_name("B21", 0x001234), "B21_S_001234");
    }

    #[test]
    fn quest_fragment() {
        assert_eq!(quest_fragment_name("B21", 0x001234), "QF_B21_001234");
    }

    #[test]
    fn exact_quest_slice_classes_are_short_and_case_insensitively_unique() {
        let classes = [
            standalone_script_name("FNV_FO3", 0x11FC64),
            standalone_script_name("FNV_FO3", 0x123191),
            standalone_script_name("FNV_FO3", 0x134491),
            standalone_script_name("FNV_FO3", 0x166305),
            quest_fragment_name("FNV_FO3", 0x06136D),
            quest_fragment_name("FNV_FO3", 0x11F935),
            topic_info_fragment_name("130161"),
            topic_info_fragment_name("134B9B"),
            "FNV_FO3_FnvSliceCompat".to_string(),
        ];
        assert!(classes.iter().all(|class_name| class_name.len() <= 38));
        let folded = classes
            .iter()
            .map(|class_name| class_name.to_ascii_lowercase())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(folded.len(), classes.len());
    }

    #[test]
    fn topic_info_fragment() {
        assert_eq!(topic_info_fragment_name("ABCDEF"), "TIF__ABCDEF");
    }

    #[test]
    fn scene_action_fragment() {
        assert_eq!(
            scene_action_fragment_name("MyScene", "001234"),
            "SF_MyScene_001234"
        );
    }
}
