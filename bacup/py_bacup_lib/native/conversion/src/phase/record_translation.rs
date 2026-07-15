//! `record_translation_maps` phase — validates all embedded YAML translation-map
//! data at run init.
//!
//! # Params
//! ```json
//! {
//!   "translation_map_overrides_dir": null,   // optional path; reserved for future use
//!   "validate_only": false                   // if true, only validate, skip signalling
//! }
//! ```
//!
//! # Responsibility
//! After this phase runs, the caller can be confident that all embedded
//! translation-map YAML files parse correctly.  The translator itself uses the
//! embedded maps via `crate::embedded::PRIMARY_MAPS`.

use crate::embedded;
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

/// Validate that a YAML string is parseable as a YAML mapping.
/// Returns the top-level key count on success, or an error string.
fn validate_yaml_is_mapping(label: &str, text: &str) -> Result<usize, String> {
    if text.trim().is_empty() {
        return Ok(0);
    }
    let value: serde_json::Value =
        serde_saphyr::from_str(text).map_err(|e| format!("{label}: parse error: {e}"))?;
    match &value {
        serde_json::Value::Object(map) => Ok(map.len()),
        serde_json::Value::Null => Ok(0),
        _ => Err(format!("{label}: expected a YAML mapping, got non-object")),
    }
}

/// Validate all embedded YAML files.
/// Returns a list of error strings (empty = all good).
pub fn validate_all_embedded() -> Vec<String> {
    embedded::ALL_YAMLS
        .iter()
        .filter_map(|(label, text)| validate_yaml_is_mapping(label, text).err())
        .collect()
}

// ---------------------------------------------------------------------------
// Phase impl
// ---------------------------------------------------------------------------

pub struct RecordTranslationMapsPhase;

impl Phase for RecordTranslationMapsPhase {
    fn name(&self) -> &'static str {
        "record_translation_maps"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        let _validate_only = ctx
            .params
            .get("validate_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let errors = validate_all_embedded();
        if !errors.is_empty() {
            let msg = errors.join("\n");
            return Err(PhaseError::Internal(format!(
                "Embedded translation-map validation failed:\n{msg}"
            )));
        }

        let map_count = embedded::ALL_YAMLS.len() as u32;

        ctx.run
            .event_tx
            .try_send(PhaseEvent::Log {
                phase: self.name(),
                level: LogLevel::Info,
                message: format!(
                    "record_translation_maps: {map_count} embedded YAML files validated OK"
                ),
            })
            .ok();

        Ok(PhaseReport {
            records_changed: 0,
            records_added: 0,
            records_vanilla_remapped: 0,
            records_dropped: 0,
            records_deferred: 0,
            assets_written: 0,
            warnings: 0,
            elapsed_ms: 0,
            items_failed: 0,
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
    fn all_embedded_yamls_parse_without_error() {
        let errors = validate_all_embedded();
        assert!(
            errors.is_empty(),
            "Embedded YAML validation failures:\n{}",
            errors.join("\n")
        );
    }

    #[test]
    fn fo76_to_fo4_map_has_expected_top_level_keys() {
        let value: serde_json::Value =
            serde_saphyr::from_str(embedded::FO76_TO_FO4).expect("fo76_to_fo4 should parse");
        let obj = value.as_object().expect("fo76_to_fo4 should be a mapping");
        assert!(
            obj.contains_key("AMMO"),
            "fo76_to_fo4 map missing AMMO record block"
        );
        assert!(
            obj.contains_key("WEAP"),
            "fo76_to_fo4 map missing WEAP record block"
        );
    }

    #[test]
    fn fnv_to_fo4_map_has_skip_records() {
        let value: serde_json::Value =
            serde_saphyr::from_str(embedded::FNV_TO_FO4).expect("fnv_to_fo4 should parse");
        let obj = value.as_object().expect("fnv_to_fo4 should be a mapping");
        assert!(
            obj.contains_key("skip_records"),
            "fnv_to_fo4 should have a skip_records section"
        );
    }

    #[test]
    fn fo76_condition_functions_parses_as_mapping() {
        let count = validate_yaml_is_mapping(
            "fo76_condition_functions",
            embedded::FO76_CONDITION_FUNCTIONS,
        )
        .expect("fo76_condition_functions should be a valid YAML mapping");
        assert_eq!(
            count, 1,
            "fo76_condition_functions should have one top-level key (functions)"
        );
    }

    #[test]
    fn validate_all_embedded_is_clean() {
        // Gate test: any malformed embedded YAML surfaces here at CI.
        let errors = validate_all_embedded();
        assert!(errors.is_empty(), "Expected no errors, got: {errors:?}");
    }

    #[test]
    fn embedded_map_count_matches_expected() {
        assert_eq!(
            embedded::ALL_YAMLS.len(),
            27,
            "ALL_YAMLS entry count changed — update this test"
        );
    }
}
