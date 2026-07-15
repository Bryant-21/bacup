//! Sweep adapter for `synthesize_weap_data_blocks` (decoded lane).

use std::any::Any;

use crate::fixups::synthesize_weap_data_blocks::{
    apply_to_record, choose_dnam_default, is_creature_root_sig,
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

pub struct SynthesizeWeapDataBlocksVisitor;

impl RecordVisitor for SynthesizeWeapDataBlocksVisitor {
    fn name(&self) -> &'static str {
        "synthesize_weap_data_blocks"
    }

    fn lane(&self) -> Lane {
        Lane::Decoded
    }

    fn gather(
        &self,
        _session: &mut PluginSession,
        _mapper: &FormKeyMapper,
        _config: &FixupConfig,
        _master_cache: &mut MasterScanCache,
    ) -> Result<GatherOutput, FixupError> {
        Ok(GatherOutput::sigs_only(vec![
            SigCode::from_str("WEAP").map_err(|e| FixupError::SchemaError(e.to_string()))?,
        ]))
    }

    fn visit_decoded(
        &self,
        record: &mut Record,
        _index: Option<&(dyn Any + Send + Sync)>,
        cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        let is_creature_root = cx
            .config
            .root_sig
            .map(is_creature_root_sig)
            .unwrap_or(false);
        let eid_str: String = record
            .eid
            .and_then(|sym| cx.interner.resolve(sym))
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let dnam_default = choose_dnam_default(is_creature_root, &eid_str);
        if apply_to_record(record, dnam_default) {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::synthesize_weap_data_blocks::SynthesizeWeapDataBlocksFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    fn bare_weap(local: u32, eid: &str, interner: &crate::sym::StringInterner) -> Record {
        let eid_sym = interner.intern(eid);
        Record {
            sig: SigCode::from_str("WEAP").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("SynthWeap.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("EDID").unwrap(),
                value: FieldValue::String(eid_sym),
            }],
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        // Bare WEAP (no DNAM/FNAM) → both injected; default-FO4 path
        // (config.root_sig = None).
        let (h_old, h_new) = seed_twin("SynthWeap.esp", |session, schema, interner| {
            session
                .add_record(
                    bare_weap(0x801, "NeedsBlocks", interner),
                    schema.as_ref(),
                    interner,
                )
                .unwrap();
        });

        let config = config_for(h_old);
        run_legacy_fixup(h_old, Box::new(SynthesizeWeapDataBlocksFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "synth_weap",
            vec![Box::new(SynthesizeWeapDataBlocksVisitor)],
            &config,
        );

        assert_eq!(reports[0].1.records_changed, 1);
        assert_handles_equal(h_old, h_new);
    }
}
