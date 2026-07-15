//! Fixup: repair/null VMAD script-property Object FormIDs
//! that resolve to neither the output plugin nor any target master.
//!
//! # Root cause
//! VMAD script properties carry Object-union FormIDs *inside* the opaque VMAD
//! subrecord blob. `FormKeyMapper::rewrite_vmad_formids` (the translate-time
//! codec) walks that blob and remaps every object FormID whose source record
//! WAS translated — those land on a `07`-prefix output record and resolve fine
//! (verified: all 859 ACTI 07-prefix VMAD refs resolve, 0 dangling). The
//! residue is object FormIDs that point at FO76 source records living OUTSIDE
//! the converted APPALACHIA slice (interior REFR/CELL/ACHR, un-emitted SCEN /
//! CHAL): the codec finds no mapping, so it leaves the source `00`-prefix
//! value, which then dangles as `Fallout4.esm:00xxxxxx` in the output.
//!
//! Census of every 00-prefix VMAD object FormID across the touched record
//! types (ACTI/TERM/DOOR/NOTE/CONT/MGEF): 754 resolve in Fallout4.esm
//! (legitimate base-game refs — KEPT byte-identical), 0 were a codec miss
//! (an emitted 07 own-record the codec failed to remap), and ~217 point at
//! source records that were never emitted (REFR 163, CELL 41, ACHR 8, SCEN 4,
//! CHAL 1). The FO4-correct representation of a script property whose target
//! legitimately does not exist is NULL.
//!
//! # Post-copy deferral (whole-plugin FO76→FO4)
//! When interiors ARE emitted (`include_interior`), the interior CELL + its
//! placed children (incl. teleport-marker REFRs) land in the output only in the
//! post-copy asset wave — AFTER this pre-copy sweep. Nulling here would clobber a
//! VMAD Object property whose target is present post-copy (e.g. a shelter door's
//! `ShelterCell` / `ShelterCellTeleportPosition`). So under
//! `FixupConfig::defer_placed_child_ref_class` the pre-copy sweep runs with
//! `defer_null` — it still REPAIRS refs that already resolve but LEAVES refs that
//! resolve nowhere intact; `repair_dangling_vmad_refs` (called from
//! `ConversionRun::repair_placed_child_refs`) then runs the authoritative
//! repair/null over the now-complete output. Mirrors the deferral in
//! `null_dangling_own_plugin_refs`.
//!
//! This is the VMAD-blob counterpart of `null_dangling_own_plugin_refs`,
//! which handles the typed FormKey-leaf and value-selected-union ref
//! slots but explicitly does NOT walk inside the VMAD blob (VMAD decodes to an
//! opaque `Bytes` field). The two passes are disjoint by subrecord domain.
//!
//! # Plugin-aware
//! Each object FormID is judged on its full encoded `(master_index, object_id)`
//! against the authoritative object-id sets of the output plugin and each named
//! master. A FormID that resolves in its addressed handle is left byte-identical
//! — the legitimate Fallout4.esm refs (and any master ref) are never clobbered.
//! If the addressed master does not contain the object but the output plugin
//! does, the FormID is repaired to the output master byte. Only a FormID that
//! resolves NOWHERE is nulled.
//!
//! The byte traversal mirrors `FormKeyMapper::rewrite_vmad_formids` exactly so
//! the same property-value layouts (types 1/7/11/17) are reached and the same
//! 4-byte FormID slot is acted on. On any malformed/truncated blob the walk
//! aborts (returns no change for that record) rather than guess.

use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;

/// Record signatures whose VMAD script-property Object FormIDs are checked.
/// Restricting to this allow-list bounds the per-record decode work and keeps
/// the pass off record types whose VMAD danglers (if any) are out of scope.
///
/// INFO carries VMAD script-property Object FormIDs in the standard Scripts
/// section, which the blob walk traverses. The walk stops after `script_count`
/// scripts and never reads the INFO fragment trailer, so listing INFO only
/// reaches the script-property objects.
///
/// BOOK/FURN/MISC/MSTT/NPC_/REFR and SCEN fragment scripts carry the same
/// standard VMAD object-property layouts and use this same resolver policy:
/// keep resolving target-master/output refs; null only slots that resolve
/// nowhere.
pub(crate) const TOUCHED_RECORD_SIGS: &[&str] = &[
    "ACTI", "TERM", "DOOR", "NOTE", "CONT", "MGEF", "PACK", "INFO", "QUST", "BOOK", "FURN", "MISC",
    "MSTT", "NPC_", "REFR", "SCEN",
];

pub struct NullDanglingVmadRefsFixup;

impl Fixup for NullDanglingVmadRefsFixup {
    fn name(&self) -> &'static str {
        "null_dangling_vmad_refs"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        // Pre-copy pass: defer nulling when placed children / interiors are
        // emitted post-copy (paired with `repair_dangling_vmad_refs`).
        run_vmad_resolution(session, mapper, config, config.defer_placed_child_ref_class)
    }
}

/// Post-copy authoritative VMAD resolve. Pairs with the pre-copy deferral: when
/// `defer_placed_child_ref_class` is set the pre-copy sweep LEFT VMAD Object refs
/// that resolved nowhere intact (interior CELL/REFR + placed children are emitted
/// post-copy). This runs over the now-complete output — repairing the emitted
/// targets to the output master byte and nulling only the genuine residue.
/// Called from `ConversionRun::repair_placed_child_refs`.
pub fn repair_dangling_vmad_refs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    run_vmad_resolution(session, mapper, config, false)
}

/// Shared body for both the pre-copy sweep and the post-copy repair. `defer_null`
/// controls whether slots that resolve nowhere are left intact (pre-copy defer)
/// or nulled (post-copy / non-deferred pipelines).
fn run_vmad_resolution(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    defer_null: bool,
) -> Result<FixupReport, FixupError> {
    {
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let resolver = VmadResolver::build(session, config)?.with_defer_null(defer_null);
        if resolver.output_objids.is_empty() {
            return Ok(report);
        }

        let available: FxHashSet<crate::ids::SigCode> = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .into_iter()
            .collect();

        let vmad_only = ["VMAD"];
        let mut changed_records = Vec::new();
        for sig_str in TOUCHED_RECORD_SIGS {
            let Ok(sig) = crate::ids::SigCode::from_str(sig_str) else {
                continue;
            };
            if !available.contains(&sig) {
                continue;
            }
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                if !session
                    .record_has_any_subrecord(&fk, &vmad_only)
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if null_dangling_in_record(&mut record, &resolver) {
                    changed_records.push(record);
                }
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "null_dangling_vmad_refs replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

/// Resolves an encoded `[master_index << 24 | object_id]` FormID (as it sits in
/// a VMAD object slot) against the output plugin and target masters.
pub(crate) struct VmadResolver {
    pub(crate) output_objids: FxHashSet<u32>,
    /// master_index → object-id set, parallel to the target master load order.
    /// `Arc` so the store2 master-scan cache can share them across sweeps.
    master_objids: Vec<Arc<FxHashSet<u32>>>,
    /// Number of target masters; the output plugin's own master byte is this.
    output_master_index: u32,
    /// When set, a slot that resolves NOWHERE is LEFT intact instead of nulled
    /// (still repaired if it resolves). Used by the pre-copy pass on whole-plugin
    /// FO76→FO4 runs where interior CELL/REFR + placed children are emitted
    /// post-copy: nulling here would clobber a VMAD Object property (e.g. a
    /// shelter door's `ShelterCell` / `ShelterCellTeleportPosition`) whose target
    /// is present only after the copy. The post-copy pass runs with this false to
    /// perform the authoritative repair/null over the now-complete output.
    defer_null: bool,
}

impl VmadResolver {
    pub(crate) fn build(
        session: &mut PluginSession,
        config: &FixupConfig,
    ) -> Result<Self, FixupError> {
        let mut master_objids = Vec::with_capacity(config.target_master_handle_ids.len());
        for &handle_id in &config.target_master_handle_ids {
            let set = session
                .local_object_ids_in_handle(handle_id)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            master_objids.push(Arc::new(set));
        }
        Self::build_with_master_objids(session, master_objids)
    }

    /// `build` with the master scan supplied by the caller (the store2
    /// master-scan cache); everything output-derived is gathered fresh.
    pub(crate) fn build_with_master_objids(
        session: &mut PluginSession,
        master_objids: Vec<Arc<FxHashSet<u32>>>,
    ) -> Result<Self, FixupError> {
        let target_id = session.target_id();
        let output_objids = session
            .local_object_ids_in_handle(target_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let output_master_index = master_objids.len() as u32;
        Ok(Self {
            output_objids,
            master_objids,
            output_master_index,
            defer_null: false,
        })
    }

    /// Enable/disable defer mode (see `defer_null`). Consuming builder so call
    /// sites read `VmadResolver::build(..)?.with_defer_null(defer)`.
    pub(crate) fn with_defer_null(mut self, defer_null: bool) -> Self {
        self.defer_null = defer_null;
        self
    }

    /// Replacement for a slot that resolves nowhere: `None` (leave intact) under
    /// defer, else `Some(0)` (null).
    fn null_replacement(&self) -> Option<u32> {
        if self.defer_null { None } else { Some(0) }
    }

    /// Returns a replacement encoded FormID when a VMAD slot needs repair/null.
    /// A null FormID, a FormID resolving in its addressed handle, or one whose
    /// master index is beyond the known masters (can't prove it dangles) is kept.
    fn replacement_for(&self, raw: u32) -> Option<u32> {
        if raw == 0 {
            return None;
        }
        let master_index = raw >> 24;
        let object_id = raw & 0x00FF_FFFF;
        if master_index == self.output_master_index {
            return if self.output_objids.contains(&object_id) {
                None
            } else {
                self.null_replacement()
            };
        }
        match self.master_objids.get(master_index as usize) {
            Some(set) if set.contains(&object_id) => None,
            Some(_) => {
                if self.output_objids.contains(&object_id) {
                    Some((self.output_master_index << 24) | object_id)
                } else {
                    self.null_replacement()
                }
            }
            // Master index beyond the known target masters — can't prove it
            // dangles; leave it.
            None => None,
        }
    }
}

pub(crate) fn null_dangling_in_record(record: &mut Record, resolver: &VmadResolver) -> bool {
    let rec_sig = record.sig.0;
    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if entry.sig.as_str() != "VMAD" {
            continue;
        }
        if let FieldValue::Bytes(bytes) = &mut entry.value {
            if null_dangling_in_vmad_blob(bytes.as_mut_slice(), resolver, &rec_sig) {
                changed = true;
            }
        }
    }
    changed
}

/// Walk the VMAD blob, repairing or nulling each dangling object FormID. Mirrors
/// `FormKeyMapper::rewrite_vmad_formids`. Returns whether any slot changed.
/// Aborts (no further change) on a malformed blob.
fn null_dangling_in_vmad_blob(
    data: &mut [u8],
    resolver: &VmadResolver,
    record_sig: &[u8; 4],
) -> bool {
    let Some(version) = read_u16(data, 0) else {
        return false;
    };
    let Some(object_format) = read_u16(data, 2) else {
        return false;
    };
    let Some(script_count) = read_u16(data, 4) else {
        return false;
    };
    if version == 0 || !matches!(object_format, 1 | 2) {
        return false;
    }
    let mut offset = 6usize;
    let mut changed = false;
    for _ in 0..script_count {
        if walk_script_entry(data, &mut offset, object_format, resolver, &mut changed).is_none() {
            return changed;
        }
    }
    if offset < data.len() {
        match record_sig {
            b"INFO" | b"PACK" | b"SCEN" => {
                null_dangling_info_pack_scen_after_scripts(
                    data,
                    &mut offset,
                    object_format,
                    resolver,
                    &mut changed,
                );
            }
            b"PERK" | b"TERM" => {
                null_dangling_perk_term_after_scripts(
                    data,
                    &mut offset,
                    object_format,
                    resolver,
                    &mut changed,
                );
            }
            b"QUST" => {
                null_dangling_qust_after_scripts(
                    data,
                    &mut offset,
                    object_format,
                    resolver,
                    &mut changed,
                );
            }
            _ => {}
        }
    }
    changed
}

fn null_dangling_info_pack_scen_after_scripts(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    advance(offset, 1, data.len())?; // i8 version
    advance(offset, 1, data.len())?; // u8 flags
    walk_script_entry(data, offset, object_format, resolver, changed)
}

fn null_dangling_perk_term_after_scripts(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    advance(offset, 1, data.len())?; // i8 version
    walk_script_entry(data, offset, object_format, resolver, changed)
}

/// Mirrors `FormKeyMapper::rewrite_vmad_qust_after_scripts` — skips fragment
/// headers/entries and walks the alias section, nulling any dangling alias
/// object FormIDs and alias-script object properties.
fn null_dangling_qust_after_scripts(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    advance(offset, 1, data.len())?; // i8 version
    let fragment_count = read_u16_advance(data, offset)? as usize;

    // Walk fragment script header (same as in formkey_mapper.rs).
    let script_name_len = read_u16_advance(data, offset)? as usize;
    if script_name_len > 0 {
        advance(offset, script_name_len, data.len())?;
        advance(offset, 1, data.len())?; // u8 flags
        let prop_count = read_u16_advance(data, offset)? as usize;
        for _ in 0..prop_count {
            skip_vmad_string(data, offset)?;
            let prop_type = read_u8_advance(data, offset)?;
            advance(offset, 1, data.len())?;
            walk_property_value(data, offset, prop_type, object_format, resolver, changed)?;
        }
    }

    // Skip fragment entries (strings + fixed-size fields, no FormIDs).
    for _ in 0..fragment_count {
        advance(offset, 2, data.len())?; // u16 stage
        advance(offset, 2, data.len())?; // i16 unknown
        advance(offset, 4, data.len())?; // i32 stage_index
        advance(offset, 1, data.len())?; // i8 unknown
        skip_vmad_string(data, offset)?; // script name
        skip_vmad_string(data, offset)?; // fragment name
    }

    // Walk alias entries.
    let alias_count = read_u16_advance(data, offset)? as usize;
    for _ in 0..alias_count {
        walk_object(data, offset, object_format, resolver, changed)?;
        advance(offset, 2, data.len())?; // i16 version
        let alias_obj_format = read_u16_advance(data, offset)?;
        let alias_script_count = read_u16_advance(data, offset)? as usize;
        for _ in 0..alias_script_count {
            walk_script_entry(data, offset, alias_obj_format, resolver, changed)?;
        }
    }
    Some(())
}

fn walk_script_entry(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    skip_vmad_string(data, offset)?;
    advance(offset, 1, data.len())?;
    let property_count = read_u16_advance(data, offset)? as usize;
    for _ in 0..property_count {
        walk_property_entry(data, offset, object_format, resolver, changed)?;
    }
    Some(())
}

fn walk_property_entry(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    skip_vmad_string(data, offset)?;
    let property_type = read_u8_advance(data, offset)?;
    advance(offset, 1, data.len())?;
    walk_property_value(
        data,
        offset,
        property_type,
        object_format,
        resolver,
        changed,
    )
}

fn walk_property_value(
    data: &mut [u8],
    offset: &mut usize,
    property_type: u8,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => walk_object(data, offset, object_format, resolver, changed),
        2 => {
            skip_vmad_string(data, offset)?;
            Some(())
        }
        3 | 4 => advance(offset, 4, data.len()),
        5 => advance(offset, 1, data.len()),
        7 => walk_struct(data, offset, object_format, resolver, changed),
        11 => {
            let count = read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                walk_object(data, offset, object_format, resolver, changed)?;
            }
            Some(())
        }
        12 => {
            let count = read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                skip_vmad_string(data, offset)?;
            }
            Some(())
        }
        13 | 14 => {
            let count = read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            advance(offset, (count as usize).checked_mul(4)?, data.len())
        }
        15 => {
            let count = read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            advance(offset, count as usize, data.len())
        }
        16 => advance(offset, 4, data.len()),
        17 => {
            let count = read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                walk_struct(data, offset, object_format, resolver, changed)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn walk_struct(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    let count = read_i32_advance(data, offset)?;
    if count < 0 {
        return None;
    }
    for _ in 0..count {
        skip_vmad_string(data, offset)?;
        let member_type = read_u8_advance(data, offset)?;
        advance(offset, 1, data.len())?;
        walk_property_value(data, offset, member_type, object_format, resolver, changed)?;
    }
    Some(())
}

fn walk_object(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    let formid_offset = if object_format == 2 {
        let formid_offset = (*offset).checked_add(4)?;
        advance(offset, 8, data.len())?;
        formid_offset
    } else {
        let formid_offset = *offset;
        advance(offset, 8, data.len())?;
        formid_offset
    };
    let raw = read_u32(data, formid_offset)?;
    if let Some(replacement) = resolver.replacement_for(raw) {
        data.get_mut(formid_offset..formid_offset.checked_add(4)?)?
            .copy_from_slice(&replacement.to_le_bytes());
        *changed = true;
    }
    Some(())
}

fn read_u8(data: &[u8], offset: usize) -> Option<u8> {
    data.get(offset).copied()
}

fn read_u8_advance(data: &[u8], offset: &mut usize) -> Option<u8> {
    let value = read_u8(data, *offset)?;
    *offset = (*offset).checked_add(1)?;
    Some(value)
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u16_advance(data: &[u8], offset: &mut usize) -> Option<u16> {
    let value = read_u16(data, *offset)?;
    *offset = (*offset).checked_add(2)?;
    Some(value)
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_i32_advance(data: &[u8], offset: &mut usize) -> Option<i32> {
    let value = read_u32(data, *offset)? as i32;
    *offset = (*offset).checked_add(4)?;
    Some(value)
}

fn advance(offset: &mut usize, by: usize, len: usize) -> Option<()> {
    let next = offset.checked_add(by)?;
    if next > len {
        return None;
    }
    *offset = next;
    Some(())
}

fn skip_vmad_string(data: &[u8], offset: &mut usize) -> Option<()> {
    let len = read_u16_advance(data, offset)? as usize;
    advance(offset, len, data.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(output: &[u32], masters: &[&[u32]]) -> VmadResolver {
        VmadResolver {
            output_objids: output.iter().copied().collect(),
            master_objids: masters
                .iter()
                .map(|ids| Arc::new(ids.iter().copied().collect()))
                .collect(),
            output_master_index: masters.len() as u32,
            defer_null: false,
        }
    }

    fn push_string(out: &mut Vec<u8>, s: &str) {
        out.extend_from_slice(&(s.len() as u16).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }

    fn push_objfmt2_object(out: &mut Vec<u8>, alias: i16, raw: u32) -> usize {
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&alias.to_le_bytes());
        let offset = out.len();
        out.extend_from_slice(&raw.to_le_bytes());
        offset
    }

    /// Build a VMAD blob (objfmt 2) with a single script and `props` properties,
    /// each a type-1 Object carrying the given encoded FormID. Returns the blob
    /// and the byte offset of each FormID for assertions.
    fn vmad_objfmt2(props: &[u32]) -> (Vec<u8>, Vec<usize>) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes()); // version
        out.extend_from_slice(&2u16.to_le_bytes()); // object format
        out.extend_from_slice(&1u16.to_le_bytes()); // script count
        push_string(&mut out, "Script");
        out.push(0); // status
        out.extend_from_slice(&(props.len() as u16).to_le_bytes()); // property count
        let mut offsets = Vec::new();
        for (i, raw) in props.iter().enumerate() {
            push_string(&mut out, &format!("P{i}"));
            out.push(1); // type = Object
            out.push(0); // status
            // objfmt 2 object: [u16][i16 alias][u32 formid]
            offsets.push(push_objfmt2_object(&mut out, 0, *raw));
        }
        (out, offsets)
    }

    fn push_object_script_entry(out: &mut Vec<u8>, raw: u32) -> usize {
        push_string(out, "Script");
        out.push(0); // status
        out.extend_from_slice(&1u16.to_le_bytes()); // property count
        push_string(out, "P0");
        out.push(1); // type = Object
        out.push(0); // status
        push_objfmt2_object(out, 0, raw)
    }

    fn vmad_fragment_objfmt2(sig: &[u8; 4], raw: u32) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        match sig {
            b"INFO" | b"PACK" | b"SCEN" => {
                out.push(4); // fragment version
                out.push(0); // flags: no fragment rows
                let offset = push_object_script_entry(&mut out, raw);
                if sig == b"SCEN" {
                    out.extend_from_slice(&0u16.to_le_bytes()); // phase fragment count
                }
                (out, offset)
            }
            b"PERK" | b"TERM" => {
                out.push(4); // fragment version
                let offset = push_object_script_entry(&mut out, raw);
                out.extend_from_slice(&0u16.to_le_bytes()); // fragment count
                (out, offset)
            }
            other => panic!("unsupported fragment VMAD sig: {:?}", other),
        }
    }

    fn raw_at(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }

    fn alias_at(b: &[u8], formid_offset: usize) -> i16 {
        i16::from_le_bytes([b[formid_offset - 2], b[formid_offset - 1]])
    }

    #[test]
    fn keeps_master_resolving_object_formid() {
        // 0x0002058E (output master byte 0 = Fallout4.esm) resolves in FO4 → keep.
        let r = resolver(&[0x111111], &[&[0x02058E]]);
        let (mut b, offs) = vmad_objfmt2(&[0x0002058E]);
        assert!(!null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0x0002058E);
    }

    #[test]
    fn nulls_dangling_fallout4_prefixed_formid() {
        // 0x008A5475 addresses Fallout4.esm (byte 0) but isn't an FO4 record and
        // its source CELL wasn't emitted → null.
        let r = resolver(&[0x111111], &[&[0x000010]]);
        let (mut b, offs) = vmad_objfmt2(&[0x008A5475]);
        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0);
    }

    /// 7 empty masters → the output plugin's own master byte is 0x07, matching
    /// the real FO76→FO4 output (Fallout4 + 6 DLCs).
    fn seven_masters() -> Vec<&'static [u32]> {
        let empty: &'static [u32] = &[];
        vec![empty; 7]
    }

    #[test]
    fn keeps_emitted_output_own_formid() {
        // 0x078AEDDB (output master byte 7) is an emitted own-record → keep.
        let r = resolver(&[0x8AEDDB], &seven_masters());
        let (mut b, offs) = vmad_objfmt2(&[0x078AEDDB]);
        assert!(!null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0x078AEDDB);
    }

    #[test]
    fn nulls_unemitted_output_own_formid() {
        // 0x078A5475 (output prefix) but not actually emitted → null.
        let r = resolver(&[0x111111], &seven_masters());
        let (mut b, offs) = vmad_objfmt2(&[0x078A5475]);
        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0);
    }

    #[test]
    fn post_copy_nulls_generator_sibling_and_is_idempotent() {
        let r = resolver(&[0x5EE7A8], &seven_masters());
        let (mut b, offs) = vmad_objfmt2(&[0x075E_5893]);

        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"REFR"));
        assert_eq!(raw_at(&b, offs[0]), 0);
        assert!(!null_dangling_in_vmad_blob(&mut b, &r, b"REFR"));
    }

    #[test]
    fn repairs_refr_scalar_object_formids_and_nulls_unemitted_ones() {
        let fallout4: &[u32] = &[0x042241];
        let mut masters = seven_masters();
        masters[0] = fallout4;
        let r = resolver(&[0x5A77D3, 0x5CA05C], &masters);
        let (mut b, offs) = vmad_objfmt2(&[0x005A77D3, 0x0053004C, 0x00042241]);

        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"REFR"));
        assert_eq!(raw_at(&b, offs[0]), 0x075A77D3);
        assert_eq!(raw_at(&b, offs[1]), 0);
        assert_eq!(raw_at(&b, offs[2]), 0x00042241);
    }

    #[test]
    fn repairs_refr_array_object_formids_without_touching_aliases() {
        let r = resolver(&[0x5D7E17, 0x5D7E18], &seven_masters());
        let mut b = Vec::new();
        b.extend_from_slice(&6u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        push_string(&mut b, "GenericEWSModuleRef");
        b.push(0);
        b.extend_from_slice(&1u16.to_le_bytes());

        push_string(&mut b, "TurfSleepPointsKeywords");
        b.push(11);
        b.push(1);
        b.extend_from_slice(&3i32.to_le_bytes());
        let repaired_offset = push_objfmt2_object(&mut b, -1, 0x005D7E17);
        let alias_repaired_offset = push_objfmt2_object(&mut b, 3, 0x005D7E18);
        let nulled_offset = push_objfmt2_object(&mut b, -1, 0x0053004A);

        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"REFR"));
        assert_eq!(raw_at(&b, repaired_offset), 0x075D7E17);
        assert_eq!(raw_at(&b, alias_repaired_offset), 0x075D7E18);
        assert_eq!(raw_at(&b, nulled_offset), 0);
        assert_eq!(alias_at(&b, repaired_offset), -1);
        assert_eq!(alias_at(&b, alias_repaired_offset), 3);
        assert_eq!(alias_at(&b, nulled_offset), -1);
    }

    #[test]
    fn keeps_null_object_formid() {
        let r = resolver(&[], &[&[]]);
        let (mut b, offs) = vmad_objfmt2(&[0]);
        assert!(!null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0);
    }

    #[test]
    fn mixed_props_null_only_dangling() {
        // [legit FO4, dangling, emitted-07]: only the middle nulls.
        let r = resolver(&[0x8AEDDB], &[&[0x02058E]]);
        let (mut b, offs) = vmad_objfmt2(&[0x0002058E, 0x008A5475, 0x078AEDDB]);
        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0x0002058E);
        assert_eq!(raw_at(&b, offs[1]), 0);
        assert_eq!(raw_at(&b, offs[2]), 0x078AEDDB);
    }

    #[test]
    fn keeps_object_in_unknown_higher_master_index() {
        // master index beyond the known masters — cannot prove it dangles → keep.
        let r = resolver(&[], &[&[0x000010]]);
        let (mut b, offs) = vmad_objfmt2(&[0x0A123456]);
        assert!(!null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0x0A123456);
    }

    #[test]
    fn malformed_blob_is_a_noop() {
        let r = resolver(&[], &[&[]]);
        let mut b = vec![0u8; 5]; // shorter than the 6-byte header
        assert!(!null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
    }

    #[test]
    fn info_is_a_touched_record_sig() {
        // Regression guard: INFO must stay in the allow-list, or its VMAD
        // script-property danglers are never walked.
        assert!(
            TOUCHED_RECORD_SIGS.contains(&"INFO"),
            "INFO must be walked for VMAD danglers (Class S, target 003BA973)"
        );
    }

    #[test]
    fn observed_vmad_dangling_hosts_are_touched_record_sigs() {
        for sig in ["BOOK", "FURN", "MISC", "MSTT", "NPC_", "REFR", "SCEN"] {
            assert!(
                TOUCHED_RECORD_SIGS.contains(&sig),
                "{sig} must be walked for VMAD object danglers"
            );
        }
    }

    #[test]
    fn nulls_info_akref1_dangling_object_formid() {
        // The exact Class S case: INFO script-property Object FormID 0x003BA973
        // (akRef1 in fragment script AddPlayersToSameInstance) addresses
        // Fallout4.esm (byte 0) but is not an FO4 record and its source REFR was
        // never emitted → null. Standard Scripts-section object (objfmt 2).
        let r = resolver(&[0x111111], &seven_masters());
        let (mut b, offs) = vmad_objfmt2(&[0x003BA973]);
        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0);
    }

    #[test]
    fn nulls_non_quest_fragment_script_object_formids() {
        let r = resolver(&[0x111111], &seven_masters());
        for sig in [b"INFO", b"PACK", b"SCEN", b"TERM"] {
            let (mut b, offset) = vmad_fragment_objfmt2(sig, 0x0032_E192);
            assert!(
                null_dangling_in_vmad_blob(&mut b, &r, sig),
                "{} fragment object should be nulled",
                std::str::from_utf8(sig).unwrap()
            );
            assert_eq!(raw_at(&b, offset), 0);
        }
    }

    #[test]
    fn defer_null_leaves_unresolved_ref_intact() {
        // Pre-copy defer: a ref whose target isn't emitted YET (interior CELL, to
        // be copied post-copy) is LEFT untouched instead of nulled.
        let r = resolver(&[0x111111], &seven_masters()).with_defer_null(true);
        let (mut b, offs) = vmad_objfmt2(&[0x007AD56F]);
        assert!(!null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0x007AD56F);
    }

    #[test]
    fn defer_null_still_repairs_already_emitted_ref() {
        // Defer mode does NOT block repair — a target already in the output is
        // still rewritten to the output master byte.
        let r = resolver(&[0x7AD56F], &seven_masters()).with_defer_null(true);
        let (mut b, offs) = vmad_objfmt2(&[0x007AD56F]);
        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0x077AD56F);
    }

    #[test]
    fn post_copy_repairs_shelter_cell_ref_after_interior_emit() {
        // The exact reported bug: source ShelterCell 0x007AD56F. Pre-copy (defer)
        // leaves it intact because the interior CELL isn't emitted yet; post-copy
        // (defer_null=false) with the CELL now in the output → repaired to 07.
        let (mut b, offs) = vmad_objfmt2(&[0x007AD56F]);

        let pre = resolver(&[0x111111], &seven_masters()).with_defer_null(true);
        assert!(!null_dangling_in_vmad_blob(&mut b, &pre, b"ACTI"));
        assert_eq!(raw_at(&b, offs[0]), 0x007AD56F, "pre-copy leaves it intact");

        let post = resolver(&[0x7AD56F], &seven_masters());
        assert!(null_dangling_in_vmad_blob(&mut b, &post, b"ACTI"));
        assert_eq!(
            raw_at(&b, offs[0]),
            0x077AD56F,
            "post-copy repairs to output"
        );
    }

    #[test]
    fn post_copy_nulls_genuine_dangler_after_defer() {
        // A ref that resolves nowhere even post-copy (target never emitted) is
        // still nulled by the authoritative post-copy pass.
        let (mut b, offs) = vmad_objfmt2(&[0x003BA973]);

        let pre = resolver(&[0x111111], &seven_masters()).with_defer_null(true);
        assert!(!null_dangling_in_vmad_blob(&mut b, &pre, b"INFO"));
        assert_eq!(raw_at(&b, offs[0]), 0x003BA973);

        let post = resolver(&[0x111111], &seven_masters());
        assert!(null_dangling_in_vmad_blob(&mut b, &post, b"INFO"));
        assert_eq!(raw_at(&b, offs[0]), 0);
    }

    #[test]
    fn nulls_array_object_and_array_struct_object_formids() {
        let mut b = Vec::new();
        b.extend_from_slice(&5u16.to_le_bytes());
        b.extend_from_slice(&2u16.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        push_string(&mut b, "Script");
        b.push(0);
        b.extend_from_slice(&2u16.to_le_bytes());

        push_string(&mut b, "ObjectArray");
        b.push(11);
        b.push(0);
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        let array_object_offset = b.len();
        b.extend_from_slice(&0x0085_2997u32.to_le_bytes());

        push_string(&mut b, "StructArray");
        b.push(17);
        b.push(0);
        b.extend_from_slice(&1i32.to_le_bytes());
        b.extend_from_slice(&1i32.to_le_bytes());
        push_string(&mut b, "MapMarker");
        b.push(1);
        b.push(0);
        b.extend_from_slice(&0u16.to_le_bytes());
        b.extend_from_slice(&0u16.to_le_bytes());
        let array_struct_offset = b.len();
        b.extend_from_slice(&0x0055_1EE0u32.to_le_bytes());

        let r = resolver(&[0x111111], &seven_masters());
        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"MSTT"));
        assert_eq!(raw_at(&b, array_object_offset), 0);
        assert_eq!(raw_at(&b, array_struct_offset), 0);
    }
}
