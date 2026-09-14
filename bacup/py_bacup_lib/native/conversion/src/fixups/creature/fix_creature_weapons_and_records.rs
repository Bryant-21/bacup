//! Fixup: fix creature weapon stats, explosions, race flags, factions, and quests.
//!
//! Creature conversions only (root sig NPC_ or LVLN); mostly fixed-offset byte
//! writes.
//!
//! **WEAP**:
//! - Melee `AttackDelay >= 5` is zeroed.
//! - Ranged weapons missing `damage_base` / `accuracy_bonus` /
//!   `action_point_cost` get FO4 defaults 10 / 100 / 20.
//! - Creature weapons clear `NPCs Use Ammo`, so actors don't exhaust inventory
//!   ammo and fall back to hand-to-hand.
//! - Liberator lasers become embedded weapons that consume no NPC ammo, like
//!   FO4 robot-integrated lasers.
//! - Spit weapons whose EITM points at the converter's own mod get vanilla
//!   `148D8F:Fallout4.esm` (MirelurkHunterSpitAttackSpell).
//! - Ranged creature weapons get `crWeaponRanged`. Floater breath weapons swap
//!   FO76/self and borrowed Bloodbug/rifle selectors for `WeaponTypeFlamer` or
//!   `WeaponTypeCryolater`.
//! - Not handled: `DamageTypes` curve-table cleanup (DAMA `array_struct:I,f`
//!   with a form-version-gated `curve_table` tail).
//!
//! **IDLE**: Floater fire, charge, and right-release trees get animation-group
//! section 171, which FO4 uses to dispatch those creature actions. FO76 leaves
//! it unset; the working FO4 Floater port uses 171.
//!
//! **EXPL**: a zero `DATA.damage` becomes 10.0; spit/barf explosions without
//! `EITM` get `crEnchMirelurkQueenSpit` (`115315:Fallout4.esm`).
//!
//! **RACE**: `VNAM` keeps the FO4 upper equipment mask plus creature low slots;
//! vanilla Deathclaws and Molerats carry high flags there. `hand_to_hand_melee`
//! is cleared for body-fighting creatures and kept for armed races.
//!
//! **FACT**: drop XNAM rows with a null faction. With none left, append FO4's
//! defaults: CaptiveFaction `03E0C8` Friend (3), SuperMutantFaction `058305`
//! Ally (2), MutantHoundFaction `0948B4` Ally, VertibirdFaction `1E5F60`
//! Friend, and Self Ally.
//!
//! **QUST**: a missing 12-byte DNAM is added with flags 0x0311
//! (`start_game_enabled | starts_enabled | run_once |
//! exclude_from_dialogue_export`) and priority 5.

use crate::fixups::creature::creature_predicate::record_is_armed_humanoid;
use crate::fixups::creature::{
    creature_internal_fixup_applies, likely_creature_weapon_editor_id,
    likely_ranged_creature_weapon_editor_id,
};
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// WEAP byte-offset constants
// ---------------------------------------------------------------------------

const WEAP_DNAM_ATTACK_DELAY_OFFSET: usize = 24;
const WEAP_DNAM_FLAGS_OFFSET: usize = 48;
const WEAP_DNAM_ANIM_TYPE_OFFSET: usize = 54;
const WEAP_DNAM_DAMAGE_BASE_OFFSET: usize = 67;
const WEAP_DNAM_ACCURACY_BONUS_OFFSET: usize = 105;
const WEAP_DNAM_ACTION_POINT_COST_OFFSET: usize = 112;
const WEAP_DNAM_MIN_LEN: usize = 116; // need through action_point_cost

const WEAP_ANIM_TYPE_MELEE: u8 = 1;
const WEAP_ANIM_TYPE_GUN: u8 = 9;

const WEAP_FLAG_NPCS_USE_AMMO: u32 = 0x0000_0002;
const WEAP_FLAG_EMBEDDED_WEAPON: u32 = 0x0008_0000;

/// Threshold above which `AttackDelay` is treated as FO76-noise on melee.
const WEAP_MELEE_ATTACK_DELAY_THRESHOLD: f32 = 5.0;

const WEAP_DEFAULT_DAMAGE_BASE: u16 = 10;
const WEAP_DEFAULT_ACCURACY_BONUS: u8 = 100;
const WEAP_DEFAULT_ACTION_POINT_COST: f32 = 20.0;

/// `MirelurkHunterSpitAttackSpell` — raw FormID for `148D8F:Fallout4.esm`.
/// Master byte 0x00 = Fallout4.esm (master index 0 in every FO4 plugin).
const WEAP_SPIT_VANILLA_EFFECT_RAW: u32 = 0x00_148D8F;

const FALLOUT4_ESM: &str = "Fallout4.esm";
const CR_WEAPON_RANGED_LOW24: u32 = 0x18_9348;
const WEAPON_TYPE_FLAMER_LOW24: u32 = 0x22_5760;
const WEAPON_TYPE_CRYOLATER_LOW24: u32 = 0x22_575F;
const ANIMS_GRIP_RIFLE_ASSAULT_LOW24: u32 = 0x01_F947;
const BLOODBUG_WEAPON_LOW24: u32 = 0x03_284E;
const FO76_WEAPON_TYPE_RANGED_LOW24: u32 = 0x33_A7C8;
const FO76_WEAPON_TYPE_FLOATER_FLAMER_LOW24: u32 = 0x5E_8B44;

const FLOATER_ANIMATION_GROUP_SECTION: u8 = 171;
const FLOATER_COMBAT_IDLE_EDITOR_IDS: &[&str] = &[
    "floaterfiresingleroot",
    "floaterfire_default",
    "floaterfire_eyeattack",
    "floaterfireauto",
    "floaterfirecharge",
    "floaterrightrelease",
    "floaterrightreleaseroot",
    "floaterreleaseright",
    "floatermagiccastrighthand",
];

// ---------------------------------------------------------------------------
// EXPL byte-offset constants
// ---------------------------------------------------------------------------

const EXPL_DATA_DAMAGE_OFFSET: usize = 28;
const EXPL_DATA_MIN_LEN: usize = 32; // 6×I + 2×f = 32

const EXPL_DEFAULT_DAMAGE: f32 = 10.0;

/// `crEnchMirelurkQueenSpit` — raw FormID for `115315:Fallout4.esm`.
const EXPL_SPIT_ENCHANTMENT_RAW: u32 = 0x00_115315;

// ---------------------------------------------------------------------------
// RACE / FACT / QUST constants
// ---------------------------------------------------------------------------

/// equip_type bitmask for a race that fights with its body: FO4's upper
/// biped/equipment flags plus `spell` (4096), `gun` (512), `shield` (8192),
/// `torch` (16384).
///
/// `hand_to_hand_melee` (1) is left out: none of the 22 vanilla FO4
/// `ActorTypeCreature` races with attacks set it, while 7/7 `ActorTypeNPC` races
/// do. FO76 sets it on beasts, and carried across it routes the actor down an
/// H2H attack path its creature graph has no states for: `GetAttackState` stays
/// 0 and no melee swing is staged, though the animation plays when sent
/// directly. See [`RACE_VNAM_ARMED_HUMANOID_BITS`] for the exception.
const RACE_VNAM_KEEP_MASK: u32 = 0xFFFF_8000 | 512 | 4096 | 8192 | 16384;

/// Bits restored for a race that carries a weapon: `hand_to_hand_melee` (1) plus
/// the weapon animation types (one-hand sword/dagger/axe/mace, two-hand
/// sword/axe, bow, staff, grenade, mine). FO76 tags mole miners, super mutants
/// and Zetans `ActorTypeCreature` though they use these; FO4 humanoid races
/// carry the whole set.
const RACE_VNAM_ARMED_HUMANOID_BITS: u32 = 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128 | 256 | 1024 | 2048;

/// FACT.XNAM payload size: faction(4) + modifier(4) + group_combat_reaction(4).
const FACT_XNAM_LEN: usize = 12;

/// QUST.DNAM (General) flag bits to set by default:
/// start_game_enabled(1) | starts_enabled(16) | run_once(256) |
/// exclude_from_dialogue_export(512).
const QUST_DEFAULT_GENERAL_FLAGS: u16 = 0x0311;
const QUST_DEFAULT_GENERAL_PRIORITY: u8 = 5;
const QUST_DNAM_LEN: usize = 12;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FixCreatureWeaponsAndRecordsFixup;

impl Fixup for FixCreatureWeaponsAndRecordsFixup {
    fn name(&self) -> &'static str {
        "fix_creature_weapons_and_records"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::CreatureGated
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        creature_internal_fixup_applies(ctx.config)
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        creature_internal_fixup_applies(config)
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let mut report = FixupReport::empty();

        // Pull target plugin own_index so we can build self-FK raw u32 for
        // FACT defaults and recognise mod-local refs for WEAP EITM swap.
        let target_own_index = (session.target_masters().len() & 0xFF) as u8;

        let sigs: &[&str] = if config.is_whole_plugin {
            &["WEAP", "IDLE", "EXPL"]
        } else {
            &["WEAP", "IDLE", "EXPL", "RACE", "FACT", "QUST"]
        };

        let mut replacements = Vec::new();
        for sig_str in sigs {
            let sig =
                SigCode::from_str(sig_str).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("fix_creature_weapons_read({sig_str}):{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };
                let eid_lower = resolve_eid_lower(&record, mapper.interner);
                if config.is_whole_plugin {
                    let keep = match *sig_str {
                        "WEAP" => likely_creature_weapon_editor_id(&eid_lower),
                        "IDLE" => is_floater_combat_idle_editor_id(&eid_lower),
                        "EXPL" => eid_lower.contains("spit") || eid_lower.contains("barf")
                            || eid_lower == super::player_fear::EXPLOSION_EID,
                        _ => false,
                    };
                    if !keep {
                        continue;
                    }
                }

                let changed = match *sig_str {
                    "WEAP" => apply_weap(&mut record, target_own_index, mapper.interner),
                    "IDLE" => apply_idle(&mut record, &eid_lower, mapper.interner),
                    "EXPL" if eid_lower == super::player_fear::EXPLOSION_EID => {
                        super::player_fear::repair_explosion(&mut record, mapper.interner, config.mod_path.as_deref())
                            .map_err(FixupError::Other)?
                    }
                    "EXPL" => apply_expl(&mut record, target_own_index, mapper.interner),
                    "RACE" => apply_race(&mut record, target_own_index),
                    "FACT" => apply_fact(&mut record, target_own_index),
                    "QUST" => apply_qust(&mut record, target_own_index),
                    _ => false,
                };
                if changed {
                    replacements.push(record);
                }
            }
        }
        report.records_changed = session
            .replace_records_contents(replacements, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// IDLE branch
// ---------------------------------------------------------------------------

fn is_floater_combat_idle_editor_id(eid_lower: &str) -> bool {
    FLOATER_COMBAT_IDLE_EDITOR_IDS.contains(&eid_lower)
}

fn apply_idle(record: &mut Record, eid_lower: &str, interner: &crate::sym::StringInterner) -> bool {
    if !is_floater_combat_idle_editor_id(eid_lower) {
        return false;
    }

    let Some(data_sig) = sig("DATA") else {
        return false;
    };
    let group_key = interner.intern("AnimationGroupSection");
    let set_group = |value: &mut FieldValue| match value {
        FieldValue::Struct(fields) => {
            if let Some((_, current)) = fields.iter_mut().find(|(key, _)| *key == group_key) {
                if *current == FieldValue::Uint(u64::from(FLOATER_ANIMATION_GROUP_SECTION)) {
                    return false;
                }
                *current = FieldValue::Uint(u64::from(FLOATER_ANIMATION_GROUP_SECTION));
            } else {
                fields.push((
                    group_key,
                    FieldValue::Uint(u64::from(FLOATER_ANIMATION_GROUP_SECTION)),
                ));
            }
            true
        }
        FieldValue::Bytes(data) if data.len() >= 4 => {
            if data[3] == FLOATER_ANIMATION_GROUP_SECTION {
                return false;
            }
            data[3] = FLOATER_ANIMATION_GROUP_SECTION;
            true
        }
        FieldValue::None => {
            *value = FieldValue::Struct(vec![(
                group_key,
                FieldValue::Uint(u64::from(FLOATER_ANIMATION_GROUP_SECTION)),
            )]);
            true
        }
        _ => false,
    };

    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == data_sig) {
        return set_group(&mut entry.value);
    }

    record.fields.push(FieldEntry {
        sig: data_sig,
        value: FieldValue::Struct(vec![(
            group_key,
            FieldValue::Uint(u64::from(FLOATER_ANIMATION_GROUP_SECTION)),
        )]),
    });
    true
}

// ---------------------------------------------------------------------------
// WEAP branch
// ---------------------------------------------------------------------------

/// Apply WEAP creature-weapon cleanup. Returns `true` when any mutation
/// occurred.
pub fn apply_weap(
    record: &mut Record,
    target_own_index: u8,
    interner: &crate::sym::StringInterner,
) -> bool {
    let mut changed = false;

    // Figure out is_ranged using DNAM.animation_type (byte 54) and the
    // record's editor_id (for spit/barf creature weapons).
    let anim_type = read_dnam_byte(record, WEAP_DNAM_ANIM_TYPE_OFFSET);
    let eid_lower = resolve_eid_lower(record, interner);
    let has_ranged_eid = likely_ranged_creature_weapon_editor_id(&eid_lower);
    let is_ranged = match anim_type {
        Some(WEAP_ANIM_TYPE_MELEE) => false,
        Some(WEAP_ANIM_TYPE_GUN) => true,
        _ => has_ranged_eid,
    };

    if !is_ranged {
        // Zero melee AttackDelay >= 5; the engine never reads it.
        if let Some(delay) = read_dnam_f32(record, WEAP_DNAM_ATTACK_DELAY_OFFSET) {
            if delay >= WEAP_MELEE_ATTACK_DELAY_THRESHOLD {
                if write_dnam_f32(record, WEAP_DNAM_ATTACK_DELAY_OFFSET, 0.0) {
                    changed = true;
                }
            }
        }
    } else {
        // Default damage_base / accuracy_bonus / action_point_cost when zero.
        if let Some(dam_base) = read_dnam_u16(record, WEAP_DNAM_DAMAGE_BASE_OFFSET) {
            if dam_base == 0
                && write_dnam_u16(
                    record,
                    WEAP_DNAM_DAMAGE_BASE_OFFSET,
                    WEAP_DEFAULT_DAMAGE_BASE,
                )
            {
                changed = true;
            }
        }
        if let Some(acc_bonus) = read_dnam_byte(record, WEAP_DNAM_ACCURACY_BONUS_OFFSET) {
            if acc_bonus == 0
                && write_dnam_byte(
                    record,
                    WEAP_DNAM_ACCURACY_BONUS_OFFSET,
                    WEAP_DEFAULT_ACCURACY_BONUS,
                )
            {
                changed = true;
            }
        }
        if let Some(ap_cost) = read_dnam_f32(record, WEAP_DNAM_ACTION_POINT_COST_OFFSET) {
            if ap_cost == 0.0
                && write_dnam_f32(
                    record,
                    WEAP_DNAM_ACTION_POINT_COST_OFFSET,
                    WEAP_DEFAULT_ACTION_POINT_COST,
                )
            {
                changed = true;
            }
        }
        // "DamageTypes" (DAMA) cleanup is form-version-conditional; deferred.

        if eid_lower.contains("liberator") && eid_lower.contains("laser") {
            if let Some(flags) = read_dnam_u32(record, WEAP_DNAM_FLAGS_OFFSET) {
                let repaired = flags | WEAP_FLAG_EMBEDDED_WEAPON;
                if repaired != flags && write_dnam_u32(record, WEAP_DNAM_FLAGS_OFFSET, repaired) {
                    changed = true;
                }
            }
        }

        if likely_creature_weapon_editor_id(&eid_lower)
            && normalize_ranged_creature_keywords(record, &eid_lower, target_own_index, interner)
        {
            changed = true;
        }
    }

    if let Some(flags) = read_dnam_u32(record, WEAP_DNAM_FLAGS_OFFSET) {
        let repaired = flags & !WEAP_FLAG_NPCS_USE_AMMO;
        if repaired != flags && write_dnam_u32(record, WEAP_DNAM_FLAGS_OFFSET, repaired) {
            changed = true;
        }
    }

    // Spit weapons: an EITM pointing into the converter's own plugin (master
    // byte == target_own_index) becomes vanilla MirelurkHunterSpitAttackSpell.
    if eid_lower.contains("spit") {
        if swap_mod_local_eitm(record, target_own_index, WEAP_SPIT_VANILLA_EFFECT_RAW) {
            changed = true;
        }
    }

    changed
}

fn normalize_ranged_creature_keywords(
    record: &mut Record,
    eid_lower: &str,
    target_own_index: u8,
    interner: &crate::sym::StringInterner,
) -> bool {
    let mut changed = false;
    if eid_lower.starts_with("crfloater") {
        for low24 in [ANIMS_GRIP_RIFLE_ASSAULT_LOW24, BLOODBUG_WEAPON_LOW24] {
            changed |= remove_fo4_keyword(record, low24, interner);
        }
        for low24 in [
            FO76_WEAPON_TYPE_RANGED_LOW24,
            FO76_WEAPON_TYPE_FLOATER_FLAMER_LOW24,
        ] {
            changed |= remove_own_keyword(record, low24, target_own_index);
        }

        if eid_lower.contains("flamer") {
            changed |= append_fo4_keyword(record, WEAPON_TYPE_FLAMER_LOW24, interner);
        } else if eid_lower.contains("freezer") {
            changed |= append_fo4_keyword(record, WEAPON_TYPE_CRYOLATER_LOW24, interner);
        }
    }
    changed |= append_fo4_keyword(record, CR_WEAPON_RANGED_LOW24, interner);
    if changed {
        sync_weap_keyword_count(record);
    }
    changed
}

fn remove_fo4_keyword(
    record: &mut Record,
    wanted_low24: u32,
    interner: &crate::sym::StringInterner,
) -> bool {
    remove_keyword_matching(record, |raw, fk| {
        raw.is_some_and(|raw| raw >> 24 == 0 && raw & 0x00FF_FFFF == wanted_low24)
            || fk.is_some_and(|fk| {
                fk.local & 0x00FF_FFFF == wanted_low24
                    && interner
                        .resolve(fk.plugin)
                        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FALLOUT4_ESM))
            })
    })
}

fn remove_own_keyword(record: &mut Record, wanted_low24: u32, own_index: u8) -> bool {
    let own_plugin = record.form_key.plugin;
    remove_keyword_matching(record, |raw, fk| {
        raw.is_some_and(|raw| (raw >> 24) as u8 == own_index && raw & 0x00FF_FFFF == wanted_low24)
            || fk
                .is_some_and(|fk| fk.plugin == own_plugin && fk.local & 0x00FF_FFFF == wanted_low24)
    })
}

fn remove_keyword_matching(
    record: &mut Record,
    matches: impl Fn(Option<u32>, Option<&crate::ids::FormKey>) -> bool + Copy,
) -> bool {
    let Some(kwda_sig) = sig("KWDA") else {
        return false;
    };
    record
        .fields
        .iter_mut()
        .filter(|entry| entry.sig == kwda_sig)
        .fold(false, |changed, entry| {
            remove_keyword_from_value(&mut entry.value, matches) || changed
        })
}

fn remove_keyword_from_value(
    value: &mut FieldValue,
    matches: impl Fn(Option<u32>, Option<&crate::ids::FormKey>) -> bool + Copy,
) -> bool {
    match value {
        FieldValue::Bytes(data) => {
            let original_len = data.len();
            let mut filtered = smallvec::SmallVec::new();
            for chunk in data.chunks_exact(4) {
                let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                if !matches(Some(raw), None) {
                    filtered.extend_from_slice(chunk);
                }
            }
            if filtered.len() == original_len {
                return false;
            }
            *data = filtered;
            true
        }
        FieldValue::FormKey(fk) if matches(None, Some(fk)) => {
            *value = FieldValue::None;
            true
        }
        FieldValue::List(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                changed |= remove_keyword_from_value(item, matches);
            }
            if changed {
                items.retain(|item| !matches!(item, FieldValue::None));
            }
            changed
        }
        _ => false,
    }
}

fn append_fo4_keyword(
    record: &mut Record,
    low24: u32,
    interner: &crate::sym::StringInterner,
) -> bool {
    if record_has_fo4_keyword(record, low24, interner) {
        return false;
    }
    let Some(kwda_sig) = sig("KWDA") else {
        return false;
    };
    let keyword = crate::ids::FormKey {
        local: low24,
        plugin: interner.intern(FALLOUT4_ESM),
    };
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == kwda_sig) {
        match &mut entry.value {
            FieldValue::Bytes(data) => data.extend_from_slice(&low24.to_le_bytes()),
            FieldValue::List(items) => items.push(FieldValue::FormKey(keyword)),
            value @ FieldValue::None => *value = FieldValue::FormKey(keyword),
            value @ FieldValue::FormKey(_) => {
                let existing = std::mem::replace(value, FieldValue::None);
                *value = FieldValue::List(vec![existing, FieldValue::FormKey(keyword)]);
            }
            _ => return false,
        }
        return true;
    }

    record.fields.push(FieldEntry {
        sig: kwda_sig,
        value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&low24.to_le_bytes())),
    });
    true
}

fn record_has_fo4_keyword(
    record: &Record,
    wanted_low24: u32,
    interner: &crate::sym::StringInterner,
) -> bool {
    let Some(kwda_sig) = sig("KWDA") else {
        return false;
    };
    record.fields.iter().any(|entry| {
        entry.sig == kwda_sig && field_value_has_fo4_keyword(&entry.value, wanted_low24, interner)
    })
}

fn field_value_has_fo4_keyword(
    value: &FieldValue,
    wanted_low24: u32,
    interner: &crate::sym::StringInterner,
) -> bool {
    match value {
        FieldValue::Bytes(data) => data.chunks_exact(4).any(|chunk| {
            let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            raw >> 24 == 0 && raw & 0x00FF_FFFF == wanted_low24
        }),
        FieldValue::FormKey(fk) => {
            fk.local & 0x00FF_FFFF == wanted_low24
                && interner
                    .resolve(fk.plugin)
                    .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FALLOUT4_ESM))
        }
        FieldValue::List(items) => items
            .iter()
            .any(|item| field_value_has_fo4_keyword(item, wanted_low24, interner)),
        _ => false,
    }
}

fn sync_weap_keyword_count(record: &mut Record) {
    let (Some(kwda_sig), Some(ksiz_sig)) = (sig("KWDA"), sig("KSIZ")) else {
        return;
    };
    let count = record
        .fields
        .iter()
        .filter(|entry| entry.sig == kwda_sig)
        .map(|entry| weap_keyword_value_count(&entry.value))
        .sum::<u32>();
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == ksiz_sig) {
        write_weap_keyword_count(&mut entry.value, count);
        return;
    }
    let insert_at = record
        .fields
        .iter()
        .position(|entry| entry.sig == kwda_sig)
        .unwrap_or(record.fields.len());
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: ksiz_sig,
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&count.to_le_bytes())),
        },
    );
}

fn weap_keyword_value_count(value: &FieldValue) -> u32 {
    match value {
        FieldValue::Bytes(data) => (data.len() / 4) as u32,
        FieldValue::FormKey(_) => 1,
        FieldValue::List(items) => items.iter().map(weap_keyword_value_count).sum(),
        _ => 0,
    }
}

fn write_weap_keyword_count(value: &mut FieldValue, count: u32) {
    match value {
        FieldValue::Bytes(data) => {
            data.clear();
            data.extend_from_slice(&count.to_le_bytes());
        }
        FieldValue::Int(current) => *current = i64::from(count),
        FieldValue::Uint(current) => *current = u64::from(count),
        _ => *value = FieldValue::Bytes(smallvec::SmallVec::from_slice(&count.to_le_bytes())),
    }
}

/// Rewrite a single 4-byte EITM (Effect) FormID payload when the current
/// master byte matches `target_own_index`. Returns `true` on change.
fn swap_mod_local_eitm(record: &mut Record, target_own_index: u8, replacement_raw: u32) -> bool {
    let eitm_sig = match SubrecordSig::from_str("EITM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    for entry in record.fields.iter_mut() {
        if entry.sig != eitm_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(data) if data.len() >= 4 => {
                let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                let current_master = ((raw >> 24) & 0xFF) as u8;
                if current_master != target_own_index || raw == 0 {
                    return false;
                }
                let new_bytes = replacement_raw.to_le_bytes();
                data[0..4].copy_from_slice(&new_bytes);
                return true;
            }
            FieldValue::FormKey(fk) => {
                // A FormKey carries its master in `fk.plugin`, not a master
                // byte, so "own plugin" can't be checked here. Skip it; the
                // bytes path covers `read_record` output.
                let _ = fk;
                return false;
            }
            _ => return false,
        }
    }
    false
}

// ---------------------------------------------------------------------------
// EXPL branch
// ---------------------------------------------------------------------------

/// Apply EXPL creature-explosion cleanup. Returns `true` on change.
pub fn apply_expl(
    record: &mut Record,
    _target_own_index: u8,
    interner: &crate::sym::StringInterner,
) -> bool {
    let mut changed = false;

    // Ensure DATA.damage (offset 28) is non-zero.
    if let Some(data_sig) = sig("DATA") {
        for entry in record.fields.iter_mut() {
            if entry.sig != data_sig {
                continue;
            }
            if let FieldValue::Bytes(data) = &mut entry.value {
                if data.len() >= EXPL_DATA_MIN_LEN {
                    let cur = f32::from_le_bytes([
                        data[EXPL_DATA_DAMAGE_OFFSET],
                        data[EXPL_DATA_DAMAGE_OFFSET + 1],
                        data[EXPL_DATA_DAMAGE_OFFSET + 2],
                        data[EXPL_DATA_DAMAGE_OFFSET + 3],
                    ]);
                    if cur == 0.0 {
                        data[EXPL_DATA_DAMAGE_OFFSET..EXPL_DATA_DAMAGE_OFFSET + 4]
                            .copy_from_slice(&EXPL_DEFAULT_DAMAGE.to_le_bytes());
                        changed = true;
                    }
                }
            }
            break; // only one DATA
        }
    }

    // Spit/barf explosions missing EITM get crEnchMirelurkQueenSpit, inserted
    // before DATA (FO4 schema order) when DATA exists.
    let eid_lower = resolve_eid_lower(record, interner);
    let is_spit_or_barf = eid_lower.contains("spit") || eid_lower.contains("barf");
    if is_spit_or_barf && !has_subrecord(record, "EITM") {
        if add_eitm_before_data(record, EXPL_SPIT_ENCHANTMENT_RAW) {
            changed = true;
        }
    }

    changed
}

/// Insert a new EITM subrecord (4-byte FormID) before the first DATA
/// subrecord. Falls back to appending if DATA isn't present. Returns `true`
/// on insert.
fn add_eitm_before_data(record: &mut Record, raw_form_id: u32) -> bool {
    let eitm_sig = match SubrecordSig::from_str("EITM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let data_sig = SubrecordSig::from_str("DATA").ok();
    let mut payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
    payload.extend_from_slice(&raw_form_id.to_le_bytes());
    let entry = FieldEntry {
        sig: eitm_sig,
        value: FieldValue::Bytes(payload),
    };
    let insert_at = match data_sig {
        Some(ds) => record.fields.iter().position(|e| e.sig == ds),
        None => None,
    };
    match insert_at {
        Some(idx) => record.fields.insert(idx, entry),
        None => record.fields.push(entry),
    }
    true
}

// ---------------------------------------------------------------------------
// RACE branch
// ---------------------------------------------------------------------------

/// Apply RACE.VNAM filter: preserve FO4-valid upper bits and creature slots.
///
/// Races that actually carry weapons keep their equipment-type bits verbatim —
/// see [`record_is_armed_humanoid`]. Stripping is deliberately one-sided: an
/// unused bit only permits an equip that never happens, while a missing one
/// blocks the race from holding that weapon class at all.
pub fn apply_race(record: &mut Record, _target_own_index: u8) -> bool {
    let keep_mask = if record_is_armed_humanoid(record) {
        RACE_VNAM_KEEP_MASK | RACE_VNAM_ARMED_HUMANOID_BITS
    } else {
        RACE_VNAM_KEEP_MASK
    };
    let vnam_sig = match SubrecordSig::from_str("VNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if entry.sig != vnam_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(data) if data.len() >= 4 => {
                let cur = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                let masked = cur & keep_mask;
                if cur != masked {
                    data[0..4].copy_from_slice(&masked.to_le_bytes());
                    changed = true;
                }
            }
            FieldValue::Uint(n) => {
                let cur = (*n & 0xFFFF_FFFF) as u32;
                let masked = cur & keep_mask;
                if cur != masked {
                    *n = masked as u64;
                    changed = true;
                }
            }
            _ => {}
        }
        break; // only one VNAM
    }
    changed
}

// ---------------------------------------------------------------------------
// FACT branch
// ---------------------------------------------------------------------------

/// Apply FACT XNAM cleanup + default-Relations append. Returns `true` on
/// change.
pub fn apply_fact(record: &mut Record, target_own_index: u8) -> bool {
    let xnam_sig = match SubrecordSig::from_str("XNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut changed = false;

    // Drop XNAM entries whose faction FormID is null (raw 0).
    let before = record.fields.len();
    let mut kept: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    for entry in record.fields.drain(..) {
        if entry.sig != xnam_sig {
            kept.push(entry);
            continue;
        }
        let keep = match &entry.value {
            FieldValue::Bytes(data) if data.len() >= FACT_XNAM_LEN => {
                let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                raw != 0
            }
            FieldValue::Struct(_) => true, // can't safely peek; assume non-null
            _ => false,
        };
        if keep {
            kept.push(entry);
        } else {
            changed = true;
        }
    }
    record.fields = kept;
    let _ = before;

    // If zero XNAM remain, add the 5 FO4 creature default Relations.
    let has_xnam = record.fields.iter().any(|e| e.sig == xnam_sig);
    if !has_xnam {
        // Self FK as raw u32: master byte = target_own_index, object_id =
        // record.form_key.local & 0xFFFFFF.
        let own_raw = ((target_own_index as u32) << 24) | (record.form_key.local & 0x00FF_FFFF);
        let defaults: [(u32, u32); 5] = [
            (0x00_03E0C8, 3), // CaptiveFaction / Friend
            (0x00_058305, 2), // SuperMutantFaction / Ally
            (0x00_0948B4, 2), // MutantHoundFaction / Ally
            (0x00_1E5F60, 3), // VertibirdFaction / Friend
            (own_raw, 2),     // Self / Ally
        ];
        for (faction_raw, reaction) in defaults {
            let mut payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
            payload.extend_from_slice(&faction_raw.to_le_bytes());
            payload.extend_from_slice(&0i32.to_le_bytes()); // modifier
            payload.extend_from_slice(&reaction.to_le_bytes());
            record.fields.push(FieldEntry {
                sig: xnam_sig,
                value: FieldValue::Bytes(payload),
            });
        }
        changed = true;
    }

    changed
}

// ---------------------------------------------------------------------------
// QUST branch
// ---------------------------------------------------------------------------

/// Apply QUST default DNAM (General). Returns `true` on insert.
pub fn apply_qust(record: &mut Record, _target_own_index: u8) -> bool {
    if has_subrecord(record, "DNAM") {
        return false;
    }
    let dnam_sig = match SubrecordSig::from_str("DNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
    payload.resize(QUST_DNAM_LEN, 0);
    payload[0..2].copy_from_slice(&QUST_DEFAULT_GENERAL_FLAGS.to_le_bytes());
    payload[2] = QUST_DEFAULT_GENERAL_PRIORITY;
    record.fields.push(FieldEntry {
        sig: dnam_sig,
        value: FieldValue::Bytes(payload),
    });
    true
}

// ---------------------------------------------------------------------------
// Byte-level helpers
// ---------------------------------------------------------------------------

fn sig(name: &str) -> Option<SubrecordSig> {
    SubrecordSig::from_str(name).ok()
}

fn has_subrecord(record: &Record, sig_name: &str) -> bool {
    match sig(sig_name) {
        Some(s) => record.fields.iter().any(|e| e.sig == s),
        None => false,
    }
}

fn read_dnam_byte(record: &Record, offset: usize) -> Option<u8> {
    let dnam_sig = sig("DNAM")?;
    for entry in &record.fields {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &entry.value {
            if data.len() > offset {
                return Some(data[offset]);
            }
        }
        return None;
    }
    None
}

fn read_dnam_u16(record: &Record, offset: usize) -> Option<u16> {
    let dnam_sig = sig("DNAM")?;
    for entry in &record.fields {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &entry.value {
            if data.len() >= offset + 2 {
                return Some(u16::from_le_bytes([data[offset], data[offset + 1]]));
            }
        }
        return None;
    }
    None
}

fn read_dnam_u32(record: &Record, offset: usize) -> Option<u32> {
    let dnam_sig = sig("DNAM")?;
    for entry in &record.fields {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &entry.value {
            if data.len() >= offset + 4 {
                return Some(u32::from_le_bytes(
                    data[offset..offset + 4].try_into().ok()?,
                ));
            }
        }
        return None;
    }
    None
}

fn read_dnam_f32(record: &Record, offset: usize) -> Option<f32> {
    let dnam_sig = sig("DNAM")?;
    for entry in &record.fields {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &entry.value {
            if data.len() >= offset + 4 {
                return Some(f32::from_le_bytes([
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ]));
            }
        }
        return None;
    }
    None
}

fn write_dnam_byte(record: &mut Record, offset: usize, value: u8) -> bool {
    let dnam_sig = match sig("DNAM") {
        Some(s) => s,
        None => return false,
    };
    for entry in record.fields.iter_mut() {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &mut entry.value {
            if data.len() > offset {
                data[offset] = value;
                return true;
            }
        }
        return false;
    }
    false
}

fn write_dnam_u16(record: &mut Record, offset: usize, value: u16) -> bool {
    let dnam_sig = match sig("DNAM") {
        Some(s) => s,
        None => return false,
    };
    for entry in record.fields.iter_mut() {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &mut entry.value {
            if data.len() >= offset + 2 {
                data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
                return true;
            }
        }
        return false;
    }
    false
}

fn write_dnam_u32(record: &mut Record, offset: usize, value: u32) -> bool {
    let dnam_sig = match sig("DNAM") {
        Some(s) => s,
        None => return false,
    };
    for entry in record.fields.iter_mut() {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &mut entry.value {
            if data.len() >= offset + 4 {
                data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                return true;
            }
        }
        return false;
    }
    false
}

fn write_dnam_f32(record: &mut Record, offset: usize, value: f32) -> bool {
    let dnam_sig = match sig("DNAM") {
        Some(s) => s,
        None => return false,
    };
    for entry in record.fields.iter_mut() {
        if entry.sig != dnam_sig {
            continue;
        }
        if let FieldValue::Bytes(data) = &mut entry.value {
            if data.len() >= offset + 4 {
                data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                return true;
            }
        }
        return false;
    }
    false
}

/// Resolve a record's eid to a lowercase string for substring matching.
/// Returns `""` when the record has no eid or the interner doesn't hold it.
fn resolve_eid_lower(record: &Record, interner: &crate::sym::StringInterner) -> String {
    record
        .eid
        .and_then(|sym| interner.resolve(sym))
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::creature::creature_predicate::{
        ACTOR_TYPE_HUMANLIKE_LOW24, ACTOR_TYPE_SUPER_MUTANT_LOW24,
    };
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn make_record(
        sig_str: &str,
        local: u32,
        plugin: &str,
        eid: Option<&str>,
        interner: &StringInterner,
    ) -> Record {
        Record {
            sig: SigCode::from_str(sig_str).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: eid.map(|s| interner.intern(s)),
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push_bytes(record: &mut Record, sig_str: &str, data: Vec<u8>) {
        let mut bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        bytes.extend_from_slice(&data);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig_str).unwrap(),
            value: FieldValue::Bytes(bytes),
        });
    }

    fn make_dnam_bytes(
        anim_type: u8,
        attack_delay: f32,
        damage_base: u16,
        accuracy_bonus: u8,
        action_point_cost: f32,
    ) -> Vec<u8> {
        let mut data = vec![0u8; 132];
        data[WEAP_DNAM_ATTACK_DELAY_OFFSET..WEAP_DNAM_ATTACK_DELAY_OFFSET + 4]
            .copy_from_slice(&attack_delay.to_le_bytes());
        data[WEAP_DNAM_ANIM_TYPE_OFFSET] = anim_type;
        data[WEAP_DNAM_DAMAGE_BASE_OFFSET..WEAP_DNAM_DAMAGE_BASE_OFFSET + 2]
            .copy_from_slice(&damage_base.to_le_bytes());
        data[WEAP_DNAM_ACCURACY_BONUS_OFFSET] = accuracy_bonus;
        data[WEAP_DNAM_ACTION_POINT_COST_OFFSET..WEAP_DNAM_ACTION_POINT_COST_OFFSET + 4]
            .copy_from_slice(&action_point_cost.to_le_bytes());
        let _ = WEAP_DNAM_MIN_LEN;
        data
    }

    fn keyword_raws(record: &Record) -> Vec<u32> {
        let kwda = SubrecordSig::from_str("KWDA").unwrap();
        let entry = record
            .fields
            .iter()
            .find(|entry| entry.sig == kwda)
            .expect("KWDA");
        let FieldValue::Bytes(data) = &entry.value else {
            panic!("expected byte-shaped KWDA");
        };
        data.chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    fn idle_group_section(record: &Record, interner: &StringInterner) -> Option<u64> {
        let data_sig = SubrecordSig::from_str("DATA").unwrap();
        let group_key = interner.intern("AnimationGroupSection");
        let entry = record.fields.iter().find(|entry| entry.sig == data_sig)?;
        match &entry.value {
            FieldValue::Struct(fields) => fields.iter().find_map(|(key, value)| {
                (*key == group_key)
                    .then_some(value)
                    .and_then(|value| match value {
                        FieldValue::Uint(value) => Some(*value),
                        _ => None,
                    })
            }),
            FieldValue::Bytes(data) if data.len() >= 4 => Some(u64::from(data[3])),
            _ => None,
        }
    }

    // ── IDLE: Floater combat animation-group section ──────────────────────

    #[test]
    fn floater_combat_idles_use_fan_port_animation_group_section() {
        let interner = StringInterner::new();
        for (index, eid_lower) in FLOATER_COMBAT_IDLE_EDITOR_IDS.iter().enumerate() {
            let mut record = make_record(
                "IDLE",
                0x550000 + index as u32,
                "Output.esp",
                Some(eid_lower),
                &interner,
            );
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("DATA").unwrap(),
                value: FieldValue::Struct(Vec::new()),
            });

            assert!(apply_idle(&mut record, eid_lower, &interner), "{eid_lower}");
            assert_eq!(
                idle_group_section(&record, &interner),
                Some(u64::from(FLOATER_ANIMATION_GROUP_SECTION)),
                "{eid_lower}"
            );
            assert!(
                !apply_idle(&mut record, eid_lower, &interner),
                "{eid_lower} should be idempotent"
            );
        }
    }

    #[test]
    fn unrelated_idle_keeps_its_animation_group_section() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "IDLE",
            0x0314DE,
            "Fallout4.esm",
            Some("BloatflyFireSingle"),
            &interner,
        );
        push_bytes(&mut record, "DATA", vec![0, 0, 0, 0, 0, 0]);

        assert!(!apply_idle(&mut record, "bloatflyfiresingle", &interner));
        assert_eq!(idle_group_section(&record, &interner), Some(0));
    }

    // ── WEAP: melee AttackDelay strip ─────────────────────────────────────

    /// Melee weapon with attack_delay >= 5 is reset to 0.
    #[test]
    fn weap_melee_strips_high_attack_delay() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "WEAP",
            0x0800,
            "Output.esp",
            Some("crMeleeClaw"),
            &mut interner,
        );
        push_bytes(
            &mut r,
            "DNAM",
            make_dnam_bytes(WEAP_ANIM_TYPE_MELEE, 7.5, 0, 0, 0.0),
        );
        let changed = apply_weap(&mut r, 1, &interner);
        assert!(changed);
        assert_eq!(read_dnam_f32(&r, WEAP_DNAM_ATTACK_DELAY_OFFSET), Some(0.0));
    }

    /// Melee weapon with low attack_delay is not touched.
    #[test]
    fn weap_melee_keeps_low_attack_delay() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "WEAP",
            0x0800,
            "Output.esp",
            Some("crMeleeClaw"),
            &mut interner,
        );
        push_bytes(
            &mut r,
            "DNAM",
            make_dnam_bytes(WEAP_ANIM_TYPE_MELEE, 2.5, 0, 0, 0.0),
        );
        let changed = apply_weap(&mut r, 1, &interner);
        assert!(!changed);
        assert_eq!(read_dnam_f32(&r, WEAP_DNAM_ATTACK_DELAY_OFFSET), Some(2.5));
    }

    // ── WEAP: ranged defaults ─────────────────────────────────────────────

    /// Gun-type weapon with zero damage_base / accuracy_bonus /
    /// action_point_cost gets FO4 defaults.
    #[test]
    fn weap_gun_fills_defaults() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "WEAP",
            0x0800,
            "Output.esp",
            Some("crGunPlasma"),
            &mut interner,
        );
        push_bytes(
            &mut r,
            "DNAM",
            make_dnam_bytes(WEAP_ANIM_TYPE_GUN, 0.0, 0, 0, 0.0),
        );
        let changed = apply_weap(&mut r, 1, &interner);
        assert!(changed);
        assert_eq!(
            read_dnam_u16(&r, WEAP_DNAM_DAMAGE_BASE_OFFSET),
            Some(WEAP_DEFAULT_DAMAGE_BASE)
        );
        assert_eq!(
            read_dnam_byte(&r, WEAP_DNAM_ACCURACY_BONUS_OFFSET),
            Some(WEAP_DEFAULT_ACCURACY_BONUS)
        );
        let ap = read_dnam_f32(&r, WEAP_DNAM_ACTION_POINT_COST_OFFSET).unwrap();
        assert!((ap - WEAP_DEFAULT_ACTION_POINT_COST).abs() < 1e-6);
    }

    /// Gun-type weapon with existing non-zero values is left alone.
    #[test]
    fn weap_gun_preserves_existing_values() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "WEAP",
            0x0800,
            "Output.esp",
            Some("crGunPlasma"),
            &mut interner,
        );
        push_bytes(
            &mut r,
            "DNAM",
            make_dnam_bytes(WEAP_ANIM_TYPE_GUN, 0.0, 42, 75, 30.0),
        );
        push_bytes(&mut r, "KSIZ", 1u32.to_le_bytes().to_vec());
        push_bytes(
            &mut r,
            "KWDA",
            CR_WEAPON_RANGED_LOW24.to_le_bytes().to_vec(),
        );
        let changed = apply_weap(&mut r, 1, &interner);
        assert!(!changed);
        assert_eq!(read_dnam_u16(&r, WEAP_DNAM_DAMAGE_BASE_OFFSET), Some(42));
    }

    #[test]
    fn floater_flamer_uses_fo4_ranged_keyword_contract() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "WEAP",
            0x55DA6B,
            "Output.esp",
            Some("crFloaterFlamerBreath"),
            &interner,
        );
        push_bytes(
            &mut record,
            "DNAM",
            make_dnam_bytes(WEAP_ANIM_TYPE_GUN, 0.0, 10, 100, 20.0),
        );
        let keywords = [
            ANIMS_GRIP_RIFLE_ASSAULT_LOW24,
            BLOODBUG_WEAPON_LOW24,
            (1u32 << 24) | FO76_WEAPON_TYPE_RANGED_LOW24,
            (1u32 << 24) | FO76_WEAPON_TYPE_FLOATER_FLAMER_LOW24,
            0x05_DDA2,
        ];
        push_bytes(
            &mut record,
            "KSIZ",
            (keywords.len() as u32).to_le_bytes().to_vec(),
        );
        push_bytes(
            &mut record,
            "KWDA",
            keywords.iter().flat_map(|raw| raw.to_le_bytes()).collect(),
        );

        assert!(apply_weap(&mut record, 1, &interner));
        assert_eq!(
            keyword_raws(&record),
            vec![0x05_DDA2, WEAPON_TYPE_FLAMER_LOW24, CR_WEAPON_RANGED_LOW24,]
        );
        let ksiz = record
            .fields
            .iter()
            .find(|entry| entry.sig == SubrecordSig::from_str("KSIZ").unwrap())
            .unwrap();
        let FieldValue::Bytes(ksiz) = &ksiz.value else {
            panic!("expected byte-shaped KSIZ");
        };
        assert_eq!(u32::from_le_bytes(ksiz.as_slice().try_into().unwrap()), 3);
        assert!(!apply_weap(&mut record, 1, &interner));
    }

    #[test]
    fn floater_freezer_adds_cryolater_and_ranged_formkeys() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "WEAP",
            0x55DA67,
            "Output.esp",
            Some("crFloaterFreezerBreath"),
            &interner,
        );
        push_bytes(
            &mut record,
            "DNAM",
            make_dnam_bytes(WEAP_ANIM_TYPE_GUN, 0.0, 10, 100, 20.0),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KWDA").unwrap(),
            value: FieldValue::FormKey(FormKey {
                local: FO76_WEAPON_TYPE_RANGED_LOW24,
                plugin: interner.intern("Output.esp"),
            }),
        });

        assert!(apply_weap(&mut record, 1, &interner));
        assert!(record_has_fo4_keyword(
            &record,
            WEAPON_TYPE_CRYOLATER_LOW24,
            &interner
        ));
        assert!(record_has_fo4_keyword(
            &record,
            CR_WEAPON_RANGED_LOW24,
            &interner
        ));
        let kwda = record
            .fields
            .iter()
            .find(|entry| entry.sig == SubrecordSig::from_str("KWDA").unwrap())
            .unwrap();
        let FieldValue::List(keywords) = &kwda.value else {
            panic!("expected formkey list");
        };
        assert_eq!(keywords.len(), 2);
        assert!(!apply_weap(&mut record, 1, &interner));
    }

    #[test]
    fn ranged_creature_weapon_clears_npc_ammo_flag_with_add_ammo_list() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "WEAP",
            0x5FA1EA,
            "Output.esp",
            Some("crMoleMinerBoss_Launcher_DailyOps"),
            &interner,
        );
        let mut dnam = make_dnam_bytes(WEAP_ANIM_TYPE_GUN, 1.2, 44, 100, 20.0);
        dnam[WEAP_DNAM_FLAGS_OFFSET..WEAP_DNAM_FLAGS_OFFSET + 4]
            .copy_from_slice(&WEAP_FLAG_NPCS_USE_AMMO.to_le_bytes());
        push_bytes(&mut record, "DNAM", dnam);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("LNAM").unwrap(),
            value: FieldValue::FormKey(FormKey {
                local: 0x178EC6,
                plugin: interner.intern("Fallout4.esm"),
            }),
        });

        assert!(apply_weap(&mut record, 1, &interner));
        let flags = read_dnam_u32(&record, WEAP_DNAM_FLAGS_OFFSET).unwrap();
        assert_eq!(flags & WEAP_FLAG_NPCS_USE_AMMO, 0);
        assert_eq!(flags & WEAP_FLAG_EMBEDDED_WEAPON, 0);
    }

    #[test]
    fn liberator_laser_uses_fo4_embedded_weapon_flags() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "WEAP",
            0x10D80A,
            "Output.esp",
            Some("crLiberatorLaserGun"),
            &interner,
        );
        let mut dnam = make_dnam_bytes(WEAP_ANIM_TYPE_GUN, 0.0, 10, 100, 20.0);
        dnam[WEAP_DNAM_FLAGS_OFFSET..WEAP_DNAM_FLAGS_OFFSET + 4]
            .copy_from_slice(&WEAP_FLAG_NPCS_USE_AMMO.to_le_bytes());
        push_bytes(&mut record, "DNAM", dnam);

        assert!(apply_weap(&mut record, 1, &interner));
        let flags = read_dnam_u32(&record, WEAP_DNAM_FLAGS_OFFSET).unwrap();
        assert_eq!(flags & WEAP_FLAG_EMBEDDED_WEAPON, WEAP_FLAG_EMBEDDED_WEAPON);
        assert_eq!(flags & WEAP_FLAG_NPCS_USE_AMMO, 0);
    }

    #[test]
    fn hto_liberator_laser_uses_same_embedded_weapon_repair() {
        let interner = StringInterner::new();
        let mut record = make_record(
            "WEAP",
            0x85B654,
            "Output.esp",
            Some("HTO_crRobot_Liberator_LaserGun"),
            &interner,
        );
        let mut dnam = make_dnam_bytes(WEAP_ANIM_TYPE_GUN, 0.0, 10, 100, 20.0);
        dnam[WEAP_DNAM_FLAGS_OFFSET..WEAP_DNAM_FLAGS_OFFSET + 4]
            .copy_from_slice(&WEAP_FLAG_NPCS_USE_AMMO.to_le_bytes());
        push_bytes(&mut record, "DNAM", dnam);

        assert!(apply_weap(&mut record, 1, &interner));
        let flags = read_dnam_u32(&record, WEAP_DNAM_FLAGS_OFFSET).unwrap();
        assert_eq!(flags & WEAP_FLAG_EMBEDDED_WEAPON, WEAP_FLAG_EMBEDDED_WEAPON);
        assert_eq!(flags & WEAP_FLAG_NPCS_USE_AMMO, 0);
    }

    /// Spit-eid weapon with no DNAM still routes through ranged
    /// path via eid heuristic. (No DNAM → byte reads return None → no-ops;
    /// just verify we don't crash and EITM swap still runs if present.)
    #[test]
    fn weap_spit_eid_no_dnam_eitm_swap_still_runs() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "WEAP",
            0x0800,
            "Output.esp",
            Some("crSpitMirelurk"),
            &mut interner,
        );
        let raw: u32 = (1u32 << 24) | 0x000123;
        let mut eitm: Vec<u8> = Vec::new();
        eitm.extend_from_slice(&raw.to_le_bytes());
        push_bytes(&mut r, "EITM", eitm);
        let changed = apply_weap(&mut r, 1, &interner);
        assert!(changed, "spit-eid EITM swap should fire");
        let eitm_sig = SubrecordSig::from_str("EITM").unwrap();
        for entry in &r.fields {
            if entry.sig == eitm_sig {
                if let FieldValue::Bytes(data) = &entry.value {
                    let new_raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    assert_eq!(new_raw, WEAP_SPIT_VANILLA_EFFECT_RAW);
                }
            }
        }
    }

    /// Spit weapon with EITM pointing at Fallout4.esm (master 0)
    /// is NOT swapped (not mod-local).
    #[test]
    fn weap_spit_keeps_vanilla_eitm() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "WEAP",
            0x0800,
            "Output.esp",
            Some("crSpitMirelurk"),
            &mut interner,
        );
        let vanilla_raw: u32 = 0x00_00ABCD;
        let mut eitm: Vec<u8> = Vec::new();
        eitm.extend_from_slice(&vanilla_raw.to_le_bytes());
        push_bytes(&mut r, "EITM", eitm);
        let changed = apply_weap(&mut r, 1, &interner);
        assert!(changed, "the ranged keyword contract should be added");
        assert!(record_has_fo4_keyword(
            &r,
            CR_WEAPON_RANGED_LOW24,
            &interner
        ));
        let eitm_sig = SubrecordSig::from_str("EITM").unwrap();
        for entry in &r.fields {
            if entry.sig == eitm_sig {
                if let FieldValue::Bytes(data) = &entry.value {
                    let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    assert_eq!(raw, vanilla_raw);
                }
            }
        }
    }

    // ── EXPL: DATA.damage default + spit EITM ─────────────────────────────

    /// EXPL.DATA.damage = 0.0 gets bumped to 10.0.
    #[test]
    fn expl_data_damage_default() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "EXPL",
            0x1234,
            "Output.esp",
            Some("crExplFire"),
            &mut interner,
        );
        push_bytes(&mut r, "DATA", vec![0u8; 32]);
        let changed = apply_expl(&mut r, 1, &interner);
        assert!(changed);
        // Damage at offset 28..32 should now be 10.0.
        let data_sig = SubrecordSig::from_str("DATA").unwrap();
        for entry in &r.fields {
            if entry.sig == data_sig {
                if let FieldValue::Bytes(data) = &entry.value {
                    let dmg = f32::from_le_bytes([
                        data[EXPL_DATA_DAMAGE_OFFSET],
                        data[EXPL_DATA_DAMAGE_OFFSET + 1],
                        data[EXPL_DATA_DAMAGE_OFFSET + 2],
                        data[EXPL_DATA_DAMAGE_OFFSET + 3],
                    ]);
                    assert!((dmg - EXPL_DEFAULT_DAMAGE).abs() < 1e-6);
                }
            }
        }
    }

    /// EXPL with spit eid and no EITM gets crEnchMirelurkQueenSpit.
    #[test]
    fn expl_spit_eid_inserts_eitm() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "EXPL",
            0x1234,
            "Output.esp",
            Some("crSpitExpl"),
            &mut interner,
        );
        push_bytes(&mut r, "DATA", vec![0u8; 32]);
        let changed = apply_expl(&mut r, 1, &interner);
        assert!(changed);
        let eitm_sig = SubrecordSig::from_str("EITM").unwrap();
        let data_sig = SubrecordSig::from_str("DATA").unwrap();
        let eitm_idx = r.fields.iter().position(|e| e.sig == eitm_sig).unwrap();
        let data_idx = r.fields.iter().position(|e| e.sig == data_sig).unwrap();
        assert!(eitm_idx < data_idx, "EITM must precede DATA");
        if let FieldValue::Bytes(d) = &r.fields[eitm_idx].value {
            let raw = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
            assert_eq!(raw, EXPL_SPIT_ENCHANTMENT_RAW);
        } else {
            panic!("EITM should be bytes");
        }
    }

    /// EXPL without spit/barf eid does not add EITM.
    #[test]
    fn expl_non_spit_eid_skips_eitm() {
        let mut interner = StringInterner::new();
        let mut r = make_record(
            "EXPL",
            0x1234,
            "Output.esp",
            Some("crExplFire"),
            &mut interner,
        );
        // Pre-fill damage = 10.0 so apply_expl is a no-op on the damage path.
        let mut data = vec![0u8; 32];
        data[EXPL_DATA_DAMAGE_OFFSET..EXPL_DATA_DAMAGE_OFFSET + 4]
            .copy_from_slice(&10.0_f32.to_le_bytes());
        push_bytes(&mut r, "DATA", data);
        let changed = apply_expl(&mut r, 1, &interner);
        assert!(!changed);
        let eitm_sig = SubrecordSig::from_str("EITM").unwrap();
        assert!(r.fields.iter().all(|e| e.sig != eitm_sig));
    }

    // ── RACE.VNAM equipment-flags mask ────────────────────────────────────

    /// Scorchbeast-style RACE.VNAM keeps the high FO4 creature bits while
    /// unsupported low equipment slots remain filtered. `hand_to_hand_melee`
    /// goes with them: no vanilla FO4 creature race sets it, and carrying it
    /// across stops the engine ever staging a melee swing.
    #[test]
    fn race_vnam_preserves_scorchbeast_high_creature_bits_and_clears_h2h() {
        let mut interner = StringInterner::new();
        let mut r = make_record("RACE", 0x5678, "Output.esp", None, &mut interner);
        // Set bits: hand_to_hand_melee(1), one_hand_sword(2), gun(512),
        // grenade(1024), spell(4096), shield(8192), torch(16384), plus the high
        // mask vanilla FO4 creatures carry.
        let cur: u32 = 1 | 2 | 512 | 1024 | 4096 | 8192 | 16384 | 0xF8FF_8000;
        let mut vnam: Vec<u8> = Vec::new();
        vnam.extend_from_slice(&cur.to_le_bytes());
        push_bytes(&mut r, "VNAM", vnam);
        let changed = apply_race(&mut r, 1);
        assert!(changed);
        let vnam_sig = SubrecordSig::from_str("VNAM").unwrap();
        for entry in &r.fields {
            if entry.sig == vnam_sig {
                if let FieldValue::Bytes(data) = &entry.value {
                    let new_val = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    assert_eq!(new_val, 512 | 4096 | 8192 | 16384 | 0xF8FF_8000);
                    assert_eq!(new_val & 1, 0, "body-fighting creature loses H2H");
                }
            }
        }
    }

    /// Weapon-animation-type bits survive on a race that carries a weapon, even
    /// when FO76 tags it `ActorTypeCreature` (mole miners).
    #[test]
    fn race_vnam_preserves_weapon_bits_on_humanlike_race() {
        let mut interner = StringInterner::new();
        let mut r = make_record("RACE", 0x012E6B, "Output.esp", None, &mut interner);
        // MoleMinerRace: ActorTypeCreature *and* ActorTypeHumanlike.
        let mut kwda: Vec<u8> = Vec::new();
        kwda.extend_from_slice(&0x00_013795u32.to_le_bytes());
        kwda.extend_from_slice(&ACTOR_TYPE_HUMANLIKE_LOW24.to_le_bytes());
        push_bytes(&mut r, "KWDA", kwda);
        let cur: u32 = 1 | 2 | 4 | 8 | 16 | 32 | 64 | 128 | 256 | 512 | 1024 | 2048 | 0xF8FF_8000;
        let mut vnam: Vec<u8> = Vec::new();
        vnam.extend_from_slice(&cur.to_le_bytes());
        push_bytes(&mut r, "VNAM", vnam);

        assert!(!apply_race(&mut r, 1), "armed humanoid must not be masked");
        let vnam_sig = SubrecordSig::from_str("VNAM").unwrap();
        let stored = r
            .fields
            .iter()
            .find(|e| e.sig == vnam_sig)
            .and_then(|e| match &e.value {
                FieldValue::Bytes(d) if d.len() >= 4 => {
                    Some(u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
                }
                _ => None,
            })
            .expect("VNAM");
        assert_eq!(stored, cur, "source equipment flags survive verbatim");
    }

    /// Super mutants swing boards and super sledges but FO76 leaves them
    /// un-humanlike, so `ActorTypeSuperMutant` has to exempt them too.
    #[test]
    fn race_vnam_preserves_weapon_bits_on_super_mutant_race() {
        let mut interner = StringInterner::new();
        let mut r = make_record("RACE", 0x5C4F6E, "Output.esp", None, &mut interner);
        let mut kwda: Vec<u8> = Vec::new();
        kwda.extend_from_slice(&0x00_013795u32.to_le_bytes());
        kwda.extend_from_slice(&ACTOR_TYPE_SUPER_MUTANT_LOW24.to_le_bytes());
        push_bytes(&mut r, "KWDA", kwda);
        // ShieldedSuperMutantRace loses two_hand_sword/axe + grenade today.
        let cur: u32 = 1 | 32 | 64 | 512 | 1024 | 8192 | 16384 | 0xF8FF_8000;
        let mut vnam: Vec<u8> = Vec::new();
        vnam.extend_from_slice(&cur.to_le_bytes());
        push_bytes(&mut r, "VNAM", vnam);

        assert!(!apply_race(&mut r, 1));
    }

    /// A beast with the same `ActorTypeCreature` tag but no humanoid marker gets
    /// FO4's body-fighting profile: no `one_hand_sword`, and no
    /// `hand_to_hand_melee` either.
    #[test]
    fn race_vnam_still_masks_plain_creature_race() {
        let mut interner = StringInterner::new();
        let mut r = make_record("RACE", 0x822A4D, "Output.esp", None, &mut interner);
        let mut kwda: Vec<u8> = Vec::new();
        kwda.extend_from_slice(&0x00_013795u32.to_le_bytes());
        push_bytes(&mut r, "KWDA", kwda);
        let cur: u32 = 1 | 2 | 512 | 1024 | 8192 | 16384 | 0xF8FF_8000;
        let mut vnam: Vec<u8> = Vec::new();
        vnam.extend_from_slice(&cur.to_le_bytes());
        push_bytes(&mut r, "VNAM", vnam);

        assert!(apply_race(&mut r, 1));
        let vnam_sig = SubrecordSig::from_str("VNAM").unwrap();
        let stored = r
            .fields
            .iter()
            .find(|e| e.sig == vnam_sig)
            .and_then(|e| match &e.value {
                FieldValue::Bytes(d) if d.len() >= 4 => {
                    Some(u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
                }
                _ => None,
            })
            .expect("VNAM");
        assert_eq!(stored, 512 | 8192 | 16384 | 0xF8FF_8000);
        assert_eq!(stored & 1, 0, "RadHog must not keep H2H");
    }

    /// RACE.VNAM already filtered is a no-op.
    #[test]
    fn race_vnam_clean_is_noop() {
        let mut interner = StringInterner::new();
        let mut r = make_record("RACE", 0x5678, "Output.esp", None, &mut interner);
        let cur: u32 = 512 | 4096 | 8192 | 16384 | 0xF8FF_8000;
        let mut vnam: Vec<u8> = Vec::new();
        vnam.extend_from_slice(&cur.to_le_bytes());
        push_bytes(&mut r, "VNAM", vnam);
        let changed = apply_race(&mut r, 1);
        assert!(!changed);
    }

    // ── FACT.XNAM defaults ────────────────────────────────────────────────

    /// FACT with no XNAM gets 5 default Relations entries.
    #[test]
    fn fact_no_xnam_gets_defaults() {
        let mut interner = StringInterner::new();
        let mut r = make_record("FACT", 0x0BCDEF, "Output.esp", None, &mut interner);
        let changed = apply_fact(&mut r, 1);
        assert!(changed);
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        let xnam_count = r.fields.iter().filter(|e| e.sig == xnam_sig).count();
        assert_eq!(xnam_count, 5, "five default Relations should be added");
        // Verify the last entry is self (master_byte == target_own_index = 1).
        let last_xnam = r.fields.iter().rev().find(|e| e.sig == xnam_sig).unwrap();
        if let FieldValue::Bytes(d) = &last_xnam.value {
            let raw = u32::from_le_bytes([d[0], d[1], d[2], d[3]]);
            assert_eq!(
                raw >> 24,
                1,
                "self FK master byte must equal target_own_index"
            );
            assert_eq!(raw & 0x00FF_FFFF, 0x0BCDEF, "self FK object_id");
            let reaction = u32::from_le_bytes([d[8], d[9], d[10], d[11]]);
            assert_eq!(reaction, 2, "self entry should be Ally (2)");
        } else {
            panic!("XNAM should be bytes");
        }
    }

    /// FACT with null XNAM entries drops them and adds defaults.
    #[test]
    fn fact_drops_null_xnam_and_adds_defaults() {
        let mut interner = StringInterner::new();
        let mut r = make_record("FACT", 0x0BCDEF, "Output.esp", None, &mut interner);
        // Add one null-faction XNAM (raw 0).
        let mut null_xnam = vec![0u8; 12];
        // modifier = 0, reaction = 0 (all zero is fine for null entry)
        let _ = null_xnam.len();
        push_bytes(&mut r, "XNAM", null_xnam);
        let changed = apply_fact(&mut r, 1);
        assert!(changed);
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        let count = r.fields.iter().filter(|e| e.sig == xnam_sig).count();
        assert_eq!(count, 5, "null XNAM dropped + 5 defaults appended");
    }

    /// FACT with an existing non-null XNAM keeps it and does NOT
    /// add defaults.
    #[test]
    fn fact_keeps_existing_xnam_skips_defaults() {
        let mut interner = StringInterner::new();
        let mut r = make_record("FACT", 0x0BCDEF, "Output.esp", None, &mut interner);
        let existing_faction: u32 = 0x00_AABBCC;
        let mut xnam: Vec<u8> = Vec::new();
        xnam.extend_from_slice(&existing_faction.to_le_bytes());
        xnam.extend_from_slice(&0i32.to_le_bytes());
        xnam.extend_from_slice(&3u32.to_le_bytes());
        push_bytes(&mut r, "XNAM", xnam);
        let changed = apply_fact(&mut r, 1);
        assert!(!changed);
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        assert_eq!(r.fields.iter().filter(|e| e.sig == xnam_sig).count(), 1);
    }

    // ── QUST DNAM (General) defaults ──────────────────────────────────────

    /// QUST without DNAM gets default General flags + priority.
    #[test]
    fn qust_missing_dnam_gets_defaults() {
        let mut interner = StringInterner::new();
        let mut r = make_record("QUST", 0xABCDEF, "Output.esp", None, &mut interner);
        let changed = apply_qust(&mut r, 1);
        assert!(changed);
        let dnam_sig = SubrecordSig::from_str("DNAM").unwrap();
        let dnam = r
            .fields
            .iter()
            .find(|e| e.sig == dnam_sig)
            .expect("DNAM appended");
        if let FieldValue::Bytes(d) = &dnam.value {
            assert_eq!(d.len(), QUST_DNAM_LEN);
            let flags = u16::from_le_bytes([d[0], d[1]]);
            assert_eq!(flags, QUST_DEFAULT_GENERAL_FLAGS);
            assert_eq!(d[2], QUST_DEFAULT_GENERAL_PRIORITY);
        } else {
            panic!("DNAM should be bytes");
        }
    }

    /// QUST with existing DNAM is left alone.
    #[test]
    fn qust_existing_dnam_is_noop() {
        let mut interner = StringInterner::new();
        let mut r = make_record("QUST", 0xABCDEF, "Output.esp", None, &mut interner);
        push_bytes(&mut r, "DNAM", vec![0x77; QUST_DNAM_LEN]);
        let changed = apply_qust(&mut r, 1);
        assert!(!changed);
    }
}
