//! `AuthoringSchema` — a lightweight parsed view of the `AUTHORING_SCHEMA_JSON`
//! constants from `generated/*.rs`. Caches parsed schemas per game name in a
//! process-wide `OnceLock<HashMap>` so the ~600 kB JSON is only deserialized
//! once per game per process.

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
    records: HashMap<String, RecordDef>,
    /// Shared authoritative schema for enum/target/flag metadata.
    compiled: Arc<esp_authoring_core::plugin_runtime::CompiledSchema>,
}

impl AuthoringSchema {
    fn from_json(
        json: &str,
        compiled: Arc<esp_authoring_core::plugin_runtime::CompiledSchema>,
    ) -> Result<Arc<Self>, String> {
        let parsed: AuthoringSchemaJson =
            serde_json::from_str(json).map_err(|e| format!("schema parse error: {e}"))?;
        let mut records = HashMap::with_capacity(parsed.records.len());
        for rec in parsed.records {
            records.insert(rec.id.clone(), rec);
        }
        Ok(Arc::new(AuthoringSchema { records, compiled }))
    }

    /// Return the record definition for `sig` (e.g. "WEAP"), or `None` if the
    /// schema has no entry for that signature.
    pub fn record_def(&self, sig: &str) -> Option<&RecordDef> {
        self.records.get(sig)
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
    /// conv-flags' Class A clamp and the validator's A2 check use this SAME
    /// mechanism so they agree on which field carries which enum. Returns the
    /// `enum_ref` id; pass it to `enum_def`, or use `enum_def_at` directly.
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

    /// THE shared struct-field byte-offset layout. Yields each
    /// field of a `struct:`-codec subrecord with its byte offset/width + enum_ref
    /// + formlink_targets, keyed by "<SUB>.<field_id>". conv-flags masks nested
    /// flag/enum bits in the raw subrecord bytes at `offset`; conv-refs validates
    /// nested FKs; validator detects nested A1/A2/D. All three iterate THIS, so
    /// they cover the identical fields and agree by construction.
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
        let schema = Self::from_json(json, compiled)?;
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
