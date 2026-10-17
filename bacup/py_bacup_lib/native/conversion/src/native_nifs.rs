//! FO76/Starfield native assets, NIF side: meshes are converted, and native mode keeps their material
//! data and texture info for FO4CS. The two modes write different meshes, so a run that keeps
//! existing outputs reconverts them when the last run used the other mode.

use std::path::Path;

const MODE_MARKER: &str = "debug/nif_material_mode.txt";
const MIXED: &str = "mixed";

fn mode_name(native_mode: bool) -> &'static str {
    if native_mode { "fo76" } else { "converted" }
}

/// BSVER from a NIF header without parsing its blocks: the header line, version, endian byte,
/// user version and block count come first.
pub fn nif_stream_version(bytes: &[u8]) -> Option<u32> {
    if !bytes.starts_with(b"Gamebryo File Format") {
        return None;
    }
    let line_end = bytes.iter().position(|&byte| byte == b'\n')?;
    let at = line_end + 1 + 4 + 1 + 4 + 4;
    let field = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([field[0], field[1], field[2], field[3]]))
}

/// Without a marker the outputs predate native mode, so they are converted meshes.
pub fn existing_nifs_match_mode(mod_root: &Path, native_mode: bool) -> bool {
    match std::fs::read_to_string(mod_root.join(MODE_MARKER)) {
        Ok(recorded) => recorded.trim() == mode_name(native_mode),
        Err(_) => !native_mode,
    }
}

/// Called before NIFs are written: until the run finishes, the outputs may hold both modes.
pub fn mark_nif_mode_mixed(mod_root: &Path) -> std::io::Result<()> {
    write_marker(mod_root, MIXED)
}

pub fn record_nif_mode(mod_root: &Path, native_mode: bool) -> std::io::Result<()> {
    write_marker(mod_root, mode_name(native_mode))
}

fn write_marker(mod_root: &Path, value: &str) -> std::io::Result<()> {
    let path = mod_root.join(MODE_MARKER);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use nif_core_native::model::{NifFile, NifValue};

    fn nif_bytes(game: &str) -> Vec<u8> {
        let mut nif = NifFile::new(game);
        nif.blocks[0].set_field("Name", NifValue::String("Root".into()));
        nif.add_block("NiNode", None::<IndexMap<String, NifValue>>);
        nif.to_bytes().unwrap()
    }

    #[test]
    fn the_stream_version_is_read_from_the_header() {
        assert_eq!(nif_stream_version(&nif_bytes("fo76")), Some(155));
        assert_eq!(nif_stream_version(&nif_bytes("fo4")), Some(130));
        assert_eq!(nif_stream_version(b"garbage"), None);
    }

    #[test]
    fn existing_outputs_match_only_the_mode_that_finished_writing_them() {
        let dir = tempfile::tempdir().unwrap();
        assert!(existing_nifs_match_mode(dir.path(), false));
        assert!(!existing_nifs_match_mode(dir.path(), true));

        record_nif_mode(dir.path(), true).unwrap();
        assert!(existing_nifs_match_mode(dir.path(), true));
        assert!(!existing_nifs_match_mode(dir.path(), false));

        mark_nif_mode_mixed(dir.path()).unwrap();
        assert!(!existing_nifs_match_mode(dir.path(), true));
        assert!(!existing_nifs_match_mode(dir.path(), false));
    }
}
