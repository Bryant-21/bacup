//! Per-run string interner backed by `lasso::ThreadedRodeo`. Sym is a 32-bit handle.
//!
//! `ThreadedRodeo` allows `intern(&self, ...)` so the same interner can be
//! used across worker threads without external synchronization.

use lasso::{Spur, ThreadedRodeo};

/// Interned-string handle. `Copy + Eq + Hash`. Scoped to one `RunHandle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sym(pub(crate) Spur);

/// Per-run string interner. `Sync` — can be shared across threads.
pub struct StringInterner {
    rodeo: ThreadedRodeo,
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            rodeo: ThreadedRodeo::default(),
        }
    }

    pub fn intern(&self, s: &str) -> Sym {
        match self.rodeo.try_get_or_intern(s) {
            Ok(sym) => Sym(sym),
            Err(err) => panic!(
                "string interner allocation failed: bytes={}, interned_strings={}, error={err:?}",
                s.len(),
                self.rodeo.len()
            ),
        }
    }

    /// Look up an existing `Sym` for `s` without minting a new one.
    /// Returns `None` when the string has not been interned in this run.
    /// Use for read-only paths where finding nothing is meaningful (e.g.
    /// "is this plugin name known to the mapper?").
    pub fn get(&self, s: &str) -> Option<Sym> {
        self.rodeo.get(s).map(Sym)
    }

    pub fn resolve(&self, sym: Sym) -> Option<&str> {
        self.rodeo.try_resolve(&sym.0)
    }

    pub fn len(&self) -> usize {
        self.rodeo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rodeo.is_empty()
    }
}

impl Default for StringInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_returns_stable_sym_for_repeated_string() {
        let interner = StringInterner::new();
        let a = interner.intern("hello");
        let b = interner.intern("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn intern_returns_distinct_syms_for_distinct_strings() {
        let interner = StringInterner::new();
        let a = interner.intern("hello");
        let b = interner.intern("world");
        assert_ne!(a, b);
    }

    #[test]
    fn resolve_returns_original_string() {
        let interner = StringInterner::new();
        let sym = interner.intern("WEAP");
        assert_eq!(interner.resolve(sym), Some("WEAP"));
    }

    #[test]
    fn resolve_returns_none_for_unknown_sym() {
        use lasso::{Key, Spur};
        let interner = StringInterner::new();
        // Construct a Spur that was never interned (usize 9999).
        if let Some(spur) = Spur::try_from_usize(9999) {
            assert!(interner.resolve(Sym(spur)).is_none());
        }
        // An empty interner has no valid Syms at all.
        assert!(interner.is_empty());
    }

    #[test]
    fn empty_interner_reports_zero_len() {
        let interner = StringInterner::new();
        assert!(interner.is_empty());
        assert_eq!(interner.len(), 0);
    }

    #[test]
    fn interner_len_grows_with_distinct_strings_only() {
        let interner = StringInterner::new();
        interner.intern("a");
        interner.intern("a");
        interner.intern("b");
        assert_eq!(interner.len(), 2);
    }

    #[test]
    fn get_returns_existing_sym_without_minting() {
        let interner = StringInterner::new();
        let interned = interner.intern("hello");
        assert_eq!(interner.get("hello"), Some(interned));
        // Read-only get must not grow the interner.
        let before = interner.len();
        assert!(interner.get("never-seen").is_none());
        assert_eq!(interner.len(), before);
    }

    #[test]
    fn intern_works_with_shared_reference() {
        let interner = StringInterner::new();
        let a = interner.intern("hello");
        let b = interner.intern("hello");
        assert_eq!(a, b);
        assert_eq!(interner.resolve(a), Some("hello"));
    }

    #[test]
    fn intern_is_thread_safe() {
        use std::sync::Arc;
        use std::thread;

        let interner = Arc::new(StringInterner::new());
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let inter = Arc::clone(&interner);
                thread::spawn(move || {
                    for j in 0..1000 {
                        inter.intern(&format!("k{}_{}", i, j));
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(interner.len(), 8 * 1000);
    }
}
