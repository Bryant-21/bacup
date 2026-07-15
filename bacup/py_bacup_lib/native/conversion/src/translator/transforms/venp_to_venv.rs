//! `venp_to_venv` transform — relayouts an FO76 FACT `VENP` ("Vendor Values")
//! payload into the FO4 FACT `VENV` ("Vendor Values") layout.
//!
//! FO4 `VENV` is `required` on every vendor FACT, but the FO76 source carries
//! the equivalent struct under a different sig (`VENP`) AND a different byte
//! layout, so a bare sig rename would produce a malformed `VENV`. This transform
//! runs after the map renames `VENP → VENV` (the field is still raw FO76 bytes
//! at that point), rewriting the bytes field-by-field into the FO4 layout.
//!
//! Layouts (live schema + live decode of FO76 vendor FACTs LC060_…/844090):
//!
//! | FO76 VENP (`struct:H,H,I,B,B,B` + bytes_6) | off | FO4 VENV (`struct:H,H,H,B,B,B,B,B,B`) | off |
//! |--------------------------------------------|-----|---------------------------------------|-----|
//! | start_hour                       u16       |  0  | start_hour                       u16  |  0  |
//! | end_hour                         u16       |  2  | end_hour                         u16  |  2  |
//! | radius                           u32       |  4  | radius                           u16  |  4  |
//! | buys_stolen_items                u8        |  8  | unknown_u8_3                     u8=0 |  6  |
//! | buy_sell_everything_not_in_list  u8        |  9  | unknown_u8_4                     u8=0 |  7  |
//! | buys_nonstolen_items             u8        | 10  | buys_stolen_items                u8   |  8  |
//! | bytes_6 (trailing, dropped)               | 11  | buy_sell_everything_not_in_list  u8   |  9  |
//! |                                            |     | buys_nonstolen_items             u8   | 10  |
//! |                                            |     | unknown                          u8=0 | 11  |
//!
//! `radius` narrows u32→u16; values seen in source (1200, 500) fit, and any
//! out-of-range value is saturated to u16::MAX rather than silently truncated.
//! The FO4-only bytes (`unknown_u8_3`, `unknown_u8_4`, `unknown`) default to 0,
//! and the FO76 `bytes_6` trailing byte is dropped.
//!
//! YAML usage (fo76_to_fo4.yaml, FACT):
//! ```yaml
//! FACT:
//!   fields:
//!     VENP: VENV
//!   transforms:
//!     VENV:
//!       type: venp_to_venv
//! ```

use super::{Transform, TransformCtx, TransformError};
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;
use smallvec::SmallVec;

/// FO4 VENV is a fixed 12-byte struct (`struct:H,H,H,B,B,B,B,B,B`).
const FO4_VENV_LEN: usize = 12;

/// Rewrites raw FO76 VENP bytes (in-place on a renamed `VENV` field) into the
/// FO4 VENV layout.
pub struct VenpToVenvTransform;

/// Relayout FO76 VENP bytes into FO4 VENV bytes. Returns `None` when the input
/// is too short to be a VENP payload (caller leaves the value untouched).
fn relayout_venp_bytes(src: &[u8]) -> Option<[u8; FO4_VENV_LEN]> {
    // FO76 VENP fixed prefix is 11 bytes (H,H,I,B,B,B); a trailing bytes_6 may
    // follow but is dropped.
    if src.len() < 11 {
        return None;
    }
    let start_hour = u16::from_le_bytes([src[0], src[1]]);
    let end_hour = u16::from_le_bytes([src[2], src[3]]);
    let radius_u32 = u32::from_le_bytes([src[4], src[5], src[6], src[7]]);
    let radius = u16::try_from(radius_u32).unwrap_or(u16::MAX);
    let buys_stolen_items = src[8];
    let buy_sell_everything_not_in_list = src[9];
    let buys_nonstolen_items = src[10];

    let mut out = [0u8; FO4_VENV_LEN];
    out[0..2].copy_from_slice(&start_hour.to_le_bytes());
    out[2..4].copy_from_slice(&end_hour.to_le_bytes());
    out[4..6].copy_from_slice(&radius.to_le_bytes());
    // out[6], out[7] = unknown_u8_3, unknown_u8_4 (default 0).
    out[8] = buys_stolen_items;
    out[9] = buy_sell_everything_not_in_list;
    out[10] = buys_nonstolen_items;
    // out[11] = unknown (default 0).
    Some(out)
}

impl Transform for VenpToVenvTransform {
    fn name(&self) -> &'static str {
        "venp_to_venv"
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
        if let Some(out) = relayout_venp_bytes(src) {
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

    /// Canonical FO76 VENP from live decode of LC060_WhitespringVendor (4124AA):
    /// start_hour=0, end_hour=24, radius=1200, all three bools true, bytes_6=00.
    fn venp_4124aa() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&0u16.to_le_bytes()); // start_hour
        v.extend_from_slice(&24u16.to_le_bytes()); // end_hour
        v.extend_from_slice(&1200u32.to_le_bytes()); // radius (u32)
        v.extend_from_slice(&[1, 1, 1]); // bools
        v.push(0); // bytes_6
        v
    }

    #[test]
    fn relayouts_live_venp_to_fo4_venv() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Bytes(SmallVec::from_vec(venp_4124aa()));
        let mut ctx = make_ctx(&mut interner);
        VenpToVenvTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();

        let FieldValue::Bytes(out) = value else {
            panic!("expected Bytes");
        };
        assert_eq!(out.len(), FO4_VENV_LEN, "FO4 VENV is 12 bytes");
        // start_hour=0, end_hour=24, radius=1200 (u16), unknown 0,0,
        // bools 1,1,1, trailing unknown 0.
        let expected: [u8; 12] = [
            0, 0, // start_hour
            24, 0, // end_hour
            0xB0, 0x04, // radius = 1200
            0, 0, // unknown_u8_3, unknown_u8_4
            1, 1, 1, // buys_stolen / buy_sell_everything / buys_nonstolen
            0, // unknown
        ];
        assert_eq!(out.as_slice(), &expected);
    }

    #[test]
    fn radius_narrows_within_u16() {
        // radius=500 (second source record 844090) fits u16.
        let mut interner = StringInterner::new();
        let mut src = Vec::new();
        src.extend_from_slice(&0u16.to_le_bytes());
        src.extend_from_slice(&24u16.to_le_bytes());
        src.extend_from_slice(&500u32.to_le_bytes());
        src.extend_from_slice(&[1, 0, 1]);
        src.push(0);
        let mut value = FieldValue::Bytes(SmallVec::from_vec(src));
        let mut ctx = make_ctx(&mut interner);
        VenpToVenvTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();
        let FieldValue::Bytes(out) = value else {
            panic!("expected Bytes");
        };
        assert_eq!(u16::from_le_bytes([out[4], out[5]]), 500);
        assert_eq!(&out[8..11], &[1, 0, 1]);
    }

    #[test]
    fn radius_over_u16_saturates() {
        let out = relayout_venp_bytes(&{
            let mut v = Vec::new();
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&0u16.to_le_bytes());
            v.extend_from_slice(&70_000u32.to_le_bytes());
            v.extend_from_slice(&[0, 0, 0]);
            v
        })
        .unwrap();
        assert_eq!(u16::from_le_bytes([out[4], out[5]]), u16::MAX);
    }

    #[test]
    fn accepts_venp_without_trailing_byte() {
        // An 11-byte VENP (no bytes_6) must still relayout.
        let mut v = Vec::new();
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&3u32.to_le_bytes());
        v.extend_from_slice(&[1, 1, 0]);
        let out = relayout_venp_bytes(&v).expect("11-byte VENP relayouts");
        assert_eq!(out.len(), FO4_VENV_LEN);
        assert_eq!(u16::from_le_bytes([out[0], out[1]]), 1);
        assert_eq!(u16::from_le_bytes([out[2], out[3]]), 2);
        assert_eq!(u16::from_le_bytes([out[4], out[5]]), 3);
        assert_eq!(&out[8..11], &[1, 1, 0]);
    }

    #[test]
    fn too_short_input_left_untouched() {
        let mut interner = StringInterner::new();
        let original = SmallVec::<[u8; 32]>::from_slice(&[0, 1, 2, 3]);
        let mut value = FieldValue::Bytes(original.clone());
        let mut ctx = make_ctx(&mut interner);
        VenpToVenvTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();
        assert_eq!(value, FieldValue::Bytes(original));
    }

    #[test]
    fn non_bytes_value_is_a_no_op() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Uint(7);
        let mut ctx = make_ctx(&mut interner);
        VenpToVenvTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();
        assert_eq!(value, FieldValue::Uint(7));
    }
}
