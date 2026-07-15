# FO76 to FO4 Material Conversion Guide

This guide summarizes the best-fit BGSM and texture mapping rules for converting Fallout 76 materials to Fallout 4's older spec-gloss pipeline.

It is based on:
- `bacup/py_bacup_lib/native/conversion/src/texture_engine/materials.rs` and the
  surrounding BACUP texture/material pipeline
- Vanilla FO4 materials under `extracted/fo4/Materials`
- Vanilla FO76 materials under `extracted/fo76/Materials`
- Reference ports in `refs/fanmods/Gauss Pistol` and `refs/fanmods/Snallygaster`
- Current converted outputs in `mods/B21_Converted_GaussPistol` and `mods/B21_Converted_LvlSnallygaster`

## Core Difference

FO76 BGSMs are effectively PBR-oriented:
- `SpecularTexture` is usually the FO76 `_r.dds` packed roughness/metalness map.
- `LightingTexture` is usually the FO76 `_l.dds` auxiliary map and often carries emissive mask data.
- `RootMaterialPath` is empty on most assets.

FO4 BGSMs are spec-gloss oriented:
- `SmoothSpecTexture` expects `_s.dds`.
- `GlowTexture` expects `_g.dds` when the material is emissive.
- `EnvmapTexture` is optional and is often inherited from `RootMaterialPath` templates rather than set per-material.

## Observed Ground Truth

FO76 extracted materials:
- Weapons: 1107/1107 sampled BGSMs are PBR, 1107/1107 have `SpecularTexture` with no `SmoothSpecTexture`, 1107/1107 have `LightingTexture`, and 1081/1107 have empty `RootMaterialPath`.
- ATX weapons: 1745/1745 have the same pattern and 1745/1745 have empty `RootMaterialPath`.
- Actors: 1963/1979 sampled BGSMs are PBR, 1959/1979 have `SpecularTexture` with no `SmoothSpecTexture`, and 1962/1979 have `LightingTexture`.

FO4 extracted materials:
- Weapons: `template/WeaponMetalTemplate_Wet.bgsm` dominates vanilla weapon BGSM roots.
- Actors: `template/CreatureTemplate_Wet.bgsm` is the main creature root, with some actor subsets using armor or skin templates.
- FO4 weapons and creatures usually leave `EnvmapTexture` empty and inherit behavior from the root template.

Fan ports:
- Gauss Pistol ports use `_s.dds` for `SmoothSpecTexture` and often set `EnvmapTexture` to `Shared/Cubemaps/metal_r.dds` for weapon metal parts.
- Snallygaster ports use `_s.dds` for `SmoothSpecTexture`, `_g.dds` for emissive variants, and `template/CreatureTemplate_Wet.bgsm`.

## Best-Fit Conversion Rules

1. Keep `DiffuseTexture` and `NormalTexture` as direct path remaps.

2. Convert FO76 `_r.dds` into FO4 `_s.dds`.
Use FO76 packed roughness/metalness data as the source for FO4 `SmoothSpecTexture`. Do not leave `_r.dds` referenced by FO4 BGSMs.

3. Convert FO76 `_l.dds` into FO4 `_g.dds` only when the material is actually emissive.
Use `_l.dds` as a glow mask when `EmitEnabled` is true and `GlowTexture` is otherwise empty.

4. Do not invent per-material envmaps from FO76 PBR maps.
Do not write FO76 `_r.dds` or `_l.dds` into `EnvmapTexture`, `InnerLayerTexture`, or `DisplacementTexture`.

5. Prefer FO4 root templates over hard-coded envmaps.
Use `RootMaterialPath` to restore category-appropriate FO4 behavior:
- Weapons: `template/WeaponMetalTemplate_Wet.bgsm`
- Weapon wood parts: `template/WeaponWoodTemplate_Wet.bgsm`
- Weapon plastic or polymer parts: `template/WeaponPlasticTemplate_Wet.bgsm`
- Creatures: `template/CreatureTemplate_Wet.bgsm`
- Armor: `template/ArmorTemplate_Wet.bgsm`
- Clothes: `template/OutfitTemplate_Wet.bgsm`

6. Leave `EnvmapTexture` empty by default.
This matches many vanilla FO4 materials that inherit reflection setup from the root template. Only set a specific cubemap when there is a clear reason and a known-good FO4 reference.

7. For emissive FO76 materials, preserve the emissive scalar settings.
Carry over `EmitEnabled`, `EmittanceColor`, and `EmittanceMult`, but pair them with a real `GlowTexture`/`Glowmap=True` so FO4 emission is spatially masked instead of flooding the whole mesh.

8. For translucent FO76 creature materials, map translucency intent to FO4 subsurface lighting.
Best fit:
- `SubsurfaceLighting = True` when FO76 `Translucency` is true
- `SubsurfaceLightingRolloff = TranslucencyTransmissiveScale`

## Recommended Category Heuristics

### Weapons

Default:
- `RootMaterialPath = template/WeaponMetalTemplate_Wet.bgsm`
- `EnvmapTexture = ""`
- `SmoothSpecTexture = converted _s.dds`

Use `Shared/Cubemaps/metal_r.dds` only when:
- A manual reference port already proves it looks better for that exact asset class.
- The part is clearly polished metal and the root template alone under-reflects.

For FO76 Gauss Pistol specifically:
- Receiver, barrels, magazines, sights: treat as weapon metal.
- Night sights: same metal root, but preserve emissive behavior with `_g.dds` generated from `_l.dds`.

### Creatures

Default:
- `RootMaterialPath = template/CreatureTemplate_Wet.bgsm`
- `EnvmapTexture = ""`
- `SmoothSpecTexture = converted _s.dds`

For glowing creature variants:
- Generate `_g.dds` from the FO76 lighting mask when emission is enabled.
- Set `GlowTexture` to that `_g.dds` and `Glowmap = True`.

For FO76 Snallygaster specifically:
- Base skin variants map cleanly to creature template plus `_s.dds`.
- `snallygasterglow` should become FO4 glow material with `_g.dds`, not a second reference to `_s.dds`.

## What The Reference Ports Suggest

The fan ports consistently do these things right:
- They ship FO4-style `_s.dds` files instead of leaving only `_r.dds`.
- They ship `_g.dds` for emissive creature variants.
- They keep weapon and creature materials on stable FO4 root templates.

The fan ports are useful as visual references, but their per-material cubemap choices should be treated as optional overrides, not the baseline rule.

## Current Failure Modes Seen In Our Converted Outputs

These are the two main cases to avoid:

1. BGSM points at `_s.dds`, but only `_r.dds` exists on disk.
Example pattern seen in `mods/B21_Converted_GaussPistol` and `mods/B21_Converted_LvlSnallygaster`.

2. Emissive `GlowTexture` points at `_s.dds` instead of a real `_g.dds` mask.
This appears on some glowing creature outputs and is visually wrong for FO4.

Practical rule:
- Material slot remapping is only half of the conversion.
- The texture output step must emit files whose names and semantics match the downgraded FO4 BGSM.

## Minimal Decision Table

| FO76 source | FO4 target | Notes |
|---|---|---|
| `DiffuseTexture` / `_d.dds` | `DiffuseTexture` / `_d.dds` | Direct path remap |
| `NormalTexture` / `_n.dds` | `NormalTexture` / `_n.dds` | Convert normal format as needed |
| `SpecularTexture` / `_r.dds` | `SmoothSpecTexture` / `_s.dds` | Main PBR to spec-gloss conversion |
| `LightingTexture` / `_l.dds` with emission | `GlowTexture` / `_g.dds` | Only when emissive or clearly glow-masked |
| `LightingTexture` / `_l.dds` without emission | usually no direct FO4 texture slot | Fold into spec-gloss conversion if needed |
| empty `RootMaterialPath` | synthesize FO4 template root | Prefer template over explicit envmap |
| FO76 translucency | FO4 subsurface lighting | Best fit for creatures |

## Recommended Baseline Policy

Use this as the default FO76 to FO4 conversion policy:

- Synthesize `RootMaterialPath` from asset category.
- Leave `EnvmapTexture` empty unless a known FO4 reference proves otherwise.
- Convert `_r.dds` into FO4 `_s.dds` and ensure that file is actually emitted.
- Convert emissive `_l.dds` into FO4 `_g.dds` and point `GlowTexture` at it.
- Preserve emissive scalar values.
- Map creature translucency to FO4 subsurface lighting.

This gives the closest match to vanilla FO4 material behavior without abusing FO4 envmap slots to store FO76 PBR data.
