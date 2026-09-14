//! Semantic Starfield `HAZD.DNAM` conversion for the FO4 target layout.
//!
//! Starfield leads with a 40-byte WWise sound reference and moves limit and
//! flags to the end (92 bytes against FO4's 52). Runs in the serial mapper pass
//! so the effect, light and impact data set FormIDs can be resolved.

use super::STARFIELD_METERS_TO_FO4_UNITS;
use crate::formkey_mapper::FormKeyMapper;
use crate::record::{FieldValue, Record};
use crate::translator::pair_hooks::fnv_mgef::{
    MgefNormalizeReport, finite_or_zero, mapped_raw_reference, null_reference, push_field,
    read_f32, read_u32, u32_value,
};

const STARFIELD_HAZD_DNAM_LEN: usize = 92;
const FO4_HAZD_DNAM_LEN: usize = 52;

/// FO4 hazard distances are feet of 64/3 units, unlike its placed-ref units.
const STARFIELD_METERS_TO_FO4_FEET: f32 = STARFIELD_METERS_TO_FO4_UNITS * 3.0 / 64.0;

/// Starfield shares FO4's six hazard flags and adds Gravity Point Source (0x40)
/// and Gravity Override (0x80).
const FO4_FLAG_MASK: u32 = 0x3F;

pub(crate) fn normalize(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
) -> MgefNormalizeReport {
    let mut report = MgefNormalizeReport::default();
    if record.sig.0 != *b"HAZD" {
        return report;
    }

    for field in &mut record.fields {
        if field.sig.0 != *b"DNAM" {
            continue;
        }
        let source = match &field.value {
            FieldValue::Bytes(bytes) if bytes.len() == STARFIELD_HAZD_DNAM_LEN => bytes.to_vec(),
            FieldValue::Bytes(bytes) if bytes.len() == FO4_HAZD_DNAM_LEN => {
                report.preserved_target_rows += 1;
                continue;
            }
            _ => {
                report.unsupported_rows += 1;
                continue;
            }
        };
        field.value = convert_dnam(&source, mapper, &mut report);
        report.converted_rows += 1;
    }
    report
}

fn convert_dnam(
    source: &[u8],
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    let effect = mapped_raw_reference("effect", read_u32(source, 40), mapper, report);
    let light = mapped_raw_reference("light", read_u32(source, 44), mapper, report);
    let impact_data_set =
        mapped_raw_reference("impact_data_set", read_u32(source, 48), mapper, report);

    let float = |offset: usize| FieldValue::Float(finite_or_zero(read_f32(source, offset)));
    let feet = |offset: usize| {
        FieldValue::Float(finite_or_zero(
            read_f32(source, offset) * STARFIELD_METERS_TO_FO4_FEET,
        ))
    };
    let interner = mapper.interner;
    let mut fields = Vec::with_capacity(13);
    let mut push = |name: &str, value: FieldValue| push_field(&mut fields, interner, name, value);
    push("limit", u32_value(read_u32(source, 84)));
    push("radius", feet(52));
    push("lifetime", float(56));
    push("image_space_radius", feet(60));
    push("target_interval", float(64));
    push("flags", u32_value(read_u32(source, 88) & FO4_FLAG_MASK));
    push("effect", effect);
    push("light", light);
    push("impact_data_set", impact_data_set);
    // The Starfield sound is a WWise event pair (plus CNDF/WWED forms); FO4 has no SNDR for it.
    push("sound", null_reference());
    push("taper_effectiveness_full_effect_radius", feet(68));
    push("taper_effectiveness_taper_weight", float(72));
    push("taper_effectiveness_taper_curse", float(76));
    FieldValue::Struct(fields)
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

    fn form_key(interner: &StringInterner, plugin: &str, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    /// No Starfield hazard effect or light has an FO4 EditorID namesake, so the
    /// run's pre-seeded EditorID+signature matches leave every slot unmapped.
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

    fn record(interner: &StringInterner, local: u32, dnam: Vec<u8>) -> Record {
        let mut record = Record::new(
            SigCode(*b"HAZD"),
            form_key(interner, "Starfield.esm", local),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(SmallVec::from_vec(dnam)),
        });
        record
    }

    fn encoded_dnam(record: &Record) -> Vec<u8> {
        fn encode(value: &FieldValue, output: &mut Vec<u8>) {
            match value {
                FieldValue::Uint(value) => output.extend_from_slice(&(*value as u32).to_le_bytes()),
                FieldValue::Float(value) => output.extend_from_slice(&value.to_le_bytes()),
                FieldValue::Bytes(bytes) => output.extend_from_slice(bytes),
                FieldValue::Struct(fields) => {
                    for (_, value) in fields {
                        encode(value, output);
                    }
                }
                other => panic!("unexpected fixture value: {other:?}"),
            }
        }
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DNAM")
            .expect("DNAM");
        let mut bytes = Vec::new();
        encode(&dnam.value, &mut bytes);
        bytes
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn f32_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn assert_feet(bytes: &[u8], offset: usize, expected: f32, what: &str) {
        let actual = f32_at(bytes, offset);
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what}: {actual} ft, expected {expected}"
        );
    }

    fn outcome<'a>(report: &'a MgefNormalizeReport, field: &str) -> &'a MgefReferenceOutcome {
        &report
            .references
            .iter()
            .find(|decision| decision.field == field)
            .unwrap_or_else(|| panic!("missing decision for {field}"))
            .outcome
    }

    /// Shipped `Starfield.esm` HAZD 0496C2 `TestEngineFireHazard`: the only
    /// vanilla hazard with a limit and an image space radius.
    const TEST_ENGINE_FIRE: &str = "0000000000000000000000000000000000000000000000000000000000000000000000000000000027883b000000000000000000000000400000c04000000040cdcc4c3e000080400000003f000000000000803f0a00000036000000";
    /// HAZD 000B25 `ENV_PoolHazard_ParasiticContamination`.
    const PARASITIC_CONTAMINATION: &str = "00000000000000000000000000000000000000000000000000000000000000000000000000000000240b000000000000000000000000204000000000000000009a99993e0000803f0000803f0000803f0000803f0000000020000000";
    /// HAZD 124F3F `Legendary_Wep_TeslaPylons_Hazard`, with a live WWise sound.
    const TESLA_PYLONS: &str = "b0488e0b51d2e43900bb174ae24458bb7446e7d6466bf7006510a9dc2f162e8e0000000000000000424f12000000000000000000000000400000c040000000000000803f000000000000000000000000000000000000000004000000";
    /// HAZD 12547C `LC165_ScriptedGravWellHazard`.
    const GRAV_WELL: &str = "414f1255ca4c698276795ec70ee034beb94fa26a2adc34ccb33dcef4e3f6f28d00000000000000000000000000000000000000000000204100002041000000000000803f000000000000000000000000000000400000000040000000";
    /// HAZD 21C4F9 `IncendiaryGrenade_Hazard`: an ENCH effect and a light.
    const INCENDIARY_GRENADE: &str = "5b4da04800c4ef6aadb4e23735a62787ac444ea26267f523232962946f44d387000000000000000027883b00e3983700000000000000904000004041000000000000803f00000000000000000000000000000000000000001a000000";

    fn convert(
        interner: &StringInterner,
        mapper: &mut FormKeyMapper<'_>,
        local: u32,
        hex_dnam: &str,
    ) -> (MgefNormalizeReport, Vec<u8>) {
        let source = hex::decode(hex_dnam).unwrap();
        assert_eq!(source.len(), STARFIELD_HAZD_DNAM_LEN);
        let mut record = record(interner, local, source);
        let report = normalize(&mut record, mapper);
        (report, encoded_dnam(&record))
    }

    #[test]
    fn test_engine_fire_lands_every_field_in_the_fo4_layout() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);

        let (report, data) = convert(&interner, &mut mapper, 0x0496C2, TEST_ENGINE_FIRE);

        assert_eq!(report.converted_rows, 1);
        assert_eq!(data.len(), FO4_HAZD_DNAM_LEN);
        assert_eq!(u32_at(&data, 0), 10, "limit");
        assert_feet(&data, 4, 6.5617, "2 m radius");
        assert_eq!(f32_at(&data, 8), 6.0, "lifetime");
        assert_feet(&data, 12, 6.5617, "2 m image space radius");
        assert_eq!(f32_at(&data, 16), 0.2, "target interval");
        assert_eq!(
            u32_at(&data, 20),
            0x36,
            "Inherit Duration | Align to Impact Normal | Drop to Ground | Taper"
        );
        assert_eq!(
            u32_at(&data, 24),
            0,
            "Ench_IncendiaryGrenade has no FO4 namesake"
        );
        assert_eq!(u32_at(&data, 28), 0, "light");
        assert_eq!(u32_at(&data, 32), 0, "impact data set");
        assert_eq!(u32_at(&data, 36), 0, "sound");
        assert_feet(&data, 40, 13.1234, "4 m full effect radius");
        assert_eq!(f32_at(&data, 44), 0.5, "taper weight");
        assert_eq!(f32_at(&data, 48), 0.0, "taper curve");
        assert_eq!(
            outcome(&report, "effect"),
            &MgefReferenceOutcome::DeferredNull {
                source_raw: 0x003B_8827
            }
        );
    }

    /// FO4 hazard distances are feet of 64/3 units: vanilla
    /// `RadiationHazardStrong512` has radius 24 and a 517-unit marker sphere,
    /// `RadiationHazardStrong1024` radius 48 and a 1025-unit one. Starfield's are
    /// metres (`SpaceDistortionCloudHazard` is 500 with `SpaceMarkerSphere_500m`).
    #[test]
    fn parasitic_pool_distances_become_fo4_feet() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);

        let (_, data) = convert(&interner, &mut mapper, 0x000B25, PARASITIC_CONTAMINATION);

        assert_feet(&data, 4, 8.2021, "2.5 m radius");
        assert_feet(&data, 40, 3.2808, "1 m full effect radius");
        assert_eq!(f32_at(&data, 16), 0.3, "target interval");
        assert_eq!(f32_at(&data, 44), 1.0, "taper weight");
        assert_eq!(f32_at(&data, 48), 1.0, "taper curve");
        assert_eq!(u32_at(&data, 20), 0x20, "Taper Effectiveness by Proximity");
    }

    #[test]
    fn tesla_pylons_wwise_sound_does_not_reach_the_fo4_struct() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);

        let (report, data) = convert(&interner, &mut mapper, 0x124F3F, TESLA_PYLONS);

        assert_eq!(data.len(), FO4_HAZD_DNAM_LEN);
        assert_eq!(u32_at(&data, 0), 0, "limit is not GUID bytes");
        assert_feet(&data, 4, 6.5617, "2 m radius");
        assert_eq!(f32_at(&data, 8), 6.0, "lifetime");
        assert_eq!(f32_at(&data, 16), 1.0, "target interval");
        assert_eq!(u32_at(&data, 20), 0x04, "Align to Impact Normal");
        assert_eq!(u32_at(&data, 36), 0, "WWise events have no FO4 SNDR");
        assert_eq!(
            outcome(&report, "effect"),
            &MgefReferenceOutcome::DeferredNull {
                source_raw: 0x0012_4F42
            }
        );
    }

    #[test]
    fn grav_well_loses_starfield_only_gravity_flags() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);

        let (_, data) = convert(&interner, &mut mapper, 0x12547C, GRAV_WELL);

        assert_eq!(u32_at(&data, 20), 0, "Gravity Point Source has no FO4 bit");
        assert_feet(&data, 4, 32.8084, "10 m radius");
        assert_eq!(f32_at(&data, 8), 10.0, "lifetime");
        assert!(
            data.chunks_exact(4)
                .all(|chunk| chunk != 2.0_f32.to_le_bytes()),
            "gravity strength 2.0 has no FO4 slot"
        );
    }

    #[test]
    fn effect_light_and_impact_data_resolve_through_the_mapper() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        for (source_local, target_local) in [
            (0x3B_8827, 0x23_C9C2), // effect
            (0x37_98E3, 0x15_AE5B), // light
            (0x10_0030, 0x24_61BB), // impact data set
        ] {
            mapper.add_mapping(
                form_key(&interner, "Starfield.esm", source_local),
                form_key(&interner, "Fallout4.esm", target_local),
            );
        }
        let mut source = hex::decode(INCENDIARY_GRENADE).unwrap();
        source[48..52].copy_from_slice(&0x10_0030_u32.to_le_bytes());
        let mut record = record(&interner, 0x21C4F9, source);

        normalize(&mut record, &mut mapper);
        let data = encoded_dnam(&record);

        assert_eq!(u32_at(&data, 24), 0x23_C9C2);
        assert_eq!(u32_at(&data, 28), 0x15_AE5B);
        assert_eq!(u32_at(&data, 32), 0x24_61BB);
        assert_eq!(u32_at(&data, 36), 0);
        assert_eq!(
            u32_at(&data, 20),
            0x1A,
            "Inherit Duration | Inherit Radius | Drop to Ground"
        );
        assert_feet(&data, 4, 14.7638, "4.5 m radius");
        assert_eq!(f32_at(&data, 8), 12.0, "lifetime");
    }

    #[test]
    fn fo4_sized_and_malformed_rows_are_left_alone() {
        let interner = StringInterner::new();
        for len in [FO4_HAZD_DNAM_LEN, 40, 0] {
            let mut mapper = mapper(&interner);
            let source = vec![7; len];
            let mut record = record(&interner, 0x0496C2, source.clone());
            let report = normalize(&mut record, &mut mapper);
            assert_eq!(report.converted_rows, 0);
            assert_eq!(
                record.fields[0].value,
                FieldValue::Bytes(SmallVec::from_vec(source))
            );
        }
    }
}
