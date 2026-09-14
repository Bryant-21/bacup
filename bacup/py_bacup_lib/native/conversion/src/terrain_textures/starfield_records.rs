use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, OnceLock};

use esp_authoring_core::plugin_runtime::{
    plugin_handle_read_authoring_record_value_json,
    plugin_handle_record_form_keys_by_signature_no_py, plugin_handle_resolve_form_id_no_py,
};
use serde_json::Value as JsonValue;

use crate::terrain_textures::ltex_walk::normalize_esp_form_key;

type BiomeGroundCoverMap = BTreeMap<String, BTreeSet<String>>;

static BIOME_GROUND_COVER_CACHE: OnceLock<Mutex<BTreeMap<u64, Arc<BiomeGroundCoverMap>>>> =
    OnceLock::new();

pub(crate) fn raw_hex_bytes(value: &JsonValue) -> Option<Vec<u8>> {
    let raw_hex = value.get("raw_hex")?.as_str()?;
    hex::decode(raw_hex).ok()
}

pub(crate) fn raw_u16_le(value: &JsonValue) -> Option<u16> {
    let bytes = raw_hex_bytes(value)?;
    let bytes: [u8; 2] = bytes.get(..2)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

pub(crate) fn raw_u32_le(value: &JsonValue) -> Option<u32> {
    let bytes = raw_hex_bytes(value)?;
    let bytes: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

pub(crate) fn resolve_raw_form_id(
    handle_id: u64,
    value: &JsonValue,
) -> Result<Option<String>, String> {
    let Some(raw_form_id) = raw_u32_le(value) else {
        return Ok(None);
    };
    if raw_form_id == 0 {
        return Ok(None);
    }
    let form_key = plugin_handle_resolve_form_id_no_py(handle_id, raw_form_id)?;
    if form_key.is_empty() {
        Ok(None)
    } else {
        Ok(Some(form_key))
    }
}

pub(crate) fn biome_ground_covers_for_ltexes(
    handle_id: u64,
    required_ltexes: &BTreeSet<String>,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let required_ltexes = required_ltexes
        .iter()
        .map(|form_key| normalize_esp_form_key(form_key).into_owned())
        .collect::<BTreeSet<_>>();
    let index = cached_biome_ground_cover_index(handle_id)?;
    Ok(required_ltexes
        .into_iter()
        .filter_map(|ltex_form_key| {
            index
                .get(&ltex_form_key)
                .cloned()
                .map(|ground_covers| (ltex_form_key, ground_covers))
        })
        .collect())
}

fn cached_biome_ground_cover_index(handle_id: u64) -> Result<Arc<BiomeGroundCoverMap>, String> {
    let mut cache = BIOME_GROUND_COVER_CACHE
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .map_err(|error| format!("Starfield BIOM cache lock poisoned: {error}"))?;
    if let Some(index) = cache.get(&handle_id) {
        return Ok(Arc::clone(index));
    }
    let index = Arc::new(build_biome_ground_cover_index(handle_id)?);
    cache.insert(handle_id, Arc::clone(&index));
    Ok(index)
}

fn build_biome_ground_cover_index(handle_id: u64) -> Result<BiomeGroundCoverMap, String> {
    let mut result = BiomeGroundCoverMap::new();

    for biom_form_key in plugin_handle_record_form_keys_by_signature_no_py(handle_id, "BIOM")? {
        let biom = plugin_handle_read_authoring_record_value_json(handle_id, &biom_form_key)
            .map_err(|error| format!("plugin lookup failed for BIOM {biom_form_key}: {error}"))?
            .ok_or_else(|| format!("BIOM not found: {biom_form_key}"))?;
        let fields = biom
            .get("fields")
            .and_then(JsonValue::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        for field in fields {
            let Some(object) = field.as_object() else {
                continue;
            };
            let Some(value) = object.get("LNAM").or_else(|| object.get("ANAM")) else {
                continue;
            };
            let Some((ltex_form_key, gcvr_form_key)) =
                biome_texture_ground_cover_link(handle_id, value)?
            else {
                continue;
            };
            let ltex_form_key = normalize_esp_form_key(&ltex_form_key).into_owned();
            result
                .entry(ltex_form_key)
                .or_default()
                .insert(normalize_esp_form_key(&gcvr_form_key).into_owned());
        }
    }

    Ok(result)
}

fn biome_texture_ground_cover_link(
    handle_id: u64,
    value: &JsonValue,
) -> Result<Option<(String, String)>, String> {
    if let Some(object) = value.as_object() {
        let ltex = object
            .get("LandTexture")
            .or_else(|| object.get("Land Texture"))
            .and_then(reference_form_key_from_value);
        let gcvr = object
            .get("GroundCover")
            .or_else(|| object.get("Ground Cover"))
            .and_then(reference_form_key_from_value);
        if let (Some(ltex), Some(gcvr)) = (ltex, gcvr) {
            return Ok(Some((ltex, gcvr)));
        }
    }

    let Some((ltex_raw, gcvr_raw)) = biome_raw_texture_ground_cover_link(value) else {
        return Ok(None);
    };
    let ltex = plugin_handle_resolve_form_id_no_py(handle_id, ltex_raw)?;
    let gcvr = plugin_handle_resolve_form_id_no_py(handle_id, gcvr_raw)?;
    if ltex.is_empty() || gcvr.is_empty() {
        return Ok(None);
    }
    Ok(Some((ltex, gcvr)))
}

fn biome_raw_texture_ground_cover_link(value: &JsonValue) -> Option<(u32, u32)> {
    let bytes = raw_hex_bytes(value)?;
    let ltex = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
    let gcvr = u32::from_le_bytes(bytes.get(8..12)?.try_into().ok()?);
    (ltex != 0 && gcvr != 0).then_some((ltex, gcvr))
}

fn reference_form_key_from_value(value: &JsonValue) -> Option<String> {
    let reference = value.get("reference")?;
    let plugin = reference.get("plugin")?.as_str()?.trim();
    let object_id = reference.get("object_id")?.as_str()?.trim();
    if plugin.is_empty() || object_id.is_empty() {
        return None;
    }
    Some(format!("{plugin}:{object_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn parses_starfield_biom_lnam_raw_link() {
        let value = json!({ "raw_hex": "0200000037EB280059BC0000" });

        assert_eq!(
            biome_raw_texture_ground_cover_link(&value),
            Some((0x28EB37, 0x00BC59))
        );
    }

    #[test]
    fn ignores_starfield_biom_lnam_without_ground_cover() {
        let value = json!({ "raw_hex": "0000000035EB280000000000" });

        assert_eq!(biome_raw_texture_ground_cover_link(&value), None);
    }

    #[test]
    fn parses_structured_reference_shape() {
        let value = json!({
            "LandTexture": {
                "reference": { "plugin": "Starfield.esm", "object_id": "28EB37" }
            },
            "GroundCover": {
                "reference": { "plugin": "Starfield.esm", "object_id": "00BC59" }
            }
        });

        let object = value.as_object().unwrap();
        assert_eq!(
            object
                .get("LandTexture")
                .and_then(reference_form_key_from_value),
            Some("Starfield.esm:28EB37".to_owned())
        );
        assert_eq!(
            object
                .get("GroundCover")
                .and_then(reference_form_key_from_value),
            Some("Starfield.esm:00BC59".to_owned())
        );
    }

    #[test]
    fn installed_starfield_master_resolves_biom_gcvr_gras_chain_when_available() {
        let Ok(data_dir) = std::env::var("STARFIELD_DATA_DIR") else {
            return;
        };
        let plugin_path = Path::new(&data_dir).join("Starfield.esm");
        if !plugin_path.is_file() {
            return;
        }
        let handle = crate::run::OwnedPluginHandle::load(&plugin_path, "starfield", None).unwrap();
        let required_ltexes = ["Starfield.esm:28EB37".to_owned()].into_iter().collect();

        let links = biome_ground_covers_for_ltexes(handle.id(), &required_ltexes).unwrap();
        assert!(
            links
                .get("Starfield.esm:28EB37")
                .is_some_and(|covers| covers.contains("Starfield.esm:00BC59"))
        );

        let grass = crate::terrain_textures::grass_walk::grass_entries_for_gcvr(
            handle.id(),
            "Starfield.esm:06953A",
            "starfield",
        )
        .unwrap();
        assert_eq!(grass.len(), 5);
        assert!(
            grass
                .iter()
                .all(|entry| !entry.model_file_name.is_empty() && entry.density > 0),
            "{grass:#?}"
        );
    }
}
