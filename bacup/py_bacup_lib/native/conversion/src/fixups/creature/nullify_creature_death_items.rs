//! Fixup: drop unresolvable DeathItem (INAM) refs from creature NPC records.
//!
//! Creature death loot converts as an LLD/LLS leveled-item chain. `NPC_.INAM`
//! (4-byte FormID of the LVLI) is kept when its target converted and removed
//! when it is null, missing from the output plugin, or absent from the named
//! master: a dangling INAM is worse than no loot.

use std::collections::HashMap;

use crate::fixups::clean_leveled_item_entries::is_invalid_ref;
use crate::fixups::creature::{
    creature_internal_fixup_applies, npc_internal_fixup_applies_to_record,
};
use crate::fixups::synthesize_workshop_boundaries::target_form_key_from_raw;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};
use crate::sym::{StringInterner, Sym};

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

        let target_masters: Vec<(String, u64)> = session
            .target_masters()
            .iter()
            .cloned()
            .map(|name| name.to_ascii_lowercase())
            .zip(config.target_master_handle_ids.iter().copied())
            .collect();
        let target_master_names = session.target_masters().to_vec();
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let own_plugin = interner.intern(&target_plugin_name);

        let mut warnings = Vec::new();
        let mut dropped_total = 0u32;
        let mut invalid_ref_cache: HashMap<FormKey, bool> = HashMap::new();

        let mut report = session.map_apply_by_sig(
            npc_sig,
            mapper,
            |view, _snapshot, fk| match view.record_decoded(fk, target_schema, interner) {
                Ok(record) => {
                    // Per-record gate (whole-plugin only): only touch the death
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
                    record_has_inam(&record).then_some(DeathItemEdit::Check(record))
                }
                Err(err) => Some(DeathItemEdit::Warn(format!(
                    "nullify_death_item_read:{err}"
                ))),
            },
            |session, mapper, _fk, edit| match edit {
                DeathItemEdit::Check(mut record) => {
                    let target_handle_id = session.target_id();
                    let dropped = apply_to_record(&mut record, |value| {
                        let Some(target) =
                            inam_target(value, &target_master_names, own_plugin, mapper.interner)
                        else {
                            // Undecodable payload: no FormID to validate, so it
                            // cannot resolve at runtime either.
                            return true;
                        };
                        if let Some(cached) = invalid_ref_cache.get(&target) {
                            return *cached;
                        }
                        let invalid = is_invalid_ref(
                            session,
                            &target,
                            mapper.interner,
                            &target_masters,
                            target_handle_id,
                            &target_plugin_name,
                        );
                        invalid_ref_cache.insert(target, invalid);
                        invalid
                    });
                    if dropped == 0 {
                        return Ok(EditOutcome::NoOp);
                    }
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
    Check(Record),
    Warn(String),
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

fn record_has_inam(record: &Record) -> bool {
    let Ok(inam_sig) = SubrecordSig::from_str("INAM") else {
        return false;
    };
    record.fields.iter().any(|entry| entry.sig == inam_sig)
}

/// Resolve the FormKey an `INAM` payload points at, accepting both the decoded
/// (`FormKey`) and raw (`Bytes`) shapes a record can carry at fixup time.
pub fn inam_target(
    value: &FieldValue,
    target_masters: &[String],
    own_plugin: Sym,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            target_form_key_from_raw(raw, target_masters, own_plugin, interner)
        }
        _ => None,
    }
}

/// Strip the INAM subrecords for which `drop_entry` returns true.
///
/// Returns the count of INAM entries removed.
pub fn apply_to_record(
    record: &mut Record,
    mut drop_entry: impl FnMut(&FieldValue) -> bool,
) -> u32 {
    let inam_sig = match SubrecordSig::from_str("INAM") {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let before = record.fields.len();
    let mut kept: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    for entry in record.fields.drain(..) {
        if entry.sig == inam_sig && drop_entry(&entry.value) {
            continue;
        }
        kept.push(entry);
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

    fn drop_all(_: &FieldValue) -> bool {
        true
    }

    fn keep_all(_: &FieldValue) -> bool {
        false
    }

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
        let dropped = apply_to_record(&mut record, drop_all);
        assert_eq!(dropped, 0);
    }

    #[test]
    fn nullify_removes_inam() {
        let mut interner = StringInterner::new();
        let mut record =
            make_npc_with_inam(0x000100, "Output.esp", Some(0x00_ABCDEF), &mut interner);
        let dropped = apply_to_record(&mut record, drop_all);
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
    fn resolvable_inam_is_kept() {
        let mut interner = StringInterner::new();
        let mut record =
            make_npc_with_inam(0x000100, "Output.esp", Some(0x00_ABCDEF), &mut interner);
        let dropped = apply_to_record(&mut record, keep_all);
        assert_eq!(dropped, 0, "a resolvable death item must survive");
        let inam_sig = SubrecordSig::from_str("INAM").unwrap();
        assert!(
            record.fields.iter().any(|e| e.sig == inam_sig),
            "INAM must remain so the creature drops loot"
        );
    }

    #[test]
    fn drops_only_the_unresolvable_inam() {
        let interner = StringInterner::new();
        let sig = SigCode::from_str("NPC_").unwrap();
        let fk = FormKey {
            local: 0x000200,
            plugin: interner.intern("Output.esp"),
        };
        let inam_sig = SubrecordSig::from_str("INAM").unwrap();
        let plugin = interner.intern("Output.esp");
        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        for local in [0x00_AAAAAA_u32, 0x00_BBBBBB_u32] {
            fields.push(FieldEntry {
                sig: inam_sig,
                value: FieldValue::FormKey(FormKey { local, plugin }),
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

        // Only the second target is missing from the output plugin.
        let dropped = apply_to_record(
            &mut record,
            |value| matches!(value, FieldValue::FormKey(fk) if fk.local == 0x00_BBBBBB),
        );

        assert_eq!(dropped, 1, "only the dangling ref should go");
        let remaining: Vec<u32> = record
            .fields
            .iter()
            .filter_map(|e| match &e.value {
                FieldValue::FormKey(fk) if e.sig == inam_sig => Some(fk.local),
                _ => None,
            })
            .collect();
        assert_eq!(remaining, vec![0x00_AAAAAA]);
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
        let dropped = apply_to_record(&mut record, drop_all);
        assert_eq!(dropped, 2);
        assert!(record.fields.is_empty());
    }

    #[test]
    fn inam_target_reads_formkey_and_raw_shapes() {
        let interner = StringInterner::new();
        let masters = vec!["Fallout4.esm".to_string()];
        let own_plugin = interner.intern("SeventySix.esm");

        let decoded = FieldValue::FormKey(FormKey {
            local: 0x00_827A03,
            plugin: own_plugin,
        });
        assert_eq!(
            inam_target(&decoded, &masters, own_plugin, &interner).map(|fk| fk.local),
            Some(0x00_827A03)
        );

        // Load index 0 is the first master.
        let mut master_bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        master_bytes.extend_from_slice(&0x00_04E350_u32.to_le_bytes());
        let raw_master = FieldValue::Bytes(master_bytes);
        let resolved = inam_target(&raw_master, &masters, own_plugin, &interner).expect("resolves");
        assert_eq!(resolved.local, 0x04E350);
        assert_eq!(interner.resolve(resolved.plugin), Some("Fallout4.esm"));

        // Load index == master count is the output plugin itself.
        let mut own_bytes: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        own_bytes.extend_from_slice(&0x01_827A03_u32.to_le_bytes());
        let raw_own = FieldValue::Bytes(own_bytes);
        let resolved = inam_target(&raw_own, &masters, own_plugin, &interner).expect("resolves");
        assert_eq!(resolved.local, 0x827A03);
        assert_eq!(interner.resolve(resolved.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn inam_target_rejects_short_payload() {
        let interner = StringInterner::new();
        let own_plugin = interner.intern("SeventySix.esm");
        let mut short: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        short.extend_from_slice(&[0x01, 0x02]);
        assert!(inam_target(&FieldValue::Bytes(short), &[], own_plugin, &interner).is_none());
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
