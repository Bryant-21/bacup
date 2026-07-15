//! Stub-record injection for fixup passes that need to materialise a target-game
//! record on demand (e.g. `_sweep_unmapped_formkeys` Branch 10).
//!

//!
//! Builds a minimal EDID-only target record and inserts it into the target
//! plugin handle; the source-YAML / cross-reference resolution path is not
//! implemented (gated off — see `resolve_injected_stub_refs.rs`).

use crate::errors::WriteError;
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::target_write::add_record_native;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors emitted by `inject_minimal_stub`.
#[derive(Debug)]
pub enum StubInjectError {
    /// `add_record_native` rejected the insertion (e.g. no plugin handle).
    Write(WriteError),
    /// Session-scoped schema or insertion plumbing failed.
    Session(String),
    /// EDID subrecord sig could not be constructed (should never fail in
    /// practice — "EDID" is a valid 4-byte ASCII sig).
    EdidSigError(String),
}

impl std::fmt::Display for StubInjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Write(e) => write!(f, "stub insert failed: {e}"),
            Self::Session(m) => write!(f, "stub session error: {m}"),
            Self::EdidSigError(m) => write!(f, "stub EDID sig error: {m}"),
        }
    }
}

impl std::error::Error for StubInjectError {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Allocate a target FormKey for `source_fk` and insert a minimal stub record
/// into the target plugin handle.
///
/// The stub carries a single `EDID` subrecord set to `editor_id`. The
/// allocator picks the next free target object-id via
/// `FormKeyMapper::allocate_or_resolve`, which respects the mapper's
/// `preserve_source_ids` option (Python's `source_id_preserved` strategy when
/// the source id is still free, `new_allocation` otherwise). The
/// (source → target) mapping is registered on the mapper so subsequent
/// references resolve to the same target FK.
///
/// An empty `editor_id` produces a stub with no EDID subrecord.
///
/// # Returns
/// The newly-allocated target FormKey. The caller should overwrite any
/// references that pointed at `source_fk` with this value (or rely on a
/// subsequent rewrite pass to do so via the registered mapping).
///
/// # Errors
/// Returns `StubInjectError::Write` when `add_record_native` fails (e.g. the
/// target handle is no longer loaded). `StubInjectError::EdidSigError` is a
/// defensive case that should not happen in practice.
pub fn inject_minimal_stub(
    target_handle_id: u64,
    source_fk: FormKey,
    editor_id: &str,
    record_sig: SigCode,
    mapper: &mut FormKeyMapper,
    schema_target: &AuthoringSchema,
) -> Result<FormKey, StubInjectError> {
    let record = build_minimal_stub_record(source_fk, editor_id, record_sig, mapper)?;
    let target_fk = record.form_key;

    // Insert into target handle.
    add_record_native(target_handle_id, record, schema_target, mapper.interner)
        .map_err(StubInjectError::Write)?;

    Ok(target_fk)
}

pub fn inject_minimal_stub_with_session(
    session: &mut PluginSession,
    source_fk: FormKey,
    editor_id: &str,
    record_sig: SigCode,
    mapper: &mut FormKeyMapper,
) -> Result<FormKey, StubInjectError> {
    let schema = session
        .schema()
        .map_err(|err| StubInjectError::Session(err.to_string()))?;
    let record = build_minimal_stub_record(source_fk, editor_id, record_sig, mapper)?;
    let target_fk = record.form_key;
    session
        .add_record(record, schema.as_ref(), mapper.interner)
        .map_err(|err| StubInjectError::Session(err.to_string()))?;
    Ok(target_fk)
}

fn build_minimal_stub_record(
    source_fk: FormKey,
    editor_id: &str,
    record_sig: SigCode,
    mapper: &mut FormKeyMapper,
) -> Result<Record, StubInjectError> {
    // Allocate target FK via the mapper. Honours preserve_source_ids when set
    // (mirrors Python's `source_id_preserved` strategy) and falls back to
    // sequential allocation otherwise (`new_allocation`).
    let eid_sym = if editor_id.is_empty() {
        None
    } else {
        Some(mapper.interner.intern(editor_id))
    };
    let target_fk = mapper.allocate_or_resolve(source_fk, eid_sym, record_sig);

    // Build minimal Record: EDID subrecord only. Mirrors Python's
    // `raw_yaml = {"EditorID": editor_id}` minimal-stub fallback.
    let edid_sig = SubrecordSig::from_str("EDID").map_err(StubInjectError::EdidSigError)?;
    let mut record = Record::new(record_sig, target_fk);
    record.flags = RecordFlags::empty();
    if let Some(sym) = eid_sym {
        record.eid = Some(sym);
        record.fields.push(FieldEntry {
            sig: edid_sig,
            value: FieldValue::String(sym),
        });
    }
    Ok(record)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions, ResolutionMode};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

    fn fo4_schema() -> std::sync::Arc<AuthoringSchema> {
        AuthoringSchema::for_game("fo4").expect("fo4 schema")
    }

    fn new_target_handle(name: &str) -> Option<u64> {
        // plugin_handle_new_native needs a Python runtime in some build configs;
        // skip the test gracefully when unavailable.
        plugin_handle_new_native(name, Some("fo4")).ok()
    }

    /// Insert a single stub via `new_allocation`: source object-id is the same as
    /// `FIRST_ALLOCATION_ID`, so the allocator picks a fresh id. The returned
    /// target FK is registered on the mapper.
    #[test]
    fn inject_minimal_stub_new_allocation_registers_mapping() {
        let handle = match new_target_handle("StubInjectNewAlloc.esm") {
            Some(h) => h,
            None => return,
        };

        let mut interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let source_fk = FormKey {
            local: 0x00_1234,
            plugin: source_plugin,
        };
        let sig = SigCode::from_str("AMMO").unwrap();

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "StubInjectNewAlloc.esm".into(),
                preserve_source_ids: false,
                resolution_mode: ResolutionMode::DeferAndFixup,
                ..Default::default()
            },
            &mut interner,
        );

        let schema = fo4_schema();
        let result =
            inject_minimal_stub(handle, source_fk, "MyStubAmmo", sig, &mut mapper, &schema);
        let target_fk = result.expect("inject_minimal_stub should succeed");

        // new_allocation: assigned object-id should be FIRST_ALLOCATION_ID (0x800)
        // and NOT the source id (0x1234), because preserve_source_ids is false.
        assert_eq!(target_fk.local, 0x0000_0800);
        let plugin_name = mapper.interner.resolve(target_fk.plugin).unwrap();
        assert_eq!(plugin_name, "StubInjectNewAlloc.esm");

        // Mapping is registered.
        assert_eq!(mapper.lookup(source_fk), Some(target_fk));
    }

    /// Insert a stub via `source_id_preserved`: with `preserve_source_ids = true`
    /// and a source object-id at/above the allocator threshold, the same id is
    /// reused under the output plugin.
    #[test]
    fn inject_minimal_stub_source_id_preserved_reuses_local() {
        let handle = match new_target_handle("StubInjectPreserved.esm") {
            Some(h) => h,
            None => return,
        };

        let mut interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let source_fk = FormKey {
            local: 0x00_3456, // above FIRST_ALLOCATION_ID
            plugin: source_plugin,
        };
        let sig = SigCode::from_str("AMMO").unwrap();

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "StubInjectPreserved.esm".into(),
                preserve_source_ids: true,
                resolution_mode: ResolutionMode::DeferAndFixup,
                ..Default::default()
            },
            &mut interner,
        );

        let schema = fo4_schema();
        let target_fk = inject_minimal_stub(
            handle,
            source_fk,
            "PreservedStub",
            sig,
            &mut mapper,
            &schema,
        )
        .expect("inject_minimal_stub should succeed");

        assert_eq!(
            target_fk.local, 0x00_3456,
            "source_id_preserved strategy must reuse the source object-id"
        );
        let plugin_name = mapper.interner.resolve(target_fk.plugin).unwrap();
        assert_eq!(plugin_name, "StubInjectPreserved.esm");

        assert_eq!(mapper.lookup(source_fk), Some(target_fk));
    }

    /// Empty EDID is accepted (mirrors Python's behaviour when the source has no
    /// EDID): no EDID subrecord is emitted but the record is still inserted with
    /// the allocated FK and the mapping is registered.
    #[test]
    fn inject_minimal_stub_empty_edid_omits_edid_subrecord() {
        let handle = match new_target_handle("StubInjectEmptyEid.esm") {
            Some(h) => h,
            None => return,
        };

        let mut interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let source_fk = FormKey {
            local: 0x00_5678,
            plugin: source_plugin,
        };
        let sig = SigCode::from_str("AMMO").unwrap();

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "StubInjectEmptyEid.esm".into(),
                preserve_source_ids: false,
                ..Default::default()
            },
            &mut interner,
        );

        let schema = fo4_schema();
        let target_fk = inject_minimal_stub(handle, source_fk, "", sig, &mut mapper, &schema)
            .expect("inject_minimal_stub should succeed even with empty EDID");

        // Mapping must still be registered.
        assert_eq!(mapper.lookup(source_fk), Some(target_fk));
    }

    /// Calling inject_minimal_stub twice with the same source_fk must return the
    /// same target FK on the second call (mapper short-circuits in
    /// `allocate_or_resolve`).
    #[test]
    fn inject_minimal_stub_repeated_source_returns_same_target() {
        let handle = match new_target_handle("StubInjectRepeat.esm") {
            Some(h) => h,
            None => return,
        };

        let mut interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let source_fk = FormKey {
            local: 0x00_2222,
            plugin: source_plugin,
        };
        let sig = SigCode::from_str("AMMO").unwrap();

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "StubInjectRepeat.esm".into(),
                preserve_source_ids: false,
                ..Default::default()
            },
            &mut interner,
        );

        let schema = fo4_schema();
        let first = inject_minimal_stub(handle, source_fk, "A", sig, &mut mapper, &schema)
            .expect("first inject should succeed");
        let second = inject_minimal_stub(handle, source_fk, "A", sig, &mut mapper, &schema)
            .expect("second inject should succeed");

        assert_eq!(
            first, second,
            "repeated injection must yield the same target FK"
        );
    }

    /// Insertion against a non-existent handle returns StubInjectError::Write.
    #[test]
    fn inject_minimal_stub_unknown_handle_errors() {
        let mut interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let source_fk = FormKey {
            local: 0x00_4444,
            plugin: source_plugin,
        };
        let sig = SigCode::from_str("AMMO").unwrap();

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Nowhere.esm".into(),
                ..Default::default()
            },
            &mut interner,
        );

        let schema = fo4_schema();
        // Handle id 0 is never assigned to a real plugin slot.
        let result = inject_minimal_stub(0, source_fk, "X", sig, &mut mapper, &schema);
        assert!(matches!(result, Err(StubInjectError::Write(_))));
    }

    #[test]
    fn inject_minimal_stub_with_session_uses_held_lock_path() {
        let handle = match new_target_handle("StubInjectSession.esm") {
            Some(h) => h,
            None => return,
        };

        let mut interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let source_fk = FormKey {
            local: 0x00_7777,
            plugin: source_plugin,
        };
        let sig = SigCode::from_str("AMMO").unwrap();
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "StubInjectSession.esm".into(),
                preserve_source_ids: false,
                ..Default::default()
            },
            &mut interner,
        );
        let mut session = open_session(handle, None).expect("open session");

        let target_fk = inject_minimal_stub_with_session(
            &mut session,
            source_fk,
            "SessionStub",
            sig,
            &mut mapper,
        )
        .expect("session injection should succeed");

        assert_eq!(mapper.lookup(source_fk), Some(target_fk));
        assert_eq!(
            session.record(target_fk.local).unwrap().signature.as_str(),
            "AMMO"
        );
    }
}
