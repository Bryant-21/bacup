from __future__ import annotations

from dataclasses import dataclass, replace
from pathlib import Path
import tempfile

from bacup_lib.behavior.clip_roles import bucket_clips_by_role
from bacup_lib.behavior.templates._schema import BehaviorTemplate


@dataclass(frozen=True)
class RenderedBehaviorBundle:
    template: BehaviorTemplate
    clip_buckets: dict[str, tuple[str, ...]]
    missing_optional_roles: tuple[str, ...]
    everything_xml: str
    root_xml: str


@dataclass(frozen=True)
class EmittedBehaviorBundle:
    everything_xml_path: Path
    root_xml_path: Path
    everything_hkx_path: Path
    root_hkx_path: Path


def render_behavior_bundle(
    template: BehaviorTemplate,
    clip_filenames: list[str],
) -> RenderedBehaviorBundle:
    clip_buckets = bucket_clips_by_role(clip_filenames)
    primary_clips: dict[str, str] = {}
    active_states = []
    missing_optional_roles = []
    for state in template.states:
        clips = clip_buckets.get(state.role, ())
        if not clips:
            if not state.required:
                missing_optional_roles.append(state.role)
                continue
            raise ValueError(
                f"Template role '{state.role}' has no clips for archetype '{template.archetype}'"
            )
        primary_clips[state.name] = clips[0]

        active_states.append(state)

    active_state_names = {state.name for state in active_states}
    active_template = replace(
        template,
        states=tuple(active_states),
        transitions=tuple(
            transition
            for transition in template.transitions
            if transition.from_state in active_state_names
            and transition.to_state in active_state_names
        ),
    )
    everything_xml = _render_everything_xml(active_template, clip_buckets, primary_clips)
    root_xml = _render_root_xml(active_template)
    return RenderedBehaviorBundle(
        template=active_template,
        clip_buckets=clip_buckets,
        missing_optional_roles=tuple(dict.fromkeys(missing_optional_roles)),
        everything_xml=everything_xml,
        root_xml=root_xml,
    )


def emit_behavior_bundle(
    base_dir: str | Path,
    rendered: RenderedBehaviorBundle,
    pack_xml_to_hkx=None,
) -> EmittedBehaviorBundle:
    base_path = Path(base_dir)
    behaviors_dir = base_path / "Behaviors"
    behaviors_dir.mkdir(parents=True, exist_ok=True)

    everything_xml_path = behaviors_dir / f"{rendered.template.behavior_name}.xml"
    root_xml_path = behaviors_dir / f"{rendered.template.root_behavior_name}.xml"
    everything_hkx_path = behaviors_dir / f"{rendered.template.behavior_name}.hkx"
    root_hkx_path = behaviors_dir / f"{rendered.template.root_behavior_name}.hkx"

    everything_xml_path.write_text(rendered.everything_xml, encoding="utf-8")
    root_xml_path.write_text(rendered.root_xml, encoding="utf-8")

    if pack_xml_to_hkx is not None:
        pack_xml_to_hkx(str(everything_xml_path), str(everything_hkx_path))
        pack_xml_to_hkx(str(root_xml_path), str(root_hkx_path))

    return EmittedBehaviorBundle(
        everything_xml_path=everything_xml_path,
        root_xml_path=root_xml_path,
        everything_hkx_path=everything_hkx_path,
        root_hkx_path=root_hkx_path,
    )


def pack_rendered_behavior_bundle(
    rendered: RenderedBehaviorBundle,
    everything_hkx_path: str | Path,
    root_hkx_path: str | Path,
    pack_xml_to_hkx,
) -> None:
    with tempfile.TemporaryDirectory(prefix="behavior_render_") as tmp_dir:
        tmp_path = Path(tmp_dir)
        everything_xml = tmp_path / "everything.xml"
        root_xml = tmp_path / "root.xml"
        everything_xml.write_text(rendered.everything_xml, encoding="utf-8")
        root_xml.write_text(rendered.root_xml, encoding="utf-8")
        pack_xml_to_hkx(str(everything_xml), str(everything_hkx_path))
        pack_xml_to_hkx(str(root_xml), str(root_hkx_path))


def _render_everything_xml(
    template: BehaviorTemplate,
    clip_buckets: dict[str, tuple[str, ...]],
    primary_clips: dict[str, str],
) -> str:
    graph_ref = _object_id(1)
    state_machine_ref = _object_id(2)
    state_refs_by_name = {
        state.name: _object_id(3 + index)
        for index, state in enumerate(template.states)
    }
    transition_start = 3 + len(template.states)
    transition_refs_by_index = {
        index: _object_id(transition_start + index)
        for index, _transition in enumerate(template.transitions)
    }
    clip_start = transition_start + len(template.transitions)
    total_clips = sum(len(clip_buckets[state.role]) for state in template.states)
    clip_refs = [_object_id(clip_start + index) for index in range(total_clips)]
    behavior_data_ref = _object_id(clip_start + total_clips)
    string_data_ref = _object_id(clip_start + total_clips + 1)

    state_refs = "\n".join(f"      {state_refs_by_name[state.name]}" for state in template.states)

    transition_map: dict[str, list[tuple[int, object]]] = {state.name: [] for state in template.states}
    for index, transition in enumerate(template.transitions):
        transition_map[transition.from_state].append((index, transition))

    state_blocks: list[str] = []
    clip_blocks: list[str] = []
    clip_index = 0
    for state_index, state in enumerate(template.states):
        bucket = clip_buckets[state.role]
        generator_ref = clip_refs[clip_index]
        state_transition_refs = "\n".join(
            f"      {transition_refs_by_index[transition_index]}"
            for transition_index, _transition in transition_map[state.name]
        )
        transition_count = len(transition_map[state.name])
        state_blocks.append(
            "\n".join(
                [
                    f'  <hkobject name="{state_refs_by_name[state.name]}" class="hkbStateMachineStateInfo" signature="0x0ed7f9d0">',
                    f'    <hkparam name="stateId">{state_index}</hkparam>',
                    f'    <hkparam name="name">{state.name}</hkparam>',
                    f'    <hkparam name="generator">{generator_ref}</hkparam>',
                    f'    <hkparam name="transitions" numelements="{transition_count}">',
                    state_transition_refs,
                    "    </hkparam>",
                    "  </hkobject>",
                ]
            )
        )
        for clip_name in bucket:
            clip_blocks.append(
                "\n".join(
                    [
                        f'  <hkobject name="{clip_refs[clip_index]}" class="{state.generator}" signature="0x333b85b9">',
                        f'    <hkparam name="name">{state.name}::{clip_name}</hkparam>',
                        f'    <hkparam name="animationName">{clip_name}</hkparam>',
                        f'    <hkparam name="mode">{"MODE_LOOPING" if state.loop else "MODE_SINGLE_PLAY"}</hkparam>',
                        "  </hkobject>",
                    ]
                )
            )
            clip_index += 1

    transition_blocks = [
        "\n".join(
            [
                f'  <hkobject name="{transition_refs_by_index[index]}" class="hkbStateMachineTransitionInfo" signature="0xe397b11e">',
                f'    <hkparam name="event">{transition.event}</hkparam>',
                f'    <hkparam name="fromState">{transition.from_state}</hkparam>',
                f'    <hkparam name="toState">{transition.to_state}</hkparam>',
                "  </hkobject>",
            ]
        )
        for index, transition in enumerate(template.transitions)
    ]

    event_names = "\n".join(
        f"      <hkcstring>{transition.event}</hkcstring>" for transition in template.transitions
    )
    animation_names = "\n".join(
        f"      <hkcstring>{clip_name}</hkcstring>"
        for state in template.states
        for clip_name in clip_buckets[state.role]
    )

    return "\n".join(
        [
            '<?xml version="1.0" encoding="ascii"?>',
            '<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1">',
            '<hksection name="__data__">',
            f'  <hkobject name="{graph_ref}" class="hkbBehaviorGraph" signature="0xb1218f86">',
            f'    <hkparam name="name">{template.behavior_name}</hkparam>',
            f'    <hkparam name="rootGenerator">{state_machine_ref}</hkparam>',
            f'    <hkparam name="data">{behavior_data_ref}</hkparam>',
            "  </hkobject>",
            f'  <hkobject name="{state_machine_ref}" class="hkbStateMachine" signature="0x816c1dcb">',
            '    <hkparam name="startStateId">0</hkparam>',
            f'    <hkparam name="states" numelements="{len(template.states)}">',
            state_refs,
            "    </hkparam>",
            "  </hkobject>",
            *state_blocks,
            *transition_blocks,
            *clip_blocks,
            f'  <hkobject name="{behavior_data_ref}" class="hkbBehaviorGraphData" signature="0x095aca5d">',
            f'    <hkparam name="stringData">{string_data_ref}</hkparam>',
            "  </hkobject>",
            f'  <hkobject name="{string_data_ref}" class="hkbBehaviorGraphStringData" signature="0xc713064e">',
            f'    <hkparam name="eventNames" numelements="{len(template.transitions)}">',
            event_names,
            "    </hkparam>",
            f'    <hkparam name="animationNames" numelements="{sum(len(clip_buckets[state.role]) for state in template.states)}">',
            animation_names,
            "    </hkparam>",
            "  </hkobject>",
            "</hksection>",
            "</hkpackfile>",
        ]
    )


def _render_root_xml(template: BehaviorTemplate) -> str:
    root_graph_ref = _object_id(1)
    everything_reference_ref = _object_id(2)
    return "\n".join(
        [
            '<?xml version="1.0" encoding="ascii"?>',
            '<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1">',
            '<hksection name="__data__">',
            f'  <hkobject name="{root_graph_ref}" class="hkbBehaviorGraph" signature="0xb1218f86">',
            f'    <hkparam name="name">{template.root_behavior_name}</hkparam>',
            f'    <hkparam name="rootGenerator">{everything_reference_ref}</hkparam>',
            "  </hkobject>",
            f'  <hkobject name="{everything_reference_ref}" class="hkbBehaviorReferenceGenerator" signature="0x0f0f0f0f">',
            f'    <hkparam name="behaviorName">{template.behavior_name}</hkparam>',
            f'    <hkparam name="externalPath">{template.behavior_name}.hkx</hkparam>',
            "  </hkobject>",
            "</hksection>",
            "</hkpackfile>",
        ]
    )


def _object_id(index: int) -> str:
    return f"#{index:04}"
