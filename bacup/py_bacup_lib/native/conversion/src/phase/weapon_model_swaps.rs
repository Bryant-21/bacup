//! FO76 weapon skins that change meshes rather than materials. A skin OMOD carries WEAP property
//! 103, a form value naming an MDSP record: ordered BNAM -> SNAM mesh path pairs. While the skin
//! is attached, every mesh the weapon loads whose path matches a BNAM loads the SNAM instead,
//! which covers the base weapon and each attachment (`DummyWeapon.nif` hides one). FO4 has no
//! MDSP record and no property 103, so the translator drops both and Tales applies the pairs at
//! runtime from this catalog.

use std::collections::BTreeMap;
use std::io::Write;

use esp_authoring_core::plugin_runtime::{ParsedSubrecord, effective_subrecords_for_record};
use serde_json::json;

use crate::ids::{FormKey, SigCode};
use crate::phase::{Phase, PhaseCtx, PhaseError, PhaseReport};
use crate::plugin_catalog;
use crate::session::open_session;
use crate::translator::Game;

pub struct WeaponModelSwapsPhase;

const MODEL_SWAP_PROPERTY: u16 = 103;
const WEAP_FORM_TYPE: u32 = u32::from_le_bytes(*b"WEAP");
const DATA_HEADER_LEN: usize = 20;
const DATA_FORM_TYPE_OFFSET: usize = 10;
const DATA_PROPERTY_COUNT_OFFSET: usize = 4;
const DATA_INCLUDE_ROW_LEN: usize = 7;
const DATA_ITEM_ROW_LEN: usize = 4;
const PROPERTY_ROW_LEN: usize = 24;
const PROPERTY_ID_OFFSET: usize = 8;
const PROPERTY_VALUE1_OFFSET: usize = 12;
const DELETED: u32 = 0x20;

fn error(message: impl Into<String>) -> PhaseError {
    PhaseError::Internal(message.into())
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// The MDSP form values of a WEAP OMOD's property 103 rows, as raw source FormIDs.
fn model_swap_forms(data: &[u8]) -> Vec<u32> {
    if u32_at(data, DATA_FORM_TYPE_OFFSET) != Some(WEAP_FORM_TYPE) {
        return Vec::new();
    }
    let (Some(includes), Some(properties), Some(slots)) = (
        u32_at(data, 0),
        u32_at(data, DATA_PROPERTY_COUNT_OFFSET),
        u32_at(data, DATA_HEADER_LEN),
    ) else {
        return Vec::new();
    };
    let start = DATA_HEADER_LEN
        + 4
        + slots as usize * 4
        + DATA_ITEM_ROW_LEN
        + includes as usize * DATA_INCLUDE_ROW_LEN;
    (0..properties as usize)
        .filter_map(|index| {
            let row = data
                .get(start + index * PROPERTY_ROW_LEN..)?
                .get(..PROPERTY_ROW_LEN)?;
            let id = u16::from_le_bytes([row[PROPERTY_ID_OFFSET], row[PROPERTY_ID_OFFSET + 1]]);
            (id == MODEL_SWAP_PROPERTY)
                .then(|| u32_at(row, PROPERTY_VALUE1_OFFSET))
                .flatten()
        })
        .filter(|&form| form != 0)
        .collect()
}

/// Lowercase, backslash-separated and relative to `Meshes\`, the form FO4 passes to the model DB.
fn mesh_path(bytes: &[u8]) -> String {
    let path = String::from_utf8_lossy(bytes)
        .trim_end_matches('\0')
        .trim()
        .replace('/', "\\")
        .to_ascii_lowercase();
    path.strip_prefix("meshes\\").unwrap_or(&path).to_owned()
}

fn model_pairs(fields: &[ParsedSubrecord]) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut original = None;
    for field in fields {
        match field.signature.as_str() {
            "BNAM" => original = Some(mesh_path(&field.data)),
            "SNAM" => {
                if let Some(original) = original.take() {
                    let replacement = mesh_path(&field.data);
                    if !original.is_empty() && !replacement.is_empty() && original != replacement {
                        pairs.push((original, replacement));
                    }
                }
            }
            _ => {}
        }
    }
    pairs
}

/// FO4 attachment meshes FO76 never shipped, each paired with the FO76 mesh of the same part. A
/// skin's MDSP lists only FO76 paths, so without these the FO4 part keeps its own mesh under the
/// skin: Nuka-World's covered Handmade barrels (FO76 kept only the uncovered ones) and the Shem
/// Drowne sword's glow, which every Revolutionary Sword skin must hide with the rest of the sword.
/// `mag75` is the one FO76 does ship, but the Screaming Eagle skins list only its `_1` mesh.
const FO4_MESH_ALIASES: &[(&str, &str)] = &[
    (
        "dlc04\\weapons\\raiderguns\\barrellight.nif",
        "dlc04\\weapons\\raiderguns\\barrellightnoguard.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrellight_1.nif",
        "dlc04\\weapons\\raiderguns\\barrellightnoguard_1.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrellightshort.nif",
        "dlc04\\weapons\\raiderguns\\barrellightshortnoguard.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrellightshort_1.nif",
        "dlc04\\weapons\\raiderguns\\barrellightshortnoguard_1.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrelsniper.nif",
        "dlc04\\weapons\\raiderguns\\barrelsniperlight.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrelsniper_1.nif",
        "dlc04\\weapons\\raiderguns\\barrelsniperlight_1.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrelsnipershortlight.nif",
        "dlc04\\weapons\\raiderguns\\barrelsnipershort.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrelsnipershortlight_1.nif",
        "dlc04\\weapons\\raiderguns\\barrelsnipershort_1.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrelwelded.nif",
        "dlc04\\weapons\\raiderguns\\barrelweldedlight.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\barrelwelded_1.nif",
        "dlc04\\weapons\\raiderguns\\barrelweldedlight_1.nif",
    ),
    (
        "dlc04\\weapons\\raiderguns\\mag75.nif",
        "dlc04\\weapons\\raiderguns\\mag75_1.nif",
    ),
    (
        "weapons\\swords\\shemdroweswordfx.nif",
        "weapons\\swords\\revstunpack.nif",
    ),
];

/// Gives each `FO4_MESH_ALIASES` part the swap of its FO76 counterpart, when the skin swaps that
/// counterpart and not the part itself.
fn add_fo4_mesh_aliases(pairs: &mut Vec<(String, String)>) {
    let mut covered: BTreeMap<String, String> = pairs.iter().cloned().collect();
    for (fo4, fo76) in FO4_MESH_ALIASES {
        if covered.contains_key(*fo4) {
            continue;
        }
        if let Some(replacement) = covered.get(*fo76).cloned() {
            covered.insert((*fo4).to_owned(), replacement.clone());
            pairs.push(((*fo4).to_owned(), replacement));
        }
    }
}

fn source_key(raw: u32, masters: &[String], plugin: &str) -> Option<(String, u32)> {
    let index = (raw >> 24) as usize;
    let owner = if index == masters.len() || index == 0xFF {
        plugin
    } else {
        masters.get(index)?.as_str()
    };
    Some((owner.to_owned(), raw & 0xFF_FFFF))
}

fn editor_id(fields: &[ParsedSubrecord]) -> String {
    fields
        .iter()
        .find(|field| field.signature == "EDID")
        .map(|field| {
            String::from_utf8_lossy(&field.data)
                .trim_end_matches('\0')
                .to_owned()
        })
        .unwrap_or_default()
}

impl Phase for WeaponModelSwapsPhase {
    fn name(&self) -> &'static str {
        "emit_weapon_model_swaps"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        if ctx.run.source != Game::Fo76 || ctx.run.target != Game::Fo4 {
            return Ok(PhaseReport::default());
        }
        if ctx.mod_path.as_os_str().is_empty() {
            return Err(PhaseError::BadParams(
                "weapon model swaps require mod_path".into(),
            ));
        }
        let mut session = open_session(ctx.run.target_handle_id, Some(ctx.run.source_handle_id))
            .map_err(|e| error(e.to_string()))?;
        let (masters, plugin) = session
            .handle_load_order(ctx.run.source_handle_id)
            .map(|(masters, plugin)| (masters.to_vec(), plugin.to_string()))
            .map_err(|e| error(e.to_string()))?;
        let scan = session
            .handle_raw_scan(ctx.run.source_handle_id)
            .map_err(|e| error(e.to_string()))?;
        let mut skins = BTreeMap::new();
        for raw in scan.raw_form_ids_of_sig(SigCode::from_str("OMOD").unwrap()) {
            ctx.check_cancel()?;
            let Some(Some((eid, forms))) = scan.with_record(raw, |record| {
                if record.flags & DELETED != 0 {
                    return None;
                }
                let fields = effective_subrecords_for_record(record);
                let forms = fields
                    .iter()
                    .find(|field| field.signature == "DATA")
                    .map(|field| model_swap_forms(&field.data))
                    .unwrap_or_default();
                (!forms.is_empty()).then(|| (editor_id(&fields), forms))
            }) else {
                continue;
            };
            let mut pairs = Vec::new();
            for form in forms {
                if let Some(Some(found)) = scan.with_record(form, |record| {
                    (record.signature.as_str() == "MDSP")
                        .then(|| model_pairs(&effective_subrecords_for_record(record)))
                }) {
                    pairs.extend(found);
                }
            }
            if !pairs.is_empty() {
                add_fo4_mesh_aliases(&mut pairs);
                skins.insert(raw, (eid, pairs));
            }
        }
        drop(scan);

        let mut catalog = Vec::new();
        let mut report = PhaseReport::default();
        for (raw, (eid, pairs)) in skins {
            let Some((owner, local)) = source_key(raw, &masters, &plugin) else {
                report.warnings += 1;
                continue;
            };
            let source = FormKey {
                local,
                plugin: ctx.run.interner.intern(&owner),
            };
            let Some(target) = ctx
                .run
                .mapper_state
                .as_ref()
                .and_then(|state| state.source_to_target.get(&source))
                .copied()
            else {
                report.warnings += 1;
                continue;
            };
            let target_plugin = ctx
                .run
                .interner
                .resolve(target.plugin)
                .unwrap_or("")
                .to_owned();
            let key = format!("{target_plugin}:{:06X}", target.local);
            let mut exists = false;
            for handle in std::iter::once(ctx.run.target_handle_id)
                .chain(ctx.run.master_handle_ids.iter().copied())
            {
                let scan = session
                    .handle_raw_scan(handle)
                    .map_err(|e| error(e.to_string()))?;
                if scan.record_metadata_by_form_key(&key).is_some() {
                    exists = true;
                    break;
                }
            }
            if !exists {
                report.warnings += 1;
                continue;
            }
            catalog.push(json!({
                "form": plugin_catalog::form_key(&target_plugin, target.local),
                "editor_id": eid,
                "models": pairs,
            }));
        }
        drop(session);

        let document = json!({
            "version": 1,
            "settings": {"source_plugin": plugin},
            "skins": catalog,
        });
        let output_plugin = std::path::Path::new(&ctx.run.config.output_plugin_name)
            .file_name()
            .ok_or_else(|| error("missing output plugin filename"))?
            .to_string_lossy()
            .into_owned();
        let destination = ctx
            .mod_path
            .join(plugin_catalog::catalog_path("WeaponModelSwaps", &output_plugin));
        let directory = destination.parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&directory).map_err(|e| error(e.to_string()))?;
        let data = serde_json::to_vec(&document).map_err(|e| error(e.to_string()))?;
        let mut pending =
            tempfile::NamedTempFile::new_in(&directory).map_err(|e| error(e.to_string()))?;
        pending.write_all(&data).map_err(|e| error(e.to_string()))?;
        pending
            .persist(destination)
            .map_err(|e| error(e.to_string()))?;
        report.assets_written = 1;
        report.records_changed =
            u32::try_from(document["skins"].as_array().map_or(0, Vec::len)).unwrap_or(u32::MAX);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn property(id: u16, value1: u32) -> Vec<u8> {
        let mut row = vec![0u8; PROPERTY_ROW_LEN];
        row[0] = 4;
        row[PROPERTY_ID_OFFSET..PROPERTY_ID_OFFSET + 2].copy_from_slice(&id.to_le_bytes());
        row[PROPERTY_VALUE1_OFFSET..PROPERTY_VALUE1_OFFSET + 4]
            .copy_from_slice(&value1.to_le_bytes());
        row
    }

    fn omod_data(form_type: &[u8; 4], slots: u32, includes: u32, rows: &[Vec<u8>]) -> Vec<u8> {
        let mut data = vec![0u8; DATA_HEADER_LEN];
        data[0..4].copy_from_slice(&includes.to_le_bytes());
        data[4..8].copy_from_slice(&(rows.len() as u32).to_le_bytes());
        data[DATA_FORM_TYPE_OFFSET..DATA_FORM_TYPE_OFFSET + 4].copy_from_slice(form_type);
        data.extend_from_slice(&slots.to_le_bytes());
        data.extend(std::iter::repeat_n(0xAB, slots as usize * 4));
        data.extend_from_slice(&[0; DATA_ITEM_ROW_LEN]);
        data.extend(std::iter::repeat_n(
            0xCD,
            includes as usize * DATA_INCLUDE_ROW_LEN,
        ));
        for row in rows {
            data.extend_from_slice(row);
        }
        data
    }

    fn subrecord(signature: &str, text: &str) -> ParsedSubrecord {
        let mut data = text.as_bytes().to_vec();
        data.push(0);
        ParsedSubrecord {
            signature: signature.into(),
            data: data.into(),
            semantic_type: None,
        }
    }

    #[test]
    fn finds_the_model_swap_form_past_slots_and_includes() {
        // ATX_mod_BaseballBat_Weapon_Paint_FireHydrantBat: property 103 -> MDSP 6C50CC, then a
        // keyword add.
        let data = omod_data(
            b"WEAP",
            2,
            1,
            &[property(103, 0x006C_50CC), property(31, 0x006C_50BF)],
        );
        assert_eq!(model_swap_forms(&data), vec![0x006C_50CC]);
    }

    #[test]
    fn ignores_other_form_types_and_truncated_rows() {
        let wallpaper = omod_data(b"MSTT", 0, 0, &[property(103, 0x0056_7367)]);
        assert!(model_swap_forms(&wallpaper).is_empty());
        let mut truncated = omod_data(b"WEAP", 0, 0, &[property(103, 0x0056_7367)]);
        truncated.truncate(truncated.len() - 1);
        assert!(model_swap_forms(&truncated).is_empty());
    }

    #[test]
    fn pairs_each_base_mesh_with_the_swap_that_follows_it() {
        let fields = [
            subrecord("BNAM", "Weapons\\BaseballBat\\BaseballBat.nif"),
            subrecord("SNAM", "ATX\\Weapons\\BaseballBat\\ATX_FireHydrantBat.nif"),
            subrecord("XALG", ""),
            subrecord("BNAM", "Meshes/Weapons/BaseballBat/Mod_Nails_1.nif"),
            subrecord("SNAM", "Weapons\\DummyWeapon.nif"),
            subrecord("BNAM", "weapons\\same.nif"),
            subrecord("SNAM", "Weapons\\Same.nif"),
            subrecord("SNAM", "weapons\\orphan.nif"),
        ];
        assert_eq!(
            model_pairs(&fields),
            vec![
                (
                    "weapons\\baseballbat\\baseballbat.nif".to_owned(),
                    "atx\\weapons\\baseballbat\\atx_firehydrantbat.nif".to_owned()
                ),
                (
                    "weapons\\baseballbat\\mod_nails_1.nif".to_owned(),
                    "weapons\\dummyweapon.nif".to_owned()
                ),
            ]
        );
    }

    fn pair(original: &str, replacement: &str) -> (String, String) {
        (original.to_owned(), replacement.to_owned())
    }

    #[test]
    fn fo4_only_handmade_barrels_take_their_fo76_counterparts_swap() {
        let barrel = "atx\\weapons\\screamingeagle\\atx_screamingeagle_barrel.nif";
        let mag = "atx\\weapons\\screamingeagle\\atx_screamingeagle_mag.nif";
        let mut pairs = vec![
            pair("dlc04\\weapons\\raiderguns\\barrellightnoguard.nif", barrel),
            pair(
                "dlc04\\weapons\\raiderguns\\barrellightnoguard_1.nif",
                barrel,
            ),
            pair("dlc04\\weapons\\raiderguns\\mag75_1.nif", mag),
        ];
        add_fo4_mesh_aliases(&mut pairs);
        let swaps: BTreeMap<_, _> = pairs.into_iter().collect();
        assert_eq!(swaps["dlc04\\weapons\\raiderguns\\barrellight.nif"], barrel);
        assert_eq!(
            swaps["dlc04\\weapons\\raiderguns\\barrellight_1.nif"],
            barrel
        );
        assert_eq!(swaps["dlc04\\weapons\\raiderguns\\mag75.nif"], mag);
        // The skin swaps no other barrel, so their FO4 counterparts stay unswapped.
        assert!(!swaps.contains_key("dlc04\\weapons\\raiderguns\\barrelwelded.nif"));
    }

    #[test]
    fn an_alias_never_replaces_a_pair_the_skin_already_has() {
        let mut pairs = vec![
            pair("weapons\\swords\\revstunpack.nif", "atx\\sword.nif"),
            pair(
                "weapons\\swords\\shemdroweswordfx.nif",
                "weapons\\dummyweapon.nif",
            ),
        ];
        add_fo4_mesh_aliases(&mut pairs);
        assert_eq!(pairs.len(), 2);
        assert_eq!(
            pairs[1],
            pair(
                "weapons\\swords\\shemdroweswordfx.nif",
                "weapons\\dummyweapon.nif"
            )
        );
    }

    #[test]
    fn source_keys_follow_master_ownership() {
        let masters = vec!["Base.esm".to_owned()];
        assert_eq!(
            source_key(0x0100_0800, &masters, "Skins.esm"),
            Some(("Skins.esm".to_owned(), 0x800))
        );
        assert_eq!(
            source_key(0x0000_0800, &masters, "Skins.esm"),
            Some(("Base.esm".to_owned(), 0x800))
        );
        assert_eq!(source_key(0x0200_0800, &masters, "Skins.esm"), None);
    }
}
