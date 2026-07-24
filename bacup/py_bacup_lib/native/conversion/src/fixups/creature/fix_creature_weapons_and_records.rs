//! Fixup: fix creature weapon stats, explosions, race flags, factions, and quests.
//!

//!
//! # What this does
//! For creature conversions (root sig NPC_ or LVLN), iterates the relevant
//! record sigs in the target plugin and applies per-sig binary mutations.
//! Every operation is a fixed-offset byte write, so the existing encoder
//! round-trips bytes verbatim.
//!
//! **WEAP** — creature weapon cleanup:
//! - **1a** Strip `AttackDelay >= 5` from melee weapons (DNAM byte 24..28
//!   set to 0.0).
//! - **1b** Ranged weapons missing `damage_base` / `accuracy_bonus` /
//!   `action_point_cost` get FO4 defaults (10 / 100 / 20) written at the
//!   fixed DNAM offsets.
//! - **1c** Liberator lasers are marked as embedded weapons and do not consume
//!   NPC inventory ammo, matching FO4 robot-integrated lasers.
//! - **1d** EITM (Effect) pointing at the converter's own mod gets swapped
//!   to vanilla `148D8F:Fallout4.esm` (MirelurkHunterSpitAttackSpell) on
//!   spit weapons.
//!
//! Deferred (documented inline):
//! - "ExtraData" removal — no FO4 subrecord with that name; YAML-only.
//! - "DamageTypes" curve-table cleanup — DAMA `array_struct:I,f` with
//!   form-version-conditional `curve_table` tail; deferred.
//!
//! **EXPL** — creature explosion cleanup:
//! - Ensure `DATA.damage` (byte 28..32, f32) is non-zero by writing 10.0
//!   when currently 0.0.
//! - Add `EITM` (Enchantment) = `crEnchMirelurkQueenSpit`
//!   (`115315:Fallout4.esm`) on spit/barf explosions missing it.
//!
//! Deferred:
//! - Top-level `Damage` / `ObjectEffect` field removal — these are YAML
//!   keys with no FO4 subrecord; nothing to remove in the binary layer.
//!
//! **RACE** — `VNAM` (Equipment Flags) is a `uint32 enum_ref=equip_type`
//! flag bitset. Preserve the FO4-valid upper equipment mask plus creature low
//! slots. Vanilla FO4 creatures such as Deathclaws and Molerats carry high
//! numeric flags here; stripping them leaves converted creatures under-specified.
//!
//! **FACT** — Drop XNAM entries whose `faction` FormID is null (raw 0).
//! When zero XNAM remain, append the FO4 default 5 entries:
//! - `03E0C8:Fallout4.esm` / Friend (3)   — CaptiveFaction
//! - `058305:Fallout4.esm` / Ally (2)     — SuperMutantFaction
//! - `0948B4:Fallout4.esm` / Ally (2)     — MutantHoundFaction
//! - `1E5F60:Fallout4.esm` / Friend (3)   — VertibirdFaction
//! - own record FK   / Ally (2)             — Self
//!
//! **QUST** — When DNAM ("General") subrecord is missing, append it with
//! flags = `start_game_enabled | starts_enabled | run_once |
//! exclude_from_dialogue_export` (= 0x0311) and priority = 5.
//!
//! # Subrecord layouts (FO4 schema)
//!
//! `WEAP.DNAM` codec
//! `struct:I,f,f,f,f,f,f,f,f,I,I,I,I,H,B,f,f,I,H,I,I,I,I,I,I,I,I,I,B,f,B,B,f,f,f,I,B,B,B,B`.
//! 132 bytes. Relevant offsets used here:
//! - 24..28 = `attack_delay` (f32)
//! - 54     = `animation_type` (u8; 1 = Melee, 9 = Gun)
//! - 67..69 = `damage_base` (u16)
//! - 105    = `accuracy_bonus` (u8)
//! - 112..116 = `action_point_cost` (f32)
//!
//! `WEAP.EITM` codec `formid` — 4 bytes, single FormID.
//!
//! `EXPL.DATA` codec
//! `struct:I,I,I,I,I,I,f,f,f,f,f,f,I,I,f,I,f,f,f,f,I` (kind
//! `parsed_with_raw_fallback`; trailing fields have `record_form_version`
//! presence conditions). Relevant fixed-prefix offsets:
//! - 24..28 = `force` (f32)
//! - 28..32 = `damage` (f32)
//!
//! `EXPL.EITM` codec `formid` — 4 bytes.
//!
//! `RACE.VNAM` codec `uint32 enum_ref=equip_type`. Equipment-type bit
//! values used:
//! - upper mask = `0xFFFF8000`
//! - `hand_to_hand_melee` = 1
//! - `spell`   = 4096
//! - `gun`     = 512
//! - `shield`  = 8192
//! - `torch`   = 16384
//!
//! `FACT.XNAM` codec `struct:I,i,I`, 12 bytes, `repeatable`. Per-entry:
//! - 0..4  = `faction` (FormID u32)
//! - 4..8  = `modifier` (i32; unused in defaults — left 0)
//! - 8..12 = `group_combat_reaction` (u32; 0=Neutral 1=Enemy 2=Ally 3=Friend)
//!
//! `QUST.DNAM` codec `struct:H,B,B,f,B,B,B,B`, 12 bytes.
//! - 0..2 = `flags` (u16; bits for `start_game_enabled` 1,
//!   `starts_enabled` 16, `run_once` 256, `exclude_from_dialogue_export` 512)
//! - 2    = `priority` (u8; default 5)
//! - rest = zeroed

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

/// equip_type bitmask: preserve FO4's upper biped/equipment flags plus common
/// creature low slots: `hand_to_hand_melee` (1), `spell` (4096), `gun` (512),
/// `shield` (8192), `torch` (16384).
const RACE_VNAM_KEEP_MASK: u32 = 0xFFFF_8000 | 1 | 512 | 4096 | 8192 | 16384;

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
            &["WEAP", "EXPL"]
        } else {
            &["WEAP", "EXPL", "RACE", "FACT", "QUST"]
        };

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
                        "EXPL" => eid_lower.contains("spit") || eid_lower.contains("barf"),
                        _ => false,
                    };
                    if !keep {
                        continue;
                    }
                }

                let changed = match *sig_str {
                    "WEAP" => apply_weap(&mut record, target_own_index, mapper.interner),
                    "EXPL" => apply_expl(&mut record, target_own_index, mapper.interner),
                    "RACE" => apply_race(&mut record, target_own_index),
                    "FACT" => apply_fact(&mut record, target_own_index),
                    "QUST" => apply_qust(&mut record, target_own_index),
                    _ => false,
                };
                if changed {
                    session
                        .replace_record(record, target_schema, mapper.interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    report.records_changed += 1;
                }
            }
        }

        Ok(report)
    }
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
        // 1a — Strip AttackDelay >= 5 from melee. Setting to 0.0 matches the
        // semantic "field removed" from the Python YAML perspective: the
        // value never gets read by the engine.
        if let Some(delay) = read_dnam_f32(record, WEAP_DNAM_ATTACK_DELAY_OFFSET) {
            if delay >= WEAP_MELEE_ATTACK_DELAY_THRESHOLD {
                if write_dnam_f32(record, WEAP_DNAM_ATTACK_DELAY_OFFSET, 0.0) {
                    changed = true;
                }
            }
        }
        // "ExtraData" removal — Python YAML key with no FO4 subrecord. Skip.
    } else {
        // 1b — Default damage_base / accuracy_bonus / action_point_cost
        // when the DNAM struct shows them as zero (Python's
        // `not data.get("DamageBase")` semantics).
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
                let repaired = (flags | WEAP_FLAG_EMBEDDED_WEAPON) & !WEAP_FLAG_NPCS_USE_AMMO;
                if repaired != flags && write_dnam_u32(record, WEAP_DNAM_FLAGS_OFFSET, repaired) {
                    changed = true;
                }
            }
        }
    }

    // 1d — Spit weapon EITM swap: if eid contains "spit" and the current
    // EITM FormID points into the converter's own plugin (master byte ==
    // target_own_index), rewrite to vanilla MirelurkHunterSpitAttackSpell.
    if eid_lower.contains("spit") {
        if swap_mod_local_eitm(record, target_own_index, WEAP_SPIT_VANILLA_EFFECT_RAW) {
            changed = true;
        }
    }

    changed
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
                // FieldValue::FormKey already encodes a FormKey; compare via
                // .local high byte. fk.local is the 24-bit object_id only in
                // the conversion FormKey shape — but the master byte is
                // implicit in fk.plugin. We can't safely identify "own
                // plugin" without comparing fk.plugin to the slot's plugin
                // name. Fall back to skipping FormKey-shaped EITMs; the
                // bytes-shaped path covers the read_record case.
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

    // For spit/barf explosions missing EITM, append one pointing at
    // crEnchMirelurkQueenSpit. Python places the field "before Data" in
    // YAML; in the binary record, subrecord order isn't validated for
    // round-trip equivalence (EITM appears before DATA in the FO4 schema
    // anyway), so we insert before DATA when possible, otherwise append.
    let eid_lower = resolve_eid_lower(record, interner);
    let is_spit_or_barf = eid_lower.contains("spit") || eid_lower.contains("barf");
    if is_spit_or_barf && !has_subrecord(record, "EITM") {
        if add_eitm_before_data(record, EXPL_SPIT_ENCHANTMENT_RAW) {
            changed = true;
        }
    }

    // YAML-only top-level Damage / ObjectEffect removal — no FO4 subrecord;
    // nothing to remove in the binary. Skip per deviation pattern.

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
pub fn apply_race(record: &mut Record, _target_own_index: u8) -> bool {
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
                let masked = cur & RACE_VNAM_KEEP_MASK;
                if cur != masked {
                    data[0..4].copy_from_slice(&masked.to_le_bytes());
                    changed = true;
                }
            }
            FieldValue::Uint(n) => {
                let cur = (*n & 0xFFFF_FFFF) as u32;
                let masked = cur & RACE_VNAM_KEEP_MASK;
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
        let changed = apply_weap(&mut r, 1, &interner);
        assert!(!changed);
        assert_eq!(read_dnam_u16(&r, WEAP_DNAM_DAMAGE_BASE_OFFSET), Some(42));
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
        assert!(!changed);
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

    /// Scorchbeast-style RACE.VNAM keeps melee and high FO4 creature bits while
    /// unsupported low equipment slots remain filtered.
    #[test]
    fn race_vnam_preserves_scorchbeast_melee_and_high_creature_bits() {
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
                    assert_eq!(new_val, 1 | 512 | 4096 | 8192 | 16384 | 0xF8FF_8000);
                }
            }
        }
    }

    /// RACE.VNAM already filtered is a no-op.
    #[test]
    fn race_vnam_clean_is_noop() {
        let mut interner = StringInterner::new();
        let mut r = make_record("RACE", 0x5678, "Output.esp", None, &mut interner);
        let cur: u32 = 1 | 512 | 4096 | 8192 | 16384 | 0xF8FF_8000;
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
