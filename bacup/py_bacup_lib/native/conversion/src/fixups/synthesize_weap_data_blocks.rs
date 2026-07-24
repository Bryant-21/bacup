//! Fixup: synthesize FO4 DNAM (weapon stats) and FNAM (animation) subrecords for
//! translated WEAP records that lack them.
//!

//!
//! # What this does
//! FO76 WEAP records store weapon stats in a DNAM binary blob whose layout
//! differs from FO4's.  After FO76→FO4 translation the DNAM is either absent
//! or contains FO76 bytes that don't decode under the FO4 codec.  This fixup
//! replaces the DNAM with a DNAM containing sensible FO4 default values,
//! preserving compatible FO76 fields (including base min/max range), and
//! injects an FNAM with FO4 animation defaults when FNAM is missing.
//! FNV/FO3 use a different legacy layout: inventory stats live in DATA,
//! ammo/sound level are separate subrecords, and animation data lives in
//! DNAM. Those sources are rebuilt field-by-field instead of being interpreted
//! with FO76 offsets.
//!
//! Creature weapons (root record type NPC_ or LVLN) receive specialised
//! defaults; melee/unarmed creature weapons additionally translate the source
//! melee geometry (speed, reach, min/max range) and combat fields so the AI
//! closes to attack instead of holding the stamped default engage range.
//!
//! # FO4 DNAM struct layout (codec `I,f,f,f,f,f,f,f,f,I,I,I,I,H,B,f,f,I,H,I,I,I,I,I,I,I,I,I,B,f,B,B,f,f,f,I,B,B,B,B`)
//!
//! | Offset | Size | Type | Field                   |
//! |--------|------|------|-------------------------|
//! |      0 |    4 |    I | ammo (formid)           |
//! |      4 |    4 |    f | speed                   |
//! |      8 |    4 |    f | reload_speed            |
//! |     12 |    4 |    f | reach                   |
//! |     16 |    4 |    f | min_range               |
//! |     20 |    4 |    f | max_range               |
//! |     24 |    4 |    f | attack_delay            |
//! |     28 |    4 |    f | unused                  |
//! |     32 |    4 |    f | damage_outofrange_mult  |
//! |     36 |    4 |    I | on_hit                  |
//! |     40 |    4 |    I | skill                   |
//! |     44 |    4 |    I | resist                  |
//! |     48 |    4 |    I | flags (bitmask)         |
//! |     52 |    2 |    H | capacity                |
//! |     54 |    1 |    B | animation_type          |
//! |     55 |    4 |    f | damage_secondary        |
//! |     59 |    4 |    f | weight                  |
//! |     63 |    4 |    I | value                   |
//! |     67 |    2 |    H | damage_base             |
//! |     69 |    4 |    I | sound_level             |
//! |     73 |    4 |    I | sound_attack            |
//! |     77 |    4 |    I | sound_attack_2d         |
//! |     81 |    4 |    I | sound_attack_loop       |
//! |     85 |    4 |    I | sound_attack_fail       |
//! |     89 |    4 |    I | sound_idle              |
//! |     93 |    4 |    I | sound_equip_sound       |
//! |     97 |    4 |    I | sound_unequip_sound     |
//! |    101 |    4 |    I | sound_fast_equip_sound  |
//! |    105 |    1 |    B | accuracy_bonus          |
//! |    106 |    4 |    f | animation_attack_seconds|
//! |    110 |    1 |    B | unknown_u8_30           |
//! |    111 |    1 |    B | unknown_u8_31           |
//! |    112 |    4 |    f | action_point_cost       |
//! |    116 |    4 |    f | full_power_seconds      |
//! |    120 |    4 |    f | min_power_per_shot      |
//! |    124 |    4 |    I | stagger (enum)          |
//! |    128 |    1 |    B | unknown_u8_36           |
//! |    129 |    1 |    B | unknown_u8_37           |
//! |    130 |    1 |    B | unknown_u8_38           |
//! |    131 |    1 |    B | unknown_u8_39           |
//!
//! Total DNAM size: 132 bytes.
//!
//! # FO4 FNAM struct layout (codec `f,f,f,f,f,B,B,B,B,f,B,I,I,I`)
//!
//! | Offset | Size | Field                       |
//! |--------|------|-----------------------------|
//! |      0 |    4 | animation_fire_seconds      |
//! |      4 |    4 | rumble_left_motor_strength  |
//! |      8 |    4 | rumble_right_motor_strength |
//! |     12 |    4 | rumble_duration             |
//! |     16 |    4 | animation_reload_seconds    |
//! |     20 |    1 | bolt_anim_byte_1            |
//! |     21 |    1 | bolt_anim_byte_2            |
//! |     22 |    1 | bolt_anim_byte_3            |
//! |     23 |    1 | bolt_anim_byte_4            |
//! |     24 |    4 | sighted_transition_seconds  |
//! |     28 |    1 | projectiles                 |
//! |     29 |    4 | override_projectile (formid)|
//! |     33 |    4 | pattern                     |
//! |     37 |    4 | rumble_period_ms            |
//!
//! Total FNAM size: 41 bytes.
//!
//! # Stagger enum  (stagger_enum)
//! None=0, Small=1, Medium=2, Large=3, Extra Large=4.
//!
//! # SoundLevel enum  (sound_level_enum)
//! Loud=0, Normal=1, Silent=2, Very Loud=3, Quiet=4.
//!
//! # DNAM flags bitmask  (WEAP.DNAM.flags)
//! CritEffectOnDeath=256, ChargingAttack=512, Automatic=32768, CantDrop=131072,
//! NotPlayable=1048576.

use crate::fixups::creature::{
    likely_creature_weapon_editor_id, likely_ranged_creature_weapon_editor_id,
};
use crate::fixups::curve_table::{CurveMeanCache, cached_curve_mean, source_key_for_target};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};
use rustc_hash::FxHashMap;
use std::path::Path;

// ---------------------------------------------------------------------------
// DNAM flags constants
// ---------------------------------------------------------------------------

const FLAG_CRIT_EFFECT_ON_DEATH: u32 = 256;
const FLAG_CHARGING_ATTACK: u32 = 512;
const FLAG_AUTOMATIC: u32 = 32_768;
const FLAG_CANT_DROP: u32 = 131_072;
const FLAG_NOT_PLAYABLE: u32 = 1_048_576;

/// Every vanilla spin-up automatic (Minigun, GatlingLaser: ChargingAttack|Automatic)
/// carries AnimationFireSeconds=1.0 — the barrel spin-up delay. The instant-fire
/// 1e-5 default lets converted spin-up guns fire before the spin-up completes.
const SPIN_UP_ANIMATION_FIRE_SECONDS: f32 = 1.0;

fn is_spin_up_automatic(dnam_flags: u32) -> bool {
    const SPIN_UP: u32 = FLAG_CHARGING_ATTACK | FLAG_AUTOMATIC;
    dnam_flags & SPIN_UP == SPIN_UP
}

pub const WEAP_SOUND_FIELD_COUNT: usize = 9;
const FO76_DNAM_SOUND_OFFSETS: [usize; WEAP_SOUND_FIELD_COUNT] =
    [83, 87, 91, 95, 99, 103, 107, 111, 115];
const FO4_DNAM_SOUND_OFFSETS: [usize; WEAP_SOUND_FIELD_COUNT] =
    [69, 73, 77, 81, 85, 89, 93, 97, 101];
const FO76_DNAM_ATTACK_DELAY_OFFSET: usize = 36;
const FO76_DNAM_DAMAGE_SECONDARY_OFFSET: usize = 69;
const FO76_DNAM_FULL_POWER_SECONDS_OFFSET: usize = 130;
const FO76_DNAM_MIN_POWER_PER_SHOT_OFFSET: usize = 134;
const FO76_DNAM_STAGGER_OFFSET: usize = 138;
const FO4_DNAM_ATTACK_DELAY_OFFSET: usize = 24;
const FO4_DNAM_DAMAGE_SECONDARY_OFFSET: usize = 55;
const FO4_DNAM_FULL_POWER_SECONDS_OFFSET: usize = 116;
const FO4_DNAM_MIN_POWER_PER_SHOT_OFFSET: usize = 120;
const FO4_DNAM_STAGGER_OFFSET: usize = 124;
const FO76_TO_FO4_MIN_POWER_PER_SHOT_SCALE: f32 = 0.1;
const FO76_DNAM_SPEED_OFFSET: usize = 4;
const FO76_DNAM_RELOAD_SPEED_OFFSET: usize = 8;
const FO76_DNAM_REACH_OFFSET: usize = 16;
const FO76_DNAM_MIN_RANGE_OFFSET: usize = 24;
const FO76_DNAM_MAX_RANGE_OFFSET: usize = 28;
const FO4_DNAM_SPEED_OFFSET: usize = 4;
const FO4_DNAM_RELOAD_SPEED_OFFSET: usize = 8;
const FO4_DNAM_REACH_OFFSET: usize = 12;
const FO4_DNAM_MIN_RANGE_OFFSET: usize = 16;
const FO4_DNAM_MAX_RANGE_OFFSET: usize = 20;

const FO76_RGW3_RUMBLE_LEFT_MOTOR_STRENGTH_OFFSET: usize = 8;
const FO76_RGW3_RUMBLE_RIGHT_MOTOR_STRENGTH_OFFSET: usize = 12;
const FO76_RGW3_RUMBLE_DURATION_OFFSET: usize = 16;
const FO76_RGW3_ANIMATION_RELOAD_SECONDS_OFFSET: usize = 20;
const FO76_RGW3_PROJECTILES_OFFSET: usize = 52;

const FO4_FNAM_ANIMATION_FIRE_SECONDS_OFFSET: usize = 0;
const FO4_FNAM_RUMBLE_LEFT_MOTOR_STRENGTH_OFFSET: usize = 4;
const FO4_FNAM_RUMBLE_RIGHT_MOTOR_STRENGTH_OFFSET: usize = 8;
const FO4_FNAM_RUMBLE_DURATION_OFFSET: usize = 12;
const FO4_FNAM_ANIMATION_RELOAD_SECONDS_OFFSET: usize = 16;
const FO4_FNAM_PROJECTILES_OFFSET: usize = 28;
const FO4_FNAM_OVERRIDE_PROJECTILE_OFFSET: usize = 29;
const FO4_FNAM_PATTERN_OFFSET: usize = 33;
const FO4_FNAM_RUMBLE_PERIOD_MS_OFFSET: usize = 37;
const FO76_DAMAGE_TYPE_ROW_LEN: usize = 12;

const LEGACY_DATA_LEN: usize = 15;
const LEGACY_DNAM_MIN_LEN: usize = 136;
const LEGACY_DNAM_PROJECTILE_OFFSET: usize = 36;
const LEGACY_DNAM_MIN_RANGE_OFFSET: usize = 44;
const LEGACY_DNAM_MAX_RANGE_OFFSET: usize = 48;
const LEGACY_DNAM_ON_HIT_OFFSET: usize = 52;
const LEGACY_DNAM_FLAGS_2_OFFSET: usize = 56;
const LEGACY_DNAM_RUMBLE_LEFT_OFFSET: usize = 72;
const LEGACY_DNAM_RUMBLE_RIGHT_OFFSET: usize = 76;
const LEGACY_DNAM_RUMBLE_DURATION_OFFSET: usize = 80;
const LEGACY_DNAM_RELOAD_TIME_OFFSET: usize = 92;
const LEGACY_DNAM_RUMBLE_PATTERN_OFFSET: usize = 108;
const LEGACY_DNAM_RUMBLE_WAVELENGTH_OFFSET: usize = 112;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceWeapFamily {
    Fo76,
    LegacyFallout,
    Other,
}

impl SourceWeapFamily {
    fn from_game(game: Option<&str>) -> Self {
        match game {
            Some(game) if game.eq_ignore_ascii_case("fnv") || game.eq_ignore_ascii_case("fo3") => {
                Self::LegacyFallout
            }
            Some(game) if game.eq_ignore_ascii_case("fo76") => Self::Fo76,
            _ => Self::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// Default DNAM/FNAM byte arrays
// ---------------------------------------------------------------------------

/// Default DNAM bytes for standard FO4 weapons.
///
/// Python `_FO4_DATA_DEFAULTS`: Speed=1.0, ReloadSpeed=1.0, Reach=1.0,
/// MinRange=256.0, MaxRange=3072.0, AttackDelay=0.0, DamageOutOfRangeMult=0.5,
/// Flags=[], Capacity=1, AnimationType=9, DamageSecondary=0.0, Weight=1.0,
/// Value=1, DamageBase=1, AnimationAttackSeconds=0.0, ActionPointCost=30.0,
/// Stagger=None(0).  Fields absent from the dict default to zero.
pub fn fo4_default_dnam() -> [u8; 132] {
    let mut buf = [0u8; 132];
    write_f32(&mut buf, 4, 1.0); // speed
    write_f32(&mut buf, 8, 1.0); // reload_speed
    write_f32(&mut buf, 12, 1.0); // reach
    write_f32(&mut buf, 16, 256.0); // min_range
    write_f32(&mut buf, 20, 3072.0); // max_range
    // attack_delay=0, unused=0 (zero init)
    write_f32(&mut buf, 32, 0.5); // damage_outofrange_mult
    // on_hit=0, skill=0, resist=0, flags=0 (zero init)
    write_u16(&mut buf, 52, 1); // capacity
    buf[54] = 9; // animation_type
    // damage_secondary=0 (zero init)
    write_f32(&mut buf, 59, 1.0); // weight
    write_u32(&mut buf, 63, 1); // value
    write_u16(&mut buf, 67, 1); // damage_base
    // sound_level=0, all sounds=0, accuracy_bonus=0 (zero init)
    // animation_attack_seconds=0 (zero init)
    write_f32(&mut buf, 112, 30.0); // action_point_cost
    // full_power_seconds=0, min_power_per_shot=0, stagger=0 (None), unknowns=0
    buf
}

/// Default DNAM bytes for creature unarmed weapons.
///
/// Python `_CREATURE_UNARMED_DATA_DEFAULTS`: Speed=1.0, ReloadSpeed=1.0,
/// Reach=1.0, MinRange=500.0, MaxRange=2000.0, DamageOutOfRangeMult=0.5,
/// Flags=[CritEffectOnDeath, CantDrop, NotPlayable], SoundLevel=Normal(1),
/// AnimationAttackSeconds≈1.5417, ActionPointCost=20.0, Stagger=Small(1).
pub fn creature_unarmed_dnam() -> [u8; 132] {
    let mut buf = [0u8; 132];
    write_f32(&mut buf, 4, 1.0); // speed
    write_f32(&mut buf, 8, 1.0); // reload_speed
    write_f32(&mut buf, 12, 1.0); // reach
    write_f32(&mut buf, 16, 500.0); // min_range
    write_f32(&mut buf, 20, 2000.0); // max_range
    // attack_delay=0, unused=0 (not in defaults)
    write_f32(&mut buf, 32, 0.5); // damage_outofrange_mult
    write_u32(
        &mut buf,
        48,
        FLAG_CRIT_EFFECT_ON_DEATH | FLAG_CANT_DROP | FLAG_NOT_PLAYABLE,
    );
    // capacity=0, animation_type=0 (not in defaults)
    // damage_secondary=0, weight=0, value=0, damage_base=0
    write_u32(&mut buf, 69, 1); // sound_level = Normal
    // all sounds=0, accuracy_bonus=0
    write_f32(&mut buf, 106, 1.541_666_7); // animation_attack_seconds ≈ 1.5416667461
    write_f32(&mut buf, 112, 20.0); // action_point_cost
    // full_power_seconds=0, min_power_per_shot=0
    write_u32(&mut buf, 124, 1); // stagger = Small
    buf
}

/// Default DNAM bytes for creature ranged weapons.
///
/// Python `_CREATURE_RANGED_DATA_DEFAULTS`: Speed=1.0, ReloadSpeed=1.0,
/// Reach=1.0, MinRange=500.0, MaxRange=1500.0, AttackDelay=3.5,
/// Flags=[CantDrop, NotPlayable], AnimationType=9, Weight=3.0, DamageBase=10,
/// SoundLevel=Normal(1), AccuracyBonus=100, AnimationAttackSeconds≈1.8333,
/// ActionPointCost=20.0.
pub fn creature_ranged_dnam() -> [u8; 132] {
    let mut buf = [0u8; 132];
    write_f32(&mut buf, 4, 1.0); // speed
    write_f32(&mut buf, 8, 1.0); // reload_speed
    write_f32(&mut buf, 12, 1.0); // reach
    write_f32(&mut buf, 16, 500.0); // min_range
    write_f32(&mut buf, 20, 1500.0); // max_range
    write_f32(&mut buf, 24, 3.5); // attack_delay
    // unused=0, damage_outofrange_mult=0 (not in defaults)
    write_u32(&mut buf, 48, FLAG_CANT_DROP | FLAG_NOT_PLAYABLE);
    // capacity=0
    buf[54] = 9; // animation_type
    // damage_secondary=0
    write_f32(&mut buf, 59, 3.0); // weight
    // value=0
    write_u16(&mut buf, 67, 10); // damage_base
    write_u32(&mut buf, 69, 1); // sound_level = Normal
    // all sounds=0
    buf[105] = 100; // accuracy_bonus
    write_f32(&mut buf, 106, 1.833_333_4); // animation_attack_seconds ≈ 1.8333333730
    write_f32(&mut buf, 112, 20.0); // action_point_cost
    // full_power_seconds=0, min_power_per_shot=0, stagger=0, unknowns=0
    buf
}

/// Default FNAM bytes.
///
/// Python `_FNAM_DEFAULTS`: AnimationFireSeconds=1e-5,
/// RumbleLeftMotorStrength=0.5, RumbleRightMotorStrength=0.5,
/// RumbleDuration=0.2, AnimationReloadSeconds=2.0,
/// SightedTransitionSeconds=0.15, Projectiles=1.
pub fn default_fnam() -> [u8; 41] {
    let mut buf = [0u8; 41];
    write_f32_41(&mut buf, 0, 1e-5_f32); // animation_fire_seconds
    write_f32_41(&mut buf, 4, 0.5); // rumble_left_motor_strength
    write_f32_41(&mut buf, 8, 0.5); // rumble_right_motor_strength
    write_f32_41(&mut buf, 12, 0.2); // rumble_duration
    write_f32_41(&mut buf, 16, 2.0); // animation_reload_seconds
    // bolt_anim bytes 20-23 = 0
    write_f32_41(&mut buf, 24, 0.15); // sighted_transition_seconds
    buf[28] = 1; // projectiles
    // override_projectile=0, pattern=0, rumble_period_ms=0
    buf
}

// ---------------------------------------------------------------------------
// Byte-write helpers (sized for each buffer type)
// ---------------------------------------------------------------------------

#[inline]
fn write_f32(buf: &mut [u8; 132], offset: usize, v: f32) {
    buf[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
}

#[inline]
fn write_u32(buf: &mut [u8; 132], offset: usize, v: u32) {
    buf[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
}

#[inline]
fn write_u16(buf: &mut [u8; 132], offset: usize, v: u16) {
    buf[offset..offset + 2].copy_from_slice(&v.to_le_bytes());
}

#[inline]
fn write_f32_41(buf: &mut [u8; 41], offset: usize, v: f32) {
    buf[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
}

fn dnam_default_bytes(default: DnamDefault) -> [u8; 132] {
    match default {
        DnamDefault::Fo4 => fo4_default_dnam(),
        DnamDefault::CreatureUnarmed => creature_unarmed_dnam(),
        DnamDefault::CreatureRanged => creature_ranged_dnam(),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SourceWeapFields {
    pub ammo_raw: Option<u32>,
    pub speed: Option<f32>,
    pub reach: Option<f32>,
    pub min_range: Option<f32>,
    pub max_range: Option<f32>,
    pub attack_delay_seconds: Option<f32>,
    pub on_hit: Option<u32>,
    pub flags: Option<u32>,
    pub capacity: Option<u16>,
    pub animation_type: Option<u8>,
    pub damage_secondary: Option<f32>,
    pub weight: Option<f32>,
    pub value: Option<u32>,
    pub damage_base: Option<u16>,
    pub sound_level: Option<u32>,
    pub accuracy_bonus: Option<u8>,
    pub action_point_cost: Option<f32>,
    pub full_power_seconds: Option<f32>,
    pub min_power_per_shot: Option<f32>,
    pub stagger: Option<u32>,
    pub override_projectile_raw: Option<u32>,
    pub rumble_left_motor_strength: Option<f32>,
    pub rumble_right_motor_strength: Option<f32>,
    pub rumble_duration: Option<f32>,
    pub animation_reload_seconds: Option<f32>,
    pub projectiles: Option<u8>,
    pub rumble_pattern: Option<u32>,
    pub rumble_period_ms: Option<u32>,
    pub sound_data_raw: [Option<u32>; WEAP_SOUND_FIELD_COUNT],
    pub curve_damage_base: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResolvedDamageType {
    target_type_raw: u32,
    damage: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceDamageType {
    source_type: FormKey,
    amount: u32,
    curve: Option<FormKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LegacyReferenceResolution {
    Null,
    Mapped(u32),
    Unmapped(FormKey),
}

fn fo4_dnam_from_fo76_raw(
    raw: &[u8],
    default: DnamDefault,
    source_fields: Option<SourceWeapFields>,
) -> [u8; 132] {
    let mut buf = dnam_default_bytes(default);
    if raw.len() < 124 {
        return buf;
    }

    if let Some(ammo_raw) = source_fields.and_then(|fields| fields.ammo_raw) {
        write_u32(&mut buf, 0, ammo_raw);
    }
    if matches!(default, DnamDefault::CreatureUnarmed) {
        // FO76 melee/unarmed DNAM geometry is byte-identical to FO4 vanilla
        // (unarmed 500/2000 reach 1.06, melee 0/10 reach 0.8), so copying it
        // reproduces vanilla exactly. The critical companion is the common
        // animation_type copy below: the RACE behavior-graph table selects
        // the animation branch by weapon anim type, and a melee weapon
        // stamped H2H leaves the creature with no usable attack (AI flees).
        copy_raw::<4>(&mut buf, FO4_DNAM_SPEED_OFFSET, raw, FO76_DNAM_SPEED_OFFSET);
        copy_raw::<4>(
            &mut buf,
            FO4_DNAM_RELOAD_SPEED_OFFSET,
            raw,
            FO76_DNAM_RELOAD_SPEED_OFFSET,
        );
        copy_raw::<4>(&mut buf, FO4_DNAM_REACH_OFFSET, raw, FO76_DNAM_REACH_OFFSET);
    }
    if matches!(default, DnamDefault::Fo4 | DnamDefault::CreatureUnarmed) {
        copy_raw::<4>(
            &mut buf,
            FO4_DNAM_MIN_RANGE_OFFSET,
            raw,
            FO76_DNAM_MIN_RANGE_OFFSET,
        );
        copy_raw::<4>(
            &mut buf,
            FO4_DNAM_MAX_RANGE_OFFSET,
            raw,
            FO76_DNAM_MAX_RANGE_OFFSET,
        );
        copy_raw::<4>(
            &mut buf,
            FO4_DNAM_ATTACK_DELAY_OFFSET,
            raw,
            FO76_DNAM_ATTACK_DELAY_OFFSET,
        );
        copy_raw::<4>(&mut buf, 48, raw, 60);
        copy_raw::<4>(
            &mut buf,
            FO4_DNAM_DAMAGE_SECONDARY_OFFSET,
            raw,
            FO76_DNAM_DAMAGE_SECONDARY_OFFSET,
        );
        if let Some(full_power_seconds) = read_f32(raw, FO76_DNAM_FULL_POWER_SECONDS_OFFSET) {
            write_f32(
                &mut buf,
                FO4_DNAM_FULL_POWER_SECONDS_OFFSET,
                full_power_seconds,
            );
        }
        if let Some(min_power_per_shot) = read_f32(raw, FO76_DNAM_MIN_POWER_PER_SHOT_OFFSET) {
            write_f32(
                &mut buf,
                FO4_DNAM_MIN_POWER_PER_SHOT_OFFSET,
                min_power_per_shot * FO76_TO_FO4_MIN_POWER_PER_SHOT_SCALE,
            );
        }
        copy_raw::<4>(
            &mut buf,
            FO4_DNAM_STAGGER_OFFSET,
            raw,
            FO76_DNAM_STAGGER_OFFSET,
        );
        if let Some(fields) = source_fields {
            apply_source_dnam_fields(&mut buf, fields);
        }
    }
    copy_raw::<2>(&mut buf, 52, raw, 64);
    copy_raw::<1>(&mut buf, 54, raw, 68);
    copy_raw::<4>(&mut buf, 59, raw, 73);
    copy_raw::<4>(&mut buf, 63, raw, 77);
    copy_raw::<2>(&mut buf, 67, raw, 81);
    copy_raw::<4>(&mut buf, 106, raw, 120);
    if let Some(fields) = source_fields {
        for (raw, target_offset) in fields
            .sound_data_raw
            .into_iter()
            .zip(FO4_DNAM_SOUND_OFFSETS)
        {
            if let Some(raw) = raw {
                write_u32(&mut buf, target_offset, raw);
            }
        }
    }
    buf
}

fn fo4_dnam_from_legacy_fields(
    default: DnamDefault,
    source_fields: Option<SourceWeapFields>,
) -> [u8; 132] {
    let mut buf = dnam_default_bytes(default);
    if let Some(fields) = source_fields {
        apply_source_dnam_fields(&mut buf, fields);
    }
    buf
}

fn apply_source_dnam_fields(dnam: &mut [u8; 132], fields: SourceWeapFields) {
    if let Some(value) = fields.ammo_raw {
        write_u32(dnam, 0, value);
    }
    if let Some(value) = fields.speed {
        write_f32(dnam, FO4_DNAM_SPEED_OFFSET, value);
    }
    if let Some(value) = fields.reach {
        write_f32(dnam, FO4_DNAM_REACH_OFFSET, value);
    }
    if let Some(value) = fields.min_range {
        write_f32(dnam, FO4_DNAM_MIN_RANGE_OFFSET, value);
    }
    if let Some(value) = fields.max_range {
        write_f32(dnam, FO4_DNAM_MAX_RANGE_OFFSET, value);
    }
    if let Some(value) = fields.attack_delay_seconds {
        write_f32(dnam, FO4_DNAM_ATTACK_DELAY_OFFSET, value);
    }
    if let Some(value) = fields.on_hit {
        write_u32(dnam, 36, value);
    }
    if let Some(value) = fields.flags {
        write_u32(dnam, 48, value);
    }
    if let Some(value) = fields.capacity {
        write_u16(dnam, 52, value);
    }
    if let Some(value) = fields.animation_type {
        dnam[54] = value;
    }
    if let Some(value) = fields.damage_secondary {
        write_f32(dnam, FO4_DNAM_DAMAGE_SECONDARY_OFFSET, value);
    }
    if let Some(value) = fields.weight {
        write_f32(dnam, 59, value);
    }
    if let Some(value) = fields.value {
        write_u32(dnam, 63, value);
    }
    if let Some(value) = fields.damage_base {
        write_u16(dnam, 67, value);
    }
    if let Some(value) = fields.sound_level {
        write_u32(dnam, 69, value);
    }
    if let Some(value) = fields.accuracy_bonus {
        dnam[105] = value;
    }
    if let Some(value) = fields.action_point_cost {
        write_f32(dnam, 112, value);
    }
    if let Some(value) = fields.full_power_seconds {
        write_f32(dnam, FO4_DNAM_FULL_POWER_SECONDS_OFFSET, value);
    }
    if let Some(value) = fields.min_power_per_shot {
        write_f32(
            dnam,
            FO4_DNAM_MIN_POWER_PER_SHOT_OFFSET,
            value * FO76_TO_FO4_MIN_POWER_PER_SHOT_SCALE,
        );
    }
    if let Some(value) = fields.stagger {
        write_u32(dnam, FO4_DNAM_STAGGER_OFFSET, value);
    }
}

#[inline]
fn copy_raw<const N: usize>(
    target: &mut [u8; 132],
    target_offset: usize,
    source: &[u8],
    source_offset: usize,
) {
    if let Some(bytes) = source.get(source_offset..source_offset + N) {
        target[target_offset..target_offset + N].copy_from_slice(bytes);
    }
}

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct SynthesizeWeapDataBlocksFixup;

impl Fixup for SynthesizeWeapDataBlocksFixup {
    fn name(&self) -> &'static str {
        "synthesize_weap_data_blocks"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let weap_sig =
            SigCode::from_str("WEAP").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let is_creature_root = config.root_sig.map(is_creature_root_sig).unwrap_or(false);

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let source_schema = config.source_schema.as_deref();
        let source_family = SourceWeapFamily::from_game(
            session
                .source_slot_opt()
                .and_then(|slot| slot.parsed.game.as_deref()),
        );
        let source_plugin_info = session.source_slot_opt().map(|slot| {
            (
                slot.parsed.header.masters.clone(),
                slot.parsed.plugin_name.clone(),
                mapper.interner.intern(&slot.parsed.plugin_name),
            )
        });
        let target_plugin_sym = mapper
            .interner
            .intern(&session.target_slot().parsed.plugin_name);
        let target_masters = session.target_masters().to_vec();
        let target_to_source: FxHashMap<FormKey, FormKey> = mapper
            .source_to_target_iter()
            .map(|(source, target)| (target, source))
            .collect();
        let mut report = FixupReport::empty();
        let mut changed_records = Vec::new();
        let mut curve_cache = CurveMeanCache::default();

        let fks = session
            .form_keys_of_sig(weap_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("synth_weap_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            // Resolve EditorID string for creature-weapon classification.
            let eid_str: String = record
                .eid
                .and_then(|sym| mapper.interner.resolve(sym))
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();

            let is_creature_weapon = is_creature_root
                || (config.is_whole_plugin && likely_creature_weapon_editor_id(&eid_str));
            let dnam_default = choose_dnam_default(is_creature_weapon, &eid_str);
            let mut source_fields = None;
            let mut resolved_damage_types = Vec::new();
            let source_fk = source_plugin_info
                .as_ref()
                .and_then(|(_, _, source_plugin_sym)| {
                    source_key_for_target(
                        fk,
                        &target_to_source,
                        target_plugin_sym,
                        *source_plugin_sym,
                    )
                });
            if let (Some(schema), Some(source_fk), Some(source_plugin_info)) =
                (source_schema, source_fk, source_plugin_info.as_ref())
                && let Ok(source_record) =
                    session.source_record_decoded(&source_fk, schema, mapper.interner)
            {
                let (source_masters, source_plugin_name, source_plugin_sym) = source_plugin_info;
                let (mut fields, reference_warnings) = extract_source_weap_fields(
                    &source_record,
                    source_family,
                    source_masters,
                    source_plugin_name,
                    *source_plugin_sym,
                    mapper,
                    &target_masters,
                );
                for warning in reference_warnings {
                    report.warnings.push(
                        mapper
                            .interner
                            .intern(&format!("synth_weap_ref:{eid_str}:{warning}")),
                    );
                }
                if let Some(source_extracted_dir) = config.source_extracted_dir.as_deref() {
                    let (damage_base, damage_types, warnings) = resolve_source_curve_damage(
                        &source_record,
                        session,
                        schema,
                        source_extracted_dir,
                        source_masters,
                        source_plugin_name,
                        *source_plugin_sym,
                        mapper,
                        &target_masters,
                        &mut curve_cache,
                    );
                    fields.curve_damage_base = damage_base;
                    resolved_damage_types = damage_types;
                    for warning in warnings {
                        report.warnings.push(
                            mapper
                                .interner
                                .intern(&format!("synth_weap_curve:{eid_str}:{warning}")),
                        );
                    }
                }
                source_fields = Some(fields);
            }

            let mut changed = apply_to_record_with_source_family(
                &mut record,
                dnam_default,
                source_fields,
                source_family,
            );
            changed |= apply_resolved_damage_types(&mut record, &resolved_damage_types);

            if changed {
                changed_records.push(record);
                report.records_changed += 1;
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "synthesize_weap_data_blocks replaced {replaced} of {expected} expected records"
            )));
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Classification helpers
// ---------------------------------------------------------------------------

/// True for creature root types (NPC_ or LVLN).
pub(crate) fn is_creature_root_sig(sig: SigCode) -> bool {
    matches!(sig.as_str(), "NPC_" | "LVLN")
}

/// Decide which DNAM default blob to use, based on creature-root classification
/// and EditorID heuristics (mirrors Python `_is_unarmed_creature_weapon` /
/// `_is_ranged_creature_weapon`).
pub(crate) fn choose_dnam_default(is_creature_root: bool, eid_lower: &str) -> DnamDefault {
    if !is_creature_root {
        return DnamDefault::Fo4;
    }
    // Python: unarmed when "unarmed" in editor_id.lower() or EquipmentType
    // == "013f42:fallout4.esm".  We can check the EditorID part here; the
    // EquipmentType check requires reading the record's fields and is deferred.
    if eid_lower.contains("unarmed") {
        return DnamDefault::CreatureUnarmed;
    }
    // FO76 Floaters use breath/fireball/stare creature weapons with ranged
    // projectile semantics. The animated-type check needs the translated fields;
    // fall back to FO4 generic when the EditorID gives no clear signal.
    if likely_ranged_creature_weapon_editor_id(eid_lower) {
        return DnamDefault::CreatureRanged;
    }
    // Default creature path: unarmed (most creature weapons in FO76 are melee).
    DnamDefault::CreatureUnarmed
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DnamDefault {
    Fo4,
    CreatureUnarmed,
    CreatureRanged,
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Inject or replace DNAM and FNAM subrecords in a WEAP record.
///
/// `dnam_default` chooses which default blob to write.
///
/// Returns `true` when the record was mutated.
///
/// Injection rules:
/// - DNAM absent → inject default.
/// - FO76 raw DNAM → relayout with the FO76 converter.
/// - FNV/FO3 DNAM → rebuild from the decoded source DATA/DNAM fields.
/// - Other target-shaped DNAM → preserve.
/// - FNAM absent → inject default.
/// - FNAM present (any variant) → leave as-is.
pub fn apply_to_record(record: &mut Record, dnam_default: DnamDefault) -> bool {
    apply_to_record_with_source_family(record, dnam_default, None, SourceWeapFamily::Fo76)
}

pub fn apply_to_record_with_source(
    record: &mut Record,
    dnam_default: DnamDefault,
    source_fields: Option<SourceWeapFields>,
) -> bool {
    apply_to_record_with_source_family(record, dnam_default, source_fields, SourceWeapFamily::Fo76)
}

pub(crate) fn apply_to_record_with_source_family(
    record: &mut Record,
    dnam_default: DnamDefault,
    source_fields: Option<SourceWeapFields>,
    source_family: SourceWeapFamily,
) -> bool {
    let dnam_sig = match SubrecordSig::from_str("DNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let fnam_sig = match SubrecordSig::from_str("FNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let raw_dnam_bytes = record.fields.iter().find_map(|e| {
        if e.sig == dnam_sig {
            if let FieldValue::Bytes(ref bytes) = e.value {
                return Some(bytes.to_vec());
            }
        }
        None
    });
    let dnam_is_raw_bytes = raw_dnam_bytes.is_some();
    let dnam_is_structured = record
        .fields
        .iter()
        .any(|e| e.sig == dnam_sig && !matches!(e.value, FieldValue::Bytes(_)));
    let has_fnam = record.fields.iter().any(|e| e.sig == fnam_sig);

    let rebuild_legacy_dnam = source_family == SourceWeapFamily::LegacyFallout
        && source_fields.is_some()
        && (dnam_is_raw_bytes || dnam_is_structured);
    let preserve_target_raw = source_family == SourceWeapFamily::Other
        && raw_dnam_bytes
            .as_ref()
            .is_some_and(|bytes| bytes.len() == 132);
    let need_dnam = rebuild_legacy_dnam || (!dnam_is_structured && !preserve_target_raw);
    let need_fnam = !has_fnam;

    let mut mutated = false;
    let mut synthesized_dnam_flags: Option<u32> = None;

    if need_dnam {
        // Remove existing raw DNAM (FO76 bytes) if present, then push defaults.
        if dnam_is_raw_bytes || rebuild_legacy_dnam {
            record.fields.retain(|e| e.sig != dnam_sig);
        }
        let mut blob = match source_family {
            SourceWeapFamily::LegacyFallout => {
                fo4_dnam_from_legacy_fields(dnam_default, source_fields)
            }
            SourceWeapFamily::Fo76 => match raw_dnam_bytes.as_deref() {
                Some(raw) => fo4_dnam_from_fo76_raw(raw, dnam_default, source_fields),
                None => dnam_default_bytes(dnam_default),
            },
            SourceWeapFamily::Other => dnam_default_bytes(dnam_default),
        };
        if let Some(damage_base) = source_fields.and_then(|fields| fields.curve_damage_base) {
            write_u16(&mut blob, 67, damage_base);
        }
        synthesized_dnam_flags = Some(u32::from_le_bytes([blob[48], blob[49], blob[50], blob[51]]));
        let mut sv: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        sv.extend_from_slice(&blob);
        record.fields.push(FieldEntry {
            sig: dnam_sig,
            value: FieldValue::Bytes(sv),
        });
        mutated = true;
    }

    if need_fnam {
        let mut fnam = default_fnam();
        if synthesized_dnam_flags.is_some_and(is_spin_up_automatic) {
            write_f32_41(
                &mut fnam,
                FO4_FNAM_ANIMATION_FIRE_SECONDS_OFFSET,
                SPIN_UP_ANIMATION_FIRE_SECONDS,
            );
        }
        if let Some(fields) = source_fields {
            apply_source_fnam_fields(&mut fnam, fields);
        }
        let mut sv: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        sv.extend_from_slice(&fnam);
        record.fields.push(FieldEntry {
            sig: fnam_sig,
            value: FieldValue::Bytes(sv),
        });
        mutated = true;
    } else if let Some(raw) = source_fields.and_then(|fields| fields.override_projectile_raw) {
        mutated |= patch_existing_fnam_override(record, fnam_sig, raw);
    }

    mutated
}

#[inline]
fn write_u32_41(buf: &mut [u8; 41], offset: usize, v: u32) {
    buf[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
}

#[inline]
fn write_u8_41(buf: &mut [u8; 41], offset: usize, v: u8) {
    buf[offset] = v;
}

fn apply_source_fnam_fields(fnam: &mut [u8; 41], fields: SourceWeapFields) {
    if let Some(value) = fields.rumble_left_motor_strength {
        write_f32_41(fnam, FO4_FNAM_RUMBLE_LEFT_MOTOR_STRENGTH_OFFSET, value);
    }
    if let Some(value) = fields.rumble_right_motor_strength {
        write_f32_41(fnam, FO4_FNAM_RUMBLE_RIGHT_MOTOR_STRENGTH_OFFSET, value);
    }
    if let Some(value) = fields.rumble_duration {
        write_f32_41(fnam, FO4_FNAM_RUMBLE_DURATION_OFFSET, value);
    }
    if let Some(value) = fields.animation_reload_seconds {
        write_f32_41(fnam, FO4_FNAM_ANIMATION_RELOAD_SECONDS_OFFSET, value);
    }
    if let Some(value) = fields.projectiles {
        write_u8_41(fnam, FO4_FNAM_PROJECTILES_OFFSET, value);
    }
    if let Some(raw) = fields.override_projectile_raw {
        write_u32_41(fnam, FO4_FNAM_OVERRIDE_PROJECTILE_OFFSET, raw);
    }
    if let Some(value) = fields.rumble_pattern {
        write_u32_41(fnam, FO4_FNAM_PATTERN_OFFSET, value);
    }
    if let Some(value) = fields.rumble_period_ms {
        write_u32_41(fnam, FO4_FNAM_RUMBLE_PERIOD_MS_OFFSET, value);
    }
}

fn patch_existing_fnam_override(record: &mut Record, fnam_sig: SubrecordSig, raw: u32) -> bool {
    for entry in record.fields.iter_mut() {
        if entry.sig != fnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &mut entry.value {
            if data.len() >= 33 {
                let current = u32::from_le_bytes([
                    data[FO4_FNAM_OVERRIDE_PROJECTILE_OFFSET],
                    data[FO4_FNAM_OVERRIDE_PROJECTILE_OFFSET + 1],
                    data[FO4_FNAM_OVERRIDE_PROJECTILE_OFFSET + 2],
                    data[FO4_FNAM_OVERRIDE_PROJECTILE_OFFSET + 3],
                ]);
                if current == 0 {
                    data[FO4_FNAM_OVERRIDE_PROJECTILE_OFFSET
                        ..FO4_FNAM_OVERRIDE_PROJECTILE_OFFSET + 4]
                        .copy_from_slice(&raw.to_le_bytes());
                    return true;
                }
            }
        }
        break;
    }
    false
}

fn extract_source_weap_fields(
    source_record: &Record,
    source_family: SourceWeapFamily,
    source_masters: &[String],
    source_plugin_name: &str,
    source_plugin_sym: Sym,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> (SourceWeapFields, Vec<String>) {
    let mut fields = SourceWeapFields::default();
    let mut warnings = Vec::new();
    let data_sig = SubrecordSig::from_str("DATA").ok();
    let dnam_sig = SubrecordSig::from_str("DNAM").ok();
    let ammo_sig = SubrecordSig::from_str("NAM0").ok();
    let sound_level_sig = SubrecordSig::from_str("VNAM").ok();
    let rgw3_sig = SubrecordSig::from_str("RGW3").ok();

    for entry in &source_record.fields {
        if source_family == SourceWeapFamily::LegacyFallout && Some(entry.sig) == data_sig {
            extract_legacy_data_fields(&mut fields, &entry.value, mapper.interner);
        } else if source_family == SourceWeapFamily::LegacyFallout && Some(entry.sig) == ammo_sig {
            apply_legacy_reference_resolution(
                &mut fields.ammo_raw,
                resolve_legacy_reference_from_value(
                    &entry.value,
                    "ammo",
                    source_masters,
                    source_plugin_name,
                    mapper,
                    target_masters,
                ),
                "ammo",
                &mut warnings,
                mapper.interner,
            );
        } else if source_family == SourceWeapFamily::LegacyFallout
            && Some(entry.sig) == sound_level_sig
        {
            fields.sound_level = scalar_u32(&entry.value);
        } else if Some(entry.sig) == dnam_sig {
            if let FieldValue::Bytes(data) = &entry.value {
                if source_family == SourceWeapFamily::LegacyFallout {
                    extract_raw_legacy_dnam_fields(&mut fields, data);
                    if let Some(raw) = read_u32(data, LEGACY_DNAM_PROJECTILE_OFFSET) {
                        apply_legacy_reference_resolution(
                            &mut fields.override_projectile_raw,
                            resolve_legacy_raw_reference(
                                raw,
                                source_masters,
                                source_plugin_name,
                                mapper,
                                target_masters,
                            ),
                            "projectile",
                            &mut warnings,
                            mapper.interner,
                        );
                    }
                } else {
                    extract_raw_dnam_fields(&mut fields, data);
                }
                if source_family == SourceWeapFamily::Fo76 && data.len() >= 4 {
                    let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    fields.ammo_raw = resolve_source_raw_to_target_raw(
                        raw,
                        source_masters,
                        source_plugin_name,
                        source_plugin_sym,
                        mapper,
                        target_masters,
                    );
                }
                if source_family == SourceWeapFamily::Fo76 {
                    for (index, source_offset) in FO76_DNAM_SOUND_OFFSETS.into_iter().enumerate() {
                        let Some(raw) = read_u32(data, source_offset) else {
                            continue;
                        };
                        fields.sound_data_raw[index] = if index == 0 {
                            Some(raw)
                        } else {
                            resolve_source_raw_to_target_raw(
                                raw,
                                source_masters,
                                source_plugin_name,
                                source_plugin_sym,
                                mapper,
                                target_masters,
                            )
                        };
                    }
                }
            } else {
                if source_family == SourceWeapFamily::LegacyFallout {
                    extract_structured_legacy_dnam_fields(
                        &mut fields,
                        &entry.value,
                        mapper.interner,
                    );
                    if let Some(source_fk) =
                        find_named_form_key(&entry.value, "projectile", mapper.interner)
                    {
                        apply_legacy_reference_resolution(
                            &mut fields.override_projectile_raw,
                            resolve_legacy_form_key_reference(source_fk, mapper, target_masters),
                            "projectile",
                            &mut warnings,
                            mapper.interner,
                        );
                    }
                } else {
                    extract_structured_dnam_fields(&mut fields, &entry.value, mapper.interner);
                }
            }
        } else if source_family == SourceWeapFamily::Fo76 && Some(entry.sig) == rgw3_sig {
            fields.override_projectile_raw = match &entry.value {
                FieldValue::Bytes(data) if data.len() >= 4 => {
                    let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    extract_raw_rgw3_fnam_fields(&mut fields, data);
                    resolve_source_raw_to_target_raw(
                        raw,
                        source_masters,
                        source_plugin_name,
                        source_plugin_sym,
                        mapper,
                        target_masters,
                    )
                }
                value => {
                    extract_structured_rgw3_fnam_fields(&mut fields, value, mapper.interner);
                    find_named_form_key(value, "override_projectile", mapper.interner).and_then(
                        |source_fk| {
                            resolve_source_fk_to_target_raw(
                                source_fk,
                                source_plugin_sym,
                                mapper,
                                target_masters,
                            )
                        },
                    )
                }
            };
        }
    }

    (fields, warnings)
}

#[allow(clippy::too_many_arguments)]
fn resolve_source_curve_damage(
    source_record: &Record,
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    source_extracted_dir: &Path,
    source_masters: &[String],
    source_plugin_name: &str,
    source_plugin_sym: Sym,
    mapper: &FormKeyMapper,
    target_masters: &[String],
    curve_cache: &mut CurveMeanCache,
) -> (Option<u16>, Vec<ResolvedDamageType>, Vec<String>) {
    let mut damage_base = None;
    let mut damage_types = Vec::new();
    let mut warnings = Vec::new();

    if let Some(entry) = source_record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "CVT0")
        && let Some(curve_fk) = source_form_key_from_value(
            &entry.value,
            "damage_curve",
            source_masters,
            source_plugin_name,
            mapper.interner,
        )
    {
        match cached_curve_mean(
            curve_fk,
            session,
            source_schema,
            source_extracted_dir,
            mapper.interner,
            curve_cache,
        ) {
            Ok(damage) => damage_base = Some(damage.min(u32::from(u16::MAX)) as u16),
            Err(error) => warnings.push(format!("primary:{error}")),
        }
    }

    if let Some(entry) = source_record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "DAMA")
    {
        let mut flattened_curve = false;
        for source_row in source_damage_type_rows(
            &entry.value,
            source_masters,
            source_plugin_name,
            mapper.interner,
        ) {
            let Some(target_type_raw) = resolve_source_fk_to_target_raw(
                source_row.source_type,
                source_plugin_sym,
                mapper,
                target_masters,
            ) else {
                continue;
            };
            let damage = if let Some(curve_fk) = source_row.curve {
                match cached_curve_mean(
                    curve_fk,
                    session,
                    source_schema,
                    source_extracted_dir,
                    mapper.interner,
                    curve_cache,
                ) {
                    Ok(damage) => {
                        flattened_curve = true;
                        damage
                    }
                    Err(error) => {
                        warnings.push(format!("typed:{error}"));
                        source_row.amount
                    }
                }
            } else {
                source_row.amount
            };
            damage_types.push(ResolvedDamageType {
                target_type_raw,
                damage,
            });
        }
        if !flattened_curve {
            damage_types.clear();
        }
    }

    (damage_base, damage_types, warnings)
}

fn source_damage_type_rows(
    value: &FieldValue,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<SourceDamageType> {
    let mut rows = Vec::new();
    match value {
        FieldValue::Bytes(data) => {
            for row in data.chunks_exact(FO76_DAMAGE_TYPE_ROW_LEN) {
                let Some(source_type) = read_u32(row, 0).and_then(|raw| {
                    source_raw_to_form_key(raw, source_masters, source_plugin_name, interner)
                }) else {
                    continue;
                };
                let amount = read_u32(row, 4).unwrap_or(0);
                let curve = read_u32(row, 8).and_then(|raw| {
                    source_raw_to_form_key(raw, source_masters, source_plugin_name, interner)
                });
                rows.push(SourceDamageType {
                    source_type,
                    amount,
                    curve,
                });
            }
        }
        FieldValue::List(values) => {
            for row in values {
                append_structured_damage_type_row(&mut rows, row, interner);
            }
        }
        FieldValue::Struct(_) => append_structured_damage_type_row(&mut rows, value, interner),
        _ => {}
    }
    rows
}

fn append_structured_damage_type_row(
    rows: &mut Vec<SourceDamageType>,
    value: &FieldValue,
    interner: &StringInterner,
) {
    let Some(source_type) = find_named_form_key(value, "damage_types_type", interner) else {
        return;
    };
    rows.push(SourceDamageType {
        source_type,
        amount: find_named_u32(value, "damage_types_amount", interner).unwrap_or(0),
        curve: find_named_form_key(value, "damage_types_curve_table", interner),
    });
}

fn source_form_key_from_value(
    value: &FieldValue,
    field_name: &str,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        FieldValue::Bytes(bytes) => source_raw_to_form_key(
            read_u32(bytes, 0)?,
            source_masters,
            source_plugin_name,
            interner,
        ),
        _ => find_named_form_key(value, field_name, interner),
    }
}

fn source_raw_to_form_key(
    raw: u32,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let load_index = (raw >> 24) as usize;
    let plugin = source_masters
        .get(load_index)
        .map(String::as_str)
        .unwrap_or(source_plugin_name);
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
    })
}

fn apply_resolved_damage_types(record: &mut Record, resolved: &[ResolvedDamageType]) -> bool {
    if resolved.is_empty() {
        return false;
    }
    let Ok(dama_sig) = SubrecordSig::from_str("DAMA") else {
        return false;
    };
    let mut bytes = smallvec::SmallVec::<[u8; 32]>::new();
    for row in resolved {
        bytes.extend_from_slice(&row.target_type_raw.to_le_bytes());
        bytes.extend_from_slice(&row.damage.to_le_bytes());
    }
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == dama_sig) {
        let changed = entry.value != FieldValue::Bytes(bytes.clone());
        entry.value = FieldValue::Bytes(bytes);
        changed
    } else {
        record.fields.push(FieldEntry {
            sig: dama_sig,
            value: FieldValue::Bytes(bytes),
        });
        true
    }
}

fn extract_raw_dnam_fields(fields: &mut SourceWeapFields, data: &[u8]) {
    fields.attack_delay_seconds = read_f32(data, FO76_DNAM_ATTACK_DELAY_OFFSET);
    fields.damage_secondary = read_f32(data, FO76_DNAM_DAMAGE_SECONDARY_OFFSET);
    fields.full_power_seconds = read_f32(data, FO76_DNAM_FULL_POWER_SECONDS_OFFSET);
    fields.min_power_per_shot = read_f32(data, FO76_DNAM_MIN_POWER_PER_SHOT_OFFSET);
    fields.stagger = read_u32(data, FO76_DNAM_STAGGER_OFFSET);
}

fn extract_legacy_data_fields(
    fields: &mut SourceWeapFields,
    value: &FieldValue,
    interner: &StringInterner,
) {
    match value {
        FieldValue::Bytes(data) if data.len() >= LEGACY_DATA_LEN => {
            let value = i32::from_le_bytes(data[0..4].try_into().unwrap());
            let weight = f32::from_le_bytes(data[8..12].try_into().unwrap());
            let damage = i16::from_le_bytes(data[12..14].try_into().unwrap());
            fields.value = Some(value.max(0) as u32);
            if weight.is_finite() && weight >= 0.0 {
                fields.weight = Some(weight);
            }
            fields.damage_base = Some(damage.max(0) as u16);
            fields.capacity = Some(u16::from(data[14]));
        }
        value => {
            fields.value = find_named_i64(value, "value", interner)
                .map(|value| value.clamp(0, i64::from(u32::MAX)) as u32);
            fields.weight = find_named_f32(value, "weight", interner)
                .filter(|weight| weight.is_finite() && *weight >= 0.0);
            fields.damage_base = find_named_i64(value, "base_damage", interner)
                .map(|damage| damage.clamp(0, i64::from(u16::MAX)) as u16);
            fields.capacity = find_named_u32(value, "clip_size", interner)
                .map(|capacity| capacity.min(u32::from(u16::MAX)) as u16);
        }
    }
}

fn extract_raw_legacy_dnam_fields(fields: &mut SourceWeapFields, data: &[u8]) {
    if data.len() < 36 {
        return;
    }
    fields.speed = sane_positive_f32(read_f32(data, 4));
    fields.reach = sane_positive_f32(read_f32(data, 8));
    fields.animation_type = read_u32(data, 0).map(legacy_animation_type);
    fields.min_range = sane_nonnegative_f32(read_f32(data, LEGACY_DNAM_MIN_RANGE_OFFSET));
    fields.max_range = sane_nonnegative_f32(read_f32(data, LEGACY_DNAM_MAX_RANGE_OFFSET));
    fields.on_hit = read_u32(data, LEGACY_DNAM_ON_HIT_OFFSET).filter(|value| *value <= 3);
    let flags_1 = data.get(12).copied().unwrap_or(0);
    let flags_2 = read_u32(data, LEGACY_DNAM_FLAGS_2_OFFSET).unwrap_or(0);
    fields.flags = Some(legacy_fo4_flags(flags_1, flags_2));
    fields.accuracy_bonus = data.get(40).copied();
    fields.projectiles = data.get(42).copied().filter(|value| *value != 0);
    fields.rumble_left_motor_strength =
        sane_nonnegative_f32(read_f32(data, LEGACY_DNAM_RUMBLE_LEFT_OFFSET));
    fields.rumble_right_motor_strength =
        sane_nonnegative_f32(read_f32(data, LEGACY_DNAM_RUMBLE_RIGHT_OFFSET));
    fields.rumble_duration =
        sane_nonnegative_f32(read_f32(data, LEGACY_DNAM_RUMBLE_DURATION_OFFSET));
    fields.animation_reload_seconds =
        sane_positive_f32(read_f32(data, LEGACY_DNAM_RELOAD_TIME_OFFSET));
    if flags_2 & (1 << 3) != 0 {
        fields.action_point_cost = sane_nonnegative_f32(read_f32(data, 68));
    }
    fields.rumble_pattern =
        read_u32(data, LEGACY_DNAM_RUMBLE_PATTERN_OFFSET).filter(|value| *value <= 3);
    fields.rumble_period_ms =
        read_f32(data, LEGACY_DNAM_RUMBLE_WAVELENGTH_OFFSET).and_then(rumble_seconds_to_period_ms);
}

fn extract_structured_legacy_dnam_fields(
    fields: &mut SourceWeapFields,
    value: &FieldValue,
    interner: &StringInterner,
) {
    fields.speed = sane_positive_f32(find_named_f32(value, "animation_multiplier", interner));
    fields.reach = sane_positive_f32(find_named_f32(value, "reach", interner));
    fields.animation_type =
        find_named_u32(value, "animation_type", interner).map(legacy_animation_type);
    fields.min_range = sane_nonnegative_f32(find_named_f32(value, "min_range", interner));
    fields.max_range = sane_nonnegative_f32(find_named_f32(value, "max_range", interner));
    fields.on_hit = find_named_u32(value, "on_hit", interner).filter(|value| *value <= 3);
    let flags_1 = find_named_u32(value, "flags_1", interner).unwrap_or(0) as u8;
    let flags_2 = find_named_u32(value, "flags_2", interner).unwrap_or(0);
    fields.flags = Some(legacy_fo4_flags(flags_1, flags_2));
    fields.accuracy_bonus = find_named_u8(value, "base_vats_to_hit_chance", interner);
    fields.projectiles =
        find_named_u8(value, "projectile_count", interner).filter(|value| *value != 0);
    fields.rumble_left_motor_strength = sane_nonnegative_f32(find_named_f32(
        value,
        "rumble_left_motor_strength",
        interner,
    ));
    fields.rumble_right_motor_strength = sane_nonnegative_f32(find_named_f32(
        value,
        "rumble_right_motor_strength",
        interner,
    ));
    fields.rumble_duration =
        sane_nonnegative_f32(find_named_f32(value, "rumble_duration", interner));
    fields.animation_reload_seconds =
        sane_positive_f32(find_named_f32(value, "reload_time", interner));
    if flags_2 & (1 << 3) != 0 {
        fields.action_point_cost =
            sane_nonnegative_f32(find_named_f32(value, "override_action_points", interner));
    }
    fields.rumble_pattern =
        find_named_u32(value, "rumble_pattern", interner).filter(|value| *value <= 3);
    fields.rumble_period_ms =
        find_named_f32(value, "rumble_wavelength", interner).and_then(rumble_seconds_to_period_ms);
}

fn legacy_animation_type(value: u32) -> u8 {
    match value {
        0 => 0,
        1 => 1,
        2 => 5,
        3..=9 => 9,
        10 | 13 => 10,
        11 | 12 => 11,
        _ => 9,
    }
}

fn legacy_fo4_flags(flags_1: u8, flags_2: u32) -> u32 {
    const FLAGS_1_MAP: [(u8, u32); 7] = [
        (0, 14),
        (1, 15),
        (2, 21),
        (3, 17),
        (4, 18),
        (5, 19),
        (7, 20),
    ];
    const FLAGS_2_MAP: [(u32, u32); 6] = [(0, 0), (1, 1), (2, 2), (4, 4), (5, 5), (6, 6)];
    let mut target = 0;
    for (source_bit, target_bit) in FLAGS_1_MAP {
        if flags_1 & (1 << source_bit) != 0 {
            target |= 1 << target_bit;
        }
    }
    for (source_bit, target_bit) in FLAGS_2_MAP {
        if flags_2 & (1 << source_bit) != 0 {
            target |= 1 << target_bit;
        }
    }
    target
}

fn sane_positive_f32(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value > 0.0)
}

fn sane_nonnegative_f32(value: Option<f32>) -> Option<f32> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn rumble_seconds_to_period_ms(value: f32) -> Option<u32> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    Some((value * 1_000.0).round().clamp(0.0, u32::MAX as f32) as u32)
}

fn extract_structured_dnam_fields(
    fields: &mut SourceWeapFields,
    value: &FieldValue,
    interner: &StringInterner,
) {
    fields.attack_delay_seconds = find_named_f32(value, "attack_delay_seconds", interner)
        .or_else(|| find_named_f32(value, "attack_delay", interner));
    fields.damage_secondary = find_named_f32(value, "secondary_damage", interner)
        .or_else(|| find_named_f32(value, "damage_secondary", interner));
    fields.full_power_seconds = find_named_f32(value, "full_power_seconds", interner);
    fields.min_power_per_shot = find_named_f32(value, "min_power_per_shot", interner);
    fields.stagger = find_named_u32(value, "stagger", interner);
}

fn extract_raw_rgw3_fnam_fields(fields: &mut SourceWeapFields, data: &[u8]) {
    fields.rumble_left_motor_strength = read_f32(data, FO76_RGW3_RUMBLE_LEFT_MOTOR_STRENGTH_OFFSET);
    fields.rumble_right_motor_strength =
        read_f32(data, FO76_RGW3_RUMBLE_RIGHT_MOTOR_STRENGTH_OFFSET);
    fields.rumble_duration = read_f32(data, FO76_RGW3_RUMBLE_DURATION_OFFSET);
    fields.animation_reload_seconds = read_f32(data, FO76_RGW3_ANIMATION_RELOAD_SECONDS_OFFSET);
    fields.projectiles = data.get(FO76_RGW3_PROJECTILES_OFFSET).copied();
}

fn extract_structured_rgw3_fnam_fields(
    fields: &mut SourceWeapFields,
    value: &FieldValue,
    interner: &StringInterner,
) {
    fields.rumble_left_motor_strength =
        find_named_f32(value, "rumble_left_motor_strength", interner);
    fields.rumble_right_motor_strength =
        find_named_f32(value, "rumble_right_motor_strength", interner);
    fields.rumble_duration = find_named_f32(value, "rumble_duration", interner);
    fields.animation_reload_seconds = find_named_f32(value, "animation_reload_seconds", interner);
    fields.projectiles = find_named_u8(value, "projectiles", interner);
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(f32::from_le_bytes(bytes.try_into().ok()?))
}

fn resolve_legacy_reference_from_value(
    value: &FieldValue,
    field_name: &str,
    source_masters: &[String],
    source_plugin_name: &str,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> LegacyReferenceResolution {
    match value {
        FieldValue::Bytes(bytes) => {
            read_u32(bytes, 0).map_or(LegacyReferenceResolution::Null, |raw| {
                resolve_legacy_raw_reference(
                    raw,
                    source_masters,
                    source_plugin_name,
                    mapper,
                    target_masters,
                )
            })
        }
        FieldValue::FormKey(source_fk) => {
            resolve_legacy_form_key_reference(*source_fk, mapper, target_masters)
        }
        _ => find_named_form_key(value, field_name, mapper.interner)
            .map_or(LegacyReferenceResolution::Null, |source_fk| {
                resolve_legacy_form_key_reference(source_fk, mapper, target_masters)
            }),
    }
}

fn resolve_legacy_raw_reference(
    raw: u32,
    source_masters: &[String],
    source_plugin_name: &str,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> LegacyReferenceResolution {
    let Some(source_fk) =
        source_raw_to_form_key(raw, source_masters, source_plugin_name, mapper.interner)
    else {
        return LegacyReferenceResolution::Null;
    };
    resolve_legacy_form_key_reference(source_fk, mapper, target_masters)
}

fn resolve_legacy_form_key_reference(
    source_fk: FormKey,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> LegacyReferenceResolution {
    resolve_mapped_source_fk_to_target_raw(source_fk, mapper, target_masters).map_or(
        LegacyReferenceResolution::Unmapped(source_fk),
        LegacyReferenceResolution::Mapped,
    )
}

fn apply_legacy_reference_resolution(
    target: &mut Option<u32>,
    resolution: LegacyReferenceResolution,
    kind: &str,
    warnings: &mut Vec<String>,
    interner: &StringInterner,
) {
    match resolution {
        LegacyReferenceResolution::Null => {}
        LegacyReferenceResolution::Mapped(raw) => *target = Some(raw),
        LegacyReferenceResolution::Unmapped(source_fk) => {
            warnings.push(format!("unmapped_{kind}:{}", source_fk.format(interner)))
        }
    }
}

fn resolve_source_raw_to_target_raw(
    raw: u32,
    source_masters: &[String],
    source_plugin_name: &str,
    source_plugin_sym: Sym,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> Option<u32> {
    if raw == 0 {
        return None;
    }
    let load_index = (raw >> 24) as usize;
    let plugin = source_masters
        .get(load_index)
        .map(String::as_str)
        .unwrap_or(source_plugin_name);
    let source_fk = FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: mapper.interner.intern(plugin),
    };
    resolve_source_fk_to_target_raw(source_fk, source_plugin_sym, mapper, target_masters)
}

fn resolve_source_fk_to_target_raw(
    source_fk: FormKey,
    source_plugin_sym: Sym,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> Option<u32> {
    if let Some(target_fk) = mapper.lookup(source_fk) {
        return encode_target_form_id(target_fk, mapper.interner, target_masters);
    }

    let first_target_master = target_masters.first()?;
    if source_fk.plugin == source_plugin_sym
        && first_target_master.eq_ignore_ascii_case("Fallout4.esm")
    {
        return Some(source_fk.local & 0x00FF_FFFF);
    }

    None
}

fn resolve_mapped_source_fk_to_target_raw(
    source_fk: FormKey,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> Option<u32> {
    let target_fk = mapper.lookup(source_fk)?;
    encode_target_form_id(target_fk, mapper.interner, target_masters)
}

fn encode_target_form_id(
    fk: FormKey,
    interner: &StringInterner,
    target_masters: &[String],
) -> Option<u32> {
    let plugin = interner.resolve(fk.plugin)?;
    let load_index = target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin))
        .unwrap_or(target_masters.len());
    if load_index > 0xFF {
        return None;
    }
    Some(((load_index as u32) << 24) | (fk.local & 0x00FF_FFFF))
}

fn find_named_form_key(
    value: &FieldValue,
    needle: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    let value = find_named_value(value, needle, interner)?;
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        _ => None,
    }
}

fn scalar_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Bytes(bytes) => read_u32(bytes, 0),
        FieldValue::Struct(fields) => fields.first().and_then(|(_, value)| scalar_u32(value)),
        _ => None,
    }
}

fn find_named_i64(value: &FieldValue, needle: &str, interner: &StringInterner) -> Option<i64> {
    let value = find_named_value(value, needle, interner)?;
    match value {
        FieldValue::Int(value) => Some(*value),
        FieldValue::Uint(value) => i64::try_from(*value).ok(),
        _ => None,
    }
}

fn find_named_f32(value: &FieldValue, needle: &str, interner: &StringInterner) -> Option<f32> {
    let value = find_named_value(value, needle, interner)?;
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(f32::from_le_bytes(bytes[0..4].try_into().ok()?))
        }
        _ => None,
    }
}

fn find_named_u8(value: &FieldValue, needle: &str, interner: &StringInterner) -> Option<u8> {
    let value = find_named_value(value, needle, interner)?;
    match value {
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Bytes(bytes) => bytes.first().copied(),
        _ => None,
    }
}

fn find_named_u32(value: &FieldValue, needle: &str, interner: &StringInterner) -> Option<u32> {
    let value = find_named_value(value, needle, interner)?;
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[0..4].try_into().ok()?))
        }
        _ => None,
    }
}

fn find_named_value<'a>(
    value: &'a FieldValue,
    needle: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    match value {
        FieldValue::Struct(fields) => {
            for (name, child) in fields {
                let is_match = interner
                    .resolve(*name)
                    .map(|s| s.eq_ignore_ascii_case(needle))
                    .unwrap_or(false);
                if is_match {
                    return Some(child);
                }
                if let Some(value) = find_named_value(child, needle, interner) {
                    return Some(value);
                }
            }
            None
        }
        FieldValue::List(values) => values
            .iter()
            .find_map(|child| find_named_value(child, needle, interner)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;
    use crate::translator::Game;

    const FLAG_AUTOMATIC: u32 = 32_768;
    const FLAG_HOLD_INPUT_TO_POWER: u32 = 2_048;

    fn make_weap(interner: &StringInterner) -> Record {
        let sig = SigCode::from_str("WEAP").unwrap();
        let fk = FormKey::parse("000800@Test.esm", interner).unwrap();
        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn has_sig(record: &Record, sig_str: &str) -> bool {
        let sig = SubrecordSig::from_str(sig_str).unwrap();
        record.fields.iter().any(|e| e.sig == sig)
    }

    fn field_bytes(record: &Record, sig_str: &str) -> Vec<u8> {
        let sig = SubrecordSig::from_str(sig_str).unwrap();
        for entry in &record.fields {
            if entry.sig == sig {
                if let FieldValue::Bytes(ref sv) = entry.value {
                    return sv.to_vec();
                }
            }
        }
        vec![]
    }

    fn push_raw_dnam(record: &mut Record, raw: &[u8]) {
        let dnam_sig = SubrecordSig::from_str("DNAM").unwrap();
        let mut sv: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        sv.extend_from_slice(raw);
        record.fields.push(FieldEntry {
            sig: dnam_sig,
            value: FieldValue::Bytes(sv),
        });
    }

    fn push_raw_field(record: &mut Record, sig: &str, raw: &[u8]) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(raw)),
        });
    }

    fn legacy_mapper_state() -> MapperState {
        MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Converted.esp".into(),
                ..MapperOptions::default()
            },
        )
    }

    fn legacy_golden_fields(dnam_len: usize) -> SourceWeapFields {
        let mut data = [0u8; LEGACY_DATA_LEN];
        data[0..4].copy_from_slice(&750i32.to_le_bytes());
        data[4..8].copy_from_slice(&80i32.to_le_bytes());
        data[8..12].copy_from_slice(&3.0f32.to_le_bytes());
        data[12..14].copy_from_slice(&22i16.to_le_bytes());
        data[14] = 12;

        let mut dnam = vec![0u8; dnam_len];
        dnam[0..4].copy_from_slice(&3u32.to_le_bytes());
        dnam[4..8].copy_from_slice(&1.0f32.to_le_bytes());
        dnam[8..12].copy_from_slice(&1.25f32.to_le_bytes());
        dnam[12] = (1 << 1) | (1 << 2) | (1 << 3);
        dnam[36..40].copy_from_slice(&0x02_CD5Fu32.to_le_bytes());
        dnam[40] = 15;
        dnam[42] = 2;
        dnam[44..48].copy_from_slice(&256.0f32.to_le_bytes());
        dnam[48..52].copy_from_slice(&768.0f32.to_le_bytes());
        dnam[52..56].copy_from_slice(&2u32.to_le_bytes());
        let flags_2: u32 = (1 << 0) | (1 << 1) | (1 << 3) | (1 << 5);
        dnam[56..60].copy_from_slice(&flags_2.to_le_bytes());
        dnam[60..64].copy_from_slice(&1.1f32.to_le_bytes());
        dnam[64..68].copy_from_slice(&1.0f32.to_le_bytes());
        dnam[68..72].copy_from_slice(&17.0f32.to_le_bytes());
        dnam[72..76].copy_from_slice(&0.5f32.to_le_bytes());
        dnam[76..80].copy_from_slice(&0.25f32.to_le_bytes());
        dnam[80..84].copy_from_slice(&0.15f32.to_le_bytes());
        dnam[92..96].copy_from_slice(&1.3f32.to_le_bytes());
        dnam[108..112].copy_from_slice(&1u32.to_le_bytes());
        dnam[112..116].copy_from_slice(&0.15f32.to_le_bytes());

        let mut fields = SourceWeapFields::default();
        extract_legacy_data_fields(
            &mut fields,
            &FieldValue::Bytes(smallvec::SmallVec::from_slice(&data)),
            &StringInterner::new(),
        );
        extract_raw_legacy_dnam_fields(&mut fields, &dnam);
        fields.ammo_raw = Some(0x01_004241);
        fields.override_projectile_raw = Some(0x02_02CD5F);
        fields.sound_level = Some(1);
        fields
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn weap_missing_both_gets_defaults() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);

        let changed = apply_to_record(&mut record, DnamDefault::Fo4);
        assert!(changed, "should mutate when DNAM and FNAM are absent");
        assert!(has_sig(&record, "DNAM"), "DNAM must be injected");
        assert!(has_sig(&record, "FNAM"), "FNAM must be injected");

        let dnam = field_bytes(&record, "DNAM");
        assert_eq!(dnam.len(), 132, "DNAM must be 132 bytes");

        let fnam = field_bytes(&record, "FNAM");
        assert_eq!(fnam.len(), 41, "FNAM must be 41 bytes");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn weap_with_structured_dnam_and_fnam_is_no_op() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);

        let dnam_sig = SubrecordSig::from_str("DNAM").unwrap();
        let fnam_sig = SubrecordSig::from_str("FNAM").unwrap();
        let sym = interner.intern("val");

        // Non-Bytes variant = already FO4-structured; must be preserved.
        record.fields.push(FieldEntry {
            sig: dnam_sig,
            value: FieldValue::String(sym),
        });
        record.fields.push(FieldEntry {
            sig: fnam_sig,
            value: FieldValue::String(sym),
        });

        let changed = apply_to_record(&mut record, DnamDefault::Fo4);
        assert!(
            !changed,
            "must not mutate when DNAM is already structured and FNAM is present"
        );
    }

    #[test]
    fn target_shaped_raw_weap_is_preserved_for_non_conversion_sources() {
        let interner = StringInterner::new();
        let mut record = make_weap(&interner);
        let dnam = [0xA5; 132];
        let fnam = [0x5A; 41];
        push_raw_dnam(&mut record, &dnam);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("FNAM").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&fnam)),
        });

        let changed = apply_to_record_with_source_family(
            &mut record,
            DnamDefault::Fo4,
            None,
            SourceWeapFamily::Other,
        );

        assert!(!changed);
        assert_eq!(field_bytes(&record, "DNAM"), dnam);
        assert_eq!(field_bytes(&record, "FNAM"), fnam);
    }

    #[test]
    fn fnv_and_fo3_legacy_weap_layouts_relayout_to_the_same_fo4_golden() {
        let fnv = legacy_golden_fields(204);
        let fo3 = legacy_golden_fields(LEGACY_DNAM_MIN_LEN);
        assert_eq!(
            fnv, fo3,
            "FNV's 68-byte DNAM tail must not affect FO4 fields"
        );

        let dnam = fo4_dnam_from_legacy_fields(DnamDefault::Fo4, Some(fnv));
        assert_eq!(
            u32::from_le_bytes(dnam[0..4].try_into().unwrap()),
            0x01_004241
        );
        assert_eq!(f32::from_le_bytes(dnam[4..8].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(dnam[12..16].try_into().unwrap()), 1.25);
        assert_eq!(f32::from_le_bytes(dnam[16..20].try_into().unwrap()), 256.0);
        assert_eq!(f32::from_le_bytes(dnam[20..24].try_into().unwrap()), 768.0);
        assert_eq!(u32::from_le_bytes(dnam[36..40].try_into().unwrap()), 2);
        let expected_flags = (1 << 0) | (1 << 1) | (1 << 5) | (1 << 15) | (1 << 17) | (1 << 21);
        assert_eq!(
            u32::from_le_bytes(dnam[48..52].try_into().unwrap()),
            expected_flags
        );
        assert_eq!(u16::from_le_bytes(dnam[52..54].try_into().unwrap()), 12);
        assert_eq!(dnam[54], 9);
        assert_eq!(f32::from_le_bytes(dnam[59..63].try_into().unwrap()), 3.0);
        assert_eq!(u32::from_le_bytes(dnam[63..67].try_into().unwrap()), 750);
        assert_eq!(u16::from_le_bytes(dnam[67..69].try_into().unwrap()), 22);
        assert_eq!(u32::from_le_bytes(dnam[69..73].try_into().unwrap()), 1);
        assert_eq!(dnam[105], 15);
        assert_eq!(f32::from_le_bytes(dnam[112..116].try_into().unwrap()), 17.0);
        for offset in [73, 77, 81, 85, 89, 93, 97, 101] {
            assert_eq!(
                u32::from_le_bytes(dnam[offset..offset + 4].try_into().unwrap()),
                0
            );
        }

        let mut fnam = default_fnam();
        apply_source_fnam_fields(&mut fnam, fnv);
        assert_eq!(f32::from_le_bytes(fnam[4..8].try_into().unwrap()), 0.5);
        assert_eq!(f32::from_le_bytes(fnam[8..12].try_into().unwrap()), 0.25);
        assert_eq!(f32::from_le_bytes(fnam[12..16].try_into().unwrap()), 0.15);
        assert_eq!(f32::from_le_bytes(fnam[16..20].try_into().unwrap()), 1.3);
        assert_eq!(fnam[28], 2);
        assert_eq!(
            u32::from_le_bytes(fnam[29..33].try_into().unwrap()),
            0x02_02CD5F
        );
        assert_eq!(u32::from_le_bytes(fnam[33..37].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(fnam[37..41].try_into().unwrap()), 150);
    }

    #[test]
    fn decoded_legacy_source_fields_use_semantic_names_not_fo76_offsets() {
        let interner = StringInterner::new();
        let field = |name, value| (interner.intern(name), value);
        let data = FieldValue::Struct(vec![
            field("value", FieldValue::Int(750)),
            field("weight", FieldValue::Float(3.0)),
            field("base_damage", FieldValue::Int(22)),
            field("clip_size", FieldValue::Uint(12)),
        ]);
        let dnam = FieldValue::Struct(vec![
            field("animation_type", FieldValue::Uint(3)),
            field("animation_multiplier", FieldValue::Float(1.0)),
            field("min_range", FieldValue::Float(256.0)),
            field("max_range", FieldValue::Float(768.0)),
            field("flags_1", FieldValue::Uint(1 << 1)),
            field("flags_2", FieldValue::Uint((1 << 3) | (1 << 5))),
            field("override_action_points", FieldValue::Float(17.0)),
            field("projectile_count", FieldValue::Uint(2)),
            field("rumble_left_motor_strength", FieldValue::Float(0.5)),
            field("reload_time", FieldValue::Float(1.3)),
        ]);
        let mut fields = SourceWeapFields::default();
        extract_legacy_data_fields(&mut fields, &data, &interner);
        extract_structured_legacy_dnam_fields(&mut fields, &dnam, &interner);

        assert_eq!(fields.value, Some(750));
        assert_eq!(fields.weight, Some(3.0));
        assert_eq!(fields.damage_base, Some(22));
        assert_eq!(fields.capacity, Some(12));
        assert_eq!(fields.animation_type, Some(9));
        assert_eq!(fields.min_range, Some(256.0));
        assert_eq!(fields.max_range, Some(768.0));
        assert_eq!(fields.flags, Some((1 << 5) | (1 << 15)));
        assert_eq!(fields.action_point_cost, Some(17.0));
        assert_eq!(fields.projectiles, Some(2));
        assert_eq!(fields.rumble_left_motor_strength, Some(0.5));
        assert_eq!(fields.animation_reload_seconds, Some(1.3));
    }

    #[test]
    fn source_game_dispatch_is_explicit() {
        assert_eq!(
            SourceWeapFamily::from_game(Some("fnv")),
            SourceWeapFamily::LegacyFallout
        );
        assert_eq!(
            SourceWeapFamily::from_game(Some("FO3")),
            SourceWeapFamily::LegacyFallout
        );
        assert_eq!(
            SourceWeapFamily::from_game(Some("fo76")),
            SourceWeapFamily::Fo76
        );
        assert_eq!(
            SourceWeapFamily::from_game(Some("fo4")),
            SourceWeapFamily::Other
        );
    }

    #[test]
    fn legacy_table_ammo_uses_authoritative_fo4_formkey() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let source_ammo = FormKey {
            local: 0x004241,
            plugin: source_plugin,
        };
        let source_entries = [(
            interner.intern("Ammo10mm"),
            source_ammo,
            SigCode::from_str("AMMO").unwrap(),
        )];
        let mut state = legacy_mapper_state();
        crate::run::seed_fnv_fo3_fo4_ammo_substitutions(
            &mut state,
            &source_entries,
            &interner,
            Game::Fnv,
            Game::Fo4,
        )
        .unwrap();
        let mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut source_weap = make_weap(&interner);
        push_raw_field(&mut source_weap, "NAM0", &0x004241u32.to_le_bytes());

        let (fields, warnings) = extract_source_weap_fields(
            &source_weap,
            SourceWeapFamily::LegacyFallout,
            &[],
            "FalloutNV.esm",
            source_plugin,
            &mapper,
            &["Fallout4.esm".into()],
        );

        assert_eq!(fields.ammo_raw, Some(0x01F276));
        assert!(warnings.is_empty());
    }

    #[test]
    fn legacy_non_table_ammo_uses_mapper_state() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let source_ammo = FormKey {
            local: 0x07EA27,
            plugin: source_plugin,
        };
        let mut state = legacy_mapper_state();
        state.source_to_target.insert(
            source_ammo,
            FormKey {
                local: 0x123456,
                plugin: interner.intern("Fallout4.esm"),
            },
        );
        let mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut source_weap = make_weap(&interner);
        push_raw_field(&mut source_weap, "NAM0", &0x07EA27u32.to_le_bytes());

        let (fields, warnings) = extract_source_weap_fields(
            &source_weap,
            SourceWeapFamily::LegacyFallout,
            &[],
            "FalloutNV.esm",
            source_plugin,
            &mapper,
            &["Fallout4.esm".into()],
        );

        assert_eq!(fields.ammo_raw, Some(0x123456));
        assert!(warnings.is_empty());
    }

    #[test]
    fn legacy_projectile_uses_mapper_state_without_substitution() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let source_projectile = FormKey {
            local: 0x02CD5F,
            plugin: source_plugin,
        };
        let mut state = legacy_mapper_state();
        state.source_to_target.insert(
            source_projectile,
            FormKey {
                local: 0x654321,
                plugin: interner.intern("Fallout4.esm"),
            },
        );
        let mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut source_weap = make_weap(&interner);
        let mut dnam = vec![0u8; LEGACY_DNAM_MIN_LEN];
        dnam[LEGACY_DNAM_PROJECTILE_OFFSET..LEGACY_DNAM_PROJECTILE_OFFSET + 4]
            .copy_from_slice(&0x02CD5Fu32.to_le_bytes());
        push_raw_dnam(&mut source_weap, &dnam);

        let (fields, warnings) = extract_source_weap_fields(
            &source_weap,
            SourceWeapFamily::LegacyFallout,
            &[],
            "FalloutNV.esm",
            source_plugin,
            &mapper,
            &["Fallout4.esm".into()],
        );

        assert_eq!(fields.override_projectile_raw, Some(0x654321));
        assert!(warnings.is_empty());
    }

    #[test]
    fn legacy_raw_zero_references_stay_null_without_warning() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let mut state = legacy_mapper_state();
        let mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut source_weap = make_weap(&interner);
        push_raw_field(&mut source_weap, "NAM0", &0u32.to_le_bytes());
        push_raw_dnam(&mut source_weap, &[0u8; LEGACY_DNAM_MIN_LEN]);

        let (fields, warnings) = extract_source_weap_fields(
            &source_weap,
            SourceWeapFamily::LegacyFallout,
            &[],
            "FalloutNV.esm",
            source_plugin,
            &mapper,
            &["Fallout4.esm".into()],
        );

        assert_eq!(fields.ammo_raw, None);
        assert_eq!(fields.override_projectile_raw, None);
        assert!(warnings.is_empty());
    }

    #[test]
    fn legacy_unmapped_nonzero_references_warn_before_null() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let mut state = legacy_mapper_state();
        let mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut source_weap = make_weap(&interner);
        push_raw_field(&mut source_weap, "NAM0", &0x004241u32.to_le_bytes());
        let mut dnam = vec![0u8; LEGACY_DNAM_MIN_LEN];
        dnam[LEGACY_DNAM_PROJECTILE_OFFSET..LEGACY_DNAM_PROJECTILE_OFFSET + 4]
            .copy_from_slice(&0x02CD5Fu32.to_le_bytes());
        push_raw_dnam(&mut source_weap, &dnam);

        let (fields, warnings) = extract_source_weap_fields(
            &source_weap,
            SourceWeapFamily::LegacyFallout,
            &[],
            "FalloutNV.esm",
            source_plugin,
            &mapper,
            &["Fallout4.esm".into()],
        );

        assert_eq!(fields.ammo_raw, None);
        assert_eq!(fields.override_projectile_raw, None);
        assert_eq!(
            warnings,
            [
                "unmapped_ammo:004241@FalloutNV.esm",
                "unmapped_projectile:02CD5F@FalloutNV.esm",
            ]
        );
    }

    #[test]
    fn legacy_source_rebuilds_target_decoded_dnam_instead_of_accepting_garbage() {
        let interner = StringInterner::new();
        let mut record = make_weap(&interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Struct(Vec::new()),
        });

        let changed = apply_to_record_with_source_family(
            &mut record,
            DnamDefault::Fo4,
            Some(legacy_golden_fields(204)),
            SourceWeapFamily::LegacyFallout,
        );

        assert!(changed);
        assert_eq!(field_bytes(&record, "DNAM").len(), 132);
        assert_eq!(
            u32::from_le_bytes(field_bytes(&record, "DNAM")[63..67].try_into().unwrap()),
            750
        );
        assert_eq!(field_bytes(&record, "FNAM").len(), 41);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn weap_with_raw_dnam_bytes_gets_replaced() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);

        // Simulate FO76 DNAM: wrong-layout raw bytes.
        push_raw_dnam(&mut record, &[0xFFu8; 80]);

        let changed = apply_to_record(&mut record, DnamDefault::Fo4);
        assert!(changed, "raw DNAM bytes must be replaced");

        // Exactly one DNAM of correct size after replacement.
        let dnam_sig = SubrecordSig::from_str("DNAM").unwrap();
        let dnam_entries: Vec<_> = record.fields.iter().filter(|e| e.sig == dnam_sig).collect();
        assert_eq!(dnam_entries.len(), 1, "exactly one DNAM after replacement");
        if let FieldValue::Bytes(ref sv) = dnam_entries[0].value {
            assert_eq!(sv.len(), 132, "replaced DNAM must be 132 bytes");
        } else {
            panic!("replaced DNAM must be FieldValue::Bytes");
        }

        assert!(has_sig(&record, "FNAM"), "FNAM must be injected");
    }

    fn synthesized_fire_seconds(flags: u32) -> f32 {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let mut raw = vec![0u8; 160];
        raw[60..64].copy_from_slice(&flags.to_le_bytes());
        push_raw_dnam(&mut record, &raw);

        apply_to_record(&mut record, DnamDefault::Fo4);

        let fnam = field_bytes(&record, "FNAM");
        f32::from_le_bytes([fnam[0], fnam[1], fnam[2], fnam[3]])
    }

    #[test]
    fn spin_up_automatic_gets_minigun_fire_seconds() {
        assert_eq!(
            synthesized_fire_seconds(FLAG_CHARGING_ATTACK | FLAG_AUTOMATIC),
            1.0
        );
    }

    #[test]
    fn non_spin_up_flags_keep_instant_fire_default() {
        assert_eq!(synthesized_fire_seconds(FLAG_AUTOMATIC), 1e-5_f32);
        assert_eq!(synthesized_fire_seconds(FLAG_HOLD_INPUT_TO_POWER), 1e-5_f32);
    }

    #[test]
    fn floater_projectile_weapons_use_ranged_creature_defaults() {
        assert_eq!(
            choose_dnam_default(true, "crfloaterflamerbreath"),
            DnamDefault::CreatureRanged
        );
        assert_eq!(
            choose_dnam_default(true, "crfloaterflamerfireball"),
            DnamDefault::CreatureRanged
        );
        assert_eq!(
            choose_dnam_default(true, "crfloaterfreezerstare"),
            DnamDefault::CreatureRanged
        );
    }

    #[test]
    fn raw_fo76_dnam_preserves_animation_type_and_attack_seconds() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let mut raw = vec![0u8; 170];
        raw[0..4].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        raw[60..64].copy_from_slice(&(FLAG_CRIT_EFFECT_ON_DEATH | FLAG_AUTOMATIC).to_le_bytes());
        raw[68] = 5;
        raw[120..124].copy_from_slice(&1.8f32.to_le_bytes());
        push_raw_dnam(&mut record, &raw);

        let changed = apply_to_record(&mut record, DnamDefault::Fo4);
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        assert_eq!(
            u32::from_le_bytes(dnam[48..52].try_into().unwrap()),
            FLAG_CRIT_EFFECT_ON_DEATH | FLAG_AUTOMATIC,
            "standard weapon flags should come from FO76 DNAM"
        );
        assert_eq!(dnam[54], 5, "animation_type should come from FO76 DNAM");
        let attack_seconds = f32::from_le_bytes(dnam[106..110].try_into().unwrap());
        assert!(
            (attack_seconds - 1.8).abs() < 1e-6,
            "animation_attack_seconds={attack_seconds}"
        );
        assert_eq!(
            u32::from_le_bytes(dnam[0..4].try_into().unwrap()),
            0,
            "raw source formids should not be copied into synthesized DNAM"
        );
    }

    #[test]
    fn raw_fo76_dnam_preserves_charge_power_fields() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let mut raw = vec![0u8; 170];
        raw[36..40].copy_from_slice(&0.15f32.to_le_bytes());
        raw[60..64]
            .copy_from_slice(&(FLAG_CRIT_EFFECT_ON_DEATH | FLAG_HOLD_INPUT_TO_POWER).to_le_bytes());
        raw[130..134].copy_from_slice(&1.0f32.to_le_bytes());
        raw[134..138].copy_from_slice(&2.0f32.to_le_bytes());
        push_raw_dnam(&mut record, &raw);

        let changed = apply_to_record(&mut record, DnamDefault::Fo4);
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        let attack_delay = f32::from_le_bytes(dnam[24..28].try_into().unwrap());
        assert!(
            (attack_delay - 0.15).abs() < 1e-6,
            "attack_delay={attack_delay}"
        );
        let full_power_seconds = f32::from_le_bytes(dnam[116..120].try_into().unwrap());
        assert!(
            (full_power_seconds - 1.0).abs() < 1e-6,
            "full_power_seconds={full_power_seconds}"
        );
        let min_power_per_shot = f32::from_le_bytes(dnam[120..124].try_into().unwrap());
        assert!(
            (min_power_per_shot - 0.2).abs() < 1e-6,
            "min_power_per_shot={min_power_per_shot}"
        );
    }

    #[test]
    fn raw_fo76_dnam_preserves_secondary_damage_and_stagger() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let mut raw = vec![0u8; 170];
        raw[69..73].copy_from_slice(&17.0f32.to_le_bytes());
        raw[138..142].copy_from_slice(&1u32.to_le_bytes());
        push_raw_dnam(&mut record, &raw);

        let changed = apply_to_record(&mut record, DnamDefault::Fo4);
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        let damage_secondary = f32::from_le_bytes(dnam[55..59].try_into().unwrap());
        assert!(
            (damage_secondary - 17.0).abs() < 1e-6,
            "damage_secondary={damage_secondary}"
        );
        assert_eq!(
            u32::from_le_bytes(dnam[124..128].try_into().unwrap()),
            1,
            "stagger should be Small"
        );
    }

    #[test]
    fn standard_weapon_preserves_fo76_base_ranges_for_omod_adjustments() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let mut raw = vec![0u8; 170];
        raw[24..28].copy_from_slice(&(-128.0f32).to_le_bytes());
        raw[28..32].copy_from_slice(&8192.0f32.to_le_bytes());
        push_raw_dnam(&mut record, &raw);

        let changed = apply_to_record(&mut record, DnamDefault::Fo4);
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        let min_range = f32::from_le_bytes(dnam[16..20].try_into().unwrap());
        let max_range = f32::from_le_bytes(dnam[20..24].try_into().unwrap());
        assert_eq!(min_range, -128.0);
        assert_eq!(max_range, 8192.0);
    }

    #[test]
    fn curve_damage_overrides_zero_fo76_damage_base() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let raw = vec![0u8; 170];
        push_raw_dnam(&mut record, &raw);

        let changed = apply_to_record_with_source(
            &mut record,
            DnamDefault::Fo4,
            Some(SourceWeapFields {
                curve_damage_base: Some(16),
                ..SourceWeapFields::default()
            }),
        );
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        assert_eq!(u16::from_le_bytes(dnam[67..69].try_into().unwrap()), 16);
    }

    #[test]
    fn curve_damage_mean_ignores_points_above_level_fifty() {
        let chainsaw = r#"{"curve":[{"x":1,"y":8},{"x":5,"y":9},{"x":10,"y":11},{"x":15,"y":12},{"x":20,"y":14},{"x":25,"y":15},{"x":30,"y":17},{"x":35,"y":19},{"x":40,"y":22},{"x":45,"y":25},{"x":50,"y":28}]}"#;
        let cattleprod = r#"{"curve":[{"x":1,"y":17},{"x":12,"y":20},{"x":23,"y":23},{"x":34,"y":26},{"x":45,"y":30},{"x":56,"y":34},{"x":540,"y":11820}]}"#;

        assert_eq!(
            crate::fixups::curve_table::mean_curve_value(chainsaw).unwrap(),
            16
        );
        assert_eq!(
            crate::fixups::curve_table::mean_curve_value(cattleprod).unwrap(),
            23
        );
    }

    #[test]
    fn resolved_curve_damage_restores_missing_damage_type_row() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let target_type_raw = 0x07_060A81;

        let changed = apply_resolved_damage_types(
            &mut record,
            &[ResolvedDamageType {
                target_type_raw,
                damage: 23,
            }],
        );
        assert!(changed);

        let dama = field_bytes(&record, "DAMA");
        assert_eq!(
            u32::from_le_bytes(dama[0..4].try_into().unwrap()),
            target_type_raw
        );
        assert_eq!(u32::from_le_bytes(dama[4..8].try_into().unwrap()), 23);
    }

    #[test]
    fn resolved_source_formids_patch_dnam_and_fnam() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let mut raw = vec![0u8; 170];
        raw[64..66].copy_from_slice(&18u16.to_le_bytes());
        raw[68] = 9;
        raw[81..83].copy_from_slice(&48u16.to_le_bytes());
        raw[120..124].copy_from_slice(&1.2f32.to_le_bytes());
        push_raw_dnam(&mut record, &raw);

        let changed = apply_to_record_with_source(
            &mut record,
            DnamDefault::Fo4,
            Some(SourceWeapFields {
                ammo_raw: Some(0x00_18ABDF),
                override_projectile_raw: Some(0x07_5916AB),
                ..SourceWeapFields::default()
            }),
        );
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        assert_eq!(
            u32::from_le_bytes(dnam[0..4].try_into().unwrap()),
            0x00_18ABDF
        );
        assert_eq!(u16::from_le_bytes(dnam[52..54].try_into().unwrap()), 18);
        assert_eq!(u16::from_le_bytes(dnam[67..69].try_into().unwrap()), 48);

        let fnam = field_bytes(&record, "FNAM");
        assert_eq!(
            u32::from_le_bytes(fnam[29..33].try_into().unwrap()),
            0x07_5916AB
        );
    }

    #[test]
    fn source_rgw3_fields_patch_synthesized_fnam() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let raw = vec![0u8; 170];
        push_raw_dnam(&mut record, &raw);

        let changed = apply_to_record_with_source(
            &mut record,
            DnamDefault::Fo4,
            Some(SourceWeapFields {
                override_projectile_raw: Some(0x07_5916AB),
                rumble_left_motor_strength: Some(0.8),
                rumble_right_motor_strength: Some(0.9),
                rumble_duration: Some(0.4),
                animation_reload_seconds: Some(4.1),
                projectiles: Some(2),
                ..SourceWeapFields::default()
            }),
        );
        assert!(changed);

        let fnam = field_bytes(&record, "FNAM");
        let rumble_left = f32::from_le_bytes(fnam[4..8].try_into().unwrap());
        assert!(
            (rumble_left - 0.8).abs() < 1e-6,
            "rumble_left={rumble_left}"
        );
        let rumble_right = f32::from_le_bytes(fnam[8..12].try_into().unwrap());
        assert!(
            (rumble_right - 0.9).abs() < 1e-6,
            "rumble_right={rumble_right}"
        );
        let rumble_duration = f32::from_le_bytes(fnam[12..16].try_into().unwrap());
        assert!(
            (rumble_duration - 0.4).abs() < 1e-6,
            "rumble_duration={rumble_duration}"
        );
        let reload_seconds = f32::from_le_bytes(fnam[16..20].try_into().unwrap());
        assert!(
            (reload_seconds - 4.1).abs() < 1e-6,
            "reload_seconds={reload_seconds}"
        );
        assert_eq!(fnam[28], 2);
        assert_eq!(
            u32::from_le_bytes(fnam[29..33].try_into().unwrap()),
            0x07_5916AB
        );
    }

    #[test]
    fn source_dnam_fields_patch_target_shaped_raw_dnam() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        push_raw_dnam(&mut record, &fo4_default_dnam());

        let changed = apply_to_record_with_source(
            &mut record,
            DnamDefault::Fo4,
            Some(SourceWeapFields {
                attack_delay_seconds: Some(0.15),
                damage_secondary: Some(17.0),
                full_power_seconds: Some(1.0),
                min_power_per_shot: Some(2.0),
                stagger: Some(1),
                ..SourceWeapFields::default()
            }),
        );
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        let attack_delay = f32::from_le_bytes(dnam[24..28].try_into().unwrap());
        assert!((attack_delay - 0.15).abs() < 1e-6);
        let damage_secondary = f32::from_le_bytes(dnam[55..59].try_into().unwrap());
        assert!((damage_secondary - 17.0).abs() < 1e-6);
        let full_power_seconds = f32::from_le_bytes(dnam[116..120].try_into().unwrap());
        assert!((full_power_seconds - 1.0).abs() < 1e-6);
        let min_power_per_shot = f32::from_le_bytes(dnam[120..124].try_into().unwrap());
        assert!((min_power_per_shot - 0.2).abs() < 1e-6);
        assert_eq!(u32::from_le_bytes(dnam[124..128].try_into().unwrap()), 1);
    }

    #[test]
    fn raw_source_dnam_fields_are_extracted() {
        let mut fields = SourceWeapFields::default();
        let mut raw = vec![0u8; 170];
        raw[36..40].copy_from_slice(&0.15f32.to_le_bytes());
        raw[69..73].copy_from_slice(&17.0f32.to_le_bytes());
        raw[130..134].copy_from_slice(&1.0f32.to_le_bytes());
        raw[134..138].copy_from_slice(&2.0f32.to_le_bytes());
        raw[138..142].copy_from_slice(&1u32.to_le_bytes());

        extract_raw_dnam_fields(&mut fields, &raw);

        assert_eq!(fields.attack_delay_seconds, Some(0.15));
        assert_eq!(fields.damage_secondary, Some(17.0));
        assert_eq!(fields.full_power_seconds, Some(1.0));
        assert_eq!(fields.min_power_per_shot, Some(2.0));
        assert_eq!(fields.stagger, Some(1));
    }

    #[test]
    fn structured_rgw3_fields_are_extracted() {
        let interner = StringInterner::new();
        let mut fields = SourceWeapFields::default();
        let rgw3 = FieldValue::Struct(vec![
            (
                interner.intern("rumble_left_motor_strength"),
                FieldValue::Float(0.8),
            ),
            (
                interner.intern("rumble_right_motor_strength"),
                FieldValue::Float(0.9),
            ),
            (interner.intern("rumble_duration"), FieldValue::Float(0.4)),
            (
                interner.intern("animation_reload_seconds"),
                FieldValue::Float(4.1),
            ),
            (interner.intern("projectiles"), FieldValue::Uint(2)),
        ]);

        extract_structured_rgw3_fnam_fields(&mut fields, &rgw3, &interner);

        assert_eq!(fields.rumble_left_motor_strength, Some(0.8));
        assert_eq!(fields.rumble_right_motor_strength, Some(0.9));
        assert_eq!(fields.rumble_duration, Some(0.4));
        assert_eq!(fields.animation_reload_seconds, Some(4.1));
        assert_eq!(fields.projectiles, Some(2));
    }

    #[test]
    fn resolved_source_sound_data_patches_dnam() {
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let mut raw = vec![0u8; 170];
        raw[64..66].copy_from_slice(&250u16.to_le_bytes());
        raw[68] = 9;
        push_raw_dnam(&mut record, &raw);

        let mut sound_data_raw = [None; WEAP_SOUND_FIELD_COUNT];
        sound_data_raw[0] = Some(3);
        sound_data_raw[1] = Some(0x00_222222);
        sound_data_raw[2] = Some(0x00_333333);
        sound_data_raw[3] = Some(0x00_444444);
        sound_data_raw[4] = Some(0x00_01DACD);
        sound_data_raw[5] = Some(0x00_555555);
        sound_data_raw[6] = Some(0x00_042B3B);
        sound_data_raw[7] = Some(0x00_042B3C);
        sound_data_raw[8] = Some(0x00_666666);

        let changed = apply_to_record_with_source(
            &mut record,
            DnamDefault::Fo4,
            Some(SourceWeapFields {
                sound_data_raw,
                ..SourceWeapFields::default()
            }),
        );
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        for (index, offset) in FO4_DNAM_SOUND_OFFSETS.into_iter().enumerate() {
            assert_eq!(
                u32::from_le_bytes(dnam[offset..offset + 4].try_into().unwrap()),
                sound_data_raw[index].unwrap(),
                "sound field {index} should be written at FO4 DNAM offset {offset}"
            );
        }
    }

    #[test]
    fn creature_unarmed_raw_dnam_translates_fo76_melee_fields() {
        // Field values mirror the real FO76 crGrognakAxe DNAM (melee creature
        // weapon): weapon_type 5, reach 0.8, min/max range 0/10.
        let mut interner = StringInterner::new();
        let mut record = make_weap(&mut interner);
        let mut raw = vec![0u8; 170];
        raw[4..8].copy_from_slice(&1.0f32.to_le_bytes()); // speed
        raw[16..20].copy_from_slice(&0.8f32.to_le_bytes()); // reach
        raw[24..28].copy_from_slice(&0.0f32.to_le_bytes()); // min_range
        raw[28..32].copy_from_slice(&10.0f32.to_le_bytes()); // max_range
        raw[60..64].copy_from_slice(
            &(FLAG_CRIT_EFFECT_ON_DEATH | FLAG_CANT_DROP | FLAG_NOT_PLAYABLE).to_le_bytes(),
        );
        raw[68] = 5; // weapon_type: two-hand melee
        raw[69..73].copy_from_slice(&15.0f32.to_le_bytes()); // secondary_damage
        raw[77..81].copy_from_slice(&606_983u32.to_le_bytes()); // value
        raw[81..83].copy_from_slice(&48u16.to_le_bytes()); // base_damage
        raw[120..124].copy_from_slice(&9.5f32.to_le_bytes()); // animation_attack_seconds
        push_raw_dnam(&mut record, &raw);

        let changed = apply_to_record(&mut record, DnamDefault::CreatureUnarmed);
        assert!(changed);

        let dnam = field_bytes(&record, "DNAM");
        let f = |o: usize| f32::from_le_bytes(dnam[o..o + 4].try_into().unwrap());
        assert!((f(4) - 1.0).abs() < 1e-6, "speed translated");
        assert!((f(12) - 0.8).abs() < 1e-6, "reach translated");
        assert_eq!(f(16), 0.0, "min_range from source, not the 500 default");
        assert_eq!(f(20), 10.0, "max_range from source, not the 2000 default");
        assert_eq!(dnam[54], 5, "animation_type from source weapon_type");
        assert!((f(55) - 15.0).abs() < 1e-6, "damage_secondary translated");
        assert_eq!(
            u32::from_le_bytes(dnam[63..67].try_into().unwrap()),
            606_983,
            "value translated"
        );
        assert_eq!(
            u16::from_le_bytes(dnam[67..69].try_into().unwrap()),
            48,
            "base_damage translated"
        );
        assert!((f(106) - 9.5).abs() < 1e-6, "animation_attack_seconds");
        assert_eq!(
            u32::from_le_bytes(dnam[48..52].try_into().unwrap()),
            FLAG_CRIT_EFFECT_ON_DEATH | FLAG_CANT_DROP | FLAG_NOT_PLAYABLE
        );
        assert_eq!(f(112), 20.0, "action_point_cost keeps the creature default");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn non_weap_record_does_not_panic() {
        let mut interner = StringInterner::new();
        let sig = SigCode::from_str("ARMO").unwrap();
        let fk = FormKey::parse("000801@Test.esm", &mut interner).unwrap();
        let mut record = Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        };
        // The record-level function is sig-agnostic; Fixup::run filters by sig.
        // Just verify no panic occurs.
        let _ = apply_to_record(&mut record, DnamDefault::Fo4);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn fo4_default_dnam_values_are_correct() {
        let dnam = fo4_default_dnam();
        assert_eq!(dnam.len(), 132);

        let speed = f32::from_le_bytes(dnam[4..8].try_into().unwrap());
        assert!((speed - 1.0).abs() < 1e-6, "speed must be 1.0, got {speed}");

        let min_range = f32::from_le_bytes(dnam[16..20].try_into().unwrap());
        assert!((min_range - 256.0).abs() < 1e-3, "min_range={min_range}");

        let max_range = f32::from_le_bytes(dnam[20..24].try_into().unwrap());
        assert!((max_range - 3072.0).abs() < 1e-3, "max_range={max_range}");

        let dormt = f32::from_le_bytes(dnam[32..36].try_into().unwrap());
        assert!((dormt - 0.5).abs() < 1e-6, "damage_outofrange_mult={dormt}");

        let cap = u16::from_le_bytes(dnam[52..54].try_into().unwrap());
        assert_eq!(cap, 1, "capacity must be 1");

        assert_eq!(dnam[54], 9, "animation_type must be 9");

        let apc = f32::from_le_bytes(dnam[112..116].try_into().unwrap());
        assert!((apc - 30.0).abs() < 1e-4, "action_point_cost={apc}");

        let stagger = u32::from_le_bytes(dnam[124..128].try_into().unwrap());
        assert_eq!(stagger, 0, "stagger must be 0 (None)");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn default_fnam_values_are_correct() {
        let fnam = default_fnam();
        assert_eq!(fnam.len(), 41);

        let afs = f32::from_le_bytes(fnam[0..4].try_into().unwrap());
        assert!(
            (afs - 1e-5_f32).abs() < 1e-10,
            "animation_fire_seconds={afs}"
        );

        let rlms = f32::from_le_bytes(fnam[4..8].try_into().unwrap());
        assert!(
            (rlms - 0.5).abs() < 1e-6,
            "rumble_left_motor_strength={rlms}"
        );

        let ars = f32::from_le_bytes(fnam[16..20].try_into().unwrap());
        assert!((ars - 2.0).abs() < 1e-6, "animation_reload_seconds={ars}");

        let sts = f32::from_le_bytes(fnam[24..28].try_into().unwrap());
        assert!(
            (sts - 0.15).abs() < 1e-5,
            "sighted_transition_seconds={sts}"
        );

        assert_eq!(fnam[28], 1, "projectiles must be 1");
    }
}
