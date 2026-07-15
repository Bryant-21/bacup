//! Sweep-visitor adapters for the fixups cleared for fusion into sweeps.
//!
//! Each visitor is a thin adapter over its legacy fixup's *existing* gather
//! function and pure kernel — the logic is shared, never duplicated, so the
//! two drivers cannot drift. Every visitor carries a twin-fixture equivalence
//! test: the legacy fixup and the sweep run on identical plugin handles and
//! the results must be byte-identical (`test_util::assert_handles_equal`).

pub mod apply_weapon_sound_defaults;
pub mod cleanup_bodypart_data;
pub mod null_dangling_misc_refs;
pub mod null_dangling_vmad_refs;
pub mod null_invalid_qust_alla_keywords;
pub mod prune_faction_relations;
pub mod remap_struct_internal_formids;
pub mod repair_scen_htid_sound_refs;
pub mod strip_atx_cobj_conditions;
pub mod strip_invalid_quest_condition_params;
pub mod synthesize_weap_data_blocks;

#[cfg(test)]
mod sweep_c_tests {
    use crate::fixups::null_dangling_vmad_refs::NullDanglingVmadRefsFixup;
    use crate::fixups::null_invalid_qust_alla_keywords::NullInvalidQustAllaKeywordsFixup;
    use crate::fixups::strip_invalid_quest_condition_params::StripInvalidQuestConditionParamsFixup;
    use crate::fixups::{Fixup, FixupRegistry};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::session::open_session;
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::null_dangling_vmad_refs::NullDanglingVmadRefsVisitor;
    use crate::store2::visitors::null_invalid_qust_alla_keywords::NullInvalidQustAllaKeywordsVisitor;
    use crate::store2::visitors::port_test_util::*;
    use crate::store2::visitors::strip_invalid_quest_condition_params::StripInvalidQuestConditionParamsVisitor;
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn rec(
        sig: &str,
        local: u32,
        eid: &str,
        extra: Vec<FieldEntry>,
        interner: &StringInterner,
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
                plugin: interner.intern("SweepC.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    fn bytes_entry(sig: &str, b: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(b)),
        }
    }

    fn vmad_blob(raw: u32) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&5u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&6u16.to_le_bytes());
        b.extend_from_slice(b"Script");
        b.push(0);
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(b"P0");
        b.push(1);
        b.push(0);
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&raw.to_le_bytes());
        b
    }

    fn ctda(function_id: u16, param1: u32, param2: u32) -> Vec<u8> {
        let mut b = vec![0u8; 32];
        b[8..10].copy_from_slice(&function_id.to_le_bytes());
        b[12..16].copy_from_slice(&param1.to_le_bytes());
        b[16..20].copy_from_slice(&param2.to_le_bytes());
        b
    }

    fn alla(rows: &[(u32, i32)]) -> Vec<u8> {
        let mut b = Vec::new();
        for (kw, idx) in rows {
            b.extend_from_slice(&kw.to_le_bytes());
            b.extend_from_slice(&idx.to_le_bytes());
        }
        b
    }

    /// The fused trio (sweep-C): legacy fixup-major sequence vs one
    /// record-major sweep, on a fixture where a single QUST is touched by all
    /// three (VMAD vs CTDA vs ALLA — distinct subrecord domains) plus
    /// per-fixup-only records.
    #[test]
    fn fused_sweep_matches_legacy_fixup_sequence() {
        let seed = |session: &mut crate::session::PluginSession,
                    schema: &std::sync::Arc<crate::schema::AuthoringSchema>,
                    interner: &StringInterner| {
            let kywd = rec("KYWD", 0x900, "GoodKW", vec![], interner);
            // QUST hit by all three visitors: dangling VMAD object FK, a
            // null-param2 fn-576 CTDA (context rule), and a dangling ALLA kw.
            let qust = rec(
                "QUST",
                0x801,
                "TripleQust",
                vec![
                    bytes_entry("VMAD", vmad_blob(0x0000_0777)),
                    bytes_entry("CTDA", ctda(576, 0, 0)),
                    bytes_entry("ALST", 1u32.to_le_bytes().to_vec()),
                    bytes_entry("ALLA", alla(&[(0x0002_FD66, 1), (0x0000_0900, 1)])),
                ],
                interner,
            );
            // ACTI hit only by the quest-param rule (non-context record).
            let acti = rec(
                "ACTI",
                0x802,
                "CtdaActi",
                vec![bytes_entry("CTDA", ctda(566, 0x07A0_1234, 0))],
                interner,
            );
            // TERM hit only by the VMAD rule.
            let term = rec(
                "TERM",
                0x803,
                "VmadTerm",
                vec![bytes_entry("VMAD", vmad_blob(0x0000_0778))],
                interner,
            );
            for r in [kywd, qust, acti, term] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        };
        let (h_old, h_new) = seed_twin("SweepC.esp", seed);
        let config = config_for(h_old);

        // Legacy: the three fixups in registry order, fixup-major.
        {
            let interner = StringInterner::new();
            let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            let mut session = open_session(h_old, None).expect("session");
            let mut registry = FixupRegistry::new();
            let fixups: [Box<dyn Fixup>; 3] = [
                Box::new(NullDanglingVmadRefsFixup),
                Box::new(StripInvalidQuestConditionParamsFixup),
                Box::new(NullInvalidQustAllaKeywordsFixup),
            ];
            for f in fixups {
                registry.register(f);
            }
            registry
                .run_all_in_session(&mut session, &mut mapper, &config)
                .expect("legacy sequence");
        }

        // v2: ONE fused sweep, record-major, visitors in the same order.
        let reports = run_visitor_sweep(
            h_new,
            "sweep_c",
            vec![
                Box::new(NullDanglingVmadRefsVisitor),
                Box::new(StripInvalidQuestConditionParamsVisitor),
                Box::new(NullInvalidQustAllaKeywordsVisitor),
            ],
            &config,
        );

        assert_eq!(reports.len(), 3, "all three visitors apply and report");
        assert_handles_equal(h_old, h_new);
    }
}

#[cfg(test)]
pub(crate) mod port_test_util {
    use std::sync::Arc;

    use crate::fixups::{Fixup, FixupConfig, FixupRegistry};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, MapperState};
    use crate::schema::AuthoringSchema;
    use crate::session::{PluginSession, open_session};
    use crate::store2::visitor::{MasterScanCache, RecordVisitor, Sweep, run_sweep};
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

    /// Build two identical plugin handles (same plugin name — FormKey encoding
    /// must match byte-for-byte) seeded by the same closure.
    pub(crate) fn seed_twin(
        plugin: &str,
        seed: impl Fn(&mut PluginSession, &Arc<AuthoringSchema>, &StringInterner),
    ) -> (u64, u64) {
        let mut handles = [0u64; 2];
        for h in handles.iter_mut() {
            *h = plugin_handle_new_native(plugin, Some("fo4")).expect("handle");
            let interner = StringInterner::new();
            let mut session = open_session(*h, None).expect("session");
            let schema = session.schema().expect("schema");
            seed(&mut session, &schema, &interner);
        }
        (handles[0], handles[1])
    }

    pub(crate) fn config_for(handle: u64) -> FixupConfig {
        let mut session = open_session(handle, None).expect("session");
        let schema = session.schema().expect("schema");
        let mut config = FixupConfig::default();
        config.target_schema = Some(schema);
        config
    }

    pub(crate) fn run_legacy_fixup(
        handle: u64,
        fixup: Box<dyn Fixup>,
        config: &FixupConfig,
    ) -> Vec<(String, crate::fixups::FixupReport)> {
        run_legacy_fixup_with_mapper(handle, fixup, config, |_| {})
    }

    pub(crate) fn run_legacy_fixup_with_mapper(
        handle: u64,
        fixup: Box<dyn Fixup>,
        config: &FixupConfig,
        seed_mapper: impl Fn(&mut FormKeyMapper),
    ) -> Vec<(String, crate::fixups::FixupReport)> {
        let interner = StringInterner::new();
        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        seed_mapper(&mut mapper);
        let mut session = open_session(handle, None).expect("session");
        let mut registry = FixupRegistry::new();
        registry.register(fixup);
        registry
            .run_all_in_session(&mut session, &mut mapper, config)
            .expect("legacy fixup")
    }

    /// Count-parity assert: the visitor's records_changed must equal the
    /// legacy fixup's, and the fixture must actually exercise a mutation.
    pub(crate) fn assert_changed_parity(
        legacy: &[(String, crate::fixups::FixupReport)],
        v2: &[(String, crate::fixups::FixupReport)],
    ) {
        let legacy_changed: u32 = legacy.iter().map(|(_, r)| r.records_changed).sum();
        let v2_changed: u32 = v2.iter().map(|(_, r)| r.records_changed).sum();
        assert_eq!(
            legacy_changed, v2_changed,
            "records_changed parity: legacy={legacy_changed} v2={v2_changed}"
        );
        assert!(
            legacy_changed > 0,
            "fixture exercised no mutation — strengthen the fixture"
        );
    }

    pub(crate) fn run_visitor_sweep(
        handle: u64,
        label: &'static str,
        visitors: Vec<Box<dyn RecordVisitor>>,
        config: &FixupConfig,
    ) -> Vec<(String, crate::fixups::FixupReport)> {
        run_visitor_sweep_with_mapper(handle, label, visitors, config, |_| {})
    }

    pub(crate) fn run_visitor_sweep_with_mapper(
        handle: u64,
        label: &'static str,
        visitors: Vec<Box<dyn RecordVisitor>>,
        config: &FixupConfig,
        seed_mapper: impl Fn(&mut FormKeyMapper),
    ) -> Vec<(String, crate::fixups::FixupReport)> {
        let interner = StringInterner::new();
        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        seed_mapper(&mut mapper);
        let mut session = open_session(handle, None).expect("session");
        let sweep = Sweep { label, visitors };
        let mut master_cache = MasterScanCache::default();
        run_sweep(&mut session, &mut mapper, config, &sweep, &mut master_cache).expect("sweep")
    }
}
