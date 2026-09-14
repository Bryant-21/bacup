//! Translation map loader — reads per-game-pair YAML files into typed structs.
//!
//! Translation-map YAML data is embedded in the binary via `include_str!` in
//! `crate::phase::record_translation::embedded`.  `TranslationMaps::load` looks
//! up the embedded text; no filesystem access is required at runtime.

use super::super::errors::ConfigError;
use super::{DeferredKind, Game};
use crate::embedded;
use crate::ids::SubrecordSig;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordRoutePolicy {
    Compatible,
    TargetProjection,
    PairHook,
    TopologyRebuild,
    Blocked,
}

impl RecordRoutePolicy {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "compatible" => Some(Self::Compatible),
            "target_projection" => Some(Self::TargetProjection),
            "pair_hook" => Some(Self::PairHook),
            "topology_rebuild" => Some(Self::TopologyRebuild),
            "blocked" => Some(Self::Blocked),
            _ => None,
        }
    }
}

pub(crate) fn is_quest_runtime_signature(signature: &str) -> bool {
    matches!(
        signature,
        "QUST" | "DIAL" | "INFO" | "SCEN" | "SMBN" | "SMEN" | "SMQN" | "DLBR" | "DLVW"
    )
}

/// Return the embedded YAML text for the map named `key` (e.g. `"fo76_to_fo4"`).
/// Returns an empty string if no embedded map exists for that key.
fn embedded_map_text(key: &str) -> &'static str {
    for (label, text) in embedded::PRIMARY_MAPS {
        if *label == key {
            return text;
        }
    }
    ""
}

/// The raw serde-saphyr YAML value type used to hold transform configs.
/// Reusing serde_json::Value since serde-saphyr can deserialize into it.
pub type YamlValue = serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MapDiagnosticSeverity {
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TranslationMapDiagnostic {
    pub severity: MapDiagnosticSeverity,
    pub code: String,
    pub pair: String,
    pub map_file: String,
    pub record_signature: Option<String>,
    pub rule: String,
    pub path: String,
    pub source_field: Option<String>,
    pub target_field: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RuleCoverage {
    pub total: usize,
    pub executable: usize,
    pub rejected: usize,
}

impl RuleCoverage {
    fn add_assign(&mut self, other: &Self) {
        self.total += other.total;
        self.executable += other.executable;
        self.rejected += other.rejected;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RecordMapCoverage {
    pub signature: String,
    pub field_rewrites: RuleCoverage,
    pub transforms: RuleCoverage,
    pub drops: RuleCoverage,
    pub diagnostic_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TranslationMapCoverage {
    pub source: String,
    pub target: String,
    pub pair: String,
    pub map_file: String,
    pub records: Vec<RecordMapCoverage>,
    pub totals: RecordMapCoverage,
    pub diagnostics: Vec<TranslationMapDiagnostic>,
}

impl TranslationMapCoverage {
    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

#[derive(Debug)]
pub enum TranslationMapCompileError {
    Load(ConfigError),
    Diagnostics(TranslationMapCoverage),
}

impl TranslationMapCompileError {
    pub fn coverage(&self) -> Option<&TranslationMapCoverage> {
        match self {
            Self::Load(_) => None,
            Self::Diagnostics(coverage) => Some(coverage),
        }
    }
}

impl std::fmt::Display for TranslationMapCompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(error) => error.fmt(f),
            Self::Diagnostics(coverage) => write!(
                f,
                "translation map compile rejected {} with {} diagnostic(s)",
                coverage.map_file,
                coverage.diagnostics.len()
            ),
        }
    }
}

impl std::error::Error for TranslationMapCompileError {}

/// Per-record translation specification loaded from the YAML map file.
#[derive(Debug, Default)]
pub struct RecordMap {
    /// Source record sig (4-char, matches the YAML key).
    pub source_sig: String,
    /// Optional target sig override (e.g. CREA → NPC_).
    pub target_sig: Option<String>,
    /// Field rewrites: source_field → target_field name mappings.
    pub field_rewrites: Vec<FieldRewrite>,
    /// Named transform invocations to run on specific fields.
    pub transforms: Vec<TransformInvocation>,
    /// Fields to drop entirely.
    pub drop_fields: Vec<String>,
    /// Optional hand-off to a dedicated conversion pass.
    pub delegate: Option<DeferredKind>,
}

impl RecordMap {
    fn has_executable_policy(&self) -> bool {
        self.target_sig.is_some()
            || !self.field_rewrites.is_empty()
            || !self.transforms.is_empty()
            || !self.drop_fields.is_empty()
            || self.delegate.is_some()
    }
}

/// A simple field-name rewrite: rename source_field to target_field.
#[derive(Debug)]
pub struct FieldRewrite {
    pub source_field: String,
    pub target_field: String,
}

/// A named transform + its config blob.
#[derive(Debug)]
pub struct TransformInvocation {
    pub field: String,
    pub name: String,
    pub config: YamlValue,
}

/// Loaded translation maps for one game pair.
#[derive(Debug, Default)]
pub struct TranslationMaps {
    record_maps: rustc_hash::FxHashMap<String, RecordMap>,
    pub skip_records: rustc_hash::FxHashSet<String>,
    pub(crate) record_routes: rustc_hash::FxHashMap<String, RecordRoutePolicy>,
}

impl TranslationMaps {
    /// Compile an embedded map with the strict lint gate.
    ///
    /// `load` remains permissive while legacy maps are being repaired. Release
    /// workflows can switch to this entry point once their coverage is clean.
    pub fn compile(source: Game, target: Game) -> Result<Self, TranslationMapCompileError> {
        let coverage = Self::coverage(source, target).map_err(TranslationMapCompileError::Load)?;
        if coverage.has_errors() {
            return Err(TranslationMapCompileError::Diagnostics(coverage));
        }
        Self::load(source, target).map_err(TranslationMapCompileError::Load)
    }

    /// Return the serializable rule-coverage and diagnostic report for a map.
    pub fn coverage(source: Game, target: Game) -> Result<TranslationMapCoverage, ConfigError> {
        let key = format!("{}_to_{}", source.as_str(), target.as_str());
        let map_file = format!("embedded:{key}.yaml");
        let text = embedded_map_text(&key);
        if text.trim().is_empty() {
            return Ok(empty_coverage(source, target, map_file));
        }
        let raw = parse_map_text(&key, text)?;
        Ok(lint_map_value(source, target, map_file, &raw))
    }

    /// Load the YAML translation map for (source, target).
    ///
    /// Uses embedded YAML data compiled into the binary.  If no embedded map
    /// exists for the pair, returns an empty `TranslationMaps` (not an error —
    /// some pairs intentionally have no map).
    pub fn load(source: Game, target: Game) -> Result<Self, ConfigError> {
        let key = format!("{}_to_{}", source.as_str(), target.as_str());
        let text = embedded_map_text(&key);
        if text.trim().is_empty() {
            return Ok(TranslationMaps::default());
        }
        let raw = parse_map_text(&key, text)?;
        let mut maps = Self::from_value(raw)?;
        if source == Game::Fo76
            && target == Game::Fo4
            && let Some(npc_map) = maps.record_maps.get_mut("NPC_")
        {
            // NPC tint layers depend on RACE tint tables that are intentionally
            // not carried into FO4. Drop the header and payload together while
            // leaving QNAM intact for the body/face skin-tone match.
            npc_map.drop_fields.push("TETI".to_string());
            npc_map.drop_fields.push("TEND".to_string());
        }
        // CK-crash-risk bisect gate: SCEN/DLBR are emitted by default (they
        // resolve NOTE\SNAM-Scene and INFO\BNAM-DLBR references). Setting
        // MODBOX_DISABLE_SCEN re-skips them so an in-game crash can be
        // bisected against scene emission. FO76->FO4 only.
        if source == Game::Fo76
            && target == Game::Fo4
            && std::env::var_os("MODBOX_DISABLE_SCEN").is_some()
        {
            maps.skip_records.insert("SCEN".to_string());
            maps.skip_records.insert("DLBR".to_string());
        }
        Ok(maps)
    }

    fn from_value(raw: serde_json::Value) -> Result<Self, ConfigError> {
        let mut maps = TranslationMaps::default();

        let obj = match raw {
            serde_json::Value::Object(m) => m,
            _ => return Ok(maps),
        };

        for (key, val) in obj {
            match key.as_str() {
                "skip_records" => {
                    if let serde_json::Value::Array(arr) = val {
                        for item in arr {
                            if let serde_json::Value::String(s) = item {
                                maps.skip_records.insert(s);
                            }
                        }
                    }
                }
                "record_routes" => {
                    if let serde_json::Value::Object(routes) = val {
                        for (policy, signatures) in routes {
                            let Some(policy) = RecordRoutePolicy::parse(&policy) else {
                                continue;
                            };
                            let Some(signatures) = signatures.as_array() else {
                                continue;
                            };
                            for signature in signatures.iter().filter_map(|value| value.as_str()) {
                                if is_raw_signature(signature) {
                                    maps.record_routes.insert(signature.to_string(), policy);
                                }
                            }
                        }
                    }
                }
                "material_overrides" => {
                    // Ignored at this layer — consumed by Python hooks.
                }
                sig => {
                    let rec_map = parse_record_map(sig, val)?;
                    maps.record_maps.insert(sig.to_string(), rec_map);
                }
            }
        }

        Ok(maps)
    }

    /// Look up the map for a given source record signature (e.g. "WEAP").
    pub fn record_map(&self, sig: &str) -> Option<&RecordMap> {
        self.record_maps.get(sig)
    }

    pub fn record_route(&self, sig: &str) -> Option<RecordRoutePolicy> {
        self.record_routes.get(sig).copied()
    }
}

fn parse_map_text(key: &str, text: &str) -> Result<serde_json::Value, ConfigError> {
    serde_saphyr::from_str(text).map_err(|error| ConfigError::MapFileMalformed {
        path: std::path::PathBuf::from(format!("embedded:{key}.yaml")),
        source: error.to_string(),
    })
}

fn empty_coverage(source: Game, target: Game, map_file: String) -> TranslationMapCoverage {
    TranslationMapCoverage {
        source: source.as_str().to_string(),
        target: target.as_str().to_string(),
        pair: format!("{}->{}", source.as_str(), target.as_str()),
        map_file,
        records: Vec::new(),
        totals: RecordMapCoverage::default(),
        diagnostics: Vec::new(),
    }
}

fn lint_map_value(
    source: Game,
    target: Game,
    map_file: String,
    raw: &serde_json::Value,
) -> TranslationMapCoverage {
    let mut coverage = empty_coverage(source, target, map_file);
    let Some(root) = raw.as_object() else {
        push_diagnostic(
            &mut coverage,
            None,
            "map",
            "$",
            "map_not_mapping",
            None,
            None,
            "translation map root must be a mapping",
        );
        return coverage;
    };

    for (key, value) in root {
        match key.as_str() {
            "skip_records" => lint_skip_records(&mut coverage, value),
            "record_routes" => lint_record_routes(&mut coverage, value),
            "material_overrides" => {}
            signature if is_raw_signature(signature) => {
                let diagnostic_start = coverage.diagnostics.len();
                let mut record = RecordMapCoverage {
                    signature: signature.to_string(),
                    ..Default::default()
                };
                lint_record_map(&mut coverage, &mut record, value);
                record.diagnostic_count = coverage.diagnostics.len() - diagnostic_start;
                coverage
                    .totals
                    .field_rewrites
                    .add_assign(&record.field_rewrites);
                coverage.totals.transforms.add_assign(&record.transforms);
                coverage.totals.drops.add_assign(&record.drops);
                coverage.records.push(record);
            }
            _ => push_diagnostic(
                &mut coverage,
                None,
                "map",
                key,
                "unknown_root_key",
                None,
                None,
                format!(
                    "root key {key:?} is neither a supported directive nor a 4CC record signature"
                ),
            ),
        }
    }

    coverage
        .records
        .sort_by(|left, right| left.signature.cmp(&right.signature));
    coverage.diagnostics.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.reason.cmp(&right.reason))
    });
    coverage
}

fn lint_record_routes(coverage: &mut TranslationMapCoverage, value: &serde_json::Value) {
    let Some(routes) = value.as_object() else {
        push_diagnostic(
            coverage,
            None,
            "record_routes",
            "record_routes",
            "invalid_rule_shape",
            None,
            None,
            "record_routes must map route policies to lists of 4CC signatures",
        );
        return;
    };
    let mut owners = rustc_hash::FxHashMap::<&str, &str>::default();
    for (policy, signatures) in routes {
        if RecordRoutePolicy::parse(policy).is_none() {
            push_diagnostic(
                coverage,
                None,
                "record_routes",
                format!("record_routes.{policy}"),
                "unsupported_record_route_policy",
                None,
                None,
                format!("record route policy {policy:?} is not registered"),
            );
            continue;
        }
        let Some(signatures) = signatures.as_array() else {
            push_diagnostic(
                coverage,
                None,
                "record_routes",
                format!("record_routes.{policy}"),
                "invalid_rule_shape",
                None,
                None,
                "record route policy must contain a list of 4CC signatures",
            );
            continue;
        };
        for (index, signature) in signatures.iter().enumerate() {
            let path = format!("record_routes.{policy}[{index}]");
            let Some(signature) = signature.as_str() else {
                push_diagnostic(
                    coverage,
                    None,
                    "record_routes",
                    path,
                    "invalid_record_signature",
                    None,
                    None,
                    "record route entries must be raw 4CC signatures",
                );
                continue;
            };
            if !is_raw_signature(signature) {
                push_diagnostic(
                    coverage,
                    Some(signature),
                    "record_routes",
                    path,
                    "invalid_record_signature",
                    Some(signature.to_string()),
                    None,
                    "record route entries must be raw 4CC signatures",
                );
                continue;
            }
            if let Some(previous) = owners.insert(signature, policy) {
                push_diagnostic(
                    coverage,
                    Some(signature),
                    "record_routes",
                    path,
                    "duplicate_record_route",
                    Some(signature.to_string()),
                    None,
                    format!("record route is already owned by {previous:?}"),
                );
            }
        }
    }
}

fn lint_skip_records(coverage: &mut TranslationMapCoverage, value: &serde_json::Value) {
    let Some(entries) = value.as_array() else {
        push_diagnostic(
            coverage,
            None,
            "skip_records",
            "skip_records",
            "invalid_rule_shape",
            None,
            None,
            "skip_records must be a list of 4CC record signatures",
        );
        return;
    };
    for (index, entry) in entries.iter().enumerate() {
        let path = format!("skip_records[{index}]");
        if !entry.as_str().is_some_and(is_raw_signature) {
            push_diagnostic(
                coverage,
                None,
                "skip_records",
                path,
                "invalid_record_signature",
                entry.as_str().map(str::to_string),
                None,
                "skip_records entries must be raw 4CC record signatures",
            );
        }
    }
}

fn lint_record_map(
    coverage: &mut TranslationMapCoverage,
    record: &mut RecordMapCoverage,
    value: &serde_json::Value,
) {
    let signature = record.signature.clone();
    let Some(mapping) = value.as_object() else {
        push_diagnostic(
            coverage,
            Some(&signature),
            "record",
            &signature,
            "invalid_rule_shape",
            None,
            None,
            "record map must be a mapping",
        );
        return;
    };

    for (key, value) in mapping {
        match key.as_str() {
            "target_record_type" => lint_target_record_type(coverage, &signature, value),
            "fields" => lint_field_rewrites(coverage, record, value),
            "transforms" => lint_transforms(coverage, record, value),
            "drop" => lint_drops(coverage, record, value),
            "delegate" => lint_delegate(coverage, &signature, value),
            "defaults" | "edid_prefix" => push_diagnostic(
                coverage,
                Some(&signature),
                key,
                format!("{signature}.{key}"),
                "ignored_top_level_key",
                None,
                None,
                format!("{key} is accepted by YAML but ignored by the translation-map executor"),
            ),
            _ => push_diagnostic(
                coverage,
                Some(&signature),
                key,
                format!("{signature}.{key}"),
                "unknown_record_key",
                None,
                None,
                format!("record-map key {key:?} is not compiled or executed"),
            ),
        }
    }
}

fn lint_delegate(
    coverage: &mut TranslationMapCoverage,
    signature: &str,
    value: &serde_json::Value,
) {
    match value.as_str() {
        Some("fnv_legacy_scripting") => {}
        Some(delegate) => push_diagnostic(
            coverage,
            Some(signature),
            "delegate",
            format!("{signature}.delegate"),
            "unsupported_delegate",
            Some(delegate.to_string()),
            None,
            format!("delegate {delegate:?} is not registered"),
        ),
        None => push_diagnostic(
            coverage,
            Some(signature),
            "delegate",
            format!("{signature}.delegate"),
            "invalid_rule_shape",
            None,
            None,
            "delegate must be a registered string name",
        ),
    }
}

fn lint_target_record_type(
    coverage: &mut TranslationMapCoverage,
    signature: &str,
    value: &serde_json::Value,
) {
    if !value.as_str().is_some_and(is_raw_signature) {
        push_diagnostic(
            coverage,
            Some(signature),
            "target_record_type",
            format!("{signature}.target_record_type"),
            "invalid_record_signature",
            None,
            value.as_str().map(str::to_string),
            "target_record_type must be a raw 4CC record signature",
        );
    }
}

fn lint_field_rewrites(
    coverage: &mut TranslationMapCoverage,
    record: &mut RecordMapCoverage,
    value: &serde_json::Value,
) {
    let signature = record.signature.as_str();
    let Some(fields) = value.as_object() else {
        push_diagnostic(
            coverage,
            Some(signature),
            "fields",
            format!("{signature}.fields"),
            "invalid_rule_shape",
            None,
            None,
            "fields must be a source-to-target mapping",
        );
        return;
    };

    for (source, target_value) in fields {
        record.field_rewrites.total += 1;
        let path = format!("{signature}.fields.{source}");
        let Some(target) = target_value.as_str() else {
            record.field_rewrites.rejected += 1;
            push_diagnostic(
                coverage,
                Some(signature),
                "field_rewrite",
                path,
                "invalid_rule_shape",
                Some(source.to_string()),
                None,
                "field rewrite target must be a string",
            );
            continue;
        };
        let source_is_raw = is_raw_signature(source);
        let target_is_raw = is_raw_signature(target);
        if source_is_raw && target_is_raw {
            record.field_rewrites.executable += 1;
        }
        if !source_is_raw || !target_is_raw {
            record.field_rewrites.rejected += 1;
            push_diagnostic(
                coverage,
                Some(signature),
                "field_rewrite",
                &path,
                "unresolved_semantic_field_path",
                Some(source.to_string()),
                Some(target.to_string()),
                "field rewrites execute on raw 4CC subrecord signatures; semantic paths require schema resolution",
            );
        } else if source != target {
            record.field_rewrites.rejected += 1;
            push_diagnostic(
                coverage,
                Some(signature),
                "field_rewrite",
                &path,
                "first_only_rewrite",
                Some(source.to_string()),
                Some(target.to_string()),
                "the executor renames only the first matching subrecord, so repeated rows are unsafe",
            );
        }
    }
}

fn lint_transforms(
    coverage: &mut TranslationMapCoverage,
    record: &mut RecordMapCoverage,
    value: &serde_json::Value,
) {
    let signature = record.signature.as_str();
    let Some(transforms) = value.as_object() else {
        push_diagnostic(
            coverage,
            Some(signature),
            "transforms",
            format!("{signature}.transforms"),
            "invalid_rule_shape",
            None,
            None,
            "transforms must be a field-to-config mapping",
        );
        return;
    };
    let registry = super::transforms::default_registry();

    for (field, config) in transforms {
        record.transforms.total += 1;
        let path = format!("{signature}.transforms.{field}");
        let mut rejected = false;
        if !is_raw_signature(field) {
            rejected = true;
            push_diagnostic(
                coverage,
                Some(signature),
                "transform",
                &path,
                "unresolved_semantic_field_path",
                Some(field.to_string()),
                None,
                "transforms execute on raw 4CC subrecord signatures; semantic paths require schema resolution",
            );
        }

        let Some(config_object) = config.as_object() else {
            record.transforms.rejected += 1;
            push_diagnostic(
                coverage,
                Some(signature),
                "transform",
                path,
                "transform_config_failure",
                Some(field.to_string()),
                None,
                "transform configuration must be a mapping with a string type",
            );
            continue;
        };
        let Some(name) = config_object.get("type").and_then(|value| value.as_str()) else {
            record.transforms.rejected += 1;
            push_diagnostic(
                coverage,
                Some(signature),
                "transform",
                path,
                "unknown_transform",
                Some(field.to_string()),
                None,
                "transform configuration is missing a string type",
            );
            continue;
        };
        if registry.get(name).is_none() {
            rejected = true;
            push_diagnostic(
                coverage,
                Some(signature),
                "transform",
                &path,
                "unsupported_transform",
                Some(field.to_string()),
                None,
                format!("transform {name:?} is not registered"),
            );
        } else if let Some(reason) = transform_config_failure(name, config) {
            rejected = true;
            push_diagnostic(
                coverage,
                Some(signature),
                "transform",
                &path,
                "transform_config_failure",
                Some(field.to_string()),
                None,
                reason,
            );
        }
        if let Some(target) = config_object.get("target").and_then(|value| value.as_str())
            && target != field
        {
            rejected = true;
            push_diagnostic(
                coverage,
                Some(signature),
                "transform",
                &path,
                "unapplied_transform_target",
                Some(field.to_string()),
                Some(target.to_string()),
                "the transform target is advisory; the executor does not rename the source field",
            );
        }
        if is_raw_signature(field) && registry.get(name).is_some() {
            record.transforms.executable += 1;
        }
        if rejected {
            record.transforms.rejected += 1;
        }
    }
}

fn lint_drops(
    coverage: &mut TranslationMapCoverage,
    record: &mut RecordMapCoverage,
    value: &serde_json::Value,
) {
    let signature = record.signature.as_str();
    let Some(entries) = value.as_array() else {
        push_diagnostic(
            coverage,
            Some(signature),
            "drop",
            format!("{signature}.drop"),
            "invalid_rule_shape",
            None,
            None,
            "drop must be a list of raw 4CC subrecord signatures",
        );
        return;
    };

    for (index, entry) in entries.iter().enumerate() {
        record.drops.total += 1;
        let path = format!("{signature}.drop[{index}]");
        let Some(field) = entry.as_str() else {
            record.drops.rejected += 1;
            push_diagnostic(
                coverage,
                Some(signature),
                "drop",
                path,
                "invalid_rule_shape",
                None,
                None,
                "drop entry must be a string",
            );
            continue;
        };
        if is_raw_signature(field) {
            record.drops.executable += 1;
        } else {
            record.drops.rejected += 1;
            push_diagnostic(
                coverage,
                Some(signature),
                "drop",
                path,
                "semantic_drop_name",
                Some(field.to_string()),
                None,
                "drop matches raw 4CC subrecord signatures; semantic names can never match",
            );
        }
    }
}

fn transform_config_failure(name: &str, config: &serde_json::Value) -> Option<String> {
    let require_non_empty_string = |key: &str| {
        config
            .get(key)
            .and_then(|value| value.as_str())
            .is_some_and(|value| !value.is_empty())
    };
    match name {
        "enum_map" if !config.get("map").is_some_and(serde_json::Value::is_object) => {
            Some("enum_map requires a map object".to_string())
        }
        "remap_formkey"
            if !require_non_empty_string("source_esm")
                || !require_non_empty_string("target_esm") =>
        {
            Some("remap_formkey requires non-empty source_esm and target_esm strings".to_string())
        }
        "fo76_scol_static" if !require_non_empty_string("source_esm") => {
            Some("fo76_scol_static requires a non-empty source_esm string".to_string())
        }
        "trim_languages" if !config.get("keep").is_some_and(serde_json::Value::is_array) => {
            Some("trim_languages requires a keep list".to_string())
        }
        _ => None,
    }
}

fn push_diagnostic(
    coverage: &mut TranslationMapCoverage,
    record_signature: Option<&str>,
    rule: &str,
    path: impl Into<String>,
    code: &str,
    source_field: Option<String>,
    target_field: Option<String>,
    reason: impl Into<String>,
) {
    coverage.diagnostics.push(TranslationMapDiagnostic {
        severity: MapDiagnosticSeverity::Error,
        code: code.to_string(),
        pair: coverage.pair.clone(),
        map_file: coverage.map_file.clone(),
        record_signature: record_signature.map(str::to_string),
        rule: rule.to_string(),
        path: path.into(),
        source_field,
        target_field,
        reason: reason.into(),
    });
}

fn is_raw_signature(value: &str) -> bool {
    SubrecordSig::from_str(value).is_ok()
}

fn parse_record_map(sig: &str, val: serde_json::Value) -> Result<RecordMap, ConfigError> {
    let mut rec = RecordMap {
        source_sig: sig.to_string(),
        ..Default::default()
    };

    let obj = match val {
        serde_json::Value::Object(m) => m,
        _ => return Ok(rec),
    };

    // target_record_type
    if let Some(serde_json::Value::String(s)) = obj.get("target_record_type") {
        rec.target_sig = Some(s.clone());
    }

    // fields: { source_field: target_field, ... }
    if let Some(serde_json::Value::Object(fields)) = obj.get("fields") {
        for (src, tgt) in fields {
            if let serde_json::Value::String(tgt_name) = tgt {
                rec.field_rewrites.push(FieldRewrite {
                    source_field: src.clone(),
                    target_field: tgt_name.clone(),
                });
            }
        }
    }

    // transforms: { field: { type: "...", ...config } }
    if let Some(serde_json::Value::Object(transforms)) = obj.get("transforms") {
        for (field, config) in transforms {
            let transform_name = config
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            rec.transforms.push(TransformInvocation {
                field: field.clone(),
                name: transform_name,
                config: config.clone(),
            });
        }
    }

    // drop: [ field, ... ]
    if let Some(serde_json::Value::Array(drops)) = obj.get("drop") {
        for d in drops {
            if let serde_json::Value::String(s) = d {
                rec.drop_fields.push(s.clone());
            }
        }
    }

    if obj.get("delegate").and_then(serde_json::Value::as_str) == Some("fnv_legacy_scripting") {
        rec.delegate = Some(DeferredKind::FnvLegacyScripting);
    }

    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic_codes(coverage: &TranslationMapCoverage) -> std::collections::HashSet<&str> {
        coverage
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect()
    }

    #[test]
    fn strict_compile_accepts_repaired_fnv_rules_while_load_remains_permissive() {
        let maps = TranslationMaps::load(Game::Fnv, Game::Fo4).expect("legacy load remains usable");
        assert!(maps.record_map("WEAP").is_some());
        TranslationMaps::compile(Game::Fnv, Game::Fo4)
            .expect("strict compile accepts the repaired FNV map");
    }

    #[test]
    fn lint_surfaces_every_silent_rule_class() {
        let raw = serde_json::json!({
            "TEST": {
                "fields": {
                    "AAAA": "BBBB",
                    "SemanticField": "FULL"
                },
                "transforms": {
                    "CCCC": { "type": "missing_transform" },
                    "DDDD": { "type": "remap_formkey", "source_esm": "FalloutNV.esm" },
                    "EEEE": { "source_esm": "FalloutNV.esm" }
                },
                "drop": ["SemanticDrop"],
                "defaults": {},
                "edid_prefix": "nv-",
                "surprise": true
            }
        });
        let coverage = lint_map_value(Game::Fnv, Game::Fo4, "embedded:test.yaml".to_string(), &raw);
        let codes = diagnostic_codes(&coverage);
        for expected in [
            "first_only_rewrite",
            "ignored_top_level_key",
            "semantic_drop_name",
            "transform_config_failure",
            "unknown_record_key",
            "unknown_transform",
            "unresolved_semantic_field_path",
            "unsupported_transform",
        ] {
            assert!(codes.contains(expected), "missing diagnostic {expected}");
        }
        assert!(coverage.has_errors());
        assert_eq!(coverage.records[0].field_rewrites.total, 2);
        assert_eq!(coverage.records[0].field_rewrites.executable, 1);
        assert_eq!(coverage.records[0].field_rewrites.rejected, 2);
    }

    #[test]
    fn lint_rejects_an_unregistered_delegate() {
        let raw = serde_json::json!({
            "SCPT": {
                "delegate": "unregistered_pass"
            }
        });
        let coverage = lint_map_value(Game::Fnv, Game::Fo4, "embedded:test.yaml".to_string(), &raw);
        assert!(coverage.diagnostics.iter().any(|diagnostic| {
            diagnostic.path == "SCPT.delegate" && diagnostic.code == "unsupported_delegate"
        }));
    }

    #[test]
    fn fnv_and_fo3_coverage_is_complete_and_machine_readable() {
        for source in [Game::Fnv, Game::Fo3] {
            let maps = TranslationMaps::load(source, Game::Fo4).unwrap();
            let coverage = TranslationMaps::coverage(source, Game::Fo4).unwrap();
            TranslationMaps::compile(source, Game::Fo4).unwrap_or_else(|error| {
                panic!("{} strict compile failed: {error}", source.as_str())
            });
            assert!(
                coverage.diagnostics.is_empty(),
                "{} strict coverage has diagnostics: {:?}",
                source.as_str(),
                coverage.diagnostics
            );
            assert_eq!(coverage.totals.field_rewrites.rejected, 0);
            assert_eq!(coverage.totals.transforms.rejected, 0);
            assert_eq!(coverage.totals.drops.rejected, 0);
            for signature in maps.record_maps.keys().filter(|sig| is_raw_signature(sig)) {
                assert!(
                    coverage
                        .records
                        .iter()
                        .any(|record| record.signature == *signature),
                    "{} coverage missing {signature}",
                    source.as_str()
                );
            }

            let json = serde_json::to_value(&coverage).expect("coverage serializes");
            assert_eq!(json["source"], source.as_str());
            assert_eq!(json["target"], "fo4");
            assert!(
                json["records"]
                    .as_array()
                    .is_some_and(|records| !records.is_empty())
            );
            assert_eq!(json["diagnostics"].as_array().map(Vec::len), Some(0));
        }
    }

    #[test]
    fn fnv_legacy_scripting_records_compile_to_delegates() {
        let maps = TranslationMaps::compile(Game::Fnv, Game::Fo4).unwrap();
        for signature in ["SCPT", "QUST", "DIAL", "INFO", "SCEN"] {
            assert_eq!(
                maps.record_map(signature)
                    .and_then(|record| record.delegate),
                Some(DeferredKind::FnvLegacyScripting),
                "{signature} must delegate to the legacy scripting pass"
            );
        }
    }

    #[test]
    fn strict_compile_accepts_a_missing_optional_map() {
        let maps = TranslationMaps::compile(Game::Fo4, Game::Fo76).unwrap();
        assert!(maps.record_maps.is_empty());
        assert!(maps.skip_records.is_empty());
    }

    #[test]
    fn load_fo76_to_fo4_map() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let weap_map = maps.record_map("WEAP").expect("WEAP map");
        assert!(
            !weap_map.field_rewrites.is_empty() || !weap_map.transforms.is_empty(),
            "WEAP map has no field rewrites or transforms"
        );
    }

    #[test]
    fn fo76_to_fo4_drops_weapon_rgw3_instead_of_mapping_to_fnam() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let weap_map = maps.record_map("WEAP").expect("WEAP map");
        assert!(
            !weap_map
                .field_rewrites
                .iter()
                .any(|rewrite| rewrite.source_field == "RGW3"),
            "FO76 WEAP RGW3 bytes must not be decoded as an FO4 WEAP field"
        );
        assert!(
            !weap_map
                .transforms
                .iter()
                .any(|transform| transform.field == "RGW3"),
            "FO76 WEAP RGW3 must not run FO4 form-key transforms"
        );
        assert!(
            weap_map.drop_fields.iter().any(|field| field == "RGW3"),
            "FO76 WEAP RGW3 should be dropped"
        );
    }

    #[test]
    fn fo76_to_fo4_keeps_npc_qnam_skin_tone() {
        // QNAM (Texture lighting) is the NPC skin tone that tints the body to
        // match the FaceGen head. Its 4-float RGBA layout is identical FO76→FO4,
        // and the FO4 whitelist keeps it. A stale `- QNAM` in the NPC_ drop list
        // silently deleted it (drop matches the raw 4CC sig), giving every
        // converted settler a dark neck seam and mismatched eyelashes.
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let npc_map = maps.record_map("NPC_").expect("NPC_ map");
        assert!(
            !npc_map.drop_fields.iter().any(|field| field == "QNAM"),
            "FO76 NPC_ QNAM skin tone must not be dropped"
        );
    }

    #[test]
    fn fo76_to_fo4_keeps_enchantment_conditions() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let enchantment_map = maps.record_map("ENCH").expect("ENCH map");
        assert!(
            enchantment_map
                .field_rewrites
                .iter()
                .any(|rewrite| rewrite.source_field == "CTDA" && rewrite.target_field == "CTDA"),
            "FO76 ENCH conditions must be carried so equipped effects remain gated"
        );
        assert!(
            !enchantment_map
                .drop_fields
                .iter()
                .any(|field| field == "CTDA"),
            "FO76 ENCH CTDA must not be dropped"
        );
    }

    #[test]
    fn fo76_to_fo4_keeps_lvln_location_conditions() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let lvln_map = maps.record_map("LVLN").expect("LVLN map");
        assert!(
            lvln_map
                .field_rewrites
                .iter()
                .any(|rewrite| rewrite.source_field == "CTDA" && rewrite.target_field == "CTDA"),
            "FO76 LVLN conditions must reach the FO76-to-FO4 condition hook"
        );
        assert!(
            !lvln_map.drop_fields.iter().any(|field| field == "CTDA"),
            "FO76 LVLN CTDA must not be dropped"
        );
    }

    #[test]
    fn fo76_to_fo4_drops_npc_tint_layers_as_complete_groups() {
        // FO76 tint indices are only meaningful against the FO76 RACE tint
        // tables, which are not carried into FO4. Keep QNAM for the body/face
        // skin-tone match, but drop both the TETI header and TEND payload so an
        // invalid layer cannot survive and an orphan payload cannot remain.
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let npc_map = maps.record_map("NPC_").expect("NPC_ map");
        assert!(
            npc_map.drop_fields.iter().any(|field| field == "TETI"),
            "FO76 NPC_ TETI tint indices must be dropped"
        );
        assert!(
            npc_map.drop_fields.iter().any(|field| field == "TEND"),
            "FO76 NPC_ TEND tint payloads must be dropped with TETI"
        );
    }

    #[test]
    fn fo76_to_fo4_drops_race_tint_count_with_tint_tables() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let race_map = maps.record_map("RACE").expect("RACE map");
        assert!(
            race_map.drop_fields.iter().any(|field| field == "TINL"),
            "FO76 RACE TINL must be dropped with the incompatible tint tables"
        );
        assert!(
            !race_map
                .field_rewrites
                .iter()
                .any(|rewrite| rewrite.source_field == "TotalNumberOfTintsInList"),
            "FO76 RACE tint count must not be carried when its tables are dropped"
        );
        for sig in [
            "TTGP", "TETI", "TTEF", "CTDA", "CIS1", "CIS2", "TTET", "TTEB", "TTEC", "TTED", "TTGE",
            "MPGN", "MPPC", "MPPI", "MPPN", "MPPM", "MPPT", "MPPF", "MPPK", "MPGS",
        ] {
            assert!(
                race_map.drop_fields.iter().any(|field| field == sig),
                "FO76 RACE face-table subrecord {sig} must be dropped"
            );
        }
    }

    #[test]
    fn fo76_to_fo4_keeps_audited_shared_subrecords() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        for (record_sig, subrecord_sig) in [
            ("WEAP", "NNAM"),
            ("NPC_", "SPCT"),
            ("RACE", "PHWT"),
            ("RACE", "BSMP"),
            ("RACE", "BSMB"),
            ("RACE", "BSMS"),
            ("RACE", "BMMP"),
            ("RACE", "FMRI"),
            ("RACE", "FMRN"),
            ("RACE", "HEAD"),
            ("RACE", "MSID"),
            ("ARMO", "DAMA"),
            ("STAT", "PRPS"),
            ("STAT", "MODC"),
            ("SCOL", "PTRN"),
            ("SCOL", "FULL"),
            ("CNCY", "MODC"),
            ("CONT", "DATA"),
            ("KEYM", "ICON"),
            ("KEYM", "MICO"),
            ("KEYM", "KSIZ"),
            ("KEYM", "KWDA"),
            ("KEYM", "MODC"),
            ("LVLI", "LVLG"),
            ("LVLN", "LVLG"),
            ("LIGH", "MODL"),
            ("BPTD", "NAM5"),
            ("FURN", "MODC"),
            ("MESG", "DNAM"),
            ("IDLM", "IDLF"),
            ("QUST", "ALFC"),
            ("QUST", "KNAM"),
        ] {
            let map = maps.record_map(record_sig).expect("record map");
            assert!(
                !map.drop_fields.iter().any(|field| field == subrecord_sig),
                "{record_sig}.{subrecord_sig} is valid in both games and must not be dropped"
            );
        }
    }

    #[test]
    fn fo76_to_fo4_skips_story_manager_records() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        for sig in ["SMBN", "SMEN", "SMQN"] {
            assert!(
                maps.skip_records.contains(sig),
                "fo76_to_fo4 should skip {sig}"
            );
        }
    }

    #[test]
    fn fo76_to_fo4_skips_records_that_need_projected_worldspace_or_nav_writers() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        for sig in ["ACHR", "CELL", "LAND", "NAVI", "NAVM", "REFR"] {
            assert!(
                maps.skip_records.contains(sig),
                "fo76_to_fo4 should skip {sig}"
            );
        }
    }

    #[test]
    fn fo76_to_fo4_skips_collision_layers() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        assert!(maps.skip_records.contains("COLL"));
    }

    #[test]
    fn fo76_to_fo4_skips_loading_screens() {
        // FO76 loading screens hold their art in BNAM (a 2D .DDS background);
        // FO4 needs NNAM (a 3D model) plus a camera transform. With no bridge
        // between them, converted screens render blank, so the type is skipped
        // and FO4 falls back to its own loading screens.
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        assert!(
            maps.skip_records.contains("LSCR"),
            "fo76_to_fo4 should skip LSCR"
        );
    }

    #[test]
    fn fo76_to_fo4_drops_default_object_manager_singleton() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        assert!(
            maps.skip_records.contains("DOBJ"),
            "FO76 DOBJ must not replace Fallout 4's game-wide singleton"
        );
    }

    #[test]
    fn fo76_to_fo4_emits_scen_and_dlbr_by_default() {
        // SCEN/DLBR are flat top-level FO4 records emitted by the generic
        // writer (resolve NOTE\SNAM-Scene + INFO\BNAM-DLBR). The
        // MODBOX_DISABLE_SCEN env gate (maps.rs::load) is not exercised here
        // to avoid process-global env races across parallel tests.
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        assert!(
            !maps.skip_records.contains("SCEN"),
            "SCEN must be emitted by default"
        );
        assert!(
            !maps.skip_records.contains("DLBR"),
            "DLBR must be emitted by default"
        );
    }

    #[test]
    fn fo76_to_fo4_converts_currency_records_to_misc() {
        let maps = TranslationMaps::load(Game::Fo76, Game::Fo4).unwrap();
        let cncy_map = maps.record_map("CNCY").expect("CNCY map");
        assert_eq!(cncy_map.target_sig.as_deref(), Some("MISC"));
    }

    #[test]
    fn load_fnv_to_fo4_map_has_skip_records() {
        let maps = TranslationMaps::load(Game::Fnv, Game::Fo4).unwrap();
        assert!(
            !maps.skip_records.is_empty(),
            "fnv_to_fo4 should have skip_records"
        );
        assert!(
            maps.skip_records.contains("NAVI"),
            "FNV NAVI must be rebuilt with the FO4 byte layout"
        );
        for signature in ["IDLE", "IDLM", "ANIO"] {
            assert!(
                maps.skip_records.contains(signature),
                "FNV {signature} requires a dedicated FO4 behavior/HKX lowerer"
            );
        }
    }

    #[test]
    fn fo3_to_fo4_rebuilds_source_navi() {
        let maps = TranslationMaps::load(Game::Fo3, Game::Fo4).unwrap();
        for signature in ["NAVI", "NAVM"] {
            assert!(
                maps.skip_records.contains(signature),
                "FO3 {signature} must be rebuilt with the FO4 byte layout"
            );
        }
        for signature in ["IDLE", "IDLM", "ANIO"] {
            assert!(
                maps.skip_records.contains(signature),
                "FO3 {signature} requires a dedicated FO4 behavior/HKX lowerer"
            );
        }
    }

    #[test]
    fn legacy_serial_rows_are_reserved_for_serial_normalization() {
        let fnv = TranslationMaps::load(Game::Fnv, Game::Fo4).unwrap();
        let fnv_alch = fnv.record_map("ALCH").expect("FNV ALCH map");
        assert!(!fnv_alch.drop_fields.iter().any(|field| field == "EFIT"));
        assert!(fnv.record_map("PERK").is_none());
        let fnv_yaml = embedded_map_text("fnv_to_fo4");
        let wrld_yaml = fnv_yaml
            .split_once("\nWRLD:")
            .expect("FNV WRLD section")
            .1
            .split_once("\nSCPT:")
            .expect("WRLD section terminator")
            .0;
        assert!(
            !wrld_yaml.contains("defaults:"),
            "WRLD reference defaults conflict with missing-stays-missing serial normalization"
        );

        let fo3 = TranslationMaps::load(Game::Fo3, Game::Fo4).unwrap();
        let fo3_mgef = fo3.record_map("MGEF").expect("FO3 MGEF map");
        assert!(
            !fo3_mgef
                .transforms
                .iter()
                .any(|transform| matches!(transform.field.as_str(), "DATA" | "ESCE"))
        );
        let fo3_alch = fo3.record_map("ALCH").expect("FO3 ALCH map");
        assert!(
            !fo3_alch
                .transforms
                .iter()
                .any(|transform| matches!(transform.field.as_str(), "EFID" | "CTDA"))
        );
    }

    #[test]
    fn skyrimse_to_fo4_keeps_world_and_navm_records_but_rebuilds_navi() {
        let maps = TranslationMaps::load(Game::SkyrimSe, Game::Fo4).unwrap();
        for sig in [
            "WRLD", "CELL", "LAND", "NAVM", "REFR", "ACHR", "WATR", "GRAS",
        ] {
            assert!(
                !maps.skip_records.contains(sig),
                "{sig} must reach topology rebuild"
            );
        }
        assert!(
            maps.skip_records.contains("NAVI"),
            "NAVI is rebuilt from converted NAVM topology"
        );
        for sig in ["FACT", "SNDR", "MUST", "IDLE", "CPTH"] {
            let map = maps.record_map(sig).expect("condition-bearing record map");
            assert!(map.transforms.iter().any(|transform| {
                transform.field == "CTDA" && transform.name == "translate_conditions"
            }));
        }
    }

    #[test]
    fn skyrimse_to_fo4_routes_pack_records_to_the_safe_lowerer() {
        let maps = TranslationMaps::load(Game::SkyrimSe, Game::Fo4).unwrap();
        assert!(
            !maps.skip_records.contains("PACK"),
            "Skyrim packages are rebuilt as target-safe FO4 travel packages"
        );
    }

    fn assert_nonquest_signature_routes(source: Game) {
        let source_schema = crate::schema::AuthoringSchema::for_game(source.as_str()).unwrap();
        let target_schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        let maps = TranslationMaps::load(source, Game::Fo4).unwrap();
        let mut missing = Vec::new();
        let mut invalid = Vec::new();
        for source_signature in source_schema.record_signatures() {
            if is_quest_runtime_signature(source_signature)
                || maps.skip_records.contains(source_signature)
            {
                continue;
            }
            let record_map = maps.record_map(source_signature);
            if record_map.is_some_and(RecordMap::has_executable_policy) {
                continue;
            }
            let Some(route) = maps.record_route(source_signature) else {
                missing.push(source_signature.to_string());
                continue;
            };
            let adapted = adapted_nonquest_signature(source, source_signature);
            if target_schema.record_def(adapted).is_none() {
                invalid.push(format!("{source_signature}->{adapted}:missing_target"));
                continue;
            }
            let source_record = source_schema
                .record_def(source_signature)
                .expect("source signature came from schema");
            let target_record = target_schema
                .record_def(adapted)
                .expect("target signature checked above");
            let equal = record_shapes_equal(source_record, target_record);
            match route {
                RecordRoutePolicy::Compatible if !equal => invalid.push(format!(
                    "{source_signature}:compatible_but_schema_divergent"
                )),
                RecordRoutePolicy::TargetProjection if equal => invalid.push(format!(
                    "{source_signature}:target_projection_but_schema_identical"
                )),
                _ => {}
            }
        }
        missing.sort();
        invalid.sort();
        assert!(
            missing.is_empty(),
            "{} non-quest signatures have no explicit FO4 shape policy: {}",
            source.as_str(),
            missing.join(", ")
        );
        assert!(
            invalid.is_empty(),
            "{} non-quest shape policies are invalid: {}",
            source.as_str(),
            invalid.join(", ")
        );
    }

    fn adapted_nonquest_signature(source: Game, source_signature: &str) -> &str {
        match (source, source_signature) {
            (Game::Fnv | Game::Fo3, "ACRE") => "ACHR",
            (Game::Fnv | Game::Fo3, "CREA") => "NPC_",
            (Game::Fo3, "LVLC") => "LVLN",
            (Game::SkyrimSe, "SCRL") => "ALCH",
            (Game::SkyrimSe, "SHOU") => "SPEL",
            _ => source_signature,
        }
    }

    fn record_shapes_equal(
        source: &crate::schema::RecordDef,
        target: &crate::schema::RecordDef,
    ) -> bool {
        source.id == target.id
            && source.subrecords.len() == target.subrecords.len()
            && source
                .subrecords
                .iter()
                .zip(&target.subrecords)
                .all(|(source, target)| {
                    source.id == target.id
                        && source.kind == target.kind
                        && source.codec == target.codec
                        && source.multiple == target.multiple
                        && source.scope_id == target.scope_id
                        && source.required == target.required
                        && source.localized == target.localized
                        && field_shapes_equal(&source.fields, &target.fields)
                        && field_shapes_equal(&source.union_variants, &target.union_variants)
                })
    }

    fn field_shapes_equal(
        source: &[crate::schema::FieldDef],
        target: &[crate::schema::FieldDef],
    ) -> bool {
        source.len() == target.len()
            && source.iter().zip(target).all(|(source, target)| {
                source.id == target.id
                    && source.kind == target.kind
                    && source.codec == target.codec
                    && field_shapes_equal(&source.fields, &target.fields)
                    && field_shapes_equal(&source.union_variants, &target.union_variants)
            })
    }

    #[test]
    fn every_legacy_fallout_and_skyrim_nonquest_signature_has_a_declared_route() {
        for source in [Game::Fnv, Game::Fo3, Game::SkyrimSe] {
            assert_nonquest_signature_routes(source);
        }
    }

    #[test]
    fn schema_divergent_record_cannot_fall_back_to_same_signature_passthrough() {
        let source_schema = crate::schema::AuthoringSchema::for_game("fnv").unwrap();
        let target_schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        assert!(!record_shapes_equal(
            source_schema.record_def("ASPC").unwrap(),
            target_schema.record_def("ASPC").unwrap(),
        ));

        let mut maps = TranslationMaps::load(Game::Fnv, Game::Fo4).unwrap();
        assert_eq!(
            maps.record_routes.remove("ASPC"),
            Some(RecordRoutePolicy::TargetProjection)
        );
        assert!(maps.record_map("ASPC").is_none());
        assert!(maps.record_route("ASPC").is_none());
    }

    #[test]
    #[ignore = "requires explicit official FNV, FO3, and Skyrim SE Data directories"]
    fn live_official_nonquest_shape_census_has_no_unclassified_records() {
        use crate::ids::SigCode;
        use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
        use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
        use crate::translator::{
            LEGACY_REQUIRED_FO4_PROJECTION_FIELDS, TranslateResult, Translator,
        };
        use esp_authoring_core::plugin_runtime::{
            ParsedItem, plugin_handle_close_native, plugin_handle_load_no_py,
            plugin_handle_store_ref,
        };
        use std::collections::{BTreeMap, BTreeSet};
        use std::path::PathBuf;

        fn collect_shapes(items: &[ParsedItem], shapes: &mut BTreeSet<(String, Vec<String>)>) {
            for item in items {
                match item {
                    ParsedItem::Record(record) => {
                        shapes.insert((
                            record.signature.as_str().to_string(),
                            record
                                .subrecords
                                .iter()
                                .map(|subrecord| subrecord.signature.as_str().to_string())
                                .collect(),
                        ));
                    }
                    ParsedItem::Group(group) => collect_shapes(&group.children, shapes),
                }
            }
        }

        #[derive(Debug, Default)]
        struct ProjectionCounts {
            encountered: usize,
            lowered: usize,
            failed: usize,
        }

        fn expected_projection_receipt(game: Game, signature: &str) -> Option<&'static str> {
            match (game, signature) {
                (Game::Fnv | Game::Fo3, "IMAD") => Some("legacy_imad_neutral_target_replacement"),
                (Game::SkyrimSe, "BPTD") => Some("skyrim_bptd_target_tail_degraded"),
                (Game::SkyrimSe, "HAZD") => Some("skyrim_hazd_spell_effect_omitted"),
                _ => None,
            }
        }

        fn regn_scopes_have_lod_distance(record: &crate::record::Record) -> bool {
            let mut in_region_data = false;
            let mut has_lod_distance = false;
            for field in &record.fields {
                if field.sig.as_str() == "RDAT" {
                    if in_region_data && !has_lod_distance {
                        return false;
                    }
                    in_region_data = true;
                    has_lod_distance = false;
                } else if in_region_data && field.sig.as_str() == "RLDM" {
                    has_lod_distance = true;
                }
            }
            !in_region_data || has_lod_distance
        }

        let cases: &[(Game, &str, &[&str])] = &[
            (
                Game::Fnv,
                "BACUP_NONQUEST_FNV_DATA_DIR",
                &[
                    "FalloutNV.esm",
                    "DeadMoney.esm",
                    "HonestHearts.esm",
                    "OldWorldBlues.esm",
                    "LonesomeRoad.esm",
                    "GunRunnersArsenal.esm",
                    "ClassicPack.esm",
                    "MercenaryPack.esm",
                    "TribalPack.esm",
                    "CaravanPack.esm",
                ],
            ),
            (
                Game::Fo3,
                "BACUP_NONQUEST_FO3_DATA_DIR",
                &[
                    "Fallout3.esm",
                    "Anchorage.esm",
                    "ThePitt.esm",
                    "BrokenSteel.esm",
                    "PointLookout.esm",
                    "Zeta.esm",
                ],
            ),
            (
                Game::SkyrimSe,
                "BACUP_NONQUEST_SKYRIMSE_DATA_DIR",
                &[
                    "Skyrim.esm",
                    "Update.esm",
                    "Dawnguard.esm",
                    "HearthFires.esm",
                    "Dragonborn.esm",
                ],
            ),
        ];
        let interner = crate::sym::StringInterner::new();

        for (game, data_env, plugins) in cases {
            let data_dir =
                PathBuf::from(std::env::var(data_env).unwrap_or_else(|_| {
                    panic!("{data_env} must name the official Data directory")
                }));
            let schema = crate::schema::AuthoringSchema::for_game(game.as_str()).unwrap();
            let target_schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
            let maps = TranslationMaps::load(*game, Game::Fo4).unwrap();
            let translator = Translator::new(*game, Game::Fo4).unwrap();
            let normalizer =
                TargetRecordNormalizer::target_only_with_interner(&target_schema, &interner);
            let mut route_records = BTreeMap::<String, usize>::new();
            let mut blocked_records = BTreeMap::<String, usize>::new();
            let mut encountered_shapes = BTreeSet::<(String, Vec<String>)>::new();
            let mut unclassified = BTreeMap::<String, usize>::new();
            let mut projection_counts = BTreeMap::<String, ProjectionCounts>::new();
            let mut projection_failures = BTreeMap::<String, usize>::new();
            for &(_, signature, _) in LEGACY_REQUIRED_FO4_PROJECTION_FIELDS
                .iter()
                .filter(|(source, _, _)| source == game)
            {
                assert_eq!(
                    maps.record_route(signature),
                    Some(RecordRoutePolicy::TargetProjection),
                    "{} {signature} must retain an explicit target-projection route",
                    game.as_str()
                );
                projection_counts.insert(signature.to_string(), ProjectionCounts::default());
            }

            for plugin in *plugins {
                let path = data_dir.join(plugin);
                assert!(
                    path.is_file(),
                    "official source is missing: {}",
                    path.display()
                );
                let strings_dir = data_dir.join("Strings");
                let handle = plugin_handle_load_no_py(
                    path.to_str().expect("Unicode plugin path"),
                    Some(game.as_str()),
                    strings_dir
                        .is_dir()
                        .then(|| strings_dir.to_string_lossy())
                        .as_deref(),
                    None,
                    true,
                )
                .unwrap_or_else(|error| panic!("load {}: {error}", path.display()));

                for signature in schema.record_signatures() {
                    let sig = SigCode::from_str(signature).unwrap();
                    let keys = iter_form_keys_of_sig(handle, sig, &interner)
                        .unwrap_or_else(|error| panic!("index {plugin} {signature}: {error}"));
                    if keys.is_empty() {
                        continue;
                    }
                    let route = if is_quest_runtime_signature(signature) {
                        "quest_runtime_boundary"
                    } else if maps.skip_records.contains(signature) {
                        "intentional_omit"
                    } else if maps
                        .record_map(signature)
                        .is_some_and(RecordMap::has_executable_policy)
                    {
                        "record_map"
                    } else {
                        match maps.record_route(signature) {
                            Some(RecordRoutePolicy::Compatible) => "compatible",
                            Some(RecordRoutePolicy::TargetProjection) => "target_projection",
                            Some(RecordRoutePolicy::PairHook) => "pair_hook",
                            Some(RecordRoutePolicy::TopologyRebuild) => "topology_rebuild",
                            Some(RecordRoutePolicy::Blocked) => "blocked",
                            None => {
                                *unclassified.entry(signature.to_string()).or_default() +=
                                    keys.len();
                                "unclassified"
                            }
                        }
                    };
                    if route == "blocked" {
                        *blocked_records.entry(signature.to_string()).or_default() += keys.len();
                    }
                    *route_records.entry(route.to_string()).or_default() += keys.len();

                    let Some((_, _, required_fields)) = LEGACY_REQUIRED_FO4_PROJECTION_FIELDS
                        .iter()
                        .find(|(source, required_signature, _)| {
                            source == game && *required_signature == signature
                        })
                    else {
                        continue;
                    };
                    for key in &keys {
                        projection_counts
                            .get_mut(signature)
                            .expect("initialized projection census")
                            .encountered += 1;
                        let source_record = match read_record_relayout_by_form_key(
                            handle, key, &schema, &interner, None,
                        ) {
                            Ok(record) => record,
                            Err(_) => {
                                projection_counts.get_mut(signature).unwrap().failed += 1;
                                *projection_failures
                                    .entry(format!("{signature}:decode_error"))
                                    .or_default() += 1;
                                continue;
                            }
                        };
                        let translated = match translator.translate(&source_record, &interner) {
                            TranslateResult::Translated(record) => record,
                            TranslateResult::Dropped { decision, .. } => {
                                let reason = interner
                                    .resolve(decision.kind)
                                    .unwrap_or("unresolved_drop_reason");
                                if reason == "unused_legacy_ingredient_sentinel" {
                                    projection_counts.get_mut(signature).unwrap().lowered += 1;
                                    continue;
                                }
                                projection_counts.get_mut(signature).unwrap().failed += 1;
                                *projection_failures
                                    .entry(format!("{signature}:dropped:{reason}"))
                                    .or_default() += 1;
                                continue;
                            }
                            TranslateResult::Deferred(kind) => {
                                projection_counts.get_mut(signature).unwrap().failed += 1;
                                *projection_failures
                                    .entry(format!("{signature}:deferred:{kind:?}"))
                                    .or_default() += 1;
                                continue;
                            }
                        };
                        if let Some(receipt) = expected_projection_receipt(*game, signature)
                            && !translated
                                .warnings
                                .iter()
                                .any(|warning| interner.resolve(*warning) == Some(receipt))
                        {
                            projection_counts.get_mut(signature).unwrap().failed += 1;
                            *projection_failures
                                .entry(format!("{signature}:missing_receipt:{receipt}"))
                                .or_default() += 1;
                            continue;
                        }
                        let TargetRecordNormalization::Keep(normalized) =
                            normalizer.normalize(translated)
                        else {
                            projection_counts.get_mut(signature).unwrap().failed += 1;
                            *projection_failures
                                .entry(format!("{signature}:target_normalizer_dropped"))
                                .or_default() += 1;
                            continue;
                        };
                        let missing =
                            if signature == "REGN" && !regn_scopes_have_lod_distance(&normalized) {
                                Some("RLDM")
                            } else {
                                required_fields.iter().copied().find(|required| {
                                    !normalized
                                        .fields
                                        .iter()
                                        .any(|field| field.sig.as_str() == *required)
                                        && !(signature == "REGN"
                                            && *required == "RLDM"
                                            && !normalized
                                                .fields
                                                .iter()
                                                .any(|field| field.sig.as_str() == "RDAT"))
                                })
                            };
                        if let Some(required) = missing {
                            projection_counts.get_mut(signature).unwrap().failed += 1;
                            *projection_failures
                                .entry(format!("{signature}:missing_required:{required}"))
                                .or_default() += 1;
                            continue;
                        }
                        projection_counts.get_mut(signature).unwrap().lowered += 1;
                    }
                }
                {
                    let store = plugin_handle_store_ref().lock().unwrap();
                    let slot = store.get(&handle).expect("loaded plugin handle");
                    collect_shapes(&slot.parsed.root_items, &mut encountered_shapes);
                }
                assert!(
                    plugin_handle_close_native(handle),
                    "close {}",
                    path.display()
                );
            }

            eprintln!(
                "{} official nonquest census: records={route_records:?}, blocked={blocked_records:?}, unique_subrecord_shapes={}",
                game.as_str(),
                encountered_shapes.len()
            );
            eprintln!(
                "{} official required projection census: {projection_counts:?}, failures={projection_failures:?}",
                game.as_str()
            );
            assert!(
                unclassified.is_empty(),
                "{} official corpus has unclassified records: {unclassified:?}",
                game.as_str()
            );
            assert!(
                projection_counts
                    .values()
                    .all(|counts| counts.failed == 0 && counts.lowered == counts.encountered),
                "{} official required projection failures: counts={projection_counts:?}, reasons={projection_failures:?}",
                game.as_str()
            );
        }
    }

    #[test]
    fn legacy_fo4_routes_only_omit_declared_source_only_records() {
        let cases = [
            (
                Game::Fnv,
                &[
                    "ALOC", "AMEF", "ANIO", "CDCK", "CHAL", "CSNO", "DEHY", "HAIR", "HUNG", "IDLE",
                    "IDLM", "LSCT", "MICN", "MSET", "NAVI", "NAVM", "RADS", "REPU", "RGDL", "SLPD",
                ][..],
            ),
            (
                Game::Fo3,
                &[
                    "ANIO", "HAIR", "IDLE", "IDLM", "MICN", "NAVI", "NAVM", "RADS", "RGDL", "SCPT",
                ][..],
            ),
            (
                Game::SkyrimSe,
                &["CLDC", "COLL", "HAIR", "NAVI", "RGDL", "SCPT", "VOLI"][..],
            ),
        ];
        for (source, expected) in cases {
            let maps = TranslationMaps::load(source, Game::Fo4).unwrap();
            let mut actual = maps
                .skip_records
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            actual.sort_unstable();
            let mut expected = expected.to_vec();
            expected.sort_unstable();
            assert_eq!(
                actual,
                expected,
                "{} omission policy drifted",
                source.as_str()
            );
        }
    }

    #[test]
    fn legacy_persistent_water_routes_through_fo4_reference_topology() {
        for source in [Game::Fnv, Game::Fo3, Game::SkyrimSe] {
            let maps = TranslationMaps::load(source, Game::Fo4).unwrap();
            assert_eq!(
                maps.record_map("PWAT")
                    .and_then(|map| map.target_sig.as_deref()),
                Some("REFR"),
                "{} PWAT records must retain source group topology as FO4 REFR records",
                source.as_str()
            );
        }
    }

    #[test]
    fn fnv_misc_items_retain_value_and_weight_data() {
        let maps = TranslationMaps::load(Game::Fnv, Game::Fo4).unwrap();
        for signature in ["MISC", "KEYM"] {
            let map = maps.record_map(signature).expect("legacy item map");
            assert!(
                !map.drop_fields.iter().any(|field| field == "DATA"),
                "{signature}.DATA is the compatible value/weight payload"
            );
        }
    }

    #[test]
    fn missing_map_returns_empty() {
        // No map file for this pair should exist.
        let maps = TranslationMaps::load(Game::Fo4, Game::Fo76).unwrap();
        assert!(maps.record_map("WEAP").is_none());
        assert!(maps.skip_records.is_empty());
    }
}
