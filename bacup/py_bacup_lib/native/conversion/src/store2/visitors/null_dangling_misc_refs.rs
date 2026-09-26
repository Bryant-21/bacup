//! Sweep adapter for `null_dangling_misc_refs` (decoded lane).

use std::any::Any;

use crate::fixups::null_dangling_misc_refs::{SlotResolver, apply_to_record, touched_record_sigs};
use crate::fixups::{FixupConfig, FixupError};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub struct NullDanglingMiscRefsVisitor;

impl RecordVisitor for NullDanglingMiscRefsVisitor {
    fn name(&self) -> &'static str {
        "null_dangling_misc_refs"
    }

    fn lane(&self) -> Lane {
        Lane::Decoded
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
        let resolver = SlotResolver::build_with_master_objids(
            session,
            mapper.interner,
            master_objids,
            &config.target_master_handle_ids,
        )?;
        let candidate_sigs = touched_record_sigs()
            .into_iter()
            .filter_map(|s| SigCode::from_str(s).ok())
            .collect();
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
        cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        let resolver = index
            .and_then(|i| i.downcast_ref::<SlotResolver>())
            .expect("slot resolver index");
        if apply_to_record(record, resolver, cx.interner) {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::null_dangling_misc_refs::NullDanglingMiscRefsFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, RecordFlags};
    use crate::session::open_session;
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_new_native,
    };
    use smallvec::SmallVec;

    const MGEF_ASSOC_ITEM_OFFSET: usize = 8;
    const MGEF_ARCHETYPE_OFFSET: usize = 64;
    const MGEF_ARCHETYPE_CLOAK: u32 = 35;

    fn record(
        sig: &str,
        local: u32,
        eid: &str,
        fields: Vec<FieldEntry>,
        plugin: &str,
        interner: &StringInterner,
    ) -> Record {
        let eid_sym = interner.intern(eid);
        let mut all_fields: SmallVec<[FieldEntry; 8]> = smallvec::smallvec![FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::String(eid_sym),
        }];
        all_fields.extend(fields);
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields: all_fields,
            warnings: SmallVec::new(),
        }
    }

    fn cloak_mgef(local: u32, assoc_item: u32, interner: &StringInterner) -> Record {
        let mut data = vec![0u8; MGEF_ARCHETYPE_OFFSET + 4];
        data[MGEF_ASSOC_ITEM_OFFSET..MGEF_ASSOC_ITEM_OFFSET + 4]
            .copy_from_slice(&assoc_item.to_le_bytes());
        data[MGEF_ARCHETYPE_OFFSET..MGEF_ARCHETYPE_OFFSET + 4]
            .copy_from_slice(&MGEF_ARCHETYPE_CLOAK.to_le_bytes());
        record(
            "MGEF",
            local,
            "POST_DLC01EnchBot_Cloak_RadiationEffect",
            vec![FieldEntry {
                sig: SubrecordSig::from_str("DATA").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(data)),
            }],
            "MiscMgef.esp",
            interner,
        )
    }

    #[test]
    fn visitor_matches_legacy_for_master_refr_output_spell_collision() {
        let master = plugin_handle_new_native("Fallout4.esm", Some("fo4")).expect("master");
        {
            let interner = StringInterner::new();
            let mut session = open_session(master, None).expect("master session");
            let schema = session.schema().expect("master schema");
            for rec in [
                record(
                    "REFR",
                    0x10F280,
                    "CollisionRefr",
                    vec![],
                    "Fallout4.esm",
                    &interner,
                ),
                record(
                    "SPEL",
                    0x0073E4,
                    "DLC01EnchBot_Cloak_RadiationSpell",
                    vec![],
                    "Fallout4.esm",
                    &interner,
                ),
            ] {
                session.add_record(rec, schema.as_ref(), &interner).unwrap();
            }
        }

        let mut handles = [0u64; 2];
        for handle in &mut handles {
            *handle = plugin_handle_new_native("MiscMgef.esp", Some("fo4")).expect("output");
            plugin_handle_add_master_native(*handle, "Fallout4.esm", None).expect("add master");
            let interner = StringInterner::new();
            let mut session = open_session(*handle, None).expect("output session");
            let schema = session.schema().expect("output schema");
            for rec in [
                record(
                    "SPEL",
                    0x10F280,
                    "POST_DLC01EnchBot_Cloak_RadiationSpell",
                    vec![],
                    "MiscMgef.esp",
                    &interner,
                ),
                cloak_mgef(0x10F26C, 0x0010_F280, &interner),
            ] {
                session.add_record(rec, schema.as_ref(), &interner).unwrap();
            }
        }

        let mut config = config_for(handles[0]);
        config.target_master_handle_ids = vec![master];
        let legacy = run_legacy_fixup(handles[0], Box::new(NullDanglingMiscRefsFixup), &config);
        let reports = run_visitor_sweep(
            handles[1],
            "misc_mgef_refs",
            vec![Box::new(NullDanglingMiscRefsVisitor)],
            &config,
        );

        assert_changed_parity(&legacy, &reports);
        assert_handles_equal(handles[0], handles[1]);
    }
}
