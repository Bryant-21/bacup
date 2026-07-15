//! Fixup: remove LeveledNpc entries that reference template-actor NPCs.
//!

//! # What this does
//! CK rejects "leveled template actors" in LeveledNpc lists — if an entry's
//! reference points to an NPC that inherits data via `TemplateActors`, CK errors
//! on load.
//!
//! Steps:
//! 1. Collect FormKeys of every target NPC_ record whose `TemplateActors` slot
//!    set has any populated template actor slot (FO4 sig `TPTA`).
//! 2. For each LVLN record in target plugin, drop every `LVLO` entry whose
//!    `Reference` FormKey is in that set.
//! 3. When at least one entry was removed, write the record back.
//!
//! # Guards
//! - Non-creature single-root runs → no-op.
//! - Whole-plugin runs → scan all NPC_/LVLN records.
//! - No NPCs have `TemplateActors` populated → no-op.
//!
//! # Subrecord layouts (FO4 schema)
//! `TPTA` codec `struct:I,I,I,I,I,I,I,I,I,I,I,I,I` — 13 FormID slots, each
//! 4 bytes. Slot order: traits, stats, factions, spell_list, ai_data,
//! ai_packages, model_animation, **base_data** (slot 7, offset 28), inventory,
//! script, def_package_list, attack_data, keywords.
//!
//! `LVLO` codec `struct:H,B,B,I,H,B,B` (`parsed_with_raw_fallback`) — 12 bytes
//! total: level (u16, off 0), unknown_u8_1 (off 2), unknown_u8_2 (off 3),
//! **reference (formid, off 4)**, count (u16, off 8), unknown_u8_3 (off 10),
//! unknown_u8_4 (off 11).
//!
//! Decoded values are usually `FieldValue::Bytes` on the `read_record` path.
//! Python-pushed records may arrive as `FieldValue::Struct`; both shapes are
//! handled.

use crate::fixups::prune_orphaned_records::is_creature_root_sig;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// TPTA / LVLO byte-level constants
// ---------------------------------------------------------------------------

/// Width of one FormID slot inside `TPTA`.
const TPTA_SLOT_WIDTH: usize = 4;

/// Byte offset of the `reference` FormID inside `LVLO` payload.
/// After level(H, 0..2) + 2 unknown bytes (2..3, 3..4).
const LVLO_REFERENCE_OFFSET: usize = 4;

/// Minimum LVLO payload length to read `reference`.
const LVLO_MIN_LEN: usize = LVLO_REFERENCE_OFFSET + 4;

// ---------------------------------------------------------------------------
// Public fixup struct
// ---------------------------------------------------------------------------

pub struct FilterLcharTemplateNpcsFixup;

impl Fixup for FilterLcharTemplateNpcsFixup {
    fn name(&self) -> &'static str {
        "filter_lchar_template_npcs"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::GraphOnly
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        match ctx.config.root_sig {
            Some(sig) => is_creature_root_sig(sig),
            None => true,
        }
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config.root_sig.map(is_creature_root_sig).unwrap_or(true)
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let target_masters = session.target_masters().to_vec();
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();

        // ── Step 1 — collect template-NPC FormKeys (TPTA with any slot set).
        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let mut template_npc_fks: HashSet<FormKey> = HashSet::new();

        let npc_fks = session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in &npc_fks {
            let record = match session.record_decoded(fk, target_schema.as_ref(), mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("filter_lchar_npc_read:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };
            if tpta_has_template_actor(&record) {
                template_npc_fks.insert(record.form_key);
            }
        }

        if template_npc_fks.is_empty() && config.target_master_handle_ids.is_empty() {
            return Ok(report);
        }
        let mut master_template_cache: HashMap<FormKey, bool> = HashMap::new();

        // ── Step 2 — walk LVLN records and drop entries pointing into the set
        // or into target-master NPCs that CK treats as template actors.
        let lvln_sig =
            SigCode::from_str("LVLN").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let reference_syms = [
            mapper.interner.intern("npc"),
            mapper.interner.intern("NPC"),
            mapper.interner.intern("Reference"),
            mapper.interner.intern("reference"),
        ];

        let lvln_fks = session
            .form_keys_of_sig(lvln_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in &lvln_fks {
            let mut record =
                match session.record_decoded(fk, target_schema.as_ref(), mapper.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("filter_lchar_lvln_read:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };
            let mut is_master_template_npc = |candidate_fk: &FormKey| {
                if let Some(is_template) = master_template_cache.get(candidate_fk) {
                    return *is_template;
                }
                let is_template = master_ref_is_template_npc(
                    session,
                    candidate_fk,
                    target_schema.as_ref(),
                    &target_masters,
                    config.target_master_handle_ids.as_slice(),
                    mapper.interner,
                    &mut report.warnings,
                );
                master_template_cache.insert(*candidate_fk, is_template);
                is_template
            };
            let removed = drop_template_lvlo_entries(
                &mut record,
                &template_npc_fks,
                &mut is_master_template_npc,
                &reference_syms,
                &target_masters,
                &target_plugin_name,
                mapper.interner,
            );
            if removed > 0 {
                session
                    .replace_record(record, target_schema.as_ref(), mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                report.records_changed += 1;
                report.records_dropped += removed;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Branch helpers
// ---------------------------------------------------------------------------

/// Returns `true` when the record carries a `TPTA` subrecord with at least one
/// populated template actor slot.
///
/// Handles both `FieldValue::Bytes` (the `read_record` decode path) and
/// `FieldValue::Struct` (Python-pushed records).
pub fn tpta_has_template_actor(record: &Record) -> bool {
    let tpta_sig = match SubrecordSig::from_str("TPTA") {
        Ok(s) => s,
        Err(_) => return false,
    };
    for entry in &record.fields {
        if entry.sig != tpta_sig {
            continue;
        }
        match &entry.value {
            FieldValue::Bytes(data) => {
                for chunk in data.chunks_exact(TPTA_SLOT_WIDTH) {
                    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    if raw != 0 {
                        return true;
                    }
                }
            }
            FieldValue::Struct(fields) => {
                for (_, val) in fields {
                    if field_value_is_non_null(val) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn field_value_is_non_null(value: &FieldValue) -> bool {
    match value {
        FieldValue::FormKey(fk) => fk.local != 0,
        FieldValue::Uint(n) => *n != 0,
        FieldValue::Int(n) => *n != 0,
        _ => false,
    }
}

fn master_ref_is_template_npc(
    session: &mut PluginSession,
    fk: &FormKey,
    target_schema: &AuthoringSchema,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    interner: &StringInterner,
    warnings: &mut Vec<Sym>,
) -> bool {
    let Some(handle_id) =
        target_master_handle_for_fk(fk, target_masters, target_master_handle_ids, interner)
    else {
        return false;
    };
    match session.record_decoded_in_handle(handle_id, fk, target_schema, interner) {
        Ok(record) => record.sig.as_str() == "NPC_" && tpta_has_template_actor(&record),
        Err(e) => {
            warnings.push(interner.intern(&format!("filter_lchar_master_npc_read:{e}")));
            false
        }
    }
}

fn target_master_handle_for_fk(
    fk: &FormKey,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    interner: &StringInterner,
) -> Option<u64> {
    let plugin_name = interner.resolve(fk.plugin)?;
    let load_index = target_masters
        .iter()
        .position(|name| name.eq_ignore_ascii_case(plugin_name))?;
    target_master_handle_ids.get(load_index).copied()
}

/// Drop every `LVLO` entry in `record` whose `Reference` FormKey is contained
/// in `template_npc_fks`. Returns the number of entries removed.
///
/// Supports both `FieldValue::Bytes` (raw 12-byte payload) and
/// `FieldValue::Struct` (Python-pushed records).  For the `Bytes` path the
/// raw FormID at offset 4..8 is resolved against `target_masters` /
/// `target_plugin_name` to produce a `FormKey` comparable with the set.
fn drop_template_lvlo_entries(
    record: &mut Record,
    template_npc_fks: &HashSet<FormKey>,
    is_template_npc_ref: &mut dyn FnMut(&FormKey) -> bool,
    reference_syms: &[crate::sym::Sym],
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> u32 {
    let lvlo_sig = match SubrecordSig::from_str("LVLO") {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let mut removed: u32 = 0;
    let mut kept: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    for entry in record.fields.drain(..) {
        if entry.sig != lvlo_sig {
            kept.push(entry);
            continue;
        }
        let ref_fk = extract_lvlo_reference(
            &entry.value,
            reference_syms,
            target_masters,
            target_plugin_name,
            interner,
        );
        let drop_it = match ref_fk {
            Some(fk) => template_npc_fks.contains(&fk) || is_template_npc_ref(&fk),
            None => false,
        };
        if drop_it {
            removed += 1;
        } else {
            kept.push(entry);
        }
    }
    if removed > 0 {
        sync_llct_count(&mut kept);
    }
    record.fields = kept;
    removed
}

fn sync_llct_count(fields: &mut smallvec::SmallVec<[FieldEntry; 8]>) {
    let count = fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "LVLO")
        .count()
        .min(u8::MAX as usize) as u64;
    let Ok(llct_sig) = SubrecordSig::from_str("LLCT") else {
        return;
    };
    if let Some(entry) = fields.iter_mut().find(|entry| entry.sig == llct_sig) {
        entry.value = FieldValue::Uint(count);
    }
}

/// Extract the `Reference` FormKey from a single LVLO `FieldValue`.
///
/// Returns `None` when the value isn't a recognised LVLO shape or the
/// reference is null / unreadable.
fn extract_lvlo_reference(
    value: &FieldValue,
    reference_syms: &[crate::sym::Sym],
    target_masters: &[String],
    target_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::Bytes(data) if data.len() >= LVLO_MIN_LEN => {
            let raw = u32::from_le_bytes([
                data[LVLO_REFERENCE_OFFSET],
                data[LVLO_REFERENCE_OFFSET + 1],
                data[LVLO_REFERENCE_OFFSET + 2],
                data[LVLO_REFERENCE_OFFSET + 3],
            ]);
            resolve_raw_form_id(raw, target_masters, target_plugin_name, interner)
        }
        FieldValue::Struct(fields) => {
            for (key, val) in fields {
                if !reference_syms.contains(key) {
                    continue;
                }
                if let FieldValue::FormKey(fk) = val {
                    if fk.local != 0 {
                        return Some(*fk);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Resolve a raw 32-bit FormID into a `FormKey` using the plugin's master
/// table. Returns `None` for null (raw == 0).
fn resolve_raw_form_id(
    raw: u32,
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let master_index = ((raw >> 24) & 0xFF) as usize;
    let object_id = raw & 0x00FF_FFFF;
    let own_index = masters.len();
    let plugin = if master_index < own_index {
        masters[master_index].as_str()
    } else {
        // Own plugin (or out-of-range master index — also treat as own).
        plugin_name
    };
    let plugin_sym = interner.intern(plugin);
    Some(FormKey {
        local: object_id,
        plugin: plugin_sym,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::{FixupConfig, FixupContext};
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::SigCode;
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::schema::AuthoringSchema;
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
    use std::sync::Arc;

    // ── helpers ────────────────────────────────────────────────────────────

    fn make_test_ctx<'a>(
        schema: &'a Arc<AuthoringSchema>,
        config: &'a FixupConfig,
        _ctx_interner: &'a mut StringInterner,
    ) -> FixupContext<'a> {
        FixupContext {
            source_handle_id: 1,
            target_handle_id: 2,
            schema_target: schema,
            schema_source: schema,
            skip_record_sigs: crate::fixups::empty_skip_record_sigs(),
            mod_path: None,
            source_extracted_dir: None,
            target_master_handle_ids: &[],
            config,
        }
    }

    fn make_record(sig_str: &str, local: u32, plugin: &str, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig_str).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    /// Build a TPTA payload with the given slot values (13 slots = 52 bytes).
    fn make_tpta_bytes(slots: [u32; 13]) -> smallvec::SmallVec<[u8; 32]> {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        for slot in slots {
            data.extend_from_slice(&slot.to_le_bytes());
        }
        data
    }

    /// Build an LVLO bytes payload with the given reference FormID.
    fn make_lvlo_bytes(reference_raw: u32) -> smallvec::SmallVec<[u8; 32]> {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        // level (u16) + unknown_u8 + unknown_u8 + reference (u32) + count (u16) + 2 unknowns
        data.extend_from_slice(&1u16.to_le_bytes()); // level
        data.push(0); // unknown_u8_1
        data.push(0); // unknown_u8_2
        data.extend_from_slice(&reference_raw.to_le_bytes()); // reference
        data.extend_from_slice(&1u16.to_le_bytes()); // count
        data.push(0); // unknown_u8_3
        data.push(0); // unknown_u8_4
        data
    }

    fn push_field(record: &mut Record, sig_str: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig_str).unwrap(),
            value,
        });
    }

    fn count_lvlo(record: &Record) -> usize {
        let sig = SubrecordSig::from_str("LVLO").unwrap();
        record.fields.iter().filter(|e| e.sig == sig).count()
    }

    fn first_uint(record: &Record, sig_str: &str) -> Option<u64> {
        let sig = SubrecordSig::from_str(sig_str).unwrap();
        record
            .fields
            .iter()
            .find(|entry| entry.sig == sig)
            .and_then(|entry| match entry.value {
                FieldValue::Uint(value) => Some(value),
                _ => None,
            })
    }

    fn drop_template_lvlo_entries_for_test(
        record: &mut Record,
        template_npc_fks: &HashSet<FormKey>,
        reference_syms: &[crate::sym::Sym],
        target_masters: &[String],
        target_plugin_name: &str,
        interner: &StringInterner,
    ) -> u32 {
        let mut no_master_templates = |_fk: &FormKey| false;
        drop_template_lvlo_entries(
            record,
            template_npc_fks,
            &mut no_master_templates,
            reference_syms,
            target_masters,
            target_plugin_name,
            interner,
        )
    }

    // ── applies_to dispatch ────────────────────────────────────────────────

    /// applies to NPC_ roots.
    #[test]
    fn applies_to_npc_root() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            ..Default::default()
        };
        let mut ctx_interner = StringInterner::new();
        let ctx = make_test_ctx(&schema, &config, &mut ctx_interner);
        assert!(FilterLcharTemplateNpcsFixup.applies_to(&ctx));
    }

    /// applies to LVLN roots.
    #[test]
    fn applies_to_lvln_root() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("LVLN").unwrap()),
            ..Default::default()
        };
        let mut ctx_interner = StringInterner::new();
        let ctx = make_test_ctx(&schema, &config, &mut ctx_interner);
        assert!(FilterLcharTemplateNpcsFixup.applies_to(&ctx));
    }

    /// does not apply to WEAP roots.
    #[test]
    fn does_not_apply_to_weap_root() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("WEAP").unwrap()),
            ..Default::default()
        };
        let mut ctx_interner = StringInterner::new();
        let ctx = make_test_ctx(&schema, &config, &mut ctx_interner);
        assert!(!FilterLcharTemplateNpcsFixup.applies_to(&ctx));
    }

    /// applies when root_sig is None for whole-plugin runs.
    #[test]
    fn applies_when_no_root_sig() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let config = FixupConfig {
            root_sig: None,
            ..Default::default()
        };
        let mut ctx_interner = StringInterner::new();
        let ctx = make_test_ctx(&schema, &config, &mut ctx_interner);
        assert!(FilterLcharTemplateNpcsFixup.applies_to(&ctx));
        let target_handle =
            plugin_handle_new_native("FilterLcharTemplateNpcsFixupTest.esp", Some("fo4"))
                .expect("test plugin handle");
        let session = open_session(target_handle, None).expect("open session");
        assert!(FilterLcharTemplateNpcsFixup.applies_to_session(&session, &config));
    }

    /// smoke: session run is no-op on an empty target plugin.
    #[test]
    fn run_with_session_is_noop_on_empty_plugin() {
        let config = FixupConfig {
            root_sig: Some(SigCode::from_str("NPC_").unwrap()),
            ..Default::default()
        };
        let target_handle =
            plugin_handle_new_native("FilterLcharTemplateNpcsFixupTest.esp", Some("fo4"))
                .expect("test plugin handle");
        let mut mapper_interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut mapper_interner);
        let mut session = open_session(target_handle, None).expect("open session");
        let report = FilterLcharTemplateNpcsFixup
            .run_with_session(&mut session, &mut mapper, &config)
            .expect("empty target plugin should produce a no-op report");
        assert!(report.is_no_op(), "empty target plugin should be a no-op");
    }

    // ── tpta_has_template_actor — Bytes shape ─────────────────────────────

    /// TPTA Bytes with base_data slot set returns true.
    #[test]
    fn tpta_bytes_with_base_data_returns_true() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", 0x000100, "Output.esp", &mut interner);
        // Slot 7 = base_data = 0x00ABCDEF, all other slots zero.
        let mut slots = [0u32; 13];
        slots[7] = 0x00_ABCDEF;
        push_field(
            &mut record,
            "TPTA",
            FieldValue::Bytes(make_tpta_bytes(slots)),
        );
        assert!(tpta_has_template_actor(&record));
    }

    #[test]
    fn tpta_bytes_with_traits_slot_returns_true() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", 0x000100, "Output.esp", &mut interner);
        let mut slots = [0u32; 13];
        slots[0] = 0x00_ABCDEF;
        push_field(
            &mut record,
            "TPTA",
            FieldValue::Bytes(make_tpta_bytes(slots)),
        );
        assert!(tpta_has_template_actor(&record));
    }

    /// TPTA Bytes with no populated slots returns false.
    #[test]
    fn tpta_bytes_without_template_actor_returns_false() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", 0x000100, "Output.esp", &mut interner);
        // All slots zero — no template inheritance.
        push_field(
            &mut record,
            "TPTA",
            FieldValue::Bytes(make_tpta_bytes([0u32; 13])),
        );
        assert!(!tpta_has_template_actor(&record));
    }

    /// Record without TPTA returns false.
    #[test]
    fn no_tpta_returns_false() {
        let mut interner = StringInterner::new();
        let record = make_record("NPC_", 0x000100, "Output.esp", &mut interner);
        assert!(!tpta_has_template_actor(&record));
    }

    /// TPTA Bytes shorter than one slot returns false.
    #[test]
    fn tpta_bytes_too_short_returns_false() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", 0x000100, "Output.esp", &mut interner);
        // Only 3 bytes — no complete FormID slot is readable.
        let mut short: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        short.extend_from_slice(&[0xFFu8; 3]);
        push_field(&mut record, "TPTA", FieldValue::Bytes(short));
        assert!(!tpta_has_template_actor(&record));
    }

    // ── tpta_has_template_actor — Struct shape ────────────────────────────

    /// TPTA Struct with FormKey base_data returns true.
    #[test]
    fn tpta_struct_with_base_data_formkey_returns_true() {
        let mut interner = StringInterner::new();
        let base_data_sym = interner.intern("base_data");
        let other_sym = interner.intern("traits");
        let mut record = make_record("NPC_", 0x000100, "Output.esp", &mut interner);
        let target_fk = FormKey {
            local: 0x00_ABCDEF,
            plugin: interner.intern("Fallout4.esm"),
        };
        push_field(
            &mut record,
            "TPTA",
            FieldValue::Struct(vec![
                (
                    other_sym,
                    FieldValue::FormKey(FormKey {
                        local: 0,
                        plugin: target_fk.plugin,
                    }),
                ),
                (base_data_sym, FieldValue::FormKey(target_fk)),
            ]),
        );
        assert!(tpta_has_template_actor(&record));
    }

    #[test]
    fn tpta_struct_with_authoring_traits_key_returns_true() {
        let mut interner = StringInterner::new();
        let traits_sym = interner.intern("Traits");
        let mut record = make_record("NPC_", 0x000100, "Output.esp", &mut interner);
        let target_fk = FormKey {
            local: 0x00_ABCDEF,
            plugin: interner.intern("Fallout4.esm"),
        };
        push_field(
            &mut record,
            "TPTA",
            FieldValue::Struct(vec![(traits_sym, FieldValue::FormKey(target_fk))]),
        );
        assert!(tpta_has_template_actor(&record));
    }

    /// TPTA Struct with only null FormKeys returns false.
    #[test]
    fn tpta_struct_with_only_null_formkeys_returns_false() {
        let mut interner = StringInterner::new();
        let base_data_sym = interner.intern("base_data");
        let traits_sym = interner.intern("Traits");
        let mut record = make_record("NPC_", 0x000100, "Output.esp", &mut interner);
        let null_fk = FormKey {
            local: 0,
            plugin: interner.intern("Fallout4.esm"),
        };
        push_field(
            &mut record,
            "TPTA",
            FieldValue::Struct(vec![
                (traits_sym, FieldValue::FormKey(null_fk)),
                (base_data_sym, FieldValue::FormKey(null_fk)),
            ]),
        );
        assert!(!tpta_has_template_actor(&record));
    }

    // ── drop_template_lvlo_entries ────────────────────────────────────────

    /// drop LVLO Bytes entries that resolve into the template set.
    #[test]
    fn drop_lvlo_bytes_pointing_to_template_npc() {
        let mut interner = StringInterner::new();
        let reference_sym = interner.intern("Reference");
        // Target masters: ["Fallout4.esm"]. own_index = 1.
        let masters = vec!["Fallout4.esm".to_string()];
        let plugin_name = "Output.esp".to_string();

        // Template NPC FK: object 0x000800 in own plugin.
        let template_fk = FormKey {
            local: 0x000800,
            plugin: interner.intern(&plugin_name),
        };
        let mut template_set = HashSet::new();
        template_set.insert(template_fk);

        let mut record = make_record("LVLN", 0x000100, &plugin_name, &mut interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(3));
        // Entry 1 — points to template NPC (own plugin, master byte 0x01, object 0x000800).
        // Raw u32 = (1 << 24) | 0x000800 = 0x01000800.
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(make_lvlo_bytes(0x01_000800)),
        );
        // Entry 2 — points to some other NPC (Fallout4.esm, master byte 0, object 0x001234).
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(make_lvlo_bytes(0x00_001234)),
        );
        // Entry 3 — another template-NPC ref (duplicate, should also be dropped).
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(make_lvlo_bytes(0x01_000800)),
        );

        let removed = drop_template_lvlo_entries_for_test(
            &mut record,
            &template_set,
            &[reference_sym],
            &masters,
            &plugin_name,
            &mut interner,
        );
        assert_eq!(removed, 2);
        assert_eq!(count_lvlo(&record), 1, "one non-template LVLO survives");
        assert_eq!(first_uint(&record, "LLCT"), Some(1));
    }

    #[test]
    fn drop_lvlo_bytes_pointing_to_master_template_npc() {
        let mut interner = StringInterner::new();
        let reference_sym = interner.intern("Reference");
        let masters = vec!["Fallout4.esm".to_string()];
        let plugin_name = "Output.esp".to_string();
        let template_set = HashSet::new();
        let fo4_sym = interner.intern("Fallout4.esm");
        let mut master_templates = |fk: &FormKey| fk.plugin == fo4_sym && fk.local == 0x0D228A;

        let mut record = make_record("LVLN", 0x000100, &plugin_name, &mut interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(2));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(make_lvlo_bytes(0x00_0D228A)),
        );
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(make_lvlo_bytes(0x00_001234)),
        );

        let removed = drop_template_lvlo_entries(
            &mut record,
            &template_set,
            &mut master_templates,
            &[reference_sym],
            &masters,
            &plugin_name,
            &interner,
        );

        assert_eq!(removed, 1);
        assert_eq!(count_lvlo(&record), 1);
        assert_eq!(first_uint(&record, "LLCT"), Some(1));
    }

    /// Empty template set is a no-op.
    #[test]
    fn empty_template_set_is_noop() {
        let mut interner = StringInterner::new();
        let reference_sym = interner.intern("Reference");
        let masters = vec!["Fallout4.esm".to_string()];
        let plugin_name = "Output.esp".to_string();
        let template_set: HashSet<FormKey> = HashSet::new();

        let mut record = make_record("LVLN", 0x000100, &plugin_name, &mut interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(2));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(make_lvlo_bytes(0x01_000800)),
        );
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(make_lvlo_bytes(0x00_001234)),
        );

        let removed = drop_template_lvlo_entries_for_test(
            &mut record,
            &template_set,
            &[reference_sym],
            &masters,
            &plugin_name,
            &mut interner,
        );
        assert_eq!(removed, 0);
        assert_eq!(count_lvlo(&record), 2);
    }

    /// Struct-shaped LVLO entries are matched against the template set.
    #[test]
    fn drop_lvlo_struct_pointing_to_template_npc() {
        let mut interner = StringInterner::new();
        let reference_sym = interner.intern("Reference");
        let masters = vec!["Fallout4.esm".to_string()];
        let plugin_name = "Output.esp".to_string();

        let template_fk = FormKey {
            local: 0x000800,
            plugin: interner.intern(&plugin_name),
        };
        let mut template_set = HashSet::new();
        template_set.insert(template_fk);

        let other_fk = FormKey {
            local: 0x001234,
            plugin: interner.intern("Fallout4.esm"),
        };

        let mut record = make_record("LVLN", 0x000100, &plugin_name, &mut interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(2));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Struct(vec![(reference_sym, FieldValue::FormKey(template_fk))]),
        );
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Struct(vec![(reference_sym, FieldValue::FormKey(other_fk))]),
        );

        let removed = drop_template_lvlo_entries_for_test(
            &mut record,
            &template_set,
            &[reference_sym],
            &masters,
            &plugin_name,
            &mut interner,
        );
        assert_eq!(removed, 1);
        assert_eq!(count_lvlo(&record), 1);
        assert_eq!(first_uint(&record, "LLCT"), Some(1));
    }

    #[test]
    fn drop_lvlo_struct_with_authoring_npc_key() {
        let mut interner = StringInterner::new();
        let reference_sym = interner.intern("NPC");
        let masters = vec!["Fallout4.esm".to_string()];
        let plugin_name = "Output.esp".to_string();

        let template_fk = FormKey {
            local: 0x000800,
            plugin: interner.intern(&plugin_name),
        };
        let mut template_set = HashSet::new();
        template_set.insert(template_fk);

        let other_fk = FormKey {
            local: 0x001234,
            plugin: interner.intern("Fallout4.esm"),
        };

        let mut record = make_record("LVLN", 0x000100, &plugin_name, &mut interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(2));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Struct(vec![(reference_sym, FieldValue::FormKey(template_fk))]),
        );
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Struct(vec![(reference_sym, FieldValue::FormKey(other_fk))]),
        );

        let removed = drop_template_lvlo_entries_for_test(
            &mut record,
            &template_set,
            &[reference_sym],
            &masters,
            &plugin_name,
            &mut interner,
        );
        assert_eq!(removed, 1);
        assert_eq!(count_lvlo(&record), 1);
        assert_eq!(first_uint(&record, "LLCT"), Some(1));
    }

    /// Null LVLO Reference (raw 0) is preserved, not dropped here — the
    /// null-Reference cleanup is the job of `clean_creature_esp_check_fields`;
    /// this fixup only drops template-pointing entries.
    #[test]
    fn null_lvlo_reference_is_not_dropped() {
        let mut interner = StringInterner::new();
        let reference_sym = interner.intern("Reference");
        let masters = vec!["Fallout4.esm".to_string()];
        let plugin_name = "Output.esp".to_string();
        let template_fk = FormKey {
            local: 0x000800,
            plugin: interner.intern(&plugin_name),
        };
        let mut template_set = HashSet::new();
        template_set.insert(template_fk);

        let mut record = make_record("LVLN", 0x000100, &plugin_name, &mut interner);
        // Null reference: raw u32 = 0.
        push_field(&mut record, "LVLO", FieldValue::Bytes(make_lvlo_bytes(0)));

        let removed = drop_template_lvlo_entries_for_test(
            &mut record,
            &template_set,
            &[reference_sym],
            &masters,
            &plugin_name,
            &mut interner,
        );
        assert_eq!(removed, 0, "null-Reference entry is not template-pointing");
        assert_eq!(count_lvlo(&record), 1);
    }

    /// Non-LVLO subrecords are preserved across the drop pass.
    #[test]
    fn non_lvlo_subrecords_preserved() {
        let mut interner = StringInterner::new();
        let reference_sym = interner.intern("Reference");
        let masters = vec!["Fallout4.esm".to_string()];
        let plugin_name = "Output.esp".to_string();
        let template_fk = FormKey {
            local: 0x000800,
            plugin: interner.intern(&plugin_name),
        };
        let mut template_set = HashSet::new();
        template_set.insert(template_fk);

        let mut record = make_record("LVLN", 0x000100, &plugin_name, &mut interner);
        // EDID before, OBND in middle, LLCT after — non-LVLO sigs.
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("TestLVLN")),
        );
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(make_lvlo_bytes(0x01_000800)),
        );
        push_field(&mut record, "LLCT", FieldValue::Uint(1));

        let removed = drop_template_lvlo_entries_for_test(
            &mut record,
            &template_set,
            &[reference_sym],
            &masters,
            &plugin_name,
            &mut interner,
        );
        assert_eq!(removed, 1);
        // EDID + LLCT survive; LVLO dropped.
        assert_eq!(record.fields.len(), 2);
        let edid_sig = SubrecordSig::from_str("EDID").unwrap();
        let llct_sig = SubrecordSig::from_str("LLCT").unwrap();
        assert!(record.fields.iter().any(|e| e.sig == edid_sig));
        assert!(record.fields.iter().any(|e| e.sig == llct_sig));
        assert_eq!(first_uint(&record, "LLCT"), Some(0));
    }

    /// `resolve_raw_form_id` master-byte routing matches
    /// `source_read::resolve_form_id` semantics.
    #[test]
    fn resolve_raw_form_id_routes_master_byte_correctly() {
        let mut interner = StringInterner::new();
        let masters = vec!["Fallout4.esm".to_string()];

        // master_byte == 0 → Fallout4.esm
        let fk1 = resolve_raw_form_id(0x00_001234, &masters, "Output.esp", &mut interner)
            .expect("non-null");
        assert_eq!(fk1.local, 0x001234);
        let plugin1 = interner.resolve(fk1.plugin).expect("plugin sym");
        assert_eq!(plugin1, "Fallout4.esm");

        // master_byte == 1 == own_index → Output.esp
        let fk2 = resolve_raw_form_id(0x01_000800, &masters, "Output.esp", &mut interner)
            .expect("non-null");
        assert_eq!(fk2.local, 0x000800);
        let plugin2 = interner.resolve(fk2.plugin).expect("plugin sym");
        assert_eq!(plugin2, "Output.esp");

        // raw == 0 → None (null FK).
        assert!(resolve_raw_form_id(0, &masters, "Output.esp", &mut interner).is_none());
    }
}
