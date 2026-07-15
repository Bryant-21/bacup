//! Fixup: remove DeathItem (INAM) from creature NPC records.
//!

//!
//! # What this does
//! FO76 creature death loot (LLD/LLS leveled item chains) cascades into hundreds
//! of FO76-specific records with no FO4 equivalents.  Rather than walking the
//! entire loot chain, this fixup removes the `DeathItem` reference from every
//! creature NPC_ record so the creature drops nothing on death.
//!
//! # INAM subrecord
//! The FO4 `NPC_.INAM` subrecord is a 4-byte FormID pointing at the LVLI
//! death-item record.  This fixup drops all INAM subrecords it finds (there
//! should be at most one per record; we drop all to be safe).

use crate::fixups::creature::{
    creature_internal_fixup_applies, npc_internal_fixup_applies_to_record,
};
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, Record};
use crate::session::{EditOutcome, PluginSession};

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct NullifyCreatureDeathItemsFixup;

impl Fixup for NullifyCreatureDeathItemsFixup {
    fn name(&self) -> &'static str {
        "nullify_creature_death_items"
    }

    fn scope(&self) -> FixupScope {
        // Whole-plugin: self-gates per NPC on the creature predicate below.
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
        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let interner = mapper.interner;
        let mut warnings = Vec::new();
        let mut dropped_total = 0u32;

        let mut report = session.map_apply_by_sig(
            npc_sig,
            mapper,
            |view, _snapshot, fk| match view.record_decoded(fk, target_schema, interner) {
                Ok(mut record) => {
                    // Per-record gate (whole-plugin only): only strip the death
                    // item on creatures. Stripping INAM off a human NPC would
                    // silently delete its intended death loot. No-op gate on a
                    // creature-graph walk.
                    if !npc_internal_fixup_applies_to_record(
                        &record,
                        view,
                        target_schema,
                        interner,
                        config,
                    ) {
                        return None;
                    }
                    let dropped = apply_to_record(&mut record);
                    (dropped > 0).then_some(DeathItemEdit::Replace { record, dropped })
                }
                Err(err) => Some(DeathItemEdit::Warn(format!(
                    "nullify_death_item_read:{err}"
                ))),
            },
            |session, mapper, _fk, edit| match edit {
                DeathItemEdit::Replace { record, dropped } => {
                    session
                        .replace_record(record, target_schema, mapper.interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    dropped_total += dropped;
                    Ok(EditOutcome::Changed)
                }
                DeathItemEdit::Warn(message) => {
                    warnings.push(mapper.interner.intern(&message));
                    Ok(EditOutcome::NoOp)
                }
            },
        )?;
        report.records_dropped += dropped_total;
        report.warnings.extend(warnings);
        Ok(report)
    }
}

enum DeathItemEdit {
    Replace { record: Record, dropped: u32 },
    Warn(String),
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Strip all INAM subrecords from `record`.
///
/// Returns the count of INAM entries removed.
pub fn apply_to_record(record: &mut Record) -> u32 {
    let inam_sig = match SubrecordSig::from_str("INAM") {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let before = record.fields.len();
    let mut kept: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    for entry in record.fields.drain(..) {
        if entry.sig != inam_sig {
            kept.push(entry);
        }
    }
    record.fields = kept;

    (before - record.fields.len()) as u32
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::{FixupConfig, FixupContext};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use std::sync::Arc;

    fn make_npc_with_inam(
        local: u32,
        plugin: &str,
        inam_raw_id: Option<u32>,
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("NPC_").unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();

        // Always add an EDID so the record is well-formed.
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("TestNPC");
        fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(edid_sym),
        });

        if let Some(raw_id) = inam_raw_id {
            let inam_sig = SubrecordSig::from_str("INAM").unwrap();
            let mut payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
            payload.extend_from_slice(&raw_id.to_le_bytes());
            fields.push(FieldEntry {
                sig: inam_sig,
                value: FieldValue::Bytes(payload),
            });
        }

        Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    #[test]
    fn nullify_no_inam_is_no_op() {
        let mut interner = StringInterner::new();
        let mut record = make_npc_with_inam(0x000100, "Output.esp", None, &mut interner);
        let dropped = apply_to_record(&mut record);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn nullify_removes_inam() {
        let mut interner = StringInterner::new();
        let mut record =
            make_npc_with_inam(0x000100, "Output.esp", Some(0x00_ABCDEF), &mut interner);
        let dropped = apply_to_record(&mut record);
        assert_eq!(dropped, 1, "must drop one INAM");
        let inam_sig = SubrecordSig::from_str("INAM").unwrap();
        assert!(
            record.fields.iter().all(|e| e.sig != inam_sig),
            "no INAM should remain"
        );
        // Other fields (EDID) must be preserved.
        assert!(!record.fields.is_empty(), "EDID must survive");
    }

    #[test]
    fn nullify_removes_multiple_inam() {
        let mut interner = StringInterner::new();
        let sig = SigCode::from_str("NPC_").unwrap();
        let fk = FormKey {
            local: 0x000200,
            plugin: interner.intern("Output.esp"),
        };
        let inam_sig = SubrecordSig::from_str("INAM").unwrap();
        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        for raw_id in [0x00_AAAAAA_u32, 0x00_BBBBBB_u32] {
            let mut payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
            payload.extend_from_slice(&raw_id.to_le_bytes());
            fields.push(FieldEntry {
                sig: inam_sig,
                value: FieldValue::Bytes(payload),
            });
        }
        let mut record = Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        };
        let dropped = apply_to_record(&mut record);
        assert_eq!(dropped, 2);
        assert!(record.fields.is_empty());
    }

    #[test]
    fn applies_to_false_for_weap_root() {
        let interner = StringInterner::new();
        let schema = Arc::new(AuthoringSchema::for_game("fo4").unwrap());
        let mut ctx_interner = StringInterner::new();
        let mut mapper_interner = StringInterner::new();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
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
        assert!(!NullifyCreatureDeathItemsFixup.applies_to(&ctx));
        let _ = (interner, mapper);
    }
}
