//! Translator orchestrator — top-level skeleton.
//!
//! `Translator` drives the per-record translation pipeline:
//!   1. PairHook::pre_translate
//!   2. Field rewrites + named transforms (from TranslationMaps + TransformRegistry)
//!   3. PairHook::post_translate
//!   4. TargetHook::run
//!
//! Field rewrites, drop lists, and named transforms are dispatched from
//! `TranslationMaps`; per-pair and per-target hooks are wired via
//! `pair_hook_for` / `target_hook_for`.

pub mod ammo_substitute;
pub mod class_a_normalize;
pub mod maps;
pub mod pair_hook;
pub mod pair_hooks;
pub mod target_hook;
pub mod target_hooks;
pub mod transforms;

use super::errors::ConfigError;
use super::formkey_mapper::FormKeyMapper;
use super::ids::SubrecordSig;
use super::record::{FieldEntry, FieldValue, Record};
use super::sym::{StringInterner, Sym};
use maps::{RecordRoutePolicy, TranslationMaps};
use pair_hook::{NoOpPairHook, PairHook};
use target_hook::{NoOpTargetHook, TargetHook};
use transforms::{TransformCtx, TransformRegistry};

fn pair_hook_for(source: Game, target: Game) -> Box<dyn PairHook> {
    match (source, target) {
        (Game::Fo3, Game::Fo4) => Box::new(pair_hooks::fnv_fo4::Fo3Fo4Hook),
        (Game::Fnv, Game::Fo4) => Box::new(pair_hooks::fnv_fo4::FnvFo4Hook),
        (Game::Fo76, Game::Fo4) => Box::new(pair_hooks::fo76_fo4::Fo76Fo4Hook),
        (Game::SkyrimSe, Game::Fo4) => Box::new(pair_hooks::skyrimse_fo4::SkyrimSeFo4Hook),
        (Game::Starfield, Game::Fo4) => Box::new(pair_hooks::starfield_fo4::StarfieldFo4Hook),
        (Game::Fo4, Game::Starfield) => Box::new(pair_hooks::fo4_starfield::Fo4StarfieldHook),
        _ => Box::new(NoOpPairHook),
    }
}

fn target_hook_for(target: Game) -> Box<dyn TargetHook> {
    match target {
        Game::Fo4 => Box::new(target_hooks::fo4::Fo4TargetHook),
        _ => Box::new(NoOpTargetHook),
    }
}

/// Supported Bethesda games for the conversion pipeline.
///
/// Serialised game strings (used as map-file name components) are produced by
/// `Game::as_str()`. `Game::from_str` matches the same lowercase identifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Game {
    Fo3,
    Fnv,
    Fo4,
    Fo76,
    Skyrim,
    SkyrimSe,
    Starfield,
    Oblivion,
}

impl Game {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "fo3" => Some(Self::Fo3),
            "fnv" => Some(Self::Fnv),
            "fo4" => Some(Self::Fo4),
            "fo76" => Some(Self::Fo76),
            "skyrim" => Some(Self::Skyrim),
            "skyrimse" => Some(Self::SkyrimSe),
            "starfield" => Some(Self::Starfield),
            "oblivion" => Some(Self::Oblivion),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fo3 => "fo3",
            Self::Fnv => "fnv",
            Self::Fo4 => "fo4",
            Self::Fo76 => "fo76",
            Self::Skyrim => "skyrim",
            Self::SkyrimSe => "skyrimse",
            Self::Starfield => "starfield",
            Self::Oblivion => "oblivion",
        }
    }
}

/// A deferred-translation reason — records that need a separate pipeline pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeferredKind {
    /// FNV legacy Papyrus scripting requires a separate script-port pass.
    FnvLegacyScripting,
    /// Record requires the V2 pipeline (not yet implemented).
    V2Pipeline,
}

/// Minimal decision attached to a Dropped or Deferred result.
#[derive(Debug)]
pub struct Decision {
    pub kind: Sym,
    pub message: String,
}

/// The outcome of translating one record.
#[derive(Debug)]
pub enum TranslateResult {
    /// Record was translated; the inner `Record` is the translated version.
    Translated(Record),
    /// Record was explicitly dropped (e.g. in the skip list or by a hook).
    Dropped { reason: Sym, decision: Decision },
    /// Record cannot be translated in this pass — needs a different pipeline.
    Deferred(DeferredKind),
}

/// Orchestrates record translation for one (source, target) game pair.
///
/// Holds:
/// - The loaded `TranslationMaps` for the pair.
/// - A `TransformRegistry` with registered named transforms.
/// - A `PairHook` (game-pair-specific logic, defaults to no-op).
/// - A `TargetHook` (target-game post-processing, defaults to no-op).
pub struct Translator {
    pub source: Game,
    pub target: Game,
    pub maps: TranslationMaps,
    pub transforms: TransformRegistry,
    pair_hook: Box<dyn PairHook>,
    target_hook: Box<dyn TargetHook>,
}

fn projection_has_field(record: &Record, signature: &str) -> bool {
    record
        .fields
        .iter()
        .any(|field| field.sig.as_str() == signature)
}

fn push_projection_field(record: &mut Record, signature: &[u8; 4], value: FieldValue) {
    if !projection_has_field(
        record,
        std::str::from_utf8(signature).expect("4CC is ASCII"),
    ) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*signature),
            value,
        });
    }
}

fn ensure_empty_keyword_array(record: &mut Record) {
    push_projection_field(record, b"KSIZ", FieldValue::Uint(0));
    push_projection_field(record, b"KWDA", FieldValue::List(Vec::new()));
}

fn lgtm_ambient_component(record: &Record, name: &str, interner: &StringInterner) -> Option<u64> {
    let data = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "DATA")?;
    match &data.value {
        FieldValue::Struct(fields) => fields.iter().find_map(|(key, value)| {
            (interner.resolve(*key) == Some(name)).then(|| match value {
                FieldValue::Uint(value) => Some(*value),
                FieldValue::Int(value) if *value >= 0 => Some(*value as u64),
                _ => None,
            })?
        }),
        FieldValue::Bytes(bytes) => {
            let index = match name {
                "ambient_color_red" => 0,
                "ambient_color_green" => 1,
                "ambient_color_blue" => 2,
                _ => return None,
            };
            bytes.get(index).copied().map(u64::from)
        }
        _ => None,
    }
}

fn synthesize_lgtm_dalc(
    record: &mut Record,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    if projection_has_field(record, "DALC") {
        return Ok(());
    }
    let red = lgtm_ambient_component(record, "ambient_color_red", interner)
        .ok_or("lgtm_missing_ambient_color")?;
    let green = lgtm_ambient_component(record, "ambient_color_green", interner)
        .ok_or("lgtm_missing_ambient_color")?;
    let blue = lgtm_ambient_component(record, "ambient_color_blue", interner)
        .ok_or("lgtm_missing_ambient_color")?;
    let schema =
        crate::schema::AuthoringSchema::for_game("fo4").map_err(|_| "fo4_schema_unavailable")?;
    let dalc = schema
        .record_def("LGTM")
        .and_then(|definition| definition.subrecord_def("DALC"))
        .ok_or("fo4_lgtm_dalc_schema_unavailable")?;
    let mut fields = Vec::with_capacity(dalc.fields.len());
    for field in &dalc.fields {
        let value = if field.id.starts_with("ambient_colors_directional_") {
            let component = if field.id.contains("_red") {
                red
            } else if field.id.contains("_green") {
                green
            } else if field.id.contains("_blue") {
                blue
            } else {
                return Err("fo4_lgtm_dalc_color_unregistered");
            };
            FieldValue::Uint(component)
        } else if field.id == "ambient_colors_fresnel_power" {
            FieldValue::Float(1.0)
        } else {
            FieldValue::Uint(0)
        };
        fields.push((interner.intern(&field.id), value));
    }
    push_projection_field(record, b"DALC", FieldValue::Struct(fields));
    Ok(())
}

fn neutral_imad_array_value(
    definition: &crate::schema::SubrecordDef,
    interner: &StringInterner,
) -> Result<FieldValue, &'static str> {
    if !definition
        .codec
        .as_deref()
        .is_some_and(|codec| codec.starts_with("array_struct:"))
        || definition.fields.is_empty()
        || definition
            .fields
            .iter()
            .any(|field| field.kind != "float32")
    {
        return Err("fo4_imad_array_shape_unregistered");
    }

    let rows = [0.0_f32, 1.0_f32]
        .into_iter()
        .map(|time| {
            FieldValue::Struct(
                definition
                    .fields
                    .iter()
                    .map(|field| {
                        let value = if field.id.ends_with("_time") {
                            time
                        } else if field.id.contains("_mult_value") {
                            1.0
                        } else {
                            0.0
                        };
                        (interner.intern(&field.id), FieldValue::Float(value))
                    })
                    .collect(),
            )
        })
        .collect();
    Ok(FieldValue::List(rows))
}

fn neutral_imad_data_value(
    definition: &crate::schema::SubrecordDef,
    interner: &StringInterner,
) -> Result<FieldValue, &'static str> {
    let count_fields = [
        "tint_color",
        "blur_radius",
        "double_vision_strength",
        "radial_blur_strength",
        "radial_blur_ramp_up",
        "radial_blur_start",
        "dof_strength",
        "dof_distance",
        "dof_range",
        "radial_blur_ramp_down",
        "radial_blur_down_start",
        "fade_color",
        "motion_blur_strength",
        "vignette_radius",
        "vignette_strength",
    ];
    let mut fields = Vec::with_capacity(definition.fields.len());
    for field in &definition.fields {
        let value = match (field.id.as_str(), field.kind.as_str()) {
            ("animatable", "uint32") => FieldValue::Uint(1),
            ("duration", "float32") => FieldValue::Float(1.0),
            ("radial_blur_center_x" | "radial_blur_center_y", "float32") => FieldValue::Float(0.5),
            (name, "uint32") if name.contains("_count") || count_fields.contains(&name) => {
                FieldValue::Uint(2)
            }
            (_, "uint32" | "uint8") => FieldValue::Uint(0),
            _ => return Err("fo4_imad_data_field_unregistered"),
        };
        fields.push((interner.intern(&field.id), value));
    }
    Ok(FieldValue::Struct(fields))
}

fn neutralize_legacy_imad(
    record: &mut Record,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    let schema =
        crate::schema::AuthoringSchema::for_game("fo4").map_err(|_| "fo4_schema_unavailable")?;
    let definition = schema
        .record_def("IMAD")
        .ok_or("fo4_imad_schema_unavailable")?;
    record.fields.retain(|field| field.sig.as_str() == "EDID");
    for subrecord in definition
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.required)
    {
        let bytes: [u8; 4] = subrecord
            .id
            .as_bytes()
            .try_into()
            .map_err(|_| "fo4_imad_required_signature_invalid")?;
        let value = match subrecord.id.as_str() {
            "DNAM" => neutral_imad_data_value(subrecord, interner)?,
            _ if subrecord
                .codec
                .as_deref()
                .is_some_and(|codec| codec.starts_with("array_struct:")) =>
            {
                neutral_imad_array_value(subrecord, interner)?
            }
            _ => return Err("fo4_imad_required_default_unregistered"),
        };
        push_projection_field(record, &bytes, value);
    }
    record
        .warnings
        .push(interner.intern("legacy_imad_neutral_target_replacement"));
    Ok(())
}

fn normalize_legacy_misc_data(record: &mut Record) -> Result<(), &'static str> {
    let Some(data) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.as_str() == "DATA")
    else {
        return Ok(());
    };
    data.value = match &data.value {
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            let mut target = bytes.to_vec();
            target.extend_from_slice(&0.0_f32.to_le_bytes());
            FieldValue::Bytes(target.into())
        }
        FieldValue::Int(value) => {
            let mut target = (*value as i32).to_le_bytes().to_vec();
            target.extend_from_slice(&0.0_f32.to_le_bytes());
            FieldValue::Bytes(target.into())
        }
        FieldValue::Uint(value) => {
            let mut target = (*value as u32).to_le_bytes().to_vec();
            target.extend_from_slice(&0.0_f32.to_le_bytes());
            FieldValue::Bytes(target.into())
        }
        value @ FieldValue::Bytes(_) => value.clone(),
        FieldValue::Struct(fields) if fields.len() == 1 => {
            let value = match &fields[0].1 {
                FieldValue::Int(value) => *value as i32,
                FieldValue::Uint(value) => {
                    i32::try_from(*value).map_err(|_| "legacy_misc_value_out_of_range")?
                }
                _ => return Err("legacy_misc_data_shape_unverified"),
            };
            let mut target = value.to_le_bytes().to_vec();
            target.extend_from_slice(&0.0_f32.to_le_bytes());
            FieldValue::Bytes(target.into())
        }
        value @ FieldValue::Struct(_) => value.clone(),
        _ => return Err("legacy_misc_data_shape_unverified"),
    };
    Ok(())
}

fn complete_skyrim_imad_arrays(
    record: &mut Record,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    {
        let mut dnam_entries = record
            .fields
            .iter_mut()
            .filter(|field| field.sig.as_str() == "DNAM");
        let dnam = dnam_entries.next().ok_or("skyrim_imad_dnam_missing")?;
        if dnam_entries.next().is_some() {
            return Err("skyrim_imad_dnam_duplicate");
        }
        let FieldValue::Struct(fields) = &mut dnam.value else {
            return Err("skyrim_imad_dnam_shape_unverified");
        };
        for count_field in ["vignette_radius", "vignette_strength"] {
            let value = fields
                .iter_mut()
                .find_map(|(name, value)| {
                    (interner.resolve(*name) == Some(count_field)).then_some(value)
                })
                .ok_or("skyrim_imad_dnam_count_missing")?;
            match value {
                FieldValue::Uint(_) | FieldValue::Int(_) => *value = FieldValue::Uint(2),
                _ => return Err("skyrim_imad_dnam_count_shape_unverified"),
            }
        }
    }

    let schema =
        crate::schema::AuthoringSchema::for_game("fo4").map_err(|_| "fo4_schema_unavailable")?;
    let definition = schema
        .record_def("IMAD")
        .ok_or("fo4_imad_schema_unavailable")?;
    for signature in ["NAM5", "NAM6"] {
        let subrecord = definition
            .subrecord_def(signature)
            .ok_or("fo4_imad_array_schema_unavailable")?;
        let bytes: [u8; 4] = signature
            .as_bytes()
            .try_into()
            .map_err(|_| "fo4_imad_required_signature_invalid")?;
        push_projection_field(
            record,
            &bytes,
            neutral_imad_array_value(subrecord, interner)?,
        );
    }
    Ok(())
}

fn ensure_regn_lod_distance(record: &mut Record) {
    let mut output = smallvec::SmallVec::new();
    let mut in_region_data = false;
    let mut has_lod_distance = false;
    for field in std::mem::take(&mut record.fields) {
        if field.sig.as_str() == "RDAT" {
            if in_region_data && !has_lod_distance {
                output.push(FieldEntry {
                    sig: SubrecordSig(*b"RLDM"),
                    value: FieldValue::Float(4096.0),
                });
            }
            in_region_data = true;
            has_lod_distance = false;
        } else if in_region_data && field.sig.as_str() == "RLDM" {
            has_lod_distance = true;
        }
        output.push(field);
    }
    if in_region_data && !has_lod_distance {
        output.push(FieldEntry {
            sig: SubrecordSig(*b"RLDM"),
            value: FieldValue::Float(4096.0),
        });
    }
    record.fields = output;
}

fn struct_member(
    fields: &[(Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<FieldValue> {
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value.clone())
}

fn lower_skyrim_hazd(record: &mut Record, interner: &StringInterner) -> Result<(), &'static str> {
    let Some(data) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.as_str() == "DATA")
    else {
        return Err("skyrim_hazd_missing_data");
    };
    data.sig = SubrecordSig(*b"DNAM");
    data.value = match &data.value {
        FieldValue::Struct(source) => {
            let mut target = Vec::new();
            for name in [
                "limit",
                "radius",
                "lifetime",
                "image_space_radius",
                "target_interval",
                "flags",
            ] {
                target.push((
                    interner.intern(name),
                    struct_member(source, name, interner).unwrap_or(FieldValue::Uint(0)),
                ));
            }
            target.push((interner.intern("effect"), FieldValue::Uint(0)));
            for name in ["light", "impact_data_set", "sound"] {
                target.push((
                    interner.intern(name),
                    struct_member(source, name, interner).unwrap_or(FieldValue::Uint(0)),
                ));
            }
            for name in [
                "taper_effectiveness_full_effect_radius",
                "taper_effectiveness_taper_weight",
                "taper_effectiveness_taper_curse",
            ] {
                target.push((interner.intern(name), FieldValue::Float(0.0)));
            }
            FieldValue::Struct(target)
        }
        FieldValue::Bytes(source) if source.len() == 40 => {
            let mut target = Vec::with_capacity(52);
            target.extend_from_slice(&source[..24]);
            target.extend_from_slice(&0_u32.to_le_bytes());
            target.extend_from_slice(&source[28..40]);
            target.extend_from_slice(&[0; 12]);
            FieldValue::Bytes(target.into())
        }
        _ => return Err("skyrim_hazd_data_shape_unverified"),
    };
    record
        .warnings
        .push(interner.intern("skyrim_hazd_spell_effect_omitted"));
    Ok(())
}

fn lower_skyrim_bpnd_struct(source: &[(Sym, FieldValue)], interner: &StringInterner) -> FieldValue {
    let mut target = Vec::new();
    let push = |target: &mut Vec<(Sym, FieldValue)>, name: &str, fallback: FieldValue| {
        target.push((
            interner.intern(name),
            struct_member(source, name, interner).unwrap_or(fallback),
        ));
    };
    push(&mut target, "damage_mult", FieldValue::Float(1.0));
    push(&mut target, "explodable_debris", FieldValue::Uint(0));
    push(&mut target, "explodable_explosion", FieldValue::Uint(0));
    push(
        &mut target,
        "explodable_debris_scale",
        FieldValue::Float(1.0),
    );
    push(&mut target, "severable_debris", FieldValue::Uint(0));
    push(&mut target, "severable_explosion", FieldValue::Uint(0));
    push(
        &mut target,
        "severable_debris_scale",
        FieldValue::Float(1.0),
    );
    for name in ["cut_min", "cut_max", "cut_radius"] {
        push(&mut target, name, FieldValue::Float(0.0));
    }
    target.push((
        interner.intern("gore_effects_local_rotate_x"),
        struct_member(source, "gore_effects_positioning_rotation_x", interner)
            .unwrap_or(FieldValue::Float(0.0)),
    ));
    target.push((
        interner.intern("gore_effects_local_rotate_y"),
        struct_member(source, "gore_effects_positioning_rotation_y", interner)
            .unwrap_or(FieldValue::Float(0.0)),
    ));
    push(&mut target, "cut_tesselation", FieldValue::Float(0.0));
    push(&mut target, "severable_impact_dataset", FieldValue::Uint(0));
    push(
        &mut target,
        "explodable_impact_dataset",
        FieldValue::Uint(0),
    );
    target.push((
        interner.intern("explodable_limb_replacement_scale"),
        struct_member(source, "limb_replacement_scale", interner).unwrap_or(FieldValue::Float(1.0)),
    ));
    for name in ["flags", "part_type", "health_percent"] {
        push(&mut target, name, FieldValue::Uint(0));
    }
    target.push((interner.intern("actor_value"), FieldValue::Uint(0)));
    for name in [
        "to_hit_chance",
        "explodable_explosion_chance",
        "non_lethal_dismemberment_chance",
        "severable_debris_count",
        "explodable_debris_count",
        "severable_decal_count",
        "explodable_decal_count",
        "geometry_segment_index",
    ] {
        push(&mut target, name, FieldValue::Uint(0));
    }
    for name in [
        "on_cripple_art_object",
        "on_cripple_debris",
        "on_cripple_explosion",
        "on_cripple_impact_dataset",
    ] {
        target.push((interner.intern(name), FieldValue::Uint(0)));
    }
    target.push((
        interner.intern("on_cripple_debris_scale"),
        FieldValue::Float(1.0),
    ));
    target.push((
        interner.intern("on_cripple_debris_count"),
        FieldValue::Uint(0),
    ));
    target.push((
        interner.intern("on_cripple_decal_count"),
        FieldValue::Uint(0),
    ));
    FieldValue::Struct(target)
}

fn lower_skyrim_bptd(record: &mut Record, interner: &StringInterner) -> Result<(), &'static str> {
    let empty = interner.intern("");
    let mut has_bptn = false;
    let mut node = None;
    let mut vats_target = None;
    let mut has_bpnd = false;
    let mut has_nam1 = false;
    let mut has_nam4 = false;
    let mut output = smallvec::SmallVec::new();
    let mut rows = 0usize;
    for mut field in std::mem::take(&mut record.fields) {
        match field.sig.as_str() {
            "BPTN" => {
                if has_bptn
                    || node.is_some()
                    || vats_target.is_some()
                    || has_bpnd
                    || has_nam1
                    || has_nam4
                {
                    return Err("skyrim_bptd_bptn_duplicate_or_prior_scope_incomplete");
                }
                let FieldValue::String(value) = field.value else {
                    return Err("skyrim_bptd_bptn_shape_unverified");
                };
                field.value = FieldValue::String(value);
                has_bptn = true;
            }
            "BPNN" => {
                if !has_bptn
                    || node.is_some()
                    || vats_target.is_some()
                    || has_bpnd
                    || has_nam1
                    || has_nam4
                {
                    return Err("skyrim_bptd_incomplete_or_duplicate_row");
                }
                let FieldValue::String(value) = field.value else {
                    return Err("skyrim_bptd_bpnn_shape_unverified");
                };
                node = Some(value);
                field.value = FieldValue::String(value);
            }
            "BPNT" => {
                if !has_bptn
                    || node.is_none()
                    || vats_target.is_some()
                    || has_bpnd
                    || has_nam1
                    || has_nam4
                {
                    return Err("skyrim_bptd_bpnt_out_of_scope_or_duplicate");
                }
                let FieldValue::String(value) = field.value else {
                    return Err("skyrim_bptd_bpnt_shape_unverified");
                };
                vats_target = Some(value);
                field.value = FieldValue::String(value);
            }
            "BPND" => {
                if !has_bptn
                    || node.is_none()
                    || vats_target.is_none()
                    || has_bpnd
                    || has_nam1
                    || has_nam4
                {
                    return Err("skyrim_bptd_bpnd_missing_scoped_names_or_duplicate");
                }
                field.value = match &field.value {
                    FieldValue::Struct(source) => lower_skyrim_bpnd_struct(source, interner),
                    FieldValue::Bytes(source) => {
                        let source_schema = crate::schema::AuthoringSchema::for_game("skyrimse")
                            .map_err(|_| "skyrim_bptd_source_schema_unavailable")?;
                        let target_schema = crate::schema::AuthoringSchema::for_game("fo4")
                            .map_err(|_| "skyrim_bptd_target_schema_unavailable")?;
                        let context = crate::struct_relayout::StructRelayoutCtx {
                            target_schema: &target_schema,
                            target_form_version: 131,
                            legacy_bptd_only: true,
                        };
                        let mut target = crate::struct_relayout::relayout_struct_bytes(
                            "BPTD",
                            "BPND",
                            source,
                            &source_schema,
                            None,
                            &context,
                        )
                        .ok_or("skyrim_bptd_bpnd_shape_unverified")?;
                        let actor_value = target_schema
                            .struct_field_layout_versioned("BPTD", "BPND", Some(131))
                            .into_iter()
                            .find(|field| field.field_id == "actor_value")
                            .ok_or("fo4_bptd_actor_value_layout_unavailable")?;
                        target[actor_value.offset..actor_value.offset + actor_value.width].fill(0);
                        FieldValue::Bytes(target.into())
                    }
                    _ => return Err("skyrim_bptd_bpnd_shape_unverified"),
                };
                has_bpnd = true;
            }
            "NAM1" => {
                if !has_bptn || !has_bpnd || has_nam1 || has_nam4 {
                    return Err("skyrim_bptd_nam1_out_of_scope_or_duplicate");
                }
                if !matches!(&field.value, FieldValue::String(_)) {
                    return Err("skyrim_bptd_nam1_shape_unverified");
                }
                has_nam1 = true;
            }
            "NAM4" => {
                if !has_bptn || !has_bpnd || !has_nam1 || has_nam4 {
                    return Err("skyrim_bptd_nam4_out_of_scope_or_duplicate");
                }
                if !matches!(&field.value, FieldValue::String(_)) {
                    return Err("skyrim_bptd_nam4_shape_unverified");
                }
                has_nam4 = true;
            }
            "NAM5" => {
                if !has_bptn || !has_bpnd || !has_nam1 || !has_nam4 {
                    return Err("skyrim_bptd_nam5_out_of_scope_or_incomplete_tail");
                }
                match &field.value {
                    FieldValue::Bytes(bytes) if bytes.is_empty() => {}
                    FieldValue::String(value) if interner.resolve(*value) == Some("") => {
                        field.value = FieldValue::Bytes(Vec::new().into());
                    }
                    FieldValue::Bytes(_) | FieldValue::String(_) => {
                        return Err("skyrim_bptd_nam5_nonempty_payload");
                    }
                    _ => return Err("skyrim_bptd_nam5_shape_unverified"),
                }
            }
            _ => {}
        }
        let is_tail = field.sig.as_str() == "NAM5";
        output.push(field);
        if is_tail {
            for (signature, value) in [
                (b"ENAM", FieldValue::String(node.unwrap_or(empty))),
                (
                    b"FNAM",
                    FieldValue::String(vats_target.or(node).unwrap_or(empty)),
                ),
                (b"BNAM", FieldValue::Uint(0)),
                (b"INAM", FieldValue::Uint(0)),
                (b"JNAM", FieldValue::Uint(0)),
                (b"CNAM", FieldValue::Uint(0)),
                (b"NAM2", FieldValue::Uint(0)),
                (b"DNAM", FieldValue::String(empty)),
            ] {
                output.push(FieldEntry {
                    sig: SubrecordSig(*signature),
                    value,
                });
            }
            rows += 1;
            has_bptn = false;
            node = None;
            vats_target = None;
            has_bpnd = false;
            has_nam1 = false;
            has_nam4 = false;
        }
    }
    if rows == 0
        || has_bptn
        || node.is_some()
        || vats_target.is_some()
        || has_bpnd
        || has_nam1
        || has_nam4
    {
        return Err("skyrim_bptd_missing_or_incomplete_rows");
    }
    record.fields = output;
    record
        .warnings
        .push(interner.intern("skyrim_bptd_target_tail_degraded"));
    Ok(())
}

fn complete_legacy_fo4_projection(
    source: Game,
    record: &mut Record,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    let signature = record.sig.as_str().to_string();
    match (source, signature.as_str()) {
        (Game::Fnv | Game::Fo3 | Game::SkyrimSe, "AVIF") => {
            push_projection_field(record, b"NAM0", FieldValue::Float(0.0));
        }
        (Game::Fo3 | Game::SkyrimSe, "ASPC") => {
            push_projection_field(record, b"WNAM", FieldValue::Uint(0));
        }
        (Game::Fnv, "CONT" | "DOOR" | "INGR" | "LIGH" | "TACT")
        | (Game::Fo3, "CONT" | "DOOR" | "LIGH" | "TACT" | "TERM")
        | (Game::SkyrimSe, "ARTO" | "CONT" | "DOOR" | "IDLM" | "LIGH" | "MSTT") => {
            ensure_empty_keyword_array(record);
        }
        (Game::Fnv | Game::Fo3, "ENCH" | "SPEL") => {
            push_projection_field(
                record,
                b"OBND",
                FieldValue::Bytes([0; 12].as_slice().into()),
            );
            if signature == "SPEL" {
                ensure_empty_keyword_array(record);
            }
        }
        (Game::Fnv | Game::Fo3, "LGTM") => synthesize_lgtm_dalc(record, interner)?,
        (Game::Fnv | Game::Fo3, "IMAD") => neutralize_legacy_imad(record, interner)?,
        (Game::Fnv | Game::Fo3, "MISC") => normalize_legacy_misc_data(record)?,
        (Game::Fnv | Game::Fo3 | Game::SkyrimSe, "REGN") => {
            ensure_regn_lod_distance(record);
        }
        (Game::SkyrimSe, "IMAD") => complete_skyrim_imad_arrays(record, interner)?,
        (Game::SkyrimSe, "BPTD") => lower_skyrim_bptd(record, interner)?,
        (Game::SkyrimSe, "HAZD") => lower_skyrim_hazd(record, interner)?,
        (Game::SkyrimSe, "AACT" | "KYWD" | "LCRT") => {
            push_projection_field(record, b"TNAM", FieldValue::Uint(0));
        }
        (Game::SkyrimSe, "EQUP") => {
            push_projection_field(record, b"ANAM", FieldValue::Uint(u32::MAX as u64));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
pub(crate) const LEGACY_REQUIRED_FO4_PROJECTION_FIELDS: &[(Game, &str, &[&str])] = &[
    (Game::Fnv, "AVIF", &["NAM0"]),
    (Game::Fnv, "CONT", &["KSIZ", "KWDA"]),
    (Game::Fnv, "DOOR", &["KSIZ", "KWDA"]),
    (Game::Fnv, "ENCH", &["OBND"]),
    (Game::Fnv, "INGR", &["KSIZ", "KWDA"]),
    (Game::Fnv, "IMAD", &["DNAM", "NAM5", "NAM6"]),
    (Game::Fnv, "LGTM", &["DALC"]),
    (Game::Fnv, "LIGH", &["KSIZ", "KWDA"]),
    (Game::Fnv, "REGN", &["RLDM"]),
    (Game::Fnv, "SPEL", &["OBND", "KSIZ", "KWDA"]),
    (Game::Fnv, "TACT", &["KSIZ", "KWDA"]),
    (Game::Fo3, "ASPC", &["WNAM"]),
    (Game::Fo3, "AVIF", &["NAM0"]),
    (Game::Fo3, "CONT", &["KSIZ", "KWDA"]),
    (Game::Fo3, "DOOR", &["KSIZ", "KWDA"]),
    (Game::Fo3, "ENCH", &["OBND"]),
    (Game::Fo3, "IMAD", &["DNAM", "NAM5", "NAM6"]),
    (Game::Fo3, "LGTM", &["DALC"]),
    (Game::Fo3, "LIGH", &["KSIZ", "KWDA"]),
    (Game::Fo3, "REGN", &["RLDM"]),
    (Game::Fo3, "SPEL", &["OBND", "KSIZ", "KWDA"]),
    (Game::Fo3, "TACT", &["KSIZ", "KWDA"]),
    (Game::Fo3, "TERM", &["KSIZ", "KWDA"]),
    (Game::SkyrimSe, "AACT", &["TNAM"]),
    (Game::SkyrimSe, "ARTO", &["KSIZ", "KWDA"]),
    (Game::SkyrimSe, "ASPC", &["WNAM"]),
    (Game::SkyrimSe, "AVIF", &["NAM0"]),
    (
        Game::SkyrimSe,
        "BPTD",
        &[
            "ENAM", "FNAM", "BNAM", "INAM", "JNAM", "CNAM", "NAM2", "DNAM",
        ],
    ),
    (Game::SkyrimSe, "CONT", &["KSIZ", "KWDA"]),
    (Game::SkyrimSe, "DOOR", &["KSIZ", "KWDA"]),
    (Game::SkyrimSe, "EQUP", &["ANAM"]),
    (Game::SkyrimSe, "HAZD", &["DNAM"]),
    (Game::SkyrimSe, "IDLM", &["KSIZ", "KWDA"]),
    (Game::SkyrimSe, "IMAD", &["NAM5", "NAM6"]),
    (Game::SkyrimSe, "KYWD", &["TNAM"]),
    (Game::SkyrimSe, "LCRT", &["TNAM"]),
    (Game::SkyrimSe, "LIGH", &["KSIZ", "KWDA"]),
    (Game::SkyrimSe, "MSTT", &["KSIZ", "KWDA"]),
    (Game::SkyrimSe, "REGN", &["RLDM"]),
];

impl Translator {
    /// Create a new `Translator` for the given game pair.
    ///
    /// Loads the YAML translation map (if any) and builds an empty
    /// `TransformRegistry`. Installs the pair hook and target hook registered
    /// for (source, target) via `pair_hook_for` / `target_hook_for`, defaulting
    /// to no-op when none are registered.
    pub fn new(source: Game, target: Game) -> Result<Self, ConfigError> {
        Ok(Self {
            source,
            target,
            maps: TranslationMaps::load(source, target)?,
            transforms: transforms::default_registry(),
            pair_hook: pair_hook_for(source, target),
            target_hook: target_hook_for(target),
        })
    }

    /// Run the pre-translate pair hook.
    pub fn pre_translate(
        &self,
        ctx: &mut pair_hook::PairCtx<'_>,
        record: &mut Record,
    ) -> pair_hook::HookResult {
        self.pair_hook.pre_translate(ctx, record)
    }

    /// Run the post-translate pair hook.
    pub fn post_translate(
        &self,
        ctx: &mut pair_hook::PairCtx<'_>,
        record: &mut Record,
    ) -> pair_hook::HookResult {
        self.pair_hook.post_translate(ctx, record)
    }

    /// Run the target hook.
    pub fn run_target_hook(
        &self,
        ctx: &mut target_hook::TargetCtx<'_>,
        record: &mut Record,
    ) -> pair_hook::HookResult {
        self.target_hook.run(ctx, record)
    }

    pub(crate) fn normalize_serial_mapper_record_once(
        &self,
        source_fk: crate::ids::FormKey,
        record: &mut Record,
        mapper: &mut FormKeyMapper<'_>,
        state: &mut pair_hooks::fnv_fo4::LegacySerialNormalizationState,
    ) -> Option<
        Result<
            pair_hooks::fnv_fo4::LegacySerialNormalizeReport,
            pair_hooks::fnv_fo4::LegacySerialDiagnostic,
        >,
    > {
        if self.source == Game::Starfield && self.target == Game::Fo4 {
            use pair_hooks::starfield_fo4::{
                book_and_art, consumable, explosion, hazard, magic_effect, object_properties,
            };
            let mut report = match &record.sig.0 {
                b"MGEF" => magic_effect::normalize_starfield_mgef_data(record, mapper),
                b"ALCH" => consumable::normalize_starfield_alch(record, mapper),
                b"BOOK" | b"DMGT" | b"ARTO" => book_and_art::normalize(record, mapper),
                b"HAZD" => hazard::normalize(record, mapper),
                b"EXPL" => explosion::normalize(record, mapper),
                _ => pair_hooks::fnv_mgef::MgefNormalizeReport::default(),
            };
            object_properties::normalize(record, mapper, &mut report);
            if report == pair_hooks::fnv_mgef::MgefNormalizeReport::default() {
                return None;
            }
            return Some(Ok(pair_hooks::fnv_fo4::LegacySerialNormalizeReport::Mgef(
                report,
            )));
        }
        pair_hooks::fnv_fo4::normalize_legacy_serial_record_once(
            self.source,
            self.target,
            source_fk,
            record,
            mapper,
            state,
        )
    }

    /// Translate one record.
    ///
    /// Applies map-driven field rewrites and named transforms. Returns:
    /// - `Translated` on success.
    /// - `Dropped` when the record sig is in `skip_records`.
    /// - `Deferred(FnvLegacyScripting)` when the map delegates to that pass.
    pub fn translate(&self, record: &Record, interner: &StringInterner) -> TranslateResult {
        self.translate_with_skip_override(record, interner, None)
    }

    /// Translate one record while bypassing the map skip list for one signature.
    ///
    /// Structured emitters use this for records that are skipped by the generic
    /// top-level translator but handled by a dedicated writer.
    pub fn translate_ignoring_skip(
        &self,
        record: &Record,
        interner: &StringInterner,
        ignored_signature: &str,
    ) -> TranslateResult {
        self.translate_with_skip_override(record, interner, Some(ignored_signature))
    }

    fn translate_with_skip_override(
        &self,
        record: &Record,
        interner: &StringInterner,
        ignored_signature: Option<&str>,
    ) -> TranslateResult {
        let sig = record.sig.as_str();

        if self.source == Game::Starfield
            && self.target == Game::Fo4
            && self.maps.record_map(sig).is_none()
            && !pair_hooks::starfield_fo4::is_mvp_topology_signature(sig)
        {
            let kind = interner.intern("starfield_mvp_record_fence");
            return TranslateResult::Dropped {
                reason: kind,
                decision: Decision {
                    kind,
                    message: format!("sig {sig} is outside the Starfield→FO4 MVP record fence"),
                },
            };
        }

        if self.source == Game::Fnv
            && self.target == Game::Fo4
            && pair_hooks::fnv_fo4::FnvFo4Hook::is_unused_ingredient_sentinel(record, interner)
        {
            let kind = interner.intern("unused_legacy_ingredient_sentinel");
            return TranslateResult::Dropped {
                reason: kind,
                decision: Decision {
                    kind,
                    message: format!(
                        "INGR {:06X} dropped: source placeholder is not a creatable ingredient",
                        record.form_key.local
                    ),
                },
            };
        }

        // Skip list.
        if ignored_signature != Some(sig) && self.maps.skip_records.contains(sig) {
            let kind = interner.intern("skip_records");
            let reason = kind;
            return TranslateResult::Dropped {
                reason,
                decision: Decision {
                    kind,
                    message: format!("sig {sig} in skip_records"),
                },
            };
        }

        let mut out = record.clone();
        if matches!(self.source, Game::Fnv | Game::Fo3) && self.target == Game::Fo4 && sig == "CREA"
        {
            out.sig = super::ids::SigCode(*b"NPC_");
        }

        // Look up the per-sig record map.
        let map = match self.maps.record_map(sig) {
            Some(m) => m,
            None => {
                if matches!(self.source, Game::Fnv | Game::Fo3 | Game::SkyrimSe)
                    && self.target == Game::Fo4
                    && !maps::is_quest_runtime_signature(sig)
                {
                    match self.maps.record_route(sig) {
                        None => {
                            let kind = interner.intern("unclassified_record_shape");
                            return TranslateResult::Dropped {
                                reason: kind,
                                decision: Decision {
                                    kind,
                                    message: format!(
                                        "sig {sig} has no explicit compatible, target-projection, pair-hook, topology, or blocked route"
                                    ),
                                },
                            };
                        }
                        Some(RecordRoutePolicy::Blocked) => {
                            let kind = interner.intern("required_field_lowering_blocked");
                            return TranslateResult::Dropped {
                                reason: kind,
                                decision: Decision {
                                    kind,
                                    message: format!(
                                        "sig {sig} is blocked because no semantically valid FO4 required-field lowering is registered"
                                    ),
                                },
                            };
                        }
                        Some(RecordRoutePolicy::TargetProjection) => {
                            if let Err(blocker) =
                                complete_legacy_fo4_projection(self.source, &mut out, interner)
                            {
                                let kind = interner.intern("required_field_lowering_blocked");
                                return TranslateResult::Dropped {
                                    reason: kind,
                                    decision: Decision {
                                        kind,
                                        message: format!("sig {sig} projection blocked: {blocker}"),
                                    },
                                };
                            }
                        }
                        _ => {}
                    }
                }
                return TranslateResult::Translated(out);
            }
        };

        if let Some(delegate) = map.delegate {
            return TranslateResult::Deferred(delegate);
        }

        // Apply target sig override.
        if let Some(ref tgt_sig) = map.target_sig {
            if let Ok(new_sig) = super::ids::SigCode::from_str(tgt_sig) {
                out.sig = new_sig;
            }
        }

        // Drop fields listed in drop_fields.
        if !map.drop_fields.is_empty() {
            let rec_sig = out.sig;
            let rec_local = out.form_key.local;
            out.fields.retain(|f| {
                let drop = map.drop_fields.iter().any(|d| d.as_str() == f.sig.as_str());
                if drop {
                    crate::drop_trace::trace(
                        "translate.drop_field",
                        rec_sig.as_str(),
                        rec_local,
                        f.sig.as_str(),
                        "sig in translation-map drop list",
                    );
                }
                !drop
            });
        }

        // Field rewrites: rename source_field → target_field.
        for rewrite in &map.field_rewrites {
            let src_sig = match SubrecordSig::from_str(&rewrite.source_field) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let tgt_sig = match SubrecordSig::from_str(&rewrite.target_field) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for entry in out.fields.iter_mut() {
                if entry.sig == src_sig {
                    entry.sig = tgt_sig;
                    break;
                }
            }
        }

        // Transform invocations.
        let mut ctx = TransformCtx { interner };
        for invocation in &map.transforms {
            let Some(transform) = self.transforms.get(&invocation.name) else {
                continue;
            };
            let field_sig = match SubrecordSig::from_str(&invocation.field) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for entry in out.fields.iter_mut() {
                if entry.sig == field_sig {
                    let _ = transform.apply(&mut ctx, &mut entry.value, &invocation.config);
                }
            }
        }

        if matches!(self.source, Game::Fnv | Game::Fo3)
            && self.target == Game::Fo4
            && out.sig.as_str() == "MISC"
        {
            if let Err(blocker) = normalize_legacy_misc_data(&mut out) {
                let kind = interner.intern("required_field_lowering_blocked");
                return TranslateResult::Dropped {
                    reason: kind,
                    decision: Decision {
                        kind,
                        message: format!("sig {sig} projection blocked: {blocker}"),
                    },
                };
            }
        }

        TranslateResult::Translated(out)
    }
}

#[cfg(test)]
mod scol_tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use smallvec::SmallVec;

    fn source_fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    #[test]
    fn fo76_scol_map_converts_every_repeated_onam_and_drops_invalid_fields() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("SCOL").unwrap(),
            source_fk(&mut interner, 0x294744),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XALG").unwrap(),
            value: FieldValue::Uint(1),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ONAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[0x12, 0x58, 0x03, 0x00, 0, 0, 0, 0])),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ONAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[0x30, 0x47, 0x03, 0x00, 0, 0, 0, 0])),
        });

        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated SCOL, got {other:?}"),
        };

        assert!(
            translated
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "XALG")
        );

        let onam_locals: Vec<_> = translated
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "ONAM")
            .filter_map(|field| match &field.value {
                FieldValue::FormKey(fk) => Some(fk.local),
                _ => None,
            })
            .collect();
        assert_eq!(onam_locals, vec![0x035812, 0x034730]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, plugin_handle_add_master_native, plugin_handle_close_native,
        plugin_handle_load_no_py, plugin_handle_new_no_py, plugin_handle_save_no_py,
        plugin_handle_store_ref,
    };
    use smallvec::SmallVec;

    #[test]
    fn translator_skeleton_translates_passthrough() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("000800@Mod.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("WEAP").unwrap(), fk);
        record.eid = Some(interner.intern("PassThrough"));

        let result = translator.translate(&record, &mut interner);
        match result {
            TranslateResult::Translated(t) => {
                assert_eq!(t.sig, record.sig);
                assert_eq!(t.eid, record.eid);
            }
            _ => panic!("expected Translated"),
        }
    }

    #[test]
    fn legacy_same_signature_passthrough_requires_an_explicit_shape_route() {
        let interner = StringInterner::new();
        let fk = FormKey::parse("000800@FalloutNV.esm", &interner).unwrap();
        let record = Record::new(SigCode::from_str("ASPC").unwrap(), fk);

        let translator = Translator::new(Game::Fnv, Game::Fo4).unwrap();
        assert!(matches!(
            translator.translate(&record, &interner),
            TranslateResult::Translated(_)
        ));

        let mut unclassified = Translator::new(Game::Fnv, Game::Fo4).unwrap();
        unclassified.maps.record_routes.remove("ASPC");
        let TranslateResult::Dropped { decision, .. } = unclassified.translate(&record, &interner)
        else {
            panic!("undeclared schema-divergent ASPC must fail closed")
        };
        assert_eq!(
            interner.resolve(decision.kind),
            Some("unclassified_record_shape")
        );
    }

    fn required_projection_fixture(signature: &str, interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey::parse("000800@ProjectionFixture.esp", interner).unwrap(),
        );
        if signature == "LGTM" {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("DATA").unwrap(),
                value: FieldValue::Struct(vec![
                    (interner.intern("ambient_color_red"), FieldValue::Uint(17)),
                    (interner.intern("ambient_color_green"), FieldValue::Uint(34)),
                    (interner.intern("ambient_color_blue"), FieldValue::Uint(51)),
                ]),
            });
        }
        if signature == "BPTD" {
            for (signature, value) in [
                ("BPTN", FieldValue::String(interner.intern("Head"))),
                ("BPNN", FieldValue::String(interner.intern("Head"))),
                ("BPNT", FieldValue::String(interner.intern("HeadTarget"))),
                (
                    "BPND",
                    FieldValue::Struct(vec![
                        (interner.intern("damage_mult"), FieldValue::Float(1.0)),
                        (interner.intern("part_type"), FieldValue::Uint(1)),
                    ]),
                ),
                ("NAM1", FieldValue::String(interner.intern(""))),
                ("NAM4", FieldValue::String(interner.intern(""))),
                ("NAM5", FieldValue::Bytes(Vec::new().into())),
            ] {
                record.fields.push(FieldEntry {
                    sig: SubrecordSig::from_str(signature).unwrap(),
                    value,
                });
            }
        }
        if signature == "HAZD" {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("DATA").unwrap(),
                value: FieldValue::Struct(vec![
                    (interner.intern("limit"), FieldValue::Uint(1)),
                    (interner.intern("radius"), FieldValue::Float(8.0)),
                    (
                        interner.intern("spell"),
                        FieldValue::FormKey(FormKey::parse("000800@Skyrim.esm", interner).unwrap()),
                    ),
                ]),
            });
        }
        if signature == "IMAD" {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("DNAM").unwrap(),
                value: FieldValue::Struct(vec![
                    (interner.intern("vignette_radius"), FieldValue::Uint(0)),
                    (interner.intern("vignette_strength"), FieldValue::Uint(0)),
                ]),
            });
        }
        if signature == "REGN" {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("RDAT").unwrap(),
                value: FieldValue::Bytes(Vec::new().into()),
            });
        }
        record
    }

    #[test]
    fn legacy_target_projection_completes_every_required_fo4_field_gap() {
        let interner = StringInterner::new();
        for &(source, signature, required) in LEGACY_REQUIRED_FO4_PROJECTION_FIELDS {
            let translator = Translator::new(source, Game::Fo4).unwrap();
            let record = required_projection_fixture(signature, &interner);
            let TranslateResult::Translated(record) = translator.translate(&record, &interner)
            else {
                panic!("{source:?} {signature} must complete its FO4 projection")
            };
            for required_signature in required {
                assert!(
                    projection_has_field(&record, required_signature),
                    "{source:?} {signature} is missing {required_signature}"
                );
            }
            let value = |field_signature: &str| {
                &record
                    .fields
                    .iter()
                    .find(|field| field.sig.as_str() == field_signature)
                    .unwrap_or_else(|| panic!("missing {signature}.{field_signature}"))
                    .value
            };
            for required_signature in required {
                match *required_signature {
                    "NAM0" => assert_eq!(value("NAM0"), &FieldValue::Float(0.0)),
                    "WNAM" | "TNAM" | "KSIZ" => {
                        assert_eq!(value(required_signature), &FieldValue::Uint(0));
                    }
                    "KWDA" => {
                        assert_eq!(value(required_signature), &FieldValue::List(Vec::new()));
                    }
                    "NAM5" | "NAM6" if signature == "IMAD" => {
                        let FieldValue::List(rows) = value(required_signature) else {
                            panic!("{source:?} {signature}.{required_signature} must be rows")
                        };
                        assert_eq!(rows.len(), 2);
                    }
                    "OBND" => {
                        let FieldValue::Bytes(bytes) = value("OBND") else {
                            panic!("{source:?} {signature}.OBND must be bytes")
                        };
                        assert_eq!(bytes.as_slice(), &[0; 12]);
                    }
                    "RLDM" => assert_eq!(value("RLDM"), &FieldValue::Float(4096.0)),
                    "ANAM" => assert_eq!(value("ANAM"), &FieldValue::Uint(u32::MAX as u64)),
                    _ => {}
                }
            }
            if signature == "IMAD" && source == Game::SkyrimSe {
                let FieldValue::Struct(dnam) = value("DNAM") else {
                    panic!("Skyrim IMAD.DNAM must be typed")
                };
                for count_field in ["vignette_radius", "vignette_strength"] {
                    assert!(dnam.iter().any(|(name, value)| {
                        interner.resolve(*name) == Some(count_field)
                            && *value == FieldValue::Uint(2)
                    }));
                }
            }
            if signature == "IMAD" && matches!(source, Game::Fnv | Game::Fo3) {
                let FieldValue::Struct(dnam) = value("DNAM") else {
                    panic!("{source:?} IMAD.DNAM must be typed")
                };
                let target_schema = AuthoringSchema::for_game("fo4").unwrap();
                let definition = target_schema.record_def("IMAD").unwrap();
                let dnam_definition = definition.subrecord_def("DNAM").unwrap();
                for count_field in dnam_definition
                    .fields
                    .iter()
                    .filter(|field| field.id.contains("_count"))
                {
                    assert!(dnam.iter().any(|(name, value)| {
                        interner.resolve(*name) == Some(count_field.id.as_str())
                            && *value == FieldValue::Uint(2)
                    }));
                }
                for array in definition.subrecords.iter().filter(|subrecord| {
                    subrecord.required
                        && subrecord
                            .codec
                            .as_deref()
                            .is_some_and(|codec| codec.starts_with("array_struct:"))
                }) {
                    let FieldValue::List(rows) = value(&array.id) else {
                        panic!("{source:?} IMAD.{} must be rows", array.id)
                    };
                    assert_eq!(rows.len(), 2, "{source:?} IMAD.{}", array.id);
                }
                assert!(record.warnings.iter().any(|warning| {
                    interner.resolve(*warning) == Some("legacy_imad_neutral_target_replacement")
                }));
            }
            if signature == "BPTD" {
                assert_eq!(value("BNAM"), &FieldValue::Uint(0));
                assert_eq!(value("NAM2"), &FieldValue::Uint(0));
                assert!(record.warnings.iter().any(|warning| {
                    interner.resolve(*warning) == Some("skyrim_bptd_target_tail_degraded")
                }));
            }
            if signature == "HAZD" {
                let FieldValue::Struct(dnam) = value("DNAM") else {
                    panic!("Skyrim HAZD.DNAM must be typed")
                };
                assert!(dnam.iter().any(|(name, value)| {
                    interner.resolve(*name) == Some("effect") && *value == FieldValue::Uint(0)
                }));
                assert!(record.warnings.iter().any(|warning| {
                    interner.resolve(*warning) == Some("skyrim_hazd_spell_effect_omitted")
                }));
            }
            if signature == "LGTM" {
                let FieldValue::Struct(dalc) = &record
                    .fields
                    .iter()
                    .find(|field| field.sig.as_str() == "DALC")
                    .unwrap()
                    .value
                else {
                    panic!("DALC must be typed")
                };
                assert!(dalc.iter().any(|(name, value)| {
                    interner.resolve(*name) == Some("ambient_colors_directional_x_red")
                        && *value == FieldValue::Uint(17)
                }));
                assert!(dalc.iter().any(|(name, value)| {
                    interner.resolve(*name) == Some("ambient_colors_directional_x_red_1")
                        && *value == FieldValue::Uint(17)
                }));
                assert_eq!(dalc.len(), 29);
                assert!(dalc.iter().any(|(name, value)| {
                    interner.resolve(*name) == Some("ambient_colors_fresnel_power")
                        && *value == FieldValue::Float(1.0)
                }));
            }
        }
    }

    #[test]
    fn legacy_misc_data_expands_to_the_fo4_value_weight_layout() {
        let interner = StringInterner::new();
        let mut record = required_projection_fixture("MISC", &interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(25_i32.to_le_bytes().as_slice().into()),
        });

        normalize_legacy_misc_data(&mut record).unwrap();

        let FieldValue::Bytes(data) = &record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value
        else {
            panic!("MISC.DATA must be raw target bytes")
        };
        assert_eq!(data.len(), 8);
        assert_eq!(&data[..4], &25_i32.to_le_bytes());
        assert_eq!(&data[4..], &0.0_f32.to_le_bytes());

        let mut currency = required_projection_fixture("MISC", &interner);
        currency.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("absolute_value"),
                FieldValue::Uint(2),
            )]),
        });

        normalize_legacy_misc_data(&mut currency).unwrap();

        let FieldValue::Bytes(data) = &currency
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value
        else {
            panic!("CMNY-to-MISC DATA must be raw target bytes")
        };
        assert_eq!(data.as_slice(), &[2, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn legacy_currency_translation_expands_the_target_misc_data_layout() {
        let interner = StringInterner::new();
        let mut currency = required_projection_fixture("CMNY", &interner);
        currency.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("absolute_value"),
                FieldValue::Uint(2),
            )]),
        });

        let translator = Translator::new(Game::Fnv, Game::Fo4).unwrap();
        let TranslateResult::Translated(currency) = translator.translate(&currency, &interner)
        else {
            panic!("CMNY must translate to a target MISC")
        };

        assert_eq!(currency.sig.as_str(), "MISC");
        let FieldValue::Bytes(data) = &currency
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap()
            .value
        else {
            panic!("translated CMNY.DATA must be raw target bytes")
        };
        assert_eq!(data.as_slice(), &[2, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn skyrim_bptd_projection_fails_closed_on_incomplete_or_malformed_rows() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::SkyrimSe, Game::Fo4).unwrap();
        let mut cases = Vec::new();

        for missing in ["BPTN", "BPNN", "BPNT", "NAM5"] {
            let mut record = required_projection_fixture("BPTD", &interner);
            record.fields.retain(|field| field.sig.as_str() != missing);
            cases.push((format!("missing_{missing}"), record));
        }

        for duplicate in ["BPTN", "BPNN", "BPNT", "NAM1", "NAM4", "NAM5"] {
            let mut record = required_projection_fixture("BPTD", &interner);
            let index = record
                .fields
                .iter()
                .position(|field| field.sig.as_str() == duplicate)
                .unwrap();
            let field = record.fields[index].clone();
            record.fields.insert(index + 1, field);
            cases.push((format!("duplicate_{duplicate}"), record));
        }

        let mut misplaced_bptn = required_projection_fixture("BPTD", &interner);
        let bptn_index = misplaced_bptn
            .fields
            .iter()
            .position(|field| field.sig.as_str() == "BPTN")
            .unwrap();
        let bptn = misplaced_bptn.fields.remove(bptn_index);
        let bpnn_index = misplaced_bptn
            .fields
            .iter()
            .position(|field| field.sig.as_str() == "BPNN")
            .unwrap();
        misplaced_bptn.fields.insert(bpnn_index + 1, bptn);
        cases.push(("misplaced_BPTN".to_string(), misplaced_bptn));

        let mut malformed_nam5 = required_projection_fixture("BPTD", &interner);
        malformed_nam5
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "NAM5")
            .unwrap()
            .value = FieldValue::Bytes(vec![1].into());
        cases.push(("malformed_NAM5_length".to_string(), malformed_nam5));

        let mut malformed_bptn = required_projection_fixture("BPTD", &interner);
        malformed_bptn
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "BPTN")
            .unwrap()
            .value = FieldValue::Bytes(Vec::new().into());
        cases.push(("malformed_BPTN_shape".to_string(), malformed_bptn));

        let mut malformed_nam1 = required_projection_fixture("BPTD", &interner);
        malformed_nam1
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "NAM1")
            .unwrap()
            .value = FieldValue::Bytes(Vec::new().into());
        cases.push(("malformed_NAM1_shape".to_string(), malformed_nam1));

        for (name, record) in cases {
            let TranslateResult::Dropped { decision, .. } =
                translator.translate(&record, &interner)
            else {
                panic!("{name} must fail closed")
            };
            assert_eq!(
                interner.resolve(decision.kind),
                Some("required_field_lowering_blocked"),
                "{name}"
            );
        }
    }

    #[test]
    fn representative_nonquest_routes_survive_target_projection_save_reopen() {
        const OUTPUT: &str = "NonquestProjectionAudit.esp";

        fn find_record<'a>(
            items: &'a [ParsedItem],
            signature: &str,
            local: u32,
        ) -> Option<&'a esp_authoring_core::plugin_runtime::ParsedRecord> {
            items.iter().find_map(|item| match item {
                ParsedItem::Record(record)
                    if record.signature.as_str() == signature
                        && (record.form_id & 0x00FF_FFFF) == local =>
                {
                    Some(record)
                }
                ParsedItem::Group(group) => find_record(&group.children, signature, local),
                _ => None,
            })
        }

        let interner = StringInterner::new();
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        let mut records = Vec::new();
        let cases = [
            (Game::Fnv, "CONT"),
            (Game::Fo3, "SPEL"),
            (Game::SkyrimSe, "FURN"),
            (Game::SkyrimSe, "STAT"),
            (Game::Fo3, "REFR"),
            (Game::Fnv, "IMAD"),
            (Game::Fo3, "IMAD"),
            (Game::SkyrimSe, "BPTD"),
            (Game::SkyrimSe, "HAZD"),
        ];
        for (index, (source, signature)) in cases.iter().copied().enumerate() {
            let local = 0x800 + index as u32;
            let mut source_record = required_projection_fixture(signature, &interner);
            if signature == "BPTD" {
                source_record
                    .fields
                    .iter_mut()
                    .find(|field| field.sig.as_str() == "BPND")
                    .unwrap()
                    .value = FieldValue::Bytes(vec![0; 84].into());
                source_record
                    .fields
                    .iter_mut()
                    .find(|field| field.sig.as_str() == "NAM5")
                    .unwrap()
                    .value = FieldValue::String(interner.intern(""));
            }
            source_record.form_key =
                FormKey::parse(&format!("{local:06X}@{OUTPUT}"), &interner).unwrap();
            if signature != "REFR" {
                let editor_id = interner.intern(&format!("Projection{signature}{local:06X}"));
                source_record.eid = Some(editor_id);
                source_record.fields.push(FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(editor_id),
                });
            }

            let translator = Translator::new(source, Game::Fo4).unwrap();
            let TranslateResult::Translated(translated) =
                translator.translate(&source_record, &interner)
            else {
                panic!("{source:?} {signature} must have an owned nonquest route")
            };
            let normalized =
                TargetRecordNormalizer::target_only_with_interner(&target_schema, &interner)
                    .normalize(translated);
            let TargetRecordNormalization::Keep(normalized) = normalized else {
                panic!("FO4 target projection must keep {source:?} {signature}")
            };
            assert!(normalized.fields.iter().all(|field| {
                target_schema
                    .record_def(signature)
                    .unwrap()
                    .subrecord_def(field.sig.as_str())
                    .is_some()
            }));
            records.push(normalized);
        }

        let handle = plugin_handle_new_no_py(OUTPUT, Some("fo4"));
        plugin_handle_add_master_native(handle, "Fallout4.esm", None).unwrap();
        for record in &records {
            crate::target_write::add_record_native(
                handle,
                record.clone(),
                &target_schema,
                &interner,
            )
            .unwrap_or_else(|error| panic!("write {} failed: {error}", record.sig.as_str()));
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(OUTPUT);
        plugin_handle_save_no_py(handle, path.to_str().unwrap()).unwrap();
        assert!(plugin_handle_close_native(handle));

        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get(&reopened).unwrap();
        for (index, (_, signature)) in cases.iter().copied().enumerate() {
            let record = find_record(&slot.parsed.root_items, signature, 0x800 + index as u32)
                .unwrap_or_else(|| panic!("missing reopened {signature}"));
            let expected = match (index, signature) {
                (5 | 6, "IMAD") => &[('D', 'N', 'A', 'M', 252_usize)][..],
                (7, "BPTD") => &[
                    ('B', 'P', 'N', 'D', 101),
                    ('E', 'N', 'A', 'M', 5),
                    ('F', 'N', 'A', 'M', 11),
                    ('B', 'N', 'A', 'M', 4),
                    ('I', 'N', 'A', 'M', 4),
                    ('J', 'N', 'A', 'M', 4),
                    ('C', 'N', 'A', 'M', 4),
                    ('N', 'A', 'M', '2', 4),
                    ('D', 'N', 'A', 'M', 1),
                ],
                (8, "HAZD") => &[('D', 'N', 'A', 'M', 52)],
                _ => &[],
            };
            for &(a, b, c, d, length) in expected {
                let subrecord_signature = format!("{a}{b}{c}{d}");
                let subrecord = record
                    .subrecords
                    .iter()
                    .find(|subrecord| subrecord.signature.as_str() == subrecord_signature)
                    .unwrap_or_else(|| {
                        let actual = record
                            .subrecords
                            .iter()
                            .map(|subrecord| subrecord.signature.as_str())
                            .collect::<Vec<_>>();
                        panic!(
                            "reopened {signature} is missing {subrecord_signature}; actual={actual:?}"
                        )
                    });
                assert_eq!(
                    subrecord.data.len(),
                    length,
                    "reopened {signature}.{subrecord_signature} has invalid size"
                );
            }
        }
        drop(store);
        assert!(plugin_handle_close_native(reopened));
    }

    #[test]
    fn fnv_legacy_scripting_records_are_deferred() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fnv, Game::Fo4).unwrap();
        for (index, signature) in ["SCPT", "QUST", "DIAL", "INFO", "SCEN"]
            .into_iter()
            .enumerate()
        {
            let fk =
                FormKey::parse(&format!("{:06X}@FalloutNV.esm", 0x800 + index), &interner).unwrap();
            let record = Record::new(SigCode::from_str(signature).unwrap(), fk);
            assert!(matches!(
                translator.translate(&record, &interner),
                TranslateResult::Deferred(DeferredKind::FnvLegacyScripting)
            ));
        }
    }

    #[test]
    fn fnv_unused_ingredient_sentinel_drops_only_the_exact_source_placeholder() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fnv, Game::Fo4).unwrap();
        let plugin = interner.intern("FalloutNV.esm");
        let sentinel_editor_id =
            interner.intern("DoNotCreateNewIngredientsWeArentUsingThemInFallout");

        let mut sentinel = Record::new(
            SigCode::from_str("INGR").unwrap(),
            FormKey {
                local: 0x03_135B,
                plugin,
            },
        );
        sentinel.eid = Some(sentinel_editor_id);
        let dropped = translator.translate(&sentinel, &interner);
        let TranslateResult::Dropped { decision, .. } = dropped else {
            panic!("expected the exact FNV ingredient sentinel to be dropped");
        };
        assert_eq!(
            interner.resolve(decision.kind),
            Some("unused_legacy_ingredient_sentinel")
        );

        let mut different_local = sentinel.clone();
        different_local.form_key.local += 1;
        assert!(matches!(
            translator.translate(&different_local, &interner),
            TranslateResult::Translated(_)
        ));

        let mut different_editor_id = sentinel.clone();
        different_editor_id.eid = Some(interner.intern("UsableIngredient"));
        assert!(matches!(
            translator.translate(&different_editor_id, &interner),
            TranslateResult::Translated(_)
        ));

        let fo3_translator = Translator::new(Game::Fo3, Game::Fo4).unwrap();
        assert!(matches!(
            fo3_translator.translate(&sentinel, &interner),
            TranslateResult::Translated(_)
        ));
    }

    #[test]
    fn repaired_legacy_maps_preserve_weapon_idle_and_misc_payload_signatures() {
        for (source, plugin, record_signature, field_signature) in [
            (Game::Fnv, "FalloutNV.esm", "WEAP", "DATA"),
            (Game::Fnv, "FalloutNV.esm", "IDLE", "DATA"),
            (Game::Fo3, "Fallout3.esm", "MISC", "FULL"),
        ] {
            let interner = StringInterner::new();
            let translator = Translator::new(source, Game::Fo4).unwrap();
            let fk = FormKey::parse(&format!("000800@{plugin}"), &interner).unwrap();
            let mut record = Record::new(SigCode::from_str(record_signature).unwrap(), fk);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(field_signature).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[1, 2, 3, 4])),
            });

            let translated = match translator.translate(&record, &interner) {
                TranslateResult::Translated(record) => record,
                other => panic!("expected translated {record_signature}, got {other:?}"),
            };
            assert_eq!(translated.fields.len(), 1);
            assert_eq!(translated.fields[0].sig.as_str(), field_signature);
        }
    }

    /// The `fo4_to_starfield` map must load for the (Fo4, Starfield) pair,
    /// carry MODL/OBND, and drop the FO4-only STAT subrecords. ObjectBounds
    /// scaling is not the map's job: OBND reaches the pipeline as raw
    /// `FieldValue::Bytes`, which `scale_nested` can't touch, so
    /// `pair_hooks/fo4_starfield::rescale_object_bounds` (`pre_translate`)
    /// rescales it (see `stat_object_bounds_reach_the_target_scaled`).
    #[test]
    fn fo4_starfield_stat_carries_modl_and_drops_fo4_only_subrecords() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo4, Game::Starfield).unwrap();
        let fk = FormKey::parse("0233A1@Fallout4.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), fk);
        record.eid = Some(interner.intern("BuildingBrownstone01"));

        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("OBND").unwrap(),
            value: FieldValue::Struct(vec![
                (interner.intern("ObjectBoundsX1"), FieldValue::Int(-140)),
                (interner.intern("ObjectBoundsY1"), FieldValue::Int(-70)),
                (interner.intern("ObjectBoundsZ1"), FieldValue::Int(0)),
                (interner.intern("ObjectBoundsX2"), FieldValue::Int(140)),
                (interner.intern("ObjectBoundsY2"), FieldValue::Int(70)),
                (interner.intern("ObjectBoundsZ2"), FieldValue::Int(700)),
            ]),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(interner.intern("Architecture\\Brownstone01.nif")),
        });
        // FO4-only / codec-mismatched subrecords the map drops.
        for sig in [
            "MODT", "MODC", "MODF", "MNAM", "PTRN", "DNAM", "FTYP", "MODS", "PRPS", "VMAD",
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[1, 2, 3, 4])),
            });
        }

        let translated = match translator.translate(&record, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated STAT, got {other:?}"),
        };

        assert_eq!(translated.sig.as_str(), "STAT");
        assert_eq!(translated.eid, record.eid);
        assert_eq!(
            translated
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["OBND", "MODL"],
            "only the Starfield-valid STAT subrecords survive"
        );

        let modl = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "MODL")
            .expect("MODL carried");
        assert!(matches!(&modl.value, FieldValue::String(sym)
            if interner.resolve(*sym) == Some("Architecture\\Brownstone01.nif")));

        // OBND is carried but left byte/value-for-value untouched by the bare
        // map-driven translate() call in this test -- there is no more
        // scale_nested transform to no-op on this synthetic Struct shape, and
        // this test never runs the real pre_translate hook that owns the
        // rescale on raw Bytes (see the doc comment above).
        let FieldValue::Struct(bounds) = &translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "OBND")
            .expect("OBND carried")
            .value
        else {
            panic!("OBND must stay a struct -- no transform touches it now");
        };
        let z2 = bounds
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some("ObjectBoundsZ2"))
            .map(|(_, value)| value)
            .expect("ObjectBoundsZ2 present");
        assert_eq!(
            z2,
            &FieldValue::Int(700),
            "unscaled: map no longer touches OBND"
        );
    }

    #[test]
    fn fnv_fo4_race_drops_legacy_head_body_and_facegen_subrecords() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fnv, Game::Fo4).unwrap();
        let fk = FormKey::parse("0987DF@FalloutNV.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("RACE").unwrap(), fk);

        for sig in [
            "EDID", "FULL", "DESC", "DATA", "NAM0", "MNAM", "INDX", "MODL", "MODT", "ICON", "FNAM",
            "NAM1", "MNAM", "INDX", "MODL", "MODT", "FNAM", "HNAM", "ENAM", "MNAM", "FGGS", "FGGA",
            "FGTS", "SNAM", "FNAM", "HEAD", "MICO",
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::None,
            });
        }

        let translated = match translator.translate(&record, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated RACE, got {other:?}"),
        };

        assert_eq!(
            translated
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID", "FULL", "DESC"]
        );
    }

    #[test]
    fn fo76_keym_retains_fo4_compatible_name_preview_and_sounds() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("27DB2F@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("KEYM").unwrap(), fk);

        for (sig, bytes) in [
            ("FULL", b"Congressional Access Card\0".as_slice()),
            ("PTRN", &0x0024_8895_u32.to_le_bytes()),
            ("YNAM", &0x0059_5D2B_u32.to_le_bytes()),
            ("ZNAM", &0x0059_5D2C_u32.to_le_bytes()),
            ("XALG", &1_u32.to_le_bytes()),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
            });
        }

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated KEYM, got {other:?}"),
        };

        assert_eq!(
            translated
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["FULL", "PTRN", "YNAM", "ZNAM"]
        );
    }

    #[test]
    fn fo76_fo4_race_preserves_sraf_subgraph_role() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("00D191@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("RACE").unwrap(), fk);

        for (sig, bytes) in [
            (
                "SGNM",
                b"Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx\0".as_slice(),
            ),
            ("SAPT", b"Actors\\Snallygaster\\Animations\0".as_slice()),
            ("SRAF", &[1, 0, 0, 0]),
            ("SAKD", &[0, 0, 0, 0]),
            ("STKD", &[0, 0, 0, 0]),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
            });
        }

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated RACE, got {other:?}"),
        };
        let sigs: Vec<_> = translated
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();

        assert_eq!(sigs, vec!["SGNM", "SAPT", "SRAF", "SAKD", "STKD"]);
    }

    #[test]
    fn fo76_currency_records_translate_to_fo4_misc() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("3F7410@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("CNCY").unwrap(), fk);
        record.eid = Some(interner.intern("LegendaryTokens"));

        let result = translator.translate(&record, &mut interner);
        match result {
            TranslateResult::Translated(t) => {
                assert_eq!(t.sig.as_str(), "MISC");
                assert_eq!(t.eid, record.eid);
            }
            other => panic!("expected translated CNCY, got {other:?}"),
        }
    }

    #[test]
    fn fo76_addn_ikek_translates_to_fo4_data_index() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("84F665@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("ADDN").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("IKEK").unwrap(),
            value: FieldValue::Uint(185_746_299),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated ADDN, got {other:?}"),
        };

        assert!(
            translated
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "IKEK")
        );
        let data = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("IKEK should become FO4 DATA");
        assert_eq!(data.value, FieldValue::Uint(185_746_299));
    }

    #[test]
    fn fo76_lvli_retains_lvlf_use_all_flag() {
        // Outfit-combining leveled lists (headwear + clothes) rely on the
        // "Use All" flag (LVLF bit 0x04) to return every component. Dropping
        // LVLF made FO4 pick only one branch → naked NPCs.
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("6CC09E@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("LVLI").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LVLF").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[4])),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated LVLI, got {other:?}"),
        };

        let lvlf = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "LVLF")
            .expect("LVLI LVLF flag must be carried to FO4");
        assert_eq!(lvlf.value, FieldValue::Bytes(SmallVec::from_slice(&[4])));
    }

    #[test]
    fn fo76_ligh_lils_drops_without_mapping_to_fo4_fnam() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("15AE5B@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("LIGH").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LILS").unwrap(),
            value: FieldValue::Float(0.0),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated LIGH, got {other:?}"),
        };

        assert!(
            translated
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "LILS"),
            "FO76 LILS must not survive into FO4 output"
        );
        assert!(
            translated
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "FNAM"),
            "FO76 LILS is not equivalent to FO4 FNAM"
        );
    }

    #[test]
    fn fo76_cont_translation_preserves_items_and_strips_zero_health_destructibles() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("11CEED@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("CONT").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("COCT").unwrap(),
            value: FieldValue::Uint(2),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[1, 0, 0, 0, 0])),
        });
        for item in [0x0673B5_u32, 0x59DD1D] {
            let mut cnto = Vec::new();
            cnto.extend_from_slice(&item.to_le_bytes());
            cnto.extend_from_slice(&1_u32.to_le_bytes());
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("CNTO").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(cnto)),
            });
        }

        let mut zero_health_dest = Vec::new();
        zero_health_dest.extend_from_slice(&0_i32.to_le_bytes());
        zero_health_dest.extend_from_slice(&[1, 0, 0, 0]);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DEST").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(zero_health_dest)),
        });
        for sig in ["DSTD", "DSTF"] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[0; 8])),
            });
        }

        let mut nonzero_health_dest = Vec::new();
        nonzero_health_dest.extend_from_slice(&25_i32.to_le_bytes());
        nonzero_health_dest.extend_from_slice(&[1, 0, 0, 0]);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DEST").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(nonzero_health_dest)),
        });
        for sig in ["DSTD", "DSTF"] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[1; 8])),
            });
        }

        let mut ctx = pair_hook::PairCtx::new(&interner);
        translator.pre_translate(&mut ctx, &mut record).unwrap();
        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated CONT, got {other:?}"),
        };

        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "CNTO")
                .count(),
            2
        );
        assert!(translated.fields.iter().any(|field| {
            field.sig.as_str() == "DATA"
                && field.value == FieldValue::Bytes(SmallVec::from_slice(&[1, 0, 0, 0, 0]))
        }));
        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "DEST")
                .count(),
            1,
            "only zero-health FO76 container destructible groups should be stripped"
        );
        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|field| matches!(field.sig.as_str(), "DSTD" | "DSTF"))
                .count(),
            2,
            "the nonzero-health destructible stage should remain"
        );
        let first_cnto = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "CNTO")
            .expect("CONT keeps CNTO");
        let FieldValue::Struct(fields) = &first_cnto.value else {
            panic!("CNTO should be structured");
        };
        assert!(matches!(
            fields
                .iter()
                .find(|(key, _)| interner.resolve(*key) == Some("item"))
                .map(|(_, value)| value),
            Some(FieldValue::FormKey(form_key)) if form_key.local == 0x0673B5
        ));
    }

    #[test]
    fn fo76_furn_translation_preserves_inventory_items() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("7AAD19@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("FURN").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("COCT").unwrap(),
            value: FieldValue::Uint(2),
        });
        for item in [0x387B02_u32, 0x7AEDA1] {
            let mut cnto = Vec::new();
            cnto.extend_from_slice(&item.to_le_bytes());
            cnto.extend_from_slice(&1_u32.to_le_bytes());
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("CNTO").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(cnto)),
            });
        }

        let mut ctx = pair_hook::PairCtx::new(&interner);
        translator.pre_translate(&mut ctx, &mut record).unwrap();
        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated FURN, got {other:?}"),
        };

        assert_eq!(
            translated
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "CNTO")
                .count(),
            2
        );
        let first_cnto = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "CNTO")
            .expect("FURN keeps CNTO");
        let FieldValue::Struct(fields) = &first_cnto.value else {
            panic!("CNTO should be structured");
        };
        assert!(matches!(
            fields
                .iter()
                .find(|(key, _)| interner.resolve(*key) == Some("item"))
                .map(|(_, value)| value),
            Some(FieldValue::FormKey(form_key)) if form_key.local == 0x387B02
        ));
    }

    #[test]
    fn fo76_npc_translation_keeps_head_part_pnam_rows() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("5858E7@SeventySix.esm", &mut interner).unwrap();
        let source_beard = FormKey::parse("135AA4@SeventySix.esm", &mut interner).unwrap();
        let target_beard = FormKey::parse("135AA4@Fallout4.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("NPC_").unwrap(), fk);
        record.eid = Some(interner.intern("W05_LvlDenizen_RaiderLooterM_or_LiteAlly"));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("W05_LvlDenizen_RaiderLooterM_or_LiteAlly")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("PNAM").unwrap(),
            value: FieldValue::FormKey(source_beard),
        });

        let mut translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated NPC_, got {other:?}"),
        };

        let pnam_count = translated
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "PNAM")
            .count();
        assert_eq!(
            pnam_count, 1,
            "FO76 NPC_ source head part should be preserved for mapper/fallback handling"
        );
        let hdpt_sig = SigCode::from_str("HDPT").unwrap();
        let beard_eid = interner.intern("Beard12");
        let mut mapper = FormKeyMapper::new(
            [(beard_eid, target_beard, hdpt_sig)],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                target_master_names: vec!["Fallout4.esm".to_string()],
                use_base_game_assets: true,
                preserve_source_ids: true,
                ..MapperOptions::default()
            },
            &interner,
        );
        mapper.allocate_or_resolve(source_beard, Some(beard_eid), hdpt_sig);
        mapper.rewrite_record(&mut translated).unwrap();

        assert!(matches!(
            translated
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "PNAM")
                .map(|field| &field.value),
            Some(FieldValue::FormKey(form_key)) if *form_key == target_beard
        ));
    }

    #[test]
    fn fo76_wrld_pipeline_drops_source_runtime_tables() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("00DC6C@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("WRLD").unwrap(), fk);
        record.eid = Some(interner.intern("TESTNewTerrain"));
        for sig in ["EDID", "RNAM", "MHDT", "OFST", "CLSZ", "NAM0"] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&[0, 1, 2, 3])),
            });
        }

        let mut ctx = pair_hook::PairCtx::new(&interner);
        translator.pre_translate(&mut ctx, &mut record).unwrap();
        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated WRLD, got {other:?}"),
        };
        let sigs: Vec<_> = translated
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();

        assert_eq!(sigs, vec!["EDID", "NAM0"]);
    }

    #[test]
    fn fo76_omod_data_transform_filters_raw_int_properties() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("7745A6@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("OMOD").unwrap(), fk);
        record.eid = Some(interner.intern("ATX_mod_BackPack_TheHoldAll_Material_Default"));

        let form_type = interner.intern("FormType");
        let properties = interner.intern("Properties");
        let property = interner.intern("Property");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Struct(vec![
                (form_type, FieldValue::Uint(1_330_467_393)),
                (
                    properties,
                    FieldValue::List(vec![
                        FieldValue::Struct(vec![(property, FieldValue::Uint(15))]),
                        FieldValue::Struct(vec![(property, FieldValue::Uint(13))]),
                        FieldValue::Struct(vec![(property, FieldValue::Uint(3))]),
                    ]),
                ),
            ]),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated OMOD, got {other:?}"),
        };
        let data = translated
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("translated OMOD keeps Data field");
        let FieldValue::Struct(data_fields) = &data.value else {
            panic!("expected Data struct");
        };
        let (_, properties_value) = data_fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some("Properties"))
            .expect("Data has Properties");
        let FieldValue::List(items) = properties_value else {
            panic!("expected Properties list");
        };
        assert!(items.is_empty());
    }

    #[test]
    fn fo76_scene_records_translated_by_generic_writer_by_default() {
        // SCEN is a flat top-level FO4 record emitted by the generic writer
        // (resolves NOTE\SNAM-Scene). MODBOX_DISABLE_SCEN re-skips it (gate
        // lives in maps.rs::load); not exercised here to avoid process-global
        // env races.
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        assert!(
            !translator.maps.skip_records.contains("SCEN"),
            "SCEN must not be in skip_records by default"
        );
        let fk = FormKey::parse("534F51@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("SCEN").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("TalesFromWestVirginiaHolotape05Scene")),
        });

        match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(out) => {
                assert_eq!(out.sig.as_str(), "SCEN");
            }
            other => panic!("expected translated SCEN, got {other:?}"),
        }
    }

    #[test]
    fn fo76_fact_venp_is_renamed_and_relaid_to_fo4_venv() {
        // FO4 FACT VENV is required; FO76 supplies VENP (different layout). The
        // map must rename VENP→VENV and relayout the bytes (not drop it).
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("4124AA@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("FACT").unwrap(), fk);
        // Canonical FO76 VENP (LC060_WhitespringVendor): end_hour=24, radius=1200,
        // bools 1,1,1, trailing bytes_6=00.
        let mut venp = Vec::new();
        venp.extend_from_slice(&0u16.to_le_bytes());
        venp.extend_from_slice(&24u16.to_le_bytes());
        venp.extend_from_slice(&1200u32.to_le_bytes());
        venp.extend_from_slice(&[1, 1, 1]);
        venp.push(0);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("VENP").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(venp)),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated FACT, got {other:?}"),
        };

        assert!(
            translated.fields.iter().all(|f| f.sig.as_str() != "VENP"),
            "VENP must be renamed away"
        );
        let venv = translated
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "VENV")
            .expect("FACT must carry a VENV after translation");
        let FieldValue::Bytes(bytes) = &venv.value else {
            panic!("VENV must be raw FO4-laid-out bytes");
        };
        let expected: [u8; 12] = [0, 0, 24, 0, 0xB0, 0x04, 0, 0, 1, 1, 1, 0];
        assert_eq!(bytes.as_slice(), &expected);
    }

    #[test]
    fn fo76_vending_machine_facts_get_fo4_barter_rules() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let legacy_venv: [u8; 12] = [0, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0, 0];

        for (form_key, merchant_container_key) in [
            ("175087@SeventySix.esm", "4E0FE7@SeventySix.esm"),
            ("1750A5@SeventySix.esm", "4E0FE6@SeventySix.esm"),
        ] {
            let fk = FormKey::parse(form_key, &mut interner).unwrap();
            let merchant_container_fk =
                FormKey::parse(merchant_container_key, &mut interner).unwrap();
            let mut record = Record::new(SigCode::from_str("FACT").unwrap(), fk);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("VENC").unwrap(),
                value: FieldValue::FormKey(merchant_container_fk),
            });
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("VENV").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&legacy_venv)),
            });

            let mut translated = match translator.translate(&record, &mut interner) {
                TranslateResult::Translated(record) => record,
                other => panic!("expected translated FACT, got {other:?}"),
            };
            translator
                .post_translate(
                    &mut crate::translator::pair_hook::PairCtx::new(&interner),
                    &mut translated,
                )
                .unwrap();

            let venv = translated
                .fields
                .iter()
                .find(|f| f.sig.as_str() == "VENV")
                .expect("legacy FO76 VENV must survive FACT translation");
            let FieldValue::Bytes(bytes) = &venv.value else {
                panic!("VENV must remain raw FO4-layout bytes");
            };
            assert_eq!(
                bytes.as_slice(),
                &[0, 0, 24, 0, 0xF4, 0x01, 0, 0, 1, 1, 1, 0]
            );

            let plvd = translated
                .fields
                .iter()
                .find(|f| f.sig.as_str() == "PLVD")
                .expect("vending-machine FACT must use a NearSelf vendor location");
            let FieldValue::Bytes(bytes) = &plvd.value else {
                panic!("PLVD must use raw FO4 location bytes");
            };
            assert_eq!(
                bytes.as_slice(),
                &[12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
            );
            assert_eq!(
                translated
                    .fields
                    .iter()
                    .find(|field| field.sig.as_str() == "VENC")
                    .map(|field| &field.value),
                Some(&FieldValue::FormKey(merchant_container_fk))
            );
        }
    }

    #[test]
    fn fo76_other_legacy_vendor_fact_is_not_rewritten_as_a_vending_machine() {
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("175086@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("FACT").unwrap(), fk);
        let legacy_venv: [u8; 12] = [0, 0, 24, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("VENV").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&legacy_venv)),
        });

        translator
            .post_translate(
                &mut crate::translator::pair_hook::PairCtx::new(&interner),
                &mut record,
            )
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "VENV")
                .map(|field| &field.value),
            Some(&FieldValue::Bytes(SmallVec::from_slice(&legacy_venv)))
        );
        assert!(
            record
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "PLVD")
        );
    }

    #[test]
    fn fo76_cobj_dnam_is_renamed_and_relaid_to_fo4_intv() {
        // FO4 COBJ carries the crafting "created object count" in INTV; the FO76
        // source carries it (plus a UI sort priority) in DNAM under a different
        // layout. The map must rename DNAM→INTV and relayout the bytes (not drop
        // it, which lost the count). Example: 05A371 (co_Weapon_Melee_BoxingGlove).
        let mut interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let fk = FormKey::parse("05A371@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("COBJ").unwrap(), fk);
        // FO76 DNAM (struct:f,H,B,B): priority_ui_sort_order=2.0, count=3, pad 0,0.
        let mut dnam = Vec::new();
        dnam.extend_from_slice(&2.0f32.to_le_bytes());
        dnam.extend_from_slice(&3u16.to_le_bytes());
        dnam.push(0);
        dnam.push(0);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(dnam)),
        });

        let translated = match translator.translate(&record, &mut interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated COBJ, got {other:?}"),
        };

        assert!(
            translated.fields.iter().all(|f| f.sig.as_str() != "DNAM"),
            "DNAM must be renamed away"
        );
        let intv = translated
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "INTV")
            .expect("COBJ must carry an INTV after translation");
        let FieldValue::Bytes(bytes) = &intv.value else {
            panic!("INTV must be raw FO4-laid-out bytes");
        };
        // FO4 INTV (struct:H,H): created_object_count=3, priority=2.
        assert_eq!(bytes.as_slice(), &[3, 0, 2, 0]);
    }

    #[test]
    fn game_from_str_round_trips() {
        for (s, g) in [
            ("fo3", Game::Fo3),
            ("fnv", Game::Fnv),
            ("fo4", Game::Fo4),
            ("fo76", Game::Fo76),
            ("skyrimse", Game::SkyrimSe),
            ("starfield", Game::Starfield),
        ] {
            assert_eq!(Game::from_str(s), Some(g));
            assert_eq!(g.as_str(), s);
        }
    }

    #[test]
    fn game_from_str_unknown_returns_none() {
        assert!(Game::from_str("unknown_game").is_none());
    }

    // -------------------------------------------------------------------------
    // Per-pair smoke tests: Translator::new must succeed for every
    // registered game pair.  YAML-only pairs use NoOpPairHook (the wildcard
    // arm in pair_hook_for); this verifies the map file loads without error.
    // -------------------------------------------------------------------------

    #[test]
    fn translator_loads_fo3_to_fo4() {
        Translator::new(Game::Fo3, Game::Fo4).expect("fo3→fo4 should load");
    }

    #[test]
    fn legacy_crea_always_targets_fo4_npc_for_fnv_and_fo3() {
        let interner = StringInterner::new();
        for (source, plugin) in [(Game::Fnv, "FalloutNV.esm"), (Game::Fo3, "Fallout3.esm")] {
            let translator = Translator::new(source, Game::Fo4).unwrap();
            let record = Record::new(
                SigCode(*b"CREA"),
                FormKey {
                    local: 0x800,
                    plugin: interner.intern(plugin),
                },
            );
            let TranslateResult::Translated(translated) = translator.translate(&record, &interner)
            else {
                panic!("legacy CREA must translate");
            };
            assert_eq!(translated.sig.as_str(), "NPC_");
        }
    }

    #[test]
    fn fo3_to_fo4_uses_narrow_proj_pair_hook() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo3, Game::Fo4).unwrap();
        let fk = FormKey::parse("000800@Fallout3.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("PROJ").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![0; 68])),
        });

        let mut ctx = pair_hook::PairCtx::new(&interner);
        translator.pre_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .unwrap();
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        assert!(matches!(&data.value, FieldValue::Bytes(bytes) if bytes.is_empty()));
        assert!(matches!(&dnam.value, FieldValue::Bytes(bytes) if bytes.len() == 93));

        let fk = FormKey::parse("001234@Fallout3.esm", &interner).unwrap();
        let mut term = Record::new(SigCode::from_str("TERM").unwrap(), fk);
        term.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("SNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3, 4])),
        });
        term.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("PNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![5, 6, 7, 8])),
        });

        translator.pre_translate(&mut ctx, &mut term).unwrap();

        assert_eq!(
            term.fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["SNAM", "PNAM"],
            "FO3 records must not run FNV-specific non-PROJ rewrites"
        );
    }

    #[test]
    fn translator_loads_fo4_to_skyrimse() {
        Translator::new(Game::Fo4, Game::SkyrimSe).expect("fo4→skyrimse should load");
    }

    #[test]
    fn translator_loads_fo76_to_fnv() {
        Translator::new(Game::Fo76, Game::Fnv).expect("fo76→fnv should load");
    }

    #[test]
    fn translator_loads_fo76_to_skyrimse() {
        Translator::new(Game::Fo76, Game::SkyrimSe).expect("fo76→skyrimse should load");
    }

    #[test]
    fn translator_loads_skyrimse_to_fo4() {
        Translator::new(Game::SkyrimSe, Game::Fo4).expect("skyrimse→fo4 should load");
    }

    #[test]
    fn translator_loads_starfield_to_fo4() {
        Translator::new(Game::Starfield, Game::Fo4).expect("starfield→fo4 should load");
    }

    #[test]
    fn starfield_to_fo4_dispatches_starfield_fo4_pair_hook() {
        // PTT2 has no NoOpPairHook effect; only StarfieldFo4Hook::pre_translate
        // drops it. This proves pair_hook_for wires the real hook, not the
        // wildcard NoOp arm, for (Starfield, Fo4).
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let fk = FormKey::parse("161610@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(interner.intern("Test")),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("PTT2").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&[0u8; 32])),
        });

        let mut ctx = pair_hook::PairCtx::new(&interner);
        translator.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            record.fields.len(),
            1,
            "StarfieldFo4Hook::drop_transforms_default_preview must strip PTT2"
        );
        assert_eq!(record.fields[0].sig.as_str(), "EDID");
    }

    #[test]
    fn starfield_to_fo4_serial_mapper_pass_relays_mgef_data() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let fk = FormKey::parse("040613@Starfield.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("MGEF").unwrap(), fk);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![0; 136])),
        });
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Starfield.esm".into(),
                source_plugin_name: "Starfield.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            &interner,
        );
        let mut state = pair_hooks::fnv_fo4::LegacySerialNormalizationState::default();

        let outcome =
            translator.normalize_serial_mapper_record_once(fk, &mut record, &mut mapper, &mut state);

        assert!(matches!(
            outcome,
            Some(Ok(pair_hooks::fnv_fo4::LegacySerialNormalizeReport::Mgef(ref report)))
                if report.converted_rows == 1
        ));
        assert!(matches!(record.fields[0].value, FieldValue::Struct(_)));

        let mut ingestible = Record::new(SigCode::from_str("ALCH").unwrap(), fk);
        ingestible.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ENIT").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![0; 56])),
        });
        let outcome = translator.normalize_serial_mapper_record_once(
            fk,
            &mut ingestible,
            &mut mapper,
            &mut state,
        );
        assert!(matches!(
            outcome,
            Some(Ok(pair_hooks::fnv_fo4::LegacySerialNormalizeReport::Mgef(ref report)))
                if report.converted_rows == 1
        ));

        let mut flora = Record::new(SigCode::from_str("FLOR").unwrap(), fk);
        flora.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("PRPS").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(
                hex::decode("05e923000000a04000000000").unwrap(),
            )),
        });
        let outcome =
            translator.normalize_serial_mapper_record_once(fk, &mut flora, &mut mapper, &mut state);
        assert!(matches!(
            outcome,
            Some(Ok(pair_hooks::fnv_fo4::LegacySerialNormalizeReport::Mgef(ref report)))
                if report.converted_rows == 1
        ));
        assert!(flora.fields.is_empty(), "an unmapped property row is not an FO4 form");

        let mut bare = Record::new(SigCode::from_str("FLOR").unwrap(), fk);
        assert!(
            translator
                .normalize_serial_mapper_record_once(fk, &mut bare, &mut mapper, &mut state)
                .is_none()
        );
    }

    #[test]
    fn starfield_to_fo4_mvp_fence_admits_mapped_records_and_drops_unknown_shapes() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        for signature in ["MSTT", "TERM", "EFSH"] {
            let fk = FormKey::parse("000800@Starfield.esm", &interner).unwrap();
            let record = Record::new(SigCode::from_str(signature).unwrap(), fk);
            assert!(
                matches!(
                    translator.translate(&record, &interner),
                    TranslateResult::Translated(_)
                ),
                "mapped {signature} must pass the MVP fence"
            );
        }
        for signature in ["RFGP", "PKIN"] {
            let fk = FormKey::parse("000800@Starfield.esm", &interner).unwrap();
            let record = Record::new(SigCode::from_str(signature).unwrap(), fk);
            let TranslateResult::Dropped { decision, .. } =
                translator.translate(&record, &interner)
            else {
                panic!("{signature} must be fenced out");
            };
            assert_eq!(decision.kind, interner.intern("starfield_mvp_record_fence"));
        }
    }

    /// Skyrim→SkyrimSE has no YAML map (no Python pair hook either); Translator
    /// must still construct successfully, yielding an empty TranslationMaps.
    #[test]
    fn translator_loads_skyrim_to_skyrimse_no_map() {
        let t = Translator::new(Game::Skyrim, Game::SkyrimSe)
            .expect("skyrim→skyrimse should load even with no YAML map");
        // No skip_records, no record maps — empty maps are valid.
        assert!(t.maps.skip_records.is_empty());
        assert!(t.maps.record_map("WEAP").is_none());
    }

    /// FO3→FNV has no YAML map; same empty-maps check.
    #[test]
    fn translator_loads_fo3_to_fnv_no_map() {
        let t = Translator::new(Game::Fo3, Game::Fnv)
            .expect("fo3→fnv should load even with no YAML map");
        assert!(t.maps.skip_records.is_empty());
        assert!(t.maps.record_map("WEAP").is_none());
    }
}
