//! Fixup: resolve post-copy placed-reference targets.
//!
//! # Why
//! A teleport door's `XTEL` (Teleport-Destination) carries the destination door
//! FormID at byte offset 0 and the transition-interior CELL FormID at offset 32.
//! In FO76 the source plugin has NO masters, so its own refs use master byte
//! `0x00`. The cell-slice copy path (`cell_slice::rewrite_placed_child_local_refs`,
//! offsets `[0,32]`) remaps that own byte `0x00` → the FO4 output own index for
//! EXTERIOR / worldspace placed children. The interior-cell emit path
//! (`run::emit_interior_cells`) uses a separate translate/insert path, and `XTEL`
//! has an IDENTICAL FO76/FO4 layout (`struct:I,f×6,I,I`) so relayout keeps it as
//! RAW bytes — `FormKeyMapper::rewrite_record` only touches decoded FormKey leaves,
//! never these raw bytes. So interior teleport doors reach the output with the
//! FO76 byte `0x00` intact, which in the FO4 load order resolves to the FIRST
//! master (`Fallout4.esm`) instead of the converted own door → dead teleport
//! (e.g. `coc WhitespringMall01` exit doors do nothing).
//!
//! # How
//! Runs in the POST-COPY hook (`ConversionRun::repair_placed_child_refs`) where
//! ALL placed children — interior and exterior — are present. Path-independent
//! like `resolve_placed_leveled_bases`: a future copy path can't bypass it.
//!
//! For each REFR carrying an `XTEL`, the door FormID (offset 0) is rewritten to the
//! own plugin when it currently names a target MASTER but an own REFR exists at the
//! same object-id (the preserved partner door); the transition CELL (offset 32) is
//! rewritten the same way against own CELLs. FO76 is masterless, so the own record
//! wins even when a FO4 master happens to contain the same object-id. Doors already
//! pointing at the own
//! plugin (correct exterior/worldspace doors, or interior doors that happened to be
//! emitted via the cell-slice path) are a no-op (idempotent). Object-ids are
//! preserved across this conversion, so the own record at the door's object-id IS
//! the intended target.
//!
//! Optional `XASP`, `XCZR`, and `XOWN` payloads use the same final resolver and
//! are omitted when their targets are still absent. An `XTEL` whose destination
//! door or non-null transition cell is absent is omitted as a complete payload.

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

/// Repair interior placed-door teleport targets in the finished output plugin.
/// Mirrors `resolve_placed_leveled_bases`: called from the post-copy hook so it
/// sees every placed child (interior + exterior), and operates on raw `XTEL` bytes
/// (no schema decode of the millions of placed refs). Scoped to FO76→FO4 by the
/// caller (`ConversionRun::repair_placed_child_refs`).
pub fn repair_placed_references(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let interner = mapper.interner;

    // REFR is the only placed type that carries an XTEL teleport destination.
    let present = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    if !present.iter().any(|s| s.as_str() == "REFR") {
        return Ok(report);
    }

    let output_master_index = session.target_masters().len() as u32;
    if output_master_index > 0xFF {
        return Ok(report);
    }
    let own_sym = interner.intern(&session.target_slot().parsed.plugin_name);

    let refr_sig = SigCode::from_str("REFR").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let cell_sig = SigCode::from_str("CELL").map_err(|e| FixupError::SchemaError(e.to_string()))?;

    let refr_fks = session
        .form_keys_of_sig(refr_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let own_refr_objids: FxHashSet<u32> = refr_fks
        .iter()
        .filter(|fk| fk.plugin == own_sym)
        .map(|fk| fk.local & 0x00FF_FFFF)
        .collect();
    let own_cell_objids: FxHashSet<u32> = session
        .form_keys_of_sig(cell_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?
        .iter()
        .filter(|fk| fk.plugin == own_sym)
        .map(|fk| fk.local & 0x00FF_FFFF)
        .collect();

    let mut master_refr_objids = Vec::with_capacity(config.target_master_handle_ids.len());
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

    for fk in refr_fks {
        if fk.plugin != own_sym {
            continue;
        }
        if !session
            .record_has_any_subrecord(&fk, &["XTEL", "XASP", "XCZR", "XOWN"])
            .unwrap_or(false)
        {
            continue;
        }
        if repair_refr_subrecords(
            session,
            &fk,
            &refr_resolver,
            &cell_resolver,
            &xczr_resolver,
            &xown_resolver,
        )? {
            report.records_changed = report.records_changed.saturating_add(1);
        }
    }

    Ok(report)
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
                    .map(|raw| refr_resolver.resolve_source_own(raw))
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
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_new_native,
    };
    use smallvec::SmallVec;

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
                record("Out.esm", "REFR", 0x800, vec![], &interner),
                record("Out.esm", "CELL", 0x802, vec![], &interner),
                record("Out.esm", "REFR", 0x900, vec![], &interner),
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
}
