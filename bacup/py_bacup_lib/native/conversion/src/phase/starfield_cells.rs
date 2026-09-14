//! Phase: `starfield_cells` — structured CELL / placed-REFR emission for the
//! `fo4:starfield` pair.
//!
//! `embedded/translation_maps/fo4_to_starfield.yaml` skip-lists `CELL` and
//! `REFR` because parentage in both engines is GROUP TOPOLOGY, not a
//! subrecord: the generic top-level writer would land them flat and the game
//! would stream nothing. This phase is the structured writer the map's header
//! comment defers to. It is the ONLY path CELL/REFR reach a Starfield target,
//! so every placed-ref strip, rescale and FormID remap lives here rather than
//! in a side pass.
//!
//! # Topology emitted (vanilla shape, measured against `Starfield.esm`)
//!
//! ```text
//! GRUP(0,"WRLD")
//!   WRLD                                   <- already written by translate_v2
//!   GRUP(1, label=WRLD form id)            <- world children
//!     CELL  (worldspace persistent cell)   <- carried 1:1, no grid
//!     GRUP(6, label=cell) / GRUP(8)        <- persistent placed refs
//!     GRUP(4, label=(block_y,block_x))     <- exterior block,     cell/32
//!       GRUP(5, label=(sub_y,sub_x))       <- exterior sub-block, cell/8
//!         CELL                             <- one per occupied SF grid square
//!         GRUP(6, label=cell)
//!           GRUP(8|9|10)                   <- persistent / temporary / VWD
//! GRUP(0,"CELL")
//!   GRUP(2, label=obj%10)                  <- interior block
//!     GRUP(3, label=(obj/10)%10)           <- interior sub-block
//!       CELL
//!       GRUP(6) / GRUP(8|9)
//! ```
//!
//! Block/sub-block divisors and label byte order are not assumed from FO4:
//! they were measured on `Starfield.esm` (738/738 exterior cells satisfy
//! `block == grid.div_euclid(32)` and `sub_block == grid.div_euclid(8)` with
//! the label laid out `[y_i16, x_i16]`; 11985/11985 interior cells satisfy
//! `block == obj % 10`, `sub == (obj / 10) % 10`).
//!
//! # Why the exterior grid is re-latticed rather than carried
//!
//! An FO4 exterior cell is 4096 units = 58.52 m; a Starfield exterior cell is
//! 100 m (measured 80/80). The ratio 1.70878 is not an integer, so cell
//! boundaries never align and `XCLC` cannot be carried: a ref scaled by
//! `1/69.99125` would land outside the cell that claims to own it and never
//! stream. Placed refs are therefore bucketed by
//! `sf_frame::sf_cell_of_fo4_units(position)` — the same unshifted 100 m frame
//! `terrain_btd_write` resamples the heightfield onto — and one CELL shell is
//! emitted per occupied bucket. Each bucket adopts the identity (FormID) and
//! payload of one overlapping source cell, so `mapper.lookup(source_cell_fk)`
//! still resolves for the ~59 % of source cells that own a bucket (this is
//! what `audio_rewire`'s REGN→XCMO compensation keys off).
//!
//! # Phase contract
//!
//! No Python, no GIL. Runs AFTER `translate` (needs `mapper_state` to resolve
//! placed-ref bases and XTEL endpoints) and BEFORE `audio_rewire` (which
//! patches `CELL.XCMO`/`XCAS` on records this phase creates).

use std::collections::{BTreeMap, HashMap, HashSet};

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::{
    NativePluginSlot, ParsedGroup, ParsedItem, ParsedRecord, WriteEffect, plugin_handle_store_ref,
};
use smallvec::SmallVec;
use smol_str::SmolStr;
use terrain_native::sf_frame;

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{LogLevel, Phase, PhaseCtx, PhaseError, PhaseEvent, PhaseReport};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::run::{ConversionRun, normalized_eid_opt};
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::translator::pair_hook::PairCtx;
use crate::translator::target_hook::TargetCtx;
use crate::translator::{Game, TranslateResult};

const TOP_GROUP: i32 = 0;
const WORLD_CHILDREN_GROUP: i32 = 1;
const INTERIOR_BLOCK_GROUP: i32 = 2;
const INTERIOR_SUB_BLOCK_GROUP: i32 = 3;
const EXTERIOR_BLOCK_GROUP: i32 = 4;
const EXTERIOR_SUB_BLOCK_GROUP: i32 = 5;
const CELL_CHILDREN_GROUP: i32 = 6;
const CELL_PERSISTENT_GROUP: i32 = 8;
const CELL_TEMPORARY_GROUP: i32 = 9;
const CELL_VISIBLE_DISTANT_GROUP: i32 = 10;

/// FO4 is 69.99125 units/metre; a Starfield render unit is one metre
/// (plan Global Constraints / R7). Every world DISTANCE crossing this pair
/// is multiplied by this; angles and dimensionless scalars are not.
const FO4_TO_SF_SPATIAL: f32 = 1.0 / 69.99125;

/// FO4 `CELL.DATA` is `uint16`, Starfield's is `uint32`.
const FO4_CELL_INTERIOR_FLAG: u64 = 0x0001;

/// Every one of the 300 sampled vanilla Starfield exterior cells carries
/// `DATA == 2` (`uint32`). The reverse pair pins the same value for the FO4
/// exteriors it synthesizes (`starfield_worldspace::EXTERIOR_CELL_DATA_FLAGS`).
const SF_EXTERIOR_CELL_DATA_FLAGS: u64 = 0x0002;

/// Only these cell children are carried. `ACHR` is fenced out of the pair
/// (world records only, no actors); `LAND` has no Starfield record type at all
/// (terrain is the BTD, written by `terrain_btd_write`); `NAVM`/`NAVI` are
/// skipped because there are no NPCs to path.
fn is_carried_placed_signature(signature: &str) -> bool {
    signature == "REFR"
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StarfieldCellsReport {
    pub worlds_emitted: u32,
    pub worlds_skipped_already_nested: u32,
    pub exterior_cells_emitted: u32,
    pub exterior_source_cells_collapsed: u32,
    pub persistent_cells_emitted: u32,
    pub interior_cells_emitted: u32,
    pub placed_refs_emitted: u32,
    /// Placed children whose base record never reached the target (excluded by
    /// the world-only fence, skip-listed, or failed translation).
    pub placed_refs_dropped_missing_base: u32,
    /// Cell children that are not carried placed refs (LAND / NAVM / ACHR).
    pub placed_refs_dropped_not_carried: u32,
    /// Placed children whose own translation dropped or failed.
    pub placed_refs_dropped_translate: u32,
    /// Placed refs with no readable `DATA` position — sited at their source
    /// cell's origin rather than dropped.
    pub placed_refs_without_position: u32,
    pub cells_dropped_translate: u32,
    pub xtel_remapped: u32,
    /// XTEL whose door endpoint did not resolve after the whole slice was
    /// translated. The subrecord is REMOVED (a dangling teleport is a load
    /// crash vector) and the ref is kept as a plain placed ref.
    pub xtel_dropped_dangling: u32,
}

// ---------------------------------------------------------------------------
// Source topology
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SourceSection {
    group_type: i32,
    /// `(form_id, signature)` in tree order.
    children: Vec<(u32, SmolStr)>,
}

#[derive(Debug)]
struct SourceCellPlan {
    form_id: u32,
    grid: Option<(i32, i32)>,
    sections: Vec<SourceSection>,
}

#[derive(Debug)]
struct SourceWorldPlan {
    world_form_id: u32,
    persistent_cells: Vec<SourceCellPlan>,
    lattice_cells: Vec<SourceCellPlan>,
}

#[derive(Debug, Default)]
struct SourcePlans {
    worlds: Vec<SourceWorldPlan>,
    interiors: Vec<SourceCellPlan>,
}

fn find_top_group<'a>(items: &'a [ParsedItem], label: &[u8; 4]) -> Option<&'a ParsedGroup> {
    items.iter().find_map(|item| match item {
        ParsedItem::Group(group) if group.group_type == TOP_GROUP && group.label == *label => {
            Some(group)
        }
        _ => None,
    })
}

fn find_group(items: &[ParsedItem], group_type: i32, label_form_id: u32) -> Option<&ParsedGroup> {
    items.iter().find_map(|item| match item {
        ParsedItem::Group(group)
            if group.group_type == group_type
                && u32::from_le_bytes(group.label) == label_form_id =>
        {
            Some(group)
        }
        _ => None,
    })
}

fn groups_of_type<'a>(
    items: &'a [ParsedItem],
    group_type: i32,
) -> impl Iterator<Item = &'a ParsedGroup> {
    items.iter().filter_map(move |item| match item {
        ParsedItem::Group(group) if group.group_type == group_type => Some(group),
        _ => None,
    })
}

fn find_subrecord<'a>(record: &'a ParsedRecord, signature: &str) -> Option<&'a [u8]> {
    record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == signature)
        .map(|subrecord| subrecord.data.as_ref())
}

fn cell_grid(record: &ParsedRecord) -> Option<(i32, i32)> {
    let data = find_subrecord(record, "XCLC")?;
    if data.len() < 8 {
        return None;
    }
    Some((
        i32::from_le_bytes(data[0..4].try_into().ok()?),
        i32::from_le_bytes(data[4..8].try_into().ok()?),
    ))
}

fn cell_sections(items: &[ParsedItem], cell_form_id: u32) -> Vec<SourceSection> {
    let Some(group) = find_group(items, CELL_CHILDREN_GROUP, cell_form_id) else {
        return Vec::new();
    };
    let mut sections = Vec::new();
    let mut loose = Vec::new();
    for child in &group.children {
        match child {
            // A record directly under the cell-children group is malformed but
            // survivable; treat it as temporary rather than losing it.
            ParsedItem::Record(record) => {
                loose.push((record.form_id, record.signature.clone()));
            }
            ParsedItem::Group(sub_group) => {
                let mut children = Vec::new();
                collect_section_children(&sub_group.children, &mut children);
                if !children.is_empty() {
                    sections.push(SourceSection {
                        group_type: sub_group.group_type,
                        children,
                    });
                }
            }
        }
    }
    if !loose.is_empty() {
        sections.push(SourceSection {
            group_type: CELL_TEMPORARY_GROUP,
            children: loose,
        });
    }
    sections
}

fn collect_section_children(items: &[ParsedItem], out: &mut Vec<(u32, SmolStr)>) {
    for item in items {
        match item {
            ParsedItem::Record(record) => out.push((record.form_id, record.signature.clone())),
            ParsedItem::Group(group) => collect_section_children(&group.children, out),
        }
    }
}

fn collect_cells(items: &[ParsedItem], out: &mut Vec<SourceCellPlan>) {
    for item in items {
        let ParsedItem::Record(record) = item else {
            continue;
        };
        if record.signature.as_str() != "CELL" {
            continue;
        }
        out.push(SourceCellPlan {
            form_id: record.form_id,
            grid: cell_grid(record),
            sections: cell_sections(items, record.form_id),
        });
    }
}

fn collect_source_plans(root_items: &[ParsedItem]) -> SourcePlans {
    let mut plans = SourcePlans::default();

    if let Some(wrld_top) = find_top_group(root_items, b"WRLD") {
        for item in &wrld_top.children {
            let ParsedItem::Record(record) = item else {
                continue;
            };
            if record.signature.as_str() != "WRLD" {
                continue;
            }
            let Some(children) =
                find_group(&wrld_top.children, WORLD_CHILDREN_GROUP, record.form_id)
            else {
                continue;
            };
            let mut plan = SourceWorldPlan {
                world_form_id: record.form_id,
                persistent_cells: Vec::new(),
                lattice_cells: Vec::new(),
            };
            collect_cells(&children.children, &mut plan.persistent_cells);
            for block in groups_of_type(&children.children, EXTERIOR_BLOCK_GROUP) {
                for sub_block in groups_of_type(&block.children, EXTERIOR_SUB_BLOCK_GROUP) {
                    collect_cells(&sub_block.children, &mut plan.lattice_cells);
                }
                // Tolerate a block group with no sub-block level (hand-built
                // fixture plugins do this); a real FO4 plugin always has one.
                collect_cells(&block.children, &mut plan.lattice_cells);
            }
            // A direct world-children CELL that carries a grid is a lattice
            // cell that lost its block nesting, not the persistent cell.
            let (lattice, persistent): (Vec<_>, Vec<_>) =
                std::mem::take(&mut plan.persistent_cells)
                    .into_iter()
                    .partition(|cell| cell.grid.is_some());
            plan.persistent_cells = persistent;
            plan.lattice_cells.extend(lattice);
            if plan.persistent_cells.is_empty() && plan.lattice_cells.is_empty() {
                continue;
            }
            plans.worlds.push(plan);
        }
    }

    if let Some(cell_top) = find_top_group(root_items, b"CELL") {
        for block in groups_of_type(&cell_top.children, INTERIOR_BLOCK_GROUP) {
            for sub_block in groups_of_type(&block.children, INTERIOR_SUB_BLOCK_GROUP) {
                collect_cells(&sub_block.children, &mut plans.interiors);
            }
        }
        // Tolerate a flattened source tree (test fixtures, hand-built plugins):
        // cells sitting directly under the top CELL group are still interiors.
        collect_cells(&cell_top.children, &mut plans.interiors);
    }

    plans
}

// ---------------------------------------------------------------------------
// Spatial rescale + placed-ref field surgery
//
// Every field below is enumerated deliberately: a world DISTANCE is scaled, an
// angle or dimensionless scalar is carried. `struct:` codec subrecords decode
// to `FieldValue::Bytes` (source_read.rs), so the map's `scale_nested` for
// REFR.DATA is inert by construction and this is the live scaler.
// ---------------------------------------------------------------------------

fn scale_f32_at(bytes: &mut [u8], offset: usize) {
    if offset + 4 > bytes.len() {
        return;
    }
    let value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("4 bytes"));
    if !value.is_finite() {
        return;
    }
    bytes[offset..offset + 4].copy_from_slice(&(value * FO4_TO_SF_SPATIAL).to_le_bytes());
}

fn read_f32_at(bytes: &[u8], offset: usize) -> Option<f32> {
    if offset + 4 > bytes.len() {
        return None;
    }
    Some(f32::from_le_bytes(
        bytes[offset..offset + 4].try_into().expect("4 bytes"),
    ))
}

fn field_bytes_mut<'a>(record: &'a mut Record, sig: &SubrecordSig) -> Option<&'a mut [u8]> {
    record
        .fields
        .iter_mut()
        .find(|entry| entry.sig == *sig)
        .and_then(|entry| match &mut entry.value {
            FieldValue::Bytes(bytes) => Some(bytes.as_mut_slice()),
            _ => None,
        })
}

fn field_bytes<'a>(record: &'a Record, sig: &SubrecordSig) -> Option<&'a [u8]> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig == *sig)
        .and_then(|entry| match &entry.value {
            FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
}

fn sub(sig: &str) -> SubrecordSig {
    SubrecordSig::from_str(sig).expect("4-byte literal signature")
}

/// FO4 world position of a placed ref, read off the still-unscaled `DATA`.
fn placed_position_fo4(record: &Record) -> Option<(f32, f32)> {
    let data = field_bytes(record, &sub("DATA"))?;
    Some((read_f32_at(data, 0)?, read_f32_at(data, 4)?))
}

/// Rescale every world-distance field a carried `REFR` can hold.
///
/// | subrecord | codec | bytes touched | treatment |
/// |---|---|---|---|
/// | `DATA` | `struct:f,f,f,f,f,f` | 0,4,8 | position — SCALED |
/// | `DATA` | | 12,16,20 | rotation (radians) — carried |
/// | `XSCL` | `float32` | — | dimensionless scale — carried |
/// | `XTEL` | `struct:I,f,f,f,f,f,f,I,I` | 4,8,12 | destination position — SCALED |
/// | `XTEL` | | 16,20,24 | destination rotation — carried |
/// | `XTEL` | | 0, 32 | door / transition FormIDs — remapped (later pass) |
/// | `XPRM` | `struct:f,f,f,f,f,f,f,I` | 0,4,8 | primitive bounds — SCALED |
/// | `XPRM` | | 12..32 | colour + type — carried |
/// | `XRDS` | `float32` | 0 | radius — SCALED |
/// | `XRGB` | `struct:f,f,f` | — | biped rotation — carried |
fn rescale_placed_ref(record: &mut Record) {
    if let Some(data) = field_bytes_mut(record, &sub("DATA")) {
        for offset in [0usize, 4, 8] {
            scale_f32_at(data, offset);
        }
    }
    if let Some(xtel) = field_bytes_mut(record, &sub("XTEL")) {
        for offset in [4usize, 8, 12] {
            scale_f32_at(xtel, offset);
        }
    }
    if let Some(xprm) = field_bytes_mut(record, &sub("XPRM")) {
        for offset in [0usize, 4, 8] {
            scale_f32_at(xprm, offset);
        }
    }
    if let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig == sub("XRDS"))
        && let FieldValue::Float(radius) = &mut entry.value
    {
        *radius *= FO4_TO_SF_SPATIAL;
    }
}

/// `CELL.XCLW` (Water Height) is a world distance carried raw by the map.
fn rescale_cell(record: &mut Record) {
    if let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig == sub("XCLW"))
        && let FieldValue::Float(height) = &mut entry.value
    {
        *height *= FO4_TO_SF_SPATIAL;
    }
}

fn set_field(record: &mut Record, sig: SubrecordSig, value: FieldValue) {
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == sig) {
        entry.value = value;
        return;
    }
    record.fields.push(FieldEntry { sig, value });
}

/// FO4 `CELL.DATA` (`uint16`) widened into Starfield's `uint32`. Only the
/// interior bit is load-bearing for topology; the rest of the low 16 bits are
/// carried because the flag layout is continuous across the engine lineage.
fn cell_data_flags(source_flags: u16) -> u64 {
    u64::from(source_flags)
}

/// `XCLC` is `struct:i,i,B,B,B,B` (12 bytes) in BOTH schemas and is 12 bytes in
/// all 300 sampled vanilla Starfield exterior cells. The trailing 4 bytes are
/// FO4 land-hiding quadrant flags with no Starfield meaning on a synthesized
/// lattice, so they are emitted zeroed.
fn exterior_grid_payload(cell_x: i32, cell_y: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(12);
    out.extend_from_slice(&cell_x.to_le_bytes());
    out.extend_from_slice(&cell_y.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out
}

/// Measured on `Starfield.esm`: the Y index occupies the low half of an
/// exterior block / sub-block label and X the high half — same as FO4. Never
/// re-derive this byte order from memory.
fn encode_exterior_grid_label(x: i16, y: i16) -> [u8; 4] {
    [
        y.to_le_bytes()[0],
        y.to_le_bytes()[1],
        x.to_le_bytes()[0],
        x.to_le_bytes()[1],
    ]
}

fn clamp_i16(value: i32) -> i16 {
    value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

/// Measured on `Starfield.esm` (11985/11985): interior CELL bucket indices are
/// derived from the cell's own object id exactly as in FO4.
fn interior_bucket_indices(form_id: u32) -> (i32, i32) {
    let object_id = form_id & 0x00FF_FFFF;
    ((object_id % 10) as i32, ((object_id / 10) % 10) as i32)
}

// ---------------------------------------------------------------------------
// Translation of one skip-listed record
// ---------------------------------------------------------------------------

/// Mirrors `ConversionRun::translate_and_remap_snapshot_record` (private to
/// `run`) minus the NVNM leg, which this pair has no use for. The sequence
/// — pre_translate → map → mapper allocate + rewrite → post_translate →
/// target hook → class-A normalize — must stay in this order: the pair hook's
/// weather repoint installs `Starfield.esm` FormKeys that the mapper must not
/// touch, which is only true if it runs after `rewrite_record`.
fn translate_skipped_record(
    run: &mut ConversionRun,
    fk: FormKey,
    ignored_signature: &str,
) -> Option<Record> {
    let schema_source = run.schema_source.clone();
    let schema_target = run.schema_target.clone();

    let mut source_record = match read_record_relayout_by_form_key(
        run.source_handle_id,
        &fk,
        &schema_source,
        &run.interner,
        None,
    ) {
        Ok(record) => record,
        Err(error) => {
            let warning = run
                .interner
                .intern(&format!("starfield_cells_read:{error}"));
            run.warnings.push(warning);
            return None;
        }
    };

    {
        let mut ctx = PairCtx::new(&run.interner);
        if let Err(error) = run.translator.pre_translate(&mut ctx, &mut source_record) {
            let warning = run
                .interner
                .intern(&format!("starfield_cells_pre_translate:{error}"));
            run.warnings.push(warning);
        }
    }

    let result =
        run.translator
            .translate_ignoring_skip(&source_record, &run.interner, ignored_signature);
    let mut translated = match result {
        TranslateResult::Translated(record) => record,
        TranslateResult::Dropped { decision, .. } => {
            run.decisions.push(decision);
            return None;
        }
        TranslateResult::Deferred(_) => return None,
    };

    if schema_target.record_def(translated.sig.as_str()).is_none() {
        let warning = run.interner.intern(&format!(
            "starfield_cells_unsupported_target_record:{}",
            translated.sig.as_str()
        ));
        run.warnings.push(warning);
        return None;
    }

    {
        let state = run.mapper_state.as_mut().expect("mapper_state checked");
        let mut mapper = FormKeyMapper::from_state(state, &run.interner);
        let normalized_eid = normalized_eid_opt(translated.eid, mapper.interner);
        translated.form_key = mapper.allocate_or_resolve(fk, normalized_eid, translated.sig);
        if let Err(error) = mapper.rewrite_record(&mut translated) {
            let warning = run
                .interner
                .intern(&format!("starfield_cells_rewrite:{error}"));
            run.warnings.push(warning);
        }
    }

    {
        let mut ctx = PairCtx::new(&run.interner);
        if let Err(error) = run.translator.post_translate(&mut ctx, &mut translated) {
            let warning = run
                .interner
                .intern(&format!("starfield_cells_post_translate:{error}"));
            run.warnings.push(warning);
        }
    }
    {
        let mut ctx = TargetCtx {
            interner: &run.interner,
        };
        if let Err(error) = run.translator.run_target_hook(&mut ctx, &mut translated) {
            let warning = run
                .interner
                .intern(&format!("starfield_cells_target_hook:{error}"));
            run.warnings.push(warning);
        }
    }

    let report = crate::translator::class_a_normalize::normalize_flags_and_enums(
        &mut translated,
        &schema_target,
        &run.interner,
    );
    for message in report.decisions {
        let kind = run.interner.intern("class_a_normalize");
        run.decisions
            .push(crate::translator::Decision { kind, message });
    }

    Some(translated)
}

/// The raw `CELL.DATA` flags on the SOURCE record. Read before translation
/// because the map drops `DATA` (FO4 `uint16` vs Starfield `uint32`).
fn source_cell_flags(run: &ConversionRun, fk: FormKey) -> Option<u16> {
    let record = read_record_relayout_by_form_key(
        run.source_handle_id,
        &fk,
        &run.schema_source,
        &run.interner,
        None,
    )
    .ok()?;
    record
        .fields
        .iter()
        .find(|entry| entry.sig == sub("DATA"))
        .and_then(|entry| match &entry.value {
            FieldValue::Uint(value) => Some(*value as u16),
            FieldValue::Int(value) => Some(*value as u16),
            FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                Some(u16::from_le_bytes([bytes[0], bytes[1]]))
            }
            _ => None,
        })
}

/// The placed ref's base (`NAME`), read off the SOURCE record so the fence can
/// run before the ref itself is allocated in the mapper.
fn source_placed_base(run: &ConversionRun, fk: FormKey) -> Option<FormKey> {
    let record = read_record_relayout_by_form_key(
        run.source_handle_id,
        &fk,
        &run.schema_source,
        &run.interner,
        None,
    )
    .ok()?;
    record
        .fields
        .iter()
        .find(|entry| entry.sig == sub("NAME"))
        .and_then(|entry| match &entry.value {
            FieldValue::FormKey(base) => Some(*base),
            _ => None,
        })
}

// ---------------------------------------------------------------------------
// In-flight emission state
// ---------------------------------------------------------------------------

struct TranslatedCell {
    /// Source cell form id this identity came from.
    source_form_id: u32,
    record: Record,
    /// `(section group type, translated placed record)` in tree order.
    children: Vec<(i32, Record)>,
}

struct WorldEmission {
    world_form_id: u32,
    persistent: Vec<TranslatedCell>,
    /// Keyed by the Starfield 100 m grid square.
    lattice: BTreeMap<(i32, i32), TranslatedCell>,
}

/// Sections other than persistent(8) / temporary(9) / visible-distant(10) are
/// not legal under a cell-children group; fold anything else into temporary
/// rather than losing the ref.
fn normalize_section_group_type(group_type: i32) -> i32 {
    match group_type {
        CELL_PERSISTENT_GROUP | CELL_TEMPORARY_GROUP | CELL_VISIBLE_DISTANT_GROUP => group_type,
        _ => CELL_TEMPORARY_GROUP,
    }
}

fn form_key(local: u32, plugin: crate::sym::Sym) -> FormKey {
    FormKey {
        local: local & 0x00FF_FFFF,
        plugin,
    }
}

/// Translate a cell's placed children, applying the fence and the rescale.
/// Returns them tagged with the (normalized) section they belong to.
fn translate_cell_children(
    run: &mut ConversionRun,
    cell: &SourceCellPlan,
    source_plugin: crate::sym::Sym,
    report: &mut StarfieldCellsReport,
) -> Vec<(i32, Record)> {
    let mut out = Vec::new();
    for section in &cell.sections {
        let group_type = normalize_section_group_type(section.group_type);
        for (child_form_id, signature) in &section.children {
            if !is_carried_placed_signature(signature.as_str()) {
                report.placed_refs_dropped_not_carried += 1;
                continue;
            }
            let child_fk = form_key(*child_form_id, source_plugin);
            let base = source_placed_base(run, child_fk);
            let base_in_scope = base.is_some_and(|base| {
                run.mapper_state
                    .as_ref()
                    .is_some_and(|state| state.source_to_target.contains_key(&base))
            });
            if !base_in_scope {
                report.placed_refs_dropped_missing_base += 1;
                continue;
            }
            let Some(mut translated) = translate_skipped_record(run, child_fk, "REFR") else {
                report.placed_refs_dropped_translate += 1;
                continue;
            };
            if placed_position_fo4(&translated).is_none() {
                report.placed_refs_without_position += 1;
            }
            rescale_placed_ref(&mut translated);
            out.push((group_type, translated));
        }
    }
    out
}

/// The FO4 cell whose centre a placed ref falls in, used only when the ref has
/// no readable position. Refs are re-sited, never dropped.
fn source_cell_centre_fo4_units(grid: Option<(i32, i32)>) -> (f32, f32) {
    let (x, y) = grid.unwrap_or((0, 0));
    (
        x as f32 * sf_frame::FO4_CELL_UNITS as f32 + sf_frame::FO4_CELL_UNITS as f32 / 2.0,
        y as f32 * sf_frame::FO4_CELL_UNITS as f32 + sf_frame::FO4_CELL_UNITS as f32 / 2.0,
    )
}

fn sf_grid_of_fo4_position(x: f32, y: f32) -> (i32, i32) {
    (
        sf_frame::sf_cell_of_fo4_units(x as f64),
        sf_frame::sf_cell_of_fo4_units(y as f64),
    )
}

// ---------------------------------------------------------------------------
// XTEL door graph
// ---------------------------------------------------------------------------

const XTEL_DOOR_OFFSET: usize = 0;
const XTEL_TRANSITION_OFFSET: usize = 32;

/// Rewrite the two source-file FormIDs embedded in a raw `XTEL` blob. Returns
/// `false` when the door endpoint does not resolve, in which case the caller
/// removes the subrecord — a dangling teleport is a cell-load crash vector.
///
/// `struct:` codec subrecords reach here as `FieldValue::Bytes`, so
/// `FormKeyMapper::rewrite_record` (typed-FormKey leaves only) never saw these.
///
/// `emitted_cells` is the set of TARGET cell FormIDs this phase actually
/// wrote. The mapper happily hands back an id for a source cell that
/// collapsed into a neighbouring lattice bucket and was never emitted, so
/// membership — not mere resolvability — is what makes the transition slot
/// safe.
fn remap_xtel_bytes(
    bytes: &mut [u8],
    raw_map: &HashMap<u32, u32>,
    emitted_cells: &HashSet<u32>,
) -> bool {
    if bytes.len() < XTEL_DOOR_OFFSET + 4 {
        return false;
    }
    let door = u32::from_le_bytes(bytes[0..4].try_into().expect("4 bytes"));
    let Some(&target_door) = raw_map.get(&door) else {
        return false;
    };
    bytes[0..4].copy_from_slice(&target_door.to_le_bytes());

    if bytes.len() >= XTEL_TRANSITION_OFFSET + 4 {
        let transition = u32::from_le_bytes(
            bytes[XTEL_TRANSITION_OFFSET..XTEL_TRANSITION_OFFSET + 4]
                .try_into()
                .expect("4 bytes"),
        );
        if transition != 0 {
            // Zero rather than point at a cell that was never written: the
            // engine resolves the destination from the door ref itself, so a
            // zeroed transition degrades gracefully where a stale id does not.
            let resolved = raw_map
                .get(&transition)
                .copied()
                .filter(|target| emitted_cells.contains(target))
                .unwrap_or(0);
            bytes[XTEL_TRANSITION_OFFSET..XTEL_TRANSITION_OFFSET + 4]
                .copy_from_slice(&resolved.to_le_bytes());
        }
    }
    true
}

fn fix_xtel(
    record: &mut Record,
    raw_map: &HashMap<u32, u32>,
    emitted_cells: &HashSet<u32>,
    report: &mut StarfieldCellsReport,
) {
    let xtel = sub("XTEL");
    let Some(index) = record.fields.iter().position(|entry| entry.sig == xtel) else {
        return;
    };
    let resolved = match &mut record.fields[index].value {
        FieldValue::Bytes(bytes) => remap_xtel_bytes(bytes.as_mut_slice(), raw_map, emitted_cells),
        _ => true,
    };
    if resolved {
        report.xtel_remapped += 1;
    } else {
        record.fields.remove(index);
        report.xtel_dropped_dangling += 1;
    }
}

// ---------------------------------------------------------------------------
// Target tree assembly
// ---------------------------------------------------------------------------

fn group(group_type: i32, label: [u8; 4], tail: &Bytes, children: Vec<ParsedItem>) -> ParsedGroup {
    ParsedGroup {
        label,
        group_type,
        tail: tail.clone(),
        children,
    }
}

/// Build the type-6 cell-children group plus its 8/9/10 sections.
fn cell_children_group(
    cell_form_id: u32,
    children: Vec<(i32, ParsedRecord)>,
    tail: &Bytes,
) -> Option<ParsedGroup> {
    if children.is_empty() {
        return None;
    }
    let label = cell_form_id.to_le_bytes();
    let mut sections: BTreeMap<i32, Vec<ParsedItem>> = BTreeMap::new();
    for (group_type, record) in children {
        sections
            .entry(group_type)
            .or_default()
            .push(ParsedItem::Record(record));
    }
    Some(group(
        CELL_CHILDREN_GROUP,
        label,
        tail,
        sections
            .into_iter()
            .map(|(group_type, items)| ParsedItem::Group(group(group_type, label, tail, items)))
            .collect(),
    ))
}

struct EncodedCell {
    cell: ParsedRecord,
    children: Vec<(i32, ParsedRecord)>,
}

fn encode_cell(
    slot: &mut NativePluginSlot,
    cell: TranslatedCell,
    schema: &crate::schema::AuthoringSchema,
    interner: &crate::sym::StringInterner,
) -> Result<Option<EncodedCell>, PhaseError> {
    let Some(cell_parsed) =
        crate::target_write::encode_record_for_slot(slot, cell.record, schema, interner)
            .map_err(|error| PhaseError::Internal(error.to_string()))?
    else {
        return Ok(None);
    };
    let mut children = Vec::with_capacity(cell.children.len());
    for (group_type, record) in cell.children {
        if let Some(parsed) =
            crate::target_write::encode_record_for_slot(slot, record, schema, interner)
                .map_err(|error| PhaseError::Internal(error.to_string()))?
        {
            children.push((group_type, parsed));
        }
    }
    Ok(Some(EncodedCell {
        cell: cell_parsed,
        children,
    }))
}

// ---------------------------------------------------------------------------
// Phase
// ---------------------------------------------------------------------------

pub struct StarfieldCellsPhase;

impl Phase for StarfieldCellsPhase {
    fn name(&self) -> &'static str {
        "starfield_cells"
    }

    fn run(&self, ctx: &mut PhaseCtx<'_>) -> Result<PhaseReport, PhaseError> {
        ctx.check_cancel()?;
        if ctx.run.source != Game::Fo4 || ctx.run.target != Game::Starfield {
            return Ok(PhaseReport::default());
        }
        if ctx.run.mapper_state.is_none() {
            return Err(PhaseError::Internal(
                "starfield_cells: mapper_state not initialized; must run after translate".into(),
            ));
        }
        let source_handle = ctx.run.source_handle().ok_or_else(|| {
            PhaseError::Internal("starfield_cells requires a source plugin".into())
        })?;

        let plans = {
            let store = plugin_handle_store_ref()
                .lock()
                .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
            let slot = store.get(&source_handle).ok_or_else(|| {
                PhaseError::Internal(format!("unknown source plugin handle: {source_handle}"))
            })?;
            collect_source_plans(&slot.parsed.root_items)
        };
        if plans.worlds.is_empty() && plans.interiors.is_empty() {
            return Ok(PhaseReport::default());
        }

        let cell_sig = SigCode::from_str("CELL").map_err(PhaseError::Internal)?;
        let source_plugin = iter_form_keys_of_sig(source_handle, cell_sig, &ctx.run.interner)
            .map_err(|error| PhaseError::Internal(error.to_string()))?
            .first()
            .map(|fk| fk.plugin)
            .ok_or_else(|| {
                PhaseError::Internal("starfield_cells: source plugin has no CELL records".into())
            })?;

        let mut report = StarfieldCellsReport::default();
        let mut worlds = Vec::new();
        for plan in &plans.worlds {
            ctx.check_cancel()?;
            worlds.push(translate_world(ctx.run, plan, source_plugin, &mut report));
        }
        let mut interiors = Vec::new();
        for plan in &plans.interiors {
            ctx.check_cancel()?;
            if let Some(cell) = translate_cell(ctx.run, plan, source_plugin, false, &mut report) {
                interiors.push(cell);
            } else {
                report.cells_dropped_translate += 1;
            }
        }

        // Both XTEL endpoints are in-slice for a whole-plugin conversion, but
        // the map is only complete once every cell AND every placed ref has
        // been allocated — hence a third pass.
        let raw_map = raw_form_id_map(ctx.run)?;
        let emitted_cells: HashSet<u32> = worlds
            .iter()
            .flat_map(|world| world.persistent.iter().chain(world.lattice.values()))
            .chain(interiors.iter())
            .filter_map(|cell| raw_map.get(&cell.source_form_id).copied())
            .collect();
        for world in &mut worlds {
            for cell in world
                .persistent
                .iter_mut()
                .chain(world.lattice.values_mut())
            {
                for (_, record) in &mut cell.children {
                    fix_xtel(record, &raw_map, &emitted_cells, &mut report);
                }
            }
        }
        for cell in &mut interiors {
            for (_, record) in &mut cell.children {
                fix_xtel(record, &raw_map, &emitted_cells, &mut report);
            }
        }

        write_target(ctx.run, worlds, interiors, &raw_map, &mut report)?;

        let message = format!(
            "starfield_cells: worlds={} worlds_already_nested={} exterior_cells={} \
             collapsed_source_cells={} persistent_cells={} interior_cells={} placed_refs={} \
             dropped_missing_base={} dropped_not_carried={} dropped_translate={} \
             no_position={} cells_dropped={} xtel_remapped={} xtel_dropped={}",
            report.worlds_emitted,
            report.worlds_skipped_already_nested,
            report.exterior_cells_emitted,
            report.exterior_source_cells_collapsed,
            report.persistent_cells_emitted,
            report.interior_cells_emitted,
            report.placed_refs_emitted,
            report.placed_refs_dropped_missing_base,
            report.placed_refs_dropped_not_carried,
            report.placed_refs_dropped_translate,
            report.placed_refs_without_position,
            report.cells_dropped_translate,
            report.xtel_remapped,
            report.xtel_dropped_dangling,
        );
        let symbol = ctx.run.interner.intern(&message);
        ctx.run.warnings.push(symbol);
        let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
            phase: "starfield_cells",
            level: LogLevel::Info,
            message,
        });
        if report.xtel_dropped_dangling > 0 {
            let _ = ctx.run.event_tx.try_send(PhaseEvent::Log {
                phase: "starfield_cells",
                level: LogLevel::Warn,
                message: format!(
                    "starfield_cells: dropped {} XTEL with an unresolvable door endpoint",
                    report.xtel_dropped_dangling
                ),
            });
        }

        Ok(PhaseReport {
            records_added: report.exterior_cells_emitted
                + report.persistent_cells_emitted
                + report.interior_cells_emitted
                + report.placed_refs_emitted,
            records_dropped: report.placed_refs_dropped_missing_base
                + report.placed_refs_dropped_not_carried
                + report.placed_refs_dropped_translate
                + report.cells_dropped_translate,
            warnings: report.xtel_dropped_dangling,
            ..PhaseReport::default()
        })
    }
}

fn raw_form_id_map(run: &ConversionRun) -> Result<HashMap<u32, u32>, PhaseError> {
    let pairs: Vec<(FormKey, FormKey)> = run
        .mapper_state
        .as_ref()
        .map(|state| {
            state
                .source_to_target
                .iter()
                .map(|(&source, &target)| (source, target))
                .collect()
        })
        .unwrap_or_default();
    if pairs.is_empty() {
        return Ok(HashMap::new());
    }
    let raw = crate::fo76_navmesh::raw_formid_mappings_for_context(
        pairs,
        &run.interner,
        run.source_handle_id,
        run.target_handle_id,
    )
    .map_err(|error| PhaseError::Internal(format!("starfield_cells_raw_map:{error}")))?;
    Ok(raw.into_iter().collect())
}

fn translate_cell(
    run: &mut ConversionRun,
    plan: &SourceCellPlan,
    source_plugin: crate::sym::Sym,
    exterior: bool,
    report: &mut StarfieldCellsReport,
) -> Option<TranslatedCell> {
    let cell_fk = form_key(plan.form_id, source_plugin);
    let source_flags = source_cell_flags(run, cell_fk).unwrap_or(0);
    let mut record = translate_skipped_record(run, cell_fk, "CELL")?;
    rescale_cell(&mut record);
    let flags = if exterior {
        SF_EXTERIOR_CELL_DATA_FLAGS
    } else {
        // Force the interior bit on: the map drops DATA, and a cell that lost
        // it would be treated as an exterior with no grid.
        cell_data_flags(source_flags) | FO4_CELL_INTERIOR_FLAG
    };
    set_field(&mut record, sub("DATA"), FieldValue::Uint(flags));
    let children = translate_cell_children(run, plan, source_plugin, report);
    Some(TranslatedCell {
        source_form_id: plan.form_id,
        record,
        children,
    })
}

fn translate_world(
    run: &mut ConversionRun,
    plan: &SourceWorldPlan,
    source_plugin: crate::sym::Sym,
    report: &mut StarfieldCellsReport,
) -> WorldEmission {
    let mut emission = WorldEmission {
        world_form_id: plan.world_form_id,
        persistent: Vec::new(),
        lattice: BTreeMap::new(),
    };

    for cell in &plan.persistent_cells {
        // The worldspace persistent cell has no grid and never re-lattices;
        // its children keep their (rescaled) positions verbatim.
        match translate_cell(run, cell, source_plugin, false, report) {
            Some(mut translated) => {
                // A persistent cell is not an interior — undo the interior bit
                // `translate_cell` forces on and keep the source's own flags.
                let source_flags =
                    source_cell_flags(run, form_key(cell.form_id, source_plugin)).unwrap_or(0);
                set_field(
                    &mut translated.record,
                    sub("DATA"),
                    FieldValue::Uint(cell_data_flags(source_flags)),
                );
                emission.persistent.push(translated);
            }
            None => report.cells_dropped_translate += 1,
        }
    }

    // Pass A — translate every source lattice cell and claim the Starfield
    // bucket its centre falls in. The first source cell to reach a bucket
    // donates its FormID and payload (so `mapper.lookup(source_cell_fk)` still
    // resolves for it); the ~41 % that collapse contribute only their refs.
    // Every source cell's home bucket therefore EXISTS after this pass, which
    // is what makes the pass-B fallback total.
    let mut pending: Vec<((i32, i32), Vec<(i32, Record)>)> = Vec::new();
    for cell in &plan.lattice_cells {
        let Some(translated) = translate_cell(run, cell, source_plugin, true, report) else {
            report.cells_dropped_translate += 1;
            continue;
        };
        let home = home_grid(cell.grid);
        match emission.lattice.entry(home) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(TranslatedCell {
                    source_form_id: translated.source_form_id,
                    record: translated.record,
                    children: Vec::new(),
                });
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                report.exterior_source_cells_collapsed += 1;
            }
        }
        pending.push((home, translated.children));
    }

    // Pass B — site every placed ref in the bucket its own (FO4-unit) position
    // falls in. A ref whose bucket has no cell (sparse source lattice, or a
    // ref sitting outside its nominal cell) falls back to its source cell's
    // home bucket rather than being dropped: the ref keeps its exact scaled
    // world position either way, only its streaming parent differs.
    for (home, children) in pending {
        for (group_type, record) in children {
            let grid = match placed_position_fo4(&record) {
                Some((x, y)) => {
                    let candidate = sf_grid_of_fo4_position(x, y);
                    if emission.lattice.contains_key(&candidate) {
                        candidate
                    } else {
                        home
                    }
                }
                None => home,
            };
            let Some(target) = emission.lattice.get_mut(&grid) else {
                // Unreachable while pass A ran for this ref's own cell; count
                // rather than panic so a malformed source tree cannot abort
                // the whole conversion.
                report.placed_refs_dropped_translate += 1;
                continue;
            };
            target.children.push((group_type, record));
        }
    }

    emission
}

/// The Starfield 100 m bucket an FO4 exterior cell's CENTRE falls in. Centre
/// rather than corner so the mapping is stable for the 1.709:1 collapse.
fn home_grid(grid: Option<(i32, i32)>) -> (i32, i32) {
    let (x, y) = source_cell_centre_fo4_units(grid);
    sf_grid_of_fo4_position(x, y)
}

fn write_target(
    run: &mut ConversionRun,
    worlds: Vec<WorldEmission>,
    interiors: Vec<TranslatedCell>,
    raw_map: &HashMap<u32, u32>,
    report: &mut StarfieldCellsReport,
) -> Result<(), PhaseError> {
    let schema = run.schema_target.clone();
    let target_handle = run.target_handle_id;

    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
    let slot = store.get_mut(&target_handle).ok_or_else(|| {
        PhaseError::Internal(format!("unknown target plugin handle: {target_handle}"))
    })?;
    let tail = Bytes::from(vec![0u8; slot.parsed.header_size.saturating_sub(16)]);

    // ---- worldspaces ------------------------------------------------------
    // Keyed by the TARGET world FormID: the world-children group's label is
    // that id, and the splice below matches the record it must follow by id
    // rather than by position.
    let mut world_children: HashMap<u32, ParsedGroup> = HashMap::new();
    for world in worlds {
        let Some(&target_world_form_id) = raw_map.get(&world.world_form_id) else {
            continue;
        };
        let mut blocks: BTreeMap<(i32, i32), BTreeMap<(i32, i32), Vec<ParsedItem>>> =
            BTreeMap::new();
        let mut children: Vec<ParsedItem> = Vec::new();

        for cell in world.persistent {
            let Some(encoded) = encode_cell(slot, cell, &schema, &run.interner)? else {
                report.cells_dropped_translate += 1;
                continue;
            };
            report.persistent_cells_emitted += 1;
            report.placed_refs_emitted += encoded.children.len() as u32;
            let cell_form_id = encoded.cell.form_id;
            children.push(ParsedItem::Record(encoded.cell));
            if let Some(group) = cell_children_group(cell_form_id, encoded.children, &tail) {
                children.push(ParsedItem::Group(group));
            }
        }

        for (grid, mut cell) in world.lattice {
            set_field(
                &mut cell.record,
                sub("XCLC"),
                FieldValue::Bytes(SmallVec::from_vec(exterior_grid_payload(grid.0, grid.1))),
            );
            let Some(encoded) = encode_cell(slot, cell, &schema, &run.interner)? else {
                report.cells_dropped_translate += 1;
                continue;
            };
            report.exterior_cells_emitted += 1;
            report.placed_refs_emitted += encoded.children.len() as u32;
            let cell_form_id = encoded.cell.form_id;
            let mut items = vec![ParsedItem::Record(encoded.cell)];
            if let Some(group) = cell_children_group(cell_form_id, encoded.children, &tail) {
                items.push(ParsedItem::Group(group));
            }
            let block = (grid.0.div_euclid(32), grid.1.div_euclid(32));
            let sub_block = (grid.0.div_euclid(8), grid.1.div_euclid(8));
            blocks
                .entry(block)
                .or_default()
                .entry(sub_block)
                .or_default()
                .extend(items);
        }

        for ((block_x, block_y), sub_blocks) in blocks {
            let mut block_items = Vec::new();
            for ((sub_x, sub_y), items) in sub_blocks {
                block_items.push(ParsedItem::Group(group(
                    EXTERIOR_SUB_BLOCK_GROUP,
                    encode_exterior_grid_label(clamp_i16(sub_x), clamp_i16(sub_y)),
                    &tail,
                    items,
                )));
            }
            children.push(ParsedItem::Group(group(
                EXTERIOR_BLOCK_GROUP,
                encode_exterior_grid_label(clamp_i16(block_x), clamp_i16(block_y)),
                &tail,
                block_items,
            )));
        }

        if children.is_empty() {
            continue;
        }
        world_children.insert(
            target_world_form_id,
            group(
                WORLD_CHILDREN_GROUP,
                target_world_form_id.to_le_bytes(),
                &tail,
                children,
            ),
        );
    }

    if !world_children.is_empty() {
        splice_world_children(slot, &mut world_children, report);
    }

    // ---- interiors --------------------------------------------------------
    let mut interior_items: BTreeMap<(i32, i32), Vec<ParsedItem>> = BTreeMap::new();
    for cell in interiors {
        let Some(encoded) = encode_cell(slot, cell, &schema, &run.interner)? else {
            report.cells_dropped_translate += 1;
            continue;
        };
        report.interior_cells_emitted += 1;
        report.placed_refs_emitted += encoded.children.len() as u32;
        let cell_form_id = encoded.cell.form_id;
        let bucket = interior_bucket_indices(cell_form_id);
        let entry = interior_items.entry(bucket).or_default();
        entry.push(ParsedItem::Record(encoded.cell));
        // Vanilla always follows an interior CELL with a cell-children group,
        // even an empty one (`ensure_interior_cell_and_child_group`).
        entry.push(ParsedItem::Group(
            cell_children_group(cell_form_id, encoded.children, &tail).unwrap_or_else(|| {
                group(
                    CELL_CHILDREN_GROUP,
                    cell_form_id.to_le_bytes(),
                    &tail,
                    Vec::new(),
                )
            }),
        ));
    }
    if !interior_items.is_empty() {
        splice_interiors(slot, interior_items, &tail);
    }

    slot.clear_record_count_cache();
    slot.apply_write_effect(&WriteEffect::RecordsAddedOrRemoved);
    Ok(())
}

/// Insert each rebuilt world-children group immediately after the WRLD record
/// it belongs to, matched by TARGET FormID.
fn splice_world_children(
    slot: &mut NativePluginSlot,
    pending: &mut HashMap<u32, ParsedGroup>,
    report: &mut StarfieldCellsReport,
) {
    let Some(wrld_top) = slot
        .parsed
        .root_items
        .iter_mut()
        .find_map(|item| match item {
            ParsedItem::Group(group)
                if group.group_type == TOP_GROUP && group.label == *b"WRLD" =>
            {
                Some(group)
            }
            _ => None,
        })
    else {
        return;
    };

    let already_nested: HashSet<u32> = wrld_top
        .children
        .iter()
        .filter_map(|item| match item {
            ParsedItem::Group(group) if group.group_type == WORLD_CHILDREN_GROUP => {
                Some(u32::from_le_bytes(group.label))
            }
            _ => None,
        })
        .collect();

    let mut rebuilt = Vec::with_capacity(wrld_top.children.len() + pending.len());
    for item in wrld_top.children.drain(..) {
        let world_form_id = match &item {
            ParsedItem::Record(record) if record.signature.as_str() == "WRLD" => {
                Some(record.form_id)
            }
            _ => None,
        };
        rebuilt.push(item);
        let Some(world_form_id) = world_form_id else {
            continue;
        };
        // Idempotency: a world that already carries a world-children group has
        // been emitted; re-nesting would duplicate every cell.
        if already_nested.contains(&world_form_id) {
            report.worlds_skipped_already_nested += 1;
            continue;
        }
        let Some(group) = pending.remove(&world_form_id) else {
            continue;
        };
        rebuilt.push(ParsedItem::Group(group));
        report.worlds_emitted += 1;
    }
    wrld_top.children = rebuilt;
}

fn splice_interiors(
    slot: &mut NativePluginSlot,
    interior_items: BTreeMap<(i32, i32), Vec<ParsedItem>>,
    tail: &Bytes,
) {
    let header_size = slot.parsed.header_size;
    let _ = header_size;
    let cell_top_index = slot.parsed.root_items.iter().position(|item| {
        matches!(item, ParsedItem::Group(group) if group.group_type == TOP_GROUP && group.label == *b"CELL")
    });
    let cell_top_index = match cell_top_index {
        Some(index) => index,
        None => {
            slot.parsed.root_items.push(ParsedItem::Group(group(
                TOP_GROUP,
                *b"CELL",
                tail,
                Vec::new(),
            )));
            slot.parsed.root_items.len() - 1
        }
    };
    let ParsedItem::Group(cell_top) = &mut slot.parsed.root_items[cell_top_index] else {
        return;
    };

    for ((block, sub_block), items) in interior_items {
        let block_group = ensure_bucket_group(cell_top, INTERIOR_BLOCK_GROUP, block, tail);
        let sub_block_group =
            ensure_bucket_group(block_group, INTERIOR_SUB_BLOCK_GROUP, sub_block, tail);
        sub_block_group.children.extend(items);
    }
}

fn ensure_bucket_group<'a>(
    parent: &'a mut ParsedGroup,
    group_type: i32,
    bucket: i32,
    tail: &Bytes,
) -> &'a mut ParsedGroup {
    let label = bucket.to_le_bytes();
    if let Some(index) = parent.children.iter().position(|item| {
        matches!(item, ParsedItem::Group(g) if g.group_type == group_type && g.label == label)
    }) {
        let ParsedItem::Group(existing) = &mut parent.children[index] else {
            unreachable!("position matched a group")
        };
        return existing;
    }
    parent.children.push(ParsedItem::Group(group(
        group_type,
        label,
        tail,
        Vec::new(),
    )));
    let ParsedItem::Group(created) = parent.children.last_mut().expect("just pushed") else {
        unreachable!("just pushed a group")
    };
    created
}

#[cfg(test)]
mod tests;
