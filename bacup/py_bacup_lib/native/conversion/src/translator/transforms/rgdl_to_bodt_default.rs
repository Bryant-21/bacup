//! `rgdl_to_bodt_default` transform — converts an RGDL DATA field to a
//! minimal BODT default payload.
//!
//! Python source: `translator.py` lines 477-482.
//!
//! The transform ignores the input value entirely and always produces a fixed
//! `FieldValue::Struct` representing the default BODT body template:
//!
//! ```json
//! { "FirstPersonFlags": ["33BODY"], "Flags": [] }
//! ```
//!
//! This mirrors the Python:
//! ```python
//! def transform_rgdl_to_bodt_default(value, ctx=None):
//!     return {"BODT": {"FirstPersonFlags": ["33BODY"], "Flags": []}}
//! ```
//!
//! Note: the Python returns a dict with a top-level "BODT" key; in the Rust
//! pipeline the field target key is set by the YAML map (`DATA → BODT`),
//! so the transform only emits the inner struct (without the outer "BODT"
//! wrapper). The caller is responsible for placing the result in the correct
//! target field.
//!
//! YAML usage (fnv_to_fo4.yaml):
//! ```yaml
//! RGDL:
//!   transforms:
//!     DATA:
//!       type: rgdl_to_bodt_default
//! ```

use super::{Transform, TransformCtx, TransformError};
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;

/// Replaces the RGDL DATA field with a minimal BODT default struct.
pub struct RgdlToBodtDefaultTransform;

impl Transform for RgdlToBodtDefaultTransform {
    fn name(&self) -> &'static str {
        "rgdl_to_bodt_default"
    }

    /// Produces `Struct([("FirstPersonFlags", List(["33BODY"])), ("Flags", List([]))])`.
    ///
    /// The input value is discarded; the output is always the fixed BODT
    /// default regardless of what DATA contained.
    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        _config: &YamlValue,
    ) -> Result<(), TransformError> {
        let fpf_sym = ctx.interner.intern("FirstPersonFlags");
        let flags_sym = ctx.interner.intern("Flags");
        let body_sym = ctx.interner.intern("33BODY");

        *value = FieldValue::Struct(vec![
            (
                fpf_sym,
                FieldValue::List(vec![FieldValue::String(body_sym)]),
            ),
            (flags_sym, FieldValue::List(vec![])),
        ]);
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

    // -------------------------------------------------------------------------
    // Core behaviour: always emits the fixed BODT default struct
    // -------------------------------------------------------------------------

    #[test]
    fn produces_bodt_default_from_any_input() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Int(0); // arbitrary DATA bytes
        let config = serde_json::Value::Null;
        let mut ctx = make_ctx(&mut interner);

        RgdlToBodtDefaultTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        let fpf_sym = interner.intern("FirstPersonFlags");
        let flags_sym = interner.intern("Flags");
        let body_sym = interner.intern("33BODY");

        let expected = FieldValue::Struct(vec![
            (
                fpf_sym,
                FieldValue::List(vec![FieldValue::String(body_sym)]),
            ),
            (flags_sym, FieldValue::List(vec![])),
        ]);
        assert_eq!(value, expected);
    }

    #[test]
    fn discards_input_bytes_value() {
        use smallvec::SmallVec;
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Bytes(SmallVec::from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]));
        let config = serde_json::Value::Null;
        let mut ctx = make_ctx(&mut interner);

        RgdlToBodtDefaultTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        // Must be a Struct, not Bytes.
        assert!(matches!(value, FieldValue::Struct(_)));
    }

    #[test]
    fn discards_null_value_input() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::None;
        let config = serde_json::Value::Null;
        let mut ctx = make_ctx(&mut interner);

        RgdlToBodtDefaultTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        assert!(matches!(value, FieldValue::Struct(_)));
    }

    #[test]
    fn first_person_flags_contains_33body() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::None;
        let config = serde_json::Value::Null;
        let mut ctx = make_ctx(&mut interner);

        RgdlToBodtDefaultTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        let body_sym = interner.intern("33BODY");
        if let FieldValue::Struct(ref fields) = value {
            let fpf_sym = interner.intern("FirstPersonFlags");
            let fpf_value = fields.iter().find(|(k, _)| *k == fpf_sym).map(|(_, v)| v);
            assert!(
                matches!(fpf_value, Some(FieldValue::List(v)) if v == &[FieldValue::String(body_sym)])
            );
        } else {
            panic!("expected FieldValue::Struct");
        }
    }

    #[test]
    fn flags_list_is_empty() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::None;
        let config = serde_json::Value::Null;
        let mut ctx = make_ctx(&mut interner);

        RgdlToBodtDefaultTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        if let FieldValue::Struct(ref fields) = value {
            let flags_sym = interner.intern("Flags");
            let flags_value = fields.iter().find(|(k, _)| *k == flags_sym).map(|(_, v)| v);
            assert!(matches!(flags_value, Some(FieldValue::List(v)) if v.is_empty()));
        } else {
            panic!("expected FieldValue::Struct");
        }
    }
}
