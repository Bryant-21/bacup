//! Semantic Starfield `MGEF.DATA` conversion for the FO4 target layout.
//!
//! Starfield reordered the whole struct (136 bytes against FO4's 152) and the
//! generated Starfield schema has no layout for it, so nothing downstream can
//! move its fields. Like the legacy normalizer it runs in the serial mapper pass,
//! where embedded FormIDs can be resolved.

use crate::formkey_mapper::FormKeyMapper;
use crate::record::{FieldValue, Record};
use crate::sym::Sym;
use crate::translator::pair_hooks::fnv_mgef::{
    FO4_MGEF_DATA_LEN, MgefEnumDecision, MgefNormalizeReport, MgefReferenceDecision,
    MgefReferenceOutcome, byte_value, finite_or_zero, mapped_raw_reference, null_reference,
    push_field, read_f32, read_u32, u16_value, u32_value,
};

pub const STARFIELD_MGEF_DATA_LEN: usize = 136;

const SCRIPT_ARCHETYPE: u32 = 1;
const NORMAL_CASTING_SOUND_LEVEL: u32 = 1;

/// Every flag bit vanilla FO4 sets on an MGEF. Starfield's own set is identical.
const FO4_FLAG_MASK: u32 = 0x1C7A_9F37;

pub fn normalize_starfield_mgef_data(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
) -> MgefNormalizeReport {
    let mut report = MgefNormalizeReport::default();
    if record.sig.0 != *b"MGEF" {
        return report;
    }
    let counter_effects = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"ESCE")
        .count();

    for field in &mut record.fields {
        if field.sig.0 != *b"DATA" {
            continue;
        }
        let source = match &field.value {
            FieldValue::Bytes(bytes) if bytes.len() == STARFIELD_MGEF_DATA_LEN => bytes.to_vec(),
            FieldValue::Bytes(bytes) if bytes.len() == FO4_MGEF_DATA_LEN => {
                report.preserved_target_rows += 1;
                continue;
            }
            _ => {
                report.unsupported_rows += 1;
                continue;
            }
        };
        field.value = convert_data(&source, counter_effects, mapper, &mut report);
        report.converted_rows += 1;
    }
    report
}

fn convert_data(
    source: &[u8],
    counter_effects: usize,
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    let source_archetype = read_u32(source, 80);
    // The mapper resolves a Starfield AVIF only through an EditorID match. The
    // object id alone would be wrong: 0002D5 is Oxygen in Starfield and
    // ActionPoints in FO4.
    let actor_value = mapped_raw_reference("actor_value", read_u32(source, 56), mapper, report);
    let actor_value_1 = mapped_raw_reference("actor_value_1", read_u32(source, 68), mapper, report);
    let resist_value = mapped_raw_reference("resist_value", read_u32(source, 64), mapper, report);

    let archetype = translate_archetype(
        source_archetype,
        !is_null(&actor_value),
        !is_null(&actor_value_1),
    );
    let source_sound_level = read_u32(source, 96);
    let casting_sound_level = translate_sound_level(source_sound_level);
    report.enums.extend([
        MgefEnumDecision {
            field: "archetype",
            source: source_archetype,
            target: archetype,
            used_default: archetype != source_archetype,
        },
        MgefEnumDecision {
            field: "casting_sound_level",
            source: source_sound_level,
            target: casting_sound_level,
            used_default: source_sound_level > 5,
        },
    ]);

    let assoc_item = if archetype_takes_assoc_item(archetype) {
        mapped_raw_reference("assoc_item", read_u32(source, 0), mapper, report)
    } else {
        let source_raw = read_u32(source, 0);
        if source_raw != 0 {
            report.references.push(MgefReferenceDecision {
                field: "assoc_item",
                outcome: MgefReferenceOutcome::DroppedIncompatible { source_raw },
            });
        }
        null_reference()
    };
    let (actor_value, actor_value_1) = match archetype {
        SCRIPT_ARCHETYPE => (null_reference(), null_reference()),
        _ => (actor_value, actor_value_1),
    };

    let mut reference = |field: &'static str, offset: usize, mapper: &mut FormKeyMapper<'_>| {
        mapped_raw_reference(field, read_u32(source, offset), mapper, report)
    };
    let casting_light = reference("casting_light", 48, mapper);
    let hit_shader = reference("hit_shader", 16, mapper);
    let enchant_shader = reference("enchant_shader", 20, mapper);
    let projectile = reference("projectile", 60, mapper);
    let explosion = reference("explosion", 32, mapper);
    let casting_art = reference("casting_art", 8, mapper);
    let hit_effect_art = reference("hit_effect_art", 36, mapper);
    let impact_data = reference("impact_data", 44, mapper);
    let enchant_art = reference("enchant_art", 24, mapper);
    let equip_ability = reference("equip_ability", 28, mapper);
    let image_space_modifier = reference("image_space_modifier", 40, mapper);
    let perk_to_apply = reference("perk_to_apply", 52, mapper);

    let float = |offset: usize| FieldValue::Float(finite_or_zero(read_f32(source, offset)));
    let casting_type = u32::from(source[100]);
    let delivery = u32::from(source[101]);
    let interner = mapper.interner;
    let mut fields: Vec<(Sym, FieldValue)> = Vec::with_capacity(43);
    let mut push = |name: &str, value: FieldValue| push_field(&mut fields, interner, name, value);
    push("flags", u32_value(read_u32(source, 106) & FO4_FLAG_MASK));
    push("base_cost", float(92));
    push("assoc_item", assoc_item);
    for name in [
        "magic_skill_unused_byte_1",
        "magic_skill_unused_byte_2",
        "magic_skill_unused_byte_3",
        "magic_skill_unused_byte_4",
    ] {
        push(name, byte_value(0));
    }
    push("resist_value", resist_value);
    push(
        "counter_effect_count",
        u16_value(u16::try_from(counter_effects).unwrap_or(u16::MAX)),
    );
    push("unknown_u8_9", byte_value(0));
    push("unknown_u8_10", byte_value(0));
    push("casting_light", casting_light);
    push("taper_weight", float(132));
    push("hit_shader", hit_shader);
    push("enchant_shader", enchant_shader);
    push("minimum_skill_level", u32_value(read_u32(source, 110)));
    push(
        "spellmaking_area",
        u32_value(finite_or_zero(read_f32(source, 84)).round().max(0.0) as u32),
    );
    push("spellmaking_casting_time", float(88));
    push("taper_curve", float(124));
    push("taper_duration", float(128));
    push("second_av_weight", float(116));
    push("archetype", u32_value(archetype));
    push("actor_value", actor_value);
    push("projectile", projectile);
    push("explosion", explosion);
    push(
        "casting_type",
        u32_value(if casting_type <= 2 { casting_type } else { 0 }),
    );
    push("delivery", u32_value(if delivery <= 4 { delivery } else { 0 }));
    push("actor_value_1", actor_value_1);
    push("casting_art", casting_art);
    push("hit_effect_art", hit_effect_art);
    push("impact_data", impact_data);
    push("skill_usage_multiplier", float(120));
    push("dual_casting_art", null_reference());
    push("dual_casting_scale", float(102));
    push("enchant_art", enchant_art);
    push("hit_visuals", null_reference());
    push("enchant_visuals", null_reference());
    push("equip_ability", equip_ability);
    push("image_space_modifier", image_space_modifier);
    push("perk_to_apply", perk_to_apply);
    push("casting_sound_level", u32_value(casting_sound_level));
    push("script_effect_ai_score", float(76));
    push("script_effect_ai_delay_time", float(72));

    FieldValue::Struct(fields)
}

fn is_null(value: &FieldValue) -> bool {
    matches!(value, FieldValue::Uint(0))
}

/// FO4 shares Starfield's numbering except where Starfield reassigned a slot
/// (36 is Command there and Add Perk in FO4) or added archetypes FO4 lacks.
/// Value archetypes without a resolvable FO4 actor value become inert Script
/// effects: vanilla FO4 never ships one with a null actor value.
fn translate_archetype(source: u32, has_actor_value: bool, has_second_actor_value: bool) -> u32 {
    match source {
        26 | 36 | 50.. => SCRIPT_ARCHETYPE,
        0 | 4 | 32 | 34 if !has_actor_value => SCRIPT_ARCHETYPE,
        5 if !(has_actor_value && has_second_actor_value) => SCRIPT_ARCHETYPE,
        _ => source,
    }
}

/// Archetypes whose FO4 `assoc_item` union has a live member.
fn archetype_takes_assoc_item(archetype: u32) -> bool {
    matches!(archetype, 12 | 17 | 18 | 25 | 34 | 35 | 39 | 40 | 45 | 46)
}

/// Starfield orders None, Silent, Quiet, Normal, Loud, Very Loud; FO4 orders
/// Loud, Normal, Silent, Very Loud, Quiet.
pub(crate) fn translate_sound_level(source: u32) -> u32 {
    match source {
        0 | 1 => 2,
        2 => 4,
        4 => 0,
        5 => 3,
        _ => NORMAL_CASTING_SOUND_LEVEL,
    }
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::sym::StringInterner;
    use crate::translator::pair_hooks::fnv_mgef::{FO4_MGEF_DATA_LEN, MgefReferenceOutcome};

    fn form_key(interner: &StringInterner, plugin: &str, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    /// The run pre-seeds EditorID+signature matches before Pass A: Starfield
    /// `Health` (0002D4) names FO4 `Health`, while Starfield `Oxygen` (0002D5)
    /// has no FO4 namesake and stays unmapped despite FO4 `ActionPoints` owning
    /// that object id.
    fn mapper(interner: &StringInterner) -> FormKeyMapper<'_> {
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Starfield.esm".into(),
                source_plugin_name: "Starfield.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            interner,
        );
        mapper.add_mapping(
            form_key(interner, "Starfield.esm", 0x2D4),
            form_key(interner, "Fallout4.esm", 0x2D4),
        );
        mapper
    }

    fn record(interner: &StringInterner, data: Vec<u8>, counter_effects: usize) -> Record {
        let mut record = Record::new(SigCode(*b"MGEF"), form_key(interner, "Starfield.esm", 0x40613));
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(SmallVec::from_vec(data)),
        });
        for _ in 0..counter_effects {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"ESCE"),
                value: FieldValue::FormKey(form_key(interner, "Starfield.esm", 0x1234)),
            });
        }
        record
    }

    fn encoded_data(record: &Record) -> Vec<u8> {
        fn encode(value: &FieldValue, output: &mut Vec<u8>) {
            match value {
                FieldValue::Uint(value) => output.extend_from_slice(&(*value as u32).to_le_bytes()),
                FieldValue::Float(value) => output.extend_from_slice(&value.to_le_bytes()),
                FieldValue::Bytes(bytes) => output.extend_from_slice(bytes),
                FieldValue::FormKey(form_key) => {
                    output.extend_from_slice(&form_key.local.to_le_bytes())
                }
                FieldValue::Struct(fields) => {
                    for (_, value) in fields {
                        encode(value, output);
                    }
                }
                other => panic!("unexpected fixture value: {other:?}"),
            }
        }
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

    /// Shipped `Starfield.esm` MGEF 040613 `DamageStimFlameHealth`, the record
    /// FO4 crashed initialising: its Starfield FormIDs landed in `archetype`.
    const DAMAGE_STIM_FLAME_HEALTH: &str = "0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000cb8d20000000000060f21d00000000000000000000000000000000000000000000000000000000000300000002010000803f048000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn damage_stim_flame_health_lands_every_field_in_the_fo4_layout() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let source = hex::decode(DAMAGE_STIM_FLAME_HEALTH).unwrap();
        assert_eq!(source.len(), STARFIELD_MGEF_DATA_LEN);
        let mut record = record(&interner, source, 0);

        let report = normalize_starfield_mgef_data(&mut record, &mut mapper);
        let data = encoded_data(&record);

        assert_eq!(report.converted_rows, 1);
        assert_eq!(data.len(), FO4_MGEF_DATA_LEN);
        assert_eq!(u32_at(&data, 0), 0x0000_8004, "Detrimental | Hide in UI");
        // Value Modifier on the Starfield-only Stim_Health_Flame AV: FO4 has no
        // such AV and never ships a value modifier without one.
        assert_eq!(u32_at(&data, 64), 1, "inert Script archetype");
        assert_eq!(u32_at(&data, 68), 0);
        assert_eq!(u32_at(&data, 16), 0, "Stim_Resist_Flame has no FO4 AV");
        assert_eq!(u32_at(&data, 80), 2, "Concentration");
        assert_eq!(u32_at(&data, 84), 1, "Touch");
        assert_eq!(f32_at(&data, 112), 1.0, "dual casting scale");
        assert_eq!(u32_at(&data, 140), 1, "Starfield Normal is FO4 Normal (1)");
        assert_eq!(u32_at(&data, 100), 0, "no impact data");
        assert_eq!(
            outcome(&report, "actor_value"),
            &MgefReferenceOutcome::DeferredNull {
                source_raw: 0x0020_8DCB
            }
        );
    }

    #[test]
    fn actor_values_resolve_by_editor_id_never_by_shared_form_id() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let mut source = vec![0; STARFIELD_MGEF_DATA_LEN];
        set_u32(&mut source, 56, 0x2D4); // Health in both games
        set_u32(&mut source, 64, 0x2D5); // Starfield Oxygen, FO4 ActionPoints
        let mut record = record(&interner, source, 0);

        normalize_starfield_mgef_data(&mut record, &mut mapper);
        let data = encoded_data(&record);

        assert_eq!(u32_at(&data, 64), 0, "value modifier survives");
        assert_eq!(u32_at(&data, 68), 0x2D4);
        assert_eq!(u32_at(&data, 16), 0, "Oxygen must not become ActionPoints");
    }

    #[test]
    fn archetype_and_sound_level_translate_into_fo4_meanings() {
        let interner = StringInterner::new();
        for (source_archetype, target_archetype) in [(36, 1), (26, 1), (50, 1), (55, 1), (48, 48), (1, 1)] {
            let mut mapper = mapper(&interner);
            let mut source = vec![0; STARFIELD_MGEF_DATA_LEN];
            set_u32(&mut source, 80, source_archetype);
            let mut record = record(&interner, source, 0);
            normalize_starfield_mgef_data(&mut record, &mut mapper);
            assert_eq!(
                u32_at(&encoded_data(&record), 64),
                target_archetype,
                "Starfield archetype {source_archetype}"
            );
        }
        // Starfield: None, Silent, Quiet, Normal, Loud, Very Loud.
        // FO4: Loud, Normal, Silent, Very Loud, Quiet.
        for (source_level, target_level) in [(0, 2), (1, 2), (2, 4), (3, 1), (4, 0), (5, 3), (9, 1)] {
            let mut mapper = mapper(&interner);
            let mut source = vec![0; STARFIELD_MGEF_DATA_LEN];
            set_u32(&mut source, 80, 1);
            set_u32(&mut source, 96, source_level);
            let mut record = record(&interner, source, 0);
            normalize_starfield_mgef_data(&mut record, &mut mapper);
            assert_eq!(u32_at(&encoded_data(&record), 140), target_level);
        }
    }

    #[test]
    fn every_starfield_slot_moves_to_its_fo4_offset() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        for (source_local, target_local) in [
            (0x10_0008, 0x20_0008), // casting art
            (0x10_0010, 0x20_0010), // hit shader
            (0x10_0014, 0x20_0014), // enchant shader
            (0x10_0018, 0x20_0018), // enchant art
            (0x10_001C, 0x20_001C), // equip ability
            (0x10_0020, 0x20_0020), // explosion
            (0x10_0024, 0x20_0024), // hit effect art
            (0x10_0028, 0x20_0028), // image space modifier
            (0x10_002C, 0x20_002C), // impact data
            (0x10_0030, 0x20_0030), // casting light
            (0x10_0034, 0x20_0034), // perk to apply
            (0x10_003C, 0x20_003C), // projectile
            (0x10_0000, 0x20_0000), // assoc item (a KYWD for Peak Value Modifier)
        ] {
            mapper.add_mapping(
                form_key(&interner, "Starfield.esm", source_local),
                form_key(&interner, "Starfield.esm", target_local),
            );
        }
        let mut source = vec![0; STARFIELD_MGEF_DATA_LEN];
        for offset in (0..56).step_by(4).chain([60]) {
            set_u32(&mut source, offset, 0x10_0000 | offset as u32);
        }
        set_u32(&mut source, 4, 0x2D4); // magic skill, unused by FO4
        set_u32(&mut source, 12, 0x10_000C); // Starfield-only MOVT slot
        set_u32(&mut source, 56, 0x2D4);
        set_f32(&mut source, 72, 0.25); // script AI delay
        set_f32(&mut source, 76, 0.5); // script AI score
        set_u32(&mut source, 80, 34); // Peak Value Modifier
        set_f32(&mut source, 84, 3.0); // spellmaking area
        set_f32(&mut source, 88, 1.5); // spellmaking casting time
        set_f32(&mut source, 92, 12.5); // base cost
        set_u32(&mut source, 96, 3);
        source[100] = 1;
        source[101] = 4;
        set_f32(&mut source, 102, 2.0); // dual casting scale
        set_u32(&mut source, 106, 0x1C7A_9F37 | 0x40); // every FO4 flag plus Animate Start/Stop
        set_u32(&mut source, 110, 25); // minimum skill level
        set_f32(&mut source, 116, 0.75); // second AV weight
        set_f32(&mut source, 120, 1.25); // skill usage multiplier
        set_f32(&mut source, 124, 2.5); // taper curve
        set_f32(&mut source, 128, 3.5); // taper duration
        set_f32(&mut source, 132, 4.5); // taper weight
        let mut record = record(&interner, source, 2);

        normalize_starfield_mgef_data(&mut record, &mut mapper);
        let data = encoded_data(&record);

        let own = |local: u32| 0x0100_0000 | local;
        let mut expected = vec![0; FO4_MGEF_DATA_LEN];
        set_u32(&mut expected, 0, 0x1C7A_9F37);
        set_f32(&mut expected, 4, 12.5);
        set_u32(&mut expected, 8, own(0x20_0000));
        expected[20] = 2; // counter effect count follows ESCE
        set_u32(&mut expected, 24, own(0x20_0030));
        set_f32(&mut expected, 28, 4.5);
        set_u32(&mut expected, 32, own(0x20_0010));
        set_u32(&mut expected, 36, own(0x20_0014));
        set_u32(&mut expected, 40, 25);
        set_u32(&mut expected, 44, 3);
        set_f32(&mut expected, 48, 1.5);
        set_f32(&mut expected, 52, 2.5);
        set_f32(&mut expected, 56, 3.5);
        set_f32(&mut expected, 60, 0.75);
        set_u32(&mut expected, 64, 34);
        set_u32(&mut expected, 68, 0x2D4);
        set_u32(&mut expected, 72, own(0x20_003C));
        set_u32(&mut expected, 76, own(0x20_0020));
        set_u32(&mut expected, 80, 1);
        set_u32(&mut expected, 84, 4);
        set_u32(&mut expected, 92, own(0x20_0008));
        set_u32(&mut expected, 96, own(0x20_0024));
        set_u32(&mut expected, 100, own(0x20_002C));
        set_f32(&mut expected, 104, 1.25);
        set_f32(&mut expected, 112, 2.0);
        set_u32(&mut expected, 116, own(0x20_0018));
        set_u32(&mut expected, 128, own(0x20_001C));
        set_u32(&mut expected, 132, own(0x20_0028));
        set_u32(&mut expected, 136, own(0x20_0034));
        set_u32(&mut expected, 140, 1);
        set_f32(&mut expected, 144, 0.5);
        set_f32(&mut expected, 148, 0.25);

        assert_eq!(data, expected);
    }

    #[test]
    fn fo4_sized_and_malformed_rows_are_left_alone() {
        let interner = StringInterner::new();
        for len in [FO4_MGEF_DATA_LEN, 72, 0] {
            let mut mapper = mapper(&interner);
            let source = vec![7; len];
            let mut record = record(&interner, source.clone(), 0);
            let report = normalize_starfield_mgef_data(&mut record, &mut mapper);
            assert_eq!(report.converted_rows, 0);
            assert_eq!(
                record.fields[0].value,
                FieldValue::Bytes(SmallVec::from_vec(source))
            );
        }
    }
}
