//! Strip NPC_ morph entries whose index is not valid for the NPC's linked (FO4)
//! race. Covers two distinct NPC morph subrecords:
//!
//! - `Face Morphs` — repeated `FMRI` (u32 index) + `FMRS` (morph values) pairs.
//!   `FMRI` is an index into the linked RACE's per-gender Face Morphs table
//!   (xEdit `wbFaceMorphToStr`, validated against `RACE` `FMRI`/`FMRN`).
//! - `Morph Keys` — `MSDK`, an `array_struct:I` of u32 keys. Each key is
//!   validated by xEdit `wbMorphValueToStr` against the linked RACE's
//!   Morph-Group preset indices (`MPPI`) ∪ Morph-Value ids (`MSID`).
//!
//! After a FO76→FO4 conversion an NPC's `RNAM` resolves to a FO4 race whose
//! tables lack the FO76-only indices (and, if a stripped RACE override was
//! emitted, may carry no table at all). xEdit then reports "… index [N] not
//! found in <race>" for every such index.
//!
//! # What this does
//! For each output NPC_ carrying FMRI or MSDK morph data:
//!   1. Resolve `RNAM` → race FormKey.
//!   2. Read the race's valid-index sets (sex-specific FMRI; and MPPI∪MSID).
//!      Prefer the race in the output plugin; fall back to the target master
//!      handles (HumanRace / GhoulRace / LostRace etc. live in `Fallout4.esm`).
//!   3. Drop each `FMRI` row (and its paired `FMRS`) whose index ∉ the NPC
//!      sex's FMRI set; filter the `MSDK` u32 array in place, dropping keys ∉
//!      the morph-key set.
//!      A race that resolves with an empty table means every index is invalid →
//!      everything is stripped (an empty valid set drops all).
//!
//! When the race can't be resolved/read anywhere (RNAM unresolved or no RACE
//! record in output or masters), the NPC is left UNTOUCHED — never strip blind.
//! There is no master-independent floor that can decide validity: FO4 vanilla
//! RACE FMRI tables carry high chargen morph IDs, so a high index is NOT proof
//! of a FO76-only morph. The subtle failure mode is sex: Human/Ghoul RACE FMRI
//! tables are stored as male block then female block. Merging the two blocks
//! makes opposite-sex morph rows look valid, but xEdit validates against the
//! NPC sex-specific block and reports every opposite-sex index as "not found".
//!
//! Policy: strip, do not remap. There is no safe mapping of a FO76 morph index
//! onto a different race's table; a wrong index is worse than a missing one.

use crate::fixups::prune_orphaned_records::is_creature_root_sig;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::StringInterner;
use rustc_hash::{FxHashMap, FxHashSet};

const BASE_MASTER: &str = "Fallout4.esm";
const HUMAN_RACE_LOCAL: u32 = 0x0001_3746;
const GHOUL_RACE_LOCAL: u32 = 0x000E_AFB6;
const NPC_ACBS_FEMALE_FLAG: u32 = 0x0000_0001;

pub struct StripInvalidNpcFaceMorphsFixup;

impl Fixup for StripInvalidNpcFaceMorphsFixup {
    fn name(&self) -> &'static str {
        "strip_invalid_npc_face_morphs"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::WholePluginSafe
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        // Runs for NPC-rooted sub-graph conversions and for whole-plugin runs
        // (root_sig None). Skips conversions rooted at unrelated single records.
        match ctx.config.root_sig {
            Some(sig) => is_creature_root_sig(sig),
            None => true,
        }
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        match config.root_sig {
            Some(sig) => is_creature_root_sig(sig),
            None => true,
        }
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .clone()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let npc_fks = session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if npc_fks.is_empty() {
            return Ok(report);
        }

        let master_handle_ids = config.target_master_handle_ids.clone();

        // Cache per-race valid index sets so repeated NPCs of the same race only
        // pay the race decode once. `None` = race unresolved/unreadable → leave
        // the NPC untouched (never strip blind).
        let mut race_valid: FxHashMap<FormKey, Option<RaceMorphSets>> = FxHashMap::default();

        for fk in &npc_fks {
            let mut record =
                match session.record_decoded(fk, target_schema.as_ref(), mapper.interner) {
                    Ok(r) => r,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("strip_face_morphs_npc_read:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };

            if !record_has_morph_data(&record) {
                continue;
            }

            let race_fk = match resolve_npc_race(&record) {
                Some(fk) => fk,
                None => continue,
            };

            if !race_valid.contains_key(&race_fk) {
                let sets = read_race_morph_sets(
                    session,
                    &race_fk,
                    target_schema.as_ref(),
                    &master_handle_ids,
                    mapper.interner,
                );
                race_valid.insert(race_fk, sets);
            }
            let removed = match race_valid.get(&race_fk) {
                Some(Some(sets)) => {
                    // Race readable: strip every NPC morph index not in the
                    // race's table (empty set ⇒ strip all).
                    let valid_fmri = sets.fmri_for_npc(&record, &race_fk, mapper.interner);
                    let mut r = drop_invalid_face_morphs(&mut record, valid_fmri);
                    r += filter_invalid_morph_keys(&mut record, &sets.morph_keys);
                    r
                }
                _ => {
                    // Race not readable anywhere (e.g. a vanilla race in a master
                    // that isn't loaded for this run). Leave the NPC UNTOUCHED:
                    // there is no master-independent rule that can tell a valid
                    // high FMRI index (FO4 races carry indices up to 921646279)
                    // from a FO76-only one, so stripping by magnitude would
                    // delete valid morphs. Never strip blind.
                    0
                }
            };
            if removed > 0 {
                session
                    .replace_record(record, target_schema.as_ref(), mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                report.records_changed += 1;
                report.records_dropped += removed;
            }
        }

        Ok(report)
    }
}

/// Valid morph-index sets for one race, as xEdit validates NPC morph data.
struct RaceMorphSets {
    /// All RACE `FMRI` indices. Used as the fallback when a race is not known
    /// to use FO4 Human/Ghoul's male-then-female table layout.
    fmri_all: FxHashSet<u32>,
    /// Male RACE `FMRI` indices, when the table can be split by sex.
    fmri_male: FxHashSet<u32>,
    /// Female RACE `FMRI` indices, when the table can be split by sex.
    fmri_female: FxHashSet<u32>,
    /// RACE Morph-Group preset indices (`MPPI`) ∪ Morph-Value ids (`MSID`) —
    /// validate NPC `Morph Keys` (MSDK).
    morph_keys: FxHashSet<u32>,
}

impl RaceMorphSets {
    fn fmri_for_npc(
        &self,
        npc: &Record,
        race_fk: &FormKey,
        interner: &StringInterner,
    ) -> &FxHashSet<u32> {
        if !is_fo4_human_or_ghoul_race(race_fk, interner) {
            return &self.fmri_all;
        }
        match npc_sex(npc) {
            Some(NpcSex::Female) if !self.fmri_female.is_empty() => &self.fmri_female,
            Some(NpcSex::Male) if !self.fmri_male.is_empty() => &self.fmri_male,
            _ => &self.fmri_all,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NpcSex {
    Male,
    Female,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Whether the record carries NPC morph data: a `FMRI` Face-Morph index or an
/// `MSDK` Morph-Keys array.
fn record_has_morph_data(record: &Record) -> bool {
    let (Ok(fmri), Ok(msdk)) = (
        SubrecordSig::from_str("FMRI"),
        SubrecordSig::from_str("MSDK"),
    ) else {
        return false;
    };
    record.fields.iter().any(|e| e.sig == fmri || e.sig == msdk)
}

/// Resolve the NPC's `RNAM` (Race) to a FormKey, or `None` when absent/null.
fn resolve_npc_race(record: &Record) -> Option<FormKey> {
    let rnam = SubrecordSig::from_str("RNAM").ok()?;
    for entry in &record.fields {
        if entry.sig != rnam {
            continue;
        }
        return match &entry.value {
            FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
            // A formid subrecord decodes to FormKey; Bytes is the raw fallback
            // (4-byte form id) which we cannot resolve to a plugin here without
            // master context — those are handled by the FormKey path after the
            // FormKeyMapper rewrite, so a Bytes RNAM means "leave untouched".
            _ => None,
        };
    }
    None
}

/// Collect the valid morph-index sets for `race_fk`. Tries the output plugin
/// first, then each target master handle (HumanRace/GhoulRace/LostRace etc.
/// live in Fallout4.esm). Returns `None` only when the race record can't be
/// found/decoded anywhere (callers then leave the NPC untouched); returns
/// `Some(sets)` — possibly with empty sets — when the race was read. Empty sets
/// are meaningful: they mean every NPC morph index is invalid → strip all.
fn read_race_morph_sets(
    session: &mut PluginSession,
    race_fk: &FormKey,
    schema: &AuthoringSchema,
    master_handle_ids: &[u64],
    interner: &StringInterner,
) -> Option<RaceMorphSets> {
    // Output plugin first (a converted RACE override carries the table iff its
    // morph subrecords were not stripped).
    if let Ok(record) = session.record_decoded(race_fk, schema, interner) {
        if record.sig.as_str() == "RACE" {
            return Some(collect_race_morph_sets(&record));
        }
    }
    // Then target masters.
    for &handle_id in master_handle_ids {
        if let Ok(record) = session.record_decoded_in_handle(handle_id, race_fk, schema, interner) {
            if record.sig.as_str() == "RACE" {
                return Some(collect_race_morph_sets(&record));
            }
        }
    }
    None
}

/// Gather a race's valid morph-index sets:
/// - `fmri`: every `FMRI` (u32) value.
/// - `morph_keys`: every `MPPI` (Morph-Group preset index) ∪ `MSID` (Morph-Value
///   id), matching xEdit `wbMorphValueToStr`'s validation source for MSDK.
fn collect_race_morph_sets(record: &Record) -> RaceMorphSets {
    let mut fmri_values = Vec::new();
    let mut morph_keys = FxHashSet::default();
    let (Ok(fmri_sig), Ok(mppi_sig), Ok(msid_sig)) = (
        SubrecordSig::from_str("FMRI"),
        SubrecordSig::from_str("MPPI"),
        SubrecordSig::from_str("MSID"),
    ) else {
        return RaceMorphSets {
            fmri_all: FxHashSet::default(),
            fmri_male: FxHashSet::default(),
            fmri_female: FxHashSet::default(),
            morph_keys,
        };
    };
    for entry in &record.fields {
        if entry.sig == fmri_sig {
            if let Some(i) = field_u32(&entry.value) {
                fmri_values.push(i);
            }
        } else if entry.sig == mppi_sig || entry.sig == msid_sig {
            if let Some(i) = field_u32(&entry.value) {
                morph_keys.insert(i);
            }
        }
    }
    let fmri_all = fmri_values.iter().copied().collect();
    let (fmri_male, fmri_female) = split_race_fmri_by_sex(&fmri_values);
    RaceMorphSets {
        fmri_all,
        fmri_male,
        fmri_female,
        morph_keys,
    }
}

fn split_race_fmri_by_sex(values: &[u32]) -> (FxHashSet<u32>, FxHashSet<u32>) {
    if values.len() < 2 || values.len() % 2 != 0 {
        return (FxHashSet::default(), FxHashSet::default());
    }
    let split = values.len() / 2;
    (
        values[..split].iter().copied().collect(),
        values[split..].iter().copied().collect(),
    )
}

fn is_fo4_human_or_ghoul_race(race_fk: &FormKey, interner: &StringInterner) -> bool {
    matches!(race_fk.local, HUMAN_RACE_LOCAL | GHOUL_RACE_LOCAL)
        && interner
            .resolve(race_fk.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(BASE_MASTER))
}

fn npc_sex(record: &Record) -> Option<NpcSex> {
    let acbs_sig = SubrecordSig::from_str("ACBS").ok()?;
    for entry in &record.fields {
        if entry.sig != acbs_sig {
            continue;
        }
        return match &entry.value {
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                let flags = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                Some(if flags & NPC_ACBS_FEMALE_FLAG != 0 {
                    NpcSex::Female
                } else {
                    NpcSex::Male
                })
            }
            _ => None,
        };
    }
    None
}

/// Filter the NPC `MSDK` "Morph Keys" array, dropping every u32 key not in
/// `valid`, AND drop the value at the same position in the parallel `MSDV`
/// "Morph Values" array. MSDK (`array_struct:I`, u32 keys) and MSDV
/// (`array_struct:f`, f32 values) are PARALLEL arrays the FO4 engine reads in
/// lockstep (wbDefinitionsFO4.pas: MSDK keys + MSDV values, same count). If we
/// drop a key without dropping its paired value the arrays desync — the engine
/// reads a malformed chunk and CRASHES in TESFile::GetChunkData on load
/// (NPC [0778814A]). So the drop is position-paired: a key dropped at
/// index i drops the value at index i; if MSDK ends empty, MSDV ends empty too.
/// Returns the number of keys removed across all MSDK subrecords.
fn filter_invalid_morph_keys(record: &mut Record, valid: &FxHashSet<u32>) -> u32 {
    let (Ok(msdk_sig), Ok(msdv_sig)) = (
        SubrecordSig::from_str("MSDK"),
        SubrecordSig::from_str("MSDV"),
    ) else {
        return 0;
    };

    // Determine which MSDK positions to keep (true = keep), in order, from the
    // first MSDK subrecord. NPC carries a single MSDK/MSDV pair.
    let mut keep_mask: Option<Vec<bool>> = None;
    let mut removed: u32 = 0;
    for entry in &mut record.fields {
        if entry.sig != msdk_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(raw) if raw.len() % 4 == 0 => {
                let mut mask = Vec::with_capacity(raw.len() / 4);
                let mut kept: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
                for chunk in raw.chunks_exact(4) {
                    let key = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    let keep = valid.contains(&key);
                    mask.push(keep);
                    if keep {
                        kept.extend_from_slice(chunk);
                    } else {
                        removed += 1;
                    }
                }
                *raw = kept;
                keep_mask.get_or_insert(mask);
            }
            FieldValue::List(items) => {
                let mut mask = Vec::with_capacity(items.len());
                items.retain(|item| {
                    let keep = field_u32(item).is_some_and(|k| valid.contains(&k));
                    mask.push(keep);
                    if !keep {
                        removed += 1;
                    }
                    keep
                });
                keep_mask.get_or_insert(mask);
            }
            _ => {}
        }
        break; // single MSDK pair per NPC
    }

    // Apply the same keep-mask to the parallel MSDV value array so the two stay
    // the same length. Without this the NPC crashes the engine on load.
    if let Some(mask) = keep_mask {
        if mask.iter().any(|keep| !keep) {
            apply_keep_mask_to_msdv(record, msdv_sig, &mask);
        }
    }
    removed
}

/// Drop the f32 values in the `MSDV` array at the positions marked `false` in
/// `mask` (the per-index keep decision computed from the parallel MSDK array).
/// Defensive: if MSDV length doesn't match `mask`, only positions within range
/// are filtered and any extra MSDV tail is preserved (never over-read).
fn apply_keep_mask_to_msdv(record: &mut Record, msdv_sig: SubrecordSig, mask: &[bool]) {
    for entry in &mut record.fields {
        if entry.sig != msdv_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(raw) if raw.len() % 4 == 0 => {
                let mut kept: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
                for (i, chunk) in raw.chunks_exact(4).enumerate() {
                    if mask.get(i).copied().unwrap_or(true) {
                        kept.extend_from_slice(chunk);
                    }
                }
                *raw = kept;
            }
            FieldValue::List(items) => {
                let mut i = 0usize;
                items.retain(|_| {
                    let keep = mask.get(i).copied().unwrap_or(true);
                    i += 1;
                    keep
                });
            }
            _ => {}
        }
        break; // single MSDV per NPC
    }
}

/// Drop every NPC `FMRI` row whose index is not in `valid`, together with the
/// `FMRS` row that immediately follows it. Returns the number of `FMRI` rows
/// removed.
fn drop_invalid_face_morphs(record: &mut Record, valid: &FxHashSet<u32>) -> u32 {
    let (Ok(fmri_sig), Ok(fmrs_sig)) = (
        SubrecordSig::from_str("FMRI"),
        SubrecordSig::from_str("FMRS"),
    ) else {
        return 0;
    };

    let mut removed: u32 = 0;
    let mut kept: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    let mut drop_next_fmrs = false;
    for entry in record.fields.drain(..) {
        if entry.sig == fmri_sig {
            let index = field_u32(&entry.value);
            let keep = index.is_some_and(|i| valid.contains(&i));
            if keep {
                drop_next_fmrs = false;
                kept.push(entry);
            } else {
                removed += 1;
                // Drop the FMRS value row paired with this FMRI.
                drop_next_fmrs = true;
            }
            continue;
        }
        if entry.sig == fmrs_sig && drop_next_fmrs {
            drop_next_fmrs = false;
            continue;
        }
        // Any non-FMRS subrecord clears the pending-drop latch (a well-formed
        // record always has FMRS directly after FMRI, but stay defensive).
        if entry.sig != fmrs_sig {
            drop_next_fmrs = false;
        }
        kept.push(entry);
    }
    record.fields = kept;
    removed
}

/// Read a u32 from an FMRI/FMRS-style decoded value. FMRI decodes as
/// `FieldValue::Uint` (codec uint32); the 4-byte `Bytes` fallback is also read.
fn field_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(n) => u32::try_from(*n).ok(),
        FieldValue::Int(n) => u32::try_from(*n).ok(),
        FieldValue::Bytes(b) if b.len() >= 4 => Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::sym::StringInterner;

    fn make_record(sig_str: &str, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig_str).unwrap(),
            form_key: FormKey {
                local: 0x000100,
                plugin: interner.intern("Output.esp"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::SmallVec::new(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn push(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn fmrs_bytes() -> FieldValue {
        // 7 floats + trailing bytes — content irrelevant for the pairing test.
        FieldValue::Bytes(smallvec::SmallVec::from_vec(vec![0u8; 28]))
    }

    fn count_sig(record: &Record, sig: &str) -> usize {
        let s = SubrecordSig::from_str(sig).unwrap();
        record.fields.iter().filter(|e| e.sig == s).count()
    }

    fn acbs(flags: u32) -> FieldValue {
        let mut raw = smallvec::SmallVec::new();
        raw.extend_from_slice(&flags.to_le_bytes());
        raw.resize(20, 0);
        FieldValue::Bytes(raw)
    }

    fn set(values: &[u32]) -> FxHashSet<u32> {
        values.iter().copied().collect()
    }

    fn fmri_values(record: &Record) -> Vec<u32> {
        let fmri_sig = SubrecordSig::from_str("FMRI").unwrap();
        record
            .fields
            .iter()
            .filter(|entry| entry.sig == fmri_sig)
            .filter_map(|entry| field_u32(&entry.value))
            .collect()
    }

    #[test]
    fn drops_invalid_fmri_and_paired_fmrs() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("TestNpc")),
        );
        // valid index 2
        push(&mut record, "FMRI", FieldValue::Uint(2));
        push(&mut record, "FMRS", fmrs_bytes());
        // invalid index 0x36EF34BA (FO76-only)
        push(&mut record, "FMRI", FieldValue::Uint(0x36EF_34BA));
        push(&mut record, "FMRS", fmrs_bytes());
        // valid index 4
        push(&mut record, "FMRI", FieldValue::Uint(4));
        push(&mut record, "FMRS", fmrs_bytes());

        let mut valid = FxHashSet::default();
        valid.insert(2u32);
        valid.insert(4u32);

        let removed = drop_invalid_face_morphs(&mut record, &valid);
        assert_eq!(removed, 1);
        assert_eq!(count_sig(&record, "FMRI"), 2);
        assert_eq!(
            count_sig(&record, "FMRS"),
            2,
            "paired FMRS dropped with FMRI"
        );
        assert_eq!(
            count_sig(&record, "EDID"),
            1,
            "non-morph subrecords preserved"
        );
    }

    #[test]
    fn keeps_high_indices_that_are_valid_in_the_resolved_race() {
        // FO4 vanilla RACE FMRI tables carry indices up to 921646279
        // (0x36EF34C7); a high magnitude is NOT proof of an invalid morph. When
        // the resolved race's table contains them, high indices must be KEPT.
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("DoctorJain")),
        );
        push(&mut record, "FMRI", FieldValue::Uint(58));
        push(&mut record, "FMRS", fmrs_bytes());
        push(&mut record, "FMRI", FieldValue::Uint(100005));
        push(&mut record, "FMRS", fmrs_bytes());
        push(&mut record, "FMRI", FieldValue::Uint(921646277));
        push(&mut record, "FMRS", fmrs_bytes());

        // The HumanRace table contains all three.
        let mut valid = FxHashSet::default();
        valid.insert(58u32);
        valid.insert(100005u32);
        valid.insert(921646277u32);

        let removed = drop_invalid_face_morphs(&mut record, &valid);
        assert_eq!(removed, 0, "every index is valid in the race table");
        assert_eq!(count_sig(&record, "FMRI"), 3, "high valid indices survive");
        assert_eq!(count_sig(&record, "FMRS"), 3);
        assert_eq!(count_sig(&record, "EDID"), 1);
    }

    #[test]
    fn keeps_all_when_every_index_valid() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push(&mut record, "FMRI", FieldValue::Uint(0));
        push(&mut record, "FMRS", fmrs_bytes());
        push(&mut record, "FMRI", FieldValue::Uint(1));
        push(&mut record, "FMRS", fmrs_bytes());

        let mut valid = FxHashSet::default();
        valid.insert(0u32);
        valid.insert(1u32);

        let removed = drop_invalid_face_morphs(&mut record, &valid);
        assert_eq!(removed, 0);
        assert_eq!(count_sig(&record, "FMRI"), 2);
        assert_eq!(count_sig(&record, "FMRS"), 2);
    }

    #[test]
    fn empty_valid_set_drops_all_morphs_but_keeps_other_fields() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("TestNpc")),
        );
        push(&mut record, "FMRI", FieldValue::Uint(0));
        push(&mut record, "FMRS", fmrs_bytes());
        push(&mut record, "FMRI", FieldValue::Uint(2));
        push(&mut record, "FMRS", fmrs_bytes());
        // a trailing non-morph subrecord
        push(&mut record, "FMIN", FieldValue::Float(1.0));

        let valid = FxHashSet::default();
        let removed = drop_invalid_face_morphs(&mut record, &valid);
        assert_eq!(removed, 2);
        assert_eq!(count_sig(&record, "FMRI"), 0);
        assert_eq!(count_sig(&record, "FMRS"), 0);
        assert_eq!(count_sig(&record, "EDID"), 1);
        assert_eq!(count_sig(&record, "FMIN"), 1);
    }

    fn msdk_bytes(keys: &[u32]) -> FieldValue {
        let mut v: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        for k in keys {
            v.extend_from_slice(&k.to_le_bytes());
        }
        FieldValue::Bytes(v)
    }

    #[test]
    fn collect_race_morph_sets_reads_fmri_and_mppi_msid() {
        let interner = StringInterner::new();
        let mut race = make_record("RACE", &interner);
        push(&mut race, "FMRI", FieldValue::Uint(0));
        push(
            &mut race,
            "FMRN",
            FieldValue::String(interner.intern("Brow")),
        );
        push(&mut race, "FMRI", FieldValue::Uint(58));
        push(&mut race, "FMRI", FieldValue::Uint(100000));
        push(&mut race, "FMRI", FieldValue::Uint(100001));
        // morph-group preset indices + morph-value ids feed the MSDK set
        push(&mut race, "MPPI", FieldValue::Uint(100));
        push(&mut race, "MPPI", FieldValue::Uint(101));
        push(&mut race, "MSID", FieldValue::Uint(200));

        let sets = collect_race_morph_sets(&race);
        assert_eq!(sets.fmri_all.len(), 4);
        assert!(sets.fmri_all.contains(&0) && sets.fmri_all.contains(&58));
        assert_eq!(sets.fmri_male, set(&[0, 58]));
        assert_eq!(sets.fmri_female, set(&[100000, 100001]));
        assert_eq!(sets.morph_keys.len(), 3);
        assert!(sets.morph_keys.contains(&100));
        assert!(sets.morph_keys.contains(&101));
        assert!(sets.morph_keys.contains(&200));
        // FMRI and morph-key sets are independent.
        assert!(!sets.morph_keys.contains(&0));
    }

    #[test]
    fn female_human_npc_uses_female_fmri_block() {
        let interner = StringInterner::new();
        let race_fk = FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: HUMAN_RACE_LOCAL,
        };
        let sets = RaceMorphSets {
            fmri_all: set(&[0, 1, 100000, 100001]),
            fmri_male: set(&[0, 1]),
            fmri_female: set(&[100000, 100001]),
            morph_keys: FxHashSet::default(),
        };
        let mut npc = make_record("NPC_", &interner);
        push(&mut npc, "ACBS", acbs(NPC_ACBS_FEMALE_FLAG));
        push(&mut npc, "FMRI", FieldValue::Uint(0));
        push(&mut npc, "FMRS", fmrs_bytes());
        push(&mut npc, "FMRI", FieldValue::Uint(100000));
        push(&mut npc, "FMRS", fmrs_bytes());

        let valid = sets.fmri_for_npc(&npc, &race_fk, &interner);
        assert_eq!(drop_invalid_face_morphs(&mut npc, valid), 1);
        assert_eq!(fmri_values(&npc), vec![100000]);
        assert_eq!(count_sig(&npc, "FMRS"), 1);
    }

    #[test]
    fn male_human_npc_uses_male_fmri_block() {
        let interner = StringInterner::new();
        let race_fk = FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: HUMAN_RACE_LOCAL,
        };
        let sets = RaceMorphSets {
            fmri_all: set(&[0, 1, 100000, 100001]),
            fmri_male: set(&[0, 1]),
            fmri_female: set(&[100000, 100001]),
            morph_keys: FxHashSet::default(),
        };
        let mut npc = make_record("NPC_", &interner);
        push(&mut npc, "ACBS", acbs(0));
        push(&mut npc, "FMRI", FieldValue::Uint(0));
        push(&mut npc, "FMRS", fmrs_bytes());
        push(&mut npc, "FMRI", FieldValue::Uint(100000));
        push(&mut npc, "FMRS", fmrs_bytes());

        let valid = sets.fmri_for_npc(&npc, &race_fk, &interner);
        assert_eq!(drop_invalid_face_morphs(&mut npc, valid), 1);
        assert_eq!(fmri_values(&npc), vec![0]);
        assert_eq!(count_sig(&npc, "FMRS"), 1);
    }

    #[test]
    fn non_human_race_keeps_all_fmri_values() {
        let interner = StringInterner::new();
        let race_fk = FormKey {
            plugin: interner.intern("Output.esp"),
            local: 0x010000,
        };
        let sets = RaceMorphSets {
            fmri_all: set(&[0, 1, 100000, 100001]),
            fmri_male: set(&[0, 1]),
            fmri_female: set(&[100000, 100001]),
            morph_keys: FxHashSet::default(),
        };
        let mut npc = make_record("NPC_", &interner);
        push(&mut npc, "ACBS", acbs(NPC_ACBS_FEMALE_FLAG));
        push(&mut npc, "FMRI", FieldValue::Uint(0));
        push(&mut npc, "FMRS", fmrs_bytes());
        push(&mut npc, "FMRI", FieldValue::Uint(100000));
        push(&mut npc, "FMRS", fmrs_bytes());

        let valid = sets.fmri_for_npc(&npc, &race_fk, &interner);
        assert_eq!(drop_invalid_face_morphs(&mut npc, valid), 0);
        assert_eq!(fmri_values(&npc), vec![0, 100000]);
        assert_eq!(count_sig(&npc, "FMRS"), 2);
    }

    #[test]
    fn filter_invalid_morph_keys_drops_unknown_keys_in_place() {
        let interner = StringInterner::new();
        let mut npc = make_record("NPC_", &interner);
        // keys 100 (valid), 999 (invalid), 200 (valid), 0x12345678 (invalid)
        push(&mut npc, "MSDK", msdk_bytes(&[100, 999, 200, 0x1234_5678]));

        let mut valid = FxHashSet::default();
        valid.insert(100u32);
        valid.insert(200u32);

        let removed = filter_invalid_morph_keys(&mut npc, &valid);
        assert_eq!(removed, 2);
        let msdk_sig = SubrecordSig::from_str("MSDK").unwrap();
        let entry = npc.fields.iter().find(|e| e.sig == msdk_sig).unwrap();
        let FieldValue::Bytes(raw) = &entry.value else {
            panic!("expected MSDK bytes");
        };
        assert_eq!(raw.len(), 8, "two 4-byte keys survive");
        assert_eq!(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]), 100);
        assert_eq!(u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]), 200);
    }

    #[test]
    fn empty_morph_key_set_strips_all_msdk_keys() {
        let interner = StringInterner::new();
        let mut npc = make_record("NPC_", &interner);
        push(&mut npc, "MSDK", msdk_bytes(&[1, 2, 3]));
        let valid = FxHashSet::default();
        let removed = filter_invalid_morph_keys(&mut npc, &valid);
        assert_eq!(removed, 3);
        let msdk_sig = SubrecordSig::from_str("MSDK").unwrap();
        let entry = npc.fields.iter().find(|e| e.sig == msdk_sig).unwrap();
        let FieldValue::Bytes(raw) = &entry.value else {
            panic!("expected MSDK bytes");
        };
        assert!(raw.is_empty());
    }

    fn msdv_bytes(values: &[f32]) -> FieldValue {
        let mut v: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        for val in values {
            v.extend_from_slice(&val.to_le_bytes());
        }
        FieldValue::Bytes(v)
    }

    fn msdv_value_count(record: &Record) -> usize {
        let msdv_sig = SubrecordSig::from_str("MSDV").unwrap();
        match record
            .fields
            .iter()
            .find(|e| e.sig == msdv_sig)
            .map(|e| &e.value)
        {
            Some(FieldValue::Bytes(raw)) => raw.len() / 4,
            Some(FieldValue::List(items)) => items.len(),
            _ => 0,
        }
    }

    #[test]
    fn filter_morph_keys_drops_paired_msdv_value_keeping_counts_equal() {
        // Dropping an MSDK key without dropping its paired MSDV value desyncs the
        // parallel arrays -> engine crash on load. MSDK key i dropped <=> MSDV
        // value i dropped.
        let interner = StringInterner::new();
        let mut npc = make_record("NPC_", &interner);
        // keys: 100 (valid), 999 (invalid), 200 (valid) — value at index 1 must drop.
        push(&mut npc, "MSDK", msdk_bytes(&[100, 999, 200]));
        push(&mut npc, "MSDV", msdv_bytes(&[1.0, 2.0, 3.0]));

        let mut valid = FxHashSet::default();
        valid.insert(100u32);
        valid.insert(200u32);

        let removed = filter_invalid_morph_keys(&mut npc, &valid);
        assert_eq!(removed, 1);
        let msdk_sig = SubrecordSig::from_str("MSDK").unwrap();
        let FieldValue::Bytes(keys) = &npc.fields.iter().find(|e| e.sig == msdk_sig).unwrap().value
        else {
            panic!("MSDK bytes")
        };
        assert_eq!(keys.len() / 4, 2, "two keys survive");
        assert_eq!(msdv_value_count(&npc), 2, "MSDV count tracks MSDK count");
        // surviving values are the ones at kept positions 0 and 2 (1.0, 3.0).
        let msdv_sig = SubrecordSig::from_str("MSDV").unwrap();
        let FieldValue::Bytes(vals) = &npc.fields.iter().find(|e| e.sig == msdv_sig).unwrap().value
        else {
            panic!("MSDV bytes")
        };
        assert_eq!(
            f32::from_le_bytes([vals[0], vals[1], vals[2], vals[3]]),
            1.0
        );
        assert_eq!(
            f32::from_le_bytes([vals[4], vals[5], vals[6], vals[7]]),
            3.0
        );
    }

    #[test]
    fn filter_morph_keys_all_invalid_empties_both_msdk_and_msdv() {
        // NPC [0778814A] shape: every key FO76-range invalid → MSDK empties →
        // MSDV must ALSO empty (was 5 values, the actual crash trigger).
        let interner = StringInterner::new();
        let mut npc = make_record("NPC_", &interner);
        push(
            &mut npc,
            "MSDK",
            msdk_bytes(&[100000, 100001, 100002, 100003, 100004]),
        );
        push(
            &mut npc,
            "MSDV",
            msdv_bytes(&[-0.16, 0.49, -0.40, 0.95, -0.05]),
        );
        let valid = FxHashSet::default(); // race table empty → all invalid

        let removed = filter_invalid_morph_keys(&mut npc, &valid);
        assert_eq!(removed, 5);
        let msdk_sig = SubrecordSig::from_str("MSDK").unwrap();
        let FieldValue::Bytes(keys) = &npc.fields.iter().find(|e| e.sig == msdk_sig).unwrap().value
        else {
            panic!("MSDK bytes")
        };
        assert!(keys.is_empty(), "MSDK emptied");
        assert_eq!(
            msdv_value_count(&npc),
            0,
            "MSDV emptied in lockstep (was 5) — no crash"
        );
    }

    #[test]
    fn record_has_morph_data_detects_fmri_or_msdk() {
        let interner = StringInterner::new();
        let mut a = make_record("NPC_", &interner);
        push(&mut a, "FMRI", FieldValue::Uint(0));
        assert!(record_has_morph_data(&a));

        let mut b = make_record("NPC_", &interner);
        push(&mut b, "MSDK", msdk_bytes(&[1]));
        assert!(record_has_morph_data(&b));

        let mut c = make_record("NPC_", &interner);
        push(&mut c, "EDID", FieldValue::String(interner.intern("X")));
        assert!(!record_has_morph_data(&c));
    }

    #[test]
    fn resolve_npc_race_returns_rnam_formkey() {
        let interner = StringInterner::new();
        let mut npc = make_record("NPC_", &interner);
        let race_fk = FormKey {
            local: 0x013746,
            plugin: interner.intern("Fallout4.esm"),
        };
        push(&mut npc, "RNAM", FieldValue::FormKey(race_fk));
        assert_eq!(resolve_npc_race(&npc), Some(race_fk));
    }

    #[test]
    fn resolve_npc_race_none_when_missing() {
        let interner = StringInterner::new();
        let npc = make_record("NPC_", &interner);
        assert!(resolve_npc_race(&npc).is_none());
    }
}
