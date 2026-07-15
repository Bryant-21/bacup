//! Sweep adapter for `strip_invalid_quest_condition_params` (decoded lane;
//! sweep-C [2/3]).

use std::any::Any;

use crate::fixups::strip_invalid_quest_condition_params::{
    QuestConditionIndex, collect_quest_condition_index, scrub_invalid_quest_references,
};
use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub struct StripInvalidQuestConditionParamsVisitor;

impl RecordVisitor for StripInvalidQuestConditionParamsVisitor {
    fn name(&self) -> &'static str {
        "strip_invalid_quest_condition_params"
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
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let quest_index = collect_quest_condition_index(
            session,
            mapper.interner,
            config,
            target_schema,
            &mut report,
        )?;
        // Degenerate-guard parity: no QUST or WRLD anywhere → cannot classify.
        let candidate_sigs = if !quest_index.can_classify_conditions() {
            Vec::new()
        } else {
            // Legacy scans every output signature (CTDA carriers self-select
            // by no-op'ing on records without a matching CTDA).
            session
                .target_signatures()
                .map_err(|e| FixupError::HandleError(e.to_string()))?
        };
        Ok(GatherOutput {
            candidate_sigs,
            index: Some(Box::new(quest_index)),
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
        let quest_index = index
            .and_then(|i| i.downcast_ref::<QuestConditionIndex>())
            .expect("quest-condition index");
        if !quest_index.can_classify_conditions() {
            return VisitOutcome::Unchanged;
        }
        let changed = scrub_invalid_quest_references(record, quest_index, _cx.interner);
        if changed {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::strip_invalid_quest_condition_params::StripInvalidQuestConditionParamsFixup;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::session::open_session;
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    /// CTDA blob (32 bytes): function id at offset 8, param1 at 12, param2 at 16.
    fn ctda(function_id: u16, param1: u32, param2: u32) -> FieldEntry {
        let mut b = vec![0u8; 32];
        b[8..10].copy_from_slice(&function_id.to_le_bytes());
        b[12..16].copy_from_slice(&param1.to_le_bytes());
        b[16..20].copy_from_slice(&param2.to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(b)),
        }
    }

    fn push_vmad_string(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&(value.len() as u16).to_le_bytes());
        out.extend_from_slice(value.as_bytes());
    }

    fn pack_fragment_vmad_object_property(alias: i16, form_id: u32) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.push(1);
        out.push(1);
        push_vmad_string(&mut out, "FragmentScript");
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        push_vmad_string(&mut out, "AliasProp");
        out.push(1);
        out.push(0);
        out.extend_from_slice(&0u16.to_le_bytes());
        let alias_offset = out.len();
        out.extend_from_slice(&alias.to_le_bytes());
        out.extend_from_slice(&form_id.to_le_bytes());
        out.push(0);
        push_vmad_string(&mut out, "FragmentScript");
        push_vmad_string(&mut out, "Fragment_0");
        (out, alias_offset)
    }

    fn pack_fragment_object(handle: u64, alias_offset: usize) -> (i16, u32) {
        let interner = crate::sym::StringInterner::new();
        let mut session = open_session(handle, None).expect("session");
        let schema = session.schema().expect("schema");
        let pack_sig = SigCode::from_str("PACK").unwrap();
        let fk = session
            .form_keys_of_sig(pack_sig, &interner)
            .unwrap()
            .into_iter()
            .find(|fk| fk.local == 0x2B_00CE)
            .expect("FF08_ShutDown");
        let record = session.record_decoded(&fk, &schema, &interner).unwrap();
        let bytes = record
            .fields
            .iter()
            .find_map(|field| (field.sig.0 == *b"VMAD").then_some(&field.value))
            .and_then(|value| match value {
                FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .expect("PACK VMAD");
        let alias = i16::from_le_bytes(bytes[alias_offset..alias_offset + 2].try_into().unwrap());
        let form_id = u32::from_le_bytes(
            bytes[alias_offset + 2..alias_offset + 6]
                .try_into()
                .unwrap(),
        );
        (alias, form_id)
    }

    fn rec(
        sig: &str,
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
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("QuestCtda.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        let cobj = 0x0004_695C;
        let (pack_vmad, pack_alias_offset) = pack_fragment_vmad_object_property(10, cobj);
        let (h_old, h_new) = seed_twin("QuestCtda.esp", |session, schema, interner| {
            let worldspace = rec("WRLD", 0x25_DA15, "Worldspace", vec![], interner);
            // A QUST so the valid-quest set is non-empty (degenerate guard) —
            // also carries a source-addressed GetInWorldspace CTDA that must be
            // rewritten to the output WRLD and a null-param2 fn-576 CTDA.
            let qust = rec(
                "QUST",
                0x801,
                "Qust",
                vec![ctda(310, 0x0125_DA15, 0), ctda(576, 0, 0)],
                interner,
            );
            // INFO ownership is group topology and therefore unavailable to the
            // decoded visitor. Procedural aliases are still dropped, while an
            // ordinary low alias stays when its owner cannot be proven.
            let info = rec(
                "INFO",
                0x802,
                "Info",
                vec![ctda(566, 0x07A0_1234, 0), ctda(566, 1, 0)],
                interner,
            );
            let pack = rec(
                "PACK",
                0x2B_00CE,
                "FF08_ShutDown",
                vec![
                    FieldEntry {
                        sig: SubrecordSig::from_str("QNAM").unwrap(),
                        value: FieldValue::Bytes(SmallVec::from_slice(&cobj.to_le_bytes())),
                    },
                    FieldEntry {
                        sig: SubrecordSig::from_str("VMAD").unwrap(),
                        value: FieldValue::Bytes(SmallVec::from_vec(pack_vmad.clone())),
                    },
                ],
                interner,
            );
            for r in [worldspace, qust, info, pack] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        run_legacy_fixup(
            h_old,
            Box::new(StripInvalidQuestConditionParamsFixup),
            &config,
        );
        let reports = run_visitor_sweep(
            h_new,
            "quest_ctda",
            vec![Box::new(StripInvalidQuestConditionParamsVisitor)],
            &config,
        );

        assert_eq!(reports.len(), 1);
        assert_handles_equal(h_old, h_new);
        assert_eq!(pack_fragment_object(h_old, pack_alias_offset), (-1, cobj));
        assert_eq!(pack_fragment_object(h_new, pack_alias_offset), (-1, cobj));
    }
}
