"""JSON serialization system for Behavior Graph nodes (compatible with Godot format)."""

import json
from typing import Any


def serialize_node(node_data: dict[str, Any]) -> dict[str, Any]:
    result = dict(node_data)

    # Convert any non-serializable types
    for key, value in result.items():
        if isinstance(value, (set, frozenset)):
            result[key] = list(value)
        elif hasattr(value, "tolist"):  # numpy arrays
            result[key] = value.tolist()

    return result


def serialize_graph(
    nodes: list[dict[str, Any]],
    connections: list[list[Any]],
    global_variables: list[dict[str, Any]] | None = None,
    global_events: list[dict[str, Any]] | None = None,
    global_transitions: list[dict[str, Any]] | None = None,
    global_payloads: list[dict[str, Any]] | None = None,
    global_properties: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    result = {
        "version": "1.0",
        "nodes": [serialize_node(n) for n in nodes],
        "connections": connections,
    }

    if global_variables is not None:
        result["global_variables"] = global_variables
    if global_events is not None:
        result["global_events"] = global_events
    if global_transitions is not None:
        result["global_transitions"] = global_transitions
    if global_payloads is not None:
        result["global_payloads"] = global_payloads
    if global_properties is not None:
        result["global_properties"] = global_properties

    return result


def save_graph_to_file(
    filepath: str,
    nodes: list[dict[str, Any]],
    connections: list[list[Any]],
    global_variables: list[dict[str, Any]] | None = None,
    global_events: list[dict[str, Any]] | None = None,
    global_transitions: list[dict[str, Any]] | None = None,
    global_payloads: list[dict[str, Any]] | None = None,
    global_properties: list[dict[str, Any]] | None = None,
    global_state: dict[str, Any] | None = None,
) -> None:
    """Save graph state to JSON file.

    global_state, if given, overrides the individual global_* arguments.
    """
    if global_state is not None:
        graph_data = {
            "version": "1.0",
            "nodes": [serialize_node(n) for n in nodes],
            "connections": connections,
            "global_state": global_state,
        }
    else:
        graph_data = serialize_graph(
            nodes,
            connections,
            global_variables,
            global_events,
            global_transitions,
            global_payloads,
            global_properties,
        )

    with open(filepath, "w", encoding="utf-8") as f:
        json.dump(graph_data, f, indent=2)


def load_graph_from_file(filepath: str) -> dict[str, Any]:
    with open(filepath, "r", encoding="utf-8") as f:
        return json.load(f)


def deserialize_graph(graph_data: dict[str, Any]) -> dict[str, Any]:
    """Deserialize graph data from loaded dictionary.

    Returns a dict with keys: nodes, connections, global_variables,
    global_events, global_transitions, global_payloads, global_properties, version.
    """
    nodes = graph_data.get("nodes", [])
    connections = graph_data.get("connections", [])

    return {
        "nodes": nodes,
        "connections": connections,
        "global_variables": graph_data.get("global_variables", []),
        "global_events": graph_data.get("global_events", []),
        "global_transitions": graph_data.get("global_transitions", []),
        "global_payloads": graph_data.get("global_payloads", []),
        "global_properties": graph_data.get("global_properties", []),
        "version": graph_data.get("version", "unknown"),
    }
