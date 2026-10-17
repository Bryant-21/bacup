# Modlist Manager

The CLI and UI import shared services directly from `creation_lib.mod_manager`.
Old UI service modules have been removed. The injected Windows runtime lives in
`py_creation_lib/native/mod_manager_vfs`; the Rust service crate is
`py_creation_lib/native/mod_manager`.

CLI entry points (global output options precede the command):

```powershell
modkit.exe --format json manager sync --mo2 "N:\ModOrganizer\Fallout 4" --library "D:\Libraries\Fallout4"
modkit.exe --format json manager launch --library "D:\Libraries\Fallout4" --profile Default
modkit.exe --format json manager launch --library "D:\Libraries\Fallout4" --profile Default --mcp-port 8923
modkit.exe --format json manager launch --library "D:\Libraries\Fallout4" --profile Default --disable-mcp
modkit.exe --format json manager status --library "D:\Libraries\Fallout4"
modkit.exe --format json manager create-mod --library "D:\Libraries\Fallout4" --profile Default "My loose files"
modkit.exe --format json manager profiles --library "D:\Libraries\Fallout4"
modkit.exe --format json manager mods --library "D:\Libraries\Fallout4" --profile Default
modkit.exe --format json manager plugins --library "D:\Libraries\Fallout4" --profile Default
modkit.exe --format json manager actions
```

`manager install` exposes FOMOD steps, choices and the destination file plan before
`--apply`; `--expect <review_token>` rejects changes to the package, environment or
owned replacement since review. ZIP/7z/RAR/folders use the same installer as the UI.
`--target` supplies target-specific dependencies; `--overlay` creates a reviewed additive
update above a reference. `--adjustments` accepts the same source repairs, destination
mapping and added sources as the UI. `--policy mo2|schema` selects the tested FOMOD policy.
Omitting `--name` uses the package name, or keeps the existing name for a reinstall.
`profiles`, `mods`, `plugins`, `files`, `archives`, `saves`, `targets`, `executables`,
`downloads` and `updates` cover the common workflows. `manager workflow --file request.json`
runs the advanced actions listed by `manager actions`, including INI editing, profile
recipes/backups, source relinking, category editing/assignment and recovery. Steps are
JSON objects with `action`, positional `args`, and keyword `options`. Mutating workflows
require `--apply`; each mutation is its own transaction, so a later failure does not
undo earlier completed steps. The native allowlist excludes controller internals.

The Archives pane previews a selected member from any listed provider: bounded text,
binary hex, DDS and common images. The CLI equivalent is `manager archives --library ...
--profile ... --archive <path-or-unique-name> --member <path>`. Use `--target` to inspect
another launch target's game folder. Member decoding is limited
to 8 MiB, image dimensions to 16 MP and display size to 1024 pixels. No extracted files
are written to the game. Unknown archive precedence stays explicitly unresolved.

Nexus category import reads the complete catalog for the selected game, preserves its
hierarchy and caches it for offline use. Local category names and assignments survive
reimport. Use Settings → Categories or `manager categories --library ... --game fallout4
--refresh --import` after signing in.

Nexus personal API keys can be saved in Windows Credential Manager. The UI reconnects
once at startup; automatic version checks default on, respecting a previously saved
off preference. `manager nexus login` uses a hidden prompt, and `manager nexus logout`
removes the saved key. Browser SSO is unavailable until Nexus issues ModBox21 an application
reference. No borrowed application identity or plaintext key file is used.

Automatic checks process 20 distinct identities per batch with a minimum one-minute
pause. Per-identity results, timestamps, sweep progress and rate-limit backoff survive
restarts. The configured interval applies to each completed identity, so a large profile
does not wait six hours between successive batches. `manager updates --library ...
--profile ... --check` processes the next due batch using that same schedule; `--files`
includes file replacements and account state. Reads do not install updates automatically.

Library and Settings are top-level dock tabs. The library uses a compact split view:
profile, search and mod priorities on the left;
executable selection and Plugins/Archives/Data/Saves/Downloads on the right. **Add mod**
and a mod's right-click menu offer **Create empty mod below**. Creation makes a real,
empty owned folder, inserts it below the selected mod in the same group, and uses the
same recoverable Rust transaction in the UI and `manager create-mod` (`--after <mod-id>`).
Right-click **Open mod folder** to add files. Settings separates appearance, library,
Nexus and runtime configuration into tabs; **Launch options** holds executable editing,
FO4 folder selection and the persistent **Keep MCP disabled** option.

Plugins shows priority, filename, effective ESM/ESP/ESL type, load index and provider.
Drag headers in either list to reorder columns, and drag dividers to resize them.
Right-click a plugin header to choose visible columns. Mod column preferences are
saved with workspace settings; plugin columns are saved with the toolkit layout.
The Row height preference sets the minimum height for both lists.
Headers load when opening the pane; unreadable headers remain visible with notices.
Drag rows, use Up/Down, or double-click a priority to change order. Moves keep known
masters before dependents, including disabled plugins. **Library order** starts with
provider priority and resolves dependencies without LOOT. Existing saved/imported orders
remain explicit; a profile without a saved order gets the dependency-aware default.
**Save load order** validates and saves; **Revert** discards the draft. Locks that conflict
with library order must be cleared explicitly. Missing/unknown masters still block saving
an invalid active order.

An existing selected F4SE loader supplies its game directory at launch. It need not match
the managed-download catalog or have readable version resources; it remains responsible
for runtime compatibility. Managed F4SE packages retain strict receipt/version checks.
Each launch filters its private plugin/order snapshots to the selected installation's
Data files, enabled virtual mod providers and owned overwrite. Unavailable names are
omitted without changing the saved profile or game-folder files.

The source CLI entrypoint is `uv run --no-sync python -m cli.main`.
Each project-runtime launch owns a separate VFS and
`runtime/sessions/<id>/output` tree for profile INI overrides, plugin snapshots,
overwrite, temp and game-root writes. Normal launches use the entire normal game
Documents folder for saves, logs and other game files. **Launch options → Isolate
Documents** optionally routes these to the session's `output/ini`, `output/saves`
and `output/f4se-logs` folders. The preference is saved per profile for both CLI
and UI. `manager launch --isolate-documents` / `--shared-documents` overrides one
launch; `manager launch-options --library ... --profile ... --isolate-documents`
saves the preference. Isolated launches copy baseline saves into their session;
installed mod payloads remain shared read-only inputs. Completed output is retained.
Nothing automatically merges competing isolated sessions into the baseline.
Use `manager import-saves --library ... --profile ... --folder <session>/output/saves --latest`
after the sessions have stopped to carry a selected save forward.

The UI can launch again while sessions run and shows their endpoints. Library
mutations remain disabled until all sessions stop. Shared library leases permit
concurrent runtime readers while excluding payload transactions. The project
runtime scopes Fallout 4 window lookup to avoid its startup second-instance
check; unrelated application windows and the process's own windows stay visible.
Two simultaneous FO4 1.11.240 / F4SE 0.7.9 instances reached their menus and
answered MCP requests independently on ports 8922 and 8923. The shared Documents
save inventory returned 196 existing saves. Session write isolation, inherited child
mounts and process-tree exit reporting also pass the native helper probes. Loading
and playing those saves was not part of this check.
The legacy USVFS adapter does not include this window-lookup hook.

MCP ports are allocated from 8922–9021 unless explicitly selected. A helper keeps
a port lease until its process tree exits. An existing external listener is
skipped, but another application can still claim a port between the availability
probe and plugin bind. The plugin reports bind failure instead of switching to an
unreported port. `B21_MCP_PORT` is inherited by the game; it changes neither the
shared INI nor the plugin's `Enabled`/`AllowMutations` settings.

Use **Keep MCP disabled** in the UI (saved with workspace settings), or pass
`--disable-mcp` to the CLI. Disabled launches allocate no MCP port and set
`B21_MCP_DISABLED=1` in the game environment. The plugin honors that override
before reading its INI, so `Enabled=1` cannot turn it back on for that process.
Status reports `mcp_disabled: true` and no endpoint. This requires the updated
B21_MCP plugin; the build is not automatically deployed to installed games.

The shared business layer is Rust in `py_creation_lib/native/mod_manager`.
It owns library/profile/target policies, installation and recovery transactions,
MO2 imports, archives and conflicts, plugin state, save inspection and transfer,
Nexus requests and downloads, controller workers, and runtime launch/supervision.
The CLI and UI both call it through `creation_lib.mod_manager`. Python in that
package supplies bindings, data objects, serialization, and foreign API signatures;
the UI service copies and compatibility re-exports have been removed.

An independent native ImGui workspace using the toolkit's existing theme, docking and settings.
In the existing toolkit, choose **Workspace > Modlist Manager**. The authoring workspace
is **Mod Builder**. Launch this feature alone with
`uv run --no-sync python -m ui.toolkit --variant=mod_manager`; its first run goes directly
to game/library setup without completing the shared authoring wizard. Startup ZIP/7z/RAR/FOMOD
paths and a single archive/folder drop queue package preparation and installer review.
This is defined local functionality, not full MO2 parity or full FOMOD certification.

[Installer coverage](INSTALLER_COVERAGE.md) now includes bounded archive dependency context,
requested FO4/F4SE discovery, explicit source binding/retry, header decoration, owned additive
updates above references and inert package previews, with specific remaining limits.

Ordinary selected F4SE loaders use their containing validated FO4 installation; managed
staged loaders keep their explicit target association. Local profiles and target-private
state are used directly by the native helper. **Launch options > Edit executables** offers an
atomic draft list/form with order/default selection and Save/Cancel. The main workspace
inherits app fonts, wraps controls and keeps readable pane tabs. Local loose/applicable
archive status scans run automatically in a separate worker with qualified partial/error
states. Latest behavior, actual-input captures and limits are in [PROGRESS.md](PROGRESS.md).

Persisted column order/widths and command bindings, read-only source notices, locked-priority
shifting, recoverable owned plugin file actions are documented in
[CUSTOMIZATION.md](CUSTOMIZATION.md), including their exact restrictions and baseline/cadence.

Validated services (2026-10-05): **1,416 tests /465.80s**, followed by final focused checks
recorded in PROGRESS.md; shared routing/restore/quit
passes **33 checks /1.52s**. Original project-owned native interception passes **23 checks**,
plus **37 managed mutation checks**; full helper/profile/lifecycle integration passes
**18 checks** in source and private frozen execution against generated files. Persistent
whiteouts, owned rename/delete and atomic saves use the policy in
[NATIVE_API_MATRIX.md](NATIVE_API_MATRIX.md). Private onedir GUI startup/restoration/normal close
passes; the latest packaged pixel capture is unavailable on this desktop. Integrated source
native pixels/input are verified; exact evidence limits are in PROGRESS.md.
A disposable editor/generator API corpus adds 68 assertions and a crash/
restart in both source and copied frozen helpers; arbitrary third-party tools, clean-machine
and real-game certification remain open.
The latest actual-input ImGui results and benchmark limits are in PROGRESS.md.
Complete runs, native screenshots, selected-target 50k evidence and limitations are recorded
in [PROGRESS.md](PROGRESS.md). Target/PE/provisioning services have 55 cases; root setup has
31, including partial-copy recovery.
The normal Workspace menu and standalone first-run entrypoint both open this class. Project-owned native
local profiles, reviewed import/install/recovery, clipped organization, profile recipes, combined
Data, save comparison/screenshots, bounded metadata adapters and retained-backup cleanup are
implemented. Full feature/UI parity remains open in [PARITY_REVIEW.md](PARITY_REVIEW.md).
The changed-file inventory is [CHANGESET.md](CHANGESET.md); current runs are [PROGRESS.md](PROGRESS.md).
The focused [acceptance review](ACCEPTANCE_REVIEW.md) maps each requested core control to its
actual implementation, records corrected local defects and identifies the precise unfinished
daily-update boundaries. Imported archive install, explicit downloaded-update destinations,
and conflict/update/endorsement/tracking filters are implemented. The overall full feature/UI
parity request remains unfinished.

Selected mod blocks preserve selection during group/priority drag and reject stale database
changes before commit. F2 edits hovered title/priority/category/version/notes/Nexus ID; status
opens Information. Virtual and anchored groups support profile-local descendant rename and
guarded Undo. Plugin-priority F2 edits a frozen validated draft; separate plugin review/apply
persists it. Shared metadata survives repeat imports; priority/group changes remain profile-local.
Changing a Nexus mod ID clears the previous file identity. Reviewed source relinking and
selective source-metadata refresh preserve source files and provide guarded persistent Undo.

**FO4 folders** configures several FO4/F4SE installations for one logical modlist. Each target
keeps game context, plugin state/validation, INIs/backups, saves, output and F4SE logs separate
by default. Reviewed target customization adds a safe launch-option subset, explicit mod
source/version/enable overrides, independent target/shared manager INI and save scopes, and
metadata-only removal/restore retaining all files. Shared priorities and organization remain
logical-profile metadata; automatic mod compatibility selection remains open. The native
filespace protects ordinary mapped writes with first-write copying into owned output;
broad game/tool compatibility and a hostile-code sandbox are not claimed.
The original MO2 sources/executables stay available. Folder-first matching F4SE preparation
uses private staging and official direct/Nexus acquisition, with explicit offline/entitlement
errors and optional cached-archive support; manual archive selection is not required for
authorized automatic acquisition. Prepared targets offer a separately acknowledged,
recoverable standard game-root bootstrap setup/restore; script providers remain private,
and source mods/game Data are not copied by setup. Managed external staging is **experimental** and requires
its own launch acknowledgement; real FO4/F4SE boot is unverified. Scope, exact catalog,
static native-plugin findings and licensing are in [F4SE_TARGETS.md](F4SE_TARGETS.md).
Exact implemented/missing scope is [F4SE_ACCEPTANCE.md](F4SE_ACCEPTANCE.md), with a proposed
approval-only [real-boot plan](F4SE_REAL_BOOT_PLAN.md). Neither runtime route has an actual
FO4/F4SE boot result.

## Standalone workflow and optional import

On first use, choose the game installation and a separate manager library, then
**Create instance**. Detection is read only and runs on explicit request. This creates
a local profile and game/extender executables; installing and organizing mods requires
no MO2 instance. Setup never writes into the game. Repeated setup preserves the existing
instance, profiles and executable edits. FO4 setup reads the selected game's default INI
when present and configures loose-file loading in its local Custom.ini. Preferences and
missing defaults remain minimal and require review before native launch; no global INI is read.
Native modded execution remains scoped to the tested FO4 adapter.

Use **Mods > Import MO2 instance...**, choose an instance directory or ModOrganizer.ini,
then **Import instance**. The automatic destination uses the toolkit app-path convention;
custom destinations and selected-profile preview/import remain advanced options.

**Primary import references mods where MO2 used them. It does not copy, move, hash or
recursively scan mod payloads.** It stores metadata for all discovered source profiles,
safe resolved paths, reverse serialized priority, enabled state, separators, categories,
versions, notes, tags and Nexus IDs. Missing sources remain explicit. Raw INI/plugin/load/
archive/locked-order files are retained. Unmanaged/game-directory entries are game context,
excluded from managed rows and counts, including legacy databases.

Ordinary local mod-root links can resolve automatically after disjoint-path validation.
Network/root/instance-ancestor/game/destination overlaps, chained roots and unsafe metadata
links fail closed or remain unavailable. Advanced mappings must select ordinary local folders.
Nested payload links are checked only during requested Files/install/export/launch scans.
MO2 overwrite output is excluded and never scanned; make it a named source mod if needed.
Historical local references are retained on repeat import for explicit reviewed removal.
Unassigned mod folders
appear disabled in an auxiliary profile. Download inventory imports bounded filenames/sizes
and allowlisted sidecars, without archives, caches, credentials, source saves or MO2 plugins.

Repeat import preserves local organization/identities, appends newly seen sources/profiles,
and keeps removed local profiles removed. Cancellation/failure rolls back the single SQLite
profile transaction; persistent run state supports retry after process interruption.

For moved references, the mod context action **Relink source folder...** reviews an ordinary
local replacement folder, requires same-mod acknowledgement, updates its shared local pointer
across profiles and offers **Undo source relink**. Bounded inventory pins names, file identities,
sizes and modification times, including empty directories; it does not hash payload equivalence
or provide filesystem snapshot isolation. Nested links/library overlaps are rejected. Fourteen
service cases and six native journeys cover stale inputs/context, repeat/reopen, cancel and faults.

**Refresh source metadata...** offers version, notes, tags, categories and a Nexus association
individually; all start unchecked. Original source metadata, display aliases and organization
stay intact. Category IDs require an explicitly chosen `categories.dat`; missing or unsupported
fields remain unavailable and preserve local values. A single matching `installedFiles` pair
identifies its file; merged/foreign pairs show a warning and clear the unrepresentable single ID.
Each input is an ordinary bounded 1 MiB file, hashed and reread before commit. QSettings scalar
escapes, UTF-8/BOM/UTF-16/CP1252 are tested; binary QVariant/typed values are not decoded.
Thirty service cases and six native journeys cover selective preservation, both profile entries,
repeated import/reopen, stale data, encodings, faults/cancel and persistent guarded Undo.
This implementation reads documented source formats independently; it copies no MO2 code.
Format reference: [MO2 metadata reader](https://github.com/ModOrganizer2/modorganizer/blob/master/src/modinforegular.cpp).
The optional ownership-copy backend remains separately tested but is not in primary import
or exposed as a normal Copy/Migrate action. It never runs implicitly.

The **Profile** selector and **Profiles** controls sit above the actual mod hierarchy,
with executable selection/Run above six right-side panes. The filter sidebar is optional
and pane proportions are resizable. Mod rows use TreeNode widgets and a list clipper;
there is no modlist table widget. Font size, row spacing and visible status/category/ID/
version/notes annotations are local settings. Narrow panes retain room for mod names.
Create/clone/rename/delete/restore are local metadata operations. Delete names a frozen
target and requires confirmation; Cancel does nothing. Removed profiles survive restart.
Shared mods/source MO2 profiles stay intact. Removing the last local profile retains mods;
new blank profiles include existing source/owned references disabled. If an independently
uninstalled owned mod is needed for restore, restore fails explicitly until that mod returns.

Groups such as Visual/Textures are expandable display folders. Higher numeric priority
wins file conflicts; visual sort preserves priority. Search reveals matching descendants.
Expansion and stable selection IDs survive sorting/restart. Up/Down select, Left/Right
collapse/expand the selected group, Space toggles enabled state, Enter selects.
Ctrl/Shift select multiple mods. Drag onto a mod to change priority or onto a group to
organize; context actions apply to the selected set. Ctrl+Z restores the previous local
entry organization when no later install/import has changed it. Undo covers enabled state,
group, priority and separator entries, not shared tags/notes or payload changes.
Library and Settings are the flat main destinations. The compact toolbar exposes Install,
Add mod, Refresh files, Undo, Actions and Filters; MO2 import is in Add mod and the Mods menu. Actions includes
folder commands, profiles, executables, metadata backups, overwrite, export and recovery.
Folder commands validate the current local profile membership, ordinary source folder and
owned location, then pass a directory argument to Explorer without a shell. They never execute
mod payloads. Explorer itself is mocked in the fixture tests.

| Shortcut | Local behavior |
| --- | --- |
| Ctrl+F | Focus the mod search, or the plugin filter when its pane has focus |
| Ctrl+P / Ctrl+E / Ctrl+S | Profile actions / executable editor / Settings |
| Ctrl+M | Choose an archive with the existing installer mode; cancellation prepares nothing |
| F5 | Background refresh of loose-file providers/conflicts; archives/plugins have separate pane refresh |
| Ctrl+Up / Ctrl+Down | Move the selected set as an ordered block one priority position; preserve selection and undo |
| Ctrl+A / Ctrl+Z | Select visible mod descendants / undo local organization from the tree |
| A-Z | Cycle visible matching mod names; reveal selection by scrolling |
| Ctrl+Enter / Ctrl+double-click | Open the selected available mod folder |
| Double-click | Show selected mod Information |
| Ctrl+N / Shift+double-click | Open the known game / selected mapped mod page on Nexus |
| F2 | Review and save the shared display title; source directory and identity stay intact |
| Delete | Review removal of an owned mod with a complete retained recovery backup |
| Alt+group click | Select matching mod descendants of that group |
| Shift+Enter | Collapse/expand the selected group or selected mod's containing group |
| PgUp / PgDown in Information | Previous/next matching mod; typing fields keep these keys |
| A-Z / Up / Down in Plugins | Cycle matching names / move selection through filtered plugins |
| Ctrl+A / Shift+selection in Plugins | Select matching plugins / select a range |
| Space in Plugins | Toggle selected draft entries; required game masters remain enabled |
| Ctrl+Up / Ctrl+Down in Plugins | Move the selected draft block while respecting core and locked priorities |
| Drag a selected plugin | Move the selected block before the target, including selections hidden by filtering; review/apply persists the draft |
| Ctrl+Enter / Ctrl+double-click / Shift+double-click in Plugins | Open managed provider folder / folder / mapped Nexus page |
| Escape | Close this manager's workflow and cancel its worker; active typing first retains its own Escape handling |

These independently implemented commands follow the official
[MO2 shortcut documentation](https://github.com/ModOrganizer2/modorganizer/wiki/List-of-MO2-Shortcuts)
where their local behavior is defined. F2 edits the hovered display title, priority, version,
category, notes or Nexus ID; hovered plugin priority edits a reviewed draft. Delete reviews
owned removal; Actions/context offers reviewed reference/separator metadata removal. Nexus links require a known domain/ID;
custom homepage URLs are not executed.
Other workspace windows do not receive these manager commands. Mutation commands are disabled
while a worker or recorded runtime session is active; typing fields keep their keys.

The Plugins pane inventories local winning providers and uses the supported modkit CLI
for header/masters inspection. Review/apply validates required cores, dependencies, cycles,
master ordering and game count limits, backs up local activation/order snapshots, and
revalidates them before native launch. It never edits global Plugins.txt. Header caches
persist with size/mtime freshness; they do not detect same-size/mtime-preserving edits or
CLI build changes. Local locked priorities use MO2's filename|priority format, retain absent
names, and block conflicting moves until explicitly unlocked. Applying saves activation,
order and locks together; runtime validation detects later edits. Full/light indices show
only when active headers are known. This strict local behavior does not replicate MO2's
automatic conflict shifting. Broader game adapters remain open.

The Archives pane explicitly indexes members through modkit into a local JSON cache,
without extraction. It shows game/mod/owned-overwrite container provenance, shadowing,
profile INI lists, active-plugin name candidates and member overlaps. Candidate order is
a profile preview; actual engine attachment and loose/archive precedence are unverified.
Inspection is bounded to 250,000 members per archive, 1,000,000 distinct combined paths
and 32 MiB metadata per archive. The supervised Windows reader cancels its entire exact
process job; output bounds are checked every 25 ms and can overshoot between checks.

Data displays a clipped expandable loose-file hierarchy. Explicit background inventory
includes available game Data, mod folders and owned runtime overwrite, with provider
provenance and highest-priority winners. Search reveals matching descendants. Owned files
can be hidden/unhidden through a full recoverable mod replacement; external source files
are read only. Hide/unhide parent preserves nested hidden files and empty directories through
the same backed-up owned replacement, rejects folder merges and rechecks the reviewed bytes.

Profiles include local rename, INI editing with stale checks and previous-value restore,
and reviewed metadata backup/restore/removal. Metadata backups retain configuration and
organization; they contain no mod/save payloads. Effective FO4 INIs use game-made runtime
edits only when their launch baseline still matches the local snapshot. Changes in the
editor take effect without being replaced by an old runtime copy. The bottom Log retains
the last 256 local status events and has explicit show/clear controls.

**Actions > Profile recipes** exports a UTF-8 `.modbox-profile.json` into owned `exports`.
It retains ordered activation/groups/separators and allowlisted plugin/order/archive/INI
configuration text, including effective FO4 runtime INIs. It excludes mod/save payloads,
source-folder metadata, credential stores, executable definitions and validation caches.
Configuration text can itself contain user-authored paths; the preview shows its exact text.
This is our own metadata format, not an MO2 instance bundle or payload distribution.

Read a recipe, map every payload to a distinct existing destination mod, name the new
profile, Review and acknowledge before Import. Same-library identities can map automatically;
equal names across libraries never do. New separator records preserve nested groups.
The selected destination profile supplies the local game instance; imported source paths
never select a game. Missing payload folders stay metadata references and remain unavailable
to native launch. Plugin/header/INI validation must run again before modded launch.

SHA-256 pins the recipe bytes; template/order, mapped metadata and instance configuration
are checked inside the SQLite commit. Repeated transfer preserves local edits and removed
profiles stay removed. Cancellation/failure rolls back all profile/separator insertions;
export never replaces previous files. Bounds: 64 MiB metadata, 100,000 entries/candidates,
2 MiB per configuration text. Thirty-three service cases and ten actual-input cases cover
same/cross-library mapping, repeat/cancel/failure/stale/invalid inputs and clipped 50k entries.
An abrupt process kill may leave an unpublished owned staging file; automatic pruning is
not implemented. Payload transport, MO2 recipe interchange and live synchronization remain
separate gaps.

Saves inventories owned per-profile runtime files on explicit background refresh.
**Save locations** also offers the selected MO2 profile's source saves and an explicitly
chosen source/global folder, both read only. It never scans Documents automatically;
root/network/reparse folders are rejected. Source inventory is limited to 100,000 directory
entries / 10,000 save and companion files. **Import / back up source save** reviews SHA-256
and folder identity, then copies to owned local profiles or retained bundles. Source files
are never moved/deleted/replaced. Source folder replacement and a newly appearing/missing
companion invalidate review. Twelve source cases and eight actual input cases cover browsing,
header inspection, copies to the current profile, backup/restore, repeats, stale inputs,
cancellation/failure and interrupted recovery without rereading source files.
FO4 basic-header inspection reads character, level, location, time and ordinal with strict
field/header bounds and stale/cancel guards. It does not decode the save body or establish
plugin compatibility. **Copy / back up save** reviews a save and matching extender companion
using SHA-256, then copies the pair to another local profile or a new owned backup bundle.
Existing destinations are never overwritten. Cancellation before pair placement leaves no
destination files; interrupted placement retains a journal and is completed only when the
already placed bytes match exactly. Changed partial destinations stop recovery for review.
**Save locations > Save backups** lists bounded owned receipts and reviews restoration into the selected
local profile. SHA-256, file identity and receipt freshness are checked before staging;
matching save/companion names block restoration. Review/acknowledgement/Restore/Cancel stay
visible while bundle information scrolls. Backups remain intact. Sixteen backup cases and
eleven pair-transfer cases cover invalid/stale receipts, growing streams, cancellation,
failed staging, repeated collisions, deleted source profiles and interrupted placement.
**Save locations > Default global FO4 saves** resolves the current Windows Documents known
folder, including ordinary redirected locations, and explicitly reads its default
`My Games/Fallout4/Saves` directory. No folder is created; customized INI/Store locations use
Browse. Seven service/six input cases cover repeat/absent/cancel/failure/reparse and source
immutability. These tests replace Documents discovery with disposable fixtures.
Save plugin comparisons and screenshot previews are described below; save-body/companion
compatibility remains unverified. No real save was read or changed as testing.

**Review save backup deletion** and **Review output backup deletion** open a separate
fixed-footer review of one owned retained bundle. Permanent removal requires acknowledgement,
rehashes the complete bundle, stages it in owned quarantine and records its SQLite commit.
Cancellation/failure before commit restores the complete bundle; committed interruption
resumes on reopening, checking every remaining file before deletion. Unexpected/changed
bytes, directories, links, original locations or commit markers stop cleanup and are preserved.
Current saves/output and other backups stay intact. Restored overwrite receipts can be removed
without deleting restored output. Fifty-five service/twelve input cases include six real
process-exit recoveries, repeated removal, stale review, runtime locks and two window sizes.

## Headless agent launch (MO2 replacement for agents)

`agent_launch.py` is a GUI-free entry point for AI agents. Every command prints one JSON
document; refusals print `{"error": ...}` and exit 1. Nothing here edits an MO2 install or
instance, game Data, physical user INIs/`Plugins.txt` or CommonLib.

```powershell
uv run --no-sync python -m creation_lib.mod_manager.agent_launch instances
uv run --no-sync python -m creation_lib.mod_manager.agent_launch sync --mo2 "<LocalAppData>\ModOrganizer\Fallout 4 - VC\ModOrganizer.ini"
uv run --no-sync python -m creation_lib.mod_manager.agent_launch launch --profile Default --exe f4se --agent-defaults --wait-game 60 [--dry-run]
uv run --no-sync python -m creation_lib.mod_manager.agent_launch status [--session <id>]
uv run --no-sync python -m creation_lib.mod_manager.agent_launch stop --pid <game, loader or helper PID>
```

- **sync** uses the normal metadata-only import into a separate agent library
  (default `<toolkit data>/mod_manager/agent-library`, overridable with `--library`), then
  mirrors MO2 as the source of truth: entry order, enabled state, separators and profile
  files follow MO2's current state on every sync. Mods are referenced in place, never copied.
  `launch --mo2 <instance>` syncs first.
- **launch** prepares the ordinary project-filespace plan for the imported executable
  (`f4se` selects the instance game's `f4se_loader.exe`) and starts the existing helper.
  The helper is spawned without a console and leaves the caller's job when that job allows
  breakaway. The filespace now creates the loader with `CREATE_NO_WINDOW`, so console
  `f4se_loader.exe` opens no terminal tab. It refuses to start while any `Fallout4.exe` runs.
- **INI scope.** With `--ini-source auto` (default) an MO2 profile whose `settings.ini` has
  `LocalSettings=false` uses the Documents INIs, read only, as its baseline, matching MO2.
  `--ini FILE:SECTION:KEY=VALUE` (repeatable) and `--agent-defaults` (always active,
  background mouse, windowed 1920x1080 and HighFPSPhysicsFix `LockCursor=false`) change only
  this launch's owned runtime copies. A Data INI override is a generated owned file mapped over
  the winning mod or game file. Overrides are skipped when the file is absent and refused when
  owned overwrite would shadow them. The next launch rebuilds the copies from the unchanged
  profile. Save-path and `sTestFile` keys cannot be overridden. `bBorderless` is inherited.
- **Status** reports the helper/loader PIDs, verified children with images, `game_pid`,
  `agent-mappings.json` (every mapped file and mount) and the owned output, INI and save
  folders. `output.game_root` receives new or changed files written beside `Fallout4.exe`,
  such as engine `ScreenShotN.png`. The game folder itself stays unchanged. **stop** accepts only a PID recorded in a live session of this library. It
  verifies the helper's creation identity and terminates that helper; the helper's
  kill-on-close job ends the game tree. It never matches processes by name.

**import-saves** `--profile Default --save <name>.fos` (repeatable) and/or `--latest` copies a
save and its `.f4se` companion from the Documents `Saves` folder (or `--folder`), read only,
into the profile's owned saves through the reviewed `save_transfer` path. Names already present
are skipped, never replaced. Run it before launching; it refuses while a session is live.

Limits: global saves are not visible in game unless copied in this way, only
LocalAppData MO2 instances are listed, and no `modkit` subcommand wraps this yet. Real
game boot through this path has not been executed; see
[F4SE_REAL_BOOT_PLAN.md](F4SE_REAL_BOOT_PLAN.md) for the pending approval request.

## Executables and the selected native runtime

Executable definitions become first-class persistent local entries. Edit title, binary,
Windows arguments, working directory, mode, hidden state and Steam ID. Local edits/removals
survive repeat import. Import never executes commands. Absolute local paths/CWD, .exe type,
quoting, credentials, length and stale reviews are checked; subprocesses use no shell.

**Direct mode** explicitly uses the normal filesystem without our selected mod profile.
It is a user-selected tool mode, never a fallback from failed modded launch. It supplies no
mod virtualization or source protection. Process/error behavior is tested with mocks.

**Selected profile mode** uses an isolated project-owned native filespace helper, scoped to:

- Windows 10+, source checkout or private onedir integration, x64 Python, x64 Fallout4.exe/F4SE in the selected game root
  or explicitly acknowledged experimental manager-owned F4SE staging,
  this project's x64 filespace DLL, and Steam already running. Development Run builds it
  offline with CMake/MSVC when needed; packaged apps include a receipt-validated DLL and
  original source/notices. No upstream runtime acquisition, manual bundle path or source-write
  acknowledgement is required. See [PROJECT_VFS.md](PROJECT_VFS.md) for source and exact limits.
- A local standalone/imported/cloned profile with a plugin activation snapshot. Unavailable plugins
  are omitted from that launch's snapshots. Enabled state/priority choose installed file mappings; existing owned runtime
  overwrite has higher precedence. Payload scan happens only on a requested launch review.
- F4SE's x64 game-root Fallout4.exe target is validated/fingerprinted. Alternate target/DLL,
  editor, Steam/launcher overrides and nondefault effective RuntimeName are refused.
  Ambiguous config duplicates and late effective config changes are rechecked. Managed target
  staging supplies its own reviewed full executable/DLL paths; arbitrary user overrides remain blocked.
- Plugin/order snapshots and all three FO4 INI names map to owned files. Missing INIs require
  explicit acceptance of minimal local settings. A distinct reserved virtual save path
  prevents fallback to global Saves. Effective owned INIs reassert save routing each run
  and reject sTestFile overrides. Game-made settings remain in that session's retained output.
- New Data output uses runtime/sessions/<session-id>/output/overwrite. Original payloads remain
  in place. The manager prepares owned output/metadata and does not deploy or edit real Plugins.txt.
- A unique VFS session, restricted DLL search, suspended process creation, atomic Windows
  job assignment and PID persistence before resume keep hooks out of the UI. Both job and
  VFS descendants are supervised. Persistent active-session pointers block library changes
  after UI restart. Preparation has a 180-second limit and never falls back to direct launch.
- Cancel applies before process creation. UI close leaves helper/game running. **Helper
  crash or supervision failure terminates its supervised game tree.** Completed/interrupted
  processes release the library guard. Helper/root/child records contain handle-derived
  Windows creation times. Recovery requires matching PID and creation time; recycled PIDs
  are excluded. Inaccessible, missing, legacy or incomplete identities keep the library locked.
  Child handles must belong to the exact helper-owned job before they are adopted.
  Recovery never kills by numeric PID; startup cleanup validates the retained Popen handle.
  A bounded, atomic owner handshake must match the actual helper before native construction
  and resume. If the parent's initial identity query fails, cancellation reaps the waiting
  helper. The source helper loads the shared Rust backend through the known base Python executable, avoiding a
  Windows venv redirector whose process handle can differ from the executing helper.
  Failed sessions retain recorded child IDs until asynchronous job termination finishes,
  so a restarted UI cannot unlock the library during that interval.

### Protected ordinary writes

First writes copy only the affected source/game file into owned output, publishing the copy
atomically before the write opens it. New files/directories also use owned output; direct
protected-source writes are refused. Deletion, managed rename/hardlinks, unsupported native
classes/APC/child creation fail explicitly. Arbitrary editors/generators and other games remain
blocked pending supported mutation semantics and runtime coverage. This is not a security sandbox:
direct syscalls, uninstrumented brokers and loader-time mutations are not certified protected.

A native disposable probe writes an existing mapped file and proves only its owned output
copy changes. It separately proves new output/saves go to owned folders and generated
source/game/user files stay intact. No real source mod,
MO2 profile, commercial game asset or game process was changed/run as testing.

Fixture success proves bounded VFS plumbing, not complete FO4/F4SE boot, Steam hand-off,
extender compatibility, plugin/game edition correctness or MO2 runtime parity. Other games,
x86/cross-bitness, global save migration and onefile packaging remain unsupported. Private
onedir GUI startup/normal close and payload/helper integration have fixture coverage; latest
packaged rendered-pixel capture failed and remains unverified. Clean-machine installation
and real game execution remain unexecuted. See [PARITY_REVIEW.md](PARITY_REVIEW.md).

## FOMOD coverage

The independent parser supports defined declarative semantics of versioned 1.0-5.0
XML scripts. A 5.1 schema header is accepted as a 5.0 compatibility alias; no separate
5.1 standard is verified. Coverage is bounded by the table and fixture tests below;
this is not full XSD conformance, an official installer certification, or support for every
historical backend. Declared `ModConfig`, `ModuleConfig`, and `XmlScript` versions are checked;
unversioned documents use bounded structural inference.

| Semantics | Tested coverage |
| --- | --- |
| Group selections | SelectAny, SelectAll, SelectExactlyOne, SelectAtMostOne, SelectAtLeastOne; cardinalities and defaults |
| Option types | Required, Optional, Recommended, NotUsable, CouldBeUsable; dynamic types use the first matching pattern |
| Legacy XML | Version 1 misspellings, version 1/2 ordering/default dependency rules, root optional groups for versions 1-3, multi-step versions 4/5 |
| Dependencies | Nested And/Or, Active/Inactive/Missing states, unset flags match empty values, explicit game/FOMM and FOSE/NVSE/SKSE/F4SE versions; undecidable states fail explicitly |
| Wizard progression | Step visibility, recalculation and sorting; explicit MO2 earlier-step versus schema same-page fixed-point flag policy |
| File planning | Required files, all matching conditional patterns, rename/folder expansion, wrappers, case-insensitive sources, omitted destinations default to source; empty sources are MO2-compatible no-op sentinels |
| Inclusion and conflicts | alwaysInstall, installIfUsable, schema boolean whitespace, integer priority, original XML order breaks ties across display sorting/legacy normalization, Windows path collisions |
| Invalid input | Missing sources, unknown schema/extension nodes, invalid cardinalities/enums/booleans, cyclic flag evaluation, malformed XML, DTD/entities, traversal |

Unknown file states require user input unless the expression can already be decided.
Known installed states come from enabled profile content and imported plugin activation.
Preparation pins local profile configuration, effective loose-file winners, ordinary provider
identities, plugin activation and bounded CCC metadata. Context is checked again during
destination review, before staging and inside the commit transaction before payload placement.
Changed/missing/new providers and assets invalidate the prepared installer. Incomplete game
context and absent non-plugin assets that may exist in archives stay unknown. File freshness
uses size/mtime; this is not filesystem snapshot isolation or hostile-edit detection.
The wizard's environment uses mutation revisions, so rendering does not sort its complete
dependency inventory every frame. A 50,000-state real-widget case checks cache invalidation.
Versions require explicit values, including requested script-extender versions. No claim
of complete dependency detection against a game or automatic extender discovery.
Module/option artwork is an explicit worker preview (PNG/JPEG/BMP/GIF/WebP first frame,
8 MiB encoded, 16 MP decoded, at most 1024x1024 retained RGB). Optional bounded info.xml shows
package-authored name/author/version/ID/website/description without trusting Nexus identity.
Image decode is not a process/memory sandbox. Bounded title/header presentation does not
affect file semantics. Executable C#/Python installers and arbitrary extension nodes are
rejected. No installer scripts are compiled or executed. XML is limited to 2 MiB,
40 nesting levels, and 30,000 elements. DTD/entity
checks operate on decoded text, including UTF-16. Fixture tests follow the published
versioned schemas. A primary-source audit and schema-validated generated cases corrected
unset flags, empty source sentinels and tie order. There are 64 original parser cases,
25 compatibility cases and three policy-controller cases; six actual-input cases exercise
switch/review/install/cancel/failure. Official-engine execution and a real mod corpus remain
untested.

A further **70-case original differential corpus** matches pinned official schema validity
through the offline .NET validator and both policies' explicit source-derived plan expectations.
It covers schema versions 1–5, group defaults/cardinality, priority/tie order, forced unavailable
options, radio defaults, duplicate flags and documented safe-planning differences. This is an
executed schema oracle, not execution of MO2's Qt engine. Reproduction, pinned hashes/source
links, guard differences and exact host limitations are in [FOMOD_DIFFERENTIAL.md](FOMOD_DIFFERENTIAL.md).

New installs default to **MO2 (defined compatibility)**. Flags from earlier visible steps
determine dynamic types; each step's first checked duplicate flag wins, with trimmed values.
The most recent visible step overrides an earlier value. SelectAll and single-option
required-selection groups force their options. MO2-style case-sensitive ordering and
specified recommended/fallback defaults are covered. UTF-8/BOM UTF-16, BOM-less UTF-16 and
Latin-1 fallback are accepted; heuristic decoding displays a notice. **Schema-oriented**
preserves same-page fixed-point evaluation, last duplicate flag/untrimmed values,
case-insensitive display sorting, unavailable-choice rejection and strict Unicode decoding.
The direct parser API defaults to this schema policy; the installer explicitly selects MO2.
Changing policy reparses in a worker, resets choices/destination edits and invalidates review.
Cancelled/failed reparsing preserves the prior engine and reviewed plan. Both policies keep
the same archive/path/context guards and require Review before install.
This does not reproduce every historical MO2/NMM quirk: contradictory required radio
choices, malformed input, Qt event/default-selection differences and declaration recovery
heuristics still need a wider differential corpus. Missing sources remain explicit errors.
The independently implemented policy was checked against the pinned
[MO2 presentation/condition behavior](https://github.com/ModOrganizer2/modorganizer-installer_fomod/blob/24f07cf7af5273efe52daa90c631a49e7e847c74/src/fomodinstallerdialog.cpp).

## Installation, Files, Nexus and recovery

Safe ZIP/unpacked and external 7z/RAR installs use reviewed file plans and private owned
staging. Owned reinstall/uninstall/restore preserve backups and organization, journal changes
with SQLite commit markers, and reconcile interrupted transactions. Sources are not adopted
or deleted. Cancellation/failure rolls back; uncertain commits report recovery explicitly.
Lifecycle reviews hash owned bytes; new externally mutable packages use size/mtime checks.

Recovery offers **Review permanent backup deletion** for one completed owned payload.
The review hashes its files, includes empty directories and pins its operation metadata.
An acknowledgement enables deletion of that backup and its restoration entry. Installed
mods, profile organization, other backups and quarantine siblings remain intact. No
automatic pruning occurs. A durable receipt and atomic rename restore intact content on
pre-commit cancellation/failure; committed interruption resumes only unchanged reviewed
files and empty directories. New/changed content or reparse points stop cleanup for manual
review. After commit, cancellation cannot recreate partially deleted bytes. Reopen the
library to finish pending cleanup. Invalid receipts fail closed; an unpublished temporary
receipt left by process termination needs manual removal. Overwrite/save histories and
uncommitted quarantine content have separate lifecycles and are not pruned by this action.
The 25 service cases and six actual-input cases use disposable generated payloads only.

After the first file review, Customize destination mapping can rename, relocate or exclude
individual incoming files. Choices/root changes reset custom mappings, and edits require
another review. Traversal, stale mappings, duplicate Windows destinations and file/directory
collisions fail explicitly. Reinstall offers Replace or Merge: Merge keeps unmatched existing
files (including hidden content), uses incoming files for path collisions and retains the full
previous folder. Both behaviors preserve profile references and have cancellation/fault tests.
Additional package sources provides a clipped expandable tree and explicit destination entry.
Only captured ordinary payload members are selectable; metadata/hidden sources, traversal,
stale sources, collisions and implicit replacements fail. Remove or relocate a conflicting
base destination first. **Manual layout (skip FOMOD rules)** explicitly bypasses malformed,
executable or duplicate configurations using the same safe reviewed install transaction.
No script/rule executes and nested FOMOD metadata is excluded. Choose the correct Data root
and variants yourself. Working FOMOD installers can also be reopened in this mode.
External-reference overlays and archive associations remain open.
Back, Next, Review, Close and Install stay in a fixed footer while long choices, artwork, additional
sources and file plans scroll. Actual input verifies long options and installation at
1280x800 and 1600x900.

Settings > Manage categories edits a library-local taxonomy with optional Nexus category ID
metadata. Assign category in mod information updates the shared mod across local profiles.
Nested rename atomically changes descendants and assignments; stale reviews/collisions fail.
Unused leaf removal is explicit. Primary and up to 31 additional categories are shared
across local profiles. Additional categories accepts one path per line. Search and the
category sidebar match any membership and nested parent paths; Uncategorized means no
memberships. MO2 comma-separated category IDs resolve through categories.dat; zero/empty
entries are skipped and unknown labels/IDs remain explicit. The first retained name is
primary. Repeat import preserves local edits, and retained owned-mod restoration preserves
memberships, including older receipts with only a primary category. Thirteen taxonomy
cases and actual input cover these workflows. External category-map validation, malformed
MO2 primary-ID quirks and broader imported-category corpora remain open.

Path/link/encryption/collision/expansion guards apply. Install defaults: 100,000 files,
200,000 entries, 8 GiB expanded, 2 GiB/file, compression ratio 2,000. Larger packages fail.
ZIP streams members; native 7z/RAR listings are bounded to 16 MiB and members stream through
argv, with timeout/cancel/CRC checks. SFX, split/encrypted archives, special files and anti-items
are rejected. External decoders are not memory sandboxes. Solid 7z and stored RAR4 fixtures
are verified; compressed RAR5 and large solid-archive performance remain gaps.

There are **27 actual child-process interruption cases** across owned operations, optional
source copying and primary metadata import. This is process-crash evidence, not a power-loss,
drive-corruption or hostile filesystem-snapshot guarantee. Exports lack that journal;
the download queue uses separate durable checkpoints/finalization receipts described below.

Files indexes loose providers with hidden-parent support, source navigation, clipping and
stale checks. Badges stay unknown for missing/incomplete/offline inventory. Nexus version
inequality is not a newer-update claim. Separate Archives inspection indexes container members.
Data > **Read loose + archives** builds a clipped union of loose files and archive members,
with provider details, overlap filtering and navigation to the archive container. FO4 unrestricted
loose overrides require explicit local bInvalidateOlderFiles=1 and empty sResourceDataDirsFinal.
Archive predictions use INI order followed by active plugin order; equal ranks and nonstandard
plugin attachment names remain unresolved. Effective INIs, profile state, plugin providers and
source stamps are rechecked across both inventories. This is a load-plan prediction, not tested
in-game engine precedence. Stage creates a reviewed combined
owned export; it does not deploy.

Saves > **Inspect saved plugins** reads bounded FO4 header versions 11/12/13/15, screenshot
dimensions and regular plugin tables; version 15/form 68+ additionally reads light tables.
Normal inspection skips screenshot bytes. Compare with the current local profile or another displayed
save to see missing/disabled/added plugins, changed full/light type and changed indices.
Profile comparison inspects winning plugin headers through cached modkit CLI calls; stale
profile/provider/save identity blocks comparison. Unknown headers do not produce predicted
index shifts. UTF-8 and explicit-notice Windows-1252 metadata are supported. Unsupported
layouts, unsafe/duplicate names, truncation, excessive dimensions/counts and malformed block
lengths fail explicitly. Twenty-one reader/comparison cases and six actual input cases cover
read/compare/cancel/failure; source folders are read only. **Preview screenshot** explicitly
reads a pinned RGBA-premultiplied screenshot in cancellable chunks, within 16 MiB/four megapixels,
and retains at most 1024x1024 RGB. Seven service / six input cases verify colors, thumbnails,
bounds, stale metadata, preview/hide, failure and cancellation. The native 1280x800 rendered
preview was viewed. Recently inspected save metadata/comparisons are bounded to eight entries,
images to four and basic headers to 32; a 35-save controller journey verifies eviction/failure
retention. Save body, extender-companion semantics and real-save runtime compatibility remain
unverified. The pixel layout is independently implemented from MO2's
[Gamebryo image reader](https://github.com/ModOrganizer2/modorganizer-game_gamebryo/blob/master/src/gamebryo/gamebryosavegame.cpp).

Actions > **Remove selected reference / separator**, or its mod context action, reviews local
profile membership removal. Bulk context selection accepts references/separators only.
Source content and other profiles remain intact; durable exclusions keep repeat imports from
reintroducing removed rows. Undo restores entries and clears only those exclusions, provided
later entries/source/ownership changes have not invalidated the snapshot. Owned removal still
uses retained payload recovery. Eleven service cases and six actual-input cases cover repeated
imports, other profiles, Undo, cancellation, faults and stale targets.

The actual save/combined Data workflows use explicit API-1 trusted metadata adapters, visible
under Settings > Metadata adapters. Registration and callback results are validated, with no
automatic discovery or script/binary loading. See [EXTENSIONS.md](EXTENSIONS.md). Installer/tool/
game runtime extensions, MO2 binary compatibility and frozen distribution remain open.

**Mods > Manage owned overwrite** reviews this local profile's generated output with SHA-256.
Copy output creates an enabled owned mod and keeps output in place. Clear with backup moves
the complete owned output folder into retained history and creates an empty output folder.
Empty directories are preserved. Restore requires empty output, rechecks the backup and never
merges/replaces existing output. Journals reconcile interrupted renames; changed placed
content stops recovery for manual review. Real MO2 overwrite files are never moved or imported.
History is bounded to 1,000 retained backups; backup removal UI remains open.

Nexus uses session credentials, explicit nxm intake/download review, entitlement/checksum/
redirect checks, deduplication and opt-in bounded version checks. Secret keys/full URLs are
not persisted. Browser handler registration is separate opt-in and tested with a fake registry.
No real account setup/download was used. The persistent local queue stores only allowlisted
identity, byte/hash and strong-ETag checkpoints; API/browser keys and CDN URLs remain in memory.
Pause keeps owned partials, and resume verifies their committed prefix, obtains fresh entitlement,
and validates range totals/ETag/content coding before appending. A changed representation restarts
safely. Strict 416 EOF proof can finalize an already complete unknown-length response. Finalizing
receipts recover rename/commit interruptions without moving a registered archive away; local
finalization needs no Nexus connection. UI progress arrives through a worker queue, without a DB
query per frame. Transfers have byte/time bounds and checksums when supplied by Nexus. File-specific
updates follow author-published replacement edges. Account endorsement/tracking use separate explicit
requests and report unconfirmed outcomes honestly; cancellation cannot undo a request already sent.
Fetched replacement/endorsement state now appears in list badges; graph results use a bounded
worker-primed cache. The nxm cold-start command opens the standalone manager, preserving existing
running-host forwarding. Profile clone includes saved target INI/plugin overlays atomically;
owned save/output directories remain separate.
These are mocked API/CDN tests, not live-account proof. Public SSO/client approval, live account/CDN
coverage and all free-account browser restart flows remain gaps.

The [Nexus controls](NEXUS_CONTROLS.md) add exact Free replacement-file browser navigation,
rotating file/version/account batches, optional automatic file checks, explicit reviewed
unendorse and other account requests, target-bound file-ID editing and downloaded additive
update routing. Failed reads remain unknown; changed sessions/installed context discard
stale results. These local controls do not establish live-account acceptance or SSO.

[Browser destinations and reviewed batches](NEXUS_ROUTING.md) add an explicit chooser for
simultaneously running libraries, pinned library/profile/game-target intake, public delivery
receipts and bounded reviewed account/download actions. Routing itself never downloads;
finished archives still need installer review. Secrets stay in memory, cancelled/unknown
outcomes remain explicit, and cross-build payload compatibility is unverified.

**Downloads > Install referenced archive** opens the reviewed installer directly against an
ordinary local imported archive. Completed downloads first review an explicit destination:
new owned mod, a matching canonical owned mod for reinstall, or an additive owned patch
above a matching reference. Ambiguous candidates require selection;
source references/target overrides cannot be silently replaced. Review pins archive identity
and destination/profile/context, and failed/cancelled new installs or reinstalls roll back.
Repeated new-mod names fail safely; explicit reinstalls retain identity, groups and enabled state
across profiles. Sources and archive bytes remain intact. Fifteen service cases and sixteen
combined native daily journeys cover this routing, filters, cancellation/fault/retry and targets.

**Filters** adds measured conflict/update/endorsement/tracking states and matching ascending/
descending status sorts. Unknown/offline state stays distinct; matching descendants reveal
anchors and display sorting retains install priority. Thirteen service cases include a
20,000-identity replacement graph; the worker precomputes graph memberships once.

**Settings > Simultaneous downloads** selects one to four slots. One retains the foreground
review flow; two to four use background transfers while local UI/metadata work remains responsive.
Identity enqueue and Nexus API resolution are serialized; independent CDN streams have private
session HTTP clients and per-job cross-process locks. Rate backoff is shared. Pending requests,
individual jobs or all transfers can pause; shutdown persists queue checkpoints. Library/account/
runtime switches wait for those checkpoints. Completion preserves open local reviews and never
installs automatically. Twelve scheduler service cases and four native journeys include two/three/
four overlapping distinct generated streams, another process's lock, independent failure/pause/
resume, rate backoff, reopen and source immutability. No live Nexus/CDN performance is claimed.

## Verification and performance

Run service tests separately from real ImGui tests: shared toolkit fixtures substitute
imgui_bundle mocks, so combining their collection invalidates native widget tests.

```powershell
uv run --no-sync pytest ui/mod_manager/tests --ignore=ui/mod_manager/tests/test_widget_ids.py --ignore=ui/mod_manager/tests/test_daily_native.py --ignore=ui/mod_manager/tests/test_priority_native.py --ignore=ui/mod_manager/tests/test_preferences_native.py --ignore=ui/mod_manager/tests/test_parity_workflows_native.py --ignore=ui/mod_manager/tests/test_installer_wizard_native.py --ignore=ui/mod_manager/tests/test_nexus_controls_native.py --ignore=ui/mod_manager/tests/test_nxm_bulk_native.py -q
uv run --no-sync pytest ui/mod_manager/tests/test_widget_ids.py ui/mod_manager/tests/test_daily_native.py ui/mod_manager/tests/test_priority_native.py ui/mod_manager/tests/test_preferences_native.py ui/mod_manager/tests/test_parity_workflows_native.py ui/mod_manager/tests/test_installer_wizard_native.py ui/mod_manager/tests/test_nexus_controls_native.py ui/mod_manager/tests/test_nxm_bulk_native.py -q
uv run --no-sync pytest ui/toolkit/tests -q
uv run --no-sync ruff check ui/mod_manager
uv run --no-sync python -m ui.mod_manager.tests.project_vfs_probe --output <temporary-report.json>
uv run --no-sync python -m ui.mod_manager.tests.project_runtime_probe --output <temporary-helper-report.json>
```

Generated fixtures cover immutable source metadata/repeated imports, cancellation/failed
installs, profile recovery/last-profile/shared-mod behavior, declarative FOMOD coverage,
archive guards and rollback, mocked Nexus and native Windows IPC. Native ImGui input exercises
tree/keyboard/IDs, menu/import/profile/executable/review/cancel flows at 1280×800, 1600×900
and 1920×1080. The project-owned native C++ and Rust-backed helper probes verify priority,
hidden/disabled exclusion, plugin/INI mappings, save isolation, new output, source-protected writes, child inheritance,
children surviving parent exit, helper-crash tree termination and restart unlock.

Historical service checkpoint: **1,234 passed /361.68s**. The earlier actual
input checkpoint passes **227 cases /152.87s**, including normal toolkit routing, toolbar/shortcuts,
field/group edits, selected mod blocks, concurrent queues, reference relinking, profile recipes,
combined Data, save comparisons/screenshots/global discovery, selected plugin block drag
and retained/reference removal. **33 recipe service cases passed / 20.74s** separately.
Source/global browsing has 39 focused save service cases
and eight actual source-save input cases.
Native minimal-INI acknowledgement and cancellation resetting that choice are tested;
the project-owned backend has no source-write acknowledgement.
External F4SE placement adds an experimental acknowledgement. Its six native journeys include
two-size fixed footers with 200 compatibility findings; the complete native rerun passes.
Standard root placement/restore adds four actual input cases with a separate root-write
acknowledgement. Enhanced root Run review verifies the ordinary loader and absence of external
overrides/root mappings, uses the project-owned filespace and cancels without starting;
all four focused cases pass /5.17s. Final controller/target/root checks pass54 cases /34.66s.
The historical optional USVFS fixture
probe passes 14 checks, including process creation identity and crash/restart cleanup.
Ruff and scoped whitespace checks pass. PID reuse is exercised with deterministic Windows
API fixtures and wrong creation tokens against live disposable processes; the run does not
force the operating system to recycle a specific PID. No actual FO4/F4SE launch is included.

The shared toolkit suite independently reproduces an existing failure:
test_settings.py::test_paths_defaults omits the current FO76 pts_root_dir default.
The manager changes neither shared settings nor that test.

Current Hyperfine 50,000-mod metadata import + repeat: **2.410 ± 0.110s**, five measured runs
(2.307-2.538s); previous checkpoint **1.735 ± 0.015s** (1.721-1.760s). These include uv
startup and generated metadata setup/cleanup on the shared host. Payloads are
not scanned/hashed/copied. A native themed 50k hierarchy capture verifies the last row with
expanded Log and toolbar/categories. An earlier capture after removing unused hover-rectangle
allocations measured 110 frames /100 warm samples: p50 **12.33ms**, p95 **13.53ms**, max
**42.78ms**; whole host RSS **267MB**, traced Python peak **50.4MB**. The current selected-target
50k capture measured **19.01ms /20.27ms** p50/p95, max **43.78ms**, whole-host RSS **262.6MB**,
traced Python peak **58.5MB**, 100 warm samples. The target selector now uses the refreshed
metadata cache rather than opening SQLite every frame. The earlier capture was
8.93ms/11.76ms at a previous checkpoint; these are separate shared-host runs, not a causal comparison.
This includes host rendering/pacing
and tracing; it is not an isolated module measurement, cold disk or real payload benchmark.
Concurrent toolkit/test work varies host load; the two checkpoints do not isolate a code
regression or establish a real-library throughput guarantee. A separate five-reference
instance import/repeat fixture measured 0.770 ± 0.249s; it is not the 50k metadata benchmark.
FOMOD context capture (two scans) plus validation on 10,002 winning paths from
20,000 generated files measured **6.13 ± 0.69s**, three Hyperfine runs including uv startup,
with setup/cleanup excluded. The earlier repeated-ancestor implementation measured
**14.44 ± 0.09s**. Containing directories are now checked before/after each fingerprint pass,
with identity pinning and per-file reparse/regular-file checks. These warm samples ran on
the shared development host; they are not a cold-disk guarantee. Context work runs in a
cancellable worker and still has substantial large-library latency.
Large solid archives, native mapping/game-start throughput and real libraries remain unmeasured.

Additional metadata-only hyperfine runs (one warmup, five measurements) include interpreter
startup and generated fixture construction: 50,000 combined entries/hierarchy plus 10,000
cached visible calls took **598.4 ± 3.5 ms**; two bounded reads and comparison of a generated
254-full/4,096-light save took **445.7 ± 35.9 ms**, with a reported statistical outlier.
These do not measure disk inventory, real archive decompression, game start or save bodies.
Commands are in tests/benchmark_metadata_adapters.py; temporary JSON reports and the viewed
save-plugins-current-1280.png / combined-data-current-1600.png captures remain in mod-manager-qa.
Raw reports/captures are under the session's temporary mod-manager-qa folder.

## Architecture, references and licensing

Dedicated modules separate MO2 metadata, SQLite, cancellable workers, package/FOMOD planning,
owned transactions, loose/archive provenance, save plugin inspection, explicit metadata adapters,
tree/status UI and executable/native-helper runtime.
Existing tooling had MO2/archive utilities but no cohesive profile library/installer.
UI presentation stays in `ui/mod_manager`; shared services and Rust code live in
`py_creation_lib`. Shared integration paths connect
normal toolkit navigation, archive intake, standalone variant, early native helper dispatch and
private native payload/build preflight, and distinguish Mod Builder.
Shared editor/record/session/style files are untouched. Game paths use manager/toolkit settings,
never .env.

No MO2 implementation/artwork was copied.
[MO2 core](https://github.com/ModOrganizer2/modorganizer/blob/master/LICENSE) and its
[official FOMOD plugin](https://github.com/ModOrganizer2/modorganizer-installer_fomod/blob/master/LICENSE)
are GPL-3.0-or-later. Project-authored code is GPL-3.0-only; third-party notices remain intact.

usvfs - User-Space Virtual File System, Copyright (C) Sebastian Herbord

[USVFS](https://github.com/ModOrganizer2/usvfs) has a conditional section-7 FOSS exception
for DLL linking/unmodified binary distribution. Its full [license](USVFS_LICENSE.txt),
copyright and repository link appear in the optional legacy Settings. The separate legacy
ctypes integration can load an explicitly selected existing bundle. Normal Run uses
the original project backend and does not acquire, load or ship upstream USVFS.
Public full-toolkit packaging is blocked until the complete distribution's license route is established.
The exception does not authorize copying MO2 GPL source. External
[7-Zip](https://www.7-zip.org/license.txt) has separate LGPL/BSD/unRAR obligations; no decoder
is bundled. Further official source evidence and acceptance priorities are in PARITY_REVIEW.md.

### Local runtime versus redistribution

Source-checkout Run builds this project's original native code offline with CMake/MSVC.
The static `/MT` x64 DLL uses generic BSD-licensed MinHook/HDE, with its full preserved notices.
Cache repair publishes a fresh generation without replacing a loaded DLL. Cancellation/failure
does not publish an incomplete build; packaged onedir data includes original source, receipt,
DLL and notices. Source/runtime cache and private packaging have 13 service cases. No upstream
runtime, game executable or F4SE binary is required or shipped by this default payload.
Complete app freezing and clean-machine installation have not run. See [PROJECT_VFS.md](PROJECT_VFS.md).

For the separate optional legacy integration, USVFS's GPL text distinguishes private execution from conveying copies. Its additional
FOSS linking/distribution permission is conditional, including notices, an eligible FOSS
software license and exclusion of proprietary linked/distributed software. Selecting an
installed DLL does not grant redistribution rights or settle that condition's application
to a complete game-facing distribution. See the [pinned license](https://github.com/ModOrganizer2/usvfs/blob/v0.5.7.2/LICENSE).

The full-toolkit packaging gap is a release inventory and dependency/license review. If optional
upstream binaries are distributed, identify their builds, provenance and static/dynamic
dependencies; audit their licenses separately. USVFS's
[manifest](https://github.com/ModOrganizer2/usvfs/blob/v0.5.7.2/vcpkg.json) lists asmjit,
spdlog, libudis86 and Boost; installed builds may differ. Audit packaged Python, UI/native
libraries, archive tools and redistributable runtime requirements too. Produce required
notices/source-access material for the chosen license route and verify no prohibited
proprietary component is linked or included. PE/version checks are not that audit.
The exception expressly names DLL linking; proxy executables and other MO2-folder files
need their own applicable terms established rather than being assumed covered by DLL approval.
The root spec includes Autodesk FBX binaries; full-toolkit redistribution has not been audited.
Automatic approval review rejected removing the public build gate, citing that proprietary
dependency and absent authorization for public distribution. The gate remains and fails
before version/build mutation. This is not an upstream GPL constraint on the original backend.
Private onedir payload staging is implemented and tested, but complete app freezing/clean-VM
execution and public redistribution remain unexecuted. Onefile is blocked by unresolved
bootloader/helper process identity. Exact sources/risks are in [PROJECT_VFS.md](PROJECT_VFS.md).

### Remaining FO4/F4SE acceptance

No live game was launched. The smallest proposed run is defined in
[F4SE_REAL_BOOT_PLAN.md](F4SE_REAL_BOOT_PLAN.md): one approved user-prepared isolated Steam
folder, exact matching F4SE and verified project filespace, owned metadata-only smoke profile with all
shared mods disabled, reviewed standard root bootstrap setup, one main-menu launch/exit,
module/log/path checks and guarded restore. It requires a new approval naming the exact
paths, files/processes and expected/unmodeled runtime effects. It does not copy source mods,
load existing saves or certify script/native-plugin behavior. An enabled real modlist still
needs a separate approved scope; fixture COW does not certify all game/native-plugin write paths. Target settings/saves/output
remain separate while the shared source modlist stays at its original locations.

When creation identity cannot be queried or an old PID-only active record is encountered,
changes intentionally stay locked; there is no force-unlock or PID-kill button. Resolve
permissions/runtime lifetime independently before retrying. Legacy/irreparably incomplete
records require separate reviewed recovery after every associated runtime has ended;
do not remove active records to work around a running game.
