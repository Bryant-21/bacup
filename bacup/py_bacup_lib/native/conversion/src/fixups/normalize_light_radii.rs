//! Fixup: convert FO76 physically-based light intensity to an FO4 falloff radius,
//! for both `LIGH` bases and the placed `REFR` `XRDS` overrides that scale them.
//!
//! FO76 lights are physically based: brightness is **lumens** in `LIGH.DATA` @ +56
//! (FO4's item `Value` slot), falloff is `1/d²`, and `DATA.Radius` (@ +4) is only a
//! cull distance. `DATA` @ +52 (FO4's `God Rays - Near Clip`) holds the colour
//! temperature in Kelvin, already baked into the RGB. FO4 has no intensity field:
//! radius *is* the falloff scale, so a byte-copied FO76 cull distance keeps every
//! lamp bright to its edge, and the overlapping additive volumes blow cells out to
//! white (`SugarGrove01`). Byte-copied `SeventySix.esm` lights run p50 500 / p90 1000
//! (22% at or above 1000); `Fallout4.esm` lights sit at p50 256 / p90 380 (max 1024),
//! 69% at exactly 256.
//!
//! A `1/d²` source falls below a fixed illuminance at `sqrt(I / E_min)`, so the FO4
//! radius scales with `sqrt(lumens)`. The anchor is a 1000 lm FO76 light reading
//! correctly at radius 256 in the Creation Kit (also `Fallout4.esm`'s modal radius).
//! Capping by the FO76 radius keeps deliberately tight culls:
//!
//! ```text
//! radius = clamp(min(256 * sqrt(lumens / 1000), fo76_radius), 1, 2048)
//! ```
//!
//! The result never exceeds the translator hook's radius, so the hook's ceilings
//! (sunlight 256, cage-bulb gobo 256, shadow caster 1024, synthetic 2048) still hold.
//!
//! FO76 `XRDS` is an additive delta on the base radius; FO4 `XRDS` is absolute. A
//! copied delta reads as a routinely negative radius whose degenerate bounding volume
//! corrupts the cell's spatial partition and silently kills physics and sound for the
//! whole cell. Overrides keep the instance's relative reach:
//!
//! ```text
//! XRDS = clamp(new_base_radius * (fo76_radius + delta) / fo76_radius, 1, 2048)
//! ```
//!
//! Runs post-copy with all records present, so no copy path bypasses it. Every base
//! radius is read before any is rewritten, so overrides use pre-rescale values. FO76
//! lumens are then cleared, since FO4 reads that slot as an item Value.

use rustc_hash::FxHashMap;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::PluginSession;

/// FO4 `LIGH.DATA` byte offsets. Radius is a `u32` @ +4; @ +56 is FO4's item
/// `Value`, which at this point still carries FO76's lumens.
const FO4_LIGH_DATA_RADIUS_OFFSET: usize = 4;
const FO76_LIGH_DATA_LUMENS_OFFSET: usize = 56;

/// Illuminance-threshold anchor: an FO76 light of `REFERENCE_LUMENS` converts to
/// `RADIUS_AT_REFERENCE_LUMENS`. 1000 lm → 256 comes from the Creation Kit check on
/// `SugarGrove01` and coincides with `Fallout4.esm`'s modal light radius.
const REFERENCE_LUMENS: f32 = 1000.0;
const RADIUS_AT_REFERENCE_LUMENS: f32 = 256.0;

/// A non-positive radius is a degenerate bounding volume, so results are floored
/// at 1; 2048 matches the translator hook's synthetic ceiling.
const MIN_RADIUS: f32 = 1.0;
const MAX_RADIUS: f32 = 2048.0;

/// Pre-rescale state of one own-plugin base light.
#[derive(Clone, Copy)]
struct BaseLight {
    /// Radius as the translator hook left it — FO76's authored cull distance.
    fo76_radius: u32,
    /// The FO4 falloff radius derived from this light's lumens.
    fo4_radius: u32,
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|slice| u32::from_le_bytes(slice.try_into().unwrap()))
}

/// FO4 falloff radius for an FO76 light, capped by its authored cull distance.
/// A light with no lumens keeps the authored radius (nothing better to go on).
fn fo4_radius_from_lumens(lumens: u32, fo76_radius: u32) -> u32 {
    if lumens == 0 {
        return fo76_radius.clamp(MIN_RADIUS as u32, MAX_RADIUS as u32);
    }
    let scaled = RADIUS_AT_REFERENCE_LUMENS * (lumens as f32 / REFERENCE_LUMENS).sqrt();
    let capped = if fo76_radius > 0 {
        scaled.min(fo76_radius as f32)
    } else {
        scaled
    };
    // `as u32` saturates and maps NaN to 0; the clamp then lifts it to the floor.
    (capped.round() as u32).clamp(MIN_RADIUS as u32, MAX_RADIUS as u32)
}

/// Absolute FO4 `XRDS` for a placed light: the rederived base radius scaled by the
/// instance's relative FO76 reach.
fn placed_radius_from_delta(base: BaseLight, delta: f32) -> f32 {
    if base.fo76_radius == 0 {
        return (base.fo4_radius as f32).clamp(MIN_RADIUS, MAX_RADIUS);
    }
    let relative = (base.fo76_radius as f32 + delta) / base.fo76_radius as f32;
    (base.fo4_radius as f32 * relative).clamp(MIN_RADIUS, MAX_RADIUS)
}

/// Rederive FO4 light radii from FO76 lumens and rescale placed `XRDS` overrides in
/// lockstep. Gated by the caller: `ConversionRun::repair_placed_child_refs` for FO76→FO4,
/// `NormalizeStarfieldLightRadiiFixup` for Starfield→FO4.
pub fn normalize_light_radii(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    _config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    normalize_light_radii_with_candidates(session, mapper, None)
}

/// Starfield lights are lumen-based with additive `XRDS` deltas, like FO76, but its
/// whole-plugin pair never reaches `repair_placed_child_refs`, where the FO76 pass
/// runs; its placed lights are already present when the canonical fixups run.
pub struct NormalizeStarfieldLightRadiiFixup;

impl Fixup for NormalizeStarfieldLightRadiiFixup {
    fn name(&self) -> &'static str {
        "normalize_starfield_light_radii"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            == Some("starfield")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        normalize_light_radii(session, mapper, config)
    }
}

pub(crate) fn normalize_light_radii_for_refr_candidates(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    refr_candidates: &[FormKey],
) -> Result<FixupReport, FixupError> {
    normalize_light_radii_with_candidates(session, mapper, Some(refr_candidates))
}

fn normalize_light_radii_with_candidates(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    refr_candidates: Option<&[FormKey]>,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let interner = mapper.interner;

    let present = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if !present.iter().any(|s| s.as_str() == "LIGH") {
        return Ok(report);
    }
    let has_refr = present.iter().any(|s| s.as_str() == "REFR");

    let own_name = session.target_slot().parsed.plugin_name.clone();
    let own_sym = interner.intern(&own_name);
    let own_load_index = session.target_masters().len();

    let ligh_sig = SigCode::from_str("LIGH").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let refr_sig = SigCode::from_str("REFR").map_err(|e| FixupError::SchemaError(e.to_string()))?;

    // Pass A: snapshot every own base light BEFORE any radius is rewritten, so the
    // placed overrides in pass B see pre-rescale values whatever the iteration order.
    let ligh_keys: Vec<_> = session
        .form_keys_of_sig(ligh_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
        .into_iter()
        .filter(|fk| fk.plugin == own_sym)
        .collect();

    let mut bases: FxHashMap<u32, BaseLight> = FxHashMap::default();
    for fk in &ligh_keys {
        let Some(data) = session
            .first_subrecord_bytes(fk, "DATA")
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        else {
            continue;
        };
        let Some(fo76_radius) = read_u32(&data, FO4_LIGH_DATA_RADIUS_OFFSET) else {
            continue;
        };
        let lumens = read_u32(&data, FO76_LIGH_DATA_LUMENS_OFFSET).unwrap_or(0);
        bases.insert(
            fk.local & 0x00FF_FFFF,
            BaseLight {
                fo76_radius,
                fo4_radius: fo4_radius_from_lumens(lumens, fo76_radius),
            },
        );
    }
    if bases.is_empty() {
        return Ok(report);
    }

    // Pass B: rescale each light placement's XRDS onto the rederived base radius.
    if has_refr {
        let discovered_refr_fks;
        let refr_fks = if let Some(refr_candidates) = refr_candidates {
            refr_candidates
        } else {
            discovered_refr_fks = session
                .form_keys_of_sig(refr_sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            &discovered_refr_fks
        };
        for fk in refr_fks {
            let Some(name) = session
                .first_subrecord_bytes(fk, "NAME")
                .map_err(|e| FixupError::HandleError(e.to_string()))?
            else {
                continue;
            };
            let Some(base_raw) = read_u32(&name, 0) else {
                continue;
            };
            // Only own-plugin LIGH bases carry an FO76 radius we can resolve.
            if (base_raw >> 24) as usize != own_load_index {
                continue;
            }
            let Some(&base) = bases.get(&(base_raw & 0x00FF_FFFF)) else {
                continue;
            };
            let Some(xrds) = session
                .first_subrecord_bytes(fk, "XRDS")
                .map_err(|e| FixupError::HandleError(e.to_string()))?
            else {
                continue;
            };
            if xrds.len() < 4 {
                continue;
            }
            let delta = f32::from_le_bytes(xrds[0..4].try_into().unwrap());
            let absolute = placed_radius_from_delta(base, delta);
            if absolute.to_le_bytes() == delta.to_le_bytes() {
                continue;
            }
            let changed = session
                .patch_subrecord_bytes(fk, "XRDS", |buf| {
                    if buf.len() < 4 {
                        return false;
                    }
                    let current = f32::from_le_bytes(buf[0..4].try_into().unwrap());
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
    }

    // Pass C: write the rederived base radii and clear FO76's lumens, which FO4
    // would otherwise read as the light's item Value.
    for fk in &ligh_keys {
        let Some(&base) = bases.get(&(fk.local & 0x00FF_FFFF)) else {
            continue;
        };
        let changed = session
            .patch_subrecord_bytes(fk, "DATA", |buf| {
                let mut touched = false;
                if buf.len() >= FO4_LIGH_DATA_RADIUS_OFFSET + 4
                    && read_u32(buf, FO4_LIGH_DATA_RADIUS_OFFSET) == Some(base.fo76_radius)
                    && base.fo4_radius != base.fo76_radius
                {
                    buf[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                        .copy_from_slice(&base.fo4_radius.to_le_bytes());
                    touched = true;
                }
                if buf.len() >= FO76_LIGH_DATA_LUMENS_OFFSET + 4
                    && read_u32(buf, FO76_LIGH_DATA_LUMENS_OFFSET) != Some(0)
                {
                    buf[FO76_LIGH_DATA_LUMENS_OFFSET..FO76_LIGH_DATA_LUMENS_OFFSET + 4]
                        .copy_from_slice(&0u32.to_le_bytes());
                    touched = true;
                }
                touched
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
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_new_native, plugin_handle_save_no_py,
    };
    use smallvec::{SmallVec, smallvec};

    fn raw_field(sig: &str, bytes: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn raw_record(
        sig: &str,
        local: u32,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("Out.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec![],
        }
    }

    fn light_fixture(interner: &StringInterner) -> u64 {
        let handle = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        let mut session = open_session(handle, None).unwrap();
        let schema = session.schema().unwrap();
        let mut data = vec![0u8; FO76_LIGH_DATA_LUMENS_OFFSET + 4];
        data[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .copy_from_slice(&1000u32.to_le_bytes());
        data[FO76_LIGH_DATA_LUMENS_OFFSET..FO76_LIGH_DATA_LUMENS_OFFSET + 4]
            .copy_from_slice(&4000u32.to_le_bytes());
        for record in [
            raw_record("LIGH", 0x100, vec![raw_field("DATA", data)], interner),
            raw_record(
                "REFR",
                0x200,
                vec![
                    raw_field("NAME", 0x0000_0100u32.to_le_bytes().to_vec()),
                    raw_field("XRDS", (-500.0f32).to_le_bytes().to_vec()),
                ],
                interner,
            ),
            raw_record(
                "REFR",
                0x201,
                vec![
                    raw_field("NAME", 0x0000_0100u32.to_le_bytes().to_vec()),
                    raw_field("XRDS", vec![1, 2]),
                ],
                interner,
            ),
            raw_record(
                "REFR",
                0x202,
                vec![raw_field("NAME", 0x0000_0100u32.to_le_bytes().to_vec())],
                interner,
            ),
        ] {
            session
                .add_record(record, schema.as_ref(), interner)
                .unwrap();
        }
        drop(session);
        handle
    }

    #[test]
    fn reference_lumens_land_on_fo4_modal_radius() {
        // The calibration anchor: 1000 lm with a wider authored cull distance.
        assert_eq!(fo4_radius_from_lumens(1000, 4000), 256);
    }

    #[test]
    fn radius_scales_with_square_root_of_lumens() {
        // 4x the lumens is 2x the reach in a 1/d^2 system, not 4x.
        assert_eq!(fo4_radius_from_lumens(4000, 8000), 512);
        assert_eq!(fo4_radius_from_lumens(250, 8000), 128);
    }

    #[test]
    fn authored_radius_caps_the_derived_radius() {
        // Artist culled tighter than the physical falloff — keep their intent.
        assert_eq!(fo4_radius_from_lumens(1000, 100), 100);
    }

    #[test]
    fn zero_authored_radius_uses_lumens_alone() {
        // LGT_HoodedLamp01NS: 2000 lm, no authored radius. The old pass copied the
        // lumens straight into the radius and produced a 2000-unit floodlight.
        assert_eq!(fo4_radius_from_lumens(2000, 0), 362);
    }

    #[test]
    fn derived_radius_never_exceeds_the_hooks_radius() {
        // The pass must stay non-increasing so the hook's sunlight/gobo/shadow
        // ceilings remain upper bounds.
        for fo76_radius in [1u32, 64, 256, 1000, 1024, 2048] {
            for lumens in [0u32, 1, 50, 1000, 20_000, 46_000] {
                assert!(
                    fo4_radius_from_lumens(lumens, fo76_radius) <= fo76_radius.max(1),
                    "lumens={lumens} fo76_radius={fo76_radius}"
                );
            }
        }
    }

    #[test]
    fn missing_lumens_keep_the_authored_radius() {
        assert_eq!(fo4_radius_from_lumens(0, 640), 640);
    }

    #[test]
    fn derived_radius_is_floored_and_capped() {
        assert_eq!(fo4_radius_from_lumens(1, 0), 8);
        assert_eq!(fo4_radius_from_lumens(0, 0), 1);
        assert_eq!(fo4_radius_from_lumens(10_000_000, 0), MAX_RADIUS as u32);
    }

    #[test]
    fn placed_override_scales_with_the_rederived_base() {
        // 50FC1E on LGT_HoodedLamp01NS: FO76 base 2000 with a -373.15 delta is an
        // 81% reach instance, which must land at 81% of the new 362 base.
        let base = BaseLight {
            fo76_radius: 2000,
            fo4_radius: 362,
        };
        let r = placed_radius_from_delta(base, -373.15);
        assert!((r - 294.46).abs() < 0.5, "got {r}");
    }

    #[test]
    fn placed_override_matching_base_is_unchanged() {
        let base = BaseLight {
            fo76_radius: 1000,
            fo4_radius: 256,
        };
        assert_eq!(placed_radius_from_delta(base, 0.0), 256.0);
    }

    #[test]
    fn placed_override_below_zero_is_floored() {
        // The negative-absolute-radius case that corrupted the cell partition.
        let base = BaseLight {
            fo76_radius: 1000,
            fo4_radius: 256,
        };
        assert_eq!(placed_radius_from_delta(base, -1262.7), MIN_RADIUS);
    }

    #[test]
    fn placed_override_on_zero_radius_base_falls_back_to_base() {
        let base = BaseLight {
            fo76_radius: 0,
            fo4_radius: 362,
        };
        assert_eq!(placed_radius_from_delta(base, -500.0), 362.0);
    }

    #[test]
    fn shared_refr_candidates_match_legacy_light_scan_exactly() {
        let interner = StringInterner::new();
        let legacy = light_fixture(&interner);
        let shared = light_fixture(&interner);
        let run = |handle, use_shared: bool| {
            let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            let report = {
                let mut session = open_session(handle, None).unwrap();
                if use_shared {
                    let discovery = crate::fixups::repair_placed_teleport_doors::discover_placed_refr_candidates(
                        &mut session,
                        &interner,
                    )
                    .unwrap();
                    normalize_light_radii_for_refr_candidates(
                        &mut session,
                        &mut mapper,
                        &discovery.light_radius_candidates,
                    )
                    .unwrap()
                } else {
                    normalize_light_radii(&mut session, &mut mapper, &FixupConfig::default())
                        .unwrap()
                }
            };
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("Out.esm");
            plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
            let bytes = std::fs::read(path).unwrap();
            assert!(plugin_handle_close_native(handle));
            (report, bytes)
        };

        let (legacy_report, legacy_bytes) = run(legacy, false);
        let (shared_report, shared_bytes) = run(shared, true);
        assert_eq!(legacy_report.records_changed, shared_report.records_changed);
        assert_eq!(legacy_report.warnings, shared_report.warnings);
        assert_eq!(legacy_bytes, shared_bytes);
    }

    #[test]
    fn starfield_light_radii_fixup_runs_only_for_starfield_to_fo4() {
        for (source_game, expected) in [("starfield", true), ("fo76", false)] {
            let target = plugin_handle_new_native("Starfield.esm", Some("fo4")).unwrap();
            let source = plugin_handle_new_native("Source.esm", Some(source_game)).unwrap();
            let applies = {
                let session = open_session(target, Some(source)).unwrap();
                NormalizeStarfieldLightRadiiFixup
                    .applies_to_session(&session, &FixupConfig::default())
            };
            assert!(plugin_handle_close_native(target));
            assert!(plugin_handle_close_native(source));
            assert_eq!(applies, expected, "{source_game}");
        }
        assert!(
            crate::store2::fixups_v2::build_default_segment_plan()
                .iter()
                .flat_map(|segment| segment.fixup_names())
                .any(|name| name == "normalize_starfield_light_radii")
        );
    }
}
