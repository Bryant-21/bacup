"""Scoped plugin selection and draft actions; persistence still requires review."""
from imgui_bundle import imgui

from creation_lib.mod_manager.paths import ManagerError


def select(panes, name, visible, *, ctrl=False, shift=False):
    if shift and panes.selection_anchor in visible:
        start, end = sorted((visible.index(panes.selection_anchor), visible.index(name)))
        chosen = set(visible[start:end + 1])
        panes.selected_plugins = panes.selected_plugins | chosen if ctrl else chosen
    elif ctrl:
        if name in panes.selected_plugins:
            panes.selected_plugins.remove(name)
        else:
            panes.selected_plugins.add(name)
        panes.selection_anchor = name
    else:
        panes.selected_plugins = {name}
        panes.selection_anchor = name
    panes.selected = name


def toggle(panes, by_name):
    chosen = panes.selected_plugins if panes.selected in panes.selected_plugins else {panes.selected}
    mutable = {name.casefold() for name in chosen if name.casefold() in by_name and not by_name[name.casefold()].forced}
    if not mutable:
        return
    enable = panes.selected.casefold() not in panes.active
    if enable:
        panes.active.update(mutable)
    else:
        panes.active.difference_update(mutable)
    panes.plan = None


def provider_action(panes, *, nexus=False):
    m = panes.manager
    if m.busy or m.runtime_active:
        raise ManagerError("Wait for the current worker and runtime before opening this provider.")
    plugin = next((item for item in panes.catalog.plugins if item.name == panes.selected), None) if panes.catalog else None
    if plugin is None or not plugin.mod_id or plugin.mod_id not in m.rows_by_id:
        raise ManagerError("This plugin has no managed mod provider. Game files remain game context.")
    if nexus:
        from creation_lib.mod_manager.nexus_links import browse, mod_page
        browse(mod_page(m.library, m.profile_id, plugin.mod_id))
    else:
        from creation_lib.mod_manager.folder_actions import reveal_mod
        reveal_mod(m.library, m.profile_id, plugin.mod_id)


def handle(panes, visible, by_name):
    from .shortcuts import pressed
    m, io = panes.manager, imgui.get_io()
    if (not imgui.is_window_focused(imgui.FocusedFlags_.child_windows) or io.want_text_input
            or m.busy or m.runtime_active):
        return
    if any(pressed(panes.workspace, command) for command in ("plugin_select_all", "plugin_folder", "plugin_up", "plugin_down")):
        if pressed(panes.workspace, "plugin_select_all"):
            panes.selected_plugins = set(visible)
            if panes.selected not in visible and visible:
                panes.selected = panes.selection_anchor = visible[0]
            return True
        elif pressed(panes.workspace, "plugin_folder"):
            panes.workspace._action(lambda: provider_action(panes))
            return True
        elif panes.selected in panes.order:
            up, down = pressed(panes.workspace, "plugin_up"), pressed(panes.workspace, "plugin_down")
            if up or down:
                chosen = panes.selected_plugins if panes.selected in panes.selected_plugins else {panes.selected}
                positions = [index for index, name in enumerate(panes.order) if name in chosen]
                if positions and (up and positions[0] > 0 or down and positions[-1] < len(panes.order) - 1):
                    panes.workspace._action(lambda: panes.move_plugins(chosen, positions[0] + (1 if down else -1)))
                    panes._scroll_plugin = panes.selected
                return True
        return
    if pressed(panes.workspace, "plugin_toggle"):
        toggle(panes, by_name)
        return True
    if io.key_ctrl or io.key_alt or io.key_super:
        return
    down, up = imgui.is_key_pressed(imgui.Key.down_arrow, False), imgui.is_key_pressed(imgui.Key.up_arrow, False)
    if down or up:
        if visible:
            current = visible.index(panes.selected) if panes.selected in visible else -1
            name = visible[max(0, min(len(visible) - 1, current + (1 if down else -1)))]
            select(panes, name, visible, shift=io.key_shift)
            panes._scroll_plugin = name
        return True
    if not io.key_shift:
        for letter in "abcdefghijklmnopqrstuvwxyz":
            if imgui.is_key_pressed(getattr(imgui.Key, letter), False):
                current = visible.index(panes.selected) if panes.selected in visible else -1
                following = visible[current + 1:] + visible[:current + 1]
                match = next((name for name in following if name.casefold().startswith(letter)), None)
                if match:
                    select(panes, match, visible)
                    panes._scroll_plugin = match
                return True
