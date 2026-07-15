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
    fn split_colon_key() {
        let (oid, plugin) = split_form_key("001234:MyMod.esm");
        assert_eq!(oid, "001234");
        assert_eq!(plugin, "MyMod.esm");
    }

    #[test]
    fn split_no_colon() {
        let (oid, plugin) = split_form_key("ABCDEF");
        assert_eq!(oid, "ABCDEF");
        assert_eq!(plugin, "");
    }

    #[test]
    fn object_id_uppercased() {
        assert_eq!(object_id_from_form_key("abcdef:Plugin.esm"), "ABCDEF");
    }

    #[test]
    fn plugin_name_extracted() {
        assert_eq!(plugin_name_from_form_key("001234:FNV.esm"), "FNV.esm");
    }

    #[test]
    fn empty_form_key() {
        let (oid, plugin) = split_form_key("");
        assert_eq!(oid, "");
        assert_eq!(plugin, "");
    }
}
