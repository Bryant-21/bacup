//! `FormKeyMapper` — translates source-game FormKeys to target-game FormKeys.
//!
//! Three resolution strategies (matching the Python layer):
//! - **vanilla_remap**: an EditorID match exists in the target masters → reuse it.
//! - **source_id_preserved**: no vanilla match, but the local object-id is still free → keep it.
//! - **new_allocation**: allocate the next sequential object-id under the output plugin.
//!
//! The EID index is caller-supplied via an iterator of `(eid_sym, FormKey, SigCode)` tuples,
//! so tests don't need real plugin handles.

use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::sym::{StringInterner, Sym};

// ---------------------------------------------------------------------------
// Public options / mode types
// ---------------------------------------------------------------------------

/// How the mapper behaves when a FormKey reference in a record has no mapping.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum ResolutionMode {
    /// Leave unmapped FormKeys as-is and continue (default). The fixup pass
    /// at the end of the conversion run replaces any that were resolved later.
    #[default]
    DeferAndFixup,
    /// Return `Err(MapperError::UnmappedFormKey)` immediately.
    Strict,
    /// Replace unmapped FormKeys with a null FormKey and push a warning Sym.
    NullAndWarn,
}

/// Options that control `FormKeyMapper` behaviour for one conversion run.
#[derive(Clone, Default)]
pub struct MapperOptions {
    /// Output plugin filename, e.g. `"MyMod.esp"`.
    pub output_plugin_name: String,
    /// Source plugin filename whose raw on-disk FormIDs are being translated.
    pub source_plugin_name: String,
    /// Source plugin master filenames in on-disk master-index order.
    pub source_master_names: Vec<String>,
    /// Target plugin master filenames in on-disk master-index order.
    pub target_master_names: Vec<String>,
    /// When `true`, prefer vanilla target records for matching EditorIDs.
    pub use_base_game_assets: bool,
    /// Signatures that must never be remapped to vanilla records by EditorID.
    pub vanilla_remap_blocked_signatures: Vec<String>,
    /// When `true`, preserve the source object-id in the output plugin when
    /// the id is not already occupied.
    pub preserve_source_ids: bool,
    /// First object-id to use for freshly generated records that cannot
    /// preserve their source id.
    pub generated_object_id_floor: u32,
    /// Controls what happens to FormKey references that have no mapping yet.
    pub resolution_mode: ResolutionMode,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors emitted by `FormKeyMapper::rewrite_record`.
#[derive(Debug)]
pub enum MapperError {
    /// A FormKey reference was not mapped and `resolution_mode == Strict`.
    UnmappedFormKey(FormKey),
}

impl std::fmt::Display for MapperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnmappedFormKey(fk) => write!(f, "unmapped FormKey: local=0x{:06X}", fk.local),
        }
    }
}

impl std::error::Error for MapperError {}

// ---------------------------------------------------------------------------
// First object-id allocated for new records (matches Python _FIRST_ALLOCATION_ID)
// ---------------------------------------------------------------------------
pub(crate) const FIRST_ALLOCATION_ID: u32 = 0x0000_0800;
const MAX_LOCAL_OBJECT_ID: u32 = 0x00FF_FFFF;

/// Object-id floor for synthesized ESS placed-actor specialization clones. Both
/// specialization passes draw synth source-ids from a shared monotonic counter
/// seeded here (see `MapperState::ess_clone_next_local`).
pub(crate) const ESS_CLONE_SYNTH_FLOOR: u32 = 0x00F0_0000;

pub(crate) fn allows_editor_id_vanilla_remap(sig: SigCode) -> bool {
    // Package templates are ABI-like: instances store positional inputs that
    // must match the referenced template definition.
    sig.as_str() != "PACK"
}

fn always_editor_id_vanilla_remap(sig: SigCode) -> bool {
    matches!(sig.as_str(), "LAYR")
}

pub(crate) fn is_static_marker_editor_id(sig: SigCode, editor_id: &str) -> bool {
    matches!(sig.as_str(), "STAT" | "SCOL" | "MSTT")
        && editor_id.to_ascii_lowercase().contains("marker")
}

pub(crate) fn mapper_allows_editor_id_vanilla_remap(options: &MapperOptions, sig: SigCode) -> bool {
    allows_editor_id_vanilla_remap(sig)
        && !options
            .vanilla_remap_blocked_signatures
            .iter()
            .any(|blocked| blocked.eq_ignore_ascii_case(sig.as_str()))
}

// ---------------------------------------------------------------------------
// MapperState — owned fields that persist across the whole conversion run
// ---------------------------------------------------------------------------

/// Owned state for a `FormKeyMapper`. Stored on `ConversionRun` so that
/// mappings survive across records and across translate→fixup phases.
///
/// Construct once (e.g. from master EID indices) at the start of `translate_all`,
/// then create short-lived `FormKeyMapper::from_state(state, interner)` views
/// for each record or fixup pass.
#[derive(Clone)]
pub struct MapperState {
    /// source FormKey → target FormKey.
    pub source_to_target: FxHashMap<FormKey, FormKey>,
    /// EID sym → list of (target FormKey, record SigCode) from target masters.
    pub target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>>,
    pub options: MapperOptions,
    /// Next object-id to assign when allocating fresh FormKeys.
    pub next_object_id: u32,
    /// Set of object-ids already claimed under the output plugin (avoids collisions).
    pub used_object_ids: rustc_hash::FxHashSet<u32>,
    /// Persistent registry of ESS placed-actor specialization clones, keyed by
    /// `(base_local, branch_target_local)`. A given (base, branch) maps to exactly
    /// one clone, including across repeated post-copy repair invocations, so an
    /// actor can never be repointed to a clone authored for a different base
    /// object (the turret-for-critter cross-wire). `ess_clone_next_local` is the
    /// shared monotonic synth-source counter for those clones.
    pub ess_clone_registry: FxHashMap<(u32, u32), FormKey>,
    pub ess_clone_next_local: u32,
}

impl MapperState {
    /// Build mapper state from an EID index iterator.
    ///
    /// `eid_iter` supplies `(eid_sym, target_form_key, sig_code)` tuples from
    /// every master plugin that should participate in vanilla-remap lookups.
    pub fn new(
        eid_iter: impl IntoIterator<Item = (Sym, FormKey, SigCode)>,
        options: MapperOptions,
    ) -> Self {
        let mut target_eid_index: FxHashMap<Sym, Vec<(FormKey, SigCode)>> = FxHashMap::default();
        for (eid, fk, sig) in eid_iter {
            target_eid_index.entry(eid).or_default().push((fk, sig));
        }
        let generated_floor = if options.generated_object_id_floor <= MAX_LOCAL_OBJECT_ID {
            options.generated_object_id_floor
        } else {
            0
        };
        MapperState {
            source_to_target: FxHashMap::default(),
            target_eid_index,
            options,
            next_object_id: generated_floor.max(FIRST_ALLOCATION_ID),
            used_object_ids: rustc_hash::FxHashSet::default(),
            ess_clone_registry: FxHashMap::default(),
            ess_clone_next_local: ESS_CLONE_SYNTH_FLOOR,
        }
    }
}

// ---------------------------------------------------------------------------
// FormKeyMapper — short-lived view over MapperState + interner borrow
// ---------------------------------------------------------------------------

/// Maps source-game `FormKey`s to target-game `FormKey`s for one conversion run.
///
/// Two constructors:
/// - `FormKeyMapper::new(eid_iter, opts, interner)` — convenience for tests and
///   one-shot uses: creates a fresh `MapperState` internally.
/// - `FormKeyMapper::from_state(state, interner)` — primary path for
///   `ConversionRun`: borrows the long-lived `MapperState` so mappings persist
///   across records and between translate and fixup phases.
pub struct FormKeyMapper<'a> {
    /// Interner shared with the caller; needed to intern/resolve plugin names and EIDs.
    pub interner: &'a StringInterner,
    /// Mutable reference into the long-lived state (or an inline owned Box).
    state: StateRef<'a>,
    /// Overlay mode (emit phase): reads fall back to this frozen base
    /// on scratch miss; writes stay in `state` (the scratch). Reproduces the
    /// legacy per-record `MapperState::clone()`-and-discard semantics without
    /// copying the multi-million-entry maps. `None` everywhere else.
    base: Option<&'a MapperState>,
}

/// Holds either a borrowed `MapperState` (from `from_state`) or an owned one
/// (from `new`) without requiring two separate structs.
enum StateRef<'a> {
    Borrowed(&'a mut MapperState),
    Owned(Box<MapperState>),
}

/// Read-only snapshot of mapper lookup tables for parallel decide phases.
#[derive(Clone)]
pub struct MapperSnapshot {
    source_to_target: Arc<FxHashMap<FormKey, FormKey>>,
    target_eid_index: Arc<FxHashMap<Sym, Vec<(FormKey, SigCode)>>>,
}

impl MapperSnapshot {
    pub fn lookup(&self, source: FormKey) -> Option<FormKey> {
        self.source_to_target.get(&source).copied()
    }

    pub fn find_target_for_eid_sig(&self, eid: Sym, sig: SigCode) -> Option<FormKey> {
        self.target_eid_index
            .get(&eid)?
            .iter()
            .find(|(_, candidate_sig)| *candidate_sig == sig)
            .map(|(fk, _)| *fk)
    }
}

/// Two-level read view over the source→target map for `walk_field_value`:
/// scratch (primary) first, frozen overlay base second. Outside overlay mode
/// `base` is `None` and this is exactly the legacy single-map read.
#[derive(Clone, Copy)]
pub(crate) struct MapView<'m> {
    primary: &'m FxHashMap<FormKey, FormKey>,
    base: Option<&'m FxHashMap<FormKey, FormKey>>,
}

impl<'m> MapView<'m> {
    pub(crate) fn get(&self, k: &FormKey) -> Option<FormKey> {
        self.primary
            .get(k)
            .copied()
            .or_else(|| self.base.and_then(|b| b.get(k).copied()))
    }
}

impl<'a> StateRef<'a> {
    fn as_mut(&mut self) -> &mut MapperState {
        match self {
            StateRef::Borrowed(s) => s,
            StateRef::Owned(b) => b.as_mut(),
        }
    }

    fn as_ref(&self) -> &MapperState {
        match self {
            StateRef::Borrowed(s) => s,
            StateRef::Owned(b) => b.as_ref(),
        }
    }
}

impl<'a> FormKeyMapper<'a> {
    /// Create a new mapper, owning a fresh `MapperState`.
    ///
    /// `eid_iter` supplies the EID index from target masters: each item is
    /// `(eid_sym, target_form_key, sig_code)`.  Pass an empty iterator when no
    /// master handles are available.
    ///
    /// This is a **convenience constructor** for tests and one-shot uses.
    /// For `ConversionRun` use `from_state` so mappings persist across records.
    pub fn new(
        eid_iter: impl IntoIterator<Item = (Sym, FormKey, SigCode)>,
        options: MapperOptions,
        interner: &'a StringInterner,
    ) -> Self {
        let state = MapperState::new(eid_iter, options);
        FormKeyMapper {
            interner,
            state: StateRef::Owned(Box::new(state)),
            base: None,
        }
    }

    /// Create a mapper view backed by an existing `MapperState`.
    ///
    /// All mappings accumulated by prior calls (`allocate_or_resolve`,
    /// `add_mapping`) are visible and persist after this view is dropped.
    pub fn from_state(state: &'a mut MapperState, interner: &'a StringInterner) -> Self {
        FormKeyMapper {
            interner,
            state: StateRef::Borrowed(state),
            base: None,
        }
    }

    /// Empty scratch carrying only the cheap template fields. Reads of the big
    /// maps go through the base; this exists so `st()` writes have a home.
    pub fn overlay_scratch(base: &MapperState) -> MapperState {
        MapperState {
            source_to_target: FxHashMap::default(),
            target_eid_index: FxHashMap::default(),
            options: base.options.clone(),
            next_object_id: base.next_object_id,
            used_object_ids: rustc_hash::FxHashSet::default(),
            ess_clone_registry: base.ess_clone_registry.clone(),
            ess_clone_next_local: base.ess_clone_next_local,
        }
    }

    /// Overlay-mode constructor: reads check `scratch` first, then fall back to
    /// the frozen `base`; writes go to `scratch`. Observationally identical to
    /// `from_state(&mut base.clone(), ...)` with the clone discarded afterwards.
    pub fn from_state_overlay(
        base: &'a MapperState,
        scratch: &'a mut MapperState,
        interner: &'a StringInterner,
    ) -> Self {
        FormKeyMapper {
            interner,
            state: StateRef::Borrowed(scratch),
            base: Some(base),
        }
    }

    fn st(&mut self) -> &mut MapperState {
        self.state.as_mut()
    }

    // ---------------------------------------------------------------------------
    // Union reads (scratch-then-base; base is None outside overlay mode)
    // ---------------------------------------------------------------------------

    fn map_get(&self, source: &FormKey) -> Option<FormKey> {
        self.state
            .as_ref()
            .source_to_target
            .get(source)
            .copied()
            .or_else(|| {
                self.base
                    .and_then(|b| b.source_to_target.get(source).copied())
            })
    }

    fn used_contains(&self, id: u32) -> bool {
        self.state.as_ref().used_object_ids.contains(&id)
            || self.base.is_some_and(|b| b.used_object_ids.contains(&id))
    }

    fn eid_matches(&self, eid: Sym, sig: SigCode) -> Option<FormKey> {
        let find = |st: &MapperState| {
            st.target_eid_index
                .get(&eid)
                .and_then(|m| m.iter().find(|(_, s)| *s == sig).map(|(fk, _)| *fk))
        };
        find(self.state.as_ref()).or_else(|| self.base.and_then(find))
    }

    // ---------------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------------

    pub fn output_plugin_sym(&mut self) -> Sym {
        let name = self.st().options.output_plugin_name.clone();
        self.interner.intern(&name)
    }

    fn allocate_local(&mut self) -> u32 {
        let mut next = self.state.as_ref().next_object_id;
        while self.used_contains(next) {
            next += 1;
        }
        let st = self.st();
        st.used_object_ids.insert(next);
        st.next_object_id = next + 1;
        next
    }

    // ---------------------------------------------------------------------------
    // Public API
    // ---------------------------------------------------------------------------

    /// Pre-register a known source→target mapping (e.g. loaded from a map file).
    pub fn add_mapping(&mut self, source: FormKey, target: FormKey) {
        debug_assert!(
            self.base.is_none(),
            "add_mapping not supported in overlay mode"
        );
        let obj = target.local;
        let plugin_name = self.st().options.output_plugin_name.to_ascii_lowercase();
        if let Some(target_plugin) = self.interner.resolve(target.plugin) {
            if target_plugin.to_ascii_lowercase() == plugin_name {
                self.st().used_object_ids.insert(obj);
            }
        }
        self.st().source_to_target.insert(source, target);
    }

    /// Mark output-plugin object-ids as claimed without recording a mapping.
    ///
    /// Used by the slice (`identity_resolve`) encounter-zone path: a fresh run
    /// has no `translate_all` state, so `used_object_ids` is empty and
    /// `allocate_or_resolve` would happily reuse a preserved source-id that the
    /// target already occupies. Reserving every existing target id forces fresh
    /// allocations above the floor instead.
    pub fn reserve_object_ids(&mut self, ids: impl IntoIterator<Item = u32>) {
        debug_assert!(
            self.base.is_none(),
            "reserve_object_ids not supported in overlay mode"
        );
        let st = self.st();
        st.used_object_ids.extend(ids);
    }

    /// Look up the ESS specialization clone already minted for `(base_local,
    /// branch_target_local)`, if one was created in this or an earlier pass.
    pub fn ess_clone_lookup(&self, key: (u32, u32)) -> Option<FormKey> {
        self.state
            .as_ref()
            .ess_clone_registry
            .get(&key)
            .copied()
            .or_else(|| {
                self.base
                    .and_then(|b| b.ess_clone_registry.get(&key).copied())
            })
    }

    /// Reserve the next synthetic source object-id for a fresh ESS clone. Monotonic
    /// and persistent across passes so two passes never allocate the same synth
    /// source key (which `allocate_or_resolve` would otherwise memoize into a shared
    /// clone target, cross-wiring actors of different bases onto one clone).
    pub fn ess_clone_next_source_local(&mut self) -> u32 {
        let st = self.st();
        let local = st.ess_clone_next_local;
        st.ess_clone_next_local = local.saturating_add(1);
        local
    }

    /// Record the clone minted for `(base_local, branch_target_local)`.
    pub fn ess_clone_register(&mut self, key: (u32, u32), clone: FormKey) {
        self.st().ess_clone_registry.insert(key, clone);
    }

    /// Resolve or allocate a target FormKey for `source`.
    ///
    /// If a mapping already exists it is returned unchanged.  Otherwise:
    /// 1. If `eid` is `Some` and `use_base_game_assets` is set, look for a
    ///    vanilla record with the same EditorID and signature in `target_eid_index`.
    /// 2. If `preserve_source_ids` is set and the source object-id is still free, reuse it.
    /// 3. Otherwise allocate the next sequential object-id.
    pub fn allocate_or_resolve(
        &mut self,
        source: FormKey,
        eid: Option<Sym>,
        sig: SigCode,
    ) -> FormKey {
        if let Some(existing) = self.map_get(&source) {
            trace_0247c1_resolution(self.interner, "preexisting", source, existing, sig);
            return existing;
        }

        // Vanilla remap via EID index.
        if let Some(eid_sym) = eid {
            let marker_static = self
                .interner
                .resolve(eid_sym)
                .is_some_and(|editor_id| is_static_marker_editor_id(sig, editor_id));
            if (self.st().options.use_base_game_assets
                || always_editor_id_vanilla_remap(sig)
                || marker_static)
                && (marker_static || mapper_allows_editor_id_vanilla_remap(&self.st().options, sig))
            {
                if let Some(target_fk) = self.eid_matches(eid_sym, sig) {
                    self.st().source_to_target.insert(source, target_fk);
                    trace_0247c1_resolution(self.interner, "eid_vanilla", source, target_fk, sig);
                    return target_fk;
                }
            }
        }

        // Allocate a local FormKey.
        let local = if self.st().options.preserve_source_ids
            && source.local >= FIRST_ALLOCATION_ID
            && !self.used_contains(source.local)
        {
            self.st().used_object_ids.insert(source.local);
            source.local
        } else {
            self.allocate_local()
        };

        let plugin = self.output_plugin_sym();
        let target = FormKey { local, plugin };
        self.st().source_to_target.insert(source, target);
        trace_0247c1_resolution(self.interner, "allocate", source, target, sig);
        target
    }

    /// Return the target `FormKey` already mapped to `source`, without allocating.
    ///
    /// Returns `None` when `source` has no mapping yet.  Use this for read-only
    /// queries (e.g. detecting unmapped references in fixup passes).
    pub fn lookup(&self, source: FormKey) -> Option<FormKey> {
        self.map_get(&source)
    }

    /// Iterate over all (source, target) FormKey pairs in the current mapping.
    ///
    /// Exposes the `source_to_target` table for read-only passes (e.g. building
    /// an inverse map from target FK back to source FK).
    pub fn source_to_target_iter(&self) -> impl Iterator<Item = (FormKey, FormKey)> + '_ {
        debug_assert!(
            self.base.is_none(),
            "source_to_target_iter not supported in overlay mode"
        );
        self.state
            .as_ref()
            .source_to_target
            .iter()
            .map(|(&src, &tgt)| (src, tgt))
    }

    pub fn as_read_snapshot(&self) -> MapperSnapshot {
        debug_assert!(
            self.base.is_none(),
            "as_read_snapshot not supported in overlay mode"
        );
        let state = self.state.as_ref();
        MapperSnapshot {
            source_to_target: Arc::new(state.source_to_target.clone()),
            target_eid_index: Arc::new(state.target_eid_index.clone()),
        }
    }

    /// Look up the vanilla (target-master) `FormKey` for `eid_str` and `sig`,
    /// unconditionally (ignoring `use_base_game_assets`).
    ///
    /// Mirrors Python `FormKeyMapper.find_vanilla(editor_id, record_type)`.
    /// Used by fixups that always need vanilla resolution (e.g. race filtering)
    /// regardless of the standalone-mod flag.
    ///
    /// Returns `None` when `eid_str` is empty or no match exists in the EID index.
    pub fn find_vanilla_fk(&mut self, eid_str: &str, sig: SigCode) -> Option<FormKey> {
        if eid_str.is_empty() {
            return None;
        }
        let eid_sym = self.interner.intern(&eid_str.to_ascii_lowercase());
        self.eid_matches(eid_sym, sig)
    }

    /// Walk every `FieldValue::FormKey` in `record` and replace it with the
    /// mapped target FormKey, according to `options.resolution_mode`.
    ///
    /// The record's own `form_key` field is NOT rewritten here — the caller
    /// controls that separately via `allocate_or_resolve`.
    ///
    /// Returns `Ok(())` on success, or `Err(MapperError::UnmappedFormKey)` when
    /// `resolution_mode == Strict` and an unmapped reference is encountered.
    pub fn rewrite_record(&mut self, record: &mut Record) -> Result<(), MapperError> {
        let mode = self.st().options.resolution_mode;
        let rec_sig = record.sig.0;
        for field in record.fields.iter_mut() {
            if field.sig.as_str() != "VMAD" {
                continue;
            }
            if let FieldValue::Bytes(bytes) = &mut field.value {
                self.rewrite_vmad_formids(bytes.as_mut_slice(), &rec_sig);
            }
        }
        let state = self.state.as_ref();
        let mapping = MapView {
            primary: &state.source_to_target,
            base: self.base.map(|b| &b.source_to_target),
        };
        for field in record.fields.iter_mut() {
            Self::walk_field_value(
                &mut field.value,
                mapping,
                mode,
                &mut record.warnings,
                self.interner,
            )?;
        }
        Ok(())
    }

    fn rewrite_vmad_formids(&mut self, data: &mut [u8], record_sig: &[u8; 4]) -> bool {
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
            let Some(script_changed) =
                self.rewrite_vmad_script_entry(data, &mut offset, object_format)
            else {
                return changed;
            };
            changed |= script_changed;
        }

        if offset < data.len() {
            let fragment_changed = match record_sig {
                b"INFO" | b"PACK" | b"SCEN" => {
                    self.rewrite_vmad_info_pack_scen_after_scripts(data, &mut offset, object_format)
                }
                b"PERK" | b"TERM" => {
                    self.rewrite_vmad_perk_term_after_scripts(data, &mut offset, object_format)
                }
                // QUST VMAD has a trailing fragment+alias section after the
                // scripts. The alias entries each contain an 8-byte VMAD object
                // plus their own embedded scripts.
                b"QUST" => self.rewrite_vmad_qust_after_scripts(data, &mut offset, object_format),
                _ => None,
            };
            if let Some(fragment_changed) = fragment_changed {
                changed |= fragment_changed;
            }
        }

        changed
    }

    fn rewrite_vmad_info_pack_scen_after_scripts(
        &mut self,
        data: &mut [u8],
        offset: &mut usize,
        object_format: u16,
    ) -> Option<bool> {
        advance(offset, 1, data.len())?; // i8 version
        advance(offset, 1, data.len())?; // u8 flags
        self.rewrite_vmad_script_entry(data, offset, object_format)
    }

    fn rewrite_vmad_perk_term_after_scripts(
        &mut self,
        data: &mut [u8],
        offset: &mut usize,
        object_format: u16,
    ) -> Option<bool> {
        advance(offset, 1, data.len())?; // i8 version
        self.rewrite_vmad_script_entry(data, offset, object_format)
    }

    /// Skips the QUST VMAD fragment section and remaps FormIDs in the alias
    /// section. Called after all top-level scripts have been walked.
    ///
    /// Layout (wbScriptFragmentsQuest + aliases):
    ///   i8 version
    ///   u16 fragment_count
    ///   u16-str script_name
    ///   if script_name non-empty: u8 flags, u16 prop_count, properties[]
    ///   fragment_count × [u16 stage, i16 unk, i32 stage_idx, i8 unk,
    ///                      u16-str script_name, u16-str fragment_name]
    ///   u16 alias_count
    ///   alias_count × [object(8 bytes), i16 version, i16 obj_format,
    ///                   u16 script_count, scripts[]]
    fn rewrite_vmad_qust_after_scripts(
        &mut self,
        data: &mut [u8],
        offset: &mut usize,
        object_format: u16,
    ) -> Option<bool> {
        // Skip i8 version + u16 fragment_count.
        advance(offset, 1, data.len())?; // i8 version
        let fragment_count = read_u16_advance(data, offset)? as usize;

        // Walk the fragment script header. It has the same layout as a VMAD
        // script entry (name + flags + properties) and may contain object-type
        // property FormIDs that need remapping.
        let script_name_len = read_u16_advance(data, offset)? as usize;
        if script_name_len > 0 {
            advance(offset, script_name_len, data.len())?; // script_name chars
            advance(offset, 1, data.len())?; // u8 flags
            let prop_count = read_u16_advance(data, offset)? as usize;
            for _ in 0..prop_count {
                skip_vmad_string(data, offset)?; // property name
                let prop_type = read_u8_advance(data, offset)?;
                advance(offset, 1, data.len())?; // u8 status/flags byte
                self.rewrite_vmad_property_value(data, offset, prop_type, object_format)?;
            }
        }

        // Skip fragment entries.
        for _ in 0..fragment_count {
            advance(offset, 2, data.len())?; // u16 stage
            advance(offset, 2, data.len())?; // i16 unknown
            advance(offset, 4, data.len())?; // i32 stage_index
            advance(offset, 1, data.len())?; // i8 unknown
            skip_vmad_string(data, offset)?; // script name
            skip_vmad_string(data, offset)?; // fragment name
        }

        // Walk alias entries — these contain FormIDs that need 00→07 remap.
        let alias_count = read_u16_advance(data, offset)? as usize;
        let mut changed = false;
        for _ in 0..alias_count {
            // Alias object: 8-byte VMAD object; formid position depends on
            // object_format (same layout as rewrite_vmad_object).
            let alias_changed = self.rewrite_vmad_object(data, offset, object_format)?;
            changed |= alias_changed;

            // Alias header: i16 version + i16 object_format.
            advance(offset, 2, data.len())?; // i16 version
            let alias_obj_format = read_u16_advance(data, offset)?;
            let alias_script_count = read_u16_advance(data, offset)? as usize;
            for _ in 0..alias_script_count {
                let sc = self.rewrite_vmad_script_entry(data, offset, alias_obj_format)?;
                changed |= sc;
            }
        }
        Some(changed)
    }

    fn rewrite_vmad_script_entry(
        &mut self,
        data: &mut [u8],
        offset: &mut usize,
        object_format: u16,
    ) -> Option<bool> {
        skip_vmad_string(data, offset)?;
        advance(offset, 1, data.len())?;
        let property_count = read_u16_advance(data, offset)? as usize;
        let mut changed = false;
        for _ in 0..property_count {
            let property_changed = self.rewrite_vmad_property_entry(data, offset, object_format)?;
            changed |= property_changed;
        }
        Some(changed)
    }

    fn rewrite_vmad_property_entry(
        &mut self,
        data: &mut [u8],
        offset: &mut usize,
        object_format: u16,
    ) -> Option<bool> {
        skip_vmad_string(data, offset)?;
        let property_type = read_u8_advance(data, offset)?;
        advance(offset, 1, data.len())?;
        self.rewrite_vmad_property_value(data, offset, property_type, object_format)
    }

    fn rewrite_vmad_property_value(
        &mut self,
        data: &mut [u8],
        offset: &mut usize,
        property_type: u8,
        object_format: u16,
    ) -> Option<bool> {
        match property_type {
            0 | 6 => Some(false),
            1 => self.rewrite_vmad_object(data, offset, object_format),
            2 => {
                skip_vmad_string(data, offset)?;
                Some(false)
            }
            3 | 4 => {
                advance(offset, 4, data.len())?;
                Some(false)
            }
            5 => {
                advance(offset, 1, data.len())?;
                Some(false)
            }
            7 => self.rewrite_vmad_struct(data, offset, object_format),
            11 => {
                let count = read_i32_advance(data, offset)?;
                if count < 0 {
                    return None;
                }
                let mut changed = false;
                for _ in 0..count {
                    changed |= self.rewrite_vmad_object(data, offset, object_format)?;
                }
                Some(changed)
            }
            12 => {
                let count = read_i32_advance(data, offset)?;
                if count < 0 {
                    return None;
                }
                for _ in 0..count {
                    skip_vmad_string(data, offset)?;
                }
                Some(false)
            }
            13 | 14 => {
                let count = read_i32_advance(data, offset)?;
                if count < 0 {
                    return None;
                }
                advance(offset, (count as usize).checked_mul(4)?, data.len())?;
                Some(false)
            }
            15 => {
                let count = read_i32_advance(data, offset)?;
                if count < 0 {
                    return None;
                }
                advance(offset, count as usize, data.len())?;
                Some(false)
            }
            16 => {
                advance(offset, 4, data.len())?;
                Some(false)
            }
            17 => {
                let count = read_i32_advance(data, offset)?;
                if count < 0 {
                    return None;
                }
                let mut changed = false;
                for _ in 0..count {
                    changed |= self.rewrite_vmad_struct(data, offset, object_format)?;
                }
                Some(changed)
            }
            _ => None,
        }
    }

    fn rewrite_vmad_struct(
        &mut self,
        data: &mut [u8],
        offset: &mut usize,
        object_format: u16,
    ) -> Option<bool> {
        let count = read_i32_advance(data, offset)?;
        if count < 0 {
            return None;
        }
        let mut changed = false;
        for _ in 0..count {
            skip_vmad_string(data, offset)?;
            let member_type = read_u8_advance(data, offset)?;
            advance(offset, 1, data.len())?;
            changed |=
                self.rewrite_vmad_property_value(data, offset, member_type, object_format)?;
        }
        Some(changed)
    }

    fn rewrite_vmad_object(
        &mut self,
        data: &mut [u8],
        offset: &mut usize,
        object_format: u16,
    ) -> Option<bool> {
        let formid_offset = if object_format == 2 {
            let formid_offset = (*offset).checked_add(4)?;
            advance(offset, 8, data.len())?;
            formid_offset
        } else {
            let formid_offset = *offset;
            advance(offset, 8, data.len())?;
            formid_offset
        };
        self.rewrite_vmad_formid_at(data, formid_offset)
    }

    pub fn rewrite_raw_formid_at(&mut self, data: &mut [u8], offset: usize) -> Option<bool> {
        let raw = read_u32(data, offset)?;
        let source_fk = self.source_formkey_for_raw_formid(raw)?;
        let target_fk = self.lookup(source_fk)?;
        let target_raw = self.raw_formid_for_target(target_fk)?;
        if target_raw == raw {
            return Some(false);
        }
        data.get_mut(offset..offset.checked_add(4)?)?
            .copy_from_slice(&target_raw.to_le_bytes());
        Some(true)
    }

    fn rewrite_vmad_formid_at(&mut self, data: &mut [u8], offset: usize) -> Option<bool> {
        self.rewrite_raw_formid_at(data, offset)
    }

    fn source_formkey_for_raw_formid(&self, raw: u32) -> Option<FormKey> {
        if raw == 0 {
            return None;
        }
        let object_id = raw & 0x00FF_FFFF;
        let index = ((raw >> 24) & 0xFF) as usize;
        let options = &self.state.as_ref().options;
        let plugin_name = if index < options.source_master_names.len() {
            options.source_master_names[index].as_str()
        } else if index == options.source_master_names.len()
            && !options.source_plugin_name.is_empty()
        {
            options.source_plugin_name.as_str()
        } else {
            return None;
        };
        Some(FormKey {
            local: object_id,
            plugin: self.interner.intern(plugin_name),
        })
    }

    fn raw_formid_for_target(&self, fk: FormKey) -> Option<u32> {
        if fk.local == 0 {
            return Some(0);
        }
        let plugin_name = self.interner.resolve(fk.plugin)?;
        let options = &self.state.as_ref().options;
        let index = if plugin_name.eq_ignore_ascii_case(&options.output_plugin_name) {
            options.target_master_names.len()
        } else {
            options
                .target_master_names
                .iter()
                .position(|name| name.eq_ignore_ascii_case(plugin_name))?
        };
        if index > 0xFF {
            return None;
        }
        Some(((index as u32) << 24) | (fk.local & 0x00FF_FFFF))
    }

    /// Recursively walk a `FieldValue`, replacing `FormKey` leaves.
    fn walk_field_value(
        value: &mut FieldValue,
        mapping: MapView<'_>,
        mode: ResolutionMode,
        warnings: &mut smallvec::SmallVec<[Sym; 2]>,
        interner: &StringInterner,
    ) -> Result<(), MapperError> {
        match value {
            FieldValue::FormKey(fk) => {
                if let Some(target) = mapping.get(fk) {
                    // TEMP instrumentation: catch the field-rewrite that
                    // turns the dropped GulperRace (0x110D23) into the FO4 STAT
                    // 0x0247C1, regardless of how source_to_target was seeded.
                    if (fk.local == 0x0011_0D23 || target.local == 0x0002_47C1)
                        && std::env::var_os("MODBOX_TRACE_0247C1").is_some()
                    {
                        eprintln!(
                            "[trace_0247c1] walk_field source={:06X} -> target={:06X}",
                            fk.local, target.local
                        );
                    }
                    if crate::drop_trace::enabled()
                        && (crate::drop_trace::is_dlc_kw_watched(fk.local)
                            || crate::drop_trace::is_dlc_kw_watched(target.local))
                    {
                        let sp = interner.resolve(fk.plugin).unwrap_or("?");
                        let tp = interner.resolve(target.plugin).unwrap_or("?");
                        crate::drop_trace::trace(
                            "mapper_map",
                            "",
                            fk.local,
                            "",
                            &format!("{sp}:{:06X} -> {tp}:{:06X}", fk.local, target.local),
                        );
                    }
                    *fk = target;
                } else {
                    if crate::drop_trace::enabled()
                        && crate::drop_trace::is_dlc_kw_watched(fk.local)
                    {
                        let sp = interner.resolve(fk.plugin).unwrap_or("?");
                        crate::drop_trace::trace(
                            "mapper_unmapped",
                            "",
                            fk.local,
                            "",
                            &format!("{sp}:{:06X} mode={mode:?}", fk.local),
                        );
                    }
                    match mode {
                        ResolutionMode::Strict => {
                            return Err(MapperError::UnmappedFormKey(*fk));
                        }
                        ResolutionMode::NullAndWarn => {
                            let warn_msg = format!("unmapped FK local=0x{:06X}", fk.local);
                            let warn_sym = interner.intern(&warn_msg);
                            warnings.push(warn_sym);
                            let null_plugin = interner.intern("__null__");
                            *fk = FormKey {
                                local: 0,
                                plugin: null_plugin,
                            };
                        }
                        ResolutionMode::DeferAndFixup => {
                            // Leave as-is.
                        }
                    }
                }
            }
            FieldValue::List(items) => {
                for item in items.iter_mut() {
                    Self::walk_field_value(item, mapping, mode, warnings, interner)?;
                }
            }
            FieldValue::Struct(fields) => {
                for (_, v) in fields.iter_mut() {
                    Self::walk_field_value(v, mapping, mode, warnings, interner)?;
                }
            }
            // All other variants carry no FormKey references.
            _ => {}
        }
        Ok(())
    }
}

/// TEMP instrumentation: log any FK resolution whose source is the
/// dropped FO76 GulperRace (0x110D23) or whose target lands on the FO4 STAT
/// IndSilo32Top01 (0x0247C1) that NPC RNAM/ATKR wrongly collapse to. Gated on
/// the `MODBOX_TRACE_0247C1` env var so a normal run is unaffected. Remove once
/// the collapse path is identified.
fn trace_0247c1_resolution(
    interner: &StringInterner,
    branch: &str,
    source: FormKey,
    target: FormKey,
    sig: SigCode,
) {
    if std::env::var_os("MODBOX_TRACE_0247C1").is_none() {
        return;
    }
    // Trace BOTH the broken GulperRace (110D23->0247C1) and the WORKING
    // GulperSmallRace (111655->04E28E) so the two can be diffed side by side.
    if !matches!(source.local, 0x0011_0D23 | 0x0011_1655)
        && !matches!(target.local, 0x0002_47C1 | 0x0004_E28E)
    {
        return;
    }
    let src_plugin = interner.resolve(source.plugin).unwrap_or("?");
    let tgt_plugin = interner.resolve(target.plugin).unwrap_or("?");
    eprintln!(
        "[trace_0247c1] branch={branch} sig={} source={src_plugin}:{:06X} -> target={tgt_plugin}:{:06X}",
        sig.as_str(),
        source.local,
        target.local,
    );
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
    let bytes = data.get(*offset..(*offset).checked_add(4)?)?;
    *offset = (*offset).checked_add(4)?;
    Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn skip_vmad_string(data: &[u8], offset: &mut usize) -> Option<()> {
    let len = read_u16_advance(data, offset)? as usize;
    advance(offset, len, data.len())
}

fn advance(offset: &mut usize, amount: usize, len: usize) -> Option<()> {
    let next = (*offset).checked_add(amount)?;
    if next > len {
        return None;
    }
    *offset = next;
    Some(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, Record};
    use crate::sym::StringInterner;

    fn weap_sig() -> SigCode {
        SigCode::from_str("WEAP").unwrap()
    }

    fn parse_fk(s: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(s, interner).unwrap()
    }

    fn push_vmad_string(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&(value.len() as u16).to_le_bytes());
        out.extend_from_slice(value.as_bytes());
    }

    fn simple_vmad_object_property(raw_formid: u32) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        let formid_offset = push_vmad_object_script_entry(&mut out, raw_formid);
        (out, formid_offset)
    }

    fn push_vmad_object_script_entry(out: &mut Vec<u8>, raw_formid: u32) -> usize {
        push_vmad_string(out, "ModScript");
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        push_vmad_string(out, "Target");
        out.push(1);
        out.push(0);
        let formid_offset = out.len() + 4;
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0i16.to_le_bytes());
        out.extend_from_slice(&raw_formid.to_le_bytes());
        formid_offset
    }

    fn fragment_vmad_object_property(sig: &[u8; 4], raw_formid: u32) -> (Vec<u8>, usize) {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        match sig {
            b"INFO" | b"PACK" | b"SCEN" => {
                out.push(4); // fragment version
                out.push(0); // flags: no fragment rows
                let formid_offset = push_vmad_object_script_entry(&mut out, raw_formid);
                if sig == b"SCEN" {
                    out.extend_from_slice(&0u16.to_le_bytes()); // phase fragment count
                }
                (out, formid_offset)
            }
            b"PERK" | b"TERM" => {
                out.push(4); // fragment version
                let formid_offset = push_vmad_object_script_entry(&mut out, raw_formid);
                out.extend_from_slice(&0u16.to_le_bytes()); // fragment count
                (out, formid_offset)
            }
            other => panic!("unsupported fragment VMAD sig: {:?}", other),
        }
    }

    fn raw_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    // ------------------------------------------------------------------
    // Basic allocation and idempotency
    // ------------------------------------------------------------------

    #[test]
    fn mapper_allocates_fresh_for_new_record() {
        let mut interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Mod.esp".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        let source = parse_fk("000800@Mod.esm", &mut mapper.interner);
        let sig = weap_sig();
        let target = mapper.allocate_or_resolve(source, None, sig);
        assert_eq!(mapper.interner.resolve(target.plugin).unwrap(), "Mod.esp");
        assert!(target.local < 0x0100_0000);
    }

    #[test]
    fn generated_object_id_floor_starts_fresh_allocations() {
        let mut interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Mod.esp".to_string(),
                preserve_source_ids: true,
                generated_object_id_floor: 0x00A0_0000,
                ..Default::default()
            },
            &mut interner,
        );
        let sig = weap_sig();
        let preserved_source = parse_fk("000900@Source.esm", &mut mapper.interner);
        let collision_source = parse_fk("000900@Other.esm", &mut mapper.interner);

        let preserved = mapper.allocate_or_resolve(preserved_source, None, sig);
        let allocated = mapper.allocate_or_resolve(collision_source, None, sig);

        assert_eq!(preserved.local, 0x000900);
        assert_eq!(allocated.local, 0x00A0_0000);
    }

    #[test]
    fn reserved_ids_block_source_id_preservation() {
        let mut interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Mod.esp".to_string(),
                preserve_source_ids: true,
                generated_object_id_floor: 0x00A0_0000,
                ..Default::default()
            },
            &mut interner,
        );
        // The target already occupies 000900; reserving it must stop a
        // preserve_source_ids allocation from stealing that id.
        mapper.reserve_object_ids([0x000900]);
        let source = parse_fk("000900@Source.esm", &mut mapper.interner);
        let target = mapper.allocate_or_resolve(source, None, weap_sig());
        assert_ne!(target.local, 0x000900);
        assert_eq!(target.local, 0x00A0_0000);
    }

    #[test]
    fn mapper_reuses_existing_mapping() {
        let mut interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Mod.esp".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        let source = parse_fk("000800@Mod.esm", &mut mapper.interner);
        let sig = weap_sig();
        let t1 = mapper.allocate_or_resolve(source, None, sig);
        let t2 = mapper.allocate_or_resolve(source, None, sig);
        assert_eq!(t1, t2);
    }

    #[test]
    fn mapper_eid_match_returns_vanilla_target() {
        let mut interner = StringInterner::new();
        // Pre-build the EID sym and target FK using the same interner.
        let eid_sym = interner.intern("WeaponPistol");
        let target_fk = parse_fk("001234@Fallout4.esm", &mut interner);
        let sig = weap_sig();

        let eid_iter = [(eid_sym, target_fk, sig)];
        let mut mapper = FormKeyMapper::new(
            eid_iter,
            MapperOptions {
                output_plugin_name: "Mod.esp".to_string(),
                use_base_game_assets: true,
                ..Default::default()
            },
            &mut interner,
        );
        let source = parse_fk("000A00@Mod.esm", &mut mapper.interner);
        // Intern the EID sym again to get the same Sym handle.
        let eid = mapper.interner.intern("WeaponPistol");
        let result = mapper.allocate_or_resolve(source, Some(eid), sig);
        assert_eq!(result, target_fk);
    }

    #[test]
    fn mapper_layer_eid_match_returns_vanilla_target_with_base_assets_disabled() {
        let mut interner = StringInterner::new();
        let layr_sig = SigCode::from_str("LAYR").unwrap();
        let eid_sym = interner.intern("randomencounters");
        let target_fk = parse_fk("1870F4@Fallout4.esm", &mut interner);

        let mut mapper = FormKeyMapper::new(
            [(eid_sym, target_fk, layr_sig)],
            MapperOptions {
                output_plugin_name: "B21_TestMod.esm".to_string(),
                use_base_game_assets: false,
                preserve_source_ids: true,
                ..Default::default()
            },
            &mut interner,
        );
        let source = parse_fk("27E044@SeventySix.esm", &mut mapper.interner);
        let eid = mapper.interner.intern("randomencounters");
        let result = mapper.allocate_or_resolve(source, Some(eid), layr_sig);

        assert_eq!(result, target_fk);
    }

    #[test]
    fn mapper_does_not_eid_remap_package_templates() {
        let mut interner = StringInterner::new();
        let eid_sym = interner.intern("followplayer");
        let target_fk = parse_fk("02A105@Fallout4.esm", &mut interner);
        let pack_sig = SigCode::from_str("PACK").unwrap();

        let mut mapper = FormKeyMapper::new(
            [(eid_sym, target_fk, pack_sig)],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                ..Default::default()
            },
            &mut interner,
        );
        let source = parse_fk("407F9F@SeventySix.esm", &mut mapper.interner);
        let eid = mapper.interner.intern("followplayer");
        let result = mapper.allocate_or_resolve(source, Some(eid), pack_sig);

        assert_eq!(
            mapper.interner.resolve(result.plugin),
            Some("SeventySix.esm")
        );
        assert_eq!(result.local, 0x407F9F);
    }

    #[test]
    fn mapper_does_not_eid_remap_blocked_signature() {
        let mut interner = StringInterner::new();
        let eid_sym = interner.intern("TerrainShelfRocks01");
        let target_fk = parse_fk("012345@Fallout4.esm", &mut interner);
        let stat_sig = SigCode::from_str("STAT").unwrap();

        let mut mapper = FormKeyMapper::new(
            [(eid_sym, target_fk, stat_sig)],
            MapperOptions {
                output_plugin_name: "Output.esp".to_string(),
                use_base_game_assets: true,
                preserve_source_ids: true,
                vanilla_remap_blocked_signatures: vec!["STAT".into()],
                ..Default::default()
            },
            &mut interner,
        );
        let source = parse_fk("012345@SeventySix.esm", &mut mapper.interner);
        let eid = mapper.interner.intern("TerrainShelfRocks01");
        let result = mapper.allocate_or_resolve(source, Some(eid), stat_sig);

        assert_eq!(mapper.interner.resolve(result.plugin), Some("Output.esp"));
        assert_eq!(result.local, 0x012345);
    }

    #[test]
    fn mapper_remaps_static_marker_even_when_signature_blocked() {
        let mut interner = StringInterner::new();
        let eid_sym = interner.intern("xmarker");
        let target_fk = parse_fk("00003B@Fallout4.esm", &mut interner);
        let stat_sig = SigCode::from_str("STAT").unwrap();

        let mut mapper = FormKeyMapper::new(
            [(eid_sym, target_fk, stat_sig)],
            MapperOptions {
                output_plugin_name: "Output.esp".to_string(),
                use_base_game_assets: false,
                preserve_source_ids: true,
                vanilla_remap_blocked_signatures: vec!["STAT".into()],
                ..Default::default()
            },
            &mut interner,
        );
        let source = parse_fk("00003B@SeventySix.esm", &mut mapper.interner);
        let eid = mapper.interner.intern("xmarker");
        let result = mapper.allocate_or_resolve(source, Some(eid), stat_sig);

        assert_eq!(result, target_fk);
    }

    #[test]
    fn find_vanilla_fk_matches_normalized_editor_id() {
        let mut interner = StringInterner::new();
        let eid_sym = interner.intern("weaponpistol");
        let target_fk = parse_fk("001234@Fallout4.esm", &mut interner);
        let sig = weap_sig();

        let mut mapper = FormKeyMapper::new(
            [(eid_sym, target_fk, sig)],
            MapperOptions {
                output_plugin_name: "Mod.esp".to_string(),
                use_base_game_assets: true,
                ..Default::default()
            },
            &mut interner,
        );

        assert_eq!(mapper.find_vanilla_fk("WeaponPistol", sig), Some(target_fk));
    }

    // ------------------------------------------------------------------
    // rewrite_record field walker
    // ------------------------------------------------------------------

    #[test]
    fn rewrite_record_replaces_nested_formkeys() {
        let mut interner = StringInterner::new();
        let source_fk = parse_fk("000A00@Mod.esm", &mut interner);
        let target_fk = parse_fk("000A00@Mod.esp", &mut interner);

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Mod.esp".into(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);

        let ammo_sym = mapper.interner.intern("ammo");
        let mut record = Record::new(weap_sig(), parse_fk("000800@Mod.esm", &mut mapper.interner));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Struct(vec![(ammo_sym, FieldValue::FormKey(source_fk))]),
        });

        mapper.rewrite_record(&mut record).unwrap();

        if let FieldValue::Struct(fields) = &record.fields[0].value {
            assert_eq!(fields[0].1, FieldValue::FormKey(target_fk));
        } else {
            panic!("expected Struct");
        }
    }

    // ------------------------------------------------------------------
    // Resolution mode tests
    // ------------------------------------------------------------------

    #[test]
    fn rewrite_strict_mode_errors_on_unmapped() {
        let mut interner = StringInterner::new();
        let unmapped_fk = parse_fk("000B00@Mod.esm", &mut interner);

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Mod.esp".into(),
                resolution_mode: ResolutionMode::Strict,
                ..Default::default()
            },
            &mut interner,
        );

        let mut record = Record::new(weap_sig(), parse_fk("000800@Mod.esm", &mut mapper.interner));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KNAM").unwrap(),
            value: FieldValue::FormKey(unmapped_fk),
        });

        let result = mapper.rewrite_record(&mut record);
        assert!(matches!(result, Err(MapperError::UnmappedFormKey(_))));
    }

    #[test]
    fn rewrite_null_and_warn_replaces_with_null_and_pushes_warning() {
        let mut interner = StringInterner::new();
        let unmapped_fk = parse_fk("000C00@Mod.esm", &mut interner);

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Mod.esp".into(),
                resolution_mode: ResolutionMode::NullAndWarn,
                ..Default::default()
            },
            &mut interner,
        );

        let mut record = Record::new(weap_sig(), parse_fk("000800@Mod.esm", &mut mapper.interner));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KNAM").unwrap(),
            value: FieldValue::FormKey(unmapped_fk),
        });

        mapper.rewrite_record(&mut record).unwrap();

        // The FK should now be null (local == 0).
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_eq!(fk.local, 0);
        } else {
            panic!("expected FormKey variant");
        }
        // A warning should have been pushed.
        assert!(!record.warnings.is_empty());
    }

    #[test]
    fn rewrite_defer_and_fixup_leaves_unmapped_intact() {
        let mut interner = StringInterner::new();
        let unmapped_fk = parse_fk("000D00@Mod.esm", &mut interner);

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Mod.esp".into(),
                resolution_mode: ResolutionMode::DeferAndFixup,
                ..Default::default()
            },
            &mut interner,
        );

        let mut record = Record::new(weap_sig(), parse_fk("000800@Mod.esm", &mut mapper.interner));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("KNAM").unwrap(),
            value: FieldValue::FormKey(unmapped_fk),
        });

        mapper.rewrite_record(&mut record).unwrap();

        // FK left as-is, no warnings.
        if let FieldValue::FormKey(fk) = &record.fields[0].value {
            assert_eq!(*fk, unmapped_fk);
        } else {
            panic!("expected FormKey variant");
        }
        assert!(record.warnings.is_empty());
    }

    #[test]
    fn rewrite_record_rewrites_raw_vmad_object_formids() {
        let mut interner = StringInterner::new();
        let source_fk = parse_fk("001234@Source.esm", &interner);
        let target_fk = parse_fk("001234@Output.esm", &interner);

        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esm".into(),
                source_plugin_name: "Source.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..Default::default()
            },
            &interner,
        );
        mapper.add_mapping(source_fk, target_fk);

        let (vmad, formid_offset) = simple_vmad_object_property(0x0000_1234);
        let mut record = Record::new(weap_sig(), parse_fk("002000@Source.esm", &interner));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("VMAD").unwrap(),
            value: FieldValue::Bytes(smallvec::SmallVec::from_vec(vmad)),
        });

        mapper.rewrite_record(&mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected VMAD bytes");
        };
        let rewritten =
            u32::from_le_bytes(bytes[formid_offset..formid_offset + 4].try_into().unwrap());
        assert_eq!(rewritten, 0x0100_1234);
    }

    #[test]
    fn rewrite_record_rewrites_fragment_vmad_object_formids() {
        for sig_str in ["INFO", "PACK", "SCEN", "TERM"] {
            let interner = StringInterner::new();
            let source_fk = parse_fk("001234@Source.esm", &interner);
            let target_fk = parse_fk("001234@Output.esm", &interner);
            let mut mapper = FormKeyMapper::new(
                [],
                MapperOptions {
                    output_plugin_name: "Output.esm".into(),
                    source_plugin_name: "Source.esm".into(),
                    target_master_names: vec!["Fallout4.esm".into()],
                    ..Default::default()
                },
                &interner,
            );
            mapper.add_mapping(source_fk, target_fk);

            let sig = SigCode::from_str(sig_str).unwrap();
            let (vmad, formid_offset) = fragment_vmad_object_property(&sig.0, 0x0000_1234);
            let mut record = Record::new(sig, parse_fk("002000@Source.esm", &interner));
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("VMAD").unwrap(),
                value: FieldValue::Bytes(smallvec::SmallVec::from_vec(vmad)),
            });

            mapper.rewrite_record(&mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected VMAD bytes");
            };
            assert_eq!(raw_at(bytes, formid_offset), 0x0100_1234, "{sig_str}");
        }
    }

    // ------------------------------------------------------------------
    // Regression: mapper_state persists across two allocations
    // ------------------------------------------------------------------

    #[test]
    fn mapper_state_persists_cross_record() {
        // Simulate two records being processed in sequence using from_state.
        // Record 1: source_fk1 → allocated target_fk1
        // Record 2: has a FieldValue::FormKey(source_fk1) that must be rewritten
        //           to target_fk1 even though record 1 was processed first.
        let mut interner = StringInterner::new();
        let source_fk1 = parse_fk("001000@Source.esm", &mut interner);
        let source_fk2 = parse_fk("002000@Source.esm", &mut interner);
        let weap = weap_sig();

        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        );

        // Process record 1: allocate target for source_fk1.
        let target_fk1 = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            mapper.allocate_or_resolve(source_fk1, None, weap)
        };

        // Process record 2: its DNAM references source_fk1 from master.
        // The mapper (from_state) must still know about the record-1 mapping.
        let mut record2 = Record::new(weap, source_fk2);
        record2.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::FormKey(source_fk1),
        });
        {
            let mut mapper = FormKeyMapper::from_state(&mut state, &mut interner);
            mapper.rewrite_record(&mut record2).unwrap();
        }

        // The DNAM in record2 should now point at target_fk1 (record-1's allocation).
        if let FieldValue::FormKey(fk) = &record2.fields[0].value {
            assert_eq!(
                *fk, target_fk1,
                "cross-record FK rewrite must use persisted state"
            );
        } else {
            panic!("expected FormKey variant");
        }
    }

    #[test]
    fn mapper_snapshot_lookups_match_real_mapper() {
        let interner = StringInterner::new();
        let source_fk = parse_fk("001000@Source.esm", &interner);
        let sig = weap_sig();
        let mut state = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Output.esp".into(),
                ..Default::default()
            },
        );
        let target_fk = {
            let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
            mapper.allocate_or_resolve(source_fk, None, sig)
        };

        let mapper = FormKeyMapper::from_state(&mut state, &interner);
        let snapshot = mapper.as_read_snapshot();
        assert_eq!(snapshot.lookup(source_fk), Some(target_fk));
    }

    // ------------------------------------------------------------------
    // Overlay mode ≡ clone-and-discard semantics
    // ------------------------------------------------------------------

    #[test]
    fn overlay_mapper_matches_clone_semantics() {
        let interner = StringInterner::new();
        let opts = MapperOptions {
            output_plugin_name: "Out.esm".to_string(),
            preserve_source_ids: true,
            use_base_game_assets: true,
            generated_object_id_floor: 0x800,
            ..Default::default()
        };
        let src = |local: u32| FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        };
        let navm = SigCode::from_str("NAVM").unwrap();
        let eid_fk = FormKey {
            local: 0x1234,
            plugin: interner.intern("Fallout4.esm"),
        };
        let eid_sym = interner.intern("someeid");

        // Template with: one existing mapping, one used id, the EID index entry.
        let mut template = MapperState::new([(eid_sym, eid_fk, navm)], opts);
        {
            let mut m = FormKeyMapper::from_state(&mut template, &interner);
            m.allocate_or_resolve(src(0x111111), None, navm); // preserved -> 0x111111
        }

        // The op sequence the emit path performs, with every allocation class:
        // hit-base, preserve-new, low-id sequential, eid-vanilla.
        let ops = |m: &mut FormKeyMapper<'_>| -> Vec<FormKey> {
            vec![
                m.allocate_or_resolve(src(0x111111), None, navm), // base map hit
                m.allocate_or_resolve(src(0x222222), None, navm), // preserve path
                m.allocate_or_resolve(src(0x000014), None, navm), // low-id -> allocate_local
                m.allocate_or_resolve(src(0x333333), Some(eid_sym), navm), // eid vanilla
                m.lookup(src(0x222222)).unwrap(),                 // sees own scratch write
            ]
        };

        // (a) clone path (current emit behavior)
        let mut cloned = template.clone();
        let got_clone = ops(&mut FormKeyMapper::from_state(&mut cloned, &interner));

        // (b) overlay path
        let mut scratch = FormKeyMapper::overlay_scratch(&template);
        let got_overlay = ops(&mut FormKeyMapper::from_state_overlay(
            &template,
            &mut scratch,
            &interner,
        ));

        assert_eq!(got_clone, got_overlay);
        // discard semantics: the template never saw the new allocations
        assert!(template.source_to_target.get(&src(0x222222)).is_none());
        assert!(!template.used_object_ids.contains(&0x222222));
        assert!(template.source_to_target.get(&src(0x000014)).is_none());
        assert!(template.source_to_target.get(&src(0x333333)).is_none());
    }

    #[test]
    fn overlay_rewrite_record_matches_clone() {
        let interner = StringInterner::new();
        let source_fk = parse_fk("00A100@Source.esm", &interner);
        let target_fk = parse_fk("00A100@Out.esm", &interner);
        let mut template = MapperState::new(
            [],
            MapperOptions {
                output_plugin_name: "Out.esm".into(),
                ..Default::default()
            },
        );
        {
            // Seed the BASE map only — the overlay scratch starts empty, so this
            // mapping is visible solely through the base fallback.
            let mut m = FormKeyMapper::from_state(&mut template, &interner);
            m.add_mapping(source_fk, target_fk);
        }

        let make_record = || {
            let mut record = Record::new(weap_sig(), parse_fk("000800@Source.esm", &interner));
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("DNAM").unwrap(),
                value: FieldValue::FormKey(source_fk),
            });
            record
        };

        let mut record_clone = make_record();
        let mut cloned = template.clone();
        FormKeyMapper::from_state(&mut cloned, &interner)
            .rewrite_record(&mut record_clone)
            .unwrap();

        let mut record_overlay = make_record();
        let mut scratch = FormKeyMapper::overlay_scratch(&template);
        FormKeyMapper::from_state_overlay(&template, &mut scratch, &interner)
            .rewrite_record(&mut record_overlay)
            .unwrap();

        assert_eq!(record_clone.fields[0].value, record_overlay.fields[0].value);
        if let FieldValue::FormKey(fk) = &record_overlay.fields[0].value {
            assert_eq!(
                *fk, target_fk,
                "base-map mapping must be visible through the overlay"
            );
        } else {
            panic!("expected FormKey variant");
        }
    }
}
