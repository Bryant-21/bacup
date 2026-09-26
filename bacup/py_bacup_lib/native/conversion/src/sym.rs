//! Per-run string interner backed by `lasso::ThreadedRodeo`. Sym is a 32-bit handle.
//!
//! `ThreadedRodeo` allows `intern(&self, ...)` so the same interner can be
//! used across worker threads without external synchronization.

use lasso::{Spur, ThreadedRodeo};
use std::sync::Mutex;

/// Interned-string handle. `Copy + Eq + Hash`. Scoped to one `RunHandle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sym(pub(crate) Spur);

/// Per-run string interner. `Sync` — can be shared across threads.
pub struct StringInterner {
    rodeo: ThreadedRodeo,
    // Prevent concurrent misses from racing Lasso's exponentially growing arena buckets.
    insert_lock: Mutex<()>,
}

impl StringInterner {
    pub fn new() -> Self {
        Self {
            rodeo: ThreadedRodeo::default(),
            insert_lock: Mutex::new(()),
        }
    }

    pub fn intern(&self, s: &str) -> Sym {
        if let Some(sym) = self.rodeo.get(s) {
            return Sym(sym);
        }

        let _guard = self
            .insert_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match self.rodeo.try_get_or_intern(s) {
            Ok(sym) => Sym(sym),
            Err(err) => panic!(
                "string interner allocation failed: bytes={}, interned_strings={}, allocated_bytes={}, error={err:?}",
                s.len(),
                self.rodeo.len(),
                self.rodeo.current_memory_usage()
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
    fn intern_resolve_and_len_track_distinct_strings() {
        use lasso::{Key, Spur};
        let interner = StringInterner::new();
        assert!(interner.is_empty());
        assert_eq!(interner.len(), 0);
        if let Some(spur) = Spur::try_from_usize(9999) {
            assert!(interner.resolve(Sym(spur)).is_none());
        }

        let hello = interner.intern("hello");
        assert_eq!(interner.intern("hello"), hello);
        let world = interner.intern("world");
        assert_ne!(hello, world);
        assert_eq!(interner.resolve(hello), Some("hello"));
        assert_eq!(interner.resolve(world), Some("world"));
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

    #[test]
    fn concurrent_inserts_share_one_arena_growth_bucket() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        const THREADS: usize = 32;

        let interner = Arc::new(StringInterner::new());
        interner.intern(&"x".repeat(4080));
        assert_eq!(interner.rodeo.current_memory_usage(), 4096);

        let barrier = Arc::new(Barrier::new(THREADS));
        let handles: Vec<_> = (0..THREADS)
            .map(|i| {
                let interner = Arc::clone(&interner);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    interner.intern(&format!("{i:032}"));
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(interner.len(), THREADS + 1);
        assert_eq!(interner.rodeo.current_memory_usage(), 4096 + 8192);
    }
}
