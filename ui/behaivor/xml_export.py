"""XML export for Fallout 4 HKX behavior graph files.

Writes the hkpackfile XML format from the graph state.
Port of Scripts/behavior_parser.gd from the Godot implementation.
"""

import logging
import struct

log = logging.getLogger(__name__)


def _float_to_u32(value):
    """Convert float to unsigned 32-bit integer (reinterpret bytes)."""
    packed = struct.pack("<f", float(value))
    return struct.unpack("<I", packed)[0]


# Transition flag names, indexed 0-14
_TRANSITION_FLAG_NAMES = [
    "FLAG_USE_TRIGGER_INTERVAL",
    "FLAG_USE_INITIATE_INTERVAL",
    "FLAG_UNINTERRUPTIBLE_WHILE_PLAYING",
    "FLAG_UNINTERRUPTIBLE_WHILE_DELAYED",
    "FLAG_DELAY_STATE_CHANGE",
    "FLAG_DISABLED",
    "FLAG_DISALLOW_RETURN_TO_PREVIOUS_STATE",
    "FLAG_DISALLOW_RANDOM_TRANSITION",
    "FLAG_DISABLE_CONDITION",
    "FLAG_ALLOW_SELF_TRANSITION_BY_TRANSITION_FROM_ANY_STATE",
    "FLAG_IS_GLOBAL_WILDCARD",
    "FLAG_IS_LOCAL_WILDCARD",
    "FLAG_FROM_NESTED_STATE_ID_IS_VALID",
    "FLAG_TO_NESTED_STATE_ID_IS_VALID",
    "FLAG_ABUT_AT_END_OF_FROM_GENERATOR",
]


def _flags_to_string(flags):
    """Convert 15-element bool array to pipe-delimited flag string."""
    parts = []
    for i, flag_set in enumerate(flags):
        if flag_set and i < len(_TRANSITION_FLAG_NAMES):
            parts.append(_TRANSITION_FLAG_NAMES[i])
    return "|".join(parts) if parts else "0"


def export_xml_file(filepath, nodes_data, connections, global_state):
    """Export graph data to a Fallout 4 behavior XML file."""
    # Build connection lookup: (from_id, port_idx) -> [to_id, ...]
    conn_map = {}
    for port_idx, from_id, to_id in connections:
        key = (from_id, port_idx)
        conn_map.setdefault(key, []).append(to_id)

    # Sort nodes by nodeID
    nodes_data.sort(key=lambda x: x.get("nodeID", 0))

    # Calculate export index offset
    node_export_index = max((n.get("nodeID", 0) for n in nodes_data), default=90)

    transitions = global_state.transitions if hasattr(global_state, 'transitions') else []
    if isinstance(transitions, list) and transitions and isinstance(transitions[0], dict):
        pass
    else:
        transitions = [t.to_dict() if hasattr(t, 'to_dict') else t for t in transitions]

    payloads = global_state.payloads if hasattr(global_state, 'payloads') else []
    if isinstance(payloads, list) and payloads and isinstance(payloads[0], dict):
        pass
    else:
        payloads = [p.to_dict() if hasattr(p, 'to_dict') else p for p in payloads]

    variables = global_state.variables if hasattr(global_state, 'variables') else []
    if isinstance(variables, list) and variables and isinstance(variables[0], dict):
        pass
    else:
        variables = [v.to_dict() if hasattr(v, 'to_dict') else v for v in variables]

    events = global_state.events if hasattr(global_state, 'events') else []
    if isinstance(events, list) and events and isinstance(events[0], dict):
        pass
    else:
        events = [e.to_dict() if hasattr(e, 'to_dict') else e for e in events]

    properties = global_state.properties if hasattr(global_state, 'properties') else []
    if isinstance(properties, list) and properties and isinstance(properties[0], dict):
        pass
    else:
        properties = [p.to_dict() if hasattr(p, 'to_dict') else p for p in properties]

    trans_len = len(transitions)
    payload_len = len(payloads)

    with open(filepath, "w", encoding="ascii", newline="\r\n") as f:
        f.write('<?xml version="1.0" encoding="ASCII" standalone="no"?>\r\n')
        f.write('<hkpackfile classversion="11" contentsversion="hk_2014.1.0-r1">\r\n')
        f.write('    <hksection name="__data__">\r\n')

        for nd in nodes_data:
            type_id = nd.get("nodeTypeID", -1)
            node_id = nd.get("nodeID", 0)
            _write_node(f, nd, type_id, node_id, conn_map,
                        node_export_index, trans_len, payload_len)

        # Write transitions
        export_idx = node_export_index
        for trans in transitions:
            export_idx += 1
            _write_blending_transition(f, trans, export_idx)

        # Write payloads
        for payload in payloads:
            export_idx += 1
            _write_string_event_payload(f, payload, export_idx)

        # Write metadata nodes
        export_idx += 1
        _write_behavior_graph_data(f, variables, events, properties, export_idx)
        export_idx += 1
        _write_variable_value_set(f, variables, export_idx)
        export_idx += 1
        _write_string_data(f, variables, events, properties, export_idx)

        f.write('    </hksection>\r\n')
        f.write('</hkpackfile>\r\n')


def _get_conn(conn_map, node_id, port_idx, single=True):
    """Get connection target(s) for a node port."""
    targets = conn_map.get((node_id, port_idx), [])
    if single:
        return f"#{targets[0]}" if targets else "null"
    return targets


def _w(f, indent, text):
    """Write an indented line."""
    f.write(f"{'    ' * indent}{text}\r\n")


def _write_event_property(f, nd, event_name, event_id_key, payload_key,
                          export_idx, trans_len):
    """Write a nested hkbEventProperty block."""
    _w(f, 3, f'<hkparam name="{event_name}">')
    _w(f, 4, f'<hkobject class="hkbEventProperty" name="{event_name}" signature="0xdb38a15">')
    _w(f, 5, f'<hkparam name="id">{nd.get(event_id_key, -1)}</hkparam>')
    payload_idx = nd.get(payload_key, -1)
    if payload_idx != -1:
        _w(f, 5, f'<hkparam name="payload">#{export_idx + payload_idx + trans_len}</hkparam>')
    else:
        _w(f, 5, '<hkparam name="payload">null</hkparam>')
    _w(f, 4, '</hkobject>')
    _w(f, 3, '</hkparam>')


def _write_node(f, nd, type_id, node_id, conn_map, export_idx, trans_len, payload_len):
    """Write a single node as an hkobject XML element."""
    from .node_types import NODE_TYPE_DEFINITIONS
    defn = NODE_TYPE_DEFINITIONS.get(type_id)
    if defn is None or defn.get("metadata_only"):
        return

    xml_class = defn["xml_class"]
    sig = defn.get("signature", "0x0")

    # Standard modifier/generator header
    def _write_standard_header(has_vbs=True, is_generator=False):
        _w(f, 2, f'<hkobject class="{xml_class}" name="#{node_id}" signature="{sig}">')
        _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
        if has_vbs:
            vbs = _get_conn(conn_map, node_id, 0)
            _w(f, 3, f'<hkparam name="variableBindingSet">{vbs}</hkparam>')
        _w(f, 3, '<!-- cachedBindables SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- areBindablesCached SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- hasEnableChanged SERIALIZE_IGNORED -->')
        _w(f, 3, f'<hkparam name="userData">{nd.get("userData", 0)}</hkparam>')
        _w(f, 3, f'<hkparam name="name">{nd.get("nodeName", "")}</hkparam>')
        _w(f, 3, '<!-- id SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- cloneState SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- type SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- nodeInfo SERIALIZE_IGNORED -->')
        if is_generator:
            _w(f, 3, '<!-- partitionInfo SERIALIZE_IGNORED -->')
            _w(f, 3, '<!-- syncInfo SERIALIZE_IGNORED -->')
            for i in range(1, 5):
                _w(f, 3, f'<!-- pad{i} SERIALIZE_IGNORED -->')

    def _write_modifier_header():
        _write_standard_header()
        _w(f, 3, f'<hkparam name="enable">{str(nd.get("enable", True)).lower()}</hkparam>')
        _w(f, 3, '<!-- padModifier1 SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- padModifier2 SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- padModifier3 SERIALIZE_IGNORED -->')

    if type_id == 0:  # hkRootLevelContainer
        _w(f, 2, f'<hkobject class="hkRootLevelContainer" name="#{node_id}" signature="{sig}">')
        _w(f, 3, '<hkparam name="namedVariants" numelements="1">')
        _w(f, 4, '<hkobject>')
        _w(f, 5, f'<hkparam name="name">{nd.get("className", "hkbBehaviorGraph")}</hkparam>')
        _w(f, 5, f'<hkparam name="className">{nd.get("className", "hkbBehaviorGraph")}</hkparam>')
        _w(f, 5, f'<hkparam name="variant">{_get_conn(conn_map, node_id, 0)}</hkparam>')
        _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
        _w(f, 2, '</hkobject>')

    elif type_id == 1:  # hkbBehaviorGraph
        _write_standard_header(has_vbs=False, is_generator=True)
        _w(f, 3, '<hkparam name="variableBindingSet">null</hkparam>')
        _w(f, 3, '<hkparam name="variableMode">VARIABLE_MODE_DISCARD_WHEN_INACTIVE</hkparam>')
        _w(f, 3, f'<hkparam name="rootGenerator">{_get_conn(conn_map, node_id, 0)}</hkparam>')
        data_ref = export_idx + trans_len + payload_len + 1
        _w(f, 3, f'<hkparam name="data">#{data_ref}</hkparam>')
        _w(f, 2, '</hkobject>')

    elif type_id == 5:  # hkbStateMachine
        _write_standard_header(is_generator=True)
        _w(f, 3, '<hkparam name="eventToSendWhenStateOrTransitionChanges">')
        _w(f, 4, '<hkobject class="hkbEvent" name="eventToSendWhenStateOrTransitionChanges" signature="0x3e0fd810">')
        _w(f, 5, f'<hkparam name="id">{nd.get("eventId", -1)}</hkparam>')
        payload_idx = nd.get("payload", -1)
        if payload_idx != -1:
            _w(f, 5, f'<hkparam name="payload">#{export_idx + payload_idx + trans_len}</hkparam>')
        else:
            _w(f, 5, '<hkparam name="payload">null</hkparam>')
        _w(f, 5, '<!-- sender SERIALIZE_IGNORED -->')
        _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
        _w(f, 3, '<hkparam name="startStateIdSelector">null</hkparam>')
        _w(f, 3, f'<hkparam name="startStateId">{nd.get("startStateId", 0)}</hkparam>')
        _w(f, 3, '<hkparam name="returnToPreviousStateEventId">-1</hkparam>')
        _w(f, 3, f'<hkparam name="randomTransitionEventId">{nd.get("randomTransitionEventId", -1)}</hkparam>')
        _w(f, 3, f'<hkparam name="transitionToNextHigherStateEventId">{nd.get("transitionToNextHigherStateEventId", -1)}</hkparam>')
        _w(f, 3, f'<hkparam name="transitionToNextLowerStateEventId">{nd.get("transitionToNextLowerStateEventId", -1)}</hkparam>')
        _w(f, 3, f'<hkparam name="syncVariableIndex">{nd.get("syncVariableIndex", -1)}</hkparam>')
        _w(f, 3, f'<hkparam name="wrapAroundStateId">{nd.get("wrapAroundStateId", False)}</hkparam>')
        _w(f, 3, '<hkparam name="maxSimultaneousTransitions">32</hkparam>')
        _w(f, 3, f'<hkparam name="startStateMode">{nd.get("startStateMode", "START_STATE_MODE_DEFAULT")}</hkparam>')
        _w(f, 3, f'<hkparam name="selfTransitionMode">{nd.get("selfTransitionMode", "SELF_TRANSITION_MODE_NO_TRANSITION")}</hkparam>')
        states = _get_conn(conn_map, node_id, 1, single=False)
        _w(f, 3, f'<hkparam name="states" numelements="{len(states)}">')
        for s in states:
            _w(f, 3, f'#{s}')
        _w(f, 3, '</hkparam>')
        _w(f, 3, f'<hkparam name="wildcardTransitions">{_get_conn(conn_map, node_id, 2)}</hkparam>')
        _w(f, 2, '</hkobject>')

    elif type_id == 6:  # hkbStateMachineStateInfo
        _w(f, 2, f'<hkobject class="{xml_class}" name="#{node_id}" signature="{sig}">')
        _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
        _w(f, 3, f'<hkparam name="variableBindingSet">{_get_conn(conn_map, node_id, 0)}</hkparam>')
        _w(f, 3, '<!-- cachedBindables SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- areBindablesCached SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- hasEnableChanged SERIALIZE_IGNORED -->')
        _w(f, 3, '<hkparam name="listeners" numelements="0">')
        _w(f, 3, '</hkparam>')
        _w(f, 3, f'<hkparam name="enterNotifyEvents">{_get_conn(conn_map, node_id, 1)}</hkparam>')
        _w(f, 3, f'<hkparam name="exitNotifyEvents">{_get_conn(conn_map, node_id, 2)}</hkparam>')
        _w(f, 3, f'<hkparam name="transitions">{_get_conn(conn_map, node_id, 3)}</hkparam>')
        _w(f, 3, f'<hkparam name="generator">{_get_conn(conn_map, node_id, 4)}</hkparam>')
        _w(f, 3, f'<hkparam name="name">{nd.get("nodeName", "")}</hkparam>')
        _w(f, 3, f'<hkparam name="stateId">{nd.get("stateId", 0)}</hkparam>')
        _w(f, 3, f'<hkparam name="probability">{nd.get("probability", "1.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="enable">{str(nd.get("enable", True)).lower()}</hkparam>')
        _w(f, 3, '<!-- hasEventlessTransitions SERIALIZE_IGNORED -->')
        _w(f, 2, '</hkobject>')

    elif type_id == 7:  # hkbStateMachineTransitionInfoArray
        _w(f, 2, f'<hkobject class="{xml_class}" name="#{node_id}" signature="{sig}">')
        _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
        trans_array = nd.get("transitionArray", [])
        _w(f, 3, f'<hkparam name="transitions" numelements="{len(trans_array)}">')
        for transition in trans_array:
            _w(f, 4, '<hkobject>')
            # triggerInterval
            _w(f, 5, '<hkparam name="triggerInterval">')
            _w(f, 6, '<hkobject class="hkbStateMachineTimeInterval" name="triggerInterval" signature="0x60a881e5">')
            _w(f, 7, '<hkparam name="enterEventId">-1</hkparam>')
            _w(f, 7, '<hkparam name="exitEventId">-1</hkparam>')
            _w(f, 7, '<hkparam name="enterTime">0.0</hkparam>')
            _w(f, 7, '<hkparam name="exitTime">0.0</hkparam>')
            _w(f, 6, '</hkobject>')
            _w(f, 5, '</hkparam>')
            # initiateInterval
            _w(f, 5, '<hkparam name="initiateInterval">')
            _w(f, 6, '<hkobject class="hkbStateMachineTimeInterval" name="initiateInterval" signature="0x60a881e5">')
            _w(f, 7, '<hkparam name="enterEventId">-1</hkparam>')
            _w(f, 7, '<hkparam name="exitEventId">-1</hkparam>')
            _w(f, 7, '<hkparam name="enterTime">0.0</hkparam>')
            _w(f, 7, '<hkparam name="exitTime">0.0</hkparam>')
            _w(f, 6, '</hkobject>')
            _w(f, 5, '</hkparam>')
            # transition reference
            trans_idx = transition.get("transition", 0)
            _w(f, 5, f'<hkparam name="transition">#{export_idx + trans_idx + 1}</hkparam>')
            _w(f, 5, '<hkparam name="condition">null</hkparam>')
            _w(f, 5, f'<hkparam name="eventId">{transition.get("eventId", -1)}</hkparam>')
            _w(f, 5, f'<hkparam name="toStateId">{transition.get("toStateId", 0)}</hkparam>')
            _w(f, 5, f'<hkparam name="fromNestedStateId">{transition.get("fromNestedStateId", 0)}</hkparam>')
            _w(f, 5, f'<hkparam name="toNestedStateId">{transition.get("toNestedStateId", 0)}</hkparam>')
            _w(f, 5, f'<hkparam name="priority">{transition.get("priority", 0)}</hkparam>')
            flags = transition.get("flags", [False] * 15)
            _w(f, 5, f'<hkparam name="flags">{_flags_to_string(flags)}</hkparam>')
            _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
        _w(f, 3, '<!-- hasEventlessTransitions SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- hasTimeBoundedTransitions SERIALIZE_IGNORED -->')
        _w(f, 2, '</hkobject>')

    elif type_id == 8:  # hkbStateMachineEventPropertyArray
        _w(f, 2, f'<hkobject class="{xml_class}" name="#{node_id}" signature="{sig}">')
        _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
        events_array = nd.get("eventsArray", [])
        _w(f, 3, f'<hkparam name="events" numelements="{len(events_array)}">')
        for event in events_array:
            _w(f, 4, '<hkobject>')
            _w(f, 5, f'<hkparam name="id">{event.get("eventID", -1)}</hkparam>')
            payload_id = event.get("payloadID", -1)
            if payload_id is not None and payload_id != -1:
                _w(f, 5, f'<hkparam name="payload">#{export_idx + payload_id + trans_len}</hkparam>')
            else:
                _w(f, 5, '<hkparam name="payload">null</hkparam>')
            _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
        _w(f, 2, '</hkobject>')

    elif type_id == 17:  # hkbVariableBindingSet
        _w(f, 2, f'<hkobject class="{xml_class}" name="#{node_id}" signature="{sig}">')
        _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
        bindings = nd.get("bindingArray", [])
        _w(f, 3, f'<hkparam name="bindings" numelements="{len(bindings)}">')
        for binding in bindings:
            _w(f, 4, '<hkobject>')
            _w(f, 5, f'<hkparam name="memberPath">{binding.get("memberPath", "")}</hkparam>')
            _w(f, 5, '<!-- memberClass SERIALIZE_IGNORED -->')
            _w(f, 5, '<!-- offsetInObjectPlusOne SERIALIZE_IGNORED -->')
            _w(f, 5, '<!-- offsetInArrayPlusOne SERIALIZE_IGNORED -->')
            _w(f, 5, '<!-- rootVariableIndex SERIALIZE_IGNORED -->')
            _w(f, 5, f'<hkparam name="variableIndex">{binding.get("variableIndex", 0)}</hkparam>')
            _w(f, 5, '<hkparam name="bitIndex">255</hkparam>')
            _w(f, 5, f'<hkparam name="bindingType">{binding.get("bindingType", "BINDING_TYPE_VARIABLE")}</hkparam>')
            _w(f, 5, '<!-- memberType SERIALIZE_IGNORED -->')
            _w(f, 5, '<!-- variableType SERIALIZE_IGNORED -->')
            _w(f, 5, '<!-- flags SERIALIZE_IGNORED -->')
            _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
        _w(f, 3, f'<hkparam name="indexOfBindingToEnable">{nd.get("indexOfBindingToEnable", -1)}</hkparam>')
        _w(f, 3, '<!-- hasOutputBinding SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- initializedOffsets SERIALIZE_IGNORED -->')
        _w(f, 2, '</hkobject>')

    elif type_id == 23:  # hkbBlenderGenerator
        _write_standard_header(is_generator=True)
        _w(f, 3, f'<hkparam name="referencePoseWeightThreshold">{nd.get("referencePoseWeightThreshold", "0.0")}</hkparam>')
        _w(f, 3, f'<hkparam name="blendParameter">{nd.get("blendParameter", "0.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="minCyclicBlendParameter">{nd.get("minCyclicBlendParameter", "0.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="maxCyclicBlendParameter">{nd.get("maxCyclicBlendParameter", "1.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="indexOfSyncMasterChild">{nd.get("indexOfSyncMasterChild", 65535)}</hkparam>')
        _w(f, 3, f'<hkparam name="flags">{nd.get("flagsIndex", 0)}</hkparam>')
        _w(f, 3, f'<hkparam name="subtractLastChild">{str(nd.get("subtractLastChild", False)).lower()}</hkparam>')
        children = _get_conn(conn_map, node_id, 1, single=False)
        _w(f, 3, f'<hkparam name="children" numelements="{len(children)}">')
        for child in children:
            _w(f, 3, f'#{child}')
        _w(f, 3, '</hkparam>')
        _w(f, 3, '<!-- childrenInternalStates SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- sortedChildren SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- endIntervalWeight SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- numActiveChildren SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- beginIntervalIndex SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- endIntervalIndex SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- initSync SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- doSubtractiveBlend SERIALIZE_IGNORED -->')
        _w(f, 2, '</hkobject>')

    elif type_id == 24:  # hkbBlenderGeneratorChild
        _w(f, 2, f'<hkobject class="{xml_class}" name="#{node_id}" signature="{sig}">')
        _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
        _w(f, 3, f'<hkparam name="variableBindingSet">{_get_conn(conn_map, node_id, 0)}</hkparam>')
        _w(f, 3, '<!-- cachedBindables SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- areBindablesCached SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- hasEnableChanged SERIALIZE_IGNORED -->')
        _w(f, 3, f'<hkparam name="generator">{_get_conn(conn_map, node_id, 1)}</hkparam>')
        _w(f, 3, f'<hkparam name="boneWeights">{_get_conn(conn_map, node_id, 2)}</hkparam>')
        _w(f, 3, f'<hkparam name="weight">{nd.get("weight", "1.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="worldFromModelWeight">{nd.get("worldFromModelWeight", "1.000000")}</hkparam>')
        _w(f, 2, '</hkobject>')

    elif type_id == 25:  # hkbClipGenerator
        _write_standard_header(is_generator=True)
        _w(f, 3, f'<hkparam name="animationName">{nd.get("animationName", "")}</hkparam>')
        _w(f, 3, f'<hkparam name="triggers">{_get_conn(conn_map, node_id, 1)}</hkparam>')
        _w(f, 3, f'<hkparam name="cropStartAmountLocalTime">{nd.get("cropStartAmountLocalTime", "0.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="cropEndAmountLocalTime">{nd.get("cropEndAmountLocalTime", "0.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="startTime">{nd.get("startTime", "0.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="playbackSpeed">{nd.get("playbackSpeed", "1.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="enforcedDuration">{nd.get("enforcedDuration", "0.000000")}</hkparam>')
        _w(f, 3, f'<hkparam name="userControlledTimeFraction">{nd.get("userControlledTimeFraction", "0.000000")}</hkparam>')
        mode = nd.get("mode", 0)
        mode_str = {0: "MODE_SINGLE_PLAY", 1: "MODE_LOOPING", 2: "MODE_USER_CONTROLLED"}.get(mode, "MODE_SINGLE_PLAY")
        _w(f, 3, f'<hkparam name="mode">{mode_str}</hkparam>')
        _w(f, 3, f'<hkparam name="flags">{nd.get("flagsIndex", 0)}</hkparam>')
        _w(f, 2, '</hkobject>')

    elif type_id == 30:  # hkbTimerModifier
        _write_modifier_header()
        _w(f, 3, f'<hkparam name="alarmTimeSeconds">{nd.get("alarmTimeSeconds", "0.000000")}</hkparam>')
        _w(f, 3, '<hkparam name="alarmEvent">')
        _w(f, 4, '<hkobject class="hkbEventProperty" name="alarmEvent" signature="0xdb38a15">')
        event_id = nd.get("eventId", -1)
        _w(f, 5, f'<hkparam name="id">{event_id}</hkparam>')
        if event_id != -1:
            payload_idx = nd.get("payload", -1)
            _w(f, 5, f'<hkparam name="payload">#{export_idx + payload_idx + trans_len}</hkparam>')
        else:
            _w(f, 5, '<hkparam name="payload">null</hkparam>')
        _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
        _w(f, 3, '<!-- secondsElapsed SERIALIZE_IGNORED -->')
        _w(f, 2, '</hkobject>')

    elif type_id == 38:  # BSRagdollContactListenerModifier
        _write_modifier_header()
        _w(f, 3, '<hkparam name="contactEvent">')
        _w(f, 4, '<hkobject class="hkbEventProperty" name="contactEvent" signature="0xdb38a15">')
        event_id = nd.get("eventId", -1)
        _w(f, 5, f'<hkparam name="id">{event_id}</hkparam>')
        if event_id != -1:
            payload_idx = nd.get("payload", -1)
            if payload_idx != -1:
                _w(f, 5, f'<hkparam name="payload">#{export_idx + payload_idx + trans_len}</hkparam>')
            else:
                _w(f, 5, '<hkparam name="payload">null</hkparam>')
        else:
            _w(f, 5, '<hkparam name="payload">null</hkparam>')
        _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
        _w(f, 3, f'<hkparam name="bones">{_get_conn(conn_map, node_id, 1)}</hkparam>')
        _w(f, 3, '<!-- throwEvent SERIALIZE_IGNORED -->')
        _w(f, 2, '</hkobject>')

    elif type_id == 39:  # BSCyclicBlendTransitionGenerator
        _write_standard_header(is_generator=True)
        _w(f, 3, f'<hkparam name="pBlenderGenerator">{_get_conn(conn_map, node_id, 1)}</hkparam>')
        _write_event_property(f, nd, "EventToFreezeBlendValue",
                              "EventToFreezeBlendValueID", "EventToFreezeBlendValuePayload",
                              export_idx, trans_len)
        _write_event_property(f, nd, "EventToCrossBlend",
                              "EventToCrossBlendID", "EventToCrossBlendPayload",
                              export_idx, trans_len)
        _write_event_property(f, nd, "TransitionOutEvent",
                              "TransitionOutEventID", "TransitionOutEventPayload",
                              export_idx, trans_len)
        _write_event_property(f, nd, "TransitionInEvent",
                              "TransitionInEventID", "TransitionInEventPayload",
                              export_idx, trans_len)
        _w(f, 3, '<hkparam name="fBlendParameter">0.0</hkparam>')
        _w(f, 3, f'<hkparam name="fTransitionDuration">{nd.get("fTransitionDuration", "0.000000")}</hkparam>')
        _w(f, 3, '<hkparam name="eBlendCurve">0</hkparam>')
        _w(f, 3, '<!-- pTransitionBlenderGeneratorsA SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- sortedChildren SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- pTempOutput SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- currentMode SERIALIZE_IGNORED -->')
        _w(f, 2, '</hkobject>')

    elif type_id == 43:  # BSAssignVariablesModifier
        _write_modifier_header()
        float_var = nd.get("floatVariable", ["0"] * 20)
        float_val = nd.get("floatValue", ["0"] * 20)
        int_var = nd.get("intVariable", [0] * 4)
        int_val = nd.get("intValue", [0] * 4)
        for i in range(20):
            _w(f, 3, f'<hkparam name="floatVariable{i + 1}">{float_var[i] if i < len(float_var) else "0"}</hkparam>')
            _w(f, 3, f'<hkparam name="floatValue{i + 1}">{float_val[i] if i < len(float_val) else "0"}</hkparam>')
        for i in range(4):
            _w(f, 3, f'<hkparam name="intVariable{i + 1}">{int_var[i] if i < len(int_var) else 0}</hkparam>')
            _w(f, 3, f'<hkparam name="intValue{i + 1}">{int_val[i] if i < len(int_val) else 0}</hkparam>')
        _w(f, 2, '</hkobject>')

    elif type_id == 45:  # BSTimerModifier
        _write_modifier_header()
        _w(f, 3, f'<hkparam name="alarmTimeSeconds">{nd.get("alarmTimeSeconds", "0.000000")}</hkparam>')
        _w(f, 3, '<hkparam name="alarmEvent">')
        _w(f, 4, '<hkobject class="hkbEventProperty" name="alarmEvent" signature="0xdb38a15">')
        event_id = nd.get("eventId", -1)
        _w(f, 5, f'<hkparam name="id">{event_id}</hkparam>')
        payload_idx = nd.get("payload", -1)
        if payload_idx != -1:
            _w(f, 5, f'<hkparam name="payload">#{export_idx + payload_idx + trans_len}</hkparam>')
        else:
            _w(f, 5, '<hkparam name="payload">null</hkparam>')
        _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
        _w(f, 3, f'<hkparam name="resetAlarm">{str(nd.get("resetAlarm", False)).lower()}</hkparam>')
        _w(f, 3, '<!-- secondsElapsed SERIALIZE_IGNORED -->')
        _w(f, 2, '</hkobject>')

    else:
        # Generic fallback for other node types
        _write_generic_node(f, nd, type_id, node_id, conn_map, xml_class, sig,
                            export_idx, trans_len, payload_len)


def _write_generic_node(f, nd, type_id, node_id, conn_map, xml_class, sig,
                         export_idx, trans_len, payload_len):
    """Write a generic node - handles most modifier/generator types."""
    from .node_types import NODE_TYPE_DEFINITIONS
    defn = NODE_TYPE_DEFINITIONS[type_id]
    props = defn.get("properties", {})
    out_ports = defn.get("output_ports", [])

    _w(f, 2, f'<hkobject class="{xml_class}" name="#{node_id}" signature="{sig}">')
    _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')

    # variableBindingSet (port 0 for most types)
    if out_ports and out_ports[0][0] == "variableBindingSet":
        vbs = _get_conn(conn_map, node_id, 0)
        _w(f, 3, f'<hkparam name="variableBindingSet">{vbs}</hkparam>')

    # Common headers
    has_common = any(p in props for p in ("userData", "nodeName"))
    if has_common:
        _w(f, 3, '<!-- cachedBindables SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- areBindablesCached SERIALIZE_IGNORED -->')
        _w(f, 3, '<!-- hasEnableChanged SERIALIZE_IGNORED -->')
        if "userData" in props:
            _w(f, 3, f'<hkparam name="userData">{nd.get("userData", 0)}</hkparam>')
        if "nodeName" in props:
            _w(f, 3, f'<hkparam name="name">{nd.get("nodeName", "")}</hkparam>')

    # Enable property
    if "enable" in props:
        _w(f, 3, f'<hkparam name="enable">{str(nd.get("enable", True)).lower()}</hkparam>')

    # Remaining scalar properties
    skip_props = {"userData", "nodeName", "enable", "nodeID", "nodeTypeID",
                  "nodeColorID", "nodePosition"}
    for prop_name, (prop_type, _default) in props.items():
        if prop_name in skip_props:
            continue
        if prop_type == "list":
            continue  # Lists need special handling
        val = nd.get(prop_name, _default)
        _w(f, 3, f'<hkparam name="{prop_name}">{val}</hkparam>')

    # Connection ports (skip port 0 which is VBS)
    for port_idx in range(1, len(out_ports)):
        port_name, multi = out_ports[port_idx]
        if multi:
            targets = _get_conn(conn_map, node_id, port_idx, single=False)
            _w(f, 3, f'<hkparam name="{port_name}" numelements="{len(targets)}">')
            for t in targets:
                _w(f, 3, f'#{t}')
            _w(f, 3, '</hkparam>')
        else:
            _w(f, 3, f'<hkparam name="{port_name}">{_get_conn(conn_map, node_id, port_idx)}</hkparam>')

    _w(f, 2, '</hkobject>')


def _write_blending_transition(f, trans, export_idx):
    """Write a hkbBlendingTransitionEffect object."""
    name = trans.get("transitionName", "")
    _w(f, 2, f'<hkobject class="hkbBlendingTransitionEffect" name="#{export_idx}" signature="0x14e54c5c">')
    _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
    _w(f, 3, '<hkparam name="variableBindingSet">null</hkparam>')
    _w(f, 3, '<!-- cachedBindables SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- areBindablesCached SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- hasEnableChanged SERIALIZE_IGNORED -->')
    _w(f, 3, '<hkparam name="userData">0</hkparam>')
    _w(f, 3, f'<hkparam name="name">{name}</hkparam>')
    _w(f, 3, '<!-- id SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- cloneState SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- type SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- nodeInfo SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- partitionInfo SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- syncInfo SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- pad1 SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- pad2 SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- pad3 SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- pad4 SERIALIZE_IGNORED -->')

    stm_map = {0: "SELF_TRANSITION_MODE_CONTINUE_IF_CYCLIC_BLEND_IF_ACYCLIC",
               1: "SELF_TRANSITION_MODE_CONTINUE",
               2: "SELF_TRANSITION_MODE_RESET",
               3: "SELF_TRANSITION_MODE_BLEND"}
    _w(f, 3, f'<hkparam name="selfTransitionMode">{stm_map.get(trans.get("transitionSelfTransitionMode", 0), stm_map[0])}</hkparam>')

    em_map = {0: "EVENT_MODE_DEFAULT", 1: "EVENT_MODE_PROCESS_ALL",
              2: "EVENT_MODE_IGNORE_FROM_GENERATOR", 3: "EVENT_MODE_IGNORE_TO_GENERATOR"}
    _w(f, 3, f'<hkparam name="eventMode">{em_map.get(trans.get("transitionEventMode", 0), em_map[0])}</hkparam>')
    _w(f, 3, '<!-- defaultEventMode SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- patchedBindingInfo SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- fromGenerator SERIALIZE_IGNORED -->')
    _w(f, 3, '<!-- toGenerator SERIALIZE_IGNORED -->')

    _w(f, 3, f'<hkparam name="duration">{trans.get("transitionDuration", "0.000000")}</hkparam>')
    _w(f, 3, f'<hkparam name="toGeneratorStartTimeFraction">{trans.get("transitionToGeneratorStartTimeFraction", "0.000000")}</hkparam>')

    flags_map = {0: "FLAG_NONE", 1: "FLAG_IGNORE_FROM_WORLD_FROM_MODEL",
                 2: "FLAG_SYNC", 3: "FLAG_IGNORE_TO_WORLD_FROM_MODEL",
                 4: "FLAG_IGNORE_TO_WORLD_FROM_MODEL_ROTATION"}
    _w(f, 3, f'<hkparam name="flags">{flags_map.get(trans.get("transitionFlags", 0), "FLAG_NONE")}</hkparam>')

    end_map = {0: "END_MODE_NONE", 1: "END_MODE_CAP_DURATION_AT_END_OF_FROM_GENERATOR"}
    _w(f, 3, f'<hkparam name="endMode">{end_map.get(trans.get("transitionEndMode", 0), "END_MODE_NONE")}</hkparam>')

    _w(f, 3, f'<hkparam name="blendCurve">{trans.get("transitionBlendCurve", 0)}</hkparam>')
    _w(f, 2, '</hkobject>')


def _write_string_event_payload(f, payload, export_idx):
    """Write a hkbStringEventPayload object."""
    name = payload.get("payloadName", payload.get("name", ""))
    _w(f, 2, f'<hkobject class="hkbStringEventPayload" name="#{export_idx}" signature="0xdf5fe694">')
    _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
    _w(f, 3, f'<hkparam name="data">{name}</hkparam>')
    _w(f, 2, '</hkobject>')


def _write_behavior_graph_data(f, variables, events, properties, export_idx):
    """Write hkbBehaviorGraphData metadata node."""
    _w(f, 2, f'<hkobject class="hkbBehaviorGraphData" name="#{export_idx}" signature="0x907a8222">')
    _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
    _w(f, 3, '<hkparam name="attributeDefaults" numelements="0"/>')

    type_names = {0: "VARIABLE_TYPE_BOOL", 1: "VARIABLE_TYPE_INT8",
                  2: "VARIABLE_TYPE_INT16", 3: "VARIABLE_TYPE_INT32",
                  4: "VARIABLE_TYPE_REAL", 5: "VARIABLE_TYPE_POINTER",
                  6: "VARIABLE_TYPE_VECTOR4", 7: "VARIABLE_TYPE_QUATERNION"}

    # variableInfos
    if not variables:
        _w(f, 3, '<hkparam name="variableInfos" numelements="0"/>')
    else:
        _w(f, 3, f'<hkparam name="variableInfos" numelements="{len(variables)}">')
        for var in variables:
            vtype = var.get("variableType", 0)
            _w(f, 4, '<hkobject>')
            _w(f, 5, '<hkparam name="role">')
            _w(f, 6, '<hkobject class="hkbRoleAttribute" name="role" signature="0xfecef669">')
            _w(f, 7, '<hkparam name="role">ROLE_DEFAULT</hkparam>')
            _w(f, 7, '<hkparam name="flags">FLAG_NONE</hkparam>')
            _w(f, 6, '</hkobject>')
            _w(f, 5, '</hkparam>')
            _w(f, 5, f'<hkparam name="type">{type_names.get(vtype, "VARIABLE_TYPE_BOOL")}</hkparam>')
            _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')

    # characterPropertyInfos
    if not properties:
        _w(f, 3, '<hkparam name="characterPropertyInfos" numelements="0"/>')
    else:
        _w(f, 3, f'<hkparam name="characterPropertyInfos" numelements="{len(properties)}">')
        for prop in properties:
            ptype = prop.get("propertiesType", 0)
            _w(f, 4, '<hkobject>')
            _w(f, 5, '<hkparam name="role">')
            _w(f, 6, '<hkobject class="hkbRoleAttribute" name="role" signature="0xfecef669">')
            _w(f, 7, '<hkparam name="role">ROLE_DEFAULT</hkparam>')
            _w(f, 7, '<hkparam name="flags">FLAG_NONE</hkparam>')
            _w(f, 6, '</hkobject>')
            _w(f, 5, '</hkparam>')
            _w(f, 5, f'<hkparam name="type">{type_names.get(ptype, "VARIABLE_TYPE_BOOL")}</hkparam>')
            _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')

    # eventInfos
    if not events:
        _w(f, 3, '<hkparam name="eventInfos" numelements="0"/>')
    else:
        _w(f, 3, f'<hkparam name="eventInfos" numelements="{len(events)}">')
        for event in events:
            flags = event.get("eventFlags", 0)
            flag_str = "FLAG_SYNC_POINT" if flags == 1 else "0"
            _w(f, 4, '<hkobject>')
            _w(f, 5, f'<hkparam name="flags">{flag_str}</hkparam>')
            _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')

    # variableBounds
    if not variables:
        _w(f, 3, '<hkparam name="variableBounds" numelements="0"/>')
    else:
        _w(f, 3, f'<hkparam name="variableBounds" numelements="{len(variables)}">')
        for var in variables:
            vtype = var.get("variableType", 0)
            min_val = var.get("variableMinValue", "0")
            max_val = var.get("variableMaxValue", "0")
            # REAL-type variables need u32 encoding for bounds
            if vtype == 4:
                try:
                    min_val = _float_to_u32(float(min_val))
                except (ValueError, TypeError):
                    min_val = 0
                try:
                    max_val = _float_to_u32(float(max_val))
                except (ValueError, TypeError):
                    max_val = 0
            _w(f, 4, '<hkobject>')
            _w(f, 5, '<hkparam name="min">')
            _w(f, 6, f'<hkobject class="hkbVariableValue" name="min" signature="0xb99bd6a">')
            _w(f, 7, f'<hkparam name="value">{min_val}</hkparam>')
            _w(f, 6, '</hkobject>')
            _w(f, 5, '</hkparam>')
            _w(f, 5, '<hkparam name="max">')
            _w(f, 6, f'<hkobject class="hkbVariableValue" name="max" signature="0xb99bd6a">')
            _w(f, 7, f'<hkparam name="value">{max_val}</hkparam>')
            _w(f, 6, '</hkobject>')
            _w(f, 5, '</hkparam>')
            _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')

    _w(f, 3, f'<hkparam name="variableInitialValues">#{export_idx + 1}</hkparam>')
    _w(f, 3, f'<hkparam name="stringData">#{export_idx + 2}</hkparam>')
    _w(f, 2, '</hkobject>')


def _write_variable_value_set(f, variables, export_idx):
    """Write hkbVariableValueSet metadata node."""
    _w(f, 2, f'<hkobject class="hkbVariableValueSet" name="#{export_idx}" signature="0xeb5f7e25">')
    _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
    pointer_count = 0
    if variables:
        _w(f, 3, f'<hkparam name="wordVariableValues" numelements="{len(variables)}">')
        for var in variables:
            vtype = var.get("variableType", 0)
            val = var.get("variableValue", "0")
            # REAL-type variables: re-encode float as u32
            if vtype == 4:
                try:
                    val = _float_to_u32(float(val))
                except (ValueError, TypeError):
                    val = 0
            if vtype == 5:
                pointer_count += 1
            _w(f, 4, '<hkobject>')
            _w(f, 5, f'<hkparam name="value">{val}</hkparam>')
            _w(f, 4, '</hkobject>')
        _w(f, 3, '</hkparam>')
    else:
        _w(f, 3, '<hkparam name="wordVariableValues" numelements="0"/>')

    # Quad values
    quad_parts = []
    for var in variables:
        if var.get("variableType", 0) in (6, 7):
            quad_parts.append(var.get("variableQuadValues", "0.0 0.0 0.0 0.0"))
    if quad_parts:
        _w(f, 3, f'<hkparam name="quadVariableValues" numelements="{len(quad_parts)}">')
        for qv in quad_parts:
            _w(f, 3, str(qv))
        _w(f, 3, '</hkparam>')
    else:
        _w(f, 3, '<hkparam name="quadVariableValues" numelements="0"/>')

    # Variant variable values (for pointer types)
    if pointer_count > 0:
        _w(f, 3, f'<hkparam name="variantVariableValues" numelements="{pointer_count}">')
        for _ in range(pointer_count):
            _w(f, 3, 'null')
        _w(f, 3, '</hkparam>')
    else:
        _w(f, 3, '<hkparam name="variantVariableValues" numelements="0"/>')
    _w(f, 2, '</hkobject>')


def _write_string_data(f, variables, events, properties, export_idx):
    """Write hkbBehaviorGraphStringData metadata node."""
    _w(f, 2, f'<hkobject class="hkbBehaviorGraphStringData" name="#{export_idx}" signature="0x1bd27f38">')
    _w(f, 3, '<!-- memSizeAndRefCount SERIALIZE_IGNORED -->')
    # Event names
    if events:
        _w(f, 3, f'<hkparam name="eventNames" numelements="{len(events)}">')
        for event in events:
            _w(f, 4, f'<hkcstring>{event.get("eventName", "")}</hkcstring>')
        _w(f, 3, '</hkparam>')
    else:
        _w(f, 3, '<hkparam name="eventNames" numelements="0"/>')
    # Attribute names
    _w(f, 3, '<hkparam name="attributeNames" numelements="0"/>')
    # Variable names
    if variables:
        _w(f, 3, f'<hkparam name="variableNames" numelements="{len(variables)}">')
        for var in variables:
            _w(f, 4, f'<hkcstring>{var.get("variableName", "")}</hkcstring>')
        _w(f, 3, '</hkparam>')
    else:
        _w(f, 3, '<hkparam name="variableNames" numelements="0"/>')
    # Character property names
    if properties:
        _w(f, 3, f'<hkparam name="characterPropertyNames" numelements="{len(properties)}">')
        for prop in properties:
            _w(f, 4, f'<hkcstring>{prop.get("propertiesName", "")}</hkcstring>')
        _w(f, 3, '</hkparam>')
    else:
        _w(f, 3, '<hkparam name="characterPropertyNames" numelements="0"/>')
    _w(f, 2, '</hkobject>')
