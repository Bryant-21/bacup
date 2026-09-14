//! Materialize an FO76 NPC's *inherited* object template into the FO4 record.
//!
//! # Why this exists
//! FO76 `ACBS.template_flags` carries a 14th bit (`0x2000`) FO4 has no slot for,
//! and its partner is `TPTA` slot 13 — FO76 `TPTA` is 14 FormIDs where FO4's is
//! 13. The bit means "take the object template from the template chain instead
//! of carrying one". Across `SeventySix.esm` the bit and an own `OBTE` block are
//! perfectly exclusive — 3087 records set the bit and none of them has a block;
//! 1174 carry a block and none of them sets the bit — which is what identifies
//! it. Translation maps `TPTA` slots 0..=12 index-identity and drops slot 13
//! along with the flag, so the inheritance link is gone by the time any fixup
//! could see it.
//!
//! FO4 *does* inherit an object template through `TPLT`: 120 vanilla
//! `Fallout4.esm` NPCs have no block while their template does, and all 93
//! robots among them (SentryBot / MrGutsy / Assaultron Legendary variants, which
//! plainly render in game) also set `Traits`. Not one vanilla robot inherits with
//! `Traits` clear. So an NPC that wants the object template *without* trait
//! inheritance — exactly what the FO76 bit expresses — has no FO4 encoding, and
//! its body silently vanishes. `HTO_LvlPRC_Liberator` loses its armor shell that
//! way.
//!
//! This indexes the source plugin, resolves each affected record's donor through
//! the `TPTA` slot / `TPLT` / `LVLN` chain, and the hook splices the donor's
//! `OBTE`..`STOP` run in during `pre_translate` — before the global field drop,
//! so a spliced block travels exactly the same path as a natively carried one.
//! Records that keep `Traits` are left alone; FO4 inherits those itself.

use super::*;
use esp_authoring_core::plugin_runtime::effective_subrecords_for_record;
use rustc_hash::FxHashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// FO76-only `ACBS.template_flags` bit: inherit the object template.
const FO76_TEMPLATE_USE_OBJECT_TEMPLATE: u16 = 0x2000;
const NPC_TEMPLATE_TRAITS: u16 = 0x0001;
const ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;
/// FO76 `TPTA` is 14 FormIDs; slot 13 is the object-template source.
const FO76_TPTA_LEN: usize = 14 * 4;
const FO76_TPTA_OBJECT_TEMPLATE_SLOT_OFFSET: usize = 13 * 4;
/// Chains are a few links deep in practice; this only stops a malformed cycle
/// that `seen` somehow misses from running away.
const MAX_TEMPLATE_CHAIN_DEPTH: usize = 12;

type ObjectTemplateBlock = Vec<(SubrecordSig, Vec<u8>)>;

/// Recipient object id → the donor's verbatim `OBTE`..`STOP` subrecord run.
pub(crate) struct InheritedObjectTemplateCatalog {
    blocks: FxHashMap<u32, ObjectTemplateBlock>,
}

impl InheritedObjectTemplateCatalog {
    fn new() -> Self {
        Self {
            blocks: FxHashMap::default(),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.blocks.len()
    }

    fn get(&self, object_id: u32) -> Option<&ObjectTemplateBlock> {
        self.blocks.get(&(object_id & 0x00FF_FFFF))
    }
}

fn catalog_slot() -> &'static RwLock<Option<Arc<InheritedObjectTemplateCatalog>>> {
    static SLOT: OnceLock<RwLock<Option<Arc<InheritedObjectTemplateCatalog>>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Publish the catalog for the run's pair hooks. The hook sees one `Record` at a
/// time, so this cross-record resolution has to reach it out of band.
pub(crate) fn install_inherited_object_template_catalog(catalog: InheritedObjectTemplateCatalog) {
    if let Ok(mut slot) = catalog_slot().write() {
        *slot = Some(Arc::new(catalog));
    }
}

pub(crate) fn inherited_object_template_catalog() -> Option<Arc<InheritedObjectTemplateCatalog>> {
    catalog_slot().read().ok().and_then(|slot| slot.clone())
}

/// What the resolver needs from one source `NPC_`.
struct SourceNpc {
    template_flags: u16,
    /// `TPTA` slot 13, raw (master byte intact).
    template_slot: u32,
    /// `TPLT`, raw.
    default_template: u32,
    block: Option<ObjectTemplateBlock>,
}

/// Index the source plugin and resolve every NPC whose object template FO4
/// cannot inherit for it.
pub(crate) fn build_inherited_object_template_catalog(
    source_handle_id: u64,
    interner: &crate::sym::StringInterner,
) -> Result<InheritedObjectTemplateCatalog, String> {
    let npc_sig = crate::ids::SigCode::from_str("NPC_").map_err(|err| err.to_string())?;
    let lvln_sig = crate::ids::SigCode::from_str("LVLN").map_err(|err| err.to_string())?;
    let (_, masters) = crate::source_read::plugin_context_for_handle(source_handle_id)
        .map_err(|err| err.to_string())?;
    let own_master_index = u8::try_from(masters.len()).unwrap_or(0);

    let npcs = read_source_npcs(source_handle_id, npc_sig, interner)?;
    let leveled = read_source_leveled_npcs(source_handle_id, lvln_sig, interner)?;

    let mut catalog = InheritedObjectTemplateCatalog::new();
    for (local, npc) in &npcs {
        if npc.template_flags & FO76_TEMPLATE_USE_OBJECT_TEMPLATE == 0 || npc.block.is_some() {
            continue;
        }
        // Keeping `Traits` means FO4 inherits the block on its own, the way
        // every vanilla robot variant does. Only the trait-less case is broken.
        if npc.template_flags & NPC_TEMPLATE_TRAITS != 0 {
            continue;
        }
        let start = if npc.template_slot != 0 {
            npc.template_slot
        } else {
            npc.default_template
        };
        let mut seen = rustc_hash::FxHashSet::default();
        if let Some(block) =
            resolve_donor_block(start, own_master_index, &npcs, &leveled, 0, &mut seen)
        {
            catalog.blocks.insert(*local, block.clone());
        }
    }
    Ok(catalog)
}

fn own_object_id(raw_form_id: u32, own_master_index: u8) -> Option<u32> {
    if (raw_form_id >> 24) as u8 != own_master_index || raw_form_id == 0 {
        return None;
    }
    Some(raw_form_id & 0x00FF_FFFF)
}

fn resolve_donor_block<'a>(
    raw_form_id: u32,
    own_master_index: u8,
    npcs: &'a FxHashMap<u32, SourceNpc>,
    leveled: &FxHashMap<u32, Vec<u32>>,
    depth: usize,
    seen: &mut rustc_hash::FxHashSet<u32>,
) -> Option<&'a ObjectTemplateBlock> {
    if depth > MAX_TEMPLATE_CHAIN_DEPTH {
        return None;
    }
    let local = own_object_id(raw_form_id, own_master_index)?;
    if !seen.insert(local) {
        return None;
    }
    if let Some(npc) = npcs.get(&local) {
        if let Some(block) = &npc.block {
            return Some(block);
        }
        for next in [npc.template_slot, npc.default_template] {
            if let Some(found) =
                resolve_donor_block(next, own_master_index, npcs, leveled, depth + 1, seen)
            {
                return Some(found);
            }
        }
    }
    for entry in leveled.get(&local).into_iter().flatten() {
        if let Some(found) =
            resolve_donor_block(*entry, own_master_index, npcs, leveled, depth + 1, seen)
        {
            return Some(found);
        }
    }
    None
}

fn read_source_npcs(
    source_handle_id: u64,
    sig: crate::ids::SigCode,
    interner: &crate::sym::StringInterner,
) -> Result<FxHashMap<u32, SourceNpc>, String> {
    let mut out = FxHashMap::default();
    let form_keys = crate::source_read::iter_form_keys_of_sig(source_handle_id, sig, interner)
        .map_err(|err| err.to_string())?;
    if form_keys.is_empty() {
        return Ok(out);
    }
    let snapshot =
        crate::source_read::snapshot_records_by_form_keys(source_handle_id, &form_keys, interner)
            .map_err(|err| err.to_string())?;
    for record in &snapshot.records {
        let subrecords = effective_subrecords_for_record(&record.raw_record);
        let mut npc = SourceNpc {
            template_flags: 0,
            template_slot: 0,
            default_template: 0,
            block: None,
        };
        let mut block: ObjectTemplateBlock = Vec::new();
        let mut in_block = false;
        for subrecord in subrecords.iter() {
            let data = subrecord.data.as_ref();
            match subrecord.signature.as_str() {
                "ACBS" if data.len() >= ACBS_TEMPLATE_FLAGS_OFFSET + 2 => {
                    npc.template_flags = u16::from_le_bytes([
                        data[ACBS_TEMPLATE_FLAGS_OFFSET],
                        data[ACBS_TEMPLATE_FLAGS_OFFSET + 1],
                    ]);
                }
                "TPTA" if data.len() >= FO76_TPTA_LEN => {
                    npc.template_slot = u32::from_le_bytes(
                        data[FO76_TPTA_OBJECT_TEMPLATE_SLOT_OFFSET
                            ..FO76_TPTA_OBJECT_TEMPLATE_SLOT_OFFSET + 4]
                            .try_into()
                            .unwrap_or([0; 4]),
                    );
                }
                "TPLT" if data.len() >= 4 => {
                    npc.default_template =
                        u32::from_le_bytes(data[..4].try_into().unwrap_or([0; 4]));
                }
                other => {
                    if other == "OBTE" {
                        in_block = true;
                    }
                    if in_block {
                        let mut sig_bytes = [0u8; 4];
                        sig_bytes.copy_from_slice(other.as_bytes());
                        block.push((SubrecordSig(sig_bytes), data.to_vec()));
                        if other == "STOP" {
                            in_block = false;
                        }
                    }
                }
            }
        }
        if !block.is_empty() {
            npc.block = Some(block);
        }
        out.insert(record.form_key.local & 0x00FF_FFFF, npc);
    }
    Ok(out)
}

fn read_source_leveled_npcs(
    source_handle_id: u64,
    sig: crate::ids::SigCode,
    interner: &crate::sym::StringInterner,
) -> Result<FxHashMap<u32, Vec<u32>>, String> {
    let mut out = FxHashMap::default();
    let form_keys = crate::source_read::iter_form_keys_of_sig(source_handle_id, sig, interner)
        .map_err(|err| err.to_string())?;
    if form_keys.is_empty() {
        return Ok(out);
    }
    let snapshot =
        crate::source_read::snapshot_records_by_form_keys(source_handle_id, &form_keys, interner)
            .map_err(|err| err.to_string())?;
    for record in &snapshot.records {
        let mut entries = Vec::new();
        for subrecord in effective_subrecords_for_record(&record.raw_record).iter() {
            if subrecord.signature.as_str() != "LVLO" {
                continue;
            }
            // FO76 `LVLO` is a bare FormID (4027 rows in SeventySix.esm); the
            // 174 remaining rows are the legacy 12-byte level/ref/count form.
            let data = subrecord.data.as_ref();
            let reference = match data.len() {
                4 => u32::from_le_bytes(data[..4].try_into().unwrap_or([0; 4])),
                len if len >= 8 => u32::from_le_bytes(data[4..8].try_into().unwrap_or([0; 4])),
                _ => continue,
            };
            entries.push(reference);
        }
        out.insert(record.form_key.local & 0x00FF_FFFF, entries);
    }
    Ok(out)
}

impl Fo76Fo4Hook {
    /// Splice the resolved donor's object-template block into an NPC that
    /// inherits one through the FO76-only template slot.
    ///
    /// The block goes immediately before `CNAM`, which follows it in all 1174
    /// `SeventySix.esm` records that carry one natively.
    pub(super) fn materialize_inherited_object_template(record: &mut Record) {
        if record.sig.0 != *b"NPC_" || record.fields.iter().any(|entry| entry.sig.0 == *b"OBTE") {
            return;
        }
        let Some(catalog) = inherited_object_template_catalog() else {
            return;
        };
        let Some(block) = catalog.get(record.form_key.local) else {
            return;
        };
        splice_object_template_block(record, block);
    }
}

/// Insert `block` immediately before `CNAM`. Returns `false` when the record has
/// no `CNAM` — the position is not guessed, since a misplaced `OBTE` run reads
/// as an out-of-order subrecord.
fn splice_object_template_block(record: &mut Record, block: &ObjectTemplateBlock) -> bool {
    let Some(anchor) = record
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"CNAM")
    else {
        return false;
    };
    let tail: Vec<FieldEntry> = record.fields.drain(anchor..).collect();
    for (sig, data) in block {
        record.fields.push(FieldEntry {
            sig: *sig,
            value: FieldValue::Bytes(SmallVec::from_slice(data)),
        });
    }
    record.fields.extend(tail);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;

    fn npc(interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("NPC_").unwrap(),
            form_key: FormKey {
                local: 0x00A5_1234,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push(record: &mut Record, sig: &str) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::new()),
        });
    }

    fn donor_block() -> ObjectTemplateBlock {
        vec![
            (SubrecordSig(*b"OBTE"), 1u32.to_le_bytes().to_vec()),
            (SubrecordSig(*b"OBTS"), vec![0u8; 16]),
            (SubrecordSig(*b"STOP"), Vec::new()),
        ]
    }

    fn sigs(record: &Record) -> Vec<String> {
        record
            .fields
            .iter()
            .map(|entry| String::from_utf8_lossy(&entry.sig.0).into_owned())
            .collect()
    }

    #[test]
    fn splices_donor_block_immediately_before_cnam() {
        let interner = StringInterner::new();
        let mut record = npc(&interner);
        for sig in ["EDID", "ACBS", "KSIZ", "KWDA", "CNAM", "FULL", "DNAM"] {
            push(&mut record, sig);
        }

        assert!(splice_object_template_block(&mut record, &donor_block()));

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "ACBS", "KSIZ", "KWDA", "OBTE", "OBTS", "STOP", "CNAM", "FULL", "DNAM"
            ]
        );
    }

    #[test]
    fn declines_to_guess_a_position_without_cnam() {
        let interner = StringInterner::new();
        let mut record = npc(&interner);
        for sig in ["EDID", "ACBS", "DNAM"] {
            push(&mut record, sig);
        }

        assert!(!splice_object_template_block(&mut record, &donor_block()));
        assert_eq!(sigs(&record), vec!["EDID", "ACBS", "DNAM"]);
    }

    fn source_npc(flags: u16, slot: u32, tplt: u32, block: bool) -> SourceNpc {
        SourceNpc {
            template_flags: flags,
            template_slot: slot,
            default_template: tplt,
            block: block.then(donor_block),
        }
    }

    /// The real Liberator shape: recipient → `TPTA` slot 13 → `TPLT` → `LVLN`
    /// → leaf that carries the block.
    #[test]
    fn resolves_donor_through_tpta_slot_tplt_and_lvln() {
        let mut npcs = FxHashMap::default();
        npcs.insert(
            0x00_2ECE,
            source_npc(0x1CFE, 0, 0x00_1D51_10 & 0x00FF_FFFF, false),
        );
        npcs.insert(0x00_75FE, source_npc(0, 0, 0, true));
        let mut leveled = FxHashMap::default();
        leveled.insert(0x00_1D5110 & 0x00FF_FFFF, vec![0x00_75FE]);

        let mut seen = rustc_hash::FxHashSet::default();
        let found = resolve_donor_block(0x00_2ECE, 0, &npcs, &leveled, 0, &mut seen);
        assert_eq!(found, Some(&donor_block()));
    }

    #[test]
    fn cycle_in_the_template_chain_terminates() {
        let mut npcs = FxHashMap::default();
        npcs.insert(0x00_0001, source_npc(0, 0x00_0002, 0, false));
        npcs.insert(0x00_0002, source_npc(0, 0x00_0001, 0, false));
        let leveled = FxHashMap::default();

        let mut seen = rustc_hash::FxHashSet::default();
        assert!(resolve_donor_block(0x00_0001, 0, &npcs, &leveled, 0, &mut seen).is_none());
    }

    #[test]
    fn ignores_form_ids_owned_by_a_master() {
        let mut npcs = FxHashMap::default();
        npcs.insert(0x00_0002, source_npc(0, 0, 0, true));
        let leveled = FxHashMap::default();

        let mut seen = rustc_hash::FxHashSet::default();
        // own_master_index 1: a 0x00-prefixed id belongs to a master, not us.
        assert!(resolve_donor_block(0x0000_0002, 1, &npcs, &leveled, 0, &mut seen).is_none());
    }
}
