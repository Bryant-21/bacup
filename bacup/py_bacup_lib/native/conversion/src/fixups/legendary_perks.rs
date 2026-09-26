use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use std::collections::HashMap;

const CARDS: &[&str] = &[
    "AmmoFactory",
    "BloodSacrifice",
    "BrawlingChemist",
    "CollateralDamage",
    "DetonationContagion",
    "ElectricAbsorption",
    "ExplodingPalm",
    "FarFlungFireworks",
    "FollowThrough",
    "FunkyDuds",
    "HackAndSlash",
    "LegendaryAgility",
    "LegendaryCharisma",
    "LegendaryEndurance",
    "LegendaryIntelligence",
    "LegendaryLuck",
    "LegendaryPerception",
    "LegendaryStrength",
    "MasterInfiltrator",
    "PowerArmorReboot",
    "PowerSprinter",
    "Retribution",
    "SizzlingStyle",
    "SurvivalShortcut",
    "TakingOneForTheTeam",
    "WhatRads",
];

pub struct LegendaryPerksFixup;

fn released_rank(eid: &str) -> Option<(&str, u8)> {
    let (key, rank) = eid.strip_prefix("LGN_")?.rsplit_once("_Perk")?;
    let rank = match rank {
        "01" => 1,
        "02" => 2,
        "03" => 3,
        "04" => 4,
        _ => return None,
    };
    CARDS.contains(&key).then_some((key, rank))
}

fn raw(sig: &[u8; 4], data: impl Into<Vec<u8>>) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value: FieldValue::Bytes(data.into().into()),
    }
}

fn error(value: impl std::fmt::Display) -> FixupError {
    FixupError::Other(value.to_string())
}

fn ranked_ability(
    source: &Record,
    fk: FormKey,
    eid: &str,
    stat_rank: Option<u8>,
    interner: &crate::sym::StringInterner,
) -> Result<Record, FixupError> {
    let effect = source
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"EFID")
        .ok_or_else(|| error("legendary stat ability has no effect"))?;
    let mut spell = Record::new(SigCode(*b"SPEL"), fk);
    spell.eid = Some(interner.intern(eid));
    spell.fields.push(FieldEntry {
        sig: SubrecordSig(*b"EDID"),
        value: FieldValue::String(spell.eid.unwrap()),
    });
    spell.fields.extend(
        source
            .fields
            .iter()
            .filter(|field| matches!(&field.sig.0, b"OBND" | b"FULL" | b"SPIT" | b"ETYP"))
            .cloned(),
    );
    spell.fields.push(effect.clone());
    if let Some(rank) = stat_rank {
        // FO76 shares one SPECIAL spell and gates its extra effects on the original rank perks.
        let mut data = [1.0f32, 2.0, 3.0, 5.0][rank as usize - 1]
            .to_le_bytes()
            .to_vec();
        data.extend_from_slice(&[0; 8]);
        spell.fields.push(raw(b"EFIT", data));
    } else {
        spell.fields.push(
            source
                .fields
                .iter()
                .find(|field| field.sig.0 == *b"EFIT")
                .ok_or_else(|| error("legendary ability has no magnitude"))?
                .clone(),
        );
    }
    Ok(spell)
}

fn armor_conditions(
    fields: &[([u8; 4], Vec<u8>)],
    mut map_keyword: impl FnMut(u32) -> Result<u32, FixupError>,
) -> Result<Vec<FieldEntry>, FixupError> {
    let mut result = Vec::new();
    for (sig, bytes) in fields {
        if sig != b"CTDA" || bytes.len() != 32 || u16::from_le_bytes([bytes[8], bytes[9]]) != 722 {
            continue;
        }
        let count = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if bytes[0] != 0x61 || !matches!(count, 5.0 | 6.0) {
            return Err(error("legendary armor-set condition changed"));
        }
        let mut condition = bytes.clone();
        let keyword = map_keyword(u32::from_le_bytes(bytes[12..16].try_into().unwrap()))?;
        condition[12..16].copy_from_slice(&keyword.to_le_bytes());
        result.push(raw(b"CTDA", condition));
    }
    if result.is_empty() {
        return Err(error(
            "legendary armor ability has no matching-set conditions",
        ));
    }
    Ok(result)
}

fn native_entries(
    key: &str,
    fields: &[([u8; 4], Vec<u8>)],
    mut map_spell: impl FnMut(u32) -> Result<u32, FixupError>,
) -> Result<Vec<FieldEntry>, FixupError> {
    let mut result = Vec::new();
    let mut kind = None;
    if key.starts_with("Legendary") || matches!(key, "FunkyDuds" | "SizzlingStyle" | "WhatRads") {
        for (sig, data) in fields {
            match sig {
                b"PRKE" => kind = data.first().copied(),
                b"PRKF" => kind = None,
                b"DATA" if kind == Some(1) && data.len() == 4 => {
                    let spell = map_spell(u32::from_le_bytes(data[..].try_into().unwrap()))?;
                    result.extend([
                        raw(b"PRKE", [1, 0, 0]),
                        raw(b"DATA", spell.to_le_bytes()),
                        raw(b"PRKF", []),
                    ]);
                }
                _ => {}
            }
        }
        if result.is_empty() {
            return Err(error(format!("legendary perk {key} has no source ability")));
        }
    } else if key == "MasterInfiltrator" {
        // FO4 gates locks and terminals with entry points instead of FO76 skill actor values.
        for entry in [62, 98] {
            result.extend([
                raw(b"PRKE", [2, 0, 255]),
                raw(b"DATA", [entry, 1, 1]),
                raw(b"EPFT", [1]),
                raw(b"EPFD", 3.0f32.to_le_bytes()),
                raw(b"PRKF", []),
            ]);
        }
        let conditions: Vec<_> = fields
            .iter()
            .filter(|(sig, bytes)| {
                sig == b"CTDA"
                    && bytes.len() == 32
                    && u16::from_le_bytes([bytes[8], bytes[9]]) == 65
            })
            .collect();
        if conditions.len() != 2 {
            return Err(error("Master Infiltrator lock thresholds changed"));
        }
        for (entry, (_, condition)) in [91, 92].into_iter().zip(conditions) {
            result.extend([
                raw(b"PRKE", [2, 0, 0]),
                raw(b"DATA", [entry, 1, 2]),
                raw(b"PRKC", [1]),
                raw(b"CTDA", condition.clone()),
                raw(b"EPFT", [1]),
                raw(b"EPFD", 1.0f32.to_le_bytes()),
                raw(b"PRKF", []),
            ]);
        }
    } else if key == "PowerSprinter" {
        let valid_entry = fields
            .iter()
            .any(|(sig, data)| sig == b"DATA" && data == &[96, 3, 1, 0]);
        let parameter = fields
            .iter()
            .find(|(sig, data)| sig == b"EPFD" && data.len() == 4);
        let Some((_, parameter)) = parameter.filter(|_| valid_entry) else {
            return Err(error("Power Sprinter source definition changed"));
        };
        let multiplier = f32::from_le_bytes(parameter[..].try_into().unwrap());
        if !multiplier.is_finite() || !(0.0..=1.0).contains(&multiplier) {
            return Err(error("Power Sprinter multiplier is invalid"));
        }
        // Tales applies this perk only while the equipped player is in power armor.
        result.extend([
            raw(b"PRKE", [2, 0, 0]),
            raw(b"DATA", [96, 3, 1]),
            raw(b"EPFT", [1]),
            raw(b"EPFD", parameter.clone()),
            raw(b"PRKF", []),
        ]);
    }
    Ok(result)
}

impl Fixup for LegendaryPerksFixup {
    fn name(&self) -> &'static str {
        "legendary_perks"
    }
    fn uses_session(&self) -> bool {
        true
    }
    fn applies_to_session(&self, session: &PluginSession, _: &FixupConfig) -> bool {
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let Some(source_id) = session.source_id() else {
            return Ok(report);
        };
        let source = session.source_slot_opt().unwrap();
        let source_plugin = source.parsed.plugin_name.clone();
        let source_masters = source.parsed.header.masters.clone();
        let schema = session.schema().map_err(error)?;
        let target_masters = session.target_masters().to_vec();
        let (candidates, special_base_fields) = {
            let scan = session.handle_raw_scan(source_id).map_err(error)?;
            let eids = scan.own_editor_ids();
            let mut result = Vec::new();
            let mut special_base_fields = HashMap::new();
            for local in scan.raw_form_ids_of_sig(SigCode(*b"PERK")) {
                let Some(eid) = eids.get(&(local & 0xFFFFFF)) else {
                    continue;
                };
                let Some((key, rank)) = released_rank(eid) else {
                    continue;
                };
                let fields = scan
                    .with_record_subrecords(local, |fields| {
                        fields
                            .iter()
                            .map(|field| {
                                (
                                    field.signature.as_bytes().try_into().unwrap(),
                                    field.data.to_vec(),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                    .ok_or_else(|| error("source legendary perk disappeared"))?;
                if rank == 1 && key.starts_with("Legendary") {
                    special_base_fields.insert(key.to_string(), fields.clone());
                }
                let source_fk = FormKey {
                    plugin: mapper.interner.intern(&source_plugin),
                    local: local & 0xFFFFFF,
                };
                let Some(target_fk) = mapper.lookup(source_fk) else {
                    continue;
                };
                result.push((key.to_string(), rank, target_fk, fields));
            }
            result.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
            (result, special_base_fields)
        };
        for (key, rank, target_fk, fields) in candidates {
            let Ok(target) = session.record_decoded(&target_fk, &schema, mapper.interner) else {
                continue;
            };
            if matches!(key.as_str(), "AmmoFactory" | "BloodSacrifice") {
                let bytes = fields
                    .iter()
                    .find(|(sig, bytes)| sig == b"EPFD" && bytes.len() == 4)
                    .map(|(_, bytes)| bytes)
                    .ok_or_else(|| error(format!("{key} source multiplier is missing")))?;
                let multiplier = f32::from_le_bytes(bytes[..].try_into().unwrap());
                if !multiplier.is_finite() || !(1.0..=3.0).contains(&multiplier) {
                    return Err(error(format!("{key} source multiplier is invalid")));
                }
                let eid = format!("B21_Legendary_{key}_{rank}_Value");
                let synthetic = FormKey {
                    plugin: mapper.interner.intern(&eid),
                    local: 1,
                };
                let fk = mapper.allocate_or_resolve(synthetic, None, SigCode(*b"GLOB"));
                let mut global = Record::new(SigCode(*b"GLOB"), fk);
                global.eid = Some(mapper.interner.intern(&eid));
                global.fields.push(FieldEntry {
                    sig: SubrecordSig(*b"EDID"),
                    value: FieldValue::String(global.eid.unwrap()),
                });
                global
                    .fields
                    .extend([raw(b"FNAM", [b'f']), raw(b"FLTV", multiplier.to_le_bytes())]);
                if session
                    .record_decoded(&fk, &schema, mapper.interner)
                    .is_ok()
                {
                    session
                        .replace_records_contents(vec![global], &schema, mapper.interner)
                        .map_err(error)?;
                    report.records_changed += 1;
                } else {
                    session
                        .add_record(global, &schema, mapper.interner)
                        .map_err(error)?;
                    report.records_added += 1;
                }
            }
            let mut ability_index = 0;
            // Higher SPECIAL ranks can omit PRKE because rank 1 owns their shared spell.
            let entry_fields = if rank > 1
                && key.starts_with("Legendary")
                && !fields.iter().any(|(sig, _)| sig == b"PRKE")
            {
                special_base_fields.get(&key).ok_or_else(|| {
                    error(format!(
                        "LGN_{key}_Perk{rank:02} requires its rank 1 source ability"
                    ))
                })?
            } else {
                &fields
            };
            let entries = native_entries(&key, entry_fields, |raw_id| {
                let index = (raw_id >> 24) as usize;
                let plugin = if index == source_masters.len() || index == 255 {
                    &source_plugin
                } else {
                    source_masters
                        .get(index)
                        .ok_or_else(|| error("invalid legendary spell master index"))?
                };
                let source_fk = FormKey {
                    plugin: mapper.interner.intern(plugin),
                    local: raw_id & 0xFFFFFF,
                };
                let source_spell = mapper
                    .lookup(source_fk)
                    .ok_or_else(|| error("unmapped legendary ability"))?;
                let spell = session
                    .record_decoded(&source_spell, &schema, mapper.interner)
                    .map_err(error)?;
                if spell.sig != SigCode(*b"SPEL") {
                    return Err(error("legendary ability must be a spell"));
                }
                ability_index += 1;
                let suffix = if ability_index == 1 {
                    String::new()
                } else {
                    ability_index.to_string()
                };
                let eid = format!("B21_Legendary_{key}_{rank}_Ability{suffix}");
                let synthetic = FormKey {
                    plugin: mapper.interner.intern(&eid),
                    local: 1,
                };
                let fk = mapper.allocate_or_resolve(synthetic, None, SigCode(*b"SPEL"));
                let mut spell = ranked_ability(
                    &spell,
                    fk,
                    &eid,
                    key.starts_with("Legendary").then_some(rank),
                    mapper.interner,
                )?;
                if matches!(key.as_str(), "FunkyDuds" | "SizzlingStyle") {
                    let source_fields = session
                        .handle_raw_scan(source_id)
                        .map_err(error)?
                        .with_record_subrecords(raw_id, |fields| {
                            fields
                                .iter()
                                .map(|field| {
                                    (
                                        field.signature.as_bytes().try_into().unwrap(),
                                        field.data.to_vec(),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .ok_or_else(|| error("source armor ability disappeared"))?;
                    spell
                        .fields
                        .extend(armor_conditions(&source_fields, |keyword| {
                            let index = (keyword >> 24) as usize;
                            let plugin = if index == source_masters.len() || index == 255 {
                                &source_plugin
                            } else {
                                source_masters
                                    .get(index)
                                    .ok_or_else(|| error("invalid armor keyword master index"))?
                            };
                            let source_fk = FormKey {
                                plugin: mapper.interner.intern(plugin),
                                local: keyword & 0xFFFFFF,
                            };
                            let target = mapper
                                .lookup(source_fk)
                                .ok_or_else(|| error("unmapped legendary armor keyword"))?;
                            encode_target_form_id(target, mapper.interner, &target_masters)
                                .ok_or_else(|| error("cannot encode legendary armor keyword"))
                        })?);
                }
                if session
                    .record_decoded(&fk, &schema, mapper.interner)
                    .is_ok()
                {
                    session
                        .replace_records_contents(vec![spell], &schema, mapper.interner)
                        .map_err(error)?;
                    report.records_changed += 1;
                } else {
                    session
                        .add_record(spell, &schema, mapper.interner)
                        .map_err(error)?;
                    report.records_added += 1;
                }
                encode_target_form_id(fk, mapper.interner, &target_masters)
                    .ok_or_else(|| error("cannot encode legendary ability"))
            })
            .map_err(|cause| error(format!("LGN_{key}_Perk{rank:02}: {cause}")))?;
            let eid = format!("B21_Legendary_{key}_{rank}");
            let synthetic = FormKey {
                plugin: mapper.interner.intern(&eid),
                local: 1,
            };
            let fk = mapper.allocate_or_resolve(synthetic, None, SigCode(*b"PERK"));
            let mut perk = Record::new(SigCode(*b"PERK"), fk);
            perk.eid = Some(mapper.interner.intern(&eid));
            perk.fields.push(FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::String(perk.eid.unwrap()),
            });
            perk.fields.extend(
                target
                    .fields
                    .into_iter()
                    .filter(|field| matches!(&field.sig.0, b"FULL" | b"DESC")),
            );
            if let Some(stat) = key.strip_prefix("Legendary") {
                let description = format!("+{} {stat}.", [1, 2, 3, 5][rank as usize - 1]);
                perk.fields.retain(|field| field.sig.0 != *b"DESC");
                perk.fields.push(FieldEntry {
                    sig: SubrecordSig(*b"DESC"),
                    value: FieldValue::String(mapper.interner.intern(&description)),
                });
            }
            perk.fields.push(raw(b"DATA", [0, 0, 1, 0, 1]));
            perk.fields.extend(entries);
            if session
                .record_decoded(&fk, &schema, mapper.interner)
                .is_ok()
            {
                session
                    .replace_records_contents(vec![perk], &schema, mapper.interner)
                    .map_err(error)?;
                report.records_changed += 1;
            } else {
                session
                    .add_record(perk, &schema, mapper.interner)
                    .map_err(error)?;
                report.records_added += 1;
            }
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excludes_ghoul_and_development_cards() {
        assert_eq!(
            released_rank("LGN_LegendaryStrength_Perk01"),
            Some(("LegendaryStrength", 1))
        );
        for eid in [
            "LGN_ThickSkin_Perk01",
            "LGN_TakingOneForTheTeam_DamageIncrease_Perk01",
            "LGN_AmmoFactory_Perk05",
            "LGN_AmmoFactory_Perk1",
        ] {
            assert_eq!(released_rank(eid), None);
        }
        assert_eq!(CARDS.len(), 26);
    }
    #[test]
    fn native_entries_restore_abilities_parameters_and_skill_gates() {
        let fields = vec![
            (*b"PRKE", vec![1, 0]),
            (*b"DATA", vec![0x81, 0xF1, 0x5C, 0]),
            (*b"PRKF", vec![]),
        ];
        let entries = native_entries("LegendaryStrength", &fields, |id| {
            assert_eq!(id, 0x5CF181);
            Ok(0x01001234)
        })
        .unwrap();
        assert_eq!(entries[1].value, raw(b"DATA", [0x34, 0x12, 0, 1]).value);
        assert_eq!(entries.len(), 3);

        let fields = vec![
            (*b"DATA", vec![96, 3, 1, 0]),
            (*b"EPFD", 0.8f32.to_le_bytes().to_vec()),
        ];
        let entries = native_entries("PowerSprinter", &fields, |_| panic!()).unwrap();
        assert_eq!(entries[1].value, raw(b"DATA", [96, 3, 1]).value);
        assert_eq!(entries[3].value, raw(b"EPFD", 0.8f32.to_le_bytes()).value);
        assert!(
            native_entries("BloodSacrifice", &fields, |_| panic!())
                .unwrap()
                .is_empty()
        );

        let mut fields = Vec::new();
        for id in [0x5A5951u32, 0x5A594C] {
            fields.extend([
                (*b"PRKE", vec![1, 0]),
                (*b"DATA", id.to_le_bytes().to_vec()),
                (*b"PRKF", vec![]),
            ]);
        }
        let entries = native_entries("WhatRads", &fields, |id| Ok(id | 0x01000000)).unwrap();
        assert_eq!(entries.len(), 6);
        assert_ne!(entries[1].value, entries[4].value);

        let mut condition = vec![0; 32];
        condition[0] = 0xA0;
        condition[4..8].copy_from_slice(&100.0f32.to_le_bytes());
        condition[8..10].copy_from_slice(&65u16.to_le_bytes());
        condition[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        let entries = native_entries(
            "MasterInfiltrator",
            &[(*b"CTDA", condition.clone()), (*b"CTDA", condition.clone())],
            |_| panic!(),
        )
        .unwrap();
        let data: Vec<_> = entries
            .iter()
            .filter(|entry| entry.sig.0 == *b"DATA")
            .collect();
        for (entry, expected) in data
            .iter()
            .zip([[62, 1, 1], [98, 1, 1], [91, 1, 2], [92, 1, 2]])
        {
            assert_eq!(entry.value, raw(b"DATA", expected).value);
        }
        assert_eq!(entries[0].value, raw(b"PRKE", [2, 0, 255]).value);
        let conditions: Vec<_> = entries
            .iter()
            .filter(|entry| entry.sig.0 == *b"CTDA")
            .collect();
        assert_eq!(conditions.len(), 2);
        assert!(
            conditions
                .iter()
                .all(|entry| entry.value == raw(b"CTDA", condition.clone()).value)
        );
    }

    #[test]
    fn armor_sets_preserve_source_thresholds_and_remap_keywords_without_rank_gates() {
        let mut gate = vec![0; 32];
        gate[8..10].copy_from_slice(&448u16.to_le_bytes());
        let mut armor = vec![0; 32];
        armor[0] = 0x61;
        armor[4..8].copy_from_slice(&5.0f32.to_le_bytes());
        armor[8..10].copy_from_slice(&722u16.to_le_bytes());
        armor[12..16].copy_from_slice(&0x1234u32.to_le_bytes());
        armor[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        let conditions =
            armor_conditions(&[(*b"CTDA", gate), (*b"CTDA", armor.clone())], |keyword| {
                assert_eq!(keyword, 0x1234);
                Ok(0x01003456)
            })
            .unwrap();
        armor[12..16].copy_from_slice(&0x01003456u32.to_le_bytes());
        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].value, raw(b"CTDA", armor).value);
        assert!(armor_conditions(&[], |_| panic!()).is_err());
    }

    #[test]
    fn special_ranks_do_not_depend_on_original_fo76_perk_ownership() {
        let interner = crate::sym::StringInterner::new();
        let fk = FormKey {
            plugin: interner.intern("Converted.esm"),
            local: 0x800,
        };
        let mut source = Record::new(SigCode(*b"SPEL"), fk);
        source.fields.extend([
            raw(b"SPIT", [0; 36]),
            raw(b"EFID", [1, 2, 3, 4]),
            raw(b"EFIT", [0; 12]),
            raw(b"EFID", [1, 2, 3, 4]),
            raw(b"EFIT", [0; 12]),
            raw(b"CTDA", [0; 32]),
        ]);
        for (rank, magnitude) in [(1, 1.0f32), (2, 2.0), (3, 3.0), (4, 5.0)] {
            let spell =
                ranked_ability(&source, fk, "B21_TestAbility", Some(rank), &interner).unwrap();
            assert_eq!(spell.fields.len(), 4);
            assert_eq!(
                spell.fields[3].value,
                raw(
                    b"EFIT",
                    [magnitude.to_le_bytes().as_slice(), &[0; 8]].concat()
                )
                .value
            );
        }
    }

    #[test]
    fn session_emits_all_special_ranks_from_base_abilities_idempotently() {
        check_special_session(&[1, 2, 3, 4]);

        check_special_session(&[4]);
    }

    fn check_special_session(target_ranks: &[u8]) {
        use crate::formkey_mapper::MapperOptions;
        use crate::session::open_session;
        use bytes::Bytes;
        use esp_authoring_core::plugin_runtime::{
            ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_close_native,
            plugin_handle_new_native, plugin_handle_store_ref,
        };
        let interner = crate::sym::StringInterner::new();
        let source_id = plugin_handle_new_native("Source.esm", Some("fo76")).unwrap();
        let target_id = plugin_handle_new_native("Output.esm", Some("fo4")).unwrap();
        let special: Vec<_> = CARDS
            .iter()
            .filter(|key| key.starts_with("Legendary"))
            .collect();
        assert_eq!(special.len(), 7);
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source = store.get_mut(&source_id).unwrap();
            for (index, key) in special.iter().enumerate() {
                let base = 0x800 + index as u32 * 0x10;
                // The installed FO76 layout gives only rank 1 a shared ability entry.
                for rank in (1..=4).rev() {
                    let mut fields = vec![
                        ("EDID", format!("LGN_{key}_Perk{rank:02}\0").into_bytes()),
                        ("DATA", vec![1, 0, 1]),
                    ];
                    if rank == 1 {
                        fields.extend([
                            ("PRKE", vec![1, 0]),
                            ("DATA", (base + 4).to_le_bytes().to_vec()),
                            ("PRKF", vec![]),
                        ]);
                    }
                    source
                        .parsed
                        .root_items
                        .push(ParsedItem::Record(ParsedRecord {
                            signature: "PERK".into(),
                            form_id: base + rank - 1,
                            flags: 0,
                            version_control: 0,
                            form_version: Some(209),
                            version2: None,
                            raw_payload: None,
                            parse_error: None,
                            subrecords: fields
                                .into_iter()
                                .map(|(sig, data)| ParsedSubrecord {
                                    signature: sig.into(),
                                    data: Bytes::from(data),
                                    semantic_type: None,
                                })
                                .collect(),
                        }));
                }
            }
            source.invalidate_sections();
        }
        let mut session = open_session(target_id, Some(source_id)).unwrap();
        session
            .target_slot_mut()
            .parsed
            .header
            .masters
            .push("Fallout4.esm".into());
        let schema = session.schema().unwrap();
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                source_plugin_name: "Source.esm".into(),
                output_plugin_name: "Output.esm".into(),
                ..MapperOptions::default()
            },
            &interner,
        );
        let target_fk = |local| FormKey {
            plugin: interner.intern("Output.esm"),
            local: local + 0x1000,
        };
        for index in 0..special.len() {
            let base = 0x800 + index as u32 * 0x10;
            for &rank in target_ranks {
                let local = base + rank as u32 - 1;
                mapper.add_mapping(
                    FormKey {
                        plugin: interner.intern("Source.esm"),
                        local,
                    },
                    target_fk(local),
                );
                let mut perk = Record::new(SigCode(*b"PERK"), target_fk(local));
                perk.fields.push(raw(b"DATA", [0, 0, 1, 0, 1]));
                session.add_record(perk, &schema, &interner).unwrap();
            }
            for local in [base + 4, base + 5] {
                mapper.add_mapping(
                    FormKey {
                        plugin: interner.intern("Source.esm"),
                        local,
                    },
                    target_fk(local),
                );
            }
            let mut spell = Record::new(SigCode(*b"SPEL"), target_fk(base + 4));
            spell.fields.push(raw(b"SPIT", [0; 36]));
            for _ in 1..=4 {
                spell.fields.extend([
                    FieldEntry {
                        sig: SubrecordSig(*b"EFID"),
                        value: FieldValue::FormKey(target_fk(base + 5)),
                    },
                    raw(b"EFIT", [0; 12]),
                    raw(b"CTDA", [0; 32]),
                ]);
            }
            session.add_record(spell, &schema, &interner).unwrap();
        }
        let config = FixupConfig::default();
        let first = LegendaryPerksFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        let expected_records = (special.len() * target_ranks.len() * 2) as u32;
        assert_eq!(first.records_added, expected_records);
        let mut spell_ids = std::collections::HashSet::new();
        let mut named_records = Vec::new();
        for (index, key) in special.iter().enumerate() {
            for &rank in target_ranks {
                let eid = format!("B21_Legendary_{key}_{rank}");
                let perk_fk = mapper
                    .lookup(FormKey {
                        plugin: interner.intern(&eid),
                        local: 1,
                    })
                    .unwrap();
                let spell_fk = mapper
                    .lookup(FormKey {
                        plugin: interner.intern(&format!("{eid}_Ability")),
                        local: 1,
                    })
                    .unwrap();
                named_records
                    .extend([(perk_fk, eid.clone()), (spell_fk, format!("{eid}_Ability"))]);
                assert!(spell_ids.insert(spell_fk));
                let perk = session
                    .record_decoded(&perk_fk, &schema, &interner)
                    .unwrap();
                assert_eq!(
                    perk.eid.and_then(|symbol| interner.resolve(symbol)),
                    Some(eid.as_str())
                );
                assert_eq!(
                    session
                        .first_subrecord_bytes(&perk_fk, "EDID")
                        .unwrap()
                        .as_deref(),
                    Some(format!("{eid}\0").as_bytes())
                );
                let entry = perk
                    .fields
                    .iter()
                    .skip_while(|field| field.sig.0 != *b"PRKE")
                    .find(|field| field.sig.0 == *b"DATA")
                    .unwrap();
                assert_eq!(
                    entry.value,
                    raw(b"DATA", (0x01000000 | spell_fk.local).to_le_bytes()).value
                );
                let spell = session
                    .record_decoded(&spell_fk, &schema, &interner)
                    .unwrap();
                assert_eq!(
                    session
                        .first_subrecord_bytes(&spell_fk, "EDID")
                        .unwrap()
                        .as_deref(),
                    Some(format!("{eid}_Ability\0").as_bytes())
                );
                assert_eq!(
                    spell
                        .fields
                        .iter()
                        .filter(|field| field.sig.0 == *b"EFID")
                        .count(),
                    1
                );
                assert!(spell.fields.iter().all(|field| field.sig.0 != *b"CTDA"));
                let efid = session
                    .first_subrecord_bytes(&spell_fk, "EFID")
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    u32::from_le_bytes(efid[..].try_into().unwrap()),
                    0x01001805 + index as u32 * 0x10
                );
                let efit = session
                    .first_subrecord_bytes(&spell_fk, "EFIT")
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    f32::from_le_bytes(efit[..4].try_into().unwrap()),
                    [1.0, 2.0, 3.0, 5.0][rank as usize - 1]
                );
            }
        }
        let second = LegendaryPerksFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(second.records_added, 0);
        assert_eq!(second.records_changed, expected_records);
        drop(session);
        assert_saved_editor_ids(target_id, &named_records, &interner);
        assert!(plugin_handle_close_native(source_id));
        assert!(plugin_handle_close_native(target_id));
    }

    #[test]
    fn session_saves_perk_and_multiplier_editor_ids() {
        use crate::formkey_mapper::MapperOptions;
        use crate::session::open_session;
        use bytes::Bytes;
        use esp_authoring_core::plugin_runtime::{
            ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_close_native,
            plugin_handle_new_native, plugin_handle_store_ref,
        };
        let interner = crate::sym::StringInterner::new();
        let source_id = plugin_handle_new_native("Source.esm", Some("fo76")).unwrap();
        let target_id = plugin_handle_new_native("Output.esm", Some("fo4")).unwrap();
        let definitions = [("AmmoFactory", 1.5f32), ("BloodSacrifice", 2.5)];
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let source = store.get_mut(&source_id).unwrap();
            for (index, (key, multiplier)) in definitions.iter().enumerate() {
                source
                    .parsed
                    .root_items
                    .push(ParsedItem::Record(ParsedRecord {
                        signature: "PERK".into(),
                        form_id: 0x800 + index as u32,
                        flags: 0,
                        version_control: 0,
                        form_version: Some(209),
                        version2: None,
                        raw_payload: None,
                        parse_error: None,
                        subrecords: [
                            ("EDID", format!("LGN_{key}_Perk01\0").into_bytes()),
                            ("EPFD", multiplier.to_le_bytes().to_vec()),
                        ]
                        .into_iter()
                        .map(|(sig, data)| ParsedSubrecord {
                            signature: sig.into(),
                            data: Bytes::from(data),
                            semantic_type: None,
                        })
                        .collect(),
                    }));
            }
            source.invalidate_sections();
        }
        let mut session = open_session(target_id, Some(source_id)).unwrap();
        let schema = session.schema().unwrap();
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                source_plugin_name: "Source.esm".into(),
                output_plugin_name: "Output.esm".into(),
                ..MapperOptions::default()
            },
            &interner,
        );
        for index in 0..definitions.len() {
            let source_fk = FormKey {
                plugin: interner.intern("Source.esm"),
                local: 0x800 + index as u32,
            };
            let target_fk = FormKey {
                plugin: interner.intern("Output.esm"),
                local: 0x1800 + index as u32,
            };
            mapper.add_mapping(source_fk, target_fk);
            let mut perk = Record::new(SigCode(*b"PERK"), target_fk);
            perk.fields.push(raw(b"DATA", [0, 0, 1, 0, 1]));
            session.add_record(perk, &schema, &interner).unwrap();
        }
        let config = FixupConfig::default();
        assert_eq!(
            LegendaryPerksFixup
                .run_with_session(&mut session, &mut mapper, &config)
                .unwrap()
                .records_added,
            4
        );
        let mut named_records = Vec::new();
        for (key, multiplier) in definitions {
            for suffix in ["", "_Value"] {
                let eid = format!("B21_Legendary_{key}_1{suffix}");
                let fk = mapper
                    .lookup(FormKey {
                        plugin: interner.intern(&eid),
                        local: 1,
                    })
                    .unwrap();
                if suffix == "_Value" {
                    let bytes = session.first_subrecord_bytes(&fk, "FLTV").unwrap().unwrap();
                    assert_eq!(
                        f32::from_le_bytes(bytes[..].try_into().unwrap()),
                        multiplier
                    );
                }
                named_records.push((fk, eid));
            }
        }
        let repeated = LegendaryPerksFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .unwrap();
        assert_eq!(repeated.records_added, 0);
        assert_eq!(repeated.records_changed, 4);
        drop(session);
        assert_saved_editor_ids(target_id, &named_records, &interner);
        assert!(plugin_handle_close_native(source_id));
        assert!(plugin_handle_close_native(target_id));
    }

    fn assert_saved_editor_ids(
        target_id: u64,
        expected: &[(FormKey, String)],
        interner: &crate::sym::StringInterner,
    ) {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_load_no_py,
            plugin_handle_save_preserving_identity_no_py,
        };
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Output.esm");
        plugin_handle_save_preserving_identity_no_py(target_id, path.to_str().unwrap()).unwrap();
        let reopened =
            plugin_handle_load_no_py(path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let mut session = crate::session::open_session(reopened, None).unwrap();
        let schema = session.schema().unwrap();
        for (fk, eid) in expected {
            let record = session.record_decoded(fk, &schema, interner).unwrap();
            assert_eq!(
                record.eid.and_then(|symbol| interner.resolve(symbol)),
                Some(eid.as_str())
            );
            let edids: Vec<_> = record
                .fields
                .iter()
                .filter(|field| field.sig.0 == *b"EDID")
                .collect();
            assert_eq!(edids.len(), 1);
            assert_eq!(
                session
                    .first_subrecord_bytes(fk, "EDID")
                    .unwrap()
                    .as_deref(),
                Some(format!("{eid}\0").as_bytes())
            );
        }
        drop(session);
        assert!(plugin_handle_close_native(reopened));
    }
}
