//! Ammo substitute table — FNV ammo EditorID → FO4 FormKey mapping.
//!
//!
//! Loads `record/translation_maps/ammo_fnv_to_fo4.yaml` and exposes a
//! `lookup` function that returns the FO4 FormKey string for a given FNV
//! ammo EditorID.
//!
//! # YAML format
//!
//! ```yaml
//! version: 1
//! ammo:
//!   Ammo10mm:
//!     master: Fallout4.esm
//!     form_id: "0001F276"
//!   Ammo556mm:
//!     master: Fallout4.esm
//!     form_id: "0001F278"
//!   # ...
//! ```
//!
//! The `form_id` is a hex string (with leading zeros). The canonical FO4
//! FormKey is formatted as `<form_id>@<master>`, e.g. `0001F276@Fallout4.esm`.
//!
//! # Usage
//!
//! ```no_run
//! use esp_authoring_core::conversion::translator::ammo_substitute::AmmoSubstituteTable;
//!
//! let table = AmmoSubstituteTable::from_yaml(yaml_str).unwrap();
//! if let Some(fk) = table.lookup("Ammo10mm") {
//!     println!("FO4 ammo FormKey: {fk}");
//! }
//! ```

use rustc_hash::FxHashMap;

/// A parsed entry from the ammo substitute YAML.
#[derive(Debug, Clone, PartialEq)]
pub struct AmmoEntry {
    /// The FO4 plugin that owns this ammo record, e.g. `Fallout4.esm`.
    pub master: String,
    /// The hex form_id string as found in the YAML, e.g. `"0001F276"`.
    pub form_id: String,
}

impl AmmoEntry {
    /// Format this entry as a canonical FormKey string: `<XXXXXX>@<master>`.
    ///
    /// The hex `form_id` is parsed as a base-16 integer and formatted with
    /// six uppercase hex digits, matching the `{:06X}` Python format.
    pub fn as_form_key(&self) -> Option<String> {
        let n = u32::from_str_radix(self.form_id.trim(), 16).ok()?;
        Some(format!("{:06X}@{}", n, self.master))
    }
}

/// In-memory ammo substitute table keyed by FNV ammo EditorID.
#[derive(Debug, Default)]
pub struct AmmoSubstituteTable {
    entries: FxHashMap<String, AmmoEntry>,
}

impl AmmoSubstituteTable {
    /// Parse the YAML text of `ammo_fnv_to_fo4.yaml` into a table.
    ///
    /// Parses via `serde_saphyr` (the project's YAML parser) into a
    /// `serde_json::Value` map. Returns an empty table (not an error) when
    /// the YAML is empty, missing the `ammo` key, or the key maps to a
    /// non-mapping value.
    pub fn from_yaml(yaml_text: &str) -> Result<Self, String> {
        if yaml_text.trim().is_empty() {
            return Ok(Self::default());
        }
        let doc: serde_json::Value =
            serde_saphyr::from_str(yaml_text).map_err(|e| format!("ammo YAML parse error: {e}"))?;

        let ammo_map = match doc.get("ammo") {
            Some(serde_json::Value::Object(m)) => m.clone(),
            _ => return Ok(Self::default()),
        };

        let mut entries = FxHashMap::default();
        for (eid, val) in &ammo_map {
            let master = val
                .get("master")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            let form_id = val
                .get("form_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned();
            if master.is_empty() || form_id.is_empty() {
                continue;
            }
            entries.insert(eid.clone(), AmmoEntry { master, form_id });
        }

        Ok(Self { entries })
    }

    /// Look up the FO4 FormKey string for a FNV ammo EditorID.
    ///
    /// Returns `None` when the EditorID is not in the table or when
    /// `form_id` cannot be parsed as a hex integer.
    pub fn lookup(&self, eid: &str) -> Option<String> {
        self.entries.get(eid)?.as_form_key()
    }

    /// Return the raw entry for an EditorID, or `None` if not found.
    pub fn entry(&self, eid: &str) -> Option<&AmmoEntry> {
        self.entries.get(eid)
    }

    /// Number of entries in the table.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_YAML: &str = r#"
version: 1
ammo:
  Ammo10mm:
    master: Fallout4.esm
    form_id: "0001F276"
  Ammo556mm:
    master: Fallout4.esm
    form_id: "0001F278"
  AmmoShotgunShell:
    master: Fallout4.esm
    form_id: "0001F673"
  Ammo762mm:
    master: DLCNukaWorld.esm
    form_id: "00037897"
"#;

    // -------------------------------------------------------------------------
    // Parsing
    // -------------------------------------------------------------------------

    #[test]
    fn parses_sample_yaml_into_correct_entry_count() {
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        assert_eq!(table.len(), 4);
    }

    #[test]
    fn parses_ammo10mm_entry() {
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        let entry = table.entry("Ammo10mm").expect("Ammo10mm should be present");
        assert_eq!(entry.master, "Fallout4.esm");
        assert_eq!(entry.form_id, "0001F276");
    }

    #[test]
    fn parses_dlc_entry_with_different_master() {
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        let entry = table
            .entry("Ammo762mm")
            .expect("Ammo762mm should be present");
        assert_eq!(entry.master, "DLCNukaWorld.esm");
        assert_eq!(entry.form_id, "00037897");
    }

    #[test]
    fn empty_yaml_produces_empty_table() {
        let table = AmmoSubstituteTable::from_yaml("").unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn yaml_without_ammo_key_produces_empty_table() {
        let yaml = "version: 1\n";
        let table = AmmoSubstituteTable::from_yaml(yaml).unwrap();
        assert!(table.is_empty());
    }

    // -------------------------------------------------------------------------
    // Lookup / FormKey formatting
    // -------------------------------------------------------------------------

    #[test]
    fn lookup_returns_correct_formkey_for_ammo10mm() {
        // 0x0001F276 = 127606, formatted {:06X} = "01F276" (mirrors Python's f"{int(...):06X}")
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        let fk = table.lookup("Ammo10mm").expect("Ammo10mm should look up");
        assert_eq!(fk, "01F276@Fallout4.esm");
    }

    #[test]
    fn lookup_returns_correct_formkey_for_ammo556mm() {
        // 0x0001F278 = 127608, formatted {:06X} = "01F278"
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        let fk = table.lookup("Ammo556mm").unwrap();
        assert_eq!(fk, "01F278@Fallout4.esm");
    }

    #[test]
    fn lookup_returns_correct_formkey_for_dlc_ammo() {
        // 0x00037897 = 227479, formatted {:06X} = "037897"
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        let fk = table.lookup("Ammo762mm").unwrap();
        assert_eq!(fk, "037897@DLCNukaWorld.esm");
    }

    #[test]
    fn lookup_returns_none_for_unknown_eid() {
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        assert!(table.lookup("AmmoUnknown").is_none());
    }

    #[test]
    fn form_id_hex_is_formatted_with_six_digits_uppercase() {
        let entry = AmmoEntry {
            master: "Fallout4.esm".into(),
            form_id: "1F276".into(), // five digits — should be padded to six
        };
        let fk = entry.as_form_key().unwrap();
        assert_eq!(fk, "01F276@Fallout4.esm");
    }

    #[test]
    fn as_form_key_returns_none_on_invalid_hex() {
        let entry = AmmoEntry {
            master: "Fallout4.esm".into(),
            form_id: "ZZZZZZ".into(),
        };
        assert!(entry.as_form_key().is_none());
    }
}
