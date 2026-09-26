//! FormKey parsing helpers for FNV legacy scripting.
//!
//! Mirrors `fnv_legacy_scripting/form_keys.py`.

/// Split an `object_id:plugin` FormKey into its two parts.
///
/// Returns `(object_id, plugin_name)`. Either part may be empty if the
/// input is malformed or missing the separator.
pub fn split_form_key(form_key: &str) -> (&str, &str) {
    let value = form_key.trim();
    match value.split_once(':') {
        Some((object_id, plugin_name)) => (object_id.trim(), plugin_name.trim()),
        None => (value, ""),
    }
}

/// Extract the object-ID portion of a FormKey and upper-case it.
pub fn object_id_from_form_key(form_key: &str) -> String {
    let (object_id, _) = split_form_key(form_key);
    object_id.to_uppercase()
}

/// Extract the plugin-name portion of a FormKey.
pub fn plugin_name_from_form_key(form_key: &str) -> String {
    let (_, plugin_name) = split_form_key(form_key);
    plugin_name.to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_extract_form_key_parts() {
        for (key, oid, plugin) in [
            ("001234:MyMod.esm", "001234", "MyMod.esm"),
            ("ABCDEF", "ABCDEF", ""),
            ("", "", ""),
        ] {
            assert_eq!(split_form_key(key), (oid, plugin), "{key:?}");
        }
        assert_eq!(object_id_from_form_key("abcdef:Plugin.esm"), "ABCDEF");
        assert_eq!(plugin_name_from_form_key("001234:FNV.esm"), "FNV.esm");
    }
}
