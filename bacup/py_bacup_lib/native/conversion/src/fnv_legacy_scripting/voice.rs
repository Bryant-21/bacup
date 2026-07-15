//! Voice path translation and .fuz packaging.
//!
//! Mirrors `fnv_legacy_scripting/voice.py`.

/// Build the FO4 target voice path for a translated INFO record.
///
/// Format: `Sound/Voice/<prefix>_nv_<source_plugin>/<voice_type>/<form_id_upper>_1.fuz`
pub fn fnv_to_fo4_voice_path(
    mod_prefix: &str,
    source_plugin: &str,
    voice_type: &str,
    form_id: &str,
) -> String {
    format!(
        "Sound/Voice/{mod_prefix}_nv_{source_plugin}/{voice_type}/{}_1.fuz",
        form_id.to_uppercase()
    )
}

/// Build the FNV source voice path for a given INFO record.
///
/// Format: `Sound/Voice/<source_plugin>/<voice_type>/<form_id_upper>.ogg`
pub fn fnv_voice_source_path(source_plugin: &str, voice_type: &str, form_id: &str) -> String {
    format!(
        "Sound/Voice/{source_plugin}/{voice_type}/{}.ogg",
        form_id.to_uppercase()
    )
}

/// Package raw OGG bytes (plus an optional LIP blob) into the FUZ format.
///
/// FUZ layout: `"FUZE"` magic + u32LE version(1) + u32LE lip_len + lip_bytes + ogg_bytes.
pub fn package_ogg_as_fuz(ogg_bytes: &[u8], lip_bytes: Option<&[u8]>) -> Vec<u8> {
    let lip = lip_bytes.unwrap_or(&[]);
    let mut out = Vec::with_capacity(12 + lip.len() + ogg_bytes.len());
    out.extend_from_slice(b"FUZE");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&(lip.len() as u32).to_le_bytes());
    out.extend_from_slice(lip);
    out.extend_from_slice(ogg_bytes);
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fo4_voice_path_format() {
        let path = fnv_to_fo4_voice_path("B21", "FNV.esm", "MaleEvenToned", "001234");
        assert_eq!(
            path,
            "Sound/Voice/B21_nv_FNV.esm/MaleEvenToned/001234_1.fuz"
        );
    }

    #[test]
    fn fo4_voice_path_uppercases_form_id() {
        let path = fnv_to_fo4_voice_path("B21", "FNV.esm", "MaleEvenToned", "abcdef");
        assert!(path.contains("ABCDEF_1.fuz"), "path: {path}");
    }

    #[test]
    fn fnv_source_path_format() {
        let path = fnv_voice_source_path("FNV.esm", "MaleEvenToned", "001234");
        assert_eq!(path, "Sound/Voice/FNV.esm/MaleEvenToned/001234.ogg");
    }

    #[test]
    fn fuz_magic_and_version() {
        let ogg = b"fake_ogg_data";
        let fuz = package_ogg_as_fuz(ogg, None);
        assert_eq!(&fuz[0..4], b"FUZE");
        // version = 1 little-endian
        assert_eq!(&fuz[4..8], &[1, 0, 0, 0]);
        // lip_len = 0 (no lip)
        assert_eq!(&fuz[8..12], &[0, 0, 0, 0]);
        // ogg data at end
        assert_eq!(&fuz[12..], ogg);
    }

    #[test]
    fn fuz_with_lip() {
        let ogg = b"ogg";
        let lip = b"lip_data";
        let fuz = package_ogg_as_fuz(ogg, Some(lip));
        assert_eq!(&fuz[0..4], b"FUZE");
        // version = 1
        assert_eq!(u32::from_le_bytes(fuz[4..8].try_into().unwrap()), 1);
        // lip_len
        assert_eq!(
            u32::from_le_bytes(fuz[8..12].try_into().unwrap()),
            lip.len() as u32
        );
        assert_eq!(&fuz[12..12 + lip.len()], lip);
        assert_eq!(&fuz[12 + lip.len()..], ogg);
    }
}
