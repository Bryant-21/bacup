//! Fixup: fix FO76-specific issues in creature Race records.
//!
//! For creature conversions (root sig NPC_ or LVLN), each RACE gets:
//!
//! - `ATKD.attack_flags`: strip FO76-only bits, keep the FO4 attack-mode flags.
//! - `ATKD.attack_angle`: Backward 180°, Left -90°, Right 90° for directional
//!   attack events whose angle is 0.
//! - `ActorTypeAnimal` for `ActorTypeCreature` races that lost it, and
//!   `RagdollOnDeath` for races that borrow another creature's behavior project
//!   and have no death clip.
//! - `VNAM`: keep the FO4-valid high equipment flags, strip unsupported low bits.
//! - Floater MiddleHand and AltAttackSlot1 emissions routed through
//!   `ProjectileNode`.
//! - Male/Female `ANAM` skeleton and behavior `MODL` rows: the source creature
//!   row replaces target-game fallback rows, so native stacks (Sheepsquatch
//!   ranged/quill attacks) don't collapse to the Deathclaw fallback. A lone
//!   unbounded `.hkx` project row gets the FO4 `FNAM` boundary.
//! - 43 all-zero `PHWT` rows when `PHTN` exists without weights, as FO4 fan
//!   ports carry.
//! - A race with no movement defaults links its unique `<Actor>_Default_MT`
//!   (humanoid-shaped races such as Scorched too); flying and floating races
//!   also get `FLMV`.
//! - Target-keyword-only subgraph blocks are stripped. Core blocks with the base
//!   `Actors\<Creature>\Animations` path stay but lose their STKD gate, and
//!   AmbushBehavior furniture branches stay. Snallygaster's mobility keywords
//!   are normalized to the FO4 fan-port shape.
//! - Adjacent duplicate subgraph blocks collapse.
//! - Role-0 (MT) blocks get the creature's `Animations\H2H` path.
//! - `RACE.DATA` acceleration/deceleration is raised to FO4's floor.
//!
//! Shared `MTBehavior`, `MeleeBehavior`, and normalized gun subgraphs stay on
//! FO4's canonical Character behavior contracts.

use crate::fixups::creature::creature_predicate::{
    record_has_actor_type_creature, record_is_armed_humanoid,
};
use crate::fixups::creature::fix_creature_weapons_and_records::apply_race as fix_equipment_flags;
use crate::fixups::creature::{
    creature_internal_fixup_applies, race_internal_fixup_applies_to_record,
};
use crate::fixups::face::build_additive_race_record::parse_canonical_subgraphs;
use crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::{EditOutcome, PluginSession};
use crate::sym::{StringInterner, Sym};
use rustc_hash::FxHashMap;

// ---------------------------------------------------------------------------
// ATKD binary layout
// ---------------------------------------------------------------------------
//
// codec `struct:f,f,I,I,f,f,f,f,f,f,i` — 44 bytes total.
// | Offset | Size | Field              | Type |
// |--------|------|--------------------|------|
// |   0    |  4   | damage_mult        | f32  |
// |   4    |  4   | attack_chance      | f32  |
// |   8    |  4   | attack_spell       | u32  |
// |  12    |  4   | attack_flags       | u32  |
// |  16    |  4   | attack_angle       | f32  |
// |  20    |  4   | strike_angle       | f32  |
// |  24    |  4   | stagger            | f32  |
// |  28    |  4   | knockdown          | f32  |
// |  32    |  4   | recovery_time      | f32  |
// |  36    |  4   | action_points_mult | f32  |
// |  40    |  4   | stagger_offset     | i32  |

const ATKD_SIZE: usize = 44;
const ATKD_ATTACK_FLAGS_OFFSET: usize = 12;
const ATKD_ATTACK_ANGLE_OFFSET: usize = 16;

/// FO76-only attack flags absent from the vanilla FO4 creature attack corpus.
const ATKD_FO76_ONLY_BITS: u32 = 0x40 | 0x100 | 0x200 | 0x400;

/// FO4 `ActorTypeAnimal` keyword local FormID (`013798:Fallout4.esm`).
const ACTOR_TYPE_ANIMAL_LOW24: u32 = 0x00_013798;

/// FO4 `RagdollOnDeath` keyword local FormID (`24A03C:Fallout4.esm`).
const RAGDOLL_ON_DEATH_LOW24: u32 = 0x00_24A03C;

/// Races that must ragdoll straight from death instead of playing a borrowed
/// death clip, keyed by `race_actor_key`.
///
/// Vanilla FO4 grants `RagdollOnDeath` to exactly one race out of 84 —
/// `DLC04_RadAntRace` — and nothing in the corpus reads the keyword, so it is
/// an engine flag rather than a condition gate. FO4 ships no ant death
/// animation: the ant runs `Actors\RadRoach\RadRoachProject.hkx`, whose
/// `death1..3` clips are roach animations. Without the keyword the ant plays a
/// roach death on an ant skeleton, which reads in game as a stall before the
/// body drops. Keep this list as narrow as vanilla's.
const RAGDOLL_ON_DEATH_ACTOR_KEYS: &[&str] = &["radant"];

const MIDDLE_HAND_EQUIP_SLOT_LOW24: u32 = 0x01_A02F;
const ALT_ATTACK_SLOT_1_LOW24: u32 = 0x13_6255;
const FLOATER_PROJECTILE_NODE: &str = "ProjectileNode";

/// FO4's `RACE.DATA` bit for actors that use the native flight controller.
const RACE_FLAG_FLIES: u32 = 0x0000_0080;

/// FO4 creature fan ports carry 43 all-zero `PHWT` rows after the 16 `PHTN`
/// phoneme names. Each row is 16 f32 weights.
const DEFAULT_CREATURE_PHWT_ROWS: usize = 43;
const PHWT_ZERO_WEIGHT_SIZE: usize = 16 * 4;

// ---------------------------------------------------------------------------
// Directional-attack angle table (matches Python `directional_attack_angles`)
// ---------------------------------------------------------------------------

const DIRECTIONAL_ATTACK_ANGLES: &[(&str, f32)] =
    &[("backward", 180.0), ("left", -90.0), ("right", 90.0)];

fn directional_angle_for_event(event: &str) -> Option<f32> {
    if event.is_empty() {
        return None;
    }
    let lower = event.to_ascii_lowercase();
    for (direction, angle) in DIRECTIONAL_ATTACK_ANGLES {
        if lower.contains(direction) {
            return Some(*angle);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FixCreatureRaceRecordsFixup;

enum RaceRecordEdit {
    Replace {
        record: Record,
        dropped: u32,
        inferred_project_path: Option<String>,
    },
    PatchRawBehaviorProject {
        inferred_project_path: String,
    },
    Warn(String),
}

impl Fixup for FixCreatureRaceRecordsFixup {
    fn name(&self) -> &'static str {
        "fix_creature_race_records"
    }

    fn scope(&self) -> FixupScope {
        // Whole-plugin: self-gates per RACE on the creature keyword below.
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
        let race_sig =
            SigCode::from_str("RACE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        // Read the donor once, before the per-record walk takes the session.
        let power_armor_subgraphs =
            fo4_power_armor_subgraph_entries(session, target_schema, mapper, config);
        let interner = mapper.interner;
        let source_behaviors = crate::fo76_behaviors::enabled(session, config);
        let movement_types = build_default_movement_type_index(session, target_schema, interner)?;
        let mut dropped = 0u32;
        let mut warnings = Vec::new();
        let mut report = session.map_apply_by_sig(
            race_sig,
            mapper,
            |view, _snapshot, fk| match view.record_decoded(fk, target_schema, interner) {
                Ok(mut record) => {
                    let weapon_subgraphs_normalized = if source_behaviors {
                        0
                    } else {
                        normalize_fo76_weapon_subgraphs(&mut record, interner)
                    };
                    let movement_linked = link_missing_default_movement_types(
                        &mut record,
                        &movement_types,
                        interner,
                        target_schema,
                    );
                    // Schema-driven (needs byte offsets at FV-131), so it runs
                    // here rather than inside `apply_to_record`, and applies to
                    // every race — the FO4 movement floor is not creature-only.
                    let rates_raised =
                        raise_movement_rates_to_fo4_floor(&mut record, target_schema) > 0;
                    // Whole-plugin gate: the FO76-creature ATKD/skeletal/behavior/
                    // subgraph fixes skip ActorTypeNPC humanoid and generated
                    // additive races. No-op on a creature-graph walk.
                    if !race_internal_fixup_applies_to_record(&record, config, interner) {
                        // The humanoid gate keeps the FO76 creature fixes off
                        // HumanRace-family records, but an EXPLICIT project override still
                        // has to reach them — Scorched carry `ActorTypeNPC` and are exactly
                        // such a record. Only the override table is consulted here; the
                        // subgraph inference stays creature-only.
                        let override_project =
                            creature_project_path_override(&record, interner).map(str::to_string);
                        if weapon_subgraphs_normalized > 0 || movement_linked || rates_raised {
                            return Some(RaceRecordEdit::Replace {
                                record,
                                dropped: 0,
                                inferred_project_path: override_project,
                            });
                        }
                        return override_project.map(|inferred_project_path| {
                            RaceRecordEdit::PatchRawBehaviorProject {
                                inferred_project_path,
                            }
                        });
                    }
                    let inferred_project_path =
                        infer_creature_project_path_from_subgraphs(&record, interner);
                    let outcome = apply_to_record_with_source_behaviors(
                        &mut record,
                        interner,
                        source_behaviors,
                    );
                    // After `apply_to_record`, so the STKD-block stripper cannot eat the
                    // donor rows — FO4's power-armor blocks carry 189 STKD rows.
                    let power_armor_seeded = power_armor_subgraphs
                        .as_deref()
                        .map(|entries| {
                            seed_power_armor_locomotion_subgraphs(&mut record, entries, interner)
                        })
                        .unwrap_or(false);
                    if weapon_subgraphs_normalized > 0
                        || outcome.changed()
                        || movement_linked
                        || power_armor_seeded
                    {
                        Some(RaceRecordEdit::Replace {
                            record,
                            dropped: outcome.dropped,
                            inferred_project_path,
                        })
                    } else {
                        inferred_project_path.map(|inferred_project_path| {
                            RaceRecordEdit::PatchRawBehaviorProject {
                                inferred_project_path,
                            }
                        })
                    }
                }
                Err(err) => Some(RaceRecordEdit::Warn(format!(
                    "fix_creature_race_read:{err}"
                ))),
            },
            |session, mapper, _fk, edit| match edit {
                RaceRecordEdit::Replace {
                    record,
                    dropped: record_dropped,
                    inferred_project_path,
                } => {
                    let record_fk = record.form_key;
                    session
                        .replace_record_contents(record, target_schema, mapper.interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    if let Some(inferred_project_path) = inferred_project_path.as_deref() {
                        let _ = patch_raw_behavior_project_modls(
                            session,
                            &record_fk,
                            inferred_project_path,
                        )?;
                    }
                    dropped += record_dropped;
                    Ok(EditOutcome::Changed)
                }
                RaceRecordEdit::PatchRawBehaviorProject {
                    inferred_project_path,
                } => {
                    let changed =
                        patch_raw_behavior_project_modls(session, _fk, &inferred_project_path)?;
                    if changed > 0 {
                        Ok(EditOutcome::Changed)
                    } else {
                        Ok(EditOutcome::NoOp)
                    }
                }
                RaceRecordEdit::Warn(message) => {
                    warnings.push(mapper.interner.intern(&message));
                    Ok(EditOutcome::NoOp)
                }
            },
        )?;
        report.records_dropped += dropped;
        report.warnings.extend(warnings);
        Ok(report)
    }
}

#[derive(Clone, Copy)]
struct DefaultMovementType {
    form_key: FormKey,
    has_float_height: bool,
}

type DefaultMovementTypeIndex = FxHashMap<String, Option<DefaultMovementType>>;

fn build_default_movement_type_index(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> Result<DefaultMovementTypeIndex, FixupError> {
    let movt_sig = SigCode::from_str("MOVT").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let jnam_sig =
        SubrecordSig::from_str("JNAM").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut index = DefaultMovementTypeIndex::default();
    let movement_fks = session
        .form_keys_of_sig(movt_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in movement_fks {
        let Ok(record) = session.record_decoded(&fk, target_schema, interner) else {
            continue;
        };
        let Some(eid) = record.eid.and_then(|eid| interner.resolve(eid)) else {
            continue;
        };
        let Some(actor_key) = default_movement_actor_key(eid) else {
            continue;
        };
        let candidate = DefaultMovementType {
            form_key: fk,
            has_float_height: record.fields.iter().any(|entry| {
                entry.sig == jnam_sig
                    && matches!(entry.value, FieldValue::Float(height) if height != 0.0)
            }),
        };
        index
            .entry(actor_key)
            .and_modify(|entry| *entry = None)
            .or_insert(Some(candidate));
    }
    Ok(index)
}

fn default_movement_actor_key(eid: &str) -> Option<String> {
    normalize_editor_id(eid)
        .strip_suffix("defaultmt")
        .filter(|actor| !actor.is_empty())
        .map(str::to_string)
}

fn race_actor_key(record: &Record, interner: &StringInterner) -> Option<String> {
    let eid = record.eid.and_then(|eid| interner.resolve(eid))?;
    normalize_editor_id(eid)
        .strip_suffix("race")
        .filter(|actor| !actor.is_empty())
        .map(str::to_string)
}

fn normalize_editor_id(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn link_missing_default_movement_types(
    record: &mut Record,
    movement_types: &DefaultMovementTypeIndex,
    interner: &StringInterner,
    target_schema: &AuthoringSchema,
) -> bool {
    let Some(actor_key) = race_actor_key(record, interner) else {
        return false;
    };
    let Some(Some(candidate)) = movement_types.get(&actor_key) else {
        return false;
    };
    let Ok(wkmv_sig) = SubrecordSig::from_str("WKMV") else {
        return false;
    };
    let Ok(flvm_sig) = SubrecordSig::from_str("FLMV") else {
        return false;
    };
    let has_default = record.fields.iter().any(|entry| entry.sig == wkmv_sig);
    let has_fly = record.fields.iter().any(|entry| entry.sig == flvm_sig);
    let needs_fly = candidate.has_float_height || race_has_fly_flag(record, target_schema);
    if has_default && (has_fly || !needs_fly) {
        return false;
    }

    let tail_sigs = ["SGNM", "SAKD", "STKD", "SRAF", "PTOP", "NTOP", "QSTI"];
    let mut insert_at = record
        .fields
        .iter()
        .position(|entry| {
            entry.sig == flvm_sig || tail_sigs.iter().any(|sig| entry.sig.as_str() == *sig)
        })
        .unwrap_or(record.fields.len());
    if has_default {
        insert_at = record
            .fields
            .iter()
            .rposition(|entry| entry.sig == wkmv_sig)
            .map(|idx| idx + 1)
            .unwrap_or(insert_at);
    } else {
        record.fields.insert(
            insert_at,
            FieldEntry {
                sig: wkmv_sig,
                value: FieldValue::FormKey(candidate.form_key),
            },
        );
        insert_at += 1;
    }
    if needs_fly && !has_fly {
        record.fields.insert(
            insert_at,
            FieldEntry {
                sig: flvm_sig,
                value: FieldValue::FormKey(candidate.form_key),
            },
        );
    }
    true
}

/// Byte offset of a `RACE.DATA` field at FO4's target form version.
///
/// `source_read` emits `struct:` subrecords as raw bytes. The layout is
/// version-gated (200 bytes at FV-131 vs a 216-byte maximal layout), so a
/// version-less lookup skews every field past the first gate.
fn race_data_field_offset(target_schema: &AuthoringSchema, field_id: &str) -> Option<usize> {
    target_schema
        .struct_field_layout_versioned("RACE", "DATA", Some(FO4_TARGET_FORM_VERSION))
        .into_iter()
        .find(|field| field.field_id == field_id && field.width == 4)
        .map(|field| field.offset)
}

fn race_has_fly_flag(record: &Record, target_schema: &AuthoringSchema) -> bool {
    let Ok(data_sig) = SubrecordSig::from_str("DATA") else {
        return false;
    };
    let Some(offset) = race_data_field_offset(target_schema, "flags") else {
        return false;
    };
    record.fields.iter().any(|entry| {
        if entry.sig != data_sig {
            return false;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            return false;
        };
        bytes
            .get(offset..offset + 4)
            .and_then(|slot| <[u8; 4]>::try_from(slot).ok())
            .is_some_and(|slot| u32::from_le_bytes(slot) & RACE_FLAG_FLIES != 0)
    })
}

/// FO4's lowest authored `RACE.DATA` acceleration/deceleration rate.
///
/// Across all 107 FO4 + DLC races the minimum is `0.1` (`DLC04_CaveCricketRace`)
/// and the base-game minimum is `0.2`. `ScorchedRace` and `WendigoRace` chase
/// correctly in game at `0.2`, and a scorchbeast probe worked at `0.5`.
const FO4_MIN_MOVEMENT_RATE: f32 = 0.2;

/// Raise implausibly low FO76 movement rates to FO4's authored floor.
///
/// FO76 moved some creatures with server-side AI, so their `RACE.DATA`
/// acceleration/deceleration is inert (`ScorchBeastRace` ships `0.01`). FO4's
/// controller uses these fields, so such an actor turns to track its target but
/// never builds enough speed to walk. This is a floor, not a rescale: 131 of 134
/// converted races already sit in FO4's range. An explicit `0.0` reads as
/// deliberate "no acceleration" and is left alone.
fn raise_movement_rates_to_fo4_floor(record: &mut Record, target_schema: &AuthoringSchema) -> u32 {
    let Ok(data_sig) = SubrecordSig::from_str("DATA") else {
        return 0;
    };
    let offsets: Vec<usize> = ["acceleration_rate", "deceleration_rate"]
        .into_iter()
        .filter_map(|field_id| race_data_field_offset(target_schema, field_id))
        .collect();
    if offsets.is_empty() {
        return 0;
    }
    let mut raised = 0;
    for entry in record.fields.iter_mut() {
        if entry.sig != data_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        for &offset in &offsets {
            let Some(slot) = bytes.get_mut(offset..offset + 4) else {
                continue;
            };
            let rate = f32::from_le_bytes(<[u8; 4]>::try_from(&*slot).unwrap());
            if rate > 0.0 && rate < FO4_MIN_MOVEMENT_RATE {
                slot.copy_from_slice(&FO4_MIN_MOVEMENT_RATE.to_le_bytes());
                raised += 1;
            }
        }
    }
    raised
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Outcome of a single-record mutation.
#[derive(Debug, Default)]
pub struct RaceFixOutcome {
    pub atkd_flags_stripped: u32,
    pub atkd_angles_injected: u32,
    pub actor_type_animal_added: bool,
    pub ragdoll_on_death_added: bool,
    pub phoneme_weights_synthesized: u32,
    pub fallback_skeleton_promoted: bool,
    pub female_anam_fixed: bool,
    pub fallback_behavior_promoted: bool,
    pub fallback_runtime_bindings_fixed: bool,
    pub female_behavior_fixed: bool,
    pub equipment_flags_fixed: bool,
    pub floater_projectile_nodes_fixed: u32,
    pub subgraph_blocks_dropped: u32,
    pub subgraph_target_keywords_stripped: u32,
    pub subgraph_paths_normalized: u32,
    pub subgraph_keywords_normalized: u32,
    pub duplicate_graphs_collapsed: u32,
    pub locomotion_h2h_paths_added: u32,
    pub dropped: u32,
}

impl RaceFixOutcome {
    pub fn changed(&self) -> bool {
        self.atkd_flags_stripped > 0
            || self.atkd_angles_injected > 0
            || self.actor_type_animal_added
            || self.ragdoll_on_death_added
            || self.phoneme_weights_synthesized > 0
            || self.fallback_skeleton_promoted
            || self.female_anam_fixed
            || self.fallback_behavior_promoted
            || self.fallback_runtime_bindings_fixed
            || self.female_behavior_fixed
            || self.equipment_flags_fixed
            || self.floater_projectile_nodes_fixed > 0
            || self.subgraph_blocks_dropped > 0
            || self.subgraph_target_keywords_stripped > 0
            || self.subgraph_paths_normalized > 0
            || self.subgraph_keywords_normalized > 0
            || self.duplicate_graphs_collapsed > 0
            || self.locomotion_h2h_paths_added > 0
    }
}

/// Apply every creature-race fix to `record`. Returns a per-fix outcome
/// summary; callers consult `outcome.changed()` to know whether to write back.
pub fn apply_to_record(record: &mut Record, interner: &StringInterner) -> RaceFixOutcome {
    apply_to_record_with_source_behaviors(record, interner, false)
}

fn apply_to_record_with_source_behaviors(
    record: &mut Record,
    interner: &StringInterner,
    source_behaviors: bool,
) -> RaceFixOutcome {
    let mut outcome = RaceFixOutcome::default();

    fix_attack_data_flags(record, &mut outcome);
    fix_directional_attack_angles(record, interner, &mut outcome);
    outcome.actor_type_animal_added = append_actor_type_animal_keyword(record, interner);
    outcome.ragdoll_on_death_added = append_ragdoll_on_death_keyword(record, interner);
    outcome.equipment_flags_fixed = fix_equipment_flags(record, 0);
    fix_floater_projectile_equip_nodes(record, interner, &mut outcome);
    synthesize_missing_phoneme_weights(record, &mut outcome);
    let promoted_fallback = promote_sheepsquatch_deathclaw_skeletal_model(record, interner);
    outcome.fallback_skeleton_promoted = promoted_fallback;
    if !promoted_fallback {
        fix_female_skeletal_model(record, interner, &mut outcome);
    }

    if promoted_fallback {
        outcome.fallback_behavior_promoted =
            promote_sheepsquatch_deathclaw_behavior_graph(record, interner);
        fix_sheepsquatch_deathclaw_runtime_bindings(record, interner, &mut outcome);
        normalize_sheepsquatch_deathclaw_subgraph_paths(record, interner, &mut outcome);
    } else {
        fix_female_behavior_graph(record, interner, &mut outcome);
    }
    strip_subgraph_blocks_with_target_keywords(record, interner, &mut outcome);
    normalize_ambushhole_subgraph_paths(record, interner, &mut outcome);
    normalize_snallygaster_subgraph_keywords(record, interner, &mut outcome);
    collapse_adjacent_duplicate_subgraphs(record, interner, &mut outcome);
    // After the block-level strips/collapses, so it never adds a path to a
    // block that is about to be dropped.
    if !source_behaviors {
        add_h2h_path_to_locomotion_blocks(record, interner, &mut outcome);
    }

    outcome.dropped = outcome.subgraph_blocks_dropped + outcome.duplicate_graphs_collapsed;
    outcome
}

fn fix_floater_projectile_equip_nodes(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    if race_actor_key(record, interner).as_deref() != Some("floater") {
        return;
    }
    let (Ok(qnam_sig), Ok(znam_sig)) = (
        SubrecordSig::from_str("QNAM"),
        SubrecordSig::from_str("ZNAM"),
    ) else {
        return;
    };
    let projectile_node = interner.intern(FLOATER_PROJECTILE_NODE);
    let mut repair_next_node = false;
    for entry in &mut record.fields {
        if entry.sig == qnam_sig {
            repair_next_node = field_formid_low24(&entry.value).is_some_and(|local| {
                matches!(
                    local,
                    MIDDLE_HAND_EQUIP_SLOT_LOW24 | ALT_ATTACK_SLOT_1_LOW24
                )
            });
            continue;
        }
        if entry.sig != znam_sig {
            continue;
        }
        if repair_next_node {
            let changed = match &mut entry.value {
                FieldValue::String(current) if *current != projectile_node => {
                    *current = projectile_node;
                    true
                }
                FieldValue::Bytes(bytes) if bytes.as_slice() != b"ProjectileNode\0" => {
                    bytes.clear();
                    bytes.extend_from_slice(b"ProjectileNode\0");
                    true
                }
                _ => false,
            };
            outcome.floater_projectile_nodes_fixed += u32::from(changed);
        }
        repair_next_node = false;
    }
}

fn field_formid_low24(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::FormKey(fk) => Some(fk.local & 0x00FF_FFFF),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[..4].try_into().ok()?) & 0x00FF_FFFF)
        }
        FieldValue::Int(value) => Some((*value as u32) & 0x00FF_FFFF),
        FieldValue::Uint(value) => Some((*value as u32) & 0x00FF_FFFF),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// FO76 GunBehavior → FO4 WeaponBehavior subgraph contract
// ---------------------------------------------------------------------------

const ACTOR_MOBILITY_INJURED_RIGHT_LOW24: u32 = 0x030B00;
const ACTOR_MOBILITY_INJURED_LEFT_LOW24: u32 = 0x030B01;

/// Replace FO76 third-person gun-mode graphs with the FO4 weapon-mode
/// equivalents in FO4's canonical shared Character behavior set.
///
/// Returns the number of `SGNM` rows changed.
pub fn normalize_fo76_weapon_subgraphs(record: &mut Record, interner: &StringInterner) -> u32 {
    let blocks = parse_canonical_subgraphs(record);
    let sgnm_count = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "SGNM")
        .count();
    if blocks.len() != sgnm_count {
        return 0;
    }

    let replacements: Vec<Option<String>> = blocks
        .iter()
        .map(|block| {
            let path = interner.resolve(block.behaviour_graph)?;
            fo4_weapon_subgraph_path(path, &block.subgraph_keywords)
        })
        .collect();

    let mut block_index = 0usize;
    let mut changed = 0u32;
    for entry in &mut record.fields {
        if entry.sig.as_str() != "SGNM" {
            continue;
        }
        if let Some(replacement) = replacements.get(block_index).and_then(Option::as_deref) {
            entry.value = FieldValue::String(interner.intern(replacement));
            changed += 1;
        }
        block_index += 1;
    }
    changed
}

fn fo4_weapon_subgraph_path(path: &str, subgraph_keywords: &[FormKey]) -> Option<String> {
    let normalized = path.replace('/', "\\");
    let lowered = normalized.to_ascii_lowercase();
    if lowered.contains("\\_1stperson\\") {
        return None;
    }
    let (_, filename) = normalized.rsplit_once('\\')?;
    let target_filename = match filename.to_ascii_lowercase().as_str() {
        "gunbehavior.hkx"
        | "biggunwrappingbehavior.hkx"
        | "shouldermountedgunwrappingbehavior.hkx"
        | "boslauncherwrappingbehavior.hkx"
        | "binocularbehavior.hkx"
        | "binocularinjuredwrappingbehavior.hkx" => "WeaponBehavior.hkx",
        "nohandikgunwrappingbehavior.hkx" => "NoHandIKWeaponWrappingBehavior.hkx",
        "nohandikrelaxedgunwrappingbehavior.hkx" => "NoHandIKRelaxedWeaponWrappingBehavior.hkx",
        "chargeupwrappinggunbehavior.hkx" => "ChargeUpWrappingWeaponBehavior.hkx",
        "leginjuredgunwrappingbehavior.hkx" => {
            let injured_right = subgraph_keywords
                .iter()
                .any(|keyword| keyword.local & 0x00FF_FFFF == ACTOR_MOBILITY_INJURED_RIGHT_LOW24);
            let injured_left = subgraph_keywords
                .iter()
                .any(|keyword| keyword.local & 0x00FF_FFFF == ACTOR_MOBILITY_INJURED_LEFT_LOW24);
            match (injured_right, injured_left) {
                (true, true) => "BothArmInjuredWeaponWrappingBehavior.hkx",
                (true, false) => "RightArmInjuredWeaponWrappingBehavior.hkx",
                (false, true) => "LeftArmInjuredWeaponWrappingBehavior.hkx",
                (false, false) => "WeaponBehavior.hkx",
            }
        }
        _ => return None,
    };

    Some(format!("Actors\\Character\\Behaviors\\{target_filename}"))
}

// ---------------------------------------------------------------------------
// Fix 1a — strip confirmed FO76-only ATKD attack_flags
// ---------------------------------------------------------------------------

fn fix_attack_data_flags(record: &mut Record, outcome: &mut RaceFixOutcome) {
    let atkd_sig = match SubrecordSig::from_str("ATKD") {
        Ok(s) => s,
        Err(_) => return,
    };

    for entry in &mut record.fields {
        if entry.sig != atkd_sig {
            continue;
        }
        if let FieldValue::Bytes(ref mut data) = entry.value {
            if data.len() < ATKD_SIZE {
                continue;
            }
            let flags = u32::from_le_bytes([
                data[ATKD_ATTACK_FLAGS_OFFSET],
                data[ATKD_ATTACK_FLAGS_OFFSET + 1],
                data[ATKD_ATTACK_FLAGS_OFFSET + 2],
                data[ATKD_ATTACK_FLAGS_OFFSET + 3],
            ]);
            if flags & ATKD_FO76_ONLY_BITS != 0 {
                let cleaned = flags & !ATKD_FO76_ONLY_BITS;
                let bytes = cleaned.to_le_bytes();
                data[ATKD_ATTACK_FLAGS_OFFSET] = bytes[0];
                data[ATKD_ATTACK_FLAGS_OFFSET + 1] = bytes[1];
                data[ATKD_ATTACK_FLAGS_OFFSET + 2] = bytes[2];
                data[ATKD_ATTACK_FLAGS_OFFSET + 3] = bytes[3];
                outcome.atkd_flags_stripped += 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fix 1c — inject AttackAngle for directional attack events
// ---------------------------------------------------------------------------
//
// ATKD precedes its ATKE within an attack group (`scope_id='attacks'`); each
// ATKE sets the most recent ATKD's attack_angle if it is still 0.

fn fix_directional_attack_angles(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let atkd_sig = match SubrecordSig::from_str("ATKD") {
        Ok(s) => s,
        Err(_) => return,
    };
    let atke_sig = match SubrecordSig::from_str("ATKE") {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut pending_atkd_idx: Option<usize> = None;

    for i in 0..record.fields.len() {
        let sig = record.fields[i].sig;
        if sig == atkd_sig {
            pending_atkd_idx = Some(i);
            continue;
        }
        if sig != atke_sig {
            // Only ATKD updates pending_attack_data; other subrecords do NOT
            // reset it, so an ATKE later in the group still pairs.
            continue;
        }

        // ATKE: extract event name (zstring → FieldValue::String).
        let event_string = extract_zstring(&record.fields[i].value, interner);
        let Some(event) = event_string else {
            pending_atkd_idx = None;
            continue;
        };

        let Some(idx) = pending_atkd_idx else {
            continue;
        };

        if let Some(angle) = directional_angle_for_event(&event) {
            if try_set_attack_angle(&mut record.fields[idx], angle) {
                outcome.atkd_angles_injected += 1;
            }
        }

        pending_atkd_idx = None;
    }
}

/// Extract the underlying string from a `FieldValue::String` (`Sym`-backed).
fn extract_zstring(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_string()),
        _ => None,
    }
}

/// Set the ATKD `attack_angle` field if it is currently 0. Returns `true` when
/// written.
fn try_set_attack_angle(entry: &mut FieldEntry, angle: f32) -> bool {
    let FieldValue::Bytes(ref mut data) = entry.value else {
        return false;
    };
    if data.len() < ATKD_SIZE {
        return false;
    }
    let current = f32::from_le_bytes([
        data[ATKD_ATTACK_ANGLE_OFFSET],
        data[ATKD_ATTACK_ANGLE_OFFSET + 1],
        data[ATKD_ATTACK_ANGLE_OFFSET + 2],
        data[ATKD_ATTACK_ANGLE_OFFSET + 3],
    ]);
    if current != 0.0 {
        return false;
    }
    let bytes = angle.to_le_bytes();
    data[ATKD_ATTACK_ANGLE_OFFSET] = bytes[0];
    data[ATKD_ATTACK_ANGLE_OFFSET + 1] = bytes[1];
    data[ATKD_ATTACK_ANGLE_OFFSET + 2] = bytes[2];
    data[ATKD_ATTACK_ANGLE_OFFSET + 3] = bytes[3];
    true
}

// ---------------------------------------------------------------------------
// Fix 1d — add ActorTypeAnimal when a creature race lost that FO4 class
// ---------------------------------------------------------------------------

fn append_actor_type_animal_keyword(record: &mut Record, interner: &StringInterner) -> bool {
    // FO76 hands `ActorTypeCreature` to mole miners, super mutants and Zetans as
    // readily as to beasts, but `ActorTypeAnimal` drives Animal Friend targeting
    // — vanilla FO4 grants it to exactly ten races, every one a real animal
    // (brahmin, cat, dog, gorilla, molerat, radstag, yao guai, ...) and never to
    // anything humanoid.
    if record_is_armed_humanoid(record) {
        return false;
    }
    if !record_has_actor_type_creature(record) {
        return false;
    }

    append_fallout4_keyword(record, interner, ACTOR_TYPE_ANIMAL_LOW24)
}

// ---------------------------------------------------------------------------
// Fix 1d2 — force ragdoll death on races that borrow another creature's graph
// ---------------------------------------------------------------------------

fn append_ragdoll_on_death_keyword(record: &mut Record, interner: &StringInterner) -> bool {
    let Some(actor_key) = race_actor_key(record, interner) else {
        return false;
    };
    if !RAGDOLL_ON_DEATH_ACTOR_KEYS.contains(&actor_key.as_str()) {
        return false;
    }

    append_fallout4_keyword(record, interner, RAGDOLL_ON_DEATH_LOW24)
}

/// Append `low24` as a `Fallout4.esm` keyword to `record`'s KWDA and resync
/// KSIZ. No-op when the keyword is already present or the record has no KWDA.
fn append_fallout4_keyword(record: &mut Record, interner: &StringInterner, low24: u32) -> bool {
    if record_has_keyword_low24(record, low24) {
        return false;
    }

    let kwda_sig = match SubrecordSig::from_str("KWDA") {
        Ok(sig) => sig,
        Err(_) => return false,
    };

    let Some(kwda_idx) = record.fields.iter().position(|entry| entry.sig == kwda_sig) else {
        return false;
    };

    let keyword_fk = FormKey {
        local: low24,
        plugin: interner.intern(FALLOUT4_ESM),
    };

    let kwda_value = &mut record.fields[kwda_idx].value;
    match kwda_value {
        FieldValue::Bytes(data) => {
            data.extend_from_slice(&low24.to_le_bytes());
        }
        FieldValue::List(items) => {
            items.push(FieldValue::FormKey(keyword_fk));
        }
        FieldValue::FormKey(_) => {
            let existing = std::mem::replace(kwda_value, FieldValue::None);
            *kwda_value = FieldValue::List(vec![existing, FieldValue::FormKey(keyword_fk)]);
        }
        _ => return false,
    }

    sync_keyword_count(record);
    true
}

fn record_has_keyword_low24(record: &Record, wanted_low24: u32) -> bool {
    let kwda_sig = match SubrecordSig::from_str("KWDA") {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    record.fields.iter().any(|entry| {
        if entry.sig != kwda_sig {
            return false;
        }
        field_value_has_keyword_low24(&entry.value, wanted_low24)
    })
}

fn field_value_has_keyword_low24(value: &FieldValue, wanted_low24: u32) -> bool {
    match value {
        FieldValue::List(items) => items
            .iter()
            .any(|item| field_value_has_keyword_low24(item, wanted_low24)),
        FieldValue::FormKey(fk) => (fk.local & 0x00FF_FFFF) == wanted_low24,
        FieldValue::Bytes(data) => data.chunks_exact(4).any(|chunk| {
            let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            (raw & 0x00FF_FFFF) == wanted_low24
        }),
        _ => false,
    }
}

fn sync_keyword_count(record: &mut Record) {
    let kwda_sig = match SubrecordSig::from_str("KWDA") {
        Ok(sig) => sig,
        Err(_) => return,
    };
    let ksiz_sig = match SubrecordSig::from_str("KSIZ") {
        Ok(sig) => sig,
        Err(_) => return,
    };
    let count = record
        .fields
        .iter()
        .filter(|entry| entry.sig == kwda_sig)
        .map(|entry| keyword_value_count(&entry.value))
        .sum::<u32>();

    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == ksiz_sig) {
        write_u32_field_value(&mut entry.value, count);
        return;
    }

    let value = FieldValue::Bytes(smallvec::SmallVec::from_slice(&count.to_le_bytes()));
    let entry = FieldEntry {
        sig: ksiz_sig,
        value,
    };
    let insert_idx = record
        .fields
        .iter()
        .position(|field| field.sig == kwda_sig)
        .unwrap_or(record.fields.len());
    record.fields.insert(insert_idx, entry);
}

fn keyword_value_count(value: &FieldValue) -> u32 {
    match value {
        FieldValue::List(items) => items.len() as u32,
        FieldValue::FormKey(_) => 1,
        FieldValue::Bytes(data) => (data.len() / 4) as u32,
        _ => 0,
    }
}

fn write_u32_field_value(value: &mut FieldValue, n: u32) {
    match value {
        FieldValue::Bytes(data) => {
            data.clear();
            data.extend_from_slice(&n.to_le_bytes());
        }
        FieldValue::Int(current) => *current = i64::from(n),
        FieldValue::Uint(current) => *current = u64::from(n),
        _ => *value = FieldValue::Bytes(smallvec::SmallVec::from_slice(&n.to_le_bytes())),
    }
}

// ---------------------------------------------------------------------------
// Fix 3 — synthesize zeroed PHWT rows when PHTN exists without weights
// ---------------------------------------------------------------------------

fn synthesize_missing_phoneme_weights(record: &mut Record, outcome: &mut RaceFixOutcome) {
    let phtn_sig = match SubrecordSig::from_str("PHTN") {
        Ok(sig) => sig,
        Err(_) => return,
    };
    let phwt_sig = match SubrecordSig::from_str("PHWT") {
        Ok(sig) => sig,
        Err(_) => return,
    };

    if !record.fields.iter().any(|entry| entry.sig == phtn_sig) {
        return;
    }

    let existing = record
        .fields
        .iter()
        .filter(|entry| entry.sig == phwt_sig)
        .count();
    if existing >= DEFAULT_CREATURE_PHWT_ROWS {
        return;
    }

    let insert_after = record
        .fields
        .iter()
        .rposition(|entry| entry.sig == phwt_sig)
        .or_else(|| {
            record
                .fields
                .iter()
                .rposition(|entry| entry.sig == phtn_sig)
        });
    let Some(insert_after) = insert_after else {
        return;
    };

    let missing = DEFAULT_CREATURE_PHWT_ROWS - existing;
    for offset in 0..missing {
        record
            .fields
            .insert(insert_after + 1 + offset, zero_phwt_entry(phwt_sig));
    }
    outcome.phoneme_weights_synthesized += missing as u32;
}

fn zero_phwt_entry(sig: SubrecordSig) -> FieldEntry {
    let mut bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
    bytes.resize(PHWT_ZERO_WEIGHT_SIZE, 0);
    FieldEntry {
        sig,
        value: FieldValue::Bytes(bytes),
    }
}

// ---------------------------------------------------------------------------
// Fix 2 override — preserve native Sheepsquatch runtime
// ---------------------------------------------------------------------------
//
// The working fan ESP uses a full Deathclaw fallback, which is stable but loses
// the FO76 Sheepsquatch ranged/quill attack events. Keep that fallback disabled
// here so the generic row repair preserves the source Sheepsquatch runtime.

fn promote_sheepsquatch_deathclaw_skeletal_model(
    _record: &mut Record,
    _interner: &StringInterner,
) -> bool {
    // The fan port proves the Deathclaw fallback can move, but it drops native
    // Sheepsquatch attack events. Preserve the source Sheepsquatch stack and let
    // the generic male-to-female repair below make the rows internally
    // consistent.
    false
}

fn promote_sheepsquatch_deathclaw_behavior_graph(
    record: &mut Record,
    interner: &StringInterner,
) -> bool {
    let mut changed = false;
    let mnam_sig = match SubrecordSig::from_str("MNAM") {
        Ok(s) => s,
        Err(_) => return promote_nested_sheepsquatch_deathclaw_behavior(record, interner),
    };
    let fnam_sig = match SubrecordSig::from_str("FNAM") {
        Ok(s) => s,
        Err(_) => return promote_nested_sheepsquatch_deathclaw_behavior(record, interner),
    };
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(s) => s,
        Err(_) => return promote_nested_sheepsquatch_deathclaw_behavior(record, interner),
    };

    if let Some((male_idx, female_idx)) =
        locate_second_male_female_pair(record, mnam_sig, fnam_sig, modl_sig)
    {
        if let Some((male_path, female_path)) =
            string_pair_from_indices(record, male_idx, female_idx, interner)
        {
            if is_sheepsquatch_deathclaw_runtime_pair(&male_path, &female_path, ".hkx") {
                changed |= copy_zstring_if_differs(record, female_idx, male_idx, interner);
            }
        }
    }

    changed | promote_nested_sheepsquatch_deathclaw_behavior(record, interner)
}

fn string_pair_from_indices(
    record: &Record,
    male_idx: usize,
    female_idx: usize,
    interner: &StringInterner,
) -> Option<(String, String)> {
    let FieldValue::String(male_sym) = record.fields[male_idx].value else {
        return None;
    };
    let FieldValue::String(female_sym) = record.fields[female_idx].value else {
        return None;
    };
    Some((
        interner.resolve(male_sym)?.to_string(),
        interner.resolve(female_sym)?.to_string(),
    ))
}

fn is_sheepsquatch_deathclaw_runtime_pair(
    male_path: &str,
    female_path: &str,
    suffix: &str,
) -> bool {
    let male = normalize_runtime_path(male_path);
    let female = normalize_runtime_path(female_path);
    !male.is_empty()
        && !female.is_empty()
        && male.ends_with(suffix)
        && female.ends_with(suffix)
        && male.starts_with("actors/sheepsquatch/")
        && female.starts_with("actors/deathclaw/")
}

fn normalize_runtime_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

const DEATHCLAW_BEHAVIOR_PROJECT: &str = "Actors\\Deathclaw\\DeathclawProject.hkx";
/// The project `XPD_JerseyDevilRace` runs on, spelled as FO76 spells it — no `Project`
/// suffix. Also the retarget target for the Lesser Devil; see
/// `creature_project_path_override`.
const JERSEY_DEVIL_BEHAVIOR_PROJECT: &str = "Actors\\JerseyDevil\\JerseyDevil.hkx";
const CAT_PET_BEHAVIOR_PROJECT: &str = "Actors\\Cat_Pet\\Cat_PetProject.hkx";
/// FO4's own human behavior project, spelled exactly as vanilla `HumanRace` spells it.
/// The retarget target for an FO76 humanoid whose converted project chain does not drive
/// FO4's human rig correctly.
const HUMAN_BEHAVIOR_PROJECT: &str = "actors\\Character\\RaiderProject.hkx";
const POWER_ARMOR_BEHAVIOR_PROJECT: &str = "Actors\\PowerArmor\\PowerArmorProject.hkx";
/// FO4's own `PowerArmorRace`, rendered `OBJID:Plugin`. Donor for the locomotion subgraph
/// blocks a converted race needs once it falls back to FO4's power-armor chain.
const FO4_POWER_ARMOR_RACE: &str = "01D31E:Fallout4.esm";
const DEATHCLAW_BEHAVIOR_GRAPH: &str = "Actors\\Deathclaw\\Behaviors\\DeathclawEverything.hkx";
const DEATHCLAW_DEFAULT_MOVT_LOCAL: u32 = 0x01E1EE;
const DEATHCLAW_UNARMED_WEAPON_LOCAL: u32 = 0x0C2C2A;
const FALLOUT4_ESM: &str = "Fallout4.esm";
const DEATHCLAW_ATTACK_EVENTS: &[&str] = &[
    "MeleeAttackStartFlipCar",
    "meleeattackStartLeft",
    "meleeAttackStartLeftSideSwipeStart",
    "meleeAttackStartPowerComboTailStart",
    "meleeAttackStartPowerForwardStart",
    "meleeAttackStartPowerHornRam",
    "meleeAttackStartPowerJumpSlash",
    "meleeAttackStartRightSideSwipeStart",
    "meleeAttackStartRight",
];

fn fix_sheepsquatch_deathclaw_runtime_bindings(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let mut changed = false;
    changed |= set_formkey_subrecord(
        record,
        "WKMV",
        fo4_form_key(DEATHCLAW_DEFAULT_MOVT_LOCAL, interner),
    );
    changed |= set_formkey_subrecord(
        record,
        "UNWP",
        fo4_form_key(DEATHCLAW_UNARMED_WEAPON_LOCAL, interner),
    );
    changed |= normalize_deathclaw_attack_events(record, interner) > 0;
    outcome.fallback_runtime_bindings_fixed = changed;
}

fn fo4_form_key(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        local,
        plugin: interner.intern(FALLOUT4_ESM),
    }
}

fn set_formkey_subrecord(record: &mut Record, sig_str: &str, replacement: FormKey) -> bool {
    let sig = match SubrecordSig::from_str(sig_str) {
        Ok(sig) => sig,
        Err(_) => return false,
    };
    for entry in &mut record.fields {
        if entry.sig != sig {
            continue;
        }
        if matches!(&entry.value, FieldValue::FormKey(current) if *current == replacement) {
            return false;
        }
        entry.value = FieldValue::FormKey(replacement);
        return true;
    }
    false
}

fn normalize_deathclaw_attack_events(record: &mut Record, interner: &StringInterner) -> u32 {
    let atke_sig = match SubrecordSig::from_str("ATKE") {
        Ok(sig) => sig,
        Err(_) => return 0,
    };
    let mut attack_index = 0usize;
    let mut changed = 0u32;

    for entry in &mut record.fields {
        if entry.sig != atke_sig {
            continue;
        }
        let replacement = DEATHCLAW_ATTACK_EVENTS[attack_index % DEATHCLAW_ATTACK_EVENTS.len()];
        attack_index += 1;

        if matches!(&entry.value, FieldValue::String(sym) if interner.resolve(*sym) == Some(replacement))
        {
            continue;
        }
        entry.value = FieldValue::String(interner.intern(replacement));
        changed += 1;
    }

    changed
}

// ---------------------------------------------------------------------------
// Fix 2a — copy Male ANAM (Skeletal Model) onto Female ANAM if they differ
// ---------------------------------------------------------------------------
//
// Schema layout (per RACE.subrecords):
//   MNAM (Male Marker, empty)
//   ANAM (Male Skeletal Model, zstring)
//   MODT (Male Model Information, bytes)
//   FNAM (Female Marker, empty)
//   ANAM (Female Skeletal Model, zstring)
//   MODT (Female Model Information, bytes)
//
// We locate the first ANAM after the first MNAM (= Male) and the first ANAM
// after the first FNAM (= Female), then copy Male zstring onto Female if both
// exist and differ. Same pattern applies to behavior graph MODL pairs below.

fn fix_female_skeletal_model(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let mnam_sig = match SubrecordSig::from_str("MNAM") {
        Ok(s) => s,
        Err(_) => return,
    };
    let fnam_sig = match SubrecordSig::from_str("FNAM") {
        Ok(s) => s,
        Err(_) => return,
    };
    let anam_sig = match SubrecordSig::from_str("ANAM") {
        Ok(s) => s,
        Err(_) => return,
    };

    if let Some((male_idx, female_idx)) =
        locate_first_male_female_pair(record, mnam_sig, fnam_sig, anam_sig)
    {
        if copy_zstring_if_differs(record, male_idx, female_idx, interner) {
            outcome.female_anam_fixed = true;
        }
    }
}

fn fix_female_behavior_graph(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    // Behavior graph block also uses MNAM/FNAM markers but with MODL subrecords.
    // Per the FO4 schema there are TWO sets of MNAM/FNAM blocks: skeletal model
    // (with ANAM) and behavior graph (with MODL). The skeletal-model markers
    // come first; the behavior-graph markers come second. We process the
    // SECOND MNAM/FNAM pair here for MODL.
    let mnam_sig = match SubrecordSig::from_str("MNAM") {
        Ok(s) => s,
        Err(_) => return,
    };
    let fnam_sig = match SubrecordSig::from_str("FNAM") {
        Ok(s) => s,
        Err(_) => return,
    };
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(s) => s,
        Err(_) => return,
    };

    if let Some((male_idx, female_idx)) =
        locate_second_male_female_pair(record, mnam_sig, fnam_sig, modl_sig)
    {
        if copy_zstring_if_differs(record, male_idx, female_idx, interner) {
            outcome.female_behavior_fixed = true;
        }
    }

    let inferred_project_path = infer_creature_project_path_from_subgraphs(record, interner);
    if copy_nested_behavior_bodydata(record, interner, inferred_project_path.as_deref()) {
        outcome.female_behavior_fixed = true;
    }
    if ensure_nested_behavior_bodydata_female_marker(record, interner) {
        outcome.female_behavior_fixed = true;
    }
}

/// Return `(male_payload_idx, female_payload_idx)` for the FIRST MNAM/FNAM
/// pair, where `payload_sig` is the data subrecord immediately following each
/// marker (e.g. ANAM after MNAM/FNAM for skeletal model).
fn locate_first_male_female_pair(
    record: &Record,
    male_marker: SubrecordSig,
    female_marker: SubrecordSig,
    payload_sig: SubrecordSig,
) -> Option<(usize, usize)> {
    let mut male_payload: Option<usize> = None;
    let mut female_payload: Option<usize> = None;
    let mut state = MarkerScan::SeekMale;
    for (i, entry) in record.fields.iter().enumerate() {
        match state {
            MarkerScan::SeekMale => {
                if entry.sig == male_marker {
                    state = MarkerScan::CollectMalePayload;
                }
            }
            MarkerScan::CollectMalePayload => {
                if entry.sig == payload_sig {
                    male_payload = Some(i);
                    state = MarkerScan::SeekFemale;
                } else if entry.sig == female_marker {
                    // Male marker without an ANAM/MODL between it and FNAM →
                    // no male payload available; bail.
                    return None;
                }
            }
            MarkerScan::SeekFemale => {
                if entry.sig == female_marker {
                    state = MarkerScan::CollectFemalePayload;
                }
            }
            MarkerScan::CollectFemalePayload => {
                if entry.sig == payload_sig {
                    female_payload = Some(i);
                    break;
                }
            }
        }
    }
    male_payload.zip(female_payload)
}

/// Return `(male_payload_idx, female_payload_idx)` for the SECOND MNAM/FNAM
/// pair (the behavior-graph block). Skips the first MNAM/FNAM pair entirely.
fn locate_second_male_female_pair(
    record: &Record,
    male_marker: SubrecordSig,
    female_marker: SubrecordSig,
    payload_sig: SubrecordSig,
) -> Option<(usize, usize)> {
    // Find indices of the FIRST male and female markers, then start scanning
    // for the second pair after the first female marker.
    let mut first_female_idx: Option<usize> = None;
    let mut first_male_seen = false;
    for (i, entry) in record.fields.iter().enumerate() {
        if !first_male_seen && entry.sig == male_marker {
            first_male_seen = true;
            continue;
        }
        if first_male_seen && entry.sig == female_marker {
            first_female_idx = Some(i);
            break;
        }
    }
    let start = first_female_idx? + 1;
    if start >= record.fields.len() {
        return None;
    }

    let sliced = &record.fields[start..];
    let mut male_payload: Option<usize> = None;
    let mut female_payload: Option<usize> = None;
    let mut state = MarkerScan::SeekMale;
    for (j, entry) in sliced.iter().enumerate() {
        let i = start + j;
        match state {
            MarkerScan::SeekMale => {
                if entry.sig == male_marker {
                    state = MarkerScan::CollectMalePayload;
                }
            }
            MarkerScan::CollectMalePayload => {
                if entry.sig == payload_sig {
                    male_payload = Some(i);
                    state = MarkerScan::SeekFemale;
                } else if entry.sig == female_marker {
                    return None;
                }
            }
            MarkerScan::SeekFemale => {
                if entry.sig == female_marker {
                    state = MarkerScan::CollectFemalePayload;
                }
            }
            MarkerScan::CollectFemalePayload => {
                if entry.sig == payload_sig {
                    female_payload = Some(i);
                    break;
                }
            }
        }
    }
    male_payload.zip(female_payload)
}

#[derive(Copy, Clone)]
enum MarkerScan {
    SeekMale,
    CollectMalePayload,
    SeekFemale,
    CollectFemalePayload,
}

/// Copy the zstring (Sym) at `src_idx` onto the entry at `dst_idx` if they
/// differ. Returns `true` when a write occurred.
fn copy_zstring_if_differs(
    record: &mut Record,
    src_idx: usize,
    dst_idx: usize,
    interner: &StringInterner,
) -> bool {
    let src_sym = match record.fields[src_idx].value {
        FieldValue::String(s) => s,
        _ => return false,
    };
    let dst_sym = match record.fields[dst_idx].value {
        FieldValue::String(s) => s,
        _ => return false,
    };
    if src_sym == dst_sym {
        return false;
    }
    // Both paths must be non-empty.
    let male_empty = interner.resolve(src_sym).map(str::is_empty).unwrap_or(true);
    let female_empty = interner.resolve(dst_sym).map(str::is_empty).unwrap_or(true);
    if male_empty || female_empty {
        return false;
    }
    record.fields[dst_idx].value = FieldValue::String(src_sym);
    true
}

fn copy_nested_behavior_bodydata(
    record: &mut Record,
    interner: &StringInterner,
    inferred_project_path: Option<&str>,
) -> bool {
    let mut changed = false;
    for entry in record.fields.iter_mut() {
        changed |=
            copy_nested_behavior_bodydata_value(&mut entry.value, interner, inferred_project_path);
    }
    changed
}

fn ensure_nested_behavior_bodydata_female_marker(
    record: &mut Record,
    interner: &StringInterner,
) -> bool {
    let mut changed = false;
    for entry in record.fields.iter_mut() {
        changed |= ensure_behavior_bodydata_female_marker_value(&mut entry.value, interner);
    }
    changed
}

fn ensure_behavior_bodydata_female_marker_value(
    value: &mut FieldValue,
    interner: &StringInterner,
) -> bool {
    match value {
        FieldValue::List(items) => {
            if ensure_behavior_bodydata_list_female_marker(items, interner) {
                return true;
            }
            items
                .iter_mut()
                .any(|child| ensure_behavior_bodydata_female_marker_value(child, interner))
        }
        FieldValue::Struct(fields) => fields
            .iter_mut()
            .any(|(_, child)| ensure_behavior_bodydata_female_marker_value(child, interner)),
        _ => false,
    }
}

fn ensure_behavior_bodydata_list_female_marker(
    items: &mut [FieldValue],
    interner: &StringInterner,
) -> bool {
    let fnam = interner.intern("FNAM");
    for item in items {
        let Some((_, path, _)) = struct_model_path_field(item, interner) else {
            continue;
        };
        if !path.to_ascii_lowercase().ends_with(".hkx") {
            continue;
        }
        let FieldValue::Struct(fields) = item else {
            return false;
        };
        if fields.iter().any(|(name, _)| *name == fnam) {
            return false;
        }
        fields.push((fnam, FieldValue::Bool(true)));
        return true;
    }
    false
}

fn copy_nested_behavior_bodydata_value(
    value: &mut FieldValue,
    interner: &StringInterner,
    inferred_project_path: Option<&str>,
) -> bool {
    match value {
        FieldValue::List(items) => {
            let changed = copy_behavior_bodydata_list(items, interner, inferred_project_path);
            if changed {
                return true;
            }
            let mut changed = false;
            for child in items.iter_mut() {
                changed |=
                    copy_nested_behavior_bodydata_value(child, interner, inferred_project_path);
            }
            changed
        }
        FieldValue::Struct(_) => {
            if let Some(inferred_project_path) = inferred_project_path {
                if synthesize_single_struct_fallback_behavior_bodydata(
                    value,
                    interner,
                    inferred_project_path,
                ) {
                    return true;
                }
            }
            let FieldValue::Struct(fields) = value else {
                return false;
            };
            let mut changed = false;
            for (_, child) in fields.iter_mut() {
                changed |=
                    copy_nested_behavior_bodydata_value(child, interner, inferred_project_path);
            }
            changed
        }
        _ => false,
    }
}

fn copy_behavior_bodydata_list(
    items: &mut Vec<FieldValue>,
    interner: &StringInterner,
    inferred_project_path: Option<&str>,
) -> bool {
    let modls: Vec<(usize, usize, String, FieldValue)> = items
        .iter()
        .enumerate()
        .filter_map(|(item_idx, item)| {
            struct_model_path_field(item, interner)
                .map(|(field_idx, path, value)| (item_idx, field_idx, path, value))
        })
        .collect();
    if let Some(inferred_project_path) = inferred_project_path {
        if synthesize_single_fallback_behavior_bodydata(
            items,
            &modls,
            interner,
            inferred_project_path,
        ) {
            return true;
        }
        if replace_inferred_behavior_bodydata_fallbacks(
            items,
            &modls,
            interner,
            inferred_project_path,
        ) {
            return true;
        }
    }

    copy_adjacent_hkx_bodydata_pair(items, &modls)
}

fn copy_adjacent_hkx_bodydata_pair(
    items: &mut [FieldValue],
    modls: &[(usize, usize, String, FieldValue)],
) -> bool {
    if modls.len() < 2 {
        return false;
    }

    for pair in modls.windows(2) {
        let (_male_idx, _male_field_idx, ref male, ref male_value) = pair[0];
        let (female_idx, female_field_idx, ref female, _) = pair[1];
        if male == female {
            continue;
        }
        if !male.to_ascii_lowercase().ends_with(".hkx")
            || !female.to_ascii_lowercase().ends_with(".hkx")
        {
            continue;
        }
        if set_struct_field_value(&mut items[female_idx], female_field_idx, male_value.clone()) {
            return true;
        }
    }

    false
}

fn synthesize_single_fallback_behavior_bodydata(
    items: &mut Vec<FieldValue>,
    modls: &[(usize, usize, String, FieldValue)],
    interner: &StringInterner,
    inferred_project_path: &str,
) -> bool {
    if modls.len() != 1 || items.len() != 1 {
        return false;
    }

    let (item_idx, field_idx, path, template_value) = &modls[0];
    if !is_behavior_project_fallback_for_inferred(path, inferred_project_path)
        || normalize_runtime_path(path) == normalize_runtime_path(inferred_project_path)
    {
        return false;
    }

    let mut fallback_row = items[*item_idx].clone();
    strip_bodydata_marker_fields(&mut fallback_row, interner);
    let replacement = path_field_value_like(template_value, inferred_project_path, interner);
    if !set_struct_field_value(&mut items[*item_idx], *field_idx, replacement) {
        return false;
    }

    items.push(fallback_row);
    true
}

fn synthesize_single_struct_fallback_behavior_bodydata(
    value: &mut FieldValue,
    interner: &StringInterner,
    inferred_project_path: &str,
) -> bool {
    let FieldValue::Struct(fields) = value else {
        return false;
    };
    let Some((field_idx, path, template_value)) =
        fields
            .iter()
            .enumerate()
            .find_map(|(idx, (field_name, child))| {
                if !model_path_key_matches(*field_name, interner) {
                    return None;
                }
                path_from_field_value(child, interner).map(|path| (idx, path, child.clone()))
            })
    else {
        return false;
    };
    if !is_behavior_project_fallback_for_inferred(&path, inferred_project_path)
        || normalize_runtime_path(&path) == normalize_runtime_path(inferred_project_path)
    {
        return false;
    }

    let mut native_fields = fields.clone();
    native_fields[field_idx].1 =
        path_field_value_like(&template_value, inferred_project_path, interner);

    let mut fallback_row = FieldValue::Struct(fields.clone());
    strip_bodydata_marker_fields(&mut fallback_row, interner);
    *value = FieldValue::List(vec![FieldValue::Struct(native_fields), fallback_row]);
    true
}

fn replace_inferred_behavior_bodydata_fallbacks(
    items: &mut [FieldValue],
    modls: &[(usize, usize, String, FieldValue)],
    interner: &StringInterner,
    inferred_project_path: &str,
) -> bool {
    let mut changed = false;
    for (item_idx, field_idx, path, template_value) in modls {
        if !is_behavior_project_fallback_for_inferred(path, inferred_project_path) {
            continue;
        }
        let replacement = path_field_value_like(template_value, inferred_project_path, interner);
        changed |= set_struct_field_value(&mut items[*item_idx], *field_idx, replacement);
    }
    changed
}

fn is_behavior_project_fallback_for_inferred(path: &str, inferred_project_path: &str) -> bool {
    if normalize_runtime_path(path) == normalize_runtime_path(inferred_project_path) {
        return false;
    }
    // A retarget onto FO4's shared human project is an authored override rather than an
    // inference from the creature's own subgraphs, so the actor-dir rules below cannot
    // apply — `actor_dir_from_behavior_project_path` deliberately refuses `Character`,
    // which would otherwise reject the override's own target.
    if normalize_runtime_path(inferred_project_path)
        == normalize_runtime_path(HUMAN_BEHAVIOR_PROJECT)
    {
        return is_behavior_project_path(path);
    }
    let Some(actor_dir) = actor_dir_from_behavior_project_path(path) else {
        return false;
    };
    let Some(inferred_actor_dir) = actor_dir_from_behavior_project_path(inferred_project_path)
    else {
        return false;
    };
    if actor_dir.eq_ignore_ascii_case("Cat_Pet")
        && inferred_actor_dir.eq_ignore_ascii_case("Cat_Pet")
        && normalize_runtime_path(inferred_project_path)
            == normalize_runtime_path(CAT_PET_BEHAVIOR_PROJECT)
        && path_file_name(path)
            .map(|name| name.eq_ignore_ascii_case("CatPet.hkx"))
            .unwrap_or(false)
    {
        return true;
    }
    !actor_dir.eq_ignore_ascii_case(inferred_actor_dir)
}

fn path_field_value_like(
    template_value: &FieldValue,
    path: &str,
    interner: &StringInterner,
) -> FieldValue {
    match template_value {
        FieldValue::Bytes(_) => {
            let mut bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
            bytes.extend_from_slice(path.as_bytes());
            bytes.push(0);
            FieldValue::Bytes(bytes)
        }
        _ => FieldValue::String(interner.intern(path)),
    }
}

fn strip_bodydata_marker_fields(value: &mut FieldValue, interner: &StringInterner) {
    let FieldValue::Struct(fields) = value else {
        return;
    };
    fields.retain(|(key, _)| {
        !interner
            .resolve(*key)
            .map(|name| {
                name.eq_ignore_ascii_case("FNAM")
                    || name.eq_ignore_ascii_case("MNAM")
                    || name.eq_ignore_ascii_case("BodyDataMarker")
                    || name.eq_ignore_ascii_case("BehaviorGraphMarker")
            })
            .unwrap_or(false)
    });
}

fn infer_creature_project_path_from_subgraphs(
    record: &Record,
    interner: &StringInterner,
) -> Option<String> {
    if let Some(project_path) = creature_project_path_override(record, interner) {
        return Some(project_path.to_string());
    }

    let sgnm_sig = SubrecordSig::from_str("SGNM").ok()?;
    let sapt_sig = SubrecordSig::from_str("SAPT").ok()?;
    let mut actors: Vec<String> = Vec::new();

    for entry in &record.fields {
        if entry.sig == sgnm_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            let Some(actor_dir) = actor_dir_from_behavior_graph(&path) else {
                continue;
            };
            push_unique_actor(&mut actors, actor_dir);
        } else if entry.sig == sapt_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            let Some(actor_dir) = actor_dir_from_animation_path(&path) else {
                continue;
            };
            push_unique_actor(&mut actors, actor_dir);
        }
    }

    let actor_dir = if actors.len() == 1 {
        actors[0].clone()
    } else if actors.is_empty() && has_subgraph_template_race(record) {
        infer_creature_actor_dir_from_skeleton(record, interner)?
    } else {
        return None;
    };
    existing_behavior_project_path_for_actor(record, interner, &actor_dir).or_else(|| {
        let project_file = inferred_project_file_for_actor(&actor_dir);
        Some(format!("Actors\\{actor_dir}\\{project_file}"))
    })
}

fn creature_project_path_override(
    record: &Record,
    interner: &StringInterner,
) -> Option<&'static str> {
    let editor_id = record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .or_else(|| {
            let edid_sig = SubrecordSig::from_str("EDID").ok()?;
            record.fields.iter().find_map(|entry| {
                if entry.sig != edid_sig {
                    return None;
                }
                let FieldValue::String(editor_id) = entry.value else {
                    return None;
                };
                interner.resolve(editor_id)
            })
        })?;
    if editor_id.eq_ignore_ascii_case("CatPetRace") {
        return Some(CAT_PET_BEHAVIOR_PROJECT);
    }
    // The Lesser Devil has no behavior chain of its own (`Actors\LesserDevil` holds only
    // `characterassets`; mesh, materials and animations live under `Actors\JerseyDevil`),
    // so it inherits subgraph blocks from `XPD_JerseyDevilRace` via `SubgraphTemplateRace`
    // and needs that race's project. FO76's male behavior row says so. The female row
    // names the Deathclaw project, whose rig shares 5 of the Lesser Devil's 125 bone
    // names, so every clip binds to nothing and the actor holds bind pose. The Jersey
    // Devil rig shares all 125.
    if editor_id.eq_ignore_ascii_case("XPD_LesserDevilRace") {
        return Some(JERSEY_DEVIL_BEHAVIOR_PROJECT);
    }
    // Scorched animate on FO4's human rig, but the converted ScorchedProject chain aims
    // the weapon ~90 degrees up (cause unisolated). FO4's human project fixes the aim and
    // keeps the Scorched skeletal model, and so the jaw. This works only because the
    // Scorched rig has all 128 FO4 Character bone names. Do NOT extend it to
    // MoleMinerRace: that rig shares just 5 (Root, COM, WEAPON), so human clips would
    // bind to nothing.
    if editor_id.eq_ignore_ascii_case("ScorchedRace") {
        return Some(HUMAN_BEHAVIOR_PROJECT);
    }
    None
}

/// FO4's `PowerArmorRace` subgraph rows, in record order, read out of the target masters.
///
/// Returns `None` when the masters are unavailable or the donor carries no `SGNM`, so a
/// missing donor degrades to "leave the race alone" rather than emitting empty blocks.
fn fo4_power_armor_subgraph_entries(
    session: &mut PluginSession,
    target_schema: &AuthoringSchema,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Option<Vec<FieldEntry>> {
    let (hex, plugin) = FO4_POWER_ARMOR_RACE.rsplit_once(':')?;
    let donor_fk = FormKey {
        local: u32::from_str_radix(hex, 16).ok()?,
        plugin: mapper.interner.intern(plugin),
    };
    let donor =
        crate::fixups::face::generate_additive_races::load_target_race_template_from_masters(
            session,
            target_schema,
            mapper,
            config,
            donor_fk,
        )?;
    let sgnm_sig = SubrecordSig::from_str("SGNM").ok()?;
    let entries: Vec<FieldEntry> = donor
        .fields
        .iter()
        .filter(|entry| is_subgraph_data_sig(entry.sig))
        .cloned()
        .collect();
    entries
        .iter()
        .any(|entry| entry.sig == sgnm_sig)
        .then_some(entries)
}

/// Give a converted race FO4's power-armor locomotion when nothing else will.
///
/// FO76 keeps base locomotion in the actor's project -> character -> root chain, so its
/// RACEs author subgraph blocks only for extras (`ZetanInvaderRace` has two). FO4 keeps it
/// in RACE subgraph blocks (`PowerArmorRace` has 127, including `MTBehavior`). A converted
/// race on `Actors\PowerArmor\PowerArmorProject.hkx` resolves to FO4's chain, finds no
/// locomotion, and T-poses in place; head-tracking still works because it is engine-side.
///
/// Donor rows go first so the race's own blocks stay last and most specific. They are
/// copied verbatim to keep the `SAKD`/`STKD`-before-`SGNM` grouping and the opaque `SRAF`
/// bytes. Races that inherit blocks through `SubgraphTemplateRace` are left alone.
fn seed_power_armor_locomotion_subgraphs(
    record: &mut Record,
    donor_entries: &[FieldEntry],
    interner: &StringInterner,
) -> bool {
    if has_subgraph_template_race(record) {
        return false;
    }
    let (Ok(modl_sig), Ok(sgnm_sig)) = (
        SubrecordSig::from_str("MODL"),
        SubrecordSig::from_str("SGNM"),
    ) else {
        return false;
    };
    let on_power_armor = record.fields.iter().any(|entry| {
        entry.sig == modl_sig
            && path_from_field_value(&entry.value, interner)
                .map(|path| {
                    normalize_runtime_path(&path)
                        == normalize_runtime_path(POWER_ARMOR_BEHAVIOR_PROJECT)
                })
                .unwrap_or(false)
    });
    if !on_power_armor {
        return false;
    }
    let can_locomote = record.fields.iter().any(|entry| {
        entry.sig == sgnm_sig
            && extract_zstring(&entry.value, interner)
                .map(|path| path.to_ascii_lowercase().contains("mtbehavior"))
                .unwrap_or(false)
    });
    if can_locomote {
        return false;
    }
    // Without an existing block there is no defensible place to put these rows, and RACE
    // subrecord order is load-bearing — so decline rather than guess an insertion point.
    let Some(at) = record
        .fields
        .iter()
        .position(|entry| is_subgraph_data_sig(entry.sig))
    else {
        return false;
    };
    record.fields.insert_many(at, donor_entries.iter().cloned());
    true
}

fn has_subgraph_template_race(record: &Record) -> bool {
    let Ok(srac_sig) = SubrecordSig::from_str("SRAC") else {
        return false;
    };
    record.fields.iter().any(|entry| {
        entry.sig == srac_sig
            && match &entry.value {
                FieldValue::FormKey(fk) => fk.local != 0,
                FieldValue::Bytes(data) if data.len() >= 4 => {
                    u32::from_le_bytes([data[0], data[1], data[2], data[3]]) != 0
                }
                _ => false,
            }
    })
}

fn infer_creature_actor_dir_from_skeleton(
    record: &Record,
    interner: &StringInterner,
) -> Option<String> {
    let mnam_sig = SubrecordSig::from_str("MNAM").ok()?;
    let fnam_sig = SubrecordSig::from_str("FNAM").ok()?;
    let anam_sig = SubrecordSig::from_str("ANAM").ok()?;
    let (male_idx, _) = locate_first_male_female_pair(record, mnam_sig, fnam_sig, anam_sig)?;
    let path = extract_zstring(&record.fields[male_idx].value, interner)?;
    actor_dir_from_skeletal_model_path(&path).map(str::to_string)
}

fn existing_behavior_project_path_for_actor(
    record: &Record,
    interner: &StringInterner,
    actor_dir: &str,
) -> Option<String> {
    let modl_sig = SubrecordSig::from_str("MODL").ok()?;
    for entry in &record.fields {
        if entry.sig == modl_sig {
            if let Some(path) = path_from_field_value(&entry.value, interner)
                && behavior_project_path_matches_actor(&path, actor_dir)
            {
                return Some(normalize_runtime_slashes(&path));
            }
        }
        if let Some(path) =
            existing_behavior_project_path_in_value(&entry.value, interner, actor_dir)
        {
            return Some(path);
        }
    }
    None
}

fn existing_behavior_project_path_in_value(
    value: &FieldValue,
    interner: &StringInterner,
    actor_dir: &str,
) -> Option<String> {
    match value {
        FieldValue::List(items) => items
            .iter()
            .find_map(|item| existing_behavior_project_path_in_value(item, interner, actor_dir)),
        FieldValue::Struct(fields) => {
            for (field_name, child) in fields {
                if model_path_key_matches(*field_name, interner) {
                    if let Some(path) = path_from_field_value(child, interner)
                        && behavior_project_path_matches_actor(&path, actor_dir)
                    {
                        return Some(normalize_runtime_slashes(&path));
                    }
                }
                if let Some(path) =
                    existing_behavior_project_path_in_value(child, interner, actor_dir)
                {
                    return Some(path);
                }
            }
            None
        }
        _ => None,
    }
}

fn behavior_project_path_matches_actor(path: &str, actor_dir: &str) -> bool {
    actor_dir_from_behavior_project_path(path)
        .map(|found| found.eq_ignore_ascii_case(actor_dir))
        .unwrap_or(false)
}

fn normalize_runtime_slashes(path: &str) -> String {
    path.replace('/', "\\")
}

fn push_unique_actor(actors: &mut Vec<String>, actor_dir: &str) {
    if actors
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(actor_dir))
    {
        return;
    }
    actors.push(actor_dir.to_string());
}

fn path_segments(path: &str) -> Vec<&str> {
    path.split(|c| c == '\\' || c == '/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn path_file_name(path: &str) -> Option<&str> {
    path_segments(path).last().copied()
}

fn actor_dir_from_behavior_graph(path: &str) -> Option<&str> {
    let segments = path_segments(path);
    if segments.len() < 4 {
        return None;
    }
    if !segments[0].eq_ignore_ascii_case("Actors") || !segments[2].eq_ignore_ascii_case("Behaviors")
    {
        return None;
    }
    if segments[1].eq_ignore_ascii_case("Shared") {
        return None;
    }
    if segments[1].eq_ignore_ascii_case("Character") {
        return None;
    }
    Some(segments[1])
}

fn actor_dir_from_animation_path(path: &str) -> Option<&str> {
    let segments = path_segments(path);
    if segments.len() < 3 {
        return None;
    }
    if !segments[0].eq_ignore_ascii_case("Actors")
        || !segments[2].eq_ignore_ascii_case("Animations")
    {
        return None;
    }
    if segments[1].eq_ignore_ascii_case("Shared") || segments[1].eq_ignore_ascii_case("Character") {
        return None;
    }
    Some(segments[1])
}

fn actor_dir_from_base_animation_path(path: &str) -> Option<&str> {
    let segments = path_segments(path);
    if segments.len() != 3 {
        return None;
    }
    if !segments[0].eq_ignore_ascii_case("Actors")
        || !segments[2].eq_ignore_ascii_case("Animations")
    {
        return None;
    }
    Some(segments[1])
}

fn actor_dir_from_behavior_project_path(path: &str) -> Option<&str> {
    let segments = path_segments(path);
    if segments.len() != 3 {
        return None;
    }
    if !segments[0].eq_ignore_ascii_case("Actors") {
        return None;
    }
    if segments[1].eq_ignore_ascii_case("Shared") || segments[1].eq_ignore_ascii_case("Character") {
        return None;
    }
    if !segments[2].to_ascii_lowercase().ends_with(".hkx") {
        return None;
    }
    Some(segments[1])
}

/// Whether `path` is shaped like any `Actors\<dir>\<file>.hkx` behavior project, including
/// the shared `Character` / `Shared` dirs that `actor_dir_from_behavior_project_path`
/// refuses to report a creature actor dir for. Requiring the `.hkx` tail is what keeps
/// this off the sibling `MODL` subrecords holding `.nif` skeletons and `.egt` body models.
fn is_behavior_project_path(path: &str) -> bool {
    let segments = path_segments(path);
    segments.len() == 3
        && segments[0].eq_ignore_ascii_case("Actors")
        && segments[2].to_ascii_lowercase().ends_with(".hkx")
}

fn actor_dir_from_skeletal_model_path(path: &str) -> Option<&str> {
    let segments = path_segments(path);
    if segments.len() < 4
        || !segments[0].eq_ignore_ascii_case("Actors")
        || !segments[2].eq_ignore_ascii_case("CharacterAssets")
        || !segments.last()?.to_ascii_lowercase().ends_with(".nif")
    {
        return None;
    }
    if segments[1].eq_ignore_ascii_case("Shared") || segments[1].eq_ignore_ascii_case("Character") {
        return None;
    }
    Some(segments[1])
}

fn is_core_behavior_graph_file(file_name: &str) -> bool {
    let file_name = file_name.to_ascii_lowercase();
    file_name.contains("core") || file_name == "deathclaweverything.hkx"
}

fn inferred_project_file_for_actor(actor_dir: &str) -> String {
    if actor_dir.eq_ignore_ascii_case("RadHog") {
        format!("{actor_dir}.hkx")
    } else {
        format!("{actor_dir}Project.hkx")
    }
}

fn promote_nested_sheepsquatch_deathclaw_behavior(
    record: &mut Record,
    interner: &StringInterner,
) -> bool {
    let mut changed = false;
    for entry in &mut record.fields {
        changed |= promote_nested_sheepsquatch_deathclaw_behavior_value(&mut entry.value, interner);
    }
    changed
}

fn promote_nested_sheepsquatch_deathclaw_behavior_value(
    value: &mut FieldValue,
    interner: &StringInterner,
) -> bool {
    match value {
        FieldValue::List(items) => {
            let mut changed = promote_behavior_bodydata_list(items, interner);
            for child in items.iter_mut() {
                changed |= promote_nested_sheepsquatch_deathclaw_behavior_value(child, interner);
            }
            changed
        }
        FieldValue::Struct(fields) => {
            let mut changed = false;
            for (_, child) in fields.iter_mut() {
                changed |= promote_nested_sheepsquatch_deathclaw_behavior_value(child, interner);
            }
            changed
        }
        _ => false,
    }
}

fn promote_behavior_bodydata_list(items: &mut [FieldValue], interner: &StringInterner) -> bool {
    let modls: Vec<(usize, usize, String, FieldValue)> = items
        .iter()
        .enumerate()
        .filter_map(|(item_idx, item)| {
            struct_model_path_field(item, interner)
                .map(|(field_idx, path, value)| (item_idx, field_idx, path, value))
        })
        .collect();
    let mut changed = false;

    if modls.len() >= 2 {
        for pair in modls.windows(2) {
            let (male_idx, male_field_idx, ref male, _) = pair[0];
            let (_female_idx, _female_field_idx, ref female, ref female_value) = pair[1];
            if !is_sheepsquatch_deathclaw_runtime_pair(male, female, ".hkx") {
                continue;
            }
            changed |=
                set_struct_field_value(&mut items[male_idx], male_field_idx, female_value.clone());
        }
    }

    for (item_idx, field_idx, path, template_value) in &modls {
        if !should_promote_bodydata_project_to_deathclaw(path) {
            continue;
        }
        let replacement =
            path_field_value_like(template_value, DEATHCLAW_BEHAVIOR_PROJECT, interner);
        changed |= set_struct_field_value(&mut items[*item_idx], *field_idx, replacement);
    }

    changed
}

fn should_promote_bodydata_project_to_deathclaw(path: &str) -> bool {
    actor_dir_from_behavior_project_path(path)
        .map(|actor_dir| !actor_dir.eq_ignore_ascii_case("Deathclaw"))
        .unwrap_or(false)
}

fn normalize_sheepsquatch_deathclaw_subgraph_paths(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let sgnm_sig = match SubrecordSig::from_str("SGNM") {
        Ok(s) => s,
        Err(_) => return,
    };
    let sapt_sig = match SubrecordSig::from_str("SAPT") {
        Ok(s) => s,
        Err(_) => return,
    };

    for entry in &mut record.fields {
        if entry.sig != sgnm_sig && entry.sig != sapt_sig {
            continue;
        }
        let Some(path) = extract_zstring(&entry.value, interner) else {
            continue;
        };

        let replacement = if entry.sig == sgnm_sig {
            actor_dir_from_behavior_graph(&path)
                .filter(|actor| actor.eq_ignore_ascii_case("Sheepsquatch"))
                .map(|_| DEATHCLAW_BEHAVIOR_GRAPH.to_string())
        } else {
            deathclaw_animation_path_for_sheepsquatch_path(&path)
        };

        let Some(replacement) = replacement else {
            continue;
        };
        if normalize_runtime_path(&path) == normalize_runtime_path(&replacement) {
            continue;
        }
        entry.value = FieldValue::String(interner.intern(&replacement));
        outcome.subgraph_paths_normalized += 1;
    }
}

fn deathclaw_animation_path_for_sheepsquatch_path(path: &str) -> Option<String> {
    let segments = path_segments(path);
    if segments.len() < 3 {
        return None;
    }
    if !segments[0].eq_ignore_ascii_case("Actors")
        || !segments[1].eq_ignore_ascii_case("Sheepsquatch")
        || !segments[2].eq_ignore_ascii_case("Animations")
    {
        return None;
    }

    let mut replacement = String::from("Actors\\Deathclaw\\Animations");
    for segment in &segments[3..] {
        replacement.push('\\');
        replacement.push_str(segment);
    }
    Some(replacement)
}

fn struct_model_path_field(
    value: &FieldValue,
    interner: &StringInterner,
) -> Option<(usize, String, FieldValue)> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    fields
        .iter()
        .enumerate()
        .find_map(|(idx, (field_name, child))| {
            if !model_path_key_matches(*field_name, interner) {
                return None;
            }
            path_from_field_value(child, interner).map(|path| (idx, path, child.clone()))
        })
}

fn model_path_key_matches(field_name: Sym, interner: &StringInterner) -> bool {
    interner
        .resolve(field_name)
        .map(|s| {
            s.eq_ignore_ascii_case("MODL")
                || s.eq_ignore_ascii_case("ModelFileName")
                || s.eq_ignore_ascii_case("File")
                || s.eq_ignore_ascii_case("FileName")
                || s.eq_ignore_ascii_case("Path")
        })
        .unwrap_or(false)
}

fn path_from_field_value(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_string()),
        FieldValue::Bytes(bytes) => {
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            std::str::from_utf8(&bytes[..end]).ok().map(str::to_string)
        }
        _ => None,
    }
}

fn set_struct_field_value(value: &mut FieldValue, field_idx: usize, new_value: FieldValue) -> bool {
    let FieldValue::Struct(fields) = value else {
        return false;
    };
    let Some((_, child)) = fields.get_mut(field_idx) else {
        return false;
    };
    if *child == new_value {
        return false;
    }
    *child = new_value;
    true
}

fn patch_raw_behavior_project_modls(
    session: &mut PluginSession,
    fk: &FormKey,
    inferred_project_path: &str,
) -> Result<u32, FixupError> {
    session
        .patch_all_subrecords_bytes(fk, "MODL", |bytes| {
            patch_behavior_project_modl_bytes(bytes, inferred_project_path)
        })
        .map_err(|e| FixupError::HandleError(e.to_string()))
}

fn patch_behavior_project_modl_bytes(bytes: &mut Vec<u8>, inferred_project_path: &str) -> bool {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    let Ok(path) = std::str::from_utf8(&bytes[..end]) else {
        return false;
    };
    if !is_behavior_project_fallback_for_inferred(path, inferred_project_path) {
        return false;
    }

    bytes.clear();
    bytes.extend_from_slice(inferred_project_path.as_bytes());
    bytes.push(0);
    true
}

// ---------------------------------------------------------------------------
// Fix 3 — strip subgraph-data blocks that contain STKD (TargetKeywords)
// ---------------------------------------------------------------------------
//
// A subgraph block starts at SGNM (BehaviourGraph zstring) and ends at SRAF.
// SAKD/STKD rows after SRAF belong to the following SGNM in both FO76 and FO4.
// STKD-only branches usually target furniture animation keywords that FO4 cannot
// satisfy after conversion. Core creature blocks that include the base animation
// path are the runtime entry points vanilla FO4 races preserve, so keep those
// blocks but strip the target-keyword gate from them. The FO4 Snallygaster
// reference plugin also omits the AmbushHole branch once the base path exists.

const SUBGRAPH_DATA_SIGS: &[&str] = &["SGNM", "SAKD", "SAPT", "STKD", "SRAF"];

fn is_subgraph_data_sig(sig: SubrecordSig) -> bool {
    let s = sig.as_str();
    SUBGRAPH_DATA_SIGS.iter().any(|x| *x == s)
}

fn strip_subgraph_blocks_with_target_keywords(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let sgnm_sig = match SubrecordSig::from_str("SGNM") {
        Ok(s) => s,
        Err(_) => return,
    };
    let stkd_sig = match SubrecordSig::from_str("STKD") {
        Ok(s) => s,
        Err(_) => return,
    };

    let blocks = identify_subgraph_blocks_through_sraf(record, sgnm_sig);
    if blocks.is_empty() {
        return;
    }

    let mut drop_indices: Vec<bool> = vec![false; record.fields.len()];
    let mut strip_stkd_indices: Vec<bool> = vec![false; record.fields.len()];
    let mut dropped = 0u32;
    let mut stripped = 0u32;
    let has_snallygaster_base_path =
        has_snallygaster_base_animation_subgraph(record, &blocks, interner);
    for block in &blocks {
        if has_snallygaster_base_path
            && is_snallygaster_ambushhole_subgraph(record, block, interner)
        {
            for i in block.start..block.end {
                drop_indices[i] = true;
            }
            mark_preceding_keyword_gate(record, block.start, &mut drop_indices);
            dropped += 1;
            continue;
        }

        let has_stkd = record.fields[block.start..block.end]
            .iter()
            .any(|e| e.sig == stkd_sig);
        if has_stkd {
            if is_core_base_animation_subgraph(record, block, interner) {
                for i in block.start..block.end {
                    if record.fields[i].sig == stkd_sig {
                        strip_stkd_indices[i] = true;
                        stripped += 1;
                    }
                }
                continue;
            } else if is_shared_ambush_actor_animation_subgraph(record, block, interner) {
                continue;
            } else {
                for i in block.start..block.end {
                    drop_indices[i] = true;
                }
                mark_preceding_keyword_gate(record, block.start, &mut drop_indices);
                dropped += 1;
            }
        }
    }

    if dropped == 0 && stripped == 0 {
        return;
    }

    let mut new_fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    for (i, entry) in record.fields.drain(..).enumerate() {
        if !drop_indices[i] && !strip_stkd_indices[i] {
            new_fields.push(entry);
        }
    }
    record.fields = new_fields;
    outcome.subgraph_blocks_dropped += dropped;
    outcome.subgraph_target_keywords_stripped += stripped;
}

/// A subgraph's SAKD/STKD keyword gate is stored BETWEEN the previous block's
/// SRAF and its own SGNM, so `identify_subgraph_blocks_through_sraf` spans
/// never contain it. Dropping a block must drop its gate too, or the orphaned
/// keywords re-attach to a neighboring subgraph after keyword normalization.
fn mark_preceding_keyword_gate(record: &Record, block_start: usize, drop_indices: &mut [bool]) {
    let sakd_sig = SubrecordSig::from_str("SAKD").ok();
    let stkd_sig = SubrecordSig::from_str("STKD").ok();
    let mut i = block_start;
    while i > 0 {
        let sig = Some(record.fields[i - 1].sig);
        if sig == sakd_sig || sig == stkd_sig {
            drop_indices[i - 1] = true;
            i -= 1;
        } else {
            break;
        }
    }
}

fn has_snallygaster_base_animation_subgraph(
    record: &Record,
    blocks: &[SubgraphBlockSpan],
    interner: &StringInterner,
) -> bool {
    blocks
        .iter()
        .any(|block| is_snallygaster_base_animation_subgraph(record, block, interner))
}

fn is_snallygaster_base_animation_subgraph(
    record: &Record,
    block: &SubgraphBlockSpan,
    interner: &StringInterner,
) -> bool {
    let sgnm_sig = match SubrecordSig::from_str("SGNM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let sapt_sig = match SubrecordSig::from_str("SAPT") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut has_snallygaster_graph = false;
    let mut has_base_path = false;
    for entry in &record.fields[block.start..block.end] {
        if entry.sig == sgnm_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            has_snallygaster_graph |= actor_dir_from_behavior_graph(&path)
                .map(|actor| actor.eq_ignore_ascii_case("Snallygaster"))
                .unwrap_or(false);
        } else if entry.sig == sapt_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            has_base_path |= normalize_path_key(&path) == "actors\\snallygaster\\animations";
        }
    }
    has_snallygaster_graph && has_base_path
}

fn is_snallygaster_ambushhole_subgraph(
    record: &Record,
    block: &SubgraphBlockSpan,
    interner: &StringInterner,
) -> bool {
    let sgnm_sig = match SubrecordSig::from_str("SGNM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let sapt_sig = match SubrecordSig::from_str("SAPT") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut has_snallygaster_graph = false;
    let mut has_ambushhole_path = false;
    for entry in &record.fields[block.start..block.end] {
        if entry.sig == sgnm_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            has_snallygaster_graph |= actor_dir_from_behavior_graph(&path)
                .map(|actor| actor.eq_ignore_ascii_case("Snallygaster"))
                .unwrap_or(false);
        } else if entry.sig == sapt_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            has_ambushhole_path |=
                normalize_path_key(&path) == "actors\\snallygaster\\animations\\ambushhole";
        }
    }
    has_snallygaster_graph && has_ambushhole_path
}

fn is_core_base_animation_subgraph(
    record: &Record,
    block: &SubgraphBlockSpan,
    interner: &StringInterner,
) -> bool {
    let sgnm_sig = match SubrecordSig::from_str("SGNM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let sapt_sig = match SubrecordSig::from_str("SAPT") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut graph_actor: Option<String> = None;
    let mut has_base_path = false;
    for entry in &record.fields[block.start..block.end] {
        if entry.sig == sgnm_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            let Some(actor) = actor_dir_from_behavior_graph(&path) else {
                continue;
            };
            let Some(file_name) = path_file_name(&path) else {
                continue;
            };
            if is_core_behavior_graph_file(file_name) {
                graph_actor = Some(actor.to_ascii_lowercase());
            }
        } else if entry.sig == sapt_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            let Some(base_actor) = actor_dir_from_base_animation_path(&path) else {
                continue;
            };
            has_base_path |= graph_actor
                .as_deref()
                .map(|actor| actor == base_actor.to_ascii_lowercase())
                .unwrap_or(false);
        }
    }
    graph_actor.is_some() && has_base_path
}

fn is_shared_ambush_actor_animation_subgraph(
    record: &Record,
    block: &SubgraphBlockSpan,
    interner: &StringInterner,
) -> bool {
    let sgnm_sig = match SubrecordSig::from_str("SGNM") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let sapt_sig = match SubrecordSig::from_str("SAPT") {
        Ok(s) => s,
        Err(_) => return false,
    };

    let mut has_shared_ambush_graph = false;
    let mut has_actor_animation_path = false;
    for entry in &record.fields[block.start..block.end] {
        if entry.sig == sgnm_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            has_shared_ambush_graph |=
                normalize_path_key(&path) == "actors\\shared\\behaviors\\ambushbehavior.hkx";
        } else if entry.sig == sapt_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            has_actor_animation_path |= actor_dir_from_animation_path(&path).is_some();
        }
    }

    has_shared_ambush_graph && has_actor_animation_path
}

fn normalize_ambushhole_subgraph_paths(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let sapt_sig = match SubrecordSig::from_str("SAPT") {
        Ok(s) => s,
        Err(_) => return,
    };

    let has_base_path = record.fields.iter().any(|entry| {
        entry.sig == sapt_sig
            && matches!(
                extract_zstring(&entry.value, interner).as_deref(),
                Some(path) if normalize_path_key(path).ends_with("\\animations")
            )
    });
    if has_base_path {
        return;
    }

    for entry in &mut record.fields {
        if entry.sig != sapt_sig {
            continue;
        }
        let sym = match &entry.value {
            FieldValue::String(sym) => *sym,
            _ => continue,
        };
        let Some(path) = interner.resolve(sym) else {
            continue;
        };
        let normalized = normalize_path_key(path);
        let Some(pos) = normalized.rfind("\\ambushhole") else {
            continue;
        };
        let replacement = path[..pos].to_string();
        entry.value = FieldValue::String(interner.intern(&replacement));
        outcome.subgraph_paths_normalized += 1;
        break;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnallygasterSubgraphKind {
    Base,
    InjuredRight,
    InjuredLeft,
    InjuredBoth,
}

fn normalize_snallygaster_subgraph_keywords(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let sgnm_sig = match SubrecordSig::from_str("SGNM") {
        Ok(s) => s,
        Err(_) => return,
    };
    let sakd_sig = match SubrecordSig::from_str("SAKD") {
        Ok(s) => s,
        Err(_) => return,
    };

    let blocks = identify_subgraph_blocks(record, sgnm_sig);
    if blocks.is_empty() || !has_snallygaster_base_animation_subgraph(record, &blocks, interner) {
        return;
    }

    let mut classified: Vec<(SnallygasterSubgraphKind, SubgraphBlockSpan)> = Vec::new();
    for block in blocks {
        let Some(kind) = snallygaster_subgraph_kind(record, &block, interner) else {
            continue;
        };
        if classified.iter().any(|(seen, _)| *seen == kind) {
            continue;
        }
        classified.push((kind, block));
    }
    if classified.len() < 2 {
        return;
    }
    classified.sort_by_key(|(_, block)| block.start);
    if classified
        .windows(2)
        .any(|pair| pair[0].1.end != pair[1].1.start)
    {
        return;
    }

    let fallout4_plugin = interner.intern("Fallout4.esm");
    let mut normalized_blocks: Vec<Vec<FieldEntry>> = Vec::new();
    let mut changed_blocks = 0u32;
    for kind in [
        SnallygasterSubgraphKind::Base,
        SnallygasterSubgraphKind::InjuredRight,
        SnallygasterSubgraphKind::InjuredLeft,
        SnallygasterSubgraphKind::InjuredBoth,
    ] {
        let Some((_, block)) = classified.iter().find(|(seen, _)| *seen == kind) else {
            continue;
        };
        let original = &record.fields[block.start..block.end];
        let mut normalized: Vec<FieldEntry> = original
            .iter()
            .filter(|entry| entry.sig != sakd_sig)
            .cloned()
            .collect();
        for local in desired_snallygaster_subgraph_keywords(kind) {
            normalized.push(FieldEntry {
                sig: sakd_sig,
                value: FieldValue::FormKey(FormKey {
                    local,
                    plugin: fallout4_plugin,
                }),
            });
        }
        if normalized != original {
            changed_blocks += 1;
        }
        normalized_blocks.push(normalized);
    }

    if changed_blocks == 0 {
        return;
    }

    let range_start = classified
        .first()
        .map(|(_, block)| block.start)
        .unwrap_or(0);
    let range_end = classified.last().map(|(_, block)| block.end).unwrap_or(0);
    let mut new_fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    let mut i = 0usize;
    while i < record.fields.len() {
        if i == range_start {
            for block in normalized_blocks.drain(..) {
                new_fields.extend(block);
            }
            i = range_end;
            continue;
        }
        new_fields.push(record.fields[i].clone());
        i += 1;
    }
    record.fields = new_fields;
    outcome.subgraph_keywords_normalized += changed_blocks;
}

fn snallygaster_subgraph_kind(
    record: &Record,
    block: &SubgraphBlockSpan,
    interner: &StringInterner,
) -> Option<SnallygasterSubgraphKind> {
    let sgnm_sig = SubrecordSig::from_str("SGNM").ok()?;
    let sapt_sig = SubrecordSig::from_str("SAPT").ok()?;

    let mut has_snallygaster_graph = false;
    let mut has_base_path = false;
    let mut injured_kind: Option<SnallygasterSubgraphKind> = None;
    for entry in &record.fields[block.start..block.end] {
        if entry.sig == sgnm_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            has_snallygaster_graph |= actor_dir_from_behavior_graph(&path)
                .map(|actor| actor.eq_ignore_ascii_case("Snallygaster"))
                .unwrap_or(false);
        } else if entry.sig == sapt_sig {
            let Some(path) = extract_zstring(&entry.value, interner) else {
                continue;
            };
            let normalized = normalize_path_key(&path);
            if normalized == "actors\\snallygaster\\animations" {
                has_base_path = true;
            } else if normalized == "actors\\snallygaster\\animations\\injured\\rightleg" {
                injured_kind = Some(SnallygasterSubgraphKind::InjuredRight);
            } else if normalized == "actors\\snallygaster\\animations\\injured\\leftleg" {
                injured_kind = Some(SnallygasterSubgraphKind::InjuredLeft);
            } else if normalized == "actors\\snallygaster\\animations\\injured\\bothlegs"
                || normalized == "actors\\snallygaster\\animations\\injured\\boothlegs"
            {
                injured_kind = Some(SnallygasterSubgraphKind::InjuredBoth);
            }
        }
    }

    if !has_snallygaster_graph {
        return None;
    }
    injured_kind.or_else(|| {
        if has_base_path {
            Some(SnallygasterSubgraphKind::Base)
        } else {
            None
        }
    })
}

fn desired_snallygaster_subgraph_keywords(kind: SnallygasterSubgraphKind) -> Vec<u32> {
    match kind {
        SnallygasterSubgraphKind::Base => vec![0x030B00],
        SnallygasterSubgraphKind::InjuredRight => vec![0x030B01],
        SnallygasterSubgraphKind::InjuredLeft => vec![0x030B01, 0x030B00],
        SnallygasterSubgraphKind::InjuredBoth => Vec::new(),
    }
}

#[derive(Debug)]
struct SubgraphBlockSpan {
    start: usize, // inclusive — index of SGNM
    end: usize,   // exclusive — first index NOT in this block
}

fn identify_subgraph_blocks(record: &Record, sgnm_sig: SubrecordSig) -> Vec<SubgraphBlockSpan> {
    let mut blocks: Vec<SubgraphBlockSpan> = Vec::new();
    let mut current_start: Option<usize> = None;
    for (i, entry) in record.fields.iter().enumerate() {
        if entry.sig == sgnm_sig {
            if let Some(start) = current_start {
                blocks.push(SubgraphBlockSpan { start, end: i });
            }
            current_start = Some(i);
        } else if current_start.is_some() && !is_subgraph_data_sig(entry.sig) {
            // Block ends at first non-subgraph-data subrecord.
            let start = current_start.take().unwrap();
            blocks.push(SubgraphBlockSpan { start, end: i });
        }
    }
    if let Some(start) = current_start {
        blocks.push(SubgraphBlockSpan {
            start,
            end: record.fields.len(),
        });
    }
    blocks
}

fn identify_subgraph_blocks_through_sraf(
    record: &Record,
    sgnm_sig: SubrecordSig,
) -> Vec<SubgraphBlockSpan> {
    let sraf_sig = SubrecordSig::from_str("SRAF").ok();
    let mut blocks: Vec<SubgraphBlockSpan> = Vec::new();
    let mut current_start: Option<usize> = None;
    for (i, entry) in record.fields.iter().enumerate() {
        if entry.sig == sgnm_sig {
            if let Some(start) = current_start {
                blocks.push(SubgraphBlockSpan { start, end: i });
            }
            current_start = Some(i);
        } else if current_start.is_some() && Some(entry.sig) == sraf_sig {
            let start = current_start.take().unwrap();
            blocks.push(SubgraphBlockSpan { start, end: i + 1 });
        } else if current_start.is_some() && !is_subgraph_data_sig(entry.sig) {
            let start = current_start.take().unwrap();
            blocks.push(SubgraphBlockSpan { start, end: i });
        }
    }
    if let Some(start) = current_start {
        blocks.push(SubgraphBlockSpan {
            start,
            end: record.fields.len(),
        });
    }
    blocks
}

// ---------------------------------------------------------------------------
// Fix 5 — collapse adjacent duplicate subgraph-data blocks
// ---------------------------------------------------------------------------
//
// When two consecutive subgraph blocks are byte-identical after normalizing
// path zstrings, drop the duplicate block. Valid creature races can repeat the
// same SGNM graph with different SAPT chains (Snallygaster injured variants),
// so SGNM alone is not a duplicate key.

fn collapse_adjacent_duplicate_subgraphs(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let sgnm_sig = match SubrecordSig::from_str("SGNM") {
        Ok(s) => s,
        Err(_) => return,
    };

    let blocks = identify_subgraph_blocks(record, sgnm_sig);
    if blocks.len() < 2 {
        return;
    }

    let mut drop_indices: Vec<bool> = vec![false; record.fields.len()];
    let mut dropped = 0u32;
    let mut last_block_key: Option<Vec<(String, String)>> = None;

    for block in &blocks {
        let block_key = subgraph_block_key(record, block, interner);
        match (&last_block_key, &block_key) {
            (Some(prev), Some(curr)) if prev == curr => {
                for i in block.start..block.end {
                    drop_indices[i] = true;
                }
                dropped += 1;
                // Keep last_block_key as `prev` to collapse runs of 3+ duplicates.
            }
            _ => {
                last_block_key = block_key;
            }
        }
    }

    if dropped == 0 {
        return;
    }

    let mut new_fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    for (i, entry) in record.fields.drain(..).enumerate() {
        if !drop_indices[i] {
            new_fields.push(entry);
        }
    }
    record.fields = new_fields;
    outcome.duplicate_graphs_collapsed += dropped;
}

// ---------------------------------------------------------------------------
// Fix 7 — give role-0 (locomotion) subgraph blocks the creature's H2H path
// ---------------------------------------------------------------------------
//
// FO4's shared MTBehavior takes unarmed locomotion clips from the creature's
// `H2H` folder, so FO4 role-0 blocks list it (`SuperMutantRace`:
// `…\Animations\MT\Neutral`, `…\Animations\H2H`, `…\Animations\Shared`). FO76
// races do not (`MoleMinerRace`: `…\Animations\MT`, `…\Animations\Shared`), so
// an unarmed converted creature head-tracks and attacks at contact range but
// never moves. Armed variants use their weapon-role paths. Converted Scorched
// already lists `Actors\Scorched\Animations\H2H` in role 0 and chases fine.

/// `SRAF` is `struct:H,H` — role then perspective. Role 0 is MT (locomotion).
const SRAF_ROLE_MT: u16 = 0;

fn add_h2h_path_to_locomotion_blocks(
    record: &mut Record,
    interner: &StringInterner,
    outcome: &mut RaceFixOutcome,
) {
    let (Ok(sgnm_sig), Ok(sapt_sig)) = (
        SubrecordSig::from_str("SGNM"),
        SubrecordSig::from_str("SAPT"),
    ) else {
        return;
    };

    // Only ever reuse a path this race already declares. That proves the
    // creature actually ships an H2H folder without consulting the filesystem,
    // and it cannot invent one for a race that has none.
    let Some(h2h) = record
        .fields
        .iter()
        .find_map(|entry| subgraph_path_sym(entry, sapt_sig, interner, path_leaf_is_h2h))
    else {
        return;
    };

    let locomotion: Vec<SubgraphBlockSpan> =
        identify_subgraph_blocks_through_sraf(record, sgnm_sig)
            .into_iter()
            .filter(|block| subgraph_block_is_locomotion(record, block))
            .collect();

    // If the race already wires H2H into locomotion anywhere, it is a shape
    // FO4 understands — leave every block alone rather than second-guessing
    // per-block variants (Scorched's injured-on-ground block deliberately
    // has no H2H row).
    let already_wired = locomotion.iter().any(|block| {
        block_path_syms(record, block, sapt_sig).any(|sym| sym_paths_equal(sym, h2h, interner))
    });
    if already_wired {
        return;
    }

    let mut insertions: Vec<usize> = Vec::new();
    for block in &locomotion {
        let paths: Vec<(usize, Sym)> = block_path_entries(record, block, sapt_sig).collect();
        let Some(&(last_index, last_sym)) = paths.last() else {
            continue;
        };
        // FO4 orders the list `MT…`, `H2H`, `Shared`, so land before a trailing
        // Shared row; otherwise append after the last path.
        let before_shared = interner
            .resolve(last_sym)
            .is_some_and(|path| path_leaf_is(path, "Shared"));
        insertions.push(if before_shared {
            last_index
        } else {
            last_index + 1
        });
    }

    // Descending, so indices computed above stay valid as we splice.
    insertions.sort_unstable();
    for at in insertions.iter().rev() {
        record.fields.insert(
            *at,
            FieldEntry {
                sig: sapt_sig,
                value: FieldValue::String(h2h),
            },
        );
        outcome.locomotion_h2h_paths_added += 1;
    }
}

/// A block with no `SRAF`, or one whose role half is 0, is a locomotion block.
/// FO4's own role-0 blocks carry an all-zero SRAF.
fn subgraph_block_is_locomotion(record: &Record, block: &SubgraphBlockSpan) -> bool {
    let Ok(sraf_sig) = SubrecordSig::from_str("SRAF") else {
        return false;
    };
    let Some(entries) = record.fields.get(block.start..block.end) else {
        return false;
    };
    match entries.iter().find(|entry| entry.sig == sraf_sig) {
        Some(entry) => match &entry.value {
            FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                u16::from_le_bytes([bytes[0], bytes[1]]) == SRAF_ROLE_MT
            }
            _ => true,
        },
        None => true,
    }
}

fn block_path_entries<'a>(
    record: &'a Record,
    block: &SubgraphBlockSpan,
    sapt_sig: SubrecordSig,
) -> impl Iterator<Item = (usize, Sym)> + 'a {
    let start = block.start;
    record
        .fields
        .get(start..block.end)
        .unwrap_or_default()
        .iter()
        .enumerate()
        .filter_map(move |(offset, entry)| match (&entry.sig, &entry.value) {
            (sig, FieldValue::String(sym)) if *sig == sapt_sig => Some((start + offset, *sym)),
            _ => None,
        })
}

fn block_path_syms<'a>(
    record: &'a Record,
    block: &SubgraphBlockSpan,
    sapt_sig: SubrecordSig,
) -> impl Iterator<Item = Sym> + 'a {
    block_path_entries(record, block, sapt_sig).map(|(_, sym)| sym)
}

fn subgraph_path_sym(
    entry: &FieldEntry,
    sapt_sig: SubrecordSig,
    interner: &StringInterner,
    predicate: fn(&str) -> bool,
) -> Option<Sym> {
    if entry.sig != sapt_sig {
        return None;
    }
    let FieldValue::String(sym) = entry.value else {
        return None;
    };
    predicate(interner.resolve(sym)?).then_some(sym)
}

fn sym_paths_equal(left: Sym, right: Sym, interner: &StringInterner) -> bool {
    left == right
        || matches!(
            (interner.resolve(left), interner.resolve(right)),
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b)
        )
}

fn path_leaf_is(path: &str, leaf: &str) -> bool {
    path.rsplit(['\\', '/'])
        .next()
        .is_some_and(|last| last.eq_ignore_ascii_case(leaf))
}

fn path_leaf_is_h2h(path: &str) -> bool {
    path_leaf_is(path, "H2H")
}

fn subgraph_block_key(
    record: &Record,
    block: &SubgraphBlockSpan,
    interner: &StringInterner,
) -> Option<Vec<(String, String)>> {
    let mut key = Vec::new();
    for entry in record.fields.get(block.start..block.end)? {
        key.push((
            entry.sig.as_str().to_string(),
            subgraph_value_key(&entry.value, interner),
        ));
    }
    Some(key)
}

fn subgraph_value_key(value: &FieldValue, interner: &StringInterner) -> String {
    match value {
        FieldValue::None => "none".to_string(),
        FieldValue::Bool(v) => format!("bool:{v}"),
        FieldValue::Int(v) => format!("int:{v}"),
        FieldValue::Uint(v) => format!("uint:{v}"),
        FieldValue::Float(v) => format!("float:{:08x}", v.to_bits()),
        FieldValue::String(sym) => interner
            .resolve(*sym)
            .map(|s| format!("str:{}", normalize_path_key(s)))
            .unwrap_or_else(|| "str:<unresolved>".to_string()),
        FieldValue::Bytes(bytes) => format!("bytes:{:02x?}", bytes.as_slice()),
        FieldValue::FormKey(fk) => format!(
            "fk:{:06x}:{}",
            fk.local,
            interner
                .resolve(fk.plugin)
                .map(str::to_ascii_lowercase)
                .unwrap_or_else(|| "<unresolved>".to_string())
        ),
        FieldValue::List(_) | FieldValue::Struct(_) => format!("{value:?}"),
    }
}

fn normalize_path_key(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::creature::creature_predicate::{
        ACTOR_TYPE_CREATURE_LOW24, ACTOR_TYPE_HUMANLIKE_LOW24, ACTOR_TYPE_NPC_LOW24,
        ACTOR_TYPE_SUPER_MUTANT_LOW24,
    };
    use crate::fixups::{FixupConfig, FixupContext, FixupRegistry};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedRecord, ParsedSubrecord, insert_parsed_record, plugin_handle_close_native,
        plugin_handle_new_native,
    };
    use smol_str::SmolStr;
    use std::sync::Arc;

    fn make_creature_config() -> (StringInterner, Arc<AuthoringSchema>, FixupConfig) {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            target_schema: Some(schema.clone()),
            ..Default::default()
        };
        (interner, schema, config)
    }

    fn make_race(local: u32, plugin: &str, interner: &StringInterner) -> Record {
        let sig = SigCode::from_str("RACE").unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn atkd_bytes(flags: u32, attack_angle: f32) -> smallvec::SmallVec<[u8; 32]> {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        data.resize(ATKD_SIZE, 0u8);
        let f = flags.to_le_bytes();
        data[ATKD_ATTACK_FLAGS_OFFSET] = f[0];
        data[ATKD_ATTACK_FLAGS_OFFSET + 1] = f[1];
        data[ATKD_ATTACK_FLAGS_OFFSET + 2] = f[2];
        data[ATKD_ATTACK_FLAGS_OFFSET + 3] = f[3];
        let a = attack_angle.to_le_bytes();
        data[ATKD_ATTACK_ANGLE_OFFSET] = a[0];
        data[ATKD_ATTACK_ANGLE_OFFSET + 1] = a[1];
        data[ATKD_ATTACK_ANGLE_OFFSET + 2] = a[2];
        data[ATKD_ATTACK_ANGLE_OFFSET + 3] = a[3];
        data
    }

    fn push_field(record: &mut Record, sig_str: &str, value: FieldValue) {
        let sig = SubrecordSig::from_str(sig_str).unwrap();
        record.fields.push(FieldEntry { sig, value });
    }

    fn snallygaster_live_behavior_bodydata_record(
        interner: &StringInterner,
        local: u32,
        plugin: &str,
    ) -> (Record, Sym) {
        let mut record = make_race(local, plugin, interner);
        let modl = interner.intern("MODL");
        let modt = interner.intern("MODT");
        let fnam = interner.intern("FNAM");
        let snally_project = interner.intern("Actors\\Snallygaster\\SnallygasterProject.hkx");
        let molerat_project = interner.intern("Actors\\Molerat\\MoleratProject.hkx");
        let mut model_info: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        model_info.extend_from_slice(&[4, 0, 0, 0]);
        model_info.resize(20, 0);

        push_field(
            &mut record,
            "MODT",
            FieldValue::List(vec![
                FieldValue::Struct(vec![
                    (modl, FieldValue::String(snally_project)),
                    (modt, FieldValue::Bytes(model_info.clone())),
                    (fnam, FieldValue::Bool(true)),
                ]),
                FieldValue::Struct(vec![
                    (modl, FieldValue::String(molerat_project)),
                    (modt, FieldValue::Bytes(model_info)),
                ]),
            ]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );

        (record, snally_project)
    }

    fn parsed_zstring_subrecord(sig: &str, path: &str) -> ParsedSubrecord {
        let mut bytes = path.as_bytes().to_vec();
        bytes.push(0);
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(bytes),
            semantic_type: None,
        }
    }

    fn parsed_bytes_subrecord(sig: &str, bytes: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: Bytes::from(bytes),
            semantic_type: None,
        }
    }

    fn parsed_snallygaster_race_with_molerat_project() -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("RACE"),
            form_id: 0x0000_0100,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                parsed_zstring_subrecord("MODL", "Actors\\Snallygaster\\SnallygasterProject.hkx"),
                parsed_bytes_subrecord("MODT", vec![4, 0, 0, 0]),
                parsed_zstring_subrecord("MODL", "Actors\\Molerat\\MoleratProject.hkx"),
                parsed_bytes_subrecord("MODT", vec![4, 0, 0, 0]),
                parsed_zstring_subrecord(
                    "SGNM",
                    "Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx",
                ),
                parsed_zstring_subrecord("SAPT", "Actors\\Snallygaster\\Animations"),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn parsed_floater_race_without_movement() -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("RACE"),
            form_id: 0x0000_0100,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                parsed_zstring_subrecord("EDID", "FloaterRace"),
                parsed_zstring_subrecord("SGNM", "Actors\\Floater\\Behaviors\\FloaterCore.hkx"),
                parsed_zstring_subrecord("SAPT", "Actors\\Floater\\Animations"),
                parsed_bytes_subrecord("SRAF", vec![0, 0, 0, 0]),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn parsed_floater_default_movement() -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("MOVT"),
            form_id: 0x0000_0200,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                parsed_zstring_subrecord("EDID", "Floater_Default_MT"),
                parsed_zstring_subrecord("MNAM", "FloaterDefault"),
                parsed_bytes_subrecord("JNAM", 80.0f32.to_le_bytes().to_vec()),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn parsed_scorched_default_movement() -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("MOVT"),
            form_id: 0x0000_0201,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                parsed_zstring_subrecord("EDID", "Scorched_Default_MT"),
                parsed_zstring_subrecord("MNAM", "ScorchedDefault"),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn parsed_race_with_equipment_flags(
        form_id: u32,
        editor_id: &str,
        actor_type_keyword: u32,
        equipment_flags: u32,
    ) -> ParsedRecord {
        ParsedRecord {
            signature: SmolStr::new("RACE"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: vec![
                parsed_zstring_subrecord("EDID", editor_id),
                parsed_bytes_subrecord("KSIZ", 1_u32.to_le_bytes().to_vec()),
                parsed_bytes_subrecord("KWDA", actor_type_keyword.to_le_bytes().to_vec()),
                parsed_bytes_subrecord("VNAM", equipment_flags.to_le_bytes().to_vec()),
            ],
            raw_payload: None,
            parse_error: None,
        }
    }

    fn bodydata_modl_sym(item: &FieldValue, modl: Sym) -> Sym {
        let FieldValue::Struct(fields) = item else {
            panic!("expected BodyData struct");
        };
        let (_, FieldValue::String(sym)) = fields
            .iter()
            .find(|(key, _)| *key == modl)
            .expect("MODL field")
        else {
            panic!("expected MODL string");
        };
        *sym
    }

    fn read_atkd_flags(data: &[u8]) -> u32 {
        u32::from_le_bytes([
            data[ATKD_ATTACK_FLAGS_OFFSET],
            data[ATKD_ATTACK_FLAGS_OFFSET + 1],
            data[ATKD_ATTACK_FLAGS_OFFSET + 2],
            data[ATKD_ATTACK_FLAGS_OFFSET + 3],
        ])
    }

    fn read_atkd_angle(data: &[u8]) -> f32 {
        f32::from_le_bytes([
            data[ATKD_ATTACK_ANGLE_OFFSET],
            data[ATKD_ATTACK_ANGLE_OFFSET + 1],
            data[ATKD_ATTACK_ANGLE_OFFSET + 2],
            data[ATKD_ATTACK_ANGLE_OFFSET + 3],
        ])
    }

    fn formid_bytes(ids: &[u32]) -> smallvec::SmallVec<[u8; 32]> {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        for id in ids {
            data.extend_from_slice(&id.to_le_bytes());
        }
        data
    }

    fn u32_bytes(value: u32) -> smallvec::SmallVec<[u8; 32]> {
        smallvec::SmallVec::from_slice(&value.to_le_bytes())
    }

    fn first_bytes<'a>(record: &'a Record, sig: &str) -> &'a [u8] {
        let sig = SubrecordSig::from_str(sig).unwrap();
        let entry = record
            .fields
            .iter()
            .find(|entry| entry.sig == sig)
            .expect("expected subrecord");
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("expected Bytes value");
        };
        bytes
    }

    fn count_sig(record: &Record, sig: &str) -> usize {
        let sig = SubrecordSig::from_str(sig).unwrap();
        record
            .fields
            .iter()
            .filter(|entry| entry.sig == sig)
            .count()
    }

    fn sigs(record: &Record) -> Vec<&str> {
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect()
    }

    #[test]
    fn adds_actor_type_animal_to_creature_race_keywords() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(&mut record, "KSIZ", FieldValue::Bytes(u32_bytes(1)));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(formid_bytes(&[0x00_013795])),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.actor_type_animal_added);

        let kwda = first_bytes(&record, "KWDA");
        let observed: Vec<u32> = kwda
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        assert_eq!(observed, vec![0x00_013795, 0x00_013798]);

        let ksiz = first_bytes(&record, "KSIZ");
        assert_eq!(u32::from_le_bytes(ksiz.try_into().unwrap()), 2);
    }

    /// Vanilla `DLC04_RadAntRace` carries `RagdollOnDeath`; the converted race
    /// lost it, so ants played a borrowed roach death clip before dropping.
    #[test]
    fn adds_ragdoll_on_death_to_rad_ant_race() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x112BEC, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("RadAntRace"));
        push_field(&mut record, "KSIZ", FieldValue::Bytes(u32_bytes(1)));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(formid_bytes(&[0x00_013795])),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.ragdoll_on_death_added);

        let kwda = first_bytes(&record, "KWDA");
        let observed: Vec<u32> = kwda
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        assert!(observed.contains(&RAGDOLL_ON_DEATH_LOW24));

        let ksiz = first_bytes(&record, "KSIZ");
        assert_eq!(
            u32::from_le_bytes(ksiz.try_into().unwrap()),
            observed.len() as u32
        );
    }

    /// The keyword is on 1 of 84 vanilla FO4 races. Every other creature owns
    /// its death clips, so seeding it broadly would change deaths that work.
    #[test]
    fn does_not_add_ragdoll_on_death_to_other_creature_races() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("MothmanRace"));
        push_field(&mut record, "KSIZ", FieldValue::Bytes(u32_bytes(1)));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(formid_bytes(&[0x00_013795])),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.ragdoll_on_death_added);

        let kwda = first_bytes(&record, "KWDA");
        let observed: Vec<u32> = kwda
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect();
        assert!(!observed.contains(&RAGDOLL_ON_DEATH_LOW24));
    }

    /// Re-running the fixup over already-fixed output must not duplicate.
    #[test]
    fn does_not_duplicate_existing_ragdoll_on_death_keyword() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x112BEC, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("RadAntRace"));
        push_field(&mut record, "KSIZ", FieldValue::Bytes(u32_bytes(2)));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(formid_bytes(&[0x00_013795, RAGDOLL_ON_DEATH_LOW24])),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.ragdoll_on_death_added);

        let kwda = first_bytes(&record, "KWDA");
        let occurrences = kwda
            .chunks_exact(4)
            .filter(|chunk| {
                u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                    == RAGDOLL_ON_DEATH_LOW24
            })
            .count();
        assert_eq!(occurrences, 1);
    }

    #[test]
    fn does_not_add_actor_type_animal_to_non_creature_race() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(&mut record, "KSIZ", FieldValue::Bytes(u32_bytes(1)));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(formid_bytes(&[0x00_0F23C5])),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.actor_type_animal_added);
        assert_eq!(first_bytes(&record, "KWDA").len(), 4);
    }

    /// FO76 tags mole miners `ActorTypeCreature`, but `ActorTypeAnimal` drives
    /// Animal Friend targeting and vanilla FO4 never grants it to a humanoid.
    #[test]
    fn does_not_add_actor_type_animal_to_armed_humanoid_race() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x012E6B, "Output.esp", &mut interner);
        push_field(&mut record, "KSIZ", FieldValue::Bytes(u32_bytes(2)));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(formid_bytes(&[
                ACTOR_TYPE_CREATURE_LOW24,
                ACTOR_TYPE_HUMANLIKE_LOW24,
            ])),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.actor_type_animal_added);
        assert_eq!(first_bytes(&record, "KWDA").len(), 8, "no keyword appended");
    }

    /// A super mutant reaches the same exemption via `ActorTypeSuperMutant`.
    #[test]
    fn does_not_add_actor_type_animal_to_super_mutant_race() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x5C4F6E, "Output.esp", &mut interner);
        push_field(&mut record, "KSIZ", FieldValue::Bytes(u32_bytes(2)));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(formid_bytes(&[
                ACTOR_TYPE_CREATURE_LOW24,
                ACTOR_TYPE_SUPER_MUTANT_LOW24,
            ])),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.actor_type_animal_added);
        assert_eq!(first_bytes(&record, "KWDA").len(), 8);
    }

    #[test]
    fn floater_routes_ranged_equip_slots_to_projectile_node() {
        let interner = StringInterner::new();
        let mut record = make_race(0x55F6FA, "Output.esp", &interner);
        record.eid = Some(interner.intern("FloaterRace"));
        for (slot, node) in [
            (MIDDLE_HAND_EQUIP_SLOT_LOW24, "Head"),
            (ALT_ATTACK_SLOT_1_LOW24, "Head"),
            (0x13_6256, "Eye"),
        ] {
            push_field(
                &mut record,
                "QNAM",
                FieldValue::FormKey(FormKey {
                    local: slot,
                    plugin: interner.intern("Fallout4.esm"),
                }),
            );
            push_field(
                &mut record,
                "ZNAM",
                FieldValue::String(interner.intern(node)),
            );
        }

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.floater_projectile_nodes_fixed, 2);
        let nodes: Vec<&str> = record
            .fields
            .iter()
            .filter(|entry| entry.sig == SubrecordSig::from_str("ZNAM").unwrap())
            .filter_map(|entry| match entry.value {
                FieldValue::String(value) => interner.resolve(value),
                _ => None,
            })
            .collect();
        assert_eq!(nodes, vec!["ProjectileNode", "ProjectileNode", "Eye"]);

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.floater_projectile_nodes_fixed, 0);
        assert!(!outcome.changed());
    }

    #[test]
    fn synthesizes_zero_phwt_rows_after_phtn_before_movement_data() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "PHTN",
            FieldValue::String(interner.intern("Aah")),
        );
        push_field(
            &mut record,
            "PHTN",
            FieldValue::String(interner.intern("BigAah")),
        );
        push_field(
            &mut record,
            "WKMV",
            FieldValue::FormKey(FormKey {
                local: 0x001234,
                plugin: interner.intern("Output.esp"),
            }),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.phoneme_weights_synthesized, 43);
        assert_eq!(count_sig(&record, "PHWT"), 43);

        let observed = sigs(&record);
        assert_eq!(&observed[0..2], &["PHTN", "PHTN"]);
        assert!(observed[2..45].iter().all(|sig| *sig == "PHWT"));
        assert_eq!(observed[45], "WKMV");
        for field in record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "PHWT")
        {
            let FieldValue::Bytes(bytes) = &field.value else {
                panic!("PHWT should be raw zero bytes");
            };
            assert_eq!(bytes.len(), PHWT_ZERO_WEIGHT_SIZE);
            assert!(bytes.iter().all(|b| *b == 0));
        }
    }

    #[test]
    fn tops_up_partial_phwt_rows_without_duplicating_existing_rows() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "PHTN",
            FieldValue::String(interner.intern("Aah")),
        );
        for _ in 0..40 {
            push_field(
                &mut record,
                "PHWT",
                FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1; PHWT_ZERO_WEIGHT_SIZE])),
            );
        }

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.phoneme_weights_synthesized, 3);
        assert_eq!(count_sig(&record, "PHWT"), 43);
    }

    #[test]
    fn applies_to_npc_root() {
        let (_, schema, config) = make_creature_config();
        let mut mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(FixCreatureRaceRecordsFixup.applies_to(&ctx));
        let _ = mapper;
    }

    #[test]
    fn applies_to_lvln_root() {
        let (_, schema, _) = make_creature_config();
        let mut mapper_interner = StringInterner::new();
        let mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("LVLN").unwrap()),
            ..Default::default()
        };
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(FixCreatureRaceRecordsFixup.applies_to(&ctx));
        let _ = mapper;
    }

    #[test]
    fn does_not_apply_to_weap_root() {
        let (_, schema, _) = make_creature_config();
        let mut mapper_interner = StringInterner::new();
        let mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(!FixCreatureRaceRecordsFixup.applies_to(&ctx));
        let _ = mapper;
    }

    #[test]
    fn does_not_apply_when_no_root_sig() {
        let (_, schema, _) = make_creature_config();
        let mut mapper_interner = StringInterner::new();
        let mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let config = FixupConfig {
            root_sig: None,
            ..Default::default()
        };
        let ctx = FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };
        assert!(!FixCreatureRaceRecordsFixup.applies_to(&ctx));
        let _ = mapper;
    }

    #[test]
    fn empty_record_is_no_op() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.changed(), "empty record must produce no changes");
    }

    #[test]
    fn strips_unknown_6_bit_from_attack_flags() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        // 0x40 | 0x04 = 0x44 (unknown_6 + power_attack)
        push_field(
            &mut record,
            "ATKD",
            FieldValue::Bytes(atkd_bytes(0x44, 0.0)),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.atkd_flags_stripped, 1);
        assert!(outcome.changed());

        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            assert_eq!(read_atkd_flags(data), 0x04, "0x40 bit must be cleared");
        } else {
            panic!("expected Bytes");
        }
    }

    #[test]
    fn strips_sheepsquatch_fo76_only_attack_flags() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        for flags in [0x200, 0x254, 0x504] {
            push_field(
                &mut record,
                "ATKD",
                FieldValue::Bytes(atkd_bytes(flags, 0.0)),
            );
        }

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.atkd_flags_stripped, 3);

        let actual: Vec<u32> = record
            .fields
            .iter()
            .map(|entry| match &entry.value {
                FieldValue::Bytes(data) => read_atkd_flags(data),
                _ => panic!("expected Bytes"),
            })
            .collect();
        assert_eq!(actual, vec![0, 0x14, 0x04]);
    }

    #[test]
    fn atkd_without_unknown_bit_not_touched() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "ATKD",
            FieldValue::Bytes(atkd_bytes(0x04, 0.0)),
        );
        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.atkd_flags_stripped, 0);
    }

    #[test]
    fn preserves_high_equipment_flags_on_creature_race() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let source_vnam = 1_u32 | 2 | 512 | 8192 | 16384 | 0xF8FF_8000;
        let mut bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        bytes.extend_from_slice(&source_vnam.to_le_bytes());
        push_field(&mut record, "VNAM", FieldValue::Bytes(bytes));

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.equipment_flags_fixed);
        let FieldValue::Bytes(data) = &record.fields[0].value else {
            panic!("expected VNAM bytes");
        };
        let actual = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        // H2H (1) and one_hand_sword (2) both go: this race has no humanoid
        // marker, so it gets FO4's body-fighting-creature profile.
        assert_eq!(actual, 512 | 8192 | 16384 | 0xF8FF_8000);
    }

    #[test]
    fn injects_attack_angle_for_directional_event() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        // ATKD with flags=0 and attack_angle=0.
        push_field(&mut record, "ATKD", FieldValue::Bytes(atkd_bytes(0, 0.0)));
        let event = interner.intern("AttackLeftPowerSwing");
        push_field(&mut record, "ATKE", FieldValue::String(event));

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.atkd_angles_injected, 1);
        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            assert_eq!(read_atkd_angle(data), -90.0);
        }
    }

    #[test]
    fn injects_attack_angle_for_backward_and_right() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(&mut record, "ATKD", FieldValue::Bytes(atkd_bytes(0, 0.0)));
        push_field(
            &mut record,
            "ATKE",
            FieldValue::String(interner.intern("AttackBackwardSwing")),
        );
        push_field(&mut record, "ATKD", FieldValue::Bytes(atkd_bytes(0, 0.0)));
        push_field(
            &mut record,
            "ATKE",
            FieldValue::String(interner.intern("AttackRightSwing")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.atkd_angles_injected, 2);

        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            assert_eq!(read_atkd_angle(data), 180.0);
        }
        if let FieldValue::Bytes(ref data) = record.fields[2].value {
            assert_eq!(read_atkd_angle(data), 90.0);
        }
    }

    #[test]
    fn non_directional_event_no_angle_injected() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(&mut record, "ATKD", FieldValue::Bytes(atkd_bytes(0, 0.0)));
        push_field(
            &mut record,
            "ATKE",
            FieldValue::String(interner.intern("AttackPowerSwing")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.atkd_angles_injected, 0);
    }

    #[test]
    fn existing_attack_angle_preserved() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        // attack_angle = 45.0 (already set)
        push_field(&mut record, "ATKD", FieldValue::Bytes(atkd_bytes(0, 45.0)));
        push_field(
            &mut record,
            "ATKE",
            FieldValue::String(interner.intern("AttackLeftSwing")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.atkd_angles_injected, 0);
        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            assert_eq!(read_atkd_angle(data), 45.0);
        }
    }

    #[test]
    fn copies_male_skeleton_to_female_when_differ() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let male = interner.intern("Actors\\Mirelurk\\CharacterAssets\\Skeleton.nif");
        let female = interner.intern("Actors\\Molerat\\CharacterAssets\\Skeleton.nif");
        push_field(&mut record, "MNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(male));
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(female));

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_anam_fixed);

        // Female ANAM should now equal Male.
        if let FieldValue::String(s) = record.fields[3].value {
            assert_eq!(s, male);
        } else {
            panic!("expected female ANAM to be String");
        }
    }

    #[test]
    fn same_skeleton_not_touched() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let same = interner.intern("Actors\\Mirelurk\\CharacterAssets\\Skeleton.nif");
        push_field(&mut record, "MNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(same));
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(same));

        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.female_anam_fixed);
    }

    #[test]
    fn copies_sheepsquatch_native_skeleton_to_female() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let male = interner.intern("actors\\sheepsquatch\\characterassets\\skeleton.nif");
        let female = interner.intern("Actors\\Deathclaw\\CharacterAssets\\skeleton.nif");
        push_field(&mut record, "MNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(male));
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(female));

        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.fallback_skeleton_promoted);
        assert!(outcome.female_anam_fixed);
        assert_eq!(record.fields[1].value, FieldValue::String(male));
        assert_eq!(record.fields[3].value, FieldValue::String(male));
    }

    #[test]
    fn copies_male_behavior_graph_to_female_when_differ() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        // First MNAM/FNAM block is skeletal model (no fix needed — same).
        let skel = interner.intern("Actors\\Mirelurk\\CharacterAssets\\Skeleton.nif");
        push_field(&mut record, "MNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(skel));
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(skel));
        // Second MNAM/FNAM block is behavior graph with MODL.
        let male_bhv = interner.intern("Actors\\Mirelurk\\Behaviors\\Mirelurk.hkx");
        let female_bhv = interner.intern("Actors\\Molerat\\Behaviors\\Molerat.hkx");
        push_field(&mut record, "MNAM", FieldValue::None);
        push_field(&mut record, "MODL", FieldValue::String(male_bhv));
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "MODL", FieldValue::String(female_bhv));

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        if let FieldValue::String(s) = record.fields[7].value {
            assert_eq!(s, male_bhv);
        } else {
            panic!("expected female MODL to be String");
        }
    }

    #[test]
    fn adds_missing_female_marker_to_single_native_behavior_project() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let modl = interner.intern("MODL");
        let modt = interner.intern("MODT");
        let fnam = interner.intern("FNAM");
        let project = interner.intern("Actors\\GraftonMonster\\GraftonProject.hkx");
        push_field(
            &mut record,
            "MODT",
            FieldValue::List(vec![FieldValue::Struct(vec![
                (modl, FieldValue::String(project)),
                (modt, FieldValue::Bytes(smallvec::smallvec![4, 0, 0, 0])),
            ])]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\GraftonMonster\\Behaviors\\GraftonCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\GraftonMonster\\Animations")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected behavior BodyData list");
        };
        assert_eq!(items.len(), 1);
        let FieldValue::Struct(fields) = &items[0] else {
            panic!("expected behavior BodyData row");
        };
        assert!(
            fields
                .iter()
                .any(|(name, value)| *name == fnam && *value == FieldValue::Bool(true))
        );

        assert!(!ensure_nested_behavior_bodydata_female_marker(
            &mut record,
            &interner
        ));
    }

    #[test]
    fn links_matching_float_movement_to_missing_default_and_fly_slots() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("FloaterRace"));
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Floater\\Behaviors\\FloaterCore.hkx")),
        );
        let movement_fk = FormKey {
            local: 0x56213F,
            plugin: interner.intern("Output.esp"),
        };
        let mut movement_types = DefaultMovementTypeIndex::default();
        movement_types.insert(
            "floater".to_string(),
            Some(DefaultMovementType {
                form_key: movement_fk,
                has_float_height: true,
            }),
        );

        assert!(link_missing_default_movement_types(
            &mut record,
            &movement_types,
            &interner,
            &fo4_schema()
        ));
        assert_eq!(sigs(&record), vec!["WKMV", "FLMV", "SGNM"]);
        assert!(matches!(
            record.fields[0].value,
            FieldValue::FormKey(fk) if fk == movement_fk
        ));
        assert!(matches!(
            record.fields[1].value,
            FieldValue::FormKey(fk) if fk == movement_fk
        ));
        assert!(!link_missing_default_movement_types(
            &mut record,
            &movement_types,
            &interner,
            &fo4_schema()
        ));
    }

    #[test]
    fn links_matching_flying_race_movement_to_missing_fly_slot() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("ScorchBeastRace"));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(
                race_data_bytes(&fo4_schema(), 1.0, 1.0, RACE_FLAG_FLIES)
                    .into_iter()
                    .collect(),
            ),
        );
        let movement_fk = FormKey {
            local: 0x2C041B,
            plugin: interner.intern("Output.esp"),
        };
        push_field(&mut record, "WKMV", FieldValue::FormKey(movement_fk));
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\ScorchBeast\\Behaviors\\ScorchBeastCore.hkx"),
            ),
        );
        let mut movement_types = DefaultMovementTypeIndex::default();
        movement_types.insert(
            "scorchbeast".to_string(),
            Some(DefaultMovementType {
                form_key: movement_fk,
                has_float_height: false,
            }),
        );

        assert!(link_missing_default_movement_types(
            &mut record,
            &movement_types,
            &interner,
            &fo4_schema()
        ));
        assert_eq!(sigs(&record), vec!["DATA", "WKMV", "FLMV", "SGNM"]);
        assert!(matches!(
            record.fields[2].value,
            FieldValue::FormKey(fk) if fk == movement_fk
        ));
        assert!(!link_missing_default_movement_types(
            &mut record,
            &movement_types,
            &interner,
            &fo4_schema()
        ));
    }

    // -- Fix 8: FO4 floor on RACE.DATA movement rates -----------------------

    /// `RACE.DATA` reaches a fixup as raw `FieldValue::Bytes` (`struct:` codecs
    /// are never decoded into a `Struct`), so these build the real byte blob and
    /// resolve offsets from the real FO4 schema at FV-131. Hand-built `Struct`
    /// fixtures pass against code that cannot work in production.
    fn fo4_schema() -> std::sync::Arc<AuthoringSchema> {
        AuthoringSchema::for_game("fo4").expect("fo4 schema")
    }

    fn race_data_bytes(schema: &AuthoringSchema, accel: f32, decel: f32, flags: u32) -> Vec<u8> {
        let layout =
            schema.struct_field_layout_versioned("RACE", "DATA", Some(FO4_TARGET_FORM_VERSION));
        let len = layout
            .iter()
            .map(|f| f.offset + f.width)
            .max()
            .expect("RACE.DATA layout");
        let mut bytes = vec![0u8; len];
        for (id, value) in [
            ("acceleration_rate", accel.to_le_bytes()),
            ("deceleration_rate", decel.to_le_bytes()),
            ("flags", flags.to_le_bytes()),
        ] {
            let offset = race_data_field_offset(schema, id).expect(id);
            bytes[offset..offset + 4].copy_from_slice(&value);
        }
        bytes
    }

    fn race_with_data(
        schema: &AuthoringSchema,
        accel: f32,
        decel: f32,
        flags: u32,
    ) -> (Record, StringInterner) {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("ScorchBeastRace"));
        let bytes = race_data_bytes(schema, accel, decel, flags);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(bytes.into_iter().collect()),
        );
        (record, interner)
    }

    fn rates_of(record: &Record, schema: &AuthoringSchema) -> Vec<f32> {
        let data_sig = SubrecordSig::from_str("DATA").unwrap();
        let entry = record
            .fields
            .iter()
            .find(|e| e.sig == data_sig)
            .expect("DATA");
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("DATA must stay Bytes");
        };
        ["acceleration_rate", "deceleration_rate"]
            .into_iter()
            .map(|id| {
                let off = race_data_field_offset(schema, id).unwrap();
                f32::from_le_bytes(bytes[off..off + 4].try_into().unwrap())
            })
            .collect()
    }

    /// The FV-131 layout must actually differ from the maximal one; if this ever
    /// stops holding, a version-less lookup would silently "work" in tests.
    #[test]
    fn race_data_offsets_resolve_at_fo4_form_version() {
        let schema = fo4_schema();
        for id in ["flags", "acceleration_rate", "deceleration_rate"] {
            assert!(
                race_data_field_offset(&schema, id).is_some(),
                "{id} must resolve at FV-{FO4_TARGET_FORM_VERSION}"
            );
        }
        // Pinned to the offsets read out of the SHIPPED converted
        // ScorchBeastRace blob (200 bytes at FV-131): flags 0xC3128D98 @32,
        // accel/decel 0.01 @36/@40. A probe patching exactly @36/@40 made
        // scorchbeasts move in-game, so these are ground-truth, not derived
        // from the same lookup the tests exercise.
        assert_eq!(race_data_field_offset(&schema, "flags"), Some(32));
        assert_eq!(
            race_data_field_offset(&schema, "acceleration_rate"),
            Some(36)
        );
        assert_eq!(
            race_data_field_offset(&schema, "deceleration_rate"),
            Some(40)
        );
    }

    #[test]
    fn raises_scorchbeast_movement_rates_to_fo4_floor() {
        let schema = fo4_schema();
        let (mut record, _interner) = race_with_data(&schema, 0.01, 0.01, RACE_FLAG_FLIES);
        assert_eq!(raise_movement_rates_to_fo4_floor(&mut record, &schema), 2);
        assert_eq!(rates_of(&record, &schema), vec![0.2, 0.2]);
    }

    /// FO4's own `DLC04_CaveCricketRace` ships `0.1`, so in-range values stand.
    #[test]
    fn leaves_in_range_movement_rates_untouched() {
        let schema = fo4_schema();
        for (accel, decel) in [(0.2_f32, 0.2_f32), (0.25, 1.0), (3.0, 1.0)] {
            let (mut record, _interner) = race_with_data(&schema, accel, decel, 0);
            assert_eq!(
                raise_movement_rates_to_fo4_floor(&mut record, &schema),
                0,
                "{accel}/{decel} should not be raised"
            );
            assert_eq!(rates_of(&record, &schema), vec![accel, decel]);
        }
    }

    /// An explicit zero reads as deliberate authoring, not an out-of-range value.
    #[test]
    fn leaves_explicit_zero_movement_rates_alone() {
        let schema = fo4_schema();
        let (mut record, _interner) = race_with_data(&schema, 0.0, 0.0, 0);
        assert_eq!(raise_movement_rates_to_fo4_floor(&mut record, &schema), 0);
        assert_eq!(rates_of(&record, &schema), vec![0.0, 0.0]);
    }

    #[test]
    fn raising_movement_rates_is_idempotent() {
        let schema = fo4_schema();
        let (mut record, _interner) = race_with_data(&schema, 0.01, 0.01, 0);
        assert_eq!(raise_movement_rates_to_fo4_floor(&mut record, &schema), 2);
        assert_eq!(raise_movement_rates_to_fo4_floor(&mut record, &schema), 0);
        assert_eq!(rates_of(&record, &schema), vec![0.2, 0.2]);
    }

    /// Raising rates must not disturb neighbouring fields — `flags` sits next to
    /// them in the same blob and drives the fly-slot decision.
    #[test]
    fn raising_movement_rates_preserves_the_rest_of_the_blob() {
        let schema = fo4_schema();
        let (mut record, _interner) = race_with_data(&schema, 0.01, 0.01, RACE_FLAG_FLIES);
        let data_sig = SubrecordSig::from_str("DATA").unwrap();
        let before = match &record
            .fields
            .iter()
            .find(|e| e.sig == data_sig)
            .unwrap()
            .value
        {
            FieldValue::Bytes(b) => b.to_vec(),
            _ => unreachable!(),
        };
        raise_movement_rates_to_fo4_floor(&mut record, &schema);
        let after = match &record
            .fields
            .iter()
            .find(|e| e.sig == data_sig)
            .unwrap()
            .value
        {
            FieldValue::Bytes(b) => b.to_vec(),
            _ => unreachable!(),
        };
        assert_eq!(before.len(), after.len(), "blob length must not change");
        let accel = race_data_field_offset(&schema, "acceleration_rate").unwrap();
        let differing: Vec<usize> = (0..before.len())
            .filter(|&i| before[i] != after[i])
            .collect();
        assert!(
            differing.iter().all(|&i| (accel..accel + 8).contains(&i)),
            "only the two rate floats may change, got {differing:?}"
        );
        assert!(race_has_fly_flag(&record, &schema), "Flies must survive");
    }

    /// The production shape for the fly gate: `flags` inside the raw DATA blob.
    #[test]
    fn detects_fly_flag_in_raw_data_bytes() {
        let schema = fo4_schema();
        let (flying, _a) = race_with_data(&schema, 1.0, 1.0, RACE_FLAG_FLIES);
        let (walking, _b) = race_with_data(&schema, 1.0, 1.0, 0x0000_0100);
        assert!(race_has_fly_flag(&flying, &schema));
        assert!(!race_has_fly_flag(&walking, &schema));
    }

    #[test]
    fn keeps_nonfloating_ground_race_out_of_fly_slot() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("GroundCreatureRace"));
        let movement_fk = FormKey {
            local: 0x000200,
            plugin: interner.intern("Output.esp"),
        };
        push_field(&mut record, "WKMV", FieldValue::FormKey(movement_fk));
        let mut movement_types = DefaultMovementTypeIndex::default();
        movement_types.insert(
            "groundcreature".to_string(),
            Some(DefaultMovementType {
                form_key: movement_fk,
                has_float_height: false,
            }),
        );

        assert!(!link_missing_default_movement_types(
            &mut record,
            &movement_types,
            &interner,
            &fo4_schema()
        ));
        assert_eq!(sigs(&record), vec!["WKMV"]);
    }

    #[test]
    fn does_not_guess_ambiguous_default_movement_type() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("FloaterRace"));
        let mut movement_types = DefaultMovementTypeIndex::default();
        movement_types.insert("floater".to_string(), None);

        assert!(!link_missing_default_movement_types(
            &mut record,
            &movement_types,
            &interner,
            &fo4_schema()
        ));
        assert!(record.fields.is_empty());
    }

    #[test]
    fn copies_snallygaster_molerat_bodydata_second_row_from_inferred_project() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let modl = interner.intern("MODL");
        let male_bhv = interner.intern("Actors\\Snallygaster\\SnallygasterProject.hkx");
        let female_bhv = interner.intern("Actors\\Molerat\\MoleratProject.hkx");
        push_field(
            &mut record,
            "DATA",
            FieldValue::List(vec![
                FieldValue::Struct(vec![(modl, FieldValue::String(male_bhv))]),
                FieldValue::Struct(vec![(modl, FieldValue::String(female_bhv))]),
            ]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected BodyData list");
        };
        let FieldValue::Struct(fields) = &items[1] else {
            panic!("expected female BodyData struct");
        };
        let (_, FieldValue::String(sym)) = &fields[0] else {
            panic!("expected female MODL string");
        };
        assert_eq!(*sym, male_bhv);
    }

    #[test]
    fn repairs_inherited_liberator_project_from_skeleton() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let output_plugin = interner.intern("Output.esp");
        let pioneer_skeleton = interner.intern("Actors\\Pioneer\\CharacterAssets\\skeleton.nif");
        let molerat_skeleton = interner.intern("Actors\\Molerat\\CharacterAssets\\skeleton.nif");
        let modl = interner.intern("MODL");
        let molerat_project = interner.intern("Actors\\Molerat\\MoleratProject.hkx");

        push_field(
            &mut record,
            "SRAC",
            FieldValue::FormKey(FormKey {
                local: 0x002ECF,
                plugin: output_plugin,
            }),
        );
        push_field(&mut record, "MNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(pioneer_skeleton));
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(molerat_skeleton));
        push_field(
            &mut record,
            "MODT",
            FieldValue::List(vec![FieldValue::Struct(vec![(
                modl,
                FieldValue::String(molerat_project),
            )])]),
        );

        assert_eq!(
            infer_creature_project_path_from_subgraphs(&record, &interner).as_deref(),
            Some("Actors\\Pioneer\\PioneerProject.hkx")
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let FieldValue::List(items) = &record.fields[5].value else {
            panic!("expected BodyData list");
        };
        assert_eq!(items.len(), 2);
        let pioneer_project = bodydata_modl_sym(&items[0], modl);
        assert_eq!(
            interner.resolve(pioneer_project),
            Some("Actors\\Pioneer\\PioneerProject.hkx")
        );
        assert_eq!(bodydata_modl_sym(&items[1], modl), molerat_project);
    }

    #[test]
    fn uses_existing_same_actor_project_name_for_stormboss_bodydata() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let modl = interner.intern("MODL");
        let storm_project = interner.intern("Actors\\StormBoss\\StormBoss.hkx");
        let fallback_project = interner.intern("Actors\\CreateABot\\CreateABotProject.hkx");
        push_field(
            &mut record,
            "DATA",
            FieldValue::List(vec![
                FieldValue::Struct(vec![(modl, FieldValue::String(storm_project))]),
                FieldValue::Struct(vec![(modl, FieldValue::String(fallback_project))]),
            ]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\StormBoss\\Behaviors\\StormBossCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\StormBoss\\Animations")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected BodyData list");
        };
        let second_sym = bodydata_modl_sym(&items[1], modl);
        assert_eq!(second_sym, storm_project);
        let wrong_project = interner.intern("Actors\\StormBoss\\StormBossProject.hkx");
        assert_ne!(second_sym, wrong_project);
    }

    #[test]
    fn rewrites_cat_pet_project_to_shipped_filename() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("CatPetRace"));
        let modl = interner.intern("MODL");
        let fnam = interner.intern("FNAM");
        let stale_project = interner.intern("Actors\\Cat_Pet\\CatPet.hkx");
        let graph = interner.intern("Actors\\Cat_Pet\\Behaviors\\CatPetCoreBehavior.hkx");
        let animations = interner.intern("Actors\\Cat_Pet\\Animations");
        push_field(
            &mut record,
            "DATA",
            FieldValue::List(vec![
                FieldValue::Struct(vec![
                    (modl, FieldValue::String(stale_project)),
                    (fnam, FieldValue::Bool(true)),
                ]),
                FieldValue::Struct(vec![(modl, FieldValue::String(stale_project))]),
            ]),
        );
        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(&mut record, "SAPT", FieldValue::String(animations));

        let outcome = apply_to_record(&mut record, &interner);

        assert!(outcome.female_behavior_fixed);
        assert_eq!(
            infer_creature_project_path_from_subgraphs(&record, &interner).as_deref(),
            Some(CAT_PET_BEHAVIOR_PROJECT)
        );
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected behavior BodyData rows");
        };
        for item in items {
            assert_eq!(
                interner.resolve(bodydata_modl_sym(item, modl)),
                Some(CAT_PET_BEHAVIOR_PROJECT)
            );
        }
        assert_eq!(record.fields[1].value, FieldValue::String(graph));
        assert_eq!(record.fields[2].value, FieldValue::String(animations));
    }

    #[test]
    fn patches_raw_cat_pet_project_with_same_actor_directory() {
        let mut bytes = b"Actors\\Cat_Pet\\CatPet.hkx\0".to_vec();

        assert!(patch_behavior_project_modl_bytes(
            &mut bytes,
            CAT_PET_BEHAVIOR_PROJECT
        ));
        assert_eq!(bytes, b"Actors\\Cat_Pet\\Cat_PetProject.hkx\0".to_vec());
    }

    #[test]
    fn retargets_scorched_project_to_fo4_human_project() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("ScorchedRace"));

        assert_eq!(
            creature_project_path_override(&record, &interner),
            Some(HUMAN_BEHAVIOR_PROJECT)
        );
    }

    #[test]
    fn mole_miners_are_not_retargeted_at_the_fo4_human_project() {
        // Their rig shares 5 of FO4 Character's 128 bone names, so the human clips would
        // bind to nothing — see `creature_project_path_override`.
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("MoleMinerRace"));

        assert_eq!(creature_project_path_override(&record, &interner), None);
    }

    #[test]
    fn patches_raw_scorched_project_across_actor_directories() {
        // The human project lives under `Actors\Character`, which
        // `actor_dir_from_behavior_project_path` refuses to report an actor dir for — so
        // this only passes because the override short-circuits that rule.
        let mut bytes = b"Actors\\Scorched\\ScorchedProject.hkx\0".to_vec();

        assert!(patch_behavior_project_modl_bytes(
            &mut bytes,
            HUMAN_BEHAVIOR_PROJECT
        ));
        assert_eq!(bytes, b"actors\\Character\\RaiderProject.hkx\0".to_vec());
    }

    #[test]
    fn human_project_retarget_leaves_skeleton_and_body_models_alone() {
        // The retarget patches every MODL in the record, and a RACE carries skeletal
        // models and body models under the same signature.
        for path in [
            "Actors\\Scorched\\CharacterAssets\\skeleton.nif",
            "Actors\\Character\\UpperBodyHumanMale.egt",
        ] {
            let mut bytes = path.as_bytes().to_vec();
            bytes.push(0);
            let before = bytes.clone();

            assert!(
                !patch_behavior_project_modl_bytes(&mut bytes, HUMAN_BEHAVIOR_PROJECT),
                "{path} must not be treated as a behavior project"
            );
            assert_eq!(bytes, before);
        }
    }

    /// A race on FO4's power-armor project with one non-locomotion block of its own.
    fn power_armor_race_without_locomotion(interner: &StringInterner) -> Record {
        let mut record = make_race(0x000100, "Output.esp", interner);
        record.eid = Some(interner.intern("ZetanInvaderRace"));
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern(POWER_ARMOR_BEHAVIOR_PROJECT)),
        );
        push_field(
            &mut record,
            "STKD",
            FieldValue::FormKey(FormKey {
                local: 0x00_1234,
                plugin: interner.intern("Output.esp"),
            }),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Character\\Behaviors\\SingleAnimFurniture.hkx"),
            ),
        );
        record
    }

    fn locomotion_donor(interner: &StringInterner) -> Vec<FieldEntry> {
        vec![
            FieldEntry {
                sig: SubrecordSig::from_str("SGNM").unwrap(),
                value: FieldValue::String(
                    interner.intern("Actors\\Character\\Behaviors\\MTBehavior.hkx"),
                ),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("SAPT").unwrap(),
                value: FieldValue::String(interner.intern("Actors\\PowerArmor\\Animations\\MT")),
            },
        ]
    }

    fn behaviour_graphs(record: &Record, interner: &StringInterner) -> Vec<String> {
        record
            .fields
            .iter()
            .filter(|e| e.sig.as_str() == "SGNM")
            .filter_map(|e| extract_zstring(&e.value, interner))
            .collect()
    }

    #[test]
    fn seeds_power_armor_locomotion_when_the_race_has_none() {
        let interner = StringInterner::new();
        let mut record = power_armor_race_without_locomotion(&interner);
        let donor = locomotion_donor(&interner);

        assert!(seed_power_armor_locomotion_subgraphs(
            &mut record,
            &donor,
            &interner
        ));

        let graphs = behaviour_graphs(&record, &interner);
        assert_eq!(graphs.len(), 2);
        assert!(graphs[0].contains("MTBehavior"), "donor rows go in first");
        assert!(
            graphs[1].contains("SingleAnimFurniture"),
            "the race's own block stays last and most specific"
        );
        // The race's own keyword row must still precede its own SGNM, since STKD gates the
        // block that FOLLOWS it.
        let sigs: Vec<&str> = record.fields.iter().map(|e| e.sig.as_str()).collect();
        let own_stkd = sigs.iter().position(|s| *s == "STKD").unwrap();
        assert!(own_stkd > sigs.iter().position(|s| *s == "SGNM").unwrap());
        assert!(own_stkd < sigs.iter().rposition(|s| *s == "SGNM").unwrap());
    }

    #[test]
    fn leaves_a_race_that_can_already_locomote_alone() {
        let interner = StringInterner::new();
        let mut record = power_armor_race_without_locomotion(&interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Character\\Behaviors\\MTBehavior.hkx")),
        );
        let before = record.fields.len();

        assert!(!seed_power_armor_locomotion_subgraphs(
            &mut record,
            &locomotion_donor(&interner),
            &interner
        ));
        assert_eq!(record.fields.len(), before);
    }

    #[test]
    fn leaves_a_race_with_a_subgraph_template_alone() {
        // Inheriting blocks through SubgraphTemplateRace is FO4's other supported
        // arrangement — the converted humanoid races all use it.
        let interner = StringInterner::new();
        let mut record = power_armor_race_without_locomotion(&interner);
        push_field(
            &mut record,
            "SRAC",
            FieldValue::FormKey(FormKey {
                local: 0x01_D31E,
                plugin: interner.intern("Fallout4.esm"),
            }),
        );
        let before = record.fields.len();

        assert!(!seed_power_armor_locomotion_subgraphs(
            &mut record,
            &locomotion_donor(&interner),
            &interner
        ));
        assert_eq!(record.fields.len(), before);
    }

    #[test]
    fn leaves_races_on_their_own_behavior_project_alone() {
        let interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\SnallygasterProject.hkx")),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Behaviors\\Root.hkx")),
        );
        let before = record.fields.len();

        assert!(!seed_power_armor_locomotion_subgraphs(
            &mut record,
            &locomotion_donor(&interner),
            &interner
        ));
        assert_eq!(record.fields.len(), before);
    }

    #[test]
    fn rewrites_only_lesser_devil_project_to_jersey_devil_donor() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        record.eid = Some(interner.intern("XPD_LesserDevilRace"));
        let modl = interner.intern("MODL");
        let fnam = interner.intern("FNAM");
        // The gendered pair as FO76 authors it: the male row already names the project the
        // race actually runs on, the female row is the Deathclaw dead weight.
        let male_project = interner.intern(JERSEY_DEVIL_BEHAVIOR_PROJECT);
        let female_project = interner.intern(DEATHCLAW_BEHAVIOR_PROJECT);
        push_field(
            &mut record,
            "DATA",
            FieldValue::List(vec![
                FieldValue::Struct(vec![
                    (modl, FieldValue::String(male_project)),
                    (fnam, FieldValue::Bool(true)),
                ]),
                FieldValue::Struct(vec![(modl, FieldValue::String(female_project))]),
            ]),
        );
        let output_plugin = interner.intern("Output.esp");
        let movement = FormKey {
            local: 0x6FB702,
            plugin: output_plugin,
        };
        let template_race = FormKey {
            local: 0x6C5D42,
            plugin: output_plugin,
        };
        let unarmed = FormKey {
            local: 0x6B3E25,
            plugin: output_plugin,
        };
        push_field(&mut record, "WKMV", FieldValue::FormKey(movement));
        push_field(&mut record, "SRAC", FieldValue::FormKey(template_race));
        push_field(&mut record, "UNWP", FieldValue::FormKey(unarmed));
        let attack_data = atkd_bytes(0, -90.0);
        let attack_event = interner.intern("meleeStart_ForwardLeftClaw");
        push_field(&mut record, "ATKD", FieldValue::Bytes(attack_data.clone()));
        push_field(&mut record, "ATKE", FieldValue::String(attack_event));

        let outcome = apply_to_record(&mut record, &interner);

        assert!(outcome.female_behavior_fixed);
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected behavior BodyData rows");
        };
        for item in items {
            assert_eq!(
                interner.resolve(bodydata_modl_sym(item, modl)),
                Some(JERSEY_DEVIL_BEHAVIOR_PROJECT)
            );
        }
        assert_eq!(record.fields[1].value, FieldValue::FormKey(movement));
        assert_eq!(record.fields[2].value, FieldValue::FormKey(template_race));
        assert_eq!(record.fields[3].value, FieldValue::FormKey(unarmed));
        assert_eq!(record.fields[4].value, FieldValue::Bytes(attack_data));
        assert_eq!(record.fields[5].value, FieldValue::String(attack_event));
    }

    #[test]
    fn copies_snallygaster_live_modt_row_group_shape() {
        let mut interner = StringInterner::new();
        let modl = interner.intern("MODL");
        let (mut record, snally_project) =
            snallygaster_live_behavior_bodydata_record(&mut interner, 0x000100, "Output.esp");

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected BodyData list");
        };
        assert_eq!(bodydata_modl_sym(&items[0], modl), snally_project);
        assert_eq!(bodydata_modl_sym(&items[1], modl), snally_project);
    }

    #[test]
    fn copies_nested_two_row_hkx_bodydata_without_inferred_project() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let modl = interner.intern("MODL");
        let male_bhv = interner.intern("Actors\\Mirelurk\\MirelurkProject.hkx");
        let female_bhv = interner.intern("Actors\\Molerat\\MoleratProject.hkx");
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![(
                interner.intern("NestedBodyDatas"),
                FieldValue::List(vec![
                    FieldValue::Struct(vec![(modl, FieldValue::String(male_bhv))]),
                    FieldValue::Struct(vec![(modl, FieldValue::String(female_bhv))]),
                ]),
            )]),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected nested BodyData struct");
        };
        let FieldValue::List(items) = &fields[0].1 else {
            panic!("expected nested BodyData list");
        };
        let FieldValue::Struct(female_fields) = &items[1] else {
            panic!("expected female BodyData struct");
        };
        let (_, FieldValue::String(sym)) = &female_fields[0] else {
            panic!("expected female MODL string");
        };
        assert_eq!(*sym, male_bhv);
    }

    #[test]
    fn copies_all_nested_behavior_bodydata_lists() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let modl = interner.intern("MODL");
        let snally_bhv = interner.intern("Actors\\Snallygaster\\SnallygasterProject.hkx");
        let molerat_bhv = interner.intern("Actors\\Molerat\\MoleratProject.hkx");
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("First"),
                    FieldValue::List(vec![
                        FieldValue::Struct(vec![(modl, FieldValue::String(snally_bhv))]),
                        FieldValue::Struct(vec![(modl, FieldValue::String(molerat_bhv))]),
                    ]),
                ),
                (
                    interner.intern("Second"),
                    FieldValue::List(vec![
                        FieldValue::Struct(vec![(modl, FieldValue::String(snally_bhv))]),
                        FieldValue::Struct(vec![(modl, FieldValue::String(molerat_bhv))]),
                    ]),
                ),
            ]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected BodyData struct");
        };
        for idx in 0..2 {
            let FieldValue::List(items) = &fields[idx].1 else {
                panic!("expected nested list");
            };
            let FieldValue::Struct(female_fields) = &items[1] else {
                panic!("expected female struct");
            };
            let (_, FieldValue::String(sym)) = &female_fields[0] else {
                panic!("expected female MODL string");
            };
            assert_eq!(*sym, snally_bhv);
        }
    }

    #[test]
    fn synthesizes_floater_project_from_single_mosquito_bodydata_row() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let modl = interner.intern("MODL");
        let fnam = interner.intern("FNAM");
        let fallback_bhv = interner.intern("Actors\\Mosquito\\MosquitoProject.hkx");
        push_field(
            &mut record,
            "DATA",
            FieldValue::List(vec![FieldValue::Struct(vec![
                (modl, FieldValue::String(fallback_bhv)),
                (fnam, FieldValue::Bool(true)),
            ])]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Floater\\Behaviors\\FloaterBehavior.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Floater\\Animations")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let project = interner.intern("Actors\\Floater\\FloaterProject.hkx");
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected BodyData list");
        };
        assert_eq!(items.len(), 2);
        let first_sym = bodydata_modl_sym(&items[0], modl);
        assert_eq!(first_sym, project);
        let second_sym = bodydata_modl_sym(&items[1], modl);
        assert_eq!(second_sym, fallback_bhv);
        let FieldValue::Struct(female_fields) = &items[1] else {
            panic!("expected female BodyData struct");
        };
        assert!(female_fields.iter().all(|(key, _)| *key != fnam));
    }

    #[test]
    fn synthesizes_floater_project_from_single_mosquito_bodydata_struct() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let modl = interner.intern("MODL");
        let fallback_bhv = interner.intern("Actors\\Mosquito\\MosquitoProject.hkx");
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![(modl, FieldValue::String(fallback_bhv))]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Floater\\Behaviors\\FloaterCore.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Floater\\Animations")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let project = interner.intern("Actors\\Floater\\FloaterProject.hkx");
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected synthesized BodyData list");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(bodydata_modl_sym(&items[0], modl), project);
        assert_eq!(bodydata_modl_sym(&items[1], modl), fallback_bhv);
    }

    #[test]
    fn restores_megasloth_project_from_single_supermutantbehemoth_bodydata_row() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let modl = interner.intern("MODL");
        let fallback_bhv = interner.intern("Actors\\SuperMutantBehemoth\\SupemutantBehemoth.hkx");
        push_field(
            &mut record,
            "DATA",
            FieldValue::List(vec![FieldValue::Struct(vec![(
                modl,
                FieldValue::String(fallback_bhv),
            )])]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\MegaSloth\\Behaviors\\MegaSlothCore.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\MegaSloth\\Animations")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let project = interner.intern("Actors\\MegaSloth\\MegaSlothProject.hkx");
        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected BodyData list");
        };
        assert_eq!(items.len(), 2);
        assert_eq!(bodydata_modl_sym(&items[0], modl), project);
        assert_eq!(bodydata_modl_sym(&items[1], modl), fallback_bhv);
    }

    #[test]
    fn preserves_sheepsquatch_native_behavior_and_attack_events() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let skel_male = interner.intern("actors\\sheepsquatch\\characterassets\\skeleton.nif");
        let skel_female = interner.intern("Actors\\Deathclaw\\CharacterAssets\\skeleton.nif");
        push_field(&mut record, "MNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(skel_male));
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(skel_female));

        let modl = interner.intern("MODL");
        let male_bhv = interner.intern("Actors\\Sheepsquatch\\SheepsquatchProject.hkx");
        let female_bhv = interner.intern("Actors\\Deathclaw\\DeathclawProject.hkx");
        push_field(
            &mut record,
            "DATA",
            FieldValue::List(vec![
                FieldValue::Struct(vec![(modl, FieldValue::String(male_bhv))]),
                FieldValue::Struct(vec![(modl, FieldValue::String(female_bhv))]),
            ]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Sheepsquatch\\Behaviors\\SheepsquatchBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Sheepsquatch\\Animations")),
        );
        push_field(
            &mut record,
            "WKMV",
            FieldValue::FormKey(FormKey {
                local: 0x001234,
                plugin: interner.intern("Output.esp"),
            }),
        );
        push_field(
            &mut record,
            "UNWP",
            FieldValue::FormKey(FormKey {
                local: 0x004567,
                plugin: interner.intern("Output.esp"),
            }),
        );
        push_field(
            &mut record,
            "ATKE",
            FieldValue::String(interner.intern("meleeAttackStartAOEQuills")),
        );
        push_field(
            &mut record,
            "ATKE",
            FieldValue::String(interner.intern("meleeAttackStartAOEStomp")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(!outcome.fallback_skeleton_promoted);
        assert!(!outcome.fallback_behavior_promoted);
        assert!(!outcome.fallback_runtime_bindings_fixed);
        assert!(outcome.female_anam_fixed);
        assert!(outcome.female_behavior_fixed);
        assert_eq!(outcome.subgraph_paths_normalized, 0);

        let FieldValue::List(items) = &record.fields[4].value else {
            panic!("expected BodyData list");
        };
        for item in items {
            let FieldValue::Struct(fields) = item else {
                panic!("expected BodyData struct");
            };
            let (_, FieldValue::String(sym)) = &fields[0] else {
                panic!("expected MODL string");
            };
            assert_eq!(*sym, male_bhv);
        }

        let FieldValue::String(graph_sym) = record.fields[5].value else {
            panic!("expected SGNM string");
        };
        assert_eq!(
            interner.resolve(graph_sym),
            Some("Actors\\Sheepsquatch\\Behaviors\\SheepsquatchBehavior.hkx")
        );
        let FieldValue::String(path_sym) = record.fields[6].value else {
            panic!("expected SAPT string");
        };
        assert_eq!(
            interner.resolve(path_sym),
            Some("Actors\\Sheepsquatch\\Animations")
        );

        let wkmv_sig = SubrecordSig::from_str("WKMV").unwrap();
        let unwp_sig = SubrecordSig::from_str("UNWP").unwrap();
        assert!(record.fields.iter().any(|entry| {
            entry.sig == wkmv_sig
                && matches!(&entry.value, FieldValue::FormKey(fk) if fk.local == 0x001234 && interner.resolve(fk.plugin) == Some("Output.esp"))
        }));
        assert!(record.fields.iter().any(|entry| {
            entry.sig == unwp_sig
                && matches!(&entry.value, FieldValue::FormKey(fk) if fk.local == 0x004567 && interner.resolve(fk.plugin) == Some("Output.esp"))
        }));
        let attack_events: Vec<&str> = record
            .fields
            .iter()
            .filter(|entry| entry.sig == SubrecordSig::from_str("ATKE").unwrap())
            .filter_map(|entry| match entry.value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            })
            .collect();
        assert_eq!(
            attack_events,
            vec!["meleeAttackStartAOEQuills", "meleeAttackStartAOEStomp"]
        );
    }

    #[test]
    fn copies_nested_behavior_bodydata_model_filename_bytes() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let model_file_name = interner.intern("ModelFileName");
        let male_bhv = smallvec::SmallVec::<[u8; 32]>::from_slice(
            b"Actors\\Snallygaster\\SnallygasterProject.hkx\0",
        );
        let female_bhv =
            smallvec::SmallVec::<[u8; 32]>::from_slice(b"Actors\\Molerat\\MoleratProject.hkx\0");
        push_field(
            &mut record,
            "MODL",
            FieldValue::List(vec![
                FieldValue::Struct(vec![(model_file_name, FieldValue::Bytes(male_bhv.clone()))]),
                FieldValue::Struct(vec![(model_file_name, FieldValue::Bytes(female_bhv))]),
            ]),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.female_behavior_fixed);

        let FieldValue::List(items) = &record.fields[0].value else {
            panic!("expected BodyData list");
        };
        let FieldValue::Struct(fields) = &items[1] else {
            panic!("expected female BodyData struct");
        };
        let (_, FieldValue::Bytes(bytes)) = &fields[0] else {
            panic!("expected female ModelFileName bytes");
        };
        assert_eq!(bytes, &male_bhv);
    }

    #[test]
    fn shared_character_subgraphs_remain_canonical_for_creature_races() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "ANAM",
            FieldValue::String(interner.intern("Actors\\MoleMiner\\CharacterAssets\\skeleton.nif")),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Character\\Behaviors\\MTBehavior.hkx")),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Character\\Behaviors\\MeleeBehavior.hkx")),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Character\\Behaviors\\GunBehavior.hkx")),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Shared\\Behaviors\\AmbushBehavior.hkx")),
        );

        let outcome = apply_to_record(&mut record, &interner);

        assert!(!outcome.changed());
        let paths: Vec<&str> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "SGNM")
            .filter_map(|entry| match entry.value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            })
            .collect();
        assert_eq!(
            paths,
            vec![
                "Actors\\Character\\Behaviors\\MTBehavior.hkx",
                "Actors\\Character\\Behaviors\\MeleeBehavior.hkx",
                "Actors\\Character\\Behaviors\\GunBehavior.hkx",
                "Actors\\Shared\\Behaviors\\AmbushBehavior.hkx",
            ]
        );
    }

    #[test]
    fn fo76_gun_subgraphs_normalize_to_canonical_fo4_weapon_graphs() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let fallout4 = interner.intern("Fallout4.esm");
        let right_injured = FormKey {
            local: 0x030B00,
            plugin: fallout4,
        };
        let left_injured = FormKey {
            local: 0x030B01,
            plugin: fallout4,
        };

        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Character\\Behaviors\\GunBehavior.hkx")),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        push_field(&mut record, "SAKD", FieldValue::FormKey(right_injured));
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Character\\Behaviors\\LegInjuredGunWrappingBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        push_field(&mut record, "SAKD", FieldValue::FormKey(left_injured));
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Character\\Behaviors\\LegInjuredGunWrappingBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        push_field(&mut record, "SAKD", FieldValue::FormKey(right_injured));
        push_field(&mut record, "SAKD", FieldValue::FormKey(left_injured));
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Character\\Behaviors\\LegInjuredGunWrappingBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Character\\Behaviors\\NoHandIKGunWrappingBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );

        let changed = normalize_fo76_weapon_subgraphs(&mut record, &interner);

        assert_eq!(changed, 5);
        let paths: Vec<&str> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "SGNM")
            .filter_map(|entry| match entry.value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            })
            .collect();
        assert_eq!(
            paths,
            vec![
                "Actors\\Character\\Behaviors\\WeaponBehavior.hkx",
                "Actors\\Character\\Behaviors\\RightArmInjuredWeaponWrappingBehavior.hkx",
                "Actors\\Character\\Behaviors\\LeftArmInjuredWeaponWrappingBehavior.hkx",
                "Actors\\Character\\Behaviors\\BothArmInjuredWeaponWrappingBehavior.hkx",
                "Actors\\Character\\Behaviors\\NoHandIKWeaponWrappingBehavior.hkx",
            ]
        );
        assert_eq!(
            normalize_fo76_weapon_subgraphs(&mut record, &interner),
            0,
            "normalization must be idempotent"
        );
    }

    #[test]
    fn first_person_gun_subgraphs_keep_the_fo4_gun_graph() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let third_person = interner.intern("Actors\\Character\\Behaviors\\GunBehavior.hkx");
        let first_person =
            interner.intern("Actors\\Character\\_1stPerson\\Behaviors\\GunBehavior.hkx");
        push_field(&mut record, "SGNM", FieldValue::String(third_person));
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        push_field(&mut record, "SGNM", FieldValue::String(first_person));
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1, 0, 1, 0])),
        );

        let changed = normalize_fo76_weapon_subgraphs(&mut record, &interner);

        assert_eq!(changed, 1);
        let paths: Vec<&str> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "SGNM")
            .filter_map(|entry| match entry.value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            })
            .collect();
        assert_eq!(
            paths,
            vec![
                "Actors\\Character\\Behaviors\\WeaponBehavior.hkx",
                "Actors\\Character\\_1stPerson\\Behaviors\\GunBehavior.hkx",
            ]
        );
    }

    #[test]
    fn local_gun_subgraph_normalizes_without_a_local_weapon_copy() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\MoleMiner\\Behaviors\\GunBehavior.hkx")),
        );

        let changed = normalize_fo76_weapon_subgraphs(&mut record, &interner);

        assert_eq!(changed, 1);
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.as_str() == "SGNM")
                .and_then(|entry| match entry.value {
                    FieldValue::String(sym) => interner.resolve(sym),
                    _ => None,
                }),
            Some("Actors\\Character\\Behaviors\\WeaponBehavior.hkx")
        );
    }

    #[test]
    fn normalizes_ambushhole_only_subgraph_path_to_base_animation_path() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\SnallygasterProject.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations\\AmbushHole")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_paths_normalized, 1);

        let FieldValue::String(sym) = record.fields[1].value else {
            panic!("expected SAPT string");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Actors\\Snallygaster\\Animations")
        );
    }

    #[test]
    fn drops_snallygaster_ambushhole_subgraph_when_base_path_exists() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations\\AmbushHole")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_blocks_dropped, 1);
        assert_eq!(record.fields.len(), 2);

        let FieldValue::String(sym) = record.fields[1].value else {
            panic!("expected SAPT string");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Actors\\Snallygaster\\Animations")
        );
    }

    #[test]
    fn strips_subgraph_block_with_target_keywords() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        // Block 1: normal — kept
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\NormalGraph.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("some/path.hkx")),
        );
        // Block 2: has STKD — dropped
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\AmbushGraph.hkx")),
        );
        let mut stkd_payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        stkd_payload.extend_from_slice(&0x00_001234_u32.to_le_bytes());
        push_field(&mut record, "STKD", FieldValue::Bytes(stkd_payload));
        // Block 3: another normal — kept
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\OtherGraph.hkx")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_blocks_dropped, 1);

        // Remaining: SGNM(normal), SAPT, SGNM(other).
        assert_eq!(record.fields.len(), 3);
        assert_eq!(record.fields[0].sig.as_str(), "SGNM");
        assert_eq!(record.fields[1].sig.as_str(), "SAPT");
        assert_eq!(record.fields[2].sig.as_str(), "SGNM");
        // The kept SGNMs must not be the ambush one.
        if let FieldValue::String(s) = record.fields[0].value {
            assert_eq!(interner.resolve(s), Some("Actors\\NormalGraph.hkx"));
        }
        if let FieldValue::String(s) = record.fields[2].value {
            assert_eq!(interner.resolve(s), Some("Actors\\OtherGraph.hkx"));
        }
    }

    #[test]
    fn strips_target_keywords_on_snallygaster_base_subgraph() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );
        let mut stkd_payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        stkd_payload.extend_from_slice(&0x00_001234_u32.to_le_bytes());
        push_field(&mut record, "STKD", FieldValue::Bytes(stkd_payload));
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_blocks_dropped, 0);
        assert_eq!(outcome.subgraph_target_keywords_stripped, 1);

        assert_eq!(record.fields.len(), 3);
        assert_eq!(record.fields[0].sig.as_str(), "SGNM");
        assert_eq!(record.fields[1].sig.as_str(), "SAPT");
        assert_eq!(record.fields[2].sig.as_str(), "SRAF");
    }

    #[test]
    fn strips_target_keywords_on_core_base_animation_subgraph() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\ScorchBeast\\Behaviors\\ScorchBeastCore.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\ScorchBeast\\Animations\\sbQueen")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\ScorchBeast\\Animations")),
        );
        let mut stkd_payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        stkd_payload.extend_from_slice(&0x00_001234_u32.to_le_bytes());
        push_field(&mut record, "STKD", FieldValue::Bytes(stkd_payload));
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x206A66,
                plugin: interner.intern("SeventySix.esm"),
            }),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_blocks_dropped, 0);
        assert_eq!(outcome.subgraph_target_keywords_stripped, 1);

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["SGNM", "SAPT", "SAPT", "SAKD"]);
    }

    #[test]
    fn preserves_shared_ambush_subgraph_with_actor_animation_target_keywords() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Shared\\Behaviors\\AmbushBehavior.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\MegaSloth\\Animations\\AmbushTree")),
        );
        push_field(
            &mut record,
            "STKD",
            FieldValue::FormKey(FormKey {
                local: 0x002012,
                plugin: interner.intern("Output.esp"),
            }),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_blocks_dropped, 0);
        assert_eq!(outcome.subgraph_target_keywords_stripped, 0);
        assert_eq!(sigs(&record), vec!["SGNM", "SAPT", "STKD", "SRAF"]);
    }

    #[test]
    fn preserves_target_keyword_after_sraf_for_following_subgraph() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\MegaSloth\\Behaviors\\MegaSlothCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\MegaSloth\\Animations")),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        push_field(
            &mut record,
            "STKD",
            FieldValue::FormKey(FormKey {
                local: 0x01A8F4,
                plugin: interner.intern("Output.esp"),
            }),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Shared\\Behaviors\\AmbushBehavior.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\MegaSloth\\Animations\\AmbushSleeping")),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_blocks_dropped, 0);
        assert_eq!(outcome.subgraph_target_keywords_stripped, 0);
        assert_eq!(
            sigs(&record),
            vec!["SGNM", "SAPT", "SRAF", "STKD", "SGNM", "SAPT", "SRAF"]
        );
    }

    #[test]
    fn drops_snallygaster_ambushhole_and_strips_base_target_keyword_shape() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let graph =
            interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx");

        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );
        let mut stkd_payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        stkd_payload.extend_from_slice(&0x00_00C301_u32.to_le_bytes());
        push_field(&mut record, "STKD", FieldValue::Bytes(stkd_payload));
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );

        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations\\AmbushHole")),
        );
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x030B01,
                plugin: interner.intern("Fallout4.esm"),
            }),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_blocks_dropped, 1);
        assert_eq!(outcome.subgraph_target_keywords_stripped, 1);

        let stkd_sig = SubrecordSig::from_str("STKD").unwrap();
        assert!(!record.fields.iter().any(|field| field.sig == stkd_sig));
        assert!(!record.fields.iter().any(|field| {
            field.sig.as_str() == "SAPT"
                && matches!(
                    extract_zstring(&field.value, &interner).as_deref(),
                    Some("Actors\\Snallygaster\\Animations\\AmbushHole")
                )
        }));
        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["SGNM", "SAPT", "SRAF"]);
    }

    /// Real FO76 subrecord order: a subgraph's SAKD/STKD gate sits BETWEEN the
    /// previous block's SRAF and its own SGNM. Dropping the AmbushHole block
    /// must take its preceding STKD gate with it — a leaked STKD survives
    /// normalization and re-attaches to whichever block follows Base, gating
    /// it on a furniture keyword the actor never carries (snallygaster T-pose
    /// root cause).
    #[test]
    fn dropping_ambushhole_block_also_drops_its_preceding_target_keyword_gate() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let graph =
            interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx");
        let base_path = interner.intern("Actors\\Snallygaster\\Animations");
        let fo76 = interner.intern("SeventySix.esm");

        // Base
        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(&mut record, "SAPT", FieldValue::String(base_path));
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        // AmbushHole, gated by STKD AnimFurnAmbushHole
        push_field(
            &mut record,
            "STKD",
            FieldValue::FormKey(FormKey {
                local: 0x00C301,
                plugin: fo76,
            }),
        );
        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations\\AmbushHole")),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        // Injured\LeftLeg, gated by one SAKD
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x030B01,
                plugin: fo76,
            }),
        );
        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Animations\\Injured\\LeftLeg"),
            ),
        );
        push_field(&mut record, "SAPT", FieldValue::String(base_path));
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        // Injured\BothLegs, gated by two SAKDs
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x030B01,
                plugin: fo76,
            }),
        );
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x030B00,
                plugin: fo76,
            }),
        );
        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Animations\\Injured\\BothLegs"),
            ),
        );
        push_field(&mut record, "SAPT", FieldValue::String(base_path));
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        // Injured\RightLeg, gated by one SAKD
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x030B00,
                plugin: fo76,
            }),
        );
        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Animations\\Injured\\RightLeg"),
            ),
        );
        push_field(&mut record, "SAPT", FieldValue::String(base_path));
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_blocks_dropped, 1);

        let stkd_sig = SubrecordSig::from_str("STKD").unwrap();
        assert!(
            !record.fields.iter().any(|field| field.sig == stkd_sig),
            "orphaned STKD must be dropped with the AmbushHole block"
        );
        assert!(!record.fields.iter().any(|field| {
            field.sig.as_str() == "SAPT"
                && matches!(
                    extract_zstring(&field.value, &interner).as_deref(),
                    Some("Actors\\Snallygaster\\Animations\\AmbushHole")
                )
        }));

        // Fan-parity shape: Base, RightLeg, LeftLeg, BothLegs with trailing
        // SAKD gates [030B00], [030B01], [030B01, 030B00], [].
        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec![
                "SGNM", "SAPT", "SRAF", "SAKD", //
                "SGNM", "SAPT", "SAPT", "SRAF", "SAKD", //
                "SGNM", "SAPT", "SAPT", "SRAF", "SAKD", "SAKD", //
                "SGNM", "SAPT", "SAPT", "SRAF",
            ]
        );
        let sakd_locals: Vec<u32> = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "SAKD")
            .map(|field| match &field.value {
                FieldValue::FormKey(fk) => fk.local,
                other => panic!("expected FormKey SAKD, got {other:?}"),
            })
            .collect();
        assert_eq!(sakd_locals, vec![0x030B00, 0x030B01, 0x030B01, 0x030B00]);
    }

    #[test]
    fn normalizes_snallygaster_subgraph_keyword_routing_to_fan_shape() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        let graph =
            interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx");

        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );

        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Animations\\Injured\\LeftLeg"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x030B01,
                plugin: interner.intern("Fallout4.esm"),
            }),
        );
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x030B00,
                plugin: interner.intern("Fallout4.esm"),
            }),
        );

        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Animations\\Injured\\BothLegs"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );
        push_field(
            &mut record,
            "SAKD",
            FieldValue::FormKey(FormKey {
                local: 0x030B00,
                plugin: interner.intern("Fallout4.esm"),
            }),
        );

        push_field(&mut record, "SGNM", FieldValue::String(graph));
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Animations\\Injured\\RightLeg"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("Actors\\Snallygaster\\Animations")),
        );
        push_field(
            &mut record,
            "SRAF",
            FieldValue::Bytes(smallvec::SmallVec::new()),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.subgraph_keywords_normalized, 3);

        let blocks = identify_subgraph_blocks(&record, SubrecordSig::from_str("SGNM").unwrap());
        let observed: Vec<(SnallygasterSubgraphKind, Vec<u32>)> = blocks
            .iter()
            .map(|block| {
                let kind = snallygaster_subgraph_kind(&record, block, &interner)
                    .expect("expected Snallygaster subgraph kind");
                let keywords = record.fields[block.start..block.end]
                    .iter()
                    .filter_map(|field| match field.value {
                        FieldValue::FormKey(fk) if field.sig.as_str() == "SAKD" => Some(fk.local),
                        _ => None,
                    })
                    .collect();
                (kind, keywords)
            })
            .collect();

        assert_eq!(
            observed,
            vec![
                (SnallygasterSubgraphKind::Base, vec![0x030B00]),
                (SnallygasterSubgraphKind::InjuredRight, vec![0x030B01]),
                (
                    SnallygasterSubgraphKind::InjuredLeft,
                    vec![0x030B01, 0x030B00]
                ),
                (SnallygasterSubgraphKind::InjuredBoth, Vec::new()),
            ]
        );
    }

    #[test]
    fn collapses_adjacent_duplicate_subgraphs() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\Graph.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("path1.hkx")),
        );
        // Adjacent duplicate (case + slash variants match normalize).
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("actors/Graph.hkx")),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(interner.intern("PATH1.hkx")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.duplicate_graphs_collapsed, 1);

        assert_eq!(record.fields.len(), 2);
        assert_eq!(record.fields[0].sig.as_str(), "SGNM");
        assert_eq!(record.fields[1].sig.as_str(), "SAPT");
    }

    #[test]
    fn preserves_same_graph_with_different_sapt_chain() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Animations\\Injured\\RightLeg"),
            ),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(
                interner.intern("actors/snallygaster/behaviors/snallygastercorebehavior.hkx"),
            ),
        );
        push_field(
            &mut record,
            "SAPT",
            FieldValue::String(
                interner.intern("Actors\\Snallygaster\\Animations\\Injured\\LeftLeg"),
            ),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.duplicate_graphs_collapsed, 0);
        assert_eq!(record.fields.len(), 4);
    }

    #[test]
    fn non_adjacent_duplicates_preserved() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\GraphA.hkx")),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\GraphB.hkx")),
        );
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("Actors\\GraphA.hkx")),
        );

        let outcome = apply_to_record(&mut record, &interner);
        assert_eq!(outcome.duplicate_graphs_collapsed, 0);
        assert_eq!(record.fields.len(), 3);
    }

    #[test]
    fn applies_multiple_fixes_in_one_record() {
        let mut interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esp", &mut interner);
        // ATKD with 0x40 and angle=0
        push_field(
            &mut record,
            "ATKD",
            FieldValue::Bytes(atkd_bytes(0x44, 0.0)),
        );
        // ATKE with backward direction
        push_field(
            &mut record,
            "ATKE",
            FieldValue::String(interner.intern("AttackBackwardSwing")),
        );
        // Skeletal model mismatch
        let male = interner.intern("male.nif");
        let female = interner.intern("female.nif");
        push_field(&mut record, "MNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(male));
        push_field(&mut record, "FNAM", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::String(female));
        // Subgraph with STKD
        push_field(
            &mut record,
            "SGNM",
            FieldValue::String(interner.intern("ambush.hkx")),
        );
        let mut stkd: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        stkd.extend_from_slice(&0x00_001234_u32.to_le_bytes());
        push_field(&mut record, "STKD", FieldValue::Bytes(stkd));

        let outcome = apply_to_record(&mut record, &interner);
        assert!(outcome.changed());
        assert_eq!(outcome.atkd_flags_stripped, 1);
        assert_eq!(outcome.atkd_angles_injected, 1);
        assert!(outcome.female_anam_fixed);
        assert_eq!(outcome.subgraph_blocks_dropped, 1);

        // Verify ATKD flags cleared and angle set.
        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            assert_eq!(read_atkd_flags(data), 0x04);
            assert_eq!(read_atkd_angle(data), 180.0);
        }
        // Verify female ANAM = male.
        if let FieldValue::String(s) = record.fields[5].value {
            assert_eq!(s, male);
        }
        // Verify subgraph block stripped (no SGNM/STKD left).
        let sgnm_sig = SubrecordSig::from_str("SGNM").unwrap();
        let stkd_sig = SubrecordSig::from_str("STKD").unwrap();
        assert!(record.fields.iter().all(|e| e.sig != sgnm_sig));
        assert!(record.fields.iter().all(|e| e.sig != stkd_sig));
    }

    #[test]
    fn registry_runs_npc_root_no_op_when_no_race_records() {
        let (_, schema, config) = make_creature_config();
        let target_handle = plugin_handle_new_native("FixCreatureRaceRecordsTest.esp", Some("fo4"))
            .expect("test plugin handle");
        let mut mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");
        let mut registry = FixupRegistry::new();
        registry.register(Box::new(FixCreatureRaceRecordsFixup));
        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all_in_session");
        assert_eq!(reports.len(), 1);
        assert!(reports[0].1.is_no_op());
    }

    #[test]
    fn registry_links_owned_floater_movement_record() {
        let (_, schema, config) = make_creature_config();
        let target_handle = plugin_handle_new_native("FixCreatureRaceRecordsTest.esp", Some("fo4"))
            .expect("test plugin handle");
        insert_parsed_record(target_handle, parsed_floater_race_without_movement())
            .expect("seed RACE");
        insert_parsed_record(target_handle, parsed_floater_default_movement()).expect("seed MOVT");

        let mut mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");
        let mut registry = FixupRegistry::new();
        registry.register(Box::new(FixCreatureRaceRecordsFixup));
        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all_in_session");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].1.records_changed, 1);

        let race_fk = FormKey {
            local: 0x000100,
            plugin: mapper.interner.intern("FixCreatureRaceRecordsTest.esp"),
        };
        let movement_fk = FormKey {
            local: 0x000200,
            plugin: mapper.interner.intern("FixCreatureRaceRecordsTest.esp"),
        };
        let repaired = session
            .record_decoded(&race_fk, schema.as_ref(), mapper.interner)
            .expect("decode repaired RACE");
        for sig in ["WKMV", "FLMV"] {
            assert!(repaired.fields.iter().any(|entry| {
                entry.sig.as_str() == sig
                    && matches!(entry.value, FieldValue::FormKey(fk) if fk == movement_fk)
            }));
        }
    }

    #[test]
    fn registry_links_owned_movement_for_humanoid_converted_race() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let plugin_name = "FixScorchedRaceMovementTest.esp";
        let target_handle =
            plugin_handle_new_native(plugin_name, Some("fo4")).expect("test plugin handle");
        let source_flags: u32 = 1 | 2 | 512 | 8192 | 16384 | 0xF8FF_8000;
        insert_parsed_record(
            target_handle,
            parsed_race_with_equipment_flags(
                0x0000_0100,
                "ScorchedRace",
                ACTOR_TYPE_NPC_LOW24,
                source_flags,
            ),
        )
        .expect("seed humanoid RACE");
        insert_parsed_record(target_handle, parsed_scorched_default_movement()).expect("seed MOVT");

        let interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &interner);
        let config = FixupConfig {
            is_whole_plugin: true,
            target_schema: Some(schema.clone()),
            ..Default::default()
        };
        let mut session = open_session(target_handle, None).expect("open session");
        let mut registry = FixupRegistry::new();
        registry.register(Box::new(FixCreatureRaceRecordsFixup));
        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run whole-plugin creature RACE fixup");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].1.records_changed, 1);

        let repaired = session
            .record_decoded(
                &FormKey {
                    local: 0x000100,
                    plugin: interner.intern(plugin_name),
                },
                schema.as_ref(),
                &interner,
            )
            .expect("decode repaired RACE");
        let movement_fk = FormKey {
            local: 0x000201,
            plugin: interner.intern(plugin_name),
        };
        assert!(repaired.fields.iter().any(|entry| {
            entry.sig.as_str() == "WKMV"
                && matches!(entry.value, FieldValue::FormKey(fk) if fk == movement_fk)
        }));
        assert!(repaired.fields.iter().any(|entry| {
            if entry.sig.as_str() != "VNAM" {
                return false;
            }
            match &entry.value {
                FieldValue::Uint(value) => *value as u32 == source_flags,
                FieldValue::Bytes(data) if data.len() >= 4 => {
                    u32::from_le_bytes(data[..4].try_into().unwrap()) == source_flags
                }
                _ => false,
            }
        }));
        drop(session);
        assert!(plugin_handle_close_native(target_handle));
    }

    #[test]
    fn registry_whole_plugin_preserves_h2h_only_on_creature_races() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let plugin_name = "FixCreatureRaceEquipmentFlagsTest.esp";
        let target_handle =
            plugin_handle_new_native(plugin_name, Some("fo4")).expect("test plugin handle");
        let source_flags: u32 = 1 | 2 | 512 | 8192 | 16384 | 0xF8FF_8000;
        insert_parsed_record(
            target_handle,
            parsed_race_with_equipment_flags(
                0x0000_0100,
                "ScorchBeastRace",
                ACTOR_TYPE_CREATURE_LOW24,
                source_flags,
            ),
        )
        .expect("seed creature RACE");
        insert_parsed_record(
            target_handle,
            parsed_race_with_equipment_flags(
                0x0000_0101,
                "HumanRace",
                ACTOR_TYPE_NPC_LOW24,
                source_flags,
            ),
        )
        .expect("seed humanoid RACE");
        insert_parsed_record(
            target_handle,
            parsed_race_with_equipment_flags(
                0x0000_0102,
                "HumanRaceAdditive",
                ACTOR_TYPE_CREATURE_LOW24,
                source_flags,
            ),
        )
        .expect("seed additive RACE");

        let interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &interner);
        let config = FixupConfig {
            is_whole_plugin: true,
            target_schema: Some(schema.clone()),
            ..Default::default()
        };
        {
            let mut session = open_session(target_handle, None).expect("open session");
            let mut registry = FixupRegistry::new();
            registry.register(Box::new(FixCreatureRaceRecordsFixup));
            let reports = registry
                .run_all_in_session(&mut session, &mut mapper, &config)
                .expect("run whole-plugin creature RACE fixup");
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].1.records_changed, 1);

            let mut read_equipment_flags = |local| {
                let record = session
                    .record_decoded(
                        &FormKey {
                            local,
                            plugin: interner.intern(plugin_name),
                        },
                        schema.as_ref(),
                        &interner,
                    )
                    .expect("decode RACE");
                record
                    .fields
                    .iter()
                    .find_map(|entry| {
                        if entry.sig.as_str() != "VNAM" {
                            return None;
                        }
                        match &entry.value {
                            FieldValue::Uint(value) => Some(*value as u32),
                            FieldValue::Bytes(data) if data.len() >= 4 => {
                                Some(u32::from_le_bytes(data[..4].try_into().unwrap()))
                            }
                            _ => None,
                        }
                    })
                    .expect("RACE.VNAM equipment flags")
            };

            let creature_flags = read_equipment_flags(0x0000_0100);
            assert_eq!(
                creature_flags & 1,
                0,
                "body-fighting creature must NOT keep H2H: 0/22 vanilla FO4 \
                 creature races set it, and it blocks melee initiation"
            );
            assert_eq!(
                creature_flags & 2,
                0,
                "unsupported creature bit is filtered"
            );
            assert_eq!(read_equipment_flags(0x0000_0101), source_flags);
            assert_eq!(read_equipment_flags(0x0000_0102), source_flags);
        }
        assert!(plugin_handle_close_native(target_handle));
    }

    #[test]
    fn registry_whole_plugin_repairs_snallygaster_live_bodydata_row_group() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let target_handle = plugin_handle_new_native("FixCreatureRaceRecordsTest.esp", Some("fo4"))
            .expect("test plugin handle");
        insert_parsed_record(
            target_handle,
            parsed_snallygaster_race_with_molerat_project(),
        )
        .expect("seed RACE");

        let mut mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let config = FixupConfig {
            is_whole_plugin: true,
            target_schema: Some(schema.clone()),
            ..Default::default()
        };
        let mut session = open_session(target_handle, None).expect("open session");
        let mut registry = FixupRegistry::new();
        registry.register(Box::new(FixCreatureRaceRecordsFixup));
        let fk = FormKey {
            local: 0x000100,
            plugin: mapper.interner.intern("FixCreatureRaceRecordsTest.esp"),
        };
        let before = session
            .record_decoded(&fk, schema.as_ref(), mapper.interner)
            .expect("decode seeded RACE");
        assert!(
            infer_creature_project_path_from_subgraphs(&before, mapper.interner).is_some(),
            "seeded RACE has no inferable Snallygaster project path: {:#?}",
            before.fields
        );
        let reports = registry
            .run_all_in_session(&mut session, &mut mapper, &config)
            .expect("run_all_in_session");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].1.records_changed, 1);

        let repaired = session
            .record_decoded(&fk, schema.as_ref(), mapper.interner)
            .expect("decode repaired RACE");
        let modl_sig = SubrecordSig::from_str("MODL").unwrap();
        let paths: Vec<String> = repaired
            .fields
            .iter()
            .filter(|entry| entry.sig == modl_sig)
            .filter_map(|entry| path_from_field_value(&entry.value, mapper.interner))
            .collect();
        assert_eq!(
            paths
                .iter()
                .filter(|path| path
                    .eq_ignore_ascii_case("Actors\\Snallygaster\\SnallygasterProject.hkx"))
                .count(),
            2
        );
        assert!(
            !paths
                .iter()
                .any(|path| path.eq_ignore_ascii_case("Actors\\Molerat\\MoleratProject.hkx"))
        );
    }

    // -- Fix 7: role-0 locomotion blocks need the creature's H2H path --------

    /// `SRAF` is `struct:H,H` — role, perspective.
    fn sraf(role: u16) -> FieldValue {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        data.extend_from_slice(&role.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        FieldValue::Bytes(data)
    }

    fn push_subgraph_block(
        record: &mut Record,
        interner: &StringInterner,
        graph: &str,
        paths: &[&str],
        role: u16,
    ) {
        push_field(record, "SGNM", FieldValue::String(interner.intern(graph)));
        for path in paths {
            push_field(record, "SAPT", FieldValue::String(interner.intern(path)));
        }
        push_field(record, "SRAF", sraf(role));
    }

    fn subgraph_paths(record: &Record, interner: &StringInterner) -> Vec<String> {
        let sapt = SubrecordSig::from_str("SAPT").unwrap();
        record
            .fields
            .iter()
            .filter(|entry| entry.sig == sapt)
            .filter_map(|entry| match entry.value {
                FieldValue::String(sym) => interner.resolve(sym).map(str::to_string),
                _ => None,
            })
            .collect()
    }

    /// MoleMiner's shape: locomotion lists only `MT`/`Shared`, and H2H exists
    /// but is reachable only from the weapon-role melee block.
    fn mole_miner_shaped_race(interner: &StringInterner) -> Record {
        let mut record = make_race(0x012E6B, "Output.esm", interner);
        push_subgraph_block(
            &mut record,
            interner,
            "Actors\\Character\\Behaviors\\MTBehavior.hkx",
            &[
                "Actors\\MoleMiner\\Animations\\MT",
                "Actors\\MoleMiner\\Animations\\Shared",
            ],
            0,
        );
        push_subgraph_block(
            &mut record,
            interner,
            "Actors\\Character\\Behaviors\\MeleeBehavior.hkx",
            &[
                "Actors\\MoleMiner\\Animations\\H2H",
                "Actors\\MoleMiner\\Animations\\Shared",
            ],
            1,
        );
        record
    }

    #[test]
    fn locomotion_block_gains_the_creature_h2h_path_before_shared() {
        let interner = StringInterner::new();
        let mut record = mole_miner_shaped_race(&interner);

        let mut outcome = RaceFixOutcome::default();
        add_h2h_path_to_locomotion_blocks(&mut record, &interner, &mut outcome);

        assert_eq!(outcome.locomotion_h2h_paths_added, 1);
        assert!(outcome.changed());
        // FO4 orders the role-0 list MT…, H2H, Shared. The weapon-role melee
        // block must be untouched.
        assert_eq!(
            subgraph_paths(&record, &interner),
            vec![
                "Actors\\MoleMiner\\Animations\\MT".to_string(),
                "Actors\\MoleMiner\\Animations\\H2H".to_string(),
                "Actors\\MoleMiner\\Animations\\Shared".to_string(),
                "Actors\\MoleMiner\\Animations\\H2H".to_string(),
                "Actors\\MoleMiner\\Animations\\Shared".to_string(),
            ]
        );
    }

    #[test]
    fn source_behaviors_keep_the_mole_miner_locomotion_paths() {
        let interner = StringInterner::new();
        let mut record = mole_miner_shaped_race(&interner);
        let paths = subgraph_paths(&record, &interner);
        let outcome = apply_to_record_with_source_behaviors(&mut record, &interner, true);
        assert_eq!(outcome.locomotion_h2h_paths_added, 0);
        assert_eq!(subgraph_paths(&record, &interner), paths);
    }

    #[test]
    fn every_locomotion_block_is_repaired_not_just_the_first() {
        let interner = StringInterner::new();
        let mut record = mole_miner_shaped_race(&interner);
        // MoleMiner's second role-0 block: the TreasureHunter variant.
        push_subgraph_block(
            &mut record,
            &interner,
            "Actors\\Character\\Behaviors\\MTBehavior.hkx",
            &[
                "Actors\\MoleMiner\\Animations\\TreasureHunter",
                "Actors\\MoleMiner\\Animations\\MT",
                "Actors\\MoleMiner\\Animations\\Shared",
            ],
            0,
        );

        let mut outcome = RaceFixOutcome::default();
        add_h2h_path_to_locomotion_blocks(&mut record, &interner, &mut outcome);

        assert_eq!(outcome.locomotion_h2h_paths_added, 2);
        let paths = subgraph_paths(&record, &interner);
        assert_eq!(
            &paths[paths.len() - 4..],
            &[
                "Actors\\MoleMiner\\Animations\\TreasureHunter".to_string(),
                "Actors\\MoleMiner\\Animations\\MT".to_string(),
                "Actors\\MoleMiner\\Animations\\H2H".to_string(),
                "Actors\\MoleMiner\\Animations\\Shared".to_string(),
            ]
        );
    }

    #[test]
    fn races_that_already_wire_h2h_into_locomotion_are_left_alone() {
        // Converted Scorched: H2H already leads the role-0 block, and its
        // injured-on-ground role-0 variant deliberately carries none. Adding a
        // row to that variant would regress a creature that already chases.
        let interner = StringInterner::new();
        let mut record = make_race(0x10CA5F, "Output.esm", &interner);
        push_subgraph_block(
            &mut record,
            &interner,
            "Actors\\Character\\Behaviors\\MTBehavior.hkx",
            &[
                "Actors\\Scorched\\Animations\\H2H",
                "Actors\\Character\\Animations\\MT\\Neutral",
            ],
            0,
        );
        push_subgraph_block(
            &mut record,
            &interner,
            "Actors\\Character\\Behaviors\\MTBehavior.hkx",
            &[
                "Actors\\Character\\Animations\\MT\\Injured\\OnGround",
                "Actors\\Character\\Animations\\MT\\Neutral",
            ],
            0,
        );
        let before = subgraph_paths(&record, &interner);

        let mut outcome = RaceFixOutcome::default();
        add_h2h_path_to_locomotion_blocks(&mut record, &interner, &mut outcome);

        assert_eq!(outcome.locomotion_h2h_paths_added, 0);
        assert_eq!(subgraph_paths(&record, &interner), before);
    }

    #[test]
    fn a_race_that_ships_no_h2h_folder_gains_nothing() {
        // The path is only ever reused from one the race already declares, so a
        // creature without an H2H folder cannot be given a dangling row.
        let interner = StringInterner::new();
        let mut record = make_race(0x000100, "Output.esm", &interner);
        push_subgraph_block(
            &mut record,
            &interner,
            "Actors\\Character\\Behaviors\\MTBehavior.hkx",
            &[
                "Actors\\Mirelurk\\Animations\\MT",
                "Actors\\Mirelurk\\Animations\\Shared",
            ],
            0,
        );
        let before = subgraph_paths(&record, &interner);

        let mut outcome = RaceFixOutcome::default();
        add_h2h_path_to_locomotion_blocks(&mut record, &interner, &mut outcome);

        assert_eq!(outcome.locomotion_h2h_paths_added, 0);
        assert_eq!(subgraph_paths(&record, &interner), before);
    }

    #[test]
    fn repair_is_idempotent_across_repeated_runs() {
        let interner = StringInterner::new();
        let mut record = mole_miner_shaped_race(&interner);

        let mut first = RaceFixOutcome::default();
        add_h2h_path_to_locomotion_blocks(&mut record, &interner, &mut first);
        let after_first = subgraph_paths(&record, &interner);

        let mut second = RaceFixOutcome::default();
        add_h2h_path_to_locomotion_blocks(&mut record, &interner, &mut second);

        assert_eq!(second.locomotion_h2h_paths_added, 0);
        assert_eq!(subgraph_paths(&record, &interner), after_first);
    }
}
