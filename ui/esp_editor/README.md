# ESP editor workflow

Launch the existing modern toolkit shell with `uv run python -m ui.toolkit --variant=esp_editor`.
Open a plugin or folder, expand a record group, and select a record. **Fields** offers schema-backed
values, named enum choices, flags, structured members and array rows. Field search also matches
member names. **Overrides** aligns versions in load order, highlights the winner, filters to
differences, and lets you navigate to a version. The toolbar's copy action asks for a destination.

The pinned **Search by** dropdown selects Editor ID, FormID, Name, Record type, or All indexed
fields. All indexed fields covers Editor ID, file FormID and type; **Name** searches FULL text on
a worker. FormID input accepts an optional `0x` prefix; displayed IDs omit it. **Record type** is a
full-width dropdown. **Sort records by** and **Descending** change display order while plugins
retain session priority. Group headings sort by their friendly labels.

**Overrides overview** provides status/scope/search filters, counts, progress, cancellation,
and separate **Winner** and **Source** navigation. **Analyze load order** reads record-type
inventory, then compares eight types per batch. The color legend explains green `[W]` winners,
red `[L]` overridden conflicts, yellow `[=]` identical overrides, gold `[O?]` unexamined inherited
overrides, purple `[P]` partial forms, and blue `[E]` unsaved changes. A conflict here means payload
bytes differ; inspect the aligned values for the meaningful differences.

Record rows/search use **file FormIDs**. Record details and row tooltips separately show the
containing plugin, original owner, file FormID, and projected session FormID. Plugin headings use
zero-based session slots (`00`, `FE:000`, or Starfield `FD:00`), while their tooltip labels the
one-based list position separately. Slots follow this editor's order; import the game's load order
before interpreting them as engine IDs. Header flags determine light/medium slots, not extensions.
An out-of-range light/medium object ID remains unresolved rather than being truncated. Owners
and slots remain unresolved until their metadata is available. Row IDs precede Editor IDs so
they stay visible in narrow panes.

A list position of 9 can correspond to slot `08`, so `089399CE` alone does not prove an ID bug.
Its high byte can instead index the containing file's master table and name a different owner.
The exact reported example was absent from all nine UI-configured FO4 ESMs when checked through
`modkit`; the containing plugin filename is needed to determine which case applies.

The normal navigation, detail, comparison, edit and save paths do **not** serialize records to JSON.
Navigation uses native record summaries. Background detail jobs fetch only selected records through
`plugin_handle_record_subrecords`; schema dataclasses drive the widgets. Strict encoding sends
subrecord tuples back through `plugin_handle_set_record_subrecords`. JSON is optional in the
**Source JSON** tab and explicit export commands.

## Findings addressed

- Full native JSON export and wrapped-text layout ran every record-view frame; field widgets were
  disconnected and edit/undo functions disabled.
- Expanded groups traversed every record every frame, including off-screen rows. Navigation and
  comparison/conflict lists clip visible rows. Sorted lists are cached; filtering/sorting of
  4,096+ summaries and all Name searches run on a worker and reject stale completion. Native group skeleton
  counts omitted nested records; plugin totals now use the real native record count, and group
  counts appear after materialization.
- The first category click collapsed when its asynchronous record count changed the ImGui label
  and therefore its ID. Stable `###` identities now survive loading, active markers and edits.
  Group load failures and search failures show explicit retry actions rather than empty results.
- Recursive master loading appended required masters after their dependent. Masters now precede it,
  with sibling plugins resolved before configured game paths.
- Raw IDs were compared across different master tables, and native summary lookup could fall back
  to an unrelated owned record with the same object ID. Scoped resolution and comparison validate
  stable origin identities, preserve repeated subrecords, and skip partial forms when choosing winners.
- Different override chains could share a winner's raw ID and overwrite each other in a Python
  dictionary. The UI now uses a plugin-scoped conflict index.
- Copy operations ignored the selected source handle. Explicit source copying now preserves the
  selected version, including when a later winner exists.
- Background scans/details/tree jobs could outlive state changes. Invalidated/cancelled results are
  discarded; closed handles are removed from caches. Shutdown drains workers before closing handles.
- Permissive inspection encoders could keep old bytes on overflow or truncate text. Editing rejects
  invalid values and preserves opaque tails. Failed writes/backup creation retain dirty state.
- Save popup requests crossed ImGui ID scopes. Popups now open in their drawing scope. Plugin close
  offers Save and close, Discard and close, or Cancel. Save As updates native and session identities
  and rejects a renamed master still required by another loaded plugin.
- Whole-application Quit, window X/Alt+F4 and setup relaunch now go through a cancellable transaction
  before shutdown. All dirty plugins participate even if the ESP workspace is inactive. Pending
  mutations finish before Save all or Discard; failed/partial saves keep the application open and
  preserve failed plugins' dirty state. Repeated quit requests reuse the prompt.
- Shared quit approvals persist per workspace for one transaction, including when
  both ESP and SWF are dirty. Edit/job state changes invalidate approval; Cancel
  clears the transaction. Discard retains live edits until shutdown so another
  workspace can still cancel. Mod Manager routing and registration are preserved.
- Native full scans delayed selected-record reads by 1.38 seconds in a real FO4 profile. Sessions
  above 100,000 records now scan globally on request. Per-record Overrides remains available,
  and unscanned counts say Not scanned rather than displaying misleading zeroes.
- Incremental overview analysis rejects cancelled or stale batches; edits invalidate completion.
  Large overview lists also filter/sort on a worker, with cached totals and clipped table rows.
  Starfield's existing small/update/medium header labels used the wrong masks; these now agree
  with the supplied xEdit reference (`100`/`200`/`400` hexadecimal).

## Verification

Run the focused tests from the repository root:

```powershell
uv run --no-sync python -m pytest ui/tests/test_esp_editor_navigation.py ui/tests/test_esp_editor_workflow.py ui/tests/test_esp_editor_load_behavior.py ui/tests/test_esp_editor_display_helpers.py ui/tests/test_esp_editor_app_validation.py ui/tests/test_esp_editor_export.py ui/toolkit/tests/test_esp_editor_workspace_menu.py ui/toolkit/tests/test_file_drop.py ui/toolkit/tests/test_esp_editor_quit.py ui/toolkit/tests/test_variants.py ui/toolkit/tests/test_launch_workspace_restore.py py_creation_lib/python/creation_lib/esp/editor/tests py_creation_lib/python/creation_lib/esp/tests/test_set_masters.py -q --tb=short --maxfail=0
```

The workflow tests use disposable native plugins, covering edit/undo/redo/save/reopen, enum edits,
flags, opaque payload preservation, invalid values, selected-source copying, master ordering,
identity collisions, partial forms, repeated selection, cancellation, stale scans/tree results,
backup/write failure, close/quit failure, stale filtering, localized/conditional edit guards and
bounded navigation work. No user plugin or game data is used in tests.
The latest combined run passed **136 tests**, with **one failing SWF test** and one pre-existing
PyGLM deprecation warning, in 1.76 seconds. A final metadata-guard regression was added afterwards;
the navigation suite then passed all **17 tests in 0.40 seconds**. The failing shared test is
`test_file_drop.py::test_swf_editor_workspace_drop_opens_swf`: it expects `open_swf`, while the
concurrently edited SWF workspace calls `open_document`. SWF files/tests were left untouched.
All ESP tests passed. Scoped Ruff and `git diff --check` passed.

The subsequent shared quit integration run passed **442 combined tests**, including
SWF and Mod Manager tests; the earlier SWF drop expectation now passes in its
owner's updated implementation. **44 focused quit/SWF tests** passed. Actual dirty
ESP+SWF quit passed in both orders with Mod Manager active, using native WM_CLOSE
events and real mouse input. The 104-frame runs cover repeated close, Cancel,
save failure, unchanged failed-save destinations, new edits after Discard and Save
approval, and successful final save/reopen. See
`.codex/tmp/quit-integration/{esp-first.json,swf-first.json,combined-tests.xml}`.

Render the actual workspace with isolated settings and synthetic plugins:

```powershell
uv run --no-sync python -m ui.tests.render_esp_editor --view fields --output .codex/tmp/esp-editor-check/fields.png
uv run --no-sync python -m ui.tests.render_esp_editor --view overrides --output .codex/tmp/esp-editor-check/overrides.png
uv run --no-sync python -m ui.tests.render_esp_editor --view save --output .codex/tmp/esp-editor-check/save.png
uv run --no-sync python -m ui.tests.render_esp_editor --view close --output .codex/tmp/esp-editor-check/close.png
uv run --no-sync python -m ui.tests.render_esp_editor --view quit --output .codex/tmp/esp-editor-check/quit.png
uv run --no-sync python -m ui.tests.verify_esp_editor_quit --with-mod-manager --output .codex/tmp/esp-editor-check/native-quit.json
uv run --no-sync python -m ui.tests.verify_esp_editor_navigation --output .codex/tmp/esp-editor-check/navigation-ui.json
uv run --no-sync python -m ui.tests.verify_workspace_quit --first esp_editor --output .codex/tmp/quit-integration/esp-first.json
uv run --no-sync python -m ui.tests.verify_workspace_quit --first swf --output .codex/tmp/quit-integration/swf-first.json
```

The navigation runner injects actual ImGui mouse and keyboard events into the modern ToolkitApp,
using isolated settings and disposable plugins. Its approximately 358-frame trace verifies the first click while
the async count is pending, repeated expansion/selection, every search mode, prefixed input,
empty results, sort direction, type filtering, the color legend, analysis cancel/retry, overview
status/scope filters, and source/winner navigation. The final screenshot is beside the JSON trace.
The runner asserts that these clean workflows leave no application error; a native header-master
string/pair mismatch caught during screenshot review is fixed and covered by a native fixture.
The original category collapse was reproduced before the fix in `category-collapse-before.json`.

Synthetic navigation benchmark, 100,000 records, 120 frames, 30 visible rows:

```powershell
hyperfine --warmup 1 --runs 3 --export-json .codex/tmp/esp-editor-check/navigation-benchmark-ux.json 'uv run --no-sync python -m ui.tests.benchmark_esp_navigation --mode unclipped' 'uv run --no-sync python -m ui.tests.benchmark_esp_navigation --mode clipped'
```

Measured locally: unclipped 2.189 s +/- 0.020 s; clipped 0.463 s +/- 0.010 s (4.73x).
The benchmark includes Python startup and synthetic record construction, reproduces the old
group traversal with the original empty-filter matcher, and stubs leaf drawing/visibility.
The new mode includes the initial deterministic sort. It excludes GPU rendering, native I/O and
production load orders. Filter calls fall from 12,000,000 to zero on this empty-query workload;
leaf calls fall from 12,000,000 to 3,600. These numbers show bounded traversal, not rendered FPS
or an end-to-end load-time guarantee.

## Read-only real profile

`ui.tests.profile_esp_editor` reads explicitly named plugins from the UI-configured game path,
times the current native/editor APIs, records process memory and a Python heartbeat, and checks
SHA-256, size and modification time afterwards. It never saves plugins/settings or builds indexes.
Only its report is written inside this repository.

```powershell
hyperfine --warmup 0 --runs 1 --export-json .codex/tmp/esp-editor-check/real-profile-ux-command.json 'uv run --no-sync python -m ui.tests.profile_esp_editor --game fo4 --plugin Fallout4.esm --plugin DLCRobot.esm --plugin DLCworkshop01.esm --plugin DLCCoast.esm --plugin DLCworkshop02.esm --plugin DLCworkshop03.esm --plugin DLCNukaWorld.esm --plugin XDI.esm --plugin WorkshopFramework.esm --output .codex/tmp/esp-editor-check/real-load-order-ux.json' --show-output
```

The local representative order contains nine real plugins and **2,193,348 records**. This is a
single process-level measurement with warm OS file caches, not a multi-run performance guarantee.
Initial load took 0.876 s; group skeletons/counts 0.053 s. Twenty heterogeneous selected-record
reads had median 0.634 ms; the first included cold schema import and took 721 ms. The existing UI
preloads schema while loading plugins. Cached same-record selection 120 times took 0.016 ms.

Fallout4.esm WRLD exposes 805,185 summaries: initial materialization still took 617 ms. Initial
sort submission took 0.115 ms and the worker finished in another 148 ms. Retrieving the cached
sorted list 120 times took 0.077 ms total. A text filter request took 0.078 ms and its worker another
242 ms. An explicitly requested full scan took 1.375 s and delayed a concurrent uncached selection
by 1.349 s; automatic scanning was correctly deferred.

Incremental analysis completed 18 batches in 2.076 s, with exactly the same 2,337 scoped chains,
statuses and winning handles as the full scan. Selection during the first batch took 39 ms, but
the REFR-heavy batch still took 935 ms. Cancellation took 0.010 ms to stop queued work; a running
native read must finish. This was an inventory-stage cancellation; batch cancellation/stale
results are also covered by fixtures. The first overview filter of 2,337 chains took 7.1 ms;
120 cached accesses took 0.065 ms. RSS after load was 2.70 GB, process peak 4.57 GB, and the largest
5 ms heartbeat gap was 350 ms. These are API/CPU observations, not rendered frame times. Timing
of a worker's remaining wait does not include work completed between the API call and that wait.
All source fingerprints were unchanged.

Sampled real FULL/DESC, VMAD and CTDA fields were read only. Malformed localized payloads and
selector-only conditional layouts now retain the same guard, including after rejected edits.
The native WM_CLOSE exercise passed through the real runner, switched to Mod Manager, cancelled
repeated closes, blocked a partial save failure, then saved and quit. Reopened fixture values were
5,678 and 1,234; cleanup happened only after all saves succeeded.

## Limits and next work

- Complex conditional, nested variable and count-dependent layouts remain read only. Localized
  string IDs resolve for inspection but require a string-table editor for changes. Count-coupled
  arrays permit supported existing-row edits while resizing is restricted.
- Plugins still use eager loading. Native scans hold a shared lock, compare payload bytes,
  and cannot yet be interrupted in Rust; Python discards stale results and stops queued batches.
  Batching improves progress/cancellation granularity, but it is slower than one full scan and
  large individual signatures still contend with selected reads. Initial huge-group summary
  materialization and its Python conversion still cause measurable pauses and memory growth.
  Reducing these needs bounded native summary queries/pagination or revised native storage.
  Selected-record comparison normalizes FormID references for readable semantic comparison.
- The representative FO4 order and synthetic lifecycle workflows are validated. All six games,
  heavily modded orders, dedicated localized editing, and arbitrary specialized layouts remain
  unrun or unsupported. Forced process termination/crashes are outside the normal quit lifecycle.
- The compatibility `ConflictScan.by_form_id` view remains lossy for colliding raw IDs. New consumers
  should use `reports`, `by_record_key`, or `report_for(handle, raw_form_id)`.
- No native rebuild was required: these changes use existing supported bindings. No push, publish,
  deployment, Plugins.txt edit, or game Data write is part of this work.

## Changed files

- `ui/esp_editor/{app.py,workspace.py,record_view.py,navigation.py,README.md}`
- `ui/toolkit/app.py` (quit lifecycle only; preserve the concurrent Mod Manager menu addition)
- `py_creation_lib/python/creation_lib/esp/editor/{session.py,fields.py,conflicts.py,override.py}`
- `ui/tests/{test_esp_editor_workflow.py,test_esp_editor_navigation.py,test_esp_editor_display_helpers.py,render_esp_editor.py,verify_esp_editor_navigation.py,verify_esp_editor_quit.py,profile_esp_editor.py,benchmark_esp_navigation.py}`
- `ui/toolkit/tests/test_esp_editor_quit.py`
- Shared quit integration: `ui/toolkit/workspaces/swf_editor_workspace.py`,
  `ui/swf_editor/commands.py`, `ui/toolkit/tests/test_workspace_quit_transaction.py`,
  `ui/tests/verify_workspace_quit.py`, and lifecycle notes in `ui/swf_editor/WORKFLOWS.md`.
