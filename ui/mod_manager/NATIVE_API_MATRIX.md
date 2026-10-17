# Owned filespace API acceptance

This is a tested Windows x64 overlay contract, not full MO2 or NTFS parity.
Generated source/game files are immutable references; all supported mutations affect
manager-owned output. Windows 11 Pro 10.0.26200 is the tested host. Other Windows builds,
real FO4/F4SE boot and third-party editors/generators still need separate acceptance.
An original disposable API corpus now passes 68 assertions plus forced crash/restart
through both source and copied frozen helpers; this is bounded coverage of the rows below.

| API or behavior | Implemented contract | Actual fixture coverage |
| --- | --- | --- |
| NtCreateFile / NtOpenFile; Win32 and Python file I/O | Frozen winner, owned output, game fallback; merged CREATE_NEW collision; fresh output safety checks; staged per-file copy-on-write | Selected/physical winners, append/truncate/new writes, metadata, case/dot/extended DOS paths, sharing denial and release |
| NtQueryAttributesFile / NtQueryFullAttributesFile | Current owned namespace; hidden entries do not return lower metadata | Virtual metadata and deletion visibility |
| NtQueryDirectoryFile / Ex | Synchronous merged snapshot, restart/cursor, classes 1/2/3/12/37/38; basic `*`/`?` matching | Merged and nested enumeration, opaque recreated directories, hidden entries; 50,000-entry workload |
| NtSetInformationFile class 10 / 65 (rename) | Classic replacement boolean and Ex flags 0/1; absolute and tracked-root-relative destination; same owned volume; existing winner replacement; persistent old-name whiteout; caller's own exclusive lease is excluded from its sharing check | Source winner rename, collision without replacement, root-relative Ex rename, temp-to-Data replacement, rename into a deleted name; exclusive Win32 SetFileInformationByHandle rename |
| Directory rename | Materializes the visible merged subtree into owned output; moves descendant whiteouts/opaque state; destination must be absent; no other open directory/descendant handles; reused deleted destination receives the new generation's hidden state | Mixed owned/source/game children, nested file, open-directory refusal, failure rollback/crash recovery and repeated generated-tree publication |
| NtSetInformationFile class 13 / 64 (delete) | Classic delete/cancel and Ex flags 0/1; committed whiteout; new opens denied while the deleted owned generation has leases; kernel unlink waits for handles | Delete winner, delete/recreate, repeated delete, cancellation, final close and new-session persistence |
| FILE_DELETE_ON_CLOSE / NtDeleteFile | Owned backing and the same classic deletion policy; never unlink referenced source/game files | Delete-on-close read handle, whiteout after close; Win32 DeleteFile/RemoveDirectory and Python unlink |
| Directory delete | Only an empty merged directory; cleans already-hidden owned remnants; recursive deletion requires child-first calls | Nonempty refusal, empty source directory, nested child-first deletion, deletion crash before the kernel flag |
| Existing file handles | Pin their backing generation. A lower reader remains on the original bytes after copy-up/replacement; future opens see owned output | Pinned old reader, sharing protection, duplicated pending-delete handle and final-close recreation |
| Same-process NtDuplicateObject | Noninheritable duplicates; one lease remains until the last duplicate closes; close-source cleanup | Duplicate lifetime and pending deletion |
| Multiprocess namespace | Owner-wide named mutex, shared epoch and bounded leases with PID + creation identity; persistent state keyed by owned path, independent of launch session | Two simultaneous injected processes: denied conflicting write, rename publication and observer refresh; fresh-session whiteout |
| Rename transaction failure | Owned destination backup plus flushed atomic journal/state publication. A failed commit rolls back; next lock/start recovers an interrupted physical rename. Unexpected identities or blocked recovery fail closed | Forced failed commit and process termination between physical move and commit, for files and directories |
| Owned TEMP/TMP | Helper supplies a self-mounted profile temp directory on the output volume; supported temporary-file replacement uses this namespace | Actual Python tempfile + os.replace through the normal planner/helper |
| Basic Win32 save/copy composition | Supported hooked primitives compose MoveFileExW replace/write-through, basic ReplaceFileW with backup and CopyFileW; no blanket support for every flag | Owned temp atomic save, replacement/backup bytes, copy merged winner and fail-if-exists collision in repeated tool runs |
| File-backed mapped views | Supported ordinary COW/writable backing handles; owned PAGE_READWRITE persists flushed bytes, PAGE_WRITECOPY leaves backing bytes unchanged, readonly handles reject writable mapping | Flush/unmap, close original file handle while view remains live, private view, forced process crash with live flushed writable view and fresh-helper restart |
| GetFileInformationByHandleEx directory query | Existing synchronous merged query classes include FileIdBothDirectoryInfo/restart; unknown native directory class fails closed | Merged handle query and restart, unknown NtQueryDirectoryFile class60 STATUS_NOT_SUPPORTED |
| Child creation | Fresh suspended owned x64 child, same manifest, injection before resume; owned job/creation-identity supervision | Child/grandchild mapping, child outliving parent, cancelled start and helper-crash cleanup |

Lease capacity is **1,024 live managed file objects per owner**, including directories;
duplicates of one tracked object share a lease. An exhausted table fails before new-file
creation, and closing handles restores capacity. Tracked output paths must be shorter
than 2,048 UTF16 characters. Directory materialization is bounded at 20,000 entries and
512 MiB of visible file content; replacement backups are bounded at 512 MiB per file.
Namespace records are bounded at 100,000 combined entries
and 16 MiB. These are deliberate fail-closed limits, not unlimited large-tool support.

Whiteouts remain persistent until the corresponding name is explicitly recreated or
replaced. Recreated deleted directories are opaque: their lower children remain hidden.
Existing directory enumeration cursors retain their snapshot until restart. Classic
pending deletion is hidden from overlay enumeration/metadata before final close; this
visibility is the defined overlay policy rather than complete NTFS equivalence.

Failure controls exist only in the separate CMake `MODBOX_VFS_TESTING=ON` build.
The production builder explicitly selects `OFF`; packaged payloads do not export these
controls. Fixtures terminate only their retained, newly created process handles.

Owned replacement uses the kernel's POSIX unlink capability internally to detach an old
permitted reader while publishing the new generation. The virtual API still requires
share-delete and refuses destination writers; it does not expose arbitrary POSIX flags.
Disposition on an already-detached handle operates only on that old object, or fails if
the kernel cannot resolve its name. It cannot hide or delete the newer named generation.

Unsupported modes include rename Ex flags beyond replacement (including POSIX), case-only
rename, hardlinks, disposition Ex POSIX/ON_CLOSE/readonly/image overrides, directory
replacement and rename with other open descendants. Unowned external temporary handles
cannot move into managed/protected namespaces. Foreign-source handle duplication and disk
handle export are refused; a child cannot inherit a managed handle, even if its inherit bit
was changed after open. Open-by-ID, UNC/device namespaces, x86, directory APC completion,
additional directory classes/flags and broker/elevated creation remain unsupported.
Directory filters follow FsRtlIsNameInExpression, including the DOS forms `<`, `>` and `"`
that FindFirstFileW sends for patterns such as `*.dll`; the fixture's
`dos_wildcard_enumeration` check covers them. These combinations fail instead of writing to game or source files.

Source/output roots must be trusted ordinary local directories. This user-mode interceptor
is not a hostile-code sandbox or a race-free kernel filesystem. Direct syscalls, unhooked
brokers, loader-time changes, external concurrent output edits, full ACL/ADS behavior,
power-loss durability and broad mapping/section/async tool behavior are not certified.
Handle-name queries can expose the owned backing name. Backups preserve bytes and basic
timestamps/attributes;
a rollback can restore a destination with a different file identity while old handles keep
their detached generation. Hidden records and interrupted staging data have bounded storage
but no permanent cleanup UI yet.

Reference contracts: [FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information)
and [FILE_DISPOSITION_INFORMATION_EX](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/ns-ntddk-_file_disposition_information_ex).

The disposable API corpus passes in source and copied frozen execution. It also verifies
injected child output, protected-source readback, truncate/basic attributes, and explicit
hardlink/ADS/direct-source-write refusals against generated trees. This is original test
code, not execution of a certified third-party editor. Exact mapped image/section/security/
async variants, broader sharing/Windows versions and trusted actual-tool integration remain
open. An explicitly approved generated-mod FO4/F4SE boot remains unexecuted and must not
touch imported mods or saves. Fixture evidence and reproduction are in PROGRESS.md.
