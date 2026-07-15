//! Sweep adapter for `strip_atx_cobj_conditions` (decoded lane).

use std::any::Any;

use crate::fixups::strip_atx_cobj_conditions::apply_to_record;
use crate::fixups::{FixupConfig, FixupError};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub struct StripAtxCobjConditionsVisitor;

impl RecordVisitor for StripAtxCobjConditionsVisitor {
    fn name(&self) -> &'static str {
        "strip_atx_cobj_conditions"
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
            SigCode::from_str("COBJ").map_err(|e| FixupError::SchemaError(e.to_string()))?,
        ]))
    }

    fn visit_decoded(
        &self,
        record: &mut Record,
        _index: Option<&(dyn Any + Send + Sync)>,
        cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        let eid_matches = record.eid.is_some_and(|sym| {
            cx.interner
                .resolve(sym)
                .is_some_and(|s| s.starts_with("ATX_"))
        });
        if eid_matches && apply_to_record(record) {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::strip_atx_cobj_conditions::StripAtxCobjConditionsFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    fn cobj(local: u32, eid: &str, interner: &crate::sym::StringInterner) -> Record {
        let eid_sym = interner.intern(eid);
        Record {
            sig: SigCode::from_str("COBJ").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("AtxCobj.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(eid_sym),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("CTDA").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_slice(&[0u8; 32])),
                },
            ],
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        let (h_old, h_new) = seed_twin("AtxCobj.esp", |session, schema, interner| {
            for r in [
                cobj(0x801, "ATX_StripMe", interner),
                cobj(0x802, "KeepMe", interner),
            ] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        run_legacy_fixup(h_old, Box::new(StripAtxCobjConditionsFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "atx",
            vec![Box::new(StripAtxCobjConditionsVisitor)],
            &config,
        );

        assert_eq!(reports[0].1.records_changed, 1);
        assert_handles_equal(h_old, h_new);
    }
}
