//! Schema-driven struct-codec facade for conversion fixups.
//!
//! # Why this module exists
//! Subrecords whose schema codec starts with `struct:` (e.g. `WEAP.DNAM` =
//! `struct:I,f,f,f,...,B,B,B,B`) decode through `source_read.rs` as
//! `FieldValue::Bytes` — a raw payload with no named-field access. The Python
//! conversion fixups operate on YAML-translated dicts and access nested
//! struct fields by display-name (`data["DamageBase"]`,
//! `entry["Relations"]["Faction"]`, …). Porting those fixups to Rust without a
//! way to decode struct payloads into name-keyed values is impossible.
//!
//! This module bridges the gap by reusing the already-built decode pipeline in
//! `authoring_serialize.rs` (`compact_subrecord_to_json` →
//! `serde_json::Value`). The facade:
//!
//! 1. Pulls the plugin's masters / name / game from the global handle store.
//! 2. Looks up the schema record + subrecord def via `compiled_schema_for_game`.
//! 3. Builds a `DecodeSpec` and runs the JSON decoder.
//!
//! The encode side is intentionally NOT included in this first cut — it
//! requires propagating `NativeImportContext` mutations (master growth,
//! localized-string allocation) back into the plugin slot, which is a
//! larger change.
//!
//! # Public surface
//! - [`decode_subrecord`] — turn `(handle_id, record_sig, subrec_sig, bytes)`
//!   into a name-keyed `serde_json::Value`.
//! - [`decoded_field_object`] — small accessor helper that returns the JSON
//!   object map when decode succeeds and produced an object.

use crate::fixups::FixupError;
use crate::ids::{SigCode, SubrecordSig};
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::{
    compact_subrecord_to_json, schema_subrecord_to_decode_spec,
};
use esp_authoring_core::plugin_runtime::{
    LocalizedStringsState, compiled_schema_for_game, plugin_handle_store_ref, schema_record_spec,
    schema_subrecord_spec,
};

// ---------------------------------------------------------------------------
// Public entry point — decode
// ---------------------------------------------------------------------------

/// Decode a subrecord payload into a name-keyed `serde_json::Value` using the
/// plugin's authoring schema.
///
/// `handle_id` selects which plugin's masters / name / strings the FormID
/// resolution and lstring lookups will use. `record_sig` is the parent
/// record signature (e.g. `WEAP`); `subrec_sig` is the subrecord signature
/// (e.g. `DNAM`). `data` is the raw subrecord payload bytes.
///
/// # Returns
/// On success the JSON value mirrors the YAML dict the legacy Python pipeline
/// works on — e.g. for `WEAP.DNAM` an object with keys `ammo`, `speed`,
/// `attack_delay`, …
///
/// # Errors
/// - `HandleError` if the plugin handle is unknown.
/// - `SchemaError` if the plugin has no recorded game, the schema has no
///   record/subrecord def for the given sigs, or the codec isn't a decodable
///   struct form.
pub fn decode_subrecord(
    handle_id: u64,
    record_sig: SigCode,
    subrec_sig: SubrecordSig,
    data: &[u8],
) -> Result<serde_json::Value, FixupError> {
    let (plugin_name, game_opt, masters, strings) = snapshot_plugin_state(handle_id)?;

    let game = game_opt.ok_or_else(|| {
        FixupError::SchemaError(format!(
            "plugin handle {handle_id} has no recorded game; cannot decode {}.{}",
            record_sig.as_str(),
            subrec_sig.as_str()
        ))
    })?;

    let compiled = compiled_schema_for_game(game.as_str())
        .map_err(|e| FixupError::SchemaError(format!("compiled_schema_for_game({game}): {e}")))?;

    let record_spec = schema_record_spec(&compiled, record_sig.as_str()).ok_or_else(|| {
        FixupError::SchemaError(format!(
            "no record_spec for {} in game {game}",
            record_sig.as_str()
        ))
    })?;

    let sub_spec = schema_subrecord_spec(record_spec, subrec_sig.as_str(), 0).ok_or_else(|| {
        FixupError::SchemaError(format!(
            "no subrecord_spec for {}.{} in game {game}",
            record_sig.as_str(),
            subrec_sig.as_str()
        ))
    })?;

    let decode_spec = schema_subrecord_to_decode_spec(sub_spec, &compiled).ok_or_else(|| {
        FixupError::SchemaError(format!(
            "schema_subrecord_to_decode_spec returned None for {}.{} (kind={}, codec={:?})",
            record_sig.as_str(),
            subrec_sig.as_str(),
            sub_spec.kind,
            sub_spec.codec
        ))
    })?;

    let value = compact_subrecord_to_json(
        data,
        Some(&decode_spec),
        Some((sub_spec, &compiled)),
        &strings,
        &masters,
        plugin_name.as_str(),
        None,
        None,
        None,
    );

    Ok(value)
}

/// Convenience: when [`decode_subrecord`] succeeds and the decoded value is a
/// JSON object, return its fields map. Returns `None` when the value is a
/// scalar / array / raw-only wrapper (`{"value": ..., "raw_hex": ...}`).
///
/// Fixups that just want to read named struct fields can lean on this helper
/// to avoid threading `serde_json::Value` matches at every call site.
pub fn decoded_field_object(
    value: serde_json::Value,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    match value {
        serde_json::Value::Object(map) => Some(map),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Snapshot the plugin slot's identity bits without holding the lock past the
/// expensive decode. Returns `(plugin_name, game, masters, strings)`.
///
/// Reads directly from `plugin_handle_store_ref()` rather than going through
/// `clone_plugin_handle_state`, because the latter returns `PyResult` whose
/// error formatting touches the Python GIL — fixups run on a Rust-only
/// thread.
fn snapshot_plugin_state(
    handle_id: u64,
) -> Result<(String, Option<String>, Vec<String>, LocalizedStringsState), FixupError> {
    let store = plugin_handle_store_ref()
        .lock()
        .map_err(|e| FixupError::HandleError(format!("plugin handle store poisoned: {e}")))?;
    let slot = store
        .get(&handle_id)
        .ok_or_else(|| FixupError::HandleError(format!("no plugin handle: {handle_id}")))?;
    Ok((
        slot.parsed.plugin_name.clone(),
        slot.parsed.game.clone(),
        slot.parsed.header.masters.clone(),
        slot.strings_ref().clone(),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};

    // Tests against real loaded plugins live in the fixup-level integration
    // tests. The unit tests here cover the handle-store boundary and the
    // small helper. The decode pipeline itself is covered by ~thousand
    // existing tests in `authoring_serialize.rs`.

    // ── unknown handle ────────────────────────────────────────────────────

    /// Decode against an unknown handle returns `HandleError`.
    #[test]
    fn decode_unknown_handle_returns_handle_error() {
        let record_sig = SigCode::from_str("WEAP").unwrap();
        let subrec_sig = SubrecordSig::from_str("DNAM").unwrap();
        let result = decode_subrecord(u64::MAX, record_sig, subrec_sig, &[0u8; 16]);
        match result {
            Err(FixupError::HandleError(msg)) => {
                assert!(
                    msg.contains(&format!("{}", u64::MAX)),
                    "error msg should mention the bad handle id: {msg}"
                );
            }
            other => panic!("expected HandleError, got {other:?}"),
        }
    }

    // ── decoded_field_object helper ───────────────────────────────────────

    /// `decoded_field_object` returns the map for an object value.
    #[test]
    fn decoded_field_object_returns_map_for_object() {
        let value = serde_json::json!({"foo": 1, "bar": "x"});
        let map = decoded_field_object(value).expect("object → map");
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("foo"), Some(&serde_json::Value::from(1)));
    }

    /// `decoded_field_object` returns None for non-object values.
    #[test]
    fn decoded_field_object_returns_none_for_non_object() {
        assert!(decoded_field_object(serde_json::Value::Null).is_none());
        assert!(decoded_field_object(serde_json::json!(42)).is_none());
        assert!(decoded_field_object(serde_json::json!("hi")).is_none());
        assert!(decoded_field_object(serde_json::json!([1, 2, 3])).is_none());
    }

    // ── end-to-end decode against the FO4 schema ──────────────────────────

    /// Defer plugin handle close so cleanup is robust on assertion failure.
    struct CloseOnDrop(u64);
    impl Drop for CloseOnDrop {
        fn drop(&mut self) {
            esp_authoring_core::plugin_runtime::plugin_handle_close_native(self.0);
        }
    }

    /// Decode WEAP.DNAM bytes via the FO4 schema and verify a few
    /// named-field values come through. WEAP.DNAM codec starts with
    /// `struct:I,f,f,f,...` — first two fields are `ammo` (formid u32) and
    /// `speed` (f32).
    #[test]
    fn decode_weap_dnam_e2e_returns_named_fields() {
        // Create a fresh FO4 plugin handle (no Python required for new/close).
        let handle_id =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Output.esp", Some("fo4"))
                .expect("plugin_handle_new_native");
        let _guard = CloseOnDrop(handle_id);

        // WEAP.DNAM codec `struct:I,f,f,f,f,f,f,f,f,I,I,I,I,H,B,f,f,I,H,I,I,I,I,I,I,I,I,I,B,f,B,B,f,f,f,I,B,B,B,B`
        // 16×I + 14×f + 2×H + 8×B = 64 + 56 + 4 + 8 = 132 bytes exactly.
        // The decoder rejects mismatched lengths (falls back to raw_hex), so
        // size matters.
        let mut data = vec![0u8; 132];
        let ammo_bytes = 0x00_001234u32.to_le_bytes();
        data[0..4].copy_from_slice(&ammo_bytes);
        let speed_bytes = 1.5f32.to_le_bytes();
        data[4..8].copy_from_slice(&speed_bytes);

        let record_sig = SigCode::from_str("WEAP").unwrap();
        let subrec_sig = SubrecordSig::from_str("DNAM").unwrap();
        let decoded = decode_subrecord(handle_id, record_sig, subrec_sig, &data)
            .expect("decode_subrecord should succeed for valid WEAP.DNAM payload");

        // Decoded must be an object (typed-pass-through, preservation=typed)
        // — or wrapped {"value":..., "raw_hex":...} (raw fallback).
        // We accept either: the facade test verifies decode runs end-to-end;
        // a fixup-level test verifies specific field semantics.
        match decoded {
            serde_json::Value::Object(map) => {
                // Keys come from authoring_key_name (CamelCase display
                // labels). Verify `Speed` round-trips.
                let speed = map
                    .get("Speed")
                    .and_then(|v| v.as_f64())
                    .unwrap_or_else(|| panic!("Speed missing/not-float: {map:?}"));
                assert!((speed - 1.5_f64).abs() < 1e-6, "Speed ≈ 1.5, got {speed}");
            }
            other => panic!("expected JSON object for typed WEAP.DNAM, got: {other}"),
        }
    }

    /// Unknown record sig returns `SchemaError`.
    #[test]
    fn decode_unknown_record_sig_returns_schema_error() {
        let handle_id =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Output.esp", Some("fo4"))
                .expect("create handle");
        let _guard = CloseOnDrop(handle_id);

        let bogus_record = SigCode::from_str("XXXX").unwrap();
        let dnam = SubrecordSig::from_str("DNAM").unwrap();
        let result = decode_subrecord(handle_id, bogus_record, dnam, &[0u8; 16]);
        match result {
            Err(FixupError::SchemaError(msg)) => {
                assert!(msg.contains("XXXX"), "msg should mention sig: {msg}");
            }
            other => panic!("expected SchemaError, got {other:?}"),
        }
    }

    /// Unknown subrecord sig under a real record returns `SchemaError`.
    #[test]
    fn decode_unknown_subrec_sig_returns_schema_error() {
        let handle_id =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Output.esp", Some("fo4"))
                .expect("create handle");
        let _guard = CloseOnDrop(handle_id);

        let weap = SigCode::from_str("WEAP").unwrap();
        let bogus_sub = SubrecordSig::from_str("ZZZZ").unwrap();
        let result = decode_subrecord(handle_id, weap, bogus_sub, &[0u8; 8]);
        match result {
            Err(FixupError::SchemaError(msg)) => {
                assert!(msg.contains("ZZZZ"), "msg should mention sig: {msg}");
            }
            other => panic!("expected SchemaError, got {other:?}"),
        }
    }

    /// Decode FACT.XNAM (Relations) — `struct:I,i,I`, 12 bytes.
    /// Verifies a fixed-size 3-field struct with mixed signed/unsigned ints
    /// alongside a FormID. FACT.XNAM is `repeatable`; this decodes a single
    /// per-entry XNAM payload.
    #[test]
    fn decode_fact_xnam_relations_e2e() {
        let handle_id =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Output.esp", Some("fo4"))
                .expect("create handle");
        let _guard = CloseOnDrop(handle_id);

        let mut data = vec![0u8; 12];
        data[0..4].copy_from_slice(&0x00_001234u32.to_le_bytes()); // faction (formid)
        data[4..8].copy_from_slice(&(-7i32).to_le_bytes()); // modifier (i32)
        data[8..12].copy_from_slice(&2u32.to_le_bytes()); // group_combat_reaction (u32)

        let record_sig = SigCode::from_str("FACT").unwrap();
        let subrec_sig = SubrecordSig::from_str("XNAM").unwrap();
        let decoded = decode_subrecord(handle_id, record_sig, subrec_sig, &data)
            .expect("decode FACT.XNAM should succeed");

        let map = decoded_field_object(decoded.clone())
            .unwrap_or_else(|| panic!("expected typed object for FACT.XNAM, got: {decoded}"));

        // Keys come from authoring_key_name(display_label, id) — CamelCase of
        // the display_label. This matches the YAML shape Python fixups
        // operated on (e.g. `entry["Relations"]["Faction"]`).
        assert!(
            map.contains_key("Faction"),
            "decoded FACT.XNAM must expose `Faction` key; got: {:?}",
            map.keys().collect::<Vec<_>>()
        );
        let modifier = map
            .get("Modifier")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| panic!("Modifier missing/not-int: {map:?}"));
        assert_eq!(modifier, -7, "Modifier should round-trip as -7");
        let group_reaction = map
            .get("GroupCombatReaction")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| panic!("GroupCombatReaction missing/not-uint: {map:?}"));
        assert_eq!(
            group_reaction, 2,
            "GroupCombatReaction should round-trip as 2"
        );
    }

    /// Decode RACE.ATKD (Attack Data) — `struct:f,f,I,I,f,f,f,f,f,f,i`,
    /// 44 bytes. Verifies an 11-field struct mixing float / formid /
    /// enum_ref'd uint / signed int.
    #[test]
    fn decode_race_atkd_attack_data_e2e() {
        let handle_id =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native("Output.esp", Some("fo4"))
                .expect("create handle");
        let _guard = CloseOnDrop(handle_id);

        let mut data = vec![0u8; 44];
        data[0..4].copy_from_slice(&2.5_f32.to_le_bytes()); // damage_mult
        data[4..8].copy_from_slice(&0.75_f32.to_le_bytes()); // attack_chance
        data[8..12].copy_from_slice(&0u32.to_le_bytes()); // attack_spell (NULL FK)
        data[12..16].copy_from_slice(&0x40u32.to_le_bytes()); // attack_flags (FO76 0x40)
        data[16..20].copy_from_slice(&90.0_f32.to_le_bytes()); // attack_angle
        data[20..24].copy_from_slice(&60.0_f32.to_le_bytes()); // strike_angle
        data[24..28].copy_from_slice(&0.5_f32.to_le_bytes()); // stagger
        data[28..32].copy_from_slice(&0.1_f32.to_le_bytes()); // knockdown
        data[32..36].copy_from_slice(&1.0_f32.to_le_bytes()); // recovery_time
        data[36..40].copy_from_slice(&1.0_f32.to_le_bytes()); // action_points_mult
        data[40..44].copy_from_slice(&(-3i32).to_le_bytes()); // stagger_offset

        let record_sig = SigCode::from_str("RACE").unwrap();
        let subrec_sig = SubrecordSig::from_str("ATKD").unwrap();
        let decoded = decode_subrecord(handle_id, record_sig, subrec_sig, &data)
            .expect("decode RACE.ATKD should succeed");

        let map = decoded_field_object(decoded.clone())
            .unwrap_or_else(|| panic!("expected typed object for RACE.ATKD, got: {decoded}"));

        // Keys are CamelCase of display labels (same convention as YAML).
        let damage_mult = map
            .get("DamageMult")
            .and_then(|v| v.as_f64())
            .unwrap_or_else(|| panic!("DamageMult missing/not-float: {map:?}"));
        assert!(
            (damage_mult - 2.5_f64).abs() < 1e-6,
            "DamageMult ≈ 2.5, got {damage_mult}"
        );
        let stagger_offset = map
            .get("StaggerOffset")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| panic!("StaggerOffset missing/not-int: {map:?}"));
        assert_eq!(stagger_offset, -3, "StaggerOffset should round-trip as -3");
        // AttackFlags is enum_ref'd → decoded as an array of token-strings.
        // For our 0x40 bit, the schema generator labels the token "Unknown6"
        // (FO76-only bit with no FO4 name). Just confirm the field exists
        // and is an array.
        let flags = map
            .get("AttackFlags")
            .unwrap_or_else(|| panic!("AttackFlags missing: {map:?}"));
        assert!(
            flags.is_array(),
            "AttackFlags should decode as a token-array (enum_ref flags), got: {flags}"
        );
        // AttackSpell is a NULL FormID (raw 0) — the schema-aware decoder
        // omits null FormID fields entirely. Document that here so fixup
        // authors know a missing key means "absent", not "decode failed".
        assert!(
            !map.contains_key("AttackSpell"),
            "NULL FormID slots are omitted from typed decode (got: {map:?})"
        );
    }
}
