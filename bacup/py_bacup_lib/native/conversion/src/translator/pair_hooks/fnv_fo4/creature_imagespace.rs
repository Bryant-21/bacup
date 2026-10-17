use std::path::Path;

use serde::Serialize;

use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::run::ConversionRun;
use crate::schema::AuthoringSchema;
use crate::sym::StringInterner;
use crate::translator::Game;

const LOWERED: &str = "fnv_creature_imagespace_curves_preserved";
const NELLIS_IMAD: u32 = 0x019CED;

pub(crate) fn decode_source_data(
    record: &str,
    subrecord: &str,
    bytes: &[u8],
    schema: &AuthoringSchema,
    interner: &StringInterner,
) -> Option<FieldValue> {
    if (record, subrecord) != ("IMAD", "DNAM") {
        return None;
    }
    let definition = schema.record_def(record)?.subrecord_def(subrecord)?;
    if !definition
        .fields
        .iter()
        .any(|field| field.id == "hdr_skin_dimmer_mult_count")
    {
        return None;
    }
    let layout = schema.struct_field_layout_versioned(record, subrecord, None);
    if layout
        .iter()
        .map(|field| field.offset + field.width)
        .max()?
        != bytes.len()
    {
        return None;
    }
    let mut fields = Vec::new();
    for field in layout {
        let definition = definition
            .fields
            .iter()
            .find(|item| item.id == field.field_id)?;
        let data = bytes.get(field.offset..field.offset + field.width)?;
        let value = match definition.kind.as_str() {
            "float32" => FieldValue::Float(f32::from_le_bytes(data.try_into().ok()?)),
            "uint32" => FieldValue::Uint(u32::from_le_bytes(data.try_into().ok()?) as u64),
            "uint8" => FieldValue::Uint(data[0] as u64),
            _ => return None,
        };
        fields.push((interner.intern(field.field_id), value));
    }
    Some(FieldValue::Struct(fields))
}

pub(crate) fn is_lowered(record: &Record, interner: &StringInterner) -> bool {
    record
        .warnings
        .iter()
        .any(|warning| interner.resolve(*warning) == Some(LOWERED))
}

fn is_nellis_source(record: &Record, interner: &StringInterner) -> bool {
    record.sig.0 == *b"IMAD"
        && record.form_key.local == NELLIS_IMAD
        && interner
            .resolve(record.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case("FalloutNV.esm"))
}

pub(crate) fn lower(record: &mut Record, interner: &StringInterner) -> Result<(), String> {
    if is_lowered(record, interner) || !is_nellis_source(record, interner) {
        return Ok(());
    }
    let source = record.clone();
    let (animated, duration) = animation_settings(&source, interner)?;
    if animated != 1 || !duration.is_finite() || duration <= 0.0 {
        return Err(
            "Nellis image-space effect requires a positive-duration animated source".into(),
        );
    }
    let _ = average_luminance_curve(&source, interner)?;
    let source_schema = AuthoringSchema::for_game("fnv").map_err(|error| error.to_string())?;
    let target_schema = AuthoringSchema::for_game("fo4").map_err(|error| error.to_string())?;
    let source_definition = source_schema
        .record_def("IMAD")
        .ok_or("source IMAD schema missing")?;
    let target_definition = target_schema
        .record_def("IMAD")
        .ok_or("target IMAD schema missing")?;
    for index in 0_u8..17 {
        for (operation, neutral) in [(0, 1.0), (64, 0.0)] {
            let signature = [index + operation, b'I', b'A', b'D'];
            if float_rows(&source, signature, 2, interner)?
                .iter()
                .any(|row| row[1] != neutral)
            {
                return Err("Nellis image-space HDR/bloom curve has unsupported nonneutral source semantics".into());
            }
        }
    }
    crate::translator::neutralize_legacy_imad(record, interner).map_err(str::to_owned)?;
    record.warnings.retain(|warning| {
        interner.resolve(*warning) != Some("legacy_imad_neutral_target_replacement")
    });
    let FieldValue::Struct(source_data) =
        value(&source, *b"DNAM").ok_or("source IMAD DNAM missing")?
    else {
        return Err("source IMAD DNAM was not decoded before relayout".into());
    };
    let FieldValue::Struct(target_data) = &mut record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"DNAM")
        .unwrap()
        .value
    else {
        unreachable!()
    };
    for (name, target) in target_data {
        let name = interner.resolve(*name).unwrap_or_default();
        if name.starts_with("hdr_")
            || name.starts_with("bloom_")
            || name.starts_with("cinematic_unused_")
        {
            continue;
        }
        if let Some((_, source)) = source_data
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some(name))
        {
            *target = source.clone();
        }
    }
    for source_field in &source.fields {
        let sig = source_field.sig.0;
        let target_sig = match sig {
            [17, b'I', b'A', b'D'] | [81, b'I', b'A', b'D'] => sig,
            [18, b'I', b'A', b'D'] => [19, b'I', b'A', b'D'],
            [82, b'I', b'A', b'D'] => [83, b'I', b'A', b'D'],
            [20, b'I', b'A', b'D'] => [18, b'I', b'A', b'D'],
            [84, b'I', b'A', b'D'] => [82, b'I', b'A', b'D'],
            [_, b'I', b'A', b'D'] => continue,
            sig if sig == *b"EDID" || sig == *b"DNAM" => continue,
            _ => sig,
        };
        let Some(source_def) = source_definition.subrecord_def(source_field.sig.as_str()) else {
            return Err("Nellis image-space source subrecord has no schema".into());
        };
        let target_name = std::str::from_utf8(&target_sig).map_err(|error| error.to_string())?;
        let Some(target_def) = target_definition.subrecord_def(target_name) else {
            return Err("Nellis image-space source subrecord has no target consumer".into());
        };
        let converted = if source_def
            .codec
            .as_deref()
            .is_some_and(|codec| codec.starts_with("array_struct:"))
        {
            if source_def.fields.len() != target_def.fields.len() {
                return Err("Nellis image-space source/target curve width differs".into());
            }
            let rows = float_rows(&source, sig, source_def.fields.len(), interner)?;
            FieldValue::Bytes(
                rows.into_iter()
                    .flatten()
                    .flat_map(f32::to_le_bytes)
                    .collect(),
            )
        } else {
            source_field.value.clone()
        };
        if let Some(target) = record
            .fields
            .iter_mut()
            .find(|field| field.sig.0 == target_sig)
        {
            target.value = converted;
        } else {
            let mut target = source_field.clone();
            target.sig = SubrecordSig(target_sig);
            target.value = converted;
            record.fields.push(target);
        }
    }
    record.warnings.push(interner.intern(LOWERED));
    Ok(())
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub(crate) struct CurvePoint {
    time: f32,
    value: f32,
}

fn average_luminance_curve(
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<CurvePoint>, String> {
    let rows = float_rows(record, [19, b'I', b'A', b'D'], 2, interner)?;
    if rows.is_empty()
        || rows.iter().any(|row| row[1] <= 0.0 || row[1] > 1.0)
        || rows.windows(2).any(|rows| rows[0][0] >= rows[1][0])
        || float_rows(record, [83, b'I', b'A', b'D'], 2, interner)?
            .iter()
            .any(|row| row[1] != 0.0)
    {
        return Err(
            "Nellis image-space luminance curve is outside the verified multiplier contract".into(),
        );
    }
    Ok(rows
        .into_iter()
        .map(|row| CurvePoint {
            time: row[0],
            value: row[1],
        })
        .collect())
}

fn float_rows(
    record: &Record,
    signature: [u8; 4],
    width: usize,
    interner: &StringInterner,
) -> Result<Vec<Vec<f32>>, String> {
    let field = value(record, signature)
        .ok_or_else(|| format!("missing Nellis IMAD curve {signature:?}"))?;
    let rows: Vec<Vec<f32>> = match field {
        FieldValue::Bytes(bytes) if bytes.len() % (width * 4) == 0 => bytes
            .chunks_exact(width * 4)
            .map(|row| {
                row.chunks_exact(4)
                    .map(|value| f32::from_le_bytes(value.try_into().unwrap()))
                    .collect()
            })
            .collect(),
        FieldValue::List(rows) => {
            let schema = AuthoringSchema::for_game("fnv").map_err(|error| error.to_string())?;
            let name = std::str::from_utf8(&signature).map_err(|error| error.to_string())?;
            let definition = schema
                .record_def("IMAD")
                .and_then(|record| record.subrecord_def(name))
                .ok_or("source curve schema missing")?;
            rows.iter()
                .map(|row| {
                    let FieldValue::Struct(fields) = row else {
                        return Err("source curve row is not a struct".to_string());
                    };
                    if width == 2 {
                        return ["_time", "_value"]
                            .into_iter()
                            .map(|suffix| {
                                match fields
                                    .iter()
                                    .find(|(name, _)| {
                                        interner
                                            .resolve(*name)
                                            .is_some_and(|name| name.ends_with(suffix))
                                    })
                                    .map(|(_, value)| value)
                                {
                                    Some(FieldValue::Float(value)) => Ok(*value),
                                    _ => Err("source curve component is not a float".to_string()),
                                }
                            })
                            .collect::<Result<Vec<_>, _>>();
                    }
                    definition
                        .fields
                        .iter()
                        .map(|definition| {
                            match fields
                                .iter()
                                .find(|(name, _)| {
                                    interner.resolve(*name) == Some(definition.id.as_str())
                                })
                                .map(|(_, value)| value)
                            {
                                Some(FieldValue::Float(value)) => Ok(*value),
                                _ => Err("source curve component is not a float".to_string()),
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        _ => return Err("source IMAD curve has an unsupported shape".into()),
    };
    if rows
        .iter()
        .any(|row| row.len() != width || row.iter().any(|value| !value.is_finite()))
    {
        return Err("source IMAD curve has invalid components".into());
    }
    Ok(rows)
}

fn value(record: &Record, signature: [u8; 4]) -> Option<&FieldValue> {
    record
        .fields
        .iter()
        .find(|field| field.sig.0 == signature)
        .map(|field| &field.value)
}

fn animation_settings(record: &Record, interner: &StringInterner) -> Result<(u32, f32), String> {
    match value(record, *b"DNAM") {
        Some(FieldValue::Bytes(bytes)) if matches!(bytes.len(), 244 | 252) => Ok((
            u32::from_le_bytes(bytes[..4].try_into().unwrap()),
            f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
        )),
        Some(FieldValue::Struct(fields)) => {
            let field = |name| {
                fields
                    .iter()
                    .find(|(key, _)| interner.resolve(*key) == Some(name))
                    .map(|(_, value)| value)
            };
            match (field("animatable"), field("duration")) {
                (Some(FieldValue::Uint(animated)), Some(FieldValue::Float(duration))) => {
                    Ok((*animated as u32, *duration))
                }
                _ => Err("Nellis IMAD animation settings are malformed".into()),
            }
        }
        _ => Err("Nellis IMAD animation settings are missing".into()),
    }
}

#[derive(Serialize)]
pub(crate) struct ImageSpaceOutput {
    version: u32,
    modifiers: Vec<ImageSpaceModifier>,
    #[serde(skip)]
    plugin: String,
}

#[derive(Serialize)]
struct ImageSpaceModifier {
    modifier: String,
    average_luminance_multiplier: Vec<CurvePoint>,
}

impl ImageSpaceOutput {
    pub(crate) fn write(&self, mod_path: &Path) -> Result<(), String> {
        let directory = mod_path.join("F4SE/Plugins/B21_ConversionRuntime/FnvImageSpace");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        std::fs::write(
            directory.join(format!("{}.json", self.plugin)),
            serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }
}

pub(crate) fn prepare_output(run: &ConversionRun) -> Result<Option<ImageSpaceOutput>, String> {
    if run.source != Game::Fnv || run.target != Game::Fo4 || run.source_handle().is_none() {
        return Ok(None);
    }
    let source_key = FormKey {
        plugin: run.interner.intern("FalloutNV.esm"),
        local: NELLIS_IMAD,
    };
    if !run.is_admitted_creature_dependency(source_key, crate::ids::SigCode(*b"IMAD")) {
        return Ok(None);
    }
    let source = crate::source_read::read_record_relayout_by_form_key(
        run.source_handle_id,
        &source_key,
        &run.schema_source,
        &run.interner,
        None,
    )
    .map_err(|error| error.to_string())?;
    let target_key = run
        .mapper_state
        .as_ref()
        .and_then(|mapper| mapper.source_to_target.get(&source_key))
        .ok_or("Nellis IMAD output mapping missing")?;
    if !run
        .interner
        .resolve(target_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(&run.config.output_plugin_name))
    {
        return Err("Nellis IMAD mapping does not name the converted output plugin".into());
    }
    let target = run
        .read_target_record_if_available(*target_key)
        .map_err(|error| error.to_string())?
        .ok_or("Nellis IMAD was not emitted")?;
    if target.sig.0 != *b"IMAD" {
        return Err("Nellis IMAD output has wrong signature".into());
    }
    if animation_settings(&source, &run.interner)? != animation_settings(&target, &run.interner)? {
        return Err("Nellis IMAD output lost source animation timing".into());
    }
    for (source_sig, target_sig) in [
        (*b"BNAM", *b"BNAM"),
        ([18, b'I', b'A', b'D'], [19, b'I', b'A', b'D']),
        ([20, b'I', b'A', b'D'], [18, b'I', b'A', b'D']),
    ] {
        if float_rows(&source, source_sig, 2, &run.interner)?
            != float_rows(&target, target_sig, 2, &run.interner)?
        {
            return Err("Nellis IMAD output lost source blur/contrast/brightness curves".into());
        }
    }
    Ok(Some(ImageSpaceOutput {
        version: 1,
        modifiers: vec![ImageSpaceModifier {
            modifier: format!(
                "{:06X}:{}",
                target_key.local,
                run.interner.resolve(target_key.plugin).unwrap_or_default()
            ),
            average_luminance_multiplier: average_luminance_curve(&source, &run.interner)?,
        }],
        plugin: run.config.output_plugin_name.clone(),
    }))
}

#[cfg(test)]
mod tests;
