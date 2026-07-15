//! `wrap_in_list` transform — wraps a scalar field value into a list of dicts.
//!
//! Python source: `translator.py` line 945.
//!
//! Config keys:
//! - `target`      — destination field name (defaults to the source field name)
//! - `wrapper_key` — the key inside each dict wrapper (defaults to `"Count"`)
//!
//! Example YAML (fo76_to_fo4.yaml):
//! ```yaml
//! NAM1:
//!   type: wrap_in_list
//!   target: NAM1
//!   wrapper_key: Count
//! ```

use super::super::super::record::FieldValue;
use super::super::maps::YamlValue;
use super::{Transform, TransformCtx, TransformError};

/// Wraps the current `FieldValue` into `List([Struct([(wrapper_key, value)])])`.
///
/// Mirrors the Python:
/// ```python
/// target_key = transform.get("target", field_name)
/// wrapper_key = transform.get("wrapper_key", "Count")
/// result[target_key] = [{wrapper_key: value}]
/// ```
///
/// In Rust the "result dict" is managed by the caller; this transform mutates
/// `value` in-place so the caller can move it to the target field name.
/// The `target` config key is returned via the convenience method
/// `WrapInListTransform::target_field` so the caller can rename the field.
pub struct WrapInListTransform;

impl WrapInListTransform {
    /// Return the target field name from config, or `None` if absent
    /// (caller should treat `None` as "same as source field").
    pub fn target_field<'c>(&self, config: &'c YamlValue) -> Option<&'c str> {
        config.get("target").and_then(|v| v.as_str())
    }

    /// Return the wrapper key from config (defaults to `"Count"`).
    pub fn wrapper_key<'c>(&self, config: &'c YamlValue) -> &'c str {
        config
            .get("wrapper_key")
            .and_then(|v| v.as_str())
            .unwrap_or("Count")
    }
}

impl Transform for WrapInListTransform {
    fn name(&self) -> &'static str {
        "wrap_in_list"
    }

    /// Wraps `value` into `List([Struct([(wrapper_key, value)])])`.
    ///
    /// The `target` config key is advisory — the caller is responsible for
    /// renaming the field if `target != source_field`. This method only
    /// restructures the value itself.
    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError> {
        let wrapper_key_str = self.wrapper_key(config);
        let key_sym = ctx.interner.intern(wrapper_key_str);

        // Take ownership of the current value, replacing it with None temporarily.
        let inner = std::mem::replace(value, FieldValue::None);

        // Build: [{wrapper_key: inner}]
        let wrapped = FieldValue::List(vec![FieldValue::Struct(vec![(key_sym, inner)])]);
        *value = wrapped;

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

    /// YAML usage (fo76_to_fo4.yaml line 1044-1047):
    ///   NAM1:
    ///     type: wrap_in_list
    ///     target: NAM1
    ///     wrapper_key: Count
    ///
    /// Input:  NAM1 = Int(3)   (a count scalar)
    /// Output: NAM1 = List([Struct([("Count", Int(3))])])
    #[test]
    fn wrap_integer_with_count_key() {
        let mut interner = StringInterner::new();
        let t = WrapInListTransform;
        let config = serde_json::json!({
            "target": "NAM1",
            "wrapper_key": "Count"
        });

        let mut value = FieldValue::Int(3);
        let mut ctx = make_ctx(&mut interner);
        t.apply(&mut ctx, &mut value, &config).unwrap();

        let count_sym = interner.intern("Count");
        let expected = FieldValue::List(vec![FieldValue::Struct(vec![(
            count_sym,
            FieldValue::Int(3),
        )])]);
        assert_eq!(value, expected);
    }

    /// Verifies the default `wrapper_key` is "Count" when the config omits it.
    /// Also covers target==field_name (same field, just wrapped).
    #[test]
    fn wrap_uses_default_wrapper_key_when_absent() {
        let mut interner = StringInterner::new();
        let t = WrapInListTransform;
        // Config without wrapper_key — defaults to "Count"
        let config = serde_json::json!({ "target": "NAM1" });

        let mut value = FieldValue::Float(1.5);
        let mut ctx = make_ctx(&mut interner);
        t.apply(&mut ctx, &mut value, &config).unwrap();

        let count_sym = interner.intern("Count");
        let expected = FieldValue::List(vec![FieldValue::Struct(vec![(
            count_sym,
            FieldValue::Float(1.5),
        )])]);
        assert_eq!(value, expected);
    }

    /// Verifies target_field() returns the config value.
    #[test]
    fn target_field_returns_config_value() {
        let t = WrapInListTransform;
        let config = serde_json::json!({ "target": "NAM1", "wrapper_key": "Count" });
        assert_eq!(t.target_field(&config), Some("NAM1"));
    }

    /// Verifies target_field() returns None when absent.
    #[test]
    fn target_field_returns_none_when_absent() {
        let t = WrapInListTransform;
        let config = serde_json::json!({ "wrapper_key": "Count" });
        assert_eq!(t.target_field(&config), None);
    }
}
