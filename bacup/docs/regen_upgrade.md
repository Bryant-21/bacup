# SeventySix upgrade generation

Upgrade mode regenerates only the asset families that changed between the
currently-deployed version and a target version, per
`bacup_lib/resources/conversion/upgrade_manifest.yaml` (bundled into the
EXE; repository source:
`bacup/py_bacup_lib/python/bacup_lib/resources/conversion/upgrade_manifest.yaml`).
It reuses everything else from the live deployment: LAND
/ NAVM / NAVI + terrain-texture records are grafted from the deployed
`SeventySix.esm` instead of recomputed, and only the BA2 shards for the
changed families are deleted and replaced on disk — the rest of the deployed
archives are left untouched. The new version is stamped into the output
ESM's `TES4` `SNAM` subrecord so the next upgrade run can auto-detect the
installed version.

## Generate the current release from an installed build

```
uv run --no-sync python scripts/regen.py --upgrade --deploy
```

- `--upgrade-manifest` is an optional override — both the UI panel and the bare
  CLI script default to the manifest bundled with the converter,
  `bacup_lib/resources/conversion/upgrade_manifest.yaml`. Pass
  `--upgrade-manifest <path>` to point at a different manifest instead.
- `--upgrade-from` is omitted here: it auto-detects the installed version by
  reading `SNAM` off the deployed `SeventySix.esm`. Pass it explicitly to
  override (e.g. if the deployed ESM predates version stamping).
- `--mod-version` is omitted here: it defaults to the manifest's `current`
  (`alpha2.1`), which is also what gets stamped into the new ESM's `SNAM`.

## alpha2 -> alpha2.1 family scope

- **Regenerated**: NIFs, Havok (including behavior-driver synthesis), scripts,
  textures, and the ESM. LAND / NAVM / NAVI and terrain-texture records are
  grafted from the deployed alpha2 ESM.
- **Packed and deployed**: the `Meshes`, `MeshesExtra`, `Animations`, `Misc`,
  and `Textures` BA2 shards from the main mod. The UI deploys the companion mod
  separately.
- **Reused from the deployed alpha2 output**: materials, terrain, LOD, and
  sounds.
- **Repeatable**: running the upgrade again when Alpha 2.1 is already installed
  repeats this same partial scope instead of returning an already-current no-op.

## alpha1 -> alpha2 family scope

- **Rebuilt**: all families. The `alpha2` `fo76:fo4` entry is `[ALL]` and its
  pair-specific force flag requires a clean build.
- **Reused from the deployed alpha1 output**: nothing.

## Adding a future alpha3

Append a new entry under `versions:` in `upgrade_manifest.yaml` with its `id`
and the families it changes, e.g.:

```yaml
  - id: alpha3
    families_by_conversion:
      "fo76:fo4": [Textures]
      "fnvfo3:fo4": [NONE]
      "skyrimse:fo4": [Textures]
    force_regen_by_conversion:
      "skyrimse:fo4": false
    notes_by_conversion:
      "skyrimse:fo4":
        - Updated Skyrim texture conversion.
```

`notes_by_conversion` scopes changelog entries to the matching B.A.C.U.P. tab.
Versions without notes for a conversion do not appear in that tab's changelog.
`families_by_conversion` is required. Every supported pair must be listed; use
`[NONE]` when a version requires no work for that pair. Every family listed for
the target version runs even when that version is already installed. Forced
clean rebuilds are declared only through `force_regen_by_conversion`.
