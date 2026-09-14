use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

use super::creature_catalog::{LegacyCreatureGame, LegacyRecordSource, StableFormKey};

const LEGACY_FACE_PART_INDICES: [u32; 8] = [0, 1, 2, 3, 4, 5, 6, 7];

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyRaceAlternateTextureEvidence {
    pub node_name: String,
    pub texture_set: StableFormKey,
    pub node_index: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyRaceFaceAssetRow {
    pub index: u32,
    pub model: Option<String>,
    pub model_information: Option<Vec<u8>>,
    pub model_bound: Option<Vec<u8>>,
    pub alternate_textures: Vec<LegacyRaceAlternateTextureEvidence>,
    pub model_flags: Option<u8>,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyRaceSexFaceAssets {
    pub rows: Vec<LegacyRaceFaceAssetRow>,
    pub texture_basis_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyEgtTextureBasisHeader {
    pub rows: u32,
    pub columns: u32,
    pub symmetric_modes: u32,
    pub asymmetric_modes: u32,
    pub texture_basis_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyRaceFaceAssetEvidence {
    pub game: LegacyCreatureGame,
    pub source: StableFormKey,
    pub male: LegacyRaceSexFaceAssets,
    pub female: LegacyRaceSexFaceAssets,
    pub source_projection_blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyRaceFaceAssetError {
    pub source: StableFormKey,
    pub reason_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LegacyRaceFaceAssetPreparation {
    pub game: LegacyCreatureGame,
    pub source: StableFormKey,
    pub evidence: Result<LegacyRaceFaceAssetEvidence, LegacyRaceFaceAssetError>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyTargetAppearanceRecordReceipt {
    pub target: StableFormKey,
    pub signature: String,
    pub record_blake3: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct LegacyTargetAppearanceClosureEvidence {
    pub race_roots: Vec<StableFormKey>,
    pub records: Vec<LegacyTargetAppearanceRecordReceipt>,
    pub closure_blake3: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaceSection {
    Male,
    Female,
}

pub(crate) fn build_legacy_ancillary_race_face_assets(
    expected: &BTreeSet<(LegacyCreatureGame, StableFormKey)>,
    sources: &[LegacyRecordSource<'_>],
    interner: &StringInterner,
) -> Result<Vec<LegacyRaceFaceAssetPreparation>, String> {
    let mut records = BTreeMap::new();
    for source in sources {
        if source.record.sig.as_str() != "RACE" {
            return Err(format!(
                "ancillary NPC race input contains {} instead of RACE",
                source.record.sig.as_str()
            ));
        }
        let plugin = interner
            .resolve(source.record.form_key.plugin)
            .ok_or_else(|| "ancillary NPC race source plugin is not interned".to_string())?;
        let key = (
            source.provenance.game,
            StableFormKey {
                local: source.record.form_key.local,
                plugin: plugin.to_string(),
            },
        );
        if records.insert(key.clone(), source.record).is_some() {
            return Err(format!(
                "ancillary NPC race input duplicates {:?} {}",
                key.0, key.1
            ));
        }
    }
    let actual = records.keys().cloned().collect::<BTreeSet<_>>();
    if &actual != expected {
        let missing = expected.difference(&actual).cloned().collect::<Vec<_>>();
        let extra = actual.difference(expected).cloned().collect::<Vec<_>>();
        return Err(format!(
            "ancillary NPC race input is not the exact effective RNAM set: missing={missing:?} extra={extra:?}"
        ));
    }
    Ok(records
        .into_iter()
        .map(|((game, source), record)| LegacyRaceFaceAssetPreparation {
            game,
            source: source.clone(),
            evidence: decode_legacy_race_face_assets(game, &source, record, interner),
        })
        .collect())
}

pub(crate) fn validate_legacy_target_appearance_closure(
    expected_race_roots: &BTreeSet<StableFormKey>,
    records: &[Record],
    interner: &StringInterner,
) -> Result<LegacyTargetAppearanceClosureEvidence, String> {
    if expected_race_roots.is_empty() {
        return Err("ancillary target appearance RACE roots are empty".to_string());
    }
    let mut record_by_key = BTreeMap::new();
    for record in records {
        if !matches!(record.sig.as_str(), "RACE" | "HDPT" | "TXST" | "FLST") {
            return Err(format!(
                "ancillary target appearance closure contains unsupported {}",
                record.sig.as_str()
            ));
        }
        let key = stable_key(record.form_key, interner)
            .ok_or_else(|| "ancillary target appearance plugin is unresolved".to_string())?;
        if record_by_key.insert(key.clone(), record).is_some() {
            return Err(format!(
                "ancillary target appearance closure duplicates {key}"
            ));
        }
    }
    let actual_roots = record_by_key
        .iter()
        .filter_map(|(key, record)| (record.sig.as_str() == "RACE").then_some(key.clone()))
        .collect::<BTreeSet<_>>();
    if &actual_roots != expected_race_roots {
        return Err(format!(
            "ancillary target appearance roots are not exact: expected={expected_race_roots:?} actual={actual_roots:?}"
        ));
    }

    let mut edges = BTreeMap::<StableFormKey, Vec<StableFormKey>>::new();
    for (owner, record) in &record_by_key {
        let specifications: &[(&str, &str)] = match record.sig.as_str() {
            "RACE" => &[
                ("HEAD", "HDPT"),
                ("FTSM", "TXST"),
                ("DFTM", "TXST"),
                ("FTSF", "TXST"),
                ("DFTF", "TXST"),
                ("CNAM", "TXST"),
                ("NAM2", "TXST"),
            ],
            "HDPT" => &[("HNAM", "HDPT"), ("TNAM", "TXST"), ("RNAM", "FLST")],
            "TXST" | "FLST" => &[],
            _ => unreachable!("signature allow-list checked above"),
        };
        let mut targets = Vec::new();
        for (field_signature, target_signature) in specifications {
            for target in record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == *field_signature)
                .flat_map(|field| form_keys(&field.value))
            {
                if target.local == 0 {
                    continue;
                }
                let target = stable_key(target, interner).ok_or_else(|| {
                    format!(
                        "ancillary target appearance {owner} {field_signature} plugin is unresolved"
                    )
                })?;
                let target_record = record_by_key.get(&target).ok_or_else(|| {
                    format!(
                        "ancillary target appearance {owner} {field_signature} misses {target_signature} {target}"
                    )
                })?;
                if target_record.sig.as_str() != *target_signature {
                    return Err(format!(
                        "ancillary target appearance {owner} {field_signature} expected {target_signature}, got {} at {target}",
                        target_record.sig.as_str()
                    ));
                }
                targets.push(target);
            }
        }
        targets.sort();
        targets.dedup();
        edges.insert(owner.clone(), targets);
    }

    let mut visited = BTreeSet::new();
    let mut pending = expected_race_roots.iter().cloned().collect::<Vec<_>>();
    while let Some(key) = pending.pop() {
        if !visited.insert(key.clone()) {
            continue;
        }
        pending.extend(edges.get(&key).into_iter().flatten().cloned());
    }
    let actual = record_by_key.keys().cloned().collect::<BTreeSet<_>>();
    if visited != actual {
        return Err(format!(
            "ancillary target appearance closure has unreachable or missing records: visited={visited:?} actual={actual:?}"
        ));
    }

    let mut receipts = Vec::with_capacity(record_by_key.len());
    for (target, record) in record_by_key {
        let canonical = canonical_target_record(&target, record, interner)?;
        receipts.push(LegacyTargetAppearanceRecordReceipt {
            target,
            signature: record.sig.as_str().to_string(),
            record_blake3: blake3::hash(&canonical).to_hex().to_string(),
        });
    }
    let race_roots = expected_race_roots.iter().cloned().collect::<Vec<_>>();
    let canonical = serde_json::to_vec(&(&race_roots, &receipts))
        .map_err(|error| format!("serialize target appearance closure: {error}"))?;
    Ok(LegacyTargetAppearanceClosureEvidence {
        race_roots,
        records: receipts,
        closure_blake3: blake3::hash(&canonical).to_hex().to_string(),
    })
}

fn canonical_target_record(
    target: &StableFormKey,
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<u8>, String> {
    let fields = record
        .fields
        .iter()
        .map(|field| {
            Ok(serde_json::json!({
                "signature": field.sig.as_str(),
                "value": canonical_value(&field.value, interner)?,
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;
    serde_json::to_vec(&serde_json::json!({
        "target": target,
        "signature": record.sig.as_str(),
        "flags": record.flags.bits(),
        "fields": fields,
    }))
    .map_err(|error| format!("serialize target appearance record {target}: {error}"))
}

fn canonical_value(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<serde_json::Value, String> {
    Ok(match value {
        FieldValue::None => serde_json::json!({"none": true}),
        FieldValue::Bool(value) => serde_json::json!({"bool": value}),
        FieldValue::Int(value) => serde_json::json!({"int": value}),
        FieldValue::Uint(value) => serde_json::json!({"uint": value}),
        FieldValue::Float(value) => {
            serde_json::json!({"float_bits": format!("{:08x}", value.to_bits())})
        }
        FieldValue::String(value) => serde_json::json!({
            "string": interner.resolve(*value).ok_or_else(|| "target appearance string is unresolved".to_string())?,
        }),
        FieldValue::Bytes(value) => serde_json::json!({"bytes": hex::encode(value)}),
        FieldValue::FormKey(value) => serde_json::json!({
            "form_key": stable_key(*value, interner).ok_or_else(|| "target appearance FormKey plugin is unresolved".to_string())?,
        }),
        FieldValue::List(values) => serde_json::json!({
            "list": values.iter().map(|value| canonical_value(value, interner)).collect::<Result<Vec<_>, _>>()?,
        }),
        FieldValue::Struct(fields) => serde_json::json!({
            "struct": fields.iter().map(|(name, value)| {
                Ok(serde_json::json!({
                    "name": interner.resolve(*name).ok_or_else(|| "target appearance struct field name is unresolved".to_string())?,
                    "value": canonical_value(value, interner)?,
                }))
            }).collect::<Result<Vec<_>, String>>()?,
        }),
    })
}

fn form_keys(value: &FieldValue) -> Vec<crate::ids::FormKey> {
    match value {
        FieldValue::FormKey(value) => vec![*value],
        FieldValue::List(values) => values.iter().flat_map(form_keys).collect(),
        FieldValue::Struct(fields) => fields
            .iter()
            .flat_map(|(_, value)| form_keys(value))
            .collect(),
        _ => Vec::new(),
    }
}

fn stable_key(value: crate::ids::FormKey, interner: &StringInterner) -> Option<StableFormKey> {
    Some(StableFormKey {
        local: value.local,
        plugin: interner.resolve(value.plugin)?.to_string(),
    })
}

pub(crate) fn decode_legacy_race_face_assets(
    game: LegacyCreatureGame,
    source: &StableFormKey,
    record: &Record,
    interner: &StringInterner,
) -> Result<LegacyRaceFaceAssetEvidence, LegacyRaceFaceAssetError> {
    if record.sig.as_str() != "RACE" {
        return race_error(source, "legacy_race_appearance_wrong_signature");
    }

    let mut in_head_data = false;
    let mut saw_head_data = false;
    let mut section = None;
    let mut current = None;
    let mut male = Vec::new();
    let mut female = Vec::new();

    for field in &record.fields {
        let signature = field.sig.as_str();
        if !in_head_data {
            if signature == "NAM0" {
                require_marker(&field.value, source)?;
                if saw_head_data {
                    return race_error(source, "legacy_race_appearance_duplicate_head_marker");
                }
                saw_head_data = true;
                in_head_data = true;
            }
            continue;
        }
        if signature == "NAM1" {
            require_marker(&field.value, source)?;
            finish_row(source, section, &mut current, &mut male, &mut female)?;
            in_head_data = false;
            break;
        }

        match signature {
            "MNAM" => {
                require_marker(&field.value, source)?;
                finish_row(source, section, &mut current, &mut male, &mut female)?;
                if section.is_some() {
                    return race_error(source, "legacy_race_appearance_sex_marker_order");
                }
                section = Some(FaceSection::Male);
            }
            "FNAM" => {
                require_marker(&field.value, source)?;
                finish_row(source, section, &mut current, &mut male, &mut female)?;
                if section != Some(FaceSection::Male) {
                    return race_error(source, "legacy_race_appearance_sex_marker_order");
                }
                section = Some(FaceSection::Female);
            }
            "INDX" => {
                finish_row(source, section, &mut current, &mut male, &mut female)?;
                if section.is_none() {
                    return race_error(source, "legacy_race_appearance_row_before_sex_marker");
                }
                current = Some(LegacyRaceFaceAssetRow {
                    index: exact_u32(&field.value, source)?,
                    model: None,
                    model_information: None,
                    model_bound: None,
                    alternate_textures: Vec::new(),
                    model_flags: None,
                    icon: None,
                });
            }
            "MODL" => {
                set_once(
                    &mut required_row(source, &mut current)?.model,
                    exact_string(&field.value, source, interner)?,
                    source,
                )?;
            }
            "MODT" => {
                set_once(
                    &mut required_row(source, &mut current)?.model_information,
                    exact_bytes(&field.value, source)?,
                    source,
                )?;
            }
            "MODB" => {
                set_once(
                    &mut required_row(source, &mut current)?.model_bound,
                    exact_bytes(&field.value, source)?,
                    source,
                )?;
            }
            "MODS" => {
                let rows = decode_alternate_textures(&field.value, source, interner)?;
                let target = &mut required_row(source, &mut current)?.alternate_textures;
                if !target.is_empty() {
                    return race_error(source, "legacy_race_appearance_duplicate_row_field");
                }
                *target = rows;
            }
            "MODD" => {
                set_once(
                    &mut required_row(source, &mut current)?.model_flags,
                    exact_u8(&field.value, source)?,
                    source,
                )?;
            }
            "ICON" => {
                set_once(
                    &mut required_row(source, &mut current)?.icon,
                    exact_string(&field.value, source, interner)?,
                    source,
                )?;
            }
            _ => return race_error(source, "legacy_race_appearance_head_subrecord_shape"),
        }
    }

    if in_head_data {
        finish_row(source, section, &mut current, &mut male, &mut female)?;
    }
    if !saw_head_data || section != Some(FaceSection::Female) {
        return race_error(source, "legacy_race_appearance_head_sections_missing");
    }
    validate_rows(source, &male)?;
    validate_rows(source, &female)?;

    let (male_texture_basis, female_texture_basis) =
        decode_legacy_race_texture_basis_models(record, source, interner)?;
    let male = LegacyRaceSexFaceAssets {
        rows: male,
        texture_basis_model: male_texture_basis,
    };
    let female = LegacyRaceSexFaceAssets {
        rows: female,
        texture_basis_model: female_texture_basis,
    };
    let canonical = serde_json::to_vec(&(game, source, &male, &female)).map_err(|_| {
        LegacyRaceFaceAssetError {
            source: source.clone(),
            reason_code: "legacy_race_appearance_receipt_serialization".to_string(),
        }
    })?;
    Ok(LegacyRaceFaceAssetEvidence {
        game,
        source: source.clone(),
        male,
        female,
        source_projection_blake3: blake3::hash(&canonical).to_hex().to_string(),
    })
}

pub(crate) fn decode_legacy_egt_texture_basis_header(
    bytes: &[u8],
) -> Result<LegacyEgtTextureBasisHeader, String> {
    const HEADER_LEN: usize = 64;
    if bytes.len() < HEADER_LEN || &bytes[..8] != b"FREGT003" {
        return Err("legacy EGT has no FREGT003 header".to_string());
    }
    let read_u32 = |offset: usize| {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("header width"))
    };
    let header = LegacyEgtTextureBasisHeader {
        rows: read_u32(8),
        columns: read_u32(12),
        symmetric_modes: read_u32(16),
        asymmetric_modes: read_u32(20),
        texture_basis_version: read_u32(24),
    };
    if header.rows == 0 || header.columns == 0 {
        return Err("legacy EGT has an empty image basis".to_string());
    }
    if bytes[28..HEADER_LEN].iter().any(|byte| *byte != 0) {
        return Err("legacy EGT reserved header bytes are nonzero".to_string());
    }
    let pixels = u64::from(header.rows)
        .checked_mul(u64::from(header.columns))
        .ok_or_else(|| "legacy EGT dimensions overflow".to_string())?;
    let mode_len = 4_u64
        .checked_add(
            pixels
                .checked_mul(3)
                .ok_or_else(|| "legacy EGT image width overflows".to_string())?,
        )
        .ok_or_else(|| "legacy EGT mode width overflows".to_string())?;
    let modes = u64::from(header.symmetric_modes)
        .checked_add(u64::from(header.asymmetric_modes))
        .ok_or_else(|| "legacy EGT mode count overflows".to_string())?;
    let expected_len = 64_u64
        .checked_add(
            modes
                .checked_mul(mode_len)
                .ok_or_else(|| "legacy EGT payload width overflows".to_string())?,
        )
        .ok_or_else(|| "legacy EGT file width overflows".to_string())?;
    if bytes.len() as u64 != expected_len {
        return Err(format!(
            "legacy EGT byte length {} does not match header {expected_len}",
            bytes.len()
        ));
    }
    Ok(header)
}

fn decode_legacy_race_texture_basis_models(
    record: &Record,
    source: &StableFormKey,
    interner: &StringInterner,
) -> Result<(String, String), LegacyRaceFaceAssetError> {
    let mut after_head = false;
    let mut section = None;
    let mut index = None;
    let mut male = None;
    let mut female = None;
    for field in &record.fields {
        match field.sig.as_str() {
            "NAM1" => {
                after_head = true;
                section = None;
                index = None;
            }
            _ if !after_head => {}
            "MNAM" if male.is_none() => {
                section = Some(FaceSection::Male);
                index = None;
            }
            "FNAM" if male.is_some() && female.is_none() => {
                section = Some(FaceSection::Female);
                index = None;
            }
            "INDX" => index = Some(exact_u32(&field.value, source)?),
            "MODL" if index == Some(3) => {
                let path = exact_string(&field.value, source, interner)?;
                if !path.to_ascii_lowercase().ends_with(".egt") {
                    continue;
                }
                let target = match section {
                    Some(FaceSection::Male) => &mut male,
                    Some(FaceSection::Female) => &mut female,
                    None => {
                        return race_error(
                            source,
                            "legacy_race_appearance_texture_basis_before_sex_marker",
                        );
                    }
                };
                if target.replace(path).is_some() {
                    return race_error(source, "legacy_race_appearance_duplicate_texture_basis");
                }
                if male.is_some() && female.is_some() {
                    break;
                }
            }
            _ => {}
        }
    }
    match (male, female) {
        (Some(male), Some(female)) => Ok((male, female)),
        _ => race_error(source, "legacy_race_appearance_texture_basis_missing"),
    }
}

fn finish_row(
    source: &StableFormKey,
    section: Option<FaceSection>,
    current: &mut Option<LegacyRaceFaceAssetRow>,
    male: &mut Vec<LegacyRaceFaceAssetRow>,
    female: &mut Vec<LegacyRaceFaceAssetRow>,
) -> Result<(), LegacyRaceFaceAssetError> {
    let Some(row) = current.take() else {
        return Ok(());
    };
    match section {
        Some(FaceSection::Male) => male.push(row),
        Some(FaceSection::Female) => female.push(row),
        None => return race_error(source, "legacy_race_appearance_row_before_sex_marker"),
    }
    Ok(())
}

fn validate_rows(
    source: &StableFormKey,
    rows: &[LegacyRaceFaceAssetRow],
) -> Result<(), LegacyRaceFaceAssetError> {
    if rows.iter().map(|row| row.index).collect::<Vec<_>>() != LEGACY_FACE_PART_INDICES {
        return race_error(source, "legacy_race_appearance_face_part_indices");
    }
    if rows[0].model.is_none()
        || rows[2].model.is_none()
        || rows[3].model.is_none()
        || rows[4].model.is_none()
        || rows[5].model.is_none()
        || rows[6].model.is_none()
        || rows[7].model.is_none()
    {
        return race_error(source, "legacy_race_appearance_required_model_missing");
    }
    Ok(())
}

fn required_row<'a>(
    source: &StableFormKey,
    current: &'a mut Option<LegacyRaceFaceAssetRow>,
) -> Result<&'a mut LegacyRaceFaceAssetRow, LegacyRaceFaceAssetError> {
    current.as_mut().ok_or_else(|| LegacyRaceFaceAssetError {
        source: source.clone(),
        reason_code: "legacy_race_appearance_row_field_before_index".to_string(),
    })
}

fn set_once<T>(
    target: &mut Option<T>,
    value: T,
    source: &StableFormKey,
) -> Result<(), LegacyRaceFaceAssetError> {
    if target.replace(value).is_some() {
        return race_error(source, "legacy_race_appearance_duplicate_row_field");
    }
    Ok(())
}

fn require_marker(
    value: &FieldValue,
    source: &StableFormKey,
) -> Result<(), LegacyRaceFaceAssetError> {
    if matches!(value, FieldValue::None | FieldValue::Bool(true))
        || matches!(value, FieldValue::Bytes(bytes) if bytes.is_empty())
    {
        Ok(())
    } else {
        race_error(source, "legacy_race_appearance_marker_shape")
    }
}

fn exact_u32(value: &FieldValue, source: &StableFormKey) -> Result<u32, LegacyRaceFaceAssetError> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value)
            .map_err(|_| value_error(source, "legacy_race_appearance_uint_width")),
        FieldValue::Bytes(value) if value.len() == 4 => {
            Ok(u32::from_le_bytes(value.as_slice().try_into().unwrap()))
        }
        _ => race_error(source, "legacy_race_appearance_uint_shape"),
    }
}

fn exact_u8(value: &FieldValue, source: &StableFormKey) -> Result<u8, LegacyRaceFaceAssetError> {
    match value {
        FieldValue::Uint(value) => u8::try_from(*value)
            .map_err(|_| value_error(source, "legacy_race_appearance_uint_width")),
        FieldValue::Bytes(value) if value.len() == 1 => Ok(value[0]),
        _ => race_error(source, "legacy_race_appearance_uint_shape"),
    }
}

fn exact_string(
    value: &FieldValue,
    source: &StableFormKey,
    interner: &StringInterner,
) -> Result<String, LegacyRaceFaceAssetError> {
    let FieldValue::String(value) = value else {
        return race_error(source, "legacy_race_appearance_string_shape");
    };
    interner
        .resolve(*value)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| value_error(source, "legacy_race_appearance_string_unresolved"))
}

fn exact_bytes(
    value: &FieldValue,
    source: &StableFormKey,
) -> Result<Vec<u8>, LegacyRaceFaceAssetError> {
    let FieldValue::Bytes(value) = value else {
        return race_error(source, "legacy_race_appearance_bytes_shape");
    };
    Ok(value.to_vec())
}

fn decode_alternate_textures(
    value: &FieldValue,
    source: &StableFormKey,
    interner: &StringInterner,
) -> Result<Vec<LegacyRaceAlternateTextureEvidence>, LegacyRaceFaceAssetError> {
    let rows = match value {
        FieldValue::List(rows) => rows.as_slice(),
        FieldValue::Struct(_) => std::slice::from_ref(value),
        _ => return race_error(source, "legacy_race_appearance_alternate_texture_shape"),
    };
    rows.iter()
        .map(|row| {
            let FieldValue::Struct(fields) = row else {
                return race_error(source, "legacy_race_appearance_alternate_texture_shape");
            };
            let named = |name: &str| {
                fields.iter().find_map(|(key, value)| {
                    (interner.resolve(*key) == Some(name)).then_some(value)
                })
            };
            let node_name = exact_string(
                named("alternate_textures_3d_name").ok_or_else(|| {
                    value_error(source, "legacy_race_appearance_alternate_texture_fields")
                })?,
                source,
                interner,
            )?;
            let texture_set = match named("alternate_textures_new_texture") {
                Some(FieldValue::FormKey(value)) => StableFormKey {
                    local: value.local,
                    plugin: interner
                        .resolve(value.plugin)
                        .ok_or_else(|| {
                            value_error(source, "legacy_race_appearance_alternate_texture_form_key")
                        })?
                        .to_string(),
                },
                _ => {
                    return race_error(source, "legacy_race_appearance_alternate_texture_form_key");
                }
            };
            let node_index = match named("alternate_textures_3d_index") {
                Some(FieldValue::Int(value)) => i32::try_from(*value).map_err(|_| {
                    value_error(source, "legacy_race_appearance_alternate_texture_index")
                })?,
                _ => {
                    return race_error(source, "legacy_race_appearance_alternate_texture_index");
                }
            };
            Ok(LegacyRaceAlternateTextureEvidence {
                node_name,
                texture_set,
                node_index,
            })
        })
        .collect()
}

fn value_error(source: &StableFormKey, reason_code: &str) -> LegacyRaceFaceAssetError {
    LegacyRaceFaceAssetError {
        source: source.clone(),
        reason_code: reason_code.to_string(),
    }
}

fn race_error<T>(source: &StableFormKey, reason_code: &str) -> Result<T, LegacyRaceFaceAssetError> {
    Err(value_error(source, reason_code))
}

#[cfg(test)]
mod tests {
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    use super::*;
    use crate::translator::pair_hooks::fnv_fo4::creature_catalog::CreatureProvenance;

    fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(sig),
            value,
        }
    }

    fn source() -> StableFormKey {
        StableFormKey {
            local: 0x19,
            plugin: "FalloutNV.esm".to_string(),
        }
    }

    fn race(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("RACE").unwrap(),
            FormKey {
                local: 0x19,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        record
            .fields
            .push(field(*b"NAM0", FieldValue::Bytes(Vec::new().into())));
        for (marker, prefix) in [(*b"MNAM", "Male"), (*b"FNAM", "Female")] {
            record
                .fields
                .push(field(marker, FieldValue::Bytes(Vec::new().into())));
            for index in LEGACY_FACE_PART_INDICES {
                record
                    .fields
                    .push(field(*b"INDX", FieldValue::Uint(index.into())));
                if index != 1 {
                    record.fields.push(field(
                        *b"MODL",
                        FieldValue::String(
                            interner.intern(&format!("Characters\\Head\\{prefix}{index}.nif")),
                        ),
                    ));
                    record.fields.push(field(
                        *b"MODT",
                        FieldValue::Bytes(vec![index as u8; 4].into()),
                    ));
                }
                if index <= 5 {
                    record.fields.push(field(
                        *b"ICON",
                        FieldValue::String(
                            interner.intern(&format!("Characters\\Head\\{prefix}{index}.dds")),
                        ),
                    ));
                }
            }
        }
        record
            .fields
            .push(field(*b"NAM1", FieldValue::Bytes(Vec::new().into())));
        for (marker, path) in [
            (*b"MNAM", "Characters\\_Male\\UpperBodyHumanMale.egt"),
            (*b"FNAM", "Characters\\_Male\\UpperBodyHumanFemale.egt"),
        ] {
            record
                .fields
                .push(field(marker, FieldValue::Bytes(Vec::new().into())));
            record.fields.push(field(*b"INDX", FieldValue::Uint(3)));
            record
                .fields
                .push(field(*b"MODL", FieldValue::String(interner.intern(path))));
        }
        record
    }

    #[test]
    fn exact_male_and_female_face_rows_are_preserved_in_source_order() {
        let interner = StringInterner::new();
        let evidence = decode_legacy_race_face_assets(
            LegacyCreatureGame::Fnv,
            &source(),
            &race(&interner),
            &interner,
        )
        .unwrap();

        assert_eq!(
            evidence
                .male
                .rows
                .iter()
                .map(|row| row.index)
                .collect::<Vec<_>>(),
            LEGACY_FACE_PART_INDICES
        );
        assert_eq!(
            evidence.female.rows[0].model.as_deref(),
            Some("Characters\\Head\\Female0.nif")
        );
        assert_eq!(
            evidence.male.texture_basis_model,
            "Characters\\_Male\\UpperBodyHumanMale.egt"
        );
        assert_eq!(evidence.source_projection_blake3.len(), 64);
    }

    #[test]
    fn egt_header_binds_exact_texture_mode_payload() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"FREGT003");
        bytes.extend_from_slice(&32_u32.to_le_bytes());
        bytes.extend_from_slice(&32_u32.to_le_bytes());
        bytes.extend_from_slice(&50_u32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 36]);
        bytes.resize(64 + 50 * (4 + 32 * 32 * 3), 0);
        let header = decode_legacy_egt_texture_basis_header(&bytes).unwrap();
        assert_eq!(header.symmetric_modes, 50);
        assert_eq!(header.asymmetric_modes, 0);

        bytes.pop();
        assert!(
            decode_legacy_egt_texture_basis_header(&bytes)
                .unwrap_err()
                .contains("byte length")
        );
    }

    #[test]
    fn sex_or_index_shape_drift_remains_terminal() {
        let interner = StringInterner::new();
        let mut record = race(&interner);
        record
            .fields
            .retain(|field| !(field.sig.as_str() == "FNAM"));
        assert_eq!(
            decode_legacy_race_face_assets(LegacyCreatureGame::Fnv, &source(), &record, &interner,)
                .unwrap_err()
                .reason_code,
            "legacy_race_appearance_head_sections_missing"
        );

        let mut record = race(&interner);
        let index = record
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "INDX")
            .unwrap();
        index.value = FieldValue::Uint(8);
        assert_eq!(
            decode_legacy_race_face_assets(LegacyCreatureGame::Fnv, &source(), &record, &interner,)
                .unwrap_err()
                .reason_code,
            "legacy_race_appearance_face_part_indices"
        );
    }

    #[test]
    fn ancillary_race_slice_must_exactly_match_effective_rnam_set() {
        let interner = StringInterner::new();
        let race = race(&interner);
        let source_record = LegacyRecordSource {
            record: &race,
            provenance: CreatureProvenance {
                game: LegacyCreatureGame::Fnv,
                source_plugin: "FalloutNV.esm".to_string(),
                precedence: 0,
            },
        };
        let expected = BTreeSet::from([(LegacyCreatureGame::Fnv, source())]);
        let prepared =
            build_legacy_ancillary_race_face_assets(&expected, &[source_record.clone()], &interner)
                .unwrap();
        assert_eq!(prepared.len(), 1);
        assert!(prepared[0].evidence.is_ok());

        assert!(
            build_legacy_ancillary_race_face_assets(&BTreeSet::new(), &[source_record], &interner)
                .unwrap_err()
                .contains("extra")
        );
    }

    #[test]
    fn mapped_target_appearance_closure_is_exact_and_hash_bound() {
        let interner = StringInterner::new();
        let plugin = interner.intern("B21_Output.esp");
        let target = |local| FormKey { local, plugin };
        let mut race = Record::new(SigCode::from_str("RACE").unwrap(), target(0x100));
        race.fields.extend([
            field(*b"HEAD", FieldValue::FormKey(target(0x200))),
            field(*b"DFTM", FieldValue::FormKey(target(0x300))),
        ]);
        let mut head = Record::new(SigCode::from_str("HDPT").unwrap(), target(0x200));
        head.fields.extend([
            field(*b"TNAM", FieldValue::FormKey(target(0x300))),
            field(*b"RNAM", FieldValue::FormKey(target(0x400))),
        ]);
        let texture = Record::new(SigCode::from_str("TXST").unwrap(), target(0x300));
        let races = Record::new(SigCode::from_str("FLST").unwrap(), target(0x400));
        let root = StableFormKey {
            local: 0x100,
            plugin: "B21_Output.esp".to_string(),
        };
        let evidence = validate_legacy_target_appearance_closure(
            &BTreeSet::from([root]),
            &[race.clone(), head.clone(), texture.clone(), races.clone()],
            &interner,
        )
        .unwrap();
        assert_eq!(evidence.records.len(), 4);
        assert_eq!(evidence.closure_blake3.len(), 64);

        assert!(
            validate_legacy_target_appearance_closure(
                &evidence.race_roots.into_iter().collect(),
                &[race, head, races],
                &interner,
            )
            .unwrap_err()
            .contains("misses TXST")
        );
    }

    #[test]
    #[ignore = "requires explicit official FNV and FO3 Data directories"]
    fn live_official_effective_rnam_races_decode_exact_face_rows() {
        let fnv_data = std::path::PathBuf::from(
            std::env::var("BACUP_CENSUS_FNV_DATA_DIR")
                .expect("BACUP_CENSUS_FNV_DATA_DIR must name the official FNV Data directory"),
        );
        let fo3_data = std::path::PathBuf::from(
            std::env::var("BACUP_CENSUS_FO3_DATA_DIR")
                .expect("BACUP_CENSUS_FO3_DATA_DIR must name the official FO3 Data directory"),
        );
        let interner = StringInterner::new();
        let mut decoded = Vec::new();
        for (game, data_root, plugin, locals) in [
            (
                LegacyCreatureGame::Fnv,
                &fnv_data,
                "FalloutNV.esm",
                &[0x000019, 0x0038E5, 0x00424A][..],
            ),
            (
                LegacyCreatureGame::Fnv,
                &fnv_data,
                "HonestHearts.esm",
                &[0x00947C][..],
            ),
            (
                LegacyCreatureGame::Fo3,
                &fo3_data,
                "Fallout3.esm",
                &[
                    0x000019, 0x0038E5, 0x0038E6, 0x00424A, 0x0042BF, 0x0042C1, 0x0042C2, 0x0042C3,
                    0x04BB8D, 0x04BF71, 0x04BF72, 0x0987DD, 0x0987DF,
                ][..],
            ),
        ] {
            let path = data_root.join(plugin);
            let handle = crate::merge_sources::load_no_py(
                path.to_str().expect("Unicode plugin path"),
                Some(match game {
                    LegacyCreatureGame::Fnv => "fnv",
                    LegacyCreatureGame::Fo3 => "fo3",
                }),
            )
            .unwrap_or_else(|error| panic!("load {}: {error}", path.display()));
            let schema = crate::schema::AuthoringSchema::for_game(match game {
                LegacyCreatureGame::Fnv => "fnv",
                LegacyCreatureGame::Fo3 => "fo3",
            })
            .expect("source schema");
            for local in locals {
                let source = StableFormKey {
                    local: *local,
                    plugin: plugin.to_string(),
                };
                let record = crate::source_read::read_record(
                    handle,
                    &format!("{plugin}:{local:06X}"),
                    &schema,
                    &interner,
                )
                .unwrap_or_else(|error| panic!("read {source}: {error}"));
                decoded.push(
                    decode_legacy_race_face_assets(game, &source, &record, &interner)
                        .unwrap_or_else(|error| panic!("decode {source}: {error:?}")),
                );
            }
            assert!(
                esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle),
                "close {}",
                path.display()
            );
        }
        assert_eq!(decoded.len(), 17);
        assert!(
            decoded.iter().all(|evidence| {
                evidence.male.rows.len() == 8 && evidence.female.rows.len() == 8
            })
        );
    }

    #[test]
    #[ignore = "requires explicit official extracted FNV and FO3 Data roots"]
    fn live_official_legacy_egt_bases_are_exact_fgts_bases() {
        let roots = [
            (
                "fnv",
                std::path::PathBuf::from(
                    std::env::var("BACUP_CENSUS_FNV_EXTRACTED_DATA")
                        .expect("BACUP_CENSUS_FNV_EXTRACTED_DATA must name extracted FNV Data"),
                ),
                &["UpperBodyHumanMale.egt", "UpperBodyHumanFemale.egt"][..],
            ),
            (
                "fo3",
                std::path::PathBuf::from(
                    std::env::var("BACUP_CENSUS_FO3_EXTRACTED_DATA")
                        .expect("BACUP_CENSUS_FO3_EXTRACTED_DATA must name extracted FO3 Data"),
                ),
                &[
                    "UpperBodyHumanMale.egt",
                    "UpperBodyHumanFemale.egt",
                    "UpperBodyChild.egt",
                    "UpperBodyChildFemale.egt",
                ][..],
            ),
        ];
        for (game, root, files) in roots {
            for file in files {
                let runtime_path = format!("Meshes\\Characters\\_Male\\{file}");
                let path = root.join(&runtime_path);
                let bytes = std::fs::read(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                let header = decode_legacy_egt_texture_basis_header(&bytes)
                    .unwrap_or_else(|error| panic!("decode {}: {error}", path.display()));
                assert_eq!(header.rows, 32);
                assert_eq!(header.columns, 32);
                assert_eq!(header.symmetric_modes, 50);
                assert_eq!(header.asymmetric_modes, 0);
                assert_eq!(header.texture_basis_version, 81);
                println!(
                    "LEGACY_EGT_BASIS game={game} path={runtime_path} bytes={} blake3={}",
                    bytes.len(),
                    blake3::hash(&bytes).to_hex()
                );
            }
        }
    }
}
