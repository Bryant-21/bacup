//! Load the target-game base Race record as a template.
//!

//! Stub: always returns `None` (DB-backed template lookup is not implemented).
//! Callers MUST fall back to the source record itself (stripped of subgraph
//! fields); the converted plugin is still produced, but the template's
//! non-subgraph subrecords (DESC/FULL/SKIN/etc.) then come from the source race
//! instead of the vanilla target race.

use crate::ids::FormKey;
use crate::record::Record;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Look up the target-game base Race YAML by FormKey and return a stripped
/// template `Record`. Returns `None` when no template can be loaded — today
/// this is unconditional.
///
/// Callers should treat `None` as "use the source record as a template",
/// which is the documented fallback path.
pub fn load_target_race_template(_target_fk: FormKey) -> Option<Record> {
    // TODO: implement DB-backed template loading. Requires SQLite reader
    // for `{target_game}_records.db`, YAML parser with safe_load_hex
    // semantics, and a path from canonical YAML back to a native `Record`.
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    #[test]
    fn returns_none_for_any_fk() {
        let mut interner = StringInterner::new();
        let fk = FormKey {
            local: 0x166729,
            plugin: interner.intern("Fallout4.esm"),
        };
        assert!(load_target_race_template(fk).is_none());
    }
}
