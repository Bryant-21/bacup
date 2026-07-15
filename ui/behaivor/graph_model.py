"""Single-graph data model for the behavior editor UI.

This is the UI's source of truth — all mutations go through here.
No multi-graph support, no graph_id.
"""

import copy
import json
import logging
import os
import shutil
import tempfile
from typing import Any

from .global_state import GlobalState
from .node_types import NODE_TYPE_DEFINITIONS
from .json_serialization import save_graph_to_file, load_graph_from_file
from .xml_export import export_xml_file

log = logging.getLogger(__name__)


class GraphModel:
    """Single behavior graph data model."""

    def __init__(self):
        self.nodes: dict[int, dict] = {}
        self.connections: list[list] = []  # [[port_idx, from_id, to_id], ...]
        self.global_state = GlobalState()
        self._next_node_id = 90

    def clear(self):
        self.nodes.clear()
        self.connections.clear()
        self.global_state.clear()
        self._next_node_id = 90

    @property
    def next_node_id(self) -> int:
        return self._next_node_id

    # --- Node operations ---

    def create_node(self, type_id: int, name: str = "",
                    properties: dict | None = None) -> dict:
        """Create a node with defaults from NODE_TYPE_DEFINITIONS."""
        if type_id not in NODE_TYPE_DEFINITIONS:
            raise ValueError(f"Unknown node type ID: {type_id}")

        defn = NODE_TYPE_DEFINITIONS[type_id]
        if defn.get("metadata_only"):
            raise ValueError(f"Cannot create metadata-only node: {defn['class_name']}")

        node_id = self._next_node_id
        self._next_node_id += 1

        node_data = {
            "nodeID": node_id,
            "nodeTypeID": type_id,
            "nodeColorID": 0,
            "nodeName": name or defn["class_name"],
        }

        for prop_name, (prop_type, default) in defn["properties"].items():
            if prop_name == "nodeName":
                continue
            node_data[prop_name] = copy.deepcopy(default) if isinstance(default, (list, dict)) else default

        if properties:
            for k, v in properties.items():
                if k in defn["properties"] or k in ("nodeName", "nodeColorID"):
                    node_data[k] = v

        self.nodes[node_id] = node_data
        return node_data

    def delete_node(self, node_id: int) -> bool:
        if node_id not in self.nodes:
            return False
        del self.nodes[node_id]
        self.connections = [
            c for c in self.connections
            if c[1] != node_id and c[2] != node_id
        ]
        return True

    def get_node(self, node_id: int) -> dict | None:
        return self.nodes.get(node_id)

    def set_node_property(self, node_id: int, prop_name: str, value: Any):
        node = self.nodes.get(node_id)
        if node is None:
            return
        node[prop_name] = value

    # --- Connection operations ---

    def connect(self, from_id: int, port_idx: int, to_id: int) -> bool:
        """Connect from_node's output port to to_node's input."""
        if from_id not in self.nodes or to_id not in self.nodes:
            return False

        conn = [port_idx, from_id, to_id]
        if conn in self.connections:
            return False

        from_node = self.nodes[from_id]
        defn = NODE_TYPE_DEFINITIONS[from_node["nodeTypeID"]]
        if port_idx >= len(defn["output_ports"]):
            return False

        # If port doesn't allow multi-connect, remove existing
        _, multi = defn["output_ports"][port_idx]
        if not multi:
            self.connections = [
                c for c in self.connections
                if not (c[0] == port_idx and c[1] == from_id)
            ]

        self.connections.append(conn)
        return True

    def disconnect(self, from_id: int, port_idx: int, to_id: int) -> bool:
        conn = [port_idx, from_id, to_id]
        try:
            self.connections.remove(conn)
            return True
        except ValueError:
            return False

    # --- Initial nodes ---

    def create_initial_nodes(self):
        """Create the default Root -> BehaviorGraph -> StateMachine chain."""
        self.clear()

        root = self.create_node(0)  # hkRootLevelContainer
        bg = self.create_node(1)    # hkbBehaviorGraph
        sm = self.create_node(5)    # hkbStateMachine

        # Root -> BG (port 0: variant)
        self.connect(root["nodeID"], 0, bg["nodeID"])
        # BG -> SM (port 0: rootGenerator)
        self.connect(bg["nodeID"], 0, sm["nodeID"])

    # --- Serialization ---

    def save_json(self, filepath: str):
        nodes_list = sorted(self.nodes.values(), key=lambda n: n.get("nodeID", 0))
        save_graph_to_file(
            filepath, nodes_list, self.connections,
            global_state=self.global_state.to_dict(),
        )

    def load_json(self, filepath: str):
        raw = load_graph_from_file(filepath)
        self.clear()

        if "global_state" in raw:
            self.global_state = GlobalState.from_dict(raw["global_state"])

        for nd in raw.get("nodes", []):
            nid = nd.get("nodeID", self._next_node_id)
            self.nodes[nid] = nd
            if nid >= self._next_node_id:
                self._next_node_id = nid + 1

        self.connections = [list(c) for c in raw.get("connections", [])]

    def import_xml(self, filepath: str) -> dict:
        """Import from XML file. Returns stats dict."""
        from creation_lib._native.havok_native import havok_behavior_graph_to_ui_json
        self.clear()

        with open(filepath, encoding="utf-8", errors="replace") as fh:
            xml_string = fh.read()

        raw = json.loads(havok_behavior_graph_to_ui_json(xml_string))

        # nodes: native returns {str_id: dict} → convert keys to int
        self.nodes = {int(k): v for k, v in raw["nodes"].items()}

        self.connections = [list(c) for c in raw["connections"]]

        # Populate global_state from the native parse result
        gs = raw.get("global_state", {})
        self.global_state.events = list(gs.get("events", []))
        self.global_state.variables = list(gs.get("variables", []))
        self.global_state.transitions = list(gs.get("transitions", []))
        self.global_state.payloads = list(gs.get("payloads", []))
        self.global_state.properties = list(gs.get("properties", []))

        self._recalc_next_id()
        return {
            "nodes": self.nodes,
            "connections": self.connections,
            "unhandled": raw.get("unhandled", []),
        }

    def export_xml(self, filepath: str):
        nodes_list = sorted(self.nodes.values(), key=lambda n: n.get("nodeID", 0))
        export_xml_file(filepath, nodes_list, self.connections, self.global_state)

    def import_hkx(self, filepath: str) -> dict:
        """Import from HKX file. Returns stats dict."""
        from creation_lib._native.havok_native import hkx_to_xml
        import io
        xml = hkx_to_xml(open(filepath, "rb").read())
        fd, tmp_path = tempfile.mkstemp(suffix=".xml", prefix="hkximport_")
        try:
            with io.open(fd, "w", encoding="utf-8") as f:
                f.write(xml)
            return self.import_xml(tmp_path)
        finally:
            try:
                os.unlink(tmp_path)
            except OSError:
                pass

    def export_hkx(self, filepath: str):
        """Export to HKX file via XML intermediate."""
        from creation_lib._native.havok_native import xml_to_hkx
        tmp_dir = tempfile.mkdtemp(prefix="hkxexport_")
        xml_path = os.path.join(tmp_dir, "export_temp.xml")
        try:
            self.export_xml(xml_path)
            hkx_bytes = xml_to_hkx(open(xml_path, "r", encoding="utf-8").read())
            open(filepath, "wb").write(hkx_bytes)
        finally:
            shutil.rmtree(tmp_dir, ignore_errors=True)

    def _recalc_next_id(self):
        if self.nodes:
            self._next_node_id = max(self.nodes.keys()) + 1
        else:
            self._next_node_id = 90

    # --- Validation ---

    def validate(self) -> list[str]:
        """Run validation checks. Returns list of warning strings."""
        warnings = []

        defined_events = set()
        for e in self.global_state.events:
            name = e.get("eventName", e.get("name", "")) if isinstance(e, dict) else e.name
            defined_events.add(name)

        # Casing inconsistencies among events
        lower_map = {}
        for name in defined_events:
            key = name.lower()
            if key in lower_map and lower_map[key] != name:
                warnings.append(
                    f"Inconsistent event casing: '{lower_map[key]}' vs '{name}'")
            lower_map[key] = name

        # Check for unnamed states
        for nd in self.nodes.values():
            type_id = nd.get("nodeTypeID")
            nid = nd.get("nodeID", "?")
            defn = NODE_TYPE_DEFINITIONS.get(type_id, {})
            class_name = defn.get("class_name", "Unknown")
            if class_name == "hkbStateMachineStateInfo":
                if not nd.get("nodeName", ""):
                    warnings.append(f"State #{nid} has no name (unnamed state)")

        warnings.extend(self._validate_state_machine_transitions())

        return warnings

    def _validate_state_machine_transitions(self) -> list[str]:
        # `toStateId` on a transition must resolve to a stateId of a stateInfo
        # owned by the same hkbStateMachine. Cross-SM targets crash F4 with a
        # wild-pointer access in hkbStateMachine::OnNotifyEvent (the engine
        # looks up the id in the SM's stateIdToIndexMap, gets garbage, then
        # dereferences it as a StateInfo*).
        out: list[str] = []
        conn_map: dict[tuple[int, int], list[int]] = {}
        for port_idx, from_id, to_id in self.connections:
            conn_map.setdefault((from_id, port_idx), []).append(to_id)

        for sm in self.nodes.values():
            if sm.get("nodeTypeID") != 5:
                continue
            sm_id = sm.get("nodeID")
            sm_label = sm.get("nodeName") or f"#{sm_id}"

            state_info_ids = conn_map.get((sm_id, 1), [])
            id_to_name: dict[int, str] = {}
            for si_id in state_info_ids:
                si = self.nodes.get(si_id)
                if not si or si.get("nodeTypeID") != 6:
                    continue
                id_to_name[int(si.get("stateId", 0))] = si.get("nodeName") or f"#{si_id}"
            valid_ids = set(id_to_name.keys())

            start_id = int(sm.get("startStateId", 0))
            if valid_ids and start_id not in valid_ids:
                out.append(
                    f"StateMachine '{sm_label}': startStateId {start_id} "
                    f"not in {sorted(valid_ids)}"
                )

            for si_id in state_info_ids:
                si = self.nodes.get(si_id)
                if not si or si.get("nodeTypeID") != 6:
                    continue
                from_label = si.get("nodeName") or f"#{si_id}"
                from_state_id = int(si.get("stateId", 0))
                array_ids = conn_map.get((si_id, 3), [])
                for arr_id in array_ids:
                    arr = self.nodes.get(arr_id)
                    if not arr or arr.get("nodeTypeID") != 7:
                        continue
                    for t_idx, t in enumerate(arr.get("transitionArray", [])):
                        target = int(t.get("toStateId", 0))
                        if target in valid_ids:
                            continue
                        ev = t.get("eventId", -1)
                        out.append(
                            f"SM '{sm_label}': transition[{t_idx}] from "
                            f"'{from_label}' (stateId {from_state_id}) on "
                            f"eventId {ev} -> toStateId {target} not in "
                            f"{sorted(valid_ids)}"
                        )

        return out
