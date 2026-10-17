"""Virtualized hierarchical mod list. No table or column widget is used here."""
from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import scaled, semantic_color

from creation_lib.mod_manager.status import badges
from creation_lib.mod_manager.mod_drag import begin as begin_drag, drop as drop_mods
from .status_icons import draw_badge
from creation_lib.mod_manager import columns as column_model
from .shortcuts import pressed, label as shortcut_label


def draw_tree(workspace, nodes, closed, reveal):
    if workspace._list_font_size:
        imgui.push_font(None, workspace._list_font_size)
    imgui.push_style_var(imgui.StyleVar_.frame_padding, (scaled(4), scaled(2)))
    imgui.push_style_var(imgui.StyleVar_.item_spacing, (scaled(4), scaled(2)))
    try:
        available = imgui.get_content_region_avail()
        layout = column_model.layout((available.x - imgui.get_style().scrollbar_size) / scaled(1), workspace._metadata_fields, workspace._columns)
        total = sum(column.width for column in layout) * scaled(1)
        imgui.set_next_window_content_size((total + imgui.get_style().scrollbar_size, 0))
        imgui.begin_child("mod_columns", (0, max(scaled(100), available.y - workspace._tree_footer)), window_flags=imgui.WindowFlags_.horizontal_scrollbar)
        try:
            _draw_tree(workspace, nodes, closed, reveal, layout)
        finally:
            imgui.end_child()
    finally:
        imgui.pop_style_var(2)
        if workspace._list_font_size:
            imgui.pop_font()


def metadata_layout(workspace, width):
    return [(column.key, column.label, scaled(column.width)) for column in
            column_model.layout(width / scaled(1), workspace._metadata_fields, workspace._columns)
            if column.key not in column_model.REQUIRED]


def draw_header(workspace, columns, start, width):
    from .columns_ui import header_gestures
    draw = imgui.get_window_draw_list()
    height = imgui.get_frame_height()
    workspace._column_header_rects = {}
    workspace._column_widths = {column.key: column.width for column in columns}
    for column in columns:
        key, label, size = column.key, column.label, scaled(column.width)
        offset = start.x + scaled(column.offset)
        imgui.set_cursor_screen_pos((offset, start.y))
        if key == "status" and imgui.calc_text_size(label).x + scaled(8) > size:
            label = "±"
        if imgui.selectable(label + "###sort_" + key, workspace._sort_by == key, size=(size - scaled(5), height))[0] and not workspace._column_drag:
            workspace._descending = not workspace._descending if workspace._sort_by == key else False
            workspace._sort_by = key
            workspace._drag_mod = None
        imgui.set_item_tooltip(("Combined measured conflict/update/account status" if key == "status" else label) + ": click to toggle visual sorting. Groups stay together; load priority is unchanged. Drag order requires ascending Priority.")
        if workspace._sort_by == key:
            x, y, radius = offset + size - scaled(7), start.y + height * .5, scaled(3)
            direction = 1 if workspace._descending else -1
            draw.add_triangle_filled((x, y + radius * direction), (x - radius, y - radius * direction), (x + radius, y - radius * direction), imgui.get_color_u32(semantic_color("accent")))
        rectangle = (offset, start.y, offset + size, start.y + height)
        workspace._column_header_rects[key] = rectangle
        imgui.push_id("column_" + key)
        header_gestures(workspace, key, rectangle)
        imgui.pop_id()
    imgui.set_cursor_screen_pos((start.x, start.y + height + scaled(4)))


def _draw_tree(workspace, nodes, closed, reveal, layout):
    m = workspace.manager
    workspace._hovered_field, workspace._field_rects = None, {}
    workspace._hovered_group = ""
    if imgui.get_drag_drop_payload_py_id() is None:
        workspace._drag_mod = None
        workspace._column_drag = None
    scale = scaled(1)
    spacing_y = imgui.get_style().item_spacing.y
    row_height = max(workspace._row_height, imgui.get_frame_height()) + spacing_y
    header_width = sum(column.width for column in layout) * scale
    pixels = tuple(column_model.Column(c.key, c.label, c.offset * scale, c.width * scale) for c in layout)
    geometry = {column.key: column for column in pixels}
    fields = [column for column in pixels if column.key not in column_model.REQUIRED]
    name_column = geometry["name"]
    header = imgui.get_cursor_screen_pos()
    draw_header(workspace, layout, header, header_width)
    imgui.begin_child("mod_hierarchy", (0, max(scaled(100), imgui.get_content_region_avail().y)), window_flags=imgui.WindowFlags_.always_vertical_scrollbar)
    window = imgui.internal.get_current_window()
    work_right = window.work_rect.max.x
    # Only the name TreeNode uses available widget width; annotations have
    # explicit screen geometry. Set its bound once for all clipped rows.
    window.work_rect.max.x = min(work_right, imgui.get_cursor_screen_pos().x + name_column.offset + name_column.width)
    try:
        draw = imgui.get_window_draw_list()
        icon_size, label_spacing = imgui.get_text_line_height(), imgui.get_tree_node_to_label_spacing()
        text_color, annotation_color = imgui.get_color_u32(imgui.Col_.text), imgui.get_color_u32(imgui.Col_.text_disabled)
        hover_fields = imgui.is_window_hovered()
        if workspace._tree_scroll_selected:
            position = next((i for i, node in enumerate(nodes) if node.row and node.row.id == m.selected_id), 0)
            imgui.set_scroll_y(max(0., position * row_height - imgui.get_content_region_avail().y * .5))
            workspace._tree_scroll_selected = False
        imgui.push_id(m.profile_id)
        clipper = imgui.ListClipper()
        clipper.begin(len(nodes), row_height)
        while clipper.step():
            for i in range(clipper.display_start, clipper.display_end):
                node = nodes[i]
                row, group = node.row, node.key.startswith("group:")
                imgui.push_id(row.id if row else node.key)
                start = imgui.get_cursor_screen_pos()
                imgui.set_cursor_screen_pos((start.x + geometry["enabled"].offset, start.y))
                imgui.begin_disabled(m.busy or m.runtime_active)
                if not group:
                    changed, enabled = imgui.checkbox("##enabled", row.enabled)
                    if changed:
                        workspace._action(lambda key=row.id, value=enabled: m.customize(key, enabled=value))
                imgui.end_disabled()
                imgui.set_cursor_screen_pos((start.x + geometry["priority"].offset, start.y))
                priority = f"{row.priority:>4}" if row else "    "
                imgui.text_disabled(priority)
                if row and not group:
                    if workspace._record_field_rects:
                        workspace._field_rects[(row.id, "priority")] = (imgui.get_item_rect_min(), imgui.get_item_rect_max())
                    if imgui.is_item_hovered():
                        workspace._hovered_field = (row.id, "priority")
                imgui.set_cursor_screen_pos((start.x + name_column.offset + 14 * scale * node.depth, start.y))
                cursor = imgui.get_cursor_screen_pos()
                flags = imgui.TreeNodeFlags_.no_tree_push_on_open | imgui.TreeNodeFlags_.span_avail_width | imgui.TreeNodeFlags_.open_on_arrow
                if not group:
                    flags |= imgui.TreeNodeFlags_.leaf
                if row and (row.id == m.selected_id or row.id in workspace._selected_mods):
                    flags |= imgui.TreeNodeFlags_.selected
                if group:
                    imgui.set_next_item_open(reveal or node.key not in closed, imgui.Cond_.always)
                # The name widget must stop before metadata so click/F2 selection cannot
                # accidentally land in a status/version annotation beside the name.
                opened = imgui.tree_node_ex("##group_node" if group else "##select_mod", flags)
                if group and imgui.is_item_hovered():
                    workspace._hovered_group = node.key.removeprefix("group:")
                if row and not group and imgui.is_item_hovered():
                    workspace._hovered_field = (row.id, "name")
                if not group and not m.busy and not m.runtime_active and not imgui.get_io().want_text_input and imgui.is_item_focused() and pressed(workspace, "mod_toggle"):
                    workspace._action(lambda key=row.id, value=row.enabled: m.customize(key, enabled=not value))
                if group and not reveal:
                    (closed.discard if opened else closed.add)(node.key)
                if group and imgui.is_item_clicked() and imgui.get_io().key_alt:
                    workspace._select_group_descendants(node.key.removeprefix("group:"))
                elif row and (imgui.is_item_clicked() and not imgui.is_item_toggled_open() or imgui.is_item_focused() and pressed(workspace, "mod_activate")):
                    m.selected_id = row.id
                    if imgui.get_io().key_shift and workspace._selection_anchor:
                        visible = [n.row.id for n in nodes if n.row and n.row.kind != "separator"]
                        if row.id in visible and workspace._selection_anchor in visible:
                            first, last = sorted((visible.index(row.id), visible.index(workspace._selection_anchor)))
                            workspace._selected_mods = set(visible[first:last + 1])
                    elif imgui.get_io().key_ctrl:
                        (workspace._selected_mods.discard if row.id in workspace._selected_mods else workspace._selected_mods.add)(row.id)
                    else:
                        if row.id in workspace._selected_mods and len(workspace._selected_mods) > 1:
                            workspace._tree_pending_select = row.id
                        else:
                            workspace._selected_mods = {row.id}
                            workspace._selection_anchor = row.id
                if row and not group and imgui.is_item_hovered() and imgui.is_mouse_double_clicked(0):
                    if imgui.get_io().key_ctrl:
                        workspace._reveal_mod_folder()
                    elif imgui.get_io().key_shift:
                        workspace._visit_nexus(mod=True)
                    else:
                        workspace._right_pane = "Information"
                caption = f"{node.name} ({node.count})" if group else node.name
                imgui.set_item_tooltip(f"{caption}\n{row.kind + ' | ' + (row.source or 'Unavailable') if row else 'Organization group'}" + ("\nMatching descendants are expanded while filtering." if reveal and group else ""))
                drag_allowed = workspace._sort_by == "priority" and not workspace._descending
                if row and not group and drag_allowed and not m.busy and not m.runtime_active and imgui.begin_drag_drop_source():
                    workspace._tree_pending_select = ""
                    if workspace._drag_mod is None:
                        workspace._action(lambda: setattr(workspace, "_drag_mod", begin_drag(workspace, row.id)))
                    if workspace._drag_mod:
                        imgui.set_drag_drop_payload_py_id("MODBOX_MOD", 1)
                        imgui.text_unformatted(f"Move {len(workspace._drag_mod.mod_ids)} selected entries")
                        imgui.text_disabled("Upper/lower half: before/after. Group: organize.")
                    imgui.end_drag_drop_source()
                if drag_allowed and not m.busy and not m.runtime_active and imgui.begin_drag_drop_target():
                    rectangle = (imgui.get_item_rect_min(), imgui.get_item_rect_max())
                    after = imgui.get_io().mouse_pos.y >= (rectangle[0].y + rectangle[1].y) / 2
                    if workspace._drag_mod:
                        y = rectangle[1].y if after else rectangle[0].y
                        imgui.get_window_draw_list().add_line((rectangle[0].x, y), (rectangle[1].x, y), imgui.get_color_u32(semantic_color("accent")), 2)
                    payload = imgui.accept_drag_drop_payload_py_id("MODBOX_MOD")
                    if payload and workspace._drag_mod:
                        if group:
                            workspace._action(lambda: drop_mods(workspace, workspace._drag_mod, group=node.key.removeprefix("group:")))
                        else:
                            workspace._action(lambda: drop_mods(workspace, workspace._drag_mod, target=row.id, after=after))
                    imgui.end_drag_drop_target()
                if group and imgui.begin_popup_context_item("group_actions"):
                    allowed = not m.busy and not m.runtime_active
                    path = node.key.removeprefix("group:")
                    if row and imgui.menu_item("Create empty mod below...", "", False, allowed)[0]:
                        workspace.metadata.new_mod(row.id)
                    if imgui.menu_item("Rename local group", shortcut_label(workspace, "mod_edit"), False, allowed)[0]:
                        workspace._action(lambda: workspace.metadata.rename_group(path))
                    if imgui.menu_item("Select group descendants", "Alt+click", False)[0]:
                        workspace._select_group_descendants(path)
                    if row and row.kind == "separator" and imgui.menu_item("Remove separator from profile", "", False, allowed)[0]:
                        workspace._action(lambda: workspace.metadata.remove_references((row.id,)))
                    if imgui.menu_item("Undo organization", shortcut_label(workspace, "mod_undo"), False, allowed)[0]:
                        workspace._action(m.undo_organization)
                    imgui.end_popup()
                if row and not group and imgui.begin_popup_context_item("mod_actions"):
                    m.selected_id = row.id
                    allowed = not m.busy and not m.runtime_active
                    selected = tuple(workspace._selected_mods) if row.id in workspace._selected_mods else (row.id,)
                    imgui.text_disabled(f"{len(selected)} selected")
                    if imgui.menu_item("Create empty mod below...", "", False, allowed)[0]:
                        workspace.metadata.new_mod(row.id)
                    imgui.separator()
                    if imgui.menu_item("Information", "", False)[0]:
                        workspace._right_pane = "Information"
                    if not group and imgui.menu_item("Open mod folder", shortcut_label(workspace, "mod_folder"), False, allowed)[0]:
                        workspace._reveal_mod_folder()
                    if not group and imgui.menu_item("Rename display title", shortcut_label(workspace, "mod_edit"), False, allowed)[0]:
                        workspace._action(workspace.metadata.rename)
                    if imgui.menu_item("Relink source folder...", "", False, allowed and row.kind == "external")[0]:
                        workspace._action(workspace.sources.open)
                    if imgui.menu_item("Undo source relink", "", False, allowed and row.id in m.reference_undo)[0]:
                        workspace._action(lambda: m.undo_source_relink(row.id))
                    if imgui.menu_item("Refresh source metadata...", "", False, allowed and row.kind == "external")[0]:
                        workspace._action(workspace.source_metadata.open)
                    if imgui.menu_item("Undo metadata refresh", "", False, allowed and row.id in m.source_metadata_undo)[0]:
                        workspace._action(lambda: m.undo_source_metadata(row.id))
                    if not group and imgui.menu_item("Remove owned mod", shortcut_label(workspace, "mod_remove"), False, allowed and row.kind == "owned")[0]:
                        workspace._action(workspace.metadata.remove)
                    removable = allowed and all(m.rows_by_id.get(identifier) and m.rows_by_id[identifier].kind in {"external", "separator"} for identifier in selected)
                    if imgui.menu_item("Remove references / separators from profile", "", False, removable)[0]:
                        workspace._action(lambda: workspace.metadata.remove_references(selected))
                    if not group and imgui.menu_item("Visit on Nexus", "Shift+double-click", False, allowed and row.nexus_mod_id > 0)[0]:
                        workspace._visit_nexus(mod=True)
                    if not group and imgui.menu_item("Disable" if row.enabled else "Enable", shortcut_label(workspace, "mod_toggle"), False, allowed)[0]:
                        workspace._action(lambda: m.bulk_organize(selected, enabled=not row.enabled))
                    for label, priority in (("Move to first priority", 0), ("Move to last priority", len(m.rows) - 1)):
                        if imgui.menu_item(label, "", False, allowed)[0]:
                            workspace._action(lambda p=priority: m.bulk_organize(selected, priority=p))
                    if imgui.menu_item("Undo organization", shortcut_label(workspace, "mod_undo"), False, allowed)[0]:
                        workspace._action(m.undo_organization)
                    if imgui.menu_item("Inspect files", "", False, allowed and not group)[0]:
                        workspace._file_mod, workspace._tab = row.id, "Files"
                        workspace._action(lambda: m.inspect_files(workspace._include_disabled_files))
                    imgui.end_popup()
                right = start.x + name_column.offset + name_column.width
                draw.push_clip_rect((cursor.x, start.y), (right, start.y + row_height), True)
                draw.add_text((cursor.x + label_spacing, cursor.y), text_color, caption)
                draw.pop_clip_rect()
                if not group:
                    for column in fields:
                        name, size = column.key, column.width
                        offset = start.x + column.offset
                        if hover_fields or workspace._record_field_rects:
                            rect = ((offset, start.y), (offset + size, start.y + row_height))
                            if workspace._record_field_rects:
                                workspace._field_rects[(row.id, name)] = rect
                            if hover_fields and imgui.is_mouse_hovering_rect(*rect):
                                workspace._hovered_field = (row.id, name)
                        draw.push_clip_rect((offset, start.y), (offset + size - 3 * scale, start.y + row_height), True)
                        position = offset + 6 * scale
                        if name == "status":
                            for badge in badges(row, m.conflict_index, workspace._file_states, m.updates, m.nexus_states, m.conflict_scan_state):
                                if position + icon_size > offset + size:
                                    break
                                draw_badge(draw, badge, position, cursor.y, icon_size)
                                if hover_fields and imgui.is_mouse_hovering_rect((position, start.y), (position + icon_size, start.y + row_height)):
                                    imgui.set_tooltip(badge.description)
                                position += icon_size + 3 * scale
                        else:
                            text = str(row.nexus_mod_id or "-") if name == "nexus" else getattr(row, name) or "-"
                            draw.add_text((position, cursor.y), annotation_color, text)
                        draw.pop_clip_rect()
                imgui.set_cursor_screen_pos((start.x, start.y + row_height - spacing_y))
                imgui.dummy((0, 0))
                imgui.pop_id()
        if workspace._tree_pending_select and imgui.is_mouse_released(0):
            if imgui.get_drag_drop_payload_py_id() is None:
                workspace._selected_mods = {workspace._tree_pending_select}
                workspace._selection_anchor = workspace._tree_pending_select
            workspace._tree_pending_select = ""
        imgui.pop_id()
        if imgui.begin_popup_context_window("library_empty_actions", imgui.PopupFlags_.mouse_button_right | imgui.PopupFlags_.no_open_over_items):
            allowed = not m.busy and not m.runtime_active
            if imgui.menu_item("Create empty mod...", "", False, allowed)[0]:
                workspace.metadata.new_mod()
            if imgui.menu_item("Install package...", "", False, allowed)[0]:
                workspace._tab = "Install"
            if imgui.menu_item("Create group...", "", False, allowed)[0]:
                workspace._tab = "New group"
            imgui.end_popup()
    finally:
        window.work_rect.max.x = work_right
        imgui.end_child()
