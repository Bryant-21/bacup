//! Fixup: drop or pre-remap non-vanilla RACE records on weapon/armor conversion
//! roots.
//!

//!
//! # What this does
//! For weapon/armor root conversions (i.e. not NPC_/LVLN), RACE records in the
//! target plugin that have no FO4 vanilla equivalent (e.g. ScorchedRace,
//! ZetanInvaderRace) are removed from the target plugin entirely — they cannot
//! become additive children of any vanilla race.
//!
//! RACE records that *do* have a vanilla equivalent (HumanRace, PowerArmorRace,
//! HumanRaceSubGraphData, etc.) are kept in the target plugin so downstream
//! passes can read their subgraph fields.  Their source-to-target mapping is
//! pre-seeded to the vanilla FormKey so all cross-record references resolve to
//! the vanilla parent instead of the newly-allocated local FK.
//!
//! # Guards
//! - `is_whole_plugin = true` → no-op.
//! - Creature root type (NPC_/LVLN) → no-op (creatures need full-clone races).
//!
//! # Vanilla lookup
//! Uses `FormKeyMapper::find_vanilla_fk(eid, RACE_SIG)`, which queries the
//! target-master EID index unconditionally (no `use_base_game_assets` gate).

use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode};
use crate::record::Record;
use crate::session::PluginSession;
use crate::sym::StringInterner;

// Re-export for tests.
pub use crate::fixups::prune_orphaned_records::is_creature_root_sig;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FilterNonVanillaRacesForWeaponRootsFixup;

impl Fixup for FilterNonVanillaRacesForWeaponRootsFixup {
    fn name(&self) -> &'static str {
        "filter_non_vanilla_races_for_weapon_roots"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::GraphOnly
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        // Skip for whole-plugin conversions.
        if ctx.config.is_whole_plugin {
            return false;
        }
        // Skip for creature roots (NPC_/LVLN).
        // When root_sig is None (unknown/whole-plugin), also skip.
        match ctx.config.root_sig {
            Some(sig) => !is_creature_root_sig(sig),
            None => false,
        }
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        if config.is_whole_plugin {
            return false;
        }
        match config.root_sig {
            Some(sig) => !is_creature_root_sig(sig),
            None => false,
        }
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

        let mut report = FixupReport::empty();

        // Collect all RACE FormKeys in the target plugin.
        let race_fks = session
            .form_keys_of_sig(race_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        if race_fks.is_empty() {
            return Ok(report);
        }

        let mut dropped_eids: Vec<String> = Vec::new();
        let mut remapped_eids: Vec<String> = Vec::new();

        for fk in &race_fks {
            // Read the record to obtain the EditorID.
            let record = match session.record_decoded(fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("race_filter_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            let eid_str = eid_of(&record, mapper.interner).unwrap_or_default();

            // Determine the outcome for this race.
            let outcome = apply_to_record(&eid_str, *fk, race_sig, mapper);

            match outcome {
                RaceOutcome::Drop => {
                    // Remove from target plugin — no vanilla equiv exists.
                    if session
                        .remove_record(fk)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?
                    {
                        dropped_eids.push(eid_str);
                        report.records_dropped += 1;
                    }
                }
                RaceOutcome::Remap => {
                    // Mapping already seeded by apply_to_record.
                    remapped_eids.push(eid_str);
                    // record_changed not incremented: the record itself is kept as-is;
                    // the seeded mapping redirects *references* to it.
                }
                RaceOutcome::Keep => {
                    // Non-race or already handled — nothing to do.
                }
            }
        }

        if !dropped_eids.is_empty() {
            dropped_eids.sort();
            let msg = mapper.interner.intern(&format!(
                "Dropped {} non-vanilla Race(s) on weapon root: {:?}",
                dropped_eids.len(),
                dropped_eids,
            ));
            report.warnings.push(msg);
        }
        if !remapped_eids.is_empty() {
            remapped_eids.sort();
            let msg = mapper.interner.intern(&format!(
                "Pre-remapped {} vanilla Race(s) on weapon root: {:?}",
                remapped_eids.len(),
                remapped_eids,
            ));
            report.warnings.push(msg);
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Per-record logic (extracted for testability)
// ---------------------------------------------------------------------------

/// What to do with a single RACE record.
#[derive(Debug, PartialEq)]
pub enum RaceOutcome {
    /// No vanilla equivalent — drop from the target plugin.
    Drop,
    /// Vanilla equivalent found — pre-seed the mapping and keep the record.
    Remap,
    /// Empty EID — leave unchanged (guard against empty-EID records).
    Keep,
}

/// Decide the outcome for one RACE record and, if `Remap`, seed the mapping.
///
/// - `eid_str` — the EditorID of the source RACE record.
/// - `source_fk` — the source (FO76) FormKey for this record.
/// - `race_sig` — the `SigCode` for "RACE".
/// - `mapper` — mutable mapper; seeded via `add_mapping` when outcome is `Remap`.
///

pub fn apply_to_record(
    eid_str: &str,
    source_fk: FormKey,
    race_sig: SigCode,
    mapper: &mut FormKeyMapper,
) -> RaceOutcome {
    if eid_str.is_empty() {
        return RaceOutcome::Keep;
    }

    match mapper.find_vanilla_fk(eid_str, race_sig) {
        None => RaceOutcome::Drop,
        Some(vanilla_fk) => {
            // Pre-seed: source FK → vanilla FK so all references redirect.
            mapper.add_mapping(source_fk, vanilla_fk);
            RaceOutcome::Remap
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: extract EditorID string from a decoded Record
// ---------------------------------------------------------------------------

fn eid_of(record: &Record, interner: &StringInterner) -> Option<String> {
    let sym = record.eid?;
    Some(interner.resolve(sym)?.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::{FixupConfig, FixupContext};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::FormKey;
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use std::sync::Arc;

    fn race_sig() -> SigCode {
        SigCode::from_str("RACE").unwrap()
    }

    fn make_fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    // Build a mapper whose EID index contains the given (eid, fk) pairs.
    // The index key is lower-cased to match `find_vanilla_fk`, which normalizes
    // its lookup key via `to_ascii_lowercase` (the target-master EID index is
    // built from normalized EditorIDs in the real run).
    fn make_mapper_with_index<'a>(
        pairs: &[(String, FormKey)],
        interner: &'a mut StringInterner,
    ) -> FormKeyMapper<'a> {
        let sig = race_sig();
        let eid_iter: Vec<_> = pairs
            .iter()
            .map(|(eid, fk)| {
                let sym = interner.intern(&eid.to_ascii_lowercase());
                (sym, *fk, sig)
            })
            .collect();
        FormKeyMapper::new(
            eid_iter,
            MapperOptions {
                output_plugin_name: "Output.esp".to_string(),
                ..Default::default()
            },
            interner,
        )
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn non_vanilla_race_returns_drop() {
        let mut interner = StringInterner::new();
        let source_fk = make_fk(0x000800, "SeventySix.esm", &mut interner);
        // No EID entries in the index → find_vanilla_fk returns None.
        let mut mapper = make_mapper_with_index(&[], &mut interner);

        let outcome = apply_to_record("ScorchedRace", source_fk, race_sig(), &mut mapper);
        assert_eq!(outcome, RaceOutcome::Drop);
        // No mapping should have been seeded.
        assert!(mapper.lookup(source_fk).is_none());
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn vanilla_race_returns_remap_and_seeds_mapping() {
        let mut interner = StringInterner::new();
        let source_fk = make_fk(0x000900, "SeventySix.esm", &mut interner);
        let vanilla_fk = make_fk(0x000019, "Fallout4.esm", &mut interner);

        let pairs = vec![("HumanRace".to_string(), vanilla_fk)];
        let mut mapper = make_mapper_with_index(&pairs, &mut interner);

        let outcome = apply_to_record("HumanRace", source_fk, race_sig(), &mut mapper);
        assert_eq!(outcome, RaceOutcome::Remap);
        // Mapping must be seeded.
        assert_eq!(mapper.lookup(source_fk), Some(vanilla_fk));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn empty_eid_returns_keep() {
        let mut interner = StringInterner::new();
        let source_fk = make_fk(0x000A00, "SeventySix.esm", &mut interner);
        let mut mapper = make_mapper_with_index(&[], &mut interner);

        let outcome = apply_to_record("", source_fk, race_sig(), &mut mapper);
        assert_eq!(outcome, RaceOutcome::Keep);
        assert!(mapper.lookup(source_fk).is_none());
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn mixed_races_drop_and_remap() {
        let mut interner = StringInterner::new();
        // Two races: HumanRace (has vanilla equiv) and ZetanInvaderRace (none).
        let human_src = make_fk(0x000B00, "SeventySix.esm", &mut interner);
        let human_vanilla = make_fk(0x000019, "Fallout4.esm", &mut interner);
        let zetan_src = make_fk(0x000C00, "SeventySix.esm", &mut interner);

        let pairs = vec![("HumanRace".to_string(), human_vanilla)];
        let mut mapper = make_mapper_with_index(&pairs, &mut interner);

        let h_outcome = apply_to_record("HumanRace", human_src, race_sig(), &mut mapper);
        let z_outcome = apply_to_record("ZetanInvaderRace", zetan_src, race_sig(), &mut mapper);

        assert_eq!(h_outcome, RaceOutcome::Remap);
        assert_eq!(z_outcome, RaceOutcome::Drop);
        assert_eq!(mapper.lookup(human_src), Some(human_vanilla));
        assert!(mapper.lookup(zetan_src).is_none());
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_false_for_npc_root() {
        let interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").unwrap());
        let config = FixupConfig {
            is_whole_plugin: false,
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            ..Default::default()
        };
        let mut ctx_interner = StringInterner::new();
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
        let fixup = FilterNonVanillaRacesForWeaponRootsFixup;
        assert!(!fixup.applies_to(&ctx));
        drop(interner);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_false_for_whole_plugin() {
        let schema = Arc::new(AuthoringSchema::for_game("fo4").unwrap());
        let config = FixupConfig {
            is_whole_plugin: true,
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
        let mut ctx_interner = StringInterner::new();
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
        let fixup = FilterNonVanillaRacesForWeaponRootsFixup;
        assert!(!fixup.applies_to(&ctx));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_true_for_weap_root() {
        let schema = Arc::new(AuthoringSchema::for_game("fo4").unwrap());
        let config = FixupConfig {
            is_whole_plugin: false,
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
        let mut ctx_interner = StringInterner::new();
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
        let fixup = FilterNonVanillaRacesForWeaponRootsFixup;
        assert!(fixup.applies_to(&ctx));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_false_for_lvln_root() {
        let schema = Arc::new(AuthoringSchema::for_game("fo4").unwrap());
        let config = FixupConfig {
            is_whole_plugin: false,
            root_sig: Some(SigCode::from_str("LVLN").unwrap()),
            ..Default::default()
        };
        let mut ctx_interner = StringInterner::new();
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
        let fixup = FilterNonVanillaRacesForWeaponRootsFixup;
        assert!(!fixup.applies_to(&ctx));
    }
}
