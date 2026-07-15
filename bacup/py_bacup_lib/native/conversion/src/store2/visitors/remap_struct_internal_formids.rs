//! Sweep adapter for `remap_struct_internal_formids` (decoded lane).

use std::any::Any;

use rustc_hash::FxHashMap;

use crate::fixups::prune_faction_relations::encoded_targets_by_source_object_id;
use crate::fixups::ref_index::remap_struct_fk_fields;
use crate::fixups::remap_struct_internal_formids::{
    FO4_TARGET_FORM_VERSION, candidate_signatures, remap_value_selected_union_formids,
};
use crate::fixups::{FixupConfig, FixupError};
use crate::formkey_mapper::FormKeyMapper;
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub(crate) struct RemapStructIndex {
    encoded_targets: FxHashMap<u32, u32>,
    target_by_source_local: FxHashMap<u32, crate::ids::FormKey>,
}

pub struct RemapStructInternalFormIdsVisitor;

impl RecordVisitor for RemapStructInternalFormIdsVisitor {
    fn name(&self) -> &'static str {
        "remap_struct_internal_formids"
    }

    fn lane(&self) -> Lane {
        Lane::Decoded
    }

    fn gather(
        &self,
        session: &mut PluginSession,
        mapper: &FormKeyMapper,
        config: &FixupConfig,
        _master_cache: &mut MasterScanCache,
    ) -> Result<GatherOutput, FixupError> {
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let target_masters = session.target_masters().to_vec();
        let encoded_targets = encoded_targets_by_source_object_id(mapper, &target_masters);
        let target_by_source_local: FxHashMap<u32, crate::ids::FormKey> = mapper
            .source_to_target_iter()
            .map(|(source, target)| (source.local, target))
            .collect();
        // Degenerate-guard parity: nothing mapped → legacy early-returns.
        let candidate_sigs = if encoded_targets.is_empty() && target_by_source_local.is_empty() {
            Vec::new()
        } else {
            candidate_signatures(session, target_schema)?
        };
        Ok(GatherOutput {
            candidate_sigs,
            index: Some(Box::new(RemapStructIndex {
                encoded_targets,
                target_by_source_local,
            })),
            warnings: Vec::new(),
        })
    }

    fn visit_decoded(
        &self,
        record: &mut Record,
        index: Option<&(dyn Any + Send + Sync)>,
        cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        let idx = index
            .and_then(|i| i.downcast_ref::<RemapStructIndex>())
            .expect("remap-struct index");
        // Post-relayout the target bytes already match the FO4 layout — feed the
        // target schema as the divergence-guard's "source".
        let mut remap = remap_struct_fk_fields(
            record,
            cx.schema,
            Some(cx.schema),
            Some(FO4_TARGET_FORM_VERSION),
            &idx.encoded_targets,
            &idx.target_by_source_local,
        );
        if matches!(record.sig.as_str(), "PACK" | "FACT") {
            remap.remapped += remap_value_selected_union_formids(record, &idx.encoded_targets);
        }
        if remap.changed() {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::remap_struct_internal_formids::RemapStructInternalFormIdsFixup;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    fn ptda(kind: i32, raw: u32) -> FieldEntry {
        let mut b = Vec::with_capacity(12);
        b.extend_from_slice(&kind.to_le_bytes());
        b.extend_from_slice(&raw.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("PTDA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(b)),
        }
    }

    fn pack(
        local: u32,
        eid: &str,
        extra: Vec<FieldEntry>,
        interner: &crate::sym::StringInterner,
    ) -> crate::record::Record {
        let eid_sym = interner.intern(eid);
        let mut fields: SmallVec<[FieldEntry; 8]> = smallvec::smallvec![FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(eid_sym),
        }];
        fields.extend(extra);
        crate::record::Record {
            sig: SigCode::from_str("PACK").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("RemapStruct.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        // PTDA type-0 reference FK 0x00000999 has a seeded source→target
        // mapping → remapped to the encoded output id; type-2 scalar untouched.
        let (h_old, h_new) = seed_twin("RemapStruct.esp", |session, schema, interner| {
            let r = pack(
                0x801,
                "PackA",
                vec![ptda(0, 0x0000_0999), ptda(2, 0x0000_000F)],
                interner,
            );
            session.add_record(r, schema.as_ref(), interner).unwrap();
        });

        let seed_mapper = |mapper: &mut crate::formkey_mapper::FormKeyMapper| {
            mapper.add_mapping(
                FormKey {
                    local: 0x999,
                    plugin: mapper.interner.intern("SeventySix.esm"),
                },
                FormKey {
                    local: 0x802,
                    plugin: mapper.interner.intern("RemapStruct.esp"),
                },
            );
        };

        let config = config_for(h_old);
        run_legacy_fixup_with_mapper(
            h_old,
            Box::new(RemapStructInternalFormIdsFixup),
            &config,
            seed_mapper,
        );
        let reports = run_visitor_sweep_with_mapper(
            h_new,
            "remap_struct",
            vec![Box::new(RemapStructInternalFormIdsVisitor)],
            &config,
            seed_mapper,
        );

        assert_eq!(reports.len(), 1);
        assert_handles_equal(h_old, h_new);
    }
}
