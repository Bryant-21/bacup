"""Local manager navigation and keyboard commands using the existing widgets."""
from imgui_bundle import imgui

from creation_lib.ui.widgets.modern import action_button
from .shortcuts import pressed, label as shortcut_label


def draw_navigation(workspace):
    m = workspace.manager
    if not m.library or not m.profiles:
        return
    allowed = not m.busy and not m.runtime_active
    for index, (label, hint, target) in enumerate((
        ("Install", "Open the installer (" + shortcut_label(workspace, "install_picker") + " opens the archive picker)", "Install"),
        ("Refresh files", "Refresh local loose-file and applicable archive conflicts in a worker (" + shortcut_label(workspace, "refresh") + ")", "Refresh"),
        ("Undo", "Undo the last local organization operation (" + shortcut_label(workspace, "mod_undo") + " in the mod tree)", "Undo"),
        ("Actions", "Profiles, executables, backups, export and recovery", "Actions"),
    )):
        if index:
            _next_button(label)
        if action_button(label, enabled=allowed):
            if target == "Refresh":
                workspace._action(lambda: m.refresh_conflicts(delay=0))
            elif target == "Undo":
                workspace._action(m.undo_organization)
            elif target == "Actions":
                imgui.open_popup("manager_toolbar_actions")
            else:
                workspace._tab = target
        imgui.set_item_tooltip(hint)
    _next_button("Add mod")
    if action_button("Add mod", enabled=allowed):
        imgui.open_popup("manager_add_mod")
    if imgui.begin_popup("manager_add_mod"):
        if imgui.menu_item("Create empty mod...", "", False)[0]:
            workspace.metadata.new_mod(m.selected_id)
        if imgui.menu_item("Install package...", "", False)[0]:
            workspace._tab = "Install"
        if imgui.menu_item("Import MO2 instance...", "", False)[0]:
            workspace._tab = "Import MO2"
        if imgui.menu_item("Create group...", "", False)[0]:
            workspace._tab = "New group"
        imgui.end_popup()
    _next_button("Filters")
    if action_button("Filters", primary=workspace._show_filters):
        workspace._show_filters = not workspace._show_filters
    imgui.separator()
    if imgui.begin_popup("manager_toolbar_actions"):
        try:
            selected = m.rows_by_id.get(m.selected_id)
            if imgui.menu_item("Open selected mod folder", shortcut_label(workspace, "mod_folder"), False, allowed and bool(selected and selected.kind != "separator"))[0]:
                workspace._reveal_mod_folder()
            if imgui.menu_item("Open library folder", "", False, allowed)[0]:
                from creation_lib.mod_manager.folder_actions import reveal_library
                workspace._action(lambda: reveal_library(m.library))
            if imgui.menu_item("Game on Nexus", shortcut_label(workspace, "nexus_game"), False, allowed)[0]:
                workspace._visit_nexus()
            removable = bool(selected and selected.kind in {"external", "separator"})
            if imgui.menu_item("Remove selected reference / separator", "", False, allowed and removable)[0]:
                workspace._action(workspace.metadata.remove_references)
            imgui.separator()
            for label, shortcut, target in (
                ("Manage profiles", shortcut_label(workspace, "profiles"), "Profiles"),
                ("Executables", shortcut_label(workspace, "executables"), "Executables"),
                ("Profile configuration", "", "Profile config"),
                ("Profile backups", "", "Profile backups"),
                ("Profile recipes", "", "Profile transfers"),
                ("Owned overwrite", "", "Overwrite"),
                ("Stage profile export", "", "Stage"),
                ("Recovery", "", "Recovery"),
            ):
                if imgui.menu_item(label, shortcut, False, allowed)[0]:
                    _destination(workspace, target)
        finally:
            imgui.end_popup()


def _next_button(label):
    """Wrap commands when the host is narrow; never put an action off screen."""
    style = imgui.get_style()
    right = imgui.get_window_pos().x + imgui.get_window_size().x - style.window_padding.x - style.scrollbar_size
    needed = imgui.calc_text_size(label).x + style.frame_padding.x * 2 + style.item_spacing.x
    if imgui.get_item_rect_max().x + needed <= right:
        imgui.same_line()


def _destination(workspace, target):
    if target == "Profiles":
        workspace._open_profiles = True
        workspace._tab = "Library"
    else:
        if target == "Executables":
            workspace.launcher.edit(workspace.launcher.selected())
        workspace._tab = target


def handle_navigation_keys(workspace):
    from .shortcuts import pressed
    m, io = workspace.manager, imgui.get_io()
    if not imgui.is_window_focused(imgui.FocusedFlags_.child_windows):
        return
    if workspace._dialog:
        dismiss_workflow(workspace)
        return
    if not m.library or not m.profile_id or m.busy or m.runtime_active:
        return
    if io.want_text_input:
        # Preserve the conventional search override, but never steal text-edit
        # chords such as a user-assigned Ctrl+A from an active input.
        if workspace._shortcuts.get("search") != "Ctrl+F" or not pressed(workspace, "search"):
            return
    if pressed(workspace, "search"):
        if workspace._right_pane == "Plugins" and workspace.panes.plugin_focused:
            workspace.panes._focus_plugin_filter = True
        else:
            workspace._focus_search = True
        workspace._tab = "Library"
        return
    if pressed(workspace, "refresh"):
        workspace._action(lambda: m.refresh_conflicts(delay=0))
        return
    for command, target in (("executables", "Executables"), ("profiles", "Profiles"), ("settings", "Settings")):
        if pressed(workspace, command):
            _destination(workspace, target)
            return
    if pressed(workspace, "nexus_game"):
        workspace._visit_nexus()
    elif pressed(workspace, "install_picker"):
        workspace._pick_package()


def dismiss_workflow(workspace):
    """Escape applies to this manager's workflow from either of its windows."""
    if (imgui.is_window_focused(imgui.FocusedFlags_.child_windows)
            and not imgui.get_io().want_text_input and pressed(workspace, "dismiss")):
        if workspace.manager.busy:
            workspace.manager.cancel()
        workspace.close_workflow()
        return True
    return False


def focus_search(workspace):
    if workspace._focus_search:
        imgui.set_keyboard_focus_here()
        workspace._focus_search = False


def handle_list_keys(workspace, nodes, closed):
    m, io = workspace.manager, imgui.get_io()
    focused = imgui.is_window_focused(imgui.FocusedFlags_.child_windows)
    if not focused and (pressed(workspace, "mod_collapse_group") or pressed(workspace, "mod_edit")):
        # Closing an editor or releasing Alt returns navigation to the Library root.
        nav = imgui.get_current_context().nav_window
        focused = bool(nav and nav.name == "Library##mod_manager")
    if not focused or io.want_text_input or m.busy:
        return
    selected = m.rows_by_id.get(m.selected_id)
    if selected and selected.group and pressed(workspace, "mod_collapse_group"):
        closed.add("group:" + selected.group)
        return
    down, up = imgui.is_key_pressed(imgui.Key.down_arrow), imgui.is_key_pressed(imgui.Key.up_arrow)
    if any(pressed(workspace, command) for command in ("mod_folder", "mod_select_all", "mod_undo", "mod_up", "mod_down")):
        if pressed(workspace, "mod_folder") and selected and selected.kind != "separator":
            workspace._reveal_mod_folder()
        elif pressed(workspace, "mod_select_all"):
            workspace._selected_mods = {node.row.id for node in nodes if node.row and node.row.kind != "separator"}
        elif pressed(workspace, "mod_undo") and not m.runtime_active:
            workspace._action(m.undo_organization)
        elif (pressed(workspace, "mod_down") or pressed(workspace, "mod_up")) and selected and not m.runtime_active:
            down = pressed(workspace, "mod_down")
            chosen = workspace._selected_mods if selected.id in workspace._selected_mods else {selected.id}
            ordered = sorted(m.rows, key=lambda row: (row.priority, row.id))
            positions = [index for index, row in enumerate(ordered) if row.id in chosen]
            if positions and (down and positions[-1] < len(ordered) - 1 or up and positions[0] > 0):
                target = positions[0] + (1 if down else -1)
                workspace._action(lambda: m.bulk_organize(tuple(chosen), priority=target))
                workspace._tree_scroll_selected = True
        return
    if not m.runtime_active and pressed(workspace, "mod_edit"):
        if workspace._hovered_group:
            workspace._action(lambda: workspace.metadata.rename_group(workspace._hovered_group))
        elif selected:
            mod_id, field = workspace._hovered_field or (selected.id, "name")
            workspace._action(lambda: workspace.metadata.edit_field(mod_id, field))
        return
    if selected and not m.runtime_active and pressed(workspace, "mod_remove"):
        workspace._action(workspace.metadata.remove)
        return
    if io.key_ctrl or io.key_alt or io.key_super:
        return
    if down or up:
        ordered = [node.row for node in nodes if node.row]
        if ordered:
            current = next((i for i, row in enumerate(ordered) if row.id == m.selected_id), -1)
            m.selected_id = ordered[max(0, min(len(ordered) - 1, current + (1 if down else -1)))].id
            workspace._tree_scroll_selected = True
    else:
        for letter in "abcdefghijklmnopqrstuvwxyz":
            if imgui.is_key_pressed(getattr(imgui.Key, letter), False):
                ordered = [node.row for node in nodes if node.row and node.row.kind != "separator"]
                current = next((i for i, row in enumerate(ordered) if row.id == m.selected_id), -1)
                following = ordered[current + 1:] + ordered[:current + 1]
                match = next((row for row in following if row.name.casefold().startswith(letter)), None)
                if match:
                    m.selected_id = workspace._selection_anchor = match.id
                    workspace._selected_mods = {match.id}
                    workspace._tree_scroll_selected = True
                break
    if selected and selected.group:
        if imgui.is_key_pressed(imgui.Key.left_arrow):
            closed.add("group:" + selected.group)
        elif imgui.is_key_pressed(imgui.Key.right_arrow):
            closed.discard("group:" + selected.group)


def handle_information_keys(workspace):
    io, m = imgui.get_io(), workspace.manager
    if (not imgui.is_window_focused(imgui.FocusedFlags_.child_windows) or io.want_text_input
            or io.key_ctrl or io.key_alt or io.key_super or m.busy):
        return
    up, down = imgui.is_key_pressed(imgui.Key.page_up, False), imgui.is_key_pressed(imgui.Key.page_down, False)
    if not up and not down:
        return
    rows = [row for row in m.filtered(workspace._query, workspace._group, workspace._enabled_only,
                                     sort_by=workspace._sort_by, descending=workspace._descending,
                                     origin=workspace._origin, category=workspace._category_filter) if row.kind != "separator"]
    if rows:
        current = next((index for index, row in enumerate(rows) if row.id == m.selected_id), -1)
        target = rows[max(0, min(len(rows) - 1, current + (1 if down else -1)))].id
        m.selected_id = workspace._selection_anchor = target
        workspace._selected_mods = {target}
        workspace._tree_scroll_selected = True
