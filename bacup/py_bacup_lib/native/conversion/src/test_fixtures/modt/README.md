# MODT compute calibration — derived rules (Plan B, Task 1)

**Goal of this fixture set:** pin, to byte-exactness, the rules the FO4 Creation
Kit uses to build a record's `MODT` (Model-Info) subrecord from a mesh's
material/texture graph, so Plan B can *compute* a correct `MODT` for novel
converted meshes that have no vanilla `MODT` to harvest.

The oracle is 7 diverse vanilla Fallout4.esm records + their meshes/materials on
disk (`extracted/fo4/`). `tools/modt_calibrate.py` replays the derived rules
against the baked fixtures here and reproduces each vanilla `MODT` **byte-for-byte
on the entry SET, all four counters, and srgb_count** (entry *order* excepted —
see below). Run it: `uv run --no-sync python tools/modt_calibrate.py`.

## Byte layout (confirmed, matches the ESP `model_info` codec)

```
u32 counter_count            == 4
u32 counters[4]              == [num_textures, num_addon_nodes, srgb_count, num_materials]
Texture[num_textures]        12 bytes each
u32  addon_nodes[num_addon_nodes]
Material[num_materials]      12 bytes each
```
Each Texture/Material entry = `{ u32 file_hash, u8 ext[4], u32 folder_hash }`,
i.e. the FO4 BA2 file hash: `file_hash = FileHash.file`, `ext = FileHash.extension`
little-endian 4CC (`dds\0`, `bgsm`, `bgem`), `folder_hash = FileHash.directory`.

## The hash

`file_hash`/`folder_hash`/`ext` come from `bsarchive::fo4::hashing::hash_file`.
It is **exposed to Python** as `materials_native.resource_id_from_path(path) ->
{file, ext, dir}` (the real hash — Plan B should call it directly). The
calibration script uses a from-scratch Python port and proves equivalence by
replaying all 63 `(path -> hash)` vectors in
`py_creation_lib/native/bsarchive/src/fo4/hashing.rs` **and** cross-checking
against `resource_id_from_path`.

`hash_file` normalizes the path first: lowercase ASCII, `/` -> `\`, strip
leading/trailing `\`. Then `file = crc32(stem)`, `directory = crc32(parent)`,
`ext = first 4 bytes of the extension packed little-endian`. crc32 is the
reflected `0xEDB88320` table, **init 0, no final xor**.

## Path form (RULE 1 — pinned)

- **Textures** hash the full path **with a leading `textures\`**. Meshes store
  slots inconsistently — some already carry `textures\...` (MetalBarrel), some
  don't (`armor/raider04/raider04_d.dds`). **Prepend `textures\` iff the path
  does not already start with it** (case-insensitive). Slash direction and case
  are irrelevant (the hash normalizes them).
- **Materials** hash the full path **with a leading `materials\`** (BGSM/BGEM
  material file paths already carry it in the NIF shader `Name`).

## Texture source (RULE 2 — pinned; this is the key finding)

The `MODT` texture list is built from the **resolved materials**, NOT from the
NIF's baked inline texture sets:

- For each shape whose shader property **names a BGSM/BGEM material** (the
  `Name` field on `BSLightingShaderProperty` / `BSEffectShaderProperty`):
  the textures are that **material file's non-empty texture slots** (read the
  `.bgsm`/`.bgem`), by slot name. The NIF's inline `BSShaderTextureSet` is only
  a partial cache and is ignored when a material is present — e.g.
  `CampFireMed01` gets `cubemaps/mudpond_e.dds` and `BPLChandelier01` gets the
  BGEM's `EnvmapTexture`+`NormalTexture`, both **absent from the inline texset**.
- For each shape with **no material** (empty `Name`): fall back to the NIF
  inline data — `BSShaderTextureSet` non-empty slots, or the
  `BSEffectShaderProperty` `Source`/`Greyscale`/`Normal`/`EnvMap`/`EnvMask`
  textures.
- **Deduplicate** by FO4 file hash (so case/slash variants and repeats collapse).

## Material swaps (RULE 3 — pinned; MODT depends on the record, not just the mesh)

If the **record** carries a material swap (`MODS` -> `MSWP`), the CK re-resolves
every shape material through the swap **and** walks one level of the material
template chain:

- **Mode A — no swap:** materials list = the deduped shape material paths
  (leaf BGSM/BGEM only). Root/template materials are **NOT** included, even
  though the BGSMs have a `RootMaterialPath` (verified: MetalBarrel, Campfire,
  Chandelier all exclude their `*Template_Wet.bgsm` roots).
- **Mode B — swap present:** for each shape, the effective material is
  `swap.get(source) or source`; the materials list additionally includes each
  effective material's **direct `RootMaterialPath`** (one level only — the
  template's own parent is *not* included). Textures likewise come from the
  swapped-in material. Verified on `Sedan_Postwar_Cheap01` (MSWP `249A39`
  swaps `Sedan01_Rust.BGSM` -> `Sedan_Postwar_Cheap01.bgsm`; MODT materials =
  `Sedan02_Postwar.BGSM`, `Sedan_Postwar_Cheap01.bgsm`, and the shared root
  `Template\VehicleTemplate_Wet.bgsm`).

> **Plan B implication:** MODT cannot be computed from the mesh alone for
> material-swapped records. The manifest/compute path should either resolve the
> record's `MODS`/`MSWP` (Mode B) or restrict computed MODT to non-swapped
> records (Mode A) and let swapped ones fall through. Most novel converted
> static meshes are Mode A.

## sRGB count (RULE 4 — pinned by slot role, NOT by filename or DDS format)

`srgb_count` = number of **deduped** textures whose slot **role** is sRGB. It is
determined by the texture's semantic slot, not its `_d`/`_n`/`_s` filename
suffix and not the DDS DXGI format (the vanilla DDS use legacy FourCC with no
sRGB flag; a `_s` BC5U is sRGB in one mesh's slot and linear in another's).

| Role (BGSM field / texset slot / effect field)                 | sRGB |
|----------------------------------------------------------------|------|
| Diffuse (BGSM `DiffuseTexture`, texset slot 0), BGEM `BaseTexture` | yes |
| Greyscale / grayscale-to-palette (`GreyscaleTexture`)          | yes  |
| Envmap / cubemap (`EnvmapTexture`, texset slot 4)              | yes  |
| Effect `Source Texture`, effect `Greyscale Texture`            | yes  |
| Glow / emissive (`GlowTexture`) *(inferred — not in sample)*   | yes  |
| Normal (`NormalTexture`, texset slot 1)                        | no   |
| Smoothness/Specular (`SmoothSpecTexture`, texset slot 7)       | no   |
| Inner / Wrinkle / Displacement / EnvMask / Specular masks      | no   |

Decisive cases: `SmokeVapor02Tile_n.dds` sits in the **diffuse** slot (slot 0)
of a texset and is counted **sRGB** despite the `_n` suffix (kills the
suffix theory); armor `_s` in slot 7 is **linear** while `mudpond_e` cubemaps
are **sRGB** (kills the format theory). Confirmed byte-exact srgb_count on all 7
meshes (15, 2, 4, 2, 6, 5, 4).

Confirmed roles: diffuse, normal, smoothspec, envmap, greyscale, base (BGEM),
effect source/greyscale. **Inferred / under-sampled:** glow, inner-layer,
wrinkle, displacement, envmask, and texset slots 2/3/5/6 (no vanilla sample
populated them) — the table above is the best inference; validate if a mesh
using them turns up.

## Entry ordering (NOT reproduced — documented open item)

Vanilla lists textures and materials in an order that is **neither** NIF
traversal order, **nor** any sort by file/dir/ext hash, **nor** a monotonic
hash-bucket order (tested `file`, `dir`, `file^dir`, `file^ext^dir`, `file+dir`,
`dir^ext`, `file*dir` at capacities 16/32/64/128/256 — no consistent monotonic
bucketing). It is the CK's internal `BSTScatterTable` iteration order (chained,
so collided entries move to overflow slots and bucket order is not preserved on
iteration). Reproducing it byte-for-byte would require emulating that container.

**This is functionally irrelevant:** `MODT` is a load-time preload manifest; the
runtime consumes the *set* of hashes, not their order. For novel meshes there is
no vanilla order to match anyway. Plan B should emit entries in a deterministic
order of its own choosing (e.g. gather order) and treat the SET + counters +
srgb_count as the correctness contract — which is exactly what this calibration
proves byte-exact.

## Addon nodes (UNDER-SAMPLED — flagged)

`num_addon_nodes` was **0** for every record sampled (STAT/ARMA/effect statics;
weapon base models carry an empty 20-byte MODT). No mesh with a nonzero
addon-node count was found in `extracted/fo4/` (no `BSValueNode` blocks in the
weapon/effect trees scanned). The array holds `u32` `BGSAddonNode` indices
referenced by the mesh's addon-node blocks. For Plan B's static-mesh targets,
`addon_nodes = []` is the expected and correct value; the non-empty case is the
one dimension **not** calibrated against real data and should be revisited if a
converted mesh ever needs it.

## Fixtures

Each `*.json` bakes the **resolved** graph (self-contained — `extracted/` is not
in git): `record`, `modl`, `material_swap`, `mode`, `modt_hex` (vanilla),
`srgb_count`, `addon_nodes`, `textures[] = {path, role, srgb}`, `materials[]`.
`textures`/`materials` are an unordered SET (see Ordering).

| fixture | record | tex | mat | srgb | exercises |
|---|---|----|----|----|---|
| `metalbarrel` | STAT 048280 | 19 | 1 | 15 | effect shaders (source+greyscale), 1 BGSM, diffuse-slot `_n` |
| `armor_raider04m` | ARMA 08158C (MOD2) | 6 | 2 | 2 | skin+armor BGSM, no-prefix texset paths, `_s` linear |
| `lightoillamp` | STAT 02D459 | 6 | 0 | 4 | zero materials (inline texsets only) |
| `fancychandeliercandle01` | STAT 0D83AB | 4 | 1 | 2 | small single-material |
| `bplchandelier01` | STAT 101064 | 11 | 2 | 6 | BGEM material + BGSM; BGEM slots absent from NIF |
| `campfire_blocks` | STAT 065585 | 13 | 4 | 5 | 4 materials; BGSM envmap absent from inline texset |
| `sedan_postwar_cheap01` | STAT 21DBB1 | 8 | 3 | 4 | **Mode B** material swap + root template |
