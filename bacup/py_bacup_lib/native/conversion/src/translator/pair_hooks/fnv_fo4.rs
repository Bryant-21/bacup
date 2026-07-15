//! FnvFo4Hook — FNV→FO4 pair-level record hook.
//!
//! Ports the retired Python `FnvToFo4Hooks` implementation.
//!
//! # Behaviors ported
//!
//! 1. **Global field drop** (`pre_translate`) — removes subrecords whose
//!    four-byte sig is `SCRI`. Maps to `global_drop_fields=frozenset({"SCRI"})`
//!    in the Python constructor.
//!
//! 2. **SCRI metadata capture** — the Python `capture_metadata` extracts a
//!    deferred-script-link when a non-empty `SCRI` string is present. In the
//!    Rust port this is represented as a pure method `capture_scri_target`
//!    that returns the target string from the `SCRI` field value when present.
//!    The orchestrator calls it before the field is dropped.
//!
//! `post_translate` also normalizes model paths so any source-game-prefixed
//! model refs from earlier conversion stages are stripped before FO4 output.

use super::fo4_layouts::{self, SourceFamily};
use super::model_paths;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};

/// FNV→FO4 pair hook.
pub struct FnvFo4Hook;

const ARMA_MODEL_SIGS: &[[u8; 4]] = &[
    *b"MODL", *b"MOD2", *b"MOD3", *b"MOD4", *b"MOD5", *b"MODT", *b"MO2T", *b"MO3T", *b"MO4T",
    *b"MO5T", *b"MODS", *b"MO2S", *b"MO3S", *b"MO4S", *b"MO5S", *b"MODD", *b"MOSD",
];

fn struct_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
}

fn collect_named_values(
    value: &FieldValue,
    name: &str,
    interner: &crate::sym::StringInterner,
    output: &mut Vec<FieldValue>,
) {
    match value {
        FieldValue::Struct(fields) => {
            for (key, value) in fields {
                if interner.resolve(*key) == Some(name) {
                    output.push(value.clone());
                } else {
                    collect_named_values(value, name, interner, output);
                }
            }
        }
        FieldValue::List(items) => {
            for item in items {
                collect_named_values(item, name, interner, output);
            }
        }
        _ => {}
    }
}

fn relayout_arma_models(record: &mut Record, interner: &crate::sym::StringInterner) {
    // FNV MODL/MOD3 are male/female actor-biped meshes; MOD2/MOD4 are
    // ground-object meshes. FO4 splits actor meshes into biped MOD2/MOD3 and
    // first-person MOD4/MOD5, so the actor path supplies both target views.
    let mut male = Vec::new();
    let mut female = Vec::new();
    let mut output = Vec::with_capacity(record.fields.len());
    let mut insert_at = None;

    for entry in record.fields.drain(..) {
        if ARMA_MODEL_SIGS.contains(&entry.sig.0) {
            insert_at.get_or_insert(output.len());
            match entry.sig.0 {
                sig if sig == *b"MODL" => match &entry.value {
                    FieldValue::List(_) | FieldValue::Struct(_) => {
                        collect_named_values(&entry.value, "MODL", interner, &mut male);
                        collect_named_values(&entry.value, "MOD3", interner, &mut female);
                    }
                    _ => male.push(entry.value.clone()),
                },
                sig if sig == *b"MOD3" => match &entry.value {
                    FieldValue::List(_) | FieldValue::Struct(_) => {
                        collect_named_values(&entry.value, "MOD3", interner, &mut female);
                    }
                    _ => female.push(entry.value.clone()),
                },
                _ => {}
            }
        } else {
            output.push(entry);
        }
    }

    let Some(insert_at) = insert_at else {
        record.fields = output.into_iter().collect();
        return;
    };
    let mut replacements = Vec::with_capacity((male.len() + female.len()) * 2);
    for value in &male {
        replacements.push(FieldEntry {
            sig: SubrecordSig(*b"MOD2"),
            value: value.clone(),
        });
    }
    for value in &female {
        replacements.push(FieldEntry {
            sig: SubrecordSig(*b"MOD3"),
            value: value.clone(),
        });
    }
    for value in male {
        replacements.push(FieldEntry {
            sig: SubrecordSig(*b"MOD4"),
            value,
        });
    }
    for value in female {
        replacements.push(FieldEntry {
            sig: SubrecordSig(*b"MOD5"),
            value,
        });
    }
    output.splice(insert_at..insert_at, replacements);
    record.fields = output.into_iter().collect();
}

fn value_has_force_redraw(value: &FieldValue, interner: &crate::sym::StringInterner) -> bool {
    match value {
        FieldValue::Uint(value) => value & 2 != 0,
        FieldValue::Int(value) => *value >= 0 && (*value as u64) & 2 != 0,
        FieldValue::Bytes(bytes) => bytes.first().is_some_and(|value| value & 2 != 0),
        FieldValue::String(value) => interner
            .resolve(*value)
            .is_some_and(|value| value.eq_ignore_ascii_case("ForceRedraw")),
        FieldValue::List(values) => values
            .iter()
            .any(|value| value_has_force_redraw(value, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| value_has_force_redraw(value, interner)),
        _ => false,
    }
}

fn rewrite_structured_term_item(
    value: &mut FieldValue,
    interner: &crate::sym::StringInterner,
    item_id: u16,
) -> bool {
    let FieldValue::Struct(fields) = value else {
        return false;
    };
    let has_submenu = fields
        .iter()
        .any(|(key, _)| interner.resolve(*key) == Some("TNAM"));
    if !has_submenu {
        return false;
    }
    let force_redraw = fields.iter().any(|(key, value)| {
        interner.resolve(*key) == Some("ANAM") && value_has_force_redraw(value, interner)
    });
    fields.retain(|(key, _)| {
        !matches!(
            interner.resolve(*key),
            Some("ANAM") | Some("ITID") | Some("INAM")
        )
    });
    let insert_at = fields
        .iter()
        .position(|(key, _)| interner.resolve(*key) == Some("TNAM"))
        .unwrap_or(fields.len());
    fields.insert(
        insert_at,
        (
            interner.intern("ANAM"),
            FieldValue::Uint(if force_redraw { 6 } else { 4 }),
        ),
    );
    fields.insert(
        insert_at + 1,
        (interner.intern("ITID"), FieldValue::Uint(item_id as u64)),
    );
    true
}

fn rewrite_term_menu_rows(record: &mut Record, interner: &crate::sym::StringInterner) {
    let source: Vec<_> = record.fields.drain(..).collect();
    let mut output = Vec::with_capacity(source.len());
    let mut first_menu_at = None;
    let mut next_item_id = 1_u32;
    let mut index = 0;
    while index < source.len() {
        if source[index].sig.0 != *b"ITXT" {
            if !matches!(
                source[index].sig.0,
                sig if sig == *b"ISIZ"
                    || sig == *b"ANAM"
                    || sig == *b"ITID"
                    || sig == *b"INAM"
            ) {
                output.push(source[index].clone());
            }
            index += 1;
            continue;
        }

        if matches!(source[index].value, FieldValue::List(_)) {
            let mut entry = source[index].clone();
            let FieldValue::List(items) = &mut entry.value else {
                unreachable!();
            };
            items.retain_mut(|item| {
                let Ok(item_id) = u16::try_from(next_item_id) else {
                    return false;
                };
                let keep = rewrite_structured_term_item(item, interner, item_id);
                if keep {
                    next_item_id += 1;
                }
                keep
            });
            if !items.is_empty() {
                first_menu_at.get_or_insert(output.len());
                output.push(entry);
            }
            index += 1;
            continue;
        }
        if matches!(source[index].value, FieldValue::Struct(_)) {
            let mut entry = source[index].clone();
            if let Ok(item_id) = u16::try_from(next_item_id)
                && rewrite_structured_term_item(&mut entry.value, interner, item_id)
            {
                first_menu_at.get_or_insert(output.len());
                output.push(entry);
                next_item_id += 1;
            }
            index += 1;
            continue;
        }

        let end = source[index + 1..]
            .iter()
            .position(|entry| entry.sig.0 == *b"ITXT")
            .map_or(source.len(), |offset| index + 1 + offset);
        let row = &source[index..end];
        let Some(tnam_at) = row.iter().position(|entry| entry.sig.0 == *b"TNAM") else {
            index = end;
            continue;
        };
        let Ok(item_id) = u16::try_from(next_item_id) else {
            index = end;
            continue;
        };
        first_menu_at.get_or_insert(output.len());
        let force_redraw = row
            .iter()
            .filter(|entry| entry.sig.0 == *b"ANAM")
            .any(|entry| value_has_force_redraw(&entry.value, interner));
        for (row_index, entry) in row.iter().enumerate() {
            if row_index == tnam_at {
                output.push(FieldEntry {
                    sig: SubrecordSig(*b"ANAM"),
                    value: FieldValue::Uint(if force_redraw { 6 } else { 4 }),
                });
                output.push(FieldEntry {
                    sig: SubrecordSig(*b"ITID"),
                    value: FieldValue::Uint(item_id as u64),
                });
            }
            if !matches!(
                entry.sig.0,
                sig if sig == *b"ANAM" || sig == *b"ITID" || sig == *b"INAM"
            ) {
                output.push(entry.clone());
            }
        }
        next_item_id += 1;
        index = end;
    }
    if let Some(insert_at) = first_menu_at {
        output.insert(
            insert_at,
            FieldEntry {
                sig: SubrecordSig(*b"ISIZ"),
                value: FieldValue::Uint((next_item_id - 1) as u64),
            },
        );
    }
    record.fields = output.into_iter().collect();
}

fn uint_from_struct(
    value: &FieldValue,
    key: &str,
    interner: &crate::sym::StringInterner,
) -> Option<u64> {
    match value {
        FieldValue::Uint(value) => Some(*value),
        FieldValue::Int(value) if *value >= 0 => Some(*value as u64),
        FieldValue::Struct(fields) => match struct_value(fields, key, interner)? {
            FieldValue::Uint(value) => Some(*value),
            FieldValue::Int(value) if *value >= 0 => Some(*value as u64),
            _ => None,
        },
        _ => None,
    }
}

fn relayout_xrmr(value: &FieldValue, interner: &crate::sym::StringInterner) -> Option<FieldValue> {
    let count = match value {
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            u16::from_le_bytes([bytes[0], bytes[1]]) as u64
        }
        _ => uint_from_struct(value, "linked_rooms_count", interner)?,
    };
    let count = u8::try_from(count).ok()?;
    // FO4 narrows the count and replaces FNV's two unknown bytes with flags
    // plus fixed default bytes. Never reinterpret the source bytes as flags.
    match value {
        FieldValue::Bytes(_) => Some(FieldValue::Bytes(smallvec::smallvec![count, 0, 1, 0])),
        _ => Some(FieldValue::Struct(vec![
            (
                interner.intern("linked_rooms_count"),
                FieldValue::Uint(count as u64),
            ),
            (interner.intern("flags"), FieldValue::Uint(0)),
            (interner.intern("unknown_u8_2"), FieldValue::Uint(1)),
            (interner.intern("unknown_u8_3"), FieldValue::Uint(0)),
        ])),
    }
}

fn relayout_refr_xrmr(record: &mut Record, interner: &crate::sym::StringInterner) {
    let source: Vec<_> = record.fields.drain(..).collect();
    let mut output = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index].sig.0 != *b"XRMR" {
            output.push(source[index].clone());
            index += 1;
            continue;
        }
        if let Some(value) = relayout_xrmr(&source[index].value, interner) {
            output.push(FieldEntry {
                sig: source[index].sig,
                value,
            });
            index += 1;
        } else {
            index += 1;
            while index < source.len() && source[index].sig.0 == *b"XLRM" {
                index += 1;
            }
        }
    }
    record.fields = output.into_iter().collect();
}

fn relayout_addn_dnam(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        // The trailing FNV bytes are unknown, while FO4 treats them as flags.
        // Preserve only the shared particle-system cap and default FO4 flags.
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(FieldValue::Bytes(smallvec::smallvec![
                bytes[0], bytes[1], 0, 0
            ]))
        }
        _ => {
            let cap = uint_from_struct(value, "master_particle_system_cap", interner)?;
            let cap = u16::try_from(cap).ok()?;
            Some(FieldValue::Struct(vec![
                (
                    interner.intern("master_particle_system_cap"),
                    FieldValue::Uint(cap as u64),
                ),
                (interner.intern("flags"), FieldValue::Uint(0)),
            ]))
        }
    }
}

const PROJ_SOURCE_DATA_FO3_SIZE: usize = 68;
const PROJ_SOURCE_DATA_FNV_SIZE: usize = 84;
const PROJ_TARGET_DNAM_SIZE: usize = 93;
const PROJ_SHARED_FLAGS_MASK: u16 = 0x03ef;

fn read_u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn copy_four_bytes(source: &[u8], source_offset: usize, target: &mut [u8], target_offset: usize) {
    target[target_offset..target_offset + 4]
        .copy_from_slice(&source[source_offset..source_offset + 4]);
}

fn proj_type_for_fo4(source_type: u16) -> u16 {
    match source_type {
        // Missile, Lobber, Beam, and Flame have the same semantic values.
        1 | 2 | 4 | 8 => source_type,
        // FNV's Continuous Beam is represented by FO4's Beam type.
        16 => 4,
        // Unknown values, including zero, safely fall back to Missile.
        _ => 1,
    }
}

fn build_fo4_proj_dnam(source: Option<&[u8]>) -> Vec<u8> {
    let mut target = vec![0_u8; PROJ_TARGET_DNAM_SIZE];
    // A deterministic target-shape default for missing/malformed source DATA.
    target[2..4].copy_from_slice(&1_u16.to_le_bytes());

    let Some(source) = source.filter(|bytes| {
        matches!(
            bytes.len(),
            PROJ_SOURCE_DATA_FO3_SIZE | PROJ_SOURCE_DATA_FNV_SIZE
        )
    }) else {
        return target;
    };

    target[0..2].copy_from_slice(&(read_u16_at(source, 0) & PROJ_SHARED_FLAGS_MASK).to_le_bytes());
    target[2..4].copy_from_slice(&proj_type_for_fo4(read_u16_at(source, 2)).to_le_bytes());

    // Rebuild field-by-field. Source tracer chance (offset 24) and the FNV-only
    // rotations/bouncy tail (68..84) do not share target meanings and are
    // intentionally dropped. All FO4-only fields retain their safe zero defaults.
    for (source_offset, target_offset) in [
        (4, 4),   // gravity
        (8, 8),   // speed
        (12, 12), // range
        (16, 16), // light
        (20, 20), // muzzle flash light
        (28, 24), // explosion alternate-trigger proximity
        (32, 28), // explosion alternate-trigger timer
        (36, 32), // explosion
        (44, 40), // muzzle flash duration
        (48, 44), // fade duration
        (52, 48), // impact force
        (64, 60), // default weapon
    ] {
        copy_four_bytes(source, source_offset, &mut target, target_offset);
    }

    // FNV/FO3 sound slots reference legacy SOUN records, but FO4 DNAM requires
    // SNDR at target offsets 36, 52, and 56. The raw-ID rewrite remaps by object
    // id without a SOUN→SNDR semantic guarantee, and PROJ is not covered by the
    // later struct target-type validator, so these unsafe refs remain zero.

    // PairCtx exposes only the interner, so embedded raw FormIDs cannot be
    // remapped here. The always-on schema-aware
    // RewriteRawObjectTemplateFormIdsFixup later remaps PROJ.DNAM through the
    // final mapper context after these refs have landed at FO4 offsets.
    target
}

fn relayout_proj_data(record: &mut Record) {
    let source_data = record.fields.iter().find_map(|entry| {
        if entry.sig.0 != *b"DATA" {
            return None;
        }
        match &entry.value {
            FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
            _ => None,
        }
    });
    let target_dnam = build_fo4_proj_dnam(source_data);
    let insert_at = record
        .fields
        .iter()
        .position(|entry| matches!(entry.sig.0, sig if sig == *b"DATA" || sig == *b"DNAM" || sig == *b"NAM2"))
        .unwrap_or(record.fields.len());

    // Vanilla FO4's required contract is one empty DATA followed by one
    // exactly-93-byte DNAM. Source/raw DNAM and source-layout NAM2 model info
    // must never be copied into that target contract.
    record.fields.retain(|entry| {
        !matches!(entry.sig.0, sig if sig == *b"DATA" || sig == *b"DNAM" || sig == *b"NAM2")
    });
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(smallvec::SmallVec::new()),
        },
    );
    record.fields.insert(
        insert_at + 1,
        FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(target_dnam)),
        },
    );
}

impl FnvFo4Hook {
    /// The single global-drop sig for FNV→FO4: `SCRI` (legacy Papyrus script ref).
    const DROP_SIG: [u8; 4] = *b"SCRI";

    /// Drop the `SCRI` subrecord from the record before translation.
    fn drop_global_fields(record: &mut Record) {
        record.fields.retain(|entry| entry.sig.0 != Self::DROP_SIG);
    }

    fn drop_incompatible_fields(record: &mut Record, interner: &crate::sym::StringInterner) {
        match record.sig.0 {
            sig if sig == *b"ARMA" => relayout_arma_models(record, interner),
            // DEBR is a repeated DATA/MODT row format. FNV MODT bytes use the
            // source game's layout; the post-asset phase rebuilds FO4 MODT.
            sig if sig == *b"DEBR" => record.fields.retain(|entry| entry.sig.0 != *b"MODT"),
            sig if sig == *b"MUSC" => record.fields.retain(|entry| entry.sig.0 != *b"FNAM"),
            sig if sig == *b"INFO" => record.fields.retain(|entry| entry.sig.0 != *b"DNAM"),
            sig if sig == *b"TERM" => {
                // FNV SNAM is one four-byte looping-sound FormID. The FO4 v131
                // TERM loader uses the same 4CC for repeatable 24-byte sound
                // rows; preserving the FNV payload fail-fasts in that loader.
                // FNV PNAM is likewise a password NOTE, not FO4 marker color.
                record.fields.retain(
                    |entry| !matches!(entry.sig.0, field if field == *b"SNAM" || field == *b"PNAM"),
                );
                rewrite_term_menu_rows(record, interner);
            }
            sig if sig == *b"WEAP" => record.fields.retain(|entry| entry.sig.0 != *b"NNAM"),
            sig if sig == *b"REFR" => {
                record.fields.retain(|entry| entry.sig.0 != *b"XRDO");
                relayout_refr_xrmr(record, interner);
                fo4_layouts::normalize_refr_xloc(record, interner);
            }
            sig if sig == *b"ADDN" => {
                for entry in &mut record.fields {
                    if entry.sig.0 == *b"DNAM" {
                        entry.value =
                            relayout_addn_dnam(&entry.value, interner).unwrap_or(FieldValue::None);
                    }
                }
                record
                    .fields
                    .retain(|entry| entry.sig.0 != *b"DNAM" || entry.value != FieldValue::None);
            }
            sig if sig == *b"PROJ" => relayout_proj_data(record),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::LegacyFallout, interner)
            }
            sig if sig == *b"WTHR" => {
                fo4_layouts::normalize_wthr(record, SourceFamily::LegacyFallout, interner)
            }
            _ => {}
        }
    }

    /// Extract the SCRI target string from the record, if present and non-empty.
    ///
    /// Returns `None` when `SCRI` is absent, when its value is not a `String`,
    /// or when the resolved string is blank. The caller should invoke this
    /// **before** calling `pre_translate` (which drops `SCRI`).
    ///
    /// Mirrors Python:
    /// ```python
    /// scri_target = source.get("SCRI")
    /// if not isinstance(scri_target, str) or not scri_target.strip():
    ///     return {}
    /// return {"deferred_script_link": {"record_type": ..., "scri_target": scri_target}}
    /// ```
    pub fn capture_scri_target<'r>(
        record: &'r Record,
        interner: &'r crate::sym::StringInterner,
    ) -> Option<&'r str> {
        let scri_sig = SubrecordSig(*b"SCRI");
        let entry = record.fields.iter().find(|e| e.sig == scri_sig)?;
        let sym = match entry.value {
            crate::record::FieldValue::String(s) => s,
            _ => return None,
        };
        let s = interner.resolve(sym)?;
        let trimmed = s.trim();
        if trimmed.is_empty() { None } else { Some(s) }
    }
}

impl PairHook for FnvFo4Hook {
    /// Drop FNV-only global fields (`SCRI`) before field translation begins.
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::drop_global_fields(record);
        Self::drop_incompatible_fields(record, ctx.interner);
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        model_paths::normalize_model_paths(ctx.interner, record);
        Ok(())
    }

    /// No synthetic records produced by this pair.
    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

/// FO3→FO4 hook restricted to cross-game layouts proven shared with FNV.
///
/// FO3 records must not run the FNV-specific TERM/ARMA/REFR/ADDN rewrites in
/// `FnvFo4Hook`; only the explicit PROJ/REFR.XLOC/EFSH/WTHR contracts handled
/// here are shared. Unrelated FNV TERM/ARMA/ADDN rewrites remain excluded.
pub struct Fo3Fo4Hook;

impl PairHook for Fo3Fo4Hook {
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        match record.sig.0 {
            sig if sig == *b"PROJ" => relayout_proj_data(record),
            sig if sig == *b"REFR" => fo4_layouts::normalize_refr_xloc(record, ctx.interner),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::LegacyFallout, ctx.interner)
            }
            sig if sig == *b"WTHR" => {
                fo4_layouts::normalize_wthr(record, SourceFamily::LegacyFallout, ctx.interner)
            }
            _ => {}
        }
        Ok(())
    }

    fn post_translate(&self, _ctx: &mut PairCtx<'_>, _record: &mut Record) -> HookResult {
        Ok(())
    }

    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::sym::StringInterner;

    fn make_ctx(interner: &StringInterner) -> PairCtx<'_> {
        PairCtx { interner }
    }

    fn make_record(sig: &str, interner: &StringInterner) -> Record {
        let fk = FormKey::parse("000800@FalloutNV.esm", interner).unwrap();
        Record::new(SigCode::from_str(sig).unwrap(), fk)
    }

    fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: crate::ids::SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn bytes_from_hex(hex: &str) -> Vec<u8> {
        assert_eq!(hex.len() % 2, 0);
        (0..hex.len())
            .step_by(2)
            .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
            .collect()
    }

    fn raw_field<'a>(record: &'a Record, sig: &str) -> &'a [u8] {
        let fields: Vec<_> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == sig)
            .collect();
        assert_eq!(fields.len(), 1, "expected exactly one {sig}");
        let FieldValue::Bytes(bytes) = &fields[0].value else {
            panic!("{sig} must be raw bytes");
        };
        bytes.as_slice()
    }

    fn u16_at(bytes: &[u8], offset: usize) -> u16 {
        u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
    }

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    fn f32_at(bytes: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    // -------------------------------------------------------------------------
    // Behavior 1: global field drop (SCRI)
    // -------------------------------------------------------------------------

    #[test]
    fn pre_translate_drops_scri_subrecord() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        let scri_sym = interner.intern("SomeScript");
        push_field(&mut record, "SCRI", FieldValue::String(scri_sym));
        push_field(&mut record, "EDID", FieldValue::None);

        let hook = FnvFo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"SCRI"), "SCRI should be dropped");
        assert!(sigs.contains(&"EDID"), "EDID should be preserved");
    }

    #[test]
    fn pre_translate_is_noop_when_no_scri_field() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "FULL", FieldValue::None);

        let hook = FnvFo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields.len(), 2);
    }

    #[test]
    fn pre_translate_drops_all_scri_fields_when_multiple_present() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        let scri_sym = interner.intern("Script1");
        push_field(&mut record, "SCRI", FieldValue::String(scri_sym));
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "SCRI", FieldValue::String(scri_sym));

        let hook = FnvFo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"SCRI"));
        assert_eq!(sigs, vec!["EDID"]);
    }

    #[test]
    fn pre_translate_rebuilds_crashing_flame_projectile_data_and_drops_legacy_nam2() {
        const FLAME_PROJECTILE_ANT_DATA: &str = "8D0008000000000000803B460000204400000000000000000000000000000000000000000000000000000000CDCC4C3E0AD7233C0000C040000000000000000000000000";
        const FLAME_PROJECTILE_ANT_NAM2: &str = "B1B0106696E60762313010669BE6076273741074B3E1C96DB2B011662D9C07A132301166329C07A173651E74E527EFD8";

        let interner = StringInterner::new();
        let mut record = make_record("PROJ", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes_from_hex(
                FLAME_PROJECTILE_ANT_DATA,
            ))),
        );
        push_field(
            &mut record,
            "NAM2",
            FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes_from_hex(
                FLAME_PROJECTILE_ANT_NAM2,
            ))),
        );

        let mut ctx = make_ctx(&interner);
        FnvFo4Hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(raw_field(&record, "DATA").is_empty());
        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "NAM2")
        );
        let dnam = raw_field(&record, "DNAM");
        assert_eq!(dnam.len(), 93);
        assert_eq!(u16_at(dnam, 0), 141);
        assert_eq!(u16_at(dnam, 2), 8);
        assert_eq!(f32_at(dnam, 4), 0.0);
        assert_eq!(f32_at(dnam, 8), 12_000.0);
        assert_eq!(f32_at(dnam, 12), 640.0);
        for offset in [16, 20, 32, 36, 52, 56, 60, 80, 84, 89] {
            assert_eq!(u32_at(dnam, offset), 0, "raw ref at offset {offset}");
        }
        assert_eq!(f32_at(dnam, 24), 0.0);
        assert_eq!(f32_at(dnam, 28), 0.0);
        assert_eq!(f32_at(dnam, 40), 0.2);
        assert_eq!(f32_at(dnam, 44), 0.01);
        assert_eq!(f32_at(dnam, 48), 6.0);
        for offset in [64, 68, 72, 76] {
            assert_eq!(f32_at(dnam, offset), 0.0, "FO4-only float at {offset}");
        }
        assert_eq!(dnam[88], 0);
    }

    #[test]
    fn pre_translate_relayouts_84_byte_proj_and_preserves_compatible_refs() {
        let mut source = Vec::new();
        source.extend_from_slice(&0xffff_u16.to_le_bytes());
        source.extend_from_slice(&16_u16.to_le_bytes());
        for value in [1.25_f32, 2.5, 3.75] {
            source.extend_from_slice(&value.to_le_bytes());
        }
        source.extend_from_slice(&0x0011_1111_u32.to_le_bytes());
        source.extend_from_slice(&0x0022_2222_u32.to_le_bytes());
        source.extend_from_slice(&99.0_f32.to_le_bytes()); // dropped tracer chance
        source.extend_from_slice(&4.25_f32.to_le_bytes());
        source.extend_from_slice(&5.5_f32.to_le_bytes());
        source.extend_from_slice(&0x0033_3333_u32.to_le_bytes());
        source.extend_from_slice(&0x0044_4444_u32.to_le_bytes());
        for value in [6.75_f32, 7.0, 8.5] {
            source.extend_from_slice(&value.to_le_bytes());
        }
        source.extend_from_slice(&0x0055_5555_u32.to_le_bytes());
        source.extend_from_slice(&0x0066_6666_u32.to_le_bytes());
        source.extend_from_slice(&0x0077_7777_u32.to_le_bytes());
        for value in [9.0_f32, 10.0, 11.0, 12.0] {
            source.extend_from_slice(&value.to_le_bytes());
        }
        assert_eq!(source.len(), 84);

        let interner = StringInterner::new();
        let mut record = make_record("PROJ", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(smallvec::SmallVec::from_vec(source)),
        );
        let mut ctx = make_ctx(&interner);
        FnvFo4Hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = raw_field(&record, "DNAM");
        assert_eq!(dnam.len(), 93);
        assert_eq!(u16_at(dnam, 0), 0x03ef);
        assert_eq!(u16_at(dnam, 2), 4); // FNV Continuous Beam -> FO4 Beam
        for (offset, expected) in [
            (4, 1.25_f32),
            (8, 2.5),
            (12, 3.75),
            (24, 4.25),
            (28, 5.5),
            (40, 6.75),
            (44, 7.0),
            (48, 8.5),
        ] {
            assert_eq!(f32_at(dnam, offset), expected);
        }
        for (offset, expected) in [
            (16, 0x0011_1111),
            (20, 0x0022_2222),
            (32, 0x0033_3333),
            (60, 0x0077_7777),
        ] {
            assert_eq!(u32_at(dnam, offset), expected);
        }
        for offset in [36, 52, 56] {
            assert_eq!(
                u32_at(dnam, offset),
                0,
                "legacy SOUN ref at target offset {offset}"
            );
        }
        assert_eq!(&dnam[64..93], &[0; 29]);
    }

    #[test]
    fn pre_translate_defaults_malformed_proj_and_deduplicates_target_contract() {
        let interner = StringInterner::new();
        let mut record = make_record("PROJ", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(smallvec::smallvec![1, 2, 3]),
        );
        push_field(&mut record, "DATA", FieldValue::None);
        push_field(
            &mut record,
            "DNAM",
            FieldValue::Bytes(smallvec::smallvec![9]),
        );
        push_field(
            &mut record,
            "NAM2",
            FieldValue::Bytes(smallvec::smallvec![8]),
        );

        let mut ctx = make_ctx(&interner);
        FnvFo4Hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(raw_field(&record, "DATA").is_empty());
        let dnam = raw_field(&record, "DNAM");
        assert_eq!(dnam.len(), 93);
        assert_eq!(u16_at(dnam, 0), 0);
        assert_eq!(u16_at(dnam, 2), 1);
        assert!(dnam[4..].iter().all(|byte| *byte == 0));
        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "NAM2")
        );
    }

    #[test]
    fn pre_translate_drops_fnv_debr_legacy_modt_rows() {
        let interner = StringInterner::new();
        let mut record = make_record("DEBR", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(smallvec::smallvec![50, b'a', 0, 1]),
        );
        push_field(
            &mut record,
            "MODT",
            FieldValue::Bytes(smallvec::smallvec![0x60, 0x0f, 0xf3, 0x85]),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![(interner.intern("percentage"), FieldValue::Uint(50))]),
        );
        push_field(
            &mut record,
            "MODT",
            FieldValue::Bytes(smallvec::smallvec![1, 2]),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["DATA", "DATA"]
        );
    }

    #[test]
    fn pre_translate_drops_term_snam_because_fo4_v131_expects_24_byte_sound_rows() {
        let interner = StringInterner::new();
        let mut term = make_record("TERM", &interner);
        push_field(
            &mut term,
            "EDID",
            FieldValue::String(interner.intern("Terminal")),
        );
        push_field(
            &mut term,
            "SNAM",
            FieldValue::Bytes(smallvec::smallvec![0x34, 0x12, 0, 0]),
        );
        push_field(
            &mut term,
            "SNAM",
            FieldValue::Struct(vec![(interner.intern("sound"), FieldValue::Uint(0x1234))]),
        );
        push_field(
            &mut term,
            "DNAM",
            FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut term)
            .unwrap();

        assert_eq!(
            term.fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID", "DNAM"]
        );

        let mut non_term = make_record("ACTI", &interner);
        push_field(
            &mut non_term,
            "SNAM",
            FieldValue::Bytes(smallvec::smallvec![0x34, 0x12, 0, 0]),
        );
        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut non_term)
            .unwrap();
        assert_eq!(non_term.fields[0].sig.as_str(), "SNAM");
    }

    #[test]
    fn pre_translate_relayouts_fnv_arma_actor_models_and_drops_source_companions() {
        let interner = StringInterner::new();
        let male = interner.intern("Armor\\Male.nif");
        let female = interner.intern("Armor\\Female.nif");
        let mut record = make_record("ARMA", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "MODL", FieldValue::String(male));
        push_field(
            &mut record,
            "MODT",
            FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
        );
        push_field(
            &mut record,
            "MOD2",
            FieldValue::String(interner.intern("Armor\\MaleGO.nif")),
        );
        push_field(
            &mut record,
            "MO2S",
            FieldValue::Bytes(smallvec::smallvec![1, 0, 0, 0]),
        );
        push_field(&mut record, "MOD3", FieldValue::String(female));
        push_field(
            &mut record,
            "MO3T",
            FieldValue::Struct(vec![(interner.intern("legacy"), FieldValue::Uint(1))]),
        );
        push_field(
            &mut record,
            "MOD4",
            FieldValue::String(interner.intern("Armor\\FemaleGO.nif")),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID", "MOD2", "MOD3", "MOD4", "MOD5"]
        );
        assert_eq!(record.fields[1].value, FieldValue::String(male));
        assert_eq!(record.fields[2].value, FieldValue::String(female));
        assert_eq!(record.fields[3].value, FieldValue::String(male));
        assert_eq!(record.fields[4].value, FieldValue::String(female));
    }

    #[test]
    fn pre_translate_relayouts_structured_fnv_arma_model_rows() {
        let interner = StringInterner::new();
        let male = interner.intern("Armor\\Male.nif");
        let female = interner.intern("Armor\\Female.nif");
        let mut record = make_record("ARMA", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::List(vec![FieldValue::Struct(vec![
                (interner.intern("MODL"), FieldValue::String(male)),
                (
                    interner.intern("MOD2"),
                    FieldValue::String(interner.intern("Armor\\MaleGO.nif")),
                ),
                (interner.intern("MOD3"), FieldValue::String(female)),
                (
                    interner.intern("MOD4"),
                    FieldValue::String(interner.intern("Armor\\FemaleGO.nif")),
                ),
                (
                    interner.intern("MODT"),
                    FieldValue::Bytes(smallvec::smallvec![1, 2]),
                ),
            ])]),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| (field.sig.as_str(), field.value.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("MOD2", FieldValue::String(male)),
                ("MOD3", FieldValue::String(female)),
                ("MOD4", FieldValue::String(male)),
                ("MOD5", FieldValue::String(female)),
            ]
        );
    }

    #[test]
    fn pre_translate_drops_raw_and_structured_same_4cc_semantic_collisions() {
        let interner = StringInterner::new();
        for (record_sig, field_sig) in [
            ("MUSC", "FNAM"),
            ("INFO", "DNAM"),
            ("TERM", "PNAM"),
            ("WEAP", "NNAM"),
            ("REFR", "XRDO"),
        ] {
            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                field_sig,
                FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
            );
            push_field(
                &mut record,
                field_sig,
                FieldValue::Struct(vec![(interner.intern("source"), FieldValue::Uint(1))]),
            );
            push_field(&mut record, "EDID", FieldValue::None);

            FnvFo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(
                record
                    .fields
                    .iter()
                    .map(|field| field.sig.as_str())
                    .collect::<Vec<_>>(),
                vec!["EDID"],
                "{record_sig}.{field_sig} must not pass through"
            );
        }
    }

    #[test]
    fn pre_translate_preserves_collision_4ccs_in_unrelated_record_contexts() {
        let interner = StringInterner::new();
        let mut record = make_record("STAT", &interner);
        for sig in ["FNAM", "DNAM", "PNAM", "ANAM", "NNAM", "XRDO", "XRMR"] {
            push_field(
                &mut record,
                sig,
                FieldValue::Bytes(smallvec::smallvec![1, 2, 3, 4]),
            );
        }

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["FNAM", "DNAM", "PNAM", "ANAM", "NNAM", "XRDO", "XRMR"]
        );
    }

    #[test]
    fn pre_translate_maps_raw_term_submenu_rows_and_drops_unsupported_rows() {
        let interner = StringInterner::new();
        let submenu = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
        let note = FormKey::parse("005678@FalloutNV.esm", &interner).unwrap();
        let submenu_2 = FormKey::parse("009ABC@FalloutNV.esm", &interner).unwrap();
        let mut record = make_record("TERM", &interner);
        push_field(&mut record, "ISIZ", FieldValue::Uint(99));
        push_field(
            &mut record,
            "ITXT",
            FieldValue::String(interner.intern("Submenu")),
        );
        push_field(
            &mut record,
            "RNAM",
            FieldValue::String(interner.intern("Loading")),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(smallvec::smallvec![2]),
        );
        push_field(&mut record, "ITID", FieldValue::Uint(99));
        push_field(&mut record, "TNAM", FieldValue::FormKey(submenu));
        push_field(
            &mut record,
            "ITXT",
            FieldValue::String(interner.intern("Read note")),
        );
        push_field(&mut record, "ANAM", FieldValue::Uint(1));
        push_field(&mut record, "ITID", FieldValue::Uint(88));
        push_field(&mut record, "INAM", FieldValue::FormKey(note));
        push_field(
            &mut record,
            "ITXT",
            FieldValue::String(interner.intern("Submenu 2")),
        );
        push_field(&mut record, "ANAM", FieldValue::Uint(0));
        push_field(&mut record, "TNAM", FieldValue::FormKey(submenu_2));

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec![
                "ISIZ", "ITXT", "RNAM", "ANAM", "ITID", "TNAM", "ITXT", "ANAM", "ITID", "TNAM"
            ]
        );
        assert_eq!(record.fields[0].value, FieldValue::Uint(2));
        assert_eq!(record.fields[3].value, FieldValue::Uint(6));
        assert_eq!(record.fields[4].value, FieldValue::Uint(1));
        assert_eq!(record.fields[7].value, FieldValue::Uint(4));
        assert_eq!(record.fields[8].value, FieldValue::Uint(2));
    }

    #[test]
    fn pre_translate_maps_structured_term_submenu_rows() {
        let interner = StringInterner::new();
        let submenu = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
        let note = FormKey::parse("005678@FalloutNV.esm", &interner).unwrap();
        let submenu_2 = FormKey::parse("009ABC@FalloutNV.esm", &interner).unwrap();
        let mut record = make_record("TERM", &interner);
        push_field(&mut record, "ISIZ", FieldValue::Uint(99));
        push_field(
            &mut record,
            "ITXT",
            FieldValue::List(vec![
                FieldValue::Struct(vec![
                    (
                        interner.intern("ITXT"),
                        FieldValue::String(interner.intern("Submenu")),
                    ),
                    (interner.intern("ANAM"), FieldValue::Uint(0)),
                    (interner.intern("ITID"), FieldValue::Uint(99)),
                    (interner.intern("TNAM"), FieldValue::FormKey(submenu)),
                ]),
                FieldValue::Struct(vec![
                    (
                        interner.intern("ITXT"),
                        FieldValue::String(interner.intern("Read note")),
                    ),
                    (interner.intern("ANAM"), FieldValue::Uint(1)),
                    (interner.intern("ITID"), FieldValue::Uint(88)),
                    (interner.intern("INAM"), FieldValue::FormKey(note)),
                ]),
                FieldValue::Struct(vec![
                    (
                        interner.intern("ITXT"),
                        FieldValue::String(interner.intern("Submenu 2")),
                    ),
                    (interner.intern("ANAM"), FieldValue::Uint(2)),
                    (interner.intern("TNAM"), FieldValue::FormKey(submenu_2)),
                ]),
            ]),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.fields[0].sig.as_str(), "ISIZ");
        assert_eq!(record.fields[0].value, FieldValue::Uint(2));
        let FieldValue::List(items) = &record.fields[1].value else {
            panic!("expected structured menu item list");
        };
        assert_eq!(items.len(), 2);
        let FieldValue::Struct(fields) = &items[0] else {
            panic!("expected structured menu item");
        };
        assert_eq!(
            fields
                .iter()
                .map(|(key, _)| interner.resolve(*key).unwrap())
                .collect::<Vec<_>>(),
            vec!["ITXT", "ANAM", "ITID", "TNAM"]
        );
        assert_eq!(fields[1].1, FieldValue::Uint(4));
        assert_eq!(fields[2].1, FieldValue::Uint(1));
        let FieldValue::Struct(fields) = &items[1] else {
            panic!("expected second structured menu item");
        };
        assert_eq!(fields[1].1, FieldValue::Uint(6));
        assert_eq!(fields[2].1, FieldValue::Uint(2));
    }

    #[test]
    fn pre_translate_removes_term_count_and_item_ids_when_all_rows_drop() {
        let interner = StringInterner::new();
        let note = FormKey::parse("005678@FalloutNV.esm", &interner).unwrap();
        let mut record = make_record("TERM", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "ISIZ", FieldValue::Uint(1));
        push_field(
            &mut record,
            "ITXT",
            FieldValue::String(interner.intern("Read note")),
        );
        push_field(&mut record, "ANAM", FieldValue::Uint(1));
        push_field(&mut record, "ITID", FieldValue::Uint(77));
        push_field(&mut record, "INAM", FieldValue::FormKey(note));

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID"]
        );
    }

    #[test]
    fn pre_translate_relayouts_raw_refr_xrmr() {
        let interner = StringInterner::new();
        let room = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
        let mut record = make_record("REFR", &interner);
        push_field(
            &mut record,
            "XRMR",
            FieldValue::Bytes(smallvec::smallvec![2, 0, 0xAA, 0xBB]),
        );
        push_field(&mut record, "XLRM", FieldValue::FormKey(room));

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(smallvec::smallvec![2, 0, 1, 0])
        );
        assert_eq!(record.fields[1].sig.as_str(), "XLRM");
    }

    #[test]
    fn pre_translate_relayouts_structured_refr_xrmr() {
        let interner = StringInterner::new();
        let mut record = make_record("REFR", &interner);
        push_field(
            &mut record,
            "XRMR",
            FieldValue::Struct(vec![
                (interner.intern("linked_rooms_count"), FieldValue::Uint(3)),
                (interner.intern("unknown_u8_1"), FieldValue::Uint(0xAA)),
                (interner.intern("unknown_u8_2"), FieldValue::Uint(0xBB)),
            ]),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured XRMR");
        };
        assert_eq!(
            fields
                .iter()
                .map(|(key, value)| (interner.resolve(*key).unwrap(), value.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("linked_rooms_count", FieldValue::Uint(3)),
                ("flags", FieldValue::Uint(0)),
                ("unknown_u8_2", FieldValue::Uint(1)),
                ("unknown_u8_3", FieldValue::Uint(0)),
            ]
        );
    }

    #[test]
    fn pre_translate_drops_overflowing_refr_xrmr_row() {
        let interner = StringInterner::new();
        let room = FormKey::parse("001234@FalloutNV.esm", &interner).unwrap();
        let mut record = make_record("REFR", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "XRMR",
            FieldValue::Bytes(smallvec::smallvec![0, 1, 0, 0]),
        );
        push_field(&mut record, "XLRM", FieldValue::FormKey(room));
        push_field(&mut record, "DATA", FieldValue::None);

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record
                .fields
                .iter()
                .map(|field| field.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID", "DATA"]
        );
    }

    #[test]
    fn pre_translate_relayouts_raw_addn_dnam_with_safe_flags() {
        let interner = StringInterner::new();
        let mut record = make_record("ADDN", &interner);
        push_field(
            &mut record,
            "DNAM",
            FieldValue::Bytes(smallvec::smallvec![0x34, 0x12, 0xAA, 0xBB]),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(
            record.fields[0].value,
            FieldValue::Bytes(smallvec::smallvec![0x34, 0x12, 0, 0])
        );
    }

    #[test]
    fn pre_translate_relayouts_structured_addn_dnam_with_safe_flags() {
        let interner = StringInterner::new();
        let mut record = make_record("ADDN", &interner);
        push_field(
            &mut record,
            "DNAM",
            FieldValue::Struct(vec![
                (
                    interner.intern("master_particle_system_cap"),
                    FieldValue::Uint(0x1234),
                ),
                (interner.intern("unknown_u8_1"), FieldValue::Uint(0xAA)),
                (interner.intern("unknown_u8_2"), FieldValue::Uint(0xBB)),
            ]),
        );

        FnvFo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured ADDN.DNAM");
        };
        assert_eq!(
            fields
                .iter()
                .map(|(key, value)| (interner.resolve(*key).unwrap(), value.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("master_particle_system_cap", FieldValue::Uint(0x1234)),
                ("flags", FieldValue::Uint(0)),
            ]
        );
    }

    // -------------------------------------------------------------------------
    // Behavior 2: SCRI metadata capture
    // -------------------------------------------------------------------------

    #[test]
    fn capture_scri_target_returns_script_name_when_present() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        let scri_sym = interner.intern("MyCustomScript");
        push_field(&mut record, "SCRI", FieldValue::String(scri_sym));

        let result = FnvFo4Hook::capture_scri_target(&record, &interner);
        assert_eq!(result, Some("MyCustomScript"));
    }

    #[test]
    fn capture_scri_target_returns_none_when_scri_absent() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);

        let result = FnvFo4Hook::capture_scri_target(&record, &interner);
        assert!(result.is_none());
    }

    #[test]
    fn capture_scri_target_returns_none_when_scri_is_empty_string() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        let scri_sym = interner.intern("   ");
        push_field(&mut record, "SCRI", FieldValue::String(scri_sym));

        let result = FnvFo4Hook::capture_scri_target(&record, &interner);
        assert!(result.is_none());
    }

    #[test]
    fn capture_scri_target_returns_none_when_scri_value_is_not_string() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        push_field(&mut record, "SCRI", FieldValue::Int(42));

        let result = FnvFo4Hook::capture_scri_target(&record, &interner);
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------------
    // post_translate / synthesize_records
    // -------------------------------------------------------------------------

    #[test]
    fn post_translate_leaves_unprefixed_model_paths_unprefixed() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Landscape\\Grass\\WastelandGrass01.nif")),
        );

        let hook = FnvFo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Landscape\\Grass\\WastelandGrass01.nif")
        );
    }

    #[test]
    fn post_translate_strips_source_prefixed_model_paths() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("fnv\\Landscape\\Grass\\WastelandGrass01.nif")),
        );

        let hook = FnvFo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Landscape\\Grass\\WastelandGrass01.nif")
        );
    }

    #[test]
    fn post_translate_strips_meshes_and_source_prefix_from_model_paths() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(
                interner.intern("Meshes\\fnv\\Landscape\\Grass\\WastelandGrass01.nif"),
            ),
        );

        let hook = FnvFo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Landscape\\Grass\\WastelandGrass01.nif")
        );
    }

    #[test]
    fn synthesize_records_returns_empty() {
        let mut interner = StringInterner::new();
        let hook = FnvFo4Hook;
        let mut ctx = make_ctx(&mut interner);
        assert!(hook.synthesize_records(&mut ctx).is_empty());
    }
}
