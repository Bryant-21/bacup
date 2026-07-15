from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

from bacup_lib.behavior.clip_roles import ROLE_ORDER


@dataclass(frozen=True)
class BehaviorStateTemplate:
    name: str
    role: str
    loop: bool = False
    generator: str = "hkbClipGenerator"
    required: bool = True


@dataclass(frozen=True)
class BehaviorTransitionTemplate:
    event: str
    from_state: str
    to_state: str


@dataclass(frozen=True)
class BehaviorTemplate:
    archetype: str
    behavior_name: str
    root_behavior_name: str
    project_name: str
    character_name: str
    model_after: str
    states: tuple[BehaviorStateTemplate, ...]
    transitions: tuple[BehaviorTransitionTemplate, ...]
    model_up: tuple[float, float, float, float] = (0.0, 0.0, 1.0, 0.0)
    model_forward: tuple[float, float, float, float] = (0.0, 1.0, 0.0, 0.0)
    model_right: tuple[float, float, float, float] = (1.0, 0.0, 0.0, 0.0)


def _template_dir(templates_dir: str | Path | None = None) -> Path:
    if templates_dir is None:
        return Path(__file__).resolve().parent
    return Path(templates_dir)


def available_template_archetypes(templates_dir: str | Path | None = None) -> list[str]:
    template_dir = _template_dir(templates_dir)
    return sorted(
        path.stem
        for path in template_dir.glob("*.yaml")
        if not path.name.startswith("_")
    )


def load_behavior_template(archetype: str, templates_dir: str | Path | None = None) -> BehaviorTemplate:
    return load_behavior_template_file(_template_dir(templates_dir) / f"{archetype}.yaml")


def load_behavior_template_file(path: str | Path) -> BehaviorTemplate:
    template_path = Path(path)
    raw = yaml.safe_load(template_path.read_text(encoding="utf-8")) or {}
    if not isinstance(raw, dict):
        raise ValueError(f"Template must be a mapping: {template_path}")
    return _parse_template(raw, template_path)


def _parse_template(raw: dict[str, Any], path: Path) -> BehaviorTemplate:
    required = (
        "archetype",
        "behavior_name",
        "root_behavior_name",
        "project_name",
        "character_name",
        "model_after",
        "states",
    )
    for key in required:
        if key not in raw:
            raise ValueError(f"Template {path} is missing required field '{key}'")

    raw_states = raw["states"]
    if not isinstance(raw_states, list) or not raw_states:
        raise ValueError(f"Template {path} must define non-empty states")

    states = tuple(_parse_state(item, path) for item in raw_states)
    state_names = {state.name for state in states}

    raw_transitions = raw.get("transitions", [])
    if not isinstance(raw_transitions, list):
        raise ValueError(f"Template {path} transitions must be a list")
    transitions = tuple(_parse_transition(item, state_names, path) for item in raw_transitions)

    return BehaviorTemplate(
        archetype=str(raw["archetype"]),
        behavior_name=str(raw["behavior_name"]),
        root_behavior_name=str(raw["root_behavior_name"]),
        project_name=str(raw["project_name"]),
        character_name=str(raw["character_name"]),
        model_after=str(raw["model_after"]),
        states=states,
        transitions=transitions,
        model_up=_parse_vector(raw.get("model_up"), (0.0, 0.0, 1.0, 0.0), path, "model_up"),
        model_forward=_parse_vector(
            raw.get("model_forward"),
            (0.0, 1.0, 0.0, 0.0),
            path,
            "model_forward",
        ),
        model_right=_parse_vector(
            raw.get("model_right"),
            (1.0, 0.0, 0.0, 0.0),
            path,
            "model_right",
        ),
    )


def _parse_state(raw: Any, path: Path) -> BehaviorStateTemplate:
    if not isinstance(raw, dict):
        raise ValueError(f"Template {path} contains a non-mapping state entry")
    try:
        role = str(raw["role"])
        name = str(raw["name"])
    except KeyError as exc:
        raise ValueError(f"Template {path} state is missing required field '{exc.args[0]}'") from exc
    if role not in ROLE_ORDER:
        raise ValueError(f"Template {path} has unsupported role '{role}'")
    return BehaviorStateTemplate(
        name=name,
        role=role,
        loop=bool(raw.get("loop", False)),
        generator=str(raw.get("generator", "hkbClipGenerator")),
        required=bool(raw.get("required", True)),
    )


def _parse_transition(raw: Any, state_names: set[str], path: Path) -> BehaviorTransitionTemplate:
    if not isinstance(raw, dict):
        raise ValueError(f"Template {path} contains a non-mapping transition entry")
    try:
        event = str(raw["event"])
        from_state = str(raw["from"])
        to_state = str(raw["to"])
    except KeyError as exc:
        raise ValueError(
            f"Template {path} transition is missing required field '{exc.args[0]}'"
        ) from exc
    if from_state not in state_names or to_state not in state_names:
        raise ValueError(
            f"Template {path} transition references unknown states: {from_state} -> {to_state}"
        )
    return BehaviorTransitionTemplate(event=event, from_state=from_state, to_state=to_state)


def _parse_vector(
    raw: Any,
    default: tuple[float, float, float, float],
    path: Path,
    field_name: str,
) -> tuple[float, float, float, float]:
    if raw is None:
        return default
    if not isinstance(raw, list) or len(raw) != 4:
        raise ValueError(f"Template {path} field '{field_name}' must be a 4-item list")
    return tuple(float(value) for value in raw)
