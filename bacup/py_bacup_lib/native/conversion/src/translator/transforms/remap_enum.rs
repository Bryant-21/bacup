//! `remap_enum` transform — maps source integer enum values to target values.
//!
//! Python source: `translator.py` lines 1023-1052.
//!
//! Values in `[0..max_value]` pass through unchanged.
//! Values outside that range are looked up in `mapping`; if absent, `default`
//! is used and a warning is emitted.
//!
//! When the source value is a string (named enum label) the Python code
//! resolves it via `_resolve_enum_string` which requires per-game schema
//! context not yet available here. That branch is stubbed with a
//! TODO — the int path is fully implemented.
//!
//! Config keys:
//! - `max_value` (integer, default 0): highest value that passes through as-is.
//! - `default` (integer, default 0): value used when out-of-range and not in mapping.
//! - `mapping` (object): string-keyed `{int_src: int_dst, ...}` override table.
//! - `enum_ref` (string, optional): schema enum name for string→int resolution.
//!   Stored for future use; currently produces a pass-through on string input.

use super::{Transform, TransformCtx, TransformError};
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;

pub struct RemapEnumTransform;

impl Transform for RemapEnumTransform {
    fn name(&self) -> &'static str {
        "remap_enum"
    }

    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError> {
        let max_value = config
            .get("max_value")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let default = config.get("default").and_then(|v| v.as_i64()).unwrap_or(0);

        // Build the out-of-range mapping from config.
        // Python: `{int(k): int(v) for k, v in transform.get("mapping", {}).items()}`
        let mapping: std::collections::HashMap<i64, i64> = config
            .get("mapping")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| {
                        let ki: i64 = k.parse().ok()?;
                        let vi: i64 = v.as_i64()?;
                        Some((ki, vi))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Attempt to get an integer out of `value`.
        let iv: i64 = match value {
            FieldValue::Int(i) => *i,
            FieldValue::Uint(u) => *u as i64,
            FieldValue::Float(f) => *f as i64,
            FieldValue::String(sym) => {
                // Python: try int(value), if that fails call _resolve_enum_string.
                // For string values: first try parsing as an integer literal.
                if let Some(s) = ctx.interner.resolve(*sym) {
                    if let Ok(n) = s.parse::<i64>() {
                        n
                    } else {
                        // TODO: call EnumLabelIndex::resolve(enum_ref, s)
                        // once game-schema context is wired into TransformCtx.
                        // For now, pass through unchanged (mirrors Python's `continue`
                        // when _resolve_enum_string returns None).
                        return Ok(());
                    }
                } else {
                    return Ok(());
                }
            }
            // Other variants cannot be coerced to int — pass through unchanged.
            _ => return Ok(()),
        };

        *value = if 0 <= iv && iv <= max_value {
            FieldValue::Int(iv)
        } else if let Some(&mapped) = mapping.get(&iv) {
            FieldValue::Int(mapped)
        } else {
            // Unmapped out-of-range value — use default.
            // Python also appends a warning string here; warnings are not yet
            // threaded through TransformCtx (TODO: add warnings vec).
            FieldValue::Int(default)
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    fn make_ctx(interner: &StringInterner) -> TransformCtx<'_> {
        TransformCtx { interner }
    }

    /// YAML usage: KYWD.Type (fo76_to_fo4.yaml lines 1090-1108).
    /// max_value=18, default=0, mapping={19:5, 20:9, 21:0, ...}
    /// Values 0..=18 pass through unchanged.
    #[test]
    fn value_in_range_passes_through() {
        let mut interner = StringInterner::new();
        let cfg = serde_json::json!({
            "max_value": 18,
            "default": 0,
            "mapping": {
                "19": 5,
                "20": 9,
                "21": 0
            }
        });

        let transform = RemapEnumTransform;

        for in_val in [0i64, 1, 10, 18] {
            let mut value = FieldValue::Int(in_val);
            let mut ctx = make_ctx(&mut interner);
            transform.apply(&mut ctx, &mut value, &cfg).unwrap();
            assert_eq!(
                value,
                FieldValue::Int(in_val),
                "expected pass-through for {in_val}"
            );
        }
    }

    /// YAML usage: KYWD.Type — values in the explicit mapping get remapped.
    #[test]
    fn out_of_range_mapped_value_is_remapped() {
        let mut interner = StringInterner::new();
        let cfg = serde_json::json!({
            "max_value": 18,
            "default": 0,
            "mapping": {
                "19": 5,
                "20": 9,
                "27": 13
            }
        });

        let transform = RemapEnumTransform;

        let cases = [(19i64, 5i64), (20, 9), (27, 13)];
        for (input, expected) in cases {
            let mut value = FieldValue::Int(input);
            let mut ctx = make_ctx(&mut interner);
            transform.apply(&mut ctx, &mut value, &cfg).unwrap();
            assert_eq!(value, FieldValue::Int(expected), "remap {input}→{expected}");
        }
    }

    /// YAML usage: KYWD.Type — out-of-range, not in mapping → default=0.
    #[test]
    fn out_of_range_unmapped_falls_back_to_default() {
        let mut interner = StringInterner::new();
        let cfg = serde_json::json!({
            "max_value": 18,
            "default": 0,
            "mapping": { "19": 5 }
        });

        let transform = RemapEnumTransform;
        let mut value = FieldValue::Int(99);
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        assert_eq!(value, FieldValue::Int(0));
    }

    /// String input that parses as integer is handled.
    #[test]
    fn string_integer_literal_is_coerced() {
        let mut interner = StringInterner::new();
        let sym = interner.intern("20");
        let mut value = FieldValue::String(sym);

        let cfg = serde_json::json!({
            "max_value": 18,
            "default": 0,
            "mapping": { "20": 9 }
        });

        let transform = RemapEnumTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        assert_eq!(value, FieldValue::Int(9));
    }

    /// String input that cannot be parsed as int is left unchanged (stub path).
    #[test]
    fn non_integer_string_passes_through_unchanged() {
        let mut interner = StringInterner::new();
        let sym = interner.intern("SomeEnumLabel");
        let mut value = FieldValue::String(sym);

        let cfg = serde_json::json!({
            "enum_ref": "keyword_type_enum",
            "max_value": 18,
            "default": 0,
            "mapping": {}
        });

        let transform = RemapEnumTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        // Value is unchanged — enum_ref resolution not yet implemented.
        if let FieldValue::String(out_sym) = value {
            assert_eq!(ctx.interner.resolve(out_sym), Some("SomeEnumLabel"));
        } else {
            panic!("expected String passthrough");
        }
    }
}
