# FO4 launch targets and F4SE preparation

An ordinary selected external/preconfigured `f4se_loader.exe` now resolves to its containing
validated FO4 installation, regardless of the previously selected game folder. Compatible
x64 game/loader/runtime/scripts are required before a new local target association commits.
An existing target for that root is reused. A manager-staged loader keeps its explicit target
association and cannot be interpreted as a game root. Review displays the associated game
folder and retains the selected executable's configured title/arguments/working directory.
One logical modlist is preserved while plugin state, INIs, saves, output and logs switch to
the associated target context. Active locks and cancelled/failed preparation remain enforced.
Only disposable fixtures have validated this path; no real F4SE process was started.

One logical Fallout 4 profile shares its enabled source mods, priorities and organization
across several configured game folders. Target selection changes game Data/CCC context,
plugin validation, plugin activation/order/locks, INIs and their metadata backups, effective
runtime settings, saves, overwrite/output receipts and F4SE logs. Switching clears current
provider, archive, combined-Data and save views. Header caches retain exact source
path/stat keys. Mod payloads stay at their original referenced or library-owned locations.
Other game profiles keep their original executables and cannot be reinterpreted as FO4.

Use **Launch options > FO4 folders**, choose a folder/name and **Review FO4 folder**.
The fixed footer retains Review/Use/Cancel while details scroll at 1280x800. **Use FO4 folder**
saves a reviewed target and automatically attempts matching F4SE preparation if it is missing.
An offline, unsupported or entitlement failure leaves a saved, unready target for retry.
It never starts Fallout4.exe as a silent fallback. After private preparation, the primary
action offers **Review game-root F4SE setup**: separately acknowledged recoverable placement
of the bootstrap components, with scripts still supplied privately through the project's filespace.
**Review root restore/recovery** restores originals or removes created components and reviewed
partial temporaries; reopening never silently changes a game. Choose **Original executables** to return
to the standalone/imported executable definitions; those records are preserved.

## Bounded release catalog

This catalog governs managed package preparation. Launching an existing, explicitly
selected x64 F4SE loader uses its containing game directory without requiring a catalog
match or readable version resources. The launcher preserves those existing files.
Managed staged packages still require their matching versions and original receipts.

Verified against the [official release site](https://f4se.silverlock.org/) on 2026-10-04:

| Detected executable build | F4SE | Acquisition | Required runtime files |
| --- | --- | --- | --- |
| 1.10.163 | 0.6.23 | Official HTTPS direct archive | loader, matching DLL, steam loader, Data scripts |
| 1.10.984 | 0.7.2 | Official Nexus file 316287 | loader, matching DLL, Data scripts |
| 1.11.240 | 0.7.9 | Official Nexus file 407709 | loader, matching DLL, Data scripts |

The fourth fixed-file-version component is retained in metadata and does not select another
release. No closest-version guess is used. Missing version resources and other builds are
unready/unsupported. Known GOG, Store and Epic folder markers are blocked by this Steam-only
native adapter. Folder markers are not complete storefront authentication; unidentified
copies still require the reviewed Steam launch assumptions and the real loader's own checks.

The [0.6.23 readme](https://f4se.silverlock.org/f4se_0_06_23_readme.txt) has an inconsistent
version heading. Package identity uses loader fixed-version metadata, exact runtime DLL
name/x64 architecture and, where present, the static core version export. The
[0.7.2 readme](https://f4se.silverlock.org/f4se_0_07_02_readme.txt) and
[current readme](https://github.com/ianpatt/f4se/blob/master/f4se_readme.txt) specify their
different component sets. Every compiled script supplied under Data/Scripts is staged;
F4SE.pex is mandatory. SDK sources, PDBs and unrelated archive members are excluded.
Installed matching components are retained; unknown/custom mismatches are preserved and
reported. Presence/metadata checks do not certify an installed script set or publisher
authenticity. Cached archives remain user-trusted inputs. Nexus checks a supplied publisher
MD5 in addition to the local SHA-256 receipt; the direct archive relies on official HTTPS.

Premium Nexus acquisition uses the connected session client. Free accounts need a fresh
**Mod Manager Download** link for the exact official file; a cached official archive can
also be selected. Browser keys, private CDN URLs and API keys never enter target metadata.
Setup does not register an account or bypass entitlement. Transfers have 64 MiB bounds,
package expansion/count limits, link/traversal/case-collision guards and cancellable workers.
Ordinary failure/cancellation removes the newly generated partial stage and leaves prior
receipts/custom files intact. A crash can retain an ignored pending/orphan stage for review;
prior complete stages are retained, with a bounded retention limit. Automatic stage cleanup
and a retention-management screen are still open.

## Runtime route and safety boundary

Private state lives under `.launch-targets/<target-id>` in the owned manager library.
Shared SQLite metadata stores target definitions and per-target/profile overlays. Each target
has a derived ownership identity; installer and runtime reviews reject changed target/game
inputs. Save/output journals are recovered in their target context. Metadata backups reject
cross-target restore/removal. A library-wide process lock and active-session discovery allow
one native runtime per library. Profile/target/configuration changes remain locked through
restart until process creation identities establish that the supervised tree has ended.

Managed F4SE uses its own staged loader, explicit game executable/runtime DLL arguments,
virtual game-root component mappings and a target script provider below user mods. This is
based on the [official legacy loader](https://github.com/ianpatt/f4se/blob/ea143b5/f4se_loader/main.cpp),
its [Steam helper](https://github.com/ianpatt/f4se/blob/ea143b5/f4se_steam_loader/main.cpp)
and the [current loader](https://github.com/ianpatt/f4se/blob/master/f4se_loader/main.cpp).
It writes no real game-root files. **The managed external staging route is experimental**:
mock helper mappings and native input do not establish a real FO4/F4SE boot or DLL-load path.
Run exposes this limitation and requires a separate experimental-route acknowledgement.
Running Steam and the project's own x64 filespace are prerequisites; development builds it offline with CMake/MSVC.
No F4SE or upstream USVFS binary is shipped by the default filespace payload.

The standard alternative in `f4se_root_setup.py` places only the official loader/runtime
DLLs (including legacy steam_loader) in the selected game root. It uses the ordinary root
loader with no alternate executable/DLL arguments or root virtual-component mapping.
The package scripts remain a target-owned virtual Data provider. Concrete review, a separate
unchecked root-write acknowledgement, retained original files, per-game operation ownership,
stopped-process checking, atomic metadata guards and explicit restore/recovery are implemented.
Failed/cancelled installs and interrupted/partial-copy recovery are fixture-tested. Changed
foreign files/backups/locks are preserved and refused. Basic copied metadata/bytes are restored;
NTFS ACL/alternate-stream/extended-metadata preservation is not implemented. No actual game
boot is certified by the documented root placement. This option removes dependence on
experimental external placement without claiming the remaining game integration is proven.

Native-plugin findings are separate from provisioning. Bounded static PE export inspection
never loads or executes the inspected DLL. It reports unknown legacy declarations, rejects
wrong architecture, incompatible explicit runtime lists, newer required F4SE and a missing
declared Address Library file. Generation flags, zero-terminated lists and breaking reserves
are checked against the [official API](https://github.com/ianpatt/f4se/blob/master/f4se/PluginAPI.h)
and [compatibility checks](https://github.com/ianpatt/f4se/blob/master/f4se/PluginManager.cpp).
Passing static declarations still leaves DLL loading, addresses, external dependencies,
script behavior and save/body/companion compatibility unverified. Root-game/overwrite/mod
native providers use the effective precedence; changed files block the reviewed launch.

The original filespace now copies an affected source/game file into owned output on its first
ordinary write, and refuses direct source writes. Managed rename/delete/hardlinks and unsupported
native operations fail explicitly. Arbitrary modded tools remain blocked pending mutation/tool
coverage; experimental staging acknowledgement remains a separate requirement. It is not a
hostile-code sandbox. Native fixtures pass; actual game boot/save/loader behavior is unverified.

F4SE's readmes forbid redistribution/modification. The implementation privately acquires
unmodified runtime files for the requesting user and ships none of those files or SDK code.
Only independent metadata/path/transaction logic and generated inert fixtures were added.
MO2/USVFS distribution licensing remains documented in README.md and USVFS_LICENSE.txt.

## Evidence and open verification

### Target customization

FO4 folders now exposes Launch options, Mod source for this target, INI and save preferences,
and Remove launch target. Every mutation uses a concrete frozen review and a cancellable
metadata transaction. Removing a target archives metadata while retaining its entire owned
state and external files. The retained list restores it; pending/complete managed root setup,
foreign game-root locks and changed root receipts block removal/restore. Limits are32 active
and256 retained targets. Removal does not reclaim disk space.

Launch arguments support a deliberate subset of the
[official F4SE loader options](https://github.com/ianpatt/f4se/blob/master/f4se_loader/Options.cpp):
verbose/wait-for-close, five priorities and positive signed64-bit affinity masks. Bootstrap,
Steam hand-off, debugger, editor and realtime overrides are refused. Overrides remain separate
from F4SE bootstrap ownership receipts. Executable fingerprints and metadata are checked again
before commit; runtime signatures include effective options.

An explicit ordinary local source folder/version label and tri-state enable override can be
assigned to one stable mod ID for one target, across its logical profiles. Shared organization,
priority and payloads stay intact. Replacement references require a same-mod acknowledgement,
bounded link/collision/path/identity/size/time inventory and repeat validation. Content equivalence
and automatic compatible-version selection are not provided. The selected target's checkbox
changes its enable override. Reset restores shared defaults. Ordinary referenced writes use
first-write COW; broader mutation/loader/tool paths remain unverified. See PROJECT_VFS.md.

INI and save scopes independently select target-local or shared manager-owned state. Scope
switches retain dormant target INIs and both save folders without copying. Shared INIs are
read-only from target configuration; edit the logical profile via Original executables. Plugins,
output and logs remain target-local. Shared saves use the original owned save/backup root while
comparisons use target profile metadata. Helper requests verify the authoritative save root and
namespace before creating a runtime adapter. Actual game save routing remains unverified.
Fallout4Prefs.ini supplies the existing local display editor; physical/global game/MO2 config,
global save sharing, GPU detection and automatic graphics presets are not implemented.

Generated fixture evidence includes31 customization and11 preference services, canonical helper
mapping/tamper refusal, save copy/backup ownership, cancelled/failed commits and both bootstrap
routes with optional flags/retained root receipt restoration. Four actual-input
journeys cover the complete review/apply/cancel/remove/restore path at1280 and1600 plus preference
commit cancellation/failure. Current complete-run results and native captures are in PROGRESS.md.

The dedicated PE/target/F4SE service fixtures cover two runtime generations, repeat imports,
target switching and stale contexts, local configuration/backups/output/save paths, cancelled
and failed publication, restart retention, exact Nexus identity/entitlement/checksum mocks,
component/script requirements and static native-plugin findings. Six actual ImGui input
journeys cover review/decline, preparation/switching, failure/cancel and experimental review.
No real MO2/profile/mod/game file was changed, no actual F4SE package was downloaded and no
game was launched. Broader real storefront/build/package/native-plugin corpora remain open.
The unavailable isolated official MO2/Qt host remains an explicit differential-test blocker.

Current totals, command outputs and performance limitations are recorded in PROGRESS.md;
all changed paths are linked in CHANGESET.md. This feature does not establish full MO2 parity
or full FOMOD support.

Exact acceptance classifications and actual automatic acquisition branches are in
[F4SE_ACCEPTANCE.md](F4SE_ACCEPTANCE.md). The concrete, unexecuted smallest boot scope and
its files/processes/write risks/reversibility are in [F4SE_REAL_BOOT_PLAN.md](F4SE_REAL_BOOT_PLAN.md).
