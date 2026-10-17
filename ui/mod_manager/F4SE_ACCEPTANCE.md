# Folder-first FO4/F4SE acceptance: exact implemented and missing scope

This answers the multiple-install request specifically. It supplements the wider MO2
matrix in PARITY_REVIEW.md; it does not convert fixture tests into a real-game claim.

## Acquisition is implemented, not a manual-archive stub

`ManagerController.apply_target()` saves the reviewed folder, then automatically calls
`prepare_target_f4se()` if matching F4SE is absent. `f4se_setup.prepare()` performs these
real production branches; tests replace only transport/package/version inputs with fixtures:

| FO4 fixed executable build | Matching release | Production acquisition | User input still needed |
| --- | --- | --- | --- |
| 1.10.163 | 0.6.23 | Official HTTPS `f4se.silverlock.org/beta/f4se_0_06_23.7z` | Internet access; no Nexus account required |
| 1.10.984 | 0.7.2 | Nexus Fallout4 mod42147, exact file316287, private session transfer | Connected account; Free also needs the fresh exact-file browser entitlement link |
| 1.11.240 | 0.7.9 | Nexus Fallout4 mod42147, exact file407709, private session transfer | Connected account; Free also needs the fresh exact-file browser entitlement link |

Premium can resolve/download through the connected session without an archive picker.
Free entitlement is enforced rather than bypassed. A cached official 7z/ZIP is an optional
offline route. Unsupported builds never use a nearby release. Existing matching F4SE is
used in place; a mismatch/custom component is preserved until separately reviewed root setup.
Failures retain the configured unready target and explain the retry requirement.

The transfer path enforces approved HTTPS hosts, response length/64MiB limits, publisher
Nexus MD5 when present, bounded safe extraction, x64 component and loader-release checks,
and optional static core runtime metadata. Target receipts store local SHA-256 manifests,
not API keys/browser keys/CDN URLs. Real endpoint/account behavior has not been exercised.

## Required scripts and legacy DLL

Every compiled `.pex` under the official package's `Data/Scripts` is extracted into
target-owned staging; `F4SE.pex` is mandatory. Source/SDK/PDB/unrelated files are excluded.
The full supplied compiled script set is an enabled synthetic target provider below user
mods. The project filespace supplies it at virtual `game/Data/Scripts`; user mod/overwrite precedence applies.
No source-mod payload is copied and no physical game Data directory is deployed into.

For 1.10.163, the prepared package must contain `f4se_loader.exe`,
`f4se_1_10_163.dll` **and `f4se_steam_loader.dll`**. The newer catalog releases require the
loader and matching runtime DLL, not the legacy Steam loader. Installed-F4SE discovery
checks required components and readable `Data/Scripts/F4SE.pex`; it does not certify the
entire installed/custom script set. Neither path proves Papyrus/script semantic compatibility.

## Standard game-root alternative is implemented

After private preparation, the primary folder action is **Review game-root F4SE setup**.
The concrete review lists the target folder and each created/replaced bootstrap component.
Only a separate unchecked acknowledgement enables **Apply root setup**. It places the
loader/runtime DLL(s) at the locations specified by the official
[legacy readme](https://f4se.silverlock.org/f4se_0_06_23_readme.txt),
[0.7.2 readme](https://f4se.silverlock.org/f4se_0_07_02_readme.txt), and
[current readme](https://github.com/ianpatt/f4se/blob/master/f4se_readme.txt).
The scripts continue through the existing native Data mappings.

The normal root loader is then used without `-altexe`, `-altdll`, external-loader placement
or virtual root-component mappings. This removes dependence on the experimental external
bootstrap route. **Documented placement is not a claim that our actual FO4/F4SE boot passed.**
Private external staging remains available and keeps its separate experimental acknowledgement.

Original root components are copied into target-owned history before replacement. A shared
library lock, per-game exclusive transaction lock, stopped-process check, fingerprints,
SHA manifests and target SQL guards protect publication. Ordinary tested failures/cancellation
roll back the bootstrap files. Interrupted writes retain an explicit receipt/lock; reopening
only reports recovery and never silently writes a game directory. **Review root restore/recovery**
freezes original/current files and retained partial temporaries before restoring/removing them.
Unrelated changed root files/backups/foreign ownership are refused. Root setup is idempotent;
it blocks reprepare/target edits until restored and forbids overlapping same-folder ownership
by another target in the same library. Another physical target remains independent.

Recovery restores file bytes and the basic metadata copied by `shutil.copy2`; preservation of
NTFS ACLs, alternate streams and extended metadata is not implemented/certified. Prefer absent
bootstrap files in an isolated test copy for first real validation. Backups are retained;
runtime/game side effects are outside this bootstrap rollback contract.

## Remaining acceptance, with classification

| Item | Current behavior | Exact remaining work | Classification |
| --- | --- | --- | --- |
| Live matching-version acquisition | Real direct/Nexus branches; exact identity/entitlement/checksum/failure mocks pass | Exercise each approved official endpoint, actual 7z package, Premium and Free entitlement flow; do not infer live success from mocks | External validation blocked by no approved live acquisition/account scope |
| Standard bootstrap/native local-profile boot | Normal root loader, private script mappings and supervision plan implemented; inert helper and actual-input tests pass | Execute approved clean FO4/F4SE main-menu boot, verify loaded F4SE/project-filespace DLL paths and owned logs, exit/restart/restore; repeat other two catalog builds | External validation blocked; exact first folder/native-build/acquisition and execution scope need user approval |
| External bootstrap route | Explicit alternative with internal overrides/root mappings and experimental acknowledgement | Actual legacy Steam-helper/current loader injection and root DLL resolution | External validation blocked; remains experimental even if standard route passes |
| Compiled script behavior | All package `.pex` files supplied virtually; mandatory F4SE.pex/payload receipt checked | Verify required script visibility and native functions during an approved disposable new-game/script exercise; broader package/custom-script corpus | External validation blocked; main-menu boot alone cannot close it |
| Native plugin/mod compatibility across game builds | Static x64/runtime-list/minimum-F4SE/Address-Library checks and honest unknown declarations; reviewed explicit per-target source/version/enable substitutions retain logical IDs and shared organization | Test actual DLL loading/addresses/dependencies per target; automatic compatible-mod selection/content equivalence remain open | External validation blocked for loading; implementation missing for automatic selection |
| Protected referenced source writes | COW/direct-source denial, managed rename/delete/whiteouts/atomic saves/rollback; 37 mutation, 23 read/write/child, 18 source/frozen helper checks and 68 API assertions plus crash/restart per source/frozen mode | Certified third-party tool adapter/eligibility, reviewed retained hidden/staging-state cleanup; broader game/tool proof. Additional native APIs stay outside NATIVE_API_MATRIX.md's current contract | Defined fixture support passes with protected generated trees unchanged; real tools/game remain unverified |
| Other distributions/builds/platforms | Exact three-build Steam catalog, Windows x64 source/private onedir; known GOG/Store/Epic markers blocked | Verified catalog/store/game/x86 adapters after a named expansion target; clean-machine/onefile/public delivery. Latest packaged pixel capture failed | Implemented private delivery is distinct from unsupported adapters and unexecuted public/clean-machine acceptance |
| Runtime settings completeness | Independent target-local/shared manager INI and save scopes, retained dormant local INIs/saves, canonical helper ownership/namespace checks; existing local Fallout4Prefs.ini editor | Physical/global INI/save sharing, hardware-derived setup and real-game save routing; minimal INI baselines still require acknowledgement | Local functionality fixture-tested; global/hardware setup missing and real routing unverified |
| Retention/preferences | Prior private stages/root originals retained; guarded restore/recovery; reviewed metadata-only target removal/restore retaining all files; bounded launch options UI separate from bootstrap receipts | Reviewed permanent root-history/private-stage/removed-state cleanup, using the existing owned-artifact acknowledgement pattern | Missing local cleanup UI; no deletion currently performed. Unrestricted bootstrap modes remain intentionally unsupported |
| Root-file metadata restoration | Bytes/basic copied metadata backed up/restored in fixtures | NTFS ACL/alternate-stream/extended-metadata preservation and corpus proof | Implementation missing/validation blocked; no complete filesystem rollback claim |
| Large real-library performance | Earlier generated 50k selected-target capture and metadata/FOMOD timings recorded | Cold real-payload inventory, solid archives, native mapping/start latency and live CDN performance | External validation blocked by absent approved corpus/execution |
| Official FOMOD equivalence | Defined declarative 1-5 behavior; 70 pinned XSD/source-derived reference fixtures; executable scripts unsupported | Execute official Qt engine on agreed corpus; implement remaining event/default/encoding/dependency/layout quirks and scripted installers if selected | External validation blocked: user confirmed no isolated host; implementation missing for listed semantics/scripts; do not re-ask for a host |

The broader unfinished screens/controls, archive previews/engine precedence,
profile payload interchange, registered Nexus SSO and extension/frozen-distribution contracts
remain individually listed in PARITY_REVIEW.md/UI_WORKFLOWS.md. They are implementation work,
not silently closed by this F4SE continuation.

The smallest real-boot approval scope is [F4SE_REAL_BOOT_PLAN.md](F4SE_REAL_BOOT_PLAN.md).
No real acquisition, game-root setup or game process was executed during implementation.
