//! Embedded YAML data files — compiled into the binary via `include_str!`.
//!
//! Translation maps, whitelists, condition-function tables, and other
//! per-game-pair YAML data are stored here. No filesystem access is required
//! at runtime; the text is baked in at compile time.

// ---------------------------------------------------------------------------
// Translation maps (one per game pair)
// ---------------------------------------------------------------------------

pub const AMMO_FNV_TO_FO4: &str = include_str!("embedded/translation_maps/ammo_fnv_to_fo4.yaml");
pub const EVENTS_FO3_TO_FO4: &str =
    include_str!("embedded/translation_maps/events_fo3_to_fo4.yaml");
pub const EVENTS_FO4_TO_FO3: &str =
    include_str!("embedded/translation_maps/events_fo4_to_fo3.yaml");
pub const EVENTS_FO76_TO_FO4: &str =
    include_str!("embedded/translation_maps/events_fo76_to_fo4.yaml");
pub const FNV_TO_FO4: &str = include_str!("embedded/translation_maps/fnv_to_fo4.yaml");
pub const FO3_TO_FO4: &str = include_str!("embedded/translation_maps/fo3_to_fo4.yaml");
pub const FO4_TO_SKYRIMSE: &str = include_str!("embedded/translation_maps/fo4_to_skyrimse.yaml");
pub const FO76_TO_FNV: &str = include_str!("embedded/translation_maps/fo76_to_fnv.yaml");
pub const FO76_TO_FO4: &str = include_str!("embedded/translation_maps/fo76_to_fo4.yaml");
pub const FO76_TO_SKYRIMSE: &str = include_str!("embedded/translation_maps/fo76_to_skyrimse.yaml");
pub const SKELETON_FNV_TO_FO4_CREATURES: &str =
    include_str!("embedded/translation_maps/skeleton_fnv_to_fo4_creatures.yaml");
pub const SKELETON_FNV_TO_FO4_ROBOTS: &str =
    include_str!("embedded/translation_maps/skeleton_fnv_to_fo4_robots.yaml");
pub const SKELETON_FO3_TO_FO4: &str =
    include_str!("embedded/translation_maps/skeleton_fo3_to_fo4.yaml");
pub const SKELETON_FO3_TO_FO4_CREATURES: &str =
    include_str!("embedded/translation_maps/skeleton_fo3_to_fo4_creatures.yaml");
pub const SKYRIMSE_TO_FO4: &str = include_str!("embedded/translation_maps/skyrimse_to_fo4.yaml");
pub const STARFIELD_TO_FO4: &str = include_str!("embedded/translation_maps/starfield_to_fo4.yaml");

// ---------------------------------------------------------------------------
// Whitelists
// ---------------------------------------------------------------------------

pub const WHITELIST_FNV: &str = include_str!("embedded/whitelists/fnv.yaml");
pub const WHITELIST_FO3: &str = include_str!("embedded/whitelists/fo3.yaml");
pub const WHITELIST_FO4: &str = include_str!("embedded/whitelists/fo4.yaml");
pub const WHITELIST_FO76: &str = include_str!("embedded/whitelists/fo76.yaml");
pub const WHITELIST_SKYRIMSE: &str = include_str!("embedded/whitelists/skyrimse.yaml");
pub const WHITELIST_STARFIELD: &str = include_str!("embedded/whitelists/starfield.yaml");
pub const UNIVERSAL_OMOD_KEYWORDS: &str =
    include_str!("embedded/whitelists/universal_omod_keywords.yaml");

// ---------------------------------------------------------------------------
// Standalone data files
// ---------------------------------------------------------------------------

pub const FO76_CONDITION_FUNCTIONS: &str = include_str!("embedded/fo76_condition_functions.yaml");
pub const SKYRIMSE_CONDITION_FUNCTIONS: &str =
    include_str!("embedded/skyrimse_condition_functions.yaml");
pub const MATERIAL_SOURCE_OVERRIDES: &str = include_str!("embedded/material_source_overrides.yaml");
pub const WEAPON_EXTRA_FKS: &str = include_str!("embedded/weapon_extra_fks.yaml");

// ---------------------------------------------------------------------------
// Lookup tables
// ---------------------------------------------------------------------------

/// Primary translation maps keyed by `"<source>_to_<target>"` label.
/// Does not include ammo/events/skeleton variant maps (accessed directly).
pub const PRIMARY_MAPS: &[(&str, &str)] = &[
    ("fnv_to_fo4", FNV_TO_FO4),
    ("fo3_to_fo4", FO3_TO_FO4),
    ("fo4_to_skyrimse", FO4_TO_SKYRIMSE),
    ("fo76_to_fnv", FO76_TO_FNV),
    ("fo76_to_fo4", FO76_TO_FO4),
    ("fo76_to_skyrimse", FO76_TO_SKYRIMSE),
    ("skyrimse_to_fo4", SKYRIMSE_TO_FO4),
    ("starfield_to_fo4", STARFIELD_TO_FO4),
];

/// Every embedded YAML file with its label — used by bulk-validation.
pub const ALL_YAMLS: &[(&str, &str)] = &[
    ("ammo_fnv_to_fo4", AMMO_FNV_TO_FO4),
    ("events_fo3_to_fo4", EVENTS_FO3_TO_FO4),
    ("events_fo4_to_fo3", EVENTS_FO4_TO_FO3),
    ("events_fo76_to_fo4", EVENTS_FO76_TO_FO4),
    ("fnv_to_fo4", FNV_TO_FO4),
    ("fo3_to_fo4", FO3_TO_FO4),
    ("fo4_to_skyrimse", FO4_TO_SKYRIMSE),
    ("fo76_to_fnv", FO76_TO_FNV),
    ("fo76_to_fo4", FO76_TO_FO4),
    ("fo76_to_skyrimse", FO76_TO_SKYRIMSE),
    (
        "skeleton_fnv_to_fo4_creatures",
        SKELETON_FNV_TO_FO4_CREATURES,
    ),
    ("skeleton_fnv_to_fo4_robots", SKELETON_FNV_TO_FO4_ROBOTS),
    ("skeleton_fo3_to_fo4", SKELETON_FO3_TO_FO4),
    (
        "skeleton_fo3_to_fo4_creatures",
        SKELETON_FO3_TO_FO4_CREATURES,
    ),
    ("skyrimse_to_fo4", SKYRIMSE_TO_FO4),
    ("starfield_to_fo4", STARFIELD_TO_FO4),
    ("whitelist_fnv", WHITELIST_FNV),
    ("whitelist_fo3", WHITELIST_FO3),
    ("whitelist_fo4", WHITELIST_FO4),
    ("whitelist_fo76", WHITELIST_FO76),
    ("whitelist_skyrimse", WHITELIST_SKYRIMSE),
    ("whitelist_starfield", WHITELIST_STARFIELD),
    ("universal_omod_keywords", UNIVERSAL_OMOD_KEYWORDS),
    ("fo76_condition_functions", FO76_CONDITION_FUNCTIONS),
    ("skyrimse_condition_functions", SKYRIMSE_CONDITION_FUNCTIONS),
    ("material_source_overrides", MATERIAL_SOURCE_OVERRIDES),
    ("weapon_extra_fks", WEAPON_EXTRA_FKS),
];
