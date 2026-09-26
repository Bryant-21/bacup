//! Ammo substitute table: FNV ammo EditorID → FO4 FormKey mapping, loaded
//! from `embedded/translation_maps/ammo_fnv_to_fo4.yaml`.
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
    /// Parse the YAML text of `ammo_fnv_to_fo4.yaml` into a table. Returns an
    /// empty table (not an error) when the YAML is empty, missing the `ammo`
    /// key, or the key maps to a non-mapping value.
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

    #[test]
    fn parses_sample_yaml_entries() {
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        assert_eq!(table.len(), 4);
        for (eid, master, form_id) in [
            ("Ammo10mm", "Fallout4.esm", "0001F276"),
            ("Ammo762mm", "DLCNukaWorld.esm", "00037897"),
        ] {
            let entry = table.entry(eid).expect("entry present");
            assert_eq!(entry.master, master, "{eid}");
            assert_eq!(entry.form_id, form_id, "{eid}");
        }
        for yaml in ["", "version: 1\n"] {
            assert!(
                AmmoSubstituteTable::from_yaml(yaml).unwrap().is_empty(),
                "{yaml:?}"
            );
        }
    }

    #[test]
    fn lookup_formats_six_digit_uppercase_formkeys() {
        let table = AmmoSubstituteTable::from_yaml(SAMPLE_YAML).unwrap();
        for (eid, expected) in [
            ("Ammo10mm", Some("01F276@Fallout4.esm")),
            ("Ammo556mm", Some("01F278@Fallout4.esm")),
            ("Ammo762mm", Some("037897@DLCNukaWorld.esm")),
            ("AmmoUnknown", None),
        ] {
            assert_eq!(table.lookup(eid).as_deref(), expected, "{eid}");
        }
        for (form_id, expected) in [("1F276", Some("01F276@Fallout4.esm")), ("ZZZZZZ", None)] {
            let entry = AmmoEntry {
                master: "Fallout4.esm".into(),
                form_id: form_id.into(),
            };
            assert_eq!(entry.as_form_key().as_deref(), expected, "{form_id}");
        }
    }
}
