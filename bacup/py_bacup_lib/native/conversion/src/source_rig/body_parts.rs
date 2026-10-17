use std::collections::BTreeMap;

use nif_core_native::skin::creature_parts::CreatureBodyPartSegment;
use serde::{Deserialize, Serialize};

use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::struct_relayout::{StructRelayoutCtx, relayout_struct_bytes};
use crate::sym::StringInterner;

use super::{CreatureBodyPartProjection, CreatureTargetRecordReference, TargetFormKey};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatureSourceBodyPart {
    pub name: String,
    pub node: String,
    pub vats_target: String,
    pub gore_target: String,
    pub limb_replacement: String,
    pub geometry_segment_index: u8,
    pub target_data: Vec<u8>,
    pub references: BTreeMap<String, CreatureTargetRecordReference>,
}

struct SourceRow<'a> {
    name: String,
    node: String,
    vats_target: String,
    gore_target: String,
    limb_replacement: String,
    data: &'a [u8],
}

fn source_rows<'a>(
    record: &'a Record,
    interner: &StringInterner,
) -> Result<Vec<SourceRow<'a>>, String> {
    let mut result = Vec::new();
    let mut name = String::new();
    let mut current: Option<SourceRow<'a>> = None;
    for field in &record.fields {
        let string = || match &field.value {
            FieldValue::String(value) => interner.resolve(*value).unwrap_or_default().to_string(),
            _ => String::new(),
        };
        match field.sig.as_str() {
            "BPTN" => name = string(),
            "BPNN" => {
                if let Some(row) = current.take() {
                    result.push(row);
                }
                current = Some(SourceRow {
                    name: std::mem::take(&mut name),
                    node: string(),
                    vats_target: String::new(),
                    gore_target: String::new(),
                    limb_replacement: String::new(),
                    data: &[],
                });
            }
            "BPNT" => {
                if let Some(row) = &mut current {
                    row.vats_target = string();
                }
            }
            "NAM4" => {
                if let Some(row) = &mut current {
                    row.gore_target = string();
                }
            }
            "NAM1" => {
                if let Some(row) = &mut current {
                    row.limb_replacement = string();
                }
            }
            "BPND" => {
                let FieldValue::Bytes(bytes) = &field.value else {
                    return Err("source BPTD row is not raw BPND".to_string());
                };
                if bytes.len() != 84 {
                    return Err(format!(
                        "source BPTD row has {} bytes, expected 84",
                        bytes.len()
                    ));
                }
                current.as_mut().ok_or("BPND has no preceding BPNN")?.data = bytes;
            }
            _ => {}
        }
    }
    if let Some(row) = current {
        result.push(row);
    }
    if result.is_empty()
        || result
            .iter()
            .any(|row| row.node.is_empty() || row.data.len() != 84)
    {
        return Err("source BPTD has missing nodes or data".to_string());
    }
    Ok(result)
}

pub fn creature_body_part_segments(
    records: &[&Record],
    game: &str,
    interner: &StringInterner,
) -> Result<Vec<CreatureBodyPartSegment>, String> {
    let mut nodes = BTreeMap::<String, (String, Option<u16>)>::new();
    for record in records {
        for row in source_rows(record, interner)? {
            if game == "skyrimse" && row.data[5] == 5 {
                continue;
            }
            let partition = matches!(game, "fnv" | "fo3").then_some(u16::from(row.data[5]));
            let entry = nodes
                .entry(row.node.to_ascii_lowercase())
                .or_insert((row.node, partition));
            if entry.1 != partition {
                entry.1 = None;
            }
        }
    }
    if nodes.len() > 255 {
        return Err("creature body-part plan exceeds FO4 segment indices".to_string());
    }
    Ok(nodes
        .into_values()
        .enumerate()
        .map(
            |(index, (node, source_partition))| CreatureBodyPartSegment {
                node,
                index: if index >= 32 {
                    (index + 1) as u8
                } else {
                    index as u8
                },
                source_partition,
            },
        )
        .collect())
}

pub fn creature_saddle_nodes(
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<String>, String> {
    Ok(source_rows(record, interner)?
        .into_iter()
        .filter(|row| row.data[5] == 5)
        .map(|row| row.node)
        .collect())
}

pub(crate) fn source_body_part_form_references(
    record: &Record,
    game: &str,
    plugin_name: &str,
    masters: &[String],
    interner: &StringInterner,
) -> Result<Vec<(String, crate::ids::FormKey)>, String> {
    if record.sig.as_str() != "BPTD" {
        return Ok(Vec::new());
    }
    let schema = AuthoringSchema::for_game(game)?;
    let layout = schema.struct_field_layout_versioned(
        "BPTD",
        "BPND",
        Some(if game == "skyrimse" { 44 } else { 15 }),
    );
    let mut output = Vec::new();
    for (index, field) in record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.as_str() == "BPND")
    {
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        for member in layout
            .iter()
            .filter(|member| !member.formlink_targets.is_empty())
        {
            let bytes = bytes
                .get(member.offset..member.offset + 4)
                .ok_or("truncated BPND reference")?;
            let raw = u32::from_le_bytes(bytes.try_into().unwrap());
            let local = raw & 0x00ff_ffff;
            if local == 0 {
                continue;
            }
            let owner_index = (raw >> 24) as usize;
            let owner = if owner_index == masters.len() {
                plugin_name
            } else {
                masters
                    .get(owner_index)
                    .map(String::as_str)
                    .ok_or("BPND reference has invalid source master index")?
            };
            output.push((
                format!("BPND[{index}].{}", member.field_id),
                crate::ids::FormKey {
                    local,
                    plugin: interner.intern(owner),
                },
            ));
        }
    }
    Ok(output)
}

pub fn creature_body_part_replacement_models(
    records: &[&Record],
    interner: &StringInterner,
) -> Vec<String> {
    records
        .iter()
        .flat_map(|record| &record.fields)
        .filter(|field| field.sig.as_str() == "NAM1")
        .filter_map(|field| match &field.value {
            FieldValue::String(value) => interner
                .resolve(*value)
                .map(|value| value.trim().replace('\\', "/").to_ascii_lowercase()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub fn project_source_body_parts(
    record: &Record,
    game: &str,
    root_node: &str,
    segments: &[CreatureBodyPartSegment],
    interner: &StringInterner,
    mut reference: impl FnMut(usize, &str, u32) -> Result<CreatureTargetRecordReference, String>,
    mut model: impl FnMut(&str) -> Result<String, String>,
) -> Result<CreatureBodyPartProjection, String> {
    let source_schema = AuthoringSchema::for_game(game)?;
    let target_schema = AuthoringSchema::for_game("fo4")?;
    let source_layout = source_schema.struct_field_layout_versioned(
        "BPTD",
        "BPND",
        Some(if game == "skyrimse" { 44 } else { 15 }),
    );
    let target_layout = target_schema.struct_field_layout_versioned("BPTD", "BPND", Some(131));
    let offset = |name: &str| {
        target_layout
            .iter()
            .find(|field| field.field_id == name)
            .map(|field| field.offset)
            .ok_or_else(|| format!("FO4 BPND lacks {name}"))
    };
    let mut parts = Vec::new();
    for (index, row) in source_rows(record, interner)?.into_iter().enumerate() {
        if game == "skyrimse" && row.data[5] == 5 {
            continue;
        }
        let segment = segments
            .iter()
            .find(|segment| segment.node.eq_ignore_ascii_case(&row.node))
            .ok_or("BPTD node absent from family segment plan")?;
        let mut target_data = relayout_struct_bytes(
            "BPTD",
            "BPND",
            row.data,
            &source_schema,
            Some(if game == "skyrimse" { 44 } else { 15 }),
            &StructRelayoutCtx {
                target_schema: &target_schema,
                target_form_version: 131,
                legacy_bptd_only: game != "skyrimse",
            },
        )
        .ok_or("source BPTD layout cannot be converted")?;
        target_data[offset("flags")?] = row.data[4] & 0x49;
        if matches!(game, "fnv" | "fo3") && row.data[4] & 0x09 != 0 {
            // FO4's old segmented-limb path selects the authored 100/101 meatcap subsegments.
            target_data[offset("flags")?] |= 0x10;
        }
        if game == "skyrimse" {
            if row.data[5] > 4 {
                return Err(format!(
                    "Skyrim body-part type {} has no FO4 runtime equivalent",
                    row.data[5]
                ));
            }
            if row.data[7] != 0xff {
                return Err(format!(
                    "Skyrim body-part actor value {} has no verified FO4 mapping",
                    row.data[7]
                ));
            }
            target_data[offset("actor_value")?..offset("actor_value")? + 4].fill(0);
            target_data[offset("explodable_limb_replacement_scale")?
                ..offset("explodable_limb_replacement_scale")? + 4]
                .copy_from_slice(&row.data[80..84]);
        }
        target_data[offset("geometry_segment_index")?] = segment.index;
        let mut references = BTreeMap::new();
        for field in &source_layout {
            if field.formlink_targets.is_empty() {
                continue;
            }
            let raw =
                u32::from_le_bytes(row.data[field.offset..field.offset + 4].try_into().unwrap());
            if raw & 0x00ff_ffff != 0 {
                references.insert(
                    field.field_id.to_string(),
                    reference(index, field.field_id, raw)?,
                );
            }
        }
        if matches!(game, "fnv" | "fo3") && (25..=31).contains(&row.data[7]) {
            references.insert(
                "actor_value".to_string(),
                CreatureTargetRecordReference::new(
                    "AVIF",
                    TargetFormKey::new(0x36c + u32::from(row.data[7] - 25), "Fallout4.esm"),
                ),
            );
        }
        parts.push(CreatureSourceBodyPart {
            name: if row.name.is_empty() {
                row.node.clone()
            } else {
                row.name
            },
            node: row.node,
            vats_target: row.vats_target,
            gore_target: row.gore_target,
            limb_replacement: if row.limb_replacement.is_empty() {
                String::new()
            } else {
                model(&row.limb_replacement)?
            },
            geometry_segment_index: segment.index,
            target_data,
            references,
        });
    }
    Ok(CreatureBodyPartProjection::SourceParts {
        root_node: root_node.to_string(),
        parts,
    })
}

pub(crate) fn body_part_data_value(
    part: &CreatureSourceBodyPart,
    interner: &StringInterner,
) -> Result<FieldValue, String> {
    let schema = AuthoringSchema::for_game("fo4")?;
    let definition = schema
        .record_def("BPTD")
        .and_then(|record| record.subrecord_def("BPND"))
        .ok_or("FO4 BPND schema missing")?;
    let layout = schema.struct_field_layout_versioned("BPTD", "BPND", Some(131));
    let mut fields = Vec::new();
    for item in layout {
        let bytes = part
            .target_data
            .get(item.offset..item.offset + item.width)
            .ok_or("FO4 body-part data truncated")?;
        let value = if let Some(reference) = part.references.get(item.field_id) {
            FieldValue::FormKey(reference.form_key.intern(interner))
        } else if !item.formlink_targets.is_empty() {
            FieldValue::Uint(0)
        } else if definition
            .fields
            .iter()
            .any(|field| field.id == item.field_id && field.kind == "float32")
        {
            FieldValue::Float(f32::from_le_bytes(
                bytes.try_into().map_err(|_| "bad float width")?,
            ))
        } else {
            let mut wide = [0u8; 8];
            wide[..bytes.len()].copy_from_slice(bytes);
            FieldValue::Uint(u64::from_le_bytes(wide))
        };
        fields.push((interner.intern(item.field_id), value));
    }
    Ok(FieldValue::Struct(fields))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    fn fixture(interner: &StringInterner, game: &str) -> Record {
        let mut record = Record::new(
            SigCode(*b"BPTD"),
            FormKey {
                local: 0x800,
                plugin: interner.intern("Source.esm"),
            },
        );
        let mut data = vec![0; 84];
        data[0..4].copy_from_slice(&2.5f32.to_le_bytes());
        data[4] = 0x59;
        data[5] = if game == "skyrimse" { 1 } else { 3 };
        data[6] = 45;
        data[7] = if game == "skyrimse" { 255 } else { 27 };
        data[8] = 75;
        data[12..16].copy_from_slice(&0x1234u32.to_le_bytes());
        data[80..84].copy_from_slice(&1.75f32.to_le_bytes());
        for (sig, value) in [
            ("BPTN", FieldValue::String(interner.intern("Arm"))),
            ("BPNN", FieldValue::String(interner.intern("ArmUpper"))),
            ("BPNT", FieldValue::String(interner.intern("ArmLower"))),
            ("BPND", FieldValue::Bytes(data.into())),
            ("NAM1", FieldValue::String(interner.intern("Gore\\arm.nif"))),
            ("NAM4", FieldValue::String(interner.intern("ArmLower"))),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value,
            });
        }
        record
    }

    #[test]
    fn source_limb_damage_vats_conditions_and_gore_close_over_segments() {
        for game in ["fnv", "fo3", "skyrimse"] {
            let interner = StringInterner::new();
            let source = fixture(&interner, game);
            let plan = creature_body_part_segments(&[&source], game, &interner).unwrap();
            let projected = project_source_body_parts(
                &source,
                game,
                "Root",
                &plan,
                &interner,
                |row, field, raw| {
                    assert_eq!((row, field, raw), (0, "explodable_debris", 0x1234));
                    Ok(CreatureTargetRecordReference::new(
                        "DEBR",
                        TargetFormKey::new(0x900, "Target.esm"),
                    ))
                },
                |path| Ok(format!("Actors\\Converted\\{path}")),
            )
            .unwrap();
            let CreatureBodyPartProjection::SourceParts { parts, .. } = projected else {
                panic!("source parts")
            };
            assert_eq!(parts.len(), 1);
            let part = &parts[0];
            assert_eq!(part.vats_target, "ArmLower");
            assert_eq!(part.geometry_segment_index, plan[0].index);
            assert_eq!(part.limb_replacement, "Actors\\Converted\\Gore\\arm.nif");
            let FieldValue::Struct(values) = body_part_data_value(part, &interner).unwrap() else {
                panic!("BPND")
            };
            let values = values
                .into_iter()
                .map(|(key, value)| (interner.resolve(key).unwrap().to_string(), value))
                .collect::<BTreeMap<_, _>>();
            assert_eq!(values["damage_mult"], FieldValue::Float(2.5));
            assert_eq!(values["health_percent"], FieldValue::Uint(45));
            assert_eq!(values["to_hit_chance"], FieldValue::Uint(75));
            assert_eq!(
                values["flags"],
                FieldValue::Uint(if game == "skyrimse" { 0x49 } else { 0x59 })
            );
            assert_eq!(
                values["explodable_limb_replacement_scale"],
                FieldValue::Float(1.75)
            );
            assert_eq!(
                values["part_type"],
                FieldValue::Uint(if game == "skyrimse" { 1 } else { 6 })
            );
            assert_eq!(
                values["explodable_debris"],
                FieldValue::FormKey(FormKey {
                    local: 0x900,
                    plugin: interner.intern("Target.esm")
                })
            );
            assert_eq!(
                values["actor_value"],
                if game == "skyrimse" {
                    FieldValue::Uint(0)
                } else {
                    FieldValue::FormKey(FormKey {
                        local: 0x36e,
                        plugin: interner.intern("Fallout4.esm"),
                    })
                }
            );
        }
    }

    #[test]
    fn tumbleweed_torso_retains_absolute_explosion_and_source_payload() {
        let interner = StringInterner::new();
        let mut source = fixture(&interner, "fnv");
        source.form_key = FormKey {
            local: 0x163cf3,
            plugin: interner.intern("FalloutNV.esm"),
        };
        source
            .fields
            .retain(|field| matches!(field.sig.as_str(), "BPNN" | "BPND"));
        source.fields[0].value = FieldValue::String(interner.intern("Bip01 Root"));
        source.fields[1].value = FieldValue::Bytes(hex::decode(
            "0000803F4800051A6464000000000000BA820100000000000000803F0000000000000000000000000000803F0000000000000000000000000000000000000000000000000000000000000000000000000000803F"
        ).unwrap().into());
        let plan = creature_body_part_segments(&[&source], "fnv", &interner).unwrap();
        let projection = project_source_body_parts(
            &source,
            "fnv",
            "Bip01 Root",
            &plan,
            &interner,
            |row, field, raw| {
                assert_eq!((row, field, raw), (0, "explodable_explosion", 0x182ba));
                Ok(CreatureTargetRecordReference::new(
                    "EXPL",
                    TargetFormKey::new(0x900, "Target.esm"),
                ))
            },
            |_| panic!("Tumbleweed has no replacement model"),
        )
        .unwrap();
        let CreatureBodyPartProjection::SourceParts { parts, .. } = projection else {
            panic!("source parts")
        };
        let FieldValue::Struct(values) = body_part_data_value(&parts[0], &interner).unwrap() else {
            panic!("BPND")
        };
        let values = values
            .into_iter()
            .map(|(key, value)| (interner.resolve(key).unwrap().to_string(), value))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(values["part_type"], FieldValue::Uint(0));
        assert_eq!(values["flags"], FieldValue::Uint(0x58));
        assert_eq!(values["explodable_explosion_chance"], FieldValue::Uint(100));
        assert_eq!(
            values["actor_value"],
            FieldValue::FormKey(FormKey {
                local: 0x36d,
                plugin: interner.intern("Fallout4.esm")
            })
        );
        assert_eq!(
            parts[0].references["explodable_explosion"].form_key,
            TargetFormKey::new(0x900, "Target.esm")
        );
    }

    #[test]
    #[ignore = "requires installed source game plugins and a B21_CREATURE_BODY_PART_PLUGINS JSON manifest"]
    fn real_source_body_part_corpus() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_load_lazy_records_no_py,
        };
        let manifest_path =
            std::path::PathBuf::from(std::env::var("B21_CREATURE_BODY_PART_PLUGINS").unwrap());
        let entries: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        let mut report = Vec::new();
        for entry in entries {
            let game = entry["game"].as_str().unwrap();
            let path = entry["path"].as_str().unwrap();
            let handle =
                plugin_handle_load_lazy_records_no_py(path, game, None, &[*b"BPTD"]).unwrap();
            let keys = {
                let mut session = crate::session::open_session(handle, None).unwrap();
                session
                    .handle_raw_scan(handle)
                    .unwrap()
                    .form_keys_of_signature("BPTD")
            };
            let interner = StringInterner::new();
            let schema = AuthoringSchema::for_game(game).unwrap();
            for key in keys {
                let source =
                    crate::source_read::read_record(handle, &key, &schema, &interner).unwrap();
                let result =
                    creature_body_part_segments(&[&source], game, &interner).and_then(|plan| {
                        project_source_body_parts(
                            &source,
                            game,
                            "Root",
                            &plan,
                            &interner,
                            |_, field, raw| {
                                Ok(CreatureTargetRecordReference::new(
                                    if field.ends_with("debris") {
                                        "DEBR"
                                    } else if field.ends_with("explosion") {
                                        "EXPL"
                                    } else {
                                        "IPDS"
                                    },
                                    TargetFormKey::new(raw & 0x00ff_ffff, "Target.esm"),
                                ))
                            },
                            |path| Ok(path.to_string()),
                        )
                        .map(|projection| (projection, plan))
                    });
                let skeleton = source
                    .fields
                    .iter()
                    .find(|field| field.sig.as_str() == "MODL")
                    .and_then(|field| match &field.value {
                        FieldValue::String(value) => {
                            interner.resolve(*value).map(|value| value.to_string())
                        }
                        _ => None,
                    });
                match result {
                    Ok((CreatureBodyPartProjection::SourceParts{parts,..},plan)) => {
                        for part in &parts { body_part_data_value(part,&interner).unwrap(); }
                        report.push(serde_json::json!({"game":game,"source":key,"skeleton":skeleton,"parts":parts,"segments":plan}));
                    }
                    Ok(_) => panic!("source parts expected"),
                    Err(error) => report.push(serde_json::json!({"game":game,"source":key,"skeleton":skeleton,"error":error})),
                }
            }
            assert!(plugin_handle_close_native(handle));
        }
        let output = manifest_path.with_file_name("creature_body_part_audit.json");
        std::fs::write(&output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        println!(
            "body-part source records={}, projected={}, report={}",
            report.len(),
            report
                .iter()
                .filter(|row| row.get("error").is_none())
                .count(),
            output.display()
        );
        assert!(!report.is_empty());
        let failures = report
            .iter()
            .filter(|row| row.get("error").is_some())
            .collect::<Vec<_>>();
        assert!(
            failures.is_empty(),
            "{}",
            serde_json::to_string_pretty(&failures).unwrap()
        );
    }

    #[test]
    fn body_part_references_resolve_master_slots_without_local_id_aliases() {
        let interner = StringInterner::new();
        let mut source = fixture(&interner, "skyrimse");
        let FieldValue::Bytes(data) = &mut source.fields[3].value else {
            panic!("BPND");
        };
        let mut bytes = data.to_vec();
        bytes[12..16].copy_from_slice(&0x00001234u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&0x01001234u32.to_le_bytes());
        *data = bytes.into();
        let masters = ["Skyrim.esm".to_string()];
        let refs = source_body_part_form_references(
            &source,
            "skyrimse",
            "Dawnguard.esm",
            &masters,
            &interner,
        )
        .unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(
            refs[0],
            (
                "BPND[3].explodable_debris".to_string(),
                FormKey {
                    local: 0x1234,
                    plugin: interner.intern("Skyrim.esm")
                }
            )
        );
        assert_eq!(
            refs[1],
            (
                "BPND[3].explodable_explosion".to_string(),
                FormKey {
                    local: 0x1234,
                    plugin: interner.intern("Dawnguard.esm")
                }
            )
        );
        assert!(
            source_body_part_form_references(&source, "skyrimse", "Skyrim.esm", &[], &interner)
                .unwrap_err()
                .contains("master index")
        );
    }

    #[test]
    fn skyrim_saddle_attachment_is_not_emitted_as_fo4_head_two() {
        let interner = StringInterner::new();
        let mut source = fixture(&interner, "skyrimse");
        let FieldValue::Bytes(data) = &mut source.fields[3].value else {
            panic!("BPND");
        };
        let mut bytes = data.to_vec();
        bytes[5] = 5;
        *data = bytes.into();
        assert_eq!(
            creature_saddle_nodes(&source, &interner).unwrap(),
            ["ArmUpper"]
        );
        let plan = creature_body_part_segments(&[&source], "skyrimse", &interner).unwrap();
        assert!(plan.is_empty());
        let projection = project_source_body_parts(
            &source,
            "skyrimse",
            "Root",
            &plan,
            &interner,
            |_, _, _| panic!("saddle has no combat dependencies"),
            |_| panic!("saddle has no replacement mesh"),
        )
        .unwrap();
        assert!(
            matches!(projection, CreatureBodyPartProjection::SourceParts { parts, .. } if parts.is_empty())
        );
    }
}
