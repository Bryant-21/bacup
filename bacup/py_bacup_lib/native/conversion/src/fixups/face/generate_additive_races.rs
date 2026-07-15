//! Synthesize additive Race records for weapon/armor conversions.
//!

//! # What this does
//! When a weapon/armor conversion walks a base-game Race (HumanRace,
//! PowerArmorRace, etc.), we must NOT write a full Race override — it would
//! crash the game. Instead, synthesize a NEW *additive* Race record that
//! contains ONLY the subgraphs relevant to the weapon being converted, with
//! the `SubgraphAdditiveRace` (`SADD`) subrecord pointing at the target-game
//! base Race (or its dedicated additive parent).
//!
//! Primary pass:
//!   1. Collect animation keyword FormKeys from converted source WEAP/ARMO
//!      records and mapped OMOD FormID properties.
//!   2. Derive a short `weapon_name` from the root EditorID for naming.
//!   3. For each RACE record in the target plugin:
//!        a. Vanilla resolution via `mapper.find_vanilla_fk(eid, RACE)`. Skip
//!           when no vanilla equivalent exists.
//!        b. Parse canonical subgraph blocks (`SubgraphBlock`).
//!        c. Filter blocks whose `target_keywords` (STKD) overlap with the
//!           weapon keyword set.
//!        d. Resolve `target_base_fk` from the per-EID
//!           `FO4_ADDITIVE_PARENTS` map; fall back to vanilla_fk.
//!        e. Build a stripped template from the source race.
//!        f. Rewrite FormKey refs in the matching blocks
//!           (mapper-aware drop for refs still pointing at source masters).
//!        g. Drop blocks whose rewritten `target_keywords` set is already
//!           provided by a target base-game/DLC RACE subgraph.
//!        h. Compose the additive RACE record.
//!        i. Allocate a target FormKey via `mapper.allocate_or_resolve` and
//!           insert via `add_record_native`.
//!
//! Secondary pass (PA):
//!   - For every freshly-generated HumanRace-additive in the plugin, derive a
//!     matching `PowerArmorRaceAdditive` by substituting Character animation
//!     paths with PowerArmor equivalents.
//!
//! # Guards (matching Python)
//! - Creature root type (NPC_/LVLN) → no-op (creatures need full-clone races).
//! - No vanilla EID match → skip that race.
//! - No subgraph blocks → skip that race.
//! - No matching blocks → skip that race.
//!

//! # Current decode limits
//! - Target-records DB lookup is not implemented; `load_target_race_template`
//!   returns `None` and this fixup falls back to the source record as template.
//! - SRAF and KWDA both currently decode as `FieldValue::Bytes`. The keyword
//!   accumulator handles the bytes path plus `List<FormKey>`/`FormKey` shapes.

use rustc_hash::{FxHashMap, FxHashSet};

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

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        applies_for_config(config)
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
        if eid_str == "PowerArmorRace" {
            continue;
        }
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

    let Some(pa_base_fk) = lookup_additive_parent_str("PowerArmorRace")
        .and_then(|s| parse_render_fk(s, mapper.interner))
    else {
        return Ok(report);
    };

    for (human_eid, human_fk) in &human_additives_to_derive {
        let human_rec = match session.record_decoded(human_fk, target_schema, mapper.interner) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let human_blocks = parse_canonical_subgraphs(&human_rec);
        let pa_blocks = derive_pa_subgraph_blocks(&human_blocks, mapper.interner);
        if pa_blocks.is_empty() {
            continue;
        }

        let pa_eid = human_eid.replacen("HumanRaceAdditive", "PowerArmorRaceAdditive", 1);
        if existing_race_eids.contains(&pa_eid) {
            continue;
        }
        let pa_eid_sym = mapper.interner.intern(&pa_eid);

        let stripped = strip_template_subgraph_fields(&human_rec);
        let mut pa_record = build_additive_race_record(stripped, pa_base_fk, &pa_blocks);
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
    for sig_name in ["WEAP", "ARMO"] {
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

fn collect_omod_formid_property_keywords(
    record: &Record,
    interner: &StringInterner,
) -> Vec<FormKey> {
    let mut out = Vec::new();
    for entry in &record.fields {
        collect_omod_formid_property_keywords_from_value(
            &entry.value,
            record.form_key.plugin,
            interner,
            &mut out,
        );
    }
    out
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
        let Some((replacement, needs_paired_path)) = fo4_behaviour_graph_for_fo76_graph(path)
        else {
            continue;
        };
        block.behaviour_graph = interner.intern(replacement);
        if needs_paired_path {
            ensure_subgraph_path(block, "Actors\\Character\\Animations\\Paired", interner);
        }
    }
}

/// FO76 graphs with no FO4 analog at all — a retained block would bind a
/// behavior FO4 cannot load, so the block is dropped instead of remapped.
fn fo76_graph_without_fo4_equivalent(path: &str) -> bool {
    path.eq_ignore_ascii_case("Actors\\Character\\Behaviors\\FaceGen.hkx")
}

fn fo4_behaviour_graph_for_fo76_graph(path: &str) -> Option<(&'static str, bool)> {
    if path.eq_ignore_ascii_case(
        "Actors\\Character\\Behaviors\\NoHandIKRelaxedGunWrappingBehavior.hkx",
    ) {
        return Some((
            "Actors\\Character\\Behaviors\\NoHandIKRelaxedWeaponWrappingBehavior.hkx",
            false,
        ));
    }
    if path.eq_ignore_ascii_case("Actors\\Character\\Behaviors\\ChargeUpWrappingGunBehavior.hkx") {
        return Some((
            "Actors\\Character\\Behaviors\\ChargeUpWrappingWeaponBehavior.hkx",
            true,
        ));
    }
    if [
        "Actors\\Character\\Behaviors\\GunBehavior.hkx",
        "Actors\\Character\\Behaviors\\BigGunWrappingBehavior.hkx",
        "Actors\\Character\\Behaviors\\ShoulderMountedGunWrappingBehavior.hkx",
        "Actors\\Character\\Behaviors\\BOSLauncherWrappingBehavior.hkx",
    ]
    .iter()
    .any(|candidate| path.eq_ignore_ascii_case(candidate))
    {
        return Some(("Actors\\Character\\Behaviors\\WeaponBehavior.hkx", true));
    }
    // FO4 has no binocular behaviors; bind them like a held gun.
    if [
        "Actors\\Character\\Behaviors\\BinocularBehavior.hkx",
        "Actors\\Character\\Behaviors\\BinocularInjuredWrappingBehavior.hkx",
    ]
    .iter()
    .any(|candidate| path.eq_ignore_ascii_case(candidate))
    {
        return Some(("Actors\\Character\\Behaviors\\WeaponBehavior.hkx", true));
    }
    // FO4 splits injured movement per-arm; the generic FO76 wrapper falls back
    // to plain movement.
    if path.eq_ignore_ascii_case("Actors\\Character\\Behaviors\\MTInjuredWrappingBehavior.hkx") {
        return Some(("Actors\\Character\\Behaviors\\MTBehavior.hkx", false));
    }
    // FO4 beds run on the generic furniture graph.
    if path.eq_ignore_ascii_case("Actors\\Character\\Behaviors\\FurnitureBed.hkx") {
        return Some(("Actors\\Character\\Behaviors\\FurnitureBehavior.hkx", false));
    }
    // FO4 cannot run shipped FO76 1st-person wrapping behaviors — a 1P block
    // pointing at one leaves the weapon unposed/unfireable in 1st person. Every
    // FO76-only 1P gun wrapper must retarget to the vanilla wrapper the fan
    // ports use (PepperShaker: SpinUpDown → BigGuns).
    if [
        "Actors\\Character\\_1stPerson\\Behaviors\\BigGunsSpinUpDown_GunWrappingBehavior.hkx",
        "Actors\\Character\\_1stPerson\\Behaviors\\BigGunsSpinningBarrel_GunWrappingBehavior.hkx",
    ]
    .iter()
    .any(|candidate| path.eq_ignore_ascii_case(candidate))
    {
        return Some((
            "Actors\\Character\\_1stPerson\\Behaviors\\BigGuns_GunWrappingBehavior.hkx",
            false,
        ));
    }
    if path.eq_ignore_ascii_case(
        "Actors\\Character\\_1stPerson\\Behaviors\\BOSLauncher_GunWrappingBehavior.hkx",
    ) {
        return Some((
            "Actors\\Character\\_1stPerson\\Behaviors\\ShoulderMounted_GunWrappingBehavior.hkx",
            false,
        ));
    }
    // FO4 has no binocular wrappers; the FO76 base binocular 1P block already
    // rides GunBehavior, so the injured variant collapses onto it.
    if path.eq_ignore_ascii_case(
        "Actors\\Character\\_1stPerson\\Behaviors\\InjuredBinoculars_GunWrappingBehavior.hkx",
    ) {
        return Some((
            "Actors\\Character\\_1stPerson\\Behaviors\\GunBehavior.hkx",
            false,
        ));
    }
    if path.eq_ignore_ascii_case(
        "Actors\\Character\\_1stPerson\\Behaviors\\InjuredMTWrappingBehavior.hkx",
    ) {
        return Some((
            "Actors\\Character\\_1stPerson\\Behaviors\\MTBehavior.hkx",
            false,
        ));
    }
    if path.eq_ignore_ascii_case("Actors\\Character\\_1stPerson\\Behaviors\\Pipboy2000.hkx") {
        return Some((
            "Actors\\Character\\_1stPerson\\Behaviors\\Pipboy.hkx",
            false,
        ));
    }
    None
}

fn ensure_subgraph_path(block: &mut SubgraphBlock, path: &str, interner: &StringInterner) {
    let already_present = block.paths.iter().any(|sym| {
        interner
            .resolve(*sym)
            .is_some_and(|existing| existing.eq_ignore_ascii_case(path))
    });
    if !already_present {
        block.paths.push(interner.intern(path));
    }
}

/// Keep only blocks that carry at least one FO76-unique target keyword — i.e. a
/// keyword that was converted into the OUTPUT plugin (no FO4 vanilla EID match).
/// A block whose every target keyword resolves to an FO4 master plugin is a
/// base-game animation the engine already provides, so it is dropped.
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

fn load_target_race_template_from_masters(
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
    use crate::sym::StringInterner;
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
    fn normalizes_fo76_gun_wrapper_graph_to_fo4_weapon_wrapper() {
        let interner = StringInterner::new();
        let mut blocks = vec![SubgraphBlock {
            behaviour_graph: interner
                .intern("Actors\\Character\\Behaviors\\NoHandIKRelaxedGunWrappingBehavior.hkx"),
            paths: vec![],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        }];

        normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);

        assert_eq!(
            interner.resolve(blocks[0].behaviour_graph).unwrap(),
            "Actors\\Character\\Behaviors\\NoHandIKRelaxedWeaponWrappingBehavior.hkx"
        );
    }

    #[test]
    fn normalizes_fo76_big_gun_wrapper_to_fo4_weapon_behavior() {
        let interner = StringInterner::new();
        let mut blocks = vec![SubgraphBlock {
            behaviour_graph: interner
                .intern("Actors\\Character\\Behaviors\\BigGunWrappingBehavior.hkx"),
            paths: vec![
                interner.intern("Actors\\Character\\Animations\\Weapon\\M2\\Player"),
                interner.intern("Actors\\Character\\Animations\\Weapon\\M2"),
                interner.intern("Actors\\Character\\Animations\\Weapon\\GripHeavy"),
            ],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        }];

        normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);

        assert_eq!(
            interner.resolve(blocks[0].behaviour_graph).unwrap(),
            "Actors\\Character\\Behaviors\\WeaponBehavior.hkx"
        );
        let paths: Vec<String> = blocks[0]
            .paths
            .iter()
            .map(|s| interner.resolve(*s).unwrap().to_string())
            .collect();
        assert_eq!(
            paths
                .iter()
                .filter(|p| p.eq_ignore_ascii_case("Actors\\Character\\Animations\\Paired"))
                .count(),
            1
        );
    }

    #[test]
    fn normalizes_fo76_third_person_gun_wrappers_to_fo4_weapon_behavior() {
        let interner = StringInterner::new();
        for graph in [
            "Actors\\Character\\Behaviors\\GunBehavior.hkx",
            "Actors\\Character\\Behaviors\\BigGunWrappingBehavior.hkx",
            "Actors\\Character\\Behaviors\\ShoulderMountedGunWrappingBehavior.hkx",
            "Actors\\Character\\Behaviors\\BOSLauncherWrappingBehavior.hkx",
        ] {
            let mut blocks = vec![SubgraphBlock {
                behaviour_graph: interner.intern(graph),
                paths: vec![interner.intern("Actors\\Character\\Animations\\Weapon\\TestGun")],
                subgraph_keywords: vec![],
                target_keywords: vec![],
                flags_bytes: None,
            }];

            normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);

            assert_eq!(
                interner.resolve(blocks[0].behaviour_graph).unwrap(),
                "Actors\\Character\\Behaviors\\WeaponBehavior.hkx"
            );
            assert!(blocks[0].paths.iter().any(|s| {
                interner.resolve(*s).is_some_and(|p| {
                    p.eq_ignore_ascii_case("Actors\\Character\\Animations\\Paired")
                })
            }));
        }
    }

    #[test]
    fn normalizes_fo76_charge_up_gun_wrapper_to_fo4_weapon_wrapper() {
        let interner = StringInterner::new();
        let mut blocks = vec![SubgraphBlock {
            behaviour_graph: interner
                .intern("Actors\\Character\\Behaviors\\ChargeUpWrappingGunBehavior.hkx"),
            paths: vec![interner.intern("Actors\\Character\\Animations\\Weapon\\ChargedGun")],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        }];

        normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);

        assert_eq!(
            interner.resolve(blocks[0].behaviour_graph).unwrap(),
            "Actors\\Character\\Behaviors\\ChargeUpWrappingWeaponBehavior.hkx"
        );
        assert!(blocks[0].paths.iter().any(|s| {
            interner
                .resolve(*s)
                .is_some_and(|p| p.eq_ignore_ascii_case("Actors\\Character\\Animations\\Paired"))
        }));
    }

    #[test]
    fn does_not_normalize_first_person_gun_wrappers() {
        let interner = StringInterner::new();
        let graph = "Actors\\Character\\_1stPerson\\Behaviors\\BigGuns_GunWrappingBehavior.hkx";
        let mut blocks = vec![SubgraphBlock {
            behaviour_graph: interner.intern(graph),
            paths: vec![interner.intern("Actors\\Character\\_1stPerson\\Animations\\Paired")],
            subgraph_keywords: vec![],
            target_keywords: vec![],
            flags_bytes: None,
        }];

        normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);

        assert_eq!(interner.resolve(blocks[0].behaviour_graph).unwrap(), graph);
    }

    #[test]
    fn normalizes_fo76_only_first_person_gun_wrappers_to_vanilla() {
        let interner = StringInterner::new();
        let cases = [
            (
                "Actors\\Character\\_1stPerson\\Behaviors\\BigGunsSpinUpDown_GunWrappingBehavior.hkx",
                "Actors\\Character\\_1stPerson\\Behaviors\\BigGuns_GunWrappingBehavior.hkx",
            ),
            (
                "Actors\\Character\\_1stPerson\\Behaviors\\BigGunsSpinningBarrel_GunWrappingBehavior.hkx",
                "Actors\\Character\\_1stPerson\\Behaviors\\BigGuns_GunWrappingBehavior.hkx",
            ),
            (
                "Actors\\Character\\_1stPerson\\Behaviors\\BOSLauncher_GunWrappingBehavior.hkx",
                "Actors\\Character\\_1stPerson\\Behaviors\\ShoulderMounted_GunWrappingBehavior.hkx",
            ),
            (
                "Actors\\Character\\_1stPerson\\Behaviors\\InjuredBinoculars_GunWrappingBehavior.hkx",
                "Actors\\Character\\_1stPerson\\Behaviors\\GunBehavior.hkx",
            ),
            (
                "Actors\\Character\\_1stPerson\\Behaviors\\InjuredMTWrappingBehavior.hkx",
                "Actors\\Character\\_1stPerson\\Behaviors\\MTBehavior.hkx",
            ),
            (
                "Actors\\Character\\_1stPerson\\Behaviors\\Pipboy2000.hkx",
                "Actors\\Character\\_1stPerson\\Behaviors\\Pipboy.hkx",
            ),
        ];
        for (fo76_graph, fo4_graph) in cases {
            let paired = interner.intern("Actors\\Character\\_1stPerson\\Animations\\Paired");
            let mut blocks = vec![SubgraphBlock {
                behaviour_graph: interner.intern(fo76_graph),
                paths: vec![paired],
                subgraph_keywords: vec![],
                target_keywords: vec![],
                flags_bytes: None,
            }];

            normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);

            assert_eq!(
                interner.resolve(blocks[0].behaviour_graph).unwrap(),
                fo4_graph
            );
            assert_eq!(
                blocks[0].paths,
                vec![paired],
                "1P mapping must not inject 3rd-person paths"
            );
        }
    }

    #[test]
    fn normalizes_fo76_binocular_and_injured_mt_graphs() {
        let interner = StringInterner::new();
        let cases = [
            (
                "Actors\\Character\\Behaviors\\BinocularBehavior.hkx",
                "Actors\\Character\\Behaviors\\WeaponBehavior.hkx",
            ),
            (
                "Actors\\Character\\Behaviors\\BinocularInjuredWrappingBehavior.hkx",
                "Actors\\Character\\Behaviors\\WeaponBehavior.hkx",
            ),
            (
                "Actors\\Character\\Behaviors\\MTInjuredWrappingBehavior.hkx",
                "Actors\\Character\\Behaviors\\MTBehavior.hkx",
            ),
            (
                "Actors\\Character\\Behaviors\\FurnitureBed.hkx",
                "Actors\\Character\\Behaviors\\FurnitureBehavior.hkx",
            ),
        ];
        for (fo76_graph, fo4_graph) in cases {
            let mut blocks = vec![SubgraphBlock {
                behaviour_graph: interner.intern(fo76_graph),
                paths: vec![],
                subgraph_keywords: vec![],
                target_keywords: vec![],
                flags_bytes: None,
            }];

            normalize_fo76_fo4_subgraph_blocks(&mut blocks, &interner);

            assert_eq!(
                interner.resolve(blocks[0].behaviour_graph).unwrap(),
                fo4_graph
            );
        }
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
}
