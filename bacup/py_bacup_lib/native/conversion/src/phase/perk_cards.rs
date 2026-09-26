use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Component, Path};

use esp_authoring_core::plugin_runtime::{
    ParsedSubrecord, compiled_schema_for_game_str, effective_subrecords_for_record,
    schema_record_spec, schema_subrecord_spec,
};
use serde_json::{Value, json};

use crate::fixups::rewrite_raw_object_template_formids::fo76_condition_references;
use crate::ids::{FormKey, SigCode};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::record::FieldValue;
use crate::schema::AuthoringSchema;
use crate::session::open_session;
use crate::translator::Game;

pub struct PerkCardsPhase;

const EFFECT_TYPES: &[&str] = &[
    "PERK", "SPEL", "MGEF", "CURV", "CNCY", "PPAK", "LVPC", "LVLP", "FLST", "GLOB", "AVIF", "CNDF",
    "KYWD", "SNDR",
];

fn error(message: impl Into<String>) -> PhaseError {
    PhaseError::Internal(message.into())
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches('\0')
        .to_owned()
}
fn field<'a>(fields: &'a [ParsedSubrecord], signature: &str) -> Option<&'a [u8]> {
    fields
        .iter()
        .find(|f| f.signature == signature)
        .map(|f| f.data.as_ref())
}
fn u32_value(bytes: &[u8]) -> Result<u32, PhaseError> {
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| error("invalid perk catalog FormID width"))?,
    ))
}
fn key(raw: u32, masters: &[String], plugin: &str) -> Result<String, PhaseError> {
    let index = (raw >> 24) as usize;
    let owner = if index == masters.len() {
        plugin
    } else {
        masters
            .get(index)
            .map(String::as_str)
            .ok_or_else(|| error(format!("invalid card FormID {raw:08X}")))?
    };
    Ok(format!("{:06X}:{owner}", raw & 0xffffff))
}
fn curve_path(root: &Path, relative: &str) -> Result<std::path::PathBuf, PhaseError> {
    let normalized = relative.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || normalized.contains(':')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(error(format!("invalid perk curve path {relative:?}")));
    }
    Ok(root.join("misc/curvetables/json").join(path))
}

fn leveled_cards(fields: &[ParsedSubrecord]) -> Result<BTreeSet<u32>, PhaseError> {
    fields
        .iter()
        .filter(|f| f.signature == "LVLO")
        .map(|f| match f.data.len() {
            4 => u32_value(&f.data),
            12 => u32_value(&f.data[4..8]),
            _ => Err(error("unsupported perk selection LVLO layout")),
        })
        .collect()
}

fn form_list(fields: &[ParsedSubrecord]) -> Result<Vec<u32>, PhaseError> {
    fields
        .iter()
        .filter(|f| f.signature == "LNAM")
        .map(|f| u32_value(&f.data))
        .collect()
}

fn curve_values_at(record: &Value, inputs: &[u64]) -> Result<Vec<f64>, PhaseError> {
    let points = record
        .get("curve")
        .and_then(|curve| curve.get("curve"))
        .and_then(Value::as_array)
        .ok_or_else(|| error("perk pack curve has no points"))?;
    inputs
        .iter()
        .map(|input| {
            points
                .iter()
                .find(|point| point.get("x").and_then(Value::as_u64) == Some(*input))
                .and_then(|point| point.get("y"))
                .and_then(Value::as_f64)
                .ok_or_else(|| error(format!("perk pack curve has no value at {input}")))
        })
        .collect()
}

fn pack_curve<'a>(
    records: &'a BTreeMap<String, Value>,
    raw: u32,
    masters: &[String],
    plugin: &str,
) -> Result<(&'a Value, String), PhaseError> {
    let source_key = key(raw, masters, plugin)?;
    let record = records
        .get(&source_key)
        .ok_or_else(|| error(format!("missing perk pack curve {source_key}")))?;
    if record.get("signature").and_then(Value::as_str) != Some("CURV") {
        return Err(error(format!(
            "perk pack reference {source_key} is not a curve"
        )));
    }
    Ok((record, source_key))
}

fn magic_effect_references(
    data: &[u8],
    fo4_schema: &AuthoringSchema,
) -> Result<Vec<u32>, PhaseError> {
    let shift = match data.len() {
        160 => 4,
        152 | 156 => 0,
        _ => {
            return Err(error(format!(
                "unsupported source perk MGEF DATA layout: {} bytes",
                data.len()
            )));
        }
    };
    // FO76's trailing bytes field prevents generic schema traversal. The
    // 160-byte layout adds flags2 at offset 4; 152/156 retain FO4 offsets.
    fo4_schema
        .struct_field_layout_versioned("MGEF", "DATA", Some(131))
        .iter()
        .filter(|f| f.width == 4 && (!f.formlink_targets.is_empty() || f.offset == 8))
        .map(|f| u32_value(&data[f.offset + shift..f.offset + shift + 4]))
        .filter(|id| !matches!(id, Ok(0 | u32::MAX)))
        .collect()
}

fn parse_card(
    fields: &[ParsedSubrecord],
    masters: &[String],
    plugin: &str,
) -> Result<Value, PhaseError> {
    let editor_id = field(fields, "EDID").map(text).unwrap_or_default();
    let data = field(fields, "DATA").ok_or_else(|| error("card has no DATA"))?;
    let supported_layout = data.len() == 7 || (data.len() == 8 && data[7] == 0);
    if !supported_layout || data[5] > 6 {
        return Err(error(format!(
            "unsupported card DATA layout for {editor_id}: {} bytes ({})",
            data.len(),
            hex::encode(data)
        )));
    }
    let mut ranks = Vec::new();
    let mut rank = None;
    for entry in fields {
        match entry.signature.as_str() {
            "PRKE" => {
                if rank.is_some() {
                    return Err(error("nested card rank"));
                }
                rank = Some(json!({}));
            }
            "DATA" if rank.is_some() => {
                if entry.data.len() != 1 || !(1..=15).contains(&entry.data[0]) {
                    return Err(error("invalid card rank cost"));
                }
                rank.as_mut().unwrap()["cost"] = json!(entry.data[0]);
            }
            "MNAM" if rank.is_some() => {
                rank.as_mut().unwrap()["source_perk"] =
                    json!(key(u32_value(&entry.data)?, masters, plugin)?);
            }
            "PRKF" => {
                let current = rank
                    .take()
                    .ok_or_else(|| error("card rank end without start"))?;
                if current.get("cost").is_none() || current.get("source_perk").is_none() {
                    return Err(error("incomplete card rank"));
                }
                ranks.push(current);
            }
            _ => {}
        }
    }
    if rank.is_some() || ranks.is_empty() {
        return Err(error("incomplete card rank list"));
    }
    let reference = |sig| -> Result<Value, PhaseError> {
        field(fields, sig)
            .map(|v| Ok(json!(key(u32_value(v)?, masters, plugin)?)))
            .unwrap_or(Ok(Value::Null))
    };
    Ok(json!({"editor_id": editor_id,
        "special": data[5], "min_level": data[4], "race_restriction": data[6],
        "art": field(fields, "MNAM").map(text).unwrap_or_default(),
        "rarity_global": reference("PCDV")?, "sound": reference("SNAM")?, "ranks": ranks}))
}

impl Phase for PerkCardsPhase {
    fn name(&self) -> &'static str {
        "emit_perk_cards"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        if ctx.mod_path.as_os_str().is_empty() {
            return Err(PhaseError::BadParams("perk cards require mod_path".into()));
        }
        let schema = compiled_schema_for_game_str("fo76").map_err(error)?;
        let source_schema = AuthoringSchema::for_game("fo76").map_err(error)?;
        let target_schema = AuthoringSchema::for_game("fo4").map_err(error)?;
        let mut session = open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
            .map_err(|e| error(e.to_string()))?;
        let (masters, plugin) = session
            .handle_load_order(ctx.run.source_handle_id)
            .map(|(m, p)| (m.to_vec(), p.to_owned()))
            .map_err(|e| error(e.to_string()))?;
        let scan = session
            .handle_raw_scan(ctx.run.source_handle_id)
            .map_err(|e| error(e.to_string()))?;
        let mut candidates = BTreeMap::new();
        let mut roots = BTreeMap::new();
        for signature in EFFECT_TYPES.iter().copied().chain(["PCRD"]) {
            for raw in scan.raw_form_ids_of_sig(SigCode::from_str(signature).unwrap()) {
                let eid = scan
                    .with_record(raw, |r| {
                        (r.flags & 0x20 == 0).then(|| {
                            field(&effective_subrecords_for_record(r), "EDID")
                                .map(text)
                                .unwrap_or_default()
                        })
                    })
                    .flatten();
                if let Some(eid) = eid {
                    if matches!(
                        eid.as_str(),
                        "LevelUp_SelectionList"
                            | "76CharGenStartingPerkCardsFormlist"
                            | "PerkCardPack_Pack001"
                            | "PerkScrapCurrency"
                    ) {
                        if roots.insert(eid.clone(), raw).is_some() {
                            return Err(error("duplicate perk catalog root"));
                        }
                    }
                    candidates.insert(raw, (signature, eid));
                }
            }
        }
        let root = |name: &str| {
            roots
                .get(name)
                .copied()
                .ok_or_else(|| error(format!("missing perk catalog root {name}")))
        };
        let selection = root("LevelUp_SelectionList")?;
        let selected = scan
            .with_record(selection, |r| {
                leveled_cards(&effective_subrecords_for_record(r))
            })
            .ok_or_else(|| error("missing level-up selection list"))??;
        let starting = root("76CharGenStartingPerkCardsFormlist")?;
        let starting_ids = scan
            .with_record(starting, |r| {
                effective_subrecords_for_record(r)
                    .iter()
                    .filter(|f| f.signature == "LNAM")
                    .map(|f| u32_value(&f.data))
                    .collect::<Result<Vec<_>, _>>()
            })
            .ok_or_else(|| error("missing starting deck"))??;
        let pack_root = root("PerkCardPack_Pack001")?;
        let pack_fields = scan
            .with_record(pack_root, |r| effective_subrecords_for_record(r).to_vec())
            .ok_or_else(|| error("missing perk pack root"))?;
        let pack_reference = |signature: &str| -> Result<u32, PhaseError> {
            field(&pack_fields, signature)
                .ok_or_else(|| error(format!("perk pack has no {signature}")))
                .and_then(u32_value)
        };
        let roll_list = pack_reference("PPRL")?;
        let foil_list = pack_reference("PPFC")?;
        let rarity_list = pack_reference("PPCL")?;
        let level_offsets = pack_reference("PPLO")?;
        let list_entries = |raw, description: &str| -> Result<Vec<u32>, PhaseError> {
            scan.with_record(raw, |r| form_list(&effective_subrecords_for_record(r)))
                .ok_or_else(|| error(format!("missing perk pack {description} list")))?
        };
        let roll_curves = list_entries(roll_list, "rarity curve")?;
        let foil_curves = list_entries(foil_list, "foil curve")?;
        let rarity_tables = list_entries(rarity_list, "rarity table")?;
        if roll_curves.len() != 4 || foil_curves.len() != 4 || rarity_tables.len() != 3 {
            return Err(error(
                "perk pack must define four slots and three rarity tables",
            ));
        }
        let mut cards = BTreeMap::new();
        let mut pending: BTreeSet<u32> = roots.values().copied().collect();
        let mut excluded = Vec::new();
        for raw in selected {
            ctx.check_cancel()?;
            let Some((signature, eid)) = candidates.get(&raw) else {
                return Err(error(format!("missing selected card {raw:08X}")));
            };
            if *signature != "PCRD" {
                return Err(error("selection entry is not a card"));
            }
            let mut card = scan
                .with_record(raw, |r| {
                    parse_card(&effective_subrecords_for_record(r), &masters, &plugin)
                })
                .ok_or_else(|| error("missing selected card"))??;
            if eid.starts_with("GHL_") || card["race_restriction"].as_u64().unwrap() == 2 {
                excluded.push(json!({"key": key(raw, &masters, &plugin)?, "reason": "ghoul-only"}));
                continue;
            }
            let card_key = key(raw, &masters, &plugin)?;
            card["key"] = json!(card_key);
            let fields = scan
                .with_record(raw, |r| effective_subrecords_for_record(r).to_vec())
                .unwrap();
            for f in fields
                .iter()
                .filter(|f| matches!(f.signature.as_str(), "PCDV" | "SNAM"))
            {
                pending.insert(u32_value(&f.data)?);
            }
            for rank in card["ranks"].as_array().unwrap() {
                let source = rank["source_perk"].as_str().unwrap();
                let (id, owner) = source.split_once(':').unwrap();
                if owner != plugin {
                    return Err(error("card rank belongs to unsupported source master"));
                }
                pending.insert((masters.len() as u32) << 24 | u32::from_str_radix(id, 16).unwrap());
            }
            cards.insert(card_key, card);
        }
        let starting_deck = starting_ids
            .into_iter()
            .map(|raw| key(raw, &masters, &plugin))
            .collect::<Result<Vec<_>, _>>()?;
        if starting_deck.iter().any(|k| !cards.contains_key(k)) {
            return Err(error("starting deck includes ineligible card"));
        }
        let source_root = ctx
            .run
            .config
            .source_extracted_dir
            .as_deref()
            .unwrap_or(ctx.source_extracted_dir);
        let mut records = BTreeMap::new();
        let mut references = BTreeSet::new();
        let mut warnings = Vec::new();
        while let Some(raw) = pending.pop_first() {
            ctx.check_cancel()?;
            let source_key = key(raw, &masters, &plugin)?;
            if records.contains_key(&source_key) {
                continue;
            }
            let Some((signature, eid)) = candidates.get(&raw) else {
                continue;
            };
            if *signature == "PCRD" {
                continue;
            }
            let fields = scan
                .with_record(raw, |r| effective_subrecords_for_record(r).to_vec())
                .ok_or_else(|| error("missing effect record"))?;
            let mut saved = json!({"signature": signature, "editor_id": eid,
                "fields": fields.iter().map(|f| json!({"signature": f.signature.as_str(), "hex": hex::encode(&f.data)})).collect::<Vec<_>>()});
            let fk = FormKey {
                local: raw & 0xffffff,
                plugin: ctx
                    .run
                    .interner
                    .intern(source_key.split_once(':').unwrap().1),
            };
            let decoded = scan
                .record_decoded(raw, &fk, &source_schema, &ctx.run.interner)
                .ok_or_else(|| error("missing localized effect record"))?
                .map_err(|e| error(e.to_string()))?;
            for (sig, label) in [("FULL", "name"), ("DESC", "description")] {
                if let Some(f) = decoded.fields.iter().find(|f| f.sig.as_str() == sig) {
                    if let FieldValue::String(value) = f.value {
                        saved[label] = json!(ctx.run.interner.resolve(value).unwrap_or(""));
                    }
                }
            }
            let mut links = Vec::new();
            let mut occurrences = BTreeMap::new();
            let mut effect_type = None;
            let mut function_type = None;
            let spec = schema_record_spec(&schema, signature)
                .ok_or_else(|| error("missing effect schema"))?;
            for f in &fields {
                let occurrence = occurrences.entry(f.signature.clone()).or_insert(0);
                if *signature == "PERK" {
                    if f.signature == "PRKE" {
                        effect_type = f.data.first().copied();
                        function_type = None;
                    }
                    if f.signature == "PRKF" {
                        effect_type = None;
                    }
                    if f.signature == "EPFT" {
                        function_type = f.data.first().copied();
                    }
                }
                if *signature == "PERK" && f.signature == "DATA" && effect_type.is_some() {
                    if matches!(effect_type, Some(0 | 1)) && f.data.len() >= 4 {
                        links.push(u32_value(&f.data[..4])?);
                    }
                } else if *signature == "PERK" && f.signature == "EPF3" && function_type == Some(8)
                {
                    links.push(u32_value(&f.data)?);
                } else if f.signature == "CTDA" {
                    links.extend(fo76_condition_references(&f.data));
                } else if *signature == "MGEF" && f.signature == "DATA" {
                    links.extend(magic_effect_references(&f.data, &target_schema)?);
                } else if let Some(def) = schema_subrecord_spec(spec, &f.signature, *occurrence) {
                    esp_authoring_core::plugin_runtime::authoring::authoring_serialize::extract_nested_form_ids(def, &schema, &f.data, &mut links);
                }
                *occurrence += 1;
            }
            saved["references"] = json!(
                links
                    .iter()
                    .filter(|id| **id != 0)
                    .map(|id| key(*id, &masters, &plugin))
                    .collect::<Result<BTreeSet<_>, _>>()?
            );
            for link in links.into_iter().filter(|id| *id != 0) {
                references.insert(link);
                if candidates.contains_key(&link) {
                    pending.insert(link);
                }
            }
            if *signature == "CURV" {
                if let Some(relative) = field(&fields, "CRVE").or_else(|| field(&fields, "JASF")) {
                    let relative = text(relative);
                    let path = curve_path(source_root, &relative)?;
                    match std::fs::read(&path) {
                        Ok(data) => {
                            saved["curve"] = serde_json::from_slice(&data)
                                .map_err(|e| error(format!("{}: {e}", path.display())))?
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            warnings.push(format!("missing curve: {relative}"))
                        }
                        Err(e) => return Err(error(e.to_string())),
                    }
                } else {
                    warnings.push(format!("curve {source_key} has no source path"));
                }
            }
            references.insert(raw);
            records.insert(source_key, saved);
        }
        let mut rarity_rows = Vec::new();
        let mut assigned = BTreeSet::new();
        for (index, raw) in rarity_tables.iter().copied().enumerate() {
            let source_key = key(raw, &masters, &plugin)?;
            let fields = scan
                .with_record(raw, |r| effective_subrecords_for_record(r).to_vec())
                .ok_or_else(|| error(format!("missing perk pack rarity table {source_key}")))?;
            let mut eligible = Vec::new();
            let mut excluded_cards = Vec::new();
            for card_raw in form_list(&fields)? {
                let card_key = key(card_raw, &masters, &plugin)?;
                if cards.contains_key(&card_key) {
                    if !assigned.insert(card_key.clone()) {
                        return Err(error(format!(
                            "perk card {card_key} appears in more than one rarity table"
                        )));
                    }
                    eligible.push(card_key);
                } else {
                    excluded_cards.push(card_key);
                }
            }
            if eligible.is_empty() {
                return Err(error(format!(
                    "perk pack rarity table {source_key} has no eligible cards"
                )));
            }
            rarity_rows.push(json!({"index": index, "source_key": source_key,
                "eligible_cards": eligible, "excluded_cards": excluded_cards}));
        }
        if assigned.len() != cards.len() {
            return Err(error(format!(
                "{} eligible perk cards are absent from the source rarity tables",
                cards.len() - assigned.len()
            )));
        }
        let mut slot_rows = Vec::new();
        for index in 0..4 {
            let (rarity_curve, rarity_key) =
                pack_curve(&records, roll_curves[index], &masters, &plugin)?;
            let (foil_curve, foil_key) =
                pack_curve(&records, foil_curves[index], &masters, &plugin)?;
            slot_rows.push(json!({"index": index, "rarity_curve": rarity_key,
                "rarity_thresholds": curve_values_at(rarity_curve, &[0, 1, 2])?,
                "foil_curve": foil_key, "foil_chances": curve_values_at(foil_curve, &[0, 1, 2])?}));
        }
        let (level_curve, level_curve_key) =
            pack_curve(&records, level_offsets, &masters, &plugin)?;
        let level_rows = level_curve
            .get("curve")
            .and_then(|curve| curve.get("curve"))
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| error("perk pack level-offset curve has no points"))?;
        let pack = json!({"source_key": key(pack_root, &masters, &plugin)?, "cards_per_pack": 4,
            "rarities": rarity_rows, "slots": slot_rows, "level_offset_curve": level_curve_key,
            "level_offsets": level_rows});
        drop(scan);
        let mut targets = BTreeMap::new();
        for raw in references {
            let source_key = key(raw, &masters, &plugin)?;
            let source = FormKey {
                local: raw & 0xffffff,
                plugin: ctx
                    .run
                    .interner
                    .intern(source_key.split_once(':').unwrap().1),
            };
            let mut binding = Value::Null;
            if let Some(target) = ctx
                .run
                .mapper_state
                .as_ref()
                .and_then(|map| map.source_to_target.get(&source))
            {
                let owner = ctx.run.interner.resolve(target.plugin).unwrap_or("");
                let target_key = format!("{owner}:{:06X}", target.local);
                for handle in std::iter::once(ctx.run.target_handle_id)
                    .chain(ctx.run.master_handle_ids.iter().copied())
                {
                    let scan = session
                        .handle_raw_scan(handle)
                        .map_err(|e| error(e.to_string()))?;
                    if let Some((_, signature, _)) = scan.record_metadata_by_form_key(&target_key) {
                        binding = json!({"plugin": owner, "object_id": format!("{:06X}", target.local), "signature": signature});
                        break;
                    }
                }
            }
            targets.insert(source_key, binding);
        }
        let mut missing_ranks = 0;
        for card in cards.values_mut() {
            for rank in card["ranks"].as_array_mut().unwrap() {
                let source_key = rank["source_perk"].as_str().unwrap().to_owned();
                let source = records
                    .get(&source_key)
                    .ok_or_else(|| error("card rank PERK missing from source closure"))?;
                rank["description"] = source.get("description").cloned().unwrap_or(json!(""));
                rank["name"] = source.get("name").cloned().unwrap_or(json!(""));
                rank["perk"] = targets.get(&source_key).cloned().unwrap_or(Value::Null);
                if rank["perk"]["signature"] != "PERK"
                    || rank["perk"]["plugin"] != ctx.run.config.output_plugin_name
                {
                    missing_ranks += 1;
                    rank["status"] = json!("missing-converted-perk");
                } else {
                    rank["status"] = json!("effect-review-required");
                }
            }
            card["name"] = card["ranks"][0]["name"].clone();
        }
        if ctx.run.mapper_state.is_some() && missing_ranks != 0 {
            return Err(error(format!(
                "perk catalog has {missing_ranks} missing or aliased rank PERKs"
            )));
        }
        let catalog = json!({"schema_version": 1, "source_plugin": plugin,
            "output_plugin": ctx.run.config.output_plugin_name, "mapping_available": ctx.run.mapper_state.is_some(),
            "cards": cards.into_values().collect::<Vec<_>>(), "starting_deck": starting_deck,
            "pack": pack,
            "roots": roots.into_iter().map(|(name, raw)| Ok((name, key(raw, &masters, &plugin)?))).collect::<Result<BTreeMap<_, _>, PhaseError>>()?,
            "excluded": excluded, "effect_records": records, "target_forms": targets,
            "missing_rank_bindings": missing_ranks, "warnings": warnings,
            "economy": {"pack_levels": [4, 6, 8, 10], "pack_interval_after_10": 5, "scrap_coins_per_unit": 2,
                "allocation_cap": 56, "special_cap": 15, "starting_level": 1}});
        let directory = ctx.mod_path.join("F4SE/Plugins/B21_TalesFromAppalachia");
        std::fs::create_dir_all(&directory).map_err(|e| error(e.to_string()))?;
        let mut output =
            tempfile::NamedTempFile::new_in(&directory).map_err(|e| error(e.to_string()))?;
        output
            .write_all(&serde_json::to_vec_pretty(&catalog).map_err(|e| error(e.to_string()))?)
            .map_err(|e| error(e.to_string()))?;
        output
            .persist(directory.join("PerkCards.json"))
            .map_err(|e| error(e.to_string()))?;
        Ok(PhaseReport {
            assets_written: 1,
            warnings: missing_ranks + warnings.len() as u32,
            ..PhaseReport::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sub(sig: &str, data: &[u8]) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: sig.into(),
            data: data.to_vec().into(),
            semantic_type: None,
        }
    }
    #[test]
    fn rank_cost_and_art_use_scope_instead_of_subrecord_occurrence() {
        let fields = vec![
            sub("DATA", &[0, 0, 0, 0, 20, 4, 0]),
            sub("MNAM", b"ExampleArt\0"),
            sub("PRKE", &[]),
            sub("DATA", &[2]),
            sub("MNAM", &[1, 8, 0, 1]),
            sub("PRKF", &[]),
        ];
        let card = parse_card(&fields, &["Base.esm".into()], "Cards.esm").unwrap();
        assert_eq!(card["art"], "ExampleArt");
        assert_eq!(card["min_level"], 20);
        assert_eq!(card["ranks"][0]["cost"], 2);
        assert_eq!(card["ranks"][0]["source_perk"], "000801:Cards.esm");
        assert!(parse_card(&fields[..5], &["Base.esm".into()], "Cards.esm").is_err());

        let mut extended = fields.clone();
        extended[0] = sub("DATA", &[0, 0, 0, 0, 20, 4, 0, 0]);
        assert_eq!(
            parse_card(&extended, &["Base.esm".into()], "Cards.esm").unwrap()["min_level"],
            20
        );
        extended[0] = sub("DATA", &[0, 0, 0, 0, 20, 4, 0, 1]);
        assert!(parse_card(&extended, &["Base.esm".into()], "Cards.esm").is_err());
    }
    #[test]
    fn selection_supports_both_source_lvpc_layouts() {
        assert_eq!(
            leveled_cards(&[
                sub("LVLO", &[1, 8, 0, 0]),
                sub("LVLO", &[1, 0, 0, 0, 2, 8, 0, 0, 1, 0, 0, 0])
            ])
            .unwrap(),
            BTreeSet::from([0x801, 0x802])
        );
        assert!(leveled_cards(&[sub("LVLO", &[1, 8])]).is_err());
        assert!(curve_path(Path::new("source"), "../outside.json").is_err());
    }
    #[test]
    fn magic_effect_closure_includes_actor_values_and_applied_perks() {
        let schema = AuthoringSchema::for_game("fo4").unwrap();
        for (size, shift) in [(152, 0), (156, 0), (160, 4)] {
            let mut data = vec![0; size];
            data[16 + shift..20 + shift].copy_from_slice(&u32::MAX.to_le_bytes());
            data[68 + shift..72 + shift].copy_from_slice(&0x801u32.to_le_bytes());
            data[136 + shift..140 + shift].copy_from_slice(&0x802u32.to_le_bytes());
            assert_eq!(
                magic_effect_references(&data, &schema).unwrap(),
                vec![0x801, 0x802]
            );
        }
        assert!(magic_effect_references(&[0; 148], &schema).is_err());
    }
    #[test]
    fn pack_curve_points_are_read_by_source_rarity_index() {
        let record = json!({"curve": {"curve": [
            {"x": 0, "y": 0.05}, {"x": 1, "y": 0.2}, {"x": 2, "y": 1.0}
        ]}});
        assert_eq!(
            curve_values_at(&record, &[0, 1, 2]).unwrap(),
            vec![0.05, 0.2, 1.0]
        );
        assert!(curve_values_at(&record, &[3]).is_err());
    }
}
