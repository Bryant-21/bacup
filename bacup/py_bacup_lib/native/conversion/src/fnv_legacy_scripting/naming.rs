//! Papyrus script naming conventions for FNV-imported scripts.
//!
//! Mirrors `fnv_legacy_scripting/naming.py`.

/// Standalone (SCPT) script class name: `<prefix>_nv_<editor_id>`.
pub fn standalone_script_name(mod_prefix: &str, source_editor_id: &str) -> String {
    format!("{mod_prefix}_nv_{source_editor_id}")
}

/// Quest fragment class name: `QF_<prefix>_nv_<editor_id>_<form_id>`.
pub fn quest_fragment_name(mod_prefix: &str, quest_editor_id: &str, form_id: &str) -> String {
    format!("QF_{mod_prefix}_nv_{quest_editor_id}_{form_id}")
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
        assert_eq!(standalone_script_name("B21", "MyScript"), "B21_nv_MyScript");
    }

    #[test]
    fn quest_fragment() {
        assert_eq!(
            quest_fragment_name("B21", "MyQuest", "001234"),
            "QF_B21_nv_MyQuest_001234"
        );
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
