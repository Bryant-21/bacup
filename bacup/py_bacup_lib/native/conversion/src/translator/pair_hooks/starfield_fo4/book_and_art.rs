//! Starfield `BOOK`, `DMGT` and `ARTO` data for the FO4 target layouts.

use smallvec::SmallVec;

use crate::formkey_mapper::FormKeyMapper;
use crate::record::{FieldValue, Record};
use crate::translator::pair_hooks::fnv_mgef::{
    MgefEnumDecision, MgefNormalizeReport, MgefReferenceDecision, MgefReferenceOutcome, byte_value,
    mapped_raw_reference, null_reference, push_field, read_u32, u32_value,
};

/// Flags, teaches, text offset X and Y, then a data slate type FO4 lacks.
const STARFIELD_BOOK_DNAM_LEN: usize = 17;
/// One resistance actor value and spell; FO4 holds an array of these rows.
const STARFIELD_DMGT_DNAM_LEN: usize = 8;
/// Art type, then flags that FO4 keeps on `RFCT` instead.
const STARFIELD_ARTO_DNAM_LEN: usize = 8;

/// Advance Actor Value, Add Spell and Add Perk select the teaches union.
const TEACH_FLAGS: u8 = 0x01 | 0x04 | 0x10;
/// FO4 defines bits 0x01-0x10; Starfield adds 0x20.
const FO4_BOOK_FLAG_MASK: u8 = 0x1F;

const MAGIC_HIT_EFFECT: u32 = 1;

pub(crate) fn normalize(record: &mut Record, mapper: &mut FormKeyMapper<'_>) -> MgefNormalizeReport {
    let mut report = MgefNormalizeReport::default();
    let sig = record.sig.0;
    if !matches!(&sig, b"BOOK" | b"DMGT" | b"ARTO") {
        return report;
    }
    for field in &mut record.fields {
        if field.sig.0 != *b"DNAM" {
            continue;
        }
        let converted = match (&sig, &field.value) {
            (b"BOOK", FieldValue::Bytes(source)) if source.len() == STARFIELD_BOOK_DNAM_LEN => {
                convert_book(source, mapper, &mut report)
            }
            (b"DMGT", FieldValue::Bytes(source)) if source.len() == STARFIELD_DMGT_DNAM_LEN => {
                convert_damage_type(source, mapper, &mut report)
            }
            (b"ARTO", FieldValue::Bytes(source)) if source.len() == STARFIELD_ARTO_DNAM_LEN => {
                convert_art_type(read_u32(source, 0), &mut report)
            }
            _ => {
                report.unsupported_rows += 1;
                continue;
            }
        };
        field.value = converted;
        report.converted_rows += 1;
    }
    report
}

fn convert_book(
    source: &[u8],
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    let mut flags = source[0] & FO4_BOOK_FLAG_MASK;
    let source_teaches = read_u32(source, 1);
    let teaches = if flags & TEACH_FLAGS != 0 {
        let teaches = mapped_raw_reference("teaches", source_teaches, mapper, report);
        // Starfield magazines teach Starfield-only perks; a teach flag left over
        // a null target would have FO4 teach nothing.
        if matches!(teaches, FieldValue::Uint(0)) {
            flags &= !TEACH_FLAGS;
        }
        teaches
    } else {
        if source_teaches != 0 {
            report.references.push(MgefReferenceDecision {
                field: "teaches",
                outcome: MgefReferenceOutcome::DroppedIncompatible {
                    source_raw: source_teaches,
                },
            });
        }
        null_reference()
    };

    let interner = mapper.interner;
    let mut fields = Vec::with_capacity(4);
    push_field(&mut fields, interner, "flags", byte_value(flags));
    push_field(&mut fields, interner, "teaches", teaches);
    push_field(&mut fields, interner, "text_offset_x", raw(&source[5..9]));
    push_field(&mut fields, interner, "text_offset_y", raw(&source[9..13]));
    FieldValue::Struct(fields)
}

fn convert_damage_type(
    source: &[u8],
    mapper: &mut FormKeyMapper<'_>,
    report: &mut MgefNormalizeReport,
) -> FieldValue {
    // The output record is written at form version 131, where FO4 reads
    // FormID rows rather than the pre-78 actor value indexes.
    let actor_value = mapped_raw_reference("actor_value", read_u32(source, 0), mapper, report);
    let spell = mapped_raw_reference("spell", read_u32(source, 4), mapper, report);
    let interner = mapper.interner;
    let mut row = Vec::with_capacity(2);
    push_field(&mut row, interner, "actor_value", actor_value);
    push_field(&mut row, interner, "spell", spell);
    FieldValue::List(vec![FieldValue::Struct(row)])
}

/// FO4 has no Misc Art Object type. Starfield attaches its misc art to
/// references (NPC grav jumps, a magic hit effect), which vanilla FO4 art does
/// as Magic Hit Effect: every MGEF hit effect art and 50 of 53 RFCT arts.
fn convert_art_type(source: u32, report: &mut MgefNormalizeReport) -> FieldValue {
    let target = if source <= 2 { source } else { MAGIC_HIT_EFFECT };
    report.enums.push(MgefEnumDecision {
        field: "art_type",
        source,
        target,
        used_default: target != source,
    });
    u32_value(target)
}

fn raw(bytes: &[u8]) -> FieldValue {
    FieldValue::Bytes(SmallVec::from_slice(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use crate::target_write::encode_field_pub;

    fn form_key(interner: &StringInterner, plugin: &str, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    /// The run pre-seeds EditorID+signature matches before Pass A. Starfield
    /// `EnergyResist` (0002EB) names FO4 `EnergyResist`; Starfield
    /// `FireResist_DONOTUSE` (0002E5) has no FO4 namesake and stays unmapped
    /// even though FO4 `FireResist` owns that object id.
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
            form_key(interner, "Starfield.esm", 0x2EB),
            form_key(interner, "Fallout4.esm", 0x2EB),
        );
        mapper
    }

    fn record(interner: &StringInterner, sig: &[u8; 4], local: u32, dnam: &str) -> Record {
        let mut record = Record::new(SigCode(*sig), form_key(interner, "Starfield.esm", local));
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(SmallVec::from_vec(hex::decode(dnam).unwrap())),
        });
        record
    }

    /// DNAM as the FO4 target writer encodes it.
    fn written_dnam(record: &Record, interner: &StringInterner) -> String {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"DNAM")
            .expect("DNAM");
        hex::encode(encode_field_pub(dnam, schema.record_def(record.sig.as_str()), interner).unwrap())
    }

    fn outcome<'a>(report: &'a MgefNormalizeReport, field: &str) -> &'a MgefReferenceOutcome {
        &report
            .references
            .iter()
            .find(|decision| decision.field == field)
            .unwrap_or_else(|| panic!("missing decision for {field}"))
            .outcome
    }

    /// Shipped `Starfield.esm` BOOK 1FF722 `Skill_Magazine_TheNewAtlantian05`:
    /// Add Perk plus Starfield's 0x20, teaching `MagPerk_TheNewAtlantian5`.
    const NEW_ATLANTIAN_05: &str = "30460c1f00000000000000000000000000";

    #[test]
    fn magazine_without_an_fo4_perk_becomes_a_plain_book() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        let mut book = record(&interner, b"BOOK", 0x1F_F722, NEW_ATLANTIAN_05);

        let report = normalize(&mut book, &mut mapper);

        assert_eq!(report.converted_rows, 1);
        // FO4 must not read the Starfield perk id as Fallout4.esm 1F0C46.
        assert_eq!(written_dnam(&book, &interner), "00".repeat(13));
        assert_eq!(
            outcome(&report, "teaches"),
            &MgefReferenceOutcome::DeferredNull {
                source_raw: 0x1F_0C46
            }
        );
    }

    #[test]
    fn magazine_perk_that_resolves_keeps_add_perk() {
        let interner = StringInterner::new();
        let mut mapper = mapper(&interner);
        mapper.add_mapping(
            form_key(&interner, "Starfield.esm", 0x1F_0C46),
            form_key(&interner, "Fallout4.esm", 0x1E_3CFB),
        );
        let mut book = record(&interner, b"BOOK", 0x1F_F722, NEW_ATLANTIAN_05);

        normalize(&mut book, &mut mapper);

        assert_eq!(
            written_dnam(&book, &interner),
            "10".to_owned() + "fb3c1e00" + &"00".repeat(8)
        );
    }

    #[test]
    fn data_slate_type_and_starfield_flag_leave_the_book() {
        let interner = StringInterner::new();
        // BOOK 001CF1 `CF_KryxHistoryInterview_Slate001`, an audio slate.
        let mut slate = record(&interner, b"BOOK", 0x1CF1, "2000000000000000000000000003000000");
        normalize(&mut slate, &mut mapper(&interner));
        assert_eq!(written_dnam(&slate, &interner), "00".repeat(13));

        // BOOK 0B2E51 `HT_Book_BestDefenseSpitting`: Can't be Taken survives.
        let mut book = record(&interner, b"BOOK", 0xB_2E51, "2200000000000000000000000001000000");
        normalize(&mut book, &mut mapper(&interner));
        assert_eq!(written_dnam(&book, &interner), "02".to_owned() + &"00".repeat(12));
    }

    #[test]
    fn damage_type_resistances_resolve_by_editor_id_never_by_shared_form_id() {
        let interner = StringInterner::new();
        // DMGT 060A81 `dtEnergy` resists with Starfield `EnergyResist`.
        let mut energy = record(&interner, b"DMGT", 0x6_0A81, "eb02000000000000");
        let report = normalize(&mut energy, &mut mapper(&interner));
        assert_eq!(report.converted_rows, 1);
        assert_eq!(written_dnam(&energy, &interner), "eb02000000000000");

        // DMGT 060A82 `dtFire_DONOTUSE` resists with `FireResist_DONOTUSE`.
        let mut fire = record(&interner, b"DMGT", 0x6_0A82, "e502000000000000");
        normalize(&mut fire, &mut mapper(&interner));
        assert_eq!(
            written_dnam(&fire, &interner),
            "00".repeat(8),
            "FireResist_DONOTUSE must not become FO4 FireResist"
        );
    }

    #[test]
    fn damage_type_spell_resolves_through_the_mapper_or_is_nulled() {
        let interner = StringInterner::new();
        // DMGT 000B79 `dtToxic` applies SPEL 255D8F `CCT_HitSpell_Poison`.
        const DT_TOXIC: &str = "000000008f5d2500";

        let mut unmapped = mapper(&interner);
        let mut toxic = record(&interner, b"DMGT", 0xB79, DT_TOXIC);
        let report = normalize(&mut toxic, &mut unmapped);
        assert_eq!(written_dnam(&toxic, &interner), "00".repeat(8));
        assert_eq!(
            outcome(&report, "spell"),
            &MgefReferenceOutcome::DeferredNull {
                source_raw: 0x25_5D8F
            }
        );

        let mut mapped = mapper(&interner);
        mapped.add_mapping(
            form_key(&interner, "Starfield.esm", 0x25_5D8F),
            form_key(&interner, "Fallout4.esm", 0x09_252C),
        );
        let mut toxic = record(&interner, b"DMGT", 0xB79, DT_TOXIC);
        normalize(&mut toxic, &mut mapped);
        assert_eq!(written_dnam(&toxic, &interner), "000000002c250900");
    }

    #[test]
    fn misc_art_objects_take_the_fo4_hit_effect_type() {
        let interner = StringInterner::new();
        for (local, dnam, written) in [
            // ARTO 016E0D `ShipGravJumpNPC`: Misc Art Object.
            (0x1_6E0D, "0300000000000000", "01000000"),
            // ARTO 00F72F `Weather_Precipitation_Snow`: hit effect, Reference
            // Effect | Attach To Camera | Inherit Rotation.
            (0xF72F, "0100000006000080", "01000000"),
            // ARTO 022DE3 `ArtifactPowerAntiGravFX_AO`: magic casting.
            (0x2_2DE3, "0000000000000000", "00000000"),
        ] {
            let mut mapper = mapper(&interner);
            let mut art = record(&interner, b"ARTO", local, dnam);
            let report = normalize(&mut art, &mut mapper);
            assert_eq!(report.converted_rows, 1, "ARTO {local:06X}");
            assert_eq!(written_dnam(&art, &interner), written, "ARTO {local:06X}");
        }
    }

    #[test]
    fn unexpected_payloads_are_left_alone() {
        let interner = StringInterner::new();
        for (sig, dnam) in [(b"BOOK", "00".repeat(13)), (b"ARTO", "01000000".into()), (b"DMGT", "00".repeat(16))] {
            let mut mapper = mapper(&interner);
            let mut unexpected = record(&interner, sig, 0x800, &dnam);
            let before = unexpected.fields[0].value.clone();
            let report = normalize(&mut unexpected, &mut mapper);
            assert_eq!(report.converted_rows, 0);
            assert_eq!(report.unsupported_rows, 1);
            assert_eq!(unexpected.fields[0].value, before);
        }
    }
}
