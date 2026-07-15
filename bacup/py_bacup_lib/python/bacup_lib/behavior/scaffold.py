"""Generate minimal Havok behavior graph XML for FO4 from converted animation clips.

When converting FO3 .kf animations to FO4, the .hkx clips need to be registered
in a behavior graph to be playable by the engine. This module generates a minimal
behavior graph that makes each clip addressable as a state-machine state.
"""

from __future__ import annotations

import re
from pathlib import Path

from bacup_lib.models import AnimationClip


def _sanitize_name(name: str) -> str:
    """Sanitize a clip name for use in XML element names and animation file paths."""
    return re.sub(r"[^a-zA-Z0-9_]", "_", name)


def _clip_filename(clip: AnimationClip) -> str:
    """Derive the .hkx animation filename from a clip name."""
    return _sanitize_name(clip.name).lower() + ".hkx"


def _playback_mode(clip: AnimationClip) -> str:
    """Map cycle_type to Havok playback mode constant."""
    if clip.cycle_type == "loop":
        return "MODE_LOOPING"
    return "MODE_SINGLE_PLAY"


def _collect_events(clips: list[AnimationClip]) -> list[str]:
    """Collect and deduplicate event texts across all clips, preserving order."""
    seen: set[str] = set()
    events: list[str] = []
    for clip in clips:
        for ev in clip.events:
            if ev.text not in seen:
                seen.add(ev.text)
                events.append(ev.text)
    return events


def _build_xml(
    clips: list[AnimationClip],
    skeleton_path: str,
) -> str:
    """Build the behavior graph XML string."""
    events = _collect_events(clips)
    num_clips = len(clips)
    num_events = len(events)

    # --- Build state refs for the state machine ---
    state_refs = "\n".join(
        f"      #state_{i}" for i in range(num_clips)
    )

    # --- Build per-clip state + clip-generator objects ---
    clip_objects: list[str] = []
    for i, clip in enumerate(clips):
        safe_name = _sanitize_name(clip.name)
        clip_objects.append(
            f'  <hkobject name="state_{i}" class="hkbStateMachineStateInfo" signature="0xed7f9d0">\n'
            f"    <hkparam name=\"stateId\">{i}</hkparam>\n"
            f"    <hkparam name=\"name\">{safe_name}</hkparam>\n"
            f"    <hkparam name=\"generator\">#clipGen_{i}</hkparam>\n"
            f'    <hkparam name="transitions">#null</hkparam>\n'
            f"  </hkobject>"
        )
        clip_objects.append(
            f'  <hkobject name="clipGen_{i}" class="hkbClipGenerator" signature="0x333b85b9">\n'
            f"    <hkparam name=\"animationName\">{_clip_filename(clip)}</hkparam>\n"
            f"    <hkparam name=\"playbackSpeed\">{clip.frequency}</hkparam>\n"
            f"    <hkparam name=\"mode\">{_playback_mode(clip)}</hkparam>\n"
            f"  </hkobject>"
        )

    clip_objects_str = "\n\n".join(clip_objects)

    # --- Build event infos and string data ---
    if events:
        event_info_entries = "\n".join(
            f"      <hkobject>\n"
            f"        <hkparam name=\"flags\">0</hkparam>\n"
            f"      </hkobject>"
            for _ in events
        )
        event_infos_block = (
            f'    <hkparam name="eventInfos" numelements="{num_events}">\n'
            f"{event_info_entries}\n"
            f"    </hkparam>"
        )
        event_name_entries = "\n".join(
            f"      <hkcstring>{ev}</hkcstring>" for ev in events
        )
        event_names_block = (
            f'    <hkparam name="eventNames" numelements="{num_events}">\n'
            f"{event_name_entries}\n"
            f"    </hkparam>"
        )
    else:
        event_infos_block = f'    <hkparam name="eventInfos" numelements="0"></hkparam>'
        event_names_block = f'    <hkparam name="eventNames" numelements="0"></hkparam>'

    xml = f"""\
<?xml version="1.0" encoding="ascii"?>
<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1">
<hksection name="__data__">

  <hkobject name="behaviorGraph" class="hkbBehaviorGraph" signature="0xb1218f86">
    <hkparam name="variableMode">VARIABLE_MODE_CONTINUOUS</hkparam>
    <hkparam name="rootGenerator">#stateMachine</hkparam>
    <hkparam name="data">#behaviorData</hkparam>
  </hkobject>

  <hkobject name="stateMachine" class="hkbStateMachine" signature="0x816c1dcb">
    <hkparam name="startStateId">0</hkparam>
    <hkparam name="states" numelements="{num_clips}">
{state_refs}
    </hkparam>
    <hkparam name="wildcardTransitions">#null</hkparam>
  </hkobject>

{clip_objects_str}

  <hkobject name="behaviorData" class="hkbBehaviorGraphData" signature="0x95aca5d">
    <hkparam name="stringData">#stringData</hkparam>
    <hkparam name="variableInfos" numelements="0"></hkparam>
{event_infos_block}
  </hkobject>

  <hkobject name="stringData" class="hkbBehaviorGraphStringData" signature="0xc713064e">
{event_names_block}
  </hkobject>

</hksection>
</hkpackfile>
"""
    return xml


def generate_behavior_xml(
    clips: list[AnimationClip],
    skeleton_path: str,
    output_path: str | Path,
) -> None:
    """Generate a minimal Havok behavior graph XML referencing the given clips.

    Creates a simple state machine where each clip is a state with basic transitions.
    The first clip is the default/idle state.

    Args:
        clips: List of converted animation clips
        skeleton_path: Relative path to the skeleton .hkx file
        output_path: Where to write the behavior XML
    """
    if not clips:
        raise ValueError("At least one animation clip is required")

    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    xml = _build_xml(clips, skeleton_path)
    output_path.write_text(xml, encoding="utf-8")
