//! Fixup: drop FACT Relations (XNAM) entries that reference factions outside
//! the converted graph.
//!

//!
//! # What this does
//! FO76 faction records carry Relations entries (XNAM subrecords) linking to
//! dozens of unrelated creature, vendor, and quest factions.  After the
//! FO76→FO4 translation sweep, all FormKeys in the target plugin have been
//! remapped.  However, only factions that were actually walked into the
//! dependency graph exist in the target plugin.
//!
//! This fixup collects every FACT FormKey present in the target plugin, then
//! for each FACT record it drops any XNAM subrecord whose referenced faction
//! FormKey is not in that set (and is not the record's own FormKey).
//!
//! # XNAM struct layout (FO4, codec `struct:I,i,I`, 12 bytes)
//! | Offset | Size | Field               |
//! |--------|------|---------------------|
//! |      0 |    4 | faction (formid)    |
//! |      4 |    4 | modifier (int32)    |
//! |      8 |    4 | group_combat_reaction (uint32) |
//!
//! XNAM decodes as `FieldValue::Struct` when the schema carries field metadata
//! (faction field `kind="formid"`); this fixup handles both the typed Struct
//! variant and the Bytes fallback.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use rustc_hash::{FxHashMap, FxHashSet};

// ---------------------------------------------------------------------------
// XNAM struct constants
// ---------------------------------------------------------------------------

/// Size of one XNAM payload in bytes (struct:I,i,I).
const XNAM_SIZE: usize = 12;
/// Byte offset of the faction FormID within XNAM data.
const XNAM_FACTION_OFFSET: usize = 0;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct PruneFactionRelationsFixup;

impl Fixup for PruneFactionRelationsFixup {
    fn name(&self) -> &'static str {
        "prune_faction_relations"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let fact_sig =
            SigCode::from_str("FACT").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();

        // ── 1. Collect all FACT FormKeys in the target plugin ──────────────
        let fact_fks = session
            .form_keys_of_sig(fact_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        if fact_fks.is_empty() {
            return Ok(report);
        }

        let graph_faction_object_ids: FxHashSet<u32> =
            fact_fks.iter().map(|fk| fk.local & 0x00FF_FFFF).collect();
        let encoded_targets = encoded_targets_by_source_object_id(mapper, session.target_masters());
        let target_formkeys = target_formkeys_by_source_object_id(mapper);
        let target_master_count = session.target_masters().len();

        // ── 2. For each FACT record, prune orphaned XNAM entries ───────────
        let mut changed_records = Vec::new();
        for fk in &fact_fks {
            let mut record = match session.record_decoded(fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper.interner.intern(&format!("fact_prune_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            let own_local = fk.local;

            let stats = prune_xnam_entries_with_rewrite(
                &mut record,
                own_local,
                &graph_faction_object_ids,
                &encoded_targets,
                &target_formkeys,
                target_master_count,
            );
            if stats.changed() {
                changed_records.push(record);
                report.records_changed += 1;
                report.records_dropped += stats.dropped;
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "prune_faction_relations replaced {replaced} of {expected} expected records"
            )));
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Record-level mutation (extracted for unit-test access)
// ---------------------------------------------------------------------------

/// Prune XNAM subrecords whose referenced faction FormKey is not in
/// `graph_faction_object_ids`.
///
/// `own_local` identifies the record's own FormKey (a faction always keeps
/// Relations entries pointing to itself).
///
/// Returns the number of XNAM entries that were dropped.
///
/// XNAM is emitted as `FieldValue::Bytes` (12-byte struct).  Unknown or
/// shorter XNAM payloads are kept as-is (no mutation).
pub fn prune_xnam_entries(
    record: &mut Record,
    own_local: u32,
    graph_faction_object_ids: &FxHashSet<u32>,
) -> u32 {
    let encoded_targets = FxHashMap::default();
    let target_formkeys = FxHashMap::default();
    prune_xnam_entries_with_rewrite(
        record,
        own_local,
        graph_faction_object_ids,
        &encoded_targets,
        &target_formkeys,
        0,
    )
    .dropped
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct XnamPruneStats {
    pub(crate) dropped: u32,
    pub(crate) rewritten: u32,
}

impl XnamPruneStats {
    pub(crate) fn changed(self) -> bool {
        self.dropped > 0 || self.rewritten > 0
    }
}

pub(crate) fn encoded_targets_by_source_object_id(
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> FxHashMap<u32, u32> {
    let mut out = FxHashMap::default();
    for (source, target) in mapper.source_to_target_iter() {
        if let Some(encoded) = encode_target_form_id(target, mapper.interner, target_masters) {
            out.insert(source.local & 0x00FF_FFFF, encoded);
        }
    }
    out
}

pub(crate) fn target_formkeys_by_source_object_id(
    mapper: &FormKeyMapper,
) -> FxHashMap<u32, FormKey> {
    let mut out = FxHashMap::default();
    for (source, target) in mapper.source_to_target_iter() {
        out.insert(source.local & 0x00FF_FFFF, target);
    }
    out
}

fn encode_target_form_id(
    target: crate::ids::FormKey,
    interner: &crate::sym::StringInterner,
    target_masters: &[String],
) -> Option<u32> {
    let object_id = target.local & 0x00FF_FFFF;
    if object_id == 0 {
        return Some(0);
    }
    let plugin_name = interner.resolve(target.plugin)?;
    let load_index = target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin_name))
        .unwrap_or(target_masters.len());
    if load_index > u8::MAX as usize {
        return None;
    }
    Some(((load_index as u32) << 24) | object_id)
}

pub(crate) fn prune_xnam_entries_with_rewrite(
    record: &mut Record,
    own_local: u32,
    graph_faction_object_ids: &FxHashSet<u32>,
    encoded_targets: &FxHashMap<u32, u32>,
    target_formkeys: &FxHashMap<u32, FormKey>,
    target_master_count: usize,
) -> XnamPruneStats {
    let xnam_sig = match SubrecordSig::from_str("XNAM") {
        Ok(sig) => sig,
        Err(_) => return XnamPruneStats::default(),
    };

    // XNAM bytes hold raw FormIDs. Source FO76 self-plugin references arrive
    // with master byte 0 because SeventySix.esm has no masters. In an FO4
    // output plugin, that same byte means Fallout4.esm, so kept source-local
    // relations must be re-encoded to the target plugin's own index.

    let mut kept: smallvec::SmallVec<[crate::record::FieldEntry; 8]> = smallvec::SmallVec::new();
    let mut stats = XnamPruneStats::default();

    for mut entry in record.fields.drain(..) {
        if entry.sig != xnam_sig {
            kept.push(entry);
            continue;
        }

        // Parse the XNAM payload — handles both legacy Bytes and typed Struct.
        let should_keep = match &entry.value {
            FieldValue::Bytes(data) if data.len() >= XNAM_SIZE => {
                let raw_faction_id = u32::from_le_bytes([
                    data[XNAM_FACTION_OFFSET],
                    data[XNAM_FACTION_OFFSET + 1],
                    data[XNAM_FACTION_OFFSET + 2],
                    data[XNAM_FACTION_OFFSET + 3],
                ]);

                // Null reference — keep it (null means no faction).
                if raw_faction_id == 0 {
                    true
                } else {
                    let raw_object_id = raw_faction_id & 0x00FF_FFFF;
                    if encoded_targets.contains_key(&raw_object_id) {
                        true
                    } else {
                        let resolved_faction_id = encoded_xnam_target(
                            raw_faction_id,
                            own_local,
                            graph_faction_object_ids,
                            encoded_targets,
                            target_master_count,
                        );
                        let object_id = resolved_faction_id & 0x00FF_FFFF;

                        // Keep if the object_id matches own FormKey's local id.
                        if object_id == (own_local & 0x00FF_FFFF) {
                            true
                        } else {
                            // Keep if the object_id is in any graph faction FK.
                            graph_faction_object_ids.contains(&object_id)
                        }
                    }
                }
            }
            // Typed struct decode: first field is the faction FK.
            FieldValue::Struct(fields) => {
                match fields.first().map(|(_, v)| v) {
                    Some(FieldValue::FormKey(fk)) => {
                        let object_id = fk.local & 0x00FF_FFFF;
                        object_id == (own_local & 0x00FF_FFFF)
                            || graph_faction_object_ids.contains(&object_id)
                            || target_formkeys.contains_key(&object_id)
                    }
                    Some(FieldValue::None) => true, // null reference
                    _ => true,                      // unexpected layout — keep
                }
            }
            // Malformed or non-bytes XNAM — keep unchanged.
            _ => true,
        };

        if should_keep {
            if let FieldValue::Bytes(data) = &mut entry.value {
                if data.len() >= XNAM_SIZE {
                    let raw_faction_id = u32::from_le_bytes([
                        data[XNAM_FACTION_OFFSET],
                        data[XNAM_FACTION_OFFSET + 1],
                        data[XNAM_FACTION_OFFSET + 2],
                        data[XNAM_FACTION_OFFSET + 3],
                    ]);
                    let encoded = encoded_xnam_target(
                        raw_faction_id,
                        own_local,
                        graph_faction_object_ids,
                        encoded_targets,
                        target_master_count,
                    );
                    if encoded != raw_faction_id {
                        data[XNAM_FACTION_OFFSET..XNAM_FACTION_OFFSET + 4]
                            .copy_from_slice(&encoded.to_le_bytes());
                        stats.rewritten += 1;
                    }
                }
            }
            if let FieldValue::Struct(fields) = &mut entry.value {
                if let Some(FieldValue::FormKey(fk)) = fields.first_mut().map(|(_, v)| v) {
                    let object_id = fk.local & 0x00FF_FFFF;
                    if let Some(target_fk) = target_formkeys.get(&object_id) {
                        if *fk != *target_fk {
                            *fk = *target_fk;
                            stats.rewritten += 1;
                        }
                    }
                }
            }
            kept.push(entry);
        } else {
            stats.dropped += 1;
        }
    }

    record.fields = kept;
    stats
}

fn encoded_xnam_target(
    raw_form_id: u32,
    own_local: u32,
    graph_faction_object_ids: &FxHashSet<u32>,
    encoded_targets: &FxHashMap<u32, u32>,
    target_master_count: usize,
) -> u32 {
    if raw_form_id == 0 {
        return 0;
    }

    let object_id = raw_form_id & 0x00FF_FFFF;
    if let Some(encoded) = encoded_targets.get(&object_id) {
        return *encoded;
    }

    if raw_form_id >> 24 == 0
        && target_master_count <= u8::MAX as usize
        && (object_id == (own_local & 0x00FF_FFFF) || graph_faction_object_ids.contains(&object_id))
    {
        return ((target_master_count as u32) << 24) | object_id;
    }

    raw_form_id
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

    /// Build a raw XNAM payload (12 bytes) with the given raw FormID.
    fn make_xnam_bytes(raw_faction_id: u32) -> smallvec::SmallVec<[u8; 32]> {
        let mut sv: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        sv.extend_from_slice(&raw_faction_id.to_le_bytes()); // faction (4)
        sv.extend_from_slice(&0i32.to_le_bytes()); // modifier (4)
        sv.extend_from_slice(&1u32.to_le_bytes()); // group_combat_reaction (4)
        sv
    }

    fn make_fact_record(
        local: u32,
        plugin: &str,
        xnam_raw_ids: &[u32],
        interner: &StringInterner,
    ) -> Record {
        let sig = SigCode::from_str("FACT").unwrap();
        let fk = FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let edid_sym = interner.intern("TestFaction");

        let mut fields: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(edid_sym),
        });
        for &raw_id in xnam_raw_ids {
            fields.push(FieldEntry {
                sig: xnam_sig,
                value: FieldValue::Bytes(make_xnam_bytes(raw_id)),
            });
        }

        Record {
            sig,
            form_key: fk,
            eid: Some(edid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn xnam_raw_ids(record: &Record) -> Vec<u32> {
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        record
            .fields
            .iter()
            .filter_map(|entry| {
                if entry.sig != xnam_sig {
                    return None;
                }
                let FieldValue::Bytes(data) = &entry.value else {
                    return None;
                };
                if data.len() < XNAM_SIZE {
                    return None;
                }
                Some(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
            })
            .collect()
    }

    fn typed_xnam_target(record: &Record) -> Option<FormKey> {
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        record.fields.iter().find_map(|entry| {
            if entry.sig != xnam_sig {
                return None;
            }
            let FieldValue::Struct(fields) = &entry.value else {
                return None;
            };
            match fields.first().map(|(_, value)| value) {
                Some(FieldValue::FormKey(fk)) => Some(*fk),
                _ => None,
            }
        })
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_no_entries_is_no_op() {
        let mut interner = StringInterner::new();
        let mut record = make_fact_record(0x000800, "Output.esp", &[], &mut interner);
        let graph: rustc_hash::FxHashSet<u32> = [0x000800].into_iter().collect();

        let dropped = prune_xnam_entries(&mut record, 0x000800, &graph);
        assert_eq!(dropped, 0);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_keeps_graph_factions() {
        let mut interner = StringInterner::new();
        // Two in-graph factions with raw ids 0x000800 and 0x000801 (master byte 0x00).
        let xnam_ids = &[0x00_000800u32, 0x00_000801u32];
        let mut record = make_fact_record(0x000800, "Output.esp", xnam_ids, &mut interner);

        let graph: rustc_hash::FxHashSet<u32> = [0x000800, 0x000801].into_iter().collect();

        let dropped = prune_xnam_entries(&mut record, 0x000800, &graph);
        assert_eq!(dropped, 0, "all in-graph factions must be kept");

        // EDID + 2 XNAM.
        assert_eq!(record.fields.len(), 3);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_drops_unknown_faction() {
        let mut interner = StringInterner::new();
        // Faction 0x000800 is in graph; 0x003BA686 is not (FO76-only).
        let xnam_ids = &[0x00_000800u32, 0x00_3BA686u32];
        let mut record = make_fact_record(0x000800, "Output.esp", xnam_ids, &mut interner);

        let graph: rustc_hash::FxHashSet<u32> = [0x000800].into_iter().collect();

        let dropped = prune_xnam_entries(&mut record, 0x000800, &graph);
        assert_eq!(dropped, 1, "unknown faction XNAM must be dropped");

        // EDID + 1 surviving XNAM.
        assert_eq!(record.fields.len(), 2);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_keeps_self_reference() {
        let mut interner = StringInterner::new();
        // Only XNAM references the record's own FormKey (self-relation).
        // Graph does not contain anything.
        let xnam_ids = &[0x00_000800u32];
        let mut record = make_fact_record(0x000800, "Output.esp", xnam_ids, &mut interner);

        let graph: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();

        let dropped = prune_xnam_entries(&mut record, 0x000800, &graph);
        assert_eq!(dropped, 0, "self-reference XNAM must never be pruned");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_keeps_null_faction_ref() {
        let mut interner = StringInterner::new();
        // Zero formid = null reference.
        let xnam_ids = &[0x00_000000u32];
        let mut record = make_fact_record(0x000800, "Output.esp", xnam_ids, &mut interner);

        let graph: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();

        let dropped = prune_xnam_entries(&mut record, 0x000800, &graph);
        assert_eq!(dropped, 0, "null XNAM must not be pruned");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_keeps_short_payload() {
        let interner = StringInterner::new();
        let sig = SigCode::from_str("FACT").unwrap();
        let fk = FormKey {
            local: 0x000800,
            plugin: interner.intern("Output.esp"),
        };
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();

        let mut short: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        short.extend_from_slice(&[0xAAu8, 0xBB, 0xCC]); // only 3 bytes

        let mut record = Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: xnam_sig,
                value: FieldValue::Bytes(short),
            }],
            warnings: smallvec::SmallVec::new(),
        };

        let graph: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();
        let dropped = prune_xnam_entries(&mut record, 0x000800, &graph);
        assert_eq!(dropped, 0, "short XNAM must not be dropped");
        assert_eq!(record.fields.len(), 1);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_preserves_non_xnam_fields() {
        let interner = StringInterner::new();
        // One XNAM (unknown faction, will be dropped) + one FULL field (kept).
        let sig = SigCode::from_str("FACT").unwrap();
        let fk = FormKey {
            local: 0x000800,
            plugin: interner.intern("Output.esp"),
        };
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        let full_sig = SubrecordSig::from_str("FULL").unwrap();

        let full_sym = interner.intern("TestFactionName");

        let mut record = Record {
            sig,
            form_key: fk,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: full_sig,
                    value: FieldValue::String(full_sym),
                },
                FieldEntry {
                    sig: xnam_sig,
                    value: FieldValue::Bytes(make_xnam_bytes(0x00_BEEF01)),
                },
            ],
            warnings: smallvec::SmallVec::new(),
        };

        let graph: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();
        let dropped = prune_xnam_entries(&mut record, 0x000800, &graph);
        assert_eq!(dropped, 1, "unknown-faction XNAM must be pruned");

        // Only FULL remains.
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig.as_str(), "FULL");
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_rewrites_kept_source_local_relation() {
        let mut interner = StringInterner::new();
        let xnam_ids = &[0x00_342ACEu32];
        let mut record = make_fact_record(0x868BAF, "Output.esp", xnam_ids, &mut interner);

        let graph: FxHashSet<u32> = [0x342ACE].into_iter().collect();
        let mut encoded_targets = FxHashMap::default();
        encoded_targets.insert(0x342ACE, 0x07_342ACE);
        let target_formkeys = FxHashMap::default();

        let stats = prune_xnam_entries_with_rewrite(
            &mut record,
            0x868BAF,
            &graph,
            &encoded_targets,
            &target_formkeys,
            7,
        );

        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.rewritten, 1);
        assert_eq!(xnam_raw_ids(&record), vec![0x07_342ACE]);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_keeps_and_rewrites_master_mapped_source_relation() {
        let mut interner = StringInterner::new();
        let xnam_ids = &[0x00_01C21Cu32];
        let mut record = make_fact_record(0x868BAF, "Output.esp", xnam_ids, &mut interner);

        let graph: FxHashSet<u32> = FxHashSet::default();
        let mut encoded_targets = FxHashMap::default();
        encoded_targets.insert(0x01C21C, 0x02_01C21C);
        let target_formkeys = FxHashMap::default();

        let stats = prune_xnam_entries_with_rewrite(
            &mut record,
            0x868BAF,
            &graph,
            &encoded_targets,
            &target_formkeys,
            7,
        );

        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.rewritten, 1);
        assert_eq!(xnam_raw_ids(&record), vec![0x02_01C21C]);
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_rewrites_typed_master_mapped_relation() {
        let interner = StringInterner::new();
        let output_plugin = interner.intern("Output.esp");
        let source_plugin = interner.intern("SeventySix.esm");
        let target_plugin = interner.intern("Fallout4.esm");
        let faction_sym = interner.intern("Faction");
        let modifier_sym = interner.intern("Modifier");
        let reaction_sym = interner.intern("GroupCombatReaction");
        let xnam_sig = SubrecordSig::from_str("XNAM").unwrap();
        let target_fk = FormKey {
            local: 0x01C21C,
            plugin: target_plugin,
        };
        let source_fk = FormKey {
            local: 0x01C21C,
            plugin: source_plugin,
        };
        let mut record = Record {
            sig: SigCode::from_str("FACT").unwrap(),
            form_key: FormKey {
                local: 0x868BAF,
                plugin: output_plugin,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: xnam_sig,
                value: FieldValue::Struct(vec![
                    (faction_sym, FieldValue::FormKey(source_fk)),
                    (modifier_sym, FieldValue::Int(0)),
                    (reaction_sym, FieldValue::Uint(1)),
                ]),
            }],
            warnings: smallvec::SmallVec::new(),
        };

        let graph: FxHashSet<u32> = FxHashSet::default();
        let encoded_targets = FxHashMap::default();
        let mut target_formkeys = FxHashMap::default();
        target_formkeys.insert(0x01C21C, target_fk);

        let stats = prune_xnam_entries_with_rewrite(
            &mut record,
            0x868BAF,
            &graph,
            &encoded_targets,
            &target_formkeys,
            7,
        );

        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.rewritten, 1);
        assert_eq!(typed_xnam_target(&record), Some(target_fk));
    }

    // -----------------------------------------------------------------------
    // -----------------------------------------------------------------------

    #[test]
    fn prune_xnam_rewrites_self_relation_to_target_own_index() {
        let mut interner = StringInterner::new();
        let xnam_ids = &[0x00_868BAFu32];
        let mut record = make_fact_record(0x868BAF, "Output.esp", xnam_ids, &mut interner);

        let graph: FxHashSet<u32> = FxHashSet::default();
        let encoded_targets = FxHashMap::default();
        let target_formkeys = FxHashMap::default();

        let stats = prune_xnam_entries_with_rewrite(
            &mut record,
            0x868BAF,
            &graph,
            &encoded_targets,
            &target_formkeys,
            7,
        );

        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.rewritten, 1);
        assert_eq!(xnam_raw_ids(&record), vec![0x07_868BAF]);
    }
}
