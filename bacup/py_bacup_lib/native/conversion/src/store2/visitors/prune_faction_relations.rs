//! Sweep adapter for `prune_faction_relations` (decoded lane).

use std::any::Any;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::prune_faction_relations::{
    encoded_targets_by_source_object_id, prune_xnam_entries_with_rewrite,
    target_formkeys_by_source_object_id,
};
use crate::fixups::{FixupConfig, FixupError};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub(crate) struct PruneFactionIndex {
    graph_faction_object_ids: FxHashSet<u32>,
    encoded_targets: FxHashMap<u32, u32>,
    target_formkeys: FxHashMap<u32, crate::ids::FormKey>,
    target_master_count: usize,
}

pub struct PruneFactionRelationsVisitor;

impl RecordVisitor for PruneFactionRelationsVisitor {
    fn name(&self) -> &'static str {
        "prune_faction_relations"
    }

    fn lane(&self) -> Lane {
        Lane::Decoded
    }

    fn gather(
        &self,
        session: &mut PluginSession,
        mapper: &FormKeyMapper,
        _config: &FixupConfig,
        _master_cache: &mut MasterScanCache,
    ) -> Result<GatherOutput, FixupError> {
        let fact_sig =
            SigCode::from_str("FACT").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let fact_fks = session
            .form_keys_of_sig(fact_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        // Degenerate-guard parity: no FACTs → legacy early-returns.
        if fact_fks.is_empty() {
            return Ok(GatherOutput::sigs_only(Vec::new()));
        }
        let graph_faction_object_ids: FxHashSet<u32> =
            fact_fks.iter().map(|fk| fk.local & 0x00FF_FFFF).collect();
        let encoded_targets = encoded_targets_by_source_object_id(mapper, session.target_masters());
        let target_formkeys = target_formkeys_by_source_object_id(mapper);
        let target_master_count = session.target_masters().len();
        Ok(GatherOutput {
            candidate_sigs: vec![fact_sig],
            index: Some(Box::new(PruneFactionIndex {
                graph_faction_object_ids,
                encoded_targets,
                target_formkeys,
                target_master_count,
            })),
            warnings: Vec::new(),
        })
    }

    fn visit_decoded(
        &self,
        record: &mut Record,
        index: Option<&(dyn Any + Send + Sync)>,
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        let idx = index
            .and_then(|i| i.downcast_ref::<PruneFactionIndex>())
            .expect("faction index");
        let stats = prune_xnam_entries_with_rewrite(
            record,
            record.form_key.local,
            &idx.graph_faction_object_ids,
            &idx.encoded_targets,
            &idx.target_formkeys,
            idx.target_master_count,
        );
        if stats.changed() {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::prune_faction_relations::PruneFactionRelationsFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    fn xnam(raw: u32) -> FieldEntry {
        // 12-byte XNAM: faction formid + modifier + group-combat-reaction.
        let mut b = Vec::with_capacity(12);
        b.extend_from_slice(&raw.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("XNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(b)),
        }
    }

    fn fact(
        local: u32,
        eid: &str,
        xnams: Vec<FieldEntry>,
        interner: &crate::sym::StringInterner,
    ) -> crate::record::Record {
        let eid_sym = interner.intern(eid);
        let mut fields: SmallVec<[FieldEntry; 8]> = smallvec::smallvec![FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(eid_sym),
        }];
        fields.extend(xnams);
        crate::record::Record {
            sig: SigCode::from_str("FACT").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("FactPrune.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        let (h_old, h_new) = seed_twin("FactPrune.esp", |session, schema, interner| {
            // FACT 0x801 relations: self (kept), in-graph 0x802 (kept),
            // unknown 0x3BA686 (dropped).
            let a = fact(
                0x801,
                "FactA",
                vec![xnam(0x0000_0801), xnam(0x0000_0802), xnam(0x003B_A686)],
                interner,
            );
            let b = fact(0x802, "FactB", vec![], interner);
            for r in [a, b] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        run_legacy_fixup(h_old, Box::new(PruneFactionRelationsFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "fact_prune",
            vec![Box::new(PruneFactionRelationsVisitor)],
            &config,
        );

        assert_eq!(reports[0].1.records_changed, 1);
        assert_handles_equal(h_old, h_new);
    }
}
