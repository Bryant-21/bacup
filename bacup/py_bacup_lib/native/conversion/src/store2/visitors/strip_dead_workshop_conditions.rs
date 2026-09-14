//! Sweep adapter for `strip_dead_workshop_conditions` (decoded lane).

use std::any::Any;

use crate::fixups::strip_dead_workshop_conditions::apply_to_record;
use crate::fixups::{FixupConfig, FixupError};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub struct StripDeadWorkshopConditionsVisitor;

impl RecordVisitor for StripDeadWorkshopConditionsVisitor {
    fn name(&self) -> &'static str {
        "strip_dead_workshop_conditions"
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
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        if apply_to_record(record) {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::strip_dead_workshop_conditions::StripDeadWorkshopConditionsFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    /// `HasKeyword(CampWorkshopKeyword) == 1`, parameter #1 still carrying the
    /// source plugin's master index.
    fn camp_gate() -> FieldEntry {
        let mut bytes = [0u8; 32];
        bytes[4..8].copy_from_slice(&1.0f32.to_bits().to_le_bytes());
        bytes[8..12].copy_from_slice(&560u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&0x0805_231Au32.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&bytes)),
        }
    }

    fn cobj(
        local: u32,
        eid: &str,
        entries: Vec<FieldEntry>,
        interner: &crate::sym::StringInterner,
    ) -> Record {
        let eid_sym = interner.intern(eid);
        let mut fields: SmallVec<[FieldEntry; 8]> = SmallVec::new();
        fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(eid_sym),
        });
        fields.extend(entries);
        Record {
            sig: SigCode::from_str("COBJ").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("DeadWorkshop.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        let (h_old, h_new) = seed_twin("DeadWorkshop.esp", |session, schema, interner| {
            for r in [
                cobj(0x801, "workshop_co_CampGated", vec![camp_gate()], interner),
                cobj(0x802, "workshop_co_Ungated", vec![], interner),
            ] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        run_legacy_fixup(h_old, Box::new(StripDeadWorkshopConditionsFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "dead_workshop",
            vec![Box::new(StripDeadWorkshopConditionsVisitor)],
            &config,
        );

        assert_eq!(reports[0].1.records_changed, 1);
        assert_handles_equal(h_old, h_new);
    }
}
