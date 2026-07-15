//! Fixup: repair SCEN `HTID` "Play Sound" references whose
//! master byte was never remapped from the FO76 source space to the output
//! plugin.
//!
//! # Root cause
//! FO76 SCEN radio scenes (`RadioG_*`, `MUSRadio76General*`) carry an action
//! `HTID` "Play Sound" FormID addressing an FO76 SNDR in the source's own master
//! space, e.g. `003EB652` = `MUSRadio76General...BingCrosby...`. That SNDR IS
//! converted into the output plugin (it exists at `073EB652`), but the action-
//! scope `HTID` FormID kept its FO76 `00` prefix instead of being remapped to the
//! output plugin index. Under the FO4 master order `003EB652` then addresses a
//! non-existent Fallout4.esm record, so xEdit reports
//! `SCEN \ ... \ HTID - Play Sound -> [003EB652] <Error: Could not be resolved>`
//! The SCEN re-emission
//! (`fnv_legacy_scripting::scene`) routes the payload through the mapper's
//! `rewrite_payload_formkeys`, but these action-scope sound FKs escape it (the
//! mapper has no source→output entry for an id-preserved converted SNDR), exactly
//! the master-byte-truncation gap the REFR `XTNM` / SNDR `BNAM` repairs already
//! cover for other slots.
//!
//! # Fix (repair-to-output / null wrong-type, collide-safe, raw-byte level)
//! For each SCEN `HTID` subrecord, repair the master byte to the output plugin
//! **only** when the raw FormID addresses a master, does NOT resolve in that
//! master, and its object-id exists in the output plugin **as an SNDR**. The
//! SNDR-type gate is what distinguishes the "Play Sound" `HTID` (formid → SNDR)
//! from the "Player Headtracking" `HTID` (an int actor-id array that shares the
//! signature): a headtracking actor-id never names an output SNDR. Working on the
//! RAW subrecord bytes (not the decoded value) sidesteps the decode-time
//! disambiguation between the two same-scope `HTID` specs — both accept a 4-byte
//! payload, so an occurrence-ordered decode cannot be trusted to type them. A FK
//! that already resolves to an SNDR (valid Fallout4.esm SNDR, or already `07`),
//! is null, or names no output SNDR is left byte-identical. A `07` HTID that
//! resolves to a non-SNDR output record is safely nulled because FO4 accepts
//! NULL in this slot and rejects the wrong type.

use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;
use crate::sym::StringInterner;

const SNDR_SIG: &str = "SNDR";

pub struct RepairScenHtidSoundRefsFixup;

impl Fixup for RepairScenHtidSoundRefsFixup {
    fn name(&self) -> &'static str {
        "repair_scen_htid_sound_refs"
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
        let mut report = FixupReport::empty();

        let scen_sig =
            SigCode::from_str("SCEN").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let present = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if !present.iter().any(|s| s.as_str() == "SCEN") {
            return Ok(report);
        }

        let resolver = HtidResolver::build(session, config, mapper.interner)?;
        // Need at least one output SNDR to repair to.
        if resolver.output_sndr_objids.is_empty() {
            return Ok(report);
        }

        let pre_filter = ["HTID"];
        let fks = session
            .form_keys_of_sig(scen_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in fks {
            // SCEN records are large; only touch the ones carrying an HTID.
            if !session
                .record_has_any_subrecord(&fk, &pre_filter)
                .unwrap_or(false)
            {
                continue;
            }
            let repaired = session
                .patch_all_subrecords_bytes(&fk, "HTID", |buf| resolver.repair_htid_bytes(buf))
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if repaired > 0 {
                report.records_changed += 1;
                report.records_dropped += repaired;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// Resolver
// ---------------------------------------------------------------------------

/// Resolves a raw `HTID` FormID (`(master_index << 24) | object_id`) against the
/// output SNDR set and the target masters, deciding whether to repair its master
/// byte to the output plugin index.
pub struct HtidResolver {
    /// Object-ids of every SNDR record in the output plugin (the repair gate).
    pub(crate) output_sndr_objids: FxHashSet<u32>,
    /// Object-ids of every record in the output plugin (wrong-type detection).
    output_objids: FxHashSet<u32>,
    /// Per target-master object-id sets, indexed by master load order — used to
    /// check whether a master-addressing FK already resolves (then leave it).
    /// `Arc` so the store2 master-scan cache can share them across sweeps.
    master_objids: Vec<Arc<FxHashSet<u32>>>,
    /// Output plugin's own master index = number of target masters; what a
    /// repaired FormID's high byte is set to.
    output_master_index: u32,
}

impl HtidResolver {
    pub(crate) fn build(
        session: &mut PluginSession,
        config: &FixupConfig,
        interner: &StringInterner,
    ) -> Result<Self, FixupError> {
        let mut master_objids = Vec::with_capacity(config.target_master_handle_ids.len());
        for &handle_id in &config.target_master_handle_ids {
            let set = session
                .local_object_ids_in_handle(handle_id)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            master_objids.push(Arc::new(set));
        }
        Self::build_with_master_objids(session, interner, master_objids)
    }

    /// `build` with the master scan supplied by the caller (the store2
    /// master-scan cache); everything output-derived is gathered fresh.
    pub(crate) fn build_with_master_objids(
        session: &mut PluginSession,
        interner: &StringInterner,
        master_objids: Vec<Arc<FxHashSet<u32>>>,
    ) -> Result<Self, FixupError> {
        let output_master_index = session.target_masters().len() as u32;
        let output_objids = session
            .local_object_ids_in_handle(session.target_id())
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let sndr_sig =
            SigCode::from_str(SNDR_SIG).map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let mut output_sndr_objids = FxHashSet::default();
        for fk in session
            .form_keys_of_sig(sndr_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            output_sndr_objids.insert(fk.local & 0x00FF_FFFF);
        }

        Ok(Self {
            output_sndr_objids,
            output_objids,
            master_objids,
            output_master_index,
        })
    }

    /// Whether `(master_index, object_id)` names a record in the addressed handle
    /// (output plugin or a loaded target master).
    fn object_exists(&self, master_index: u32, object_id: u32) -> bool {
        if master_index == self.output_master_index {
            self.output_sndr_objids.contains(&object_id)
        } else {
            self.master_objids
                .get(master_index as usize)
                .is_some_and(|set| set.contains(&object_id))
        }
    }

    /// Repair decision for a raw `HTID` FormID. Returns the repaired raw value
    /// (high byte = output master index) when the FK addresses a master, does NOT
    /// resolve there, and its object-id names an output SNDR; otherwise `None`.
    fn repair_raw(&self, raw: u32) -> Option<u32> {
        if raw & 0x00FF_FFFF == 0 {
            return None; // null object-id — leave (also covers raw == 0)
        }
        let master_index = raw >> 24;
        let object_id = raw & 0x00FF_FFFF;
        if master_index == self.output_master_index
            && self.output_objids.contains(&object_id)
            && !self.output_sndr_objids.contains(&object_id)
        {
            return Some(0);
        }
        // Already addresses the output plugin → nothing to repair.
        if master_index == self.output_master_index {
            return None;
        }
        // Resolves in its addressed master → legitimately-inherited vanilla sound.
        if self.object_exists(master_index, object_id) {
            return None;
        }
        // Master-addressing, unresolved there, but the converted SNDR exists in
        // the output → repair the high byte to the output index. Object-ids are
        // unique per plugin, so 00:X→07:X restores the same converted SNDR.
        if self.output_sndr_objids.contains(&object_id) {
            return Some((self.output_master_index << 24) | object_id);
        }
        None
    }

    /// Patch one raw `HTID` subrecord buffer in place. Returns whether it changed.
    /// A non-4-byte buffer (the Player-Headtracking int-array variant with ≠1
    /// element) is left untouched; a 4-byte buffer is read as a little-endian
    /// FormID and repaired only when `repair_raw` fires (the SNDR gate makes a
    /// 4-byte headtracking actor-id a no-op too).
    pub(crate) fn repair_htid_bytes(&self, buf: &mut Vec<u8>) -> bool {
        if buf.len() != 4 {
            return false;
        }
        let raw = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        match self.repair_raw(raw) {
            Some(new_raw) => {
                buf.copy_from_slice(&new_raw.to_le_bytes());
                true
            }
            None => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a resolver from explicit object-id sets. `master_objids[0]` is the
    /// first target master (master index 0); `output_master_index` follows.
    fn resolver(
        sndr_objids: &[u32],
        master0_objids: &[u32],
        output_master_index: u32,
    ) -> HtidResolver {
        HtidResolver {
            output_sndr_objids: sndr_objids.iter().copied().collect(),
            output_objids: sndr_objids.iter().copied().collect(),
            master_objids: vec![Arc::new(master0_objids.iter().copied().collect())],
            output_master_index,
        }
    }

    fn le(raw: u32) -> Vec<u8> {
        raw.to_le_bytes().to_vec()
    }

    #[test]
    fn repairs_master_byte_truncated_play_sound_to_output_sndr() {
        // 003EB652 addresses master 0 (Fallout4.esm) but is absent there; the
        // converted SNDR exists in the output (master index 1) → repair to
        // 013EB652.
        let r = resolver(&[0x3EB652], &[], 1);
        let mut buf = le(0x003EB652);
        assert!(r.repair_htid_bytes(&mut buf));
        assert_eq!(buf, le(0x013EB652));
    }

    #[test]
    fn keeps_play_sound_that_resolves_in_master() {
        // A real Fallout4.esm SNDR (object-id present in master 0) is vanilla-
        // inherited — keep it byte-identical.
        let r = resolver(&[0x22B6D7], &[0x22B6D7], 1);
        let mut buf = le(0x0022B6D7);
        assert!(!r.repair_htid_bytes(&mut buf));
        assert_eq!(buf, le(0x0022B6D7));
    }

    #[test]
    fn leaves_headtracking_actor_id_untouched() {
        // A small actor-id whose object-id names NO output SNDR is never touched
        // (this is what keeps a 4-byte Player-Headtracking HTID safe).
        let r = resolver(&[0x3EB652], &[], 1);
        let mut buf = le(0x00000003);
        assert!(!r.repair_htid_bytes(&mut buf));
        assert_eq!(buf, le(0x00000003));
    }

    #[test]
    fn leaves_null_and_already_output_play_sound() {
        let r = resolver(&[0x3EB652], &[], 1);
        // null object-id
        let mut z = le(0x00000000);
        assert!(!r.repair_htid_bytes(&mut z));
        // already addresses the output plugin (master index 1)
        let mut out = le(0x013EB652);
        assert!(!r.repair_htid_bytes(&mut out));
        assert_eq!(out, le(0x013EB652));
    }

    #[test]
    fn skips_non_four_byte_htid_array_variant() {
        // The Player-Headtracking HTID with >1 actor is an N*4-byte int array —
        // not a single FormID; the byte-length guard skips it.
        let r = resolver(&[0x3EB652], &[], 1);
        let mut multi = vec![3u8, 0, 0, 0, 4, 0, 0, 0]; // two actor ids
        assert!(!r.repair_htid_bytes(&mut multi));
        assert_eq!(multi, vec![3u8, 0, 0, 0, 4, 0, 0, 0]);
    }

    #[test]
    fn repair_raw_high_byte_targets_output_index_not_hardcoded_07() {
        // The repaired high byte must equal output_master_index, not a hardcoded
        // 7 — a plugin with a different master count repairs to its own index.
        let r = resolver(&[0xABCDEF], &[], 3);
        assert_eq!(r.repair_raw(0x00ABCDEF), Some(0x03ABCDEF));
    }

    #[test]
    fn nulls_already_output_wrong_type_htid() {
        let mut r = resolver(&[0x0900], &[], 1);
        r.output_objids.insert(0x84401F);
        let mut buf = le(0x0184401F);
        assert!(r.repair_htid_bytes(&mut buf));
        assert_eq!(buf, le(0));
    }
}
