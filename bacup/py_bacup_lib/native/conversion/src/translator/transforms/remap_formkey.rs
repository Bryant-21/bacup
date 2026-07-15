//! `remap_formkey` transform — rewrites FormKey plugin names embedded as
//! strings inside struct/list field values.
//!
//! Python source: `translator.py` lines 793-807 + helper `_deep_remap_formkey`
//! at lines 219-227.
//!
//! The transform replaces every occurrence of `source_esm` with `target_esm`
//! in all `FieldValue::String` leaves, recursing through `List` and `Struct`
//! variants. `FieldValue::FormKey` typed values are NOT touched here — those
//! are handled by `FormKeyMapper::rewrite_record`.

use super::{Transform, TransformCtx, TransformError};
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;
use std::collections::HashMap;

/// Rewrites FormKey plugin-name substrings embedded in string-typed field values.
///
/// Config keys:
/// - `source_esm` (string): plugin name to replace, e.g. `"SeventySix.esm"`
/// - `target_esm` (string): replacement plugin name, e.g. `"Fallout4.esm"`
pub struct RemapFormkeyTransform;

impl RemapFormkeyTransform {
    /// Recursively replace `source_esm` with `target_esm` in all string leaves.
    fn deep_remap(
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        source_esm: &str,
        target_esm: &str,
    ) {
        match value {
            FieldValue::String(sym) => {
                if let Some(s) = ctx.interner.resolve(*sym) {
                    if s.contains(source_esm) {
                        let replaced = s.replace(source_esm, target_esm);
                        *sym = ctx.interner.intern(&replaced);
                    }
                }
            }
            FieldValue::List(items) => {
                for item in items.iter_mut() {
                    Self::deep_remap(ctx, item, source_esm, target_esm);
                }
            }
            FieldValue::Struct(fields) => {
                for (_key, val) in fields.iter_mut() {
                    Self::deep_remap(ctx, val, source_esm, target_esm);
                }
            }
            // Non-string leaves (Int, Uint, Float, Bool, Bytes, FormKey, None)
            // are passed through unchanged.
            _ => {}
        }
    }

    fn apply_list_filters(ctx: &TransformCtx<'_>, value: &mut FieldValue, config: &YamlValue) {
        let Some(filters) = config.get("list_filters").and_then(|v| v.as_array()) else {
            return;
        };

        for filter in filters {
            let Some(path) = filter.get("path").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(key) = filter.get("key").and_then(|v| v.as_str()) else {
                continue;
            };
            let drop_if_int = filter
                .get("drop_if_int")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let int_to_name_map = parse_int_to_name_map(filter);
            let path_parts: Vec<_> = path.split('.').filter(|part| !part.is_empty()).collect();

            let Some(FieldValue::List(items)) = value_at_path_mut(ctx, value, &path_parts) else {
                continue;
            };

            items.retain_mut(|item| {
                let FieldValue::Struct(fields) = item else {
                    return true;
                };
                let Some((_, property_value)) = fields
                    .iter_mut()
                    .find(|(field_key, _)| ctx.interner.resolve(*field_key) == Some(key))
                else {
                    return true;
                };
                let Some(property_int) = field_value_int(property_value) else {
                    return true;
                };

                if let Some(name) = int_to_name_map.get(&property_int) {
                    *property_value = FieldValue::String(ctx.interner.intern(name));
                    true
                } else {
                    !drop_if_int
                }
            });
        }
    }
}

fn parse_int_to_name_map(filter: &YamlValue) -> HashMap<i64, String> {
    let Some(entries) = filter.get("int_to_name_map").and_then(|v| v.as_object()) else {
        return HashMap::new();
    };

    entries
        .iter()
        .filter_map(|(key, value)| {
            let number = key.parse::<i64>().ok()?;
            let name = value.as_str()?;
            Some((number, name.to_string()))
        })
        .collect()
}

fn value_at_path_mut<'a>(
    ctx: &TransformCtx<'_>,
    mut value: &'a mut FieldValue,
    path: &[&str],
) -> Option<&'a mut FieldValue> {
    for part in path {
        let FieldValue::Struct(fields) = value else {
            return None;
        };
        value = &mut fields
            .iter_mut()
            .find(|(field_key, _)| ctx.interner.resolve(*field_key) == Some(*part))?
            .1;
    }
    Some(value)
}

fn field_value_int(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Int(n) => Some(*n),
        FieldValue::Uint(n) => i64::try_from(*n).ok(),
        _ => None,
    }
}

impl Transform for RemapFormkeyTransform {
    fn name(&self) -> &'static str {
        "remap_formkey"
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

        if source_esm.is_empty() || target_esm.is_empty() {
            return Err(TransformError::BadConfig(
                "remap_formkey requires non-empty source_esm and target_esm".into(),
            ));
        }

        Self::deep_remap(ctx, value, source_esm, target_esm);
        Self::apply_list_filters(ctx, value, config);
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

    fn config(source: &str, target: &str) -> YamlValue {
        serde_json::json!({
            "source_esm": source,
            "target_esm": target,
        })
    }

    /// YAML usage: RGW3 field with source_esm/target_esm (fo76_to_fo4.yaml line 78-80).
    /// A string value containing "SeventySix.esm" should have the plugin name replaced.
    #[test]
    fn remaps_plugin_name_in_plain_string() {
        let mut interner = StringInterner::new();
        let sym = interner.intern("001234@SeventySix.esm");
        let mut value = FieldValue::String(sym);
        let cfg = config("SeventySix.esm", "Fallout4.esm");

        let transform = RemapFormkeyTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        if let FieldValue::String(out_sym) = value {
            assert_eq!(ctx.interner.resolve(out_sym), Some("001234@Fallout4.esm"));
        } else {
            panic!("expected FieldValue::String");
        }
    }

    /// YAML usage: Keywords field (list of FormKey strings) (fo76_to_fo4.yaml line 101-104).
    /// Each string in a List should have the plugin name replaced.
    #[test]
    fn remaps_plugin_name_in_list_of_strings() {
        let mut interner = StringInterner::new();
        let sym1 = interner.intern("000AAA@SeventySix.esm");
        let sym2 = interner.intern("000BBB@SeventySix.esm");
        let mut value = FieldValue::List(vec![FieldValue::String(sym1), FieldValue::String(sym2)]);
        let cfg = config("SeventySix.esm", "Fallout4.esm");

        let transform = RemapFormkeyTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        if let FieldValue::List(items) = value {
            assert_eq!(items.len(), 2);
            if let FieldValue::String(s) = items[0] {
                assert_eq!(ctx.interner.resolve(s), Some("000AAA@Fallout4.esm"));
            } else {
                panic!("expected String at index 0");
            }
            if let FieldValue::String(s) = items[1] {
                assert_eq!(ctx.interner.resolve(s), Some("000BBB@Fallout4.esm"));
            } else {
                panic!("expected String at index 1");
            }
        } else {
            panic!("expected FieldValue::List");
        }
    }

    /// Non-string leaves (Int) inside a Struct should pass through unchanged.
    #[test]
    fn leaves_non_string_values_unchanged() {
        let mut interner = StringInterner::new();
        let key_sym = interner.intern("Count");
        let mut value = FieldValue::Struct(vec![(key_sym, FieldValue::Int(42))]);
        let cfg = config("SeventySix.esm", "Fallout4.esm");

        let transform = RemapFormkeyTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        if let FieldValue::Struct(fields) = value {
            assert_eq!(fields[0].1, FieldValue::Int(42));
        } else {
            panic!("expected FieldValue::Struct");
        }
    }

    #[test]
    fn list_filters_drop_unmapped_int_entries_and_map_known_ints() {
        let mut interner = StringInterner::new();
        let properties_sym = interner.intern("Properties");
        let property_sym = interner.intern("Property");
        let existing_property = interner.intern("Keywords");
        let mut value = FieldValue::Struct(vec![(
            properties_sym,
            FieldValue::List(vec![
                FieldValue::Struct(vec![(property_sym, FieldValue::Uint(15))]),
                FieldValue::Struct(vec![(property_sym, FieldValue::Int(97))]),
                FieldValue::Struct(vec![(property_sym, FieldValue::String(existing_property))]),
            ]),
        )]);
        let cfg = serde_json::json!({
            "source_esm": "SeventySix.esm",
            "target_esm": "Fallout4.esm",
            "list_filters": [{
                "path": "Properties",
                "key": "Property",
                "drop_if_int": true,
                "int_to_name_map": {
                    "97": "AimModelRecoilMaxDegPerShot"
                }
            }]
        });

        let transform = RemapFormkeyTransform;
        let mut ctx = make_ctx(&mut interner);
        transform.apply(&mut ctx, &mut value, &cfg).unwrap();

        let FieldValue::Struct(fields) = value else {
            panic!("expected FieldValue::Struct");
        };
        let FieldValue::List(items) = &fields[0].1 else {
            panic!("expected Properties list");
        };
        assert_eq!(items.len(), 2);
        let FieldValue::Struct(first) = &items[0] else {
            panic!("expected first item struct");
        };
        assert_eq!(
            first.first().and_then(|(_, value)| match value {
                FieldValue::String(sym) => ctx.interner.resolve(*sym),
                _ => None,
            }),
            Some("AimModelRecoilMaxDegPerShot")
        );
        let FieldValue::Struct(second) = &items[1] else {
            panic!("expected second item struct");
        };
        assert_eq!(
            second.first().and_then(|(_, value)| match value {
                FieldValue::String(sym) => ctx.interner.resolve(*sym),
                _ => None,
            }),
            Some("Keywords")
        );
    }

    /// Missing config keys should return BadConfig error.
    #[test]
    fn returns_error_on_empty_source_esm() {
        let mut interner = StringInterner::new();
        let sym = interner.intern("001234@SeventySix.esm");
        let mut value = FieldValue::String(sym);
        let cfg = serde_json::json!({ "source_esm": "", "target_esm": "Fallout4.esm" });

        let transform = RemapFormkeyTransform;
        let mut ctx = make_ctx(&mut interner);
        let result = transform.apply(&mut ctx, &mut value, &cfg);
        assert!(matches!(result, Err(TransformError::BadConfig(_))));
    }
}
