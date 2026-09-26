use super::*;

struct FixtureReader(IndexMap<u32, SourceRecord>);
impl Reader for FixtureReader {
    fn records(&mut self, signature: &str) -> Vec<(u32, String)> {
        self.0
            .iter()
            .filter(|(_, record)| record.0 == signature)
            .map(|(id, record)| (*id, record.1.clone()))
            .collect()
    }
    fn fields(&mut self, id: u32) -> Result<SourceRecord, String> {
        self.0
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("unresolved source reference {id:06X}"))
    }
}

#[test]
fn matches_python_semantic_fixtures() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../../test_fixtures/challenges/python_parity.json"
    ))
    .unwrap();
    for (index, case) in cases.iter().enumerate() {
        let args = case["args"].as_array().unwrap();
        let actual: Result<Value, String> = match text(&case["operation"]) {
            "decode_condition" => decode_condition(&args[0], text(&case["kwargs"]["kind"])),
            "group_conditions" => Ok(json!(group_conditions(
                serde_json::from_value(args[0].clone()).unwrap()
            ))),
            "parse_challenge" => {
                let globals = args[2]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(id, value)| (id.parse().unwrap(), value.as_f64().unwrap_or(f64::NAN)))
                    .collect();
                parse_challenge(
                    text(&args[0]),
                    &serde_json::from_value(args[1].clone()).unwrap(),
                    &globals,
                )
            }
            "stat_kind" => stat_kind(text(&args[0])).map(|result| json!(result)),
            "reward" => Ok(reward(
                text(&args[0]),
                args[1].as_u64().unwrap(),
                args[2].as_bool().unwrap(),
            )),
            "prune" => {
                let mut entries: IndexMap<u32, Value> =
                    serde_json::from_value::<Vec<(u32, Value)>>(args[0].clone())
                        .unwrap()
                        .into_iter()
                        .collect();
                let mut dropped = serde_json::from_value(args[1].clone()).unwrap();
                prune(&mut entries, &mut dropped);
                Ok(json!([entries.into_iter().collect::<Vec<_>>(), dropped]))
            }
            "build_catalog" => {
                let mut reader = FixtureReader(
                    serde_json::from_value::<Vec<(u32, SourceRecord)>>(args[0].clone())
                        .unwrap()
                        .into_iter()
                        .collect(),
                );
                let table: HashMap<LookupKey, String> = args[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| {
                        (
                            (text(&row[0]).into(), text(&row[1]).into()),
                            text(&row[2]).into(),
                        )
                    })
                    .collect();
                build_catalog(
                    &mut reader,
                    &mut |sig, eid| table.get(&(sig.into(), eid.into())).cloned(),
                    text(&args[2]),
                    text(&args[3]),
                    text(&args[4]),
                )
                .map(|result| json!(result))
            }
            other => panic!("unknown case {other}"),
        };
        if let Some(expected) = case.get("error") {
            assert_eq!(
                actual.unwrap_err(),
                text(expected),
                "case {index}: {}",
                case["operation"]
            );
        } else {
            assert_eq!(
                actual.unwrap(),
                case["result"],
                "case {index}: {}",
                case["operation"]
            );
        }
    }
}

#[test]
fn condition_forms_preserve_context_and_reject_cycles_depth_and_comparisons() {
    let raw = |id: u32, flags: u32| {
        json!({"Type":flags,"Function":875,"RunOn":"Target",
        "ComparisonValue":{"variant":"comparison_value_float","value":1.0},
        "Parameter1":{"variant":"condition_form","value":{"reference":{"plugin":"SeventySix.esm","object_id":format!("{id:06X}")}}},
        "Parameter2":{"variant":"none","value":{}}})
    };
    let mut reader = FixtureReader(
        (1..=5)
            .map(|id| {
                (
                    id,
                    (
                        "CNDF".into(),
                        format!("Form{id}"),
                        BTreeMap::from([("CTDA".into(), vec![raw(id + 1, 0)])]),
                    ),
                )
            })
            .collect(),
    );
    let mut resolve = |_: &str, _: &str| None;
    let mut builder = Builder {
        reader: &mut reader,
        resolver: &mut resolve,
        source: "SeventySix.esm",
        output: "SeventySix.esm",
        forms: HashMap::new(),
        remaps: BTreeSet::new(),
    };
    let groups = |flags| group_conditions(vec![decode_condition(&raw(1, flags), "kill").unwrap()]);
    assert_eq!(
        builder
            .expand(groups(0), "kill", &mut Vec::new())
            .unwrap_err(),
        "condition form nesting deeper than 4"
    );
    assert_eq!(
        builder
            .expand(groups(0x40), "kill", &mut Vec::new())
            .unwrap_err(),
        "condition form comparison"
    );
    builder.forms.insert((1, "kill".into()), groups(0));
    assert_eq!(
        builder
            .expand(groups(0), "kill", &mut Vec::new())
            .unwrap_err(),
        "condition form cycle"
    );
    builder.forms.insert((1, "kill".into()), vec![]);
    assert_eq!(
        builder
            .expand(groups(0x20), "kill", &mut Vec::new())
            .unwrap(),
        vec![vec![
            json!({"fn":"group","on":"target","negate":true,"groups":[]})
        ]]
    );
}

#[test]
fn ascii_serialization_preserves_names_and_line_endings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.json");
    write_json(&path, json!({"z":"café 🐟\u{7f}","a":1.0})).unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "{\n  \"a\": 1.0,\n  \"z\": \"caf\\u00e9 \\ud83d\\udc1f\\u007f\"\n}\n"
    );
}

#[test]
fn batched_lookup_preserves_file_order_case_signature_and_master_ownership() {
    use esp_authoring_core::plugin_runtime::{
        ParsedItem, ParsedRecord, ParsedSubrecord, plugin_handle_save_no_py,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("Converted.esm");
    let handle = OwnedPluginHandle::new("Converted.esm", "fo4");
    {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store.get_mut(&handle.id()).unwrap();
        slot.parsed.header.masters = vec!["Fallout4.esm".into()];
        slot.parsed.root_items = [
            ("KYWD", 0x01000001, "Exact"),
            ("KYWD", 0x01000002, "Exact"),
            ("KYWD", 0x00000003, "Override"),
            ("RACE", 0x01000004, "Exact"),
            ("KYWD", 0x01000005, "exact"),
        ]
        .into_iter()
        .map(|(sig, id, eid)| {
            ParsedItem::Record(ParsedRecord {
                signature: sig.into(),
                form_id: id,
                flags: 0,
                version_control: 0,
                form_version: Some(131),
                version2: None,
                subrecords: vec![ParsedSubrecord {
                    signature: "EDID".into(),
                    data: format!("{eid}\0").into_bytes().into(),
                    semantic_type: None,
                }],
                raw_payload: None,
                parse_error: None,
            })
        })
        .collect();
    }
    plugin_handle_save_no_py(handle.id(), path.to_str().unwrap()).unwrap();
    let indexed = OwnedPluginHandle::load_index(&path, "fo4").unwrap();
    let queries = [
        ("KYWD", "Exact"),
        ("KYWD", "Override"),
        ("RACE", "Exact"),
        ("KYWD", "exact"),
        ("KYWD", "EXACT"),
    ]
    .map(|(sig, eid)| (sig.to_owned(), eid.to_owned()))
    .into_iter()
    .collect();
    let mut found = HashMap::new();
    {
        let store = plugin_handle_store_ref().lock().unwrap();
        batch_lookup(&store[&indexed.id()], &queries, &mut found).unwrap();
        assert_eq!(
            found.get(&("KYWD".into(), "Exact".into())).unwrap(),
            "Converted.esm:000001"
        );
        assert_eq!(
            found.get(&("KYWD".into(), "Override".into())).unwrap(),
            "Fallout4.esm:000003"
        );
        assert_eq!(
            found.get(&("RACE".into(), "Exact".into())).unwrap(),
            "Converted.esm:000004"
        );
        assert_eq!(
            found.get(&("KYWD".into(), "exact".into())).unwrap(),
            "Converted.esm:000005"
        );
        assert!(!found.contains_key(&("KYWD".into(), "EXACT".into())));
        let before = found.clone();
        batch_lookup(&store[&indexed.id()], &queries, &mut found).unwrap();
        assert_eq!(found, before);
    }
}
