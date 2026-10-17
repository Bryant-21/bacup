# SWF editor workflows and coverage

The editor has three document modes. **Visual** edits flat vector artwork and
explicit frame animation. **Source** edits AS3 buffers and the standard SWF
project manifest. **Binary** keeps a loaded movie opaque and changes selected
tags or compiled classes without rebuilding its other tags.

Open with File → Open or drop a `.swf` / `.swfproj`. File → New AS3 menu provides
a document class, game font setup and character-0 binding. The Document panel
contains AS3 Sources, Structure, ABC Inspector and Build & Preview. Source and
binary movies use that panel and the player; the canvas does not execute AS3.

Save (`Ctrl+S`) writes a project. Export (`Ctrl+E`) builds a separate SWF. Build
(`F5`) uses the current source buffers, without overwriting external `.as`
files. Preview (`Ctrl+F5`) runs a disposable compiled copy in a chosen Ruffle
desktop executable, with player output available in Build & Preview. The
installed player is discovered from PATH. No player is downloaded.

## Checklist derived from the checkout SWF skill

| Skill workflow | Editor coverage | Verification / limits |
|---|---|---|
| Create/open vector icon SWF | Visual canvas, real CWS export, FWS/CWS open | Disposable export/reopen and GPU output tests |
| Select/move/scale/rotate | Move with Select; scale/rotate in Properties; transformed bounds | Gesture and pixel tests; scale/rotate handles are not provided |
| Pen and direct selection | Adaptive cubic-to-quadratic pen conversion; anchor and quadratic control editing | Primitive/path export, cancelled path, exact curve-join tests |
| Rectangles, ellipses and lines | Filled primitives; Shift constraints; grid snap | Exportable and tessellated in tool tests |
| Fill and eyedropper | Bucket, palette/property colors, shared sampled bucket color | Color changes and undo verified; first fill region is edited |
| Layers | Add/delete/duplicate/reorder, visibility/lock/opacity | Included in round-trip projects; alpha verified in GPU output |
| Timeline | Explicit keyframes, holds, blank frames, labels, playback, scrub and onion skin | Hold/remove/label/alpha/blank-tail round trips verified |
| Motion/shape tweens | Unsupported | Export refuses authored tween metadata instead of silently flattening it |
| Path operations | Union/subtract/intersect/exclude; solid Outline Stroke | Boolean placement/history and stroke-outline tests; outlines use rounded joins/caps |
| SVG import/export | Path/rect/circle/ellipse artwork; selected shape SVG; binary shape replacement | Import and replacement tests. SVG group transforms, CSS, complex paints and text are not fully supported by the existing codec |
| Raster import | Threshold tracing through existing Pillow/vtracer pipeline | PNG tested; other readable JPG/BMP/DDS formats use the same pipeline and report decoder errors |
| Shape library | Existing searchable gallery, game/category/source filters and placement | Library placement/undo and existing thumbnail regressions tested. Index rebuilding remains in Settings/CLI |
| Full editing-state projects | Shape geometry, layer state, keyframes, transforms and AS3 buffers | Visual and source projects save/reopen and are CLI-packable |
| AS3 menu compilation | Native compiler, new/attached inline sources, scripts, exports, sprites/stage/art manifest | Actual widget typing, focused save/build shortcuts, compiled classes and export backing verified; syntax errors remain editable/saveable |
| Compiled class reading | Outline, method-filtered disassembly, transitive dependencies | Actual native reports shown in rendered UI; disassembly is not source decompilation |
| Existing-menu AS3 editing | Independent native method recovery, editable method bodies and method-only compilation; supplied-source class replacement/merge remains | Actual HUD frame method recovery/recompilation demonstrated; general class decompilation is unsupported |
| Existing HUD layout | Native compiled-stage pixels, display-list hierarchy, sprite/frame inspection and surgical placement-matrix editing | Stock FO4 HUD authored frame renders with companion fonts and no preview warnings; actual coordinate/pixel dragging, Escape, hidden-viewport interruption, save/reopen and export verified |
| Advanced ABC editing | Class prefix rename and opcode pattern/replacement with one expected match | Real patch/rename/preservation tests. Replacement supports getlocal, getlex, callproperty, callpropvoid and keep; not a complete bytecode assembler |
| Artwork/symbol reuse in menus | Choose donor exports and inject their character closures | Native symbol injection and PNG rendering verified |
| Structure and asset editing | Named top-level tags, sprite tag lists, bindings; raw payload export/replace; vector SVG replacement | Untouched tag bytes verified exact. Bitmap/font/text/sound lack typed editing panels |
| Compiled preview | Ruffle desktop playback; exported artwork symbol → native PNG | AS3 constructor executed twice; player restart/close cleanup tested. Symbol rendering rejects unsupported content |
| Game input/native callbacks/deploy | Author AS3 hooks; inspect compiled output | FO4/F4SE callbacks, font mapping, input routing, item delivery and deployment require separate in-game acceptance |

## File compatibility and preservation

Visual projects emit the ordinary `canvas`, `fps`, `version`, `background`,
`shapes`, `stage`, `scripts`, `exports` schema, plus `editor_scene` for layer
editing. A shape can contain `tag_code` and base64 `tag_data` instead of an SVG
path. `script_sources` maps script names to saved source buffers. The shared
packer in this checkout accepts both additions; existing projects with disk
SVG/AS3 paths retain their behavior. Save As retains source-art lookup without
relocating originals. On 2026-10-03, `build_modkit_cli.bat` rebuilt and refreshed
both the repository and PATH-installed `modkit.exe`; their executable hashes
match and `doctor` reports current packaged sources. Both executables packed
editor artwork and buffered AS3 fixtures, which then saved/reopened/exported
correctly. The editor uses the same updated shared Python packer.

Binary projects contain `editor_mode: "binary"`, embedded `base_movie`, source
buffers and the original path. They are editing sessions. The CLI packer
explicitly refuses them; use the editor's Export, which retains the movie's
opaque tags. Save/reopen retains protection against exporting onto the opened
original SWF. An untouched binary export is identical to the opened bytes.

Movies with scripts, nested sprites, unsupported tags, tint/addition transforms,
clipping, fractional frame rates, nonzero stage origins or DefineShape4 stroke/
winding flags stay in Binary mode. Editing their vector definitions does not
replace their full timelines. FWS and CWS containers are supported. ZWS and GFX
containers need conversion before editing. The existing codec uses integer FPS;
fractional frame-rate authoring is not certified.

The vector canvas approximates gradients and does not show bitmap/text/script
drawing or all Scaleform effects. Use Ruffle or supported native symbol rendering
for broader preview. Ruffle success does not certify Scaleform. The native AS3
compiler is not Flex: vector-literal/coercion and GraphicsPath hazards documented
in `.agents/skills/swf/references/scaleform-menus.md` still apply.

## Failure, cancellation and lifecycle

New/open/drop and app quit protect unsaved state with Save/Discard/Cancel. The
SWF workspace implements the host's `confirm_exit` hook even when inactive and
waits for another editor's modal to resolve. Failed saves retain dirty state and
the destination's old contents. Cancelled dialogs do not perform the requested
transition. Export does not mark source projects saved.

Build/edit jobs receive a document snapshot. Cancel discards their results; a
changed document also discards the previous result. A failed class edit retains
the previous movie. Undo/redo retain geometry and other document state. Mouse
release outside the viewport completes captured gestures; Escape cancels the
current gesture. Preview restart/close cleans its temporary SWF/config/log files.

## Reproduce verification

```powershell
uv run python -m pytest ui/tests/test_swf_editor_workflow.py ui/tests/test_swf_editor_visual.py
uv run python -m ui.tests.verify_swf_editor --output .codex/tmp/swf-editor --view source
uv run python -m ui.tests.verify_swf_editor --output .codex/tmp/swf-editor --view edit
uv run python -m ui.tests.verify_swf_editor --output .codex/tmp/swf-editor --view art
uv run python -m ui.tests.verify_swf_editor --output .codex/tmp/swf-editor --view structure
uv run python -m ui.tests.verify_swf_editor --output .codex/tmp/swf-editor --view inspector
uv run python -m ui.tests.verify_swf_editor --output .codex/tmp/swf-editor --view diagnostics
uv run python -m ui.tests.verify_swf_editor --output .codex/tmp/swf-editor --view combined
uv run python -m ui.tests.verify_swf_preview --output .codex/tmp/swf-editor
uv run --no-sync python -m ui.tests.verify_swf_cli --output .codex/tmp/swf-editor
```

The rendering harness uses the supported ToolkitApp runner, hidden windows and
isolated settings. Its fixtures are generated locally; it does not edit game
SWFs, publish, deploy or update normal application settings. Screenshots and
interaction traces are written to the specified checkout-local directory.

The original combined trace reproduced an ESP discard approval reopening over the
waiting SWF guard. The host now retains per-workspace approvals for one quit
transaction and revalidates the reviewed editor state each frame and before exit.
New edits, replaced documents, or pending jobs invalidate approval and return to
that editor after closing the later quit modal. Cancel and new transactions clear
all approval/UI state. Quit-time Discard retains the live dirty document until
shutdown, so a later workspace's Cancel preserves both editors. New/open discard
still performs its explicitly requested transition. Save failures retain dirty
data; confirmation/reset errors keep the application open.

The original harness now passes at
`.codex/tmp/quit-integration/swf-repro/combined-trace.json`. The stricter real-input
harness uses actual WM_CLOSE events and ImGui mouse input with both ESP and SWF
dirty and Mod Manager active:

```powershell
uv run --no-sync python -m ui.tests.verify_workspace_quit --first esp_editor --output .codex/tmp/quit-integration/esp-first.json
uv run --no-sync python -m ui.tests.verify_workspace_quit --first swf --output .codex/tmp/quit-integration/swf-first.json
uv run --no-sync python -m pytest ui/toolkit/tests/test_workspace_quit_transaction.py ui/toolkit/tests/test_esp_editor_quit.py ui/tests/test_swf_editor_workflow.py -q
```

Both real-input orders passed in 104 frames. They cover Discard then Cancel,
repeated closes, save failures with unchanged destination bytes, edits after
Discard and Save approvals, successful final saves, and exact ESP/SWF reopening.
The focused run passed 44 tests. The prior combined file set plus transaction
regressions passed 442 tests; XML evidence is in
`.codex/tmp/quit-integration/{focused-tests.xml,combined-tests.xml}`. The old
blocked trace is retained as before-fix evidence.

SWF verification before the shared quit integration on 2026-10-03: 414 combined SWF, ESP, toolkit and mod-manager
tests passed. Ruff and scoped `git diff --check` passed. Actual ESP navigation,
native window quit with mod manager active, mod-manager empty startup, SWF
source typing/focused shortcuts and seven SWF views passed. Ruffle executed the
compiled constructor twice and repeated close removed both preview directories.
Evidence lives in `.codex/tmp/swf-editor/`: `combined-tests.xml`, `edit-trace.json`,
`esp-navigation.json`, `esp-quit-mod-manager.json`, `preview-evidence.json`,
panel PNGs and the explicitly blocked `combined-trace.json`.

The normal CLI rebuild required no new setup. Previous repository and installed
packages remain under the path recorded in `cli-rebuild-backup-info.json`.
`cli-build.log` records the build; `cli-rebuild-evidence.json` records both
executables' hashes, freshness, pack/inspect/class checks, save/reopen/export,
binary-session refusal and unchanged fixture originals/external AS3 files.

## Prioritized remaining capability gaps

| Priority | Capability | What remains |
|---|---|---|
| P0 | General existing-class source recovery | **Backend work.** Independent native method recovery and recompilation now support a conservative AVM2 subset. Complete classes, exceptions, arbitrary control flow, runtime/custom namespaces and complete compiler semantics remain unsupported; extend the project-owned backend |
| P1 | Typed bitmap, embedded-font, text and sound asset editing | **Missing authoring APIs/codecs.** Native symbol rendering can decode supported bitmaps; there are no exposed typed asset round-trip editing APIs. Build those and reference validation before typed panels |
| P1 | General nested binary timeline authoring | **Backend/editor-model work.** Authored display-list inspection, cumulative frames, placement transforms, clip-mask preview and Shape4 winding rules are available. Add/remove/reorder operations, typed button states, filters, morph interpolation and all stroke flags remain unsupported |
| P1 | Game-accurate font/input/F4SE behavior | **Runtime acceptance.** Use disposable test mods and the existing game-control workflow. Ruffle and static checks cannot certify Scaleform callbacks or input routing |
| P2 | Graphical source-project stage/sprites/class bindings, renamed donor symbols | **Feasible UI exposure.** Project packing and `inject_symbols_renamed` already exist. Add structured forms and dependency previews instead of requiring manifest/complete-JSON edits |
| P2 | Multi-region fills, gradient/stroke widgets, transform handles, diagnostic navigation | **Mostly UI work with existing data/compiler output.** The canvas also needs accurate gradient paint evaluation; it currently approximates gradients. AS3 coloring uses the bundled C++ lexer and lacks language intelligence |
| P2 | Motion/shape tweens | **Editor/model work.** Motion can be interpolated and baked into existing explicit-frame packing. Shape tweening also needs contour/topology correspondence; it cannot be exposed as a checkbox alone |
| P3 | ZWS/GFX, richer SVG and complete AS3/bytecode compatibility | **Backend/tooling work.** Container conversion, SVG transforms/CSS/paints/text, compiler language/coercion coverage and bytecode assembler support need implementation or an explicit external tool choice |

No external decompiler, alternate compiler or conversion tool was installed.
FWS/CWS editing with known source and supported native method recovery are
available; universal SWF/AS3 editing remains an explicit roadmap goal.

## Library replacement (2026-10-04)

The gallery now searches **all** indexed assets. The old 200-row cutoff and
post-query User filtering are removed. Game, name/tags/source, category (including
subcategories), Base/User, source-movie and thumbnail-size controls remain.
Search is debounced; category/source changes are immediate. Only visible grid
rows are drawn, with 128-row metadata pages and at most eight resident pages.

Source databases open read-only. A cancellable background job makes a searchable
metadata sidecar once per database revision, without copying geometry/SVG blobs.
Asset identities include the source path, revision and row ID. Replacement,
WAL updates, game changes and late jobs cannot substitute another row's geometry.
Placement checks the source revision again immediately before its undoable commit,
and cancels if the document, active frame/layer or selection changed while loading.

One isolated helper process decodes geometry and uses the existing native vector
renderer for previews; it never executes the imported movie's AS3. Worker startup
and result transport run off the UI thread. Work/result queues are bounded, with
six preview jobs, two texture-upload attempts per frame, at most 128 live textures
and 16 MB of pixel cache. The disk cache uses content and renderer-version hashes;
corrupted PNGs rebuild. Hidden tabs pause requests; cleanup releases each owned
texture once and closes the helper, including an interrupted startup.

**Import artwork** copies SVG, traced PNG/JPG/BMP/DDS or supported vector
definitions from SWFs into a separate `swf_user_shapes.db`, scoped to the game
selected when the import starts. The threshold slider controls bright-on-dark
raster tracing. **Save selected shapes to User library** copies each shape's
definition, rather than its canvas-instance transform/opacity or a composited
selection. Save project preserves those canvas values. Imports are transactional;
cancelled/failed work leaves original files and base indexes intact. Unsupported
definitions are reported, and Retry restarts failed previews/catalog loads.

New index entries and personal assets store authoritative DefineShape tag data.
This preserves alpha, gradients (including focal points), strokes, mid-shape style
tables, shape versions, edge bounds and Shape4 flags. Existing legacy indexes remain
usable through their generated SVG, after removing the preview background. SVG
import now rounds absolute endpoints so fractional/cubic paths close without twip
drift, retains opacity and checks cubic error away from the midpoint. Legacy SVG
can already have flattened gradients or omitted strokes: it displays a recovery
warning, and incomplete stroke entries require an explicit Settings > Indexes
rebuild. No existing index or original game SWF was rewritten during verification.

Library reuse supports independent vector geometry. Bitmap-dependent shapes need
their source movie/dependencies through the document workflow. SVG library import
accepts flat filled paths, rectangles, circles and ellipses; expand transforms,
strokes, CSS, text, gradient paints, clipping/masks and unsupported arc/smooth path
commands first. Empty/malformed assets fail visibly. Limits are 16 MB per vector
asset, 64 MB per import and 4,096 SWF definitions. This replacement does not add a
decompiler, typed font/sound/bitmap editing, nested movie authoring or exact canvas
gradient paint evaluation. Native previews preserve supported paints; the canvas
still approximates gradients. Frozen toolkit-worker startup and in-game behavior
have not been certified by these source-run native-window tests.

Measured evidence is under `.codex/tmp/swf-library/`:

| Check | Before | After |
|---|---|---|
| Eight searches, 25,000-asset fixture (hyperfine, five runs; process startup included) | 2.194 s mean | 0.239 s mean |
| Eight searches, existing 7,149-asset / 520 MB index, read-only | 9.180 s mean | 0.242 s mean |
| Native fixture library work | draw p95 16.25 ms / max 197.17 ms | cold total UI work p95 0.59 ms / max 9.51 ms |
| Native existing-index search/return, including every visible preview | synchronous queries and blobs during draw | total UI work p95 below 0.6 ms / max below 1 ms; 18 rendered previews, no unavailable previews |

The new query timing uses 128 metadata rows per page; the historical query loaded
200 rows with full blobs/SVG. Initial snapshot creation and preview startup remain
asynchronous. Frame timings include background result draining as well as drawing,
not only metadata futures. The rendered sample is not certification of every
indexed asset or every game.

`final-tests.xml` records 107 passing SWF tests. Library cases include paging and
late User rows, literal/punctuation searches, full geometry/save/undo, legacy curves,
SVG/raster/SWF imports, source/WAL replacement, corrupt sidecars/PNG, cancelled
transactions, dead-helper retry, interrupted startup and real GL cache eviction.
`validated-final/interactions.json` records 853 actual ImGui frames: typed search,
scrolling to the last asset, double-click placement, save/reopen/export, game changes,
imports, active-import interruption, invalid-import feedback and hide/reopen. Its peaks were 21
textures/pixel tiles, two pages and six jobs; cleanup released the resources.
`source-regression/edit-trace.json` records native AS3 typing/save/build after the
library integration. Scoped Ruff and diff checks passed.

The normal CLI/native rebuild completed after transient Windows file locks cleared;
the final PATH resource/executable promotion used copies after its active CLI exited,
with no further recursive deletion. No other session was stopped.
`final-cli/cli-rebuild-evidence.json` verifies both the repository
and PATH executables as current, matching SHA-256
`e5b2fbfd069a6b9a8a4556c8758d2f8da08f44ebdc9e8997c23669347b5d81d1`, with artwork and
buffered AS3 pack/save/reopen/export, class checks and opaque-session refusal.
Previous packages remain at the path in `cli-backup-info.json`. Host/adapter quit
handling and the other editors were left to their owning session.

## Compiled HUD workflow checkpoint

Compiled menus now expose their authored display list in Layers, placement matrices
in Properties and root/sprite frames in Timeline. Edits patch only PlaceObject
matrix bytes, recursively updating their containing sprite tag sizes. Unknown
suffix fields, masks, clip actions, ABC, fonts and unrelated definitions remain
untouched. Partial timeline modifications inherit previous fields. A placement
edit persists until the next matrix key/removal; editing a sprite's contents changes
its shared definition and the Properties panel explains that scope.

Rendering uses the project-owned native stage backend and existing SWF/tiny-skia
dependencies. It handles nested timelines, vector paints, clip masks, button up
states and embedded/imported font outlines, including the HUD's font-class aliases
and supported HTML font/paragraph formatting. Clip masks use stencil fills without
paint alpha or line strokes, and Shape4 winding flags are respected.
The companion fonts_en.swf is read
from the selected game's extracted Interface folder through toolkit settings.
The authored stock FO4 HUD frame now renders with no preview warnings. Filters,
non-normal blends and mixed HTML runs are explicitly limited; scripts are not run.

ABC Inspector can recover a supported existing method into AS3 Sources. Build/Export
compiles its body with the original signature and transplants only that method,
preserving its ID, accessor/visibility namespace, constructors and other methods.
Getter/call/property/return and supported structured conditional/loop recovery are
independently implemented. Constructors/statics, exception handlers, custom or
foreign nonpublic namespaces, generic/runtime names, complex branches, coercion
opcodes and interleaved stack expressions fail visibly. Recovery does not guess
source when evaluation order or namespace meaning cannot be retained. Compilation
rejects ambiguous member namespaces and nonpublic receivers it cannot prove,
including another object's public property sharing a private member's name. Actual
stock HUD MeterBar.frame1 and MeterBarWidget's Percent getter recover; replacing
stop() with gotoAndStop(2) has been demonstrated in compiled bytecode.
JPEXS is excluded from the product backend; no external decompiler was installed,
downloaded, invoked, bundled or copied into the implementation.

Evidence under `.codex/tmp/swf-stage/`: `native-final-tests.xml` records 126 passing
SWF backend/UI tests (one existing PyGLM deprecation warning). Two Rust tests pass
for namespace and evaluation-order rejection. `native-final/interactions.json`
and its PNG record 181 real native frames. They cover a
disposable stock FO4 HUD copy, hierarchy selection, coordinate and pixel dragging,
exact preservation of other tags/ABC, undo/redo, save/reopen/export, unsaved Cancel,
Escape and hidden-viewport interruption, and actual source-widget method edits.
`native-final/preservation.json` confirms all 198 original class outlines and
1,056 other method reports remain unchanged after editing MeterBar.frame1.
`native-final/method-preview.json` records original and edited instance/getter
methods executing with expected values in three disposable Ruffle previews,
including repeated close cleanup. Ruffle automatically fetched its OpenH264 codec
into each disposable cache; those directories were removed. No JPEXS was used.
The source HUD SHA-256 remains
`e73d78ad1dc4afc4d5d3fd9469066eda37c56a3872d8df4c877b94e62864d37e`.
The model resolves 291 visible placements and 319 timelines including the root.

The final creation-native build compiled but shared `_native.pyd` promotion was
blocked by toolkit process 61500's Windows file lock. Tests used an isolated copy of
the freshly built DLL through `.codex/tmp/swf-stage/run-current-native.py`.
Run `uv run --no-sync python scripts/ensure_native.py --package creation` after
the existing toolkit/native consumers safely release it. No process was stopped.

AS1/2
source editing, button-record hierarchy, JPEG-only hit bounds, arbitrary tag
authoring, ZWS/GFX input and script-created runtime display objects remain outside
this compiled-stage editor. Bounds/hit tests are authored rectangular bounds;
rendering and live Scaleform behavior remain separate acceptance checks. The
existing Ruffle preview still runs a disposable compiled copy. Shared host and
workspace adapter lifecycle files were left untouched.
