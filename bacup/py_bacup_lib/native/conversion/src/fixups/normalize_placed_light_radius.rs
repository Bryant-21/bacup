//! Fixup: convert FO76 additive placed-light `XRDS` to an FO4 absolute radius.
//!
//! # Why
//! A placed light REFR carries an `XRDS` "Radius" override. In **FO76** that value
//! is an **additive delta** applied to the base `LIGH` radius; in **FO4** `XRDS` is
//! an **absolute** radius override. Byte-copying the FO76 delta straight through
//! makes FO4 read it as an absolute radius — and the deltas are routinely negative
//! (e.g. `-316`), so the placed light ends up with a **negative radius**.
//!
//! A non-positive light radius produces a degenerate bounding volume. When the cell
//! attaches and inserts that light into its spatial partition (the havok broadphase
//! and the audio spatial queries both index it), the bad volume corrupts the
//! structure, so **physics and sound silently stop working for the whole cell** (and
//! the lights render wrong). This was isolated by in-cell bisection — emptying a cell
//! of everything but its placed lights reproduced it; removing the lights fixed it —
//! and confirmed numerically: `base_radius + delta` lands on a sensible positive
//! radius (~690–785) for every sampled light, exactly the additive signature.
//!
//! The existing base-record patch (`fo76_fo4::ensure_light_radius`) only populates
//! the **base** `LIGH.DATA` radius (zero → Value); it never touches the **placed**
//! REFR's `XRDS`, and would not catch a negative value if it did.
//!
//! # How
//! Post-copy pass (all records present, path-independent — a future copy path can't
//! bypass it). For every placed REFR whose `NAME` base is an own-plugin `LIGH` and
//! which carries an `XRDS` override, rewrite
//!   `XRDS = clamp(base_LIGH_radius + xrds_delta, MIN_RADIUS, MAX_RADIUS)`.
//! `base_LIGH_radius` is read the same way `ensure_light_radius` writes it (DATA
//! radius @ +4, else item Value @ +56), so the two passes agree. REFRs without an
//! `XRDS` use the base radius directly and are left untouched.

use rustc_hash::FxHashMap;

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;

/// FO4 `LIGH.DATA` field byte offsets: radius is a `u32` at +4, item Value a `u32`
/// at +56 — the same offsets `fo76_fo4::ensure_light_radius` uses.
const FO4_LIGH_DATA_RADIUS_OFFSET: usize = 4;
const FO4_LIGH_DATA_VALUE_OFFSET: usize = 56;

/// Smallest radius left on a placed light. A non-positive radius is the exact defect
/// this pass removes, so the additive result is clamped here (matches the base
/// patch's `.max(1)`).
const MIN_RADIUS: f32 = 1.0;
const MAX_RADIUS: f32 = 2048.0;

/// FO4 base-light radius from a raw `LIGH.DATA` subrecord: the populated radius
/// (`u32` @ +4) when present, else the item Value (`u32` @ +56, floored at 1) —
/// mirroring `ensure_light_radius`'s own fallback so this pass and the base patch
/// resolve the same base radius.
fn ligh_base_radius_from_data(bytes: &[u8]) -> Option<f32> {
    if bytes.len() < FO4_LIGH_DATA_RADIUS_OFFSET + 4 {
        return None;
    }
    let radius = u32::from_le_bytes(
        bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .try_into()
            .unwrap(),
    );
    if radius > 0 {
        return Some(radius as f32);
    }
    let value = if bytes.len() >= FO4_LIGH_DATA_VALUE_OFFSET + 4 {
        u32::from_le_bytes(
            bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
                .try_into()
                .unwrap(),
        )
    } else {
        0
    };
    Some(value.max(1) as f32)
}

/// Convert an FO76 additive `XRDS` delta to an FO4 absolute radius override.
fn additive_xrds_to_absolute(base_radius: f32, delta: f32) -> f32 {
    (base_radius + delta).clamp(MIN_RADIUS, MAX_RADIUS)
}

/// Rewrite FO76 additive placed-light `XRDS` deltas to FO4 absolute radii. FO76→FO4
/// only; gated by the caller (`ConversionRun::repair_placed_child_refs`).
pub fn normalize_placed_light_radius(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    _config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let interner = mapper.interner;

    let present = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let has = |sig: &str| present.iter().any(|s| s.as_str() == sig);
    if !has("REFR") || !has("LIGH") {
        return Ok(report);
    }

    let own_name = session.target_slot().parsed.plugin_name.clone();
    let own_sym = interner.intern(&own_name);
    let masters = session.target_masters().to_vec();
    let own_load_index = masters.len();

    let ligh_sig = SigCode::from_str("LIGH").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let refr_sig = SigCode::from_str("REFR").map_err(|e| FixupError::SchemaError(e.to_string()))?;

    // Pass A: own-plugin base-light radii, keyed by local object-id.
    let mut radius_of: FxHashMap<u32, f32> = FxHashMap::default();
    for fk in session
        .form_keys_of_sig(ligh_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        if fk.plugin != own_sym {
            continue;
        }
        let Some(data) = session
            .first_subrecord_bytes(&fk, "DATA")
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        else {
            continue;
        };
        if let Some(radius) = ligh_base_radius_from_data(&data) {
            radius_of.insert(fk.local & 0x00FF_FFFF, radius);
        }
    }
    if radius_of.is_empty() {
        return Ok(report);
    }

    // Pass B: rewrite each light placement's additive XRDS to an absolute radius.
    for fk in session
        .form_keys_of_sig(refr_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        let Some(name) = session
            .first_subrecord_bytes(&fk, "NAME")
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        else {
            continue;
        };
        if name.len() < 4 {
            continue;
        }
        let base_raw = u32::from_le_bytes([name[0], name[1], name[2], name[3]]);
        // Only own-plugin LIGH bases carry an FO76 additive radius we can resolve.
        if (base_raw >> 24) as usize != own_load_index {
            continue;
        }
        let Some(&base_radius) = radius_of.get(&(base_raw & 0x00FF_FFFF)) else {
            continue;
        };
        let Some(xrds) = session
            .first_subrecord_bytes(&fk, "XRDS")
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        else {
            continue;
        };
        if xrds.len() < 4 {
            continue;
        }
        let delta = f32::from_le_bytes([xrds[0], xrds[1], xrds[2], xrds[3]]);
        let absolute = additive_xrds_to_absolute(base_radius, delta);
        if absolute.to_le_bytes() == delta.to_le_bytes() {
            continue;
        }
        let changed = session
            .patch_subrecord_bytes(&fk, "XRDS", |buf| {
                if buf.len() < 4 {
                    return false;
                }
                let current = f32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if current.to_le_bytes() != delta.to_le_bytes() {
                    return false;
                }
                buf[0..4].copy_from_slice(&absolute.to_le_bytes());
                true
            })
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if changed {
            report.records_changed = report.records_changed.saturating_add(1);
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 64-byte FO4 `LIGH.DATA` blob with the given radius (@+4) and Value
    /// (@+56).
    fn data_blob(radius: u32, value: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 64];
        bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .copy_from_slice(&radius.to_le_bytes());
        bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
            .copy_from_slice(&value.to_le_bytes());
        bytes
    }

    #[test]
    fn base_radius_prefers_populated_radius() {
        assert_eq!(
            ligh_base_radius_from_data(&data_blob(500, 1100)),
            Some(500.0)
        );
    }

    #[test]
    fn base_radius_falls_back_to_value_when_radius_zero() {
        // The common converted case: FO76 base carries no DATA radius, so
        // ensure_light_radius leaves it 0 and Value is the effective radius.
        assert_eq!(
            ligh_base_radius_from_data(&data_blob(0, 1100)),
            Some(1100.0)
        );
    }

    #[test]
    fn base_radius_floors_at_one() {
        assert_eq!(ligh_base_radius_from_data(&data_blob(0, 0)), Some(1.0));
    }

    #[test]
    fn base_radius_none_when_data_too_short() {
        assert_eq!(ligh_base_radius_from_data(&[0u8; 4]), None);
    }

    #[test]
    fn additive_recovers_intended_radius() {
        // 1100 + (-316.18) = 783.82, the live ground-truth case.
        let r = additive_xrds_to_absolute(1100.0, -316.176_7);
        assert!((r - 783.823_3).abs() < 0.01, "got {r}");
    }

    #[test]
    fn additive_clamps_when_delta_exceeds_base() {
        // 1000 + (-1262.7) would be negative → clamped to the positive floor.
        assert_eq!(additive_xrds_to_absolute(1000.0, -1262.7), MIN_RADIUS);
    }

    #[test]
    fn additive_clamps_large_positive_radius() {
        assert_eq!(additive_xrds_to_absolute(1000.0, 15_534.625), MAX_RADIUS);
    }

    #[test]
    fn additive_handles_positive_delta() {
        assert_eq!(additive_xrds_to_absolute(500.0, 50.0), 550.0);
    }
}
