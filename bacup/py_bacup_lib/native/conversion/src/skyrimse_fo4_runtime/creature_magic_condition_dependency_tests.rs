#[test]
fn magic_condition_dependency_edges_exclude_only_lowered_actor_value_operands() {
    let interner = StringInterner::new();
    let health = FormKey::parse("0002D4@Fallout4.esm", &interner).unwrap();
    let magicka = FormKey::parse("000001@B21_SkyrimMagicRuntime.esm", &interner).unwrap();
    let unknown_runtime = FormKey::parse("00FFFF@B21_SkyrimMagicRuntime.esm", &interner).unwrap();
    let source_health_number = key(&interner, 0x2D4);
    let mut spell = record(&interner, "SPEL", 0x800);
    for (function, operand) in [
        (640, health),
        (640, magicka),
        (640, unknown_runtime),
        (640, source_health_number),
        (560, health),
    ] {
        spell.fields.push(FieldEntry {
            sig: SubrecordSig(*b"CTDA"),
            value: FieldValue::Struct(vec![
                (interner.intern("function"), FieldValue::Uint(function)),
                (interner.intern("parameter_1"), FieldValue::FormKey(operand)),
                (
                    interner.intern("reference"),
                    FieldValue::FormKey(key(&interner, 0x14)),
                ),
            ]),
        });
    }
    let records = [(spell.form_key, &spell)].into_iter().collect();
    let references = record_references(&spell, &records, &interner);
    let operands = references
        .iter()
        .filter(|edge| edge.field_path.ends_with(".parameter_1"))
        .map(|edge| (edge.field_path.as_str(), edge.target))
        .collect::<HashSet<_>>();
    assert_eq!(
        operands,
        HashSet::from([
            ("CTDA[2].parameter_1", unknown_runtime),
            ("CTDA[3].parameter_1", source_health_number),
            ("CTDA[4].parameter_1", health),
        ])
    );
    assert_eq!(
        references
            .iter()
            .filter(|edge| edge.field_path.ends_with(".reference"))
            .count(),
        5
    );
}

#[test]
#[ignore = "requires the full source Skyrim creature ability corpus and installed source plugins"]
fn source_magic_condition_dependency_closure_keeps_source_predicates() {
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_load_no_py,
    };
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::env::var("MODKIT_SKYRIM_ABILITY_MAGIC_INPUTS")
                .expect("source ability corpus manifest"),
        )
        .unwrap(),
    )
    .unwrap();
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("skyrimse").unwrap();
    let (mut records, mut catalog) = fixture(&interner);
    let mut handles = HashMap::new();
    let mut expected_source_edges = HashSet::new();
    let mut lowered_operands = Vec::new();
    for input in manifest.as_array().unwrap() {
        let path = input["path"].as_str().unwrap();
        let handle = *handles.entry(path.to_string()).or_insert_with(|| {
            plugin_handle_load_no_py(path, Some("skyrimse"), None, None, true).unwrap()
        });
        let source_key = input["key"].as_str().unwrap().replace(':', "@");
        let source =
            crate::source_read::read_record(handle, &source_key, &schema, &interner).unwrap();
        for (index, field) in source
            .fields
            .iter()
            .enumerate()
            .filter(|(_, field)| field.sig.0 == *b"CTDA")
        {
            let FieldValue::Struct(members) = &field.value else {
                panic!("typed condition expected in {source_key}")
            };
            let function = members
                .iter()
                .find(|(name, _)| interner.resolve(*name) == Some("function"))
                .unwrap();
            for (name, value) in members {
                let FieldValue::FormKey(target) = value else {
                    continue;
                };
                let name = interner.resolve(*name).unwrap();
                let path = format!("CTDA[{index}].{name}");
                let is_lowered = function.1 == FieldValue::Uint(640)
                    && name == "parameter_1"
                    && matches!(
                        (interner.resolve(target.plugin).unwrap(), target.local),
                        ("Fallout4.esm", 0x2D4) | ("B21_SkyrimMagicRuntime.esm", 1)
                    );
                if is_lowered {
                    lowered_operands.push((source.form_key, path, *target));
                } else {
                    expected_source_edges.insert((source.form_key, path, *target));
                }
            }
        }
        if source.sig.0 == *b"SPEL" {
            catalog.races[0].attack_spells.push(source.form_key);
        }
        records.push(source);
    }
    for handle in handles.into_values() {
        assert!(plugin_handle_close_native(handle));
    }
    let ledger = build_skyrim_creature_dependency_ledger(&records, &catalog, &interner).unwrap();
    let actual_edges = ledger
        .records
        .iter()
        .flat_map(|record| record.references.iter())
        .filter(|edge| edge.field_path.starts_with("CTDA["))
        .map(|edge| (edge.owner.form_key, edge.field_path.clone(), edge.target))
        .collect::<HashSet<_>>();
    assert_eq!(
        actual_edges, expected_source_edges,
        "the real creature closure must preserve every source condition reference"
    );
    assert_eq!(lowered_operands.len(), 7);
    for (owner, path, target) in lowered_operands {
        assert!(ledger.candidates[0].closure.contains(&owner));
        assert!(!ledger.candidates[0].closure.contains(&target));
        assert!(!actual_edges.contains(&(owner, path, target)));
    }
}
