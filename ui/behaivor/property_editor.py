"""Property editor panel for the Behavior Graph Editor.

Renders an imgui panel showing and editing properties for the currently
selected node. Handles all property types including complex list sub-editors
for bindings, transitions, events, expressions, triggers, ranges, bone
indices/weights, and BSAssignVariablesModifier arrays.
"""

from imgui_bundle import imgui

from .node_types import NODE_TYPE_DEFINITIONS

# ---------------------------------------------------------------------------
# Color themes (must match canvas)
# ---------------------------------------------------------------------------

COLOR_THEMES = [
    (80, 80, 80), (100, 140, 200), (60, 80, 140), (180, 80, 80),
    (140, 80, 160), (100, 60, 100), (60, 160, 100), (80, 140, 80),
    (140, 100, 60), (200, 140, 60), (200, 200, 80), (80, 160, 180),
    (200, 100, 120), (160, 120, 180), (100, 180, 160), (180, 160, 100),
    (120, 100, 80), (60, 120, 100),
]

COLOR_THEME_NAMES = [
    "Default", "Blue", "Deep Blue", "Red", "Purple", "Eggplant",
    "Emerald", "Green", "Brown", "Orange", "Yellow", "Cerulean",
    "Rose", "Lavender", "Teal", "Sand", "Mocha", "Forest",
]

# ---------------------------------------------------------------------------
# Transition flags (hkbStateMachineTransitionInfoArray)
# ---------------------------------------------------------------------------

TRANSITION_FLAG_NAMES = [
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

# ---------------------------------------------------------------------------
# Event range modes (hkbEventRangeDataArray)
# ---------------------------------------------------------------------------

EVENT_RANGE_MODE_NAMES = [
    "EVENT_MODE_SEND_ONCE",
    "EVENT_MODE_SEND_ON_ENTER_RANGE",
    "EVENT_MODE_SEND_ON_EXIT_RANGE",
]

# ---------------------------------------------------------------------------
# Binding types (hkbVariableBindingSet)
# ---------------------------------------------------------------------------

BINDING_TYPE_NAMES = [
    "BINDING_TYPE_VARIABLE",
    "BINDING_TYPE_CHARACTER_PROPERTY",
]


# ---------------------------------------------------------------------------
# Helpers for accessing event/variable data from mixed dict/dataclass lists
# ---------------------------------------------------------------------------

def _event_name(event) -> str:
    """Extract event name from dict or GlobalEvent dataclass."""
    if isinstance(event, dict):
        return event.get("eventName", event.get("name", ""))
    return getattr(event, "name", "")


def _event_id(event) -> int:
    """Extract event ID from dict or GlobalEvent dataclass."""
    if isinstance(event, dict):
        return event.get("eventID", event.get("event_id", -1))
    return getattr(event, "event_id", -1)


def _variable_name(var) -> str:
    """Extract variable name from dict or GlobalVariable dataclass."""
    if isinstance(var, dict):
        return var.get("name", "")
    return getattr(var, "name", "")


def _variable_index(var) -> int:
    """Extract variable index from dict or GlobalVariable dataclass."""
    if isinstance(var, dict):
        return var.get("index", 0)
    return getattr(var, "index", 0)


def _payload_name(payload) -> str:
    """Extract payload name from dict or GlobalPayload dataclass."""
    if isinstance(payload, dict):
        return payload.get("name", "")
    return getattr(payload, "name", "")


# ---------------------------------------------------------------------------
# PropertyEditorPanel
# ---------------------------------------------------------------------------

class PropertyEditorPanel:
    """Imgui panel for editing properties of the selected behavior graph node."""

    def __init__(self):
        self._node_id: int | None = None
        self._global_state = None  # set each frame from model
        self.window_name: str = "Properties"
        self._visible: bool = True

    # -- public interface ---------------------------------------------------

    def render(self, model):
        """Render inside a docked imgui window.

        Args:
            model: GraphModel instance with .nodes dict and .global_state.
        """
        if not self._visible:
            return
        visible, _ = imgui.begin(self.window_name)
        if not visible:
            imgui.end()
            return

        self._global_state = model.global_state

        node_id = self._node_id
        if node_id is None or node_id not in model.nodes:
            imgui.text("No node selected")
            imgui.end()
            return

        node = model.nodes[node_id]
        type_id = node.get("nodeTypeID", -1)
        defn = NODE_TYPE_DEFINITIONS.get(type_id, {})

        # Header
        class_name = defn.get("class_name", "Unknown")
        imgui.text(f"{class_name} #{node_id}")
        imgui.separator()

        # Editable name
        changed, new_name = imgui.input_text("Name##prop_name", node.get("nodeName", ""))
        if changed:
            node["nodeName"] = new_name

        # Color theme picker
        self._render_color_picker(node)

        imgui.separator()

        # Properties from definition
        for prop_name, (prop_type, default) in defn.get("properties", {}).items():
            if prop_name in ("nodeName",):
                continue
            self._render_property(node, prop_name, prop_type, default, type_id)

        imgui.end()

    @property
    def selected_node_id(self) -> int | None:
        return self._node_id

    @selected_node_id.setter
    def selected_node_id(self, value: int | None):
        self._node_id = value

    # -- color picker -------------------------------------------------------

    def _render_color_picker(self, node: dict):
        """Render 18 color swatch buttons in a 9x2 grid."""
        current = node.get("nodeColorID", 0)
        imgui.text("Color:")
        imgui.same_line()
        for i, (r, g, b) in enumerate(COLOR_THEMES):
            if i > 0 and i % 9 != 0:
                imgui.same_line()
            imgui.push_style_color(
                imgui.Col_.button,
                imgui.ImVec4(r / 255.0, g / 255.0, b / 255.0, 1.0),
            )
            imgui.push_style_color(
                imgui.Col_.button_hovered,
                imgui.ImVec4(
                    min(r / 255.0 + 0.15, 1.0),
                    min(g / 255.0 + 0.15, 1.0),
                    min(b / 255.0 + 0.15, 1.0),
                    1.0,
                ),
            )
            # White border for selected, transparent for others
            if i == current:
                imgui.push_style_color(
                    imgui.Col_.border, imgui.ImVec4(1, 1, 1, 1)
                )
                imgui.push_style_var(imgui.StyleVar_.frame_border_size, 2.0)

            if imgui.button(f"##color_{i}", imgui.ImVec2(18, 18)):
                node["nodeColorID"] = i

            if i == current:
                imgui.pop_style_var()
                imgui.pop_style_color()  # border

            imgui.pop_style_color(2)  # button, button_hovered

            if imgui.is_item_hovered():
                imgui.set_tooltip(COLOR_THEME_NAMES[i])

    # -- property dispatch --------------------------------------------------

    def _render_property(self, node: dict, prop_name: str, prop_type: str,
                         default, type_id: int):
        """Dispatch to the correct widget based on property type."""
        if prop_type == "bool":
            self._render_bool(node, prop_name, default)
        elif prop_type == "int":
            self._render_int(node, prop_name, default)
        elif prop_type == "str":
            self._render_str(node, prop_name, default)
        elif prop_type == "list":
            self._render_list_property(node, prop_name, default, type_id)

    # -- scalar widgets -----------------------------------------------------

    def _render_bool(self, node: dict, prop_name: str, default):
        val = node.get(prop_name, default)
        changed, new_val = imgui.checkbox(f"{prop_name}##prop", bool(val))
        if changed:
            node[prop_name] = new_val

    def _render_int(self, node: dict, prop_name: str, default):
        val = node.get(prop_name, default)
        changed, new_val = imgui.input_int(f"{prop_name}##prop", int(val))
        if changed:
            node[prop_name] = new_val

        # Show event/variable name hints for known ID fields
        prop_lower = prop_name.lower()
        if self._global_state is not None:
            if "eventid" in prop_lower or "event_id" in prop_lower:
                self._show_event_hint(int(val))
                self._render_event_combo(node, prop_name)
            elif "payload" in prop_lower:
                self._show_payload_hint(int(val))
                self._render_payload_combo(node, prop_name)
            elif "variableindex" in prop_lower or "syncvariableindex" in prop_lower:
                self._show_variable_hint(int(val))
                self._render_variable_combo(node, prop_name)

    def _render_str(self, node: dict, prop_name: str, default):
        val = str(node.get(prop_name, default))
        changed, new_val = imgui.input_text(f"{prop_name}##prop", val)
        if changed:
            node[prop_name] = new_val

    # -- event / variable hints ---------------------------------------------

    def _show_event_hint(self, event_id: int):
        """Show the event name as a hint next to the event ID field."""
        if self._global_state is None or event_id < 0:
            return
        for evt in self._global_state.events:
            if _event_id(evt) == event_id:
                name = _event_name(evt)
                if name:
                    imgui.same_line()
                    imgui.text_colored(imgui.ImVec4(0.6, 0.8, 1.0, 1.0), f"({name})")
                return

    def _show_payload_hint(self, payload_id: int):
        """Show payload name hint."""
        if self._global_state is None or payload_id < 0:
            return
        payloads = self._global_state.payloads
        if 0 <= payload_id < len(payloads):
            name = _payload_name(payloads[payload_id])
            if name:
                imgui.same_line()
                imgui.text_colored(imgui.ImVec4(0.8, 0.7, 1.0, 1.0), f"({name})")

    def _show_variable_hint(self, var_index: int):
        """Show variable name hint."""
        if self._global_state is None or var_index < 0:
            return
        for var in self._global_state.variables:
            if _variable_index(var) == var_index:
                name = _variable_name(var)
                if name:
                    imgui.same_line()
                    imgui.text_colored(imgui.ImVec4(0.6, 1.0, 0.7, 1.0), f"({name})")
                return

    # -- event / variable / payload combo selectors -------------------------

    def _render_event_combo(self, node: dict, prop_name: str):
        """Combo box to pick an event from global_state.events."""
        if self._global_state is None:
            return
        events = self._global_state.events
        if not events:
            return

        current_val = int(node.get(prop_name, -1))
        # Build label list
        labels = []
        ids = []
        selected_idx = 0
        labels.append(f"-1: (none)")
        ids.append(-1)
        for i, evt in enumerate(events):
            eid = _event_id(evt)
            ename = _event_name(evt)
            labels.append(f"{eid}: {ename}")
            ids.append(eid)
            if eid == current_val:
                selected_idx = i + 1

        imgui.set_next_item_width(imgui.get_content_region_avail().x)
        changed, new_idx = imgui.combo(
            f"##event_combo_{prop_name}", selected_idx, labels
        )
        if changed and 0 <= new_idx < len(ids):
            node[prop_name] = ids[new_idx]

    def _render_payload_combo(self, node: dict, prop_name: str):
        """Combo box to pick a payload from global_state.payloads."""
        if self._global_state is None:
            return
        payloads = self._global_state.payloads
        if not payloads:
            return

        current_val = int(node.get(prop_name, -1))
        labels = ["-1: (none)"]
        ids = [-1]
        selected_idx = 0
        for i, p in enumerate(payloads):
            pname = _payload_name(p)
            labels.append(f"{i}: {pname}")
            ids.append(i)
            if i == current_val:
                selected_idx = i + 1

        imgui.set_next_item_width(imgui.get_content_region_avail().x)
        changed, new_idx = imgui.combo(
            f"##payload_combo_{prop_name}", selected_idx, labels
        )
        if changed and 0 <= new_idx < len(ids):
            node[prop_name] = ids[new_idx]

    def _render_variable_combo(self, node: dict, prop_name: str):
        """Combo box to pick a variable from global_state.variables."""
        if self._global_state is None:
            return
        variables = self._global_state.variables
        if not variables:
            return

        current_val = int(node.get(prop_name, -1))
        labels = ["-1: (none)"]
        ids = [-1]
        selected_idx = 0
        for i, var in enumerate(variables):
            vidx = _variable_index(var)
            vname = _variable_name(var)
            labels.append(f"{vidx}: {vname}")
            ids.append(vidx)
            if vidx == current_val:
                selected_idx = i + 1

        imgui.set_next_item_width(imgui.get_content_region_avail().x)
        changed, new_idx = imgui.combo(
            f"##var_combo_{prop_name}", selected_idx, labels
        )
        if changed and 0 <= new_idx < len(ids):
            node[prop_name] = ids[new_idx]

    # -- list property dispatch ---------------------------------------------

    def _render_list_property(self, node: dict, prop_name: str, default,
                              type_id: int):
        """Dispatch to specialised list editors based on prop_name / type_id."""
        if prop_name == "bindingArray":
            self._render_binding_array(node, prop_name)
        elif prop_name == "transitionArray":
            self._render_transition_array(node, prop_name)
        elif prop_name == "eventsArray":
            self._render_events_array(node, prop_name)
        elif prop_name == "expressionArray":
            self._render_expression_array(node, prop_name)
        elif prop_name == "triggersArray":
            self._render_triggers_array(node, prop_name)
        elif prop_name == "rangeArray":
            self._render_range_array(node, prop_name)
        elif prop_name == "boneIndices":
            self._render_bone_indices(node, prop_name)
        elif prop_name == "boneWeights":
            self._render_bone_weights(node, prop_name)
        elif prop_name == "bIsActiveArray":
            self._render_is_active_array(node, prop_name, default)
        elif prop_name in ("floatVariable", "floatValue"):
            self._render_fixed_str_array(node, prop_name, 20)
        elif prop_name in ("intVariable", "intValue"):
            self._render_fixed_int_array(node, prop_name, 4)
        else:
            # Generic fallback: show as read-only text
            imgui.text(f"{prop_name}: (list, {len(node.get(prop_name, []))} items)")

    # -----------------------------------------------------------------------
    # 1. Binding array (hkbVariableBindingSet, type 17)
    # -----------------------------------------------------------------------

    def _render_binding_array(self, node: dict, prop_name: str):
        entries: list = node.setdefault(prop_name, [])

        header_open = imgui.collapsing_header(
            f"Bindings ({len(entries)})##bindings_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        remove_idx = -1
        for i, entry in enumerate(entries):
            entry_open = imgui.tree_node(f"Binding [{i}]##binding_{i}")
            if not entry_open:
                continue

            # memberPath
            mp = entry.get("memberPath", "")
            ch, new_mp = imgui.input_text(f"memberPath##bind_mp_{i}", mp)
            if ch:
                entry["memberPath"] = new_mp

            # variableIndex
            vi = int(entry.get("variableIndex", -1))
            ch, new_vi = imgui.input_int(f"variableIndex##bind_vi_{i}", vi)
            if ch:
                entry["variableIndex"] = new_vi
            if self._global_state is not None:
                self._show_variable_hint(vi)

            # bindingType combo
            bt = entry.get("bindingType", "BINDING_TYPE_VARIABLE")
            bt_idx = 0
            if bt in BINDING_TYPE_NAMES:
                bt_idx = BINDING_TYPE_NAMES.index(bt)
            ch, new_bt_idx = imgui.combo(
                f"bindingType##bind_bt_{i}", bt_idx, BINDING_TYPE_NAMES
            )
            if ch:
                entry["bindingType"] = BINDING_TYPE_NAMES[new_bt_idx]

            # Remove button
            if imgui.button(f"Remove##bind_rm_{i}"):
                remove_idx = i

            imgui.tree_pop()

        if remove_idx >= 0:
            entries.pop(remove_idx)

        if imgui.button("+ Add Binding##bind_add"):
            entries.append({
                "memberPath": "",
                "variableIndex": -1,
                "bindingType": "BINDING_TYPE_VARIABLE",
            })

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 2. Transition array (hkbStateMachineTransitionInfoArray, type 7)
    # -----------------------------------------------------------------------

    def _render_transition_array(self, node: dict, prop_name: str):
        entries: list = node.setdefault(prop_name, [])

        header_open = imgui.collapsing_header(
            f"Transitions ({len(entries)})##trans_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        remove_idx = -1
        for i, entry in enumerate(entries):
            entry_open = imgui.tree_node(f"Transition [{i}]##trans_{i}")
            if not entry_open:
                continue

            # transition index
            ch, nv = imgui.input_int(f"transition##trans_t_{i}",
                                     int(entry.get("transition", 0)))
            if ch:
                entry["transition"] = nv

            # eventId
            eid = int(entry.get("eventId", -1))
            ch, nv = imgui.input_int(f"eventId##trans_eid_{i}", eid)
            if ch:
                entry["eventId"] = nv
            self._show_event_hint(eid)

            # toStateId
            ch, nv = imgui.input_int(f"toStateId##trans_ts_{i}",
                                     int(entry.get("toStateId", -1)))
            if ch:
                entry["toStateId"] = nv

            # fromNestedStateId
            ch, nv = imgui.input_int(f"fromNestedStateId##trans_fns_{i}",
                                     int(entry.get("fromNestedStateId", -1)))
            if ch:
                entry["fromNestedStateId"] = nv

            # toNestedStateId
            ch, nv = imgui.input_int(f"toNestedStateId##trans_tns_{i}",
                                     int(entry.get("toNestedStateId", -1)))
            if ch:
                entry["toNestedStateId"] = nv

            # priority
            ch, nv = imgui.input_int(f"priority##trans_pri_{i}",
                                     int(entry.get("priority", 0)))
            if ch:
                entry["priority"] = nv

            # flags (15-element bool array)
            flags: list = entry.setdefault("flags", [False] * 15)
            # Ensure it is always 15 elements
            while len(flags) < 15:
                flags.append(False)

            if imgui.tree_node(f"Flags##trans_flags_{i}"):
                for fi in range(15):
                    flag_name = TRANSITION_FLAG_NAMES[fi] if fi < len(
                        TRANSITION_FLAG_NAMES) else f"flag_{fi}"
                    ch, nv = imgui.checkbox(f"{flag_name}##trans_fl_{i}_{fi}",
                                            bool(flags[fi]))
                    if ch:
                        flags[fi] = nv
                imgui.tree_pop()

            if imgui.button(f"Remove##trans_rm_{i}"):
                remove_idx = i

            imgui.tree_pop()

        if remove_idx >= 0:
            entries.pop(remove_idx)

        if imgui.button("+ Add Transition##trans_add"):
            entries.append({
                "transition": 0,
                "eventId": -1,
                "toStateId": -1,
                "fromNestedStateId": -1,
                "toNestedStateId": -1,
                "priority": 0,
                "flags": [False] * 15,
            })

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 3. Events array (hkbStateMachineEventPropertyArray, type 8)
    # -----------------------------------------------------------------------

    def _render_events_array(self, node: dict, prop_name: str):
        entries: list = node.setdefault(prop_name, [])

        header_open = imgui.collapsing_header(
            f"Events ({len(entries)})##evtarr_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        remove_idx = -1
        for i, entry in enumerate(entries):
            entry_open = imgui.tree_node(f"Event [{i}]##evtarr_{i}")
            if not entry_open:
                continue

            # eventID
            eid = int(entry.get("eventID", -1))
            ch, nv = imgui.input_int(f"eventID##evtarr_eid_{i}", eid)
            if ch:
                entry["eventID"] = nv
            self._show_event_hint(eid)

            # Event combo selector
            if self._global_state and self._global_state.events:
                self._render_inline_event_combo(entry, "eventID",
                                                f"evtarr_ecb_{i}")

            # payloadID
            pid = int(entry.get("payloadID", -1))
            ch, nv = imgui.input_int(f"payloadID##evtarr_pid_{i}", pid)
            if ch:
                entry["payloadID"] = nv
            self._show_payload_hint(pid)

            if imgui.button(f"Remove##evtarr_rm_{i}"):
                remove_idx = i

            imgui.tree_pop()

        if remove_idx >= 0:
            entries.pop(remove_idx)

        if imgui.button("+ Add Event##evtarr_add"):
            entries.append({
                "eventID": -1,
                "payloadID": -1,
            })

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 4. Expression array (hkbExpressionDataArray, type 21)
    # -----------------------------------------------------------------------

    def _render_expression_array(self, node: dict, prop_name: str):
        entries: list = node.setdefault(prop_name, [])

        header_open = imgui.collapsing_header(
            f"Expressions ({len(entries)})##expr_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        remove_idx = -1
        for i, entry in enumerate(entries):
            entry_open = imgui.tree_node(f"Expression [{i}]##expr_{i}")
            if not entry_open:
                continue

            # expression string
            expr = str(entry.get("expression", ""))
            ch, nv = imgui.input_text(f"expression##expr_e_{i}", expr)
            if ch:
                entry["expression"] = nv

            # assignmentIndex
            ch, nv = imgui.input_int(f"assignmentIndex##expr_ai_{i}",
                                     int(entry.get("assignmentIndex", 0)))
            if ch:
                entry["assignmentIndex"] = nv

            # assignmentEventMode
            ch, nv = imgui.input_int(f"assignmentEventMode##expr_aem_{i}",
                                     int(entry.get("assignmentEventMode", 0)))
            if ch:
                entry["assignmentEventMode"] = nv

            if imgui.button(f"Remove##expr_rm_{i}"):
                remove_idx = i

            imgui.tree_pop()

        if remove_idx >= 0:
            entries.pop(remove_idx)

        if imgui.button("+ Add Expression##expr_add"):
            entries.append({
                "expression": "",
                "assignmentIndex": 0,
                "assignmentEventMode": 0,
            })

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 5. Triggers array (hkbClipTriggerArray, type 26)
    # -----------------------------------------------------------------------

    def _render_triggers_array(self, node: dict, prop_name: str):
        entries: list = node.setdefault(prop_name, [])

        header_open = imgui.collapsing_header(
            f"Triggers ({len(entries)})##trig_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        remove_idx = -1
        for i, entry in enumerate(entries):
            entry_open = imgui.tree_node(f"Trigger [{i}]##trig_{i}")
            if not entry_open:
                continue

            # localTime (float stored as string)
            lt = str(entry.get("localTime", "0.000000"))
            ch, nv = imgui.input_text(f"localTime##trig_lt_{i}", lt)
            if ch:
                entry["localTime"] = nv

            # eventID
            eid = int(entry.get("eventID", -1))
            ch, nv = imgui.input_int(f"eventID##trig_eid_{i}", eid)
            if ch:
                entry["eventID"] = nv
            self._show_event_hint(eid)

            if self._global_state and self._global_state.events:
                self._render_inline_event_combo(entry, "eventID",
                                                f"trig_ecb_{i}")

            # relativeToEndOfClip
            ch, nv = imgui.checkbox(
                f"relativeToEndOfClip##trig_re_{i}",
                bool(entry.get("relativeToEndOfClip", False)),
            )
            if ch:
                entry["relativeToEndOfClip"] = nv

            # acyclic
            ch, nv = imgui.checkbox(
                f"acyclic##trig_ac_{i}",
                bool(entry.get("acyclic", False)),
            )
            if ch:
                entry["acyclic"] = nv

            # isAnnotation
            ch, nv = imgui.checkbox(
                f"isAnnotation##trig_ia_{i}",
                bool(entry.get("isAnnotation", False)),
            )
            if ch:
                entry["isAnnotation"] = nv

            if imgui.button(f"Remove##trig_rm_{i}"):
                remove_idx = i

            imgui.tree_pop()

        if remove_idx >= 0:
            entries.pop(remove_idx)

        if imgui.button("+ Add Trigger##trig_add"):
            entries.append({
                "localTime": "0.000000",
                "eventID": -1,
                "relativeToEndOfClip": False,
                "acyclic": False,
                "isAnnotation": False,
            })

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 6. Range array (hkbEventRangeDataArray, type 36)
    # -----------------------------------------------------------------------

    def _render_range_array(self, node: dict, prop_name: str):
        entries: list = node.setdefault(prop_name, [])

        header_open = imgui.collapsing_header(
            f"Event Ranges ({len(entries)})##rng_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        remove_idx = -1
        for i, entry in enumerate(entries):
            entry_open = imgui.tree_node(f"Range [{i}]##rng_{i}")
            if not entry_open:
                continue

            # upperBound (float as string)
            ub = str(entry.get("upperBound", "0.000000"))
            ch, nv = imgui.input_text(f"upperBound##rng_ub_{i}", ub)
            if ch:
                entry["upperBound"] = nv

            # eventID
            eid = int(entry.get("eventID", -1))
            ch, nv = imgui.input_int(f"eventID##rng_eid_{i}", eid)
            if ch:
                entry["eventID"] = nv
            self._show_event_hint(eid)

            if self._global_state and self._global_state.events:
                self._render_inline_event_combo(entry, "eventID",
                                                f"rng_ecb_{i}")

            # payloadID
            pid = int(entry.get("payloadID", -1))
            ch, nv = imgui.input_int(f"payloadID##rng_pid_{i}", pid)
            if ch:
                entry["payloadID"] = nv
            self._show_payload_hint(pid)

            # eventMode combo
            em = int(entry.get("eventMode", 0))
            em = max(0, min(em, len(EVENT_RANGE_MODE_NAMES) - 1))
            ch, new_em = imgui.combo(
                f"eventMode##rng_em_{i}", em, EVENT_RANGE_MODE_NAMES
            )
            if ch:
                entry["eventMode"] = new_em

            if imgui.button(f"Remove##rng_rm_{i}"):
                remove_idx = i

            imgui.tree_pop()

        if remove_idx >= 0:
            entries.pop(remove_idx)

        if imgui.button("+ Add Range##rng_add"):
            entries.append({
                "upperBound": "0.000000",
                "eventID": -1,
                "payloadID": -1,
                "eventMode": 0,
            })

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 7. Bone indices (hkbBoneIndexArray, type 13)
    # -----------------------------------------------------------------------

    def _render_bone_indices(self, node: dict, prop_name: str):
        entries: list = node.setdefault(prop_name, [])

        header_open = imgui.collapsing_header(
            f"Bone Indices ({len(entries)})##bi_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        remove_idx = -1
        for i in range(len(entries)):
            ch, nv = imgui.input_int(f"[{i}]##bi_{i}", int(entries[i]))
            if ch:
                entries[i] = nv
            imgui.same_line()
            if imgui.small_button(f"X##bi_rm_{i}"):
                remove_idx = i

        if remove_idx >= 0:
            entries.pop(remove_idx)

        if imgui.button("+ Add Bone Index##bi_add"):
            entries.append(0)

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 8. Bone weights (hkbBoneWeightArray, type 14) - float as string
    # -----------------------------------------------------------------------

    def _render_bone_weights(self, node: dict, prop_name: str):
        entries: list = node.setdefault(prop_name, [])

        header_open = imgui.collapsing_header(
            f"Bone Weights ({len(entries)})##bw_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        remove_idx = -1
        for i in range(len(entries)):
            val = str(entries[i])
            ch, nv = imgui.input_text(f"[{i}]##bw_{i}", val)
            if ch:
                entries[i] = nv
            imgui.same_line()
            if imgui.small_button(f"X##bw_rm_{i}"):
                remove_idx = i

        if remove_idx >= 0:
            entries.pop(remove_idx)

        if imgui.button("+ Add Bone Weight##bw_add"):
            entries.append("0.000000")

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 9. bIsActiveArray (BSIsActiveModifier, type 16) - fixed 10 bools
    # -----------------------------------------------------------------------

    def _render_is_active_array(self, node: dict, prop_name: str, default):
        entries: list = node.setdefault(prop_name, list(default))
        # Ensure exactly 10 elements
        while len(entries) < 10:
            entries.append(False)

        header_open = imgui.collapsing_header(
            f"bIsActive[10]##bia_header"
        )
        if not header_open:
            return

        imgui.indent(8)
        for i in range(10):
            ch, nv = imgui.checkbox(f"bIsActive[{i}]##bia_{i}",
                                    bool(entries[i]))
            if ch:
                entries[i] = nv
        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 10. Fixed-length string arrays (floatVariable, floatValue) - 20 elements
    # -----------------------------------------------------------------------

    def _render_fixed_str_array(self, node: dict, prop_name: str, count: int):
        entries: list = node.setdefault(prop_name, ["0"] * count)
        while len(entries) < count:
            entries.append("0")

        header_open = imgui.collapsing_header(
            f"{prop_name}[{count}]##fsa_{prop_name}_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        # Show variable hints for floatVariable entries
        is_var_ref = prop_name == "floatVariable"

        for i in range(count):
            val = str(entries[i])
            ch, nv = imgui.input_text(f"[{i}]##fsa_{prop_name}_{i}", val)
            if ch:
                entries[i] = nv

            # For floatVariable, the value is a variable index encoded as str
            if is_var_ref and self._global_state is not None:
                try:
                    var_idx = int(val)
                    self._show_variable_hint(var_idx)
                except (ValueError, TypeError):
                    pass

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # 11. Fixed-length int arrays (intVariable, intValue) - 4 elements
    # -----------------------------------------------------------------------

    def _render_fixed_int_array(self, node: dict, prop_name: str, count: int):
        entries: list = node.setdefault(prop_name, [0] * count)
        while len(entries) < count:
            entries.append(0)

        header_open = imgui.collapsing_header(
            f"{prop_name}[{count}]##fia_{prop_name}_header"
        )
        if not header_open:
            return

        imgui.indent(8)

        is_var_ref = prop_name == "intVariable"

        for i in range(count):
            ch, nv = imgui.input_int(f"[{i}]##fia_{prop_name}_{i}",
                                     int(entries[i]))
            if ch:
                entries[i] = nv

            if is_var_ref and self._global_state is not None:
                self._show_variable_hint(int(entries[i]))

        imgui.unindent(8)

    # -----------------------------------------------------------------------
    # Inline event combo (used by sub-editors)
    # -----------------------------------------------------------------------

    def _render_inline_event_combo(self, entry: dict, key: str, uid: str):
        """Render an inline event combo for a dict entry's key.

        Used inside list sub-editors (events array, triggers, ranges)
        where the entry is a dict rather than the node itself.
        """
        if self._global_state is None:
            return
        events = self._global_state.events
        if not events:
            return

        current_val = int(entry.get(key, -1))

        labels = ["-1: (none)"]
        ids = [-1]
        selected_idx = 0
        for i, evt in enumerate(events):
            eid = _event_id(evt)
            ename = _event_name(evt)
            labels.append(f"{eid}: {ename}")
            ids.append(eid)
            if eid == current_val:
                selected_idx = i + 1

        imgui.set_next_item_width(imgui.get_content_region_avail().x)
        changed, new_idx = imgui.combo(f"##evt_{uid}", selected_idx, labels)
        if changed and 0 <= new_idx < len(ids):
            entry[key] = ids[new_idx]
