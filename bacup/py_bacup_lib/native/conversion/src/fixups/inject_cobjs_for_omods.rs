//! Fixup: inject ConstructibleObject records that reference OMODs in the target.

//!
//! # What this does
//! After translation, the target plugin contains OMOD (ObjectModifications)
//! records but typically lacks their corresponding COBJ (ConstructibleObjects)
//! workbench recipes. Without the COBJ records, the attachment mod categories
//! are invisible at the in-game workbench.
//!
//! This fixup scans every COBJ record in the source plugin. For each COBJ that
//! references at least one OMOD FormKey (via any `FieldValue::FormKey` in the
//! record), and that has not already been translated into the target, the fixup:
//!   1. Reads the COBJ from the source plugin.
//!   2. Allocates a target FormKey and rewrites all cross-plugin FormKey
//!      references via `FormKeyMapper`.
//!   3. Writes the rewritten record into the target plugin via
//!      `add_record_native`.
//!
//! # Guard
//! When the conversion root is a creature type (NPC_/LVLN), creature sub-graphs
//! never have OMOD/COBJ chains. Skip entirely.
//!
//! # FK matching strategy
//! The source COBJ records reference *source* FormKeys for the OMODs. The
//! target already has translated copies whose FormKeys differ. We build a
//! reverse map (`target FK → source FK`) from `mapper.source_to_target`, then
//! use the source FK set to match against source COBJ FK references. This
//! avoids needing an external SQLite reverse-reference DB: the mapper already
//! tracks every source→target allocation made during `translate_all`.
//!
//! # Components / cross-plugin refs
//! COBJ Components (c_Silver, etc.) are handled naturally: `rewrite_record`
//! remaps all `FieldValue::FormKey` references through the mapper, which
//! vanilla-remaps them against the target DB.

use rustc_hash::FxHashSet;

use crate::fixups::prune_orphaned_records::is_creature_root_sig;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct InjectCobjsForOmodsFixup;

impl Fixup for InjectCobjsForOmodsFixup {
    fn name(&self) -> &'static str {
        "inject_cobjs_for_omods"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        if let Some(sig) = ctx.config.root_sig {
            if is_creature_root_sig(sig) {
                return false;
            }
        }
        // No source plugin to read from → nothing to inject.
        ctx.source_handle_id != 0
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        if let Some(sig) = config.root_sig {
            if is_creature_root_sig(sig) {
                return false;
            }
        }
        session.source_id().is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let source_schema = session
            .source_schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        // ── 1. Collect OMOD FormKeys already in the target ───────────────────
        let omod_sig =
            SigCode::from_str("OMOD").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let target_omod_fks = session
            .form_keys_of_sig(omod_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        if target_omod_fks.is_empty() {
            return Ok(report);
        }

        // ── 2. Build source OMOD FK set via reverse lookup in mapper ─────────
        // mapper.source_to_target maps source FK → target FK.
        // We need the inverse: for each target OMOD FK, find the source FK.
        let target_omod_set: FxHashSet<FormKey> = target_omod_fks.into_iter().collect();
        let source_omod_fks: FxHashSet<FormKey> =
            collect_source_fks_for_targets(&target_omod_set, mapper);

        if source_omod_fks.is_empty() {
            // No source-side mappings found for any target OMOD → nothing to inject.
            return Ok(report);
        }

        // ── 3. Collect FormKeys already in the target (dedup guard) ──────────
        let mut existing_target_fks = collect_all_target_fks(session, mapper)?;

        // ── 4. Iterate source COBJ records ───────────────────────────────────
        let cobj_sig =
            SigCode::from_str("COBJ").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let source_cobj_fks = session
            .source_form_keys_of_sig(cobj_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        if source_cobj_fks.is_empty() {
            return Ok(report);
        }

        // ── 5. For each source COBJ: check if it references an OMOD ─────────
        for src_fk in source_cobj_fks {
            let src_record = match session.source_record_decoded(
                &src_fk,
                source_schema.as_ref(),
                mapper.interner,
            ) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("inject_cobjs_for_omods:read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            // Check if this COBJ record references any source OMOD FK.
            if !cobj_references_omod(&src_record, &source_omod_fks) {
                continue;
            }

            // Allocate target FK and rewrite cross-plugin references.
            let mut translated = src_record;
            let target_fk =
                mapper.allocate_or_resolve(translated.form_key, translated.eid, cobj_sig);
            translated.form_key = target_fk;

            // Skip if this COBJ already exists in the target (e.g. was already
            // translated by translate_all because it referenced a non-OMOD field
            // that pulled it in).
            if existing_target_fks.contains(&target_fk) {
                continue;
            }

            if let Err(e) = mapper.rewrite_record(&mut translated) {
                let w = mapper
                    .interner
                    .intern(&format!("inject_cobjs_for_omods:rewrite_err:{e}"));
                report.warnings.push(w);
                // Continue: a partial rewrite is better than no injection.
            }

            match session.add_record(translated, target_schema.as_ref(), mapper.interner) {
                Ok(()) => {
                    report.records_added += 1;
                    existing_target_fks.insert(target_fk);
                }
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("inject_cobjs_for_omods:add_err:{e}"));
                    report.warnings.push(w);
                }
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Helper: synthesize_cobj_for_omod (extracted for testability)
// ---------------------------------------------------------------------------

/// Given a source COBJ record, check whether it references any of the given
/// source OMOD FormKeys (anywhere in its field tree).
///
/// Extracted as a standalone function for unit-test access.
pub fn cobj_references_omod(record: &Record, omod_fks: &FxHashSet<FormKey>) -> bool {
    record
        .fields
        .iter()
        .any(|entry| field_value_contains_fk(&entry.value, omod_fks))
}

/// Recursively walk a `FieldValue` and return `true` if any `FormKey` leaf
/// matches one of the given keys.
fn field_value_contains_fk(value: &FieldValue, keys: &FxHashSet<FormKey>) -> bool {
    match value {
        FieldValue::FormKey(fk) => keys.contains(fk),
        FieldValue::Struct(fields) => fields.iter().any(|(_, v)| field_value_contains_fk(v, keys)),
        FieldValue::List(items) => items.iter().any(|v| field_value_contains_fk(v, keys)),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Helper: build source FK set from the mapper's source_to_target table
// ---------------------------------------------------------------------------

/// Given a set of *target* FormKeys, return the corresponding *source* FormKeys
/// by inverting `mapper.source_to_target`.
///
/// The mapper owns `source_to_target: FxHashMap<source_fk, target_fk>`. We do
/// a linear scan (the map is never large enough to warrant a pre-built inverse).
fn collect_source_fks_for_targets(
    target_fks: &FxHashSet<FormKey>,
    mapper: &FormKeyMapper,
) -> FxHashSet<FormKey> {
    // Access the mapper's source_to_target via a public method.
    // FormKeyMapper exposes source_to_target through source_to_target_iter().
    let mut out = FxHashSet::default();
    for (src, tgt) in mapper.source_to_target_iter() {
        if target_fks.contains(&tgt) {
            out.insert(src);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Helper: collect all FK keys already present in the target plugin
// ---------------------------------------------------------------------------

/// Return the set of all FormKeys that are currently in the target plugin,
/// across all record types.
fn collect_all_target_fks(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
) -> Result<FxHashSet<FormKey>, FixupError> {
    let sigs = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;

    let mut out = FxHashSet::default();
    for sig in sigs {
        let fks = session
            .form_keys_of_sig(sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        out.extend(fks);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;
    use rustc_hash::FxHashSet;

    fn make_interner() -> StringInterner {
        StringInterner::new()
    }

    fn make_fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        let plugin_sym = interner.intern(plugin);
        FormKey {
            local,
            plugin: plugin_sym,
        }
    }

    fn make_cobj_with_fk(
        self_fk: FormKey,
        referenced_fk: FormKey,
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("COBJ").unwrap();
        let cnam_sig = SubrecordSig::from_str("CNAM").unwrap();
        Record {
            sig,
            form_key: self_fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: cnam_sig,
                value: FieldValue::FormKey(referenced_fk),
            }],
            warnings: smallvec::SmallVec::new(),
        }
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn cobj_references_omod_direct_match() {
        let mut interner = make_interner();
        let omod_fk = make_fk(0x001234, "SeventySix.esm", &mut interner);
        let self_fk = make_fk(0x005678, "SeventySix.esm", &mut interner);

        let record = make_cobj_with_fk(self_fk, omod_fk, &mut interner);

        let mut omod_set: FxHashSet<FormKey> = FxHashSet::default();
        omod_set.insert(omod_fk);

        assert!(cobj_references_omod(&record, &omod_set));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn cobj_references_omod_no_match() {
        let mut interner = make_interner();
        let omod_fk = make_fk(0x001234, "SeventySix.esm", &mut interner);
        let other_fk = make_fk(0x009999, "SeventySix.esm", &mut interner);
        let self_fk = make_fk(0x005678, "SeventySix.esm", &mut interner);

        let record = make_cobj_with_fk(self_fk, other_fk, &mut interner);

        let mut omod_set: FxHashSet<FormKey> = FxHashSet::default();
        omod_set.insert(omod_fk);

        assert!(!cobj_references_omod(&record, &omod_set));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn cobj_references_omod_empty_fields() {
        let mut interner = make_interner();
        let omod_fk = make_fk(0x001234, "SeventySix.esm", &mut interner);
        let self_fk = make_fk(0x005678, "SeventySix.esm", &mut interner);

        let sig = SigCode::from_str("COBJ").unwrap();
        let record = Record {
            sig,
            form_key: self_fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        };

        let mut omod_set: FxHashSet<FormKey> = FxHashSet::default();
        omod_set.insert(omod_fk);

        assert!(!cobj_references_omod(&record, &omod_set));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn cobj_references_omod_inside_struct() {
        let mut interner = make_interner();
        let omod_fk = make_fk(0x001234, "SeventySix.esm", &mut interner);
        let self_fk = make_fk(0x005678, "SeventySix.esm", &mut interner);

        let field_sym = interner.intern("created_object");
        let sig = SigCode::from_str("COBJ").unwrap();
        let cnam_sig = SubrecordSig::from_str("CNAM").unwrap();

        let record = Record {
            sig,
            form_key: self_fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: cnam_sig,
                value: FieldValue::Struct(vec![(field_sym, FieldValue::FormKey(omod_fk))]),
            }],
            warnings: smallvec::SmallVec::new(),
        };

        let mut omod_set: FxHashSet<FormKey> = FxHashSet::default();
        omod_set.insert(omod_fk);

        assert!(cobj_references_omod(&record, &omod_set));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn cobj_references_omod_empty_omod_set() {
        let mut interner = make_interner();
        let omod_fk = make_fk(0x001234, "SeventySix.esm", &mut interner);
        let self_fk = make_fk(0x005678, "SeventySix.esm", &mut interner);

        let record = make_cobj_with_fk(self_fk, omod_fk, &mut interner);
        let omod_set: FxHashSet<FormKey> = FxHashSet::default();

        assert!(!cobj_references_omod(&record, &omod_set));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_creature_root_is_false() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use crate::sym::StringInterner;
        use std::sync::Arc;

        let mut interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        let root_sig = SigCode::from_str("NPC_").unwrap();
        let config = FixupConfig {
            root_sig: Some(root_sig),
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

        assert!(!InjectCobjsForOmodsFixup.applies_to(&ctx));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_lvln_root_is_false() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use crate::sym::StringInterner;
        use std::sync::Arc;

        let mut interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        let root_sig = SigCode::from_str("LVLN").unwrap();
        let config = FixupConfig {
            root_sig: Some(root_sig),
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

        assert!(!InjectCobjsForOmodsFixup.applies_to(&ctx));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_weap_root_with_source_is_true() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use crate::sym::StringInterner;
        use std::sync::Arc;

        let mut interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        let root_sig = SigCode::from_str("WEAP").unwrap();
        let config = FixupConfig {
            root_sig: Some(root_sig),
            ..Default::default()
        };

        let ctx = FixupContext {
            // Non-zero source handle — fixup should apply.
            source_handle_id: 42,
            target_handle_id: 43,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };

        assert!(InjectCobjsForOmodsFixup.applies_to(&ctx));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn applies_to_no_source_is_false() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use crate::sym::StringInterner;
        use std::sync::Arc;

        let mut interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        let root_sig = SigCode::from_str("WEAP").unwrap();
        let config = FixupConfig {
            root_sig: Some(root_sig),
            ..Default::default()
        };

        let ctx = FixupContext {
            source_handle_id: 0,
            target_handle_id: 43,
            schema_target: &schema,
            schema_source: &schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config: &config,
        };

        assert!(!InjectCobjsForOmodsFixup.applies_to(&ctx));
    }
}
