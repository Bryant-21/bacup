use super::*;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use esp_authoring_core::plugin_runtime::{
    clone_plugin_handle_state_no_py, insert_interior_cell_with_children, plugin_handle_new_native,
    plugin_handle_save_no_py,
};

fn new_target(localized: bool) -> u64 {
    let handle = plugin_handle_new_native("InteriorTest.esm", Some("fo4")).unwrap();
    let mut store = plugin_handle_store_ref().lock().unwrap();
    let slot = store.get_mut(&handle).unwrap();
    slot.parsed.header.masters = [
        "Fallout4.esm",
        "DLCRobot.esm",
        "DLCworkshop01.esm",
        "DLCCoast.esm",
        "DLCworkshop02.esm",
        "DLCworkshop03.esm",
        "DLCNukaWorld.esm",
        "FALLOUT4.ESM",
    ]
    .map(str::to_owned)
    .to_vec();
    slot.parsed.header.master_sizes = vec![0; slot.parsed.header.masters.len()];
    if localized {
        slot.parsed.header.flags |= 0x80;
    }
    handle
}

fn record(interner: &StringInterner, signature: &str, id: u32) -> Record {
    Record::new(
        SigCode::from_str(signature).unwrap(),
        FormKey {
            local: id,
            plugin: interner.intern("InteriorTest.esm"),
        },
    )
}

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).unwrap(),
        value,
    }
}

fn fixture(interner: &StringInterner, count: usize) -> (Record, Vec<Record>, Vec<Record>) {
    let mut cell = record(interner, "CELL", 0x800);
    cell.fields.push(field("DATA", FieldValue::Uint(1)));
    cell.fields.push(field(
        "FULL",
        FieldValue::String(interner.intern("Interior name")),
    ));
    let children: Vec<Record> = (0..count)
        .map(|index| {
            let mut child = record(interner, "REFR", 0x801 + index as u32);
            child.fields.push(field(
                "NAME",
                FieldValue::FormKey(FormKey {
                    local: 0x1000,
                    plugin: interner.intern(if index % 2 == 0 {
                        "fallout4.esm"
                    } else {
                        "dlccoast.ESM"
                    }),
                }),
            ));
            child.fields.push(field(
                "DATA",
                FieldValue::Bytes(SmallVec::from_slice(&[0; 24])),
            ));
            child.fields.push(field("XSCL", FieldValue::Float(1.0)));
            child
        })
        .collect();
    let split = count / 2;
    let mut persistent = children;
    let temporary = persistent.split_off(split);
    (cell, persistent, temporary)
}

fn insert_per_record(
    handle: u64,
    cell: Record,
    persistent: Vec<Record>,
    temporary: Vec<Record>,
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Result<InteriorInsertOutcome, WriteError> {
    let mut dropped = 0;
    let (cell, persistent, temporary) = {
        let mut store = plugin_handle_store_ref().lock().unwrap();
        let slot = store
            .get_mut(&handle)
            .ok_or_else(|| WriteError::InsertFailure(format!("no plugin handle: {handle}")))?;
        let Some(cell) = encode_record_for_slot(slot, cell, schema, interner)? else {
            return Ok(InteriorInsertOutcome {
                cell_inserted: false,
                children_inserted: 0,
                children_dropped: 0,
            });
        };
        let mut encode = |records: Vec<Record>| -> Result<Vec<ParsedRecord>, WriteError> {
            let mut parsed = Vec::with_capacity(records.len());
            for record in records {
                match encode_record_for_slot(slot, record, schema, interner)? {
                    Some(record) => parsed.push(record),
                    None => dropped += 1,
                }
            }
            Ok(parsed)
        };
        (cell, encode(persistent)?, encode(temporary)?)
    };
    let inserted = persistent.len() + temporary.len();
    insert_interior_cell_with_children(handle, cell, persistent, temporary)
        .map_err(WriteError::InsertFailure)?;
    apply_records_changed_effect(handle)?;
    Ok(InteriorInsertOutcome {
        cell_inserted: true,
        children_inserted: inserted as u32,
        children_dropped: dropped,
    })
}

#[test]
fn interior_batch_matches_per_record_encoding_and_localization() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    for localized in [false, true] {
        let old = new_target(localized);
        let new = new_target(localized);
        let (cell, mut persistent, mut temporary) = fixture(&interner, 12);
        persistent.insert(2, record(&interner, "XXXX", 0x900));
        temporary.insert(1, record(&interner, "XXXX", 0x901));
        for (children, text, id) in [
            (&mut persistent, "Persistent name", 0x902),
            (&mut temporary, "Temporary name", 0x903),
        ] {
            let mut named = record(&interner, "REFR", id);
            named
                .fields
                .push(field("FULL", FieldValue::String(interner.intern(text))));
            children.push(named);
        }
        let before = insert_per_record(
            old,
            cell.clone(),
            persistent.clone(),
            temporary.clone(),
            &schema,
            &interner,
        )
        .unwrap();
        let after = add_interior_cell_with_children_native(
            new, cell, persistent, temporary, &schema, &interner,
        )
        .unwrap();
        assert_eq!(before.cell_inserted, after.cell_inserted);
        assert_eq!(before.children_inserted, after.children_inserted);
        assert_eq!(before.children_dropped, 2);
        assert_eq!(before.children_dropped, after.children_dropped);
        let temp = tempfile::tempdir().unwrap();
        let old_path = temp.path().join("before.esm");
        let new_path = temp.path().join("after.esm");
        plugin_handle_save_no_py(old, old_path.to_str().unwrap()).unwrap();
        plugin_handle_save_no_py(new, new_path.to_str().unwrap()).unwrap();
        assert_eq!(
            std::fs::read(old_path).unwrap(),
            std::fs::read(new_path).unwrap()
        );
        let (_, old_strings) = clone_plugin_handle_state_no_py(old).unwrap();
        let (_, new_strings) = clone_plugin_handle_state_no_py(new).unwrap();
        assert_eq!(old_strings.by_language, new_strings.by_language);
        assert_eq!(old_strings.table_types, new_strings.table_types);
        assert_eq!(old_strings.default_language, new_strings.default_language);
        if localized {
            assert_eq!(new_strings.by_language["en"].len(), 1);
            assert_eq!(
                new_strings.by_language["en"].values().next().unwrap(),
                "Interior name"
            );
        }
        let mut store = plugin_handle_store_ref().lock().unwrap();
        store.remove(&old);
        store.remove(&new);
    }
}

#[test]
fn interior_batch_dropped_cell_does_not_encode_children() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let handle = new_target(true);
    let mut child = record(&interner, "CELL", 0x801);
    child.fields.push(field(
        "FULL",
        FieldValue::String(interner.intern("Do not allocate")),
    ));
    let outcome = add_interior_cell_with_children_native(
        handle,
        record(&interner, "XXXX", 0x800),
        vec![child],
        vec![],
        &schema,
        &interner,
    )
    .unwrap();
    assert!(!outcome.cell_inserted);
    assert_eq!(outcome.children_inserted, 0);
    let (plugin, strings) = clone_plugin_handle_state_no_py(handle).unwrap();
    assert!(plugin.root_items.is_empty());
    assert!(strings.by_language.values().all(HashMap::is_empty));
    plugin_handle_store_ref().lock().unwrap().remove(&handle);
}

fn benchmark(batched: bool) {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").unwrap();
    let handle = new_target(false);
    let (cell, persistent, temporary) = fixture(&interner, 100_000);
    let started = Instant::now();
    let insert = if batched {
        add_interior_cell_with_children_native
    } else {
        insert_per_record
    };
    let result = insert(handle, cell, persistent, temporary, &schema, &interner).unwrap();
    eprintln!(
        "interior_encode batched={batched} children={} elapsed_ms={:.3}",
        result.children_inserted,
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert_eq!(result.children_inserted, 100_000);
    plugin_handle_store_ref().lock().unwrap().remove(&handle);
}

#[test]
#[ignore = "bounded encoder benchmark; run with hyperfine"]
fn interior_batch_benchmark_before() {
    benchmark(false);
}

#[test]
#[ignore = "bounded encoder benchmark; run with hyperfine"]
fn interior_batch_benchmark_after() {
    benchmark(true);
}
