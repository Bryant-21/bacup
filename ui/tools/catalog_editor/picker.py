"""One shared modal record picker for the catalog editor."""

from __future__ import annotations

from collections.abc import Callable, Sequence

from imgui_bundle import imgui

from creation_lib.catalog.plugin_catalog import parse_form_key
from ui.tools.catalog_editor.forms import OFFICIAL_MASTERS

POPUP_ID = "Pick a record##catalog_editor"


class Picker:
    def __init__(self, forms) -> None:
        self.forms = forms
        self.landed: dict[str, str] = {}
        self._wants_open = False
        self._signatures: tuple[str, ...] = ()
        self._apply: Callable[[str], None] | None = None
        self._source = None
        self._text = ""
        self._key = ""
        self._error = ""
        self._results: list[tuple[str, str]] = []
        self._excluded: set[str] = set()  # casefolded plugin names the user unchecked; kept between picks

    def open(self, signatures: Sequence[str] | None, apply: Callable[[str], None], current: str = "",
             source=None) -> None:
        """`source` (anything with `search`) replaces the plugin's record search for this one pick."""
        self._wants_open = True
        self._signatures = tuple(signatures or ())
        self._source = source
        self._apply = apply
        self._key = current
        self._error = ""
        self._refresh()

    def request(self, slot: str, signatures: Sequence[str] | None, current: str = "") -> None:
        self.open(signatures, lambda picked: self.landed.__setitem__(slot, picked), current)

    def take(self, slot: str) -> str | None:
        return self.landed.pop(slot, None)

    def plugin_groups(self) -> list[tuple[str, list[str]]]:
        """Loaded plugins for the filter tree: the open plugin, its masters, Bethesda's, then the rest."""
        names = list(getattr(self.forms, "loaded_names", ()) or ())
        if not names:
            return []
        own = getattr(self.forms, "name", "")
        masters = {m.casefold() for m in getattr(self.forms, "masters", ())}
        official = {m.casefold() for m in OFFICIAL_MASTERS}
        groups: dict[str, list[str]] = {"Your plugin": [], "Masters": [], "Fallout 4 & DLC": [], "Other loaded": []}
        for name in names:
            folded = name.casefold()
            if folded == own.casefold():
                groups["Your plugin"].append(name)
            elif folded in official:
                groups["Fallout 4 & DLC"].append(name)
            elif folded in masters:
                groups["Masters"].append(name)
            else:
                groups["Other loaded"].append(name)
        return [(title, members) for title, members in groups.items() if members]

    def set_plugin_enabled(self, name: str, enabled: bool) -> None:
        if enabled:
            self._excluded.discard(name.casefold())
        else:
            self._excluded.add(name.casefold())
        self._refresh()

    def enabled_plugins(self) -> list[str] | None:
        """Plugin names to search, or None for all of them."""
        names = list(getattr(self.forms, "loaded_names", ()) or ())
        if not self._excluded or not names:
            return None
        return [name for name in names if name.casefold() not in self._excluded]

    def _refresh(self) -> None:
        source = self._source or self.forms
        if source is None:
            self._results = []
        elif self._source is None and (plugins := self.enabled_plugins()) is not None:
            self._results = source.search(self._text, self._signatures or None, 200, plugins=plugins)
        else:
            self._results = source.search(self._text, self._signatures or None, 200)

    def _draw_plugin_tree(self) -> None:
        imgui.text("Search in")
        for title, members in self.plugin_groups():
            all_on = all(name.casefold() not in self._excluded for name in members)
            imgui.set_next_item_open(True, imgui.Cond_.once)
            opened = imgui.tree_node_ex(f"##group_{title}", imgui.TreeNodeFlags_.allow_overlap)
            imgui.same_line()
            changed, value = imgui.checkbox(f"{title}##check_{title}", all_on)
            if changed:
                for name in members:
                    (self._excluded.discard if value else self._excluded.add)(name.casefold())
                self._refresh()
            if opened:
                for name in members:
                    changed, value = imgui.checkbox(f"{name}##plugin_{name}", name.casefold() not in self._excluded)
                    if changed:
                        self.set_plugin_enabled(name, value)
                imgui.tree_pop()

    def draw(self) -> None:
        if self._wants_open:
            imgui.open_popup(POPUP_ID)
            self._wants_open = False
        viewport = imgui.get_main_viewport().size
        imgui.set_next_window_size(imgui.ImVec2(min(1000, viewport.x * 0.9), min(640, viewport.y * 0.85)),
                                   imgui.Cond_.appearing)
        opened, _ = imgui.begin_popup_modal(POPUP_ID, None)
        if not opened:
            return
        imgui.text(f"Record type: {', '.join(self._signatures) or 'any'}")
        if self.forms is None:
            imgui.text_disabled("No plugin is open: type a FormKey below.")
        tree = self._source is None and bool(self.plugin_groups())
        if tree:
            if imgui.begin_child("##plugins", imgui.ImVec2(260, -80), imgui.ChildFlags_.borders):
                self._draw_plugin_tree()
            imgui.end_child()
            imgui.same_line()
        imgui.begin_group()
        imgui.set_next_item_width(-1)
        if imgui.is_window_appearing():
            imgui.set_keyboard_focus_here()
        changed, self._text = imgui.input_text_with_hint("##search", "Type part of an EditorID or name", self._text)
        if changed:
            self._refresh()
        if imgui.begin_child("##results", imgui.ImVec2(0, -80), imgui.ChildFlags_.borders):
            if not self._results and self.enabled_plugins() == []:
                imgui.text_disabled("Every plugin is unchecked on the left.")
            for key, label in self._results:
                if imgui.selectable(f"{label}##{key}", key == self._key, imgui.SelectableFlags_.allow_double_click)[0]:
                    self._key = key
                    if imgui.is_mouse_double_clicked(0):
                        self._finish(key)
        imgui.end_child()
        imgui.end_group()
        imgui.set_next_item_width(300)
        _, self._key = imgui.input_text_with_hint("##key", "or type Plugin.esp:000000", self._key)
        imgui.same_line()
        if imgui.button("Use"):
            self._use_typed()
        imgui.same_line()
        if imgui.button("Clear"):
            self._finish("")
        imgui.same_line()
        if imgui.button("Cancel"):
            self._apply = None
            imgui.close_current_popup()
        if self._error:
            imgui.text_colored(imgui.ImVec4(1.0, 0.4, 0.4, 1.0), self._error)
        imgui.end_popup()

    def _use_typed(self) -> None:
        text = self._key.strip()
        if parse_form_key(text) is None:
            self._error = "Not a FormKey: use Plugin.esp:HEX (for example SeventySix.esm:35ADBA)."
            return
        self._finish(text)

    def _finish(self, key: str) -> None:
        apply, self._apply = self._apply, None
        imgui.close_current_popup()
        if apply is not None:
            apply(key)
