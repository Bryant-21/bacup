//! FO76's photometric light model, as the FO76 runtime evaluates it, plus the
//! record contract FO4CS `[Fo76Lighting]` reads back from converted `LIGH` bases.
//!
//! FO76 (`sub_10fbf70`, called from `TESObjectLIGH::GenDynamic`) turns lumens into
//! luminous intensity in candela, clamped to 80 000:
//!
//! ```text
//! I = lumens * k * m        m = placed ref XLIG fade (1.0 at base level)
//! k = 1/(4π) omni, 1/(2π) hemisphere, 1/π spot,
//!     1/(2π(1 − cos(FOV/2))) for a spot with flag 0x800000 (cone-normalised)
//! ```
//!
//! Flag 0x400000 derives the radius from intensity (`sub_4ed09b0`): the distance
//! at which illuminance falls to 5 lux, `69.99 * sqrt(0.2 * I)` game units.
//!
//! FO4CS [Fo76Lighting] photometric contract, on a converted non-carryable `LIGH`:
//! - `DATA+56` (FO4 `Value`, u32 LE): FO76 lumens, the raw FO76 u32 integer
//!   (FO76's slot is an integer, e.g. `LGT_TCS_OmniNS_100_StreetLight` = 100).
//!   No vanilla FO4 `LIGH` sets `Value`, so `> 0` marks an FO76 photometric light.
//! - `DATA+60` (FO4 `Weight`, f32 LE): FO76 flags above FO4's defined range, as an
//!   integer-valued float: 0x400000 → 1, 0x800000 → 2, 0x1000000 → 4, 0x2000000 → 8.
//! - `FNAM`: `I / (5 * (R / 69.99)²)` with `R` the base runtime radius, so FO4CS's
//!   `I = 5 * dimmer * (R / 69.99)²` reproduces FO76's base intensity.

pub(crate) const FO76_LIGH_FLAG_AUTO_RADIUS: u32 = 0x0040_0000;
pub(crate) const FO76_LIGH_FLAG_CONE_NORMALISED_SPOT: u32 = 0x0080_0000;
pub(crate) const FO76_LIGH_FLAG_UNKNOWN_24: u32 = 0x0100_0000;
pub(crate) const FO76_LIGH_FLAG_UNKNOWN_25: u32 = 0x0200_0000;

pub(crate) const FO4_LIGH_FLAG_CAN_BE_CARRIED: u32 = 0x0000_0002;
const FO4_LIGH_FLAG_SHADOW_SPOTLIGHT: u32 = 0x0000_0400;
const FO4_LIGH_FLAG_SHADOW_HEMISPHERE: u32 = 0x0000_0800;
const FO4_LIGH_FLAG_NON_SHADOW_SPOTLIGHT: u32 = 0x0000_4000;

const GAME_UNITS_PER_METRE: f64 = 69.99;
const AUTO_RADIUS_EDGE_LUX: f64 = 5.0;
const MAX_INTENSITY_CANDELA: f64 = 80_000.0;

/// FO4 culls and sizes the light volume with this radius before FO4CS sees it,
/// so it must be positive and bounded.
pub(crate) const MIN_RADIUS: f32 = 1.0;
pub(crate) const MAX_RADIUS: f32 = 2048.0;
pub(crate) const MIN_DIMMER: f32 = 0.01;
pub(crate) const MAX_DIMMER: f32 = 100.0;
pub(crate) const DEFAULT_DIMMER: f32 = 1.0;

/// `(FO76 flag, DATA+60 contract value)`.
const CONTRACT_BITS: [(u32, u32); 4] = [
    (FO76_LIGH_FLAG_AUTO_RADIUS, 1),
    (FO76_LIGH_FLAG_CONE_NORMALISED_SPOT, 2),
    (FO76_LIGH_FLAG_UNKNOWN_24, 4),
    (FO76_LIGH_FLAG_UNKNOWN_25, 8),
];

/// FO4CS [Fo76Lighting] photometric contract: the `DATA+60` value for FO76 flags.
pub(crate) fn contract_weight_from_fo76_flags(fo76_flags: u32) -> f32 {
    CONTRACT_BITS
        .iter()
        .filter(|(flag, _)| fo76_flags & flag != 0)
        .map(|(_, value)| *value)
        .sum::<u32>() as f32
}

/// Inverse of `contract_weight_from_fo76_flags`; anything that is not one of the
/// 16 integer codes decodes to no flags.
pub(crate) fn fo76_flags_from_contract_weight(weight: f32) -> u32 {
    if !(0.0..=15.0).contains(&weight) || weight.fract() != 0.0 {
        return 0;
    }
    let code = weight as u32;
    CONTRACT_BITS
        .iter()
        .filter(|(_, value)| code & value != 0)
        .map(|(flag, _)| *flag)
        .fold(0, |flags, flag| flags | flag)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LightShape {
    Omni,
    Hemisphere,
    Spot,
}

impl LightShape {
    pub(crate) fn from_flags(flags: u32) -> Self {
        if flags & (FO4_LIGH_FLAG_SHADOW_SPOTLIGHT | FO4_LIGH_FLAG_NON_SHADOW_SPOTLIGHT) != 0 {
            Self::Spot
        } else if flags & FO4_LIGH_FLAG_SHADOW_HEMISPHERE != 0 {
            Self::Hemisphere
        } else {
            Self::Omni
        }
    }
}

/// The inputs of FO76's light model. `flags` may hold either the raw FO76 flag
/// word or the FO4-masked one with the high bits restored from the contract.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fo76Light {
    pub lumens: u32,
    pub flags: u32,
    pub fov_degrees: f32,
}

impl Fo76Light {
    pub(crate) fn is_auto_radius(&self) -> bool {
        self.flags & FO76_LIGH_FLAG_AUTO_RADIUS != 0
    }

    fn lumens_to_candela(&self) -> f64 {
        let spot = 1.0 / std::f64::consts::PI;
        match LightShape::from_flags(self.flags) {
            LightShape::Omni => 1.0 / (4.0 * std::f64::consts::PI),
            LightShape::Hemisphere => 1.0 / (2.0 * std::f64::consts::PI),
            LightShape::Spot if self.flags & FO76_LIGH_FLAG_CONE_NORMALISED_SPOT != 0 => {
                let half_angle = (f64::from(self.fov_degrees) * 0.5).to_radians();
                let solid_angle = 2.0 * std::f64::consts::PI * (1.0 - half_angle.cos());
                if solid_angle.is_finite() && solid_angle > 1e-6 {
                    1.0 / solid_angle
                } else {
                    spot
                }
            }
            LightShape::Spot => spot,
        }
    }

    /// Luminous intensity in candela with the ref's XLIG multiplier applied.
    pub(crate) fn intensity(&self, multiplier: f32) -> f64 {
        let intensity = f64::from(self.lumens) * self.lumens_to_candela() * f64::from(multiplier);
        if intensity.is_finite() {
            intensity.clamp(0.0, MAX_INTENSITY_CANDELA)
        } else {
            0.0
        }
    }

    /// The ref's XLIG fade multiplier with its XLIG FOV delta folded in. FO76 spreads
    /// a cone-normalised spot's lumens over the ref's own cone (`sub_1412B4CF0`: base
    /// FOV + XLIG FOV), but the base `FNAM` holds the intensity at the base FOV.
    pub(crate) fn placed_multiplier(&self, multiplier: f32, fov_delta: f32) -> f32 {
        if self.lumens == 0 || !fov_delta.is_finite() || fov_delta == 0.0 {
            return multiplier;
        }
        let placed = Self {
            fov_degrees: self.fov_degrees + fov_delta,
            ..*self
        };
        let ratio = placed.lumens_to_candela() / self.lumens_to_candela();
        if ratio.is_finite() && ratio > 0.0 {
            (f64::from(multiplier) * ratio) as f32
        } else {
            multiplier
        }
    }

    /// FO76's runtime base radius before FO4's safety clamp.
    pub(crate) fn runtime_radius(&self, stored_radius: u32) -> f64 {
        if self.is_auto_radius() {
            auto_radius(self.intensity(1.0))
        } else {
            f64::from(stored_radius)
        }
    }

    /// Base `FNAM` for the radius actually written. An auto-radius light whose
    /// radius survived the clamp is 1.0 exactly: its radius *is* its intensity.
    pub(crate) fn base_dimmer(&self, written_radius: u32) -> f32 {
        if self.lumens == 0 {
            return DEFAULT_DIMMER;
        }
        if self.is_auto_radius()
            && f64::from(written_radius) == auto_radius(self.intensity(1.0)).round()
        {
            return DEFAULT_DIMMER;
        }
        let radius_metres = f64::from(written_radius) / GAME_UNITS_PER_METRE;
        let dimmer = self.intensity(1.0) / (AUTO_RADIUS_EDGE_LUX * radius_metres * radius_metres);
        if dimmer.is_finite() {
            (dimmer as f32).clamp(MIN_DIMMER, MAX_DIMMER)
        } else {
            MAX_DIMMER
        }
    }
}

/// Radius at which a light of `intensity` candela falls to 5 lux.
pub(crate) fn auto_radius(intensity: f64) -> f64 {
    GAME_UNITS_PER_METRE * (intensity / AUTO_RADIUS_EDGE_LUX).sqrt()
}

/// The only clamp left on a converted radius: positive, and within what FO4's
/// light volumes handle.
pub(crate) fn clamp_radius(radius: f64) -> u32 {
    if !radius.is_finite() {
        return MIN_RADIUS as u32;
    }
    (radius.round() as f32).clamp(MIN_RADIUS, MAX_RADIUS) as u32
}

/// A placed light's effective radius: FO76 adds its `XRDS` delta to the base
/// runtime radius, clamped like any converted radius.
pub(crate) fn placed_radius(base_runtime_radius: f64, delta: f32) -> f32 {
    let delta = if delta.is_finite() {
        f64::from(delta)
    } else {
        0.0
    };
    let absolute = base_runtime_radius + delta;
    if absolute.is_finite() {
        (absolute as f32).clamp(MIN_RADIUS, MAX_RADIUS)
    } else {
        MIN_RADIUS
    }
}

/// FO4 `XRDS` for a placed light. FO4 also adds `XRDS` to the base radius
/// (`TESObjectREFR::GetRadius`, 1.10.155 0x47e600: `ExtraRadius + base`), so it is
/// the effective radius less the radius the base record stores.
pub(crate) fn placed_xrds(written_base_radius: u32, effective_radius: f32) -> f32 {
    effective_radius - written_base_radius as f32
}

/// FO4 XLIG fade delta for a ref whose FO76 XLIG fade is the multiplier `m`.
///
/// FO4 `GetFade` is `FNAM + XLIG.fade`. FO4CS derives intensity from the ref's
/// radius (`I = 5 * dimmer * (R / 69.99)²`), so a ref that also changes the radius
/// of a stored-radius base scales its dimmer by `(R_base / R_ref)²` to keep
/// FO76's `I_base * m` (`scale_by_radius`). An auto-radius base, or one without
/// lumens, keeps `FNAM * (m − 1)`.
pub(crate) fn placed_fade_delta(
    base_dimmer: f32,
    multiplier: f32,
    base_radius: u32,
    placed_radius: Option<f32>,
    scale_by_radius: bool,
) -> f32 {
    let multiplier = if multiplier.is_finite() {
        multiplier
    } else {
        1.0
    };
    let radius_ratio_sq = match placed_radius {
        Some(placed) if scale_by_radius && placed > 0.0 => {
            let ratio = base_radius as f32 / placed;
            ratio * ratio
        }
        _ => 1.0,
    };
    if radius_ratio_sq == 1.0 {
        return base_dimmer * (multiplier - 1.0);
    }
    (base_dimmer * multiplier * radius_ratio_sq).clamp(0.0, MAX_DIMMER) - base_dimmer
}

/// Formula vectors shared with FO4CS (`physical_lights_tests.cpp`):
/// `(name, lumens, FO4 shape flags, FOV, FO76 high flags, stored radius,
///   intensity cd, auto radius, written radius, FNAM)`.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(crate) const SHARED_VECTORS: &[(&str, u32, u32, f32, u32, u32, f64, f64, u32, f32)] = &[
    (
        "omni 1000 lm auto",
        1000,
        0,
        0.0,
        FO76_LIGH_FLAG_AUTO_RADIUS,
        0,
        79.577_47,
        279.219_7,
        279,
        1.0,
    ),
    (
        "spot 1000 lm auto",
        1000,
        FO4_LIGH_FLAG_NON_SHADOW_SPOTLIGHT,
        90.0,
        FO76_LIGH_FLAG_AUTO_RADIUS,
        0,
        318.309_9,
        558.439_4,
        558,
        1.0,
    ),
    (
        "spot 1000 lm cone-normalised FOV 90 auto (LGT_OBS_SpotNS_01K_warm)",
        1000,
        FO4_LIGH_FLAG_NON_SHADOW_SPOTLIGHT,
        90.0,
        FO76_LIGH_FLAG_AUTO_RADIUS | FO76_LIGH_FLAG_CONE_NORMALISED_SPOT,
        16,
        543.389_0,
        729.636_2,
        730,
        1.0,
    ),
    (
        "hemisphere 1000 lm auto",
        1000,
        FO4_LIGH_FLAG_SHADOW_HEMISPHERE,
        0.0,
        FO76_LIGH_FLAG_AUTO_RADIUS,
        0,
        159.154_9,
        394.876_3,
        395,
        1.0,
    ),
    (
        "omni 2000 lm auto (LGT_HoodedLamp01NS)",
        2000,
        0,
        0.0,
        FO76_LIGH_FLAG_AUTO_RADIUS,
        0,
        159.154_9,
        394.876_3,
        395,
        1.0,
    ),
    (
        "shadow spot 10000 lm auto (DaylightLED01_SPOTSHADOW)",
        10_000,
        FO4_LIGH_FLAG_SHADOW_SPOTLIGHT,
        90.0,
        FO76_LIGH_FLAG_AUTO_RADIUS,
        1765,
        3183.098_9,
        1765.940_5,
        1766,
        1.0,
    ),
    (
        "spot 1000 lm stored 600 (LGT_RKE_StreetLight)",
        1000,
        FO4_LIGH_FLAG_NON_SHADOW_SPOTLIGHT,
        90.0,
        0,
        600,
        318.309_9,
        558.439_4,
        600,
        0.866_263,
    ),
    (
        "spot 2000 lm stored 500 (LGT_TCS_SpotNS_2000)",
        2000,
        FO4_LIGH_FLAG_NON_SHADOW_SPOTLIGHT,
        90.0,
        0,
        500,
        636.619_8,
        789.752_6,
        500,
        2.494_837,
    ),
    (
        "omni 100 lm stored 25 (LGT_TCS_OmniNS_100)",
        100,
        0,
        0.0,
        0,
        25,
        7.957_747,
        88.297_0,
        25,
        12.474_183,
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_vectors_match_fo76_formulas() {
        for &(name, lumens, shape, fov, high, stored, intensity, auto, written, dimmer) in
            SHARED_VECTORS
        {
            let light = Fo76Light {
                lumens,
                flags: shape | high,
                fov_degrees: fov,
            };
            let got_intensity = light.intensity(1.0);
            assert!(
                (got_intensity - intensity).abs() < 1e-3,
                "{name}: I {got_intensity}"
            );
            let got_auto = auto_radius(got_intensity);
            assert!((got_auto - auto).abs() < 1e-3, "{name}: auto R {got_auto}");
            let got_written = clamp_radius(light.runtime_radius(stored));
            assert_eq!(got_written, written, "{name}: radius");
            let got_dimmer = light.base_dimmer(got_written);
            assert!(
                (got_dimmer - dimmer).abs() < 1e-5,
                "{name}: FNAM {got_dimmer}"
            );
        }
    }

    #[test]
    fn auto_radius_dimmer_is_exactly_one() {
        for lumens in [1u32, 50, 750, 1000, 2000, 4321, 10_000] {
            for shape in [
                0,
                FO4_LIGH_FLAG_SHADOW_HEMISPHERE,
                FO4_LIGH_FLAG_SHADOW_SPOTLIGHT,
            ] {
                let light = Fo76Light {
                    lumens,
                    flags: shape | FO76_LIGH_FLAG_AUTO_RADIUS,
                    fov_degrees: 70.0,
                };
                let radius = clamp_radius(light.runtime_radius(0));
                assert_eq!(
                    light.base_dimmer(radius).to_bits(),
                    1.0f32.to_bits(),
                    "{lumens} {shape:#x}"
                );
            }
        }
    }

    #[test]
    fn auto_radius_past_the_clamp_keeps_intensity_in_the_dimmer() {
        // 80 000 lm spot = 25 465 cd, auto radius 7 897 → written 2048.
        let light = Fo76Light {
            lumens: 80_000,
            flags: FO4_LIGH_FLAG_SHADOW_SPOTLIGHT | FO76_LIGH_FLAG_AUTO_RADIUS,
            fov_degrees: 90.0,
        };
        let radius = clamp_radius(light.runtime_radius(0));
        assert_eq!(radius, 2048);
        let dimmer = light.base_dimmer(radius);
        let metres = 2048.0 / GAME_UNITS_PER_METRE;
        let rebuilt = 5.0 * f64::from(dimmer) * metres * metres;
        assert!((rebuilt - light.intensity(1.0)).abs() / light.intensity(1.0) < 1e-5);
    }

    #[test]
    fn intensity_clamps_and_dimmer_bounds() {
        let flood = Fo76Light {
            lumens: 2_000_000,
            flags: FO4_LIGH_FLAG_SHADOW_SPOTLIGHT,
            fov_degrees: 90.0,
        };
        assert_eq!(flood.intensity(1.0), MAX_INTENSITY_CANDELA);
        assert_eq!(flood.base_dimmer(1), MAX_DIMMER);
        let faint = Fo76Light {
            lumens: 1,
            flags: 0,
            fov_degrees: 0.0,
        };
        assert_eq!(faint.base_dimmer(2048), MIN_DIMMER);
        let dark = Fo76Light {
            lumens: 0,
            flags: 0,
            fov_degrees: 0.0,
        };
        assert_eq!(dark.base_dimmer(300), DEFAULT_DIMMER);
        // A zero-FOV cone-normalised spot falls back to the plain spot constant.
        let degenerate = Fo76Light {
            lumens: 1000,
            flags: FO4_LIGH_FLAG_SHADOW_SPOTLIGHT | FO76_LIGH_FLAG_CONE_NORMALISED_SPOT,
            fov_degrees: 0.0,
        };
        assert!((degenerate.intensity(1.0) - 318.309_9).abs() < 1e-3);
    }

    #[test]
    fn contract_weight_round_trips_every_combination() {
        assert_eq!(contract_weight_from_fo76_flags(0x0000_C001), 0.0);
        assert_eq!(
            contract_weight_from_fo76_flags(FO76_LIGH_FLAG_AUTO_RADIUS),
            1.0
        );
        assert_eq!(
            contract_weight_from_fo76_flags(FO76_LIGH_FLAG_CONE_NORMALISED_SPOT),
            2.0
        );
        assert_eq!(
            contract_weight_from_fo76_flags(FO76_LIGH_FLAG_UNKNOWN_24),
            4.0
        );
        assert_eq!(
            contract_weight_from_fo76_flags(FO76_LIGH_FLAG_UNKNOWN_25),
            8.0
        );
        // LGT_RKE_StreetLight_SpotNS_1000_Warm flags.
        assert_eq!(contract_weight_from_fo76_flags(0x0100_C001), 4.0);
        for code in 0u32..16 {
            let flags = fo76_flags_from_contract_weight(code as f32);
            assert_eq!(contract_weight_from_fo76_flags(flags), code as f32);
        }
        assert_eq!(fo76_flags_from_contract_weight(0.5), 0);
        assert_eq!(fo76_flags_from_contract_weight(16.0), 0);
        assert_eq!(fo76_flags_from_contract_weight(f32::NAN), 0);
    }

    #[test]
    fn placed_multiplier_spreads_a_cone_normalised_spot_over_the_ref_fov() {
        // Whitespring awning lamp REFR 6293BA on LGT_WSR_SpotS_02K_warm 33CAEC:
        // 2000 lm, base FOV 90, XLIG FOV +50.93 and fade 1.6 → FO76 765 cd.
        let awning = Fo76Light {
            lumens: 2000,
            flags: FO4_LIGH_FLAG_SHADOW_SPOTLIGHT | FO76_LIGH_FLAG_CONE_NORMALISED_SPOT,
            fov_degrees: 90.0,
        };
        let multiplier = awning.placed_multiplier(1.6, 50.927_094);
        let intensity = awning.intensity(multiplier);
        assert!((intensity - 765.26).abs() < 0.1, "got {intensity} cd");

        // No FOV delta, a plain spot, or no lumens: the multiplier is FO76's own.
        assert_eq!(awning.placed_multiplier(1.6, 0.0), 1.6);
        let plain = Fo76Light {
            flags: FO4_LIGH_FLAG_SHADOW_SPOTLIGHT,
            ..awning
        };
        assert_eq!(plain.placed_multiplier(1.6, 25.0), 1.6);
        let dark = Fo76Light {
            lumens: 0,
            ..awning
        };
        assert_eq!(dark.placed_multiplier(1.6, 25.0), 1.6);
        assert_eq!(awning.placed_multiplier(1.6, f32::NAN), 1.6);
    }

    #[test]
    fn placed_radius_adds_the_fo76_delta() {
        // LGT_OBS_SpotNS_01K_warm at Fort Atlas: runtime 729.6 − 435 → 295.
        let base = Fo76Light {
            lumens: 1000,
            flags: FO4_LIGH_FLAG_NON_SHADOW_SPOTLIGHT
                | FO76_LIGH_FLAG_AUTO_RADIUS
                | FO76_LIGH_FLAG_CONE_NORMALISED_SPOT,
            fov_degrees: 90.0,
        };
        let r = placed_radius(base.runtime_radius(16), -435.0);
        assert!((r - 294.636).abs() < 1e-2, "got {r}");
        assert_eq!(placed_radius(600.0, 0.0), 600.0);
        assert_eq!(placed_radius(600.0, -1262.7), MIN_RADIUS);
        assert_eq!(placed_radius(2000.0, 500.0), MAX_RADIUS);
        assert_eq!(placed_radius(300.0, f32::NAN), 300.0);
    }

    #[test]
    fn placed_fade_converts_the_fo76_multiplier_to_an_fo4_delta() {
        // Fort Atlas XLIG fade distribution: p10 0.6, p50 1.0, p90 1.6.
        for (multiplier, expected) in [(0.6f32, 0.6f32 - 1.0), (1.0, 0.0), (1.6, 1.6 - 1.0)] {
            assert_eq!(
                placed_fade_delta(1.0, multiplier, 279, None, false),
                expected
            );
            assert_eq!(
                placed_fade_delta(1.0, multiplier, 600, None, true),
                expected
            );
            // An auto-radius base (or one without lumens) ignores the ref radius.
            assert_eq!(
                placed_fade_delta(1.0, multiplier, 730, Some(295.0), false),
                expected
            );
            // FO4's dimmer then equals FO76's multiplier.
            assert!(
                (1.0 + placed_fade_delta(1.0, multiplier, 600, None, true) - multiplier).abs()
                    < 1e-6
            );
        }
        // Per-base FNAM scales the delta.
        assert!((placed_fade_delta(2.5, 1.6, 500, None, true) - 1.5).abs() < 1e-6);
        // A stored-radius ref at half the base radius needs 4x the dimmer.
        let delta = placed_fade_delta(0.866_263, 1.0, 600, Some(300.0), true);
        assert!(
            (0.866_263 + delta - 4.0 * 0.866_263).abs() < 1e-5,
            "got {delta}"
        );
        // Fade 0 turns the light off.
        assert_eq!(placed_fade_delta(2.0, 0.0, 600, Some(300.0), true), -2.0);
    }

    #[test]
    fn placed_xrds_is_the_delta_the_engine_adds_to_the_written_base() {
        // FO4 adds XRDS to the base radius (TESObjectREFR::GetRadius, 1.10.155
        // 0x47e600). REFR 086293BA on LGT_WSR_SpotS_02K_warm (33CAEC): stored base
        // 1631, FO76 delta -1262.7 -> 368.3. Written as absolute it ran at 1999.
        let effective = placed_radius(1631.0, -1262.702);
        assert!((effective - 368.298).abs() < 1e-3, "got {effective}");
        let xrds = placed_xrds(1631, effective);
        assert!((xrds - -1262.702).abs() < 1e-3, "got {xrds}");
        assert_eq!(1631.0 + xrds, effective);
        // The fade keeps FO76's I_base * m at the effective radius, not at 1999.
        let fade = placed_fade_delta(0.400_253, 1.6, 1631, Some(effective), true);
        assert!((0.400_253 + fade - 12.559).abs() < 2e-3, "got {fade}");

        // An auto-radius base rounds its runtime radius when written, and the
        // delta is taken from the written value: 729.64 - 435 = 294.64 on 730.
        let xrds = placed_xrds(730, placed_radius(729.636, -435.0));
        assert!((xrds - -435.364).abs() < 1e-2, "got {xrds}");
        // A non-positive reach lands on the floor, one past the ceiling on 2048.
        assert_eq!(placed_xrds(600, placed_radius(600.0, -1262.7)), MIN_RADIUS - 600.0);
        assert_eq!(placed_xrds(2000, placed_radius(2000.0, 500.0)), MAX_RADIUS - 2000.0);
        assert_eq!(placed_xrds(600, placed_radius(600.0, 0.0)), 0.0);
    }
}
