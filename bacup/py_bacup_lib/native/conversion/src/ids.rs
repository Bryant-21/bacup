//! Typed id newtypes used throughout conversion_store.

use crate::sym::{StringInterner, Sym};
use slotmap::new_key_type;

new_key_type! {
    /// Stable, versioned id for records in `RecordStore`. SlotMap-backed:
    /// dropped+reallocated slots return a different `RecordId`.
    pub struct RecordId;
}

/// Index into the GraphStore's nodes vector. Not versioned; never reused
/// because the graph is built once per run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// 4-byte ESP record signature ("WEAP", "ARMO", etc.). Compact, `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SigCode(pub [u8; 4]);

impl SigCode {
    pub fn from_str(s: &str) -> Result<Self, String> {
        let bytes = s.as_bytes();
        if bytes.len() != 4 {
            return Err(format!(
                "SigCode requires 4 ASCII bytes, got {}: {s:?}",
                bytes.len()
            ));
        }
        Ok(SigCode([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("SigCode bytes are always valid UTF-8 by construction")
    }
}

/// 4-byte ESP subrecord signature ("EDID", "FULL", etc.). Parallel to `SigCode`
/// but semantically scoped to subrecord identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubrecordSig(pub [u8; 4]);

impl SubrecordSig {
    pub fn from_str(s: &str) -> Result<Self, String> {
        let bytes = s.as_bytes();
        if bytes.len() != 4 {
            return Err(format!(
                "SubrecordSig requires 4 ASCII bytes, got {}: {s:?}",
                bytes.len()
            ));
        }
        Ok(SubrecordSig([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0)
            .expect("SubrecordSig bytes are always valid UTF-8 by construction")
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        if b.len() < 4 {
            return Err(format!("SubrecordSig requires 4 bytes, got {}", b.len()));
        }
        Ok(SubrecordSig([b[0], b[1], b[2], b[3]]))
    }
}

/// FormKey: 32-bit local id field + interned plugin name. The local id
/// conventionally holds the post-master-strip value (i.e., the lower 24 bits
/// of a FormID) but the field is u32 to allow parsing of full 32-bit hex inputs.
///
/// String form: "<6-hex>@<plugin-name>", e.g. "000810@SeventySix.esm".
/// Plugin name is case-insensitive but stored as the exact intern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormKey {
    pub local: u32,
    pub plugin: Sym,
}

impl FormKey {
    /// Parse `"00000810@Plugin.esm"` form. Hex prefix may be 1-8 chars
    /// (lower 24 bits is conventional after stripping the master byte).
    pub fn parse(s: &str, interner: &StringInterner) -> Result<Self, String> {
        let (hex, plugin) = s
            .split_once('@')
            .ok_or_else(|| format!("FormKey missing '@': {s:?}"))?;
        if hex.is_empty() || hex.len() > 8 {
            return Err(format!("FormKey hex must be 1-8 chars: {s:?}"));
        }
        let local = u32::from_str_radix(hex, 16)
            .map_err(|e| format!("FormKey hex parse error in {s:?}: {e}"))?;
        if plugin.is_empty() {
            return Err(format!("FormKey plugin name is empty: {s:?}"));
        }
        Ok(FormKey {
            local,
            plugin: interner.intern(plugin),
        })
    }

    pub fn format(&self, interner: &StringInterner) -> String {
        format!(
            "{:06X}@{}",
            self.local,
            interner
                .resolve(self.plugin)
                .expect("FormKey plugin Sym must be interned in the same run")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sig_code_round_trips_four_bytes_and_rejects_other_lengths() {
        let sig = SigCode::from_str("WEAP").unwrap();
        assert_eq!(sig.as_str(), "WEAP");
        for input in ["WEA", "WEAPS", ""] {
            assert!(SigCode::from_str(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn formkey_parses_formats_and_compares_canonical_form() {
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("000810@SeventySix.esm", &mut interner).unwrap();
        assert_eq!(fk.local, 0x810);
        assert_eq!(interner.resolve(fk.plugin), Some("SeventySix.esm"));
        assert_eq!(fk.format(&interner), "000810@SeventySix.esm");
        let same = FormKey::parse("000810@SeventySix.esm", &mut interner).unwrap();
        assert_eq!(fk, same);
    }

    #[test]
    fn formkey_rejects_malformed_strings() {
        let mut interner = StringInterner::new();
        for (name, input) in [
            ("missing at sign", "000810SeventySix.esm"),
            ("empty hex", "@SeventySix.esm"),
            ("empty plugin", "000810@"),
        ] {
            assert!(FormKey::parse(input, &mut interner).is_err(), "{name}");
        }
    }

    #[test]
    fn slotmap_record_ids_are_versioned() {
        use slotmap::SlotMap;
        let mut sm: SlotMap<RecordId, &'static str> = SlotMap::with_key();
        let id1 = sm.insert("first");
        sm.remove(id1);
        let id2 = sm.insert("second");
        // After removal+reinsert, the new id must differ from the old.
        assert_ne!(id1, id2);
        // Old id must not resolve.
        assert!(sm.get(id1).is_none());
    }
}
