# Local manager controls and tested boundaries

The new controls use the existing ImGui theme, controller workers, ToolkitSettings and local
SQLite library. All source references remain read-only. No shared navigation/styling or
concurrent ESM editor/session service was changed in this continuation.

## Columns and command bindings

Drag a header edge to resize or drag a header onto another to place it before that column.
The mod hierarchy remains a clipped TreeNode list. Custom widths/orders scroll horizontally;
default geometry adapts to narrow panes. Settings > Configure mod columns offers bounded
width sliders, Move up/down, Reset, Save and Cancel. Enabled, priority and name are required;
the existing visibility preferences control optional fields. Widths are unscaled UI units.
Column order changes presentation, never install priority.

Settings > Configure shortcuts edits 24 global/mod/plugin commands. Empty disables a command.
Ctrl/Shift/Alt, A–Z, digits, F1–F24 and named navigation keys are accepted. Collisions are
rejected within a pane and against global commands; separate Mod and Plugin panes can share
bindings. Alt+F4 is reserved. Global commands need Ctrl/Alt or a function key, except dismissal.
Text inputs retain editing chords; only the conventional Ctrl+F search override operates
inside active text editing. Standard arrows, letter navigation and mouse gestures remain
fixed. Invalid persisted settings recover defaults. Draft Save rejects later preference changes.

## Source metadata observation

Read-only observation watches referenced meta.ini files and imported categories.dat files.
The first observation establishes a baseline; it cannot identify edits made before that
baseline. SHA256 detects same-size/same-timestamp content changes. Notices survive restart
and repeated imports. Missing/unavailable files are explicit; dismissing a notice rechecks its
exact observation and current source association. A selected referenced mod is prioritized.

Workers rotate 128 files about every two idle seconds, plus the selected file if outside that
batch. Each file is bounded to 1 MiB, and a batch stops after reaching a 4 MiB read threshold
(one final file can bring it below 5 MiB). Up to 100,000 files are watched and 200 notices listed;
the UI shows total pending notices and progress since refresh. A 50,000-file cycle needs at
least about 13 minutes at that cadence, before I/O/worker delays. Busy jobs/runtimes pause
observation. Refresh requests another batch rather than claiming the entire library is fresh.

The notice button opens selective field review or guarded dismissal. Source edits invalidate
an open review. Review starts with fields unchecked; Apply retains unchecked local fields.
Notices do not auto-import values or automatically acknowledge a partial field refresh.
Category notices review the selected reference with the explicit mapping file. Binary Qt
QVariant/typed values remain unavailable. This is a source notice panel, not a general durable
notification history for all app events. Source cache observation changes only library metadata.

## Plugin locks and owned files

Draft block drag, keyboard moves and exact-priority editing shift free plugins around absolute
locks, preserving free order and fixed core slots. An insertion onto a lock chooses a free slot
in the move direction, falling back to the opposite side when necessary. Selected locked/core
plugins cannot be moved. Absent-plugin locks reserve no slot and remain metadata. Dependency,
master grouping, core activation and index-limit review still gates Save; this defined policy
does not claim every upstream MO2 lock-conflict behavior.

Plugins > select a plugin > Owned plugin file actions offers Review, Apply and Cancel for:

- Rename an inactive owned FO4 plugin, retaining its extension and updating local filename lists.
- Ghost/unghost an inactive owned file; inventory displays its logical name and Ghosted badge.
- Set/clear the master flag of an inspected, unghosted, non-light FO4 ESP via the supported
  modkit CLI, staging output and verifying its header before publication.

Game/core/overwrite files and referenced MO2 files are disabled with a reason. Activation in
any local profile/target, an owned provider enabled in another profile, stale inputs, occupied
destinations, known dependent-master renames/ghosting, and rename sets with matching archive/
INI sidecars are refused. Details must be inspected first. ESL conversion/compaction and
medium/update/localized flags are unsupported here. No record ID is remapped.

Only the plugin is staged/backed up; unrelated mod assets are not copied. A durable journal
coordinates the file move and SQLite metadata commit. Failure/cancel before commit restores
the original file and metadata. A late cancellation/fault after durable commit returns the
committed action; finalization failures explicitly identify committed state and retain recovery
material. Startup completes committed work or rolls back uncommitted
work. Unexpected later content is preserved with an explicit recovery error. Backups remain
under .plugin-actions; guarded Undo is available for the latest action in the UI session, and
the service can restore a retained operation after reopen. Later metadata/file changes block
Undo. A general history browser/pruner for these backups remains open. Changes invalidate local
plugin validation/cache and require a fresh Details/activation review. Target substitutions and
cross-profile dependent/sidecar refactoring require a separate workflow.

## Evidence

Actual input covers header drag/grip resize, persisted preferences and typed shortcut changes;
source notice Review/Cancel/stale invalidation/Dismiss; and
owned ghost Review/Cancel/Apply/Undo/stale/failure. Sizes are 1280×800 and 1600×900 with
100/125/150% inherited text. Pixel review checks the same flows; lists/forms scroll when needed.
Generated plugin tests include a real CLI header write, five process-crash boundaries,
cancel/failure boundaries, stale inputs and unexpected destination preservation.

Latest counts, pixel files, private delivery, performance samples and limitations are recorded
in [PROGRESS.md](PROGRESS.md). FOMOD/runtime parity limits remain in [PARITY_REVIEW.md](PARITY_REVIEW.md)
and [NATIVE_API_MATRIX.md](NATIVE_API_MATRIX.md). No real MO2 source, archive, game, account or
third-party tool was modified or executed in validation.
