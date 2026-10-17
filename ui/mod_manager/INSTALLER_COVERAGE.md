# Local installer coverage and additive updates

This pass extends the existing worker/controller, SQLite library, original native filespace
helper and modern ImGui widgets. Only dedicated manager modules/tests/docs changed. Imports
retain source references; additive updates copy only reviewed incoming payloads. Testing uses
generated fixtures and real download archives in disposable libraries. Full MO2 parity, official-engine equivalence and full FOMOD
support are not claimed.

| Area | Implemented coverage | Remaining limits |
| --- | --- | --- |
| Declarative FOMOD | Existing versions 1–5 rules, two compatibility policies, 70-case schema/reference corpus, safe file planning and the real-package acceptance below | Official Qt events remain untested; C#/Python installers are rejected |
| Archive dependencies | Missing requested non-plugin paths use bounded archive inventory. INI-listed archives and standard active-plugin Main/Textures BA2 candidates can establish Active; complete FO4 inventories establish Missing | This is the local load-plan policy, not engine attachment. Nonstandard/other-adapter/incomplete inventories remain Unknown. Packed ESP/ESM/ESL files never satisfy loose plugin dependencies |
| Versions | Requested game version from PE metadata; ordinary FO4 roots discover matching F4SE using the existing exact three-build loader/core/script catalog; managed targets retain pinned readiness checks | FOSE/NVSE/SKSE and arbitrary future F4SE builds remain unavailable. Mismatched/missing inputs need explicit input or Retry. No acquisition or executable loading occurs here |
| Wizard repair | Explicit missing selected file/folder binding to existing payload in the same package; authored declarations/destination basenames retained; clear/reopen dependencies/reset-current-choices/manual controls | Existing sources cannot be silently rebound. Contradictory required radio choices block Next/planning; Reset preserves requirements. Corrected package/dependencies or explicit manual layout are necessary |
| Fresh review | Choices, complete/unknown environment, states, versions, repairs, manual root, additions, destination edits, profile/target and reviewed plan pinned before staging/publication | Identity/size/time checks are not immutable snapshots; deliberate same-size/time/identity rewrites can evade metadata guards |
| Decoration | Left/Right/RightOfImage title, six-digit RGB/eight-digit ARGB, moduleImage defaults/visibility/fade/height, worker decode and first frame | Dark/transparent titles use a readable theme fallback. Height caps at 64–240 scaled pixels and available body space. Bad optional art is a notice; decoding is not a process sandbox |
| Referenced updates | New owned additive patch directly above the reference. Association/priority publish in the same transaction; cancel/rollback/recovery include the relationship | Unmatched reference files remain; removals/full replacement are unavailable. Patch follows reference enablement only in original profile/target and disables after source association changes. Priority edits/later mods can override it |
| Package previews | Additional package sources > selected file > Preview: inert text/hex/images from inspected ZIP/7z/RAR/folders, no editor or execution | Member limit 8 MiB; text read 64 KiB/display 32 Ki characters, binary display 4 KiB. PNG/JPEG/BMP/GIF/WebP/DDS first frames use the 16-MP/1024-pixel decoder. NIF geometry rendering remains outside this inert preview contract |
| Associations | Existing startup paths and archive/folder drop enter review | ZIP/7z/RAR activation writes the real registry and is excluded from this task. Existing nxm registration is separate |

Archive inventories remain read-only through `modkit archive list`. The manager now uses
a separate bounded native member reader for BA2 and BSA previews, exposed in Archives and
`manager archives`. Compressed and uncompressed FO4/OG/TES4/Skyrim/SSE fixtures exercise text,
UTF-16, binary, PNG and DDS previews, source changes, cancellation and byte limits.

Real-package acceptance on 2026-10-10 installed eight existing MO2 download archives into
a disposable library through the supported CLI: PipBoyShadows ZIP, Continous Beam Muzzle
RAR, 90 FPS 7z, Homemaker 1.36 ZIP, NEO ZIP, BakaFramework 7z, WeaponSpecialization RAR and
Robco Dynamic Item Sorter ZIP. Nine installations passed, including Homemaker's default
and alternative Build Set Disablers selection. Published paths exactly matched reviewed
plans and all eight source SHA-256 hashes stayed unchanged. The retained report is
`db/mod_manager/installer-acceptance-20261010/report.json`. The final packaged executable
also passed all eight packages/nine variants (`frozen-report.json` in that directory),
including ZIP, RAR and 7z extraction. The repeatable harness is
`tests/installer_acceptance.py`. This is real-package installation evidence, not official
MO2 Qt execution or in-game compatibility certification for these mods.

Additive updates are selected in the ordinary Install destination control. Download destination
review still offers new owned mod/owned reinstall and does not automatically choose Nexus
replacement files. Cloning another profile does not expand the patch scope. Owned patch
reinstall/merge/uninstall use existing backup/recovery. Source changes invalidate open reviews.

Focused evidence: 15 discovery, 21 repair/review, 17 decoration, 22 inert preview and 15
additive-update cases. Eleven actual-input wizard journeys cover 1280×800/1600×900 and
100/125/150% text. The input harness reserves image bounds without OpenGL; real backend
captures separately exercise texture upload, title placement and fade. Full-suite results
and current private delivery evidence are recorded in [PROGRESS.md](PROGRESS.md).

Tests fault/cancel before publication, after payload placement and before SQLite commit;
restore association with owned content; repeat imports; change/delete archive/activation/INI/
game folder/extender components/configuration/target; and preserve original source bytes.
The generated-fixture suite does not launch a game or operate MO2. Real-package acceptance
only reads the source downloads and game metadata; it writes into disposable libraries.

The [tree probe](tests/tree_regression_probe.py) compares local baseline
`0d8446f3c093daf21a547a0ab26895c4e4e16998` without changing the checkout. Stable per-row
column scaling, wrapper access, spacing, colours and work-rectangle work moved outside the
clipped loop. For 50,004 rows, profiled draw CPU fell from 10.70 to 8.36 ms/frame (baseline
8.59; 209 calls); scaling calls fell 53,224 → 22,311. These profiling runs disable observation
only in the fixture. Renderer tails vary with tracing/pacing/other work; this CPU result alone
does not establish a frame p95 SLA. Final quiet measurements are in the checkpoint.

Three quiet pairs with observation enabled measured current p50 4.16–4.18ms and p95
4.60–5.20ms at 50,004 rows, with no consistent frame gain over baseline. Whole-host RSS
was 347.9–349.0MB. These hidden native windows exclude cold preparation, real payload/game
I/O and Nexus graphs; profiling CPU and quiet frame measurements are separate evidence.

Implementation is original; no MO2/USVFS code was reused. The official FOMOD source is
GPL-3.0-or-later. Its non-plugin any-file check is optional; our archive/loose policy is
explicit above. References: [pinned MO2 installer](https://github.com/ModOrganizer2/modorganizer-installer_fomod/blob/24f07cf7af5273efe52daa90c631a49e7e847c74/src/installerfomod.cpp),
[pinned presentation schema](https://github.com/Nexus-Mods/Nexus-Mod-Manager/blob/b6d77fc52cf9dc82c909399313ba562c938a8af2/Script%20Types/XmlScript/Schemas/XmlScript5.0.xsd),
[differential evidence](FOMOD_DIFFERENTIAL.md).

Scripted installers require a separate constrained process, bounded versioned plan/callback
protocol, reviewed owned staging, runtime/dependency/licensing decisions and cancel/rollback
tests. They have not been compiled, run or included in this contract.
