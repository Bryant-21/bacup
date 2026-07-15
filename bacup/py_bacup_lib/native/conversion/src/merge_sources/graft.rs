use std::collections::{HashMap, HashSet};

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{CompiledSchema, ParsedGroup, ParsedItem, ParsedRecord};

use super::classify::Classification;
use super::repoint::{DanglingReference, RepointStats, repoint_record_with_schema};

pub(crate) fn graft_lineage(
    output: &mut Vec<ParsedItem>,
    grafted: Vec<ParsedItem>,
    classification: &Classification,
    primary_ids: &HashSet<u32>,
    stats: &mut RepointStats,
    header_size: usize,
    schema: Option<&CompiledSchema>,
) {
    let mut container_grafts = HashMap::new();
    for item in grafted {
        let ParsedItem::Group(mut top_group) = item else {
            continue;
        };
        top_group.children = transform_items(
            top_group.children,
            classification,
            primary_ids,
            stats,
            &mut container_grafts,
            schema,
        );
        if top_group.label == *b"CELL" && top_group.group_type == 0 {
            let mut cells = Vec::new();
            collect_interior_cells(top_group.children, &mut cells);
            for cell in cells {
                insert_interior_cell(output, cell, header_size);
            }
        } else if !top_group.children.is_empty() {
            merge_top_group(output, top_group);
        }
    }
    apply_container_grafts(output, &mut container_grafts, header_size);
}

fn transform_items(
    items: Vec<ParsedItem>,
    classification: &Classification,
    primary_ids: &HashSet<u32>,
    stats: &mut RepointStats,
    container_grafts: &mut HashMap<u32, (i32, Vec<ParsedItem>)>,
    schema: Option<&CompiledSchema>,
) -> Vec<ParsedItem> {
    let mut output = Vec::new();
    let mut iter = items.into_iter().peekable();
    while let Some(item) = iter.next() {
        match item {
            ParsedItem::Record(mut record) => {
                if classification.dropped.contains(&record.form_id) {
                    if let Some(child_group_type) = container_group_type(&record)
                        && matches!(iter.peek(), Some(ParsedItem::Group(group)) if group.group_type == child_group_type)
                    {
                        let ParsedItem::Group(group) = iter.next().unwrap() else {
                            unreachable!()
                        };
                        let items = transform_items(
                            group.children,
                            classification,
                            primary_ids,
                            stats,
                            container_grafts,
                            schema,
                        );
                        container_grafts
                            .entry(classification.remap[&record.form_id])
                            .or_insert_with(|| (child_group_type, Vec::new()))
                            .1
                            .extend(items);
                    }
                    continue;
                }
                record.form_id = classification.remap[&record.form_id];
                repoint_record_with_schema(
                    &mut record,
                    &classification.remap,
                    primary_ids,
                    stats,
                    schema,
                );
                output.push(ParsedItem::Record(record));
            }
            ParsedItem::Group(mut group) => {
                repoint_group_label(&mut group, classification, primary_ids, stats);
                group.children = transform_items(
                    group.children,
                    classification,
                    primary_ids,
                    stats,
                    container_grafts,
                    schema,
                );
                if !group.children.is_empty() {
                    output.push(ParsedItem::Group(group));
                }
            }
        }
    }
    output
}

fn repoint_group_label(
    group: &mut ParsedGroup,
    classification: &Classification,
    primary_ids: &HashSet<u32>,
    stats: &mut RepointStats,
) {
    if !matches!(group.group_type, 1 | 6 | 7 | 8 | 9 | 10) {
        return;
    }
    let raw = u32::from_le_bytes(group.label);
    if raw == 0 || raw < 0x800 || raw >= 0xFF00_0000 {
        return;
    }
    if let Some(&mapped) = classification.remap.get(&raw) {
        if mapped != raw {
            group.label = mapped.to_le_bytes();
            stats.remapped += 1;
        }
    } else if !primary_ids.contains(&raw) {
        stats.dangling.push(DanglingReference::group(raw));
    }
}

fn container_group_type(record: &ParsedRecord) -> Option<i32> {
    match record.signature.as_str() {
        "WRLD" => Some(1),
        "CELL" => Some(6),
        "DIAL" => Some(7),
        _ => None,
    }
}

pub(crate) fn merge_top_group(output: &mut Vec<ParsedItem>, incoming: ParsedGroup) {
    if let Some(ParsedItem::Group(existing)) = output.iter_mut().find(|item| {
        matches!(item, ParsedItem::Group(group) if group.group_type == incoming.group_type && group.label == incoming.label)
    }) {
        existing.children.extend(incoming.children);
    } else {
        output.push(ParsedItem::Group(incoming));
    }
}

fn apply_container_grafts(
    items: &mut Vec<ParsedItem>,
    container_grafts: &mut HashMap<u32, (i32, Vec<ParsedItem>)>,
    header_size: usize,
) {
    let mut index = 0;
    while index < items.len() {
        let survivor_fid = match &items[index] {
            ParsedItem::Record(record) => Some(record.form_id),
            ParsedItem::Group(_) => None,
        };
        if let Some(survivor_fid) = survivor_fid
            && let Some((child_group_type, children)) = container_grafts.remove(&survivor_fid)
        {
            if index + 1 < items.len()
                && let ParsedItem::Group(group) = &mut items[index + 1]
                && group.group_type == child_group_type
            {
                group.children.extend(children);
            } else {
                items.insert(
                    index + 1,
                    ParsedItem::Group(ParsedGroup {
                        label: survivor_fid.to_le_bytes(),
                        group_type: child_group_type,
                        tail: Bytes::from(vec![0; header_size.saturating_sub(16)]),
                        children,
                    }),
                );
            }
        }
        if let ParsedItem::Group(group) = &mut items[index] {
            apply_container_grafts(&mut group.children, container_grafts, header_size);
        }
        index += 1;
    }
}

fn collect_interior_cells(items: Vec<ParsedItem>, cells: &mut Vec<Vec<ParsedItem>>) {
    let mut iter = items.into_iter().peekable();
    while let Some(item) = iter.next() {
        match item {
            ParsedItem::Record(record) if record.signature.as_str() == "CELL" => {
                let mut entry = vec![ParsedItem::Record(record)];
                if matches!(iter.peek(), Some(ParsedItem::Group(group)) if group.group_type == 6) {
                    entry.push(iter.next().unwrap());
                }
                cells.push(entry);
            }
            ParsedItem::Group(group) => collect_interior_cells(group.children, cells),
            _ => {}
        }
    }
}

fn insert_interior_cell(output: &mut Vec<ParsedItem>, entry: Vec<ParsedItem>, header_size: usize) {
    let fid = match &entry[0] {
        ParsedItem::Record(record) => record.form_id,
        ParsedItem::Group(_) => unreachable!(),
    };
    let top_index = match output.iter().position(
        |item| matches!(item, ParsedItem::Group(group) if group.group_type == 0 && group.label == *b"CELL"),
    ) {
        Some(index) => index,
        None => {
            output.push(ParsedItem::Group(ParsedGroup {
                label: *b"CELL",
                group_type: 0,
                tail: Bytes::from(vec![0; header_size.saturating_sub(16)]),
                children: Vec::new(),
            }));
            output.len() - 1
        }
    };
    let ParsedItem::Group(top) = &mut output[top_index] else {
        unreachable!()
    };
    let block_label = (fid % 10_i32 as u32).to_le_bytes();
    let block_index = ensure_child_group(&mut top.children, 2, block_label, header_size);
    let ParsedItem::Group(block) = &mut top.children[block_index] else {
        unreachable!()
    };
    let subblock_label = ((fid / 10) % 10).to_le_bytes();
    let subblock_index = ensure_child_group(&mut block.children, 3, subblock_label, header_size);
    let ParsedItem::Group(subblock) = &mut block.children[subblock_index] else {
        unreachable!()
    };
    subblock.children.extend(entry);
}

fn ensure_child_group(
    items: &mut Vec<ParsedItem>,
    group_type: i32,
    label: [u8; 4],
    header_size: usize,
) -> usize {
    if let Some(index) = items.iter().position(
        |item| matches!(item, ParsedItem::Group(group) if group.group_type == group_type && group.label == label),
    ) {
        return index;
    }
    items.push(ParsedItem::Group(ParsedGroup {
        label,
        group_type,
        tail: Bytes::from(vec![0; header_size.saturating_sub(16)]),
        children: Vec::new(),
    }));
    items.len() - 1
}
