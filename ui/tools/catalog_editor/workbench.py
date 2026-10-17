"""The entry workbench every feature page uses: entry browser, header with layer strip, problems banner,
tabbed result table (what it does · my changes · master <-> mine) and a tabbed inspector.

The functions at the top are pure (filters, entry names, the per-entry diff and the layer strip); the
`Workbench` class draws them. Features plug in through the hooks on `features.FeatureExtension`.
"""

from __future__ import annotations

import copy
import json
import logging
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from typing import Any

from creation_lib.catalog.plugin_catalog import parse_form_key
from ui.tools.catalog_editor.features import (
    Badge,
    EntryProblem,
    EntryView,
    FeatureExtension,
    InspectorTab,
    RemoveWording,
)
from ui.tools.catalog_editor.model import ADDED, INHERITED, KEY_SEPARATOR, OVERRIDDEN, REMOVED

_log = logging.getLogger("tools.catalog_editor")

MINE, CHANGED, PROBLEMS, ALL = "Mine", "Changed", "Problems", "All"
FILTERS = (MINE, CHANGED, PROBLEMS, ALL)
FILTER_HELP = {
    MINE: "Entries your plugin adds, changes or removes.",
    CHANGED: "Entries from a master that your plugin changes or removes.",
    PROBLEMS: "Entries with a problem or a warning.",
    ALL: "Every entry: your plugin's merged over its masters'.",
}
STATUS = {
    INHERITED: ("from a master", "muted"),
    OVERRIDDEN: ("you changed", "warning"),
    ADDED: ("you added", "success"),
    REMOVED: ("you removed", "error"),
}
NARROW_WIDTH = 1400
BACKGROUND_CHECK_ENTRIES = 500  # a layer this big is checked off the UI thread
IDENTITY_FIELDS = ("form", "target", "spell", "source", "id", "code", "name")


# --- browser (pure) ---------------------------------------------------------------------------------


def row_in_filter(row, mode: str, problem_keys: set[str]) -> bool:
    if mode == MINE:
        return row.status != INHERITED
    if mode == CHANGED:
        return row.status in (OVERRIDDEN, REMOVED)
    if mode == PROBLEMS:
        return row.key in problem_keys
    return True


def filter_rows(rows: Sequence, mode: str, needle: str, name_of: Callable[[Any], str],
                problem_keys: set[str]) -> list:
    needle = needle.strip().casefold()
    out = []
    for row in rows:
        if not row_in_filter(row, mode, problem_keys):
            continue
        if needle and needle not in str(row.key).casefold() and needle not in name_of(row).casefold():
            continue
        out.append(row)
    return out


def default_entry_name(row, describe: Callable[[str], str | None] | None) -> str:
    """The entry's own editor_id/name, else the EditorID of a FormKey key, else the key."""
    entry = row.entry if isinstance(row.entry, dict) else {}
    for name in ("editor_id", "name"):
        if isinstance(entry.get(name), str) and entry[name].strip():
            return entry[name]
    first = str(row.key).split(KEY_SEPARATOR, 1)[0]
    if describe is not None and parse_form_key(first) is not None:
        described = describe(first)
        if described:
            return described.split(" [", 1)[0]
    return str(row.key)


# --- diff (pure) -------------------------------------------------------------------------------------


class _Missing:
    def __repr__(self) -> str:
        return "MISSING"

    def __bool__(self) -> bool:
        return False


MISSING: Any = _Missing()
SAME, CHANGE, ADD, REMOVE = "same", "changed", "added", "removed"


@dataclass(frozen=True)
class DiffLine:
    field: str  # top-level field; "" when the entry itself is not an object
    before: Any  # the master's value, MISSING when absent
    after: Any  # the author's value, MISSING when absent
    kind: str
    master_index: int | None = None  # set for items of a list field
    mine_index: int | None = None

    @property
    def is_item(self) -> bool:
        return self.master_index is not None or self.mine_index is not None


def _canon_text(text: str) -> str:
    parsed = parse_form_key(text)
    return f"{parsed[0]}:{parsed[1]:X}" if parsed else text.casefold()


def item_identity(item: Any) -> str | None:
    """What makes two list items "the same item": their form (or similar key), else nothing."""
    if isinstance(item, dict):
        for name in IDENTITY_FIELDS:
            value = item.get(name)
            if isinstance(value, str) and value:
                return f"{name}={_canon_text(value)}"
        return None
    if isinstance(item, list) and item and isinstance(item[0], str):
        return _canon_text(item[0].replace("/", "\\"))
    if isinstance(item, str):
        return _canon_text(item)
    return None


def match_items(master: Sequence, mine: Sequence) -> list[tuple[int | None, int | None]]:
    """Pairs (master index, mine index): first by form, then by position; unpaired sides are None."""
    pairs: dict[int, int] = {}
    used: set[int] = set()
    identities = [item_identity(item) for item in mine]
    for i, item in enumerate(master):
        identity = item_identity(item)
        if identity is None:
            continue
        j = next((j for j, other in enumerate(identities) if other == identity and j not in used), None)
        if j is not None:
            pairs[i], _ = j, used.add(j)
    for i in range(len(master)):
        # Items without a form (curve points, plain values) pair by position; a different form is a different item.
        if i not in pairs and i < len(mine) and i not in used and identities[i] is None and item_identity(master[i]) is None:
            pairs[i], _ = i, used.add(i)
    out: list[tuple[int | None, int | None]] = [(i, pairs.get(i)) for i in range(len(master))]
    out += [(None, j) for j in range(len(mine)) if j not in used]
    return sorted(out, key=lambda pair: pair[1] if pair[1] is not None else pair[0] - 0.5)


def _list_lines(name: str, before: list, after: list) -> list[DiffLine]:
    lines = []
    for i, j in match_items(before, after):
        old = before[i] if i is not None else MISSING
        new = after[j] if j is not None else MISSING
        kind = ADD if i is None else REMOVE if j is None else SAME if old == new else CHANGE
        lines.append(DiffLine(name, old, new, kind, i, j))
    return lines


def _line(name: str, before: Any, after: Any) -> DiffLine:
    kind = SAME if before == after else ADD if before is MISSING else REMOVE if after is MISSING else CHANGE
    return DiffLine(name, before, after, kind)


def diff_entry(master: Any, mine: Any) -> list[DiffLine]:
    """Field by field (and item by item for list fields) differences of the author's entry from the master's."""
    if isinstance(master, dict) and isinstance(mine, dict):
        lines: list[DiffLine] = []
        for name in dict.fromkeys([*master, *mine]):
            if name == "remove":
                continue
            before, after = master.get(name, MISSING), mine.get(name, MISSING)
            if isinstance(before, list) and isinstance(after, list):
                lines += _list_lines(name, before, after)
            else:
                lines.append(_line(name, before, after))
        return lines
    if isinstance(master, list) and isinstance(mine, list):
        return _list_lines("", master, mine)
    return [_line("", MISSING if master is None else master, MISSING if mine is None else mine)]


def revert_line(mine: Any, master: Any, line: DiffLine) -> Any:
    """`mine` with `line` put back the way the master has it."""
    if not line.field and not line.is_item:
        return copy.deepcopy(master)
    out = copy.deepcopy(mine)
    if not line.is_item:
        if line.before is MISSING:
            out.pop(line.field, None)
        else:
            out[line.field] = copy.deepcopy(line.before)
        return out
    items = out if not line.field else out.setdefault(line.field, [])
    if line.kind == ADD:
        del items[line.mine_index]
    elif line.kind == REMOVE:
        items.insert(min(line.master_index, len(items)), copy.deepcopy(line.before))
    elif line.kind == CHANGE:
        items[line.mine_index] = copy.deepcopy(line.before)
    return out


def diff_counts(lines: Sequence[DiffLine]) -> tuple[int, int, int]:
    """(added, removed, changed) items and fields."""
    added = sum(line.kind == ADD for line in lines)
    removed = sum(line.kind == REMOVE for line in lines)
    return added, removed, sum(line.kind == CHANGE for line in lines)


def main_list_of(entry: Any, preferred: tuple[str, str] | None) -> tuple[str, str] | None:
    if preferred is not None:
        return preferred
    if isinstance(entry, dict):
        for name, value in entry.items():
            if isinstance(value, list) and value and isinstance(value[0], (dict, list)):
                return name, name.replace("_", " ")
    return None


def _count(entry: Any, main: tuple[str, str] | None) -> str:
    if main is None or not isinstance(entry, dict):
        return ""
    return f" · {len(entry.get(main[0]) or [])} {main[1]}"


def layer_strip(row, plugin: str, master: str, main: tuple[str, str] | None, removed_note: str = "") -> str:
    """"SeventySix.esm · 5 items -> B21_X.esp · +1 -1 ~1 -> Result: 5 items", as plain text."""
    if row.status == ADDED:
        return f"{plugin} adds this -> Result{_count(row.entry, main)}"
    base = f"{master}{_count(row.inherited, main)}"
    if row.status == INHERITED:
        return f"{base} -> Result (unchanged)"
    if row.status == REMOVED:
        return f"{base} -> {plugin} removes it -> {removed_note or 'not in the catalog'}"
    added, removed, changed = diff_counts(diff_entry(row.inherited, row.entry))
    return f"{base} -> {plugin} · +{added} -{removed} ~{changed} -> Result{_count(row.entry, main)}"


def copy_notice_text(master: str, noun: str = "entry", count: str = "") -> str:
    return (f"This {noun} belongs to {master}. To change it, your plugin saves its own copy of the whole {noun}"
            f"{count}. From now on, changes {master} makes to this {noun} won't reach you.")


def form_keys_in(value: Any, depth: int = 0) -> list[str]:
    if isinstance(value, str):
        return [value] if parse_form_key(value) else []
    if depth > 6:
        return []
    if isinstance(value, dict):
        return [key for child in value.values() for key in form_keys_in(child, depth + 1)]
    if isinstance(value, list):
        return [key for child in value for key in form_keys_in(child, depth + 1)]
    return []


# --- value text (pure) -------------------------------------------------------------------------------


def value_text(value: Any, name_of: Callable[[str], str] = lambda key: key, limit: int = 90) -> str:
    """A short, readable rendering of any catalog value."""
    if value is MISSING or value is None:
        text = "(none)"
    elif isinstance(value, bool):
        text = "yes" if value else "no"
    elif isinstance(value, (int, float)):
        text = f"{value:g}"
    elif isinstance(value, str):
        text = name_of(value) if parse_form_key(value) else (value or '""')
    elif isinstance(value, dict):
        text = _dict_text(value, name_of)
    elif isinstance(value, list):
        if value and all(isinstance(v, (str, int, float)) and not isinstance(v, bool) for v in value) and len(value) <= 4:
            text = ", ".join(value_text(v, name_of) for v in value)
        else:
            text = f"{len(value)} item{'s' if len(value) != 1 else ''}"
    else:
        text = str(value)
    return text if len(text) <= limit else text[: limit - 3] + "..."


def _dict_text(value: dict, name_of: Callable[[str], str]) -> str:
    if "value" in value and set(value) <= {"value", "global", "curve", "unresolved"}:
        extra = " (from a global)" if value.get("global") else " (from a curve)" if value.get("curve") else ""
        return value_text(value.get("value"), name_of) + extra
    parts = []
    identity = next((name for name in IDENTITY_FIELDS if isinstance(value.get(name), str) and value[name]), None)
    if identity:
        parts.append(value_text(value[identity], name_of))
    for name, child in value.items():
        if name == identity or len(parts) >= 4:
            continue
        if isinstance(child, (dict, list)) and not (isinstance(child, dict) and "value" in child):
            if isinstance(child, list) and child:
                parts.append(f"{len(child)} {name.replace('_', ' ')}")
            continue
        parts.append(f"{name.replace('_', ' ')} {value_text(child, name_of, 30)}")
    return " · ".join(parts) or "{}"


def humanize(name: str) -> str:
    text = name.replace("_", " ").strip()
    return text[:1].upper() + text[1:] if text else "Value"


# --- the workbench (imgui) ---------------------------------------------------------------------------


@dataclass
class PendingCopy:
    feature: str
    section: str
    key: str
    entry: Any
    notice: str


CONFIRM_POPUP = "Make your own copy?##catalog_copy_confirm"


def _revision(doc) -> Any:
    return getattr(doc, "revision", None), getattr(doc, "edit_count", 0)


class Workbench:
    def __init__(self, tool) -> None:
        self.tool = tool
        self.mode = ALL
        self.search = ""
        self.items: dict[tuple[str, str, str], int | None] = {}
        self.pending: PendingCopy | None = None
        self._open_confirm = False
        self._focus_tab: str | None = None
        self._rows_cache: dict[tuple, list] = {}
        self._problem_cache: tuple[Any, list] | None = None
        self._pending: tuple[Any, Any] | None = None
        self._names: dict[str, str] = {}
        self._entry_names: dict = {}  # row key -> name, for the page stored under None
        self._visible: list = []
        self._prefetched: set[tuple] = set()

    # -- data --

    def extension(self, doc) -> FeatureExtension | None:
        from ui.tools.catalog_editor.tool import extension_for

        return extension_for(doc.name)

    def rows(self, doc, section: str) -> list:
        key = (id(doc), section)
        cached = self._rows_cache.get(key)
        if cached is None or cached[0] != _revision(doc):
            cached = (_revision(doc), doc.rows(section))
            self._rows_cache[key] = cached
        return cached[1]

    def selected_row(self, doc, section: str):
        key = self.tool._selected_key
        if key is None:
            return None
        return next((row for row in self.rows(doc, section) if row.key == key), None)

    def _check(self, doc) -> tuple[list, list]:
        own, extra = [], []
        try:
            own = list(doc.validate(self.tool.forms))
            extension = self.extension(doc)
            extra = list(extension.problems(doc, self.tool.forms)) if extension is not None else []
        except Exception:
            _log.exception("checking %s failed", doc.name)
        return own, extra

    def _checked(self, doc) -> tuple[list, list]:
        """(the document's own problems, its extension's), recomputed only after an edit."""
        key = (id(doc), _revision(doc))
        if self._problem_cache is not None and self._problem_cache[0] == key:
            return self._problem_cache[1]
        pool = getattr(self.tool, "_pool", None)
        if pool is None or getattr(doc, "edit_count", 0) <= BACKGROUND_CHECK_ENTRIES:
            self._problem_cache = (key, self._check(doc))
            return self._problem_cache[1]
        # Checking SeventySix.esm's own catalogs takes tens of seconds; keep drawing meanwhile.
        if self._pending is None or self._pending[0] != key:
            if self._pending is not None:
                self._pending[1].cancel()
            self._pending = (key, pool.submit(self._check, doc))
        elif self._pending[1].done():
            self._problem_cache, self._pending = (key, self._pending[1].result()), None
            return self._problem_cache[1]
        if self._problem_cache is not None and self._problem_cache[0][0] == id(doc):
            return self._problem_cache[1]
        return [], []

    @property
    def checking(self) -> bool:
        return self._pending is not None

    def problems(self, doc) -> list:
        """The open feature's live problems and warnings."""
        own, extra = self._checked(doc)
        return own + extra

    def problem_keys(self, doc, section: str) -> set[str]:
        return {problem.key for problem in self.problems(doc) if problem.section == section and problem.key}

    def describe(self, key: str) -> str | None:
        forms = self.tool.forms
        return forms.describe(key) if hasattr(forms, "describe") else None

    def name_of_key(self, key: str) -> str:
        if key not in self._names:
            described = self.describe(key)
            self._names[key] = described.split(" [", 1)[0] if described else key
        return self._names[key]

    def entry_name(self, doc, section: str, row) -> str:
        page = (doc.name, section, _revision(doc))
        if self._entry_names.get(None) != page:
            self._entry_names = {None: page}
        key = row.key
        if key not in self._entry_names:
            extension = self.extension(doc)
            name = extension.entry_name(self.tool, doc, section, row) if extension is not None else None
            self._entry_names[key] = name or default_entry_name(row, self.describe)
        return self._entry_names[key]

    def _prefetch(self, row) -> None:
        """Name every FormKey of the entry in one query per plugin, instead of one slow query per key."""
        prefetch = getattr(self.tool.forms, "prefetch", None)
        marker = (id(self.tool.forms), row.key, id(row.entry))
        if prefetch is None or marker in self._prefetched:
            return
        self._prefetched.add(marker)
        keys = [str(row.key).split(KEY_SEPARATOR, 1)[0], *form_keys_in(row.entry), *form_keys_in(row.inherited)]
        try:
            prefetch(keys)
        except Exception:
            _log.exception("looking up the forms of %s failed", row.key)

    def view(self, doc, section: str, row) -> EntryView:
        key = (doc.name, section, row.key)
        self._prefetch(row)
        read_only = row.status == REMOVED
        master = getattr(row, "origin", "") or (row.source if row.status != ADDED else "")

        def select(index: int | None) -> None:
            self.items[key] = index

        def focus(title: str) -> None:
            self._focus_tab = title

        return EntryView(self.tool, doc, section, row, self.tool._render_context(doc, read_only=read_only), read_only,
                         master, self.items.get(key), select, focus, self._names)

    def main_list(self, view: EntryView) -> tuple[str, str] | None:
        extension = self.extension(view.doc)
        preferred = extension.main_list(view.doc, view.section) if extension is not None else None
        return main_list_of(view.row.inherited if view.row.status == REMOVED else view.entry, preferred)

    def remove_wording(self, doc, section: str) -> RemoveWording:
        extension = self.extension(doc)
        wording = extension.remove_wording(doc, section) if extension is not None else None
        return wording or RemoveWording(
            "Remove from the catalog",
            "Your plugin marks this entry removed: Tales then behaves as if no catalog listed it.",
            "Removed: Tales behaves as if no catalog listed it.")

    def entry_problems(self, view: EntryView) -> list[EntryProblem]:
        own, extra = self._checked(view.doc)
        extension = self.extension(view.doc)
        found = extension.entry_problems(view) if extension is not None else None
        mine = [problem for problem in (own if found is not None else own + extra)
                if problem.section == view.section and problem.key == view.row.key]
        return [EntryProblem(problem.message, blocking=getattr(problem, "blocking", True)) for problem in mine] + (found or [])

    # -- editing --

    def request_edit(self, view: EntryView, new_entry: Any) -> None:
        """Apply an edit; the first change to an inherited entry asks first, because it copies the whole entry."""
        if view.read_only:
            return
        if view.row.status == INHERITED:
            extension = self.extension(view.doc)
            notice = extension.copy_notice(view, new_entry) if extension is not None else None
            self.pending = PendingCopy(view.doc.name, view.section, view.row.key, copy.deepcopy(new_entry),
                                       notice or copy_notice_text(view.master or "a master"))
            self._open_confirm = True
            return
        self.tool.edit_entry(view.section, view.row, new_entry)

    def confirm_copy(self) -> None:
        pending, self.pending = self.pending, None
        doc = self.tool._current_document()
        if pending is None or doc is None or doc.name != pending.feature:
            return
        row = next((r for r in doc.rows(pending.section) if r.key == pending.key), None)
        if row is not None and row.status != REMOVED:
            self.tool.edit_entry(pending.section, row, pending.entry)
            self.tool._selected_key = row.key

    def cancel_copy(self) -> None:
        self.pending = None

    def undo_line(self, view: EntryView, line: DiffLine) -> None:
        reverted = revert_line(view.entry, view.row.inherited, line)
        if reverted == view.row.inherited:
            self.tool.revert_row(view.section, view.row)
        else:
            self.tool.edit_entry(view.section, view.row, reverted)

    # -- browser --

    def visible_rows(self, doc, section: str) -> list:
        problem_keys = self.problem_keys(doc, section) if self.mode == PROBLEMS else set()
        return filter_rows(self.rows(doc, section), self.mode, self.search,
                           lambda row: self.entry_name(doc, section, row), problem_keys)

    def draw_browser(self, doc, section: str, height: float = 0) -> None:
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import semantic_color

        for index, mode in enumerate(FILTERS):
            if index:
                imgui.same_line()
            active = mode == self.mode
            if active:
                imgui.push_style_color(imgui.Col_.button, imgui.get_style_color_vec4(imgui.Col_.button_active))
            if imgui.small_button(f"{mode}##filter"):
                self.mode = mode
            if active:
                imgui.pop_style_color()
            imgui.set_item_tooltip(FILTER_HELP[mode])
        imgui.set_next_item_width(-1)
        _, self.search = imgui.input_text_with_hint("##browser_search", "Search names and keys", self.search)
        rows = self.rows(doc, section)
        visible = self._visible = self.visible_rows(doc, section)
        imgui.text_disabled(f"{len(visible):,} of {len(rows):,}")
        problem_keys = self.problem_keys(doc, section)
        legend = imgui.get_text_line_height_with_spacing() * 3
        if imgui.begin_child("##browser_rows", imgui.ImVec2(0, height or -legend)):
            self._draw_browser_rows(doc, section, visible, problem_keys)
        imgui.end_child()
        if not height:
            for status in (INHERITED, OVERRIDDEN, ADDED, REMOVED):
                self._dot(semantic_color(STATUS[status][1]))
                imgui.text_disabled(STATUS[status][0])
                if status in (INHERITED, ADDED):
                    imgui.same_line()

    def _dot(self, color) -> None:
        from imgui_bundle import imgui

        pos = imgui.get_cursor_screen_pos()
        size = imgui.get_text_line_height()
        imgui.get_window_draw_list().add_circle_filled(imgui.ImVec2(pos.x + size * 0.4, pos.y + size * 0.5), size * 0.28,
                                                       imgui.get_color_u32(color))
        imgui.dummy(imgui.ImVec2(size * 0.9, size))
        imgui.same_line()

    def _draw_browser_rows(self, doc, section: str, visible: list, problem_keys: set[str]) -> None:
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import semantic_color

        extension = self.extension(doc)
        line = imgui.get_text_line_height()
        draw = imgui.get_window_draw_list()
        clipper = imgui.ListClipper()
        clipper.begin(len(visible))
        while clipper.step():
            for index in range(clipper.display_start, clipper.display_end):
                row = visible[index]
                imgui.push_id(str(row.key))
                pos = imgui.get_cursor_screen_pos()
                if imgui.selectable("##entry", self.tool._selected_key == row.key, 0, imgui.ImVec2(0, line))[0]:
                    self.tool.select_row(row.key)
                imgui.set_item_tooltip(f"{row.key}\n{STATUS.get(row.status, (row.status,))[0]}")
                color = imgui.get_color_u32(semantic_color(STATUS.get(row.status, ("", "muted"))[1]))
                draw.add_circle_filled(imgui.ImVec2(pos.x + line * 0.4, pos.y + line * 0.5), line * 0.28, color)
                x = pos.x + line
                name = self.entry_name(doc, section, row)
                draw.add_text(imgui.ImVec2(x, pos.y), imgui.get_color_u32(imgui.Col_.text), name)
                x += imgui.calc_text_size(name).x + line * 0.6
                badges = extension.browser_badges(self.tool, doc, section, row) if extension is not None else []
                if row.key in problem_keys:
                    badges = [*badges, Badge("problem", "warning")]
                for badge in badges:
                    draw.add_text(imgui.ImVec2(x, pos.y), imgui.get_color_u32(semantic_color(badge.role)), badge.text)
                    x += imgui.calc_text_size(badge.text).x + line * 0.6
                if name != row.key:
                    draw.add_text(imgui.ImVec2(x, pos.y), imgui.get_color_u32(imgui.Col_.text_disabled), str(row.key))
                imgui.pop_id()

    def draw_browser_combo(self, doc, section: str) -> None:
        """The browser folded into a combo, for windows too narrow for three panes."""
        from imgui_bundle import imgui

        row = self.selected_row(doc, section)
        preview = self.entry_name(doc, section, row) if row is not None else "Pick an entry..."
        imgui.set_next_item_width(-1)
        if imgui.begin_combo("##entry_combo", preview, imgui.ComboFlags_.height_largest):
            self.draw_browser(doc, section, height=imgui.get_text_line_height_with_spacing() * 18)
            imgui.end_combo()

    # -- centre --

    def draw_centre(self, doc, section: str) -> None:
        from imgui_bundle import imgui

        row = self.selected_row(doc, section)
        if row is None:
            imgui.text_disabled("Pick an entry in the list to see what it does.")
            return
        view = self.view(doc, section, row)
        extension = self.extension(doc)
        self._draw_header(view)
        if extension is not None and extension.has_situation(doc, section):
            if imgui.begin_child("##situation", imgui.ImVec2(0, 0), imgui.ChildFlags_.auto_resize_y | imgui.ChildFlags_.borders):
                extension.draw_situation(view)
            imgui.end_child()
        self._draw_banner(view)
        result = extension.result_view(doc, section) if extension is not None else None
        if imgui.begin_tab_bar("##entry_tabs"):
            if imgui.begin_tab_item(f"{result.title if result else 'What it holds'}###result")[0]:
                if imgui.begin_child("##result_body"):
                    changed, new = result.draw(view) if result else self._draw_fields(view)
                    if changed:
                        self.request_edit(view, new)
                imgui.end_child()
                imgui.end_tab_item()
            if imgui.begin_tab_item("My changes###changes")[0]:
                if imgui.begin_child("##changes_body"):
                    self._draw_changes(view)
                imgui.end_child()
                imgui.end_tab_item()
            if view.master and imgui.begin_tab_item(f"{view.master} <-> Mine###diff")[0]:
                if imgui.begin_child("##diff_body"):
                    self._draw_diff(view)
                imgui.end_child()
                imgui.end_tab_item()
            imgui.end_tab_bar()

    def _draw_header(self, view: EntryView) -> None:
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import heading, semantic_color

        doc, row = view.doc, view.row
        extension = self.extension(doc)
        heading(self.entry_name(doc, view.section, row))
        imgui.text_disabled(str(row.key))
        imgui.same_line()
        badge = extension.status_badge(view) if extension is not None else None
        if badge is None:
            text, role = STATUS.get(row.status, (row.status, "muted"))
            badge = Badge(f"{text} {view.master}" if row.status == INHERITED else text, role)
        imgui.text_colored(semantic_color(badge.role), badge.text)
        if badge.tooltip:
            imgui.set_item_tooltip(badge.tooltip)
        wording = self.remove_wording(doc, view.section)
        plugin = getattr(self.tool._session, "plugin", "your plugin")
        plugin = plugin if isinstance(plugin, str) else getattr(plugin, "name", "your plugin")
        imgui.text_wrapped("Layers: " + layer_strip(row, plugin, view.master, self.main_list(view), wording.removed_note))
        if row.status == OVERRIDDEN:
            imgui.text_colored(semantic_color("warning"),
                               f"Your plugin owns a copy of this {view.master} entry. Later changes {view.master} "
                               "makes to it won't reach you.")
        self._draw_entry_actions(view, wording)

    def _draw_entry_actions(self, view: EntryView, wording: RemoveWording) -> None:
        from imgui_bundle import imgui

        row, section = view.row, view.section
        if imgui.small_button("View JSON##entry"):
            self.tool._json.show(f"{view.doc.name} / {section} / {row.key} ({row.status})", row.entry)
        if row.status == INHERITED:
            imgui.same_line()
            if imgui.small_button("Change it in my plugin..."):
                self.request_edit(view, copy.deepcopy(row.entry))
            imgui.set_item_tooltip("Copy this entry into your plugin so you can change it.")
        if row.status in (INHERITED, OVERRIDDEN):
            imgui.same_line()
            if imgui.small_button(f"{wording.label}##remove_entry"):
                self.tool.remove_row(section, row)
            imgui.set_item_tooltip(wording.tooltip)
        if row.status == OVERRIDDEN:
            imgui.same_line()
            if imgui.small_button(f"Revert to {view.master}'s##revert_entry"):
                self.tool.revert_row(section, row)
            imgui.set_item_tooltip("Drop your copy; the master's entry applies again, and its later changes reach you.")
        if row.status == REMOVED:
            imgui.same_line()
            if imgui.small_button("Undo: use the master's entry again##restore_entry"):
                self.tool.revert_row(section, row)
        if row.status == ADDED:
            imgui.same_line()
            if imgui.small_button("Delete my entry##delete_entry"):
                self.tool.remove_row(section, row)

    def _draw_banner(self, view: EntryView) -> None:
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import semantic_color

        problems = self.entry_problems(view)
        if not problems:
            return
        severe = any(problem.whole or problem.blocking for problem in problems)
        color = semantic_color("error" if severe else "warning")
        imgui.push_style_color(imgui.Col_.border, color)
        if imgui.begin_child("##banner", imgui.ImVec2(0, 0), imgui.ChildFlags_.auto_resize_y | imgui.ChildFlags_.borders):
            for index, problem in enumerate(problems):
                imgui.push_id(index)
                imgui.push_style_color(imgui.Col_.text, color)
                imgui.text_wrapped(problem.message)
                imgui.pop_style_color()
                if problem.item is not None and imgui.small_button("Show it"):
                    view.select_item(problem.item)
                    view.focus_tab("Checks" if problem.check is not None else "In game")
                imgui.pop_id()
        imgui.end_child()
        imgui.pop_style_color()

    def _draw_fields(self, view: EntryView) -> tuple[bool, Any]:
        """The generic "what it holds" view: every field in plain text, and which layer set it."""
        from imgui_bundle import imgui

        entry, master = view.entry, view.row.inherited
        plugin = view.doc.plugin if isinstance(getattr(view.doc, "plugin", None), str) else "your plugin"
        fields = list(entry.items()) if isinstance(entry, dict) else [("", entry)]
        flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h | imgui.TableFlags_.resizable
        if imgui.begin_table("##fields", 3, flags):
            imgui.table_setup_column("Field", imgui.TableColumnFlags_.width_stretch, 1.0)
            imgui.table_setup_column("Value", imgui.TableColumnFlags_.width_stretch, 3.0)
            imgui.table_setup_column("Set by", imgui.TableColumnFlags_.width_stretch, 1.0)
            imgui.table_headers_row()
            for name, value in fields:
                if name == "remove":
                    continue
                imgui.table_next_row()
                imgui.table_next_column()
                imgui.text(humanize(name) if name else "Value")
                imgui.table_next_column()
                imgui.text_wrapped(value_text(value, view.name_of, 400))
                imgui.table_next_column()
                before = master.get(name, MISSING) if isinstance(master, dict) else master
                imgui.text_disabled(view.master if view.master and before == value and view.row.status != ADDED else plugin)
            imgui.end_table()
        return False, entry

    def item_text(self, view: EntryView, field_name: str, item: Any) -> str:
        if item is MISSING:
            return "-"
        extension = self.extension(view.doc)
        text = extension.describe_item(view.doc, view.section, field_name, item, view.name_of) if extension else None
        return text or value_text(item, view.name_of)

    def _line_label(self, line: DiffLine) -> str:
        name = humanize(line.field) if line.field else "Item"
        if line.is_item:
            number = (line.mine_index if line.mine_index is not None else line.master_index) + 1
            return f"{name} {number}"
        return name

    def _draw_changes(self, view: EntryView) -> None:
        from imgui_bundle import imgui

        row = view.row
        if row.status == INHERITED:
            imgui.text_wrapped(f"No changes: this is {view.master}'s entry as it is.")
            return
        if row.status == ADDED:
            imgui.text_wrapped("You added this entry; no master has it.")
            return
        if row.status == REMOVED:
            imgui.text_wrapped(self.remove_wording(view.doc, view.section).removed_note)
            if imgui.small_button("Undo##restore_removed"):
                self.tool.revert_row(view.section, row)
            return
        lines = [line for line in diff_entry(row.inherited, row.entry) if line.kind != SAME]
        if not lines:
            imgui.text_wrapped("Your copy is the same as the master's.")
            return
        self._diff_table(view, lines, ("What", f"Was ({view.master})", "Now (yours)"))

    def _draw_diff(self, view: EntryView) -> None:
        from imgui_bundle import imgui

        row = view.row
        if row.status == ADDED:
            imgui.text_wrapped("No master has this entry.")
            return
        mine = row.inherited if row.status == INHERITED else row.entry
        if row.status == REMOVED:
            imgui.text_wrapped(self.remove_wording(view.doc, view.section).removed_note)
            mine = MISSING
        lines = diff_entry(row.inherited, mine)
        plugin = view.doc.plugin if isinstance(getattr(view.doc, "plugin", None), str) else "yours"
        self._diff_table(view, lines, ("#", view.master, f"{plugin} (yours)"), editable=row.status == OVERRIDDEN)

    def _diff_table(self, view: EntryView, lines: list[DiffLine], titles: tuple[str, str, str],
                    editable: bool = True) -> None:
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import semantic_color

        roles = {SAME: "muted", CHANGE: "warning", ADD: "success", REMOVE: "error"}
        flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h | imgui.TableFlags_.resizable
        if not imgui.begin_table("##diff", 5, flags):
            return
        imgui.table_setup_column(titles[0], imgui.TableColumnFlags_.width_stretch, 1.0)
        imgui.table_setup_column(titles[1], imgui.TableColumnFlags_.width_stretch, 3.0)
        imgui.table_setup_column(titles[2], imgui.TableColumnFlags_.width_stretch, 3.0)
        imgui.table_setup_column("##state", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("removed__").x)
        imgui.table_setup_column("##undo", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("Undo___").x)
        imgui.table_headers_row()
        undo = None
        for index, line in enumerate(lines):
            imgui.push_id(index)
            imgui.table_next_row()
            imgui.table_next_column()
            if line.is_item and line.mine_index is not None:
                if imgui.selectable(self._line_label(line), view.item == line.mine_index,
                                    imgui.SelectableFlags_.span_all_columns | imgui.SelectableFlags_.allow_overlap)[0]:
                    view.select_item(line.mine_index)
            else:
                imgui.text(self._line_label(line))
            for value in (line.before, line.after):
                imgui.table_next_column()
                text = self.item_text(view, line.field, value) if line.is_item else value_text(value, view.name_of)
                if line.kind == SAME and value is line.after:
                    imgui.text_disabled("same")
                else:
                    imgui.text_wrapped(text)
            imgui.table_next_column()
            imgui.text_colored(semantic_color(roles[line.kind]), line.kind)
            imgui.table_next_column()
            if editable and line.kind != SAME and imgui.small_button("Undo"):
                undo = line
            imgui.pop_id()
        imgui.end_table()
        if undo is not None:
            self.undo_line(view, undo)

    # -- inspector --

    def draw_inspector(self, doc, section: str) -> None:
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import semantic_color

        row = self.selected_row(doc, section)
        if row is None:
            imgui.text_disabled("Select an entry to see its fields.")
            self._draw_popout_button()
            return
        view = self.view(doc, section, row)
        extension = self.extension(doc)
        main = self.main_list(view)
        items = (view.entry.get(main[0]) if main and isinstance(view.entry, dict) else None) or []
        if view.item is not None and view.item >= len(items):
            view.select_item(None)
            view.item = None
        title = self.entry_name(doc, section, row)
        if view.item is not None and main is not None:
            title = f"{humanize(main[1])} {view.item + 1} of {title}: {self.item_text(view, main[0], items[view.item])}"
        imgui.text_wrapped(title)
        self._draw_stepper(view, items)
        imgui.same_line()
        self._draw_popout_button()
        sentence = extension.sentence(view) if extension is not None else ""
        if sentence:
            imgui.push_style_color(imgui.Col_.text, semantic_color("accent"))
            imgui.text_wrapped(sentence)
            imgui.pop_style_color()
        if view.read_only:
            imgui.text_disabled("Removed by your plugin; undo the removal to edit it.")
        tabs = (extension.inspector_tabs(view) if extension is not None else []) or self._generic_tabs(view, extension)
        focus, self._focus_tab = self._focus_tab, None
        if imgui.begin_tab_bar("##inspector_tabs"):
            for tab in tabs:
                flags = imgui.TabItemFlags_.set_selected if focus and tab.title.startswith(focus) else 0
                if imgui.begin_tab_item(f"{tab.title}###{tab.title.split(' (', 1)[0]}", None, flags)[0]:
                    if imgui.begin_child("##inspector_body"):
                        changed, new = tab.draw(view)
                        if changed:
                            self.request_edit(view, new)
                    imgui.end_child()
                    imgui.end_tab_item()
            imgui.end_tab_bar()

    def _draw_stepper(self, view: EntryView, items: list) -> None:
        from imgui_bundle import imgui

        if view.item is not None:
            if imgui.small_button("< prev##item") and view.item > 0:
                view.select_item(view.item - 1)
            imgui.same_line()
            if imgui.small_button("next >##item") and view.item + 1 < len(items):
                view.select_item(view.item + 1)
            imgui.same_line()
            if imgui.small_button("Whole entry##item"):
                view.select_item(None)
            return
        keys = [row.key for row in self._visible]
        position = keys.index(view.row.key) if view.row.key in keys else -1
        if imgui.small_button("< prev##entry") and position > 0:
            self.tool.select_row(keys[position - 1])
        imgui.same_line()
        if imgui.small_button("next >##entry") and 0 <= position < len(keys) - 1:
            self.tool.select_row(keys[position + 1])

    def _draw_popout_button(self) -> None:
        from imgui_bundle import imgui

        if imgui.small_button("Dock back##inspector" if self.tool._popped_out else "Pop out##inspector"):
            self.tool._popped_out = not self.tool._popped_out

    def _generic_tabs(self, view: EntryView, extension: FeatureExtension | None) -> list[InspectorTab]:
        tabs = [InspectorTab("Fields", self._draw_form)]
        if extension is not None and type(extension).draw_detail is not FeatureExtension.draw_detail:
            tabs.append(InspectorTab("In game", lambda v: (extension.draw_detail(v.tool, v.doc, v.section, v.row),
                                                           (False, v.entry))[1]))
        return tabs

    def _draw_form(self, view: EntryView) -> tuple[bool, Any]:
        from ui.tools.catalog_editor import render

        extension = self.extension(view.doc)
        entry = view.row.inherited if view.row.status == REMOVED else view.entry
        if extension is not None and extension.custom_form(view.doc, view.section):
            return extension.draw_form(view.tool, view.doc, view.section, view.row, view.ctx, view.read_only)
        return render.draw_value("", view.doc.entry_schema(view.section), copy.deepcopy(entry), view.ctx,
                                 ("entry", view.section, str(view.row.key)))

    # -- status bar and popups --

    def draw_status_bar(self, doc, section: str) -> None:
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import semantic_color

        rows = self.rows(doc, section)
        mine = sum(row.status != INHERITED for row in rows)
        imgui.text_disabled(f"{section}: {len(rows):,} entries, {mine:,} changed by your plugin")
        problems = [problem for problem in self.problems(doc) if problem.section == section]
        imgui.same_line()
        if self.checking:
            imgui.text_disabled("· checking...")
            imgui.same_line()
        if not problems:
            imgui.text_colored(semantic_color("success"), "· no problems")
            return
        blocking = sum(getattr(problem, "blocking", True) for problem in problems)
        text = f"· {len(problems)} problem(s)" + (f", {blocking} blocking save" if blocking else ", all warnings")
        imgui.push_style_color(imgui.Col_.text, semantic_color("error" if blocking else "warning"))
        if imgui.small_button(f"{text}##status_problems"):
            self.mode = PROBLEMS
        imgui.pop_style_color()
        imgui.set_item_tooltip("Show only the entries with problems.")

    def draw_popups(self) -> None:
        from imgui_bundle import imgui

        from creation_lib.ui.widgets.modern import scaled

        if self._open_confirm:
            imgui.open_popup(CONFIRM_POPUP)
            self._open_confirm = False
        opened, _ = imgui.begin_popup_modal(CONFIRM_POPUP, None, imgui.WindowFlags_.always_auto_resize)
        if not opened:
            return
        imgui.push_text_wrap_pos(scaled(460))
        imgui.text_wrapped(self.pending.notice if self.pending else "")
        imgui.pop_text_wrap_pos()
        imgui.spacing()
        if imgui.button("Make my copy"):
            self.confirm_copy()
            imgui.close_current_popup()
        imgui.same_line()
        if imgui.button("Cancel"):
            self.cancel_copy()
            imgui.close_current_popup()
        imgui.end_popup()


def json_preview(value: Any) -> str:
    return json.dumps(value, indent=2, default=str)
