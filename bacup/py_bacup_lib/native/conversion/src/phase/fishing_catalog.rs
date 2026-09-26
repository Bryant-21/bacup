use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

use esp_authoring_core::plugin_runtime::{
    ParsedSubrecord, compiled_schema_for_game_str, effective_subrecords_for_record,
    schema_record_spec, schema_subrecord_spec,
};
use serde_json::{Value, json};

use crate::fixups::rewrite_raw_object_template_formids::fo76_condition_references;
use crate::ids::{FormKey, SigCode};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::session::open_session;
use crate::translator::Game;

pub struct FishingCatalogPhase;

const CATALOG_TYPES: &[&str] = &[
    "FISH", "DFOB", "LVLI", "CNDF", "GLOB", "FLST", "OMOD", "CHAL", "GMRW", "COBJ", "BOOK", "AVIF",
    "KYWD", "WEAP", "FURN", "AACT", "IDLE", "GMST", "WTHR",
];

fn editor_id(fields: &[ParsedSubrecord]) -> String {
    fields
        .iter()
        .find(|field| field.signature == "EDID")
        .map(|field| {
            String::from_utf8_lossy(&field.data)
                .trim_end_matches('\0')
                .to_string()
        })
        .unwrap_or_default()
}

fn is_root(signature: &str, eid: &str) -> bool {
    signature == "FISH"
        || eid.to_ascii_lowercase().contains("fishing")
        || (signature == "GMST"
            && matches!(
                eid,
                "fFishOutOfStamindDashDistanceMultiplier"
                    | "fFishOutOfStamindDashDelayMultiplier"
                    | "fFishOutOfStaminaReelInSpeedMultiplier"
                    | "fMinigameStartDelaySeconds"
                    | "fJoystickDeadzone"
                    | "fMaxMouseJoystickRadius"
                    | "fFishCatchLerpTime"
            ))
}

fn ordered_fields(fields: &[ParsedSubrecord]) -> Value {
    // CTDA belongs to the preceding LVLO. Grouping subrecords by signature loses that boundary.
    Value::Array(
        fields
            .iter()
            .map(|field| {
                json!({
                    "signature": field.signature.as_str(),
                    "hex": hex::encode(&field.data),
                })
            })
            .collect(),
    )
}

fn source_key(raw: u32, masters: &[String], plugin: &str) -> Result<String, String> {
    let index = (raw >> 24) as usize;
    let owner = if index == masters.len() || index == 0xFF {
        plugin
    } else {
        masters
            .get(index)
            .map(String::as_str)
            .ok_or_else(|| format!("fishing source FormID {raw:08X} has invalid master index"))?
    };
    Ok(format!("{:06X}:{owner}", raw & 0xFFFFFF))
}

impl Phase for FishingCatalogPhase {
    fn name(&self) -> &'static str {
        "emit_fishing_catalog"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        if ctx.mod_path.as_os_str().is_empty() {
            return Err(PhaseError::BadParams(
                "fishing catalog requires mod_path".into(),
            ));
        }
        let schema = compiled_schema_for_game_str("fo76").map_err(PhaseError::Internal)?;
        let mut session = open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let (masters, plugin) = session
            .handle_load_order(ctx.run.source_handle_id)
            .map(|(masters, plugin)| (masters.to_vec(), plugin.to_string()))
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let scan = session
            .handle_raw_scan(ctx.run.source_handle_id)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let mut candidates = BTreeMap::new();
        let mut pending = BTreeSet::new();
        let mut fish_count = 0;
        for signature in CATALOG_TYPES {
            ctx.check_cancel()?;
            for raw in scan.raw_form_ids_of_sig(SigCode::from_str(signature).unwrap()) {
                let (eid, deleted) = scan
                    .with_record(raw, |record| {
                        (
                            editor_id(&effective_subrecords_for_record(record)),
                            record.flags & 0x20 != 0,
                        )
                    })
                    .ok_or_else(|| {
                        PhaseError::Internal(format!("missing fishing candidate {raw:08X}"))
                    })?;
                if deleted {
                    continue;
                }
                if *signature == "FISH" {
                    fish_count += 1;
                }
                if is_root(signature, &eid) {
                    pending.insert(raw);
                }
                candidates.insert(raw, (*signature, eid));
            }
        }
        let mut records = BTreeMap::new();
        let mut references = BTreeSet::new();
        let mut weather_scanned = false;
        loop {
            let Some(raw) = pending.pop_first() else {
                if weather_scanned {
                    break;
                }
                weather_scanned = true;
                // FO4 WTHR lacks the keyword component used by FO76 catch conditions.
                for (&raw, (signature, _)) in &candidates {
                    if *signature != "WTHR" {
                        continue;
                    }
                    let relevant = scan
                        .with_record(raw, |record| {
                            effective_subrecords_for_record(record)
                                .iter()
                                .filter(|field| field.signature == "KWDA")
                                .any(|field| {
                                    field.data.chunks_exact(4).any(|chunk| {
                                        references.contains(&u32::from_le_bytes(
                                            chunk.try_into().unwrap(),
                                        ))
                                    })
                                })
                        })
                        .unwrap_or(false);
                    if relevant {
                        pending.insert(raw);
                    }
                }
                continue;
            };
            if records.contains_key(&raw) {
                continue;
            }
            ctx.check_cancel()?;
            let (signature, eid) = &candidates[&raw];
            let (fields, links, flags, form_version) = scan
                .with_record(raw, |record| {
                    if let Some(error) = &record.parse_error {
                        return Err(PhaseError::Internal(format!(
                            "cannot preserve fishing record {raw:08X}: {error}"
                        )));
                    }
                    let fields = effective_subrecords_for_record(record);
                    let spec = schema_record_spec(&schema, signature).ok_or_else(|| {
                        PhaseError::Internal(format!("missing fishing schema for {signature}"))
                    })?;
                    let mut links = Vec::new();
                    let mut occurrences = BTreeMap::new();
                    for field in fields.iter() {
                        let count = occurrences.entry(field.signature.clone()).or_insert(0);
                        let mut references = Vec::new();
                        if field.signature == "CTDA" {
                            if field.data.len() != 32 {
                                return Err(PhaseError::Internal(format!(
                                    "unsupported fishing condition size in {raw:08X}: {}", field.data.len()
                                )));
                            }
                            references = fo76_condition_references(&field.data);
                        } else if let Some(field_spec) = schema_subrecord_spec(spec, field.signature.as_str(), *count) {
                            esp_authoring_core::plugin_runtime::authoring::authoring_serialize::extract_nested_form_ids(
                                field_spec, &schema, &field.data, &mut references,
                            );
                        }
                        *count += 1;
                        links.extend(references.into_iter().map(|reference| (field.signature.clone(), reference)));
                    }
                    Ok((
                        ordered_fields(&fields),
                        links,
                        record.flags,
                        record.form_version,
                    ))
                })
                .ok_or_else(|| {
                    PhaseError::Internal(format!("missing fishing record {raw:08X}"))
                })??;
            for (field, reference) in links {
                if reference == 0 {
                    continue;
                }
                source_key(reference, &masters, &plugin).map_err(|error| {
                    PhaseError::Internal(format!("{signature} {raw:08X} {eid} {field}: {error}"))
                })?;
                references.insert(reference);
                if candidates.contains_key(&reference) && !records.contains_key(&reference) {
                    pending.insert(reference);
                }
            }
            records.insert(
                raw,
                json!({
                    "source_form_id": format!("{raw:08X}"),
                    "form_key": source_key(raw, &masters, &plugin).map_err(PhaseError::Internal)?,
                    "signature": signature,
                    "editor_id": eid,
                    "flags": flags,
                    "form_version": form_version,
                    "fields": fields,
                }),
            );
        }
        references.extend(records.keys());
        drop(scan);
        let mut target_forms = BTreeMap::new();
        for raw in references {
            let key = source_key(raw, &masters, &plugin).map_err(PhaseError::Internal)?;
            let (_, owner) = key.split_once(':').unwrap();
            let source = FormKey {
                local: raw & 0xFFFFFF,
                plugin: ctx.run.interner.intern(owner),
            };
            let target = ctx
                .run
                .mapper_state
                .as_ref()
                .and_then(|state| state.source_to_target.get(&source));
            let resolved = if let Some(target) = target {
                let owner = ctx.run.interner.resolve(target.plugin).unwrap_or("");
                let target_key = format!("{owner}:{:06X}", target.local);
                let mut identity = None;
                for handle in std::iter::once(ctx.run.target_handle_id)
                    .chain(ctx.run.master_handle_ids.iter().copied())
                {
                    let scan = session
                        .handle_raw_scan(handle)
                        .map_err(|error| PhaseError::Internal(error.to_string()))?;
                    if let Some((raw, signature, _)) = scan.record_metadata_by_form_key(&target_key)
                    {
                        identity = scan.with_record(raw, |record| {
                            json!({
                                "plugin": owner,
                                "object_id": format!("{:06X}", target.local),
                                "signature": signature,
                                "editor_id": editor_id(&effective_subrecords_for_record(record)),
                            })
                        });
                        break;
                    }
                }
                identity
            } else {
                None
            };
            target_forms.insert(key, resolved);
        }
        drop(session);
        let output_plugin = &ctx.run.config.output_plugin_name;
        let catalog = json!({
            "schema_version": 1,
            "source_plugin": plugin,
            "source_masters": masters,
            "output_plugin": output_plugin,
            "fish_count": fish_count,
            "mapping_available": ctx.run.mapper_state.is_some(),
            "weather_keywords_preserved": true,
            "records": records.into_values().collect::<Vec<_>>(),
            "target_forms": target_forms,
        });
        let directory = ctx
            .mod_path
            .join("F4SE/Plugins/B21_TalesFromAppalachia/Fishing");
        let filename = std::path::Path::new(output_plugin)
            .file_name()
            .ok_or_else(|| PhaseError::BadParams("missing output plugin filename".into()))?;
        let destination = directory.join(format!("{}.json", filename.to_string_lossy()));
        let data = serde_json::to_vec(&catalog)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        std::fs::create_dir_all(&directory)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        let mut pending_file = tempfile::NamedTempFile::new_in(&directory)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        pending_file
            .write_all(&data)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        pending_file
            .persist(destination)
            .map_err(|error| PhaseError::Internal(error.to_string()))?;
        Ok(PhaseReport {
            assets_written: 1,
            ..PhaseReport::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_keys_preserve_master_ownership() {
        let masters = vec!["Base.esm".into()];
        assert_eq!(
            source_key(0x00000800, &masters, "Fish.esm").unwrap(),
            "000800:Base.esm"
        );
        assert_eq!(
            source_key(0x01000800, &masters, "Fish.esm").unwrap(),
            "000800:Fish.esm"
        );
        assert_eq!(
            source_key(0xFF000800, &masters, "Fish.esm").unwrap(),
            "000800:Fish.esm"
        );
        assert!(source_key(0x02000800, &masters, "Fish.esm").is_err());
    }

    #[test]
    fn fish_without_fishing_prefix_is_a_root() {
        assert!(is_root("FISH", "Burn_Fish_LocalLegend_Deathjaw"));
        assert!(is_root("DFOB", "ActionFishingStartCasting_DO"));
        assert!(!is_root("LVLI", "LL_Vendor_Weapons"));
        assert!(is_root("GMST", "fFishOutOfStamindDashDistanceMultiplier"));
        assert!(is_root("GMST", "fJoystickDeadzone"));
        assert!(!is_root("GMST", "fJumpHeightMin"));
    }

    #[test]
    fn repeated_conditions_keep_their_entry_boundaries_and_bytes() {
        let fields: Vec<_> = [
            ("LVLO", vec![1, 2, 3, 4]),
            ("CTDA", vec![0xA4; 32]),
            ("CTDA", vec![1; 32]),
            ("LVLO", vec![5, 6, 7, 8]),
            ("CTDA", vec![0; 32]),
        ]
        .into_iter()
        .map(|(signature, data)| ParsedSubrecord {
            signature: signature.into(),
            data: data.into(),
            semantic_type: None,
        })
        .collect();
        let exported = ordered_fields(&fields);
        for (original, saved) in fields.iter().zip(exported.as_array().unwrap()) {
            assert_eq!(saved["signature"], original.signature.as_str());
            assert_eq!(
                hex::decode(saved["hex"].as_str().unwrap()).unwrap(),
                original.data.as_ref()
            );
        }
        assert_eq!(exported.as_array().unwrap().len(), 5);
    }
}
