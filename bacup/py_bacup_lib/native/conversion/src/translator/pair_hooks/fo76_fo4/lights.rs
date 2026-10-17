use super::*;
use crate::fixups::fo76_light_model::{
    FO4_LIGH_FLAG_CAN_BE_CARRIED, contract_weight_from_fo76_flags,
};

pub(super) const FO4_LIGH_DATA_RADIUS_OFFSET: usize = 4;
pub(super) const FO4_LIGH_DATA_NEAR_CLIP_OFFSET: usize = 24;
pub(super) const FO4_LIGH_DATA_SCALAR_OFFSET: usize = 44;
pub(super) const FO4_LIGH_DATA_EXPONENT_OFFSET: usize = 48;
/// FO4 reads `DATA` @ +52 as God Rays - Near Clip; FO76 stores the light's
/// **colour temperature in Kelvin** in that slot (sampled values are exclusively
/// 1800/2500/2700/3000/3500/4000/4500/5000/5500/6000/7000/7500/8400, nonzero on
/// 83% of lights). Byte-copied, the raw Kelvin integer reinterprets as a
/// denormal float — 4000 becomes 5.6e-42 — where every vanilla FO4 light uses
/// either 10.0 or nothing. The FO76 colour is already baked into the `DATA` RGB,
/// so the Kelvin carries no information FO4 needs and is simply cleared.
pub(super) const FO4_LIGH_DATA_GOD_RAYS_NEAR_CLIP_OFFSET: usize = 52;
/// FO4 `Value` (u32). FO76 stores its lumens in the same slot as a u32 integer
/// (`LGT_TCS_OmniNS_100_StreetLight` = 100), kept for FO4CS; see
/// `fixups::fo76_light_model`.
pub(super) const FO4_LIGH_DATA_VALUE_OFFSET: usize = 56;
/// FO4 `Weight` (f32), which carries the FO76 high-flag contract.
pub(super) const FO4_LIGH_DATA_WEIGHT_OFFSET: usize = 60;
pub(super) const FO4_LIGH_DATA_LEN: usize = FO4_LIGH_DATA_WEIGHT_OFFSET + 4;
pub(super) const FO4_LIGH_DATA_FLAGS_OFFSET: usize = 12;
pub(super) const FO4_LIGH_DATA_FLICKER_PERIOD_OFFSET: usize = 28;
pub(super) const FO4_LIGH_DATA_FLICKER_INTENSITY_AMP_OFFSET: usize = 32;
pub(super) const FO4_LIGH_DEFAULT_FADE: f32 = 1.0;
pub(super) const FO4_LIGH_DEFAULT_SCALAR: f32 = 1.0;
pub(super) const FO4_LIGH_DEFAULT_EXPONENT: f32 = 2.0;
pub(super) const FO4_LIGH_DEFAULT_VALUE: u32 = 0;
pub(super) const FO4_LIGH_DEFAULT_WEIGHT: f32 = 0.0;
pub(super) const FO4_LIGH_DEFAULT_GOD_RAYS_NEAR_CLIP: f32 = 0.0;
/// A zero radius is a degenerate bounding volume. The FO76 runtime radius is
/// written later by `fixups::normalize_light_radii`.
pub(super) const FO4_LIGH_MIN_RADIUS: u32 = 1;
pub(super) const FO4_CAGE_BULB_GOBO_PATH: &str = "textures/effects/gobos/cagebulbgobo01_d.dds";
/// Cage-bulb gobo lights wider than this get the larger near clip below.
pub(super) const FO4_CAGE_BULB_GOBO_NEAR_CLIP_MIN_RADIUS: u32 = 256;
pub(super) const FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP: f32 = 32.0;
/// FO4 flicker-intensity-amplitude ceilings (`DATA` @ +32). FO76 authors this on
/// a far larger scale (flicker lights: median ~3, up to 30000) than FO4, whose
/// flicker lights never exceed 2.0 — and gobo/fire lights stay <= 0.8. Byte-copying
/// the FO76 value makes FO4 read a huge amplitude and strobe the light violently,
/// so it is clamped into FO4's own authored envelope (tighter for gobo lights).
pub(super) const FO4_LIGH_MAX_FLICKER_INTENSITY_AMP: f32 = 2.0;
pub(super) const FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO: f32 = 0.8;
/// FO4's comparable non-shadow fire lights author flicker periods around
/// 0.33-0.36 and intensity amplitudes no higher than 0.8. FO76 fire lights
/// commonly carry 1.0/2.0, which FO4 renders as a rapid, harsh strobe.
pub(super) const FO4_LIGH_MAX_FIRE_FLICKER_PERIOD: f32 = 0.363_636_37;
pub(super) const FO4_LIGH_MAX_FIRE_FLICKER_INTENSITY_AMP: f32 = 0.8;
/// Near-clip floor for a converted light (`DATA` @ +24). FO76 leaves near clip at
/// 1.0 on almost every light; FO4's population median is ~7.2, and a near plane
/// this small wrecks shadow-map depth precision. FO76 carries no signal for FO4's
/// intended value, so a floor at the FO4 median is the safe heuristic.
pub(super) const FO4_LIGH_MIN_NEAR_CLIP: f32 = 7.217;
/// `non_specular` bit (`DATA` flags @ +12). FO76 sets it on ~96% of lights (their
/// PBR pipeline drives specular separately); FO4 leaves it clear on ~82%. Byte-
/// copying it disables specular highlights on nearly every converted light, so it
/// is cleared to restore FO4-native specular.
pub(super) const FO4_LIGH_FLAGS_NON_SPECULAR: u32 = 0x0000_8000;
pub(super) const FO4_LIGH_FLAGS_SHADOW_SPOTLIGHT: u32 = 0x0000_0400;
/// FO76 commonly pairs `attenuation_only` with shadow spotlights, but carrying
/// that combination into FO4 washes out projected lights instead of preserving it.
pub(super) const FO4_LIGH_FLAGS_ATTENUATION_ONLY: u32 = 0x0001_0000;
/// FO4 defines LIGH `DATA` flag bits only up to 0x200000; FO76 sets higher bits
/// (0x400000+) that the FO4 engine must not see. They are masked off the flag
/// word and survive in the `Weight` contract instead.
pub(super) const FO4_LIGH_FLAGS_VALID_MASK: u32 = 0x003F_FFFF;
impl Fo76Fo4Hook {
    pub(super) fn ensure_light_radius(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"LIGH" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    if bytes.len() < FO4_LIGH_DATA_RADIUS_OFFSET + 4 {
                        continue;
                    }
                    let radius = u32::from_le_bytes(
                        bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                            .try_into()
                            .unwrap(),
                    );
                    if radius > 0 {
                        continue;
                    }
                    bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                        .copy_from_slice(&FO4_LIGH_MIN_RADIUS.to_le_bytes());
                }
                FieldValue::Struct(fields) => {
                    let radius_index = fields.iter().position(|(name, _)| {
                        Self::struct_field_name_is(interner, *name, "Radius")
                    });

                    if let Some(index) = radius_index {
                        if Self::field_value_positive_u32(&fields[index].1).is_none() {
                            fields[index].1 = FieldValue::Uint(u64::from(FO4_LIGH_MIN_RADIUS));
                        }
                    } else {
                        fields.push((
                            interner.intern("Radius"),
                            FieldValue::Uint(u64::from(FO4_LIGH_MIN_RADIUS)),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn normalize_light_data_for_fo4(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"LIGH" {
            return;
        }

        let is_fire_light = Self::is_fire_light(interner, record);
        let max_flicker_intensity_amp = if Self::light_has_gobo(interner, record) {
            FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO
        } else if is_fire_light {
            FO4_LIGH_MAX_FIRE_FLICKER_INTENSITY_AMP
        } else {
            FO4_LIGH_MAX_FLICKER_INTENSITY_AMP
        };

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    Self::write_raw_light_photometric_contract(bytes);
                    Self::normalize_raw_light_flags(bytes);
                    Self::raise_raw_light_near_clip(bytes, FO4_LIGH_MIN_NEAR_CLIP);
                    if is_fire_light {
                        Self::clamp_raw_light_float_max(
                            bytes,
                            FO4_LIGH_DATA_FLICKER_PERIOD_OFFSET,
                            FO4_LIGH_MAX_FIRE_FLICKER_PERIOD,
                        );
                    }
                    Self::clamp_raw_light_float_max(
                        bytes,
                        FO4_LIGH_DATA_FLICKER_INTENSITY_AMP_OFFSET,
                        max_flicker_intensity_amp,
                    );
                    if bytes.len() >= FO4_LIGH_DATA_SCALAR_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_SCALAR_OFFSET..FO4_LIGH_DATA_SCALAR_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_SCALAR.to_le_bytes());
                    }
                    if bytes.len() >= FO4_LIGH_DATA_EXPONENT_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_EXPONENT_OFFSET..FO4_LIGH_DATA_EXPONENT_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_EXPONENT.to_le_bytes());
                    }
                    if bytes.len() >= FO4_LIGH_DATA_GOD_RAYS_NEAR_CLIP_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_GOD_RAYS_NEAR_CLIP_OFFSET
                            ..FO4_LIGH_DATA_GOD_RAYS_NEAR_CLIP_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_GOD_RAYS_NEAR_CLIP.to_le_bytes());
                    }
                    if bytes.len() > FO4_LIGH_DATA_LEN {
                        bytes.truncate(FO4_LIGH_DATA_LEN);
                    }
                }
                FieldValue::Struct(fields) => {
                    fields.retain(|(name, _)| {
                        !Self::struct_field_name_is(interner, *name, "Bytes19")
                    });
                    Self::write_struct_light_photometric_contract(interner, fields);
                    Self::set_struct_float_field(
                        interner,
                        fields,
                        "GodRaysNearClip",
                        FO4_LIGH_DEFAULT_GOD_RAYS_NEAR_CLIP,
                    );
                    Self::ensure_struct_float_field(
                        interner,
                        fields,
                        "Scalar",
                        FO4_LIGH_DEFAULT_SCALAR,
                    );
                    Self::ensure_struct_float_field(
                        interner,
                        fields,
                        "Exponent",
                        FO4_LIGH_DEFAULT_EXPONENT,
                    );
                    Self::clamp_struct_float_field(
                        interner,
                        fields,
                        "FlickerEffectIntensityAmplitude",
                        max_flicker_intensity_amp,
                    );
                    if is_fire_light {
                        Self::clamp_struct_float_field(
                            interner,
                            fields,
                            "FlickerEffectPeriod",
                            FO4_LIGH_MAX_FIRE_FLICKER_PERIOD,
                        );
                    }
                    Self::raise_struct_float_field(
                        interner,
                        fields,
                        "NearClip",
                        FO4_LIGH_MIN_NEAR_CLIP,
                    );
                    Self::normalize_struct_light_flags(interner, fields);
                }
                _ => {}
            }
        }
    }

    /// FO4CS [Fo76Lighting] photometric contract (see `fixups::fo76_light_model`):
    /// FO76 lumens stay in `Value` (@ +56, u32) and the FO76 flags above FO4's
    /// range move into `Weight` (@ +60, f32) before the flag word is masked.
    /// A carryable light's `Value`/`Weight` are FO4 item stats, so they are reset.
    pub(super) fn write_raw_light_photometric_contract(bytes: &mut [u8]) {
        if bytes.len() < FO4_LIGH_DATA_WEIGHT_OFFSET + 4 {
            return;
        }
        let flags = u32::from_le_bytes(
            bytes[FO4_LIGH_DATA_FLAGS_OFFSET..FO4_LIGH_DATA_FLAGS_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        let weight = if flags & FO4_LIGH_FLAG_CAN_BE_CARRIED != 0 {
            bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
                .copy_from_slice(&FO4_LIGH_DEFAULT_VALUE.to_le_bytes());
            FO4_LIGH_DEFAULT_WEIGHT
        } else {
            contract_weight_from_fo76_flags(flags)
        };
        bytes[FO4_LIGH_DATA_WEIGHT_OFFSET..FO4_LIGH_DATA_WEIGHT_OFFSET + 4]
            .copy_from_slice(&weight.to_le_bytes());
    }

    /// Struct-branch counterpart of `write_raw_light_photometric_contract`, for an
    /// integer-valued `Flags` field.
    pub(super) fn write_struct_light_photometric_contract(
        interner: &crate::sym::StringInterner,
        fields: &mut Vec<(crate::sym::Sym, FieldValue)>,
    ) {
        let flags = fields
            .iter()
            .find(|(name, _)| Self::struct_field_name_is(interner, *name, "Flags"))
            .and_then(|(_, value)| match value {
                FieldValue::Uint(flags) => u32::try_from(*flags).ok(),
                FieldValue::Int(flags) => u32::try_from(*flags).ok(),
                _ => None,
            })
            .unwrap_or(0);
        let weight = if flags & FO4_LIGH_FLAG_CAN_BE_CARRIED != 0 {
            if let Some((_, value)) = fields
                .iter_mut()
                .find(|(name, _)| Self::struct_field_name_is(interner, *name, "Value"))
            {
                *value = FieldValue::Uint(u64::from(FO4_LIGH_DEFAULT_VALUE));
            }
            FO4_LIGH_DEFAULT_WEIGHT
        } else {
            contract_weight_from_fo76_flags(flags)
        };
        Self::set_struct_float_field(interner, fields, "Weight", weight);
    }

    /// Whether a LIGH carries a projected-light mask (`NAM0` gobo) — the signal
    /// used to pick FO4's tighter flicker ceiling for gobo/fire lights.
    pub(super) fn light_has_gobo(interner: &crate::sym::StringInterner, record: &Record) -> bool {
        record.fields.iter().any(|entry| {
            if entry.sig.0 != *b"NAM0" {
                return false;
            }
            match &entry.value {
                FieldValue::String(sym) => interner
                    .resolve(*sym)
                    .is_some_and(|path| !path.trim().is_empty()),
                FieldValue::Bytes(bytes) => !trim_nul_suffix(bytes.as_slice()).is_empty(),
                _ => false,
            }
        })
    }

    pub(super) fn is_fire_light(interner: &crate::sym::StringInterner, record: &Record) -> bool {
        record
            .eid
            .and_then(|eid| interner.resolve(eid))
            .is_some_and(|editor_id| {
                let editor_id = editor_id.to_ascii_lowercase();
                editor_id.contains("fire") && editor_id.contains("flicker")
            })
    }

    pub(super) fn normalize_light_flags_for_fo4(flags: u32) -> u32 {
        let mut normalized = (flags & FO4_LIGH_FLAGS_VALID_MASK) & !FO4_LIGH_FLAGS_NON_SPECULAR;
        if normalized & FO4_LIGH_FLAGS_SHADOW_SPOTLIGHT != 0 {
            normalized &= !FO4_LIGH_FLAGS_ATTENUATION_ONLY;
        }
        normalized
    }

    /// Clear FO76's incompatible flag combinations from a raw `DATA` blob so the
    /// converted light uses FO4-native rendering behavior.
    pub(super) fn normalize_raw_light_flags(bytes: &mut [u8]) {
        if bytes.len() < FO4_LIGH_DATA_FLAGS_OFFSET + 4 {
            return;
        }
        let range = FO4_LIGH_DATA_FLAGS_OFFSET..FO4_LIGH_DATA_FLAGS_OFFSET + 4;
        let flags = u32::from_le_bytes(bytes[range.clone()].try_into().unwrap());
        let normalized = Self::normalize_light_flags_for_fo4(flags);
        bytes[range].copy_from_slice(&normalized.to_le_bytes());
    }

    /// Clamp a raw `DATA` float field down to `maximum` (also normalizes a
    /// non-finite value to `maximum`). Mirror of `raise_raw_light_near_clip`.
    pub(super) fn clamp_raw_light_float_max(bytes: &mut [u8], offset: usize, maximum: f32) {
        if bytes.len() < offset + 4 {
            return;
        }
        let current = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        if current.is_finite() && current <= maximum {
            return;
        }
        bytes[offset..offset + 4].copy_from_slice(&maximum.to_le_bytes());
    }

    /// Struct-branch counterpart of `clamp_raw_light_float_max`. Leaves an absent
    /// field alone (a missing flicker amplitude means the field simply isn't there).
    pub(super) fn clamp_struct_float_field(
        interner: &crate::sym::StringInterner,
        fields: &mut [(crate::sym::Sym, FieldValue)],
        field_name: &str,
        maximum: f32,
    ) {
        let Some(index) = fields
            .iter()
            .position(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
        else {
            return;
        };
        let needs_clamp = match &fields[index].1 {
            FieldValue::Float(value) => !value.is_finite() || *value > maximum,
            _ => false,
        };
        if needs_clamp {
            fields[index].1 = FieldValue::Float(maximum);
        }
    }

    /// Struct-branch counterpart of `normalize_raw_light_flags`, best-effort for an
    /// integer-valued `Flags` field; other representations are left untouched.
    pub(super) fn normalize_struct_light_flags(
        interner: &crate::sym::StringInterner,
        fields: &mut [(crate::sym::Sym, FieldValue)],
    ) {
        for (name, value) in fields.iter_mut() {
            if !Self::struct_field_name_is(interner, *name, "Flags") {
                continue;
            }
            let current = match value {
                FieldValue::Uint(v) => u32::try_from(*v).ok(),
                FieldValue::Int(v) => u32::try_from(*v).ok(),
                _ => None,
            };
            if let Some(flags) = current {
                let normalized = Self::normalize_light_flags_for_fo4(flags);
                *value = FieldValue::Uint(u64::from(normalized));
            }
        }
    }

    pub(super) fn normalize_cage_bulb_gobo_light_for_fo4(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"LIGH" || !Self::record_has_cage_bulb_gobo(interner, record) {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    if bytes.len() < FO4_LIGH_DATA_RADIUS_OFFSET + 4 {
                        continue;
                    }
                    let radius = u32::from_le_bytes(
                        bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                            .try_into()
                            .unwrap(),
                    );
                    if radius <= FO4_CAGE_BULB_GOBO_NEAR_CLIP_MIN_RADIUS {
                        continue;
                    }
                    Self::raise_raw_light_near_clip(bytes, FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP);
                }
                FieldValue::Struct(fields) => {
                    let Some(radius) = fields
                        .iter()
                        .find(|(name, _)| Self::struct_field_name_is(interner, *name, "Radius"))
                        .and_then(|(_, radius)| Self::field_value_positive_u32(radius))
                    else {
                        continue;
                    };
                    if radius <= FO4_CAGE_BULB_GOBO_NEAR_CLIP_MIN_RADIUS {
                        continue;
                    }
                    Self::raise_struct_float_field(
                        interner,
                        fields,
                        "NearClip",
                        FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP,
                    );
                }
                _ => {}
            }
        }
    }

    pub(super) fn record_has_cage_bulb_gobo(
        interner: &crate::sym::StringInterner,
        record: &Record,
    ) -> bool {
        record.fields.iter().any(|entry| {
            if entry.sig.0 != *b"NAM0" {
                return false;
            }
            match &entry.value {
                FieldValue::String(sym) => interner
                    .resolve(*sym)
                    .is_some_and(Self::is_cage_bulb_gobo_path),
                FieldValue::Bytes(bytes) => std::str::from_utf8(trim_nul_suffix(bytes.as_slice()))
                    .ok()
                    .is_some_and(Self::is_cage_bulb_gobo_path),
                _ => false,
            }
        })
    }

    pub(super) fn is_cage_bulb_gobo_path(path: &str) -> bool {
        let normalized = path.replace('\\', "/").to_ascii_lowercase();
        let without_data = normalized
            .strip_prefix("data/")
            .unwrap_or(normalized.as_str());
        without_data == FO4_CAGE_BULB_GOBO_PATH
    }

    pub(super) fn raise_raw_light_near_clip(bytes: &mut [u8], minimum: f32) {
        if bytes.len() < FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4 {
            return;
        }
        let current = f32::from_le_bytes(
            bytes[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        if current.is_finite() && current >= minimum {
            return;
        }
        bytes[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
            .copy_from_slice(&minimum.to_le_bytes());
    }

    pub(super) fn ensure_light_fade_value(record: &mut Record) {
        if record.sig.0 != *b"LIGH" || record.fields.iter().any(|entry| entry.sig.0 == *b"FNAM") {
            return;
        }

        let fade = FieldEntry {
            sig: SubrecordSig(*b"FNAM"),
            value: FieldValue::Float(FO4_LIGH_DEFAULT_FADE),
        };
        if let Some(data_index) = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"DATA")
        {
            record.fields.insert(data_index + 1, fade);
        } else {
            record.fields.push(fade);
        }
    }

    /// Overwrite a struct float field, inserting it when absent. Unlike
    /// `ensure_struct_float_field` (which only fills a gap) this always wins —
    /// used where the carried FO76 value is meaningless in FO4.
    pub(super) fn set_struct_float_field(
        interner: &crate::sym::StringInterner,
        fields: &mut Vec<(crate::sym::Sym, FieldValue)>,
        field_name: &str,
        value: f32,
    ) {
        match fields
            .iter()
            .position(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
        {
            Some(index) => fields[index].1 = FieldValue::Float(value),
            None => fields.push((interner.intern(field_name), FieldValue::Float(value))),
        }
    }

    pub(super) fn ensure_struct_float_field(
        interner: &crate::sym::StringInterner,
        fields: &mut Vec<(crate::sym::Sym, FieldValue)>,
        field_name: &str,
        default: f32,
    ) {
        if fields
            .iter()
            .any(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
        {
            return;
        }

        fields.push((interner.intern(field_name), FieldValue::Float(default)));
    }

    pub(super) fn raise_struct_float_field(
        interner: &crate::sym::StringInterner,
        fields: &mut Vec<(crate::sym::Sym, FieldValue)>,
        field_name: &str,
        minimum: f32,
    ) {
        let Some(index) = fields
            .iter()
            .position(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
        else {
            fields.push((interner.intern(field_name), FieldValue::Float(minimum)));
            return;
        };

        let needs_raise = match &fields[index].1 {
            FieldValue::Float(value) => !value.is_finite() || *value < minimum,
            _ => true,
        };
        if needs_raise {
            fields[index].1 = FieldValue::Float(minimum);
        }
    }

    pub(super) fn field_value_positive_u32(value: &FieldValue) -> Option<u32> {
        match value {
            FieldValue::Uint(value) => u32::try_from(*value).ok().filter(|value| *value > 0),
            FieldValue::Int(value) => u32::try_from(*value).ok().filter(|value| *value > 0),
            FieldValue::Float(value) if *value > 0.0 => Some(*value as u32),
            _ => None,
        }
    }
}
