# Regen Command Reference

> For incremental "upgrade generation" regens (alpha2+), see [regen_upgrade.md](regen_upgrade.md).

Run these from the repo root in PowerShell.

## Full SeventySix via `regen.py`

Full SeventySix regen, no deploy:

```powershell
uv run --no-sync python scripts\regen.py --overwrite-existing
```

Full SeventySix regen, deploy after run:

```powershell
uv run --no-sync python scripts\regen.py --overwrite-existing --deploy
```

Deploy existing full SeventySix output:

```powershell
uv run --no-sync python scripts\regen.py --deploy-only
```

Undeploy full SeventySix output:

```powershell
uv run --no-sync python scripts\regen.py --undeploy
```

Structural records + terrain gate shape, with assets and BA2 disabled:

```powershell
uv run --no-sync python scripts\regen.py --no-nifs --no-textures --no-materials --no-havok --no-drivers --no-animations --no-sounds --no-lod --no-ba2 --overwrite-existing
```

## Notes

- `regen.py --deploy-only` and `regen.py --undeploy` operate on the full `SeventySix` output.

## Deferred Gate 7 smoke run

Do not run this as part of documentation-only or extraction work. Once the
required game data is available, the canonical BACUP end-to-end smoke is:

```powershell
uv run --no-sync python scripts\regen.py --pair fo76:fo4 --mod-name B21_Gate7_GaussPistol --base-game-only --no-placed-records --no-npc-faces --no-terrain --no-lod --no-scripts --no-sounds --anim-text-data-native --validate-output --overwrite-existing
```

The current unified driver intentionally has no single-root FormKey selector;
the retired Gauss-only tool is not a supported entrypoint. This is therefore the
smallest supported `regen.py` shape that still performs the general record pass,
Gauss Pistol asset conversion, target save, post-build/archive work, and CK-free
AnimTextData generation.

Gate 7 passes when the command exits zero, `mods/B21_Gate7_GaussPistol/SeventySix.esm`
contains `GaussPistol` (`54A165:SeventySix.esm`), its referenced converted mesh,
material, and texture files exist in the output, the output contains generated
`data/Meshes/AnimTextData/` files, archive/post-build outputs are present, and
`--validate-output` reports no fatal error. The old
`tests/conversion_baselines/gauss_pistol` fixture was retired with the deleted
single-asset workflow, so it is historical evidence rather than a runnable
comparison target.
