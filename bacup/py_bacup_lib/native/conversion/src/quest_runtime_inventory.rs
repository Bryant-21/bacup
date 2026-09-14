use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use esp_authoring_core::plugin_runtime::effective_subrecords_for_record;
use papyrus_core::pex::{
    PexFilePayload, PexFunctionPayload, PexInstructionPayload, PexObjectPayload, PexValuePayload,
    parse_pex_file,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::phase::story_manager::{StoryManagerRouteSeed, StoryManagerRouteSeedReport};
use crate::record::{FieldValue, Record};
use crate::run::ConversionRun;
use crate::session::{PluginSession, SessionError, open_session};
use crate::target_assets::TargetAssetStore;

const SCHEMA_VERSION: u32 = 1;
const QUST_FLAG_START_GAME_ENABLED: u16 = 0x0001;
const QUST_FLAG_STARTS_ENABLED: u16 = 0x0010;
const OP_JMP: u8 = 0x14;
const OP_JMPT: u8 = 0x15;
const OP_JMPF: u8 = 0x16;
const OP_CALLMETHOD: u8 = 0x17;
const OP_CALLPARENT: u8 = 0x18;
const OP_CALLSTATIC: u8 = 0x19;
const OP_RETURN: u8 = 0x1A;
const OP_STRUCTGET: u8 = 0x26;

const NATIVE_ENGINE_EVENTS: &[&str] = &[
    "ADIA", "CLOC", "HACK", "KILL", "LCLD", "LEVL", "LOCK", "REMP", "TMEE",
];
const PLACED_SIGNATURES: &[&str] = &["ACHR", "PARW", "PBEA", "PGRE", "PHZD", "PMIS", "REFR"];
const UNPROVEN_RUNTIME_SIGNATURES: &[&str] = &["INFO", "QUST", "SCEN"];
const SUPPORTED_ENTRYPOINTS: &[&str] = &[
    "onactivate",
    "onequipped",
    "oninit",
    "onlocationchange",
    "onmenuitemrun",
    "onquestinit",
    "ontriggerenter",
];
const TERMINAL_MENU_STORY_SENDER: &str = "defaultsendstoryeventonmenuitemrun";
const TERMINAL_MENU_DATA_PROPERTY: &str = "menudata";
const TERMINAL_MENU_DATA_TYPE: &str = "DefaultSendStoryEventOnMenuItemRun#MenuDatum[]";
const TERMINAL_MENU_DATUM_TYPE: &str = "DefaultSendStoryEventOnMenuItemRun#MenuDatum";
const TERMINAL_MENU_STORY_MEMBER: &str = "StoryEventToSend";
const ADAPTER_SCRIPTS: &[&str] = &[
    "b21:storyeventonactivatestartscene",
    "b21:storyeventontriggerenter",
];
const KNOWN_UNSUPPORTED_REASONS: &[&str] = &[
    "online_service_unavailable",
    "server_authoritative_state",
    "multiplayer_instance_semantics",
    "source_producer_absent",
];

#[derive(Debug, Clone)]
pub struct QuestRuntimeInventoryOptions {
    pub source_pex_roots: Vec<PathBuf>,
    pub target_pex_roots: Vec<PathBuf>,
    pub patch_source_root: PathBuf,
    pub addition_scripts: Vec<String>,
    pub unsupported_manifest: PathBuf,
    pub script_conversion_enabled: bool,
}

#[derive(Debug, Default)]
struct QuestInventoryMeasurements {
    total: Duration,
    manifest: Duration,
    query_build: Duration,
    session_open: Duration,
    source_discovery: Duration,
    target_discovery: Duration,
    source_evidence: Duration,
    target_evidence: Duration,
    route_classification: Duration,
    report_assembly: Duration,
    seed_routes: usize,
    source_queries: usize,
    target_queries: usize,
    source_carriers: usize,
    target_carriers: usize,
    source_records: usize,
    target_records: usize,
    source_vmads: usize,
    target_vmads: usize,
    source_evidence_detail: EvidenceCollectionMeasurements,
    target_evidence_detail: EvidenceCollectionMeasurements,
    pex: PexScanMeasurements,
}

#[derive(Debug, Default)]
struct EvidenceCollectionMeasurements {
    metadata: Duration,
    placement: Duration,
    snapshots: Duration,
    placement_referrers: usize,
    signature_cache_hits: usize,
    signature_cache_misses: usize,
    records_with_vmad: usize,
}

#[derive(Debug, Default)]
struct PexScanMeasurements {
    duration: Duration,
    requests: usize,
    cache_hits: usize,
    found: usize,
    parsed: usize,
    errors: usize,
}

impl QuestInventoryMeasurements {
    fn emit(&self, succeeded: bool) {
        eprintln!(
            "quest_inventory_timing succeeded={succeeded} total_ms={} manifest_ms={} query_build_ms={} session_open_ms={} source_discovery_ms={} target_discovery_ms={} source_evidence_ms={} target_evidence_ms={} source_evidence_metadata_ms={} target_evidence_metadata_ms={} source_evidence_placement_ms={} target_evidence_placement_ms={} source_evidence_snapshots_ms={} target_evidence_snapshots_ms={} source_placement_referrers={} target_placement_referrers={} source_signature_cache_hits={} target_signature_cache_hits={} source_signature_cache_misses={} target_signature_cache_misses={} source_records_with_vmad={} target_records_with_vmad={} pex_scan_nested_ms={} route_classification_ms={} report_assembly_ms={} seed_routes={} source_queries={} target_queries={} source_carriers={} target_carriers={} source_records={} target_records={} source_vmads={} target_vmads={} pex_requests={} pex_cache_hits={} pex_found={} pex_parsed={} pex_errors={}",
            self.total.as_millis(),
            self.manifest.as_millis(),
            self.query_build.as_millis(),
            self.session_open.as_millis(),
            self.source_discovery.as_millis(),
            self.target_discovery.as_millis(),
            self.source_evidence.as_millis(),
            self.target_evidence.as_millis(),
            self.source_evidence_detail.metadata.as_millis(),
            self.target_evidence_detail.metadata.as_millis(),
            self.source_evidence_detail.placement.as_millis(),
            self.target_evidence_detail.placement.as_millis(),
            self.source_evidence_detail.snapshots.as_millis(),
            self.target_evidence_detail.snapshots.as_millis(),
            self.source_evidence_detail.placement_referrers,
            self.target_evidence_detail.placement_referrers,
            self.source_evidence_detail.signature_cache_hits,
            self.target_evidence_detail.signature_cache_hits,
            self.source_evidence_detail.signature_cache_misses,
            self.target_evidence_detail.signature_cache_misses,
            self.source_evidence_detail.records_with_vmad,
            self.target_evidence_detail.records_with_vmad,
            self.pex.duration.as_millis(),
            self.route_classification.as_millis(),
            self.report_assembly.as_millis(),
            self.seed_routes,
            self.source_queries,
            self.target_queries,
            self.source_carriers,
            self.target_carriers,
            self.source_records,
            self.target_records,
            self.source_vmads,
            self.target_vmads,
            self.pex.requests,
            self.pex.cache_hits,
            self.pex.found,
            self.pex.parsed,
            self.pex.errors,
        );
    }
}

impl PexScanMeasurements {
    fn absorb(&mut self, other: &Self) {
        self.duration += other.duration;
        self.requests += other.requests;
        self.cache_hits += other.cache_hits;
        self.found += other.found;
        self.parsed += other.parsed;
        self.errors += other.errors;
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct QuestRuntimeInventorySummary {
    pub total: usize,
    pub native: usize,
    pub adapted: usize,
    pub explicitly_unsupported: usize,
    pub unclassified: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct QuestRuntimeInventoryReport {
    pub schema_version: u32,
    pub source_game: String,
    pub target_game: String,
    pub summary: QuestRuntimeInventorySummary,
    pub coverage_complete: bool,
    pub routes: Vec<InventoryRoute>,
    pub unclassified_route_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory_error: Option<String>,
}

const QUEST_SHAPE_SIGNATURES: &[&str] = &[
    "QUST", "DIAL", "INFO", "SCPT", "PACK", "SCEN", "SMEN", "SMBN", "SMQN", "DLVW", "DLBR",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuestRecordShapeCount {
    pub signature: String,
    pub fingerprint: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledQuestShapeInventoryReport {
    pub schema_version: u32,
    pub source_game: String,
    pub plugins: Vec<String>,
    pub records_scanned: usize,
    pub decode_failures: BTreeMap<String, usize>,
    pub shapes: Vec<QuestRecordShapeCount>,
}

pub fn inventory_quest_record_shapes(
    source_game: &str,
    plugins: Vec<String>,
    records: &[Record],
    decode_failures: BTreeMap<String, usize>,
    interner: &crate::sym::StringInterner,
) -> InstalledQuestShapeInventoryReport {
    let mut counts = BTreeMap::<(String, String), usize>::new();
    for record in records.iter().filter(|record| {
        QUEST_SHAPE_SIGNATURES
            .iter()
            .any(|signature| record.sig.as_str() == *signature)
    }) {
        *counts
            .entry((
                record.sig.as_str().to_string(),
                quest_record_shape_fingerprint(record, interner),
            ))
            .or_default() += 1;
    }
    let shapes = counts
        .into_iter()
        .map(|((signature, fingerprint), count)| QuestRecordShapeCount {
            signature,
            fingerprint,
            count,
        })
        .collect::<Vec<_>>();
    InstalledQuestShapeInventoryReport {
        schema_version: 1,
        source_game: source_game.to_ascii_lowercase(),
        plugins,
        records_scanned: records.len(),
        decode_failures,
        shapes,
    }
}

pub fn inventory_installed_quest_plugins(
    source_game: &str,
    plugin_paths: &[PathBuf],
) -> Result<InstalledQuestShapeInventoryReport, String> {
    let schema = crate::schema::AuthoringSchema::for_game(source_game)?;
    let interner = crate::sym::StringInterner::new();
    let mut records = Vec::new();
    let mut plugins = Vec::new();
    let mut decode_failures = BTreeMap::new();
    for plugin_path in plugin_paths {
        let plugin_name = plugin_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "quest shape inventory path has no plugin name: {}",
                    plugin_path.display()
                )
            })?
            .to_string();
        let handle = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            plugin_path.to_str().ok_or_else(|| {
                format!(
                    "quest shape inventory path is not UTF-8: {}",
                    plugin_path.display()
                )
            })?,
            Some(source_game),
            None,
            None,
            true,
        )
        .map_err(|error| format!("load quest shape inventory plugin {plugin_name}: {error}"))?;
        let result = (|| {
            for signature in QUEST_SHAPE_SIGNATURES {
                let sig = crate::ids::SigCode::from_str(signature)
                    .map_err(|error| format!("quest inventory signature {signature}: {error}"))?;
                for form_key in crate::source_read::iter_form_keys_of_sig(handle, sig, &interner)
                    .map_err(|error| format!("inventory {plugin_name} {signature}: {error}"))?
                {
                    match crate::source_read::read_record_relayout_by_form_key(
                        handle, &form_key, &schema, &interner, None,
                    ) {
                        Ok(record) => records.push(record),
                        Err(_) => {
                            *decode_failures
                                .entry(format!("{plugin_name}:{signature}"))
                                .or_default() += 1;
                        }
                    }
                }
            }
            Ok::<_, String>(())
        })();
        esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle);
        result?;
        plugins.push(plugin_name);
    }
    Ok(inventory_quest_record_shapes(
        source_game,
        plugins,
        &records,
        decode_failures,
        &interner,
    ))
}

fn quest_record_shape_fingerprint(
    record: &Record,
    interner: &crate::sym::StringInterner,
) -> String {
    let mut fields = BTreeMap::<String, usize>::new();
    for field in &record.fields {
        let token = format!(
            "{}:{}",
            field.sig.as_str(),
            quest_field_shape(&field.value, interner)
        );
        *fields.entry(token).or_default() += 1;
    }
    fields
        .into_iter()
        .map(|(field, count)| format!("{field}x{count}"))
        .collect::<Vec<_>>()
        .join("|")
}

fn quest_field_shape(value: &FieldValue, interner: &crate::sym::StringInterner) -> String {
    match value {
        FieldValue::None => "none".to_string(),
        FieldValue::Bool(_) => "bool".to_string(),
        FieldValue::Int(_) => "int".to_string(),
        FieldValue::Uint(_) => "uint".to_string(),
        FieldValue::Float(_) => "float".to_string(),
        FieldValue::String(_) => "string".to_string(),
        FieldValue::Bytes(bytes) => format!("bytes{}", bytes.len()),
        FieldValue::FormKey(_) => "formkey".to_string(),
        FieldValue::List(values) => format!(
            "list[{}]",
            values
                .iter()
                .map(|value| quest_field_shape(value, interner))
                .collect::<Vec<_>>()
                .join(",")
        ),
        FieldValue::Struct(fields) => format!(
            "struct{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!(
                    "{}:{}",
                    interner.resolve(*name).unwrap_or("<unresolved>"),
                    quest_field_shape(value, interner)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

impl QuestRuntimeInventoryReport {
    pub fn failure(seed: &StoryManagerRouteSeedReport, message: impl Into<String>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            source_game: seed.source_game.clone(),
            target_game: seed.target_game.clone(),
            summary: QuestRuntimeInventorySummary {
                total: 0,
                native: 0,
                adapted: 0,
                explicitly_unsupported: 0,
                unclassified: 0,
            },
            coverage_complete: false,
            routes: Vec::new(),
            unclassified_route_ids: Vec::new(),
            inventory_error: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InventoryRoute {
    #[serde(flatten)]
    pub route: StoryManagerRouteSeed,
    pub classification: String,
    pub runtime_proof: Vec<RuntimeProof>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported_evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeProof {
    pub scope: EvidenceScope,
    pub carrier: String,
    pub carrier_signature: String,
    pub script_name: String,
    pub keyword_property: String,
    pub entrypoint: String,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceScope {
    Source,
    Target,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Source,
    Converted,
    PersistentPatch,
    FullScriptAddition,
    PairAdapter,
}

#[derive(Debug, Clone)]
struct ProducerEvidence {
    scope: EvidenceScope,
    carrier: String,
    carrier_signature: String,
    script_name: String,
    property_bindings: BTreeMap<String, Vec<PropertyFormBinding>>,
    bound_quest_fragments: HashSet<String>,
    pex: Option<Arc<PexFilePayload>>,
    placed: bool,
    provenance: Provenance,
    autostart: Option<bool>,
    local_source: bool,
    runtime_issue: Option<String>,
    pex_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PropertyFormBinding {
    form_key: String,
    struct_member: Option<String>,
}

#[derive(Debug)]
struct ProducerProof<'a> {
    evidence: &'a ProducerEvidence,
    property_name: Option<String>,
    entrypoint: Option<String>,
    sends_story_event: bool,
    starts_quest_directly: bool,
}

impl ProducerProof<'_> {
    fn qualifies(&self) -> bool {
        self.property_name.is_some()
            && self.entrypoint.is_some()
            && self.sends_story_event
            && !self.starts_quest_directly
            && self.evidence.autostart == Some(false)
            && self.has_runtime_entry()
            && self.evidence.pex.is_some()
            && (self.evidence.scope != EvidenceScope::Source || self.evidence.local_source)
    }

    fn has_runtime_entry(&self) -> bool {
        self.entrypoint.as_deref().is_some_and(|entrypoint| {
            if entrypoint.eq_ignore_ascii_case("OnEquipped") {
                return self.evidence.carrier_signature.eq_ignore_ascii_case("BOOK");
            }
            if entrypoint.eq_ignore_ascii_case("OnMenuItemRun") {
                return self.evidence.carrier_signature.eq_ignore_ascii_case("TERM")
                    && self.evidence.placed;
            }
            self.evidence.placed
                || (entrypoint.starts_with("Fragment_")
                    && self.evidence.carrier_signature.eq_ignore_ascii_case("QUST")
                    && self
                        .evidence
                        .bound_quest_fragments
                        .contains(&entrypoint.to_ascii_lowercase()))
        })
    }

    fn payload(&self) -> Option<RuntimeProof> {
        Some(RuntimeProof {
            scope: self.evidence.scope,
            carrier: self.evidence.carrier.clone(),
            carrier_signature: self.evidence.carrier_signature.clone(),
            script_name: self.evidence.script_name.clone(),
            keyword_property: self.property_name.clone()?,
            entrypoint: self.entrypoint.clone()?,
            provenance: self.evidence.provenance,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct FormKeyIdentity {
    local: u32,
    plugin_lower: String,
}

impl FormKeyIdentity {
    fn parse(value: &str) -> Result<Self, String> {
        let text = value.trim();
        let (hex, plugin) = if let Some((hex, plugin)) = text.split_once('@') {
            (hex, plugin)
        } else if let Some((left, right)) = text.rsplit_once(':') {
            if is_hex_local(right) {
                (right, left)
            } else if is_hex_local(left) {
                (left, right)
            } else {
                return Err(format!("invalid FormKey {value:?}"));
            }
        } else {
            return Err(format!("invalid FormKey {value:?}"));
        };
        if plugin.trim().is_empty() || !is_hex_local(hex) {
            return Err(format!("invalid FormKey {value:?}"));
        }
        Ok(Self {
            local: u32::from_str_radix(hex.trim(), 16)
                .map_err(|error| format!("invalid FormKey {value:?}: {error}"))?
                & 0x00FF_FFFF,
            plugin_lower: plugin.trim().to_ascii_lowercase(),
        })
    }
}

fn is_hex_local(value: &str) -> bool {
    !value.trim().is_empty()
        && value.trim().len() <= 8
        && value.trim().bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn normalize_script_name(value: &str) -> String {
    value
        .replace(['\\', '/'], ":")
        .trim_matches(':')
        .to_ascii_lowercase()
}

fn normalize_papyrus_identifier(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn form_keys_from_reference(
    value: &Value,
    struct_member: Option<&str>,
    found: &mut BTreeSet<PropertyFormBinding>,
) {
    match value {
        Value::Object(object) => {
            if let Some(reference) = object.get("reference").and_then(Value::as_object) {
                let plugin = reference.get("plugin").and_then(Value::as_str);
                let object_id = reference.get("object_id").and_then(Value::as_str);
                if let (Some(plugin), Some(object_id)) = (plugin, object_id) {
                    let plugin = plugin.trim();
                    let object_id = object_id.trim();
                    if !plugin.is_empty()
                        && is_hex_local(object_id)
                        && let Ok(local) = u32::from_str_radix(object_id, 16)
                    {
                        found.insert(PropertyFormBinding {
                            form_key: format!("{:06X}@{}", local & 0x00FF_FFFF, plugin),
                            struct_member: struct_member.map(str::to_string),
                        });
                    }
                }
            }
            let nested_member = object
                .get("memberName")
                .and_then(Value::as_str)
                .or(struct_member);
            object
                .values()
                .for_each(|nested| form_keys_from_reference(nested, nested_member, found));
        }
        Value::Array(values) => values
            .iter()
            .for_each(|nested| form_keys_from_reference(nested, struct_member, found)),
        _ => {}
    }
}

fn vmad_script_bindings(
    payload: &Value,
) -> Vec<(String, BTreeMap<String, Vec<PropertyFormBinding>>)> {
    fn visit(value: &Value, found: &mut Vec<(String, BTreeMap<String, Vec<PropertyFormBinding>>)>) {
        match value {
            Value::Object(object) => {
                if let Some(script_name) = object.get("ScriptName").and_then(Value::as_str) {
                    let mut bindings = BTreeMap::new();
                    if let Some(properties) = object.get("Properties").and_then(Value::as_array) {
                        for property in properties {
                            let Some(property) = property.as_object() else {
                                continue;
                            };
                            let Some(name) = property.get("propertyName").and_then(Value::as_str)
                            else {
                                continue;
                            };
                            if let Some(value) = property.get("Value") {
                                let mut property_bindings = BTreeSet::new();
                                form_keys_from_reference(value, None, &mut property_bindings);
                                if !property_bindings.is_empty() {
                                    bindings.insert(
                                        name.to_string(),
                                        property_bindings.into_iter().collect(),
                                    );
                                }
                            }
                        }
                    }
                    found.push((script_name.to_string(), bindings));
                }
                object.values().for_each(|nested| visit(nested, found));
            }
            Value::Array(values) => values.iter().for_each(|nested| visit(nested, found)),
            _ => {}
        }
    }

    let mut found = Vec::new();
    visit(payload, &mut found);
    let mut unique = BTreeMap::new();
    for (script_name, bindings) in found {
        let key = format!(
            "{}\u{1f}{}",
            normalize_script_name(&script_name),
            bindings
                .iter()
                .map(|(name, values)| {
                    format!(
                        "{}={}",
                        name.to_ascii_lowercase(),
                        values
                            .iter()
                            .map(|binding| format!(
                                "{}:{}",
                                binding
                                    .struct_member
                                    .as_deref()
                                    .unwrap_or_default()
                                    .to_ascii_lowercase(),
                                binding.form_key
                            ))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                })
                .collect::<Vec<_>>()
                .join("\u{1e}")
        );
        unique.insert(key, (script_name, bindings));
    }
    unique.into_values().collect()
}

fn quest_fragment_bindings(payload: &Value) -> HashMap<String, HashSet<String>> {
    let Some(fragments) = payload
        .get("Script Fragments")
        .and_then(Value::as_object)
        .and_then(|script_fragments| script_fragments.get("Fragments"))
        .and_then(Value::as_array)
    else {
        return HashMap::new();
    };
    let mut bindings: HashMap<String, HashSet<String>> = HashMap::new();
    for fragment in fragments {
        let Some(script_name) = fragment.get("ScriptName").and_then(Value::as_str) else {
            continue;
        };
        let Some(fragment_name) = fragment.get("FragmentName").and_then(Value::as_str) else {
            continue;
        };
        bindings
            .entry(normalize_script_name(script_name))
            .or_default()
            .insert(fragment_name.to_ascii_lowercase());
    }
    bindings
}

#[derive(Debug, Clone)]
struct ResolvedPex {
    pex: Option<Arc<PexFilePayload>>,
    error: Option<String>,
    path: Option<PathBuf>,
}

struct PexResolver {
    roots: Vec<PathBuf>,
    target_assets: Option<Arc<TargetAssetStore>>,
    cache: HashMap<String, ResolvedPex>,
    measurements: PexScanMeasurements,
}

impl PexResolver {
    fn new(roots: Vec<PathBuf>, target_assets: Option<Arc<TargetAssetStore>>) -> Self {
        Self {
            roots,
            target_assets,
            cache: HashMap::new(),
            measurements: PexScanMeasurements::default(),
        }
    }

    fn resolve(&mut self, script_name: &str) -> ResolvedPex {
        self.measurements.requests += 1;
        let key = normalize_script_name(script_name);
        if let Some(cached) = self.cache.get(&key) {
            self.measurements.cache_hits += 1;
            return cached.clone();
        }
        let started = Instant::now();
        let relative = script_relative_path(script_name, "pex");
        let mut path = self
            .roots
            .iter()
            .find_map(|root| find_case_insensitive_path(root, &relative));
        if path.is_none()
            && let Some(store) = self.target_assets.as_deref()
        {
            let asset_path = format!("scripts/{}", relative.to_string_lossy().replace('\\', "/"));
            if store.has_asset(&asset_path) {
                path = store.materialize(&asset_path).ok().flatten();
            }
        }
        let resolved = match path {
            Some(path) => match parse_pex_file(&path.to_string_lossy()) {
                Ok(pex) => ResolvedPex {
                    pex: Some(Arc::new(pex)),
                    error: None,
                    path: Some(path),
                },
                Err(error) => ResolvedPex {
                    pex: None,
                    error: Some(error),
                    path: Some(path),
                },
            },
            None => ResolvedPex {
                pex: None,
                error: Some("missing".to_string()),
                path: None,
            },
        };
        if resolved.path.is_some() {
            self.measurements.found += 1;
        }
        if resolved.pex.is_some() {
            self.measurements.parsed += 1;
        }
        if resolved.error.is_some() {
            self.measurements.errors += 1;
        }
        self.measurements.duration += started.elapsed();
        self.cache.insert(key, resolved.clone());
        resolved
    }
}

fn script_relative_path(script_name: &str, extension: &str) -> PathBuf {
    let mut path = PathBuf::new();
    for component in script_name
        .replace('\\', ":")
        .replace('/', ":")
        .split(':')
        .filter(|component| !component.is_empty())
    {
        path.push(component);
    }
    path.set_extension(extension);
    path
}

fn find_case_insensitive_path(root: &Path, relative: &Path) -> Option<PathBuf> {
    let direct = root.join(relative);
    if direct.is_file() {
        return Some(direct);
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let wanted = component.as_os_str().to_string_lossy();
        let entry = std::fs::read_dir(&current)
            .ok()?
            .filter_map(Result::ok)
            .find(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&wanted)
            })?;
        current = entry.path();
    }
    current.is_file().then_some(current)
}

fn source_pex_is_local(path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let parts = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    parts.contains("client") && !parts.contains("server")
}

fn provenance_for_script(script_name: &str, options: &QuestRuntimeInventoryOptions) -> Provenance {
    let key = normalize_script_name(script_name);
    if ADAPTER_SCRIPTS.contains(&key.as_str()) {
        return Provenance::PairAdapter;
    }
    if options
        .addition_scripts
        .iter()
        .any(|candidate| normalize_script_name(candidate) == key)
    {
        return Provenance::FullScriptAddition;
    }
    if find_case_insensitive_path(
        &options.patch_source_root,
        &script_relative_path(script_name, "psc"),
    )
    .is_some()
    {
        return Provenance::PersistentPatch;
    }
    Provenance::Converted
}

fn record_snapshots(
    signature: &str,
    record: &esp_authoring_core::plugin_runtime::ParsedRecord,
    masters: &[String],
    plugin_name: &str,
) -> (Option<bool>, Vec<Value>) {
    let subrecords = effective_subrecords_for_record(record);
    let autostart = if signature.eq_ignore_ascii_case("QUST") {
        subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "DNAM")
            .and_then(|subrecord| {
                (subrecord.data.len() >= 2).then(|| {
                    let flags = u16::from_le_bytes([subrecord.data[0], subrecord.data[1]]);
                    flags & (QUST_FLAG_START_GAME_ENABLED | QUST_FLAG_STARTS_ENABLED) != 0
                })
            })
    } else {
        Some(false)
    };
    let vmads = subrecords
        .iter()
        .filter(|subrecord| subrecord.signature.as_str() == "VMAD")
        .filter_map(|subrecord| {
            compact_vmad_payload_json(&subrecord.data, masters, plugin_name, Some(signature))
        })
        .collect();
    (autostart, vmads)
}

fn propagate_terminal_placements(
    mut reachable: HashSet<String>,
    parents_by_child: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    loop {
        let newly_reachable = parents_by_child
            .iter()
            .filter(|(child, parents)| {
                !reachable.contains(*child)
                    && parents.iter().any(|parent| reachable.contains(parent))
            })
            .map(|(child, _)| child.clone())
            .collect::<Vec<_>>();
        if newly_reachable.is_empty() {
            return reachable;
        }
        reachable.extend(newly_reachable);
    }
}

fn transitively_placed_terminals(
    scan: &crate::session::HandleRawScan<'_>,
    terminal_carriers: &[String],
) -> HashSet<String> {
    let mut frontier = terminal_carriers.iter().cloned().collect::<BTreeSet<_>>();
    let mut visited = HashSet::new();
    let mut directly_placed = HashSet::new();
    let mut parents_by_child = HashMap::<String, HashSet<String>>::new();

    while !frontier.is_empty() {
        let queries = frontier.into_iter().collect::<Vec<_>>();
        let referrers = scan.referencing_form_keys_by_query(&queries);
        let mut next = BTreeSet::new();
        for child in queries {
            visited.insert(child.clone());
            for parent in referrers.get(&child).into_iter().flatten() {
                let signature = scan
                    .record_metadata_by_form_key(parent)
                    .map(|(_, signature, _)| signature)
                    .unwrap_or_default();
                if PLACED_SIGNATURES.contains(&signature.as_str()) {
                    directly_placed.insert(child.clone());
                } else if signature.eq_ignore_ascii_case("TERM") {
                    parents_by_child
                        .entry(child.clone())
                        .or_default()
                        .insert(parent.clone());
                    if !visited.contains(parent) {
                        next.insert(parent.clone());
                    }
                }
            }
        }
        frontier = next;
    }

    propagate_terminal_placements(directly_placed, &parents_by_child)
}

fn collect_carrier_evidence(
    session: &mut PluginSession<'_>,
    handle_id: u64,
    carriers: &BTreeSet<String>,
    scope: EvidenceScope,
    pex_resolver: &mut PexResolver,
    options: &QuestRuntimeInventoryOptions,
    records_scanned: &mut usize,
    vmads_scanned: &mut usize,
    measurements: &mut EvidenceCollectionMeasurements,
) -> Result<Vec<ProducerEvidence>, String> {
    let scan = session.handle_raw_scan(handle_id).map_err(session_error)?;
    let metadata_started = Instant::now();
    let mut metadata = Vec::new();
    let mut signature_by_form_key = HashMap::new();
    let mut placement_queries = Vec::new();
    let mut terminal_carriers = Vec::new();
    for carrier in carriers {
        let Some((raw_form_id, signature, rendered_carrier)) =
            scan.record_metadata_by_form_key(carrier)
        else {
            continue;
        };
        signature_by_form_key.insert(carrier.clone(), signature.clone());
        if !PLACED_SIGNATURES.contains(&signature.as_str())
            && !UNPROVEN_RUNTIME_SIGNATURES.contains(&signature.as_str())
        {
            placement_queries.push(carrier.clone());
        }
        if signature.eq_ignore_ascii_case("TERM") {
            terminal_carriers.push(carrier.clone());
        }
        metadata.push((carrier.clone(), signature, raw_form_id, rendered_carrier));
    }
    measurements.metadata = metadata_started.elapsed();
    let placement_started = Instant::now();
    let transitively_placed_terminals = transitively_placed_terminals(&scan, &terminal_carriers);
    let placement_refs = scan.referencing_form_keys_by_query(&placement_queries);
    let mut placed_by_carrier = HashMap::new();
    for carrier in placement_queries {
        let mut placed = transitively_placed_terminals.contains(&carrier);
        if !placed {
            for referencing in placement_refs.get(&carrier).into_iter().flatten() {
                measurements.placement_referrers += 1;
                let signature = if let Some(signature) = signature_by_form_key.get(referencing) {
                    measurements.signature_cache_hits += 1;
                    signature.clone()
                } else {
                    measurements.signature_cache_misses += 1;
                    let signature = scan
                        .record_metadata_by_form_key(referencing)
                        .map(|(_, signature, _)| signature)
                        .unwrap_or_default();
                    signature_by_form_key.insert(referencing.clone(), signature.clone());
                    signature
                };
                if PLACED_SIGNATURES.contains(&signature.as_str()) {
                    placed = true;
                    break;
                }
            }
        }
        placed_by_carrier.insert(carrier, placed);
    }
    measurements.placement = placement_started.elapsed();

    let mut evidence = Vec::new();
    let mut provenance_by_script = HashMap::new();
    for (carrier, signature, raw_form_id, rendered_carrier) in metadata {
        *records_scanned += 1;
        let placed = PLACED_SIGNATURES.contains(&signature.as_str())
            || placed_by_carrier.get(&carrier).copied().unwrap_or(false);
        let snapshots_started = Instant::now();
        let snapshots = scan
            .with_record_context(raw_form_id, |record, masters, plugin_name| {
                record_snapshots(&signature, record, masters, plugin_name)
            })
            .unwrap_or((None, Vec::new()));
        measurements.snapshots += snapshots_started.elapsed();
        measurements.records_with_vmad += usize::from(!snapshots.1.is_empty());
        *vmads_scanned += snapshots.1.len();
        if signature.eq_ignore_ascii_case("QUST") {
            evidence.push(ProducerEvidence {
                scope,
                carrier: canonical_form_key(&rendered_carrier)?,
                carrier_signature: signature.clone(),
                script_name: String::new(),
                property_bindings: BTreeMap::new(),
                bound_quest_fragments: HashSet::new(),
                pex: None,
                placed,
                provenance: if scope == EvidenceScope::Source {
                    Provenance::Source
                } else {
                    Provenance::Converted
                },
                autostart: snapshots.0,
                local_source: scope != EvidenceScope::Source,
                runtime_issue: Some("unproven_runtime_entry".to_string()),
                pex_error: None,
            });
        }
        for vmad in snapshots.1 {
            let quest_fragment_bindings = quest_fragment_bindings(&vmad);
            for (script_name, property_bindings) in vmad_script_bindings(&vmad) {
                let resolved = pex_resolver.resolve(&script_name);
                let provenance = if scope == EvidenceScope::Source {
                    Provenance::Source
                } else {
                    *provenance_by_script
                        .entry(normalize_script_name(&script_name))
                        .or_insert_with(|| provenance_for_script(&script_name, options))
                };
                let disabled_current_run_producer = scope == EvidenceScope::Target
                    && !options.script_conversion_enabled
                    && matches!(
                        provenance,
                        Provenance::PersistentPatch
                            | Provenance::FullScriptAddition
                            | Provenance::PairAdapter
                    );
                evidence.push(ProducerEvidence {
                    scope,
                    carrier: canonical_form_key(&rendered_carrier)?,
                    carrier_signature: signature.clone(),
                    bound_quest_fragments: quest_fragment_bindings
                        .get(&normalize_script_name(&script_name))
                        .cloned()
                        .unwrap_or_default(),
                    script_name,
                    property_bindings,
                    pex: if disabled_current_run_producer {
                        None
                    } else {
                        resolved.pex
                    },
                    placed,
                    provenance,
                    autostart: snapshots.0,
                    local_source: scope != EvidenceScope::Source
                        || source_pex_is_local(resolved.path.as_deref()),
                    runtime_issue: (!placed).then(|| {
                        if UNPROVEN_RUNTIME_SIGNATURES.contains(&signature.as_str()) {
                            "unproven_runtime_entry".to_string()
                        } else {
                            "unplaced_carrier".to_string()
                        }
                    }),
                    pex_error: if disabled_current_run_producer {
                        Some("script_conversion_disabled".to_string())
                    } else {
                        resolved.error
                    },
                });
            }
        }
    }
    Ok(evidence)
}

fn session_error(error: SessionError) -> String {
    error.to_string()
}

fn canonical_form_key(value: &str) -> Result<String, String> {
    let identity = FormKeyIdentity::parse(value)?;
    let plugin = if let Some((hex, plugin)) = value.split_once('@') {
        let _ = hex;
        plugin.trim()
    } else {
        value
            .rsplit_once(':')
            .map(|(plugin, _)| plugin.trim())
            .ok_or_else(|| format!("invalid FormKey {value:?}"))?
    };
    Ok(format!("{:06X}@{}", identity.local, plugin))
}

fn plugin_index_form_key(value: &str) -> Result<String, String> {
    let identity = FormKeyIdentity::parse(value)?;
    let text = value.trim();
    let plugin = if let Some((_, plugin)) = text.split_once('@') {
        plugin.trim()
    } else if let Some((left, right)) = text.rsplit_once(':') {
        if is_hex_local(right) {
            left.trim()
        } else {
            right.trim()
        }
    } else {
        return Err(format!("invalid FormKey {value:?}"));
    };
    Ok(format!("{plugin}:{:06X}", identity.local))
}

fn expected_keywords(route: &StoryManagerRouteSeed) -> (Option<&str>, Option<&str>) {
    let source = if route.source_selector_keywords.len() == 1 {
        route.source_selector_keywords.first().map(String::as_str)
    } else {
        route.source_start_keyword.as_deref()
    };
    let target = route.bridge_keyword.as_deref().or_else(|| {
        (route.target_selector_keywords.len() == 1)
            .then(|| route.target_selector_keywords[0].as_str())
    });
    (source, target)
}

fn collect_evidence(
    run: &ConversionRun,
    options: &QuestRuntimeInventoryOptions,
    measurements: &mut QuestInventoryMeasurements,
) -> Result<Vec<ProducerEvidence>, String> {
    let query_started = Instant::now();
    measurements.seed_routes = run.story_manager_route_seed.routes.len();
    let mut source_queries = BTreeSet::new();
    let mut target_queries = BTreeSet::new();
    let mut target_quests = BTreeSet::new();
    let query_result = (|| {
        for route in &run.story_manager_route_seed.routes {
            let (source_keyword, target_keyword) = expected_keywords(route);
            if let Some(source_keyword) = source_keyword {
                source_queries.insert(plugin_index_form_key(source_keyword)?);
            }
            if let Some(target_keyword) = target_keyword {
                target_queries.insert(plugin_index_form_key(target_keyword)?);
            }
            if let Some(target_quest) = &route.target_quest {
                let target_quest = plugin_index_form_key(target_quest)?;
                target_queries.insert(target_quest.clone());
                target_quests.insert(target_quest);
            }
        }
        Ok::<_, String>(())
    })();
    measurements.query_build = query_started.elapsed();
    query_result?;

    let session_started = Instant::now();
    let session = open_session(run.target_handle_id, Some(run.source_handle_id));
    measurements.session_open = session_started.elapsed();
    let mut session = session.map_err(session_error)?;
    let source_queries = source_queries.into_iter().collect::<Vec<_>>();
    measurements.source_queries = source_queries.len();
    let source_discovery_started = Instant::now();
    let source_carriers = session
        .referencing_form_keys_by_query_in_handle(run.source_handle_id, &source_queries)
        .map_err(session_error);
    measurements.source_discovery = source_discovery_started.elapsed();
    let source_carriers = source_carriers?
        .into_values()
        .flatten()
        .collect::<BTreeSet<_>>();
    measurements.source_carriers = source_carriers.len();
    let mut target_carriers = target_quests;
    let target_queries = target_queries.into_iter().collect::<Vec<_>>();
    measurements.target_queries = target_queries.len();
    let target_discovery_started = Instant::now();
    let discovered_target = session
        .referencing_form_keys_by_query_in_handle(run.target_handle_id, &target_queries)
        .map_err(session_error);
    measurements.target_discovery = target_discovery_started.elapsed();
    target_carriers.extend(discovered_target?.into_values().flatten());
    measurements.target_carriers = target_carriers.len();

    let mut source_pex = PexResolver::new(options.source_pex_roots.clone(), None);
    let mut target_pex =
        PexResolver::new(options.target_pex_roots.clone(), run.target_assets.clone());
    let source_evidence_started = Instant::now();
    let source_result = collect_carrier_evidence(
        &mut session,
        run.source_handle_id,
        &source_carriers,
        EvidenceScope::Source,
        &mut source_pex,
        options,
        &mut measurements.source_records,
        &mut measurements.source_vmads,
        &mut measurements.source_evidence_detail,
    );
    measurements.source_evidence = source_evidence_started.elapsed();
    measurements.pex.absorb(&source_pex.measurements);
    let mut evidence = source_result?;
    let target_evidence_started = Instant::now();
    let target_result = collect_carrier_evidence(
        &mut session,
        run.target_handle_id,
        &target_carriers,
        EvidenceScope::Target,
        &mut target_pex,
        options,
        &mut measurements.target_records,
        &mut measurements.target_vmads,
        &mut measurements.target_evidence_detail,
    );
    measurements.target_evidence = target_evidence_started.elapsed();
    measurements.pex.absorb(&target_pex.measurements);
    evidence.extend(target_result?);
    Ok(evidence)
}

fn reachable_instruction_indexes(function: &PexFunctionPayload) -> HashSet<usize> {
    if function.instructions.is_empty() {
        return HashSet::new();
    }
    let mut reachable = HashSet::new();
    let mut pending = vec![0_i64];
    while let Some(index) = pending.pop() {
        let Ok(index_usize) = usize::try_from(index) else {
            continue;
        };
        if index_usize >= function.instructions.len() || !reachable.insert(index_usize) {
            continue;
        }
        let instruction = &function.instructions[index_usize];
        if instruction.opcode == OP_RETURN {
            continue;
        }
        if matches!(instruction.opcode, OP_JMP | OP_JMPT | OP_JMPF) {
            if let Some(offset) = instruction.args.last().and_then(value_i64) {
                pending.push(index + offset);
            }
            if instruction.opcode != OP_JMP {
                pending.push(index + 1);
            }
            continue;
        }
        pending.push(index + 1);
    }
    reachable
}

fn value_i64(value: &PexValuePayload) -> Option<i64> {
    value.data.as_i64()
}

fn value_string(value: &PexValuePayload) -> Option<&str> {
    value.data.as_str()
}

fn is_supported_entrypoint(name: &str) -> bool {
    let key = name.to_ascii_lowercase();
    SUPPORTED_ENTRYPOINTS.contains(&key.as_str()) || key.starts_with("fragment_")
}

fn method_name(instruction: &PexInstructionPayload) -> Option<&str> {
    let index = match instruction.opcode {
        OP_CALLMETHOD | OP_CALLPARENT => 0,
        OP_CALLSTATIC => 1,
        _ => return None,
    };
    instruction.args.get(index).and_then(value_string)
}

fn method_receiver(instruction: &PexInstructionPayload) -> String {
    if instruction.opcode != OP_CALLMETHOD {
        return String::new();
    }
    let Some(receiver) = instruction.args.get(1) else {
        return String::new();
    };
    if receiver.value_type != 1 {
        return String::new();
    }
    value_string(receiver)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_quest_start_method(
    method: &str,
    receiver: &str,
    receiver_types: &HashMap<String, String>,
) -> bool {
    let normalized = method
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    if normalized.contains("startquest") {
        return true;
    }
    if normalized != "start" {
        return false;
    }
    receiver_types
        .get(&receiver.to_ascii_lowercase())
        .is_none_or(|receiver_type| !receiver_type.eq_ignore_ascii_case("scene"))
}

fn matching_pex_objects<'a>(
    pex: &'a PexFilePayload,
    script_name: &str,
) -> Vec<&'a PexObjectPayload> {
    let script_key = normalize_script_name(script_name);
    pex.objects
        .iter()
        .filter(|object| normalize_script_name(&object.name) == script_key)
        .collect()
}

fn has_exact_terminal_menu_story_shape(
    pex: &PexFilePayload,
    script_name: &str,
    property_name: &str,
) -> bool {
    let matching_objects = matching_pex_objects(pex, script_name);
    if matching_objects.len() != 1 {
        return false;
    }
    let object = matching_objects[0];
    object.properties.iter().any(|property| {
        property.name.eq_ignore_ascii_case(property_name)
            && property
                .name
                .eq_ignore_ascii_case(TERMINAL_MENU_DATA_PROPERTY)
            && property.ty.eq_ignore_ascii_case(TERMINAL_MENU_DATA_TYPE)
    }) && object.structs.iter().any(|structure| {
        structure.name.eq_ignore_ascii_case("MenuDatum")
            && structure.members.iter().any(|member| {
                member.name.eq_ignore_ascii_case(TERMINAL_MENU_STORY_MEMBER)
                    && member.ty.eq_ignore_ascii_case("Keyword")
            })
    })
}

fn indirect_terminal_story_receivers(function: &PexFunctionPayload) -> HashSet<String> {
    let local_types = function
        .params
        .iter()
        .map(|parameter| (parameter.name.to_ascii_lowercase(), parameter.ty.as_str()))
        .chain(
            function
                .locals
                .iter()
                .map(|local| (local.name.to_ascii_lowercase(), local.ty.as_str())),
        )
        .collect::<HashMap<_, _>>();
    reachable_instruction_indexes(function)
        .into_iter()
        .filter_map(|instruction_index| {
            let instruction = &function.instructions[instruction_index];
            if instruction.opcode != OP_STRUCTGET {
                return None;
            }
            let destination = instruction.args.first().and_then(value_string)?;
            let source = instruction.args.get(1).and_then(value_string)?;
            let member = instruction.args.get(2).and_then(value_string)?;
            (member.eq_ignore_ascii_case(TERMINAL_MENU_STORY_MEMBER)
                && local_types
                    .get(&source.to_ascii_lowercase())
                    .is_some_and(|ty| ty.eq_ignore_ascii_case(TERMINAL_MENU_DATUM_TYPE))
                && local_types
                    .get(&destination.to_ascii_lowercase())
                    .is_some_and(|ty| ty.eq_ignore_ascii_case("Keyword")))
            .then(|| destination.to_ascii_lowercase())
        })
        .collect()
}

#[derive(Debug, Default)]
struct PexCallResult {
    first_sender_entrypoint: Option<String>,
    sends_story_event: bool,
    starts_quest_directly: bool,
}

fn pex_calls(
    pex: &PexFilePayload,
    script_name: &str,
    property_name: Option<&str>,
    scan_all_functions: bool,
) -> PexCallResult {
    pex_calls_with_start_receivers(
        pex,
        script_name,
        property_name,
        scan_all_functions,
        None,
        false,
    )
}

fn pex_calls_with_start_receivers(
    pex: &PexFilePayload,
    script_name: &str,
    property_name: Option<&str>,
    scan_all_functions: bool,
    start_receivers: Option<&HashSet<String>>,
    allow_indirect_story_receiver: bool,
) -> PexCallResult {
    let matching_objects = matching_pex_objects(pex, script_name);
    if matching_objects.len() != 1 {
        return PexCallResult::default();
    }
    let object = matching_objects[0];
    let script_key = normalize_script_name(script_name);
    let normalized_property_name = property_name.map(normalize_papyrus_identifier);
    let keyword_auto_vars = object
        .properties
        .iter()
        .filter(|property| {
            normalized_property_name
                .as_ref()
                .is_some_and(|name| normalize_papyrus_identifier(&property.name) == *name)
                && property.ty.eq_ignore_ascii_case("keyword")
        })
        .map(|property| {
            if property.auto_var.is_empty() {
                format!("::{}_var", property.name).to_ascii_lowercase()
            } else {
                property.auto_var.to_ascii_lowercase()
            }
        })
        .collect::<HashSet<_>>();
    let receiver_types = object
        .properties
        .iter()
        .map(|property| {
            let receiver = if property.auto_var.is_empty() {
                format!("::{}_var", property.name)
            } else {
                property.auto_var.clone()
            };
            (receiver.to_ascii_lowercase(), property.ty.clone())
        })
        .collect::<HashMap<_, _>>();

    struct FunctionRef<'a> {
        state: String,
        function: &'a PexFunctionPayload,
    }
    let mut functions = Vec::new();
    for state in &object.states {
        for function in &state.functions {
            functions.push(FunctionRef {
                state: state.name.to_ascii_lowercase(),
                function,
            });
        }
    }
    let mut by_state_and_name: HashMap<(String, String), Vec<usize>> = HashMap::new();
    let mut global_by_name: HashMap<String, Vec<usize>> = HashMap::new();
    let mut entrypoints = Vec::new();
    for (index, function_ref) in functions.iter().enumerate() {
        let name = function_ref.function.name.to_ascii_lowercase();
        by_state_and_name
            .entry((function_ref.state.clone(), name.clone()))
            .or_default()
            .push(index);
        if function_ref.function.is_global {
            global_by_name.entry(name).or_default().push(index);
        }
        if (scan_all_functions || is_supported_entrypoint(&function_ref.function.name))
            && (!allow_indirect_story_receiver
                || function_ref
                    .function
                    .name
                    .eq_ignore_ascii_case("OnMenuItemRun"))
        {
            entrypoints.push(index);
        }
    }

    let mut result = PexCallResult::default();
    for entrypoint_index in entrypoints {
        let mut pending = vec![entrypoint_index];
        let mut visited = HashSet::new();
        let mut sends_story_event = false;
        let mut starts_quest_directly = false;
        while let Some(function_index) = pending.pop() {
            if !visited.insert(function_index) {
                continue;
            }
            let function_ref = &functions[function_index];
            let indirect_story_receivers = allow_indirect_story_receiver
                .then(|| indirect_terminal_story_receivers(function_ref.function))
                .unwrap_or_default();
            for instruction_index in reachable_instruction_indexes(function_ref.function) {
                let instruction = &function_ref.function.instructions[instruction_index];
                let Some(method) = method_name(instruction) else {
                    continue;
                };
                let method_key = method.to_ascii_lowercase();
                let receiver = method_receiver(instruction);
                let explicit_start_quest =
                    normalize_papyrus_identifier(method).contains("startquest");
                if is_quest_start_method(method, &receiver, &receiver_types)
                    && start_receivers
                        .is_none_or(|allowed| explicit_start_quest || allowed.contains(&receiver))
                {
                    starts_quest_directly = true;
                }
                if matches!(
                    method_key.as_str(),
                    "sendstoryevent" | "sendstoryeventandwait"
                ) && (keyword_auto_vars.contains(&receiver)
                    || indirect_story_receivers.contains(&receiver))
                {
                    sends_story_event = true;
                }
                if instruction.opcode == OP_CALLMETHOD && receiver == "self" {
                    for state in [function_ref.state.as_str(), ""] {
                        if let Some(candidates) =
                            by_state_and_name.get(&(state.to_string(), method_key.clone()))
                        {
                            pending.extend(candidates.iter().copied());
                        }
                    }
                } else if instruction.opcode == OP_CALLSTATIC
                    && instruction
                        .args
                        .first()
                        .and_then(value_string)
                        .is_some_and(|owner| normalize_script_name(owner) == script_key)
                    && let Some(candidates) = global_by_name.get(&method_key)
                {
                    pending.extend(candidates.iter().copied());
                }
            }
        }
        if sends_story_event && result.first_sender_entrypoint.is_none() {
            result.first_sender_entrypoint =
                Some(functions[entrypoint_index].function.name.clone());
        }
        result.sends_story_event |= sends_story_event;
        result.starts_quest_directly |= starts_quest_directly;
    }
    result
}

fn prove<'a>(
    evidence: &'a ProducerEvidence,
    expected_keyword: Option<&str>,
    direct_start_quest: Option<&FormKeyIdentity>,
) -> ProducerProof<'a> {
    let exact_terminal_menu_sender = evidence.carrier_signature.eq_ignore_ascii_case("TERM")
        && normalize_script_name(&evidence.script_name) == TERMINAL_MENU_STORY_SENDER;
    let property_name = expected_keyword.and_then(|expected| {
        let expected = FormKeyIdentity::parse(expected).ok()?;
        evidence
            .property_bindings
            .iter()
            .filter_map(|(name, bindings)| {
                bindings
                    .iter()
                    .any(|binding| {
                        FormKeyIdentity::parse(&binding.form_key).ok().as_ref() == Some(&expected)
                            && (!exact_terminal_menu_sender
                                || (name.eq_ignore_ascii_case(TERMINAL_MENU_DATA_PROPERTY)
                                    && binding.struct_member.as_deref().is_some_and(|member| {
                                        member.eq_ignore_ascii_case(TERMINAL_MENU_STORY_MEMBER)
                                    })))
                    })
                    .then_some(name)
            })
            .min_by_key(|name| name.to_ascii_lowercase())
            .cloned()
    });
    let Some(pex) = evidence.pex.as_deref() else {
        return ProducerProof {
            evidence,
            property_name,
            entrypoint: None,
            sends_story_event: false,
            starts_quest_directly: false,
        };
    };
    if property_name.is_none() || pex.objects.is_empty() {
        return ProducerProof {
            evidence,
            property_name,
            entrypoint: None,
            sends_story_event: false,
            starts_quest_directly: false,
        };
    }
    let exact_terminal_menu_shape = property_name
        .as_deref()
        .is_some_and(|name| has_exact_terminal_menu_story_shape(pex, &evidence.script_name, name));
    if exact_terminal_menu_sender && !exact_terminal_menu_shape {
        return ProducerProof {
            evidence,
            property_name,
            entrypoint: None,
            sends_story_event: false,
            starts_quest_directly: direct_start_quest
                .is_some_and(|target_quest| evidence_starts_quest_directly(evidence, target_quest)),
        };
    }
    let allow_indirect_story_receiver = exact_terminal_menu_sender && exact_terminal_menu_shape;
    let calls = pex_calls_with_start_receivers(
        pex,
        &evidence.script_name,
        property_name.as_deref(),
        false,
        None,
        allow_indirect_story_receiver,
    );
    ProducerProof {
        evidence,
        property_name,
        entrypoint: calls.first_sender_entrypoint,
        sends_story_event: calls.sends_story_event,
        starts_quest_directly: direct_start_quest
            .is_some_and(|target_quest| evidence_starts_quest_directly(evidence, target_quest)),
    }
}

fn evidence_form_keys(evidence: &ProducerEvidence) -> HashSet<FormKeyIdentity> {
    evidence
        .property_bindings
        .values()
        .flatten()
        .filter_map(|binding| FormKeyIdentity::parse(&binding.form_key).ok())
        .collect()
}

fn target_evidence_is_route_associated(
    evidence: &ProducerEvidence,
    route: &StoryManagerRouteSeed,
    expected_keyword: Option<&str>,
    proven_carriers: &HashSet<FormKeyIdentity>,
) -> bool {
    let Ok(carrier) = FormKeyIdentity::parse(&evidence.carrier) else {
        return false;
    };
    let relevant = evidence_form_keys(evidence);
    let target_quest = route
        .target_quest
        .as_deref()
        .and_then(|value| FormKeyIdentity::parse(value).ok());
    let expected = expected_keyword.and_then(|value| FormKeyIdentity::parse(value).ok());
    proven_carriers.contains(&carrier)
        || target_quest.as_ref().is_some_and(|quest| carrier == *quest)
        || target_quest
            .as_ref()
            .is_some_and(|quest| relevant.contains(quest))
        || expected
            .as_ref()
            .is_some_and(|keyword| relevant.contains(keyword))
}

fn evidence_starts_quest_directly(
    evidence: &ProducerEvidence,
    target_quest: &FormKeyIdentity,
) -> bool {
    let Some(pex) = evidence.pex.as_deref() else {
        return false;
    };
    if matching_pex_objects(pex, &evidence.script_name).len() != 1 {
        return true;
    }
    let target_property_names = evidence
        .property_bindings
        .iter()
        .filter_map(|(name, bindings)| {
            bindings
                .iter()
                .any(|binding| {
                    FormKeyIdentity::parse(&binding.form_key).ok().as_ref() == Some(target_quest)
                })
                .then_some(normalize_papyrus_identifier(name))
        })
        .collect::<HashSet<_>>();
    let object = matching_pex_objects(pex, &evidence.script_name)[0];
    let mut target_receivers = object
        .properties
        .iter()
        .filter(|property| {
            target_property_names.contains(&normalize_papyrus_identifier(&property.name))
        })
        .map(|property| {
            if property.auto_var.is_empty() {
                format!("::{}_var", property.name).to_ascii_lowercase()
            } else {
                property.auto_var.to_ascii_lowercase()
            }
        })
        .collect::<HashSet<_>>();
    if FormKeyIdentity::parse(&evidence.carrier)
        .ok()
        .is_some_and(|carrier| carrier == *target_quest)
    {
        target_receivers.insert("self".to_string());
    }
    pex_calls_with_start_receivers(
        pex,
        &evidence.script_name,
        None,
        true,
        Some(&target_receivers),
        false,
    )
    .starts_quest_directly
}

fn evidence_has_bypass(
    evidence: &ProducerEvidence,
    target_quest: Option<&FormKeyIdentity>,
) -> bool {
    let Some(target_quest) = target_quest else {
        return false;
    };
    (evidence.autostart == Some(true)
        && FormKeyIdentity::parse(&evidence.carrier)
            .ok()
            .is_some_and(|carrier| carrier == *target_quest))
        || evidence_starts_quest_directly(evidence, target_quest)
}

fn target_quest_autostart(
    route: &StoryManagerRouteSeed,
    target_evidence: &[&ProducerEvidence],
) -> Option<bool> {
    let target_quest = FormKeyIdentity::parse(route.target_quest.as_deref()?).ok()?;
    let metadata = target_evidence
        .iter()
        .filter(|evidence| evidence.carrier_signature.eq_ignore_ascii_case("QUST"))
        .filter(|evidence| {
            FormKeyIdentity::parse(&evidence.carrier)
                .ok()
                .is_some_and(|carrier| carrier == target_quest)
        })
        .map(|evidence| evidence.autostart)
        .collect::<Vec<_>>();
    if metadata.contains(&Some(true)) {
        return Some(true);
    }
    if !metadata.is_empty() && metadata.iter().all(|value| *value == Some(false)) {
        return Some(false);
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct UnsupportedRouteKey {
    source_quest: FormKeyIdentity,
    source_node: Option<FormKeyIdentity>,
    source_event: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UnsupportedManifest {
    schema_version: u32,
    #[serde(default)]
    routes: Vec<UnsupportedRoute>,
}

#[derive(Debug, Clone, Deserialize)]
struct UnsupportedRoute {
    source_quest: String,
    source_node: Option<String>,
    source_event: String,
    reason_code: String,
    evidence: String,
}

impl UnsupportedRoute {
    fn key(&self) -> Result<UnsupportedRouteKey, String> {
        Ok(UnsupportedRouteKey {
            source_quest: FormKeyIdentity::parse(&self.source_quest)?,
            source_node: self
                .source_node
                .as_deref()
                .map(FormKeyIdentity::parse)
                .transpose()?,
            source_event: self.source_event.to_ascii_uppercase(),
        })
    }
}

fn route_key(route: &StoryManagerRouteSeed) -> Result<UnsupportedRouteKey, String> {
    Ok(UnsupportedRouteKey {
        source_quest: FormKeyIdentity::parse(&route.source_quest)?,
        source_node: route
            .source_node
            .as_deref()
            .map(FormKeyIdentity::parse)
            .transpose()?,
        source_event: route
            .source_root_event
            .as_deref()
            .or(route.source_quest_event.as_deref())
            .unwrap_or("none")
            .to_ascii_uppercase(),
    })
}

fn load_unsupported_manifest(
    path: &Path,
) -> Result<HashMap<UnsupportedRouteKey, UnsupportedRoute>, String> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    let manifest: UnsupportedManifest = serde_saphyr::from_str(&text)
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    if manifest.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "{}: schema_version must be {SCHEMA_VERSION}",
            path.display()
        ));
    }
    let mut result = HashMap::new();
    for entry in manifest.routes {
        let values = [
            entry.source_quest.as_str(),
            entry.source_node.as_deref().unwrap_or_default(),
            entry.source_event.as_str(),
            entry.reason_code.as_str(),
            entry.evidence.as_str(),
        ];
        if values
            .iter()
            .any(|value| value.contains('*') || value.contains('?'))
        {
            return Err(format!(
                "{}: unsupported routes may not use wildcards",
                path.display()
            ));
        }
        if !KNOWN_UNSUPPORTED_REASONS.contains(&entry.reason_code.as_str()) {
            return Err(format!(
                "{}: unknown reason_code {:?}",
                path.display(),
                entry.reason_code
            ));
        }
        if entry.evidence.trim().is_empty() {
            return Err(format!(
                "{}: unsupported route evidence is empty",
                path.display()
            ));
        }
        let key = entry.key()?;
        if result.insert(key, entry).is_some() {
            return Err(format!("{}: duplicate unsupported route", path.display()));
        }
    }
    Ok(result)
}

struct RouteEvidenceIndex<'a> {
    source_by_binding: HashMap<FormKeyIdentity, Vec<usize>>,
    target_by_binding: HashMap<FormKeyIdentity, Vec<usize>>,
    target_by_carrier: HashMap<FormKeyIdentity, Vec<usize>>,
    source: Vec<&'a ProducerEvidence>,
    target: Vec<&'a ProducerEvidence>,
}

impl<'a> RouteEvidenceIndex<'a> {
    fn new(evidence: &'a [ProducerEvidence]) -> Self {
        let mut index = Self {
            source_by_binding: HashMap::new(),
            target_by_binding: HashMap::new(),
            target_by_carrier: HashMap::new(),
            source: Vec::new(),
            target: Vec::new(),
        };
        for item in evidence {
            let (items, bindings) = match item.scope {
                EvidenceScope::Source => (&mut index.source, &mut index.source_by_binding),
                EvidenceScope::Target => (&mut index.target, &mut index.target_by_binding),
            };
            let item_index = items.len();
            items.push(item);
            let mut seen_bindings = HashSet::new();
            for binding in item.property_bindings.values().flatten() {
                if let Ok(identity) = FormKeyIdentity::parse(&binding.form_key)
                    && seen_bindings.insert(identity.clone())
                {
                    bindings.entry(identity).or_default().push(item_index);
                }
            }
            if item.scope == EvidenceScope::Target
                && let Ok(identity) = FormKeyIdentity::parse(&item.carrier)
            {
                index
                    .target_by_carrier
                    .entry(identity)
                    .or_default()
                    .push(item_index);
            }
        }
        index
    }

    fn candidates<'b>(
        items: &[&'a ProducerEvidence],
        lists: impl IntoIterator<Item = Option<&'b Vec<usize>>>,
    ) -> Vec<&'a ProducerEvidence> {
        let mut indexes = BTreeSet::new();
        for list in lists.into_iter().flatten() {
            indexes.extend(list.iter().copied());
        }
        indexes.into_iter().map(|index| items[index]).collect()
    }

    fn target_for_route(&self, route: &StoryManagerRouteSeed) -> Vec<&'a ProducerEvidence> {
        let (_, target_keyword) = expected_keywords(route);
        let target_keyword = target_keyword.and_then(|value| FormKeyIdentity::parse(value).ok());
        let target_quest = route
            .target_quest
            .as_deref()
            .and_then(|value| FormKeyIdentity::parse(value).ok());
        let mut indexes = BTreeSet::new();
        for list in [
            target_keyword
                .as_ref()
                .and_then(|key| self.target_by_binding.get(key)),
            target_quest
                .as_ref()
                .and_then(|key| self.target_by_binding.get(key)),
            target_quest
                .as_ref()
                .and_then(|key| self.target_by_carrier.get(key)),
        ]
        .into_iter()
        .flatten()
        {
            indexes.extend(list.iter().copied());
        }
        let proven_carriers = target_keyword
            .as_ref()
            .and_then(|key| self.target_by_binding.get(key))
            .into_iter()
            .flatten()
            .filter_map(|index| FormKeyIdentity::parse(&self.target[*index].carrier).ok())
            .collect::<HashSet<_>>();
        for carrier in proven_carriers {
            if let Some(list) = self.target_by_carrier.get(&carrier) {
                indexes.extend(list.iter().copied());
            }
        }
        indexes
            .into_iter()
            .map(|index| self.target[index])
            .collect()
    }

    fn source_for_route(&self, route: &StoryManagerRouteSeed) -> Vec<&'a ProducerEvidence> {
        let (source_keyword, _) = expected_keywords(route);
        let source_keyword = source_keyword.and_then(|value| FormKeyIdentity::parse(value).ok());
        Self::candidates(
            &self.source,
            [source_keyword
                .as_ref()
                .and_then(|key| self.source_by_binding.get(key))],
        )
    }
}

fn classify_route(
    mut route: StoryManagerRouteSeed,
    source_evidence: &[&ProducerEvidence],
    target_evidence: &[&ProducerEvidence],
    unsupported: &HashMap<UnsupportedRouteKey, UnsupportedRoute>,
) -> Result<InventoryRoute, String> {
    let mut issues = route.issues.iter().cloned().collect::<BTreeSet<_>>();
    let mut classification = None;
    let mut runtime_proof = Vec::new();
    let (_, target_keyword) = expected_keywords(&route);
    let target_quest = route
        .target_quest
        .as_deref()
        .and_then(|value| FormKeyIdentity::parse(value).ok());
    let keyword_proofs = target_evidence
        .iter()
        .map(|item| prove(item, target_keyword, target_quest.as_ref()))
        .collect::<Vec<_>>();
    let proven_carriers = keyword_proofs
        .iter()
        .filter(|proof| proof.property_name.is_some())
        .filter_map(|proof| FormKeyIdentity::parse(&proof.evidence.carrier).ok())
        .collect::<HashSet<_>>();
    let route_bypass = target_evidence.iter().any(|item| {
        target_evidence_is_route_associated(item, &route, target_keyword, &proven_carriers)
            && evidence_has_bypass(item, target_quest.as_ref())
    });
    let target_quest_autostart = target_quest_autostart(&route, &target_evidence);
    let autostart_unknown = target_quest_autostart.is_none();
    let structurally_eligible = route.emission_status == "emitted"
        && route.structural_status == "eligible"
        && route.condition_valid;

    if !structurally_eligible {
        issues.insert("structural_invalid".to_string());
        if route.target_selector_keywords.len() > 1 {
            issues.insert("conflicting_selector_keywords".to_string());
        }
    } else if autostart_unknown {
        issues.insert("autostart_unknown".to_string());
    } else if route_bypass {
        issues.insert("story_manager_bypass".to_string());
    } else if route
        .target_event
        .as_deref()
        .is_some_and(|event| NATIVE_ENGINE_EVENTS.contains(&event.to_ascii_uppercase().as_str()))
    {
        classification = Some("native".to_string());
    } else if route
        .target_event
        .as_deref()
        .is_some_and(|event| event.eq_ignore_ascii_case("SCPT"))
    {
        let (source_keyword, _) = expected_keywords(&route);
        let source_quest = FormKeyIdentity::parse(&route.source_quest).ok();
        let source_proofs = source_evidence
            .iter()
            .map(|item| prove(item, source_keyword, source_quest.as_ref()))
            .collect::<Vec<_>>();
        let qualifying_source = source_proofs
            .iter()
            .filter(|proof| proof.qualifies())
            .collect::<Vec<_>>();
        let qualifying_target = keyword_proofs
            .iter()
            .filter(|proof| proof.qualifies())
            .collect::<Vec<_>>();
        let adapted_target = qualifying_target
            .iter()
            .copied()
            .filter(|proof| {
                matches!(
                    proof.evidence.provenance,
                    Provenance::PersistentPatch
                        | Provenance::FullScriptAddition
                        | Provenance::PairAdapter
                ) || ADAPTER_SCRIPTS
                    .contains(&normalize_script_name(&proof.evidence.script_name).as_str())
            })
            .collect::<Vec<_>>();
        let native_target = qualifying_target
            .iter()
            .copied()
            .filter(|proof| {
                matches!(
                    proof.evidence.provenance,
                    Provenance::Converted | Provenance::Source
                )
            })
            .collect::<Vec<_>>();

        if let Some(proof) = adapted_target.first() {
            classification = Some("adapted".to_string());
            if let Some(payload) = proof.payload() {
                runtime_proof.push(payload);
            }
        } else if let (Some(source), Some(target)) =
            (qualifying_source.first(), native_target.first())
        {
            classification = Some("native".to_string());
            runtime_proof.extend([source.payload(), target.payload()].into_iter().flatten());
        } else {
            let target_bound = keyword_proofs
                .iter()
                .filter(|proof| proof.property_name.is_some())
                .collect::<Vec<_>>();
            let source_bound = source_proofs
                .iter()
                .filter(|proof| proof.property_name.is_some())
                .collect::<Vec<_>>();
            if route.bridge_keyword.is_some() && qualifying_target.is_empty() {
                issues.insert("bridge_without_producer".to_string());
            }
            if !qualifying_source.is_empty() && qualifying_target.is_empty() {
                issues.insert("sender_lost_in_conversion".to_string());
            } else if qualifying_source.is_empty() {
                issues.insert("no_local_source_producer".to_string());
            }
            if target_bound
                .iter()
                .any(|proof| proof.evidence.pex.is_none())
            {
                issues.insert("missing_compiled_pex".to_string());
            }
            for proof in &target_bound {
                if !proof.evidence.placed {
                    issues.insert(
                        proof
                            .evidence
                            .runtime_issue
                            .clone()
                            .unwrap_or_else(|| "unplaced_carrier".to_string()),
                    );
                }
                if proof.evidence.autostart.is_none() {
                    issues.insert("autostart_unknown".to_string());
                }
                if proof.evidence.pex_error.as_deref() == Some("script_conversion_disabled") {
                    issues.insert("script_conversion_disabled".to_string());
                }
            }
            if !source_bound.is_empty() && target_bound.is_empty() {
                issues.insert("sender_lost_in_conversion".to_string());
            }
        }
    }

    let manifest_entry = if !route_bypass && !autostart_unknown {
        unsupported.get(&route_key(&route)?)
    } else {
        None
    };
    let mut unsupported_reason_code = None;
    let mut unsupported_evidence = None;
    if let Some(entry) = manifest_entry {
        if classification.is_some() {
            return Err(format!(
                "stale unsupported manifest entry for {}: a producer now exists",
                route.route_id
            ));
        }
        classification = Some("explicitly_unsupported".to_string());
        unsupported_reason_code = Some(entry.reason_code.clone());
        unsupported_evidence = Some(entry.evidence.clone());
    }
    runtime_proof.sort_by(|left, right| {
        format!(
            "{:?}\u{1f}{}\u{1f}{}",
            left.scope,
            left.carrier.to_ascii_lowercase(),
            left.script_name.to_ascii_lowercase()
        )
        .cmp(&format!(
            "{:?}\u{1f}{}\u{1f}{}",
            right.scope,
            right.carrier.to_ascii_lowercase(),
            right.script_name.to_ascii_lowercase()
        ))
    });
    route.issues = issues.into_iter().collect();
    Ok(InventoryRoute {
        route,
        classification: classification.unwrap_or_else(|| "unclassified".to_string()),
        runtime_proof,
        unsupported_reason_code,
        unsupported_evidence,
    })
}

#[cfg(test)]
fn classify_route_full_scan(
    route: StoryManagerRouteSeed,
    evidence: &[ProducerEvidence],
    unsupported: &HashMap<UnsupportedRouteKey, UnsupportedRoute>,
) -> Result<InventoryRoute, String> {
    let source = evidence
        .iter()
        .filter(|item| item.scope == EvidenceScope::Source)
        .collect::<Vec<_>>();
    let target = evidence
        .iter()
        .filter(|item| item.scope == EvidenceScope::Target)
        .collect::<Vec<_>>();
    classify_route(route, &source, &target, unsupported)
}

fn build_report(
    seed: &StoryManagerRouteSeedReport,
    evidence: &[ProducerEvidence],
    unsupported: &HashMap<UnsupportedRouteKey, UnsupportedRoute>,
) -> Result<QuestRuntimeInventoryReport, String> {
    build_report_internal(seed, evidence, unsupported, None)
}

fn build_report_internal(
    seed: &StoryManagerRouteSeedReport,
    evidence: &[ProducerEvidence],
    unsupported: &HashMap<UnsupportedRouteKey, UnsupportedRoute>,
    mut measurements: Option<&mut QuestInventoryMeasurements>,
) -> Result<QuestRuntimeInventoryReport, String> {
    let classification_started = Instant::now();
    let classification_result = (|| {
        if seed.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "native route seed schema_version must be {SCHEMA_VERSION}"
            ));
        }
        let seed_keys = seed
            .routes
            .iter()
            .map(route_key)
            .collect::<Result<HashSet<_>, _>>()?;
        if let Some(stale) = unsupported.keys().find(|key| !seed_keys.contains(*key)) {
            return Err(format!(
                "unsupported manifest entry does not match a native seed route: {stale:?}"
            ));
        }
        let mut routes = seed.routes.clone();
        routes.sort_by(|left, right| left.route_id.cmp(&right.route_id));
        let evidence_index = RouteEvidenceIndex::new(evidence);
        routes
            .into_iter()
            .map(|route| {
                let source_evidence = evidence_index.source_for_route(&route);
                let target_evidence = evidence_index.target_for_route(&route);
                classify_route(route, &source_evidence, &target_evidence, unsupported)
            })
            .collect::<Result<Vec<_>, _>>()
    })();
    if let Some(measurements) = measurements.as_deref_mut() {
        measurements.route_classification = classification_started.elapsed();
    }
    let routes = classification_result?;
    let assembly_started = Instant::now();
    let count = |classification: &str| {
        routes
            .iter()
            .filter(|route| route.classification == classification)
            .count()
    };
    let unclassified_route_ids = routes
        .iter()
        .filter(|route| route.classification == "unclassified")
        .map(|route| route.route.route_id.clone())
        .collect::<Vec<_>>();
    let report = QuestRuntimeInventoryReport {
        schema_version: SCHEMA_VERSION,
        source_game: seed.source_game.clone(),
        target_game: seed.target_game.clone(),
        summary: QuestRuntimeInventorySummary {
            total: routes.len(),
            native: count("native"),
            adapted: count("adapted"),
            explicitly_unsupported: count("explicitly_unsupported"),
            unclassified: count("unclassified"),
        },
        coverage_complete: unclassified_route_ids.is_empty(),
        routes,
        unclassified_route_ids,
        inventory_error: None,
    };
    if let Some(measurements) = measurements {
        measurements.report_assembly = assembly_started.elapsed();
    }
    Ok(report)
}

impl ConversionRun {
    pub fn quest_runtime_inventory(
        &self,
        options: &QuestRuntimeInventoryOptions,
    ) -> QuestRuntimeInventoryReport {
        let seed = &self.story_manager_route_seed;
        let total_started = Instant::now();
        let mut measurements = QuestInventoryMeasurements::default();
        let result = (|| {
            let manifest_started = Instant::now();
            let unsupported = load_unsupported_manifest(&options.unsupported_manifest);
            measurements.manifest = manifest_started.elapsed();
            let unsupported = unsupported?;
            let evidence = collect_evidence(self, options, &mut measurements)?;
            build_report_internal(seed, &evidence, &unsupported, Some(&mut measurements))
        })();
        measurements.total = total_started.elapsed();
        measurements.emit(result.is_ok());
        result.unwrap_or_else(|error| QuestRuntimeInventoryReport::failure(seed, error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_load_no_py,
    };
    use papyrus_core::pex::{
        PexInstructionPayload, PexObjectPayload, PexPropertyPayload, PexStatePayload,
    };
    use serde_json::json;

    fn target_queries_from_inventory_report(report: &Value) -> Vec<String> {
        let mut queries = BTreeSet::new();
        for route in report["routes"].as_array().into_iter().flatten() {
            if let Some(value) = route.get("bridge_keyword").and_then(Value::as_str) {
                queries.insert(plugin_index_form_key(value).unwrap());
            } else if let Some(values) = route
                .get("target_selector_keywords")
                .and_then(Value::as_array)
                && values.len() == 1
                && let Some(value) = values[0].as_str()
            {
                queries.insert(plugin_index_form_key(value).unwrap());
            }
            if let Some(value) = route.get("target_quest").and_then(Value::as_str) {
                queries.insert(plugin_index_form_key(value).unwrap());
            }
        }
        queries.into_iter().collect()
    }

    #[test]
    #[ignore = "requires QUEST_TARGET_PLUGIN and QUEST_INVENTORY_REPORT"]
    fn current_target_carrier_lookup_matches_legacy_session_path() {
        let plugin_path = PathBuf::from(std::env::var_os("QUEST_TARGET_PLUGIN").unwrap());
        let report_path = PathBuf::from(std::env::var_os("QUEST_INVENTORY_REPORT").unwrap());
        let report: Value = serde_json::from_slice(&std::fs::read(report_path).unwrap()).unwrap();
        let queries = target_queries_from_inventory_report(&report);
        let handle =
            plugin_handle_load_no_py(plugin_path.to_str().unwrap(), Some("fo4"), None, None, true)
                .unwrap();
        let mut session = open_session(handle, None).unwrap();
        let mut carriers = session
            .referencing_form_keys_by_query_in_handle(handle, &queries)
            .unwrap()
            .into_values()
            .flatten()
            .collect::<BTreeSet<_>>();
        carriers.extend(
            report["routes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|route| route.get("target_quest").and_then(Value::as_str))
                .map(|value| plugin_index_form_key(value).unwrap()),
        );

        let legacy = carriers
            .iter()
            .map(|carrier| {
                let signature = session.record_signature_in_handle(handle, carrier).unwrap();
                (carrier.clone(), signature)
            })
            .collect::<Vec<_>>();
        let scan = session.handle_raw_scan(handle).unwrap();
        let indexed = carriers
            .iter()
            .map(|carrier| {
                let signature = scan
                    .record_metadata_by_form_key(carrier)
                    .map(|(_, signature, _)| signature);
                (carrier.clone(), signature)
            })
            .collect::<Vec<_>>();
        assert_eq!(indexed, legacy);
        assert_eq!(scan.record_metadata_by_form_key("not-a-form-key"), None);
        drop(scan);
        assert_eq!(
            session
                .record_signature_in_handle(handle, "not-a-form-key")
                .unwrap(),
            None
        );
        drop(session);
        plugin_handle_close_native(handle);
    }

    #[test]
    fn measured_report_preserves_serialized_output() {
        let seed = StoryManagerRouteSeedReport {
            schema_version: SCHEMA_VERSION,
            source_game: "fo76".to_string(),
            target_game: "fo4".to_string(),
            routes: vec![route("KILL")],
        };
        let unsupported = HashMap::new();
        let expected = build_report(&seed, &[], &unsupported).unwrap();
        let mut measurements = QuestInventoryMeasurements::default();
        let measured =
            build_report_internal(&seed, &[], &unsupported, Some(&mut measurements)).unwrap();

        assert_eq!(
            serde_json::to_value(measured).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
    }

    #[test]
    fn measured_report_records_failed_classification_without_changing_error() {
        let seed = StoryManagerRouteSeedReport {
            schema_version: SCHEMA_VERSION + 1,
            source_game: "fo76".to_string(),
            target_game: "fo4".to_string(),
            routes: Vec::new(),
        };
        let unsupported = HashMap::new();
        let expected = build_report(&seed, &[], &unsupported).unwrap_err();
        let mut measurements = QuestInventoryMeasurements::default();
        let measured =
            build_report_internal(&seed, &[], &unsupported, Some(&mut measurements)).unwrap_err();

        assert_eq!(measured, expected);
        assert!(measurements.route_classification > Duration::ZERO);
    }

    #[test]
    fn pex_measurements_count_requests_cache_hits_and_errors() {
        let root = tempfile::tempdir().unwrap();
        let mut resolver = PexResolver::new(vec![root.path().to_path_buf()], None);

        assert!(resolver.resolve("Missing:InventoryScript").pex.is_none());
        assert!(resolver.resolve("Missing:InventoryScript").pex.is_none());

        assert_eq!(resolver.measurements.requests, 2);
        assert_eq!(resolver.measurements.cache_hits, 1);
        assert_eq!(resolver.measurements.found, 0);
        assert_eq!(resolver.measurements.parsed, 0);
        assert_eq!(resolver.measurements.errors, 1);
    }

    #[test]
    fn quest_shape_inventory_is_deterministic_and_preserves_payload_shapes() {
        let interner = crate::sym::StringInterner::new();
        let plugin = interner.intern("Skyrim.esm");
        let mut first = Record::new(
            SigCode(*b"QUST"),
            FormKey {
                local: 0x100,
                plugin,
            },
        );
        first.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"VMAD"),
                value: FieldValue::Bytes(vec![0_u8; 12].into()),
            },
            FieldEntry {
                sig: SubrecordSig(*b"CTDA"),
                value: FieldValue::Bytes(vec![0_u8; 32].into()),
            },
        ]);
        let mut second = first.clone();
        second.form_key.local = 0x101;
        let report = inventory_quest_record_shapes(
            "skyrimse",
            vec!["Skyrim.esm".to_string()],
            &[second, first],
            BTreeMap::new(),
            &interner,
        );
        assert_eq!(report.records_scanned, 2);
        assert_eq!(report.shapes.len(), 1);
        assert_eq!(report.shapes[0].count, 2);
        assert!(report.shapes[0].fingerprint.contains("CTDA:bytes32x1"));
        assert!(report.shapes[0].fingerprint.contains("VMAD:bytes12x1"));
    }

    #[test]
    #[ignore = "requires explicit FNV, FO3, and Skyrim official-corpus data directories"]
    fn installed_official_quest_shape_inventory_requires_all_three_corpora() {
        let configurations = [
            (
                "fnv",
                "FNV_QUEST_CORPUS_DATA_DIR",
                &[
                    "FalloutNV.esm",
                    "DeadMoney.esm",
                    "HonestHearts.esm",
                    "OldWorldBlues.esm",
                    "LonesomeRoad.esm",
                    "GunRunnersArsenal.esm",
                ][..],
            ),
            (
                "fo3",
                "FO3_QUEST_CORPUS_DATA_DIR",
                &[
                    "Fallout3.esm",
                    "Anchorage.esm",
                    "ThePitt.esm",
                    "BrokenSteel.esm",
                    "PointLookout.esm",
                    "Zeta.esm",
                ][..],
            ),
            (
                "skyrimse",
                "SKYRIMSE_QUEST_CORPUS_DATA_DIR",
                &[
                    "Skyrim.esm",
                    "Update.esm",
                    "Dawnguard.esm",
                    "HearthFires.esm",
                    "Dragonborn.esm",
                ][..],
            ),
        ];
        let mut reports = Vec::new();
        for (game, variable, plugin_names) in configurations {
            let data_dir = std::env::var_os(variable)
                .map(PathBuf::from)
                .unwrap_or_else(|| panic!("explicit official-corpus audit requires {variable}"));
            assert!(
                data_dir.is_dir(),
                "{variable} is not a directory: {}",
                data_dir.display()
            );
            let plugin_paths = plugin_names
                .iter()
                .map(|plugin| data_dir.join(plugin))
                .collect::<Vec<_>>();
            for plugin_path in &plugin_paths {
                assert!(
                    plugin_path.is_file(),
                    "explicit official-corpus audit is missing {}",
                    plugin_path.display()
                );
            }
            let report = inventory_installed_quest_plugins(game, &plugin_paths).unwrap();
            assert!(report.records_scanned > 0, "{game}");
            assert!(!report.shapes.is_empty(), "{game}");
            assert_eq!(
                report.plugins,
                plugin_names
                    .iter()
                    .map(|plugin| (*plugin).to_string())
                    .collect::<Vec<_>>(),
                "{game} official master ledger"
            );
            let expected_decode_failures = match game {
                "fo3" => BTreeMap::from([
                    ("Anchorage.esm:INFO".to_string(), 1),
                    ("Anchorage.esm:QUST".to_string(), 1),
                    ("ThePitt.esm:INFO".to_string(), 1),
                    ("Zeta.esm:INFO".to_string(), 1),
                    ("Zeta.esm:QUST".to_string(), 1),
                ]),
                "fnv" | "skyrimse" => BTreeMap::new(),
                _ => unreachable!(),
            };
            assert_eq!(
                report.decode_failures, expected_decode_failures,
                "{game} decoder-failure ledger changed"
            );
            reports.push(report);
        }
        assert_eq!(reports.len(), 3);
        for report in &reports {
            eprintln!(
                "quest-shape-audit-summary game={} plugins={} records={} shapes={} decode_failures={}",
                report.source_game,
                report.plugins.len(),
                report.records_scanned,
                report.shapes.len(),
                report.decode_failures.values().sum::<usize>()
            );
            for (record_family, count) in &report.decode_failures {
                eprintln!(
                    "quest-shape-audit-decode-failure game={} family={} count={}",
                    report.source_game, record_family, count
                );
            }
        }
        if std::env::var_os("BACUP_QUEST_SHAPE_AUDIT_VERBOSE").is_some() {
            eprintln!("{}", serde_json::to_string_pretty(&reports).unwrap());
        }
    }

    fn value_identifier(value: &str) -> PexValuePayload {
        PexValuePayload {
            value_type: 1,
            data: json!(value),
        }
    }

    fn value_int(value: i64) -> PexValuePayload {
        PexValuePayload {
            value_type: 3,
            data: json!(value),
        }
    }

    fn property_binding(form_key: &str) -> Vec<PropertyFormBinding> {
        vec![PropertyFormBinding {
            form_key: form_key.to_string(),
            struct_member: None,
        }]
    }

    fn struct_property_binding(form_key: &str, member: &str) -> Vec<PropertyFormBinding> {
        vec![PropertyFormBinding {
            form_key: form_key.to_string(),
            struct_member: Some(member.to_string()),
        }]
    }

    fn function(name: &str, instructions: Vec<PexInstructionPayload>) -> PexFunctionPayload {
        PexFunctionPayload {
            name: name.to_string(),
            return_type: String::new(),
            docstring: String::new(),
            user_flags: 0,
            is_native: false,
            is_global: false,
            params: Vec::new(),
            locals: Vec::new(),
            instructions,
        }
    }

    fn pex_with_property(
        script_name: &str,
        property_name: &str,
        functions: Vec<PexFunctionPayload>,
    ) -> PexFilePayload {
        PexFilePayload {
            magic: 0,
            major_version: 3,
            minor_version: 9,
            game_id: 2,
            compilation_time: 0,
            source_filename: String::new(),
            username: String::new(),
            machine_name: String::new(),
            string_table: Vec::new(),
            debug_info: None,
            user_flags: Vec::new(),
            objects: vec![PexObjectPayload {
                name: script_name.to_string(),
                parent: String::new(),
                docstring: String::new(),
                is_const: false,
                auto_state: String::new(),
                structs: Vec::new(),
                user_flags: 0,
                variables: Vec::new(),
                guards: Vec::new(),
                properties: vec![PexPropertyPayload {
                    name: property_name.to_string(),
                    ty: "Keyword".to_string(),
                    docstring: String::new(),
                    user_flags: 0,
                    flags: 0,
                    auto_var: format!("::{property_name}_var"),
                    getter: None,
                    setter: None,
                }],
                states: vec![PexStatePayload {
                    name: String::new(),
                    functions,
                }],
            }],
        }
    }

    fn pex(script_name: &str, functions: Vec<PexFunctionPayload>) -> PexFilePayload {
        pex_with_property(script_name, "StoryEventKeyword", functions)
    }

    fn route(target_event: &str) -> StoryManagerRouteSeed {
        StoryManagerRouteSeed {
            route_id: "route".to_string(),
            source_quest: "400100@SeventySix.esm".to_string(),
            target_quest: Some("500100@SeventySix.esm".to_string()),
            quest_editor_id: Some("TestQuest".to_string()),
            source_node: Some("400200@SeventySix.esm".to_string()),
            target_node: Some("500200@SeventySix.esm".to_string()),
            source_root: Some("400210@SeventySix.esm".to_string()),
            target_root: Some("500210@SeventySix.esm".to_string()),
            target_event_root: Some("000111@Fallout4.esm".to_string()),
            source_quest_event: None,
            target_quest_event: None,
            source_root_event: Some(target_event.to_string()),
            target_event: Some(target_event.to_string()),
            source_start_keyword: None,
            source_metadata_fields: Vec::new(),
            source_selector_keywords: vec!["400300@SeventySix.esm".to_string()],
            target_selector_keywords: vec!["500300@SeventySix.esm".to_string()],
            bridge_keyword: None,
            emission_status: "emitted".to_string(),
            skip_reason: None,
            condition_valid: true,
            structural_status: "eligible".to_string(),
            producer_status: if target_event == "SCPT" {
                "unproven".to_string()
            } else {
                "not_required".to_string()
            },
            fo76_only_event: false,
            issues: Vec::new(),
        }
    }

    fn sender(
        scope: EvidenceScope,
        keyword: &str,
        provenance: Provenance,
        pex: Option<PexFilePayload>,
    ) -> ProducerEvidence {
        let script_name = if provenance == Provenance::PairAdapter {
            "B21:StoryEventOnTriggerEnter"
        } else {
            "LocalSender"
        };
        ProducerEvidence {
            scope,
            carrier: if scope == EvidenceScope::Source {
                "400400@SeventySix.esm".to_string()
            } else {
                "500400@SeventySix.esm".to_string()
            },
            carrier_signature: "REFR".to_string(),
            script_name: script_name.to_string(),
            property_bindings: BTreeMap::from([(
                "StoryEventKeyword".to_string(),
                property_binding(keyword),
            )]),
            bound_quest_fragments: HashSet::new(),
            pex: pex.map(Arc::new).or_else(|| {
                Some(Arc::new(super::tests::pex(
                    script_name,
                    vec![function(
                        if provenance == Provenance::PairAdapter {
                            "OnTriggerEnter"
                        } else {
                            "OnActivate"
                        },
                        vec![PexInstructionPayload {
                            opcode: OP_CALLMETHOD,
                            args: vec![
                                value_identifier("SendStoryEventAndWait"),
                                value_identifier("::StoryEventKeyword_var"),
                                value_identifier("None"),
                            ],
                        }],
                    )],
                )))
            }),
            placed: true,
            provenance,
            autostart: Some(false),
            local_source: true,
            runtime_issue: None,
            pex_error: None,
        }
    }

    fn unplaced_quest_sender(
        scope: EvidenceScope,
        keyword: &str,
        provenance: Provenance,
        entrypoint: &str,
        bound: bool,
    ) -> ProducerEvidence {
        let mut evidence = sender(scope, keyword, provenance, None);
        evidence.carrier = if scope == EvidenceScope::Source {
            "400100@SeventySix.esm".to_string()
        } else {
            "500100@SeventySix.esm".to_string()
        };
        evidence.carrier_signature = "QUST".to_string();
        evidence.placed = false;
        evidence.runtime_issue = Some("unproven_runtime_entry".to_string());
        let payload = json!({
            "Script Fragments": {
                "Script": {"ScriptName": evidence.script_name.clone()},
                "Fragments": [{
                    "Quest Stage": 9000,
                    "Quest Stage Index": 0,
                    "ScriptName": evidence.script_name.clone(),
                    "FragmentName": if bound {
                        entrypoint
                    } else {
                        "Fragment_Stage_0100_Item_00"
                    }
                }]
            }
        });
        let mut fragment_bindings = quest_fragment_bindings(&payload);
        evidence.bound_quest_fragments = fragment_bindings
            .remove(&normalize_script_name(&evidence.script_name))
            .unwrap_or_default();
        evidence.pex = Some(Arc::new(pex(
            &evidence.script_name,
            vec![function(
                entrypoint,
                vec![PexInstructionPayload {
                    opcode: OP_CALLMETHOD,
                    args: vec![
                        value_identifier("SendStoryEvent"),
                        value_identifier("::StoryEventKeyword_var"),
                        value_identifier("None"),
                    ],
                }],
            )],
        )));
        evidence
    }

    fn quest_metadata(autostart: Option<bool>) -> ProducerEvidence {
        ProducerEvidence {
            scope: EvidenceScope::Target,
            carrier: "500100@SeventySix.esm".to_string(),
            carrier_signature: "QUST".to_string(),
            script_name: String::new(),
            property_bindings: BTreeMap::new(),
            bound_quest_fragments: HashSet::new(),
            pex: None,
            placed: false,
            provenance: Provenance::Converted,
            autostart,
            local_source: true,
            runtime_issue: Some("unproven_runtime_entry".to_string()),
            pex_error: None,
        }
    }

    fn quest_metadata_for(carrier: &str, autostart: Option<bool>) -> ProducerEvidence {
        let mut evidence = quest_metadata(autostart);
        evidence.carrier = carrier.to_string();
        evidence
    }

    fn quest_fragment_sender_with_property_variant(
        carrier: &str,
        keyword: &str,
        vmad_property_name: &str,
        pex_property_name: &str,
    ) -> ProducerEvidence {
        let script_name = format!("Fragments:Quests:QF_Test_{carrier}");
        let entrypoint = "Fragment_Stage_9000_Item_00";
        ProducerEvidence {
            scope: EvidenceScope::Target,
            carrier: carrier.to_string(),
            carrier_signature: "QUST".to_string(),
            script_name: script_name.clone(),
            property_bindings: BTreeMap::from([(
                vmad_property_name.to_string(),
                property_binding(keyword),
            )]),
            bound_quest_fragments: HashSet::from([entrypoint.to_ascii_lowercase()]),
            pex: Some(Arc::new(pex_with_property(
                &script_name,
                pex_property_name,
                vec![function(
                    entrypoint,
                    vec![PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("SendStoryEvent"),
                            value_identifier(&format!("::{pex_property_name}_var")),
                            value_identifier("None"),
                        ],
                    }],
                )],
            ))),
            placed: false,
            provenance: Provenance::PersistentPatch,
            autostart: Some(false),
            local_source: true,
            runtime_issue: Some("unproven_runtime_entry".to_string()),
            pex_error: None,
        }
    }

    fn report(
        route: StoryManagerRouteSeed,
        evidence: &[ProducerEvidence],
    ) -> QuestRuntimeInventoryReport {
        build_report(
            &StoryManagerRouteSeedReport {
                schema_version: SCHEMA_VERSION,
                source_game: "fo76".to_string(),
                target_game: "fo4".to_string(),
                routes: vec![route],
            },
            evidence,
            &HashMap::new(),
        )
        .unwrap()
    }

    fn assert_indexed_route_matches_full_scan(
        route: StoryManagerRouteSeed,
        evidence: &[ProducerEvidence],
    ) {
        let unsupported = HashMap::new();
        let full = classify_route_full_scan(route.clone(), evidence, &unsupported).unwrap();
        let index = RouteEvidenceIndex::new(evidence);
        let source = index.source_for_route(&route);
        let target = index.target_for_route(&route);
        let indexed = classify_route(route, &source, &target, &unsupported).unwrap();
        assert_eq!(
            serde_json::to_value(full).unwrap(),
            serde_json::to_value(indexed).unwrap()
        );
    }

    #[test]
    fn route_evidence_index_matches_full_scan_for_duplicates_negatives_and_sibling_bypass() {
        let mut unrelated = sender(
            EvidenceScope::Target,
            "700300@SeventySix.esm",
            Provenance::Converted,
            None,
        );
        unrelated.carrier = "not-a-form-key".to_string();
        unrelated.pex = None;
        unrelated.pex_error = Some("missing".to_string());

        let adapted = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::PairAdapter,
            None,
        );
        let native = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::Converted,
            None,
        );
        let mut sibling_bypass = sender(
            EvidenceScope::Target,
            "700300@SeventySix.esm",
            Provenance::Converted,
            Some(pex(
                "LocalSender",
                vec![function(
                    "OnActivate",
                    vec![PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("StartQuest"),
                            value_identifier("self"),
                            value_identifier("None"),
                        ],
                    }],
                )],
            )),
        );
        sibling_bypass.carrier = adapted.carrier.clone();

        let evidence = vec![
            unrelated.clone(),
            adapted.clone(),
            native,
            adapted,
            sibling_bypass,
            quest_metadata(Some(false)),
            sender(
                EvidenceScope::Source,
                "400300@SeventySix.esm",
                Provenance::Source,
                None,
            ),
        ];
        assert_indexed_route_matches_full_scan(route("SCPT"), &evidence);

        let mut missing = unrelated;
        missing.property_bindings = BTreeMap::from([(
            "StoryEventKeyword".to_string(),
            property_binding("500300@SeventySix.esm"),
        )]);
        assert_indexed_route_matches_full_scan(route("SCPT"), &[missing, quest_metadata(None)]);

        assert_indexed_route_matches_full_scan(
            route("SCPT"),
            &[
                sender(
                    EvidenceScope::Source,
                    "400300@SeventySix.esm",
                    Provenance::Source,
                    None,
                ),
                quest_metadata(Some(false)),
            ],
        );
    }

    #[test]
    fn reachable_instructions_do_not_cross_return() {
        let function = function(
            "OnInit",
            vec![
                PexInstructionPayload {
                    opcode: OP_RETURN,
                    args: vec![value_identifier("None")],
                },
                PexInstructionPayload {
                    opcode: OP_CALLMETHOD,
                    args: vec![
                        value_identifier("StartQuest"),
                        value_identifier("self"),
                        value_identifier("None"),
                    ],
                },
            ],
        );
        assert_eq!(reachable_instruction_indexes(&function), HashSet::from([0]));
    }

    #[test]
    fn route_form_keys_are_normalized_for_native_plugin_indexes() {
        assert_eq!(
            plugin_index_form_key("405E14@SeventySix.esm").unwrap(),
            "SeventySix.esm:405E14"
        );
        assert_eq!(
            plugin_index_form_key("405E14:SeventySix.esm").unwrap(),
            "SeventySix.esm:405E14"
        );
        assert_eq!(
            plugin_index_form_key("SeventySix.esm:405E14").unwrap(),
            "SeventySix.esm:405E14"
        );
    }

    #[test]
    fn sender_proof_follows_local_self_helper() {
        let entrypoint = function(
            "OnTriggerEnter",
            vec![PexInstructionPayload {
                opcode: OP_CALLMETHOD,
                args: vec![
                    value_identifier("Send"),
                    value_identifier("self"),
                    value_identifier("None"),
                ],
            }],
        );
        let helper = function(
            "Send",
            vec![PexInstructionPayload {
                opcode: OP_CALLMETHOD,
                args: vec![
                    value_identifier("SendStoryEvent"),
                    value_identifier("::StoryEventKeyword_var"),
                    value_identifier("None"),
                ],
            }],
        );
        let calls = pex_calls(
            &pex("B21:Adapter", vec![entrypoint, helper]),
            "B21:Adapter",
            Some("StoryEventKeyword"),
            false,
        );
        assert!(calls.sends_story_event);
        assert_eq!(
            calls.first_sender_entrypoint.as_deref(),
            Some("OnTriggerEnter")
        );
    }

    #[test]
    fn unknown_start_receiver_fails_closed_as_quest_bypass() {
        let calls = pex_calls(
            &pex(
                "B21:Adapter",
                vec![function(
                    "OnInit",
                    vec![PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("Start"),
                            value_identifier("unknown"),
                            value_identifier("None"),
                        ],
                    }],
                )],
            ),
            "B21:Adapter",
            None,
            true,
        );
        assert!(calls.starts_quest_directly);
    }

    #[test]
    fn vmad_binding_collection_is_exact_and_deduplicated() {
        let payload = json!({
            "Scripts": [{
                "ScriptName": "B21:Adapter",
                "Properties": [{
                    "propertyName": "StoryEventKeyword",
                    "Value": {
                        "FormID": {
                            "reference": {
                                "plugin": "Patch.esp",
                                "object_id": "000123"
                            }
                        }
                    }
                }]
            }]
        });
        let bindings = vmad_script_bindings(&payload);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0, "B21:Adapter");
        assert_eq!(
            bindings[0].1["StoryEventKeyword"],
            property_binding("000123@Patch.esp")
        );
    }

    #[test]
    fn vmad_binding_collection_keeps_nested_terminal_menu_keyword() {
        let payload = json!({
            "Scripts": [{
                "ScriptName": "DefaultSendStoryEventOnMenuItemRun",
                "Properties": [{
                    "propertyName": "MenuData",
                    "Value": [[{
                        "memberName": "StoryEventToSend",
                        "Value": {
                            "FormID": {
                                "reference": {
                                    "plugin": "SeventySix.esm",
                                    "object_id": "2C3F0B"
                                }
                            }
                        }
                    }]]
                }]
            }]
        });

        let bindings = vmad_script_bindings(&payload);

        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings[0].1["MenuData"],
            struct_property_binding("2C3F0B@SeventySix.esm", "StoryEventToSend")
        );
    }

    #[test]
    fn vmad_binding_collection_keeps_every_nested_member_qualified_form() {
        let payload = json!({
            "Scripts": [{
                "ScriptName": "DefaultSendStoryEventOnMenuItemRun",
                "Properties": [{
                    "propertyName": "MenuData",
                    "Value": [[
                        {
                            "memberName": "ActiveQuestKeyword",
                            "Value": {"reference": {
                                "plugin": "SeventySix.esm",
                                "object_id": "111111"
                            }}
                        },
                        {
                            "memberName": "StoryEventToSend",
                            "Value": {"reference": {
                                "plugin": "SeventySix.esm",
                                "object_id": "2C3F0B"
                            }}
                        }
                    ]]
                }]
            }]
        });

        let bindings = vmad_script_bindings(&payload);

        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].1["MenuData"].len(), 2);
        assert!(bindings[0].1["MenuData"].contains(&PropertyFormBinding {
            form_key: "111111@SeventySix.esm".to_string(),
            struct_member: Some("ActiveQuestKeyword".to_string()),
        }));
        assert!(bindings[0].1["MenuData"].contains(&PropertyFormBinding {
            form_key: "2C3F0B@SeventySix.esm".to_string(),
            struct_member: Some("StoryEventToSend".to_string()),
        }));
    }

    #[test]
    fn papyrus_property_matching_ignores_case_and_underscores() {
        let calls = pex_calls(
            &pex_with_property(
                "Fragments:Quests:QF_W05_MQS_205P_0041CB6D",
                "W05_MQA_206P_QuestStartKeyword",
                vec![function(
                    "Fragment_Stage_9000_Item_00",
                    vec![PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("SendStoryEvent"),
                            value_identifier("::W05_MQA_206P_QuestStartKeyword_var"),
                            value_identifier("None"),
                        ],
                    }],
                )],
            ),
            "Fragments:Quests:QF_W05_MQS_205P_0041CB6D",
            Some("w05_mqa_206p_quest_start_keyword"),
            false,
        );

        assert!(calls.sends_story_event);
        assert_eq!(
            calls.first_sender_entrypoint.as_deref(),
            Some("Fragment_Stage_9000_Item_00")
        );
    }

    #[test]
    fn quest_fragment_binding_collection_uses_fragment_row_script_names() {
        let payload = json!({
            "Script Fragments": {
                "Script": {"ScriptName": "TopLevelScript"},
                "Fragments": [{
                    "Quest Stage": 9000,
                    "Quest Stage Index": 0,
                    "ScriptName": "Fragments:Quests:QF_TestQuest_00012345",
                    "FragmentName": "Fragment_Stage_9000_Item_00"
                }]
            }
        });

        let bindings = quest_fragment_bindings(&payload);

        assert_eq!(bindings.len(), 1);
        assert_eq!(
            bindings["fragments:quests:qf_testquest_00012345"],
            HashSet::from(["fragment_stage_9000_item_00".to_string()])
        );
        assert!(!bindings.contains_key("toplevelscript"));
    }

    #[test]
    fn jump_offsets_are_relative_to_current_instruction() {
        let function = function(
            "OnInit",
            vec![
                PexInstructionPayload {
                    opcode: OP_JMP,
                    args: vec![value_int(2)],
                },
                PexInstructionPayload {
                    opcode: OP_CALLMETHOD,
                    args: vec![
                        value_identifier("StartQuest"),
                        value_identifier("self"),
                        value_identifier("None"),
                    ],
                },
                PexInstructionPayload {
                    opcode: OP_RETURN,
                    args: vec![value_identifier("None")],
                },
            ],
        );
        assert_eq!(
            reachable_instruction_indexes(&function),
            HashSet::from([0, 2])
        );
    }

    #[test]
    fn native_engine_event_needs_quest_metadata_but_no_script_producer() {
        let report = report(route("CLOC"), &[quest_metadata(Some(false))]);
        assert_eq!(report.routes[0].classification, "native");
        assert!(report.coverage_complete);
    }

    #[test]
    fn missing_target_quest_metadata_fails_closed() {
        let report = report(route("CLOC"), &[]);
        assert_eq!(report.routes[0].classification, "unclassified");
        assert_eq!(report.routes[0].route.issues, vec!["autostart_unknown"]);
    }

    #[test]
    fn inventory_route_serializes_one_merged_issues_field() {
        let mut seed = route("CLOC");
        seed.issues.push("seed_issue".to_string());

        let report = report(seed, &[]);
        let serialized = serde_json::to_string(&report.routes[0]).unwrap();

        assert_eq!(serialized.matches("\"issues\":").count(), 1);
        assert_eq!(
            report.routes[0].route.issues,
            vec!["autostart_unknown", "seed_issue"]
        );
    }

    #[test]
    fn surviving_source_and_target_scpt_senders_are_native() {
        let evidence = [
            sender(
                EvidenceScope::Source,
                "400300@SeventySix.esm",
                Provenance::Source,
                None,
            ),
            sender(
                EvidenceScope::Target,
                "500300@SeventySix.esm",
                Provenance::Converted,
                None,
            ),
            quest_metadata(Some(false)),
        ];
        let report = report(route("SCPT"), &evidence);
        assert_eq!(report.routes[0].classification, "native");
        assert_eq!(report.routes[0].runtime_proof.len(), 2);
    }

    #[test]
    fn unplaced_quest_fragment_senders_prove_scpt_runtime_parity() {
        let entrypoint = "Fragment_Stage_9000_Item_00";
        let evidence = [
            unplaced_quest_sender(
                EvidenceScope::Source,
                "400300@SeventySix.esm",
                Provenance::Source,
                entrypoint,
                true,
            ),
            unplaced_quest_sender(
                EvidenceScope::Target,
                "500300@SeventySix.esm",
                Provenance::Converted,
                entrypoint,
                true,
            ),
            quest_metadata(Some(false)),
        ];

        let report = report(route("SCPT"), &evidence);

        assert_eq!(report.routes[0].classification, "native");
        assert_eq!(report.routes[0].runtime_proof.len(), 2);
        assert!(
            report.routes[0]
                .runtime_proof
                .iter()
                .all(|proof| proof.entrypoint == entrypoint)
        );
        assert!(report.routes[0].route.issues.is_empty());
    }

    #[test]
    fn w05_selector_only_routes_use_exact_vmad_form_keys_across_property_spellings() {
        for (producer, quest, selector, vmad_property, pex_property) in [
            (
                "405E14@SeventySix.esm",
                "41A39D@SeventySix.esm",
                "41A340@SeventySix.esm",
                "W05_MQ_003P_Muscle_QuestStartKeyword",
                "W05_MQ_003P_Muscle_QuestStart_Keyword",
            ),
            (
                "41A39D@SeventySix.esm",
                "41C976@SeventySix.esm",
                "41C979@SeventySix.esm",
                "W05_MQ_004P_Crane_QuestStartKeyword",
                "W05_MQ_004P_Crane_QuestStart_Keyword",
            ),
            (
                "41C976@SeventySix.esm",
                "3FBBB2@SeventySix.esm",
                "3FBBBB@SeventySix.esm",
                "W05_MQ_101P_QuestStartKeyword",
                "W05_MQ_101P_QuestStart_Keyword",
            ),
            (
                "3FBBB2@SeventySix.esm",
                "3FBC0D@SeventySix.esm",
                "3FBC0E@SeventySix.esm",
                "W05_MQ_101P_A_QuestStartKeyword",
                "W05_MQ_101P_A_QuestStart_Keyword",
            ),
            (
                "3FBBB2@SeventySix.esm",
                "3FBC10@SeventySix.esm",
                "3FBC0F@SeventySix.esm",
                "W05_MQ_101P_B_QuestStartKeyword",
                "W05_MQ_101P_B_QuestStart_Keyword",
            ),
            (
                "592500@SeventySix.esm",
                "40C458@SeventySix.esm",
                "40C45A@SeventySix.esm",
                "W05_MQS_204P_QuestStartKeyword",
                "W05_MQS_204P_QuestStart_Keyword",
            ),
            (
                "40C458@SeventySix.esm",
                "41CB6D@SeventySix.esm",
                "41CC41@SeventySix.esm",
                "W05_MQS_205P_QuestStartKeyword",
                "W05_MQS_205P_QuestStart_Keyword",
            ),
            (
                "41CB6D@SeventySix.esm",
                "54EDB9@SeventySix.esm",
                "54EF78@SeventySix.esm",
                "W05_MQA_206P_QuestStart_Keyword",
                "W05_MQA_206P_QuestStartKeyword",
            ),
            (
                "548B7A@SeventySix.esm",
                "54EDB9@SeventySix.esm",
                "54EF78@SeventySix.esm",
                "W05_MQA_206P_QuestStart_Keyword",
                "W05_MQA_206P_QuestStartKeyword",
            ),
            (
                "535E55@SeventySix.esm",
                "548B7A@SeventySix.esm",
                "548CFC@SeventySix.esm",
                "W05_MQR_205P_QuestStart_Keyword",
                "W05_MQR_205P_QuestStartKeyword",
            ),
            (
                "3FFC02@SeventySix.esm",
                "40D28D@SeventySix.esm",
                "40D47C@SeventySix.esm",
                "W05_MQR_201P_QuestStart_Keyword",
                "W05_MQR_201P_QuestStartKeyword",
            ),
            (
                "3FFC00@SeventySix.esm",
                "3F28C3@SeventySix.esm",
                "3F28C5@SeventySix.esm",
                "W05_MQS_201P_QuestStartKeyword",
                "W05_MQS_201P_QuestStart_Keyword",
            ),
        ] {
            let mut seed = route("SCPT");
            seed.source_quest = quest.to_string();
            seed.target_quest = Some(quest.to_string());
            seed.source_start_keyword = None;
            seed.source_selector_keywords = vec![selector.to_string()];
            seed.target_selector_keywords = vec![selector.to_string()];
            let evidence = [
                quest_fragment_sender_with_property_variant(
                    producer,
                    selector,
                    vmad_property,
                    pex_property,
                ),
                quest_metadata_for(quest, Some(false)),
            ];

            let report = report(seed, &evidence);

            assert_eq!(
                report.routes[0].classification, "adapted",
                "producer {producer} -> quest {quest}"
            );
            assert_eq!(report.routes[0].runtime_proof[0].carrier, producer);
            assert_eq!(
                report.routes[0].runtime_proof[0].keyword_property,
                vmad_property
            );
        }
    }

    #[test]
    fn w05_mqr_205p_a_remains_unproven_without_an_exact_sender() {
        let quest = "5588EF@SeventySix.esm";
        let selector = "559346@SeventySix.esm";
        let mut seed = route("SCPT");
        seed.source_quest = quest.to_string();
        seed.target_quest = Some(quest.to_string());
        seed.source_start_keyword = Some(selector.to_string());
        seed.source_selector_keywords = vec![selector.to_string()];
        seed.target_selector_keywords = vec![selector.to_string()];
        let mut property_only = quest_fragment_sender_with_property_variant(
            "54EDB9@SeventySix.esm",
            selector,
            "W05_MQR_205P_A_QuestStart_Keyword",
            "W05_MQR_205P_A_QuestStartKeyword",
        );
        Arc::make_mut(property_only.pex.as_mut().unwrap()).objects[0].states[0].functions[0]
            .instructions = vec![PexInstructionPayload {
            opcode: OP_RETURN,
            args: vec![value_identifier("None")],
        }];

        let report = report(
            seed,
            &[property_only, quest_metadata_for(quest, Some(false))],
        );

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"no_local_source_producer".to_string())
        );
    }

    #[test]
    fn w05_mqr_205p_a_is_proven_by_the_mqa206_stage9000_story_sender() {
        let quest = "5588EF@SeventySix.esm";
        let selector = "559346@SeventySix.esm";
        let producer = "54EDB9@SeventySix.esm";
        let mut seed = route("SCPT");
        seed.source_quest = quest.to_string();
        seed.target_quest = Some(quest.to_string());
        seed.source_start_keyword = Some(selector.to_string());
        seed.source_selector_keywords = vec![selector.to_string()];
        seed.target_selector_keywords = vec![selector.to_string()];
        let evidence = [
            quest_fragment_sender_with_property_variant(
                producer,
                selector,
                "W05_MQR_205P_A_QuestStart_Keyword",
                "W05_MQR_205P_A_QuestStartKeyword",
            ),
            quest_metadata_for(quest, Some(false)),
        ];

        let report = report(seed, &evidence);

        assert_eq!(report.routes[0].classification, "adapted");
        assert_eq!(report.routes[0].runtime_proof.len(), 1);
        assert_eq!(report.routes[0].runtime_proof[0].carrier, producer);
        assert_eq!(
            report.routes[0].runtime_proof[0].entrypoint,
            "Fragment_Stage_9000_Item_00"
        );
        assert_eq!(
            report.routes[0].runtime_proof[0].provenance,
            Provenance::PersistentPatch
        );
    }

    #[test]
    fn starting_an_unrelated_quest_does_not_bypass_the_story_manager_route() {
        let target_quest = "40C458@SeventySix.esm";
        let selector = "40C45A@SeventySix.esm";
        let mut seed = route("SCPT");
        seed.source_quest = target_quest.to_string();
        seed.target_quest = Some(target_quest.to_string());
        seed.source_start_keyword = None;
        seed.source_selector_keywords = vec![selector.to_string()];
        seed.target_selector_keywords = vec![selector.to_string()];
        let mut producer = quest_fragment_sender_with_property_variant(
            "592500@SeventySix.esm",
            selector,
            "W05_MQS_204P_QuestStartKeyword",
            "W05_MQS_204P_QuestStartKeyword",
        );
        producer.property_bindings.insert(
            "UnrelatedQuest".to_string(),
            property_binding("600600@SeventySix.esm"),
        );
        let producer_pex = Arc::make_mut(producer.pex.as_mut().unwrap());
        producer_pex.objects[0].properties.push(PexPropertyPayload {
            name: "UnrelatedQuest".to_string(),
            ty: "Quest".to_string(),
            docstring: String::new(),
            user_flags: 0,
            flags: 0,
            auto_var: "::UnrelatedQuest_var".to_string(),
            getter: None,
            setter: None,
        });
        producer_pex.objects[0].states[0].functions[0]
            .instructions
            .push(PexInstructionPayload {
                opcode: OP_CALLMETHOD,
                args: vec![
                    value_identifier("Start"),
                    value_identifier("::UnrelatedQuest_var"),
                    value_identifier("None"),
                ],
            });

        let unrelated_start_report = report(
            seed.clone(),
            &[
                producer.clone(),
                quest_metadata_for(target_quest, Some(false)),
            ],
        );
        assert_eq!(unrelated_start_report.routes[0].classification, "adapted");
        assert!(
            !unrelated_start_report.routes[0]
                .route
                .issues
                .contains(&"story_manager_bypass".to_string())
        );

        producer
            .property_bindings
            .insert("UnrelatedQuest".to_string(), property_binding(target_quest));
        let report = report(
            seed,
            &[producer, quest_metadata_for(target_quest, Some(false))],
        );
        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"story_manager_bypass".to_string())
        );
    }

    #[test]
    fn unbound_quest_fragment_senders_do_not_prove_scpt_runtime_parity() {
        let entrypoint = "Fragment_Stage_9000_Item_00";
        let evidence = [
            unplaced_quest_sender(
                EvidenceScope::Source,
                "400300@SeventySix.esm",
                Provenance::Source,
                entrypoint,
                false,
            ),
            unplaced_quest_sender(
                EvidenceScope::Target,
                "500300@SeventySix.esm",
                Provenance::Converted,
                entrypoint,
                false,
            ),
            quest_metadata(Some(false)),
        ];

        let report = report(route("SCPT"), &evidence);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"no_local_source_producer".to_string())
        );
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"unproven_runtime_entry".to_string())
        );
    }

    #[test]
    fn unplaced_non_fragment_quest_handlers_do_not_prove_scpt_runtime_parity() {
        let evidence = [
            unplaced_quest_sender(
                EvidenceScope::Source,
                "400300@SeventySix.esm",
                Provenance::Source,
                "OnActivate",
                true,
            ),
            unplaced_quest_sender(
                EvidenceScope::Target,
                "500300@SeventySix.esm",
                Provenance::Converted,
                "OnActivate",
                true,
            ),
            quest_metadata(Some(false)),
        ];

        let report = report(route("SCPT"), &evidence);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"no_local_source_producer".to_string())
        );
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"unproven_runtime_entry".to_string())
        );
    }

    #[test]
    fn terminal_placement_propagates_through_nested_submenus_only() {
        let reachable = propagate_terminal_placements(
            HashSet::from(["PlacedRoot".to_string()]),
            &HashMap::from([
                (
                    "FirstSubmenu".to_string(),
                    HashSet::from(["PlacedRoot".to_string()]),
                ),
                (
                    "SecondSubmenu".to_string(),
                    HashSet::from(["FirstSubmenu".to_string()]),
                ),
                (
                    "UnplacedCycleA".to_string(),
                    HashSet::from(["UnplacedCycleB".to_string()]),
                ),
                (
                    "UnplacedCycleB".to_string(),
                    HashSet::from(["UnplacedCycleA".to_string()]),
                ),
            ]),
        );

        assert!(reachable.contains("FirstSubmenu"));
        assert!(reachable.contains("SecondSubmenu"));
        assert!(!reachable.contains("UnplacedCycleA"));
        assert!(!reachable.contains("UnplacedCycleB"));
    }

    fn terminal_menu_sender(placed: bool, signature: &str) -> ProducerEvidence {
        let mut payload = pex(
            "DefaultSendStoryEventOnMenuItemRun",
            vec![function(
                "OnMenuItemRun",
                vec![
                    PexInstructionPayload {
                        opcode: OP_STRUCTGET,
                        args: vec![
                            value_identifier("::entryStoryEvent_var"),
                            value_identifier("entry"),
                            value_identifier(TERMINAL_MENU_STORY_MEMBER),
                        ],
                    },
                    PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("SendStoryEventAndWait"),
                            value_identifier("::entryStoryEvent_var"),
                            value_identifier("None"),
                        ],
                    },
                ],
            )],
        );
        payload.objects[0].properties[0].name = "MenuData".to_string();
        payload.objects[0].properties[0].ty =
            "DefaultSendStoryEventOnMenuItemRun#MenuDatum[]".to_string();
        payload.objects[0].properties[0].auto_var = "::MenuData_var".to_string();
        payload.objects[0].structs = vec![papyrus_core::pex::PexStructPayload {
            name: "MenuDatum".to_string(),
            members: vec![papyrus_core::pex::PexStructMemberPayload {
                name: TERMINAL_MENU_STORY_MEMBER.to_string(),
                ty: "Keyword".to_string(),
                user_flags: 0,
                data: value_identifier("None"),
                is_const: false,
                docstring: String::new(),
            }],
        }];
        payload.objects[0].states[0].functions[0].locals = vec![
            papyrus_core::pex::PexLocalPayload {
                name: "entry".to_string(),
                ty: TERMINAL_MENU_DATUM_TYPE.to_string(),
            },
            papyrus_core::pex::PexLocalPayload {
                name: "::entryStoryEvent_var".to_string(),
                ty: "Keyword".to_string(),
            },
        ];
        let mut evidence = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::PersistentPatch,
            Some(payload),
        );
        evidence.carrier_signature = signature.to_string();
        evidence.script_name = "DefaultSendStoryEventOnMenuItemRun".to_string();
        evidence.property_bindings = BTreeMap::from([(
            "MenuData".to_string(),
            struct_property_binding("500300@SeventySix.esm", TERMINAL_MENU_STORY_MEMBER),
        )]);
        evidence.placed = placed;
        evidence.runtime_issue = (!placed).then(|| "unplaced_carrier".to_string());
        evidence
    }

    #[test]
    fn placed_terminal_menu_sender_is_a_proven_runtime_entry() {
        let evidence = terminal_menu_sender(true, "TERM");

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "adapted");
        assert_eq!(
            report.routes[0].runtime_proof[0].entrypoint,
            "OnMenuItemRun"
        );
    }

    #[test]
    fn terminal_menu_sender_rejects_keyword_bound_to_a_different_struct_member() {
        let mut evidence = terminal_menu_sender(true, "TERM");
        evidence.property_bindings = BTreeMap::from([(
            "MenuData".to_string(),
            struct_property_binding("500300@SeventySix.esm", "ActiveQuestKeyword"),
        )]);

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
    }

    #[test]
    fn terminal_menu_sender_requires_the_exact_struct_declaration() {
        let mut evidence = terminal_menu_sender(true, "TERM");
        Arc::make_mut(evidence.pex.as_mut().unwrap()).objects[0].structs[0].members[0].name =
            "ActiveQuestKeyword".to_string();

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
    }

    #[test]
    fn terminal_menu_sender_requires_story_member_dataflow() {
        let mut evidence = terminal_menu_sender(true, "TERM");
        Arc::make_mut(evidence.pex.as_mut().unwrap()).objects[0].states[0].functions[0]
            .instructions[0]
            .args[2] = value_identifier("ActiveQuestKeyword");

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
    }

    #[test]
    fn terminal_menu_sender_requires_on_menu_item_run() {
        let mut evidence = terminal_menu_sender(true, "TERM");
        Arc::make_mut(evidence.pex.as_mut().unwrap()).objects[0].states[0].functions[0].name =
            "OnInit".to_string();

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
    }

    #[test]
    fn terminal_menu_sender_requires_the_exact_script() {
        let mut evidence = terminal_menu_sender(true, "TERM");
        evidence.script_name = "OtherTerminalSender".to_string();

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
    }

    #[test]
    fn unplaced_terminal_menu_sender_is_not_a_runtime_entry() {
        let evidence = terminal_menu_sender(false, "TERM");

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"unplaced_carrier".to_string())
        );
    }

    #[test]
    fn placed_non_terminal_menu_sender_is_not_a_runtime_entry() {
        let evidence = terminal_menu_sender(true, "ACHR");

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
    }

    #[test]
    fn book_on_equipped_sender_is_a_proven_inventory_runtime_entry() {
        let mut evidence = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::PersistentPatch,
            Some(pex(
                "LocalSender",
                vec![function(
                    "OnEquipped",
                    vec![PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("SendStoryEventAndWait"),
                            value_identifier("::StoryEventKeyword_var"),
                            value_identifier("None"),
                        ],
                    }],
                )],
            )),
        );
        evidence.carrier_signature = "BOOK".to_string();
        evidence.placed = false;
        evidence.runtime_issue = Some("unplaced_carrier".to_string());

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "adapted");
        assert_eq!(report.routes[0].runtime_proof[0].entrypoint, "OnEquipped");
    }

    #[test]
    fn unplaced_book_on_activate_is_not_an_inventory_runtime_entry() {
        let mut evidence = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::PersistentPatch,
            None,
        );
        evidence.carrier_signature = "BOOK".to_string();
        evidence.placed = false;
        evidence.runtime_issue = Some("unplaced_carrier".to_string());

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"unplaced_carrier".to_string())
        );
    }

    #[test]
    fn placed_non_book_on_equipped_is_not_an_inventory_runtime_entry() {
        let mut evidence = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::PersistentPatch,
            Some(pex(
                "LocalSender",
                vec![function(
                    "OnEquipped",
                    vec![PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("SendStoryEventAndWait"),
                            value_identifier("::StoryEventKeyword_var"),
                            value_identifier("None"),
                        ],
                    }],
                )],
            )),
        );
        evidence.carrier_signature = "ACHR".to_string();

        let report = report(route("SCPT"), &[evidence, quest_metadata(Some(false))]);

        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(report.routes[0].runtime_proof.is_empty());
    }

    #[test]
    fn exact_pair_adapter_is_sufficient_for_scpt_route() {
        let evidence = [
            sender(
                EvidenceScope::Target,
                "500300@SeventySix.esm",
                Provenance::PairAdapter,
                None,
            ),
            quest_metadata(Some(false)),
        ];
        let report = report(route("SCPT"), &evidence);
        assert_eq!(report.routes[0].classification, "adapted");
        assert_eq!(
            report.routes[0].runtime_proof[0].provenance,
            Provenance::PairAdapter
        );
    }

    #[test]
    fn target_quest_autostart_blocks_story_manager_route() {
        let evidence = [
            sender(
                EvidenceScope::Target,
                "500300@SeventySix.esm",
                Provenance::PairAdapter,
                None,
            ),
            quest_metadata(Some(true)),
        ];
        let report = report(route("SCPT"), &evidence);
        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"story_manager_bypass".to_string())
        );
    }

    #[test]
    fn autostart_helper_quest_sending_route_keyword_is_not_a_bypass() {
        let mut helper = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::PersistentPatch,
            None,
        );
        helper.carrier = "500500@SeventySix.esm".to_string();
        helper.carrier_signature = "QUST".to_string();
        helper.placed = false;
        helper.autostart = Some(true);
        let evidence = [
            sender(
                EvidenceScope::Target,
                "500300@SeventySix.esm",
                Provenance::PairAdapter,
                None,
            ),
            helper,
            quest_metadata(Some(false)),
        ];
        let report = report(route("SCPT"), &evidence);
        assert_eq!(report.routes[0].classification, "adapted");
        assert!(
            !report.routes[0]
                .route
                .issues
                .contains(&"story_manager_bypass".to_string())
        );
    }

    #[test]
    fn autostart_helper_alone_does_not_report_a_bypass() {
        let mut helper = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::PersistentPatch,
            None,
        );
        helper.carrier = "500500@SeventySix.esm".to_string();
        helper.carrier_signature = "QUST".to_string();
        helper.placed = false;
        helper.autostart = Some(true);
        let report = report(route("SCPT"), &[helper, quest_metadata(Some(false))]);
        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(
            !report.routes[0]
                .route
                .issues
                .contains(&"story_manager_bypass".to_string())
        );
    }

    #[test]
    fn direct_start_on_route_associated_sender_dominates_valid_story_send() {
        let bypass_pex = pex(
            "B21:StoryEventOnTriggerEnter",
            vec![function(
                "OnTriggerEnter",
                vec![
                    PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("SendStoryEvent"),
                            value_identifier("::StoryEventKeyword_var"),
                            value_identifier("None"),
                        ],
                    },
                    PexInstructionPayload {
                        opcode: OP_CALLMETHOD,
                        args: vec![
                            value_identifier("StartQuest"),
                            value_identifier("self"),
                            value_identifier("None"),
                        ],
                    },
                ],
            )],
        );
        let evidence = [
            sender(
                EvidenceScope::Target,
                "500300@SeventySix.esm",
                Provenance::PairAdapter,
                Some(bypass_pex),
            ),
            quest_metadata(Some(false)),
        ];
        let report = report(route("SCPT"), &evidence);
        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"story_manager_bypass".to_string())
        );
    }

    #[test]
    fn script_conversion_disabled_is_reported_for_bound_adapter() {
        let mut adapter = sender(
            EvidenceScope::Target,
            "500300@SeventySix.esm",
            Provenance::PairAdapter,
            None,
        );
        adapter.pex = None;
        adapter.pex_error = Some("script_conversion_disabled".to_string());
        let report = report(route("SCPT"), &[adapter, quest_metadata(Some(false))]);
        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"script_conversion_disabled".to_string())
        );
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"missing_compiled_pex".to_string())
        );
    }

    #[test]
    fn structural_failure_is_not_overridden_by_producer_evidence() {
        let mut invalid = route("SCPT");
        invalid.condition_valid = false;
        let report = report(
            invalid,
            &[
                sender(
                    EvidenceScope::Target,
                    "500300@SeventySix.esm",
                    Provenance::PairAdapter,
                    None,
                ),
                quest_metadata(Some(false)),
            ],
        );
        assert_eq!(report.routes[0].classification, "unclassified");
        assert!(
            report.routes[0]
                .route
                .issues
                .contains(&"structural_invalid".to_string())
        );
    }
}
