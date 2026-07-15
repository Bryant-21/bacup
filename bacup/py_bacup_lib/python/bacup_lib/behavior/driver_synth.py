"""Inject Havok modifier chains to drive telemetry-driven behavior variables.

When converting FO76 weapon behaviors to FO4, some behavior graphs (e.g.
MeltdownFX) declare a float variable bound to a Havok generator member
path (typically ``BGSGamebryoSequenceGenerator.fTimePercent``) but contain
no internal modifier that writes the variable. In FO76 the engine or an
external Papyrus driver fed those variables; that driver does not survive
the conversion.

FO4 vanilla weapons (Cryolator, Flamer, Minigun) solve the same problem
with an **HKX-internal modifier chain** — no Papyrus. Pattern:

    hkbDampingModifier (kP~0.15-0.2)
       rawValue   <- fRaw variable (1 on fire, 0 on idle)
       dampedValue ->  fAmount variable (what the sequence reads)
       kP         <- fDampRate variable

    hkbModifierGenerator wraps the visible sequence so the damper ticks
    every frame. State machine transitions set ``fRaw`` on WeaponFire /
    WeaponSheathe, and the damper smoothly lerps toward the target.

This module:

1. Detects telemetry-driven, externally-fed variables in a behavior HKX.
2. Mutates the unpacked HKX XML to add the modifier chain, new variables,
   and self-transition wiring so the variable is driven internally.
3. Writes the mutated XML back to disk (caller repacks to .hkx).

No Papyrus, MGEF, or ENCH records are emitted.
"""
from __future__ import annotations

import copy
import logging
import os
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path

import yaml

_log = logging.getLogger("conversion.behavior.driver_synth")

# Default driver tunables — used when ``behavior_drivers.yaml`` has no
# entry for the detected variable name.
_FALLBACK_PATTERN: dict = {
    "ramp_events": ["WeaponFire"],
    "decay_events": ["WeaponSheathe"],
    "damping_kp": 0.15,
    "ramp_target": 1.0,
    "decay_target": 0.0,
    "initial_value": 0.0,
}


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class BehaviorDriverSpec:
    """A single (behavior, variable) tuple that needs an internal driver.

    The renderer uses this to emit an HKX modifier chain:
      * a new "raw" companion variable (the target value, stepped on events)
      * a new "damp rate" companion variable (const, not externally bound)
      * a ``hkbDampingModifier`` that writes ``variable_name`` from the raw
        companion with kP = ``damping_kp``
      * a ``hkbModifierGenerator`` wrapping the existing generator so the
        damper ticks every frame
      * self-transitions on the hosting state that pulse the raw companion
        to ``ramp_target`` on ``ramp_events`` and ``decay_target`` on
        ``decay_events``
    """

    behavior_path: str  # forward-slash rel path under data/meshes/
    behavior_name: str  # bare folder name, e.g. "MeltdownFX"
    variable_name: str  # declared telemetry variable, e.g. "fOverheatAmount"
    sink_member_path: str  # member path the variable binds to, e.g. "fTimePercent"
    ramp_events: tuple[str, ...]
    decay_events: tuple[str, ...]
    initial_value: float = 0.0
    ramp_target: float = 1.0
    decay_target: float = 0.0
    damping_kp: float = 0.15


@dataclass
class DriverConfig:
    """Loaded ``behavior_drivers.yaml`` content."""

    variable_patterns: dict[str, dict] = field(default_factory=dict)
    telemetry_sinks: tuple[str, ...] = ()
    internal_writer_classes: tuple[str, ...] = ()


# ---------------------------------------------------------------------------
# Config loading
# ---------------------------------------------------------------------------


def load_driver_config(config_path: str | os.PathLike[str] | None = None) -> DriverConfig:
    """Load ``behavior_drivers.yaml``. Defaults to the bundled file."""
    if config_path is None:
        config_path = Path(__file__).with_name("drivers.yaml")
    config_path = Path(config_path)
    if not config_path.is_file():
        _log.warning("behavior_drivers.yaml not found at %s — using defaults", config_path)
        return DriverConfig(
            variable_patterns={},
            telemetry_sinks=("fTimePercent", "fPercent", "fOverheatAmount"),
            internal_writer_classes=("BSTimerModifier", "hkbDampingModifier"),
        )
    raw = yaml.safe_load(config_path.read_text(encoding="utf-8")) or {}
    return DriverConfig(
        variable_patterns=raw.get("variable_patterns", {}) or {},
        telemetry_sinks=tuple(raw.get("telemetry_sinks", []) or []),
        internal_writer_classes=tuple(raw.get("internal_writer_classes", []) or []),
    )


# ---------------------------------------------------------------------------
# HKX detection
# ---------------------------------------------------------------------------


def _xml_objects(root: ET.Element) -> list[ET.Element]:
    """Return all <hkobject> elements with a ``class`` attribute."""
    return [el for el in root.iter("hkobject") if el.get("class")]


def _read_hkparam_text(obj: ET.Element, name: str) -> str:
    for p in obj.findall("hkparam"):
        if p.get("name") == name:
            return (p.text or "").strip()
    return ""


def _read_hkstrings(obj: ET.Element, name: str) -> list[str]:
    for p in obj.findall("hkparam"):
        if p.get("name") == name:
            return [(s.text or "").strip() for s in p.findall("hkcstring")]
    return []


def _binding_set_member_paths(
    objects_by_id: dict[str, ET.Element], binding_set_ref: str
) -> list[tuple[str, int]]:
    """Return ``[(memberPath, variableIndex)]`` for a binding set reference."""
    if not binding_set_ref or binding_set_ref == "null":
        return []
    binding_set = objects_by_id.get(binding_set_ref)
    if binding_set is None:
        return []
    pairs: list[tuple[str, int]] = []
    for p in binding_set.findall("hkparam"):
        if p.get("name") != "bindings":
            continue
        for child in p.findall("hkobject"):
            mp = _read_hkparam_text(child, "memberPath")
            vi_raw = _read_hkparam_text(child, "variableIndex")
            try:
                vi = int(vi_raw)
            except ValueError:
                continue
            if mp:
                pairs.append((mp, vi))
    return pairs


def detect_unbound_variables(
    xml_path: str | os.PathLike[str],
    config: DriverConfig | None = None,
) -> list[tuple[str, str]]:
    """Scan unpacked behavior XML for telemetry variables needing a driver.

    Returns a list of ``(variable_name, sink_member_path)`` tuples for
    variables that are:
      * declared in ``hkbBehaviorGraphStringData.variableNames``,
      * bound (via any ``hkbVariableBindingSet``) to a memberPath whose
        simple name matches one of ``config.telemetry_sinks``, AND
      * NOT written to by any modifier-class object referencing the same
        variable index.
    """
    cfg = config or load_driver_config()
    sinks = {s.lower() for s in cfg.telemetry_sinks}
    writer_classes = {c.lower() for c in cfg.internal_writer_classes}

    tree = ET.parse(os.fspath(xml_path))
    root = tree.getroot()
    objects = _xml_objects(root)
    objects_by_id = {o.get("name", ""): o for o in objects}

    variable_names: list[str] = []
    for obj in objects:
        if obj.get("class") == "hkbBehaviorGraphStringData":
            variable_names = _read_hkstrings(obj, "variableNames")
            break

    if not variable_names:
        return []

    sink_bindings: dict[int, str] = {}
    written_indices: set[int] = set()
    for obj in objects:
        cls = (obj.get("class") or "").strip()
        binding_ref = _read_hkparam_text(obj, "variableBindingSet")
        bindings = _binding_set_member_paths(objects_by_id, binding_ref)
        for member_path, vi in bindings:
            if vi < 0 or vi >= len(variable_names):
                continue
            simple = member_path.split(".")[-1].split("/")[-1].lower()
            if simple in sinks:
                sink_bindings.setdefault(vi, member_path)
        cls_lower = cls.lower()
        if cls_lower in writer_classes or (
            "modifier" in cls_lower and cls_lower != "hkbvariablebindingset"
        ):
            for _, vi in bindings:
                if 0 <= vi < len(variable_names):
                    written_indices.add(vi)

    results: list[tuple[str, str]] = []
    for vi, member_path in sink_bindings.items():
        if vi in written_indices:
            continue
        results.append((variable_names[vi], member_path))
    return results


def specs_for_behavior(
    behavior_xml_path: str | os.PathLike[str],
    behavior_rel_path: str,
    config: DriverConfig | None = None,
) -> list[BehaviorDriverSpec]:
    """Build a BehaviorDriverSpec list for one behavior XML file."""
    cfg = config or load_driver_config()
    detections = detect_unbound_variables(behavior_xml_path, cfg)
    if not detections:
        return []

    behavior_name = _behavior_name_from_path(behavior_rel_path)
    specs: list[BehaviorDriverSpec] = []
    for var_name, sink in detections:
        pattern = _resolve_pattern(var_name, sink, cfg)
        specs.append(
            BehaviorDriverSpec(
                behavior_path=behavior_rel_path.replace("\\", "/"),
                behavior_name=behavior_name,
                variable_name=var_name,
                sink_member_path=sink,
                ramp_events=tuple(pattern["ramp_events"]),
                decay_events=tuple(pattern["decay_events"]),
                initial_value=float(pattern.get("initial_value", 0.0)),
                ramp_target=float(pattern.get("ramp_target", 1.0)),
                decay_target=float(pattern.get("decay_target", 0.0)),
                damping_kp=float(pattern.get("damping_kp", 0.15)),
            )
        )
    return specs


def _resolve_pattern(var_name: str, sink: str, cfg: DriverConfig) -> dict:
    """Look up a driver pattern by variable name then by sink path."""
    table = cfg.variable_patterns or {}
    table_lower = {k.lower(): v for k, v in table.items()}
    candidate = table_lower.get(var_name.lower())
    if candidate is None:
        candidate = table_lower.get(sink.split(".")[-1].lower())
    if candidate is None:
        return dict(_FALLBACK_PATTERN)
    merged = dict(_FALLBACK_PATTERN)
    merged.update(candidate)
    return merged


def _behavior_name_from_path(rel_path: str) -> str:
    parts = rel_path.replace("\\", "/").split("/")
    if "UniqueBehaviors" in parts:
        idx = parts.index("UniqueBehaviors")
        if idx + 1 < len(parts):
            return parts[idx + 1]
    if len(parts) >= 3:
        return parts[-3]
    return Path(rel_path).stem


# ---------------------------------------------------------------------------
# HKX XML mutation — the actual "synthesis"
# ---------------------------------------------------------------------------


def _fmt_float(value: float) -> str:
    """Havok packfile XML float format: 6 decimal places, always signed
    integer dot like vanilla."""
    return f"{value:.6f}"


class _IdAllocator:
    """Allocates unique ``#NNNN`` object IDs that don't collide with
    existing IDs in the parsed XML."""

    def __init__(self, existing_ids: set[str]) -> None:
        self._used = set(existing_ids)
        # Start past the largest existing ID so new objects sort at the end,
        # matching the vanilla convention where later-authored nodes get
        # higher IDs. Bump forward past any numeric max.
        max_num = 0
        for ident in existing_ids:
            stripped = ident.lstrip("#").lstrip("0") or "0"
            try:
                max_num = max(max_num, int(stripped, 10))
            except ValueError:
                continue
        self._next = max_num + 1

    def next(self) -> str:
        while True:
            ident = f"#{self._next:04d}"
            self._next += 1
            if ident not in self._used:
                self._used.add(ident)
                return ident


def _hkobject(object_id: str, class_name: str, signature: str) -> ET.Element:
    el = ET.Element("hkobject")
    el.set("name", object_id)
    el.set("class", class_name)
    el.set("signature", signature)
    return el


def _hkparam(parent: ET.Element, name: str, text: str | None = None) -> ET.Element:
    p = ET.SubElement(parent, "hkparam")
    p.set("name", name)
    if text is not None:
        p.text = text
    return p


def _binding(
    member_path: str,
    variable_index: int,
    binding_type: str = "BINDING_TYPE_VARIABLE",
) -> ET.Element:
    """Build a ``<hkobject class="hkbVariableBindingSetBinding">`` element."""
    b = ET.Element("hkobject", {"class": "hkbVariableBindingSetBinding", "signature": "0x4d592f72"})
    _hkparam(b, "memberPath", member_path)
    _hkparam(b, "memberClass", "null")
    _hkparam(b, "offsetInObjectPlusOne", "0")
    _hkparam(b, "offsetInArrayPlusOne", "0")
    _hkparam(b, "rootVariableIndex", "0")
    _hkparam(b, "variableIndex", str(variable_index))
    _hkparam(b, "bitIndex", "-1")
    _hkparam(b, "bindingType", binding_type)
    _hkparam(b, "memberType", "0")
    _hkparam(b, "variableType", "0")
    _hkparam(b, "flags", "0")
    return b


def _build_binding_set(
    obj_id: str, bindings: list[ET.Element]
) -> ET.Element:
    bs = _hkobject(obj_id, "hkbVariableBindingSet", "0xe942f339")
    bindings_param = _hkparam(bs, "bindings")
    bindings_param.set("numelements", str(len(bindings)))
    for b in bindings:
        bindings_param.append(b)
    _hkparam(bs, "indexOfBindingToEnable", "-1")
    return bs


def _build_damping_modifier(
    obj_id: str,
    name: str,
    kp: float,
    binding_set_ref: str,
) -> ET.Element:
    mod = _hkobject(obj_id, "hkbDampingModifier", "0x68a51d05")
    _hkparam(mod, "variableBindingSet", binding_set_ref)
    _hkparam(mod, "userData", "1")
    _hkparam(mod, "name", name)
    _hkparam(mod, "enable", "true")
    _hkparam(mod, "kP", _fmt_float(kp))
    _hkparam(mod, "kI", _fmt_float(0.0))
    _hkparam(mod, "kD", _fmt_float(0.0))
    _hkparam(mod, "enableScalarDamping", "true")
    _hkparam(mod, "enableVectorDamping", "false")
    _hkparam(mod, "rawValue", _fmt_float(0.0))
    _hkparam(mod, "dampedValue", _fmt_float(0.0))
    _hkparam(mod, "rawVector", "(0.000000 0.000000 0.000000 0.000000)")
    _hkparam(mod, "dampedVector", "(0.000000 0.000000 0.000000 0.000000)")
    _hkparam(mod, "vecErrorSum", "(0.000000 0.000000 0.000000 0.000000)")
    _hkparam(mod, "vecPreviousError", "(0.000000 0.000000 0.000000 0.000000)")
    _hkparam(mod, "errorSum", _fmt_float(0.0))
    _hkparam(mod, "previousError", _fmt_float(0.0))
    return mod


def _build_assign_variables_modifier(
    obj_id: str,
    name: str,
    binding_set_ref: str,
    float_values: list[float],
) -> ET.Element:
    """Build a ``BSAssignVariablesModifier`` that assigns up to 20 float
    values into bound variables every tick.

    The modifier has 20 ``floatValueN`` slots and 20 ``floatVariableN``
    slots; the bound set points the slot indices at target variables via
    ``memberPath="floatVariableN"`` bindings. Our callers typically bind
    ``floatVariable1`` to the raw companion variable and leave slots 2-20
    untouched, mirroring Cryolator's #0040 with only slots 1+2 bound.

    ``float_values`` are the ``floatValue1..N`` constants (missing slots
    default to 0.0 per vanilla).
    """
    mod = _hkobject(obj_id, "BSAssignVariablesModifier", "0x64a6ca08")
    _hkparam(mod, "variableBindingSet", binding_set_ref)
    _hkparam(mod, "userData", "0")
    _hkparam(mod, "name", name)
    _hkparam(mod, "enable", "true")
    for i in range(1, 21):
        _hkparam(mod, f"floatVariable{i}", _fmt_float(0.0))
        value = float_values[i - 1] if i - 1 < len(float_values) else 0.0
        _hkparam(mod, f"floatValue{i}", _fmt_float(value))
    for i in range(1, 5):
        _hkparam(mod, f"intVariable{i}", "0")
        _hkparam(mod, f"intValue{i}", "0")
    return mod


def _build_reference_pose_generator(obj_id: str, name: str) -> ET.Element:
    gen = _hkobject(obj_id, "hkbReferencePoseGenerator", "0xbc1536ee")
    _hkparam(gen, "variableBindingSet", "null")
    _hkparam(gen, "userData", "0")
    _hkparam(gen, "name", name)
    return gen


def _build_blending_transition_effect(obj_id: str, name: str) -> ET.Element:
    """Zero-duration transition effect — snap between states instantly.
    Vanilla Cryolator uses the same #0010 for both transitions."""
    eff = _hkobject(obj_id, "hkbBlendingTransitionEffect", "0x14e54c5c")
    _hkparam(eff, "variableBindingSet", "null")
    _hkparam(eff, "userData", "0")
    _hkparam(eff, "name", name)
    _hkparam(
        eff,
        "selfTransitionMode",
        "SELF_TRANSITION_MODE_CONTINUE_IF_CYCLIC_BLEND_IF_ACYCLIC",
    )
    _hkparam(eff, "eventMode", "EVENT_MODE_DEFAULT")
    _hkparam(eff, "duration", _fmt_float(0.0))
    _hkparam(eff, "toGeneratorStartTimeFraction", _fmt_float(0.0))
    _hkparam(eff, "flags", "FLAG_NONE")
    _hkparam(eff, "endMode", "END_MODE_NONE")
    _hkparam(eff, "blendCurve", "0")
    _hkparam(eff, "alignmentBone", "-1")
    return eff


def _build_transition_info(
    event_id: int,
    to_state_id: int,
    transition_effect_ref: str,
) -> ET.Element:
    """One row in ``hkbStateMachineTransitionInfoArray.transitions``."""
    tr = ET.Element(
        "hkobject",
        {"class": "hkbStateMachineTransitionInfo", "signature": "0xcdec8025"},
    )
    trigger = _hkparam(tr, "triggerInterval")
    t_obj = ET.SubElement(
        trigger,
        "hkobject",
        {"class": "hkbStateMachineTimeInterval", "signature": "0x60a881e5"},
    )
    _hkparam(t_obj, "enterEventId", "-1")
    _hkparam(t_obj, "exitEventId", "-1")
    _hkparam(t_obj, "enterTime", _fmt_float(0.0))
    _hkparam(t_obj, "exitTime", _fmt_float(0.0))
    initiate = _hkparam(tr, "initiateInterval")
    i_obj = ET.SubElement(
        initiate,
        "hkobject",
        {"class": "hkbStateMachineTimeInterval", "signature": "0x60a881e5"},
    )
    _hkparam(i_obj, "enterEventId", "-1")
    _hkparam(i_obj, "exitEventId", "-1")
    _hkparam(i_obj, "enterTime", _fmt_float(0.0))
    _hkparam(i_obj, "exitTime", _fmt_float(0.0))
    _hkparam(tr, "transition", transition_effect_ref)
    _hkparam(tr, "condition", "null")
    _hkparam(tr, "eventId", str(event_id))
    _hkparam(tr, "toStateId", str(to_state_id))
    _hkparam(tr, "fromNestedStateId", "0")
    _hkparam(tr, "toNestedStateId", "0")
    _hkparam(tr, "priority", "0")
    _hkparam(tr, "flags", "0")
    return tr


def _build_transition_info_array(
    obj_id: str, transitions: list[ET.Element]
) -> ET.Element:
    arr = _hkobject(obj_id, "hkbStateMachineTransitionInfoArray", "0x704a19af")
    trans_param = _hkparam(arr, "transitions")
    trans_param.set("numelements", str(len(transitions)))
    for t in transitions:
        trans_param.append(t)
    return arr


def _build_state_machine_state_info(
    obj_id: str,
    name: str,
    state_id: int,
    generator_ref: str,
    transitions_ref: str,
) -> ET.Element:
    st = _hkobject(obj_id, "hkbStateMachineStateInfo", "0x39d76713")
    _hkparam(st, "variableBindingSet", "null")
    listeners = _hkparam(st, "listeners")
    listeners.set("numelements", "0")
    _hkparam(st, "enterNotifyEvents", "null")
    _hkparam(st, "exitNotifyEvents", "null")
    _hkparam(st, "transitions", transitions_ref)
    _hkparam(st, "generator", generator_ref)
    _hkparam(st, "name", name)
    _hkparam(st, "stateId", str(state_id))
    _hkparam(st, "probability", _fmt_float(1.0))
    _hkparam(st, "enable", "true")
    return st


def _build_inner_state_machine(
    obj_id: str,
    name: str,
    start_state_id: int,
    state_refs: list[str],
) -> ET.Element:
    sm = _hkobject(obj_id, "hkbStateMachine", "0xa5896bcf")
    _hkparam(sm, "variableBindingSet", "null")
    _hkparam(sm, "userData", "0")
    _hkparam(sm, "name", name)
    # eventToSendWhenStateOrTransitionChanges — required nested event stub
    ev = _hkparam(sm, "eventToSendWhenStateOrTransitionChanges")
    ev_obj = ET.SubElement(
        ev, "hkobject", {"class": "hkbEvent", "signature": "0x3e0fd810"}
    )
    _hkparam(ev_obj, "id", "-1")
    _hkparam(ev_obj, "payload", "null")
    _hkparam(ev_obj, "sender", "null")
    _hkparam(sm, "startStateIdSelector", "null")
    _hkparam(sm, "startStateId", str(start_state_id))
    _hkparam(sm, "returnToPreviousStateEventId", "-1")
    _hkparam(sm, "randomTransitionEventId", "-1")
    _hkparam(sm, "transitionToNextHigherStateEventId", "-1")
    _hkparam(sm, "transitionToNextLowerStateEventId", "-1")
    _hkparam(sm, "syncVariableIndex", "-1")
    _hkparam(sm, "wrapAroundStateId", "false")
    _hkparam(sm, "maxSimultaneousTransitions", "32")
    _hkparam(sm, "startStateMode", "START_STATE_MODE_DEFAULT")
    _hkparam(sm, "selfTransitionMode", "SELF_TRANSITION_MODE_NO_TRANSITION")
    states_param = _hkparam(sm, "states")
    states_param.set("numelements", str(len(state_refs)))
    for sr in state_refs:
        ref_el = ET.SubElement(states_param, "hkobject")
        ref_el.text = sr
    _hkparam(sm, "wildcardTransitions", "null")
    return sm


def _build_modifier_generator(
    obj_id: str, name: str, modifier_ref: str, inner_generator_ref: str
) -> ET.Element:
    gen = _hkobject(obj_id, "hkbModifierGenerator", "0xc499fc9e")
    _hkparam(gen, "variableBindingSet", "null")
    _hkparam(gen, "userData", "1")
    _hkparam(gen, "name", name)
    _hkparam(gen, "modifier", modifier_ref)
    _hkparam(gen, "generator", inner_generator_ref)
    return gen


def _find_data_section(root: ET.Element) -> ET.Element:
    """Return the ``<hksection name="__data__">`` element."""
    for section in root.iter("hksection"):
        if section.get("name") == "__data__":
            return section
    raise ValueError("HKX XML has no <hksection name=\"__data__\">")


def _extend_string_array(obj: ET.Element, param_name: str, new_items: list[str]) -> int:
    """Append ``new_items`` to a named ``<hkparam>`` list of ``<hkcstring>``.

    Returns the index of the first appended item.
    """
    for p in obj.findall("hkparam"):
        if p.get("name") != param_name:
            continue
        existing = p.findall("hkcstring")
        start = len(existing)
        for s in new_items:
            el = ET.SubElement(p, "hkcstring")
            el.text = s
        p.set("numelements", str(start + len(new_items)))
        return start
    raise ValueError(f"hkparam {param_name!r} not found")


def _extend_variable_infos_and_initial_values(
    objects_by_id: dict[str, ET.Element],
    behavior_data_obj: ET.Element,
    new_variables: list[tuple[str, float]],
) -> list[int]:
    """Append ``(name, initial_value)`` tuples to the behavior's variable
    registry. Returns the assigned indices (same order as input).

    Mutates:
      * ``hkbBehaviorGraphData.variableInfos`` (add one per var)
      * ``hkbBehaviorGraphStringData.variableNames`` (add name)
      * ``hkbVariableValueSet.wordVariableValues`` (add initial value as
        a 32-bit IEEE float bit-pattern int, matching vanilla encoding)
    """
    import struct

    # Resolve referenced sub-objects
    string_data_ref = _read_hkparam_text(behavior_data_obj, "stringData")
    initial_values_ref = _read_hkparam_text(behavior_data_obj, "variableInitialValues")
    string_data = objects_by_id.get(string_data_ref)
    initial_values = objects_by_id.get(initial_values_ref)
    if string_data is None or initial_values is None:
        raise ValueError("behavior graph data missing stringData or variableInitialValues")

    # 1. Append variableInfos
    var_infos: ET.Element | None = None
    for p in behavior_data_obj.findall("hkparam"):
        if p.get("name") == "variableInfos":
            var_infos = p
            break
    if var_infos is None:
        raise ValueError("behavior graph data missing variableInfos")
    existing_count = len(var_infos.findall("hkobject"))
    for _ in new_variables:
        info = ET.SubElement(var_infos, "hkobject", {
            "class": "hkbVariableInfo", "signature": "0xa5ae6be2"
        })
        role_param = _hkparam(info, "role")
        role_obj = ET.SubElement(role_param, "hkobject", {
            "class": "hkbRoleAttribute", "signature": "0xfecef669"
        })
        _hkparam(role_obj, "role", "ROLE_DEFAULT")
        _hkparam(role_obj, "flags", "FLAG_NONE")
        _hkparam(info, "type", "VARIABLE_TYPE_REAL")
    var_infos.set("numelements", str(existing_count + len(new_variables)))

    # 2. Append variable names
    _extend_string_array(string_data, "variableNames", [n for n, _ in new_variables])

    # 3. Append initial values as 32-bit float bit-patterns (matching
    # vanilla encoding — the <value>N</value> int is the IEEE-754 bits).
    word_values: ET.Element | None = None
    for p in initial_values.findall("hkparam"):
        if p.get("name") == "wordVariableValues":
            word_values = p
            break
    if word_values is None:
        raise ValueError("variableInitialValues missing wordVariableValues")
    existing_word_count = len(word_values.findall("hkobject"))
    for _, initial in new_variables:
        raw_bits = struct.unpack("<I", struct.pack("<f", float(initial)))[0]
        val_obj = ET.SubElement(word_values, "hkobject", {
            "class": "hkbVariableValue", "signature": "0x0b99bd6a"
        })
        _hkparam(val_obj, "value", str(raw_bits))
    word_values.set("numelements", str(existing_word_count + len(new_variables)))

    return [existing_count + i for i in range(len(new_variables))]


def _find_event_id(
    string_data: ET.Element, event_name: str
) -> int:
    """Return the 0-based index of ``event_name`` in ``eventNames``, or -1."""
    names = _read_hkstrings(string_data, "eventNames")
    for i, n in enumerate(names):
        if n.lower() == event_name.lower():
            return i
    return -1


def _pretty_indent(root: ET.Element) -> None:
    """ElementTree 3.9+ ``indent`` applied in place for readable XML
    output. The packer parses either form, but nice-looking diffs help
    debugging a lot."""
    try:
        ET.indent(root, space="    ", level=0)
    except AttributeError:
        # Older Python; skip indentation. Packer still handles it.
        pass


def inject_driver_chain(
    xml_path: str | os.PathLike[str],
    spec: BehaviorDriverSpec,
) -> None:
    """Mutate the unpacked behavior XML so ``spec.variable_name`` is driven
    internally by a Havok modifier chain with event-driven pulsing.

    Writes the mutated tree back to ``xml_path``. Caller repacks to .hkx.

    Strategy (matches vanilla FO4 Cryolator / Flamer / Minigun):

    1. Append helper variables ``<var>_Raw`` + ``<var>_DampRate``.
    2. Append a ``hkbDampingModifier`` that writes ``<var>`` (dampedValue)
       from ``<var>_Raw`` (rawValue) with kP = ``<var>_DampRate``.
    3. Wrap the host state's original generator in a ``hkbModifierGenerator``
       so the damper ticks every frame — same as v2.
    4. Build an inner 2-state ``hkbStateMachine``:
         * IdleState (startState, stateId=0): generator is a
           ``hkbModifierGenerator`` whose modifier is a
           ``BSAssignVariablesModifier`` that writes ``decay_target``
           to ``<var>_Raw`` each tick, and whose inner generator is a
           ``hkbReferencePoseGenerator`` (no-op pose).
         * FiringState (stateId=1): generator is the damper-wrapped
           original generator from step 3, with a ``BSAssignVariablesModifier``
           prepended via another ``hkbModifierGenerator`` that writes
           ``ramp_target`` to ``<var>_Raw``.
       Transitions: Idle -> Firing on each ramp event, Firing -> Idle
       on each decay event. Missing event names are added to the
       behavior's event table so the transition eventIds resolve.
    5. Re-point the host state's generator at this inner state machine.
    6. Seed ``<var>_Raw``'s initial value to ``decay_target`` so the
       FX starts idle on weapon equip.

    Idempotent: if the behavior already contains a ``hkbDampingModifier``
    writing ``variable_name``, this function is a no-op.
    """
    tree = ET.parse(os.fspath(xml_path))
    root = tree.getroot()
    data_section = _find_data_section(root)

    objects = _xml_objects(root)
    objects_by_id = {o.get("name", ""): o for o in objects}
    existing_ids = set(objects_by_id.keys())
    ids = _IdAllocator(existing_ids)

    # Locate key objects
    graph = _first_object_of_class(objects, "hkbBehaviorGraph")
    if graph is None:
        raise ValueError("no hkbBehaviorGraph in XML")
    behavior_data_ref = _read_hkparam_text(graph, "data")
    behavior_data = objects_by_id.get(behavior_data_ref)
    if behavior_data is None:
        raise ValueError("hkbBehaviorGraph.data reference not resolvable")
    string_data_ref = _read_hkparam_text(behavior_data, "stringData")
    string_data = objects_by_id[string_data_ref]

    variable_names = _read_hkstrings(string_data, "variableNames")
    try:
        target_var_index = variable_names.index(spec.variable_name)
    except ValueError:
        raise ValueError(
            f"variable {spec.variable_name!r} not declared in this behavior"
        )

    # Idempotency check: scan for an existing damping modifier binding its
    # dampedValue to the target variable.
    for obj in objects:
        if obj.get("class") != "hkbDampingModifier":
            continue
        binding_ref = _read_hkparam_text(obj, "variableBindingSet")
        for mp, vi in _binding_set_member_paths(objects_by_id, binding_ref):
            if mp == "dampedValue" and vi == target_var_index:
                _log.info(
                    "[inject_driver_chain] %s already has damper writing %s; skip",
                    spec.behavior_name, spec.variable_name,
                )
                return

    # Step 1: add Raw + DampRate helper variables. Raw's initial value
    # is decay_target (idle on equip); DampRate's is damping_kp (const).
    raw_name = f"{spec.variable_name}_Raw"
    damp_rate_name = f"{spec.variable_name}_DampRate"
    new_indices = _extend_variable_infos_and_initial_values(
        objects_by_id,
        behavior_data,
        [(raw_name, spec.decay_target), (damp_rate_name, spec.damping_kp)],
    )
    raw_var_index, damp_rate_var_index = new_indices

    # Step 2: build the damping modifier + its binding set
    damp_binding_set_id = ids.next()
    damp_modifier_id = ids.next()
    damp_binding_set = _build_binding_set(damp_binding_set_id, [
        _binding("kP", damp_rate_var_index),
        _binding("rawValue", raw_var_index),
        _binding("dampedValue", target_var_index),
    ])
    damp_modifier = _build_damping_modifier(
        damp_modifier_id,
        name=f"{spec.variable_name}_DampingModifier",
        kp=spec.damping_kp,
        binding_set_ref=damp_binding_set_id,
    )

    # Step 3: locate the host state and wrap its generator with the damper.
    root_generator_ref = _read_hkparam_text(graph, "rootGenerator")
    host_state, host_state_machine = _find_host_state(objects_by_id, root_generator_ref)
    if host_state is None:
        raise ValueError("could not locate a host state to wrap")

    original_gen_ref = _read_hkparam_text(host_state, "generator")
    if not original_gen_ref or original_gen_ref == "null":
        raise ValueError("host state has no generator to wrap")

    damper_gen_id = ids.next()
    damper_gen = _build_modifier_generator(
        damper_gen_id,
        name=f"{spec.variable_name}_DamperGen",
        modifier_ref=damp_modifier_id,
        inner_generator_ref=original_gen_ref,
    )

    # Step 4: ensure all ramp / decay event names exist in the event
    # table. Unknown events get appended so the transition eventIds
    # resolve. We must do this BEFORE allocating state-machine pieces
    # so the recorded indices are stable.
    ramp_event_ids = _resolve_or_append_event_ids(
        string_data, list(spec.ramp_events) or ["WeaponFire"]
    )
    decay_event_ids = _resolve_or_append_event_ids(
        string_data, list(spec.decay_events) or ["WeaponSheathe"]
    )
    # Also bump hkbBehaviorGraphData.eventInfos to match the new event
    # count — without this, the behavior fails to load (eventInfos array
    # must be parallel to eventNames).
    _extend_event_infos(behavior_data, string_data)

    # Step 5: build the IdleState — BSAssignVariablesModifier writes
    # decay_target to <var>_Raw, inner generator is a reference pose.
    idle_binding_set_id = ids.next()
    idle_binding_set = _build_binding_set(
        idle_binding_set_id,
        [_binding("floatVariable1", raw_var_index)],
    )
    idle_assign_id = ids.next()
    idle_assign = _build_assign_variables_modifier(
        idle_assign_id,
        name=f"{spec.variable_name}_AssignIdle",
        binding_set_ref=idle_binding_set_id,
        float_values=[spec.decay_target],
    )
    idle_ref_pose_id = ids.next()
    idle_ref_pose = _build_reference_pose_generator(
        idle_ref_pose_id, name=f"{spec.variable_name}_IdleRefPose"
    )
    idle_mod_gen_id = ids.next()
    idle_mod_gen = _build_modifier_generator(
        idle_mod_gen_id,
        name=f"{spec.variable_name}_IdleModGen",
        modifier_ref=idle_assign_id,
        inner_generator_ref=idle_ref_pose_id,
    )

    # Step 6: build the FiringState — BSAssignVariablesModifier writes
    # ramp_target to <var>_Raw, inner generator is the damper-wrapped
    # original.
    fire_binding_set_id = ids.next()
    fire_binding_set = _build_binding_set(
        fire_binding_set_id,
        [_binding("floatVariable1", raw_var_index)],
    )
    fire_assign_id = ids.next()
    fire_assign = _build_assign_variables_modifier(
        fire_assign_id,
        name=f"{spec.variable_name}_AssignFire",
        binding_set_ref=fire_binding_set_id,
        float_values=[spec.ramp_target],
    )
    fire_mod_gen_id = ids.next()
    fire_mod_gen = _build_modifier_generator(
        fire_mod_gen_id,
        name=f"{spec.variable_name}_FireModGen",
        modifier_ref=fire_assign_id,
        inner_generator_ref=damper_gen_id,
    )

    # Step 7: build the shared zero-duration transition effect.
    transition_effect_id = ids.next()
    transition_effect = _build_blending_transition_effect(
        transition_effect_id, name=f"{spec.variable_name}_SnapTransition"
    )

    # Step 8: build transition arrays.
    # Idle state: one transition per ramp event → firing (stateId=1).
    idle_transitions = [
        _build_transition_info(
            event_id=eid,
            to_state_id=1,
            transition_effect_ref=transition_effect_id,
        )
        for eid in ramp_event_ids
    ]
    idle_trans_arr_id = ids.next()
    idle_trans_arr = _build_transition_info_array(idle_trans_arr_id, idle_transitions)

    # Firing state: one transition per decay event → idle (stateId=0).
    fire_transitions = [
        _build_transition_info(
            event_id=eid,
            to_state_id=0,
            transition_effect_ref=transition_effect_id,
        )
        for eid in decay_event_ids
    ]
    fire_trans_arr_id = ids.next()
    fire_trans_arr = _build_transition_info_array(fire_trans_arr_id, fire_transitions)

    # Step 9: build the two state-info objects.
    idle_state_id = ids.next()
    idle_state = _build_state_machine_state_info(
        idle_state_id,
        name=f"{spec.variable_name}_IdleState",
        state_id=0,
        generator_ref=idle_mod_gen_id,
        transitions_ref=idle_trans_arr_id,
    )
    fire_state_id = ids.next()
    fire_state = _build_state_machine_state_info(
        fire_state_id,
        name=f"{spec.variable_name}_FiringState",
        state_id=1,
        generator_ref=fire_mod_gen_id,
        transitions_ref=fire_trans_arr_id,
    )

    # Step 10: build the inner state machine and re-point the host state
    # at it.
    inner_sm_id = ids.next()
    inner_sm = _build_inner_state_machine(
        inner_sm_id,
        name=f"{spec.variable_name}_DriverStateMachine",
        start_state_id=0,  # idle
        state_refs=[idle_state_id, fire_state_id],
    )
    for p in host_state.findall("hkparam"):
        if p.get("name") == "generator":
            p.text = inner_sm_id
            break

    # Step 11: emit all new objects into the data section. Order doesn't
    # affect the packer but post-host placement reads well in diffs.
    for el in (
        damp_binding_set,
        damp_modifier,
        damper_gen,
        idle_binding_set,
        idle_assign,
        idle_ref_pose,
        idle_mod_gen,
        fire_binding_set,
        fire_assign,
        fire_mod_gen,
        transition_effect,
        idle_trans_arr,
        fire_trans_arr,
        idle_state,
        fire_state,
        inner_sm,
    ):
        data_section.append(el)

    _pretty_indent(root)
    tree.write(os.fspath(xml_path), encoding="ASCII", xml_declaration=True)
    _log.info(
        "[inject_driver_chain] %s: event-driven chain installed — "
        "raw_var_index=%d ramp_events=%s decay_events=%s",
        spec.behavior_name,
        raw_var_index,
        ramp_event_ids,
        decay_event_ids,
    )


def _resolve_or_append_event_ids(
    string_data: ET.Element, event_names: list[str]
) -> list[int]:
    """Return event-id indices for each name, appending missing ones."""
    existing = _read_hkstrings(string_data, "eventNames")
    lower_index = {n.lower(): i for i, n in enumerate(existing)}
    ids: list[int] = []
    to_append: list[str] = []
    for name in event_names:
        key = name.lower()
        if key in lower_index:
            ids.append(lower_index[key])
        else:
            # Will be appended; its index is len(existing) + len(to_append)
            ids.append(len(existing) + len(to_append))
            to_append.append(name)
            lower_index[key] = ids[-1]
    if to_append:
        _extend_string_array(string_data, "eventNames", to_append)
    return ids


def _extend_event_infos(
    behavior_data: ET.Element, string_data: ET.Element
) -> None:
    """Pad ``hkbBehaviorGraphData.eventInfos`` to match the event-name count."""
    name_count = len(_read_hkstrings(string_data, "eventNames"))
    for p in behavior_data.findall("hkparam"):
        if p.get("name") != "eventInfos":
            continue
        existing = p.findall("hkobject")
        while len(existing) < name_count:
            info = ET.SubElement(
                p,
                "hkobject",
                {"class": "hkbEventInfo", "signature": "0x5874eed4"},
            )
            _hkparam(info, "flags", "0")
            existing = p.findall("hkobject")
        p.set("numelements", str(len(existing)))
        return


def _first_object_of_class(
    objects: list[ET.Element], class_name: str
) -> ET.Element | None:
    for obj in objects:
        if obj.get("class") == class_name:
            return obj
    return None


def _find_host_state(
    objects_by_id: dict[str, ET.Element], root_generator_ref: str
) -> tuple[ET.Element | None, ET.Element | None]:
    """Follow the graph from ``rootGenerator`` until we find an
    ``hkbStateMachineStateInfo``. Returns ``(host_state, parent_state_machine)``
    or ``(None, None)`` if the structure is unexpected.
    """
    current = objects_by_id.get(root_generator_ref)
    state_machine: ET.Element | None = None
    while current is not None:
        cls = current.get("class")
        if cls == "hkbStateMachine":
            state_machine = current
            # Take the first state in ``states``
            states_param: ET.Element | None = None
            for p in current.findall("hkparam"):
                if p.get("name") == "states":
                    states_param = p
                    break
            if states_param is None:
                return _single_state_info(objects_by_id), state_machine
            first_state_ref_el = states_param.find("hkobject")
            if first_state_ref_el is None:
                return _single_state_info(objects_by_id), state_machine
            first_state_ref = (first_state_ref_el.text or "").strip()
            current = objects_by_id.get(first_state_ref)
            continue
        if cls == "hkbStateMachineStateInfo":
            return current, state_machine
        # Unknown intermediate node — stop.
        break
    return None, state_machine


def _single_state_info(
    objects_by_id: dict[str, ET.Element],
) -> ET.Element | None:
    states = [
        obj
        for obj in objects_by_id.values()
        if obj.get("class") == "hkbStateMachineStateInfo"
    ]
    if len(states) == 1:
        return states[0]
    return None


# ---------------------------------------------------------------------------
# File-system helpers
# ---------------------------------------------------------------------------


def find_behavior_hkx_files(mod_data_meshes_dir: str | os.PathLike[str]) -> list[str]:
    """Return absolute paths to all .hkx behavior files under the mod's
    ``data/meshes/`` tree."""
    base = Path(mod_data_meshes_dir)
    if not base.is_dir():
        return []
    found: set[str] = set()
    for sub in (base / "UniqueBehaviors", base):
        if not sub.is_dir():
            continue
        for hkx in sub.rglob("*.hkx"):
            parts = {p.lower() for p in hkx.parts}
            if "behaviors" in parts or "uniquebehaviors" in parts:
                found.add(str(hkx))
    return sorted(found)


def behavior_rel_path(mod_path: str | os.PathLike[str], hkx_abs_path: str) -> str:
    """Return the forward-slash path of an HKX relative to the mod's data dir."""
    data_dir = Path(mod_path) / "data"
    try:
        rel = Path(hkx_abs_path).relative_to(data_dir)
    except ValueError:
        rel = Path(hkx_abs_path).relative_to(Path(mod_path))
    return str(rel).replace("\\", "/")
