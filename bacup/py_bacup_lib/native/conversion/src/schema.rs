//! `AuthoringSchema` — a lightweight parsed view of the `AUTHORING_SCHEMA_JSON`
//! constants from `generated/*.rs`. Caches parsed schemas per game name in a
//! process-wide `OnceLock<HashMap>` so the ~600 kB JSON is only deserialized
//! once per game per process.

use rustc_hash::FxHashMap;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

// ---------------------------------------------------------------------------
// JSON structures (minimal subset needed by read_record)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SubrecordDef {
    /// The 4-byte subrecord signature, e.g. "EDID".
    pub id: String,
    /// "parsed" or "raw".
    #[serde(default = "default_kind")]
    pub kind: String,
    /// The codec name, e.g. "zstring", "uint32", "formid", "struct:I", etc.
    #[serde(default)]
    pub codec: Option<String>,
    /// Named fields within this subrecord (for struct codecs).
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    /// Subrecord-level union variants. These are used by a few large records
    /// whose payload shape depends on record context, such as EFSH.DNAM.
    #[serde(default)]
    pub union_variants: Vec<FieldDef>,
    /// Whether this subrecord may appear multiple times.
    #[serde(default, alias = "repeatable")]
    pub multiple: bool,
    /// Grouping scope for repeated subrecord blocks, e.g. SPEL effects.
    #[serde(default)]
    pub scope_id: Option<String>,
    /// Whether this subrecord is required.
    #[serde(default)]
    pub required: bool,
    /// Whether this contains a localized string.
    #[serde(default)]
    pub localized: bool,
}

fn default_kind() -> String {
    "parsed".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub id: String,
    /// The codec kind for this specific field, e.g. "zstring", "uint32", "formid", etc.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub codec: Option<String>,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    #[serde(default)]
    pub union_variants: Vec<FieldDef>,
    #[serde(default)]
    pub display_label: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecordDef {
    /// The 4-byte record signature, e.g. "WEAP".
    pub id: String,
    /// Subrecord definitions indexed by their `id`.
    #[serde(default)]
    pub subrecords: Vec<SubrecordDef>,
    #[serde(default)]
    pub display_label: Option<String>,
}

impl RecordDef {
    /// Look up a subrecord definition by its 4-byte signature string.
    pub fn subrecord_def(&self, sig: &str) -> Option<&SubrecordDef> {
        self.subrecords.iter().find(|s| s.id == sig)
    }
}

// Top-level JSON shape.
#[derive(Debug, Deserialize)]
struct AuthoringSchemaJson {
    #[serde(default)]
    records: Vec<RecordDef>,
}

// ---------------------------------------------------------------------------
// AuthoringSchema
// ---------------------------------------------------------------------------

/// Parsed view of one game's authoring schema. Immutable after construction.
///
/// The `records` map is the lightweight decode view used by `read_record`.
/// Metadata that the conversion fixups validate against (enums, reference
/// target sets, header-flag masks) is NOT re-parsed here — it delegates to the
/// authoritative `esp_authoring_core::schema::CompiledSchema` via `compiled`,
/// so there is exactly one parse of the enum/target/flag metadata.
pub struct AuthoringSchema {
    /// Record defs indexed by the 4-byte signature string.
    records: HashMap<String, IndexedRecordDef>,
    /// Shared authoritative schema for enum/target/flag metadata.
    compiled: Arc<esp_authoring_core::plugin_runtime::CompiledSchema>,
}

/// Starfield's `BFCB`…`BFCE` component runs reuse record-level 4CCs with
/// unrelated payloads, and the generated schema lists every component
/// definition ahead of the record-level ones.
const COMPONENT_SCOPE: &str = "base_form_components";

pub(crate) struct IndexedRecordDef {
    definition: RecordDef,
    subrecord_positions: FxHashMap<String, usize>,
    component_positions: FxHashMap<String, usize>,
    record_level_positions: FxHashMap<String, usize>,
}

impl IndexedRecordDef {
    fn new(definition: RecordDef) -> Self {
        let mut subrecord_positions = FxHashMap::default();
        let mut component_positions = FxHashMap::default();
        let mut record_level_positions = FxHashMap::default();
        for (position, subrecord) in definition.subrecords.iter().enumerate() {
            // Overloaded signatures retain the decoder's first-definition rule.
            subrecord_positions
                .entry(subrecord.id.clone())
                .or_insert(position);
            let scoped_positions = if subrecord.scope_id.as_deref() == Some(COMPONENT_SCOPE) {
                &mut component_positions
            } else {
                &mut record_level_positions
            };
            scoped_positions
                .entry(subrecord.id.clone())
                .or_insert(position);
        }
        Self {
            definition,
            subrecord_positions,
            component_positions,
            record_level_positions,
        }
    }

    pub(crate) fn subrecord_def(&self, sig: &str) -> Option<&SubrecordDef> {
        self.subrecord_positions
            .get(sig)
            .map(|&position| &self.definition.subrecords[position])
    }

    /// Resolve an overloaded signature by whether the source reader is inside
    /// a component run, falling back to the first definition.
    pub(crate) fn subrecord_def_in_component_run(
        &self,
        sig: &str,
        in_component_run: bool,
    ) -> Option<&SubrecordDef> {
        let scoped_positions = if in_component_run {
            &self.component_positions
        } else {
            &self.record_level_positions
        };
        scoped_positions
            .get(sig)
            .or_else(|| self.subrecord_positions.get(sig))
            .map(|&position| &self.definition.subrecords[position])
    }
}

impl AuthoringSchema {
    fn from_json(
        game: &str,
        json: &str,
        compiled: Arc<esp_authoring_core::plugin_runtime::CompiledSchema>,
    ) -> Result<Arc<Self>, String> {
        let parsed: AuthoringSchemaJson =
            serde_json::from_str(json).map_err(|e| format!("schema parse error: {e}"))?;
        let mut records = HashMap::with_capacity(parsed.records.len());
        for mut rec in parsed.records {
            if game == "fo4" && matches!(rec.id.as_str(), "WEAP" | "ARMO") {
                if let Some(damage) = rec.subrecords.iter_mut().find(|def| def.id == "DAMA") {
                    // The maximal schema includes a curve FormID from version 152;
                    // conversion writes FO4 version 131, where each row is 8 bytes.
                    damage.codec = Some("array_struct:I,I".to_string());
                    damage.fields.truncate(2);
                }
            }
            records.insert(rec.id.clone(), IndexedRecordDef::new(rec));
        }
        Ok(Arc::new(AuthoringSchema { records, compiled }))
    }

    /// Return the record definition for `sig` (e.g. "WEAP"), or `None` if the
    /// schema has no entry for that signature.
    pub fn record_def(&self, sig: &str) -> Option<&RecordDef> {
        self.records.get(sig).map(|record| &record.definition)
    }

    pub(crate) fn record_lookup(&self, sig: &str) -> Option<&IndexedRecordDef> {
        self.records.get(sig)
    }

    pub fn record_signatures(&self) -> impl Iterator<Item = &str> {
        self.records.keys().map(String::as_str)
    }

    // --- Conversion-fixup metadata accessors (delegate to CompiledSchema) ---

    /// Resolve an enum definition by `enum_ref`. conv-flags reads
    /// `is_flags()` / `valid_flag_mask()` / `contains_value()` /
    /// `fallback_value()` off the result for Class A masking and clamping.
    pub fn enum_def(
        &self,
        enum_ref: &str,
    ) -> Option<&esp_authoring_core::plugin_runtime::SchemaEnumJson> {
        self.compiled.enum_def(enum_ref)
    }

    /// Record-header valid-flag mask for `sig`. None ⇒ no flag metadata was
    /// captured for this record — callers MUST NOT strip header bits (warn
    /// only) so a valid-but-unmodeled FO4 bit is never silently dropped.
    /// Permissive records (xEdit `(True,True)`) report `0xFFFF_FFFF`.
    pub fn header_flag_mask(&self, sig: &str) -> Option<u32> {
        self.compiled
            .record_def(sig)
            .and_then(|r| r.record_flags())
            .map(|rf| rf.valid_mask())
    }

    /// Full record-flag metadata for `sig` (strip()/invalid_bits()/permissive).
    pub fn record_flags(
        &self,
        sig: &str,
    ) -> Option<&esp_authoring_core::plugin_runtime::SchemaRecordFlagsJson> {
        self.compiled.record_def(sig).and_then(|r| r.record_flags())
    }

    /// THE shared reference-target accessor used by conv-refs Pass 2 and the
    /// validator. `field_path` is the subrecord sig, or
    /// `"<SUB>.<field_id>"` for a field inside a struct codec.
    pub fn allowed_targets(
        &self,
        record_sig: &str,
        field_path: &str,
    ) -> Option<esp_authoring_core::plugin_runtime::RefTargetSpec<'_>> {
        self.compiled.allowed_targets(record_sig, field_path)
    }

    /// THE shared enum-ref locator (enum analogue of `allowed_targets`).
    /// Returns the `enum_ref` id; pass it to `enum_def`, or use `enum_def_at`.
    ///
    /// SCOPE-BLIND — see `esp_authoring_core`'s `enum_ref_at`. Masking/clamping
    /// callers must use `enum_def_at_in_scope`.
    pub fn enum_ref_at(&self, record_sig: &str, field_path: &str) -> Option<&str> {
        self.compiled.enum_ref_at(record_sig, field_path)
    }

    /// Resolve `enum_ref_at` straight to the enum definition.
    pub fn enum_def_at(
        &self,
        record_sig: &str,
        field_path: &str,
    ) -> Option<&esp_authoring_core::plugin_runtime::SchemaEnumJson> {
        self.compiled.enum_def_at(record_sig, field_path)
    }

    /// Scope-aware `enum_def_at` — required for scope-overloaded subrecords
    /// (QUST.FNAM objectives vs aliases).
    pub fn enum_def_at_in_scope(
        &self,
        record_sig: &str,
        field_path: &str,
        scope: Option<&str>,
    ) -> Option<&esp_authoring_core::plugin_runtime::SchemaEnumJson> {
        self.compiled
            .enum_def_at_in_scope(record_sig, field_path, scope)
    }

    /// Authoritative compiled record def for `sig` (subrecord/field `enum_ref`s,
    /// `union_variants`, `display_label`s). The conversion crate's own minimal
    /// `RecordDef` drops `enum_ref`, so the Class A masking pass walks this to
    /// enumerate enum-bearing paths, then matches each against the decoded
    /// record by `id` or `display_label`.
    pub fn compiled_record_def(
        &self,
        sig: &str,
    ) -> Option<&esp_authoring_core::plugin_runtime::SchemaRecordJson> {
        self.compiled.record_def(sig)
    }

    /// Shared struct-field byte layout: each field of a `struct:`-codec
    /// subrecord with its byte offset/width, enum_ref and formlink_targets, keyed
    /// by "<SUB>.<field_id>". conv-flags (nested flag/enum masking at `offset`),
    /// conv-refs (nested FK validation) and the validator (nested A1/A2/D) all
    /// iterate this, so they cover the same fields.
    pub fn struct_field_layout(
        &self,
        record_sig: &str,
        subrecord_sig: &str,
    ) -> Vec<esp_authoring_core::plugin_runtime::StructFieldInfo<'_>> {
        self.compiled.struct_field_layout(record_sig, subrecord_sig)
    }

    /// UNION-AWARE struct-field layout. For a `record_form_version` union
    /// subrecord (e.g. EFSH.DNAM) the active variant — hence field
    /// offsets/widths — depends on the record's form_version. Pass the
    /// ParsedRecord's `form_version` so the masking/validation walk targets the
    /// correct variant's bytes. `None` falls back to the legacy/first variant.
    pub fn struct_field_layout_versioned(
        &self,
        record_sig: &str,
        subrecord_sig: &str,
        form_version: Option<u16>,
    ) -> Vec<esp_authoring_core::plugin_runtime::StructFieldInfo<'_>> {
        self.compiled
            .struct_field_layout_versioned(record_sig, subrecord_sig, form_version)
    }

    /// THE shared flag-field enumerator. Yields every flag-storage
    /// enum field in a record as (field_path, &SchemaEnumJson), flattening struct
    /// codecs (subrecord-level + struct sub-fields). conv-flags' Class A pass
    /// iterates this and masks each; validator detects over the same set. Pair
    /// with `struct_field_layout` for the byte offset/width of each path.
    pub fn iter_flag_fields(
        &self,
        record_sig: &str,
    ) -> Vec<(String, &esp_authoring_core::plugin_runtime::SchemaEnumJson)> {
        self.compiled.iter_flag_fields(record_sig)
    }

    /// xEdit `.SetRequired` for a subrecord — conv-refs uses this to decide
    /// strip-optional vs leave-for-NULL-where-required. None ⇒ subrecord/record
    /// not in schema.
    pub fn subrecord_required(&self, record_sig: &str, subrecord_sig: &str) -> Option<bool> {
        let rec = self.compiled.record_def(record_sig)?;
        rec.subrecords
            .iter()
            .find(|s| s.id == subrecord_sig)
            .map(|s| s.required())
    }

    /// Obtain the parsed schema for `game`. The first call per game name
    /// deserializes the JSON; subsequent calls return the cached `Arc`.
    ///
    /// Supported game names: `fo4`, `fo76`, `skyrimse`, `starfield`, `fnv`,
    /// `fo3`, `oblivion`.
    pub fn for_game(game: &str) -> Result<Arc<Self>, String> {
        static CACHE: OnceLock<Mutex<HashMap<String, Arc<AuthoringSchema>>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        {
            let guard = cache.lock().unwrap();
            if let Some(schema) = guard.get(game) {
                return Ok(schema.clone());
            }
        }
        // Deserialize outside the lock to avoid holding it during expensive JSON parse.
        let json = json_for_game(game).ok_or_else(|| format!("unsupported game: {game:?}"))?;
        let compiled = esp_authoring_core::plugin_runtime::compiled_schema_for_game_str(game)?;
        let schema = Self::from_json(game, json, compiled)?;
        let mut guard = cache.lock().unwrap();
        // Another thread might have populated the cache while we were parsing.
        Ok(guard.entry(game.to_string()).or_insert(schema).clone())
    }
}

/// Map game name → the static JSON string from the generated module.
fn json_for_game(game: &str) -> Option<&'static str> {
    // Dispatch to generated constants via the schema_registry that already
    // handles per-game lookup. We call the same path the existing code uses.
    esp_authoring_core::schema_registry::schema_json_for_game(game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_subrecords_preserve_every_game_definition_and_first_match() {
        for game in [
            "fo76",
            "fo4",
            "skyrimse",
            "fnv",
            "fo3",
            "starfield",
            "oblivion",
        ] {
            let schema = AuthoringSchema::for_game(game).unwrap();
            for signature in schema.record_signatures() {
                let definition = schema.record_def(signature).unwrap();
                let indexed = schema.record_lookup(signature).unwrap();
                for subrecord in &definition.subrecords {
                    let old = definition.subrecord_def(&subrecord.id).unwrap();
                    let new = indexed.subrecord_def(&subrecord.id).unwrap();
                    assert!(
                        std::ptr::eq(old, new),
                        "{game} {signature}.{}",
                        subrecord.id
                    );
                }
                for missing in ["", "XYZ", "UNKNOWN", "\0\0\0\0"] {
                    assert_eq!(
                        definition.subrecord_def(missing).is_some(),
                        indexed.subrecord_def(missing).is_some()
                    );
                }
            }
            assert!(schema.record_lookup("UNKNOWN").is_none());
        }
        let definition: RecordDef = serde_json::from_value(serde_json::json!({
            "id":"TEST", "subrecords":[{"id":"TNAM","codec":"float32"},{"id":"TNAM","codec":"formid"},{"id":"FULL","localized":true}]
        })).unwrap();
        let indexed = IndexedRecordDef::new(definition);
        assert_eq!(
            indexed.subrecord_def("TNAM").unwrap().codec.as_deref(),
            Some("float32")
        );
        assert!(indexed.subrecord_def("FULL").unwrap().localized);
    }

    #[test]
    #[ignore = "requires SCHEMA_LOOKUP_PLUGIN, SCHEMA_LOOKUP_GAME and SCHEMA_LOOKUP_REPORT"]
    fn indexed_subrecords_match_complete_plugin_corpus() {
        use crate::store2::source::SourceEsm;
        use esp_authoring_core::plugin_runtime::effective_subrecords_for_record;
        use std::time::Instant;
        let path = std::env::var("SCHEMA_LOOKUP_PLUGIN").unwrap();
        let game = std::env::var("SCHEMA_LOOKUP_GAME").unwrap();
        let schema = AuthoringSchema::for_game(&game).unwrap();
        let source = SourceEsm::open(std::path::Path::new(&path)).unwrap();
        let (mut old_secs, mut new_secs, mut subrecords, mut missing) = (0.0, 0.0, 0usize, 0usize);
        for (index, entry) in source.index.iter().enumerate() {
            let parsed = source.view_at(index).unwrap().to_parsed_record().unwrap();
            let signature = std::str::from_utf8(&entry.sig).unwrap();
            let rows = effective_subrecords_for_record(&parsed);
            let old = schema.record_def(signature);
            let new = schema.record_lookup(signature);
            let legacy = || {
                let started = Instant::now();
                let values = rows
                    .iter()
                    .map(|row| {
                        old.and_then(|record| record.subrecord_def(row.signature.as_str()))
                            .map(|def| def as *const SubrecordDef)
                    })
                    .collect::<Vec<_>>();
                (values, started.elapsed().as_secs_f64())
            };
            let indexed = || {
                let started = Instant::now();
                let values = rows
                    .iter()
                    .map(|row| {
                        new.and_then(|record| record.subrecord_def(row.signature.as_str()))
                            .map(|def| def as *const SubrecordDef)
                    })
                    .collect::<Vec<_>>();
                (values, started.elapsed().as_secs_f64())
            };
            let ((before, before_secs), (after, after_secs)) = if index % 2 == 0 {
                let after = indexed();
                (legacy(), after)
            } else {
                let before = legacy();
                (before, indexed())
            };
            assert_eq!(before, after, "record {index} {signature}");
            old_secs += before_secs;
            new_secs += after_secs;
            subrecords += rows.len();
            missing += before.iter().filter(|value| value.is_none()).count();
        }
        let report = serde_json::json!({"plugin":path,"game":game,"records":source.record_count(),"subrecords":subrecords,"unknown_definitions":missing,"identical_definition_identity":true,"legacy_seconds":old_secs,"indexed_seconds":new_secs});
        std::fs::write(
            std::env::var("SCHEMA_LOOKUP_REPORT").unwrap(),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        eprintln!("{report}");
    }

    #[test]
    fn parse_fo4_authoring_schema() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let weap = schema.record_def("WEAP").expect("WEAP record_def");
        assert!(
            weap.subrecord_def("EDID").is_some(),
            "WEAP must have EDID subrecord def"
        );
    }

    #[test]
    fn parse_fo4_schema_cached_is_same_arc() {
        let a = AuthoringSchema::for_game("fo4").expect("fo4 schema first call");
        let b = AuthoringSchema::for_game("fo4").expect("fo4 schema second call");
        // Same backing Arc pointer.
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    fn unsupported_game_returns_error() {
        let result = AuthoringSchema::for_game("nonexistent_game_xyz");
        assert!(result.is_err());
    }
}
