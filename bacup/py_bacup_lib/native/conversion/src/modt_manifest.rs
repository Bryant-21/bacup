//! MODT compute manifest — the per-output-mesh material/texture/addon-node graph
//! the asset waves emit and the `regenerate_modt` phase consumes to compute a
//! byte-exact FO4 `MODT` for novel converted meshes.
//!
//! Shape rationale (see `src/test_fixtures/modt/README.md` for the byte-exact
//! calibration): a record's `MODT` texture list is built from the mesh's
//! **resolved material slots** (not the NIF inline texture sets), so the producer
//! must resolve each shape's `.bgsm`/`.bgem` and emit the final FO4 texture paths
//! with their semantic slot ROLE (which fixes sRGB per RULE 4), the resolved
//! material paths, and any addon-node indices. Material swaps are a per-RECORD
//! concern (`MODS`/`MSWP`), resolved by the phase — not baked here.
//!
//! This module defines only the data shape + the role→sRGB rule; the producer is
//! wired in a later step.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One texture slot referenced by an output mesh's resolved materials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestTexture {
    /// FO4 texture path as stored on the resolved material slot. May or may not
    /// already carry a leading `textures\` — the encoder normalizes it (RULE 1).
    pub path: String,
    /// Semantic slot role (e.g. `diffuse`, `normal`, `envmap`, `eff_source`).
    /// Drives sRGB per RULE 4 via [`role_is_srgb`]. Kept as a free-form string so
    /// the manifest stays forward-compatible with roles not yet calibrated;
    /// unknown roles are treated as linear.
    pub role: String,
}

impl ManifestTexture {
    /// Whether this texture counts toward `MODT`'s sRGB counter (by ROLE).
    pub fn is_srgb(&self) -> bool {
        role_is_srgb(&self.role)
    }
}

/// Per-output-mesh MODT inputs. The asset-wave producer emits one of these per
/// converted output mesh; the phase computes `MODT` from it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MeshModtEntry {
    /// Resolved FO4 material paths (leaf `.bgsm`/`.bgem`), Mode-A order. Deduped
    /// by file hash at encode time.
    #[serde(default)]
    pub materials: Vec<String>,
    /// Textures from the resolved material slots (Mode A), with slot roles.
    /// Deduped by file hash at encode time.
    #[serde(default)]
    pub textures: Vec<ManifestTexture>,
    /// `BGSAddonNode` indices referenced by the mesh's addon-node blocks. Empty
    /// for static-mesh targets (the only calibrated case).
    #[serde(default)]
    pub addon_nodes: Vec<u32>,
}

/// The full manifest: `output-mesh-path -> entry`. The key is normalized exactly
/// like `harvest_modt::normalize_model_path` (lowercase, forward slashes, no
/// leading `meshes/`) so the phase can look up a record's `MODL` directly.
///
/// Serializes as a bare JSON object (`{ "<mesh>": { ... } }`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MeshModtManifest {
    pub meshes: BTreeMap<String, MeshModtEntry>,
}

impl MeshModtManifest {
    pub fn get(&self, normalized_model_path: &str) -> Option<&MeshModtEntry> {
        self.meshes.get(normalized_model_path)
    }

    pub fn is_empty(&self) -> bool {
        self.meshes.is_empty()
    }
}

/// The sRGB rule (README RULE 4): sRGB-ness is a property of the texture's
/// semantic SLOT ROLE, not its filename suffix or DDS format.
///
/// sRGB: diffuse/base, greyscale, envmap/cubemap, glow/emissive, and the effect
/// shader Source/Greyscale/EnvMap roles. Everything else (normal, smoothspec,
/// specular, inner/wrinkle/displacement/envmask) and any unknown role is linear.
pub fn role_is_srgb(role: &str) -> bool {
    matches!(
        role.trim().to_ascii_lowercase().as_str(),
        "diffuse"
            | "base"
            | "greyscale"
            | "grayscale"
            | "envmap"
            | "cubemap"
            | "eff_envmap"
            | "glow"
            | "emissive"
            | "eff_source"
            | "eff_greyscale"
            | "eff_grayscale"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_roles_are_classified_by_role_not_suffix() {
        // sRGB roles
        for r in [
            "diffuse",
            "base",
            "greyscale",
            "envmap",
            "eff_source",
            "eff_greyscale",
            "eff_envmap",
            "glow",
        ] {
            assert!(role_is_srgb(r), "{r} should be sRGB");
        }
        // Linear roles
        for r in [
            "normal",
            "smoothspec",
            "specular",
            "envmask",
            "inner",
            "wrinkle",
            "displacement",
            "unknown_future_role",
        ] {
            assert!(!role_is_srgb(r), "{r} should be linear");
        }
        // Case / whitespace insensitive.
        assert!(role_is_srgb("  Diffuse "));
        assert!(!role_is_srgb("NORMAL"));
    }

    #[test]
    fn manifest_serializes_as_bare_map() {
        let mut m = MeshModtManifest::default();
        m.meshes.insert(
            "setdressing/x.nif".to_string(),
            MeshModtEntry {
                materials: vec!["materials\\x.bgsm".to_string()],
                textures: vec![ManifestTexture {
                    path: "textures\\x_d.dds".to_string(),
                    role: "diffuse".to_string(),
                }],
                addon_nodes: vec![],
            },
        );
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.starts_with("{\"setdressing/x.nif\":"), "got {json}");
        let back: MeshModtManifest = serde_json::from_str(&json).unwrap();
        assert!(back.get("setdressing/x.nif").is_some());
    }
}
