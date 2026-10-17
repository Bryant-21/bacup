# Visible workflow map and acceptance boundaries

[Browser routing and batches](NEXUS_ROUTING.md) add startup/pasted-link destination review
for multiple live libraries, receipt checking after unknown delivery, Information > Review
selected Nexus account actions and Settings > Nexus > Review pending downloads (next 20).
Explicit confirmation precedes Apply; Pause this download batch preserves checkpoints.
Escape/Back/Cancel release local review state. Routing never starts a transfer or install.

The [Nexus controls](NEXUS_CONTROLS.md) add Information > exact replacement browser page,
reviewed account Send/Cancel; Settings > next-20 files/version/account reads and optional
automatic file checks; Target source > bound file ID; Downloads > new / owned reinstall /
additive reference review. The download and account reviews have one scrolling body and
fixed actions. Account confirmation names the connected account; no action is automatic.

The [installer continuation](INSTALLER_COVERAGE.md) adds these visible journeys:

- Install > Installation destination > Additive update: choose a reference, open package,
  Review destination files > Install into library. Incoming owned files publish directly
  above the reference; unmatched source content remains in place.
- Missing source repair > enter declaration/replacement > Bind missing source > Review.
  Clear source repairs, Reopen package and dependencies and Reset current step choices
  are explicit. Contradictory required choices remain blocked until resolved or manual layout.
- Additional package sources > selected file > Preview selected package file. Inert bounded
  text/hex/image results can be closed; every content edit still requires destination review.

Eleven new actual-input journeys supplement the existing suite. Native header/fade, repair
and preview captures cover both requested sizes; registry associations remain excluded.

This records the implemented controls against the MO2 workflows in
[PARITY_REVIEW.md](PARITY_REVIEW.md). It is a control map, not a full UI parity declaration.
The app retains its existing theme. The mod hierarchy uses clipped native TreeNode controls,
not a table widget. Tests below use generated disposable instances and actual ImGui input.
The exact remaining daily-update controls, larger local capabilities and external-validation
boundary are enumerated in [ACCEPTANCE_REVIEW.md](ACCEPTANCE_REVIEW.md).

| Workflow | Entry and resulting behavior | Evidence / remaining boundary |
| --- | --- | --- |
| App entry / first run | Existing toolkit: Workspace > Modlist Manager; standalone `--variant=mod_manager` goes to game/library setup, Detect/Create instance | Real registry/menu input at 1280/1600 and entrypoint/settings isolation; shared authoring wizard flag remains untouched. FO4 context, optional MO2. Wider detection/preferences open. |
| Import | Import MO2: original folder, mappings, preview/import or selected owned copy | Read-only metadata/reference import; all profiles/repeats/removal exclusions, bounded cancellable migration. Broad real-instance corpus open. |
| Columns/bindings | Drag header edges or headers; Settings > Configure mod columns / Configure shortcuts | Persisted table-free geometry, horizontal custom layout, bounded sliders/order drafts and command binding collision/Save/Cancel/stale checks. Actual header drag/grip and typed binding input at varied sizes/fonts. |
| Source notices | Source metadata changed button or Settings > Source metadata notices > Review fields / Dismiss | Bounded read-only metadata batches; persistent notices, strong hashes, stale-review invalidation and guarded dismissal. First observation is a baseline; large libraries rotate. Binary Qt values/general event history remain open. |
| Owned plugin files | Plugins > select owned plugin > Owned plugin file actions > Review / Apply / Cancel; guarded Undo | FO4 inactive-file rename/ghost/unghost and non-light ESP master flag via staged CLI. Single-file journal/backup, crash/cancel/rollback and unexpected-content preservation. References, core, active/shared providers and dependent/sidecar rename sets are refused. |
| Daily layout | Profile above left hierarchy; executable/Run above Plugins/Archives/Data/Saves/Downloads/Information; inline status and the existing host Log | Toolkit font inheritance, vector badges, real sortable hierarchy headers; [reported fixes](REPORTED_FIXES.md). All MO2 fields/menu equivalents remain open. |
| FO4 folders | Target selector and FO4 folders > name/folder > Review > Use; matching F4SE discovered or automatically acquired/prepared privately, cached archive optional; prepared targets offer Review game-root setup > separate acknowledgement > Apply, and reviewed root restore/recovery. Original executables restores imported definitions. | Target-specific INIs/plugin state/backups/saves/output/context, exact three-build catalog, rollback/repeat/stale/restart fixtures. Six original target input journeys plus four root setup/restore/cancel/failure journeys. Standard root placement removes dependency on the experimental external route. Both real boot and live acquisition remain unverified. Exact gaps/approval scope: F4SE_ACCEPTANCE.md and F4SE_REAL_BOOT_PLAN.md. |
| Mod organization | Ctrl/Shift/Alt selection, ordered priority/group block drag, separators, tags/notes/categories, filter sidebar, Undo | Frozen selected blocks preserve selection; before/after drop and group assignment have stale database guards and Undo. Broader icon/customization corpus open. |
| Mod fields/removal | F2 edits hovered title/priority/category/version/notes/Nexus ID; status opens Information; Delete reviews owned removal; Actions/context reviews references/separators | Bounded shared metadata and profile-local priority, fixed-footer review/cancel/save, repeat/stale/cancel/failure tests; changing mod ID clears file ID. Source payloads intact. |
| Group rename | Hover group > F2 or group context > Rename local group, including groups without a separator | Profile-local descendant rename with collision/stale/cancel/failure checks, repeat-import preservation and guarded Undo. Mod titles/source folders/other profiles remain intact. |
| Reference repair | Mod context > Relink source folder..., review replacement and acknowledge same mod; Undo source relink | Shared local pointer changes across profiles; bounded identity/size/time inventory, links/library overlaps rejected; stale/cancel/failure/Undo/repeat/reopen. No content-equivalence hash or filesystem snapshot claim. |
| Source metadata | Mod context > Refresh source metadata..., choose optional categories.dat and individual fields; Undo metadata refresh | Version/notes/tags/categories/atomic Nexus association start unchecked. Missing/unsupported fields preserve local values; merged installedFiles ambiguity is explicit. Bounded hashed input rereads, full local context guard and atomic rollback/persistent Undo. Automatic observation/remaining Qt types open. |
| Plugin priority | F2 over hovered priority or plugin context > Edit draft priority; Ctrl+click slider for exact value; Use reviewed draft priority | Frozen complete draft including hidden rows; headers/master/locks validated, cancel/stale/failure/core/locked guards. Existing separate Plugins review/apply persists local files. Other plugin field semantics remain open. |
| Profile lifecycle | Profiles: create/clone/rename/delete; Removed profiles restore | Local configuration/organization and saved target overlays clone atomically; cancelled/failed publication preserves both layers. Saves/output stay separate. Source MO2 stays intact. |
| Profile INIs | Actions > Profile configuration; FO4 folders > INI and save preferences | Local edit/stale guard/previous baseline restore; independently choose target-local/shared manager INIs and saves. Shared target INIs read-only; edit original logical profile. Local display INI shortcut; physical global/hardware setup open. |
| Target customization | FO4 folders > Launch options / Mod source for this target / INI and save preferences / Remove launch target; reviewed restore in retained list | Frozen worker Review/Apply/Cancel, same-mod source acknowledgement, target enable policies and bounded F4SE arguments, metadata-only removal/restore retains files.31 customization/11 preference services, four native journeys including cancelled/failed apply. Real boot/save routing unverified. |
| Profile backups/recipes | Actions > Profile backups / Profile recipes | Metadata backup/restore/remove and explicit cross-library mapping/recipe transfer. Payload transport/MO2 recipe interchange open. |
| Installer | Install/Ctrl+M: archive/folder, manual tree or declarative FOMOD wizard; Review before owned install/reinstall/merge | Safe archive paths/limits/cancellation/rollback; fixed navigation/install footer. Full FOMOD certification and executable scripts open. |
| Package intake | Startup ZIP/7z/RAR/FOMOD path or one archive/folder dropped on manager | Queues until library/profile/worker are ready; prepares review without auto-install. Decline/corrupt/multiple/pending-file input cases preserve state. OS registration of associations remains open. |
| File management | Information > Files / Data origin controls | Providers, folder reveal, owned hide/unhide with retained replacement; source references cannot be directly deleted. Archive previews/empty-only folder controls open. |
| Plugins | Details reads CLI headers; draft activation/indices/locks/order; Review/Apply/Cancel | Core/master/dependency/cycle/count validation, selected-block keyboard/drag and stale gesture guards. Strict locks differ from MO2 shifting behavior. |
| Archives | Refresh/index, filters/member hierarchy, candidate overlaps/provenance | CLI archive reader and cached bounded metadata. Wider BSA/BA2 variant/activation corpus open. |
| Combined Data | Data > Read loose + archives; archive/overlap filters, provider popup and archive navigation | Effective FO4 INI/plugin prediction, pinned inventory, unresolved cases retained explicitly. In-game precedence proof open. |
| Saves | Saves > Save locations: owned, MO2 source, Browse, Default global FO4 saves | Explicit read-only inventories; Windows Documents resolver is fixture-replaced in tests. Customized INI/Store default discovery open. |
| Save inspection | Select save > Inspect save header / Inspect saved plugins; compare Current local profile or another save; Preview/Hide screenshot | FO4 header versions 11/12/13/15 and supported light table layout; pinned bounded metadata/images/cache, fixed footer. Save body/companion compatibility open. |
| Save transport | Copy/back up or Import source; Save locations > Save backups > reviewed restore | Hash/identity/companion checks, no replacement, retained receipts and interruption recovery. Sources remain intact. |
| Retained cleanup | Save backups > Review save backup deletion; Owned overwrite > Review output backup deletion; Recovery > Review permanent backup deletion | Acknowledgement, rollback before commit, guarded committed cleanup/reopen and unexpected-content preservation; native fixed footer at both sizes. |
| Generated output | Actions > Owned overwrite: Review, Copy to owned mod, Clear with backup, Restore into empty output | Manager-owned output only; MO2 overwrite excluded from import/scanning. Managed rename/delete/whiteouts/atomic saves are implemented; 37 mutation checks and the 68-assertion source/frozen tool API corpus pass. Arbitrary third-party tools remain blocked; NATIVE_API_MATRIX.md defines support. |
| Executables/runtime | Edit executables beside Run: atomic draft list/form, browse, add/edit/remove/reorder/default/Save/Cancel; selected Run opens review | Ordinary F4SE selects its validated containing FO4 root; staged loaders use explicit association. Integrated editor/review/status flows pass at varied sizes/fonts. Private frozen startup/normal close/helper checks pass; latest packaged pixel capture failed and is unverified. Real boot, other games/x86, certified tools and public/clean-machine delivery remain open. |
| Downloads/Nexus | Downloads pane; pinned Nexus intake; simultaneous-instance destination/receipt review; next-20 reviewed account/download actions; one-to-four transfers and per-request/job/batch/all pause; exact Free files-page navigation | Mock API/CDN/range/ETag/hash/restart/finalization, real disposable Windows IPC, private clients/locks/backoff and actual input. Finished downloads review new/owned-reinstall/additive destinations separately. Registered SSO and live account/OS/CDN acceptance remain unverified. |
| Status filters/sorts | Automatic worker publishes loose then applicable archive conflict states; Refresh files/F5; sidebar conflict/update/endorsement/tracking filters and display sorts | Local file status needs no Nexus. Partial/error/unavailable states remain qualified, stale results are discarded, tree anchors retained and display order does not change install priority. Integrated badges and size/font input journeys pass; engine precedence remains predicted. |
| Preferences/extensions | Settings: density/metadata/categories/runtime; Metadata adapters inventory | Trusted explicit API-1 save/archive adapters used by real workers. General game/install/tool ABI, binary plugins and packaging open. |
| Help/status/cancel | Existing app Help and manager help; bounded show/clear Log; workflow Cancel operation, Escape or Back to Library | Busy/focus/typing/disabled input covered at multiple sizes. A broader notification system is open. |

The service suite and [native input suite](tests/test_widget_ids.py) must run separately:
shared toolkit fixtures replace native bindings during service collection. Latest results,
commands, performance limitations and licensing are in [README.md](README.md).
Successful generated-fixture checks do not establish real game/account/installer-engine parity.
