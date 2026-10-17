//! Fixup: light radii for `LIGH` bases and the placed `REFR` overrides that scale
//! them, for the two lumen-based source games.
//!
//! **FO76** (`normalize_light_radii_fo76`) keeps FO76's own light model: FO4CS
//! `[Fo76Lighting]` evaluates it at runtime from the lumens and the flag contract
//! the translator hook leaves in `DATA` (see `fixups::fo76_light_model`). This
//! pass only converts the fields whose *meaning* differs between the games:
//!
//! - base `DATA.Radius` becomes FO76's runtime radius: the stored radius, or for
//!   flag 0x400000 the 5-lux auto radius; base `FNAM` becomes the dimmer that
//!   reproduces FO76's intensity at that radius (exactly 1.0 for auto radius);
//! - placed `XRDS`: both games add it to the base radius, but the written base is
//!   FO76's runtime radius clamped and rounded, so `XRDS = clamp(base_runtime +
//!   delta) - written_base` (usually the FO76 delta itself);
//! - placed `XLIG` fade: FO76 multiplies intensity by it, FO4 adds it to `FNAM`,
//!   so `fade = FNAM * (m − 1)`, scaled by `(R_base / R_ref)²` when a stored-radius
//!   ref also changes the radius (FO4CS derives intensity from the ref's effective
//!   radius, `R_base + XRDS`). A cone-normalised spot's `m` also carries
//!   `k(FOV_ref) / k(FOV_base)`: FO76 spreads its lumens over the ref's own cone.
//!
//! **Starfield** (`normalize_light_radii`) still approximates its lights in FO4
//! terms. A `1/d²` source falls below a fixed illuminance at `sqrt(I / E_min)`, so
//! the FO4 radius scales with `sqrt(lumens)`, anchored at 1000 lm → 256 and capped
//! by the authored radius:
//!
//! ```text
//! radius = clamp(min(256 * sqrt(lumens / 1000), source_radius), 1, 2048)
//! XRDS   = clamp(new_base_radius * (source_radius + delta) / source_radius, 1, 2048)
//!          - new_base_radius
//! ```
//!
//! after which the lumens are cleared, since FO4 reads that slot as an item Value.
//!
//! In both modes the effective radius (base + `XRDS`) is clamped to [1, 2048]: a
//! non-positive one is a degenerate bounding volume that corrupts the cell's spatial
//! partition and silently kills physics and sound for the whole cell. Runs post-copy
//! with all records present; every base is read before any is rewritten, so
//! overrides use pre-rewrite values.

use rustc_hash::FxHashMap;

use crate::fixups::fo76_light_model::{
    FO4_LIGH_FLAG_CAN_BE_CARRIED, Fo76Light, clamp_radius, fo76_flags_from_contract_weight,
    placed_fade_delta, placed_radius, placed_xrds,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::PluginSession;

/// FO4 `LIGH.DATA` byte offsets. Radius is a `u32` @ +4; @ +56 is FO4's item
/// `Value`, which carries the source game's lumens.
const FO4_LIGH_DATA_RADIUS_OFFSET: usize = 4;
const FO4_LIGH_DATA_FLAGS_OFFSET: usize = 12;
const FO4_LIGH_DATA_FOV_OFFSET: usize = 20;
const FO76_LIGH_DATA_LUMENS_OFFSET: usize = 56;
const FO4_LIGH_DATA_WEIGHT_OFFSET: usize = 60;
/// `XLIG` FOV delta (f32) @ +0 and fade (f32) @ +4.
const XLIG_FOV_OFFSET: usize = 0;
const XLIG_FADE_OFFSET: usize = 4;

/// Illuminance-threshold anchor: a Starfield light of `REFERENCE_LUMENS` converts
/// to `RADIUS_AT_REFERENCE_LUMENS`. 1000 lm → 256 comes from the Creation Kit check
/// on `SugarGrove01` and coincides with `Fallout4.esm`'s modal light radius.
const REFERENCE_LUMENS: f32 = 1000.0;
const RADIUS_AT_REFERENCE_LUMENS: f32 = 256.0;

/// A non-positive radius is a degenerate bounding volume, so results are floored
/// at 1; 2048 matches the translator hook's synthetic ceiling.
const MIN_RADIUS: f32 = 1.0;
const MAX_RADIUS: f32 = 2048.0;

/// Pre-rescale state of one own-plugin Starfield base light.
#[derive(Clone, Copy)]
struct BaseLight {
    /// Radius as the translator hook left it — the source's authored cull distance.
    fo76_radius: u32,
    /// The FO4 falloff radius derived from this light's lumens.
    fo4_radius: u32,
}

/// One own-plugin FO76 base light, read before any of it is rewritten.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fo76BaseLight {
    /// A ref that changes the radius also rescales the fade (see `placed_fade_delta`).
    scale_fade_by_radius: bool,
    /// FO76's runtime radius before FO4's clamp; placed `XRDS` deltas add to it.
    runtime_radius: f64,
    radius: u32,
    dimmer: f32,
    /// Folds a ref's XLIG FOV delta into its fade (`Fo76Light::placed_multiplier`).
    light: Fo76Light,
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|slice| u32::from_le_bytes(slice.try_into().unwrap()))
}

fn read_f32(bytes: &[u8], offset: usize) -> Option<f32> {
    bytes
        .get(offset..offset + 4)
        .map(|slice| f32::from_le_bytes(slice.try_into().unwrap()))
}

/// FO4 falloff radius for a Starfield light, capped by its authored cull distance.
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

/// Effective FO4 radius of a placed Starfield light: the rederived base radius
/// scaled by the instance's relative reach. `XRDS` is this less `fo4_radius`.
fn placed_radius_from_delta(base: BaseLight, delta: f32) -> f32 {
    if base.fo76_radius == 0 {
        return (base.fo4_radius as f32).clamp(MIN_RADIUS, MAX_RADIUS);
    }
    let relative = (base.fo76_radius as f32 + delta) / base.fo76_radius as f32;
    (base.fo4_radius as f32 * relative).clamp(MIN_RADIUS, MAX_RADIUS)
}

/// Reads a base the FO76 translator hook has already normalised: FO4 flag word
/// masked, FO76 high flags in the `DATA+60` contract, lumens still at `DATA+56`.
/// A carryable light carries no contract.
fn fo76_base_light(data: &[u8]) -> Option<Fo76BaseLight> {
    let stored_radius = read_u32(data, FO4_LIGH_DATA_RADIUS_OFFSET)?;
    let flags = read_u32(data, FO4_LIGH_DATA_FLAGS_OFFSET).unwrap_or(0);
    let (lumens, fo76_flags) = if flags & FO4_LIGH_FLAG_CAN_BE_CARRIED != 0 {
        (0, 0)
    } else {
        (
            read_u32(data, FO76_LIGH_DATA_LUMENS_OFFSET).unwrap_or(0),
            read_f32(data, FO4_LIGH_DATA_WEIGHT_OFFSET)
                .map(fo76_flags_from_contract_weight)
                .unwrap_or(0),
        )
    };
    let light = Fo76Light {
        lumens,
        flags: flags | fo76_flags,
        fov_degrees: read_f32(data, FO4_LIGH_DATA_FOV_OFFSET).unwrap_or(0.0),
    };
    let runtime_radius = light.runtime_radius(stored_radius);
    let radius = clamp_radius(runtime_radius);
    Some(Fo76BaseLight {
        scale_fade_by_radius: lumens > 0 && !light.is_auto_radius(),
        runtime_radius,
        radius,
        dimmer: light.base_dimmer(radius),
        light,
    })
}

/// Rederive FO4 light radii from Starfield lumens and rescale placed `XRDS`
/// overrides in lockstep. Run by `NormalizeStarfieldLightRadiiFixup`.
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
            // Only own-plugin LIGH bases carry a source radius we can resolve.
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
            let xrds = placed_xrds(base.fo4_radius, placed_radius_from_delta(base, delta));
            if patch_f32(session, fk, "XRDS", 0, delta, xrds)? {
                report.records_changed = report.records_changed.saturating_add(1);
            }
        }
    }

    // Pass C: write the rederived base radii and clear the lumens, which FO4
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

/// Replace the f32 at `offset` of `sig` with `new`, only while it still holds
/// `expected`. Returns whether the subrecord changed.
fn patch_f32(
    session: &mut PluginSession,
    fk: &FormKey,
    sig: &str,
    offset: usize,
    expected: f32,
    new: f32,
) -> Result<bool, FixupError> {
    if new.to_le_bytes() == expected.to_le_bytes() {
        return Ok(false);
    }
    session
        .patch_subrecord_bytes(fk, sig, |buf| {
            if read_f32(buf, offset).map(f32::to_le_bytes) != Some(expected.to_le_bytes()) {
                return false;
            }
            buf[offset..offset + 4].copy_from_slice(&new.to_le_bytes());
            true
        })
        .map_err(|e| FixupError::HandleError(e.to_string()))
}

/// FO76→FO4 light pass, run by `ConversionRun::repair_placed_child_refs`.
/// `refr_candidates` (the REFRs carrying `XRDS` or `XLIG`) replaces the full REFR
/// scan when the caller has already discovered them.
pub(crate) fn normalize_light_radii_fo76(
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

    // Pass A: read every own base before any is rewritten.
    let ligh_keys: Vec<_> = session
        .form_keys_of_sig(ligh_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
        .into_iter()
        .filter(|fk| fk.plugin == own_sym)
        .collect();
    let mut bases: FxHashMap<u32, Fo76BaseLight> = FxHashMap::default();
    for fk in &ligh_keys {
        let Some(data) = session
            .first_subrecord_bytes(fk, "DATA")
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        else {
            continue;
        };
        if let Some(base) = fo76_base_light(&data) {
            bases.insert(fk.local & 0x00FF_FFFF, base);
        }
    }
    if bases.is_empty() {
        return Ok(report);
    }

    // Pass B: placed XRDS (delta on the written base) and XLIG fade (multiplier →
    // delta, at the ref's effective radius).
    let mut refs_needing_xlig = 0usize;
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
            if (base_raw >> 24) as usize != own_load_index {
                continue;
            }
            let Some(&base) = bases.get(&(base_raw & 0x00FF_FFFF)) else {
                continue;
            };
            let xrds_delta = session
                .first_subrecord_bytes(fk, "XRDS")
                .map_err(|e| FixupError::HandleError(e.to_string()))?
                .and_then(|xrds| read_f32(&xrds, 0));
            let xlig = session
                .first_subrecord_bytes(fk, "XLIG")
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let xlig_fade = xlig
                .as_ref()
                .and_then(|xlig| read_f32(xlig, XLIG_FADE_OFFSET));
            let xlig_fov_delta = xlig
                .as_ref()
                .and_then(|xlig| read_f32(xlig, XLIG_FOV_OFFSET))
                .unwrap_or(0.0);

            let mut changed = false;
            let placed = xrds_delta.map(|delta| placed_radius(base.runtime_radius, delta));
            if let (Some(delta), Some(effective)) = (xrds_delta, placed) {
                let xrds = placed_xrds(base.radius, effective);
                changed |= patch_f32(session, fk, "XRDS", 0, delta, xrds)?;
            }
            match xlig_fade {
                Some(multiplier) => {
                    let fade = placed_fade_delta(
                        base.dimmer,
                        base.light.placed_multiplier(multiplier, xlig_fov_delta),
                        base.radius,
                        placed,
                        base.scale_fade_by_radius,
                    );
                    changed |= patch_f32(session, fk, "XLIG", XLIG_FADE_OFFSET, multiplier, fade)?;
                }
                None => {
                    if placed_fade_delta(
                        base.dimmer,
                        1.0,
                        base.radius,
                        placed,
                        base.scale_fade_by_radius,
                    ) != 0.0
                    {
                        refs_needing_xlig += 1;
                    }
                }
            }
            if changed {
                report.records_changed = report.records_changed.saturating_add(1);
            }
        }
    }
    if refs_needing_xlig > 0 {
        report.warnings.push(interner.intern(&format!(
            "normalize_light_radii_fo76: {refs_needing_xlig} placed light(s) change a stored-radius base's XRDS without an XLIG to carry the matching fade; FO4CS sees their intensity scaled by (R_ref / R_base)^2"
        )));
    }

    // Pass C: base runtime radius and the dimmer that reproduces FO76's intensity.
    // Lumens and the flag contract stay for FO4CS.
    for fk in &ligh_keys {
        let Some(&base) = bases.get(&(fk.local & 0x00FF_FFFF)) else {
            continue;
        };
        let mut changed = session
            .patch_subrecord_bytes(fk, "DATA", |buf| {
                if read_u32(buf, FO4_LIGH_DATA_RADIUS_OFFSET) == Some(base.radius) {
                    return false;
                }
                buf[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                    .copy_from_slice(&base.radius.to_le_bytes());
                true
            })
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let fade = session
            .first_subrecord_bytes(fk, "FNAM")
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .and_then(|fnam| read_f32(&fnam, 0));
        if let Some(fade) = fade {
            changed |= patch_f32(session, fk, "FNAM", 0, fade, base.dimmer)?;
        }
        if changed {
            report.records_changed = report.records_changed.saturating_add(1);
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::fo76_light_model::{
        FO76_LIGH_FLAG_AUTO_RADIUS, FO76_LIGH_FLAG_CONE_NORMALISED_SPOT, SHARED_VECTORS,
        contract_weight_from_fo76_flags,
    };
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_new_native, plugin_handle_save_no_py,
    };
    use smallvec::{SmallVec, smallvec};

    const NON_SHADOW_SPOTLIGHT: u32 = 0x0000_4000;

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

    /// A LIGH as the FO76 translator hook leaves it: FO4-masked flags, lumens at
    /// +56, the high-flag contract at +60 and `FNAM` 1.0.
    fn fo76_ligh(
        local: u32,
        lumens: u32,
        flags: u32,
        fov: f32,
        radius: u32,
        fo76_high: u32,
        interner: &StringInterner,
    ) -> Record {
        let mut data = vec![0u8; FO4_LIGH_DATA_WEIGHT_OFFSET + 4];
        data[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .copy_from_slice(&radius.to_le_bytes());
        data[FO4_LIGH_DATA_FLAGS_OFFSET..FO4_LIGH_DATA_FLAGS_OFFSET + 4]
            .copy_from_slice(&flags.to_le_bytes());
        data[FO4_LIGH_DATA_FOV_OFFSET..FO4_LIGH_DATA_FOV_OFFSET + 4]
            .copy_from_slice(&fov.to_le_bytes());
        data[FO76_LIGH_DATA_LUMENS_OFFSET..FO76_LIGH_DATA_LUMENS_OFFSET + 4]
            .copy_from_slice(&lumens.to_le_bytes());
        data[FO4_LIGH_DATA_WEIGHT_OFFSET..FO4_LIGH_DATA_WEIGHT_OFFSET + 4]
            .copy_from_slice(&contract_weight_from_fo76_flags(fo76_high).to_le_bytes());
        raw_record(
            "LIGH",
            local,
            vec![
                raw_field("DATA", data),
                raw_field("FNAM", 1.0f32.to_le_bytes().to_vec()),
            ],
            interner,
        )
    }

    fn placed_light(
        local: u32,
        base: u32,
        xrds: Option<f32>,
        xlig_fade: Option<f32>,
        interner: &StringInterner,
    ) -> Record {
        let mut fields = vec![raw_field("NAME", base.to_le_bytes().to_vec())];
        if let Some(xrds) = xrds {
            fields.push(raw_field("XRDS", xrds.to_le_bytes().to_vec()));
        }
        if let Some(fade) = xlig_fade {
            let mut xlig = vec![0u8; 24];
            xlig[0..4].copy_from_slice(&5.0f32.to_le_bytes());
            xlig[4..8].copy_from_slice(&fade.to_le_bytes());
            xlig[8..12].copy_from_slice(&7.0f32.to_le_bytes());
            fields.push(raw_field("XLIG", xlig));
        }
        raw_record("REFR", local, fields, interner)
    }

    const AUTO_SPOT: u32 = 0x100;
    const STORED_SPOT: u32 = 0x101;
    const CARRIED: u32 = 0x102;

    fn fo76_light_fixture(interner: &StringInterner) -> u64 {
        let handle = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        let mut session = open_session(handle, None).unwrap();
        let schema = session.schema().unwrap();
        for record in [
            // LGT_OBS_SpotNS_01K_warm: stale stored radius 16, auto 729.6.
            fo76_ligh(
                AUTO_SPOT,
                1000,
                NON_SHADOW_SPOTLIGHT,
                90.0,
                16,
                FO76_LIGH_FLAG_AUTO_RADIUS | FO76_LIGH_FLAG_CONE_NORMALISED_SPOT,
                interner,
            ),
            // LGT_RKE_StreetLight_SpotNS_1000_Warm: stored 600, 318 cd.
            fo76_ligh(
                STORED_SPOT,
                1000,
                NON_SHADOW_SPOTLIGHT,
                90.0,
                600,
                0,
                interner,
            ),
            // Carryable: lumens already cleared by the hook; a stale weight is ignored.
            fo76_ligh(
                CARRIED,
                0,
                0x0000_0002,
                0.0,
                300,
                FO76_LIGH_FLAG_AUTO_RADIUS,
                interner,
            ),
            placed_light(0x200, AUTO_SPOT, Some(-435.0), Some(0.6), interner),
            placed_light(0x201, AUTO_SPOT, None, Some(1.6), interner),
            placed_light(0x202, STORED_SPOT, Some(-300.0), Some(1.6), interner),
            placed_light(0x203, STORED_SPOT, None, Some(0.6), interner),
            placed_light(0x204, STORED_SPOT, None, Some(1.0), interner),
            placed_light(0x205, STORED_SPOT, Some(-300.0), None, interner),
            placed_light(0x206, CARRIED, Some(100.0), Some(1.6), interner),
            placed_light(0x207, STORED_SPOT, None, None, interner),
        ] {
            session
                .add_record(record, schema.as_ref(), interner)
                .unwrap();
        }
        drop(session);
        handle
    }

    fn subrecord_f32(
        session: &mut PluginSession,
        local: u32,
        sig: &str,
        offset: usize,
        interner: &StringInterner,
    ) -> Option<f32> {
        let fk = FormKey {
            local,
            plugin: interner.intern("Out.esm"),
        };
        session
            .first_subrecord_bytes(&fk, sig)
            .unwrap()
            .and_then(|bytes| read_f32(&bytes, offset))
    }

    fn subrecord_u32(
        session: &mut PluginSession,
        local: u32,
        sig: &str,
        offset: usize,
        interner: &StringInterner,
    ) -> Option<u32> {
        let fk = FormKey {
            local,
            plugin: interner.intern("Out.esm"),
        };
        session
            .first_subrecord_bytes(&fk, sig)
            .unwrap()
            .and_then(|bytes| read_u32(&bytes, offset))
    }

    #[test]
    fn fo4_radius_from_lumens_calibration_caps_and_floors() {
        for (name, lumens, fo76_radius, expected) in [
            ("1000 lm calibration anchor", 1000, 4000, 256),
            ("4x lumens doubles reach", 4000, 8000, 512),
            ("quarter lumens halves reach", 250, 8000, 128),
            ("authored radius caps", 1000, 100, 100),
            ("no authored radius uses lumens alone", 2000, 0, 362),
            ("missing lumens keep authored radius", 0, 640, 640),
            ("floored", 1, 0, 8),
            ("no data", 0, 0, 1),
            ("capped", 10_000_000, 0, MAX_RADIUS as u32),
        ] {
            assert_eq!(
                fo4_radius_from_lumens(lumens, fo76_radius),
                expected,
                "{name}"
            );
        }

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
    fn placed_override_scales_with_the_rederived_base() {
        let hooded_lamp = BaseLight {
            fo76_radius: 2000,
            fo4_radius: 362,
        };
        let r = placed_radius_from_delta(hooded_lamp, -373.15);
        assert!((r - 294.46).abs() < 0.5, "got {r}");

        let base = BaseLight {
            fo76_radius: 1000,
            fo4_radius: 256,
        };
        assert_eq!(placed_radius_from_delta(base, 0.0), 256.0);
        // The negative-absolute-radius case that corrupted the cell partition.
        assert_eq!(placed_radius_from_delta(base, -1262.7), MIN_RADIUS);
        let zero_radius_base = BaseLight {
            fo76_radius: 0,
            fo4_radius: 362,
        };
        assert_eq!(placed_radius_from_delta(zero_radius_base, -500.0), 362.0);
    }

    #[test]
    fn fo76_base_light_reads_the_hook_contract() {
        for &(name, lumens, shape, fov, high, stored, _, _, radius, dimmer) in SHARED_VECTORS {
            let mut data = vec![0u8; FO4_LIGH_DATA_WEIGHT_OFFSET + 4];
            data[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                .copy_from_slice(&stored.to_le_bytes());
            data[FO4_LIGH_DATA_FLAGS_OFFSET..FO4_LIGH_DATA_FLAGS_OFFSET + 4]
                .copy_from_slice(&shape.to_le_bytes());
            data[FO4_LIGH_DATA_FOV_OFFSET..FO4_LIGH_DATA_FOV_OFFSET + 4]
                .copy_from_slice(&fov.to_le_bytes());
            data[FO76_LIGH_DATA_LUMENS_OFFSET..FO76_LIGH_DATA_LUMENS_OFFSET + 4]
                .copy_from_slice(&lumens.to_le_bytes());
            data[FO4_LIGH_DATA_WEIGHT_OFFSET..FO4_LIGH_DATA_WEIGHT_OFFSET + 4]
                .copy_from_slice(&contract_weight_from_fo76_flags(high).to_le_bytes());
            let base = fo76_base_light(&data).unwrap();
            assert_eq!(base.radius, radius, "{name}");
            assert_eq!(
                base.scale_fade_by_radius,
                high & FO76_LIGH_FLAG_AUTO_RADIUS == 0,
                "{name}"
            );
            assert!(
                (base.dimmer - dimmer).abs() < 1e-5,
                "{name}: {}",
                base.dimmer
            );

            // The carry flag switches the contract off.
            data[FO4_LIGH_DATA_FLAGS_OFFSET..FO4_LIGH_DATA_FLAGS_OFFSET + 4]
                .copy_from_slice(&(shape | FO4_LIGH_FLAG_CAN_BE_CARRIED).to_le_bytes());
            let carried = fo76_base_light(&data).unwrap();
            assert!(!carried.scale_fade_by_radius, "{name}");
            assert_eq!(carried.radius, stored.clamp(1, 2048), "{name}");
            assert_eq!(carried.dimmer, 1.0, "{name}");
        }
    }

    #[test]
    fn fo76_pass_writes_runtime_radii_fade_and_delta_overrides() {
        let interner = StringInterner::new();
        let handle = fo76_light_fixture(&interner);
        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut session = open_session(handle, None).unwrap();
        let report = normalize_light_radii_fo76(&mut session, &mut mapper, None).unwrap();
        let mut f32_at = |local: u32, sig: &str, offset: usize| {
            subrecord_f32(&mut session, local, sig, offset, &interner)
        };

        // Bases: runtime radius and FNAM; lumens and contract untouched.
        assert_eq!(f32_at(AUTO_SPOT, "FNAM", 0), Some(1.0));
        let stored_dimmer = f32_at(STORED_SPOT, "FNAM", 0).unwrap();
        assert!(
            (stored_dimmer - 0.866_263).abs() < 1e-5,
            "got {stored_dimmer}"
        );
        assert_eq!(f32_at(CARRIED, "FNAM", 0), Some(1.0));
        assert_eq!(
            f32_at(AUTO_SPOT, "DATA", FO4_LIGH_DATA_WEIGHT_OFFSET),
            Some(3.0)
        );

        // Auto base, Fort Atlas ref: 729.64 − 435 = 294.64 on the written 730.
        // The cone-normalised base widens 90° → 95° under the XLIG FOV delta, so
        // FO76 spreads its lumens over the wider cone: k(95) / k(90) = 0.902 85,
        // fade 0.6 → 0.6 × 0.902 85 − 1.
        let cone_ratio = 0.902_85f32;
        let xrds = f32_at(0x200, "XRDS", 0).unwrap();
        assert!((730.0 + xrds - 294.636).abs() < 1e-2, "got {xrds}");
        let fade = f32_at(0x200, "XLIG", XLIG_FADE_OFFSET).unwrap();
        assert!((fade - (0.6 * cone_ratio - 1.0)).abs() < 1e-4, "got {fade}");
        // FOV and the remaining XLIG fields are untouched.
        assert_eq!(f32_at(0x200, "XLIG", 0), Some(5.0));
        assert_eq!(f32_at(0x200, "XLIG", 8), Some(7.0));
        let fade = f32_at(0x201, "XLIG", XLIG_FADE_OFFSET).unwrap();
        assert!((fade - (1.6 * cone_ratio - 1.0)).abs() < 1e-4, "got {fade}");

        // Stored base 600, ref −300 → 300: dimmer × 1.6 × (600/300)².
        assert_eq!(f32_at(0x202, "XRDS", 0), Some(-300.0));
        let fade = f32_at(0x202, "XLIG", XLIG_FADE_OFFSET).unwrap();
        assert!(
            (stored_dimmer + fade - stored_dimmer * 1.6 * 4.0).abs() < 1e-5,
            "got {fade}"
        );
        // No XRDS: plain FNAM * (m − 1).
        assert_eq!(
            f32_at(0x203, "XLIG", XLIG_FADE_OFFSET),
            Some(stored_dimmer * (0.6f32 - 1.0))
        );
        assert_eq!(f32_at(0x204, "XLIG", XLIG_FADE_OFFSET), Some(0.0));
        // XRDS without XLIG: radius converted, fade left for the warning.
        assert_eq!(f32_at(0x205, "XRDS", 0), Some(-300.0));
        assert_eq!(f32_at(0x205, "XLIG", XLIG_FADE_OFFSET), None);
        // Carryable base: stored radius 300 + delta 100, fade 1.6 → 0.6.
        assert_eq!(f32_at(0x206, "XRDS", 0), Some(100.0));
        assert_eq!(f32_at(0x206, "XLIG", XLIG_FADE_OFFSET), Some(1.6f32 - 1.0));

        let mut u32_at = |local: u32, sig: &str, offset: usize| {
            subrecord_u32(&mut session, local, sig, offset, &interner)
        };
        assert_eq!(
            u32_at(AUTO_SPOT, "DATA", FO4_LIGH_DATA_RADIUS_OFFSET),
            Some(730)
        );
        assert_eq!(
            u32_at(AUTO_SPOT, "DATA", FO76_LIGH_DATA_LUMENS_OFFSET),
            Some(1000)
        );
        assert_eq!(
            u32_at(STORED_SPOT, "DATA", FO4_LIGH_DATA_RADIUS_OFFSET),
            Some(600)
        );
        assert_eq!(
            u32_at(STORED_SPOT, "DATA", FO76_LIGH_DATA_LUMENS_OFFSET),
            Some(1000)
        );
        assert_eq!(
            u32_at(CARRIED, "DATA", FO4_LIGH_DATA_RADIUS_OFFSET),
            Some(300)
        );

        // Bases AUTO_SPOT + STORED_SPOT (CARRIED is unchanged), refs 0x200-0x206
        // except 0x205, whose FO76 delta is already the FO4 one and has no XLIG.
        assert_eq!(report.records_changed, 2 + 6);
        assert_eq!(report.warnings.len(), 1);
        drop(session);
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn fo76_awning_lamp_reaches_fo4cs_at_its_fo76_intensity() {
        // REFR 6293BA on LGT_WSR_SpotS_02K_warm 33CAEC: shadow spot, cone-normalised,
        // 2000 lm, FOV 90, stored radius 1631; XRDS −1262.702, XLIG FOV +50.927,
        // fade 1.6. FO76: 2000 / (2π(1 − cos 70.46°)) × 1.6 = 765.3 cd at 368.3.
        const AWNING: u32 = 0x300;
        let interner = StringInterner::new();
        let handle = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        let mut session = open_session(handle, None).unwrap();
        let schema = session.schema().unwrap();
        let mut xlig = vec![0u8; 24];
        xlig[0..4].copy_from_slice(&50.927_094f32.to_le_bytes());
        xlig[4..8].copy_from_slice(&1.6f32.to_le_bytes());
        let lamp = raw_record(
            "REFR",
            0x301,
            vec![
                raw_field("NAME", AWNING.to_le_bytes().to_vec()),
                raw_field("XRDS", (-1262.702f32).to_le_bytes().to_vec()),
                raw_field("XLIG", xlig),
            ],
            &interner,
        );
        for record in [
            fo76_ligh(
                AWNING,
                2000,
                0x0000_0400,
                90.0,
                1631,
                FO76_LIGH_FLAG_CONE_NORMALISED_SPOT,
                &interner,
            ),
            lamp,
        ] {
            session
                .add_record(record, schema.as_ref(), &interner)
                .unwrap();
        }
        drop(session);

        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut session = open_session(handle, None).unwrap();
        normalize_light_radii_fo76(&mut session, &mut mapper, None).unwrap();
        let mut f32_at = |local: u32, sig: &str, offset: usize| {
            subrecord_f32(&mut session, local, sig, offset, &interner)
        };
        let fnam = f32_at(AWNING, "FNAM", 0).unwrap();
        let fade = f32_at(0x301, "XLIG", XLIG_FADE_OFFSET).unwrap();
        let xrds = f32_at(0x301, "XRDS", 0).unwrap();
        // The FOV delta stays for the engine's cone.
        assert_eq!(f32_at(0x301, "XLIG", 0), Some(50.927_094));
        let radius_metres = f64::from(1631.0 + xrds) / 69.99;
        // FO4CS IntensityFromFo4Light: 5 × (FNAM + XLIG fade) × (R / 69.99)².
        let intensity = 5.0 * f64::from(fnam + fade) * radius_metres * radius_metres;
        assert!((intensity - 765.3).abs() < 0.5, "got {intensity} cd");
        drop(session);
        assert!(plugin_handle_close_native(handle));
    }

    #[test]
    fn fo76_refr_candidates_match_the_full_scan_exactly() {
        let interner = StringInterner::new();
        let full = fo76_light_fixture(&interner);
        let shared = fo76_light_fixture(&interner);
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
                    normalize_light_radii_fo76(
                        &mut session,
                        &mut mapper,
                        Some(&discovery.light_radius_candidates),
                    )
                    .unwrap()
                } else {
                    normalize_light_radii_fo76(&mut session, &mut mapper, None).unwrap()
                }
            };
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("Out.esm");
            plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
            let bytes = std::fs::read(path).unwrap();
            assert!(plugin_handle_close_native(handle));
            (report, bytes)
        };

        let (full_report, full_bytes) = run(full, false);
        let (shared_report, shared_bytes) = run(shared, true);
        assert_eq!(full_report.records_changed, shared_report.records_changed);
        assert_eq!(full_report.warnings, shared_report.warnings);
        assert_eq!(full_bytes, shared_bytes);
    }

    #[test]
    fn starfield_pass_still_rescales_and_clears_lumens() {
        let interner = StringInterner::new();
        let handle = light_fixture(&interner);
        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut session = open_session(handle, None).unwrap();
        normalize_light_radii(&mut session, &mut mapper, &FixupConfig::default()).unwrap();
        // 4000 lm → 512; XRDS −500 on 1000 → 50 % of 512, i.e. 512 − 256.
        assert_eq!(
            subrecord_u32(
                &mut session,
                0x100,
                "DATA",
                FO4_LIGH_DATA_RADIUS_OFFSET,
                &interner
            ),
            Some(512)
        );
        assert_eq!(
            subrecord_u32(
                &mut session,
                0x100,
                "DATA",
                FO76_LIGH_DATA_LUMENS_OFFSET,
                &interner
            ),
            Some(0)
        );
        assert_eq!(
            subrecord_f32(&mut session, 0x200, "XRDS", 0, &interner),
            Some(-256.0)
        );
        drop(session);
        assert!(plugin_handle_close_native(handle));
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
