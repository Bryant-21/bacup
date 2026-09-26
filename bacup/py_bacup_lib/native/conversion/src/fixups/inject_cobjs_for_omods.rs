//! Fixup: inject ConstructibleObject records that reference OMODs in the target.
//!
//! Translated OMODs arrive without their COBJ workbench recipes, which leaves their
//! mod categories invisible at the workbench. Each source COBJ that references a
//! translated OMOD (matched by reversing `mapper.source_to_target`) and is not yet
//! in the target is rewritten through the mapper and added. Creature roots
//! (NPC_/LVLN) are skipped: they never carry OMOD/COBJ chains.

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

    fn make_cobj_with_fk(self_fk: FormKey, referenced_fk: FormKey) -> Record {
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

    #[test]
    fn cobj_references_omod_in_direct_and_struct_fields() {
        let interner = make_interner();
        let omod_fk = make_fk(0x001234, "SeventySix.esm", &interner);
        let other_fk = make_fk(0x009999, "SeventySix.esm", &interner);
        let self_fk = make_fk(0x005678, "SeventySix.esm", &interner);
        let mut empty = make_cobj_with_fk(self_fk, omod_fk);
        empty.fields.clear();
        let mut in_struct = make_cobj_with_fk(self_fk, omod_fk);
        in_struct.fields[0].value = FieldValue::Struct(vec![(
            interner.intern("created_object"),
            FieldValue::FormKey(omod_fk),
        )]);
        let omods = FxHashSet::from_iter([omod_fk]);
        let no_omods = FxHashSet::default();

        for (name, record, omod_set, expected) in [
            (
                "direct match",
                make_cobj_with_fk(self_fk, omod_fk),
                &omods,
                true,
            ),
            (
                "other form",
                make_cobj_with_fk(self_fk, other_fk),
                &omods,
                false,
            ),
            ("no fields", empty, &omods, false),
            ("inside struct", in_struct, &omods, true),
            (
                "empty omod set",
                make_cobj_with_fk(self_fk, omod_fk),
                &no_omods,
                false,
            ),
        ] {
            assert_eq!(cobj_references_omod(&record, omod_set), expected, "{name}");
        }
    }

    #[test]
    fn applies_only_to_non_creature_roots_with_a_source() {
        use crate::fixups::{FixupConfig, FixupContext};
        use crate::schema::AuthoringSchema;
        use std::sync::Arc;

        let schema = Arc::new(AuthoringSchema::for_game("fo4").expect("fo4 schema"));
        for (name, root, source_handle_id, expected) in [
            ("creature root", "NPC_", 1, false),
            ("leveled npc root", "LVLN", 1, false),
            ("weapon root with source", "WEAP", 42, true),
            ("no source", "WEAP", 0, false),
        ] {
            let config = FixupConfig {
                root_sig: Some(SigCode::from_str(root).unwrap()),
                ..Default::default()
            };
            let ctx = FixupContext {
                source_handle_id,
                target_handle_id: 43,
                schema_target: &schema,
                schema_source: &schema,
                skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
                mod_path: None,
                source_extracted_dir: None,
                target_master_handle_ids: &[],
                config: &config,
            };
            assert_eq!(
                InjectCobjsForOmodsFixup.applies_to(&ctx),
                expected,
                "{name}"
            );
        }
    }
}
