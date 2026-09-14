use conversion_native::ids::{FormKey, SigCode, SubrecordSig};
use conversion_native::record::{FieldEntry, FieldValue, Record, RecordFlags};
use conversion_native::schema::AuthoringSchema;
use conversion_native::sym::{StringInterner, Sym};
use conversion_native::target_write::{
    add_quest_child_record_native, add_record_native, add_topic_child_record_native,
};
use esp_authoring_core::plugin_runtime::{
    ParsedGroup, ParsedItem, ParsedRecord, plugin_handle_close_native, plugin_handle_load_no_py,
    plugin_handle_new_no_py, plugin_handle_save_no_py, plugin_handle_store_ref,
};
use smallvec::SmallVec;

const QUEST: u32 = 0x0008_1000;
const DIALOGUE: u32 = 0x0008_1001;
const INFO: u32 = 0x0008_1002;
const SCENE: u32 = 0x0008_1003;
const BRANCH: u32 = 0x0008_1004;
const VIEW: u32 = 0x0008_1005;
const EVENT: u32 = 0x0008_1006;
const EVENT_BRANCH: u32 = 0x0008_1007;
const EVENT_QUEST: u32 = 0x0008_1008;

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).expect("literal subrecord signature"),
        value,
    }
}

fn form(plugin: Sym, local: u32) -> FieldValue {
    FieldValue::FormKey(FormKey { local, plugin })
}

fn record(
    signature: &str,
    plugin: Sym,
    local: u32,
    editor_id: &str,
    interner: &StringInterner,
) -> Record {
    let mut record = Record::new(
        SigCode::from_str(signature).expect("literal record signature"),
        FormKey { local, plugin },
    );
    record.eid = Some(interner.intern(editor_id));
    record.flags = RecordFlags::empty();
    record
}

fn raw(record: &ParsedRecord, signature: &str) -> Vec<Vec<u8>> {
    record
        .subrecords
        .iter()
        .filter(|subrecord| subrecord.signature.as_str() == signature)
        .map(|subrecord| subrecord.data.to_vec())
        .collect()
}

fn record_in<'a>(items: &'a [ParsedItem], signature: &str, local: u32) -> &'a ParsedRecord {
    items
        .iter()
        .find_map(|item| match item {
            ParsedItem::Record(record)
                if record.signature.as_str() == signature
                    && (record.form_id & 0x00FF_FFFF) == local =>
            {
                Some(record)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{signature} {local:06X}"))
}

fn top_group<'a>(items: &'a [ParsedItem], signature: &[u8; 4]) -> &'a ParsedGroup {
    items
        .iter()
        .find_map(|item| match item {
            ParsedItem::Group(group) if group.group_type == 0 && group.label == *signature => {
                Some(group)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("top-level {} group", String::from_utf8_lossy(signature)))
}

fn child_group<'a>(items: &'a [ParsedItem], group_type: i32, label: u32) -> &'a ParsedGroup {
    items
        .iter()
        .find_map(|item| match item {
            ParsedItem::Group(group)
                if group.group_type == group_type && group.label == label.to_le_bytes() =>
            {
                Some(group)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("group {group_type} for {label:08X}"))
}

#[test]
fn fo4_quest_runtime_topology_survives_save_reopen_and_keeps_dialogue_view_top_level() {
    let interner = StringInterner::new();
    let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema");
    let plugin = interner.intern("QuestRuntimeTopology.esm");
    let handle = plugin_handle_new_no_py("QuestRuntimeTopology.esm", Some("fo4"));

    let mut quest = record(
        "QUST",
        plugin,
        QUEST,
        "QuestRuntimeTopologyQuest",
        &interner,
    );
    quest.fields.push(field(
        "DATA",
        FieldValue::Bytes(SmallVec::from_slice(&[1; 16])),
    ));
    add_record_native(handle, quest, &schema, &interner).expect("write QUST");

    let mut dialogue = record(
        "DIAL",
        plugin,
        DIALOGUE,
        "QuestRuntimeTopologyTopic",
        &interner,
    );
    dialogue.fields.extend([
        field("QNAM", form(plugin, QUEST)),
        field("BNAM", form(plugin, BRANCH)),
        field("DATA", FieldValue::Bytes(SmallVec::from_slice(&[0; 4]))),
    ]);
    assert!(
        add_quest_child_record_native(handle, dialogue, &schema, &interner).expect("write DIAL")
    );

    let mut info = record("INFO", plugin, INFO, "QuestRuntimeTopologyInfo", &interner);
    let mut condition = SmallVec::<[u8; 32]>::new();
    condition.resize(32, 0);
    condition[4..8].copy_from_slice(&1.0_f32.to_le_bytes());
    info.fields.extend([
        field("CTDA", FieldValue::Bytes(condition)),
        field(
            "CIS1",
            FieldValue::String(interner.intern("TopologyCondition")),
        ),
    ]);
    assert!(
        add_topic_child_record_native(handle, info, DIALOGUE, &schema, &interner)
            .expect("write INFO")
    );

    let mut scene = record(
        "SCEN",
        plugin,
        SCENE,
        "QuestRuntimeTopologyScene",
        &interner,
    );
    scene.fields.push(field("PNAM", form(plugin, QUEST)));
    assert!(add_quest_child_record_native(handle, scene, &schema, &interner).expect("write SCEN"));

    let mut branch = record(
        "DLBR",
        plugin,
        BRANCH,
        "QuestRuntimeTopologyBranch",
        &interner,
    );
    branch.fields.extend([
        field("QNAM", form(plugin, QUEST)),
        field("SNAM", form(plugin, DIALOGUE)),
    ]);
    assert!(add_quest_child_record_native(handle, branch, &schema, &interner).expect("write DLBR"));

    // FO4's canonical group order places DLVW after MUST at the top level. It
    // carries a QNAM quest reference, but that reference is not topology.
    let mut view = record("DLVW", plugin, VIEW, "QuestRuntimeTopologyView", &interner);
    view.fields.push(field("QNAM", form(plugin, QUEST)));
    add_record_native(handle, view, &schema, &interner).expect("write DLVW");

    let mut event = record(
        "SMEN",
        plugin,
        EVENT,
        "QuestRuntimeTopologyEvent",
        &interner,
    );
    event.fields.push(field(
        "ENAM",
        FieldValue::Bytes(SmallVec::from_slice(b"SCPT")),
    ));
    add_record_native(handle, event, &schema, &interner).expect("write SMEN");

    let mut event_branch = record(
        "SMBN",
        plugin,
        EVENT_BRANCH,
        "QuestRuntimeTopologyEventBranch",
        &interner,
    );
    event_branch.fields.push(field("PNAM", form(plugin, EVENT)));
    add_record_native(handle, event_branch, &schema, &interner).expect("write SMBN");

    let mut event_quest = record(
        "SMQN",
        plugin,
        EVENT_QUEST,
        "QuestRuntimeTopologyEventQuest",
        &interner,
    );
    event_quest.fields.extend([
        field("PNAM", form(plugin, EVENT_BRANCH)),
        field("NNAM", form(plugin, QUEST)),
    ]);
    add_record_native(handle, event_quest, &schema, &interner).expect("write SMQN");

    let directory = tempfile::tempdir().expect("temporary output directory");
    let path = directory.path().join("QuestRuntimeTopology.esm");
    plugin_handle_save_no_py(handle, path.to_str().expect("UTF-8 output path")).expect("save");
    plugin_handle_close_native(handle);
    let reopened = plugin_handle_load_no_py(
        path.to_str().expect("UTF-8 output path"),
        Some("fo4"),
        None,
        None,
        true,
    )
    .expect("reopen FO4 fixture");

    let store = plugin_handle_store_ref().lock().expect("plugin store");
    let slot = store.get(&reopened).expect("reopened fixture slot");
    let root = &slot.parsed.root_items;

    let quest_group = top_group(root, b"QUST");
    record_in(&quest_group.children, "QUST", QUEST);
    let quest_children = child_group(&quest_group.children, 10, QUEST);
    let dial = record_in(&quest_children.children, "DIAL", DIALOGUE);
    record_in(&quest_children.children, "DLBR", BRANCH);
    record_in(&quest_children.children, "SCEN", SCENE);
    let topic_children = child_group(&quest_children.children, 7, DIALOGUE);
    let info = record_in(&topic_children.children, "INFO", INFO);

    let view_group = top_group(root, b"DLVW");
    let view = record_in(&view_group.children, "DLVW", VIEW);
    assert!(
        !quest_children.children.iter().any(|item| matches!(
            item,
            ParsedItem::Record(record) if record.signature.as_str() == "DLVW"
        )),
        "DLVW must remain top-level rather than becoming a quest child"
    );

    let event_group = top_group(root, b"SMEN");
    record_in(&event_group.children, "SMEN", EVENT);
    let event_branch_group = top_group(root, b"SMBN");
    let event_branch = record_in(&event_branch_group.children, "SMBN", EVENT_BRANCH);
    let event_quest_group = top_group(root, b"SMQN");
    let event_quest = record_in(&event_quest_group.children, "SMQN", EVENT_QUEST);

    assert_eq!(raw(dial, "BNAM"), [BRANCH.to_le_bytes().to_vec()]);
    assert_eq!(
        raw(record_in(&quest_children.children, "DLBR", BRANCH), "QNAM"),
        [QUEST.to_le_bytes().to_vec()]
    );
    assert_eq!(
        raw(record_in(&quest_children.children, "DLBR", BRANCH), "SNAM"),
        [DIALOGUE.to_le_bytes().to_vec()]
    );
    assert_eq!(
        raw(record_in(&quest_children.children, "SCEN", SCENE), "PNAM"),
        [QUEST.to_le_bytes().to_vec()]
    );
    assert_eq!(raw(view, "QNAM"), [QUEST.to_le_bytes().to_vec()]);
    assert_eq!(raw(event_branch, "PNAM"), [EVENT.to_le_bytes().to_vec()]);
    assert_eq!(
        raw(event_quest, "PNAM"),
        [EVENT_BRANCH.to_le_bytes().to_vec()]
    );
    assert_eq!(raw(event_quest, "NNAM"), [QUEST.to_le_bytes().to_vec()]);
    assert_eq!(raw(info, "CTDA").len(), 1);
    assert_eq!(raw(info, "CIS1"), [b"TopologyCondition\0".to_vec()]);

    drop(store);
    plugin_handle_close_native(reopened);
}
