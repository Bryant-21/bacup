use std::collections::{BTreeMap, BTreeSet};

use esp_authoring_core::plugin_runtime::{ParsedSubrecord, effective_subrecords_for_record};
use serde_json::{Value, json};

use super::{error, field, key, text, u32_value};
use crate::phase::PhaseError;
use crate::session::PluginSession;

pub(super) const ITEM_TYPES: &[&str] = &["AMMO", "ALCH", "WEAP"];
type NativeItems = BTreeMap<(String, String), BTreeMap<String, Value>>;

fn keyword_roots(
    cards: &BTreeMap<String, Value>,
    records: &BTreeMap<String, Value>,
    masters: &[String],
    plugin: &str,
) -> Result<BTreeSet<u32>, PhaseError> {
    let mut roots = BTreeSet::new();
    for card in cards.values() {
        for rank in card["ranks"]
            .as_array()
            .ok_or_else(|| error("weight card has no ranks"))?
        {
            let source = rank["source_perk"]
                .as_str()
                .ok_or_else(|| error("weight rank has no source"))?;
            let mut entry = false;
            let mut point = 0;
            let mut tab = 0;
            for field in records
                .get(source)
                .ok_or_else(|| error("weight source rank is missing"))?["fields"]
                .as_array()
                .ok_or_else(|| error("weight rank has no fields"))?
            {
                let sig = field["signature"].as_str().unwrap_or_default();
                let bytes = hex::decode(field["hex"].as_str().unwrap_or_default())
                    .map_err(|e| error(e.to_string()))?;
                match sig {
                    "PRKE" => {
                        entry = bytes.first() == Some(&2);
                        point = 0;
                        tab = 0;
                    }
                    "PRKF" => entry = false,
                    "DATA" if entry => point = bytes.first().copied().unwrap_or_default(),
                    "PRKC" if entry => tab = bytes.first().copied().unwrap_or_default(),
                    "CTDA" if entry && point == 158 && tab == 1 => {
                        if bytes.len() < 32 {
                            return Err(error("truncated weight condition"));
                        }
                        if u16::from_le_bytes(bytes[8..10].try_into().unwrap()) != 560 {
                            continue;
                        }
                        let keyword = u32_value(&bytes[12..16])?;
                        if records
                            .get(&key(keyword, masters, plugin)?)
                            .is_none_or(|r| r["signature"] != "KYWD")
                        {
                            return Err(error(
                                "weight condition keyword is absent from source closure",
                            ));
                        }
                        roots.insert(keyword);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(roots)
}

pub(super) fn collect(
    items: &[(u32, &str)],
    cards: &BTreeMap<String, Value>,
    records: &BTreeMap<String, Value>,
    read: impl Fn(u32) -> Option<Vec<ParsedSubrecord>>,
    masters: &[String],
    plugin: &str,
) -> Result<(Vec<Value>, BTreeSet<u32>), PhaseError> {
    let roots = keyword_roots(cards, records, masters, plugin)?;
    let mut rows = Vec::new();
    let mut references = BTreeSet::new();
    for &(raw, signature) in items {
        let Some(fields) = read(raw) else { continue };
        let mut keywords = BTreeSet::new();
        for entry in fields.iter().filter(|entry| entry.signature == "KWDA") {
            if entry.data.len() % 4 != 0 {
                return Err(error("invalid item keyword width"));
            }
            for bytes in entry.data.chunks_exact(4) {
                let keyword = u32_value(bytes)?;
                if roots.contains(&keyword) {
                    keywords.insert(keyword);
                }
            }
        }
        if keywords.is_empty() {
            continue;
        }
        references.insert(raw);
        references.extend(&keywords);
        rows.push(json!({"source_item": key(raw, masters, plugin)?, "signature": signature,
            "editor_id": field(&fields, "EDID").map(text).unwrap_or_default(),
            "keywords": keywords.into_iter().map(|id| key(id, masters, plugin)).collect::<Result<Vec<_>, _>>()?,
            "targets": []}));
    }
    Ok((rows, references))
}

pub(super) fn native_items(
    session: &mut PluginSession,
    handles: &[u64],
) -> Result<NativeItems, PhaseError> {
    let mut index = NativeItems::new();
    for handle in handles {
        let scan = session
            .handle_raw_scan(*handle)
            .map_err(|e| error(e.to_string()))?;
        for signature in ITEM_TYPES {
            for form_key in scan.form_keys_of_signature(signature) {
                let Some((raw, _, rendered)) = scan.record_metadata_by_form_key(&form_key) else {
                    continue;
                };
                let Some(editor_id) = scan
                    .with_record(raw, |r| {
                        (r.flags & 0x20 == 0).then(|| {
                            field(&effective_subrecords_for_record(r), "EDID")
                                .map(text)
                                .unwrap_or_default()
                        })
                    })
                    .flatten()
                    .filter(|name| !name.is_empty())
                else {
                    continue;
                };
                let (owner, id) = rendered
                    .rsplit_once(':')
                    .ok_or_else(|| error("invalid native weight item key"))?;
                index
                    .entry((signature.to_string(), editor_id.to_ascii_lowercase()))
                    .or_default()
                    .insert(
                        rendered.clone(),
                        json!({"plugin": owner, "object_id": id, "signature": signature,
                        "editor_id": editor_id, "provenance": "editor-id-and-signature"}),
                    );
            }
        }
    }
    Ok(index)
}

pub(super) fn bind(rows: &mut [Value], targets: &BTreeMap<String, Value>, native: &NativeItems) {
    for row in rows {
        let signature = row["signature"].as_str().unwrap();
        let mut bindings = BTreeMap::new();
        if let Some(target) = targets
            .get(row["source_item"].as_str().unwrap())
            .filter(|t| t["signature"] == signature)
        {
            let mut target = target.clone();
            target["provenance"] = json!("final-mapper");
            bindings.insert(
                format!(
                    "{}:{}",
                    target["plugin"].as_str().unwrap(),
                    target["object_id"].as_str().unwrap()
                ),
                target,
            );
        }
        if let Some(matches) = native.get(&(
            signature.to_string(),
            row["editor_id"].as_str().unwrap().to_ascii_lowercase(),
        )) {
            for (key, target) in matches {
                bindings
                    .entry(key.clone())
                    .or_insert_with(|| target.clone());
            }
        }
        row["targets"] = json!(bindings.into_values().collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(signature: &str, data: &[u8]) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: data.to_vec().into(),
            semantic_type: None,
        }
    }

    #[test]
    fn classifications_use_selected_weight_conditions_and_source_kwda_only() {
        let mut condition = vec![0; 32];
        condition[8..10].copy_from_slice(&560u16.to_le_bytes());
        condition[12..16].copy_from_slice(&0xA00u32.to_le_bytes());
        let fields = vec![
            sub("PRKE", &[2, 0, 0]),
            sub("DATA", &[158, 2, 2]),
            sub("PRKC", &[1]),
            sub("CTDA", &condition),
            sub("PRKF", &[]),
        ];
        let cards = BTreeMap::from([(
            "Card".into(),
            json!({"ranks":[{"source_perk":"000800:Source.esm"}]}),
        )]);
        let records = BTreeMap::from([
            (
                "000800:Source.esm".into(),
                json!({"fields":fields.iter().map(|f|json!({"signature":f.signature.as_str(),"hex":hex::encode(&f.data)})).collect::<Vec<_>>()}),
            ),
            ("000A00:Source.esm".into(), json!({"signature":"KYWD"})),
        ]);
        let items = BTreeMap::from([
            (
                1,
                vec![
                    sub("EDID", b"AmmoSame\0"),
                    sub("KWDA", &[0, 10, 0, 0, 1, 10, 0, 0]),
                ],
            ),
            (
                2,
                vec![sub("EDID", b"NoCategory\0"), sub("KWDA", &[1, 10, 0, 0])],
            ),
        ]);
        let (rows, refs) = collect(
            &[(1, "AMMO"), (2, "ALCH")],
            &cards,
            &records,
            |id| items.get(&id).cloned(),
            &[],
            "Source.esm",
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["keywords"], json!(["000A00:Source.esm"]));
        assert_eq!(refs, BTreeSet::from([1, 0xA00]));
    }

    #[test]
    fn final_mapper_and_exact_native_identity_allow_multiple_targets_without_local_id_guessing() {
        let mut rows = vec![
            json!({"source_item":"000001:Source.esm","signature":"AMMO","editor_id":"AmmoSame","keywords":["000A00:Source.esm"]}),
        ];
        let target =
            |id: &str, sig: &str| json!({"plugin":"Base.esm","object_id":id,"signature":sig});
        let native = NativeItems::from([
            (
                ("AMMO".into(), "ammosame".into()),
                BTreeMap::from([
                    ("Base.esm:000002".into(), target("000002", "AMMO")),
                    ("Base.esm:000003".into(), target("000003", "AMMO")),
                ]),
            ),
            (
                ("AMMO".into(), "differentname".into()),
                BTreeMap::from([("Base.esm:000001".into(), target("000001", "AMMO"))]),
            ),
            (
                ("WEAP".into(), "ammosame".into()),
                BTreeMap::from([("Base.esm:000004".into(), target("000004", "WEAP"))]),
            ),
        ]);
        bind(
            &mut rows,
            &BTreeMap::from([(
                "000001:Source.esm".into(),
                json!({"plugin":"Output.esm","object_id":"000100","signature":"AMMO"}),
            )]),
            &native,
        );
        assert_eq!(rows[0]["targets"].as_array().unwrap().len(), 3);
        assert!(
            rows[0]["targets"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["object_id"] != "000001" && t["signature"] == "AMMO")
        );
        bind(
            &mut rows,
            &BTreeMap::from([("000001:Source.esm".into(), target("000005", "WEAP"))]),
            &native,
        );
        assert_eq!(rows[0]["targets"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn native_identity_scan_uses_plugin_keys_and_rejects_deleted_records() {
        use crate::ids::{FormKey, SigCode, SubrecordSig};
        use crate::record::{FieldEntry, FieldValue, Record};
        use crate::session::open_session;
        use crate::sym::StringInterner;
        use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

        let handle = plugin_handle_new_native("NativeWeight.esm", Some("fo4")).unwrap();
        let mut session = open_session(handle, None).unwrap();
        session
            .target_slot_mut()
            .parsed
            .header
            .masters
            .push("Base.esm".into());
        let interner = StringInterner::new();
        let schema = session.schema().unwrap();
        for (id, signature, name, deleted) in [
            (0x801, "AMMO", "AmmoSame", false),
            (0x802, "ALCH", "AidSame", false),
            (0x803, "WEAP", "Gone", true),
        ] {
            let mut record = Record::new(
                SigCode::from_str(signature).unwrap(),
                FormKey {
                    local: id,
                    plugin: interner.intern("NativeWeight.esm"),
                },
            );
            if deleted {
                record.flags = crate::record::RecordFlags::from_bits_retain(0x20);
            }
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"EDID"),
                value: FieldValue::Bytes(format!("{name}\0").as_bytes().into()),
            });
            session.add_record(record, &schema, &interner).unwrap();
        }
        let native = native_items(&mut session, &[handle]).unwrap();
        assert_eq!(native.len(), 2);
        let ammo = native.get(&("AMMO".into(), "ammosame".into())).unwrap();
        let binding = ammo.get("NativeWeight.esm:000801").unwrap();
        assert_eq!(binding["plugin"], "NativeWeight.esm");
        assert_eq!(binding["object_id"], "000801");
        assert!(!native.contains_key(&("WEAP".into(), "gone".into())));
    }
}
