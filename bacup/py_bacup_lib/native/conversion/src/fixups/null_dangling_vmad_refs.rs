//! Fixup: repair/null VMAD script-property Object FormIDs that resolve to neither
//! the output plugin nor any target master.
//!
//! VMAD Object-union FormIDs sit inside the opaque VMAD blob.
//! `FormKeyMapper::rewrite_vmad_formids` remaps every object whose source record was
//! translated, but objects in FO76 records outside the converted Appalachia slice
//! (interior REFR/CELL/ACHR, un-emitted SCEN/CHAL) keep their source `00` prefix and
//! dangle as `Fallout4.esm:00xxxxxx`. Census of 00-prefix VMAD objects on
//! ACTI/TERM/DOOR/NOTE/CONT/MGEF: 754 resolve in Fallout4.esm (kept), 0 codec
//! misses, ~217 never emitted (REFR 163, CELL 41, ACHR 8, SCEN 4, CHAL 1).
//!
//! Each FormID is judged on its encoded `(master_index, object_id)`. One that
//! resolves in its addressed handle is left byte-identical; one absent from its
//! master but present in the output is repaired to the output master byte; one that
//! resolves nowhere is nulled.
//!
//! With `include_interior`, interior cells and their placed children (e.g. the
//! targets of a shelter door's `ShelterCell` / `ShelterCellTeleportPosition`) arrive
//! post-copy. Under `FixupConfig::defer_placed_child_ref_class` the pre-copy sweep
//! repairs but does not null; `repair_dangling_vmad_refs` (from
//! `ConversionRun::repair_placed_child_refs`) nulls over the complete output.
//!
//! VMAD-blob counterpart of `null_dangling_own_plugin_refs`, which never walks inside
//! VMAD; the two are disjoint by subrecord. The byte walk mirrors
//! `FormKeyMapper::rewrite_vmad_formids` (property types 1/7/11/17) and aborts with
//! no change on a malformed or truncated blob.

use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;

use super::validate_reference_target_types::FO4_HARDCODED_AVIF_LOCAL_IDS;

/// Record signatures whose VMAD script-property Object FormIDs are checked; the
/// allow-list bounds decode work. All use the standard VMAD object-property layouts.
/// For INFO the walk stops after `script_count` scripts and never reads the fragment
/// trailer, so only script-property objects are reached.
pub(crate) const TOUCHED_RECORD_SIGS: &[&str] = &[
    "ACTI", "TERM", "DOOR", "NOTE", "CONT", "MGEF", "PACK", "INFO", "QUST", "BOOK", "FURN", "MISC",
    "MSTT", "NPC_", "REFR", "ACHR", "SCEN",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum QustVmadLane {
    TopLevel,
    Fragment,
    Alias,
}

struct ForcedOutputBinding {
    quest_object_id: u32,
    lane: QustVmadLane,
    alias_index: Option<i16>,
    script_name: &'static [u8],
    property_name: &'static [u8],
    target_object_id: u32,
}

/// FO76 QUST VMAD bindings whose source-local target collides with a real
/// Fallout4.esm record at the same object ID. Only these exact binding paths
/// may prefer the output record over the addressed master.
const QUST_FORCED_OUTPUT_BINDINGS: &[ForcedOutputBinding] = &[
    ForcedOutputBinding {
        quest_object_id: 0x0007_1505,
        lane: QustVmadLane::Alias,
        alias_index: Some(0),
        script_name: b"EN01_BunkerAccessPadAliasScript",
        property_name: b"InvisibleBunkerDoor",
        target_object_id: 0x001A_E92D,
    },
    ForcedOutputBinding {
        quest_object_id: 0x0010_E201,
        lane: QustVmadLane::Fragment,
        alias_index: None,
        script_name: b"Fragments:Quests:QF_TW002_0010E201",
        property_name: b"TW002Hunter01Ref",
        target_object_id: 0x0010_E216,
    },
    ForcedOutputBinding {
        quest_object_id: 0x0010_E201,
        lane: QustVmadLane::Fragment,
        alias_index: None,
        script_name: b"Fragments:Quests:QF_TW002_0010E201",
        property_name: b"TW002Hunter02Ref",
        target_object_id: 0x0010_E218,
    },
    ForcedOutputBinding {
        quest_object_id: 0x0010_E201,
        lane: QustVmadLane::Fragment,
        alias_index: None,
        script_name: b"Fragments:Quests:QF_TW002_0010E201",
        property_name: b"TW002Hunter03Ref",
        target_object_id: 0x0010_E217,
    },
    ForcedOutputBinding {
        quest_object_id: 0x0034_43FB,
        lane: QustVmadLane::TopLevel,
        alias_index: None,
        script_name: b"MTR07_EarthQuestScript",
        property_name: b"MTR07_EarthIgnitionCoreReactorTrigger04Ref",
        target_object_id: 0x0013_AAEE,
    },
    ForcedOutputBinding {
        quest_object_id: 0x0034_43FB,
        lane: QustVmadLane::TopLevel,
        alias_index: None,
        script_name: b"MTR07_EarthQuestScript",
        property_name: b"myWorkshop",
        target_object_id: 0x0018_43FC,
    },
    ForcedOutputBinding {
        quest_object_id: 0x0034_43FB,
        lane: QustVmadLane::Fragment,
        alias_index: None,
        script_name: b"Fragments:Quests:QF_MTR07_Earth_003443FB",
        property_name: b"MountBlairWorkshopRef",
        target_object_id: 0x0018_43FC,
    },
    ForcedOutputBinding {
        quest_object_id: 0x002C_460C,
        lane: QustVmadLane::Alias,
        alias_index: Some(22),
        script_name: b"DefaultAliasOnContainerChangedFrom",
        property_name: b"ItemChangedFromReferences",
        target_object_id: 0x0023_D302,
    },
    ForcedOutputBinding {
        quest_object_id: 0x002A_93F5,
        lane: QustVmadLane::TopLevel,
        alias_index: None,
        script_name: b"CB04_QuestScript",
        property_name: b"CB04_Reward_LootPrewar_SafeRef",
        target_object_id: 0x0023_AD3C,
    },
    ForcedOutputBinding {
        quest_object_id: 0x002A_93F5,
        lane: QustVmadLane::TopLevel,
        alias_index: None,
        script_name: b"CB04_QuestScript",
        property_name: b"EBSActor",
        target_object_id: 0x0022_278C,
    },
    ForcedOutputBinding {
        quest_object_id: 0x0013_1033,
        lane: QustVmadLane::TopLevel,
        alias_index: None,
        script_name: b"SFL02_Track_QuestScript",
        property_name: b"SFL02_Track_RadioSignalMarkerRef",
        target_object_id: 0x0013_3D6A,
    },
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

/// Post-copy VMAD resolve over the complete output: repairs emitted targets to the
/// output master byte and nulls the residue a deferred pre-copy sweep left intact.
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
#[derive(Clone)]
pub(crate) struct VmadResolver {
    pub(crate) output_objids: FxHashSet<u32>,
    /// master_index → object-id set, parallel to the target master load order.
    /// `Arc` so the store2 master-scan cache can share them across sweeps.
    master_objids: Vec<Arc<FxHashSet<u32>>>,
    /// Encoded master index of Fallout4.esm for an FO4 target, when that master
    /// is actually present in the output plugin header.
    fo4_base_master_index: Option<u32>,
    /// Number of target masters; the output plugin's own master byte is this.
    output_master_index: u32,
    /// Leave a slot that resolves nowhere intact instead of nulling it (resolving
    /// slots are still repaired). Set by the pre-copy pass on whole-plugin FO76→FO4
    /// runs, whose interior targets arrive post-copy; the post-copy pass clears it.
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
        let fo4_base_master_index = discover_fo4_base_master_index(
            session.target_slot().parsed.game.as_deref(),
            session.target_masters(),
        );
        let target_id = session.target_id();
        let output_objids = session
            .local_object_ids_in_handle(target_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let output_master_index = master_objids.len() as u32;
        Ok(Self {
            output_objids,
            master_objids,
            fo4_base_master_index,
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
    fn replacement_for(&self, raw: u32, force_output: bool) -> Option<u32> {
        if raw == 0 {
            return None;
        }
        let master_index = raw >> 24;
        let object_id = raw & 0x00FF_FFFF;
        if force_output && self.output_objids.contains(&object_id) {
            let replacement = (self.output_master_index << 24) | object_id;
            return (raw != replacement).then_some(replacement);
        }
        // FO4 exposes engine-intrinsic ActorValues in VMAD bindings even though
        // Fallout4.esm contains no physical AVIF rows for them.
        if self.fo4_base_master_index == Some(master_index)
            && FO4_HARDCODED_AVIF_LOCAL_IDS.contains(&object_id)
        {
            return None;
        }
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

fn discover_fo4_base_master_index(
    target_game: Option<&str>,
    target_masters: &[String],
) -> Option<u32> {
    if target_game != Some("fo4") {
        return None;
    }
    target_masters
        .iter()
        .position(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
        .and_then(|index| u32::try_from(index).ok())
}

pub(crate) fn null_dangling_in_record(record: &mut Record, resolver: &VmadResolver) -> bool {
    let rec_sig = record.sig.0;
    let mut changed = false;
    for entry in record.fields.iter_mut() {
        if entry.sig.as_str() != "VMAD" {
            continue;
        }
        if let FieldValue::Bytes(bytes) = &mut entry.value {
            if null_dangling_in_vmad_blob_for_record(
                bytes.as_mut_slice(),
                resolver,
                &rec_sig,
                Some(record.form_key.local),
            ) {
                changed = true;
            }
        }
    }
    changed
}

#[derive(Clone, Copy)]
struct QustLaneScope {
    quest_object_id: u32,
    lane: QustVmadLane,
    alias_index: Option<i16>,
}

struct QustPropertyScope<'a> {
    lane: QustLaneScope,
    script_name: &'a [u8],
    property_name: &'a [u8],
}

fn is_forced_output_binding(
    scope: Option<&QustPropertyScope<'_>>,
    raw: u32,
    resolver: &VmadResolver,
) -> bool {
    let Some(scope) = scope else {
        return false;
    };
    if resolver.fo4_base_master_index.is_none() {
        return false;
    }
    let target_object_id = raw & 0x00FF_FFFF;
    resolver.output_objids.contains(&target_object_id)
        && QUST_FORCED_OUTPUT_BINDINGS.iter().any(|binding| {
            binding.quest_object_id == scope.lane.quest_object_id
                && binding.lane == scope.lane.lane
                && binding.alias_index == scope.lane.alias_index
                && binding.script_name == scope.script_name
                && binding.property_name == scope.property_name
                && binding.target_object_id == target_object_id
        })
}

/// Walk the VMAD blob, repairing or nulling each dangling object FormID. Mirrors
/// `FormKeyMapper::rewrite_vmad_formids`. Returns whether any slot changed.
/// Aborts (no further change) on a malformed blob.
fn null_dangling_in_vmad_blob(
    data: &mut [u8],
    resolver: &VmadResolver,
    record_sig: &[u8; 4],
) -> bool {
    null_dangling_in_vmad_blob_for_record(data, resolver, record_sig, None)
}

fn null_dangling_in_vmad_blob_for_record(
    data: &mut [u8],
    resolver: &VmadResolver,
    record_sig: &[u8; 4],
    record_object_id: Option<u32>,
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
    let quest_object_id = record_object_id.filter(|record_object_id| {
        record_sig == b"QUST"
            && resolver.fo4_base_master_index.is_some()
            && QUST_FORCED_OUTPUT_BINDINGS
                .iter()
                .any(|binding| binding.quest_object_id == *record_object_id)
    });
    let mut offset = 6usize;
    let mut changed = false;
    for _ in 0..script_count {
        let scope = quest_object_id.map(|quest_object_id| QustLaneScope {
            quest_object_id,
            lane: QustVmadLane::TopLevel,
            alias_index: None,
        });
        if walk_script_entry(
            data,
            &mut offset,
            object_format,
            resolver,
            &mut changed,
            scope,
        )
        .is_none()
        {
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
                    quest_object_id,
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
    walk_script_entry(data, offset, object_format, resolver, changed, None)
}

fn null_dangling_perk_term_after_scripts(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
) -> Option<()> {
    advance(offset, 1, data.len())?; // i8 version
    walk_script_entry(data, offset, object_format, resolver, changed, None)
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
    quest_object_id: Option<u32>,
) -> Option<()> {
    advance(offset, 1, data.len())?; // i8 version
    let fragment_count = read_u16_advance(data, offset)? as usize;

    // Walk fragment script header (same as in formkey_mapper.rs).
    let (script_name, script_name_is_empty) =
        read_vmad_string_if(data, offset, quest_object_id.is_some())?;
    if !script_name_is_empty {
        advance(offset, 1, data.len())?; // u8 flags
        let prop_count = read_u16_advance(data, offset)? as usize;
        for _ in 0..prop_count {
            let (property_name, _) = read_vmad_string_if(data, offset, quest_object_id.is_some())?;
            let prop_type = read_u8_advance(data, offset)?;
            advance(offset, 1, data.len())?;
            let lane = quest_object_id.map(|quest_object_id| QustLaneScope {
                quest_object_id,
                lane: QustVmadLane::Fragment,
                alias_index: None,
            });
            let scope = lane.map(|lane| QustPropertyScope {
                lane,
                script_name: &script_name,
                property_name: &property_name,
            });
            walk_property_value(
                data,
                offset,
                prop_type,
                object_format,
                resolver,
                changed,
                scope.as_ref(),
            )?;
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
        let alias_index = object_alias(data, *offset, object_format)?;
        walk_object(data, offset, object_format, resolver, changed, None)?;
        advance(offset, 2, data.len())?; // i16 version
        let alias_obj_format = read_u16_advance(data, offset)?;
        let alias_script_count = read_u16_advance(data, offset)? as usize;
        for _ in 0..alias_script_count {
            let scope = quest_object_id.map(|quest_object_id| QustLaneScope {
                quest_object_id,
                lane: QustVmadLane::Alias,
                alias_index: Some(alias_index),
            });
            walk_script_entry(data, offset, alias_obj_format, resolver, changed, scope)?;
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
    lane: Option<QustLaneScope>,
) -> Option<()> {
    let (script_name, _) = read_vmad_string_if(data, offset, lane.is_some())?;
    advance(offset, 1, data.len())?;
    let property_count = read_u16_advance(data, offset)? as usize;
    for _ in 0..property_count {
        walk_property_entry(
            data,
            offset,
            object_format,
            resolver,
            changed,
            lane,
            &script_name,
        )?;
    }
    Some(())
}

fn walk_property_entry(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
    lane: Option<QustLaneScope>,
    script_name: &[u8],
) -> Option<()> {
    let (property_name, _) = read_vmad_string_if(data, offset, lane.is_some())?;
    let property_type = read_u8_advance(data, offset)?;
    advance(offset, 1, data.len())?;
    let scope = lane.map(|lane| QustPropertyScope {
        lane,
        script_name,
        property_name: &property_name,
    });
    walk_property_value(
        data,
        offset,
        property_type,
        object_format,
        resolver,
        changed,
        scope.as_ref(),
    )
}

fn walk_property_value(
    data: &mut [u8],
    offset: &mut usize,
    property_type: u8,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
    scope: Option<&QustPropertyScope<'_>>,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => walk_object(data, offset, object_format, resolver, changed, scope),
        2 => {
            skip_vmad_string(data, offset)?;
            Some(())
        }
        3 | 4 => advance(offset, 4, data.len()),
        5 => advance(offset, 1, data.len()),
        7 => walk_struct(data, offset, object_format, resolver, changed, scope),
        11 => {
            let count = read_i32_advance(data, offset)?;
            if count < 0 {
                return None;
            }
            for _ in 0..count {
                walk_object(data, offset, object_format, resolver, changed, scope)?;
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
                walk_struct(data, offset, object_format, resolver, changed, scope)?;
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
    scope: Option<&QustPropertyScope<'_>>,
) -> Option<()> {
    let count = read_i32_advance(data, offset)?;
    if count < 0 {
        return None;
    }
    for _ in 0..count {
        skip_vmad_string(data, offset)?;
        let member_type = read_u8_advance(data, offset)?;
        advance(offset, 1, data.len())?;
        walk_property_value(
            data,
            offset,
            member_type,
            object_format,
            resolver,
            changed,
            scope,
        )?;
    }
    Some(())
}

fn walk_object(
    data: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    resolver: &VmadResolver,
    changed: &mut bool,
    scope: Option<&QustPropertyScope<'_>>,
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
    let force_output = is_forced_output_binding(scope, raw, resolver);
    if let Some(replacement) = resolver.replacement_for(raw, force_output) {
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

fn read_i16(data: &[u8], offset: usize) -> Option<i16> {
    read_u16(data, offset).map(|value| value as i16)
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

fn read_vmad_string_if(data: &[u8], offset: &mut usize, capture: bool) -> Option<(Vec<u8>, bool)> {
    let len = read_u16_advance(data, offset)? as usize;
    let end = offset.checked_add(len)?;
    let bytes = data.get(*offset..end)?;
    let value = if capture { bytes.to_vec() } else { Vec::new() };
    *offset = end;
    Some((value, len == 0))
}

fn object_alias(data: &[u8], offset: usize, object_format: u16) -> Option<i16> {
    let alias_offset = if object_format == 2 {
        offset.checked_add(2)?
    } else {
        offset.checked_add(4)?
    };
    read_i16(data, alias_offset)
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
            fo4_base_master_index: None,
            output_master_index: masters.len() as u32,
            defer_null: false,
        }
    }

    fn resolver_for_target(
        output: &[u32],
        masters: &[&[u32]],
        target_game: Option<&str>,
        target_master_names: &[&str],
    ) -> VmadResolver {
        assert_eq!(masters.len(), target_master_names.len());
        let target_master_names: Vec<String> = target_master_names
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        VmadResolver {
            fo4_base_master_index: discover_fo4_base_master_index(
                target_game,
                &target_master_names,
            ),
            ..resolver(output, masters)
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

    fn push_objfmt1_object(out: &mut Vec<u8>, alias: i16, raw: u32) -> usize {
        let offset = out.len();
        out.extend_from_slice(&raw.to_le_bytes());
        out.extend_from_slice(&alias.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
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

    fn push_named_object_property(
        out: &mut Vec<u8>,
        property_name: &str,
        raw: u32,
        is_array: bool,
    ) -> usize {
        push_string(out, property_name);
        out.push(if is_array { 11 } else { 1 });
        out.push(0);
        if is_array {
            out.extend_from_slice(&1i32.to_le_bytes());
        }
        push_objfmt2_object(out, -1, raw)
    }

    fn push_named_script(
        out: &mut Vec<u8>,
        script_name: &str,
        property_name: &str,
        raw: u32,
        is_array: bool,
    ) -> usize {
        push_string(out, script_name);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        push_named_object_property(out, property_name, raw, is_array)
    }

    fn qust_scoped_vmad_objfmt2(
        lane: QustVmadLane,
        alias_index: Option<i16>,
        script_name: &str,
        property_name: &str,
        raw: u32,
        is_array: bool,
    ) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(
            &(if lane == QustVmadLane::TopLevel {
                1u16
            } else {
                0u16
            })
            .to_le_bytes(),
        );
        if lane == QustVmadLane::TopLevel {
            let offset = push_named_script(&mut out, script_name, property_name, raw, is_array);
            return (out, offset);
        }

        out.push(4);
        out.extend_from_slice(&0u16.to_le_bytes());
        if lane == QustVmadLane::Fragment {
            push_string(&mut out, script_name);
            out.push(0);
            out.extend_from_slice(&1u16.to_le_bytes());
            let offset = push_named_object_property(&mut out, property_name, raw, is_array);
            out.extend_from_slice(&0u16.to_le_bytes());
            return (out, offset);
        }

        push_string(&mut out, "");
        out.extend_from_slice(&1u16.to_le_bytes());
        push_objfmt2_object(&mut out, alias_index.expect("alias lane index"), 0);
        out.extend_from_slice(&6i16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        let offset = push_named_script(&mut out, script_name, property_name, raw, is_array);
        (out, offset)
    }

    fn vmad_objfmt1(props: &[u32]) -> (Vec<u8>, Vec<usize>) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        push_string(&mut out, "Script");
        out.push(0);
        out.extend_from_slice(&(props.len() as u16).to_le_bytes());
        let mut offsets = Vec::new();
        for (i, raw) in props.iter().enumerate() {
            push_string(&mut out, &format!("P{i}"));
            out.push(1);
            out.push(0);
            offsets.push(push_objfmt1_object(&mut out, 0, *raw));
        }
        (out, offsets)
    }

    fn qust_fragment_vmad_objfmt2(props: &[u32]) -> (Vec<u8>, Vec<usize>) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // script count
        out.push(4); // fragment version
        out.extend_from_slice(&0u16.to_le_bytes()); // fragment count
        push_string(&mut out, "Fragments:Quests:Fixture");
        out.push(0); // script flags
        out.extend_from_slice(&(props.len() as u16).to_le_bytes());
        let mut offsets = Vec::new();
        for (i, raw) in props.iter().enumerate() {
            push_string(&mut out, &format!("P{i}"));
            out.push(1); // type = Object
            out.push(0); // status
            offsets.push(push_objfmt2_object(&mut out, -1, *raw));
        }
        out.extend_from_slice(&0u16.to_le_bytes()); // alias count
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
    fn exact_quest_bindings_prefer_output_collisions_and_are_idempotent() {
        for binding in QUST_FORCED_OUTPUT_BINDINGS {
            let is_array = binding.property_name == b"ItemChangedFromReferences";
            let script_name = std::str::from_utf8(binding.script_name).unwrap();
            let property_name = std::str::from_utf8(binding.property_name).unwrap();
            let (mut b, offset) = qust_scoped_vmad_objfmt2(
                binding.lane,
                binding.alias_index,
                script_name,
                property_name,
                binding.target_object_id,
                is_array,
            );
            let r = resolver_for_target(
                &[binding.target_object_id],
                &[&[binding.target_object_id]],
                Some("fo4"),
                &["Fallout4.esm"],
            );

            assert!(
                null_dangling_in_vmad_blob_for_record(
                    &mut b,
                    &r,
                    b"QUST",
                    Some(binding.quest_object_id),
                ),
                "{script_name}.{property_name} should prefer the output collision"
            );
            assert_eq!(raw_at(&b, offset), (1 << 24) | binding.target_object_id);
            assert!(
                !null_dangling_in_vmad_blob_for_record(
                    &mut b,
                    &r,
                    b"QUST",
                    Some(binding.quest_object_id),
                ),
                "{script_name}.{property_name} should be idempotent"
            );
        }
    }

    #[test]
    fn preserves_master_collision_outside_exact_quest_binding_scope() {
        let binding = &QUST_FORCED_OUTPUT_BINDINGS[4];
        let different_object_id = binding.target_object_id + 1;
        let colliding_object_ids = [binding.target_object_id, different_object_id];
        let r = resolver_for_target(
            &colliding_object_ids,
            &[&colliding_object_ids],
            Some("fo4"),
            &["Fallout4.esm"],
        );
        let near_misses = [
            (
                binding.quest_object_id + 1,
                binding.lane,
                binding.alias_index,
                std::str::from_utf8(binding.script_name).unwrap(),
                std::str::from_utf8(binding.property_name).unwrap(),
                binding.target_object_id,
            ),
            (
                binding.quest_object_id,
                QustVmadLane::Fragment,
                None,
                std::str::from_utf8(binding.script_name).unwrap(),
                std::str::from_utf8(binding.property_name).unwrap(),
                binding.target_object_id,
            ),
            (
                binding.quest_object_id,
                binding.lane,
                binding.alias_index,
                "DifferentScript",
                std::str::from_utf8(binding.property_name).unwrap(),
                binding.target_object_id,
            ),
            (
                binding.quest_object_id,
                binding.lane,
                binding.alias_index,
                std::str::from_utf8(binding.script_name).unwrap(),
                "DifferentProperty",
                binding.target_object_id,
            ),
            (
                binding.quest_object_id,
                binding.lane,
                binding.alias_index,
                std::str::from_utf8(binding.script_name).unwrap(),
                std::str::from_utf8(binding.property_name).unwrap(),
                different_object_id,
            ),
        ];

        for (quest_object_id, lane, alias_index, script_name, property_name, raw) in near_misses {
            let (mut b, offset) =
                qust_scoped_vmad_objfmt2(lane, alias_index, script_name, property_name, raw, false);
            assert!(!null_dangling_in_vmad_blob_for_record(
                &mut b,
                &r,
                b"QUST",
                Some(quest_object_id),
            ));
            assert_eq!(raw_at(&b, offset), raw);
        }
    }

    #[test]
    fn alias_collision_requires_exact_alias_index() {
        let binding = QUST_FORCED_OUTPUT_BINDINGS
            .iter()
            .find(|binding| binding.alias_index == Some(22))
            .unwrap();
        let r = resolver_for_target(
            &[binding.target_object_id],
            &[&[binding.target_object_id]],
            Some("fo4"),
            &["Fallout4.esm"],
        );
        let (mut b, offset) = qust_scoped_vmad_objfmt2(
            binding.lane,
            Some(21),
            std::str::from_utf8(binding.script_name).unwrap(),
            std::str::from_utf8(binding.property_name).unwrap(),
            binding.target_object_id,
            true,
        );

        assert!(!null_dangling_in_vmad_blob_for_record(
            &mut b,
            &r,
            b"QUST",
            Some(binding.quest_object_id),
        ));
        assert_eq!(raw_at(&b, offset), binding.target_object_id);
    }

    #[test]
    fn exact_binding_does_not_force_target_absent_from_output() {
        let binding = &QUST_FORCED_OUTPUT_BINDINGS[0];
        let r = resolver_for_target(
            &[],
            &[&[binding.target_object_id]],
            Some("fo4"),
            &["Fallout4.esm"],
        );
        let (mut b, offset) = qust_scoped_vmad_objfmt2(
            binding.lane,
            binding.alias_index,
            std::str::from_utf8(binding.script_name).unwrap(),
            std::str::from_utf8(binding.property_name).unwrap(),
            binding.target_object_id,
            false,
        );

        assert!(!null_dangling_in_vmad_blob_for_record(
            &mut b,
            &r,
            b"QUST",
            Some(binding.quest_object_id),
        ));
        assert_eq!(raw_at(&b, offset), binding.target_object_id);
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

    #[test]
    fn keeps_fo4_intrinsic_aggression_for_actual_master_in_both_object_formats() {
        let r = resolver_for_target(&[0x111111], &[&[]], Some("fo4"), &["Fallout4.esm"]);

        let (mut objfmt1, objfmt1_offsets) = vmad_objfmt1(&[0x0000_02BC]);
        assert!(!null_dangling_in_vmad_blob(&mut objfmt1, &r, b"ACTI"));
        assert_eq!(raw_at(&objfmt1, objfmt1_offsets[0]), 0x0000_02BC);

        let (mut objfmt2, objfmt2_offsets) = qust_fragment_vmad_objfmt2(&[0x0000_02BC]);
        assert!(!null_dangling_in_vmad_blob(&mut objfmt2, &r, b"QUST"));
        assert_eq!(raw_at(&objfmt2, objfmt2_offsets[0]), 0x0000_02BC);
    }

    #[test]
    fn keeps_radical_power_generated_quest_binding() {
        let r = resolver_for_target(&[], &[&[]], Some("fo4"), &["Fallout4.esm"]);
        let (mut vmad, offset) = qust_scoped_vmad_objfmt2(
            QustVmadLane::TopLevel,
            None,
            "W05_002P_Radical_QuestScript",
            "PowerGenerated",
            0x0000_032E,
            false,
        );

        assert!(!null_dangling_in_vmad_blob_for_record(
            &mut vmad,
            &r,
            b"QUST",
            Some(0x0040_F5BE),
        ));
        assert_eq!(raw_at(&vmad, offset), 0x0000_032E);
    }

    #[test]
    fn nulls_fo4_aggression_object_id_for_non_fo4_target() {
        let r = resolver_for_target(&[0x111111], &[&[]], Some("fo76"), &["Fallout4.esm"]);
        let (mut b, offsets) = vmad_objfmt2(&[0x0000_02BC]);

        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offsets[0]), 0);
    }

    #[test]
    fn nulls_fo4_aggression_object_id_at_wrong_master_index() {
        let r = resolver_for_target(
            &[0x111111],
            &[&[], &[]],
            Some("fo4"),
            &["Other.esm", "Fallout4.esm"],
        );
        let (mut b, offsets) = vmad_objfmt2(&[0x0000_02BC]);

        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offsets[0]), 0);
    }

    #[test]
    fn keeps_fo4_aggression_at_nonzero_discovered_master_index() {
        let r = resolver_for_target(
            &[0x111111],
            &[&[], &[]],
            Some("fo4"),
            &["Other.esm", "Fallout4.esm"],
        );
        let (mut b, offsets) = vmad_objfmt2(&[0x0100_02BC]);

        assert!(!null_dangling_in_vmad_blob(&mut b, &r, b"ACTI"));
        assert_eq!(raw_at(&b, offsets[0]), 0x0100_02BC);
    }

    #[test]
    fn nulls_arbitrary_absent_fo4_object_next_to_intrinsic_aggression() {
        let r = resolver_for_target(&[0x111111], &[&[]], Some("fo4"), &["Fallout4.esm"]);
        let (mut b, offsets) = qust_fragment_vmad_objfmt2(&[0x0000_02BC, 0x00FF_FFFE]);

        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"QUST"));
        assert_eq!(raw_at(&b, offsets[0]), 0x0000_02BC);
        assert_eq!(raw_at(&b, offsets[1]), 0);
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
    fn repairs_reported_achr_object_formids() {
        assert!(TOUCHED_RECORD_SIGS.contains(&"ACHR"));
        let r = resolver(&[0x58912C, 0x589134, 0x589136], &seven_masters());
        let (mut b, offs) = vmad_objfmt2(&[0x0058912C, 0x00589134, 0x00589136]);

        assert!(null_dangling_in_vmad_blob(&mut b, &r, b"ACHR"));
        assert_eq!(raw_at(&b, offs[0]), 0x0758912C);
        assert_eq!(raw_at(&b, offs[1]), 0x07589134);
        assert_eq!(raw_at(&b, offs[2]), 0x07589136);
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
