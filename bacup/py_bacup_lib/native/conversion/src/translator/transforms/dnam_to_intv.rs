//! `dnam_to_intv` transform — relayouts an FO76 COBJ `DNAM` ("Data") payload
//! into the FO4 COBJ `INTV` ("Data") layout.
//!
//! FO76 stores the crafting "created object count" (how many of the created
//! object one craft yields) plus the UI sort priority in `DNAM`; FO4 stores the
//! same two values under a different sig (`INTV`) AND a different byte layout, so
//! a bare sig rename would produce a malformed `INTV`. This transform runs after
//! the map renames `DNAM → INTV` (the field is still raw FO76 bytes at that
//! point), rewriting the bytes field-by-field into the FO4 layout.
//!
//! Layouts (live schema decode):
//!
//! | FO76 DNAM (`struct:f,H,B,B`)          | off | FO4 INTV (`struct:H,H`)          | off |
//! |---------------------------------------|-----|----------------------------------|-----|
//! | priority_ui_sort_order        f32     |  0  | created_object_count      u16    |  0  |
//! | created_object_count          u16     |  4  | priority                  u16    |  2  |
//! | unknown_u8_2                  u8       |  6  |                                  |     |
//! | unknown_u8_3                  u8       |  7  |                                  |     |
//!
//! `priority` narrows f32→u16: the FO76 UI sort order is rounded to nearest and
//! saturated into `[0, u16::MAX]` (negatives → 0), rather than truncated.
//!
//! YAML usage (fo76_to_fo4.yaml, COBJ):
//! ```yaml
//! COBJ:
//!   fields:
//!     DNAM: INTV
//!   transforms:
//!     INTV:
//!       type: dnam_to_intv
//! ```

use super::{Transform, TransformCtx, TransformError};
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;
use smallvec::SmallVec;

/// FO4 COBJ INTV is a fixed 4-byte struct (`struct:H,H`).
const FO4_INTV_LEN: usize = 4;

/// Rewrites raw FO76 DNAM bytes (in-place on a renamed `INTV` field) into the
/// FO4 INTV layout.
pub struct DnamToIntvTransform;

/// Relayout FO76 DNAM bytes into FO4 INTV bytes. Returns `None` when the input
/// is too short to carry both load-bearing fields (caller leaves it untouched).
fn relayout_dnam_bytes(src: &[u8]) -> Option<[u8; FO4_INTV_LEN]> {
    // Need the f32 priority (0..4) and the u16 count (4..6); the two trailing
    // pad bytes are optional and dropped.
    if src.len() < 6 {
        return None;
    }
    let priority_f = f32::from_le_bytes([src[0], src[1], src[2], src[3]]);
    let created_object_count = u16::from_le_bytes([src[4], src[5]]);
    let priority = if priority_f.is_finite() {
        priority_f.round().clamp(0.0, u16::MAX as f32) as u16
    } else {
        0
    };

    let mut out = [0u8; FO4_INTV_LEN];
    out[0..2].copy_from_slice(&created_object_count.to_le_bytes());
    out[2..4].copy_from_slice(&priority.to_le_bytes());
    Some(out)
}

impl Transform for DnamToIntvTransform {
    fn name(&self) -> &'static str {
        "dnam_to_intv"
    }

    fn apply(
        &self,
        _ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        _config: &YamlValue,
    ) -> Result<(), TransformError> {
        let FieldValue::Bytes(src) = value else {
            // Already decoded/relaid or unexpected shape — leave it for the
            // target normalizer rather than corrupting it.
            return Ok(());
        };
        if let Some(out) = relayout_dnam_bytes(src) {
            *value = FieldValue::Bytes(SmallVec::from_slice(&out));
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

    /// Canonical FO76 COBJ DNAM: priority_ui_sort_order=2.0, count=3, pad 0,0.
    fn dnam_count3() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&2.0f32.to_le_bytes()); // priority_ui_sort_order
        v.extend_from_slice(&3u16.to_le_bytes()); // created_object_count
        v.push(0); // unknown_u8_2
        v.push(0); // unknown_u8_3
        v
    }

    #[test]
    fn relayouts_dnam_to_fo4_intv() {
        let interner = StringInterner::new();
        let mut value = FieldValue::Bytes(SmallVec::from_vec(dnam_count3()));
        let mut ctx = make_ctx(&interner);
        DnamToIntvTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();

        let FieldValue::Bytes(out) = value else {
            panic!("expected Bytes");
        };
        assert_eq!(out.len(), FO4_INTV_LEN, "FO4 INTV is 4 bytes");
        // count=3, priority=2.
        assert_eq!(out.as_slice(), &[3, 0, 2, 0]);
    }

    #[test]
    fn count_is_preserved_verbatim() {
        // 05A371 boxing-glove recipe: count is the load-bearing value.
        let mut src = Vec::new();
        src.extend_from_slice(&0.0f32.to_le_bytes());
        src.extend_from_slice(&5u16.to_le_bytes());
        src.push(0);
        src.push(0);
        let out = relayout_dnam_bytes(&src).expect("full DNAM relayouts");
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), 5);
    }

    #[test]
    fn priority_rounds_and_saturates() {
        let out = relayout_dnam_bytes(&{
            let mut v = Vec::new();
            v.extend_from_slice(&70_000.0f32.to_le_bytes());
            v.extend_from_slice(&1u16.to_le_bytes());
            v
        })
        .unwrap();
        assert_eq!(u16::from_le_bytes([out[2], out[3]]), u16::MAX);
    }

    #[test]
    fn accepts_dnam_without_trailing_pad() {
        // A 6-byte DNAM (no trailing pad) must still relayout.
        let mut v = Vec::new();
        v.extend_from_slice(&1.4f32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        let out = relayout_dnam_bytes(&v).expect("6-byte DNAM relayouts");
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), 2);
        assert_eq!(u16::from_le_bytes([out[2], out[3]]), 1); // 1.4 rounds to 1
    }

    #[test]
    fn too_short_input_left_untouched() {
        let interner = StringInterner::new();
        let original = SmallVec::<[u8; 32]>::from_slice(&[0, 1, 2, 3]);
        let mut value = FieldValue::Bytes(original.clone());
        let mut ctx = make_ctx(&interner);
        DnamToIntvTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();
        assert_eq!(value, FieldValue::Bytes(original));
    }

    #[test]
    fn non_bytes_value_is_a_no_op() {
        let interner = StringInterner::new();
        let mut value = FieldValue::Uint(7);
        let mut ctx = make_ctx(&interner);
        DnamToIntvTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();
        assert_eq!(value, FieldValue::Uint(7));
    }
}
