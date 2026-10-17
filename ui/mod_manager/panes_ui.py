"""MO2's right-hand information architecture, using native toolkit styling."""
from pathlib import Path

from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import action_button, scaled

from creation_lib.mod_manager.plugin_state import load_indices, review, default_order, is_master, is_light
from .plugin_keys import handle as handle_plugin_keys, provider_action, select as select_plugin
from creation_lib.mod_manager.plugin_drag import begin as begin_plugin_drag, drop as drop_plugins
from .shortcuts import pressed, label as shortcut_label


class ManagerPanes:
    def __init__(self, workspace):
        self.workspace = workspace
        self.catalog = None
        self.order, self.active = [], set()
        self.locked = {}
        self.query, self.selected = "", ""
        self.selected_plugins, self.selection_anchor = set(), ""
        self.plugin_focused, self._focus_plugin_filter, self._scroll_plugin = False, False, ""
        self.plan = None
        self._drag = None
        self._requested = None
        self._auto_plugins = None
        self.archive_query, self.archive_source, self.archive_conflicts = "", "", False
        self._archive_key, self._archive_members = None, []
        self._archive_selected = ""
        from .archive_preview_ui import ArchivePreviewUI
        self.archive_preview = ArchivePreviewUI(workspace)
        self.save_query, self.save_selected = "", ""
        self.data_query, self.data_expanded = "", set()
        self.data_combined, self.data_overlaps = True, False
        self._data_entry = None
        self._data_overlap_key, self._data_overlap_rows = None, []

    @property
    def manager(self):
        return self.workspace.manager

    def draw(self):
        w = self.workspace
        information = False
        requested = w._right_pane
        applying = requested != self._requested
        opened_name = ""
        imgui.push_style_var(imgui.StyleVar_.frame_padding, (scaled(8), scaled(3)))
        try:
            if imgui.begin_tab_bar("manager_panes", imgui.TabBarFlags_.fitting_policy_scroll | imgui.TabBarFlags_.tab_list_popup_button):
                try:
                    for name, draw in (("Plugins", self.plugins), ("Archives", self.archives), ("Data", self.data),
                                       ("Saves", self.saves), ("Downloads", self.downloads), ("Information", None)):
                        flags = imgui.TabItemFlags_.set_selected if applying and name == requested else 0
                        if imgui.begin_tab_item("Mod info" if name == "Information" else name, flags=flags)[0]:
                            opened_name = name
                            imgui.pop_style_var()
                            try:
                                if draw:
                                    draw()
                                else:
                                    information = True
                            finally:
                                imgui.push_style_var(imgui.StyleVar_.frame_padding, (scaled(8), scaled(3)))
                                imgui.end_tab_item()
                finally:
                    imgui.end_tab_bar()
        finally:
            imgui.pop_style_var()
        if opened_name and not applying and w._right_pane == requested:
            w._right_pane = opened_name
        self._requested = requested
        return not information

    def plugins(self):
        w, m = self.workspace, self.manager
        if imgui.get_drag_drop_payload_py_id() is None:
            self._drag = None
        self.plugin_focused = imgui.is_window_focused(imgui.FocusedFlags_.child_windows)
        context = (id(m.library), m.profile_id, m.target_id)
        if w._right_pane == "Plugins" and self._auto_plugins != context and m.profile_id and not m.busy and not m.runtime_active:
            self._auto_plugins = context
            if m.plugin_catalog is None:
                w._action(lambda: m.inspect_plugins(metadata=True))
        if action_button("Refresh plugins", enabled=bool(m.profile_id) and not m.busy and not m.runtime_active):
            w._action(lambda: m.inspect_plugins(metadata=True))
        imgui.same_line()
        if action_button("Library order", enabled=bool(m.plugin_catalog) and not m.busy and not m.runtime_active):
            def arrange():
                from dataclasses import replace
                self.order = default_order(replace(self.catalog, locked=tuple(self.locked.values())))
                self.plan = None
            w._action(arrange)
        imgui.set_item_tooltip("Start with library priority, then put required masters before their dependents. Save to keep this order.")
        from .toolbar_ui import _next_button
        if w.plugin_file_ui.last and m.library is getattr(w.plugin_file_ui, "library", None):
            _next_button("Undo last owned plugin file action")
            if action_button("Undo last owned plugin file action", enabled=not m.busy and not m.runtime_active):
                w._action(w.plugin_file_ui.undo)
        catalog = m.plugin_catalog
        if not catalog:
            imgui.text_wrapped("Reading plugins and dependencies..." if m.busy else "Refresh plugins to see this profile's load order.")
            return
        if catalog is not self.catalog:
            self.catalog = catalog
            self.order = [p.name for p in catalog.plugins]
            self.active = {p.name.casefold() for p in catalog.plugins if p.enabled}
            self.locked = {name.casefold(): (name, priority) for name, priority in catalog.locked}
            self.plan = None
            self.selected_plugins.intersection_update(self.order)
            if self.selected not in self.order:
                self.selected = ""
            if self.selection_anchor not in self.order:
                self.selection_anchor = ""
        by_name = {p.name.casefold(): p for p in catalog.plugins}
        positions = {name: index for index, name in enumerate(self.order)}
        indices = load_indices(catalog, self.order, self.active)
        _next_button("Save load order")
        if action_button("Save load order", primary=True, enabled=not m.busy and not m.runtime_active):
            w._action(lambda: m.apply_plugins(review(catalog, self.order, self.active, self.locked.values())))
        imgui.set_item_tooltip("Validate dependencies and save this profile's plugin activation and order.")
        imgui.same_line()
        if action_button("Revert", enabled=not m.busy):
            self.catalog, self.plan = None, None
        imgui.set_next_item_width(-1)
        if self._focus_plugin_filter:
            imgui.set_keyboard_focus_here()
            self._focus_plugin_filter = False
        _, self.query = imgui.input_text_with_hint("##plugin_filter", "Search plugins...", self.query)
        if self.selected in self.order:
            index = self.order.index(self.selected)
            for label, step in (("Up", -1), ("Down", 1)):
                if action_button(label, enabled=0 <= index + step < len(self.order) and not m.busy and not m.runtime_active):
                    w._action(lambda step=step: self.move_plugin(self.selected, index + step))
                imgui.same_line()
            key = self.selected.casefold()
            if action_button("Unlock priority" if key in self.locked else "Lock priority", enabled=not by_name[key].forced and not m.busy and not m.runtime_active):
                if key in self.locked:
                    del self.locked[key]
                else:
                    self.locked[key] = (self.selected, index)
                self.plan = None
        if self.locked:
            imgui.same_line()
            if action_button("Clear draft locks", enabled=not m.busy and not m.runtime_active):
                self.locked, self.plan = {}, None
        if catalog.issues and imgui.collapsing_header(f"Plugin notices ({len(catalog.issues)})"):
            for issue in catalog.issues[:10]:
                imgui.text_wrapped(issue)
        visible = [name for name in self.order if not self.query or self.query.casefold() in (name + " " + by_name[name.casefold()].provider).casefold()]
        hovered_priority = ""
        height = max(scaled(100), imgui.get_content_region_avail().y - imgui.get_frame_height_with_spacing() * 2)
        imgui.begin_child("plugin_list", (0, height))
        keyboard_handled = handle_plugin_keys(self, visible, by_name)
        imgui.push_style_var(imgui.StyleVar_.cell_padding, (scaled(3), scaled(1)))
        row_height = max(w._row_height, imgui.get_frame_height()) + 2 * imgui.get_style().cell_padding.y
        table_open = imgui.begin_table("plugin_table", 5, imgui.TableFlags_.resizable | imgui.TableFlags_.reorderable | imgui.TableFlags_.hideable | imgui.TableFlags_.row_bg | imgui.TableFlags_.scroll_y | imgui.TableFlags_.borders_inner_v)
        if not table_open:
            imgui.pop_style_var()
            imgui.end_child()
            return
        imgui.table_setup_column("Priority", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("Priority").x + scaled(6))
        imgui.table_setup_column("Plugin", imgui.TableColumnFlags_.width_stretch, 3)
        imgui.table_setup_column("Type", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("ESP / ESL").x + scaled(6))
        imgui.table_setup_column("Index", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("FE:000").x + scaled(6))
        imgui.table_setup_column("Provider", imgui.TableColumnFlags_.width_stretch, 1)
        imgui.table_setup_scroll_freeze(0, 1)
        imgui.table_next_row(imgui.TableRowFlags_.headers)
        for column, label in enumerate(("Priority", "Plugin", "Type", "Index", "Provider")):
            imgui.table_set_column_index(column)
            imgui.table_header(label)
            imgui.set_item_tooltip("Drag to reorder columns; drag a divider to resize. Right-click to choose visible columns.")
        if self._scroll_plugin in visible:
            imgui.set_scroll_y(max(0, visible.index(self._scroll_plugin) * row_height - height * .4))
            self._scroll_plugin = ""
        clipper = imgui.ListClipper()
        clipper.begin(len(visible), row_height)
        while clipper.step():
            for index in range(clipper.display_start, clipper.display_end):
                name = visible[index]
                plugin = by_name[name.casefold()]
                imgui.push_id(name.casefold())
                imgui.table_next_row(min_row_height=row_height)
                if name in self.selected_plugins:
                    imgui.table_set_bg_color(imgui.TableBgTarget_.row_bg0, imgui.get_color_u32(imgui.Col_.header))
                imgui.table_set_column_index(0)
                cursor = imgui.get_cursor_screen_pos()
                if imgui.selectable("##plugin_priority_" + name.casefold(), False, size=(0, imgui.get_frame_height()))[0] and not keyboard_handled:
                    select_plugin(self, name, visible)
                imgui.get_window_draw_list().add_text(cursor, imgui.get_color_u32(imgui.Col_.text), str(positions[name]))
                if imgui.is_item_hovered():
                    hovered_priority = name
                    if imgui.is_mouse_double_clicked(0) and not plugin.forced and not m.busy and not m.runtime_active:
                        w._action(lambda: w.plugin_priority.open(name))
                imgui.set_item_tooltip("Double-click to set a priority. Drag a plugin or use Up / Down to move it.")
                imgui.table_set_column_index(1)
                imgui.begin_disabled(plugin.forced or m.busy or m.runtime_active)
                changed, enabled = imgui.checkbox("##plugin_enabled", name.casefold() in self.active)
                if changed and not keyboard_handled:
                    (self.active.add if enabled else self.active.discard)(name.casefold())
                    self.plan = None
                imgui.end_disabled()
                imgui.same_line()
                if imgui.selectable("##plugin_selection", name in self.selected_plugins, imgui.SelectableFlags_.allow_double_click, size=(0, imgui.get_frame_height()))[0] and not keyboard_handled:
                    io = imgui.get_io()
                    select_plugin(self, name, visible, ctrl=io.key_ctrl, shift=io.key_shift)
                    if imgui.is_mouse_double_clicked(0) and (io.key_ctrl or io.key_shift):
                        w._action(lambda: provider_action(self, nexus=io.key_shift))
                imgui.get_window_draw_list().add_text(imgui.get_item_rect_min(), imgui.get_color_u32(imgui.Col_.text), name)
                imgui.set_item_tooltip(f"{name}\n{plugin.provider}\n{plugin.path or 'Unavailable'}\nLoad index: {indices[name.casefold()]} (requires known active headers)\n" + ("Required game master" if plugin.forced else "Drag to reorder; changes require review."))
                if not m.busy and not m.runtime_active and imgui.begin_drag_drop_source():
                    if self._drag is None:
                        w._action(lambda: setattr(self, "_drag", begin_plugin_drag(self, name)))
                    if self._drag:
                        imgui.set_drag_drop_payload_py_id("MODBOX_PLUGIN", 1)
                        imgui.text_unformatted(f"Move {len(self._drag.selected)} plugin(s) before target")
                        imgui.text_unformatted(name)
                    imgui.end_drag_drop_source()
                if not m.busy and not m.runtime_active and imgui.begin_drag_drop_target():
                    if imgui.accept_drag_drop_payload_py_id("MODBOX_PLUGIN") and self._drag:
                        w._action(lambda: drop_plugins(self, self._drag, name))
                    imgui.end_drag_drop_target()
                if imgui.begin_popup_context_item("plugin_actions"):
                    if self.selected != name:
                        select_plugin(self, name, visible)
                    allowed = not m.busy and not m.runtime_active and bool(plugin.mod_id in m.rows_by_id)
                    if imgui.menu_item("Edit draft priority", shortcut_label(w, "plugin_edit") + " over priority", False, not plugin.forced and name.casefold() not in self.locked and not m.busy and not m.runtime_active)[0]:
                        w._action(lambda: w.plugin_priority.open(name))
                    if imgui.menu_item("Open provider folder", shortcut_label(w, "plugin_folder"), False, allowed)[0]:
                        w._action(lambda: provider_action(self))
                    if imgui.menu_item("Visit provider on Nexus", "Shift+double-click", False, allowed)[0]:
                        w._action(lambda: provider_action(self, nexus=True))
                    from creation_lib.mod_manager.plugin_file_actions import unavailable
                    if imgui.menu_item("Owned plugin file actions...", "", False, not m.busy and not m.runtime_active and not unavailable(m.library, catalog, plugin))[0]:
                        w._action(lambda: w.plugin_file_ui.open(name))
                    if imgui.menu_item("Unlock priority" if name.casefold() in self.locked else "Lock priority", "", False, not plugin.forced and not m.busy and not m.runtime_active)[0]:
                        if name.casefold() in self.locked:
                            del self.locked[name.casefold()]
                        else:
                            self.locked[name.casefold()] = (name, self.order.index(name))
                        self.plan = None
                    imgui.end_popup()
                imgui.table_set_column_index(2)
                kind = ("ESM" if is_master(plugin) else "ESP") + (" / ESL" if is_light(plugin) and not name.lower().endswith(".esl") else "")
                if name.lower().endswith(".esl"):
                    kind = "ESL"
                imgui.text_unformatted(kind if plugin.header else Path(name).suffix[1:].upper() + " ?")
                imgui.set_item_tooltip("Header flags determine master and light behavior. '?' means the header has not been read.")
                imgui.table_set_column_index(3)
                imgui.text_unformatted(indices[name.casefold()])
                imgui.table_set_column_index(4)
                imgui.text_unformatted("Missing" if not plugin.path else "Ghosted" if plugin.ghosted else plugin.provider)
                imgui.set_item_tooltip(plugin.path or "This plugin is unavailable and will be omitted from launch files.")
                imgui.pop_id()
        imgui.end_table()
        imgui.pop_style_var()
        io = imgui.get_io()
        if (hovered_priority and imgui.is_window_focused(imgui.FocusedFlags_.child_windows)
                and not io.want_text_input and not m.busy and not m.runtime_active and pressed(w, "plugin_edit")):
            w._action(lambda: w.plugin_priority.open(hovered_priority))
        imgui.end_child()
        selected = by_name.get(self.selected.casefold())
        if selected:
            imgui.text_unformatted(selected.name + ("  [locked]" if selected.name.casefold() in self.locked else ""))
            imgui.text_disabled("Requires: " + ("Not inspected" if selected.masters is None else ", ".join(selected.masters) or "No masters"))
            imgui.set_item_tooltip(f"Provider: {selected.provider}\n" + (selected.header or {}).get("description", ""))
        else:
            imgui.text_disabled(f"{len(self.active):,} active / {len(self.order):,} plugins. Drag to reorder; right-click for actions.")

    def move_plugin(self, name, destination):
        self.move_plugins({name}, destination)

    def move_plugins(self, selected, destination):
        from creation_lib.mod_manager.plugin_order import move
        candidate = move(self.catalog, self.order, selected, destination, self.locked.values())
        self.order, self.plan = candidate, None

    def data(self):
        w, m = self.workspace, self.manager
        if action_button("Refresh Data", enabled=bool(m.profile_id) and not m.busy and not m.runtime_active):
            w._action(m.inspect_files)
        if action_button("Read loose + archives", enabled=bool(m.profile_id) and not m.busy and not m.runtime_active):
            w._action(m.inspect_combined_data)
        imgui.set_item_tooltip("Background combined provenance. Archive and loose override winners are explicit profile predictions; unresolved cases are marked.")
        if m.combined_index:
            _, self.data_combined = imgui.checkbox("Include archive members", self.data_combined)
            imgui.same_line()
            _, self.data_overlaps = imgui.checkbox("Overlaps only", self.data_overlaps)
        combined = m.combined_index if self.data_combined else None
        if not m.file_index and not combined:
            imgui.text_wrapped("Index providers to browse this profile's virtual loose files.")
            return
        if action_button("Conflicts and providers"):
            w._tab = "Files"
        tree = combined.tree if combined else m.file_index.tree
        if tree is None:
            imgui.text_wrapped("Refresh Data to build this inventory's folder hierarchy.")
            return
        imgui.set_next_item_width(-1)
        _, self.data_query = imgui.input_text_with_hint("##data_filter", "Filter virtual file paths", self.data_query)
        entries = tree.visible(self.data_expanded, self.data_query)
        if combined and self.data_overlaps:
            key = id(entries)
            if self._data_overlap_key != key:
                self._data_overlap_key = key
                self._data_overlap_rows = [row for row in entries if row.entry is None or len(row.entry.active) > 1]
            entries = self._data_overlap_rows
        imgui.begin_child("data_files")
        clipper = imgui.ListClipper()
        clipper.begin(len(entries))
        while clipper.step():
            for index in range(clipper.display_start, clipper.display_end):
                row = entries[index]
                imgui.indent(scaled(16) * row.depth)
                imgui.push_id(("file/" if row.entry else "folder/") + row.path.casefold())
                if row.entry is None:
                    imgui.set_next_item_open(bool(self.data_query) or row.path.casefold() in self.data_expanded, imgui.Cond_.always)
                    opened = imgui.tree_node_ex(Path(row.path).name + "###data_folder", imgui.TreeNodeFlags_.no_tree_push_on_open | imgui.TreeNodeFlags_.span_avail_width)
                    if not self.data_query:
                        (self.data_expanded.add if opened else self.data_expanded.discard)(row.path.casefold())
                else:
                    entry = row.entry
                    if imgui.selectable(Path(entry.path).name + "###data_file", w._file_selected == entry.path)[0]:
                        w._file_selected = entry.path
                        if combined:
                            self._data_entry = entry
                            imgui.open_popup("combined_providers")
                        else:
                            w._tab = "Files"
                    imgui.set_item_tooltip(entry.path + "\n" + ("Predicted winner: " if combined else "Winner: ")
                                           + (entry.winner.name if entry.winner else "Unresolved / inactive") + f"\n{len(entry.providers)} providers"
                                           + ("\n" + entry.reason if combined else ""))
                    if combined and imgui.begin_popup("combined_providers"):
                        try:
                            imgui.text_wrapped(entry.path)
                            imgui.text_wrapped(entry.reason)
                            imgui.text_disabled("Metadata prediction; actual engine resolution unverified")
                            for candidate in entry.active:
                                imgui.push_id(candidate.kind + candidate.provider.path)
                                imgui.text_wrapped(candidate.name)
                                imgui.set_item_tooltip(candidate.provider.path)
                                if candidate.archive and imgui.menu_item("Show archive", "", False)[0]:
                                    self.archive_source, self.archive_query = candidate.archive.identity, entry.path
                                    w._right_pane = "Archives"
                                imgui.pop_id()
                            if entry.loose and imgui.menu_item("Inspect loose providers", "", False)[0]:
                                w._tab = "Files"
                        finally:
                            imgui.end_popup()
                imgui.pop_id()
                imgui.unindent(scaled(16) * row.depth)
        imgui.end_child()

    def archives(self):
        m, w = self.manager, self.workspace
        if action_button("Refresh archives", enabled=bool(m.profile_id) and not m.busy and not m.runtime_active):
            w._action(m.inspect_archives)
        index = m.archive_index
        if not index:
            imgui.text_wrapped("Refresh to read archive member inventories through modkit. Game Data, mod and owned overwrite containers are inspected in the background; no member is extracted.")
            return
        if self.archive_source and self.archive_source not in {archive.identity for archive in index.archives}:
            self.archive_source = ""
        imgui.text_disabled(f"{len(index.archives):,} containers | {len(index.members):,} member paths")
        imgui.text_wrapped("Profile load-plan preview: INI-listed archives and matching active-plugin candidates. Engine member precedence remains unverified.")
        imgui.set_next_item_width(-1)
        _, self.archive_query = imgui.input_text_with_hint("##archive_filter", "Filter archive members", self.archive_query)
        _, self.archive_conflicts = imgui.checkbox("Overlapping archive candidates", self.archive_conflicts)
        imgui.begin_child("archive_inventory", (0, max(scaled(60), imgui.get_content_region_avail().y * .3)))
        clipper = imgui.ListClipper()
        clipper.begin(len(index.archives) + 1)
        while clipper.step():
            for position in range(clipper.display_start, clipper.display_end):
                if not position:
                    if imgui.selectable("All archive members", not self.archive_source)[0]:
                        self.archive_source = ""
                    continue
                archive = index.archives[position - 1]
                imgui.push_id(str(archive.identity))
                count = f"{len(archive.members):,}" if archive.selected else "Not inspected"
                if imgui.selectable(f"{archive.name} | {archive.activation} | {count}###archive_container", self.archive_source == archive.identity)[0]:
                    self.archive_source = archive.identity
                imgui.set_item_tooltip(f"{archive.provider.name}\n{archive.reason}\n{archive.provider.path}")
                imgui.pop_id()
        imgui.end_child()
        selected = next((archive for archive in index.archives if archive.identity == self.archive_source), None)
        if selected:
            imgui.text_wrapped(selected.reason + " | Provider: " + selected.provider.name)
            if action_button("Reveal archive container"):
                from creation_lib.mod_manager.conflicts import reveal
                w._action(lambda: reveal(selected.provider))
        key = id(index), self.archive_query.casefold(), self.archive_source, self.archive_conflicts
        if key != self._archive_key:
            self._archive_key = key
            self._archive_members = [member for member in index.members if (not self.archive_query or key[1] in member.path.casefold())
                                     and (not self.archive_source or any(archive.identity == self.archive_source for archive in member.archives))
                                     and (not self.archive_conflicts or len(member.candidates) > 1)]
        member_selected = next((member for member in self._archive_members if member.path == self._archive_selected), None)
        imgui.begin_child("archive_members", (0, max(scaled(70), imgui.get_content_region_avail().y - scaled(240)) if member_selected else 0))
        clipper = imgui.ListClipper()
        clipper.begin(len(self._archive_members))
        while clipper.step():
            for position in range(clipper.display_start, clipper.display_end):
                member = self._archive_members[position]
                imgui.push_id(member.path.casefold())
                if imgui.selectable(f"{member.path} | {len(member.archives)} providers###archive_member", self._archive_selected == member.path)[0]:
                    self._archive_selected = member.path
                imgui.set_item_tooltip("\n".join(f"{archive.name} | {archive.provider.name} | {archive.activation}" for archive in member.archives))
                imgui.pop_id()
        imgui.end_child()

        if member_selected:
            self.archive_preview.draw(index, member_selected)

    def saves(self):
        m, w = self.manager, self.workspace
        if not m.library or not m.profile_id:
            imgui.text_wrapped("Choose a profile to browse its local saves.")
            return
        width = imgui.get_content_region_avail().x
        index = m.save_index
        external = bool(index and index.external)
        if action_button("Refresh source saves" if external else "Refresh owned saves", enabled=not m.busy and not m.runtime_active):
            w._action(lambda: m.inspect_source_saves(index.root)) if external else w._action(m.inspect_saves)
        if not index:
            if width >= imgui.get_item_rect_size().x + imgui.calc_text_size("Save locations").x + scaled(36) + imgui.get_style().item_spacing.x:
                imgui.same_line()
            self.save_backup_button()
            imgui.text_wrapped("Refresh to list this profile's owned saves in the background. Source MO2 and global saves are not included.")
            return
        label = "Inspect save header"
        if width >= imgui.get_item_rect_size().x + imgui.calc_text_size(label).x + scaled(36) + imgui.get_style().item_spacing.x:
            imgui.same_line()
        if action_button(label, enabled=index.game == "fallout4" and bool(self.save_selected) and not m.busy and not m.runtime_active):
            w._action(lambda: m.inspect_save_header(self.save_selected))
        imgui.set_item_tooltip("Read bounded FO4 basic display fields. Save-body validity and plugin compatibility remain unverified.")
        if action_button("Inspect saved plugins", enabled=index.game == "fallout4" and bool(self.save_selected) and not m.busy and not m.runtime_active):
            w._save_inspect_path, w._tab = self.save_selected, "Save plugins"
            w._action(lambda: m.inspect_save_plugins(self.save_selected))
        imgui.set_item_tooltip("Read bounded FO4 regular/light plugin tables, then compare with this profile or another displayed save.")
        if action_button("Import / back up source save" if external else "Copy / back up save", enabled=bool(self.save_selected) and not m.busy and not m.runtime_active):
            m.save_transfer_plan = None
            w._save_copy_path, w._save_copy_target, w._save_copy_ack = self.save_selected, m.profile_id if external else "", False
            w._tab = "Save copy"
        if width >= imgui.get_item_rect_size().x + imgui.calc_text_size("Save locations").x + scaled(36) + imgui.get_style().item_spacing.x:
            imgui.same_line()
        self.save_backup_button()
        imgui.set_next_item_width(-1)
        _, self.save_query = imgui.input_text_with_hint("##save_filter", f"{len(index.files):,} {'source' if external else 'owned'} saves | Filter names or characters", self.save_query)
        imgui.set_item_tooltip(index.root)
        query = self.save_query.casefold()
        files = [save for save in index.files if not query or query in (save.name + " " + m.save_details.get(save.path, {}).get("player", "")).casefold()]
        if self.save_selected not in {save.path for save in index.files}:
            self.save_selected = ""
        detail_height = imgui.get_text_line_height_with_spacing() * 8 if self.save_selected in m.save_details else 0
        imgui.begin_child("owned_saves", (0, max(scaled(40), imgui.get_content_region_avail().y - detail_height)))
        try:
            clipper = imgui.ListClipper()
            clipper.begin(len(files))
            while clipper.step():
                for position in range(clipper.display_start, clipper.display_end):
                    save = files[position]
                    imgui.push_id(save.path)
                    if imgui.selectable(save.name + "###owned_save", self.save_selected == save.path)[0]:
                        self.save_selected = save.path
                    imgui.set_item_tooltip(f"{save.size:,} bytes | Companion: {Path(save.companion).name if save.companion else 'None'}\n{save.path}")
                    imgui.pop_id()
        finally:
            imgui.end_child()
        data = m.save_details.get(self.save_selected)
        if data:
            imgui.text_wrapped(f"{data['player']} | Level {data['level']} | Save #{data['number']}\n{data['location']}\n{data['created']}")
            imgui.set_item_tooltip(f"Played: {data['played']}\nRace: {data['race']}\nHeader version: {data['header_version']}\n" + data["coverage"])
            imgui.text_disabled("Basic FO4 header; compatibility unverified.")

    def save_backup_button(self):
        m, w = self.manager, self.workspace
        if action_button("Save locations", enabled=not m.busy and not m.runtime_active):
            imgui.open_popup("save_locations")
        if imgui.begin_popup("save_locations"):
            try:
                if imgui.menu_item("Owned profile saves", "", False)[0]:
                    w._action(m.inspect_saves)
                if imgui.menu_item("Default global FO4 saves", "", False, enabled=bool(m.profile_id))[0]:
                    w._action(m.inspect_global_saves)
                if imgui.menu_item("Browse source/global folder...", "", False)[0]:
                    w._tab = "Browse saves"
                profile = next((item for item in m.profiles if item.id == m.profile_id), None)
                if imgui.menu_item("MO2 profile source saves", "", False, enabled=bool(profile and profile.source))[0]:
                    w._action(lambda: m.inspect_source_saves(str(Path(profile.source) / "saves")))
                if imgui.menu_item("Save backups", "", False)[0]:
                    m.save_transfer_plan = None
                    w._save_backup_ack = False
                    w._tab = "Save backups"
            finally:
                imgui.end_popup()

    def downloads(self):
        w, m = self.workspace, self.manager
        if action_button("Nexus connection and requests"):
            w._tab = "Nexus"
        w._draw_download_jobs()
        imgui.text_wrapped("Owned downloads")
        for record in m.download_records:
            imgui.push_id(record["game"] + str(record["mod_id"]) + ":" + str(record["file_id"]))
            imgui.text_wrapped(Path(record["path"]).name + "  " + record["version"])
            if action_button("Install download", enabled=bool(m.profile_id) and not m.busy and not m.runtime_active):
                w._action(lambda record=record: w.download_installer.open(record))
            imgui.pop_id()
        imported = m.instance_configuration.get("download_inventory", [])
        if imported:
            imgui.separator()
            imgui.text_disabled(f"{len(imported):,} original MO2 downloads (metadata references)")
            imgui.text_wrapped("Select an archive to review its installer and local destination. Original archives remain in MO2; metadata import never copies them.")
            imgui.begin_child("imported_downloads", (0, 0))
            clipper = imgui.ListClipper()
            clipper.begin(len(imported))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    record = imported[index]
                    imgui.push_id(record["source"])
                    if imgui.selectable("##imported_download", False)[0] and m.profile_id and not m.busy and not m.runtime_active:
                        w._action(lambda record=record: w.download_installer.open(record, imported=True))
                    imgui.get_window_draw_list().add_text(imgui.get_item_rect_min(), imgui.get_color_u32(imgui.Col_.text), record["name"])
                    imgui.set_item_tooltip(f"{record['bytes']:,} bytes\n{record['source']}")
                    imgui.pop_id()
            imgui.end_child()
