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
