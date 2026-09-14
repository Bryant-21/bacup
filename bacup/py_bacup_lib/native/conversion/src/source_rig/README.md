# Source-rig creature scaffold

This subtree is an offline contract and XML source emitter for creature rigs that
remain on their original skeleton. It does not retarget a creature to a Fallout 4
donor rig.

The API is source-game-neutral. A source adapter ends at `CreatureManifest`:

- a Skyrim adapter supplies the decoded HKX skeleton order, converted clip paths,
  transform-track mappings, and recovered annotations/events;
- an FNV/FO3 adapter supplies the same fields after KF controller and compact
  B-spline decoding against the source skeleton.

Both adapters must produce parent-before-child `BoneDecl` entries and ordered
`ClipBinding::transform_track_to_bone_indices`. From that point onward, the
emitters and packers are identical. Source decoding and clip/skeleton conversion
intentionally remain outside this module. `ClipBinding::skeleton_path` is the
runtime rig path; an animation's source-format `originalSkeletonName` remains
adapter evidence and is not substituted for that path.

Clip bindings may be sparse. Every listed transform-track mapping must be unique
and in range, and its count must match `declared_transform_tracks`, but bone 0 is
not mandatory. This preserves valid FNV KF-derived bindings such as an 84-track
Gecko clip mapped into an 87-bone rig beginning at Pelvis.

The manifest keeps the two skeleton roles separate:

- `visual_skeleton_nif` is the `Skeleton.nif` selected by the FO4 `RACE` record.
  It contains the visual hierarchy, attachment nodes, bounds, character
  controller, and ragdoll data.
- `animation_skeleton.path` is the FO4-ready `Skeleton.hkx` used by the Havok
  character and clip bindings. It is a named/hierarchical subset of the NIF rig.

`emit_idle_scaffold` validates the manifest and emits deterministic classversion-11
XML sources for the project, character, root graph, and core graph. The root graph
contains a `BSBehaviorGraphSwapGenerator` with `userData=1`; the core graph loops
the selected Idle clip. The project explicitly lists the character, root, core,
and animation files. Generated artifacts carry both their eventual runtime `.hkx`
path and their editable `.xml` source path.

`pack_idle_scaffold` writes those four XML sources, packs them through
`havok_native`, rereads each AMD64 packfile, validates its classversion-11
`hk_2014.1.0-r1` header and round-tripped XML, and returns a runtime manifest plus
an artifact report. The Idle MVP emits an empty `ragdollName` and has no ragdoll
runtime path.

`MvpGraphManifest` maps five arbitrary `ClipDecl::name` values onto
the source-neutral runtime roles Idle, WalkForward, TurnLeft90, TurnRight90, and
Attack1. `emit_mvp_scaffold` and `pack_mvp_scaffold` then produce the same four
artifacts with a deterministic state machine:

- Idle is the default looping state;
- `startWalk` enters the looping WalkForward state and `Idle` returns it;
- `TurnLeft90` and `TurnRight90` enter acyclic clips that abut back to Idle;
- the manifest-selected lowercase `melee*` event enters acyclic Attack1, which
  abuts back to Idle when the clip completes.

The default MVP is graph-driven. `MvpMotionManifest` is the path for a
serializer that recovered extracted planar reference frames. Each role
declares both its extracted-frame count and whether it is animation-driven;
validation rejects either `animation_driven=true` with zero frames or nonzero
frames on a graph-driven role. This lets a null-motion Skyrim wolf keep all five
roles graph-driven while an FNV Gecko can mark only WalkForward and Attack1.

When any role is animation-driven, the emitter adds the live FO4 declarations
`startAnimationDriven` and `bAnimationDriven` to both root and core graphs. Only
the selected states send the correctly cased entry event and wrap their clip in
a `BSIsActiveModifier` bound to `bAnimationDriven`. Leaving the state deactivates
that modifier, so the acyclic Attack1 transition back to unwrapped Idle stops
driven motion without inventing a nonexistent stop event. Clip annotations such
as `weaponSwing`, `preHitFrame`, and `HitFrame` stay in the converted clip HKX;
the scaffold's clip generators deliberately use `triggers=null` instead of
copying annotation timing into the graph.

The MVP role manifest is independent of source naming. A Skyrim adapter can map
`mt_idle_wolf`/`walkforward_wolf` while an FNV adapter maps KF-derived names such
as `mtidle`/`mtforward` through the same API. Neither emission path branches on a
source game.

Manifest paths and embedded Havok paths have separate roles. `ScaffoldPaths`,
the skeleton path, and clip paths are Data-root deployment locations such as
`Actors\B21_Gecko\Animations\mtidle.hkx`; they remain unchanged in the runtime
manifest and determine where packing writes files. XML/HKX references are
derived relative to the directory containing the project: `Characters\...`,
`CharacterAssets\...`, `Behaviors\...`, and `Animations\...`. Validation rejects
an internal Havok dependency outside that project directory rather than writing
an invalid Data-root reference into a packfile.

The validator checks canonical case-insensitive Windows paths, file closure,
unique bone/clip/declaration names, skeleton hierarchy, clip transform-track
bindings, melee event prefixes, root declaration supersets, object IDs and
references, every `numelements`, declaration/info/value alignment, and variable
binding indices. Every emitted class-bearing `hkobject` is also gated against
the bundled hk2014 descriptor registry; unknown classes, missing signatures, and
signature mismatches fail before packing and again after the packed HKX is read
back. In particular, the FO4 `hkbStateMachine` signature is `0xa5896bcf`.

`emit_creature_record_closure` is the shared FO4 record boundary for source
adapters. It consumes the same `CreatureManifest`/`MvpGraphManifest` used by the
Havok scaffold plus a target-owned `CreatureRecordManifest` and a compact
`CreatureRecordProfile`. The emitter has no Skyrim/FNV switch: both a wolf and a
KF-derived Gecko profile produce the same six-record closure:

- custom `RACE`, `NPC_`, skin `ARMO`, body `ARMA`, root-only `BPTD`, and unarmed
  `WEAP` records;
- `RACE` links to the custom visual `Skeleton.nif`, packed project `.hkx`, skin,
  body-part data, graph-selected lowercase `melee*` event, and unarmed weapon;
- `RACE.SGNM/SAPT/SRAF` register the generated core behavior and animation
  directory as a third-person subgraph, so the CK-free AnimTextData generator
  discovers the new family instead of producing an empty request set;
- `ARMA` links only to the custom race and source-owned converted body NIF;
- `NPC_` explicitly links to the custom race and skin;
- the non-dismembering body-part row maps the declared rig root node to geometry
  segment 32;
- every target-plugin FormKey resolves inside the emitted closure. The only
  external links are the verified FO4 `BothHands`, `AnimsUnarmed`, and
  `WeaponTypeUnarmed` records.

All six records pass the live FO4 `AuthoringSchema` normalizer before they are
returned. A normalizer field loss is an error, as are source-plugin FormKeys,
donor/out-of-tree runtime paths, `.hkt`/authoring paths, or a body segment other
than the currently proven root segment 32. Unsupported record fields stay out of
the profile instead of being synthesized from guessed bytes.

`CreatureCorpusPlan` is the serializable broad-corpus boundary. It
uses one `CreatureCorpusCandidate` per source RACE. Its `source_identity` keeps
that RACE identity, while `primary_record_identity` selects exactly one primary
NPC `RecordVariant`; further NPC variants and attacks stay grouped under the
same helper-record family. Planning is deterministic regardless of candidate
order. Each candidate appears exactly once in `planned` or `rejected`; the typed
rejection ledger records upstream catalog rejection or curated exclusion,
missing/ambiguous family, motion, attack, and RACE-data mappings, capability
gaps, invalid primary mappings, and source/output collisions. `canonical_json`
validates accounting and collision invariants before serialization.

A rig family must carry `RaceDataMapping::Mapped { target: Fo4RaceDataTarget }` before it
is corpus-ready. The target model exposes source-neutral height, weight,
movement, size, biped-slot, XP, and orientation values plus typed FO4 race flags.
`RaceDataMapping::Missing` rejects the family with the exact missing semantic
fields. No family inherits Molerat `RACE.DATA` bytes.

`emit_creature_record_projection` expands the fixed closure without changing
the six-record fixture API. It preserves the primary source identity, emits one
`NPC_` per record variant, emits one unarmed `WEAP` and `RACE.ATKD/ATKE` pair per
attack, and shares generated RACE/skin/ARMA/BPTD helpers. Final assembly is
normalized as a whole. The validator closes every FormKey, verifies every
attack event, and requires the explicit `RootOnly32` BPTD row arrays to contain
exactly one aligned row whose geometry segment is 32.

The live FNV/FO3 and Skyrim builders use this module per unique motion family.
They convert the family skeleton, clips, bodies, and ragdoll, then author a new
FO4 project, character, swap-generator root, and functional core state machine.
Source behavior topology is evidence for roles and events; it is not a graph
that must be reproduced. Tied roles, optional topology, unused clips, and a
missing source trigger are degradable: the adapters select a canonical clip and
use deterministic FO4 events. Missing or invalid mandatory assets/bindings,
pack/reread failures, record/ref collisions, or publication failures remain
fatal.

Actor Action integration is recipe-bound and transactional. Pair Stage 1
enumerates the source-neutral graph requirements, `ConversionRun` allocates
collision-safe generated-record leases, and exactly one deterministic candidate
per family owns the complete action set. Non-owner candidates validate against
that family owner instead of publishing duplicates. Each generated `IDLE`
contains `EDID`, the exact root behavior `DNAM`, graph event `ENAM`, `ANAM`
parent/previous links, and the six-byte FO4 `DATA` structure. Requirements cover
idle, movement start/stop, death/ragdoll, melee, ranged start/stop, swim/fly,
and equipment actions when declared by the graph. Their `Action*` parents are
exact `Fallout4.esm` `AACT` closure dependencies.

`actor_action_admission_report` is fatal-free only when every exact generated
record plan is present. Publication consumes the matching leases only after the
whole normalized record batch and recursive FormKey closure validate; collisions,
pack/reread failures, unresolved mandatory references, and failed commits remain
atomic fatal errors. Optional or source-only semantics cross the terminal
boundary only as canonical `CreatureDegradationReceipt` warnings.
