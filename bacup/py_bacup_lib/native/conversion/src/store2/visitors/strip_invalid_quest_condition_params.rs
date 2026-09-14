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
        let quest_index =
            collect_quest_condition_index(session, mapper, config, target_schema, &mut report)?;
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
    use crate::fixups::strip_invalid_quest_condition_params::{
        StripInvalidQuestConditionParamsFixup, repair_final_quest_reference_conditions,
    };
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::session::open_session;
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitor::{MasterScanCache, Sweep, run_sweep};
    use crate::store2::visitors::port_test_util::*;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
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

    fn field(sig: &str, bytes: &[u8]) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
        }
    }

    fn source_pack_get_stage_done(stage: u32) -> FieldEntry {
        let mut condition = ctda(59, stage, 0);
        let FieldValue::Bytes(bytes) = &mut condition.value else {
            unreachable!();
        };
        bytes[4..8].copy_from_slice(&1f32.to_le_bytes());
        bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        condition
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

    fn pack_stage_condition(handle: u64) -> Option<(u32, u32)> {
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
            .find_map(|field| (field.sig.0 == *b"CTDA").then_some(&field.value))
            .and_then(|value| match value {
                FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
                _ => None,
            })?;
        Some((
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
        ))
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

    fn wayward_record(
        sig: &str,
        local: u32,
        fields: Vec<FieldEntry>,
        interner: &crate::sym::StringInterner,
    ) -> crate::record::Record {
        crate::record::Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn production_sweep_restores_wayward_pack_gates_and_final_pass_preserves_them() {
        let source_handle =
            plugin_handle_new_native("SeventySix.esm", Some("fo76")).expect("source handle");
        let target_handle =
            plugin_handle_new_native("SeventySix.esm", Some("fo4")).expect("target handle");
        let interner = crate::sym::StringInterner::new();
        let owner = 0x0040_5E14u32;
        let pack_fixtures = [
            (
                0x0040_BD1E,
                vec![
                    "000000000000803F3B000000C2010000000000000000000000000000FFFFFFFF",
                    "00000000000000003B000000D6010000000000000000000000000000FFFFFFFF",
                ],
            ),
            (
                0x0040_BD22,
                vec!["000000000000803F3B00944312020000000000000000000000000000FFFFFFFF"],
            ),
            (
                0x0058_52E4,
                vec!["000000000000803F3B00944326020000000000000000000000000000FFFFFFFF"],
            ),
        ];

        let source_schema = {
            let mut session = open_session(source_handle, None).expect("source session");
            let schema = session.schema().expect("source schema");
            let quest = wayward_record(
                "QUST",
                owner,
                [450u16, 470, 530, 550]
                    .into_iter()
                    .map(|stage| field("INDX", &stage.to_le_bytes()))
                    .collect(),
                &interner,
            );
            session
                .add_record(quest, schema.as_ref(), &interner)
                .expect("source quest");
            for (pack, conditions) in &pack_fixtures {
                let mut fields: Vec<_> = conditions
                    .iter()
                    .map(|condition| field("CTDA", &hex::decode(condition).unwrap()))
                    .collect();
                fields.push(field("QNAM", &owner.to_le_bytes()));
                session
                    .add_record(
                        wayward_record("PACK", *pack, fields, &interner),
                        schema.as_ref(),
                        &interner,
                    )
                    .expect("source pack");
            }
            schema
        };
        let target_schema = {
            let mut session = open_session(target_handle, None).expect("target session");
            let schema = session.schema().expect("target schema");
            let quest = wayward_record(
                "QUST",
                owner,
                [450u16, 470, 530, 550]
                    .into_iter()
                    .map(|stage| field("INDX", &stage.to_le_bytes()))
                    .collect(),
                &interner,
            );
            session
                .add_record(quest, schema.as_ref(), &interner)
                .expect("target quest");
            for (pack, _) in &pack_fixtures {
                session
                    .add_record(
                        wayward_record(
                            "PACK",
                            *pack,
                            vec![field("QNAM", &owner.to_le_bytes())],
                            &interner,
                        ),
                        schema.as_ref(),
                        &interner,
                    )
                    .expect("target pack without translated CTDA");
            }
            schema
        };

        let mut config = FixupConfig::default();
        config.source_schema = Some(source_schema);
        config.target_schema = Some(target_schema);
        let mut mapper_state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut mapper_state, &interner);
        for local in [owner, 0x0040_BD1E, 0x0040_BD22, 0x0058_52E4] {
            let form_key = FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            };
            mapper.add_mapping(form_key, form_key);
        }

        {
            let mut session =
                open_session(target_handle, Some(source_handle)).expect("production session");
            let sweep = Sweep {
                label: "wayward_pack_condition_recovery",
                visitors: vec![Box::new(StripInvalidQuestConditionParamsVisitor)],
            };
            let reports = run_sweep(
                &mut session,
                &mut mapper,
                &config,
                &sweep,
                &mut MasterScanCache::default(),
            )
            .expect("condition sweep");
            assert_eq!(reports.len(), 1);
            assert_eq!(reports[0].1.records_changed, 3);
            repair_final_quest_reference_conditions(&mut session, &interner, &config)
                .expect("final condition repair");
        }

        let expected = [
            (0x0040_BD1E, vec![(450, 1.0), (470, 0.0)]),
            (0x0040_BD22, vec![(530, 1.0)]),
            (0x0058_52E4, vec![(550, 1.0)]),
        ];
        let mut session = open_session(target_handle, None).expect("result session");
        for (pack, expected_conditions) in expected {
            let form_key = FormKey {
                local: pack,
                plugin: interner.intern("SeventySix.esm"),
            };
            let record = session
                .record_decoded(
                    &form_key,
                    config.target_schema.as_deref().unwrap(),
                    &interner,
                )
                .expect("recovered pack");
            let conditions: Vec<_> = record
                .fields
                .iter()
                .filter_map(|entry| match (&entry.sig.0, &entry.value) {
                    (b"CTDA", FieldValue::Bytes(bytes)) => Some(bytes.as_slice()),
                    _ => None,
                })
                .collect();
            assert_eq!(conditions.len(), expected_conditions.len());
            for (bytes, (stage, comparison)) in conditions.iter().zip(expected_conditions) {
                assert_eq!(u16::from_le_bytes(bytes[8..10].try_into().unwrap()), 59);
                assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), owner);
                assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), stage);
                assert_eq!(
                    f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                    comparison
                );
            }
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        let cobj = 0x0004_695C;
        let owner_quest = 0x0000_0801;
        let owner_stage = 530;
        let (pack_vmad, pack_alias_offset) = pack_fragment_vmad_object_property(10, cobj);
        let (h_old, h_new) = seed_twin("QuestCtda.esp", |session, schema, interner| {
            let worldspace = rec("WRLD", 0x25_DA15, "Worldspace", vec![], interner);
            // A QUST so the valid-quest set is non-empty (degenerate guard) —
            // also carries a source-addressed GetInWorldspace CTDA that must be
            // rewritten to the output WRLD and a null-param2 fn-576 CTDA.
            let qust = rec(
                "QUST",
                owner_quest,
                "Qust",
                vec![
                    field("INDX", &(owner_stage as u16).to_le_bytes()),
                    ctda(310, 0x0125_DA15, 0),
                    ctda(576, 0, 0),
                ],
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
                        value: FieldValue::Bytes(SmallVec::from_slice(&owner_quest.to_le_bytes())),
                    },
                    source_pack_get_stage_done(owner_stage),
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
        // The shared port-test helper intentionally opens no source handle, so
        // source-only PACK lowering must fail closed in both implementations.
        assert_eq!(pack_stage_condition(h_old), None);
        assert_eq!(pack_stage_condition(h_new), None);
    }
}
