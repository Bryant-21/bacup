//! Fixup: remove LeveledNpc entries that reference template-actor NPCs.
//!
//! The CK rejects leveled template actors: an LVLN entry whose NPC inherits
//! data through `TemplateActors` (`TPTA`) errors on load. Entries pointing at an
//! NPC_ with any populated TPTA slot, in the output or a target master, are
//! dropped. No-op on non-creature single-root runs.
//!
//! `TPTA` is 13 FormID slots: traits, stats, factions, spell_list, ai_data,
//! ai_packages, model_animation, base_data (slot 7, offset 28), inventory,
//! script, def_package_list, attack_data, keywords. `LVLO`
//! (`struct:H,B,B,I,H,B,B`, 12 bytes) holds the reference FormID at offset 4.
//! Entries arrive as `FieldValue::Bytes` or `FieldValue::Struct`.

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

/// Drop every `LVLO` entry in `record` whose `Reference` is a template NPC.
/// Returns the number of entries removed.
///
/// Handles `FieldValue::Bytes` (raw 12-byte payload, FormID at offset 4..8
/// resolved against `target_masters` / `target_plugin_name`) and
/// `FieldValue::Struct`.
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

    /// Whole-plugin runs have no root_sig, and still apply.
    #[test]
    fn applies_to_npc_lvln_and_whole_plugin_roots() {
        let schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        for (root, expected) in [
            (Some("NPC_"), true),
            (Some("LVLN"), true),
            (Some("WEAP"), false),
            (None, true),
        ] {
            let config = FixupConfig {
                root_sig: root.map(|sig| SigCode::from_str(sig).unwrap()),
                ..Default::default()
            };
            let mut ctx_interner = StringInterner::new();
            let ctx = make_test_ctx(&schema, &config, &mut ctx_interner);
            assert_eq!(
                FilterLcharTemplateNpcsFixup.applies_to(&ctx),
                expected,
                "{root:?}"
            );
        }
        let target_handle =
            plugin_handle_new_native("FilterLcharTemplateNpcsFixupTest.esp", Some("fo4"))
                .expect("test plugin handle");
        let session = open_session(target_handle, None).expect("open session");
        assert!(FilterLcharTemplateNpcsFixup.applies_to_session(&session, &FixupConfig::default()));
    }

    // ── tpta_has_template_actor — Bytes shape ─────────────────────────────

    #[test]
    fn tpta_has_template_actor_reads_bytes_and_struct_shapes() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let target_fk = FormKey {
            local: 0x00_ABCDEF,
            plugin: fallout4,
        };
        let null_fk = FormKey {
            local: 0,
            plugin: fallout4,
        };
        let slots_with = |index: usize| {
            let mut slots = [0u32; 13];
            slots[index] = 0x00_ABCDEF;
            FieldValue::Bytes(make_tpta_bytes(slots))
        };
        for (name, tpta, expected) in [
            ("bytes_base_data", Some(slots_with(7)), true),
            ("bytes_traits", Some(slots_with(0)), true),
            (
                "bytes_all_zero",
                Some(FieldValue::Bytes(make_tpta_bytes([0u32; 13]))),
                false,
            ),
            ("absent", None, false),
            (
                "bytes_too_short",
                Some(FieldValue::Bytes(smallvec::SmallVec::from_slice(
                    &[0xFFu8; 3],
                ))),
                false,
            ),
            (
                "struct_base_data",
                Some(FieldValue::Struct(vec![
                    (interner.intern("traits"), FieldValue::FormKey(null_fk)),
                    (interner.intern("base_data"), FieldValue::FormKey(target_fk)),
                ])),
                true,
            ),
            (
                "struct_authoring_traits",
                Some(FieldValue::Struct(vec![(
                    interner.intern("Traits"),
                    FieldValue::FormKey(target_fk),
                )])),
                true,
            ),
            (
                "struct_only_null",
                Some(FieldValue::Struct(vec![
                    (interner.intern("Traits"), FieldValue::FormKey(null_fk)),
                    (interner.intern("base_data"), FieldValue::FormKey(null_fk)),
                ])),
                false,
            ),
        ] {
            let mut record = make_record("NPC_", 0x000100, "Output.esp", &interner);
            if let Some(tpta) = tpta {
                push_field(&mut record, "TPTA", tpta);
            }
            assert_eq!(tpta_has_template_actor(&record), expected, "{name}");
        }
    }

    // ── tpta_has_template_actor — Struct shape ────────────────────────────

    // ── drop_template_lvlo_entries ────────────────────────────────────────

    /// Only template-pointing entries go: a null Reference (raw 0) is the job of
    /// `clean_creature_esp_check_fields`, and non-LVLO subrecords survive.
    #[test]
    fn drops_byte_lvlo_entries_pointing_to_template_npcs() {
        let masters = vec!["Fallout4.esm".to_string()];
        let plugin_name = "Output.esp";
        for (name, use_template_set, raws, removed, remaining) in [
            (
                "own_template",
                true,
                vec![0x01_000800, 0x00_001234, 0x01_000800],
                2,
                1,
            ),
            (
                "master_template",
                false,
                vec![0x00_0D228A, 0x00_001234],
                1,
                1,
            ),
            (
                "empty_template_set",
                false,
                vec![0x01_000800, 0x00_001234],
                0,
                2,
            ),
            ("null_reference", true, vec![0], 0, 1),
        ] {
            let interner = StringInterner::new();
            let reference_sym = interner.intern("Reference");
            let fo4_sym = interner.intern("Fallout4.esm");
            let mut master_templates = |fk: &FormKey| fk.plugin == fo4_sym && fk.local == 0x0D228A;
            let mut template_set = HashSet::new();
            if use_template_set {
                template_set.insert(FormKey {
                    local: 0x000800,
                    plugin: interner.intern(plugin_name),
                });
            }

            let mut record = make_record("LVLN", 0x000100, plugin_name, &interner);
            push_field(
                &mut record,
                "EDID",
                FieldValue::String(interner.intern("TestLVLN")),
            );
            push_field(&mut record, "LLCT", FieldValue::Uint(raws.len() as u64));
            for raw in &raws {
                push_field(
                    &mut record,
                    "LVLO",
                    FieldValue::Bytes(make_lvlo_bytes(*raw)),
                );
            }

            let dropped = drop_template_lvlo_entries(
                &mut record,
                &template_set,
                &mut master_templates,
                &[reference_sym],
                &masters,
                plugin_name,
                &interner,
            );
            assert_eq!(dropped, removed, "{name}");
            assert_eq!(count_lvlo(&record), remaining, "{name}");
            assert_eq!(
                first_uint(&record, "LLCT"),
                Some(remaining as u64),
                "{name}"
            );
            assert_eq!(
                record.fields.len(),
                remaining + 2,
                "{name}: EDID and LLCT survive"
            );
        }
    }

    #[test]
    fn drops_struct_lvlo_entries_pointing_to_template_npcs() {
        for key in ["Reference", "NPC"] {
            let interner = StringInterner::new();
            let reference_sym = interner.intern(key);
            let masters = vec!["Fallout4.esm".to_string()];
            let plugin_name = "Output.esp".to_string();
            let template_fk = FormKey {
                local: 0x000800,
                plugin: interner.intern(&plugin_name),
            };
            let template_set = HashSet::from([template_fk]);
            let other_fk = FormKey {
                local: 0x001234,
                plugin: interner.intern("Fallout4.esm"),
            };

            let mut record = make_record("LVLN", 0x000100, &plugin_name, &interner);
            push_field(&mut record, "LLCT", FieldValue::Uint(2));
            for fk in [template_fk, other_fk] {
                push_field(
                    &mut record,
                    "LVLO",
                    FieldValue::Struct(vec![(reference_sym, FieldValue::FormKey(fk))]),
                );
            }

            let removed = drop_template_lvlo_entries_for_test(
                &mut record,
                &template_set,
                &[reference_sym],
                &masters,
                &plugin_name,
                &interner,
            );
            assert_eq!(removed, 1, "{key}");
            assert_eq!(count_lvlo(&record), 1, "{key}");
            assert_eq!(first_uint(&record, "LLCT"), Some(1), "{key}");
        }
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
