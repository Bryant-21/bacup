//! `translate_effects` transform — clean effect lists for the target game.
//!
//! Port of `RecordTranslator._translate_effects` (Python line 1724) and the
//! dispatch block at Python line 1143.
//!
//! For each effect dict in the list:
//! - Drop effects with no `BaseEffect` or with a null BaseEffect (`000000:…`).
//! - If `Data` is a raw hex/bytes String, replace it with the `default_data` config dict.
//! - Remap all String FormKeys via source_esm → target_esm.
//! - Recurse into any `Conditions` list (same logic as `translate_conditions` but
//!   without the function-code remapping — conditions inside effects are already
//!   named in FO76 sources, so only the FormKey remap is needed).
//!
//! Config keys:
//! - `source_esm`  : e.g. `"SeventySix.esm"`
//! - `target_esm`  : e.g. `"Fallout4.esm"`
//! - `default_data`: a JSON object used when `Data` is a raw string

use super::super::super::record::FieldValue;
use super::super::super::sym::StringInterner;
use super::super::maps::YamlValue;
use super::scale_nested::deep_remap_formkey;
use super::{Transform, TransformCtx, TransformError};

pub struct TranslateEffectsTransform;

impl Transform for TranslateEffectsTransform {
    fn name(&self) -> &'static str {
        "translate_effects"
    }

    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError> {
        let source_esm = config
            .get("source_esm")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let target_esm = config
            .get("target_esm")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let default_data = config.get("default_data");

        let list = match value {
            FieldValue::List(l) => l,
            // Non-list: pass through unchanged (mirrors Python behaviour)
            _ => return Ok(()),
        };

        let do_remap = !source_esm.is_empty() && !target_esm.is_empty();

        let mut cleaned: Vec<FieldValue> = Vec::with_capacity(list.len());
        for eff in list.drain(..) {
            match eff {
                FieldValue::Struct(mut pairs) => {
                    // Check BaseEffect — drop if missing or null-formkey.
                    let base_key = ctx.interner.intern("BaseEffect");
                    let base_present = pairs.iter().any(|(sym, fv)| {
                        if *sym != base_key {
                            return false;
                        }
                        match fv {
                            FieldValue::None => false,
                            FieldValue::String(s) => {
                                let resolved = ctx.interner.resolve(*s).unwrap_or("");
                                // null FormKey: "000000@<plugin>" (Rust) or "<plugin>:000000" (Python)
                                if resolved.is_empty() {
                                    return false;
                                }
                                if resolved.starts_with("000000@") {
                                    return false;
                                }
                                if let Some((_, hex)) = resolved.rsplit_once(':') {
                                    if hex == "000000" {
                                        return false;
                                    }
                                }
                                true
                            }
                            _ => true,
                        }
                    });
                    if !base_present {
                        // drop this effect
                        continue;
                    }

                    // Replace raw-hex Data with default_data.
                    let data_key = ctx.interner.intern("Data");
                    if let Some(default) = default_data {
                        if let Some((_, data_fv)) =
                            pairs.iter_mut().find(|(sym, _)| *sym == data_key)
                        {
                            if matches!(data_fv, FieldValue::String(_)) {
                                // Replace with a Struct built from the default_data JSON object.
                                *data_fv = json_obj_to_struct(default, ctx.interner);
                            }
                        }
                    }

                    // Recurse into Conditions list — remap FormKeys only (no fn-code remapping here).
                    let cond_key = ctx.interner.intern("Conditions");
                    if do_remap {
                        if let Some((_, cond_fv)) =
                            pairs.iter_mut().find(|(sym, _)| *sym == cond_key)
                        {
                            deep_remap_formkey(cond_fv, ctx.interner, source_esm, target_esm);
                        }
                    }

                    let mut eff_out = FieldValue::Struct(pairs);

                    // Remap all FormKey strings in the whole effect.
                    if do_remap {
                        deep_remap_formkey(&mut eff_out, ctx.interner, source_esm, target_esm);
                    }

                    cleaned.push(eff_out);
                }
                // Non-struct entries pass through unchanged (mirrors Python).
                other => cleaned.push(other),
            }
        }

        *list = cleaned;
        Ok(())
    }
}

/// Build a `FieldValue::Struct` from a JSON object (config blob).
/// String values are interned; numbers become Float/Int; booleans become Bool.
fn json_obj_to_struct(obj: &YamlValue, interner: &StringInterner) -> FieldValue {
    let pairs = match obj.as_object() {
        Some(m) => m
            .iter()
            .map(|(k, v)| {
                let sym = interner.intern(k);
                let fv = json_val_to_field(v, interner);
                (sym, fv)
            })
            .collect(),
        None => return FieldValue::None,
    };
    FieldValue::Struct(pairs)
}

fn json_val_to_field(v: &YamlValue, interner: &StringInterner) -> FieldValue {
    match v {
        YamlValue::Null => FieldValue::None,
        YamlValue::Bool(b) => FieldValue::Bool(*b),
        YamlValue::Number(n) => {
            if let Some(f) = n.as_f64() {
                FieldValue::Float(f as f32)
            } else {
                FieldValue::None
            }
        }
        YamlValue::String(s) => FieldValue::String(interner.intern(s)),
        YamlValue::Array(arr) => {
            FieldValue::List(arr.iter().map(|i| json_val_to_field(i, interner)).collect())
        }
        YamlValue::Object(_) => json_obj_to_struct(v, interner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;
    use crate::translator::transforms::TransformCtx;
    use serde_json::json;

    fn make_effect(
        interner: &StringInterner,
        base_effect: Option<&str>,
        data: Option<FieldValue>,
        conditions: Option<FieldValue>,
    ) -> FieldValue {
        let mut pairs: Vec<(crate::sym::Sym, FieldValue)> = Vec::new();
        if let Some(b) = base_effect {
            let sym = interner.intern("BaseEffect");
            pairs.push((sym, FieldValue::String(interner.intern(b))));
        }
        if let Some(d) = data {
            let sym = interner.intern("Data");
            pairs.push((sym, d));
        }
        if let Some(c) = conditions {
            let sym = interner.intern("Conditions");
            pairs.push((sym, c));
        }
        FieldValue::Struct(pairs)
    }

    fn make_effect_list(items: Vec<FieldValue>) -> FieldValue {
        FieldValue::List(items)
    }

    fn get_list(fv: &FieldValue) -> &Vec<FieldValue> {
        match fv {
            FieldValue::List(l) => l,
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn translate_effects_drops_effect_with_null_base() {
        // BaseEffect starting with "000000:" → drop
        let mut interner = StringInterner::new();
        let eff = make_effect(&mut interner, Some("000000@SeventySix.esm"), None, None);
        let mut value = make_effect_list(vec![eff]);
        let config = json!({ "source_esm": "SeventySix.esm", "target_esm": "Fallout4.esm" });
        let t = TranslateEffectsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert!(
            get_list(&value).is_empty(),
            "null base effect should be dropped"
        );
    }

    #[test]
    fn translate_effects_drops_effect_without_base() {
        // No BaseEffect at all → drop
        let mut interner = StringInterner::new();
        let eff = make_effect(&mut interner, None, Some(FieldValue::Float(1.0)), None);
        let mut value = make_effect_list(vec![eff]);
        let config = json!({});
        let t = TranslateEffectsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert!(get_list(&value).is_empty());
    }

    #[test]
    fn translate_effects_keeps_valid_effect() {
        let mut interner = StringInterner::new();
        let eff = make_effect(
            &mut interner,
            Some("012345@SeventySix.esm"),
            Some(FieldValue::Struct(vec![])),
            None,
        );
        let mut value = make_effect_list(vec![eff]);
        let config = json!({ "source_esm": "SeventySix.esm", "target_esm": "Fallout4.esm" });
        let t = TranslateEffectsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(get_list(&value).len(), 1);
    }

    #[test]
    fn translate_effects_replaces_raw_hex_data_with_default() {
        // Data is a raw hex String → replaced with default_data struct.
        let mut interner = StringInterner::new();
        let hex_str = interner.intern("0x0000000000000000");
        let eff = make_effect(
            &mut interner,
            Some("AABBCC@SeventySix.esm"),
            Some(FieldValue::String(hex_str)),
            None,
        );
        let mut value = make_effect_list(vec![eff]);
        let config = json!({
            "default_data": { "Magnitude": 0.0 }
        });
        let t = TranslateEffectsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();

        let effects = get_list(&value);
        assert_eq!(effects.len(), 1);
        if let FieldValue::Struct(pairs) = &effects[0] {
            let data_sym = ctx.interner.intern("Data");
            let data = pairs.iter().find(|(s, _)| *s == data_sym).map(|(_, v)| v);
            assert!(
                matches!(data, Some(FieldValue::Struct(_))),
                "Data should be a Struct after replacement"
            );
        }
    }

    #[test]
    fn translate_effects_remaps_esm_in_base_effect() {
        let mut interner = StringInterner::new();
        let eff = make_effect(&mut interner, Some("001122@SeventySix.esm"), None, None);
        let mut value = make_effect_list(vec![eff]);
        let config = json!({ "source_esm": "SeventySix.esm", "target_esm": "Fallout4.esm" });
        let t = TranslateEffectsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();

        if let FieldValue::Struct(pairs) = &get_list(&value)[0] {
            let base_sym = ctx.interner.intern("BaseEffect");
            let base = pairs.iter().find(|(s, _)| *s == base_sym).map(|(_, v)| v);
            if let Some(FieldValue::String(sym)) = base {
                let s = ctx.interner.resolve(*sym).unwrap();
                assert!(s.contains("Fallout4.esm"), "Expected Fallout4.esm in: {s}");
            }
        }
    }

    #[test]
    fn translate_effects_non_list_passes_through() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Int(99);
        let config = json!({});
        let t = TranslateEffectsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(value, FieldValue::Int(99));
    }

    #[test]
    fn translate_effects_multiple_effects_partial_drop() {
        // 2 effects: first valid, second null-base → only first survives
        let mut interner = StringInterner::new();
        let valid = make_effect(&mut interner, Some("AABB@SeventySix.esm"), None, None);
        let null_base = make_effect(&mut interner, Some("000000@SeventySix.esm"), None, None);
        let mut value = make_effect_list(vec![valid, null_base]);
        let config = json!({ "source_esm": "SeventySix.esm", "target_esm": "Fallout4.esm" });
        let t = TranslateEffectsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(get_list(&value).len(), 1);
    }

    #[test]
    fn translate_effects_non_struct_entry_passes_through() {
        // A non-struct item in the list passes through (Python mirrors this).
        let mut interner = StringInterner::new();
        let mut value = FieldValue::List(vec![FieldValue::Int(42)]);
        let config = json!({});
        let t = TranslateEffectsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(get_list(&value), &[FieldValue::Int(42)]);
    }
}
