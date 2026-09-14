//! Fixup: resolve post-copy placed-reference targets.
//!
//! `XTEL` (Teleport-Destination, `struct:I,f×6,I,I`) holds the destination door at
//! offset 0 and the transition-interior CELL at offset 32. The masterless FO76
//! source uses own master byte `0x00`. The cell-slice copy
//! (`cell_slice::rewrite_placed_child_local_refs`, offsets `[0,32]`) remaps it for
//! exterior children, but interior cells go through `run::emit_interior_cells`, and
//! XTEL's identical FO76/FO4 layout stays raw bytes that
//! `FormKeyMapper::rewrite_record` never touches. `0x00` then resolves to
//! `Fallout4.esm` and the teleport is dead (e.g. `coc WhitespringMall01` exit doors).
//!
//! Runs in the post-copy hook (`ConversionRun::repair_placed_child_refs`) with all
//! placed children present, so no copy path bypasses it. A door (offset 0) naming a
//! target master is rewritten to the own plugin when an own REFR exists at that
//! object-id; the transition CELL (offset 32) likewise against own CELLs. Object-ids
//! are preserved and FO76 is masterless, so the own record wins even when a FO4
//! master has the same id. Doors already pointing at the own plugin are left alone.
//!
//! Optional `XASP`, `XCZR`, and `XOWN` payloads use the same final resolver and
//! are omitted when their targets are still absent. An `XTEL` whose destination
//! door is absent/nonpersistent, or whose non-null transition cell is absent, is
//! omitted as a complete payload.

use rustc_hash::FxHashSet;

use esp_authoring_core::plugin_runtime::WriteEffect;

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::PluginSession;

const XCZR_TARGET_SIGS: &[&str] = &[
    "PLYR", "ACHR", "REFR", "PGRE", "PHZD", "PMIS", "PARW", "PBAR", "PBEA", "PCON", "PFLA",
];
const XOWN_TARGET_SIGS: &[&str] = &["FACT", "NPC_"];

/// `XTEL` byte offset of the destination-door FormID.
const XTEL_DOOR_OFFSET: usize = 0;
/// `XTEL` byte offset of the transition-interior CELL FormID (offset 28 is a u32
/// flags word, not a FormID). Matches `cell_slice`'s `XTEL => &[0, 32]`.
const XTEL_TRANSITION_OFFSET: usize = 32;
const RECORD_FLAG_PERSISTENT: u32 = 0x0000_0400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefResolution {
    Keep,
    Repair(u32),
    Dangling,
}

struct FinalRefResolver {
    output_objids: FxHashSet<u32>,
    master_objids: Vec<FxHashSet<u32>>,
    output_master_index: u32,
}

impl FinalRefResolver {
    fn resolve(&self, raw: u32) -> RefResolution {
        let object_id = raw & 0x00FF_FFFF;
        if object_id == 0 {
            return RefResolution::Dangling;
        }

        let load_index = raw >> 24;
        if load_index < self.output_master_index {
            let Some(master) = self.master_objids.get(load_index as usize) else {
                return RefResolution::Keep;
            };
            if master.contains(&object_id) {
                return RefResolution::Keep;
            }
            if self.output_objids.contains(&object_id) {
                return RefResolution::Repair((self.output_master_index << 24) | object_id);
            }
            return RefResolution::Dangling;
        }

        if load_index == self.output_master_index && self.output_objids.contains(&object_id) {
            RefResolution::Keep
        } else {
            RefResolution::Dangling
        }
    }

    fn resolve_source_own(&self, raw: u32) -> RefResolution {
        let object_id = raw & 0x00FF_FFFF;
        let load_index = raw >> 24;
        if object_id != 0
            && load_index < self.output_master_index
            && self.output_objids.contains(&object_id)
        {
            return RefResolution::Repair((self.output_master_index << 24) | object_id);
        }
        self.resolve(raw)
    }
}

/// Read the little-endian FormID at `buf[offset..offset+4]`, or `None` if short.
fn read_formid(buf: &[u8], offset: usize) -> Option<u32> {
    buf.get(offset..offset + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Stage-local REFR view built after workshop structural writes and discarded
/// before actor specialization can invalidate record identity indexes again.
#[derive(Default)]
pub(crate) struct PlacedRefrDiscovery {
    repair_applicable: bool,
    pub(crate) shared_candidates_available: bool,
    pub(crate) all_refr_fks: Vec<FormKey>,
    pub(crate) teleport_candidates: Vec<FormKey>,
    pub(crate) light_radius_candidates: Vec<FormKey>,
    pub(crate) xezn_candidates: Vec<FormKey>,
    pub(crate) records_seen: usize,
    pub(crate) records_inspected: usize,
}

pub(crate) fn discover_placed_refr_candidates(
    session: &mut PluginSession,
    interner: &crate::sym::StringInterner,
) -> Result<PlacedRefrDiscovery, FixupError> {
    let present = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if !present.iter().any(|s| s.as_str() == "REFR") {
        return Ok(PlacedRefrDiscovery {
            shared_candidates_available: true,
            ..Default::default()
        });
    }
    if session.target_masters().len() > 0xFF {
        return Ok(PlacedRefrDiscovery::default());
    }

    let refr_sig = SigCode::from_str("REFR").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let all_refr_fks = session
        .form_keys_of_sig(refr_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut discovery = PlacedRefrDiscovery {
        repair_applicable: true,
        shared_candidates_available: true,
        records_seen: all_refr_fks.len(),
        all_refr_fks,
        ..Default::default()
    };

    for fk in &discovery.all_refr_fks {
        let Ok(raw_form_id) = session.raw_form_id_for_form_key(fk) else {
            discovery.light_radius_candidates.push(*fk);
            continue;
        };
        let Ok(record) = session.record(raw_form_id) else {
            discovery.light_radius_candidates.push(*fk);
            continue;
        };
        discovery.records_inspected += 1;
        let mut teleport = false;
        let mut light_radius = false;
        let mut xezn = false;
        for subrecord in &record.subrecords {
            match subrecord.signature.as_str() {
                "XTEL" | "XASP" | "XCZR" | "XOWN" => teleport = true,
                "XRDS" => light_radius = true,
                "XEZN" => xezn = true,
                _ => {}
            }
        }
        if teleport {
            discovery.teleport_candidates.push(*fk);
        }
        if light_radius {
            discovery.light_radius_candidates.push(*fk);
        }
        if xezn {
            discovery.xezn_candidates.push(*fk);
        }
    }

    Ok(discovery)
}

/// Repair interior placed-door teleport targets in the finished output plugin.
/// Mirrors `resolve_placed_leveled_bases`: called from the post-copy hook so it
/// sees every placed child (interior + exterior), and operates on raw `XTEL` bytes
/// (no schema decode of the millions of placed refs). Scoped to FO76/Starfield→FO4
/// by the caller (`ConversionRun::repair_placed_child_refs`).
pub fn repair_placed_references(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let present = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if !present.iter().any(|s| s.as_str() == "REFR") {
        return Ok(FixupReport::empty());
    }
    if session.target_masters().len() > 0xFF {
        return Ok(FixupReport::empty());
    }
    let refr_sig = SigCode::from_str("REFR").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let refr_fks = session
        .form_keys_of_sig(refr_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    repair_placed_references_with_refr_inputs(session, mapper, config, &refr_fks, None)
}

pub(crate) fn repair_placed_references_with_candidates(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    discovery: &PlacedRefrDiscovery,
) -> Result<FixupReport, FixupError> {
    if !discovery.repair_applicable {
        return Ok(FixupReport::empty());
    }
    repair_placed_references_with_refr_inputs(
        session,
        mapper,
        config,
        &discovery.all_refr_fks,
        Some(&discovery.teleport_candidates),
    )
}

fn repair_placed_references_with_refr_inputs(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
    all_refr_fks: &[FormKey],
    teleport_candidates: Option<&[FormKey]>,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let interner = mapper.interner;

    let output_master_index = session.target_masters().len() as u32;
    if output_master_index > 0xFF {
        return Ok(report);
    }
    let own_sym = interner.intern(&session.target_slot().parsed.plugin_name);

    let refr_sig = SigCode::from_str("REFR").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let cell_sig = SigCode::from_str("CELL").map_err(|e| FixupError::SchemaError(e.to_string()))?;

    let own_refr_objids: FxHashSet<u32> = all_refr_fks
        .iter()
        .filter(|fk| fk.plugin == own_sym)
        .map(|fk| fk.local & 0x00FF_FFFF)
        .collect();
    let target_handle_id = session.target_id();
    let own_persistent_refr_objids =
        collect_persistent_objids_for_sig_in_handle(session, target_handle_id, refr_sig)?;
    let own_cell_objids: FxHashSet<u32> = session
        .form_keys_of_sig(cell_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
        .iter()
        .filter(|fk| fk.plugin == own_sym)
        .map(|fk| fk.local & 0x00FF_FFFF)
        .collect();

    let mut master_refr_objids = Vec::with_capacity(config.target_master_handle_ids.len());
    let mut master_persistent_refr_objids =
        Vec::with_capacity(config.target_master_handle_ids.len());
    let mut master_cell_objids = Vec::with_capacity(config.target_master_handle_ids.len());
    let mut master_xczr_objids = Vec::with_capacity(config.target_master_handle_ids.len());
    let mut master_xown_objids = Vec::with_capacity(config.target_master_handle_ids.len());
    let own_xczr_objids = collect_objids_for_sigs_in_handle(
        session,
        session.target_id(),
        XCZR_TARGET_SIGS,
        interner,
    )?;
    let own_xown_objids = collect_objids_for_sigs_in_handle(
        session,
        session.target_id(),
        XOWN_TARGET_SIGS,
        interner,
    )?;
    for &handle_id in &config.target_master_handle_ids {
        master_refr_objids.push(
            session
                .form_keys_of_sig_in_handle(handle_id, refr_sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?
                .into_iter()
                .map(|fk| fk.local & 0x00FF_FFFF)
                .collect(),
        );
        master_persistent_refr_objids.push(collect_persistent_objids_for_sig_in_handle(
            session, handle_id, refr_sig,
        )?);
        master_cell_objids.push(
            session
                .form_keys_of_sig_in_handle(handle_id, cell_sig, interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?
                .into_iter()
                .map(|fk| fk.local & 0x00FF_FFFF)
                .collect(),
        );
        master_xczr_objids.push(collect_objids_for_sigs_in_handle(
            session,
            handle_id,
            XCZR_TARGET_SIGS,
            interner,
        )?);
        master_xown_objids.push(collect_objids_for_sigs_in_handle(
            session,
            handle_id,
            XOWN_TARGET_SIGS,
            interner,
        )?);
    }

    let refr_resolver = FinalRefResolver {
        output_objids: own_refr_objids,
        master_objids: master_refr_objids,
        output_master_index,
    };
    let door_resolver = FinalRefResolver {
        output_objids: own_persistent_refr_objids,
        master_objids: master_persistent_refr_objids,
        output_master_index,
    };
    let cell_resolver = FinalRefResolver {
        output_objids: own_cell_objids,
        master_objids: master_cell_objids,
        output_master_index,
    };
    let xczr_resolver = FinalRefResolver {
        output_objids: own_xczr_objids,
        master_objids: master_xczr_objids,
        output_master_index,
    };
    let xown_resolver = FinalRefResolver {
        output_objids: own_xown_objids,
        master_objids: master_xown_objids,
        output_master_index,
    };

    let candidate_fks = teleport_candidates.unwrap_or(all_refr_fks);
    for fk in candidate_fks {
        if fk.plugin != own_sym {
            continue;
        }
        if teleport_candidates.is_none()
            && !session
                .record_has_any_subrecord(fk, &["XTEL", "XASP", "XCZR", "XOWN"])
                .unwrap_or(false)
        {
            continue;
        }
        if repair_refr_subrecords(
            session,
            fk,
            &refr_resolver,
            &door_resolver,
            &cell_resolver,
            &xczr_resolver,
            &xown_resolver,
        )? {
            report.records_changed = report.records_changed.saturating_add(1);
        }
    }

    Ok(report)
}

fn collect_persistent_objids_for_sig_in_handle(
    session: &mut PluginSession,
    handle_id: u64,
    sig: SigCode,
) -> Result<FxHashSet<u32>, FixupError> {
    let scan = session
        .handle_raw_scan(handle_id)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    Ok(scan
        .raw_form_ids_of_sig(sig)
        .into_iter()
        .filter(|raw| {
            scan.with_record(*raw, |record| record.flags & RECORD_FLAG_PERSISTENT != 0)
                .unwrap_or(false)
        })
        .map(|raw| raw & 0x00FF_FFFF)
        .collect())
}

fn collect_objids_for_sigs_in_handle(
    session: &mut PluginSession,
    handle_id: u64,
    sigs: &[&str],
    interner: &crate::sym::StringInterner,
) -> Result<FxHashSet<u32>, FixupError> {
    let mut object_ids = FxHashSet::default();
    for sig in sigs {
        let sig = SigCode::from_str(sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
        for fk in session
            .form_keys_of_sig_in_handle(handle_id, sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            object_ids.insert(fk.local & 0x00FF_FFFF);
        }
    }
    Ok(object_ids)
}

pub fn repair_placed_teleport_doors(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    repair_placed_references(session, mapper, config)
}

fn repair_refr_subrecords(
    session: &mut PluginSession,
    fk: &FormKey,
    refr_resolver: &FinalRefResolver,
    door_resolver: &FinalRefResolver,
    cell_resolver: &FinalRefResolver,
    xczr_resolver: &FinalRefResolver,
    xown_resolver: &FinalRefResolver,
) -> Result<bool, FixupError> {
    let raw_form_id = session
        .raw_form_id_for_form_key(fk)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut changed = false;
    {
        let record = session
            .record_mut(raw_form_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let old_subrecords = std::mem::take(&mut record.subrecords);
        let mut retained = Vec::with_capacity(old_subrecords.len());
        let mut iter = old_subrecords.into_iter().peekable();

        while let Some(mut subrecord) = iter.next() {
            let sig = subrecord.signature.as_str();
            let resolution = match sig {
                "XASP" => Some(
                    read_formid(&subrecord.data, 0)
                        .map(|raw| refr_resolver.resolve(raw))
                        .unwrap_or(RefResolution::Dangling),
                ),
                "XCZR" => Some(
                    read_formid(&subrecord.data, 0)
                        .map(|raw| xczr_resolver.resolve(raw))
                        .unwrap_or(RefResolution::Dangling),
                ),
                "XOWN" => Some(
                    read_formid(&subrecord.data, 0)
                        .map(|raw| xown_resolver.resolve(raw))
                        .unwrap_or(RefResolution::Dangling),
                ),
                _ => None,
            };

            if sig == "XTEL" {
                let door = read_formid(&subrecord.data, XTEL_DOOR_OFFSET)
                    .map(|raw| door_resolver.resolve_source_own(raw))
                    .unwrap_or(RefResolution::Dangling);
                let transition = read_formid(&subrecord.data, XTEL_TRANSITION_OFFSET)
                    .filter(|raw| *raw != 0)
                    .map(|raw| cell_resolver.resolve_source_own(raw))
                    .unwrap_or(RefResolution::Keep);
                if matches!(door, RefResolution::Dangling)
                    || matches!(transition, RefResolution::Dangling)
                {
                    changed = true;
                    continue;
                }
                let mut bytes = subrecord.data.to_vec();
                if let RefResolution::Repair(raw) = door {
                    bytes[XTEL_DOOR_OFFSET..XTEL_DOOR_OFFSET + 4]
                        .copy_from_slice(&raw.to_le_bytes());
                    changed = true;
                }
                if let RefResolution::Repair(raw) = transition {
                    bytes[XTEL_TRANSITION_OFFSET..XTEL_TRANSITION_OFFSET + 4]
                        .copy_from_slice(&raw.to_le_bytes());
                    changed = true;
                }
                subrecord.data = bytes.into();
                retained.push(subrecord);
                continue;
            }

            match resolution {
                Some(RefResolution::Repair(raw)) => {
                    let mut bytes = subrecord.data.to_vec();
                    bytes[..4].copy_from_slice(&raw.to_le_bytes());
                    subrecord.data = bytes.into();
                    changed = true;
                    retained.push(subrecord);
                }
                Some(RefResolution::Dangling) => {
                    changed = true;
                    if sig == "XOWN" {
                        while iter
                            .peek()
                            .is_some_and(|next| matches!(next.signature.as_str(), "XRNK" | "XGLB"))
                        {
                            iter.next();
                        }
                    }
                }
                Some(RefResolution::Keep) | None => retained.push(subrecord),
            }
        }
        record.subrecords = retained;
    }
    if changed {
        session.record_effect(WriteEffect::RecordContents {
            form_ids: smallvec::smallvec![raw_form_id],
        });
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{
        ParsedSubrecord, plugin_handle_add_master_native, plugin_handle_close_native,
        plugin_handle_load_no_py, plugin_handle_new_native, plugin_handle_save_no_py,
    };
    use smallvec::SmallVec;
    use smol_str::SmolStr;

    fn resolver(output: &[u32], masters: &[&[u32]]) -> FinalRefResolver {
        FinalRefResolver {
            output_objids: output.iter().copied().collect(),
            master_objids: masters
                .iter()
                .map(|ids| ids.iter().copied().collect())
                .collect(),
            output_master_index: masters.len() as u32,
        }
    }

    #[test]
    fn rewrites_master_byte_door_to_own_when_partner_exists() {
        let resolver = resolver(&[0x6295D2], &[&[]]);
        assert_eq!(
            resolver.resolve(0x0062_95D2),
            RefResolution::Repair(0x0162_95D2),
        );
    }

    #[test]
    fn leaves_already_own_ref_untouched() {
        let resolver = resolver(&[0x6295D2], &[&[]]);
        assert_eq!(resolver.resolve(0x0162_95D2), RefResolution::Keep);
    }

    #[test]
    fn leaves_valid_master_ref_even_with_own_shadow() {
        let resolver = resolver(&[0x6295D2], &[&[0x6295D2]]);
        assert_eq!(resolver.resolve(0x0062_95D2), RefResolution::Keep);
    }

    #[test]
    fn source_own_ref_prefers_output_when_master_has_same_object_id() {
        let resolver = resolver(&[0x6295D2], &[&[0x6295D2]]);
        assert_eq!(
            resolver.resolve_source_own(0x0062_95D2),
            RefResolution::Repair(0x0162_95D2),
        );
    }

    #[test]
    fn marks_null_and_missing_targets_dangling() {
        let resolver = resolver(&[0x111111], &[&[0x000010]]);
        assert_eq!(resolver.resolve(0), RefResolution::Dangling);
        assert_eq!(resolver.resolve(0x0062_95D2), RefResolution::Dangling);
        assert_eq!(resolver.resolve(0x0162_95D2), RefResolution::Dangling);
    }

    #[test]
    fn preserves_unscanned_declared_master() {
        let resolver = FinalRefResolver {
            output_objids: FxHashSet::default(),
            master_objids: Vec::new(),
            output_master_index: 1,
        };
        assert_eq!(resolver.resolve(0x0000_ABCD), RefResolution::Keep);
    }

    fn field(sig: &str, bytes: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn record(
        plugin: &str,
        sig: &str,
        local: u32,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern(plugin),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    fn xtel(door: u32, transition: u32) -> FieldEntry {
        let mut bytes = vec![0u8; 36];
        bytes[XTEL_DOOR_OFFSET..XTEL_DOOR_OFFSET + 4].copy_from_slice(&door.to_le_bytes());
        bytes[XTEL_TRANSITION_OFFSET..XTEL_TRANSITION_OFFSET + 4]
            .copy_from_slice(&transition.to_le_bytes());
        field("XTEL", bytes)
    }

    fn formid_field(sig: &str, raw: u32) -> FieldEntry {
        let mut bytes = raw.to_le_bytes().to_vec();
        if sig == "XOWN" {
            bytes.extend_from_slice(&0u64.to_le_bytes());
        }
        field(sig, bytes)
    }

    fn persistent_record(
        plugin: &str,
        sig: &str,
        local: u32,
        fields: Vec<FieldEntry>,
        interner: &StringInterner,
    ) -> Record {
        let mut record = record(plugin, sig, local, fields, interner);
        record.flags = RecordFlags::PERSISTENT;
        record
    }

    #[test]
    fn starfield_to_fo4_drops_unresolved_and_wrong_type_refr_xown() {
        let interner = StringInterner::new();
        let master = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        {
            let mut session = open_session(master, None).unwrap();
            let schema = session.schema().unwrap();
            session
                .add_record(
                    record("Fallout4.esm", "REFR", 0x01961D, vec![], &interner),
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
        }

        let target = plugin_handle_new_native("Starfield.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        {
            let mut session = open_session(target, None).unwrap();
            let schema = session.schema().unwrap();
            for placed in [
                record(
                    "Starfield.esm",
                    "REFR",
                    0x1FDD1A,
                    vec![formid_field("XOWN", 0x0001_961D)],
                    &interner,
                ),
                record(
                    "Starfield.esm",
                    "REFR",
                    0x1FDD1C,
                    vec![formid_field("XOWN", 0x0026_FDEA)],
                    &interner,
                ),
            ] {
                session
                    .add_record(placed, schema.as_ref(), &interner)
                    .unwrap();
            }
        }

        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut config = FixupConfig::default();
        config.target_master_handle_ids = vec![master];
        let mut session = open_session(target, None).unwrap();

        let report = repair_placed_references(&mut session, &mut mapper, &config).unwrap();

        assert_eq!(report.records_changed, 2);
        for local in [0x1FDD1A, 0x1FDD1C] {
            let fk = FormKey {
                local,
                plugin: interner.intern("Starfield.esm"),
            };
            assert!(
                session
                    .first_subrecord_bytes(&fk, "XOWN")
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn starfield_to_fo4_repairs_persistent_xtel_partner_and_drops_nonpersistent_target() {
        let interner = StringInterner::new();
        let master = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        let target = plugin_handle_new_native("Starfield.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        {
            let mut session = open_session(target, None).unwrap();
            let schema = session.schema().unwrap();
            for placed in [
                persistent_record("Starfield.esm", "REFR", 0x23BED9, vec![], &interner),
                record("Starfield.esm", "REFR", 0x0DD2A5, vec![], &interner),
                record(
                    "Starfield.esm",
                    "REFR",
                    0x22920F,
                    vec![xtel(0x0023_BED9, 0)],
                    &interner,
                ),
                record(
                    "Starfield.esm",
                    "REFR",
                    0x0DD2AB,
                    vec![xtel(0x000D_D2A5, 0)],
                    &interner,
                ),
            ] {
                session
                    .add_record(placed, schema.as_ref(), &interner)
                    .unwrap();
            }
        }

        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut config = FixupConfig::default();
        config.target_master_handle_ids = vec![master];
        let mut session = open_session(target, None).unwrap();

        let report = repair_placed_references(&mut session, &mut mapper, &config).unwrap();

        assert_eq!(report.records_changed, 2);
        let persistent_fk = FormKey {
            local: 0x22920F,
            plugin: interner.intern("Starfield.esm"),
        };
        let persistent_xtel = session
            .first_subrecord_bytes(&persistent_fk, "XTEL")
            .unwrap()
            .unwrap();
        assert_eq!(
            read_formid(&persistent_xtel, XTEL_DOOR_OFFSET),
            Some(0x0123_BED9)
        );
        let nonpersistent_fk = FormKey {
            local: 0x0DD2AB,
            plugin: interner.intern("Starfield.esm"),
        };
        assert!(
            session
                .first_subrecord_bytes(&nonpersistent_fk, "XTEL")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn post_copy_repairs_and_drops_placed_reference_payloads_idempotently() {
        let interner = StringInterner::new();
        let master = plugin_handle_new_native("MasterA.esm", Some("fo4")).unwrap();
        {
            let mut session = open_session(master, None).unwrap();
            let schema = session.schema().unwrap();
            for record in [
                record("MasterA.esm", "REFR", 0x800, vec![], &interner),
                record("MasterA.esm", "FACT", 0x801, vec![], &interner),
                record("MasterA.esm", "CELL", 0x802, vec![], &interner),
                record("MasterA.esm", "ACHR", 0x803, vec![], &interner),
                record("MasterA.esm", "NPC_", 0x804, vec![], &interner),
                record("MasterA.esm", "KYWD", 0x805, vec![], &interner),
            ] {
                session
                    .add_record(record, schema.as_ref(), &interner)
                    .unwrap();
            }
        }

        let target = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "MasterA.esm", None).unwrap();
        {
            let mut session = open_session(target, None).unwrap();
            let schema = session.schema().unwrap();
            let records = [
                persistent_record("Out.esm", "REFR", 0x800, vec![], &interner),
                record("Out.esm", "CELL", 0x802, vec![], &interner),
                persistent_record("Out.esm", "REFR", 0x900, vec![], &interner),
                record("Out.esm", "CELL", 0x901, vec![], &interner),
                record("Out.esm", "FACT", 0x902, vec![], &interner),
                record(
                    "Out.esm",
                    "REFR",
                    0xA00,
                    vec![
                        formid_field("XASP", 0x0100_DEAD),
                        formid_field("XCZR", 0x0000_0900),
                        formid_field("XOWN", 0x0100_BEEF),
                        field("XRNK", 1i32.to_le_bytes().to_vec()),
                        formid_field("XGLB", 0x0000_0801),
                        xtel(0x0100_CAFE, 0),
                    ],
                    &interner,
                ),
                record(
                    "Out.esm",
                    "REFR",
                    0xA01,
                    vec![
                        formid_field("XASP", 0x0000_0800),
                        formid_field("XCZR", 0x0000_0800),
                        formid_field("XOWN", 0x0000_0801),
                        xtel(0x0000_0800, 0x0000_0802),
                    ],
                    &interner,
                ),
                record(
                    "Out.esm",
                    "REFR",
                    0xA02,
                    vec![xtel(0x0000_0900, 0x0000_0901)],
                    &interner,
                ),
                record(
                    "Out.esm",
                    "REFR",
                    0xA03,
                    vec![
                        formid_field("XCZR", 0x0100_DEAD),
                        formid_field("XCZR", 0x0000_0803),
                        formid_field("XOWN", 0x0100_BEEF),
                        field("XRNK", 2i32.to_le_bytes().to_vec()),
                        formid_field("XOWN", 0x0000_0804),
                        field("XRNK", 3i32.to_le_bytes().to_vec()),
                    ],
                    &interner,
                ),
                record(
                    "Out.esm",
                    "REFR",
                    0xA04,
                    vec![formid_field("XCZR", 0x0000_0805)],
                    &interner,
                ),
            ];
            for record in records {
                session
                    .add_record(record, schema.as_ref(), &interner)
                    .unwrap();
            }
            // Author the ownership row in its raw anchored order. The generic
            // decoded writer canonicalizes XGLB later in the record, which is
            // not the repeatable XOWN + companion layout this repair consumes.
            let fk = FormKey {
                local: 0xA00,
                plugin: interner.intern("Out.esm"),
            };
            let raw = session.raw_form_id_for_form_key(&fk).unwrap();
            let record = session.record_mut(raw).unwrap();
            let xglb_index = record
                .subrecords
                .iter()
                .position(|subrecord| subrecord.signature.as_str() == "XGLB")
                .unwrap();
            let xglb = record.subrecords.remove(xglb_index);
            let xrnk_index = record
                .subrecords
                .iter()
                .position(|subrecord| subrecord.signature.as_str() == "XRNK")
                .unwrap();
            record.subrecords.insert(xrnk_index + 1, xglb);
        }

        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let mut config = FixupConfig::default();
        config.target_master_handle_ids = vec![master];
        let mut session = open_session(target, None).unwrap();

        let first = repair_placed_references(&mut session, &mut mapper, &config).unwrap();
        assert_eq!(first.records_changed, 5);

        let dropped_fk = FormKey {
            local: 0xA00,
            plugin: interner.intern("Out.esm"),
        };
        let dropped_raw = session.raw_form_id_for_form_key(&dropped_fk).unwrap();
        let dropped_sigs: Vec<String> = session
            .record_mut(dropped_raw)
            .unwrap()
            .subrecords
            .iter()
            .map(|subrecord| subrecord.signature.to_string())
            .collect();
        for sig in ["XASP", "XOWN", "XRNK", "XGLB", "XTEL"] {
            assert!(
                session
                    .first_subrecord_bytes(&dropped_fk, sig)
                    .unwrap()
                    .is_none(),
                "{sig} should be removed; surviving={dropped_sigs:?}"
            );
        }
        assert_eq!(
            read_formid(
                &session
                    .first_subrecord_bytes(&dropped_fk, "XCZR")
                    .unwrap()
                    .unwrap(),
                0,
            ),
            Some(0x0100_0900),
        );

        let master_fk = FormKey {
            local: 0xA01,
            plugin: interner.intern("Out.esm"),
        };
        for sig in ["XASP", "XCZR", "XOWN", "XTEL"] {
            assert!(
                session
                    .first_subrecord_bytes(&master_fk, sig)
                    .unwrap()
                    .is_some()
            );
        }
        let shadowed_xtel = session
            .first_subrecord_bytes(&master_fk, "XTEL")
            .unwrap()
            .unwrap();
        assert_eq!(
            read_formid(&shadowed_xtel, XTEL_DOOR_OFFSET),
            Some(0x0100_0800),
        );
        assert_eq!(
            read_formid(&shadowed_xtel, XTEL_TRANSITION_OFFSET),
            Some(0x0100_0802),
        );

        let repaired_fk = FormKey {
            local: 0xA02,
            plugin: interner.intern("Out.esm"),
        };
        let repaired_xtel = session
            .first_subrecord_bytes(&repaired_fk, "XTEL")
            .unwrap()
            .unwrap();
        assert_eq!(
            read_formid(&repaired_xtel, XTEL_DOOR_OFFSET),
            Some(0x0100_0900)
        );
        assert_eq!(
            read_formid(&repaired_xtel, XTEL_TRANSITION_OFFSET),
            Some(0x0100_0901),
        );

        let repeatable_fk = FormKey {
            local: 0xA03,
            plugin: interner.intern("Out.esm"),
        };
        let repeatable_raw = session.raw_form_id_for_form_key(&repeatable_fk).unwrap();
        let repeatable = session.record_mut(repeatable_raw).unwrap();
        let repeatable_sigs: Vec<&str> = repeatable
            .subrecords
            .iter()
            .map(|subrecord| subrecord.signature.as_str())
            .collect();
        assert_eq!(repeatable_sigs, vec!["XOWN", "XRNK", "XCZR"]);
        let xown = repeatable
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "XOWN")
            .unwrap();
        let xczr = repeatable
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "XCZR")
            .unwrap();
        assert_eq!(read_formid(&xown.data, 0), Some(0x0000_0804));
        assert_eq!(read_formid(&xczr.data, 0), Some(0x0000_0803));

        let wrong_type_fk = FormKey {
            local: 0xA04,
            plugin: interner.intern("Out.esm"),
        };
        assert!(
            session
                .first_subrecord_bytes(&wrong_type_fk, "XCZR")
                .unwrap()
                .is_none()
        );

        let second = repair_placed_references(&mut session, &mut mapper, &config).unwrap();
        assert_eq!(second.records_changed, 0);
    }

    #[test]
    fn discovery_scans_each_refr_once_and_observes_structural_changes() {
        let interner = StringInterner::new();
        let target = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "MasterA.esm", None).unwrap();
        let own_plugin = interner.intern("Out.esm");
        {
            let mut session = open_session(target, None).unwrap();
            let schema = session.schema().unwrap();
            for placed in [
                record(
                    "Out.esm",
                    "REFR",
                    0x100,
                    vec![xtel(0x0100_0200, 0), field("XRDS", vec![0; 4])],
                    &interner,
                ),
                record(
                    "Out.esm",
                    "REFR",
                    0x101,
                    vec![field("XEZN", vec![1, 2])],
                    &interner,
                ),
                record("Out.esm", "REFR", 0x102, vec![], &interner),
            ] {
                session
                    .add_record(placed, schema.as_ref(), &interner)
                    .unwrap();
            }
            let compressed_fk = FormKey {
                local: 0x101,
                plugin: own_plugin,
            };
            let compressed_raw = session.raw_form_id_for_form_key(&compressed_fk).unwrap();
            let compressed = session.record_mut(compressed_raw).unwrap();
            compressed.raw_payload = Some(Bytes::from_static(b"retained compressed payload"));
            compressed.parse_error = Some("fixture decode warning".to_string());

            let first = discover_placed_refr_candidates(&mut session, &interner).unwrap();
            assert_eq!(first.records_seen, 3);
            assert_eq!(first.records_inspected, 3);
            assert_eq!(
                first
                    .teleport_candidates
                    .iter()
                    .map(|fk| fk.local)
                    .collect::<Vec<_>>(),
                vec![0x100]
            );
            assert_eq!(
                first
                    .light_radius_candidates
                    .iter()
                    .map(|fk| fk.local)
                    .collect::<Vec<_>>(),
                vec![0x100]
            );
            assert_eq!(
                first
                    .xezn_candidates
                    .iter()
                    .map(|fk| fk.local)
                    .collect::<Vec<_>>(),
                vec![0x101]
            );

            let altered_fk = FormKey {
                local: 0x102,
                plugin: own_plugin,
            };
            let altered_raw = session.raw_form_id_for_form_key(&altered_fk).unwrap();
            session
                .record_mut(altered_raw)
                .unwrap()
                .subrecords
                .push(ParsedSubrecord {
                    signature: SmolStr::new("XTEL"),
                    data: Bytes::from(vec![0; 36]),
                    semantic_type: None,
                });
            session.record_effect(WriteEffect::RecordContents {
                form_ids: smallvec::smallvec![altered_raw],
            });
            let after_earlier_pass =
                discover_placed_refr_candidates(&mut session, &interner).unwrap();
            assert_eq!(after_earlier_pass.records_seen, 3);
            assert_eq!(after_earlier_pass.records_inspected, 3);
            assert!(after_earlier_pass.teleport_candidates.contains(&altered_fk));

            let added_fk = FormKey {
                local: 0x103,
                plugin: own_plugin,
            };
            session
                .add_record(
                    record(
                        "Out.esm",
                        "REFR",
                        added_fk.local,
                        vec![formid_field("XASP", 0x0100_0100)],
                        &interner,
                    ),
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
            let after_add = discover_placed_refr_candidates(&mut session, &interner).unwrap();
            assert_eq!(after_add.records_seen, 4);
            assert_eq!(after_add.records_inspected, 4);
            assert!(after_add.teleport_candidates.contains(&added_fk));

            assert!(session.remove_record(&added_fk).unwrap());
            let after_remove = discover_placed_refr_candidates(&mut session, &interner).unwrap();
            assert_eq!(after_remove.records_seen, 3);
            assert_eq!(after_remove.records_inspected, 3);
            assert!(!after_remove.teleport_candidates.contains(&added_fk));
        }
        assert!(plugin_handle_close_native(target));
    }

    #[test]
    fn shared_candidates_match_legacy_scan_for_compressed_and_malformed_records() {
        let temp = tempfile::tempdir().unwrap();
        let fixture_path = temp.path().join("Out.esm");
        let legacy_path = temp.path().join("legacy.esm");
        let shared_path = temp.path().join("shared.esm");
        let interner = StringInterner::new();
        let master = plugin_handle_new_native("MasterA.esm", Some("fo4")).unwrap();
        let fixture = plugin_handle_new_native("Out.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(fixture, "MasterA.esm", None).unwrap();
        {
            let mut session = open_session(fixture, None).unwrap();
            let schema = session.schema().unwrap();
            for placed in [
                persistent_record("Out.esm", "REFR", 0x900, vec![], &interner),
                record("Out.esm", "CELL", 0x901, vec![], &interner),
                record(
                    "Out.esm",
                    "REFR",
                    0xA00,
                    vec![xtel(0x0000_0900, 0x0000_0901)],
                    &interner,
                ),
                record(
                    "Out.esm",
                    "REFR",
                    0xA01,
                    vec![field("XTEL", vec![1, 2]), field("XRDS", vec![3, 4])],
                    &interner,
                ),
                record(
                    "Out.esm",
                    "REFR",
                    0xA02,
                    vec![field("XEZN", vec![5, 6])],
                    &interner,
                ),
            ] {
                let mut placed = placed;
                if placed.form_key.local == 0xA00 {
                    placed.flags |= RecordFlags::COMPRESSED;
                }
                session
                    .add_record(placed, schema.as_ref(), &interner)
                    .unwrap();
            }
        }
        plugin_handle_save_no_py(fixture, fixture_path.to_str().unwrap()).unwrap();
        assert!(plugin_handle_close_native(fixture));

        let run = |shared: bool, output: &std::path::Path| {
            let handle = plugin_handle_load_no_py(
                fixture_path.to_str().unwrap(),
                Some("fo4"),
                None,
                None,
                true,
            )
            .unwrap();
            let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            let mut config = FixupConfig::default();
            config.target_master_handle_ids = vec![master];
            let report = {
                let mut session = open_session(handle, None).unwrap();
                let compressed_fk = FormKey {
                    local: 0xA00,
                    plugin: interner.intern("Out.esm"),
                };
                let compressed_raw = session.raw_form_id_for_form_key(&compressed_fk).unwrap();
                assert!(
                    session
                        .record(compressed_raw)
                        .unwrap()
                        .raw_payload
                        .is_some()
                );
                if shared {
                    let discovery =
                        discover_placed_refr_candidates(&mut session, &interner).unwrap();
                    repair_placed_references_with_candidates(
                        &mut session,
                        &mut mapper,
                        &config,
                        &discovery,
                    )
                    .unwrap()
                } else {
                    repair_placed_references(&mut session, &mut mapper, &config).unwrap()
                }
            };
            plugin_handle_save_no_py(handle, output.to_str().unwrap()).unwrap();
            assert!(plugin_handle_close_native(handle));
            (report, std::fs::read(output).unwrap())
        };

        let (legacy_report, legacy_bytes) = run(false, &legacy_path);
        let (shared_report, shared_bytes) = run(true, &shared_path);
        assert_eq!(legacy_report.records_changed, shared_report.records_changed);
        assert_eq!(legacy_report.records_dropped, shared_report.records_dropped);
        assert_eq!(legacy_report.warnings, shared_report.warnings);
        assert_eq!(legacy_report.diagnostics, shared_report.diagnostics);
        assert_eq!(legacy_bytes, shared_bytes);
        assert!(plugin_handle_close_native(master));
    }
}
