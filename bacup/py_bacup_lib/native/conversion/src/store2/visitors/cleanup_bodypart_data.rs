//! Sweep adapter for `cleanup_bodypart_data` (decoded lane). The legacy
//! fixup's phase-1 RACE scan becomes the gather; the visitor touches only
//! BPTD records via the pre-built `bptd_to_skel` map.

use std::any::Any;
use std::collections::HashMap;

use crate::fixups::creature::cleanup_bodypart_data::{
    apply_to_record, extract_bptd_formkey, extract_male_skeletal_model_sym,
};
use crate::fixups::{FixupConfig, FixupError};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::Record;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SweepCtx, VisitOutcome,
};
use crate::sym::Sym;

pub struct CleanupBodypartDataVisitor;

impl RecordVisitor for CleanupBodypartDataVisitor {
    fn name(&self) -> &'static str {
        "cleanup_bodypart_data"
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
        let race_sig =
            SigCode::from_str("RACE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let bptd_sig =
            SigCode::from_str("BPTD").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut warnings = Vec::new();
        let race_fks = session
            .form_keys_of_sig(race_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut bptd_to_skel: HashMap<FormKey, Sym> = HashMap::new();
        for fk in race_fks {
            let record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    warnings.push(
                        mapper
                            .interner
                            .intern(&format!("cleanup_bptd_race_read:{e}")),
                    );
                    continue;
                }
            };
            let Some(bptd_fk) = extract_bptd_formkey(&record) else {
                continue;
            };
            let Some(skel_sym) = extract_male_skeletal_model_sym(&record) else {
                continue;
            };
            if mapper
                .interner
                .resolve(skel_sym)
                .map(str::is_empty)
                .unwrap_or(true)
            {
                continue;
            }
            bptd_to_skel.insert(bptd_fk, skel_sym);
        }

        // Degenerate-guard parity: no mapped RACE → legacy early-returns.
        let candidate_sigs = if bptd_to_skel.is_empty() {
            Vec::new()
        } else {
            vec![bptd_sig]
        };
        Ok(GatherOutput {
            candidate_sigs,
            index: Some(Box::new(bptd_to_skel)),
            warnings,
        })
    }

    fn visit_decoded(
        &self,
        record: &mut Record,
        index: Option<&(dyn Any + Send + Sync)>,
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> VisitOutcome {
        let bptd_to_skel = index
            .and_then(|i| i.downcast_ref::<HashMap<FormKey, Sym>>())
            .expect("bptd-to-skel index");
        let Some(&skel_sym) = bptd_to_skel.get(&record.form_key) else {
            return VisitOutcome::Unchanged;
        };
        if apply_to_record(record, skel_sym) {
            VisitOutcome::Changed
        } else {
            VisitOutcome::Unchanged
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::creature::cleanup_bodypart_data::CleanupBodypartDataFixup;
    use crate::ids::SubrecordSig;
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
                plugin: interner.intern("Bptd.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        let (h_old, h_new) = seed_twin("Bptd.esp", |session, schema, interner| {
            // RACE: GNAM → BPTD 0x802; MNAM marker then ANAM = male skeleton.
            let race = rec(
                "RACE",
                0x801,
                "CreatureRace",
                vec![
                    FieldEntry {
                        sig: SubrecordSig::from_str("GNAM").unwrap(),
                        value: FieldValue::FormKey(FormKey {
                            local: 0x802,
                            plugin: interner.intern("Bptd.esp"),
                        }),
                    },
                    FieldEntry {
                        sig: SubrecordSig::from_str("MNAM").unwrap(),
                        value: FieldValue::Bytes(SmallVec::new()),
                    },
                    FieldEntry {
                        sig: SubrecordSig::from_str("ANAM").unwrap(),
                        value: FieldValue::String(
                            interner.intern("Actors\\Creature\\Skeleton.nif"),
                        ),
                    },
                ],
                interner,
            );
            // BPTD with a differing MODL → rewritten to the RACE skeleton.
            let bptd = rec(
                "BPTD",
                0x802,
                "CreatureBptd",
                vec![FieldEntry {
                    sig: SubrecordSig::from_str("MODL").unwrap(),
                    value: FieldValue::String(interner.intern("Actors\\Wrong.nif")),
                }],
                interner,
            );
            for r in [race, bptd] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        run_legacy_fixup(h_old, Box::new(CleanupBodypartDataFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "bptd",
            vec![Box::new(CleanupBodypartDataVisitor)],
            &config,
        );

        assert_eq!(reports.len(), 1);
        assert_handles_equal(h_old, h_new);
    }
}
