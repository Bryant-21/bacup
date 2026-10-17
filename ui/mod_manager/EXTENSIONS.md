# Metadata adapter API 1

The manager accepts explicitly registered **trusted application code** for save readers and
combined archive precedence policies. These adapters are used by the actual background
save/Data UI workflows. Settings > Metadata adapters shows their IDs, game, version and scope.
No package script, entry point, DLL or MO2 plugin is discovered or imported automatically.

`extensions.Adapter` declares an ID, name, kind (`save` or `archive`), canonical game key,
callback, summary and API version. `Registry.register` rejects duplicate game/kind mappings,
duplicate IDs, invalid display bounds, unsupported versions and more than 64 adapters.
Construct `ManagerController(extensions=registry)` with the intended registry. The default
registry provides the independently implemented FO4 save/plugin-table reader and combined
Data predictor. A custom registry must explicitly register every intended capability.

The save callback accepts `(Save, cancel)` and returns `save_inspection.Inspection`. Registry
dispatch checks cancellation before/after the callback, exact source path, filesystem identity,
inventory size/mtime, full/light count bounds, safe plugin names and case-insensitive duplicates.
It never publishes a partial result. Callbacks should use bounded reads and observe cancellation.
The built-in reader skips screenshot bytes and never allocates/decodes the save body.

The archive callback accepts `(path, loose, archives, settings, *, game)` and returns
`combined_data.Entry`. Dispatch checks the original path/provenance, a bounded explanation,
and that any predicted winner belongs to the enabled, nonhidden candidate set. Returning
`winner=None` with an explanation represents unresolved precedence. The caller pins local
profile state, effective INIs, plugin activation, provider metadata and archive stamps across
the combined inventory. The default FO4 adapter deliberately retains ambiguous attachment
and equal-rank cases instead of choosing an arbitrary archive.

Callbacks execute in the application worker. **This is a contract, not a Python security
sandbox**: trusted code can access the process/filesystem. There is no UI for loading arbitrary
modules, and these APIs do not grant install/deployment, account or launch capabilities.
They do not implement MO2's binary ABI, game/installer/tool extensions or portable packaging.
Those interfaces need separate runtime/write-policy and distribution/license decisions.

Tests exercise actual built-in dispatch, unsupported/duplicate contracts, malformed results,
inactive winners, stale source identity, cancellation and UI failure retention. The built-in
coverage and limitations are recorded in [README.md](README.md) and [PARITY_REVIEW.md](PARITY_REVIEW.md).
