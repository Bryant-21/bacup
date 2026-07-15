//! Derive PowerArmor subgraph blocks from parsed Human additive blocks.
//!

//! # What this does
//! Substitutes Character animation paths with their PowerArmor equivalents:
//! - `Actors\Character\animations\weapon\<name>`
//!   → `Actors\powerarmor\animations\Weapons\<name>`
//! - Heavy weapon roots like `Weapon\M2` / `Weapon\GripHeavy`
//!   → FO4 PA heavy roots (`Weapons\M2`, `Grips\Minigun`)
//! - `Actors\Character\_1stPerson\animations\<name>`
//!   → `Actors\powerarmor\_1stperson\animations\<name>`
//!
//! Only blocks that contain at least one PA-relevant path (i.e., a Character
//! weapon-specific or first-person path that maps to a PA path) are included
//! in the output. Generic shared paths (e.g. `Actors\Character\Animations\Paired`)
//! alone don't warrant a PA subgraph entry.
//!
//! Path matching is case-insensitive on the prefix segments (Python regex
//! uses `(?i)` and accepts both `/` and `\` as separators); the rewritten
//! output always uses backslashes to match the FO4 file-system convention.

use crate::fixups::face::build_additive_race_record::SubgraphBlock;
use crate::sym::{StringInterner, Sym};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Derive PowerArmor subgraph blocks from Human additive blocks by
/// substituting Character animation paths with PA equivalents.
///
/// Only blocks containing at least one PA-relevant path are emitted.
pub fn derive_pa_subgraph_blocks(
    human_blocks: &[SubgraphBlock],
    interner: &StringInterner,
) -> Vec<SubgraphBlock> {
    let mut out: Vec<SubgraphBlock> = Vec::new();
    for block in human_blocks {
        let mut new_paths: Vec<Sym> = Vec::with_capacity(block.paths.len());
        let mut has_specific = false;
        let heavy_weapon_block = block
            .paths
            .iter()
            .any(|s| interner.resolve(*s).is_some_and(is_heavy_weapon_path));
        for s in &block.paths {
            let resolved = match interner.resolve(*s) {
                Some(v) => v.to_string(),
                None => continue,
            };
            if let Some(rest) = strip_character_weapon_prefix(&resolved) {
                let pa = if heavy_weapon_block {
                    heavy_weapon_pa_path(&rest).unwrap_or_else(|| {
                        format!("Actors\\PowerArmor\\Animations\\Weapons\\{rest}")
                    })
                } else {
                    format!("Actors\\powerarmor\\animations\\Weapons\\{rest}")
                };
                push_interned_path(&mut new_paths, &pa, interner);
                has_specific = true;
            } else if let Some(rest) = strip_character_1stperson_prefix(&resolved) {
                let pa = format!("Actors\\powerarmor\\_1stperson\\animations\\{rest}");
                new_paths.push(interner.intern(&pa));
                has_specific = true;
            } else if heavy_weapon_block
                && path_eq_ci(&resolved, "Actors\\Character\\Animations\\Paired")
            {
                push_interned_path(
                    &mut new_paths,
                    "Actors\\PowerArmor\\Animations\\Paired",
                    interner,
                );
            } else {
                new_paths.push(*s);
            }
        }
        if !has_specific {
            continue;
        }
        let mut new_block = block.clone();
        new_block.behaviour_graph =
            rewrite_pa_behaviour_graph(block.behaviour_graph, interner, heavy_weapon_block);
        new_block.paths = new_paths;
        if heavy_weapon_block {
            push_interned_path(
                &mut new_block.paths,
                "Actors\\PowerArmor\\Animations\\Paired",
                interner,
            );
        }
        out.push(new_block);
    }
    out
}

// ---------------------------------------------------------------------------
// Private helpers — segment stripping
// ---------------------------------------------------------------------------

/// Strip an `Actors[/\]Character[/\]animations[/\]weapon[/\]` (case-insensitive)
/// prefix, returning the remainder.
fn strip_character_weapon_prefix(s: &str) -> Option<String> {
    let r1 = strip_one_segment_ci(s, "Actors")?;
    let r2 = strip_one_segment_ci(&r1, "Character")?;
    let r3 = strip_one_segment_ci(&r2, "animations")?;
    strip_one_segment_ci(&r3, "weapon")
}

/// Strip an `Actors[/\]Character[/\]_1stPerson[/\]animations[/\]`
/// (case-insensitive) prefix, returning the remainder.
fn strip_character_1stperson_prefix(s: &str) -> Option<String> {
    let r1 = strip_one_segment_ci(s, "Actors")?;
    let r2 = strip_one_segment_ci(&r1, "Character")?;
    let r3 = strip_one_segment_ci(&r2, "_1stPerson")?;
    strip_one_segment_ci(&r3, "animations")
}

/// Strip one case-insensitive segment followed by `/` or `\`. Returns the
/// remainder (excluding the separator) when matched.
fn strip_one_segment_ci(s: &str, seg: &str) -> Option<String> {
    if s.len() <= seg.len() {
        return None;
    }
    let head = &s[..seg.len()];
    if !head.eq_ignore_ascii_case(seg) {
        return None;
    }
    let sep = s.as_bytes()[seg.len()];
    if sep != b'/' && sep != b'\\' {
        return None;
    }
    Some(s[seg.len() + 1..].to_string())
}

fn rewrite_pa_behaviour_graph(
    graph: Sym,
    interner: &StringInterner,
    heavy_weapon_block: bool,
) -> Sym {
    let Some(path) = interner.resolve(graph) else {
        return graph;
    };
    if heavy_weapon_block
        && (path.eq_ignore_ascii_case("Actors\\Character\\Behaviors\\WeaponBehavior.hkx")
            || path
                .eq_ignore_ascii_case("Actors\\Character\\Behaviors\\BigGunWrappingBehavior.hkx"))
    {
        return interner
            .intern("Actors\\Character\\Behaviors\\PowerArmorHeavyWeaponWrappingBehavior.hkx");
    }
    if path.eq_ignore_ascii_case(
        "Actors\\Character\\Behaviors\\NoHandIKRelaxedWeaponWrappingBehavior.hkx",
    ) {
        return interner.intern("Actors\\Character\\Behaviors\\NoHandIKWeaponWrappingBehavior.hkx");
    }
    if path.eq_ignore_ascii_case(
        "Actors\\Character\\_1stPerson\\Behaviors\\Pistol_GunWrappingBehavior.hkx",
    ) {
        return interner.intern("Actors\\Character\\_1stPerson\\Behaviors\\GunBehavior.hkx");
    }
    graph
}

fn is_heavy_weapon_path(path: &str) -> bool {
    strip_character_weapon_prefix(path).is_some_and(|rest| {
        rest_eq_or_under_ci(&rest, "M2") || rest_eq_or_under_ci(&rest, "GripHeavy")
    })
}

fn heavy_weapon_pa_path(rest: &str) -> Option<String> {
    if rest_eq_or_under_ci(rest, "M2") {
        return Some("Actors\\PowerArmor\\Animations\\Weapons\\M2".to_string());
    }
    if rest_eq_or_under_ci(rest, "GripHeavy") {
        return Some("Actors\\PowerArmor\\Animations\\Grips\\Minigun".to_string());
    }
    None
}

fn rest_eq_or_under_ci(rest: &str, prefix: &str) -> bool {
    let normalized = rest.replace('/', "\\");
    normalized.eq_ignore_ascii_case(prefix)
        || (normalized.len() > prefix.len()
            && normalized[..prefix.len()].eq_ignore_ascii_case(prefix)
            && normalized.as_bytes()[prefix.len()] == b'\\')
}

fn path_eq_ci(path: &str, target: &str) -> bool {
    path.replace('/', "\\").eq_ignore_ascii_case(target)
}

fn push_interned_path(paths: &mut Vec<Sym>, path: &str, interner: &StringInterner) {
    let already_present = paths.iter().any(|s| {
        interner
            .resolve(*s)
            .is_some_and(|existing| path_eq_ci(existing, path))
    });
    if !already_present {
        paths.push(interner.intern(path));
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    fn sym(s: &str, interner: &StringInterner) -> Sym {
        interner.intern(s)
    }

    /// character-weapon path is rewritten to PA path.
    #[test]
    fn rewrites_character_weapon() {
        let mut interner = StringInterner::new();
        let block = SubgraphBlock {
            behaviour_graph: sym(
                "Actors\\Character\\Behaviors\\NoHandIKRelaxedWeaponWrappingBehavior.hkx",
                &mut interner,
            ),
            paths: vec![sym(
                "Actors\\Character\\animations\\weapon\\MyGun",
                &mut interner,
            )],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };
        let out = derive_pa_subgraph_blocks(&[block], &mut interner);
        assert_eq!(out.len(), 1);
        let p = interner.resolve(out[0].paths[0]).unwrap();
        assert_eq!(p, "Actors\\powerarmor\\animations\\Weapons\\MyGun");
        assert_eq!(
            interner.resolve(out[0].behaviour_graph).unwrap(),
            "Actors\\Character\\Behaviors\\NoHandIKWeaponWrappingBehavior.hkx"
        );
    }

    /// character-1stperson path is rewritten.
    #[test]
    fn rewrites_first_person() {
        let mut interner = StringInterner::new();
        let block = SubgraphBlock {
            behaviour_graph: sym(
                "Actors\\Character\\_1stPerson\\Behaviors\\Pistol_GunWrappingBehavior.hkx",
                &mut interner,
            ),
            paths: vec![sym(
                "Actors\\Character\\_1stPerson\\animations\\MyAnim",
                &mut interner,
            )],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };
        let out = derive_pa_subgraph_blocks(&[block], &mut interner);
        assert_eq!(out.len(), 1);
        let p = interner.resolve(out[0].paths[0]).unwrap();
        assert_eq!(p, "Actors\\powerarmor\\_1stperson\\animations\\MyAnim");
        assert_eq!(
            interner.resolve(out[0].behaviour_graph).unwrap(),
            "Actors\\Character\\_1stPerson\\Behaviors\\GunBehavior.hkx"
        );
    }

    /// blocks with no PA-specific path are dropped.
    #[test]
    fn drops_blocks_without_specific_paths() {
        let mut interner = StringInterner::new();
        let block = SubgraphBlock {
            behaviour_graph: sym("Actors\\Character\\Foo.hkx", &mut interner),
            paths: vec![sym(
                "Actors\\Character\\Animations\\Paired\\Kill01",
                &mut interner,
            )],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };
        let out = derive_pa_subgraph_blocks(&[block], &mut interner);
        assert!(out.is_empty());
    }

    /// case-insensitive prefix matching with mixed slash styles.
    #[test]
    fn case_insensitive_prefix() {
        let mut interner = StringInterner::new();
        let block = SubgraphBlock {
            behaviour_graph: sym("Actors\\Character\\Foo.hkx", &mut interner),
            paths: vec![sym(
                "actors/character/ANIMATIONS/weapon/MyGun",
                &mut interner,
            )],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };
        let out = derive_pa_subgraph_blocks(&[block], &mut interner);
        assert_eq!(out.len(), 1);
        let p = interner.resolve(out[0].paths[0]).unwrap();
        assert_eq!(p, "Actors\\powerarmor\\animations\\Weapons\\MyGun");
    }

    /// generic shared paths within a PA-relevant block are
    /// preserved (only the specific paths get rewritten; the block survives
    /// because it has at least one specific path).
    #[test]
    fn preserves_shared_paths_inside_pa_block() {
        let mut interner = StringInterner::new();
        let shared = sym(
            "Actors\\Character\\Animations\\Paired\\Kill01",
            &mut interner,
        );
        let specific = sym(
            "Actors\\Character\\animations\\weapon\\MyGun",
            &mut interner,
        );
        let block = SubgraphBlock {
            behaviour_graph: sym("Actors\\Character\\Foo.hkx", &mut interner),
            paths: vec![shared, specific],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };
        let out = derive_pa_subgraph_blocks(&[block], &mut interner);
        assert_eq!(out.len(), 1);
        // Shared path preserved unchanged; specific path rewritten.
        assert_eq!(
            interner.resolve(out[0].paths[0]).unwrap(),
            "Actors\\Character\\Animations\\Paired\\Kill01"
        );
        assert_eq!(
            interner.resolve(out[0].paths[1]).unwrap(),
            "Actors\\powerarmor\\animations\\Weapons\\MyGun"
        );
    }

    #[test]
    fn heavy_weapon_block_uses_pa_heavy_graph_and_roots() {
        let mut interner = StringInterner::new();
        let block = SubgraphBlock {
            behaviour_graph: sym(
                "Actors\\Character\\Behaviors\\WeaponBehavior.hkx",
                &mut interner,
            ),
            paths: vec![
                sym(
                    "Actors\\Character\\Animations\\Weapon\\M2\\Player",
                    &mut interner,
                ),
                sym("Actors\\Character\\Animations\\Weapon\\M2", &mut interner),
                sym(
                    "Actors\\Character\\Animations\\Weapon\\GripHeavy",
                    &mut interner,
                ),
                sym("Actors\\Character\\Animations\\Paired", &mut interner),
            ],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };

        let out = derive_pa_subgraph_blocks(&[block], &mut interner);

        assert_eq!(out.len(), 1);
        assert_eq!(
            interner.resolve(out[0].behaviour_graph).unwrap(),
            "Actors\\Character\\Behaviors\\PowerArmorHeavyWeaponWrappingBehavior.hkx"
        );
        let paths: Vec<String> = out[0]
            .paths
            .iter()
            .map(|s| interner.resolve(*s).unwrap().to_string())
            .collect();
        assert_eq!(
            paths
                .iter()
                .filter(|p| p.eq_ignore_ascii_case("Actors\\PowerArmor\\Animations\\Weapons\\M2"))
                .count(),
            1
        );
        assert!(
            paths
                .iter()
                .any(|p| p.eq_ignore_ascii_case("Actors\\PowerArmor\\Animations\\Grips\\Minigun"))
        );
        assert!(
            paths
                .iter()
                .any(|p| p.eq_ignore_ascii_case("Actors\\PowerArmor\\Animations\\Paired"))
        );
        assert!(
            !paths
                .iter()
                .any(|p| p.eq_ignore_ascii_case("Actors\\Character\\Animations\\Paired"))
        );
    }

    /// empty input → empty output.
    #[test]
    fn empty_input_returns_empty() {
        let mut interner = StringInterner::new();
        assert!(derive_pa_subgraph_blocks(&[], &mut interner).is_empty());
    }
}
