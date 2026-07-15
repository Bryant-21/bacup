//! Fixup: synthesize FO4 DNAM (weapon stats) and FNAM (animation) subrecords for
//! translated WEAP records that lack them.
//!

//!
//! # What this does
//! FO76 WEAP records store weapon stats in a DNAM binary blob whose layout
//! differs from FO4's.  After FO76→FO4 translation the DNAM is either absent
//! or contains FO76 bytes that don't decode under the FO4 codec.  This fixup
//! replaces the DNAM with a DNAM containing sensible FO4 default values,
//! preserving FO76 animation fields that map cleanly to FO4, and injects an
//! FNAM with FO4 animation defaults when FNAM is missing.
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
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};
use rustc_hash::FxHashMap;

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
    pub attack_delay_seconds: Option<f32>,
    pub damage_secondary: Option<f32>,
    pub full_power_seconds: Option<f32>,
    pub min_power_per_shot: Option<f32>,
    pub stagger: Option<u32>,
    pub override_projectile_raw: Option<u32>,
    pub rumble_left_motor_strength: Option<f32>,
    pub rumble_right_motor_strength: Option<f32>,
    pub rumble_duration: Option<f32>,
    pub animation_reload_seconds: Option<f32>,
    pub projectiles: Option<u8>,
    pub sound_data_raw: [Option<u32>; WEAP_SOUND_FIELD_COUNT],
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
    }
    if matches!(default, DnamDefault::Fo4 | DnamDefault::CreatureUnarmed) {
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

fn apply_source_dnam_fields(dnam: &mut [u8; 132], fields: SourceWeapFields) {
    if let Some(value) = fields.attack_delay_seconds {
        write_f32(dnam, FO4_DNAM_ATTACK_DELAY_OFFSET, value);
    }
    if let Some(value) = fields.damage_secondary {
        write_f32(dnam, FO4_DNAM_DAMAGE_SECONDARY_OFFSET, value);
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
        let source_plugin_info = session.source_slot_opt().map(|slot| {
            (
                slot.parsed.header.masters.clone(),
                slot.parsed.plugin_name.clone(),
                mapper.interner.intern(&slot.parsed.plugin_name),
            )
        });
        let target_masters = session.target_masters().to_vec();
        let target_to_source: FxHashMap<FormKey, FormKey> = mapper
            .source_to_target_iter()
            .map(|(source, target)| (target, source))
            .collect();
        let mut report = FixupReport::empty();
        let mut changed_records = Vec::new();

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
            let source_fields = source_schema
                .and_then(|schema| {
                    target_to_source
                        .get(&fk)
                        .map(|source_fk| (schema, *source_fk))
                })
                .and_then(|(schema, source_fk)| {
                    let (source_masters, source_plugin_name, source_plugin_sym) =
                        source_plugin_info.as_ref()?;
                    let source_record = session
                        .source_record_decoded(&source_fk, schema, mapper.interner)
                        .ok()?;
                    Some(extract_source_weap_fields(
                        &source_record,
                        source_masters,
                        source_plugin_name,
                        *source_plugin_sym,
                        mapper,
                        &target_masters,
                    ))
                });

            let changed = apply_to_record_with_source(&mut record, dnam_default, source_fields);

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
/// Injection rules (mirror Python logic):
/// - DNAM absent → inject default.
/// - DNAM present as `FieldValue::Bytes` → replace with default (FO76 raw bytes).
/// - DNAM present as another variant → leave as-is (already FO4 structured).
/// - FNAM absent → inject default.
/// - FNAM present (any variant) → leave as-is.
pub fn apply_to_record(record: &mut Record, dnam_default: DnamDefault) -> bool {
    apply_to_record_with_source(record, dnam_default, None)
}

pub fn apply_to_record_with_source(
    record: &mut Record,
    dnam_default: DnamDefault,
    source_fields: Option<SourceWeapFields>,
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

    let need_dnam = !dnam_is_structured; // absent or raw bytes → inject
    let need_fnam = !has_fnam;

    let mut mutated = false;
    let mut synthesized_dnam_flags: Option<u32> = None;

    if need_dnam {
        // Remove existing raw DNAM (FO76 bytes) if present, then push defaults.
        if dnam_is_raw_bytes {
            record.fields.retain(|e| e.sig != dnam_sig);
        }
        let blob = match raw_dnam_bytes.as_deref() {
            Some(raw) => fo4_dnam_from_fo76_raw(raw, dnam_default, source_fields),
            None => dnam_default_bytes(dnam_default),
        };
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
    source_masters: &[String],
    source_plugin_name: &str,
    source_plugin_sym: Sym,
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> SourceWeapFields {
    let mut fields = SourceWeapFields::default();
    let dnam_sig = SubrecordSig::from_str("DNAM").ok();
    let rgw3_sig = SubrecordSig::from_str("RGW3").ok();

    for entry in &source_record.fields {
        if Some(entry.sig) == dnam_sig {
            if let FieldValue::Bytes(data) = &entry.value {
                extract_raw_dnam_fields(&mut fields, data);
                if data.len() >= 4 {
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
            } else {
                extract_structured_dnam_fields(&mut fields, &entry.value, mapper.interner);
            }
        } else if Some(entry.sig) == rgw3_sig {
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

    fields
}

fn extract_raw_dnam_fields(fields: &mut SourceWeapFields, data: &[u8]) {
    fields.attack_delay_seconds = read_f32(data, FO76_DNAM_ATTACK_DELAY_OFFSET);
    fields.damage_secondary = read_f32(data, FO76_DNAM_DAMAGE_SECONDARY_OFFSET);
    fields.full_power_seconds = read_f32(data, FO76_DNAM_FULL_POWER_SECONDS_OFFSET);
    fields.min_power_per_shot = read_f32(data, FO76_DNAM_MIN_POWER_PER_SHOT_OFFSET);
    fields.stagger = read_u32(data, FO76_DNAM_STAGGER_OFFSET);
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
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

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
