use super::*;

pub(super) const FO4_LIGH_DATA_RADIUS_OFFSET: usize = 4;
pub(super) const FO76_LIGH_DATA_VALUE_OFFSET: usize = 56;
pub(super) const FO4_LIGH_DATA_NEAR_CLIP_OFFSET: usize = 24;
pub(super) const FO4_LIGH_DATA_SCALAR_OFFSET: usize = 44;
pub(super) const FO4_LIGH_DATA_EXPONENT_OFFSET: usize = 48;
pub(super) const FO4_LIGH_DATA_VALUE_OFFSET: usize = 56;
pub(super) const FO4_LIGH_DATA_WEIGHT_OFFSET: usize = 60;
pub(super) const FO4_LIGH_DATA_LEN: usize = FO4_LIGH_DATA_WEIGHT_OFFSET + 4;
pub(super) const FO4_LIGH_DATA_FLAGS_OFFSET: usize = 12;
pub(super) const FO4_LIGH_DATA_FLICKER_INTENSITY_AMP_OFFSET: usize = 32;
pub(super) const FO4_LIGH_DEFAULT_FADE: f32 = 1.0;
pub(super) const FO4_LIGH_DEFAULT_SCALAR: f32 = 1.0;
pub(super) const FO4_LIGH_DEFAULT_EXPONENT: f32 = 2.0;
pub(super) const FO4_LIGH_DEFAULT_VALUE: u32 = 0;
pub(super) const FO4_LIGH_DEFAULT_WEIGHT: f32 = 0.0;
pub(super) const FO4_LIGH_MAX_SYNTHETIC_RADIUS: u32 = 2048;
pub(super) const FO4_CAGE_BULB_GOBO_PATH: &str = "textures/effects/gobos/cagebulbgobo01_d.dds";
pub(super) const FO4_CAGE_BULB_GOBO_MAX_RADIUS: u32 = 256;
pub(super) const FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP: f32 = 32.0;
/// FO4 flicker-intensity-amplitude ceilings (`DATA` @ +32). FO76 authors this on
/// a far larger scale (flicker lights: median ~3, up to 30000) than FO4, whose
/// flicker lights never exceed 2.0 — and gobo/fire lights stay <= 0.8. Byte-copying
/// the FO76 value makes FO4 read a huge amplitude and strobe the light violently,
/// so it is clamped into FO4's own authored envelope (tighter for gobo lights).
pub(super) const FO4_LIGH_MAX_FLICKER_INTENSITY_AMP: f32 = 2.0;
pub(super) const FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO: f32 = 0.8;
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
/// (0x400000+) that are meaningless in FO4. Mask the flags to FO4's defined range.
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
                    let fallback = if bytes.len() >= FO76_LIGH_DATA_VALUE_OFFSET + 4 {
                        u32::from_le_bytes(
                            bytes[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
                                .try_into()
                                .unwrap(),
                        )
                    } else {
                        1
                    }
                    .clamp(1, FO4_LIGH_MAX_SYNTHETIC_RADIUS);
                    bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                        .copy_from_slice(&fallback.to_le_bytes());
                }
                FieldValue::Struct(fields) => {
                    let radius_index = fields.iter().position(|(name, _)| {
                        Self::struct_field_name_is(interner, *name, "Radius")
                    });
                    let fallback = Self::positive_u32_struct_field(interner, fields, "Value")
                        .unwrap_or(1)
                        .min(FO4_LIGH_MAX_SYNTHETIC_RADIUS);

                    if let Some(index) = radius_index {
                        if Self::field_value_positive_u32(&fields[index].1).is_none() {
                            fields[index].1 = FieldValue::Uint(u64::from(fallback));
                        }
                    } else {
                        fields.push((
                            interner.intern("Radius"),
                            FieldValue::Uint(u64::from(fallback)),
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

        // Gobo/fire lights (a NAM0 mask is present) get FO4's tighter flicker ceiling.
        let max_flicker_intensity_amp = if Self::light_has_gobo(interner, record) {
            FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO
        } else {
            FO4_LIGH_MAX_FLICKER_INTENSITY_AMP
        };

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    Self::normalize_raw_light_flags(bytes);
                    Self::raise_raw_light_near_clip(bytes, FO4_LIGH_MIN_NEAR_CLIP);
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
                    if bytes.len() >= FO4_LIGH_DATA_VALUE_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_VALUE.to_le_bytes());
                    }
                    if bytes.len() >= FO4_LIGH_DATA_WEIGHT_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_WEIGHT_OFFSET..FO4_LIGH_DATA_WEIGHT_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_WEIGHT.to_le_bytes());
                    }
                    if bytes.len() > FO4_LIGH_DATA_LEN {
                        bytes.truncate(FO4_LIGH_DATA_LEN);
                    }
                }
                FieldValue::Struct(fields) => {
                    fields.retain(|(name, _)| {
                        !Self::struct_field_name_is(interner, *name, "Value")
                            && !Self::struct_field_name_is(interner, *name, "Bytes19")
                            && !Self::struct_field_name_is(interner, *name, "Weight")
                    });
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
                    if radius <= FO4_CAGE_BULB_GOBO_MAX_RADIUS {
                        continue;
                    }
                    bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                        .copy_from_slice(&FO4_CAGE_BULB_GOBO_MAX_RADIUS.to_le_bytes());
                    Self::raise_raw_light_near_clip(bytes, FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP);
                }
                FieldValue::Struct(fields) => {
                    let Some(radius_index) = fields.iter().position(|(name, _)| {
                        Self::struct_field_name_is(interner, *name, "Radius")
                    }) else {
                        continue;
                    };
                    let Some(radius) = Self::field_value_positive_u32(&fields[radius_index].1)
                    else {
                        continue;
                    };
                    if radius <= FO4_CAGE_BULB_GOBO_MAX_RADIUS {
                        continue;
                    }
                    fields[radius_index].1 =
                        FieldValue::Uint(u64::from(FO4_CAGE_BULB_GOBO_MAX_RADIUS));
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
