# Focused MO2 and folder-first acceptance review - 2026-10-05

2026-10-10 update: the seven requested audit areas now have shared Rust implementations
and CLI/UI entrypoints; current evidence is at the top of PROGRESS.md. This includes two
simultaneous real game menus/MCP endpoints, bounded archive member previews, complete Nexus
category import, secure remembered-key startup reconnect, durable update sweeps, nine
real-package installs and the CLI workflow catalog. Registered browser SSO remains dependent
on a Nexus-issued application reference (the project has none). Historical evidence below
retains its original date and broader MO2 parity limits.

Latest local Nexus routing/batches and frozen receipt: [NEXUS_ROUTING.md](NEXUS_ROUTING.md) and
[PROGRESS.md](PROGRESS.md). Full services/native input and final affected checks passed.
Live account/browser outcomes remain unaccepted. The latest copied private build has
inspected nonblank library pixels; packaged routing/bulk review pixels remain unexecuted. Simultaneous
instance choice and bounded reviewed account/download batches are implemented; registered
SSO, bulk stored-history resume and unattended installation remain gaps.

The requested daily controls are present for the bounded source-checkout FO4 workflow, with
specific gaps below. This is not completion of the user's full feature/UI parity request.
The review checked the current UI handlers, controller workers, ownership services and tests,
rather than inferring implementation from the earlier matrix's labels. No real game/account,
registry, MO2 source or archive was used for testing.

## Core workflow integration

| Requested workflow | Implemented UI and backend connection | Exact local limitation |
| --- | --- | --- |
| Nexus/nxm | Optional Windows Credential Manager storage and startup reconnect; exact game/mod/file requests; single matching-instance forwarding or explicit library/profile/target chooser; actual Windows peer identity, public delivery receipts, expiry/context/replay guards and close/cancel handling. Cold browser start opens the manager. | Registered application SSO requires a Nexus-issued application reference. Live OS/browser handoff remains unaccepted; protocol registration remains opt-in with previous command shown. No hostile same-user isolation or older-build IPC compatibility claim. See [NEXUS_ROUTING.md](NEXUS_ROUTING.md). |
| Updates/endorsement/tracking | Rotating next-20 reads share account snapshots; optional automatic file checks; exact Free file navigation; reviewed single/next-20 endorse/unendorse/track/untrack and pending download queue actions; target file-ID editing; new/owned-reinstall/additive install destinations | Live eligibility/account/CDN/browser outcomes remain unexecuted. Bulk stored-history resume and unattended installation are absent; payload compatibility is not inferred. See [NEXUS_ROUTING.md](NEXUS_ROUTING.md). |
| Download/install/update | Downloads and Settings expose requests, one-to-four transfers, individual/batch/all pause, range/checksum/backoff and durable resume/finalization/history removal. Imported archives open installer review; completed downloads review explicit new/owned-reinstall/additive destinations. Manual/FOMOD review, reinstall/merge/uninstall and retained recovery are connected. | Full reference replacement/removal remains absent. Additive patches own incoming files above references without copying original payloads. Review pins archive/destination/context and rolls back cancelled/failed installs; repeated imports and explicit reinstalls retain organization. |
| Tree/grouping/sorting/customization | Native clipped TreeNode hierarchy, separators/nested groups, Ctrl/Shift/Alt selection, before/after block drag, group assignment/rename/Undo, categories/tags/notes, search/sidebar, hovered field editing and clickable sort. Persisted header resize/reorder with horizontal scrolling and Save/Cancel preferences; 24 configurable command bindings with collision/stale-draft guards. Priority drag is restricted to ascending Priority. | Standard navigation/mouse gestures stay fixed. Source notices persist but a general app-event notification history is absent. Measured status sorts preserve anchors and installation priority; unknown/offline stays distinct. Version sort is metadata text. Exact new coverage and owned-file restrictions are in CUSTOMIZATION.md. |
| Profiles | Create/clone/rename/delete/restore, local INI edit/restore, metadata backups/recipes and reviewed save pair copy/backup/restore. Clone now atomically preserves every saved target overlay as well as logical configuration/organization. | Clone does not copy save/output directories. Entire save-folder cloning, payload/MO2-format profile interchange and physical/global runtime INI/save sharing are absent. Existing save pair copy controls provide explicit transfers. Save-body/F4SE-companion compatibility is not established by header comparison. |
| FO4 folders and launch | Selector/name/folder Review/Use, exact installed-F4SE discovery or matching private preparation, optional cached archive, retry after saved-unready failure. Separate root bootstrap review/acknowledgement/apply/restore/recovery; project-owned native filespace; frozen local profile Run with process supervision, protected ordinary writes and owned output. | Windows x64 Steam FO4 catalog is limited to 1.10.163/0.6.23, 1.10.984/0.7.2 and 1.11.240/0.7.9. Other builds/storefronts/games need adapters. Modded editors/generators are blocked; direct execution does not provide the local modded profile. Real bootstrap/game behavior is unverified. |
| Target overrides/preferences/removal | Reviewed optional flags, same-mod folder/version/file-ID/enable metadata, reset, independent target-local/shared manager INI/save scopes, display INI shortcut and metadata-only removal/restore. Runtime signatures and canonical save ownership/namespace checks include these choices. | Source/enable overrides apply across that target's logical profiles; order/grouping stay profile-local/shared across targets. Automatic compatible payload choice/content equivalence is absent. Substituted folders default to unknown file identity until explicitly declared; shared identity changes invalidate that declaration. Physical global sharing/GPU presets and permanent removed-state/stage/root-history cleanup are absent. |

## Local defects corrected in this review

1. The modlist's endorsement badge always claimed account state was not fetched, even when
   Information displayed a fetched result. It now shows Endorsed/Undecided/Abstained/Unknown
   truthfully and displays only author-published replacements for a known installed file ID.
   Replacement graphs are built once; the 64-key result cache is bounded. The account worker
   primes up to64 known installed identities so normal drawing does not repeat graph traversal.
2. Clone copied logical profile files/entries but lost target-local INIs/plugin overlays.
   All saved target overlays now copy in the same SQLite transaction. Cancellation/failure at
   publication rolls back both layers. Saves/output remain separate and the dialog says so.
3. The dedicated nxm bootstrap and frozen command selected the full Toolkit on cold start,
   potentially entering its authoring setup. They now choose the standalone manager. Existing
   authenticated forwarding to an already-running host remains first; no registration was
   executed during tests.

## Reconciled checkpoint - 2026-10-05

Managed rename/delete, persistent whiteouts, rollback/atomic saves and the disposable native
tool API corpus are implemented, not pending local features. Automatic loose/applicable
archive badges, the executable draft editor, authoritative external F4SE association and
readable responsive layout are also implemented. Private onedir GUI startup/normal close and
source/frozen helpers pass; the latest packaged pixel capture failed and remains unverified.
The former statements below that treated those milestones as absent have been corrected.
The in-app help and Refresh tooltip now describe the current behavior, including excluded
MO2 overwrite. Exact remaining local work and minimum external prerequisites are in
[PARITY_REVIEW.md](PARITY_REVIEW.md), separated from intentional support limits.

## Larger missing local capabilities and consequences

- **Script installers:** executable C#/Python installers are rejected. Manual layout can select
  existing packaged files but cannot reproduce generated patches/files or scripted game-state
  logic. Support needs a separate constrained installer process, a bounded callback/plan protocol,
  reviewed staging-only output and cancellation/rollback, plus a defined runtime/dependency and
  licensing policy. Running arbitrary scripts inside the app is not an implementation shortcut.
- **FOMOD context and engine semantics:** declarative versions1-5 have defined tested coverage,
  including two flag/default/encoding policies and70 original schema/reference cases. Automatic
  bounded archive context, requested game/matching FO4 F4SE discovery, explicit source
  binding/retry, required-conflict controls and bounded header decoration now work.
  Non-FO4 extenders, unknown archive attachment and additional Qt event/codec quirks remain
  open. INSTALLER_COVERAGE.md defines the tested behavior and limits.
  Official Qt execution is a separate unavailable validation host, not the explanation for
  every one of these missing implementations.
- **Native tool eligibility and cleanup:** COW, managed rename/delete, persistent whiteouts,
  atomic saves and rollback/crash recovery are implemented. The disposable original API
  corpus passes 68 assertions plus forced crash/restart in each source/frozen mode; it is not
  a certified third-party editor. Production modded editor/generator launch remains blocked.
  There is no reviewed permanent-cleanup UI for hidden namespace/interrupted-stage records.
  Additional async/section/security/native classes and other platforms are outside the current
  contract, not implied supported features. Preserve NATIVE_API_MATRIX.md when expanding it.
  Direct syscalls/loader-time mutations and external races are not certified protected.
- **General plugin APIs:** current API1 accepts explicitly registered trusted save-reader and
  archive-precedence callbacks used by workers. It is not module discovery, a security sandbox,
  an installer/game/tool/preview contract or MO2 ABI compatibility. Users cannot drop in MO2
  DLL/Python plugins. Those interfaces require versioned capability/result contracts, runtime
  ownership/write rules, lifecycle cancellation and distribution/dependency work.
- **Plugins/Data:** defined anchored free-plugin shifting, inactive owned FO4 plugin
  rename/ghost/master-flag transactions are implemented.
  Broader upstream lock-conflict equivalence, active/dependent/sidecar/cross-profile refactoring,
  ESL conversion/other flags, external-list merge remain open.
  Inert package and bounded BSA/BA2 payload previews exist. Empty owned mod creation is
  available from the right-click menu and CLI. In-game archive precedence,
  new rule execution through the actual SDK and broader formats require external corpus proof.
- **Completeness/retention:** binary Qt QVariant/typed-list decoding, general event notification
  history, full NTFS ACL/ADS restoration and public/clean-machine distribution acceptance remain
  open. Bounded persistent source metadata notices and automatic 60-second local payload/conflict
  rescans exist; neither auto-imports changed source fields. Removed target metadata,
  private stages/root originals and native hidden state retain bounded storage without a
  reviewed permanent-cleanup screen for those particular histories.

## External validation, kept separate

Real standard/external FO4/F4SE boot, DLL paths/scripts/save routing/native-plugin behavior;
live Nexus Premium/Free entitlement/CDN/account outcomes; real OS protocol registration/default
browser behavior; wider real archive/save/instance corpora;
cold payload/solid-archive/native-start timings remain unexecuted. Official MO2/Qt differential
execution remains blocked by the unavailable isolated host; do not request it again.
Real local game boots were subsequently authorized and tested; live account writes were not exercised. Licensing/dependency review
is also required before distribution; no upstream MO2 implementation/artwork or runtime binaries
were incorporated or redistributed by this work.

## Current checkpoint and next local scope

The former daily-update gaps are implemented: direct imported-archive review, explicit completed
download destinations, protected owned reinstall and status filters/sorts.28 focused services and
16 actual-input daily journeys passed, including cancellation/fault/rollback/retry/repeat and
target override guards. The user-selected original filespace is now the normal launcher backend;
37 mutation checks,23 read/write/child checks, two failed-initializer checks and18 helper
checks per source/frozen mode pass. The original tool API corpus adds68 assertions plus
forced crash/restart per source/frozen mode; third-party modded tools remain blocked.

Remaining local native work is a supported third-party tool adapter/eligibility flow and
reviewed cleanup for retained hidden/staging state. Additional APIs/platforms require an
explicit contract expansion and validation; the existing supported/unsupported matrix remains
unchanged. Private delivery lifecycle/helpers are implemented; public/clean-machine delivery
and latest packaged rendered-pixel evidence remain open. Script/general-plugin contracts remain
separate concrete capability gaps.
A real main-menu boot cannot establish full feature/UI parity. Its unexecuted proposal remains
in F4SE_REAL_BOOT_PLAN.md. Current regressions/performance limits are in PROGRESS.md.
