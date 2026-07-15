//! Sweep adapter for `apply_weapon_sound_defaults` (raw lane).

use std::any::Any;

use crate::fixups::apply_weapon_sound_defaults::patch_dnam_bytes;
use crate::fixups::{FixupConfig, FixupError};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;
use crate::store2::visitor::{
    GatherOutput, Lane, MasterScanCache, RecordVisitor, SubrecordPatch, SweepCtx,
};
use crate::sym::Sym;

pub struct ApplyWeaponSoundDefaultsVisitor;

impl RecordVisitor for ApplyWeaponSoundDefaultsVisitor {
    fn name(&self) -> &'static str {
        "apply_weapon_sound_defaults"
    }

    fn lane(&self) -> Lane {
        Lane::RawBytes
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

    fn visit_raw(
        &self,
        subrecords: &[(&str, &[u8])],
        _index: Option<&(dyn Any + Send + Sync)>,
        _cx: &SweepCtx<'_>,
        _warnings: &mut Vec<Sym>,
    ) -> Vec<SubrecordPatch> {
        // Legacy patches the FIRST DNAM via patch_subrecord_bytes; a WEAP
        // without DNAM is a no-op here (legacy surfaces it as a warning only).
        for (sig, data) in subrecords {
            if *sig != "DNAM" {
                continue;
            }
            let mut buf = data.to_vec();
            if patch_dnam_bytes(&mut buf) {
                return vec![SubrecordPatch {
                    sig: "DNAM",
                    occurrence: 0,
                    new_bytes: buf,
                }];
            }
            return Vec::new();
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::apply_weapon_sound_defaults::ApplyWeaponSoundDefaultsFixup;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::store2::test_util::assert_handles_equal;
    use crate::store2::visitors::port_test_util::*;
    use smallvec::SmallVec;

    fn weap(local: u32, eid: &str, dnam: Vec<u8>, interner: &crate::sym::StringInterner) -> Record {
        let eid_sym = interner.intern(eid);
        Record {
            sig: SigCode::from_str("WEAP").unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("WeapSnd.esp"),
            },
            eid: Some(eid_sym),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![
                FieldEntry {
                    sig: SubrecordSig::from_str("EDID").unwrap(),
                    value: FieldValue::String(eid_sym),
                },
                FieldEntry {
                    sig: SubrecordSig::from_str("DNAM").unwrap(),
                    value: FieldValue::Bytes(SmallVec::from_vec(dnam)),
                },
            ],
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn visitor_matches_legacy_fixup() {
        let (h_old, h_new) = seed_twin("WeapSnd.esp", |session, schema, interner| {
            // All-zero sound slots → all three defaults injected.
            let needs_defaults = weap(0x801, "WeapZero", vec![0u8; 105], interner);
            // Fully-populated sound slots → byte-identical no-op.
            let populated = weap(0x802, "WeapFull", vec![0xFFu8; 105], interner);
            // Short DNAM → no-op.
            let short = weap(0x803, "WeapShort", vec![0u8; 16], interner);
            for r in [needs_defaults, populated, short] {
                session.add_record(r, schema.as_ref(), interner).unwrap();
            }
        });

        let config = config_for(h_old);
        let legacy = run_legacy_fixup(h_old, Box::new(ApplyWeaponSoundDefaultsFixup), &config);
        let reports = run_visitor_sweep(
            h_new,
            "weap_snd",
            vec![Box::new(ApplyWeaponSoundDefaultsVisitor)],
            &config,
        );

        assert_changed_parity(&legacy, &reports);
        assert_handles_equal(h_old, h_new);
    }
}
