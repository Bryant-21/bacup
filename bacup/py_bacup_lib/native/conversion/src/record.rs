//! Record value type used throughout the native-first converter.
//!
//! `Record` is the in-memory representation of a decoded ESP record for the
//! conversion pipeline. It is distinct from `ParsedRecord` (which is the raw
//! byte-sliced representation used by the ESP I/O layer) — `Record` holds
//! decoded, typed `FieldValue`s produced by `read_record`.

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::sym::Sym;
use smallvec::SmallVec;

bitflags::bitflags! {
    /// Standard ESP record flags. Subset commonly needed by the converter;
    /// additional bits can be added as phases require them.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct RecordFlags: u32 {
        /// Record is deleted (0x20).
        const DELETED             = 0x0000_0020;
        /// Record is a border region / turns off AI (0x40).
        const BORDER_REGION       = 0x0000_0040;
        /// Record is a turn-off-fire (0x80).
        const TURN_OFF_FIRE       = 0x0000_0080;
        /// Record is cast shadows (0x200).
        const CAST_SHADOWS        = 0x0000_0200;
        /// Record is persistent / initially disabled requires persistent (0x400).
        const PERSISTENT          = 0x0000_0400;
        /// Record is initially disabled (0x800).
        const INITIALLY_DISABLED  = 0x0000_0800;
        /// Record is ignored (0x1000).
        const IGNORED             = 0x0000_1000;
        /// Visible when distant (0x8000).
        const VISIBLE_WHEN_DISTANT = 0x0000_8000;
        /// Random anim start (0x10000).
        const RANDOM_ANIM_START   = 0x0001_0000;
        /// Record is dangerous / off limits (0x20000).
        const DANGEROUS           = 0x0002_0000;
        /// Compressed record data (0x40000).
        const COMPRESSED          = 0x0004_0000;
        /// Can't wait (0x80000).
        const CANT_WAIT           = 0x0008_0000;
        /// Is marker (0x800000).
        const IS_MARKER           = 0x0080_0000;
    }
}

/// A decoded field value produced by the schema-driven source decoder.
///
/// `PartialEq` is derived so that tests and later phases can assert
/// round-trip equality without bespoke comparators.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Subrecord existed but had no decodable content (zero-length, etc.).
    None,
    Bool(bool),
    Int(i64),
    Uint(u64),
    Float(f32),
    String(Sym),
    Bytes(SmallVec<[u8; 32]>),
    FormKey(FormKey),
    List(Vec<FieldValue>),
    Struct(Vec<(Sym, FieldValue)>),
}

/// A single decoded subrecord entry inside a `Record`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldEntry {
    pub sig: SubrecordSig,
    pub value: FieldValue,
}

/// A decoded ESP record — the primary data type flowing through the
/// conversion pipeline.
#[derive(Debug, Clone)]
pub struct Record {
    pub sig: SigCode,
    pub form_key: FormKey,
    /// Editor ID, if an EDID subrecord was present and decoded.
    pub eid: Option<Sym>,
    pub flags: RecordFlags,
    pub fields: SmallVec<[FieldEntry; 8]>,
    /// Non-fatal decode warnings (unknown codecs, truncated data, etc.).
    pub warnings: SmallVec<[Sym; 2]>,
}

impl Record {
    /// Construct an empty record for a given signature and form key.
    /// All other fields are defaulted.
    pub fn new(sig: SigCode, form_key: FormKey) -> Self {
        Record {
            sig,
            form_key,
            eid: None,
            flags: RecordFlags::empty(),
            fields: SmallVec::new(),
            warnings: SmallVec::new(),
        }
    }

    /// Reconcile the `CITC` (condition item count) subrecord with the number of
    /// `CTDA`/`CTDT` conditions actually present.
    ///
    /// Condition-dropping passes remove `CTDA` subrecords with `Vec::retain`
    /// but leave `CITC` at its original value. FO4 then reads the stale
    /// overcount and evaluates a phantom condition off uninitialized memory —
    /// e.g. a region-music `MUST` track whose FO76-only condition was stripped
    /// crashes the audio-manager update (null deref) when the track plays.
    /// No-op on records without a `CITC`. Returns whether any `CITC` changed.
    ///
    /// Each `CITC` counts ONLY the conditions in its own group — the
    /// `CTDA`/`CTDT` rows that immediately follow it (interleaved `CIS1`/`CIS2`
    /// parameter strings are skipped, not counted), up to the next non-condition
    /// subrecord. A record can hold several independent groups: a `SCEN` carries
    /// a per-action `CITC` plus phase conditions that belong to no `CITC`. A
    /// record-wide `CTDA` total would wrongly inflate every `CITC` — e.g. a SCEN
    /// action with zero conditions inherits the phase `CTDA` count, and FO4 then
    /// reads phantom conditions off the following action and crashes.
    pub(crate) fn sync_condition_count(&mut self) -> bool {
        let len = self.fields.len();
        let mut changed = false;
        for i in 0..len {
            if self.fields[i].sig.0 != *b"CITC" {
                continue;
            }
            let mut count = 0u32;
            let mut j = i + 1;
            while j < len {
                match &self.fields[j].sig.0 {
                    b"CTDA" | b"CTDT" => count += 1,
                    b"CIS1" | b"CIS2" => {}
                    _ => break,
                }
                j += 1;
            }
            changed |= write_u32_field(&mut self.fields[i].value, count);
        }
        changed
    }
}

/// Overwrite a u32-typed field value with `value`, returning whether it changed.
fn write_u32_field(field: &mut FieldValue, value: u32) -> bool {
    match field {
        FieldValue::Uint(n) => {
            let changed = *n != u64::from(value);
            *n = u64::from(value);
            changed
        }
        FieldValue::Int(n) => {
            let changed = *n != i64::from(value);
            *n = i64::from(value);
            changed
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let prev = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            bytes[..4].copy_from_slice(&value.to_le_bytes());
            prev != value
        }
        FieldValue::Bytes(bytes) => {
            bytes.clear();
            bytes.extend_from_slice(&value.to_le_bytes());
            true
        }
        FieldValue::Struct(fields) => fields
            .first_mut()
            .map(|(_, first)| write_u32_field(first, value))
            .unwrap_or(false),
        other => {
            *other = FieldValue::Uint(u64::from(value));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::sym::StringInterner;

    fn null_form_key(interner: &StringInterner) -> FormKey {
        FormKey::parse("000000@test.esm", interner).unwrap()
    }

    #[test]
    fn record_default_is_empty() {
        let mut interner = StringInterner::new();
        let fk = null_form_key(&mut interner);
        let r = Record::new(SigCode::from_str("WEAP").unwrap(), fk);
        assert_eq!(r.sig.as_str(), "WEAP");
        assert_eq!(r.fields.len(), 0);
        assert!(r.eid.is_none());
    }

    #[test]
    fn field_value_round_trip_int() {
        let v = FieldValue::Int(42);
        if let FieldValue::Int(n) = v {
            assert_eq!(n, 42);
        } else {
            panic!("expected Int variant");
        }
    }

    fn sub(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    #[test]
    fn sync_condition_count_follows_dropped_ctda() {
        let mut interner = StringInterner::new();
        let fk = null_form_key(&mut interner);
        let mut r = Record::new(SigCode::from_str("MUST").unwrap(), fk);
        // CITC says 2 but only one CTDA survives — the post-drop state that
        // crashed the FO4 audio update on a converted region-music track.
        r.fields.push(sub("CITC", FieldValue::Uint(2)));
        r.fields.push(sub(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0u8; 32])),
        ));

        assert!(r.sync_condition_count(), "CITC 2 -> 1 is a change");
        let citc = r.fields.iter().find(|e| e.sig.0 == *b"CITC").unwrap();
        assert_eq!(citc.value, FieldValue::Uint(1));
        assert!(!r.sync_condition_count(), "second pass is idempotent");
    }

    #[test]
    fn sync_condition_count_handles_raw_bytes_citc() {
        let mut interner = StringInterner::new();
        let fk = null_form_key(&mut interner);
        let mut r = Record::new(SigCode::from_str("PACK").unwrap(), fk);
        r.fields.push(sub(
            "CITC",
            FieldValue::Bytes(SmallVec::from_vec(3u32.to_le_bytes().to_vec())),
        ));
        // No CTDA at all -> count must drop to 0.
        assert!(r.sync_condition_count());
        let citc = r.fields.iter().find(|e| e.sig.0 == *b"CITC").unwrap();
        assert_eq!(
            citc.value,
            FieldValue::Bytes(SmallVec::from_vec(0u32.to_le_bytes().to_vec()))
        );
    }

    #[test]
    fn sync_condition_count_noop_without_citc() {
        let mut interner = StringInterner::new();
        let fk = null_form_key(&mut interner);
        let mut r = Record::new(SigCode::from_str("INFO").unwrap(), fk);
        r.fields.push(sub(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0u8; 32])),
        ));
        assert!(!r.sync_condition_count(), "no CITC -> nothing to sync");
    }

    #[test]
    fn sync_condition_count_scen_per_group_ignores_phase_conditions() {
        let mut interner = StringInterner::new();
        let fk = null_form_key(&mut interner);
        let mut r = Record::new(SigCode::from_str("SCEN").unwrap(), fk);
        // Two phase conditions (belong to no CITC) precede a per-action CITC
        // whose own action has zero conditions. The old record-wide count
        // stamped 2 here -> FO4 read two phantom conditions off the next action
        // and crashed (test_VHarbison_Dialogue_Someone scene CTD).
        r.fields.push(sub(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0u8; 32])),
        ));
        r.fields.push(sub(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0u8; 32])),
        ));
        r.fields.push(sub("ANAM", FieldValue::Uint(0)));
        r.fields.push(sub("CITC", FieldValue::Uint(2)));
        r.fields.push(sub("ANAM", FieldValue::Uint(0)));

        assert!(r.sync_condition_count(), "stale action CITC 2 -> 0");
        let citc = r.fields.iter().find(|e| e.sig.0 == *b"CITC").unwrap();
        assert_eq!(citc.value, FieldValue::Uint(0));
    }

    #[test]
    fn sync_condition_count_multiple_groups_each_local() {
        let mut interner = StringInterner::new();
        let fk = null_form_key(&mut interner);
        let mut r = Record::new(SigCode::from_str("SCEN").unwrap(), fk);
        // action 1: CITC + 1 CTDA (with a CIS1 param string) ; action 2: CITC + 2 CTDA.
        r.fields.push(sub("CITC", FieldValue::Uint(0)));
        r.fields.push(sub(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0u8; 32])),
        ));
        r.fields.push(sub(
            "CIS1",
            FieldValue::Bytes(SmallVec::from_vec(vec![0u8; 4])),
        ));
        r.fields.push(sub("ANAM", FieldValue::Uint(0)));
        r.fields.push(sub("CITC", FieldValue::Uint(0)));
        r.fields.push(sub(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0u8; 32])),
        ));
        r.fields.push(sub(
            "CTDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0u8; 32])),
        ));

        assert!(r.sync_condition_count());
        let counts: Vec<_> = r
            .fields
            .iter()
            .filter(|e| e.sig.0 == *b"CITC")
            .map(|e| e.value.clone())
            .collect();
        assert_eq!(counts, vec![FieldValue::Uint(1), FieldValue::Uint(2)]);
    }

    #[test]
    fn field_value_partial_eq_works() {
        assert_eq!(FieldValue::Int(1), FieldValue::Int(1));
        assert_ne!(FieldValue::Int(1), FieldValue::Int(2));
        assert_ne!(FieldValue::Int(1), FieldValue::Bool(true));
        assert_eq!(FieldValue::None, FieldValue::None);
    }

    #[test]
    fn record_flags_bitflags_compose() {
        let flags = RecordFlags::DELETED | RecordFlags::PERSISTENT;
        assert!(flags.contains(RecordFlags::DELETED));
        assert!(flags.contains(RecordFlags::PERSISTENT));
        assert!(!flags.contains(RecordFlags::IGNORED));
        assert_eq!(flags.bits(), 0x0000_0020 | 0x0000_0400);
    }

    #[test]
    fn field_entry_sig_round_trip() {
        let sig = SubrecordSig::from_str("EDID").unwrap();
        let entry = FieldEntry {
            sig,
            value: FieldValue::None,
        };
        assert_eq!(entry.sig.as_str(), "EDID");
    }
}
