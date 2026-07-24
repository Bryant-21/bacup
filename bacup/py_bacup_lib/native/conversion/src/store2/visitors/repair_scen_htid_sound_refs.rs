//! Sweep adapter for `repair_scen_htid_sound_refs` (raw lane).

use std::any::Any;

use crate::fixups::repair_scen_htid_sound_refs::HtidResolver;
use crate::fixups::{FixupConfig, FixupError};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SubrecordPatch, SweepCtx,
};
use crate::sym::Sym;

pub struct RepairScenHtidSoundRefsVisitor;

impl RecordVisitor for RepairScenHtidSoundRefsVisitor {
    fn name(&self) -> &'static str {
        "repair_scen_htid_sound_refs"
    }

    fn lane(&self) -> Lane {
        Lane::RawBytes
    }

    fn gather(
        &self,
        session: &mut PluginSession,
        mapper: &FormKeyMapper,
        config: &FixupConfig,
        master_cache: &mut MasterScanCache,
    ) -> Result<GatherOutput, FixupError> {
        let master_objids =
            master_cache.master_objid_sets(session, &config.target_master_handle_ids)?;
        let resolver =
            HtidResolver::build_with_master_objids(session, mapper.interner, master_objids)?;
        // Degenerate-guard parity: need at least one output SNDR to repair to.
        let candidate_sigs = if resolver.output_sndr_objids.is_empty() {
            Vec::new()
        } else {
            vec![SigCode::from_str("SCEN").map_err(|e| FixupError::SchemaError(e.to_string()))?]
        };
        Ok(GatherOutput {
            candidate_sigs,
            index: Some(Box::new(resolver)),
            warnings: Vec::new(),
        })
    }

    fn visit_raw(
        &self,
        subrecords: &[(&str, &[u8])],
        index: Option<&(dyn Any + Send + Sync)>,
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> Vec<SubrecordPatch> {
        let resolver = index
            .and_then(|i| i.downcast_ref::<HtidResolver>())
            .expect("htid resolver index");
        let mut patches = Vec::new();
        let mut occurrence = 0usize;
        let mut saw_topic = false;
        let mut saw_looping_max = false;
        for (sig, data) in subrecords {
            match *sig {
                "ANAM" => {
                    saw_topic = false;
                    saw_looping_max = false;
                }
                "DATA" => saw_topic = true,
                "DMAX" => saw_looping_max = true,
                _ => {}
            }
            if *sig != "HTID" {
                continue;
            }
            let this = occurrence;
            occurrence += 1;
            if !saw_topic || saw_looping_max {
                continue;
            }
            let mut buf = data.to_vec();
            if resolver.repair_htid_bytes(&mut buf) {
                patches.push(SubrecordPatch {
                    sig: "HTID",
                    occurrence: this,
                    new_bytes: buf,
                });
            }
        }
        patches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::repair_scen_htid_sound_refs::RepairScenHtidSoundRefsFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::session::open_session;
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_new_native,
    };
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
                plugin: interner.intern("HtidScen.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    fn htid(raw: u32) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str("HTID").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&raw.to_le_bytes())),
        }
    }

    /// Twin handles WITH one (empty) target master, so a master-addressed HTID
    /// can fail to resolve there and get repaired to the output index (1).
    fn seed_repair_twin() -> (u64, u64, u64) {
        let master = plugin_handle_new_native("MasterA.esm", Some("fo4")).expect("master");
        let mut handles = [0u64; 2];
        for h in handles.iter_mut() {
            *h = plugin_handle_new_native("HtidScen.esp", Some("fo4")).expect("handle");
            plugin_handle_add_master_native(*h, "MasterA.esm", None).expect("add master");
            let interner = StringInterner::new();
            let mut session = open_session(*h, None).expect("session");
            let schema = session.schema().expect("schema");
            // Output SNDR 0x900 (the repair target).
            let sndr = rec("SNDR", 0x900, "OutSndr", vec![], &interner);
            // SCEN: HTIDs must sit inside an action row (ANAM anchor) or the
            // encode-time actions-scope normalizer drops them. First HTID
            // 0x00000900 = master 0, unresolved there, objid names the output
            // SNDR → repaired to 0x01000900. Second already output-addressed →
            // untouched.
            let anam = FieldEntry {
                sig: SubrecordSig::from_str("ANAM").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&4u16.to_le_bytes())),
            };
            let data = FieldEntry {
                sig: SubrecordSig::from_str("DATA").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&0u32.to_le_bytes())),
            };
            let dmax = FieldEntry {
                sig: SubrecordSig::from_str("DMAX").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_slice(&10f32.to_le_bytes())),
            };
            let scen = rec(
                "SCEN",
                0x801,
                "Scene",
                vec![anam, data, htid(0x0000_0900), htid(0x0100_0900), dmax],
                &interner,
            );
            for r in [sndr, scen] {
                session.add_record(r, schema.as_ref(), &interner).unwrap();
            }
        }
        (handles[0], handles[1], master)
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        let (h_old, h_new, master) = seed_repair_twin();
        let mut config = config_for(h_old);
        config.target_master_handle_ids = vec![master];

        let legacy = run_legacy_fixup(h_old, Box::new(RepairScenHtidSoundRefsFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "htid",
            vec![Box::new(RepairScenHtidSoundRefsVisitor)],
            &config,
        );

        assert_changed_parity(&legacy, &reports);
        assert_handles_equal(h_old, h_new);
    }

    #[test]
    fn visitor_skips_headtracking_htid_after_dmax() {
        let master = plugin_handle_new_native("MasterA.esm", Some("fo4")).expect("master");
        let mut handles = [0u64; 2];
        for handle in &mut handles {
            *handle = plugin_handle_new_native("HtidScen.esp", Some("fo4")).expect("handle");
            plugin_handle_add_master_native(*handle, "MasterA.esm", None).expect("add master");
            let interner = StringInterner::new();
            let mut session = open_session(*handle, None).expect("session");
            let schema = session.schema().expect("schema");
            let sndr = rec("SNDR", 0x900, "OutSndr", vec![], &interner);
            let scen = rec(
                "SCEN",
                0x801,
                "Scene",
                vec![
                    FieldEntry {
                        sig: SubrecordSig::from_str("ANAM").unwrap(),
                        value: FieldValue::Bytes(SmallVec::from_slice(&4u16.to_le_bytes())),
                    },
                    FieldEntry {
                        sig: SubrecordSig::from_str("DATA").unwrap(),
                        value: FieldValue::Bytes(SmallVec::from_slice(&0u32.to_le_bytes())),
                    },
                    FieldEntry {
                        sig: SubrecordSig::from_str("DMAX").unwrap(),
                        value: FieldValue::Bytes(SmallVec::from_slice(&10f32.to_le_bytes())),
                    },
                    htid(0x0000_0900),
                ],
                &interner,
            );
            for record in [sndr, scen] {
                session
                    .add_record(record, schema.as_ref(), &interner)
                    .unwrap();
            }
        }

        let mut config = config_for(handles[1]);
        config.target_master_handle_ids = vec![master];
        let reports = run_visitor_sweep(
            handles[1],
            "htid",
            vec![Box::new(RepairScenHtidSoundRefsVisitor)],
            &config,
        );

        assert_eq!(
            reports
                .iter()
                .map(|(_, report)| report.records_changed)
                .sum::<u32>(),
            0
        );
        assert_handles_equal(handles[0], handles[1]);
    }
}
