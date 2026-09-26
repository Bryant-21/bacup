use std::collections::BTreeMap;
use std::io::Write;

use esp_authoring_core::plugin_runtime::{ParsedSubrecord, effective_subrecords_for_record};
use serde::Serialize;
use serde_json::{Value, json};

use crate::fixups::rewrite_raw_object_template_formids::fo76_condition_reference_offsets;
use crate::ids::{FormKey, SigCode};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::session::open_session;
use crate::translator::Game;

pub struct ObjectiveAreasPhase;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Identity {
    plugin: String,
    object_id: u32,
}

#[derive(Clone)]
struct Record {
    raw: u32,
    identity: Identity,
    masters: Vec<String>,
    file: String,
    signature: String,
    eid: String,
    version: u16,
    fields: Vec<ParsedSubrecord>,
}

#[derive(Clone, Debug)]
struct Target {
    objective: u16,
    ordinal: usize,
    alias: u32,
    flags: u8,
    area_flags: u8,
    keyword: u32,
    radius: u32,
    conditions: Vec<Vec<u8>>,
}

fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn identity(raw: u32, masters: &[String], plugin: &str) -> Result<Identity, PhaseError> {
    let index = (raw >> 24) as usize;
    let owner = if index == masters.len() || index == 0xff {
        plugin
    } else {
        masters.get(index).map(String::as_str).ok_or_else(|| {
            PhaseError::Internal(format!("invalid objective-area FormID {raw:08X}"))
        })?
    };
    Ok(Identity {
        plugin: owner.into(),
        object_id: raw & 0xffffff,
    })
}

fn targets(record: &Record, source: bool) -> Result<Vec<Target>, String> {
    let mut result: Vec<Target> = Vec::new();
    let mut objective = None;
    let mut ordinal = 0;
    let mut target_open = false;
    for field in &record.fields {
        let data = field.data.as_ref();
        match field.signature.as_str() {
            "QOBJ" => {
                if data.len() != 2 {
                    return Err("invalid QOBJ size".into());
                }
                objective = Some(u16::from_le_bytes(data.try_into().unwrap()));
                ordinal = 0;
                target_open = false;
            }
            "QSTA" => {
                let objective = objective.ok_or("QSTA without QOBJ")?;
                let (flags, area_flags, keyword, radius) = if source {
                    // FO76 1.7.25.39 reader AFD460: byte flags before v158; radius from v151.
                    let flags_width = if record.version >= 158 {
                        2
                    } else if record.version >= 151 {
                        1
                    } else {
                        4
                    };
                    let keyword_width = if record.version >= 82 { 4 } else { 0 };
                    let expected =
                        4 + flags_width + keyword_width + if record.version >= 151 { 4 } else { 0 };
                    if data.len() != expected {
                        return Err(format!(
                            "unsupported QSTA v{} size {}",
                            record.version,
                            data.len()
                        ));
                    }
                    (
                        data[4],
                        if record.version >= 158 { data[5] } else { 0 },
                        if keyword_width != 0 {
                            u32_at(data, 4 + flags_width)
                        } else {
                            0
                        },
                        if record.version >= 151 {
                            u32_at(data, 4 + flags_width + keyword_width)
                        } else {
                            0
                        },
                    )
                } else {
                    if data.len() != 12 {
                        return Err(format!("invalid FO4 QSTA size {}", data.len()));
                    }
                    (data[4], 0, u32_at(data, 8), 0)
                };
                result.push(Target {
                    objective,
                    ordinal,
                    alias: u32_at(data, 0),
                    flags,
                    area_flags,
                    keyword,
                    radius,
                    conditions: Vec::new(),
                });
                ordinal += 1;
                target_open = true;
            }
            "CTDA" if target_open => {
                if data.len() != 32 {
                    return Err("unsupported target CTDA size".into());
                }
                result.last_mut().unwrap().conditions.push(data.to_vec());
            }
            "CIS1" | "CIS2" if target_open => {}
            _ => target_open = false,
        }
    }
    Ok(result)
}

impl Phase for ObjectiveAreasPhase {
    fn name(&self) -> &'static str {
        "emit_objective_areas"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        if ctx.mod_path.as_os_str().is_empty() {
            return Err(PhaseError::BadParams(
                "objective areas require mod_path".into(),
            ));
        }
        let existing = ctx
            .params
            .get("match_existing")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if ctx.run.mapper_state.is_none() && !existing {
            return Err(PhaseError::BadParams(
                "objective areas require translation mapping or explicit match_existing".into(),
            ));
        }
        let mut session = open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        let mut read = |handle| -> Result<Vec<Record>, PhaseError> {
            let (masters, plugin) = session
                .handle_load_order(handle)
                .map(|(m, p)| (m.to_vec(), p.to_string()))
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            let scan = session
                .handle_raw_scan(handle)
                .map_err(|e| PhaseError::Internal(e.to_string()))?;
            let mut records = Vec::new();
            for sig in ["QUST", "GLOB", "KYWD"] {
                for raw in scan.raw_form_ids_of_sig(SigCode::from_str(sig).unwrap()) {
                    let record = scan
                        .with_record(raw, |record| {
                            if record.flags & 0x20 != 0 {
                                return Ok(None);
                            }
                            if let Some(error) = &record.parse_error {
                                return Err(PhaseError::Internal(error.clone()));
                            }
                            let fields = effective_subrecords_for_record(record).to_vec();
                            let eid = fields
                                .iter()
                                .find(|f| f.signature == "EDID")
                                .map(|f| {
                                    String::from_utf8_lossy(&f.data)
                                        .trim_end_matches('\0')
                                        .to_string()
                                })
                                .unwrap_or_default();
                            Ok(Some(Record {
                                raw,
                                identity: identity(raw, &masters, &plugin)?,
                                masters: masters.clone(),
                                file: plugin.clone(),
                                signature: sig.into(),
                                eid,
                                version: record.form_version.ok_or_else(|| {
                                    PhaseError::Internal("missing record form version".into())
                                })?,
                                fields,
                            }))
                        })
                        .ok_or_else(|| {
                            PhaseError::Internal(format!("missing record {raw:08X}"))
                        })??;
                    if let Some(record) = record {
                        records.push(record);
                    }
                }
            }
            Ok(records)
        };
        let sources = read(ctx.run.source_handle_id)?;
        let mut outputs = read(ctx.run.target_handle_id)?;
        for &handle in &ctx.run.master_handle_ids {
            outputs.extend(read(handle)?);
        }
        drop(session);
        let source_by_raw: BTreeMap<_, _> = sources.iter().map(|r| (r.raw, r)).collect();
        let mut mapped = BTreeMap::new();
        for source in &sources {
            let source_key = FormKey {
                local: source.identity.object_id,
                plugin: ctx.run.interner.intern(&source.identity.plugin),
            };
            let map = ctx
                .run
                .mapper_state
                .as_ref()
                .and_then(|m| m.source_to_target.get(&source_key));
            let matches: Vec<_> = outputs
                .iter()
                .filter(|output| {
                    if output.signature != source.signature {
                        return false;
                    }
                    if let Some(key) = map {
                        output.identity.object_id == key.local
                            && output.identity.plugin.eq_ignore_ascii_case(
                                ctx.run.interner.resolve(key.plugin).unwrap_or(""),
                            )
                    } else {
                        existing
                            && !source.eid.is_empty()
                            && source.eid.eq_ignore_ascii_case(&output.eid)
                    }
                })
                .collect();
            if let [output] = matches.as_slice() {
                mapped.insert(source.raw, *output);
            }
        }
        let mut areas = Vec::new();
        let mut skipped = Vec::new();
        let mut declarations = 0;
        for source in sources.iter().filter(|r| r.signature == "QUST") {
            ctx.check_cancel()?;
            let source_targets = targets(source, true)
                .map_err(|e| PhaseError::Internal(format!("{}: {e}", source.eid)))?;
            let selected: Vec<_> = source_targets
                .iter()
                .filter(|t| t.radius != 0 || t.area_flags != 0)
                .collect();
            if selected.is_empty() {
                continue;
            }
            declarations += selected.iter().filter(|t| t.radius != 0).count();
            let Some(output) = mapped.get(&source.raw) else {
                skipped.push(json!({"quest": source.eid, "reason": "quest missing or ambiguous", "targets": selected.len()}));
                continue;
            };
            let output_targets = targets(output, false)
                .map_err(|e| PhaseError::Internal(format!("{}: {e}", output.eid)))?;
            for target in selected {
                let correspondence = (|| -> Result<Value, String> {
                    let source_objective: Vec<_> = source_targets
                        .iter()
                        .filter(|t| t.objective == target.objective)
                        .collect();
                    let output_objective: Vec<_> = output_targets
                        .iter()
                        .filter(|t| t.objective == target.objective)
                        .collect();
                    if source_objective.len() != output_objective.len() {
                        return Err("objective target count changed".into());
                    }
                    for (a, b) in source_objective.iter().zip(&output_objective) {
                        if a.alias != b.alias || a.flags != b.flags {
                            return Err("objective target sequence changed".into());
                        }
                        if a.ordinal == target.ordinal
                            && source_objective.iter().any(|t| {
                                t.alias == a.alias
                                    && t.keyword == a.keyword
                                    && (t.radius != a.radius || t.area_flags != a.area_flags)
                            })
                        {
                            if a.conditions.len() != b.conditions.len() {
                                return Err("ambiguous repeated-alias conditions changed".into());
                            }
                            for (before, after) in a.conditions.iter().zip(&b.conditions) {
                                let mut normalized = before.clone();
                                for offset in fo76_condition_reference_offsets(before) {
                                    let raw = u32_at(before, offset);
                                    if raw == 0 {
                                        continue;
                                    }
                                    let expected = if let Some(record) = mapped.get(&raw) {
                                        record.identity.clone()
                                    } else {
                                        let key = identity(raw, &source.masters, &source.file)
                                            .map_err(|e| e.to_string())?;
                                        let source_key = FormKey {
                                            local: key.object_id,
                                            plugin: ctx.run.interner.intern(&key.plugin),
                                        };
                                        if let Some(key) = ctx
                                            .run
                                            .mapper_state
                                            .as_ref()
                                            .and_then(|m| m.source_to_target.get(&source_key))
                                        {
                                            Identity {
                                                plugin: ctx
                                                    .run
                                                    .interner
                                                    .resolve(key.plugin)
                                                    .unwrap_or("")
                                                    .into(),
                                                object_id: key.local,
                                            }
                                        } else {
                                            key
                                        }
                                    };
                                    let actual = identity(
                                        u32_at(after, offset),
                                        &output.masters,
                                        &output.file,
                                    )
                                    .map_err(|e| e.to_string())?;
                                    if expected != actual {
                                        return Err(
                                            "ambiguous repeated-alias conditions changed".into()
                                        );
                                    }
                                    normalized[offset..offset + 4]
                                        .copy_from_slice(&after[offset..offset + 4]);
                                }
                                if normalized[0] != after[0]
                                    || normalized[4..10] != after[4..10]
                                    || normalized[12..32] != after[12..32]
                                {
                                    return Err(
                                        "ambiguous repeated-alias conditions changed".into()
                                    );
                                }
                            }
                        }
                        let expected_keyword = if a.keyword == 0 {
                            None
                        } else {
                            Some(&mapped.get(&a.keyword).ok_or("keyword unmapped")?.identity)
                        };
                        let actual_keyword = if b.keyword == 0 {
                            None
                        } else {
                            Some(
                                identity(b.keyword, &output.masters, &output.file)
                                    .map_err(|e| e.to_string())?,
                            )
                        };
                        if expected_keyword != actual_keyword.as_ref() {
                            return Err("target keyword changed".into());
                        }
                    }
                    let actual = output_objective[target.ordinal];
                    let radius = if target.area_flags & 1 != 0 {
                        if target.radius == 0 {
                            json!({"kind": "literal", "value": 0})
                        } else {
                            let global =
                                mapped.get(&target.radius).ok_or("radius global unmapped")?;
                            if global.signature != "GLOB"
                                || source_by_raw
                                    .get(&target.radius)
                                    .is_none_or(|r| r.signature != "GLOB")
                            {
                                return Err("radius reference is not a global".into());
                            }
                            json!({"kind": "global", "form": global.identity})
                        }
                    } else {
                        json!({"kind": "literal", "value": target.radius})
                    };
                    let conditions: Vec<_> = actual.conditions.iter().map(|c| {
                        let mut offsets = fo76_condition_reference_offsets(c);
                        if u16::from_le_bytes(c[8..10].try_into().unwrap()) == 59 && !offsets.contains(&12) { offsets.push(12); }
                        let parameter = |offset| {
                            let raw = u32_at(c, offset);
                            if offsets.contains(&offset) { json!({"kind": "form", "form": if raw == 0 { None } else { identity(raw, &output.masters, &output.file).ok() }}) }
                            else { json!({"kind": "number", "value": raw}) }
                        };
                        json!({
                        "flags": c[0], "function": u16::from_le_bytes(c[8..10].try_into().unwrap()),
                        "comparison_bits": u32_at(c, 4), "run_on": u32_at(c, 20),
                        "global": if c[0] & 4 != 0 { identity(u32_at(c, 4), &output.masters, &output.file).ok() } else { None },
                        "parameters": [parameter(12), parameter(16)], "run_on_data": parameter(24),
                    })}).collect();
                    Ok(json!({"quest": output.identity, "editor_id": output.eid,
                        "objective": target.objective, "target": target.ordinal, "target_count": output_objective.len(),
                        "alias": actual.alias, "flags": actual.flags, "area_flags": target.area_flags,
                        "keyword": if target.keyword == 0 { None } else { mapped.get(&target.keyword).map(|r| &r.identity) },
                        "conditions": conditions, "radius": radius}))
                })();
                match correspondence {
                    Ok(area) => areas.push(area),
                    Err(reason) => skipped.push(json!({"quest": source.eid, "objective": target.objective, "target": target.ordinal, "reason": reason})),
                }
            }
        }
        let output_plugin = &ctx.run.config.output_plugin_name;
        let catalog = json!({"schema_version": 1, "output_plugin": output_plugin,
            "nonzero_source_declarations": declarations, "areas": areas, "skipped": skipped});
        let directory = ctx
            .mod_path
            .join("F4SE/Plugins/B21_TalesFromAppalachia/ObjectiveAreas");
        std::fs::create_dir_all(&directory).map_err(|e| PhaseError::Internal(e.to_string()))?;
        let filename = std::path::Path::new(output_plugin)
            .file_name()
            .ok_or_else(|| PhaseError::BadParams("missing output filename".into()))?;
        let mut file = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        file.write_all(
            &serde_json::to_vec_pretty(&catalog)
                .map_err(|e| PhaseError::Internal(e.to_string()))?,
        )
        .map_err(|e| PhaseError::Internal(e.to_string()))?;
        file.persist(directory.join(format!("{}.json", filename.to_string_lossy())))
            .map_err(|e| PhaseError::Internal(e.to_string()))?;
        Ok(PhaseReport {
            assets_written: 1,
            warnings: skipped.len() as u32,
            ..PhaseReport::default()
        })
    }
}
