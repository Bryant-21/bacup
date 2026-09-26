//! `rewrite_creature_anam` transform: rewrites FNV creature ANAM paths to
//! FO4-style project `.hkx` paths.
//!
//! The ANAM field on RACE records contains a path like
//! `Creatures\Bloatfly\...` or `Actors\Character\...`. The transform normalises
//! backslashes, splits on the `actors` or `creatures` component, and uses the
//! creature directory name as `target_name`, producing
//! `Actors\<target_name>\<target_name>Project.hkx`. When the target name must
//! differ from the directory name, the orchestrator pre-populates the value
//! before dispatching the transform.
//!
//! YAML usage (fnv_to_fo4.yaml):
//! ```yaml
//! RACE:
//!   transforms:
//!     ANAM:
//!       type: rewrite_creature_anam
//! ```

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

    #[test]
    fn rewrites_creature_behavior_paths_to_fo4_actor_projects() {
        for (input, expected) in [
            (
                r"Creatures\Bloatfly\BloatflyProject.hkx",
                r"Actors\Bloatfly\BloatflyProject.hkx",
            ),
            (
                r"Actors\Radscorpion\RadscorpionProject.hkx",
                r"Actors\Radscorpion\RadscorpionProject.hkx",
            ),
            (
                "Creatures/DeathClaw/DeathClawProject.hkx",
                r"Actors\DeathClaw\DeathClawProject.hkx",
            ),
            (
                r"Creatures\GeckoPowder\something.hkx",
                r"Actors\GeckoPowder\GeckoPowderProject.hkx",
            ),
            (
                r"Meshes\Weapons\Gun\GunProject.hkx",
                r"Meshes\Weapons\Gun\GunProject.hkx",
            ),
            ("", ""),
        ] {
            let (interner, value) = apply(input);
            let FieldValue::String(sym) = value else {
                panic!("expected FieldValue::String for {input:?}");
            };
            assert_eq!(interner.resolve(sym), Some(expected), "{input:?}");
        }

        let interner = StringInterner::new();
        let mut value = FieldValue::Int(42);
        RewriteCreatureAnamTransform
            .apply(
                &mut make_ctx(&interner),
                &mut value,
                &serde_json::Value::Null,
            )
            .unwrap();
        assert_eq!(value, FieldValue::Int(42));
    }
}
