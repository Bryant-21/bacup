//! Material expansion CK applies to a retained shader when it combines the
//! shape, plus the material CRC that decides bucket sharing.
//!
//! A named shader is rebuilt from its BGSM/BGEM v2 material; an unnamed one
//! is a source-authoritative clone that only gains `Transform_Changed`.

use std::hash::{DefaultHasher, Hash, Hasher};

use indexmap::IndexMap;
use materials_native::bgem::BgemData;
use materials_native::bgsm::BgsmData;
use nif_core_native::model::{NifBlock, NifValue};

use crate::assets::AssetResolver;
use crate::crc;
use crate::error::{PrevisError, Result};
use crate::nif::material_relative_path;
use crate::swap::SwapLayers;

pub const SHADER_DEFAULT: u32 = 0;
pub const SHADER_ENVIRONMENT_MAP: u32 = 1;
pub const SHADER_GLOW: u32 = 2;
pub const SHADER_PARALLAX: u32 = 3;
pub const SHADER_FACEGEN: u32 = 4;
pub const SHADER_SKIN_TINT: u32 = 5;
pub const SHADER_HAIR_TINT: u32 = 6;
pub const SHADER_PARALLAX_OCCLUSION: u32 = 7;
pub const SHADER_MULTILAYER_PARALLAX: u32 = 11;
pub const SHADER_SPARKLE_SNOW: u32 = 14;
pub const SHADER_EYE_ENVMAP: u32 = 16;
/// Serialized features CK's factory builds unchanged. Legacy 8/9/10 are
/// canonicalized by CK (to 19/18/14), which this writer does not do.
const UNNAMED_LIGHTING_FEATURES: [u32; 13] = [0, 1, 2, 3, 4, 5, 6, 7, 11, 14, 16, 18, 19];

const LIGHTING_SHADER: &str = "BSLightingShaderProperty";
const EFFECT_SHADER: &str = "BSEffectShaderProperty";
const LIGHTING_MATERIAL_TYPE: u32 = 2;
const EFFECT_MATERIAL_TYPE: u32 = 1;
/// CK initialises the effect PCD lookup scale to this; the serialized word
/// is undefined in its captures (FINDINGS, effect PCD grayscale).
const EFFECT_LOOKUP_SCALE: f32 = 1.0;

mod flag1 {
    pub const SPECULAR: u32 = 1 << 0;
    pub const GREYSCALE_TO_PALETTE_COLOR: u32 = 1 << 4;
    pub const GREYSCALE_TO_PALETTE_ALPHA: u32 = 1 << 5;
    pub const USE_FALLOFF: u32 = 1 << 6;
    pub const ENVIRONMENT_MAPPING: u32 = 1 << 7;
    pub const RGB_FALLOFF: u32 = 1 << 8;
    pub const CAST_SHADOWS: u32 = 1 << 9;
    pub const MODEL_SPACE_NORMALS: u32 = 1 << 12;
    pub const REFRACTION: u32 = 1 << 15;
    pub const FIRE_REFRACTION: u32 = 1 << 16;
    pub const EYE_ENVIRONMENT_MAPPING: u32 = 1 << 17;
    pub const SCREENDOOR_ALPHA_FADE: u32 = 1 << 19;
    pub const TESSELLATE: u32 = 1 << 25;
    pub const DECAL: u32 = 1 << 26;
    pub const DYNAMIC_DECAL: u32 = 1 << 27;
    pub const EXTERNAL_EMITTANCE: u32 = 1 << 29;
    pub const SOFT_EFFECT: u32 = 1 << 30;
    pub const ZBUFFER_TEST: u32 = 1 << 31;
    /// Bits with a recovered, verified meaning (`SHADER_FLAG_1_BITS`).
    pub const KNOWN: u32 = 0xEE7F97F9;
}

mod flag2 {
    pub const ZBUFFER_WRITE: u32 = 1 << 0;
    pub const NO_FADE: u32 = 1 << 3;
    pub const DOUBLE_SIDED: u32 = 1 << 4;
    pub const GLOW_MAP: u32 = 1 << 6;
    pub const TRANSFORM_CHANGED: u32 = 1 << 7;
    pub const WEAPON_BLOOD: u32 = 1 << 17;
    pub const HIDE_ON_LOCAL_MAP: u32 = 1 << 18;
    pub const SKEW_SPECULAR_ALPHA: u32 = 1 << 22;
    pub const TREE_ANIM: u32 = 1 << 29;
    pub const EFFECT_LIGHTING: u32 = 1 << 30;
    /// Bits with a recovered, verified meaning (`SHADER_FLAG_2_BITS`).
    pub const KNOWN: u32 = 0x656600F9;
}

pub const HIDE_ON_LOCAL_MAP: u32 = flag2::HIDE_ON_LOCAL_MAP;
pub const TREE_ANIM: u32 = flag2::TREE_ANIM;

const DEFAULT_WETNESS: [f32; 6] = [0.8, 0.7, 0.3, 1.0, 1.2, 0.1];
const WETNESS_NAMES: [&str; 6] = ["Spec Scale", "Spec Power", "Min Var", "Env Map Scale", "Fresnel Power", "Metalness"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Alpha {
    pub flags: u16,
    pub threshold: u8,
}

impl Alpha {
    /// Only the parts of the alpha state that are live affect bucket sharing.
    pub fn compatibility_key(&self) -> [u32; 6] {
        let flags = self.flags as u32;
        let test = (flags >> 9) & 1;
        let blend = flags & 1;
        [
            test,
            if test != 0 { self.threshold as u32 } else { 0 },
            if test != 0 { flags & 0x1C00 } else { 0 },
            blend,
            if blend != 0 { flags & 0x1E } else { 0 },
            if blend != 0 { flags & 0x1E0 } else { 0 },
        ]
    }
}

/// Where a combined shape's NiAlphaProperty comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlphaSource {
    /// The source shape's own property, unchanged.
    Source,
    /// A BGSM's state; `None` when it neither alpha-tests nor blends, in which
    /// case CK drops the source property.
    Material(Option<Alpha>),
}

/// NiAlphaProperty bits a BGSM decides: blend enable, alpha test enable and
/// the editor alpha threshold (`EnableEditorAlphaRef`).
pub const MATERIAL_ALPHA_BITS: u16 = 0x8201;

/// A lighting or effect shader after CK's material expansion.
#[derive(Clone, Debug)]
pub struct ExpandedShader {
    pub fields: IndexMap<String, NifValue>,
    pub source_shader_type: u32,
    pub shader_type: u32,
    pub flags1: u32,
    pub flags2: u32,
    /// The texture-set slots; an effect shader has no texture set.
    pub textures: Option<Vec<String>>,
    pub material_crc32: u32,
    pub alpha: AlphaSource,
    pub grayscale: f32,
}

impl ExpandedShader {
    /// Equal for [`Self::identical`] shaders.
    pub fn content_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for (name, value) in &self.fields {
            name.hash(&mut hasher);
            hash_value(value, &mut hasher);
        }
        (self.flags1, self.flags2, self.material_crc32, &self.textures).hash(&mut hasher);
        hasher.finish()
    }

    /// Field order and float bits included, so one can stand in for the other.
    pub fn identical(&self, other: &ExpandedShader) -> bool {
        self.fields.len() == other.fields.len()
            && self.fields.iter().zip(&other.fields).all(|((a, x), (b, y))| a == b && identical_value(x, y))
            && (self.source_shader_type, self.shader_type, self.flags1, self.flags2, self.material_crc32)
                == (other.source_shader_type, other.shader_type, other.flags1, other.flags2, other.material_crc32)
            && self.textures == other.textures
            && self.alpha == other.alpha
            && self.grayscale.to_bits() == other.grayscale.to_bits()
    }
}

fn hash_value(value: &NifValue, hasher: &mut DefaultHasher) {
    std::mem::discriminant(value).hash(hasher);
    match value {
        NifValue::Float(v) => v.to_bits().hash(hasher),
        NifValue::Int(v) => v.hash(hasher),
        NifValue::UInt(v) | NifValue::FloatNan(v) => v.hash(hasher),
        NifValue::Ref(v) => v.hash(hasher),
        NifValue::String(s) | NifValue::Char(s) => s.hash(hasher),
        NifValue::Struct(fields) => fields.iter().for_each(|(name, value)| {
            name.hash(hasher);
            hash_value(value, hasher);
        }),
        NifValue::Array(items) => items.iter().for_each(|value| hash_value(value, hasher)),
        _ => {}
    }
}

/// Floats compare by bits: `0.0` and `-0.0` serialize differently.
fn identical_value(a: &NifValue, b: &NifValue) -> bool {
    match (a, b) {
        (NifValue::Float(x), NifValue::Float(y)) => x.to_bits() == y.to_bits(),
        (NifValue::Struct(x), NifValue::Struct(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|((m, p), (n, q))| m == n && identical_value(p, q))
        }
        (NifValue::Array(x), NifValue::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| identical_value(p, q)),
        (
            NifValue::Vec3(_)
            | NifValue::Vec4(_)
            | NifValue::Color3(_)
            | NifValue::Color4(_)
            | NifValue::Quaternion(_)
            | NifValue::Matrix33(_)
            | NifValue::Matrix44(_),
            _,
        ) => false,
        _ => a == b,
    }
}

fn uint(value: Option<&NifValue>) -> Option<u64> {
    match value? {
        NifValue::UInt(v) => Some(*v),
        NifValue::Int(v) if *v >= 0 => Some(*v as u64),
        _ => None,
    }
}

pub fn get_bare<'a>(fields: &'a IndexMap<String, NifValue>, name: &str) -> Option<&'a NifValue> {
    fields
        .get(name)
        .or_else(|| fields.iter().find(|(k, _)| k.split(':').next() == Some(name)).map(|(_, v)| v))
}

/// Replaces a field matched by exact or suffix-less name, else appends it.
pub fn set_bare(fields: &mut IndexMap<String, NifValue>, name: &str, value: NifValue) {
    if let Some(slot) = fields.get_mut(name) {
        *slot = value;
        return;
    }
    if let Some((_, slot)) = fields.iter_mut().find(|(k, _)| k.split(':').next() == Some(name)) {
        *slot = value;
        return;
    }
    fields.insert(name.to_string(), value);
}

fn float(value: f32) -> NifValue {
    NifValue::Float(value as f64)
}

fn color(values: [f32; 3]) -> NifValue {
    NifValue::Struct(IndexMap::from([
        ("r".to_string(), float(values[0])),
        ("g".to_string(), float(values[1])),
        ("b".to_string(), float(values[2])),
    ]))
}

fn uv(u: f32, v: f32) -> NifValue {
    NifValue::Struct(IndexMap::from([("u".to_string(), float(u)), ("v".to_string(), float(v))]))
}

fn clean(value: &str) -> &str {
    value.trim_end_matches('\0')
}

/// CK prefixes resource directories under `Materials\` with a capital letter.
pub fn output_material_name(source: &str) -> String {
    let normalized = source.replace('/', "\\");
    let prefix = "materials\\";
    if !normalized.to_ascii_lowercase().starts_with(prefix) {
        return source.to_string();
    }
    let relative = &normalized[prefix.len()..];
    let parts: Vec<&str> = relative.split('\\').collect();
    let mut rebuilt: Vec<String> = parts[..parts.len() - 1]
        .iter()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    rebuilt.push(parts[parts.len() - 1].to_string());
    let mut relative = rebuilt.join("\\");
    let actor_prefix = "actors\\character\\";
    if relative.to_ascii_lowercase().starts_with(actor_prefix) {
        relative = format!("{actor_prefix}{}", &relative[actor_prefix.len()..]);
    }
    format!("Materials\\{relative}")
}

pub fn output_texture_name(value: Option<&str>) -> String {
    let normalized = clean(value.unwrap_or("")).replace('\\', "/");
    let prefix = "textures/";
    if normalized.to_ascii_lowercase().starts_with(prefix) {
        normalized[prefix.len()..].to_string()
    } else {
        normalized
    }
}

fn clamp_mode(tile_u: bool, tile_v: bool) -> u32 {
    (tile_u as u32) << 1 | tile_v as u32
}

fn read_bgsm(assets: &AssetResolver, relative: &str) -> Result<BgsmData> {
    let bytes = assets
        .read(relative)?
        .ok_or_else(|| PrevisError::unsupported(format!("material not found: {relative}")))?;
    let material = materials_native::bgsm::parse(&bytes)
        .map_err(|e| PrevisError::invalid(format!("{relative}: {e}")))?;
    if material.header.version != 2 {
        return Err(PrevisError::unsupported(format!(
            "only the verified FO4 BGSM v2 subset is supported: {relative}"
        )));
    }
    Ok(material)
}

fn root_relative(root: &str) -> String {
    format!("materials/{}", root.replace('\\', "/"))
}

/// The material followed by its `RootMaterialPath` templates.
fn material_chain(assets: &AssetResolver, material: BgsmData) -> Result<Vec<BgsmData>> {
    let mut seen: Vec<String> = Vec::new();
    let mut root = clean(&material.RootMaterialPath).to_string();
    let mut chain = vec![material];
    while !root.is_empty() {
        let relative = root_relative(&root);
        let key = relative.to_ascii_lowercase();
        if seen.contains(&key) {
            return Err(PrevisError::invalid(format!("cyclic BGSM root material chain at {relative}")));
        }
        seen.push(key.clone());
        let parent = read_bgsm(assets, &relative)?;
        let next = clean(&parent.RootMaterialPath).to_string();
        chain.push(parent);
        if !next.is_empty() && root_relative(&next).to_ascii_lowercase() == key {
            break;
        }
        root = next;
    }
    Ok(chain)
}

/// A wetness value of `-1` inherits that field from the next template.
fn resolved_wetness(chain: &[BgsmData]) -> [f32; 6] {
    let mut resolved = DEFAULT_WETNESS;
    for (index, slot) in resolved.iter_mut().enumerate() {
        for material in chain {
            let value = match index {
                0 => Some(material.WetnessControlSpecScale),
                1 => Some(material.WetnessControlSpecPowerScale),
                2 => Some(material.WetnessControlSpecMinvar),
                3 => material.WetnessControlEnvMapScale,
                4 => Some(material.WetnessControlFresnelPower),
                _ => Some(material.WetnessControlMetalness),
            };
            if let Some(value) = value.filter(|&v| v != -1.0) {
                *slot = value;
                break;
            }
        }
    }
    resolved
}

fn crc_u32(crc: u32, value: u32) -> u32 {
    crc::update(crc, &value.to_le_bytes())
}

fn crc_f32(crc: u32, value: f32) -> u32 {
    crc::update(crc, &value.to_le_bytes())
}

/// CK's round-half-up float quantization, wrapped to 32 bits.
fn quantized(value: f32, scale: f32) -> u32 {
    let scaled = value * scale;
    let truncated = (scaled as f64).trunc();
    // f64 remainder is exact, so this wraps like an unbounded integer would.
    let integer = truncated.rem_euclid(4_294_967_296.0) as u32;
    if scaled - truncated as f32 >= 0.5 { integer.wrapping_add(1) } else { integer }
}

struct CrcInputs<'a> {
    shader_type: u32,
    uv_offset: [f32; 2],
    uv_scale: [f32; 2],
    textures: &'a [String],
    clamp: u32,
    alpha: f32,
    smoothness: f32,
    fresnel: f32,
    wetness: [f32; 6],
    specular: [f32; 3],
    specular_strength: f32,
    refraction: f32,
    subsurface: f32,
    rimlight: f32,
    backlight: f32,
    environment_scale: f32,
    hair_tint: [f32; 3],
    skin_tint: [f32; 4],
    /// Raw float parameters of the Parallax Occ, MultiLayer Parallax,
    /// Sparkle Snow and Eye Envmap features, in serialized order.
    feature_parameters: &'a [f32],
}

fn lighting_material_crc32(inputs: &CrcInputs) -> u32 {
    let mut crc = crc_u32(0, LIGHTING_MATERIAL_TYPE);
    for value in [inputs.uv_offset[0], inputs.uv_offset[1], inputs.uv_scale[0], inputs.uv_scale[1]] {
        crc = crc_f32(crc, value);
    }
    crc = crc_u32(crc_u32(crc, 0), 0);
    for slot in [0, 1, 2, 7, 3] {
        crc = crc::update_upper(crc, &inputs.textures[slot]);
    }
    let w = inputs.wetness;
    let s = inputs.specular;
    for value in [
        inputs.clamp,
        quantized(inputs.alpha, 255.0),
        quantized(inputs.smoothness, 1.0),
        quantized(inputs.fresnel, 1.0),
        quantized(w[0], 255.0),
        quantized(w[1], 1.0),
        quantized(w[2], 255.0),
        quantized(w[3], 1.0),
        quantized(w[4], 255.0),
        quantized(w[5], 255.0),
        quantized(s[0], 255.0),
        quantized(s[1], 255.0),
        quantized(s[2], 255.0),
        quantized(inputs.specular_strength, 255.0),
        quantized(inputs.refraction, 255.0),
        quantized(inputs.subsurface, 255.0),
        quantized(inputs.rimlight, 1.0),
        quantized(inputs.backlight, 1.0),
        0,
    ] {
        crc = crc_u32(crc, value);
    }
    let texture_slots: &[usize] = match inputs.shader_type {
        SHADER_ENVIRONMENT_MAP | SHADER_EYE_ENVMAP => &[4],
        SHADER_GLOW => &[2],
        SHADER_PARALLAX | SHADER_PARALLAX_OCCLUSION => &[3],
        SHADER_FACEGEN => &[5],
        SHADER_MULTILAYER_PARALLAX => &[3, 4],
        _ => &[],
    };
    for &slot in texture_slots {
        crc = crc::update_upper(crc, &inputs.textures[slot]);
    }
    let parameters: &[f32] = match inputs.shader_type {
        SHADER_ENVIRONMENT_MAP => std::slice::from_ref(&inputs.environment_scale),
        SHADER_SKIN_TINT => &inputs.skin_tint,
        SHADER_HAIR_TINT => &inputs.hair_tint,
        SHADER_PARALLAX_OCCLUSION | SHADER_MULTILAYER_PARALLAX | SHADER_SPARKLE_SNOW | SHADER_EYE_ENVMAP => {
            inputs.feature_parameters
        }
        _ => &[],
    };
    for &value in parameters {
        crc = crc_f32(crc, value);
    }
    crc
}

/// Effect material CRC: base stream, five uppercase texture paths, then the
/// falloff, soft-depth, colour and scale floats and four trailing bytes.
fn effect_material_crc32(fields: &IndexMap<String, NifValue>) -> Result<u32> {
    let mut crc = crc_u32(0, EFFECT_MATERIAL_TYPE);
    for value in field_floats(fields, "UV Offset", 2)?.into_iter().chain(field_floats(fields, "UV Scale", 2)?) {
        crc = crc_f32(crc, value);
    }
    crc = crc_u32(crc_u32(crc, 0), 0);
    for name in ["Source Texture", "Greyscale Texture", "Env Map Texture", "Normal Texture", "Env Mask Texture"] {
        crc = crc::update_upper(crc, &field_string(fields, name));
    }
    let mut floats = Vec::with_capacity(11);
    for name in ["Falloff Start Angle", "Falloff Stop Angle", "Falloff Start Opacity", "Falloff Stop Opacity", "Soft Falloff Depth"] {
        floats.extend(field_floats(fields, name, 1)?);
    }
    floats.extend(field_floats(fields, "Base Color", 4)?);
    floats.extend(field_floats(fields, "Base Color Scale", 1)?);
    floats.extend(field_floats(fields, "Environment Map Scale", 1)?);
    for value in floats {
        crc = crc_f32(crc, value);
    }
    let byte = |name: &str| -> Result<u8> {
        u8::try_from(field_uint(fields, name)?)
            .map_err(|_| PrevisError::invalid(format!("effect shader {name} exceeds a byte")))
    };
    let tail = [byte("Texture Clamp Mode")?, byte("Lighting Influence")?, byte("Env Map Min LOD")?, 0];
    Ok(crc::update(crc, &tail))
}

fn scalar_floats(value: &NifValue, out: &mut Vec<f32>) -> bool {
    match value {
        NifValue::Float(v) => out.push(*v as f32),
        NifValue::FloatNan(bits) => out.push(f32::from_bits(*bits as u32)),
        NifValue::Vec3(v) | NifValue::Color3(v) => out.extend(v),
        NifValue::Vec4(v) | NifValue::Color4(v) => out.extend(v),
        NifValue::Struct(fields) => return fields.values().all(|v| scalar_floats(v, out)),
        _ => return false,
    }
    true
}

/// A float field, or the `count` floats of a vector/colour field, in order.
fn field_floats(fields: &IndexMap<String, NifValue>, name: &str, count: usize) -> Result<Vec<f32>> {
    let mut out = Vec::with_capacity(count);
    match get_bare(fields, name) {
        Some(value) if scalar_floats(value, &mut out) && out.len() == count => Ok(out),
        _ => Err(PrevisError::invalid(format!("shader field {name} is not {count} float(s)"))),
    }
}

fn field_uint(fields: &IndexMap<String, NifValue>, name: &str) -> Result<u64> {
    uint(get_bare(fields, name)).ok_or_else(|| PrevisError::invalid(format!("shader field {name} is not an integer")))
}

fn field_string(fields: &IndexMap<String, NifValue>, name: &str) -> String {
    match get_bare(fields, name) {
        Some(NifValue::String(s)) => clean(s).to_string(),
        _ => String::new(),
    }
}

/// Expands a retained shader the way CK's combiner does, after the
/// reference's material swaps. `texture_set` is the source shader's
/// `BSShaderTextureSet`; `color_index` is the base model's `MODC`.
pub fn expand_shader(
    source: &NifBlock,
    texture_set: Option<&NifBlock>,
    assets: &AssetResolver,
    swap: SwapLayers,
    color_index: Option<f32>,
) -> Result<ExpandedShader> {
    if !matches!(source.type_name.as_str(), LIGHTING_SHADER | EFFECT_SHADER) {
        return Err(PrevisError::unsupported(format!(
            "precombine requires a lighting or effect shader, not {}",
            source.type_name
        )));
    }
    if uint(get_bare(&source.fields, "Num Extra Data List")).unwrap_or(0) != 0
        || !matches!(get_bare(&source.fields, "Controller"), Some(NifValue::Ref(-1)) | None)
    {
        return Err(PrevisError::unsupported("shader has extra data or a controller"));
    }
    let named = !field_string(&source.fields, "Name").is_empty();
    match (source.type_name.as_str(), named) {
        (LIGHTING_SHADER, true) => expand_lighting_shader(source, assets, swap, color_index),
        (LIGHTING_SHADER, false) => clone_unnamed_lighting_shader(source, texture_set, swap, color_index),
        (_, true) => expand_effect_shader(source, assets, swap),
        (_, false) => clone_unnamed_effect_shader(source, swap),
    }
}

fn material_name_after_swap(source_name: &str, assets: &AssetResolver, swap: SwapLayers) -> Result<String> {
    // CK keeps the original material when the replacement fails to load.
    if let Some(replacement) = swap.replacement(source_name)? {
        if assets.read(&material_relative_path(&replacement))?.is_some() {
            return Ok(replacement);
        }
    }
    Ok(source_name.to_string())
}

/// A swap is keyed by material name; one that would match an unnamed shader
/// (a bare wildcard) has no observed CK behaviour.
fn reject_swap_of_unnamed(swap: SwapLayers) -> Result<()> {
    if swap.replacement("")?.is_some() || swap.color_index("")?.is_some() {
        return Err(PrevisError::unsupported("a material swap matching an unnamed shader is not verified"));
    }
    Ok(())
}

fn checked_flags(flags1: u32, flags2: u32) -> Result<()> {
    if flags1 & !flag1::KNOWN != 0 || flags2 & !flag2::KNOWN != 0 {
        return Err(PrevisError::unsupported(format!(
            "shader flags are outside the verified subset: 0x{:08X}/0x{:08X}",
            flags1 & !flag1::KNOWN,
            flags2 & !flag2::KNOWN
        )));
    }
    Ok(())
}

/// CK keeps every source field, texture and feature payload of an unnamed
/// lighting shader, adding only `Transform_Changed` (and dropping the Eye
/// Envmap source flag, which the feature ID carries instead).
fn clone_unnamed_lighting_shader(
    source: &NifBlock,
    texture_set: Option<&NifBlock>,
    swap: SwapLayers,
    color_index: Option<f32>,
) -> Result<ExpandedShader> {
    reject_swap_of_unnamed(swap)?;
    let mut fields = source.fields.clone();
    let shader_type = field_uint(&fields, "Shader Type")? as u32;
    if !UNNAMED_LIGHTING_FEATURES.contains(&shader_type) {
        return Err(PrevisError::unsupported(format!(
            "unnamed lighting feature {shader_type} is outside the verified subset"
        )));
    }
    let mut flags1 = field_uint(&fields, "Shader Flags 1")? as u32;
    if shader_type == SHADER_EYE_ENVMAP {
        flags1 &= !flag1::EYE_ENVIRONMENT_MAPPING;
    }
    let flags2 = field_uint(&fields, "Shader Flags 2")? as u32 | flag2::TRANSFORM_CHANGED;
    checked_flags(flags1, flags2)?;
    if color_index.is_some() && flags1 & flag1::GREYSCALE_TO_PALETTE_COLOR != 0 {
        return Err(PrevisError::unsupported("a model color index on an unnamed palette shader is not verified"));
    }
    let Some(NifValue::Array(slots)) = texture_set.and_then(|set| get_bare(&set.fields, "Textures")) else {
        return Err(PrevisError::unsupported("unnamed lighting shader requires a texture set"));
    };
    let textures: Vec<String> = slots
        .iter()
        .map(|slot| match slot {
            NifValue::String(s) => clean(s).to_string(),
            _ => String::new(),
        })
        .collect();
    if textures.len() < 8 {
        return Err(PrevisError::invalid("lighting texture set has fewer than eight slots"));
    }
    set_bare(&mut fields, "Shader Flags 1", NifValue::UInt(flags1 as u64));
    set_bare(&mut fields, "Shader Flags 2", NifValue::UInt(flags2 as u64));

    let wetness = field_floats(&fields, "Wetness", 6)?;
    let specular = field_floats(&fields, "Specular Color", 3)?;
    let optional = |name: &str, count: usize| field_floats(&fields, name, count).unwrap_or_else(|_| vec![0.0; count]);
    let skin = [optional("Skin Tint Color", 3), optional("Skin Tint Alpha", 1)].concat();
    let feature_parameters: Vec<f32> = match shader_type {
        SHADER_PARALLAX_OCCLUSION => [field_floats(&fields, "Max Passes", 1)?, field_floats(&fields, "Scale", 1)?].concat(),
        SHADER_MULTILAYER_PARALLAX => [
            field_floats(&fields, "Parallax Inner Layer Thickness", 1)?,
            field_floats(&fields, "Parallax Refraction Scale", 1)?,
            field_floats(&fields, "Parallax Inner Layer Texture Scale", 2)?,
            field_floats(&fields, "Parallax Envmap Strength", 1)?,
        ]
        .concat(),
        SHADER_SPARKLE_SNOW => field_floats(&fields, "Sparkle Parameters", 4)?,
        SHADER_EYE_ENVMAP => [
            field_floats(&fields, "Eye Cubemap Scale", 1)?,
            field_floats(&fields, "Left Eye Reflection Center", 3)?,
            field_floats(&fields, "Right Eye Reflection Center", 3)?,
        ]
        .concat(),
        _ => Vec::new(),
    };
    let one = |name: &str| field_floats(&fields, name, 1).map(|v| v[0]);
    let uv_offset = field_floats(&fields, "UV Offset", 2)?;
    let uv_scale = field_floats(&fields, "UV Scale", 2)?;
    let material_crc32 = lighting_material_crc32(&CrcInputs {
        shader_type,
        uv_offset: [uv_offset[0], uv_offset[1]],
        uv_scale: [uv_scale[0], uv_scale[1]],
        textures: &textures,
        clamp: field_uint(&fields, "Texture Clamp Mode")? as u32,
        alpha: one("Alpha")?,
        smoothness: one("Smoothness")?,
        fresnel: one("Fresnel Power")?,
        wetness: [wetness[0], wetness[1], wetness[2], wetness[3], wetness[4], wetness[5]],
        specular: [specular[0], specular[1], specular[2]],
        specular_strength: one("Specular Strength")?,
        refraction: one("Refraction Strength")?,
        subsurface: one("Subsurface Rolloff")?,
        rimlight: one("Rimlight Power")?,
        backlight: one("Backlight Power")?,
        environment_scale: optional("Environment Map Scale", 1)[0],
        hair_tint: {
            let hair = optional("Hair Tint Color", 3);
            [hair[0], hair[1], hair[2]]
        },
        skin_tint: [skin[0], skin[1], skin[2], skin[3]],
        feature_parameters: &feature_parameters,
    });
    Ok(ExpandedShader {
        grayscale: one("Grayscale to Palette Scale")?,
        fields,
        source_shader_type: shader_type,
        shader_type,
        flags1,
        flags2,
        textures: Some(textures),
        material_crc32,
        alpha: AlphaSource::Source,
    })
}

/// CK capitalises the leading `textures\` token and the `.dds` extension of
/// an unnamed effect shader's source texture.
fn canonical_effect_source_texture(value: &str) -> String {
    let mut out = value.to_string();
    if out.to_ascii_lowercase().starts_with("textures\\") {
        out.replace_range(..1, "T");
    }
    if out.to_ascii_lowercase().ends_with(".dds") {
        let at = out.len() - 4;
        out.replace_range(at.., ".DDS");
    }
    out
}

fn clone_unnamed_effect_shader(source: &NifBlock, swap: SwapLayers) -> Result<ExpandedShader> {
    reject_swap_of_unnamed(swap)?;
    let mut fields = source.fields.clone();
    let flags1 = field_uint(&fields, "Shader Flags 1")? as u32;
    let flags2 = field_uint(&fields, "Shader Flags 2")? as u32 | flag2::TRANSFORM_CHANGED;
    checked_flags(flags1, flags2)?;
    set_bare(&mut fields, "Shader Flags 2", NifValue::UInt(flags2 as u64));
    let source_texture = canonical_effect_source_texture(&field_string(&fields, "Source Texture"));
    set_bare(&mut fields, "Source Texture", NifValue::String(source_texture));
    effect_shader(fields, flags1, flags2)
}

fn effect_shader(fields: IndexMap<String, NifValue>, flags1: u32, flags2: u32) -> Result<ExpandedShader> {
    Ok(ExpandedShader {
        material_crc32: effect_material_crc32(&fields)?,
        fields,
        source_shader_type: 0,
        shader_type: 0,
        flags1,
        flags2,
        textures: None,
        alpha: AlphaSource::Source,
        grayscale: EFFECT_LOOKUP_SCALE,
    })
}

fn read_bgem(assets: &AssetResolver, relative: &str) -> Result<BgemData> {
    let bytes = assets
        .read(relative)?
        .ok_or_else(|| PrevisError::unsupported(format!("material not found: {relative}")))?;
    let material = materials_native::bgem::parse(&bytes).map_err(|e| PrevisError::invalid(format!("{relative}: {e}")))?;
    if material.header.version != 2 {
        return Err(PrevisError::unsupported(format!(
            "only the verified FO4 BGEM v2 subset is supported: {relative}"
        )));
    }
    Ok(material)
}

fn set_flag(flags: &mut u32, bit: u32, enabled: bool) {
    if enabled {
        *flags |= bit;
    } else {
        *flags &= !bit;
    }
}

/// Rebuilds a named effect shader from its BGEM, as CK's finalization does.
fn expand_effect_shader(source: &NifBlock, assets: &AssetResolver, swap: SwapLayers) -> Result<ExpandedShader> {
    let name = material_name_after_swap(&field_string(&source.fields, "Name"), assets, swap)?;
    let relative = material_relative_path(&name);
    let material = read_bgem(assets, &relative)?;
    let header = &material.header;
    let mut flags1 = field_uint(&source.fields, "Shader Flags 1")? as u32;
    let mut flags2 = field_uint(&source.fields, "Shader Flags 2")? as u32;
    for (bit, enabled) in [
        (flag1::GREYSCALE_TO_PALETTE_COLOR, header.grayscale_to_palette_color),
        (flag1::GREYSCALE_TO_PALETTE_ALPHA, material.GrayscaleToPaletteAlpha),
        (flag1::USE_FALLOFF, material.FalloffEnabled),
        (flag1::ENVIRONMENT_MAPPING, header.env_mapping.unwrap_or(false)),
        (flag1::RGB_FALLOFF, material.FalloffColorEnabled),
        (flag1::REFRACTION, header.refraction),
        (flag1::FIRE_REFRACTION, header.refraction_falloff),
        // Retail and CK 1.11.240 output set it exactly when effect lighting is on.
        (flag1::EXTERNAL_EMITTANCE, material.EffectLightingEnabled),
        (flag1::DECAL, header.decal),
        (flag1::DYNAMIC_DECAL, header.decal),
        (flag1::SOFT_EFFECT, material.SoftEnabled),
        (flag1::ZBUFFER_TEST, header.zbuffer_test),
    ] {
        set_flag(&mut flags1, bit, enabled);
    }
    for (bit, enabled) in [
        (flag2::ZBUFFER_WRITE, header.zbuffer_write),
        (flag2::NO_FADE, header.decal_nofade),
        (flag2::DOUBLE_SIDED, header.two_sided),
        (flag2::GLOW_MAP, material.Glowmap.unwrap_or(false)),
        (flag2::WEAPON_BLOOD, material.BloodEnabled),
        (flag2::EFFECT_LIGHTING, material.EffectLightingEnabled),
        (flag2::TRANSFORM_CHANGED, true),
    ] {
        set_flag(&mut flags2, bit, enabled);
    }
    checked_flags(flags1, flags2)?;
    // CK truncates: a 0.5 influence (127.5) is written as 127.
    let lighting_influence = material.LightingInfluence * 255.0;
    if !(0.0..256.0).contains(&lighting_influence) {
        return Err(PrevisError::invalid(format!("lighting influence is outside byte range: {relative}")));
    }
    let [r, g, b] = material.BaseColor;
    let mut fields = source.fields.clone();
    for (field, value) in [
        ("Name", NifValue::String(output_material_name(&name))),
        ("Shader Flags 1", NifValue::UInt(flags1 as u64)),
        ("Shader Flags 2", NifValue::UInt(flags2 as u64)),
        ("UV Offset", uv(header.u_offset, header.v_offset)),
        ("UV Scale", uv(header.u_scale, header.v_scale)),
        ("Source Texture", NifValue::String(output_texture_name(Some(&material.BaseTexture)))),
        ("Texture Clamp Mode", NifValue::UInt(clamp_mode(header.tile_u, header.tile_v) as u64)),
        ("Lighting Influence", NifValue::UInt(lighting_influence as u64)),
        ("Env Map Min LOD", NifValue::UInt(material.EnvmapMinLOD as u64)),
        ("Unused Byte", NifValue::UInt(0)),
        ("Falloff Start Angle", float(material.FalloffStartAngle)),
        ("Falloff Stop Angle", float(material.FalloffStopAngle)),
        ("Falloff Start Opacity", float(material.FalloffStartOpacity)),
        ("Falloff Stop Opacity", float(material.FalloffStopOpacity)),
        (
            "Base Color",
            NifValue::Struct(IndexMap::from([
                ("r".to_string(), float(r)),
                ("g".to_string(), float(g)),
                ("b".to_string(), float(b)),
                ("a".to_string(), float(header.alpha)),
            ])),
        ),
        ("Base Color Scale", float(material.BaseColorScale)),
        ("Soft Falloff Depth", float(material.SoftDepth)),
        ("Greyscale Texture", NifValue::String(output_texture_name(Some(&material.GrayscaleTexture)))),
        ("Env Map Texture", NifValue::String(output_texture_name(Some(&material.EnvmapTexture)))),
        ("Normal Texture", NifValue::String(output_texture_name(Some(&material.NormalTexture)))),
        ("Env Mask Texture", NifValue::String(output_texture_name(Some(&material.EnvmapMaskTexture)))),
        ("Environment Map Scale", float(header.env_mapping_mask_scale.unwrap_or(0.0))),
    ] {
        set_bare(&mut fields, field, value);
    }
    effect_shader(fields, flags1, flags2)
}

/// Rebuilds a named BGSM-backed lighting shader from its material.
fn expand_lighting_shader(
    source: &NifBlock,
    assets: &AssetResolver,
    swap: SwapLayers,
    color_index: Option<f32>,
) -> Result<ExpandedShader> {
    let fields_in = &source.fields;
    let source_name = field_string(fields_in, "Name");
    let name = material_name_after_swap(&source_name, assets, swap)?;
    let source_shader_type = uint(get_bare(fields_in, "Shader Type")).unwrap_or(0) as u32;
    let source_flags1 = uint(get_bare(fields_in, "Shader Flags 1")).unwrap_or(0) as u32;
    let source_flags2 = uint(get_bare(fields_in, "Shader Flags 2")).unwrap_or(0) as u32;
    let texture_set = match get_bare(fields_in, "Texture Set") {
        Some(NifValue::Ref(r)) => *r,
        _ => -1,
    };

    let relative = material_relative_path(&name);
    let material = read_bgsm(assets, &relative)?;
    let chain = material_chain(assets, material.clone())?;
    let header = &material.header;
    let env_mapping = header.env_mapping.unwrap_or(false);

    let mut flags1 = source_flags1 & !flag1::RGB_FALLOFF;
    let mut flags2 = source_flags2;
    let specular_enabled = material.SpecularEnabled && material.SpecularMult != 0.0;
    if !specular_enabled {
        flags1 &= !flag1::SPECULAR;
    }
    if !material.CastShadows {
        flags1 &= !flag1::CAST_SHADOWS;
    }
    if !env_mapping {
        flags1 &= !flag1::ENVIRONMENT_MAPPING;
    }
    // CK 1.11.240 output clears these source bits whenever the material
    // (typically a swapped-in one) leaves them off.
    set_flag(&mut flags1, flag1::GREYSCALE_TO_PALETTE_COLOR, header.grayscale_to_palette_color);
    set_flag(&mut flags1, flag1::SCREENDOOR_ALPHA_FADE, material.DissolveFade);
    set_flag(&mut flags2, flag2::DOUBLE_SIDED, header.two_sided);
    set_flag(&mut flags2, flag2::NO_FADE, header.decal_nofade);
    let material_driven = texture_set < 0;
    for (enabled, bit) in [
        (material_driven && specular_enabled, flag1::SPECULAR),
        (env_mapping, flag1::ENVIRONMENT_MAPPING),
        (material.CastShadows, flag1::CAST_SHADOWS),
        (material.ModelSpaceNormals, flag1::MODEL_SPACE_NORMALS),
        (header.refraction, flag1::REFRACTION),
        (header.refraction_falloff, flag1::FIRE_REFRACTION),
        (material.Tessellate, flag1::TESSELLATE),
        (material.ExternalEmittance, flag1::EXTERNAL_EMITTANCE),
        (header.decal, flag1::DECAL),
        (header.decal, flag1::DYNAMIC_DECAL),
        (header.zbuffer_test, flag1::ZBUFFER_TEST),
    ] {
        if enabled {
            flags1 |= bit;
        }
    }
    for (enabled, bit) in [
        (material.SkewSpecularAlpha.unwrap_or(false), flag2::SKEW_SPECULAR_ALPHA),
        (material.Tree, flag2::TREE_ANIM),
        (material.Glowmap && !env_mapping, flag2::GLOW_MAP),
        (true, flag2::TRANSFORM_CHANGED),
    ] {
        if enabled {
            flags2 |= bit;
        }
    }
    checked_flags(flags1, flags2)?;

    let emissive = if material.EmitEnabled {
        material
            .EmittanceColor
            .ok_or_else(|| PrevisError::invalid(format!("enabled emission has no color: {relative}")))?
    } else {
        [0.0; 3]
    };
    let uses_subsurface = material.BackLighting.unwrap_or(false) || material.SubsurfaceLighting.unwrap_or(false);
    let uses_backlight = uses_subsurface || header.refraction_falloff;
    let subsurface = if uses_subsurface {
        material
            .SubsurfaceLightingRolloff
            .ok_or_else(|| PrevisError::invalid(format!("{relative} has no subsurface rolloff")))?
    } else {
        0.0
    };
    let backlight = if uses_backlight {
        material
            .BackLightPower
            .ok_or_else(|| PrevisError::invalid(format!("{relative} has no back-light power")))?
    } else {
        0.0
    };
    let shader_type = if material.Hair {
        SHADER_HAIR_TINT
    } else if material.SkinTint {
        SHADER_SKIN_TINT
    } else if material.Facegen {
        SHADER_FACEGEN
    } else if env_mapping {
        SHADER_ENVIRONMENT_MAP
    } else if material.Glowmap {
        SHADER_GLOW
    } else {
        SHADER_DEFAULT
    };
    let wetness = resolved_wetness(&chain);
    let clamp = clamp_mode(header.tile_u, header.tile_v);
    let rimlight = f32::MAX;
    let environment_scale = header.env_mapping_mask_scale.unwrap_or(0.0);

    // A color remap index replaces the palette scale of greyscale-to-palette
    // shaders (retail oracle: swap CNAM, else the model's MODC).
    let grayscale = match swap.color_index(&source_name)?.or(color_index) {
        Some(index) if flags1 & flag1::GREYSCALE_TO_PALETTE_COLOR != 0 => index,
        _ => material.GrayscaleToPaletteScale,
    };

    let mut fields = fields_in.clone();
    set_bare(&mut fields, "Shader Type", NifValue::UInt(shader_type as u64));
    set_bare(&mut fields, "Name", NifValue::String(output_material_name(&name)));
    set_bare(&mut fields, "Shader Flags 1", NifValue::UInt(flags1 as u64));
    set_bare(&mut fields, "Shader Flags 2", NifValue::UInt(flags2 as u64));
    set_bare(&mut fields, "UV Offset", uv(header.u_offset, header.v_offset));
    set_bare(&mut fields, "UV Scale", uv(header.u_scale, header.v_scale));
    set_bare(&mut fields, "Emissive Color", color(emissive));
    set_bare(&mut fields, "Emissive Multiple", float(material.EmittanceMult));
    set_bare(&mut fields, "Root Material", NifValue::String(String::new()));
    set_bare(&mut fields, "Texture Clamp Mode", NifValue::UInt(clamp as u64));
    set_bare(&mut fields, "Alpha", float(header.alpha));
    set_bare(&mut fields, "Refraction Strength", float(header.refraction_power));
    set_bare(&mut fields, "Smoothness", float(material.Smoothness));
    set_bare(&mut fields, "Specular Color", color(material.SpecularColor));
    set_bare(&mut fields, "Specular Strength", float(material.SpecularMult));
    set_bare(&mut fields, "Subsurface Rolloff", float(subsurface));
    set_bare(&mut fields, "Rimlight Power", float(rimlight));
    set_bare(&mut fields, "Backlight Power", float(backlight));
    set_bare(&mut fields, "Grayscale to Palette Scale", float(grayscale));
    set_bare(&mut fields, "Fresnel Power", float(material.FresnelPower));
    set_bare(
        &mut fields,
        "Wetness",
        NifValue::Struct(
            WETNESS_NAMES
                .iter()
                .zip(wetness)
                .map(|(name, value)| (name.to_string(), float(value)))
                .collect(),
        ),
    );
    if material.Hair {
        set_bare(&mut fields, "Hair Tint Color", color(material.HairTintColor));
    }
    if material.SkinTint {
        set_bare(&mut fields, "Skin Tint Color", color([0.0; 3]));
        set_bare(&mut fields, "Skin Tint Alpha", float(0.0));
    }
    if env_mapping {
        set_bare(&mut fields, "Environment Map Scale", float(environment_scale));
        set_bare(&mut fields, "Use Screen Space Reflections", NifValue::UInt(header.ssr as u64));
        set_bare(
            &mut fields,
            "Wetness Control: Use SSR",
            NifValue::UInt(chain.iter().any(|m| m.header.wet_ssr) as u64),
        );
    }

    let textures: Vec<String> = [
        Some(material.DiffuseTexture.as_str()),
        Some(material.NormalTexture.as_str()),
        material.GlowTexture.as_deref(),
        Some(material.GreyscaleTexture.as_str()),
        material.EnvmapTexture.as_deref(),
        if material.Facegen { material.WrinklesTexture.as_deref() } else { None },
        material.InnerLayerTexture.as_deref(),
        Some(material.SmoothSpecTexture.as_str()),
        if material.Tessellate { material.DisplacementTexture.as_deref() } else { None },
        None,
    ]
    .into_iter()
    .map(output_texture_name)
    .collect();

    let material_crc32 = lighting_material_crc32(&CrcInputs {
        shader_type,
        uv_offset: [header.u_offset, header.v_offset],
        uv_scale: [header.u_scale, header.v_scale],
        textures: &textures,
        clamp,
        alpha: header.alpha,
        smoothness: material.Smoothness,
        fresnel: material.FresnelPower,
        wetness,
        specular: material.SpecularColor,
        specular_strength: material.SpecularMult,
        refraction: header.refraction_power,
        subsurface,
        rimlight,
        backlight,
        environment_scale,
        hair_tint: material.HairTintColor,
        skin_tint: [0.0; 4],
        feature_parameters: &[],
    });

    let material_alpha = (header.alpha_test || header.alpha_blend_mode0 != 0).then(|| Alpha {
        flags: (header.alpha_blend_mode0 as u32
            | (header.alpha_blend_mode1 << 1)
            | (header.alpha_blend_mode2 << 5)
            | ((header.alpha_test as u32) << 9)
            | ((material.EnableEditorAlphaRef as u32) << 15)) as u16,
        threshold: header.alpha_test_ref,
    });

    Ok(ExpandedShader {
        fields,
        source_shader_type,
        shader_type,
        flags1,
        flags2,
        textures: Some(textures),
        material_crc32,
        alpha: AlphaSource::Material(material_alpha),
        grayscale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_material_name_matches_ck_casing() {
        assert_eq!(
            output_material_name("materials\\architecture\\buildings\\x.BGSM"),
            "Materials\\Architecture\\Buildings\\x.BGSM"
        );
        assert_eq!(
            output_material_name("Materials/Actors/Character/Hair/h.bgsm"),
            "Materials\\actors\\character\\Hair\\h.bgsm"
        );
        assert_eq!(output_material_name("C:\\Build\\materials\\a.bgsm"), "C:\\Build\\materials\\a.bgsm");
    }

    fn block(type_name: &str, fields: Vec<(&str, NifValue)>) -> NifBlock {
        let mut block = NifBlock::new(0, type_name.to_string());
        block.fields = fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        block
    }

    fn no_assets() -> AssetResolver {
        AssetResolver::new(Vec::new(), &[]).unwrap()
    }

    #[test]
    fn unnamed_lighting_shader_is_a_source_clone_with_transform_changed() {
        let wetness = NifValue::Struct(WETNESS_NAMES.iter().map(|n| (n.to_string(), float(-1.0))).collect());
        let source = block(
            LIGHTING_SHADER,
            vec![
                ("Shader Type", NifValue::UInt(0)),
                ("Name", NifValue::String(String::new())),
                ("Shader Flags 1", NifValue::UInt(0x8040_0201)),
                ("Shader Flags 2", NifValue::UInt(0x21)),
                ("UV Offset", uv(0.0, 0.0)),
                ("UV Scale", uv(1.0, 1.0)),
                ("Texture Set", NifValue::Ref(1)),
                ("Texture Clamp Mode", NifValue::UInt(3)),
                ("Alpha", float(1.0)),
                ("Refraction Strength", float(0.0)),
                ("Smoothness", float(1.0)),
                ("Specular Color", color([0.5, 0.25, 1.0])),
                ("Specular Strength", float(1.0)),
                ("Subsurface Rolloff", float(0.3)),
                ("Rimlight Power", float(2.0)),
                ("Backlight Power", float(0.0)),
                ("Grayscale to Palette Scale", float(0.5)),
                ("Fresnel Power", float(5.0)),
                ("Wetness", wetness),
            ],
        );
        let slots: Vec<NifValue> = ["textures\\a_d.dds", "textures\\a_n.dds", "", "", "", "", "", "textures\\a_s.dds", "", ""]
            .iter()
            .map(|t| NifValue::String(t.to_string()))
            .collect();
        let texture_set = block("BSShaderTextureSet", vec![("Textures", NifValue::Array(slots))]);
        let expanded = expand_shader(&source, Some(&texture_set), &no_assets(), SwapLayers::default(), None).unwrap();
        assert_eq!(expanded.flags2, 0x21 | flag2::TRANSFORM_CHANGED);
        assert_eq!(expanded.textures.as_ref().map(|t| t[7].as_str()), Some("textures\\a_s.dds"));
        assert_eq!(expanded.grayscale, 0.5);
        assert!(matches!(expanded.alpha, AlphaSource::Source));
        let mut fields = expanded.fields.clone();
        set_bare(&mut fields, "Shader Flags 2", NifValue::UInt(0x21));
        assert_eq!(fields, source.fields);
    }

    fn unnamed_effect(source_texture: &str, lighting_influence: u64) -> NifBlock {
        let base_color = NifValue::Struct(["r", "g", "b", "a"].iter().map(|c| (c.to_string(), float(1.0))).collect());
        let text = |s: &str| NifValue::String(s.to_string());
        block(
            EFFECT_SHADER,
            vec![
                ("Name", text("")),
                ("Shader Flags 1", NifValue::UInt(0x8000_0000)),
                ("Shader Flags 2", NifValue::UInt(0x21)),
                ("UV Offset", uv(0.0, 0.0)),
                ("UV Scale", uv(1.0, 1.0)),
                ("Source Texture", text(source_texture)),
                ("Texture Clamp Mode", NifValue::UInt(3)),
                ("Lighting Influence", NifValue::UInt(lighting_influence)),
                ("Env Map Min LOD", NifValue::UInt(0)),
                ("Unused Byte", NifValue::UInt(0)),
                ("Falloff Start Angle", float(1.0)),
                ("Falloff Stop Angle", float(1.0)),
                ("Falloff Start Opacity", float(0.0)),
                ("Falloff Stop Opacity", float(0.0)),
                ("Base Color", base_color),
                ("Base Color Scale", float(1.0)),
                ("Soft Falloff Depth", float(100.0)),
                ("Greyscale Texture", text("")),
                ("Env Map Texture", text("")),
                ("Normal Texture", text("")),
                ("Env Mask Texture", text("")),
                ("Environment Map Scale", float(1.0)),
            ],
        )
    }

    #[test]
    fn unnamed_effect_shader_is_cloned_with_ck_source_texture_casing() {
        let expand = |source: &NifBlock| expand_shader(source, None, &no_assets(), SwapLayers::default(), None).unwrap();
        let expanded = expand(&unnamed_effect("textures\\effects\\glow.dds", 255));
        assert_eq!(get_bare(&expanded.fields, "Source Texture"), Some(&NifValue::String("Textures\\effects\\glow.DDS".into())));
        assert_eq!(expanded.flags2, 0x21 | flag2::TRANSFORM_CHANGED);
        assert!(expanded.textures.is_none());
        assert_eq!(expanded.grayscale, EFFECT_LOOKUP_SCALE);
        // The material CRC hashes texture paths uppercased but every byte of the tail.
        assert_eq!(expanded.material_crc32, expand(&unnamed_effect("TEXTURES\\EFFECTS\\GLOW.DDS", 255)).material_crc32);
        assert_ne!(expanded.material_crc32, expand(&unnamed_effect("textures\\effects\\glow.dds", 254)).material_crc32);
    }

    #[test]
    fn quantization_rounds_half_up_and_wraps() {
        assert_eq!(quantized(0.5, 255.0), 128);
        assert_eq!(quantized(1.0, 1.0), 1);
        assert_eq!(quantized(f32::MAX, 1.0), 0);
        assert_eq!(quantized(-1.0, 255.0), (-255i32) as u32);
    }
}
