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
pub(crate) type CurvePointsCache = FxHashMap<FormKey, Result<Vec<(f64, f64)>, String>>;

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

pub(crate) fn cached_curve_at_level(
    curve_fk: FormKey,
    level: u16,
    session: &mut PluginSession,
    source_schema: &AuthoringSchema,
    source_extracted_dir: &Path,
    interner: &StringInterner,
    cache: &mut CurvePointsCache,
) -> Result<u32, String> {
    if !cache.contains_key(&curve_fk) {
        let points = read_curve_json(
            curve_fk,
            session,
            source_schema,
            source_extracted_dir,
            interner,
        )
        .and_then(|(json, label)| curve_points(&json).map_err(|error| format!("{label}:{error}")));
        cache.insert(curve_fk, points);
    }
    match &cache[&curve_fk] {
        Ok(points) => Ok(curve_value_at_level(points, f64::from(level))),
        Err(error) => Err(error.clone()),
    }
}

fn read_curve_mean(
    curve_fk: FormKey,
    session: &mut PluginSession,
    source_schema: &AuthoringSchema,
    source_extracted_dir: &Path,
    interner: &StringInterner,
) -> Result<u32, String> {
    let (json, label) = read_curve_json(
        curve_fk,
        session,
        source_schema,
        source_extracted_dir,
        interner,
    )?;
    mean_curve_value(&json).map_err(|error| format!("{label}:{error}"))
}

fn read_curve_json(
    curve_fk: FormKey,
    session: &mut PluginSession,
    source_schema: &AuthoringSchema,
    source_extracted_dir: &Path,
    interner: &StringInterner,
) -> Result<(String, String), String> {
    let record = session
        .source_record_decoded(&curve_fk, source_schema, interner)
        .map_err(|error| format!("{:06X}:record:{error}", curve_fk.local))?;
    // 697 of SeventySix.esm's CURV records name their JSON under the older CRVE tag.
    let jasf = record
        .fields
        .iter()
        .find(|entry| matches!(entry.sig.as_str(), "JASF" | "CRVE"))
        .and_then(|entry| jasf_path(&entry.value, interner))
        .ok_or_else(|| format!("{:06X}:missing_jasf", curve_fk.local))?;
    let path = source_extracted_dir
        .join("misc")
        .join("curvetables")
        .join("json")
        .join(jasf.replace('\\', "/").to_ascii_lowercase());
    let label = path.display().to_string();
    let json = std::fs::read_to_string(&path).map_err(|error| format!("{label}:{error}"))?;
    Ok((json, label))
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

fn curve_points(json: &str) -> Result<Vec<(f64, f64)>, String> {
    let curve_file: CurveFile =
        serde_json::from_str(json).map_err(|error| format!("invalid_json:{error}"))?;
    let mut points: Vec<(f64, f64)> = curve_file
        .curve
        .iter()
        .filter(|point| point.x.is_finite() && point.y.is_finite())
        .map(|point| (point.x, point.y))
        .collect();
    if points.is_empty() {
        return Err("no_finite_points".into());
    }
    points.sort_by(|left, right| left.0.total_cmp(&right.0));
    Ok(points)
}

/// The curve's value at `level`: linear between authored points, held flat past
/// the first and last one. `points` must be non-empty and sorted by level.
pub(crate) fn curve_value_at_level(points: &[(f64, f64)], level: f64) -> u32 {
    let value = match points.iter().position(|(x, _)| *x >= level) {
        None => points[points.len() - 1].1,
        Some(0) => points[0].1,
        Some(index) => {
            let (low_x, low_y) = points[index - 1];
            let (high_x, high_y) = points[index];
            if high_x == low_x {
                high_y
            } else {
                low_y + (high_y - low_y) * (level - low_x) / (high_x - low_x)
            }
        }
    };
    value.round().clamp(0.0, u32::MAX as f64) as u32
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

    const HEALTH_UNIVERSAL_TIER18: [(f64, f64); 3] = [(1.0, 36.0), (23.0, 176.0), (100.0, 842.0)];

    #[test]
    fn curve_is_read_at_the_requested_level_between_authored_points() {
        let at = |level| curve_value_at_level(&HEALTH_UNIVERSAL_TIER18, level);
        assert_eq!(at(1.0), 36);
        assert_eq!(at(12.0), 106);
        assert_eq!(at(23.0), 176);
        assert_eq!(at(100.0), 842);

        assert_eq!(at(0.0), 36);
        assert_eq!(at(540.0), 842);

        assert_eq!(
            curve_points(r#"{"curve":[{"x":23,"y":176},{"x":1,"y":36}]}"#),
            Ok(vec![(1.0, 36.0), (23.0, 176.0)])
        );
        assert_eq!(
            curve_points(r#"{"curve":[]}"#),
            Err("no_finite_points".to_string())
        );
    }

    #[test]
    fn source_key_prefers_explicit_mapping_then_same_named_local() {
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
