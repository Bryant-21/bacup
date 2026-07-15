//! `trim_languages` transform — filters localized-string entries by language.
//!
//! Python source: `translator.py` line 824.
//!
//! Config keys:
//! - `keep`   — list of language names to retain (required; empty list keeps nothing)
//! - `target` — destination field name (defaults to the source field name, advisory)
//!
//! Operates on `FieldValue::Struct` values that contain a `"Values"` subfield
//! holding a `FieldValue::List` of `FieldValue::Struct` entries, each with a
//! `"Language"` subfield. Entries whose `Language` is not in `keep` are dropped.
//!
//! Non-Struct values or Structs without a `"Values"` field are passed through.
//!
//! Example YAML usages:
//!
//! Usage 1 — standard (fo76_to_fo4.yaml line 34):
//! ```yaml
//! FULL:
//!   type: trim_languages
//!   keep: [English, Chinese, German, French, Spanish, Italian, Japanese, Polish, ...]
//! ```
//!
//! Usage 2 — with `target` rename (fo76_to_fo4.yaml line 1153):
//! ```yaml
//! FULL:
//!   type: trim_languages
//!   target: Name
//!   keep: [English, Chinese, German, French, Spanish, Italian]
//! ```

use std::collections::HashSet;

use super::super::super::record::FieldValue;
use super::super::maps::YamlValue;
use super::{Transform, TransformCtx, TransformError};

/// Filters language entries inside a localized-string Struct.
///
/// Mirrors Python:
/// ```python
/// keep = set(transform.get("keep", []))
/// if isinstance(value, dict) and "Values" in value:
///     trimmed = {**value}
///     trimmed["Values"] = [v for v in value["Values"] if v.get("Language") in keep]
///     result[target_key] = trimmed
/// else:
///     result[target_key] = value
/// ```
///
/// In Rust the "result dict" renaming is advisory — the caller handles field
/// renaming based on `TrimLanguagesTransform::target_field`. This method
/// mutates `value` in-place: it filters the `Values` list and replaces it.
pub struct TrimLanguagesTransform;

impl TrimLanguagesTransform {
    /// Return the target field name from config, or `None` if absent.
    pub fn target_field<'c>(&self, config: &'c YamlValue) -> Option<&'c str> {
        config.get("target").and_then(|v| v.as_str())
    }
}

impl Transform for TrimLanguagesTransform {
    fn name(&self) -> &'static str {
        "trim_languages"
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

        if let FieldValue::Struct(outer_fields) = value {
            // Find the "Values" field index.
            let values_idx = outer_fields
                .iter()
                .position(|(sym, _)| ctx.interner.resolve(*sym) == Some("Values"));

            if let Some(idx) = values_idx {
                // Extract and filter the Values list.
                let filtered = if let FieldValue::List(entries) = &outer_fields[idx].1 {
                    entries
                        .iter()
                        .filter(|entry| {
                            // Keep entry if its "Language" field value is in keep_set.
                            if let FieldValue::Struct(entry_fields) = entry {
                                entry_fields.iter().any(|(sym, val)| {
                                    if ctx.interner.resolve(*sym) == Some("Language") {
                                        if let FieldValue::String(lang_sym) = val {
                                            ctx.interner
                                                .resolve(*lang_sym)
                                                .map(|lang| keep_set.contains(lang))
                                                .unwrap_or(false)
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    }
                                })
                            } else {
                                false
                            }
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    // "Values" exists but isn't a List — leave it alone.
                    return Ok(());
                };

                outer_fields[idx].1 = FieldValue::List(filtered);
            }
            // If no "Values" field, pass through unchanged.
        }
        // Non-Struct values pass through unchanged.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::FieldValue;
    use crate::sym::StringInterner;
    use crate::translator::transforms::TransformCtx;

    fn make_ctx(interner: &StringInterner) -> TransformCtx<'_> {
        TransformCtx { interner }
    }

    /// Build a language entry Struct: {Language: <lang>, String: <text>}
    fn lang_entry(interner: &StringInterner, lang: &str, text: &str) -> FieldValue {
        let lang_sym = interner.intern(lang);
        let lang_key = interner.intern("Language");
        let text_sym = interner.intern(text);
        let str_key = interner.intern("String");
        FieldValue::Struct(vec![
            (lang_key, FieldValue::String(lang_sym)),
            (str_key, FieldValue::String(text_sym)),
        ])
    }

    /// Build an LSTRING-style outer Struct: {Values: [entries...]}
    fn lstring_value(interner: &StringInterner, entries: Vec<FieldValue>) -> FieldValue {
        let values_key = interner.intern("Values");
        FieldValue::Struct(vec![(values_key, FieldValue::List(entries))])
    }

    /// Count entries in the Values list of a Struct.
    fn values_len(value: &FieldValue) -> usize {
        if let FieldValue::Struct(outer) = value {
            if let Some((_, FieldValue::List(entries))) = outer.first() {
                return entries.len();
            }
        }
        0
    }

    /// Extract language names from surviving entries.
    fn surviving_langs(interner: &StringInterner, value: &FieldValue) -> Vec<String> {
        if let FieldValue::Struct(outer) = value {
            if let Some((_, FieldValue::List(entries))) = outer.first() {
                return entries
                    .iter()
                    .filter_map(|e| {
                        if let FieldValue::Struct(fields) = e {
                            fields.iter().find_map(|(sym, val)| {
                                if interner.resolve(*sym) == Some("Language") {
                                    if let FieldValue::String(s) = val {
                                        interner.resolve(*s).map(String::from)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        }
                    })
                    .collect();
            }
        }
        vec![]
    }

    /// Usage 1: standard trim (fo76_to_fo4.yaml line 34).
    /// FULL: trim_languages keep: [English, Chinese, German, ...]
    ///
    /// Input has English, Russian, Polish, Korean — only English survives.
    #[test]
    fn trim_keeps_listed_languages() {
        let mut interner = StringInterner::new();
        let entries = vec![
            lang_entry(&mut interner, "English", "Hello"),
            lang_entry(&mut interner, "Russian", "Привет"),
            lang_entry(&mut interner, "Korean", "안녕"),
        ];
        let mut value = lstring_value(&mut interner, entries);

        let config = serde_json::json!({
            "keep": ["English", "Chinese", "German", "French", "Spanish", "Italian", "Japanese", "Polish"]
        });

        let mut ctx = make_ctx(&mut interner);
        TrimLanguagesTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        assert_eq!(values_len(&value), 1);
        assert_eq!(surviving_langs(&interner, &value), vec!["English"]);
    }

    /// Usage 2: trim with target rename config (fo76_to_fo4.yaml line 1153).
    /// The target rename is advisory; this test confirms the value is trimmed
    /// correctly and target_field() returns the config value.
    #[test]
    fn trim_with_target_config() {
        let mut interner = StringInterner::new();
        let entries = vec![
            lang_entry(&mut interner, "English", "Weapon"),
            lang_entry(&mut interner, "Spanish", "Arma"),
            lang_entry(&mut interner, "Russian", "Оружие"),
        ];
        let mut value = lstring_value(&mut interner, entries);

        let config = serde_json::json!({
            "target": "Name",
            "keep": ["English", "Chinese", "German", "French", "Spanish", "Italian"]
        });

        let t = TrimLanguagesTransform;
        let mut ctx = make_ctx(&mut interner);
        t.apply(&mut ctx, &mut value, &config).unwrap();

        // English and Spanish are in keep; Russian is not.
        assert_eq!(values_len(&value), 2);
        let langs = surviving_langs(&interner, &value);
        assert!(langs.contains(&"English".to_string()));
        assert!(langs.contains(&"Spanish".to_string()));
        assert!(!langs.contains(&"Russian".to_string()));

        // target_field() must return "Name" for the caller to rename.
        assert_eq!(t.target_field(&config), Some("Name"));
    }

    /// Non-Struct values pass through unchanged.
    #[test]
    fn non_struct_passes_through() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Int(42);
        let config = serde_json::json!({ "keep": ["English"] });

        let mut ctx = make_ctx(&mut interner);
        TrimLanguagesTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        assert_eq!(value, FieldValue::Int(42));
    }

    /// A Struct without a "Values" field passes through unchanged.
    #[test]
    fn struct_without_values_passes_through() {
        let mut interner = StringInterner::new();
        let key = interner.intern("Other");
        let mut value = FieldValue::Struct(vec![(key, FieldValue::Int(1))]);
        let config = serde_json::json!({ "keep": ["English"] });

        let mut ctx = make_ctx(&mut interner);
        TrimLanguagesTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        // Still a Struct with one field.
        if let FieldValue::Struct(fields) = &value {
            assert_eq!(fields.len(), 1);
        } else {
            panic!("expected Struct");
        }
    }
}
