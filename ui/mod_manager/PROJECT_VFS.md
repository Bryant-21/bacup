# Project-owned virtual filespace

The user explicitly selected **our own virtual filespace**. This supersedes upstream USVFS
provisioning. Default launch now uses original ModBox21 native interception; upstream
USVFS binaries are not its required dependency. Native source is in `native/` and the Python
ownership adapter is `project_vfs.py`. No upstream USVFS implementation code is reused.

The current milestone adds managed rename/delete, persistent whiteouts, atomic saves using
owned TEMP/TMP, shared leases and file/directory rollback/crash recovery. **37 actual mutation
checks** pass alongside23 read/write/child and two initializer-failure checks. Exact API
classes/flags/bounds and generation semantics are in [NATIVE_API_MATRIX.md](NATIVE_API_MATRIX.md).
Private onedir GUI/library restoration and the18-check frozen helper are verified; wider
tool/game/Windows acceptance remains open. Current run details are in PROGRESS.md.

## Verified native milestone - 2026-10-05

Built locally with MSVC19.51, Windows SDK10.0.26100 and static `/MT`, on Windows11Pro
10.0.26200. No runtime download, driver, global hook, security-setting change, privileged
installation or game launch. Windows10+ is the bounded target; other OS builds are untested.

The actual injected C++ fixture passed **23 native checks**: selected winner reads; case/dot
and extended DOS paths; nested virtual files; merged/nested enumeration; metadata;
CREATE_NEW collision; first-write copy/readback; direct source writes denied; source bytes
preserved; physical game-file COW; new output directory/file visibility; exact owned-runtime snapshot writes; delete/recreate;
unmanaged rename/hardlink into protected Data denied; ordinary outside rename allowed; child
and grandchild propagation. Python also verified unchanged generated source/game bytes.
Two actual initializer-failure checks verified malformed manifest rejection, no unhooked
fallback and reaping of the exact newly-created supervised process.

The complete default planner/helper passed **18 checks**: priority/hidden/disabled state,
plugin snapshots, local INIs and owned configuration writes, projected user-directory temporary
files, isolated saves, owned output, source COW, Python child mapping,
unchanged source/game/user bytes, children outliving parent, exact creation identities,
pre-creation cancel/no child/unlock, helper-crash owned-tree termination and restart unlock.
The normal UI uses this backend even when an old upstream bundle preference exists.
Minimal INIs and experimental F4SE retain separate acknowledgements; source-write acceptance
is unnecessary for this backend. Editors/generators remain blocked.

Evidence under the Windows Temp directory: mod-manager-qa/project-vfs-expanded-native.json,
project-vfs-helper-native.json; modbox-project-vfs-expanded-probe.log and
modbox-project-vfs-helper-probe.log. Ignored manual build: cache/mod-manager-native/Release.
This is actual native API interception, not a launcher-only helper.

The full service run passed1338 /420.48s; the actual-input suite passed243 /210.20s, plus
16 daily-input journeys /19.59s. Shared routing/restore/quit passed33 /1.52s in an earlier separate
process. After final runtime error-text/legacy-constructor fixes,63 focused services passed
/17.23s. Cache/source/license/package coverage now has13 service cases; final results are
in PROGRESS.md. Private onedir freezing/GUI/helper acceptance passed; clean-machine installation remains unexecuted.

## Local build and packaging

Development builds offline with CMake and Visual Studio18/2026 x64 BuildTools. A cross-process
cache lock serializes builds, and compilation runs in an owned kill-on-close job with a
four-minute bound. Receipts pin source/ABI/DLL and both GPL-3.0/BSD notices. Repeated use validates
the cache; repair publishes a fresh generation without replacing a loaded DLL. Cancellation
or failure publishes no incomplete build. Old/unpublished generations remain retained; no
permanent cleanup UI exists. Packaged onedir data includes DLL, receipt, original source and
both license texts. Private onedir GUI/helper acceptance passed. Onefile identity and clean-machine delivery remain unverified.
The existing public full-toolkit build gate remains pending a distribution audit, including
proprietary FBX; it is not a GPL restriction on this original backend.

## Performance evidence and limitations

Hyperfine measured the 50,000-map alias fixture with one warmup and three runs:
command mean36.906s +/-2.901s, range34.831-40.220s. The last native phase sample recorded
10,000 open/read/close operations in3009.85ms, 10,000 metadata requests in1859.45ms,
full50,000-name enumeration1491.15ms, fixture working set46,358,528bytes. A local per-query
metadata cache avoids repeated ancestor checks for aliases to one physical file.

Those50,000 mappings initially reference **one generated7-byte backing file**, so this
cache particularly benefits that case. Native timings exclude setup/manifest/injection;
command timing includes generation/configuration/injection/cleanup and uv startup. Shared
host load is not isolated. The final separate50,000-distinct-file benchmark passed three runs (before the final
exact-snapshot/user-root hardening, which leaves its mounted Data branch unchanged): command mean66.816s
+/-1.662s, range65.616-68.712s. Last native phases:10k reads3748.09ms,10k metadata2344.46ms,
full enumeration4374.36ms, working set72,704,000bytes. Snapshot ancestor caches trade memory
for fewer repeated metadata checks; no cache outlives its query/manifest validation. Opens
and writes retain fresh full checks. Earlier distinct enumeration13514.51ms is historical;
shared host load prevents treating the difference as an isolated causal benchmark.
Evidence: Temp/mod-manager-qa/project-vfs-native-50k-unique.json and
project-vfs-native-50k-unique-command.json; log modbox-project-vfs-benchmark-unique.log.
Full50k unique enumeration still takes seconds; faster cold/real-library startup remains work. Neither scenario measures cold real assets, archives, game/renderer load,
network or a UI SLA. The final50,004-row current warm native UI capture recorded p50/p95 4.09/5.23ms,
whole-host RSS315.9MB and the last row visible. It excludes setup/API/graph priming, cold
import/payload/native start and is separately bounded in PROGRESS.md.

## Architecture and dependencies

The helper creates its own suspended x64 process in a kill-on-close Windows job, injects the
project DLL and initializes an immutable UTF16 mapping manifest before resuming the main thread.
The DLL hooks NtCreateFile/NtOpenFile, directory enumeration, path metadata, close/duplicate and
selected mutation APIs. Reads combine frozen priority winners, physical game fallback and
highest-priority dynamic output. Owned runtime snapshots are writable exact mappings;
INI/plugin folder mounts route temporary files into owned storage. Ordinary files remain in place. A first write stages only that file outside virtual mounts and atomically publishes
it into owned output; subsequent handles operate on that file. New files/directories route
to owned output. Protected source/game/user roots reject unprojected writes; reparse points/ADS/output hardlinks
are refused. Standard Win32 child creation starts suspended, receives the same manifest/DLL and
then resumes. Unknown native process creation fails rather than creating an unhooked descendant.

The original injector finds the target's mapped system ntdll image; it does not assume identical
ASLR addresses or attach to a PID/name. A bounded original x64 thunk calls the native loader,
then a separate remote thread initializes hooks outside DllMain/loader lock. Failure keeps the
new process suspended or terminates it; existing exact creation-identity/job supervision remains.
LdrLoadDll is a narrow native ABI dependency requiring continued Windows-version tests.

MinHook1.3.3 is a generic hook primitive, copied from the clean locally vendored source.
Its full BSD-2-clause MinHook/HDE notices are preserved in `native/vendor/minhook/LICENSE.txt`
(SHA256 d5c2224982b0b95a16b098a561335fa93f3eaf4d0aa964b7843edb986df78dc8).
The filespace/injection/fixture code is original project code under the repository GPL-3.0 license,
preserved in py_creation_lib/native/mod_manager_vfs/LICENSE.txt and the runtime Project-LICENSE.txt.
No CommonLib submodule, umbrella Rust extension, ESM editor or record/session service is changed.

Official reference contracts: [NtCreateFile](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile),
[NtQueryDirectoryFileEx](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntquerydirectoryfileex),
[process creation flags](https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags),
[CreateRemoteThread](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createremotethread).

## Exact current limits

Managed rename/delete, persistent whiteouts, owned atomic replacement, bounded sharing,
directory materialization and crash rollback are now implemented. The exact supported
classes, flags, bounds and handle policy are in [NATIVE_API_MATRIX.md](NATIVE_API_MATRIX.md).
Hardlinks, inherited managed handles, foreign-source duplication/disk-handle export,
directory APC completion, additional information classes/query flags,
UNC/device namespaces, open-by-ID, x86/cross-bitness and broker/elevated
child creation are not implemented. Unsupported intercepted mutations/process creation fail
closed. This is not a hostile-code sandbox: direct syscalls or uninstrumented/broker processes
and mutations during loader initialization are not certified protected. Handle identity/name
queries can expose backing paths; broad mapped DLL/section/async/game/tool behavior is unverified.
External concurrent source/output changes are not filesystem snapshot isolation; reparse/hardlink
checks are defense in depth, not race-free kernel enforcement. Full NTFS ACL/ADS and engine
archive/load-order behavior remain outside the native milestone.

A disposable original editor/generator API corpus now passes through source and copied
frozen helpers: 68 assertions over three runs plus a forced crash/restart. It verifies
atomic/temp saves, basic ReplaceFileW backup, exclusive handle rename, generated-tree
publication/republication, child output, handle-based merged directory queries, writable/
private mapped views, truncate/attributes and protected-source refusals. All generated
source/game/user paths and bytes remain unchanged. PROGRESS.md records evidence and the two
corrected rename/hidden-generation defects; NATIVE_API_MATRIX.md defines the tested contract.
This does not certify arbitrary third-party editors, all mapping/section/async/security
APIs or a hostile-code sandbox. Real FO4/F4SE boot, broad Windows/tool/game corpus,
clean-VM installation and cold real-payload performance remain unexecuted.
