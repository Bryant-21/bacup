use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{
    ParsedItem, ParsedRecord, ParsedSubrecord, effective_subrecords_for_record,
    insert_parsed_record_in_slot, plugin_handle_store_ref, replace_parsed_record_contents_in_slot,
};
use nif_core_native::model::{NifFile, NifValue};

use crate::fixups::recentre_far_interiors::INTERIOR_COORDINATE_LIMIT;
use crate::phase::{LogLevel, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};

const ANCHOR_LIMIT: f32 = 29_000.0;

fn bytes(record: &ParsedRecord, sig: &str) -> Option<Bytes> {
    effective_subrecords_for_record(record)
        .iter()
        .find(|sub| sub.signature == sig)
        .map(|sub| sub.data.clone())
}

fn text(record: &ParsedRecord, sig: &str) -> String {
    bytes(record, sig)
        .map(|data| {
            String::from_utf8_lossy(&data)
                .trim_end_matches('\0')
                .to_string()
        })
        .unwrap_or_default()
}

fn set_bytes(record: &mut ParsedRecord, sig: &str, data: impl Into<Bytes>) {
    if let Some(sub) = record
        .subrecords
        .iter_mut()
        .find(|sub| sub.signature == sig)
    {
        sub.data = data.into();
    } else {
        record.subrecords.push(ParsedSubrecord {
            signature: sig.into(),
            data: data.into(),
            semantic_type: None,
        });
    }
    record.raw_payload = None;
}

fn decoded(record: &ParsedRecord) -> ParsedRecord {
    let mut result = record.clone();
    result.subrecords = effective_subrecords_for_record(record).into_owned();
    result.raw_payload = None;
    result
}

fn walk(items: &[ParsedItem], f: &mut impl FnMut(&ParsedRecord)) {
    for item in items {
        match item {
            ParsedItem::Record(record) => f(record),
            ParsedItem::Group(group) => walk(&group.children, f),
        }
    }
}

fn shelter_refs(items: &[ParsedItem], cells: &BTreeSet<u32>, out: &mut Vec<ParsedRecord>) {
    for item in items {
        let ParsedItem::Group(group) = item else {
            continue;
        };
        if group.group_type == 6 && cells.contains(&u32::from_le_bytes(group.label)) {
            walk(&group.children, &mut |record| {
                if record.signature == "REFR" && record.flags & 0x20 == 0 {
                    out.push(decoded(record));
                }
            });
        } else {
            shelter_refs(&group.children, cells, out);
        }
    }
}

fn floats(data: &[u8]) -> Vec<f32> {
    data.chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

fn anchor(record: &ParsedRecord) -> Option<([f32; 3], [f32; 3])> {
    let data = bytes(record, "DATA")?;
    if data.len() != 24 {
        return None;
    }
    let values = floats(&data);
    let scale = bytes(record, "XSCL")
        .map(|v| floats(&v))
        .unwrap_or_else(|| vec![1.0]);
    let [scale] = scale.as_slice() else {
        return None;
    };
    if !values.iter().all(|v| v.is_finite()) || !scale.is_finite() || *scale <= 0.0 {
        return None;
    }
    if values[..2]
        .iter()
        .all(|v| v.abs() <= INTERIOR_COORDINATE_LIMIT)
    {
        return None;
    }
    let position = [
        values[0].clamp(-ANCHOR_LIMIT, ANCHOR_LIMIT),
        values[1].clamp(-ANCHOR_LIMIT, ANCHOR_LIMIT),
        values[2],
    ];
    let mut offset = [0, 1, 2].map(|i| (values[i] - position[i]) / scale);
    // REFR uses Rx(-x) Ry(-y) Rz(-z); move its displacement into model space.
    for axis in 0..3 {
        let (s, c) = values[axis + 3].sin_cos();
        let a = (axis + 1) % 3;
        let b = (axis + 2) % 3;
        (offset[a], offset[b]) = (c * offset[a] - s * offset[b], s * offset[a] + c * offset[b]);
    }
    Some((position, offset))
}

fn offset_scenery(nif: &mut NifFile, offset: [f32; 3]) -> Result<(), String> {
    if nif.blocks.iter().any(|block| {
        block.type_name.starts_with("bhk")
            || block.type_name.contains("Controller")
            || block.type_name.contains("Skin")
            || block.type_name.contains("Particle")
            || block.type_name == "BSBehaviorGraphExtraData"
    }) {
        return Err(
            "collision, animation, skin or particles require a separate transform repair".into(),
        );
    }
    let root = nif.blocks.first().ok_or("mesh has no root")?;
    let identity = NifValue::Matrix33([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    let identity_rotation = match root.get_field("Rotation") {
        Some(NifValue::Struct(fields)) => (1..=3).all(|i| {
            (1..=3).all(|j| {
                fields.get(&format!("m{i}{j}"))
                    == Some(&NifValue::Float(if i == j { 1.0 } else { 0.0 }))
            })
        }),
        value => value == Some(&identity),
    };
    if !matches!(root.type_name.as_str(), "NiNode" | "BSFadeNode")
        || translation(root.get_field("Translation")) != Some([0.0; 3])
        || !identity_rotation
        || root.get_field("Scale") != Some(&NifValue::Float(1.0))
        || nif.header.footer_roots.iter().any(|id| *id != 0)
    {
        return Err("mesh requires a single identity node root".into());
    }
    let Some(NifValue::Array(children)) = root.get_field("Children").cloned() else {
        return Err("mesh root has no children".into());
    };
    // FO4 replaces the root transform with REFR's transform, so the offset must
    // sit below it. Vertex buffers and their half-float precision stay untouched.
    let id = nif.add_block("NiNode", None);
    let node = &mut nif.blocks[id];
    node.set_field("Name", NifValue::String("B21_ShelterSceneryOffset".into()));
    node.set_field("Flags", NifValue::UInt(14));
    node.set_field("Translation", NifValue::Vec3(offset));
    node.set_field("Rotation", identity);
    node.set_field("Scale", NifValue::Float(1.0));
    node.set_field("Num Children", NifValue::UInt(children.len() as u64));
    node.set_field("Children", NifValue::Array(children));
    nif.blocks[0].set_field("Num Children", NifValue::UInt(1));
    nif.blocks[0].set_field("Children", NifValue::Array(vec![NifValue::Ref(id as i32)]));
    Ok(())
}

fn translation(value: Option<&NifValue>) -> Option<[f32; 3]> {
    match value? {
        NifValue::Vec3(value) => Some(*value),
        NifValue::Struct(fields) => {
            let number = |name| match fields.get(name) {
                Some(NifValue::Float(value)) => Some(*value as f32),
                _ => None,
            };
            Some([number("x")?, number("y")?, number("z")?])
        }
        _ => None,
    }
}

pub(super) fn repair(ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
    let (bases, mut used, refs, own_index, mut next) = {
        let store = plugin_handle_store_ref()
            .lock()
            .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
        let slot = store
            .get(&ctx.run.target_handle_id)
            .ok_or_else(|| PhaseError::Internal("missing target plugin".into()))?;
        let mut cells = BTreeSet::new();
        let mut bases = BTreeMap::new();
        let mut used = BTreeSet::new();
        let own_index = slot.parsed.header.masters.len() as u32;
        walk(&slot.parsed.root_items, &mut |record| {
            used.insert(record.form_id & 0x00FF_FFFF);
            if record.signature == "CELL"
                && text(record, "EDID").starts_with("Shelters")
                && bytes(record, "DATA")
                    .is_some_and(|data| data.first().is_some_and(|v| v & 1 != 0))
            {
                cells.insert(record.form_id);
            }
            if record.signature == "STAT" && record.form_id >> 24 == own_index {
                bases.insert(record.form_id, decoded(record));
            }
        });
        let mut refs = Vec::new();
        shelter_refs(&slot.parsed.root_items, &cells, &mut refs);
        (
            bases,
            used,
            refs,
            own_index,
            slot.parsed.header.next_object_id.max(0x800),
        )
    };
    let mut report = PhaseReport::default();
    let mut changes = Vec::new();
    for mut reference in refs {
        ctx.check_cancel()?;
        if reference.form_id >> 24 != own_index {
            continue;
        }
        let Some((position, offset)) = anchor(&reference) else {
            continue;
        };
        let Some(base_id) = bytes(&reference, "NAME")
            .and_then(|v| v.as_ref().try_into().ok().map(u32::from_le_bytes))
        else {
            continue;
        };
        let Some(base) = bases.get(&base_id) else {
            continue;
        };
        let model = text(base, "MODL").replace('\\', "/");
        if model.is_empty() || bytes(base, "VMAD").is_some() {
            continue;
        }
        let path = std::iter::once(ctx.mod_path.join("data"))
            .chain(ctx.target_extracted_dir.map(|path| path.to_path_buf()))
            .chain(ctx.target_data_dir.map(|path| path.to_path_buf()))
            .map(|root| root.join("meshes").join(&model))
            .find(|path| path.is_file());
        let Some(path) = path else {
            report.warnings += 1;
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "regenerate_modt",
                level: LogLevel::Warn,
                message: format!(
                    "shelter scenery {:08X} left unchanged: missing converted mesh {model}",
                    reference.form_id
                ),
            });
            continue;
        };
        let generated = format!(
            "B21/ShelterScenery/{:06X}.nif",
            reference.form_id & 0x00FF_FFFF
        );
        let output = ctx.mod_path.join("data/meshes").join(&generated);
        let mut nif = NifFile::load(&path).map_err(|error| {
            PhaseError::Internal(format!("shelter scenery {}: {error}", path.display()))
        })?;
        if let Err(reason) = offset_scenery(&mut nif, offset) {
            report.warnings += 1;
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "regenerate_modt",
                level: LogLevel::Warn,
                message: format!(
                    "shelter scenery {:08X} left unchanged: {reason}",
                    reference.form_id
                ),
            });
            continue;
        }
        while next <= 0x00FF_FFFF && used.contains(&next) {
            next += 1;
        }
        if next > 0x00FF_FFFF {
            return Err(PhaseError::Internal(
                "shelter scenery FormIDs exhausted".into(),
            ));
        }
        let mut variant = base.clone();
        variant.form_id = (own_index << 24) | next;
        used.insert(next);
        next += 1;
        set_bytes(
            &mut variant,
            "EDID",
            format!(
                "B21_ShelterScenery_{:06X}\0",
                reference.form_id & 0x00FF_FFFF
            )
            .into_bytes(),
        );
        set_bytes(
            &mut variant,
            "MODL",
            format!("{}\0", generated.replace('/', "\\")).into_bytes(),
        );
        if let Some(bounds) = bytes(&variant, "OBND").filter(|b| b.len() == 12) {
            let bounds: Vec<u8> = bounds
                .chunks_exact(2)
                .enumerate()
                .flat_map(|(i, v)| {
                    let value = i16::from_le_bytes(v.try_into().unwrap()) as f32 + offset[i % 3];
                    (value.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16).to_le_bytes()
                })
                .collect();
            set_bytes(&mut variant, "OBND", bounds);
        }
        let mut data = bytes(&reference, "DATA").unwrap().to_vec();
        for axis in 0..3 {
            data[axis * 4..axis * 4 + 4].copy_from_slice(&position[axis].to_le_bytes());
        }
        set_bytes(&mut reference, "DATA", data);
        set_bytes(
            &mut reference,
            "NAME",
            variant.form_id.to_le_bytes().to_vec(),
        );
        std::fs::create_dir_all(output.parent().unwrap())
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        nif.save(Some(output))
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        changes.push((variant, reference));
    }
    if !changes.is_empty() {
        let mut store = plugin_handle_store_ref()
            .lock()
            .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
        let slot = store
            .get_mut(&ctx.run.target_handle_id)
            .ok_or_else(|| PhaseError::Internal("missing target plugin".into()))?;
        report.assets_written = changes.len() as u32;
        report.records_changed = changes.len() as u32 * 2;
        for (variant, reference) in changes {
            insert_parsed_record_in_slot(slot, variant);
            if !replace_parsed_record_contents_in_slot(slot, reference) {
                return Err(PhaseError::Internal("shelter reference disappeared".into()));
            }
        }
        slot.invalidate_sections();
    }
    let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
        phase: "regenerate_modt",
        level: LogLevel::Info,
        message: format!(
            "shelter scenery: reanchored={} skipped={}",
            report.assets_written, report.warnings
        ),
    });
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use esp_authoring_core::plugin_runtime::{
        COMPRESSED_RECORD_FLAG, ParsedGroup, compress_subrecords_payload,
        plugin_handle_close_native, plugin_handle_new_native, plugin_handle_save_no_py,
    };
    use std::path::Path;

    const BASE: u32 = 0x0078_1982;
    const FIRST: u32 = 0x008B_4CF9;
    const SECOND: u32 = 0x008B_4EE9;

    fn record(signature: &str, form_id: u32) -> ParsedRecord {
        ParsedRecord {
            signature: signature.into(),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: Some(0),
            subrecords: Vec::new(),
            raw_payload: None,
            parse_error: None,
        }
    }

    fn reference(form_id: u32, transform: [f32; 6], scale: f32) -> ParsedRecord {
        let mut record = record("REFR", form_id);
        set_bytes(&mut record, "NAME", BASE.to_le_bytes().to_vec());
        set_bytes(
            &mut record,
            "DATA",
            transform
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        set_bytes(&mut record, "XSCL", scale.to_le_bytes().to_vec());
        record
    }

    fn examples() -> [ParsedRecord; 2] {
        [
            reference(
                FIRST,
                [
                    -92522.5625,
                    -22994.029,
                    3009.4644,
                    -0.102412455,
                    -0.06863723,
                    2.0471778,
                ],
                2.6699998,
            ),
            reference(
                SECOND,
                [
                    -20487.309,
                    -84517.78,
                    -216.85945,
                    -0.035275515,
                    -0.0071857702,
                    1.3337246,
                ],
                3.49,
            ),
        ]
    }

    fn world_point(record: &ParsedRecord, point: [f32; 3]) -> [f64; 3] {
        let data = floats(&bytes(record, "DATA").unwrap());
        let scale = floats(&bytes(record, "XSCL").unwrap())[0] as f64;
        let [x, y, z] = [data[3] as f64, data[4] as f64, data[5] as f64];
        let (sx, cx) = (-x).sin_cos();
        let (sy, cy) = (-y).sin_cos();
        let (sz, cz) = (-z).sin_cos();
        let matrix = [
            [cy * cz, -cy * sz, sy],
            [cx * sz + sx * sy * cz, cx * cz - sx * sy * sz, -sx * cy],
            [sx * sz - cx * sy * cz, sx * cz + cx * sy * sz, cx * cy],
        ];
        [0, 1, 2].map(|i| {
            data[i] as f64 + scale * (0..3).map(|j| matrix[i][j] * point[j] as f64).sum::<f64>()
        })
    }

    #[test]
    fn reported_rotated_scaled_mountains_keep_world_positions() {
        for original in examples() {
            let (position, offset) = anchor(&original).unwrap();
            assert!(position[..2].iter().all(|v| v.abs() <= ANCHOR_LIMIT));
            let mut moved = original.clone();
            let mut data = bytes(&moved, "DATA").unwrap().to_vec();
            for i in 0..3 {
                data[i * 4..i * 4 + 4].copy_from_slice(&position[i].to_le_bytes());
            }
            set_bytes(&mut moved, "DATA", data);
            for point in [
                [0.0; 3],
                [2326.0, -7568.0, 863.0],
                [-6420.0, 24400.0, 1206.0],
            ] {
                let expected = world_point(&original, point);
                let actual = world_point(&moved, [0, 1, 2].map(|i| point[i] + offset[i]));
                assert!(
                    (0..3).all(|i| (expected[i] - actual[i]).abs() < 0.03),
                    "{expected:?} != {actual:?}"
                );
            }
            assert!(anchor(&moved).is_none());
        }
        assert!(anchor(&reference(0x800, [10.0, -20.0, 0.0, 0.0, 0.0, 0.0], 8.0)).is_none());
    }

    fn group(kind: i32, label: [u8; 4], children: Vec<ParsedItem>) -> ParsedItem {
        ParsedItem::Group(ParsedGroup {
            group_type: kind,
            label,
            tail: Bytes::new(),
            children,
        })
    }

    fn cell(id: u32, name: &str, refs: Vec<ParsedRecord>) -> Vec<ParsedItem> {
        let mut cell = record("CELL", id);
        set_bytes(&mut cell, "EDID", format!("{name}\0").into_bytes());
        set_bytes(&mut cell, "DATA", vec![1, 0]);
        vec![
            ParsedItem::Record(cell),
            group(
                6,
                id.to_le_bytes(),
                vec![group(
                    9,
                    id.to_le_bytes(),
                    refs.into_iter().map(ParsedItem::Record).collect(),
                )],
            ),
        ]
    }

    fn run_repair(handle: u64, root: &Path) -> PhaseReport {
        run_repair_with_assets(handle, root, None)
    }

    fn run_repair_with_assets(handle: u64, root: &Path, assets: Option<&Path>) -> PhaseReport {
        use crate::run::{RunConfig, RunError, RunParams, create_run, drop_run, with_run};
        use crate::translator::Game;
        let id = create_run(RunParams {
            source: Game::Fo76,
            target: Game::Fo4,
            source_handle_id: 0,
            target_handle_id: handle,
            master_handle_ids: vec![],
            config: RunConfig::default(),
        })
        .unwrap();
        let report = with_run(id, |run| -> Result<PhaseReport, RunError> {
            let report = repair(&mut PhaseCtx {
                run,
                mod_path: root,
                source_extracted_dir: root,
                target_extracted_dir: assets,
                target_data_dir: None,
                params: &serde_json::Value::Null,
                cancel: &std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            })
            .map_err(|e| RunError::InvalidConfig(e.to_string()))?;
            if assets.is_some() {
                let mut reasons = BTreeMap::<String, usize>::new();
                for event in run.event_rx.try_iter() {
                    if let PhaseEvent::Log { message, .. } = event {
                        if let Some((_, reason)) = message.split_once("left unchanged: ") {
                            *reasons.entry(reason.to_string()).or_default() += 1;
                        }
                    }
                }
                eprintln!("live shelter exclusions: {reasons:?}");
            }
            Ok(report)
        })
        .unwrap();
        drop_run(id).unwrap();
        report
    }

    #[test]
    #[ignore = "requires the locally converted SeventySix master and meshes"]
    fn live_shelter_records_reanchor_in_memory_with_temporary_mesh_outputs() {
        use esp_authoring_core::plugin_runtime::plugin_handle_load_no_py;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../mods/SeventySix");
        let handle = plugin_handle_load_no_py(
            root.join("SeventySix.esm").to_str().unwrap(),
            Some("fo4"),
            None,
            None,
            true,
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let report = run_repair_with_assets(handle, temp.path(), Some(&root.join("data")));
        eprintln!(
            "live shelter repair: refs={} warnings={}",
            report.assets_written, report.warnings
        );
        assert!(report.assets_written >= 2);
        {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&handle).unwrap();
            let mut found = 0;
            walk(&slot.parsed.root_items, &mut |r| {
                if [FIRST, SECOND].contains(&(r.form_id & 0x00FF_FFFF)) {
                    found += 1;
                    assert!(anchor(r).is_none());
                    let original = examples()
                        .into_iter()
                        .find(|original| original.form_id == r.form_id & 0x00FF_FFFF)
                        .unwrap();
                    assert_eq!(bytes(r, "XSCL"), bytes(&original, "XSCL"));
                    assert_eq!(
                        &bytes(r, "DATA").unwrap()[12..],
                        &bytes(&original, "DATA").unwrap()[12..]
                    );
                }
            });
            assert_eq!(found, 2);
        }
        plugin_handle_close_native(handle);
    }

    #[test]
    fn repair_clones_shared_base_preserves_topology_and_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let model = "setdressing/shelters/mountain.nif";
        let mesh = temp.path().join("data/meshes").join(model);
        std::fs::create_dir_all(mesh.parent().unwrap()).unwrap();
        let mut nif = NifFile::new("fo4");
        let child = nif.add_block("NiNode", None);
        nif.blocks[0].set_field(
            "Children",
            NifValue::Array(vec![NifValue::Ref(child as i32)]),
        );
        nif.blocks[0].set_field("Num Children", NifValue::UInt(1));
        nif.save(Some(mesh)).unwrap();
        let handle = plugin_handle_new_native("Test.esp", Some("fo4")).unwrap();
        let mut base = record("STAT", BASE);
        set_bytes(
            &mut base,
            "EDID",
            b"Shelters_SummerCamp_Mountains02\0".to_vec(),
        );
        set_bytes(&mut base, "MODL", format!("{model}\0").into_bytes());
        let mut refs = examples().to_vec();
        refs[0].raw_payload = Some(Bytes::from(
            compress_subrecords_payload(&refs[0].subrecords).unwrap(),
        ));
        refs[0].subrecords.clear();
        refs[0].flags = COMPRESSED_RECORD_FLAG;
        let mut ordinary_ref = examples()[0].clone();
        ordinary_ref.form_id = 0x800;
        let mut cells = cell(0x8B4C48, "SheltersMoonlightCamp", refs);
        cells.extend(cell(0x900, "OtherInterior", vec![ordinary_ref]));
        {
            let mut store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            slot.parsed.root_items = vec![
                group(0, *b"STAT", vec![ParsedItem::Record(base.clone())]),
                group(0, *b"CELL", cells),
            ];
            slot.parsed.header.next_object_id = 0x800;
            slot.invalidate_sections();
        }
        let report = run_repair(handle, temp.path());
        assert_eq!(
            (
                report.records_changed,
                report.assets_written,
                report.warnings
            ),
            (4, 2, 0)
        );
        assert_eq!(run_repair(handle, temp.path()).records_changed, 0);
        {
            let store = plugin_handle_store_ref().lock().unwrap();
            let slot = store.get(&handle).unwrap();
            let mut records = BTreeMap::new();
            walk(&slot.parsed.root_items, &mut |r| {
                records.insert(r.form_id, decoded(r));
            });
            assert_eq!(bytes(&records[&BASE], "MODL"), bytes(&base, "MODL"));
            assert_eq!(
                bytes(&records[&0x800], "NAME"),
                Some(Bytes::copy_from_slice(&BASE.to_le_bytes()))
            );
            let mut refs = Vec::new();
            shelter_refs(
                &slot.parsed.root_items,
                &BTreeSet::from([0x8B4C48]),
                &mut refs,
            );
            assert_eq!(refs.len(), 2);
            let ids: BTreeSet<u32> = refs
                .iter()
                .map(|r| u32::from_le_bytes(bytes(r, "NAME").unwrap().as_ref().try_into().unwrap()))
                .collect();
            assert_eq!(ids.len(), 2);
            for r in refs {
                assert!(anchor(&r).is_none());
            }
        }
        plugin_handle_save_no_py(handle, temp.path().join("fixed.esp").to_str().unwrap()).unwrap();
        plugin_handle_close_native(handle);
        let fixed = NifFile::load(
            temp.path()
                .join("data/meshes/B21/ShelterScenery/8B4CF9.nif"),
        )
        .unwrap();
        assert_eq!(fixed.blocks.len(), nif.blocks.len() + 1);
        assert_eq!(
            translation(fixed.blocks[child].get_field("Translation")),
            translation(nif.blocks[child].get_field("Translation"))
        );
    }

    #[test]
    fn animated_or_colliding_meshes_are_left_intact() {
        for block in [
            "bhkCollisionObject",
            "NiTransformController",
            "BSSkin::Instance",
            "NiParticleSystem",
        ] {
            let mut nif = NifFile::new("fo4");
            nif.add_block(block, None);
            let original = nif.blocks.len();
            assert!(offset_scenery(&mut nif, [100.0; 3]).is_err());
            assert_eq!(nif.blocks.len(), original);
        }
    }

    #[test]
    #[ignore = "requires the locally converted FO76 shelter mesh"]
    fn converted_mountain_mesh_roundtrip_preserves_all_vertices() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../mods/SeventySix/data/meshes/setdressing/shelters/shelters_summercamp/Shelters_SummerCamp_MountainForest02.nif");
        let original = NifFile::load(path).unwrap();
        for reference in examples() {
            let mut nif = original.clone();
            let (_, offset) = anchor(&reference).unwrap();
            offset_scenery(&mut nif, offset).unwrap();
            let encoded = nif.to_bytes().unwrap();
            let reread = NifFile::from_bytes(&encoded, None).unwrap();
            for (id, block) in original.blocks.iter().enumerate() {
                for field in [
                    "Vertex Data",
                    "Triangles",
                    "Translation",
                    "Rotation",
                    "Scale",
                    "Bounding Sphere",
                ] {
                    assert_eq!(
                        reread.blocks[id].get_field(field),
                        block.get_field(field),
                        "block {id} {field}"
                    );
                }
            }
            assert_eq!(
                translation(reread.blocks.last().unwrap().get_field("Translation")),
                Some(offset)
            );
        }
    }
}
