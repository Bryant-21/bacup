//! `Fo4StarfieldHook`: fo4→starfield pair-level record hook (world records only).
//!
//! Two jobs the YAML translation map cannot do:
//!
//! 1. **Struct relayout.** `embedded/translation_maps/fo4_to_starfield.yaml`
//!    can only rename or drop a subrecord; it cannot repack one. Five FO4
//!    subrecords carry the record's actual visual payload but have a different
//!    Starfield byte layout under the same (or a renamed) 4CC, so the map had
//!    to drop them. This hook rebuilds each one in Starfield shape:
//!    `LIGH.DATA`→`DAT2`, `SCOL.ONAM`, `WATR.DNAM`, `LGTM.DATA`, `LGTM.DALC`.
//!    `SCOL.DATA` is the sixth member of the family for the opposite reason:
//!    its layout is *identical* in both games, so the map carries it raw — but
//!    its position floats are FO4 game units and need the spatial rescale that
//!    a raw carry skips.
//!
//! 2. **Placeholder weather.** `WTHR` and `CLMT` are whole-record `skip_records`
//!    entries in the map, and the map's `defaults:` block is inert (see the
//!    map header), so every surviving reference into the weather system would
//!    dangle. This hook repoints them at vanilla Starfield forms.
//!
//! Phase ordering matters (`run.rs`: pre_translate → map drop/rename/transforms
//! → `FormKeyMapper::rewrite_record` → post_translate):
//! - Relayouts that touch no FormKey run in `pre_translate`, so the generic
//!   translate step copies an already-Starfield-shaped payload through.
//! - `SCOL.ONAM` also runs in `pre_translate` but emits a `FieldValue::Struct`
//!   whose `Static` member stays a `FieldValue::FormKey`, so the mapper still
//!   remaps it and the writer still resolves its master index.
//! - Weather repointing runs in `post_translate` because it installs
//!   `Starfield.esm` FormKeys that the mapper must not touch.

mod struct_relayout;
mod weather;

use crate::record::Record;
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};

pub struct Fo4StarfieldHook;

impl PairHook for Fo4StarfieldHook {
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        struct_relayout::relayout_ligh_data_to_dat2(record);
        struct_relayout::relayout_scol_onam(ctx.interner, record);
        struct_relayout::rescale_scol_placements(record);
        struct_relayout::rescale_object_bounds(record);
        struct_relayout::relayout_watr_dnam(record);
        struct_relayout::relayout_lgtm_data(record);
        struct_relayout::relayout_lgtm_dalc(record);
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        weather::point_weather_refs_at_vanilla_starfield(ctx.interner, record);
        Ok(())
    }

    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

/// End-to-end interlock between the hook and `fo4_to_starfield.yaml`.
///
/// Each relayout only reaches the output if the map stops dropping its
/// subrecord, so these run the real `pre_translate` → `Translator::translate`
/// → `post_translate` sequence against the embedded map rather than calling the
/// relayout functions directly.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::sym::StringInterner;
    use crate::translator::{Game, TranslateResult, Translator};

    fn translate(translator: &Translator, interner: &StringInterner, mut record: Record) -> Record {
        Fo4StarfieldHook
            .pre_translate(&mut PairCtx::new(interner), &mut record)
            .unwrap();
        let TranslateResult::Translated(mut out) = translator.translate(&record, interner) else {
            panic!("{} should translate", record.sig.as_str());
        };
        Fo4StarfieldHook
            .post_translate(&mut PairCtx::new(interner), &mut out)
            .unwrap();
        out
    }

    fn source(sig: &str, interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str(sig).unwrap(),
            FormKey::parse("000800@Fallout4.esm", interner).unwrap(),
        )
    }

    fn push_bytes(record: &mut Record, sig: &str, payload: Vec<u8>) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(payload.into_iter().collect()),
        });
    }

    fn payload_len(record: &Record, sig: &str) -> usize {
        match &record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == sig)
            .unwrap_or_else(|| panic!("{sig} should survive translation"))
            .value
        {
            FieldValue::Bytes(raw) => raw.len(),
            other => panic!("{sig} should be raw Bytes, got {other:?}"),
        }
    }

    fn fo4_starfield_translator() -> Translator {
        Translator::new(Game::Fo4, Game::Starfield)
            .expect("the embedded fo4_to_starfield map must parse")
    }

    #[test]
    fn ligh_reaches_the_target_as_dat2_not_as_a_dropped_data() {
        let interner = StringInterner::new();
        let translator = fo4_starfield_translator();
        let mut rec = source("LIGH", &interner);
        push_bytes(&mut rec, "DATA", vec![0u8; 64]);

        let out = translate(&translator, &interner, rec);

        assert_eq!(payload_len(&out, "DAT2"), 76);
        assert!(
            out.fields.iter().all(|e| e.sig.as_str() != "DATA"),
            "the map's DATA drop must still fire as the fail-closed backstop"
        );
    }

    #[test]
    fn scol_part_references_survive_the_map_drop_list() {
        let interner = StringInterner::new();
        let translator = fo4_starfield_translator();
        let mut rec = source("SCOL", &interner);
        let part = FormKey::parse("0247C1@Fallout4.esm", &interner).unwrap();
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ONAM").unwrap(),
            value: FieldValue::FormKey(part),
        });

        let out = translate(&translator, &interner, rec);

        let onam = out
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "ONAM")
            .expect("SCOL parts must not be dropped — spec gate 3");
        let FieldValue::Struct(members) = &onam.value else {
            panic!("ONAM should be a Starfield-shaped struct");
        };
        assert_eq!(members[0].1, FieldValue::FormKey(part));
    }

    #[test]
    fn scol_part_placements_reach_the_target_in_metres() {
        let interner = StringInterner::new();
        let translator = fo4_starfield_translator();
        let mut rec = source("SCOL", &interner);
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("ONAM").unwrap(),
            value: FieldValue::FormKey(FormKey::parse("0247C1@Fallout4.esm", &interner).unwrap()),
        });
        // Position 844/142/0 game units (the vanilla FO4 p90 and median),
        // rotation 1.5 rad, per-part scale 2.0.
        let mut row = Vec::new();
        for v in [844.0f32, 142.0, 0.0, 1.5, 0.0, 0.0, 2.0] {
            row.extend_from_slice(&v.to_le_bytes());
        }
        push_bytes(&mut rec, "DATA", row);

        let out = translate(&translator, &interner, rec);

        let FieldValue::Bytes(raw) = &out
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "DATA")
            .expect("SCOL placements must survive the map")
            .value
        else {
            panic!("DATA should stay raw Bytes");
        };
        let v: Vec<f32> = raw
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert!(
            (10.0..14.0).contains(&v[0]),
            "x should be metres, got {}",
            v[0]
        );
        assert!(
            (1.0..3.0).contains(&v[1]),
            "y should be metres, got {}",
            v[1]
        );
        assert_eq!(v[2], 0.0);
        assert_eq!(
            &v[3..6],
            &[1.5, 0.0, 0.0],
            "rotation is radians, never scaled"
        );
        assert_eq!(v[6], 2.0, "per-part scale is dimensionless, never scaled");
    }

    #[test]
    fn stat_object_bounds_reach_the_target_scaled() {
        // Interlock for the (now removed) dead `scale_nested` OBND map entry
        // (see the yaml's inline rationale comments): the hook must scale
        // OBND itself, and the map must not drop it out from under the hook.
        let interner = StringInterner::new();
        let translator = fo4_starfield_translator();
        let mut rec = source("STAT", &interner);
        let mut obnd = Vec::new();
        for v in [-70i16, -70, 0, 70, 70, 140] {
            obnd.extend_from_slice(&v.to_le_bytes());
        }
        push_bytes(&mut rec, "OBND", obnd);

        let out = translate(&translator, &interner, rec);

        let obnd_out = out
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "OBND")
            .expect("OBND must survive translation, not be dropped");
        let FieldValue::Bytes(raw) = &obnd_out.value else {
            panic!("OBND should stay raw Bytes");
        };
        let bounds: Vec<i16> = raw
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes(c.try_into().unwrap()))
            .collect();
        // -70/70 * 0.0142875 = -1.000125/1.000125 -> rounds to -1/1;
        // 140 * 0.0142875 = 2.0 -> 2.
        assert_eq!(bounds, vec![-1, -1, 0, 1, 1, 2]);
    }

    #[test]
    fn watr_and_lgtm_visual_payloads_survive_at_starfield_widths() {
        let interner = StringInterner::new();
        let translator = fo4_starfield_translator();

        let mut water = source("WATR", &interner);
        push_bytes(&mut water, "DNAM", vec![0u8; 201]);
        assert_eq!(
            payload_len(&translate(&translator, &interner, water), "DNAM"),
            152
        );

        let mut template = source("LGTM", &interner);
        push_bytes(&mut template, "DATA", vec![0u8; 136]);
        push_bytes(&mut template, "DALC", vec![0u8; 32]);
        let out = translate(&translator, &interner, template);
        assert_eq!(payload_len(&out, "DATA"), 108);
        assert_eq!(payload_len(&out, "DALC"), 24);
    }

    #[test]
    fn weather_records_are_still_skipped_wholesale() {
        let interner = StringInterner::new();
        let translator = fo4_starfield_translator();
        for sig in ["WTHR", "CLMT"] {
            let record = source(sig, &interner);
            assert!(
                matches!(
                    translator.translate(&record, &interner),
                    TranslateResult::Dropped { .. }
                ),
                "{sig} must stay a whole-record skip"
            );
        }
    }

    #[test]
    fn worldspace_leaves_translation_with_a_vanilla_starfield_climate() {
        let interner = StringInterner::new();
        let translator = fo4_starfield_translator();
        let mut rec = source("WRLD", &interner);
        rec.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CNAM").unwrap(),
            value: FieldValue::FormKey(FormKey::parse("00015B@Fallout4.esm", &interner).unwrap()),
        });

        let out = translate(&translator, &interner, rec);

        let cnam = out
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "CNAM")
            .expect("converted worldspaces need a climate");
        let FieldValue::FormKey(fk) = cnam.value else {
            panic!("CNAM should be a FormKey");
        };
        assert_eq!(interner.resolve(fk.plugin), Some("Starfield.esm"));
        assert_eq!(fk.local, 0x0001_5F);
    }
}
