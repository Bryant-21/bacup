//! `strip_subfields` transform — removes or keeps named subfields in a Struct.
//!
//! Python source: `translator.py` line 1117.
//!
//! Config keys (mutually exclusive; `keep` takes precedence if both present):
//! - `remove` — list of subfield names to drop
//! - `keep`   — list of subfield names to retain (all others dropped)
//!
//! If the resulting Struct is empty, the field is dropped (value becomes None).
//! Non-Struct values are passed through unchanged.
//!
//! Example YAML usages:
//!
//! Usage 1 — `remove` (fo76_to_fo4.yaml line 48):
//! ```yaml
//! group_model:
//!   type: strip_subfields
//!   remove: [XFLG, ENLT, ENLS, AUUV, MODD]
//! ```
//!
//! Usage 2 — `keep` (fo4_to_skyrimse.yaml line 104):
//! ```yaml
//! DATA:
//!   type: strip_subfields
//!   keep: [value, Value, weight, Weight]
//! ```

use std::collections::HashSet;

use super::super::super::record::FieldValue;
use super::super::maps::YamlValue;
use super::{Transform, TransformCtx, TransformError};

/// Strips named subfields from a `FieldValue::Struct`.
///
/// Mirrors Python:
/// ```python
/// if isinstance(value, dict):
///     remove_set = set(transform.get("remove", []))
///     keep_set   = set(transform.get("keep", []))
///     if keep_set:
///         stripped = {k: v for k, v in value.items() if k in keep_set}
///     elif remove_set:
///         stripped = {k: v for k, v in value.items() if k not in remove_set}
///     else:
///         stripped = value
///     if stripped:
///         result[field_name] = stripped
///     else:
///         result.pop(field_name, None)
/// ```
pub struct StripSubfieldsTransform;

impl Transform for StripSubfieldsTransform {
    fn name(&self) -> &'static str {
        "strip_subfields"
    }

    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError> {
        let keep_set: HashSet<&str> = config
            .get("keep")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|s| s.as_str()).collect())
            .unwrap_or_default();

        let remove_set: HashSet<&str> = config
            .get("remove")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|s| s.as_str()).collect())
            .unwrap_or_default();

        if let FieldValue::Struct(fields) = value {
            let retained: Vec<_> = if !keep_set.is_empty() {
                fields
                    .drain(..)
                    .filter(|(sym, _)| {
                        ctx.interner
                            .resolve(*sym)
                            .map(|name| keep_set.contains(name))
                            .unwrap_or(false)
                    })
                    .collect()
            } else if !remove_set.is_empty() {
                fields
                    .drain(..)
                    .filter(|(sym, _)| {
                        ctx.interner
                            .resolve(*sym)
                            .map(|name| !remove_set.contains(name))
                            .unwrap_or(true)
                    })
                    .collect()
            } else {
                // Neither keep nor remove specified — passthrough.
                return Ok(());
            };

            if retained.is_empty() {
                // All subfields were stripped: signal drop via None.
                *value = FieldValue::None;
            } else {
                *value = FieldValue::Struct(retained);
            }
        }
        // Non-Struct values pass through unchanged.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;
    use crate::translator::transforms::TransformCtx;

    fn make_ctx(interner: &StringInterner) -> TransformCtx<'_> {
        TransformCtx { interner }
    }

    /// Builds a FieldValue::Struct from name-value pairs.
    fn make_struct(interner: &StringInterner, pairs: &[(&str, FieldValue)]) -> FieldValue {
        FieldValue::Struct(
            pairs
                .iter()
                .map(|(name, val)| (interner.intern(name), val.clone()))
                .collect(),
        )
    }

    /// Returns the field names present in a Struct.
    fn field_names(interner: &StringInterner, value: &FieldValue) -> Vec<String> {
        match value {
            FieldValue::Struct(fields) => fields
                .iter()
                .filter_map(|(sym, _)| interner.resolve(*sym).map(String::from))
                .collect(),
            _ => vec![],
        }
    }

    /// Usage 1: `remove` list (fo76_to_fo4.yaml line 48).
    /// group_model: strip_subfields remove: [XFLG, ENLT, ENLS, AUUV, MODD]
    ///
    /// Input struct has MODL + XFLG + ENLT; after strip only MODL remains.
    #[test]
    fn remove_drops_named_subfields() {
        let mut interner = StringInterner::new();
        let mut value = make_struct(
            &mut interner,
            &[
                ("MODL", FieldValue::Int(1)),
                ("XFLG", FieldValue::Int(2)),
                ("ENLT", FieldValue::Int(3)),
                ("ENLS", FieldValue::Int(4)),
            ],
        );
        let config = serde_json::json!({
            "remove": ["XFLG", "ENLT", "ENLS", "AUUV", "MODD"]
        });

        let mut ctx = make_ctx(&mut interner);
        StripSubfieldsTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        let names = field_names(&interner, &value);
        assert_eq!(names, vec!["MODL"]);
    }

    /// Usage 2: `keep` list (fo4_to_skyrimse.yaml line 104).
    /// DATA: strip_subfields keep: [value, Value, weight, Weight]
    ///
    /// Input has value + weight + damage; only value and weight survive.
    #[test]
    fn keep_retains_only_named_subfields() {
        let mut interner = StringInterner::new();
        let mut value = make_struct(
            &mut interner,
            &[
                ("value", FieldValue::Int(10)),
                ("weight", FieldValue::Float(1.5)),
                ("damage", FieldValue::Int(5)),
            ],
        );
        let config = serde_json::json!({
            "keep": ["value", "Value", "weight", "Weight"]
        });

        let mut ctx = make_ctx(&mut interner);
        StripSubfieldsTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        let names = field_names(&interner, &value);
        assert_eq!(names, vec!["value", "weight"]);
    }

    /// When all subfields are removed the value becomes None (signals field drop).
    #[test]
    fn all_removed_yields_none() {
        let mut interner = StringInterner::new();
        let mut value = make_struct(
            &mut interner,
            &[("XFLG", FieldValue::Int(1)), ("ENLT", FieldValue::Int(2))],
        );
        let config = serde_json::json!({ "remove": ["XFLG", "ENLT"] });

        let mut ctx = make_ctx(&mut interner);
        StripSubfieldsTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        assert_eq!(value, FieldValue::None);
    }

    /// Non-Struct values are passed through unchanged.
    #[test]
    fn non_struct_passes_through() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Int(42);
        let config = serde_json::json!({ "remove": ["XFLG"] });

        let mut ctx = make_ctx(&mut interner);
        StripSubfieldsTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        assert_eq!(value, FieldValue::Int(42));
    }

    /// Empty config (no keep, no remove) leaves the struct intact.
    #[test]
    fn empty_config_is_passthrough() {
        let mut interner = StringInterner::new();
        let mut value = make_struct(
            &mut interner,
            &[("MODL", FieldValue::Int(1)), ("XFLG", FieldValue::Int(2))],
        );
        let config = serde_json::json!({});

        let mut ctx = make_ctx(&mut interner);
        StripSubfieldsTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        let names = field_names(&interner, &value);
        assert_eq!(names, vec!["MODL", "XFLG"]);
    }
}
