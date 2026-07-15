//! Fixup: synthesize FO4 workshop boundary triggers for FO76 public workshops.
//!
//! FO76 public workshops encode capture/build limits differently from FO4. After
//! converting the public workbench to FO4's `WorkshopWorkbench`, the workbench can
//! still lack `WorkshopLinkSandbox` and any `WorkshopLinkedPrimitive` trigger.
//! FO4 workshop capture then cannot bound the enemy-clear check and reports that
//! enemies remain even when the area is clear.
//!
//! The workshop's location is resolved through the LCSR `WorkshopRefType` entry
//! that claims the workbench (FO76 bakes one per public workshop), NOT through
//! the workbench's parent cell: every FO76 workbench sits in the shared
//! worldspace persistent cell, so a cell-based lookup resolves all workshops to
//! one location and the Boss-ref strip never reaches the real per-workshop
//! LCTNs. Boss special refs are stripped so the location matches the vanilla
//! claim-on-visit shape (RedRocketTruckStopLocation: `LocTypeClearable`, zero
//! Boss LCSR rows).

use esp_authoring_core::plugin_runtime::{
    ParsedRecord, ParsedSubrecord, build_vmad_bytes_from_payload, effective_subrecords_for_record,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::{SmallVec, smallvec};

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
use crate::session::{PluginSession, SessionError};
use crate::sym::{StringInterner, Sym};

const FO4_MASTER_PLUGIN: &str = "Fallout4.esm";
const SYNTHETIC_SANDBOX_SOURCE_PLUGIN: &str = "__fo76_to_fo4_workshop_sandbox__";
const SYNTHETIC_BUILDABLE_SOURCE_PLUGIN: &str = "__fo76_to_fo4_workshop_buildable__";
const SYNTHETIC_LOCATION_SOURCE_PLUGIN: &str = "__fo76_to_fo4_workshop_location__";

const DEFAULT_EMPTY_TRIGGER_LOCAL: u32 = 0x0002_24E3;
const WORKSHOP_WORKBENCH_LOCAL: u32 = 0x000C_1AEB;
const WORKSHOP_LINK_CENTER_LOCAL: u32 = 0x0003_8C0B;
const WORKSHOP_LINK_SANDBOX_LOCAL: u32 = 0x0022_B5A7;
const WORKSHOP_LINKED_PRIMITIVE_LOCAL: u32 = 0x000B_91E6;
const LOC_TYPE_SETTLEMENT_LOCAL: u32 = 0x0002_2611;
const LOC_TYPE_WORKSHOP_LOCAL: u32 = 0x0002_34F1;
const LOC_TYPE_WORKSHOP_SETTLEMENT_LOCAL: u32 = 0x0008_3C9A;
const LOC_TYPE_CLEARABLE_LOCAL: u32 = 0x0006_4EDE;
const LOCATION_CENTER_MARKER_LOCAL: u32 = 0x0001_F40F;
const WORKSHOP_REF_TYPE_LOCAL: u32 = 0x0002_34E9;
const BOSS_REF_TYPE_LOCAL: u32 = 0x0000_3956;

const WORKSHOP_SCRIPT_NAME: &str = "workshopscript";
const WORKSHOP_VMAD_VERSION: u16 = 6;
const WORKSHOP_VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_SCRIPT_FLAG_INHERITED: u8 = 1;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;
const DEFAULT_WORKSHOP_MAX_DRAWS: i32 = 3000;
const DEFAULT_WORKSHOP_MAX_TRIANGLES: i32 = 3_000_000;

const PERSISTENT_GROUP: i32 = 8;
const TEMPORARY_GROUP: i32 = 9;
const DEFAULT_RADIUS: f32 = 4096.0;
const MIN_VERTICAL_HALF_EXTENT: f32 = 2048.0;
const PRIMITIVE_BOX_TYPE: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    position: [f32; 3],
    rotation: [f32; 3],
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BoundaryShape {
    placement: Placement,
    bounds: [f32; 3],
}

#[derive(Clone, Copy)]
struct WorkshopForms {
    trigger_base: FormKey,
    link_center: FormKey,
    link_sandbox: FormKey,
    linked_primitive: FormKey,
    loc_type_settlement: FormKey,
    loc_type_workshop: FormKey,
    loc_type_workshop_settlement: FormKey,
    loc_type_clearable: FormKey,
    location_center_marker: FormKey,
    workshop_ref_type: FormKey,
    boss_ref_type: FormKey,
}

#[derive(Clone, Copy)]
struct RawWorkshopForms {
    workbench_base: u32,
    link_center: u32,
    link_sandbox: u32,
    linked_primitive: u32,
}

#[derive(Clone, Copy)]
struct WorkshopCandidate {
    form_key: FormKey,
    raw_form_id: u32,
    has_sandbox_link: bool,
}

#[derive(Default)]
struct PlannedLocation {
    location_add: Option<Record>,
    location_replace: Option<Record>,
    cell_replace: Option<Record>,
    records_added: u32,
}

pub fn synthesize_workshop_boundaries(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let target_schema = config
        .target_schema
        .clone()
        .ok_or_else(|| FixupError::SchemaError("missing target schema".into()))?;
    let interner = mapper.interner;
    let target_masters = session.target_masters().to_vec();

    let Some(raw_forms) = raw_workshop_forms(&target_masters) else {
        return Ok(report);
    };
    let forms = workshop_forms(interner);
    mapper.reserve_object_ids(
        session
            .local_object_ids_in_handle(session.target_id())
            .map_err(|e| FixupError::HandleError(e.to_string()))?,
    );

    let own_plugin = interner.intern(&session.target_slot().parsed.plugin_name);
    let refr_sig = SigCode::from_str("REFR").map_err(FixupError::SchemaError)?;
    let started = std::time::Instant::now();
    let (candidates, buildable_link_targets) =
        collect_workshop_candidates(session, refr_sig, own_plugin, &raw_forms)?;
    let workbench_locations = if candidates.is_empty() {
        FxHashMap::default()
    } else {
        collect_workbench_locations(
            session,
            target_schema.as_ref(),
            forms.workshop_ref_type,
            interner,
        )?
    };
    eprintln!(
        "[workshop_timing] collect candidates={} elapsed_ms={}",
        candidates.len(),
        started.elapsed().as_millis()
    );

    struct CandidatePlan {
        cell_form_id: u32,
        warn_objid: u32,
        workbench: Record,
        workbench_changed: bool,
        triggers: Option<(Record, Record, FormKey)>,
        location_add: Option<Record>,
        location_replace: Option<Record>,
        cell_replace: Option<Record>,
    }

    // Pass 1 — reads and planning only. Structural inserts invalidate every
    // target index section, so interleaving them with indexed reads (as the old
    // per-candidate loop did) rebuilt the multi-million-record locator once per
    // workshop. All writes are deferred to pass 2.
    let pass_started = std::time::Instant::now();
    let mut plans: Vec<CandidatePlan> = Vec::with_capacity(candidates.len());
    let mut planned_cell_xlcn: FxHashMap<u32, FormKey> = FxHashMap::default();
    for candidate in candidates {
        let Some(cell_form_id) = session
            .parent_cell_form_id_for_record(candidate.raw_form_id)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        else {
            report.warnings.push(interner.intern(&format!(
                "workshop_boundary_missing_parent_cell:{:06X}",
                candidate.form_key.local & 0x00FF_FFFF
            )));
            continue;
        };

        let mut workbench = session
            .record_decoded(&candidate.form_key, target_schema.as_ref(), interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let shape = boundary_shape(
            session,
            &workbench,
            &forms,
            &raw_forms,
            target_schema.as_ref(),
            interner,
        )?;
        let workbench_changed = ensure_workshop_script(&mut workbench);
        let planned = if let Some(location_fk) = workbench_locations.get(&candidate.form_key) {
            plan_strip_boss_refs_at_location(
                session,
                location_fk,
                &forms,
                target_schema.as_ref(),
                interner,
            )?
        } else {
            plan_workshop_location(
                session,
                mapper,
                target_schema.as_ref(),
                cell_form_id,
                &workbench,
                &forms,
                &target_masters,
                own_plugin,
                shape,
                interner,
                &mut planned_cell_xlcn,
            )?
        };
        report.records_added = report.records_added.saturating_add(planned.records_added);

        let triggers = if candidate.has_sandbox_link
            || buildable_link_targets.contains(&candidate.raw_form_id)
        {
            None
        } else {
            let sandbox_edid = synthetic_edid(&workbench, "WorkshopSandboxArea", interner);
            let buildable_edid = synthetic_edid(&workbench, "WorkshopBuildableArea", interner);
            let sandbox_edid_sym = interner.intern(&sandbox_edid);
            let buildable_edid_sym = interner.intern(&buildable_edid);
            let sandbox_fk = mapper.allocate_or_resolve(
                synthetic_source_key(
                    candidate.form_key,
                    SYNTHETIC_SANDBOX_SOURCE_PLUGIN,
                    interner,
                ),
                Some(sandbox_edid_sym),
                refr_sig,
            );
            let buildable_fk = mapper.allocate_or_resolve(
                synthetic_source_key(
                    candidate.form_key,
                    SYNTHETIC_BUILDABLE_SOURCE_PLUGIN,
                    interner,
                ),
                Some(buildable_edid_sym),
                refr_sig,
            );
            let sandbox = build_trigger_record(
                sandbox_fk,
                sandbox_edid_sym,
                forms.trigger_base,
                shape,
                None,
                RecordFlags::PERSISTENT,
                interner,
            );
            let buildable = build_trigger_record(
                buildable_fk,
                buildable_edid_sym,
                forms.trigger_base,
                shape,
                Some((forms.linked_primitive, candidate.form_key)),
                RecordFlags::empty(),
                interner,
            );
            Some((sandbox, buildable, sandbox_fk))
        };
        plans.push(CandidatePlan {
            cell_form_id,
            warn_objid: candidate.form_key.local & 0x00FF_FFFF,
            workbench,
            workbench_changed,
            triggers,
            location_add: planned.location_add,
            location_replace: planned.location_replace,
            cell_replace: planned.cell_replace,
        });
    }
    eprintln!(
        "[workshop_timing] plan plans={} elapsed_ms={}",
        plans.len(),
        pass_started.elapsed().as_millis()
    );

    // Pass 2 — writes only. Content changes go through ONE batched
    // single-traversal replace instead of a full-tree scan per record.
    let pass_started = std::time::Instant::now();
    let mut batch: Vec<Record> = Vec::new();
    for mut plan in plans {
        if let Some(location) = plan.location_add {
            session
                .add_record(location, target_schema.as_ref(), interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
        }
        if let Some(location) = plan.location_replace {
            batch.push(location);
        }
        if let Some(cell) = plan.cell_replace {
            batch.push(cell);
        }
        if let Some((sandbox, buildable, sandbox_fk)) = plan.triggers {
            let sandbox_inserted = session
                .insert_placed_child_into_cell_group(
                    plan.cell_form_id,
                    PERSISTENT_GROUP,
                    sandbox,
                    target_schema.as_ref(),
                    interner,
                )
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            let buildable_inserted = session
                .insert_placed_child_into_cell_group(
                    plan.cell_form_id,
                    TEMPORARY_GROUP,
                    buildable,
                    target_schema.as_ref(),
                    interner,
                )
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if !sandbox_inserted || !buildable_inserted {
                report.warnings.push(interner.intern(&format!(
                    "workshop_boundary_insert_failed:{:06X}",
                    plan.warn_objid
                )));
                continue;
            }
            plan.workbench_changed |= append_link(
                &mut plan.workbench,
                forms.link_sandbox,
                sandbox_fk,
                interner,
            );
            report.records_added = report.records_added.saturating_add(2);
        }
        if plan.workbench_changed {
            batch.push(plan.workbench);
        }
    }
    let insert_elapsed = pass_started.elapsed();
    let replace_started = std::time::Instant::now();
    report.records_changed = report.records_changed.saturating_add(
        session
            .replace_records_contents(batch, target_schema.as_ref(), interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))? as u32,
    );
    eprintln!(
        "[workshop_timing] write insert_ms={} replace_ms={}",
        insert_elapsed.as_millis(),
        replace_started.elapsed().as_millis()
    );

    Ok(report)
}

fn ensure_workshop_script(record: &mut Record) -> bool {
    if record
        .fields
        .iter()
        .any(|entry| entry.sig.as_str() == "VMAD")
    {
        return false;
    }
    let insert_at = record
        .fields
        .iter()
        .position(|entry| entry.sig.as_str() == "EDID")
        .map_or(0, |index| index + 1);
    record.fields.insert(
        insert_at,
        FieldEntry {
            sig: subrecord_sig("VMAD"),
            value: FieldValue::Bytes(SmallVec::from_vec(workshop_script_vmad_bytes())),
        },
    );
    true
}

fn workshop_script_vmad_bytes() -> Vec<u8> {
    let payload = serde_json::json!({
        "Version": WORKSHOP_VMAD_VERSION,
        "Object Format": WORKSHOP_VMAD_OBJECT_FORMAT,
        "Scripts": [{
            "ScriptName": WORKSHOP_SCRIPT_NAME,
            "Flags": VMAD_SCRIPT_FLAG_INHERITED,
            "Properties": [
                vmad_bool_property("EnableAutomaticPlayerOwnership", true),
                vmad_bool_property("AllowAttacksBeforeOwned", false),
                vmad_int_property("MaxDraws", DEFAULT_WORKSHOP_MAX_DRAWS),
                vmad_int_property("MaxTriangles", DEFAULT_WORKSHOP_MAX_TRIANGLES),
                vmad_bool_property("MinRecruitmentProhibitRandom", true),
                vmad_bool_property("AllowUnownedFromLowHappiness", true),
            ],
        }],
    });
    build_vmad_bytes_from_payload(&payload, &[], FO4_MASTER_PLUGIN)
        .expect("hard-coded workshop VMAD payload must encode")
}

fn vmad_bool_property(name: &str, value: bool) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Bool",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": value,
    })
}

fn vmad_int_property(name: &str, value: i32) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Int32",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": value,
    })
}

fn workshop_forms(interner: &StringInterner) -> WorkshopForms {
    WorkshopForms {
        trigger_base: fo4_form_key(DEFAULT_EMPTY_TRIGGER_LOCAL, interner),
        link_center: fo4_form_key(WORKSHOP_LINK_CENTER_LOCAL, interner),
        link_sandbox: fo4_form_key(WORKSHOP_LINK_SANDBOX_LOCAL, interner),
        linked_primitive: fo4_form_key(WORKSHOP_LINKED_PRIMITIVE_LOCAL, interner),
        loc_type_settlement: fo4_form_key(LOC_TYPE_SETTLEMENT_LOCAL, interner),
        loc_type_workshop: fo4_form_key(LOC_TYPE_WORKSHOP_LOCAL, interner),
        loc_type_workshop_settlement: fo4_form_key(LOC_TYPE_WORKSHOP_SETTLEMENT_LOCAL, interner),
        loc_type_clearable: fo4_form_key(LOC_TYPE_CLEARABLE_LOCAL, interner),
        location_center_marker: fo4_form_key(LOCATION_CENTER_MARKER_LOCAL, interner),
        workshop_ref_type: fo4_form_key(WORKSHOP_REF_TYPE_LOCAL, interner),
        boss_ref_type: fo4_form_key(BOSS_REF_TYPE_LOCAL, interner),
    }
}

fn raw_workshop_forms(target_masters: &[String]) -> Option<RawWorkshopForms> {
    Some(RawWorkshopForms {
        workbench_base: encoded_master_form_id(
            target_masters,
            FO4_MASTER_PLUGIN,
            WORKSHOP_WORKBENCH_LOCAL,
        )?,
        link_center: encoded_master_form_id(
            target_masters,
            FO4_MASTER_PLUGIN,
            WORKSHOP_LINK_CENTER_LOCAL,
        )?,
        link_sandbox: encoded_master_form_id(
            target_masters,
            FO4_MASTER_PLUGIN,
            WORKSHOP_LINK_SANDBOX_LOCAL,
        )?,
        linked_primitive: encoded_master_form_id(
            target_masters,
            FO4_MASTER_PLUGIN,
            WORKSHOP_LINKED_PRIMITIVE_LOCAL,
        )?,
    })
}

fn fo4_form_key(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        local,
        plugin: interner.intern(FO4_MASTER_PLUGIN),
    }
}

fn encoded_master_form_id(target_masters: &[String], plugin: &str, local: u32) -> Option<u32> {
    target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin))
        .map(|index| ((index as u32) << 24) | (local & 0x00FF_FFFF))
}

fn target_form_key_from_raw(
    raw_form_id: u32,
    target_masters: &[String],
    own_plugin: Sym,
    interner: &StringInterner,
) -> Option<FormKey> {
    let load_index = (raw_form_id >> 24) as usize;
    let plugin = if load_index < target_masters.len() {
        interner.intern(&target_masters[load_index])
    } else if load_index == target_masters.len() || load_index == 0xFF {
        own_plugin
    } else {
        return None;
    };
    Some(FormKey {
        local: raw_form_id & 0x00FF_FFFF,
        plugin,
    })
}

fn collect_workshop_candidates(
    session: &mut PluginSession,
    refr_sig: SigCode,
    own_plugin: Sym,
    raw_forms: &RawWorkshopForms,
) -> Result<(Vec<WorkshopCandidate>, FxHashSet<u32>), FixupError> {
    use rayon::prelude::*;

    let own_index = session.target_masters().len() as u8;
    let target_id = session.target_id();
    let scan = session
        .handle_raw_scan(target_id)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let refr_form_ids = scan.raw_form_ids_of_sig(refr_sig);
    let hits: Vec<WorkshopScanHit> = refr_form_ids
        .par_iter()
        .filter(|raw_form_id| raw_record_is_own(**raw_form_id, own_index))
        .filter_map(|raw_form_id| {
            scan.with_record_subrecords(*raw_form_id, |subrecords| {
                scan_workshop_refr(*raw_form_id, own_plugin, raw_forms, subrecords)
            })
            .flatten()
        })
        .collect();

    let mut candidates = Vec::new();
    let mut buildable_link_targets = FxHashSet::default();
    for hit in hits {
        if let Some(candidate) = hit.candidate {
            candidates.push(candidate);
        }
        buildable_link_targets.extend(hit.buildable_link_targets);
    }
    Ok((candidates, buildable_link_targets))
}

struct WorkshopScanHit {
    candidate: Option<WorkshopCandidate>,
    buildable_link_targets: SmallVec<[u32; 1]>,
}

fn raw_record_is_own(raw_form_id: u32, own_index: u8) -> bool {
    let index = (raw_form_id >> 24) as u8;
    index == 0xFF || index == own_index
}

fn scan_workshop_refr(
    raw_form_id: u32,
    own_plugin: Sym,
    raw_forms: &RawWorkshopForms,
    subrecords: &[ParsedSubrecord],
) -> Option<WorkshopScanHit> {
    let mut is_workbench = false;
    let mut has_sandbox_link = false;
    let mut buildable_link_targets = SmallVec::new();

    for subrecord in subrecords {
        match subrecord.signature.as_str() {
            "NAME" => {
                is_workbench |=
                    read_raw_form_id(subrecord.data.as_ref(), 0) == Some(raw_forms.workbench_base);
            }
            "XLKR" => {
                let keyword = read_raw_form_id(subrecord.data.as_ref(), 0);
                has_sandbox_link |= keyword == Some(raw_forms.link_sandbox);
                if keyword == Some(raw_forms.linked_primitive) {
                    if let Some(target) = read_raw_form_id(subrecord.data.as_ref(), 4) {
                        buildable_link_targets.push(target);
                    }
                }
            }
            _ => {}
        }
    }

    if !is_workbench && buildable_link_targets.is_empty() {
        return None;
    }
    Some(WorkshopScanHit {
        candidate: is_workbench.then_some(WorkshopCandidate {
            form_key: FormKey {
                local: raw_form_id & 0x00FF_FFFF,
                plugin: own_plugin,
            },
            raw_form_id,
            has_sandbox_link,
        }),
        buildable_link_targets,
    })
}

fn raw_name_form_id(record: &ParsedRecord) -> Option<u32> {
    effective_subrecords_for_record(record)
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "NAME")
        .and_then(|subrecord| read_raw_form_id(subrecord.data.as_ref(), 0))
}

fn read_raw_form_id(buf: &[u8], offset: usize) -> Option<u32> {
    let bytes = buf.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

#[allow(clippy::too_many_arguments)]
fn plan_workshop_location(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &crate::schema::AuthoringSchema,
    cell_form_id: u32,
    workbench: &Record,
    forms: &WorkshopForms,
    target_masters: &[String],
    own_plugin: Sym,
    shape: BoundaryShape,
    interner: &StringInterner,
    planned_cell_xlcn: &mut FxHashMap<u32, FormKey>,
) -> Result<PlannedLocation, FixupError> {
    let mut planned = PlannedLocation::default();
    let cell_fk = target_form_key_from_raw(cell_form_id, target_masters, own_plugin, interner)
        .ok_or_else(|| {
            FixupError::HandleError(format!(
                "invalid parent cell FormID {cell_form_id:08X} for {} target masters",
                target_masters.len()
            ))
        })?;
    let mut cell = session
        .record_decoded(&cell_fk, target_schema, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let stamped_location = form_key_field(&cell, "XLCN");
    // A cell stamped by an earlier candidate in this pass isn't visible in the
    // target yet (writes are deferred); consult the planned stamps so a second
    // workbench in the same cell doesn't synthesize a duplicate location.
    let current_location =
        stamped_location.or_else(|| planned_cell_xlcn.get(&cell_form_id).copied());

    if let Some(location_fk) = current_location {
        if let Some(mut location) =
            decode_target_record_opt(session, &location_fk, target_schema, interner)?
        {
            if location_has_keyword(&location, forms.loc_type_workshop) {
                if strip_workshop_boss_refs(&mut location, forms.boss_ref_type, interner) {
                    planned.location_replace = Some(location);
                }
                return Ok(planned);
            }
        } else if stamped_location.is_none() {
            // Planned-but-not-yet-added location: built workshop-keyworded with
            // no boss refs, so there is nothing to change.
            return Ok(planned);
        }
    }

    let lctn_sig = sig_code("LCTN");
    let location_edid = synthetic_edid(workbench, "WorkshopLocation", interner);
    let location_edid_sym = interner.intern(&location_edid);
    let location_fk = mapper.allocate_or_resolve(
        synthetic_source_key(
            workbench.form_key,
            SYNTHETIC_LOCATION_SOURCE_PLUGIN,
            interner,
        ),
        Some(location_edid_sym),
        lctn_sig,
    );
    let center_fk = linked_ref(workbench, forms.link_center, interner);
    let world_cell = if let Some(location_fk) = current_location {
        decode_target_record_opt(session, &location_fk, target_schema, interner)?
            .and_then(|location| location_world_cell(&location, interner))
    } else {
        None
    };
    let grid = cell_grid(&cell, interner).unwrap_or((0, 0));
    let radius = shape.bounds[0].max(shape.bounds[1]);
    let location = build_workshop_location_record(
        location_fk,
        location_edid_sym,
        current_location,
        workbench.form_key,
        center_fk,
        world_cell,
        grid,
        radius,
        forms,
        interner,
    );

    if decode_target_record_opt(session, &location_fk, target_schema, interner)?.is_some() {
        planned.location_replace = Some(location);
    } else {
        planned.location_add = Some(location);
        planned.records_added = 1;
    }

    // Never stamp XLCN on a grid-less cell: for FO76 sources that is the shared
    // worldspace persistent cell holding every workbench, and a location written
    // there cross-wires all workshops to whichever candidate ran first.
    if cell_grid(&cell, interner).is_some() && set_form_key_field(&mut cell, "XLCN", location_fk) {
        planned_cell_xlcn.insert(cell_form_id, location_fk);
        planned.cell_replace = Some(cell);
    }

    Ok(planned)
}

#[allow(clippy::too_many_arguments)]
fn build_workshop_location_record(
    form_key: FormKey,
    edid: Sym,
    parent_location: Option<FormKey>,
    workbench: FormKey,
    center_marker: Option<FormKey>,
    world_cell: Option<FormKey>,
    grid: (i16, i16),
    radius: f32,
    forms: &WorkshopForms,
    interner: &StringInterner,
) -> Record {
    let mut fields: SmallVec<[FieldEntry; 8]> = smallvec![FieldEntry {
        sig: subrecord_sig("EDID"),
        value: FieldValue::String(edid),
    }];

    if let Some(world_cell) = world_cell {
        let mut rows = Vec::new();
        if let Some(center_marker) = center_marker {
            rows.push(location_special_ref_row(
                forms.location_center_marker,
                center_marker,
                world_cell,
                grid,
                interner,
            ));
        }
        rows.push(location_special_ref_row(
            forms.workshop_ref_type,
            workbench,
            world_cell,
            grid,
            interner,
        ));
        fields.push(FieldEntry {
            sig: subrecord_sig("LCSR"),
            value: FieldValue::List(rows),
        });
    }

    let keywords = vec![
        forms.loc_type_settlement,
        forms.loc_type_workshop,
        forms.loc_type_workshop_settlement,
        forms.loc_type_clearable,
    ];
    fields.push(FieldEntry {
        sig: subrecord_sig("KSIZ"),
        value: FieldValue::Uint(keywords.len() as u64),
    });
    fields.push(FieldEntry {
        sig: subrecord_sig("KWDA"),
        value: FieldValue::List(keywords.into_iter().map(FieldValue::FormKey).collect()),
    });
    if let Some(parent_location) = parent_location {
        fields.push(FieldEntry {
            sig: subrecord_sig("PNAM"),
            value: FieldValue::FormKey(parent_location),
        });
    }
    if let Some(center_marker) = center_marker {
        fields.push(FieldEntry {
            sig: subrecord_sig("MNAM"),
            value: FieldValue::FormKey(center_marker),
        });
    }
    fields.push(FieldEntry {
        sig: subrecord_sig("RNAM"),
        value: FieldValue::Float(radius),
    });
    fields.push(FieldEntry {
        sig: subrecord_sig("ANAM"),
        value: FieldValue::Float(1.0),
    });

    Record {
        sig: sig_code("LCTN"),
        form_key,
        eid: Some(edid),
        flags: RecordFlags::empty(),
        fields,
        warnings: SmallVec::new(),
    }
}

fn location_special_ref_row(
    loc_ref_type: FormKey,
    reference: FormKey,
    world_cell: FormKey,
    grid: (i16, i16),
    interner: &StringInterner,
) -> FieldValue {
    FieldValue::Struct(vec![
        (
            interner.intern("master_special_references_loc_ref_type"),
            FieldValue::FormKey(loc_ref_type),
        ),
        (
            interner.intern("master_special_references_ref"),
            FieldValue::FormKey(reference),
        ),
        (
            interner.intern("master_special_references_world_cell"),
            FieldValue::FormKey(world_cell),
        ),
        (
            interner.intern("master_special_references_grid_y"),
            FieldValue::Int(grid.1 as i64),
        ),
        (
            interner.intern("master_special_references_grid_x"),
            FieldValue::Int(grid.0 as i64),
        ),
    ])
}

fn boundary_shape(
    session: &mut PluginSession,
    workbench: &Record,
    forms: &WorkshopForms,
    raw_forms: &RawWorkshopForms,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> Result<BoundaryShape, FixupError> {
    if let Some(shape) = raw_boundary_shape(session, workbench, raw_forms) {
        return Ok(shape);
    }

    let workbench_placement = read_placement(workbench, interner).unwrap_or_default();
    let Some(center_fk) = linked_ref(workbench, forms.link_center, interner) else {
        return Ok(default_shape(workbench_placement));
    };
    let Some(center) = decode_target_record_opt(session, &center_fk, target_schema, interner)?
    else {
        return Ok(default_shape(workbench_placement));
    };
    let center_placement = read_placement(&center, interner).unwrap_or(workbench_placement);
    let Some(center_base_fk) = record_base(&center) else {
        return Ok(default_shape(center_placement));
    };
    let Some(center_base) =
        decode_target_record_opt(session, &center_base_fk, target_schema, interner)?
    else {
        return Ok(default_shape(center_placement));
    };

    let bounds = bounds_from_center_base(&center_base, interner).unwrap_or([
        DEFAULT_RADIUS,
        DEFAULT_RADIUS,
        MIN_VERTICAL_HALF_EXTENT,
    ]);
    Ok(shape_from_placement(center_placement, bounds))
}

fn raw_boundary_shape(
    session: &mut PluginSession,
    workbench: &Record,
    raw_forms: &RawWorkshopForms,
) -> Option<BoundaryShape> {
    let workbench_raw_id = session.raw_form_id_for_form_key(&workbench.form_key).ok()?;
    let (workbench_placement, center_raw_id) = {
        let workbench_raw = session.record(workbench_raw_id).ok()?;
        (
            raw_record_placement(workbench_raw),
            raw_linked_ref(workbench_raw, raw_forms.link_center),
        )
    };

    let Some(center_raw_id) = center_raw_id else {
        return workbench_placement.map(default_shape);
    };
    let (center_placement, center_base_raw_id) = {
        let center_raw = session.record(center_raw_id).ok()?;
        (
            raw_record_placement(center_raw),
            raw_name_form_id(center_raw),
        )
    };
    let Some(center_placement) = center_placement else {
        return workbench_placement.map(default_shape);
    };
    let bounds = center_base_raw_id
        .and_then(|raw_id| session.record(raw_id).ok())
        .and_then(raw_object_bounds)
        .unwrap_or([DEFAULT_RADIUS, DEFAULT_RADIUS, MIN_VERTICAL_HALF_EXTENT]);
    Some(shape_from_placement(center_placement, bounds))
}

fn raw_record_placement(record: &ParsedRecord) -> Option<Placement> {
    let subrecords = effective_subrecords_for_record(record);
    let data = subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "DATA")?;
    Some(Placement {
        position: [
            read_f32_at(data.data.as_ref(), 0)?,
            read_f32_at(data.data.as_ref(), 4)?,
            read_f32_at(data.data.as_ref(), 8)?,
        ],
        rotation: [
            read_f32_at(data.data.as_ref(), 12)?,
            read_f32_at(data.data.as_ref(), 16)?,
            read_f32_at(data.data.as_ref(), 20)?,
        ],
    })
}

fn raw_linked_ref(record: &ParsedRecord, keyword_raw: u32) -> Option<u32> {
    effective_subrecords_for_record(record)
        .iter()
        .filter(|subrecord| subrecord.signature.as_str() == "XLKR")
        .find_map(|subrecord| {
            (read_raw_form_id(subrecord.data.as_ref(), 0) == Some(keyword_raw))
                .then(|| read_raw_form_id(subrecord.data.as_ref(), 4))
                .flatten()
        })
}

fn raw_object_bounds(record: &ParsedRecord) -> Option<[f32; 3]> {
    let subrecords = effective_subrecords_for_record(record);
    let data = subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "OBND")?;
    let values = [
        read_i16_at(data.data.as_ref(), 0)?,
        read_i16_at(data.data.as_ref(), 2)?,
        read_i16_at(data.data.as_ref(), 4)?,
        read_i16_at(data.data.as_ref(), 6)?,
        read_i16_at(data.data.as_ref(), 8)?,
        read_i16_at(data.data.as_ref(), 10)?,
    ];
    let radius_x = values[0].unsigned_abs().max(values[3].unsigned_abs()) as f32;
    let radius_y = values[1].unsigned_abs().max(values[4].unsigned_abs()) as f32;
    let radius_z = (values[2].unsigned_abs().max(values[5].unsigned_abs()) as f32)
        .max(MIN_VERTICAL_HALF_EXTENT);
    (radius_x > 0.0 && radius_y > 0.0).then_some([radius_x, radius_y, radius_z])
}

fn read_f32_at(bytes: &[u8], offset: usize) -> Option<f32> {
    let value = bytes.get(offset..offset.checked_add(4)?)?;
    Some(f32::from_le_bytes(value.try_into().ok()?))
}

fn read_i16_at(bytes: &[u8], offset: usize) -> Option<i16> {
    let value = bytes.get(offset..offset.checked_add(2)?)?;
    Some(i16::from_le_bytes(value.try_into().ok()?))
}

pub(crate) fn decode_target_record_opt(
    session: &mut PluginSession,
    fk: &FormKey,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> Result<Option<Record>, FixupError> {
    match session.record_decoded(fk, target_schema, interner) {
        Ok(record) => Ok(Some(record)),
        Err(err) if is_missing_target_record_decode(&err) => Ok(None),
        Err(err) => Err(FixupError::HandleError(err.to_string())),
    }
}

fn is_missing_target_record_decode(err: &SessionError) -> bool {
    match err {
        SessionError::RecordNotFound(_) => true,
        SessionError::Other(message) => message.starts_with("record not found:"),
        _ => false,
    }
}

fn default_shape(placement: Placement) -> BoundaryShape {
    shape_from_placement(
        placement,
        [DEFAULT_RADIUS, DEFAULT_RADIUS, MIN_VERTICAL_HALF_EXTENT],
    )
}

fn shape_from_placement(mut placement: Placement, bounds: [f32; 3]) -> BoundaryShape {
    placement.position[2] += bounds[2] * 0.5;
    BoundaryShape { placement, bounds }
}

fn bounds_from_center_base(record: &Record, interner: &StringInterner) -> Option<[f32; 3]> {
    if let Some(bounds) = object_bounds(record, interner) {
        return Some(bounds);
    }
    record
        .eid
        .and_then(|eid| interner.resolve(eid))
        .and_then(parse_max_radius)
        .map(|radius| [radius, radius, MIN_VERTICAL_HALF_EXTENT])
}

fn object_bounds(record: &Record, interner: &StringInterner) -> Option<[f32; 3]> {
    let value = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "OBND")
        .map(|entry| &entry.value)?;
    let x1 = struct_number(value, "object_bounds_x1", interner)?;
    let y1 = struct_number(value, "object_bounds_y1", interner)?;
    let z1 = struct_number(value, "object_bounds_z1", interner).unwrap_or(0.0);
    let x2 = struct_number(value, "object_bounds_x2", interner)?;
    let y2 = struct_number(value, "object_bounds_y2", interner)?;
    let z2 = struct_number(value, "object_bounds_z2", interner).unwrap_or(0.0);
    let radius_x = x1.abs().max(x2.abs());
    let radius_y = y1.abs().max(y2.abs());
    let radius_z = z1.abs().max(z2.abs()).max(MIN_VERTICAL_HALF_EXTENT);
    if radius_x <= 0.0 || radius_y <= 0.0 {
        return None;
    }
    Some([radius_x, radius_y, radius_z])
}

fn parse_max_radius(editor_id: &str) -> Option<f32> {
    let start = editor_id.find("Max")? + 3;
    let digits: String = editor_id[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<f32>().ok().filter(|value| *value > 0.0)
}

fn read_placement(record: &Record, interner: &StringInterner) -> Option<Placement> {
    let value = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "DATA")
        .map(|entry| &entry.value)?;
    Some(Placement {
        position: [
            struct_number(value, "position_rotation_position_x", interner)?,
            struct_number(value, "position_rotation_position_y", interner)?,
            struct_number(value, "position_rotation_position_z", interner)?,
        ],
        rotation: [
            struct_number(value, "position_rotation_rotation_x", interner).unwrap_or(0.0),
            struct_number(value, "position_rotation_rotation_y", interner).unwrap_or(0.0),
            struct_number(value, "position_rotation_rotation_z", interner).unwrap_or(0.0),
        ],
    })
}

fn record_base(record: &Record) -> Option<FormKey> {
    record.fields.iter().find_map(|entry| {
        if entry.sig.as_str() == "NAME" {
            if let FieldValue::FormKey(fk) = entry.value {
                return Some(fk);
            }
        }
        None
    })
}

fn form_key_field(record: &Record, sig: &str) -> Option<FormKey> {
    record.fields.iter().find_map(|entry| {
        if entry.sig.as_str() == sig {
            if let FieldValue::FormKey(fk) = entry.value {
                return Some(fk);
            }
        }
        None
    })
}

fn set_form_key_field(record: &mut Record, sig: &str, target: FormKey) -> bool {
    for entry in &mut record.fields {
        if entry.sig.as_str() != sig {
            continue;
        }
        if entry.value == FieldValue::FormKey(target) {
            return false;
        }
        entry.value = FieldValue::FormKey(target);
        return true;
    }
    record.fields.push(FieldEntry {
        sig: subrecord_sig(sig),
        value: FieldValue::FormKey(target),
    });
    true
}

fn location_has_keyword(record: &Record, keyword: FormKey) -> bool {
    record.fields.iter().any(|entry| {
        entry.sig.as_str() == "KWDA"
            && matches!(
                &entry.value,
                FieldValue::List(items) if items.contains(&FieldValue::FormKey(keyword))
            )
    })
}

fn collect_workbench_locations(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    workshop_ref_type: FormKey,
    interner: &StringInterner,
) -> Result<FxHashMap<FormKey, FormKey>, FixupError> {
    let lctn_sig = sig_code("LCTN");
    let lctn_fks = session
        .form_keys_of_sig(lctn_sig, interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let mut map = FxHashMap::default();
    for lctn_fk in lctn_fks {
        let Some(location) = decode_target_record_opt(session, &lctn_fk, target_schema, interner)?
        else {
            continue;
        };
        for workbench_fk in workshop_special_refs(&location, workshop_ref_type, interner) {
            map.entry(workbench_fk).or_insert(lctn_fk);
        }
    }
    Ok(map)
}

fn workshop_special_refs(
    record: &Record,
    workshop_ref_type: FormKey,
    interner: &StringInterner,
) -> SmallVec<[FormKey; 2]> {
    let mut out = SmallVec::new();
    for entry in &record.fields {
        if entry.sig.as_str() != "LCSR" {
            continue;
        }
        let FieldValue::List(rows) = &entry.value else {
            continue;
        };
        for row in rows {
            if struct_form_key(row, "master_special_references_loc_ref_type", interner)
                == Some(workshop_ref_type)
            {
                if let Some(fk) = struct_form_key(row, "master_special_references_ref", interner) {
                    out.push(fk);
                }
            }
        }
    }
    out
}

fn plan_strip_boss_refs_at_location(
    session: &mut PluginSession,
    location_fk: &FormKey,
    forms: &WorkshopForms,
    target_schema: &crate::schema::AuthoringSchema,
    interner: &StringInterner,
) -> Result<PlannedLocation, FixupError> {
    let mut planned = PlannedLocation::default();
    let Some(mut location) =
        decode_target_record_opt(session, location_fk, target_schema, interner)?
    else {
        return Ok(planned);
    };
    let mut changed = strip_workshop_boss_refs(&mut location, forms.boss_ref_type, interner);
    changed |= ensure_workshop_location_keywords(&mut location, forms);
    if changed {
        planned.location_replace = Some(location);
    }
    Ok(planned)
}

/// Ensure the FO4 workshop keyword set on a resolved workshop location. Most
/// converted locations already carry them, but some (e.g. Wavy Willards) miss
/// the synthesis path and would break keyword-conditioned registration.
fn ensure_workshop_location_keywords(record: &mut Record, forms: &WorkshopForms) -> bool {
    let required = [
        forms.loc_type_settlement,
        forms.loc_type_workshop,
        forms.loc_type_workshop_settlement,
        forms.loc_type_clearable,
    ];
    let mut changed = false;
    let kwda_index = record
        .fields
        .iter()
        .position(|entry| entry.sig.as_str() == "KWDA");
    let keyword_count;
    if let Some(kwda_index) = kwda_index {
        let FieldValue::List(items) = &mut record.fields[kwda_index].value else {
            return false;
        };
        for keyword in required {
            if !items.contains(&FieldValue::FormKey(keyword)) {
                items.push(FieldValue::FormKey(keyword));
                changed = true;
            }
        }
        keyword_count = items.len() as u64;
    } else {
        let insert_at = record
            .fields
            .iter()
            .position(|entry| matches!(entry.sig.as_str(), "PNAM" | "RNAM" | "ANAM"))
            .unwrap_or(record.fields.len());
        record.fields.insert(
            insert_at,
            FieldEntry {
                sig: subrecord_sig("KWDA"),
                value: FieldValue::List(
                    required.iter().copied().map(FieldValue::FormKey).collect(),
                ),
            },
        );
        keyword_count = required.len() as u64;
        changed = true;
    }
    if changed {
        let kwda_index = record
            .fields
            .iter()
            .position(|entry| entry.sig.as_str() == "KWDA")
            .expect("KWDA present after ensure");
        if let Some(ksiz) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.as_str() == "KSIZ")
        {
            ksiz.value = FieldValue::Uint(keyword_count);
        } else {
            record.fields.insert(
                kwda_index,
                FieldEntry {
                    sig: subrecord_sig("KSIZ"),
                    value: FieldValue::Uint(keyword_count),
                },
            );
        }
    }
    changed
}

fn strip_workshop_boss_refs(
    record: &mut Record,
    boss_ref_type: FormKey,
    interner: &StringInterner,
) -> bool {
    let mut changed = false;
    for entry in &mut record.fields {
        if entry.sig.as_str() != "LCSR" {
            continue;
        }
        let FieldValue::List(rows) = &mut entry.value else {
            continue;
        };
        let original_len = rows.len();
        rows.retain(|row| {
            struct_form_key(row, "master_special_references_loc_ref_type", interner)
                != Some(boss_ref_type)
        });
        changed |= rows.len() != original_len;
    }
    changed
}

fn location_world_cell(record: &Record, interner: &StringInterner) -> Option<FormKey> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "LCEC")
        .and_then(|entry| struct_form_key(&entry.value, "world", interner))
}

fn cell_grid(record: &Record, interner: &StringInterner) -> Option<(i16, i16)> {
    let value = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "XCLC")
        .map(|entry| &entry.value)?;
    let x = struct_number(value, "grid_x", interner)? as i16;
    let y = struct_number(value, "grid_y", interner)? as i16;
    Some((x, y))
}

fn linked_ref(record: &Record, keyword: FormKey, interner: &StringInterner) -> Option<FormKey> {
    record.fields.iter().find_map(|entry| {
        if entry.sig.as_str() != "XLKR" {
            return None;
        }
        let FieldValue::Struct(fields) = &entry.value else {
            return None;
        };
        let mut row_keyword = None;
        let mut row_ref = None;
        for (name, value) in fields {
            if field_name_matches(*name, "keyword_ref", interner) {
                if let FieldValue::FormKey(fk) = value {
                    row_keyword = Some(*fk);
                }
            } else if field_name_matches(*name, "ref", interner) {
                if let FieldValue::FormKey(fk) = value {
                    row_ref = Some(*fk);
                }
            }
        }
        (row_keyword == Some(keyword)).then_some(row_ref).flatten()
    })
}

fn append_link(
    record: &mut Record,
    keyword: FormKey,
    target: FormKey,
    interner: &StringInterner,
) -> bool {
    if linked_ref(record, keyword, interner) == Some(target) {
        return false;
    }
    record.fields.push(FieldEntry {
        sig: subrecord_sig("XLKR"),
        value: FieldValue::Struct(vec![
            (interner.intern("keyword_ref"), FieldValue::FormKey(keyword)),
            (interner.intern("ref"), FieldValue::FormKey(target)),
        ]),
    });
    true
}

fn struct_number(value: &FieldValue, name: &str, interner: &StringInterner) -> Option<f32> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    fields
        .iter()
        .find(|(field_name, _)| field_name_matches(*field_name, name, interner))
        .and_then(|(_, value)| number_value(value))
}

pub(crate) fn struct_form_key(
    value: &FieldValue,
    name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    let FieldValue::Struct(fields) = value else {
        return None;
    };
    fields
        .iter()
        .find(|(field_name, _)| field_name_matches(*field_name, name, interner))
        .and_then(|(_, value)| {
            if let FieldValue::FormKey(fk) = value {
                Some(*fk)
            } else {
                None
            }
        })
}

fn field_name_matches(field_name: Sym, expected: &str, interner: &StringInterner) -> bool {
    interner
        .resolve(field_name)
        .is_some_and(|actual| normalized_field_name(actual) == normalized_field_name(expected))
}

fn normalized_field_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn number_value(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Uint(value) => Some(*value as f32),
        _ => None,
    }
}

fn build_trigger_record(
    form_key: FormKey,
    edid: Sym,
    base: FormKey,
    shape: BoundaryShape,
    link: Option<(FormKey, FormKey)>,
    flags: RecordFlags,
    interner: &StringInterner,
) -> Record {
    let mut fields: SmallVec<[FieldEntry; 8]> = smallvec![
        FieldEntry {
            sig: subrecord_sig("EDID"),
            value: FieldValue::String(edid),
        },
        FieldEntry {
            sig: subrecord_sig("NAME"),
            value: FieldValue::FormKey(base),
        },
    ];
    if let Some((keyword, target)) = link {
        fields.push(FieldEntry {
            sig: subrecord_sig("XLKR"),
            value: FieldValue::Struct(vec![
                (interner.intern("keyword_ref"), FieldValue::FormKey(keyword)),
                (interner.intern("ref"), FieldValue::FormKey(target)),
            ]),
        });
    }
    fields.push(FieldEntry {
        sig: subrecord_sig("XPRM"),
        value: FieldValue::Struct(vec![
            (
                interner.intern("bounds_x"),
                FieldValue::Float(shape.bounds[0]),
            ),
            (
                interner.intern("bounds_y"),
                FieldValue::Float(shape.bounds[1]),
            ),
            (
                interner.intern("bounds_z"),
                FieldValue::Float(shape.bounds[2]),
            ),
            (interner.intern("color_red"), FieldValue::Float(0.8)),
            (
                interner.intern("color_green"),
                FieldValue::Float(0.298_039_23),
            ),
            (interner.intern("color_blue"), FieldValue::Float(0.2)),
            (interner.intern("unknown"), FieldValue::Float(0.3)),
            (
                interner.intern("type"),
                FieldValue::Uint(PRIMITIVE_BOX_TYPE as u64),
            ),
        ]),
    });
    fields.push(FieldEntry {
        sig: subrecord_sig("DATA"),
        value: FieldValue::Struct(vec![
            (
                interner.intern("position_rotation_position_x"),
                FieldValue::Float(shape.placement.position[0]),
            ),
            (
                interner.intern("position_rotation_position_y"),
                FieldValue::Float(shape.placement.position[1]),
            ),
            (
                interner.intern("position_rotation_position_z"),
                FieldValue::Float(shape.placement.position[2]),
            ),
            (
                interner.intern("position_rotation_rotation_x"),
                FieldValue::Float(shape.placement.rotation[0]),
            ),
            (
                interner.intern("position_rotation_rotation_y"),
                FieldValue::Float(shape.placement.rotation[1]),
            ),
            (
                interner.intern("position_rotation_rotation_z"),
                FieldValue::Float(shape.placement.rotation[2]),
            ),
        ]),
    });

    Record {
        sig: sig_code("REFR"),
        form_key,
        eid: Some(edid),
        flags,
        fields,
        warnings: SmallVec::new(),
    }
}

fn synthetic_edid(workbench: &Record, suffix: &str, interner: &StringInterner) -> String {
    let base = workbench
        .eid
        .and_then(|eid| interner.resolve(eid))
        .map(str::to_string)
        .unwrap_or_else(|| format!("Workshop{:06X}", workbench.form_key.local & 0x00FF_FFFF));
    format!("{base}_{suffix}")
}

fn synthetic_source_key(workbench: FormKey, plugin: &str, interner: &StringInterner) -> FormKey {
    FormKey {
        local: 1,
        plugin: interner.intern(&format!("{plugin}_{:06X}", workbench.local & 0x00FF_FFFF)),
    }
}

fn sig_code(sig: &str) -> SigCode {
    SigCode::from_str(sig).expect("hard-coded signature must be valid")
}

fn subrecord_sig(sig: &str) -> SubrecordSig {
    SubrecordSig::from_str(sig).expect("hard-coded subrecord signature must be valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FIRST_ALLOCATION_ID, MapperOptions, MapperState};
    use bytes::Bytes;

    fn fk(local: u32, plugin: Sym) -> FormKey {
        FormKey { local, plugin }
    }

    fn parsed_subrecord(sig: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: sig.into(),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn parsed_record(sig: &str, form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        ParsedRecord {
            signature: sig.into(),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        }
    }

    fn placement_bytes(position: [f32; 3], rotation: [f32; 3]) -> Vec<u8> {
        position
            .into_iter()
            .chain(rotation)
            .flat_map(f32::to_le_bytes)
            .collect()
    }

    fn record_with_obnd(interner: &StringInterner, values: [i64; 6]) -> Record {
        let [x1, y1, z1, x2, y2, z2] = values;
        Record {
            sig: sig_code("STAT"),
            form_key: fk(0x800, interner.intern("Test.esp")),
            eid: Some(interner.intern("SpawnCenter_Min2048_Max4096")),
            flags: RecordFlags::empty(),
            fields: smallvec![FieldEntry {
                sig: subrecord_sig("OBND"),
                value: FieldValue::Struct(vec![
                    (interner.intern("object_bounds_x1"), FieldValue::Int(x1)),
                    (interner.intern("ObjectBoundsY1"), FieldValue::Int(y1)),
                    (interner.intern("ObjectBoundsZ1"), FieldValue::Int(z1)),
                    (interner.intern("ObjectBoundsX2"), FieldValue::Int(x2)),
                    (interner.intern("ObjectBoundsY2"), FieldValue::Int(y2)),
                    (interner.intern("ObjectBoundsZ2"), FieldValue::Int(z2)),
                ]),
            }],
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn object_bounds_drive_workshop_radius() {
        let interner = StringInterner::new();
        let record = record_with_obnd(&interner, [-5112, -5112, -64, 5112, 5112, 160]);

        assert_eq!(
            bounds_from_center_base(&record, &interner),
            Some([5112.0, 5112.0, MIN_VERTICAL_HALF_EXTENT])
        );
    }

    #[test]
    fn raw_records_preserve_workshop_center_and_bounds() {
        let center_keyword: u32 = 0x0003_8C0B;
        let center_ref: u32 = 0x0708_8C18;
        let center_base: u32 = 0x0743_CF11;
        let mut link = Vec::new();
        link.extend_from_slice(&center_keyword.to_le_bytes());
        link.extend_from_slice(&center_ref.to_le_bytes());
        let workbench = parsed_record(
            "REFR",
            0x0708_8ACF,
            vec![
                parsed_subrecord("XLKR", link),
                parsed_subrecord(
                    "DATA",
                    placement_bytes([-156_886.28, 137_004.39, 574.60], [0.0, 0.0, 2.90]),
                ),
            ],
        );
        let center = parsed_record(
            "REFR",
            center_ref,
            vec![
                parsed_subrecord("NAME", center_base.to_le_bytes().to_vec()),
                parsed_subrecord(
                    "DATA",
                    placement_bytes([-157_385.38, 138_592.11, 584.0], [0.0; 3]),
                ),
            ],
        );
        let mut obnd = Vec::new();
        for value in [-8200_i16, -8200, 0, 8200, 8200, 160] {
            obnd.extend_from_slice(&value.to_le_bytes());
        }
        let center_base_record =
            parsed_record("STAT", center_base, vec![parsed_subrecord("OBND", obnd)]);

        assert_eq!(raw_linked_ref(&workbench, center_keyword), Some(center_ref));
        assert_eq!(raw_name_form_id(&center), Some(center_base));
        let shape = shape_from_placement(
            raw_record_placement(&center).expect("center placement"),
            raw_object_bounds(&center_base_record).expect("center bounds"),
        );
        assert_eq!(shape.placement.position, [-157_385.38, 138_592.11, 1608.0]);
        assert_eq!(shape.bounds, [8200.0, 8200.0, MIN_VERTICAL_HALF_EXTENT]);
    }

    #[test]
    fn raw_workshop_scan_collects_candidate_and_buildable_target_in_one_pass() {
        let interner = StringInterner::new();
        let own_plugin = interner.intern("SeventySix.esm");
        let raw_forms = RawWorkshopForms {
            workbench_base: 0x000C_1AEB,
            link_center: 0x0003_8C0B,
            link_sandbox: 0x0022_B5A7,
            linked_primitive: 0x000B_91E6,
        };
        let raw_form_id = 0x0708_8ACF;
        let buildable_target: u32 = 0x0708_8AD0;
        let mut sandbox_link = Vec::new();
        sandbox_link.extend_from_slice(&raw_forms.link_sandbox.to_le_bytes());
        sandbox_link.extend_from_slice(&0x0708_8AD1_u32.to_le_bytes());
        let mut buildable_link = Vec::new();
        buildable_link.extend_from_slice(&raw_forms.linked_primitive.to_le_bytes());
        buildable_link.extend_from_slice(&buildable_target.to_le_bytes());
        let subrecords = vec![
            parsed_subrecord("NAME", raw_forms.workbench_base.to_le_bytes().to_vec()),
            parsed_subrecord("XLKR", sandbox_link),
            parsed_subrecord("XLKR", buildable_link),
        ];

        let hit = scan_workshop_refr(raw_form_id, own_plugin, &raw_forms, &subrecords)
            .expect("workshop scan hit");
        let candidate = hit.candidate.expect("workshop candidate");
        assert_eq!(candidate.form_key, fk(0x088ACF, own_plugin));
        assert_eq!(candidate.raw_form_id, raw_form_id);
        assert!(candidate.has_sandbox_link);
        assert_eq!(hit.buildable_link_targets.as_slice(), &[buildable_target]);
    }

    #[test]
    fn raw_record_ownership_accepts_local_and_own_index_forms() {
        assert!(raw_record_is_own(0xFF00_0800, 7));
        assert!(raw_record_is_own(0x0700_0800, 7));
        assert!(!raw_record_is_own(0x0000_0800, 7));
    }

    #[test]
    fn parent_cell_form_key_preserves_master_identity() {
        let interner = StringInterner::new();
        let own_plugin = interner.intern("SeventySix.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let masters = vec!["Fallout4.esm".to_string(), "DLCRobot.esm".to_string()];

        assert_eq!(
            target_form_key_from_raw(0x0001_B2B6, &masters, own_plugin, &interner),
            Some(fk(0x01B2B6, fallout4))
        );
        assert_eq!(
            target_form_key_from_raw(0x0200_0800, &masters, own_plugin, &interner),
            Some(fk(0x000800, own_plugin))
        );
        assert_eq!(
            target_form_key_from_raw(0xFF00_0800, &masters, own_plugin, &interner),
            Some(fk(0x000800, own_plugin))
        );
        assert_eq!(
            target_form_key_from_raw(0x0300_0800, &masters, own_plugin, &interner),
            None
        );
    }

    #[test]
    fn workshop_locations_drop_converted_boss_special_refs() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let forms = workshop_forms(&interner);
        let world = fk(0x25DA15, plugin);
        let mut location = Record {
            sig: sig_code("LCTN"),
            form_key: fk(0x2DE57D, plugin),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![FieldEntry {
                sig: subrecord_sig("LCSR"),
                value: FieldValue::List(vec![
                    location_special_ref_row(
                        forms.boss_ref_type,
                        fk(0x29C2DA, plugin),
                        world,
                        (-39, 33),
                        &interner,
                    ),
                    location_special_ref_row(
                        forms.boss_ref_type,
                        fk(0x29C2D7, plugin),
                        world,
                        (-39, 33),
                        &interner,
                    ),
                    location_special_ref_row(
                        forms.workshop_ref_type,
                        fk(0x088ACF, plugin),
                        world,
                        (-39, 33),
                        &interner,
                    ),
                ]),
            }],
            warnings: SmallVec::new(),
        };

        assert!(strip_workshop_boss_refs(
            &mut location,
            forms.boss_ref_type,
            &interner
        ));
        let FieldValue::List(rows) = &location.fields[0].value else {
            panic!("LCSR rows");
        };
        assert_eq!(rows.len(), 1);
        assert_eq!(
            struct_form_key(
                &rows[0],
                "master_special_references_loc_ref_type",
                &interner
            ),
            Some(forms.workshop_ref_type)
        );
        assert!(!strip_workshop_boss_refs(
            &mut location,
            forms.boss_ref_type,
            &interner
        ));
    }

    #[test]
    fn ensure_workshop_location_keywords_fills_missing_set() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let forms = workshop_forms(&interner);
        let mut location = Record {
            sig: sig_code("LCTN"),
            form_key: fk(0x0B23D7, plugin),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![
                FieldEntry {
                    sig: subrecord_sig("KSIZ"),
                    value: FieldValue::Uint(1),
                },
                FieldEntry {
                    sig: subrecord_sig("KWDA"),
                    value: FieldValue::List(vec![FieldValue::FormKey(forms.loc_type_clearable)]),
                },
                FieldEntry {
                    sig: subrecord_sig("ANAM"),
                    value: FieldValue::Float(1.0),
                },
            ],
            warnings: SmallVec::new(),
        };

        assert!(ensure_workshop_location_keywords(&mut location, &forms));
        let FieldValue::List(items) = &location.fields[1].value else {
            panic!("KWDA list");
        };
        assert_eq!(items.len(), 4);
        assert!(matches!(
            location.fields[0].value,
            FieldValue::Uint(count) if count == 4
        ));
        assert!(!ensure_workshop_location_keywords(&mut location, &forms));
    }

    #[test]
    fn workshop_special_refs_finds_workbench_claims() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let forms = workshop_forms(&interner);
        let world = fk(0x25DA15, plugin);
        let location = Record {
            sig: sig_code("LCTN"),
            form_key: fk(0x2DE57D, plugin),
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec![FieldEntry {
                sig: subrecord_sig("LCSR"),
                value: FieldValue::List(vec![
                    location_special_ref_row(
                        forms.boss_ref_type,
                        fk(0x29C2DA, plugin),
                        world,
                        (-39, 33),
                        &interner,
                    ),
                    location_special_ref_row(
                        forms.workshop_ref_type,
                        fk(0x088ACF, plugin),
                        world,
                        (-39, 33),
                        &interner,
                    ),
                ]),
            }],
            warnings: SmallVec::new(),
        };

        let refs = workshop_special_refs(&location, forms.workshop_ref_type, &interner);
        assert_eq!(refs.as_slice(), &[fk(0x088ACF, plugin)]);
        assert!(
            workshop_special_refs(&location, forms.boss_ref_type, &interner).as_slice()
                == &[fk(0x29C2DA, plugin)]
        );
    }

    #[test]
    fn editor_id_max_radius_is_fallback() {
        assert_eq!(
            parse_max_radius("SpawnCenter_Min2048_Max4096"),
            Some(4096.0)
        );
    }

    #[test]
    fn build_trigger_record_uses_box_primitive_and_offset_z() {
        let interner = StringInterner::new();
        let plugin = interner.intern("Test.esp");
        let shape = shape_from_placement(
            Placement {
                position: [10.0, 20.0, 30.0],
                rotation: [0.1, 0.2, 0.3],
            },
            [100.0, 200.0, 300.0],
        );
        let record = build_trigger_record(
            fk(0x900, plugin),
            interner.intern("Boundary"),
            fo4_form_key(DEFAULT_EMPTY_TRIGGER_LOCAL, &interner),
            shape,
            Some((
                fo4_form_key(WORKSHOP_LINKED_PRIMITIVE_LOCAL, &interner),
                fk(0x800, plugin),
            )),
            RecordFlags::PERSISTENT,
            &interner,
        );

        let xprm = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "XPRM")
            .expect("XPRM");
        assert_eq!(
            struct_number(&xprm.value, "type", &interner),
            Some(PRIMITIVE_BOX_TYPE as f32)
        );
        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA");
        assert_eq!(
            struct_number(&data.value, "position_rotation_position_z", &interner),
            Some(180.0)
        );
        assert!(record.flags.contains(RecordFlags::PERSISTENT));
    }

    #[test]
    fn synthetic_source_keys_force_generated_floor_allocation() {
        let interner = StringInterner::new();
        let output_plugin = interner.intern("SeventySix.esm");
        let workbench = fk(0x20106D, output_plugin);
        let source = synthetic_source_key(workbench, SYNTHETIC_SANDBOX_SOURCE_PLUGIN, &interner);
        assert!(source.local < FIRST_ALLOCATION_ID);

        let refr_sig = sig_code("REFR");
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                preserve_source_ids: true,
                generated_object_id_floor: 0x00A0_0000,
                ..MapperOptions::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let target = mapper.allocate_or_resolve(source, None, refr_sig);

        assert_eq!(target.local, 0x00A0_0000);
        assert_eq!(target.plugin, output_plugin);
    }

    #[test]
    fn build_workshop_location_record_uses_fo4_workshop_location_pattern() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let forms = workshop_forms(&interner);
        let parent = fk(0x2F70F9, plugin);
        let workbench = fk(0x20106D, plugin);
        let center = fk(0x20106E, plugin);
        let world = fk(0x25DA15, plugin);
        let record = build_workshop_location_record(
            fk(0xA00000, plugin),
            interner.intern("BeckleyWorkshopRef_WorkshopLocation"),
            Some(parent),
            workbench,
            Some(center),
            Some(world),
            (7, 5),
            5112.0,
            &forms,
            &interner,
        );

        let keywords = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "KWDA")
            .expect("KWDA");
        let FieldValue::List(items) = &keywords.value else {
            panic!("KWDA should be a list");
        };
        assert!(items.contains(&FieldValue::FormKey(forms.loc_type_settlement)));
        assert!(items.contains(&FieldValue::FormKey(forms.loc_type_workshop)));
        assert!(items.contains(&FieldValue::FormKey(forms.loc_type_workshop_settlement)));
        assert!(items.contains(&FieldValue::FormKey(forms.loc_type_clearable)));
        assert_eq!(form_key_field(&record, "PNAM"), Some(parent));
        assert_eq!(form_key_field(&record, "MNAM"), Some(center));

        let lcsr = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LCSR")
            .expect("LCSR");
        let FieldValue::List(rows) = &lcsr.value else {
            panic!("LCSR should be a row list");
        };
        assert_eq!(rows.len(), 2);
        assert_eq!(
            struct_form_key(
                &rows[0],
                "master_special_references_loc_ref_type",
                &interner
            ),
            Some(forms.location_center_marker)
        );
        assert_eq!(
            struct_form_key(&rows[0], "master_special_references_ref", &interner),
            Some(center)
        );
        assert_eq!(
            struct_form_key(
                &rows[1],
                "master_special_references_loc_ref_type",
                &interner
            ),
            Some(forms.workshop_ref_type)
        );
        assert_eq!(
            struct_form_key(&rows[1], "master_special_references_ref", &interner),
            Some(workbench)
        );
        assert_eq!(
            struct_number(&rows[1], "master_special_references_grid_x", &interner),
            Some(7.0)
        );
        assert_eq!(
            struct_number(&rows[1], "master_special_references_grid_y", &interner),
            Some(5.0)
        );
    }

    #[derive(Debug, PartialEq)]
    struct TestVmadProperty {
        name: String,
        property_type: u8,
        flags: u8,
        value: TestVmadValue,
    }

    #[derive(Debug, PartialEq)]
    enum TestVmadValue {
        Bool(bool),
        Int(i32),
    }

    fn read_test_vmad_string(bytes: &[u8], offset: &mut usize) -> String {
        let len = u16::from_le_bytes(bytes[*offset..*offset + 2].try_into().unwrap()) as usize;
        *offset += 2;
        let value = std::str::from_utf8(&bytes[*offset..*offset + len])
            .unwrap()
            .to_string();
        *offset += len;
        value
    }

    fn read_test_workshop_vmad(bytes: &[u8]) -> (String, u8, Vec<TestVmadProperty>) {
        let mut offset = 0;
        let version = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        offset += 2;
        let object_format = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        offset += 2;
        let script_count = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        offset += 2;
        assert_eq!(version, WORKSHOP_VMAD_VERSION);
        assert_eq!(object_format, WORKSHOP_VMAD_OBJECT_FORMAT);
        assert_eq!(script_count, 1);

        let script_name = read_test_vmad_string(bytes, &mut offset);
        let script_flags = bytes[offset];
        offset += 1;
        let property_count = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        offset += 2;

        let mut properties = Vec::new();
        for _ in 0..property_count {
            let name = read_test_vmad_string(bytes, &mut offset);
            let property_type = bytes[offset];
            offset += 1;
            let flags = bytes[offset];
            offset += 1;
            let value = match property_type {
                3 => {
                    let value = i32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                    offset += 4;
                    TestVmadValue::Int(value)
                }
                5 => {
                    let value = bytes[offset] != 0;
                    offset += 1;
                    TestVmadValue::Bool(value)
                }
                other => panic!("unexpected test VMAD property type {other}"),
            };
            properties.push(TestVmadProperty {
                name,
                property_type,
                flags,
                value,
            });
        }

        assert_eq!(offset, bytes.len());
        (script_name, script_flags, properties)
    }

    #[test]
    fn ensure_workshop_script_adds_inherited_workshopscript_vmad() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let mut record = Record {
            sig: sig_code("REFR"),
            form_key: fk(0x20106D, plugin),
            eid: Some(interner.intern("BeckleyWorkshopRef")),
            flags: RecordFlags::PERSISTENT,
            fields: smallvec![FieldEntry {
                sig: subrecord_sig("NAME"),
                value: FieldValue::FormKey(fo4_form_key(WORKSHOP_WORKBENCH_LOCAL, &interner)),
            }],
            warnings: SmallVec::new(),
        };

        assert!(ensure_workshop_script(&mut record));
        let vmad = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "VMAD")
            .expect("VMAD");
        let FieldValue::Bytes(bytes) = &vmad.value else {
            panic!("VMAD should be raw bytes");
        };
        let (script_name, script_flags, properties) = read_test_workshop_vmad(bytes);

        assert_eq!(script_name, WORKSHOP_SCRIPT_NAME);
        assert_eq!(script_flags, VMAD_SCRIPT_FLAG_INHERITED);
        assert_eq!(properties.len(), 6);
        assert!(
            properties
                .iter()
                .all(|property| property.flags == VMAD_PROPERTY_FLAG_EDITED)
        );
        assert!(properties.iter().any(|property| {
            property.name == "EnableAutomaticPlayerOwnership"
                && property.value == TestVmadValue::Bool(true)
        }));
        assert!(properties.iter().any(|property| {
            property.name == "AllowAttacksBeforeOwned"
                && property.value == TestVmadValue::Bool(false)
        }));
        assert!(properties.iter().any(|property| {
            property.name == "MaxDraws"
                && property.value == TestVmadValue::Int(DEFAULT_WORKSHOP_MAX_DRAWS)
        }));
        assert!(properties.iter().any(|property| {
            property.name == "MaxTriangles"
                && property.value == TestVmadValue::Int(DEFAULT_WORKSHOP_MAX_TRIANGLES)
        }));
        assert!(
            !properties
                .iter()
                .any(|property| property.name == "OwnedByPlayer")
        );
    }

    #[test]
    fn ensure_workshop_script_preserves_existing_vmad() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let existing = SmallVec::from_slice(&[1, 2, 3, 4]);
        let mut record = Record {
            sig: sig_code("REFR"),
            form_key: fk(0x20106D, plugin),
            eid: Some(interner.intern("BeckleyWorkshopRef")),
            flags: RecordFlags::PERSISTENT,
            fields: smallvec![FieldEntry {
                sig: subrecord_sig("VMAD"),
                value: FieldValue::Bytes(existing.clone()),
            }],
            warnings: SmallVec::new(),
        };

        assert!(!ensure_workshop_script(&mut record));
        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].value, FieldValue::Bytes(existing));
    }

    #[test]
    fn set_form_key_field_replaces_existing_cell_location() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let old_location = fk(0x2F70F9, plugin);
        let new_location = fk(0xA00000, plugin);
        let mut cell = Record {
            sig: sig_code("CELL"),
            form_key: fk(0x050B2C, plugin),
            eid: Some(interner.intern("WorkshopCell")),
            flags: RecordFlags::PERSISTENT,
            fields: smallvec![FieldEntry {
                sig: subrecord_sig("XLCN"),
                value: FieldValue::FormKey(old_location),
            }],
            warnings: SmallVec::new(),
        };

        assert!(set_form_key_field(&mut cell, "XLCN", new_location));
        assert_eq!(form_key_field(&cell, "XLCN"), Some(new_location));
        assert!(!set_form_key_field(&mut cell, "XLCN", new_location));
    }

    #[test]
    fn decoded_display_style_fields_are_read() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let center_keyword = fo4_form_key(WORKSHOP_LINK_CENTER_LOCAL, &interner);
        let center_ref = fk(0x20106E, plugin);
        let record = Record {
            sig: sig_code("REFR"),
            form_key: fk(0x20106D, plugin),
            eid: Some(interner.intern("BeckleyWorkshopRef")),
            flags: RecordFlags::PERSISTENT,
            fields: smallvec![
                FieldEntry {
                    sig: subrecord_sig("XLKR"),
                    value: FieldValue::Struct(vec![
                        (
                            interner.intern("KeywordRef"),
                            FieldValue::FormKey(center_keyword)
                        ),
                        (interner.intern("Ref"), FieldValue::FormKey(center_ref)),
                    ]),
                },
                FieldEntry {
                    sig: subrecord_sig("DATA"),
                    value: FieldValue::Struct(vec![
                        (
                            interner.intern("PositionRotationPositionX"),
                            FieldValue::Float(-191025.46875),
                        ),
                        (
                            interner.intern("PositionRotationPositionY"),
                            FieldValue::Float(-126624.265625),
                        ),
                        (
                            interner.intern("PositionRotationPositionZ"),
                            FieldValue::Float(7185.8164),
                        ),
                        (
                            interner.intern("PositionRotationRotationZ"),
                            FieldValue::Float(4.283185),
                        ),
                    ]),
                },
            ],
            warnings: SmallVec::new(),
        };

        assert_eq!(
            linked_ref(&record, center_keyword, &interner),
            Some(center_ref)
        );
        assert_eq!(
            read_placement(&record, &interner),
            Some(Placement {
                position: [-191025.46875, -126624.265625, 7185.8164],
                rotation: [0.0, 0.0, 4.283185],
            })
        );
    }

    #[test]
    fn missing_target_record_decode_is_optional() {
        assert!(is_missing_target_record_decode(
            &SessionError::RecordNotFound("Fallout4.esm:2196DC".into())
        ));
        assert!(is_missing_target_record_decode(&SessionError::Other(
            "record not found: Fallout4.esm:2196DC".into()
        )));
        assert!(!is_missing_target_record_decode(&SessionError::Other(
            "decode failed".into()
        )));
    }
}
