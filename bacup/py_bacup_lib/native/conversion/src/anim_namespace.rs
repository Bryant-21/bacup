//! Namespaced output tree for converted weapon animations.
//!
//! FO76 re-authored many weapon clips at the same relative path as an FO4 clip.
//! Shipping them there overwrites the base game; skipping them (the base-game
//! dedup in `convert_havok`) leaves converted weapons playing FO4's clips.
//! Instead, colliding clips go under a namespace segment and the owning
//! animation set is inserted into its RACE subgraph's `SAPT` chain ahead of the
//! original. `SAPT` is an ordered search path, so the converted clip wins where
//! it exists and the base game resolves the rest.
//!
//! The shipped path and the `SAPT` entry come from different phases and must
//! match exactly, so the mapping lives here.

/// Directory segment inserted after `Animations` to hold converted clips.
///
/// Named for the SOURCE game, not for BACUP: every pair converts into FO4, so a
/// shared segment would collide between two converted mods installed together.
pub const ANIM_NAMESPACE: &str = "FO76";

/// Animation trees eligible for namespacing, as a lower-cased forward-slash
/// prefix, the namespace insertion index, and the weapon-set directory index.
///
/// Weapon and first-person trees only. Shared trees (`Behaviors`,
/// `CharacterAssets`, the locomotion roots) stay skipped by the base-game
/// dedup so converted clips can never displace FO4's defaults.
const ELIGIBLE_TREES: &[(&str, usize, usize)] = &[
    ("actors/character/animations/weapon/", 3, 4),
    ("actors/character/_1stperson/animations/", 4, 4),
    ("actors/powerarmor/animations/weapons/", 3, 4),
    ("actors/powerarmor/_1stperson/animations/", 4, 4),
];

/// Directories directly under a first-person animation root that hold shared
/// infrastructure rather than a weapon's own set. The third-person trees are
/// already narrowed to `Weapon\`; the first-person roots hold their weapon sets
/// as direct children, so the shared siblings are excluded by name instead.
const SHARED_ANIM_DIRS: &[&str] = &["common", "paired", "furniture", "pipboy", "loose"];

/// Namespaced twin of `path`, or `None` when `path` is not an eligible weapon
/// or first-person animation path.
///
/// Accepts either separator and preserves the one it was given along with the
/// original segment casing. `path` must be relative to `Meshes/`.
pub fn namespaced_anim_path(path: &str) -> Option<String> {
    let forward = path.replace('\\', "/");
    let lower = forward.to_ascii_lowercase();
    let insert_at = ELIGIBLE_TREES
        .iter()
        .find_map(|(prefix, index, _)| lower.starts_with(prefix).then_some(*index))?;

    let mut segments: Vec<&str> = forward.split('/').collect();
    if segments.len() <= insert_at {
        return None;
    }
    // Already namespaced — inserting again would produce a second nested copy.
    if segments[insert_at].eq_ignore_ascii_case(ANIM_NAMESPACE) {
        return None;
    }
    let head = segments[insert_at];
    if SHARED_ANIM_DIRS
        .iter()
        .any(|shared| head.eq_ignore_ascii_case(shared))
    {
        return None;
    }
    // A loose clip sitting directly on the root is shared, not a weapon set.
    if insert_at + 1 == segments.len() && head.to_ascii_lowercase().ends_with(".hkx") {
        return None;
    }
    segments.insert(insert_at, ANIM_NAMESPACE);

    let separator = if path.contains('\\') { "\\" } else { "/" };
    Some(segments.join(separator))
}

/// Stable key for the weapon animation set containing `path`.
///
/// A subgraph searches its own set and then several shared fallback sets. Only the
/// OWNING set is namespaced: a fallback is shared by every weapon in the grip, so a
/// namespaced twin there would hand all of them one FO76 weapon's clips ahead of FO4's
/// compatible ones. Widening it was tried and measured to fail in game.
pub fn animation_set_path_key(path: &str) -> Option<String> {
    namespaced_anim_path(path)?;

    let forward = path.replace('\\', "/");
    let lower = forward.to_ascii_lowercase();
    let root_at = ELIGIBLE_TREES
        .iter()
        .find_map(|(prefix, _, root_at)| lower.starts_with(prefix).then_some(*root_at))?;
    let segments: Vec<&str> = lower.split('/').collect();
    Some(segments.get(..=root_at)?.join("/"))
}

/// Whether `path` already sits under the namespace.
pub fn is_namespaced_anim_path(path: &str) -> bool {
    let forward = path.replace('\\', "/");
    let mut segments: Vec<&str> = forward.split('/').collect();
    let Some(index) = segments
        .iter()
        .position(|segment| segment.eq_ignore_ascii_case(ANIM_NAMESPACE))
    else {
        return false;
    };
    segments.remove(index);
    let without = segments.join("/").to_ascii_lowercase();
    ELIGIBLE_TREES
        .iter()
        .any(|(prefix, at, _)| *at == index && without.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespaces_character_weapon_dir() {
        assert_eq!(
            namespaced_anim_path("Actors\\Character\\Animations\\Weapon\\Pistol").as_deref(),
            Some("Actors\\Character\\Animations\\FO76\\Weapon\\Pistol")
        );
    }

    #[test]
    fn namespaces_character_first_person_dir() {
        assert_eq!(
            namespaced_anim_path("Actors\\Character\\_1stPerson\\Animations\\SingleActionRevolver")
                .as_deref(),
            Some("Actors\\Character\\_1stPerson\\Animations\\FO76\\SingleActionRevolver")
        );
    }

    #[test]
    fn namespaces_power_armor_dirs() {
        assert_eq!(
            namespaced_anim_path("Actors\\PowerArmor\\Animations\\Weapons\\M2").as_deref(),
            Some("Actors\\PowerArmor\\Animations\\FO76\\Weapons\\M2")
        );
        assert_eq!(
            namespaced_anim_path("Actors\\PowerArmor\\_1stPerson\\Animations\\Pistol").as_deref(),
            Some("Actors\\PowerArmor\\_1stPerson\\Animations\\FO76\\Pistol")
        );
    }

    #[test]
    fn namespaces_files_and_keeps_separator_and_case() {
        assert_eq!(
            namespaced_anim_path(
                "actors/character/animations/weapon/SingleActionRevolver/wpnfiresingleready.hkx"
            )
            .as_deref(),
            Some(
                "actors/character/animations/FO76/weapon/SingleActionRevolver/wpnfiresingleready.hkx"
            )
        );
    }

    #[test]
    fn animation_set_key_groups_player_subdir_with_owning_weapon_only() {
        assert_eq!(
            animation_set_path_key("Actors\\Character\\Animations\\Weapon\\AlienRifle\\Player")
                .as_deref(),
            Some("actors/character/animations/weapon/alienrifle")
        );
        assert_ne!(
            animation_set_path_key("Actors\\Character\\Animations\\Weapon\\AlienRifle"),
            animation_set_path_key("Actors\\Character\\Animations\\Weapon\\GripRifleStraight")
        );
    }

    #[test]
    fn leaves_shared_and_locomotion_trees_alone() {
        for path in [
            "Actors\\Character\\Behaviors\\WeaponBehavior.hkx",
            "Actors\\Character\\CharacterAssets\\skeleton.hkx",
            "Actors\\Character\\Animations\\MT\\Neutral",
            "Actors\\Character\\Animations\\Paired",
            "Actors\\Character\\Animations",
            // Shared siblings of the first-person weapon sets.
            "Actors\\Character\\_1stPerson\\Animations\\Paired",
            "Actors\\Character\\_1stPerson\\Animations\\Common",
            "Actors\\Character\\_1stPerson\\Animations\\Pipboy",
            "Actors\\PowerArmor\\_1stPerson\\Animations\\Common",
            // Loose clip on the first-person root, not a weapon set.
            "Actors\\Character\\_1stPerson\\Animations\\idleteleport.hkx",
        ] {
            assert_eq!(namespaced_anim_path(path), None, "{path}");
        }
    }

    #[test]
    fn is_idempotent() {
        let once = namespaced_anim_path("Actors\\Character\\Animations\\Weapon\\Pistol").unwrap();
        assert_eq!(namespaced_anim_path(&once), None);
        assert!(is_namespaced_anim_path(&once));
        assert!(!is_namespaced_anim_path(
            "Actors\\Character\\Animations\\Weapon\\Pistol"
        ));
    }
}
