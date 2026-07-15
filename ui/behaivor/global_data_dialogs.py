"""Imgui modal popup for editing global behavior graph data.

Provides a tabbed modal dialog for editing Variables, Events, Transitions,
Payloads, and Properties stored in GlobalState.
"""

from imgui_bundle import imgui

# ── Enum labels for combo boxes ──────────────────────────────────────────

VARIABLE_TYPES = [
    "Bool", "Int8", "Int16", "Int32", "Real",
    "Pointer", "Vector4", "Quaternion",
]

EVENT_FLAGS = ["None", "Sync Point"]

SELF_TRANSITION_MODES = [
    "Continue if Cyclic, Blend if Acyclic",
    "Continue",
    "Reset",
    "Blend",
]

EVENT_MODES = [
    "Default",
    "Process All",
    "Ignore From Generator",
    "Ignore To Generator",
]

TRANSITION_FLAGS = [
    "None",
    "Ignore From World From Model",
    "Sync",
    "Ignore To World From Model",
    "Ignore To World From Model Rotation",
]

END_MODES = ["None", "Transition Until End of From Generator"]

# Blend curve labels (hkbBlendCurveUtils::BlendCurve enum)
BLEND_CURVES = [
    "Smooth (0)",
    "Linear (1)",
    "Linear To Smooth (2)",
    "Smooth To Linear (3)",
]


class GlobalDataDialogs:
    """Imgui modal popup with tab bar for editing global graph data."""

    def __init__(self):
        self._open: bool = False
        self._pending_open: bool = False
        self._tab: str = "variables"
        self._behavior_db = None

        # Import-popup state
        self._import_open: bool = False
        self._import_popup_id: str = ""
        self._import_filter: str = ""
        self._import_items: list[tuple] = []  # (name, ...) tuples from DB
        self._import_selected: dict[str, bool] = {}
        self._import_target: str = ""  # "variables" or "events"

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def open(self, tab: str = "variables", behavior_db=None):
        """Open the dialog, showing the specified tab."""
        self._open = True
        self._pending_open = True
        self._tab = tab
        self._behavior_db = behavior_db

    def render(self, global_state):
        """Call each frame. Shows modal popup if open."""
        if not self._open:
            return

        if self._pending_open:
            imgui.open_popup("Global Data##global_dlg")
            self._pending_open = False

        center = imgui.get_main_viewport().get_center()
        imgui.set_next_window_pos(center, imgui.Cond_.appearing, imgui.ImVec2(0.5, 0.5))
        imgui.set_next_window_size(imgui.ImVec2(900, 500), imgui.Cond_.appearing)

        opened, _ = imgui.begin_popup_modal("Global Data##global_dlg")
        if opened:
            if imgui.begin_tab_bar("global_tabs"):
                self._render_variables_tab(global_state)
                self._render_events_tab(global_state)
                self._render_transitions_tab(global_state)
                self._render_payloads_tab(global_state)
                self._render_properties_tab(global_state)
                imgui.end_tab_bar()

            imgui.separator()
            if imgui.button("Close"):
                self._open = False
                imgui.close_current_popup()
            imgui.end_popup()
        else:
            self._open = False

    # ------------------------------------------------------------------
    # Variables tab
    # ------------------------------------------------------------------

    def _render_variables_tab(self, gs):
        flags = imgui.TabItemFlags_.set_selected if self._tab == "variables" else imgui.TabItemFlags_.none
        selected, _ = imgui.begin_tab_item("Variables", flags=flags)
        if not selected:
            return

        tbl_flags = (
            imgui.TableFlags_.borders
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.scroll_y
        )
        avail = imgui.get_content_region_avail()
        # Reserve space for: add/delete buttons row + separator + Close button below tab bar
        table_height = avail.y - imgui.get_frame_height_with_spacing() * 3

        if imgui.begin_table("##var_tbl", 7, tbl_flags, imgui.ImVec2(0, table_height)):
            imgui.table_setup_column("ID", imgui.TableColumnFlags_.width_fixed, 40)
            imgui.table_setup_column("Name")
            imgui.table_setup_column("Type", imgui.TableColumnFlags_.width_fixed, 110)
            imgui.table_setup_column("Value", imgui.TableColumnFlags_.width_fixed, 80)
            imgui.table_setup_column("Min", imgui.TableColumnFlags_.width_fixed, 80)
            imgui.table_setup_column("Max", imgui.TableColumnFlags_.width_fixed, 80)
            imgui.table_setup_column("Quad Values")
            imgui.table_headers_row()

            to_delete = -1
            for i, v in enumerate(gs.variables):
                imgui.table_next_row()
                imgui.push_id(f"var_{i}")

                # ID
                imgui.table_next_column()
                imgui.text(str(v.get("variableID", i)))

                # Name
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##name", v.get("variableName", ""))
                if changed:
                    v["variableName"] = val

                # Type combo
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                cur_type = int(v.get("variableType", 0))
                if 0 <= cur_type < len(VARIABLE_TYPES):
                    preview = VARIABLE_TYPES[cur_type]
                else:
                    preview = str(cur_type)
                if imgui.begin_combo("##type", preview):
                    for ti, label in enumerate(VARIABLE_TYPES):
                        is_sel = ti == cur_type
                        if imgui.selectable(label, is_sel)[0]:
                            v["variableType"] = ti
                        if is_sel:
                            imgui.set_item_default_focus()
                    imgui.end_combo()

                # Value
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##val", str(v.get("variableValue", "0")))
                if changed:
                    v["variableValue"] = val

                # Min
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##min", str(v.get("variableMinValue", "0")))
                if changed:
                    v["variableMinValue"] = val

                # Max
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##max", str(v.get("variableMaxValue", "0")))
                if changed:
                    v["variableMaxValue"] = val

                # Quad values
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##quad", str(v.get("variableQuadValues", "")))
                if changed:
                    v["variableQuadValues"] = val

                # Right-click context menu for delete
                if imgui.begin_popup_context_item("##ctx"):
                    if imgui.selectable("Delete", False)[0]:
                        to_delete = i
                    imgui.end_popup()

                imgui.pop_id()

            imgui.end_table()

            if to_delete >= 0:
                gs.variables.pop(to_delete)

        # Buttons below table
        if imgui.button("Add Variable"):
            next_id = len(gs.variables)
            gs.variables.append({
                "variableID": next_id,
                "variableName": "",
                "variableType": 0,
                "variableValue": "0",
                "variableMinValue": "0",
                "variableMaxValue": "0",
                "variableQuadValues": "",
            })

        imgui.same_line()
        if imgui.button("Delete Last") and gs.variables:
            gs.variables.pop()

        if self._behavior_db is not None and self._behavior_db.available:
            imgui.same_line()
            if imgui.button("Import Known...##imp_var"):
                self._open_import_popup("variables", gs)

        # Render the import sub-popup if active
        self._render_import_popup_if_active(gs)

        imgui.end_tab_item()

    # ------------------------------------------------------------------
    # Events tab
    # ------------------------------------------------------------------

    def _render_events_tab(self, gs):
        flags = imgui.TabItemFlags_.set_selected if self._tab == "events" else imgui.TabItemFlags_.none
        selected, _ = imgui.begin_tab_item("Events", flags=flags)
        if not selected:
            return

        tbl_flags = (
            imgui.TableFlags_.borders
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.scroll_y
        )
        avail = imgui.get_content_region_avail()
        table_height = avail.y - imgui.get_frame_height_with_spacing() * 3

        if imgui.begin_table("##evt_tbl", 3, tbl_flags, imgui.ImVec2(0, table_height)):
            imgui.table_setup_column("ID", imgui.TableColumnFlags_.width_fixed, 40)
            imgui.table_setup_column("Name")
            imgui.table_setup_column("Flags", imgui.TableColumnFlags_.width_fixed, 130)
            imgui.table_headers_row()

            to_delete = -1
            for i, e in enumerate(gs.events):
                imgui.table_next_row()
                imgui.push_id(f"evt_{i}")

                # ID
                imgui.table_next_column()
                imgui.text(str(e.get("eventID", i)))

                # Name
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##name", e.get("eventName", ""))
                if changed:
                    e["eventName"] = val

                # Flags combo
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                cur_flags = int(e.get("eventFlags", 0))
                if 0 <= cur_flags < len(EVENT_FLAGS):
                    preview = EVENT_FLAGS[cur_flags]
                else:
                    preview = str(cur_flags)
                if imgui.begin_combo("##flags", preview):
                    for fi, label in enumerate(EVENT_FLAGS):
                        is_sel = fi == cur_flags
                        if imgui.selectable(label, is_sel)[0]:
                            e["eventFlags"] = fi
                        if is_sel:
                            imgui.set_item_default_focus()
                    imgui.end_combo()

                if imgui.begin_popup_context_item("##ctx"):
                    if imgui.selectable("Delete", False)[0]:
                        to_delete = i
                    imgui.end_popup()

                imgui.pop_id()

            imgui.end_table()

            if to_delete >= 0:
                gs.events.pop(to_delete)

        if imgui.button("Add Event"):
            next_id = len(gs.events)
            gs.events.append({
                "eventID": next_id,
                "eventName": "",
                "eventFlags": 0,
            })

        imgui.same_line()
        if imgui.button("Delete Last") and gs.events:
            gs.events.pop()

        if self._behavior_db is not None and self._behavior_db.available:
            imgui.same_line()
            if imgui.button("Import Known...##imp_evt"):
                self._open_import_popup("events", gs)

        self._render_import_popup_if_active(gs)

        imgui.end_tab_item()

    # ------------------------------------------------------------------
    # Transitions tab
    # ------------------------------------------------------------------

    def _render_transitions_tab(self, gs):
        flags = imgui.TabItemFlags_.set_selected if self._tab == "transitions" else imgui.TabItemFlags_.none
        selected, _ = imgui.begin_tab_item("Transitions", flags=flags)
        if not selected:
            return

        tbl_flags = (
            imgui.TableFlags_.borders
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.scroll_y
        )
        avail = imgui.get_content_region_avail()
        table_height = avail.y - imgui.get_frame_height_with_spacing() * 3

        if imgui.begin_table("##trans_tbl", 8, tbl_flags, imgui.ImVec2(0, table_height)):
            imgui.table_setup_column("ID", imgui.TableColumnFlags_.width_fixed, 30)
            imgui.table_setup_column("Name")
            imgui.table_setup_column("Duration", imgui.TableColumnFlags_.width_fixed, 70)
            imgui.table_setup_column("Self Trans. Mode", imgui.TableColumnFlags_.width_fixed, 120)
            imgui.table_setup_column("Event Mode", imgui.TableColumnFlags_.width_fixed, 120)
            imgui.table_setup_column("Flags", imgui.TableColumnFlags_.width_fixed, 120)
            imgui.table_setup_column("End Mode", imgui.TableColumnFlags_.width_fixed, 120)
            imgui.table_setup_column("Blend Curve", imgui.TableColumnFlags_.width_fixed, 100)
            imgui.table_headers_row()

            to_delete = -1
            for i, t in enumerate(gs.transitions):
                imgui.table_next_row()
                imgui.push_id(f"trans_{i}")

                # ID
                imgui.table_next_column()
                imgui.text(str(t.get("transitionID", i)))

                # Name
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##name", t.get("transitionName", ""))
                if changed:
                    t["transitionName"] = val

                # Duration
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##dur", str(t.get("transitionDuration", "0")))
                if changed:
                    t["transitionDuration"] = val

                # Self transition mode
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                self._combo_enum("##stm", t, "transitionSelfTransitionMode", SELF_TRANSITION_MODES)

                # Event mode
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                self._combo_enum("##em", t, "transitionEventMode", EVENT_MODES)

                # Flags
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                self._combo_enum("##fl", t, "transitionFlags", TRANSITION_FLAGS)

                # End mode
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                self._combo_enum("##endm", t, "transitionEndMode", END_MODES)

                # Blend curve
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                self._combo_enum("##bc", t, "transitionBlendCurve", BLEND_CURVES)

                if imgui.begin_popup_context_item("##ctx"):
                    if imgui.selectable("Delete", False)[0]:
                        to_delete = i
                    imgui.end_popup()

                imgui.pop_id()

            imgui.end_table()

            if to_delete >= 0:
                gs.transitions.pop(to_delete)

        if imgui.button("Add Transition"):
            next_id = len(gs.transitions)
            gs.transitions.append({
                "transitionID": next_id,
                "transitionName": "",
                "transitionDuration": "0",
                "transitionSelfTransitionMode": 0,
                "transitionEventMode": 0,
                "transitionFlags": 0,
                "transitionEndMode": 0,
                "transitionBlendCurve": 0,
                "transitionVariableBindingSet": 0,
                "transitionToGeneratorStartTimeFraction": "0",
            })

        imgui.same_line()
        if imgui.button("Delete Last") and gs.transitions:
            gs.transitions.pop()

        imgui.end_tab_item()

    # ------------------------------------------------------------------
    # Payloads tab
    # ------------------------------------------------------------------

    def _render_payloads_tab(self, gs):
        flags = imgui.TabItemFlags_.set_selected if self._tab == "payloads" else imgui.TabItemFlags_.none
        selected, _ = imgui.begin_tab_item("Payloads", flags=flags)
        if not selected:
            return

        tbl_flags = (
            imgui.TableFlags_.borders
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.scroll_y
        )
        avail = imgui.get_content_region_avail()
        table_height = avail.y - imgui.get_frame_height_with_spacing() * 3

        if imgui.begin_table("##pay_tbl", 2, tbl_flags, imgui.ImVec2(0, table_height)):
            imgui.table_setup_column("ID", imgui.TableColumnFlags_.width_fixed, 40)
            imgui.table_setup_column("Name")
            imgui.table_headers_row()

            to_delete = -1
            for i, p in enumerate(gs.payloads):
                imgui.table_next_row()
                imgui.push_id(f"pay_{i}")

                imgui.table_next_column()
                imgui.text(str(p.get("payloadID", i)))

                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##name", p.get("payloadName", ""))
                if changed:
                    p["payloadName"] = val

                if imgui.begin_popup_context_item("##ctx"):
                    if imgui.selectable("Delete", False)[0]:
                        to_delete = i
                    imgui.end_popup()

                imgui.pop_id()

            imgui.end_table()

            if to_delete >= 0:
                gs.payloads.pop(to_delete)

        if imgui.button("Add Payload"):
            next_id = len(gs.payloads)
            gs.payloads.append({
                "payloadID": next_id,
                "payloadName": "",
            })

        imgui.same_line()
        if imgui.button("Delete Last") and gs.payloads:
            gs.payloads.pop()

        imgui.end_tab_item()

    # ------------------------------------------------------------------
    # Properties tab
    # ------------------------------------------------------------------

    def _render_properties_tab(self, gs):
        flags = imgui.TabItemFlags_.set_selected if self._tab == "properties" else imgui.TabItemFlags_.none
        selected, _ = imgui.begin_tab_item("Properties", flags=flags)
        if not selected:
            return

        tbl_flags = (
            imgui.TableFlags_.borders
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.scroll_y
        )
        avail = imgui.get_content_region_avail()
        table_height = avail.y - imgui.get_frame_height_with_spacing() * 3

        if imgui.begin_table("##prop_tbl", 3, tbl_flags, imgui.ImVec2(0, table_height)):
            imgui.table_setup_column("ID", imgui.TableColumnFlags_.width_fixed, 40)
            imgui.table_setup_column("Name")
            imgui.table_setup_column("Type", imgui.TableColumnFlags_.width_fixed, 110)
            imgui.table_headers_row()

            to_delete = -1
            for i, p in enumerate(gs.properties):
                imgui.table_next_row()
                imgui.push_id(f"prop_{i}")

                imgui.table_next_column()
                imgui.text(str(p.get("propertiesID", i)))

                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                changed, val = imgui.input_text("##name", p.get("propertiesName", ""))
                if changed:
                    p["propertiesName"] = val

                # Type combo — reuses VARIABLE_TYPES since property types are the same enum
                imgui.table_next_column()
                imgui.set_next_item_width(-1)
                self._combo_enum("##type", p, "propertiesType", VARIABLE_TYPES)

                if imgui.begin_popup_context_item("##ctx"):
                    if imgui.selectable("Delete", False)[0]:
                        to_delete = i
                    imgui.end_popup()

                imgui.pop_id()

            imgui.end_table()

            if to_delete >= 0:
                gs.properties.pop(to_delete)

        if imgui.button("Add Property"):
            next_id = len(gs.properties)
            gs.properties.append({
                "propertiesID": next_id,
                "propertiesName": "",
                "propertiesType": 0,
            })

        imgui.same_line()
        if imgui.button("Delete Last") and gs.properties:
            gs.properties.pop()

        imgui.end_tab_item()

    # ------------------------------------------------------------------
    # Import sub-popup (for Variables and Events)
    # ------------------------------------------------------------------

    def _open_import_popup(self, target: str, gs):
        """Prepare and open the import checklist sub-popup."""
        self._import_target = target
        self._import_filter = ""
        self._import_selected = {}

        if target == "variables":
            existing = {v.get("variableName", "") for v in gs.variables}
            raw = self._behavior_db.get_variables_with_counts()
            # raw: list of (name, type_str, count)
            self._import_items = [(name, vtype, count) for name, vtype, count in raw]
            popup_id = "Import Variables##imp_var_pop"
        else:
            existing = {e.get("eventName", "") for e in gs.events}
            raw = self._behavior_db.get_events_with_counts()
            # raw: list of (name, count)
            self._import_items = [(name, count) for name, count in raw]
            popup_id = "Import Events##imp_evt_pop"

        # Pre-deselect items already present
        for item in self._import_items:
            name = item[0]
            self._import_selected[name] = False

        self._import_popup_id = popup_id
        self._import_open = True
        imgui.open_popup(popup_id)

    def _render_import_popup_if_active(self, gs):
        """Render the import checklist sub-popup if it is open."""
        if not self._import_open:
            return

        center = imgui.get_main_viewport().get_center()
        imgui.set_next_window_pos(center, imgui.Cond_.appearing, imgui.ImVec2(0.5, 0.5))
        imgui.set_next_window_size(imgui.ImVec2(500, 450), imgui.Cond_.appearing)

        opened, _ = imgui.begin_popup_modal(self._import_popup_id)
        if not opened:
            self._import_open = False
            return

        # Determine existing names for grey-out
        if self._import_target == "variables":
            existing = {v.get("variableName", "") for v in gs.variables}
        else:
            existing = {e.get("eventName", "") for e in gs.events}

        # Filter input
        changed, self._import_filter = imgui.input_text_with_hint(
            "##imp_filter", "Filter...", self._import_filter
        )
        filter_lower = self._import_filter.lower()

        imgui.same_line()
        if imgui.button("Select All"):
            for item in self._import_items:
                name = item[0]
                if name not in existing and (not filter_lower or filter_lower in name.lower()):
                    self._import_selected[name] = True

        imgui.same_line()
        if imgui.button("Select None"):
            for key in self._import_selected:
                self._import_selected[key] = False

        imgui.separator()

        # Scrolling checklist
        avail = imgui.get_content_region_avail()
        child_height = avail.y - 34
        imgui.begin_child("##imp_list", imgui.ImVec2(0, child_height))

        for item in self._import_items:
            name = item[0]
            if filter_lower and filter_lower not in name.lower():
                continue

            already_exists = name in existing
            if already_exists:
                imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.5, 0.5, 0.5, 1.0))

            is_checked = self._import_selected.get(name, False)

            if already_exists:
                # Show disabled checkbox for items already present
                imgui.begin_disabled()
                imgui.checkbox(f"##chk_{name}", False)
                imgui.end_disabled()
            else:
                clicked, new_val = imgui.checkbox(f"##chk_{name}", is_checked)
                if clicked:
                    self._import_selected[name] = new_val

            imgui.same_line()

            # Build display label
            if self._import_target == "variables":
                # item = (name, type_str, count)
                vtype = item[1] if len(item) > 1 else ""
                count = item[2] if len(item) > 2 else 0
                label = f"{name}  ({vtype}, {count} uses)"
            else:
                # item = (name, count)
                count = item[1] if len(item) > 1 else 0
                label = f"{name}  ({count} uses)"

            if already_exists:
                label += "  [already added]"

            imgui.text(label)

            if already_exists:
                imgui.pop_style_color()

        imgui.end_child()

        # OK / Cancel
        imgui.separator()
        if imgui.button("OK", imgui.ImVec2(80, 0)):
            self._apply_import(gs)
            self._import_open = False
            imgui.close_current_popup()

        imgui.same_line()
        if imgui.button("Cancel", imgui.ImVec2(80, 0)):
            self._import_open = False
            imgui.close_current_popup()

        imgui.end_popup()

    def _apply_import(self, gs):
        """Add selected items from the import popup to the global state."""
        selected_names = [
            name for name, checked in self._import_selected.items() if checked
        ]
        if not selected_names:
            return

        if self._import_target == "variables":
            existing = {v.get("variableName", "") for v in gs.variables}
            # Build a lookup from import items for type info
            type_lookup = {}
            for item in self._import_items:
                type_lookup[item[0]] = item[1] if len(item) > 1 else ""

            for name in selected_names:
                if name in existing:
                    continue
                next_id = len(gs.variables)
                # Map type string to index
                vtype_str = type_lookup.get(name, "")
                vtype_idx = 0
                for ti, label in enumerate(VARIABLE_TYPES):
                    if label.lower() == vtype_str.lower():
                        vtype_idx = ti
                        break

                gs.variables.append({
                    "variableID": next_id,
                    "variableName": name,
                    "variableType": vtype_idx,
                    "variableValue": "0",
                    "variableMinValue": "0",
                    "variableMaxValue": "0",
                    "variableQuadValues": "",
                })

        elif self._import_target == "events":
            existing = {e.get("eventName", "") for e in gs.events}
            for name in selected_names:
                if name in existing:
                    continue
                next_id = len(gs.events)
                gs.events.append({
                    "eventID": next_id,
                    "eventName": name,
                    "eventFlags": 0,
                })

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _combo_enum(label: str, data: dict, key: str, options: list[str]):
        """Render a combo box for an integer-indexed enum field in a dict."""
        cur = int(data.get(key, 0))
        if 0 <= cur < len(options):
            preview = options[cur]
        else:
            preview = str(cur)
        if imgui.begin_combo(label, preview):
            for idx, opt_label in enumerate(options):
                is_sel = idx == cur
                if imgui.selectable(opt_label, is_sel)[0]:
                    data[key] = idx
                if is_sel:
                    imgui.set_item_default_focus()
            imgui.end_combo()
