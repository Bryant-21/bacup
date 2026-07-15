//! Fixup: inject the minimum required child block into records
//! that the FO76→FO4 port left structurally incomplete, so xEdit's FO4 grammar
//! stops reporting a phantom "Found a NULL reference" on a missing block.
//!
//! Two cases:
//!
//! 1. **ALCH / ENCH / SPEL with zero Effects**. FO76 carries effect-less magic
//!    items; FO4's grammar wants ≥1 Effect, so xEdit synthesizes one and flags
//!    its missing `EFID`. ENCH/SPEL retain their existing `DamageHealth`
//!    placeholder. ALCH repair is restricted to three verified effect-less
//!    SeventySix.esm records and replaces any incomplete effect children with
//!    `RestoreHealthGeneric [MGEF:00023735]` plus an all-zero `EFIT`
//!    (`struct:f,I,I` = magnitude 0.0, area 0, duration 0). The Effect block is
//!    the LAST block in the FO4 grammar, so a tail append is grammar-correct.
//!
//! 2. **NPC_ with no CNAM (Class)**. FO76 allows an NPC without an
//!    explicit Class; FO4 requires one. We inject `CNAM` → `Citizen [CLAS:0001326B]`
//!    (a stable Fallout4.esm class). UNLIKE the Effect block, `CNAM` is NOT a tail
//!    field in NPC_ — it sits at a fixed position in the FO4 NPC_ grammar (between
//!    the `OBTS`/`STOP` template block and `FULL`/`DATA`). Inserting it at the wrong
//!    offset would produce a NEW "out of order subrecord" error, so the insert
//!    position is derived from the schema's ordered subrecord list, NOT hardcoded.
//!
//! # Plugin-aware
//! The base FormIDs name `Fallout4.esm` explicitly; the encoder resolves that to
//! the output's actual master index at write time (it is index 0 here, but never
//! assumed). If `Fallout4.esm` is not among the target masters, injection is
//! skipped rather than emitting an unresolvable reference.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

/// The existing stub base effect appended to effect-less ENCH/SPEL records.
const STUB_BASE_EFFECT_LOCAL: u32 = 0x0011_D53C; // DamageHealth
/// Non-hostile placeholder used only for the three verified effect-less ALCH records.
const ALCH_STUB_BASE_EFFECT_LOCAL: u32 = 0x0002_3735; // RestoreHealthGeneric
const VERIFIED_EFFECTLESS_ALCH_LOCALS: [u32; 3] = [0x007B_11A5, 0x0063_F048, 0x0063_F047];
const SOURCE_MASTER: &str = "SeventySix.esm";
const ALCH_EFFECT_CHILD_SIGS: [&str; 5] = ["EFID", "EFIT", "CTDA", "CIS1", "CIS2"];
/// The fallback class injected into NPC_ records with no CNAM.
const FALLBACK_CLASS_LOCAL: u32 = 0x0001_326B; // Citizen
/// Both base records live in Fallout4.esm.
const BASE_MASTER: &str = "Fallout4.esm";

pub struct InjectRequiredChildBlocksFixup;

impl Fixup for InjectRequiredChildBlocksFixup {
    fn name(&self) -> &'static str {
        "inject_required_child_blocks"
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
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        // Plugin-aware guard: the base records must exist as a target master.
        let base_master_present = session
            .target_masters()
            .iter()
            .any(|m| m.eq_ignore_ascii_case(BASE_MASTER));
        if !base_master_present {
            return Ok(report);
        }
        let base_sym = mapper.interner.intern(BASE_MASTER);

        let available: FxHashSet<SigCode> = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .into_iter()
            .collect();

        let mut changed_records = Vec::new();

        let alch_sig = SigCode::from_str("ALCH").expect("ALCH sig");
        if available.contains(&alch_sig) {
            let fks = session
                .form_keys_of_sig(alch_sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                if !is_verified_effectless_alch(&fk, mapper.interner) {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if repair_verified_effectless_alch(&mut record, base_sym, mapper.interner) {
                    changed_records.push(record);
                }
            }
        }

        // ── 1. Effect-less ENCH / SPEL → append inert stub Effect ────────────
        for sig_str in ["ENCH", "SPEL"] {
            let Ok(sig) = SigCode::from_str(sig_str) else {
                continue;
            };
            if !available.contains(&sig) {
                continue;
            }
            let fks = session
                .form_keys_of_sig(sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                // Cheap pre-filter: only records that LACK an EFID need a stub.
                if session
                    .record_has_any_subrecord(&fk, &["EFID"])
                    .unwrap_or(true)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if append_stub_effect(&mut record, base_sym) {
                    changed_records.push(record);
                }
            }
        }

        // ── 2. NPC_ with no CNAM → inject a fallback Class at its schema slot ─
        let npc_sig = SigCode::from_str("NPC_").expect("NPC_ sig");
        if available.contains(&npc_sig) {
            let cnam_order = cnam_schema_index(session);
            let fks = session
                .form_keys_of_sig(npc_sig, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            for fk in fks {
                if session
                    .record_has_any_subrecord(&fk, &["CNAM"])
                    .unwrap_or(true)
                {
                    continue;
                }
                let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if inject_npc_cnam(&mut record, base_sym, cnam_order) {
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
                "inject_required_child_blocks replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

/// Append `EFID` + zero `EFIT` to an effect-less ENCH/SPEL. Returns `false`
/// (no-op) if the record already carries an `EFID` — the caller pre-filters, but
/// this keeps the helper self-guarding for the unit tests.
fn append_stub_effect(record: &mut Record, base_sym: crate::sym::Sym) -> bool {
    if record.fields.iter().any(|e| e.sig.as_str() == "EFID") {
        return false;
    }
    let (efid, efit) = ench_spell_stub_effect_fields(base_sym);
    record.fields.push(efid);
    record.fields.push(efit);
    true
}

fn repair_verified_effectless_alch(
    record: &mut Record,
    base_sym: crate::sym::Sym,
    interner: &StringInterner,
) -> bool {
    if record.sig.as_str() != "ALCH" || !is_verified_effectless_alch(&record.form_key, interner) {
        return false;
    }

    if record
        .fields
        .iter()
        .any(|entry| entry.sig.as_str() == "EFID" && !is_null_effect_reference(&entry.value))
    {
        return false;
    }

    record
        .fields
        .retain(|entry| !ALCH_EFFECT_CHILD_SIGS.contains(&entry.sig.as_str()));
    let (efid, efit) = alch_stub_effect_fields(base_sym);
    record.fields.push(efid);
    record.fields.push(efit);
    true
}

fn is_verified_effectless_alch(form_key: &FormKey, interner: &StringInterner) -> bool {
    VERIFIED_EFFECTLESS_ALCH_LOCALS.contains(&form_key.local)
        && interner
            .resolve(form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(SOURCE_MASTER))
}

fn is_null_effect_reference(value: &FieldValue) -> bool {
    match value {
        FieldValue::None => true,
        FieldValue::FormKey(form_key) => form_key.local == 0,
        FieldValue::Bytes(bytes) => bytes.len() == 4 && bytes.iter().all(|byte| *byte == 0),
        _ => false,
    }
}

fn ench_spell_stub_effect_fields(base_sym: crate::sym::Sym) -> (FieldEntry, FieldEntry) {
    effect_fields(base_sym, STUB_BASE_EFFECT_LOCAL)
}

fn alch_stub_effect_fields(base_sym: crate::sym::Sym) -> (FieldEntry, FieldEntry) {
    effect_fields(base_sym, ALCH_STUB_BASE_EFFECT_LOCAL)
}

fn effect_fields(base_sym: crate::sym::Sym, effect_local: u32) -> (FieldEntry, FieldEntry) {
    let efid = FieldEntry {
        sig: SubrecordSig::from_str("EFID").expect("EFID sig"),
        value: FieldValue::FormKey(FormKey {
            plugin: base_sym,
            local: effect_local,
        }),
    };
    // EFIT is struct:f,I,I (magnitude f32, area u32, duration u32). All-zero =
    // inert. Emitted as raw Bytes (the encoder writes a Bytes value verbatim).
    let efit = FieldEntry {
        sig: SubrecordSig::from_str("EFIT").expect("EFIT sig"),
        value: FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 12])),
    };
    (efid, efit)
}

/// The index of `CNAM` in the FO4 NPC_ schema's ordered subrecord list, used to
/// place an injected CNAM at its grammar-correct position. `None` if the schema
/// has no NPC_/CNAM entry (then injection falls back to a safe no-op).
fn cnam_schema_index(session: &mut PluginSession) -> Option<usize> {
    let schema = session.schema().ok()?;
    let npc = schema.record_def("NPC_")?;
    npc.subrecords.iter().position(|s| s.id == "CNAM")
}

/// Insert `CNAM` → fallback Class at its schema-ordered position: before the
/// first existing field whose subrecord sits AFTER CNAM in the schema order.
/// Returns `false` if the record already has a CNAM or the schema order is
/// unavailable (defensive — never inject at an unverified offset).
fn inject_npc_cnam(
    record: &mut Record,
    base_sym: crate::sym::Sym,
    cnam_order: Option<usize>,
) -> bool {
    if record.fields.iter().any(|e| e.sig.as_str() == "CNAM") {
        return false;
    }
    let Some(cnam_idx) = cnam_order else {
        return false;
    };
    let cnam = FieldEntry {
        sig: SubrecordSig::from_str("CNAM").expect("CNAM sig"),
        value: FieldValue::FormKey(FormKey {
            plugin: base_sym,
            local: FALLBACK_CLASS_LOCAL,
        }),
    };
    // Insert CNAM before the first existing field that comes AFTER it in schema
    // order. Each existing field is ranked by its LAST schema-order occurrence
    // (`rposition`), which disambiguates sigs the FO4 NPC_ grammar lists twice —
    // notably FULL (a Template-Name slot at idx 54, before CNAM, AND the NPC Name
    // slot at idx 58, after CNAM). A real NPC's lone post-template FULL is the
    // Name, so ranking by its last occurrence (58 > CNAM) correctly puts CNAM
    // before it; ranking by the first (54 < CNAM) would wrongly skip it.
    let insert_at = record
        .fields
        .iter()
        .position(|e| npc_subrecord_order(e.sig.as_str()).is_some_and(|i| i > cnam_idx))
        .unwrap_or(record.fields.len());
    record.fields.insert(insert_at, cnam);
    true
}

/// Rank a subrecord sig within the FO4 NPC_ schema order by its LAST occurrence
/// (the FO4 NPC_ grammar lists FULL/OBTF/OBTS etc. more than once; the later slot
/// is the one a fully-built NPC's trailing fields belong to). The CNAM index
/// itself is read live from the schema (`cnam_schema_index`) so a schema change
/// is picked up; only the relative order of existing fields vs CNAM matters here.
fn npc_subrecord_order(sig: &str) -> Option<usize> {
    NPC_SUBRECORD_ORDER.iter().rposition(|s| *s == sig)
}

/// The FO4 NPC_ subrecord order (schema `records["NPC_"].subrecords`). Kept in
/// sync with the generated schema; the CNAM insert ranks existing fields against
/// the live CNAM index, so only the RELATIVE order around CNAM must be correct.
const NPC_SUBRECORD_ORDER: &[&str] = &[
    "EDID", "VMAD", "OBND", "PTRN", "STCP", "ACBS", "SNAM", "INAM", "VTCK", "TPLT", "LTPT", "LTPC",
    "TPTA", "RNAM", "SPCT", "SPLO", "DEST", "DAMC", "DSTD", "DSTA", "DMDL", "DMDT", "DMDC", "DMDS",
    "DSTF", "WNAM", "ANAM", "ATKR", "ATKD", "ATKE", "ATKW", "ATKS", "ATKT", "SPOR", "OCOR", "GWOR",
    "ECOR", "FCPL", "RCLR", "PRKZ", "PRKR", "PRPS", "FTYP", "NTRM", "COCT", "CNTO", "COED", "AIDT",
    "PKID", "KSIZ", "KWDA", "APPR", "OBTE", "OBTF", "FULL", "OBTS", "STOP", "CNAM", "FULL", "SHRT",
    "DATA", "DNAM", "PNAM", "HCLF", "BCLF", "ZNAM", "GNAM", "NAM5", "NAM6", "NAM7", "NAM4", "MWGT",
    "NAM8", "CS2H", "CS2K", "CS2D", "CS2E", "CS2F", "CSCR", "PFRN", "DOFT", "SOFT", "DPLT", "CRIF",
    "FTST", "QNAM", "MSDK", "MSDV", "TETI", "TEND", "MRSV", "FMRI", "FMRS", "FMIN", "ATTX",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SigCode;
    use crate::record::{Record, RecordFlags};
    use crate::sym::StringInterner;

    fn record(sig: &str, fields: Vec<(&str, FieldValue)>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                plugin: interner.intern("SeventySix.esm"),
                local: 0x000800,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields
                .into_iter()
                .map(|(s, v)| FieldEntry {
                    sig: SubrecordSig::from_str(s).unwrap(),
                    value: v,
                })
                .collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn order(record: &Record) -> Vec<String> {
        record
            .fields
            .iter()
            .map(|e| e.sig.as_str().to_string())
            .collect()
    }

    #[test]
    fn appends_stub_effect_to_effectless_ench() {
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        let mut rec = record(
            "ENCH",
            vec![
                (
                    "EDID",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(b"x\0")),
                ),
                (
                    "OBND",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 12])),
                ),
                (
                    "FULL",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1, 0, 0, 0])),
                ),
                (
                    "ENIT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 36])),
                ),
            ],
            &interner,
        );
        assert!(append_stub_effect(&mut rec, base));
        // EFID + EFIT appended at the tail, in that order, after ENIT.
        assert_eq!(
            order(&rec),
            ["EDID", "OBND", "FULL", "ENIT", "EFID", "EFIT"]
        );
        let efid = &rec.fields[4];
        let FieldValue::FormKey(fk) = &efid.value else {
            panic!("EFID must be a FormKey")
        };
        assert_eq!(fk.local, STUB_BASE_EFFECT_LOCAL);
        assert_eq!(fk.plugin, base);
        let FieldValue::Bytes(efit) = &rec.fields[5].value else {
            panic!("EFIT bytes")
        };
        assert_eq!(
            efit.as_slice(),
            &[0u8; 12],
            "inert zero EFIT (mag/area/dur = 0)"
        );
    }

    #[test]
    fn leaves_ench_that_already_has_an_effect() {
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        let mut rec = record(
            "ENCH",
            vec![
                (
                    "EDID",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(b"x\0")),
                ),
                (
                    "ENIT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 36])),
                ),
                (
                    "EFID",
                    FieldValue::FormKey(FormKey {
                        plugin: base,
                        local: 0x00ABCDEF,
                    }),
                ),
                (
                    "EFIT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 12])),
                ),
            ],
            &interner,
        );
        assert!(
            !append_stub_effect(&mut rec, base),
            "must not touch an ENCH with an effect"
        );
        assert_eq!(order(&rec), ["EDID", "ENIT", "EFID", "EFIT"]);
    }

    #[test]
    fn repairs_only_verified_effectless_alch_form_keys() {
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        for local in VERIFIED_EFFECTLESS_ALCH_LOCALS {
            let mut rec = record("ALCH", vec![], &interner);
            rec.form_key.local = local;
            assert!(repair_verified_effectless_alch(&mut rec, base, &interner));
        }

        let mut unrelated = record("ALCH", vec![], &interner);
        let unrelated_before = unrelated.fields.clone();
        assert!(!repair_verified_effectless_alch(
            &mut unrelated,
            base,
            &interner
        ));
        assert_eq!(unrelated.fields, unrelated_before);

        let mut wrong_plugin = record("ALCH", vec![], &interner);
        wrong_plugin.form_key.local = VERIFIED_EFFECTLESS_ALCH_LOCALS[0];
        wrong_plugin.form_key.plugin = interner.intern("Other.esm");
        assert!(!repair_verified_effectless_alch(
            &mut wrong_plugin,
            base,
            &interner
        ));
    }

    #[test]
    fn clears_invalid_alch_effect_children_before_inserting_one_pair() {
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        let mut rec = record(
            "ALCH",
            vec![
                (
                    "EDID",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(b"Consumable\0")),
                ),
                (
                    "ENIT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 20])),
                ),
                ("EFID", FieldValue::None),
                (
                    "EFIT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0xFFu8; 12])),
                ),
                (
                    "CTDA",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0xAAu8; 32])),
                ),
                (
                    "CIS1",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(b"first\0")),
                ),
                (
                    "CIS2",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(b"second\0")),
                ),
                (
                    "EFID",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 4])),
                ),
            ],
            &interner,
        );
        rec.form_key.local = VERIFIED_EFFECTLESS_ALCH_LOCALS[0];

        assert!(repair_verified_effectless_alch(&mut rec, base, &interner));
        assert_eq!(order(&rec), ["EDID", "ENIT", "EFID", "EFIT"]);
        let FieldValue::FormKey(efid) = rec.fields[2].value else {
            panic!("EFID must be a FormKey")
        };
        assert_eq!(efid.local, ALCH_STUB_BASE_EFFECT_LOCAL);
        assert_eq!(efid.plugin, base);
        let FieldValue::Bytes(efit) = &rec.fields[3].value else {
            panic!("EFIT must be bytes")
        };
        assert_eq!(efit.as_slice(), &[0u8; 12]);
    }

    #[test]
    fn leaves_alch_with_valid_multiple_effects_unchanged() {
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        let mut rec = record(
            "ALCH",
            vec![
                (
                    "EFID",
                    FieldValue::FormKey(FormKey {
                        plugin: base,
                        local: 0x0012_3456,
                    }),
                ),
                (
                    "EFIT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1u8; 12])),
                ),
                (
                    "CTDA",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[2u8; 32])),
                ),
                (
                    "EFID",
                    FieldValue::FormKey(FormKey {
                        plugin: base,
                        local: 0x0065_4321,
                    }),
                ),
                (
                    "EFIT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[3u8; 12])),
                ),
            ],
            &interner,
        );
        rec.form_key.local = VERIFIED_EFFECTLESS_ALCH_LOCALS[1];
        let before = rec.fields.clone();

        assert!(!repair_verified_effectless_alch(&mut rec, base, &interner));
        assert_eq!(rec.fields, before);
    }

    #[test]
    fn alch_repair_is_idempotent() {
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        let mut rec = record(
            "ALCH",
            vec![(
                "EFIT",
                FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0xFFu8; 12])),
            )],
            &interner,
        );
        rec.form_key.local = VERIFIED_EFFECTLESS_ALCH_LOCALS[2];

        assert!(repair_verified_effectless_alch(&mut rec, base, &interner));
        let once = rec.fields.clone();
        assert!(!repair_verified_effectless_alch(&mut rec, base, &interner));
        assert_eq!(rec.fields, once);
        assert_eq!(order(&rec), ["EFID", "EFIT"]);
    }

    #[test]
    fn injects_npc_cnam_at_schema_position_before_full() {
        // A realistic NPC has shape EDID,OBND,ACBS,(VTCK,)RNAM,PRPS,AIDT,FULL,
        // DATA,DNAM,... — CNAM must land between AIDT and FULL (schema order:
        // ...AIDT...OBTS,STOP,CNAM,SHRT,DATA → CNAM precedes FULL/DATA).
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        let cnam_idx = npc_subrecord_order("CNAM");
        let mut rec = record(
            "NPC_",
            vec![
                (
                    "EDID",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(b"x\0")),
                ),
                (
                    "OBND",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 12])),
                ),
                (
                    "ACBS",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 20])),
                ),
                (
                    "RNAM",
                    FieldValue::FormKey(FormKey {
                        plugin: base,
                        local: 0x013746,
                    }),
                ),
                ("PRPS", FieldValue::Bytes(smallvec::SmallVec::new())),
                (
                    "AIDT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 24])),
                ),
                (
                    "FULL",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[1, 0, 0, 0])),
                ),
                ("DATA", FieldValue::Bytes(smallvec::SmallVec::new())),
                (
                    "DNAM",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 8])),
                ),
            ],
            &interner,
        );
        assert!(inject_npc_cnam(&mut rec, base, cnam_idx));
        let got = order(&rec);
        // CNAM must come immediately before FULL (the first field after it in
        // schema order), NOT at the tail and NOT before AIDT.
        let cnam_pos = got.iter().position(|s| s == "CNAM").unwrap();
        let full_pos = got.iter().position(|s| s == "FULL").unwrap();
        let aidt_pos = got.iter().position(|s| s == "AIDT").unwrap();
        assert!(aidt_pos < cnam_pos, "CNAM must come after AIDT");
        assert_eq!(
            cnam_pos + 1,
            full_pos,
            "CNAM must sit immediately before FULL"
        );
        let FieldValue::FormKey(fk) = &rec.fields[cnam_pos].value else {
            panic!()
        };
        assert_eq!(fk.local, FALLBACK_CLASS_LOCAL);
        assert_eq!(fk.plugin, base);
    }

    #[test]
    fn leaves_npc_that_already_has_cnam() {
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        let cnam_idx = npc_subrecord_order("CNAM");
        let mut rec = record(
            "NPC_",
            vec![
                (
                    "EDID",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(b"x\0")),
                ),
                (
                    "CNAM",
                    FieldValue::FormKey(FormKey {
                        plugin: base,
                        local: 0x00AAAA,
                    }),
                ),
                ("DATA", FieldValue::Bytes(smallvec::SmallVec::new())),
            ],
            &interner,
        );
        assert!(!inject_npc_cnam(&mut rec, base, cnam_idx));
        assert_eq!(order(&rec), ["EDID", "CNAM", "DATA"]);
    }

    #[test]
    fn npc_cnam_lands_before_data_when_no_full() {
        // An NPC whose only post-CNAM field present is DATA → CNAM before DATA.
        let interner = StringInterner::new();
        let base = interner.intern(BASE_MASTER);
        let cnam_idx = npc_subrecord_order("CNAM");
        let mut rec = record(
            "NPC_",
            vec![
                (
                    "EDID",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(b"x\0")),
                ),
                (
                    "ACBS",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 20])),
                ),
                (
                    "AIDT",
                    FieldValue::Bytes(smallvec::SmallVec::from_slice(&[0u8; 24])),
                ),
                ("DATA", FieldValue::Bytes(smallvec::SmallVec::new())),
            ],
            &interner,
        );
        assert!(inject_npc_cnam(&mut rec, base, cnam_idx));
        let got = order(&rec);
        let cnam_pos = got.iter().position(|s| s == "CNAM").unwrap();
        let data_pos = got.iter().position(|s| s == "DATA").unwrap();
        assert_eq!(cnam_pos + 1, data_pos, "CNAM immediately before DATA");
    }
}
