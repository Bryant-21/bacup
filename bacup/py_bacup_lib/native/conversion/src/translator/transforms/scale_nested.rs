//! `scale_nested` transform — scale specific numeric subfields of a Struct value.
//!
//! Port of the Python `scale_nested` branch in `RecordTranslator._apply_transforms`.
//!
//! Config keys (all optional):
//! - `subfields`          : `{subfield_name: factor, ...}`  — multiply each subfield by factor
//! - `clamp_max_subfields`: `{subfield_name: max, ...}`     — clamp each subfield to ≤ max
//! - `remap_source_esm`   : source ESM name string to replace in any String subfields
//! - `remap_target_esm`   : replacement ESM name string

use super::super::super::record::FieldValue;
use super::super::super::sym::StringInterner;
use super::super::maps::YamlValue;
use super::{Transform, TransformCtx, TransformError};

pub struct ScaleNestedTransform;

impl Transform for ScaleNestedTransform {
    fn name(&self) -> &'static str {
        "scale_nested"
    }

    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError> {
        let pairs = match value {
            FieldValue::Struct(p) => p,
            _ => {
                // Python: if not dict → keep value, emit warning.
                // We can't emit warnings through apply() yet, but we keep value unchanged.
                return Ok(());
            }
        };

        // --- scale subfields ---
        if let Some(YamlValue::Object(subfields)) = config.get("subfields") {
            for (key, factor_val) in subfields {
                let factor = match factor_val.as_f64() {
                    Some(f) => f,
                    None => continue,
                };
                let key_sym = ctx.interner.intern(key);
                for (sym, fv) in pairs.iter_mut() {
                    if *sym == key_sym {
                        scale_field_value(fv, factor);
                        break;
                    }
                }
            }
        }

        // --- clamp_max_subfields ---
        if let Some(YamlValue::Object(clamp_map)) = config.get("clamp_max_subfields") {
            for (key, max_val) in clamp_map {
                let maximum = match max_val.as_f64() {
                    Some(f) => f,
                    None => continue,
                };
                let key_sym = ctx.interner.intern(key);
                for (sym, fv) in pairs.iter_mut() {
                    if *sym == key_sym {
                        clamp_field_value(fv, maximum);
                        break;
                    }
                }
            }
        }

        // --- remap FormKey strings (ESM rename) ---
        let remap_src = config
            .get("remap_source_esm")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let remap_tgt = config
            .get("remap_target_esm")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !remap_src.is_empty() && !remap_tgt.is_empty() {
            for (_sym, fv) in pairs.iter_mut() {
                deep_remap_formkey(fv, ctx.interner, remap_src, remap_tgt);
            }
        }

        Ok(())
    }
}

/// Multiply a numeric FieldValue by `factor`. No-op for non-numeric variants.
fn scale_field_value(fv: &mut FieldValue, factor: f64) {
    match fv {
        FieldValue::Float(f) => *f = (*f as f64 * factor) as f32,
        FieldValue::Int(n) => *fv = FieldValue::Float((*n as f64 * factor) as f32),
        FieldValue::Uint(n) => *fv = FieldValue::Float((*n as f64 * factor) as f32),
        _ => {}
    }
}

/// Clamp a numeric FieldValue to `maximum`. No-op for non-numeric variants.
fn clamp_field_value(fv: &mut FieldValue, maximum: f64) {
    let current = match fv {
        FieldValue::Float(f) => *f as f64,
        FieldValue::Int(n) => *n as f64,
        FieldValue::Uint(n) => *n as f64,
        _ => return,
    };
    if current > maximum {
        *fv = FieldValue::Float(maximum as f32);
    }
}

/// Recursively replace `source_esm` with `target_esm` inside any String values.
/// Mirrors `_deep_remap_formkey` from Python (string replacement only; no FormKey struct).
pub fn deep_remap_formkey(
    fv: &mut FieldValue,
    interner: &StringInterner,
    source_esm: &str,
    target_esm: &str,
) {
    match fv {
        FieldValue::String(sym) => {
            if let Some(s) = interner.resolve(*sym) {
                if s.contains(source_esm) {
                    let new_s = s.replace(source_esm, target_esm);
                    *sym = interner.intern(&new_s);
                }
            }
        }
        FieldValue::List(items) => {
            for item in items.iter_mut() {
                deep_remap_formkey(item, interner, source_esm, target_esm);
            }
        }
        FieldValue::Struct(pairs) => {
            for (_key, val) in pairs.iter_mut() {
                deep_remap_formkey(val, interner, source_esm, target_esm);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;
    use crate::translator::transforms::TransformCtx;
    use serde_json::json;

    fn make_struct(interner: &StringInterner, fields: &[(&str, FieldValue)]) -> FieldValue {
        FieldValue::Struct(
            fields
                .iter()
                .map(|(k, v)| (interner.intern(k), v.clone()))
                .collect(),
        )
    }

    fn get_float(fv: &FieldValue, interner: &StringInterner, key: &str) -> Option<f32> {
        if let FieldValue::Struct(pairs) = fv {
            for (sym, val) in pairs {
                if interner.resolve(*sym) == Some(key) {
                    return match val {
                        FieldValue::Float(f) => Some(*f),
                        FieldValue::Int(n) => Some(*n as f32),
                        FieldValue::Uint(n) => Some(*n as f32),
                        _ => None,
                    };
                }
            }
        }
        None
    }

    #[test]
    fn scale_nested_scales_numeric_subfield() {
        // Mirrors WEAP Data with MinRange: 100.0 scaled by 0.12226
        let mut interner = StringInterner::new();
        let mut value = make_struct(
            &mut interner,
            &[
                ("MinRange", FieldValue::Float(100.0)),
                ("MaxRange", FieldValue::Float(200.0)),
            ],
        );
        let config = json!({
            "type": "scale_nested",
            "subfields": {
                "MinRange": 0.12226,
                "MaxRange": 0.08498
            }
        });
        let t = ScaleNestedTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();

        let min = get_float(&value, ctx.interner, "MinRange").unwrap();
        let max = get_float(&value, ctx.interner, "MaxRange").unwrap();
        assert!((min - 12.226_f32).abs() < 1e-3, "MinRange scaled: {min}");
        assert!((max - 16.996_f32).abs() < 1e-3, "MaxRange scaled: {max}");
    }

    #[test]
    fn scale_nested_scales_int_subfield_promoting_to_float() {
        let mut interner = StringInterner::new();
        let mut value = make_struct(&mut interner, &[("MinPowerPerShot", FieldValue::Int(10))]);
        let config = json!({ "subfields": { "MinPowerPerShot": 0.1 } });
        let t = ScaleNestedTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        let v = get_float(&value, ctx.interner, "MinPowerPerShot").unwrap();
        assert!((v - 1.0_f32).abs() < 1e-6, "Got {v}");
    }

    #[test]
    fn scale_nested_clamps_max_subfield() {
        // CritDamageMult: 3.0 clamped to 2.0
        let mut interner = StringInterner::new();
        let mut value = make_struct(&mut interner, &[("CritDamageMult", FieldValue::Float(3.0))]);
        let config = json!({ "clamp_max_subfields": { "CritDamageMult": 2.0 } });
        let t = ScaleNestedTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        let v = get_float(&value, ctx.interner, "CritDamageMult").unwrap();
        assert!((v - 2.0_f32).abs() < 1e-6, "Got {v}");
    }

    #[test]
    fn scale_nested_no_clamp_when_below_max() {
        let mut interner = StringInterner::new();
        let mut value = make_struct(&mut interner, &[("CritDamageMult", FieldValue::Float(1.5))]);
        let config = json!({ "clamp_max_subfields": { "CritDamageMult": 2.0 } });
        let t = ScaleNestedTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        let v = get_float(&value, ctx.interner, "CritDamageMult").unwrap();
        assert!((v - 1.5_f32).abs() < 1e-6, "Got {v}");
    }

    #[test]
    fn scale_nested_non_struct_value_is_left_unchanged() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Int(42);
        let config = json!({ "subfields": { "X": 2.0 } });
        let t = ScaleNestedTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(value, FieldValue::Int(42));
    }

    #[test]
    fn scale_nested_missing_subfield_is_ignored() {
        let mut interner = StringInterner::new();
        let mut value = make_struct(&mut interner, &[("Other", FieldValue::Float(5.0))]);
        let config = json!({ "subfields": { "MinRange": 0.5 } });
        let t = ScaleNestedTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        // Other unchanged
        let v = get_float(&value, ctx.interner, "Other").unwrap();
        assert!((v - 5.0_f32).abs() < 1e-6);
    }

    #[test]
    fn scale_nested_remap_esm_in_string_subfield() {
        let mut interner = StringInterner::new();
        let fk_str = interner.intern("000800@SeventySix.esm");
        let mut value = make_struct(
            &mut interner,
            &[("ProjectileOverride", FieldValue::String(fk_str))],
        );
        let config = json!({
            "remap_source_esm": "SeventySix.esm",
            "remap_target_esm": "Fallout4.esm"
        });
        let t = ScaleNestedTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        if let FieldValue::Struct(pairs) = &value {
            let (_, v) = &pairs[0];
            if let FieldValue::String(sym) = v {
                let s = ctx.interner.resolve(*sym).unwrap();
                assert!(s.contains("Fallout4.esm"), "Got: {s}");
            } else {
                panic!("Expected String");
            }
        }
    }

    #[test]
    fn deep_remap_formkey_replaces_esm_in_nested_struct() {
        let mut interner = StringInterner::new();
        let inner_sym = interner.intern("000100@SeventySix.esm");
        let mut fv = FieldValue::Struct(vec![(
            interner.intern("Inner"),
            FieldValue::String(inner_sym),
        )]);
        deep_remap_formkey(&mut fv, &mut interner, "SeventySix.esm", "Fallout4.esm");
        if let FieldValue::Struct(pairs) = &fv {
            if let FieldValue::String(sym) = &pairs[0].1 {
                assert_eq!(interner.resolve(*sym).unwrap(), "000100@Fallout4.esm");
            }
        }
    }
}
