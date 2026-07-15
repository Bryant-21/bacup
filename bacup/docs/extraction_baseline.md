# Conversion extraction baseline

## Baseline 2026-07-13

### Branch and worktree

- Intake was on clean `main`.
- `pre_extraction_status.txt` was 0 bytes (no pre-existing worktree changes).
- Extraction branch: `bacup-extraction`.

### Test results

`cargo test -p conversion_native`

- The run was terminated after about 6.5 minutes because process CPU time remained fixed for more than 3 minutes; Cargo exited `-1` with `error: test failed` rather than printing a final `test result` summary.
- Before termination, 2,015 tests completed successfully and 34 tests had run longer than 60 seconds.
- Two explicit pre-existing failures were observed:
  - `phase::equipment::tests::convert_equipment_empty_keys`
  - `phase::projected_navmeshes::tests::emit_discards_per_record_mapper_mutations`

`uv run python scripts/ensure_native.py`

- Exit code `0`; no output.

`uv run --no-sync pytest py_creation_lib/python/creation_lib/conversion/tests -q --no-header -p no:cacheprovider`

- `1 failed, 288 passed, 77 skipped`.
- Pre-existing failure: `py_creation_lib/python/creation_lib/conversion/tests/test_lod_worldspace_discovery.py::test_fo76_default_discovers_worldspaces_but_keeps_tuned_profile` (`AssertionError: assert ['APPALACHIA'] == []`).

Cross-cutting selection (`test_texture_dirs_order.py`, `test_havok_python_thin_bindings.py`, `test_max_kf_bridge.py`, and `test_backend_host_classify.py`)

- Collection failed before the selection ran: `ModuleNotFoundError: No module named 'creation_lib.conversion.animation.kf_reader'`.

These results are the pre-extraction baseline. Do not treat them as extraction regressions.

## Embedded-resource inventory

### Rust `include_str!` / `include_bytes!`

The scan found 48 real macro callsites. The following 45 are crate-local and remain valid when the complete conversion crate is moved:

```text
src/embedded.rs:11  embedded/translation_maps/ammo_fnv_to_fo4.yaml
src/embedded.rs:13  embedded/translation_maps/events_fo3_to_fo4.yaml
src/embedded.rs:15  embedded/translation_maps/events_fo4_to_fo3.yaml
src/embedded.rs:17  embedded/translation_maps/events_fo76_to_fo4.yaml
src/embedded.rs:18  embedded/translation_maps/fnv_to_fo4.yaml
src/embedded.rs:19  embedded/translation_maps/fo3_to_fo4.yaml
src/embedded.rs:20  embedded/translation_maps/fo4_to_skyrimse.yaml
src/embedded.rs:21  embedded/translation_maps/fo76_to_fnv.yaml
src/embedded.rs:22  embedded/translation_maps/fo76_to_fo4.yaml
src/embedded.rs:23  embedded/translation_maps/fo76_to_skyrimse.yaml
src/embedded.rs:25  embedded/translation_maps/skeleton_fnv_to_fo4_creatures.yaml
src/embedded.rs:27  embedded/translation_maps/skeleton_fnv_to_fo4_robots.yaml
src/embedded.rs:29  embedded/translation_maps/skeleton_fo3_to_fo4.yaml
src/embedded.rs:31  embedded/translation_maps/skeleton_fo3_to_fo4_creatures.yaml
src/embedded.rs:32  embedded/translation_maps/skyrimse_to_fo4.yaml
src/embedded.rs:33  embedded/translation_maps/starfield_to_fo4.yaml
src/embedded.rs:39  embedded/whitelists/fnv.yaml
src/embedded.rs:40  embedded/whitelists/fo3.yaml
src/embedded.rs:41  embedded/whitelists/fo4.yaml
src/embedded.rs:42  embedded/whitelists/fo76.yaml
src/embedded.rs:43  embedded/whitelists/skyrimse.yaml
src/embedded.rs:44  embedded/whitelists/starfield.yaml
src/embedded.rs:46  embedded/whitelists/universal_omod_keywords.yaml
src/embedded.rs:52  embedded/fo76_condition_functions.yaml
src/embedded.rs:54  embedded/skyrimse_condition_functions.yaml
src/embedded.rs:55  embedded/material_source_overrides.yaml
src/embedded.rs:56  embedded/weapon_extra_fks.yaml
src/run.rs:6992  run.rs
src/target_write.rs:7818  test_fixtures/nvnm_grid_ck/src_4EA53D.nvnm.bin
src/target_write.rs:7819  test_fixtures/nvnm_grid_ck/ck_4EA53D.nvnm.bin
src/target_write.rs:7825  test_fixtures/nvnm_grid_ck/src_4EA542.nvnm.bin
src/target_write.rs:7826  test_fixtures/nvnm_grid_ck/ck_4EA542.nvnm.bin
src/target_write.rs:7832  test_fixtures/nvnm_grid_ck/src_4EA534.nvnm.bin
src/target_write.rs:7833  test_fixtures/nvnm_grid_ck/ck_4EA534.nvnm.bin
src/target_write.rs:7839  test_fixtures/nvnm_grid_ck/src_2B740A.nvnm.bin
src/target_write.rs:7840  test_fixtures/nvnm_grid_ck/ck_2B740A.nvnm.bin
src/target_write.rs:7846  test_fixtures/nvnm_grid_ck/src_4EA532.nvnm.bin
src/target_write.rs:7847  test_fixtures/nvnm_grid_ck/ck_4EA532.nvnm.bin
src/target_write.rs:7855  test_fixtures/nvnm_grid_ck/src_4EA53D.nvnm.bin
src/phase/animations.rs:56  animations/weapon_family_table.yaml
src/phase/creatures.rs:28  catalog.yaml
src/phase/drivers.rs:29  havok/drivers.yaml
src/phase/face.rs:43  resources/face/hair_lookup.yaml
src/phase/face.rs:45  resources/face/named_bones.yaml
src/phase/walk.rs:37  walk/policy.yaml
```

Three source-relative includes reach the sibling `esp` crate and must be retargeted when conversion moves. From `bacup/py_bacup_lib/native/conversion/src`, `../../esp/...` becomes `../../../../../py_creation_lib/native/esp/...`:

```text
src/skyrim_navmesh.rs:113  ../../esp/src/nvnm/tests/fixtures/0e537d_skyrim_v12.nvnm.hex
src/skyrim_navmesh.rs:139  ../../esp/src/nvnm/tests/fixtures/0e537d_skyrim_v12.nvnm.hex
src/target_write.rs:7748  ../../esp/src/nvnm/tests/fixtures/4ea534_fo4.nvnm.bin
```

The grep also matches comment-only mentions at `src/embedded.rs:1`, `src/translator/maps.rs:3`, and `src/translator/transforms/condition_functions.rs:6`; they are not resource lookups.

### `CARGO_MANIFEST_DIR` classification

The scan found 37 macro callsites and one comment-only hit. Every hit is classified below.

Moves to `ck_native` in Task 9; keep the current manifest-relative depth unchanged:

```text
src/fixups/havok/anim_text_data_emit.rs:1452       ../../../refs/fanmods/Snallygaster/Meshes
src/fixups/havok/anim_text_data_emit.rs:2808       ../../..
src/fixups/havok/anim_text_data_bucket_files.rs:798   ../../../refs/fanmods/Snallygaster/Meshes/AnimTextData
src/fixups/havok/anim_text_data_bucket_files.rs:924   ../../../refs/fanmods/Snallygaster/Meshes/AnimTextData/SyncAnimData/ResolvedSyncAnimDataB21_Snallygaster.txt
src/fixups/havok/anim_text_data_bucket_files.rs:1285  ../../../extracted/fo4/Meshes/AnimTextData/animationstancedata/14636681807525876636.txt
src/fixups/havok/anim_text_data_bucket_files.rs:1394  ../../../refs/fanmods/Gauss Pistol/Meshes/AnimTextData/AnimationStanceData
src/fixups/havok/anim_text_data_event_resolver.rs:537  ../../../extracted/fo4/meshes
src/fixups/havok/anim_text_data_event_resolver.rs:541  ../../../refs/fanmods/Snallygaster/Meshes
src/fixups/havok/anim_text_data_extract.rs:738       ../../../refs/fanmods/Snallygaster/Meshes
src/fixups/havok/anim_text_data_graph.rs:890         ../../../extracted/fo4/Meshes
src/fixups/havok/anim_text_data_offsets.rs:779       ../../../refs/fanmods/Snallygaster/Meshes/Actors/Snallygaster/Animations/{name}
src/fixups/havok/anim_text_data_offsets.rs:947       ../../..
src/fixups/havok/anim_text_data_offsets.rs:1087      ../../../refs/fanmods/Snallygaster/Meshes
src/fixups/havok/anim_text_data_offsets.rs:1183      ../../../refs/fanmods/Snallygaster/Meshes
src/fixups/havok/anim_text_data_offsets.rs:1266      ../../../refs/fanmods/Gauss Pistol/Meshes
src/fixups/havok/anim_text_data_offsets.rs:1270      ../../../extracted/fo4/Meshes
src/fixups/havok/anim_text_data_speed.rs:3217        ../../..
src/fixups/havok/anim_text_data_speed/contour.rs:696 ../../..
src/fixups/havok/anim_text_data_stance.rs:1281       ../../../extracted/fo4/Meshes
src/fixups/havok/anim_text_data_stance.rs:1662       ../../..
src/fixups/havok/anim_text_data_stance.rs:1950       ../../../mods/B21_AnimText_Snallygaster/data/Meshes/Actors/Snallygaster
src/fixups/havok/anim_text_data_sync.rs:1542         ../../..
```

The following two helpers serve both pure emit tests and conversion-driver tests. Split/copy them with the tests in Task 9: the `ck_native` copy keeps `../../../`; the conversion copy gains one level (`../../../../`) in Task 13:

```text
src/fixups/havok/anim_text_data_emit.rs:1688  ../../../refs/fanmods/Gauss Pistol/Meshes
src/fixups/havok/anim_text_data_emit.rs:1692  ../../../extracted/fo4/Meshes
```

Stays in conversion and reaches repo root; add one parent level in Task 13 (or the equivalent ancestor-count change):

```text
src/relocation.rs:811                              ancestors().nth(3) -> ancestors().nth(4)
src/relocation.rs:864                              ancestors().nth(3) -> ancestors().nth(4)
src/texture_engine/corpus_tests.rs:26               ancestors().nth(3) -> ancestors().nth(4)
src/fixups/havok/anim_text_data_emit.rs:1714        ../../../refs/fanmods/Gauss Pistol/Gauss Pistol.esp -> ../../../../refs/fanmods/Gauss Pistol/Gauss Pistol.esp
src/fixups/havok/anim_text_data_emit.rs:1835        ../../.. -> ../../../..
```

Stays in conversion but is crate-local; moving the complete crate preserves these paths, so do not add `../`:

```text
src/modt_compute.rs:238                    src/test_fixtures/modt/{name}.json
src/fnv_legacy_scripting/function_map.rs:47 src/fnv_legacy_scripting/data
src/terrain_textures/nif_refs.rs:135       Cargo.toml
src/phase/copy_textures.rs:520             src/test_fixtures/gamebryo_nifs/cratelarge01.nif
src/test_fixtures/mod.rs:8                 src/conversion/test_fixtures/{name}
src/phase/gamebryo_nifs.rs:571             src/test_fixtures/gamebryo_nifs/cratelarge01.nif
src/phase/gamebryo_nifs.rs:718             src/test_fixtures/gamebryo_nifs/cratelarge01.nif
```

One conversion-side lookup targets the Python package rather than repo-root fixtures. It needs an explicit retarget, not a blind single-level change:

```text
src/fixups/inject_weap_extra_data.rs:88
  current: ../../python/creation_lib/conversion/record/weapon_extra_fks.yaml
  Task 13 interim: ../../../../py_creation_lib/python/creation_lib/conversion/record/weapon_extra_fks.yaml
  after Task 14: ../../python/bacup_lib/record/weapon_extra_fks.yaml
```

The remaining grep hit is the comment at `src/fnv_legacy_scripting/function_map.rs:4`; it stays with conversion and requires no path edit.

### Python `importlib.resources` / `__file__`

The scan found these 34 files. Recheck each lookup after the flattened Python-package move in Task 14:

```text
py_creation_lib/python/creation_lib/conversion/animation/weapon_family_classifier.py
py_creation_lib/python/creation_lib/conversion/behavior/driver_synth.py
py_creation_lib/python/creation_lib/conversion/behavior/templates/_schema.py
py_creation_lib/python/creation_lib/conversion/native_maps.py
py_creation_lib/python/creation_lib/conversion/omod_filter.py
py_creation_lib/python/creation_lib/conversion/regen_pipeline.py
py_creation_lib/python/creation_lib/conversion/tests/conftest.py
py_creation_lib/python/creation_lib/conversion/tests/fixtures/fnv_weapon_e2e/_generate.py
py_creation_lib/python/creation_lib/conversion/tests/fixtures/fo76/_generate.py
py_creation_lib/python/creation_lib/conversion/tests/fixtures/nif/fnv/weapons/_generate_m2_min_nifs.py
py_creation_lib/python/creation_lib/conversion/tests/test_cleanup_reachability.py
py_creation_lib/python/creation_lib/conversion/tests/test_deathclaw_conversion.py
py_creation_lib/python/creation_lib/conversion/tests/test_en02_script_patch_batch.py
py_creation_lib/python/creation_lib/conversion/tests/test_en02_whitespring_access_patch.py
py_creation_lib/python/creation_lib/conversion/tests/test_en07_nuke_script_patches.py
py_creation_lib/python/creation_lib/conversion/tests/test_fissure_script_patches.py
py_creation_lib/python/creation_lib/conversion/tests/test_gaussrifle_conversion.py
py_creation_lib/python/creation_lib/conversion/tests/test_golden_harness.py
py_creation_lib/python/creation_lib/conversion/tests/test_m7_yaml_load_allowlist.py
py_creation_lib/python/creation_lib/conversion/tests/test_msilo_script_patches.py
py_creation_lib/python/creation_lib/conversion/tests/test_native_boundary_rules.py
py_creation_lib/python/creation_lib/conversion/tests/test_native_run.py
py_creation_lib/python/creation_lib/conversion/tests/test_native_translate_all.py
py_creation_lib/python/creation_lib/conversion/tests/test_native_weapon_role.py
py_creation_lib/python/creation_lib/conversion/tests/test_p0_creature_script_patches.py
py_creation_lib/python/creation_lib/conversion/tests/test_package_fragment_script_patches.py
py_creation_lib/python/creation_lib/conversion/tests/test_scorched_script_patches.py
py_creation_lib/python/creation_lib/conversion/tests/test_terminal_fragment_script_patches.py
py_creation_lib/python/creation_lib/conversion/tests/test_terrain_textures_smoke.py
py_creation_lib/python/creation_lib/conversion/tests/test_topicinfo_fragment_script_patches.py
py_creation_lib/python/creation_lib/conversion/tests/test_unified_driver.py
py_creation_lib/python/creation_lib/conversion/tests/test_weapon_block_diff.py
py_creation_lib/python/creation_lib/conversion/workflows/asset_phases.py
py_creation_lib/python/creation_lib/conversion/workflows/unified.py
```

## Task 16 handle-API outcomes

No BACUP Python function accepts a plugin handle created by `creation_lib.esp`.
Numeric plugin handles remain private to the BACUP native module and are only
looked up through a `ConversionRun`.

| Former surface or phase option | Classification | Outcome |
| --- | --- | --- |
| `conversion_collect_eid_rows`, `conversion_collect_eid_rows_from_path` | Generic ESP inspection | Removed. Callers use `creation_lib.esp.Plugin.record_index_rows()`; the public creation API owns its native handle. |
| `conversion_record_refs_by_signature`, `conversion_record_refs_by_form_keys` | Generic ESP inspection | Removed. Signature and FormKey filters are provided by `Plugin.record_index_rows()`. |
| `conversion_plugin_set_snam(handle_id, ...)` | Generic ESP editing / run target mutation | Removed. Standalone reads use `Plugin.header.description`; conversion writes use `conversion_run_set_target_description(run_id, ...)`. |
| `conversion_diagnose_navmesh_links(handle_id)` | Conversion-stateful standalone operation | Now accepts `(plugin_path, game)`, opens an `OwnedPluginHandle` inside BACUP, and closes it on success or error. |
| `conversion_collect_lod_closures(handle_id, ...)` | Conversion-stateful source inspection | Replaced by `conversion_run_collect_lod_closures(run_id, root_form_keys)`, using the run-owned source handle and source directory. |
| `conversion_normalize_placed_records(target_handle_id, ...)` | Conversion-stateful target mutation | Standalone export removed. Placed-record normalization remains inside the run-owned conversion/copy pipeline. |
| `conversion_run_source_handle`, `conversion_run_target_handle` | Registry escape hatch | Removed. `ConversionRun` no longer exposes or caches numeric plugin handles. |
| Script-reference collection and subrecord rewrite through `creation_lib.esp.native_runtime` | Cross-registry target inspection/editing | Replaced by `conversion_run_script_reference_records(run_id, ...)` and `conversion_run_set_record_subrecords(run_id, ...)`. |
| Standalone terrain `source_handle_id`, `target_handle_id`, `record_output_mode` | Conversion-stateful standalone operation | Rejected as legacy options. Standalone terrain requires `source_plugin_path` and opens it locally; run terrain uses the run-owned source and target. |
| `walk.source_handle`, `walk.master_handles` | Run phase override | Rejected. Walk always uses the run-owned source and ordered master handles. |
| `graft_terrain.prior_handle_id` | Run phase auxiliary input | Rejected. `prior_plugin_path` is opened conversion-locally and closed on success or error. |
| `regenerate_modt.output_handle_id`, `regenerate_modt.deployed_esm_handle_id` | Run phase target/auxiliary override | Rejected. MODT edits the run target; optional `deployed_esm_path` is opened conversion-locally and closed after harvesting. |
| Generic source/target worldspace header, terrain ID, TERM, and cell-height operations | Generic ESP inspection/editing | Moved behind public `creation_lib.esp.Plugin` methods; BACUP never passes their handles into its registry. |
