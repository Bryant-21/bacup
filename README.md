# B.A.C.U.P.

**Bethesda Asset Conversion Utility Program** — a framework for converting
records and assets from one Bethesda / Creation Engine game to another. BACUP
is not a single-purpose Fallout 76 → Fallout 4 tool: each game's record
schema, FormID space, and asset formats are modeled independently, and a
source → target *pair* plugs into a shared conversion pipeline (schema-driven
record translation, FormID remapping, per-pair fixups) plus shared asset
converters for meshes, materials, textures, Havok, terrain, and audio.

## Supported conversions

A unified driver dispatches by source → target pair. As of this writing:

| Pair | Maturity |
|---|---|
| Fallout 76 → Fallout 4 (`fo76:fo4`) | Most mature pipeline — full record translation, terrain/LOD, Havok, NPC/creature conversion, audio, archive packaging |
| Fallout 3 / New Vegas → Fallout 4 (`fnvfo3:fo4`) | Wired into the same driver and pair-hook architecture; hook coverage still growing |
| Skyrim SE → Fallout 4 (`skyrimse:fo4`) | Wired into the same driver; earliest-stage hook coverage |

Adding a new pair means implementing another pair-hook module against the
existing phase pipeline — it isn't a new engine. The framework carries
per-game schemas for Fallout 3, Fallout: New Vegas, Fallout 4, Fallout 76,
Skyrim SE, and Starfield; conversion pairs targeting the schemas beyond the
three above are not yet wired into the driver.

## What it converts

- **Plugin records** — schema-driven translation and FormID remapping across
  per-game schemas, with per-pair fixups for source/target quirks
- **Meshes** — NIF conversion, including legacy Gamebryo-era meshes
- **Materials** — material file translation and path remapping
- **Textures** — DDS conversion and channel remapping
- **Havok** — skeletons, behaviors, animation drivers, cloth/ragdoll
  postprocessing
- **Terrain + LOD** — terrain graft and object/terrain LOD generation
- **Sound**
- **Archives** — BA2/BSA packaging of converted assets
- NPCs/creatures, equipment, interior cells, navmeshes, and Story Manager
  content convert as part of the record pipeline

## Architecture

- **Native Rust core** — `bacup_lib._native.conversion_native` runs the
  conversion work as a registry of phases (record translation, terrain,
  meshes, materials, textures, Havok, sound, archive output) executed either
  flat or as a dependency-scheduled stage DAG for phase-level parallelism.
  BACUP compiles its own private `esp_authoring_core` for plugin I/O; numeric
  ESP handles never cross into or out of this extension.
- **Shared schema/asset layer** — built on top of py-creation-lib (a
  submodule here) for per-game record schemas, NIF, DDS, BA2/BSA, materials,
  and terrain primitives.
- **Python orchestration** (`bacup_lib`) — drives conversion runs, exposes the
  scriptable `scripts/regen.py` entry point, and reports phase progress.
- **Desktop UI** (`bacup_ui`) — an ImGui application for interactive setup,
  browsing, and driving conversions without the CLI.

## Build

    git clone --recursive https://github.com/Bryant-21/bacup.git
    cd bacup
    uv sync                      # builds both native packages (needs Rust + MSVC)
    uv run python -m bacup_ui    # launch the BACUP UI

Release exe: `bacup/build_bacup.ps1` / `bacup/build_bacup.bat` (BACUP.exe). CI
builds it on tags.

## Convert a game

    uv run --no-sync python scripts/regen.py --overwrite-existing                    # fo76:fo4 (default)
    uv run --no-sync python scripts/regen.py --pair fnvfo3:fo4 --overwrite-existing
    uv run --no-sync python scripts/regen.py --pair skyrimse:fo4 --overwrite-existing

See `bacup/docs/regen_commands.md` and `bacup/docs/non-fo76-conversion-workflows.md`
for the full command reference, including deploy/undeploy and partial-scope runs.

`fo76:fo4` LOD profiles live under `bacup/scripts/lod_settings/` — see
`bacup/docs/regen_commands.md` for the named profiles.

## Game data

You need your own legally-owned copies of the games you convert between.
Point the in-app setup at your installs. Asset-dependent tests skip unless
`FO4_EXTRACTED_DIR` / `FO76_EXTRACTED_DIR` (and friends — see the
py-creation-lib README) are set. Never commit game assets.

## License

GPL-3.0 — see LICENSE. Family: [py-creation-lib](https://github.com/Bryant-21/py-creation-lib) ·
[modkit21](https://github.com/Bryant-21/modkit21)
