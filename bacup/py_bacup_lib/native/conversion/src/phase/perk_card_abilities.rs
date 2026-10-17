use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::StringInterner;

// These inherited names now contain FO76 rank conditions or different effects.
pub(crate) const PRESERVED_MAGIC: &[(u32, &str, &str)] = &[
    (0x04B252, "SPEL", "AbPerkStrongBack"),
    (0x04B268, "MGEF", "AbFortifyCharisma"),
    (0x04C47E, "SPEL", "AbPerkLeadBelly"),
    (0x04C93A, "SPEL", "AbPerkNightPerson"),
    (0x04D870, "MGEF", "AbFortifyActionPoints"),
    (0x04D871, "SPEL", "AbPerkActionBoyGirl"),
    (0x04D884, "SPEL", "AbPerkNerdRage"),
    (0x04D8A9, "SPEL", "AbPerkSolarPowered"),
    (0x05DD99, "MGEF", "FortifyLimbRegenRate"),
    (0x0CA9A0, "SPEL", "AbPerkRefractor"),
    (0x0E36FA, "MGEF", "AbWaterbreathing"),
    (0x0E36FB, "SPEL", "AbPerkAquaboy"),
    (0x1504F1, "MGEF", "abRestoreHealth"),
    (0x1D246C, "SPEL", "abPerkLoneWanderer"),
    (0x2387E2, "MGEF", "modWeapBleedEffect"),
    (0x50CA63, "SPEL", "AbPerkInspirational"),
    (0x006DC4, "SPEL", "PerkOverlyGenerousRadDamage"),
    (0x18731E, "SPEL", "AbPerkEMTHealthRegen03"),
    (0x1D33D6, "SPEL", "PerkStagger"),
    (0x320161, "SPEL", "PerkBulletShield"),
    (0x1E325A, "SPEL", "PerkSneakInvisibility"),
    (0x3896EF, "SPEL", "PerkHealingHandsSpell"),
    (0x38AB89, "SPEL", "PerkInjector01Spell"),
    (0x397CB6, "SPEL", "PerkRevenantSpell"),
    (0x43CEFE, "SPEL", "PerkTargetDebuffSpell"),
    (0x3E21F8, "SPEL", "PerkTenderizer01Spell"),
    (0x3E956B, "SPEL", "PerkFriendlyFire01Spell"),
    (0x51ABC3, "SPEL", "PerkFriendlyFire02Spell"),
    (0x51ABC5, "SPEL", "PerkFriendlyFire03Spell"),
];

pub(crate) fn preserves_magic_name(signature: &str, editor_id: &str) -> bool {
    PRESERVED_MAGIC.iter().any(|(_, sig, eid)| {
        signature.eq_ignore_ascii_case(sig) && editor_id.eq_ignore_ascii_case(eid)
    })
}

fn raw(sig: &str, data: &[u8]) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(sig).unwrap(),
        value: FieldValue::Bytes(data.into()),
    }
}

fn ability_blocks(fields: &[(String, Vec<u8>)]) -> Result<Vec<Vec<(String, Vec<u8>)>>, String> {
    let mut blocks = Vec::new();
    let mut current = None;
    for (sig, bytes) in fields {
        if sig == "PRKE" {
            if current.is_some() {
                return Err("unterminated perk ability entry".into());
            }
            current = Some(vec![(sig.clone(), bytes.clone())]);
        } else if let Some(block) = current.as_mut() {
            block.push((sig.clone(), bytes.clone()));
            if sig == "PRKF" {
                let block = current.take().unwrap();
                if block[0].1.first() == Some(&1)
                    || block
                        .iter()
                        .any(|(sig, data)| sig == "EPFT" && data.first() == Some(&5))
                {
                    blocks.push(block);
                }
            }
        }
    }
    if current.is_some() {
        return Err("unterminated perk ability entry".into());
    }
    Ok(blocks)
}

fn ability_id(block: &[(String, Vec<u8>)]) -> Result<u32, String> {
    let parameter = if block[0].1.first() == Some(&1) {
        "DATA"
    } else {
        "EPFD"
    };
    let bytes = block
        .iter()
        .find(|(sig, _)| sig == parameter)
        .ok_or("perk ability has no spell")?
        .1
        .as_slice();
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| "invalid ability spell width")?,
    ))
}

fn missing_blocks(
    root: &[Vec<(String, Vec<u8>)>],
    rank: &[(String, Vec<u8>)],
) -> Result<Vec<Vec<(String, Vec<u8>)>>, String> {
    let present = ability_blocks(rank)?
        .iter()
        .map(|block| ability_id(block))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let has_selected_spell = !selected_spell_parameters(rank)?.is_empty();
    root.iter()
        .filter_map(|block| match ability_id(block) {
            Ok(_) if block[0].1.first() != Some(&1) && has_selected_spell => None,
            Ok(id) if !present.contains(&id) => Some(Ok(block.clone())),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn binding(
    value: &Value,
    expected: &str,
    output: &str,
    interner: &StringInterner,
) -> Result<FormKey, String> {
    if value["signature"] != expected || value["plugin"] != output {
        return Err(format!(
            "regular perk ability requires source-owned {expected}: {value}"
        ));
    }
    FormKey::parse(
        &format!(
            "{}@{}",
            value["object_id"].as_str().ok_or("missing binding id")?,
            output
        ),
        interner,
    )
}

fn source_fields(
    records: &BTreeMap<String, Value>,
    key: &str,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    records.get(key).ok_or("missing source perk")?["fields"]
        .as_array()
        .ok_or("source perk has no fields")?
        .iter()
        .map(|field| {
            Ok((
                field["signature"]
                    .as_str()
                    .ok_or("missing source field signature")?
                    .to_owned(),
                hex::decode(field["hex"].as_str().ok_or("missing source field bytes")?)
                    .map_err(|e| e.to_string())?,
            ))
        })
        .collect()
}

fn target_fields(
    session: &mut PluginSession,
    handle: u64,
    raw: u32,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    session
        .handle_raw_scan(handle)
        .map_err(|e| e.to_string())?
        .with_record_subrecords(raw, |fields| {
            fields
                .iter()
                .map(|field| (field.signature.to_string(), field.data.to_vec()))
                .collect()
        })
        .ok_or("converted rank perk missing".into())
}

fn selected_spell_parameters(fields: &[(String, Vec<u8>)]) -> Result<Vec<(usize, u32)>, String> {
    let mut kind = None;
    let mut index = 0;
    let mut result = Vec::new();
    for (sig, bytes) in fields {
        match sig.as_str() {
            "PRKE" | "PRKF" => kind = None,
            "EPFT" => kind = bytes.first().copied(),
            "EPFD" => {
                if kind == Some(5) {
                    result.push((
                        index,
                        u32::from_le_bytes(
                            bytes
                                .as_slice()
                                .try_into()
                                .map_err(|_| "invalid SelectSpell width")?,
                        ),
                    ));
                }
                index += 1;
            }
            _ => {}
        }
    }
    Ok(result)
}

pub(crate) fn materialize(
    session: &mut PluginSession,
    target_handle: u64,
    cards: &BTreeMap<String, Value>,
    records: &BTreeMap<String, Value>,
    targets: &BTreeMap<String, Value>,
    source_masters: &[String],
    source_plugin: &str,
    output_plugin: &str,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<(Value, u32), String> {
    let target_masters = session.target_masters().to_vec();
    let mut manifest = BTreeMap::new();
    let mut changed = 0;
    let mapped_spell = |id: u32| -> Result<FormKey, String> {
        let owner = if (id >> 24) as usize == source_masters.len() {
            source_plugin
        } else {
            source_masters
                .get((id >> 24) as usize)
                .ok_or("invalid source ability master")?
        };
        let source = format!("{:06X}:{owner}", id & 0xffffff);
        binding(
            targets.get(&source).ok_or("unmapped regular perk spell")?,
            "SPEL",
            output_plugin,
            interner,
        )
    };
    for card in cards.values() {
        for rank in card["ranks"].as_array().ok_or("card has no ranks")? {
            let fields = source_fields(
                records,
                rank["source_perk"]
                    .as_str()
                    .ok_or("card has no source perk")?,
            )?;
            let source = selected_spell_parameters(&fields)?;
            if source.is_empty() {
                continue;
            }
            let perk = binding(&rank["perk"], "PERK", output_plugin, interner)?;
            let raw = encode_target_form_id(perk, interner, &target_masters)
                .ok_or("cannot encode rank perk")?;
            let actual = selected_spell_parameters(&target_fields(session, target_handle, raw)?)?;
            if actual.len() != source.len() {
                return Err("converted SelectSpell entry count differs from source".into());
            }
            let mut replacements = BTreeMap::new();
            for ((index, before), (_, source_id)) in actual.iter().zip(&source) {
                let after =
                    encode_target_form_id(mapped_spell(*source_id)?, interner, &target_masters)
                        .ok_or("cannot encode selected spell")?;
                if *before != after {
                    replacements.insert(*index, after);
                }
            }
            if !replacements.is_empty() {
                let mut record = session
                    .record_decoded(&perk, schema, interner)
                    .map_err(|e| e.to_string())?;
                for (index, field) in record
                    .fields
                    .iter_mut()
                    .filter(|field| field.sig.as_str() == "EPFD")
                    .enumerate()
                {
                    if let Some(id) = replacements.get(&index) {
                        field.value = self::raw("EPFD", &id.to_le_bytes()).value;
                    }
                }
                if !session
                    .replace_record_contents(record, schema, interner)
                    .map_err(|e| e.to_string())?
                {
                    return Err("converted rank disappeared during SelectSpell remapping".into());
                }
                changed += 1;
            }
        }
    }
    for (key, card) in cards {
        let ranks = card["ranks"].as_array().ok_or("card has no ranks")?;
        let source_key = ranks[0]["source_perk"]
            .as_str()
            .ok_or("card has no source perk")?;
        let root_fields = source_fields(records, source_key)?;
        let source_blocks = ability_blocks(&root_fields)?;
        let mut has_spells = !source_blocks.is_empty();
        for rank in ranks.iter().skip(1) {
            let fields = source_fields(
                records,
                rank["source_perk"]
                    .as_str()
                    .ok_or("rank has no source perk")?,
            )?;
            has_spells |= !ability_blocks(&fields)?.is_empty();
        }
        if !has_spells {
            continue;
        }
        let mut expected_ids = BTreeSet::new();
        for block in source_blocks {
            let id = ability_id(&block)?;
            let spell = mapped_spell(id)?;
            if session
                .record_decoded(&spell, schema, interner)
                .map_err(|e| e.to_string())?
                .sig
                .as_str()
                != "SPEL"
            {
                return Err("regular perk ability binding is not a spell".into());
            }
            expected_ids.insert(
                encode_target_form_id(spell, interner, &target_masters)
                    .ok_or("cannot encode regular perk ability")?,
            );
        }
        let root = binding(&ranks[0]["perk"], "PERK", output_plugin, interner)?;
        let root_raw = encode_target_form_id(root, interner, &target_masters)
            .ok_or("cannot encode root perk")?;
        let root_blocks = ability_blocks(&target_fields(session, target_handle, root_raw)?)?;
        let actual = root_blocks
            .iter()
            .map(|block| ability_id(block))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if actual != expected_ids {
            return Err(format!(
                "{key}: converted root ability does not match source spells"
            ));
        }
        let mut bindings = Vec::new();
        for rank in ranks {
            let perk = binding(&rank["perk"], "PERK", output_plugin, interner)?;
            let raw = encode_target_form_id(perk, interner, &target_masters)
                .ok_or("cannot encode rank perk")?;
            let additions =
                missing_blocks(&root_blocks, &target_fields(session, target_handle, raw)?)?;
            if !additions.is_empty() {
                let mut record = session
                    .record_decoded(&perk, schema, interner)
                    .map_err(|e| e.to_string())?;
                record.fields.extend(
                    additions
                        .iter()
                        .flatten()
                        .map(|(sig, data)| self::raw(sig, data)),
                );
                if !session
                    .replace_record_contents(record, schema, interner)
                    .map_err(|e| e.to_string())?
                {
                    return Err("converted rank disappeared during ability inheritance".into());
                }
                changed += 1;
            }
            let rank_spells = ability_blocks(&target_fields(session, target_handle, raw)?)?
                .iter()
                .map(|block| {
                    let id = ability_id(block)?;
                    if (id >> 24) as usize != target_masters.len() {
                        return Err("rank spell does not belong to converted output".to_owned());
                    }
                    Ok(format!("{:06X}:{output_plugin}", id & 0xffffff))
                })
                .collect::<Result<BTreeSet<_>, String>>()?;
            bindings.push(json!({"perk": format!("{:06X}:{output_plugin}", perk.local), "spells": rank_spells}));
        }
        manifest.insert(key, bindings);
    }
    Ok((json!({"version": 1, "cards": manifest}), changed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: u8, data: &[u8]) -> Vec<(String, Vec<u8>)> {
        vec![
            ("PRKE".into(), vec![kind, 0, 0]),
            ("DATA".into(), data.to_vec()),
            ("PRKF".into(), vec![]),
        ]
    }

    #[test]
    fn marker_ranks_inherit_abilities_without_stacking_root_entrypoints() {
        let ability = entry(1, &0x0804D871u32.to_le_bytes());
        let direct = entry(2, &[14, 2, 1]);
        let mut root = ability.clone();
        root.extend(direct.clone());
        let blocks = ability_blocks(&root).unwrap();
        assert_eq!(blocks, vec![ability.clone()]);
        assert_eq!(missing_blocks(&blocks, &direct).unwrap(), vec![ability]);
        assert_eq!(direct, entry(2, &[14, 2, 1]));
    }

    #[test]
    fn repeated_materialization_and_existing_spell_do_not_duplicate() {
        let ability = entry(1, &0x0831BE4Bu32.to_le_bytes());
        let blocks = ability_blocks(&ability).unwrap();
        assert!(missing_blocks(&blocks, &ability).unwrap().is_empty());
    }

    #[test]
    fn abilities_keep_conditions_and_reject_truncated_spell_ids() {
        let mut ability = entry(1, &0x0831BE4Bu32.to_le_bytes());
        ability.insert(2, ("CTDA".into(), vec![7; 32]));
        assert_eq!(ability_blocks(&ability).unwrap(), vec![ability]);
        assert!(ability_id(&entry(1, &[1, 2, 3])).is_err());
        assert!(ability_blocks(&[("PRKE".into(), vec![1])]).is_err());
    }

    #[test]
    fn select_spell_references_are_not_numeric_entrypoint_parameters() {
        let mut fields = entry(2, &[51, 10, 1]);
        fields.splice(
            2..2,
            [
                ("EPFT".into(), vec![5]),
                ("EPFD".into(), 0x1234u32.to_le_bytes().to_vec()),
            ],
        );
        let selected = ability_blocks(&fields).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(ability_id(&selected[0]).unwrap(), 0x1234);
        assert_eq!(
            selected_spell_parameters(&fields).unwrap(),
            vec![(0, 0x1234)]
        );
        fields[2].1 = vec![1];
        assert!(ability_blocks(&fields).unwrap().is_empty());
        assert!(selected_spell_parameters(&fields).unwrap().is_empty());
    }

    #[test]
    fn higher_rank_selected_spell_replaces_root_spell_without_duplicating_effects() {
        let selected = |id: u32| {
            let mut fields = entry(2, &[51, 10, 1]);
            fields.splice(
                2..2,
                [
                    ("EPFT".into(), vec![5]),
                    ("EPFD".into(), id.to_le_bytes().to_vec()),
                ],
            );
            fields
        };
        let root = ability_blocks(&selected(0x100)).unwrap();
        assert!(missing_blocks(&root, &selected(0x200)).unwrap().is_empty());
        assert_eq!(missing_blocks(&root, &entry(2, &[14, 2, 1])).unwrap(), root);
    }

    #[test]
    fn production_materialization_remaps_selected_spells_and_preserves_higher_direct_effects() {
        use crate::ids::SigCode;
        use crate::record::Record;
        use crate::session::open_session;
        use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

        let interner = StringInterner::new();
        let target = plugin_handle_new_native("AbilityOutput.esm", Some("fo4")).unwrap();
        let mut session = open_session(target, None).unwrap();
        session
            .target_slot_mut()
            .parsed
            .header
            .masters
            .push("Fallout4.esm".into());
        let schema = session.schema().unwrap();
        let fk = |id| FormKey {
            local: id,
            plugin: interner.intern("AbilityOutput.esm"),
        };
        for id in [0xA00, 0xA01, 0xA02] {
            let mut spell = Record::new(SigCode(*b"SPEL"), fk(id));
            spell.fields.push(raw("SPIT", &[0; 36]));
            session.add_record(spell, &schema, &interner).unwrap();
        }
        let mut ability = entry(1, &0x01000A00u32.to_le_bytes());
        let mut selected = entry(2, &[51, 10, 1]);
        selected.splice(
            2..2,
            [
                ("EPFT".into(), vec![5]),
                ("EPFD".into(), 0xA01u32.to_le_bytes().to_vec()),
            ],
        );
        ability.extend(selected);
        let direct = entry(2, &[14, 2, 1]);
        let mut replacement = entry(2, &[51, 10, 1]);
        replacement.splice(
            2..2,
            [
                ("EPFT".into(), vec![5]),
                ("EPFD".into(), 0xA02u32.to_le_bytes().to_vec()),
            ],
        );
        for (id, entries) in [
            (0x900, ability.clone()),
            (0x901, direct.clone()),
            (0x902, direct.clone()),
            (0x903, replacement.clone()),
        ] {
            let mut perk = Record::new(SigCode(*b"PERK"), fk(id));
            perk.fields.push(raw("DATA", &[0, 0, 1, 0, 1]));
            perk.fields
                .extend(entries.iter().map(|(sig, data)| raw(sig, data)));
            session.add_record(perk, &schema, &interner).unwrap();
        }
        let mut source_entries = ability;
        source_entries[1].1 = 0xA00u32.to_le_bytes().to_vec();
        let source_json = |fields: Vec<(String, Vec<u8>)>| {
            json!({"fields": fields.into_iter()
            .map(|(signature, bytes)| json!({"signature": signature, "hex": hex::encode(bytes)})).collect::<Vec<_>>()})
        };
        let records = BTreeMap::from([
            ("000900:Source.esm".into(), source_json(source_entries)),
            ("000901:Source.esm".into(), source_json(direct.clone())),
            ("000902:Source.esm".into(), source_json(direct.clone())),
            ("000903:Source.esm".into(), source_json(replacement)),
        ]);
        let targets = BTreeMap::from([
            (
                "000A00:Source.esm".into(),
                json!({"plugin":"AbilityOutput.esm", "object_id":"000A00", "signature":"SPEL"}),
            ),
            (
                "000A01:Source.esm".into(),
                json!({"plugin":"AbilityOutput.esm", "object_id":"000A01", "signature":"SPEL"}),
            ),
            (
                "000A02:Source.esm".into(),
                json!({"plugin":"AbilityOutput.esm", "object_id":"000A02", "signature":"SPEL"}),
            ),
        ]);
        let cards = BTreeMap::from([
            (
                "000800:Source.esm".into(),
                json!({"ranks":[
            {"source_perk":"000900:Source.esm", "perk":{"plugin":"AbilityOutput.esm", "object_id":"000900", "signature":"PERK"}},
            {"source_perk":"000901:Source.esm", "perk":{"plugin":"AbilityOutput.esm", "object_id":"000901", "signature":"PERK"}}]}),
            ),
            (
                "000810:Source.esm".into(),
                json!({"ranks":[
            {"source_perk":"000902:Source.esm", "perk":{"plugin":"AbilityOutput.esm", "object_id":"000902", "signature":"PERK"}},
            {"source_perk":"000903:Source.esm", "perk":{"plugin":"AbilityOutput.esm", "object_id":"000903", "signature":"PERK"}}]}),
            ),
        ]);
        let (manifest, changed) = materialize(
            &mut session,
            target,
            &cards,
            &records,
            &targets,
            &[],
            "Source.esm",
            "AbilityOutput.esm",
            &schema,
            &interner,
        )
        .unwrap();
        assert_eq!(changed, 3);
        assert_eq!(
            manifest["cards"]["000810:Source.esm"][0]["spells"],
            json!([])
        );
        assert_eq!(
            manifest["cards"]["000810:Source.esm"][1]["spells"],
            json!(["000A02:AbilityOutput.esm"])
        );
        assert_eq!(
            manifest["cards"]["000800:Source.esm"][1]["spells"],
            json!(["000A00:AbilityOutput.esm", "000A01:AbilityOutput.esm"])
        );
        let repaired = target_fields(&mut session, target, 0x01000901).unwrap();
        assert_eq!(ability_blocks(&repaired).unwrap().len(), 2);
        assert!(
            repaired
                .windows(direct.len())
                .any(|fields| fields == direct)
        );
        assert_eq!(
            selected_spell_parameters(&repaired).unwrap(),
            vec![(0, 0x01000A01)]
        );
        let (_, repeated) = materialize(
            &mut session,
            target,
            &cards,
            &records,
            &targets,
            &[],
            "Source.esm",
            "AbilityOutput.esm",
            &schema,
            &interner,
        )
        .unwrap();
        assert_eq!(repeated, 0);
    }
}
