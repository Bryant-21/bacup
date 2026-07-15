"""Translate FO3 NiControllerSequence animation clips into a full Havok behavior graph.

FO3 uses NiControllerManager with NiControllerSequence blocks to manage animations.
FO4 uses Havok behavior graphs (hkbBehaviorGraph, hkbStateMachine, hkbClipGenerator,
hkbBlendGenerator) with event-driven state transitions.

This module performs a best-effort translation from one paradigm to the other:
  1. Group clips by functional category (idle, attack, locomotion, etc.)
  2. Build a hierarchical state machine with sub-state machines per category
  3. Generate transition events between categories
  4. Use blend generators for locomotion (walk/run) blending
  5. Map NIF cycle types to Havok playback modes
  6. Respect per-channel priority for blend ordering

The generated behavior graph is functional enough that animations play in FO4,
though manual tuning may be needed for complex animation sets.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

from bacup_lib.models import AnimationClip


# ---------------------------------------------------------------------------
# Category classification
# ---------------------------------------------------------------------------

# Each pattern list is checked in order; first match wins.
_CATEGORY_PATTERNS: list[tuple[str, list[str]]] = [
    ("idle", [r"(?i)^idle", r"(?i)idle"]),
    ("attack", [r"(?i)^attack", r"(?i)attack", r"(?i)power"]),
    ("locomotion", [r"(?i)^walk", r"(?i)walk", r"(?i)^run", r"(?i)run"]),
    ("block", [r"(?i)^block", r"(?i)block"]),
    ("equip", [r"(?i)^equip", r"(?i)^unequip", r"(?i)equip", r"(?i)holster"]),
    ("aim", [r"(?i)aim"]),
]

# All known categories (order matters for state IDs and transitions)
CATEGORIES = ["idle", "attack", "locomotion", "block", "equip", "aim", "misc"]


@dataclass
class _ClipEntry:
    """Internal: a clip tagged with its category and a max bone priority."""

    clip: AnimationClip
    category: str
    max_priority: int


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def translate_behavior(
    clips: list[AnimationClip],
    skeleton_path: str,
    output_path: str | Path,
) -> None:
    """Translate FO3 AnimationClips into a Havok behavior graph XML.

    Groups clips by functional category, builds nested state machines,
    generates inter-state transition events, and writes the result as
    Havok-compatible XML.

    Args:
        clips: Animation clips parsed from FO3 .kf files.
        skeleton_path: Relative path to the skeleton .hkx file.
        output_path: Destination for the generated behavior XML.

    Raises:
        ValueError: If *clips* is empty.
    """
    if not clips:
        raise ValueError("At least one animation clip is required")

    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    entries = [_classify(c) for c in clips]
    grouped = _group_by_category(entries)

    xml = _build_full_xml(grouped, skeleton_path)
    output_path.write_text(xml, encoding="utf-8")


# ---------------------------------------------------------------------------
# Classification helpers
# ---------------------------------------------------------------------------


def classify_clip(clip: AnimationClip) -> str:
    """Return the functional category for *clip* based on its name.

    Exposed for testing.
    """
    return _classify(clip).category


def _classify(clip: AnimationClip) -> _ClipEntry:
    """Classify a single clip and compute its max bone priority."""
    category = "misc"
    for cat, patterns in _CATEGORY_PATTERNS:
        if any(re.search(p, clip.name) for p in patterns):
            category = cat
            break

    max_pri = 0
    for ch in clip.channels:
        if ch.priority > max_pri:
            max_pri = ch.priority
    if max_pri == 0:
        max_pri = 26  # default Havok priority

    return _ClipEntry(clip=clip, category=category, max_priority=max_pri)


def _group_by_category(
    entries: list[_ClipEntry],
) -> dict[str, list[_ClipEntry]]:
    """Group classified entries by category, preserving order."""
    grouped: dict[str, list[_ClipEntry]] = {}
    for entry in entries:
        grouped.setdefault(entry.category, []).append(entry)

    # Sort clips within each category by priority (highest first) for blend ordering
    for cat_entries in grouped.values():
        cat_entries.sort(key=lambda e: e.max_priority, reverse=True)

    return grouped


# ---------------------------------------------------------------------------
# Name / mode helpers
# ---------------------------------------------------------------------------


def _sanitize(name: str) -> str:
    return re.sub(r"[^a-zA-Z0-9_]", "_", name)


def _clip_filename(clip: AnimationClip) -> str:
    return _sanitize(clip.name).lower() + ".hkx"


def _playback_mode(clip: AnimationClip) -> str:
    if clip.cycle_type == "loop":
        return "MODE_LOOPING"
    return "MODE_SINGLE_PLAY"


# ---------------------------------------------------------------------------
# Event generation
# ---------------------------------------------------------------------------


def generate_transition_events(
    grouped: dict[str, list[_ClipEntry]],
) -> list[str]:
    """Build the list of transition event names.

    Creates pairwise ``<src>To<Dst>`` events between all present categories,
    plus a ``wildcardToIdle`` event for the default return-to-idle transition.
    Also includes any clip-embedded events.

    Exposed for testing.
    """
    present = [c for c in CATEGORIES if c in grouped]
    events: list[str] = []
    seen: set[str] = set()

    def _add(name: str) -> None:
        if name not in seen:
            seen.add(name)
            events.append(name)

    # Pairwise transition events between categories
    for src in present:
        for dst in present:
            if src != dst:
                _add(f"{src}To{dst.capitalize()}")

    # Wildcard return-to-idle
    if "idle" in grouped:
        _add("wildcardToIdle")

    # Clip-embedded events (from NiTextKeyExtraData / annotations)
    for entries in grouped.values():
        for entry in entries:
            for ev in entry.clip.events:
                _add(ev.text)

    return events


# ---------------------------------------------------------------------------
# XML builder — object ID allocator
# ---------------------------------------------------------------------------


class _IdAlloc:
    """Simple monotonic ID allocator for hkobject name attributes."""

    def __init__(self, prefix: str = "obj") -> None:
        self._prefix = prefix
        self._next = 0

    def alloc(self, tag: str = "") -> str:
        name = f"{self._prefix}_{self._next}"
        if tag:
            name = f"{tag}_{self._next}"
        self._next += 1
        return name


# ---------------------------------------------------------------------------
# XML builder — main
# ---------------------------------------------------------------------------


def _build_full_xml(
    grouped: dict[str, list[_ClipEntry]],
    skeleton_path: str,
) -> str:
    """Construct the complete behavior graph XML string."""
    ids = _IdAlloc()
    objects: list[str] = []
    events = generate_transition_events(grouped)

    present_cats = [c for c in CATEGORIES if c in grouped]

    # --- Root behavior graph ---
    bg_name = ids.alloc("behaviorGraph")
    sm_root_name = ids.alloc("rootSM")
    data_name = ids.alloc("behaviorData")
    str_data_name = ids.alloc("stringData")

    objects.append(
        f'  <!-- Root behavior graph - translated from FO3 NiControllerManager -->\n'
        f'  <hkobject name="{bg_name}" class="hkbBehaviorGraph" signature="0xb1218f86">\n'
        f'    <hkparam name="variableMode">VARIABLE_MODE_CONTINUOUS</hkparam>\n'
        f'    <hkparam name="rootGenerator">#{sm_root_name}</hkparam>\n'
        f'    <hkparam name="data">#{data_name}</hkparam>\n'
        f'  </hkobject>'
    )

    # --- Root state machine (one state per category) ---
    category_state_names: dict[str, str] = {}
    category_generator_names: dict[str, str] = {}

    for cat in present_cats:
        category_state_names[cat] = ids.alloc(f"catState_{cat}")
        category_generator_names[cat] = ids.alloc(f"catGen_{cat}")

    state_refs = "\n".join(
        f"      #{category_state_names[c]}" for c in present_cats
    )

    # Wildcard transitions object for return-to-idle
    wildcard_ref = "#null"
    wildcard_obj_name = ""
    if "idle" in grouped:
        wildcard_obj_name = ids.alloc("wildcardTrans")
        idle_event_idx = _event_index(events, "wildcardToIdle")
        idle_state_id = present_cats.index("idle")
        wildcard_ref = f"#{wildcard_obj_name}"

        # Build the wildcard transition info
        wt_info_name = ids.alloc("wtInfo")
        objects.append(
            f'\n  <!-- Wildcard transition: any state -> idle (default fallback) -->\n'
            f'  <hkobject name="{wildcard_obj_name}" class="hkbStateMachineTransitionInfoArray" signature="0xe397b11e">\n'
            f'    <hkparam name="transitions" numelements="1">\n'
            f'      <hkobject>\n'
            f'        <hkparam name="eventId">{idle_event_idx}</hkparam>\n'
            f'        <hkparam name="toStateId">{idle_state_id}</hkparam>\n'
            f'        <hkparam name="transition">#null</hkparam>\n'
            f'        <hkparam name="flags">0</hkparam>\n'
            f'      </hkobject>\n'
            f'    </hkparam>\n'
            f'  </hkobject>'
        )

    # Determine start state: prefer idle, else first present category
    start_state_id = 0
    if "idle" in present_cats:
        start_state_id = present_cats.index("idle")

    objects.append(
        f'\n  <!-- Root state machine - one state per animation category -->\n'
        f'  <hkobject name="{sm_root_name}" class="hkbStateMachine" signature="0x816c1dcb">\n'
        f'    <hkparam name="startStateId">{start_state_id}</hkparam>\n'
        f'    <hkparam name="states" numelements="{len(present_cats)}">\n'
        f'{state_refs}\n'
        f'    </hkparam>\n'
        f'    <hkparam name="wildcardTransitions">{wildcard_ref}</hkparam>\n'
        f'  </hkobject>'
    )

    # --- Per-category state info + transitions ---
    for cat_idx, cat in enumerate(present_cats):
        entries = grouped[cat]
        state_name = category_state_names[cat]
        gen_name = category_generator_names[cat]

        # Build transition array for this state → other categories
        trans_items = _build_category_transitions(
            cat, cat_idx, present_cats, events, ids
        )
        trans_ref = "#null"
        if trans_items:
            trans_obj_name = ids.alloc(f"trans_{cat}")
            trans_ref = f"#{trans_obj_name}"
            trans_entries = "\n".join(
                f'      <hkobject>\n'
                f'        <hkparam name="eventId">{eid}</hkparam>\n'
                f'        <hkparam name="toStateId">{tid}</hkparam>\n'
                f'        <hkparam name="transition">#null</hkparam>\n'
                f'        <hkparam name="flags">0</hkparam>\n'
                f'      </hkobject>'
                for eid, tid in trans_items
            )
            objects.append(
                f'\n  <hkobject name="{trans_obj_name}" class="hkbStateMachineTransitionInfoArray" signature="0xe397b11e">\n'
                f'    <hkparam name="transitions" numelements="{len(trans_items)}">\n'
                f'{trans_entries}\n'
                f'    </hkparam>\n'
                f'  </hkobject>'
            )

        objects.append(
            f'\n  <!-- Category state: {cat} ({len(entries)} clip(s)) -->\n'
            f'  <hkobject name="{state_name}" class="hkbStateMachineStateInfo" signature="0xed7f9d0">\n'
            f'    <hkparam name="stateId">{cat_idx}</hkparam>\n'
            f'    <hkparam name="name">{cat}</hkparam>\n'
            f'    <hkparam name="generator">#{gen_name}</hkparam>\n'
            f'    <hkparam name="transitions">{trans_ref}</hkparam>\n'
            f'  </hkobject>'
        )

    # --- Per-category generator (sub-state-machine, blend, or single clip) ---
    for cat in present_cats:
        entries = grouped[cat]
        gen_name = category_generator_names[cat]

        if cat == "locomotion" and len(entries) > 1:
            # Locomotion uses a blend generator so walk/run can blend
            _build_blend_generator(entries, gen_name, ids, objects)
        elif len(entries) == 1:
            # Single clip — just a clip generator
            _build_clip_generator(entries[0], gen_name, objects)
        else:
            # Multiple clips — sub-state machine with variants
            _build_sub_state_machine(entries, gen_name, cat, ids, objects)

    # --- Behavior data + string data ---
    _build_data_objects(events, data_name, str_data_name, objects)

    objects_str = "\n".join(objects)

    return (
        '<?xml version="1.0" encoding="ascii"?>\n'
        '<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1">\n'
        '<hksection name="__data__">\n\n'
        f'{objects_str}\n\n'
        '</hksection>\n'
        '</hkpackfile>\n'
    )


# ---------------------------------------------------------------------------
# Transition builder
# ---------------------------------------------------------------------------


def _event_index(events: list[str], name: str) -> int:
    """Return event index, or -1 if not found."""
    try:
        return events.index(name)
    except ValueError:
        return -1


def _build_category_transitions(
    src_cat: str,
    src_idx: int,
    present_cats: list[str],
    events: list[str],
    ids: _IdAlloc,
) -> list[tuple[int, int]]:
    """Return (eventId, toStateId) pairs for transitions FROM this category."""
    result: list[tuple[int, int]] = []
    for dst_idx, dst_cat in enumerate(present_cats):
        if dst_cat == src_cat:
            continue
        ev_name = f"{src_cat}To{dst_cat.capitalize()}"
        ev_idx = _event_index(events, ev_name)
        if ev_idx >= 0:
            result.append((ev_idx, dst_idx))
    return result


# ---------------------------------------------------------------------------
# Generator builders
# ---------------------------------------------------------------------------


def _build_clip_generator(
    entry: _ClipEntry,
    obj_name: str,
    objects: list[str],
) -> None:
    """Emit a single hkbClipGenerator."""
    clip = entry.clip
    objects.append(
        f'\n  <!-- Clip: {clip.name} (priority={entry.max_priority}, cycle={clip.cycle_type}) -->\n'
        f'  <hkobject name="{obj_name}" class="hkbClipGenerator" signature="0x333b85b9">\n'
        f'    <hkparam name="animationName">{_clip_filename(clip)}</hkparam>\n'
        f'    <hkparam name="playbackSpeed">{clip.frequency}</hkparam>\n'
        f'    <hkparam name="mode">{_playback_mode(clip)}</hkparam>\n'
        f'  </hkobject>'
    )


def _build_blend_generator(
    entries: list[_ClipEntry],
    obj_name: str,
    ids: _IdAlloc,
    objects: list[str],
) -> None:
    """Emit an hkbBlendGenerator referencing child clip generators.

    Clips are ordered by descending priority — higher priority clips come first
    in the blend children list and thus take precedence.
    """
    child_names: list[str] = []
    for entry in entries:
        child_name = ids.alloc(f"blendChild_{_sanitize(entry.clip.name)}")
        child_names.append(child_name)

    child_refs = "\n".join(f"      #{n}" for n in child_names)

    objects.append(
        f'\n  <!-- Locomotion blend generator ({len(entries)} clips, priority-ordered) -->\n'
        f'  <hkobject name="{obj_name}" class="hkbBlenderGenerator" signature="0x22df7f">\n'
        f'    <hkparam name="children" numelements="{len(entries)}">\n'
        f'{child_refs}\n'
        f'    </hkparam>\n'
        f'    <hkparam name="flags">0</hkparam>\n'
        f'  </hkobject>'
    )

    # Emit the child clip generators
    for entry, child_name in zip(entries, child_names):
        _build_clip_generator(entry, child_name, objects)


def _build_sub_state_machine(
    entries: list[_ClipEntry],
    obj_name: str,
    category: str,
    ids: _IdAlloc,
    objects: list[str],
) -> None:
    """Emit a sub-state machine for a category with multiple variant clips.

    For example, attack category might have AttackLeft, AttackRight, AttackPower.
    """
    sub_state_names: list[str] = []
    sub_clip_names: list[str] = []

    for i, entry in enumerate(entries):
        sub_state_names.append(ids.alloc(f"subState_{category}_{_sanitize(entry.clip.name)}"))
        sub_clip_names.append(ids.alloc(f"subClip_{category}_{_sanitize(entry.clip.name)}"))

    sub_state_refs = "\n".join(f"      #{n}" for n in sub_state_names)

    objects.append(
        f'\n  <!-- Sub-state machine for {category} ({len(entries)} variants) -->\n'
        f'  <hkobject name="{obj_name}" class="hkbStateMachine" signature="0x816c1dcb">\n'
        f'    <hkparam name="startStateId">0</hkparam>\n'
        f'    <hkparam name="states" numelements="{len(entries)}">\n'
        f'{sub_state_refs}\n'
        f'    </hkparam>\n'
        f'    <hkparam name="wildcardTransitions">#null</hkparam>\n'
        f'  </hkobject>'
    )

    for i, (entry, state_name, clip_name) in enumerate(
        zip(entries, sub_state_names, sub_clip_names)
    ):
        safe = _sanitize(entry.clip.name)
        objects.append(
            f'\n  <hkobject name="{state_name}" class="hkbStateMachineStateInfo" signature="0xed7f9d0">\n'
            f'    <hkparam name="stateId">{i}</hkparam>\n'
            f'    <hkparam name="name">{safe}</hkparam>\n'
            f'    <hkparam name="generator">#{clip_name}</hkparam>\n'
            f'    <hkparam name="transitions">#null</hkparam>\n'
            f'  </hkobject>'
        )
        _build_clip_generator(entry, clip_name, objects)


# ---------------------------------------------------------------------------
# Data / string-data objects
# ---------------------------------------------------------------------------


def _build_data_objects(
    events: list[str],
    data_name: str,
    str_data_name: str,
    objects: list[str],
) -> None:
    """Emit hkbBehaviorGraphData and hkbBehaviorGraphStringData."""
    num_events = len(events)

    if events:
        event_info_entries = "\n".join(
            "      <hkobject>\n"
            "        <hkparam name=\"flags\">0</hkparam>\n"
            "      </hkobject>"
            for _ in events
        )
        event_infos_block = (
            f'    <hkparam name="eventInfos" numelements="{num_events}">\n'
            f'{event_info_entries}\n'
            f'    </hkparam>'
        )
        event_name_entries = "\n".join(
            f"      <hkcstring>{ev}</hkcstring>" for ev in events
        )
        event_names_block = (
            f'    <hkparam name="eventNames" numelements="{num_events}">\n'
            f'{event_name_entries}\n'
            f'    </hkparam>'
        )
    else:
        event_infos_block = '    <hkparam name="eventInfos" numelements="0"></hkparam>'
        event_names_block = '    <hkparam name="eventNames" numelements="0"></hkparam>'

    objects.append(
        f'\n  <hkobject name="{data_name}" class="hkbBehaviorGraphData" signature="0x95aca5d">\n'
        f'    <hkparam name="stringData">#{str_data_name}</hkparam>\n'
        f'    <hkparam name="variableInfos" numelements="0"></hkparam>\n'
        f'{event_infos_block}\n'
        f'  </hkobject>'
    )

    objects.append(
        f'\n  <hkobject name="{str_data_name}" class="hkbBehaviorGraphStringData" signature="0xc713064e">\n'
        f'{event_names_block}\n'
        f'  </hkobject>'
    )
