//! Byte-exact FO4 `MODT` compute for novel converted meshes (Plan B core).
//!
//! Given a [`MeshModtEntry`] (the resolved material/texture/addon graph of one
//! output mesh) this produces the raw `MODT` subrecord bytes the FO4 Creation Kit
//! would build for that mesh — proven byte-exact against 7 vanilla meshes (see
//! `src/test_fixtures/modt/README.md`).
//!
//! ## Byte layout (matches the ESP `model_info` codec)
//! ```text
//! u32 counter_count            == 4
//! u32 counters[4]              == [num_textures, num_addon_nodes, srgb_count, num_materials]
//! Texture[num_textures]        12 bytes each
//! u32  addon_nodes[num_addon_nodes]
//! Material[num_materials]      12 bytes each
//! ```
//! Each Texture/Material entry = `{ u32 file_hash, u8 ext[4], u32 folder_hash }`
//! (the FO4 BA2 file hash: `ext` is the extension 4CC little-endian, e.g.
//! `dds\0`/`bgsm`/`bgem`).
//!
//! ## Rules (all byte-exact-verified)
//! - **Hash:** `bsarchive_native::fo4::hash_file` (the real FO4 BA2 hash).
//! - **Path form (RULE 1):** prepend `textures\` / `materials\` iff the stored
//!   path doesn't already start with it (case-insensitive). Slash/case are
//!   irrelevant — the hash normalizes them.
//! - **Textures (RULE 2):** from the resolved material slots, deduped by file
//!   hash.
//! - **sRGB (RULE 4):** counted by slot ROLE (see [`crate::modt_manifest::role_is_srgb`]).
//! - **Order:** irrelevant. Vanilla uses the CK scatter-table iteration order;
//!   the correctness contract is entry-SET + all 4 counters + srgb_count, NOT
//!   byte-identity. We emit gather order.
//! - **Swaps (RULE 3):** v1 restricts compute to NON-swapped records; swapped
//!   records return `None` from [`compute_modt`] (deferred).
//!
//! The layout mirrors `esp_authoring_core::plugin_runtime::encode_model_info_json`
//! (Plan C); we emit it locally to avoid threading a schema spec + `PyResult`
//! across the crate boundary. The 7 vanilla fixtures are the byte-exact oracle.

use bsarchive_native::{BStr, fo4::hash_file};
use rustc_hash::FxHashSet;

use crate::modt_manifest::MeshModtEntry;

/// Hash one path (already prefix-normalized) into its 12-byte MODT entry +
/// dedup key.
fn hash_entry(path: &str) -> ([u8; 12], (u32, u32, u32)) {
    let (h, _) = hash_file(BStr::new(path.as_bytes()));
    let mut out = [0u8; 12];
    out[0..4].copy_from_slice(&h.file.to_le_bytes());
    out[4..8].copy_from_slice(&h.extension.to_le_bytes());
    out[8..12].copy_from_slice(&h.directory.to_le_bytes());
    (out, (h.file, h.extension, h.directory))
}

/// Prepend `<prefix_dir>\` iff `path` doesn't already start with it
/// (case-insensitive, slash-insensitive). RULE 1.
fn ensure_prefixed(path: &str, prefix_dir: &str) -> String {
    let normalized = path.to_ascii_lowercase().replace('/', "\\");
    let needle = format!("{prefix_dir}\\");
    if normalized.starts_with(&needle) {
        path.to_string()
    } else {
        format!("{prefix_dir}\\{path}")
    }
}

/// Encode the raw `MODT` bytes for a mesh's resolved graph. Mode-A (no material
/// swap); the caller gates swaps via [`compute_modt`].
pub fn encode_modt(entry: &MeshModtEntry) -> Vec<u8> {
    // Textures — deduped by file hash; sRGB counted by role.
    let mut seen_tex: FxHashSet<(u32, u32, u32)> = FxHashSet::default();
    let mut tex_bytes: Vec<[u8; 12]> = Vec::with_capacity(entry.textures.len());
    let mut srgb_count: u32 = 0;
    for t in &entry.textures {
        let path = ensure_prefixed(&t.path, "textures");
        let (bytes, key) = hash_entry(&path);
        if seen_tex.insert(key) {
            tex_bytes.push(bytes);
            if t.is_srgb() {
                srgb_count += 1;
            }
        }
    }

    // Materials — deduped by file hash.
    let mut seen_mat: FxHashSet<(u32, u32, u32)> = FxHashSet::default();
    let mut mat_bytes: Vec<[u8; 12]> = Vec::with_capacity(entry.materials.len());
    for m in &entry.materials {
        let path = ensure_prefixed(m, "materials");
        let (bytes, key) = hash_entry(&path);
        if seen_mat.insert(key) {
            mat_bytes.push(bytes);
        }
    }

    let mut out = Vec::with_capacity(
        20 + tex_bytes.len() * 12 + entry.addon_nodes.len() * 4 + mat_bytes.len() * 12,
    );
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&(tex_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&(entry.addon_nodes.len() as u32).to_le_bytes());
    out.extend_from_slice(&srgb_count.to_le_bytes());
    out.extend_from_slice(&(mat_bytes.len() as u32).to_le_bytes());
    for e in &tex_bytes {
        out.extend_from_slice(e);
    }
    for a in &entry.addon_nodes {
        out.extend_from_slice(&a.to_le_bytes());
    }
    for e in &mat_bytes {
        out.extend_from_slice(e);
    }
    out
}

/// v1 compute entry: `Some(bytes)` for a non-swapped record, `None` if the record
/// carries a material swap (`MODS`/`MSWP`) — swap resolution is deferred, so the
/// caller falls back to deployed-ESM reuse (upgrade) or drop.
pub fn compute_modt(entry: &MeshModtEntry, has_material_swap: bool) -> Option<Vec<u8>> {
    if has_material_swap {
        return None;
    }
    Some(encode_modt(entry))
}

/// A decoded `MODT` — hashes only (paths are not recoverable). Used for the
/// set-based fixture comparison and available for upgrade-reuse validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedModt {
    pub num_textures: u32,
    pub num_addon_nodes: u32,
    pub srgb_count: u32,
    pub num_materials: u32,
    pub textures: Vec<[u8; 12]>,
    pub addon_nodes: Vec<u32>,
    pub materials: Vec<[u8; 12]>,
}

/// Decode raw `MODT` bytes. `None` on malformed / non-`counter_count==4` payloads
/// or trailing bytes.
pub fn decode_modt(b: &[u8]) -> Option<DecodedModt> {
    if b.len() < 20 {
        return None;
    }
    let rd = |o: usize| -> u32 { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) };
    if rd(0) != 4 {
        return None;
    }
    let num_textures = rd(4);
    let num_addon_nodes = rd(8);
    let srgb_count = rd(12);
    let num_materials = rd(16);
    let mut off = 20usize;

    let read_entry = |off: &mut usize| -> Option<[u8; 12]> {
        if *off + 12 > b.len() {
            return None;
        }
        let mut e = [0u8; 12];
        e.copy_from_slice(&b[*off..*off + 12]);
        *off += 12;
        Some(e)
    };

    let mut textures = Vec::with_capacity(num_textures as usize);
    for _ in 0..num_textures {
        textures.push(read_entry(&mut off)?);
    }
    let mut addon_nodes = Vec::with_capacity(num_addon_nodes as usize);
    for _ in 0..num_addon_nodes {
        if off + 4 > b.len() {
            return None;
        }
        addon_nodes.push(rd(off));
        off += 4;
    }
    let mut materials = Vec::with_capacity(num_materials as usize);
    for _ in 0..num_materials {
        materials.push(read_entry(&mut off)?);
    }
    if off != b.len() {
        return None;
    }
    Some(DecodedModt {
        num_textures,
        num_addon_nodes,
        srgb_count,
        num_materials,
        textures,
        addon_nodes,
        materials,
    })
}

// ---------------------------------------------------------------------------
// Tests — the byte-exact calibration gate (mirrors tools/modt_calibrate.py).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modt_manifest::{ManifestTexture, role_is_srgb};
    use serde::Deserialize;

    /// The 7 vanilla-mesh oracle fixtures.
    const FIXTURES: &[&str] = &[
        "metalbarrel",
        "armor_raider04m",
        "lightoillamp",
        "fancychandeliercandle01",
        "bplchandelier01",
        "campfire_blocks",
        "sedan_postwar_cheap01",
    ];

    #[derive(Deserialize)]
    struct FixtureTexture {
        path: String,
        role: String,
        srgb: bool,
    }

    #[derive(Deserialize)]
    struct Fixture {
        #[serde(default)]
        material_swap: std::collections::HashMap<String, String>,
        modt_hex: String,
        srgb_count: u32,
        #[serde(default)]
        addon_nodes: Vec<u32>,
        textures: Vec<FixtureTexture>,
        #[serde(default)]
        materials: Vec<String>,
    }

    fn load(name: &str) -> Fixture {
        let path = format!(
            "{}/src/test_fixtures/modt/{}.json",
            env!("CARGO_MANIFEST_DIR"),
            name
        );
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse fixture {path}: {e}"))
    }

    fn entry_of(fx: &Fixture) -> MeshModtEntry {
        MeshModtEntry {
            materials: fx.materials.clone(),
            textures: fx
                .textures
                .iter()
                .map(|t| ManifestTexture {
                    path: t.path.clone(),
                    role: t.role.clone(),
                })
                .collect(),
            addon_nodes: fx.addon_nodes.clone(),
        }
    }

    fn sorted(mut v: Vec<[u8; 12]>) -> Vec<[u8; 12]> {
        v.sort_unstable();
        v
    }

    /// The hard gate: compute reproduces every vanilla `MODT` byte-exact on the
    /// entry SET + all 4 counters + srgb_count (entry order excepted per README).
    #[test]
    fn compute_reproduces_all_seven_vanilla_modt() {
        for name in FIXTURES {
            let fx = load(name);

            // RULE 4 cross-check: our role→sRGB rule matches the fixture per texture.
            for t in &fx.textures {
                assert_eq!(
                    role_is_srgb(&t.role),
                    t.srgb,
                    "{name}: role '{}' (path {}) sRGB classification mismatch",
                    t.role,
                    t.path
                );
            }

            let entry = entry_of(&fx);
            let got = encode_modt(&entry);
            let expected = hex::decode(&fx.modt_hex).expect("fixture modt_hex");

            let dg = decode_modt(&got).expect("computed MODT decodes");
            let de = decode_modt(&expected).expect("vanilla MODT decodes");

            // All 4 counters.
            assert_eq!(dg.num_textures, de.num_textures, "{name}: num_textures");
            assert_eq!(dg.num_addon_nodes, de.num_addon_nodes, "{name}: num_addon");
            assert_eq!(dg.srgb_count, de.srgb_count, "{name}: srgb_count");
            assert_eq!(dg.srgb_count, fx.srgb_count, "{name}: srgb vs fixture");
            assert_eq!(dg.num_materials, de.num_materials, "{name}: num_materials");

            // Entry SETs (order-independent).
            assert_eq!(
                sorted(dg.textures.clone()),
                sorted(de.textures.clone()),
                "{name}: texture hash SET"
            );
            assert_eq!(
                sorted(dg.materials.clone()),
                sorted(de.materials.clone()),
                "{name}: material hash SET"
            );
            let mut ga = dg.addon_nodes.clone();
            let mut ea = de.addon_nodes.clone();
            ga.sort_unstable();
            ea.sort_unstable();
            assert_eq!(ga, ea, "{name}: addon-node SET");
        }
    }

    /// `compute_modt` gates material-swapped records (v1): Sedan is Mode B.
    #[test]
    fn compute_modt_gates_material_swapped_records() {
        let fx = load("sedan_postwar_cheap01");
        assert!(!fx.material_swap.is_empty(), "sedan fixture is Mode B");
        let entry = entry_of(&fx);

        assert!(
            compute_modt(&entry, true).is_none(),
            "swapped record must return None (v1 deferral)"
        );
        let non_swapped = compute_modt(&entry, false).expect("non-swapped computes");
        assert_eq!(
            non_swapped,
            encode_modt(&entry),
            "compute_modt(non-swap) == encode_modt"
        );
    }

    #[test]
    fn ensure_prefixed_is_case_and_slash_insensitive() {
        assert_eq!(
            ensure_prefixed("Textures\\SetDressing\\x_d.dds", "textures"),
            "Textures\\SetDressing\\x_d.dds"
        );
        assert_eq!(
            ensure_prefixed("textures/x_d.dds", "textures"),
            "textures/x_d.dds"
        );
        assert_eq!(
            ensure_prefixed("armor/raider04/raider04_d.dds", "textures"),
            "textures\\armor/raider04/raider04_d.dds"
        );
        assert_eq!(
            ensure_prefixed("Materials\\x.BGSM", "materials"),
            "Materials\\x.BGSM"
        );
    }

    #[test]
    fn decode_rejects_malformed() {
        assert!(decode_modt(&[]).is_none());
        assert!(decode_modt(&[0u8; 8]).is_none());
        // counter_count != 4
        let mut b = 5u32.to_le_bytes().to_vec();
        b.extend_from_slice(&[0u8; 16]);
        assert!(decode_modt(&b).is_none());
        // trailing bytes
        let mut b = 4u32.to_le_bytes().to_vec();
        b.extend_from_slice(&0u32.to_le_bytes()); // 0 textures
        b.extend_from_slice(&0u32.to_le_bytes()); // 0 addon
        b.extend_from_slice(&0u32.to_le_bytes()); // 0 srgb
        b.extend_from_slice(&0u32.to_le_bytes()); // 0 materials
        b.push(0xAB); // trailing
        assert!(decode_modt(&b).is_none());
    }
}
