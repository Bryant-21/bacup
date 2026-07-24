use crate::ids::FormKey;
use crate::record::FieldValue;
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};
use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::path::Path;

const MAX_LEVEL: f64 = 50.0;

pub(crate) type CurveMeanCache = FxHashMap<FormKey, Result<u32, String>>;

pub(crate) fn source_key_for_target(
    target: FormKey,
    target_to_source: &FxHashMap<FormKey, FormKey>,
    target_plugin: Sym,
    source_plugin: Sym,
) -> Option<FormKey> {
    target_to_source.get(&target).copied().or_else(|| {
        (target.plugin == target_plugin && target_plugin == source_plugin).then_some(FormKey {
            local: target.local,
            plugin: source_plugin,
        })
    })
}

#[derive(Deserialize)]
struct CurveFile {
    curve: Vec<CurvePoint>,
}

#[derive(Deserialize)]
struct CurvePoint {
    x: f64,
    y: f64,
}

pub(crate) fn cached_curve_mean(
    curve_fk: FormKey,
    session: &mut PluginSession,
    source_schema: &AuthoringSchema,
    source_extracted_dir: &Path,
    interner: &StringInterner,
    cache: &mut CurveMeanCache,
) -> Result<u32, String> {
    if let Some(cached) = cache.get(&curve_fk) {
        return cached.clone();
    }
    let result = read_curve_mean(
        curve_fk,
        session,
        source_schema,
        source_extracted_dir,
        interner,
    );
    cache.insert(curve_fk, result.clone());
    result
}

fn read_curve_mean(
    curve_fk: FormKey,
    session: &mut PluginSession,
    source_schema: &AuthoringSchema,
    source_extracted_dir: &Path,
    interner: &StringInterner,
) -> Result<u32, String> {
    let record = session
        .source_record_decoded(&curve_fk, source_schema, interner)
        .map_err(|error| format!("{:06X}:record:{error}", curve_fk.local))?;
    let jasf = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "JASF")
        .and_then(|entry| jasf_path(&entry.value, interner))
        .ok_or_else(|| format!("{:06X}:missing_jasf", curve_fk.local))?;
    let path = source_extracted_dir
        .join("misc")
        .join("curvetables")
        .join("json")
        .join(jasf.replace('\\', "/").to_ascii_lowercase());
    let json =
        std::fs::read_to_string(&path).map_err(|error| format!("{}:{error}", path.display()))?;
    mean_curve_value(&json).map_err(|error| format!("{}:{error}", path.display()))
}

fn jasf_path(value: &FieldValue, interner: &StringInterner) -> Option<String> {
    match value {
        FieldValue::String(sym) => interner.resolve(*sym).map(str::to_owned),
        FieldValue::Bytes(bytes) => {
            let end = bytes
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(bytes.len());
            std::str::from_utf8(&bytes[..end]).ok().map(str::to_owned)
        }
        _ => None,
    }
}

pub(crate) fn mean_curve_value(json: &str) -> Result<u32, String> {
    let curve_file: CurveFile =
        serde_json::from_str(json).map_err(|error| format!("invalid_json:{error}"))?;
    let mut values: Vec<f64> = curve_file
        .curve
        .iter()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .filter(|point| point.x <= MAX_LEVEL)
        .map(|point| point.y)
        .collect();
    if values.is_empty()
        && let Some(point) = curve_file
            .curve
            .iter()
            .filter(|point| point.x.is_finite() && point.y.is_finite())
            .min_by(|left, right| left.x.total_cmp(&right.x))
    {
        values.push(point.y);
    }
    if values.is_empty() {
        return Err("no_finite_points".into());
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    Ok(mean.round().clamp(0.0, u32::MAX as f64) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_local_target_resolves_without_mapper_entry() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let target = FormKey {
            local: 0x043C75,
            plugin,
        };

        assert_eq!(
            source_key_for_target(target, &FxHashMap::default(), plugin, plugin),
            Some(target)
        );
    }

    #[test]
    fn explicit_source_mapping_wins_over_same_local_fallback() {
        let interner = StringInterner::new();
        let target_plugin = interner.intern("Output.esp");
        let source_plugin = interner.intern("SeventySix.esm");
        let target = FormKey {
            local: 0x000800,
            plugin: target_plugin,
        };
        let source = FormKey {
            local: 0x043C75,
            plugin: source_plugin,
        };
        let mappings = FxHashMap::from_iter([(target, source)]);

        assert_eq!(
            source_key_for_target(target, &mappings, target_plugin, source_plugin),
            Some(source)
        );
    }

    #[test]
    fn differently_named_output_requires_an_explicit_mapping() {
        let interner = StringInterner::new();
        let target_plugin = interner.intern("Output.esp");
        let source_plugin = interner.intern("SeventySix.esm");
        let target = FormKey {
            local: 0x043C75,
            plugin: target_plugin,
        };

        assert_eq!(
            source_key_for_target(target, &FxHashMap::default(), target_plugin, source_plugin,),
            None
        );
    }
}
