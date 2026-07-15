//! `rewrite_creature_anam` transform — rewrites FNV creature ANAM paths to
//! FO4-style project `.hkx` paths.
//!
//! Python source: `translator.py` lines 456-474.
//!
//! The ANAM field on RACE records contains a path like
//! `Creatures\Bloatfly\...` or `Actors\Character\...`. When the source dir
//! resolves to a known creature catalog entry the path is rewritten to
//! `Actors\<target_name>\<target_name>Project.hkx`.
//!
//! The Rust port implements the path-rewrite logic directly (without a
//! creature catalog, which is a Python-side data structure). It normalises
//! backslashes, splits on `actors` or `creatures` path components, and
//! produces the FO4 project path when the source dir can be extracted.
//!
//! YAML usage (fnv_to_fo4.yaml):
//! ```yaml
//! RACE:
//!   transforms:
//!     ANAM:
//!       type: rewrite_creature_anam
//! ```
//!
//! # Design note
//!
//! The Python implementation relies on a `CreatureCatalog` to look up
//! `target_name` from `source_dir`. The Rust port omits the catalog lookup —
//! instead it extracts the creature directory name from the path and uses that
//! as the FO4 `target_name`. This mirrors what the catalog does for the vast
//! majority of entries (where `target_name == dir_name`). When a catalog
//! override is needed, the orchestrator should pre-populate the value before
//! dispatching the transform.

use super::{Transform, TransformCtx, TransformError};
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;

/// Rewrites creature ANAM paths from FNV layout to FO4 project hkx layout.
pub struct RewriteCreatureAnamTransform;

impl RewriteCreatureAnamTransform {
    /// Extract the creature directory from a path, or `None` if not found.
    ///
    /// Looks for `actors/` or `creatures/` path components (case-insensitive)
    /// and returns `creatures/<next_component>` when found.
    fn creature_dir_from_path(path: &str) -> Option<String> {
        let normalised: String = path.replace('\\', "/");
        let parts: Vec<&str> = normalised
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        for (i, part) in parts[..parts.len().saturating_sub(1)].iter().enumerate() {
            let lower = part.to_lowercase();
            if lower == "actors" || lower == "creatures" {
                if let Some(dir) = parts.get(i + 1) {
                    return Some(format!("Creatures/{dir}"));
                }
            }
        }
        None
    }

    /// Derive the FO4 creature name from the source directory path component.
    ///
    /// Given `Creatures/Bloatfly` → `Bloatfly`.
    fn creature_name_from_dir(dir: &str) -> Option<&str> {
        dir.rsplit('/').next().filter(|s| !s.is_empty())
    }
}

impl Transform for RewriteCreatureAnamTransform {
    fn name(&self) -> &'static str {
        "rewrite_creature_anam"
    }

    /// Rewrite the ANAM string value to an FO4 project hkx path.
    ///
    /// Non-string values are passed through unchanged. Strings that do not
    /// match the expected pattern are also passed through unchanged.
    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        _config: &YamlValue,
    ) -> Result<(), TransformError> {
        let sym = match value {
            FieldValue::String(s) => *s,
            _ => return Ok(()),
        };

        let path = match ctx.interner.resolve(sym) {
            Some(s) => s.to_owned(),
            None => return Ok(()),
        };

        let source_dir = match Self::creature_dir_from_path(&path) {
            Some(d) => d,
            None => return Ok(()),
        };

        let creature_name = match Self::creature_name_from_dir(&source_dir) {
            Some(n) => n.to_owned(),
            None => return Ok(()),
        };

        let fo4_path = format!("Actors\\{}\\{}Project.hkx", creature_name, creature_name);
        *value = FieldValue::String(ctx.interner.intern(&fo4_path));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;
    use crate::translator::transforms::TransformCtx;

    fn make_ctx(interner: &StringInterner) -> TransformCtx<'_> {
        TransformCtx { interner }
    }

    fn apply(path: &str) -> (StringInterner, FieldValue) {
        let mut interner = StringInterner::new();
        let sym = interner.intern(path);
        let mut value = FieldValue::String(sym);
        let mut ctx = make_ctx(&mut interner);
        RewriteCreatureAnamTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();
        (interner, value)
    }

    // -------------------------------------------------------------------------
    // Happy-path rewrites
    // -------------------------------------------------------------------------

    #[test]
    fn rewrites_creatures_path_with_backslashes() {
        let (interner, value) = apply(r"Creatures\Bloatfly\BloatflyProject.hkx");
        if let FieldValue::String(sym) = value {
            assert_eq!(
                interner.resolve(sym),
                Some(r"Actors\Bloatfly\BloatflyProject.hkx")
            );
        } else {
            panic!("expected FieldValue::String");
        }
    }

    #[test]
    fn rewrites_actors_path_with_backslashes() {
        let (interner, value) = apply(r"Actors\Radscorpion\RadscorpionProject.hkx");
        if let FieldValue::String(sym) = value {
            assert_eq!(
                interner.resolve(sym),
                Some(r"Actors\Radscorpion\RadscorpionProject.hkx")
            );
        } else {
            panic!("expected FieldValue::String");
        }
    }

    #[test]
    fn rewrites_creatures_path_with_forward_slashes() {
        let (interner, value) = apply("Creatures/DeathClaw/DeathClawProject.hkx");
        if let FieldValue::String(sym) = value {
            assert_eq!(
                interner.resolve(sym),
                Some(r"Actors\DeathClaw\DeathClawProject.hkx")
            );
        } else {
            panic!("expected FieldValue::String");
        }
    }

    #[test]
    fn uses_dir_component_as_creature_name() {
        let (interner, value) = apply(r"Creatures\GeckoPowder\something.hkx");
        if let FieldValue::String(sym) = value {
            let result = interner.resolve(sym).unwrap();
            assert_eq!(result, r"Actors\GeckoPowder\GeckoPowderProject.hkx");
        } else {
            panic!("expected FieldValue::String");
        }
    }

    // -------------------------------------------------------------------------
    // Pass-through cases
    // -------------------------------------------------------------------------

    #[test]
    fn passthrough_non_string_value() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Int(42);
        let mut ctx = make_ctx(&mut interner);
        RewriteCreatureAnamTransform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();
        assert_eq!(value, FieldValue::Int(42));
    }

    #[test]
    fn passthrough_path_with_no_actors_or_creatures_component() {
        // A path that does not contain actors/ or creatures/ should pass through.
        let (interner, value) = apply(r"Meshes\Weapons\Gun\GunProject.hkx");
        if let FieldValue::String(sym) = value {
            assert_eq!(
                interner.resolve(sym),
                Some(r"Meshes\Weapons\Gun\GunProject.hkx")
            );
        } else {
            panic!("expected FieldValue::String");
        }
    }

    #[test]
    fn passthrough_empty_string() {
        let (interner, value) = apply("");
        if let FieldValue::String(sym) = value {
            assert_eq!(interner.resolve(sym), Some(""));
        } else {
            panic!("expected FieldValue::String");
        }
    }
}
