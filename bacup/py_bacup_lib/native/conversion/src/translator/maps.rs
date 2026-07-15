//! Translation map loader — reads per-game-pair YAML files into typed structs.
//!
//! Translation-map YAML data is embedded in the binary via `include_str!` in
//! `crate::phase::record_translation::embedded`.  `TranslationMaps::load` looks
//! up the embedded text; no filesystem access is required at runtime.

use super::super::errors::ConfigError;
use super::Game;
use crate::embedded;

/// Return the embedded YAML text for the map named `key` (e.g. `"fo76_to_fo4"`).
/// Returns an empty string if no embedded map exists for that key.
fn embedded_map_text(key: &str) -> &'static str {
    for (label, text) in embedded::PRIMARY_MAPS {
        if *label == key {
            return text;
        }
    }
    ""
}

/// The raw serde-saphyr YAML value type used to hold transform configs.
/// Reusing serde_json::Value since serde-saphyr can deserialize into it.
pub type YamlValue = serde_json::Value;

/// Per-record translation specification loaded from the YAML map file.
#[derive(Debug, Default)]
pub struct RecordMap {
    /// Source record sig (4-char, matches the YAML key).
    pub source_sig: String,
    /// Optional target sig override (e.g. CREA → NPC_).
    pub target_sig: Option<String>,
    /// Field rewrites: source_field → target_field name mappings.
    pub field_rewrites: Vec<FieldRewrite>,
    /// Named transform invocations to run on specific fields.
    pub transforms: Vec<TransformInvocation>,
    /// Fields to drop entirely.
    pub drop_fields: Vec<String>,
}

/// A simple field-name rewrite: rename source_field to target_field.
#[derive(Debug)]
pub struct FieldRewrite {
    pub source_field: String,
    pub target_field: String,
}

/// A named transform + its config blob.
#[derive(Debug)]
pub struct TransformInvocation {
    pub field: String,
    pub name: String,
    pub config: YamlValue,
}

/// Loaded translation maps for one game pair.
#[derive(Debug, Default)]
pub struct TranslationMaps {
    record_maps: rustc_hash::FxHashMap<String, RecordMap>,
    pub skip_records: rustc_hash::FxHashSet<String>,
}

impl TranslationMaps {
    /// Load the YAML translation map for (source, target).
    ///
    /// Uses embedded YAML data compiled into the binary.  If no embedded map
    /// exists for the pair, returns an empty `TranslationMaps` (not an error —
    /// some pairs intentionally have no map).
    pub fn load(source: Game, target: Game) -> Result<Self, ConfigError> {
        let key = format!("{}_to_{}", source.as_str(), target.as_str());
        let text = embedded_map_text(&key);
        if text.trim().is_empty() {
            return Ok(TranslationMaps::default());
        }
        let raw: serde_json::Value =
            serde_saphyr::from_str(text).map_err(|e| ConfigError::MapFileMalformed {
                path: std::path::PathBuf::from(format!("embedded:{key}.yaml")),
                source: e.to_string(),
            })?;
        let mut maps = Self::from_value(raw)?;
        if source == Game::Fo76
            && target == Game::Fo4
            && let Some(npc_map) = maps.record_maps.get_mut("NPC_")
        {
            // NPC tint layers depend on RACE tint tables that are intentionally
            // not carried into FO4. Drop the header and payload together while
            // leaving QNAM intact for the body/face skin-tone match.
            npc_map.drop_fields.push("TETI".to_string());
            npc_map.drop_fields.push("TEND".to_string());
        }
        // CK-crash-risk bisect gate: SCEN/DLBR are emitted by default (they
        // resolve NOTE\SNAM-Scene and INFO\BNAM-DLBR references). Setting
        // MODBOX_DISABLE_SCEN re-skips them so an in-game crash can be
        // bisected against scene emission. FO76->FO4 only.
        if source == Game::Fo76
            && target == Game::Fo4
            && std::env::var_os("MODBOX_DISABLE_SCEN").is_some()
        {
            maps.skip_records.insert("SCEN".to_string());
            maps.skip_records.insert("DLBR".to_string());
        }
        Ok(maps)
    }

    fn from_value(raw: serde_json::Value) -> Result<Self, ConfigError> {
        let mut maps = TranslationMaps::default();

        let obj = match raw {
            serde_json::Value::Object(m) => m,
            _ => return Ok(maps),
        };

        for (key, val) in obj {
            match key.as_str() {
                "skip_records" => {
                    if let serde_json::Value::Array(arr) = val {
                        for item in arr {
                            if let serde_json::Value::String(s) = item {
                                maps.skip_records.insert(s);
                            }
                        }
                    }
                }
                "material_overrides" => {
                    // Ignored at this layer — consumed by Python hooks.
                }
                sig => {
                    let rec_map = parse_record_map(sig, val)?;
                    maps.record_maps.insert(sig.to_string(), rec_map);
                }
            }
        }

        Ok(maps)
    }

    /// Look up the map for a given source record signature (e.g. "WEAP").
    pub fn record_map(&self, sig: &str) -> Option<&RecordMap> {
        self.record_maps.get(sig)
    }
}

fn parse_record_map(sig: &str, val: serde_json::Value) -> Result<RecordMap, ConfigError> {
    let mut rec = RecordMap {
        source_sig: sig.to_string(),
        ..Default::default()
    };

    let obj = match val {
        serde_json::Value::Object(m) => m,
        _ => return Ok(rec),
    };

    // target_record_type
    if let Some(serde_json::Value::String(s)) = obj.get("target_record_type") {
        rec.target_sig = Some(s.clone());
    }

    // fields: { source_field: target_field, ... }
    if let Some(serde_json::Value::Object(fields)) = obj.get("fields") {
        for (src, tgt) in fields {
            if let serde_json::Value::String(tgt_name) = tgt {
                rec.field_rewrites.push(FieldRewrite {
                    source_field: src.clone(),
                    target_field: tgt_name.clone(),
                });
            }
        }
    }

    // transforms: { field: { type: "...", ...config } }
    if let Some(serde_json::Value::Object(transforms)) = obj.get("transforms") {
        for (field, config) in transforms {
            let transform_name = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            rec.transforms.push(TransformInvocation {
                field: field.clone(),
                name: transform_name,
                config: config.clone(),
            });
        }
    }

    // drop: [ field, ... ]
    if let Some(serde_json::Value::Array(drops)) = obj.get("drop") {
        for d in drops {
            if let serde_json::Value::String(s) = d {
                rec.drop_fields.push(s.clone());
            }
        }
    }

    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_fo76_to_fo4_map() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let weap_map = maps.record_map("WEAP").expect("WEAP map");
        assert!(
            !weap_map.field_rewrites.is_empty() || !weap_map.transforms.is_empty(),
            "WEAP map has no field rewrites or transforms"
        );
    }

    #[test]
    fn fo76_to_fo4_drops_weapon_rgw3_instead_of_mapping_to_fnam() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let weap_map = maps.record_map("WEAP").expect("WEAP map");
        assert!(
            !weap_map
                .field_rewrites
                .iter()
                .any(|rewrite| rewrite.source_field == "RGW3"),
            "FO76 WEAP RGW3 bytes must not be decoded as an FO4 WEAP field"
        );
        assert!(
            !weap_map
                .transforms
                .iter()
                .any(|transform| transform.field == "RGW3"),
            "FO76 WEAP RGW3 must not run FO4 form-key transforms"
        );
        assert!(
            weap_map.drop_fields.iter().any(|field| field == "RGW3"),
            "FO76 WEAP RGW3 should be dropped"
        );
    }

    #[test]
    fn fo76_to_fo4_keeps_npc_qnam_skin_tone() {
        // QNAM (Texture lighting) is the NPC skin tone that tints the body to
        // match the FaceGen head. Its 4-float RGBA layout is identical FO76→FO4,
        // and the FO4 whitelist keeps it. A stale `- QNAM` in the NPC_ drop list
        // silently deleted it (drop matches the raw 4CC sig), giving every
        // converted settler a dark neck seam and mismatched eyelashes.
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let npc_map = maps.record_map("NPC_").expect("NPC_ map");
        assert!(
            !npc_map.drop_fields.iter().any(|field| field == "QNAM"),
            "FO76 NPC_ QNAM skin tone must not be dropped"
        );
    }

    #[test]
    fn fo76_to_fo4_keeps_enchantment_conditions() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let enchantment_map = maps.record_map("ENCH").expect("ENCH map");
        assert!(
            enchantment_map
                .field_rewrites
                .iter()
                .any(|rewrite| rewrite.source_field == "CTDA" && rewrite.target_field == "CTDA"),
            "FO76 ENCH conditions must be carried so equipped effects remain gated"
        );
        assert!(
            !enchantment_map
                .drop_fields
                .iter()
                .any(|field| field == "CTDA"),
            "FO76 ENCH CTDA must not be dropped"
        );
    }

    #[test]
    fn fo76_to_fo4_drops_npc_tint_layers_as_complete_groups() {
        // FO76 tint indices are only meaningful against the FO76 RACE tint
        // tables, which are not carried into FO4. Keep QNAM for the body/face
        // skin-tone match, but drop both the TETI header and TEND payload so an
        // invalid layer cannot survive and an orphan payload cannot remain.
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let npc_map = maps.record_map("NPC_").expect("NPC_ map");
        assert!(
            npc_map.drop_fields.iter().any(|field| field == "TETI"),
            "FO76 NPC_ TETI tint indices must be dropped"
        );
        assert!(
            npc_map.drop_fields.iter().any(|field| field == "TEND"),
            "FO76 NPC_ TEND tint payloads must be dropped with TETI"
        );
    }

    #[test]
    fn fo76_to_fo4_drops_race_tint_count_with_tint_tables() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let race_map = maps.record_map("RACE").expect("RACE map");
        assert!(
            race_map.drop_fields.iter().any(|field| field == "TINL"),
            "FO76 RACE TINL must be dropped with the incompatible tint tables"
        );
        assert!(
            !race_map
                .field_rewrites
                .iter()
                .any(|rewrite| rewrite.source_field == "TotalNumberOfTintsInList"),
            "FO76 RACE tint count must not be carried when its tables are dropped"
        );
        for sig in [
            "TTGP", "TETI", "TTEF", "CTDA", "CIS1", "CIS2", "TTET", "TTEB", "TTEC", "TTED", "TTGE",
            "MPGN", "MPPC", "MPPI", "MPPN", "MPPM", "MPPT", "MPPF", "MPPK", "MPGS",
        ] {
            assert!(
                race_map.drop_fields.iter().any(|field| field == sig),
                "FO76 RACE face-table subrecord {sig} must be dropped"
            );
        }
    }

    #[test]
    fn fo76_to_fo4_keeps_audited_shared_subrecords() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        for (record_sig, subrecord_sig) in [
            ("WEAP", "NNAM"),
            ("NPC_", "SPCT"),
            ("RACE", "PHWT"),
            ("RACE", "BSMP"),
            ("RACE", "BSMB"),
            ("RACE", "BSMS"),
            ("RACE", "BMMP"),
            ("RACE", "FMRI"),
            ("RACE", "FMRN"),
            ("RACE", "HEAD"),
            ("RACE", "MSID"),
            ("ARMO", "DAMA"),
            ("STAT", "MODC"),
            ("SCOL", "PTRN"),
            ("SCOL", "FULL"),
            ("CNCY", "MODC"),
            ("CONT", "DATA"),
            ("KEYM", "ICON"),
            ("KEYM", "MICO"),
            ("KEYM", "KSIZ"),
            ("KEYM", "KWDA"),
            ("KEYM", "MODC"),
            ("LVLN", "LVLG"),
            ("LIGH", "MODL"),
            ("BPTD", "NAM5"),
            ("FURN", "MODC"),
            ("MESG", "DNAM"),
            ("IDLM", "IDLF"),
            ("QUST", "ALFC"),
            ("QUST", "KNAM"),
        ] {
            let map = maps.record_map(record_sig).expect("record map");
            assert!(
                !map.drop_fields.iter().any(|field| field == subrecord_sig),
                "{record_sig}.{subrecord_sig} is valid in both games and must not be dropped"
            );
        }
    }

    #[test]
    fn fo76_to_fo4_skips_story_manager_records() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        for sig in ["SMBN", "SMEN", "SMQN"] {
            assert!(
                maps.skip_records.contains(sig),
                "fo76_to_fo4 should skip {sig}"
            );
        }
    }

    #[test]
    fn fo76_to_fo4_skips_records_that_need_projected_worldspace_or_nav_writers() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        for sig in ["ACHR", "CELL", "LAND", "NAVI", "NAVM", "REFR"] {
            assert!(
                maps.skip_records.contains(sig),
                "fo76_to_fo4 should skip {sig}"
            );
        }
    }

    #[test]
    fn fo76_to_fo4_skips_collision_layers() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        assert!(maps.skip_records.contains("COLL"));
    }

    #[test]
    fn fo76_to_fo4_emits_scen_and_dlbr_by_default() {
        // SCEN/DLBR are flat top-level FO4 records emitted by the generic
        // writer (resolve NOTE\SNAM-Scene + INFO\BNAM-DLBR). The
        // MODBOX_DISABLE_SCEN env gate (maps.rs::load) is not exercised here
        // to avoid process-global env races across parallel tests.
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        assert!(
            !maps.skip_records.contains("SCEN"),
            "SCEN must be emitted by default"
        );
        assert!(
            !maps.skip_records.contains("DLBR"),
            "DLBR must be emitted by default"
        );
    }

    #[test]
    fn fo76_to_fo4_converts_currency_records_to_misc() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let cncy_map = maps.record_map("CNCY").expect("CNCY map");
        assert_eq!(cncy_map.target_sig.as_deref(), Some("MISC"));
    }

    #[test]
    fn load_fnv_to_fo4_map_has_skip_records() {
        let maps = TranslationMaps::load(Game::Fnv, Game::Fo4).unwrap();
        assert!(
            !maps.skip_records.is_empty(),
            "fnv_to_fo4 should have skip_records"
        );
        assert!(
            maps.skip_records.contains("NAVI"),
            "FNV NAVI must be rebuilt with the FO4 byte layout"
        );
    }

    #[test]
    fn fo3_to_fo4_rebuilds_source_navi() {
        let maps = TranslationMaps::load(Game::Fo3, Game::Fo4).unwrap();
        for signature in ["NAVI", "NAVM"] {
            assert!(
                maps.skip_records.contains(signature),
                "FO3 {signature} must be rebuilt with the FO4 byte layout"
            );
        }
    }

    #[test]
    fn skyrimse_to_fo4_keeps_world_and_navm_records_but_rebuilds_navi() {
        let maps = TranslationMaps::load(Game::SkyrimSe, Game::Fo4).unwrap();
        for sig in [
            "WRLD", "CELL", "LAND", "NAVM", "REFR", "ACHR", "WATR", "GRAS",
        ] {
            assert!(
                !maps.skip_records.contains(sig),
                "{sig} must reach topology rebuild"
            );
        }
        assert!(
            maps.skip_records.contains("NAVI"),
            "NAVI is rebuilt from converted NAVM topology"
        );
        for sig in ["FACT", "SNDR", "MUST", "IDLE", "CPTH"] {
            let map = maps.record_map(sig).expect("condition-bearing record map");
            assert!(map.transforms.iter().any(|transform| {
                transform.field == "CTDA" && transform.name == "translate_conditions"
            }));
        }
    }

    #[test]
    fn missing_map_returns_empty() {
        // No map file for this pair should exist.
        let maps = TranslationMaps::load(Game::Fo4, Game::Fo76).unwrap();
        assert!(maps.record_map("WEAP").is_none());
        assert!(maps.skip_records.is_empty());
    }
}
