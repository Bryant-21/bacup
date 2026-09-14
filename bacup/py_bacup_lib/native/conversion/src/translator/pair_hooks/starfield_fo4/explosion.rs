//! Semantic Starfield `EXPL.ENAM` conversion into the FO4 `DATA` layout.
//!
//! Starfield moved the explosion data to ENAM, replaced both SNDR slots with
//! 40-byte WWise sound references and added a condition form and a duration
//! (164 bytes against FO4's 84). FO4 EXPL has no ENAM, so Pass F's target
//! normalizer would drop it; converting it here, in the serial mapper pass, also
//! lets the embedded FormIDs resolve.

use super::STARFIELD_METERS_TO_FO4_UNITS;
use super::magic_effect::translate_sound_level;
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SubrecordSig;
use crate::record::{FieldValue, Record};
use crate::translator::pair_hooks::fnv_mgef::{
    MgefEnumDecision, MgefNormalizeReport, MgefReferenceDecision, MgefReferenceOutcome,
    finite_or_zero, mapped_raw_reference, null_reference, push_field, read_f32, read_u32,
    u32_value,
};

const STARFIELD_EXPL_ENAM_LEN: usize = 164;

/// FO4 and Starfield define the same eleven explosion flags.
const FO4_FLAG_MASK: u32 = 0x7FF;
const EXTRA_LARGE_STAGGER: u32 = 4;

pub(crate) fn normalize(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
) -> MgefNormalizeReport {
    let mut report = MgefNormalizeReport::default();
    if record.sig.0 != *b"EXPL" {
        return report;
    }

    for field in &mut record.fields {
        if field.sig.0 != *b"ENAM" {
            continue;
        }
        let source = match &field.value {
            FieldValue::Bytes(bytes) if bytes.len() == STARFIELD_EXPL_ENAM_LEN => bytes.to_vec(),
            _ => {
                report.unsupported_rows += 1;
                continue;
            }
        };
        field.sig = SubrecordSig(*b"DATA");
        field.value = convert_enam(&source, mapper, &mut report);
        report.converted_rows += 1;
    }
    report
}

fn convert_enam(
    source: &[u8],
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    let mut reference = |field: &'static str, offset: usize, mapper: &mut FormKeyMapper<'_>| {
        mapped_raw_reference(field, read_u32(source, offset), mapper, report)
    };
    let light = reference("light", 0, mapper);
    let impact_data_set = reference("impact_data_set", 84, mapper);
    let placed_object = reference("placed_object", 88, mapper);
    let spawn_projectile = reference("spawn_projectile", 92, mapper);

    let condition_form = read_u32(source, 96);
    if condition_form != 0 {
        report.references.push(MgefReferenceDecision {
            field: "condition_form",
            outcome: MgefReferenceOutcome::DroppedIncompatible {
                source_raw: condition_form,
            },
        });
    }
    let source_sound_level = read_u32(source, 128);
    let sound_level = translate_sound_level(source_sound_level);
    report.enums.push(MgefEnumDecision {
        field: "sound_level",
        source: source_sound_level,
        target: sound_level,
        used_default: source_sound_level > 5,
    });
    let stagger = match read_u32(source, 136) {
        stagger @ 0..=EXTRA_LARGE_STAGGER => stagger,
        _ => 0,
    };

    let float = |offset: usize| FieldValue::Float(finite_or_zero(read_f32(source, offset)));
    let units = |offset: usize| {
        FieldValue::Float(finite_or_zero(
            read_f32(source, offset) * STARFIELD_METERS_TO_FO4_UNITS,
        ))
    };
    let interner = mapper.interner;
    let mut fields = Vec::with_capacity(21);
    let mut push = |name: &str, value: FieldValue| push_field(&mut fields, interner, name, value);
    push("light", light);
    // Both Starfield sounds are WWise event pairs (the second is unused); FO4
    // has no SNDR for them.
    push("sound_1", null_reference());
    push("sound_2", null_reference());
    push("impact_data_set", impact_data_set);
    push("placed_object", placed_object);
    push("spawn_projectile", spawn_projectile);
    push("force", float(100));
    push("damage", float(104));
    push("inner_radius", units(108));
    push("outer_radius", units(112));
    push("is_radius", units(116));
    push("vertical_offset_mult", float(120));
    push("flags", u32_value(read_u32(source, 124) & FO4_FLAG_MASK));
    push("sound_level", u32_value(sound_level));
    push("placed_object_autofade_delay", float(132));
    push("stagger", u32_value(stagger));
    push("spawn_x", float(140));
    push("spawn_y", float(144));
    push("spawn_z", float(148));
    push("spawn_spread_degrees", float(152));
    push("spawn_count", u32_value(read_u32(source, 156)));
    FieldValue::Struct(fields)
}

/// Starfield orders None, Silent, Quiet, Normal, Loud, Very Loud; FO4 orders
/// Loud, Normal, Silent, Very Loud, Quiet.
#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
    use smallvec::SmallVec;
    use smol_str::SmolStr;

    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use crate::translator::pair_hook::PairCtx;
    use crate::translator::pair_hooks::fnv_mgef::MgefReferenceOutcome;
    use crate::translator::{Game, TranslateResult, Translator};

    const FO4_EXPL_DATA_LEN: usize = 84;

    fn form_key(interner: &StringInterner, plugin: &str, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    /// The run maps Starfield onto Fallout4.esm only by EditorID+signature, never
    /// by object id, so an empty mapper leaves every slot unmapped.
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

    fn record(interner: &StringInterner, local: u32, enam: Vec<u8>) -> Record {
        let mut record = Record::new(
            SigCode(*b"EXPL"),
            form_key(interner, "Starfield.esm", local),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ENAM"),
            value: FieldValue::Bytes(SmallVec::from_vec(enam)),
        });
        record
    }

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

    fn encoded_data(record: &Record) -> Vec<u8> {
        assert!(
            record.fields.iter().all(|field| field.sig.0 != *b"ENAM"),
            "FO4 EXPL has no ENAM"
        );
        let data = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DATA")
            .expect("DATA");
        let mut bytes = Vec::new();
        encode(&data.value, &mut bytes);
        bytes
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn f32_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn set_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn set_f32(bytes: &mut [u8], offset: usize, value: f32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn outcome<'a>(report: &'a MgefNormalizeReport, field: &str) -> &'a MgefReferenceOutcome {
        &report
            .references
            .iter()
            .find(|decision| decision.field == field)
            .unwrap_or_else(|| panic!("missing decision for {field}"))
            .outcome
    }

    fn convert(
        interner: &StringInterner,
        mapper: &mut FormKeyMapper<'_>,
        local: u32,
        hex_enam: &str,
    ) -> (MgefNormalizeReport, Vec<u8>) {
        let source = hex::decode(hex_enam).unwrap();
        assert_eq!(source.len(), STARFIELD_EXPL_ENAM_LEN);
        let mut record = record(interner, local, source);
        let report = normalize(&mut record, mapper);
        (report, encoded_data(&record))
    }

    /// Shipped `Starfield.esm` EXPL 02957A `HornetNest_Explosion`: a light, an
    /// impact data set, a WWise sound and four spawned `HornetNestCone_Projectile`s.
    const HORNET_NEST: &str = "65181b00cb485f43a3db4a7ffe6813abd542a28e00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000fdcc15000000000098950200000000000000204100008c42000000400000a0400000204100000000430000000400000000000000010000000000000000000000000080bf000096420400000000002041";
    /// EXPL 19B894 `TerrormorphAcidSplashSmallExplosion`: places the HAZD
    /// `Terrormorph_ENV_CloudHazard_Spores_Toxic` and fades it after 10 s.
    const TERRORMORPH_ACID_SPLASH: &str = "00000000a24124a0329d9dbcaff1a64735bdfcb300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000007a0b000000000000000000000000803f0000803f0000003f0000004000000000cdcccc3d4204000001000000000020410000000000000000000000000000000000000000000000000000803f";
    /// EXPL 09114E `StarbornWeapon_SupernovaExplosion`: gated by the CNDF
    /// `IsNotInTargetSpaceship`, with space-scale radii.
    const STARBORN_SUPERNOVA: &str = "998f0d007e475d2490e8ed0e7fb49ab74829a08e00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000fd7a1900000016430000484300007a4300007a4400409c45cdcccc3d5300000004000000000000000200000000000000000000000000803f000034430000000000002041";

    /// FO4 explosion radii are game units, unlike its hazard radii: vanilla
    /// `fragGrenadeExplosion` has inner 150, outer 500 and IS radius 1000, and
    /// Starfield retuned the same EditorID to 3.5, 8 and 50 m (245, 560 and 3500
    /// units); `REDeathExplosion` went from outer 64 to 2 m (140 units).
    #[test]
    fn hornet_nest_lands_every_field_in_the_fo4_layout() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);

        let (report, data) = convert(&interner, &mut mapper, 0x02957A, HORNET_NEST);

        let mut expected = vec![0; FO4_EXPL_DATA_LEN];
        set_f32(&mut expected, 24, 10.0); // force
        set_f32(&mut expected, 28, 70.0); // damage
        set_f32(&mut expected, 32, 2.0 * STARFIELD_METERS_TO_FO4_UNITS);
        set_f32(&mut expected, 36, 5.0 * STARFIELD_METERS_TO_FO4_UNITS);
        set_f32(&mut expected, 40, 10.0 * STARFIELD_METERS_TO_FO4_UNITS);
        set_u32(&mut expected, 48, 0x43); // Unknown 0 | World Orientation | Chain
        set_u32(&mut expected, 52, 0); // Starfield Loud is FO4 Loud (0)
        set_u32(&mut expected, 60, 1); // Small stagger
        set_f32(&mut expected, 72, -1.0); // spawn z
        set_f32(&mut expected, 76, 75.0); // spread degrees
        set_u32(&mut expected, 80, 4); // spawn count

        assert_eq!(report.converted_rows, 1);
        assert_eq!(data, expected);
        assert_eq!(
            outcome(&report, "light"),
            &MgefReferenceOutcome::DeferredNull {
                source_raw: 0x001B_1865
            }
        );
        assert_eq!(
            outcome(&report, "impact_data_set"),
            &MgefReferenceOutcome::DeferredNull {
                source_raw: 0x0015_CCFD
            }
        );
        assert_eq!(
            outcome(&report, "spawn_projectile"),
            &MgefReferenceOutcome::DeferredNull {
                source_raw: 0x0002_9598
            }
        );
    }

    #[test]
    fn hornet_nest_references_resolve_through_the_mapper() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        for (source_local, target_local) in [
            (0x1B_1865, 0x0F_4B7A), // light
            (0x15_CCFD, 0x04_BB6A), // impact data set
        ] {
            mapper.add_mapping(
                form_key(&interner, "Starfield.esm", source_local),
                form_key(&interner, "Fallout4.esm", target_local),
            );
        }
        mapper.add_mapping(
            form_key(&interner, "Starfield.esm", 0x02_9598),
            form_key(&interner, "Starfield.esm", 0x02_9598),
        );

        let (_, data) = convert(&interner, &mut mapper, 0x02957A, HORNET_NEST);

        assert_eq!(u32_at(&data, 0), 0x000F_4B7A, "light");
        assert_eq!(u32_at(&data, 4), 0, "WWise sound has no FO4 SNDR");
        assert_eq!(u32_at(&data, 8), 0, "Starfield's second sound is unused");
        assert_eq!(u32_at(&data, 12), 0x0004_BB6A, "impact data set");
        assert_eq!(u32_at(&data, 16), 0, "no placed object");
        assert_eq!(u32_at(&data, 20), 0x0102_9598, "converted projectile");
    }

    #[test]
    fn terrormorph_splash_keeps_its_placed_hazard_fade_and_silent_level() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        mapper.add_mapping(
            form_key(&interner, "Starfield.esm", 0x000B7A),
            form_key(&interner, "Starfield.esm", 0x000B7A),
        );

        let (_, data) = convert(&interner, &mut mapper, 0x19B894, TERRORMORPH_ACID_SPLASH);

        assert_eq!(u32_at(&data, 16), 0x0100_0B7A, "placed HAZD");
        assert_eq!(f32_at(&data, 24), 1.0, "force");
        assert_eq!(f32_at(&data, 32), 0.5 * STARFIELD_METERS_TO_FO4_UNITS);
        assert_eq!(f32_at(&data, 36), 2.0 * STARFIELD_METERS_TO_FO4_UNITS);
        assert_eq!(f32_at(&data, 44), 0.1, "vertical offset mult");
        assert_eq!(
            u32_at(&data, 48),
            0x442,
            "World Orientation | Ignore Image Space Swap | Skip Underwater Tests"
        );
        assert_eq!(u32_at(&data, 52), 2, "Starfield Silent is FO4 Silent (2)");
        assert_eq!(f32_at(&data, 56), 10.0, "placed object autofade delay");
    }

    #[test]
    fn supernova_condition_form_and_duration_have_no_fo4_slot() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);

        let (report, data) = convert(&interner, &mut mapper, 0x09114E, STARBORN_SUPERNOVA);

        assert_eq!(data.len(), FO4_EXPL_DATA_LEN);
        assert_eq!(
            outcome(&report, "condition_form"),
            &MgefReferenceOutcome::DroppedIncompatible {
                source_raw: 0x0019_7AFD
            }
        );
        assert!(
            data.chunks_exact(4)
                .all(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()) != 0x0019_7AFD),
            "the CNDF must not land in a FO4 slot"
        );
        assert_eq!(f32_at(&data, 24), 150.0, "force");
        assert_eq!(f32_at(&data, 28), 200.0, "damage");
        assert_eq!(f32_at(&data, 32), 250.0 * STARFIELD_METERS_TO_FO4_UNITS);
        assert_eq!(f32_at(&data, 36), 1000.0 * STARFIELD_METERS_TO_FO4_UNITS);
        assert_eq!(f32_at(&data, 40), 5000.0 * STARFIELD_METERS_TO_FO4_UNITS);
        assert_eq!(u32_at(&data, 60), 2, "Medium stagger");
        assert_eq!(f32_at(&data, 72), 1.0, "spawn z");
        assert_eq!(f32_at(&data, 76), 180.0, "spread degrees");
        assert_eq!(u32_at(&data, 80), 0, "spawn count, not the 10 s duration");
    }

    /// Thirteen `Starfield.esm` explosions still share an EditorID with
    /// `Fallout4.esm`, and every sound level lines up by name: Starfield Normal (3)
    /// is FO4 1 on eight of them, Loud (4) is 0 on three, Very Loud (5) is 3 on two
    /// and Silent (1) is 2 on `REDeathExplosion`.
    #[test]
    fn flags_stagger_and_sound_level_translate_into_fo4_meanings() {
        let interner = StringInterner::new();
        for (source_level, target_level) in [(0, 2), (1, 2), (2, 4), (3, 1), (4, 0), (5, 3), (9, 1)]
        {
            let mut mapper = mapper(&interner);
            let mut source = hex::decode(HORNET_NEST).unwrap();
            set_u32(&mut source, 128, source_level);
            let mut record = record(&interner, 0x02957A, source);
            normalize(&mut record, &mut mapper);
            assert_eq!(
                u32_at(&encoded_data(&record), 52),
                target_level,
                "Starfield sound level {source_level}"
            );
        }

        let mut mapper = mapper(&interner);
        let mut source = hex::decode(HORNET_NEST).unwrap();
        set_u32(&mut source, 124, u32::MAX);
        set_u32(&mut source, 136, 5);
        let mut record = record(&interner, 0x02957A, source);
        normalize(&mut record, &mut mapper);
        let data = encoded_data(&record);
        assert_eq!(u32_at(&data, 48), 0x7FF, "only the eleven shared flags");
        assert_eq!(u32_at(&data, 60), 0, "stagger past Extra Large");
    }

    #[test]
    fn malformed_enam_rows_are_left_alone() {
        let interner = StringInterner::new();
        for len in [FO4_EXPL_DATA_LEN, 160, 0] {
            let mut mapper = mapper(&interner);
            let source = vec![7; len];
            let mut record = record(&interner, 0x02957A, source.clone());
            let report = normalize(&mut record, &mut mapper);
            assert_eq!(report.converted_rows, 0);
            assert_eq!(report.unsupported_rows, 1);
            assert_eq!(record.fields[0].sig, SubrecordSig(*b"ENAM"));
            assert_eq!(
                record.fields[0].value,
                FieldValue::Bytes(SmallVec::from_vec(source))
            );
        }
    }

    fn parsed_subrecord(signature: &'static str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new_static(signature),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    /// ENAM reaches the serial pass: the Starfield decoder keeps struct codecs as
    /// raw bytes and translation keeps unmapped signatures. Only Pass F's target
    /// normalizer drops it, because FO4 EXPL has no ENAM.
    #[test]
    fn starfield_enam_reaches_the_serial_pass_and_leaves_as_fo4_data() {
        let interner = StringInterner::new();
        let source_schema = AuthoringSchema::for_game("starfield").expect("starfield schema");
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let enam = hex::decode(STARBORN_SUPERNOVA).unwrap();
        // Shipped EXPL 09114E, without its particle-system component body.
        let raw_record = ParsedRecord {
            signature: SmolStr::new_static("EXPL"),
            form_id: 0x0009_114E,
            flags: 0,
            version_control: 0,
            form_version: Some(581),
            version2: Some(7),
            subrecords: vec![
                parsed_subrecord("EDID", b"StarbornWeapon_SupernovaExplosion\0".to_vec()),
                parsed_subrecord("BFCB", b"ParticleSystem_Component\0".to_vec()),
                parsed_subrecord("BFCE", Vec::new()),
                parsed_subrecord(
                    "MODL",
                    b"Effects\\Explosions\\FXEmptyExplosion_Space.nif\0".to_vec(),
                ),
                parsed_subrecord("FLLD", hex::decode("01000000").unwrap()),
                parsed_subrecord("EITM", hex::decode("56110900").unwrap()),
                parsed_subrecord("MNAM", hex::decode("7db01500").unwrap()),
                parsed_subrecord("ENAM", enam.clone()),
                parsed_subrecord("DAMA", hex::decode("e8ed0100c800000000000000").unwrap()),
            ],
            raw_payload: None,
            parse_error: None,
        };
        let source_fk = form_key(&interner, "Starfield.esm", 0x09114E);
        let mut decoded = crate::source_read::decode_record_from_parsed(
            &raw_record,
            &source_fk,
            &source_schema,
            &[],
            "Starfield.esm",
            None,
            false,
            &interner,
        )
        .unwrap();
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        translator
            .pre_translate(&mut PairCtx::new(&interner), &mut decoded)
            .unwrap();
        let TranslateResult::Translated(mut translated) = translator.translate(&decoded, &interner)
        else {
            panic!("EXPL should translate");
        };
        let serial_input = translated
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"ENAM")
            .expect("ENAM survives until the serial pass");
        assert_eq!(
            serial_input.value,
            FieldValue::Bytes(SmallVec::from_vec(enam))
        );

        let mut mapper = mapper(&interner);
        normalize(&mut translated, &mut mapper);
        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def("EXPL"),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(written) = normalizer.normalize(translated) else {
            panic!("FO4 keeps EXPL");
        };

        let signatures: Vec<&str> = written
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert!(signatures.contains(&"DATA"), "{signatures:?}");
        assert!(!signatures.contains(&"ENAM") && !signatures.contains(&"DAMA"));
        let data = encoded_data(&written);
        assert_eq!(
            data.len(),
            FO4_EXPL_DATA_LEN,
            "every field id matches the FO4 schema"
        );
        assert_eq!(f32_at(&data, 36), 1000.0 * STARFIELD_METERS_TO_FO4_UNITS);
        assert_eq!(u32_at(&data, 48), 0x53);
        assert_eq!(u32_at(&data, 52), 0, "Loud");
        assert_eq!(f32_at(&data, 76), 180.0);
    }
}
