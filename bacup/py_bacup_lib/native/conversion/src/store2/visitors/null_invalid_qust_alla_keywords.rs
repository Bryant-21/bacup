//! Sweep adapter for `null_invalid_qust_alla_keywords` (the normative port
//! template).

use std::any::Any;

use rustc_hash::FxHashSet;

use crate::fixups::null_invalid_qust_alla_keywords::{
    collect_valid_keyword_encoded_ids, null_invalid_alla_keywords,
};
use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub struct NullInvalidQustAllaKeywordsVisitor;

impl RecordVisitor for NullInvalidQustAllaKeywordsVisitor {
    fn name(&self) -> &'static str {
        "null_invalid_qust_alla_keywords" // == legacy Fixup::name()
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
        let mut report = FixupReport::empty();
        let valid =
            collect_valid_keyword_encoded_ids(session, mapper.interner, config, &mut report)?;
        Ok(GatherOutput {
            candidate_sigs: vec![
                SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?,
            ],
            index: Some(Box::new(valid)),
            warnings: report.warnings,
        })
    }

    fn visit_decoded(
        &self,
        record: &mut Record,
        index: Option<&(dyn Any + Send + Sync)>,
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        let valid = index
            .and_then(|i| i.downcast_ref::<FxHashSet<u32>>())
            .expect("valid-KYWD index");
        // Degenerate-guard parity: the legacy fixup early-returns when no KYWD
        // exists anywhere rather than nulling every ALLA keyword blind.
        if valid.is_empty() {
            return VisitOutcome::Unchanged;
        }
        if null_invalid_alla_keywords(record, valid) {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::null_invalid_qust_alla_keywords::NullInvalidQustAllaKeywordsFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    fn rec(
        sig: &str,
        local: u32,
        eid: &str,
        extra: Vec<FieldEntry>,
        interner: &crate::sym::StringInterner,
        plugin: &str,
    ) -> Record {
        let eid_sym = interner.intern(eid);
        let mut fields: SmallVec<[FieldEntry; 8]> = smallvec::smallvec![FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(eid_sym),
        }];
        fields.extend(extra);
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    fn alla(rows: &[(u32, i32)]) -> FieldEntry {
        let mut bytes = Vec::new();
        for (keyword, alias_index) in rows {
            bytes.extend_from_slice(&keyword.to_le_bytes());
            bytes.extend_from_slice(&alias_index.to_le_bytes());
        }
        FieldEntry {
            sig: SubrecordSig::from_str("ALLA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    /// ALST anchor — required so encode-time QUST normalization
    /// (`emit_qust_alias_segment`) keeps ALLA rows referencing this alias id.
    fn alst(alias_id: u32) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str("ALST").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&alias_id.to_le_bytes())),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        const PLUGIN: &str = "AllaTwin.esp";
        let (h_old, h_new) = seed_twin(PLUGIN, |session, schema, interner| {
            // One real KYWD (encoded id == local for a no-master plugin).
            let kywd = rec("KYWD", 0x900, "GoodKW", vec![], interner, PLUGIN);
            // QUST with one dangling + one valid ALLA keyword (alias ids 1/2
            // anchored by ALSTs so normalization keeps the rows).
            let qust_bad = rec(
                "QUST",
                0x801,
                "QustBad",
                vec![
                    alst(1),
                    alst(2),
                    alla(&[(0x0002_FD66, 1), (0x0000_0900, 2)]),
                ],
                interner,
                PLUGIN,
            );
            // Control QUST: all-valid ALLA — must stay byte-identical.
            let qust_ok = rec(
                "QUST",
                0x802,
                "QustOk",
                vec![alst(5), alst(6), alla(&[(0, 5), (0x0000_0900, 6)])],
                interner,
                PLUGIN,
            );
            for r in [kywd, qust_bad, qust_ok] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        run_legacy_fixup(h_old, Box::new(NullInvalidQustAllaKeywordsFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "alla",
            vec![Box::new(NullInvalidQustAllaKeywordsVisitor)],
            &config,
        );

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].0, "null_invalid_qust_alla_keywords");
        assert_eq!(reports[0].1.records_changed, 1);
        assert_handles_equal(h_old, h_new);
    }
}
