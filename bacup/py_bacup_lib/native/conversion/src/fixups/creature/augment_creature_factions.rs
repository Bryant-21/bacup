//! Fixup: add WildlifeFaction to creature NPC records that lack it.
//!

//!
//! # What this does
//! FO4 creatures need `WildlifeFaction` (FK `022B31:Fallout4.esm`) for the AI
//! to treat them as hostile wildlife.  FO76 omits this faction — its aggression
//! uses different systems.  Without it, converted creatures may not aggro correctly.
//!
//! This fixup scans every NPC_ record in the target plugin.  For each one that
//! does not already have an SNAM entry whose faction FormID matches WildlifeFaction,
//! it appends a new SNAM entry (rank 0).
//!
//! # SNAM struct layout (FO4, codec `struct:I,b`, 5 bytes)
//! | Offset | Size | Field   |
//! |--------|------|---------|
//! |      0 |    4 | faction (FormID, little-endian) |
//! |      4 |    1 | rank (int8) |
//!
//! # WildlifeFaction
//! FK `022B31:Fallout4.esm` — raw FormID `0x00_022B31` (Fallout4.esm is master
//! index 0 in every FO4 plugin).

use crate::fixups::creature::{
    creature_internal_fixup_applies, npc_internal_fixup_applies_to_record,
};
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Raw FormID for WildlifeFaction (022B31:Fallout4.esm).
/// Master byte 0x00 = Fallout4.esm (master index 0 in every FO4 plugin).
const WILDLIFE_FACTION_FORM_ID: u32 = 0x00_022B31;

/// SNAM payload size: 4-byte FormID + 1-byte rank.
const SNAM_SIZE: usize = 5;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct AugmentCreatureFactionsFixup;

impl Fixup for AugmentCreatureFactionsFixup {
    fn name(&self) -> &'static str {
        "augment_creature_factions"
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

        let mut report = session.map_apply_by_sig(
            npc_sig,
            mapper,
            |view, _snapshot, fk| match view.record_decoded(fk, target_schema, interner) {
                Ok(mut record) => {
                    // Per-record gate (whole-plugin only): WildlifeFaction only
                    // belongs on creatures. No-op gate on a creature-graph walk.
                    if !npc_internal_fixup_applies_to_record(
                        &record,
                        view,
                        target_schema,
                        interner,
                        config,
                    ) {
                        return None;
                    }
                    apply_to_record(&mut record, interner).then_some(FactionEdit::Replace(record))
                }
                Err(err) => Some(FactionEdit::Warn(format!("augment_factions_read:{err}"))),
            },
            |session, mapper, _fk, edit| match edit {
                FactionEdit::Replace(record) => {
                    session
                        .replace_record(record, target_schema, mapper.interner)
                        .map_err(|e| FixupError::HandleError(e.to_string()))?;
                    Ok(EditOutcome::Changed)
                }
                FactionEdit::Warn(message) => {
                    warnings.push(mapper.interner.intern(&message));
                    Ok(EditOutcome::NoOp)
                }
            },
        )?;
        report.warnings.extend(warnings);
        Ok(report)
    }
}

enum FactionEdit {
    Replace(Record),
    Warn(String),
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Append a WildlifeFaction SNAM entry if none already exists.
///
/// Returns `true` if a new SNAM was added.
pub fn apply_to_record(record: &mut Record, interner: &crate::sym::StringInterner) -> bool {
    let snam_sig = match SubrecordSig::from_str("SNAM") {
        Ok(s) => s,
        Err(_) => return false,
    };

    // Check if WildlifeFaction is already present.
    for entry in &record.fields {
        if entry.sig != snam_sig {
            continue;
        }
        if snam_has_wildlife_faction(entry) {
            return false;
        }
    }

    // Append a new SNAM: FormID(4LE) + rank(1) = 5 bytes.
    let mut payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
    payload.extend_from_slice(&WILDLIFE_FACTION_FORM_ID.to_le_bytes()); // faction
    payload.push(0u8); // rank = 0

    // Intern "WildlifeFaction" for FieldValue::String if needed, but we use Bytes
    // so the downstream encoder writes the raw struct correctly.
    let _ = interner; // not needed for Bytes path
    record.fields.push(FieldEntry {
        sig: snam_sig,
        value: FieldValue::Bytes(payload),
    });

    true
}

/// Returns `true` if the SNAM entry's faction FormID matches WildlifeFaction.
///
/// Handles both `FieldValue::Bytes` (raw struct) and `FieldValue::Struct`
/// (decoded by schema).
fn snam_has_wildlife_faction(entry: &FieldEntry) -> bool {
    match &entry.value {
        FieldValue::Bytes(data) if data.len() >= SNAM_SIZE => {
            let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            // Match on object_id (lower 24 bits) to be master-index-agnostic.
            (raw & 0x00FF_FFFF) == (WILDLIFE_FACTION_FORM_ID & 0x00FF_FFFF)
        }
        FieldValue::Struct(fields) => {
            // Decoded as {"faction": FormKey, "rank": ...}
            fields.iter().any(|(_, v)| {
                if let FieldValue::FormKey(fk) = v {
                    (fk.local & 0x00FF_FFFF) == (WILDLIFE_FACTION_FORM_ID & 0x00FF_FFFF)
                } else {
                    false
                }
            })
        }
        _ => false,
    }
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

    fn make_npc(
        local: u32,
        plugin: &str,
        snam_form_ids: &[u32],
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("NPC_").unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        let snam_sig = SubrecordSig::from_str("SNAM").unwrap();

        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        for &raw_id in snam_form_ids {
            let mut payload: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
            payload.extend_from_slice(&raw_id.to_le_bytes());
            payload.push(0u8); // rank
            fields.push(FieldEntry {
                sig: snam_sig,
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

    fn make_creature_config() -> (StringInterner, Arc<AuthoringSchema>, FixupConfig) {
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            ..Default::default()
        };
        (interner, schema, config)
    }

    #[test]
    fn augment_adds_wildlife_faction_when_absent() {
        let mut interner = StringInterner::new();
        let mut record = make_npc(0x000100, "Output.esp", &[], &mut interner);
        let changed = apply_to_record(&mut record, &mut interner);
        assert!(changed, "must add WildlifeFaction when no factions present");
        // One new SNAM.
        let snam_sig = SubrecordSig::from_str("SNAM").unwrap();
        let snam_count = record.fields.iter().filter(|e| e.sig == snam_sig).count();
        assert_eq!(snam_count, 1);
        // Payload encodes WILDLIFE_FACTION_FORM_ID.
        if let FieldValue::Bytes(ref data) = record.fields[0].value {
            let raw = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            assert_eq!(raw, WILDLIFE_FACTION_FORM_ID);
        }
    }

    #[test]
    fn augment_skips_when_wildlife_faction_already_present() {
        let mut interner = StringInterner::new();
        let mut record = make_npc(
            0x000100,
            "Output.esp",
            &[WILDLIFE_FACTION_FORM_ID],
            &mut interner,
        );
        let changed = apply_to_record(&mut record, &mut interner);
        assert!(!changed, "must not add duplicate WildlifeFaction");
        let snam_sig = SubrecordSig::from_str("SNAM").unwrap();
        assert_eq!(
            record.fields.iter().filter(|e| e.sig == snam_sig).count(),
            1
        );
    }

    #[test]
    fn augment_adds_when_other_factions_exist() {
        let mut interner = StringInterner::new();
        // Some other faction (not WildlifeFaction).
        let mut record = make_npc(0x000100, "Output.esp", &[0x00_001234], &mut interner);
        let changed = apply_to_record(&mut record, &mut interner);
        assert!(changed, "must add WildlifeFaction alongside other factions");
        let snam_sig = SubrecordSig::from_str("SNAM").unwrap();
        assert_eq!(
            record.fields.iter().filter(|e| e.sig == snam_sig).count(),
            2
        );
    }

    #[test]
    fn applies_to_false_for_weap_root() {
        let (mut mapper_interner, schema, _) = make_creature_config();
        let mut ctx_interner = StringInterner::new();
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
        assert!(!AugmentCreatureFactionsFixup.applies_to(&ctx));
        let _ = mapper; // suppress unused warning
    }

    #[test]
    fn applies_to_true_for_lvln_root() {
        let (mut mapper_interner, schema, _) = make_creature_config();
        let mut ctx_interner = StringInterner::new();
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("LVLN").unwrap()),
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
        assert!(AugmentCreatureFactionsFixup.applies_to(&ctx));
        let _ = mapper;
    }
}
