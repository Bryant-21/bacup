//! Convert FO76 SCOL ONAM parts to FO4-compatible static references.
//!
//! FO76 stores SCOL part references as an 8-byte struct:
//!   - u32 static form id
//!   - u32 material swap form id
//!
//! FO4 SCOL ONAM stores only the static form id. The generic reader keeps the
//! FO76 struct as raw bytes, so this transform narrows the first dword into a
//! typed FormKey before the FormKeyMapper runs.

use smallvec::SmallVec;

use super::{Transform, TransformCtx, TransformError};
use crate::ids::FormKey;
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;

pub struct Fo76ScolStaticTransform;

impl Transform for Fo76ScolStaticTransform {
    fn name(&self) -> &'static str {
        "fo76_scol_static"
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
            .unwrap_or("SeventySix.esm");
        if source_esm.is_empty() {
            return Err(TransformError::BadConfig(
                "fo76_scol_static requires a non-empty source_esm".into(),
            ));
        }

        let FieldValue::Bytes(bytes) = value else {
            return Ok(());
        };

        if bytes.len() < 4 {
            let mut padded = SmallVec::<[u8; 32]>::from_slice(bytes.as_slice());
            padded.resize(4, 0);
            *value = FieldValue::Bytes(padded);
            return Ok(());
        }

        let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if raw == 0 {
            *value = FieldValue::Bytes(SmallVec::from_slice(&0u32.to_le_bytes()));
            return Ok(());
        }

        *value = FieldValue::FormKey(FormKey {
            local: raw & 0x00FF_FFFF,
            plugin: ctx.interner.intern(source_esm),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    #[test]
    fn converts_fo76_eight_byte_part_to_formkey() {
        let mut interner = StringInterner::new();
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        let mut value =
            FieldValue::Bytes(SmallVec::from_slice(&[0x12, 0x58, 0x03, 0x00, 0, 0, 0, 0]));
        let config = serde_json::json!({ "source_esm": "SeventySix.esm" });

        Fo76ScolStaticTransform
            .apply(&mut ctx, &mut value, &config)
            .unwrap();

        match value {
            FieldValue::FormKey(fk) => {
                assert_eq!(fk.local, 0x035812);
                assert_eq!(ctx.interner.resolve(fk.plugin), Some("SeventySix.esm"));
            }
            other => panic!("expected FormKey, got {other:?}"),
        }
    }

    #[test]
    fn zero_part_becomes_four_byte_null_payload() {
        let mut interner = StringInterner::new();
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        let mut value = FieldValue::Bytes(SmallVec::from_slice(&[0, 0, 0, 0, 1, 2, 3, 4]));

        Fo76ScolStaticTransform
            .apply(&mut ctx, &mut value, &serde_json::json!({}))
            .unwrap();

        assert_eq!(
            value,
            FieldValue::Bytes(SmallVec::from_slice(&[0, 0, 0, 0]))
        );
    }
}
