//! Plugin-driven precombine planning for interior and exterior CELLs.
//!
//! Each placed reference to a STAT or SCOL base is tested on its own: an
//! ineligible or unverified reference is excluded and stays a normal
//! reference, which is always loadable, rather than rejecting its CELL.
//! Eligible references are grouped by CK's key of their runtime bound (the
//! quantised centre inside, the overlapped subdivisions outside); each group
//! becomes one `_OC` NIF holding every base that shares the key.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use nif_core_native::model::NifFile;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;

use crate::assets::{AssetResolver, mesh_path};
use crate::crc::{EXTERIOR_CELL_SIZE, exterior_mask_key, spatial_group_key};
use crate::error::{PrevisError, Result};
use crate::f32ops::{Transform, euler_rotation};
use crate::group::{EXPORT_INFO_SCOL, EXPORT_INFO_STAT, SourceModel, unbuildable_shape};
use crate::metadata::Xcri;
use crate::shader::{ExpandedShader, TREE_ANIM};
use crate::swap::{MaterialSwap, SwapLayers, read_swap};
use crate::nif::{BoundRow, bound_rows, placed_bound};
use crate::plugin::{LoadOrder, Plugin, RecordRef, read_f32, read_u32, zstring};
use crate::records::{self, BASE_FLAG_IS_MARKER, RECORD_FLAG_DELETED};

pub const REFERENCE_FLAG_INITIALLY_DISABLED: u32 = 0x0000_0800;
/// A non-occluder base is still combined, into its own group class: CK's key
/// function (`0xD22280`) sets bit 31 from this base form flag. The eligibility
/// gate CK traces as "is non-occluder" calls the base's marker virtual
/// (`+0x410`), not this flag.
const BASE_FLAG_NON_OCCLUDER: u32 = 0x0000_0010;
const NON_OCCLUDER_GROUP_BIT: u32 = 0x8000_0000;
/// Outside, CK also splits off references whose bound radius is under 100:
/// every CK group of the 25 Appalachia and seven retail oracle CELLs with bit
/// 30 has radii up to 99.5, every one without it radii from 101.1.
const SMALL_GROUP_BIT: u32 = 0x4000_0000;
const SMALL_RADIUS: f32 = 100.0;
/// CK's surface for a LAND without heights (as in previs scenes).
const FLAT_LAND_HEIGHT: f32 = -2048.0;
const WORLD_PARENT_USE_LAND: u16 = 0x0001;
/// Base subrecords with a verified-irrelevant or absent effect on combining.
/// `MNAM` (distant-LOD meshes) is carried by bases retail combines freely;
/// CK 1.11.240 combined every reference to a base carrying `NVNM`.
const STAT_FIELDS: [&[u8; 4]; 12] =
    [b"EDID", b"OBND", b"PTRN", b"FULL", b"MODL", b"MODT", b"MODC", b"MODS", b"PRPS", b"DNAM", b"MNAM", b"NVNM"];
const SCOL_FIELDS: [&[u8; 4]; 9] = [b"EDID", b"OBND", b"PTRN", b"FULL", b"MODL", b"MODT", b"FLTR", b"ONAM", b"DATA"];
/// Reference fields that never block combining: retail CK combines tens of
/// thousands of references carrying layer and reference-group data, and CK
/// 1.11.240 combined every reference carrying `XALP`, `XACT`, `ONAM` or `XRGD`.
const COMBINABLE_REFERENCE_FIELDS: [&[u8; 4]; 10] =
    [b"EDID", b"NAME", b"DATA", b"XSCL", b"XLYR", b"XRFG", b"XALP", b"XACT", b"ONAM", b"XRGD"];
const SCOL_PLACEMENT_SIZE: usize = 28;
/// Fallout4.esm MultiBoundMarker, RoomMarker and PortalMarker. CK keys refs
/// parented under their nodes by the marker FormID; which refs the engine
/// attaches to a room is not reproduced, so such CELLs are not combined.
const ROOM_MARKER_OBJECT_IDS: [u32; 3] = [0x15, 0x1F, 0x20];

/// A resolved `MSWP` and the key identifying it across the load order.
pub struct Swap {
    pub key: String,
    pub swap: MaterialSwap,
}

/// Prepared source models keyed by lowercase mesh path and material swaps.
#[derive(Default)]
pub struct SourceCache {
    models: Mutex<FxHashMap<String, Arc<SourceModel>>>,
    /// Expanded shaders by content: most shapes share theirs with others.
    shaders: Mutex<FxHashMap<u64, Vec<Arc<ExpandedShader>>>>,
}

impl SourceCache {
    pub fn model(
        &self,
        model: &str,
        assets: &AssetResolver,
        reference_swap: Option<&Swap>,
        model_swap: Option<&Swap>,
        color_index: Option<f32>,
    ) -> Result<Arc<SourceModel>> {
        let relative = mesh_path(model)?;
        let key = format!(
            "{}|{}|{}|{:?}",
            relative.to_ascii_lowercase(),
            reference_swap.map_or("", |s| s.key.as_str()),
            model_swap.map_or("", |s| s.key.as_str()),
            color_index.map(f32::to_bits)
        );
        if let Some(found) = self.models.lock().unwrap().get(&key) {
            return Ok(found.clone());
        }
        let bytes = assets
            .read(&relative)?
            .ok_or_else(|| PrevisError::unsupported(format!("model not found: {relative}")))?;
        let swap = SwapLayers {
            reference: reference_swap.map(|s| &s.swap),
            model: model_swap.map(|s| &s.swap),
        };
        let mut loaded = SourceModel::load(&relative, &bytes, assets, swap, color_index)?;
        {
            let mut shaders = self.shaders.lock().unwrap();
            for shape in &mut loaded.shapes {
                let copies = shaders.entry(shape.shader.content_hash()).or_default();
                match copies.iter().find(|copy| copy.identical(&shape.shader)) {
                    Some(copy) => shape.shader = copy.clone(),
                    None => copies.push(shape.shader.clone()),
                }
            }
        }
        let loaded = Arc::new(loaded);
        self.models.lock().unwrap().insert(key, loaded.clone());
        Ok(loaded)
    }
}

/// One component model of a combined base with its local placement.
pub struct PlannedSource {
    pub model: Arc<SourceModel>,
    /// SCOL component placement composed under each reference; `None` for a
    /// plain STAT, whose reference transforms CK uses unchanged.
    pub component: Option<Transform>,
    pub base_record_flags: u32,
    pub leaf: Option<(f32, f32)>,
    /// The STAT's EditorID, naming its root collision clone.
    pub editor_id: String,
    /// The collision goes to the CELL's `_Physics.NIF` rather than a clone.
    pub merges_collision: bool,
}

/// A STAT or SCOL base and its component sources in the CELL source list.
pub struct PlannedBase {
    pub sources: Range<usize>,
    pub export_info: [u8; 10],
}

/// One base's instances inside a group, in CK's reverse candidate order.
pub struct GroupMember {
    pub base: usize,
    pub instances: Vec<Transform>,
}

pub struct PlannedGroup {
    pub mesh_id: u32,
    pub root_name: String,
    pub members: Vec<GroupMember>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExcludedReference {
    pub form_id: u32,
    pub reason: String,
}

pub struct CellPrecombine {
    pub cell_form_id: u32,
    pub sources: Vec<PlannedSource>,
    pub bases: Vec<PlannedBase>,
    pub groups: Vec<PlannedGroup>,
    /// Every combined reference's base and placement, in reference order.
    pub placements: Vec<(usize, Transform)>,
    pub xcri: Xcri,
    pub excluded: Vec<ExcludedReference>,
}

pub struct Context<'a> {
    pub load_order: &'a LoadOrder,
    pub assets: &'a AssetResolver,
    pub sources: &'a SourceCache,
    /// References some REFR/ACHR links to (`XLKR`); CK never combines them.
    pub linked_from: &'a FxHashSet<u32>,
    /// See [`exterior_persistent_references`].
    pub exterior_persistent: &'a FxHashMap<u32, Vec<RecordRef>>,
}

/// Each exterior CELL's persistent REFRs. A plugin keeps them in its world's
/// persistent CELL; the engine loads each into the CELL its position falls in,
/// and CK combines them there (Appalachia CELL (-26,22) combines persistent
/// REFR 2DBA64).
pub fn exterior_persistent_references(plugin: &Plugin) -> Result<FxHashMap<u32, Vec<RecordRef>>> {
    let grid_cells: FxHashSet<u32> = plugin.world_cells.values().flatten().copied().collect();
    let mut by_cell: FxHashMap<u32, Vec<RecordRef>> = FxHashMap::default();
    let mut holders: Vec<(u32, u32)> = plugin
        .cells
        .iter()
        .filter(|(id, topology)| !grid_cells.contains(id) && topology.world.is_some() && !topology.persistent.is_empty())
        .map(|(&id, topology)| (id, topology.world.unwrap()))
        .collect();
    holders.sort_unstable();
    for (holder, world) in holders {
        for child in plugin.cells[&holder].persistent.iter().filter(|c| &c.signature == b"REFR") {
            let subrecords = plugin.subrecords_at(child)?;
            let Some(data) = subrecords.first(b"DATA") else { continue };
            let (Some(x), Some(y)) = (read_f32(data, 0), read_f32(data, 4)) else { continue };
            let grid = ((x / EXTERIOR_CELL_SIZE).floor() as i32, (y / EXTERIOR_CELL_SIZE).floor() as i32);
            if let [cell] = plugin.cells_at(world, grid.0, grid.1) {
                by_cell.entry(*cell).or_default().push(*child);
            }
        }
    }
    Ok(by_cell)
}

/// The lowest LAND vertex under an exterior CELL, from the parent world's
/// LAND when the world uses it; `None` without one.
fn land_minimum(context: &Context, plugin: &Plugin, cell_form_id: u32, grid: (i32, i32)) -> Result<Option<f32>> {
    let land_of = |owner: &Plugin, cell: u32| owner.cells.get(&cell).and_then(|t| t.temporary.iter().find(|r| &r.signature == b"LAND").copied());
    let mut found = land_of(plugin, cell_form_id).map(|land| (plugin, land));
    if found.is_none() {
        let world = plugin.cells.get(&cell_form_id).and_then(|t| t.world).and_then(|w| plugin.record(w));
        if let Some(world) = world {
            let subrecords = plugin.subrecords_at(&world)?;
            let uses_parent_land = subrecords
                .first(b"PNAM")
                .and_then(|d| d.first().copied())
                .is_some_and(|flags| flags as u16 & WORLD_PARENT_USE_LAND != 0);
            let parent = subrecords.first(b"WNAM").and_then(|d| read_u32(d, 0)).filter(|_| uses_parent_land);
            if let Some((parent_plugin, parent_world)) = parent.and_then(|raw| context.load_order.resolve(plugin, raw)) {
                if let [parent_cell] = parent_plugin.cells_at(parent_world.form_id, grid.0, grid.1) {
                    found = land_of(parent_plugin, *parent_cell).map(|land| (parent_plugin, land));
                }
            }
        }
    }
    let Some((owner, land)) = found else { return Ok(None) };
    let subrecords = owner.subrecords_at(&land)?;
    let Some(vhgt) = subrecords.first(b"VHGT") else { return Ok(Some(FLAT_LAND_HEIGHT)) };
    if vhgt.len() < 4 + 33 * 33 {
        return Err(PrevisError::invalid(format!("LAND {:08X} VHGT is truncated", land.form_id)));
    }
    let mut deltas = [[0i8; 33]; 33];
    for (index, &value) in vhgt[4..4 + 33 * 33].iter().enumerate() {
        deltas[index / 33][index % 33] = value as i8;
    }
    let heights = crate::scene::vhgt_heights(read_f32(vhgt, 0).unwrap(), &deltas);
    Ok(Some(heights.iter().flatten().copied().fold(f32::INFINITY, f32::min)))
}

/// REFR/ACHR form IDs that another reference in `plugin` links to.
pub fn linked_references(plugin: &Plugin) -> Result<FxHashSet<u32>> {
    let mut targets = FxHashSet::default();
    for signature in [b"REFR", b"ACHR"] {
        for record in plugin.records_of(signature) {
            for (field, data) in plugin.subrecords_at(&record)?.iter() {
                if &field == b"XLKR" {
                    if let Some(target) = read_u32(data, 4) {
                        targets.insert(target);
                    }
                }
            }
        }
    }
    Ok(targets)
}

fn check_fields(plugin: &Plugin, record: &RecordRef, allowed: &[&[u8; 4]]) -> std::result::Result<(), String> {
    let subrecords = plugin.subrecords_at(record).map_err(|e| e.to_string())?;
    for (signature, _) in subrecords.iter() {
        if !allowed.contains(&&signature) {
            return Err(format!(
                "{} {:08X} has unverified field {}",
                String::from_utf8_lossy(&record.signature),
                record.form_id,
                String::from_utf8_lossy(&signature)
            ));
        }
    }
    Ok(())
}

fn model_of(plugin: &Plugin, record: &RecordRef) -> Result<String> {
    records::read_base(plugin, record)?.model.ok_or_else(|| {
        PrevisError::unsupported(format!("base {:08X} has no model path", record.form_id))
    })
}

fn placement_transform(euler: [f32; 3], translation: [f32; 3], scale: f32) -> Transform {
    Transform {
        rotation: if euler.iter().all(|&v| v == 0.0) {
            Transform::IDENTITY.rotation
        } else {
            euler_rotation(euler)
        },
        translation,
        scale,
    }
}

/// SCOL components in CK's traversal order (the stored order reversed).
fn scol_components(plugin: &Plugin, record: &RecordRef) -> Result<Vec<(u32, Transform)>> {
    let subrecords = plugin.subrecords_at(record)?;
    let mut components = Vec::new();
    let mut pending = None;
    for (signature, data) in subrecords.iter() {
        match &signature {
            b"ONAM" => {
                if pending.is_some() {
                    return Err(PrevisError::invalid("SCOL Static has no following Placements"));
                }
                pending = Some(read_u32(data, 0).ok_or_else(|| PrevisError::invalid("SCOL ONAM is truncated"))?);
            }
            b"DATA" => {
                let base = pending.take().ok_or_else(|| PrevisError::invalid("SCOL Placements has no Static"))?;
                if data.is_empty() || data.len() % SCOL_PLACEMENT_SIZE != 0 {
                    return Err(PrevisError::invalid("SCOL placements are malformed"));
                }
                for chunk in data.chunks(SCOL_PLACEMENT_SIZE) {
                    let v = |i: usize| read_f32(chunk, i * 4).unwrap();
                    components.push((base, placement_transform([v(3), v(4), v(5)], [v(0), v(1), v(2)], v(6))));
                }
            }
            _ => {}
        }
    }
    if pending.is_some() || components.is_empty() {
        return Err(PrevisError::invalid("SCOL has no complete component placements"));
    }
    components.reverse();
    Ok(components)
}

fn editor_id_of(plugin: &Plugin, record: &RecordRef) -> Result<String> {
    Ok(plugin.subrecords_at(record)?.first(b"EDID").map(zstring).unwrap_or_default())
}

fn swap_of(context: &Context, owner: &Plugin, raw: u32) -> Result<std::result::Result<Swap, String>> {
    let Some((plugin, record)) = context.load_order.resolve(owner, raw) else {
        return Ok(Err(format!("material swap {raw:08X} is unresolved")));
    };
    if &record.signature != b"MSWP" {
        return Ok(Err(format!("material swap {raw:08X} is not an MSWP")));
    }
    Ok(match read_swap(plugin, &record) {
        Ok(swap) => Ok(Swap {
            key: format!("{}:{:08X}", plugin.name, record.form_id),
            swap,
        }),
        Err(error) => Err(error.to_string()),
    })
}

fn model_swap_of(plugin: &Plugin, record: &RecordRef) -> Result<Option<u32>> {
    Ok(plugin.subrecords_at(record)?.first(b"MODS").and_then(|d| read_u32(d, 0)))
}

fn model_color_index_of(plugin: &Plugin, record: &RecordRef) -> Result<Option<f32>> {
    Ok(plugin.subrecords_at(record)?.first(b"MODC").and_then(|d| read_f32(d, 0)))
}

fn leaf_of(plugin: &Plugin, record: &RecordRef) -> Result<Option<(f32, f32)>> {
    let subrecords = plugin.subrecords_at(record)?;
    Ok(subrecords
        .first(b"DNAM")
        .and_then(|d| Some((read_f32(d, 8)?, read_f32(d, 12)?))))
}

/// The per-reference half of CK's eligibility predicate (`0xD74530`) that is
/// visible in plugin data. `Ok` carries the base and transform; `Err` the
/// exclusion reason. Unverified fields exclude rather than guess.
fn reference_gate(context: &Context, plugin: &Plugin, child: &RecordRef) -> Result<std::result::Result<(u32, Transform), String>> {
    if child.flags & RECORD_FLAG_DELETED != 0 {
        return Ok(Err("deleted".into()));
    }
    if child.flags & REFERENCE_FLAG_INITIALLY_DISABLED != 0 {
        return Ok(Err("initially disabled".into()));
    }
    if context.linked_from.contains(&child.form_id) {
        return Ok(Err("linked to/from".into()));
    }
    let subrecords = plugin.subrecords_at(child)?;
    let (mut base, mut data, mut scale) = (None, None, None);
    for (signature, payload) in subrecords.iter() {
        let reason = match &signature {
            b"NAME" => {
                base = read_u32(payload, 0);
                continue;
            }
            b"DATA" => {
                data = Some(payload);
                continue;
            }
            b"XSCL" => {
                scale = read_f32(payload, 0);
                continue;
            }
            other if COMBINABLE_REFERENCE_FIELDS.contains(&other) => continue,
            b"VMAD" => "scripted".to_string(),
            b"XESP" => "enable parented".to_string(),
            b"XLKR" => "linked to/from".to_string(),
            b"XATR" => "attach ref".to_string(),
            b"XEMI" => "external emittance".to_string(),
            b"XLRT" => "loc ref type".to_string(),
            b"XPRM" => "primitive".to_string(),
            b"XMSP" => continue,
            other => format!("unverified field {}", String::from_utf8_lossy(other)),
        };
        return Ok(Err(reason));
    }
    let base = base.ok_or_else(|| PrevisError::invalid(format!("REFR {:08X} has no base", child.form_id)))?;
    Ok(Ok((base, records::placement(data, scale))))
}

/// What the runtime bound of a base is measured on: the STAT's own model, or
/// a SCOL's shipped composite NIF rather than its expanded components.
enum BoundModel {
    Source(Arc<SourceModel>),
    Composite(Vec<BoundRow>),
}

impl BoundModel {
    fn rows(&self) -> &[BoundRow] {
        match self {
            BoundModel::Source(model) => &model.bound_rows,
            BoundModel::Composite(rows) => rows,
        }
    }
}

struct PreparedBase {
    base: PlannedBase,
    bound: BoundModel,
    non_occluder: bool,
    /// Why a scaled placement's collision cannot be pre-scaled, if so.
    unscalable_collision: Option<String>,
}

/// Resolves one STAT/SCOL base into its sources. `Ok(None)` means the base is
/// not a precombine candidate type at all; `Ok(Some(Err))` an exclusion.
fn prepare_base(
    context: &Context,
    plugin: &Plugin,
    base_raw: u32,
    reference_swap: Option<u32>,
    sources: &mut Vec<PlannedSource>,
) -> Result<Option<std::result::Result<PreparedBase, String>>> {
    let Some((base_plugin, base_record)) = context.load_order.resolve(plugin, base_raw) else {
        return Ok(Some(Err(format!("base {base_raw:08X} is unresolved"))));
    };
    if !matches!(&base_record.signature, b"STAT" | b"SCOL") {
        return Ok(None);
    }
    if base_record.flags & BASE_FLAG_IS_MARKER != 0 {
        return Ok(Some(Err("marker".into())));
    }
    let reference_swap = match reference_swap.map(|raw| swap_of(context, plugin, raw)).transpose()? {
        Some(Err(reason)) => return Ok(Some(Err(reason))),
        found => found.map(|s| s.unwrap()),
    };
    let reference_swap = reference_swap.as_ref();
    let resolved_model_swap = |plugin: &Plugin, record: &RecordRef| -> Result<std::result::Result<Option<Swap>, String>> {
        Ok(match model_swap_of(plugin, record)? {
            Some(raw) => swap_of(context, plugin, raw)?.map(Some),
            None => Ok(None),
        })
    };
    let expand = || -> Result<std::result::Result<(Vec<PlannedSource>, PreparedBoundKind), String>> {
        let model = model_of(base_plugin, &base_record)?;
        if &base_record.signature == b"STAT" {
            if let Err(reason) = check_fields(base_plugin, &base_record, &STAT_FIELDS) {
                return Ok(Err(reason));
            }
            let model_swap = match resolved_model_swap(base_plugin, &base_record)? {
                Ok(found) => found,
                Err(reason) => return Ok(Err(reason)),
            };
            // Leaf data sits in the STAT's own DNAM, as for SCOL components.
            let source = PlannedSource {
                model: context.sources.model(
                    &model,
                    context.assets,
                    reference_swap,
                    model_swap.as_ref(),
                    model_color_index_of(base_plugin, &base_record)?,
                )?,
                component: None,
                base_record_flags: base_record.flags,
                leaf: leaf_of(base_plugin, &base_record)?,
                editor_id: editor_id_of(base_plugin, &base_record)?,
                merges_collision: false,
            };
            return Ok(Ok((vec![source], PreparedBoundKind::Source)));
        }
        if let Err(reason) = check_fields(base_plugin, &base_record, &SCOL_FIELDS) {
            return Ok(Err(reason));
        }
        let mut expanded = Vec::new();
        for (component_raw, component) in scol_components(base_plugin, &base_record)? {
            let Some((component_plugin, component_record)) = context.load_order.resolve(base_plugin, component_raw)
            else {
                return Ok(Err(format!("SCOL component {component_raw:08X} is unresolved")));
            };
            if &component_record.signature != b"STAT" {
                return Ok(Err("SCOL components must be STAT records".into()));
            }
            if let Err(reason) = check_fields(component_plugin, &component_record, &STAT_FIELDS) {
                return Ok(Err(reason));
            }
            let component_swap = match resolved_model_swap(component_plugin, &component_record)? {
                Ok(Some(_)) if reference_swap.is_some() => {
                    return Ok(Err("a SCOL reference swap over component material swaps is not yet supported".into()));
                }
                Ok(found) => found,
                Err(reason) => return Ok(Err(reason)),
            };
            let component_model = model_of(component_plugin, &component_record)?;
            expanded.push(PlannedSource {
                model: context.sources.model(
                    &component_model,
                    context.assets,
                    reference_swap,
                    component_swap.as_ref(),
                    model_color_index_of(component_plugin, &component_record)?,
                )?,
                component: Some(component),
                base_record_flags: base_record.flags,
                leaf: leaf_of(component_plugin, &component_record)?,
                editor_id: editor_id_of(component_plugin, &component_record)?,
                merges_collision: false,
            });
        }
        let relative = mesh_path(&model)?;
        let bytes = context
            .assets
            .read(&relative)?
            .ok_or_else(|| PrevisError::unsupported(format!("model not found: {relative}")))?;
        let composite = NifFile::from_bytes_raw_arrays(&bytes, None)
            .map_err(|e| PrevisError::invalid(format!("{relative}: NIF parse failed: {e}")))?;
        Ok(Ok((expanded, PreparedBoundKind::Composite(composite))))
    };
    let (mut expanded, bound) = match expand() {
        Ok(Ok(found)) => found,
        Ok(Err(reason)) => return Ok(Some(Err(reason))),
        Err(error) => return Ok(Some(Err(error.to_string()))),
    };
    // One shape failing `CanAddToCombinedShape` makes CK clone every collision
    // of the base (CELL 1294F4: exactly the 13 SCOL references CK clones).
    let base_merges = expanded
        .iter()
        .filter_map(|s| s.model.collision.as_ref()?.merged.as_ref())
        .flatten()
        .all(|(_, merged)| merged.fits_combined_shape);
    for source in &mut expanded {
        source.merges_collision = base_merges && source.model.collision.as_ref().is_some_and(|c| c.merged.is_some());
    }
    let animated_without_leaf = expanded
        .iter()
        .any(|s| s.leaf.is_none() && s.model.shapes.iter().any(|shape| shape.shader.flags2 & TREE_ANIM != 0));
    if animated_without_leaf {
        return Ok(Some(Err("tree-animated shape without leaf amplitude/frequency".into())));
    }
    if let Some(reason) = expanded.iter().find_map(|s| unbuildable_shape(&s.model)) {
        return Ok(Some(Err(reason)));
    }
    let unscalable = |s: &PlannedSource| {
        s.model.collision.as_ref().filter(|_| !s.merges_collision).and_then(|c| c.unscalable().map(str::to_string))
    };
    if let Some(reason) = expanded.iter().filter(|s| s.component.is_some_and(|c| c.scale != 1.0)).find_map(unscalable) {
        return Ok(Some(Err(reason)));
    }
    let unscalable_collision = expanded.iter().find_map(unscalable);
    let first = sources.len();
    let bound = match bound {
        PreparedBoundKind::Source => BoundModel::Source(expanded[0].model.clone()),
        PreparedBoundKind::Composite(nif) => BoundModel::Composite(bound_rows(&nif)?),
    };
    let export_info = if &base_record.signature == b"STAT" { EXPORT_INFO_STAT } else { EXPORT_INFO_SCOL };
    sources.extend(expanded);
    Ok(Some(Ok(PreparedBase {
        base: PlannedBase {
            sources: first..sources.len(),
            export_info,
        },
        bound,
        non_occluder: base_record.flags & BASE_FLAG_NON_OCCLUDER != 0,
        unscalable_collision,
    })))
}

enum PreparedBoundKind {
    Source,
    Composite(NifFile),
}

struct Candidate {
    form_id: u32,
    base: usize,
    transform: Transform,
    center: [f32; 3],
    radius: f32,
    mesh_id: u32,
}

/// A combinable reference's runtime bound, for oracle comparison tools.
#[derive(Debug, Clone, Serialize)]
pub struct CandidateBound {
    pub form_id: u32,
    pub base_form_id: u32,
    pub center: [f32; 3],
    pub radius: f32,
    pub non_occluder: bool,
    pub mesh_id: u32,
}

/// How a CELL's group keys are derived.
#[derive(Clone, Copy, Debug)]
enum KeySpace {
    Interior,
    Exterior { grid: (i32, i32), land_minimum: Option<f32> },
}

fn group_key(center: [f32; 3], radius: f32, non_occluder: bool, space: KeySpace) -> u32 {
    let (key, small) = match space {
        KeySpace::Interior => (spatial_group_key(center), false),
        KeySpace::Exterior { grid, land_minimum } => {
            (exterior_mask_key(center, radius, grid, land_minimum), radius < SMALL_RADIUS)
        }
    };
    key | if small { SMALL_GROUP_BIT } else { 0 } | if non_occluder { NON_OCCLUDER_GROUP_BIT } else { 0 }
}

struct Gathered {
    sources: Vec<PlannedSource>,
    prepared: Vec<PreparedBase>,
    /// Keyed candidates with their base FormIDs, in reference order.
    candidates: Vec<(Candidate, u32)>,
    excluded: Vec<ExcludedReference>,
}

fn gather(context: &Context, cell_form_id: u32) -> Result<Gathered> {
    let plugin = context.load_order.target();
    let record = plugin
        .record(cell_form_id)
        .filter(|r| &r.signature == b"CELL")
        .ok_or_else(|| PrevisError::invalid(format!("CELL {cell_form_id:08X} is not in {}", plugin.name)))?;
    let cell = records::read_cell(plugin, &record)?;
    let space = match (cell.interior, cell.grid) {
        (true, _) => KeySpace::Interior,
        (false, Some(grid)) => KeySpace::Exterior {
            grid,
            land_minimum: land_minimum(context, plugin, cell_form_id, grid)?,
        },
        (false, None) => return Err(PrevisError::invalid(format!("exterior CELL {cell_form_id:08X} has no grid"))),
    };
    let topology = plugin.cells.get(&cell_form_id).cloned().unwrap_or_default();
    let placed_persistent = context.exterior_persistent.get(&cell_form_id).map_or(&[][..], Vec::as_slice);
    // CK walks the CELL's loaded reference array; persistent children load
    // before temporary ones (inferred from static RE, not yet traced).
    let children = || placed_persistent.iter().chain(&topology.persistent).chain(&topology.temporary);
    for child in children() {
        let base = plugin.subrecords_at(child)?.first(b"NAME").and_then(|d| read_u32(d, 0));
        if let Some(raw) = base {
            let (owner, object_id) = plugin.owner_of(raw);
            if owner.eq_ignore_ascii_case("Fallout4.esm") && ROOM_MARKER_OBJECT_IDS.contains(&object_id) {
                return Err(PrevisError::unsupported("room/multibound-parented grouping is not reproduced"));
            }
        }
    }

    let mut sources = Vec::new();
    let mut prepared: Vec<PreparedBase> = Vec::new();
    let mut base_index: FxHashMap<(u32, Option<u32>), Option<std::result::Result<usize, String>>> = FxHashMap::default();
    let mut candidates = Vec::new();
    let mut excluded = Vec::new();
    for child in children() {
        if &child.signature != b"REFR" {
            continue;
        }
        let subrecords = plugin.subrecords_at(child)?;
        let base_raw = match subrecords.first(b"NAME").and_then(|d| read_u32(d, 0)) {
            Some(raw) => raw,
            None => continue,
        };
        let reference_swap = subrecords.first(b"XMSP").and_then(|d| read_u32(d, 0));
        let entry = match base_index.get(&(base_raw, reference_swap)) {
            Some(entry) => entry.clone(),
            None => {
                let entry = prepare_base(context, plugin, base_raw, reference_swap, &mut sources)?.map(|outcome| {
                    outcome.map(|base| {
                        prepared.push(base);
                        prepared.len() - 1
                    })
                });
                base_index.insert((base_raw, reference_swap), entry.clone());
                entry
            }
        };
        let base = match entry {
            None => continue,
            Some(Err(reason)) => {
                excluded.push(ExcludedReference { form_id: child.form_id, reason });
                continue;
            }
            Some(Ok(base)) => base,
        };
        let (_, transform) = match reference_gate(context, plugin, child)? {
            Ok(found) => found,
            Err(reason) => {
                excluded.push(ExcludedReference { form_id: child.form_id, reason });
                continue;
            }
        };
        if let Some(reason) = prepared[base].unscalable_collision.as_ref().filter(|_| transform.scale != 1.0) {
            excluded.push(ExcludedReference {
                form_id: child.form_id,
                reason: reason.clone(),
            });
            continue;
        }
        let (center, radius) = placed_bound(prepared[base].bound.rows(), &transform)?;
        let mesh_id = group_key(center, radius, prepared[base].non_occluder, space);
        candidates.push((
            Candidate {
                form_id: child.form_id,
                base,
                transform,
                center,
                radius,
                mesh_id,
            },
            base_raw,
        ));
    }
    Ok(Gathered {
        sources,
        prepared,
        candidates,
        excluded,
    })
}

/// Every combinable reference's bound and group key, before empty groups are
/// dropped; for oracle comparison tools.
pub fn candidate_bounds(context: &Context, cell_form_id: u32) -> Result<(Vec<CandidateBound>, Vec<ExcludedReference>)> {
    let gathered = gather(context, cell_form_id)?;
    let bounds = gathered
        .candidates
        .iter()
        .map(|(c, base_form_id)| CandidateBound {
            form_id: c.form_id,
            base_form_id: *base_form_id,
            center: c.center,
            radius: c.radius,
            non_occluder: gathered.prepared[c.base].non_occluder,
            mesh_id: c.mesh_id,
        })
        .collect();
    Ok((bounds, gathered.excluded))
}

pub fn plan_cell(context: &Context, cell_form_id: u32) -> Result<Option<CellPrecombine>> {
    let Gathered {
        sources,
        prepared,
        candidates,
        mut excluded,
    } = gather(context, cell_form_id)?;
    let mut candidates: Vec<Candidate> = candidates.into_iter().map(|(c, _)| c).collect();
    // CK output never shows a group holding only marker-culled models.
    let has_geometry = |c: &Candidate| sources[prepared[c.base].base.sources.clone()].iter().any(|s| !s.model.shapes.is_empty());
    let keys_with_geometry: FxHashSet<u32> = candidates.iter().filter(|c| has_geometry(c)).map(|c| c.mesh_id).collect();
    candidates.retain(|c| {
        let keep = keys_with_geometry.contains(&c.mesh_id);
        if !keep {
            excluded.push(ExcludedReference {
                form_id: c.form_id,
                reason: "a group without geometry is not verified".into(),
            });
        }
        keep
    });
    if candidates.is_empty() {
        return Ok(None);
    }
    let mut mesh_ids: Vec<u32> = candidates.iter().map(|c| c.mesh_id).collect();
    mesh_ids.sort_unstable();
    mesh_ids.dedup();
    let groups = mesh_ids
        .iter()
        .map(|&mesh_id| PlannedGroup {
            mesh_id,
            root_name: format!("{:08X}_{mesh_id:08X}_OC", cell_form_id & 0x00FF_FFFF),
            members: group_members(&candidates, mesh_id),
        })
        .collect();
    Ok(Some(CellPrecombine {
        cell_form_id,
        sources,
        bases: prepared.into_iter().map(|p| p.base).collect(),
        groups,
        placements: candidates.iter().map(|c| (c.base, c.transform)).collect(),
        xcri: Xcri {
            mesh_ids,
            reference_mesh_pairs: candidates.iter().map(|c| (c.form_id, c.mesh_id)).collect(),
        },
        excluded,
    }))
}

/// Instances per base in reverse candidate order (CK walks the array
/// backwards). Bases appear in first-seen order along that walk; the
/// multi-base order is not yet verified against CK.
fn group_members(candidates: &[Candidate], mesh_id: u32) -> Vec<GroupMember> {
    let mut members: Vec<GroupMember> = Vec::new();
    for candidate in candidates.iter().rev().filter(|c| c.mesh_id == mesh_id) {
        match members.iter_mut().find(|m| m.base == candidate.base) {
            Some(member) => member.instances.push(candidate.transform),
            None => members.push(GroupMember {
                base: candidate.base,
                instances: vec![candidate.transform],
            }),
        }
    }
    members
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn non_occluder_bases_key_into_their_own_group_class() {
        let center = [113.69702911376953, -8.496938789903652e-06, 211.31422424316406];
        assert_eq!(group_key(center, 10.0, false, KeySpace::Interior), 0x1600_0F31);
        assert_eq!(group_key(center, 10.0, true, KeySpace::Interior), 0x9600_0F31);
    }

    /// CK group keys of Appalachia CELL (-26,22) references 08384058,
    /// 0838440C and 0838451A: the size and non-occluder classes stack on the
    /// exterior mask key.
    #[test]
    fn exterior_keys_carry_size_and_non_occluder_classes() {
        let space = KeySpace::Exterior {
            grid: (-26, 22),
            land_minimum: Some(13112.0),
        };
        let key = |center, radius, non_occluder| group_key(center, radius, non_occluder, space);
        assert_eq!(key([-104289.453125, 93444.265625, 14893.13671875], 154.13095, false), 0x3989_CFA7);
        assert_eq!(key([-103540.390625, 93424.6640625, 15080.69140625], 46.969967, true), 0xF989_CFA7);
        assert_eq!(key([-103511.9765625, 93311.890625, 15077.0009765625], 91.81535, false), 0x7A8F_F4D8);
    }

    fn record(signature: &[u8; 4], form_id: u32, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = signature.to_vec();
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&form_id.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(payload);
        out
    }

    fn group(label: u32, kind: u32, body: &[u8]) -> Vec<u8> {
        let mut out = b"GRUP".to_vec();
        out.extend_from_slice(&((24 + body.len()) as u32).to_le_bytes());
        out.extend_from_slice(&label.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(body);
        out
    }

    fn field(signature: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = signature.to_vec();
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    fn reference(form_id: u32, position: [f32; 3]) -> Vec<u8> {
        let mut data = Vec::new();
        for value in position.iter().chain(&[0.0; 3]) {
            data.extend_from_slice(&value.to_le_bytes());
        }
        let payload = [field(b"NAME", &0x0000_0800u32.to_le_bytes()), field(b"DATA", &data)].concat();
        record(b"REFR", form_id, 0x400, &payload)
    }

    fn exterior_cell(form_id: u32, grid: (i32, i32)) -> Vec<u8> {
        let xclc = [grid.0.to_le_bytes(), grid.1.to_le_bytes()].concat();
        record(b"CELL", form_id, 0, &[field(b"DATA", &[0, 0]), field(b"XCLC", &xclc)].concat())
    }

    /// Interior CELL 800; world 900 with persistent CELL 901 (holding REFRs
    /// A01..A03) and grid CELLs 910 at (-1,1) and 911 at (0,0).
    pub(crate) fn world_fixture(dir: &std::path::Path) -> Plugin {
        const WORLD: u32 = 0x0000_0900;
        const PERSISTENT_CELL: u32 = 0x0000_0901;
        let interior = group(u32::from_le_bytes(*b"CELL"), 0, &record(b"CELL", 0x800, 0, &field(b"DATA", &[1, 0])));
        let persistent = group(
            PERSISTENT_CELL,
            6,
            &group(
                PERSISTENT_CELL,
                8,
                &[reference(0xA01, [-100.0, 5000.0, 0.0]), reference(0xA02, [4095.0, 10.0, 0.0]), reference(0xA03, [9000.0, 9000.0, 0.0])].concat(),
            ),
        );
        let block = group(0, 4, &group(0, 5, &[exterior_cell(0x910, (-1, 1)), exterior_cell(0x911, (0, 0))].concat()));
        let world_children = group(WORLD, 1, &[record(b"CELL", PERSISTENT_CELL, 0, &field(b"DATA", &[0, 0])), persistent, block].concat());
        let header = record(b"TES4", 0, 0, &field(b"HEDR", &[0; 12]));
        let world = record(b"WRLD", WORLD, 0, &field(b"EDID", b"Test\0"));
        let bytes = [header, interior, group(u32::from_le_bytes(*b"WRLD"), 0, &[world, world_children].concat())].concat();
        let path = dir.join("Test.esp");
        std::fs::write(&path, bytes).unwrap();
        Plugin::open(&path).unwrap()
    }

    /// Persistent REFRs live in the world's persistent CELL; each belongs to
    /// the grid CELL its position falls in, or to none.
    #[test]
    fn persistent_references_attach_to_the_cell_under_them() {
        let dir = tempfile::tempdir().unwrap();
        let plugin = world_fixture(dir.path());
        let placed = exterior_persistent_references(&plugin).unwrap();
        let ids = |cell: u32| placed.get(&cell).map(|refs| refs.iter().map(|r| r.form_id).collect::<Vec<_>>());
        assert_eq!(ids(0x910), Some(vec![0xA01]));
        assert_eq!(ids(0x911), Some(vec![0xA02]));
        assert_eq!(placed.len(), 2);
    }
}
