"""Tales Catalogs — author a mod's per-plugin Tales feature catalogs (<Feature>/<Plugin>.json).

Open a plugin, pick a feature, and see every entry the plugin's masters already define merged with the
plugin's own. Override or remove an inherited entry, add new ones, then save only the plugin's own layer.
"""

from __future__ import annotations

import copy
import json
import logging
from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path
from typing import Callable

from imgui_bundle import imgui

from creation_lib.ui.widgets import Drawer, pick_file, pick_folder
from creation_lib.ui.widgets.modern import action_button, scaled, semantic_color
from ui.tools.base import BaseTool
from ui.tools.catalog_editor.assets import AssetBrowser, AssetSource
from ui.tools.catalog_editor.jsonview import JsonViewer
from ui.tools.catalog_editor.workbench import NARROW_WIDTH, Workbench

_log = logging.getLogger("tools.catalog_editor")

_PLUGINS = [("Plugins", "*.esm *.esp *.esl")]
SETTINGS_TAB = "Settings"
_GUARD_POPUP = "Unsaved changes##catalog_editor_guard"
_MASTERS_POPUP = "Masters##catalog_editor_masters"
DEFAULT_PANES = {"browser": 0.2, "inspector": 0.38}
_MIN_PANE = {"browser": 0.1, "inspector": 0.2}
_MIN_TABLE = 0.2
_SPLITTER_WIDTH = 6


def _feature_name(feature) -> str:
    return feature if isinstance(feature, str) else feature.name


def extension_for(feature: str):
    from ui.tools.catalog_editor import features

    return features.extension_for(feature)


def _form_source(path, session, data_dir: str, settings):
    """Record search over the plugin and its masters; without it the editor still works but the picker is empty."""
    from ui.tools.catalog_editor.forms import PluginFormSource

    try:
        master_dirs = list(dict.fromkeys(Path(m.path).parent for m in session.masters if m.path is not None))
        source = PluginFormSource.open(path, data_dir=data_dir or None, master_dirs=master_dirs, toolkit_settings=settings)
        # Masters load lazily and the first lookup in each takes seconds; pay that here, off the UI thread.
        source.prefetch([f"{name}:000800" for name in source.loaded_names])
        for feature in session.features():
            extension = extension_for(feature)
            if extension is not None:
                extension.warm(source)
        return source
    except Exception:
        _log.exception("record search unavailable for %s", path)
        return None


class CatalogEditorTool(BaseTool):
    name = "Tales Catalogs"
    tool_id = "catalog_editor"
    description = (
        "Create a mod's Tales feature catalogs (F4SE/Plugins/B21_TalesFromAppalachia/<Feature>/<Plugin>.json). "
        "Entries from the plugin's masters show merged with your own; override, remove or add, and save "
        "only your plugin's layer."
    )
    category = "Mod Tools"

    def __init__(self):
        super().__init__()
        self._toolkit_settings = None
        self.bacup_output_dir_provider: Callable[[], str] | None = None
        self._pool = ThreadPoolExecutor(max_workers=1, thread_name_prefix="catalog_editor")
        self._loading: Future | None = None
        self._forms_loading: Future | None = None
        self._session = None
        self._picker = None
        self._documents: dict = {}
        self._last_plugin = ""
        self._extra_master_dirs: list[str] = []
        self._feature = ""
        self._section = ""
        self._selected_key: str | None = None
        self._new_key = ""
        self._problems: list = []
        self._guarded_action: Callable[[], None] | None = None
        self._open_guard = False
        self._force_tab = True
        self._assets: AssetBrowser | None = None
        self._json = JsonViewer()
        self._about = Drawer("catalog_about", side="left")
        self._panes = dict(DEFAULT_PANES)
        self._popped_out = False
        self._workbench = Workbench(self)

    # -- settings --

    def get_default_settings(self) -> dict:
        return {"last_plugin": "", "extra_master_dirs": [], "panes": dict(DEFAULT_PANES), "editor_popped_out": False}

    def apply_settings(self, settings: dict) -> None:
        self._last_plugin = settings.get("last_plugin", "")
        self._extra_master_dirs = list(settings.get("extra_master_dirs", []))
        saved = settings.get("panes") or {}
        self._panes = {name: float(saved.get(name, default)) for name, default in DEFAULT_PANES.items()}
        self._clamp_panes()
        self._popped_out = bool(settings.get("editor_popped_out", False))

    def collect_settings(self) -> dict:
        return {"last_plugin": self._last_plugin, "extra_master_dirs": list(self._extra_master_dirs),
                "panes": dict(self._panes), "editor_popped_out": self._popped_out}

    def cleanup(self) -> None:
        self._pool.shutdown(wait=False, cancel_futures=True)
        if self._assets is not None:
            self._assets.source.close()
        super().cleanup()

    def _data_dir(self) -> str:
        """The FO4 Data folder from Settings -> Paths, where masters and the DLCs are found."""
        if self._toolkit_settings is None:
            return ""
        root = self._toolkit_settings.get_game_paths("fo4").get("root_dir", "")
        return str(Path(root) / "Data") if root and (Path(root) / "Data").is_dir() else ""

    def _bacup_output_dir(self) -> str:
        return self.bacup_output_dir_provider() if self.bacup_output_dir_provider else ""

    def _asset_roots(self) -> list[str]:
        """Folders searched for loose assets and BA2s: the plugin's, its masters', Data and the BACUP output."""
        plugin_path = getattr(self._session, "plugin_path", None)
        roots = [str(Path(plugin_path).parent)] if plugin_path else []
        if self._session is not None:
            roots += [str(Path(m.path).parent) for m in self._session.masters if m.path is not None]
        roots += [self._data_dir(), self._bacup_output_dir(), *self._extra_master_dirs]
        return [root for root in roots if root]

    def _refresh_assets(self) -> None:
        roots = self._asset_roots()
        if self._assets is None:
            self._assets = AssetBrowser(AssetSource(roots))
        else:
            self._assets.source.set_roots(roots)

    # -- panes --

    def _clamp_panes(self) -> None:
        self._panes["browser"] = max(_MIN_PANE["browser"], self._panes["browser"])
        self._panes["inspector"] = max(_MIN_PANE["inspector"], self._panes["inspector"])
        over = self._panes["browser"] + self._panes["inspector"] - (1 - _MIN_TABLE)
        if over > 0:
            self._panes["inspector"] = max(_MIN_PANE["inspector"], self._panes["inspector"] - over)
            self._panes["browser"] = max(_MIN_PANE["browser"], 1 - _MIN_TABLE - self._panes["inspector"])

    def drag_splitter(self, which: str, delta_px: float, usable_px: float) -> None:
        """`browser`: the bar between entry browser and table; `inspector`: between table and inspector."""
        if usable_px <= 0:
            return
        fraction = delta_px / usable_px
        if which == "browser":
            self._panes["browser"] += fraction
        else:
            self._panes["inspector"] -= fraction
        self._clamp_panes()

    def _splitter(self, which: str, height: float, usable: float) -> None:
        imgui.same_line()
        start = imgui.get_cursor_screen_pos()
        imgui.invisible_button(f"##split_{which}", imgui.ImVec2(scaled(_SPLITTER_WIDTH), height))
        active = imgui.is_item_active()
        if active or imgui.is_item_hovered():
            imgui.set_mouse_cursor(imgui.MouseCursor_.resize_ew)
        color = imgui.get_color_u32(imgui.Col_.separator_active if active else imgui.Col_.separator)
        mid = start.x + scaled(_SPLITTER_WIDTH) / 2
        imgui.get_window_draw_list().add_line(imgui.ImVec2(mid, start.y), imgui.ImVec2(mid, start.y + height), color, 2.0)
        if active:
            self.drag_splitter(which, imgui.get_io().mouse_delta.x, usable)
        imgui.same_line()

    # -- session --

    def has_unsaved_changes(self) -> bool:
        return self._session is not None and bool(self._session.dirty_features())

    def _document(self, name: str):
        if name not in self._documents:
            self._documents[name] = self._session.feature(name)
        return self._documents[name]

    def _current_document(self):
        return self._document(self._feature) if self._session is not None and self._feature else None

    def _guarded(self, action: Callable[[], None]) -> None:
        """Run `action` now, or after the author decides about unsaved changes."""
        if self.has_unsaved_changes():
            self._guarded_action = action
            self._open_guard = True
        else:
            action()

    def request_open(self) -> None:
        self._guarded(self._choose_plugin)

    def _choose_plugin(self) -> None:
        default = str(Path(self._last_plugin).parent) if self._last_plugin else ""
        path = pick_file("Open plugin", _PLUGINS, default_path=default)
        if path:
            self.open_plugin(path)

    def open_plugin(self, path: str) -> None:
        from ui.tools.catalog_editor.model import CatalogSession

        data_dir, bacup_dir = self._data_dir(), self._bacup_output_dir()
        extra = tuple(self._extra_master_dirs)

        settings = self._toolkit_settings

        def load():
            session = CatalogSession.open(
                path, data_dir=data_dir or None, bacup_output_dir=bacup_dir or None, extra_master_dirs=extra,
            )
            session.forms = _form_source(path, session, data_dir, settings)
            return session

        self._last_plugin = path
        self._error_msg = self._result_msg = ""
        self._status_msg = f"Loading {Path(path).name} and its masters..."
        self._loading = self._pool.submit(load)

    def _poll(self) -> None:
        if self._forms_loading is not None and self._forms_loading.done():
            future, self._forms_loading = self._forms_loading, None
            if self._session is not None and future.exception() is None:
                self._session.forms = future.result()
                if self._picker is not None:
                    self._picker.forms = self._session.forms
        if self._loading is None or not self._loading.done():
            return
        future, self._loading = self._loading, None
        try:
            session = future.result()
        except Exception as error:
            _log.exception("catalog session failed to open")
            self._error_msg = f"Could not open the plugin: {error}"
            return
        from ui.tools.catalog_editor.picker import Picker

        self._session = session
        self._picker = Picker(session.forms)
        self._refresh_assets()
        self._documents.clear()
        self._problems = []
        names = [_feature_name(feature) for feature in session.features()]
        self._feature = names[0] if names else ""
        self._select_section("")
        missing = list(session.missing_masters)
        self._result_msg = f"{session.plugin.name} opened." if hasattr(session.plugin, "name") else "Plugin opened."
        if missing:
            self._result_msg += f" {len(missing)} master(s) not found; use Masters -> Add folder."

    def _select_feature(self, name: str) -> None:
        self._feature = name
        self._select_section("")

    def _select_section(self, section: str) -> None:
        self._section = section
        self._selected_key = None
        self._new_key = ""
        self._force_tab = True

    def add_master_folder(self, path: str) -> None:
        if not path or self._session is None:
            return
        self._session.add_master_folder(path)
        if path not in self._extra_master_dirs:
            self._extra_master_dirs.append(path)
        self._refresh_assets()
        session, plugin, data_dir, settings = self._session, self._last_plugin, self._data_dir(), self._toolkit_settings
        self._forms_loading = self._pool.submit(_form_source, plugin, session, data_dir, settings)

    def load_plugin_for_picking(self, path: str) -> None:
        if not path or not hasattr(self.forms, "load_more"):
            return
        try:
            name = self.forms.load_more(path)
        except Exception as error:
            self.notify(f"Could not load {Path(path).name}: {error}", error=True)
            return
        self.notify(f"{name} loaded: its records can be picked now.")

    # -- actions (no imgui) --

    def override_row(self, section: str, row) -> None:
        doc = self._current_document()
        kind = doc.section_kind(section)
        doc.set_entry(section, copy.deepcopy(row.entry), map_key=row.key if kind == "map" else None)
        self._selected_key = row.key

    def remove_row(self, section: str, row) -> None:
        self._current_document().remove(section, row.key)
        if self._selected_key == row.key and row.status == "added":
            self._selected_key = None

    def revert_row(self, section: str, row) -> None:
        self._current_document().revert(section, row.key)

    def new_entry(self, section: str, map_key: str | None = None) -> str | None:
        from ui.tools.catalog_editor import render

        doc = self._current_document()
        if doc.section_kind(section) == "map" and not (map_key or "").strip():
            self._error_msg = "Type a name for the new entry first."
            return None
        value = render.default_value(doc.entry_schema(section), doc.schema)
        try:
            key = doc.set_entry(section, value, map_key=(map_key or "").strip() or None)
        except ValueError as error:
            self._error_msg = f"Could not add an entry: {error}"
            return None
        self._selected_key = key
        return key

    def edit_entry(self, section: str, row, new_value) -> None:
        doc = self._current_document()
        kind = doc.section_kind(section)
        key = doc.set_entry(section, new_value, map_key=row.key if kind == "map" else None)
        if key != row.key:
            if row.status == "added":
                doc.remove(section, row.key)
            self._selected_key = key

    @property
    def forms(self):
        return self._session.forms if self._session is not None else None

    def select_row(self, key: str) -> None:
        self._selected_key = key

    def notify(self, message: str, error: bool = False) -> None:
        self._error_msg, self._result_msg = (message, "") if error else ("", message)

    def pick_record(self, signatures, apply: Callable[[str], None], current: str = "", source=None) -> None:
        if self._picker is not None:
            self._picker.open(signatures, apply, current, source)

    def _problems_of(self, doc) -> list:
        """The document's own problems plus its feature extension's (which may be non-blocking warnings)."""
        problems = list(doc.validate(self._session.forms))
        extension = extension_for(doc.name)
        if extension is not None:
            problems += extension.problems(doc, self._session.forms)
        return problems

    def validate(self) -> list:
        """Validate the unsaved features (else the open one) and keep the problems for the panel."""
        if self._session is None:
            return []
        docs = [self._document(_feature_name(f)) for f in self._session.features()]
        targets = [doc for doc in docs if doc.dirty] or [self._current_document()]
        self._problems = [problem for doc in targets if doc is not None for problem in self._problems_of(doc)]
        blocking = sum(getattr(problem, "blocking", True) for problem in self._problems)
        if blocking:
            self._error_msg = f"{blocking} problem(s) found."
        else:
            self._error_msg = ""
            warnings = len(self._problems)
            self._result_msg = f"No problems found; {warnings} warning(s)." if warnings else "No problems found."
        return self._problems

    def save(self, everything: bool) -> list[Path]:
        """Validate, then write the open feature (or every changed one). Nothing is written with problems."""
        if self._session is None:
            return []
        if everything:
            docs = [self._document(_feature_name(f)) for f in self._session.features()]
            targets = [doc for doc in docs if doc.dirty]
        else:
            doc = self._current_document()
            targets = [doc] if doc is not None and doc.dirty else []
        if not targets:
            self._result_msg = "Nothing to save."
            return []
        problems = [problem for doc in targets for problem in self._problems_of(doc)]
        self._problems = problems
        blocking = sum(getattr(problem, "blocking", True) for problem in problems)
        if blocking:
            self._error_msg = f"Not saved: {blocking} problem(s) to fix first."
            return []
        saved = []
        for doc in targets:
            try:
                saved.append(doc.save())
            except Exception as error:
                self._error_msg = f"Could not save {doc.name}: {error}"
                return saved
        self._error_msg = ""
        self._result_msg = "Saved " + ", ".join(path.name for path in saved) if saved else "Nothing to save."
        return saved

    def goto_problem(self, problem) -> None:
        self._select_feature(problem.feature)
        self._select_section(problem.section or "")
        self._selected_key = problem.key

    # -- layout --

    def draw_window(self) -> None:
        if not self.visible:
            return
        imgui.set_next_window_size(imgui.ImVec2(scaled(1500), scaled(860)), imgui.Cond_.first_use_ever)
        expanded, self.visible = imgui.begin(f"{self.name}###tool_{self.tool_id}", True)
        if expanded:
            self.draw_content()
            if self._error_msg:
                imgui.text_colored(semantic_color("error"), self._error_msg)
            elif self._result_msg:
                imgui.text_colored(semantic_color("success"), self._result_msg)
        imgui.end()

    def draw_content(self) -> None:
        self._poll()
        self._draw_top_bar()
        imgui.separator()
        if self._session is None:
            if self._loading is None:
                imgui.text_wrapped("Open your plugin to edit its Tales catalogs. Entries from its masters are shown "
                                   "merged with yours; only your plugin's own layer is saved.")
            self._draw_guard()
            return
        doc = self._current_document()
        if doc is not None:
            self._draw_sections(doc)
            if self._section == SETTINGS_TAB:
                self._draw_settings(doc)
            elif doc.section_kind(self._section) == "opaque":
                imgui.text_disabled("This section has no editable shape here; shown read-only.")
                imgui.text_wrapped(json.dumps([row.entry for row in doc.rows(self._section)], indent=2, default=str))
            else:
                self._draw_workbench(doc, self._section)
        self._draw_inspector_window()
        if self._picker is not None:
            self._picker.draw()
        if self._assets is not None:
            self._assets.draw()
        self._json.draw()
        self._workbench.draw_popups()
        self._about.draw()
        self._draw_guard()

    def _draw_workbench(self, doc, section: str) -> None:
        workbench = self._workbench
        self._draw_toolbar(doc, section)
        avail = imgui.get_content_region_avail()
        line = imgui.get_frame_height_with_spacing()
        problems_height = scaled(130) if self._problems else 0
        height = max(scaled(120), avail.y - line - problems_height)
        narrow = avail.x < scaled(NARROW_WIDTH)
        spacing = imgui.get_style().item_spacing.x
        docked = not self._popped_out
        bars = (0 if narrow else 1) + (1 if docked else 0)
        usable = max(1.0, avail.x - bars * (scaled(_SPLITTER_WIDTH) + 2 * spacing))
        left = 0.0 if narrow else usable * self._panes["browser"]
        right = usable * self._panes["inspector"] if docked else 0.0
        if narrow:
            workbench.draw_browser_combo(doc, section)
            height -= line
        else:
            if imgui.begin_child("##catalog_browser", imgui.ImVec2(left, height), imgui.ChildFlags_.borders):
                workbench.draw_browser(doc, section)
            imgui.end_child()
            self._splitter("browser", height, usable)
        middle = max(1.0, usable - left - right) if docked else imgui.get_content_region_avail().x
        if imgui.begin_child("##catalog_centre", imgui.ImVec2(middle, height), imgui.ChildFlags_.borders):
            workbench.draw_centre(doc, section)
        imgui.end_child()
        if docked:
            self._splitter("inspector", height, usable)
            if imgui.begin_child("##catalog_inspector", imgui.ImVec2(0, height), imgui.ChildFlags_.borders):
                workbench.draw_inspector(doc, section)
            imgui.end_child()
        workbench.draw_status_bar(doc, section)
        if self._problems:
            if imgui.begin_child("##catalog_problems", imgui.ImVec2(0, problems_height), imgui.ChildFlags_.borders):
                self._draw_problems()
            imgui.end_child()

    def _draw_inspector_window(self) -> None:
        """The inspector in its own resizable, dockable window; closing it docks the inspector back."""
        if not self._popped_out:
            return
        imgui.set_next_window_size(imgui.ImVec2(scaled(720), scaled(760)), imgui.Cond_.first_use_ever)
        expanded, keep_open = imgui.begin("Inspector###catalog_editor_popout", True)
        doc = self._current_document()
        if expanded and doc is not None and self._section not in ("", SETTINGS_TAB) \
                and doc.section_kind(self._section) != "opaque":
            self._workbench.draw_inspector(doc, self._section)
        imgui.end()
        if not keep_open:
            self._popped_out = False

    def _draw_top_bar(self) -> None:
        if action_button("Open plugin..."):
            self.request_open()
        imgui.same_line()
        loaded = self._session is not None
        imgui.text(Path(self._last_plugin).name if self._last_plugin else "no plugin")
        if self._loading is not None:
            imgui.same_line()
            imgui.text_disabled(self._status_msg)
        if not loaded:
            return
        imgui.same_line()
        missing = len(self._session.missing_masters)
        if imgui.button(f"Masters ({len(self._session.masters) - missing}/{len(self._session.masters)})##catalog_masters"):
            imgui.open_popup(_MASTERS_POPUP)
        if missing:
            imgui.same_line()
            imgui.text_colored(semantic_color("warning"), f"{missing} missing")
        self._draw_masters_popup()
        imgui.same_line()
        self._draw_feature_combo()
        self._draw_about_button()
        imgui.same_line()
        if imgui.button("Validate"):
            self.validate()
        imgui.same_line()
        doc = self._current_document()
        if action_button("Save", primary=True, enabled=doc is not None and doc.dirty):
            self.save(everything=False)
        imgui.same_line()
        if action_button("Save all", enabled=self.has_unsaved_changes()):
            self.save(everything=True)

    def _feature_label(self, name: str) -> str:
        doc = self._documents.get(name)
        if doc is None or not doc.edit_count:
            return name
        return f"{name}  ({'* ' if doc.dirty else ''}{doc.edit_count})"

    def _draw_feature_combo(self) -> None:
        imgui.text("Feature")
        imgui.same_line()
        imgui.set_next_item_width(scaled(260))
        if imgui.begin_combo("##catalog_feature", self._feature_label(self._feature), imgui.ComboFlags_.height_large):
            for feature in self._session.features():
                name = _feature_name(feature)
                if imgui.selectable(f"{self._feature_label(name)}##feature_{name}", name == self._feature)[0] \
                        and name != self._feature:
                    self._select_feature(name)
            imgui.end_combo()

    def _draw_masters_popup(self) -> None:
        if not imgui.begin_popup(_MASTERS_POPUP):
            return
        for master in self._session.masters:
            if master.path:
                imgui.text(master.name)
                imgui.same_line()
                imgui.text_disabled(str(master.path))
            else:
                imgui.text_colored(semantic_color("error"), f"{master.name}  (not found)")
        imgui.separator()
        if imgui.button("Add folder..."):
            self.add_master_folder(pick_folder("Folder with master plugins") or "")
        if hasattr(self.forms, "load_more"):
            imgui.same_line()
            if imgui.button("Load plugin for picking..."):
                self.load_plugin_for_picking(pick_file("Load another plugin for picking records", _PLUGINS,
                                                       default_path=self._data_dir()) or "")
            imgui.set_item_tooltip("Add any other plugin (SeventySix.esm, another mod) so its records can be picked.")
        imgui.end_popup()

    def _draw_sections(self, doc) -> None:
        sections = list(doc.sections())
        if self._section not in (*sections, SETTINGS_TAB):
            self._section = sections[0] if sections else SETTINGS_TAB
        if imgui.begin_tab_bar("##catalog_sections"):
            for section in (*sections, SETTINGS_TAB):
                flags = imgui.TabItemFlags_.set_selected if section == self._section and self._force_tab else 0
                if imgui.begin_tab_item(f"{section}##tab", None, flags)[0]:
                    if section != self._section:
                        self._select_section(section)
                    imgui.end_tab_item()
            imgui.end_tab_bar()
        self._force_tab = False
        description = self._section_description(doc, self._section)
        if description:
            imgui.text_wrapped(description)

    @staticmethod
    def _about_paragraphs(doc) -> list[str]:
        schema = doc.schema if isinstance(doc.schema, dict) else {}
        help_text = str(schema.get("x-help", "")).split("\n\n")
        return [text.strip() for text in (schema.get("description"), *help_text) if text and text.strip()]

    def _draw_about_button(self) -> None:
        doc = self._current_document()
        paragraphs = self._about_paragraphs(doc) if doc is not None else []
        if not paragraphs:
            return
        imgui.same_line()
        title = f"About {self._feature}"
        if self._about.is_open:
            self._about.open(self._draw_about_text, title)  # follows the feature combo while open
        self._about.toggle_button("About", self._draw_about_text, title)
        imgui.set_item_tooltip("What this feature does in game and how its entries work.")

    def _draw_about_text(self) -> None:
        doc = self._current_document()
        for text in self._about_paragraphs(doc) if doc is not None else []:
            imgui.text_wrapped(text)
            imgui.spacing()

    @staticmethod
    def _section_description(doc, section: str) -> str:
        schema = doc.schema if isinstance(doc.schema, dict) else {}
        key = "settings" if section == SETTINGS_TAB else section
        node = schema.get("properties", {}).get(key, {})
        return node.get("description", "") if isinstance(node, dict) else ""

    def _draw_toolbar(self, doc, section: str) -> None:
        if doc.section_kind(section) == "map":
            imgui.set_next_item_width(scaled(180))
            _, self._new_key = imgui.input_text_with_hint("##catalog_new_key", "name of the new entry", self._new_key)
            imgui.same_line()
        if imgui.button("New entry"):
            if self.new_entry(section, self._new_key) is not None:
                self._new_key = ""
        extension = extension_for(doc.name)
        for action in extension.section_actions(self, doc, section) if extension is not None else ():
            imgui.same_line()
            if imgui.button(action.label):
                action.run()
            if action.tooltip:
                imgui.set_item_tooltip(action.tooltip)

    def _render_context(self, doc, read_only: bool):
        from ui.tools.catalog_editor import render

        return render.RenderContext(doc.schema, self._session.forms, self._picker, doc.section_keys,
                                    read_only=read_only, assets=self._assets)

    def _draw_settings(self, doc) -> None:
        from ui.tools.catalog_editor import render

        properties = (doc.settings_schema() or {}).get("properties", {})
        ctx = self._render_context(doc, read_only=False)
        entries = list(doc.settings())
        if imgui.button("View JSON##settings"):
            self._json.show(f"{doc.name} settings", {name: value for name, value, _, _ in entries})
        for name, value, source, by_author in entries:
            imgui.push_id(name)
            if not by_author:
                imgui.push_style_color(imgui.Col_.text, semantic_color("muted"))
            node = properties.get(name, {})
            changed, new = render.draw_value(node.get("title") or render.humanize(name), node, copy.deepcopy(value),
                                             ctx, ("setting", name))
            if not by_author:
                imgui.pop_style_color()
            if changed:
                doc.set_setting(name, new)
            imgui.same_line()
            imgui.text_disabled(f"from {source}" if not by_author else "yours")
            if by_author:
                imgui.same_line()
                if imgui.small_button("Revert"):
                    doc.revert_setting(name)
            imgui.pop_id()

    def _draw_problems(self) -> None:
        imgui.text_colored(semantic_color("error"), f"{len(self._problems)} problem(s)")
        imgui.same_line()
        if imgui.small_button("Clear"):
            self._problems = []
            return
        for index, problem in enumerate(self._problems):
            where = " / ".join(str(part) for part in (problem.feature, problem.section, problem.key) if part)
            warning = "" if getattr(problem, "blocking", True) else "(warning) "
            if imgui.selectable(f"{warning}{where}: {problem.message}##problem{index}", False)[0]:
                self.goto_problem(problem)

    def _draw_guard(self) -> None:
        if self._open_guard:
            imgui.open_popup(_GUARD_POPUP)
            self._open_guard = False
        opened, _ = imgui.begin_popup_modal(_GUARD_POPUP, None, imgui.WindowFlags_.always_auto_resize)
        if not opened:
            return
        imgui.text("You have unsaved catalog changes.")
        action = self._guarded_action
        if imgui.button("Save all and continue"):
            if self.save(everything=True) or not self.has_unsaved_changes():
                self._finish_guard(action)
        imgui.same_line()
        if imgui.button("Discard and continue"):
            self._finish_guard(action)
        imgui.same_line()
        if imgui.button("Cancel"):
            self._finish_guard(None)
        imgui.end_popup()

    def _finish_guard(self, action: Callable[[], None] | None) -> None:
        self._guarded_action = None
        imgui.close_current_popup()
        if action is not None:
            action()
