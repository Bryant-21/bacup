# Non-FO76 Conversion Workflows

BACUP's unified driver supports the source-pair IDs declared in
`bacup_lib.source_pairs`: `fo76:fo4`, `fnvfo3:fo4`, and `skyrimse:fo4`.
Select one explicitly from the repository root:

```powershell
uv run --no-sync python scripts\regen.py --pair fnvfo3:fo4 --overwrite-existing
uv run --no-sync python scripts\regen.py --pair skyrimse:fo4 --overwrite-existing
```

New pair work belongs in `bacup_lib` and must use the existing path/run-based
conversion boundary. Do not restore `creation_lib.conversion`, the retired
whole-plugin runtime, or cross-extension ESP handles. Add pair-specific tests
and a documented smoke command with expected plugin, asset, and validation
outputs.

The old FNV script and single-asset/backend-parity helpers are available only in
Git history.

## Isolated agent runner

Build a folder-based snapshot of `regen.py` and both native extensions:

```powershell
bacup\build_regen_onedir.bat
```

The build writes a filtered `.env` beside `dist\BACUP-Regen\BACUP-Regen.exe`
containing only conversion game/data/extracted paths. Credentials and unrelated
repository settings are excluded. You can still override those paths in the
calling process, then run either pair without loading the editable environment's
native DLLs:

```powershell
dist\BACUP-Regen\BACUP-Regen.exe --pair fnvfo3:fo4
dist\BACUP-Regen\BACUP-Regen.exe --pair skyrimse:fo4
```

The executable is a build-time snapshot. Rebuild it after converter or native
changes before using it as end-to-end evidence. Its `mods/`, `cache/`, and logs
are written beside the executable.
