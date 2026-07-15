//! Fixup: expand ARMA additional races from RACE ArmorRace.
//!
//! FO76 custom humanoid races can use a vanilla armor race (for example
//! `LostRace` -> `HumanRace`). FO4 does not reliably apply an armor addon to the
//! custom race when only the armor race appears in ARMA.RNAM/MODL. If an ARMA
//! already supports the armor race, add the custom race to its repeatable MODL
//! additional-race list so equipped body-slot armor still supplies a model.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

pub struct ExpandArmaRacesFromArmorRaceFixup;

impl Fixup for ExpandArmaRacesFromArmorRaceFixup {
    fn name(&self) -> &'static str {
        "expand_arma_races_from_armor_race"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let race_sig =
            SigCode::from_str("RACE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let arma_sig =
            SigCode::from_str("ARMA").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();
        let expansions =
            collect_armor_race_expansions(session, mapper, target_schema, race_sig, &mut report)?;
        if expansions.is_empty() {
            return Ok(report);
        }

        let arma_fks = session
            .form_keys_of_sig(arma_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let mut changed_records = Vec::new();

        for fk in arma_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(e) => {
                    let warning = mapper.interner.intern(&format!(
                        "expand_arma_races_from_armor_race:arma_read_err:{e}"
                    ));
                    report.warnings.push(warning);
                    continue;
                }
            };

            let added = augment_arma_additional_races(&mut record, &expansions);
            if added > 0 {
                report.records_changed += 1;
                report.records_added += added;
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "expand_arma_races_from_armor_race replaced {replaced} of {expected} expected records"
            )));
        }

        Ok(report)
    }
}

fn collect_armor_race_expansions(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    race_sig: SigCode,
    report: &mut FixupReport,
) -> Result<FxHashMap<FormKey, Vec<FormKey>>, FixupError> {
    let race_fks = session
        .form_keys_of_sig(race_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut expansions: FxHashMap<FormKey, Vec<FormKey>> = FxHashMap::default();

    for fk in race_fks {
        let record = match session.record_decoded(&fk, target_schema, mapper.interner) {
            Ok(record) => record,
            Err(e) => {
                let warning = mapper.interner.intern(&format!(
                    "expand_arma_races_from_armor_race:race_read_err:{e}"
                ));
                report.warnings.push(warning);
                continue;
            }
        };

        let Some(armor_race) = record_primary_race(&record) else {
            continue;
        };
        if armor_race.local == 0 || armor_race == fk {
            continue;
        }

        let races = expansions.entry(armor_race).or_default();
        if !races.contains(&fk) {
            races.push(fk);
        }
    }

    Ok(expansions)
}

pub fn augment_arma_additional_races(
    record: &mut Record,
    expansions: &FxHashMap<FormKey, Vec<FormKey>>,
) -> u32 {
    let rnam_sig = match SubrecordSig::from_str("RNAM") {
        Ok(sig) => sig,
        Err(_) => return 0,
    };
    let modl_sig = match SubrecordSig::from_str("MODL") {
        Ok(sig) => sig,
        Err(_) => return 0,
    };

    let mut applicable_races = FxHashSet::default();
    for entry in &record.fields {
        if (entry.sig == rnam_sig || entry.sig == modl_sig)
            && let Some(fk) = first_formkey(&entry.value)
            && fk.local != 0
        {
            applicable_races.insert(fk);
        }
    }

    let mut to_add = Vec::new();
    for race in applicable_races.iter() {
        let Some(expanded_races) = expansions.get(race) else {
            continue;
        };
        for expanded_race in expanded_races {
            if !applicable_races.contains(expanded_race) && !to_add.contains(expanded_race) {
                to_add.push(*expanded_race);
            }
        }
    }

    if to_add.is_empty() {
        return 0;
    }

    let mut insert_at = record
        .fields
        .iter()
        .rposition(|entry| entry.sig == modl_sig)
        .map(|index| index + 1)
        .unwrap_or_else(|| first_post_additional_race_index(record).unwrap_or(record.fields.len()));

    let added = to_add.len() as u32;
    for fk in to_add {
        record.fields.insert(
            insert_at,
            FieldEntry {
                sig: modl_sig,
                value: FieldValue::FormKey(fk),
            },
        );
        insert_at += 1;
    }

    added
}

fn record_primary_race(record: &Record) -> Option<FormKey> {
    let rnam_sig = SubrecordSig::from_str("RNAM").ok()?;
    record
        .fields
        .iter()
        .find(|entry| entry.sig == rnam_sig)
        .and_then(|entry| first_formkey(&entry.value))
}

fn first_formkey(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_formkey(value)),
        FieldValue::List(items) => items.iter().find_map(first_formkey),
        _ => None,
    }
}

fn first_post_additional_race_index(record: &Record) -> Option<usize> {
    record.fields.iter().position(|entry| {
        matches!(
            entry.sig.as_str(),
            "SNDD" | "ONAM" | "BSMP" | "BSMB" | "BSMS" | "BSMR"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn fk(hex: &str, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(&format!("{hex}@{plugin}"), interner).unwrap()
    }

    fn make_record(sig: &str, form_key: FormKey) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key,
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn additional_races(record: &Record) -> Vec<FormKey> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "MODL")
            .filter_map(|entry| first_formkey(&entry.value))
            .collect()
    }

    #[test]
    fn adds_custom_race_when_arma_supports_its_armor_race() {
        let interner = StringInterner::new();
        let feral = fk("013746", "Fallout4.esm", &interner);
        let human = fk("0EAFB6", "Fallout4.esm", &interner);
        let lost = fk("727CCF", "SeventySix.esm", &interner);

        let mut expansions = FxHashMap::default();
        expansions.insert(human, vec![lost]);

        let mut arma = make_record("ARMA", fk("727050", "SeventySix.esm", &interner));
        push_field(&mut arma, "RNAM", FieldValue::FormKey(feral));
        push_field(&mut arma, "MODL", FieldValue::FormKey(human));

        assert_eq!(augment_arma_additional_races(&mut arma, &expansions), 1);
        assert_eq!(additional_races(&arma), vec![human, lost]);
    }

    #[test]
    fn uses_primary_race_as_expansion_seed() {
        let interner = StringInterner::new();
        let human = fk("0EAFB6", "Fallout4.esm", &interner);
        let lost = fk("727CCF", "SeventySix.esm", &interner);

        let mut expansions = FxHashMap::default();
        expansions.insert(human, vec![lost]);

        let mut arma = make_record("ARMA", fk("753E3B", "SeventySix.esm", &interner));
        push_field(&mut arma, "RNAM", FieldValue::FormKey(human));

        assert_eq!(augment_arma_additional_races(&mut arma, &expansions), 1);
        assert_eq!(additional_races(&arma), vec![lost]);
    }

    #[test]
    fn skips_existing_custom_race() {
        let interner = StringInterner::new();
        let human = fk("0EAFB6", "Fallout4.esm", &interner);
        let lost = fk("727CCF", "SeventySix.esm", &interner);

        let mut expansions = FxHashMap::default();
        expansions.insert(human, vec![lost]);

        let mut arma = make_record("ARMA", fk("753E3B", "SeventySix.esm", &interner));
        push_field(&mut arma, "RNAM", FieldValue::FormKey(human));
        push_field(&mut arma, "MODL", FieldValue::FormKey(lost));

        assert_eq!(augment_arma_additional_races(&mut arma, &expansions), 0);
        assert_eq!(additional_races(&arma), vec![lost]);
    }

    #[test]
    fn inserts_before_post_modl_fields_when_no_modl_exists() {
        let interner = StringInterner::new();
        let human = fk("0EAFB6", "Fallout4.esm", &interner);
        let lost = fk("727CCF", "SeventySix.esm", &interner);
        let footstep = fk("012345", "Fallout4.esm", &interner);

        let mut expansions = FxHashMap::default();
        expansions.insert(human, vec![lost]);

        let mut arma = make_record("ARMA", fk("753E3B", "SeventySix.esm", &interner));
        push_field(&mut arma, "RNAM", FieldValue::FormKey(human));
        push_field(&mut arma, "SNDD", FieldValue::FormKey(footstep));

        assert_eq!(augment_arma_additional_races(&mut arma, &expansions), 1);
        assert_eq!(arma.fields[1].sig.as_str(), "MODL");
        assert_eq!(arma.fields[2].sig.as_str(), "SNDD");
    }
}
