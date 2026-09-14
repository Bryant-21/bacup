//! FNV dialogue voice-manifest construction.
//!
//! FNV voices are OGG files while FO4 dialogue uses FUZ containers containing
//! an XWM stream and regenerated LIP data.  This module deliberately emits a
//! manifest rather than writing a fake FUZ header around OGG bytes.  Python
//! consumes the manifest through `creation_lib.audio.release`.

use serde::{Deserialize, Serialize};

pub const FNV_VOICE_MANIFEST_VERSION: u32 = 1;

/// One response that needs the authoritative WAV -> LIP/XWM -> FUZ pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FnvVoiceManifestEntry {
    /// The source plugin that owns this INFO response before merge flattening.
    pub origin_plugin: String,
    /// Stable key for the extracted data root containing `origin_plugin` voices.
    pub source_root: String,
    pub source_voice_type: String,
    pub source_candidates: Vec<String>,
    pub target_plugin: String,
    /// FO4 VTYP EDID.  This is never inferred by the manifest consumer.
    pub target_voice_type_edid: String,
    pub info_form_id: String,
    /// Zero-based response index; FO4 filenames are one-based.
    pub response_index: u32,
    pub target_path: String,
    pub transcript: Option<String>,
}

/// JSON payload consumed by `bacup_lib.fnv_voice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FnvVoiceManifest {
    pub version: u32,
    pub entries: Vec<FnvVoiceManifestEntry>,
}

impl Default for FnvVoiceManifest {
    fn default() -> Self {
        Self {
            version: FNV_VOICE_MANIFEST_VERSION,
            entries: Vec::new(),
        }
    }
}

impl FnvVoiceManifestEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source_plugin: impl Into<String>,
        source_voice_type: impl Into<String>,
        target_plugin: impl Into<String>,
        target_voice_type_edid: impl Into<String>,
        info_form_id: impl AsRef<str>,
        response_index: u32,
        transcript: Option<String>,
    ) -> Self {
        let source_plugin = source_plugin.into();
        Self::new_with_provenance(
            source_plugin.clone(),
            source_plugin,
            source_voice_type,
            target_plugin,
            target_voice_type_edid,
            info_form_id,
            response_index,
            transcript,
        )
    }

    /// Construct an entry with merge provenance.  Production merged-source
    /// callers must use this constructor rather than the compatibility
    /// `new`, whose root key equals its source plugin.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_provenance(
        origin_plugin: impl Into<String>,
        source_root: impl Into<String>,
        source_voice_type: impl Into<String>,
        target_plugin: impl Into<String>,
        target_voice_type_edid: impl Into<String>,
        info_form_id: impl AsRef<str>,
        response_index: u32,
        transcript: Option<String>,
    ) -> Self {
        let origin_plugin = origin_plugin.into();
        let source_root = source_root.into();
        let source_voice_type = source_voice_type.into();
        let target_plugin = target_plugin.into();
        let target_voice_type_edid = target_voice_type_edid.into();
        let info_form_id = normalize_info_form_id(info_form_id.as_ref());
        let source_candidates = fnv_voice_source_candidates(
            &origin_plugin,
            &source_voice_type,
            &info_form_id,
            response_index,
        );
        let target_path = fnv_to_fo4_response_voice_path(
            &target_plugin,
            &target_voice_type_edid,
            &info_form_id,
            response_index,
        );
        Self {
            origin_plugin,
            source_root,
            source_voice_type,
            source_candidates,
            target_plugin,
            target_voice_type_edid,
            info_form_id,
            response_index,
            target_path,
            transcript,
        }
    }
}

/// Build the FO4 target voice path for an INFO response.
///
/// The target plugin and FO4 voice-type EDID are explicit because they are
/// record mappings, not defaults.  Response filenames use a one-based suffix.
pub fn fnv_to_fo4_response_voice_path(
    target_plugin: &str,
    target_voice_type_edid: &str,
    info_form_id: &str,
    response_index: u32,
) -> String {
    format!(
        "Sound/Voice/{target_plugin}/{target_voice_type_edid}/{}_{}.fuz",
        normalize_info_form_id(info_form_id),
        response_index + 1,
    )
}

/// Legacy convenience for callers that have not yet supplied a response
/// index.  New conversion code should use `fnv_to_fo4_response_voice_path`.
pub fn fnv_to_fo4_voice_path(
    mod_prefix: &str,
    source_plugin: &str,
    voice_type: &str,
    form_id: &str,
) -> String {
    let target_plugin = format!("{mod_prefix}_nv_{source_plugin}");
    fnv_to_fo4_response_voice_path(&target_plugin, voice_type, form_id, 0)
}

/// Possible FNV source paths for one INFO response.
///
/// Most FNV responses use the one-based response suffix.  Some legacy assets
/// omit `_1` for their first response, so response zero carries that exact
/// fallback rather than guessing another response's audio.
pub fn fnv_voice_source_candidates(
    source_plugin: &str,
    source_voice_type: &str,
    info_form_id: &str,
    response_index: u32,
) -> Vec<String> {
    let prefix = format!(
        "Sound/Voice/{source_plugin}/{source_voice_type}/{}",
        normalize_info_form_id(info_form_id)
    );
    let mut candidates = vec![format!("{prefix}_{}.ogg", response_index + 1)];
    if response_index == 0 {
        candidates.push(format!("{prefix}.ogg"));
    }
    candidates
}

fn normalize_info_form_id(info_form_id: &str) -> String {
    let trimmed = info_form_id
        .trim()
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    u32::from_str_radix(trimmed, 16)
        .map(|value| format!("{value:08X}"))
        .unwrap_or_else(|_| trimmed.to_uppercase())
}

/// Legacy source-path helper for a first INFO response.
pub fn fnv_voice_source_path(source_plugin: &str, voice_type: &str, form_id: &str) -> String {
    fnv_voice_source_candidates(source_plugin, voice_type, form_id, 0)
        .into_iter()
        .next()
        .expect("FNV voice candidates always contains the numbered path")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fo4_response_path_uses_target_plugin_voice_edid_and_response_identity() {
        assert_eq!(
            fnv_to_fo4_response_voice_path(
                "B21_nv_FalloutNV.esm",
                "MaleAdult01DefaultB",
                "130161",
                0,
            ),
            "Sound/Voice/B21_nv_FalloutNV.esm/MaleAdult01DefaultB/00130161_1.fuz"
        );
        assert!(
            fnv_to_fo4_response_voice_path("Converted.esm", "Voice", "1", 0)
                .ends_with("/00000001_1.fuz")
        );
        assert_eq!(
            fnv_to_fo4_response_voice_path(
                "B21_nv_FalloutNV.esm",
                "MaleAdult01DefaultB",
                "134b9b",
                1,
            ),
            "Sound/Voice/B21_nv_FalloutNV.esm/MaleAdult01DefaultB/00134B9B_2.fuz"
        );
    }

    #[test]
    fn source_candidates_do_not_cross_response_boundaries() {
        assert_eq!(
            fnv_voice_source_candidates("FalloutNV.esm", "MaleAdult", "134B9B", 1),
            vec!["Sound/Voice/FalloutNV.esm/MaleAdult/00134B9B_2.ogg"]
        );
        assert_eq!(
            fnv_voice_source_candidates("FalloutNV.esm", "MaleAdult", "130161", 0),
            vec![
                "Sound/Voice/FalloutNV.esm/MaleAdult/00130161_1.ogg",
                "Sound/Voice/FalloutNV.esm/MaleAdult/00130161.ogg",
            ]
        );
    }

    #[test]
    fn golden_entries_require_mapped_fo4_voice_type() {
        let first = FnvVoiceManifestEntry::new_with_provenance(
            "FalloutNV.esm",
            "fnv-base",
            "MaleAdult",
            "B21_nv_FalloutNV.esm",
            "MaleAdult01DefaultB",
            "130161",
            0,
            Some("Hello.".into()),
        );
        let second = FnvVoiceManifestEntry::new_with_provenance(
            "Fallout3.esm",
            "fo3-base",
            "MaleAdult",
            "B21_nv_FalloutNV.esm",
            "MaleAdult01DefaultB",
            "134B9B",
            1,
            Some("Goodbye.".into()),
        );
        let manifest = FnvVoiceManifest {
            version: FNV_VOICE_MANIFEST_VERSION,
            entries: vec![first, second],
        };
        let json = serde_json::to_value(&manifest).expect("manifest serializes");
        let entries = json["entries"].as_array().expect("entries");
        assert_eq!(entries[0]["target_voice_type_edid"], "MaleAdult01DefaultB");
        assert_eq!(entries[0]["origin_plugin"], "FalloutNV.esm");
        assert_eq!(entries[0]["source_root"], "fnv-base");
        assert_eq!(
            entries[0]["target_path"],
            "Sound/Voice/B21_nv_FalloutNV.esm/MaleAdult01DefaultB/00130161_1.fuz"
        );
        assert_eq!(
            entries[1]["target_path"],
            "Sound/Voice/B21_nv_FalloutNV.esm/MaleAdult01DefaultB/00134B9B_2.fuz"
        );
        assert_ne!(entries[0]["target_voice_type_edid"], "MaleEvenToned");
    }
}
