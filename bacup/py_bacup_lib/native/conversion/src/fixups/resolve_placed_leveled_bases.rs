//! Fixup: resolve placed references whose base object is a leveled item list.
//!
//! # Why
//! FO76 allows a placed world reference (REFR / ACHR / PGRE / PHZD) to name a
//! **leveled item list (LVLI)** as its base object — the engine rolls the list at
//! spawn time. FO4 does NOT: a placed ref's `NAME` base must be a concrete object.
//! When such a ref survives conversion with its LVLI base intact, the CK reports
//! "Missing/Invalid base object for reference" and drops (or, for some types,
//! crashes on) the ref.
//!
//! The cell-slice copy path (`esp_authoring_core::cell_slice::replace_placed_lvli_base`)
//! already resolves this for EXTERIOR / projected placed children: it flattens the
//! LVLI and picks one concrete entry. The interior-cell emit path
//! (`run::emit_interior_cells`) uses a separate translate/insert path that never
//! invokes it, so interior placed-LVLI refs reach the output unresolved.
//!
//! # How
//! This pass runs in the POST-COPY hook (`ConversionRun::repair_placed_child_refs`),
//! where ALL placed children — interior and exterior — are present in the output
//! (REFR ∈ `skip_records`, so the in-phase fixups run before any placed child
//! exists; mirrors `strip_refr_placed_child_subrecords`). It is path-independent:
//! it inspects the finished output, so a future copy path can't bypass it.
//!
//! For each placed ref whose `NAME` base resolves to an output or target-master
//! LVLI, it flattens that LVLI to its valid placeable leaf entries (recursing
//! into nested LVLI, depth-bounded, cycle-guarded), then rewrites `NAME` to one
//! entry chosen by the SAME deterministic hash the cell-slice path uses (`seed =
//! (ref_raw << 32) ^ base_raw`), so the choice is reproducible across regens.
//! Already-resolved exterior refs have a non-LVLI base, so they are a no-op
//! (idempotent).

use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::ref_index::build_target_fk_sig_map;
use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

/// Placed-reference record signatures whose `NAME` names a base object. Matches
/// `cell_slice::is_placed_child_signature` so this pass covers exactly the records
/// the cell-slice copy path runs `replace_placed_lvli_base` on.
const PLACED_SIGS: &[&str] = &["REFR", "ACHR", "PHZD", "PGRE", "PGRD"];

/// Recursion bound for flattening nested leveled lists — matches the cell-slice
/// path (`append_leveled_item_entry_keys` depth >= 8 stop).
const MAX_LEVELED_DEPTH: usize = 8;

/// LVLO raw-entry FO4 reference FormID offset (entry payload is `[count?][FormID]`;
/// FO4 stores the reference at byte offset 4) — matches `clean_leveled_item_entries`.
const LVLO_REFERENCE_OFFSET: usize = 4;

/// Classification of a leveled-list entry's base object.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BaseKind {
    /// Itself the leveled-list type being flattened — recurse, never a leaf.
    Leveled,
    /// A resolvable base of a type valid for the placed record — a valid leaf.
    ValidBase,
    /// Null, an own-plugin object-id that was never emitted, or a resolvable base
    /// of a type that is not valid for the placed record — drop.
    Skip,
}

#[derive(Clone, Copy)]
struct SourcePluginInfo<'a> {
    schema: &'a AuthoringSchema,
    masters: &'a [String],
    own_name: &'a str,
    own_sym: Sym,
}

/// Deterministic SplitMix64-style index into a leveled list's entries. Ported
/// verbatim from `cell_slice::stable_leveled_entry_index` so interior and exterior
/// placed-LVLI refs pick by the same rule.
fn stable_leveled_entry_index(seed: u64, len: usize) -> usize {
    let mut value = seed ^ 0x9E37_79B9_7F4A_7C15;
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as usize) % len.max(1)
}

/// Classify a leveled-entry base FormKey against the output FK→sig index.
///
/// `leveled_sig` is the leveled-list type being flattened (`LVLI` for object
/// placements, `LVLN` for actor placements). `leaf_is_valid` decides whether a
/// resolved non-leveled base sig is a legal leaf for the placed record type — for
/// REFR this is "any non-LVLI object"; for ACHR it is "NPC_ only" (FO4 ACHR base
/// allows only NPC_).
fn classify_base(
    fk: &FormKey,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    own_sym: Sym,
    leveled_sig: &str,
    leaf_is_valid: &dyn Fn(&str) -> bool,
) -> BaseKind {
    if fk.local == 0 {
        return BaseKind::Skip;
    }
    match fk_to_sig.get(&(fk.local, fk.plugin)) {
        Some(sig) if sig.as_str() == leveled_sig => BaseKind::Leveled,
        Some(sig) if leaf_is_valid(sig.as_str()) => BaseKind::ValidBase,
        // Resolvable but the wrong type for this placed record (e.g. a STAT under
        // the ACHR NPC_-only rule) → not a usable leaf.
        Some(_) => BaseKind::Skip,
        // Not in the output index: an own-plugin object-id that no record was
        // emitted for was dropped → Skip; a master object-id is a vanilla concrete
        // base we don't index → assume valid (mirrors cell-slice non-own=valid).
        None if fk.plugin == own_sym => BaseKind::Skip,
        None => BaseKind::ValidBase,
    }
}

/// Flatten a leveled list to its valid placeable leaf bases (deduped, stable
/// order), recursing into nested LVLI entries (depth-bounded, cycle-guarded).
/// `entries_of` yields a list's direct entry FormKeys; `classify` reports each.
fn collect_leveled_leaf_bases(
    base: &FormKey,
    entries_of: &dyn Fn(&FormKey) -> Vec<FormKey>,
    classify: &dyn Fn(&FormKey) -> BaseKind,
) -> Vec<FormKey> {
    let mut out = Vec::new();
    let mut visited = FxHashSet::default();
    collect_recurse(base, entries_of, classify, &mut visited, 0, &mut out);
    out
}

fn collect_recurse(
    node: &FormKey,
    entries_of: &dyn Fn(&FormKey) -> Vec<FormKey>,
    classify: &dyn Fn(&FormKey) -> BaseKind,
    visited: &mut FxHashSet<FormKey>,
    depth: usize,
    out: &mut Vec<FormKey>,
) {
    if depth >= MAX_LEVELED_DEPTH || !visited.insert(*node) {
        return;
    }
    for entry in entries_of(node) {
        match classify(&entry) {
            BaseKind::ValidBase => {
                if !out.contains(&entry) {
                    out.push(entry);
                }
            }
            BaseKind::Leveled => {
                collect_recurse(&entry, entries_of, classify, visited, depth + 1, out)
            }
            BaseKind::Skip => {}
        }
    }
}

fn source_key_for_target_leveled(
    target_fk: FormKey,
    target_to_source: &FxHashMap<FormKey, FormKey>,
    target_own_sym: Sym,
    source_own_sym: Sym,
) -> Option<FormKey> {
    target_to_source.get(&target_fk).copied().or_else(|| {
        (target_fk.plugin == target_own_sym).then_some(FormKey {
            local: target_fk.local,
            plugin: source_own_sym,
        })
    })
}

fn resolve_source_leaf_to_target(
    source_leaf: FormKey,
    mapper: &FormKeyMapper<'_>,
    source_own_sym: Sym,
    target_own_sym: Sym,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    own_sym: Sym,
    leveled_sig: &str,
    leaf_is_valid: &dyn Fn(&str) -> bool,
) -> Option<FormKey> {
    if let Some(mapped) = mapper.lookup(source_leaf) {
        if classify_base(&mapped, fk_to_sig, own_sym, leveled_sig, leaf_is_valid)
            == BaseKind::ValidBase
        {
            return Some(mapped);
        }
    }
    if source_leaf.plugin != source_own_sym {
        return None;
    }
    let same_local_target = FormKey {
        local: source_leaf.local,
        plugin: target_own_sym,
    };
    (classify_base(
        &same_local_target,
        fk_to_sig,
        own_sym,
        leveled_sig,
        leaf_is_valid,
    ) == BaseKind::ValidBase)
        .then_some(same_local_target)
}

fn collect_source_leveled_leaf_bases(
    base: &FormKey,
    source_entries_of: &mut dyn FnMut(&FormKey) -> Vec<(FormKey, Option<SigCode>)>,
    resolve_leaf: &dyn Fn(FormKey) -> Option<FormKey>,
    leveled_sig: &str,
) -> Vec<FormKey> {
    let mut out = Vec::new();
    let mut visited = FxHashSet::default();
    collect_source_recurse(
        base,
        source_entries_of,
        resolve_leaf,
        leveled_sig,
        &mut visited,
        0,
        &mut out,
    );
    out
}

fn collect_source_recurse(
    node: &FormKey,
    source_entries_of: &mut dyn FnMut(&FormKey) -> Vec<(FormKey, Option<SigCode>)>,
    resolve_leaf: &dyn Fn(FormKey) -> Option<FormKey>,
    leveled_sig: &str,
    visited: &mut FxHashSet<FormKey>,
    depth: usize,
    out: &mut Vec<FormKey>,
) {
    if depth >= MAX_LEVELED_DEPTH || !visited.insert(*node) {
        return;
    }
    for (source_entry, source_sig) in source_entries_of(node) {
        if matches!(source_sig, Some(sig) if sig.as_str() == leveled_sig) {
            collect_source_recurse(
                &source_entry,
                source_entries_of,
                resolve_leaf,
                leveled_sig,
                visited,
                depth + 1,
                out,
            );
            continue;
        }
        if let Some(target_leaf) = resolve_leaf(source_entry) {
            if !out.contains(&target_leaf) {
                out.push(target_leaf);
            }
        }
    }
}

/// Encode a FormKey to its target-encoded `(load_index << 24) | object_id`, using
/// the output master table (`own_name` is the output plugin = highest load index).
/// Returns `None` if the plugin is neither the output nor a known master.
fn encode_form_id(
    fk: &FormKey,
    masters: &[String],
    own_name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    let name = interner.resolve(fk.plugin)?;
    let load_index = if name.eq_ignore_ascii_case(own_name) {
        masters.len()
    } else {
        masters.iter().position(|m| m.eq_ignore_ascii_case(name))?
    };
    if load_index > 0xFF {
        return None;
    }
    Some(((load_index as u32) << 24) | (fk.local & 0x00FF_FFFF))
}

/// Decode a target-encoded raw FormID back to a FormKey via the output master
/// table. Returns `None` if the load index names no master / output plugin.
fn decode_form_id(
    raw: u32,
    masters: &[String],
    own_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    let load_index = (raw >> 24) as usize;
    let name = if load_index == masters.len() {
        own_name
    } else {
        masters.get(load_index)?.as_str()
    };
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(name),
    })
}

fn target_master_handle_for_fk(
    fk: &FormKey,
    masters: &[String],
    target_master_handle_ids: &[u64],
    interner: &StringInterner,
) -> Option<u64> {
    let name = interner.resolve(fk.plugin)?;
    let load_index = masters.iter().position(|m| m.eq_ignore_ascii_case(name))?;
    target_master_handle_ids.get(load_index).copied()
}

fn target_record_signature(
    session: &mut PluginSession,
    fk: &FormKey,
    output_fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    masters: &[String],
    target_master_handle_ids: &[u64],
    own_sym: Sym,
    interner: &StringInterner,
) -> Option<SigCode> {
    if fk.plugin == own_sym {
        return output_fk_to_sig.get(&(fk.local, fk.plugin)).copied();
    }
    let handle_id = target_master_handle_for_fk(fk, masters, target_master_handle_ids, interner)?;
    let plugin_name = interner.resolve(fk.plugin)?;
    let fk_str = format!("{plugin_name}:{:06X}", fk.local & 0x00FF_FFFF);
    let sig = session
        .record_signature_in_handle(handle_id, &fk_str)
        .ok()
        .flatten()?;
    SigCode::from_str(&sig).ok()
}

fn target_record_decoded(
    session: &mut PluginSession,
    fk: &FormKey,
    target_schema: &AuthoringSchema,
    masters: &[String],
    target_master_handle_ids: &[u64],
    own_sym: Sym,
    interner: &StringInterner,
) -> Option<Record> {
    if fk.plugin == own_sym {
        return session.record_decoded(fk, target_schema, interner).ok();
    }
    let handle_id = target_master_handle_for_fk(fk, masters, target_master_handle_ids, interner)?;
    session
        .record_decoded_in_handle(handle_id, fk, target_schema, interner)
        .ok()
}

/// Extract the leveled-entry reference FormKeys (LVLO / LVLE subrecords) of a
/// decoded leveled-list record (LVLI or LVLN — both use LVLO/LVLE entries).
/// Handles both decoded-struct entries (named `Reference` / `item` / `npc` field)
/// and raw `LVLO` byte entries (FO4 FormID at offset 4).
fn leveled_entry_form_keys(
    record: &Record,
    masters: &[String],
    own_name: &str,
    interner: &StringInterner,
) -> Vec<FormKey> {
    let mut out = Vec::new();
    for entry in &record.fields {
        if !matches!(entry.sig.as_str(), "LVLO" | "LVLE") {
            continue;
        }
        let entry_fk = match &entry.value {
            FieldValue::Bytes(data) if data.len() >= LVLO_REFERENCE_OFFSET + 4 => {
                let raw = u32::from_le_bytes([
                    data[LVLO_REFERENCE_OFFSET],
                    data[LVLO_REFERENCE_OFFSET + 1],
                    data[LVLO_REFERENCE_OFFSET + 2],
                    data[LVLO_REFERENCE_OFFSET + 3],
                ]);
                decode_form_id(raw, masters, own_name, interner)
            }
            FieldValue::Struct(fields) => fields.iter().find_map(|(field_sym, field_val)| {
                let is_ref = matches!(
                    interner.resolve(*field_sym),
                    Some("Reference")
                        | Some("reference")
                        | Some("Item")
                        | Some("item")
                        | Some("NPC")
                        | Some("npc")
                );
                match field_val {
                    FieldValue::FormKey(fk) if is_ref => Some(*fk),
                    _ => None,
                }
            }),
            _ => None,
        };
        if let Some(fk) = entry_fk {
            if fk.local != 0 {
                out.push(fk);
            }
        }
    }
    out
}

/// Precompute, for every output record of `leveled_sig`, its flattened valid leaf
/// bases (`None`-valued entries omitted). Empty vectors are intentional: if a
/// placed ref names that leveled list, there is no legal FO4 base and the ref
/// must be removed.
fn build_leveled_leaf_map(
    session: &mut PluginSession,
    target_schema: &crate::schema::AuthoringSchema,
    source_info: Option<SourcePluginInfo<'_>>,
    interner: &StringInterner,
    mapper: &FormKeyMapper<'_>,
    masters: &[String],
    own_name: &str,
    own_sym: Sym,
    target_master_handle_ids: &[u64],
    target_to_source: &FxHashMap<FormKey, FormKey>,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    base_fks: &[FormKey],
    leveled_sig: &str,
    leaf_is_valid: &dyn Fn(&str) -> bool,
) -> Result<FxHashMap<FormKey, Vec<FormKey>>, FixupError> {
    if base_fks.is_empty() {
        return Ok(FxHashMap::default());
    }
    let leveled_sig_code =
        SigCode::from_str(leveled_sig).map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut sig_map = fk_to_sig.clone();
    let mut entries: FxHashMap<FormKey, Vec<FormKey>> = FxHashMap::default();
    let mut queue = base_fks.to_vec();
    let mut visited = FxHashSet::default();

    while let Some(fk) = queue.pop() {
        if !visited.insert(fk) {
            continue;
        }
        let Some(sig) = target_record_signature(
            session,
            &fk,
            fk_to_sig,
            masters,
            target_master_handle_ids,
            own_sym,
            interner,
        ) else {
            continue;
        };
        sig_map.insert((fk.local, fk.plugin), sig);
        if sig != leveled_sig_code {
            continue;
        }
        let direct = target_record_decoded(
            session,
            &fk,
            target_schema,
            masters,
            target_master_handle_ids,
            own_sym,
            interner,
        )
        .map(|record| leveled_entry_form_keys(&record, masters, own_name, interner))
        .unwrap_or_default();
        for entry in &direct {
            if !visited.contains(entry) {
                queue.push(*entry);
            }
        }
        entries.insert(fk, direct);
    }

    let entries_of = |fk: &FormKey| entries.get(fk).cloned().unwrap_or_default();
    let classify = |fk: &FormKey| classify_base(fk, &sig_map, own_sym, leveled_sig, leaf_is_valid);
    let mut out = FxHashMap::default();
    for lfk in base_fks {
        let mut leaves = collect_leveled_leaf_bases(lfk, &entries_of, &classify);
        if leaves.is_empty() {
            if let Some(info) = source_info {
                leaves = collect_source_fallback_leaf_bases(
                    session,
                    info,
                    interner,
                    mapper,
                    target_to_source,
                    *lfk,
                    own_sym,
                    &sig_map,
                    leveled_sig,
                    leaf_is_valid,
                );
            }
        }
        out.insert(*lfk, leaves);
    }
    Ok(out)
}

fn collect_source_fallback_leaf_bases(
    session: &mut PluginSession<'_>,
    source_info: SourcePluginInfo<'_>,
    interner: &StringInterner,
    mapper: &FormKeyMapper<'_>,
    target_to_source: &FxHashMap<FormKey, FormKey>,
    target_base: FormKey,
    target_own_sym: Sym,
    fk_to_sig: &FxHashMap<(u32, Sym), SigCode>,
    leveled_sig: &str,
    leaf_is_valid: &dyn Fn(&str) -> bool,
) -> Vec<FormKey> {
    let Some(source_base) = source_key_for_target_leveled(
        target_base,
        target_to_source,
        target_own_sym,
        source_info.own_sym,
    ) else {
        return Vec::new();
    };

    let mut source_entries_of = |source_fk: &FormKey| -> Vec<(FormKey, Option<SigCode>)> {
        let Ok(record) = session.source_record_decoded(source_fk, source_info.schema, interner)
        else {
            return Vec::new();
        };
        if record.sig.as_str() != leveled_sig {
            return Vec::new();
        }
        leveled_entry_form_keys(&record, source_info.masters, source_info.own_name, interner)
            .into_iter()
            .map(|entry_fk| {
                let sig = session
                    .source_record_decoded(&entry_fk, source_info.schema, interner)
                    .ok()
                    .map(|record| record.sig);
                (entry_fk, sig)
            })
            .collect()
    };
    let resolve_leaf = |source_leaf: FormKey| {
        resolve_source_leaf_to_target(
            source_leaf,
            mapper,
            source_info.own_sym,
            target_own_sym,
            fk_to_sig,
            target_own_sym,
            leveled_sig,
            leaf_is_valid,
        )
    };
    collect_source_leveled_leaf_bases(
        &source_base,
        &mut source_entries_of,
        &resolve_leaf,
        leveled_sig,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlacedBaseAction {
    Keep,
    Drop,
    Replace(FormKey),
}

#[derive(Clone, Copy, Debug)]
struct PlacedLeveledRef {
    fk: FormKey,
    ref_raw: u32,
    base_raw: u32,
    base_fk: FormKey,
}

fn placed_base_action(
    base_fk: &FormKey,
    leaves_map: &FxHashMap<FormKey, Vec<FormKey>>,
    seed: u64,
    preferred: Option<FormKey>,
) -> PlacedBaseAction {
    let Some(leaves) = leaves_map.get(base_fk) else {
        return PlacedBaseAction::Keep;
    };
    if leaves.is_empty() {
        return PlacedBaseAction::Drop;
    }
    if let Some(pref) = preferred {
        if leaves.contains(&pref) {
            return PlacedBaseAction::Replace(pref);
        }
    }
    PlacedBaseAction::Replace(leaves[stable_leveled_entry_index(seed, leaves.len())])
}

/// FO76 "Leveled Placed Item" convention: an `LPI_<name>` list used as a placed
/// base contains a `UseLPI_<name>` entry that is the default (non-nuked,
/// non-harvested) placement; nuke and harvested variants share the same list.
/// FO4 can't defer LVLI-base resolution to runtime conditions, so when flattening
/// such a list we must pick that default leaf rather than a stable-random one —
/// otherwise a placement can land on a nuke-only variant (e.g. a flux-producing
/// flora) that in FO76 only appears inside an active blast zone.
fn prefer_default_leaf_index(base_eid: &str, leaf_eids: &[Option<String>]) -> Option<usize> {
    let want = format!("use{}", base_eid.to_ascii_lowercase());
    leaf_eids.iter().position(|eid| {
        eid.as_deref()
            .is_some_and(|e| e.to_ascii_lowercase() == want)
    })
}

fn read_editor_id(session: &mut PluginSession, fk: &FormKey) -> Option<String> {
    let bytes = session.first_subrecord_bytes(fk, "EDID").ok().flatten()?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).ok().map(str::to_owned)
}

/// For each placed leveled base with a `UseLPI_<name>` default leaf, record that
/// leaf as the preferred resolution (see [`prefer_default_leaf_index`]).
fn build_default_leaf_preferences(
    session: &mut PluginSession,
    bases: &[FormKey],
    leaves_map: &FxHashMap<FormKey, Vec<FormKey>>,
) -> FxHashMap<FormKey, FormKey> {
    let mut out = FxHashMap::default();
    for base in bases {
        let Some(leaves) = leaves_map.get(base) else {
            continue;
        };
        if leaves.len() < 2 {
            continue;
        }
        let Some(base_eid) = read_editor_id(session, base) else {
            continue;
        };
        let leaf_eids: Vec<Option<String>> =
            leaves.iter().map(|l| read_editor_id(session, l)).collect();
        if let Some(idx) = prefer_default_leaf_index(&base_eid, &leaf_eids) {
            out.insert(*base, leaves[idx]);
        }
    }
    out
}

fn resolved_base_is_invalid_for_placed_sig(placed_sig: &str, base_sig: &str) -> bool {
    placed_sig == "ACHR" && !matches!(base_sig, "NPC_" | "LVLN")
}

/// Resolve placed leveled-list bases in the finished output plugin. Mirrors
/// `validate_reference_target_types::strip_refr_placed_child_subrecords`: called
/// from the post-copy hook so it sees every placed child.
///
/// Object placements (REFR/PHZD/PGRE/PGRD) whose base is an LVLI are flattened to a
/// concrete non-LVLI object; actor placements (ACHR) whose base is an LVLN are
/// flattened to a concrete NPC_ (FO4 ACHR base allows only NPC_). The LVLN path is
/// defensive — current FO76 data places leveled actors as concrete NPC_ already, so
/// it is normally a no-op, but it guards against an LVLN base reaching the output.
pub fn resolve_placed_leveled_bases(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();

    let target_schema = config
        .target_schema
        .as_deref()
        .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

    // Cheap gate: need a placed-ref type. Leveled bases can live in the output
    // or in target masters, so an output-only LVLI/LVLN presence check is not
    // sufficient.
    let present_sigs = session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let has_placed = PLACED_SIGS
        .iter()
        .any(|p| present_sigs.iter().any(|s| s.as_str() == *p));
    if !has_placed {
        return Ok(report);
    }

    let interner = mapper.interner;
    let own_name = session.target_slot().parsed.plugin_name.clone();
    let own_sym = interner.intern(&own_name);
    let masters = session.target_masters().to_vec();
    let source_info_owned = session.source_slot_opt().map(|slot| {
        let own_name = slot.parsed.plugin_name.clone();
        let own_sym = interner.intern(&own_name);
        (slot.parsed.header.masters.clone(), own_name, own_sym)
    });
    let source_info = match (config.source_schema.as_deref(), source_info_owned.as_ref()) {
        (Some(schema), Some((source_masters, source_own_name, source_own_sym))) => {
            Some(SourcePluginInfo {
                schema,
                masters: source_masters.as_slice(),
                own_name: source_own_name.as_str(),
                own_sym: *source_own_sym,
            })
        }
        _ => None,
    };

    let fk_to_sig = build_target_fk_sig_map(session, interner)?;
    if fk_to_sig.is_empty() {
        return Ok(report);
    }
    let target_to_source: FxHashMap<FormKey, FormKey> = mapper
        .source_to_target_iter()
        .map(|(source, target)| (target, source))
        .collect();

    let mut object_refs = Vec::<PlacedLeveledRef>::new();
    let mut actor_refs = Vec::<PlacedLeveledRef>::new();
    let mut object_base_set = FxHashSet::default();
    let mut actor_base_set = FxHashSet::default();
    let mut object_bases = Vec::<FormKey>::new();
    let mut actor_bases = Vec::<FormKey>::new();
    let mut drop_refs = Vec::<FormKey>::new();

    for &placed in PLACED_SIGS {
        if !present_sigs.iter().any(|s| s.as_str() == placed) {
            continue;
        }
        let Ok(sig) = SigCode::from_str(placed) else {
            continue;
        };
        let fks = session
            .form_keys_of_sig(sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in fks {
            let Some(name_bytes) = session
                .first_subrecord_bytes(&fk, "NAME")
                .map_err(|e| FixupError::HandleError(e.to_string()))?
            else {
                continue;
            };
            if name_bytes.len() < 4 {
                continue;
            }
            let base_raw =
                u32::from_le_bytes([name_bytes[0], name_bytes[1], name_bytes[2], name_bytes[3]]);
            if base_raw & 0x00FF_FFFF == 0 {
                drop_refs.push(fk);
                continue;
            }
            let Some(base_fk) = decode_form_id(base_raw, &masters, &own_name, interner) else {
                drop_refs.push(fk);
                continue;
            };
            let Some(base_sig) = target_record_signature(
                session,
                &base_fk,
                &fk_to_sig,
                &masters,
                &config.target_master_handle_ids,
                own_sym,
                interner,
            ) else {
                drop_refs.push(fk);
                continue;
            };
            let target_leveled_sig = if placed == "ACHR" { "LVLN" } else { "LVLI" };
            if resolved_base_is_invalid_for_placed_sig(placed, base_sig.as_str()) {
                drop_refs.push(fk);
                continue;
            }
            if base_sig.as_str() != target_leveled_sig {
                continue;
            }
            let Some(ref_raw) = encode_form_id(&fk, &masters, &own_name, interner) else {
                continue;
            };
            let placed_ref = PlacedLeveledRef {
                fk,
                ref_raw,
                base_raw,
                base_fk,
            };
            if placed == "ACHR" {
                actor_refs.push(placed_ref);
                if actor_base_set.insert(base_fk) {
                    actor_bases.push(base_fk);
                }
            } else {
                object_refs.push(placed_ref);
                if object_base_set.insert(base_fk) {
                    object_bases.push(base_fk);
                }
            }
        }
    }

    if object_refs.is_empty() && actor_refs.is_empty() {
        if !drop_refs.is_empty() {
            let removed = session
                .remove_records(&drop_refs)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            report.records_dropped = report.records_dropped.saturating_add(removed as u32);
        }
        return Ok(report);
    }

    // Object-placement bases: LVLI → any concrete non-LVLI object.
    let lvli_leaves = build_leveled_leaf_map(
        session,
        target_schema,
        source_info,
        interner,
        mapper,
        &masters,
        &own_name,
        own_sym,
        &config.target_master_handle_ids,
        &target_to_source,
        &fk_to_sig,
        &object_bases,
        "LVLI",
        &|sig| sig != "LVLI",
    )?;
    // Actor-placement bases (ACHR): LVLN → concrete NPC_ only.
    let lvln_leaves = build_leveled_leaf_map(
        session,
        target_schema,
        source_info,
        interner,
        mapper,
        &masters,
        &own_name,
        own_sym,
        &config.target_master_handle_ids,
        &target_to_source,
        &fk_to_sig,
        &actor_bases,
        "LVLN",
        &|sig| sig == "NPC_",
    )?;

    let lvli_preferred = build_default_leaf_preferences(session, &object_bases, &lvli_leaves);
    let lvln_preferred = build_default_leaf_preferences(session, &actor_bases, &lvln_leaves);

    for (placed_refs, leaves_map, preferred) in [
        (object_refs.as_slice(), &lvli_leaves, &lvli_preferred),
        (actor_refs.as_slice(), &lvln_leaves, &lvln_preferred),
    ] {
        for placed_ref in placed_refs {
            let seed = ((placed_ref.ref_raw as u64) << 32) ^ placed_ref.base_raw as u64;
            let pick = match placed_base_action(
                &placed_ref.base_fk,
                leaves_map,
                seed,
                preferred.get(&placed_ref.base_fk).copied(),
            ) {
                PlacedBaseAction::Keep => continue,
                PlacedBaseAction::Drop => {
                    drop_refs.push(placed_ref.fk);
                    continue;
                }
                PlacedBaseAction::Replace(pick) => pick,
            };
            let Some(rep) = encode_form_id(&pick, &masters, &own_name, interner) else {
                continue;
            };
            if rep == placed_ref.base_raw {
                continue;
            }
            let changed = session
                .patch_subrecord_bytes(&placed_ref.fk, "NAME", |buf| {
                    if buf.len() < 4 {
                        return false;
                    }
                    let current = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                    if current != placed_ref.base_raw {
                        return false;
                    }
                    buf[0..4].copy_from_slice(&rep.to_le_bytes());
                    true
                })
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if changed {
                report.records_changed = report.records_changed.saturating_add(1);
            }
        }
    }
    if !drop_refs.is_empty() {
        let removed = session
            .remove_records(&drop_refs)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_dropped = report.records_dropped.saturating_add(removed as u32);
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};

    fn fk(hex: &str, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey::parse(&format!("{hex}@{plugin}"), interner).unwrap()
    }

    const OWN: &str = "SeventySix.esm";
    fn masters() -> Vec<String> {
        vec!["Fallout4.esm".to_string(), "DLCRobot.esm".to_string()]
    }

    // -- stable_leveled_entry_index ----------------------------------------

    #[test]
    fn stable_index_is_deterministic_and_in_range() {
        for len in [1usize, 2, 3, 7, 64] {
            let a = stable_leveled_entry_index(0xDEAD_BEEF, len);
            let b = stable_leveled_entry_index(0xDEAD_BEEF, len);
            assert_eq!(a, b, "same seed → same index");
            assert!(a < len, "index {a} out of range for len {len}");
        }
    }

    #[test]
    fn stable_index_differs_for_different_seeds() {
        // Two distinct seeds should usually pick different slots in a wide list.
        let a = stable_leveled_entry_index(1, 64);
        let b = stable_leveled_entry_index(2, 64);
        assert_ne!(a, b);
    }

    // -- classify_base -----------------------------------------------------

    // Object-placement leaf rule (REFR): any non-LVLI is a valid base.
    fn non_lvli(sig: &str) -> bool {
        sig != "LVLI"
    }
    // Actor-placement leaf rule (ACHR): only NPC_ is a valid base.
    fn npc_only(sig: &str) -> bool {
        sig == "NPC_"
    }

    #[test]
    fn classify_distinguishes_lvli_validbase_and_skip() {
        let interner = StringInterner::new();
        let own_sym = interner.intern(OWN);
        let lvli = fk("000A01", OWN, &interner);
        let stat = fk("000A02", OWN, &interner);
        let absent = fk("000A03", OWN, &interner);
        let master_item = fk("00ABCD", "Fallout4.esm", &interner);
        let null = fk("000000", OWN, &interner);

        let mut map: FxHashMap<(u32, Sym), SigCode> = FxHashMap::default();
        map.insert(
            (lvli.local, lvli.plugin),
            SigCode::from_str("LVLI").unwrap(),
        );
        map.insert(
            (stat.local, stat.plugin),
            SigCode::from_str("STAT").unwrap(),
        );

        let c = |f: &FormKey| classify_base(f, &map, own_sym, "LVLI", &non_lvli);
        assert_eq!(c(&lvli), BaseKind::Leveled);
        assert_eq!(c(&stat), BaseKind::ValidBase);
        // own-plugin object-id absent from output → never emitted → Skip.
        assert_eq!(c(&absent), BaseKind::Skip);
        // a master object-id we don't index is assumed a valid concrete base.
        assert_eq!(c(&master_item), BaseKind::ValidBase);
        assert_eq!(c(&null), BaseKind::Skip);
    }

    #[test]
    fn classify_known_master_lvli_recurses_not_validbase() {
        let interner = StringInterner::new();
        let own_sym = interner.intern(OWN);
        let master_lvli = fk("1957A7", "Fallout4.esm", &interner);

        let mut map: FxHashMap<(u32, Sym), SigCode> = FxHashMap::default();
        map.insert(
            (master_lvli.local, master_lvli.plugin),
            SigCode::from_str("LVLI").unwrap(),
        );

        let c = |f: &FormKey| classify_base(f, &map, own_sym, "LVLI", &non_lvli);
        assert_eq!(c(&master_lvli), BaseKind::Leveled);
    }

    #[test]
    fn classify_lvln_rule_accepts_npc_recurses_lvln_skips_other() {
        let interner = StringInterner::new();
        let own_sym = interner.intern(OWN);
        let lvln = fk("000B01", OWN, &interner);
        let npc = fk("000B02", OWN, &interner);
        let stat = fk("000B03", OWN, &interner);

        let mut map: FxHashMap<(u32, Sym), SigCode> = FxHashMap::default();
        map.insert(
            (lvln.local, lvln.plugin),
            SigCode::from_str("LVLN").unwrap(),
        );
        map.insert((npc.local, npc.plugin), SigCode::from_str("NPC_").unwrap());
        map.insert(
            (stat.local, stat.plugin),
            SigCode::from_str("STAT").unwrap(),
        );

        let c = |f: &FormKey| classify_base(f, &map, own_sym, "LVLN", &npc_only);
        assert_eq!(c(&lvln), BaseKind::Leveled, "LVLN recurses");
        assert_eq!(c(&npc), BaseKind::ValidBase, "NPC_ is a valid ACHR leaf");
        // A STAT is not a valid ACHR base (FO4 ACHR base = NPC_ only) → Skip.
        assert_eq!(
            c(&stat),
            BaseKind::Skip,
            "non-NPC_ leaf rejected under ACHR rule"
        );
    }

    #[test]
    fn collect_flattens_lvln_to_npc_leaves() {
        let interner = StringInterner::new();
        let own_sym = interner.intern(OWN);
        let lvln = fk("000B01", OWN, &interner);
        let nested = fk("000B02", OWN, &interner);
        let npc_a = fk("000B10", OWN, &interner);
        let npc_b = fk("000B11", OWN, &interner);
        let stat = fk("000B20", OWN, &interner); // wrong type → excluded

        let mut map: FxHashMap<(u32, Sym), SigCode> = FxHashMap::default();
        map.insert(
            (lvln.local, lvln.plugin),
            SigCode::from_str("LVLN").unwrap(),
        );
        map.insert(
            (nested.local, nested.plugin),
            SigCode::from_str("LVLN").unwrap(),
        );
        map.insert(
            (npc_a.local, npc_a.plugin),
            SigCode::from_str("NPC_").unwrap(),
        );
        map.insert(
            (npc_b.local, npc_b.plugin),
            SigCode::from_str("NPC_").unwrap(),
        );
        map.insert(
            (stat.local, stat.plugin),
            SigCode::from_str("STAT").unwrap(),
        );

        let entries = move |f: &FormKey| -> Vec<FormKey> {
            if *f == lvln {
                vec![npc_a, nested, stat]
            } else if *f == nested {
                vec![npc_b]
            } else {
                vec![]
            }
        };
        let classify = |f: &FormKey| classify_base(f, &map, own_sym, "LVLN", &npc_only);

        let leaves = collect_leveled_leaf_bases(&lvln, &entries, &classify);
        assert_eq!(
            leaves,
            vec![npc_a, npc_b],
            "nested LVLN flattened to NPC_; STAT dropped"
        );
    }

    // -- collect_leveled_leaf_bases ----------------------------------------

    #[test]
    fn collect_drops_lvli_and_recurses_into_nested() {
        let interner = StringInterner::new();
        let base = fk("000100", OWN, &interner);
        let nested = fk("000101", OWN, &interner);
        let item_a = fk("000200", OWN, &interner);
        let item_b = fk("000201", OWN, &interner);

        let entries = move |f: &FormKey| -> Vec<FormKey> {
            if *f == base {
                vec![item_a, nested]
            } else if *f == nested {
                vec![item_b]
            } else {
                vec![]
            }
        };
        let classify = move |f: &FormKey| -> BaseKind {
            if *f == nested {
                BaseKind::Leveled
            } else {
                BaseKind::ValidBase
            }
        };

        let leaves = collect_leveled_leaf_bases(&base, &entries, &classify);
        assert_eq!(
            leaves,
            vec![item_a, item_b],
            "LVLI not a leaf; nested flattened"
        );
    }

    #[test]
    fn collect_is_cycle_safe() {
        let interner = StringInterner::new();
        let a = fk("000100", OWN, &interner);
        let b = fk("000101", OWN, &interner);
        let item = fk("000200", OWN, &interner);

        let entries = move |f: &FormKey| -> Vec<FormKey> {
            if *f == a {
                vec![b]
            } else if *f == b {
                vec![a, item] // cycle back to a
            } else {
                vec![]
            }
        };
        let classify = move |f: &FormKey| -> BaseKind {
            if *f == a || *f == b {
                BaseKind::Leveled
            } else {
                BaseKind::ValidBase
            }
        };

        let leaves = collect_leveled_leaf_bases(&a, &entries, &classify);
        assert_eq!(
            leaves,
            vec![item],
            "cycle does not loop; single leaf collected"
        );
    }

    #[test]
    fn collect_dedups_repeated_leaf() {
        let interner = StringInterner::new();
        let base = fk("000100", OWN, &interner);
        let nested = fk("000101", OWN, &interner);
        let item = fk("000200", OWN, &interner);

        let entries = move |f: &FormKey| -> Vec<FormKey> {
            if *f == base {
                vec![item, nested]
            } else if *f == nested {
                vec![item] // same item again
            } else {
                vec![]
            }
        };
        let classify = move |f: &FormKey| {
            if *f == nested {
                BaseKind::Leveled
            } else {
                BaseKind::ValidBase
            }
        };

        let leaves = collect_leveled_leaf_bases(&base, &entries, &classify);
        assert_eq!(leaves, vec![item], "duplicate leaf collapsed");
    }

    #[test]
    fn collect_skips_unresolvable_entries() {
        let interner = StringInterner::new();
        let base = fk("000100", OWN, &interner);
        let good = fk("000200", OWN, &interner);
        let dropped = fk("000201", OWN, &interner);

        let entries = move |f: &FormKey| {
            if *f == base {
                vec![good, dropped]
            } else {
                vec![]
            }
        };
        let classify = move |f: &FormKey| {
            if *f == dropped {
                BaseKind::Skip
            } else {
                BaseKind::ValidBase
            }
        };

        let leaves = collect_leveled_leaf_bases(&base, &entries, &classify);
        assert_eq!(leaves, vec![good], "Skip entries excluded");
    }

    #[test]
    fn source_key_falls_back_to_same_local_for_output_owned_lvli() {
        let interner = StringInterner::new();
        let source_own = interner.intern("Source.esm");
        let target_own = interner.intern("Target.esp");
        let target_lvli = fk("001000", "Target.esp", &interner);
        let target_to_source = FxHashMap::default();

        assert_eq!(
            source_key_for_target_leveled(target_lvli, &target_to_source, target_own, source_own),
            Some(FormKey {
                local: 0x001000,
                plugin: source_own,
            })
        );
    }

    #[test]
    fn source_collect_flattens_nested_lvli_to_target_leaves() {
        let interner = StringInterner::new();
        let source_base = fk("001000", "Source.esm", &interner);
        let source_nested = fk("002000", "Source.esm", &interner);
        let source_item_a = fk("003000", "Source.esm", &interner);
        let source_item_b = fk("004000", "Source.esm", &interner);
        let target_item_a = fk("003000", "Target.esp", &interner);
        let target_item_b = fk("004000", "Target.esp", &interner);
        let lvli = SigCode::from_str("LVLI").unwrap();
        let misc = SigCode::from_str("MISC").unwrap();

        let mut source_entries: FxHashMap<FormKey, Vec<(FormKey, Option<SigCode>)>> =
            FxHashMap::default();
        source_entries.insert(
            source_base,
            vec![(source_nested, Some(lvli)), (source_item_a, Some(misc))],
        );
        source_entries.insert(
            source_nested,
            vec![(source_item_b, Some(misc)), (source_item_a, Some(misc))],
        );

        let mut target_map = FxHashMap::default();
        target_map.insert(source_item_a, target_item_a);
        target_map.insert(source_item_b, target_item_b);
        let mut source_entries_of =
            |fk: &FormKey| source_entries.get(fk).cloned().unwrap_or_default();
        let resolve_leaf = |fk: FormKey| target_map.get(&fk).copied();

        let leaves = collect_source_leveled_leaf_bases(
            &source_base,
            &mut source_entries_of,
            &resolve_leaf,
            "LVLI",
        );

        assert_eq!(leaves, vec![target_item_b, target_item_a]);
    }

    #[test]
    fn placed_base_action_drops_empty_leveled_list() {
        let interner = StringInterner::new();
        let base = fk("067396", OWN, &interner);
        let mut leaves_map = FxHashMap::default();
        leaves_map.insert(base, Vec::new());

        assert_eq!(
            placed_base_action(&base, &leaves_map, 0x0782_3E98_0706_7396, None),
            PlacedBaseAction::Drop
        );
    }

    #[test]
    fn placed_base_action_replaces_nested_leveled_list_leaf() {
        let interner = StringInterner::new();
        let base = fk("2151AB", OWN, &interner);
        let leaf = fk("0366BF", OWN, &interner);
        let mut leaves_map = FxHashMap::default();
        leaves_map.insert(base, vec![leaf]);

        assert_eq!(
            placed_base_action(&base, &leaves_map, 0x0782_3E9B_0721_51AB, None),
            PlacedBaseAction::Replace(leaf)
        );
    }

    #[test]
    fn placed_base_action_prefers_supplied_leaf_over_stable_pick() {
        let interner = StringInterner::new();
        let base = fk("2151AB", OWN, &interner);
        let a = fk("0366BF", OWN, &interner);
        let b = fk("155D76", OWN, &interner);
        let mut leaves_map = FxHashMap::default();
        leaves_map.insert(base, vec![a, b]);

        // Without a preference, the stable pick is used; with one, it wins.
        assert_eq!(
            placed_base_action(&base, &leaves_map, 0x0782_3E9B_0721_51AB, Some(a)),
            PlacedBaseAction::Replace(a)
        );
        // A preferred leaf that is not in the list is ignored (falls back to stable).
        let outside = fk("0AAAAA", OWN, &interner);
        assert!(matches!(
            placed_base_action(&base, &leaves_map, 0x0782_3E9B_0721_51AB, Some(outside)),
            PlacedBaseAction::Replace(pick) if pick == a || pick == b
        ));
    }

    #[test]
    fn prefer_default_leaf_index_matches_use_prefixed_entry() {
        // LPI_FloraSootFlower01 → UseLPI_FloraSootFlower01 (non-nuked default).
        let leaves = vec![
            Some("FloraRadGeigerBlossom01".to_string()),
            Some("UseLPI_FloraSootFlower01_Harvested".to_string()),
            Some("UseLPI_FloraSootFlower01".to_string()),
        ];
        assert_eq!(
            prefer_default_leaf_index("LPI_FloraSootFlower01", &leaves),
            Some(2)
        );
        // No matching default leaf → None (caller falls back to stable pick).
        let no_default = vec![
            Some("FloraRadGeigerBlossom01".to_string()),
            Some("SomethingElse".to_string()),
        ];
        assert_eq!(
            prefer_default_leaf_index("LPI_FloraSootFlower01", &no_default),
            None
        );
    }

    #[test]
    fn achr_base_accepts_only_npc_or_lvln() {
        assert!(!resolved_base_is_invalid_for_placed_sig("ACHR", "NPC_"));
        assert!(!resolved_base_is_invalid_for_placed_sig("ACHR", "LVLN"));
        assert!(resolved_base_is_invalid_for_placed_sig("ACHR", "STAT"));
        assert!(
            !resolved_base_is_invalid_for_placed_sig("REFR", "STAT"),
            "object placed refs are not broadened by the ACHR-only rule"
        );
    }

    // -- encode / decode form id -------------------------------------------

    #[test]
    fn encode_decode_roundtrip_own_and_master() {
        let interner = StringInterner::new();
        let masters = masters();
        let own = fk("0512AB", OWN, &interner);
        let fo4 = fk("0012CD", "Fallout4.esm", &interner);
        let dlc = fk("0034EF", "DLCRobot.esm", &interner);

        // own plugin = load index 2 (after the two masters).
        assert_eq!(
            encode_form_id(&own, &masters, OWN, &interner),
            Some(0x0205_12AB)
        );
        assert_eq!(
            encode_form_id(&fo4, &masters, OWN, &interner),
            Some(0x0000_12CD)
        );
        assert_eq!(
            encode_form_id(&dlc, &masters, OWN, &interner),
            Some(0x0100_34EF)
        );

        for f in [own, fo4, dlc] {
            let raw = encode_form_id(&f, &masters, OWN, &interner).unwrap();
            assert_eq!(decode_form_id(raw, &masters, OWN, &interner), Some(f));
        }
    }

    #[test]
    fn decode_rejects_unknown_load_index() {
        let interner = StringInterner::new();
        let masters = masters();
        // load index 5 names no master and isn't the output (own = index 2).
        assert_eq!(decode_form_id(0x0500_0001, &masters, OWN, &interner), None);
    }

    // -- lvli_entry_form_keys ----------------------------------------------

    fn lvli_record(entries: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("LVLI").unwrap(),
            form_key: fk("000100", OWN, interner),
            eid: None,
            flags: RecordFlags::empty(),
            fields: entries.into_iter().collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    #[test]
    fn lvli_entries_reads_struct_reference_field() {
        let interner = StringInterner::new();
        let item = fk("000200", OWN, &interner);
        let ref_sym = interner.intern("Reference");
        let entry = FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Struct(vec![(ref_sym, FieldValue::FormKey(item))]),
        };
        let rec = lvli_record(vec![entry], &interner);
        assert_eq!(
            leveled_entry_form_keys(&rec, &masters(), OWN, &interner),
            vec![item]
        );
    }

    #[test]
    fn lvli_entries_reads_raw_lvlo_bytes() {
        let interner = StringInterner::new();
        let masters = masters();
        let item = fk("000200", OWN, &interner);
        let raw = encode_form_id(&item, &masters, OWN, &interner).unwrap();
        let mut data = smallvec::smallvec![0u8; 12];
        data[LVLO_REFERENCE_OFFSET..LVLO_REFERENCE_OFFSET + 4].copy_from_slice(&raw.to_le_bytes());
        let entry = FieldEntry {
            sig: SubrecordSig::from_str("LVLO").unwrap(),
            value: FieldValue::Bytes(data),
        };
        let rec = lvli_record(vec![entry], &interner);
        assert_eq!(
            leveled_entry_form_keys(&rec, &masters, OWN, &interner),
            vec![item]
        );
    }

    #[test]
    fn lvli_entries_ignores_non_entry_subrecords() {
        let interner = StringInterner::new();
        let llct = FieldEntry {
            sig: SubrecordSig::from_str("LLCT").unwrap(),
            value: FieldValue::Uint(1),
        };
        let rec = lvli_record(vec![llct], &interner);
        assert!(leveled_entry_form_keys(&rec, &masters(), OWN, &interner).is_empty());
    }
}
