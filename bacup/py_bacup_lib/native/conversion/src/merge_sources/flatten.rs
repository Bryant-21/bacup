use std::collections::{HashMap, HashSet};

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{
    CompiledSchema, ParsedGroup, ParsedItem, ParsedPlugin, ParsedRecord, ParsedSubrecord,
    clone_plugin_handle_state_no_py, compiled_schema_for_game_str,
    editor_id_from_effective_subrecords,
};
use smol_str::SmolStr;

use super::MergeError;
use super::graft::merge_top_group;
use super::repoint::rewrite_merge_formids_in_subrecords;

pub(crate) struct FlattenedLineage {
    pub tree: Vec<ParsedItem>,
    pub used_ids: HashSet<u32>,
    pub eid_index: HashMap<(String, SmolStr), u32>,
    pub template: ParsedPlugin,
}

struct PreparedPlugin {
    plugin: ParsedPlugin,
    raw_to_output: HashMap<u32, u32>,
    overrides: HashSet<u32>,
}

pub(crate) fn flatten_lineage(handles: &[u64]) -> Result<FlattenedLineage, MergeError> {
    let mut plugins = Vec::with_capacity(handles.len());
    for handle in handles {
        plugins.push(
            clone_plugin_handle_state_no_py(*handle)
                .map_err(MergeError::Load)?
                .0,
        );
    }
    let Some(template) = plugins.first().cloned() else {
        return Err(MergeError::Load("lineage has no plugins".to_string()));
    };
    validate_plugins(&plugins)?;

    let mut tree = Vec::new();
    let mut used_ids = HashSet::new();
    let mut next_candidate = 0x800;
    let mut identities: HashMap<(String, u32), u32> = HashMap::new();
    let mut override_winners = HashMap::new();
    let mut container_appends = HashMap::new();
    let lineage_names: Vec<String> = plugins
        .iter()
        .map(|plugin| plugin.plugin_name.to_lowercase())
        .collect();
    let mut prepared = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let plugin_name = plugin.plugin_name.to_lowercase();
        let mut raw_to_output = HashMap::new();
        let mut overrides = HashSet::new();
        allocate_records(
            &plugin.root_items,
            &plugin_name,
            &plugin.header.masters,
            &mut identities,
            &mut used_ids,
            &mut next_candidate,
            &mut raw_to_output,
            &mut overrides,
        )?;
        prepared.push(PreparedPlugin {
            plugin,
            raw_to_output,
            overrides,
        });
    }
    for plugin in prepared {
        merge_prepared_plugin(
            &mut tree,
            &identities,
            &lineage_names,
            &mut override_winners,
            &mut container_appends,
            plugin,
        )?;
    }
    apply_container_appends(&mut tree, &mut container_appends, template.header_size);
    apply_override_winners(&mut tree, &override_winners);
    let mut eid_index = HashMap::new();
    collect_eids(&tree, &mut eid_index);
    Ok(FlattenedLineage {
        tree,
        used_ids,
        eid_index,
        template,
    })
}

fn validate_plugins(plugins: &[ParsedPlugin]) -> Result<(), MergeError> {
    let mut earlier = HashSet::new();
    for plugin in plugins {
        for master in &plugin.header.masters {
            if !earlier.contains(&master.to_lowercase()) {
                return Err(MergeError::UnknownMaster(format!(
                    "{} requires {master}",
                    plugin.plugin_name
                )));
            }
        }
        earlier.insert(plugin.plugin_name.to_lowercase());
    }
    Ok(())
}

fn merge_prepared_plugin(
    output: &mut Vec<ParsedItem>,
    identities: &HashMap<(String, u32), u32>,
    lineage_names: &[String],
    override_winners: &mut HashMap<u32, ParsedRecord>,
    container_appends: &mut HashMap<u32, (i32, Vec<ParsedItem>)>,
    prepared: PreparedPlugin,
) -> Result<(), MergeError> {
    let PreparedPlugin {
        plugin,
        raw_to_output,
        overrides,
    } = prepared;
    let plugin_name = plugin.plugin_name.to_lowercase();
    let schema = plugin
        .game
        .as_deref()
        .map(compiled_schema_for_game_str)
        .transpose()
        .map_err(MergeError::Load)?;
    let own_index = plugin.header.masters.len() as u8;
    let transformed = transform_plugin_items(
        plugin.root_items,
        &plugin_name,
        &plugin.header.masters,
        own_index,
        identities,
        lineage_names,
        &raw_to_output,
        &overrides,
        override_winners,
        container_appends,
        schema.as_deref(),
    )?;
    for item in transformed {
        match item {
            ParsedItem::Group(group) => merge_top_group(output, group),
            ParsedItem::Record(record) => output.push(ParsedItem::Record(record)),
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn allocate_records(
    items: &[ParsedItem],
    plugin_name: &str,
    masters: &[String],
    identities: &mut HashMap<(String, u32), u32>,
    used_ids: &mut HashSet<u32>,
    next_candidate: &mut u32,
    raw_to_output: &mut HashMap<u32, u32>,
    overrides: &mut HashSet<u32>,
) -> Result<(), MergeError> {
    for item in items {
        match item {
            ParsedItem::Group(group) => allocate_records(
                &group.children,
                plugin_name,
                masters,
                identities,
                used_ids,
                next_candidate,
                raw_to_output,
                overrides,
            )?,
            ParsedItem::Record(record) => {
                let source_index = (record.form_id >> 24) as u8;
                let object_id = record.form_id & 0x00FF_FFFF;
                let output_id = if (source_index as usize) < masters.len() {
                    let key = (masters[source_index as usize].to_lowercase(), object_id);
                    if let Some(&output_id) = identities.get(&key) {
                        overrides.insert(record.form_id);
                        output_id
                    } else {
                        let output_id = allocate_id(object_id, used_ids, next_candidate);
                        identities.insert(key, output_id);
                        output_id
                    }
                } else {
                    let key = (plugin_name.to_string(), object_id);
                    if let Some(&output_id) = identities.get(&key) {
                        output_id
                    } else {
                        let output_id = allocate_id(object_id, used_ids, next_candidate);
                        identities.insert(key, output_id);
                        output_id
                    }
                };
                raw_to_output.insert(record.form_id, output_id);
            }
        }
    }
    Ok(())
}

fn allocate_id(preferred: u32, used_ids: &mut HashSet<u32>, next_candidate: &mut u32) -> u32 {
    if used_ids.insert(preferred) {
        if preferred >= *next_candidate {
            *next_candidate = preferred.saturating_add(1);
        }
        return preferred;
    }
    *next_candidate = (*next_candidate).max(0x800);
    while used_ids.contains(next_candidate) {
        *next_candidate = next_candidate.saturating_add(1);
    }
    let allocated = *next_candidate;
    used_ids.insert(allocated);
    *next_candidate = next_candidate.saturating_add(1);
    allocated
}

#[allow(clippy::too_many_arguments)]
fn transform_plugin_items(
    items: Vec<ParsedItem>,
    plugin_name: &str,
    masters: &[String],
    own_index: u8,
    identities: &HashMap<(String, u32), u32>,
    lineage_names: &[String],
    raw_to_output: &HashMap<u32, u32>,
    overrides: &HashSet<u32>,
    override_winners: &mut HashMap<u32, ParsedRecord>,
    container_appends: &mut HashMap<u32, (i32, Vec<ParsedItem>)>,
    schema: Option<&CompiledSchema>,
) -> Result<Vec<ParsedItem>, MergeError> {
    let mut transformed = Vec::new();
    let mut iter = items.into_iter().peekable();
    while let Some(item) = iter.next() {
        match item {
            ParsedItem::Record(mut record) => {
                let raw_id = record.form_id;
                record.form_id = raw_to_output[&raw_id];
                repoint_lineage_record(
                    &mut record,
                    plugin_name,
                    masters,
                    own_index,
                    identities,
                    lineage_names,
                    raw_to_output,
                    schema,
                )?;
                if overrides.contains(&raw_id) {
                    override_winners.insert(record.form_id, record.clone());
                    if let Some(child_type) = container_group_type(&record)
                        && matches!(iter.peek(), Some(ParsedItem::Group(group)) if group.group_type == child_type)
                    {
                        let ParsedItem::Group(group) = iter.next().unwrap() else {
                            unreachable!()
                        };
                        let children = transform_plugin_items(
                            group.children,
                            plugin_name,
                            masters,
                            own_index,
                            identities,
                            lineage_names,
                            raw_to_output,
                            overrides,
                            override_winners,
                            container_appends,
                            schema,
                        )?;
                        container_appends
                            .entry(record.form_id)
                            .or_insert_with(|| (child_type, Vec::new()))
                            .1
                            .extend(children);
                    }
                } else {
                    transformed.push(ParsedItem::Record(record));
                }
            }
            ParsedItem::Group(mut group) => {
                repoint_lineage_group(
                    &mut group,
                    plugin_name,
                    masters,
                    own_index,
                    identities,
                    lineage_names,
                    raw_to_output,
                )?;
                group.children = transform_plugin_items(
                    group.children,
                    plugin_name,
                    masters,
                    own_index,
                    identities,
                    lineage_names,
                    raw_to_output,
                    overrides,
                    override_winners,
                    container_appends,
                    schema,
                )?;
                if !group.children.is_empty() {
                    transformed.push(ParsedItem::Group(group));
                }
            }
        }
    }
    Ok(transformed)
}

fn apply_container_appends(
    items: &mut Vec<ParsedItem>,
    container_appends: &mut HashMap<u32, (i32, Vec<ParsedItem>)>,
    header_size: usize,
) {
    let mut index = 0;
    while index < items.len() {
        let record_id = match &items[index] {
            ParsedItem::Record(record) => Some(record.form_id),
            ParsedItem::Group(_) => None,
        };
        if let Some(record_id) = record_id {
            if let Some((child_group_type, children)) = container_appends.remove(&record_id) {
                if index + 1 < items.len()
                    && let ParsedItem::Group(group) = &mut items[index + 1]
                    && group.group_type == child_group_type
                {
                    group.children.extend(children);
                } else {
                    items.insert(
                        index + 1,
                        ParsedItem::Group(ParsedGroup {
                            label: record_id.to_le_bytes(),
                            group_type: child_group_type,
                            tail: Bytes::from(vec![0; header_size.saturating_sub(16)]),
                            children,
                        }),
                    );
                }
            }
        }
        if let ParsedItem::Group(group) = &mut items[index] {
            apply_container_appends(&mut group.children, container_appends, header_size);
        }
        index += 1;
    }
}

fn apply_override_winners(items: &mut [ParsedItem], override_winners: &HashMap<u32, ParsedRecord>) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                if let Some(winner) = override_winners.get(&record.form_id) {
                    *record = winner.clone();
                }
            }
            ParsedItem::Group(group) => {
                apply_override_winners(&mut group.children, override_winners);
            }
        }
    }
}

fn repoint_lineage_group(
    group: &mut ParsedGroup,
    plugin_name: &str,
    masters: &[String],
    own_index: u8,
    identities: &HashMap<(String, u32), u32>,
    lineage_names: &[String],
    raw_to_output: &HashMap<u32, u32>,
) -> Result<(), MergeError> {
    if matches!(group.group_type, 1 | 6 | 7 | 8 | 9 | 10) {
        let raw = u32::from_le_bytes(group.label);
        group.label = resolve_lineage_ref(
            raw,
            plugin_name,
            masters,
            own_index,
            identities,
            lineage_names,
            raw_to_output,
        )?
        .to_le_bytes();
    }
    Ok(())
}

fn repoint_lineage_record(
    record: &mut ParsedRecord,
    plugin_name: &str,
    masters: &[String],
    own_index: u8,
    identities: &HashMap<(String, u32), u32>,
    lineage_names: &[String],
    raw_to_output: &HashMap<u32, u32>,
    schema: Option<&CompiledSchema>,
) -> Result<(), MergeError> {
    let signature = record.signature.to_string();
    let mut error = None;
    let mut rewrite = |subrecord_signature: &str, raw| match resolve_lineage_ref(
        raw,
        plugin_name,
        masters,
        own_index,
        identities,
        lineage_names,
        raw_to_output,
    ) {
        Ok(mapped) if mapped != raw => Some(mapped),
        Ok(_) => None,
        Err(resolve_error) => {
            if error.is_none() {
                error = Some(MergeError::Load(format!(
                    "{signature}.{subrecord_signature} reference walk failed: {resolve_error}"
                )));
            }
            None
        }
    };
    if rewrite_merge_formids_in_subrecords(&signature, &mut record.subrecords, schema, &mut rewrite)
    {
        record.raw_payload = None;
    }
    error.map_or(Ok(()), Err)
}

fn resolve_lineage_ref(
    raw: u32,
    plugin_name: &str,
    masters: &[String],
    own_index: u8,
    identities: &HashMap<(String, u32), u32>,
    lineage_names: &[String],
    raw_to_output: &HashMap<u32, u32>,
) -> Result<u32, MergeError> {
    if raw == 0 || raw < 0x800 || raw >= 0xFF00_0000 {
        return Ok(raw);
    }
    if let Some(&output) = raw_to_output.get(&raw) {
        return Ok(output);
    }
    let source_index = (raw >> 24) as u8;
    let object_id = raw & 0x00FF_FFFF;
    let origin = if (source_index as usize) < masters.len() {
        masters[source_index as usize].to_lowercase()
    } else if source_index == own_index || source_index == 0xFF {
        plugin_name.to_string()
    } else if let Some(lineage_plugin) = lineage_names.get(source_index as usize) {
        lineage_plugin.clone()
    } else {
        return Ok(raw);
    };
    Ok(identities.get(&(origin, object_id)).copied().unwrap_or(raw))
}

fn container_group_type(record: &ParsedRecord) -> Option<i32> {
    match record.signature.as_str() {
        "WRLD" => Some(1),
        "CELL" => Some(6),
        "DIAL" => Some(7),
        _ => None,
    }
}

fn collect_eids(items: &[ParsedItem], index: &mut HashMap<(String, SmolStr), u32>) {
    for item in items {
        match item {
            ParsedItem::Record(record) => {
                let editor_id = editor_id_from_effective_subrecords(&record.subrecords);
                if !editor_id.is_empty() {
                    index.insert(
                        (editor_id.to_lowercase(), record.signature.clone()),
                        record.form_id,
                    );
                }
            }
            ParsedItem::Group(group) => collect_eids(&group.children, index),
        }
    }
}

#[cfg(test)]
mod tests {
    use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::extract_skyrim_nvmi_form_ids;
    use esp_authoring_core::plugin_runtime::plugin_handle_close_native;

    use super::*;
    use crate::merge_sources::test_util::{formid_sub, rec, write_test_plugin_with_masters};

    fn find_record<'a>(items: &'a [ParsedItem], editor_id: &str) -> Option<&'a ParsedRecord> {
        for item in items {
            match item {
                ParsedItem::Record(record)
                    if editor_id_from_effective_subrecords(&record.subrecords) == editor_id =>
                {
                    return Some(record);
                }
                ParsedItem::Group(group) => {
                    if let Some(record) = find_record(&group.children, editor_id) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn raw_subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: signature.into(),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn skyrim_v12_nvnm(parent: u32, interior: bool, linked_navmesh: u32, door_ref: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&12u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(if interior { 0 } else { parent }).to_le_bytes());
        if interior {
            data.extend_from_slice(&parent.to_le_bytes());
        } else {
            data.extend_from_slice(&0i16.to_le_bytes());
            data.extend_from_slice(&0i16.to_le_bytes());
        }
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&linked_navmesh.to_le_bytes());
        data.extend_from_slice(&0i16.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0i16.to_le_bytes());
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(&door_ref.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data
    }

    fn skyrim_v12_nvmi(
        navmesh: u32,
        parent: u32,
        interior: bool,
        linked_navmesh: u32,
        door_ref: u32,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&navmesh.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        for value in [0.0f32; 4] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&linked_navmesh.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&door_ref.to_le_bytes());
        data.push(u8::from(!interior));
        if !interior {
            for value in [0.0f32; 6] {
                data.extend_from_slice(&value.to_le_bytes());
            }
            data.extend_from_slice(&1u32.to_le_bytes());
            for vertex in [0u16, 1, 2] {
                data.extend_from_slice(&vertex.to_le_bytes());
            }
            data.extend_from_slice(&3u32.to_le_bytes());
            for value in [0.0f32; 9] {
                data.extend_from_slice(&value.to_le_bytes());
            }
        }
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(if interior { 0 } else { parent }).to_le_bytes());
        if interior {
            data.extend_from_slice(&parent.to_le_bytes());
        } else {
            data.extend_from_slice(&0i16.to_le_bytes());
            data.extend_from_slice(&0i16.to_le_bytes());
        }
        data
    }

    #[test]
    fn dlc_override_replaces_base_record_at_preserved_id() {
        let tmp = tempfile::tempdir().unwrap();
        let base = write_test_plugin_with_masters(
            tmp.path(),
            "Base.esm",
            "fnv",
            Vec::new(),
            vec![rec("GLOB", 0x1200, "OldTimeScale")],
        );
        let dlc = write_test_plugin_with_masters(
            tmp.path(),
            "DLC.esm",
            "fnv",
            vec!["Base.esm".to_string()],
            vec![rec("GLOB", 0x1200, "NewTimeScale")],
        );
        let handles = [
            super::super::load_no_py(base.to_str().unwrap(), Some("fnv")).unwrap(),
            super::super::load_no_py(dlc.to_str().unwrap(), Some("fnv")).unwrap(),
        ];
        let flattened = flatten_lineage(&handles).unwrap();
        assert_eq!(
            find_record(&flattened.tree, "NewTimeScale")
                .unwrap()
                .form_id,
            0x1200
        );
        assert!(find_record(&flattened.tree, "OldTimeScale").is_none());
        handles.into_iter().for_each(|handle| {
            plugin_handle_close_native(handle);
        });
    }

    #[test]
    fn colliding_dlc_record_is_reallocated_and_repointed() {
        let tmp = tempfile::tempdir().unwrap();
        let base = write_test_plugin_with_masters(
            tmp.path(),
            "Base.esm",
            "fnv",
            Vec::new(),
            vec![rec("GLOB", 0x1200, "BaseRecord")],
        );
        let mut referring = rec("ACTI", 0x0100_1300, "ReferringRecord");
        referring.subrecords.push(formid_sub("SCRI", 0x0100_1200));
        let dlc = write_test_plugin_with_masters(
            tmp.path(),
            "DLC.esm",
            "fnv",
            vec!["Base.esm".to_string()],
            vec![rec("SCPT", 0x0100_1200, "DlcRecord"), referring],
        );
        let handles = [
            super::super::load_no_py(base.to_str().unwrap(), Some("fnv")).unwrap(),
            super::super::load_no_py(dlc.to_str().unwrap(), Some("fnv")).unwrap(),
        ];
        let flattened = flatten_lineage(&handles).unwrap();
        let allocated = find_record(&flattened.tree, "DlcRecord").unwrap().form_id;
        assert_ne!(allocated, 0x1200);
        let referring = find_record(&flattened.tree, "ReferringRecord").unwrap();
        let scri = referring
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "SCRI")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(scri.data[0..4].try_into().unwrap()),
            allocated
        );
        handles.into_iter().for_each(|handle| {
            plugin_handle_close_native(handle);
        });
    }

    #[test]
    fn skyrim_v12_navmesh_formids_follow_colliding_dlc_reallocation() {
        let tmp = tempfile::tempdir().unwrap();
        let base = write_test_plugin_with_masters(
            tmp.path(),
            "Skyrim.esm",
            "skyrimse",
            Vec::new(),
            (0x1200..=0x1700)
                .step_by(0x100)
                .map(|form_id| rec("GLOB", form_id, &format!("Base{form_id:04X}")))
                .collect(),
        );

        let source_nav_a = 0x0100_1200;
        let source_nav_b = 0x0100_1300;
        let source_cell = 0x0100_1400;
        let source_door = 0x0100_1500;
        let source_world = 0x0100_1600;
        let mut nav_a = rec("NAVM", source_nav_a, "DlcNavA");
        nav_a.subrecords.push(raw_subrecord(
            "NVNM",
            skyrim_v12_nvnm(source_world, false, source_nav_b, source_door),
        ));
        let mut nav_b = rec("NAVM", source_nav_b, "DlcNavB");
        nav_b.subrecords.push(raw_subrecord(
            "NVNM",
            skyrim_v12_nvnm(source_cell, true, source_nav_a, source_door),
        ));
        let mut navi = rec("NAVI", 0x0100_1700, "DlcNavi");
        navi.subrecords
            .push(raw_subrecord("NVER", 12u32.to_le_bytes().to_vec()));
        navi.subrecords.push(raw_subrecord(
            "NVMI",
            skyrim_v12_nvmi(source_nav_a, source_world, false, source_nav_b, source_door),
        ));
        navi.subrecords.push(raw_subrecord(
            "NVMI",
            skyrim_v12_nvmi(source_nav_b, source_cell, true, source_nav_a, source_door),
        ));
        let dlc = write_test_plugin_with_masters(
            tmp.path(),
            "Dawnguard.esm",
            "skyrimse",
            vec!["Skyrim.esm".to_string()],
            vec![
                nav_a,
                nav_b,
                rec("CELL", source_cell, "DlcCell"),
                rec("REFR", source_door, "DlcDoor"),
                rec("WRLD", source_world, "DlcWorld"),
                navi,
            ],
        );
        let handles = [
            super::super::load_no_py(base.to_str().unwrap(), Some("skyrimse")).unwrap(),
            super::super::load_no_py(dlc.to_str().unwrap(), Some("skyrimse")).unwrap(),
        ];

        let flattened = flatten_lineage(&handles).unwrap();
        let nav_a = find_record(&flattened.tree, "DlcNavA").unwrap();
        let nav_b = find_record(&flattened.tree, "DlcNavB").unwrap();
        let cell = find_record(&flattened.tree, "DlcCell").unwrap().form_id;
        let door = find_record(&flattened.tree, "DlcDoor").unwrap().form_id;
        let world = find_record(&flattened.tree, "DlcWorld").unwrap().form_id;
        assert_ne!(nav_a.form_id, source_nav_a);
        assert_ne!(nav_b.form_id, source_nav_b);
        assert_ne!(cell, source_cell);
        assert_ne!(door, source_door);
        assert_ne!(world, source_world);

        let nav_a_nvnm = nav_a
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "NVNM")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(nav_a_nvnm.data[8..12].try_into().unwrap()),
            world
        );
        assert_eq!(
            u32::from_le_bytes(nav_a_nvnm.data[32..36].try_into().unwrap()),
            nav_b.form_id
        );
        assert_eq!(
            u32::from_le_bytes(nav_a_nvnm.data[48..52].try_into().unwrap()),
            door
        );
        let nav_b_nvnm = nav_b
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "NVNM")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(nav_b_nvnm.data[12..16].try_into().unwrap()),
            cell
        );
        assert_eq!(
            u32::from_le_bytes(nav_b_nvnm.data[32..36].try_into().unwrap()),
            nav_a.form_id
        );
        assert_eq!(
            u32::from_le_bytes(nav_b_nvnm.data[48..52].try_into().unwrap()),
            door
        );

        let navi = find_record(&flattened.tree, "DlcNavi").unwrap();
        let nvmis = navi
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "NVMI")
            .collect::<Vec<_>>();
        let mut exterior_refs = Vec::new();
        extract_skyrim_nvmi_form_ids(&nvmis[0].data, &mut exterior_refs);
        assert_eq!(
            exterior_refs,
            vec![nav_a.form_id, nav_b.form_id, door, world]
        );
        let mut interior_refs = Vec::new();
        extract_skyrim_nvmi_form_ids(&nvmis[1].data, &mut interior_refs);
        assert_eq!(
            interior_refs,
            vec![nav_b.form_id, nav_a.form_id, door, cell]
        );

        handles.into_iter().for_each(|handle| {
            plugin_handle_close_native(handle);
        });
    }

    #[test]
    fn unknown_master_is_hard_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let dlc = write_test_plugin_with_masters(
            tmp.path(),
            "DLC.esm",
            "fnv",
            vec!["Missing.esm".to_string()],
            vec![rec("GLOB", 0x0100_1200, "Record")],
        );
        let handle = super::super::load_no_py(dlc.to_str().unwrap(), Some("fnv")).unwrap();
        assert!(matches!(
            flatten_lineage(&[handle]),
            Err(MergeError::UnknownMaster(_))
        ));
        plugin_handle_close_native(handle);
    }

    #[test]
    fn primary_lineage_preserves_vanilla_invalid_high_byte_reference() {
        let tmp = tempfile::tempdir().unwrap();
        let mut region = rec("REGN", 0x16B8FE, "AudioINCMountain");
        region.subrecords.push(formid_sub("RDSI", 0x0102_76B2));
        let primary = write_test_plugin_with_masters(
            tmp.path(),
            "FalloutNV.esm",
            "fnv",
            Vec::new(),
            vec![region],
        );
        let handle = super::super::load_no_py(primary.to_str().unwrap(), Some("fnv")).unwrap();
        let flattened = flatten_lineage(&[handle]).unwrap();
        let region = find_record(&flattened.tree, "AudioINCMountain").unwrap();
        let rdsi = region
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "RDSI")
            .unwrap();
        assert_eq!(
            u32::from_le_bytes(rdsi.data[0..4].try_into().unwrap()),
            0x0102_76B2
        );
        plugin_handle_close_native(handle);
    }

    #[test]
    fn primary_lineage_preserves_existing_low_object_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let primary = write_test_plugin_with_masters(
            tmp.path(),
            "FalloutNV.esm",
            "fnv",
            Vec::new(),
            vec![rec("GLOB", 0x0005EF, "LowVanillaRecord")],
        );
        let handle = super::super::load_no_py(primary.to_str().unwrap(), Some("fnv")).unwrap();
        let flattened = flatten_lineage(&[handle]).unwrap();
        assert_eq!(
            find_record(&flattened.tree, "LowVanillaRecord")
                .unwrap()
                .form_id,
            0x0005EF
        );
        plugin_handle_close_native(handle);
    }

    #[test]
    fn dlc_master_namespace_injection_without_base_record_is_new() {
        let tmp = tempfile::tempdir().unwrap();
        let base = write_test_plugin_with_masters(
            tmp.path(),
            "FalloutNV.esm",
            "fnv",
            Vec::new(),
            vec![rec("GLOB", 0x1200, "BaseRecord")],
        );
        let dlc = write_test_plugin_with_masters(
            tmp.path(),
            "LonesomeRoad.esm",
            "fnv",
            vec!["FalloutNV.esm".to_string()],
            vec![rec("AVIF", 0x000005EF, "AVVariable04")],
        );
        let handles = [
            super::super::load_no_py(base.to_str().unwrap(), Some("fnv")).unwrap(),
            super::super::load_no_py(dlc.to_str().unwrap(), Some("fnv")).unwrap(),
        ];
        let flattened = flatten_lineage(&handles).unwrap();
        assert_eq!(
            find_record(&flattened.tree, "AVVariable04")
                .unwrap()
                .form_id,
            0x000005EF
        );
        handles.into_iter().for_each(|handle| {
            plugin_handle_close_native(handle);
        });
    }

    #[test]
    fn dlc_record_header_above_declared_master_count_is_owned() {
        let tmp = tempfile::tempdir().unwrap();
        let base = write_test_plugin_with_masters(
            tmp.path(),
            "FalloutNV.esm",
            "fnv",
            Vec::new(),
            vec![rec("GLOB", 0x1200, "BaseRecord")],
        );
        let dlc = write_test_plugin_with_masters(
            tmp.path(),
            "GunRunnersArsenal.esm",
            "fnv",
            vec!["FalloutNV.esm".to_string()],
            vec![rec("REFR", 0x02000801, "GRAOwnedRef")],
        );
        let handles = [
            super::super::load_no_py(base.to_str().unwrap(), Some("fnv")).unwrap(),
            super::super::load_no_py(dlc.to_str().unwrap(), Some("fnv")).unwrap(),
        ];
        let flattened = flatten_lineage(&handles).unwrap();
        assert_eq!(
            find_record(&flattened.tree, "GRAOwnedRef").unwrap().form_id,
            0x00000801
        );
        handles.into_iter().for_each(|handle| {
            plugin_handle_close_native(handle);
        });
    }
}
