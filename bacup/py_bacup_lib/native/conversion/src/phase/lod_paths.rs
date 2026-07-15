//! FO76→FO4 object-LOD path derivation — the SINGLE source of the convention
//! that maps a base record's full model (MODL) to its LOD mesh paths.
//!
//! FO76 does NOT store LOD model paths in the record; it ships LOD meshes by
//! folder convention under `meshes\[dlcNN\]lod\…`. FO4 instead carries the LOD
//! paths in the base record's `MNAM` "Distant LOD" subrecord (4 × 260-byte slots,
//! `Meshes\`-relative, rooted at `LOD\…`). This module derives both sides from a
//! single MODL so the synthesize-MNAM phase (`synthesize_object_lod`) and the
//! source-asset discovery (`walk`) agree byte-for-byte.
//!
//! Verified pairs (against real FO76 `_lod.nif` + vanilla Fallout4.esm MNAM):
//!   MODL `DLC03\Architecture\Barn\BarnCupolaMainRoof01.nif`
//!     → fo76 source `dlc03/lod/architecture/barn/barncupolamainroof01_lod.nif`
//!     → fo4 MNAM    `DLC03\LOD\Architecture\Barn\BarnCupolaMainRoof01_LOD.nif`
//!   MODL `Architecture\Airport\AirportTerminalDestroyed01.nif`
//!     → fo76 source `lod/architecture/airport/airportterminaldestroyed01_lod.nif`
//!     → fo4 MNAM    `LOD\Architecture\Airport\AirportTerminalDestroyed01_LOD.nif`

use crate::translator::Game;

/// One LOD slot candidate derived from a base MODL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LodCandidate {
    /// LOD level 0..=3 (Level0..Level3 = LOD4..LOD32 in FO4 terms).
    pub level: usize,
    /// `true` for the multi-level naming (`_lod_0.nif`..`_lod_3.nif`);
    /// `false` for the single-level naming (`_lod.nif`, always level 0).
    pub multi: bool,
    /// FO76 source mesh path, lowercase, `/`-separated, relative to the `meshes`
    /// root (no `meshes/` prefix). Existence is checked under the source dir.
    pub source_rel: String,
    /// FO4 `MNAM` slot string: `Meshes\`-relative (no `Meshes\` prefix),
    /// backslash-separated, rooted at `LOD\…`, original MODL casing preserved.
    pub mnam: String,
}

pub fn derive_lod_candidates(source_game: Game, modl: &str) -> Vec<LodCandidate> {
    match source_game {
        Game::SkyrimSe => derive_skyrim_lod_candidates(modl),
        _ => derive_fo76_lod_candidates(modl),
    }
}

/// Split a base MODL into `(optional dlc prefix, stem)` where the stem is the
/// `.nif`-less remainder after any `dlcNN\` prefix, in original casing with
/// backslash separators. Returns `None` for empty / non-`.nif` MODLs.
fn normalize_modl(modl: &str) -> Option<(Option<String>, String)> {
    let trimmed = modl.trim().trim_end_matches('\0').trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut m = trimmed.replace('/', "\\");
    while let Some(stripped) = m.strip_prefix('\\') {
        m = stripped.to_string();
    }
    if m.len() >= 7 && m[..7].eq_ignore_ascii_case("meshes\\") {
        m = m[7..].to_string();
    }
    if m.len() >= 5 && m[..5].eq_ignore_ascii_case("fo76\\") {
        m = m[5..].to_string();
    }
    if m.len() < 4 || !m[m.len() - 4..].eq_ignore_ascii_case(".nif") {
        return None;
    }
    let body = &m[..m.len() - 4];
    let lower = m.to_ascii_lowercase();
    let (prefix, stem) = if lower.starts_with("dlc03\\") || lower.starts_with("dlc04\\") {
        // `m[..5]` keeps the original casing of the 5-char "DLCnn" segment;
        // `body[6..]` skips "DLCnn\" (6 chars).
        (Some(m[..5].to_string()), body[6..].to_string())
    } else {
        (None, body.to_string())
    };
    if stem.is_empty() {
        return None;
    }
    Some((prefix, stem))
}

fn fo4_mnam_string_inner(prefix: &Option<String>, stem: &str, level: usize, multi: bool) -> String {
    let prefix_part = prefix
        .as_ref()
        .map(|p| format!("{p}\\"))
        .unwrap_or_default();
    let suffix = if multi {
        format!("_LOD_{level}.nif")
    } else {
        "_LOD.nif".to_string()
    };
    format!("{prefix_part}LOD\\{stem}{suffix}")
}

fn fo76_source_rel(prefix: &Option<String>, stem: &str, level: usize, multi: bool) -> String {
    let prefix_part = prefix
        .as_ref()
        .map(|p| format!("{}/", p.to_ascii_lowercase()))
        .unwrap_or_default();
    let stem_fs = stem.to_ascii_lowercase().replace('\\', "/");
    let suffix = if multi {
        format!("_lod_{level}.nif")
    } else {
        "_lod.nif".to_string()
    };
    format!("{prefix_part}lod/{stem_fs}{suffix}")
}

/// Derive all LOD slot candidates for a base MODL: the single-level form
/// (`_lod.nif` → level 0) plus the four multi-level forms (`_lod_0.nif`..
/// `_lod_3.nif` → levels 0..3). Callers existence-check `source_rel` and keep
/// the hits; in practice a mesh ships EITHER the single or the multi set.
pub fn derive_fo76_lod_candidates(modl: &str) -> Vec<LodCandidate> {
    let Some((prefix, stem)) = normalize_modl(modl) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(5);
    out.push(LodCandidate {
        level: 0,
        multi: false,
        source_rel: fo76_source_rel(&prefix, &stem, 0, false),
        mnam: fo4_mnam_string_inner(&prefix, &stem, 0, false),
    });
    for level in 0..4 {
        out.push(LodCandidate {
            level,
            multi: true,
            source_rel: fo76_source_rel(&prefix, &stem, level, true),
            mnam: fo4_mnam_string_inner(&prefix, &stem, level, true),
        });
    }
    out
}

/// Derive Skyrim's sibling LOD convention. Unlike FO76, Skyrim keeps
/// `foo_lod[_N].nif` beside `foo.nif` instead of inserting a `LOD` directory.
pub fn derive_skyrim_lod_candidates(modl: &str) -> Vec<LodCandidate> {
    let Some(stem) = normalize_skyrim_modl(modl) else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(5);
    out.push(skyrim_candidate(&stem, 0, false));
    for level in 0..4 {
        out.push(skyrim_candidate(&stem, level, true));
    }
    out
}

fn normalize_skyrim_modl(modl: &str) -> Option<String> {
    let mut path = modl
        .trim()
        .trim_end_matches('\0')
        .trim()
        .trim_start_matches(['\\', '/'])
        .replace('/', "\\");
    if path.len() >= 7 && path[..7].eq_ignore_ascii_case("meshes\\") {
        path = path[7..].to_string();
    }
    if path.len() < 4 || !path[path.len() - 4..].eq_ignore_ascii_case(".nif") {
        return None;
    }
    path.truncate(path.len() - 4);
    (!path.is_empty()).then_some(path)
}

fn skyrim_candidate(stem: &str, level: usize, multi: bool) -> LodCandidate {
    let suffix = if multi {
        format!("_LOD_{level}.nif")
    } else {
        "_LOD.nif".to_string()
    };
    let mnam = format!("{stem}{suffix}");
    LodCandidate {
        level,
        multi,
        source_rel: mnam.to_ascii_lowercase().replace('\\', "/"),
        mnam,
    }
}

/// Emit the FO4 `MNAM` slot string for one LOD level of a base MODL.
/// `multi=false` is the single-level (`_LOD.nif`) form; `multi=true` is the
/// per-level (`_LOD_{level}.nif`) form. Returns `None` for a non-`.nif` MODL.
pub fn fo4_mnam_string(modl: &str, level: usize, multi: bool) -> Option<String> {
    let (prefix, stem) = normalize_modl(modl)?;
    Some(fo4_mnam_string_inner(&prefix, &stem, level, multi))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single(c: &[LodCandidate]) -> &LodCandidate {
        c.iter().find(|c| !c.multi).expect("single candidate")
    }
    fn multi(c: &[LodCandidate], level: usize) -> &LodCandidate {
        c.iter()
            .find(|c| c.multi && c.level == level)
            .expect("multi candidate")
    }

    #[test]
    fn candidate_count_is_single_plus_four_multi() {
        let c = derive_fo76_lod_candidates("Architecture\\Foo\\Bar01.nif");
        assert_eq!(c.len(), 5);
        assert_eq!(c.iter().filter(|c| c.multi).count(), 4);
        assert_eq!(c.iter().filter(|c| !c.multi).count(), 1);
    }

    #[test]
    fn plain_path_inserts_lod_at_front() {
        let c = derive_fo76_lod_candidates("Architecture\\Airport\\AirportTerminalDestroyed01.nif");
        assert_eq!(
            single(&c).source_rel,
            "lod/architecture/airport/airportterminaldestroyed01_lod.nif"
        );
        assert_eq!(
            single(&c).mnam,
            "LOD\\Architecture\\Airport\\AirportTerminalDestroyed01_LOD.nif"
        );
    }

    #[test]
    fn dlc_prefixed_path_inserts_lod_after_dlc() {
        let c = derive_fo76_lod_candidates("DLC03\\Architecture\\Barn\\BarnCupolaMainRoof01.nif");
        assert_eq!(
            single(&c).source_rel,
            "dlc03/lod/architecture/barn/barncupolamainroof01_lod.nif"
        );
        assert_eq!(
            single(&c).mnam,
            "DLC03\\LOD\\Architecture\\Barn\\BarnCupolaMainRoof01_LOD.nif"
        );
    }

    #[test]
    fn multi_level_naming_uses_underscore_index() {
        let c = derive_fo76_lod_candidates("DLC04\\Foo\\Bar01.nif");
        assert_eq!(multi(&c, 0).source_rel, "dlc04/lod/foo/bar01_lod_0.nif");
        assert_eq!(multi(&c, 3).source_rel, "dlc04/lod/foo/bar01_lod_3.nif");
        assert_eq!(multi(&c, 0).mnam, "DLC04\\LOD\\Foo\\Bar01_LOD_0.nif");
        assert_eq!(multi(&c, 3).mnam, "DLC04\\LOD\\Foo\\Bar01_LOD_3.nif");
    }

    #[test]
    fn forward_slashes_and_meshes_prefix_are_normalized() {
        let c = derive_fo76_lod_candidates("Meshes/Architecture/Foo/Bar01.nif");
        assert_eq!(single(&c).source_rel, "lod/architecture/foo/bar01_lod.nif");
        assert_eq!(single(&c).mnam, "LOD\\Architecture\\Foo\\Bar01_LOD.nif");
    }

    #[test]
    fn converted_fo76_namespace_is_stripped_before_deriving_lod() {
        let c = derive_fo76_lod_candidates(
            "FO76\\Landscape\\Trees\\Chargen\\TreeMaplePreWar01Orange.nif",
        );
        assert_eq!(
            multi(&c, 3).source_rel,
            "lod/landscape/trees/chargen/treemapleprewar01orange_lod_3.nif"
        );
        assert_eq!(
            multi(&c, 3).mnam,
            "LOD\\Landscape\\Trees\\Chargen\\TreeMaplePreWar01Orange_LOD_3.nif"
        );
    }

    #[test]
    fn no_underscore_zero_form_is_not_produced() {
        // `_lod0` (no underscore) does NOT occur — only `_lod` and `_lod_N`.
        let c = derive_fo76_lod_candidates("Architecture\\Foo\\Bar01.nif");
        assert!(!c.iter().any(|c| c.source_rel.contains("_lod0")));
        assert!(!c.iter().any(|c| c.mnam.contains("_LOD0")));
    }

    #[test]
    fn skyrim_candidates_keep_the_full_sibling_namespace() {
        let candidates =
            derive_skyrim_lod_candidates("Meshes/Architecture/Farmhouse/Farmhouse01.NIF");
        assert_eq!(
            single(&candidates).source_rel,
            "architecture/farmhouse/farmhouse01_lod.nif"
        );
        assert_eq!(
            single(&candidates).mnam,
            "Architecture\\Farmhouse\\Farmhouse01_LOD.nif"
        );
        assert_eq!(
            multi(&candidates, 3).source_rel,
            "architecture/farmhouse/farmhouse01_lod_3.nif"
        );
    }

    #[test]
    fn fo4_mnam_string_matches_candidate() {
        assert_eq!(
            fo4_mnam_string(
                "DLC03\\Architecture\\Barn\\BarnCupolaMainRoof01.nif",
                0,
                false
            )
            .as_deref(),
            Some("DLC03\\LOD\\Architecture\\Barn\\BarnCupolaMainRoof01_LOD.nif")
        );
        assert_eq!(
            fo4_mnam_string("Buildings\\Church\\ChurchMainAAdd01.nif", 0, true).as_deref(),
            Some("LOD\\Buildings\\Church\\ChurchMainAAdd01_LOD_0.nif")
        );
    }

    #[test]
    fn non_nif_and_empty_yield_no_candidates() {
        assert!(derive_fo76_lod_candidates("").is_empty());
        assert!(derive_fo76_lod_candidates("Foo\\Bar.dds").is_empty());
        assert!(fo4_mnam_string("", 0, false).is_none());
        assert!(fo4_mnam_string("Foo\\Bar.dds", 0, false).is_none());
    }
}
