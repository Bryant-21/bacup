//! `enum_map` transform — maps string (or list-of-string) enum values to new
//! string (or numeric) values via an explicit lookup table.
//!
//! Python source: `translator.py` lines 756-783.
//!
//! Config keys:
//! - `map` (object): string-keyed lookup table. Keys are the stringified source
//!   values; values are the replacement values (any JSON scalar).
//! - `fallback` (optional): what to do for values not in the map.
//!   - omitted / null → keep original value (same as `__passthrough__`).
//!   - `"__passthrough__"` → keep original value.
//!   - `null` present as YAML `fallback: ~` → use_drop: drop list items.
//!   - any other value → use as the scalar fallback.
//!
//! On scalar input: str(value) is looked up in `map`. On list input: each
//! element is independently looked up. The `use_drop` mode (fallback is
//! JSON `null` and the key `"fallback"` exists) drops unmapped list entries.

use super::{Transform, TransformCtx, TransformError};
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;

pub struct EnumMapTransform;

impl EnumMapTransform {
    /// Stringify a `FieldValue` the same way Python's `str(value)` would for
    /// the enum-map lookup key.
    fn to_str_key(ctx: &TransformCtx<'_>, value: &FieldValue) -> String {
        match value {
            FieldValue::String(sym) => ctx.interner.resolve(*sym).unwrap_or("").to_owned(),
            FieldValue::Int(i) => i.to_string(),
            FieldValue::Uint(u) => u.to_string(),
            FieldValue::Float(f) => f.to_string(),
            FieldValue::Bool(b) => b.to_string(),
            FieldValue::None => "None".into(),
            FieldValue::List(items) => {
                // Mirrors Python's `str([...])` — rarely used in practice but
                // the Python code checks `str_key in mapping` before the list branch.
                let inner: Vec<String> = items.iter().map(|v| Self::to_str_key(ctx, v)).collect();
                format!("[{}]", inner.join(", "))
            }
            FieldValue::Struct(_) | FieldValue::Bytes(_) | FieldValue::FormKey(_) => {
                // No meaningful string representation for enum lookup.
                String::new()
            }
        }
    }

    /// Convert a JSON config value to a `FieldValue`.
    fn json_to_field_value(ctx: &mut TransformCtx<'_>, v: &YamlValue) -> FieldValue {
        match v {
            YamlValue::Null => FieldValue::None,
            YamlValue::Bool(b) => FieldValue::Bool(*b),
            YamlValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    FieldValue::Int(i)
                } else if let Some(u) = n.as_u64() {
                    FieldValue::Uint(u)
                } else if let Some(f) = n.as_f64() {
                    FieldValue::Float(f as f32)
                } else {
                    FieldValue::None
                }
            }
            YamlValue::String(s) => {
                let sym = ctx.interner.intern(s);
                FieldValue::String(sym)
            }
            _ => FieldValue::None,
        }
    }
}

impl Transform for EnumMapTransform {
    fn name(&self) -> &'static str {
        "enum_map"
    }

    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError> {
        let mapping = match config.get("map") {
            Some(YamlValue::Object(m)) => m,
            _ => {
                return Err(TransformError::BadConfig(
                    "enum_map requires a 'map' object".into(),
                ));
            }
        };

        let fallback_entry = config.get("fallback");
        let use_passthrough = fallback_entry
            .and_then(|v| v.as_str())
            .map(|s| s == "__passthrough__")
            .unwrap_or(false);
        // use_drop: fallback key is present and its value is null (YAML `~`)
        let use_drop = fallback_entry.map(|v| v.is_null()).unwrap_or(false)
            && config.get("fallback").is_some();
        let fallback_val: Option<&YamlValue> =
            fallback_entry.filter(|v| !v.is_null() && !use_passthrough);

        // Check str(value) first — matches Python's first branch.
        let str_key = Self::to_str_key(ctx, value);
        if let Some(mapped) = mapping.get(&str_key) {
            *value = Self::json_to_field_value(ctx, mapped);
            return Ok(());
        }

        // List path
        if let FieldValue::List(items) = value {
            let mut mapped_items: Vec<FieldValue> = Vec::with_capacity(items.len());
            for item in items.drain(..) {
                let key = Self::to_str_key(ctx, &item);
                if let Some(mapped) = mapping.get(&key) {
                    mapped_items.push(Self::json_to_field_value(ctx, mapped));
                } else if use_drop {
                    // drop unmapped entries
                } else if use_passthrough {
                    mapped_items.push(item);
                } else {
                    // default: keep original (no-fallback case)
                    mapped_items.push(item);
                }
            }
            *items = mapped_items;
            return Ok(());
        }

        // Scalar fallback
        if use_passthrough {
            // keep as-is
        } else if let Some(fb) = fallback_val {
            *value = Self::json_to_field_value(ctx, fb);
        } else {
            // no fallback key at all — keep original (Python: `mapping.get(str_key, default)` where default=value)
        }

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

    /// YAML usage: AVIF.Flags (fo76_to_fo4.yaml lines 1128-1133).
    /// `UserDefinedDefault` → `unknown_13`, `DamageIsPositive` → `damage_is_positive`,
    /// other values pass through (`fallback: __passthrough__`).
    #[test]
    fn maps_known_string_to_string() {
        let mut interner = StringInterner::new();
        let sym = interner.intern("UserDefinedDefault");
        let mut value = FieldValue::String(sym);

        let cfg = serde_json::json!({
            "map": {
                "UserDefinedDefault": "unknown_13",
                "DamageIsPositive": "damage_is_positive"
            },
            "fallback": "__passthrough__"
        });

        let transform = EnumMapTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        if let FieldValue::String(sym) = value {
            assert_eq!(ctx.interner.resolve(sym), Some("unknown_13"));
        } else {
            panic!("expected FieldValue::String");
        }
    }

    /// YAML usage: AVIF.Flags passthrough for unknown values.
    #[test]
    fn passthrough_unknown_value_unchanged() {
        let mut interner = StringInterner::new();
        let sym = interner.intern("SomeUnknownFlag");
        let mut value = FieldValue::String(sym);

        let cfg = serde_json::json!({
            "map": {
                "UserDefinedDefault": "unknown_13"
            },
            "fallback": "__passthrough__"
        });

        let transform = EnumMapTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        if let FieldValue::String(sym) = value {
            assert_eq!(ctx.interner.resolve(sym), Some("SomeUnknownFlag"));
        } else {
            panic!("expected FieldValue::String (passthrough)");
        }
    }

    /// YAML usage: fnv_to_fo4.yaml PROJ.SoundLevel (lines 240-246).
    /// String → integer mapping with a scalar fallback.
    #[test]
    fn maps_string_to_integer() {
        let mut interner = StringInterner::new();
        let sym = interner.intern("VeryLoud");
        let mut value = FieldValue::String(sym);

        let cfg = serde_json::json!({
            "map": {
                "Normal": 0,
                "Silent": 0,
                "Loud": 1,
                "VeryLoud": 2
            },
            "fallback": 0
        });

        let transform = EnumMapTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        assert_eq!(value, FieldValue::Int(2));
    }

    /// Scalar fallback is used when value is not in map.
    #[test]
    fn uses_scalar_fallback_for_unmapped_value() {
        let mut interner = StringInterner::new();
        let sym = interner.intern("Unknown");
        let mut value = FieldValue::String(sym);

        let cfg = serde_json::json!({
            "map": { "Normal": 0 },
            "fallback": 0
        });

        let transform = EnumMapTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        assert_eq!(value, FieldValue::Int(0));
    }

    /// List of values: mapped and unmapped entries with passthrough.
    #[test]
    fn maps_list_with_passthrough() {
        let mut interner = StringInterner::new();
        let s1 = interner.intern("DamageIsPositive");
        let s2 = interner.intern("OtherFlag");
        let mut value = FieldValue::List(vec![FieldValue::String(s1), FieldValue::String(s2)]);

        let cfg = serde_json::json!({
            "map": { "DamageIsPositive": "damage_is_positive" },
            "fallback": "__passthrough__"
        });

        let transform = EnumMapTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        if let FieldValue::List(items) = value {
            assert_eq!(items.len(), 2);
            if let FieldValue::String(s) = items[0] {
                assert_eq!(ctx.interner.resolve(s), Some("damage_is_positive"));
            } else {
                panic!("expected String at [0]");
            }
            if let FieldValue::String(s) = items[1] {
                assert_eq!(ctx.interner.resolve(s), Some("OtherFlag"));
            } else {
                panic!("expected String at [1] (passthrough)");
            }
        } else {
            panic!("expected List");
        }
    }
}
