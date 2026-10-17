//! Fixup: expand ARMA additional races from RACE armor compatibility.
//!
//! FO76 custom humanoid races can use a vanilla armor race (for example
//! `LostRace` -> `HumanRace`). FO4 does not reliably apply an armor addon to the
//! custom race when only the armor race appears in ARMA.RNAM/MODL. If an ARMA
//! already supports the armor race, add the custom race to its repeatable MODL
//! additional-race list so equipped body-slot armor still supplies a model.
//! SRAC shares animation, not armor. Retain explicitly referenced shared addons
//! only where they do not overlap an addon's custom-race model, such as a shared
//! ghoul head alongside an Overgrown body.

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::face::materialize_leveled_template_npcs::read_target_or_master_record;
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
        let armo_sig =
            SigCode::from_str("ARMO").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();
        let (expansions, subgraph_races) =
            collect_armor_race_expansions(session, mapper, target_schema, race_sig, &mut report)?;
        if expansions.is_empty() && subgraph_races.is_empty() {
            return Ok(report);
        }

        let arma_fks = session
            .form_keys_of_sig(arma_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let existing_armas = arma_fks.iter().copied().collect::<FxHashSet<_>>();
        let target_plugin = mapper.output_plugin_sym();
        let target_masters = session.target_masters().to_vec();
        let (referenced_master_armas, addon_races) = collect_referenced_addons(
            session,
            mapper,
            target_schema,
            armo_sig,
            target_plugin,
            &subgraph_races,
            config,
            &mut report,
        )?;
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

            let added = augment_arma_additional_races(
                &mut record,
                &expansions,
                addon_races.get(&fk).map(Vec::as_slice).unwrap_or(&[]),
            );
            if added > 0 {
                report.records_changed += 1;
                report.records_added += added;
                changed_records.push(record);
            }
        }

        let mut master_overrides = Vec::new();
        for fk in referenced_master_armas {
            if existing_armas.contains(&fk) {
                continue;
            }
            let Some(mut record) = read_target_or_master_record(
                session,
                target_schema,
                mapper.interner,
                target_plugin,
                &target_masters,
                &config.target_master_handle_ids,
                fk,
            ) else {
                let warning = mapper
                    .interner
                    .intern("expand_arma_races_from_armor_race:referenced_master_arma_read_err");
                report.warnings.push(warning);
                continue;
            };
            if record.sig != arma_sig {
                continue;
            }
            if augment_arma_additional_races(
                &mut record,
                &expansions,
                addon_races.get(&fk).map(Vec::as_slice).unwrap_or(&[]),
            ) > 0
            {
                master_overrides.push(record);
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

        let overrides_added = session
            .add_records(master_overrides, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_added += overrides_added as u32;

        Ok(report)
    }
}

fn collect_referenced_addons(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    armo_sig: SigCode,
    target_plugin: crate::sym::Sym,
    subgraph_races: &FxHashMap<FormKey, FormKey>,
    config: &FixupConfig,
    report: &mut FixupReport,
) -> Result<(FxHashSet<FormKey>, FxHashMap<FormKey, Vec<FormKey>>), FixupError> {
    let modl_sig =
        SubrecordSig::from_str("MODL").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let armo_fks = session
        .form_keys_of_sig(armo_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut referenced = FxHashSet::default();
    let mut addon_races = FxHashMap::<FormKey, Vec<FormKey>>::default();
    let target_masters = session.target_masters().to_vec();

    for fk in armo_fks {
        let record = match session.record_decoded(&fk, target_schema, mapper.interner) {
            Ok(record) => record,
            Err(e) => {
                let warning = mapper.interner.intern(&format!(
                    "expand_arma_races_from_armor_race:armo_read_err:{e}"
                ));
                report.warnings.push(warning);
                continue;
            }
        };
        let addon_keys: Vec<_> = record
            .fields
            .iter()
            .filter(|entry| entry.sig == modl_sig)
            .filter_map(|entry| first_formkey(&entry.value))
            .filter(|addon| addon.local != 0)
            .collect();
        referenced.extend(
            addon_keys
                .iter()
                .copied()
                .filter(|addon| addon.plugin != target_plugin),
        );
        let Some(race) = record_ref(&record, "RNAM") else {
            continue;
        };
        let Some(parent) = subgraph_races.get(&race) else {
            continue;
        };
        let mut addons = Vec::new();
        for addon in addon_keys {
            if let Some(record) = session
                .record_decoded(&addon, target_schema, mapper.interner)
                .ok()
                .or_else(|| {
                    read_target_or_master_record(
                        session,
                        target_schema,
                        mapper.interner,
                        target_plugin,
                        &target_masters,
                        &config.target_master_handle_ids,
                        addon,
                    )
                })
                && record.sig.as_str() == "ARMA"
            {
                addons.push(record);
            }
        }
        let covered_slots = addons
            .iter()
            .filter(|addon| arma_supports_race(addon, race))
            .fold(0, |mask, addon| mask | biped_slots(addon));
        for addon in addons {
            if !arma_supports_race(&addon, race)
                && arma_supports_race(&addon, *parent)
                && biped_slots(&addon) & covered_slots == 0
            {
                let races = addon_races.entry(addon.form_key).or_default();
                if !races.contains(&race) {
                    races.push(race);
                }
            }
        }
    }

    Ok((referenced, addon_races))
}

fn collect_armor_race_expansions(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    race_sig: SigCode,
    report: &mut FixupReport,
) -> Result<
    (
        FxHashMap<FormKey, Vec<FormKey>>,
        FxHashMap<FormKey, FormKey>,
    ),
    FixupError,
> {
    let race_fks = session
        .form_keys_of_sig(race_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut expansions: FxHashMap<FormKey, Vec<FormKey>> = FxHashMap::default();
    let mut subgraph_races = FxHashMap::default();

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

        if let Some(armor_race) = record_ref(&record, "RNAM").filter(|race| *race != fk) {
            let races = expansions.entry(armor_race).or_default();
            if !races.contains(&fk) {
                races.push(fk);
            }
        } else if let Some(parent) = record_ref(&record, "SRAC").filter(|race| *race != fk) {
            subgraph_races.insert(fk, parent);
        }
    }

    Ok((expansions, subgraph_races))
}

pub fn augment_arma_additional_races(
    record: &mut Record,
    expansions: &FxHashMap<FormKey, Vec<FormKey>>,
    additional_races: &[FormKey],
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

    let mut to_add: Vec<_> = additional_races
        .iter()
        .copied()
        .filter(|race| !applicable_races.contains(race))
        .collect();
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

fn record_ref(record: &Record, sig: &str) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == sig)
        .and_then(|entry| first_formkey(&entry.value))
        .filter(|fk| fk.local != 0)
}

fn arma_supports_race(record: &Record, race: FormKey) -> bool {
    record.fields.iter().any(|entry| {
        matches!(entry.sig.as_str(), "RNAM" | "MODL") && first_formkey(&entry.value) == Some(race)
    })
}

fn biped_slots(record: &Record) -> u64 {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "BOD2")
        .map(|entry| match entry.value {
            FieldValue::Uint(mask) => mask,
            FieldValue::Int(mask) if mask >= 0 => mask as u64,
            _ => 0,
        })
        .unwrap_or(0)
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
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

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

        assert_eq!(
            augment_arma_additional_races(&mut arma, &expansions, &[]),
            1
        );
        assert_eq!(additional_races(&arma), vec![human, lost]);

        let mut expansions = FxHashMap::default();
        expansions.insert(human, vec![lost]);

        let mut arma = make_record("ARMA", fk("753E3B", "SeventySix.esm", &interner));
        push_field(&mut arma, "RNAM", FieldValue::FormKey(human));

        assert_eq!(
            augment_arma_additional_races(&mut arma, &expansions, &[]),
            1
        );
        assert_eq!(additional_races(&arma), vec![lost]);

        let mut expansions = FxHashMap::default();
        expansions.insert(human, vec![lost]);

        let mut arma = make_record("ARMA", fk("753E3B", "SeventySix.esm", &interner));
        push_field(&mut arma, "RNAM", FieldValue::FormKey(human));
        push_field(&mut arma, "MODL", FieldValue::FormKey(lost));

        assert_eq!(
            augment_arma_additional_races(&mut arma, &expansions, &[]),
            0
        );
        assert_eq!(additional_races(&arma), vec![lost]);

        let footstep = fk("012345", "Fallout4.esm", &interner);

        let mut expansions = FxHashMap::default();
        expansions.insert(human, vec![lost]);

        let mut arma = make_record("ARMA", fk("753E3B", "SeventySix.esm", &interner));
        push_field(&mut arma, "RNAM", FieldValue::FormKey(human));
        push_field(&mut arma, "SNDD", FieldValue::FormKey(footstep));

        assert_eq!(
            augment_arma_additional_races(&mut arma, &expansions, &[]),
            1
        );
        assert_eq!(arma.fields[1].sig.as_str(), "MODL");
        assert_eq!(arma.fields[2].sig.as_str(), "SNDD");
    }

    #[test]
    fn materializes_shared_head_without_overlapping_custom_skin_body() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let output = interner.intern("SeventySix.esm");
        let feral_race = FormKey {
            local: 0x06_B4EC,
            plugin: fallout4,
        };
        let overgrown_race = FormKey {
            local: 0x6B_3A3F,
            plugin: output,
        };
        let feral_head = FormKey {
            local: 0x07_ED01,
            plugin: fallout4,
        };
        let overgrown_armor = FormKey {
            local: 0x6D_EBE3,
            plugin: output,
        };
        let feral_body = fk("012345", "Fallout4.esm", &interner);
        let overgrown_body = fk("FFFFFE", "SeventySix.esm", &interner);
        let converted_parent_body = fk("FFFFFD", "SeventySix.esm", &interner);
        let unreferenced_parent_head = fk("FFFFFC", "SeventySix.esm", &interner);

        let fallout4_handle = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let schema = AuthoringSchema::for_game("fo4").unwrap();

        {
            let mut session = open_session(fallout4_handle, None).unwrap();
            let mut head = make_record("ARMA", feral_head);
            push_field(&mut head, "RNAM", FieldValue::FormKey(feral_race));
            push_field(&mut head, "BOD2", FieldValue::Uint(1));
            session
                .add_record(head, schema.as_ref(), &interner)
                .unwrap();
            let mut body = make_record("ARMA", feral_body);
            push_field(&mut body, "RNAM", FieldValue::FormKey(feral_race));
            push_field(&mut body, "BOD2", FieldValue::Uint(1 << 3));
            session
                .add_record(body, schema.as_ref(), &interner)
                .unwrap();
        }
        {
            let mut session = open_session(target_handle, None).unwrap();
            let mut race = make_record("RACE", overgrown_race);
            push_field(&mut race, "SRAC", FieldValue::FormKey(feral_race));
            session
                .add_record(race, schema.as_ref(), &interner)
                .unwrap();
            let mut body = make_record("ARMA", overgrown_body);
            push_field(&mut body, "RNAM", FieldValue::FormKey(overgrown_race));
            push_field(&mut body, "BOD2", FieldValue::Uint(1 << 3));
            session
                .add_record(body, schema.as_ref(), &interner)
                .unwrap();

            for (addon, slots) in [
                (converted_parent_body, 1 << 3),
                (unreferenced_parent_head, 1),
            ] {
                let mut record = make_record("ARMA", addon);
                push_field(&mut record, "RNAM", FieldValue::FormKey(feral_race));
                push_field(&mut record, "BOD2", FieldValue::Uint(slots));
                session
                    .add_record(record, schema.as_ref(), &interner)
                    .unwrap();
            }

            let mut armor = make_record("ARMO", overgrown_armor);
            push_field(&mut armor, "RNAM", FieldValue::FormKey(overgrown_race));
            push_field(&mut armor, "INDX", FieldValue::Uint(0));
            push_field(&mut armor, "MODL", FieldValue::FormKey(feral_head));
            push_field(&mut armor, "INDX", FieldValue::Uint(0));
            push_field(&mut armor, "MODL", FieldValue::FormKey(feral_body));
            push_field(&mut armor, "INDX", FieldValue::Uint(0));
            push_field(&mut armor, "MODL", FieldValue::FormKey(overgrown_body));
            push_field(&mut armor, "INDX", FieldValue::Uint(0));
            push_field(
                &mut armor,
                "MODL",
                FieldValue::FormKey(converted_parent_body),
            );
            session
                .add_record(armor, schema.as_ref(), &interner)
                .unwrap();
        }

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let config = FixupConfig {
            target_master_handle_ids: vec![fallout4_handle],
            target_schema: Some(schema.clone()),
            ..Default::default()
        };
        let mut session = open_session(target_handle, None).unwrap();
        let report = ExpandArmaRacesFromArmorRaceFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();

        assert_eq!(report.records_added, 1);
        let head_override = session
            .record_decoded(&feral_head, schema.as_ref(), &interner)
            .unwrap();
        assert_eq!(additional_races(&head_override), vec![overgrown_race]);
        assert!(
            session
                .record_decoded(&feral_body, schema.as_ref(), &interner)
                .is_err()
        );
        let custom_body = session
            .record_decoded(&overgrown_body, schema.as_ref(), &interner)
            .unwrap();
        assert!(additional_races(&custom_body).is_empty());
        for addon in [converted_parent_body, unreferenced_parent_head] {
            let record = session
                .record_decoded(&addon, schema.as_ref(), &interner)
                .unwrap();
            assert!(additional_races(&record).is_empty());
        }
        let repeated = ExpandArmaRacesFromArmorRaceFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(repeated.records_changed, 0);
        assert_eq!(repeated.records_added, 0);

        drop(session);
        assert!(plugin_handle_close_native(target_handle));
        assert!(plugin_handle_close_native(fallout4_handle));
    }
}
