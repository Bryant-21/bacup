//! Synthesize additive Race records for weapon/armor conversions.
//!
//! A full override of a base-game Race (HumanRace, PowerArmorRace, ...) crashes
//! the game. Instead, each target RACE with a vanilla equivalent yields a new
//! additive RACE holding only the subgraph blocks whose `target_keywords` (STKD)
//! match the converted WEAP/ARMO/OMOD animation keywords, with `SADD` pointing at
//! the base race or its `FO4_ADDITIVE_PARENTS` parent. Block refs still pointing
//! at source masters are dropped, and blocks a base-game/DLC RACE already
//! provides are skipped. When the source has no matching PowerArmorRace blocks,
//! a `PowerArmorRaceAdditive` is derived from each new HumanRace additive.
//!
//! No-op for creature roots (NPC_/LVLN), which need full-clone races.
//! `load_target_race_template` is a stub, so the source race is the template.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::anim_namespace::{
    animation_set_path_key, is_namespaced_anim_path, namespaced_anim_path,
};
use crate::fixups::face::build_additive_race_record::{
    SubgraphBlock, build_additive_race_record, parse_canonical_subgraphs,
    strip_template_subgraph_fields,
};
use crate::fixups::face::derive_pa_subgraph_blocks::derive_pa_subgraph_blocks;
use crate::fixups::face::rewrite_subgraph_block_formkeys::rewrite_subgraph_block_formkeys;
use crate::fixups::prune_orphaned_records::is_creature_root_sig;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Map of source RACE EditorID → target-game additive-parent FormKey rendered
/// as `OBJID:Plugin.esm`.
const FO4_ADDITIVE_PARENTS: &[(&str, &str)] = &[
    // HumanRace -> HumanRaceSubGraphData (Fallout4.esm:166729).
    ("HumanRace", "166729:Fallout4.esm"),
    // PowerArmorRace itself (Fallout4.esm:01D31E).
    ("PowerArmorRace", "01D31E:Fallout4.esm"),
];

/// Map of source RACE EditorID → normalized prefix for additive EditorID
/// composition.
const RACE_EID_NORMALIZE: &[(&str, &str)] = &[
    // HumanRaceSubGraphData -> HumanRace; everything else passes through.
    ("HumanRaceSubGraphData", "HumanRace"),
];

const SYNTHETIC_ADDITIVE_RACE_PLUGIN: &str = "__fo76_to_fo4_additive_race__";
const ANIMATION_KEYWORD_OWNER_SIGNATURES: [&str; 6] =
    ["WEAP", "ARMO", "FURN", "TERM", "ALCH", "NPC_"];

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct GenerateAdditiveRacesFixup;

impl Fixup for GenerateAdditiveRacesFixup {
    fn name(&self) -> &'static str {
        "generate_additive_races"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::WholePluginSafe
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        applies_for_config(ctx.config)
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        applies_for_config(config) && source_game == Some("fo76") && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let race_sig =
            SigCode::from_str("RACE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        // Synthesize from the SOURCE plugin for both paths: the base races a
        // weapon rides on (HumanRace / HumanRaceSubGraphData) are vanilla-remapped
        // to Fallout4.esm and never written as local target records, so the target
        // RACE list is empty on a bounded graph run. Only the synthesized additive
        // EditorID differs per path.
        let naming = if config.is_whole_plugin && config.root_sig.is_none() {
            AdditiveNaming::PluginPort
        } else {
            let root_eid = find_root_eid(session, target_schema.as_ref(), mapper, config)?;
            AdditiveNaming::PerWeapon(derive_weapon_name(&root_eid))
        };
        run_additive_races_from_source(
            session,
            mapper,
            config,
            target_schema.as_ref(),
            race_sig,
            naming,
        )
    }
}

// ---------------------------------------------------------------------------
// Top-level plumbing
// ---------------------------------------------------------------------------

fn applies_for_config(config: &FixupConfig) -> bool {
    if config.is_whole_plugin {
        return config
            .root_sig
            .map(|sig| !is_creature_root_sig(sig))
            .unwrap_or(true);
    }
    config
        .root_sig
        .map(|sig| !is_creature_root_sig(sig))
        .unwrap_or(false)
}

/// Naming strategy for synthesized additive RACE EditorIDs. Whole-plugin runs
/// emit one shared `<Race>AdditivePluginPort` per race; bounded graph runs emit
/// a per-weapon `<Race>Additive<WeaponName>` keyed off the conversion root's
/// EditorID.
enum AdditiveNaming {
    PluginPort,
    PerWeapon(String),
}

impl AdditiveNaming {
    fn additive_eid(&self, eid_prefix: &str) -> String {
        match self {
            AdditiveNaming::PluginPort => whole_plugin_additive_eid(eid_prefix),
            AdditiveNaming::PerWeapon(weapon_name) => format!("{eid_prefix}Additive{weapon_name}"),
        }
    }
}

#[derive(Clone)]
struct AnimationKeywordUse {
    fk: FormKey,
    weapon_name: String,
}

fn run_additive_races_from_source(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    target_schema: &crate::schema::AuthoringSchema,
    race_sig: SigCode,
    naming: AdditiveNaming,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let source_schema = session
        .source_schema()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;

    let keyword_uses =
        collect_source_animation_keyword_uses(session, source_schema.as_ref(), mapper)?;
    if keyword_uses.is_empty() {
        return Ok(report);
    }
    let keyword_fks: FxHashSet<FormKey> = keyword_uses
        .iter()
        .map(|keyword_use| keyword_use.fk)
        .collect();
    let mut existing_race_eids =
        collect_target_race_eids(session, target_schema, mapper, race_sig)?;
    let race_fks = session
        .source_form_keys_of_sig(race_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut source_plugins = collect_source_plugins(mapper);
    let output_plugin_sym = mapper.output_plugin_sym();
    let mut additive_count: u32 = 0;
    let mut human_additives_to_derive: Vec<(String, FormKey)> = Vec::new();
    // Target-space animation keywords that end up gating a third-person weapon block
    // we actually emit. `strip_generic_grips_from_owned_weapons` needs the emitted set,
    // not the candidate set: a keyword whose block is dropped by the dedupe below still
    // needs its weapon's grip, because FO4's generic block is all that weapon has left.
    let mut third_person_block_keywords: FxHashSet<FormKey> = FxHashSet::default();

    for race_fk in &race_fks {
        source_plugins.insert(race_fk.plugin);
        let race_record =
            match session.source_record_decoded(race_fk, source_schema.as_ref(), mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("generate_additive_races:source_race_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };
        let Some(eid_str) = record_eid_string(&race_record, mapper.interner) else {
            continue;
        };
        let vanilla_fk = match mapper.find_vanilla_fk(&eid_str, race_sig) {
            Some(v) => v,
            None => continue,
        };

        let source_blocks = parse_canonical_subgraphs(&race_record);
        if source_blocks.is_empty() {
            continue;
        }

        let owned_matching: Vec<SubgraphBlock> = source_blocks
            .iter()
            .filter(|b| {
                b.target_keywords
                    .iter()
                    .any(|kfk| keyword_fks.contains(kfk))
            })
            .cloned()
            .collect();
        if owned_matching.is_empty() {
            continue;
        }

        seed_vanilla_keyword_mappings_for_blocks(
            session,
            source_schema.as_ref(),
            mapper,
            &owned_matching,
        )?;

        let target_base_fk =
            resolve_additive_parent(&eid_str, vanilla_fk, mapper).unwrap_or(vanilla_fk);
        let eid_prefix = race_eid_normalize(&eid_str);
        let additive_eid = naming.additive_eid(eid_prefix);
        if existing_race_eids.contains(&additive_eid) {
            continue;
        }
        let additive_eid_sym = mapper.interner.intern(&additive_eid);

        let mut rewritten =
            rewrite_subgraph_block_formkeys(owned_matching, mapper, &source_plugins);
        normalize_fo76_fo4_subgraph_blocks(&mut rewritten, mapper.interner);
        rewritten = retain_fo76_unique_subgraph_blocks(rewritten, output_plugin_sym);
        if rewritten.is_empty() {
            continue;
        }
        for block in rewritten.iter().filter(|b| is_third_person_weapon_role(b)) {
            // Grips are excluded: a block that named one would otherwise mark every
            // weapon carrying that grip as ours, and suppress the fan restore for the
            // many that have no block of their own.
            third_person_block_keywords.extend(
                block
                    .target_keywords
                    .iter()
                    .filter(|kw| !is_fo4_grip_keyword(kw, mapper.interner))
                    .copied(),
            );
        }

        let stripped_template = load_target_race_template_from_masters(
            session,
            target_schema,
            mapper,
            config,
            target_base_fk,
        )
        .map(|record| strip_template_subgraph_fields(&record))
        .unwrap_or_else(|| Record::new(race_sig, target_base_fk));
        let mut additive =
            build_additive_race_record(stripped_template, target_base_fk, &rewritten);
        set_record_editor_id(&mut additive, additive_eid_sym);

        let synth_source_fk = synthetic_source_form_key(additive_count, mapper.interner);
        let new_fk = mapper.allocate_or_resolve(synth_source_fk, Some(additive_eid_sym), race_sig);
        additive.form_key = new_fk;

        match session.add_record(additive, target_schema, mapper.interner) {
            Ok(()) => {
                report.records_added += 1;
                existing_race_eids.insert(additive_eid.clone());
                if eid_prefix == "HumanRace" {
                    human_additives_to_derive.push((additive_eid, new_fk));
                }
                additive_count += 1;
            }
            Err(e) => {
                let w = mapper
                    .interner
                    .intern(&format!("generate_additive_races:add_err:{e}"));
                report.warnings.push(w);
            }
        }
    }

    restore_generic_grips_on_unowned_weapons(
        session,
        target_schema,
        mapper,
        &third_person_block_keywords,
        &mut report,
    )?;

    let Some(pa_base_fk) = lookup_additive_parent_str("PowerArmorRace")
        .and_then(|s| parse_render_fk(s, mapper.interner))
    else {
        return Ok(report);
    };
    let Some(pa_template) =
        load_target_race_template_from_masters(session, target_schema, mapper, config, pa_base_fk)
    else {
        let warning = mapper
            .interner
            .intern("generate_additive_races:missing_target_power_armor_race_template");
        report.warnings.push(warning);
        return Ok(report);
    };

    for (human_eid, human_fk) in &human_additives_to_derive {
        let human_rec = match session.record_decoded(human_fk, target_schema, mapper.interner) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let mut human_blocks = parse_canonical_subgraphs(&human_rec);
        strip_namespaced_subgraph_paths(&mut human_blocks, mapper.interner);
        let mut pa_blocks = derive_pa_subgraph_blocks(&human_blocks, mapper.interner);
        if pa_blocks.is_empty() {
            continue;
        }
        insert_namespaced_subgraph_paths(&mut pa_blocks, mapper.interner);

        let pa_eid = human_eid.replacen("HumanRaceAdditive", "PowerArmorRaceAdditive", 1);
        if existing_race_eids.contains(&pa_eid) {
            continue;
        }
        let pa_eid_sym = mapper.interner.intern(&pa_eid);

        let mut pa_record =
            build_power_armor_additive_race_record(&pa_template, pa_base_fk, &pa_blocks);
        set_record_editor_id(&mut pa_record, pa_eid_sym);

        let synth_source_fk = synthetic_source_form_key(additive_count, mapper.interner);
        let new_fk = mapper.allocate_or_resolve(synth_source_fk, Some(pa_eid_sym), race_sig);
        pa_record.form_key = new_fk;

        match session.add_record(pa_record, target_schema, mapper.interner) {
            Ok(()) => {
                report.records_added += 1;
                existing_race_eids.insert(pa_eid);
                additive_count += 1;
            }
            Err(e) => {
                let w = mapper
                    .interner
                    .intern(&format!("generate_additive_races:pa_add_err:{e}"));
                report.warnings.push(w);
            }
        }
    }

    Ok(report)
}

fn collect_source_animation_keyword_uses(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
) -> Result<Vec<AnimationKeywordUse>, FixupError> {
    let mut by_keyword: FxHashMap<FormKey, String> = FxHashMap::default();
    // FURN/TERM/ALCH/NPC_ carry `AnimFurn*` keywords (furniture/terminal
    // interaction subgraphs, inhaler consumables, ghoul-ambush poses); the
    // Anims/Anim EID-prefix filter below keeps every other keyword out.
    for sig_name in ANIMATION_KEYWORD_OWNER_SIGNATURES {
        let sig =
            SigCode::from_str(sig_name).map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let fks = match session.source_form_keys_of_sig(sig, mapper.interner) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for fk in fks {
            if mapper.lookup(fk).is_none() {
                continue;
            }
            let record = match session.source_record_decoded(&fk, source_schema, mapper.interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
            let owner_eid = record_eid_string(&record, mapper.interner).unwrap_or_default();
            for keyword_fk in collect_form_keys_from_subrecord(&record, "KWDA")? {
                if by_keyword.contains_key(&keyword_fk) {
                    continue;
                }
                let Some(keyword_eid) =
                    source_keyword_eid(session, source_schema, mapper, keyword_fk)
                else {
                    continue;
                };
                let Some(weapon_name) =
                    derive_weapon_name_from_animation_keyword(&keyword_eid, &owner_eid)
                else {
                    continue;
                };
                by_keyword.insert(keyword_fk, weapon_name);
            }
        }
    }

    let omod_sig = SigCode::from_str("OMOD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let omod_fks = session
        .source_form_keys_of_sig(omod_sig, mapper.interner)
        .unwrap_or_default();
    for fk in omod_fks {
        if mapper.lookup(fk).is_none() {
            continue;
        }
        let record = match session.source_record_decoded(&fk, source_schema, mapper.interner) {
            Ok(record) => record,
            Err(_) => continue,
        };
        let owner_eid = record_eid_string(&record, mapper.interner).unwrap_or_default();
        for keyword_fk in collect_omod_formid_property_keywords(&record, mapper.interner) {
            if by_keyword.contains_key(&keyword_fk) {
                continue;
            }
            let Some(keyword_eid) = source_keyword_eid(session, source_schema, mapper, keyword_fk)
            else {
                continue;
            };
            let Some(weapon_name) =
                derive_weapon_name_from_animation_keyword(&keyword_eid, &owner_eid)
            else {
                continue;
            };
            by_keyword.insert(keyword_fk, weapon_name);
        }
    }

    let mut out: Vec<AnimationKeywordUse> = by_keyword
        .into_iter()
        .map(|(fk, weapon_name)| AnimationKeywordUse { fk, weapon_name })
        .collect();
    out.sort_by(|a, b| {
        a.weapon_name
            .cmp(&b.weapon_name)
            .then(a.fk.local.cmp(&b.fk.local))
    });
    Ok(out)
}

/// The four `AnimsGrip*` keywords FO4 defines, by `Fallout4.esm` object id and EditorID.
///
/// FO76 inherited all four at the SAME object ids, which is why they are held out of the
/// mapper's vanilla EditorID remap — see `FO76_FO4_VANILLA_REMAP_BLOCKED_FORM_IDS`.
const FO4_GRIP_KEYWORDS: [(u32, &str); 4] = [
    (0x01_F948, "AnimsGripPistol"),
    (0x01_F947, "AnimsGripRifleAssault"),
    (0x04_64EF, "AnimsGripRifleStraight"),
    (0x0A_A937, "AnimsGripShoulderFired"),
];

fn is_fo4_grip_object_id(object_id: u32) -> bool {
    FO4_GRIP_KEYWORDS.iter().any(|(grip, _)| *grip == object_id)
}

const FALLOUT4_ESM: &str = "Fallout4.esm";

const SRAF_ROLE_WEAPON: u16 = 1;

fn is_fo4_grip_keyword(fk: &FormKey, interner: &StringInterner) -> bool {
    is_fo4_grip_object_id(fk.local & 0x00FF_FFFF)
        && interner
            .resolve(fk.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FALLOUT4_ESM))
}

/// `SRAF` is two u16s: role in the low half, perspective flag in the high half.
/// Role 1 == Weapon; a zero high half == third person.
fn is_third_person_weapon_role(block: &SubgraphBlock) -> bool {
    block.flags_bytes.as_deref().is_some_and(|flags| {
        flags.len() >= 4
            && u16::from_le_bytes([flags[0], flags[1]]) == SRAF_ROLE_WEAPON
            && u16::from_le_bytes([flags[2], flags[3]]) == 0
    })
}

/// Give back FO4's generic grip keyword to converted weapons that have no third-person
/// block of their own.
///
/// FO76 inherits FO4's four `AnimsGrip*` keywords at the same object ids.
/// `FO76_FO4_VANILLA_REMAP_BLOCKED_FORM_IDS` keeps them source-owned under a
/// `fo76`-suffixed EditorID, so FO4's generic fan (48 blocks answer
/// `AnimsGripRifleStraight` alone) cannot outrank the block emitted for a converted
/// weapon's own animation keyword. Renaming at the mapper also covers grips added by
/// OMOD keyword properties, which a `KWDA`-only strip missed.
///
/// FO76 reskins of FO4 weapon types (.44, assault rifle, 10mm) have no animation
/// keyword; the generic fan is their third-person animation, so they get the FO4 grip
/// back. On the 2026-09-03 build, 203 of 1122 weapons carry a grip, 47 have a block of
/// their own, 156 do not.
fn restore_generic_grips_on_unowned_weapons(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
    third_person_block_keywords: &FxHashSet<FormKey>,
    report: &mut FixupReport,
) -> Result<(), FixupError> {
    let twins = source_owned_grip_twins(session, target_schema, mapper)?;
    if twins.is_empty() {
        return Ok(());
    }
    let weap_sig = SigCode::from_str("WEAP").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let weapon_fks = session
        .form_keys_of_sig(weap_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;

    for fk in weapon_fks {
        let Ok(mut record) = session.record_decoded(&fk, target_schema, mapper.interner) else {
            continue;
        };
        let keywords = collect_form_keys_from_subrecord(&record, "KWDA")?;
        // A weapon we emitted a block for must NOT reach the fan — that is the whole point.
        if keywords
            .iter()
            .any(|kw| third_person_block_keywords.contains(kw))
        {
            continue;
        }
        let mut restore: Vec<u32> = keywords
            .iter()
            .filter_map(|kw| twins.get(kw).copied())
            .collect();
        restore.sort_unstable();
        restore.dedup();
        if restore.is_empty() {
            continue;
        }
        if !add_fo4_grip_keywords(&mut record, &restore, mapper.interner) {
            continue;
        }
        match session.replace_record_contents(record, target_schema, mapper.interner) {
            Ok(true) => report.records_changed += 1,
            Ok(false) => {}
            Err(e) => {
                let w = mapper
                    .interner
                    .intern(&format!("generate_additive_races:grip_restore_err:{e}"));
                report.warnings.push(w);
            }
        }
    }
    Ok(())
}

/// Map each source-owned grip keyword in the target plugin back to the FO4 object id it
/// was inherited from.
///
/// Keyed on the EditorID rather than the object id: the collision rename appends `fo76`
/// (plus a digit when that collides too) and the mapper is free to reallocate the local
/// id, so the name is the only stable link back to the FO4 original.
fn source_owned_grip_twins(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
) -> Result<FxHashMap<FormKey, u32>, FixupError> {
    let kywd_sig = SigCode::from_str("KYWD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut out = FxHashMap::default();
    let Ok(keyword_fks) = session.form_keys_of_sig(kywd_sig, mapper.interner) else {
        return Ok(out);
    };
    for fk in keyword_fks {
        let Ok(record) = session.record_decoded(&fk, target_schema, mapper.interner) else {
            continue;
        };
        let Some(editor_id) = record.eid.and_then(|sym| mapper.interner.resolve(sym)) else {
            continue;
        };
        for (object_id, name) in FO4_GRIP_KEYWORDS {
            if is_collision_renamed_twin(editor_id, name) {
                out.insert(fk, object_id);
                break;
            }
        }
    }
    Ok(out)
}

/// Whether `editor_id` is `base` carrying the collision rename's `fo76` suffix, with the
/// disambiguating digits a second collision would append.
fn is_collision_renamed_twin(editor_id: &str, base: &str) -> bool {
    let Some(rest) = editor_id
        .get(..base.len())
        .filter(|prefix| prefix.eq_ignore_ascii_case(base))
        .and_then(|_| editor_id.get(base.len()..))
    else {
        return false;
    };
    rest.get(..4)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case("fo76"))
        && rest[4..].chars().all(|c| c.is_ascii_digit())
}

/// Append FO4 grip keywords to a weapon's `KWDA` and resync `KSIZ`.
/// Returns whether anything was added.
fn add_fo4_grip_keywords(
    record: &mut Record,
    object_ids: &[u32],
    interner: &StringInterner,
) -> bool {
    let Ok(kwda_sig) = SubrecordSig::from_str("KWDA") else {
        return false;
    };
    let fo4 = interner.intern(FALLOUT4_ESM);
    let mut added = false;
    let mut kept: u32 = 0;
    for entry in record
        .fields
        .iter_mut()
        .filter(|entry| entry.sig == kwda_sig)
    {
        match &mut entry.value {
            FieldValue::List(items) => {
                if !added {
                    for &object_id in object_ids {
                        items.push(FieldValue::FormKey(FormKey {
                            local: object_id,
                            plugin: fo4,
                        }));
                    }
                    added = true;
                }
                kept += items.len() as u32;
            }
            FieldValue::Bytes(bytes) => {
                if !added {
                    for &object_id in object_ids {
                        // Fallout4.esm is master index 0 in every plugin this converter
                        // emits, so the raw form is the bare object id.
                        bytes.extend_from_slice(&object_id.to_le_bytes());
                    }
                    added = true;
                }
                kept += (bytes.len() / 4) as u32;
            }
            _ => {}
        }
    }
    if !added {
        return false;
    }
    if let Ok(ksiz_sig) = SubrecordSig::from_str("KSIZ")
        && let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == ksiz_sig)
    {
        entry.value = FieldValue::Bytes(smallvec::SmallVec::from_slice(&kept.to_le_bytes()));
    }
    true
}

fn collect_omod_formid_property_keywords(
    record: &Record,
    interner: &StringInterner,
) -> Vec<FormKey> {
    let mut out = Vec::new();
    let data_sig = SubrecordSig(*b"DATA");
    for entry in &record.fields {
        if entry.sig == data_sig
            && let FieldValue::Bytes(bytes) = &entry.value
        {
            collect_omod_raw_data_property_keywords(bytes, record.form_key.plugin, &mut out);
            continue;
        }
        collect_omod_formid_property_keywords_from_value(
            &entry.value,
            record.form_key.plugin,
            interner,
            &mut out,
        );
    }
    out
}

/// Extract `Keywords` (property 31) FormID values from a raw OMOD `DATA`
/// payload. The production decode carries OMOD DATA as opaque bytes, so the
/// struct walker below never sees property rows for real records. Layout
/// mirrors `rewrite_omod_data_bytes` (rewrite_raw_object_template_formids.rs):
/// 20-byte header (include_count u32 @0, property_count u32 @4), attach-parent
/// slot count u32 @20 + 4-byte slots, one 4-byte item row, 7-byte include
/// rows, then 24-byte property rows (value_type u8 @0, property u16 @8,
/// Value1 u32 @12). Value1 is a FormID only for value_type 4/6; only
/// source-local ids (master byte 0) are keyword candidates.
fn collect_omod_raw_data_property_keywords(
    bytes: &[u8],
    owner_plugin: Sym,
    out: &mut Vec<FormKey>,
) {
    const HEADER_LEN: usize = 20;
    const ATTACH_PARENT_SLOT_COUNT_LEN: usize = 4;
    const ITEM_ROW_LEN: usize = 4;
    const INCLUDE_ROW_LEN: usize = 7;
    const PROPERTY_ROW_LEN: usize = 24;
    const KEYWORDS_PROPERTY: u16 = 31;

    if bytes.len() < HEADER_LEN + ATTACH_PARENT_SLOT_COUNT_LEN {
        return;
    }
    let include_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let attach_parent_slot_count =
        u32::from_le_bytes(bytes[HEADER_LEN..HEADER_LEN + 4].try_into().unwrap()) as usize;
    let attach_parent_slots_start = HEADER_LEN + ATTACH_PARENT_SLOT_COUNT_LEN;
    let Some(properties_start) = attach_parent_slot_count
        .checked_mul(4)
        .and_then(|len| attach_parent_slots_start.checked_add(len))
        .and_then(|pos| pos.checked_add(ITEM_ROW_LEN))
        .and_then(|pos| {
            include_count
                .checked_mul(INCLUDE_ROW_LEN)
                .and_then(|len| pos.checked_add(len))
        })
    else {
        return;
    };
    let Some(properties_end) = property_count
        .checked_mul(PROPERTY_ROW_LEN)
        .and_then(|len| properties_start.checked_add(len))
    else {
        return;
    };
    if properties_end > bytes.len() {
        return;
    }
    for index in 0..property_count {
        let row = &bytes[properties_start + index * PROPERTY_ROW_LEN..][..PROPERTY_ROW_LEN];
        let value_type = row[0];
        let property = u16::from_le_bytes([row[8], row[9]]);
        if property != KEYWORDS_PROPERTY || !matches!(value_type, 4 | 6) {
            continue;
        }
        let raw = u32::from_le_bytes([row[12], row[13], row[14], row[15]]);
        if raw != 0 && raw >> 24 == 0 {
            out.push(FormKey {
                local: raw,
                plugin: owner_plugin,
            });
        }
    }
}

fn collect_omod_formid_property_keywords_from_value(
    value: &FieldValue,
    owner_plugin: Sym,
    interner: &StringInterner,
    out: &mut Vec<FormKey>,
) {
    match value {
        FieldValue::List(values) => {
            for value in values {
                collect_omod_formid_property_keywords_from_value(
                    value,
                    owner_plugin,
                    interner,
                    out,
                );
            }
        }
        FieldValue::Struct(fields) => {
            let field = |name: &str| {
                fields.iter().find_map(|(field_name, value)| {
                    interner
                        .resolve(*field_name)
                        .is_some_and(|field_name| field_name.eq_ignore_ascii_case(name))
                        .then_some(value)
                })
            };
            let is_keyword_property = field("Property").and_then(field_value_u32) == Some(31);
            let is_formid = field("ValueType").is_some_and(|value| {
                matches!(field_value_u32(value), Some(4 | 6))
                    || matches!(value, FieldValue::String(sym) if interner.resolve(*sym).is_some_and(|name| matches!(name, "FormIDInt" | "FormIDFloat")))
            });
            if is_keyword_property && is_formid {
                if let Some(local) = field("Value1").and_then(field_value_u32)
                    && local <= 0x00ff_ffff
                {
                    out.push(FormKey {
                        local,
                        plugin: owner_plugin,
                    });
                }
            }
            for (_, value) in fields {
                collect_omod_formid_property_keywords_from_value(
                    value,
                    owner_plugin,
                    interner,
                    out,
                );
            }
        }
        _ => {}
    }
}

fn field_value_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn whole_plugin_additive_eid(eid_prefix: &str) -> String {
    format!("{eid_prefix}AdditivePluginPort")
}

fn source_keyword_eid(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
    keyword_fk: FormKey,
) -> Option<String> {
    let record = session
        .source_record_decoded(&keyword_fk, source_schema, mapper.interner)
        .ok()?;
    record_eid_string(&record, mapper.interner)
}

fn derive_weapon_name_from_animation_keyword(
    keyword_eid: &str,
    _owner_eid: &str,
) -> Option<String> {
    let rest = keyword_eid
        .strip_prefix("Anims")
        .or_else(|| keyword_eid.strip_prefix("Anim"))?;
    if !rest.is_empty() {
        return Some(derive_weapon_name(rest));
    }
    None
}

fn normalize_fo76_fo4_subgraph_blocks(blocks: &mut Vec<SubgraphBlock>, interner: &StringInterner) {
    blocks.retain(|block| {
        interner
            .resolve(block.behaviour_graph)
            .is_none_or(|path| !fo76_graph_without_fo4_equivalent(path))
    });
    for block in blocks.iter_mut() {
        let Some(path) = interner.resolve(block.behaviour_graph) else {
            continue;
        };
        if let Some(replacement) = fo4_deferred_behaviour_graph(path) {
            block.behaviour_graph = interner.intern(replacement);
        }
    }
    insert_namespaced_subgraph_paths(blocks, interner);
}

/// FO76 graphs with no FO4 analog at all — a retained block would bind a
/// behavior FO4 cannot load, so the block is dropped instead of remapped.
fn fo76_graph_without_fo4_equivalent(path: &str) -> bool {
    path.eq_ignore_ascii_case("Actors\\Character\\Behaviors\\FaceGen.hkx")
}

fn fo4_deferred_behaviour_graph(path: &str) -> Option<&'static str> {
    if path.eq_ignore_ascii_case("Actors\\Character\\_1stPerson\\Behaviors\\Pipboy2000.hkx") {
        return Some("Actors\\Character\\_1stPerson\\Behaviors\\Pipboy.hkx");
    }
    None
}

/// Insert the owning weapon set's namespaced twins directly ahead of its
/// original entries in the `SAPT` chain.
///
/// `SAPT` resolves in order, so converted clips under the namespace win and the
/// original entry stays the fallback for everything not re-authored. Later
/// animation sets stay unnamespaced so FO4's compatible fallback beats another
/// FO76 weapon class's colliding clips.
///
/// Only the owning set is namespaced. A shared grip set is inherited by every
/// weapon in the grip, so namespacing every eligible entry (tried in game) gave
/// all of them the 186 clips under `FO76\Weapon\GripHeavy`, authored for other
/// FO76 weapons: the .50 Cal's standing fire distorted and the Alien Rifle lost
/// its jump and walk.
fn insert_namespaced_subgraph_paths(blocks: &mut [SubgraphBlock], interner: &StringInterner) {
    for block in blocks.iter_mut() {
        let Some(owning_set) = block
            .paths
            .iter()
            .find_map(|sym| interner.resolve(*sym).and_then(animation_set_path_key))
        else {
            continue;
        };
        let mut paths: Vec<Sym> = Vec::with_capacity(block.paths.len());
        for sym in &block.paths {
            if let Some(namespaced) = interner
                .resolve(*sym)
                .filter(|path| animation_set_path_key(path).as_deref() == Some(&owning_set))
                .and_then(namespaced_anim_path)
                .map(|namespaced| interner.intern(&namespaced))
                && !paths.contains(&namespaced)
                && !block.paths.contains(&namespaced)
            {
                paths.push(namespaced);
            }
            paths.push(*sym);
        }
        block.paths = paths;
    }
}

/// Drop namespaced `SAPT` entries from blocks read back off a built record.
///
/// The PowerArmor derivation matches on `Animations\Weapon\` adjacency, which
/// the namespace segment breaks; it re-derives from the original entries and
/// its own namespaced twins are inserted afterwards.
fn strip_namespaced_subgraph_paths(blocks: &mut [SubgraphBlock], interner: &StringInterner) {
    for block in blocks.iter_mut() {
        block.paths.retain(|sym| {
            interner
                .resolve(*sym)
                .is_none_or(|path| !is_namespaced_anim_path(path))
        });
    }
}

fn retain_fo76_unique_subgraph_blocks(
    blocks: Vec<SubgraphBlock>,
    output_plugin: Sym,
) -> Vec<SubgraphBlock> {
    blocks
        .into_iter()
        .filter(|block| {
            block
                .target_keywords
                .iter()
                .any(|fk| fk.plugin == output_plugin)
        })
        .collect()
}

fn seed_vanilla_keyword_mappings_for_blocks(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
    blocks: &[SubgraphBlock],
) -> Result<(), FixupError> {
    let keyword_sig =
        SigCode::from_str("KYWD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut seen: FxHashSet<FormKey> = FxHashSet::default();
    for fk in blocks
        .iter()
        .flat_map(|block| block.subgraph_keywords.iter().chain(&block.target_keywords))
    {
        if !seen.insert(*fk) || mapper.lookup(*fk).is_some() {
            continue;
        }
        let Ok(record) = session.source_record_decoded(fk, source_schema, mapper.interner) else {
            continue;
        };
        let Some(eid) = record_eid_string(&record, mapper.interner) else {
            continue;
        };
        seed_vanilla_keyword_mapping(mapper, *fk, &eid, keyword_sig);
    }
    Ok(())
}

fn seed_vanilla_keyword_mapping(
    mapper: &mut FormKeyMapper,
    source_fk: FormKey,
    keyword_eid: &str,
    keyword_sig: SigCode,
) -> Option<FormKey> {
    if let Some(mapped) = mapper.lookup(source_fk) {
        return Some(mapped);
    }
    let vanilla_fk = mapper.find_vanilla_fk(keyword_eid, keyword_sig)?;
    mapper.add_mapping(source_fk, vanilla_fk);
    Some(vanilla_fk)
}

fn collect_form_keys_from_subrecord(
    record: &Record,
    sig: &str,
) -> Result<Vec<FormKey>, FixupError> {
    let wanted = SubrecordSig::from_str(sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut out = Vec::new();
    for entry in &record.fields {
        if entry.sig == wanted {
            accumulate_form_keys(&entry.value, &mut out);
        }
    }
    Ok(out)
}

fn accumulate_form_keys(value: &FieldValue, out: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(fk) => out.push(*fk),
        FieldValue::List(values) => {
            for value in values {
                accumulate_form_keys(value, out);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                accumulate_form_keys(value, out);
            }
        }
        _ => {}
    }
}

fn collect_target_race_eids(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
    race_sig: SigCode,
) -> Result<FxHashSet<String>, FixupError> {
    let mut out = FxHashSet::default();
    let fks = session
        .form_keys_of_sig(race_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    for fk in fks {
        let Ok(record) = session.record_decoded(&fk, target_schema, mapper.interner) else {
            continue;
        };
        if let Some(eid) = record_eid_string(&record, mapper.interner) {
            out.insert(eid);
        }
    }
    Ok(out)
}

fn record_eid_string(record: &Record, interner: &StringInterner) -> Option<String> {
    record
        .eid
        .and_then(|sym| interner.resolve(sym).map(|s| s.to_string()))
}

fn set_record_editor_id(record: &mut Record, eid: Sym) {
    record.eid = Some(eid);
    let edid = SubrecordSig::from_str("EDID").expect("EDID sig");
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == edid) {
        entry.value = FieldValue::String(eid);
        return;
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: edid,
            value: FieldValue::String(eid),
        },
    );
}

/// Read a RACE straight out of the target game's masters. Also used by
/// `fix_creature_race_records` to borrow FO4's own power-armor subgraph blocks.
pub(crate) fn load_target_race_template_from_masters(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    target_base_fk: FormKey,
) -> Option<Record> {
    for &handle_id in &config.target_master_handle_ids {
        if let Ok(record) = session.record_decoded_in_handle(
            handle_id,
            &target_base_fk,
            target_schema,
            mapper.interner,
        ) {
            return Some(record);
        }
    }
    None
}

fn build_power_armor_additive_race_record(
    target_power_armor_template: &Record,
    power_armor_race_fk: FormKey,
    blocks: &[SubgraphBlock],
) -> Record {
    build_additive_race_record(
        strip_template_subgraph_fields(target_power_armor_template),
        power_armor_race_fk,
        blocks,
    )
}

/// Find the EditorID of the conversion root record. The "root" is identified
/// positionally as the first record in the target plugin whose signature
/// matches `ctx.config.root_sig`.
fn find_root_eid(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<String, FixupError> {
    let root_sig = match config.root_sig {
        Some(sig) => sig,
        None => return Ok(String::new()),
    };
    let fks = match session.form_keys_of_sig(root_sig, mapper.interner) {
        Ok(v) => v,
        Err(_) => return Ok(String::new()),
    };
    for fk in &fks {
        if let Ok(rec) = session.record_decoded(fk, target_schema, mapper.interner) {
            if let Some(sym) = rec.eid {
                if let Some(s) = mapper.interner.resolve(sym) {
                    return Ok(s.to_string());
                }
            }
        }
    }
    Ok(String::new())
}

/// Derive a short weapon name from the root EditorID: strip `_NONPLAYABLE` /
/// `zzz_` markers, then take the last underscore-delimited segment when any
/// underscores remain.
pub fn derive_weapon_name(root_eid: &str) -> String {
    let mut s = root_eid.replace("_NONPLAYABLE", "").replace("zzz_", "");
    if s.contains('_') {
        if let Some(last) = s.rsplit('_').next() {
            s = last.to_string();
        }
    }
    if s.is_empty() {
        "Unknown".to_string()
    } else {
        s
    }
}

fn synthetic_source_form_key(n: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        local: n + 1,
        plugin: interner.intern(SYNTHETIC_ADDITIVE_RACE_PLUGIN),
    }
}

/// Collect every source-plugin Sym observed by the mapper.
fn collect_source_plugins(mapper: &mut FormKeyMapper) -> FxHashSet<Sym> {
    let mut out: FxHashSet<Sym> = FxHashSet::default();
    for (src, _) in mapper.source_to_target_iter() {
        out.insert(src.plugin);
    }
    out
}

/// Parse an "OBJID:Plugin.esm" rendering into a `FormKey`.
fn parse_render_fk(rendered: &str, interner: &StringInterner) -> Option<FormKey> {
    let (hex, plugin) = rendered.rsplit_once(':')?;
    let local = u32::from_str_radix(hex.trim(), 16).ok()?;
    let plugin_sym = interner.intern(plugin.trim());
    Some(FormKey {
        local,
        plugin: plugin_sym,
    })
}

/// Look up the rendered additive-parent FormKey string for an EID.
fn lookup_additive_parent_str(eid: &str) -> Option<&'static str> {
    FO4_ADDITIVE_PARENTS
        .iter()
        .find_map(|(k, v)| if *k == eid { Some(*v) } else { None })
}

/// Resolve the SADD target — `FO4_ADDITIVE_PARENTS` lookup first, else the
/// fallback vanilla FK.
fn resolve_additive_parent(
    eid: &str,
    fallback: FormKey,
    mapper: &mut FormKeyMapper,
) -> Option<FormKey> {
    match lookup_additive_parent_str(eid) {
        Some(rendered) => parse_render_fk(rendered, mapper.interner).or(Some(fallback)),
        None => Some(fallback),
    }
}

/// Normalize the source RACE EID for additive EditorID composition.
fn race_eid_normalize(eid: &str) -> &str {
    for (k, v) in RACE_EID_NORMALIZE {
        if *k == eid {
            return *v;
        }
    }
    eid
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::{FixupConfig, FixupContext};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
    use std::sync::Arc;

    fn make_test_ctx<'a>(
        schema: &'a Arc<AuthoringSchema>,
        config: &'a FixupConfig,
    ) -> FixupContext<'a> {
        FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: schema,
            schema_source: schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config,
        }
    }

    // -----------------------------------------------------------------------
    // Applies_to gate tests
    // -----------------------------------------------------------------------

    /// applies to WEAP root.
    #[test]
    fn applies_to_weap_root() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
        let ctx = make_test_ctx(&schema, &config);
        assert!(GenerateAdditiveRacesFixup.applies_to(&ctx));
    }

    /// does not apply to NPC_ root.
    #[test]
    fn does_not_apply_to_npc_root() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            ..Default::default()
        };
        let ctx = make_test_ctx(&schema, &config);
        assert!(!GenerateAdditiveRacesFixup.applies_to(&ctx));
    }

    /// does not apply to LVLN root.
    #[test]
    fn does_not_apply_to_lvln_root() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("LVLN").unwrap()),
            ..Default::default()
        };
        let ctx = make_test_ctx(&schema, &config);
        assert!(!GenerateAdditiveRacesFixup.applies_to(&ctx));
    }

    /// does not apply when root_sig is None.
    #[test]
    fn does_not_apply_when_no_root_sig() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig {
            root_sig: None,
            ..Default::default()
        };
        let ctx = make_test_ctx(&schema, &config);
        assert!(!GenerateAdditiveRacesFixup.applies_to(&ctx));
    }

    /// applies to whole-plugin runs with no single root.
    #[test]
    fn applies_to_whole_plugin_without_root_sig() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let config = FixupConfig {
            is_whole_plugin: true,
            root_sig: None,
            ..Default::default()
        };
        let ctx = make_test_ctx(&schema, &config);
        assert!(GenerateAdditiveRacesFixup.applies_to(&ctx));
    }

    #[test]
    fn applies_to_fo76_to_fo4_session() {
        let source = plugin_handle_new_native("SeventySixSource.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native("SeventySixOutput.esm", Some("fo4")).unwrap();
        let session = open_session(target, Some(source)).unwrap();
        let config = FixupConfig {
            is_whole_plugin: true,
            root_sig: None,
            ..Default::default()
        };

        assert!(GenerateAdditiveRacesFixup.applies_to_session(&session, &config));
    }

    #[test]
    fn does_not_apply_to_starfield_to_fo4_session() {
        let source = plugin_handle_new_native("StarfieldSource.esm", Some("starfield")).unwrap();
        let target = plugin_handle_new_native("StarfieldOutput.esm", Some("fo4")).unwrap();
        let session = open_session(target, Some(source)).unwrap();
        let config = FixupConfig {
            is_whole_plugin: true,
            root_sig: None,
            ..Default::default()
        };

        assert!(!GenerateAdditiveRacesFixup.applies_to_session(&session, &config));
    }

    // -----------------------------------------------------------------------
    // derive_weapon_name unit tests
    // -----------------------------------------------------------------------

    /// strips `_NONPLAYABLE`.
    #[test]
    fn derive_weapon_name_strips_nonplayable() {
        assert_eq!(derive_weapon_name("MyGun_NONPLAYABLE"), "MyGun");
    }

    /// strips `zzz_` prefix.
    #[test]
    fn derive_weapon_name_strips_zzz_prefix() {
        assert_eq!(derive_weapon_name("zzz_MyGun"), "MyGun");
    }

    /// takes last underscore segment.
    #[test]
    fn derive_weapon_name_takes_last_segment() {
        assert_eq!(derive_weapon_name("Faction_Class_MyGun"), "MyGun");
    }

    /// empty fallback to "Unknown".
    #[test]
    fn derive_weapon_name_empty_fallback() {
        assert_eq!(derive_weapon_name(""), "Unknown");
    }

    /// simple name passes through unchanged.
    #[test]
    fn derive_weapon_name_passes_through() {
        assert_eq!(derive_weapon_name("MyGun"), "MyGun");
    }

    #[test]
    fn animation_keyword_name_keeps_grips_for_data_dedupe() {
        assert_eq!(
            derive_weapon_name_from_animation_keyword("AnimsGaussPistol", "GaussPistol"),
            Some("GaussPistol".to_string())
        );
        assert_eq!(
            derive_weapon_name_from_animation_keyword("AnimsGripPistol", "GaussPistol"),
            Some("GripPistol".to_string())
        );
        assert_eq!(
            derive_weapon_name_from_animation_keyword("AnimPepperShaker", "PepperShaker"),
            Some("PepperShaker".to_string())
        );
    }

    #[test]
    fn furniture_workbench_animation_keywords_are_collected_for_additive_races() {
        assert!(ANIMATION_KEYWORD_OWNER_SIGNATURES.contains(&"FURN"));
        assert_eq!(
            derive_weapon_name_from_animation_keyword(
                "AnimFurnWorkbenchTinkersA",
                "WorkbenchTinkersA"
            ),
            Some("FurnWorkbenchTinkersA".to_string())
        );
        assert_eq!(
            derive_weapon_name_from_animation_keyword(
                "AnimFurnWorkbenchBrewing",
                "WorkbenchBrewing"
            ),
            Some("FurnWorkbenchBrewing".to_string())
        );
    }

    #[test]
    fn alien_rifle_subgraph_keeps_third_person_fallbacks_unnamespaced() {
        let interner = StringInterner::new();
        let mut blocks = vec![SubgraphBlock {
            behaviour_graph: interner.intern("Actors\\Character\\Behaviors\\WeaponBehavior.hkx"),
            paths: [
                "Actors\\Character\\Animations\\Weapon\\AlienRifle",
                "Actors\\Character\\Animations\\Weapon\\CombatShotgun\\Player",
                "Actors\\Character\\Animations\\Weapon\\CombatShotgun",
                "Actors\\Character\\Animations\\Weapon\\GripRifleStraight\\Player",
                "Actors\\Character\\Animations\\Weapon\\GripRifleStraight",
                "Actors\\Character\\Animations\\Paired",
            ]
            .iter()
            .map(|p| interner.intern(p))
            .collect(),
            subgraph_keywords: Vec::new(),
            target_keywords: Vec::new(),
            flags_bytes: None,
        }];

        insert_namespaced_subgraph_paths(&mut blocks, &interner);

        let resolved: Vec<String> = blocks[0]
            .paths
            .iter()
            .map(|s| interner.resolve(*s).unwrap().to_string())
            .collect();
        assert_eq!(
            resolved,
            vec![
                "Actors\\Character\\Animations\\FO76\\Weapon\\AlienRifle",
                "Actors\\Character\\Animations\\Weapon\\AlienRifle",
                // Fallback sets stay unnamespaced: they are shared by every weapon in
                // the grip, so a namespaced twin would hand all of them another FO76
                // weapon's clips ahead of FO4's compatible ones.
                "Actors\\Character\\Animations\\Weapon\\CombatShotgun\\Player",
                "Actors\\Character\\Animations\\Weapon\\CombatShotgun",
                "Actors\\Character\\Animations\\Weapon\\GripRifleStraight\\Player",
                "Actors\\Character\\Animations\\Weapon\\GripRifleStraight",
                "Actors\\Character\\Animations\\Paired",
            ]
        );

        // The PowerArmor derivation reads blocks back off the built record and
        // matches on Animations\Weapon adjacency, so the twins must strip out.
        strip_namespaced_subgraph_paths(&mut blocks, &interner);
        let stripped: Vec<String> = blocks[0]
            .paths
            .iter()
            .map(|s| interner.resolve(*s).unwrap().to_string())
            .collect();
        assert_eq!(
            stripped,
            vec![
                "Actors\\Character\\Animations\\Weapon\\AlienRifle",
                "Actors\\Character\\Animations\\Weapon\\CombatShotgun\\Player",
                "Actors\\Character\\Animations\\Weapon\\CombatShotgun",
                "Actors\\Character\\Animations\\Weapon\\GripRifleStraight\\Player",
                "Actors\\Character\\Animations\\Weapon\\GripRifleStraight",
                "Actors\\Character\\Animations\\Paired",
            ]
        );
    }

    #[test]
    fn omod_formid_keyword_properties_supply_animation_keywords() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let mut record = Record::new(
            SigCode::from_str("OMOD").unwrap(),
            FormKey {
                local: 0x8445e2,
                plugin,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("Properties"),
                FieldValue::List(vec![
                    FieldValue::Struct(vec![
                        (interner.intern("ValueType"), FieldValue::Uint(4)),
                        (interner.intern("Property"), FieldValue::Uint(31)),
                        (interner.intern("Value1"), FieldValue::Uint(0x85780d)),
                    ]),
                    FieldValue::Struct(vec![
                        (interner.intern("ValueType"), FieldValue::Uint(1)),
                        (interner.intern("Property"), FieldValue::Uint(31)),
                        (interner.intern("Value1"), FieldValue::Uint(0x123456)),
                    ]),
                ]),
            )]),
        });

        assert_eq!(
            collect_omod_formid_property_keywords(&record, &interner),
            vec![FormKey {
                local: 0x85780d,
                plugin,
            }]
        );
    }

    /// The production fo76 decode carries OMOD DATA as opaque bytes (the
    /// typed-Struct shape above never occurs on real records); keyword
    /// properties must be extracted from the raw 24-byte property rows.
    /// Mirrors mod_Nitro_Grip_Base (8445E2): a non-keyword property-73 row
    /// ahead of the AnimsGripNitroPistol keyword row.
    #[test]
    fn omod_raw_data_bytes_supply_animation_keywords() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");

        fn property_row(value_type: u8, property: u16, value1: u32) -> Vec<u8> {
            let mut row = vec![0u8; 24];
            row[0] = value_type;
            row[8..10].copy_from_slice(&property.to_le_bytes());
            row[12..16].copy_from_slice(&value1.to_le_bytes());
            row
        }

        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // include_count
        data.extend_from_slice(&4u32.to_le_bytes()); // property_count
        data.extend_from_slice(&[0u8; 8]); // format/flags
        data.extend_from_slice(&0x02249Fu32.to_le_bytes()); // attach point
        data.extend_from_slice(&1u32.to_le_bytes()); // attach parent slot count
        data.extend_from_slice(&0x03477Eu32.to_le_bytes()); // slot
        data.extend_from_slice(&[0u8; 4]); // item row
        data.extend(property_row(4, 73, 0x04334D)); // non-keyword FormID property
        data.extend(property_row(4, 31, 0x85780D)); // AnimsGripNitroPistol
        data.extend(property_row(1, 31, 0x123456)); // keyword property, Int value
        data.extend(property_row(4, 31, 0x0685780C)); // non-source master byte

        let mut record = Record::new(
            SigCode::from_str("OMOD").unwrap(),
            FormKey {
                local: 0x8445e2,
                plugin,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(data.into_iter().collect()),
        });

        assert_eq!(
            collect_omod_formid_property_keywords(&record, &interner),
            vec![FormKey {
                local: 0x85780D,
                plugin,
            }]
        );
    }

    /// Drop a block whose every target keyword resolves to an FO4 master
    /// (base game OR any DLC); keep a block with >=1 FO76-unique keyword
    /// (mapped into the output plugin); keep mixed blocks.
    #[test]
    fn retain_fo76_unique_drops_base_and_dlc_only_blocks() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let fo4 = interner.intern("Fallout4.esm");
        let dlc = interner.intern("DLCNukaWorld.esm");
        let graph =
            interner.intern("Actors\\Character\\Behaviors\\NoHandIKWeaponWrappingBehavior.hkx");

        let base_only = SubgraphBlock {
            behaviour_graph: graph,
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![FormKey {
                local: 0x111,
                plugin: fo4,
            }],
            flags_bytes: None,
        };
        let dlc_only = SubgraphBlock {
            behaviour_graph: graph,
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![FormKey {
                local: 0x009A77,
                plugin: dlc,
            }],
            flags_bytes: None,
        };
        let fo76_unique = SubgraphBlock {
            behaviour_graph: graph,
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![FormKey {
                local: 0x568776,
                plugin: output,
            }],
            flags_bytes: None,
        };
        let mixed = SubgraphBlock {
            behaviour_graph: graph,
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![
                FormKey {
                    local: 0x222,
                    plugin: fo4,
                },
                FormKey {
                    local: 0x575_19F,
                    plugin: output,
                },
            ],
            flags_bytes: None,
        };

        let retained = retain_fo76_unique_subgraph_blocks(
            vec![base_only, dlc_only, fo76_unique.clone(), mixed.clone()],
            output,
        );

        assert_eq!(retained.len(), 2);
        assert_eq!(retained[0].target_keywords, fo76_unique.target_keywords);
        assert_eq!(retained[1].target_keywords, mixed.target_keywords);
    }

    #[test]
    fn seed_vanilla_keyword_mapping_preserves_shared_source_keyword() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let target_plugin = interner.intern("Fallout4.esm");
        let target_eid = interner.intern("animsgrippistol");
        let keyword_sig = SigCode::from_str("KYWD").unwrap();
        let source_fk = FormKey {
            local: 0x01F948,
            plugin: source_plugin,
        };
        let target_fk = FormKey {
            local: 0x01F948,
            plugin: target_plugin,
        };
        let mut mapper = FormKeyMapper::new(
            [(target_eid, target_fk, keyword_sig)],
            MapperOptions::default(),
            &interner,
        );

        assert_eq!(
            seed_vanilla_keyword_mapping(&mut mapper, source_fk, "AnimsGripPistol", keyword_sig),
            Some(target_fk)
        );
        assert_eq!(mapper.lookup(source_fk), Some(target_fk));
    }

    #[test]
    fn preserves_fo76_combat_graphs_and_source_fallback_order() {
        let interner = StringInterner::new();
        for graph in [
            "Actors\\Character\\Behaviors\\GunBehavior.hkx",
            "Actors\\Character\\Behaviors\\MeleeBehavior.hkx",
            "Actors\\Character\\Behaviors\\BinocularBehavior.hkx",
            "Actors\\Character\\Behaviors\\BinocularInjuredWrappingBehavior.hkx",
            "Actors\\Character\\Behaviors\\MTInjuredWrappingBehavior.hkx",
            "Actors\\Character\\Behaviors\\BigGunWrappingBehavior.hkx",
            "Actors\\Character\\Behaviors\\NoHandIKRelaxedGunWrappingBehavior.hkx",
            "Actors\\Character\\Behaviors\\ChargeUpWrappingGunBehavior.hkx",
            "Actors\\Character\\_1stPerson\\Behaviors\\BigGunsSpinUpDown_GunWrappingBehavior.hkx",
            "Actors\\Character\\_1stPerson\\Behaviors\\InjuredBinoculars_GunWrappingBehavior.hkx",
        ] {
            let paths = vec![
                interner.intern("Actors\\Creature\\Animations\\Weapon"),
                interner.intern("Actors\\Creature\\Animations\\Shared"),
            ];
            let mut blocks = vec![SubgraphBlock {
                behaviour_graph: interner.intern(graph),
                paths: paths.clone(),
                subgraph_keywords: vec![],
                target_keywords: vec![],
                flags_bytes: None,
            }];
            normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);
            assert_eq!(interner.resolve(blocks[0].behaviour_graph), Some(graph));
            assert_eq!(blocks[0].paths, paths);
        }
    }

    #[test]
    fn furniture_keeps_its_source_core_and_pipboy_keeps_its_existing_route() {
        assert_eq!(
            fo4_deferred_behaviour_graph("Actors\\Character\\Behaviors\\FurnitureBed.hkx"),
            None
        );
        assert_eq!(
            fo4_deferred_behaviour_graph(
                "Actors\\Character\\_1stPerson\\Behaviors\\Pipboy2000.hkx"
            ),
            Some("Actors\\Character\\_1stPerson\\Behaviors\\Pipboy.hkx")
        );
    }

    #[test]
    fn drops_blocks_whose_graph_has_no_fo4_equivalent() {
        let interner = StringInterner::new();
        let graph = "Actors\\Character\\Behaviors\\FaceGen.hkx";
        let mut blocks = vec![SubgraphBlock {
            behaviour_graph: interner.intern(graph),
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        }];

        normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);

        assert!(blocks.is_empty(), "{graph} block must be dropped");
    }

    // -----------------------------------------------------------------------
    // race_eid_normalize unit tests
    // -----------------------------------------------------------------------

    /// `HumanRaceSubGraphData` normalizes to `HumanRace`.
    #[test]
    fn race_eid_normalize_subgraph_data() {
        assert_eq!(race_eid_normalize("HumanRaceSubGraphData"), "HumanRace");
    }

    /// unknown EID passes through unchanged.
    #[test]
    fn race_eid_normalize_passes_through() {
        assert_eq!(race_eid_normalize("PowerArmorRace"), "PowerArmorRace");
    }

    #[test]
    fn race_eid_normalize_passes_through_super_mutant() {
        assert_eq!(race_eid_normalize("SuperMutantRace"), "SuperMutantRace");
    }

    #[test]
    fn whole_plugin_additive_eid_is_one_per_race() {
        assert_eq!(
            whole_plugin_additive_eid("HumanRace"),
            "HumanRaceAdditivePluginPort"
        );
        assert_eq!(
            whole_plugin_additive_eid("SuperMutantRace"),
            "SuperMutantRaceAdditivePluginPort"
        );
    }

    // -----------------------------------------------------------------------
    // FO4_ADDITIVE_PARENTS constant table
    // -----------------------------------------------------------------------

    /// known EIDs resolve to expected FK strings.
    #[test]
    fn additive_parent_known_eids() {
        assert_eq!(
            lookup_additive_parent_str("HumanRace"),
            Some("166729:Fallout4.esm")
        );
        assert_eq!(
            lookup_additive_parent_str("PowerArmorRace"),
            Some("01D31E:Fallout4.esm")
        );
    }

    /// unknown EID has no additive-parent entry.
    #[test]
    fn additive_parent_unknown_eid_is_none() {
        assert!(lookup_additive_parent_str("GhoulRace").is_none());
    }

    #[test]
    fn additive_parent_super_mutant_falls_back_to_vanilla_race() {
        let mut interner = StringInterner::new();
        let plugin = interner.intern("Fallout4.esm");
        let fallback = FormKey {
            local: 0x0001A009,
            plugin,
        };
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut interner);

        assert_eq!(
            resolve_additive_parent("SuperMutantRace", fallback, &mut mapper),
            Some(fallback)
        );
    }

    #[test]
    fn power_armor_additive_uses_target_power_armor_template() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let pa_base_fk = FormKey {
            local: 0x01D31E,
            plugin: fallout4,
        };
        let mut pa_template = Record::new(SigCode::from_str("RACE").unwrap(), pa_base_fk);
        let project = interner.intern("Actors\\PowerArmor\\PowerArmorProject.hkx");
        pa_template.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(project),
        });
        let block = SubgraphBlock {
            behaviour_graph: interner.intern("Actors\\Character\\Behaviors\\WeaponBehavior.hkx"),
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        };

        let additive = build_power_armor_additive_race_record(&pa_template, pa_base_fk, &[block]);

        assert!(additive.fields.iter().any(|field| {
            field.sig.as_str() == "MODL" && field.value == FieldValue::String(project)
        }));
        assert!(additive.fields.iter().any(|field| {
            field.sig.as_str() == "SADD" && field.value == FieldValue::FormKey(pa_base_fk)
        }));
    }

    // -----------------------------------------------------------------------
    // Synthetic source allocation
    // -----------------------------------------------------------------------

    #[test]
    fn synthetic_source_form_key_uses_shared_fresh_allocator() {
        use crate::formkey_mapper::FIRST_ALLOCATION_ID;

        let mut interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let output = interner.intern("SeventySix.esm");
        let master_identity = FormKey {
            local: 1,
            plugin: fallout4,
        };
        let race_sig = SigCode::from_str("RACE").unwrap();
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                preserve_source_ids: true,
                generated_object_id_floor: 0x00A0_0000,
                ..MapperOptions::default()
            },
            &mut interner,
        );
        mapper.add_mapping(master_identity, master_identity);

        let source = synthetic_source_form_key(0, mapper.interner);
        assert!(source.local < FIRST_ALLOCATION_ID);
        assert_ne!(source, master_identity);

        let allocated = mapper.allocate_or_resolve(source, None, race_sig);
        assert_eq!(
            allocated,
            FormKey {
                local: 0x00A0_0000,
                plugin: output,
            }
        );
        assert_eq!(mapper.lookup(master_identity), Some(master_identity));
    }

    // -----------------------------------------------------------------------
    // AdditiveNaming — per-path additive EditorID composition.
    // -----------------------------------------------------------------------

    /// Asset-port / bounded runs name the additive per-weapon; whole-plugin
    /// runs share one `<Race>AdditivePluginPort` per race.
    #[test]
    fn additive_naming_per_path() {
        assert_eq!(
            AdditiveNaming::PerWeapon("GaussPistol".to_string()).additive_eid("HumanRace"),
            "HumanRaceAdditiveGaussPistol"
        );
        assert_eq!(
            AdditiveNaming::PluginPort.additive_eid("HumanRace"),
            "HumanRaceAdditivePluginPort"
        );
    }

    #[test]
    fn set_record_editor_id_rewrites_edid_field() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Output.esp");
        let old = interner.intern("HumanRaceSubGraphData");
        let new = interner.intern("HumanRaceAdditiveGaussPistol");
        let mut record = Record::new(
            SigCode::from_str("RACE").unwrap(),
            FormKey {
                local: 0x800,
                plugin,
            },
        );
        record.eid = Some(old);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(old),
        });

        set_record_editor_id(&mut record, new);

        assert_eq!(record.eid, Some(new));
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].value, FieldValue::String(new));
    }

    fn fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    fn fo4_fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("Fallout4.esm"),
        }
    }

    fn weapon_with_keywords(interner: &StringInterner, keywords: FieldValue, count: u32) -> Record {
        let mut record = Record::new(SigCode::from_str("WEAP").unwrap(), fk(interner, 0x01_0000));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KSIZ").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&count.to_le_bytes())),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KWDA").unwrap(),
            value: keywords,
        });
        record
    }

    fn keyword_count(record: &Record) -> u32 {
        let ksiz = SubrecordSig::from_str("KSIZ").unwrap();
        match &record.fields.iter().find(|e| e.sig == ksiz).unwrap().value {
            FieldValue::Bytes(b) => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            other => panic!("unexpected KSIZ shape: {other:?}"),
        }
    }

    /// Only the four real grips. A `SeventySix.esm` keyword that happens to share an
    /// object id with one of them is a different record entirely — which is exactly what
    /// the source-owned twin becomes once it is held out of the vanilla remap.
    #[test]
    fn only_fallout4_grip_keywords_are_recognised() {
        let interner = StringInterner::new();
        for local in [0x01_F948, 0x01_F947, 0x04_64EF, 0x0A_A937] {
            assert!(is_fo4_grip_keyword(&fo4_fk(&interner, local), &interner));
            assert!(!is_fo4_grip_keyword(&fk(&interner, local), &interner));
        }
        for local in [0x63_E33B, 0x01_866A, 0x0F_4AEA] {
            assert!(!is_fo4_grip_keyword(&fo4_fk(&interner, local), &interner));
        }
    }

    /// The twin is found by NAME, because the collision rename owns the suffix and the
    /// mapper is free to reallocate the local id.
    #[test]
    fn collision_renamed_twin_is_recognised_by_name() {
        for (editor_id, base, expected) in [
            ("AnimsGripRifleStraightfo76", "AnimsGripRifleStraight", true),
            ("animsgripriflestraightFO76", "AnimsGripRifleStraight", true),
            // A second collision appends a disambiguating digit.
            ("AnimsGripPistolfo761", "AnimsGripPistol", true),
            // The un-renamed vanilla record must never be mistaken for the twin.
            ("AnimsGripRifleStraight", "AnimsGripRifleStraight", false),
            // A namesake that merely starts the same is not the twin.
            ("AnimsGripRifleStraightAlt", "AnimsGripRifleStraight", false),
            ("AnimsGrip", "AnimsGripRifleStraight", false),
        ] {
            assert_eq!(
                is_collision_renamed_twin(editor_id, base),
                expected,
                "{editor_id} vs {base}"
            );
        }
    }

    /// A weapon with no block of its own keeps riding FO4's generic fan: the source-owned
    /// twin alone would leave it with no third-person animation at all.
    #[test]
    fn grip_is_restored_to_a_decoded_keyword_list_and_ksiz_resyncs() {
        let interner = StringInterner::new();
        let mut record = weapon_with_keywords(
            &interner,
            FieldValue::List(vec![
                FieldValue::FormKey(fk(&interner, 0x04_64EF)),
                FieldValue::FormKey(fo4_fk(&interner, 0x0F_4AEA)),
            ]),
            2,
        );

        assert!(add_fo4_grip_keywords(&mut record, &[0x04_64EF], &interner));

        let kwda = SubrecordSig::from_str("KWDA").unwrap();
        let FieldValue::List(items) = &record.fields.iter().find(|e| e.sig == kwda).unwrap().value
        else {
            panic!("KWDA should still be a list");
        };
        assert_eq!(items.len(), 3);
        assert_eq!(keyword_count(&record), 3);
        assert!(items.iter().any(|item| matches!(
            item,
            FieldValue::FormKey(added)
                if added.local == 0x04_64EF && is_fo4_grip_keyword(added, &interner)
        )));
    }

    #[test]
    fn grip_is_restored_to_the_raw_bytes_keyword_array() {
        let interner = StringInterner::new();
        let mut bytes = Vec::new();
        for raw in [0x08_04_64EFu32, 0x00_0F_4AEA] {
            bytes.extend_from_slice(&raw.to_le_bytes());
        }
        let mut record = weapon_with_keywords(
            &interner,
            FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes)),
            2,
        );

        assert!(add_fo4_grip_keywords(&mut record, &[0x04_64EF], &interner));
        assert_eq!(keyword_count(&record), 3);

        let kwda = SubrecordSig::from_str("KWDA").unwrap();
        let FieldValue::Bytes(bytes) = &record.fields.iter().find(|e| e.sig == kwda).unwrap().value
        else {
            panic!("KWDA should still be raw bytes");
        };
        // Fallout4.esm is master index 0, so the appended row is the bare object id.
        assert_eq!(&bytes[8..12], &0x00_04_64EFu32.to_le_bytes());
    }
}
