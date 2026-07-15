//! Sweep adapter for `null_dangling_vmad_refs` (decoded lane; sweep-C [1/3]).

use std::any::Any;

use crate::fixups::null_dangling_vmad_refs::{
    TOUCHED_RECORD_SIGS, VmadResolver, null_dangling_in_record,
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

pub struct NullDanglingVmadRefsVisitor;

impl RecordVisitor for NullDanglingVmadRefsVisitor {
    fn name(&self) -> &'static str {
        "null_dangling_vmad_refs"
    }

    fn lane(&self) -> Lane {
        Lane::Decoded
    }

    fn gather(
        &self,
        session: &mut PluginSession,
        _mapper: &FormKeyMapper,
        config: &FixupConfig,
        master_cache: &mut MasterScanCache,
    ) -> Result<GatherOutput, FixupError> {
        let master_objids =
            master_cache.master_objid_sets(session, &config.target_master_handle_ids)?;
        // Pre-copy sweep: on whole-plugin FO76→FO4 runs, interior CELL/REFR +
        // placed children are emitted post-copy — defer nulling refs that resolve
        // nowhere so `repair_dangling_vmad_refs` resolves them post-copy.
        let resolver = VmadResolver::build_with_master_objids(session, master_objids)?
            .with_defer_null(config.defer_placed_child_ref_class);
        // Degenerate-guard parity: legacy no-ops when the output has no records.
        let candidate_sigs = if resolver.output_objids.is_empty() {
            Vec::new()
        } else {
            TOUCHED_RECORD_SIGS
                .iter()
                .filter_map(|s| SigCode::from_str(s).ok())
                .collect()
        };
        Ok(GatherOutput {
            candidate_sigs,
            index: Some(Box::new(resolver)),
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
        let resolver = index
            .and_then(|i| i.downcast_ref::<VmadResolver>())
            .expect("vmad resolver index");
        if null_dangling_in_record(record, resolver) {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::null_dangling_vmad_refs::NullDanglingVmadRefsFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    /// Minimal VMAD blob (version 5, object format 2): one script with one
    /// object property whose FormID is `raw`. Mirrors the legacy fixup's own
    /// `vmad_objfmt2` test builder — objfmt-2 stores the object union as
    /// `[u16][u16][u32 formid]` (FormID LAST).
    fn vmad_blob(raw: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&5u16.to_le_bytes()); // version
        b.extend_from_slice(&2u16.to_le_bytes()); // object format
        b.extend_from_slice(&1u16.to_le_bytes()); // script count
        let script_name = b"Script";
        b.extend_from_slice(&(script_name.len() as u16).to_le_bytes());
        b.extend_from_slice(script_name);
        b.push(0); // status
        b.extend_from_slice(&1u16.to_le_bytes()); // property count
        let prop_name = b"P0";
        b.extend_from_slice(&(prop_name.len() as u16).to_le_bytes());
        b.extend_from_slice(prop_name);
        b.push(1); // property type 1 = object
        b.push(0); // status
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&raw.to_le_bytes());
        b
    }

    fn acti(local: u32, eid: &str, vmad: Vec<u8>, interner: &crate::sym::StringInterner) -> Record {
        let eid_sym = interner.intern(eid);
        Record {
            sig: SigCode::from_str("ACTI").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("VmadRefs.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(eid_sym),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("VMAD").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(vmad)),
                },
            ],
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        // No masters → output master index 0. Property FK 0x00000999 resolves
        // nowhere → nulled; 0x00000801 names an output record → kept.
        let (h_old, h_new) = seed_twin("VmadRefs.esp", |session, schema, interner| {
            for r in [
                acti(0x801, "ActiDangling", vmad_blob(0x0000_0999), interner),
                acti(0x802, "ActiResolving", vmad_blob(0x0000_0801), interner),
            ] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        let legacy = run_legacy_fixup(h_old, Box::new(NullDanglingVmadRefsFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "vmad_refs",
            vec![Box::new(NullDanglingVmadRefsVisitor)],
            &config,
        );

        assert_changed_parity(&legacy, &reports);
        assert_handles_equal(h_old, h_new);
    }
}
