//! Second pass of `rewrite_mswp_material_paths`.
//!
//! The first pass namespaces `BNAM` on the material swaps the output owns. A
//! record whose model path relocated into the namespace may still point at a
//! swap remapped onto a target master, and a base-game swap is keyed on the
//! *pre-relocation* material path. The relocated mesh references
//! `<ns>\...bgsm`, the base swap looks for `...bgsm`, nothing matches, and the
//! unswapped base material renders (e.g. the FO4 flag on `FlagpoleUSAFlag02`).
//!
//! Masters are opened index-only, so the base swap's contents are unreadable.
//! The clone is built from the source swap the remap came from and inserted
//! into the output with its originals namespaced. Records keyed off that base
//! swap are repointed at the clone.
//!
//! Phase-contract: no Python / GIL. Records are walked and mutated directly on
//! the plugin-handle store, like `regenerate_modt`.

use bytes::Bytes;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use smol_str::SmolStr;
use std::collections::HashSet;

use esp_authoring_core::plugin_runtime::{ParsedItem, ParsedRecord, plugin_handle_store_ref};

use crate::fixups::harvest_modt::fo4_model_slots;
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::phase::{PhaseCtx, PhaseError};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

/// EditorID prefix for a swap cloned so its originals can carry the namespace.
/// Keyed by the source swap's object-id, so the id is stable across runs.
const CLONED_SWAP_EDITOR_ID_PREFIX: &str = "B21_FO76_NSMSWP_";

fn material_swap_sig(path_sig: &str) -> Option<&'static str> {
    match path_sig {
        "MODL" => Some("MODS"),
        "MOD2" => Some("MO2S"),
        "MOD3" => Some("MO3S"),
        "MOD4" => Some("MO4S"),
        "MOD5" => Some("MO5S"),
        _ => None,
    }
}

fn zstring(data: &[u8]) -> Option<String> {
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    (end > 0).then(|| String::from_utf8_lossy(&data[..end]).into_owned())
}

fn form_id(data: &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = data.try_into().ok()?;
    let value = u32::from_le_bytes(bytes);
    (value != 0).then_some(value)
}

/// True when `path` already sits under `<namespace>\` — i.e. the model was
/// relocated, so every material it reaches is namespaced too.
fn is_namespaced_model_path(path: &str, namespace: &str) -> bool {
    let normalized = path.trim().trim_matches('\0').replace('\\', "/");
    let mut parts = normalized.split('/').filter(|part| !part.is_empty());
    let Some(first) = parts.next() else {
        return false;
    };
    // Model paths are `Meshes/`-relative, but tolerate an explicit root.
    if first.eq_ignore_ascii_case("meshes") {
        return parts
            .next()
            .is_some_and(|p| p.eq_ignore_ascii_case(namespace));
    }
    first.eq_ignore_ascii_case(namespace)
}

fn walk_records<'a>(items: &'a [ParsedItem], f: &mut impl FnMut(&'a ParsedRecord)) {
    for item in items {
        match item {
            ParsedItem::Record(r) => f(r),
            ParsedItem::Group(g) => walk_records(&g.children, f),
        }
    }
}

fn walk_records_mut(items: &mut [ParsedItem], f: &mut impl FnMut(&mut ParsedRecord)) {
    for item in items {
        match item {
            ParsedItem::Record(r) => f(r),
            ParsedItem::Group(g) => walk_records_mut(&mut g.children, f),
        }
    }
}

/// One output record that reaches a base-owned swap through a relocated model.
/// The base swap's FormID is only the gate — its contents are unreadable from an
/// index-only master, so the clone is built from the source swap instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Candidate {
    record_form_id: u32,
    swap_sig: &'static str,
}

/// Collect every record whose model path is namespaced but whose paired swap
/// resolves outside the output plugin.
fn collect_candidates(items: &[ParsedItem], namespace: &str, own_index: u32) -> Vec<Candidate> {
    let slots = fo4_model_slots();
    let mut out = Vec::new();
    walk_records(items, &mut |rec| {
        let Some(model_slots) = slots.get(rec.signature.as_str()) else {
            return;
        };
        for model_slot in model_slots {
            let Some(swap_sig) = material_swap_sig(&model_slot.path_sig) else {
                continue;
            };
            let namespaced = rec.subrecords.iter().any(|sub| {
                sub.signature.as_str() == model_slot.path_sig
                    && zstring(&sub.data)
                        .is_some_and(|path| is_namespaced_model_path(&path, namespace))
            });
            if !namespaced {
                continue;
            }
            let Some(base_swap_form_id) = rec
                .subrecords
                .iter()
                .find(|sub| sub.signature.as_str() == swap_sig)
                .and_then(|sub| form_id(&sub.data))
            else {
                continue;
            };
            if (base_swap_form_id >> 24) == own_index {
                continue;
            }
            out.push(Candidate {
                record_form_id: rec.form_id,
                swap_sig,
            });
        }
    });
    out
}

/// A source `MSWP`'s subrecords in order, as `(sig, payload)`. Carried verbatim
/// rather than rebuilt from `BNAM`/`SNAM` pairs so per-swap extras (`FNAM` tree
/// folder, `CNAM` colour remap) survive; anything FO4's schema does not know is
/// dropped on encode.
type SwapSubrecords = Vec<(SmolStr, Vec<u8>)>;

fn swap_subrecords_from_record(rec: &ParsedRecord) -> Option<SwapSubrecords> {
    if rec.signature.as_str() != "MSWP" {
        return None;
    }
    let has_original = rec
        .subrecords
        .iter()
        .any(|sub| sub.signature.as_str() == "BNAM");
    has_original.then(|| {
        rec.subrecords
            .iter()
            // EDID is reassigned on the clone so the two records stay distinct.
            .filter(|sub| sub.signature.as_str() != "EDID")
            .map(|sub| (sub.signature.clone(), sub.data.to_vec()))
            .collect()
    })
}

/// Namespace every original that names a relocated material. `None` when the
/// swap touches nothing relocated — the base swap is still correct, so cloning
/// it would only add a redundant record.
fn namespaced_subrecords(
    subrecords: &SwapSubrecords,
    relocation_members: &HashSet<String>,
    namespace: &str,
) -> Option<SwapSubrecords> {
    let mut changed = false;
    let out = subrecords
        .iter()
        .map(|(sig, data)| {
            if sig.as_str() != "BNAM" {
                return (sig.clone(), data.clone());
            }
            let rewritten = zstring(data).and_then(|current| {
                super::mswp_material_paths::namespace_relocated_material(
                    &current,
                    relocation_members,
                    namespace,
                )
            });
            match rewritten {
                Some(rewritten) => {
                    changed = true;
                    let mut bytes = rewritten.into_bytes();
                    bytes.push(0);
                    (sig.clone(), bytes)
                }
                None => (sig.clone(), data.clone()),
            }
        })
        .collect();
    changed.then_some(out)
}

/// Assign an object-id to every swap that needs a clone.
///
/// The ids must come from the run's shared `FormKeyMapper`, not a counter
/// seeded from the target plugin. The mapper records every id it hands out,
/// so the navmesh, audio and face passes that synthesize later can't reuse
/// them. The loader silently drops records with duplicate FormIDs.
fn allocate_clone_ids(
    mapper: &mut FormKeyMapper<'_>,
    source_object_ids: impl IntoIterator<Item = u32>,
) -> FxHashMap<u32, u32> {
    source_object_ids
        .into_iter()
        .map(|source_object_id| (source_object_id, mapper.allocate_generated().local))
        .collect()
}

fn build_clone(
    form_key: FormKey,
    source_object_id: u32,
    subrecords: &SwapSubrecords,
    interner: &StringInterner,
) -> Result<Record, PhaseError> {
    let sig = SigCode::from_str("MSWP").map_err(|e| PhaseError::Internal(e.to_string()))?;
    let mut record = Record::new(sig, form_key);
    let editor_id = interner.intern(&format!(
        "{CLONED_SWAP_EDITOR_ID_PREFIX}{:06X}",
        source_object_id & 0x00FF_FFFF
    ));
    record.eid = Some(editor_id);
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"EDID"),
        value: FieldValue::String(editor_id),
    });
    for (sub_sig, data) in subrecords {
        let sig =
            SubrecordSig::from_str(sub_sig).map_err(|e| PhaseError::Internal(e.to_string()))?;
        record.fields.push(FieldEntry {
            sig,
            value: FieldValue::Bytes(SmallVec::from_slice(data)),
        });
    }
    Ok(record)
}

/// Repoint records at their cloned swap. Returns the number of records changed.
fn repoint(items: &mut [ParsedItem], replacements: &FxHashMap<(u32, &'static str), u32>) -> u32 {
    let mut changed = 0u32;
    walk_records_mut(items, &mut |rec| {
        let mut touched = false;
        for sub in rec.subrecords.iter_mut() {
            let Some(new_form_id) = sub
                .signature
                .as_str()
                .parse::<SwapSigKey>()
                .ok()
                .and_then(|key| replacements.get(&(rec.form_id, key.0)))
            else {
                continue;
            };
            sub.data = Bytes::from(new_form_id.to_le_bytes().to_vec());
            touched = true;
        }
        if touched {
            changed += 1;
        }
    });
    changed
}

/// Newtype so a raw subrecord signature can be matched against the static swap
/// signatures without allocating.
struct SwapSigKey(&'static str);

impl std::str::FromStr for SwapSigKey {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for sig in ["MODS", "MO2S", "MO3S", "MO4S", "MO5S"] {
            if s == sig {
                return Ok(SwapSigKey(sig));
            }
        }
        Err(())
    }
}

/// What the pass did. `unresolved` counts records that reach a base-owned swap
/// through a relocated model but whose source swap could not be read — their
/// swap stays broken, so the count is surfaced rather than silently dropped.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Report {
    pub changed: u32,
    pub added: u32,
    pub unresolved: u32,
}

/// Run the pass.
pub(super) fn run(ctx: &mut PhaseCtx<'_>, namespace: &str) -> Result<Report, PhaseError> {
    let target_handle_id = ctx.run.target_handle_id;
    let source_handle_id = ctx.run.source_handle_id;

    // Stage A — read only. Collect candidates from the output, then resolve each
    // one's swap through the source plugin (masters are index-only, so the base
    // swap's contents are not readable from them).
    let (candidates, source_swaps, target_plugin_name) = {
        let store = plugin_handle_store_ref()
            .lock()
            .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
        let target = store.get(&target_handle_id).ok_or_else(|| {
            PhaseError::BadParams(format!("unknown target handle: {target_handle_id}"))
        })?;
        let target_plugin_name = target.parsed.plugin_name.clone();
        let own_index = (target.parsed.header.masters.len() & 0xFF) as u32;
        let candidates = collect_candidates(&target.parsed.root_items, namespace, own_index);
        if candidates.is_empty() {
            return Ok(Report::default());
        }

        let Some(source) = store.get(&source_handle_id) else {
            // No source plugin (asset-only run): every candidate stays broken.
            return Ok(Report {
                unresolved: candidates.len() as u32,
                ..Report::default()
            });
        };
        // The output preserves source object-ids, so a candidate's own id also
        // addresses the source record that its swap ref was remapped from.
        let wanted: FxHashSet<u32> = candidates
            .iter()
            .map(|c| c.record_form_id & 0x00FF_FFFF)
            .collect();
        let mut source_swap_ref: FxHashMap<(u32, &'static str), u32> = FxHashMap::default();
        let mut wanted_swaps: FxHashSet<u32> = FxHashSet::default();
        walk_records(&source.parsed.root_items, &mut |rec| {
            if !wanted.contains(&(rec.form_id & 0x00FF_FFFF)) {
                return;
            }
            for sub in &rec.subrecords {
                let Ok(key) = sub.signature.as_str().parse::<SwapSigKey>() else {
                    continue;
                };
                let Some(fid) = form_id(&sub.data) else {
                    continue;
                };
                source_swap_ref.insert((rec.form_id & 0x00FF_FFFF, key.0), fid & 0x00FF_FFFF);
                wanted_swaps.insert(fid & 0x00FF_FFFF);
            }
        });
        let mut pairs_by_object_id: FxHashMap<u32, SwapSubrecords> = FxHashMap::default();
        walk_records(&source.parsed.root_items, &mut |rec| {
            let object_id = rec.form_id & 0x00FF_FFFF;
            if !wanted_swaps.contains(&object_id) {
                return;
            }
            if let Some(subrecords) = swap_subrecords_from_record(rec) {
                pairs_by_object_id.insert(object_id, subrecords);
            }
        });
        (
            candidates,
            (source_swap_ref, pairs_by_object_id),
            target_plugin_name,
        )
    };
    let (source_swap_ref, pairs_by_object_id) = source_swaps;

    ctx.check_cancel()?;

    // Stage B — build the clones (one per source swap, shared by every record
    // that reaches it), without holding the store lock.
    let own_plugin = ctx.run.interner.intern(&target_plugin_name);

    // Resolve which swaps actually need a clone before allocating, so a swap
    // that reaches nothing relocated does not consume an object-id.
    // Deterministic order: the allocated ids must not depend on walk order.
    let mut source_object_ids: Vec<u32> = pairs_by_object_id.keys().copied().collect();
    source_object_ids.sort_unstable();
    let pending: Vec<(u32, SwapSubrecords)> = source_object_ids
        .into_iter()
        .filter_map(|source_object_id| {
            namespaced_subrecords(
                &pairs_by_object_id[&source_object_id],
                &ctx.run.relocation_members,
                namespace,
            )
            .map(|namespaced| (source_object_id, namespaced))
        })
        .collect();

    let clone_by_source = {
        let state = ctx.run.mapper_state.as_mut().ok_or_else(|| {
            PhaseError::Internal(
                "relocated_base_material_swaps: mapper_state not initialized; must run after translate"
                    .into(),
            )
        })?;
        let mut mapper = FormKeyMapper::from_state(state, &ctx.run.interner);
        allocate_clone_ids(&mut mapper, pending.iter().map(|(id, _)| *id))
    };

    let mut additions: Vec<Record> = Vec::new();
    for (source_object_id, namespaced) in &pending {
        let form_key = FormKey {
            local: clone_by_source[source_object_id],
            plugin: own_plugin,
        };
        additions.push(build_clone(
            form_key,
            *source_object_id,
            namespaced,
            &ctx.run.interner,
        )?);
    }
    // A candidate is only unresolved when its source swap could not be read at
    // all. A source swap that reaches nothing relocated needs no clone — the
    // base swap it was remapped onto is still correct.
    let mut replacements: FxHashMap<(u32, &'static str), u32> = FxHashMap::default();
    let mut unresolved = 0u32;
    for candidate in &candidates {
        let key = (candidate.record_form_id & 0x00FF_FFFF, candidate.swap_sig);
        let Some(source_swap) = source_swap_ref.get(&key) else {
            unresolved += 1;
            continue;
        };
        if !pairs_by_object_id.contains_key(source_swap) {
            unresolved += 1;
            continue;
        }
        let Some(clone_object_id) = clone_by_source.get(source_swap) else {
            continue;
        };
        replacements.insert(
            (candidate.record_form_id, candidate.swap_sig),
            *clone_object_id,
        );
    }
    if additions.is_empty() || replacements.is_empty() {
        return Ok(Report {
            unresolved,
            ..Report::default()
        });
    }

    ctx.check_cancel()?;

    // Stage C — write.
    let added = additions.len() as u32;
    let schema = ctx.run.schema_target.clone();
    let mut store = plugin_handle_store_ref()
        .lock()
        .map_err(|_| PhaseError::Internal("plugin handle store poisoned".into()))?;
    let target = store.get_mut(&target_handle_id).ok_or_else(|| {
        PhaseError::BadParams(format!("unknown target handle: {target_handle_id}"))
    })?;
    let own_index = (target.parsed.header.masters.len() & 0xFF) as u32;
    for record in additions {
        crate::target_write::add_record_in_slot(target, record, &schema, &ctx.run.interner)
            .map_err(|e| PhaseError::Internal(format!("swap clone insert failed: {e}")))?;
    }
    // Candidate ids were collected raw; the stored refs carry the own-plugin
    // index, so re-key the replacements into full FormIDs before rewriting.
    let indexed: FxHashMap<(u32, &'static str), u32> = replacements
        .into_iter()
        .map(|((record_form_id, sig), object_id)| {
            ((record_form_id, sig), (own_index << 24) | object_id)
        })
        .collect();
    let changed = repoint(&mut target.parsed.root_items, &indexed);
    Ok(Report {
        changed,
        added,
        unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use esp_authoring_core::plugin_runtime::ParsedSubrecord;

    fn sub(sig: &str, data: &[u8]) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::from(sig),
            data: Bytes::copy_from_slice(data),
            semantic_type: None,
        }
    }

    fn stat(form_id: u32, subrecords: Vec<ParsedSubrecord>) -> ParsedItem {
        ParsedItem::Record(ParsedRecord {
            signature: SmolStr::from("STAT"),
            form_id,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords,
            raw_payload: None,
            parse_error: None,
        })
    }

    #[test]
    fn clone_ids_come_from_the_shared_mapper_and_never_reuse_a_claimed_id() {
        let mut interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new(
            [],
            crate::formkey_mapper::MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                generated_object_id_floor: 0x00B1_1A74,
                ..Default::default()
            },
            &mut interner,
        );
        // Another synthesis pass already owns the first two ids above the floor
        // (the navmesh-gen cell and its placed refs, in the shipped case).
        mapper.reserve_object_ids([0x00B1_1A74, 0x00B1_1A75]);

        let ids = allocate_clone_ids(&mut mapper, [0x02_0B98, 0x02_0B9B]);

        let mut allocated: Vec<u32> = ids.values().copied().collect();
        allocated.sort_unstable();
        assert_eq!(allocated, vec![0x00B1_1A76, 0x00B1_1A77]);

        // Allocating again must not hand back an id already issued: that is the
        // registration a private counter skips.
        let later = allocate_clone_ids(&mut mapper, [0x03_0000]);
        let later_id = later[&0x03_0000];
        assert!(!allocated.contains(&later_id));
    }

    #[test]
    fn namespaced_model_path_is_recognized_with_and_without_meshes_root() {
        assert!(is_namespaced_model_path(
            "FO76\\SetDressing\\Minutemen\\FlagpoleMinutemen02.nif",
            "FO76"
        ));
        assert!(is_namespaced_model_path(
            "Meshes\\FO76\\Landscape\\Rocks\\RockCliff01.nif",
            "FO76"
        ));
        assert!(is_namespaced_model_path("fo76/landscape/x.nif", "FO76"));
        assert!(!is_namespaced_model_path(
            "SetDressing\\Minutemen\\FlagpoleMinutemen02.nif",
            "FO76"
        ));
        // A folder that merely starts with the namespace must not match.
        assert!(!is_namespaced_model_path("FO76Extra\\x.nif", "FO76"));
    }

    #[test]
    fn candidate_needs_both_a_namespaced_model_and_an_out_of_plugin_swap() {
        let own_index = 0x08u32;
        let items = vec![
            // Relocated model + base-owned swap -> candidate.
            stat(
                0x0817_FE47,
                vec![
                    sub(
                        "MODL",
                        b"FO76\\SetDressing\\Minutemen\\FlagpoleMinutemen02.nif\0",
                    ),
                    sub("MODS", &0x0017_FE49u32.to_le_bytes()),
                ],
            ),
            // Relocated model + own swap -> already handled by pass 1.
            stat(
                0x0817_FE50,
                vec![
                    sub(
                        "MODL",
                        b"FO76\\SetDressing\\Minutemen\\FlagpoleMinutemen02.nif\0",
                    ),
                    sub("MODS", &0x085D_C922u32.to_le_bytes()),
                ],
            ),
            // Un-relocated model + base swap -> nothing to reconcile.
            stat(
                0x0817_FE26,
                vec![
                    sub("MODL", b"SetDressing\\Flag\\FlagpoleUSA01.nif\0"),
                    sub("MODS", &0x0017_FE27u32.to_le_bytes()),
                ],
            ),
        ];

        let candidates = collect_candidates(&items, "FO76", own_index);

        assert_eq!(
            candidates,
            vec![Candidate {
                record_form_id: 0x0817_FE47,
                swap_sig: "MODS",
            }]
        );
    }

    fn zsub(sig: &str, value: &str) -> (SmolStr, Vec<u8>) {
        let mut bytes = value.as_bytes().to_vec();
        bytes.push(0);
        (SmolStr::from(sig), bytes)
    }

    #[test]
    fn swap_namespaces_only_relocated_originals_and_keeps_extras() {
        let relocation_members =
            HashSet::from(["materials/setdressing/minutemen/flagminutemen01.bgsm".to_string()]);
        let subrecords = vec![
            zsub("FNAM", "SetDressing"),
            zsub("BNAM", "SetDressing\\Minutemen\\FlagMinutemen01.BGSM"),
            zsub("SNAM", "SetDressing\\BOSFlag01.BGSM"),
            zsub("BNAM", "SetDressing\\Power\\PowerSwitchBox01.BGSM"),
            zsub("SNAM", "SetDressing\\Other01.BGSM"),
        ];

        let out = namespaced_subrecords(&subrecords, &relocation_members, "FO76")
            .expect("clone required");

        // Tree folder survives rather than being rebuilt away.
        assert_eq!(out[0].0.as_str(), "FNAM");
        assert_eq!(zstring(&out[0].1).unwrap(), "SetDressing");
        assert_eq!(
            zstring(&out[1].1).unwrap(),
            "FO76\\SetDressing\\Minutemen\\FlagMinutemen01.BGSM"
        );
        // Replacement points at an FO4 base material and must stay put.
        assert_eq!(zstring(&out[2].1).unwrap(), "SetDressing\\BOSFlag01.BGSM");
        // Untouched by relocation -> left alone.
        assert_eq!(
            zstring(&out[3].1).unwrap(),
            "SetDressing\\Power\\PowerSwitchBox01.BGSM"
        );
    }

    #[test]
    fn swap_touching_no_relocated_material_is_not_cloned() {
        let relocation_members =
            HashSet::from(["materials/landscape/rocks/rock01.bgsm".to_string()]);
        let subrecords = vec![
            zsub("BNAM", "SetDressing\\Minutemen\\FlagMinutemen01.BGSM"),
            zsub("SNAM", "SetDressing\\BOSFlag01.BGSM"),
        ];

        assert!(namespaced_subrecords(&subrecords, &relocation_members, "FO76").is_none());
    }

    #[test]
    fn swap_subrecords_drop_edid_and_require_an_original() {
        let mswp = ParsedRecord {
            signature: SmolStr::from("MSWP"),
            form_id: 0x0017_FE49,
            flags: 0,
            version_control: 0,
            form_version: None,
            version2: None,
            subrecords: vec![
                sub("EDID", b"Flag_FlagMinutemen01_to_BOSFlag_02\0"),
                sub("BNAM", b"SetDressing\\Minutemen\\FlagMinutemen01.BGSM\0"),
                sub("SNAM", b"SetDressing\\BOSFlag01.BGSM\0"),
            ],
            raw_payload: None,
            parse_error: None,
        };

        let out = swap_subrecords_from_record(&mswp).expect("has an original");

        assert_eq!(
            out.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(),
            vec!["BNAM", "SNAM"]
        );

        let mut no_original = mswp.clone();
        no_original
            .subrecords
            .retain(|s| s.signature.as_str() != "BNAM");
        assert!(swap_subrecords_from_record(&no_original).is_none());
    }

    #[test]
    fn repoint_rewrites_only_the_mapped_slot() {
        let mut items = vec![stat(
            0x0817_FE47,
            vec![
                sub(
                    "MODL",
                    b"FO76\\SetDressing\\Minutemen\\FlagpoleMinutemen02.nif\0",
                ),
                sub("MODS", &0x0017_FE49u32.to_le_bytes()),
            ],
        )];
        let replacements = FxHashMap::from_iter([((0x0817_FE47u32, "MODS"), 0x0801_0000u32)]);

        assert_eq!(repoint(&mut items, &replacements), 1);

        let ParsedItem::Record(rec) = &items[0] else {
            panic!("expected record");
        };
        let mods = rec
            .subrecords
            .iter()
            .find(|s| s.signature.as_str() == "MODS")
            .unwrap();
        assert_eq!(form_id(&mods.data), Some(0x0801_0000));
    }
}
