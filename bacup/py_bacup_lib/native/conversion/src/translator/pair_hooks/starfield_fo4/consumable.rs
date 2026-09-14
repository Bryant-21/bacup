//! Starfield `ALCH` effect data for the FO4 target layout.

use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::record::{FieldValue, Record};
use crate::translator::pair_hooks::fnv_mgef::{
    MgefNormalizeReport, finite_or_zero, mapped_raw_reference, null_reference, push_field,
    read_f32, read_u32,
};

/// Value, flags, addiction and chance, then a 40-byte WWise sound reference.
const STARFIELD_ENIT_LEN: usize = 56;
const EFIT_LEN: usize = 12;

pub fn normalize_starfield_alch(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
) -> MgefNormalizeReport {
    let mut report = MgefNormalizeReport::default();
    if record.sig.0 != *b"ALCH" {
        return report;
    }

    // Starfield effects may drive magnitude, area and duration from globals and
    // carry item-card flags and an index; FO4 effects are EFID, EFIT and
    // conditions only, and it reads an effect ZNAM as the put-down sound.
    let mut in_effects = false;
    record.fields.retain(|field| match &field.sig.0 {
        b"EFID" => {
            in_effects = true;
            true
        }
        b"MNAM" | b"ANAM" | b"ZNAM" | b"EFIF" | b"MUID" => !in_effects,
        _ => true,
    });

    let interner = mapper.interner;
    for field in &mut record.fields {
        let FieldValue::Bytes(bytes) = &mut field.value else {
            continue;
        };
        match &field.sig.0 {
            b"ENIT" if bytes.len() == STARFIELD_ENIT_LEN => {
                let source = bytes.to_vec();
                let addiction =
                    mapped_raw_reference("addiction", read_u32(&source, 8), mapper, &mut report);
                let mut fields = Vec::with_capacity(5);
                push_field(&mut fields, interner, "value", raw(&source[0..4]));
                push_field(&mut fields, interner, "flags", raw(&source[4..8]));
                push_field(&mut fields, interner, "addiction", addiction);
                push_field(&mut fields, interner, "addiction_chance", raw(&source[12..16]));
                // Starfield consume sounds are WWise events; FO4 has no SNDR for them.
                push_field(&mut fields, interner, "sound_consume", null_reference());
                field.value = FieldValue::Struct(fields);
                report.converted_rows += 1;
            }
            b"EFIT" if bytes.len() == EFIT_LEN => {
                let area = finite_or_zero(read_f32(bytes, 4)).round().max(0.0) as u32;
                bytes[4..8].copy_from_slice(&area.to_le_bytes());
            }
            _ => {}
        }
    }
    report
}

fn raw(bytes: &[u8]) -> FieldValue {
    FieldValue::Bytes(SmallVec::from_slice(bytes))
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::sym::StringInterner;
    use crate::translator::pair_hooks::fnv_mgef::MgefReferenceOutcome;

    fn form_key(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("Starfield.esm"),
        }
    }

    fn mapper(interner: &StringInterner) -> FormKeyMapper<'_> {
        FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Starfield.esm".into(),
                source_plugin_name: "Starfield.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            interner,
        )
    }

    fn bytes(hex: &str) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(hex::decode(hex).unwrap()))
    }

    fn entry(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }
    }

    fn encoded(value: &FieldValue) -> Vec<u8> {
        match value {
            FieldValue::Bytes(bytes) => bytes.to_vec(),
            FieldValue::Uint(value) => (*value as u32).to_le_bytes().to_vec(),
            FieldValue::Float(value) => value.to_le_bytes().to_vec(),
            FieldValue::Struct(fields) => fields.iter().flat_map(|(_, value)| encoded(value)).collect(),
            other => panic!("unexpected fixture value: {other:?}"),
        }
    }

    fn fields<'a>(record: &'a Record, sig: &[u8; 4]) -> Vec<&'a FieldValue> {
        record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *sig)
            .map(|field| &field.value)
            .collect()
    }

    /// Shipped `Starfield.esm` ALCH 002449 `Drink_NonAlc_ToGoCup_Tranquilitea_TeaBlack`,
    /// trimmed to its effect data and first two effects.
    fn tranquilitea(interner: &StringInterner) -> Record {
        let mut record = Record::new(SigCode(*b"ALCH"), form_key(interner, 0x2449));
        record.fields.extend([
            entry(b"DATA", bytes("9a99993e")),
            entry(
                b"ENIT",
                bytes("4b000000030000000000000000000000484e9e0425efc150f0bf12128e5b9682000000000000000000000000000000000000000000000000"),
            ),
            entry(b"EFID", FieldValue::FormKey(form_key(interner, 0x15_0731))),
            entry(b"EFIT", bytes("000000400000000000000000")),
            entry(b"MNAM", FieldValue::FormKey(form_key(interner, 0x2B_9826))),
            entry(b"ZNAM", FieldValue::FormKey(form_key(interner, 0x2B_981A))),
            entry(b"EFIF", FieldValue::Uint(0)),
            entry(b"MUID", FieldValue::Uint(2)),
            entry(b"EFID", FieldValue::FormKey(form_key(interner, 0x2E_E837))),
            entry(b"EFIT", bytes("0000803f0000000001000000")),
            entry(b"CTDA", bytes("600000000000803f4a00f9444c323100000000000000000000000000ffffffff")),
            entry(b"MNAM", FieldValue::FormKey(form_key(interner, 0x2E_FD78))),
            entry(b"ZNAM", FieldValue::FormKey(form_key(interner, 0x2F_0AD4))),
            entry(b"EFIF", FieldValue::Uint(0)),
            entry(b"MUID", FieldValue::Uint(2)),
        ]);
        record
    }

    #[test]
    fn tranquilitea_effect_data_takes_the_fo4_layout_without_a_wwise_sound() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let mut record = tranquilitea(&interner);

        let report = normalize_starfield_alch(&mut record, &mut mapper);

        assert_eq!(report.converted_rows, 1);
        let enit = fields(&record, b"ENIT");
        assert_eq!(enit.len(), 1);
        // value 75, No Auto-Calc | Food Item, no addiction, no chance, no sound.
        assert_eq!(
            encoded(enit[0]),
            hex::decode("4b000000030000000000000000000000" .to_owned() + "00000000").unwrap()
        );
    }

    #[test]
    fn starfield_effect_globals_and_ids_leave_the_effect_list() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let mut record = tranquilitea(&interner);

        normalize_starfield_alch(&mut record, &mut mapper);

        let signatures: Vec<&str> = record.fields.iter().map(|field| field.sig.as_str()).collect();
        assert_eq!(
            signatures,
            ["DATA", "ENIT", "EFID", "EFIT", "EFID", "EFIT", "CTDA"],
            "FO4 reads an effect ZNAM as the put-down sound"
        );
    }

    #[test]
    fn addiction_resolves_through_the_mapper_or_is_nulled() {
        let interner = StringInterner::new();
        let addicted_enit = "4b000000030000003d8d2600" .to_owned() + "3333b33e" + &"00".repeat(40);

        let mut mapped = mapper(&interner);
        mapped.add_mapping(
            form_key(&interner, 0x26_8D3D),
            FormKey {
                local: 0x0012_3456,
                plugin: interner.intern("Fallout4.esm"),
            },
        );
        let mut record = Record::new(SigCode(*b"ALCH"), form_key(&interner, 0x2449));
        record.fields.push(entry(b"ENIT", bytes(&addicted_enit)));
        normalize_starfield_alch(&mut record, &mut mapped);
        assert_eq!(&encoded(fields(&record, b"ENIT")[0])[8..12], &0x0012_3456_u32.to_le_bytes());

        let mut unmapped = mapper(&interner);
        let mut record = Record::new(SigCode(*b"ALCH"), form_key(&interner, 0x2449));
        record.fields.push(entry(b"ENIT", bytes(&addicted_enit)));
        let report = normalize_starfield_alch(&mut record, &mut unmapped);
        let enit = encoded(fields(&record, b"ENIT")[0]);
        assert_eq!(&enit[8..12], &[0; 4]);
        assert_eq!(f32::from_le_bytes(enit[12..16].try_into().unwrap()), 0.35);
        assert!(report.references.iter().any(|decision| decision.field == "addiction"
            && decision.outcome == MgefReferenceOutcome::DeferredNull { source_raw: 0x26_8D3D }));
    }

    #[test]
    fn effect_area_becomes_fo4_whole_units() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let mut record = Record::new(SigCode(*b"ALCH"), form_key(&interner, 0x2449));
        let mut efit = Vec::new();
        efit.extend_from_slice(&5.0_f32.to_le_bytes());
        efit.extend_from_slice(&2.6_f32.to_le_bytes());
        efit.extend_from_slice(&30_u32.to_le_bytes());
        record.fields.push(entry(b"EFIT", FieldValue::Bytes(SmallVec::from_vec(efit))));

        normalize_starfield_alch(&mut record, &mut mapper);

        let efit = encoded(fields(&record, b"EFIT")[0]);
        assert_eq!(f32::from_le_bytes(efit[0..4].try_into().unwrap()), 5.0);
        assert_eq!(u32::from_le_bytes(efit[4..8].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(efit[8..12].try_into().unwrap()), 30);
    }
}
