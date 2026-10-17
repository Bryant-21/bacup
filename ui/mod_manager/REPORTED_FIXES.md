# Hands-on launcher, status and layout fixes - 2026-10-05

All changes in this continuation are dedicated manager modules/tests/docs. The four reports
were resolved with the following concrete behavior:

1. **F4SE in another FO4 folder:** the ordinary selected loader's validated installation
   becomes authoritative instead of failing on the previous game selection. Managed staged
   loaders require their explicit target association. Runtime compatibility checks remain;
   one logical modlist uses associated target-private plugin state, INIs, saves and output.
   Review shows the resolved root, preserves configured arguments and respects active locks.
2. **Empty icons/conflicts:** an independent worker automatically measures local loose and
   applicable archive providers. Winners/losers populate badges without Nexus; unavailable,
   unresolved, partial and failed scans remain visible and qualified. Stale scan results are
   discarded after context changes. F5 refreshes; UI frame access uses the cached index.
3. **Executable editing:** an obvious button beside Run opens a draft list/form with browse,
   add/edit/remove/reorder/default, arguments and working directory. Save is atomic; Cancel
   leaves the local catalog unchanged. Repeat import preserves edits, order and tombstones.
4. **Cramped tabs/UI:** app font inheritance, readable tab overflow, larger spacing, visible
   splitter/minimum pane widths and responsive launch controls. Status glyphs remain visible
   at 150% font size. Import/installer dialog metrics retain their existing tested behavior.

Complete suites pass 1,356 services, 247 actual-input UI cases and 16 daily workflows.
Focused final launch/catalog/scaled-input checks pass 18 cases. Actual native pixels and input
flows were verified at 1280x800/1600x900 and 100/125/150% fonts. Full evidence, the disposable
native tool corpus and precise performance/coverage limitations are in [PROGRESS.md](PROGRESS.md).
No real MO2 installation, game, mod archive or user profile was changed or launched as testing.

## Previous hands-on report fixes - 2026-10-04

All changes in this batch are in `ui/mod_manager`. No shared host, styling, ESM editor,
real MO2 instance, game folder or real user configuration was changed during validation.

Reproduced causes: an empty bundle was treated as a filesystem path without a setup route;
the instance augmenter injected MO2 overwrite as an ordinary reference; column headings were
painted text without input widgets; fixed small font settings and whole-row name hit bounds
caused cramped/misdirected input; status glyphs depended on unavailable font coverage; and
the manager kept its own full log in addition to Toolkit's host LogPanel. The fixes below
address those paths. The before/after native capture and actual-input checks are in PROGRESS.md.

1. **USVFS setup:** a modded Run with no trusted selection opens Runtime setup. Known
   app/selected-instance locations are checked read-only; relative paths resolve from the
   checkout/app directory. Both DLLs and both proxies must have correct PE architecture and
   version 0.5.7.2; unknown/mixed versions are refused before a candidate DLL is loaded, including
   the native helper's constructor. A separate origin acknowledgement is required. There is
   no binary download or install. File version metadata is not provenance/authenticity proof.
   The expected metadata is defined by the official [DLL resource](https://github.com/ModOrganizer2/usvfs/blob/v0.5.7.2/src/usvfs_dll/version.rc)
   and [proxy resource](https://github.com/ModOrganizer2/usvfs/blob/v0.5.7.2/src/usvfs_proxy/version.rc).
2. **MO2 overwrite:** selected and instance imports exclude its output, references and counts.
   The folder is not scanned. Unassigned named mods remain supported. Repeat imports preserve
   previously created local references rather than silently deleting local organization;
   historical output references can be removed through the existing reviewed reference UI.
   Source files and the manager's own target/profile output remain intact.
3. **Headers:** enabled/priority/name and visible metadata headers are real mouse/keyboard
   widgets with direction marks. Repeated clicks toggle ascending/descending. Ties include
   priority/name/stable ID. Hierarchies stay grouped, selection stays stable and load priorities
   remain unchanged. Status sorts source kind, stated in its tooltip. Drag order is guarded
   to ascending Priority; explicit priority edits remain available.
4. **Typography/layout:** list/header text inherits the Toolkit font by default. Existing tiny
   saved sizes migrate to inheritance; optional 18–28 custom sizes and 24–46 row density remain.
   Fixed leading columns and responsive annotations preserve the name area; the actual name
   widget stops before metadata so name click/F2 cannot accidentally select a metadata field.
5. **Status marks:** font-independent vectors replace icon-font rendering in the modlist.
   Neutral circle/dash means unknown/off/empty with precise hover descriptions; arrows,
   checks, warning triangles and differing-version marks convey measured states. Unknown
   update/endorsement data is not fabricated. Mod titles supplied by users retain their text.
6. **Log:** the manager's extra full log, toggle and private event buffer are removed. Lifecycle
   status, errors and cancellation warnings propagate once per state transition through
   `toolkit.mod_manager` to the existing root LogPanel. No extra handler is installed. Inline
   status/progress remains; host View > Log exposes the existing severity/filter/clear controls.

Validation checkpoints and capture paths are maintained in [PROGRESS.md](PROGRESS.md).
The complete MO2/FOMOD/runtime acceptance boundaries remain in [PARITY_REVIEW.md](PARITY_REVIEW.md).
Real game boot and source-write copy-on-write are still unverified/missing respectively.
