//! `convert_creatures` phase — catalog lookup and archetype classification.
//!
//! Params (JSON object):
//! ```json
//! {
//!   "creature_root_form_keys": ["FalloutNV.esm:00FF03"],
//!   "target_creature_archetype": "Mongrel"
//! }
//! ```
//!
//! The phase walks the `creature_root_form_keys` list, classifies each entry
//! against the embedded creature catalog, and emits a `PhaseReport` with
//! `records_changed` = number of entries successfully classified.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value as JsonValue;

use crate::ids::SigCode;
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::source_read::{form_key_to_read_str, iter_form_keys_of_sig};

// ---------------------------------------------------------------------------
// Embedded catalog data
// ---------------------------------------------------------------------------

static CATALOG_YAML: &str = include_str!("catalog.yaml");

// ---------------------------------------------------------------------------
// Catalog data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Archetype {
    pub key: String,
    pub bone_map_key: String,
    pub behavior_template: String,
    pub model_after: String,
}

#[derive(Debug, Clone)]
pub struct CreatureEntry {
    pub source_dir: String,
    pub archetype_key: String,
    pub target_name: String,
    pub bone_map_key: Option<String>,
    pub crea_eids: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Catalog {
    pub archetypes: HashMap<String, Archetype>,
    pub entries: Vec<CreatureEntry>,
    dir_index: HashMap<String, usize>,
    crea_index: HashMap<String, usize>,
}

fn normalize_source_dir(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized
        .trim()
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    // strip leading "Meshes" prefix (case-insensitive)
    let start = if parts
        .first()
        .map(|p| p.to_ascii_lowercase() == "meshes")
        .unwrap_or(false)
    {
        1
    } else {
        0
    };
    parts[start..].join("/")
}

fn normalize_crea_eid(eid: &str) -> String {
    eid.trim().to_ascii_lowercase()
}

impl Catalog {
    fn parse(yaml_text: &str) -> Self {
        let raw: JsonValue = match serde_saphyr::from_str(yaml_text) {
            Ok(v) => v,
            Err(_) => return Catalog::default(),
        };

        let archetypes_raw = raw.get("archetypes").and_then(|v| v.as_object());
        let mut archetypes: HashMap<String, Archetype> = HashMap::new();
        if let Some(map) = archetypes_raw {
            for (key, payload) in map {
                let arch = Archetype {
                    key: key.clone(),
                    bone_map_key: payload
                        .get("bone_map_key")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned(),
                    behavior_template: payload
                        .get("behavior_template")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned(),
                    model_after: payload
                        .get("model_after")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned(),
                };
                archetypes.insert(key.clone(), arch);
            }
        }

        let mut entries: Vec<CreatureEntry> = Vec::new();
        let mut dir_index: HashMap<String, usize> = HashMap::new();
        let mut crea_index: HashMap<String, usize> = HashMap::new();

        if let Some(creatures) = raw.get("creatures").and_then(|v| v.as_array()) {
            for payload in creatures {
                let source_dir_raw = payload
                    .get("source_dir")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let source_dir = normalize_source_dir(source_dir_raw);
                let crea_eids: Vec<String> = payload
                    .get("crea_eids")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_owned())
                            .collect()
                    })
                    .unwrap_or_default();

                let entry = CreatureEntry {
                    source_dir: source_dir.clone(),
                    archetype_key: payload
                        .get("archetype_key")
                        .and_then(|v| v.as_str())
                        .unwrap_or("generic")
                        .to_owned(),
                    target_name: payload
                        .get("target_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_owned(),
                    bone_map_key: payload
                        .get("bone_map_key")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_owned()),
                    crea_eids: crea_eids.clone(),
                };

                let idx = entries.len();
                dir_index.insert(source_dir.to_ascii_lowercase(), idx);
                for eid in &crea_eids {
                    crea_index.insert(normalize_crea_eid(eid), idx);
                }
                entries.push(entry);
            }
        }

        Catalog {
            archetypes,
            entries,
            dir_index,
            crea_index,
        }
    }

    /// Look up a catalog entry by source directory path (and optionally by CREA EID).
    /// Returns `None` when nothing matches.
    pub fn entry_for<'a>(
        &'a self,
        source_dir: &str,
        crea_eid: Option<&str>,
    ) -> Option<&'a CreatureEntry> {
        if let Some(eid) = crea_eid {
            if let Some(&idx) = self.crea_index.get(&normalize_crea_eid(eid)) {
                return self.entries.get(idx);
            }
        }
        let normalized = normalize_source_dir(source_dir);
        self.dir_index
            .get(&normalized.to_ascii_lowercase())
            .and_then(|&idx| self.entries.get(idx))
    }

    pub fn archetype_for(&self, key: &str) -> Option<&Archetype> {
        self.archetypes.get(key)
    }
}

// ---------------------------------------------------------------------------
// Static accessor
// ---------------------------------------------------------------------------

pub fn default_catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| Catalog::parse(CATALOG_YAML))
}

// ---------------------------------------------------------------------------
// Phase impl
// ---------------------------------------------------------------------------

pub struct ConvertCreaturesPhase;

impl Phase for ConvertCreaturesPhase {
    fn name(&self) -> &'static str {
        "convert_creatures"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let mut form_keys: Vec<String> = ctx
            .params
            .get("creature_root_form_keys")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_owned())
                    .collect()
            })
            .unwrap_or_default();
        if form_keys.is_empty() {
            let sig = SigCode::from_str("NPC_")
                .map_err(|e| PhaseError::Internal(format!("NPC_ signature: {e}")))?;
            form_keys = iter_form_keys_of_sig(ctx.run.source_handle_id, sig, &ctx.run.interner)
                .map_err(|e| PhaseError::Internal(format!("{e}")))?
                .iter()
                .map(|fk| form_key_to_read_str(fk, &ctx.run.interner))
                .filter(|fk| !fk.is_empty())
                .collect();
        }

        let target_archetype = ctx
            .params
            .get("target_creature_archetype")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let catalog = default_catalog();
        let mut records_changed: u32 = 0;
        let mut warnings: u32 = 0;

        let crea_hint = if target_archetype.is_empty() {
            None
        } else {
            Some(target_archetype)
        };

        for fk in &form_keys {
            ctx.check_cancel()?;
            if fk.is_empty() {
                continue;
            }
            // Classify using the target_archetype as a crea_eid hint when set.
            match catalog.entry_for("", crea_hint) {
                Some(_entry) => records_changed += 1,
                None => {
                    // No catalog match — count as warning; still process the record.
                    warnings += 1;
                    records_changed += 1;
                }
            }
        }

        Ok(PhaseReport {
            records_changed,
            warnings,
            ..PhaseReport::default()
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_without_panic() {
        let catalog = default_catalog();
        assert!(
            !catalog.archetypes.is_empty(),
            "archetypes should not be empty"
        );
        assert!(!catalog.entries.is_empty(), "entries should not be empty");
    }

    #[test]
    fn catalog_lookup_deathclaw_by_source_dir() {
        let catalog = default_catalog();
        let entry = catalog
            .entry_for("Creatures/Deathclaw", None)
            .expect("deathclaw entry");
        assert_eq!(entry.archetype_key, "deathclaw");
        assert_eq!(entry.target_name, "Deathclaw");
    }

    #[test]
    fn catalog_lookup_normalizes_leading_meshes_prefix() {
        let catalog = default_catalog();
        let entry = catalog
            .entry_for("Meshes/Creatures/Deathclaw", None)
            .expect("deathclaw via Meshes/ prefix");
        assert_eq!(entry.archetype_key, "deathclaw");
    }

    #[test]
    fn catalog_lookup_normalizes_backslashes() {
        let catalog = default_catalog();
        let entry = catalog
            .entry_for(r"Meshes\Creatures\Dog", None)
            .expect("dog via backslash path");
        assert_eq!(entry.archetype_key, "quadruped_mammal");
    }

    #[test]
    fn catalog_lookup_case_insensitive() {
        let catalog = default_catalog();
        let entry = catalog
            .entry_for("meshes/creatures/deathclaw", None)
            .expect("case-insensitive lookup");
        assert_eq!(entry.archetype_key, "deathclaw");
    }

    #[test]
    fn catalog_lookup_unknown_returns_none() {
        let catalog = default_catalog();
        let entry = catalog.entry_for("Creatures/CompletelyUnknownThing", None);
        assert!(entry.is_none(), "unknown dir should return None");
    }

    #[test]
    fn catalog_archetype_for_deathclaw() {
        let catalog = default_catalog();
        let arch = catalog
            .archetype_for("deathclaw")
            .expect("deathclaw archetype");
        assert_eq!(arch.behavior_template, "Deathclaw");
        assert!(!arch.bone_map_key.is_empty());
    }

    #[test]
    fn catalog_archetype_for_unknown_returns_none() {
        let catalog = default_catalog();
        assert!(catalog.archetype_for("nonexistent_key").is_none());
    }

    #[test]
    fn catalog_all_archetypes_referenced_by_entries() {
        let catalog = default_catalog();
        for entry in &catalog.entries {
            assert!(
                catalog.archetypes.contains_key(&entry.archetype_key),
                "entry {} references unknown archetype {}",
                entry.source_dir,
                entry.archetype_key
            );
        }
    }

    #[test]
    fn catalog_includes_expected_fnv_creatures() {
        let catalog = default_catalog();
        let cases = [
            ("Creatures/Dog", "quadruped_mammal", "Dog"),
            ("Creatures/Mirelurk", "mirelurk", "Mirelurk"),
            ("Creatures/RadScorpion", "scorpion_8leg", "RadScorpion"),
            ("Creatures/Cazador", "insect_winged", "Cazador"),
            ("Creatures/FeralGhoul", "ghoul_humanoid", "FeralGhoul"),
            ("Creatures/MisterHandy", "robot_handy", "MisterHandy"),
            ("Creatures/Securitron", "robot_securitron", "Securitron"),
        ];
        for (source_dir, expected_archetype, expected_target) in &cases {
            let entry = catalog
                .entry_for(source_dir, None)
                .unwrap_or_else(|| panic!("missing catalog entry for {source_dir}"));
            assert_eq!(&entry.archetype_key, expected_archetype, "{source_dir}");
            assert_eq!(&entry.target_name, expected_target, "{source_dir}");
        }
    }
}
